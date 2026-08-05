from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).with_name("screen_topology_v1.py")
SPEC = importlib.util.spec_from_file_location("screen_topology", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
screen = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(screen)


def _task(first_pair: int, outcome: str, teacher: str) -> dict:
    return {
        "first_pair": first_pair,
        "pair_count": 4,
        "games": 8,
        "diagnostics": 2,
        "shadow_noninterference": "byte_identical_export_streams",
        "outcome_sha256": outcome,
        "teacher_sha256": teacher,
        "reference_outcome_sha256": outcome,
        "reference_teacher_sha256": teacher,
        "log_path": f"log-{first_pair}",
        "outcome_path": f"outcome-{first_pair}",
        "teacher_path": f"teacher-{first_pair}",
    }


def _report(workers: int) -> dict:
    return {
        "schema": screen._COLLECTOR.SCHEMA,
        "status": "complete",
        "base_seed": 1_400_001,
        "pair_start": 0,
        "pairs": 16,
        "games": 32,
        "workers": workers,
        "task_pairs": 4,
        "search_diagnostics": 8,
        "tasks": [
            _task(first, f"{first + 1:064x}", f"{first + 101:064x}")
            for first in (0, 4, 8, 12)
        ],
    }


class _FakeMemory:
    available = 8_000


class _FakeProcess:
    def __init__(self, rss: int) -> None:
        self.rss = rss

    def children(self, recursive: bool = False) -> list[_FakeProcess]:
        assert recursive
        return []

    def memory_info(self) -> object:
        return type("Memory", (), {"rss": self.rss})()


class _FakePsutil:
    @staticmethod
    def cpu_percent(interval: object = None) -> float:
        return 37.5

    @staticmethod
    def virtual_memory() -> _FakeMemory:
        return _FakeMemory()

    @staticmethod
    def Process(pid: int) -> _FakeProcess:
        return _FakeProcess(4_000)


class _FakeRunner:
    returncode = 0
    stdout = "1, 42, 512\n"
    stderr = ""

    def __call__(self, *args: object, **kwargs: object) -> _FakeRunner:
        return self


class _RunningChild:
    pid = 123

    @staticmethod
    def poll() -> None:
        return None


class _UnkillableChild:
    pid = 456

    @staticmethod
    def poll() -> None:
        return None

    @staticmethod
    def wait(timeout: float) -> None:
        raise __import__("subprocess").TimeoutExpired("child", timeout)

    @staticmethod
    def terminate() -> None:
        return None

    @staticmethod
    def kill() -> None:
        return None


class ScreenTopologyTests(unittest.TestCase):
    def test_canonical_digest_is_order_independent(self) -> None:
        rows = [
            {"episode_id": 2, "step": 1, "physical_decision_id": 3, "substep_index": 0, "candidate_seat": "p0", "model_input_sha256": "b", "value": 2},
            {"episode_id": 1, "step": 1, "physical_decision_id": 3, "substep_index": 0, "candidate_seat": "p0", "model_input_sha256": "a", "value": 1},
        ]
        self.assertEqual(screen._canonical_label_digest(rows), screen._canonical_label_digest(list(reversed(rows))))

    def test_validates_exact_ranges_and_noninterference(self) -> None:
        report = _report(1)
        with tempfile.TemporaryDirectory() as directory:
            result = screen._validate_task_bindings(
                report,
                Path(directory) / "report.json",
                base_seed=1_400_001,
                pair_start=0,
                pairs=16,
                workers=1,
                validate_artifacts=False,
            )
        self.assertEqual(result["task_ranges"], [[0, 4], [4, 4], [8, 4], [12, 4]])
        self.assertIsNone(result["label_digest"])

    def test_rejects_root_range_mismatch(self) -> None:
        report = _report(1)
        report["pairs"] = 15
        with self.assertRaisesRegex(RuntimeError, "root binding mismatch"):
            screen._validate_task_bindings(
                report,
                Path("report.json"),
                base_seed=1_400_001,
                pair_start=0,
                pairs=16,
                workers=1,
                validate_artifacts=False,
            )

    def test_monitor_sample_and_summary_use_all_required_fields(self) -> None:
        sample = screen._sample_process(
            _FakeProcess(4_000),
            _FakePsutil,
            runner=_FakeRunner(),
        )
        self.assertEqual(sample["gpu_utilization_percent"], 42.0)
        self.assertEqual(sample["process_tree_rss_bytes"], 4_000.0)
        summary = screen._summary([sample, {**sample, "gpu_utilization_percent": 58.0}])
        self.assertEqual(summary["sample_count"], 2)
        self.assertEqual(summary["gpu_utilization_percent"]["maximum"], 58.0)
        self.assertEqual(summary["system_available_memory_bytes"]["minimum"], 8_000.0)

    def test_monitor_enforces_the_child_timeout_while_process_is_live(self) -> None:
        clock = iter((0.0, 0.0, 1.0))
        with self.assertRaisesRegex(RuntimeError, "exceeded 1 second timeout"):
            screen._monitor_process(
                _RunningChild(),
                _FakePsutil,
                poll_seconds=0.1,
                timeout_seconds=1.0,
                runner=_FakeRunner(),
                sleep=lambda _: None,
                monotonic=lambda: next(clock),
            )

    def test_monitor_accepts_rss_disappearance_only_after_child_exit(self) -> None:
        class GoneProcess(Exception):
            pass

        class VanishedProcess(_FakeProcess):
            def children(self, recursive: bool = False) -> list[_FakeProcess]:
                assert recursive
                raise GoneProcess("pid vanished")

        class RacePsutil(_FakePsutil):
            process_calls = 0

            @classmethod
            def Process(cls, pid: int) -> _FakeProcess:
                cls.process_calls += 1
                return _FakeProcess(4_000) if cls.process_calls == 1 else VanishedProcess(0)

        class RaceChild:
            pid = 789

            def __init__(self) -> None:
                self.polls = iter((None, None, None, None, 0))

            def poll(self) -> int | None:
                return next(self.polls, 0)

        samples = screen._monitor_process(
            RaceChild(),
            RacePsutil,
            poll_seconds=0.1,
            runner=_FakeRunner(),
            sleep=lambda _: None,
        )
        self.assertEqual(len(samples), 1)
        self.assertEqual(samples[0]["process_tree_rss_bytes"], 4_000.0)

    def test_terminate_fails_closed_when_process_remains_live(self) -> None:
        failed_taskkill = type("Completed", (), {"returncode": 1})()
        with (
            mock.patch.object(screen.os, "name", "nt"),
            mock.patch.object(screen.subprocess, "run", return_value=failed_taskkill),
        ):
            with self.assertRaisesRegex(RuntimeError, "remains live after termination"):
                screen._terminate(_UnkillableChild())

    def test_cross_topology_requires_identical_bindings(self) -> None:
        topologies = [
            {
                "workers": worker,
                "search_diagnostics": 8,
                "label_digest": "same",
                "task_hashes": {"a": "b"},
                "task_ranges": [[0, 4]],
                "provenance": {"same": True},
            }
            for worker in screen.EXPECTED_WORKERS
        ]
        screen._require_cross_topology_identity(topologies)
        topologies[2]["label_digest"] = "different"
        with self.assertRaisesRegex(RuntimeError, "label_digest"):
            screen._require_cross_topology_identity(topologies)

    def test_cross_topology_rejects_provenance_drift(self) -> None:
        topologies = [
            {
                "workers": worker,
                "search_diagnostics": 8,
                "label_digest": "same",
                "task_hashes": {"a": "b"},
                "task_ranges": [[0, 4]],
                "provenance": {"collector": "same"},
            }
            for worker in screen.EXPECTED_WORKERS
        ]
        topologies[2]["provenance"] = {"collector": "different"}
        with self.assertRaisesRegex(RuntimeError, "provenance"):
            screen._require_cross_topology_identity(topologies)

    def test_plan_only_prints_three_commands_without_execution(self) -> None:
        with mock.patch.object(screen._COLLECTOR, "build_topology_screen_commands", return_value=[["one"], ["two"], ["four"]]) as builder, mock.patch.object(screen.subprocess, "Popen") as popen:
            with mock.patch("sys.stdout") as output:
                self.assertEqual(
                    screen.main(
                        [
                            "--report", "report.json",
                            "--revealed-root", "revealed",
                            "--mage-repo", "mage",
                            "--scorer-exe", "scorer",
                            "--reference-scorer-exe", "reference",
                            "--outcome-root", "outcome",
                            "--source-database", "database",
                            "--maven", "mvn",
                        ]
                    ),
                    0,
                )
            builder.assert_called_once()
            popen.assert_not_called()


if __name__ == "__main__":
    unittest.main()
