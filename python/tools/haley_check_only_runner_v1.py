"""HaleysPC check-only runner (version 1): the owner command the host reservation's supervisor runs on HaleysPC.

haley_check_only_dispatch_v1.py calls host_reservation_v1.dispatch, which acquires the HaleysPC lock and creates
through WMI: python host_reservation_v1.py supervise --token T -- python -u haley_check_only_runner_v1.py --manifest
PLAN --host haleyspc --root WORKER_ROOT. The supervisor's job therefore contains this runner and cargo, and the lock
releases when they end (CLAUDE #574). The runner checks the plan (schema haley-check-only-plan/v1), every pinned
file and that its token holds this lane's lock for this run, prepares a clean detached git checkout of the pinned
commit from the staged bundle, and runs the one cargo command at BelowNormal priority with an isolated target
directory. The compiler is selected by --config build.rustc=<absolute rustc> with RUSTC unset (CLAUDE #526
S4): the native-store capture sees a drive-absolute RUSTC and the crate compile sees no build override. It keeps
C: above its reserve after the plan's declared growth, writes WORKER_ROOT/cargo.log and WORKER_ROOT/completion.json
and exits with the cargo exit code; a refusal or error before or around cargo is recorded in completion.json
(refused or error, exit code null) and exits 2.

Check-only: the result is pass or fail evidence for the suite owner, never a timing or identity receipt.
"""
import argparse
import datetime
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path, PureWindowsPath

PLAN_SCHEMA = 'haley-check-only-plan/v1'
COMPLETION_SCHEMA = 'haley-check-only-completion/v1'
ROOT = 'C:/mtg-line-a/check-only/'
BELOW_NORMAL_PRIORITY_CLASS = 0x00004000
MIN_RESERVE_BYTES = 60 * 2 ** 30  # artifact law clause 1, every volume
# Variables that would override the compiler or its flags; removed so the pinned toolchain alone decides.
OVERRIDES = ('RUSTC', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_RUSTFLAGS', 'RUSTC_WRAPPER',
             'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER',
             'CARGO_BUILD_RUSTC', 'CARGO_BUILD_TARGET', 'CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET_DIR', 'RUSTDOCFLAGS')
# Cargo reads any configuration key from CARGO_<KEY>; these families change the profile, build or target settings.
OVERRIDE_PREFIXES = ('CARGO_PROFILE_', 'CARGO_BUILD_', 'CARGO_TARGET_', 'CARGO_ENCODED_')
# Jack's PC checks out under Git for Windows' system default core.autocrlf=true. The clone pins the same value so the
# files that .gitattributes leaves to text=auto get the same bytes on both hosts (the *.rs, *.toml and *.dek rules
# pin their own endings).
LINE_ENDINGS = ('core.autocrlf', 'true')
TEST_RESULT = re.compile(r'^test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored', re.M)


def sha256(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def require(ok, message):
    if not ok:
        raise SystemExit('check-only refused: ' + message)


def under_root(path):
    text = str(path).replace('\\', '/')
    return text.startswith(ROOT) and '..' not in text


def check_plan(plan):
    """Plan shape and path rules; returns nothing, refuses on any violation."""
    require(plan.get('schema') == PLAN_SCHEMA, 'plan schema')
    require(plan.get('host') == 'haleyspc', 'plan host')
    require(re.fullmatch(r'[0-9a-f]{40}', plan.get('commit', '')) is not None, 'plan commit is not 40 lowercase hex')
    require(re.fullmatch(r'[a-z0-9][a-z0-9-]{0,63}', plan.get('lane', '')) is not None, 'plan lane')
    require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{0,95}', plan.get('run_id', '')) is not None, 'plan run id')
    for key in ('source_root', 'target_dir', 'temp_dir'):
        require(under_root(plan[key]), key + ' lies outside ' + ROOT)
    for key in ('bundle', 'helper', 'launcher', 'dispatch'):
        require(under_root(plan['files'][key]['path']), key + ' lies outside ' + ROOT)
    require(under_root(plan['worker_root']), 'worker_root lies outside ' + ROOT)
    require(int(plan.get('reserve_bytes', 0)) >= MIN_RESERVE_BYTES and int(plan.get('growth_bytes', 0)) > 0,
            'the plan must keep at least the 60 GiB reserve and declare its growth')
    args = plan.get('cargo_args')
    require(isinstance(args, list) and args and all(isinstance(a, str) for a in args), 'cargo arguments')
    require(args[0] in ('test', 'build', 'check', 'clippy'), 'only test, build, check or clippy commands are admitted')
    require(not any(a.startswith('--target-dir') or a.startswith('--config') for a in args),
            'the target directory and compiler come from the plan, not the arguments')
    for tool in ('cargo', 'rustc'):
        path = PureWindowsPath(plan['toolchain'][tool]['path'])
        require(path.is_absolute() and path.drive, tool + ' path must be drive-absolute')


def build_rustc_config(rustc_path):
    """The S4 selection: a TOML literal string keeps the backslashes the native-store capture requires."""
    windows = str(PureWindowsPath(rustc_path))
    require('\\' in windows and '/' not in windows and "'" not in windows, 'rustc path must be a backslash path')
    return "build.rustc='%s'" % windows


def build_environment(base, plan):
    env = {k: v for k, v in base.items()
           if k.upper() not in OVERRIDES and not k.upper().startswith(OVERRIDE_PREFIXES)}
    toolchain_bin = str(PureWindowsPath(plan['toolchain']['rustc']['path']).parent)
    env['PATH'] = toolchain_bin + os.pathsep + base.get('PATH', '')
    env['CARGO_TARGET_DIR'] = str(PureWindowsPath(plan['target_dir']))
    env['TEMP'] = env['TMP'] = str(PureWindowsPath(plan['temp_dir']))
    env['CARGO_INCREMENTAL'] = '0'
    return env


def build_command(plan):
    cargo = str(PureWindowsPath(plan['toolchain']['cargo']['path']))
    return [cargo, '--config', build_rustc_config(plan['toolchain']['rustc']['path'])] + plan['cargo_args']


def load_helper(path):
    """The staged host_reservation_v1 module (its pin is checked before this is called)."""
    import importlib.util
    spec = importlib.util.spec_from_file_location('host_reservation_v1', str(path))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def held_reservation(plan, helper, environ):
    """The supervisor passes its token in the environment; it must hold this lane's lock for this run."""
    token = environ.get(helper.TOKEN_ENV)
    require(bool(token), 'no host reservation token: the runner runs only under the reservation supervisor')
    state = helper.status(token)
    record = state.get('record') or {}
    require(state.get('token_fate') == 'holds' and record.get('lane') == plan['lane']
            and record.get('work_id') == plan['run_id'], "the token does not hold this lane's lock for this run")
    return token


def test_counts(log_text):
    totals = [0, 0, 0]
    for match in TEST_RESULT.finditer(log_text):
        for index in range(3):
            totals[index] += int(match.group(index + 1))
    return dict(zip(('passed', 'failed', 'ignored'), totals))


def git(source, *args):
    result = subprocess.run(['git', '-C', str(source)] + list(args), capture_output=True, text=True)
    require(result.returncode == 0, 'git %s failed: %s' % (' '.join(args), result.stderr.strip()[-300:]))
    return result.stdout.strip()


def prepare_source(plan):
    """A clean detached checkout of the pinned commit, cloned from the staged bundle when absent."""
    source = Path(plan['source_root'])
    bundle = plan['files']['bundle']['path']
    if not source.exists():
        result = subprocess.run(['git', 'clone', '--quiet', '--no-checkout', '--config', '%s=%s' % LINE_ENDINGS,
                                 bundle, str(source)], capture_output=True, text=True)
        require(result.returncode == 0, 'git clone from the bundle failed: ' + result.stderr.strip()[-300:])
    require(git(source, 'config', '--local', '--get', LINE_ENDINGS[0]) == LINE_ENDINGS[1],
            'the checkout does not pin %s=%s' % LINE_ENDINGS)
    git(source, 'checkout', '--quiet', '--detach', plan['commit'])
    head = git(source, 'rev-parse', 'HEAD')
    require(head == plan['commit'], 'checkout HEAD %s differs from the pinned commit' % head)
    status = git(source, 'status', '--porcelain=v1', '--untracked-files=all')
    require(status == '', 'the checkout is not clean')
    return head


def toolchain_record(plan):
    rustc = str(PureWindowsPath(plan['toolchain']['rustc']['path']))
    verbose = subprocess.run([rustc, '-vV'], capture_output=True).stdout
    cargo = subprocess.run([str(PureWindowsPath(plan['toolchain']['cargo']['path'])), '-V'],
                           capture_output=True, text=True).stdout.strip()
    return {'rustc_verbose_version': verbose.decode('utf-8', 'replace'),
            'rustc_verbose_version_sha256': hashlib.sha256(verbose).hexdigest(), 'cargo_version': cargo}


def check_space(plan, free_bytes):
    """The build may grow C: by growth_bytes; the drive keeps its reserve (artifact law clause 1)."""
    require(free_bytes - int(plan['growth_bytes']) >= int(plan['reserve_bytes']),
            'C: has %d free bytes; %d of growth would cross the %d-byte reserve'
            % (free_bytes, int(plan['growth_bytes']), int(plan['reserve_bytes'])))


def run(plan, root, environ):
    """Every check, the checkout and the one cargo command; returns the completion fields."""
    require(sha256(__file__) == plan['files']['launcher']['sha256'], 'runner differs from its plan pin')
    for key in ('bundle', 'helper', 'dispatch'):
        require(sha256(plan['files'][key]['path']) == plan['files'][key]['sha256'], key + ' differs from its pin')
    for tool in ('cargo', 'rustc'):
        require(sha256(plan['toolchain'][tool]['path']) == plan['toolchain'][tool]['sha256'], tool + ' differs from its pin')
    fields = {'reservation_token': held_reservation(plan, load_helper(plan['files']['helper']['path']), environ)}
    fields['free_bytes_before'] = shutil.disk_usage(PureWindowsPath(ROOT).anchor).free
    check_space(plan, fields['free_bytes_before'])
    Path(plan['temp_dir']).mkdir(parents=True, exist_ok=True)
    fields['head'] = prepare_source(plan)
    fields['clean'] = True
    fields['toolchain'] = toolchain_record(plan)
    fields['argv'] = build_command(plan)
    log_path = root / 'cargo.log'
    with open(log_path, 'wb') as log:
        result = subprocess.run(fields['argv'], stdout=log, stderr=subprocess.STDOUT,
                                env=build_environment(environ, plan), cwd=plan['source_root'],
                                creationflags=BELOW_NORMAL_PRIORITY_CLASS, timeout=int(plan.get('timeout_seconds', 5400)))
    fields.update({'exit_code': result.returncode, 'log_sha256': sha256(log_path),
                   'test_counts': test_counts(log_path.read_text(encoding='utf-8', errors='replace')),
                   'free_bytes_after': shutil.disk_usage(PureWindowsPath(ROOT).anchor).free})
    return fields


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--manifest', required=True)
    parser.add_argument('--host', required=True)
    parser.add_argument('--root', required=True)
    args = parser.parse_args()
    require(args.host == 'haleyspc', 'this runner admits HaleysPC only')
    plan_bytes = Path(args.manifest).read_bytes()
    plan = json.loads(plan_bytes)
    check_plan(plan)
    require(str(PureWindowsPath(args.root)) == str(PureWindowsPath(plan['worker_root'])), 'root differs from the plan')
    root = Path(args.root)
    root.mkdir(parents=True)  # a second start of the same plan stops here
    record = {'schema': COMPLETION_SCHEMA, 'host': 'haleyspc', 'lane': plan['lane'], 'run_id': plan['run_id'],
              'plan_sha256': hashlib.sha256(plan_bytes).hexdigest(), 'commit': plan['commit'], 'started_utc': now(),
              'nonclaims': ['check-only pass or fail evidence for the suite owner',
                            'not a timing or identity receipt', 'no GPU test is in scope']}
    try:
        record.update(run(plan, root, os.environ))
        code = record['exit_code']
    except (SystemExit, Exception) as stopped:  # every stop before or around cargo leaves a record to collect
        record.update({'refused' if isinstance(stopped, SystemExit) else 'error': str(stopped) or repr(stopped),
                       'exit_code': None})
        code = 2
    record['finished_utc'] = now()
    (root / 'completion.json').write_text(json.dumps(record, indent=1) + '\n', encoding='utf-8', newline='\n')
    sys.exit(code)


if __name__ == '__main__':
    main()
