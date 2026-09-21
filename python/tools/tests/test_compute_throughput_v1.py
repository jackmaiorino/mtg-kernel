import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from compute_throughput_v1 import require_choice


class ComputeLaunchTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.binary = self.put("trainer.exe", "test executable")
        self.config = self.put("config.json", {"updates": [[{}, {}]] * 200, "gpu_ordinal": 1})
        self.plan = {
            "schema": "public-training-compute-choice/v1", "planned_updates": 200,
            "inventory": {host: {"checked_at": datetime.now(timezone.utc).isoformat(),
                                  "eligible": host == "jack", "reason": "test allocation"}
                          for host in ["jack", "haleyspc", "runpod"]},
            "candidates": [self.benchmark(1, [10, 5, 5]), self.benchmark(4, [10, 2, 2])],
            "selected": "jack-4",
        }

    def put(self, name, value):
        path = self.root/name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value), encoding="utf-8")
        return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}

    def benchmark(self, workers, times):
        label = f"jack-{workers}"
        output = self.root/label/"outputs"
        request = self.put(f"{label}/request.json", {
            "config": {"updates": [[{}, {}]] * 200, "gpu_ordinal": 1}, "resume": None,
            "stop_after": 3, "collector_workers": workers, "output_directory": str(output)})
        execution = self.put(f"{label}/execution.json", {
            "binary": self.binary, "request": request, "exit_code": 0,
            "timeout": False, "seconds": sum(times)+2})
        completion = self.put(f"{label}/outputs/completion.json", {
            "first_update": 0, "next_update": 3,
            "receipts": [{"update": i, "episodes": 2, "natural_games": 2, "seconds": seconds}
                         for i, seconds in enumerate(times)]})
        outputs = {f"{i:04}/{name}": self.put(f"{label}/outputs/{i:04}/{name}", {"update": i, "name": name})
                   for i in range(3) for name in ["checkpoint.json", "optimizer.json", "episode-000.json", "episode-001.json"]}
        report = self.put(f"{label}/benchmark.json", {
            "config": self.config, "execution": execution, "completion": completion,
            "host": "jack", "outputs": outputs})
        return {"id": label, "host": "jack", "workers": workers, "benchmark": report,
                "setup_seconds": 0, "transfer_seconds": 0, "recovery_seconds": 0}

    def check(self):
        plan = self.put("choice.json", self.plan)
        return require_choice(plan["path"], self.binary["sha256"], self.config["sha256"], 200)

    def test_chooses_measured_parallel_allocation(self):
        self.assertEqual(self.check()["workers"], 4)

    def test_refuses_slower_serial_choice(self):
        self.plan["selected"] = "jack-1"
        with self.assertRaisesRegex(ValueError, "slower"):
            self.check()

    def test_serial_measurement_alone_is_not_qualification(self):
        self.plan["candidates"] = self.plan["candidates"][:1]
        self.plan["selected"] = "jack-1"
        with self.assertRaisesRegex(ValueError, "serial timing alone"):
            self.check()

    def test_wrong_faster_output_is_rejected(self):
        report = json.loads(Path(self.plan["candidates"][1]["benchmark"]["path"]).read_text())
        report["outputs"]["0000/episode-000.json"] = self.put(
            "jack-4/outputs/0000/episode-000.json", {"changed_action": True})
        self.plan["candidates"][1]["benchmark"] = self.put("jack-4/benchmark.json", report)
        with self.assertRaisesRegex(ValueError, "not byte-identical"):
            self.check()

    def test_faster_claim_cannot_borrow_other_runs_outputs(self):
        first = json.loads(Path(self.plan["candidates"][0]["benchmark"]["path"]).read_text())
        second = json.loads(Path(self.plan["candidates"][1]["benchmark"]["path"]).read_text())
        second["outputs"] = first["outputs"]
        self.plan["candidates"][1]["benchmark"] = self.put("jack-4/benchmark.json", second)
        with self.assertRaisesRegex(ValueError, "not from the measured execution"):
            self.check()

    def test_changed_binary_requires_requalification(self):
        Path(self.binary["path"]).write_text("different executable")
        with self.assertRaisesRegex(ValueError, "evidence changed"):
            self.check()

    def test_stale_inventory_requires_refresh(self):
        self.plan["inventory"]["haleyspc"]["checked_at"] = "2000-01-01T00:00:00+00:00"
        with self.assertRaisesRegex(ValueError, "refresh resource inventory"):
            self.check()

    def test_eligible_unmeasured_machine_prevents_local_fallback(self):
        self.plan["inventory"]["haleyspc"]["eligible"] = True
        with self.assertRaisesRegex(ValueError, "no throughput measurement"):
            self.check()

    def test_gpu_override_cannot_qualify_default_device_dispatch(self):
        report = json.loads(Path(self.plan["candidates"][1]["benchmark"]["path"]).read_text())
        execution = json.loads(Path(report["execution"]["path"]).read_text())
        request = json.loads(Path(execution["request"]["path"]).read_text())
        request["execution_gpu_ordinal"] = 0
        execution["request"] = self.put("jack-4/request.json", request)
        report["execution"] = self.put("jack-4/execution.json", execution)
        self.plan["candidates"][1]["benchmark"] = self.put("jack-4/benchmark.json", report)
        with self.assertRaisesRegex(ValueError, "device-aware"):
            self.check()

    def test_completion_gpu_must_match_actual_dispatch(self):
        report = json.loads(Path(self.plan["candidates"][1]["benchmark"]["path"]).read_text())
        completion = json.loads(Path(report["completion"]["path"]).read_text())
        completion["execution_gpu_ordinal"] = 0
        report["completion"] = self.put("jack-4/outputs/completion.json", completion)
        self.plan["candidates"][1]["benchmark"] = self.put("jack-4/benchmark.json", report)
        with self.assertRaisesRegex(ValueError, "completion GPU"):
            self.check()


if __name__ == "__main__":
    unittest.main()
