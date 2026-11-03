"""V6 value-leaf panel read tests on synthetic capture records."""
import copy
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
GRID = (256, 1024)
HOR = ("h0", "t0")
REPS = 400
LIMIT = 512 << 20


def vl_record(rec, host_class="linux-zen5-9950x", cpu=0.5, memo="on", tag="", binary="x" * 64):
    base = {c["search_seed"]: c for c in rec["candidates"] if c["budget"] == B}
    configs = []
    for horizon in HOR:
        for n_sims in GRID:
            for seed in s0.SEEDS:
                c = base[seed]
                configs.append({"horizon": horizon, "n_sims": n_sims, "search_seed": seed,
                                "seed_root": f"w5/2a/{rec['episode']}/{horizon}/{n_sims}/{seed}",
                                "status": c["status"], "cpu_s": cpu * n_sims / 256,
                                "primary_sha256": f"h{rec['episode']}{horizon}{n_sims}{seed}{tag}",
                                "n": c["n"], "value_units": [w * vl.UNIT for w in c["wins"]], "draws": c["draws"]})
    return {"schema": vl.VL_SCHEMA, "episode": rec["episode"], "game": rec["game"], "host_class": host_class,
            "binary_sha256": binary, "key_memo": "on", "key_memo_limit_bytes": LIMIT, "configs": configs,
            "comparator": [{"budget": B, "search_seed": s, "cpu_s": 12.0, "key_memo": memo,
                            "key_memo_limit_bytes": LIMIT} for s in s0.SEEDS],
            "qref_extra": {"rollouts": 128, "seed_root": f"w5/2a/qref/{rec['episode']}", "actions": [], "n": [],
                           "wins": [], "draws": []}}


def panel_of(recs):
    return vl.panel_identities(recs, source_sha256=PARENT)


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
        self.records = records()
        self.panel = panel_of(self.records)
        self.vl = {r["episode"]: vl_record(r) for r in self.panel}

    def lock(self, vlr=None):
        return vl.select(self.records, vlr or self.vl, PARENT, B, GRID, HOR, REPS)

    def test_identity_panel_equals_stage0b_panel(self):
        full = s0.draw_panel(self.records, s0.PANEL, s0.PANEL_NAMESPACE, PARENT)
        self.assertEqual([r["episode"] for r in full], [r["episode"] for r in self.panel])

    def test_halves_are_fixed_disjoint_and_sized(self):
        a, b = vl.halves(self.panel)
        self.assertEqual((len(a), len(b)), (150, 150))
        self.assertFalse({r["episode"] for r in a} & {r["episode"] for r in b})
        self.assertEqual(vl.halves(list(reversed(self.panel))), (a, b))

    def test_selection_never_reads_confirmation_outcomes(self):
        _, conf = vl.halves(self.panel)
        conf_ids = {r["episode"] for r in conf}
        poisoned = copy.deepcopy(self.records)
        for r in poisoned:
            if r["episode"] in conf_ids:
                for key in ("policy", "candidates", "qref"):
                    r.pop(key, None)
        only = {e: v for e, v in self.vl.items() if e not in conf_ids}
        self.assertEqual(vl.select(poisoned, only, PARENT, B, GRID, HOR, REPS), self.lock())
        with self.assertRaises(s0.StageError):
            vl.confirm(self.records, only, PARENT, B, self.lock(), replicates=REPS)

    def test_cheap_equal_gain_value_leaf_passes_in_the_node_class(self):
        lock = self.lock()
        self.assertEqual(lock["selected"], "h0-n256")
        out = vl.confirm(self.records, self.vl, PARENT, B, lock, replicates=REPS)
        self.assertEqual(out["verdict"], "pass", out["clauses"])
        self.assertTrue(out["binding"])
        self.assertGreater(out["interval_efficiency_ratio"]["p10"], 1)
        self.assertEqual(set(out["s4a_both_budgets_0b_cost_disclosure"]), {"16000", "64000"})

    def test_development_class_or_mixed_binaries_are_not_binding(self):
        dev = {r["episode"]: vl_record(r, host_class="dev-windows-haleyspc") for r in self.panel}
        out = vl.confirm(self.records, dev, PARENT, B, self.lock(dev), replicates=REPS)
        self.assertFalse(out["binding"])
        self.assertIn("development", out["label"])
        sel, _ = vl.halves(self.panel)
        sel_ids = {r["episode"] for r in sel}
        mixed = {r["episode"]: vl_record(r, host_class="dev-windows-haleyspc" if r["episode"] in sel_ids
                                         else "linux-zen5-9950x") for r in self.panel}
        self.assertFalse(vl.confirm(self.records, mixed, PARENT, B, self.lock(mixed), replicates=REPS)["binding"])
        two = {r["episode"]: vl_record(r, binary=("y" if r["episode"] in sel_ids else "x") * 64) for r in self.panel}
        self.assertFalse(vl.confirm(self.records, two, PARENT, B, self.lock(two), replicates=REPS)["binding"])

    def test_expensive_value_leaf_fails_the_efficiency_clauses(self):
        dear = {r["episode"]: vl_record(r, cpu=8.0) for r in self.panel}
        out = vl.confirm(self.records, dear, PARENT, B, self.lock(dear), replicates=REPS)
        self.assertEqual(out["verdict"], "fail")
        self.assertFalse(out["clauses"]["efficiency_point"])

    def test_nonpositive_comparator_is_inconclusive(self):
        lock = self.lock()
        real = s0.state_gains

        def negative(record, budgets=s0.BUDGETS):
            out = real(record, budgets)
            for b in budgets:
                out[b]["gain"] = -abs(out[b]["gain"]) - 1e-3
            return out
        with mock.patch.object(s0, "state_gains", negative):
            out = vl.confirm(self.records, self.vl, PARENT, B, lock, replicates=REPS)
        self.assertEqual(out["verdict"], "inconclusive")

    def test_lock_is_authenticated(self):
        lock = self.lock()
        other = "t0-n1024" if lock["selected"] != "t0-n1024" else "h0-n256"
        forged = dict(lock, selected=other)
        with self.assertRaises(s0.StageError):  # body no longer matches its hash
            vl.confirm(self.records, self.vl, PARENT, B, forged, replicates=REPS)
        body = {k: v for k, v in forged.items() if k != "lock_sha256"}
        import hashlib
        forged["lock_sha256"] = hashlib.sha256(json.dumps(body, sort_keys=True).encode()).hexdigest()
        with self.assertRaises(s0.StageError):  # rehashed, but not what the selection half gives
            vl.confirm(self.records, self.vl, PARENT, B, forged, replicates=REPS)

    def test_refusals(self):
        rec = self.panel[0]
        cases = []
        cases.append(vl_record(rec, memo="off"))
        bad = vl_record(rec)
        bad["comparator"][0]["key_memo_limit_bytes"] = 128 << 20
        cases.append(bad)
        bad = vl_record(rec)
        a = rec["qref"]["actions"][0]
        bad["qref_extra"] = {"rollouts": 128, "seed_root": "x", "actions": [a], "n": [128], "wins": [1], "draws": [0]}
        cases.append(bad)
        bad = vl_record(rec)
        bad["qref_extra"] = {"rollouts": 128, "seed_root": "x", "actions": [9, 9], "n": [128, 128], "wins": [1, 2],
                             "draws": [0, 0]}
        cases.append(bad)
        bad = vl_record(rec)
        bad["configs"] = bad["configs"][1:]
        cases.append(bad)
        bad = vl_record(rec)
        bad["configs"][0]["n_sims"] = 333
        cases.append(bad)
        bad = vl_record(rec)
        bad["comparator"][0]["cpu_s"] = -1.0
        cases.append(bad)
        for bad in cases:
            with self.assertRaises(s0.StageError):
                vl.validate_vl(bad, rec, B, GRID, HOR)

    def test_crossclass(self):
        eps = [r["episode"] for r in self.panel[:5]]
        node = {e: vl_record(r, tag="") for e, r in ((r["episode"], r) for r in self.panel[:5])}
        dev = {e: vl_record(r, host_class="dev-windows-haleyspc") for e, r in ((r["episode"], r) for r in self.panel[:5])}
        self.assertTrue(vl.crossclass(node, dev, eps)["all_equal"])
        dev2 = {e: vl_record(r, host_class="dev-windows-haleyspc", tag="z") for e, r in
                ((r["episode"], r) for r in self.panel[:5])}
        res = vl.crossclass(node, dev2, eps)
        self.assertFalse(res["all_equal"])
        self.assertEqual(res["compared"], 5 * len(GRID) * len(HOR) * 2)
        with self.assertRaises(s0.StageError):  # same class on both sides
            vl.crossclass(node, node, eps)
        with self.assertRaises(s0.StageError):  # a named episode missing
            vl.crossclass(node, {e: v for e, v in dev.items() if e != eps[0]}, eps)

    def test_cli_round_trip(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "cap").mkdir()
            (d / "vl").mkdir()
            (d / "cap" / "capture-0.jsonl").write_text(
                "".join(json.dumps(r) + "\n" for r in self.records), encoding="utf-8")
            (d / "vl" / "vl-0.jsonl").write_text(
                "".join(json.dumps(v) + "\n" for v in self.vl.values()), encoding="utf-8")
            common = ["--captures", str(d / "cap"), "--vl", str(d / "vl"), "--parent-sha256", PARENT,
                      "--budget", str(B), "--lock", str(d / "lock.json")]
            grid = ["--grid", ",".join(map(str, GRID)), "--horizons", ",".join(HOR)]
            with mock.patch.object(s0, "REPLICATES", REPS):
                self.assertEqual(vl.main(["select", *common, *grid]), 0)
                self.assertEqual(vl.main(["select", *common, *grid]), 1)  # already locked
                self.assertEqual(vl.main(["confirm", *common, "--out", str(d / "out.json")]), 0)
            self.assertIn(json.loads((d / "out.json").read_text())["verdict"], ("pass", "fail", "inconclusive"))


if __name__ == "__main__":
    unittest.main()
