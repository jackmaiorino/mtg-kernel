"""Build the assigned baseline once under an admitted two-core host_slots claim."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path('D:/search-buffer-reuse-20261010')
SOURCE = ROOT / 'baseline-source'
BASE = '1d2404775a141db4cd90d7579f8f765db37d55df'
LINKER = Path('C:/Program Files (x86)/Microsoft Visual Studio/18/BuildTools/VC/Tools/MSVC/14.50.35717/bin/Hostx64/x64/link.exe')
RESERVE = 60 * 2**30
CAP = 20 * 2**30


def pin(path):
    path = Path(path)
    with path.open('rb') as stream:
        return {'path': path.as_posix(), 'sha256': hashlib.file_digest(stream, 'sha256').hexdigest(), 'bytes': path.stat().st_size}


def tree_bytes(path):
    total = 0
    for folder, dirs, files in os.walk(path, followlinks=False):
        dirs[:] = [d for d in dirs if not Path(folder, d).is_junction()]
        for name in files:
            item = Path(folder, name)
            try:
                if not item.is_symlink():
                    total += item.stat().st_size
            except FileNotFoundError:
                pass
    return total


def check_storage():
    if (ROOT / 'STOP').exists():
        raise RuntimeError('Lane STOP file present')
    size = tree_bytes(ROOT)
    if size >= CAP or shutil.disk_usage(ROOT).free < RESERVE:
        raise RuntimeError('Owned tree cap or D: reserve exceeded')
    return size


def main():
    sys.path.insert(0, str(SOURCE / 'python/tools'))
    from host_slots_v1 import parse_cores
    if not os.environ.get('HOST_SLOTS_CLAIM') or len(parse_cores(os.environ.get('HOST_SLOTS_CORES', ''))) != 2:
        raise RuntimeError('Run through supported host_slots two-core claim')
    if subprocess.check_output(['git', '-C', str(SOURCE), 'rev-parse', 'HEAD'], text=True).strip() != BASE:
        raise RuntimeError('Baseline source changed')
    runtime_paths = ['mtg-kernel', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml']
    if subprocess.check_output(['git', '-C', str(SOURCE), 'status', '--porcelain', '--untracked-files=all', '--', *runtime_paths], text=True).strip():
        raise RuntimeError('Baseline runtime source is dirty')
    check_storage()
    output = ROOT / 'build-baseline.json'
    if output.exists():
        raise RuntimeError('Existing terminal build receipt retained; use a new attempt helper')
    target = ROOT / 'target'
    target.mkdir(exist_ok=True)
    script = ROOT / 'build-baseline.cmd'
    script.write_text('@echo off\ncall "C:\\Program Files (x86)\\Microsoft Visual Studio\\18\\BuildTools\\Common7\\Tools\\VsDevCmd.bat" -arch=x64 -host_arch=x64 >nul\nif errorlevel 1 exit /b 1\nset CARGO_TARGET_DIR=D:\\search-buffer-reuse-20261010\\target\nset CARGO_BUILD_JOBS=2\nset RUSTFLAGS=\nset CARGO_ENCODED_RUSTFLAGS=\ncd /d D:\\search-buffer-reuse-20261010\\baseline-source\nC:\\Users\\Jack\\.cargo\\bin\\cargo.exe +1.94.1 build --release --locked -p mtg-kernel --bin regret_census_v1 -j 2\nexit /b %errorlevel%\n', encoding='utf-8')
    receipt = {'schema': 'search-buffer-reuse-build/v1', 'label': 'baseline', 'git_commit': BASE,
               'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
               'toolchain': 'rustc 1.94.1 (e408947bf 2026-03-25), cargo 1.94.1 (29ea6fb6a 2026-03-24)',
               'linker': pin(LINKER), 'linker_version': '14.50.35725.0',
               'cargo_lock': pin(SOURCE / 'Cargo.lock'), 'command': 'cargo +1.94.1 build --release --locked -p mtg-kernel --bin regret_census_v1 -j 2',
               'profile': 'release, lto thin, codegen-units 1, default target-cpu, no RUSTFLAGS',
               'admitted_cores': os.environ['HOST_SLOTS_CORES'], 'gpu': None,
               'projected_bytes': 12 * 2**30, 'cap_bytes': CAP, 'reserve_bytes': RESERVE, 'complete': False}
    started = time.perf_counter()
    code = None
    log = ROOT / 'build-baseline.log'
    try:
        with log.open('x', encoding='utf-8') as stream:
            child = subprocess.Popen(['cmd.exe', '/d', '/c', str(script)], stdout=stream, stderr=subprocess.STDOUT)
            while code is None:
                try:
                    code = child.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    try:
                        check_storage()
                    except Exception:
                        subprocess.run(['taskkill.exe', '/pid', str(child.pid), '/t', '/f'], capture_output=True, check=False)
                        child.wait()
                        raise
        receipt['exit_code'] = code
        if code != 0:
            raise RuntimeError(f'Baseline build failed: exit {code}')
        if subprocess.check_output(['git', '-C', str(SOURCE), 'status', '--porcelain', '--untracked-files=all', '--', *runtime_paths], text=True).strip():
            raise RuntimeError('Baseline runtime source changed during build')
        built = target / 'release/regret_census_v1.exe'
        baseline = ROOT / 'baseline-regret_census_v1.exe'
        shutil.copy2(built, baseline)
        digest = pin(baseline)['sha256']
        cold = Path('E:/pinned-binaries') / digest / baseline.name
        if shutil.disk_usage('E:/').free < RESERVE + baseline.stat().st_size:
            raise RuntimeError('Insufficient E: reserve to pin baseline')
        cold.parent.mkdir(parents=True, exist_ok=True)
        if cold.exists():
            if pin(cold)['sha256'] != digest:
                raise RuntimeError('Existing binary hash collision')
        else:
            shutil.copy2(baseline, cold)
        if pin(cold)['sha256'] != digest:
            raise RuntimeError('Cold baseline copy differs')
        receipt['binary'] = pin(cold)
        receipt['ssd_recovery_copy'] = pin(baseline)
        receipt['actual_owned_tree_bytes'] = check_storage()
        receipt['complete'] = True
    except Exception as exc:
        receipt['error'] = f'{type(exc).__name__}: {exc}'
    finally:
        receipt['elapsed_seconds'] = time.perf_counter() - started
        receipt['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        receipt['build_log'] = pin(log) if log.exists() else None
        output.write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
        print(json.dumps(receipt), flush=True)
    return 0 if receipt['complete'] else 1


if __name__ == '__main__':
    sys.exit(main())
