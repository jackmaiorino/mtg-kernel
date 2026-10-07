"""Bounded exact saved-behavior diagnostic with qualified independent CPU jobs."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import subprocess
import sys
import time
from public_training_dispatch_v2 import read,write,pin,checked
from public_evaluation_dispatch_v1 import inventory,ssh,REMOTE

PILOT=Path('E:/mtg-postboard-campaign-20260921/public-stack-screen-001')
TOOLS=Path('E:/mtg-meta-recovery-20260921/public-stack-sensitivity-tools-001')
REMOTE_BASE='C:/mtg-node/public-stack-sensitivity-001'
LOCAL_DATA='D:/mtg-training-working/stack-screen-training-native-0/computehost/recovered/jobs/structured/outputs'
REMOTE_DATA='C:/mtg-node/stack-screen-training-native-0/jobs/structured/outputs'

def signature(result):
    return dict(groups=result['groups'],exact_behavior_rows=result['exact_behavior_rows'],
        inputs=[r['trajectory']['sha256'] for r in result['archives']],
        exact=[r['exact_rows'] for r in result['archives']],
        weights=result['model']['weights_sha256'],optimizer=result['model']['optimizer_sha256'])

def worker(spec):
    out=Path(spec['root']);out.mkdir()
    binary=checked(spec['binary']);start=time.monotonic()
    def one(job):
        folder=out/job['id'];folder.mkdir()
        request=dict(job['command'],output_directory=str(folder/'outputs'))
        write(folder/'request.json',request)
        with (folder/'stdout.log').open('wb') as log:
            run=subprocess.run([str(binary),str(folder/'request.json')],stdout=log,stderr=subprocess.STDOUT,
                timeout=180,creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS|subprocess.CREATE_NO_WINDOW)
        assert run.returncode==0,(job['id'],run.returncode,str(folder/'stdout.log'))
        result=read(folder/'outputs/result.json')
        return dict(id=job['id'],result=pin(folder/'outputs/result.json'),signature=signature(result))
    with ThreadPoolExecutor(max_workers=spec['workers']) as pool:jobs=list(pool.map(one,spec['jobs']))
    result=dict(jobs=jobs,workers=spec['workers'],seconds=time.monotonic()-start)
    write(out/'result.json',result)
    return result

def main(root):
    root.mkdir();start=time.monotonic()
    owners={h:inventory(h) for h in ['desktop','computehost']}
    assert all(not s['active'] for s in owners.values())
    write(root/'owners-before.json',owners)
    cloud=pin('E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json')
    build=read(TOOLS/'completion.json');assert build['complete']
    binary=build['binaries']['stack_sensitivity_v1'];checked(binary)
    audit=read(PILOT/'training-audit.json');assert audit['complete']
    report=read(checked(audit['arms']['structured']['report']));m=read(PILOT/'manifest.json')
    jobs=[]
    for update in range(19,200,20):
        checkpoint=report['outputs'][f'{update-1:04}/checkpoint.json']
        trajectories=[report['outputs'][f'{update:04}/episode-{e:03}.json'] for e in range(10)]
        for f in [checkpoint,*trajectories]:checked(f)
        jobs.append(dict(id=f'u{update:03}',command=dict(config=m['training_configs']['structured'],checkpoint=checkpoint,trajectories=trajectories)))
    write(root/'manifest.json',dict(jobs=jobs,binary=binary,build=pin(TOOLS/'completion.json'),
        source=pin(__file__),design=pin(Path(__file__).resolve().parents[2]/'docs/public_stack_trace_diagnostic_20260921.md'),
        cloud=cloud,cloud_status='Dated authenticated HTTP403; no paid allocation authorized.',max_archives=100))
    setup=time.monotonic()
    ssh(f"New-Item -ItemType Directory -Path '{REMOTE_BASE}' | Out-Null")
    for local,name in [(checked(binary),'diagnostic.exe'),(Path(__file__),'runner.py'),
        (Path(__file__).with_name('public_training_dispatch_v2.py'),'public_training_dispatch_v2.py'),
        (Path(__file__).with_name('public_evaluation_dispatch_v1.py'),'public_evaluation_dispatch_v1.py')]:
        subprocess.run(['scp','-q',str(local),f'{REMOTE}:{REMOTE_BASE}/{name}'],check=True,timeout=60)
    setup=time.monotonic()-setup
    remote_binary=dict(path=REMOTE_BASE+'/diagnostic.exe',sha256=binary['sha256'])
    def execute(label,chosen,workers,remote=False):
        current=inventory('computehost' if remote else 'desktop');assert not current['active']
        mapped=json.loads(json.dumps(chosen))
        if remote:
            for job in mapped:
                for f in [job['command']['checkpoint'],*job['command']['trajectories']]:
                    normalized=Path(f['path']).as_posix();assert normalized.startswith(LOCAL_DATA+'/')
                    f['path']=normalized.replace(LOCAL_DATA,REMOTE_DATA,1)
        spec=dict(root=REMOTE_BASE+'/'+label if remote else str(root/label),binary=remote_binary if remote else binary,jobs=mapped,workers=workers)
        begin=time.monotonic()
        if not remote:r=worker(spec)
        else:
            write(root/(label+'-request.json'),spec)
            subprocess.run(['scp','-q',str(root/(label+'-request.json')),f'{REMOTE}:{REMOTE_BASE}/{label}.json'],check=True,timeout=60)
            script=f'''$created=@()
try {{
 foreach ($drive in @('D','E')) {{
  if (Test-Path -LiteralPath "${{drive}}:/") {{throw 'preserve existing drive'}}
  & subst "${{drive}}:" "C:/mtg-node/public-stack-panel-001/inputs/$drive"
  if ($LASTEXITCODE -ne 0) {{throw 'mapping failed'}}
  $created+=$drive
 }}
 & python '{REMOTE_BASE}/runner.py' --worker '{REMOTE_BASE}/{label}.json'
 if ($LASTEXITCODE -ne 0) {{throw 'diagnostic failed'}}
}} finally {{ foreach ($drive in $created) {{ & subst "${{drive}}:" /D }} }}'''
            r=json.loads(ssh(script,timeout=240))
            write(root/(label+'-result.json'),r)
        r['wall_seconds']=time.monotonic()-begin
        return r
    sample=[jobs[i] for i in [0,3,6,9]]
    cases={};reference=None
    for host in ['desktop','computehost']:
        for workers in [1,4]:
            label=f'{host}-w{workers}'
            r=execute(label,sample,workers,host=='computehost')
            sig=[j['signature'] for j in r['jobs']]
            assert reference is None or sig==reference,'saved scoring differs with placement'
            reference=sig;cases[label]=r
            print(dict(stage='timing',case=label,seconds=r['wall_seconds']),flush=True)
    def both(label,chosen):
        begin=time.monotonic()
        with ThreadPoolExecutor(max_workers=2) as pool:
            left=pool.submit(execute,label+'-desktop',chosen[::2],4,False)
            right=pool.submit(execute,label+'-computehost',chosen[1::2],4,True)
            lr,rr=left.result(),right.result()
        return dict(jobs=sorted(lr['jobs']+rr['jobs'],key=lambda j:j['id']),workers=8,
            seconds=max(lr['seconds'],rr['seconds']),wall_seconds=time.monotonic()-begin)
    combined=both('both-w4',sample)
    assert [j['signature'] for j in combined['jobs']]==reference
    cases['both-w4']=combined
    # Entire 10-job diagnostic is small; include measured per-case transport and
    # setup before choosing a remote placement. No simulated gameplay involved.
    forecasts={label:2.5*r['seconds']+(r['wall_seconds']-r['seconds'])+(setup if label.startswith(('computehost','both')) else 0) for label,r in cases.items()}
    selected=min(forecasts,key=forecasts.get)
    write(root/'compute-choice.json',dict(cases=cases,forecasts=forecasts,selected=selected,remote_setup_seconds=setup,
        reason='Fastest measured complete-job allocation including both-PC split and transport. Ten total jobs; no larger campaign.'))
    # Qualification and final selection must remain bound to the original inputs.
    saved=read(root/'manifest.json');assert saved['source']==pin(__file__) and saved['jobs']==jobs and saved['binary']==pin(checked(binary))
    r=both('full',jobs) if selected=='both-w4' else execute('full',jobs,4 if selected.endswith('w4') else 1,selected.startswith('computehost'))
    assert len(r['jobs'])==10 and sum(len(j['signature']['inputs']) for j in r['jobs'])==100
    write(root/'result.json',dict(complete=True,selected=selected,archives=100,jobs=r['jobs'],seconds=r['wall_seconds'],
        total_seconds=time.monotonic()-start,manifest=pin(root/'manifest.json'),choice=pin(root/'compute-choice.json')))
    write(root/'owners-after.json',{h:inventory(h) for h in ['desktop','computehost']})
    print(dict(complete=True,archives=100,seconds=r['wall_seconds']),flush=True)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--root',type=Path);p.add_argument('--worker',type=Path);a=p.parse_args()
    if a.worker:print(json.dumps(worker(read(a.worker))))
    else:main(a.root)
