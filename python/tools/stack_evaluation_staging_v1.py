"""Time full-plan request placement with no native evaluation process."""
import shutil
import subprocess
import time
import zipfile
from pathlib import Path
from public_evaluation_dispatch_v1 import read,write,pin,checked,inventory,ssh,REMOTE
from evaluation_recovery_fixture_v1 import assignment


def measure(root,label,plan_pin,allocation,remote):
    plan=read(checked(plan_pin));assigned=assignment(plan['jobs'],allocation)
    jobs={j['id']:j for j in plan['jobs']};folder=root/('staging-'+label);folder.mkdir()
    began=time.monotonic();specs={}
    for host in sorted(allocation):
        a=allocation[host];snap=inventory(host);assert not snap['active']
        native=Path(f"{a['drive']}:/mtg-state-prevention-eval/{root.name}/staging-{label}") if host=='jack' else Path(remote['native_root'])/('staging-'+label)
        canonical=folder/host;canonical.mkdir()
        if host=='jack':
            native.mkdir(parents=True);shutil.copy2(checked(plan['binary']),native/'public-evaluator.exe')
        else:
            ssh(f"New-Item -ItemType Directory -Path '{native.as_posix()}' | Out-Null")
            subprocess.run(['scp','-q',str(checked(plan['binary'])),f'{REMOTE}:{native.as_posix()}/public-evaluator.exe'],check=True,timeout=60)
        spec=dict(host=host,storage=a,workers=a['workers'],binary=plan['binary'],
            native_binary=dict(path=str(native/'public-evaluator.exe'),sha256=plan['binary']['sha256']),
            group_wall_seconds=plan['full_group_wall_cap_seconds'],job_wall_seconds=300,jobs=[])
        for identifier in assigned[host]:
            dest=canonical/'jobs'/identifier;dest.mkdir(parents=True)
            native_job=native/'jobs'/identifier
            command=dict(jobs[identifier]['command'],output_directory=str(native_job/'outputs'))
            write(dest/'request.json',command)
            if host=='jack':native_job.mkdir(parents=True);shutil.copy2(dest/'request.json',native_job/'request.json')
            request=pin(dest/'request.json')
            spec['jobs'].append(dict(id=identifier,native_directory=str(native_job),request=request,
                native_request=dict(path=str(native_job/'request.json'),sha256=request['sha256'])))
        write(canonical/'spec.json',spec)
        if host=='jack':shutil.copy2(canonical/'spec.json',native/'spec.json')
        else:
            with zipfile.ZipFile(canonical/'requests.zip','x',compression=zipfile.ZIP_DEFLATED) as archive:
                archive.write(canonical/'spec.json','spec.json')
                for identifier in assigned[host]:archive.write(canonical/'jobs'/identifier/'request.json',f'jobs/{identifier}/request.json')
            subprocess.run(['scp','-q',str(canonical/'requests.zip'),f'{REMOTE}:{native.as_posix()}/requests.zip'],check=True,timeout=60)
            ssh(f"Expand-Archive -LiteralPath '{native.as_posix()}/requests.zip' -DestinationPath '{native.as_posix()}'",timeout=180)
        specs[host]=pin(canonical/'spec.json')
    path=folder/'result.json'
    write(path,dict(schema='stack-full-request-staging/v1',plan=plan_pin,allocation=allocation,specs=specs,
        seconds=time.monotonic()-began,native_launched=False,jobs=512,
        method='Same per-host binary copy, full request serialization, local copies and remote request ZIP placement as dispatch_v2. Native execution intentionally omitted.'))
    return pin(path)


def validate(item,allocation,plan_pin):
    row=read(checked(item));plan=read(checked(plan_pin))
    assert row['schema']=='stack-full-request-staging/v1' and row['plan']==plan_pin and not row['native_launched']
    assert row['jobs']==512 and row['seconds']>0
    assert {h:{k:v for k,v in a.items() if k!='workers'} for h,a in row['allocation'].items()}=={h:{k:v for k,v in a.items() if k!='workers'} for h,a in allocation.items()}
    expected={j['id']:j for j in plan['jobs']};seen=set();assigned=assignment(plan['jobs'],allocation)
    assert set(row['specs'])==set(allocation)
    for host,spec_pin in row['specs'].items():
        spec=read(checked(spec_pin));assert spec['binary']==plan['binary']
        assert [j['id'] for j in spec['jobs']]==assigned[host]
        for job in spec['jobs']:
            assert job['id'] not in seen;seen.add(job['id'])
            request=read(checked(job['request']));want=expected[job['id']]['command']
            assert {k:v for k,v in request.items() if k!='output_directory'}=={k:v for k,v in want.items() if k!='output_directory'}
    assert seen==set(expected)
    return row['seconds']
