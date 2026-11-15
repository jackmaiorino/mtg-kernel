"""Paired counterfactual read of a London state's own mulligans: does mulliganing beat keeping those very hands?

Hand-agnostic by construction: no hand, deck or card rule is used to pick or to judge a deal. The state under test
plays as the learner seat against a copy of itself fixed to keep seven (`keep_seven` mode, as the arm's fixed opponents
play). Two stages, the same seeds throughout:

1. **Screen.** `--deals` deals are played in full with the state's own mulligan line (the trainer has no pregame-only
   mode, and it refuses a batch that is all non-natural, so a full game is the cheap way to learn what the state does
   with each seven). The deals where the learner mulliganed at least once are the affected deals.
2. **Pairs.** Each affected deal is played twice on the same seed, the same decks, seats and starting player: the state's
   own line again (which must reproduce the screen game: counted), and the learner forced to keep its seven
   (`--force keep_seven`: the learner seat's mulligan mode is `keep_seven`; or `--force offset`: the learner stays
   `learned` with a lean of 1e6 so that it never mulligans). The paired difference is the learner's terminal reward
   with its own line minus its terminal reward with the forced keep. The two lines share the opening deal; later draws
   differ once a mulligan has changed how many cards were drawn.

The report gives the pair count, the mean paired difference, its standard error and a 95% interval, and whether the
lower bound is above 0, over all pairs and over pairs whose two games both ended naturally, and by the kept size of the
own line. It also counts the screened deals, the affected share, own-line replays that did not equal the screen, and
forced games that did not keep seven.

  mull_paired.py run --trainer BIN --base BASE.json --dose STATE_DOSE.json --out-dir DIR
        [--deals N] [--namespace NS] [--force keep_seven|offset] [--decks A,B,...] [--chunk-size N] [--workers N]
  mull_paired.py analyze --pairs DIR/pairs.jsonl [--out report.json]
"""
import argparse
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mull_probe as mp  # noqa: E402

FORCE_OFFSET = 1.0e6  # exactly a float32; subtracted from the mulligan logit it makes a mulligan impossible
Z95 = 1.96


def modes(learner, learner_mode):
    return [learner_mode, 'keep_seven'] if learner == 0 else ['keep_seven', learner_mode]


def with_suffix(deal, suffix, force=None, force_offset=False):
    """A copy of the deal's entry with a new id and, for a forced keep, the learner's mode (or the lean)."""
    entry = json.loads(json.dumps(deal['entry']))
    ep = entry['episode']
    ep['id'] = f'{ep["id"]}-{suffix}'
    if force == 'keep_seven':
        ep['mulligan_modes'] = modes(deal['learner_seat'], 'keep_seven')
    elif force == 'offset':
        ep['london_keep_offset'] = FORCE_OFFSET
    return entry


def outcome(traj, seat):
    rec = traj['opening_record']['seats'][seat]
    return {'taken': rec['mulligans_taken'], 'kept': rec['kept_hand_size'], 'reward': mp.terminal_reward(traj, seat),
            'natural': mp.is_natural(traj), 'mode': rec['mode']}


def screen(args, template, deals, out_dir, log):
    rows = []
    entries = [with_suffix(d, 'screen') for d in deals]
    for deal, (entry, traj, _) in zip(deals, mp.collect(args.trainer, template, entries, out_dir / 'screen',
                                                         args.chunk_size, args.workers, log=log)):
        rows.append({'i': deal['i'], 'learner_seat': deal['learner_seat'], 'decks': deal['decks'],
                     'seed': deal['entry']['episode']['seed'], **outcome(traj, deal['learner_seat'])})
    return rows


def pairs(args, template, deals, screened, out_dir, log):
    chosen = [d for d, s in zip(deals, screened) if s['taken'] >= 1]
    own = [with_suffix(d, 'own') for d in chosen]
    keep = [with_suffix(d, 'keep', args.force) for d in chosen]
    results = {'own': [], 'keep': []}
    for name, entries in (('own', own), ('keep', keep)):
        for d, (entry, traj, _) in zip(chosen, mp.collect(args.trainer, template, entries, out_dir / name,
                                                           args.chunk_size, args.workers, log=log)):
            results[name].append(outcome(traj, d['learner_seat']))
    rows = []
    for d, s, o, k in zip(chosen, [s for s in screened if s['taken'] >= 1], results['own'], results['keep']):
        rows.append({'i': d['i'], 'decks': d['decks'], 'learner_seat': d['learner_seat'], 'screen': s, 'own': o,
                     'keep': k})
    return rows


def interval(diffs):
    n = len(diffs)
    if n == 0:
        return {'n': 0}
    m = mp.mean(diffs)
    s = mp.sd(diffs)
    se = None if s is None else s / math.sqrt(n)
    return {'n': n, 'mean': m, 'sd': s, 'se': se, 'lower95': None if se is None else m - Z95 * se,
            'upper95': None if se is None else m + Z95 * se, 'lower_bound_above_0': None if se is None else m - Z95 * se > 0}


def analyze(rows, deals_screened=None):
    """The paired report from pair rows (each with `own` and `keep` outcomes and the `screen` game)."""
    usable = [r for r in rows if r['own']['reward'] is not None and r['keep']['reward'] is not None]
    diffs = [r['own']['reward'] - r['keep']['reward'] for r in usable]
    natural = [r['own']['reward'] - r['keep']['reward'] for r in usable if r['own']['natural'] and r['keep']['natural']]
    by_size = {}
    for r in usable:
        by_size.setdefault(r['own']['kept'], []).append(r['own']['reward'] - r['keep']['reward'])
    return {'schema': 'w5-mull-paired/v1', 'deals_screened': deals_screened, 'affected_deals': len(rows),
            'affected_share': None if not deals_screened else len(rows) / deals_screened,
            'pairs_without_a_reward': len(rows) - len(usable),
            'own_replay_differs_from_screen': sum(1 for r in rows if (r['own']['kept'], r['own']['reward']) !=
                                                  (r['screen']['kept'], r['screen']['reward'])),
            'forced_game_did_not_keep_seven': sum(1 for r in rows if r['keep']['taken'] != 0),
            'paired_difference_own_minus_forced_keep': interval(diffs),
            'natural_pairs_only': interval(natural),
            'by_own_kept_size': {str(k): interval(v) for k, v in sorted(by_size.items())},
            'note': 'paired on the opening deal; draws after a mulligan differ. No hand or deck rule is used.'}


def format_report(rep):
    f = lambda x: '-' if x is None else (f'{x:.4f}' if isinstance(x, float) else str(x))  # noqa: E731
    rows = [['all pairs', *(f(rep['paired_difference_own_minus_forced_keep'].get(k)) for k in
                            ('n', 'mean', 'se', 'lower95', 'upper95', 'lower_bound_above_0'))],
            ['natural pairs only', *(f(rep['natural_pairs_only'].get(k)) for k in
                                     ('n', 'mean', 'se', 'lower95', 'upper95', 'lower_bound_above_0'))]]
    for size, iv in rep['by_own_kept_size'].items():
        rows.append([f'own kept {size}', *(f(iv.get(k)) for k in ('n', 'mean', 'se', 'lower95', 'upper95',
                                                                 'lower_bound_above_0'))])
    return '\n'.join([f'screened {rep["deals_screened"]} deals, affected {rep["affected_deals"]} '
                      f'(share {f(rep["affected_share"])}); own replays differing from the screen '
                      f'{rep["own_replay_differs_from_screen"]}; forced games not at seven '
                      f'{rep["forced_game_did_not_keep_seven"]}',
                      mp.markdown_table(['pairs', 'n', 'mean diff', 'SE', 'lower 95', 'upper 95', 'lower > 0'], rows)])


def run(args):
    base, dose = mp.load_base_and_dose(args.base, args.dose)
    names = args.decks.split(',') if args.decks else mp.DEFAULT_DECKS
    regs = mp.registrations_of(base, names)
    deals = mp.deal_episodes(regs, args.deals, args.namespace, 'pair', lambda L: modes(L, 'learned'))
    template, _ = mp.base_entries(base, dose, args.namespace, 1, workers=args.workers)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    log = lambda row: print(json.dumps(row), flush=True)  # noqa: E731
    screened = screen(args, template, deals, out_dir, log)
    mp.write_json(out_dir / 'screen.json', screened)
    rows = pairs(args, template, deals, screened, out_dir, log)
    with open(out_dir / 'pairs.jsonl', 'w', encoding='utf-8', newline='\n') as f:
        for r in rows:
            f.write(json.dumps(r) + '\n')
    rep = analyze(rows, len(deals))
    rep.update({'force': args.force, 'namespace': args.namespace, 'trainer_sha256': mp.sha256_file(args.trainer),
                'base_sha256': mp.sha256_file(args.base), 'dose_sha256': mp.sha256_file(args.dose)})
    mp.write_json(out_dir / 'report.json', rep)
    print(format_report(rep))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest='cmd', required=True)
    r = sub.add_parser('run')
    for k in ('--trainer', '--base', '--dose', '--out-dir'):
        r.add_argument(k, required=True)
    r.add_argument('--deals', type=int, default=4000)
    r.add_argument('--namespace', default='paired/read')
    r.add_argument('--force', choices=('keep_seven', 'offset'), default='keep_seven')
    r.add_argument('--decks')
    r.add_argument('--chunk-size', type=int, default=27)
    r.add_argument('--workers', type=int, default=2)
    a = sub.add_parser('analyze')
    a.add_argument('--pairs', required=True)
    a.add_argument('--deals-screened', type=int)
    a.add_argument('--out')
    args = ap.parse_args(argv)
    if args.cmd == 'run':
        return run(args)
    rows = [json.loads(line) for line in Path(args.pairs).read_text(encoding='utf-8').splitlines() if line.strip()]
    rep = analyze(rows, args.deals_screened)
    if args.out:
        mp.write_json(args.out, rep)
    print(format_report(rep))
    return 0


if __name__ == '__main__':
    sys.exit(main())
