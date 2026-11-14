"""London base for arm M (TOOLING-T5 item 4): chain A's base with the London opening on every episode.

  london_base.py --base BASE.json --out LONDON-BASE.json [--check]

Every episode gets `opening: "london-v1"` and `mulligan_modes` (one mode per physical seat, the trainer's own field
names and spellings), appended after the episode's existing keys:
  - an episode against the `current` opponent (self-play): ["learned", "learned"];
  - an episode against a declared `fixed` opponent (d6107922 and A48 in chain A's base): `learned` for the episode's
    learner seat, `keep_seven` for the other seat (the opponents' pregame rows are frozen and untrained).
`max_physical_decisions` and `max_policy_steps` of every episode are raised by 70 (the London pregame decisions), but
never past the trainer's limits (100000 physical decisions, 1000000 policy steps; the trainer refuses an episode above
them). Chain A's physical cap is already 100000, so it stays there while its policy-step cap becomes 200070; the
summary lists the cut as `capped_at_trainer_limit`. A base with a cap above a limit is refused.
Nothing else changes: key order, every other value and the base's own opponent sources are kept. The output is
`json.dumps(indent=1)` of the base in its own key order plus a newline (LF), so its hash reproduces; the expected
sha256 is written beside it as `LONDON-BASE.json.sha256` (`<hex>  <file name>`, the sha256sum form).

Refused (nothing is written): a base that already has an opening, mulligan modes, a keep offset, a reset start or a
search opponent on any episode (so the +70 is applied exactly once); an opponent kind other than `current` and
`fixed`; a fixed opponent the base does not declare; a learner seat other than 0 or 1; a missing or non-integer cap.
--check prints what would be written (counts and the output's sha256) and writes nothing.

The parent and opponent sources need the FlatV7 feature transfer for a London game: see v7_dose.py. The two
conversions touch disjoint fields, so they commute.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path

SCHEMA = 'mtg-kernel-native-expanded-training-run/v1'
OPENING = 'london-v1'
LEARNED = 'learned'
KEEP_SEVEN = 'keep_seven'
CAP_KEYS = ('max_physical_decisions', 'max_policy_steps')
CAP_RAISE = 70
# The trainer's hard limits on an episode (ExpandedEpisodeV1 `invalid episode limits`: physical decisions 1..=100_000,
# policy steps 1..=1_000_000). A raised cap never goes past its limit: chain A's physical cap already sits at it.
CAP_CEILING = {'max_physical_decisions': 100_000, 'max_policy_steps': 1_000_000}
SEATS = (0, 1)
# Episode fields a base converted by this builder must not carry (the trainer refuses most of them with an opening).
REFUSED_EPISODE_KEYS = ('opening', 'mulligan_modes', 'london_keep_offset', 'reset', 'opponent_search')


class LondonBaseError(ValueError):
    """A refusal: the base is not one this builder converts."""


def is_int(value):
    return isinstance(value, int) and not isinstance(value, bool)


def declared_opponents(base):
    """The ids of the base's named opponents (its `opponents` list)."""
    ids = []
    for item in base.get('opponents') or []:
        if not isinstance(item, dict) or not isinstance(item.get('id'), str):
            raise LondonBaseError('an opponents entry has no string id')
        if item['id'] in ids:
            raise LondonBaseError(f'opponent id {item["id"]!r} is declared twice')
        ids.append(item['id'])
    return ids


def episode_modes(opponent, learner_seat, declared, where):
    """The two seat modes of one episode, by its opponent entry and its learner seat."""
    if not isinstance(opponent, dict):
        raise LondonBaseError(f'{where}: no opponent entry')
    kind = opponent.get('kind')
    if kind == 'current':
        if set(opponent) != {'kind'}:
            raise LondonBaseError(f'{where}: a current opponent carries only its kind')
        return [LEARNED, LEARNED]
    if kind == 'fixed':
        if set(opponent) != {'kind', 'id'}:
            raise LondonBaseError(f'{where}: a fixed opponent carries exactly its kind and id')
        if opponent['id'] not in declared:
            raise LondonBaseError(f'{where}: fixed opponent {opponent["id"]!r} is not declared by the base')
        modes = [KEEP_SEVEN, KEEP_SEVEN]
        modes[learner_seat] = LEARNED
        return modes
    raise LondonBaseError(f'{where}: unknown opponent kind {kind!r} (current and fixed only)')


def convert_entry(entry, declared, where, stats):
    if not isinstance(entry, dict) or not isinstance(entry.get('episode'), dict):
        raise LondonBaseError(f'{where}: not an episode entry')
    episode = entry['episode']
    for key in REFUSED_EPISODE_KEYS:
        if key in episode:
            raise LondonBaseError(f'{where}: the episode already has {key!r} (the base is not an unconverted base)')
    seat = episode.get('learner_seat')
    if not is_int(seat) or seat not in SEATS:
        raise LondonBaseError(f'{where}: learner_seat {seat!r} is not 0 or 1')
    modes = episode_modes(entry.get('opponent'), seat, declared, where)
    for key in CAP_KEYS:
        if not is_int(episode.get(key)) or episode[key] < 0:
            raise LondonBaseError(f'{where}: {key} is missing or not a non-negative integer')
        if episode[key] > CAP_CEILING[key]:
            raise LondonBaseError(f'{where}: {key} {episode[key]} is above the trainer limit {CAP_CEILING[key]}')
        stats['caps'][key]['before'].add(episode[key])
        raised = min(episode[key] + CAP_RAISE, CAP_CEILING[key])
        if raised != episode[key] + CAP_RAISE:
            stats['capped'][key] = stats['capped'].get(key, 0) + 1
        episode[key] = raised
        stats['caps'][key]['after'].add(episode[key])
    episode['opening'] = OPENING
    episode['mulligan_modes'] = modes
    opponent = entry['opponent']
    name = 'current' if opponent['kind'] == 'current' else f'fixed:{opponent["id"]}'
    stats['by_opponent'][name] = stats['by_opponent'].get(name, 0) + 1
    stats['by_learner_seat'][str(seat)] = stats['by_learner_seat'].get(str(seat), 0) + 1
    mode_key = '/'.join(modes)
    stats['by_modes'][mode_key] = stats['by_modes'].get(mode_key, 0) + 1


def build(base):
    """(London base, summary) from an unconverted base; `base` is not modified."""
    if not isinstance(base, dict) or base.get('schema') != SCHEMA:
        raise LondonBaseError(f'not a {SCHEMA} config')
    iterations = base.get('iterations')
    if not isinstance(iterations, list) or not iterations:
        raise LondonBaseError('the base has no iterations')
    declared = declared_opponents(base)
    doc = copy.deepcopy(base)
    stats = {'caps': {key: {'before': set(), 'after': set()} for key in CAP_KEYS}, 'by_opponent': {},
             'by_learner_seat': {}, 'by_modes': {}, 'capped': {}}
    episodes = 0
    for i, item in enumerate(doc['iterations']):
        entries = item.get('episodes') if isinstance(item, dict) else None
        if not isinstance(entries, list) or not entries:
            raise LondonBaseError(f'iteration {i} has no episodes')
        for j, entry in enumerate(entries):
            convert_entry(entry, declared, f'iteration {i} episode {j}', stats)
            episodes += 1
    summary = {'iterations': len(iterations), 'episodes': episodes, 'by_opponent': stats['by_opponent'],
               'by_learner_seat': stats['by_learner_seat'], 'by_modes': stats['by_modes'],
               'caps': {key: {'before': sorted(v['before']), 'after': sorted(v['after'])}
                        for key, v in stats['caps'].items()}}
    if stats['capped']:  # present only when a raise was cut at the trainer limit, with the episodes it applied to
        summary['capped_at_trainer_limit'] = stats['capped']
    return doc, summary


def render(doc):
    """The exact bytes written: indent 1, the document's own key order, LF, one trailing newline."""
    return (json.dumps(doc, indent=1) + '\n').encode('utf-8')


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def write_atomic(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f'{path.name}.{os.getpid()}.tmp')
    with open(tmp, 'wb') as f:
        f.write(data)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


def write_with_hash(doc, out):
    """Writes the base and `<out>.sha256`; returns the base's sha256. An existing different output is refused."""
    out = Path(out)
    data = render(doc)
    digest = sha256_bytes(data)
    if out.exists() and out.read_bytes() != data:
        raise LondonBaseError(f'{out} exists with different content (never overwritten)')
    write_atomic(out, data)
    write_atomic(out.with_name(out.name + '.sha256'), f'{digest}  {out.name}\n'.encode('utf-8'))
    return digest


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument('--base', required=True, help="chain A's base (r2 base)")
    ap.add_argument('--out', required=True, help='the London base to write; its sha256 goes beside it')
    ap.add_argument('--check', action='store_true', help='print what would be written and write nothing')
    a = ap.parse_args(argv)
    raw = Path(a.base).read_bytes()
    try:
        doc, summary = build(json.loads(raw.decode('utf-8')))
        summary['input_sha256'] = sha256_bytes(raw)
        summary['output_sha256'] = sha256_bytes(render(doc))
        summary['out'] = Path(a.out).as_posix()
        summary['written'] = False
        if not a.check:
            write_with_hash(doc, a.out)
            summary['written'] = True
    except LondonBaseError as exc:
        raise SystemExit(f'refused: {exc}')
    print(json.dumps(summary, indent=1, sort_keys=True))


if __name__ == '__main__':
    main()
