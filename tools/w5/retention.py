"""W5 retention, budget and replay selection (collab LANES/lead/W5-DESIGN.md v1.3 sections 2.2 and 9; E7).

A chain is a sequence of native training segments (`native_expanded_training_run_v1` configs) listed in a chain
manifest (schema `w5-chain/v1`):

  {"schema": "w5-chain/v1", "chain": "w5/chain1", "arm": "X" | "C", "launched_unix": 1.0e9,
   "x_cap_seconds": 907200.0,  (optional; the X cap frozen at Stage 0, default 10.5 days)
   "snapshot_dir": "/abs/dir",  (optional; default <manifest stem>-snapshots beside the manifest)
   "c_wall_seconds": 604800.0,  (optional, C manifests; the projection horizon, default the 7-day C wall)
   "segments": [{"config": "/abs/seg-000.json", "offset": 0}, ...]}

A segment entry may also carry `root` and `updates` (the chain driver, tools/w5/chains.py, writes them from the
config it wrote); then the config is not read again, which keeps a long C chain's manifest cheap to load.

`offset` is the global update index of the segment's first iteration. Boundary b is the state after b updates
(b = 0 is the parent). Every segment receipt records the update's after-state hash; with `chain_ledger` it also
records the chain-only ledger (`w5-chain-ledger/v1`).

Rules (frozen in the design):
- Full states: X chains keep every boundary b with b % 64 == 0. C chains keep b % 4096 == 0, and b % 64 == 0
  within the most recent 4,096 updates. Every other checkpoint is pruned once its segment and the next one are
  complete (a live segment resumes from its own checkpoints and the next segment's source).
- Trajectories: kept for the most recent 64 complete segments (rolling).
- Projection (C chain): R-hat(t) = CPU(t) + rate(last 24 h, or since launch) x (7 days - t).
- Snapshots (X chain): from the first update whose ledger passes 0.85 R-hat, every update keeps a weights-only
  snapshot while R_i is open, if (measured snapshot bytes) x (projected updates between 0.85 and 1.10 R-hat) fits
  30 GB. A snapshot (`checkpoint_container_v1 weights-snapshot`) is the parameters alone, about a third of a full
  state; it is cut from the full checkpoint before that checkpoint is pruned, and checked against the recorded
  after-state hash. For play it is materialized (`checkpoint_container_v1 materialize`): the same parameters with
  zero moments. It never resumes training.
- Pause at 1.10 R-hat while C runs. When C ends, R_i is its final ledger; X's endpoint is the first boundary whose
  ledger is at or past R_i. If X paused below R_i, it resumes with `stop_at_usec = R_i`.
- Selection: the endpoint's checkpoint if kept, else its weights-only snapshot, else a replay from the nearest
  kept full state at or before it, which must reproduce every recorded hash. Then a PRUNE manifest lists every
  non-selected full state and snapshot, by kind.
- S learners (XS): a full state is two files, the S checkpoint record (`checkpoint.json`, schema
  `w5-s-learner-checkpoint/v1`) and the S container it pins (`s-train-state.bin`). The container's pin comes from
  the durable update receipt (`update.json` `s_learner.state`) into the index, never from the record, so a
  missing or damaged record cannot hide or redirect it. Every rule above treats the pair as one state: planned
  container first, pruned, kept, verified and counted together, never a file a surviving state uses, no path
  twice (`checkpoint_parts`).

Commands:
  retention.py index CHAIN.json                    per-update index (cached in INDEX next to the manifest; a
                                                   long-lived caller keeps it in memory with IndexCache)
  retention.py snapshot CHAIN.json BINARY --from B  weights-only snapshots of every unpruned checkpoint from B
  retention.py plan CHAIN.json [--snapshot-from B]  rolling prune plan (checkpoints, trajectories) as JSON
  retention.py apply PLAN.json [--hold DIR]         delete (or move to a same-volume hold folder) and log
  retention.py project C_CHAIN.json [--now T]      R-hat and the C chain's rate
  retention.py decide X_CHAIN.json C_CHAIN.json [--c-ended] [--x-running-seconds S] [--now T]
  retention.py select X_CHAIN.json R_USEC          endpoint, and the replay spec when its state is not kept
  retention.py replay-config X_CHAIN.json SPEC.json OUT.json OUTPUT_DIR
  retention.py verify-replay SPEC.json REPLAY_OUTPUT_DIR
  retention.py prune-manifest CHAIN.json SELECTED_BOUNDARY OUT.json
  retention.py guard --cap-bytes N [--next-bytes M] [--chain CHAIN.json ...] ROOT [ROOT ...]
"""
import argparse
import contextlib
import hashlib
import math
import json
import os
import subprocess
import sys
import tempfile
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


def state_pin(update_doc):
    """The S container an update published (`update.json`'s `s_learner.state`), or None for net-8."""
    return (update_doc.get('s_learner') or {}).get('state')


def checkpoint_parts(pin, state=None):
    """The files of one full train state, container first: an S update's container (`state`, from the durable
    update receipt, never from the record) and the checkpoint itself. Each part is {'path', 'sha256', 'part'}."""
    parts = []
    if state is not None:
        parts.append({'path': Path(state['path']).as_posix(), 'sha256': state['sha256'], 'part': 's_state'})
    parts.append({'path': Path(pin['path']).as_posix(), 'sha256': pin['sha256'], 'part': 'checkpoint'})
    return parts


def record_parts(rec):
    return checkpoint_parts(rec['checkpoint'], rec.get('state'))


def state_present(pin, state=None):
    """Every file of the state exists."""
    return all(Path(p['path']).exists() for p in checkpoint_parts(pin, state))


def state_present_any(pin, state=None):
    """Some file of the state exists."""
    return any(Path(p['path']).exists() for p in checkpoint_parts(pin, state))


def state_intact(pin, state=None):
    """Every file of the state exists and matches its pin."""
    return all(Path(p['path']).exists() and sha(p['path']) == p['sha256'] for p in checkpoint_parts(pin, state))


def load_chain(path):
    chain = read(path)
    if chain.get('schema') != 'w5-chain/v1' or chain.get('arm') not in ('X', 'C'):
        raise SystemExit('stop: not a w5-chain/v1 manifest with arm X or C')
    offset = 0
    for seg in chain['segments']:
        if seg['offset'] != offset:
            raise SystemExit('stop: segment offsets are not contiguous from 0')
        if 'root' not in seg or 'updates' not in seg:
            cfg = read(seg['config'])
            seg['root'] = cfg['output_directory']
            seg['updates'] = len(cfg['iterations'])
        offset += seg['updates']
    return chain


def segment_complete(seg):
    return Path(seg['root'], 'completion.json').exists()


def index_path(chain_path):
    return Path(chain_path).with_suffix('.index.jsonl')


def scan(chain, known):
    """Index records of the completed updates from `known` on, in order, stopping at the first update without its
    receipt (a segment's iterations complete in order)."""
    new = []
    for s, seg in enumerate(chain['segments']):
        if seg['offset'] + seg['updates'] <= known:
            continue
        for i in range(max(0, known - seg['offset']), seg['updates']):
            complete = Path(seg['root'], 'iterations', f'{i:06d}', 'complete.json')
            if not complete.exists():
                return new
            receipt = read(complete)
            doc = read(receipt['update']['path'])
            ledger = receipt.get('chain_ledger') or {}
            new.append({'update': seg['offset'] + i, 'boundary': seg['offset'] + i + 1, 'segment': s, 'iteration': i,
                        'after_state_sha256': doc['after_state_sha256'], 'checkpoint': doc['checkpoint'],
                        'state': state_pin(doc),
                        'ledger_usec': ledger.get('usec'), 'unix_seconds': ledger.get('unix_seconds'),
                        'instance': ledger.get('instance')})
    return new


@contextlib.contextmanager
def index_lock(chain_path):
    """The chain index file's writers (`index`, `IndexCache.refresh`) take turns: each reads, scans and appends under
    this lock, so no two append the same records."""
    path = Path(chain_path).with_suffix('.index.lock')
    with open(path, 'a+b') as f:
        if os.name == 'nt':
            import msvcrt
            f.seek(0)
            while True:
                try:
                    msvcrt.locking(f.fileno(), msvcrt.LK_NBLCK, 1)
                    break
                except OSError:
                    time.sleep(0.05)
            try:
                yield
            finally:
                f.seek(0)
                msvcrt.locking(f.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl
            fcntl.flock(f.fileno(), fcntl.LOCK_EX)
            try:
                yield
            finally:
                fcntl.flock(f.fileno(), fcntl.LOCK_UN)


def index(chain_path):
    """One record per completed update: update, boundary, segment, after-state hash, checkpoint pin, ledger."""
    with index_lock(chain_path):
        return index_locked(chain_path)


def index_locked(chain_path):
    chain = load_chain(chain_path)
    cache = index_path(chain_path)
    records = []
    if cache.exists():
        records = [json.loads(line) for line in cache.read_text(encoding='utf-8').splitlines() if line]
    if any('state' not in r for r in records):
        # Caches written before S containers were tracked: read each record's container pin from its durable
        # update receipt once, then rewrite the cache.
        for rec in records:
            if 'state' not in rec:
                seg = chain['segments'][rec['segment']]
                complete = Path(seg['root'], 'iterations', f"{rec['iteration']:06d}", 'complete.json')
                rec['state'] = state_pin(read(read(complete)['update']['path']))
        tmp = cache.with_suffix(cache.suffix + '.tmp')
        tmp.write_text(''.join(json.dumps(r) + '\n' for r in records), encoding='utf-8')
        os.replace(tmp, cache)
    new = scan(chain, len(records))
    if new:
        with open(cache, 'ab') as f:
            terminate(f)
            f.write(''.join(json.dumps(rec) + '\n' for rec in new).encode('utf-8'))
    return chain, records + new


def terminate(f):
    """Before an append (`f` open in 'ab'): a last line left without its newline (an interrupted append whose record
    is complete, which `index` reads as a line) gets one, so the new lines never join it."""
    size = os.fstat(f.fileno()).st_size
    if size:
        with open(f.name, 'rb') as r:
            r.seek(size - 1)
            if r.read(1) != b'\n':
                f.write(b'\n')


class IndexCache:
    """`index` with the records kept in memory across calls (`chains.py supervise --loop`): under the index lock, a
    refresh reads only the cache file's new lines and the receipts past them, and appends what it finds as `index`
    does, so it returns the same records. A cache file that was replaced or shortened, whose last line lacks its
    newline, or that does not end at the cursor, is read whole through `index` instead.

    It also keeps what `plan` and `guard` need not look at again:
    - `settled`: boundaries every file of whose full state was found absent. Nothing writes into a completed
      iteration again, so they stay absent; an absent file is never a plan item or a guard count, and it cannot
      shadow an existing one in `plan`'s set of files surviving states use. `active` is the other records, rebuilt
      whenever the records are read whole.
    - `complete`: segments found with completion.json (never removed).
    - `swept`: complete segments older than the trajectory window whose trajectories were found gone."""

    def __init__(self, chain_path):
        self.chain_path = Path(chain_path)
        self.records, self.active, self.offset, self.ident = None, [], 0, None
        self.settled, self.complete, self.swept = set(), set(), set()

    def _whole(self):
        """Every record as `index` reads them (the lock is held); the cursor is then the file's end."""
        chain, records = index_locked(self.chain_path)
        cache = index_path(self.chain_path)
        if cache.exists():
            with open(cache, 'ab') as f:
                terminate(f)
                st = os.fstat(f.fileno())
            self.offset, self.ident = st.st_size, (st.st_dev, st.st_ino)
        else:
            self.offset, self.ident = 0, None
        self.records = records
        self.active = [r for r in records if r['boundary'] not in self.settled]
        return chain, list(records)

    def refresh(self):
        """(chain, records), equal to `index(chain_path)` at the same moment."""
        with index_lock(self.chain_path):
            return self._refresh()

    def _refresh(self):
        cache = index_path(self.chain_path)
        if self.records is None:
            return self._whole()
        try:
            f = open(cache, 'rb')
        except FileNotFoundError:
            return self._whole()
        with f:  # every fallback below runs with no handle open (a migration replaces the file)
            st = os.fstat(f.fileno())
            moved = (st.st_dev, st.st_ino) != self.ident or st.st_size < self.offset
            if not moved:
                f.seek(self.offset)
                data = f.read()
        if moved:
            return self._whole()  # replaced or shortened
        if data and not data.endswith(b'\n'):
            return self._whole()  # a last line without its newline: read as index() reads it
        added = [json.loads(line) for line in data.decode('utf-8').splitlines() if line]
        if any('state' not in r for r in added):
            return self._whole()  # index() migrates the file first
        chain = load_chain(self.chain_path)
        new = scan(chain, len(self.records) + len(added))
        if new:
            text = ''.join(json.dumps(r) + '\n' for r in new).encode('utf-8')
            with open(cache, 'ab') as f:
                before = os.fstat(f.fileno())
                moved = (before.st_dev, before.st_ino) != self.ident or before.st_size != self.offset + len(data)
                if not moved:
                    f.write(text)
                    f.flush()
            if moved:
                return self._whole()  # not the file just read (a writer outside the lock): append nothing
            data += text
        self.offset += len(data)
        self.records.extend(added + new)
        self.active.extend(r for r in added + new if r['boundary'] not in self.settled)
        return chain, list(self.records)

    def settle(self, boundaries):
        """Records whose full-state files were all found absent leave `active`."""
        if boundaries:
            self.settled |= set(boundaries)
            self.active = [r for r in self.active if r['boundary'] not in self.settled]


def keeps_full_state(arm, boundary, latest):
    """Whether boundary b keeps its full train state under the arm's strides."""
    if boundary % FULL_STRIDE:
        return False
    if arm == 'X':
        return True
    return boundary % C_LONG_STRIDE == 0 or boundary > latest - C_RECENT


def snapshot_dir(chain_path, chain):
    path = Path(chain_path)
    return Path(chain.get('snapshot_dir') or path.with_name(path.stem + '-snapshots'))


def snapshot_index_path(chain_path):
    return Path(chain_path).with_suffix('.snapshots.jsonl')


def snapshots(chain_path):
    """Weights-only snapshots by boundary (the last record of a boundary wins; files must still exist)."""
    path = snapshot_index_path(chain_path)
    out = {}
    if path.exists():
        for line in path.read_text(encoding='utf-8').splitlines():
            if line:
                rec = json.loads(line)
                out[rec['boundary']] = rec
    return {b: r for b, r in out.items() if Path(r['path']).exists()}


def snapshot(chain_path, binary, snapshot_from, max_new=None):
    """Cuts a weights-only snapshot of every unpruned checkpoint at or past `snapshot_from` that keeps no full
    state and has none yet (the binary verifies the checkpoint against its pin), earliest first and at most
    `max_new` of them. Each snapshot must have been cut from the recorded after-state."""
    chain, records = index(chain_path)
    latest = records[-1]['boundary'] if records else 0
    have = snapshots(chain_path)
    directory = snapshot_dir(chain_path, chain)
    directory.mkdir(parents=True, exist_ok=True)
    made = []
    for rec in records:
        b = rec['boundary']
        if max_new is not None and len(made) >= max_new:
            break
        if b < snapshot_from or b in have or keeps_full_state(chain['arm'], b, latest):
            continue
        if not Path(rec['checkpoint']['path']).exists():
            continue
        out = directory / f'b{b:09d}.weights'
        if out.exists():
            raise SystemExit(f'stop: {out} exists without an index record')
        run = subprocess.run([binary, 'weights-snapshot', '--input', rec['checkpoint']['path'], '--sha256',
                              rec['checkpoint']['sha256'], '--output', out.resolve().as_posix()],
                             capture_output=True, text=True)
        if run.returncode != 0:
            raise SystemExit(f'stop: weights snapshot of boundary {b} failed: {run.stderr.strip()[-400:]}')
        receipt = json.loads(run.stdout)
        if receipt['cut_from_state_sha256'] != rec['after_state_sha256']:
            raise SystemExit(f'stop: the snapshot of boundary {b} was cut from another state')
        entry = {'boundary': b, 'path': Path(receipt['output']['path']).as_posix(),
                 'sha256': receipt['output']['sha256'], 'bytes': receipt['snapshot_bytes'],
                 'cut_from_state_sha256': receipt['cut_from_state_sha256'],
                 'parameters_sha256': receipt['parameters_sha256']}
        with open(snapshot_index_path(chain_path), 'a', encoding='utf-8') as f:
            f.write(json.dumps(entry) + '\n')
        made.append(entry)
    return made


def probe_snapshot_bytes(records, binary):
    """The size of a weights-only snapshot of the latest checkpoint, cut to a temporary file and deleted."""
    if binary is None:
        return None
    for rec in reversed(records):
        path = Path(rec['checkpoint']['path'])
        if not path.exists() or sha(path) != rec['checkpoint']['sha256']:
            continue
        with tempfile.TemporaryDirectory(dir=path.parent) as tmp:
            out = Path(tmp).resolve() / 'probe.weights'
            run = subprocess.run([binary, 'weights-snapshot', '--input', path.as_posix(), '--sha256',
                                  rec['checkpoint']['sha256'], '--output', out.as_posix()],
                                 capture_output=True, text=True)
            if run.returncode == 0:
                return json.loads(run.stdout)['snapshot_bytes']
    return None


def plan(chain_path, snapshot_from=None, cache=None):
    """Rolling prune plan. A checkpoint is pruned when its boundary keeps no full state, its segment and the next
    segment are complete, it is not the latest, and, inside the snapshot window (boundary at or past
    `snapshot_from`), its weights-only snapshot exists. Trajectories of complete segments older than the 64 most
    recent are pruned. With an `IndexCache` the same plan reads only its active records and unswept segments."""
    chain, records = index(chain_path) if cache is None else cache.refresh()
    candidates = records if cache is None else cache.active
    have = snapshots(chain_path)
    after = {r['boundary']: r['after_state_sha256'] for r in candidates}

    def verified(b):
        snap = have.get(b)
        return bool(snap) and snap['cut_from_state_sha256'] == after.get(b) and sha(snap['path']) == snap['sha256']
    latest = records[-1]['boundary'] if records else 0
    segs = chain['segments']
    if cache is None:
        complete = [segment_complete(s) for s in segs]
    else:
        cache.complete |= {i for i, s in enumerate(segs) if i not in cache.complete and segment_complete(s)}
        complete = [i in cache.complete for i in range(len(segs))]
    checkpoints, trajectories = [], []
    prunable = []
    for rec in candidates:
        s = rec['segment']
        if keeps_full_state(chain['arm'], rec['boundary'], latest) or rec['boundary'] == latest:
            continue
        if snapshot_from is not None and rec['boundary'] >= snapshot_from and not verified(rec['boundary']):
            continue
        if not (complete[s] and s + 1 < len(segs) and complete[s + 1]):
            continue
        prunable.append(rec)
    pruned = {r['boundary'] for r in prunable}
    surviving = {Path(p['path']).as_posix() for r in candidates if r['boundary'] not in pruned
                 for p in record_parts(r)}
    seen, gone = set(), []
    for rec in prunable:
        for part in record_parts(rec):
            path = Path(part['path'])
            if part['path'] in surviving or part['path'] in seen or not path.exists():
                continue
            seen.add(part['path'])
            item = {'boundary': rec['boundary'], 'path': path.as_posix(), 'sha256': part['sha256'],
                    'bytes': path.stat().st_size}
            if part['part'] != 'checkpoint':
                item['part'] = part['part']
            checkpoints.append(item)
        if cache is not None and not state_present_any(rec['checkpoint'], rec.get('state')):
            gone.append(rec['boundary'])
    done = [i for i, c in enumerate(complete) if c]
    for s in done[:-TRAJECTORY_SEGMENTS] if len(done) > TRAJECTORY_SEGMENTS else []:
        if cache is not None and s in cache.swept:
            continue
        found = sorted(Path(segs[s]['root'], 'iterations').glob('*/attempt-*/collect/episode-*.json'))
        for path in found:
            trajectories.append({'segment': s, 'path': path.as_posix(), 'bytes': path.stat().st_size})
        if cache is not None and not found:
            cache.swept.add(s)
    if cache is not None:
        cache.settle(gone)
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


def project(c_records, launched, now=None, horizon=HORIZON):
    """R-hat_i(t) = CPU(t) + rate(last 24 h, or since launch if shorter) x (7 days - t), in microseconds (the
    horizon is the C wall: 7 days in Stage 1, a Stage 0 qualification chain's own wall)."""
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
    rhat = cpu + rate * max(0.0, horizon - elapsed)
    return {'cpu_usec': cpu, 'rate_usec_per_s': rate, 'elapsed_s': elapsed, 'r_hat_usec': rhat,
            'window_s': t_now - t0}


def first_at_or_past(records, r_usec):
    for rec in records:
        if rec.get('ledger_usec') is not None and rec['ledger_usec'] >= r_usec:
            return rec
    return None


def decide(x_chain, x_records, c_chain, c_records, c_ended, x_running_s=None, now=None,
           snapshot_bytes=None):
    """The pair's next action for X: run, snapshot (cut a weights-only snapshot of every update from now),
    pause, resume_to (R_i), select, or stop_under_budget (X cap). `snapshot_bytes` is the measured size of one
    weights-only snapshot. Ledger values only; never R-hat for recoverability."""
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
    horizon = float(c_chain.get('c_wall_seconds') or HORIZON)
    proj = project(c_records, c_chain['launched_unix'], now, horizon)
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
        if per_update and snapshot_bytes:
            # Whole boundaries, both ends of the window included.
            projected = math.ceil((PAUSE_AT - SNAPSHOT_TRIGGER) * rhat / per_update) + 1
            fits = snapshot_bytes * projected <= SNAPSHOT_CAP_BYTES
        else:
            projected, fits = None, False
        return dict(out, action='snapshot' if fits else 'run', snapshot_window_updates=projected,
                    snapshot_fits=fits)
    return dict(out, action='run')


def select(chain, records, r_usec, weights=None):
    """The endpoint (first boundary at or past R): its checkpoint if still on disk, else its weights-only snapshot
    (`weights`: boundary to snapshot record), else the replay spec from the nearest kept full state at or before
    it."""
    hit = first_at_or_past(records, r_usec)
    if not hit:
        raise SystemExit('stop: no boundary reaches R yet')
    endpoint = hit['boundary']
    by_boundary = {r['boundary']: r for r in records}
    if state_intact(hit['checkpoint'], hit.get('state')):
        return {'endpoint_boundary': endpoint, 'r_usec': r_usec, 'checkpoint': hit['checkpoint'], 'replay': None}
    snap = (weights or {}).get(endpoint)
    if snap and sha(snap['path']) == snap['sha256'] and snap['cut_from_state_sha256'] == hit['after_state_sha256']:
        return {'endpoint_boundary': endpoint, 'r_usec': r_usec, 'checkpoint': hit['checkpoint'],
                'weights_snapshot': {k: snap[k] for k in ('path', 'sha256', 'parameters_sha256')}, 'replay': None}
    start = None
    for b in range(endpoint - 1, -1, -1):
        if b == 0:
            start = {'boundary': 0, 'checkpoint': None}
            break
        rec = by_boundary.get(b)
        if rec and state_present(rec['checkpoint'], rec.get('state')):
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
    got, checkpoint, state = [], None, None
    for i, item in enumerate(expected):
        complete = Path(replay_root, 'iterations', f'{i:06d}', 'complete.json')
        doc = read(read(complete)['update']['path']) if complete.exists() else {}
        got.append(doc.get('after_state_sha256'))
        checkpoint = doc.get('checkpoint')
        state = state_pin(doc)
    equal = [g == e['after_state_sha256'] for g, e in zip(got, expected)]
    ok = all(equal) and len(got) == len(expected)
    if ok and not (checkpoint and state_intact(checkpoint, state)):
        ok = False
    return {'schema': 'w5-replay-receipt/v1', 'endpoint_boundary': spec['endpoint_boundary'],
            'from_boundary': spec['replay']['from']['boundary'], 'updates': len(expected),
            'equal': ok, 'first_mismatch': None if all(equal) else expected[equal.index(False)]['boundary'],
            'endpoint_after_state_sha256': got[-1] if got else None,
            'endpoint_checkpoint': checkpoint if ok else None, 'endpoint_state': state if ok else None}


def prune_manifest(chain, records, selected_boundary, replay_receipt=None, weights=None):
    """ARTIFACT-LAW PRUNE manifest after the pair's endpoint is fixed: every full state and every weights-only
    snapshot on disk except the selected endpoint's; receipts (complete.json, update.json, ledgers, the snapshot
    index) are kept. The endpoint is kept as its full state when one is on disk, else its weights-only snapshot,
    else its verified replay (the replayed checkpoint, outside the chain's trees)."""
    prune, keep = [], []
    weights = weights or {}
    if replay_receipt is not None:
        if not (replay_receipt.get('equal') and replay_receipt.get('endpoint_boundary') == selected_boundary
                and replay_receipt.get('endpoint_checkpoint')):
            raise SystemExit('stop: the replay receipt is not a verified replay of the selected boundary')
        pin = replay_receipt['endpoint_checkpoint']
        record = next((r for r in records if r['boundary'] == selected_boundary), None)
        if record is None or replay_receipt.get('endpoint_after_state_sha256') != record['after_state_sha256']:
            raise SystemExit("stop: the replayed endpoint's state is not this chain's recorded state")
        replay_state = replay_receipt.get('endpoint_state')
        # An S chain's replayed endpoint is two files: a receipt without the container's pin (one written
        # before containers were tracked) must be re-verified first.
        if record.get('state') is not None and replay_state is None:
            raise SystemExit("stop: the replay receipt lacks the S endpoint's container; rerun verify-replay")
        if not state_intact(pin, replay_state):
            raise SystemExit('stop: the replayed checkpoint differs from its pin')
        for part in checkpoint_parts(pin, replay_state):
            item = {'boundary': selected_boundary, 'kind': 'full', 'path': part['path'], 'sha256': part['sha256'],
                    'bytes': Path(part['path']).stat().st_size, 'replayed': True}
            if part['part'] != 'checkpoint':
                item['part'] = part['part']
            keep.append(item)
    full_prune = []
    for rec in records:
        parts = [p for p in record_parts(rec) if Path(p['path']).exists()]
        if not parts:
            continue
        # The endpoint's full state is kept only when every file still matches its pin (as selection requires).
        intact = rec['boundary'] == selected_boundary and not keep and state_intact(rec['checkpoint'],
                                                                                    rec.get('state'))
        for part in parts:
            item = {'boundary': rec['boundary'], 'kind': 'full', 'path': part['path'], 'sha256': part['sha256'],
                    'bytes': Path(part['path']).stat().st_size}
            if part['part'] != 'checkpoint':
                item['part'] = part['part']
            (keep if intact else full_prune).append(item)
    # A file the kept endpoint uses is never pruned, and no file is listed twice.
    kept_paths, seen = {k['path'] for k in keep}, set()
    for item in full_prune:
        if item['path'] not in kept_paths and item['path'] not in seen:
            seen.add(item['path'])
            prune.append(item)
    for b, snap in sorted(weights.items()):
        item = {'boundary': b, 'kind': 'weights', 'path': snap['path'], 'sha256': snap['sha256'],
                'bytes': Path(snap['path']).stat().st_size}
        if b == selected_boundary and not keep:
            record = next((r for r in records if r['boundary'] == b), None)
            if record is None or snap['cut_from_state_sha256'] != record['after_state_sha256'] or \
                    sha(snap['path']) != snap['sha256']:
                raise SystemExit("stop: the selected snapshot is not this chain's recorded endpoint")
            keep.append(item)
        else:
            prune.append(item)
    if not keep and selected_boundary != 0:
        raise SystemExit('stop: the selected endpoint is not on disk (replay it first)')
    by_kind = {k: sum(p['bytes'] for p in prune if p['kind'] == k) for k in ('full', 'weights')}
    return {'schema': 'artifact-prune-manifest/v1', 'lane': 'w5', 'chain': chain['chain'], 'arm': chain['arm'],
            'reason': 'W5 section 2.2: the pair endpoint is fixed; non-selected full states and snapshots pruned',
            'keep': {'selected_checkpoint': keep,
                     'receipts': 'every complete.json, update.json, ledger and the snapshot index'},
            'prune': prune, 'prune_bytes': sum(p['bytes'] for p in prune), 'prune_bytes_by_kind': by_kind}


def tree_bytes(root):
    total = 0
    for dirpath, _, names in os.walk(root):
        for name in names:
            try:
                total += os.path.getsize(os.path.join(dirpath, name))
            except OSError:
                pass
    return total


def under(path, roots):
    path = Path(path).resolve()
    return any(path == Path(r).resolve() or Path(r).resolve() in path.parents for r in roots)


def guard(roots, cap_bytes, next_bytes=0, chains=(), caches=None):
    """The per-node byte guard: refuses (exit 1) when the W5 trees plus the next segment would pass the cap. For
    each chain manifest given, its full states and weights-only snapshots on disk are reported by kind, and every
    one of them must lie under a guarded root (a snapshot directory outside the roots would escape the cap).
    `caches` (manifest path to `IndexCache`) gives the same report from the chains' active records."""
    used = sum(tree_bytes(r) for r in roots)
    kinds, escaped = [], []
    for chain_path in chains:
        cache = (caches or {}).get(Path(chain_path).as_posix())
        if cache is None:
            chain, records = index(chain_path)
        else:
            chain, _ = cache.refresh()
            records = cache.active
            cache.settle([r['boundary'] for r in records if not state_present_any(r['checkpoint'], r.get('state'))])
            records = cache.active
        full = sorted({Path(p['path']) for r in records for p in record_parts(r) if Path(p['path']).exists()})
        weights = [Path(s['path']) for s in snapshots(chain_path).values()]
        # Where the chain will write (segment roots and its snapshot directory), checked before anything exists.
        destinations = [Path(seg['root']) for seg in chain['segments']] + [snapshot_dir(chain_path, chain)]
        escaped += [p.as_posix() for p in destinations + full + weights if not under(p, roots)]
        kinds.append({'chain': chain['chain'], 'full_state_bytes': sum(p.stat().st_size for p in full),
                      'full_states': len(full), 'weights_bytes': sum(p.stat().st_size for p in weights),
                      'weights_snapshots': len(weights)})
    return {'schema': 'w5-byte-guard/v1', 'roots': [Path(r).as_posix() for r in roots], 'used_bytes': used,
            'next_bytes': next_bytes, 'cap_bytes': cap_bytes, 'by_chain': kinds, 'outside_roots': escaped[:20],
            'ok': used + next_bytes <= cap_bytes and not escaped}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('index'); p.add_argument('chain')
    p = sub.add_parser('snapshot'); p.add_argument('chain'); p.add_argument('binary')
    p.add_argument('--from', dest='snapshot_from', type=int, required=True)
    p = sub.add_parser('plan'); p.add_argument('chain'); p.add_argument('--snapshot-from', type=int)
    p = sub.add_parser('apply'); p.add_argument('plan'); p.add_argument('--log', required=True); p.add_argument('--hold')
    p = sub.add_parser('project'); p.add_argument('chain'); p.add_argument('--now', type=float)
    p = sub.add_parser('decide')
    p.add_argument('x_chain'); p.add_argument('c_chain'); p.add_argument('--c-ended', action='store_true')
    p.add_argument('--x-running-seconds', type=float); p.add_argument('--now', type=float)
    p.add_argument('--binary')
    p = sub.add_parser('select'); p.add_argument('chain'); p.add_argument('r_usec', type=int)
    p = sub.add_parser('replay-config')
    p.add_argument('chain'); p.add_argument('spec'); p.add_argument('out'); p.add_argument('output_dir')
    p = sub.add_parser('verify-replay'); p.add_argument('spec'); p.add_argument('replay_root')
    p = sub.add_parser('prune-manifest'); p.add_argument('chain'); p.add_argument('boundary', type=int); p.add_argument('out')
    p.add_argument('--replay-receipt')
    p = sub.add_parser('guard'); p.add_argument('roots', nargs='+'); p.add_argument('--cap-bytes', type=int, required=True)
    p.add_argument('--next-bytes', type=int, default=0)
    p.add_argument('--chain', dest='chains', action='append', default=[])
    a = ap.parse_args(argv)
    if a.cmd == 'index':
        _, records = index(a.chain)
        out = {'updates': len(records), 'latest_boundary': records[-1]['boundary'] if records else 0}
    elif a.cmd == 'snapshot':
        made = snapshot(a.chain, a.binary, a.snapshot_from)
        out = {'snapshots': len(made), 'bytes': sum(m['bytes'] for m in made)}
    elif a.cmd == 'plan':
        out = plan(a.chain, a.snapshot_from)
    elif a.cmd == 'apply':
        out = {'removed': apply(read(a.plan), a.log, a.hold)}
    elif a.cmd == 'project':
        chain, records = index(a.chain)
        out = project(records, chain['launched_unix'], a.now, float(chain.get('c_wall_seconds') or HORIZON))
    elif a.cmd == 'decide':
        x_chain, x_records = index(a.x_chain)
        c_chain, c_records = index(a.c_chain)
        # A measured snapshot: one already cut, or a probe cut from the latest checkpoint and deleted. Without
        # either the window is not admitted (the endpoint is rebuilt by replay).
        made = snapshots(a.x_chain)
        size, measured = (max(s['bytes'] for s in made.values()), True) if made else (None, False)
        out = decide(x_chain, x_records, c_chain, c_records, a.c_ended, a.x_running_seconds, a.now, size)
        if size is None and out['action'] == 'run' and 'snapshot_fits' in out and a.binary:
            # Only the snapshot admission needs a size: probe now, then decide again.
            size = probe_snapshot_bytes(x_records, a.binary)
            measured = size is not None
            out = decide(x_chain, x_records, c_chain, c_records, a.c_ended, a.x_running_seconds, a.now, size)
        out['snapshot_bytes'] = size
        out['snapshot_bytes_measured'] = measured
    elif a.cmd == 'select':
        chain, records = index(a.chain)
        out = select(chain, records, a.r_usec, snapshots(a.chain))
    elif a.cmd == 'replay-config':
        chain = load_chain(a.chain)
        write(a.out, replay_config(chain, read(a.spec), a.output_dir))
        out = {'config': Path(a.out).as_posix()}
    elif a.cmd == 'verify-replay':
        out = verify_replay(read(a.spec), a.replay_root)
    elif a.cmd == 'prune-manifest':
        chain, records = index(a.chain)
        out = prune_manifest(chain, records, a.boundary, read(a.replay_receipt) if a.replay_receipt else None,
                             snapshots(a.chain))
        write(a.out, out)
    else:
        out = guard(a.roots, a.cap_bytes, a.next_bytes, a.chains)
    print(json.dumps(out, indent=1))
    ok = out.get('ok', True) and out.get('equal', True)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
