"""Continue the one owned build queue after baseline completion; no training."""
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path('D:/training-io-speedups-20261010')
REPO = Path('C:/Users/Jack/.codex/worktrees/training-io-candidate-20261010/mtg-kernel')
BASELINE = 'b3bd1c1c0d7b5b6766359a3901549de768bcc175'
CONFIG = Path('D:/training-speedups-20261009/desktop/source-config.json')
PYTHON = Path('D:/mtg-kernel-uv-python-019f63a2/cpython-3.13.14-windows-x86_64-none/python.exe')


def pin(path):
    with Path(path).open('rb') as stream:
        return {'path': str(path), 'sha256': hashlib.file_digest(stream, 'sha256').hexdigest()}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def now():
    return datetime.now(timezone.utc).isoformat()


def main():
    require(Path(sys.executable).resolve() == PYTHON.resolve(), 'pinned Python required')
    commit = sys.argv[1]
    out = ROOT / 'build-coordinator'; out.mkdir(exist_ok=False)
    state = {'complete': False, 'started_utc': now(), 'candidate_commit': commit,
             'source': pin(__file__), 'phase': 'waiting_baseline'}

    def save():
        temporary = out / 'state.pending'
        with temporary.open('w', encoding='utf-8') as stream:
            json.dump(state, stream, indent=2); stream.flush(); os.fsync(stream.fileno())
        os.replace(temporary, out / 'state.json')

    def stable():
        require(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip() == commit,
                'candidate head changed before build')
        require(not subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=REPO).strip(),
                'candidate tracked files changed before build')

    def preserve_runtime(variant, expected):
        binary = ROOT / 'target/release/native_expanded_training_run_v1.exe'
        identity = pin(binary)
        destination = Path('E:/pinned-binaries') / identity['sha256'] / binary.name
        require(shutil.disk_usage('E:/').free >= 60 * 1024**3 + binary.stat().st_size, 'binary recovery reserve')
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.exists():
            with binary.open('rb') as source, destination.open('xb') as copied:
                shutil.copyfileobj(source, copied); copied.flush(); os.fsync(copied.fileno())
        require(pin(destination)['sha256'] == identity['sha256'], 'pinned binary differs')
        validation = json.loads(subprocess.check_output([str(destination), '--validate-config', str(CONFIG)], text=True))
        require(validation['runtime']['engine_commit'] == expected, 'compiled source commit differs')
        record = {key: validation['runtime'][key] for key in ('engine_commit', 'tracked_tree_sha256')}
        record['binary'] = pin(destination)
        folder = ROOT / 'runtimes'; folder.mkdir(exist_ok=True)
        with (folder / (variant + '.json')).open('x', encoding='utf-8') as stream:
            json.dump(record, stream, indent=2)
        return pin(folder / (variant + '.json'))

    save()
    try:
        helper = REPO / 'docs/reports/training_io_speedups_20261010/resume_qualifications.py'
        spec = importlib.util.spec_from_file_location('build_directory_events', helper)
        module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        watcher = module.DirectoryChangeWait(ROOT)
        deadline = time.monotonic() + 21600
        try:
            result = ROOT / 'baseline-build-two-core.exit'
            while not result.exists():
                require(time.monotonic() < deadline, 'bounded baseline build wait expired')
                watcher.wait(60000)
        finally:
            watcher.close()
        require(int(result.read_text().strip()) == 0, 'baseline build failed; preserved logs')
        stable()
        state['baseline_runtime'] = preserve_runtime('baseline', BASELINE); save()
        temp = ROOT / 'native-test-temp'; temp.mkdir(exist_ok=True)
        lines = ['@echo off', 'set VSCMD_SKIP_SENDTELEMETRY=1',
                 'call "C:\\Program Files (x86)\\Microsoft Visual Studio\\18\\BuildTools\\Common7\\Tools\\VsDevCmd.bat" -arch=x64 -host_arch=x64 >nul',
                 'if errorlevel 1 exit /b %errorlevel%', 'set TEMP=' + str(temp), 'set TMP=' + str(temp),
                 'cd /d "' + str(REPO) + '"',
                 'cargo +1.94.1 build --release --locked -p mtg-kernel --bin native_expanded_training_run_v1 -j 2 --target-dir D:/training-io-speedups-20261010/target > D:/training-io-speedups-20261010/candidate-build.log 2>&1',
                 'if errorlevel 1 goto failed']
        filters = ['trajectory_input_workers_preserve_order_and_reject_mutated_or_unknown_bytes',
                   'checkpoint_input_binding_checks_order_and_exact_scalar_bits',
                   'ordinary_trainer_two_iteration_v3_fixture_state_hash_is_unchanged',
                   'ordinary_trainer_two_iteration_gae_fixture_cpu',
                   'native_expanded_training_run_v1::tests']
        for index, name in enumerate(filters):
            lines += ['cargo +1.94.1 test --release --locked -p mtg-kernel --lib -j 2 --target-dir D:/training-io-speedups-20261010/target '
                      + name + ' -- --test-threads=2 > D:/training-io-speedups-20261010/native-test-' + str(index) + '.log 2>&1',
                      'if errorlevel 1 goto failed']
        lines += ['echo 0 > D:/training-io-speedups-20261010/candidate-build.exit', 'exit /b 0',
                  ':failed', 'echo 1 > D:/training-io-speedups-20261010/candidate-build.exit', 'exit /b 1']
        command = ROOT / 'build-candidate.cmd'
        with command.open('xb') as stream:
            stream.write(('\r\n'.join(lines) + '\r\n').encode())
        stable(); state['phase'] = 'candidate_build_tests'; state['command'] = pin(command); save()
        argv = [str(PYTHON), '-B', str(REPO / 'python/tools/host_slots_v1.py'), 'run', '--wait',
                '--lane', 'codex-training-io-speedups-20261010', '--work-id', 'candidate-build-tests',
                '--cores', '2', '--wait-timeout', '21600', '--cwd', str(REPO), '--', 'cmd.exe', '/d', '/c', str(command)]
        with (out / 'candidate-supervisor.log').open('xb') as log:
            child = subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT)
        require(child.returncode == 0, 'guarded candidate build/tests failed')
        require(int((ROOT / 'candidate-build.exit').read_text().strip()) == 0, 'candidate test failure')
        stable(); state['candidate_runtime'] = preserve_runtime('candidate', commit)
        state.update(complete=True, phase='built_and_tested', finished_utc=now()); save()
    except BaseException as error:
        state.update(error=type(error).__name__ + ': ' + str(error), finished_utc=now()); save(); raise


if __name__ == '__main__':
    main()
