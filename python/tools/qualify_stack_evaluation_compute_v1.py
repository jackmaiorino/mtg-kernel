"""Bounded final-endpoint CPU grid with exact replay and full-count recovery."""
import argparse
import copy
import json
import time
from pathlib import Path
from public_evaluation_dispatch_v1 import read,write,pin,checked,inventory,prepare_remote
from public_evaluation_dispatch_v2 import dispatch
from public_training_storage_v1 import storage
from stack_evaluation_recovery_v1 import build,calibrate
from stack_evaluation_throughput_v1 import validate_report,require_choice
from stack_evaluation_staging_v1 import measure as measure_staging


def qualify(root,plan_path):
    plan_pin=pin(plan_path);plan=read(checked(plan_pin))
    assert plan['schema']=='stack-screen-evaluation/v1' and len(plan['qualification_jobs'])==32
    assert plan['expected_jobs']==512 and plan['qualification_matches']==256
    root.mkdir()
    snapshots={h:inventory(h) for h in ['desktop','computehost']}
    available={};stores={}
    for host,drive in [('desktop','D'),('computehost','C')]:
        snapshot=snapshots[host];write(root/(host+'-inventory.json'),snapshot)
        assert not snapshot['active'],'preserve current training or other native work'
        available[host]=dict(checked_at=snapshot['at'],evidence=pin(root/(host+'-inventory.json')),
            eligible=True,reason='Idle native ownership; retain existing qualified SSD scratch role.')
        stores[host]={k:v for k,v in storage(snapshot,drive).items() if k in ['drive','disk_serial','disk_name']}
    cloud=Path('E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json')
    available['runpod']=dict(checked_at=read(cloud)['checked_at'],evidence=pin(cloud),eligible=False,
        reason='Authenticated inventory HTTP403; no new paid allocation authorized.')
    cases=[]
    for host,counts in [('desktop',[1,8,24]),('computehost',[1,8,16])]:
        for count in counts:cases.append((f'{host}-w{count}',{host:dict(stores[host],workers=count,job_weight=1)}))
    dependencies=[pin(Path(__file__).with_name(n)) for n in ['stack_evaluation_throughput_v1.py',
        'stack_evaluation_recovery_v1.py','public_evaluation_dispatch_v1.py','public_evaluation_dispatch_v2.py',
        'evaluation_recovery_fixture_v1.py','qualify_evaluation_placements_v6.py','stack_evaluation_staging_v1.py']]
    write(root/'design.json',dict(plan=plan_pin,runner=pin(__file__),dependencies=dependencies,
        question='Choose useful completed eight-match throughput on both PCs including staging and copied full-count recovery.',
        unique_native_cases=256,maximum_native_matches=2050,cases=cases,
        cross_host_cases='Fastest measured single-host worker counts combined at Desktop:ComputeHost job weights1:1 and2:1.',
        cheap_native_cases=2,case_wall_cap_seconds=600,new_case_launch_budget_seconds=1200,
        timing_repetitions=1,recovery_repetitions=2,storage_scope='Current D/C SSD roles, not a new exhaustive disk comparison.',
        no_paid_compute=True,formal_panel_launched=False,review=plan['review'],
        limitation='One native timing per allocation. Forecast uncertain for rare long games; no playing-strength inference from qualification.'))
    staging=read(plan_path.parent/'remote-staging.json')
    remote=prepare_remote(root,[v for k,v in staging['assets'].items() if k.startswith('inputs/')])
    began=time.monotonic();counter=0
    def owners(label):
        nonlocal counter
        current={h:inventory(h) for h in snapshots}
        path=root/f'owners-{counter:03}-{label}.json';counter+=1;write(path,current)
        assert all(not s['active'] for s in current.values()),'another native owner appeared; preserve evidence'
        return pin(path)
    cheap=copy.deepcopy(plan['qualification_jobs'][:1]);cheap[0]['command']['matches']=cheap[0]['command']['matches'][:1]
    cheap_fingerprint=None
    for host in snapshots:
        owners('before-cheap-'+host)
        p=dispatch(root,'cheap-'+host,plan['binary'],cheap,{host:dict(stores[host],workers=1,job_weight=1)},remote,60)
        r=read(checked(p));owners('after-cheap-'+host)
        assert r['execution_seconds']<30 and r['matches']==1
        assert cheap_fingerprint is None or cheap_fingerprint==r['fingerprints']
        cheap_fingerprint=r['fingerprints']
    candidates=[];baseline=None;fixture=None;remote_source=None;calibrations={};staging_costs={}
    def measure(label,allocation):
        nonlocal baseline,fixture,remote_source
        assert time.monotonic()-began<1200,'new-case launch budget exhausted'
        before=owners('before-'+label)
        result=dispatch(root,label,plan['binary'],plan['qualification_jobs'],allocation,remote,600)
        after=owners('after-'+label)
        report,actual=validate_report(result,plan)
        assert baseline is None or actual==baseline,'allocation changes gameplay'
        baseline=actual
        assert all(actual[k]==v for k,v in cheap_fingerprint.items())
        if fixture is None:
            fixture,remote_source=build(root,plan_pin,result,remote)
            owners('after-fixture-build')
        key=json.dumps({h:{k:v for k,v in a.items() if k!='workers'} for h,a in allocation.items()},sort_keys=True)
        if key not in calibrations:
            staging_costs[key]=measure_staging(root,label,plan_pin,allocation,remote)
            owners('after-full-staging-'+label)
            calibrations[key]=calibrate(root,label,allocation,fixture,remote,remote_source)
            owners('after-recovery-'+label)
        candidates.append(dict(id=label,report=result,before_owners=before,after_owners=after,recovery=calibrations[key],full_staging=staging_costs[key]))
        print(dict(case=label,matches=report['matches'],native_seconds=report['execution_seconds']),flush=True)
        return report
    native={}
    for label,allocation in cases:native[label]=measure(label,allocation)
    best={}
    for host in snapshots:
        # Recovery/storage is identical among worker counts on the same host;
        # choose completed native+staging cost before measured cross-host tests.
        label=min((k for k in native if set(native[k]['allocation'])=={host}),key=lambda k:native[k]['execution_seconds']+native[k]['staging_seconds'])
        best[host]=native[label]['allocation'][host]
    for weight in [1,2]:measure(f'both-desktop-weight-{weight}',{h:dict(a,job_weight=weight if h=='desktop' else 1) for h,a in best.items()})
    choice=dict(schema='stack-evaluation-allocation/v1',plan=plan_pin,binary=plan['binary'],
        inventory=available,candidates=candidates,dependencies=dependencies,selected=None,remote_setup_seconds=remote['seconds'])
    write(root/'choice-draft.json',choice)
    selected=require_choice(root/'choice-draft.json',plan_pin);choice['selected']=selected['id']
    owners('final');write(root/'compute-choice.json',choice)
    assert require_choice(root/'compute-choice.json',plan_pin)==selected
    write(root/'qualification.json',dict(complete=True,plan=plan_pin,selected=selected,
        compute_choice=pin(root/'compute-choice.json'),native_matches=2050,unique_cases=256,
        exact_match_comparisons=7*256,formal_panel_launched=False))
    print(selected,flush=True)


if __name__=='__main__':
    if not __debug__:raise RuntimeError('Assertions required')
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True);p.add_argument('--plan',type=Path,required=True);a=p.parse_args();qualify(a.root,a.plan)
