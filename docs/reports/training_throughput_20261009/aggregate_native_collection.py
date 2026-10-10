"""Timing-only read of the explicitly selected native CPU retained collections."""
from collections import Counter,defaultdict
from datetime import datetime,timezone
import hashlib
import json
from pathlib import Path
import statistics

ROOT=Path('E:/nine-deck-baseline-20261007/campaign/retained')
OUT=Path(__file__).resolve().parent/'native-collection-metrics.json'

def quantile(values,q):
    a=sorted(values);x=(len(a)-1)*q;i=int(x)
    return a[i]+(a[min(i+1,len(a)-1)]-a[i])*(x-i)

def stats(values):
    return {'n':len(values),'sum':sum(values),'mean':statistics.mean(values),
            'median':statistics.median(values),'p90':quantile(values,.9),
            'p95':quantile(values,.95),'p99':quantile(values,.99),'max':max(values)}

def summarize(rows):
    elapsed=sum(r['elapsed'] for r in rows)
    initialization=sum(r['initialization'] for r in rows)
    busy=sum(r['sum_busy'] for r in rows)
    capacity=sum(r['workers']*(r['elapsed']-r['initialization']) for r in rows)
    maximum=sum(r['max_busy'] for r in rows)
    counts=Counter()
    for r in rows:counts.update(r['episode_histogram'])
    return {'updates':len(rows),'episodes':sum(r['episodes'] for r in rows),
            'elapsed_seconds':stats([r['elapsed'] for r in rows]),
            'initialization_seconds':stats([r['initialization'] for r in rows]),
            'initialization_percent_of_collection':100*initialization/elapsed,
            'workers_histogram':dict(Counter(r['workers'] for r in rows)),
            'worker_episode_count_histogram':dict(sorted(counts.items())),
            'sum_worker_busy_seconds':busy,'worker_capacity_seconds_excluding_initialization':capacity,
            'worker_busy_capacity_proxy':busy/capacity,
            'max_worker_busy_seconds':stats([r['max_busy'] for r in rows]),
            'mean_worker_busy_seconds':stats([r['mean_busy'] for r in rows]),
            'max_to_mean_worker_busy_ratio':stats([r['max_busy']/r['mean_busy'] for r in rows]),
            'max_worker_busy_fraction_of_post_initialization_collection':maximum/(elapsed-initialization),
            'mean_worker_busy_fraction_of_post_initialization_collection':sum(r['mean_busy'] for r in rows)/(elapsed-initialization),
            'post_initialization_nonmax_worker_seconds':elapsed-initialization-maximum,
            'all_complete':all(r['complete'] for r in rows)}

all_rows=[]; blocks={};input_list=[];duplicate_iterations=[];worker_totals=defaultdict(lambda:defaultdict(float))
for replica,start in [('r4',4),('r5',1),('r6',1)]:
    for block in range(start,21):
        name=f'{replica}/b{block:02}'
        rows=[];seen=Counter()
        for p in sorted((ROOT/name/'iterations').glob('*/attempt-*/collect/collection.json')):
            raw=p.read_bytes();r=json.loads(raw);input_list.append((p.as_posix(),hashlib.sha256(raw).hexdigest()))
            iteration=p.parents[2].name;seen[iteration]+=1
            workers=r['collection_worker_timings'];ws=r['collection_workers_started']
            assert len(workers)==ws,(p,len(workers),ws)
            times=[w['busy_seconds'] for w in workers]
            for w in workers:
                wt=worker_totals[str(w['worker'])]
                wt['busy_seconds']+=w['busy_seconds'];wt['completed_episodes']+=w['completed_episodes'];wt['updates']+=1
            row={'elapsed':r['collection_elapsed_seconds'],
                 'initialization':r['collection_initialization_seconds'],'workers':ws,
                 'sum_busy':sum(times),'max_busy':max(times),'mean_busy':statistics.mean(times),
                 'episodes':sum(w['completed_episodes'] for w in workers),
                 'episode_histogram':dict(Counter(w['completed_episodes'] for w in workers)),
                 'complete':r['complete']}
            assert row['elapsed']>=row['initialization']>=0,p
            rows.append(row)
        assert len(rows)==162,(name,len(rows))
        duplicates={k:v for k,v in seen.items() if v!=1}
        if duplicates:duplicate_iterations.append({name:duplicates})
        assert set(seen)=={f'{i:06}' for i in range(162)},name
        blocks[name]=summarize(rows);all_rows+=rows
    print(replica,'processed',sum(v['updates'] for k,v in blocks.items() if k.startswith(replica)),flush=True)

assert not duplicate_iterations,duplicate_iterations
assert len(all_rows)==9234,len(all_rows)
payload={'schema':'native-collection-timing-audit/20261009','generated_utc':datetime.now(timezone.utc).isoformat(),
         'root':ROOT.as_posix(),'selection':'r4 b04..b20, r5/r6 b01..b20, 162 iterations each, one complete collection per iteration',
         'all':summarize(all_rows),'blocks':blocks,'worker_totals':worker_totals,
         'source_count':len(input_list),'source_hash_rollup_sha256':hashlib.sha256(json.dumps(input_list,separators=(',',':')).encode()).hexdigest(),
         'source_hash_rollup_definition':'SHA256 of UTF-8 json.dumps(sorted traversal list of [POSIX absolute path, SHA256 raw file bytes], separators=(comma, colon)); replica r4,r5,r6; blocks ascending; file path ascending.',
         'limitations':['Busy counters are worker wall time, not CPU utilization.',
                        'Max worker busy is total work assigned to one worker within a batch, not one game latency.',
                        'Initialization is excluded from utilization proxy denominator but remains in collection wall.',
                        'No trajectory, non-natural ledger, reward, outcome, or holdout fields were selected.']}
OUT.write_text(json.dumps(payload,indent=2)+'\n', newline="\n")
print(json.dumps({'all':payload['all'],'source_count':len(input_list),'source_hash_rollup_sha256':payload['source_hash_rollup_sha256']},indent=2))
