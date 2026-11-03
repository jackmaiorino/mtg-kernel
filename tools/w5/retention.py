"""W5 retention, budget and replay selection (collab LANES/lead/W5-DESIGN.md v1.3 sections 2.2 and 9; E7).

A chain is a sequence of native training segments (`native_expanded_training_run_v1` configs) listed in a chain
manifest (schema `w5-chain/v1`):

  {"schema": "w5-chain/v1", "chain": "w5/chain1", "arm": "X" | "C", "launched_unix": 1.0e9,
   "x_cap_seconds": 907200.0,  (optional; the X cap frozen at Stage 0, default 10.5 days)
   "segments": [{"config": "/abs/seg-000.json", "offset": 0}, ...]}

`offset` is the global update index of the segment's first iteration. Boundary b is the state after b updates
(b = 0 is the parent). Every segment receipt records the update's after-state hash; with `chain_ledger` it also
records the chain-only ledger (`w5-chain-ledger/v1`).

Rules (frozen in the design):
- Full states: X chains keep every boundary b with b % 64 == 0. C chains keep b % 4096 == 0, and b % 64 == 0
  within the most recent 4,096 updates. Every other checkpoint is pruned once its segment and the next one are
  complete (a live segment resumes from its own checkpoints and the next segment's source).
- Trajectories: kept for the most recent 64 complete segments (rolling).
- Projection (C chain): R-hat(t) = CPU(t) + rate(last 24 h, or since launch) x (7 days - t).
- Snapshots (X chain): from the first update whose ledger passes 0.85 R-hat, every update's checkpoint is kept
  while R_i is open, if (measured checkpoint bytes) x (projected updates between 0.85 and 1.10 R-hat) fits 30 GB.
  (A kept checkpoint is the full state; weights-only stripping is not implemented.)
- Pause at 1.10 R-hat while C runs. When C ends, R_i is its final ledger; X's endpoint is the first boundary whose
  ledger is at or past R_i. If X paused below R_i, it resumes with `stop_at_usec = R_i`.
- Selection: the endpoint's checkpoint if kept, else a replay from the nearest kept full state at or before it,
  which must reproduce every recorded hash. Then a PRUNE manifest lists every non-selected full state and snapshot.

Commands:
  retention.py index CHAIN.json                    per-update index (cached in INDEX next to the manifest)
  retention.py plan CHAIN.json [--snapshot-from B]  rolling prune plan (checkpoints, trajectories) as JSON
  retention.py apply PLAN.json [--hold DIR]         delete (or move to a same-volume hold folder) and log
  retention.py project C_CHAIN.json [--now T]      R-hat and the C chain's rate
  retention.py decide X_CHAIN.json C_CHAIN.json [--c-ended] [--x-running-seconds S] [--now T]
  retention.py select X_CHAIN.json R_USEC          endpoint, and the replay spec when its state is not kept
  retention.py replay-config X_CHAIN.json SPEC.json OUT.json OUTPUT_DIR
  retention.py verify-replay SPEC.json REPLAY_OUTPUT_DIR
  retention.py prune-manifest CHAIN.json SELECTED_BOUNDARY OUT.json
  retention.py guard --cap-bytes N [--next-bytes M] ROOT [ROOT ...]
"""
import argparse
import hashlib
import json
import os
import shutil
import statistics
import sys
import time
from pathlib import Path

FULL_STRIDE = 64
C_LONG_STRIDE = 4096
C_RECENT = 4096
TRAJECTORY_SEGMENTS = 64
DAY = 86400.0
HORIZON = 7 * DAY
SNAPSHOT_TRIGGER = 0.85
PAUSE_AT = 1.10
SNAPSHOT_CAP_BYTES = 30 * 10**9
X_CAP_SECONDS = 10.5 * DAY  # default; a chain manifest's `x_cap_seconds` (frozen at Stage 0) overrides it
HISTORY_KINDS = ('initial', 'completed_iteration')
INDEX_SCHEMA = 'w5-chain-index/v1'


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def write(path, doc):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + '.tmp')
    tmp.write_text(json.dumps(doc, indent=1) + '\n', encoding='utf-8')
    os.replace(tmp, path)


def sha(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def load_chain(path):
    chain = read(path)
    if chain.get('schema') != 'w5-chain/v1' or chain.get('arm') not in ('X', 'C'):
        raise SystemExit('stop: not a w5-chain/v1 manifest with arm X or C')
    offset = 0
    for seg in chain['segments']:
        if seg['offset'] != offset:
            raise SystemExit('stop: segment offsets are not contiguous from 0')
        cfg = read(seg['config'])
        seg['root'] = cfg['output_directory']
        seg['updates'] = len(cfg['iterations'])
        offset += seg['updates']
    return chain


def segment_complete(seg):
    return Path(seg['root'], 'completion.json').exists()


def index_path(chain_path):
    return Path(chain_path).with_suffix('.index.jsonl')


def index(chain_path):
    """One record per completed update: update, boundary, segment, after-state hash, checkpoint pin, ledger."""
    chain = load_chain(chain_path)
    cache = index_path(chain_path)
    records = []
    if cache.exists():
        records = [json.loads(line) for line in cache.read_text(encoding='utf-8').splitlines() if line]
    known = len(records)
    new = []
    for s, seg in enumerate(chain['segments']):
        for i in range(seg['updates']):
            update = seg['offset'] + i
            if update < known:
                continue
            complete = Path(seg['root'], 'iterations', f'{i:06d}', 'complete.json')
            if not complete.exists():
                break
            receipt = read(complete)
            doc = read(receipt['update']['path'])
            ledger = receipt.get('chain_ledger') or {}
            new.append({'update': update, 'boundary': update + 1, 'segment': s, 'iteration': i,
                        'after_state_sha256': doc['after_state_sha256'], 'checkpoint': doc['checkpoint'],
                        'ledger_usec': ledger.get('usec'), 'unix_seconds': ledger.get('unix_seconds'),
                        'instance': ledger.get('instance')})
        else:
            continue
        break
    if new:
        with open(cache, 'a', encoding='utf-8') as f:
            for rec in new:
                f.write(json.dumps(rec) + '\n')
    return chain, records + new


def keeps_full_state(arm, boundary, latest):
    """Whether boundary b keeps its full train state under the arm's strides."""
    if boundary % FULL_STRIDE:
        return False
    if arm == 'X':
        return True
    return boundary % C_LONG_STRIDE == 0 or boundary > latest - C_RECENT


def plan(chain_path, snapshot_from=None):
    """Rolling prune plan. A checkpoint is pruned when its boundary keeps no full state, it is not a snapshot
    (boundary at or past `snapshot_from`), its segment and the next segment are complete, and it is not the
    latest. Trajectories of complete segments older than the 64 most recent are pruned."""
    chain, records = index(chain_path)
    latest = records[-1]['boundary'] if records else 0
    segs = chain['segments']
    complete = [segment_complete(s) for s in segs]
    checkpoints, trajectories = [], []
    for rec in records:
        s = rec['segment']
        if keeps_full_state(chain['arm'], rec['boundary'], latest) or rec['boundary'] == latest:
            continue
        if snapshot_from is not None and rec['boundary'] >= snapshot_from:
            continue
        if not (complete[s] and s + 1 < len(segs) and complete[s + 1]):
            continue
        path = Path(rec['checkpoint']['path'])
        if path.exists():
            checkpoints.append({'boundary': rec['boundary'], 'path': path.as_posix(),
                                'sha256': rec['checkpoint']['sha256'], 'bytes': path.stat().st_size})
    done = [i for i, c in enumerate(complete) if c]
    for s in done[:-TRAJECTORY_SEGMENTS] if len(done) > TRAJECTORY_SEGMENTS else []:
        for path in sorted(Path(segs[s]['root'], 'iterations').glob('*/attempt-*/collect/episode-*.json')):
            trajectories.append({'segment': s, 'path': path.as_posix(), 'bytes': path.stat().st_size})
    return {'schema': 'w5-retention-plan/v1', 'chain': chain['chain'], 'arm': chain['arm'], 'latest': latest,
            'snapshot_from': snapshot_from, 'checkpoints': checkpoints, 'trajectories': trajectories,
            'bytes': sum(c['bytes'] for c in checkpoints) + sum(t['bytes'] for t in trajectories)}


def apply(plan_doc, log_path, hold=None):
    """Deletes (or moves under `hold`, same volume) every planned file, appending one log line per file.
    Checkpoints are regenerable by replay against their recorded hashes; trajectories by recollection."""
    done = 0
    with open(log_path, 'a', encoding='utf-8') as log:
        for kind in ('checkpoints', 'trajectories'):
            for item in plan_doc[kind]:
                path = Path(item['path'])
                if not path.exists():
                    continue
                if hold:
                    dst = Path(hold) / path.relative_to(path.anchor)
                    dst.parent.mkdir(parents=True, exist_ok=True)
                    os.replace(path, dst)
                    item = dict(item, moved_to=dst.as_posix())
                else:
                    path.unlink()
                log.write(json.dumps(dict(item, kind=kind, unix_seconds=time.time())) + '\n')
                done += 1
    return done


def ledger_series(records):
    return [(r['unix_seconds'], r['ledger_usec']) for r in records
            if r.get('unix_seconds') is not None and r.get('ledger_usec') is not None]


def project(c_records, launched, now=None):
    """R-hat_i(t) = CPU(t) + rate(last 24 h, or since launch if shorter) x (7 days - t), in microseconds."""
    series = ledger_series(c_records)
    if not series:
        raise SystemExit('stop: the C chain has no ledger records')
    t_now, cpu = series[-1]
    now = t_now if now is None else now
    elapsed = now - launched
    window_start = max(launched, t_now - DAY)
    base = [(t, c) for t, c in series if t <= window_start]
    t0, c0 = base[-1] if base and window_start > launched else (launched, 0)
    rate = (cpu - c0) / (t_now - t0) if t_now > t0 else 0.0
    rhat = cpu + rate * max(0.0, HORIZON - elapsed)
    return {'cpu_usec': cpu, 'rate_usec_per_s': rate, 'elapsed_s': elapsed, 'r_hat_usec': rhat,
            'window_s': t_now - t0}


def first_at_or_past(records, r_usec):
    for rec in records:
        if rec.get('ledger_usec') is not None and rec['ledger_usec'] >= r_usec:
            return rec
    return None


def decide(x_chain, x_records, c_chain, c_records, c_ended, x_running_s=None, now=None,
           checkpoint_bytes=None):
    """The pair's next action for X: run, snapshot (keep every checkpoint from now), pause, resume_to (R_i),
    select, or stop_under_budget (X cap). Ledger values only; never R-hat for recoverability."""
    x_ledger = x_records[-1]['ledger_usec'] if x_records else 0
    x_cap = float(x_chain.get('x_cap_seconds', X_CAP_SECONDS))
    out = {'x_ledger_usec': x_ledger, 'x_boundary': x_records[-1]['boundary'] if x_records else 0,
           'x_cap_seconds': x_cap}
    if c_ended:
        r = c_records[-1]['ledger_usec']
        out['r_usec'] = r
        hit = first_at_or_past(x_records, r)
        if hit:
            return dict(out, action='select', endpoint_boundary=hit['boundary'])
        if x_running_s is not None and x_running_s >= x_cap:
            return dict(out, action='stop_under_budget')
        return dict(out, action='resume_to', stop_at_usec=r)
    proj = project(c_records, c_chain['launched_unix'], now)
    out['projection'] = proj
    rhat = proj['r_hat_usec']
    if x_running_s is not None and x_running_s >= x_cap:
        return dict(out, action='stop_under_budget')
    if x_ledger >= PAUSE_AT * rhat:
        return dict(out, action='pause')
    if x_ledger >= SNAPSHOT_TRIGGER * rhat:
        recent = x_records[-65:]
        per_update = ((recent[-1]['ledger_usec'] - recent[0]['ledger_usec']) / (len(recent) - 1)
                      if len(recent) > 1 else None)
        if per_update and checkpoint_bytes:
            projected = (PAUSE_AT - SNAPSHOT_TRIGGER) * rhat / per_update
            fits = checkpoint_bytes * projected <= SNAPSHOT_CAP_BYTES
        else:
            projected, fits = None, False
        return dict(out, action='snapshot' if fits else 'run', snapshot_window_updates=projected,
                    snapshot_fits=fits)
    return dict(out, action='run')


def select(chain, records, r_usec):
    """The endpoint (first boundary at or past R) and, unless its checkpoint is still on disk, the replay spec
    from the nearest kept full state at or before it."""
    hit = first_at_or_past(records, r_usec)
    if not hit:
        raise SystemExit('stop: no boundary reaches R yet')
    endpoint = hit['boundary']
    by_boundary = {r['boundary']: r for r in records}
    if Path(hit['checkpoint']['path']).exists() and sha(hit['checkpoint']['path']) == hit['checkpoint']['sha256']:
        return {'endpoint_boundary': endpoint, 'r_usec': r_usec, 'checkpoint': hit['checkpoint'], 'replay': None}
    start = None
    for b in range(endpoint - 1, -1, -1):
        if b == 0:
            start = {'boundary': 0, 'checkpoint': None}
            break
        rec = by_boundary.get(b)
        if rec and Path(rec['checkpoint']['path']).exists():
            start = {'boundary': b, 'checkpoint': rec['checkpoint']}
            break
    expected = [{'boundary': b, 'after_state_sha256': by_boundary[b]['after_state_sha256']}
                for b in range(start['boundary'] + 1, endpoint + 1)]
    return {'endpoint_boundary': endpoint, 'r_usec': r_usec, 'checkpoint': hit['checkpoint'],
            'replay': {'chain': chain['chain'], 'from': start, 'updates': len(expected), 'expected': expected}}


def replay_config(chain, spec, out_dir):
    """A segment config that replays boundaries from+1..endpoint: the chain's own iterations, the kept state as
    its source, the global update offset of its first update, and no ledger. Opponent assignments that name the
    run's own history (`initial`, `completed_iteration`) resolve against a segment's source and iteration
    indices, so a replay that slices them is refused; every segment crossed must share one opponent roster and
    one source import."""
    replay = spec['replay']
    first = replay['from']['boundary']
    last = spec['endpoint_boundary']
    iterations, used = [], []
    for seg in chain['segments']:
        cfg = read(seg['config'])
        for i, item in enumerate(cfg['iterations']):
            if first <= seg['offset'] + i < last:
                for episode in item['episodes']:
                    if episode['opponent'].get('kind') in HISTORY_KINDS:
                        raise SystemExit('stop: the replay range uses a history-relative opponent '
                                         f"({episode['opponent']['kind']}); replay its whole segment instead")
                iterations.append(item)
                if not used or used[-1][1] is not seg:
                    used.append((cfg, seg))
    if len(iterations) != last - first or not 1 <= len(iterations) <= 4096:
        raise SystemExit('stop: the replay range is not covered by the chain segments, or exceeds 4096')
    cfg = json.loads(json.dumps(used[0][0]))
    for other, _ in used[1:]:
        if other['opponents'] != cfg['opponents'] or {k: v for k, v in other['initial_source'].items()
                                                       if k != 'checkpoint'} != {
                k: v for k, v in cfg['initial_source'].items() if k != 'checkpoint'}:
            raise SystemExit('stop: the replay crosses segments with different rosters or source imports')
    source = dict(cfg['initial_source'])
    if replay['from']['checkpoint'] is not None:
        source['checkpoint'] = replay['from']['checkpoint']
    elif first != 0:
        raise SystemExit('stop: a replay from a nonzero boundary needs its kept checkpoint')
    else:
        source = dict(read(chain['segments'][0]['config'])['initial_source'])
    cfg['initial_source'] = source
    cfg['iterations'] = iterations
    if cfg.get('ex1'):
        cfg['ex1']['update_offset'] = first
    cfg.pop('chain_ledger', None)
    cfg['output_directory'] = Path(out_dir).as_posix()
    return cfg


def verify_replay(spec, replay_root):
    """Every replayed update must reproduce its recorded after-state hash."""
    expected = spec['replay']['expected']
    got, checkpoint = [], None
    for i, item in enumerate(expected):
        complete = Path(replay_root, 'iterations', f'{i:06d}', 'complete.json')
        doc = read(read(complete)['update']['path']) if complete.exists() else {}
        got.append(doc.get('after_state_sha256'))
        checkpoint = doc.get('checkpoint')
    equal = [g == e['after_state_sha256'] for g, e in zip(got, expected)]
    ok = all(equal) and len(got) == len(expected)
    if ok and not (checkpoint and Path(checkpoint['path']).exists() and sha(checkpoint['path']) == checkpoint['sha256']):
        ok = False
    return {'schema': 'w5-replay-receipt/v1', 'endpoint_boundary': spec['endpoint_boundary'],
            'from_boundary': spec['replay']['from']['boundary'], 'updates': len(expected),
            'equal': ok, 'first_mismatch': None if all(equal) else expected[equal.index(False)]['boundary'],
            'endpoint_after_state_sha256': got[-1] if got else None,
            'endpoint_checkpoint': checkpoint if ok else None}


def prune_manifest(chain, records, selected_boundary, replay_receipt=None):
    """ARTIFACT-LAW PRUNE manifest after the pair's endpoint is fixed: every checkpoint on disk except the
    selected one; receipts (complete.json, update.json, ledgers) are kept. A selected boundary rebuilt by replay
    is kept through its verified replay receipt (the replayed checkpoint, outside the chain's trees)."""
    prune, keep = [], []
    if replay_receipt is not None:
        if not (replay_receipt.get('equal') and replay_receipt.get('endpoint_boundary') == selected_boundary
                and replay_receipt.get('endpoint_checkpoint')):
            raise SystemExit('stop: the replay receipt is not a verified replay of the selected boundary')
        pin = replay_receipt['endpoint_checkpoint']
        keep.append({'boundary': selected_boundary, 'path': Path(pin['path']).as_posix(), 'sha256': pin['sha256'],
                     'bytes': Path(pin['path']).stat().st_size, 'replayed': True})
    for rec in records:
        path = Path(rec['checkpoint']['path'])
        if not path.exists():
            continue
        item = {'boundary': rec['boundary'], 'path': path.as_posix(), 'sha256': rec['checkpoint']['sha256'],
                'bytes': path.stat().st_size}
        (keep if rec['boundary'] == selected_boundary else prune).append(item)
    if not keep and selected_boundary != 0:
        raise SystemExit('stop: the selected checkpoint is not on disk (replay it first)')
    return {'schema': 'artifact-prune-manifest/v1', 'lane': 'w5', 'chain': chain['chain'], 'arm': chain['arm'],
            'reason': 'W5 section 2.2: the pair endpoint is fixed; non-selected full states and snapshots pruned',
            'keep': {'selected_checkpoint': keep, 'receipts': 'every complete.json, update.json and ledger'},
            'prune': prune, 'prune_bytes': sum(p['bytes'] for p in prune)}


def tree_bytes(root):
    total = 0
    for dirpath, _, names in os.walk(root):
        for name in names:
            try:
                total += os.path.getsize(os.path.join(dirpath, name))
            except OSError:
                pass
    return total


def guard(roots, cap_bytes, next_bytes=0):
    """The per-node byte guard: refuses (exit 1) when the W5 trees plus the next segment would pass the cap."""
    used = sum(tree_bytes(r) for r in roots)
    return {'schema': 'w5-byte-guard/v1', 'roots': [Path(r).as_posix() for r in roots], 'used_bytes': used,
            'next_bytes': next_bytes, 'cap_bytes': cap_bytes, 'ok': used + next_bytes <= cap_bytes}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('index'); p.add_argument('chain')
    p = sub.add_parser('plan'); p.add_argument('chain'); p.add_argument('--snapshot-from', type=int)
    p = sub.add_parser('apply'); p.add_argument('plan'); p.add_argument('--log', required=True); p.add_argument('--hold')
    p = sub.add_parser('project'); p.add_argument('chain'); p.add_argument('--now', type=float)
    p = sub.add_parser('decide')
    p.add_argument('x_chain'); p.add_argument('c_chain'); p.add_argument('--c-ended', action='store_true')
    p.add_argument('--x-running-seconds', type=float); p.add_argument('--now', type=float)
    p = sub.add_parser('select'); p.add_argument('chain'); p.add_argument('r_usec', type=int)
    p = sub.add_parser('replay-config')
    p.add_argument('chain'); p.add_argument('spec'); p.add_argument('out'); p.add_argument('output_dir')
    p = sub.add_parser('verify-replay'); p.add_argument('spec'); p.add_argument('replay_root')
    p = sub.add_parser('prune-manifest'); p.add_argument('chain'); p.add_argument('boundary', type=int); p.add_argument('out')
    p.add_argument('--replay-receipt')
    p = sub.add_parser('guard'); p.add_argument('roots', nargs='+'); p.add_argument('--cap-bytes', type=int, required=True)
    p.add_argument('--next-bytes', type=int, default=0)
    a = ap.parse_args(argv)
    if a.cmd == 'index':
        _, records = index(a.chain)
        out = {'updates': len(records), 'latest_boundary': records[-1]['boundary'] if records else 0}
    elif a.cmd == 'plan':
        out = plan(a.chain, a.snapshot_from)
    elif a.cmd == 'apply':
        out = {'removed': apply(read(a.plan), a.log, a.hold)}
    elif a.cmd == 'project':
        chain, records = index(a.chain)
        out = project(records, chain['launched_unix'], a.now)
    elif a.cmd == 'decide':
        x_chain, x_records = index(a.x_chain)
        c_chain, c_records = index(a.c_chain)
        sizes = [Path(r['checkpoint']['path']).stat().st_size for r in x_records[-1:]
                 if Path(r['checkpoint']['path']).exists()]
        out = decide(x_chain, x_records, c_chain, c_records, a.c_ended, a.x_running_seconds, a.now,
                     sizes[0] if sizes else None)
    elif a.cmd == 'select':
        chain, records = index(a.chain)
        out = select(chain, records, a.r_usec)
    elif a.cmd == 'replay-config':
        chain = load_chain(a.chain)
        write(a.out, replay_config(chain, read(a.spec), a.output_dir))
        out = {'config': Path(a.out).as_posix()}
    elif a.cmd == 'verify-replay':
        out = verify_replay(read(a.spec), a.replay_root)
    elif a.cmd == 'prune-manifest':
        chain, records = index(a.chain)
        out = prune_manifest(chain, records, a.boundary, read(a.replay_receipt) if a.replay_receipt else None)
        write(a.out, out)
    else:
        out = guard(a.roots, a.cap_bytes, a.next_bytes)
    print(json.dumps(out, indent=1))
    ok = out.get('ok', True) and out.get('equal', True)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
