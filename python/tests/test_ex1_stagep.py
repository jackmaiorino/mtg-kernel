"""EX1 Stage P launcher (tools/ex1/stagep.py): plan pins and refusals, the throughput guard, the #336 quarantine,
the per-chain USD meter, localization and packing. The training binary is a Python stand-in."""
import hashlib
import json
import socket
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools" / "ex1"))
import stagep  # noqa: E402

FAKE = '''
import json, sys, time
from pathlib import Path
cfg = json.loads(Path(sys.argv[1]).read_text())
root = Path(cfg["output_directory"])
delay = float(next(p for p in Path(cfg["output_directory"]).parents if (p / "DELAY").exists())
              .joinpath("DELAY").read_text() or 0)
for i in range(len(cfg["iterations"])):
    d = root / "iterations" / f"{i:06d}"
    if (d / "complete.json").exists():
        continue
    d.mkdir(parents=True, exist_ok=True)
    (d / "attempt-000000").mkdir(exist_ok=True)
    time.sleep(delay)
    c = d / "collection.json"; c.write_text(json.dumps({"collection_elapsed_seconds": 1.5}))
    u = d / "update.json"; u.write_text(json.dumps({"after_state_sha256": "s" * 64}))
    (d / "complete.json").write_text(json.dumps({"iteration": i, "collection": {"path": c.as_posix()},
        "update": {"path": u.as_posix()}, "driver_process_cpu_seconds": 1.0}))
(root / "completion.json").write_text("{}")
'''


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class StagePTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        t = Path(self.tmp.name)
        self.root = t / "stagep"
        self.root.mkdir()
        (self.root / "DELAY").write_text("0")
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
        self.qualification = t / "QUALIFICATION.json"
        self.qualification.write_text(json.dumps({
            "design": {"version": "v3.3"},
            "engine": {"binaries": {"native_expanded_training_run_v1": sha(self.binary)}},
            "checks": {"resume_determinism": {"pass": True, "states": ["s" * 64]}, "labels": {"pass": True}},
            "calibration": {"r2": {"k1": {"c_star": 0.0387}, "k2": {"pass": True}},
                            "r1": {"k1": {"c_star": 0.047}, "k2": {"pass": True}}}}))
        self.plan = t / "PLAN.json"
        self.free = 500e9
        usage = mock.patch.object(stagep.shutil, "disk_usage",
                                  side_effect=lambda _: type("U", (), {"free": self.free})())
        usage.start()
        self.addCleanup(usage.stop)

    def tearDown(self):
        self.tmp.cleanup()

    def make(self, places=None, binary=None):
        places = places or {"r2-c1": "desk:6:0:0", "r2-c2": "desk:6:0:0", "r1-c1": "desk:6:0:0",
                            "r1-c2": "pod:8:0.24:6.0"}
        argv = ["plan", "--qualification", str(self.qualification), "--binary", str(binary or self.binary),
                "--root", self.root.as_posix(), "--out", str(self.plan)]
        for lineage, (dose, base) in self.inputs.items():
            argv += ["--parent", f"{lineage}={dose}", "--base", f"{lineage}={base}"]
        for chain, place in places.items():
            argv += ["--place", f"{chain}={place}"]
        stagep.main(argv)
        return json.loads(self.plan.read_text())

    def throughput_record(self, host, name=None, **changes):
        plan = json.loads(self.plan.read_text())
        record = {"host": host, "hostname": socket.gethostname(), "binary_sha256": plan["binary"]["sha256"],
                  "cores": 8, "workers": 10, "exit": 0, "state_equal": True, "state_required": True}
        record.update(changes)
        stagep.write(self.root / "throughput" / (name or f"THROUGHPUT-{host}.json"), record)

    PACKED = {"r2-c1": "desk:8:0:0:g", "r2-c2": "desk:8:0:0:g", "r1-c1": "desk:6:0:0", "r1-c2": "pod:8:0.24:6.0"}

    def test_a_pack_group_shares_host_and_cores(self):
        plan = self.make(self.PACKED)
        self.assertEqual({n: (c.get("group"), c.get("pack")) for n, c in plan["chains"].items()},
                         {"r2-c1": ("g", 2), "r2-c2": ("g", 2), "r1-c1": (None, None), "r1-c2": (None, None)})
        with self.assertRaisesRegex(SystemExit, "one host and core count"):
            self.make(dict(self.PACKED, **{"r2-c2": "desk:6:0:0:g"}))
        with self.assertRaisesRegex(SystemExit, "one host and core count"):
            self.make(dict(self.PACKED, **{"r2-c2": "pod:8:0:0:g"}))

    def test_a_pack_group_needs_a_record_at_its_own_pack_size(self):
        self.make(self.PACKED)
        self.throughput_record("desk")                     # K = 1 records stay valid for unpacked chains
        with self.assertRaisesRegex(SystemExit, "no throughput record for desk at pack 2"):
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r2-c1", "--chain", "r2-c2"])
        self.throughput_record("desk", name="THROUGHPUT-desk-k2.json", pack=3)
        with self.assertRaisesRegex(SystemExit, "recorded at pack 3, not 2"):
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r2-c1", "--chain", "r2-c2"])
        self.throughput_record("desk", name="THROUGHPUT-desk-k2.json", pack=2, state_equal=False)
        with self.assertRaisesRegex(SystemExit, "state differs"):
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r2-c1", "--chain", "r2-c2"])
        guard_k1 = stagep.guard(json.loads(self.plan.read_text()), "r1-c1",
                                json.loads(self.plan.read_text())["chains"]["r1-c1"])
        self.assertNotIn("pack", guard_k1)                 # an old record without the field reads as K = 1

    def test_a_pack_group_runs_all_its_chains_together_in_one_claim(self):
        plan = self.make(self.PACKED)
        self.throughput_record("desk", name="THROUGHPUT-desk-k2.json", pack=2)
        with self.assertRaisesRegex(SystemExit, "runs all its chains together"):
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r2-c1"])
        with self.assertRaisesRegex(SystemExit, "only the chains of one pack group"):
            self.throughput_record("desk")
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r1-c1", "--chain", "r2-c1"])
        with self.assertRaises(SystemExit) as stop:
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r2-c1", "--chain", "r2-c2", "--fast-poll"])
        self.assertEqual(stop.exception.code, 0)
        for name in ("r2-c1", "r2-c2"):
            self.assertTrue((self.root / "chains" / name / "DONE.json").exists())
            self.assertEqual(stagep.completed(plan["chains"][name]["native"]), 300)
        events = [json.loads(line) for line in (self.root / "LAUNCH.jsonl").read_text().splitlines()]
        starts = [e for e in events if e["event"] == "start"]
        self.assertEqual(sorted(e["chain"] for e in starts), ["r2-c1", "r2-c2"])
        self.assertTrue(all("_members" in e["cmd"] for e in starts))

    def test_members_checks_again_and_runs_only_inside_its_claim(self):
        self.make(self.PACKED)
        results = str(self.root / "members.json")
        argv = ["_members", "--plan", str(self.plan), "--results", results, "--chain", "r2-c1", "--chain", "r2-c2"]
        with self.assertRaisesRegex(SystemExit, "no throughput record"):
            stagep.main(argv)
        self.throughput_record("desk", name="THROUGHPUT-desk-k2.json", pack=2)
        with self.assertRaisesRegex(SystemExit, "runs all its chains together"):
            stagep.main(argv[:-2])
        plan = json.loads(self.plan.read_text())
        plan["hosts"]["desk"]["wrapper"] = ["wrapper"]
        self.plan.write_text(json.dumps(plan))
        with mock.patch.dict(stagep.os.environ, {}, clear=True), \
                self.assertRaisesRegex(SystemExit, "only inside its host_slots claim"):
            stagep.main(argv)
        self.assertFalse(Path(results).exists())

    def test_a_group_exit_is_nonzero_when_any_member_fails(self):
        plan = self.make(self.PACKED)
        results = self.root / "members.json"
        for codes, want in (({"r2-c1": -9, "r2-c2": 0}, -9), ({"r2-c1": 0, "r2-c2": 3}, 3), ({"r2-c1": 0, "r2-c2": 0}, 0)):
            write = f"import json; json.dump({json.dumps(codes)}, open({str(results)!r}, 'w'))"
            with self.subTest(codes=codes):
                got = stagep.supervise(plan, ["r2-c1", "r2-c2"], [sys.executable, "-c", write], True, results)
                self.assertEqual(got, want)
        self.assertFalse((self.root / "chains" / "r2-c1" / "DONE.json").exists())

    def test_stop_acts_while_the_group_claim_waits(self):
        plan = self.make(self.PACKED)
        (self.root / "STOP").write_text("")
        started = stagep.time.time()
        got = stagep.supervise(plan, ["r2-c1", "r2-c2"], [sys.executable, "-c", "import time; time.sleep(60)"],
                               True, self.root / "members.json")
        self.assertEqual(got, stagep.EXIT_STOP)
        self.assertLess(stagep.time.time() - started, 30)

    def test_a_pack_group_refuses_a_paid_host(self):
        with self.assertRaisesRegex(SystemExit, "paid host"):
            self.make(dict(self.PACKED, **{"r2-c1": "pod:8:1:3:g", "r2-c2": "pod:8:1:3:g"}))

    def test_packed_throughput_checks_every_copy_and_records_the_group_rate(self):
        self.make(self.PACKED)
        stagep.main(["throughput", "--plan", str(self.plan), "--host", "desk"])
        single = json.loads((self.root / "throughput" / "THROUGHPUT-desk.json").read_text())
        self.assertEqual((single["pack"], single["state_equal"]), (1, True))
        stagep.main(["throughput", "--plan", str(self.plan), "--host", "desk", "--pack", "2", "--repeats", "2"])
        record = json.loads((self.root / "throughput" / "THROUGHPUT-desk-k2.json").read_text())
        self.assertEqual((record["pack"], record["repeats"], record["completed_updates"]), (2, 2, 4))
        self.assertEqual(len(record["copies"]), 2)
        self.assertTrue(all(len(c["updates"]) == 2 and c["state_equal"] for c in record["copies"]))
        self.assertTrue(record["state_equal"])
        self.assertGreater(record["group_updates_per_hour"], 0)
        self.assertEqual(record["copies"][1]["delay_seconds"], single["wall_seconds"] / 2)
        self.assertEqual(json.loads((self.root / "throughput" / "THROUGHPUT-desk.json").read_text()), single)
        stagep.guard(json.loads(self.plan.read_text()), "r2-c1", json.loads(self.plan.read_text())["chains"]["r2-c1"])

    def test_packed_throughput_fails_when_one_copy_differs(self):
        self.make(self.PACKED)
        real = stagep.qual.iterations
        calls = []

        def one_bad(native):
            rows = list(real(native))
            calls.append(native)
            if len(calls) == 2:                            # the second copy's first update
                rows = [(r, dict(u, after_state_sha256="x" * 64), c) for r, u, c in rows]
            return iter(rows)

        with mock.patch.object(stagep.platform, "system", return_value="Windows"), \
                mock.patch.object(stagep.qual, "iterations", side_effect=one_bad):
            with self.assertRaisesRegex(SystemExit, "state differs"):
                stagep.main(["throughput", "--plan", str(self.plan), "--host", "desk", "--pack", "2",
                             "--repeats", "1", "--stagger", "0"])
        record = json.loads((self.root / "throughput" / "THROUGHPUT-desk-k2.json").read_text())
        self.assertEqual([c["state_equal"] for c in record["copies"]], [True, False])
        self.assertFalse(record["state_equal"])

    def test_packed_throughput_fails_when_a_copy_leaves_no_update(self):
        self.make(self.PACKED)
        real = stagep.qual.iterations
        calls = []

        def none_once(native):
            calls.append(native)
            return iter([]) if len(calls) == 2 else real(native)

        with mock.patch.object(stagep.qual, "iterations", side_effect=none_once):
            with self.assertRaisesRegex(SystemExit, "failed"):
                stagep.main(["throughput", "--plan", str(self.plan), "--host", "desk", "--pack", "2",
                             "--repeats", "1", "--stagger", "0"])
        record = json.loads((self.root / "throughput" / "THROUGHPUT-desk-k2.json").read_text())
        self.assertEqual(record["completed_updates"], 1)
        self.assertIsNone(record["group_updates_per_hour"])
        self.assertNotEqual(record["exit"], 0)
        self.assertFalse(record["state_equal"])

    def test_throughput_flags(self):
        self.make()
        for argv in (["--pack", "0"], ["--repeats", "2"], ["--stagger", "1"], ["--pack", "2", "--repeats", "0"]):
            with self.subTest(argv=argv), self.assertRaises(SystemExit) as stop, \
                    mock.patch("sys.stderr", new=__import__("io").StringIO()):
                stagep.main(["throughput", "--plan", str(self.plan), "--host", "desk", *argv])
            self.assertEqual(stop.exception.code, 2)

    def test_plan_pins_the_frozen_inputs_and_refuses_departures(self):
        plan = self.make()
        self.assertEqual(plan["chains"]["r2-c1"]["coefficient"], 0.0387)
        self.assertEqual(plan["chains"]["r1-c2"]["coefficient"], 0.047)
        self.assertEqual(plan["allotted_usd"], 6.0)
        cfg = json.loads(Path(plan["chains"]["r1-c1"]["config"]["path"]).read_text())
        self.assertEqual(len(cfg["iterations"]), 300)
        self.assertEqual(cfg["ex1"], {"run": "ex1-stagep-r1-c1", "update_offset": 0, "coefficient": 0.047,
                                      "cap": {"mode": "ratio", "kappa": 1.0}})
        self.assertEqual(cfg["preparation_workers"], 1)
        self.assertEqual(cfg["collection_workers"], 10)
        ids = {e["episode"]["id"] for it in cfg["iterations"] for e in it["episodes"]}
        self.assertEqual(len(ids), 600)
        with self.assertRaises(SystemExit):        # allotments above the ceiling
            self.make({"r2-c1": "pod:8:1:20", "r2-c2": "pod:8:1:6", "r1-c1": "desk:6:0:0", "r1-c2": "desk:6:0:0"})
        with self.assertRaises(SystemExit):        # a paid host without a budget
            self.make({"r2-c1": "pod:8:1:0", "r2-c2": "desk:6:0:0", "r1-c1": "desk:6:0:0", "r1-c2": "desk:6:0:0"})
        other = Path(self.tmp.name) / "other.py"
        other.write_text("# not the qualified binary\n")
        with self.assertRaises(SystemExit):
            self.make(binary=other)

    def test_run_refuses_without_a_matching_throughput_record(self):
        self.make()
        with self.assertRaisesRegex(SystemExit, "no throughput record"):
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r2-c1"])
        self.throughput_record("desk", hostname="elsewhere")
        with self.assertRaisesRegex(SystemExit, "recorded on elsewhere"):
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r2-c1"])
        self.throughput_record("desk", state_equal=False)
        with self.assertRaisesRegex(SystemExit, "state differs"):
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r2-c1"])
        self.throughput_record("desk", cores=2)
        with self.assertRaisesRegex(SystemExit, "fewer cores"):
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r2-c1"])

    def test_a_chain_runs_to_completion_and_reruns_an_interrupted_iteration(self):
        plan = self.make()
        self.throughput_record("desk")
        native = Path(plan["chains"]["r2-c1"]["native"])
        (native / "iterations" / "000000" / "attempt-000000").mkdir(parents=True)   # interrupted (#336)
        with self.assertRaises(SystemExit) as stop:
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r2-c1", "--fast-poll"])
        self.assertEqual(stop.exception.code, 0)
        self.assertTrue((self.root / "chains" / "r2-c1" / "DONE.json").exists())
        self.assertEqual(stagep.completed(native), 300)
        self.assertEqual(len(list((self.root / "chains" / "r2-c1" / "interrupted").iterdir())), 1)
        self.assertEqual(stagep.status(type("A", (), {"plan": str(self.plan)}))["r2-c1"]["completed"], 300)

    def test_the_meter_stops_a_paid_chain_at_its_allotment(self):
        self.make({"r2-c1": "desk:6:0:0", "r2-c2": "desk:6:0:0", "r1-c1": "desk:6:0:0",
                   "r1-c2": "pod:8:3600:0.5"})          # 1 USD per second, 0.5 USD allotted
        self.throughput_record("pod")
        (self.root / "DELAY").write_text("0.05")
        with self.assertRaises(SystemExit) as stop:
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r1-c2", "--fast-poll"])
        self.assertEqual(stop.exception.code, stagep.EXIT_BUDGET)
        truncated = json.loads((self.root / "chains" / "r1-c2" / "TRUNCATED.json").read_text())
        self.assertLess(truncated["completed_updates"], 300)
        self.assertIn("inconclusive on budget", truncated["stage_p"])
        with self.assertRaisesRegex(SystemExit, "inconclusive on budget"):
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r1-c2"])

    def test_run_refuses_below_the_reserve(self):
        self.make()
        self.throughput_record("desk")
        self.free = 70e9                      # 15.9 GB still to write plus 60 GiB is more than 70 GB
        with self.assertRaisesRegex(SystemExit, "plus the host reserve"):
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r2-c1"])
        plan = json.loads(self.plan.read_text())
        self.assertEqual({h["reserve_gib"] for h in plan["hosts"].values()}, {60.0})
        self.assertEqual(plan["byte_budget"]["cap_bytes"], 70_000_000_000)

    def test_a_changed_input_is_refused(self):
        plan = self.make()
        self.throughput_record("desk")
        Path(plan["chains"]["r2-c1"]["config"]["path"]).write_text("{}")
        with self.assertRaisesRegex(SystemExit, "chain config differs"):
            stagep.main(["run", "--plan", str(self.plan), "--chain", "r2-c1"])

    def test_localize_maps_paths_and_lists_the_files_to_copy(self):
        self.make()
        export = Path(self.tmp.name) / "export"
        prefix = Path(self.tmp.name).as_posix()
        stagep.main(["localize", "--plan", str(self.plan), "--host", "pod", "--export", str(export),
                     "--map", f"{prefix}=/workspace/ex1"])
        local = json.loads((export / "PLAN.json").read_text())
        self.assertTrue(local["chains"]["r1-c2"]["config"]["path"].startswith("/workspace/ex1/"))
        cfg_text = (export / "configs" / "r1-c2.json").read_text()
        self.assertNotIn(prefix, cfg_text)
        self.assertEqual(local["chains"]["r1-c2"]["config"]["sha256"],
                         hashlib.sha256((export / "configs" / "r1-c2.json").read_bytes()).hexdigest())
        copy = json.loads((export / "COPY.json").read_text())
        hosts = {row["host"] for row in copy["files"]}
        self.assertTrue(all(h.startswith("/workspace/ex1/") for h in hosts))
        # r1-c2's config, parent, checkpoint and base; the qualification and binary; the throughput config and the
        # r2 checkpoint it starts from.
        self.assertEqual(len(copy["files"]), 8)
        self.assertTrue(local["throughput_config"]["path"].endswith("/throughput-config.json"))

    def test_a_localized_host_runs_throughput_and_its_chain_from_copied_files(self):
        self.make()
        export = Path(self.tmp.name) / "export"
        remote = Path(self.tmp.name) / "remote"
        prefix = Path(self.tmp.name).as_posix()
        stagep.main(["localize", "--plan", str(self.plan), "--host", "pod", "--export", str(export),
                     "--map", f"{prefix}/stagep={remote.as_posix()}/stagep",
                     "--map", f"{prefix}={remote.as_posix()}/inputs"])
        copy = json.loads((export / "COPY.json").read_text())
        for row in copy["files"]:
            target = Path(row["host"])
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(Path(row["local"]).read_bytes())
        plan = Path(copy["plan"])
        plan.parent.mkdir(parents=True, exist_ok=True)
        plan.write_bytes((export / "PLAN.json").read_bytes())
        (remote / "stagep" / "DELAY").write_text("0")
        stagep.main(["throughput", "--plan", str(plan), "--host", "pod"])
        record = json.loads((remote / "stagep" / "throughput" / "THROUGHPUT-pod.json").read_text())
        self.assertTrue(record["state_equal"])
        with self.assertRaises(SystemExit) as stop:
            stagep.main(["run", "--plan", str(plan), "--chain", "r1-c2", "--fast-poll"])
        self.assertEqual(stop.exception.code, 0)
        self.assertTrue((remote / "stagep" / "chains" / "r1-c2" / "DONE.json").exists())


if __name__ == "__main__":
    unittest.main()
