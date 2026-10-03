"""Read-only, pinned full training exposure census; never reads outcomes."""
import argparse
from collections import Counter, defaultdict
from concurrent.futures import ProcessPoolExecutor
import hashlib
import json
from pathlib import Path
import time

ROOT = Path('E:/mtg-postboard-campaign-20260921/public-stack-screen-001')
CONTRACT = Path(__file__).resolve().parents[2]/'data/public_stack_features_v1/contract.json'

def read(path):
    return json.loads(Path(path).read_bytes())

def pin(path):
    path=Path(path)
    return dict(path=str(path),sha256=hashlib.sha256(path.read_bytes()).hexdigest())

def checked(item):
    raw=Path(item['path']).read_bytes()
    assert hashlib.sha256(raw).hexdigest()==item['sha256'],item['path']
    return json.loads(raw)

def write(path,value):
    with Path(path).open('x',encoding='utf-8') as f:json.dump(value,f,indent=2)

def count_item(item):
    t=checked(item['file']);arm=item['arm']
    assert t['schema']=='mtg-kernel-public-stack-trajectory/v1' and t['input_mode']==arm
    assert len(t['decisions'])==len(t['auxiliary'])
    seat=t['episode']['learner_seat'];deck=t['episode']['registered'][seat]['label']
    counters={name:Counter() for name in ['all','choice']};physical=set();episode_flags=set()
    for row,aux in zip(t['decisions'],t['auxiliary']):
        if row['actor']!=seat:
            assert aux is None
            continue
        assert aux is not None and aux['contract_sha256']==item['contract_sha256']
        assert aux['schema']=='mtg-kernel-public-stack-input/v1'
        assert (aux.get('permutation') is not None)==(arm=='permuted')
        baseline=[r for r in aux['rows'] if r['features'][1]==1]
        assert len(baseline)==aux['stack_items']
        flags=set();items=Counter()
        for r in baseline:
            f=r['features'];assert len(f)==312
            assert all(v in [0.,1.] for v in f[12:16]+f[289:305])
            mode=f[25:281];assert sum(mode)==1 and all(v in [0.,1.] for v in mode)
            found=dict(kicked=f[15]==1,flashback=f[13]==1,x_nonzero=any(f[289:305]),mode_nonprimary=mode[0]==0,copy=f[12]==1,madness=f[14]==1)
            for k,v in found.items():
                if v:flags.add(k);items[k]+=1
        if any(k in flags for k in ['kicked','flashback','x_nonzero','mode_nonprimary']):flags.add('special')
        if baseline:flags.add('stack')
        if len(baseline)>1:flags.add('multi_stack')
        physical.add(row['physical_decision_id']);episode_flags.update(flags)
        for name in ['all']+(['choice'] if len(row['logits'])>1 else []):
            c=counters[name];c['decisions']+=1;c['stack_item_observations']+=len(baseline)
            for f in flags:c[f+'_decisions']+=1
            for k,v in items.items():c[k+'_items']+=v
    return dict(arm=arm,deck=deck,update=item['update'],episode=item['episode'],
        file=item['file'],physical_decisions=len(physical),episode_flags=sorted(episode_flags),
        counts={k:dict(v) for k,v in counters.items()})

def run(items,workers):
    start=time.monotonic()
    if workers==1:rows=list(map(count_item,items))
    else:
        with ProcessPoolExecutor(max_workers=workers) as pool:rows=list(pool.map(count_item,items,chunksize=4))
    return rows,time.monotonic()-start

def aggregate(rows):
    groups=defaultdict(Counter)
    for r in rows:
        for key in [r['arm'],r['arm']+'/'+r['deck']]:
            c=groups[key];c['episodes']+=1;c['physical_decisions']+=r['physical_decisions']
            for f in r['episode_flags']:c[f+'_episodes']+=1
            for cat,d in r['counts'].items():c.update({cat+'/'+k:v for k,v in d.items()})
    return {k:dict(v) for k,v in sorted(groups.items())}

def main():
    p=argparse.ArgumentParser();p.add_argument('--root',type=Path,required=True);a=p.parse_args()
    a.root.mkdir();audit=read(ROOT/'training-audit.json');assert audit['complete'] and audit['full_natural_games']==6000
    items=[];reports=[]
    for arm in ['structured','permuted','disabled']:
        report=audit['arms'][arm]['report'];r=checked(report);reports.append(report)
        for name,f in sorted(r['outputs'].items()):
            if '/episode-' in name:
                update=int(name.split('/')[0]);episode=int(Path(name).stem.split('-')[1])
                items.append(dict(arm=arm,update=update,episode=episode,file=f,contract_sha256=pin(CONTRACT)['sha256']))
    assert len(items)==6000 and len({(i['arm'],i['update'],i['episode']) for i in items})==6000
    design=Path(__file__).resolve().parents[2]/'docs/public_stack_trace_diagnostic_20260921.md'
    write(a.root/'manifest.json',dict(audit=pin(ROOT/'training-audit.json'),reports=reports,items=items,
        implementation=pin(__file__),design=pin(design),no_outcome_selection=True))
    # Ten updates times three episode slots per arm, selected without opening data.
    sample=[i for i in items if i['update'] in range(19,200,20) and i['episode'] in [0,4,9]]
    assert len(sample)==90
    serial,st=run(sample,1);parallel,pt=run(sample,8);assert serial==parallel
    workers=8 if pt<st else 1
    write(a.root/'throughput.json',dict(files=90,serial_seconds=st,parallel_seconds=pt,workers=workers,exact_counts=True,
        placement='Local existing SSD archive; no native simulation, model scoring or GPU use. Remote transfer of the census is unnecessary.',
        sample=[i['file'] for i in sample]))
    print(dict(stage='qualified',serial_seconds=st,parallel_seconds=pt,workers=workers),flush=True)
    rows,elapsed=run(items,workers)
    assert len(rows)==6000 and elapsed<600
    result=dict(complete=True,episodes=6000,seconds=elapsed,workers=workers,groups=aggregate(rows),
        manifest=pin(a.root/'manifest.json'),non_claim='Repeated on-policy exposure, not independent events, game utility or causal strength.')
    write(a.root/'episode-counts.json',rows);write(a.root/'result.json',result)
    print(dict(complete=True,episodes=6000,seconds=elapsed),flush=True)

if __name__=='__main__':main()
