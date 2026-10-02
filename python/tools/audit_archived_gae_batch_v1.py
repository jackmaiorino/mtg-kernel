"""Reconstruct one complete archived update without model execution or mutation."""
import hashlib
import json
import struct
import time
import zipfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import numpy as np

B = Path('E:/mtg-meta-recovery-20260921')
M = Path('E:/mtg-postboard-campaign-20260921/control-variance-001/replica-1/control-variance-replica-1-training/archive.json')
OUT = B / 'archived-gae-batch-001.json'
assert not OUT.exists()
manifest = json.loads(M.read_bytes())
refs = [(s['archive']['path'], n, h) for s in manifest['shards'] for n,h in s['files'].items()]
def ref(suffix):
    found = [r for r in refs if r[1].endswith(suffix)]
    assert len(found) == 1
    return found[0]
def read(r):
    with zipfile.ZipFile(r[0]) as z:
        raw = z.read(r[1])
    assert hashlib.sha256(raw).hexdigest() == r[2]
    return json.loads(raw),len(raw)
request_ref = ref('jobs/a/request.json')
receipt_ref = ref('jobs/a/outputs/0000/receipt.json')
request,_ = read(request_ref);receipt,_ = read(receipt_ref)
config = request['config'];schedule = config['updates'][0]
assert len(schedule)==10 and not config['inputs_enabled']
jobs=[ref(f'jobs/a/outputs/0000/episode-{i:03}.json') for i in range(10)]
start=time.monotonic();serial=[read(r) for r in jobs];serial_s=time.monotonic()-start
start=time.monotonic()
with ThreadPoolExecutor(4) as pool:
    parallel=list(pool.map(read,jobs))
parallel_s=time.monotonic()-start
assert serial==parallel
f=np.float32
def value(bits):return f(struct.unpack('<f',struct.pack('<I',bits))[0])
def bits(x):return struct.unpack('<I',struct.pack('<f',x))[0]
gamma=f(config['gamma']);lam=f(config['lambda']);rows=[];raw=[]
for slot,((t,_),episode) in enumerate(zip(serial,schedule)):
    assert t['episode']==episode
    assert t['optimizer_state_sha256']==receipt['before_state_sha256']
    assert t['terminal']['terminal_classification']=='natural'
    ds=t['decisions'];groups=[];i=0
    while i<len(ds):
        first=ds[i];end=i+first['substep_count'];group=ds[i:end]
        assert len(group)==first['substep_count']
        assert all(x['physical_decision_id']==first['physical_decision_id'] and x['substep_index']==j for j,x in enumerate(group))
        if first['actor']==episode['learner_seat']:groups.append(group)
        i=end
    reward=f(t['terminal']['terminal_reward'][episode['learner_seat']]);nv=f(0);na=f(0)
    results=[None]*len(groups)
    for i in reversed(range(len(groups))):
        v=value(groups[i][0]['value']);r=reward if i==len(groups)-1 else f(0)
        delta=f(f(r+f(gamma*nv))-v)
        advantage=f(delta+f(f(gamma*lam)*na))
        results[i]=advantage;nv=v;na=advantage
    for i,(g,a) in enumerate(zip(groups,results)):
        k=len(groups)-1-i
        row=dict(slot=slot,deck=episode['selected'][episode['learner_seat']]['label'],step=g[0]['step'],
            physical_decision_id=g[0]['physical_decision_id'],substeps=len(g),later_learner_groups=k,
            value=float(value(g[0]['value'])),terminal_reward=float(reward),raw_advantage=float(a),
            terminal_direct_coefficient=float(gamma*lam)**k,
            any_nonforced=any(len(x['logits'])>1 for x in g))
        raw.append(a);rows.append(row)
assert len(rows)==receipt['learner_groups']
assert sum(r['substeps'] for r in rows)==receipt['learner_substeps']
def mean(xs):
    total=f(0)
    for x in xs:total=f(total+x)
    return f(total/f(len(xs)))
def std(xs,mu):
    return f(np.sqrt(mean([f(f(x-mu)*f(x-mu)) for x in xs])))
mu=mean(raw);sigma=std(raw,mu);denom=f(sigma+f(1e-6))
norm=[f(f(a-mu)/denom) for a in raw]
nm=mean(norm);stats=dict(mean=float(nm),std=float(std(norm,nm)),min=float(min(norm)),max=float(max(norm)),clip_fraction=0.)
assert all(bits(v)==bits(receipt['advantage_statistics'][k]) for k,v in stats.items()),(stats,receipt['advantage_statistics'])
for row,a in zip(rows,norm):row['normalized_advantage']=float(a)
choices=[r for r in rows if r['any_nonforced']]
summary=dict(episodes=10,learner_groups=len(rows),choice_groups=len(choices),gamma=float(gamma),lambda_f32=float(lam),
    raw_mean=float(mu),raw_std=float(sigma),statistics_bit_match=True,
    choice_terminal_coefficient_below_001=sum(r['terminal_direct_coefficient']<.01 for r in choices),
    choice_terminal_coefficient_median=float(np.median([r['terminal_direct_coefficient'] for r in choices])),
    normalized_sign_changes=sum((r['raw_advantage']>0)!=(r['normalized_advantage']>0) for r in choices))
result=dict(complete=True,summary=summary,rows=rows,request_ref=request_ref,receipt_ref=receipt_ref,episode_refs=jobs,
    manifest_sha256=hashlib.sha256(M.read_bytes()).hexdigest(),script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    serial_seconds=serial_s,four_reader_seconds=parallel_s,exact_reader_agreement=True,input_bytes=sum(n for _,n in serial),
    limits='One first continuation batch, not g115 historical training census or a hand-fork causal diagnosis. Direct terminal coefficient excludes learned bootstrapped value credit and is pre-normalization. Values can already encode future consequences. No sampled-action counterfactual or per-parameter gradient recovered. Members verified against manifest, whole zip not rehashed.')
OUT.write_text(json.dumps(result,indent=2))
print(json.dumps(dict(summary=summary,serial_seconds=serial_s,four_reader_seconds=parallel_s),indent=2))
