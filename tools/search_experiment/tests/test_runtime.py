import json
import ctypes
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import launch
import analyze
import certify_qualification
import sequential_driver


class QualificationTests(unittest.TestCase):
    @unittest.skipUnless(os.name == "nt", "actual Windows delete-sharing contention")
    def test_atomic_publish_waits_for_windows_reader_and_preserves_persistent_errors(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "state.json"
            launch.publish(path, {"old": True})
            kernel = ctypes.WinDLL("kernel32", use_last_error=True)
            kernel.CreateFileW.argtypes = [ctypes.c_wchar_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_void_p,
                                          ctypes.c_ulong, ctypes.c_ulong, ctypes.c_void_p]
            kernel.CreateFileW.restype = ctypes.c_void_p
            kernel.CloseHandle.argtypes = [ctypes.c_void_p]
            handle = kernel.CreateFileW(str(path), 0x80000000, 3, None, 3, 0, None)
            self.assertNotEqual(handle, ctypes.c_void_p(-1).value)
            released = threading.Event()

            def release():
                time.sleep(.12)
                kernel.CloseHandle(handle)
                released.set()

            thread = threading.Thread(target=release)
            thread.start()
            try:
                launch.publish(path, {"new": True})
            finally:
                thread.join()
            self.assertTrue(released.is_set())
            self.assertEqual(launch.load(path), {"new": True})
            denied = PermissionError("persistent sharing denial")
            denied.winerror = 5
            with patch.object(launch.os, "replace", side_effect=denied) as replace, patch.object(launch.time, "sleep"):
                with self.assertRaises(PermissionError):
                    launch.publish(path, {"lost": True})
                self.assertEqual(replace.call_count, 20)
            self.assertEqual(launch.load(path), {"new": True})

    def test_controller_helper_large_output_is_retained_without_pipe_wait(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "helper.log"
            code, cpu = sequential_driver.logged_command(
                [sys.executable, "-c", "import sys;sys.stdout.write('x'*131072);sys.stderr.write('done')"],
                path, launch.ProcessClock)
            self.assertEqual(code, 0)
            self.assertGreaterEqual(cpu, 0)
            self.assertEqual(path.stat().st_size, 131076)

    def test_output_accounting_tolerates_only_disappearance(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            temp = root / "state.json.tmp"
            temp.write_bytes(b"abc")
            original = Path.stat

            def disappear(path, *args, **kwargs):
                if path == temp:
                    temp.unlink()
                    raise FileNotFoundError(str(temp))
                return original(path, *args, **kwargs)

            with patch.object(Path, "stat", disappear):
                self.assertEqual(launch.output_bytes(root), 0)
            (root / "retained.json").write_bytes(b"abcd")
            self.assertEqual(launch.output_bytes(root), 4)
            with patch.object(Path, "stat", side_effect=PermissionError("denied")):
                with self.assertRaises(PermissionError):
                    launch.output_bytes(root)

    def calibrated_spec(self, directory):
        root = Path(directory)
        binary, model, calibration = (root / name for name in ("binary", "model", "calibration.json"))
        binary.write_bytes(b"native")
        model.write_bytes(b"frozen model")
        launch.publish(calibration, {"schema": "search-sharing-allocation-cost-calibration/v1", "quality_read": False})
        asset = lambda path: {"path": str(path), "sha256": launch.digest(path)}
        spec = {"schema": launch.SCHEMA, "host": launch.platform.node(), "binary": asset(binary),
                "host_slots": asset(HERE.parents[1] / "python/tools/host_slots_v1.py"), "core_pool": "0-1",
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

    def test_representative_cap_requires_explicit_abc_mode(self):
        with tempfile.TemporaryDirectory() as directory:
            spec = self.calibrated_spec(directory)
            spec["qualification_cpu_cap_seconds"] = 2400
            with patch.object(launch, "storage_guard"):
                with self.assertRaisesRegex(ValueError, "qualification CPU cap"):
                    launch.validate(spec, 1, "qualify")
                spec["representative_qualification"] = True
                with self.assertRaisesRegex(ValueError, "requires ABC"):
                    launch.validate(spec, 1, "qualify")
                spec["workload"] = {"arms": "ABC", "evaluation": {"enabled": True}}
                launch.validate(spec, 1, "qualify")
                spec["qualification_cpu_cap_seconds"] = 2401
                with self.assertRaisesRegex(ValueError, "qualification CPU cap"):
                    launch.validate(spec, 1, "qualify")
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

    def test_actual_core_pool_drift_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            spec = self.calibrated_spec(directory)
            with self.assertRaisesRegex(ValueError, "actual allocation differs"):
                launch.allocation(spec, [2, 3])
            paths = []
            for n, cores in ((1, [0, 1]), (2, [2, 3])):
                path = Path(directory) / f"pool{n}.json"
                launch.publish(path, {"workers": n, "compatible": {}, "error": None,
                                      "allocation": {"cores": cores, "topology": []}})
                paths.append(path)
            with self.assertRaisesRegex(ValueError, "same observed core pool"):
                launch.qualify(paths, Path(directory) / "receipt.json")

    def test_legacy_certification_preserves_sources_and_rejects_claim_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = self.calibrated_spec(directory)
            target.update(commit="same-native", runtime_commit="new-guard")
            guard = root / "guard.py"
            guard.write_bytes(b"old guard")
            asset = lambda path: {"path": str(path), "sha256": launch.digest(path)}
            target["host_reservation"] = asset(HERE.parents[1] / "python/tools/host_reservation_v1.py")
            old = json.loads(json.dumps(target))
            old.pop("core_pool")
            old.update(runtime_commit="old-guard", runtime_helper=asset(guard))
            old["inputs"].append(old["runtime_helper"])
            source = root / "source.json"
            launch.publish(source, old)
            guard.write_bytes(b"new pool guard")
            target["runtime_helper"] = asset(guard)
            target["inputs"].append(target["runtime_helper"])
            manifest = root / "bound.json"
            launch.publish(manifest, target)
            paths = []
            for workers in (1, 2):
                label = f"legacy{workers}"
                folder = root / label
                folder.mkdir()
                rows = []
                for number in range(2):
                    output = folder / f"{number}.jsonl"
                    output.write_bytes(b"immutable native output")
                    rows.append({"id": str(number + 1), "primary_sha256": str(number), "exit_code": 0,
                                 "affinity_core": number, "output": str(output), "output_sha256": launch.digest(output)})
                batch = folder / "completion.json"
                launch.publish(batch, {"label": label, "workers": workers, "compatible": launch.compatibility(old),
                                      "error": None, "results": rows, "peak_active_workers": workers,
                                      "wall_seconds": 2 / workers, "cpu_seconds": 1, "cores_claimed": 2})
                log = root / f"DISPATCH-{label}.log"
                log.write_text(json.dumps({"claimed": label, "cores": "0-1"}))
                launch.publish(root / f"DISPATCH-{label}.json", {"state": "completed", "claim_release_verified": True,
                    "completion": str(batch), "manifest": asset(source), "claim_id": label, "log": str(log)})
                paths.append(batch)
            originals = [launch.digest(path) for path in paths]
            with patch.object(launch, "storage_guard"):
                receipt = certify_qualification.certify(manifest, paths, root / "certified.json")
                self.assertEqual(receipt["compatible"], launch.compatibility(target))
                self.assertEqual([launch.digest(path) for path in paths], originals)
                self.assertTrue(all(row["legacy_metadata_added"] for row in receipt["certification_provenance"]))
                with self.assertRaisesRegex(ValueError, "destination already exists"):
                    certify_qualification.certify(manifest, paths, paths[0])
                self.assertEqual([launch.digest(path) for path in paths], originals)
                log.write_text(json.dumps({"claimed": "legacy2", "cores": "2-3"}))
                with self.assertRaisesRegex(ValueError, "outer claim differs"):
                    certify_qualification.certify(manifest, paths, root / "bad.json")

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
                                      "allocation": {"cores": [0, 1], "topology": []},
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
