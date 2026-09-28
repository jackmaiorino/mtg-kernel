"""Pinned R14/collector Rust verification owner for the g115 WMI transport.

Runs only the fixed library-test suites below. Receipts are engineering evidence, not qualification.
The manifest reserves additional E-drive growth; periodic free-space checks are
conservative volume guards, not an OS disk quota or exclusive disk reservation.
"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

SCHEMA = 'g115-r14-windows-tests/v1'
SUITES = {'panel': ['registry_evolution', 'native_checkpoint_export_v1', 'sideboard_play_policy_v1', 'expanded_deck_training_v1'],
          'collector': ['opponent_kind', '--include-ignored']}
FEATURES = 'native-training-store-v2-production,experimental-burn-net8-packed-cuda-v1'
ENV_KEYS = {'PATH', 'INCLUDE', 'LIB', 'LIBPATH', 'CUDA_PATH', 'CUDA_PATH_V12_8'}
GIB = 1024 ** 3


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def pinned(ref):
    path = Path(ref['path']).resolve(strict=True)
    require(digest(path) == ref['sha256'], f'Pinned file changed: {path}')
    return path


def save(path, value):
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')
    os.replace(temporary, path)


def structural(plan, host):
    require(plan['schema'] == SCHEMA and host == 'jack', 'Build mode is Jack Windows only')
    require(plan['jobs'] in (1, 2, 3, 4), 'Build jobs must be 1..4')
    require(plan['suite'] in SUITES, 'Only pinned panel/collector library tests allowed')
    require(30 <= plan['timeout_seconds'] <= 3600, 'Build timeout must be 30..3600 seconds')
    require(0 < plan['additional_growth_bytes'] <= 32 * GIB, 'Explicit bounded build growth required')
    require(set(plan['environment']) <= ENV_KEYS, 'Unsupported build environment override')
    require(plan['source_commit'] and len(plan['source_commit']) == 40, 'Full source commit required')


def e_path(value):
    path = Path(value)
    require(path.is_absolute() and path.drive.lower() == 'e:', 'Build paths must be absolute on E')
    resolved = path.resolve()
    require(resolved.drive.lower() == 'e:', 'Resolved build path escaped E')
    return resolved


def available_memory():
    class Status(ctypes.Structure):
        _fields_ = [('length', ctypes.c_ulong), ('load', ctypes.c_ulong)] + [
            (name, ctypes.c_ulonglong) for name in ('total', 'available', 'page_total',
                                                   'page_available', 'virtual_total',
                                                   'virtual_available', 'extended')]
    status = Status()
    status.length = ctypes.sizeof(status)
    require(ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)), 'Memory census failed')
    return status.available


def admission(plan, host):
    structural(plan, host)
    require(os.name == 'nt', 'Windows required')
    repo = e_path(plan['source'])
    target = e_path(plan['target_directory'])
    temporary = e_path(plan['temp_directory'])
    require(target.name.startswith('cargo-target-g115-') and temporary.name.startswith('g115-build-temp-'),
            'Dedicated g115 target and temp directory names required')
    require(not target.is_relative_to(repo) and not temporary.is_relative_to(repo)
            and target != temporary, 'Build outputs must be separate from checkout')
    require(Path(__file__).resolve() == pinned(plan['documents']['launcher']), 'Launcher identity differs')
    pinned(plan['transport']['dispatcher'])
    def git(*args):
        return subprocess.check_output(['git', '-C', str(repo), *args], text=True).strip()
    require(git('rev-parse', 'HEAD') == plan['source_commit'], 'Source commit changed')
    require(not git('status', '--porcelain', '--untracked-files=normal'), 'Clean committed source required')
    cargo, rustc, linker = [pinned(plan['tools'][name]) for name in ('cargo', 'rustc', 'linker')]
    env = os.environ.copy()
    for name in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
                 'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER'):
        env.pop(name, None)
    env.update(plan['environment'])
    env.update(CARGO_TARGET_DIR=str(target), TEMP=str(temporary), TMP=str(temporary),
               CARGO_BUILD_JOBS=str(plan['jobs']), RUSTC=str(rustc), CARGO_INCREMENTAL='0',
               CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=str(linker))
    versions = {name: subprocess.check_output([str(tool), flag], text=True, env=env).strip()
                for name, tool, flag in [('rustc', rustc, '-vV'), ('cargo', cargo, '--version')]}
    require(versions['rustc'].startswith('rustc 1.94.1 '), 'Pinned Rust 1.94.1 required')
    free = shutil.disk_usage('E:/').free
    require(free >= 60 * GIB + plan['additional_growth_bytes'], 'E reserve plus build growth unavailable')
    memory = available_memory()
    require(memory >= 32 * GIB, 'Jack memory reserve unavailable')
    features = FEATURES if plan['suite'] == 'panel' else 'experimental-burn-net8-packed-cuda-v1'
    command = [str(cargo), 'test', '--locked', '--offline', '-p', 'mtg-kernel', '--release',
               '--config', 'profile.release.lto=false',
               '--config', 'profile.release.codegen-units=16',
               '--config', 'profile.release.package.mtg-kernel.codegen-units=16',
               '--features', features, '-j', str(plan['jobs']), '--lib', '--', *SUITES[plan['suite']]]
    return repo, target, temporary, env, command, {
        'source_commit': plan['source_commit'], 'versions': versions, 'tools': plan['tools'],
        'command': command, 'E_free_bytes': free, 'available_memory_bytes': memory,
        'additional_growth_bytes': plan['additional_growth_bytes'], 'formal_measurement': False}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--manifest', required=True)
    parser.add_argument('--host', required=True)
    parser.add_argument('--root')
    parser.add_argument('--check-only', action='store_true')
    args = parser.parse_args()
    plan = json.loads(Path(args.manifest).read_text(encoding='utf-8-sig'))
    repo, target, temporary, env, command, receipt = admission(plan, args.host)
    if args.check_only:
        print(json.dumps({'admitted': True, 'build_dispatched': False, **receipt}))
        return
    require(args.root is not None, 'Fresh worker root required')
    root = e_path(args.root)
    require(root.is_relative_to(e_path(plan['evidence_parent'])) and root != e_path(plan['evidence_parent']),
            'Worker root must be below the named evidence parent')
    require(not root.exists(), 'Worker root exists')
    root.mkdir(parents=False)
    # CreateNew ownership lock: never remove somebody else's stale lock.
    lock_path = target.with_name(target.name + '.g115-build.lock')
    child = None
    lock = None
    started = time.monotonic()
    receipt.update(complete=False, owner_pid=os.getpid(), manifest_sha256=digest(args.manifest))
    try:
        lock = lock_path.open('x', encoding='utf-8')
        lock.write(json.dumps({'owner_pid': os.getpid(), 'root': str(root)})); lock.flush()
        target.mkdir(exist_ok=True); temporary.mkdir(exist_ok=True)
        save(root / 'start.json', receipt)
        with (root / 'tests.log').open('wb') as log:
            child = subprocess.Popen(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT,
                                     creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW)
            receipt['cargo_pid'] = child.pid
            save(root / 'start.json', receipt)
            while True:
                try:
                    code = child.wait(timeout=5)
                    break
                except subprocess.TimeoutExpired:
                    require(time.monotonic() - started < plan['timeout_seconds'], 'Build time bound exceeded')
                    free = shutil.disk_usage('E:/').free
                    require(free >= 60 * GIB, 'E reserve reached during build')
                    require(receipt['E_free_bytes'] - free <= plan['additional_growth_bytes'],
                            'Observed net E-volume use exceeded build allowance')
                    require(available_memory() >= 32 * GIB, 'Memory reserve reached during build')
                    save(root / 'progress.json', {'elapsed_seconds': time.monotonic()-started,
                                                 'cargo_pid': child.pid, 'E_free_bytes': free})
        receipt['exit_code'] = code
        require(code == 0, 'Cargo tests failed; inspect tests.log')
        require(subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
                == plan['source_commit'], 'Source commit moved during build')
        require(not subprocess.check_output(['git', '-C', str(repo), 'status', '--porcelain',
                                             '--untracked-files=normal'], text=True).strip(),
                'Source changed during build')
        receipt['complete'] = True
    except Exception as error:
        receipt['error'] = str(error)
        if child is not None and child.poll() is None:
            # Popen retains the owned process handle; no broad process-name cleanup.
            subprocess.run(['taskkill.exe', '/PID', str(child.pid), '/T', '/F'], capture_output=True, check=False)
            child.wait(timeout=30)
    finally:
        receipt['elapsed_seconds'] = time.monotonic() - started
        receipt['E_free_after_bytes'] = shutil.disk_usage('E:/').free
        save(root / 'completion.json', receipt)
        if lock is not None:
            lock.close()
            lock_path.unlink()  # Only the lock created by this invocation.
    require(receipt['complete'], receipt.get('error', 'Build incomplete'))


if __name__ == '__main__':
    main()
