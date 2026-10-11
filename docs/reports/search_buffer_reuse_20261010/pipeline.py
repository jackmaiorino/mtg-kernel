"""One marker watcher, with no core claim while waiting; no automatic retries.

Parent writes candidate-ready.json to bind the approved source snapshot.
measurements-approved.json additionally authorizes the fixed qualification/ABBA.
"""
import argparse
import datetime
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

import qualification as q

ROOT = q.SCRATCH
READY = ROOT / 'candidate-ready.json'
MEASURE = ROOT / 'measurements-approved.json'
PYTHON = str(q.PYTHON)
SLOTS = q.REPO / 'python/tools/host_slots_v1.py'
CARGO = 'C:/Users/Jack/.cargo/bin/cargo.exe'
HERE = Path(__file__).resolve().parent
BASE = q.BASE


def commands():
    common = [CARGO, '+1.94.1']
    tail = ['--release', '--locked', '-p', 'mtg-kernel', '-j', '2']
    checks = [common + ['test', *tail, '--lib', name, '--', '--test-threads=2']
              for name in ('native_flat_tensorizer', 'native_policy_value_net_v1', 'sideboard_play_policy_v1')]
    checks += [common + ['clippy', *tail, '--lib', '--', '-D', 'warnings'],
               common + ['check', *tail, '--lib', '--features', 'gameplay-decision-trace-v1'],
               common + ['build', *tail, '--bin', 'regret_census_v1']]
    return checks


def ready():
    marker = json.loads(READY.read_text())
    q.require(marker['baseline_commit'] == BASE, 'Marker baseline differs')
    q.require(marker['build_and_tests_approved'] is True, 'Build/test snapshot approval missing')
    q.require(marker['commands'] == commands(), 'Approved command list differs')
    q.require(marker['pipeline_sha256'] == q.pin(__file__)['sha256'], 'Pipeline changed after approval')
    return marker


def admission():
    q.require(not (ROOT / 'STOP').exists(), 'Lane STOP file present')
    sys.path.insert(0, str(q.REPO / 'python/tools'))
    from host_slots_v1 import parse_cores
    q.require(bool(os.environ.get('HOST_SLOTS_CLAIM')) and len(parse_cores(os.environ.get('HOST_SLOTS_CORES', ''))) == 2,
              'Supported two-core host_slots claim required')


def approved_measurement(marker):
    if not MEASURE.exists():
        return False
    decision = json.loads(MEASURE.read_text())
    q.require(decision['candidate_commit'] == marker['candidate_commit'], 'Measurement commit differs')
    q.require(decision['qualification_sha256'] == q.pin(HERE / 'qualification.py')['sha256'], 'Measurement coordinator differs')
    q.require(decision['phases'] == ['qualify', 'abba'], 'Measurement phase approval differs')
    q.require(decision['limits'] == q.LIMITS and decision['root_ids'] == q.ROOT_IDS, 'Measurement workload differs')
    return True


def measure(marker):
    admission()
    q.require(approved_measurement(marker), 'No approval for measurements')
    source = ROOT / 'candidate-source'
    binary = ROOT / 'candidate-regret_census_v1.exe'
    base = [PYTHON, '-B', str(HERE / 'qualification.py')]
    terminal = {'schema': 'search-buffer-reuse-measurement-terminal/v1', 'complete': False, 'started_utc': q.now()}
    try:
        subprocess.run(base + ['prepare', '--attempt', 'q001', '--candidate-binary', str(binary),
                              '--candidate-commit', marker['candidate_commit'], '--candidate-source', str(source)], check=True)
        for phase in ('qualify', 'abba'):
            terminal['active_phase'] = phase
            subprocess.run(base + ['run', '--attempt', 'q001', '--phase', phase], check=True)
        terminal['complete'] = True
    except Exception as exc:
        terminal['error'] = f'{type(exc).__name__}: {exc}'
        raise
    finally:
        terminal['finished_utc'] = q.now()
        q.put(ROOT / 'measurement-terminal.json', terminal)


def candidate():
    admission()
    marker = ready()
    q.storage()
    source = ROOT / 'candidate-source'
    q.require(subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip() == marker['candidate_commit'], 'Candidate snapshot differs')
    runtime_paths = ['mtg-kernel', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml']
    q.require(not subprocess.check_output(['git', '-C', str(source), 'status', '--porcelain', '--untracked-files=all', '--', *runtime_paths], text=True).strip(), 'Candidate runtime source dirty')
    baseline = json.loads((ROOT / 'build-baseline.json').read_text())
    q.verify(baseline['linker'])
    receipt = {'schema': 'search-buffer-reuse-candidate-build/v1', 'commit': marker['candidate_commit'],
               'baseline_commit': BASE, 'started_utc': q.now(), 'complete': False,
               'admitted_cores': os.environ['HOST_SLOTS_CORES'], 'commands': [],
               'toolchain': baseline['toolchain'], 'linker': baseline['linker'],
               'linker_version': baseline['linker_version'], 'cargo_lock': q.pin(source / 'Cargo.lock')}
    terminal = ROOT / 'build-candidate.json'
    q.require(not terminal.exists(), 'Retain existing candidate attempt')
    try:
        for index, command in enumerate(commands(), 1):
            script = ROOT / f'candidate-{index}.cmd'
            script.write_text('@echo off\ncall "C:\\Program Files (x86)\\Microsoft Visual Studio\\18\\BuildTools\\Common7\\Tools\\VsDevCmd.bat" -arch=x64 -host_arch=x64 >nul\nif errorlevel 1 exit /b 1\nset CARGO_TARGET_DIR=D:\\search-buffer-reuse-20261010\\target\nset CARGO_BUILD_JOBS=2\nset RUSTFLAGS=\nset CARGO_ENCODED_RUSTFLAGS=\ncd /d D:\\search-buffer-reuse-20261010\\candidate-source\n' + subprocess.list2cmdline(command) + '\nexit /b %errorlevel%\n', encoding='utf-8')
            q.storage()
            started = time.perf_counter()
            log = ROOT / f'candidate-{index}.log'
            with log.open('x', encoding='utf-8') as stream:
                child = subprocess.Popen(['cmd.exe', '/d', '/c', str(script)], stdout=stream, stderr=subprocess.STDOUT)
                while True:
                    try:
                        code = child.wait(timeout=30)
                        break
                    except subprocess.TimeoutExpired:
                        try:
                            q.storage()
                        except Exception:
                            subprocess.run(['taskkill.exe', '/pid', str(child.pid), '/t', '/f'], capture_output=True, check=False)
                            child.wait()
                            raise
            receipt['commands'].append({'argv': command, 'exit_code': code, 'elapsed_seconds': time.perf_counter() - started,
                                        'log': q.pin(log)})
            q.require(code == 0, 'Candidate check/build failed; preserve logs')
        binary = ROOT / 'candidate-regret_census_v1.exe'
        q.require(not subprocess.check_output(['git', '-C', str(source), 'status', '--porcelain', '--untracked-files=all', '--', *runtime_paths], text=True).strip(), 'Candidate runtime source changed during build')
        q.copy_once(ROOT / 'target/release/regret_census_v1.exe', binary)
        fingerprint = q.pin(binary)
        receipt['binary'] = q.copy_once(binary, Path('E:/pinned-binaries') / fingerprint['sha256'] / 'regret_census_v1.exe')
        receipt['ssd_recovery_copy'] = fingerprint
        receipt['complete'] = True
    except Exception as exc:
        receipt['error'] = f'{type(exc).__name__}: {exc}'
    finally:
        receipt['finished_utc'] = q.now()
        q.put(terminal, receipt)
        print(json.dumps(receipt), flush=True)
    q.require(receipt['complete'], receipt.get('error', 'Candidate incomplete'))


def submit(mode):
    log = ROOT / (mode + '-claim.log')
    command = [PYTHON, '-B', str(SLOTS), 'submit', '--log', str(log), '--lane', 'codex-search-buffer-reuse-20261010',
               '--work-id', mode, '--cores', '2', '--cwd', str(q.REPO), '--', PYTHON, '-B', str(Path(__file__).resolve()), mode]
    handle = json.loads(subprocess.check_output(command, text=True))
    q.put(ROOT / (mode + '-handle.json'), handle)
    return handle


def watch():
    q.put(ROOT / 'pipeline-watcher.json', {'pid': os.getpid(), 'created_utc': q.now(), 'poll_seconds': 30,
                                         'deadline_seconds': 43200, 'core_claim_while_waiting': False})
    started = time.monotonic()
    candidate_submitted = measurement_submitted = False
    previous, delay = None, 30
    while time.monotonic() - started < 43200:
        q.require(not (ROOT / 'STOP').exists(), 'Lane STOP file present')
        baseline = ROOT / 'build-baseline.json'
        if baseline.exists():
            q.require(json.loads(baseline.read_text())['complete'], 'Baseline failed; inspect retained receipt')
            if READY.exists() and not candidate_submitted:
                marker = ready()
                source = ROOT / 'candidate-source'
                q.require(not source.exists(), 'Candidate source already exists; inspect before resubmitting')
                subprocess.run(['git', '-C', str(q.REPO), 'worktree', 'add', '--detach', str(source), marker['candidate_commit']], check=True)
                print(json.dumps({'candidate_submitted': submit('candidate')}), flush=True)
                candidate_submitted = True
        candidate_receipt = ROOT / 'build-candidate.json'
        if candidate_submitted and candidate_receipt.exists():
            q.require(json.loads(candidate_receipt.read_text())['complete'], 'Candidate failed; inspect retained receipt')
            marker = ready()
            measurement_terminal = ROOT / 'measurement-terminal.json'
            if measurement_terminal.exists():
                status = json.loads(measurement_terminal.read_text())
                q.require(status['complete'], status.get('error', 'Measurement failed; inspect retained receipt'))
            if (ROOT / 'q001/result.json').exists():
                print(json.dumps({'complete': True, 'result': str(ROOT / 'q001/result.json')}), flush=True)
                return
            if approved_measurement(marker) and not measurement_submitted:
                print(json.dumps({'measurement_submitted': submit('measure')}), flush=True)
                measurement_submitted = True
        state = (baseline.exists(), READY.exists(), candidate_receipt.exists(), MEASURE.exists(),
                 (ROOT / 'q001/result.json').exists(), candidate_submitted, measurement_submitted)
        delay = 30 if state != previous else min(delay * 2, 300)
        previous = state
        time.sleep(delay)
    raise TimeoutError('Watcher deadline reached; handles and all artifacts retained')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('watch', 'candidate', 'measure', 'show-commands'))
    args = parser.parse_args()
    if args.mode == 'show-commands':
        print(json.dumps(commands(), indent=2))
    elif args.mode == 'watch':
        terminal = {'schema': 'search-buffer-reuse-watcher-terminal/v1', 'complete': False}
        try:
            watch()
            terminal['complete'] = True
        except Exception as exc:
            terminal['error'] = f'{type(exc).__name__}: {exc}'
            raise
        finally:
            terminal['finished_utc'] = q.now()
            q.put(ROOT / 'pipeline-watcher-terminal.json', terminal)
    elif args.mode == 'candidate':
        candidate()
    else:
        measure(ready())


if __name__ == '__main__':
    main()
