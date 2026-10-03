"""HaleysPC check-only path (version 1): run one cargo test or build command of a pinned mtg-kernel commit on
HaleysPC and collect pass or fail evidence (goal amendment 2026-09-28 02:15, CLAUDE #563; design CLAUDE #565, #566).

Flow, each step through an existing interface:
  prepare   git bundle of the pinned commit and the plan (schema haley-check-only-plan/v1); one staging map in
            the launcher's schema (g115-line-a-staging-map/v1) for the bundle, the runner, the dispatch script,
            the reservation helper and the plan, staged by the launcher's g115_line_a_stage_v1.py (hash checks,
            60 GiB reserve, no overwrite, SHA256 verification).
  dispatch  haley_check_only_dispatch_v1.py over SSH: one HaleysPC process calls host_reservation_v1.dispatch
            (acquire, busy refusal, WMI creation of the supervisor running the runner, handoff), so the
            dispatching process owns the lock until the supervisor adopts (CLAUDE #574).
  collect   the runner's completion and cargo log and the reservation's status copied into
            docs/reports/haley_check_only_v1/<run-id>/ with a receipt (schema haley-check-only-receipt/v1).

Check-only: pass or fail evidence for the suite owner, never a timing or identity receipt; no GPU tests.
Usage: python python/tools/haley_check_only_v1.py {prepare,dispatch,collect} RUN_DIR [options]
"""
import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

from haley_check_only_runner_v1 import build_command

HOST = 'haley@100.71.75.65'
ROOT = 'C:/mtg-line-a/check-only/'
MAP_SCHEMA = 'g115-line-a-staging-map/v1'
PLAN_SCHEMA = 'haley-check-only-plan/v1'
RECEIPT_SCHEMA = 'haley-check-only-receipt/v1'
REPO = Path(__file__).resolve().parents[2]
REPORTS = REPO / 'docs/reports/haley_check_only_v1'
RUNNER = Path(__file__).with_name('haley_check_only_runner_v1.py')
REMOTE_PYTHON = 'C:/Users/haley/AppData/Local/Programs/Python/Python312/python.exe'
DISPATCH = Path(__file__).with_name('haley_check_only_dispatch_v1.py')
GIB = 2 ** 30
RESERVE_BYTES = 60 * GIB  # artifact law clause 1, every volume
GROWTH_BYTES = 8 * GIB  # a panel-suite release test build measured about 1 GB of target on Jack's PC
# host_reservation_v1.dispatch outcomes. A run started (it may finish before the handoff). Only Held is a definite
# refusal that leaves nothing acquired for this run (the host is reserved, or the busy refusal cancelled this
# acquisition); Refused can follow a created supervisor, and an unconfirmed spawn may be running, so neither is retried.
STARTED = ('dispatched', 'finished-before-handoff')
RETRYABLE = ('held',)


def sha256(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def require(ok, message):
    if not ok:
        raise SystemExit('check-only refused: ' + message)


def remote_paths(lane, run_id, commit):
    return {'bundle': ROOT + 'bundles/%s.bundle' % commit, 'plan': ROOT + 'plans/%s.json' % run_id,
            'source_root': ROOT + 'src/%s' % commit, 'target_dir': ROOT + 'target/%s' % lane,
            'temp_dir': ROOT + 'tmp/%s' % run_id, 'worker_root': ROOT + 'runs/%s' % run_id}


def tool_remote(path):
    """Tools are content-addressed so a changed tool never overwrites a pinned one."""
    return ROOT + 'tools/%s/%s' % (sha256(path), Path(path).name)


def entry(local, remote):
    return {'local': str(local).replace('\\', '/'), 'remote': remote, 'sha256': sha256(local),
            'bytes': Path(local).stat().st_size}


def staging_map(entries, tools_commit):
    for item in entries:
        require(item['remote'].startswith(ROOT) and '..' not in item['remote'], 'a staged path leaves ' + ROOT)
    return {'schema': MAP_SCHEMA, 'host': 'haleyspc', 'tools_commit': tools_commit, 'files': entries}


def make_bundle(repo, ref, commit, out):
    """A bundle of ref's history, which must contain the pinned commit; the runner checks the commit out."""
    require(re.fullmatch(r'[0-9a-f]{40}', commit) is not None, 'commit is not 40 lowercase hex')
    ancestor = subprocess.run(['git', '-C', str(repo), 'merge-base', '--is-ancestor', commit, ref], capture_output=True)
    require(ancestor.returncode == 0, 'the pinned commit is not in ' + ref)
    path = Path(out) / ('%s.bundle' % commit)
    created = subprocess.run(['git', '-C', str(repo), 'bundle', 'create', str(path), ref], capture_output=True, text=True)
    require(created.returncode == 0, 'git bundle failed: ' + created.stderr.strip()[-300:])
    return path


def make_plan(lane, run_id, commit, cargo_args, files, toolchain, timeout_seconds=5400, growth_bytes=GROWTH_BYTES):
    paths = remote_paths(lane, run_id, commit)
    return {'schema': PLAN_SCHEMA, 'host': 'haleyspc', 'lane': lane, 'run_id': run_id, 'commit': commit,
            'files': files, 'toolchain': toolchain, 'source_root': paths['source_root'],
            'target_dir': paths['target_dir'], 'temp_dir': paths['temp_dir'], 'worker_root': paths['worker_root'],
            'cargo_args': list(cargo_args), 'timeout_seconds': timeout_seconds,
            'reserve_bytes': RESERVE_BYTES, 'growth_bytes': growth_bytes,
            'release_condition': 'the supervisor job is empty after the cargo command',
            'nonclaims': ['check-only pass or fail evidence for the suite owner', 'not a timing or identity receipt',
                          'no GPU test is in scope']}


def make_receipt(state, collected_dir, staging_receipt_sha256, plan_path):
    """Binds every collected file; a run that stopped before or around cargo is receipted as refused or error."""
    folder = Path(collected_dir)
    files = {path.name: sha256(path) for path in sorted(folder.glob('*'))
             if path.is_file() and path.name != 'receipt.json'}
    completion = json.loads((folder / 'completion.json').read_text(encoding='utf-8'))
    dispatched = json.loads((folder / 'dispatch.json').read_text(encoding='utf-8'))
    reservation = json.loads((folder / 'reservation-status.json').read_text(encoding='utf-8'))
    plan_bytes = Path(plan_path).read_bytes()
    plan = json.loads(plan_bytes)
    plan_sha256 = hashlib.sha256(plan_bytes).hexdigest()
    require(all(plan.get(key) == state[key] for key in ('lane', 'run_id', 'commit', 'cargo_args', 'toolchain')),
            'the local state differs from the prepared plan')
    require(completion.get('plan_sha256') == plan_sha256,
            'the completion plan digest differs from the prepared plan bytes')
    require(completion.get('commit') == state['commit'] and completion.get('run_id') == state['run_id'],
            'the completion record belongs to another run')
    stopped = [key for key in ('refused', 'error') if key in completion]
    if 'cargo.log' in files or completion.get('log_sha256') is not None or not stopped:
        require('cargo.log' in files and completion.get('log_sha256') == files['cargo.log'],
                'the collected cargo log differs from its producer digest')
    receipt = {'schema': RECEIPT_SCHEMA, 'host': 'haleyspc', 'lane': state['lane'], 'run_id': state['run_id'],
               'commit': state['commit'], 'cargo_args': state['cargo_args'],
               'started_utc': completion['started_utc'], 'finished_utc': completion['finished_utc'],
               'exit_code': completion['exit_code'],
               'reservation': {'token': dispatched['token'], 'dispatch_state': dispatched.get('state'),
                               'supervisor_pid': dispatched.get('pid'), 'token_fate': reservation.get('token_fate'),
                               'runner_token': completion.get('reservation_token')},
               'plan_sha256': plan_sha256, 'log_sha256': completion.get('log_sha256'),
               'staging_receipt_sha256': staging_receipt_sha256, 'collected_files': files,
               'nonclaims': completion['nonclaims']}
    if stopped:
        receipt.update({'outcome': stopped[0], 'detail': completion[stopped[0]]})
        return receipt
    require(completion.get('head') == state['commit'], 'the remote checkout differs from the pinned commit')
    require(completion.get('reservation_token') == dispatched.get('token'), 'the run did not hold the dispatched token')
    require(completion.get('argv') == build_command(plan), 'the executed command differs from the prepared plan')
    receipt.update({'outcome': 'ran', 'remote_head': completion['head'], 'clean': completion.get('clean'),
                    'toolchain': {'rustc_verbose_version_sha256': completion['toolchain']['rustc_verbose_version_sha256'],
                                  'cargo_version': completion['toolchain']['cargo_version'],
                                  'rustc': state['toolchain']['rustc'], 'cargo': state['toolchain']['cargo']},
                    'argv': completion['argv'], 'test_counts': completion['test_counts'],
                    'free_bytes': [completion.get('free_bytes_before'), completion.get('free_bytes_after')]})
    return receipt


class Remote:
    """HaleysPC over SSH and scp (BatchMode, no prompts)."""

    def __init__(self, host=HOST, run=subprocess.run):
        self.host, self.run = host, run

    def powershell(self, script, timeout=1800, check=True):
        result = self.run(['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=8', self.host,
                           'powershell -NoProfile -NonInteractive -Command "%s"' % script],
                          capture_output=True, text=True, timeout=timeout)
        if not check:
            return result
        require(result.returncode == 0, 'HaleysPC command failed: ' + (result.stderr or result.stdout).strip()[-400:])
        return result.stdout

    def fetch(self, remote, local, check=True):
        result = self.run(['scp', '-q', '-o', 'BatchMode=yes', '%s:%s' % (self.host, remote), str(local)],
                          capture_output=True, text=True, timeout=600)
        require(result.returncode == 0 or not check, 'scp failed: ' + result.stderr.strip()[-400:])
        return result.returncode == 0


def stage(tools_dir, map_path, receipt_path):
    """The launcher's staging tool from a pinned read-only checkout (its python/tools directory)."""
    sys.path.insert(0, str(tools_dir))
    import g115_line_a_stage_v1 as launcher_stage
    return launcher_stage.stage(map_path, receipt_path, launcher_stage.SshRemote())


def plan_files(bundle, bundle_remote, helper):
    return {'bundle': {'path': bundle_remote, 'sha256': sha256(bundle)},
            'launcher': {'path': tool_remote(RUNNER), 'sha256': sha256(RUNNER)},
            'dispatch': {'path': tool_remote(DISPATCH), 'sha256': sha256(DISPATCH)},
            'helper': {'path': tool_remote(helper), 'sha256': sha256(helper)}}


def remote_script(script, python=REMOTE_PYTHON):
    """A PowerShell line running one staged Python script with literal single-quoted arguments."""
    return "& '%s' %s" % (python, ' '.join("'%s'" % part for part in script))


def cmd_prepare(args):
    run_dir = Path(args.run_dir)
    run_dir.mkdir(parents=True)
    toolchain = json.loads(Path(args.toolchain).read_text(encoding='utf-8'))
    bundle = make_bundle(args.repo, args.ref, args.commit, run_dir)
    paths = remote_paths(args.lane, args.run_id, args.commit)
    files = plan_files(bundle, paths['bundle'], args.helper)
    plan = make_plan(args.lane, args.run_id, args.commit, json.loads(args.cargo_args), files, toolchain,
                     args.timeout_seconds)
    plan_path = run_dir / 'plan.json'
    plan_path.write_text(json.dumps(plan, indent=1) + '\n', encoding='utf-8', newline='\n')
    tools_commit = subprocess.run(['git', '-C', str(args.launcher_tools), 'rev-parse', 'HEAD'],
                                  capture_output=True, text=True).stdout.strip()
    staged = staging_map([entry(bundle, paths['bundle']), entry(RUNNER, files['launcher']['path']),
                          entry(DISPATCH, files['dispatch']['path']), entry(args.helper, files['helper']['path']),
                          entry(plan_path, paths['plan'])], tools_commit)
    (run_dir / 'staging-map.json').write_text(json.dumps(staged, indent=1) + '\n', encoding='utf-8', newline='\n')
    stage(Path(args.launcher_tools) / 'python/tools', run_dir / 'staging-map.json', run_dir / 'staging-receipt.json')
    state = {'lane': args.lane, 'run_id': args.run_id, 'commit': args.commit, 'toolchain': toolchain,
             'cargo_args': plan['cargo_args'], 'files': files, 'paths': paths}
    (run_dir / 'state.json').write_text(json.dumps(state, indent=1) + '\n', encoding='utf-8', newline='\n')
    print(json.dumps({'prepared': str(run_dir), 'plan_sha256': sha256(plan_path)}))


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=1, default=str) + '\n', encoding='utf-8', newline='\n')


def last_json(text):
    """The dispatch script's answer: its last stdout line as a JSON object, or None."""
    lines = (text or '').strip().splitlines()
    try:
        value = json.loads(lines[-1]) if lines else None
    except ValueError:
        return None
    return value if isinstance(value, dict) else None


def may_attempt(earlier):
    """Only a definite Held answer admits another attempt; a pending (lost response), unknown, refused or
    unconfirmed attempt never does."""
    return earlier is None or earlier.get('state') in RETRYABLE


def cmd_dispatch(args):
    run_dir = Path(args.run_dir)
    state = json.loads((run_dir / 'state.json').read_text(encoding='utf-8'))
    attempt = run_dir / 'dispatch-attempt.json'
    earlier = json.loads(attempt.read_text(encoding='utf-8')) if attempt.exists() else None
    require(may_attempt(earlier), 'an earlier attempt ended %s; a lost or unconfirmed dispatch is never retried'
            % (earlier or {}).get('state'))
    write_json(attempt, {'state': 'pending'})
    result = Remote().powershell(remote_script([state['files']['dispatch']['path'], '--plan', state['paths']['plan'],
                                                '--python', REMOTE_PYTHON]), check=False)
    answer = last_json(result.stdout)
    recorded = answer or {'state': 'unknown', 'returncode': result.returncode,
                          'stderr': (result.stderr or '').strip()[-400:]}
    write_json(attempt, recorded)
    with open(run_dir / 'dispatch-attempts.jsonl', 'a', encoding='utf-8', newline="\n") as history:
        history.write(json.dumps(recorded, default=str) + "\n")
    require(recorded.get('state') in STARTED, 'dispatch ended %s (see %s)' % (recorded.get('state'), attempt))
    write_json(run_dir / 'dispatch.json', answer)
    print(json.dumps({'dispatched': state['run_id'], 'state': answer['state'], 'token': answer['token'],
                      'supervisor_pid': answer.get('pid')}))


def cmd_collect(args):
    run_dir = Path(args.run_dir)
    state = json.loads((run_dir / 'state.json').read_text(encoding='utf-8'))
    dispatched = json.loads((run_dir / 'dispatch.json').read_text(encoding='utf-8'))
    remote = Remote()
    reservation = json.loads(remote.powershell(remote_script([state['files']['dispatch']['path'], '--plan',
                                                              state['paths']['plan'], '--python', REMOTE_PYTHON,
                                                              '--status', dispatched['token']])).strip().splitlines()[-1])
    fate = str(reservation.get('token_fate'))
    require(fate != 'holds', 'the run still holds the reservation; collect after it ends')
    require(fate == 'released' or fate.startswith('reclaimed-'), 'the reservation has no recorded end (%s)' % fate)
    out = REPORTS / state['run_id']
    out.mkdir(parents=True)
    remote.fetch(state['paths']['worker_root'] + '/completion.json', out / 'completion.json')
    ran = json.loads((out / 'completion.json').read_text(encoding='utf-8')).get('exit_code') is not None
    remote.fetch(state['paths']['worker_root'] + '/cargo.log', out / 'cargo.log', check=ran)  # absent if stopped early
    for name, value in (('dispatch.json', dispatched), ('reservation-status.json', reservation)):
        (out / name).write_text(json.dumps(value, indent=1, default=str) + '\n', encoding='utf-8', newline='\n')
    receipt = make_receipt(state, out, sha256(run_dir / 'staging-receipt.json'), run_dir / 'plan.json')
    (out / 'receipt.json').write_text(json.dumps(receipt, indent=1) + '\n', encoding='utf-8', newline='\n')
    print(json.dumps({'receipt': str(out / 'receipt.json'), 'sha256': sha256(out / 'receipt.json'),
                      'outcome': receipt['outcome'], 'exit_code': receipt['exit_code'],
                      'test_counts': receipt.get('test_counts')}))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest='op', required=True)
    prepare = sub.add_parser('prepare')
    prepare.add_argument('run_dir')
    prepare.add_argument('--repo', default=str(REPO))
    prepare.add_argument('--ref', required=True, help='a ref whose history contains the pinned commit')
    prepare.add_argument('--commit', required=True)
    prepare.add_argument('--lane', required=True)
    prepare.add_argument('--run-id', required=True)
    prepare.add_argument('--cargo-args', required=True, help='JSON list, for example ["test","--release",...]')
    prepare.add_argument('--toolchain', required=True, help='JSON file: {"cargo": {"path","sha256"}, "rustc": {...}}')
    prepare.add_argument('--helper', required=True, help='local host_reservation_v1.py at its pinned commit')
    prepare.add_argument('--launcher-tools', required=True, help='pinned read-only checkout of opus/line-a-launcher-v1')
    prepare.add_argument('--timeout-seconds', type=int, default=5400)
    for op in ('dispatch', 'collect'):
        sub.add_parser(op).add_argument('run_dir')
    args = parser.parse_args()
    {'prepare': cmd_prepare, 'dispatch': cmd_dispatch, 'collect': cmd_collect}[args.op](args)


if __name__ == '__main__':
    main()
