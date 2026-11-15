"""Shared helpers of the London mulligan measurement tools (mull_calibrate.py, mull_start_replication.py,
mull_fixedhand.py, mull_paired.py). Measurement only: nothing here trains a lineage or touches a running chain.

**Collect-only runs.** The trainer has no collect-only mode, so a run is a one-iteration config: its episodes are
collected by the unchanged source state, the single update that follows is thrown away with the run directory. `collect`
builds one such config per chunk of episodes (the same episode entries a chain's segment config holds, stamped by
`tools/ex1/qual.py build_config`, so a probe sees the same games, seeds aside), runs the trainer on each, checks every
trajectory against its pin, yields `(entry, trajectory, path)` and deletes the chunk's run directory afterwards unless
told to keep it.

**Rows.** A trajectory's decision rows carry `tensor.state` and `tensor.action_features` as float32 bit patterns,
`logits` (bits) and `value`. A pregame row has state column 89 equal to 1.0. An announcement (keep or mulligan?) has
two action rows: the mulligan row has action columns 15 and 69 set, the keep row column 15 only, and both carry the
acting seat's OWN mulligans taken over 16 in column 63 (column 66 is the other seat's count and is never read). Any
other pregame row with more than one action row is a put-back (bottom) decision.
"""
import gzip
import hashlib
import json
import math
import shutil
import struct
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
LN9 = math.log(9)
COUNTS = 7  # own mulligans taken at an announcement: 0 to 6
PREGAME_STATE_COLUMN = 89
OWN_COUNT_COLUMN = 63
KIND_COLUMNS = (15, 69)
DEFAULT_DECKS = ['Wildfire', 'Rally', 'Affinity', 'Elves', 'Spy', 'Burn', 'Terror', 'CawGates', 'Faeries']


def qual():
    ex1 = str(HERE.parent / 'ex1')
    if ex1 not in sys.path:
        sys.path.insert(0, ex1)
    import qual as q
    return q


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def read_json(path):
    """A JSON file: plain, gzip, or a sealed record (one zstd frame)."""
    data = Path(path).read_bytes()
    if data[:2] == b'\x1f\x8b':
        data = gzip.decompress(data)
    else:
        data = qual().open_record(data)
    return json.loads(data.decode('utf-8'))


def f32(bits):
    """A float32 given as its bit pattern (an int) or already a number."""
    if isinstance(bits, bool) or not isinstance(bits, (int, float)):
        raise ValueError(f'not a tensor number: {bits!r}')
    if isinstance(bits, int):
        if not 0 <= bits < 1 << 32:
            raise ValueError(f'not a float32 bit pattern: {bits!r}')
        return struct.unpack('<f', struct.pack('<I', bits))[0]
    return float(bits)


def to_bits(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def sigmoid(x):
    if x >= 0:
        return 1.0 / (1.0 + math.exp(-x))
    e = math.exp(x)
    return e / (1.0 + e)


# ---------------------------------------------------------------- statistics (no numpy)

def quantile(values, q):
    """The q-quantile of `values` by linear interpolation between order statistics (numpy's default)."""
    xs = sorted(values)
    if not xs:
        raise ValueError('quantile of nothing')
    pos = q * (len(xs) - 1)
    lo = int(math.floor(pos))
    hi = min(lo + 1, len(xs) - 1)
    return xs[lo] + (xs[hi] - xs[lo]) * (pos - lo)


def mean(values):
    values = list(values)
    return sum(values) / len(values) if values else None


def sd(values):
    values = list(values)
    if len(values) < 2:
        return None
    m = sum(values) / len(values)
    return math.sqrt(sum((v - m) ** 2 for v in values) / (len(values) - 1))


def weighted_median(values, weights):
    """The weighted median: sort by value; the first value whose cumulative weight reaches half the total. If the
    cumulative weight equals half exactly there, the median is the midpoint of that value and the next one (so equal
    weights give the ordinary median)."""
    pairs = sorted(zip(values, weights))
    total = sum(w for _, w in pairs)
    if not pairs or not total > 0:
        raise ValueError('weighted median of nothing')
    half, run = total / 2.0, 0.0
    for i, (v, w) in enumerate(pairs):
        run += w
        if run > half + 1e-12 * total:
            return v
        if abs(run - half) <= 1e-12 * total:
            return (v + pairs[i + 1][0]) / 2.0 if i + 1 < len(pairs) else v
    return pairs[-1][0]


def entropy_nats(logits):
    m = max(logits)
    z = [math.exp(x - m) for x in logits]
    s = sum(z)
    return -sum((v / s) * math.log(v / s) for v in z if v > 0)


# ---------------------------------------------------------------- decision rows

def is_pregame(row):
    state = row['tensor']['state']
    return len(state) > PREGAME_STATE_COLUMN and f32(state[PREGAME_STATE_COLUMN]) == 1.0


def announcement(row, with_tensor=False):
    """The announcement a pregame row holds, or None for any other row (a put-back, an in-game row). A dict:
    `actor`, `k` (the acting seat's own mulligans taken, from action column 63 only: exactly k/16 in float32, one k in
    0 to 6, both action rows agreeing), `mull_row`, `keep_row`, `logits`, `gap` (mulligan logit minus keep logit),
    `value` (the head's estimate on the row, or None) and, with `with_tensor`, the row's `tensor`. A row that looks
    like an announcement but whose own-count column is not k/16 raises ValueError."""
    if not is_pregame(row):
        return None
    logits = row.get('logits')
    if not (isinstance(logits, list) and len(logits) == 2):
        return None
    flat = row['tensor']['action_features']
    if len(flat) % 2:
        return None
    width = len(flat) // 2
    if width <= max(OWN_COUNT_COLUMN, *KIND_COLUMNS):
        raise ValueError('an action row too narrow for the pregame columns')
    kinds = [(f32(flat[r * width + KIND_COLUMNS[0]]), f32(flat[r * width + KIND_COLUMNS[1]])) for r in (0, 1)]
    mull = [r for r in (0, 1) if kinds[r] == (1.0, 1.0)]
    keep = [r for r in (0, 1) if kinds[r] == (1.0, 0.0)]
    if len(mull) != 1 or len(keep) != 1:
        return None
    own = [f32(flat[r * width + OWN_COUNT_COLUMN]) for r in (0, 1)]
    k = round(own[0] * 16)
    if own[0] != own[1] or not 0 <= k < COUNTS or own[0] != struct.unpack('<f', struct.pack('<f', k / 16))[0]:
        raise ValueError(f'the own-count column of an announcement is {own}, not k/16 for one k in 0..{COUNTS - 1}')
    lg = [f32(b) for b in logits]
    value = row.get('value', row.get('value_bits'))
    out = {'actor': row['actor'], 'k': k, 'mull_row': mull[0], 'keep_row': keep[0], 'logits': list(logits),
           'gap': lg[mull[0]] - lg[keep[0]], 'value': None if value is None else f32(value)}
    if with_tensor:
        out['tensor'] = row['tensor']
    return out


def pregame_decisions(traj, with_tensor=False):
    """`(announcements, bottoms)` of a trajectory, each in row order. A bottom is `{'actor', 'n', 'logits'}` for a
    pregame row with more than one action row that is not an announcement."""
    anns, bottoms = [], []
    for row in traj['decisions']:
        if not is_pregame(row):
            continue
        a = announcement(row, with_tensor)
        if a is not None:
            anns.append(a)
        elif isinstance(row.get('logits'), list) and len(row['logits']) > 1:
            bottoms.append({'actor': row['actor'], 'n': len(row['logits']), 'logits': row['logits']})
    return anns, bottoms


def seat_group(modes, actor):
    """'keep_seven' if the other seat is a fixed keep-seven seat, else 'self_play' (the other seat is learned too)."""
    return 'keep_seven' if modes[1 - actor] == 'keep_seven' else 'self_play'


def deck_of(traj, seat):
    return str(traj['episode']['registered'][seat].get('label', '')).split('/')[0]


def terminal_reward(traj, seat):
    term = traj.get('terminal') or {}
    r = term.get('terminal_reward')
    return None if not isinstance(r, list) or seat >= len(r) else float(r[seat])


def is_natural(traj):
    return (traj.get('terminal') or {}).get('terminal_classification') == 'natural'


# ---------------------------------------------------------------- inputs

def load_base_and_dose(base_path, dose_path):
    return read_json(base_path), json.loads(Path(dose_path).read_text(encoding='utf-8'))


def base_entries(base, dose, run, updates, offset=0, keep_offset=None, pregame_advantage=None, namespace='w5',
                 envelope=None, workers=2, fixed_update=None):
    """`(template config, entries)`: the entries of `updates` updates of the base from global update `offset`, ids and
    seeds as the chain tools stamp them for seed root `run`, with the lean and pregame mode if given. With
    `fixed_update` every stamped episode carries the lean of that one update (a start replication stamps the full vector
    at update 0 on every game) instead of the lean of its own update. The template is the config without its
    iterations (what `collect` completes per chunk)."""
    q = qual()
    cfg = q.build_config(base, dose, 'unused', run, namespace, offset, updates, workers, keep_offset=keep_offset,
                         pregame_advantage=pregame_advantage)
    if keep_offset is not None and fixed_update is not None:
        value = q.keep_offset_value(keep_offset, fixed_update)
        for item in cfg['iterations']:
            for e in item['episodes']:
                if 'london_keep_offset' in e['episode']:
                    e['episode']['london_keep_offset'] = value
    entries = [e for item in cfg['iterations'] for e in item['episodes']]
    template = {k: v for k, v in cfg.items() if k != 'iterations'}
    if envelope:
        template['record_envelope'] = envelope
    return template, entries


def command(program):
    """The argv prefix of a program given as a path or a list; a `.py` path runs under this interpreter (the tests'
    stand-in trainer and scorer are scripts, the real ones are binaries)."""
    if isinstance(program, (list, tuple)):
        return [str(x) for x in program]
    return [sys.executable, str(program)] if str(program).endswith('.py') else [str(program)]


def collect(trainer, template, entries, workdir, chunk_size=27, workers=2, prep_workers=1, keep_runs=False,
            log=None, check=True):
    """Runs the trainer on `entries` in chunks of `chunk_size` (one-iteration collect-only configs from `template`) and
    yields `(entry, trajectory, path)` in order. The trajectory file's bytes are pinned by its collection record
    (checked), its episode id must equal the entry's. A failing chunk raises RuntimeError. The chunk's run directory is
    deleted after the consumer has taken its trajectories unless `keep_runs`."""
    workdir = Path(workdir)
    runs = workdir / 'run'
    cmd = command(trainer)
    for k, start in enumerate(range(0, len(entries), chunk_size)):
        chunk = entries[start:start + chunk_size]
        root = runs / f'c{k:05d}'
        if root.exists():
            shutil.rmtree(root)
        root.mkdir(parents=True)
        cfg = dict(template)
        cfg.update(iterations=[{'episodes': chunk}], output_directory=root.as_posix(), collection_workers=workers,
                   preparation_workers=prep_workers)
        cfg.setdefault('max_non_natural_episode_fraction', 0.5)
        cfg_path = runs / f'c{k:05d}.config.json'
        cfg_path.write_text(json.dumps(cfg) + '\n', encoding='utf-8')
        t0 = time.time()
        with open(workdir / 'trainer.out', 'ab') as so, open(workdir / 'trainer.err', 'ab') as se:
            code = subprocess.call(cmd + [str(cfg_path)], stdout=so, stderr=se)
        complete = root / 'iterations' / '000000' / 'complete.json'
        if code != 0 or not complete.exists():
            raise RuntimeError(f'trainer failed on chunk {k} (exit {code}); see {workdir / "trainer.err"}')
        coll = read_json(read_json(complete)['collection']['path'])
        trajs = coll['trajectories']
        if len(trajs) != len(chunk):
            raise RuntimeError(f'chunk {k}: {len(trajs)} trajectories for {len(chunk)} episodes')
        for entry, tr in zip(chunk, trajs):
            if check and sha256_file(tr['path']) != tr['sha256']:
                raise RuntimeError(f'trajectory {tr["path"]} differs from its pin')
            traj = read_json(tr['path'])
            if traj['episode']['id'] != entry['episode']['id']:
                raise RuntimeError('trajectory order differs from the episode order')
            yield entry, traj, Path(tr['path'])
        if log is not None:
            log({'chunk': k, 'games': len(chunk), 'wall_s': round(time.time() - t0, 1)})
        if not keep_runs:
            shutil.rmtree(root)


def deal_episodes(registrations, n, namespace, seed_label, modes_for, learner_rule=None):
    """Deals for the paired and probe tools: `n` episodes cycling the ordered pairs of `registrations` (a list of deck
    registrations as in a base's `registered` entries), the seat of the learner and the starting player cycling too.
    Seeds are SHA-256 of `namespace/seed_label/i` (so a deal is the same for every mode that reuses `i`). `modes_for`
    maps the learner seat to the two mulligan modes. Returns `[{'i', 'decks', 'entry'}]`, entry = `{'episode',
    'opponent': {'kind': 'current'}}`."""
    d = len(registrations)
    pairs = d * d
    out = []
    for i in range(n):
        own, other, rep = i % d, (i // d) % d, i // pairs
        learner = (own + other + rep) % 2
        start = (own + 2 * other + rep + i // 2) % 2
        seats = [registrations[own], registrations[other]] if learner == 0 else \
            [registrations[other], registrations[own]]
        seed = int.from_bytes(hashlib.sha256(f'{namespace}/{seed_label}/{i}'.encode()).digest()[:8], 'little') & (2 ** 63 - 1)
        ep = {'id': f'{seed_label}-{i:05d}-s{learner}', 'seed': seed, 'starting_player': start,
              'learner_seat': learner, 'registered': seats, 'selected': seats, 'postboard': False,
              'max_physical_decisions': 100000, 'max_policy_steps': 200070,
              'runtime_rules': 'resolution-boundary-v1', 'opening': 'london-v1', 'mulligan_modes': modes_for(learner)}
        out.append({'i': i, 'learner_seat': learner,
                    'decks': [str(r['label']).split('/')[0] for r in seats],
                    'entry': {'episode': ep, 'opponent': {'kind': 'current'}}})
    return out


def registrations_of(base, names):
    """The first registration of each deck name found in the base's episodes, in the order of `names`."""
    found = {}
    for item in base['iterations']:
        for e in item['episodes']:
            for reg in e['episode']['registered']:
                found.setdefault(str(reg['label']).split('/')[0], reg)
    missing = [n for n in names if n not in found]
    if missing:
        raise ValueError(f'the base has no registration for decks {missing}')
    return [found[n] for n in names]


def write_json(path, doc):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(doc, indent=1, sort_keys=True) + '\n', encoding='utf-8', newline='\n')


def markdown_table(headers, rows):
    out = ['| ' + ' | '.join(headers) + ' |', '| ' + ' | '.join('---' for _ in headers) + ' |']
    out += ['| ' + ' | '.join('-' if v is None else (f'{v:.4g}' if isinstance(v, float) else str(v)) for v in r) + ' |'
            for r in rows]
    return '\n'.join(out)
