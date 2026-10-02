"""Confidence on the fixed, exactly replayed 100-archive diagnostic sample."""
from collections import defaultdict
from concurrent.futures import ProcessPoolExecutor
import hashlib
import json
from pathlib import Path
import time
import numpy as np

ROOT=Path('E:/mtg-meta-recovery-20260921/public-stack-sensitivity-001')

def one(pin):
    raw=Path(pin['path']).read_bytes();assert hashlib.sha256(raw).hexdigest()==pin['sha256']
    t=json.loads(raw);seat=t['episode']['learner_seat'];groups=defaultdict(list)
    for row,aux in zip(t['decisions'],t['auxiliary']):
        if row['actor']!=seat or len(row['logits'])<=1:continue
        logits=np.asarray(row['logits'],dtype=np.uint32).view(np.float32).astype(np.float64)
        assert np.isfinite(logits).all()
        ordered=np.sort(logits);p=np.exp(logits-logits.max());p/=p.sum()
        entropy=-np.sum(p[p>0]*np.log(p[p>0]))/np.log(len(p))
        values=[float(p.max()),float(ordered[-1]-ordered[-2]),float(entropy)]
        special=any(r['features'][1]==1 and (r['features'][13]==1 or r['features'][15]==1 or r['features'][25]==0 or any(r['features'][289:305])) for r in aux['rows'])
        category='empty' if aux['stack_items']==0 else 'special' if special else 'default'
        groups['all'].append(values);groups[category].append(values)
    return dict(groups)

def main():
    start=time.monotonic();result=json.loads((ROOT/'result.json').read_text());assert result['complete']
    plan=ROOT/'probability-diagnostic-plan.md';assert plan.exists()
    pins=[]
    for job in result['jobs']:
        raw=Path(job['result']['path']).read_bytes();assert hashlib.sha256(raw).hexdigest()==job['result']['sha256']
        pins.extend(r['trajectory'] for r in json.loads(raw)['archives'])
    assert len(pins)==100 and len({p['sha256'] for p in pins})==100
    groups=defaultdict(list)
    with ProcessPoolExecutor(max_workers=8) as pool:
        for row in pool.map(one,pins,chunksize=4):
            for k,v in row.items():groups[k].extend(v)
    summary={}
    for k,values in groups.items():
        a=np.asarray(values);summary[k]=dict(choices=len(a),top_probability_ge_099=int((a[:,0]>=.99).sum()),
            top_probability_ge_0999=int((a[:,0]>=.999).sum()),quantile_levels=[0,.25,.5,.75,.95,1],
            top_probability_quantiles=np.quantile(a[:,0],[0,.25,.5,.75,.95,1]).tolist(),
            top_two_logit_gap_quantiles=np.quantile(a[:,1],[0,.25,.5,.75,.95,1]).tolist(),
            normalized_entropy_quantiles=np.quantile(a[:,2],[0,.25,.5,.75,.95,1]).tolist())
        expected=sum(j['signature']['groups'].get(k,{}).get('choices',0) for j in result['jobs'])
        assert len(a)==expected
    value=dict(complete=True,archives=100,groups=summary,seconds=time.monotonic()-start,
        plan_sha256=hashlib.sha256(plan.read_bytes()).hexdigest(),script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        sensitivity_result_sha256=hashlib.sha256((ROOT/'result.json').read_bytes()).hexdigest())
    with (ROOT/'confidence.json').open('x') as f:json.dump(value,f,indent=2)
    print(json.dumps(value,indent=2))

if __name__=='__main__':main()
