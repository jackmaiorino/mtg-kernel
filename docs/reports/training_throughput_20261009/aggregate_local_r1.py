"""Timing/count/hash metadata only from retained r1 native training receipts."""
from collections import Counter,defaultdict
from datetime import datetime,timezone
import hashlib
import json
from pathlib import Path
import statistics

ROOT=Path('E:/nine-deck-baseline-20261007/computehost-campaign/retained/r1')
OUT=Path(__file__).resolve().parent/'local-r1-stages.json'
FIELDS=['input_read_seconds','behavior_replay_seconds','learner_update_seconds',
        'checkpoint_io_seconds','update_elapsed_seconds','physical_decisions','policy_substeps']

def sha(raw):return hashlib.sha256(raw).hexdigest()
def quantile(v,q):
    v=sorted(v);x=(len(v)-1)*q;i=int(x)
    return v[i]+(v[min(i+1,len(v)-1)]-v[i])*(x-i)
def stats(v):return {'n':len(v),'sum':sum(v),'mean':statistics.mean(v),'median':statistics.median(v),
                     'p90':quantile(v,.9),'p95':quantile(v,.95),'p99':quantile(v,.99),'max':max(v)}
def summarize(rows):
    t={k:sum(r[k] for r in rows) for k in FIELDS}
    el=sum(r['collection_elapsed_seconds'] for r in rows)
    init=sum(r['collection_initialization_seconds'] for r in rows)
    cap=sum(r['collection_workers_started']*(r['collection_elapsed_seconds']-r['collection_initialization_seconds']) for r in rows)
    busy=sum(r['worker_busy_seconds'] for r in rows)
    counts=Counter()
    for r in rows:counts.update(r['worker_episode_count_histogram'])
    total=t['update_elapsed_seconds']+el
    return {'updates':len(rows),'episodes':sum(r['completed_episodes'] for r in rows),
            'update_stage_sums':t,'collection_elapsed_seconds':el,'collection_initialization_seconds':init,
            'collection_plus_update_seconds':total,
            'update_timer_residual_seconds':t['update_elapsed_seconds']-sum(t[k] for k in FIELDS[:4]),
            'stage_percent_of_collection_plus_update':{**{k:100*t[k]/total for k in FIELDS[:4]},
                'collection':100*el/total},
            'initialization_percent_of_collection':100*init/el,
            'sum_worker_busy_seconds':busy,'worker_capacity_seconds_excluding_initialization':cap,
            'worker_busy_capacity_proxy':busy/cap,
            'max_worker_busy_seconds':stats([r['max_worker_busy_seconds'] for r in rows]),
            'max_to_mean_worker_busy_ratio':stats([r['max_worker_busy_seconds']/r['mean_worker_busy_seconds'] for r in rows]),
            'worker_episode_count_histogram':dict(counts),
            'workers_histogram':dict(Counter(r['collection_workers_started'] for r in rows)),
            'update_elapsed_stats':stats([r['update_elapsed_seconds'] for r in rows]),
            'collection_elapsed_stats':stats([r['collection_elapsed_seconds'] for r in rows]),
            'all_receipts_complete':True}

blocks={};all_rows=[];all_sources=[]
for block in range(1,21):
    name=f'b{block:02}';base=ROOT/name/'iterations';rows=[];sources=[]
    for iteration in range(162):
        directory=base/f'{iteration:06}'
        cp=directory/'complete.json';cb=cp.read_bytes();complete=json.loads(cb)
        assert complete['iteration']==iteration,cp
        attempts=[p for p in directory.glob('attempt-*') if (p/'collect/collection.json').is_file() and (p/'update/update.json').is_file()]
        selected=[]
        for a in attempts:
            colpath=a/'collect/collection.json';uppath=a/'update/update.json'
            colraw=colpath.read_bytes();upraw=uppath.read_bytes()
            if sha(colraw)==complete['collection']['sha256'] and sha(upraw)==complete['update']['sha256']:
                selected.append((colpath,colraw,uppath,upraw))
        assert len(selected)==1,(directory,len(selected))
        colpath,colraw,uppath,upraw=selected[0]
        col=json.loads(colraw);upd=json.loads(upraw)
        assert col['complete'] and upd['complete'],directory
        timings=col['collection_worker_timings'];workers=col['collection_workers_started']
        assert len(timings)==workers,directory
        busy=[w['busy_seconds'] for w in timings]
        row={k:upd[k] for k in FIELDS}
        row.update({k:col[k] for k in ['collection_elapsed_seconds','collection_initialization_seconds','collection_workers_started']})
        row.update(worker_busy_seconds=sum(busy),max_worker_busy_seconds=max(busy),mean_worker_busy_seconds=statistics.mean(busy),
                   completed_episodes=sum(w['completed_episodes'] for w in timings),
                   worker_episode_count_histogram=dict(Counter(w['completed_episodes'] for w in timings)))
        rows.append(row)
        sources.extend([(cp.as_posix(),sha(cb)),(colpath.as_posix(),sha(colraw)),(uppath.as_posix(),sha(upraw))])
    blocks[name]=summarize(rows)
    blocks[name]['source_count']=len(sources)
    blocks[name]['source_hash_rollup_sha256']=sha(json.dumps(sources,separators=(',',':')).encode())
    blocks[name]['complete_json_hash_matches']=324
    all_rows+=rows;all_sources+=sources
    if block%5==0:print('r1 processed blocks',block,flush=True)

payload={'schema':'native-expanded-local-r1-stages/20261009','generated_utc':datetime.now(timezone.utc).isoformat(),
         'root':ROOT.as_posix(),'selection':'b01..b20;162 completed iterations each; collection/update raw SHA256 verified against complete.json',
         'all':summarize(all_rows),'blocks':blocks,'source_count':len(all_sources),
         'source_hash_rollup_sha256':sha(json.dumps(all_sources,separators=(',',':')).encode()),
         'source_hash_rollup_definition':'SHA256 UTF8 compact JSON list of [absolute POSIX path, raw SHA256]; blocks,iterations ascending; per iteration complete,collection,update.',
         'complete_json_hash_matches':6480,
         'limitations':['Summed collection/update host wall is not whole-job wall.',
                        'Worker busy is worker wall time, not CPU utilization; max worker busy is not per-game latency.',
                        'No trajectory, non-natural ledger, reward, loss, outcome or holdout fields selected.']}
OUT.write_text(json.dumps(payload,indent=2)+'\n', newline="\n")
print(json.dumps({'all':payload['all'],'source_count':payload['source_count'],'source_hash_rollup_sha256':payload['source_hash_rollup_sha256']},indent=2))
