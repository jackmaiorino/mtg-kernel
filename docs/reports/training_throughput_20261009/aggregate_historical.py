"""Read only explicitly listed engineering timing artifacts, never outcome fields."""
from collections import defaultdict
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import statistics

OUT = Path(__file__).resolve().parent
SCREEN = Path('D:/e-scratch/g115-line-a-screen-training-001/run-s1')
SPEEDUPS = Path('D:/e-scratch/training-speedups-20261001')
INPUTS = []

def read(path):
    raw = path.read_bytes()
    INPUTS.append({'path': path.as_posix(), 'sha256': hashlib.sha256(raw).hexdigest()})
    return json.loads(raw)

def quantile(values, q):
    a=sorted(values); x=(len(a)-1)*q; i=int(x)
    return a[i]+(a[min(i+1,len(a)-1)]-a[i])*(x-i)

def stats(values):
    return {'n':len(values), 'sum':sum(values), 'mean':statistics.mean(values),
            'median':statistics.median(values),'p90':quantile(values,.9),
            'p95':quantile(values,.95),'max':max(values)} if values else {}

def aggregate(rows):
    stages=defaultdict(float); counts=defaultdict(int)
    for r in rows:
        for k,v in r['stage_seconds'].items():stages[k]+=v;counts[k]+=1
    total=sum(r['seconds'] for r in rows)
    first=[r for r in rows if r['segment_first']]
    other=[r for r in rows if not r['segment_first']]
    ordered=sorted(rows,key=lambda x:x['seconds'],reverse=True)
    return {'updates':len(rows),'natural_games':sum(r['natural_games'] for r in rows),
            'physical_decisions':sum(r['physical_decisions'] for r in rows),
            'learner_substeps':sum(r['learner_substeps'] for r in rows),
            'collector_workers':sorted(set(r['collector_workers'] for r in rows)),
            'update_seconds':stats([r['seconds'] for r in rows]),
            'stage_seconds':dict(stages),'stage_counts':dict(counts),
            'stage_share_percent':{k:100*v/total for k,v in stages.items()},
            'unattributed_seconds':total-sum(stages.values()),
            'segment_first_update_seconds':stats([r['seconds'] for r in first]),
            'segment_first_device_seconds':stats([r['stage_seconds']['device_update_call'] for r in first]),
            'other_device_seconds':stats([r['stage_seconds']['device_update_call'] for r in other]),
            'top_10_percent_time_share':sum(r['seconds'] for r in ordered[:max(1,len(rows)//10)])/total,
            'top_updates':[{'run':r['run'],'update':r['update'],'seconds':r['seconds'],
                            'collection_seconds':r['collection_seconds']} for r in ordered[:5]]}

all_rows=[]; runs={}
for run in sorted(SCREEN.glob('la-screen-*')):
    rows=[]
    for segment in sorted(run.glob('segment-*')):
        first=int(segment.name.split('-')[-1])
        for folder in sorted((segment/'outputs').iterdir()):
            path=folder/'receipt.json'
            if not folder.name.isdigit() or not path.is_file():continue
            r=read(path)
            row={k:r[k] for k in ['update','seconds','collection_seconds','stage_seconds',
                    'natural_games','physical_decisions','learner_substeps','collector_workers',
                    'before_state_sha256','after_state_sha256']}
            row.update(run=run.name,segment_first=r['update']==first)
            rows.append(row)
    rows.sort(key=lambda r:r['update'])
    assert [r['update'] for r in rows]==list(range(200)),run
    chain=all(a['after_state_sha256']==b['before_state_sha256'] for a,b in zip(rows,rows[1:]))
    runs[run.name]=aggregate(rows)
    runs[run.name]['chain_contiguous']=chain
    runs[run.name]['per_segment']=[{'first_update':i,'seconds':sum(r['seconds'] for r in rows[i:i+40]),
                                  'device_update_seconds':sum(r['stage_seconds']['device_update_call'] for r in rows[i:i+40]),
                                  'first_device_seconds':rows[i]['stage_seconds']['device_update_call']}
                                 for i in range(0,200,40)]
    all_rows+=rows

phases=[]
for filename in ['training-completion.json','training-completion-g115-line-a-training-continuation-001.json']:
    doc=read(SCREEN/filename)
    rows=[]
    for r in doc['rows']:
        c=r.get('compression',{}); ct=c.get('timing',[])
        rows.append({'id':r['id'],'complete':r['complete'],'chain_length':len(r.get('chain',[])),
                     'wait_seconds':r.get('wait_seconds'),
                     'compression':{k:c.get(k) for k in ['folders','logical_bytes','physical_bytes','charged_bytes','seconds','max_lag','lag','failure']},
                     'compression_records':len(ct),'compression_service':stats([t['service_seconds'] for t in ct]),
                     'publication_to_compression_done':stats([t['finished_unix']-t['completed_unix'] for t in ct]),
                     'recorded_completion_unix_range':[min(t['completed_unix'] for t in ct),max(t['finished_unix'] for t in ct)] if ct else None,
                     'child_spawns':[{k:x.get(k) for k in ['first','last','pid','started_unix','finished_unix','seconds']} for x in r.get('child_spawns',[])]})
    phases.append({'source':filename,'complete':doc['complete'],'completed':doc['completed'],
                   'seconds':doc['seconds'],'rows':rows})

profile=read(SPEEDUPS/'collection-profile-haley-q001/collection-attribution-001.json')
scaling=read(Path('D:/lead-screen/scaling/scaling.json'))
policy=read(SPEEDUPS/'execution-speed-haley-q006/policy-comparison-001.json')
profile_rows=[]
for r in profile['rows']:
    stages=r['global_inclusive_stages']
    total=stages['search.total']['seconds']
    profile_rows.append({'phase':r['phase'],'run_id':r['run_id'],'profile':r['profile'],
                        'inclusive_stages':stages,'batches':r['batches'],
                        'first_device_update_seconds':r['first_device_update_seconds'],
                        'later_median_device_update_seconds':r['later_median_device_update_seconds'],
                        'global_stage_share_of_search_percent':{k:100*v['seconds']/total for k,v in stages.items()} if total else {}})

profile_summary=[]
for phase in ['serial','concurrent']:
    for arm in ['treatment','control']:
        rows=[r for r in profile_rows if r['phase']==phase and r['run_id'].endswith(arm)]
        batches=[b for r in rows for b in r['batches']]
        collection=sum(b['collection_seconds'] for b in batches)
        span=sum(b['batch_game_span_seconds'] for b in batches)
        critical=sum(b['critical_game_seconds'] for b in batches)
        work=sum(b['worker_game_seconds'] for b in batches)
        search=sum(b['critical_game_seconds']*b['critical_stage_shares_percent']['collection.search_decision']/100 for b in batches)
        profile_summary.append({'phase':phase,'arm':arm,'batches':len(batches),
                                'collection_seconds':collection,'critical_game_seconds':critical,
                                'critical_fraction_of_collection':critical/collection,
                                'worker_game_seconds':work,'worker_seconds_per_batch_span_second':work/span,
                                'critical_search_percent':100*search/critical})

reservation=read(SCREEN/'reservation.json')
recovery=read(Path('D:/e-scratch/g115-line-a-training-recovery-001/run-r30/continuation-recovery.json'))
first_acquired=datetime.fromisoformat(reservation['acquired_at']).timestamp()
last_compressed=max(r['recorded_completion_unix_range'][1] for p in phases for r in p['rows'] if r['recorded_completion_unix_range'])
elapsed_envelope={'start':'original reservation acquisition',
                  'start_utc':reservation['acquired_at'],
                  'end':'latest recorded final folder compression completion, not final launcher exit',
                  'end_utc':datetime.fromtimestamp(last_compressed,timezone.utc).isoformat(),
                  'seconds':last_compressed-first_acquired,
                  'recovery_complete':recovery['complete'],
                  'recovery_segments':len(recovery['segments']),
                  'recovery_all_hashes_equal':all(s['all_hashes_equal'] for s in recovery['segments']),
                  'recovery_remeasured_bytes':recovery['remeasured_bytes']}

result={'schema':'historical-training-timing-audit/20261009',
        'generated_utc':datetime.now(timezone.utc).isoformat(),
        'scope':'800 timing receipts in explicit completed historical public-feature training root; CPU native-expanded current recipe is separate',
        'runs':runs,'arms':{arm:aggregate([r for r in all_rows if r['run'].endswith(arm)]) for arm in ['treatment','control']},
        'all':aggregate(all_rows),'launcher_phases':phases,
        'launcher_phase_seconds_sum':sum(p['seconds'] for p in phases),
        'critical_update_seconds_sum':max(sum(r['seconds'] for r in all_rows if r['run']==name and r['update']<160) for name in runs)+max(sum(r['seconds'] for r in all_rows if r['run']==name and r['update']>=160) for name in runs),
        'elapsed_envelope':elapsed_envelope,
        'collector_scaling':scaling,
        'execution_speed_policy':{k:policy[k] for k in ['schema','factor','scope','primary_store_hashes_equal',
                                                      'behavior_hashes_equal','raw_store_hashes_equal_within_build',
                                                      'table','plan','references']},
        'collection_profile':{'semantics':profile['semantics'],'phases':profile['phases'],'rows':profile_rows,'summary':profile_summary},
        'inputs':INPUTS,
        'limitations':['Per-process update wall sums are not concurrent job wall.',
                       'Launcher phase sum excludes storage hold interval and recovery workflow.',
                       'Nested search counters overlap and must not be added as exclusive shares.',
                       'CUDA device counters measure host scopes, not actual GPU kernel service.']}
(OUT/'historical-metrics.json').write_text(json.dumps(result,indent=2)+'\n', newline="\n")
print(json.dumps({'runs':{k:{x:v[x] for x in ['updates','natural_games','update_seconds','stage_share_percent','chain_contiguous']} for k,v in runs.items()},
                  'arms':result['arms'],'launcher_phase_seconds_sum':result['launcher_phase_seconds_sum'],
                  'critical_update_seconds_sum':result['critical_update_seconds_sum']},indent=2))
