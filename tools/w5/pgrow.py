"""P-grow opponent rosters (collab LANES/chains-20261107/DESIGN.md section 2, the P-grow row of the pool table).

A CG arm is a C-recipe arm whose fixed-parent share of the base (episodes whose opponent is `{"id": <replaces>,
"kind": "fixed"}`, the 50% d6107922 share of the 30/50/20 schedule) plays the chain's own checkpoint ladder instead:
every `stride`-th boundary's state to date, the chain's start state (boundary 0, the arm's parent) included. The base's
history-relative opponent kinds are refused (segments would change them), so each segment gets a roster of named
opponents (`ladder-u<boundary>`), fixed when its config is derived.

A fork (`history`): a CG chain started from another chain's state at boundary U (its arm parent) can carry that chain's
ladder before U as a pinned prefix (`ladder-h<generation>-u<boundary>`, a `w5-ladder-history/v1` document written by
`pgrow.py history`, bound to the arm parent by its `fork` checkpoint), so "the chain's own ladder to date" spans the
lineage it forked from, a fork of a fork included. History states come first in ladder order; the fork's own states
follow (its boundary 0 is the fork state itself).

Determinism. For segment s at global update offset o = s L:
- the ladder to date is the history (if any), then every boundary b = k stride with 0 <= b <= o (stride is a multiple
  of L, so every ladder state is a segment end, persisted before segment s starts);
- the roster is the whole ladder while it has at most `roster_max` states; past that it is the `roster_max` states with
  the smallest keys sha256(json([SUBSET_TAG, seed_root, s, id])) (id the member's opponent id), a uniform subset without
  replacement, listed in ladder order (the native trainer accepts at most 128 named opponents per run, the base's
  other named opponents included);
- each replaced episode (global update u, episode slot j) plays roster member
  int(sha256(json([DRAW_TAG, seed_root, u, j]))) mod (roster size).
So each episode's opponent is uniform over the ladder to date (exactly per episode while the ladder fits the roster;
marginally, by the uniform subset, once it does not), and the roster and every draw are a pure function of the seed
root, the segment index and the ladder (its boundaries and hashes). The draw keys hold no training outcome, so the
draw stream is fixed before any update runs.

Verification (fail closed). A ladder state b > 0 is the output checkpoint of the segment that ends at b: the segment
must be complete with all L iterations, and its checkpoint must hash to its receipt's pin (`next_source`, as every
segment's start). The chain's `LADDER.json` records each state's checkpoint and after-state hashes at its first
appearance (fsynced with its directory before the segment is appended); a member's later use requires the same
hashes, and a record that lacks a state the manifest already reached refuses, so a rewritten receipt or checkpoint,
or a lost record, stops the chain instead of changing a roster. Boundary 0 is the parent's dose pin (checked by the
config build). A missing or corrupt state raises `chains.Stop`.
"""
import copy
import hashlib
import json
import os
import re
from pathlib import Path

ARM_KEYS = ('stride', 'roster_max', 'replaces', 'test_stride', 'history')
DRAW_TAG = 'chains-pgrow-draw/v1'
SUBSET_TAG = 'chains-pgrow-subset/v1'
LADDER_SCHEMA = 'w5-ladder/v1'
HISTORY_SCHEMA = 'w5-ladder-history/v1'
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


def history_id(generation, boundary):
    """A history member: the state at `boundary` of the lineage's `generation`-th chain (1 the chain that started
    from the arm parent of the first fork, 2 that fork, and so on), so ids never collide along a lineage."""
    return f'ladder-h{generation}-u{boundary}'


def sha_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def check_history(doc, stride, parent_source):
    """A `w5-ladder-history/v1` document's states, checked: ids `ladder-h<generation>-u<boundary>` in ascending
    (generation, boundary) order at stride multiples, every checkpoint present at its pin, none the fork state, and
    the document's `fork` (the state it was cut at) the arm parent itself, so a history only joins its own lineage."""
    if doc.get('schema') != HISTORY_SCHEMA or doc.get('stride') != stride:
        raise ValueError(f'the history is not a {HISTORY_SCHEMA} document at stride {stride}')
    parent = (parent_source.get('checkpoint') or {}).get('sha256')
    if not parent or ((doc.get('fork') or {}).get('checkpoint') or {}).get('sha256') != parent:
        raise ValueError('the history was cut at another state than the arm parent (its fork checkpoint differs)')
    states = doc.get('states') or []
    keys = [(st.get('generation'), st.get('boundary')) for st in states]
    if not states or any(type(g) is not int or g < 1 or type(b) is not int or b < 0 or b % stride for g, b in keys) \
            or keys != sorted(set(keys)) or [st.get('id') for st in states] != [history_id(g, b) for g, b in keys]:
        raise ValueError('history states must be distinct ladder-h<generation>-u<boundary> states in ascending order '
                         'at multiples of the stride')
    for st in states:
        ck = st['checkpoint']
        if not Path(ck['path']).exists() or sha_file(ck['path']) != ck['sha256']:
            raise ValueError(f'history state {st["id"]} differs from its pin ({ck["path"]})')
        if ck['sha256'] == parent:
            raise ValueError(f'history state {st["id"]} is the fork state itself (the arm parent)')
    return states


def replaced(entry, replaces):
    return entry.get('opponent') == {'id': replaces, 'kind': 'fixed'}


def check_spec(spec, base, segment_updates, long_stride, parent_source=None, pin=None):
    """The arm's normalized `ladder` block, or a ValueError naming what the plan refuses. `pin(path)` pins a history
    document (chains.pin)."""
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
    if any(o.startswith(('ladder-u', 'ladder-h')) for o in others):
        raise ValueError('the base already names a ladder opponent')
    if type(roster_max) is not int or not 1 <= roster_max <= TRAINER_OPPONENTS_MAX - len(others):
        raise ValueError(f'roster_max must be 1 to {TRAINER_OPPONENTS_MAX - len(others)} (the trainer\'s '
                         f'{TRAINER_OPPONENTS_MAX} named opponents less the base\'s other {len(others)})')
    out = {'stride': stride, 'roster_max': roster_max, 'replaces': replaces, 'test_stride': test}
    if spec.get('history') is not None:
        check_history(json.loads(Path(spec['history']).read_text(encoding='utf-8')), stride, parent_source or {})
        out['history'] = pin(spec['history'])  # absent otherwise: a fresh CG arm's file is unchanged
    return out


def history_states(spec, parent_source):
    """[(id, source, after_state_sha256)] of the arm's pinned history (none without one): the parent's source (same
    lineage, same import) with each history checkpoint; the document is re-checked against the arm's pin and its
    fork state against the parent."""
    pinned = spec.get('history')
    if not pinned:
        return []
    if not Path(pinned['path']).exists() or sha_file(pinned['path']) != pinned['sha256']:
        raise LadderError(f'the ladder history {pinned["path"]} differs from its pin')
    doc = json.loads(Path(pinned['path']).read_text(encoding='utf-8'))
    if ((doc.get('fork') or {}).get('checkpoint') or {}).get('sha256') != \
            (parent_source.get('checkpoint') or {}).get('sha256'):
        raise LadderError('the ladder history was cut at another state than the arm parent')
    out = []
    for st in doc['states']:
        source = copy.deepcopy(parent_source)
        source['checkpoint'] = dict(st['checkpoint'])
        out.append((st['id'], source, st.get('after_state_sha256')))
    return out


def boundaries(offset, stride):
    """The ladder boundaries to date for a segment starting at global update `offset`."""
    return list(range(0, offset + 1, stride))


def roster(seed_root, segment, ids, roster_max):
    """The segment's roster: every ladder member while they fit, else a hashed uniform subset (ladder order)."""
    if len(ids) <= roster_max:
        return list(ids)
    chosen = set(sorted(ids, key=lambda i: (_key([SUBSET_TAG, seed_root, segment, i]), str(i)))[:roster_max])
    return [i for i in ids if i in chosen]


def draw(seed_root, update, slot, n):
    """The roster index replaced episode `slot` of global update `update` plays."""
    return _key([DRAW_TAG, seed_root, update, slot]) % n


def ladder(parent_source, segments, offset, spec, record_path, next_source, chain=None, select=None):
    """[(id, source)] for the roster members of the ladder to date at `offset` (`select(ids)`, by default every
    member), each verified (module docstring). A state not yet in the record at `record_path` is verified and recorded
    at its first appearance whether or not it is a member; a recorded non-member is not read again, so a segment start
    hashes at most the roster and the new states, not the whole ladder. `spec` is the arm's ladder block; `segments`
    are the chain manifest's; `next_source(root)` is chains.next_source (the segment's last source with its output
    checkpoint, checked against the receipt pin)."""
    stride = spec['stride']
    record_path = Path(record_path)
    record = json.loads(record_path.read_text(encoding='utf-8')) if record_path.exists() else \
        {'schema': LADDER_SCHEMA, 'chain': chain, 'stride': stride, 'states': {}}
    if record.get('schema') != LADDER_SCHEMA or record.get('stride') != stride or \
            (chain is not None and record.get('chain') != chain):
        raise LadderError(f'{record_path} is not this chain\'s ladder record at stride {stride}')
    history = {i: (source, after) for i, source, after in history_states(spec, parent_source)}
    ends = {seg['offset'] + seg['updates']: seg for seg in segments}
    # Every state to date at the manifest's last segment was recorded (and fsynced) before that segment was appended,
    # so one missing now means the record was lost or edited: refuse rather than pin it again from current files.
    if segments:
        reached = boundaries(max(seg['offset'] for seg in segments), stride)
        lost = [i for i in list(history) + [member_id(b) for b in reached] if i not in record['states']]
        if lost:
            raise LadderError(f'{record_path} lacks established ladder states {lost[:8]}')
    own = {member_id(b): b for b in boundaries(offset, stride)}
    ids = list(history) + list(own)
    members = set(ids if select is None else select(ids))
    out, added = [], False
    for i in ids:
        if i not in members and i in record['states']:
            continue
        if i in history:
            source, after = copy.deepcopy(history[i][0]), history[i][1]
            ck = source['checkpoint']
            if not Path(ck['path']).exists() or sha_file(ck['path']) != ck['sha256']:
                raise LadderError(f'ladder state {i}: {ck["path"]} is missing or differs from its pin')
        elif own[i] == 0:
            source, after = copy.deepcopy(parent_source), None
        else:
            b = own[i]
            seg = ends.get(b)
            if seg is None:
                raise LadderError(f'no segment ends at ladder boundary {b}')
            root = Path(seg['root'])
            done = len(list((root / 'iterations').glob('*/complete.json'))) if (root / 'iterations').exists() else 0
            if not (root / 'completion.json').exists() or done != seg['updates']:
                raise LadderError(f'the segment ending at ladder boundary {b} ({root}) is not complete')
            try:
                source = next_source(root)
                complete = json.loads((root / 'iterations' / f'{seg["updates"] - 1:06d}' / 'complete.json')
                                      .read_text(encoding='utf-8'))
                after = json.loads(Path(complete['update']['path']).read_text(encoding='utf-8'))['after_state_sha256']
            except Exception as exc:  # chains.Stop: a missing checkpoint or one that differs from its receipt
                raise LadderError(f'ladder state {b}: {exc}') from exc
        pinned = source.get('checkpoint')
        if not pinned or not pinned.get('sha256'):
            raise LadderError(f'ladder state {i} has no checkpoint pin')
        # The checkpoint file hash pins the bytes this chain plays (the file names its own run's paths, so replicas in
        # other roots differ); the after-state hash is the root-independent identity of the state.
        entry = {'sha256': pinned['sha256'], 'path': pinned['path'], 'after_state_sha256': after}
        have = record['states'].get(i)
        if have is None:
            record['states'][i] = entry
            added = True
        elif (have['sha256'], have.get('after_state_sha256')) != (entry['sha256'], entry['after_state_sha256']):
            raise LadderError(f'ladder state {i} hashes to {pinned["sha256"]} (after state '
                              f'{entry["after_state_sha256"]}), recorded {have["sha256"]} '
                              f'({have.get("after_state_sha256")})')
        if i in members:
            out.append((i, source))
    if added:
        record_path.parent.mkdir(parents=True, exist_ok=True)
        tmp = record_path.with_name(record_path.name + '.tmp')
        with open(tmp, 'w', encoding='utf-8', newline='\n') as f:
            f.write(json.dumps(record, indent=1, sort_keys=True) + '\n')
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, record_path)
        if os.name != 'nt':  # the rename is durable before the segment that relies on it is appended
            fd = os.open(record_path.parent, os.O_RDONLY)
            try:
                os.fsync(fd)
            finally:
                os.close(fd)
    return out


def apply(cfg, spec, states, seed_root, segment, offset):
    """Rewrites a segment config built from the base: replaced episodes play their drawn roster member, and the
    config's named opponents are the base's others plus the members drawn in this segment (ladder order).
    `states` are [(opponent id, source)]: the ladder's states to date or just the segment's roster members
    (pgrow.ladder with `select`): the roster of a roster is itself. Returns the roster record (hashes, per-member draw
    counts) for logging."""
    by_id = dict(states)
    members = roster(seed_root, segment, [i for i, _ in states], spec['roster_max'])
    counts = {i: 0 for i in members}
    for k, item in enumerate(cfg['iterations']):
        update = offset + k
        for j, entry in enumerate(item['episodes']):
            if replaced(entry, spec['replaces']):
                i = members[draw(seed_root, update, j, len(members))]
                entry['opponent'] = {'id': i, 'kind': 'fixed'}
                counts[i] += 1
    used = [i for i in members if counts[i]]
    cfg['opponents'] = [o for o in cfg.get('opponents', []) if o['id'] != spec['replaces']] + \
        [{'id': i, 'source': copy.deepcopy(by_id[i])} for i in used]
    if len(cfg['opponents']) > TRAINER_OPPONENTS_MAX:
        raise LadderError(f'{len(cfg["opponents"])} named opponents pass the trainer\'s {TRAINER_OPPONENTS_MAX}')
    return {'segment': segment, 'offset': offset, 'roster': members,
            'hashes': {i: by_id[i]['checkpoint']['sha256'] for i in members},
            'draws': {i: counts[i] for i in members}}


def write_history(chain_manifest, until, stride, out, copy_to=None):
    """`pgrow.py history`: a fork's ladder prefix from a chain cut at boundary `until` (a positive stride multiple at
    a segment end): the chain's inherited history, if it is itself a fork (re-verified against its arm pin), then its
    own ladder states before `until` as generation g + 1 (g the inherited generation, 0 for a chain without history),
    each checkpoint verified against its receipt or pin. With `copy_to`, every checkpoint, the cut state's included,
    is copied there by content (`<sha256>.json`), so the fork reads no other tree. `<out>.parent.json` is the fork's
    dose record (the cut state: the chain's source there with that checkpoint); the history's `fork` names it."""
    import gzip
    import shutil
    man = json.loads(Path(chain_manifest).read_text(encoding='utf-8'))
    ends = {seg['offset'] + seg['updates']: seg for seg in man['segments']}
    if until <= 0 or until % stride or until not in ends:
        raise LadderError(f'the fork boundary {until} must be a positive multiple of {stride} at a segment end')

    def place(ck):
        """A verified checkpoint pin, copied by content under `copy_to` when given."""
        if not Path(ck['path']).exists() or sha_file(ck['path']) != ck['sha256']:
            raise LadderError(f'{ck["path"]} is missing or differs from its pin')
        if not copy_to:
            return dict(ck)
        dst = Path(copy_to) / f'{ck["sha256"]}.json'
        if not dst.exists():
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ck['path'], dst.with_name(dst.name + '.tmp'))
            os.replace(dst.with_name(dst.name + '.tmp'), dst)
        if sha_file(dst) != ck['sha256']:
            raise LadderError(f'{dst} differs from {ck["sha256"]}')
        return {'path': dst.as_posix(), 'sha256': ck['sha256']}

    def segment_end(b):
        root = Path(ends[b]['root'])
        if not (root / 'completion.json').exists():
            raise LadderError(f'the segment ending at {b} is not complete')
        complete = json.loads((root / 'iterations' / f'{ends[b]["updates"] - 1:06d}' / 'complete.json')
                              .read_text(encoding='utf-8'))
        upd = Path(complete['update']['path'])
        ck = {'path': (upd.parent / 'checkpoint.json').as_posix(),
              'sha256': complete['output_identity']['checkpoint_sha256']}
        source = copy.deepcopy(complete['source'])
        source['checkpoint'] = place(ck)
        return source, json.loads(upd.read_text(encoding='utf-8'))['after_state_sha256']

    arm = json.loads(Path(man['arm_file']['path']).read_text(encoding='utf-8'))
    if sha_file(man['arm_file']['path']) != man['arm_file']['sha256']:
        raise LadderError('the chain\'s arm file differs from its manifest pin')
    parent_source = json.loads(Path(arm['parent']['path']).read_text(encoding='utf-8'))['source']
    inherited = []
    if (arm.get('ladder') or {}).get('history'):
        if arm['ladder']['stride'] != stride:
            raise LadderError('a fork of a fork keeps its lineage\'s stride')
        for i, source, after in history_states(arm['ladder'], parent_source):
            g, b = (int(x) for x in re.fullmatch(r'ladder-h(\d+)-u(\d+)', i).groups())
            ck = source['checkpoint']
            if sha_file(ck['path']) != ck['sha256']:
                raise LadderError(f'inherited history state {i} differs from its pin')
            inherited.append({'id': i, 'generation': g, 'boundary': b, 'checkpoint': place(ck),
                              'after_state_sha256': after})
    generation = max((st['generation'] for st in inherited), default=0) + 1
    fork_source, fork_after = segment_end(until)
    states = list(inherited)
    for b in range(0, until, stride):
        if b == 0:
            first = Path(man['segments'][0]['config'])
            cfg = json.loads(first.read_text(encoding='utf-8')) if first.exists() else \
                json.loads(gzip.open(str(first) + '.gz', 'rt', encoding='utf-8').read())
            ck, after = place(cfg['initial_source']['checkpoint']), None
        else:
            source, after = segment_end(b)
            ck = source['checkpoint']
        states.append({'id': history_id(generation, b), 'generation': generation, 'boundary': b, 'checkpoint': ck,
                       'after_state_sha256': after})
    doc = {'schema': HISTORY_SCHEMA, 'chain': man['chain'], 'seed_root': man['seed_root'], 'stride': stride,
           'fork': {'boundary': until, 'checkpoint': fork_source['checkpoint'], 'after_state_sha256': fork_after},
           'states': states}
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    Path(out).write_text(json.dumps(doc, indent=1, sort_keys=True) + '\n', encoding='utf-8', newline='\n')
    parent = Path(str(out).removesuffix('.json') + '.parent.json')
    parent.write_text(json.dumps({'origin': {'chain': man['chain'], 'boundary': until,
                                             'after_state_sha256': fork_after}, 'source': fork_source},
                                 indent=1, sort_keys=True) + '\n', encoding='utf-8', newline='\n')
    return doc, parent


def main(argv=None):
    import argparse
    ap = argparse.ArgumentParser(description='P-grow ladder tools')
    sub = ap.add_subparsers(dest='cmd', required=True)
    h = sub.add_parser('history', help='a fork\'s ladder prefix and dose record from a chain manifest')
    h.add_argument('--chain-manifest', required=True)
    h.add_argument('--until', type=int, required=True)
    h.add_argument('--stride', type=int, default=STRIDE)
    h.add_argument('--copy-to')
    h.add_argument('--out', required=True)
    a = ap.parse_args(argv)
    doc, parent = write_history(a.chain_manifest, a.until, a.stride, a.out, a.copy_to)
    print(json.dumps({'out': a.out, 'states': len(doc['states']), 'parent': parent.as_posix()}))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
