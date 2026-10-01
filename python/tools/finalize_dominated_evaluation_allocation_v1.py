"""Finish a complete grid when an oversized recovery case cannot win on time.

No native work is launched. Keep the existing volume envelope. An excluded
allocation must exceed it and have a native-only projection already slower
than the fastest fully qualified allocation, even with zero transfer cost.
"""
import argparse
import copy
import statistics
from pathlib import Path
from public_evaluation_dispatch_v1 import read, write, pin, checked, inventory
from evaluation_throughput_v4 import require_choice
from evaluation_recovery_fixture_v1 import validate_calibration


def finish(root, prior):
    original = pin(prior/'choice-draft.json')
    choice = read(checked(original))
    plan = read(checked(choice['plan']))
    assert choice['schema'] == 'cpu-bo3-allocation/v4' and choice['selected'] is None
    root.mkdir()
    fresh = {h: inventory(h) for h in ('jack', 'haleyspc')}
    for host, snapshot in fresh.items():
        old = read(checked(choice['inventory'][host]['evidence']))
        assert not snapshot['active'] and choice['inventory'][host]['eligible']
        assert snapshot['cpu'] == old['cpu']
    write(root/'owner-inventory.json', fresh)
    grouped, baseline = {}, None
    for candidate in choice['candidates']:
        report = read(checked(candidate['report']))
        actual = {}
        for job in report['jobs']:
            execution = read(checked(job['execution']))
            assert execution['exit_code'] == 0 and not execution['timeout']
            assert execution['binary'] == plan['binary']
            request = read(checked(job['request']))
            assert execution['request'] == job['request']
            expected = next(j['command'] for j in plan['qualification_jobs'] if j['id'] == job['id'])
            assert {k:v for k,v in request.items() if k != 'output_directory'} == {k:v for k,v in expected.items() if k != 'output_directory'}
            folder = Path(job['output_directory'])
            complete = read(folder/'completion.json')
            assert complete['matches'] == len(complete['match_sha256']) == 1
            for index, digest in enumerate(complete['match_sha256']):
                assert pin(folder/f'match-{index:06}.json')['sha256'] == digest
                actual[f"{job['id']}/{index}"] = digest
        assert len(actual) == report['matches'] == 48 and actual == report['fingerprints']
        assert baseline is None or actual == baseline
        baseline = actual
        grouped.setdefault(candidate['allocation_id'], []).append((candidate, report))
    excluded, retained = [], []
    for label, runs in grouped.items():
        assert len(runs) == 2
        allocation = runs[0][1]['allocation']
        assert all(r['allocation'] == allocation for _,r in runs)
        _, _, sizes = validate_calibration(choice['recovery_calibrations'][label], allocation, plan, plan['binary'])
        ratios = []
        for _, report in runs:
            ratios.extend(plan['expected_matches']/report['matches'] *
                sum((Path(j['output_directory'])/'match-000000.json').stat().st_size for j in report['jobs'] if j['host'] == host)/sizes[host]
                for host in allocation)
        if max(ratios) > choice['recovery_projection']['max_byte_ratio']:
            excluded.append(dict(allocation_id=label, candidates=[c for c,_ in runs],
                maximum_recovery_byte_ratio=max(ratios),
                native_only_projected_seconds=statistics.median(plan['expected_matches']/r['matches']*r['execution_seconds'] for _,r in runs)))
        else:
            retained.extend(c for c,_ in runs)
    assert excluded and retained
    qualified = copy.deepcopy(choice)
    qualified['candidates'] = retained
    qualified['excluded_recovery_cases'] = excluded
    qualified['original_grid'] = original
    qualified['finalizer'] = pin(__file__)
    write(root/'choice-draft.json', qualified)
    selected = require_choice(root/'choice-draft.json', choice['plan'], plan['binary'])
    assert all(e['native_only_projected_seconds'] > selected['projected_seconds'] for e in excluded), 'Oversized case could win; measure its recovery instead'
    qualified['selected'] = selected['id']
    write(root/'compute-choice.json', qualified)
    assert require_choice(root/'compute-choice.json', choice['plan'], plan['binary']) == selected
    result = dict(complete=True, selected=selected, compute_choice=pin(root/'compute-choice.json'),
        native_matches=48*len(choice['candidates']), new_native_matches=0,
        reused_native_matches=48*len(choice['candidates']), unique_cases=48,
        exact_file_comparisons=48*(len(choice['candidates'])-1), excluded=excluded,
        interpretation='Exclusion is dominance under the existing projection objective, not a bound on actual future wall time. Volume guard unchanged. All gameplay replays verified.',
        review='Known Fable zero-read429 through September22 07:00EDT; no endorsement.', full_panel_launched=False)
    write(root/'qualification.json', result)
    print(dict(selected=selected, excluded=excluded), flush=True)


if __name__ == '__main__':
    assert __debug__
    p=argparse.ArgumentParser();p.add_argument('--root',type=Path,required=True);p.add_argument('--prior',type=Path,required=True);a=p.parse_args()
    finish(a.root,a.prior)
