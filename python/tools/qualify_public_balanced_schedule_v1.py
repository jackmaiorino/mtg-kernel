"""Bounded exact-workload scaling on idle compute host while the maintainer has a live owner.

This driver never launches full training and rejects newly idle desktop capacity.
"""
import argparse
from pathlib import Path
import time

from public_training_dispatch_v2 import read, write, pin, checked, dispatch_qualification
from public_entropy_available_replica_v2 import availability
from public_entropy_remote_replica_v1 import jobs_for
from qualify_state_prevention_compute_v1 import place, reports
from qualify_public_entropy_compute_v1 import audit
from compute_throughput_v2 import require_allocation
from public_balanced_schedule_analysis_v1 import statistical_checks, breadth_checks


def qualify(root, pilot):
    m = read(pilot/'manifest.json')
    assert m['schema'] == 'matched-public-balanced-schedule/v1'
    assert set(m['training_configs']) == {'control', 'balanced'}
    for item in [m['runner'], m['design'], m['analysis'], m['bootstrap_implementation'],
                 m['training_binary'], *m['training_configs'].values(), *m['dependencies']]:
        checked(item)
    statistical_checks(); breadth_checks()
    root.mkdir(parents=True)
    available = availability(root)
    configs, binary = m['training_configs'], m['training_binary']
    write(root/'manifest.json', dict(pilot=pin(pilot/'manifest.json'), binary=binary, configs=configs,
        runner=pin(__file__), workers=[1, 4, 10], initial_updates=3,
        unique_arm_games=60, maximum_executed_games=180, native_wall_seconds=300,
        source_commit=m['source_commit'], full_training_launched=False,
        dependencies=[pin(Path(__file__).with_name(n)) for n in ['public_training_dispatch_v2.py',
            'public_entropy_available_replica_v2.py', 'public_entropy_remote_replica_v1.py',
            'qualify_state_prevention_compute_v1.py', 'qualify_public_entropy_compute_v1.py',
            'compute_throughput_v2.py', 'public_evaluation_dispatch_v1.py']],
        non_claim='Timing/correctness only, no outcome inspection or human strength. If desktop becomes idle, requalify expanded placements.'))
    reference, candidates, projections, learning = {}, [], {}, {}
    compared = 0
    started = time.monotonic()
    for count in [1, 4, 10]:
        assert time.monotonic()-started < 900, 'qualification launch budget exhausted'
        current = root/f'availability-w{count}'
        current.mkdir()
        availability(current)
        placements = {arm: place('computehost', 0, count) for arm in configs}
        group_pin = dispatch_qualification(root/f'{root.name}-w{count}', binary, configs,
            placements, 'sequential', updates=3)
        group = read(checked(group_pin))
        estimate = 0
        for arm, report in reports(group_pin).items():
            fingerprint = {name: pin(checked(item))['sha256'] for name, item in report['outputs'].items()}
            if arm in reference:
                assert reference[arm] == fingerprint, 'scaling changed synchronous learning outputs'
                compared += len(fingerprint)
            else:
                reference[arm] = fingerprint
                learning[arm] = audit(report)
            completion = read(checked(report['completion']))
            execution = read(checked(report['execution']))
            if count == 1:
                assert execution['seconds'] < 120, 'cheap timing envelope exceeded'
            estimate += execution['seconds'] + 197*sum(r['seconds'] for r in completion['receipts'][1:])/2
        name = f'computehost-w{count}'
        projections[name] = estimate + group['staging_seconds'] + group['recovery_seconds']*200/3
        candidates.append(dict(id=name, benchmark=group_pin))
        print(dict(case=name, projected_seconds=projections[name], exact_files_compared=compared), flush=True)
    choice = dict(schema='public-training-allocation/v2', jobs=jobs_for(configs), inventory=available,
        candidates=candidates, selected=min(projections, key=projections.get))
    write(root/'compute-choice.json', choice)
    selected = require_allocation(root/'compute-choice.json', binary['sha256'], jobs_for(configs))
    assert compared == 144
    write(root/'qualification.json', dict(complete=True, pilot=pin(pilot/'manifest.json'),
        selected=selected, projections=projections, executed_games=180, unique_arm_games=60,
        exact_file_comparisons=compared, learning=learning, seconds=time.monotonic()-started,
        full_training_launched=False, recovery_model='Existing training-v2 proportional archive estimate; not corrected evaluation-v2 costing.',
        review=m['review']))
    print(dict(qualified=True, selected=selected, full_training_launched=False), flush=True)


if __name__ == '__main__':
    if not __debug__:
        raise RuntimeError('Validation must remain enabled')
    p = argparse.ArgumentParser()
    p.add_argument('--root', type=Path, required=True)
    p.add_argument('--pilot', type=Path, required=True)
    a = p.parse_args()
    qualify(a.root, a.pilot)
