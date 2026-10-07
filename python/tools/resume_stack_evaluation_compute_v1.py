"""Finish missing bounded timing cases without repeating completed evidence."""
import argparse
import json
from pathlib import Path
from public_evaluation_dispatch_v1 import read,write,pin,checked,inventory
from public_evaluation_dispatch_v2 import dispatch
from stack_evaluation_throughput_v1 import validate_report,require_choice
from stack_evaluation_recovery_v1 import calibrate
from stack_evaluation_staging_v1 import measure as measure_staging


def resume(root,previous):
    design=read(previous/'design.json');checked(design['runner'])
    for item in design['dependencies']:checked(item)
    stopped=read(previous/'controller-completion.json')
    assert stopped['terminal'] and stopped['exit_code']!=0 and stopped['reason']=='new-case launch budget exhausted'
    assert not (previous/'qualification.json').exists()
    plan_pin=design['plan'];plan=read(checked(plan_pin))
    root.mkdir();fresh={h:inventory(h) for h in ['desktop','computehost']};available={}
    for h,s in fresh.items():
        assert not s['active'],'preserve native owner'
        assert s['cpu']==read(previous/(h+'-inventory.json'))['cpu'],'hardware changed'
        write(root/(h+'-inventory.json'),s)
        available[h]=dict(checked_at=s['at'],evidence=pin(root/(h+'-inventory.json')),eligible=True,reason='Idle after terminal timing controller; same CPUs and storage roles.')
    cloud_path=Path('E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json')
    available['runpod']=dict(checked_at=read(cloud_path)['checked_at'],evidence=pin(cloud_path),eligible=False,
        reason='Authenticated HTTP403; no new paid allocation authorized.')
    remote=read(previous/'remote-staging.json')
    fixture=pin(previous/'recovery-fixture/manifest.json')
    assert read(checked(fixture))['plan']==plan_pin
    remote_source=remote['native_root']+'/copied-fixture'
    cases=list(design['cases']);reports={};baseline=None
    for label,allocation in cases:
        report,actual=validate_report(pin(previous/label/'result.json'),plan)
        assert report['allocation']==allocation
        assert baseline is None or actual==baseline
        baseline=actual;reports[label]=report
    best={}
    for host in fresh:
        label=min((k for k in reports if set(reports[k]['allocation'])=={host}),key=lambda k:reports[k]['execution_seconds']+reports[k]['staging_seconds'])
        best[host]=reports[label]['allocation'][host]
    for weight in [1,2]:cases.append((f'both-desktop-weight-{weight}',{h:dict(a,job_weight=weight if h=='desktop' else 1) for h,a in best.items()}))
    pending=[label for label,_ in cases if not (previous/label/'result.json').exists()]
    assert pending==['both-desktop-weight-2'],'this continuation is bounded to the one unlaunched final case'
    write(root/'design.json',dict(schema='stack-timing-continuation/v1',previous=pin(previous/'design.json'),
        previous_completion=pin(previous/'controller-completion.json'),plan=plan_pin,runner=pin(__file__),
        cases=cases,only_new_case=pending,maximum_new_native_matches=256,total_native_matches_after_completion=2050,
        fixture=fixture,remote_staging=pin(previous/'remote-staging.json'),
        reason='Retain seven complete matched timings and three placement recovery calibrations; complete the final allocation after the original new-launch time budget expired. No scientific gate or workload changes.'))
    candidates=[];calibration_cache={};staging_cache={}
    def owners(label):
        current={h:inventory(h) for h in fresh};path=root/('owners-'+label+'.json');write(path,current)
        assert all(not s['active'] for s in current.values()),'contended timing'
        return pin(path)
    for label,allocation in cases:
        report_path=previous/label/'result.json'
        key=json.dumps({h:{k:v for k,v in a.items() if k!='workers'} for h,a in allocation.items()},sort_keys=True)
        if report_path.exists():
            report_pin=pin(report_path);report,actual=validate_report(report_pin,plan)
            assert report['allocation']==allocation and actual==baseline
            def one(pattern):
                found=list(previous.glob(pattern));assert len(found)==1;return pin(found[0])
            before=one(f'owners-*-before-{label}.json');after=one(f'owners-*-after-{label}.json')
            if key not in calibration_cache:
                calibration_cache[key]=pin(previous/('recovery-'+label)/'calibration.json')
                staging_cache[key]=pin(previous/('staging-'+label)/'result.json')
                owners_after=one(f'owners-*-after-recovery-{label}.json')
                assert all(not s['active'] for s in read(checked(owners_after)).values())
        else:
            before=owners('before-'+label)
            report_pin=dispatch(root,label,plan['binary'],plan['qualification_jobs'],allocation,remote,600)
            after=owners('after-'+label)
            report,actual=validate_report(report_pin,plan);assert actual==baseline
            staging_cache[key]=measure_staging(root,label,plan_pin,allocation,remote);owners('after-staging-'+label)
            calibration_cache[key]=calibrate(root,label,allocation,fixture,remote,remote_source);owners('after-recovery-'+label)
        candidates.append(dict(id=label,report=report_pin,before_owners=before,after_owners=after,
            recovery=calibration_cache[key],full_staging=staging_cache[key]))
    choice=dict(schema='stack-evaluation-allocation/v1',plan=plan_pin,binary=plan['binary'],inventory=available,
        candidates=candidates,dependencies=design['dependencies'],selected=None,remote_setup_seconds=remote['seconds'],continuation=pin(root/'design.json'))
    write(root/'choice-draft.json',choice);selected=require_choice(root/'choice-draft.json',plan_pin)
    choice['selected']=selected['id'];owners('final');write(root/'compute-choice.json',choice)
    assert require_choice(root/'compute-choice.json',plan_pin)==selected
    write(root/'qualification.json',dict(complete=True,plan=plan_pin,selected=selected,
        compute_choice=pin(root/'compute-choice.json'),native_matches=2050,new_native_matches=256,
        reused_native_matches=1794,unique_cases=256,exact_match_comparisons=7*256,formal_panel_launched=False))
    print(selected,flush=True)


if __name__=='__main__':
    if not __debug__:raise RuntimeError('Assertions required')
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True);p.add_argument('--previous',type=Path,required=True);a=p.parse_args();resume(a.root,a.previous)
