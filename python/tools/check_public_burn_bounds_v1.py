"""Check the recorded interval arithmetic and extract a no-unknown win proof."""
import argparse,collections,hashlib,json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('output_json',type=Path);a=p.parse_args()
r=json.loads(a.output_json.read_bytes());root=r['burn_audit']['roots'][0]
counts=collections.Counter()
def verify(t):
    lo,hi,reason,branches=t
    if branches:
        assert reason in (7,8)
        for _,c in branches:verify(c)
        reduce=max if reason==7 else min
        assert lo==reduce(c[0] for _,c in branches) and hi==reduce(c[1] for _,c in branches)
    elif reason==2:assert lo==hi and lo in (-1,0,1)
    else:assert (lo,hi)==(-1,1)
def proof(t):
    lo,hi,reason,branches=t;assert lo==1;counts[reason]+=1
    if reason==2:return
    assert reason in (7,8)
    children=[c for _,c in branches]
    if reason==7:children=[next(c for c in children if c[0]==1)]
    for c in children:proof(c)
for outcome in root['audit']['outcomes']:verify(outcome['tree'])
actor=root['record']['actor'];opponent='p1' if actor=='p0' else 'p0'
face=next(i for i,x in enumerate(root['record']['visible']['ordered_actions']) if x['target'].get('player')==opponent)
proof(root['audit']['outcomes'][face]['tree'])
print(json.dumps(dict(source_sha256=hashlib.sha256(a.output_json.read_bytes()).hexdigest(),face_index=face,proof_counts=dict(counts),interval_arithmetic_verified=True)))
