"""In-context calibration of a per-count lean vector for a London arm: measurement only, it trains nothing.

The lean the trainer subtracts from a mulligan logit has one entry per number k of mulligans the acting seat has already
taken (0 to 6). The rule for entry k is the one used for a single scalar lean, applied at every count: the median of the
parent's own mulligan-minus-keep logit gap at count k, plus ln 9 (so the mulligan probability on the median hand is
10%), rounded to 4 decimals and floored at 0. Nothing about a hand, a deck or a card enters.

The gaps are read in the arm's own context: the arm's own base mixture is played by the parent state itself with no lean
field (a saturated parent mulligans at every count, so every count is reached by the walk itself, never forced), and
every announcement of every learned seat is kept. The median is weighted so that the seats facing a fixed keep-seven
opponent seat and the seats in self-play count 70% and 30% at each count, however many announcements each group makes.
A count with fewer than `--min-announcements` announcements (100), or with an empty group, stops the tool:
`UNREACHED k` on stderr, exit 3.

  mull_calibrate.py run --trainer BIN --base BASE.json --dose PARENT_DOSE.json --out-dir DIR
                    [--seed-root R] [--updates N] [--chunk-size N] [--workers N] [--limit-episodes N]
                    [--lockstep-scores SCORES.jsonl] [--no-tensors]
  mull_calibrate.py analyze --announcements DIR/announcements.jsonl --out DIR/calibration.json
                    [--updates N] [--lockstep-scores SCORES.jsonl]

`run` writes `announcements.jsonl` (one record per announcement, with its request tensor unless --no-tensors: the fixed
states `mull_fixedhand.py` scores), `calibration.json` and a text table. `--lockstep-scores` is a scorer output with
`hand_size` and `gap` per row (both seats mulliganing in lockstep: the earlier probe), printed beside the in-context
numbers; it chooses nothing.

The predicted start (description): for each seat whose gaps were seen at all seven counts, the chance of each kept hand
size under the vector, p(mulligan at k) = sigmoid(gap_k - b_k), weighted 70/30 by group, and the predicted decisions
per update at each count. It is a prediction from a saturated walk and says how the arm should begin, not how it will.
"""
import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mull_probe as mp  # noqa: E402

WEIGHTS = {'keep_seven': 0.7, 'self_play': 0.3}
MIN_ANNOUNCEMENTS = 100


class Unreached(Exception):
    def __init__(self, k, n, why):
        super().__init__(f'UNREACHED {k}: {why} ({n} announcements)')
        self.k, self.n = k, n


def group_weights(records):
    """Per record the weight that makes each group's total weight its share (0.7, 0.3) at the record's count."""
    n = {}
    for r in records:
        n[(r['k'], r['group'])] = n.get((r['k'], r['group']), 0) + 1
    return [WEIGHTS[r['group']] / n[(r['k'], r['group'])] for r in records]


def b_value(median):
    """round(median + ln 9, 4), floored at 0."""
    return max(0.0, round(median + mp.LN9, 4))


def per_count(records, min_n=MIN_ANNOUNCEMENTS):
    out = []
    for k in range(mp.COUNTS):
        rk = [r for r in records if r['k'] == k]
        if len(rk) < min_n:
            raise Unreached(k, len(rk), f'under {min_n}')
        groups = {g: [r for r in rk if r['group'] == g] for g in WEIGHTS}
        empty = [g for g, rs in groups.items() if not rs]
        if empty:
            raise Unreached(k, len(rk), f'no announcements in group {empty[0]}')
        gaps = [r['gap'] for r in rk]
        median = mp.weighted_median(gaps, group_weights(rk))
        row = {'k': k, 'n': len(rk), 'n_keep_seven': len(groups['keep_seven']), 'n_self_play': len(groups['self_play']),
               'median_weighted': median, 'b': b_value(median),
               'median_keep_seven': mp.quantile([r['gap'] for r in groups['keep_seven']], 0.5),
               'median_self_play': mp.quantile([r['gap'] for r in groups['self_play']], 0.5),
               'median_pooled': mp.quantile(gaps, 0.5), 'p10': mp.quantile(gaps, 0.1), 'p90': mp.quantile(gaps, 0.9)}
        out.append(row)
    return out


def seats_of(records):
    """`{(game, actor): {k: gap}}` and the group of each seat."""
    seats, group = {}, {}
    for r in records:
        key = (r['game'], r['actor'])
        seats.setdefault(key, {})[r['k']] = r['gap']
        group[key] = r['group']
    return seats, group


def predicted_start(records, vector, seats_per_update=None):
    """The predicted kept-size distribution and decisions per update at each count from the complete seats."""
    seats, group = seats_of(records)
    complete = {s: g for s, g in seats.items() if set(g) == set(range(mp.COUNTS))}
    n_group = {g: sum(1 for s in complete if group[s] == g) for g in WEIGHTS}
    if not all(n_group.values()):
        return {'complete_seats': len(complete), 'incomplete_seats': len(seats) - len(complete),
                'note': 'a group has no complete seat: no prediction'}
    size = [0.0] * 8       # kept hand size 0..7, weighted
    reach = [0.0] * mp.COUNTS  # chance a seat is asked at count k
    for s, gaps in complete.items():
        w = WEIGHTS[group[s]] / n_group[group[s]]
        p = [mp.sigmoid(gaps[k] - vector[k]) for k in range(mp.COUNTS)]
        asked = 1.0
        for k in range(mp.COUNTS):
            reach[k] += w * asked
            size[7 - k] += w * asked * (1.0 - p[k])
            asked *= p[k]
        size[0] += w * asked  # mulliganed at every count: zero cards
    mull7 = 1.0 - size[7]
    out = {'complete_seats': len(complete), 'incomplete_seats': len(seats) - len(complete),
           'kept_size_share': {str(k): size[k] for k in range(8)}, 'mean_kept': sum(k * size[k] for k in range(8)),
           'share_mulligan_the_seven': mull7,
           'zero_card_share_of_mulliganers': None if mull7 <= 0 else size[0] / mull7,
           'five_or_six_share_of_mulliganers': None if mull7 <= 0 else (size[5] + size[6]) / mull7,
           'reach_share_by_count': reach}
    if seats_per_update:
        out['predicted_decisions_per_update'] = [seats_per_update * r for r in reach]
    return out


def lockstep_numbers(path):
    """Median gap and median + ln 9 by count from a scorer output (rows with `hand_size` and `gap`), or None."""
    if not path:
        return None
    by_k = {}
    for line in Path(path).read_text(encoding='utf-8').splitlines():
        if line.strip():
            r = json.loads(line)
            by_k.setdefault(7 - int(r['hand_size']), []).append(float(r['gap']))
    return [{'k': k, 'n': len(by_k[k]), 'median': mp.quantile(by_k[k], 0.5),
             'b': b_value(mp.quantile(by_k[k], 0.5))} for k in sorted(by_k) if 0 <= k < mp.COUNTS]


def analyze(records, updates=None, lockstep=None, min_n=MIN_ANNOUNCEMENTS):
    rows = per_count(records, min_n)
    vector = [r['b'] for r in rows]
    seats, _ = seats_of(records)
    learned_seats = sum(1 for g in seats.values() if 0 in g)
    out = {'schema': 'w5-mull-calibrate/v1', 'rule': 'b_k = max(0, round(weighted median gap at count k + ln 9, 4)); '
           'median weights keep_seven 0.7, self_play 0.3 per count; field absent in the walk',
           'weights': WEIGHTS, 'min_announcements': min_n, 'announcements': len(records), 'learned_seats': learned_seats,
           'updates_covered': updates, 'per_count': rows, 'vector': vector}
    if not any(v > 0 for v in vector):
        raise ValueError('every entry of the vector is 0: nothing to lean on')
    out['predicted_start'] = predicted_start(records, vector, None if not updates else learned_seats / updates)
    lock = lockstep_numbers(lockstep)
    if lock:
        out['lockstep'] = lock
        by_k = {r['k']: r for r in lock}
        out['shift_in_context_minus_lockstep'] = [
            None if r['k'] not in by_k else r['median_weighted'] - by_k[r['k']]['median'] for r in rows]
    return out


def format_table(out):
    lines = [f'vector b = {out["vector"]}', '']
    lock = {r['k']: r for r in out.get('lockstep', [])}
    headers = ['k', 'n', 'n keep7', 'n self', 'median (70/30)', 'b', 'median keep7', 'median self', 'p10', 'p90']
    if lock:
        headers += ['lockstep median', 'lockstep b']
    rows = []
    for r in out['per_count']:
        row = [r['k'], r['n'], r['n_keep_seven'], r['n_self_play'], r['median_weighted'], r['b'],
               r['median_keep_seven'], r['median_self_play'], r['p10'], r['p90']]
        if lock:
            row += [lock[r['k']]['median'] if r['k'] in lock else None, lock[r['k']]['b'] if r['k'] in lock else None]
        rows.append(row)
    lines.append(mp.markdown_table(headers, rows))
    ps = out['predicted_start']
    if 'kept_size_share' in ps:
        lines += ['', f'predicted start from {ps["complete_seats"]} complete seats ({ps["incomplete_seats"]} incomplete): '
                  f'share mulliganing the seven {ps["share_mulligan_the_seven"]:.4f}, mean kept {ps["mean_kept"]:.3f}',
                  mp.markdown_table(['kept size'] + [str(k) for k in range(7, -1, -1)],
                                    [['share'] + [ps['kept_size_share'][str(k)] for k in range(7, -1, -1)]])]
    return '\n'.join(lines)


def read_announcements(path):
    return [json.loads(line) for line in Path(path).read_text(encoding='utf-8').splitlines() if line.strip()]


def record_of(entry, traj, a, game_index, with_tensor):
    ep = traj['episode']
    modes = ep['mulligan_modes']
    rec = {'game': ep['id'], 'i': game_index, 'actor': a['actor'], 'k': a['k'], 'group': mp.seat_group(modes, a['actor']),
           'deck': mp.deck_of(traj, a['actor']), 'opp_deck': mp.deck_of(traj, 1 - a['actor']),
           'hand_size': 7 - a['k'], 'mull_row': a['mull_row'], 'keep_row': a['keep_row'], 'gap': a['gap'],
           'logits_bits': a['logits'], 'value': a['value']}
    if with_tensor:
        rec['tensor'] = a['tensor']
    return rec


def run(args):
    base, dose = mp.load_base_and_dose(args.base, args.dose)
    updates = args.updates or len(base['iterations'])
    template, entries = mp.base_entries(base, dose, args.seed_root, updates, envelope=args.envelope,
                                        workers=args.workers)
    if args.limit_episodes:
        entries = entries[:args.limit_episodes]
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / 'announcements.jsonl'
    records = []
    with open(path, 'w', encoding='utf-8', newline='\n') as f:
        log = lambda row: print(json.dumps(row), flush=True)  # noqa: E731
        for i, (entry, traj, _) in enumerate(mp.collect(args.trainer, template, entries, out_dir, args.chunk_size,
                                                        args.workers, log=log)):
            for a in mp.pregame_decisions(traj, with_tensor=not args.no_tensors)[0]:
                rec = record_of(entry, traj, a, i, not args.no_tensors)
                f.write(json.dumps(rec) + '\n')
                records.append({k: v for k, v in rec.items() if k != 'tensor'})
    covered = len(entries) / 10.0 if args.limit_episodes else updates
    return finish(records, args, covered)


def finish(records, args, updates):
    try:
        out = analyze(records, updates, getattr(args, 'lockstep_scores', None), args.min_announcements)
    except Unreached as exc:
        print(str(exc), file=sys.stderr)
        return 3
    out_path = Path(getattr(args, 'out', None) or Path(args.out_dir) / 'calibration.json')
    mp.write_json(out_path, out)
    print(format_table(out))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest='cmd', required=True)
    r = sub.add_parser('run')
    for k in ('--trainer', '--base', '--dose', '--out-dir'):
        r.add_argument(k, required=True)
    r.add_argument('--seed-root', default='calibration/run')
    r.add_argument('--updates', type=int)
    r.add_argument('--chunk-size', type=int, default=27)
    r.add_argument('--workers', type=int, default=2)
    r.add_argument('--limit-episodes', type=int)
    r.add_argument('--envelope')
    r.add_argument('--no-tensors', action='store_true')
    r.add_argument('--lockstep-scores')
    r.add_argument('--min-announcements', type=int, default=MIN_ANNOUNCEMENTS)
    a = sub.add_parser('analyze')
    a.add_argument('--announcements', required=True)
    a.add_argument('--out', required=True)
    a.add_argument('--updates', type=float)
    a.add_argument('--lockstep-scores')
    a.add_argument('--min-announcements', type=int, default=MIN_ANNOUNCEMENTS)
    args = ap.parse_args(argv)
    if args.cmd == 'run':
        return run(args)
    return finish(read_announcements(args.announcements), args, args.updates)


if __name__ == '__main__':
    sys.exit(main())
