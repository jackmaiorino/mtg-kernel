from __future__ import annotations

import copy
from datetime import timedelta
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
LAUNCHER_PATH = ROOT / "python" / "tools" / "multirun_launcher_v1.py"
FAKE_TRAINER = Path(__file__).resolve().parent / "fixtures_multirun_fake_trainer_v1.py"

SPEC = importlib.util.spec_from_file_location("multirun_launcher_v1", LAUNCHER_PATH)
assert SPEC is not None and SPEC.loader is not None
launcher = importlib.util.module_from_spec(SPEC)
sys.modules["multirun_launcher_v1"] = launcher
SPEC.loader.exec_module(launcher)


def fake_workload(directory: Path, runs: int = 3, planned: int = 6, extra_env: dict | None = None,
                  extra_argv: list[str] | None = None) -> Path:
    raw = {
        "schema": launcher.WORKLOAD_SCHEMA,
        "adapter": "command-template-v1",
        "executable": str(FAKE_TRAINER),
        "executable_sha256": launcher.sha256_file(FAKE_TRAINER),
        "planned_updates": planned,
        "template": {
            "argv": [sys.executable, "{executable}", "--seed", "{seed}", "--out", "{output_dir}",
                     "--updates", "{planned_updates}", "--stop-after", "{stop_after}",
                     "--update-seconds", "0.15"] + (extra_argv or []),
            "env": dict(extra_env or {}),
            "output_dir": "out",
            "generation_regex": r"^update-0*(\d+)\.",
            "episodes_per_update": 64,
        },
        "runs": [{"id": f"run-{index}", "seed": 1000 + index} for index in range(runs)],
    }
    path = directory / "workload.json"
    path.write_text(json.dumps(raw))
    return path


def inventory(eligible_haleyspc: bool = False, checked_at: str | None = None) -> dict:
    moment = checked_at or launcher.iso(launcher.utc_now())
    return {"schema": launcher.INVENTORY_SCHEMA, "hosts": {
        "jack": {"eligible": True, "reason": "test host", "checked_at": moment, "detail": {}},
        "haleyspc": {"eligible": eligible_haleyspc, "reason": "offline in test", "checked_at": moment, "detail": {}},
        "runpod": {"eligible": False, "reason": "no lease authority in test", "checked_at": moment, "detail": {}},
    }}


def alloc(text: str):
    return launcher.parse_allocation(text)


class QualifiedFixture(unittest.TestCase):
    """One qualification shared by the launch tests (serial, 2-wide and 3-wide)."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.temporary = tempfile.TemporaryDirectory()
        cls.base = Path(cls.temporary.name)
        cls.workload_path = fake_workload(cls.base)
        cls.workload = launcher.load_workload(cls.workload_path)
        executors = {"jack": launcher.LocalExecutor(cls.workload)}
        cls.choice = launcher.qualify(cls.workload, cls.base / "qualification",
                                      [alloc("1@0"), alloc("2@0"), alloc("3@0")], 3, inventory(), executors,
                                      stop_on_saturation=False, per_process_mib=0.0)
        cls.choice_path = cls.base / "qualification" / "compute-choice.json"

    @classmethod
    def tearDownClass(cls) -> None:
        cls.temporary.cleanup()

    def mutated_choice(self, mutate) -> Path:
        choice = copy.deepcopy(launcher.read_json(self.choice_path))
        mutate(choice)
        path = Path(tempfile.mkdtemp(dir=self.base)) / "compute-choice.json"
        launcher.write_json(path, choice)
        return path

    def assertRefused(self, path: Path, fragment: str, workload=None) -> None:
        with self.assertRaises(launcher.LaunchRefused) as caught:
            launcher.require_choice(path, workload or self.workload)
        self.assertIn(fragment, str(caught.exception))


class QualificationTests(QualifiedFixture):
    def test_receipt_measures_serial_and_parallel_and_selects_the_fastest(self) -> None:
        candidates = {c["id"]: c for c in self.choice["candidates"]}
        self.assertEqual({c["concurrency"] for c in candidates.values()}, {1, 2, 3})
        self.assertTrue(all(c["status"] == "qualified" for c in candidates.values()))
        for candidate in candidates.values():
            self.assertEqual(candidate["episodes"], 3 * 3 * 64)
            self.assertTrue(all(entry["byte_identical"] for entry in candidate["per_run"].values()))
        fastest = min(candidates.values(), key=lambda c: c["projected_seconds"])
        self.assertEqual(self.choice["selected"], fastest["id"])
        self.assertGreater(fastest["concurrency"], 1)
        self.assertEqual(launcher.require_choice(self.choice_path, self.workload)["id"], fastest["id"])

    def test_every_qualification_process_gets_a_prefix_ticket(self) -> None:
        legs = [self.base / "qualification" / "serial-golden", self.base / "qualification" / "serial-repeat"]
        legs += [self.base / "qualification" / c["id"] for c in self.choice["candidates"] if c["concurrency"] > 1]
        for leg in legs:
            tickets = sorted(leg.glob("*.ticket.json"))
            self.assertTrue(tickets, leg)
            for path in tickets:
                ticket = launcher.read_json(path)
                self.assertEqual((ticket["kind"], ticket["stop_after_generation"], ticket["compute_choice_sha256"]),
                                 ("qualification", 3, None))
                invocation = launcher.read_json(leg / ticket["run_id"] / "invocation.json")
                self.assertEqual(ticket["seed"], next(r.seed for r in self.workload.runs if r.id == ticket["run_id"]))
                self.assertIn("--stop-after", invocation["argv"])

    def test_launch_writes_one_manifest_entry_per_run_and_audits_the_golden_prefix(self) -> None:
        root = self.base / "launch"
        manifest = launcher.launch(self.workload, self.choice_path, root,
                                   {"jack": launcher.LocalExecutor(self.workload)})
        self.assertEqual(manifest["status"], "complete")
        self.assertEqual(set(manifest["runs"]), {"run-0", "run-1", "run-2"})
        for run_id, entry in manifest["runs"].items():
            self.assertEqual(entry["completed_generation"], 6)
            self.assertTrue(entry["golden_prefix_identical"], run_id)
            run_manifest = launcher.read_json(root / "runs" / run_id / "run-manifest.json")
            self.assertEqual(run_manifest["compute_choice_sha256"], launcher.sha256_file(self.choice_path))
            ticket = launcher.read_json(root / "runs" / f"{run_id}.ticket.json")
            self.assertEqual(ticket["seed"], entry["seed"])
            self.assertEqual(ticket["executable_sha256"], self.workload.executable_sha256)
            self.assertEqual((ticket["kind"], ticket["stop_after_generation"]), ("launch", None))
            self.assertEqual(ticket["compute_choice_sha256"], launcher.sha256_file(self.choice_path))
        on_disk = launcher.read_json(root / "experiment-manifest.json")
        self.assertEqual(on_disk["allocation"], launcher.require_choice(self.choice_path, self.workload)["allocation"])
        with self.assertRaises(launcher.LaunchRefused):
            launcher.launch(self.workload, self.choice_path, root, {"jack": launcher.LocalExecutor(self.workload)})
        # A full-length serial rerun of a launched run reproduces every output byte.
        record = launcher.verify(self.workload, self.choice_path, root, ["run-1"], self.base / "verify",
                                 launcher.LocalExecutor(self.workload))
        self.assertTrue(record["runs"]["run-1"]["byte_identical"])
        self.assertEqual(record["runs"]["run-1"]["serial_completed_generation"], 6)
        ticket = launcher.read_json(self.base / "verify" / "run-1.ticket.json")
        self.assertEqual((ticket["kind"], ticket["compute_choice_sha256"]),
                         ("launch", launcher.sha256_file(self.choice_path)))
        with self.assertRaises(launcher.LaunchRefused):
            launcher.verify(self.workload, self.choice_path, root, ["run-9"], self.base / "verify-2",
                            launcher.LocalExecutor(self.workload))


class LaunchRefusalTests(QualifiedFixture):
    def test_missing_receipt(self) -> None:
        self.assertRefused(self.base / "absent.json", "no compute-choice receipt")

    def test_changed_executable(self) -> None:
        self.assertRefused(self.mutated_choice(lambda c: c.update(executable_sha256="0" * 64)), "executable changed")

    def test_changed_workload(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            other = launcher.load_workload(fake_workload(Path(directory), extra_argv=["--update-seconds", "0.1"]))
            self.assertRefused(self.choice_path, "workload", other)

    def test_changed_run_set(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            other = launcher.load_workload(fake_workload(Path(directory), runs=2))
            self.assertRefused(self.choice_path, "workload", other)

    def test_stale_inventory(self) -> None:
        stale = launcher.iso(launcher.utc_now() - timedelta(hours=25))
        path = self.mutated_choice(lambda c: c["inventory"]["hosts"]["haleyspc"].update(checked_at=stale))
        self.assertRefused(path, "refresh the resource inventory: haleyspc")

    def test_missing_host_in_inventory(self) -> None:
        self.assertRefused(self.mutated_choice(lambda c: c["inventory"]["hosts"].pop("runpod")), "RunPod")

    def test_serial_only(self) -> None:
        def serial_only(choice):
            choice["candidates"] = [c for c in choice["candidates"] if c["concurrency"] == 1]
            choice["selected"] = choice["candidates"][0]["id"]
        self.assertRefused(self.mutated_choice(serial_only), "serial timing alone is insufficient")

    def test_candidate_claimed_identical_with_a_different_digest(self) -> None:
        def forge(choice):
            selected = next(c for c in choice["candidates"] if c["id"] == choice["selected"])
            selected["per_run"]["run-1"]["digest_set_sha256"] = "f" * 64
        self.assertRefused(self.mutated_choice(forge), "digest differs from golden")

    def test_selected_is_not_the_fastest(self) -> None:
        def slower(choice):
            choice["selected"] = max((c for c in choice["candidates"] if c["status"] == "qualified"),
                                     key=lambda c: c["projected_seconds"])["id"]
        self.assertRefused(self.mutated_choice(slower), "slower than a qualified alternative")

    def test_eligible_host_without_measurement(self) -> None:
        path = self.mutated_choice(lambda c: c["inventory"]["hosts"]["haleyspc"].update(eligible=True))
        self.assertRefused(path, "no throughput measurement: haleyspc")

    def test_incomplete_benchmark(self) -> None:
        def incomplete(choice):
            selected = next(c for c in choice["candidates"] if c["id"] == choice["selected"])
            selected["episodes"] -= 64
        self.assertRefused(self.mutated_choice(incomplete), "did not complete every qualification update")

    def test_golden_outputs_changed_on_disk(self) -> None:
        golden_root = Path(launcher.read_json(self.choice_path)["golden"]["root"])
        target = golden_root / "run-2" / "out" / "update-00000002.state.bin"
        original = target.read_bytes()
        try:
            target.write_bytes(original + b"x")
            self.assertRefused(self.choice_path, "golden outputs on disk no longer match")
        finally:
            target.write_bytes(original)

    def test_cli_refuses_with_exit_code_three_before_spawning(self) -> None:
        root = self.base / "never-created"
        code = launcher.main(["launch", "--workload", str(self.workload_path), "--choice",
                              str(self.base / "absent.json"), "--root", str(root)])
        self.assertEqual(code, 3)
        self.assertFalse(root.exists())


class DeterminismGateTests(unittest.TestCase):
    def test_nondeterministic_trainer_cannot_qualify(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            workload = launcher.load_workload(fake_workload(Path(directory), runs=2,
                                                            extra_env={"FAKE_TRAINER_NOISE": "1"}))
            with self.assertRaises(launcher.LaunchRefused) as caught:
                launcher.qualify(workload, Path(directory) / "q", [alloc("1@0"), alloc("2@0")], 3, inventory(),
                                 {"jack": launcher.LocalExecutor(workload)}, per_process_mib=0.0)
            self.assertIn("serial repeat is not byte-identical", str(caught.exception))

    def test_concurrency_dependent_outputs_disqualify_the_parallel_allocation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            workload = launcher.load_workload(fake_workload(Path(directory), runs=2,
                                                            extra_env={"FAKE_TRAINER_CONTENTION": "1"}))
            choice = launcher.qualify(workload, Path(directory) / "q", [alloc("1@0"), alloc("2@0")], 3,
                                      inventory(), {"jack": launcher.LocalExecutor(workload)},
                                      per_process_mib=0.0)
            statuses = {c["concurrency"]: c["status"] for c in choice["candidates"]}
            self.assertEqual(statuses, {1: "qualified", 2: "disqualified"})
            self.assertEqual(choice["selected"], next(c["id"] for c in choice["candidates"] if c["concurrency"] == 1))
            # The serial allocation is then the only qualified choice, and the guard accepts it.
            path = Path(directory) / "q" / "compute-choice.json"
            self.assertEqual(launcher.require_choice(path, workload)["concurrency"], 1)

    def test_changed_runtime_data_requires_requalification(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            data = base / "data"
            data.mkdir()
            (data / "snapshot.bin").write_bytes(b"weights v1")
            raw = json.loads(fake_workload(base, runs=1).read_text())
            raw["data_root"] = str(data)
            (base / "workload.json").write_text(json.dumps(raw))
            workload = launcher.load_workload(base / "workload.json")
            choice = launcher.qualify(workload, base / "q", [alloc("1@0"), alloc("2@0")], 3, inventory(),
                                      {"jack": launcher.LocalExecutor(workload)}, per_process_mib=0.0)
            self.assertEqual(choice["data_tree_sha256"], workload.data_tree_sha256())
            self.assertEqual(choice["launcher_sha256"], launcher.sha256_file(LAUNCHER_PATH))
            launcher.require_choice(base / "q" / "compute-choice.json", workload)
            (data / "snapshot.bin").write_bytes(b"weights v2")
            with self.assertRaises(launcher.LaunchRefused) as caught:
                launcher.require_choice(base / "q" / "compute-choice.json", workload)
            self.assertIn("runtime data", str(caught.exception))

    def test_failed_serial_golden_stops_qualification(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            workload = launcher.load_workload(fake_workload(Path(directory), runs=1, extra_argv=["--fail"]))
            with self.assertRaises(launcher.LaunchRefused):
                launcher.qualify(workload, Path(directory) / "q", [alloc("1@0"), alloc("2@0")], 3, inventory(),
                                 {"jack": launcher.LocalExecutor(workload)}, per_process_mib=0.0)

    def test_qualification_needs_serial_and_parallel(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            workload = launcher.load_workload(fake_workload(Path(directory), runs=1))
            for candidates in ([alloc("1@0")], [alloc("2@0")]):
                with self.assertRaises(launcher.LaunchRefused):
                    launcher.qualify(workload, Path(directory) / f"q{len(candidates[0])}{candidates[0][0].capacity}",
                                     candidates, 3, inventory(), {"jack": launcher.LocalExecutor(workload)})


class UnitTests(unittest.TestCase):
    def test_parse_allocation(self) -> None:
        slots = launcher.parse_allocation("3@0+1@1+2@haleyspc:0")
        self.assertEqual([(s.host, s.device, s.capacity) for s in slots],
                         [("jack", 0, 3), ("jack", 1, 1), ("haleyspc", 0, 2)])
        self.assertEqual(launcher.concurrency(slots), 6)
        for bad in ("0@0", "1@0+2@0", "x", "1@mars:0"):
            with self.assertRaises(launcher.LaunchRefused):
                launcher.parse_allocation(bad)

    def test_projection_uses_waves(self) -> None:
        statistics = {"startup_seconds": 10.0, "steady_update_seconds": 2.0, "tail_seconds": 5.0}
        self.assertEqual(launcher.project_seconds(statistics, 8, 4, 100, 30.0), 30.0 + 2 * (10 + 200 + 5))
        self.assertEqual(launcher.project_seconds(statistics, 5, 4, 100, 0.0), 2 * 215)

    def test_gpu_fit_uses_each_device_footprint(self) -> None:
        devices = [{"index": 0, "memory_total_mib": 12282, "memory_used_mib": 2939},
                   {"index": 1, "memory_total_mib": 6144, "memory_used_mib": 9}]
        footprint = {0: 2853.0, 1: 2599.0}
        self.assertEqual(launcher.device_fits(alloc("3@0+2@1"), footprint.get, devices=devices), [])
        reasons = launcher.device_fits(alloc("4@0+3@1"), footprint.get, devices=devices)
        self.assertEqual([reason.split(":")[0] for reason in reasons], ["device 0", "device 1"])
        self.assertEqual(launcher.device_fits(alloc("1@2"), footprint.get, devices=devices), ["device 2 not present"])

    def test_idle_capacity_needs_two_consecutive_idle_windows_with_waiting_runs(self) -> None:
        def samples(pattern):
            return [{"t": 5.0 * i, "cpu_percent": cpu, "waiting_runs": waiting}
                    for i, (cpu, waiting) in enumerate(pattern)]
        idle = [(20.0, 2)] * 12
        busy = [(95.0, 2)] * 12
        drained = [(20.0, 0)] * 12
        self.assertEqual(len(launcher.idle_capacity_events(samples(idle + idle))), 1)
        self.assertEqual(launcher.idle_capacity_events(samples(idle + busy + idle)), [])
        self.assertEqual(launcher.idle_capacity_events(samples(drained + drained)), [])

    def test_pilot_adapter_owns_the_per_run_variables(self) -> None:
        adapter = launcher.NativeSciencePilotAdapterV1()
        with self.assertRaises(launcher.LaunchRefused):
            adapter.validate({"knobs": {"MULTIRUN_BASE_SEED": "1"}})
        adapter.validate({"knobs": {"MULTIRUN_WORKERS": "2"}})
        with self.assertRaises(launcher.LaunchRefused):
            adapter.validate({"knobs": {}, "arms": {"x": {"knobs": {"MULTIRUN_STORE_PARENT": "D:/elsewhere"}}}})
        raw = {"knobs": {"MULTIRUN_WORKERS": "2"},
               "arms": {"envrand": {"knobs": {"MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2": "1"}}}}
        workload = launcher.Workload(raw, adapter, Path("unused.exe"), "0" * 64, 256, [])
        env = adapter.environment(workload, launcher.RunSpec("a", 5), "D:/r/a", 1, 8)
        self.assertNotIn("MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2", env)
        armed = adapter.environment(workload, launcher.RunSpec("b", 6, "envrand"), "D:/r/b", 0, None)
        self.assertEqual((armed["MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2"], armed["MULTIRUN_WORKERS"]), ("1", "2"))
        self.assertEqual(env["MULTIRUN_RUNS"], "1")
        self.assertEqual(env["MULTIRUN_BASE_SEED"], "5")
        self.assertEqual(env["MULTIRUN_UPDATES"], "256")
        self.assertEqual(env["MULTIRUN_STOP_AFTER_GENERATION"], "8")
        self.assertEqual(env["MTG_KERNEL_PILOT_CUDA_ORDINAL"], "1")
        self.assertNotIn("MULTIRUN_STOP_AFTER_GENERATION", armed)
        # The harness honours stops only at its four-update checkpoint boundaries.
        for bad in (3, 4, 10):
            with self.assertRaises(launcher.LaunchRefused):
                adapter.validate_prefix(bad, 256)
        adapter.validate_prefix(12, 256)
        self.assertEqual(adapter.generation_of("store/segments/segment-00000008.continuation-00000001.json"), 8)
        self.assertEqual(adapter.generation_of("store/checkpoints/update-00000012.state.f32le"), 12)
        self.assertIsNone(adapter.generation_of("store/latest.json"))
        self.assertTrue(adapter.static("store/run.json"))
        self.assertFalse(adapter.log_succeeded("running 0 tests\ntest result: ok. 0 passed"))
        self.assertTrue(adapter.log_succeeded("test result: ok. 1 passed; 0 failed"))

    def test_arms_are_validated_and_enter_the_workload_identity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = json.loads(fake_workload(Path(directory)).read_text())
            base["arms"] = {"control": {"knobs": {}}, "treatment": {"knobs": {"FAKE_ARM": "1"}}}
            base["runs"] = [{"id": "c0", "seed": 1, "arm": "control"}, {"id": "t0", "seed": 2, "arm": "treatment"}]
            path = Path(directory) / "arms.json"
            path.write_text(json.dumps(base))
            workload = launcher.load_workload(path)
            self.assertEqual(workload.knobs(workload.runs[1]), {"FAKE_ARM": "1"})
            env = workload.adapter.environment(workload, workload.runs[1], directory, 0, None)
            self.assertEqual(env["FAKE_ARM"], "1")
            other = dict(base, arms={"control": {"knobs": {}}, "treatment": {"knobs": {"FAKE_ARM": "2"}}})
            path.write_text(json.dumps(other))
            self.assertNotEqual(launcher.load_workload(path).sha256, workload.sha256)
            for runs in ([{"id": "c0", "seed": 1}], [{"id": "c0", "seed": 1, "arm": "missing"}]):
                path.write_text(json.dumps(dict(base, runs=runs)))
                with self.assertRaises(launcher.LaunchRefused):
                    launcher.load_workload(path)

    def test_ssh_executor_refuses_a_staged_executable_with_another_hash(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            workload = launcher.load_workload(fake_workload(Path(directory), runs=1))
            calls = []

            def runner(command, **_):
                calls.append(command)
                stdout = "0" * 64 if "-EncodedCommand" in command and "Get-FileHash" in _decode(command) else ""
                return subprocess.CompletedProcess(command, 0, stdout, "")

            executor = launcher.SshPowerShellExecutor(workload, "haleyspc", "haley@example", Path(directory),
                                                      runner=runner)
            with self.assertRaises(RuntimeError) as caught:
                executor.stage()
            self.assertIn("staged executable hash differs", str(caught.exception))
            self.assertTrue(any(command[0] == "scp" for command in calls))


class SshRunTests(unittest.TestCase):
    def test_remote_run_stages_once_runs_with_owned_env_and_returns_comparable_outputs(self) -> None:
        import shutil as shell
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            executable = base / "trainer.exe"
            executable.write_bytes(b"remote trainer")
            data_root = base / "repo" / "data"
            data_root.mkdir(parents=True)
            raw = {"schema": launcher.WORKLOAD_SCHEMA, "adapter": "native-science-loop-pilot-v1",
                   "executable": str(executable), "executable_sha256": launcher.sha256_file(executable),
                   "planned_updates": 128, "knobs": {}, "runs": [{"id": "r0", "seed": 77}]}
            (base / "w.json").write_text(json.dumps(raw))
            workload = launcher.load_workload(base / "w.json")
            remote_archive = base / "remote.zip"
            scripts = []

            def runner(command, **_):
                if command[0] == "ssh":
                    script = _decode(command)
                    scripts.append(script)
                    if "Get-FileHash" in script:
                        return subprocess.CompletedProcess(command, 0, workload.executable_sha256 + "\r\n", "")
                    if "Compress-Archive" in script:
                        # The remote side trains to generation 8 and archives its run root.
                        staging = base / "remote-run"
                        store = staging / "parent" / "run-0" / "store"
                        (store / "checkpoints").mkdir(parents=True, exist_ok=True)
                        (store / "run.json").write_text("{}")
                        for generation in (0, 4, 8):
                            (store / "checkpoints" / f"update-{generation:08}.state.f32le").write_bytes(
                                bytes([generation]))
                        (staging / "process.log").write_text("test result: ok. 1 passed")
                        shell.make_archive(str(remote_archive.with_suffix("")), "zip", staging)
                        return subprocess.CompletedProcess(command, 0, "noise\r\n0\r\n", "")
                    return subprocess.CompletedProcess(command, 0, "", "")
                if command[0] == "scp" and command[-2].endswith(".zip"):
                    shell.copyfile(remote_archive, command[-1])
                return subprocess.CompletedProcess(command, 0, "", "")

            executor = launcher.SshPowerShellExecutor(workload, "haleyspc", "haley@example", data_root, runner=runner)
            result = executor.run(workload.runs[0], base / "runs" / "r0", 0, 8,
                                  {"MULTIRUN_LAUNCH_TICKET": "D:/t/r0.ticket.json"})
            executor.run(launcher.RunSpec("r1", 78), base / "runs" / "r1", 1, 8)
            self.assertEqual(sum("Get-FileHash" in script for script in scripts), 1, "stage once per host")
            self.assertEqual((result.exit_code, result.host, sorted(result.generations)), (0, "haleyspc", [4, 8]))
            run_script = next(script for script in scripts if "Compress-Archive" in script)
            for assignment in ("$env:MULTIRUN_BASE_SEED='77'", "$env:MULTIRUN_RUNS='1'",
                               "$env:MULTIRUN_STOP_AFTER_GENERATION='8'",
                               "$env:MULTIRUN_LAUNCH_TICKET='D:/t/r0.ticket.json'",
                               "Remove-Item Env:CUDA_VISIBLE_DEVICES"):
                self.assertIn(assignment, run_script)
            digests = workload.adapter.output_digests(base / "runs" / "r0")
            self.assertEqual(sorted(digests), ["store/checkpoints/update-00000000.state.f32le",
                                               "store/checkpoints/update-00000004.state.f32le",
                                               "store/checkpoints/update-00000008.state.f32le", "store/run.json"])
            self.assertGreater(executor.transfer_seconds, 0.0)


def _decode(command: list[str]) -> str:
    import base64
    return base64.b64decode(command[command.index("-EncodedCommand") + 1]).decode("utf-16le")


if __name__ == "__main__":
    unittest.main()
