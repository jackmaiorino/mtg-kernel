import copy
import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import analyze_stage4a as A  # noqa: E402

DRAWS = 300  # tests use fewer draws for speed; the frozen value is 10,000


def arm_row(j=0, w_extra=0, unk=0, disc=False):
    """World i: i<j winning with suffix; next w_extra wins without suffix; last `unk` truncated; rest nonwin."""
    ev = []
    for i in range(16):
        if i < j:
            e = dict(w=True, j=True, unknown=False, end="win")
        elif i < j + w_extra:
            e = dict(w=True, j=False, unknown=False, end="win")
        elif i >= 16 - unk:
            e = dict(w=False, j=False, unknown=True, end="truncated")
        else:
            e = dict(w=False, j=False, unknown=False, end="nonwin")
        e.update(world=i, transitions=100)
        ev.append(e)
    return {"selection": {"discovery": {"discovered": disc, "completions_natural": 1 if disc else 0,
                                        "completion_wins": 0, "completion_distinct_seeds": 1 if disc else 0},
                          "incomplete": False, "faults": [], "cap_reached": False,
                          "transitions": 5000, "inference_calls": 70},
            "eval": ev,
            "summary": {"w": j + w_extra, "j": j, "unknown": unk, "faults": 0}}


def make(per_cell=25, e=None, a=None, d=None, e_disc=lambda m, c, k: True, models=("r1", "r2"), jvary=0):
    """Return (roots, rows). e/a/d are kwargs dicts for arm_row; E's j varies by jvary across roots."""
    e, a, d = e or {}, a or {}, d or {}
    roots, rows = [], []
    for m in models:
        for c in A.CELLS:
            for k in range(per_cell):
                rid = "%s-%s-%d" % (m, c.replace("/", "_"), k)
                roots.append(dict(root_id=rid, model=m, cell=c, stratum=c.split("/")[0], game=k, seed=1000 + k))
                ee = dict(e)
                if jvary:
                    ee["j"] = e.get("j", 0) + (k % (jvary + 1))
                rows.append({"kind": "s4a_root", "root_id": rid, "model": m, "cell": c, "stratum": c.split("/")[0],
                             "game": k, "seed": 1000 + k, "invalid": False, "rejected_eval_worlds": [],
                             "config": {"limits": {"formal": True}},
                             "arms": {"E": arm_row(disc=e_disc(m, c, k), **ee), "A": arm_row(**a), "D": arm_row(**d)},
                             "cost": {"selection_transitions": 15000, "eval_transitions": 4800},
                             "timing": {"root_wall": 2.5}})
    return roots, rows


def run(roots, rows, **kw):
    return A.run_analysis(roots, [("rows", rows, 0)], DRAWS, **kw)


class QuantileTests(unittest.TestCase):
    def test_type7_hand_computed(self):
        self.assertEqual(A.quantile7([1, 2, 3, 4, 5], 0.25), 2)
        self.assertAlmostEqual(A.quantile7([1, 2, 3, 4], 0.25), 1.75)  # numpy default
        self.assertAlmostEqual(A.quantile7([1, 2, 3, 4], 0.5), 2.5)
        self.assertAlmostEqual(A.quantile7([10, 20], 0.975), 19.75)
        self.assertEqual(A.quantile7([7], 0.003125), 7)
        self.assertEqual(A.quantile7([1, 2, 3], 1.0), 3)
        self.assertIsNone(A.quantile7([], 0.5))
        xs = list(range(10000))
        self.assertAlmostEqual(A.quantile7(xs, 0.003125), 9999 * 0.003125)


class BootstrapTests(unittest.TestCase):
    def test_reproducible_and_seeded(self):
        vec = {"k": {c: [i % 5 for i in range(25)] for c in A.CELLS}}
        sizes = {c: 25 for c in A.CELLS}
        b1 = A.bootstrap(vec, sizes, 200)
        b2 = A.bootstrap(vec, sizes, 200)
        b3 = A.bootstrap(vec, sizes, 200, seed=1)
        self.assertEqual(b1, b2)
        self.assertNotEqual(b1, b3)

    def test_draw_procedure(self):
        import random
        rng = random.Random(A.BOOT_SEED)
        vec = {"k": {c: list(range(25)) for c in A.CELLS}}
        sizes = {c: 25 for c in A.CELLS}
        tot = 0.0
        for c in A.CELLS:
            idx = [rng.randrange(25) for _ in range(25)]
            tot += sum(idx) / (25 * 16)
        expect = 100.0 * tot / 4
        self.assertAlmostEqual(A.bootstrap(vec, sizes, 1)["k"][0], expect)

    def test_analysis_reproducible(self):
        roots, rows = make(e=dict(j=6), jvary=3)
        r1, r2 = run(roots, rows), run(roots, rows)
        self.assertEqual(json.dumps(r1, sort_keys=True), json.dumps(r2, sort_keys=True))


class AnalysisTests(unittest.TestCase):
    def assertJsonFinite(self, res):
        json.dumps(res, allow_nan=False)

    def test_clean_pass(self):
        roots, rows = make(e=dict(j=10, w_extra=1), jvary=3)
        res = run(roots, rows)
        self.assertEqual(res["disposition"], A.DISPOSITIONS[3], res["reason"])
        for m in res["models"].values():
            self.assertTrue(m["valid"], m["problems"])
            self.assertEqual(len(m["contrasts"]), 4)
            self.assertTrue(all(c["pass"] for c in m["contrasts"].values()))
            self.assertTrue(all(i["identity_ok"] for i in m["accounting"].values()))
            self.assertEqual(m["contrasts"]["E-A/W"]["obs"]["events_of"], 1600)
        self.assertJsonFinite(res)
        txt = A.render_text(res)
        self.assertIn("Candidate experience generator qualified", txt)

    def test_failing_discovery_total(self):
        roots, rows = make(e=dict(j=10), e_disc=lambda m, c, k: k < 10)  # 40/100 per model
        res = run(roots, rows)
        self.assertEqual(res["disposition"], A.DISPOSITIONS[1])
        self.assertFalse(res["models"]["r1"]["discovery"]["E"]["gate"])

    def test_failing_discovery_one_stage(self):
        # 60/100 overall but only 20/50 target
        roots, rows = make(e=dict(j=10), e_disc=lambda m, c, k: c.startswith("cast") or k < 10)
        res = run(roots, rows)
        d = res["models"]["r2"]["discovery"]["E"]
        self.assertEqual((d["all"], d["cast"], d["target"]), (70, 50, 20))
        self.assertEqual(res["disposition"], A.DISPOSITIONS[1])

    def test_unknowns_observed_pass_conservative_fail(self):
        roots, rows = make(e=dict(j=6), a=dict(unk=14), d=dict(unk=14))
        res = run(roots, rows)
        c = res["models"]["r1"]["contrasts"]["E-A/W"]
        self.assertTrue(c["obs"]["pass"])
        self.assertEqual(c["obs"]["events"], 600)
        self.assertFalse(c["cons"]["pass"])
        self.assertEqual(c["cons"]["events"], -800)  # (2 - 10) per root x 100
        self.assertEqual(res["disposition"], A.DISPOSITIONS[2])
        self.assertTrue(all(i["identity_ok"] for i in res["models"]["r1"]["accounting"].values()))

    def test_all_ties(self):
        roots, rows = make(e=dict(j=4), a=dict(j=4), d=dict(j=4))
        res = run(roots, rows)
        c = res["models"]["r1"]["contrasts"]["E-D/J"]
        self.assertEqual(c["obs"]["events"], 0)
        self.assertEqual(c["obs"]["ci95"], [0.0, 0.0])
        self.assertEqual(c["obs"]["bonf_99375"], [0.0, 0.0])
        self.assertEqual(res["disposition"], A.DISPOSITIONS[2])
        self.assertJsonFinite(res)

    def test_missing_root_invalid(self):
        roots, rows = make(e=dict(j=10))
        rows = rows[1:]
        res = run(roots, rows)
        self.assertEqual(res["disposition"], A.DISPOSITIONS[0])
        self.assertEqual(res["models"]["r1"]["missing"], 1)
        self.assertTrue(res["models"]["r2"]["valid"])
        self.assertIn("DESCRIPTIVE", A.render_text(res))
        self.assertJsonFinite(res)

    def test_empty_cell(self):
        roots, rows = make(e=dict(j=10))
        roots = [r for r in roots if r["cell"] != "target/A48"]
        res = run(roots, rows)
        self.assertEqual(res["disposition"], A.DISPOSITIONS[0])
        self.assertEqual(res["models"]["r1"]["cell_sizes"]["target/A48"], 0)
        self.assertJsonFinite(res)

    def test_empty_inputs(self):
        res = A.run_analysis([], [], 50)
        self.assertEqual(res["disposition"], A.DISPOSITIONS[0])
        self.assertJsonFinite(res)
        roots, _ = make()
        res = A.run_analysis(roots, [("x", [], 0)], 50)
        self.assertEqual(res["disposition"], A.DISPOSITIONS[0])
        self.assertEqual(res["models"]["r1"]["usable"], 0)
        self.assertJsonFinite(res)
        A.render_text(res)

    def test_error_and_fault_rows_invalid(self):
        roots, rows = make(e=dict(j=10))
        res = run(roots, rows + [{"kind": "error", "model": "r1", "message": "boom"}])
        self.assertFalse(res["models"]["r1"]["valid"])
        rows2 = copy.deepcopy(rows)
        rows2[0]["rejected_eval_worlds"] = [3]
        rows2[1]["arms"]["A"]["selection"]["faults"] = ["x"]
        res = run(roots, rows2)
        self.assertEqual(res["models"]["r1"]["flagged"], 2)
        self.assertEqual(res["disposition"], A.DISPOSITIONS[0])

    def test_malformed_eval_invalid(self):
        roots, rows = make(e=dict(j=10))
        rows[0]["arms"]["D"]["eval"].pop()
        rows[1]["arms"]["E"]["eval"][0]["transitions"] = float("inf")
        res = A.run_analysis(roots, [("r", rows, 0)], 50)
        self.assertEqual(res["disposition"], A.DISPOSITIONS[0])
        self.assertEqual(res["models"]["r1"]["usable"], 98)

    def test_nonformal_label(self):
        roots, rows = make(e=dict(j=10))
        rows[0]["config"]["limits"]["formal"] = False
        res = run(roots, rows, allow_nonformal=True)
        self.assertEqual(res["label"].split(";")[0], "ENGINEERING, NOT A RESULT")
        res = run(roots, rows, allow_nonformal=False)
        self.assertTrue(res["disposition"].startswith("REJECTED"))

    def test_cli_and_exit_codes(self):
        roots, rows = make(e=dict(j=10), models=("r1", "r2"))
        with tempfile.TemporaryDirectory() as td:
            rp, r1p, out = [os.path.join(td, x) for x in ("roots.jsonl", "r1.jsonl", "out")]
            with open(rp, "w") as fh:
                fh.write("\n".join(json.dumps(r) for r in roots) + "\n")
            with open(r1p, "w") as fh:
                fh.write("\n".join(json.dumps(r) for r in rows if r["model"] == "r1") + "\nnot json\n")
            code = A.main(["--roots", rp, "--rows", r1p, "--out", out, "--draws", "50"])
            self.assertEqual(code, 0)  # failure gates (invalid) still exit 0
            with open(os.path.join(out, "analysis.json")) as fh:
                data = json.load(fh)
            self.assertEqual(data["disposition"], A.DISPOSITIONS[0])
            self.assertTrue(data["label"].endswith("NON-FROZEN BOOTSTRAP DRAWS"))
            self.assertTrue(os.path.exists(os.path.join(out, "analysis.txt")))


if __name__ == "__main__":
    unittest.main()
