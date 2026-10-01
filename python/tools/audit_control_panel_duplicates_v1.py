"""Exact archival game fingerprints, not an independence estimator."""
import collections,hashlib,json,time,zipfile
from pathlib import Path
import psutil
psutil.Process().nice(psutil.BELOW_NORMAL_PRIORITY_CLASS)
B=Path('E:/mtg-meta-recovery-20260921');out=B/'control-panel-duplicates-001.json'
if out.exists():raise RuntimeError('preserve existing result')
p=B/'teacher-wholematch-comparison-repaired-001/plan.json';rp=B/'teacher-wholematch-dispatch-repaired-001/formal-result.json'
plan=json.loads(p.read_bytes());report=json.loads(rp.read_bytes());receipts={r['id']:r for r in report['jobs']}
rows=[];started=time.monotonic()
def digest(x):return hashlib.sha256(json.dumps(x,sort_keys=True,separators=(',',':')).encode()).hexdigest()
for job in plan['jobs']:
 if job['arm']!='g115':continue
 ref=receipts[job['id']]['bundle'];raw=Path(ref['path']).read_bytes()
 if hashlib.sha256(raw).hexdigest()!=ref['sha256']:raise RuntimeError('archive mismatch')
 with zipfile.ZipFile(ref['path']) as z:r=json.loads(z.read('result.json'))
 games=[]
 for g in r['collected']['trajectory']['games']:
  original=digest(g)
  for d in g['decisions']:
   d.pop('decision_index',None)
   o=d.get('visible',{}).get('observation',{})
   for k in ('step_index','physical_decision_id','substep_index','substep_count'):o.pop(k,None)
  games.append(dict(game_index=g['game_index'],raw_hash=original,counter_free_hash=digest(g),terminal=g['terminal']))
 rows.append(dict(match=job['id'],games=games))
groups=collections.defaultdict(list)
for r in rows:
 for g in r['games']:groups[g['counter_free_hash']].append([r['match'],g['game_index']])
lookup={r['match']:r for r in rows};pairs=[]
for name,r in lookup.items():
 if '-s0-' not in name:continue
 other=lookup[name.replace('-s0-','-s1-')]
 a={g['counter_free_hash'] for g in r['games']};b={g['counter_free_hash'] for g in other['games']}
 pairs.append(dict(first=name,second=other['match'],games_first=len(r['games']),games_second=len(other['games']),shared_game_hashes=len(a&b),full_match_game_sequence_equal=[g['counter_free_hash'] for g in r['games']]==[g['counter_free_hash'] for g in other['games']]))
result=dict(complete=True,seconds=time.monotonic()-started,matches=len(rows),recorded_games=sum(len(r['games']) for r in rows),unique_counter_free_game_hashes=len(groups),seat_pairs=len(pairs),full_equal_pairs=sum(p['full_match_game_sequence_equal'] for p in pairs),pairs_with_shared_games=sum(p['shared_game_hashes']>0 for p in pairs),
 duplicate_groups=[v for v in groups.values() if len(v)>1],pairs=pairs,rows=rows,plan_sha256=hashlib.sha256(p.read_bytes()).hexdigest(),report_sha256=hashlib.sha256(rp.read_bytes()).hexdigest(),
 normalization='Remove only decision_index and four observation recording counters; preserve game index/start, all actor-visible content, action masses/selections, package identities and terminal.',
 non_claim='Exact game-record equality only, not independent sample size. Different hashes need not be independent. No result deletion or altered gate.')
with out.open('x') as f:json.dump(result,f,indent=2)
print(json.dumps({k:v for k,v in result.items() if k not in ['duplicate_groups','pairs','rows']},indent=2))
