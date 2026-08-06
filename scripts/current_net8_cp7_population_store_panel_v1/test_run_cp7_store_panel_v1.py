#!/usr/bin/env python3
"""Focused offline checks for the population Store CP7 panel runner."""

from __future__ import annotations

import concurrent.futures
import importlib.util
import json
from pathlib import Path
import tempfile
import threading
import time
import types
import unittest


MODULE = Path(__file__).with_name("run_cp7_store_panel_v1.py")
SPEC = importlib.util.spec_from_file_location("panel", MODULE)
assert SPEC is not None and SPEC.loader is not None
panel = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(panel)


class PanelRunnerTest(unittest.TestCase):
    def setUp(self) -> None:
        self.checkpoint = {
            "authority_kind": panel.AUTHORITY_KIND,
            "source_run_sha256": "1" * 64, "source_generation": 1024,
            "source_checkpoint_sha256": "2" * 64,
            "source_sidecar_sha256": "3" * 64,
            "source_payload_sha256": "4" * 64,
            "source_train_state_sha256": "5" * 64,
            "loaded_run_sha256": "1" * 64, "loaded_generation": 1024,
            "loaded_checkpoint_sha256": "2" * 64,
            "loaded_payload_sha256": "4" * 64,
            "loaded_train_state_sha256": "5" * 64,
            "model_parameter_sha256": "6" * 64,
            "environment_trajectory_contract": panel.ENVIRONMENT_CONTRACT,
            "sampler_identity": panel.SAMPLER_IDENTITY,
            "sampler_contract_sha256": panel.SAMPLER_CONTRACT,
        }
        self.model = {"root": "store", "checkpoint": self.checkpoint}

    def _terminal(self, pair: int, seat: str, ordinal: int) -> dict[str, object]:
        episode = pair * 2 + int(seat[1])
        reward = 1 if seat == "p0" else -1
        return {
            "record_type": "terminal", "schema_version": 2,
            "record_ordinal": ordinal, "pair_index": pair, "episode_id": episode,
            "candidate_seat": seat, "base_seed_u64_hex": "0000000000000007",
            "pair_environment_seed_u64_hex": f"{pair + 100:016x}",
            "deck_ids": ["Rally", "Rally"], "randomization_identity": "legacy_v1",
            "core_environment_hash_u64_hex": "0" * 16,
            "diagnostic_state_hash_u64_hex": "1" * 16,
            "first_outcome_decision_ordinal": None, "outcome_decision_count": 0,
            "terminal": {"schema_version": 5, "episode_id": episode,
                         "terminal_classification": "natural",
                         "terminal_code": "natural_game_over", "terminal_reason": "game_over",
                         "terminal_outcome": "p0_win", "winner": "p0",
                         "terminal_reward": [1, -1]},
            "candidate_terminal_reward": reward, "checkpoint": self.checkpoint,
        }

    def _rows(self, pairs: int = 2) -> list[dict[str, object]]:
        rows: list[dict[str, object]] = [panel.expected_header(self.checkpoint)]
        for pair in range(pairs):
            rows.extend((self._terminal(pair, "p0", len(rows)),
                         self._terminal(pair, "p1", len(rows) + 1)))
        return rows

    @staticmethod
    def _write(path: Path, rows: list[dict[str, object]]) -> None:
        with path.open("wb") as handle:
            for row in rows:
                handle.write(json.dumps(row, sort_keys=True, separators=(",", ":")).encode())
                handle.write(b"\n")

    def test_task_pairs_changes_contiguous_shard_coverage(self) -> None:
        chunks = panel.chunk_ranges(0, 128, 32)
        self.assertEqual(chunks, [(0, 32), (32, 32), (64, 32), (96, 32)])
        tasks = panel.planned_tasks(["a", "b", "c"], chunks)
        self.assertEqual(len(tasks), 12)
        self.assertTrue(all(sum(task["pair_count"] for task in tasks
                                if task["label"] == label) == 128
                            for label in ("a", "b", "c")))
        args = types.SimpleNamespace(mage_repo=Path("mage"), scorer_exe=Path("scorer"),
                                     generation=1024, base_seed=7, maven=Path("mvn"))
        command = panel.anchor_command(args, self.model, 32, 32, Path("outcome.jsonl"))
        self.assertIn("--first-episode 64", command[-1])
        self.assertIn("--pairs 32", command[-1])

    def test_database_leases_cannot_overlap(self) -> None:
        leases = panel.DatabaseLeasePool([Path("db-0")])
        active = 0
        maximum = 0
        lock = threading.Lock()

        def use_database() -> None:
            nonlocal active, maximum
            worker, root = leases.acquire()
            try:
                with lock:
                    active += 1
                    maximum = max(maximum, active)
                time.sleep(0.01)
                with lock:
                    active -= 1
            finally:
                leases.release(worker, root)

        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
            list(executor.map(lambda _: use_database(), range(8)))
        self.assertEqual(maximum, 1)

    def test_strict_outcome_accepts_exact_terminal_only_shard(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "outcome.jsonl"
            self._write(path, self._rows())
            result = panel.validate_outcome_shard(
                path, self.model, base_seed=7, first_pair=0, pair_count=2)
            self.assertEqual(result["pair_count"], 2)
            self.assertEqual(len(result["outcomes"]), 4)

    def test_duplicate_missing_interleaved_and_malformed_terminals_fail(self) -> None:
        cases: dict[str, list[dict[str, object]]] = {}
        duplicate = self._rows()
        duplicate.insert(2, dict(duplicate[1]))
        cases["duplicate"] = duplicate
        cases["missing"] = self._rows()[:-1]
        interleaved = self._rows()
        interleaved[2], interleaved[3] = interleaved[3], interleaved[2]
        cases["interleaved"] = interleaved
        malformed = self._rows()
        malformed[2] = dict(malformed[2])
        malformed[2]["candidate_terminal_reward"] = 1
        cases["malformed-reward"] = malformed
        for rows in cases.values():
            for ordinal, row in enumerate(rows):
                row["record_ordinal"] = ordinal
        with tempfile.TemporaryDirectory() as directory:
            for name, rows in cases.items():
                with self.subTest(name=name):
                    path = Path(directory) / f"{name}.jsonl"
                    self._write(path, rows)
                    with self.assertRaises(ValueError):
                        panel.validate_outcome_shard(
                            path, self.model, base_seed=7, first_pair=0, pair_count=2)

    def test_terminal_wdl_aggregation(self) -> None:
        summary = panel.aggregate_terminal_wdl([
            {"label": "a", "by_seat": {"p0": "win", "p1": "loss"}},
            {"label": "a", "by_seat": {"p0": "draw", "p1": "draw"}},
        ], ["a"])
        self.assertEqual(summary["a"]["overall_wdl"], {"win": 1, "draw": 2, "loss": 1})

    def test_duplicate_model_labels_and_roots_fail_before_store_access(self) -> None:
        common = ["--evidence-root", "new", "--generation", "1024", "--mode", "smoke",
                  "--base-seed", "1", "--pairs", "1", "--scorer-exe", "scorer",
                  "--mage-repo", "mage", "--source-database", "cards", "--maven", "mvn"]
        with self.assertRaises(ValueError):
            panel.main(common + ["--model", "same=one", "--model", "same=two",
                                 "--model", "third=three"])
        with self.assertRaises(ValueError):
            panel.main(common + ["--model", "one=root", "--model", "two=root",
                                 "--model", "three=other"])


if __name__ == "__main__":
    unittest.main()
