from __future__ import annotations

import contextlib
import importlib.util
import io
import json
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "s_learner_attribution_runner", Path(__file__).with_name("run.py")
)
run = importlib.util.module_from_spec(spec)
spec.loader.exec_module(run)


def attempt_receipt(workers, *, seed=7, after="after", profiled=False):
    receipt = {
        "schema": "s-gae-bench-v1",
        "seed": seed,
        "groups": 4,
        "substeps": 5,
        "workers": workers,
        "seconds": [0.1],
        "before_state_sha256": "before",
        "after_state_sha256": after,
    }
    if profiled:
        receipt["profile"] = [{}]
    return {
        "command": ["stub"],
        "wall_seconds": 0.2,
        "user_seconds": 0.1,
        "system_seconds": 0.0,
        "max_rss_kib": 1,
        "page_faults": 1,
        "involuntary_switches": 0,
        "receipt": receipt,
    }


class AttributionRunnerTests(unittest.TestCase):
    def setUp(self):
        self.args = SimpleNamespace(
            baseline="baseline",
            candidate="candidate",
            profile="instrumented",
            baseline_commit="a",
            candidate_commit="b",
            build=None,
            out="receipt",
            workers=[1, 2],
            seed=7,
            groups=4,
            reps=1,
            rounds=1,
            cpus=None,
            skip_parity=False,
            profile_runs=1,
        )

    def test_commands_receive_the_same_resolved_paths_as_the_manifest(self):
        with TemporaryDirectory(dir=Path.cwd()) as folder, contextlib.chdir(folder):
            paths = {
                arm: Path(getattr(self.args, arm)).resolve()
                for arm in ("baseline", "candidate", "profile")
            }
            for arm, path in paths.items():
                path.write_bytes(arm.encode())

            def parity(args, out):
                self.assertEqual(args.baseline, paths["baseline"])
                self.assertEqual(args.candidate, paths["candidate"])
                return {}

            def abba(args, out):
                self.assertEqual(args.baseline, paths["baseline"])
                self.assertEqual(args.candidate, paths["candidate"])
                return {}

            def profile(args, out, reference):
                self.assertEqual(args.profile, paths["profile"])
                return {}

            with (
                patch.object(run, "parity", side_effect=parity),
                patch.object(run, "abba", side_effect=abba),
                patch.object(run, "profile", side_effect=profile),
                patch.object(run, "summarize", return_value=""),
                contextlib.redirect_stdout(io.StringIO()),
            ):
                run.command_run(self.args)
            manifest = json.loads(Path("receipt/manifest.json").read_text())
            for arm, path in paths.items():
                self.assertEqual(manifest["binaries"][arm], {
                    "commit": "a" if arm == "baseline" else "b",
                    **run.binary_identity(path),
                })

    def test_abba_refuses_cross_worker_adam_drift_even_when_arms_agree(self):
        def attempt(binary, arguments, cpus):
            workers = int(arguments[-1].split("=")[1])
            return attempt_receipt(workers, after=f"after-{workers}")

        with (
            patch.object(run, "attempt", side_effect=attempt),
            contextlib.redirect_stderr(io.StringIO()),
            self.assertRaisesRegex(RuntimeError, "digest differs across worker counts"),
        ):
            run.abba(self.args, None)

    def test_abba_accepts_matching_states_across_worker_counts(self):
        def attempt(binary, arguments, cpus):
            return attempt_receipt(int(arguments[-1].split("=")[1]))

        with (
            patch.object(run, "attempt", side_effect=attempt),
            contextlib.redirect_stderr(io.StringIO()),
        ):
            receipts = run.abba(self.args, None)
        self.assertEqual(set(receipts), {"1", "2"})

    def test_profile_refuses_a_receipt_for_a_different_fixture(self):
        expected = attempt_receipt(1)["receipt"]
        expected.pop("seconds")
        self.args.workers = [1]
        with (
            TemporaryDirectory(dir=Path.cwd()) as folder,
            patch.object(run, "attempt", return_value=attempt_receipt(1, seed=8, profiled=True)),
            self.assertRaisesRegex(RuntimeError, "gae-bench fixture differs"),
        ):
            run.profile(self.args, Path(folder), {"1": {"receipt_common": expected}})

    def test_backward_macs_summary_separates_run_unused_and_skipped_work(self):
        def macs(dx, dw, attention):
            return {"dx": dx, "dw": dw, "attention": attention, "rows": 0}

        before = {
            "total": macs(60, 30, 10),
            "final_block_object_query_rows_total": macs(10, 8, 2),
        }
        self.assertEqual(
            run.backward_macs_summary(before, 4),
            "Backward GEMM MACs per update: dx 60, dw 30, attention 10 (100 run); "
            "final-block object query rows 20 (20.0% of the full backward's 100).",
        )
        after = {
            "total": macs(50, 22, 8),
            "final_block_object_query_rows_total": macs(10, 8, 2),
            "final_block_removed_total": macs(10, 8, 2),
            "final_block_live_substeps": 4,
        }
        self.assertEqual(
            run.backward_macs_summary(after, 4),
            "Backward GEMM MACs per update: dx 50, dw 22, attention 8 (80 run); "
            "final-block object query rows 20 (20.0% of the full backward's 100), "
            "20 of them skipped (4 of 4 substeps compact).",
        )

    def test_pool_summary_reports_phases_and_task_kinds_by_worker(self):
        def report(scale):
            return {
                "pool": {
                    "helper_spawns": 1,
                    "joins": 1,
                    "spawn_failures": 0,
                    "spawn_ns": 2e6 * scale,
                    "join_wait_ns": 1e6 * scale,
                    "phases": 23,
                    "record_phases": 22,
                    "replay_phases": 22,
                    "phase_wall_ns": 100e6 * scale,
                    "busy_ns_by_worker": [96e6 * scale, 90e6 * scale],
                    "idle_ns_by_worker": [4e6 * scale, 10e6 * scale],
                },
                "tasks": {
                    "record": {"items_by_worker": [22, 22], "busy_ns_by_worker": [80e6 * scale, 70e6 * scale]},
                    "replay": {"items_by_worker": [300, 250], "busy_ns_by_worker": [14e6 * scale, 18e6 * scale]},
                },
            }

        self.assertEqual(
            run.pool_summary([report(1.0), report(3.0)]),
            [
                "Workers: 1 helper spawns, 1 joins, 0 spawn failures per update (spawn 4.000 ms, "
                "join wait 2.000 ms); 23 phases (22 recording, 22 replaying), 200.000 ms in phases; "
                "busy by worker [192.0, 180.0] ms, idle in phases [8.0, 20.0] ms.",
                "record tasks: 44 items, busy by worker [160.0, 140.0] ms.",
                "replay tasks: 550 items, busy by worker [28.0, 36.0] ms.",
            ],
        )


if __name__ == "__main__":
    unittest.main()
