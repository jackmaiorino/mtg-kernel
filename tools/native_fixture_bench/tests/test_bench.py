"""The native fixture benchmark driver, against a fake native run binary: the
A-B-B-A plan, the comparison rule, parity accounting (timing, build
provenance, pins), cold resume, refusals and the generator wrapper."""
import hashlib
import json
import os
import shutil
import stat
import sys
import tempfile
import time
import unittest
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import bench  # noqa: E402

# A stand-in for native_expanded_training_run_v1 with the same layout: run.json
# (with build provenance), per-iteration collection/update receipts carrying
# fresh timings, a checkpoint, complete.json pins, completion.json, the
# per-iteration stdout line and the diagnostics line. It resumes completed
# iterations and honours --max-new-iterations.
FAKE_BINARY = r'''
import hashlib, json, os, random, sys
from pathlib import Path

args = sys.argv[1:]
limit = int(args[args.index("--max-new-iterations") + 1]) if "--max-new-iterations" in args else None
config = json.loads(Path(args[0]).read_text())
root = Path(config["output_directory"])
root.mkdir(parents=True, exist_ok=True)
(root / "run.lock").touch()
shift = int(os.environ.get("FAKE_STATE_SHIFT", "0"))
exit_at = os.environ.get("FAKE_EXIT_AT")
diagnostics = os.environ.get("MTG_KERNEL_CACHE_DIAGNOSTICS_V1")
commit = os.environ.get("FAKE_GIT_COMMIT", "aaaa")

def publish(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    data = json.dumps(value, sort_keys=True).encode()
    path.write_bytes(data)
    return {"path": str(path), "sha256": hashlib.sha256(data).hexdigest()}

manifest = {"schema": "mtg-kernel-native-expanded-training-run/v1", "config": config,
            "git_commit": commit, "tracked_tree_sha256": commit * 2, "implementation_sha256": "i"}
if (root / "run.json").exists():
    if json.loads((root / "run.json").read_text()) != manifest:
        sys.exit("resume plan or implementation differs")
else:
    publish(root / "run.json", manifest)
state, step = config["start_state"], config["start_step"]
receipts, new, lookups = [], 0, 0
for index, iteration in enumerate(config["iterations"]):
    directory = root / "iterations" / f"{index:06d}"
    complete = directory / "complete.json"
    if complete.exists():
        receipt = json.loads(complete.read_text())
        receipts.append({"path": str(complete),
                         "sha256": hashlib.sha256(complete.read_bytes()).hexdigest()})
        state, step = receipt["output_identity"]["state"], receipt["output_identity"]["adam_step"]
        continue
    if limit is not None and new == limit:
        break
    if exit_at is not None and int(exit_at) == index:
        sys.exit(3)
    attempt = directory / "attempt-000000"
    trajectory = publish(attempt / "collect" / "trajectory-000000.json",
                         {"seed": iteration["seed"], "decisions": [state % 7, iteration["seed"]]})
    collection = publish(attempt / "collect" / "collection.json",
                         {"trajectories": [trajectory], "collection_elapsed_seconds": random.random(),
                          "collection_initialization_seconds": random.random()})
    state, step = (state * 31 + iteration["seed"] + shift) % 1000003, step + 1
    checkpoint = publish(attempt / "update" / "checkpoint.json",
                         {"state": state, "adam_step": step, "moments": [state, state + 1]})
    update = publish(attempt / "update" / "update.json",
                     {"checkpoint": checkpoint, "learner_update_seconds": random.random(),
                      "checkpoint_io_seconds": random.random(),
                      "update_preparation": {"preparation_elapsed_seconds": random.random()}})
    identity = {"adam_step": step, "state": state, "state_sha256": checkpoint["sha256"]}
    receipts.append(publish(complete, {"iteration": index, "collection": collection, "update": update,
                                       "output_identity": identity}))
    new, lookups = new + 1, lookups + 3
    if diagnostics:
        with open(diagnostics, "a") as handle:
            handle.write(json.dumps({"schema": "mtg-kernel-cache-diagnostics/v1",
                                     "completed_iteration": index, "newly_completed": True,
                                     "process_id": os.getpid(),
                                     "loaded_source": {"total": {"cache_lookups": lookups,
                                                                 "scoped_nanos": random.randrange(1, 10**6)}}})
                         + "\n")
    print(json.dumps({"completed_iteration": index, "adam_step": step, "scheduler_timing": {
        "iteration_wall_seconds": 0.01, "collection_execution_seconds": 0.004,
        "update_execution_seconds": 0.005, "collection_validation_seconds": 0.0005,
        "update_validation_seconds": 0.0005}}), flush=True)
finished = len(receipts) == len(config["iterations"])
result = {"schema": "mtg-kernel-native-expanded-training-run/v1", "complete": finished,
          "completed_iterations": len(receipts), "planned_iterations": len(config["iterations"]),
          "iterations": receipts}
if finished:
    publish(root / "completion.json", result)
print(json.dumps(result))
'''

# A stand-in for the lib test binary's ignored generator entry point.
FAKE_GENERATOR = r'''
import hashlib, json, os, sys
from pathlib import Path
assert "--ignored" in sys.argv and "--exact" in sys.argv
root = Path(os.environ["MTG_NATIVE_FIXTURE_ROOT"])
root.mkdir(parents=True, exist_ok=True)
config = root / "cases" / "fresh" / "config.json"
config.parent.mkdir(parents=True)
output = str(root.resolve() / "work/fresh/run")
data = json.dumps({"output_directory": output, "iterations": [
    {"seed": int(os.environ["MTG_NATIVE_FIXTURE_SEED"])}]}).encode()
config.write_bytes(data)
manifest = {"schema": "mtg-kernel-native-fixture/v1", "root": str(root.resolve()),
            "options": {"seed": int(os.environ["MTG_NATIVE_FIXTURE_SEED"])},
            "cases": [{"name": "fresh", "output_directory": output, "iterations": 1,
                       "config": {"path": str(config), "sha256": hashlib.sha256(data).hexdigest()}}]}
(root / "fixture.json").write_text(json.dumps(manifest, indent=2))
print("NATIVE_FIXTURE_V1 " + json.dumps(manifest))
'''


def executable(directory: Path, name: str, source: str) -> Path:
    if os.name == "nt":
        # Windows runs no shebang script, so a batch file starts the interpreter.
        script = directory / f"{name}.py"
        script.write_text(source, encoding="utf-8")
        path = directory / f"{name}.cmd"
        path.write_text(f'@"{sys.executable}" "{script}" %*\n@exit /b %ERRORLEVEL%\n')
        return path
    path = directory / name
    path.write_text(f"#!{sys.executable}\n{source}")
    path.chmod(path.stat().st_mode | stat.S_IXUSR)
    return path


def make_fixture(root: Path, iterations: int = 2) -> None:
    root.mkdir(parents=True)
    cases = []
    for offset, name in enumerate(("fresh", "warm")):
        config = {"output_directory": str(root / "work" / name / "run"), "start_state": 5 + offset,
                  "start_step": 2 * offset, "iterations": [{"seed": 100 * offset + i}
                                                          for i in range(iterations)]}
        path = root / "cases" / name / "config.json"
        path.parent.mkdir(parents=True)
        data = json.dumps(config).encode()
        path.write_bytes(data)
        cases.append({"name": name, "output_directory": config["output_directory"],
                      "iterations": iterations, "first_iteration": 2 * offset,
                      "config": {"path": str(path), "sha256": hashlib.sha256(data).hexdigest()}})
    fixture = {"schema": bench.FIXTURE_SCHEMA, "root": str(root), "cases": cases,
               "generator": {"git_commit": "test"}, "options": {}, "workload": {}}
    (root / "fixture.json").write_text(json.dumps(fixture))


class FixtureValidationTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(dir=Path.cwd())
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "fixture"
        make_fixture(self.root)
        self.fixture = bench.read_json(self.root / "fixture.json")
        self.case = self.fixture["cases"][0]

    def test_fixture_must_bind_case_count_and_config_output(self):
        configs = bench.validate_fixture(self.fixture, self.root)
        self.assertEqual(list(configs), ["fresh", "warm"])
        with self.assertRaisesRegex(bench.BenchError, "at least one case"):
            bench.validate_fixture({**self.fixture, "cases": []}, self.root)
        self.case["iterations"] = 1
        with self.assertRaisesRegex(bench.BenchError, "iteration counts differ"):
            bench.validate_fixture(self.fixture, self.root)
        self.case["iterations"] = 2
        config_path = Path(self.case["config"]["path"])
        config = {**configs["fresh"], "output_directory": str(self.root / "elsewhere")}
        config_path.write_text(json.dumps(config), encoding="utf-8")
        self.case["config"]["sha256"] = bench.sha256_file(config_path)
        with self.assertRaisesRegex(bench.BenchError, "manifest and config must write"):
            bench.validate_fixture(self.fixture, self.root)

    def test_report_off_and_durable_result_check_use_the_actual_sample(self):
        seen_environment = []

        def run_fake(command, env, stdout, stderr, timeout):
            seen_environment.append(env)
            with (patch.object(sys, "argv", command), patch.dict(os.environ, env, clear=True),
                  stdout.open("w", encoding="utf-8") as output, redirect_stdout(output)):
                exec(FAKE_BINARY, {"__name__": "__main__"})
            return {"timed_out": False, "exit_code": 0, "wall_seconds": 0.1,
                    "max_rss_mib": 1.0, "user_seconds": 0.01, "sys_seconds": 0.01}

        out = self.root.parent / "samples"
        entry = {"ordinal": 0, "kind": "timed", "position": "A1", "case": "fresh"}
        arm = {"name": "baseline", "binary": "fake", "sha256": "test", "env": {}}
        with (patch.object(bench, "run_process", side_effect=run_fake),
              patch.dict(os.environ, {bench.DIAGNOSTICS_ENV: "inherited.jsonl"})):
            sample = bench.run_sample(entry, arm, self.case, out, {}, None, cache_diagnostics=False)
        self.assertTrue(sample["ok"], sample.get("error"))
        self.assertFalse(sample["cache_diagnostics"])
        self.assertNotIn(bench.DIAGNOSTICS_ENV, seen_environment[0])
        self.assertIsNone(sample["diagnostics"])
        tree = out / sample["run_directory"]
        result = bench.read_json(tree / "completion.json")
        prefix = self.case["output_directory"]
        with self.assertRaisesRegex(bench.BenchError, "planned iterations"):
            bench.require_completed_sample(result, sample["boundaries"], tree, prefix, 3)
        with self.assertRaisesRegex(bench.BenchError, "durable iteration boundaries"):
            bench.require_completed_sample(result, sample["boundaries"][:1], tree, prefix, 2)
        boundary = tree / "iterations/000001/complete.json"
        boundary.write_text("{}", encoding="utf-8")
        with self.assertRaisesRegex(bench.BenchError, "durable completion does not match"):
            bench.require_completed_sample(result, sample["boundaries"], tree, prefix, 2)

    def test_pool_cases_bind_the_member_bytes_and_each_draw(self):
        def publish(path, data):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(data), encoding="utf-8")
            return {"path": str(path), "sha256": bench.sha256_file(path)}

        descriptor = publish(self.root / "descriptor.json", {"synthetic": "fixture"})
        member_pins = [publish(self.root / f"pool/member-{index:02d}/checkpoint.json",
                               {"member": index}) for index in range(2)]
        members = [{"id": f"member-{index:02d}", "checkpoint": pin}
                   for index, pin in enumerate(member_pins)]
        source = {"play_import": descriptor, "checkpoint": None}
        self.fixture.update(options={"pool_members": [2], "seed": 2026091601,
                                     "games": 3, "updates": 1},
                            descriptor=descriptor,
                            pool={"directory": str(self.root / "pool"), "members": members,
                                  "draw": {"tag": "mtg-kernel-pool-fixture-draw/v1",
                                           "seed_root": "2026091601"}})
        episodes = [{"episode": {"id": f"fixture-0000-{slot:02d}", "seed": 2026091601 + slot,
                                 "starting_player": slot % 2, "learner_seat": 0},
                     "opponent": {"kind": "current"}} for slot in range(3)]
        fresh = {"output_directory": self.case["output_directory"], "initial_source": source,
                 "iterations": [{"episodes": episodes}]}
        self.case["iterations"] = 1
        self.case["config"] = publish(Path(self.case["config"]["path"]), fresh)

        # Published draw rule for seed 2026091601, iteration 0 and slots 0..2.
        draws = ["member-00", "member-01", "member-01"]
        pool = {**fresh, "output_directory": str(self.root / "work/pool2/run"),
                "opponents": [{"id": member["id"],
                               "source": {**source, "checkpoint": member["checkpoint"]}}
                              for member in members],
                "iterations": [{"episodes": [
                    {**row, "opponent": {"kind": "fixed", "id": drawn}}
                    for row, drawn in zip(episodes, draws, strict=True)]}]}
        config_path = self.root / "cases/pool2/config.json"
        case = {"name": "pool2", "iterations": 1, "first_iteration": 0,
                "output_directory": pool["output_directory"], "config": publish(config_path, pool),
                "pool": {"members": 2, "draws": [draws.copy()]}}
        self.fixture["cases"].append(case)
        bench.validate_fixture(self.fixture, self.root)
        case["pool"]["draws"][0][0] = "member-01"
        with self.assertRaisesRegex(bench.BenchError, "differs from the pinned roster and draws"):
            bench.validate_fixture(self.fixture, self.root)
        case["pool"]["draws"][0][0] = draws[0]
        pool["iterations"][0]["episodes"][0]["episode"]["seed"] += 1
        case["config"] = publish(config_path, pool)
        with self.assertRaisesRegex(bench.BenchError, "differs from the pinned roster and draws"):
            bench.validate_fixture(self.fixture, self.root)
        Path(member_pins[0]["path"]).write_text("{}", encoding="utf-8")
        with self.assertRaisesRegex(bench.BenchError, "pool fixture pin changed"):
            bench.validate_fixture(self.fixture, self.root)


class PlanAndRuleTest(unittest.TestCase):
    def test_the_plan_runs_warm_ups_then_balanced_blocks_then_cold_resumes(self):
        plan = bench.plan_samples(["fresh", "warm"], blocks=2, warmup_runs=1, cold_resume=True)
        fresh = [(p["kind"], p["arm_letter"], p["position"]) for p in plan if p["case"] == "fresh"]
        self.assertEqual(fresh, [
            ("warmup", "A", "warmup1"), ("warmup", "B", "warmup1"),
            ("timed", "A", "A1"), ("timed", "B", "B1"), ("timed", "B", "B2"), ("timed", "A", "A2"),
            ("timed", "A", "A1"), ("timed", "B", "B1"), ("timed", "B", "B2"), ("timed", "A", "A2"),
            ("cold_resume", "A", "cold"), ("cold_resume", "B", "cold")])
        self.assertEqual([p["ordinal"] for p in plan], list(range(len(plan))))
        self.assertEqual(plan[len(fresh)]["case"], "warm")

    def test_the_rule_reports_inconclusive_unless_the_effect_exceeds_the_spread(self):
        base, fast = [10.0, 10.2, 10.1, 10.3], [8.0, 8.1, 8.05, 8.2]
        blocks = [([10.0, 10.3], [8.0, 8.1]), ([10.2, 10.1], [8.05, 8.2])]
        self.assertEqual(bench.verdict(base, fast, blocks, True)["verdict"], "improvement")
        swapped = [(b, a) for a, b in blocks]
        self.assertEqual(bench.verdict(fast, base, swapped, True)["verdict"], "regression")
        self.assertEqual(bench.verdict(base, fast, blocks, False)["verdict"], "regression")
        noisy = [9.0, 11.0, 10.0, 10.5]
        noisy_blocks = [([10.0, 10.3], [9.0, 11.0]), ([10.2, 10.1], [10.0, 10.5])]
        result = bench.verdict(base, noisy, noisy_blocks, True)
        self.assertEqual(result["verdict"], "inconclusive")
        self.assertIn("spread", result["reason"])
        # One block cannot show that the direction repeats: an A/A probe with
        # a concurrent build once measured a 12% "effect" this way.
        self.assertEqual(bench.verdict(base[:2], fast[:2], blocks[:1], True)["reason"],
                         "fewer than two complete A-B-B-A blocks")
        disagree = [([10.0, 10.0], [9.0, 9.0]), ([9.0, 9.0], [10.0, 10.0])]
        self.assertEqual(bench.verdict([10.0] * 4, [9.0] * 4, disagree, True)["reason"],
                         "A-B-B-A blocks disagree in direction")
        self.assertEqual(bench.verdict([10.0], [9.0], [], True)["reason"],
                         "fewer than two samples per arm")

    def test_peak_rss_units_follow_the_platform(self):
        self.assertEqual(bench.max_rss_mib(2048, "linux"), 2.0)
        self.assertEqual(bench.max_rss_mib(2 << 20, "darwin"), 2.0)

    def test_a_windows_verbatim_path_compares_as_its_ordinary_form(self):
        # Rust's canonicalize returns \\?\C:\... on Windows; the driver builds C:\....
        self.assertEqual(bench.plain_path("\\\\?\\C:\\f\\work", windows=True), "C:\\f\\work")
        self.assertEqual(bench.plain_path("\\\\?\\UNC\\host\\share\\f", windows=True),
                         "\\\\host\\share\\f")
        self.assertEqual(bench.plain_path("\\\\?\\C:\\f", windows=False), "\\\\?\\C:\\f")
        self.assertEqual(bench.plain_path("/tmp/f", windows=True), "/tmp/f")
        if os.name == "nt":
            root = Path(tempfile.gettempdir()).resolve()
            verbatim = "\\\\?\\" + str(root)
            self.assertTrue(bench.matches_fixture_path(verbatim + "\\work", root / "work"))
            self.assertEqual(bench.map_pin(verbatim + "\\run\\a.json", str(root / "run")), "a.json")


class RunProcessTest(unittest.TestCase):
    """The process timer: wait4 on Linux and macOS, a job object on Windows."""

    def setUp(self):
        self.temp = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.temp, ignore_errors=True)

    def run_python(self, source: str, timeout=None) -> dict:
        # A virtual environment's python.exe on Windows is a launcher that runs the
        # base interpreter as its child, and the peak working set is the started
        # process's own, so start the base interpreter itself there.
        python = getattr(sys, "_base_executable", None) if os.name == "nt" else None
        return bench.run_process([python or sys.executable, "-c", source], dict(os.environ),
                                 self.temp / "stdout.txt", self.temp / "stderr.txt", timeout)

    def test_a_run_reports_its_exit_code_cpu_time_and_peak_memory(self):
        result = self.run_python(
            "import sys, time\n"
            "data = b'x' * (96 << 20)\n"
            "end = time.process_time() + 0.3\n"
            "while time.process_time() < end:\n"
            "    pass\n"
            "sys.stdout.write('done')\n"
            "sys.exit(7)")
        self.assertEqual(result["exit_code"], 7)
        self.assertFalse(result["timed_out"])
        self.assertGreater(result["user_seconds"] + result["sys_seconds"], 0.2)
        self.assertGreater(result["wall_seconds"], 0.2)
        self.assertGreater(result["max_rss_mib"], 90, result)
        if os.name == "nt":
            self.assertGreaterEqual(result["peak_commit_mib"], 90, result)
            self.assertGreaterEqual(result["job_processes"], 1, result)
        self.assertEqual((self.temp / "stdout.txt").read_text(), "done")

    def test_a_timeout_ends_the_whole_process_tree(self):
        started, survived = self.temp / "started", self.temp / "survived"
        child = (f"import pathlib, time; pathlib.Path({str(started)!r}).touch(); "
                 f"time.sleep(3); pathlib.Path({str(survived)!r}).touch()")
        result = self.run_python(
            "import pathlib, subprocess, sys, time\n"
            f"subprocess.Popen([sys.executable, '-c', {child!r}])\n"
            f"while not pathlib.Path({str(started)!r}).exists():\n"
            "    time.sleep(0.05)\n"
            "time.sleep(60)", timeout=2.0)
        self.assertTrue(result["timed_out"])
        self.assertNotEqual(result["exit_code"], 0)
        self.assertLess(result["wall_seconds"], 30)
        self.assertTrue(started.exists(), "the child started before the timeout")
        time.sleep(3.5)
        self.assertFalse(survived.exists(), "the timeout also ended the child")


class TreeComparisonTest(unittest.TestCase):
    def compare_timing_prefix(self, left_semantic, right_semantic):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            left, right = root / "reference", root / "sample"
            for tree, time_value, semantic in ((left, 1, left_semantic), (right, 2, right_semantic)):
                tree.mkdir()
                fields = {f"phase_{i:05d}_seconds": time_value for i in range(bench.COLLECTED_DIFFERENCES)}
                fields["zzz_semantic"] = semantic
                (tree / "receipt.json").write_text(json.dumps(fields), encoding="utf-8")
            comparison = bench.TreeComparison(left, right, prefix="run", across_builds=False)
            return comparison.file("receipt.json"), comparison.summary()

    def test_timing_prefix_cannot_hide_later_semantic_difference(self):
        file_result, summary = self.compare_timing_prefix("before", "after")
        self.assertEqual(file_result["status"], "mismatch")
        self.assertTrue(file_result["truncated"])
        self.assertIn("JSON difference limit exceeded", file_result["differences"])
        self.assertEqual(file_result["classes"], {"timing": bench.COLLECTED_DIFFERENCES})
        self.assertEqual(summary["status"], "mismatch")
        self.assertEqual(summary["accounted_files"], [])

    def test_exactly_the_limit_of_fully_classified_differences_is_accounted(self):
        file_result, summary = self.compare_timing_prefix("same", "same")
        self.assertEqual(file_result, {"status": "accounted", "classes": {"timing": bench.COLLECTED_DIFFERENCES}})
        self.assertEqual(summary["status"], "equal_up_to_accounted_fields")


@unittest.skipUnless(hasattr(os, "wait4") or os.name == "nt", "the driver times runs with wait4 or a job object")
class DriverTest(unittest.TestCase):
    def setUp(self):
        self.temp = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.temp, ignore_errors=True)
        self.binary = executable(self.temp, "native_run", FAKE_BINARY)
        self.fixture = self.temp / "fixture"
        make_fixture(self.fixture)
        self.out = self.temp / "out"

    def bench(self, *extra, baseline=None, candidate=None):
        argv = ["run", "--fixture", str(self.fixture), "--out", str(self.out),
                "--baseline", baseline or f"main={self.binary}",
                "--candidate", candidate or f"branch={self.binary}",
                "--arm-env", "branch:FAKE_GIT_COMMIT=bbbb", *extra]
        with redirect_stdout(StringIO()), redirect_stderr(StringIO()) as errors:
            code = bench.main(argv)
        self.errors = errors.getvalue()
        return code

    def samples(self):
        return [json.loads(line) for line in (self.out / "samples.jsonl").read_text().splitlines()]

    def test_two_builds_of_the_same_run_match_up_to_timing_and_provenance(self):
        code = self.bench("--warmup-runs", "1", "--cold-resume", "--label", "unit")
        self.assertEqual(code, 0, self.errors)
        samples = self.samples()
        self.assertEqual(len(samples), 2 * (2 + 4 + 2))
        self.assertTrue(all(s["ok"] for s in samples))
        self.assertFalse((self.fixture / "work" / "fresh" / "run").exists(), "runs are moved out")
        by_case = {case: [s for s in samples if s["case"] == case] for case in ("fresh", "warm")}
        for case, rows in by_case.items():
            self.assertNotIn("parity", rows[0], "the first sample is the reference")
            for sample in rows[1:]:
                parity = sample["parity"]
                self.assertEqual(parity["status"], "equal_up_to_accounted_fields", (case, parity))
                self.assertTrue(parity["boundaries_equal"])
                self.assertEqual(parity["boundaries"], 2)
                classes = {f["file"]: f["classes"] for f in parity["accounted_files"]}
                self.assertEqual(classes["iterations/000000/attempt-000000/update/update.json"],
                                 {"timing": 3})
                self.assertEqual(classes["iterations/000000/complete.json"], {"pin": 2})
                self.assertIn("pin", classes["completion.json"])
                across = sample["arm"] != rows[0]["arm"]
                self.assertEqual(parity["across_builds"], across)
                self.assertEqual("run.json" in classes, across)
                if across:
                    self.assertEqual(classes["run.json"], {"provenance": 2})
        cold = [s for s in samples if s["kind"] == "cold_resume"]
        self.assertEqual([s["first_process_completed"] for s in cold], [1] * 4)
        self.assertTrue(all(len(s["processes"]) == 2 for s in cold))
        self.assertTrue(all(s["newly_completed_iterations"] == 1 for s in cold))
        timed = [s for s in samples if s["kind"] == "timed"]
        self.assertEqual([s["position"] for s in timed[:4]], ["A1", "B1", "B2", "A2"])
        self.assertTrue(all(s["durable_completed_iterations"] == 2 for s in timed))
        self.assertTrue(all(s["diagnostics"]["lines"] == 2 for s in timed))
        self.assertAlmostEqual(timed[0]["metrics"]["scheduler.iteration_wall_seconds"], 0.02)
        self.assertIn("receipts.checkpoint_io_seconds", timed[0]["metrics"])
        comparison = json.loads((self.out / "comparison.json").read_text())
        self.assertTrue(comparison["ok"])
        self.assertEqual(comparison["compared_by"]["sha256"], bench.sha256_file(Path(bench.__file__)))
        fresh = comparison["cases"]["fresh"]
        self.assertEqual(fresh["timed_samples"], {"main": 2, "branch": 2})
        self.assertIn(fresh["metrics"]["wall_seconds"]["verdict"],
                      ("inconclusive", "improvement", "regression"))
        self.assertEqual(fresh["metrics"]["wall_seconds"]["baseline"]["n"], 2)
        self.assertEqual(len(fresh["metrics"]["wall_seconds"]["block_ratios"]), 1)
        counters = fresh["counters"]["branch"]
        self.assertEqual(counters["counts"], {"loaded_source": {"total": {"cache_lookups": 6}}})
        self.assertTrue(counters["equal_across_samples"], "timing leaves are not compared")
        self.assertIn("loaded_source/total/scoped_nanos", counters["timing_medians"])
        self.assertEqual([c["status"] for c in fresh["cold_resume"]],
                         ["equal_up_to_accounted_fields"] * 2)
        manifest = json.loads((self.out / "bench.json").read_text())
        self.assertEqual(manifest["status"], "complete")
        self.assertEqual(manifest["arms"]["candidate"]["env"], {"FAKE_GIT_COMMIT": "bbbb"})
        self.assertEqual(manifest["arms"]["baseline"]["sha256"], bench.sha256_file(self.binary))
        self.assertEqual(manifest["outputs"]["samples.jsonl"],
                         bench.sha256_file(self.out / "samples.jsonl"))
        self.assertEqual(len(manifest["plan"]), len(samples))
        self.assertTrue(manifest["binaries_unchanged"])
        # compare recomputes the same comparison from the retained samples.
        before = comparison["cases"]
        with redirect_stdout(StringIO()):
            self.assertEqual(bench.main(["compare", "--out", str(self.out)]), 0)
        self.assertEqual(json.loads((self.out / "comparison.json").read_text())["cases"], before)
        # receipt keeps every sample as one row, with the outputs' pins and the verdicts.
        receipt = self.temp / "receipt.json"
        with redirect_stdout(StringIO()):
            self.assertEqual(bench.main(["receipt", "--run", f"smoke={self.out}",
                                         "--write", str(receipt)]), 0)
        smoke = json.loads(receipt.read_text())["runs"]["smoke"]
        self.assertEqual(len(smoke["samples"]), len(samples))
        row = dict(zip(smoke["sample_columns"], smoke["samples"][2]))
        self.assertEqual((row["kind"], row["position"]), ("timed", "A1"))
        self.assertEqual(row["parity"], "equal_up_to_accounted_fields")
        self.assertIn("timing", row["accounted"])
        self.assertEqual(smoke["outputs_sha256"]["samples.jsonl"],
                         bench.sha256_file(self.out / "samples.jsonl"))
        self.assertEqual(smoke["cases"]["fresh"]["counters"]["branch"]["counts"],
                         counters["counts"])
        self.assertIn("wall_seconds", [r[0] for r in smoke["cases"]["fresh"]["verdicts"]])
        with open(self.out / "samples.jsonl", "a") as handle:
            handle.write("{}\n")
        with redirect_stderr(StringIO()):
            self.assertEqual(bench.main(["compare", "--out", str(self.out)]), 2)
            self.assertEqual(bench.main(["receipt", "--run", f"smoke={self.out}",
                                         "--write", str(receipt)]), 2)

    def test_a_changed_state_or_a_failed_sample_fails_the_comparison(self):
        code = self.bench("--warmup-runs", "0", "--cases", "fresh",
                          "--keep-runs", "reference", "--arm-env", "branch:FAKE_STATE_SHIFT=1")
        self.assertEqual(code, 1)
        samples = self.samples()
        branch = [s for s in samples if s["arm"] == "branch"]
        for sample in branch:
            parity = sample["parity"]
            self.assertEqual(parity["status"], "mismatch")
            self.assertFalse(parity["boundaries_equal"])
            mismatched = {f["file"] for f in parity["mismatched_files"]}
            checkpoint = "iterations/000000/attempt-000000/update/checkpoint.json"
            self.assertIn(checkpoint, mismatched)
            self.assertTrue((self.out / sample["run_directory"] / checkpoint).is_file())
            self.assertFalse(sample.get("run_pruned", False))
        main = [s for s in samples if s["arm"] == "main"]
        self.assertEqual(main[1]["parity"]["status"], "equal_up_to_accounted_fields")
        self.assertTrue(main[1]["run_pruned"])
        self.assertFalse((self.out / main[1]["run_directory"]).exists())
        comparison = json.loads((self.out / "comparison.json").read_text())
        self.assertFalse(comparison["cases"]["fresh"]["parity_ok"])

        shutil.rmtree(self.out)
        code = self.bench("--warmup-runs", "0", "--cases", "fresh", "--arm-env", "branch:FAKE_EXIT_AT=1")
        self.assertEqual(code, 1)
        failed = [s for s in self.samples() if not s["ok"]]
        self.assertEqual([s["arm"] for s in failed], ["branch", "branch"])
        self.assertEqual(failed[0]["error"], "exit code 3")
        self.assertEqual(failed[0]["durable_completed_iterations"], 1, "the published prefix is kept")
        comparison = json.loads((self.out / "comparison.json").read_text())
        self.assertEqual(len(comparison["cases"]["fresh"]["failures"]), 2)
        self.assertEqual(comparison["cases"]["fresh"]["metrics"]["wall_seconds"]["verdict"],
                         "inconclusive")

    def test_reference_retention_prunes_every_other_run_tree(self):
        self.assertEqual(self.bench("--warmup-runs", "0", "--cases", "fresh",
                                    "--keep-runs", "reference"), 0, self.errors)
        samples = self.samples()
        kept = [s for s in samples if (self.out / s["run_directory"]).exists()]
        self.assertEqual([s["ordinal"] for s in kept], [0])
        self.assertTrue(all(s.get("run_pruned") for s in samples[1:]))
        self.assertTrue(all(s["parity"]["status"] == "equal_up_to_accounted_fields"
                            for s in samples[1:]))

    def test_the_driver_refuses_unsafe_or_ambiguous_requests(self):
        self.out.mkdir()
        (self.out / "stale").write_text("x")
        self.assertEqual(self.bench(), 2)
        self.assertIn("absent or an empty directory", self.errors)
        shutil.rmtree(self.out)
        leftover = self.fixture / "work" / "fresh" / "run"
        leftover.mkdir(parents=True)
        self.assertEqual(self.bench(), 2)
        self.assertIn("interrupted", self.errors)
        shutil.rmtree(leftover)
        self.assertEqual(self.bench("--cases", "pool"), 2)
        self.assertIn("unknown case", self.errors)
        self.assertEqual(self.bench("--env", f"{bench.DIAGNOSTICS_ENV}=/x"), 2)
        self.assertEqual(self.bench(candidate=f"main={self.binary}"), 2)
        config = self.fixture / "cases" / "fresh" / "config.json"
        config.write_text(config.read_text() + " ")
        self.assertEqual(self.bench(), 2)
        self.assertIn("changed since generation", self.errors)

    def test_generate_runs_the_ignored_entry_point_and_records_it(self):
        generator = executable(self.temp, "mtg_kernel_lib_tests", FAKE_GENERATOR)
        root = self.temp / "generated"
        with redirect_stdout(StringIO()):
            code = bench.main(["generate", "--root", str(root), "--seed", "7",
                               "--pool-members", "2,18", "--test-binary", str(generator)])
        self.assertEqual(code, 0)
        record = json.loads((root / "generate" / "generate.json").read_text())
        self.assertEqual(record["settings"]["MTG_NATIVE_FIXTURE_SEED"], "7")
        self.assertEqual(record["settings"]["MTG_NATIVE_FIXTURE_WARMUP_UPDATES"], "2")
        self.assertEqual(record["settings"]["MTG_NATIVE_FIXTURE_POOL_MEMBERS"], "2,18")
        self.assertEqual(record["fixture_sha256"], bench.sha256_file(root / "fixture.json"))
        self.assertEqual(record["test_binary"]["sha256"], bench.sha256_file(generator))
        self.assertIn(bench.GENERATOR_TEST, record["command"])
        with redirect_stderr(StringIO()):
            self.assertEqual(bench.main(["generate", "--root", str(root),
                                         "--test-binary", str(generator)]), 2, "nonempty root")


if __name__ == "__main__":
    unittest.main()
