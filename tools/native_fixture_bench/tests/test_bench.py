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
import unittest
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
from pathlib import Path

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
data = json.dumps({"seed": int(os.environ["MTG_NATIVE_FIXTURE_SEED"])}).encode()
config.write_bytes(data)
manifest = {"schema": "mtg-kernel-native-fixture/v1", "root": str(root.resolve()),
            "options": {"seed": int(os.environ["MTG_NATIVE_FIXTURE_SEED"])},
            "cases": [{"name": "fresh", "output_directory": str(root.resolve() / "work/fresh/run"),
                       "config": {"path": str(config), "sha256": hashlib.sha256(data).hexdigest()}}]}
(root / "fixture.json").write_text(json.dumps(manifest, indent=2))
print("NATIVE_FIXTURE_V1 " + json.dumps(manifest))
'''


def executable(directory: Path, name: str, source: str) -> Path:
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


@unittest.skipUnless(sys.platform.startswith(("linux", "darwin")), "the driver uses os.wait4 and process groups")
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
        with open(self.out / "samples.jsonl", "a") as handle:
            handle.write("{}\n")
        with redirect_stderr(StringIO()):
            self.assertEqual(bench.main(["compare", "--out", str(self.out)]), 2)

    def test_a_changed_state_or_a_failed_sample_fails_the_comparison(self):
        code = self.bench("--warmup-runs", "0", "--cases", "fresh",
                          "--arm-env", "branch:FAKE_STATE_SHIFT=1")
        self.assertEqual(code, 1)
        samples = self.samples()
        branch = [s for s in samples if s["arm"] == "branch"]
        for sample in branch:
            parity = sample["parity"]
            self.assertEqual(parity["status"], "mismatch")
            self.assertFalse(parity["boundaries_equal"])
            mismatched = {f["file"] for f in parity["mismatched_files"]}
            self.assertIn("iterations/000000/attempt-000000/update/checkpoint.json", mismatched)
        main = [s for s in samples if s["arm"] == "main"]
        self.assertEqual(main[1]["parity"]["status"], "equal_up_to_accounted_fields")
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
                               "--test-binary", str(generator)])
        self.assertEqual(code, 0)
        record = json.loads((root / "generate" / "generate.json").read_text())
        self.assertEqual(record["settings"]["MTG_NATIVE_FIXTURE_SEED"], "7")
        self.assertEqual(record["settings"]["MTG_NATIVE_FIXTURE_WARMUP_UPDATES"], "2")
        self.assertEqual(record["fixture_sha256"], bench.sha256_file(root / "fixture.json"))
        self.assertEqual(record["test_binary"]["sha256"], bench.sha256_file(generator))
        self.assertIn(bench.GENERATOR_TEST, record["command"])
        with redirect_stderr(StringIO()):
            self.assertEqual(bench.main(["generate", "--root", str(root),
                                         "--test-binary", str(generator)]), 2, "nonempty root")


if __name__ == "__main__":
    unittest.main()
