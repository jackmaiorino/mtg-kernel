"""HaleysPC check-only dispatch (version 1): one remote process that acquires the host reservation and starts the run.

Runs on HaleysPC over SSH. It checks the staged plan's pins (the runner, the reservation helper, this script and
the bundle) and calls host_reservation_v1.dispatch in this process: acquire, the busy refusal, WMI creation of the
supervisor running the runner (supervise --token T -- python -u haley_check_only_runner_v1.py ...), and the
handoff. Acquiring in the dispatching process keeps it the lock's owner until the supervisor adopts (CLAUDE #574).
It prints the dispatch result as JSON; --status prints the helper's status for a token instead.

Usage: python haley_check_only_dispatch_v1.py --plan PLAN --python PYTHON
       python haley_check_only_dispatch_v1.py --plan PLAN --python PYTHON --status TOKEN
"""
import argparse
import hashlib
import importlib.util
import json
import sys
from pathlib import Path, PureWindowsPath

ROOT = 'C:/mtg-line-a/check-only/'
# The broad native pattern: builds, tests, trainers and evaluators all make the host busy.
BUSY_PATTERN = r'^(cargo|rustc|public_.*|native_.*|expanded_deck_.*|learned_sideboard_v1|trainer|mtg_kernel.*)\.exe$'


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def require(ok, message):
    if not ok:
        raise SystemExit('check-only dispatch refused: ' + message)


def load_helper(path):
    spec = importlib.util.spec_from_file_location('host_reservation_v1', str(path))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def owner_command(plan, plan_path, python):
    return [str(PureWindowsPath(python)), '-u', str(PureWindowsPath(plan['files']['launcher']['path'])),
            '--manifest', str(PureWindowsPath(plan_path)), '--host', 'haleyspc',
            '--root', str(PureWindowsPath(plan['worker_root']))]


def check_pins(plan, plan_path, script_path):
    require(plan.get('schema') == 'haley-check-only-plan/v1' and plan.get('host') == 'haleyspc', 'plan schema or host')
    keys = ('launcher', 'helper', 'dispatch', 'bundle')
    for key in keys:
        path = plan['files'][key]['path']
        require(path.replace('\\', '/').startswith(ROOT) and '..' not in path, key + ' lies outside ' + ROOT)
    for key in keys:
        require(sha256(plan['files'][key]['path']) == plan['files'][key]['sha256'], key + ' differs from its pin')
    require(str(PureWindowsPath(script_path)) == str(PureWindowsPath(plan['files']['dispatch']['path'])),
            'this script is not the pinned dispatch script')
    require(str(PureWindowsPath(plan_path)).replace('\\', '/').startswith(ROOT), 'plan lies outside ' + ROOT)


def dispatch(plan, plan_path, python, helper):
    return helper.dispatch(plan['lane'], plan['run_id'], plan['release_condition'],
                           owner_command(plan, plan_path, python), cwd=str(PureWindowsPath(ROOT)),
                           busy_pattern=BUSY_PATTERN,
                           transport_record={'kind': 'haley-check-only-v1', 'plan_sha256': sha256(plan_path),
                                             'dispatch_sha256': plan['files']['dispatch']['sha256']},
                           python=str(PureWindowsPath(python)))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--plan', required=True)
    parser.add_argument('--python', required=True)
    parser.add_argument('--status')
    args = parser.parse_args()
    plan = json.loads(Path(args.plan).read_bytes())
    check_pins(plan, args.plan, __file__)
    helper = load_helper(plan['files']['helper']['path'])
    if args.status:
        print(json.dumps(helper.status(args.status), default=str))
        return
    try:
        result = dispatch(plan, args.plan, args.python, helper)
    except (helper.Held, helper.Refused) as exc:
        state = 'held' if isinstance(exc, helper.Held) else 'refused'
        print(json.dumps({'state': state, 'detail': str(exc), 'holder': getattr(exc, 'holder', None)}, default=str))
        sys.exit(3)
    print(json.dumps(result, default=str))
    sys.exit(0 if result.get('state') == 'dispatched' else 2)


if __name__ == '__main__':
    main()
