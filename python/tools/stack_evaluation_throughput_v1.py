"""Guard final-checkpoint CPU placement and copied full-count recovery evidence."""
from datetime import datetime,timezone
from pathlib import Path
import math
import json
from public_evaluation_dispatch_v1 import read,pin,checked,HOSTS,inventory


def validate_report(item,plan):
    report=read(checked(item));expected={j['id']:j for j in plan['qualification_jobs']}
    assert len(report['jobs'])==32 and report['matches']==256
    actual={};seen=set()
    for host,a in report['allocation'].items():
        worker=read(checked(report['workers'][host]))
        assert worker['hostname'].upper()==HOSTS[host] and not worker['errors'] and worker['workers']==a['workers']
    for job in report['jobs']:
        assert job['id'] in expected and job['id'] not in seen;seen.add(job['id'])
        request=read(checked(job['request']));want=expected[job['id']]['command']
        assert {k:v for k,v in request.items() if k!='output_directory'}=={k:v for k,v in want.items() if k!='output_directory'}
        e=read(checked(job['execution']));assert e['exit_code']==0 and not e['timeout']
        assert e['binary']==plan['binary'] and e['request']==job['request']
        assert e['host']==job['host'] and e['storage']==report['allocation'][job['host']]
        assert e['native_binary']['sha256']==plan['binary']['sha256'] and e['native_request']['sha256']==job['request']['sha256']
        recovery=read(checked(job['recovery']));assert recovery['mismatches']==0
        directory=Path(job['output_directory']);completion=read(directory/'completion.json')
        start=read(directory/'start.json');assert start['command']==request
        assert completion['matches']==len(completion['match_sha256'])==8
        games=decisions=0
        for i,digest in enumerate(completion['match_sha256']):
            path=directory/f'match-{i:06}.json';assert pin(path)['sha256']==digest
            m=read(path);assert m['match']==request['matches'][i] and not m['decisions'] and m['models']==start['models']
            actual[f"{job['id']}/{i}"]=digest;games+=len(m['games']);decisions+=m['decision_count']
        assert games==completion['natural_games'] and decisions==completion['decisions']
    assert seen==set(expected) and actual==report['fingerprints']
    return report,actual


def require_choice(path,plan_pin):
    path=Path(path)
    assert not (path.parent/'qualification-revocation.json').exists(),'qualification revoked'
    choice=read(path);plan=read(checked(plan_pin))
    assert choice['schema']=='stack-evaluation-allocation/v1' and choice['plan']==plan_pin
    assert plan['schema']=='stack-screen-evaluation/v1' and plan['expected_jobs']==512 and plan['expected_matches']==4096
    assert choice['binary']==plan['binary'];checked(plan['binary'])
    for dependency in choice['dependencies']:checked(dependency)
    assert set(choice['inventory'])=={'jack','haleyspc','runpod'}
    for host,row in choice['inventory'].items():
        checked(row['evidence']);when=datetime.fromisoformat(row['checked_at'])
        assert when.tzinfo and 0<=(datetime.now(timezone.utc)-when).total_seconds()<86400
        assert row['reason'] and isinstance(row['eligible'],bool)
    eligible={h for h,r in choice['inventory'].items() if r['eligible']}
    assert eligible=={'jack','haleyspc'},'this bounded grid requires both idle PCs and no paid allocation'
    options=[];baseline=None;counts={h:set() for h in eligible};cache={}
    for candidate in choice['candidates']:
        report,fingerprints=validate_report(candidate['report'],plan)
        assert baseline is None or baseline==fingerprints,'placement changes saved gameplay'
        baseline=fingerprints
        for host,a in report['allocation'].items():
            assert host in eligible and 1<=a['workers']<=32
            counts[host].add(a['workers'])
            snap=read(checked(choice['inventory'][host]['evidence']))
            partition=next(p for p in snap['partitions'] if p['DriveLetter']==a['drive'])
            disk=next(d for d in snap['disks'] if d['Number']==partition['DiskNumber'])
            assert a['disk_serial']==disk['SerialNumber'] and a['disk_name']==disk['FriendlyName']
        for owner_pin in [candidate['before_owners'],candidate['after_owners']]:
            owners=read(checked(owner_pin));assert set(owners)==eligible
            assert all(not s['active'] for s in owners.values()),'contended timing'
        before=read(checked(candidate['before_owners']));after=read(checked(candidate['after_owners']))
        for job in report['jobs']:
            h=job['host'];directory=Path(job['output_directory']).parent
            started=read(directory/'started.json')['started_unix']
            elapsed=read(checked(job['execution']))['seconds']
            assert datetime.fromisoformat(before[h]['at']).timestamp()<=started
            assert datetime.fromisoformat(after[h]['at']).timestamp()>=started+elapsed
        assert all(math.isfinite(report[k]) and report[k]>=0 for k in ['staging_seconds','execution_seconds','recovery_seconds'])
        assert report['execution_seconds']>0
        from stack_evaluation_recovery_v1 import validate
        calibration=candidate['recovery']
        cache_key=json.dumps([calibration,{h:{k:v for k,v in a.items() if k!='workers'} for h,a in report['allocation'].items()}],sort_keys=True)
        if cache_key not in cache:cache[cache_key]=validate(calibration,report['allocation'],plan_pin)
        recovery=cache[cache_key]
        # Scale complete native jobs only. Fixed staging and export costs must
        # not be multiplied by the sample-to-panel ratio.
        scale=512/32
        setup=choice['remote_setup_seconds'] if 'haleyspc' in report['allocation'] else 0
        assert math.isfinite(setup) and setup>=0
        from stack_evaluation_staging_v1 import validate as validate_staging
        staging=validate_staging(candidate['full_staging'],report['allocation'],plan_pin)
        assert math.isfinite(staging)
        projected=setup+scale*report['execution_seconds']+staging+recovery
        options.append(dict(id=candidate['id'],allocation=report['allocation'],projected_seconds=projected,
            scaled_native_seconds=scale*report['execution_seconds'],measured_full_staging_seconds=staging,
            fixture_recovery_seconds=recovery,scope='Point forecast from one matched timing per allocation and two copied full-count recovery samples. Not a confidence bound.'))
    assert all(1 in v and any(n>1 for n in v) for v in counts.values())
    assert len({c['id'] for c in choice['candidates']})==len(choice['candidates'])
    assert options
    selected=min(options,key=lambda o:o['projected_seconds'])
    if choice['selected'] is not None:assert selected['id']==choice['selected'],'not fastest qualified allocation'
    return selected


def dispatch_qualified(root,label,choice_path,plan_pin,remote):
    from public_evaluation_dispatch_v2 import dispatch
    choice=read(choice_path);assert choice['selected'] is not None
    selected=require_choice(choice_path,plan_pin);plan=read(checked(plan_pin))
    current={h:inventory(h) for h in ['jack','haleyspc']}
    assert all(not s['active'] for s in current.values()),'preserve active native owners'
    assert selected['projected_seconds']<plan['projection_cap_seconds']
    return dispatch(root,label,plan['binary'],plan['jobs'],selected['allocation'],remote,plan['full_group_wall_cap_seconds'])
