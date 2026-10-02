"""Bounded read-only search of all32 Burn-candidate g115 archive matches.

Finds visible motifs, not certified wins. No new games, training or gate.
"""
import argparse,hashlib,json,time,zipfile
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
B=Path('E:/mtg-meta-recovery-20260921')
def pin(p):
    p=Path(p);return dict(path=str(p),sha256=hashlib.sha256(p.read_bytes()).hexdigest())
def inspect(item):
    job,receipt=item;p=Path(receipt['bundle']['path'])
    assert pin(p)['sha256']==receipt['bundle']['sha256']
    with zipfile.ZipFile(p) as z:raw=z.read('result.json')
    assert hashlib.sha256(raw).hexdigest()==receipt['raw_sha256']
    result=json.loads(raw);traj=result['collected']['trajectory'];assert traj['ending']['kind']=='complete'
    actor=['p0','p1'][job['candidate_seat']];other=1-job['candidate_seat']
    counts=dict(gameplay=0,bolt_target=0,second_bolt=0,opponent_empty=0,life_4_to_6=0)
    candidates=[]
    for game_index,g in enumerate(traj['games']):
        for row in g['decisions']:
            v=row['visible']
            if row['actor']!=actor or v['kind']!='gameplay':continue
            counts['gameplay']+=1;actions=v['ordered_actions']
            if not actions or not all(a['action_kind']=='choose_target' and a['source']['card_db_id']==66 for a in actions):continue
            counts['bolt_target']+=1;obs=v['observation'];proj=obs['projection']
            hand_bolts=sum(c['stable']['card_db_id']==66 for c in obs['own_hand'])
            if not hand_bolts:continue
            counts['second_bolt']+=1
            empty=proj['hand_counts'][other]==0
            counts['opponent_empty']+=empty
            life=proj['life_totals'][other]
            counts['life_4_to_6']+=empty and 4<=life<=6
            candidates.append(dict(match=job['id'],game_index=game_index,decision_index=row['decision_index'],
                actor=actor,physical_decision_id=obs['physical_decision_id'],substep_index=obs['substep_index'],
                turn=proj['turn'],phase=proj['phase'],opponent_hand=proj['hand_counts'][other],
                opponent_life=life,hand_bolts=hand_bolts,mana_pool=proj['mana_pools'][job['candidate_seat']],
                selected_action=actions[row['behavior']['selected_index']],
                face_available=any(a['target'].get('player')==['p0','p1'][other] for a in actions),
                bundle=receipt['bundle']))
    return dict(match=job['id'],games=len(traj['games']),input_bytes=len(raw),counts=counts,candidates=candidates)
def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);a=ap.parse_args();assert not a.output.exists()
    plan_path=B/'teacher-wholematch-comparison-repaired-001/plan.json';report_path=B/'teacher-wholematch-dispatch-repaired-001/formal-result.json'
    plan=json.loads(plan_path.read_bytes());report=json.loads(report_path.read_bytes());assert report['complete']
    receipts={j['id']:j for j in report['jobs']}
    jobs=[j for j in plan['jobs'] if j['arm']=='g115' and j['candidate_deck']=='Burn'];assert len(jobs)==32
    work=[(j,receipts[j['id']]) for j in jobs]
    t=time.monotonic();serial=[inspect(j) for j in work];serial_s=time.monotonic()-t
    t=time.monotonic()
    with ThreadPoolExecutor(4) as pool:parallel=list(pool.map(inspect,work))
    parallel_s=time.monotonic()-t;assert serial==parallel
    total={k:sum(r['counts'][k] for r in serial) for k in serial[0]['counts']}
    result=dict(complete=True,plan=pin(plan_path),report=pin(report_path),script=pin(__file__),
        matches=32,games=sum(r['games'] for r in serial),input_bytes=sum(r['input_bytes'] for r in serial),
        serial_seconds=serial_s,four_reader_seconds=parallel_s,exact_reader_agreement=True,
        counts=total,candidates=[c for r in serial for c in r['candidates']],
        scope='All32 Burn-candidate g115 conditions of consumed dev panel, candidate seat only. Both seats, eight opponents, two seeds. Visible motif counts only; no terminal witness or gate. Mana/effects/response legality not certified. Repeated targets can belong to same physical decision.')
    with a.output.open('x',encoding='utf8') as f:json.dump(result,f,indent=2)
    print(json.dumps({k:result[k] for k in ['matches','games','input_bytes','serial_seconds','four_reader_seconds','counts','candidates']},indent=2))
if __name__=='__main__':main()
