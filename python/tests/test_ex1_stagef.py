"""EX1 Stage F launcher and readouts (tools/ex1/stagef.py, tools/ex1/stagef_read.py): the extent formula, pairing,
arm-specific throughput guards, the hold at update 300, rolling thinning, and the section 6 statistics. The
training binary is a Python stand-in."""
import hashlib
import json
import os
import socket
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools" / "ex1"))
import stagef  # noqa: E402
import stagef_read  # noqa: E402

FAKE = '''
import json, sys, time, hashlib
from pathlib import Path
cfg = json.loads(Path(sys.argv[1]).read_text())
root = Path(cfg["output_directory"])
slow = next((float(p.joinpath("SLOW_AFTER_300").read_text()) for p in root.parents
             if (p / "SLOW_AFTER_300").exists()), 0.0)
for i in range(len(cfg["iterations"])):
    d = root / "iterations" / f"{i:06d}"
    if (d / "complete.json").exists():
        continue
    a = d / "attempt-000000"
    (a / "collect").mkdir(parents=True, exist_ok=True)
    (a / "update").mkdir(parents=True, exist_ok=True)
    if i >= 300:
        time.sleep(slow)
    ep = a / "collect" / "episode-0000.json"; ep.write_text("trajectory %d" % i)
    ck = a / "update" / "checkpoint.json"; ck.write_text("checkpoint %d" % i)
    h = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
    c = a / "collect" / "collection.json"
    c.write_text(json.dumps({"collection_elapsed_seconds": 1.5,
                             "trajectories": [{"path": ep.as_posix(), "sha256": h(ep)}]}))
    u = a / "update" / "update.json"; u.write_text(json.dumps({"after_state_sha256": "s" * 64}))
    (d / "complete.json").write_text(json.dumps({"iteration": i, "collection": {"path": c.as_posix()},
        "update": {"path": u.as_posix()}, "driver_process_cpu_seconds": 1.0,
        "output_identity": {"checkpoint_sha256": h(ck)}}))
(root / "completion.json").write_text("{}")
'''


SITE = '''
import os, shutil
if os.environ.get("STAGEF_TEST_FREE"):
    _free = float(os.environ["STAGEF_TEST_FREE"])
    shutil.disk_usage = lambda _path: type("U", (), {"free": _free})()
'''


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class StageFTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        t = Path(self.tmp.name)
        self.t = t
        self.root = t / "stagef"
        self.root.mkdir()
        self.binary = t / "fake_binary.py"
        self.binary.write_text(FAKE)
        self.inputs = {}
        for lineage in ("r1", "r2"):
            ckpt = t / f"{lineage}-ckpt.json"
            ckpt.write_text(json.dumps({"lineage": lineage}))
            dose = t / f"{lineage}-dose.json"
            dose.write_text(json.dumps({"source": {"checkpoint": {"path": ckpt.as_posix(), "sha256": sha(ckpt)}}}))
            base = t / f"{lineage}-base.json"
            base.write_text(json.dumps({"schema": "x", "preparation_workers": 2, "iterations": [
                {"episodes": [{"episode": {"learner_seat": s}, "opponent": {"kind": "current"}} for s in (0, 1)]}]}))
            self.inputs[lineage] = (dose, base)
        bsha = sha(self.binary)
        self.qualification = t / "QUALIFICATION.json"
        self.qualification.write_text(json.dumps({
            "design": {"version": "v3.3"}, "engine": {"binaries": {"native_expanded_training_run_v1": bsha}},
            "checks": {"resume_determinism": {"pass": True, "states": ["s" * 64]},
                       "hook_on_off": {"pass": True, "o_state": "s" * 64}},
            "measurements": {"cpu_per_x_update": {"mean": 924.0}}}))
        self.calibration = t / "CALIBRATION.json"
        self.calibration.write_text(json.dumps(self.cal()))
        self.timing = t / "O-TIMING.json"
        self.timing.write_text(json.dumps({"schema": stagef.O_TIMING_SCHEMA, "binary_sha256": bsha,
                                           "cpu_seconds_per_o_update": {"r1": 2.0, "r2": 2.0}}))
        self.plan = t / "PLAN.json"
        usage = mock.patch.object(stagef.shutil, "disk_usage", side_effect=lambda _: type("U", (), {"free": 500e9})())
        usage.start()
        self.addCleanup(usage.stop)
        # A pack group's `_members` subprocess sees the same free space through a test-only sitecustomize.
        (t / "sitecustomize.py").write_text(SITE)
        env = mock.patch.dict(stagef.os.environ, {
            "PYTHONPATH": os.pathsep.join([str(t), *filter(None, [os.environ.get("PYTHONPATH")])]),
            "STAGEF_TEST_FREE": "500e9"})
        env.start()
        self.addCleanup(env.stop)

    def tearDown(self):
        self.tmp.cleanup()

    def cal(self, **changes):
        cal = {"schema": "ex1-p2-calibration/v1", "pass": True, "dose": {"r_star": 1.0, "kappa": 2.0},
               "binary": {"sha256": sha(self.binary)},
               "lineages": {lin: {"k1": {"c_star": c}, "k2": {"pass": True, "coefficient": c, "kappa": 2.0}}
                            for lin, c in (("r1", 0.15), ("r2", 0.128))}}
        cal.update(changes)
        return cal

    def budget(self, usd):
        path = self.t / f"BUDGET-{usd}.json"
        path.write_text(json.dumps({"schema": stagef.BUDGET_SCHEMA, "allowance_usd": 200.0,
                                    "spend": [{"item": "G(i)", "usd": 200.0 - usd}]}))
        return path

    def make(self, places=None, b=30.0, namespace="ex1-stagef-test", receipt=None):
        places = places or {name: "desk:2" for name in stagef.CHAINS}
        argv = ["plan", "--qualification", str(self.qualification), "--calibration", str(self.calibration),
                "--binary", str(self.binary), "--root", self.root.as_posix(), "--out", str(self.plan),
                "--budget", str(self.budget(b)), "--o-timing", str(self.timing), "--namespace", namespace]
        if receipt:
            argv += ["--regeneration-receipt", str(receipt)]
        for lineage, (dose, base) in self.inputs.items():
            argv += ["--parent", f"{lineage}={dose}", "--base", f"{lineage}={base}"]
        for chain, place in places.items():
            argv += ["--place", f"{chain}={place}"]
        stagef.main(argv)
        return json.loads(self.plan.read_text())

    def record(self, host, arm, pack=1, **changes):
        plan = json.loads(self.plan.read_text())
        rec = {"host": host, "arm": arm, "hostname": socket.gethostname(), "binary_sha256": plan["binary"]["sha256"],
               "cores": 8, "workers": 10, "pack": pack, "exit": 0, "state_equal": True, "state_required": True}
        rec.update(changes)
        stagef.write(stagef.throughput_path(plan, host, arm, pack), rec)

    # ---------------------------------------------------------------- plan

    def test_extent_is_section_5s_formula(self):
        plan = self.make(b=30.0)
        expected = int(0.8 * 30.0 * 3600 // (0.07 * (4 * 924.0 + 4 * 2.0)))
        self.assertEqual(plan["extent"]["updates"], expected)
        self.assertTrue(all(c["updates"] == expected for c in plan["chains"].values()))
        self.assertEqual(stagef.extent(184.0, [924.0] * 4 + [2.0] * 4), 2043)
        with self.assertRaisesRegex(SystemExit, "below the hold"):
            self.make(b=20.0)

    def test_pairs_share_seeds_and_differ_only_in_the_ex1_block(self):
        plan = self.make()
        cfg = {n: json.loads(Path(c["config"]["path"]).read_text()) for n, c in plan["chains"].items()}
        for lin in ("r1", "r2"):
            for i in (1, 2):
                x, o = cfg[f"{lin}-p{i}-x"], cfg[f"{lin}-p{i}-o"]
                self.assertEqual(x["iterations"], o["iterations"])
                self.assertIn("ex1", x)
                self.assertNotIn("ex1", o)
                self.assertEqual(x["ex1"]["cap"], {"mode": "ratio", "kappa": 2.0})
                self.assertEqual(x["ex1"]["coefficient"], {"r1": 0.15, "r2": 0.128}[lin])
                self.assertEqual(stagef.pair_difference(x, o), [])
        seeds = {n: {e["episode"]["seed"] for it in c["iterations"] for e in it["episodes"]} for n, c in cfg.items()}
        keys = sorted({n[:-2] for n in cfg})
        for a in keys:
            for b in keys:
                if a != b:
                    self.assertFalse(seeds[a + "-x"] & seeds[b + "-x"], (a, b))

    def test_plan_refusals(self):
        self.calibration.write_text(json.dumps(self.cal(dose={"r_star": 0.3, "kappa": 1.0})))
        with self.assertRaisesRegex(SystemExit, "not at r"):
            self.make()
        self.calibration.write_text(json.dumps(self.cal()))
        with self.assertRaisesRegex(SystemExit, "fresh namespace"):
            self.make(namespace="ex1-p2-20261101")
        places = {name: "desk:2" for name in stagef.CHAINS}
        with self.assertRaisesRegex(SystemExit, "only X chains pack"):
            self.make(dict(places, **{"r1-p1-o": "desk:2:g"}))
        with self.assertRaisesRegex(SystemExit, "owned hosts"):
            self.make(dict(places, **{"r1-p1-x": "pod:8:0.24:6"}))

    def test_guard_needs_the_arms_own_record(self):
        self.make()
        with self.assertRaisesRegex(SystemExit, "no X throughput record for desk"):
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r1-p1-x", "--fast-poll"])
        self.record("desk", "o")
        stagef.write(stagef.throughput_path(json.loads(self.plan.read_text()), "desk", "x", 1),
                     json.loads(stagef.throughput_path(json.loads(self.plan.read_text()), "desk", "o", 1)
                                .read_text()))
        with self.assertRaisesRegex(SystemExit, "recorded for arm o, not x"):
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r1-p1-x", "--fast-poll"])

    # ---------------------------------------------------------------- hold

    def test_a_chain_stops_at_the_hold_until_the_lead_clears_it(self):
        plan = self.make()
        self.record("desk", "x")
        (self.root / "SLOW_AFTER_300").write_text("30")
        with self.assertRaises(SystemExit) as stop:
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r2-p1-x", "--fast-poll"])
        self.assertEqual(stop.exception.code, stagef.EXIT_HOLD)
        native = Path(plan["chains"]["r2-p1-x"]["native"])
        self.assertEqual(stagef.completed(native), 300)
        self.assertFalse((native / "iterations" / "000300").exists())       # the partial one is moved aside
        with self.assertRaisesRegex(SystemExit, "held at update 300"):
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r2-p1-x", "--fast-poll"])
        (self.root / "SLOW_AFTER_300").write_text("0")
        stagef.write(self.root / stagef.HOLD_CLEARED, {"read": "steps 1 and 2 pass"})
        with self.assertRaises(SystemExit) as end:
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r2-p1-x", "--fast-poll"])
        self.assertEqual(end.exception.code, 0)
        self.assertEqual(stagef.completed(native), plan["extent"]["updates"])
        done = json.loads((self.root / "chains" / "r2-p1-x" / "DONE.json").read_text())
        self.assertEqual(done["completed_updates"], plan["extent"]["updates"])
        self.assertEqual(done["metered_usd"], 0.0)

    def test_stagef_stopped_refuses_every_chain(self):
        self.make()
        self.record("desk", "o")
        stagef.write(self.root / stagef.STOPPED, {"why": "step 1 failed at the hold"})
        with self.assertRaisesRegex(SystemExit, "Stage F is stopped"):
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r1-p1-o", "--fast-poll"])

    def test_a_pack_groups_members_stop_at_the_hold_one_by_one(self):
        places = {name: "desk:2" for name in stagef.CHAINS}
        places.update({"r1-p1-x": "desk:8:g", "r1-p2-x": "desk:8:g"})
        plan = self.make(places)
        self.record("desk", "x", pack=2)
        (self.root / "SLOW_AFTER_300").write_text("30")
        with self.assertRaises(SystemExit) as stop:
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r1-p1-x", "--chain", "r1-p2-x",
                         "--fast-poll"])
        self.assertEqual(stop.exception.code, stagef.EXIT_HOLD)
        for name in ("r1-p1-x", "r1-p2-x"):
            self.assertEqual(stagef.completed(plan["chains"][name]["native"]), 300)
        with self.assertRaisesRegex(SystemExit, "pack group g is held"):
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r1-p1-x", "--chain", "r1-p2-x",
                         "--fast-poll"])

    # ---------------------------------------------------------------- storage

    def test_thin_moves_only_uncited_bulk_outside_the_keep_set(self):
        plan = self.make()
        self.record("desk", "o")
        (self.root / "SLOW_AFTER_300").write_text("30")
        with self.assertRaises(SystemExit):
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r1-p1-o", "--fast-poll"])
        native = Path(plan["chains"]["r1-p1-o"]["native"])
        dry = stagef.thin_once(stagef.load_plan(self.plan), apply=False)
        self.assertEqual(stagef.completed(native), 300)
        # Kept: updates 100, 200, 300 (also the hold) and the last 20 (281..300); 300 - 22 = 278 iterations thinned.
        self.assertEqual(dry["chains"]["r1-p1-o"]["files"], 2 * 278)
        self.assertTrue((native / "iterations" / "000000" / "attempt-000000" / "update" / "checkpoint.json").exists())
        out = stagef.thin_once(stagef.load_plan(self.plan), apply=True)
        self.assertEqual(out["chains"]["r1-p1-o"]["files"], 2 * 278)
        it0 = native / "iterations" / "000000" / "attempt-000000"
        self.assertFalse((it0 / "update" / "checkpoint.json").exists())
        self.assertFalse((it0 / "collect" / "episode-0000.json").exists())
        for kept in ("complete.json",):
            self.assertTrue((native / "iterations" / "000000" / kept).exists())
        self.assertTrue((it0 / "collect" / "collection.json").exists())
        self.assertTrue((it0 / "update" / "update.json").exists())
        for update in (100, 200, 281, 300):
            self.assertTrue((native / "iterations" / f"{update - 1:06d}" / "attempt-000000" / "update" /
                             "checkpoint.json").exists(), update)
        rows = [json.loads(x) for x in (self.root / "THIN-MANIFEST.jsonl").read_text().splitlines()]
        self.assertEqual(len(rows), 2 * 278)
        self.assertTrue(all(Path(r["to"]).exists() and sha(r["to"]) == r["sha256"] for r in rows))
        self.assertEqual(stagef.thin_once(stagef.load_plan(self.plan), apply=True)["bytes"], 0)   # idempotent
        # Recent holding folders stay without a regeneration receipt.
        stagef.main(["purge", "--plan", str(self.plan), "--apply"])
        self.assertTrue(all(Path(r["to"]).exists() for r in rows))

    def test_thin_stops_on_a_file_that_does_not_match_its_pin(self):
        plan = self.make()
        self.record("desk", "o")
        (self.root / "SLOW_AFTER_300").write_text("30")
        with self.assertRaises(SystemExit):
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r1-p1-o", "--fast-poll"])
        ck = Path(plan["chains"]["r1-p1-o"]["native"]) / "iterations" / "000005" / "attempt-000000" / "update"
        (ck / "checkpoint.json").write_text("tampered")
        with self.assertRaisesRegex(SystemExit, "does not match the hash"):
            stagef.thin_once(stagef.load_plan(self.plan), apply=True)

    def test_purge_with_a_regeneration_receipt_deletes_at_once(self):
        receipt = self.t / "REGEN.json"
        receipt.write_text("{}")
        plan = self.make(receipt=receipt)
        held = self.root / "_hold-thin" / time.strftime("%Y%m%d") / "r1-p1-o" / "iterations" / "000001"
        held.mkdir(parents=True)
        (held / "checkpoint.json").write_text("x")
        self.assertEqual(plan["thin"]["regeneration_receipt"]["sha256"], sha(receipt))
        stagef.main(["purge", "--plan", str(self.plan), "--apply"])
        self.assertFalse((self.root / "_hold-thin" / time.strftime("%Y%m%d")).exists())


class StageFReadTests(unittest.TestCase):
    def test_case_lists(self):
        cases = stagef_read.all_cases()["pairs"]
        self.assertEqual(len(cases), 4)
        for v in cases.values():
            self.assertEqual(len(v["primary"]), 360)
            self.assertEqual(len(v["transfer"]), 240)
            self.assertEqual(len({c["case"] for c in v["primary"]}), 360)
            self.assertTrue(all(c["own"] == "Spy" for c in v["primary"]))
        seeds = {k: {c["seed"] for c in v["primary"]} | {c["seed"] for c in v["transfer"]} for k, v in cases.items()}
        keys = sorted(seeds)
        for a in keys:
            for b in keys:
                if a != b:
                    self.assertFalse(seeds[a] & seeds[b])
        structure = {k: [c["case"] for c in v["primary"]] for k, v in cases.items()}
        self.assertEqual(len({tuple(s) for s in structure.values()}), 1)

    def test_mcnemar(self):
        self.assertAlmostEqual(stagef_read.mcnemar_one_sided(5, 0), 1 / 32)
        self.assertAlmostEqual(stagef_read.mcnemar_one_sided(4, 0), 1 / 16)
        self.assertEqual(stagef_read.mcnemar_one_sided(0, 0), 1.0)
        self.assertAlmostEqual(stagef_read.mcnemar_one_sided(1, 1), 0.75)

    def rows(self, completions, failed=()):
        out = []
        for lin in ("r1", "r2"):
            for i in (1, 2):
                for k in range(10):
                    for arm in ("x", "o"):
                        done = (lin, i, k, arm) in completions
                        out.append({"lineage": lin, "pair": i, "arm": arm, "case": f"c{k}",
                                    "replay_ok": (lin, i, k, arm) not in failed, "combo_resolved": done,
                                    "combo_won": False})
        return out

    def test_primary(self):
        comp = {(lin, i, k, "x") for lin in ("r1", "r2") for i in (1, 2) for k in range(3)}
        out = stagef_read.primary(self.rows(comp))["lineages"]
        for lin in ("r1", "r2"):
            self.assertEqual((out[lin]["primary"]["b"], out[lin]["primary"]["c"]), (6, 0))
            self.assertAlmostEqual(out[lin]["primary"]["p_one_sided"], 1 / 64)
            self.assertTrue(out[lin]["chain_consistency"])
        out = stagef_read.primary(self.rows(comp, failed={("r1", 1, 0, "x"), ("r1", 1, 5, "o")}))["lineages"]["r1"]
        self.assertEqual(out["primary"]["excluded"], 2)
        self.assertEqual((out["primary"]["b"], out["primary"]["c"]), (5, 0))
        self.assertEqual((out["sensitivity"]["b"], out["sensitivity"]["c"]), (5, 1))
        with self.assertRaisesRegex(SystemExit, "not played by both arms"):
            stagef_read.primary(self.rows(comp)[:-1])

    def test_holm(self):
        self.assertEqual(stagef_read.holm({"a": 0.01, "b": 0.015, "c": 0.04, "d": 0.5}),
                         {"a": True, "b": True, "c": False, "d": False})
        self.assertEqual(stagef_read.holm({"a": 0.016, "b": 0.001, "c": 0.9, "d": 0.9}),
                         {"a": True, "b": True, "c": False, "d": False})

    def test_transfer(self):
        rows = []
        for cell in stagef_read.CELLS:
            for lin in ("r1", "r2"):
                for i in (1, 2):
                    for k in range(20):
                        for arm in ("x", "o"):
                            missed = 1 if arm == "o" or k % 5 == 0 else 0
                            rows.append({"lineage": lin, "pair": i, "arm": arm, "case": f"c{k}", "cell": cell,
                                         "certified_opportunities": 1, "missed": missed})
        out = stagef_read.transfer(rows, permutations=2000, bootstraps=200)
        again = stagef_read.transfer(rows, permutations=2000, bootstraps=200)
        self.assertEqual(out, again)                     # seeded
        for cell, v in out["cells"].items():
            self.assertLess(v["conjunction_p"], 0.01)
            self.assertTrue(v["holm_rejects"])
            self.assertAlmostEqual(v["lineages"]["r1"]["conditional_miss_rate"]["o_minus_x"], 0.8)

    def test_panel(self):
        rows = [{"lineage": lin, "pair": i, "arm": arm, "state": f"s{k}", "p_self": 0.1 if arm == "x" else 0.0}
                for lin in ("r1", "r2") for i in (1, 2) for k in range(5) for arm in ("x", "o")]
        out = stagef_read.panel(rows, bootstraps=100)["pairs"]
        self.assertAlmostEqual(out["r1-p1"]["x_minus_o"], 0.1)
        self.assertEqual(len(out), 4)


if __name__ == "__main__":
    unittest.main()
