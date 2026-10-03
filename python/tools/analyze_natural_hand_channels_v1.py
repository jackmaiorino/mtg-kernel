"""Hash-checked descriptive analysis of32 frozen natural-root ablations."""
import argparse,hashlib,json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('root',type=Path);a=p.parse_args()
done=json.loads((a.root/'completion.json').read_bytes());assert done['complete'] and done['byte_identical']
assert len(done['outputs'])==2 and done['outputs'][0]['sha256']==done['outputs'][1]['sha256']
for ref in done['outputs']:assert hashlib.sha256(Path(ref['path']).read_bytes()).hexdigest()==ref['sha256']
r=json.loads(Path(done['outputs'][0]['path']).read_bytes());assert r['positions']==8 and r['combinations']==len(r['records'])==32
rows=[]
for step in sorted({x['step'] for x in r['records']}):
    cells={(x['token_replaced'],x['digest_zeroed']):x for x in r['records'] if x['step']==step};assert len(cells)==4
    base=cells[False,False];top=max(range(len(base['logits'])),key=lambda i:base['logits'][i]);effects={}
    for key,label in [((True,False),'token'),((False,True),'digest'),((True,True),'both')]:
        x=cells[key];delta=[v-w for v,w in zip(x['logits'],base['logits'])]
        effects[label]=dict(pairwise_logit_shift_span=max(delta)-min(delta),
            argmax_changed=max(range(len(x['logits'])),key=lambda i:x['logits'][i])!=top,
            baseline_top_probability_change=x['probabilities'][top]-base['probabilities'][top],
            value_change=x['value']-base['value'])
    rows.append(dict(step=step,original_token=base['original_token'],menu_size=len(base['logits']),baseline_top_probability=base['probabilities'][top],effects=effects))
assert len(rows)==8
out=dict(complete=True,result_sha256=done['outputs'][0]['sha256'],rows=rows,
    flips={k:sum(x['effects'][k]['argmax_changed'] for x in rows) for k in ('token','digest','both')},
    limits='One episode, one replaced card identity, artificial interventions of unequal size, no tactical labels or independent training samples.')
with (a.root/'analysis.json').open('x',encoding='utf8') as f:json.dump(out,f,indent=2)
print(json.dumps(dict(flips=out['flips'],ranges={k:[min(x['effects'][k]['pairwise_logit_shift_span'] for x in rows),max(x['effects'][k]['pairwise_logit_shift_span'] for x in rows)] for k in out['flips']}),indent=2))
