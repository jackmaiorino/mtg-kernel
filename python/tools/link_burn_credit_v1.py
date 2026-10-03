"""Link existing batch credit to decoded action tensors; no model or games."""
import hashlib,json,struct,zipfile
from pathlib import Path
B=Path('E:/mtg-meta-recovery-20260921')
credit_path=B/'archived-gae-batch-001.json'
credit=json.loads(credit_path.read_bytes())
out=B/'burn-credit-link-001.json'
assert not out.exists()
def floats(bits):return [struct.unpack('<f',struct.pack('<I',x))[0] for x in bits]
def decode(t):
    af=floats(t['action_features']);rf=floats(t['action_ref_features'])
    assert len(af)%195==0 and len(rf)==25*len(t['action_ref_card_ids'])
    actions=[]
    for i in range(len(af)//195):
        f=af[i*195:(i+1)*195]
        kinds=[k for k,x in enumerate(f[:27]) if x==1]
        assert len(kinds)==1
        sources=[t['action_ref_card_ids'][j]-1 for j,a in enumerate(t['action_ref_action_indices']) if a==i and rf[j*25]==1]
        actions.append(dict(kind=kinds[0],sources=sources,opponent_target=f[46]==1,self_target=f[45]==1,card_target=f[44]==1))
    return actions
# Independent semantic check against an engine-captured natural root.
capture_path=B/'burn-recorder-natural-002/on.json'
root=json.loads(capture_path.read_bytes())['burn_audit']['roots'][0]
decoded=decode(root['tensor_bits'])
for a,semantic in zip(decoded,root['record']['visible']['ordered_actions']):
    assert a['kind']==6 and a['sources']==[semantic['source']['card_db_id']]
    assert a['opponent_target']==(semantic['target'].get('player')=='p0')
    assert a['self_target']==(semantic['target'].get('player')=='p1')
lookup={(r['slot'],r['physical_decision_id']):r for r in credit['rows']}
family={37:'Fireblast',41:'Galvanic Blast',63:'Lava Dart',66:'Lightning Bolt'}
rows=[]
for slot,ref in enumerate(credit['episode_refs']):
    with zipfile.ZipFile(ref[0]) as z:raw=z.read(ref[1])
    assert hashlib.sha256(raw).hexdigest()==ref[2]
    episode=json.loads(raw)
    for d in episode['decisions']:
        if d['actor']!=episode['episode']['learner_seat']:continue
        actions=decode(d['tensor'])
        if not actions or not all(a['kind']==6 and len(a['sources'])==1 and a['sources'][0] in family for a in actions):continue
        c=lookup[slot,d['physical_decision_id']]
        assert all(a['sources']==actions[0]['sources'] for a in actions)
        hand=[token-1 for token,g in zip(d['tensor']['object_card_ids'],d['tensor']['object_groups']) if g==0 and token-1 in family]
        selected=actions[d['selected']]
        rows.append(dict(slot=slot,step=d['step'],source=family[actions[0]['sources'][0]],hand_burn=[family[x] for x in hand],
            selected=selected,face_available=any(a['opponent_target'] for a in actions),credit=c,
            value_and_rounding_remainder=c['raw_advantage']-c['terminal_reward']*c['terminal_direct_coefficient']))
summary=dict(target_substeps=len(rows),physical_groups=len({(r['slot'],r['credit']['physical_decision_id']) for r in rows}),
    with_hand_burn=sum(bool(r['hand_burn']) for r in rows),face_selected=sum(r['selected']['opponent_target'] for r in rows),
    card_selected=sum(r['selected']['card_target'] for r in rows),positive_normalized=sum(r['credit']['normalized_advantage']>0 for r in rows))
result=dict(complete=True,summary=summary,rows=rows,credit_sha256=hashlib.sha256(credit_path.read_bytes()).hexdigest(),
    engine_capture_sha256=hashlib.sha256(capture_path.read_bytes()).hexdigest(),engine_semantic_check=True,
    limits='First batch only, no certified tactical labels or unchosen-action credit. Tensor identity mapping checked on one natural Bolt menu. Remainder includes bootstrapped values and f32 roundoff, not a causal effect or normalized credit decomposition.')
out.write_text(json.dumps(result,indent=2))
print(json.dumps(summary))
for r in rows:print(r['slot'],r['step'],r['source'],r['hand_burn'],r['selected'],round(r['credit']['normalized_advantage'],5),round(r['credit']['terminal_direct_coefficient'],6))
