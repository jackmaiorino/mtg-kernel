"""P-grow opponent rosters (collab LANES/chains-20261107/DESIGN.md section 2, the P-grow row of the pool table).

A CG arm is a C-recipe arm whose fixed-parent share of the base (episodes whose opponent is `{"id": <replaces>,
"kind": "fixed"}`, the 50% d6107922 share of the 30/50/20 schedule) plays the chain's own checkpoint ladder instead:
every `stride`-th boundary's state to date, the chain's start state (boundary 0, the arm's parent) included. The base's
history-relative opponent kinds are refused (segments would change them), so each segment gets a roster of named
opponents (`ladder-u<boundary>`), fixed when its config is derived.

Determinism. For segment s at global update offset o = s L:
- the ladder to date is every boundary b = k stride with 0 <= b <= o (stride is a multiple of L, so every ladder state
  is a segment end, persisted before segment s starts);
- the roster is the whole ladder while it has at most `roster_max` states; past that it is the `roster_max` states with
  the smallest keys sha256(json([SUBSET_TAG, seed_root, s, b])), a uniform subset without replacement, listed in
  boundary order (the native trainer accepts at most 128 named opponents per run, the base's other named opponents
  included);
- each replaced episode (global update u, episode slot j) plays roster member
  int(sha256(json([DRAW_TAG, seed_root, u, j]))) mod (roster size).
So each episode's opponent is uniform over the ladder to date (exactly per episode while the ladder fits the roster;
marginally, by the uniform subset, once it does not), and the roster and every draw are a pure function of the seed
root, the segment index and the ladder (its boundaries and hashes). The draw keys hold no training outcome, so the
draw stream is fixed before any update runs.

Verification (fail closed). A ladder state b > 0 is the output checkpoint of the segment that ends at b: the segment
must be complete with all L iterations, and its checkpoint must hash to its receipt's pin (`next_source`, as every
segment's start). The chain's `LADDER.json` records each state's hash at its first use; every later roster build
requires the same hash, so a rewritten receipt or checkpoint stops the chain instead of changing a roster. Boundary 0
is the parent's dose pin (checked by the config build). A missing or corrupt state raises `chains.Stop`.
"""
import copy
import hashlib
import json
import os
from pathlib import Path

ARM_KEYS = ('stride', 'roster_max', 'replaces', 'test_stride')
DRAW_TAG = 'chains-pgrow-draw/v1'
SUBSET_TAG = 'chains-pgrow-subset/v1'
LADDER_SCHEMA = 'w5-ladder/v1'
STRIDE = 4096
ROSTER_MAX = 64
REPLACES = 'parent'
TRAINER_OPPONENTS_MAX = 128  # native_expanded_training_run_v1: "opponent roster exceeds 128 entries"


class LadderError(Exception):
    """A ladder state is missing, incomplete or differs from its pin (raised as chains.Stop by the caller)."""


def _key(parts):
    return int(hashlib.sha256(json.dumps(list(parts), separators=(',', ':')).encode()).hexdigest(), 16)


def member_id(boundary):
    return f'ladder-u{boundary}'


def replaced(entry, replaces):
    return entry.get('opponent') == {'id': replaces, 'kind': 'fixed'}


def check_spec(spec, base, segment_updates, long_stride):
    """The arm's normalized `ladder` block, or a ValueError naming what the plan refuses."""
    spec = dict(spec or {})
    unknown = set(spec) - set(ARM_KEYS)
    if unknown:
        raise ValueError(f'unknown ladder keys {sorted(unknown)}')
    stride = spec.get('stride', STRIDE)
    roster_max = spec.get('roster_max', ROSTER_MAX)
    replaces = spec.get('replaces', REPLACES)
    test = spec.get('test_stride', False)
    if type(stride) is not int or stride < 1 or stride % segment_updates:
        raise ValueError(f'the ladder stride must be a positive multiple of the segment length ({segment_updates}), '
                         'so every ladder state is a segment end')
    if test not in (True, False):
        raise ValueError('test_stride is true or false')
    if stride % long_stride and not test:
        raise ValueError(f'the ladder stride must be a multiple of {long_stride} (the states rolling retention keeps '
                         'for the chain\'s life); a shorter stride is for smokes only (test_stride)')
    others = [o['id'] for o in base.get('opponents', []) if o['id'] != replaces]
    if not isinstance(replaces, str) or replaces not in {o['id'] for o in base.get('opponents', [])}:
        raise ValueError(f'the base has no named opponent {replaces!r} to replace')
    if not any(replaced(e, replaces) for it in base['iterations'] for e in it['episodes']):
        raise ValueError(f'no base episode plays {replaces!r}')
    if any(o.startswith('ladder-u') for o in others):
        raise ValueError('the base already names a ladder-u opponent')
    if type(roster_max) is not int or not 1 <= roster_max <= TRAINER_OPPONENTS_MAX - len(others):
        raise ValueError(f'roster_max must be 1 to {TRAINER_OPPONENTS_MAX - len(others)} (the trainer\'s '
                         f'{TRAINER_OPPONENTS_MAX} named opponents less the base\'s other {len(others)})')
    return {'stride': stride, 'roster_max': roster_max, 'replaces': replaces, 'test_stride': test}


def boundaries(offset, stride):
    """The ladder boundaries to date for a segment starting at global update `offset`."""
    return list(range(0, offset + 1, stride))


def roster(seed_root, segment, bounds, roster_max):
    """The segment's roster: every ladder boundary while they fit, else a hashed uniform subset (boundary order)."""
    if len(bounds) <= roster_max:
        return list(bounds)
    ranked = sorted(bounds, key=lambda b: (_key([SUBSET_TAG, seed_root, segment, b]), b))
    return sorted(ranked[:roster_max])


def draw(seed_root, update, slot, n):
    """The roster index replaced episode `slot` of global update `update` plays."""
    return _key([DRAW_TAG, seed_root, update, slot]) % n


def ladder(parent_source, segments, offset, stride, record_path, next_source, chain=None):
    """[(boundary, source)] for the ladder to date at `offset`, each state verified (module docstring). `segments` are
    the chain manifest's; `next_source(root)` is chains.next_source (the segment's last source with its output
    checkpoint, checked against the receipt pin). The record at `record_path` is extended with new states."""
    record_path = Path(record_path)
    record = json.loads(record_path.read_text(encoding='utf-8')) if record_path.exists() else \
        {'schema': LADDER_SCHEMA, 'chain': chain, 'stride': stride, 'states': {}}
    if record.get('schema') != LADDER_SCHEMA or record.get('stride') != stride or \
            (chain is not None and record.get('chain') != chain):
        raise LadderError(f'{record_path} is not this chain\'s ladder record at stride {stride}')
    ends = {seg['offset'] + seg['updates']: seg for seg in segments}
    out, added = [], False
    for b in boundaries(offset, stride):
        if b == 0:
            source = copy.deepcopy(parent_source)
        else:
            seg = ends.get(b)
            if seg is None:
                raise LadderError(f'no segment ends at ladder boundary {b}')
            root = Path(seg['root'])
            done = len(list((root / 'iterations').glob('*/complete.json'))) if (root / 'iterations').exists() else 0
            if not (root / 'completion.json').exists() or done != seg['updates']:
                raise LadderError(f'the segment ending at ladder boundary {b} ({root}) is not complete')
            try:
                source = next_source(root)
            except Exception as exc:  # chains.Stop: a missing checkpoint or one that differs from its receipt
                raise LadderError(f'ladder state {b}: {exc}') from exc
        pinned = source.get('checkpoint')
        if not pinned or not pinned.get('sha256'):
            raise LadderError(f'ladder state {b} has no checkpoint pin')
        have = record['states'].get(str(b))
        if have is None:
            record['states'][str(b)] = {'sha256': pinned['sha256'], 'path': pinned['path']}
            added = True
        elif have['sha256'] != pinned['sha256']:
            raise LadderError(f'ladder state {b} hashes to {pinned["sha256"]}, recorded {have["sha256"]}')
        out.append((b, source))
    if added:
        record_path.parent.mkdir(parents=True, exist_ok=True)
        tmp = record_path.with_name(record_path.name + '.tmp')
        with open(tmp, 'w', encoding='utf-8', newline='\n') as f:
            f.write(json.dumps(record, indent=1, sort_keys=True) + '\n')
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, record_path)
    return out


def apply(cfg, spec, states, seed_root, segment, offset):
    """Rewrites a segment config built from the base: replaced episodes play their drawn roster member, and the
    config's named opponents are the base's others plus the members drawn in this segment (boundary order).
    Returns the roster record (boundaries, hashes, per-member draw counts) for logging."""
    by_b = dict(states)
    members = roster(seed_root, segment, [b for b, _ in states], spec['roster_max'])
    counts = {b: 0 for b in members}
    for i, item in enumerate(cfg['iterations']):
        update = offset + i
        for j, entry in enumerate(item['episodes']):
            if replaced(entry, spec['replaces']):
                b = members[draw(seed_root, update, j, len(members))]
                entry['opponent'] = {'id': member_id(b), 'kind': 'fixed'}
                counts[b] += 1
    used = [b for b in members if counts[b]]
    cfg['opponents'] = [o for o in cfg.get('opponents', []) if o['id'] != spec['replaces']] + \
        [{'id': member_id(b), 'source': copy.deepcopy(by_b[b])} for b in used]
    if len(cfg['opponents']) > TRAINER_OPPONENTS_MAX:
        raise LadderError(f'{len(cfg["opponents"])} named opponents pass the trainer\'s {TRAINER_OPPONENTS_MAX}')
    return {'segment': segment, 'offset': offset, 'ladder': [b for b, _ in states], 'roster': members,
            'hashes': {str(b): by_b[b]['checkpoint']['sha256'] for b in members},
            'draws': {str(b): counts[b] for b in members}}
