"""Bounded evaluation split/replay qualification; never launches a formal panel."""
import argparse
import copy
import hashlib
import time
from pathlib import Path

from public_evaluation_dispatch_v1 import read, write, pin, checked, inventory, prepare_remote
from public_evaluation_dispatch_v2 import dispatch
from public_training_storage_v1 import storage


def split_jobs(jobs, size):
    pieces, mapping = [], {}
    for job in jobs:
        for offset in range(0, len(job['command']['matches']), size):
            item = copy.deepcopy(job)
            item['id'] = f"{job['id']}-c{offset//size:02}"
            item['command']['matches'] = item['command']['matches'][offset:offset+size]
            pieces.append(item)
            for index in range(len(item['command']['matches'])):
                mapping[f"{item['id']}/{index}"] = f"{job['id']}/{offset+index}"
    assert len(mapping) == sum(len(j['command']['matches']) for j in jobs)
    assert len(set(mapping.values())) == len(mapping)
    return pieces, mapping


def run(root, hosts):
    repo = Path(__file__).resolve().parents[2]
    build = Path('E:/mtg-meta-recovery-20260921/public-stack-evaluation-tools-002/completion.json')
    binary = read(build)['binaries']['public_feature_evaluation_v1']
    checked(binary)
    baseline = Path('E:/mtg-meta-recovery-20260921/public-stack-evaluation-003')
    assert read(baseline/'result.json')['status'] == 'STACK-BO3-AND-LOADED-REPLAY-ENGINEERING-PASS'
    old = Path('E:/mtg-postboard-campaign-20260921/public-balanced-schedule-pilot-001/replica-1/manifest.json')
    templates = read(old)['jobs']
    own_labels = sorted({j['cases'][0]['own'] for j in templates})
    jobs, assets = [], {}
    def add(item):
        checked(item)
        assets[item['path']] = item
    for mode_index, mode in enumerate(['structured','permuted','disabled']):
        for seat in range(2):
            own = own_labels[mode_index*2+seat]
            template = next(t for t in templates if t['candidate_seat'] == seat and t['cases'][0]['own'] == own)
            command = read(checked(template['template']))
            command['sources'] = read(baseline/f'trained-{mode}-p{seat}/request.json')['sources']
            command['capture_decisions'] = False
            # Two preselected seeds per opponent, all eight opponents; outcomes
            # are never consulted when choosing timing or replay cases.
            command['matches'] = [command['matches'][i] for i in range(64) if i % 8 < 2]
            for index, match in enumerate(command['matches']):
                label = f'stack-chunk-engineering-20260921/{mode}/{seat}/{index}'
                match['config']['seed'] = int.from_bytes(hashlib.sha256(label.encode()).digest()[:8], 'little')
            endpoint = command['sources'][seat]
            add(endpoint['config']); add(endpoint['checkpoint'])
            add(pin(checked(endpoint['checkpoint']).parent/'optimizer.json'))
            descriptor = command['sources'][1-seat]['source']['play_import']
            add(descriptor)
            for item in read(checked(descriptor)).values():
                if isinstance(item, dict) and 'path' in item and 'sha256' in item:
                    add(item)
            jobs.append(dict(id=f'{mode}-p{seat}', arm=mode, label=f'{mode}-p{seat}', command=command))
    assert len(jobs) == 6 and sum(len(j['command']['matches']) for j in jobs) == 96
    split, mapping = split_jobs(jobs, 8)
    assert len(split) == 12
    root.mkdir()
    snapshots = {host: inventory(host) for host in ['desktop','computehost']}
    stores = {}
    for host, drive in [('desktop','D'),('computehost','C')]:
        write(root/f'{host}-inventory.json', snapshots[host])
        stores[host] = {k:v for k,v in storage(snapshots[host],drive).items() if k in ['drive','disk_serial','disk_name']}
    assert all(not snapshots[h]['active'] for h in hosts), 'preserve competing native owners'
    cloud = Path('E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json')
    write(root/'manifest.json', dict(schema='stack-chunk-qualification/v1', runner=pin(__file__),
        dependencies=[pin(Path(__file__).with_name(n)) for n in ['public_evaluation_dispatch_v1.py','public_evaluation_dispatch_v2.py','public_training_storage_v1.py']],
        binary=binary, native_build=pin(build), gameplay_qualification=pin(baseline/'result.json'),
        template_source=pin(old), jobs=jobs, split_jobs=split, original_match_mapping=mapping,
        unique_engineering_cases=96, maximum_bo3_executions=96*(1+2*len(hosts))+len(hosts), selected_hosts=hosts,
        deferred_hosts=[h for h in snapshots if h not in hosts], new_case_launch_budget_seconds=900,
        group_wall_seconds=600, original_chunk_matches=16, split_chunk_matches=8,
        cloud=dict(evidence=pin(cloud), checked_at=read(cloud)['checked_at'], reason='Authenticated HTTP403; no paid allocation.'),
        limitations='Six own-deck/seat combinations, eight opponents, two seeds each; tiny trained checkpoints. Exact split/replay check and preliminary worker scaling, not a final-checkpoint production allocation or strength evaluation.',
        review='Fable known zero-read429 until September22 07:00EDT; no retry or endorsement. Bounded engineering under desktop research authority.'))
    remote = prepare_remote(root, list(assets.values()))
    started = time.monotonic()
    def execute(label, items, host, workers):
        assert time.monotonic()-started < 900, 'new-case launch budget exhausted'
        result_pin = dispatch(root,label,binary,items,{host:dict(stores[host],workers=workers)},remote,600)
        result = read(checked(result_pin))
        following = inventory(host)
        write(root/(label+'-post-owners.json'),following)
        assert not following['active'], 'Owner appeared during qualification; preserve outputs but reject timing'
        assert result['matches'] == sum(len(j['command']['matches']) for j in items)
        for job in result['jobs']:
            execution = read(checked(job['execution']))
            assert execution['exit_code'] == 0 and not execution['timeout']
        print(dict(completed=label,matches=result['matches'],execution_seconds=result['execution_seconds']),flush=True)
        return result_pin, result
    cheap = copy.deepcopy(jobs[:1]); cheap[0]['command']['matches'] = cheap[0]['command']['matches'][:1]
    cheap_results = []
    for host in hosts:
        p,r = execute('cheap-'+host,cheap,host,1)
        assert r['execution_seconds'] < 30, 'cheap timing exceeds bounded envelope'
        cheap_results.append((p,r))
    assert all(r['fingerprints'] == cheap_results[0][1]['fingerprints'] for _,r in cheap_results), 'cheap host replay differs'
    reference_pin, reference = execute('original-'+hosts[0],jobs,hosts[0],1)
    assert all(reference['fingerprints'][k] == v for k,v in cheap_results[0][1]['fingerprints'].items())
    reports = []
    for host, workers in [(h,w) for h in hosts for w in [1,8]]:
        p,r = execute(f'split-{host}-w{workers}',split,host,workers)
        normalized = {mapping[k]:v for k,v in r['fingerprints'].items()}
        assert normalized == reference['fingerprints'], 'split chunk or host changes saved match bytes'
        reports.append(dict(report=p,host=host,workers=workers,exact_matches=len(normalized),
            execution_seconds=r['execution_seconds'],staging_seconds=r['staging_seconds'],recovery_seconds=r['recovery_seconds']))
    write(root/'result.json', dict(status='STACK-SPLIT-REPLAY-PASS' if len(hosts)==2 else 'STACK-SPLIT-REPLAY-PARTIAL',
        qualified_hosts=hosts,bo3_executions=96*(1+2*len(hosts))+len(hosts),unique_cases=96,
        exact_split_match_comparisons=192*len(hosts),reference=reference_pin,reports=reports,
        cheap=[p for p,_ in cheap_results],remote_staging=pin(root/'remote-staging.json'),
        elapsed_seconds=time.monotonic()-started,formal_panel_launched=False,
        non_claim='Splitting preserves every match byte on qualified hosts only. Preliminary scaling on engineering checkpoints; qualify other hosts, final endpoint timing and full-panel recovery separately. No playing-strength conclusion.'))


if __name__ == '__main__':
    if not __debug__: raise RuntimeError('Qualification requires assertions')
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True)
    p.add_argument('--host',choices=['desktop','computehost','both'],default='both');a=p.parse_args()
    run(a.root.resolve(),['desktop','computehost'] if a.host=='both' else [a.host])
