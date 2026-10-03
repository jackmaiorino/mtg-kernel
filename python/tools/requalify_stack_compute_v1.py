"""Replace the one contended timing case without rerunning frozen comparisons."""
import argparse
import copy
import os
from datetime import datetime
from pathlib import Path
from public_training_dispatch_v2 import read,write,pin,checked,preflight
from public_training_storage_v1 import dispatch,require_storage_choice
from public_evaluation_dispatch_v1 import inventory
from qualify_stack_compute_v1 import audit


def run(root):
    old=Path('E:/mtg-meta-recovery-20260921/public-stack-compute-001')
    m=read(old/'manifest.json');previous=read(old/'compute-choice.json')
    correction=read(old/'timing-contamination-correction.json')
    assert correction['invalid_for_placement']==['b-w10']
    binary,configs=m['binary'],m['configs']
    checked(binary)
    # Only controller validation changed; native workload and all other bound
    # dispatch/storage dependencies remain identical to the measured cases.
    for item in previous['storage_dependencies']:
        if Path(item['path']).name!='compute_throughput_v3.py':checked(item)
    candidates=[c for c in previous['candidates'] if c['id']!='b-w10']
    assert len(candidates)==5
    root.mkdir()
    (root/'temp').mkdir()
    cuda=Path('C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8')
    os.environ['CUDA_PATH']=str(cuda);os.environ['PATH']=str(cuda/'bin')+os.pathsep+os.environ['PATH']
    os.environ['TEMP']=os.environ['TMP']=str(root/'temp')
    fresh={host:inventory(host) for host in ['jack','haleyspc']}
    available=copy.deepcopy(previous['inventory'])
    for host,snapshot in fresh.items():
        write(root/f'{host}-inventory.json',snapshot)
        assert not snapshot['active'],'preserve competing native owner'
        before=read(checked(previous['inventory'][host]['evidence']))['hardware']
        assert snapshot['cpu']==before['cpu'],'requalify complete grid after hardware change'
        available[host].update(checked_at=snapshot['at'],evidence=pin(root/f'{host}-inventory.json'))
    followups={}
    order=['a-w1','b-w1','a-w4','b-w4','a-w10','b-w10']
    for candidate in candidates:
        group=read(checked(candidate['benchmark']))
        reports={a:read(checked(p)) for a,p in group['jobs'].items()}
        last=max(read(checked(r['execution']))['finished_unix'] for r in reports.values())
        following=order[order.index(candidate['id'])+1]
        native=Path(f'D:/mtg-training-working/public-stack-compute-001-{following}-native-0')
        followups[candidate['id']]={}
        for host in fresh:
            path=native/(host+'-preflight.json');observation=read(path)
            assert not observation['active'] and datetime.fromisoformat(observation['at']).timestamp()>=last
            followups[candidate['id']][host]=pin(path)
    replacement=next(c for c in m['cases'] if c['id']=='b-w10')
    write(root/'manifest.json',dict(schema='stack-timing-correction/v1',runner=pin(__file__),
        previous=pin(old/'manifest.json'),revocation=pin(old/'qualification-revocation.json'),
        inherited_candidates=candidates,following_owner_checks=followups,
        binary=binary,configs=configs,replacement=replacement,maximum_game_executions=90,
        limitations='Retained five cases have later clean preflight observations; replacement also requires clean post-run ownership. This is not continuous monitoring of unrelated short-lived work.',
        no_formal_training=True))
    group_pin=dispatch(root/(root.name+'-b-w10'),binary,configs,replacement['placements'],replacement['storage'],3,mode='device_queues',stack=True)
    for host in fresh:
        observation=preflight(host,[p for p in replacement['placements'].values() if p['host']==host])
        write(root/(host+'-post-owners.json'),observation)
    group=read(checked(group_pin))
    reference=read(checked(candidates[0]['benchmark']))
    for arm,item in group['jobs'].items():
        report=read(checked(item));audit(report,arm)
        original=read(checked(reference['jobs'][arm]))
        assert {n:p['sha256'] for n,p in report['outputs'].items()}=={n:p['sha256'] for n,p in original['outputs'].items()}
    candidates.append(dict(id='b-w10-remeasured',benchmark=group_pin))
    projections={}
    for candidate in candidates:
        group=read(checked(candidate['benchmark']));queues={}
        for item in group['jobs'].values():
            r=read(checked(item));e=read(checked(r['execution']));c=read(checked(r['completion']))
            steady=sum(x['seconds'] for x in c['receipts'][1:])/2
            p=r['placement'];key=(p['host'],p['gpu_uuid'])
            queues[key]=queues.get(key,0)+e['seconds']+197*steady
        projections[candidate['id']]=max(queues.values())+group['staging_seconds']+group['recovery_seconds']*200/3
    choice=dict(previous,inventory=available,candidates=candidates,selected=min(projections,key=projections.get),
        storage_dependencies=[pin(p['path']) for p in previous['storage_dependencies']],
        correction_manifest=pin(root/'manifest.json'))
    write(root/'compute-choice.json',choice)
    selected=require_storage_choice(root/'compute-choice.json',binary,configs,stack=True)
    write(root/'result.json',dict(status='STACK-TIMING-CORRECTION-PASS',selected=selected,
        game_executions=90,exact_saved_file_comparisons=108,projections=projections,
        original_choice_remains_revoked=True,full_training_launched=False))
    print(selected,flush=True)


if __name__=='__main__':
    if not __debug__:raise RuntimeError('Assertions required')
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True);a=p.parse_args();run(a.root.resolve())
