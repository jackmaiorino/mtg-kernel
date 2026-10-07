"""Bounded archived-tensor timing matrix, admitted by the named WMI transport.

This is qualification only. It cannot dispatch a production audit or training.
The output cap is checked periodically, not enforced as an OS filesystem quota.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import copy
import ctypes
from ctypes import wintypes as W
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import threading
import time
from g115_d4_build_launch_v1 import available_memory
from windows_owned_child_policy_v1 import configure_owned_child

GIB = 1024**3
MATRIX = [[1, 1], [1, 2], [1, 4], [1, 8], [4, 1], [8, 1]]


def require(ok, message):
    if not ok:
        raise ValueError(message)


def pin(path):
    path = Path(path)
    with path.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    return {'path': str(path), 'sha256': digest}


def checked(ref):
    require(pin(ref['path'])['sha256'] == ref['sha256'], 'Pinned input changed')
    return Path(ref['path'])


def read(ref):
    return json.loads(checked(ref).read_bytes())


def save(path, value):
    temporary = path.with_suffix('.partial')
    with temporary.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)


def limits(plan, host, commands):
    require(plan['schema'] == 'g115-d4-replay-timing/v1' and host in ('desktop', 'computehost'),
            'Named Windows replay timing only')
    require(plan['matrix'] == MATRIX and plan['job_seconds'] == 120
            and plan['total_seconds'] == 900, 'Fixed timing matrix and bounds required')
    require(plan['formal_measurement'] is False, 'Qualification only')
    require(0 < plan['output_cap_bytes'] <= GIB, 'Output allowance must be at most 1GiB')
    require(8 <= len(commands) <= 16, 'Eight to sixteen representative commands required')
    kinds = set()
    for command in commands:
        kinds.add(command.get('trajectory_kind', 'public_input'))
        require(2 <= len(command['models']) <= 4 and 8 <= len(command['trajectories']) <= 16,
                'Timing requires 2..4 models and 8..16 trajectories per command')
        require(2 <= command['max_choice_rows_per_trajectory'] <= 64
                and command['minimum_behavior_replay_rows'] >= 1,
                'Bounded choice rows and behavior replay required')
        require(len({x['sha256'] for x in command['trajectories']}) == len(command['trajectories']),
                'Duplicate trajectories cannot inflate completed work')
    require(kinds == {'public_input', 'native_expanded_v3'}, 'Both archive families required')


def inventory(root, reserve, allowance):
    command = "@{computer_name=$env:COMPUTERNAME; logical_cpus=[Environment]::ProcessorCount; physical_cores=(@(Get-CimInstance Win32_Processor | ForEach-Object NumberOfCores) | Measure-Object -Sum).Sum; competing=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -match '^(public_.*|native_.*|learned_sideboard_v1|trainer|mtg_kernel.*|cargo|rustc)\\.exe$'} | Select-Object ProcessId,Name)} | ConvertTo-Json -Depth 4 -Compress"
    result = json.loads(subprocess.check_output(
        ['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', command],
        text=True, timeout=30, creationflags=subprocess.CREATE_NO_WINDOW))
    result.update(memory_available=available_memory(), disk_free=shutil.disk_usage(root.parent).free)
    require(not result['competing'], 'Competing native work present')
    require(result['memory_available'] >= reserve and result['disk_free'] >= 60*GIB+allowance,
            'Resource reserve plus output allowance unavailable')
    return result


def admission(plan, host):
    require(os.name == 'nt', 'Windows required')
    commands = [read(ref) for ref in plan['commands']]
    limits(plan, host, commands)
    require(checked(plan['documents']['launcher']).resolve() == Path(__file__).resolve(), 'Wrong launcher')
    checked(plan['transport']['dispatcher'])
    expected = {Path(__file__).with_name(name).resolve() for name in
                ('g115_d4_build_launch_v1.py', 'windows_owned_child_policy_v1.py')}
    require({checked(ref).resolve() for ref in plan['dependencies']} == expected, 'Helper pins incomplete')
    binary = checked(plan['binary'])
    require(binary.name == 'public_policy_replay_audit_v1.exe', 'Only archived scorer allowed')
    build = read(plan['build'])
    require(build['complete'] and build['exit_code'] == 0
            and build['binaries']['public_policy_replay_audit_v1']['sha256'] == plan['binary']['sha256'],
            'Successful matching build required')
    # Validate every explicit input pin, including nested model/config references.
    def pins(value):
        if isinstance(value, dict):
            if set(value) == {'path', 'sha256'}:
                checked(value)
            else:
                for item in value.values():
                    pins(item)
        elif isinstance(value, list):
            for item in value:
                pins(item)
    for command in commands:
        pins(command)
    requested_root = Path(plan['worker_root'])
    root = requested_root.resolve()
    expected_drive = 'e:' if host == 'desktop' else 'c:'
    require(requested_root.is_absolute() and root.drive.lower() == expected_drive and not root.exists()
            and root.parent.is_dir(), 'Fresh host-specific evidence drive root required')
    reserve = (32 if host == 'desktop' else 8)*GIB
    live = inventory(root, reserve, plan['output_cap_bytes'])
    require(live['computer_name'].lower() == plan['computer_name'].lower(), 'Wrong computer')
    return commands, binary, root, reserve, live


def cpu_seconds(child):
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.GetProcessTimes.argtypes = [W.HANDLE] + [ctypes.POINTER(W.FILETIME)]*4
    kernel.GetProcessTimes.restype = W.BOOL
    values = [W.FILETIME() for _ in range(4)]
    require(kernel.GetProcessTimes(W.HANDLE(int(child._handle)), *(ctypes.byref(v) for v in values)),
            'Process CPU telemetry failed')
    return sum((v.dwHighDateTime << 32) + v.dwLowDateTime for v in values[2:])/10_000_000


def run(plan, commands, binary, root, reserve, live):
    root.mkdir()
    started = time.monotonic()
    result = {'complete': False, 'inventory': live, 'phases': [], 'manifest': plan,
              'formal_measurement': False}
    reference = None
    stop = threading.Event()
    guard_lock = threading.Lock()
    def guard():
        require(not stop.is_set() and time.monotonic()-started < 900, 'Timing stopped or time limit reached')
        with guard_lock:
            size = 0
            for path in root.rglob('*'):
                try:
                    if path.is_file():
                        size += path.stat().st_size
                except FileNotFoundError:
                    pass  # An owned atomic publication can rename a .partial file.
            require(size < plan['output_cap_bytes'] and shutil.disk_usage(root).free >= 60*GIB
                    and available_memory() >= reserve, 'Output cap or resource reserve reached')

    def execute(index, template, folder, threads):
        guard()
        job = folder/f'job-{index:02}'
        job.mkdir()
        config = copy.deepcopy(template)
        config.update(workers=threads, output_directory=str(job/'output'))
        save(job/'command.json', config)
        row = {'index': index, 'complete': False}
        child = None
        begin = time.monotonic()
        try:
            with (job/'stdout.log').open('xb') as out, (job/'stderr.log').open('xb') as err:
                child = subprocess.Popen([str(binary), str(job/'command.json')], stdout=out, stderr=err,
                                         creationflags=subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS)
                row['scheduling'] = configure_owned_child(child, binary)
                row['samples'] = []
                while True:
                    guard()
                    require(time.monotonic()-begin < 120, 'Per-command timing bound reached')
                    try:
                        child.wait(timeout=1)
                        break
                    except subprocess.TimeoutExpired:
                        row['samples'].append({'seconds': time.monotonic()-begin, 'cpu_seconds': cpu_seconds(child)})
                row['cpu_seconds'] = cpu_seconds(child)
                require(child.returncode == 0, 'Scorer failed; no automatic retry')
            completion = json.loads((job/'output/completion.json').read_bytes())
            require(completion['terminal_outcomes_used'] is False
                    and completion['trajectories'] == len(config['trajectories'])
                    and completion['exact_behavior_replay_rows'] >= config['minimum_behavior_replay_rows']
                    and completion['choice_rows'] > 0, 'Incomplete archived replay')
            files = sorted((job/'output').glob('*.json'))
            require(len(files) == len(config['trajectories'])+1, 'Unexpected scorer output count')
            row.update(complete=True, choice_rows=completion['choice_rows'],
                       hashes={p.name: pin(p)['sha256'] for p in files})
            guard()
            return row
        except BaseException as error:
            stop.set()
            row['error'] = repr(error)
            if child is not None and child.poll() is None:
                child.kill()
                child.wait(timeout=30)
            raise
        finally:
            row['seconds'] = time.monotonic()-begin
            save(job/'execution.json', row)
    try:
        for processes, threads in MATRIX:
            guard()
            folder = root/f'processes-{processes}-threads-{threads}'
            folder.mkdir()
            begin = time.monotonic()
            with ThreadPoolExecutor(max_workers=processes) as pool:
                futures = [pool.submit(execute, i, c, folder, threads) for i, c in enumerate(commands)]
                rows = [future.result() for future in futures]
            hashes = [row['hashes'] for row in rows]
            require(reference is None or hashes == reference, 'Serial/parallel scorer bytes differ')
            reference = hashes
            seconds = time.monotonic()-begin
            count = sum(row['choice_rows'] for row in rows)
            phase = {'processes': processes, 'threads': threads, 'seconds': seconds, 'choice_rows': count,
                     'rows_per_second': count/seconds, 'serial_bytes_equal': True, 'rows': rows}
            save(folder/'completion.json', phase)
            result['phases'].append(phase)
        guard()
        result['complete'] = True
    except BaseException as error:
        result['error'] = repr(error)
        raise
    finally:
        result['seconds'] = time.monotonic()-started
        save(root/'completion.json', result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', required=True)
    parser.add_argument('--host', required=True)
    parser.add_argument('--root')
    parser.add_argument('--check-only', action='store_true')
    args = parser.parse_args()
    plan = json.loads(Path(args.manifest).read_bytes())
    commands, binary, root, reserve, live = admission(plan, args.host)
    if args.check_only:
        print(json.dumps({'admitted': True, 'inventory': live, 'native_started': False}))
        return
    require(args.root is not None and Path(args.root).resolve() == root, 'Worker root differs')
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.CreateMutexW.argtypes = [ctypes.c_void_p, W.BOOL, W.LPCWSTR]
    kernel.CreateMutexW.restype = W.HANDLE
    kernel.CloseHandle.argtypes = [W.HANDLE]
    handle = kernel.CreateMutexW(None, False, 'Local\\mtg-g115-d4-bounded-timing-v1')
    require(bool(handle), 'Timing mutex creation failed')
    try:
        require(ctypes.get_last_error() != 183, 'D4 timing owner already present')
        run(plan, commands, binary, root, reserve, live)
    finally:
        kernel.CloseHandle(handle)


if __name__ == '__main__':
    main()
