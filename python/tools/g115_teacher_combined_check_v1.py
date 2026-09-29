"""ASTRA #077 three-stage configuration validation packet, dispatched only after parent handoff.

Canonical WMI transport owns the job. Hot writes stay in one fresh SSD root;
sealed logs/receipts go to E:; build outputs remain in the retained fresh D: root. Resource
checks are conservative accounting, not an operating-system disk quota.
"""
import argparse
import ctypes
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
import traceback

sys.dont_write_bytecode = True
GIB = 1024 ** 3
WORK_ID = 'teacher-combined-check-006'
SCRATCH = Path('D:/e-scratch/g115-' + WORK_ID)
COLD = Path('E:/mtg-g115-lineage-20260923') / WORK_ID
CODE_UNDER_CHECK = '27122ce62b44576707321b265e4736f19a3f0c4c'
PREPARATION_CHANGED_PATHS = {
    'python/tools/g115_teacher_combined_check_v1.py',
    'python/tests/test_g115_teacher_combined_check_v1.py',
}
CACHE_ROOT = Path('D:/e-scratch/g115-teacher-combined-check-004')
CARGO_CONFIG = '[profile.release.package.mtg-kernel]\ncodegen-units = 4\n'
AFFECTED_TEST = 'expanded_deck_training_v1::tests::line_b_teacher_rollouts_are_invariant_to_hidden_placement_and_engine_rng'
STAGE_ARGS = [
    ('default-invariance', ['test', '--release', '--locked', '--offline', '-p', 'mtg-kernel', '--lib', AFFECTED_TEST, '--', '--exact']),
    ('default-jsonl', ['build', '--release', '--locked', '--offline', '-p', 'mtg-kernel', '--bin', 'kernel_rl_env']),
    ('cuda-compile', ['check', '--release', '--locked', '--offline', '-p', 'mtg-kernel', '--lib', '--features', 'experimental-burn-net8-packed-cuda-v1']),
]


def stages(cargo):
    return [{'label': label, 'argv': [cargo, *args]} for label, args in STAGE_ARGS]

LINKER = Path('C:/Program Files (x86)/Microsoft Visual Studio/18/BuildTools/VC/Tools/MSVC/14.50.35717/bin/Hostx64/x64/link.exe')
LINKER_SHA256 = 'ee9b29be652eee20affa6963a7ce54d01271b0f1b2443e315ea86469cbb95694'
LINKER_BANNER = 'Microsoft (R) Incremental Linker Version 14.50.35725.0'
LINKER_USAGE = 'usage: LINK [options] [files] [@commandfile]'


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def check_stage_exit(plan, label, argv, code, output):
    if code == 0:
        return
    # The observed nonzero help exit is not a general linker success code.
    # Bind the exception to the actual executable, pin and exact help argv.
    require(code == 1100 and label == 'version-linker' and len(argv) == 2
            and argv[1] == '/?' and Path(argv[0]).resolve() == LINKER.resolve(),
            'Stage failed: ' + label)
    linker = plan['tools']['linker']
    require(Path(linker['path']).resolve() == LINKER.resolve()
            and linker['sha256'] == LINKER_SHA256 and sha(argv[0]) == LINKER_SHA256,
            'Linker help executable pin differs')
    lines = output.splitlines()
    require(bool(lines) and lines[0] == LINKER_BANNER
            and LINKER_USAGE in [line.strip() for line in lines]
            and 'options:' in [line.strip() for line in lines], 'Linker help output differs')
    # MSVC diagnostics have a severity followed by a colon or diagnostic code.
    # /ERRORREPORT and similar help option names are not error diagnostics.
    require(not re.search(r'\b(?:fatal\s+)?error(?:\s+[A-Z]+\d+)?\s*:', output, re.I),
            'Linker help contains an error diagnostic')


def save(path, value):
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')
    temporary.replace(path)


def publish_receipt(path, value):
    """Stage and verify before the Windows atomic, non-replacing rename."""
    require(not path.exists(), 'Terminal receipt already sealed: ' + str(path))
    payload = (json.dumps(value, indent=2) + '\n').encode('utf-8')
    with tempfile.NamedTemporaryFile(mode='wb', dir=path.parent,
                                     prefix='.completion-', suffix='.pending', delete=False) as stream:
        staged = Path(stream.name)
        stream.write(payload)
        stream.flush()
        os.fsync(stream.fileno())
    require(staged.read_bytes() == payload, 'Terminal receipt staging differs')
    json.loads(staged.read_text(encoding='utf-8'))
    # On Windows rename fails if the destination exists, including a racing
    # publisher. Failed staging stays nonterminal for recovery; never expose
    # an unfinished owner-completion.json or overwrite a sealed receipt.
    staged.rename(path)


def admission_failure(digest):
    # Validation did not establish any manifest paths. Use only fixed owned
    # roots, bounded diagnostics and an empty step list; never start a stage.
    result = {'complete': False, 'phase': 'worker-admission', 'steps': [],
              'manifest_sha256': digest[:64], 'error': traceback.format_exc()[-8192:],
              'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat()}
    try:
        SCRATCH.mkdir(parents=True, exist_ok=True)
        publish_receipt(SCRATCH / 'owner-completion.json', result)
    except BaseException:
        result['local_receipt_error'] = traceback.format_exc()[-2048:]
    try:
        COLD.mkdir(parents=True, exist_ok=True)
        publish_receipt(COLD / 'owner-completion.json', result)
    except BaseException:
        result['publication_error'] = traceback.format_exc()[-2048:]
        print(json.dumps(result), file=sys.stderr)
    return 1


def files(root):
    if not root.exists():
        return
    for directory, dirs, names in os.walk(root, followlinks=False):
        for name in dirs + names:
            p = Path(directory) / name
            require(not p.is_symlink() and not p.is_junction(), 'Linked job/cache path: ' + str(p))
        for name in names:
            yield Path(directory) / name


def size(root):
    # Round every logical file up to a 4 KiB allocation unit. No cache or
    # staging subtree is excluded; volume growth is checked independently.
    total = 0
    for p in files(root):
        try:
            total += ((p.stat().st_size + 4095) // 4096) * 4096
        except FileNotFoundError:  # Cargo can rename a live temporary file.
            continue
    return total


def available_memory():
    class Memory(ctypes.Structure):
        _fields_ = [('length', ctypes.c_ulong), ('load', ctypes.c_ulong)] + [
            (n, ctypes.c_ulonglong) for n in ('total', 'available', 'pt', 'pa', 'vt', 'va', 'ex')]
    value = Memory()
    value.length = ctypes.sizeof(value)
    require(ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(value)), 'Memory query failed')
    return value.available


def cache_inventory(root):
    rows = sorted((p.relative_to(root).as_posix(), p.stat().st_size) for p in files(root))
    require(bool(rows), 'Empty build cache: ' + str(root))
    return {'files': len(rows), 'allocated_bytes': sum((n + 4095) // 4096 * 4096 for _, n in rows),
            'inventory_sha256': hashlib.sha256(json.dumps(rows, separators=(',', ':')).encode()).hexdigest()}


def validate(manifest, digest):
    require(sha(manifest) == digest, 'Manifest hash differs')
    plan = json.loads(manifest.read_text(encoding='utf-8'))
    require(plan['schema'] == 'g115-teacher-combined-check/v1', 'Wrong packet')
    require(plan['work_id'] == WORK_ID, 'Work identity differs')
    require(plan['stages'] == stages(plan['tools']['cargo']['path']), 'Three-stage scope differs')
    require(plan['jobs'] == 4 and plan['cap_bytes'] == 32 * GIB, 'Build/cap scope differs')
    require(plan['reserve_bytes'] == 60 * GIB and plan['memory_reserve_bytes'] == 32 * GIB,
            'Reserve scope differs')
    scratch = Path(plan['scratch']).resolve()
    require(scratch == SCRATCH.resolve(), 'Wrong SSD root')
    for key, suffix in [('temp', 'temp'), ('target', 'target'), ('cargo_home', 'cargo-home')]:
        require(Path(plan[key]).resolve() == scratch / suffix, 'Wrong owned build path: ' + key)
    require(Path(plan['cold']).resolve() == COLD.resolve(), 'Wrong cold root')
    for key, expected in [('owner_completion', COLD / 'owner-completion.json'),
                          ('active_progress', SCRATCH / 'progress.json'),
                          ('active_dispatch', SCRATCH / 'dispatch.json')]:
        require(Path(plan[key]).resolve() == expected.resolve(), 'Wrong output path: ' + key)
    require(plan['code_under_check'] == CODE_UNDER_CHECK, 'Code under check differs')
    require(plan['cargo_config'] == CARGO_CONFIG, 'Build configuration differs')
    expected = [(CACHE_ROOT / 'target', 'target'), (CACHE_ROOT / 'cargo-home' / 'registry', 'cargo-home/registry')]
    require(len(plan['caches']) == len(expected), 'Cache scope differs')
    for ref, (root, destination) in zip(plan['caches'], expected):
        require(Path(ref['path']).resolve() == root.resolve() and ref['destination'] == destination,
                'Cache source/destination differs')
        require(ref['inventory'] == cache_inventory(root), 'Cache inventory changed')
    require(plan['closure_headroom_bytes'] == 256 * 1024**2
            and 0 < plan['projected_bytes'] <= plan['cap_bytes']
            and plan['stage_timeout_seconds'] == 3600, 'Budget/time bounds differ')
    for ref in plan['pins']:
        require(Path(ref['path']).stat().st_size == ref['bytes'] and sha(ref['path']) == ref['sha256'],
                'Pin differs: ' + ref['path'])
    require(Path(plan['script']).resolve() == Path(__file__).resolve(), 'Wrong owner script')
    clean(plan)
    return plan


def clean(plan):
    def git(*args):
        return subprocess.check_output(['git', '-C', plan['source'], *args], text=True).strip()
    require(git('rev-parse', 'HEAD') == plan['source_commit'], 'Source head differs')
    require(not git('status', '--porcelain'), 'Source worktree is dirty')
    require(git('merge-base', CODE_UNDER_CHECK, 'HEAD') == CODE_UNDER_CHECK, 'Checked code is not ancestor')
    changed = set(git('diff', '--name-only', CODE_UNDER_CHECK, 'HEAD').splitlines())
    require(changed <= PREPARATION_CHANGED_PATHS, 'Preparation changed reviewed Rust/build inputs')


def before_cutoff(plan):
    require(datetime.datetime.now(datetime.timezone.utc) < datetime.datetime.fromisoformat(plan['no_new_work_at']),
            'No-new-work cutoff reached')


def dispatch(plan, manifest, digest):
    import host_reservation_v1 as reservation
    require(not os.environ.get(reservation.TEST_ROOT_ENV), 'Canonical reservation only')
    before_cutoff(plan)
    scratch = Path(plan['scratch'])
    require(not (scratch / 'attempt.json').exists(), 'Attempt already exists; inspect later, never redispatch')
    require(not (Path(plan['cold']) / 'owner-completion.json').exists(), 'Packet already terminal')
    require(all(shutil.disk_usage(d).free >= plan['reserve_bytes'] + plan['cap_bytes'] for d in ('D:/', 'E:/')), 'Reserve plus cap unavailable')
    require(available_memory() >= plan['memory_reserve_bytes'], 'Memory reserve unavailable')
    scratch.mkdir(parents=True, exist_ok=True)
    with (scratch / 'attempt.json').open('x', encoding='utf-8') as stream:
        json.dump({'manifest_sha256': digest, 'source_commit': plan['source_commit'], 'started_utc': reservation.now_utc()}, stream)
    result = {'manifest_sha256': digest}
    try:
        result.update(reservation.dispatch(
            'codex-g115', WORK_ID,
            'release only after worker terminal and contained job empty',
            [sys.executable, str(Path(__file__).resolve()), '--worker', '--manifest', str(manifest), '--manifest-sha256', digest],
            str(scratch), python=sys.executable, busy_pattern=plan['busy_pattern'],
            transport_record={'owner_completion': str(Path(plan['cold']) / 'owner-completion.json'),
                              'manifest_sha256': digest, 'source_commit': plan['source_commit']}))
    except BaseException:
        result.update(state='refused', error=traceback.format_exc())
    save(scratch / 'dispatch.json', result)
    print(json.dumps(result))
    require(result.get('state') in ('dispatched', 'finished-before-handoff'), 'No confirmed dispatch; preserve attempt')


def worker(plan, digest):
    import host_reservation_v1 as reservation
    import g115_reserved_dispatch_v1 as adapter
    from windows_held_spawn_v1 import spawn_held
    scratch, cold = Path(plan['scratch']), Path(plan['cold'])
    start_free = {d: shutil.disk_usage(d).free for d in ('D:/', 'E:/')}
    result = {'complete': False, 'source_commit': plan['source_commit'], 'merge_commit': plan['merge_commit'],
              'manifest_sha256': digest, 'started_utc': reservation.now_utc(), 'steps': [], 'gpu_tests': False,
              'code_under_check': plan['code_under_check'], 'cargo_config': plan['cargo_config']}
    pinned_bytes = 0
    copy_checked = time.monotonic()

    def guard(extra=0, terminal=False):
        free = {d: shutil.disk_usage(d).free for d in start_free}
        require(all(v - extra >= plan['reserve_bytes'] for v in free.values()), 'Disk reserve reached')
        used = size(scratch) + size(cold) + pinned_bytes
        ceiling = plan['cap_bytes'] - (0 if terminal else plan['closure_headroom_bytes'])
        require(used + extra <= ceiling, 'Total cache/target/staging/evidence cap reached')
        require(sum(max(0, start_free[d] - free[d]) for d in free) + extra <= ceiling, 'Observed volume growth cap reached')
        memory = available_memory()
        require(terminal or memory >= plan['memory_reserve_bytes'], 'Memory reserve reached')
        return {'accounted_bytes': used, 'free_bytes': free, 'available_memory_bytes': memory}

    def copy_new(src, dest, terminal=False, staged_cache=False):
        nonlocal copy_checked
        require(not dest.exists(), 'Refuse overwriting evidence/cache: ' + str(dest))
        if not staged_cache:
            guard(src.stat().st_size, terminal=terminal)
        dest.parent.mkdir(parents=True, exist_ok=True)
        with src.open('rb') as inp, dest.open('xb') as out:
            while chunk := inp.read(1024 * 1024):
                out.write(chunk)
                if time.monotonic() - copy_checked >= 2:
                    guard(terminal=terminal)
                    copy_checked = time.monotonic()
        require(sha(src) == sha(dest), 'Copy hash differs: ' + str(src))

    def run(label, argv, timeout):
        adapter.check_owner(); clean(plan); guard(); before_cutoff(plan)
        row = {'label': label, 'argv': argv, 'started_utc': reservation.now_utc()}
        result['steps'].append(row)
        log_path = scratch / (label + '.log')
        start = time.monotonic()
        child = None
        try:
            with log_path.open('xb') as log:
                child, placement = spawn_held(argv, argv[0], cwd=plan['source'], env=env,
                    stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                    creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW)
                row.update(pid=child.pid, placement=placement)
                save(scratch / 'progress.json', result)
                while True:
                    try:
                        code = child.wait(timeout=5)
                        break
                    except subprocess.TimeoutExpired:
                        guard()
                        require(time.monotonic() - start < timeout, 'Stage time bound: ' + label)
            row.update(exit_code=code, seconds=time.monotonic()-start, finished_utc=reservation.now_utc(),
                       log=str(cold / log_path.name), log_sha256=sha(log_path))
            save(scratch / 'progress.json', result)
            output = log_path.read_text(encoding='utf-8', errors='replace')
            check_stage_exit(plan, label, argv, code, output)
            guard(); clean(plan)
            return output
        except BaseException:
            # Includes the first progress write and log close after spawn.
            # Only the live Popen-owned child tree, within the reserved job.
            if child is not None and child.poll() is None:
                subprocess.run(['C:/Windows/System32/taskkill.exe', '/PID', str(child.pid), '/T', '/F'],
                               capture_output=True, timeout=30)
                child.wait(timeout=30)
            raise

    try:
        require(not os.environ.get(reservation.TEST_ROOT_ENV), 'Canonical reservation only')
        result['reservation_token'] = adapter.check_owner()
        before_cutoff(plan)
        result['initial_resources'] = guard()
        for key in ('temp', 'target', 'cargo_home'):
            Path(plan[key]).mkdir(parents=True, exist_ok=False)
        guard(sum(ref['inventory']['allocated_bytes'] for ref in plan['caches']))
        for ref in plan['caches']:
            root = Path(ref['path'])
            require(cache_inventory(root) == ref['inventory'], 'Cache changed before staging')
            for src in files(root):
                copy_new(src, scratch / ref['destination'] / src.relative_to(root), staged_cache=True)
            require(cache_inventory(root) == ref['inventory'], 'Cache changed during staging')
        (Path(plan['cargo_home']) / 'config.toml').write_text(plan['cargo_config'], encoding='utf-8')
        result['cache_staged_bytes'] = sum(ref['inventory']['allocated_bytes'] for ref in plan['caches'])
        env = os.environ.copy()
        for key in list(env):
            if key in ('RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER') or key.startswith(('MTG_', 'CARGO_')):
                env.pop(key)
        env.update(plan['environment'])
        env.update(CARGO_HOME=plan['cargo_home'], CARGO_TARGET_DIR=plan['target'],
                   CARGO_BUILD_JOBS='4', CARGO_INCREMENTAL='0', RUSTC=plan['tools']['rustc']['path'],
                   CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=plan['tools']['linker']['path'],
                   TEMP=plan['temp'], TMP=plan['temp'], CARGO_TERM_COLOR='never', RUST_TEST_THREADS='1',
                   RUST_BACKTRACE='0', PYTHONDONTWRITEBYTECODE='1')
        # Clear after all overlays, including inherited or manifest settings.
        for key in list(env):
            if key.upper() == 'RUST_TEST_NOCAPTURE':
                env.pop(key)
        env['PATH'] = str(Path(plan['tools']['rustc']['path']).parent) + os.pathsep + env['PATH']
        result['capture'] = 'libtest default; RUST_TEST_NOCAPTURE removed'
        result['build_outputs'] = []
        for stage in plan['stages']:
            output = run(stage['label'], stage['argv'], plan['stage_timeout_seconds'])
            if stage['label'] == 'default-invariance':
                found = re.findall(r'^test (\S+) \.\.\. (ok|FAILED|ignored)\s*$', output, re.M)
                require(found == [(AFFECTED_TEST, 'ok')], 'Affected exact test name/outcome differs')
                require(len(re.findall(r'^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; \d+ filtered out;', output, re.M)) == 1,
                        'Affected test summary differs')
                result['selected_test_names'] = [AFFECTED_TEST]
                result['selected_passed'] = 1
                paths = re.findall(r'^\s*Running unittests src[\\/]lib\.rs \((.+\.exe)\)\s*$', output, re.M)
                require(len(paths) == 1, 'Expected Cargo library-test executable path')
                binary = Path(paths[0])
                if not binary.is_absolute():
                    binary = Path(plan['source']) / binary
            elif stage['label'] == 'default-jsonl':
                binary = Path(plan['target']) / 'release' / 'kernel_rl_env.exe'
            else:
                continue  # cargo check produces no runnable CUDA library artifact.
            require(binary.resolve().is_relative_to(Path(plan['target']).resolve()), 'Build output outside owned target')
            result['build_outputs'].append({'stage': stage['label'], 'path': str(binary),
                                           'bytes': binary.stat().st_size, 'sha256': sha(binary)})
        result['stages_passed'] = 3
        result['final_resources'] = guard()
        result['complete'] = True
    except BaseException:
        result['error'] = traceback.format_exc()
    finally:
        if result.get('reservation_token'):
            try:
                result['auxiliary_cleanup'] = adapter.cleanup_auxiliary({'compiler_auxiliary': plan['auxiliary']})
            except BaseException:
                result['cleanup_error'] = traceback.format_exc()
                result['complete'] = False
        result['finished_utc'] = reservation.now_utc()
        result['scope'] = 'Default library-test compilation and one exact invariance test, default JSONL build, CUDA-feature library check only; no CUDA linking/GPU behavior, broader suite, adoption or strength evidence.'
        # Terminal receipt last, so file-driven wakeup observes sealed logs.
        try:
            save(scratch / 'owner-completion.json', result)
            for src in sorted(scratch.iterdir()):
                if src.is_file() and src.name != 'owner-completion.json':
                    copy_new(src, cold / src.name, terminal=True)
            guard((scratch / 'owner-completion.json').stat().st_size, terminal=True)
            publish_receipt(cold / 'owner-completion.json', result)
        except BaseException:
            result['complete'] = False
            result['seal_error'] = traceback.format_exc()
            result['recovery_root'] = str(scratch)
            try:
                save(scratch / 'owner-completion.json', result)
            except BaseException:
                result['local_receipt_error'] = traceback.format_exc()[-2048:]
            # Failure uses the same verified atomic publication as success.
            publish_receipt(cold / 'owner-completion.json', result)
    return 0 if result['complete'] else 1


def main():
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--check-only', action='store_true')
    mode.add_argument('--dispatch', action='store_true')
    mode.add_argument('--worker', action='store_true')
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--manifest-sha256', required=True)
    args = parser.parse_args()
    if args.worker:
        try:
            plan = validate(args.manifest, args.manifest_sha256)
        except BaseException:
            return admission_failure(args.manifest_sha256)
        return worker(plan, args.manifest_sha256)
    plan = validate(args.manifest, args.manifest_sha256)
    if args.check_only:
        print(json.dumps({'static_pins_verified': len(plan['pins']), 'source_commit': plan['source_commit'], 'code_under_check': CODE_UNDER_CHECK, 'stages': len(STAGE_ARGS), 'dispatched': False}))
    elif args.dispatch:
        dispatch(plan, args.manifest.resolve(), args.manifest_sha256)
    return 0


if __name__ == '__main__':
    sys.exit(main())
