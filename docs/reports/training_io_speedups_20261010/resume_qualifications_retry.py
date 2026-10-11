"""Run the unchanged eight qualifications once, preserving the failed first attempt.

Only the first controller receipt path changes. Read-only reservation/process
waiting precedes the unchanged public driver; its final guard remains authoritative.
No acquisition, exemptions, raw native execution, retry or cleanup occurs here.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

DEFAULT_ROOT = Path('D:/training-io-speedups-20261010/desktop')
PLAN_SHA = 'ea5b941aed34d997765abd43e1411bef60ab15c78f252bb2ae5b4903e04face2'
PRIOR_PID = 37480
LIMIT_SECONDS = 6 * 3600


def require(ok, message):
    if not ok:
        raise ValueError(message)


def now():
    return datetime.now(timezone.utc).isoformat()


def pin(path):
    spelling = str(path)
    with Path(path).open('rb') as stream:
        return {'path': spelling, 'sha256': hashlib.file_digest(stream, 'sha256').hexdigest()}


def checked(ref):
    require(pin(ref['path']) == ref, 'frozen source changed: ' + ref['path'])
    return json.loads(Path(ref['path']).read_bytes())


def load(name, path, tools=None):
    previous = list(sys.path)
    existing_modules = set(sys.modules)
    if tools is not None:
        sys.path.insert(0, str(tools))
    try:
        spec = importlib.util.spec_from_file_location(name, path)
        module = importlib.util.module_from_spec(spec)
        sys.modules[name] = module
        spec.loader.exec_module(module)
        return module
    finally:
        sys.path[:] = previous
        # Dependencies retain their objects in the loaded module, while the
        # next variant imports its own frozen versions under ordinary names.
        if tools is not None:
            for key in set(sys.modules) - existing_modules - {name}:
                file = getattr(sys.modules[key], '__file__', None)
                if file and Path(file).is_relative_to(tools):
                    del sys.modules[key]


def remaining(deadline):
    seconds = deadline - time.monotonic()
    require(seconds > 0, 'six-hour coordinator bound reached; no further launch')
    return seconds


def await_admission(reservations, dispatch, binary, watcher_type, deadline, changed):
    """Arm one directory watch and wait on same-handle process identities."""
    began = time.monotonic()
    watcher = watcher_type(reservations.lock_path().parent)
    last = None
    fallbacks = 0
    try:
        while True:
            remaining(deadline)
            status = reservations.status()
            compact = {'canonical_state': status.get('state'),
                       'generation': (status.get('record') or {}).get('generation'),
                       'lane': (status.get('record') or {}).get('lane')}
            require(compact['canonical_state'] in ('free', 'held'), 'unknown canonical reservation state')
            if compact['canonical_state'] != 'free':
                if compact != last:
                    changed(compact); last = compact
                refresh = time.monotonic() + 600
                while True:
                    signaled = watcher.wait(min(60000, max(1, int(remaining(deadline) * 1000))))
                    if signaled or time.monotonic() >= refresh:
                        break
                continue
            busy = reservations.busy_processes(dispatch.busy_pattern(binary), [])
            identities = []
            for row in busy:
                pid = int(row['ProcessId'])
                identities.append({'pid': pid, 'name': row['Name'],
                                   'creation_time': reservations.creation_time(pid)})
            compact['busy_processes'] = identities
            if compact != last:
                changed(compact); last = compact
            if not busy:
                # Both observations are advisory; atomic acquisition/busy refusal
                # remain inside the frozen public dispatcher.
                if reservations.status().get('state') == 'free':
                    return {'seconds': time.monotonic() - began, 'final': compact}
                continue
            handle = None
            try:
                for identity in identities:
                    if identity['creation_time'] is None:
                        continue
                    candidate = reservations._OpenProcess(
                        reservations.PROCESS_QUERY_LIMITED_INFORMATION | reservations.SYNCHRONIZE,
                        False, identity['pid'])
                    if candidate and reservations._creation_of(candidate) == identity['creation_time']:
                        handle = candidate
                        break
                    if candidate:
                        reservations._CloseHandle(candidate)
                if handle:
                    result = reservations._WaitForSingleObject(
                        handle, min(60000, max(1, int(remaining(deadline) * 1000))))
                    require(result in (reservations.WAIT_OBJECT_0, reservations.WAIT_TIMEOUT),
                            'verified busy-process handle wait failed')
                    fallbacks = 0
                else:
                    time.sleep(min(30 if fallbacks < 2 else 60, remaining(deadline)))
                    fallbacks += 1
            finally:
                if handle:
                    reservations._CloseHandle(handle)
    finally:
        watcher.close()


def prepare(root, plan, sources):
    labels = [f'qual-{variant}-w{workers}' for variant in ('baseline', 'candidate') for workers in (1, 2, 4, 8)]
    require(plan['stage'] == 'qualify' and [entry['label'] for entry in plan['commands']] == labels,
            'exact frozen eight-case qualification order required')
    commands = []
    for index, entry in enumerate(plan['commands']):
        variant = entry['variant']
        request = checked(entry['request'])
        sources.extend([entry['request'], request['config'], request['runtime']])
        checked(request['config']); checked(request['runtime'])
        require(request['runtime'] == plan['runtimes'][variant] and request['kind'] == 'training'
                and request['placement']['host'] == 'desktop', 'qualification runtime/host differs')
        require(not Path(request['root']).exists() and not Path(request['cold_root']).exists(),
                'preserve existing qualification hot/cold roots')
        argv = list(entry['argv'])
        expected_receipt = root / 'controllers' / (entry['label'] + '.json')
        require(argv[0] == sys.executable and argv[1] == '-B'
                and Path(argv[2]) == root / 'formal_case_driver.py'
                and Path(argv[argv.index('--launcher-root') + 1]) == root / variant
                and argv[argv.index('--request') + 1] == entry['request']['path']
                and argv[argv.index('--action') + 1] == 'qualify'
                and Path(argv[argv.index('--receipt') + 1]) == expected_receipt
                and '--declare-desktop-cores' in argv, 'frozen public driver command differs')
        if index == 0:
            require(checked(pin(expected_receipt)).get('complete') is False, 'prior failed first controller required')
            sources.append(pin(expected_receipt))
            argv[argv.index('--receipt') + 1] = str(root / 'controllers/qual-baseline-w1-attempt2.json')
        require(not Path(argv[argv.index('--receipt') + 1]).exists(), 'preserve existing controller receipt')
        commands.append({'label': entry['label'], 'variant': variant, 'request': entry['request'],
                         'argv': argv, 'controller_path': argv[argv.index('--receipt') + 1]})
    return commands


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=DEFAULT_ROOT)
    parser.add_argument('--prior-creation', type=int, required=True, help='Expected creation FILETIME of prior PID 37480')
    args = parser.parse_args()
    root = args.root.resolve()
    require(os.name == 'nt' and platform.node().upper() == 'DESKTOP-DJ1C40R', 'Jack desktop required')
    require(args.prior_creation > 0 and root == DEFAULT_ROOT, 'exact owned root/prior identity required')
    deadline = time.monotonic() + LIMIT_SECONDS
    plan_ref = pin(root / 'qualification.plan.json')
    require(plan_ref['sha256'] == PLAN_SHA, 'frozen qualification plan differs')
    plan = checked(plan_ref)
    require(pin(sys.executable) == plan['python_pin'], 'frozen pinned Python required')
    prior_ref = pin(root / 'qualification-coordinator/state.json')
    prior = checked(prior_ref)
    require(prior.get('complete') is False and prior.get('completed') == []
            and prior.get('finished_utc') and prior.get('error') and prior.get('plan') == plan_ref,
            'terminal failed prior coordinator with zero completions required')
    runner_ref = pin(Path(__file__).resolve())
    sources = [plan_ref, plan['python_pin'], runner_ref, prior_ref]
    sources += list(plan['helpers'].values()) + [plan['helper'], plan['source_config'], plan['runtime_decks']]
    sources += [ref for refs in plan['tools'].values() for ref in refs]
    sources += list(plan['runtimes'].values()) + [row['original'] for row in plan['input_dependencies']]
    sources += [checked(ref)['binary'] for ref in plan['runtimes'].values()]
    sources += plan['supporting_helpers']
    sources += [pin(path) for path in sorted((root / 'qualification-coordinator').rglob('*')) if path.is_file()]
    commands = prepare(root, plan, sources)
    for ref in sources:
        require(pin(ref['path']) == ref, 'frozen source changed: ' + ref['path'])
    apis = {}
    for variant in ('baseline', 'candidate'):
        tools = root / variant / 'python/tools'
        apis[variant] = (load('retry_' + variant + '_reservations', tools / 'host_reservation_v1.py'),
                         load('retry_' + variant + '_dispatch', tools / 'native_expanded_dispatch_v1.py', tools))
    require(apis['baseline'][0].process_state(PRIOR_PID, args.prior_creation) == 'absent',
            'prior coordinator process is live or unknown; no launch')
    watcher = load('retry_directory_watch', root.parent / 'resume_qualifications.py').DirectoryChangeWait
    out = root / 'qualification-coordinator-v2'
    out.mkdir(exist_ok=False)
    overlay = {'schema': 'qualification-launch-overlay/v1', 'original_plan': plan_ref, 'runner_source': runner_ref,
               'prior_state': prior_ref, 'prior_process': {'pid': PRIOR_PID, 'creation_time': args.prior_creation,
                                                         'state': 'absent'}, 'commands': commands,
               'only_argv_change': 'first --receipt becomes controllers/qual-baseline-w1-attempt2.json'}
    overlay_path = out / 'launch-overlay.json'
    with overlay_path.open('x', encoding='utf-8') as stream:
        json.dump(overlay, stream, indent=2, allow_nan=False); stream.flush(); os.fsync(stream.fileno())
    overlay_ref = pin(overlay_path)
    sources.append(overlay_ref)
    state = {'schema': 'qualification-coordinator-retry/v1', 'complete': False, 'completed': [],
             'plan': plan_ref, 'launch_overlay': overlay_ref, 'runner_source': runner_ref,
             'started_utc': now(), 'wait_limit_seconds': LIMIT_SECONDS, 'queue_waits': []}

    def save():
        pending = out / 'state.pending'
        with pending.open('w', encoding='utf-8') as stream:
            json.dump(state, stream, indent=2, allow_nan=False); stream.flush(); os.fsync(stream.fileno())
        os.replace(pending, out / 'state.json')

    def stable():
        remaining(deadline)
        for ref in sources:
            require(pin(ref['path']) == ref, 'frozen source changed: ' + ref['path'])

    save()
    try:
        canonical = None
        for entry in commands:
            label = entry['label']
            state['active_case'] = label
            reservations, dispatch = apis[entry['variant']]
            stable()
            runtime = checked(plan['runtimes'][entry['variant']])
            require(pin(runtime['binary']['path']) == runtime['binary'], 'runtime binary changed')
            request = checked(entry['request'])
            require(not Path(request['root']).exists() and not Path(request['cold_root']).exists()
                    and not Path(entry['controller_path']).exists(), 'case output exists; no launch')

            def changed(value):
                state['waiting'] = value; save()

            waited = await_admission(reservations, dispatch, runtime['binary']['path'], watcher, deadline, changed)
            state['queue_waits'].append({'case': label, **waited}); state.pop('waiting', None); save()
            stable()
            require(reservations.process_state(PRIOR_PID, args.prior_creation) == 'absent', 'prior coordinator is not absent')
            # Exactly one public driver invocation per entry; final refusal is terminal.
            with (out / (label + '.log')).open('xb') as log:
                remaining(deadline)
                child = subprocess.Popen(entry['argv'], stdout=log, stderr=subprocess.STDOUT)
                state['driver_pid'] = child.pid; save()
                result = child.wait(timeout=remaining(deadline))
            require(result == 0, 'qualification failed; preserve fresh receipt and stop: ' + label)
            controller_ref = pin(entry['controller_path'])
            controller = checked(controller_ref)
            require(controller['complete'] is True and controller['request'] == entry['request'], 'incomplete controller')
            report_ref = controller['report']
            report = checked(report_ref)
            require(report['complete'] is True and report['qualification'] is True
                    and report['completed_updates'] == 1 and report['completed_games'] == 10,
                    'incomplete first-update qualification')
            if canonical is None:
                canonical = report['fingerprint']
            else:
                require(report['fingerprint'] == canonical, 'full first-update fingerprint differs')
            state['completed'].append({'label': label, 'controller': controller_ref, 'report': report_ref})
            state.pop('driver_pid', None); save()
        stable()
        state.update(complete=True, canonical_fingerprint=canonical)
    except BaseException as error:
        state['error'] = type(error).__name__ + ': ' + str(error)
        if isinstance(error, subprocess.TimeoutExpired):
            state['driver_supervision'] = 'Public driver remains under its original runtime guard; no kill or retry performed.'
        raise
    finally:
        state['finished_utc'] = now(); save()


if __name__ == '__main__':
    main()
