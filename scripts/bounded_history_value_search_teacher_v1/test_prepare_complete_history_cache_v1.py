#!/usr/bin/env python3
"""Synthetic tests for the complete-history search-teacher cache preparer."""

from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import torch


SCRIPT = Path(__file__).with_name("prepare_complete_history_cache_v1.py")
SPEC = importlib.util.spec_from_file_location("prepare_search_teacher_cache", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
preparer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(preparer)


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _join(pair_count: int) -> dict:
    return {
        "episode_count": pair_count * 2,
        "pair_count": pair_count,
        "policy_step_count": pair_count * 2,
        "physical_decision_count": pair_count * 2,
        "selected_action_kind_counts": {"pass": pair_count * 2},
        "selected_semantics_public": True,
        "terminal_replays_exact": True,
        "complete_policy_steps": True,
        "complete_physical_decisions": True,
    }


def _fixture(root: Path, *, task_ranges: list[tuple[int, int]] | None = None) -> tuple[Path, dict]:
    if task_ranges is None:
        task_ranges = [(0, preparer.PAIR_COUNT)]
    shared_teacher = b"teacher reference and shadow\n"
    shared_outcome = b"outcome reference and shadow\n"
    paths = {
        "reference_teacher_path": root / "reference.teacher.jsonl",
        "teacher_path": root / "shadow.teacher.jsonl",
        "reference_outcome_path": root / "reference.outcome.jsonl",
        "outcome_path": root / "shadow.outcome.jsonl",
        "reference_log_path": root / "reference.log",
        "log_path": root / "shadow.log",
    }
    for key in ("reference_teacher_path", "teacher_path"):
        paths[key].write_bytes(shared_teacher)
    for key in ("reference_outcome_path", "outcome_path"):
        paths[key].write_bytes(shared_outcome)
    paths["reference_log_path"].write_bytes(b"reference log\n")
    paths["log_path"].write_bytes(b"shadow log\n")

    tasks = []
    for first_pair, pair_count in task_ranges:
        task = {
            "first_pair": first_pair,
            "pair_count": pair_count,
            "shadow_noninterference": preparer.SHADOW_NONINTERFERENCE,
        }
        for path_key, hash_key in preparer.PATH_HASH_FIELDS:
            task[path_key] = str(paths[path_key])
            task[hash_key] = _sha256(paths[path_key])
        tasks.append(task)
    report = {
        "schema": preparer.CORPUS_SCHEMA,
        "status": "complete",
        "base_seed": preparer.BASE_SEED,
        "pair_start": preparer.PAIR_START,
        "pairs": preparer.PAIR_COUNT,
        "target_quality_gate": {
            "minimum_equal_episode_mass_override_rate_per_seat": 0.05,
            "p0": True,
            "p1": True,
            "pass": True,
        },
        "advance_to_training": True,
        "by_candidate_seat": {
            "p0": {"equal_episode_mass_override_rate": 0.10},
            "p1": {"equal_episode_mass_override_rate": 0.10},
        },
        "tasks": tasks,
    }
    report_path = root / "collector-report.json"
    report_path.write_text(json.dumps(report), encoding="utf-8")
    return report_path, report


def _mock_loaders():
    def load_teacher(path: Path):
        rows = [
            {
                "pair_index": pair,
                "object_card_ids": torch.tensor([pair], dtype=torch.long),
                "object_groups": torch.tensor([pair], dtype=torch.long),
            }
            for pair in range(preparer.PAIR_COUNT)
        ]
        return rows, {}

    def load_outcome(path: Path):
        rows = [
            {
                "pair_index": pair,
                "object_card_ids": torch.tensor([pair], dtype=torch.long),
                "object_groups": torch.tensor([pair], dtype=torch.long),
            }
            for pair in range(preparer.PAIR_COUNT)
        ]
        return rows, {}

    def validate_join(policy, value, teacher_terminals, outcome_terminals):
        return _join(len({int(row["pair_index"]) for row in policy}))

    return load_teacher, load_outcome, validate_join


class PrepareCompleteHistoryCacheTests(unittest.TestCase):
    def test_prepares_exact_version_lanes_and_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            collector_report, _ = _fixture(root)
            cache = root / "cache.pt"
            report = root / "cache-report.json"
            load_teacher, load_outcome, validate_join = _mock_loaders()
            with (
                mock.patch.object(preparer.structured, "_load_teacher", load_teacher),
                mock.patch.object(preparer.structured, "_load_outcome", load_outcome),
                mock.patch.object(
                    preparer.structured,
                    "_validate_complete_history_join",
                    validate_join,
                ),
            ):
                result = preparer.prepare(
                    type(
                        "Args",
                        (),
                        {"collector_report": collector_report, "cache": cache, "report": report},
                    )()
                )
            payload = torch.load(cache, map_location="cpu", weights_only=False)
            self.assertEqual(payload["version"], preparer.structured.SCRIPT_VERSION)
            self.assertEqual(len(payload["policy"]), preparer.PAIR_COUNT)
            self.assertEqual(len(payload["value"]), preparer.PAIR_COUNT)
            self.assertEqual(result["cache_sha256"], _sha256(cache))
            self.assertEqual(result["collector_report_sha256"], _sha256(collector_report))
            self.assertEqual(payload["source"]["collector_report_sha256"], _sha256(collector_report))
            self.assertEqual(len(result["tasks"]), 1)
            self.assertFalse((root / "cache.pt.tmp").exists())
            self.assertFalse((root / "cache-report.json.tmp").exists())

    def test_requires_quality_gate_pass(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            collector_report, report_data = _fixture(root)
            report_data["target_quality_gate"]["pass"] = False
            collector_report.write_text(json.dumps(report_data), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "target-quality gate"):
                preparer.prepare(
                    type(
                        "Args",
                        (),
                        {
                            "collector_report": collector_report,
                            "cache": root / "cache.pt",
                            "report": root / "cache-report.json",
                        },
                    )()
                )

    def test_requires_disjoint_exact_task_coverage(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            collector_report, _ = _fixture(root, task_ranges=[(0, 128), (127, 129)])
            def load_teacher(path: Path):
                return (
                    [
                        {
                            "pair_index": pair,
                            "object_card_ids": torch.tensor([pair]),
                            "object_groups": torch.tensor([pair]),
                        }
                        for pair in range(128)
                    ],
                    {},
                )

            with (
                mock.patch.object(preparer.structured, "_load_teacher", load_teacher),
                mock.patch.object(preparer.structured, "_load_outcome", load_teacher),
                mock.patch.object(
                    preparer.structured,
                    "_validate_complete_history_join",
                    lambda policy, value, teacher, outcome: _join(128),
                ),
                self.assertRaisesRegex(RuntimeError, "pair ranges overlap"),
            ):
                preparer.prepare(
                    type(
                        "Args",
                        (),
                        {
                            "collector_report": collector_report,
                            "cache": root / "cache.pt",
                            "report": root / "cache-report.json",
                        },
                    )()
                )

    def test_requires_reference_streams_to_match_shadow_streams(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            collector_report, report_data = _fixture(root)
            reference = root / "reference.teacher.jsonl"
            reference.write_bytes(b"different reference\n")
            task = report_data["tasks"][0]
            task["reference_teacher_sha256"] = _sha256(reference)
            collector_report.write_text(json.dumps(report_data), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "byte-identical"):
                preparer.prepare(
                    type(
                        "Args",
                        (),
                        {
                            "collector_report": collector_report,
                            "cache": root / "cache.pt",
                            "report": root / "cache-report.json",
                        },
                    )()
                )

    def test_requires_every_task_file_hash_binding(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            collector_report, report_data = _fixture(root)
            report_data["tasks"][0]["log_sha256"] = "0" * 64
            collector_report.write_text(json.dumps(report_data), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "task hash mismatch"):
                preparer.prepare(
                    type(
                        "Args",
                        (),
                        {
                            "collector_report": collector_report,
                            "cache": root / "cache.pt",
                            "report": root / "cache-report.json",
                        },
                    )()
                )


if __name__ == "__main__":
    unittest.main()
