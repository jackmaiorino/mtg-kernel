"""V6 value-leaf panel read tests on synthetic capture records."""
import json
import random
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(Path(__file__).resolve().parent))
import stage0b as s0  # noqa: E402
import target_ref  # noqa: E402
import value_leaf_2a as vl  # noqa: E402
from test_stage0b import PARENT, records  # noqa: E402

B = 16000
REPS = 400


def vl_record(rec, host_class="linux-zen5-9950x", cpu=0.5, memo="on", tag=""):
    base = {c["search_seed"]: c for c in rec["candidates"] if c["budget"] == B}
    configs = []
    for n_sims in (256, 1024):
        for seed in s0.SEEDS:
            c = base[seed]
            configs.append({"horizon": "h0", "n_sims": n_sims, "search_seed": seed,
                            "seed_root": f"w5/2a/{rec['episode']}/{n_sims}/{seed}", "status": c["status"],
                            "cpu_s": cpu * n_sims / 256, "primary_sha256": f"h{rec['episode']}{n_sims}{seed}{tag}",
                            "n": c["n"], "value_units": [w * vl.UNIT for w in c["wins"]], "draws": c["draws"]})
    return {"schema": vl.VL_SCHEMA, "episode": rec["episode"], "game": rec["game"], "host_class": host_class,
            "binary_sha256": "x" * 64, "configs": configs,
            "comparator": [{"budget": B, "search_seed": s, "cpu_s": 12.0, "key_memo": memo} for s in s0.SEEDS],
            "qref_extra": {"rollouts": 128, "seed_root": f"w5/2a/qref/{rec['episode']}", "actions": [], "n": [],
                           "wins": [], "draws": []}}


def panel():
    return s0.draw_panel(records(), s0.PANEL, s0.PANEL_NAMESPACE, PARENT)


class TargetTests(unittest.TestCase):
    def test_vl_target_equals_ex1_target_on_equivalent_units(self):
        rng = random.Random(5)
        for _ in range(200):
            k = rng.randint(2, 6)
            bits = [target_ref.f32bits(rng.uniform(-3, 3)) for _ in range(k)]
            n = [rng.randint(0, 40) for _ in range(k)]
            wins = [rng.randint(0, x) for x in n]
            draws = [rng.randint(0, x - w) for x, w in zip(n, wins)]
            units = [(2 * w + d) * (vl.UNIT // 2) for w, d in zip(wins, draws)]
            a, b = target_ref.target(bits, n, wins, draws), vl.vl_target(bits, n, units)
            if isinstance(a, tuple):
                self.assertEqual(a, b)
            else:
                self.assertEqual([target_ref.bits64(x) for x in a["q"]], [target_ref.bits64(x) for x in b["q"]])


class RuleTests(unittest.TestCase):
    def setUp(self):
        self.panel = panel()
        self.vl = {r["episode"]: vl_record(r) for r in self.panel}

    def test_halves_are_fixed_disjoint_and_sized(self):
        a, b = vl.halves(self.panel)
        self.assertEqual(len(a), 150)
        self.assertEqual(len(b), 150)
        self.assertFalse({r["episode"] for r in a} & {r["episode"] for r in b})
        self.assertEqual(vl.halves(list(reversed(self.panel))), (a, b))

    def test_selection_reads_only_the_selection_half(self):
        sel, conf = vl.halves(self.panel)
        only = {r["episode"]: self.vl[r["episode"]] for r in sel}
        self.assertEqual(vl.select(self.panel, only, B, REPS), vl.select(self.panel, self.vl, B, REPS))
        with self.assertRaises(s0.StageError):
            vl.confirm(self.panel, only, B, vl.select(self.panel, only, B, REPS))

    def test_cheap_equal_gain_value_leaf_passes_in_the_node_class(self):
        lock = vl.select(self.panel, self.vl, B, REPS)
        self.assertEqual(lock["selected"], "h0-n256")
        out = vl.confirm(self.panel, self.vl, B, lock, replicates=REPS)
        self.assertEqual(out["verdict"], "pass", out["clauses"])
        self.assertTrue(out["binding"])
        self.assertGreater(out["interval_efficiency_ratio"]["p10"], 1)

    def test_other_class_is_a_development_read(self):
        dev = {e: vl_record(r, host_class="dev-windows-haleyspc") for e, r in
               ((r["episode"], r) for r in self.panel)}
        out = vl.confirm(self.panel, dev, B, vl.select(self.panel, dev, B, REPS), replicates=REPS)
        self.assertFalse(out["binding"])
        self.assertIn("development", out["label"])

    def test_expensive_value_leaf_fails_the_efficiency_clauses(self):
        dear = {r["episode"]: vl_record(r, cpu=8.0) for r in self.panel}
        out = vl.confirm(self.panel, dear, B, vl.select(self.panel, dear, B, REPS), replicates=REPS)
        self.assertEqual(out["verdict"], "fail")
        self.assertFalse(out["clauses"]["efficiency_point"])

    def test_nonpositive_comparator_is_inconclusive(self):
        lock = vl.select(self.panel, self.vl, B, REPS)
        real = s0.state_gains

        def negative(record, budgets=s0.BUDGETS):
            out = real(record, budgets)
            for b in budgets:
                out[b]["gain"] = -abs(out[b]["gain"]) - 1e-3
            return out
        with mock.patch.object(s0, "state_gains", negative):
            out = vl.confirm(self.panel, self.vl, B, lock, replicates=REPS)
        self.assertEqual(out["verdict"], "inconclusive")

    def test_lock_must_match(self):
        lock = vl.select(self.panel, self.vl, B, REPS)
        lock["lock_sha256"] = "0" * 64
        with self.assertRaises(s0.StageError):
            vl.confirm(self.panel, self.vl, B, lock, replicates=REPS)

    def test_refusals(self):
        rec = self.panel[0]
        bad = vl_record(rec, memo="off")
        with self.assertRaises(s0.StageError):
            vl.validate_vl(bad, rec, B)
        bad = vl_record(rec)
        a = rec["qref"]["actions"][0]
        bad["qref_extra"] = {"rollouts": 128, "seed_root": "x", "actions": [a], "n": [128], "wins": [1], "draws": [0]}
        with self.assertRaises(s0.StageError):
            vl.validate_vl(bad, rec, B)
        bad = vl_record(rec)
        bad["configs"] = bad["configs"][1:]
        with self.assertRaises(s0.StageError):
            vl.validate_vl(bad, rec, B)

    def test_crossclass(self):
        other = {e: vl_record(r, tag="") for e, r in ((r["episode"], r) for r in self.panel[:5])}
        self.assertTrue(vl.crossclass(self.vl, other)["all_equal"])
        other = {e: vl_record(r, tag="z") for e, r in ((r["episode"], r) for r in self.panel[:5])}
        res = vl.crossclass(self.vl, other)
        self.assertFalse(res["all_equal"])
        self.assertEqual(res["compared"], 20)

    def test_cli_round_trip(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "cap").mkdir()
            (d / "vl").mkdir()
            (d / "cap" / "capture-0.jsonl").write_text(
                "".join(json.dumps(r) + "\n" for r in records()), encoding="utf-8")
            (d / "vl" / "vl-0.jsonl").write_text(
                "".join(json.dumps(v) + "\n" for v in self.vl.values()), encoding="utf-8")
            common = ["--captures", str(d / "cap"), "--vl", str(d / "vl"), "--parent-sha256", PARENT,
                      "--budget", str(B), "--lock", str(d / "lock.json")]
            with mock.patch.object(s0, "REPLICATES", REPS):
                self.assertEqual(vl.main(["select", *common]), 0)
                self.assertEqual(vl.main(["select", *common]), 1)  # already locked
                self.assertEqual(vl.main(["confirm", *common, "--out", str(d / "out.json")]), 0)
            self.assertIn(json.loads((d / "out.json").read_text())["verdict"], ("pass", "fail", "inconclusive"))


if __name__ == "__main__":
    unittest.main()
