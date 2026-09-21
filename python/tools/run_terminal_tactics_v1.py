"""Bounded exact natural replay and public terminal witnesses across fixed jobs."""
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
TOOLS=Path('E:/mtg-meta-recovery-20260921/public-terminal-tactics-tools-002')
REMOTE_BASE='C:/mtg-node/public-terminal-tactics-001'
LOCAL_DATA='D:/mtg-training-working/stack-screen-training-native-0/haleyspc/recovered/jobs/structured/outputs'
REMOTE_DATA='C:/mtg-node/stack-screen-training-native-0/jobs/structured/outputs'

def signature(result):
    # Host-specific archive paths are metadata; hashes, witnesses and scores
    # must otherwise agree exactly across every qualified placement.
    roots=json.loads(json.dumps(result['roots']))
    for root in roots:root['trajectory']['path']=root['trajectory']['sha256']
    return dict(roots=roots,exact_decisions=result['exact_decisions'],branches=result['branches'],
        inputs=[r['trajectory']['sha256'] for r in result['archives']],
        exact=[r['exact_decisions'] for r in result['archives']],model=result['model'])

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
    probe=read('E:/mtg-meta-recovery-20260921/public-terminal-tactics-probe-001/result.json')
    assert probe['complete'] and probe['byte_identical']
    owners={h:inventory(h) for h in ['jack','haleyspc']}
    assert all(not s['active'] for s in owners.values())
    write(root/'owners-before.json',owners)
    cloud=pin('E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json')
    build=read(TOOLS/'completion.json');assert build['complete']
    binary=build['binaries']['public_terminal_tactics_v1'];checked(binary)
    assert probe['binary']==binary
    audit=read(PILOT/'training-audit.json');assert audit['complete']
    report=read(checked(audit['arms']['structured']['report']));m=read(PILOT/'manifest.json')
    jobs=[]
    for update in range(19,200,20):
        trajectories=[report['outputs'][f'{update:04}/episode-{e:03}.json'] for e in range(10)]
        for f in trajectories:checked(f)
        jobs.append(dict(id=f'u{update:03}',command=dict(source=m['source'],trajectories=trajectories)))
    write(root/'manifest.json',dict(jobs=jobs,binary=binary,build=pin(TOOLS/'completion.json'),
        source=pin(__file__),design=pin(Path(__file__).resolve().parents[2]/'docs/public_terminal_tactic_audit_20260921.md'),
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
        current=inventory('haleyspc' if remote else 'jack');assert not current['active']
        mapped=json.loads(json.dumps(chosen))
        if remote:
            for job in mapped:
                for f in job['command']['trajectories']:
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
    for host in ['jack','haleyspc']:
        for workers in [1,4]:
            label=f'{host}-w{workers}'
            r=execute(label,sample,workers,host=='haleyspc')
            sig=[j['signature'] for j in r['jobs']]
            assert reference is None or sig==reference,'saved scoring differs with placement'
            reference=sig;cases[label]=r
            print(dict(stage='timing',case=label,seconds=r['wall_seconds']),flush=True)
    def both(label,chosen):
        begin=time.monotonic()
        with ThreadPoolExecutor(max_workers=2) as pool:
            left=pool.submit(execute,label+'-jack',chosen[::2],4,False)
            right=pool.submit(execute,label+'-haley',chosen[1::2],4,True)
            lr,rr=left.result(),right.result()
        return dict(jobs=sorted(lr['jobs']+rr['jobs'],key=lambda j:j['id']),workers=8,
            seconds=max(lr['seconds'],rr['seconds']),wall_seconds=time.monotonic()-begin)
    combined=both('both-w4',sample)
    assert [j['signature'] for j in combined['jobs']]==reference
    cases['both-w4']=combined
    # Entire 10-job diagnostic is small; include measured per-case transport and
    # setup before choosing a remote placement. No simulated gameplay involved.
    forecasts={label:2.5*r['seconds']+(r['wall_seconds']-r['seconds'])+(setup if label.startswith(('haley','both')) else 0) for label,r in cases.items()}
    selected=min(forecasts,key=forecasts.get)
    write(root/'compute-choice.json',dict(cases=cases,forecasts=forecasts,selected=selected,remote_setup_seconds=setup,
        reason='Fastest measured complete-job allocation including both-PC split and transport. Ten total jobs; no larger campaign.'))
    # Qualification and final selection must remain bound to the original inputs.
    saved=read(root/'manifest.json');assert saved['source']==pin(__file__) and saved['jobs']==jobs and saved['binary']==pin(checked(binary))
    r=both('full',jobs) if selected=='both-w4' else execute('full',jobs,4 if selected.endswith('w4') else 1,selected.startswith('haley'))
    assert len(r['jobs'])==10 and sum(len(j['signature']['inputs']) for j in r['jobs'])==100
    write(root/'result.json',dict(complete=True,selected=selected,archives=100,jobs=r['jobs'],seconds=r['wall_seconds'],
        total_seconds=time.monotonic()-start,manifest=pin(root/'manifest.json'),choice=pin(root/'compute-choice.json')))
    write(root/'owners-after.json',{h:inventory(h) for h in ['jack','haleyspc']})
    print(dict(complete=True,archives=100,seconds=r['wall_seconds']),flush=True)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--root',type=Path);p.add_argument('--worker',type=Path);a=p.parse_args()
    if a.worker:print(json.dumps(worker(read(a.worker))))
    else:main(a.root)
