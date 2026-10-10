"""Run one prepared comparison case through the supported reserved dispatcher.

This does not create requests, qualify an allocation, retry, or prune artifacts.
Its controller clock includes observation latency; use dispatch/child clocks
for measured throughput. Run independently on the actual target host.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import importlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def pin(path):
    path = Path(path)
    with path.open('rb') as stream:
        return {'path': str(path), 'sha256': hashlib.file_digest(stream, 'sha256').hexdigest()}


def save(path, value):
    with Path(path).open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write('\n')


def wait_for_release(reservations, dispatched, deadline):
    """Observe the owned supervisor's exit without leaving a polling gap."""
    token = dispatched['token']
    state = reservations.status(token)
    handle = None
    if os.name == 'nt' and state.get('token_fate') == 'holds':
        # The immutable lock owner is the launcher. Handoff/adopt events,
        # projected by status(), identify the actual supervisor instead.
        creations = {row['creation_time'] for row in state.get('processes', {}).values()
                     if row['pid'] == dispatched['pid'] and row['state'] == 'alive'}
        if len(creations) == 1:
            handle = reservations._OpenProcess(
                reservations.PROCESS_QUERY_LIMITED_INFORMATION | reservations.SYNCHRONIZE,
                False, dispatched['pid'])
            if handle and reservations._creation_of(handle) != next(iter(creations)):
                reservations._CloseHandle(handle)
                handle = None
    checks = 0
    try:
        while True:
            fate = state.get('token_fate', 'unknown')
            if fate.startswith('released'):
                return fate
            if fate != 'holds' or time.monotonic() > deadline:
                raise RuntimeError('case needs investigation: ' + fate)
            if handle:
                # The dispatcher already owns runtime/storage supervision.
                # This waits for completion, then verifies canonical release.
                while time.monotonic() < deadline:
                    result = reservations._WaitForSingleObject(handle, 60_000)
                    if result == reservations.WAIT_OBJECT_0:
                        break
                    if result != reservations.WAIT_TIMEOUT:
                        raise RuntimeError('cannot wait for owned supervisor')
                state = reservations.status(token)
                if state.get('token_fate') == 'holds':
                    raise RuntimeError('supervisor ended or deadline elapsed without canonical release')
            else:
                time.sleep(30 if checks < 2 else 60)
                checks += 1
                state = reservations.status(token)
    finally:
        if handle:
            reservations._CloseHandle(handle)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--launcher-root', type=Path, required=True)
    parser.add_argument('--request', type=Path, required=True)
    parser.add_argument('--action', choices=['qualify', 'dispatch'], required=True)
    parser.add_argument('--receipt', type=Path, required=True)
    parser.add_argument('--compute-host-name')
    parser.add_argument('--declare-desktop-cores', action='store_true')
    args = parser.parse_args()
    if args.receipt.exists():
        raise ValueError('preserve existing case receipt; choose a new attempt')
    request = json.loads(args.request.read_bytes())
    root = Path(request['root'])
    if root.exists() or Path(request['cold_root']).exists():
        raise ValueError('case roots already exist; no automatic retries')
    tools = args.launcher_root / 'python/tools'
    sys.path.insert(0, str(tools))
    dispatch = importlib.import_module('native_expanded_dispatch_v1')
    # The existing launcher dual-uses one projection for logical and physical
    # accounting. This comparison supplies measured physical growth there and
    # additionally enforces the full logical growth bound immediately before
    # launch. Neither the disk reserve nor the logical allowance is relaxed.
    logical_projection = request['comparison_logical_projection_bytes']
    if type(logical_projection) is not int or logical_projection <= 0:
        raise ValueError('positive logical artifact projection required')
    observed = dispatch.validate_storage(request['storage'])
    if observed['logical_bytes'] + logical_projection > request['storage']['max_logical_bytes']:
        raise ValueError('current plus projected logical artifacts exceed allowance')
    spec = importlib.util.spec_from_file_location('case_reservation', tools / 'host_reservation_v1.py')
    reservations = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = reservations
    spec.loader.exec_module(reservations)
    command = [sys.executable, '-B', str(tools / 'native_expanded_dispatch_v1.py'),
               args.action, str(args.request)]
    adapter_pin = None
    if args.declare_desktop_cores:
        if request['placement']['host'] != 'desktop':
            raise ValueError('desktop core declaration adapter is requires desktop placement')
        adapter = Path(__file__).with_name('formal_launcher_adapter.py')
        adapter_pin = pin(adapter)
        command = [sys.executable, '-B', str(adapter), '--launcher-root', str(args.launcher_root),
                   '--cores', ','.join(map(str, request['placement']['cpu_affinity'])),
                   args.action, str(args.request)]
    if args.compute_host_name:
        command += ['--compute-host-name', args.compute_host_name]
    began = time.monotonic()
    receipt = {'schema': 'training-speedup-case/v1', 'action': args.action,
               'request': pin(args.request), 'launcher': pin(tools / 'native_expanded_dispatch_v1.py'),
               'launcher_adapter': adapter_pin,
               'controller_source': pin(__file__),
               'started_utc': datetime.now(timezone.utc).isoformat(),
               'logical_preflight': {'current_bytes': observed['logical_bytes'],
                                     'additional_bytes': logical_projection,
                                     'cap_bytes': request['storage']['max_logical_bytes']}}
    try:
        child = subprocess.run(command, text=True, capture_output=True, check=True)
        dispatched = json.loads(child.stdout.strip().splitlines()[-1])
        receipt['dispatch'] = {k: dispatched[k] for k in
                               ('state', 'pid', 'generation', 'nested') if k in dispatched}
        if dispatched.get('nested') or dispatched['state'] == 'spawn-unconfirmed':
            raise ValueError('case driver requires confirmed standalone reservation')
        deadline = began + request['wall_seconds'] + 1800
        receipt['reservation_fate'] = wait_for_release(reservations, dispatched, deadline)
        report = root / 'report.json'
        value = json.loads(report.read_bytes())
        if not value['complete']:
            raise ValueError('case did not complete')
        execution_path = Path(value['execution']['path'])
        if pin(execution_path) != value['execution']:
            raise ValueError('execution receipt changed')
        execution = json.loads(execution_path.read_bytes())
        if execution['exit_code'] != 0 or execution['error'] is not None:
            raise ValueError('native execution failed')
        receipt.update(complete=True, report=pin(report), dispatch_seconds=value['seconds'],
                       child_seconds=execution.get('child_seconds'),
                       completed_games=value['completed_games'],
                       completed_updates=value['completed_updates'])
    except Exception as error:
        receipt.update(complete=False, error=f'{type(error).__name__}: {error}')
        if isinstance(error, subprocess.CalledProcessError):
            receipt['stderr'] = error.stderr[-4000:]
        raise
    finally:
        receipt['controller_seconds_including_observation'] = time.monotonic() - began
        receipt['finished_utc'] = datetime.now(timezone.utc).isoformat()
        save(args.receipt, receipt)
        print(json.dumps(receipt, allow_nan=False))


if __name__ == '__main__':
    main()
