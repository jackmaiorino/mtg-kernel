"""Read public resources at nominal hand-combo roots, not legal-win labels."""
import hashlib,json,zipfile
from pathlib import Path
B=Path('E:/mtg-meta-recovery-20260921');cp=B/'natural-burn-candidate-inspection-001.json'
out=B/'hand-burn-resources-001.json';assert not out.exists()
cards_path=Path('E:/mtg-kernel-public-stack-features-codex/data/cards_v1.json')
cards=json.loads(cards_path.read_bytes())['cards'];rows=[]
for old in json.loads(cp.read_bytes())['rows']:
    c=old['candidate']
    if not c['nominal_hand_dependent']:continue
    path=Path(c['bundle']['path']);raw=path.read_bytes();assert hashlib.sha256(raw).hexdigest()==c['bundle']['sha256']
    with zipfile.ZipFile(path) as z:r=json.loads(z.read('result.json'))
    record=next(x for x in r['collected']['trajectory']['games'][c['game_index']]['decisions'] if x['decision_index']==c['decision_index'])
    o=record['visible']['observation'];p=o['projection'];actor=['p0','p1'].index(record['actor']);battle=p['battlefield'][actor]
    hand=[cards[x['stable']['card_db_id']]['name'] for x in o['own_hand']]
    own=[dict(name=cards[x['stable']['card_db_id']]['name'],tapped=x['tapped'],artifact=x['characteristics']['type_flags']['artifact']) for x in battle]
    pending=p['engine_context']['pending_cast'];assert pending and pending['chosen_targets']==[]
    rows.append(dict(match=c['match'],game_array_index=c['game_index'],decision_index=c['decision_index'],actor=record['actor'],active_player=p['active_player'],
        source=c['source'],opponent_life=c['opponent_life'],own_life=p['life_totals'][actor],hand=hand,battlefield=own,
        mana_pool=p['mana_pools'][actor],artifacts=sum(x['artifact'] for x in own),mountains=sum(x['name']=='Mountain' for x in own),
        pending_cast_mode=pending['cast_mode'],pending_origin=pending['origin_zone'],pending_sacrifices=pending['sacrifice_chosen'],
        nominal_ceiling=c['nominal_ceiling'],selected=c['selected_action'],visible_group=old['counter_free_visible_sha256']))
result=dict(complete=True,rows=rows,input_sha256=hashlib.sha256(cp.read_bytes()).hexdigest(),registry_sha256=hashlib.sha256(cards_path.read_bytes()).hexdigest(),
    limits='Public-resource inventory only; targets precede payment in the source engine. No counterfactual state executed, no label or full-turn feasibility claim. Both duplicate records retained.')
out.write_text(json.dumps(result,indent=2))
for r in rows:print(json.dumps({k:v for k,v in r.items() if k not in ['selected','visible_group']},separators=(',',':')))
