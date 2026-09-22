"""Summarize the complete frozen-policy control, never an advancement gate."""
import argparse
import hashlib
import json
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument('root', type=Path)
a = p.parse_args()
done = json.loads((a.root / 'completion.json').read_bytes())
assert done['complete'] and done['byte_identical'] and len(done['outputs']) == 2
for ref in done['outputs']:
    assert hashlib.sha256(Path(ref['path']).read_bytes()).hexdigest() == ref['sha256']
assert done['outputs'][0]['sha256'] == done['outputs'][1]['sha256']
result = json.loads(Path(done['outputs'][0]['path']).read_bytes())
assert result['positions'] == len(result['records']) == 8
assert result['source']['checkpoint']['sha256'] == '88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1'
assert {(r['actor'], r['has_second_bolt'], r['reverse']) for r in result['records']} == {
    (s, b, h) for s in (0, 1) for b in (False, True) for h in (False, True)}
summary = []
for r in result['records']:
    if r['reverse']:
        continue
    summary.append(dict(actor=r['actor'], second_bolt=r['has_second_bolt'],
        face_probability=r['probabilities'][r['face_index']],
        removal_probability=r['probabilities'][r['removal_index']], value=r['value'],
        argmax=max(range(len(r['probabilities'])), key=lambda i: r['probabilities'][i]),
        face_index=r['face_index'], removal_index=r['removal_index'],
        face_outcome=r['signature']['face_line']['outcome'],
        removal_outcome=r['signature']['removal_line']['outcome']))
land, bolt = [r for r in result['records'] if r['actor'] == 0 and not r['reverse']]
x, y = land['signature']['tensor'], bolt['signature']['tensor']
out = dict(complete=True, promotion_gate=False, new_training_runs=0,
    distinct_hand_states=2, rows=summary, source=result['source'],
    result_sha256=done['outputs'][0]['sha256'],
    changed_tensor_fields=[k for k in x if x[k] != y[k]],
    changed_state_indices=[i for i, (v, w) in enumerate(zip(x['state'], y['state'])) if v != w],
    object_card_ids=[x['object_card_ids'], y['object_card_ids']],
    value_difference=bolt['value']-land['value'],
    non_claim='Synthetic familiar-card continuation only. No natural prevalence, isolated embedding cause, unique optimal-action proof, training-seed inference or whole-match gain.')
with (a.root / 'analysis.json').open('x', encoding='utf8') as f:
    json.dump(out, f, indent=2)
print(json.dumps(dict(rows=summary, changed_tensor_fields=out['changed_tensor_fields'],
                     changed_state_scalars=len(out['changed_state_indices'])), indent=2))
