"""E8 Stage 0b tool tests on synthetic capture records."""
import json
import random
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import stage0b as s0  # noqa: E402
import target_ref  # noqa: E402  (stage0b put tools/ex1 on the path)


def record(episode, rng, reached=True, fail=(), drop_qref=False, game=None):
    chain, update, seed, slot = "w5/0b", episode // 10, 1000 + episode, episode % 10
    out = {"schema": s0.CAPTURE_SCHEMA, "episode": episode, "game": game or f"g{episode}", "chain": chain,
           "update": update, "seed": seed, "slot": slot, "k": s0.placement_index(chain, update, seed, slot),
           "reached": reached}
    if not reached:
        return out
    k = 4
    out["policy"] = {"logit_bits": [target_ref.f32bits(rng.uniform(-2, 2)) for _ in range(k)]}
    true_value = [rng.uniform(0.2, 0.8) for _ in range(k)]
    cands = []
    for budget in s0.BUDGETS:
        for search_seed in s0.SEEDS:
            n = [rng.randint(0, budget // 400) + (8 if a < 3 else 0) for a in range(k)]
            wins = [int(n[a] * true_value[a]) for a in range(k)]
            cands.append({"budget": budget, "search_seed": search_seed, "cpu_s": budget / 1500 + rng.random(),
                          "status": "failed" if (budget, search_seed) in fail else "ok",
                          "n": n, "wins": wins, "draws": [0] * k})
    out["candidates"] = cands
    actions = list(range(1 if drop_qref else 0, k))
    out["qref"] = {"rollouts": 128, "actions": actions, "n": [128] * len(actions),
                   "wins": [int(128 * true_value[a]) for a in actions], "draws": [0] * len(actions)}
    return out


def records(count=340, seed=3):
    rng = random.Random(seed)
    return [record(i, rng, reached=(i % 9 != 4)) for i in range(count)]


class PlacementTests(unittest.TestCase):
    def test_index_range_and_uniformity(self):
        ks = [s0.placement_index("w5/chain1", u, 7, j) for u in range(200) for j in range(10)]
        self.assertTrue(all(1 <= k <= 128 for k in ks))
        self.assertAlmostEqual(sum(ks) / len(ks), 64.5, delta=3.0)

    def test_fixtures_are_current(self):
        stored = json.loads((HERE / "placement_fixtures_v1.json").read_text(encoding="utf-8"))
        self.assertEqual(stored, s0.fixtures())

    def test_hash_matches_ex1_convention(self):
        self.assertEqual(s0.hash_u64("ex1/placement", "r", 299, 9007199254740993, 7),
                         target_ref.hash_u64("ex1/placement", "r", 299, 9007199254740993, 7))


class GainTests(unittest.TestCase):
    def test_gain_by_hand(self):
        rng = random.Random(5)
        rec = record(0, rng)
        out = s0.state_gains(rec)
        p = s0.policy(rec)
        ref = {a: w / 128 for a, w in zip(rec["qref"]["actions"], rec["qref"]["wins"])}
        for budget in s0.BUDGETS:
            gains = []
            for c in rec["candidates"]:
                if c["budget"] != budget:
                    continue
                t = target_ref.target(rec["policy"]["logit_bits"], c["n"], c["wins"], c["draws"])
                q = p if isinstance(t, tuple) else t["q"]
                gains.append(sum((q[a] - p[a]) * ref[a] for a in range(len(p))))
            self.assertAlmostEqual(out[budget]["gain"], sum(gains) / 2, places=12)

    def test_failed_search_gives_zero_gain(self):
        rng = random.Random(5)
        rec = record(0, rng, fail={(16000, 0), (16000, 1)})
        out = s0.state_gains(rec)
        self.assertEqual(out[16000]["gain"], 0.0)
        self.assertEqual(out[16000]["outcomes"], ["failed", "failed"])
        self.assertEqual(len(out[16000]["cpu_s"]), 2)  # failed attempts still cost

    def test_qref_must_cover_supported_actions(self):
        with self.assertRaises(s0.StageError):
            s0.state_gains(record(0, random.Random(5), drop_qref=True))

    def test_engine_q_must_match_mirror(self):
        rec = record(0, random.Random(5))
        c = rec["candidates"][0]
        t = target_ref.target(rec["policy"]["logit_bits"], c["n"], c["wins"], c["draws"])
        if not isinstance(t, tuple):
            c["q_bits"] = [target_ref.bits64(x) for x in t["q"]]
            s0.state_gains(rec)
            c["q_bits"][0] ^= 1
            with self.assertRaises(s0.StageError):
                s0.state_gains(rec)


class PanelTests(unittest.TestCase):
    def test_first_300_reached_in_episode_order(self):
        recs = records()
        panel = s0.draw_panel(recs, 300)
        self.assertEqual(len(panel), 300)
        self.assertTrue(all(r["reached"] for r in panel))
        reached = [r for r in recs if r["reached"]]
        self.assertEqual([r["episode"] for r in panel], [r["episode"] for r in reached[:300]])

    def test_refusals(self):
        recs = records()
        with self.assertRaises(s0.StageError):
            s0.draw_panel(recs[:100], 300)
        gap = recs[:5] + recs[6:]
        with self.assertRaises(s0.StageError):
            s0.draw_panel(gap, 300)
        bad_k = [dict(r) for r in recs]
        bad_k[3]["k"] = bad_k[3]["k"] % 128 + 1
        with self.assertRaises(s0.StageError):
            s0.draw_panel(bad_k, 300)
        dup = [dict(r) for r in recs]
        dup[2]["game"] = dup[1]["game"]
        with self.assertRaises(s0.StageError):
            s0.draw_panel(dup, 300)


def summary(g16, lo16, c16, g64, lo64, c64):
    return {16000: {"G": g16, "lower_10pct": lo16, "c": c16}, 64000: {"G": g64, "lower_10pct": lo64, "c": c64}}


class RuleTests(unittest.TestCase):
    def test_cheaper_budget_wins_on_ratio(self):
        self.assertEqual(s0.decide(summary(0.010, 0.002, 10.9, 0.016, 0.004, 43.8))["B"], 16000)

    def test_half_gain_floor(self):
        out = summary(0.007, 0.002, 10.9, 0.016, 0.004, 43.8)
        self.assertEqual(s0.decide(out)["B"], 64000)
        self.assertFalse(out[16000]["clause_half_gain_floor"])

    def test_lower_bound_clause(self):
        self.assertEqual(s0.decide(summary(0.010, -0.001, 10.9, 0.016, 0.004, 43.8))["B"], 64000)

    def test_four_figure_tie_goes_to_larger_budget(self):
        # ratios 1.00004e-3 and 1.00001e-3 agree to 4 significant figures
        self.assertEqual(s0.decide(summary(0.0200008, 0.001, 20.0, 0.0400004, 0.001, 40.0))["B"], 64000)
        self.assertEqual(s0.decide(summary(0.0202, 0.001, 20.0, 0.0400, 0.001, 40.0))["B"], 16000)

    def test_stop(self):
        out = s0.decide(summary(0.01, -0.001, 10.9, 0.016, -0.002, 43.8))
        self.assertEqual(out["decision"], "stop")
        self.assertIsNone(out["B"])


class EndToEndTests(unittest.TestCase):
    def test_select_and_gate_are_deterministic(self):
        recs = records()
        a = s0.select(recs, replicates=400)
        b = s0.select(recs, replicates=400)
        self.assertEqual(a, b)
        self.assertEqual(a["boot_seed"], s0.tag_seed("w5/0b/boot"))
        self.assertIn(a["decision"], ("budget", "stop"))
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "capture-000.jsonl"
            path.write_text("".join(json.dumps(r) + "\n" for r in reversed(recs)))
            self.assertEqual(s0.load_records(Path(tmp)), recs)
        g = s0.gate(recs, 16000, "X8", replicates=400)
        self.assertEqual(g["panel_size"], 100)
        self.assertIsInstance(g["pass"], bool)


if __name__ == "__main__":
    unittest.main()
