import json
import os
import pathlib
import subprocess
BASE=pathlib.Path(__file__).parent
remote=r'''
import pathlib,json,hashlib,datetime,collections
r=pathlib.Path('C:/mtg-node/nine-deck-baseline-20261007/campaign')
rows=[]
keys=('input_read_seconds','behavior_replay_seconds','learner_update_seconds','checkpoint_io_seconds','update_elapsed_seconds','physical_decisions','policy_substeps')
for run,count in [('r1',20),('r2',20),('r3',20),('r4',3)]:
 for block in range(1,count+1):
  q=r/'retained'/run/f'b{block:02d}'
  row={'run':run,'block':f'b{block:02d}','root':str(q),'complete':False,'iterations':0}
  if not q.exists():
   row['missing']='retained root absent';rows.append(row);continue
  retained_raw=(q/'RETAINED.json').read_bytes()
  retained=json.loads(retained_raw)
  bound={pathlib.Path(x['to']).relative_to(q).as_posix():x['sha256'] for x in retained['files']}
  verified=hashlib.sha256(); sums=collections.defaultdict(float); maxima=collections.defaultdict(float)
  errors=[]
  for i in sorted((q/'iterations').iterdir()):
   try:
    complete_raw=(i/'complete.json').read_bytes()
    complete_hash=hashlib.sha256(complete_raw).hexdigest()
    assert complete_hash==bound[(i/'complete.json').relative_to(q).as_posix()], 'complete pin mismatch'
    comp=json.loads(complete_raw)
    parsed={}
    for field,sub in [('update','update/update.json'),('collection','collect/collection.json')]:
     source=pathlib.Path(comp[field]['path'])
     start=source.parts.index('iterations')
     local=q.joinpath(*source.parts[start:])
     raw=local.read_bytes();digest=hashlib.sha256(raw).hexdigest()
     assert digest==comp[field]['sha256'], field+' pin mismatch'
     assert digest==bound[local.relative_to(q).as_posix()], field+' retained mismatch'
     verified.update((local.relative_to(q).as_posix()+' '+digest+'\n').encode())
     parsed[field]=json.loads(raw)
    u,c=parsed['update'],parsed['collection']
    for k in keys:sums[k]+=u[k]
    for k in ('collection_elapsed_seconds','collection_initialization_seconds'):sums[k]+=c[k]
    busy=[w['busy_seconds'] for w in c['collection_worker_timings']]
    sums['collection_worker_busy_seconds']+=sum(busy)
    sums['collection_worker_max_busy_seconds']+=max(busy,default=0)
    sums['collection_worker_capacity_seconds']+=c['collection_workers_started']*(c['collection_elapsed_seconds']-c['collection_initialization_seconds'])
    sums['collection_completed_episodes']+=sum(w['completed_episodes'] for w in c['collection_worker_timings'])
    p=u.get('update_preparation',{})
    for k in ('preparation_elapsed_seconds','behavior_binding_seconds','opponent_loading_seconds','parallel_replay_seconds','validation_and_planning_seconds','physical_group_jobs','opponent_load_calls','opponent_cache_hits','current_model_reuses'):
     sums['preparation_'+k]+=p.get(k,0)
    sums['preparation_worker_busy_seconds']+=sum(w['busy_wall_seconds'] for w in p.get('worker_timings',[]))
    sums['preparation_worker_capacity_seconds']+=p.get('started_workers',0)*p.get('parallel_replay_seconds',0)
    maxima['decoded_tensor_payload_bytes']=max(maxima['decoded_tensor_payload_bytes'],p.get('decoded_tensor_payload_bytes',0))
    row['iterations']+=1
   except Exception as exc:errors.append({'iteration':i.name,'error':str(exc),'type':type(exc).__name__})
  row.update(complete=not errors and row['iterations']==162, sums=dict(sums),maxima=dict(maxima),errors=errors,retained_sha256=hashlib.sha256(retained_raw).hexdigest(),verified_update_collection_pairs=row['iterations'],ordered_pair_pin_digest=verified.hexdigest())
  rows.append(row)
print(json.dumps({'observed_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'host':'HALEYSPC','attempted_blocks':len(rows),'complete_blocks':sum(x['complete'] for x in rows),'blocks':rows,'note':'Read retained update/collection/complete metadata only. Verified each complete pin against RETAINED and each update+collection against both complete and RETAINED. Capacity is started workers times parallel collection window or parallel replay wall, not measured CPU utilization.'}))
'''
p=subprocess.run(['ssh','-o','BatchMode=yes','-o','ConnectTimeout=10',os.environ['MTG_AUDIT_SSH_TARGET'],os.environ['MTG_AUDIT_REMOTE_PYTHON'],'-B','-'],input=remote,capture_output=True,text=True,timeout=300)
if p.returncode:
 print(json.dumps({'code':p.returncode,'stderr':p.stderr}))
else:
 value=json.loads(p.stdout);out=BASE/'haley-update-collection-breakdown.json';out.write_text(json.dumps(value,indent=2), newline="\n")
 print(json.dumps({'output':str(out),'attempted_blocks':value['attempted_blocks'],'complete_blocks':value['complete_blocks'],'iterations':sum(b['iterations'] for b in value['blocks']),'errors':[{'run':b['run'],'block':b['block'],'errors':b['errors']} for b in value['blocks'] if b.get('errors')]}))
