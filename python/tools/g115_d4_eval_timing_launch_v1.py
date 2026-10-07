"""Fixed D4 evaluation timing matrix. Invoke only through the named WMI transport."""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
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
from g115_d4_eval_timing_contract_v1 import validate_panel
from windows_owned_child_policy_v1 import configure_owned_child


def pin(path):
    path = Path(path)
    return dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest())


def checked(item):
    if pin(item['path'])['sha256'] != item['sha256']:
        raise ValueError('pinned input changed')
    return Path(item['path'])


def read_pin(item):
    return json.loads(checked(item).read_bytes())


def write(path, value):
    path = Path(path)
    temp = path.with_suffix(path.suffix + '.partial')
    with temp.open('x') as out:
        json.dump(value, out, indent=2, allow_nan=False)
        out.flush()
        os.fsync(out.fileno())
    os.replace(temp, path)


def validate_limits(plan, host):
    if plan.get('schema') != 'g115-d4-bounded-eval-timing/v1' or host not in ('desktop', 'computehost'):
        raise ValueError('bounded Windows evaluation timing only')
    if plan.get('workers') != [1, 4, 8] or plan.get('job_seconds') != 180 or plan.get('total_seconds') != 3600:
        raise ValueError('fixed scaling matrix and time limits required')
    if plan.get('formal_measurement') is not False:
        raise ValueError('timing cannot be formal measurement')


def admission(plan, host):
    validate_limits(plan, host)
    if checked(plan['documents']['launcher']).resolve() != Path(__file__).resolve():
        raise ValueError('wrong launcher')
    checked(plan['transport']['dispatcher'])
    expected = {Path(__file__).with_name(n).resolve() for n in
                ['g115_d4_eval_timing_contract_v1.py', 'windows_owned_child_policy_v1.py']}
    if {checked(x).resolve() for x in plan['dependencies']} != expected:
        raise ValueError('helper pins incomplete')
    workload = read_pin(plan['workload'])
    if checked(workload['contract']).resolve() not in expected:
        raise ValueError('workload contract differs')
    if checked(workload['binary']).name != 'learned_sideboard_v1.exe':
        raise ValueError('unsupported native evaluator')
    configs = [read_pin(j['template']) for j in workload['jobs']]
    validate_panel(workload, configs)
    for source in [*workload['sources'].values(), workload['opponent']]:
        read_pin(source['checkpoint'])
        descriptor = read_pin(source['play_import'])
        for key in ('initialization', 'parameters'):
            checked(descriptor[key])
    place = plan['hosts'][host]
    if place['computer_name'].lower() != os.environ.get('COMPUTERNAME', '').lower():
        raise ValueError('wrong host identity')
    if place['reserve_memory'] < (32 if host == 'desktop' else 8)*1024**3 or place['reserve_disk'] < 60*1024**3:
        raise ValueError('reserve lowered')
    root = Path(place['worker_root'])
    if not root.is_absolute() or root.exists() or not root.parent.is_dir():
        raise ValueError('fresh absolute worker root required')
    return workload, configs, place


def inventory(place):
    if os.name != 'nt':
        raise ValueError('Windows required')
    command = "$ErrorActionPreference='Stop'; $o=Get-CimInstance Win32_OperatingSystem; $p=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -match '^(learned_sideboard_v1|native_expanded_training_run_v1|public_feature_evaluation_v1|public_feature_training_v1|public_stack_training_v1|trainer|cargo|rustc|mtg_kernel.*)\\.exe$'}); [pscustomobject]@{free_memory_bytes=([long]$o.FreePhysicalMemory*1024);logical_cpus=[Environment]::ProcessorCount;active=@($p | Select-Object ProcessId,Name)} | ConvertTo-Json -Compress -Depth 4"
    value = json.loads(subprocess.check_output(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', command], text=True, creationflags=subprocess.CREATE_NO_WINDOW, timeout=30))
    value['free_disk_bytes'] = shutil.disk_usage(Path(place['worker_root']).parent).free
    if value['active'] or value['free_memory_bytes'] < place['reserve_memory'] or value['free_disk_bytes'] < place['reserve_disk']:
        raise ValueError('competing native work or reserve refusal')
    return value


def cpu_seconds(child):
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.GetProcessTimes.argtypes = [W.HANDLE] + [ctypes.POINTER(W.FILETIME)]*4
    kernel.GetProcessTimes.restype = W.BOOL
    fields = [W.FILETIME() for _ in range(4)]
    if not kernel.GetProcessTimes(W.HANDLE(int(child._handle)), *(ctypes.byref(x) for x in fields)):
        raise ctypes.WinError(ctypes.get_last_error())
    return sum((x.dwHighDateTime << 32) + x.dwLowDateTime for x in fields[2:])/10_000_000


def audit(config, binary):
    root = Path(config['output_directory'])
    final = json.loads((root/'completion.json').read_bytes())
    if final['completed_matches'] != 7 or final['no_training_performed'] is not True or len(list(root.glob('match-*.json'))) != 7:
        raise ValueError('incomplete evaluation batch')
    start = json.loads((root/'run-start.json').read_bytes())
    if start['binary']['sha256'] != binary['sha256']:
        raise ValueError('wrong executed binary')
    checkpoints = [read_pin(s['checkpoint']) for s in config['model_sources']]
    stores = []
    for i, match in enumerate(config['matches']):
        path = root/f'match-{i:06}.json'
        row = json.loads(path.read_bytes())
        if row['config'] != match['config'] or row.get('search_usage') is not None or row['seat_generations'] != ['v4', 'v4']:
            raise ValueError('match configuration or generation differs')
        if not 2 <= len(row['games']) <= 6 or row['outcome'] not in ('draw', {'winner': {'winner': 0}}, {'winner': {'winner': 1}}):
            raise ValueError('missing terminal match result')
        for s, source in enumerate(config['model_sources']):
            model = row['play_models'][s]
            if any(model[k] != checkpoints[s][k] for k in ('state_sha256', 'source_import')) or model['checkpoint_sha256'] != source['checkpoint']['sha256']:
                raise ValueError('wrong evaluation model')
            if {k:row['explicit_registrations'][s][k] for k in ('label', 'mainboard', 'sideboard')} != match['registered'][s]:
                raise ValueError('wrong registration')
        if any(x['selected_actions'] != [{'kind': 'done'}] for x in row['sideboard_decisions']):
            raise ValueError('sideboard policy changed')
        stores.append(pin(path))
    return stores


def execute(root, job, template, workload, deadline, stop):
    if stop.is_set() or time.monotonic()+180 > deadline:
        stop.set()
        raise ValueError('matrix stopped or insufficient remaining bound')
    folder = root/job['name']
    folder.mkdir()
    (folder/'temp').mkdir()
    config = copy.deepcopy(template)
    config['output_directory'] = str(folder/'outputs')
    write(folder/'config.json', config)
    started = time.monotonic()
    row = dict(name=job['name'], complete=False, samples=[])
    child = None
    try:
        with (folder/'stdout.log').open('xb') as out, (folder/'stderr.log').open('xb') as err:
            child = subprocess.Popen([str(checked(workload['binary'])), '--config', str(folder/'config.json')], stdout=out, stderr=err, env=dict(os.environ, TEMP=str(folder/'temp'), TMP=str(folder/'temp')), creationflags=subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS)
            row['pid'] = child.pid
            row['scheduling'] = configure_owned_child(child, checked(workload['binary']))
            write(folder/'process.json', row['scheduling'])
            while child.poll() is None:
                row['samples'].append(dict(epoch=time.time(), cpu_seconds=cpu_seconds(child)))
                if stop.is_set() or time.monotonic()-started > 180 or time.monotonic() > deadline:
                    raise TimeoutError('bounded matrix stopped')
                try:
                    child.wait(timeout=1)
                except subprocess.TimeoutExpired:
                    pass
            row['cpu_seconds'] = cpu_seconds(child)
            row['exit_code'] = child.returncode
            row['native_seconds'] = time.monotonic()-started
            if child.returncode != 0:
                raise ValueError('native evaluation failure; no retry')
        audit_started = time.monotonic()
        row['stores'] = audit(config, workload['binary'])
        row['audit_seconds'] = time.monotonic()-audit_started
        row['complete'] = True
        return row
    except BaseException as error:
        stop.set()
        row['error'] = repr(error)
        if child is not None and child.poll() is None:
            child.kill()
            child.wait()
        raise
    finally:
        row['wall_seconds'] = time.monotonic()-started
        write(folder/'execution.json', row)


def run(plan, workload, configs, place):
    root = Path(place['worker_root'])
    root.mkdir()
    started = time.monotonic()
    result = dict(complete=False, phases=[], formal_measurement=False)
    references = None
    try:
        for workers in plan['workers']:
            live = inventory(place)
            folder = root/f'workers-{workers}'
            folder.mkdir()
            write(folder/'inventory.json', live)
            phase_started = time.monotonic()
            stop = threading.Event()
            rows = []
            with ThreadPoolExecutor(max_workers=workers) as pool:
                futures = [pool.submit(execute, folder, job, config, workload, started+3600, stop) for job, config in zip(workload['jobs'], configs)]
                for future in as_completed(futures):
                    rows.append(future.result())
            hashes = {r['name']:[x['sha256'] for x in r['stores']] for r in rows}
            if references is not None and hashes != references:
                raise ValueError('serial/parallel raw match bytes differ')
            references = hashes
            phase = dict(workers=workers, seconds=time.monotonic()-phase_started, matches=630, rows=sorted(rows, key=lambda r:r['name']), serial_bytes_equal=True)
            write(folder/'completion.json', phase)
            result['phases'].append(phase)
        result['complete'] = True
    except BaseException as error:
        result['error'] = repr(error)
        raise
    finally:
        result['seconds'] = time.monotonic()-started
        result['scope'] = 'Timing only; no strength aggregate, cloud qualification or promotion.'
        write(root/'completion.json', result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--host', choices=['desktop', 'computehost'], required=True)
    parser.add_argument('--root', type=Path)
    parser.add_argument('--check-only', action='store_true')
    args = parser.parse_args()
    plan = json.loads(args.manifest.read_bytes())
    workload, configs, place = admission(plan, args.host)
    live = inventory(place)
    if args.check_only:
        print(json.dumps(dict(admitted=True, inventory=live, native_started=False)))
        return
    if args.root is None or args.root.resolve() != Path(place['worker_root']).resolve():
        raise ValueError('worker root differs')
    # Share the training timing mutex within this Windows session.
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.CreateMutexW.argtypes = [ctypes.c_void_p, W.BOOL, W.LPCWSTR]
    kernel.CreateMutexW.restype = W.HANDLE
    kernel.CloseHandle.argtypes = [W.HANDLE]
    kernel.CloseHandle.restype = W.BOOL
    handle = kernel.CreateMutexW(None, False, 'Local\\mtg-g115-d4-bounded-timing-v1')
    if not handle:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        if ctypes.get_last_error() == 183:
            raise ValueError('D4 timing owner already present')
        run(plan, workload, configs, place)
    finally:
        kernel.CloseHandle(handle)


if __name__ == '__main__':
    main()
