"""Synthetic tests for the SD1 analyzer (tools/sd1/sd1_analyze.py): a known effect is recovered, the bootstrap
indices are shared across statistics and scorings, the nonnatural worst cases move in each call's direction, the
frozen choice follows the seeded tie order, Holm steps down as declared, and games resample S1's unit."""
import random
import struct
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import sd1_analyze as sa  # noqa: E402

REPS = 400  # replicates in tests (the formal value is 10,000)


def bits(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def binomial(rng, n, q):
    return sum(rng.random() < q for _ in range(n))


def make_record(rng, episode, q_true, logits, arms, half=256, nonnatural=0.0):
    """One synthetic reached record. q_true: per-action true win probability (no draws). arms: {label: [per-seed
    dict(n, wins)]} root counts. nonnatural: chance a reference rollout ends nonnaturally (scored apart)."""
    width = len(q_true)
    p, _, _ = sa.target_ref.softmax([sa.target_ref.f32(bits(z)) for z in logits])
    refs = {'half_rollouts': half, 'coverage_max': 16, 'full_coverage': True, 'actions': list(range(width)),
            'q': {}, 'vp': {}}
    for h in ('a', 'b'):
        wins, draws, nns = [], [], []
        for j in range(width):
            w = nn = 0
            for _ in range(half):
                if rng.random() < nonnatural:
                    nn += 1
                elif rng.random() < q_true[j]:
                    w += 1
            wins.append(w)
            draws.append(0)
            nns.append(nn)
        refs['q'][h] = {'wins': wins, 'draws': draws, 'nonnatural': nns}
        w = nn = 0
        drawn = [0] * width
        for _ in range(half):
            a = sa_draw(rng, p)
            drawn[a] += 1
            if rng.random() < nonnatural:
                nn += 1
            elif rng.random() < q_true[a]:
                w += 1
        refs['vp'][h] = {'wins': w, 'draws': 0, 'nonnatural': nn, 'drawn': drawn}
    rows = []
    for label, seeds in arms.items():
        for s, root in enumerate(seeds):
            n, wins = root['n'], root['wins']
            row = {'arm': label, 'world': 'fresh', 'search_seed': s, 'seed_root': f'x/{label}/{s}', 'status': 'ok',
                   'cpu_s': 1.0, 'visits': sum(n), 'n': n, 'wins': wins, 'draws': [0] * width,
                   'perm_live': root.get('perm', list(range(width))),
                   'search': {'attempted': sum(n), 'completed': sum(n), 'nonnatural': 0}}
            row['frozen_choice'] = sa.frozen_choice(row)
            row['pick_v2'] = sa.pick_v2(row)
            rows.append(row)
    return {'schema': sa.RECORD_SCHEMA, 'episode': episode, 'game': f'g{episode}', 'step': 3, 'reached': True,
            'width': width, 'policy': {'logit_bits': [bits(z) for z in logits]}, 'arms': rows, 'references': refs}


def sa_draw(rng, p):
    u, acc = rng.random(), 0.0
    for a, pa in enumerate(p):
        acc += pa
        if u < acc:
            return a
    return len(p) - 1


def good_root(q_true, sims=400):
    """A root whose counts follow the true values (search that finds the better action)."""
    n = [sims // len(q_true)] * len(q_true)
    return {'n': n, 'wins': [round(q * k) for q, k in zip(q_true, n)]}


def flat_root(width, sims=400):
    """A root that learned nothing: equal counts and values, so q = m_S softmax(equal) on the support."""
    n = [sims // width] * width
    return {'n': n, 'wins': [k // 2 for k in n]}


class KnownEffect(unittest.TestCase):
    def setUp(self):
        rng = random.Random(7)
        self.records = []
        self.expected_tgt = []
        self.expected_pick = []
        for e in range(120):
            q_true = [0.35 + 0.3 * rng.random() for _ in range(3)]
            logits = [rng.gauss(0, 1) for _ in range(3)]
            p, _, _ = sa.target_ref.softmax([sa.target_ref.f32(bits(z)) for z in logits])
            root = good_root(q_true)
            t = sa.target_ref.target([bits(z) for z in logits], root['n'], root['wins'], [0, 0, 0])
            self.expected_tgt.append(sum((t['q'][j] - p[j]) * q_true[j] for j in range(3)))
            pick = max(range(3), key=lambda j: (root['wins'][j] / root['n'][j], -j))
            self.expected_pick.append(q_true[pick] - sum(p[j] * q_true[j] for j in range(3)))
            arms = {'D64': [root, root], 'P64': [root, root], 'T64': [root, root], 'D256': [flat_root(3)]}
            self.records.append(make_record(rng, e, q_true, logits, arms))

    def test_known_effect_is_recovered_and_called(self):
        states = [sa.state_forms(r) for r in self.records]
        values = sa.per_state_values(states)
        stats = sa.summarize(values, sa.tag_seed(sa.BOOT_TAG), REPS, sa.DELTA_P)
        true_tgt = sum(self.expected_tgt) / len(self.expected_tgt)
        true_pick = sum(self.expected_pick) / len(self.expected_pick)
        self.assertAlmostEqual(stats['D64.tgt']['point']['estimand'], true_tgt, delta=0.006)
        self.assertAlmostEqual(stats['D64.pick']['point']['estimand'], true_pick, delta=0.006)
        self.assertGreater(true_pick, 0.03)
        self.assertEqual(stats['D64.pick']['call'], 'positive')
        # Identical arms give identical statistics and a zero contrast.
        self.assertEqual(stats['T64.pick']['point'], stats['P64.pick']['point'])
        self.assertEqual(stats['T64-P64.pick']['point']['estimand'], 0.0)
        self.assertEqual(stats['T64-P64.pick']['call'], 'excluded')
        # Headroom exists by construction (the actions' values differ).
        self.assertEqual(stats['hstar']['call'], 'positive')
        rows, triggers = sa.readout(stats)
        self.assertTrue(rows['Headroom']['holds'])
        self.assertFalse(rows['Alignment']['holds_part_p'])
        self.assertTrue(triggers['3a G1 (D, P or T at 64k or 256k has a positive G^pick)'])

    def test_bootstrap_indices_are_shared(self):
        xs = [float(i % 7) for i in range(50)]
        values = {'a': {'estimand': xs}, 'b': {'estimand': [x + 1.0 for x in xs]},
                  'c': {'estimand': xs, 'gmin': [2 * x for x in xs]}}
        reps = sa.bootstrap(values, 123, REPS)
        # b = a + 1 state by state: every sorted replicate is shifted by exactly 1 only with shared indices.
        for ra, rb in zip(reps['a']['estimand'], reps['b']['estimand']):
            self.assertAlmostEqual(rb - ra, 1.0, places=12)
        self.assertEqual(reps['a']['estimand'], reps['c']['estimand'])
        for ra, rg in zip(reps['a']['estimand'], reps['c']['gmin']):
            self.assertAlmostEqual(rg, 2 * ra, places=12)
        # The same seed replays exactly; another seed differs.
        self.assertEqual(reps, sa.bootstrap(values, 123, REPS))
        self.assertNotEqual(reps['a'], sa.bootstrap(values, 124, REPS)['a'])

    def test_resampling_unit_is_the_state(self):
        # Contrast replicates equal the difference of arm replicates only when both use one index draw per state.
        states = [sa.state_forms(r) for r in self.records[:40]]
        values = sa.per_state_values(states, ['D64.pick', 'D256.pick', 'D256-D64.pick'])
        n = len(states)
        rng = sa.SplitMix64(99)
        idx = [sa.unbiased_index(n, rng.next_u64) for _ in range(n)]
        mean = lambda name: sum(values[name]['estimand'][i] for i in idx) / n  # noqa: E731
        self.assertAlmostEqual(mean('D256-D64.pick'), mean('D256.pick') - mean('D64.pick'), places=12)
        reps = sa.bootstrap(values, 99, 1)
        self.assertAlmostEqual(reps['D256-D64.pick']['estimand'][0], mean('D256-D64.pick'), places=12)


class WorstCase(unittest.TestCase):
    def test_directions(self):
        rng = random.Random(3)
        rec = make_record(rng, 0, [0.3, 0.7], [0.0, 0.0], {'D64': [good_root([0.3, 0.7])]}, half=64,
                          nonnatural=0.2)
        st = sa.state_forms(rec)
        forms = sa.statistic_forms(st)
        for name in ('D64.tgt', 'D64.pick', 'hstar', 'oracle'):
            est = forms[name].value(st['cells'], 'estimand')
            lo = forms[name].value(st['cells'], 'gmin')
            hi = forms[name].value(st['cells'], 'gmax')
            self.assertLessEqual(lo, est + 1e-15, name)
            self.assertLessEqual(est, hi + 1e-15, name)
        # G^pick picks action 1: Qbar(1) - Vbar; gmin scores Q's nonnatural 0 and V_p's 1.
        f = forms['D64.pick']
        c = st['cells']
        qa, qb, va, vb = c[('q', 'a', 1)], c[('q', 'b', 1)], c[('v', 'a', None)], c[('v', 'b', None)]
        want = 0.5 * (qa[0] / 64 + qb[0] / 64) - 0.5 * ((va[0] + va[2]) / 64 + (vb[0] + vb[2]) / 64)
        self.assertAlmostEqual(f.value(c, 'gmin'), want, places=12)
        self.assertGreater(qa[2] + va[2], 0)

    def test_contrast_uses_combined_coefficients(self):
        cells = {('q', 'a', 0): (10, 0, 4, 20)}
        x = sa.Form().add(('q', 'a', 0), 1.0)
        y = sa.Form().add(('q', 'a', 0), 0.25)
        d = x.plus(y, -1.0)
        self.assertAlmostEqual(d.value(cells, 'gmin'), 0.75 * 10 / 20)
        self.assertAlmostEqual(d.value(cells, 'gmax'), 0.75 * 14 / 20)


class Picks(unittest.TestCase):
    def test_frozen_choice_ties_follow_the_seeded_order(self):
        arm = {'n': [8, 8, 8, 2], 'wins': [6, 6, 3, 2], 'draws': [0] * 4, 'perm_live': [3, 1, 0, 2]}
        self.assertEqual(sa.frozen_choice(arm), 1)
        arm['perm_live'] = [0, 1, 2, 3]
        self.assertEqual(sa.frozen_choice(arm), 0)
        self.assertIsNone(sa.frozen_choice({'n': [7, 7], 'wins': [7, 0], 'draws': [0, 0], 'perm_live': [0, 1]}))
        # Value-leaf trees read the value units.
        vl = {'n': [8, 8], 'wins': [0, 0], 'draws': [0, 0], 'value_units': [3 << 20, 5 << 20], 'perm_live': [0, 1]}
        self.assertEqual(sa.frozen_choice(vl), 1)

    def test_pick_v2(self):
        self.assertEqual(sa.pick_v2({'n': [9, 9, 3], 'wins': [3, 5, 3], 'draws': [2, 0, 0]}), 1)
        self.assertEqual(sa.pick_v2({'n': [9, 9, 3], 'wins': [4, 3, 3], 'draws': [0, 2, 0]}), 0)

    def test_engine_pick_mismatch_is_refused(self):
        rng = random.Random(1)
        rec = make_record(rng, 0, [0.3, 0.7], [0.0, 0.0], {'D64': [good_root([0.3, 0.7])]}, half=8)
        rec['arms'][0]['frozen_choice'] = 0
        with self.assertRaises(sa.AnalyzeError):
            sa.state_forms(rec)


class Calls(unittest.TestCase):
    def test_p_values_match_the_index_convention(self):
        reps = sorted([-0.01] * 30 + [0.02] * 370)  # 30 of 400 at or below zero
        p = sa.p_positive(reps, 0.02, sa.DELTA_P)
        for alpha in (0.10, 0.16, 0.2):
            lower = reps[sa.lower_index(alpha, len(reps))]
            self.assertEqual(lower > 0, p <= alpha, alpha)
        self.assertEqual(sa.p_positive(reps, 0.001, sa.DELTA_P), 1.0)
        e = sa.p_excluded(reps, 0.015)
        for alpha in (0.5, 0.9, 0.95):
            upper = reps[sa.upper_index(alpha, len(reps))]
            self.assertEqual(upper < 0.015, e <= alpha, alpha)

    def test_holm_steps_down(self):
        def stat(shift):
            return {s: [shift + 0.001 * (i % 5) for i in range(60)] for s in sa.SCORINGS}
        values = {'big': stat(0.05), 'mid': stat(0.03), 'zero': stat(0.0)}
        rows = [{'row': r, 'components': [{'stat': r, 'kind': 'positive'}]} for r in ('zero', 'mid', 'big')]
        scored, _ = sa.holm(rows, values, 5, REPS, sa.DELTA_P)
        by = {r['row']: r for r in scored}
        self.assertTrue(by['big']['confirmed'] and by['mid']['confirmed'])
        self.assertFalse(by['zero']['confirmed'])
        self.assertAlmostEqual(min(r['holm_level'] for r in scored), 0.05 / 3)
        # Intersection-union: a row with a failing component fails.
        rows = [{'row': 'both', 'components': [{'stat': 'big', 'kind': 'positive'},
                                               {'stat': 'zero', 'kind': 'positive'}]}]
        scored, _ = sa.holm(rows, values, 5, REPS, sa.DELTA_P)
        self.assertFalse(scored[0]['confirmed'])


class Games(unittest.TestCase):
    def cases(self, effect):
        rng = random.Random(11)
        out = []
        for f in range(3):
            for o in range(4):
                for fam in range(4):
                    base = rng.random()
                    for seat in (0, 1):
                        for arm in ('plain', 'D'):
                            p = min(1.0, base * 0.8 + (effect if arm == 'D' else 0.0))
                            out.append({'focal_deck': f, 'opposing_deck': o, 'seed_family': fam, 'seat': seat,
                                        'arm': arm, 'score': 1.0 if rng.random() < p else 0.0,
                                        'nonnatural': False})
        return out

    def test_units_and_strata(self):
        cases = self.cases(0.0)
        result, units = sa.game_bootstrap(cases, [('D', 'plain'), ('D', None)], 17, REPS)
        self.assertEqual(sorted(units), [0, 1, 2])
        self.assertTrue(all(len(u) == 16 for u in units.values()))
        # Equal focal-deck weights: the point is the mean of per-deck means.
        per_deck = []
        for f in range(3):
            xs = [c['score'] for c in cases if c['focal_deck'] == f and c['arm'] == 'D']
            per_deck.append(sum(xs) / len(xs))
        self.assertAlmostEqual(result[('D', None)]['point'], sum(per_deck) / 3, places=12)
        # A unit carries both seats and both arms: shuffling seats or arms inside units leaves every replicate
        # unchanged, while moving a case across units would not.
        swapped = [dict(c, seat=1 - c['seat']) for c in cases]
        again, _ = sa.game_bootstrap(swapped, [('D', 'plain')], 17, REPS)
        self.assertEqual(again[('D', 'plain')]['reps'], result[('D', 'plain')]['reps'])

    def test_known_game_effect_and_worst_case(self):
        calls = sa.game_calls(self.cases(0.3), [('D', 'plain')], seed=3, replicates=REPS)
        self.assertEqual(calls[('D', 'plain')]['call'], 'positive')
        cases = self.cases(0.0)
        cases[0]['nonnatural'] = True
        lo = sa.game_score(cases[0], 1, 'gmin')
        hi = sa.game_score(cases[0], 1, 'gmax')
        self.assertEqual((lo, sa.game_score(cases[0], 1, 'estimand'), hi), (0.0, 0.5, 1.0))
        self.assertEqual(sa.game_score(cases[0], -1, 'gmin'), 1.0)


if __name__ == '__main__':
    unittest.main()
