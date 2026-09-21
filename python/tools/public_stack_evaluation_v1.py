"""Prepare and execute the complete frozen stack screen after audited training."""
import argparse
import copy
from pathlib import Path
from public_training_dispatch_v2 import read,write,pin,checked
from public_evaluation_dispatch_v1 import prepare_remote
from public_stack_analysis_v1 import ARMS,GATES,analyze


def prepare(root,pilot):
    m=read(pilot/'manifest.json');training=read(pilot/'training-audit.json')
    assert m['schema']=='matched-public-stack-screen/v1' and m['gates']==GATES
    assert training['complete'] and training['full_natural_games']==6000
    assert m['analysis']==pin(Path(__file__).with_name('public_stack_analysis_v1.py'))
    for key in ['analysis','design','evaluation_binary','evaluation_build','evaluation_qualification','evaluation_transfer_comparison']:checked(m[key])
    assets={}
    def add(item):checked(item);assets[item['path']]=item
    endpoints=dict(g115=dict(kind='legacy',source=m['source'],v3_forced_actions=False))
    for arm in ARMS[1:]:
        final=training['arms'][arm];assert final['updates']==200 and final['natural_games']==2000
        endpoints[arm]=dict(kind='stack_checkpoint',config=m['training_configs'][arm],checkpoint=final['checkpoint'])
        for item in [m['training_configs'][arm],final['checkpoint'],final['optimizer']]:add(item)
    descriptor=m['evaluation_opponent']['source']['play_import'];add(descriptor)
    for item in read(checked(descriptor)).values():
        if isinstance(item,dict) and 'path' in item and 'sha256' in item:add(item)
    jobs=[]
    for template in m['jobs']:
        for arm in ARMS:
            seat=template['candidate_seat'];command=read(checked(template['template']))
            command['sources'][seat]=endpoints[arm]
            command['sources'][1-seat]=m['evaluation_opponent']
            assert len(command['matches'])==8 and not command['capture_decisions']
            jobs.append(dict(id=f"{arm}-{template['label']}",arm=arm,label=template['label'],command=command,
                own=template['cases'][0]['own'],seat=seat,chunk=template['original_indices'][0]//8))
    assert len(jobs)==512 and sum(len(j['command']['matches']) for j in jobs)==4096
    jobs.sort(key=lambda j:(j['label'],j['arm']))
    own=sorted({j['own'] for j in jobs});assert len(own)==8
    # Outcome-independent timing sample: every own deck and endpoint, balanced
    # seats, rotating opponent chunk. Whole eight-match jobs retain startup cost.
    samples=[]
    for a,arm in enumerate(ARMS):
        for i,deck in enumerate(own):
            seat=(i+a)%2;chunk=(i+a)%8
            samples.append(next(j for j in jobs if j['arm']==arm and j['own']==deck and j['seat']==seat and j['chunk']==chunk))
    samples.sort(key=lambda j:(j['label'],j['arm']))
    assert len(samples)==32 and len({j['id'] for j in samples})==32
    root.mkdir()
    write(root/'plan.json',dict(schema='stack-screen-evaluation/v1',pilot=pin(pilot/'manifest.json'),
        training=pin(pilot/'training-audit.json'),binary=m['evaluation_binary'],endpoints=endpoints,
        jobs=jobs,qualification_jobs=samples,expected_matches=4096,expected_jobs=512,
        qualification_matches=256,runner=pin(__file__),analysis=m['analysis'],
        split_qualification=pin('E:/mtg-meta-recovery-20260921/public-stack-chunks-joint-001/result.json'),
        full_group_wall_cap_seconds=1800,projection_cap_seconds=1800,no_prefix_selection=True,
        no_paid_compute=True,review=m['review']))
    prepare_remote(root,list(assets.values()))
    print(dict(prepared=str(root),matches=4096,jobs=512,qualification_matches=256),flush=True)


def run(root,compute):
    from stack_evaluation_throughput_v1 import require_choice,dispatch_qualified
    plan_pin=pin(root/'plan.json');plan=read(checked(plan_pin))
    checked(plan['runner']);checked(plan['analysis'])
    qualification=read(compute/'qualification.json')
    assert qualification['complete'] and qualification['plan']==plan_pin
    selected=require_choice(compute/'compute-choice.json',plan_pin)
    assert selected==qualification['selected'] and selected['projected_seconds']<plan['projection_cap_seconds']
    write(root/'launch.json',dict(plan=plan_pin,choice=pin(compute/'compute-choice.json'),selected=selected,runner=pin(__file__)))
    result_pin=dispatch_qualified(root,'full-panel',compute/'compute-choice.json',plan_pin,read(root/'remote-staging.json'))
    result=read(checked(result_pin))
    assert result['matches']==4096 and len(result['jobs'])==512
    write(root/'evaluation.json',dict(pilot=plan['pilot'],endpoints=plan['endpoints'],jobs=result['jobs'],dispatch=result_pin))
    write(root/'completion.json',dict(complete=True,matches=4096,evaluation=pin(root/'evaluation.json'),outcome_analysis_performed=False))
    print(dict(complete=True,matches=4096,analysis_pending=True),flush=True)


if __name__=='__main__':
    if not __debug__:raise RuntimeError('Assertions required')
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('mode',choices=['prepare','run','analyze']);p.add_argument('--root',type=Path,required=True);p.add_argument('--pilot',type=Path);p.add_argument('--compute',type=Path);a=p.parse_args()
    if a.mode=='prepare':prepare(a.root,a.pilot)
    elif a.mode=='run':run(a.root,a.compute)
    else:
        completion=read(a.root/'completion.json');assert completion['complete'] and completion['matches']==4096
        analyze(a.pilot,checked(completion['evaluation']))
