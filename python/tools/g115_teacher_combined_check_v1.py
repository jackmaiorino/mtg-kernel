"""ASTRA #020 seven-test correctness packet, dispatched only after parent handoff.

Canonical WMI transport owns the job. Hot writes stay in one fresh SSD root;
sealed evidence and the immutable test executable go to E:. Periodic resource
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
import time
import traceback

sys.dont_write_bytecode = True
GIB = 1024 ** 3
TESTS = [
    'expanded_deck_training_v1::registry_evolution_dispatch_tests::' + name
    for name in (
        'learner_initialization_refuses_registry_evolution_descriptors',
        'inference_refuses_registry_evolution_with_a_successor_checkpoint',
        'unknown_schemas_are_refused_without_fallback',
        'bo3_transition_admission_refuses_registry_evolution_descriptors',
    )
] + [
    'expanded_deck_training_v1::public_features::search_opponent::collect_tests::boundary_audit_checks_every_available_perturbation_of_a_live_root',
    'expanded_deck_training_v1::phase1_parallel_collection::tests::unclamped_learner_collection_is_serial_parallel_identical_and_keeps_opponents_legacy',
    'expanded_deck_training_v1::public_features::opponent_kind::tests::unclamped_learner_rows_replay_with_legacy_singletons_and_refuse_tampering',
]


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def save(path, value):
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')
    temporary.replace(path)


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


def validate(manifest, digest):
    require(sha(manifest) == digest, 'Manifest hash differs')
    plan = json.loads(manifest.read_text(encoding='utf-8'))
    require(plan['schema'] == 'g115-teacher-combined-check/v1', 'Wrong packet')
    require(plan['tests'] == TESTS, 'Seven-test scope differs')
    require(plan['jobs'] == 4 and plan['cap_bytes'] == 32 * GIB, 'Build/cap scope differs')
    require(plan['reserve_bytes'] == 60 * GIB and plan['memory_reserve_bytes'] == 32 * GIB,
            'Reserve scope differs')
    scratch = Path(plan['scratch']).resolve()
    require(scratch == Path('D:/e-scratch/g115-teacher-combined-check-001').resolve(), 'Wrong SSD root')
    for key in ('target', 'temp', 'cargo_home'):
        require(Path(plan[key]).resolve().is_relative_to(scratch), 'Hot path outside owned root: ' + key)
    require(Path(plan['cold']).resolve() == Path('E:/mtg-g115-lineage-20260923/teacher-combined-check-001').resolve(), 'Wrong cold root')
    for ref in plan['pins']:
        require(sha(ref['path']) == ref['sha256'], 'Pin differs: ' + ref['path'])
    require(Path(plan['script']).resolve() == Path(__file__).resolve(), 'Wrong owner script')
    clean(plan)
    return plan


def clean(plan):
    def git(*args):
        return subprocess.check_output(['git', '-C', plan['source'], *args], text=True).strip()
    require(git('rev-parse', 'HEAD') == plan['source_commit'], 'Source head differs')
    require(not git('status', '--porcelain'), 'Source worktree is dirty')


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
            'codex-g115', 'teacher-combined-check-001',
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
              'manifest_sha256': digest, 'started_utc': reservation.now_utc(), 'steps': [], 'gpu_tests': False}
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
        with log_path.open('xb') as log:
            child, placement = spawn_held(argv, argv[0], cwd=plan['source'], env=env,
                stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW)
            row.update(pid=child.pid, placement=placement)
            save(scratch / 'progress.json', result)
            try:
                while True:
                    try:
                        code = child.wait(timeout=5)
                        break
                    except subprocess.TimeoutExpired:
                        guard()
                        require(time.monotonic() - start < timeout, 'Stage time bound: ' + label)
            except BaseException:
                # Only the live Popen-owned child tree, within the reserved job.
                if child.poll() is None:
                    subprocess.run(['C:/Windows/System32/taskkill.exe', '/PID', str(child.pid), '/T', '/F'],
                                   capture_output=True, timeout=30)
                    child.wait(timeout=30)
                raise
        row.update(exit_code=code, seconds=time.monotonic()-start, finished_utc=reservation.now_utc(),
                   log=str(cold / log_path.name), log_sha256=sha(log_path))
        save(scratch / 'progress.json', result)
        require(code == 0, 'Stage failed: ' + label)
        guard(); clean(plan)
        return log_path.read_text(encoding='utf-8', errors='replace')

    try:
        require(not os.environ.get(reservation.TEST_ROOT_ENV), 'Canonical reservation only')
        result['reservation_token'] = adapter.check_owner()
        before_cutoff(plan)
        result['initial_resources'] = guard()
        for key in ('target', 'temp', 'cargo_home'):
            Path(plan[key]).mkdir(parents=True, exist_ok=False)
        cache_copied = 0
        guard(plan['cache_staging_estimate_bytes'])
        # Copy only registry archive/index inputs. The source cache is never
        # written; extraction, cache locks and Cargo metadata use owned SSD.
        for name in ('cache', 'index'):
            origin = Path(plan['registry_source']) / name
            for src in files(origin):
                dest = Path(plan['cargo_home']) / 'registry' / name / src.relative_to(origin)
                copy_new(src, dest, staged_cache=True)
                cache_copied += src.stat().st_size
        result['cache_staged_bytes'] = cache_copied
        env = os.environ.copy()
        for key in list(env):
            if key in ('RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER') or key.startswith(('MTG_', 'CARGO_')):
                env.pop(key)
        env.update(plan['environment'])
        env.update(CARGO_HOME=plan['cargo_home'], CARGO_TARGET_DIR=plan['target'], TEMP=plan['temp'], TMP=plan['temp'],
                   CARGO_BUILD_JOBS='4', CARGO_INCREMENTAL='0', RUSTC=plan['tools']['rustc']['path'],
                   CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=plan['tools']['linker']['path'],
                   RUST_TEST_THREADS='1', RUST_BACKTRACE='0', PYTHONDONTWRITEBYTECODE='1')
        env['PATH'] = str(Path(plan['tools']['rustc']['path']).parent) + os.pathsep + env['PATH']
        versions = {}
        for tool, flag in [('cargo', '--version'), ('rustc', '-vV'), ('linker', '/?')]:
            versions[tool] = run('version-' + tool, [plan['tools'][tool]['path'], flag], 60).splitlines()[0]
        require(versions['cargo'].startswith('cargo 1.94.1 ') and versions['rustc'].startswith('rustc 1.94.1 '), 'Toolchain version differs')
        require('14.50.35725.0' in versions['linker'], 'Linker version differs')
        result['versions'] = versions
        output = run('test-build', plan['build_command'], plan['build_timeout_seconds'])
        artifacts = []
        for line in output.splitlines():
            try:
                obj = json.loads(line)
            except ValueError:
                continue
            if obj.get('reason') == 'compiler-artifact' and obj.get('executable') and obj.get('profile', {}).get('test') and obj['target']['name'] == 'mtg_kernel':
                artifacts.append(Path(obj['executable']))
        require(len(artifacts) == 1, 'Expected exactly one library-test executable')
        binary = artifacts[0]
        require(binary.resolve().is_relative_to(Path(plan['target']).resolve()), 'Artifact outside owned target')
        binary_hash = sha(binary)
        pinned = Path('E:/pinned-binaries') / binary_hash / binary.name
        pinned_bytes = binary.stat().st_size
        if pinned.exists():
            require(sha(pinned) == binary_hash, 'Existing immutable binary differs')
        else:
            copy_new(binary, pinned)
        result['binary'] = {'path': str(pinned), 'sha256': binary_hash, 'bytes': pinned_bytes}
        for index, name in enumerate(TESTS):
            output = run(f'test-{index:02}', [str(pinned), name, '--exact', '--include-ignored', '--test-threads=1', '--color', 'never', '--nocapture'], plan['test_timeout_seconds'])
            found = re.findall(r'^test (\S+) \.\.\. (ok|FAILED|ignored)\s*$', output, re.M)
            require(found == [(name, 'ok')], 'Selected test name/outcome differs: ' + name)
            require(len(re.findall(r'^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; \d+ filtered out;', output, re.M)) == 1, 'Test summary differs')
        result['selected_test_names'] = TESTS
        result['selected_passed'] = 7
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
        result['scope'] = 'Seven combined-source CPU correctness checks; no real R14 inference, throughput, adoption or strength evidence.'
        save(scratch / 'owner-completion.json', result)
        # Terminal receipt last, so file-driven wakeup observes sealed logs.
        try:
            for src in sorted(scratch.iterdir()):
                if src.is_file() and src.name != 'owner-completion.json':
                    copy_new(src, cold / src.name, terminal=True)
            copy_new(scratch / 'owner-completion.json', cold / 'owner-completion.json', terminal=True)
        except BaseException:
            result['complete'] = False
            result['seal_error'] = traceback.format_exc()
            result['recovery_root'] = str(scratch)
            save(scratch / 'owner-completion.json', result)
            # Preserve a small terminal failure even if resource accounting
            # stopped full sealing. Never overwrite an already sealed receipt.
            destination = cold / 'owner-completion.json'
            if not destination.exists():
                with destination.open('x', encoding='utf-8') as stream:
                    json.dump(result, stream, indent=2)
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
    plan = validate(args.manifest, args.manifest_sha256)
    if args.check_only:
        print(json.dumps({'static_pins_verified': len(plan['pins']), 'source_commit': plan['source_commit'], 'selected_tests': len(TESTS), 'dispatched': False}))
    elif args.dispatch:
        dispatch(plan, args.manifest.resolve(), args.manifest_sha256)
    else:
        return worker(plan, args.manifest_sha256)
    return 0


if __name__ == '__main__':
    sys.exit(main())
