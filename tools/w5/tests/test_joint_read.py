"""E13 joint-bootstrap reader tests. Spellbench parity and the end-to-end read on Spellbench's published
pauper-ladder-v1 snapshot run when SPELLBENCH_ROOT points at a Spellbench checkout (CI pins one)."""
import hashlib
import json
import os
import random
import tempfile
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import joint_read as jr  # noqa: E402

SPELLBENCH = os.environ.get("SPELLBENCH_ROOT")
if SPELLBENCH:
    sys.path.insert(0, str(Path(SPELLBENCH) / "python"))


def synthetic(seed=7, pairs=24):
    """Rungs r0 (anchor) < r1 < r2; entrants a1..a6 and b1..b6 each play the three rungs."""
    rng = random.Random(seed)
    strength = {"r0": 0.0, "r1": 0.6, "r2": 1.2}
    strength.update({f"a{i}": 0.9 + 0.05 * i for i in range(1, 7)})
    strength.update({f"b{i}": 0.7 + 0.04 * i for i in range(1, 7)})
    edges = [("r0", "r1"), ("r1", "r2"), ("r0", "r2")]
    edges += [(e, r) for e in strength if e[0] in "ab" for r in ("r0", "r1", "r2")]
    matchups = []
    for x, y in edges:
        a_id, b_id = sorted((x, y))
        p = 1 / (1 + pow(2.718281828, strength[b_id] - strength[a_id]))
        values = tuple(sum(2 if rng.random() < p else 0 for _ in range(2)) for _ in range(pairs))
        matchups.append(jr.Matchup(a_id, b_id, values))
    matchups.sort(key=lambda m: (m.a_id, m.b_id))
    ids = {name: name for name in strength}
    return matchups, ids


def spec(read="P1", under=()):
    return {"read": read, "arm_a": "X8", "arm_b": "C", "appended_rungs": [],
            "pairs": [{"pair": i, "a": f"a{i}", "b": f"b{i}", "under_budget": i in under} for i in range(1, 7)]}


class StatisticsTests(unittest.TestCase):
    def test_seed_and_quantiles_are_frozen(self):
        self.assertEqual(jr.read_seed("P1"),
                         int.from_bytes(hashlib.sha256(b"w5/read/P1").digest()[:8], "little"))
        self.assertNotEqual(jr.read_seed("P1"), jr.read_seed("P2"))
        self.assertEqual(jr.quantile_indices(2000), (24, 1975))
        lo, hi = jr.quantile_indices(1999)
        self.assertEqual(lo, 1998 - hi)

    def test_t_quantiles(self):
        self.assertAlmostEqual(jr.t_quantile(0.975, 5), 2.570582, places=5)
        self.assertAlmostEqual(jr.t_quantile(0.975, 2), 4.302653, places=5)
        self.assertAlmostEqual(jr.t_quantile(0.995, 3), 5.840909, places=5)

    def test_labels_in_order(self):
        self.assertEqual(jr.label(10, 90, [5, 5, 5, 5, 5, -1], "X8", "C"), "X8 stronger")
        self.assertEqual(jr.label(10, 40, [5, 5, 5, 5, -1, -1], "X8", "C"), "no worthwhile difference")
        self.assertEqual(jr.label(10, 90, [5, 5, 5, 5, -1, -1], "X8", "C"), "not resolved")
        self.assertEqual(jr.label(-90, -10, [-5, -5, -5, 1], "X8", "C"), "C stronger")
        self.assertEqual(jr.label(-60, 20, [1, 1, 1], "X8", "C"), "not resolved")
        with self.assertRaises(jr.ReadError):
            jr.label(1, 2, [1, 1, 1, 1, 1], "X8", "C")

    def test_spec_checks(self):
        bad = spec()
        bad["pairs"][1]["a"] = "a1"
        with self.assertRaises(jr.ReadError):
            jr.check_spec(bad)
        with self.assertRaises(jr.ReadError):  # an appended rung cannot also be an entrant
            jr.check_spec(dict(spec(), appended_rungs=["a1"]))


class ReadTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.matchups, cls.ids = synthetic()
        cls.result = jr.read(cls.matchups, "r0", cls.ids, spec(under=(6,)), replicates=300)

    def test_estimate_is_mean_of_point_differences(self):
        point = jr.point_fit(self.matchups, "r0")
        diffs = [(point[f"a{i}"] - point[f"b{i}"]) * jr.ELO for i in range(1, 7)]
        self.assertAlmostEqual(self.result["estimate"], sum(diffs) / 6, places=9)
        lo, hi = self.result["interval_975"]
        self.assertLess(lo, self.result["estimate"])
        self.assertGreater(hi, self.result["estimate"])

    def test_deterministic_and_seed_dependent(self):
        again = jr.read(self.matchups, "r0", self.ids, spec(under=(6,)), replicates=300)
        self.assertEqual(again["interval_975"], self.result["interval_975"])
        other = jr.read(self.matchups, "r0", self.ids, spec("P2", under=(6,)), replicates=300)
        self.assertNotEqual(other["interval_975"], self.result["interval_975"])

    def test_joint_refit_matches_a_direct_replicate(self):
        """Replicate 1 by hand: one resample of every matchup, one joint refit, the estimand in that fit."""
        rng = jr.SplitMix64(jr.read_seed("P1"))
        pairs = []
        for m in self.matchups:
            n = len(m.values)
            s = sum(m.values[jr.unbiased_index(n, rng.next_u64)] for _ in range(n))
            pairs.append(jr.ScorePair(m.a_id, m.b_id, s + 1.0, 4.0 * n - s + 1.0))
        fit = jr.fit_score_pairs(pairs, "r0")
        direct = sum(fit[f"a{i}"] - fit[f"b{i}"] for i in range(1, 7)) / 6 * jr.ELO
        boot = jr.bootstrap(self.matchups, "r0", jr.read_seed("P1"), 1,
                            {"x": lambda f: sum(f[f"a{i}"] - f[f"b{i}"] for i in range(1, 7)) / 6 * jr.ELO})
        self.assertEqual(boot["samples"]["x"], [direct])

    def test_sensitivity_and_per_rung(self):
        sens = self.result["sensitivity_without_under_budget"]
        self.assertEqual(sens["pairs"], 5)
        self.assertIsNone(sens["label"])
        with_rung = jr.read(self.matchups, "r0", self.ids, dict(spec(), appended_rungs=["r2"]), replicates=200)
        self.assertEqual(with_rung["per_rung_sensitivity"][0]["without"], "r2")
        self.assertNotIn("sensitivity_without_under_budget", with_rung)
        self.assertIsNotNone(self.result["secondary_t_interval"])

    def test_degenerate_bootstrap_fails_closed(self):
        real, calls = jr.fit_score_pairs, {"n": 0}

        def flaky(pairs, anchor):
            calls["n"] += 1
            if calls["n"] % 3:  # two of every three refits fail
                raise jr.FitError("degenerate")
            return real(pairs, anchor)
        jr.fit_score_pairs = flaky
        try:
            with self.assertRaises(jr.ReadError):
                jr.bootstrap(self.matchups, "r0", 1, 30, {"x": lambda f: 0.0})
        finally:
            jr.fit_score_pairs = real
        with self.assertRaises(jr.FitError):
            jr.fit_score_pairs([jr.ScorePair("a", "b", 8.0, 0.0)], "a")


@unittest.skipUnless(SPELLBENCH, "SPELLBENCH_ROOT not set")
class SpellbenchTests(unittest.TestCase):
    def test_fit_is_spellbench_bit_for_bit(self):
        from spellbench.arena import ratings
        matchups, _ = synthetic(seed=11)
        pairs = [jr.ScorePair(m.a_id, m.b_id, sum(m.values) + 1.0, 4.0 * len(m.values) - sum(m.values) + 1.0)
                 for m in matchups]
        ours = jr.fit_score_pairs(pairs, "r0")
        theirs = ratings._fit_score_pairs([ratings._ScorePair(p.a_id, p.b_id, p.a_score, p.b_score)
                                           for p in pairs], "r0")
        self.assertEqual(ours, dict(theirs.ratings_log_units))
        rng_a, rng_b = jr.SplitMix64(123), ratings._SplitMix64(123)
        self.assertEqual([rng_a.next_u64() for _ in range(5)], [rng_b.next_u64() for _ in range(5)])

    def test_published_snapshot_end_to_end(self):
        directory = Path(SPELLBENCH) / "benchmarks" / "pauper-ladder-v1" / "snapshots" / "2026-11-01"
        matchups, anchor, ids, provenance = jr.load_directory(directory, verify_published=True)
        self.assertGreater(provenance["published_intervals_reproduced"], 0)
        demo = {"read": "P1", "arm_a": "upper", "arm_b": "lower", "appended_rungs": [],
                "pairs": [{"pair": 1, "a": "g115", "b": "gorge-ar8"},
                          {"pair": 2, "a": "oh2-r2s1-b512", "b": "c12"},
                          {"pair": 3, "a": "oh2-r2s1-b512-argmax", "b": "heuristic"}]}
        result = jr.read(matchups, anchor, ids, demo)
        lo, hi = result["interval_975"]
        self.assertLess(lo, result["estimate"])
        self.assertLess(result["estimate"], hi)
        self.assertEqual(result["failed_replicates"], 0)
        gate = Path(tempfile.mkdtemp()) / "w5-gate.json"
        gate.write_text(json.dumps({"source": str(directory), "gate": "pass-8"}))
        jr.admit_snapshot(directory, gate, demo, provenance)
        gate.write_text(json.dumps({"source": str(directory), "gate": "stop"}))
        with self.assertRaises(jr.ReadError):
            jr.admit_snapshot(directory, gate, demo, provenance)
        out = os.environ.get("W5_READ_RECEIPT")
        if out:
            Path(out).write_text(json.dumps(dict(result, provenance=provenance), indent=2) + "\n")


if __name__ == "__main__":
    unittest.main()
