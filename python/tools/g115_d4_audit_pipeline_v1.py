"""WMI-owned archived learning-signal audit and complete-path qualification.

Production requires qualified signals, exact coverage and allocation evidence.
All copies and failed trees are retained. Only named input members are extracted.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import ctypes
from ctypes import wintypes as W
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time
import zipfile
from g115_d4_audit_storage_v1 import AuditStorage, charge, require, physical_tree
from g115_d4_build_launch_v1 import available_memory
from g115_d4_replay_timing_launch_v1 import pin, checked, read, save, inventory, cpu_seconds
from windows_owned_child_policy_v1 import configure_owned_child

SCHEMA = 'g115-d4-audit-pipeline-qualification/v1'
MATRIX = [1, 4, 8]
REMOTE_ENDPOINTS = {'r4-a', 'r4-b', 'r5-a', 'r5-b'}
LOCAL_ENDPOINTS = {'r1-a', 'r1-b', 'r2-a', 'r2-b', 'r3-a', 'r3-b',
                   'reused-entropy-r1', 'reused-entropy-r2',
                   'reused-balanced-r1', 'reused-balanced-r2', 'pilot-control', 'pilot-broader'}


def name_ok(name):
    return isinstance(name, str) and 0 < len(name) <= 60 and all(c.isalnum() or c in '-_' for c in name)


def canonical(value):
    """Paths necessarily change between fresh phases; all input digests remain."""
    if isinstance(value, list):
        return [canonical(v) for v in value]
    if isinstance(value, dict):
        if set(value) == {'path', 'sha256'}:
            return {'sha256': value['sha256']}
        return {k: canonical(v) for k, v in value.items()}
    return value


def resolve(value, staged):
    if isinstance(value, list):
        return [resolve(v, staged) for v in value]
    if isinstance(value, dict):
        if set(value) == {'path', 'sha256'} and value['path'].startswith('@stage/'):
            require(value['path'] in staged and staged[value['path']]['sha256'] == value['sha256'],
                    'Staged reference absent or changed')
            return staged[value['path']]
        return {k: resolve(v, staged) for k, v in value.items()}
    return value


def limits(plan, work, commands):
    production = plan['mode'] == 'production'
    require(plan['schema'] == SCHEMA and plan['mode'] in ('qualification', 'production'), 'Unknown audit mode')
    require(plan['child_seconds'] == 120 and plan['formal_measurement'] is False, 'Archived audit bounds differ')
    if production:
        require(plan['workers'] in ([4], [8]) and 900 <= plan['total_seconds'] <= 21600
                and 0 < plan['cap_bytes'] <= 60_000_000_000, 'Bounded selected production allocation required')
        require(len(commands) in (800, 2400) and 1 <= len(work['groups']) <= 2800,
                'Full assigned audit shard required')
        require('signal_tool' in plan and 'signals' in work, 'All-record signal audit required')
    else:
        require(plan['workers'] == MATRIX and plan['total_seconds'] == 900
                and 0 < plan['cap_bytes'] <= 4*1024**3, 'Fixed qualification matrix required')
        require(8 <= len(commands) <= 16 and 1 <= len(work['groups']) <= 32, 'Qualification coverage differs')
    names = [g['name'] for g in work['groups']]
    require(len(set(names)) == len(names) and all(name_ok(n) for n in names), 'Invalid input group names')
    total = 0
    for group in work['groups']:
        files = group['files']
        require(1 <= len(files) <= 12, 'Input group size differs')
        names = [f['name'] for f in files]
        require(len(set(names)) == len(names)
                and all(Path(n).name == n and n not in ('.', '..') and ':' not in n for n in names),
                'Input filenames must be unique leaves')
        require(all(type(f['bytes']) is int and 0 < f['bytes'] <= 128*1024**2 for f in files),
                'Explicit bounded input lengths required')
        require(sum(f['bytes'] for f in files) <= 192*1024**2, 'Group raw peak too large')
        total += sum(f['bytes'] for f in files)
    require(total <= (200*1024**3 if production else 1024**3), 'Input volume exceeds audit bound')
    for command in commands:
        require(2 <= len(command['models']) <= 3 and len(command['trajectories']) == 10
                and command['max_choice_rows_per_trajectory'] == 16
                and command['minimum_behavior_replay_rows'] >= 1, 'Scoring workload differs')
    kinds = {c.get('trajectory_kind', 'public_input') for c in commands}
    require(kinds <= {'public_input', 'native_expanded_v3'} and (production or len(kinds) == 2),
            'Unsupported or incomplete trajectory family')


def signal_limits(plan, work, commands, host):
    enabled = 'signal_tool' in plan
    require(enabled == ('signals' in work), 'Signal owner and workload must agree')
    signals = [read(p) for p in work['signals']] if enabled else []
    require(not enabled or len(signals) == len(commands), 'Signal coverage differs')
    for command, signal in zip(commands, signals):
        require(signal['schema'] == 'g115-d4-learning-signal-command/v1'
                and signal['trajectories'] == command['trajectories'], 'Signal/scorer trajectory order differs')
    if plan['mode'] == 'production':
        expected = REMOTE_ENDPOINTS if host == 'haleyspc' else LOCAL_ENDPOINTS
        coverage = [(s['endpoint'], s['update']) for s in signals]
        require(len(coverage) == len(set(coverage)) and set(coverage) == {(e,u) for e in expected for u in range(200)},
                'Missing, duplicate or foreign assigned update')
        require(plan['cap_bytes'] <= (40_000_000_000 if host == 'haleyspc' else 60_000_000_000),
                'Selected host cap exceeded')
        evidence = read(plan['throughput'])
        require(evidence['complete'] and evidence['signal_tool_sha256'] == plan['signal_tool']['sha256']
                and evidence['binary_sha256'] == plan['binary']['sha256']
                and evidence['launcher_sha256'] == plan['documents']['launcher']['sha256']
                and evidence['dependencies'] == plan['dependencies']
                and evidence['host'] == host and evidence['scientific_outputs_verified'] is True,
                'Compatible verified whole-path throughput evidence required')
        phases = evidence['phases']
        require([p['workers'] for p in phases] == MATRIX
                and all(p['seconds'] > 0 and p['choice_rows'] == phases[0]['choice_rows']
                        and p['signal_updates'] == phases[0]['signal_updates'] == 8 for p in phases),
                'Matched serial/parallel complete-work timing required')
        selected = min(phases, key=lambda p:p['seconds'])['workers']
        allocation = read(plan['allocation'])
        require(plan['workers'] == [selected] and allocation['workers'][host] == selected
                and set(allocation['selected_for_production_wiring']['endpoints']) == REMOTE_ENDPOINTS,
                'Workers or shard differ from selected allocation')
        parity = read(plan['cross_host_signal_verification'])
        require(parity['scientific_outputs_equal'] is True
                and parity['signal_tool_sha256'] == plan['signal_tool']['sha256']
                and parity['binary_sha256'] == plan['binary']['sha256'],
                'Cross-host signal parity not verified')
        require(host != 'haleyspc' or bool(plan.get('retained_roots')), 'Transferred archive storage must be charged')
    return signals


def admission(plan, host):
    require(os.name == 'nt' and host in ('jack', 'haleyspc'), 'Windows archive qualification only')
    require(checked(plan['documents']['launcher']).resolve() == Path(__file__).resolve(), 'Wrong owner')
    checked(plan['transport']['dispatcher'])
    expected = {Path(__file__).with_name(n).resolve() for n in
                ['g115_d4_audit_storage_v1.py', 'g115_d4_build_launch_v1.py',
                 'g115_d4_replay_timing_launch_v1.py', 'windows_owned_child_policy_v1.py']}
    require({checked(p).resolve() for p in plan['dependencies']} == expected, 'Helper pins incomplete')
    if 'signal_tool' in plan:
        require(checked(plan['signal_tool']).resolve() == Path(__file__).with_name('g115_d4_learning_signal_v1.py').resolve(),
                'Only the named learning-signal child is admitted')
    binary = checked(plan['binary'])
    build = read(plan['build'])
    require(binary.name == 'public_policy_replay_audit_v1.exe' and build['complete']
            and build['binaries'][binary.stem]['sha256'] == plan['binary']['sha256'], 'Scorer build differs')
    compact = checked(plan['compact'])
    require(compact.name.lower() == 'compact.exe', 'Only named compressor allowed')
    work = read(plan['workload'])
    commands = [read(ref) for ref in work['commands']]
    limits(plan, work, commands)
    signal_limits(plan, work, commands, host)
    indexes = {}
    for group in work['groups']:
        for file in group['files']:
            ref = file['index']
            if ref['sha256'] not in indexes:
                indexes[ref['sha256']] = read(ref)
            archive = indexes[ref['sha256']]
            matches = [s for s in archive['shards'] if s['archive'] == file['archive']]
            require(len(matches) == 1 and matches[0]['files'].get(file['member']) == file['sha256'],
                    'Member is not bound by the archive index')
    root = Path(plan['worker_root'])
    drive = 'e:' if host == 'jack' else 'c:'
    reserve_memory = (32 if host == 'jack' else 8)*1024**3
    require(root.is_absolute() and root.resolve().drive.lower() == drive and not root.exists(),
            'Fresh host-specific root required')
    retained = [Path(p).resolve(strict=True) for p in plan.get('retained_roots', [])]
    require(len(retained) == len(set(retained)) and all(p.drive.lower() == drive and not root.resolve().is_relative_to(p)
            and not p.is_relative_to(root.resolve()) for p in retained), 'Invalid retained tree scope')
    require(all(not a.is_relative_to(b) and not b.is_relative_to(a)
                for i,a in enumerate(retained) for b in retained[i+1:]), 'Retained trees overlap')
    prior = sum(physical_tree(p)['charged_bytes'] for p in retained)
    require(prior < plan['cap_bytes'], 'Retained footprint exhausts cap')
    if plan['mode'] == 'production' and host == 'haleyspc':
        require(all(any(Path(f['archive']['path']).resolve().is_relative_to(p) for p in retained)
                    for g in work['groups'] for f in g['files']), 'Uncharged transferred archive')
    live = inventory(root, reserve_memory, plan['cap_bytes']-prior)
    require(live['computer_name'].lower() == plan['computer_name'].lower(), 'Wrong host')
    return work, commands, binary, compact, live


def execute(plan, work, commands, binary, compact, live, host):
    storage = AuditStorage(plan['worker_root'], plan['cap_bytes'], drive='e:' if host == 'jack' else 'c:',
                           retained_roots=plan.get('retained_roots', []))
    reserve_memory = (32 if host == 'jack' else 8)*1024**3
    started = time.monotonic()
    stop = threading.Event()
    result = {'complete': False, 'phases': [], 'inventory': live, 'formal_measurement': False,
              'host': host, 'launcher_sha256': plan['documents']['launcher']['sha256'],
              'dependencies': plan['dependencies'], 'binary_sha256': plan['binary']['sha256'],
              'signal_tool_sha256': plan.get('signal_tool', {}).get('sha256')}
    reference = None
    signal_commands = signal_limits(plan, work, commands, host)
    signal_reference = None

    def guard():
        require(not stop.is_set() and time.monotonic()-started < plan['total_seconds'], 'Pipeline stopped or timed out')
        require(available_memory() >= reserve_memory, 'Memory reserve reached')
        storage.check()

    def child(command, folder, label, qos=False):
        begin = time.monotonic()
        proc = None
        try:
            guard()
            with (folder/(label+'.stdout')).open('xb') as out, (folder/(label+'.stderr')).open('xb') as err:
                proc = subprocess.Popen(command, stdout=out, stderr=err,
                                        creationflags=subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS)
                scheduling = configure_owned_child(proc, binary) if qos else None
                while True:
                    try:
                        proc.wait(timeout=1)
                        break
                    except subprocess.TimeoutExpired:
                        guard()
                        require(time.monotonic()-begin < 120, 'Owned child time bound reached')
                require(proc.returncode == 0, 'Owned child failed; no retry')
                guard()
                return {'seconds': time.monotonic()-begin, 'cpu_seconds': cpu_seconds(proc),
                        'pid': proc.pid, 'scheduling': scheduling}
        except BaseException:
            stop.set()
            if proc is not None and proc.poll() is None:
                proc.kill()
                proc.wait(timeout=30)
            raise

    def stage(group, workers):
        name = f'w{workers}-input-{group["name"]}'
        raw = sum(f['bytes'] for f in group['files'])
        folder = storage.reserve(name, charge(2*raw+1024**2, len(group['files'])+8))
        begin = time.monotonic()
        succeeded = False
        try:
            for file in group['files']:
                guard()
                with zipfile.ZipFile(file['archive']['path']) as archive:
                    info = archive.getinfo(file['member'])
                    require(info.file_size == file['bytes'], 'ZIP entry length differs')
                    digest = hashlib.sha256()
                    count = 0
                    with archive.open(info) as source, (folder/file['name']).open('xb') as target:
                        while block := source.read(1024**2):
                            count += len(block)
                            require(count <= file['bytes'], 'ZIP extraction exceeded declared length')
                            target.write(block)
                            digest.update(block)
                            if count % (8*1024**2) == 0:
                                guard()
                        target.flush()
                        os.fsync(target.fileno())
                    require(count == file['bytes'] and digest.hexdigest() == file['sha256'], 'Extracted member differs')
            extraction = time.monotonic()-begin
            compression = child([str(compact), '/C', '/A', '/Q', '/EXE:LZX']
                                + [str(folder/f['name']) for f in group['files']], folder, 'compact')
            check_start = time.monotonic()
            staged = {}
            for file in group['files']:
                ref = pin(folder/file['name'])
                require(ref['sha256'] == file['sha256'], 'Compressed readback differs')
                staged[f'@stage/{group["name"]}/{file["name"]}'] = ref
            row = {'name': name, 'extraction_seconds': extraction, 'compression': compression,
                   'readback_seconds': time.monotonic()-check_start, 'staged': staged}
            save(folder/'stage.json', row)
            row['storage'] = storage.seal(name)
            succeeded = True
            return row
        finally:
            if not succeeded:
                stop.set()
                if name in storage.ledger.pending:
                    storage.seal(name, failed=True)

    def score(index, template, workers, staged):
        name = f'w{workers}-score-{index:02}'
        folder = storage.reserve(name, 32*1024**2)
        succeeded = False
        try:
            command = resolve(template, staged)
            command.update(workers=1, output_directory=str(folder/'output'))
            save(folder/'command.json', command)
            timing = child([str(binary), str(folder/'command.json')], folder, 'scorer', qos=True)
            final = json.loads((folder/'output/completion.json').read_bytes())
            require(final['trajectories'] == 10 and final['choice_rows'] > 0
                    and final['exact_behavior_replay_rows'] >= command['minimum_behavior_replay_rows']
                    and final['terminal_outcomes_used'] is False, 'Incomplete saved-tensor audit')
            files = sorted((folder/'output').glob('*.json'))
            require(len(files) == 11, 'Scorer output count differs')
            signatures = {p.name: hashlib.sha256(json.dumps(canonical(json.loads(p.read_bytes())),
                          sort_keys=True, separators=(',', ':')).encode()).hexdigest() for p in files}
            row = {'index': index, 'timing': timing, 'choice_rows': final['choice_rows'],
                   'signatures': signatures, 'files': [pin(p) for p in files]}
            if signal_commands:
                signal = resolve(signal_commands[index], staged)
                signal['output'] = str(folder/'signal.json')
                save(folder/'signal-command.json', signal)
                signal_timing = child([sys.executable, '-B', str(checked(plan['signal_tool'])),
                                       '--command', str(folder/'signal-command.json')], folder, 'signal')
                signal_result = json.loads((folder/'signal.json').read_bytes())
                require(signal_result['complete'] and signal_result['summary']['normalized_statistics_bit_equal'],
                        'Incomplete learning-signal audit')
                row.update(signal=pin(folder/'signal.json'), signal_timing=signal_timing,
                           signal_signature=hashlib.sha256(json.dumps(canonical(signal_result), sort_keys=True,
                                                          separators=(',', ':')).encode()).hexdigest())
            save(folder/'scoring.json', row)
            row['storage'] = storage.seal(name)
            succeeded = True
            return row
        finally:
            if not succeeded:
                stop.set()
                if name in storage.ledger.pending:
                    storage.seal(name, failed=True)
    try:
        save(storage.control/'manifest.json', plan)
        for workers in plan['workers']:
            begin = time.monotonic()
            with ThreadPoolExecutor(max_workers=workers) as pool:
                inputs = list(pool.map(lambda group: stage(group, workers), work['groups']))
            staged = {key: ref for row in inputs for key, ref in row['staged'].items()}
            with ThreadPoolExecutor(max_workers=workers) as pool:
                futures = [pool.submit(score, i, command, workers, staged) for i, command in enumerate(commands)]
                rows = [f.result() for f in futures]
            signatures = [r['signatures'] for r in rows]
            require(reference is None or signatures == reference, 'Combined serial/parallel scoring differs')
            reference = signatures
            signal_signatures = [r.get('signal_signature') for r in rows]
            require(signal_reference is None or signal_signatures == signal_reference, 'Serial/parallel raw signals differ')
            signal_reference = signal_signatures
            seconds = time.monotonic()-begin
            phase = {'workers': workers, 'seconds': seconds, 'inputs': inputs, 'rows': rows,
                     'choice_rows': sum(r['choice_rows'] for r in rows), 'scoring_equal': True,
                     'signal_updates': len(signal_commands), 'signal_equal': True}
            save(storage.control/f'phase-{workers}.json', phase)
            result['phases'].append(phase)
            guard()
        result['complete'] = True
    except BaseException as error:
        stop.set()
        result['error'] = repr(error)
        raise
    finally:
        result.update(seconds=time.monotonic()-started, charged_bytes=storage.ledger.committed,
                      pending=storage.ledger.pending)
        save(storage.control/'completion.json', result)
        storage.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', required=True)
    parser.add_argument('--host', required=True)
    parser.add_argument('--root')
    parser.add_argument('--check-only', action='store_true')
    args = parser.parse_args()
    plan = json.loads(Path(args.manifest).read_bytes())
    work, commands, binary, compact, live = admission(plan, args.host)
    if args.check_only:
        print(json.dumps({'admitted': True, 'native_started': False, 'inventory': live}))
        return
    require(args.root is not None and Path(args.root).resolve() == Path(plan['worker_root']).resolve(),
            'Worker root differs')
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.CreateMutexW.argtypes = [ctypes.c_void_p, W.BOOL, W.LPCWSTR]
    kernel.CreateMutexW.restype = W.HANDLE
    kernel.CloseHandle.argtypes = [W.HANDLE]
    handle = kernel.CreateMutexW(None, False, 'Local\\mtg-g115-d4-bounded-timing-v1')
    require(bool(handle), 'Timing mutex creation failed')
    try:
        require(ctypes.get_last_error() != 183, 'D4 owner already present')
        execute(plan, work, commands, binary, compact, live, args.host)
    finally:
        kernel.CloseHandle(handle)


if __name__ == '__main__':
    main()
