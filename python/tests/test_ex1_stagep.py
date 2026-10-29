"""EX1 Stage P launcher (tools/ex1/stagep.py): plan pins and refusals, the throughput guard, the #336 quarantine,
the per-chain USD meter and localization. The training binary is a Python stand-in."""
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
delay = float(Path(cfg["output_directory"]).parents[2].joinpath("DELAY").read_text() or 0)
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

    def throughput_record(self, host, **changes):
        plan = json.loads(self.plan.read_text())
        record = {"host": host, "hostname": socket.gethostname(), "binary_sha256": plan["binary"]["sha256"],
                  "cores": 8, "workers": 10, "exit": 0, "state_equal": True, "state_required": True}
        record.update(changes)
        stagep.write(self.root / "throughput" / f"THROUGHPUT-{host}.json", record)

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
        self.assertEqual(len(copy["files"]), 6)   # config, parent, checkpoint, base, qualification, binary


if __name__ == "__main__":
    unittest.main()
