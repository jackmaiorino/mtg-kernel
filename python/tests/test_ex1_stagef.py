"""EX1 Stage F launcher and readouts (tools/ex1/stagef.py, tools/ex1/stagef_read.py): the extent formula, pairing,
arm-specific throughput guards, segments and the exact hold at update 300, thinning and purging, the byte cap, and
the section 6 statistics. The training binary is a Python stand-in that, like the native trainer, keeps each
iteration's source and output checkpoint."""
import hashlib
import json
import os
import socket
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools" / "ex1"))
import stagef  # noqa: E402
import stagef_read  # noqa: E402

FAKE = '''
import json, sys, hashlib
from pathlib import Path
cfg = json.loads(Path(sys.argv[1]).read_text())
root = Path(cfg["output_directory"])
h = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
source = cfg["initial_source"]
for i, item in enumerate(cfg["iterations"]):
    d = root / "iterations" / f"{i:06d}"
    a = d / "attempt-000000"
    if (d / "complete.json").exists():
        done = json.loads((d / "complete.json").read_text())
        source = dict(done["source"], checkpoint={"path": (a / "update" / "checkpoint.json").as_posix(),
                                                  "sha256": done["output_identity"]["checkpoint_sha256"]})
        continue
    # The trainer reads its source checkpoint and refuses a changed one.
    assert h(Path(source["checkpoint"]["path"])) == source["checkpoint"]["sha256"], "source differs"
    (a / "collect").mkdir(parents=True, exist_ok=True)
    (a / "update").mkdir(parents=True, exist_ok=True)
    ep = a / "collect" / "episode-0000.json"; ep.write_text("trajectory " + item["episodes"][0]["episode"]["id"])
    ck = a / "update" / "checkpoint.json"
    ck.write_text("checkpoint after " + item["episodes"][0]["episode"]["id"] + " " + str(cfg.get("ex1")))
    c = a / "collect" / "collection.json"
    c.write_text(json.dumps({"collection_elapsed_seconds": 1.5,
                             "trajectories": [{"path": ep.as_posix(), "sha256": h(ep)}]}))
    u = a / "update" / "update.json"; u.write_text(json.dumps({"after_state_sha256": h(ck)}))
    (d / "complete.json").write_text(json.dumps({"iteration": i, "collection": {"path": c.as_posix()},
        "update": {"path": u.as_posix()}, "driver_process_cpu_seconds": 1.0, "source": source,
        "output_identity": {"checkpoint_sha256": h(ck)}}))
    source = dict(source, checkpoint={"path": ck.as_posix(), "sha256": h(ck)})
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
            dose.write_text(json.dumps({"source": {"checkpoint": {"path": ckpt.as_posix(), "sha256": sha(ckpt)},
                                                   "play_import": {"path": "t1.json", "sha256": "1" * 64}}}))
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
        # Subprocesses (`_drive`, `_members`) see the same free space through a test-only sitecustomize.
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

    def receipts(self, **changes):
        out = []
        for arm in ("x", "o"):
            path = self.t / f"SEGCHECK-{arm}.json"
            rec = {"schema": "ex1-stagef-segcheck/v1", "pass": True, "binary_sha256": sha(self.binary), "arm": arm}
            rec.update(changes)
            path.write_text(json.dumps(rec))
            out.append(path)
        return out

    def make(self, places=None, b=40.0, namespace="ex1-stagef-test", receipts=(), cap=None):
        places = places or {name: "desk:2" for name in stagef.CHAINS}
        argv = ["plan", "--qualification", str(self.qualification), "--calibration", str(self.calibration),
                "--binary", str(self.binary), "--root", self.root.as_posix(), "--out", str(self.plan),
                "--budget", str(self.budget(b)), "--o-timing", str(self.timing), "--namespace", namespace]
        for receipt in receipts:
            argv += ["--regeneration-receipt", str(receipt)]
        if cap:
            argv += ["--byte-cap", str(cap)]
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

    def run_chain(self, *names):
        argv = ["run", "--plan", str(self.plan), "--fast-poll"]
        for name in names:
            argv += ["--chain", name]
        with self.assertRaises(SystemExit) as out:
            stagef.main(argv)
        return out.exception.code

    # ---------------------------------------------------------------- plan

    def test_extent_is_section_5s_formula(self):
        plan = self.make(b=40.0)
        expected = int(0.8 * 40.0 * 3600 // (0.07 * (4 * 924.0 + 4 * 2.0)))
        self.assertEqual(plan["extent"]["updates"], expected)
        self.assertTrue(all(c["updates"] == expected for c in plan["chains"].values()))
        self.assertEqual(stagef.extent(184.0, [924.0] * 4 + [2.0] * 4), 2043)
        self.assertEqual(stagef.segments(2043)[:3], [(0, 300), (300, 100), (400, 100)])
        self.assertEqual(stagef.segments(2043)[-1], (2000, 43))
        self.assertEqual(sum(n for _, n in stagef.segments(2043)), 2043)
        with self.assertRaisesRegex(SystemExit, "below the hold"):
            self.make(b=20.0)

    def test_pairs_share_seeds_and_differ_only_in_the_ex1_block(self):
        plan = self.make()
        cfg = {n: json.loads(Path(c["config"]["path"]).read_text()) for n, c in plan["chains"].items()}
        for lin in ("r1", "r2"):
            for i in (1, 2):
                x, o = cfg[f"{lin}-p{i}-x"], cfg[f"{lin}-p{i}-o"]
                self.assertEqual(x["iterations"], o["iterations"])
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

    def test_a_segment_config_is_the_full_configs_slice(self):
        plan = self.make()
        full = json.loads(Path(plan["chains"]["r2-p1-x"]["config"]["path"]).read_text())
        source = {"checkpoint": {"path": "c.json", "sha256": "c" * 64}}
        seg = stagef.segment_config(full, 300, 33, self.root / "n", source)
        self.assertEqual(seg["iterations"], full["iterations"][300:333])
        self.assertEqual(seg["ex1"]["update_offset"], 300)
        self.assertEqual(seg["initial_source"], source)
        self.assertEqual(full["ex1"]["update_offset"], 0)            # the pinned config is untouched
        self.assertEqual({k: v for k, v in seg.items() if k not in ("iterations", "ex1", "initial_source",
                                                                     "output_directory")},
                         {k: v for k, v in full.items() if k not in ("iterations", "ex1", "initial_source",
                                                                      "output_directory")})

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
        plan = self.make()
        with self.assertRaisesRegex(SystemExit, "no X throughput record for desk"):
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r1-p1-x", "--fast-poll"])
        self.record("desk", "o")
        stagef.write(stagef.throughput_path(plan, "desk", "x", 1),
                     json.loads(stagef.throughput_path(plan, "desk", "o", 1).read_text()))
        with self.assertRaisesRegex(SystemExit, "recorded for arm o, not x"):
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r1-p1-x", "--fast-poll"])

    # ---------------------------------------------------------------- segments and the hold

    def test_a_chain_stops_exactly_at_the_hold_and_continues_in_segments(self):
        plan = self.make()
        self.record("desk", "x")
        self.assertEqual(self.run_chain("r2-p1-x"), stagef.EXIT_HOLD)
        chain = plan["chains"]["r2-p1-x"]
        native = Path(chain["native"])
        self.assertEqual(stagef.completed(native), 300)                   # the Stage P layout, exactly 300
        self.assertFalse(stagef.segment_native(chain, 1).exists())
        with self.assertRaisesRegex(SystemExit, "held at update 300"):
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r2-p1-x", "--fast-poll"])
        stagef.write(self.root / stagef.HOLD_CLEARED, {"read": "steps 1 and 2 pass"})
        self.assertEqual(self.run_chain("r2-p1-x"), 0)
        updates = plan["extent"]["updates"]
        self.assertEqual(stagef.progress(chain), (updates, None))
        seg1 = json.loads((self.root / "chains" / "r2-p1-x" / "segments" / "seg-001.json").read_text())
        last0 = json.loads((native / "iterations" / "000299" / "complete.json").read_text())
        self.assertEqual(seg1["initial_source"]["checkpoint"]["sha256"], last0["output_identity"]["checkpoint_sha256"])
        self.assertEqual(seg1["initial_source"]["play_import"], last0["source"]["play_import"])
        self.assertEqual(seg1["ex1"]["update_offset"], 300)
        first1 = json.loads((stagef.segment_native(chain, 1) / "iterations" / "000000" / "complete.json").read_text())
        self.assertEqual(first1["source"]["checkpoint"]["sha256"], last0["output_identity"]["checkpoint_sha256"])
        done = json.loads((self.root / "chains" / "r2-p1-x" / "DONE.json").read_text())
        self.assertEqual((done["completed_updates"], done["metered_usd"], done["segments"]),
                         (updates, 0.0, len(stagef.segments(updates))))

    def test_stagef_stopped_refuses_every_chain(self):
        self.make()
        self.record("desk", "o")
        stagef.write(self.root / stagef.STOPPED, {"why": "step 1 failed at the hold"})
        with self.assertRaisesRegex(SystemExit, "Stage F is stopped"):
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r1-p1-o", "--fast-poll"])

    def test_a_pack_groups_members_each_stop_at_the_hold(self):
        places = {name: "desk:2" for name in stagef.CHAINS}
        places.update({"r1-p1-x": "desk:8:g", "r1-p2-x": "desk:8:g"})
        plan = self.make(places)
        self.record("desk", "x", pack=2)
        self.assertEqual(self.run_chain("r1-p1-x", "r1-p2-x"), stagef.EXIT_HOLD)
        for name in ("r1-p1-x", "r1-p2-x"):
            self.assertEqual(stagef.progress(plan["chains"][name]), (300, 1))
        with self.assertRaisesRegex(SystemExit, "pack group g is held"):
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r1-p1-x", "--chain", "r1-p2-x",
                         "--fast-poll"])

    def test_the_byte_cap_refuses_a_launch(self):
        self.make(cap=1e5)
        self.record("desk", "o")
        (self.root / "bulk.bin").write_bytes(b"x" * 200_000)
        with self.assertRaisesRegex(SystemExit, "past the 0 GB cap"):
            stagef.main(["run", "--plan", str(self.plan), "--chain", "r1-p1-o", "--fast-poll"])

    # ---------------------------------------------------------------- storage

    def test_finished_segments_are_thinned_keeping_records_and_each_segments_last_checkpoint(self):
        plan = self.make()
        self.record("desk", "o")
        stagef.write(self.root / stagef.HOLD_CLEARED, {})
        self.assertEqual(self.run_chain("r1-p1-o"), 0)
        chain = plan["chains"]["r1-p1-o"]
        updates = plan["extent"]["updates"]
        rows = stagef.thin_rows(self.root)
        # Every trajectory, and every checkpoint but each segment's last.
        self.assertEqual(len(rows), 2 * updates - len(stagef.segments(updates)))
        for s, (_, n) in enumerate(stagef.segments(updates)):
            native = stagef.segment_native(chain, s)
            for i in range(n):
                a = native / "iterations" / f"{i:06d}" / "attempt-000000"
                self.assertTrue((native / "iterations" / f"{i:06d}" / "complete.json").exists())
                self.assertTrue((a / "collect" / "collection.json").exists())
                self.assertTrue((a / "update" / "update.json").exists())
                self.assertEqual((a / "update" / "checkpoint.json").exists(), i == n - 1)
                self.assertFalse((a / "collect" / "episode-0000.json").exists())
        self.assertEqual(len({r["to"] for r in rows}), len(rows))           # no two segments share a held path
        self.assertTrue(all(Path(r["to"]).exists() and sha(r["to"]) == r["sha256"] for r in rows))
        self.assertEqual(stagef.thin_chain(stagef.load_plan(self.plan), "r1-p1-o", apply=True), (0, 0))
        # Without a regeneration receipt nothing is purged inside the hold period.
        self.assertEqual(stagef.purge_held(stagef.load_plan(self.plan), apply=True)["files"], 0)
        with mock.patch.object(stagef.time, "time", return_value=rows[-1]["moved_at"] + 7 * 86400 + 1):
            out = stagef.purge_held(stagef.load_plan(self.plan), apply=True)
        self.assertEqual(out["files"], len(rows))
        self.assertFalse(any(Path(r["to"]).exists() for r in rows))
        self.assertEqual(len(stagef.manifest_rows(self.root / "thin" / "PURGE-MANIFEST.jsonl")), len(rows))

    def test_passing_segment_checks_for_both_arms_purge_after_each_segment(self):
        plan = self.make(receipts=self.receipts())
        self.record("desk", "o")
        self.assertEqual(self.run_chain("r2-p2-o"), stagef.EXIT_HOLD)
        rows = stagef.thin_rows(self.root)
        self.assertEqual(len(rows), 300 + 299)
        self.assertFalse(any(Path(r["to"]).exists() for r in rows))
        self.assertEqual(len(plan["thin"]["regeneration_receipts"]), 2)

    def test_a_failed_or_partial_segment_check_is_refused(self):
        with self.assertRaisesRegex(SystemExit, "not a passing segment check"):
            self.make(receipts=self.receipts(**{"pass": False}))
        with self.assertRaisesRegex(SystemExit, "cover arms"):
            self.make(receipts=self.receipts()[:1])
        with self.assertRaisesRegex(SystemExit, "not a passing segment check"):
            self.make(receipts=self.receipts(binary_sha256="0" * 64))

    def test_purge_refuses_a_linked_holding_folder_and_runs_one_at_a_time(self):
        self.make()
        self.record("desk", "o")
        self.run_chain("r1-p1-o")
        rows = stagef.thin_rows(self.root)
        later = rows[-1]["moved_at"] + 7 * 86400 + 1
        lock = self.root / "thin" / "PURGE.lock"
        live = {"pid": os.getpid(), "created": stagef.stagep.psutil.Process().create_time(), "token": "other"}
        lock.write_text(json.dumps(live))
        os.utime(lock, (0, 0))                            # old, but its owner (this process) still runs
        with mock.patch.object(stagef.time, "time", return_value=later):
            self.assertTrue(stagef.purge_held(stagef.load_plan(self.plan), apply=True).get("busy"))
        self.assertEqual(json.loads(lock.read_text())["token"], "other")      # never taken from a live owner
        lock.write_text(json.dumps(dict(live, created=live["created"] - 1000)))   # a reused PID: reclaimed
        held = stagef.lock(lock)
        self.assertIsNotNone(held)
        stagef.unlock(held)
        self.assertFalse(lock.exists())
        hold = self.root / "_hold-thin"
        outside = self.t / "outside"
        hold.rename(outside)
        if os.name == "nt":
            import _winapi
            _winapi.CreateJunction(str(outside), str(hold))
        else:
            os.symlink(outside, hold)
        with mock.patch.object(stagef.time, "time", return_value=later):
            with self.assertRaisesRegex(SystemExit, "is a link"):
                stagef.purge_held(stagef.load_plan(self.plan), apply=True)
        self.assertTrue(all((outside / Path(r["to"]).relative_to(hold)).exists() for r in rows))

    def test_room_counts_every_unfinished_chains_segment(self):
        plan = self.make()
        plan["byte_budget"]["cap_bytes"] = 8 * 300 * stagef.BYTES_PER_UPDATE + stagef.tree_bytes(self.root) + 10
        self.assertIsNone(stagef.room(plan))
        (self.root / "bulk.bin").write_bytes(b"x" * 20)
        self.assertIn("past the", stagef.room(plan))

    def test_a_changed_kept_checkpoint_stops_the_next_segment(self):
        plan = self.make()
        self.record("desk", "o")
        self.assertEqual(self.run_chain("r1-p2-o"), stagef.EXIT_HOLD)
        kept = Path(plan["chains"]["r1-p2-o"]["native"]) / "iterations" / "000299" / "attempt-000000" / "update"
        (kept / "checkpoint.json").write_text("tampered")
        stagef.write(self.root / stagef.HOLD_CLEARED, {})
        self.assertNotEqual(self.run_chain("r1-p2-o"), 0)
        self.assertFalse(stagef.segment_native(plan["chains"]["r1-p2-o"], 1).exists())

    def test_thin_stops_on_a_file_that_does_not_match_its_pin(self):
        plan = self.make()
        self.record("desk", "o")
        self.run_chain("r1-p1-o")
        row = stagef.thin_rows(self.root)[0]
        Path(row["to"]).replace(row["from"])
        Path(row["from"]).write_text("tampered")
        with self.assertRaisesRegex(SystemExit, "differs from the hash"):
            stagef.thin_chain(stagef.load_plan(self.plan), "r1-p1-o", apply=True)
        self.assertTrue(plan)


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
        for a in seeds:
            for b in seeds:
                if a != b:
                    self.assertFalse(seeds[a] & seeds[b])
        self.assertEqual(len({tuple(c["case"] for c in v["primary"]) for v in cases.values()}), 1)

    def test_mcnemar(self):
        self.assertAlmostEqual(stagef_read.mcnemar_one_sided(5, 0), 1 / 32)
        self.assertAlmostEqual(stagef_read.mcnemar_one_sided(4, 0), 1 / 16)
        self.assertEqual(stagef_read.mcnemar_one_sided(0, 0), 1.0)
        self.assertAlmostEqual(stagef_read.mcnemar_one_sided(1, 1), 0.75)

    def rows(self, completions, failed=()):
        out = []
        for lin in ("r1", "r2"):
            for i in (1, 2):
                for k, c in enumerate(stagef_read.primary_cases(lin, i)):
                    for arm in ("x", "o"):
                        out.append({"lineage": lin, "pair": i, "arm": arm, "case": c["case"],
                                    "replay_ok": (lin, i, k, arm) not in failed,
                                    "combo_resolved": (lin, i, k, arm) in completions, "combo_won": False})
        return out

    def test_primary(self):
        comp = {(lin, i, k, "x") for lin in ("r1", "r2") for i in (1, 2) for k in range(3)}
        out = stagef_read.primary(self.rows(comp))["lineages"]
        for lin in ("r1", "r2"):
            self.assertEqual((out[lin]["primary"]["b"], out[lin]["primary"]["c"]), (6, 0))
            self.assertAlmostEqual(out[lin]["primary"]["p_one_sided"], 1 / 64)
            self.assertTrue(out[lin]["chain_consistency"])
        out = stagef_read.primary(self.rows(comp, failed={("r1", 1, 0, "x"), ("r1", 1, 5, "o"),
                                                          ("r1", 2, 7, "x"), ("r1", 2, 7, "o")}))["lineages"]["r1"]
        self.assertEqual(out["primary"]["excluded"], 3)
        self.assertEqual(out["failed_replays"], {"x": 2, "o": 2})
        self.assertEqual((out["primary"]["b"], out["primary"]["c"]), (5, 0))
        self.assertEqual((out["sensitivity"]["b"], out["sensitivity"]["c"]), (5, 2))

    def test_primary_refuses_missing_or_unscheduled_cases(self):
        rows = self.rows(set())
        with self.assertRaisesRegex(SystemExit, "not played by both arms"):
            stagef_read.primary(rows[:-1])
        with self.assertRaisesRegex(SystemExit, "scheduled cases have no rows"):
            stagef_read.primary(rows[:-2])
        with self.assertRaisesRegex(SystemExit, "rows are not scheduled"):
            stagef_read.primary(rows + [dict(r, case="extra") for r in rows[:2]])

    def test_holm(self):
        self.assertEqual(stagef_read.holm({"a": 0.01, "b": 0.015, "c": 0.04, "d": 0.5}),
                         {"a": True, "b": True, "c": False, "d": False})
        self.assertEqual(stagef_read.holm({"a": 0.016, "b": 0.001, "c": 0.9, "d": 0.9}),
                         {"a": True, "b": True, "c": False, "d": False})

    def transfer_rows(self):
        rows = []
        for lin, i, case, cell in sorted(stagef_read.scheduled_transfer()):
            k = sum(map(ord, case))
            for arm in ("x", "o"):
                rows.append({"lineage": lin, "pair": i, "arm": arm, "case": case, "cell": cell,
                             "certified_opportunities": 1, "missed": 1 if arm == "o" or k % 5 == 0 else 0})
        return rows

    def test_transfer(self):
        rows = self.transfer_rows()
        out = stagef_read.transfer(rows, permutations=500, bootstraps=50)
        self.assertEqual(out, stagef_read.transfer(rows, permutations=500, bootstraps=50))     # seeded
        for v in out["cells"].values():
            self.assertLess(v["conjunction_p"], 0.01)
            self.assertTrue(v["holm_rejects"])
            self.assertEqual(v["lineages"]["r1"]["games_per_arm"], 160)
        with self.assertRaisesRegex(SystemExit, "scheduled cases have no rows"):
            stagef_read.transfer(rows[:-2], permutations=10, bootstraps=10)

    def test_panel(self):
        rows = [{"lineage": lin, "pair": i, "arm": arm, "state": f"s{k}", "p_self": 0.1 if arm == "x" else 0.0}
                for lin in ("r1", "r2") for i in (1, 2) for k in range(5) for arm in ("x", "o")]
        out = stagef_read.panel(rows, bootstraps=100)["pairs"]
        self.assertAlmostEqual(out["r1-p1"]["x_minus_o"], 0.1)
        self.assertEqual(len(out), 4)


if __name__ == "__main__":
    unittest.main()
