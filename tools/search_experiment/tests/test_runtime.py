import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import launch
import analyze


class QualificationTests(unittest.TestCase):
    def calibrated_spec(self, directory):
        root = Path(directory)
        binary, model, calibration = (root / name for name in ("binary", "model", "calibration.json"))
        binary.write_bytes(b"native")
        model.write_bytes(b"frozen model")
        launch.publish(calibration, {"schema": "search-sharing-allocation-cost-calibration/v1", "quality_read": False})
        asset = lambda path: {"path": str(path), "sha256": launch.digest(path)}
        spec = {"schema": launch.SCHEMA, "host": launch.platform.node(), "binary": asset(binary),
                "inputs": [asset(model)], "storage": {"volume": "D"}, "eligible_workers": 2, "cores_claimed": 2,
                "cpu_cap_seconds": 43200, "external_cpu_seconds": 2000, "projected_output_bytes": 0,
                "qualification_cpu_cap_seconds": 1200,
                "workload": {"arms": "A", "evaluation": {"enabled": False}},
                "jobs": [{"id": str(n), "group": str(n), "max_wall_seconds": 600,
                          "max_cpu_seconds": 300, "expected_rows": 1, "argv": [],
                          "env": {"PROFILE_ARM": "A", "PROFILE_EVALUATE": "0",
                                  "BUDGET_TOTAL": "16000", "PROFILE_ANCHOR": "16000"}} for n in (1, 2)]}
        receipt = root / "receipt.json"
        rates = {str(n): {"completed_jobs": 2, "wall_seconds": 2 / n, "completed_per_second": n,
                          "peak_active_workers": n} for n in (1, 2)}
        launch.publish(receipt, {"compatible": launch.compatibility(spec), "parity": True, "rates": rates})
        spec["qualification"] = asset(receipt)
        spec["calibration"] = asset(calibration)
        spec["inputs"].append(spec["calibration"])
        return spec

    def test_derived_calibration_preserves_identity_but_changed_model_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            spec = self.calibrated_spec(directory)
            launch.qualification_guard(spec, 2)
            model = Path(spec["inputs"][0]["path"])
            model.write_bytes(b"changed model")
            spec["inputs"][0]["sha256"] = launch.digest(model)
            with self.assertRaisesRegex(ValueError, "incompatible"):
                launch.qualification_guard(spec, 2)

    def test_changed_calibration_bytes_fail_independent_hash_check(self):
        with tempfile.TemporaryDirectory() as directory:
            spec = self.calibrated_spec(directory)
            Path(spec["calibration"]["path"]).write_text("changed calibration")
            with patch.object(launch, "storage_guard"):
                with self.assertRaisesRegex(ValueError, "input hash differs"):
                    launch.validate(spec, 2, "calibrate")

    def test_substantial_calibration_requires_its_own_completed_work_qualification(self):
        with tempfile.TemporaryDirectory() as directory:
            spec = self.calibrated_spec(directory)
            with patch.object(launch, "storage_guard"):
                launch.validate(spec, 2, "calibrate")
                with self.assertRaisesRegex(ValueError, "ABC frozen-evaluation"):
                    launch.validate(spec, 2, "measure")
                saved = spec.pop("qualification")
                with self.assertRaisesRegex(ValueError, "qualification required"):
                    launch.validate(spec, 2, "calibrate")
                spec["qualification"] = saved
                receipt = Path(saved["path"])
                quality_receipt = launch.load(receipt)
                quality_spec = {**spec, "workload": {"arms": "ABC", "evaluation": {"enabled": True}}}
                quality_receipt["compatible"] = launch.compatibility(quality_spec)
                launch.publish(receipt, quality_receipt)
                saved["sha256"] = launch.digest(receipt)
                with self.assertRaisesRegex(ValueError, "incompatible"):
                    launch.validate(spec, 2, "calibrate")
                spec["workload"] = {"arms": "ABC", "evaluation": {"enabled": True}}
                with self.assertRaisesRegex(ValueError, "cost-only A"):
                    launch.validate(spec, 2, "calibrate")

    def test_missing_parallel_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            spec = {"binary": {"sha256": "binary"}, "workload": {"mode": "search"},
                    "inputs": [], "storage": {"volume": "D"}, "eligible_workers": 2, "cores_claimed": 1}
            receipt = Path(directory) / "receipt.json"
            launch.publish(receipt, {"compatible": launch.compatibility(spec), "parity": True,
                                     "rates": {"1": {"completed_jobs": 1, "wall_seconds": 1, "completed_per_second": 1}}})
            spec["qualification"] = {"path": str(receipt), "sha256": launch.digest(receipt)}
            with self.assertRaisesRegex(ValueError, "serial and parallel"):
                launch.qualification_guard(spec, 1)

    def test_fastest_eligible_and_changed_workload(self):
        with tempfile.TemporaryDirectory() as directory:
            spec = {"binary": {"sha256": "binary"}, "workload": {"mode": "search"},
                    "inputs": [], "storage": {"volume": "D"}, "eligible_workers": 2, "cores_claimed": 1}
            receipt = Path(directory) / "receipt.json"
            rates = {str(n): {"completed_jobs": 4, "wall_seconds": 4 / n, "completed_per_second": n,
                              "peak_active_workers": n} for n in (1, 2, 4)}
            launch.publish(receipt, {"compatible": launch.compatibility(spec), "parity": True, "rates": rates})
            spec["qualification"] = {"path": str(receipt), "sha256": launch.digest(receipt)}
            launch.qualification_guard(spec, 2)
            with self.assertRaisesRegex(ValueError, "fastest"):
                launch.qualification_guard(spec, 1)
            spec["workload"]["mode"] = "changed"
            with self.assertRaisesRegex(ValueError, "incompatible"):
                launch.qualification_guard(spec, 2)
            spec["workload"]["mode"] = "search"
            rates["2"]["peak_active_workers"] = 1
            launch.publish(receipt, {"compatible": launch.compatibility(spec), "parity": True, "rates": rates})
            spec["qualification"]["sha256"] = launch.digest(receipt)
            with self.assertRaisesRegex(ValueError, "parallel overlap"):
                launch.qualification_guard(spec, 2)

    def test_parallel_primary_mismatch_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = []
            for n in (1, 2):
                path = Path(directory) / f"{n}.json"
                launch.publish(path, {"workers": n, "compatible": {}, "error": None,
                                      "wall_seconds": 1, "cpu_seconds": .1,
                                      "results": [{"id": "root", "primary_sha256": str(n)}]})
                paths.append(path)
            with self.assertRaisesRegex(ValueError, "outputs differ"):
                launch.qualify(paths, Path(directory) / "receipt.json")

    def test_ledger_exclusive(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "ledger.jsonl"
            with launch.ledger_owner(path):
                with self.assertRaises(FileExistsError):
                    with launch.ledger_owner(path):
                        pass
            self.assertFalse(Path(str(path) + ".lock").exists())

    def test_interrupted_attempt_budget_retained(self):
        rows = [{"label": "old", "cpu_seconds": 0, "reserved_cpu_seconds": 1200},
                {"label": "old", "cpu_seconds": 100},
                {"label": "new", "cpu_seconds": 0, "reserved_cpu_seconds": 500}]
        self.assertEqual(launch.accounted_cpu(rows), 600)

    @unittest.skipUnless(os.name == "nt", "Windows final CPU handle")
    def test_final_cpu_after_exit_and_power(self):
        child = subprocess.Popen([sys.executable, "-c", "sum(range(2000000))"], creationflags=0x4)
        clock = launch.ProcessClock(child)
        try:
            power = clock.qualify_power()
            self.assertEqual(power["state_mask"] & 1, 0)
            import ctypes
            nt = ctypes.WinDLL("ntdll")
            nt.NtResumeProcess.argtypes = [ctypes.c_void_p]
            self.assertEqual(nt.NtResumeProcess(int(child._handle)), 0)
            child.wait(timeout=10)
            self.assertGreater(clock.read(), 0)
            self.assertEqual(clock.read(), clock.read())
        finally:
            if child.poll() is None:
                child.kill()
                child.wait()
            clock.close()

    @unittest.skipUnless(sys.platform == "linux", "Linux wait4 accounting")
    def test_linux_wait4_final_cpu_and_affinity(self):
        import time
        core = min(os.sched_getaffinity(0))
        def before_exec():
            os.sched_setaffinity(0, {core})
            os.nice(max(0, 10 - os.nice(0)))
        code = "import os;assert len(os.sched_getaffinity(0))==1;assert os.nice(0)>=10;sum(range(2000000))"
        child = subprocess.Popen([sys.executable, "-c", code], preexec_fn=before_exec)
        clock = launch.ProcessClock(child)
        while clock.poll() is None:
            time.sleep(.01)
        self.assertEqual(child.returncode, 0)
        self.assertGreater(clock.read(), 0)
        self.assertEqual(clock.read(), clock.read())

    @unittest.skipUnless(sys.platform == "linux", "Linux killed-child wait4 accounting")
    def test_linux_stop_retains_final_cpu(self):
        child = subprocess.Popen([sys.executable, "-c", "while True: sum(range(1000))"])
        clock = launch.ProcessClock(child)
        import time
        time.sleep(.05)
        cpu = clock.stop()
        self.assertLess(child.returncode, 0)
        self.assertGreaterEqual(cpu, 0)
        self.assertEqual(cpu, clock.read())


def row(arm, budget, wins, unknown=False):
    return {"kind": "search_allocation_experiment", "id": "root", "position": "middle", "seed": "seed",
            "budget": budget, "arm": arm, "primary_sha256": arm, "evaluation_sha256": arm,
            "root_obs_sha256": "obs", "root_menu_sha256": "menu", "bank_binding_sha256": "bank",
            "key_mode": "repaired", "interchange": True, "teacher": {"supported": True},
            "evaluation": {"supported": True, "continuation": "frozen-search-tree", "scheduled": 2, "seed_namespace": "fresh",
                           "samples": [{"index": i, "world_seed": i, "outcome": {"w": bool(wins), "unknown": unknown}}
                                       for i in range(2)]}}


class AnalysisTests(unittest.TestCase):
    def test_world_mismatch_refused(self):
        a, b = row("A", 64000, 0), row("B", 64000, 1)
        b["evaluation"]["samples"][0]["world_seed"] = 77
        with self.assertRaisesRegex(ValueError, "not paired"):
            analyze.paired(a, b, 2)

    def test_primary_anchor_ignores_secondary_direction(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rows.jsonl"
            rows = [row(arm, budget, (arm != "A") if budget == 64000 else (arm == "A"))
                    for budget in (16000, 64000) for arm in "ABC"]
            path.write_text("\n".join(json.dumps(r) for r in rows))
            result = analyze.analyze([path], [{"id": "root", "position": "middle"}], ["seed"], [16000, 64000],
                                     worlds=2, repetitions=100)
            self.assertEqual(result["contrasts"]["B-A"]["mean_difference"], 1)
            self.assertFalse(result["contrasts"]["B-A"]["advances_quality"], "missing final process costs refuse advancement")

    def test_unknown_never_advances(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rows.jsonl"
            path.write_text("\n".join(json.dumps(row(arm, 64000, arm != "A", unknown=arm == "C")) for arm in "ABC"))
            result = analyze.analyze([path], [{"id": "root", "position": "middle"}], ["seed"], [64000], worlds=2,
                                     repetitions=100)
            self.assertEqual(result["verdict"], "inconclusive")
            self.assertFalse(result["contrasts"]["C-A"]["advances_quality"])


if __name__ == "__main__":
    unittest.main()
