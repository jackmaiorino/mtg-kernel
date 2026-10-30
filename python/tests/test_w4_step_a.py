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
