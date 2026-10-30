"""W4 Step A tooling: split rules, allocation, probes' AUC and fits, A2 statistics."""
import hashlib
import sys
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools" / "w4"))
import common  # noqa: E402
from a2_positions import allocate  # noqa: E402
from a2_read import paired, signal_share  # noqa: E402
from prep import half  # noqa: E402


class SplitTests(unittest.TestCase):
    def test_game_half_is_last_byte_parity(self):
        for gid in ("r2-s1-b512-vs-Ha/o0-t5-k0-s1", "r2-s1-b512-vs-T1/o3-t2-k0-s0"):
            odd = hashlib.sha256(gid.encode()).digest()[-1] % 2
            self.assertEqual(half(gid), "read" if odd else "fit")

    def test_template_key(self):
        key = "a12:b34:f0"
        odd = hashlib.sha256(key.encode()).digest()[-1] % 2
        self.assertEqual(common.template_half(12, 34, 0), "read" if odd else "fit")


class AllocationTests(unittest.TestCase):
    def test_floor_and_total(self):
        a = allocate({"A": 500, "B": 10, "C": 100, "D": 30})
        self.assertEqual(sum(a.values()), 300)
        self.assertEqual(a["B"], 10)
        self.assertEqual(a["D"], 20)

    def test_capped_by_counts(self):
        self.assertEqual(allocate({"A": 5, "B": 200}), {"A": 5, "B": 200})

    def test_largest_remainder(self):
        a = allocate({"A": 400, "B": 400, "C": 15})
        self.assertEqual((a["C"], a["A"] + a["B"]), (15, 285))


class FitTests(unittest.TestCase):
    def test_auc_ties_and_order(self):
        self.assertEqual(common.auc([0, 0, 1, 1], np.array([0.1, 0.2, 0.3, 0.4])), 1.0)
        self.assertEqual(common.auc([0, 1], np.array([0.5, 0.5])), 0.5)
        self.assertIsNone(common.auc([1, 1], np.array([0.1, 0.2])))

    def test_spearman_with_ties(self):
        self.assertAlmostEqual(common.spearman([1, 2, 3], [1, 3, 2]), 0.5)
        self.assertAlmostEqual(common.spearman([1, 1, 2], [1, 1, 2]), 1.0)

    def test_fractional_logistic_recovers_slope(self):
        rng = np.random.default_rng(0)
        x = rng.normal(size=(4000, 1))
        p = common.sigmoid(0.5 + 2.0 * x[:, 0])
        y = (rng.random(4000) < p).astype(float)
        a, b = common.fit_logistic(x, y, np.ones(4000), 1e3)
        self.assertAlmostEqual(a, 0.5, delta=0.15)
        self.assertAlmostEqual(b[0], 2.0, delta=0.2)

    def test_choose_c_prefers_smaller_on_ties(self):
        x = np.zeros((40, 1))
        y = np.array([0.0, 1.0] * 20)
        c, losses, _ = common.choose_c(x, y, np.ones(40), [k // 2 for k in range(40)], np.ones(40))
        self.assertEqual(c, common.C_GRID[0])

    def test_group_folds_keep_groups_whole(self):
        groups = [k // 3 for k in range(30)]
        folds = common.group_folds(groups)
        for g in set(groups):
            self.assertEqual(len({folds[i] for i in range(30) if groups[i] == g}), 1)


class ReferenceTests(unittest.TestCase):
    """Each rewritten fit against a closed form or an independent computation."""

    def test_saturated_logistic_matches_closed_form(self):
        # One binary feature, effectively unpenalized: intercept logit(p0), slope logit(p1) - logit(p0).
        x = np.array([[0.0]] * 10 + [[1.0]] * 10)
        y = np.array([1.0] * 3 + [0.0] * 7 + [1.0] * 8 + [0.0] * 2)
        a, b = common.fit_logistic(x, y, np.ones(20), 1e12)
        logit = lambda p: np.log(p / (1 - p))
        self.assertAlmostEqual(a, logit(0.3), places=5)
        self.assertAlmostEqual(b[0], logit(0.8) - logit(0.3), places=5)

    def test_penalized_weighted_fractional_fit_is_stationary(self):
        # The penalized objective's gradient is zero at the returned fit.
        rng = np.random.default_rng(1)
        x = rng.normal(size=(300, 3))
        y = rng.choice([0.0, 0.5, 1.0], size=300)
        w = rng.random(300)
        c = 0.1
        a, b = common.fit_logistic(x, y, w, c)
        p = common.sigmoid(a + x @ b)
        self.assertAlmostEqual(float((w * (p - y)).sum()), 0.0, places=6)
        np.testing.assert_allclose(x.T @ (w * (p - y)) + b / c, 0.0, atol=1e-6)

    def test_auc_matches_pairwise_count(self):
        rng = np.random.default_rng(2)
        s = np.round(rng.normal(size=200), 1)
        y = rng.random(200) < 0.4
        pos, neg = s[y], s[~y]
        brute = np.mean([(p > n) + 0.5 * (p == n) for p in pos for n in neg])
        self.assertAlmostEqual(common.auc(y, s), brute, places=12)

    def test_spearman_matches_rank_formula(self):
        a = [3.0, 1.0, 4.0, 1.5, 5.0, 9.0]
        b = [2.0, 7.0, 1.0, 8.0, 2.5, 3.0]
        ra, rb = np.argsort(np.argsort(a)) + 1, np.argsort(np.argsort(b)) + 1
        rho = 1 - 6 * ((ra - rb) ** 2).sum() / (6 * 35)
        self.assertAlmostEqual(common.spearman(a, b), rho, places=12)

    def test_tied_spearman_matches_hand_ranks(self):
        # Average ranks by hand: a -> [1, 2.5, 2.5, 4], b -> [1, 3, 2, 4].
        ra, rb = np.array([1, 2.5, 2.5, 4]), np.array([1, 3, 2, 4])
        da, db = ra - ra.mean(), rb - rb.mean()
        rho = (da * db).sum() / np.sqrt((da ** 2).sum() * (db ** 2).sum())
        self.assertAlmostEqual(common.spearman([10, 20, 20, 30], [1, 3, 2, 4]), rho, places=12)

    def test_nearly_separable_fit_converges(self):
        # Round-3 adversarial case: standardized features, binary targets, C = 1000.
        rng = np.random.default_rng(3)
        x = rng.normal(size=(60, 5))
        y = (x[:, 0] + 0.05 * rng.normal(size=60) > 0).astype(float)
        w = rng.random(60) + 0.5
        a, b = common.fit_logistic(x, y, w, 1000.0)
        p = common.sigmoid(a + x @ b)
        grad = np.concatenate([[(w * (p - y)).sum()], x.T @ (w * (p - y)) + b / 1000.0])
        self.assertLess(np.abs(grad).max(), 1e-5)

    def test_sigmoid_is_overflow_safe(self):
        with np.errstate(over="raise"):
            np.testing.assert_allclose(common.sigmoid(np.array([-1000.0, 0.0, 1000.0])), [0.0, 0.5, 1.0])

    def test_bootstrap_matches_manual_resampling(self):
        groups = ["g1", "g1", "g2", "g3", "g3", "g3"]
        vals = np.array([1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        lo, hi, n = common.bootstrap_groups(groups, lambda idx: float(vals[idx].mean()), 20261032)
        rng = np.random.Generator(np.random.PCG64(20261032))
        members = {"g1": [0, 1], "g2": [2], "g3": [3, 4, 5]}
        keys = ["g1", "g2", "g3"]
        manual = []
        for _ in range(common.BOOTSTRAP):
            idx = np.concatenate([members[keys[i]] for i in rng.integers(0, 3, 3)])
            manual.append(vals[idx].mean())
        self.assertEqual(n, common.BOOTSTRAP)
        self.assertAlmostEqual(lo, float(np.percentile(manual, 2.5)), places=12)
        self.assertAlmostEqual(hi, float(np.percentile(manual, 97.5)), places=12)


class A2Tests(unittest.TestCase):
    def test_paired_gap_and_se(self):
        gap, se = paired([1, 1, 0, 1], [0, 1, 0, 0])
        self.assertAlmostEqual(gap, 0.5)
        self.assertAlmostEqual(se, np.std([1, 0, 0, 1], ddof=1) / 2)

    def test_signal_share_clamped(self):
        self.assertEqual(signal_share(np.array([0.0, 0.1, -0.1]), np.array([1.0, 1.0, 1.0])), 0.0)
        self.assertEqual(signal_share(np.array([0.0, 0.1, -0.1]), np.zeros(3)), 1.0)


if __name__ == "__main__":
    unittest.main()
