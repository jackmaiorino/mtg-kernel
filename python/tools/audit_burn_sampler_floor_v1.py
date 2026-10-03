"""Compare exact recorded sampling masses to unclamped loss probabilities."""
import hashlib,json,math,struct
from pathlib import Path
from generate_fast_sampler_candidate_vectors_v1 import candidate_masses
B=Path('E:/mtg-meta-recovery-20260921');rows=[]
for folder in ('burn-tree-natural-002','hand-burn-tree-natural-001'):
 p=B/folder/'on.json';r=json.loads(p.read_bytes())['burn_audit']['roots'][0]
 bits=r['logits_bits'];z=[struct.unpack('<f',struct.pack('<I',b))[0] for b in bits];m=candidate_masses(tuple(bits))
 recorded=tuple(int(x) for x in r['record']['behavior']['mass_numerators'])
 if m!=recorded:raise RuntimeError('exact behavior masses differ')
 top=max(z);weights=[math.exp(x-top) for x in z];soft=[w/sum(weights) for w in weights];sample=[x/2**64 for x in m]
 actor=r['record']['actor'];opp='p0' if actor=='p1' else 'p1'
 face=next(i for i,a in enumerate(r['audit']['actions']) if a['target'].get('player')==opp)
 selected=r['record']['behavior']['selected_index']
 rows.append(dict(root=folder,source_sha256=hashlib.sha256(p.read_bytes()).hexdigest(),logits=z,selected=selected,face=face,
    exact_mass_match=True,clamped_indices=[i for i,x in enumerate(z) if top-x>=16],face_gap=top-z[face],face_sampling_probability=sample[face],face_softmax_probability=soft[face],
    face_probability_ratio=sample[face]/soft[face],total_variation=.5*sum(abs(a-b) for a,b in zip(sample,soft)),
    unclamped_nll_face_gradient_per_unit_advantage_when_face_selected=soft[face]-1,
    unclamped_nll_face_gradient_per_unit_advantage_when_actual_selected=soft[face]-(selected==face)))
p=B/'burn-sampler-floor-001.json'
with p.open('x') as f:json.dump(dict(complete=True,rows=rows,scope='Exact frozen sampler reconstruction and scalar loss derivative only. No CUDA gradient experiment, historical credit claim, sampler change, or gate.'),f,indent=2)
for r in rows:print(json.dumps(r))
