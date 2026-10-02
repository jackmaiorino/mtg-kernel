"""Descriptive nonempty-hand coverage in the consumed development archive. No games."""
import hashlib,json,time,zipfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
B=Path('E:/mtg-meta-recovery-20260921')
FAMILY={37:('Fireblast',4),41:('Galvanic Blast',4),63:('Lava Dart',1),66:('Lightning Bolt',3)}
def require(ok,why):
    if not ok: raise ValueError(why)
def pin(p):
    p=Path(p); return {'path':str(p),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
def inspect(work):
    job,rec=work
    raw=Path(rec['bundle']['path']).read_bytes()
    require(hashlib.sha256(raw).hexdigest()==rec['bundle']['sha256'],'bundle hash')
    import io
    with zipfile.ZipFile(io.BytesIO(raw)) as z: raw=z.read('result.json')
    require(hashlib.sha256(raw).hexdigest()==rec['raw_sha256'],'raw hash')
    data=json.loads(raw);trajectory=data['collected']['trajectory']
    require(trajectory['ending']['kind']=='complete','incomplete archive')
    require(all(p['gameplay']['source']['checkpoint']['sha256']=='88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1' for p in data['packages']),'model differs')
    out=[];total=empty=0
    for gi,g in enumerate(trajectory['games']):
        for row in g['decisions']:
            v=row['visible']
            if v['kind']!='gameplay':continue
            actions=v['ordered_actions'];o=v['observation'];p=o['projection']
            if not actions or not all(a['action_kind']=='choose_target' and a['source']['card_db_id'] in FAMILY and a['remaining']==1 for a in actions):continue
            hand=[c['stable']['card_db_id'] for c in o['own_hand'] if c['stable']['card_db_id'] in FAMILY]
            if not hand:continue
            total+=1;actor=['p0','p1'].index(row['actor']);other=1-actor
            if p['hand_counts'][other]==0:empty+=1;continue
            source=actions[0]['source']['card_db_id'];require(all(a['source']['card_db_id']==source for a in actions),'mixed sources')
            damage=FAMILY[source][1];ceiling=damage+sum(FAMILY[c][1] for c in hand);life=p['life_totals'][other]
            out.append(dict(match=job['id'],seed=job['seed'],cell=job['cell'],game_index=gi,decision_index=row['decision_index'],actor=row['actor'],source=FAMILY[source][0],hand=[FAMILY[c][0] for c in hand],opponent_hand_count=p['hand_counts'][other],opponent_life=life,nominal_hand_dependent=damage<life<=ceiling,nominal_immediate=0<life<=damage,face_available=any(a['target'].get('player')==['p0','p1'][other] for a in actions)))
    return dict(total=total,empty=empty,rows=out)
def summary(rows):
    return dict(records=len(rows),distinct_seeds=len({r['seed'] for r in rows}),matches=len({r['match'] for r in rows}),games=len({(r['match'],r['game_index']) for r in rows}),matchup_clusters=len({r['cell'] for r in rows}))
def main():
    output=B/'nonempty-hand-coverage-001.json';require(not output.exists(),'output exists')
    pp=B/'teacher-wholematch-comparison-repaired-001/plan.json';rp=B/'teacher-wholematch-dispatch-repaired-001/formal-result.json'
    plan=json.loads(pp.read_bytes());report=json.loads(rp.read_bytes());require(report['complete'],'report incomplete')
    jobs=[j for j in plan['jobs'] if j['arm']=='g115'];receipts={r['id']:r for r in report['jobs']}
    require(len(jobs)==256 and len({j['seed'] for j in jobs})==128,'archive scope differs')
    registry=Path('E:/mtg-kernel-public-stack-features-codex/data/cards_v1.json');cards=json.loads(registry.read_bytes())['cards']
    require(all(cards[k]['name']==v[0] for k,v in FAMILY.items()),'card mapping')
    work=[(j,receipts[j['id']]) for j in jobs];start=time.monotonic();serial=list(map(inspect,work));serial_s=time.monotonic()-start
    start=time.monotonic()
    with ThreadPoolExecutor(4) as pool:parallel=list(pool.map(inspect,work))
    parallel_s=time.monotonic()-start;require(serial==parallel,'parallel output differs')
    require(sum(r['total'] for r in serial)==288 and sum(r['empty'] for r in serial)==15,'prior count differs')
    rows=[r for batch in serial for r in batch['rows']]
    groups={'all_nonempty':summary(rows),'nominal_hand_dependent':summary([r for r in rows if r['nominal_hand_dependent']]),'nominal_immediate':summary([r for r in rows if r['nominal_immediate']])}
    out=dict(complete=True,plan=pin(pp),report=pin(rp),registry=pin(registry),script=pin(__file__),serial_seconds=serial_s,four_reader_seconds=parallel_s,exact_reader_agreement=True,groups=groups,rows=rows,limits='128 consumed environment seeds; 256 seat-swapped matches; both actors counted. Record and game counts are not independent samples. Optimistic burn sum ignores payment, metalcraft, prevention, responses and stack. No win certification, regret, outcome read, population prevalence, or strength claim. Training n=0.')
    with output.open('x',encoding='utf8') as f:json.dump(out,f,indent=2)
    print(json.dumps({k:out[k] for k in ['groups','serial_seconds','four_reader_seconds']},indent=2))
if __name__=='__main__':main()
