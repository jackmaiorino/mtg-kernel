"""Bounded telemetry engineering: two fixed archived episodes per control run.

No engine, training or outcome gate. Reads 12 pinned members, not whole 24GB
archives. Reports logit-softmax entropy, not quantized sampler entropy.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import math
from pathlib import Path
import struct
import time
import zipfile

BASE = Path('E:/mtg-postboard-campaign-20260921/control-variance-001')


def pin(path):
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


def entropy(bits):
    z = [struct.unpack('<f', struct.pack('<I', b))[0] for b in bits]
    assert z and all(math.isfinite(x) for x in z)
    m = max(z)
    weights = [math.exp(x-m) for x in z]
    total = math.fsum(weights)
    probs = [w/total for w in weights]
    return -math.fsum(p*math.log(p) for p in probs if p), max(probs)


def read_job(job):
    with zipfile.ZipFile(job['archive']) as z:
        raw = z.read(job['member'])
    assert hashlib.sha256(raw).hexdigest() == job['sha256']
    obj = json.loads(raw)
    actor = obj['episode']['learner_seat']
    rows = [r for r in obj['decisions'] if r['actor'] == actor]
    choices = [r for r in rows if len(r['logits']) > 1]
    assert choices
    values = [entropy(r['logits']) for r in choices]
    return dict(run=job['run'], update=job['update'], member=job['member'],
        member_sha256=job['sha256'], input_bytes=len(raw), learner_seat=actor,
        own_deck=obj['episode']['selected'][actor]['label'],
        opponent_deck=obj['episode']['selected'][1-actor]['label'],
        learner_rows=len(rows), choices=len(choices),
        mean_choice_entropy_nats=math.fsum(h for h,_ in values)/len(values),
        mean_normalized_choice_entropy=math.fsum(h/math.log(len(r['logits']))
            for r,(h,_) in zip(choices,values))/len(values),
        fraction_top_at_least_099=sum(p>=.99 for _,p in values)/len(values))


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--output',type=Path,required=True)
    args=ap.parse_args()
    assert not args.output.exists()
    jobs=[]
    manifests=[]
    for replica in (1,2,3):
        folder=BASE/f'replica-{replica}'/f'control-variance-replica-{replica}-training'
        path=folder/'archive.json'
        manifests.append(pin(path))
        archive=json.loads(path.read_bytes())
        assert archive['mismatches']==0 or archive['mismatches']==[]
        for arm in ('a','b'):
            for update in (0,199):
                suffix=f'jobs/{arm}/outputs/{update:04d}/episode-000.json'
                candidates=[(s,n,h) for s in archive['shards'] for n,h in s['files'].items()
                            if n.endswith(suffix)]
                assert len(candidates)==1
                shard,name,digest=candidates[0]
                jobs.append(dict(run=f'r{replica}-{arm}',update=update,
                    archive=shard['archive']['path'],member=name,sha256=digest))
    start=time.monotonic()
    serial=[read_job(j) for j in jobs]
    serial_seconds=time.monotonic()-start
    start=time.monotonic()
    with ThreadPoolExecutor(4) as pool:
        parallel=list(pool.map(read_job,jobs))
    parallel_seconds=time.monotonic()-start
    assert serial==parallel
    result=dict(scope='12 fixed-episode telemetry check, not full-run entropy or collapse diagnosis',
        metric='float64 softmax of archived f32 logits, natural-log entropy, forced menus excluded',
        independent_runs=6, episodes_per_run=2, episode_slot=0, updates=[0,199],
        serial_seconds=serial_seconds, four_reader_seconds=parallel_seconds,
        exact_reader_agreement=True, manifests=manifests, script=pin(Path(__file__)),
        member_integrity='SHA256 checked against archive manifest for every consumed member; whole zip not rehashed',
        limits=['different visited states and opponents confound time comparisons',
                'one episode per endpoint is not a per-run distribution estimate',
                'not execution Q64/clamped entropy or full autoregressive entropy',
                'no validated collapse alarm and no causal win-rate inference'],rows=serial)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    with args.output.open('x',encoding='utf8') as f:
        json.dump(result,f,indent=2)
    print(json.dumps({k:result[k] for k in ('serial_seconds','four_reader_seconds','exact_reader_agreement')}))
    for r in serial:
        print(r['run'],r['update'],r['choices'],round(r['mean_choice_entropy_nats'],4),
              round(r['mean_normalized_choice_entropy'],4),round(r['fraction_top_at_least_099'],4))


if __name__=='__main__':
    main()
