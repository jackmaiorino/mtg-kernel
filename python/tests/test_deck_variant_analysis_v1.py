import math
import sys
import unittest
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "python" / "tools"))

import deck_variant_analysis_v1 as dva  # noqa: E402
import paired_sideboard_estimator_v1 as mirror  # noqa: E402


def rows_for(pilots, decks, blocks, win, fail=()):
    """Synthetic rows: win(pilot, deck, opponent, block, seat, start) -> 0/1; ``fail`` lists
    (pilot, deck, opponent, block, seat, start) cases with a scientific failure."""
    out = []
    for pilot in pilots:
        for opponent in range(9):
            for block in range(blocks):
                for seat in (0, 1):
                    for start in (0, 1):
                        for deck in decks:
                            key = (pilot, deck, opponent, block, seat, start)
                            failed = key in fail
                            out.append({"pilot": pilot, "deck": deck, "opponent": opponent, "block": block,
                                        "seat": seat, "starting_player": start, "complete": not failed,
                                        "win": None if failed else win(*key)})
    return out


class CrossCheck(unittest.TestCase):
    def test_python_mirror_matches_the_rust_bounds_at_b0_alpha_levels(self):
        # Same fixture and expected values as the Rust test
        # pool_swaps::tests::paired_bootstrap_matches_the_python_mirror_at_b0_alpha_levels.
        deltas = [1, 0, -1, 1, 1, 0, 0, -1, 1, 0, 1, 0, 0, 1, -1, 0, 1, 1, 0, 0]
        self.assertEqual(mirror.paired_bootstrap_ci(deltas, 1000, 0x5EED, "one_sided_lower", 0.05 / 3)["lower"], -0.1)
        self.assertEqual(mirror.paired_bootstrap_ci(deltas, 1000, 0x5EED, "one_sided_lower")["lower"], 0.0)
        two = mirror.paired_bootstrap_ci(deltas, 1000, 0x5EED, "two_sided", 0.05)
        self.assertEqual((two["lower"], two["upper"]), (-0.05, 0.55))

    def test_single_stratum_analyzer_agrees_with_the_mirror_within_monte_carlo_error(self):
        rng = np.random.default_rng(3)
        deltas = rng.choice([-1, 0, 0, 1, 1], size=400)
        series = [deltas.astype(float)]
        opponents = np.zeros(400, dtype=int)
        samples = dva.bootstrap_field(series, opponents, np.zeros(400, dtype=int), 20000, 11)[:, 0]
        ours = dva.lower_bound(samples, 0.05)
        theirs = mirror.paired_bootstrap_ci([int(d) for d in deltas], 4000, 5, "one_sided_lower")["lower"]
        self.assertAlmostEqual(ours, theirs, delta=0.01)


class Estimates(unittest.TestCase):
    def test_field_mean_weights_opponents_equally(self):
        delta = np.array([1.0] * 4 + [0.0] * 32 + [1.0] * 8)
        opponents = np.array([0] * 4 + [o for o in range(1, 9) for _ in range(4)] + [8] * 8)
        # Opponent 0 has mean 1; opponent 8 has 12 cases with mean 2/3; the rest 0.
        self.assertAlmostEqual(dva.field_mean(delta, opponents), (1 + 2 / 3) / 9)

    def test_common_mask_and_worst_case_imputation(self):
        fail = {("g115", 1, 0, 0, 0, 0)}
        rows = rows_for(["g115"], [0, 1], 2, lambda p, d, o, b, s, t: int(d == 1), fail)
        stage = dva.Stage(rows)
        self.assertAlmostEqual(stage.masked_share("g115"), 1 / 72)
        self.assertTrue(np.isnan(stage.deltas("g115", 1, 0)[0]))
        self.assertEqual(stage.deltas("g115", 1, 0, worst_case=True)[0], -1.0)

    def test_ranking_breaks_ties_by_global_index(self):
        self.assertEqual(dva.rank({5: 0.1, 2: 0.1, 9: 0.3}, [5, 2, 9]), [9, 2, 5])

    def test_hoeffding_rule_counts_masked_cases_and_rounds_up_to_blocks(self):
        rows = rows_for(["g115", "a48"], [0, 1], 64, lambda p, d, o, b, s, t: int(d == 1 and (b + s) % 2 == 0))
        stage = dva.Stage(rows)
        rule = dva.hoeffding_v(stage, [1], 0, ["g115", "a48"], 2304)
        self.assertAlmostEqual(rule["padding"], math.sqrt(math.log(2 / 0.05) / 4608))
        self.assertAlmostEqual(rule["bounds"]["1/g115"], 0.5 + rule["padding"])
        self.assertEqual(rule["planned_n4"] % 36, 0)
        self.assertGreaterEqual(rule["planned_n4"], 29070 * rule["v"])
        full = dva.hoeffding_v(stage, [1], 0, ["g115", "a48"], 4608)
        self.assertGreater(full["bounds"]["1/g115"], 0.75, "unscheduled cases count as discordant")

    def test_affordable_n4_rounds_down_and_keeps_the_reserve(self):
        self.assertEqual(dva.affordable_n4(150 * 3600, 100 * 3600, 15 * 3600, 1.0), 126000)
        self.assertEqual(dva.affordable_n4(10, 20, 5, 1.0), 0)

    def test_kendall_tau_b(self):
        self.assertEqual(dva.kendall_tau_b([1, 2, 3], [1, 2, 3]), 1.0)
        self.assertEqual(dva.kendall_tau_b([1, 2, 3], [3, 2, 1]), -1.0)


class Labels(unittest.TestCase):
    def test_gain_under_both_pilots_and_one_pilot_labels(self):
        def win(p, d, o, b, s, t):
            base = (o + b + s + t) % 2
            if d == 1:  # strong gain everywhere
                return 1
            if d == 2 and p == "g115":  # gain for g115 only
                return 1
            return base
        rows = rows_for(["g115", "a48", "H"], [0, 1, 2, 3], 16, win)
        labels = dva.m4_labels(dva.Stage(rows), [1, 2, 3], None, 0, 4000, 1)
        self.assertEqual(labels["1"]["label"], "gain under both principal pilots")
        self.assertTrue(labels["1"]["material"])
        self.assertEqual(labels["2"]["label"], "one-pilot gain (g115): pilot-specific")
        self.assertEqual(labels["3"]["label"], "no demonstrated gain")

    def test_h_harm_is_its_own_label(self):
        def win(p, d, o, b, s, t):
            base = (o + b + s + t) % 2
            if d == 1:
                return 0 if p == "H" else 1
            return base
        rows = rows_for(["g115", "a48", "H"], [0, 1], 16, win)
        labels = dva.m4_labels(dva.Stage(rows), [1], None, 0, 4000, 1)
        self.assertEqual(labels["1"]["label"], "principal-pilot gain with H harm")
        self.assertEqual(labels["1"]["harm_flags"], ["H"])

    def test_bootstrap_is_deterministic_for_a_seed(self):
        rows = rows_for(["g115", "a48", "H"], [0, 1], 4, lambda p, d, o, b, s, t: (o * d + b + s) % 2)
        a = dva.m4_labels(dva.Stage(rows), [1], None, 0, 1000, 9)
        b = dva.m4_labels(dva.Stage(rows), [1], None, 0, 1000, 9)
        self.assertEqual(a, b)


if __name__ == "__main__":
    unittest.main()
