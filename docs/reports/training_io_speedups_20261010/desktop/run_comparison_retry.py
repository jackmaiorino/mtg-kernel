"""Add retry provenance and free-plus-busy admission to the frozen coordinator.

The original coordinator executes under its real file path. Only source pin
spelling, additional verified sources and its admission wait are adapted; every
case timing statement and public driver command remains unchanged.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import time

BASE = Path('D:/training-io-speedups-20261010')
ROOT = BASE / 'desktop'
RETRY_REFS = ('helper', 'qualification_plan', 'qualification_state', 'retry_staging_loader')


def require(ok, message):
    if not ok:
        raise ValueError(message)


def pin(path):
    spelling = str(path)
    with Path(path).open('rb') as stream:
        return {'path': spelling, 'sha256': hashlib.file_digest(stream, 'sha256').hexdigest()}


def verify(ref):
    require(pin(ref['path']) == ref, 'retry source changed: ' + ref['path'])
    return ref


def checked(ref):
    verify(ref)
    return json.loads(Path(ref['path']).read_bytes())


def load(name, ref):
    verify(ref)
    spec = importlib.util.spec_from_file_location(name, ref['path'])
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def retry_sources(plan):
    sources = [verify(plan[key]) for key in RETRY_REFS]
    require(Path(plan['qualification_plan']['path']) == ROOT / 'qualification.plan.json',
            'exact original qualification plan required')
    frozen = checked(plan['qualification_plan'])
    state = checked(plan['qualification_state'])
    labels = [f'qual-{variant}-w{workers}' for variant in ('baseline', 'candidate') for workers in (1, 2, 4, 8)]
    require(frozen['stage'] == 'qualify' and frozen['helper'] == plan['helper']
            and [item['label'] for item in frozen['commands']] == labels
            and state['complete'] is True and not state.get('error') and state['plan'] == plan['qualification_plan']
            and [item['label'] for item in state['completed']] == labels,
            'complete qualification provenance differs from sealed plan')
    runner = verify(state['runner_source'])
    require(Path(runner['path']) == BASE / 'resume_qualifications_retry.py', 'exact admission runner required')
    sources += [pin(Path(__file__).resolve()), runner]
    if 'launch_overlay' in state:
        sources.append(verify(state['launch_overlay']))
    for item, command in zip(state['completed'], frozen['commands']):
        controller, report, request = checked(item['controller']), checked(item['report']), checked(command['request'])
        require(controller['complete'] is True and controller['request'] == command['request']
                and controller['report'] == item['report'] and report['request'] == command['request'],
                'qualification receipt index binding differs: ' + item['label'])
        runtime = checked(request['runtime'])
        sources += [item['controller'], item['report'], command['request'], request['config'], request['runtime'], runtime['binary']]
    for ref in sources:
        verify(ref)
    return list({(ref['path'], ref['sha256']): ref for ref in sources}.values()), runner


def admission_adapter(plan, admission, watcher_type):
    """Exactly one advisory admission wait for each unchanged ABBA command."""
    cases = iter(plan['commands'])
    apis = {}
    for variant in ('baseline', 'candidate'):
        tools = ROOT / variant / 'python/tools'
        expected = {Path(ref['path']): ref for ref in plan['tools'][variant]}
        for name in ('host_reservation_v1.py', 'native_expanded_dispatch_v1.py'):
            verify(expected[tools / name])
        apis[variant] = (admission.load('formal_retry_' + variant + '_reservations', tools / 'host_reservation_v1.py'),
                         admission.load('formal_retry_' + variant + '_dispatch', tools / 'native_expanded_dispatch_v1.py', tools))

    def wait_free(changed=None, deadline=None):
        case = next(cases)
        request = checked(case['request'])
        runtime = checked(request['runtime'])
        verify(runtime['binary'])
        reservations, dispatch = apis[case['variant']]
        waited = admission.await_admission(reservations, dispatch, runtime['binary']['path'], watcher_type,
                                          deadline if deadline is not None else time.monotonic() + 7200,
                                          changed if changed is not None else lambda value: None)
        return {**waited, 'scope': 'Read-only canonical reservation and frozen busy-pattern queue; outside case timing. Public dispatch remains the final atomic guard.'}
    return wait_free


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', type=Path, default=ROOT / 'formal/seal.plan.json')
    parser.add_argument('--decks', type=Path, required=True)
    parser.add_argument('--t1', type=Path, required=True)
    parser.add_argument('--execute', action='store_true')
    args = parser.parse_args()
    plan_ref = pin(args.plan)
    plan = checked(plan_ref)
    require(plan['stage'] == 'seal' and len(plan['commands']) == 4, 'sealed four-case plan required')
    require(pin(sys.executable) == plan['python_pin'], 'pinned comparison Python required')
    extra_sources, runner_ref = retry_sources(plan)
    original = verify(plan['helpers']['run_comparison.py'])
    require(Path(original['path']) == ROOT / 'run_comparison.py', 'exact frozen coordinator required')
    coordinator = load('frozen_formal_coordinator', original)
    admission = load('formal_retry_admission', runner_ref)
    sys.path.insert(0, str(ROOT))
    waiter = load('wait_canonical_free', plan['helpers']['wait_canonical_free.py'])
    watcher_ref = next(ref for ref in plan['supporting_helpers'] if Path(ref['path']) == BASE / 'resume_qualifications.py')
    watcher = load('formal_retry_directory_watch', watcher_ref).DirectoryChangeWait
    waiter.wait_free = admission_adapter(plan, admission, watcher)
    coordinator.pin = pin

    def coordinator_checked(ref):
        value = checked(ref)
        if ref == plan_ref:
            # The sealed file and common helper pins remain unchanged. The
            # execution source graph additionally records every retry input.
            return {**value, 'supporting_helpers': value['supporting_helpers'] + extra_sources}
        return value

    coordinator.checked = coordinator_checked
    coordinator.main()


if __name__ == '__main__':
    main()
