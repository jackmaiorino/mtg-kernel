"""Start replication of a London arm: measurement only, it trains nothing and chooses no constant.

A collect-only run of the arm's own base (every game of the base cycle) from the parent state, with the arm's whole
lean vector stamped at update 0 and the arm's pregame advantage mode in the config, the trajectories kept. From it:

* the **launch-gate numbers**: the share of learned seats that mulligan the opening seven, among those seats the share
  that end at zero cards and the share that end at five or six, and the kept-size distribution; each against the
  thresholds it is given (`--gate-*`, defaults 15% to 35%, at most 10%, at least 60%), which come from the arm's plan;
* the **value price**: the realized terminal return of seats that kept six cards against seats that kept seven in the
  same stratum (deck, opponent, seat), weighted by the number of six-card keeps, and, on the same seats, the value
  head's estimate at the six-card keep announcement minus its estimate at the seven-card announcement;
* the **bottom decisions**: the entropy (nats, and as a share of ln n) of the put-back choices;
* the decisions and mulligans by own count, the head's mean pregame estimate and the mean realized return at each count.

  mull_start_replication.py run --trainer BIN --base BASE.json --dose PARENT_DOSE.json --out-dir DIR
        (--vector-json CALIBRATION.json | --b0 b0,b1,b2,b3,b4,b5,b6) [--end-update 32768]
        [--pregame-advantage terminal_v1] [--seed-root R] [--updates N] [--chunk-size N] [--workers N]
        [--keep-trajectories] [--envelope zstd1] [--gate-share-low X ...]
  mull_start_replication.py analyze --seats DIR/seats.jsonl --out DIR/report.json [gate options]

`run` writes `seats.jsonl` (one row per learned seat), `report.json`, a text report, and with `--keep-trajectories`
the trajectory files under `DIR/trajectories` with their pins in `trajectories.json`.
"""
import argparse
import json
import math
import re
import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mull_probe as mp  # noqa: E402

GATE_DEFAULTS = {'share_low': 0.15, 'share_high': 0.35, 'zero_max': 0.10, 'five_six_min': 0.60}


def opponent_label(entry):
    opp = entry.get('opponent') or {}
    return f'{opp.get("kind")}/{opp.get("id")}' if opp.get('id') else str(opp.get('kind'))


def seat_rows(entry, traj, game_index):
    """One row per learned seat of a game: its deck, opponent, seat, kept size, mulligans taken, terminal reward and
    classification, announcements (count, value, gap) and bottom decisions (n, logits as bits)."""
    ep = traj['episode']
    modes = ep['mulligan_modes']
    record = traj['opening_record']['seats']
    anns, bottoms = mp.pregame_decisions(traj)
    rows = []
    for seat, rec in enumerate(record):
        if rec['mode'] != 'learned':
            continue
        rows.append({'game': ep['id'], 'i': game_index, 'seat': seat, 'deck': mp.deck_of(traj, seat),
                     'opp_deck': mp.deck_of(traj, 1 - seat), 'opponent': opponent_label(entry),
                     'group': mp.seat_group(modes, seat), 'kept': rec['kept_hand_size'],
                     'taken': rec['mulligans_taken'], 'reward': mp.terminal_reward(traj, seat),
                     'natural': mp.is_natural(traj),
                     'anns': [[a['k'], a['value'], a['gap']] for a in anns if a['actor'] == seat],
                     'bottoms': [[b['n'], b['logits']] for b in bottoms if b['actor'] == seat]})
    return rows


def gate_numbers(seats):
    n = len(seats)
    mull = [s for s in seats if s['taken'] >= 1]
    kept = [sum(1 for s in seats if s['kept'] == k) for k in range(8)]
    return {'learned_seats': n, 'mulligan_the_seven': len(mull),
            'share_mulligan_the_seven': None if not n else len(mull) / n,
            'zero_card_share_of_mulliganers': None if not mull else sum(1 for s in mull if s['kept'] == 0) / len(mull),
            'five_or_six_share_of_mulliganers': (None if not mull else
                                                 sum(1 for s in mull if s['kept'] in (5, 6)) / len(mull)),
            'kept_size_count': kept, 'kept_size_share': [None if not n else c / n for c in kept]}


def evaluate_gate(nums, gate):
    checks = []

    def add(name, value, ok, rule):
        checks.append({'check': name, 'value': value, 'rule': rule, 'pass': bool(ok)})
    share, zero, five = (nums['share_mulligan_the_seven'], nums['zero_card_share_of_mulliganers'],
                         nums['five_or_six_share_of_mulliganers'])
    add('share mulliganing the seven', share, share is not None and gate['share_low'] <= share <= gate['share_high'],
        f'between {gate["share_low"]} and {gate["share_high"]}')
    add('zero-card share of mulliganers', zero, zero is not None and zero <= gate['zero_max'], f'at most {gate["zero_max"]}')
    add('five-or-six share of mulliganers', five, five is not None and five >= gate['five_six_min'],
        f'at least {gate["five_six_min"]}')
    return {'thresholds': dict(gate), 'checks': checks, 'pass': all(c['pass'] for c in checks)}


def value_price(seats):
    """Realized return of six-card keeps against matched seven-card keeps, and the head's price on the same seats."""
    strata = {}
    for s in seats:
        if s['reward'] is None or s['kept'] not in (6, 7):
            continue
        strata.setdefault((s['deck'], s['opponent'], s['seat']), {6: [], 7: []})[s['kept']].append(s['reward'])
    num = var = 0.0
    n_six = strata_used = unmatched = 0
    for cell in strata.values():
        if cell[6] and not cell[7]:
            unmatched += len(cell[6])
    for cell in strata.values():
        if not cell[6] or not cell[7]:
            continue
        n6, n7 = len(cell[6]), len(cell[7])
        s6, s7 = mp.sd(cell[6]), mp.sd(cell[7])
        v6 = 1.0 if s6 is None else s6 * s6   # a returns' variance is at most 1: the fallback is the bound
        v7 = 1.0 if s7 is None else s7 * s7
        num += n6 * (mp.mean(cell[6]) - mp.mean(cell[7]))
        n_six += n6
        var += n6 * n6 * (v6 / n6 + v7 / n7)
        strata_used += 1
    realized = None if not n_six else {'difference_six_minus_seven': num / n_six,
                                       'standard_error': math.sqrt(var) / n_six, 'six_card_keeps_matched': n_six,
                                       'strata_used': strata_used, 'six_card_keeps_without_a_seven_keep': unmatched}
    # the head's own estimates on the same seats: V at the count-1 announcement minus V at the count-0 announcement
    diffs, diffs_six = [], []
    for s in seats:
        by_k = {k: v for k, v, _ in s['anns']}
        if 0 in by_k and 1 in by_k and by_k[0] is not None and by_k[1] is not None:
            diffs.append(by_k[1] - by_k[0])
            if s['kept'] == 6:
                diffs_six.append(by_k[1] - by_k[0])
    head = {'seats_with_a_mulligan': len(diffs), 'mean_v_six_minus_v_seven_all_mulliganers': mp.mean(diffs),
            'seats_keeping_six': len(diffs_six), 'mean_v_six_minus_v_seven_six_keepers': mp.mean(diffs_six)}
    return {'realized_return': realized, 'value_head': head}


def bottom_entropy(seats):
    rows = [(n, mp.entropy_nats([mp.f32(b) for b in logits])) for s in seats for n, logits in s['bottoms']]
    by_n = {}
    for n, h in rows:
        by_n.setdefault(n, []).append(h)
    shares = [h / math.log(n) for n, h in rows if n > 1]
    return {'decisions': len(rows), 'mean_entropy_nats': mp.mean([h for _, h in rows]),
            'mean_share_of_ln_n': mp.mean(shares),
            'by_choices': {str(n): {'decisions': len(hs), 'mean_entropy_nats': mp.mean(hs),
                                    'mean_share_of_ln_n': mp.mean([h / math.log(n) for h in hs])}
                           for n, hs in sorted(by_n.items())}}


def by_count(seats, updates=None):
    """Per own count k: announcements, the mulligans among them (a seat with `taken` mulligans mulliganed at every
    count below `taken`), the mean value head estimate and the mean realized return of the seats asked."""
    out = []
    for k in range(mp.COUNTS):
        rows = [(s, a) for s in seats for a in s['anns'] if a[0] == k]
        mulls = sum(1 for s, _ in rows if s['taken'] > k)
        vals = [a[1] for _, a in rows if a[1] is not None]
        rets = [s['reward'] for s, _ in rows if s['reward'] is not None]
        out.append({'k': k, 'decisions': len(rows), 'mulligans': mulls,
                    'mulligan_again_share': None if not rows else mulls / len(rows),
                    'decisions_per_update': None if not updates else len(rows) / updates,
                    'mean_value': mp.mean(vals), 'mean_realized_return': mp.mean(rets)})
    return out


def analyze(seats, gate=None, updates=None):
    gate = dict(GATE_DEFAULTS, **(gate or {}))
    nums = gate_numbers(seats)
    return {'schema': 'w5-mull-start-replication/v1', 'launch_gate_numbers': nums,
            'launch_gate': evaluate_gate(nums, gate), 'value_price': value_price(seats),
            'bottom_decisions': bottom_entropy(seats), 'by_count': by_count(seats, updates),
            'natural_share': mp.mean([1.0 if s['natural'] else 0.0 for s in seats]),
            'description_only': 'the replication chooses no constant and no mechanism'}


def format_report(rep):
    nums = rep['launch_gate_numbers']
    out = [f'learned seats {nums["learned_seats"]}; kept-size share 7..0: ' +
           ' '.join('-' if nums['kept_size_share'][k] is None else f'{nums["kept_size_share"][k]:.4f}'
                    for k in range(7, -1, -1)),
           'launch gate (' + ('PASS' if rep['launch_gate']['pass'] else 'FAIL') + ')',
           mp.markdown_table(['check', 'value', 'rule', 'pass'], [[c['check'], c['value'], c['rule'], c['pass']]
                                                                  for c in rep['launch_gate']['checks']])]
    vp = rep['value_price']
    r = vp['realized_return']
    out.append('realized return, six-card keeps minus matched seven-card keeps: ' + (
        'none (no matched strata)' if r is None else
        f'{r["difference_six_minus_seven"]:.4f} (SE {r["standard_error"]:.4f}, {r["six_card_keeps_matched"]} keeps in '
        f'{r["strata_used"]} strata, {r["six_card_keeps_without_a_seven_keep"]} unmatched)'))
    h = vp['value_head']
    f = lambda x: 'none' if x is None else f'{x:.4f}'  # noqa: E731
    out.append(f'value head, V(six) minus V(seven) on the same seats: all mulliganers {f(h["mean_v_six_minus_v_seven_all_mulliganers"])} '
               f'(n {h["seats_with_a_mulligan"]}), six-card keepers {f(h["mean_v_six_minus_v_seven_six_keepers"])} '
               f'(n {h["seats_keeping_six"]})')
    b = rep['bottom_decisions']
    out.append(f'bottom decisions: {b["decisions"]}, mean entropy {f(b["mean_entropy_nats"])} nats, '
               f'{f(b["mean_share_of_ln_n"])} of ln n')
    out.append(mp.markdown_table(['k', 'decisions', 'mulligans', 'again share', 'mean V', 'mean realized'],
                                 [[c['k'], c['decisions'], c['mulligans'], c['mulligan_again_share'], c['mean_value'],
                                   c['mean_realized_return']] for c in rep['by_count']]))
    return '\n'.join(out)


def parse_vector(args):
    if args.vector_json:
        vec = json.loads(Path(args.vector_json).read_text(encoding='utf-8'))['vector']
    elif args.b0:
        vec = [float(x) for x in args.b0.split(',')]
    else:
        raise SystemExit('give --vector-json or --b0')
    return vec


def gate_args(args):
    return {'share_low': args.gate_share_low, 'share_high': args.gate_share_high, 'zero_max': args.gate_zero_max,
            'five_six_min': args.gate_five_six_min}


def run(args):
    base, dose = mp.load_base_and_dose(args.base, args.dose)
    keep_offset = {'b0': parse_vector(args), 'end_update': args.end_update, 'shape': 'linear-v1'}
    pregame = None if args.pregame_advantage in (None, 'none') else args.pregame_advantage
    updates = args.updates or len(base['iterations'])
    template, entries = mp.base_entries(base, dose, args.seed_root, updates, offset=0, keep_offset=keep_offset,
                                        pregame_advantage=pregame, envelope=args.envelope, workers=args.workers,
                                        fixed_update=0)
    if args.limit_episodes:
        entries = entries[:args.limit_episodes]
    out_dir = Path(args.out_dir)
    keep_dir = out_dir / 'trajectories'
    if args.keep_trajectories:
        keep_dir.mkdir(parents=True, exist_ok=True)
    seats, pins = [], {}
    with open(out_dir / 'seats.jsonl', 'w', encoding='utf-8', newline='\n') as f:
        log = lambda row: print(json.dumps(row), flush=True)  # noqa: E731
        for i, (entry, traj, path) in enumerate(mp.collect(args.trainer, template, entries, out_dir, args.chunk_size,
                                                          args.workers, log=log)):
            for row in seat_rows(entry, traj, i):
                f.write(json.dumps(row) + '\n')
                seats.append(row)
            if args.keep_trajectories:
                target = keep_dir / (re.sub(r'[^A-Za-z0-9._-]', '_', traj['episode']['id']) + '.json')
                shutil.copyfile(path, target)
                pins[traj['episode']['id']] = {'file': target.name, 'sha256': mp.sha256_file(target)}
    if args.keep_trajectories:
        mp.write_json(out_dir / 'trajectories.json', pins)
    return finish(seats, args, len(entries) / 10.0, Path(out_dir) / 'report.json', extra={
        'keep_offset': keep_offset, 'pregame_advantage': pregame, 'seed_root': args.seed_root,
        'trainer': {'path': str(args.trainer), 'sha256': mp.sha256_file(args.trainer)},
        'base_sha256': mp.sha256_file(args.base), 'dose_sha256': mp.sha256_file(args.dose), 'episodes': len(entries)})


def finish(seats, args, updates, out_path, extra=None):
    rep = analyze(seats, gate_args(args), updates)
    rep.update(extra or {})
    mp.write_json(out_path, rep)
    print(format_report(rep))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest='cmd', required=True)

    def gates(p):
        p.add_argument('--gate-share-low', type=float, default=GATE_DEFAULTS['share_low'])
        p.add_argument('--gate-share-high', type=float, default=GATE_DEFAULTS['share_high'])
        p.add_argument('--gate-zero-max', type=float, default=GATE_DEFAULTS['zero_max'])
        p.add_argument('--gate-five-six-min', type=float, default=GATE_DEFAULTS['five_six_min'])
    r = sub.add_parser('run')
    for k in ('--trainer', '--base', '--dose', '--out-dir'):
        r.add_argument(k, required=True)
    r.add_argument('--vector-json')
    r.add_argument('--b0')
    r.add_argument('--end-update', type=int, default=32768)
    r.add_argument('--pregame-advantage', default='none')
    r.add_argument('--seed-root', default='replication/run')
    r.add_argument('--updates', type=int)
    r.add_argument('--limit-episodes', type=int)
    r.add_argument('--chunk-size', type=int, default=27)
    r.add_argument('--workers', type=int, default=2)
    r.add_argument('--envelope', default='zstd1')
    r.add_argument('--keep-trajectories', action='store_true')
    gates(r)
    a = sub.add_parser('analyze')
    a.add_argument('--seats', required=True)
    a.add_argument('--out', required=True)
    a.add_argument('--updates', type=float)
    gates(a)
    args = ap.parse_args(argv)
    if args.cmd == 'run':
        Path(args.out_dir).mkdir(parents=True, exist_ok=True)
        return run(args)
    seats = [json.loads(line) for line in Path(args.seats).read_text(encoding='utf-8').splitlines() if line.strip()]
    return finish(seats, args, args.updates, args.out)


if __name__ == '__main__':
    sys.exit(main())
