"""One-time bounded clean ABBA queued launcher. Never retries a started phase."""
import argparse
from datetime import datetime,timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
ROOT=Path(__file__).parent
PYTHON=Path('D:/mtg-kernel-uv-python-019f63a2/cpython-3.13.14-windows-x86_64-none/python.exe')
INVENTORY=Path('C:/Users/Jack/.codex/temporary/training-speedups-20261009-inventory.py')

def require(ok,msg):
    if not ok:raise ValueError(msg)

def pin(p):
    p=Path(p)
    with p.open('rb') as f:h=hashlib.file_digest(f,'sha256').hexdigest()
    return dict(path=str(p),sha256=h)

def now():return datetime.now(timezone.utc).isoformat()
def read(p):return json.loads(Path(p).read_bytes())

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--decks',type=Path,required=True)
    ap.add_argument('--t1',type=Path,required=True)
    args=ap.parse_args()
    require(Path(sys.executable).resolve()==PYTHON.resolve(),'pinned Python313 required')
    out=ROOT/'formal-v4-launcher-repaired'
    require(not out.exists() and not (ROOT/'formal-v4').exists(),'preserve existing attempts; no preparation retry')
    sources=[pin(x) for x in (Path(__file__),INVENTORY,ROOT/'prepare_formal_v4_repaired.py',ROOT/'prepare_desktop.py',ROOT/'run_formal_clean_v4.py',ROOT/'wait_canonical_free.py',ROOT.parent/'resume_qualifications.py',ROOT/'formal_case_driver_v4.py',ROOT/'observe_native_affinity_v4.py',ROOT/'formal_launcher_adapter.py',args.decks,args.t1,PYTHON)]
    for variant in ('baseline','candidate'):
        sources += [pin(x) for x in (ROOT/variant/'python/tools').glob('*.py')]
    out.mkdir();state=dict(schema='training-speedup-clean-queued-launch/v1',complete=False,started_utc=now(),sources=sources,phases=[],queue_waits=[])
    def save():
        temp=out/'state.pending'
        with temp.open('w',encoding='utf-8') as f:json.dump(state,f,indent=2);f.flush();os.fsync(f.fileno())
        os.replace(temp,out/'state.json')
    def stable():
        for ref in sources:require(pin(ref['path'])==ref,'queued source changed: '+ref['path'])
    def run(name,argv,code=None):
        stable();record=dict(phase=name,argv=argv,started_utc=now());state['phases'].append(record);save();start=time.monotonic()
        with (out/(name+'.stdout')).open('xb') as stdout,(out/(name+'.stderr')).open('xb') as stderr:
            child=subprocess.run(argv,input=code.encode('utf-8') if code else None,stdout=stdout,stderr=stderr)
        record.update(seconds=time.monotonic()-start,exit_code=child.returncode,finished_utc=now());save()
        require(child.returncode==0,'phase failed; no automatic retry: '+name)
    sys.path.insert(0,str(ROOT));import wait_canonical_free
    save();deadline=time.monotonic()+7200;attempt=0
    try:
        while True:
            def changed(value):state['waiting']=value;save()
            queue=wait_canonical_free.wait_free(changed,deadline);state['queue_waits'].append(queue);state.pop('waiting',None);save()
            stable();attempt+=1;inventory=out/('inventory-'+str(attempt)+'.json')
            text=INVENTORY.read_text(encoding='utf-8-sig')
            require(text.count("'inventory-20261009.json'")==1,'inventory output constant differs')
            text=text.replace("'inventory-20261009.json'",repr(inventory.as_posix()))
            run('inventory-'+str(attempt),[str(PYTHON),'-B','-'],text)
            data=read(inventory);require(set(data)=={'desktop','computehost','runpod'},'three-host inventory incomplete')
            # A free-validation race may return to waiting only before preparation.
            if data['desktop'].get('reservations') or wait_canonical_free.reservation_api().status().get('state')!='free':
                require(time.monotonic()<deadline,'two-hour queued wait expired');continue
            state['inventory']=pin(inventory);save();break
        run('prepare',[str(PYTHON),'-B',str(ROOT/'prepare_formal_v4_repaired.py'),'prepare','--inventory',str(inventory)])
        prepared=read(ROOT/'formal-v4/prepare.plan.json');state['prepared_plan']=pin(ROOT/'formal-v4/prepare.plan.json');save()
        for i,case in enumerate(prepared['commands'],1):
            require(case['argv'][0]==str(PYTHON) and case['argv'][-2]=='check-choice','public pinned check-choice required')
            run('check-choice-'+str(i),case['argv'])
        run('seal',prepared['seal_argv'])
        seal=ROOT/'formal-v4/seal.plan.json';state['sealed_plan']=pin(seal);save()
        argv=[str(PYTHON),'-B',str(ROOT/'run_formal_clean_v4.py'),'--plan',str(seal),'--decks',str(args.decks),'--t1',str(args.t1)]
        run('dryrun',argv)
        run('execute',argv+['--execute'])
        terminal=ROOT/'formal-v4/coordinator/state.json';cold=Path('E:/training-speedups-20261009/formal-v4-final-state.json')
        require(read(terminal)['complete'] and pin(terminal)['sha256']==pin(cold)['sha256'],'terminal coordinator lacks verified E copy')
        state.update(complete=True,coordinator_state=pin(terminal),cold_state=pin(cold),finished_utc=now());save()
    except BaseException as error:
        state.update(error=type(error).__name__+': '+str(error),finished_utc=now());save();raise

if __name__=='__main__':main()
