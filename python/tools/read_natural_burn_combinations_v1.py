"""Read all256 archived g115 matches for four-family burn motifs, no games."""
import argparse,hashlib,json,time,zipfile
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
B=Path('E:/mtg-meta-recovery-20260921')
FAMILY={37:('Fireblast',4),41:('Galvanic Blast',4),63:('Lava Dart',1),66:('Lightning Bolt',3)}
def pin(p):
    p=Path(p);return dict(path=str(p),sha256=hashlib.sha256(p.read_bytes()).hexdigest())
def inspect(item):
    job,rec=item;path=Path(rec['bundle']['path']);assert pin(path)['sha256']==rec['bundle']['sha256']
    with zipfile.ZipFile(path) as z:raw=z.read('result.json')
    assert hashlib.sha256(raw).hexdigest()==rec['raw_sha256'];r=json.loads(raw)
    assert all(p['gameplay']['source']['checkpoint']['sha256']=='88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1' for p in r['packages'])
    t=r['collected']['trajectory'];assert t['ending']['kind']=='complete'
    counts=dict(gameplay=0,burn_target=0,with_hand_burn=0,opponent_empty=0,empty_nominal_hand_dependent=0,empty_nominal_immediate=0)
    candidates=[]
    for gi,g in enumerate(t['games']):
        for row in g['decisions']:
            v=row['visible']
            if v['kind']!='gameplay':continue
            counts['gameplay']+=1;actions=v['ordered_actions']
            if not actions or not all(a['action_kind']=='choose_target' and a['source']['card_db_id'] in FAMILY and a['remaining']==1 for a in actions):continue
            counts['burn_target']+=1;o=v['observation'];p=o['projection'];actor=['p0','p1'].index(row['actor']);other=1-actor
            source=actions[0]['source']['card_db_id'];assert all(a['source']['card_db_id']==source for a in actions)
            hand=[c['stable']['card_db_id'] for c in o['own_hand'] if c['stable']['card_db_id'] in FAMILY]
            if not hand:continue
            counts['with_hand_burn']+=1;empty=p['hand_counts'][other]==0;counts['opponent_empty']+=empty
            current=FAMILY[source][1];total=current+sum(FAMILY[c][1] for c in hand);life=p['life_totals'][other]
            dependent=current<life<=total;immediate=0<life<=current
            counts['empty_nominal_hand_dependent']+=empty and dependent
            counts['empty_nominal_immediate']+=empty and immediate
            if empty:
                candidates.append(dict(match=job['id'],game_index=gi,decision_index=row['decision_index'],actor=row['actor'],
                    physical_decision_id=o['physical_decision_id'],substep_index=o['substep_index'],turn=p['turn'],phase=p['phase'],
                    source=FAMILY[source][0],hand=[FAMILY[c][0] for c in hand],opponent_life=life,nominal_ceiling=total,
                    nominal_hand_dependent=dependent,nominal_immediate=immediate,mana_pool=p['mana_pools'][actor],
                    face_available=any(a['target'].get('player')==['p0','p1'][other] for a in actions),
                    selected_action=actions[row['behavior']['selected_index']],bundle=rec['bundle']))
    return dict(match=job['id'],games=len(t['games']),bytes=len(raw),counts=counts,candidates=candidates)
def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);a=ap.parse_args();assert not a.output.exists()
    pp=B/'teacher-wholematch-comparison-repaired-001/plan.json';rp=B/'teacher-wholematch-dispatch-repaired-001/formal-result.json'
    plan=json.loads(pp.read_bytes());report=json.loads(rp.read_bytes());assert report['complete']
    cards=Path('E:/mtg-kernel-public-stack-features-codex/data/cards_v1.json');registry=json.loads(cards.read_bytes())['cards']
    assert all(registry[k]['name']==v[0] for k,v in FAMILY.items())
    jobs=[j for j in plan['jobs'] if j['arm']=='g115'];assert len(jobs)==256
    receipts={r['id']:r for r in report['jobs']};work=[(j,receipts[j['id']]) for j in jobs]
    started=time.monotonic();serial=[inspect(x) for x in work];serial_s=time.monotonic()-started
    started=time.monotonic()
    with ThreadPoolExecutor(4) as pool:parallel=list(pool.map(inspect,work))
    parallel_s=time.monotonic()-started;assert serial==parallel
    counts={k:sum(x['counts'][k] for x in serial) for k in serial[0]['counts']}
    out=dict(complete=True,plan=pin(pp),report=pin(rp),registry=pin(cards),script=pin(__file__),
        matches=256,games=sum(x['games'] for x in serial),input_bytes=sum(x['bytes'] for x in serial),
        serial_seconds=serial_s,four_reader_seconds=parallel_s,exact_reader_agreement=True,counts=counts,
        candidates=[r for x in serial for r in x['candidates']],
        limits='Both actors of all256 consumed control matches. Four instant burn families only. Galvanic Blast uses optimistic4 damage. Nominal sum ignores mana, alternate costs, prevention, lifegain, responses and existing stack. Not a legal-win certificate or independent training evidence; candidates never counted as errors.')
    with a.output.open('x',encoding='utf8') as f:json.dump(out,f,indent=2)
    print(json.dumps({k:out[k] for k in ['matches','games','input_bytes','serial_seconds','four_reader_seconds','counts']},indent=2))
if __name__=='__main__':main()
