"""HaleysPC check-only path (version 1): run one cargo test or build command of a pinned mtg-kernel commit on
HaleysPC and collect pass or fail evidence (goal amendment 2026-09-28 02:15, CLAUDE #563; design CLAUDE #565, #566).

Flow, each step through an existing interface:
  prepare   git bundle of the pinned commit; a staging map in the launcher's schema (g115-line-a-staging-map/v1)
            for the bundle, the runner, the reservation helper and the dispatcher, staged by the launcher's
            g115_line_a_stage_v1.py (hash checks, 60 GiB reserve, no overwrite, SHA256 verification); the
            reservation acquired on HaleysPC (host_reservation_v1.py acquire); the plan (schema
            haley-check-only-plan/v1, carrying the token) written and staged the same way.
  dispatch  the staged g115_d3_windows_dispatch.ps1 over SSH (CODEX #595 recipe) with fresh worker and control
            roots; the dispatcher starts haley_check_only_runner_v1.py through WMI; the created owner is
            recorded with host_reservation_v1.py handoff.
  collect   the owner records, the runner's completion and the cargo log copied into
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
import time
from pathlib import Path

HOST = 'haley@100.71.75.65'
ROOT = 'C:/mtg-line-a/check-only/'
MAP_SCHEMA = 'g115-line-a-staging-map/v1'
PLAN_SCHEMA = 'haley-check-only-plan/v1'
RECEIPT_SCHEMA = 'haley-check-only-receipt/v1'
REPO = Path(__file__).resolve().parents[2]
REPORTS = REPO / 'docs/reports/haley_check_only_v1'
RUNNER = Path(__file__).with_name('haley_check_only_runner_v1.py')
REMOTE_PYTHON = 'C:/Users/haley/AppData/Local/Programs/Python/Python312/python.exe'
COLLECTED = [('control', 'owner-start.json'), ('control', 'owner-completion.json'), ('control', 'config.json'),
             ('control', 'worker.log'), ('runs', 'completion.json'), ('runs', 'cargo.log')]


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
            'temp_dir': ROOT + 'tmp/%s' % run_id, 'worker_root': ROOT + 'runs/%s' % run_id,
            'control_root': ROOT + 'control/%s' % run_id}


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


def make_plan(lane, run_id, commit, cargo_args, files, toolchain, token, dispatcher_sha256, timeout_seconds=5400):
    paths = remote_paths(lane, run_id, commit)
    return {'schema': PLAN_SCHEMA, 'host': 'haleyspc', 'lane': lane, 'run_id': run_id, 'commit': commit,
            'files': files, 'toolchain': toolchain, 'source_root': paths['source_root'],
            'target_dir': paths['target_dir'], 'temp_dir': paths['temp_dir'], 'cargo_args': list(cargo_args),
            'reservation_token': token, 'timeout_seconds': timeout_seconds,
            'documents': {'launcher': dict(files['launcher'])}, 'transport': {'dispatcher': {'sha256': dispatcher_sha256}},
            'nonclaims': ['check-only pass or fail evidence for the suite owner', 'not a timing or identity receipt',
                          'no GPU test is in scope']}


def make_receipt(state, collected_dir, staging_receipts):
    files = {}
    for path in sorted(Path(collected_dir).glob('*')):
        if path.is_file() and path.name != 'receipt.json':
            files[path.name] = sha256(path)
    completion = json.loads((Path(collected_dir) / 'completion.json').read_text(encoding='utf-8'))
    owner = json.loads((Path(collected_dir) / 'owner-completion.json').read_text(encoding='utf-8-sig'))
    require(completion.get('commit') == state['commit'] and completion.get('head') == state['commit'],
            'the remote checkout differs from the pinned commit')
    return {'schema': RECEIPT_SCHEMA, 'host': 'haleyspc', 'lane': state['lane'], 'run_id': state['run_id'],
            'commit': state['commit'], 'remote_head': completion['head'], 'clean': completion.get('clean'),
            'toolchain': {'rustc_verbose_version_sha256': completion['toolchain']['rustc_verbose_version_sha256'],
                          'cargo_version': completion['toolchain']['cargo_version'],
                          'rustc': state['toolchain']['rustc'], 'cargo': state['toolchain']['cargo']},
            'cargo_args': state['cargo_args'], 'argv': completion['argv'],
            'started_utc': completion['started_utc'], 'finished_utc': completion['finished_utc'],
            'exit_code': completion['exit_code'], 'test_counts': completion['test_counts'],
            'owner_complete': owner.get('complete'), 'reservation_token': state['token'],
            'staging_receipts': staging_receipts, 'collected_files': files,
            'nonclaims': completion['nonclaims']}


class Remote:
    """HaleysPC over SSH and scp (BatchMode, no prompts)."""

    def __init__(self, host=HOST, run=subprocess.run):
        self.host, self.run = host, run

    def powershell(self, script, timeout=1800):
        result = self.run(['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=8', self.host,
                           'powershell -NoProfile -NonInteractive -Command "%s"' % script],
                          capture_output=True, text=True, timeout=timeout)
        require(result.returncode == 0, 'HaleysPC command failed: ' + (result.stderr or result.stdout).strip()[-400:])
        return result.stdout

    def fetch(self, remote, local):
        result = self.run(['scp', '-q', '-o', 'BatchMode=yes', '%s:%s' % (self.host, remote), str(local)],
                          capture_output=True, text=True, timeout=600)
        require(result.returncode == 0, 'scp failed: ' + result.stderr.strip()[-400:])


def stage(tools_dir, map_path, receipt_path):
    """The launcher's staging tool from a pinned read-only checkout (its python/tools directory)."""
    sys.path.insert(0, str(tools_dir))
    import g115_line_a_stage_v1 as launcher_stage
    return launcher_stage.stage(map_path, receipt_path, launcher_stage.SshRemote())


def cmd_prepare(args):
    run_dir = Path(args.run_dir)
    run_dir.mkdir(parents=True)
    toolchain = json.loads(Path(args.toolchain).read_text(encoding='utf-8'))
    bundle = make_bundle(args.repo, args.ref, args.commit, run_dir)
    paths = remote_paths(args.lane, args.run_id, args.commit)
    files = {'bundle': {'path': paths['bundle'], 'sha256': sha256(bundle)},
             'launcher': {'path': tool_remote(RUNNER), 'sha256': sha256(RUNNER)},
             'helper': {'path': tool_remote(args.helper), 'sha256': sha256(args.helper)}}
    dispatcher = {'path': tool_remote(args.dispatcher), 'sha256': sha256(args.dispatcher)}
    tools_commit = subprocess.run(['git', '-C', str(args.launcher_tools), 'rev-parse', 'HEAD'],
                                  capture_output=True, text=True).stdout.strip()
    first = staging_map([entry(bundle, paths['bundle']), entry(RUNNER, files['launcher']['path']),
                         entry(args.helper, files['helper']['path']), entry(args.dispatcher, dispatcher['path'])],
                        tools_commit)
    (run_dir / 'staging-map-1.json').write_text(json.dumps(first, indent=1) + '\n', encoding='utf-8', newline='\n')
    stage(Path(args.launcher_tools) / 'python/tools', run_dir / 'staging-map-1.json', run_dir / 'staging-receipt-1.json')
    remote = Remote()
    acquired = remote.powershell("& '%s' '%s' acquire --lane '%s' --work-id '%s' --release-condition '%s'" % (
        REMOTE_PYTHON, files['helper']['path'], args.lane, args.run_id, 'supervisor job empty after the cargo command'))
    token = json.loads(acquired)['token']
    plan = make_plan(args.lane, args.run_id, args.commit, json.loads(args.cargo_args), files, toolchain, token,
                     dispatcher['sha256'], args.timeout_seconds)
    (run_dir / 'plan.json').write_text(json.dumps(plan, indent=1) + '\n', encoding='utf-8', newline='\n')
    second = staging_map([entry(run_dir / 'plan.json', paths['plan'])], tools_commit)
    (run_dir / 'staging-map-2.json').write_text(json.dumps(second, indent=1) + '\n', encoding='utf-8', newline='\n')
    stage(Path(args.launcher_tools) / 'python/tools', run_dir / 'staging-map-2.json', run_dir / 'staging-receipt-2.json')
    state = {'lane': args.lane, 'run_id': args.run_id, 'commit': args.commit, 'token': token, 'toolchain': toolchain,
             'cargo_args': plan['cargo_args'], 'helper': files['helper']['path'], 'dispatcher': dispatcher['path'],
             'paths': paths}
    (run_dir / 'state.json').write_text(json.dumps(state, indent=1) + '\n', encoding='utf-8', newline='\n')
    print(json.dumps({'prepared': str(run_dir), 'token': token}))


def cmd_dispatch(args):
    run_dir = Path(args.run_dir)
    state = json.loads((run_dir / 'state.json').read_text(encoding='utf-8'))
    remote, paths = Remote(), state['paths']
    output = remote.powershell("& '%s' -Manifest '%s' -HostName haleyspc -WorkerRoot '%s' -ControlRoot '%s' -Python '%s'" % (
        state['dispatcher'], paths['plan'], paths['worker_root'], paths['control_root'], REMOTE_PYTHON))
    (run_dir / 'dispatch.txt').write_text(output, encoding='utf-8', newline='\n')
    owner = re.search(r'"?owner_pid"?\s*[:=]\s*(\d+)', output)
    require(owner is not None, 'the dispatcher reported no owner pid; the reservation stays with its owner record')
    remote.powershell("& '%s' '%s' handoff --token '%s' --pid %s" % (REMOTE_PYTHON, state['helper'], state['token'],
                                                                     owner.group(1)))
    print(json.dumps({'dispatched': state['run_id'], 'owner_pid': int(owner.group(1))}))


def cmd_collect(args):
    run_dir = Path(args.run_dir)
    state = json.loads((run_dir / 'state.json').read_text(encoding='utf-8'))
    remote, paths = Remote(), state['paths']
    out = REPORTS / state['run_id']
    out.mkdir(parents=True)
    for root, name in COLLECTED:
        base = paths['control_root'] if root == 'control' else paths['worker_root']
        remote.fetch(base + '/' + name, out / name)
    staging = {name: sha256(run_dir / name) for name in ('staging-receipt-1.json', 'staging-receipt-2.json')}
    receipt = make_receipt(state, out, staging)
    (out / 'receipt.json').write_text(json.dumps(receipt, indent=1) + '\n', encoding='utf-8', newline='\n')
    print(json.dumps({'receipt': str(out / 'receipt.json'), 'sha256': sha256(out / 'receipt.json'),
                      'exit_code': receipt['exit_code'], 'test_counts': receipt['test_counts']}))


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
    prepare.add_argument('--dispatcher', required=True, help='local g115_d3_windows_dispatch.ps1 Codex pinned')
    prepare.add_argument('--launcher-tools', required=True, help='pinned read-only checkout of opus/line-a-launcher-v1')
    prepare.add_argument('--timeout-seconds', type=int, default=5400)
    for op in ('dispatch', 'collect'):
        sub.add_parser(op).add_argument('run_dir')
    args = parser.parse_args()
    {'prepare': cmd_prepare, 'dispatch': cmd_dispatch, 'collect': cmd_collect}[args.op](args)


if __name__ == '__main__':
    main()
