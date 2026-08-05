from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock


SCRIPT_DIR = Path(__file__).resolve().parent
MODULE_PATH = SCRIPT_DIR / "run_cp7_gate_v4.py"
SPEC = importlib.util.spec_from_file_location("run_cp7_gate_v4", MODULE_PATH)
GATE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(GATE)

from _test_support import shard_rows, write_rows  # noqa: E402


def terminal(pair: int, seat: str, reward: int, seed: str | None = None) -> dict:
    return {
        "pair_index": pair,
        "episode_id": pair * 2 + int(seat == "p1"),
        "candidate_seat": seat,
        "base_seed_u64_hex": f"{GATE.PANEL['base_seed']:016x}",
        "pair_environment_seed_u64_hex": seed or f"{pair + 1:016x}",
        "deck_ids": ["Rally", "Rally"],
        "randomization_identity": "environment-randomization-v2",
        "candidate_terminal_reward": reward,
    }


def panel(candidate_rewards: list[int], baseline_rewards: list[int]):
    candidate = {}
    baseline = {}
    for index, (candidate_reward, baseline_reward) in enumerate(
        zip(candidate_rewards, baseline_rewards)
    ):
        pair = index // 2
        seat = "p0" if index % 2 == 0 else "p1"
        key = (pair, seat)
        candidate[key] = terminal(pair, seat, candidate_reward)
        baseline[key] = terminal(pair, seat, baseline_reward)
    return candidate, baseline


class Cp7GateV4Tests(unittest.TestCase):
    def test_formal_partition_is_eight_total_tasks(self) -> None:
        self.assertEqual(GATE._chunk_ranges(), [(0, 32), (32, 32), (64, 32), (96, 32)])
        self.assertEqual(len(GATE._chunk_ranges()) * len(GATE.ARM_ORDER), 8)
        self.assertEqual(GATE.TOPOLOGY["workers"], 8)

    def test_all_gates_pass_at_exact_positive_margins(self) -> None:
        candidate, baseline = panel([1, 1, 1, 1], [-1, -1, -1, -1])
        result = GATE.adjudicate(candidate, baseline)
        self.assertEqual(result["terminal_order"]["nets"], {"overall": 4, "p0": 2, "p1": 2})
        self.assertEqual(result["win_margin"], 4)
        self.assertTrue(result["pass"])

    def test_terminal_order_cannot_substitute_for_win_margin(self) -> None:
        candidate, baseline = panel([0, 0, 0, 0], [-1, -1, -1, -1])
        result = GATE.adjudicate(candidate, baseline)
        self.assertEqual(result["terminal_order"]["nets"]["overall"], 4)
        self.assertEqual(result["win_margin"], 0)
        self.assertTrue(result["gates"]["terminal_order_net_floor"])
        self.assertFalse(result["gates"]["win_margin_floor"])
        self.assertFalse(result["pass"])

    def test_each_seat_floor_is_independent(self) -> None:
        candidate, baseline = panel(
            [1, -1, 1, -1, 1, -1, 1, 0, 1, 0, 1, 0, 1, 0],
            [-1, 1, -1, 1, -1, 1, -1, 0, -1, 0, -1, 0, -1, 0],
        )
        result = GATE.adjudicate(candidate, baseline)
        self.assertEqual(result["terminal_order"]["nets"], {"overall": 4, "p0": 7, "p1": -3})
        self.assertEqual(result["win_margin"], 4)
        self.assertFalse(result["gates"]["p1_terminal_order_net_floor"])
        self.assertFalse(result["pass"])

    def test_cross_arm_receipt_mismatch_fails_closed(self) -> None:
        candidate, baseline = panel([1, 1, 1, 1], [-1, -1, -1, -1])
        baseline[(0, "p0")]["pair_environment_seed_u64_hex"] = "f" * 16
        with self.assertRaisesRegex(ValueError, "matched CP7 receipt differs"):
            GATE.adjudicate(candidate, baseline)

    def test_cross_arm_deck_receipt_mismatch_fails_closed(self) -> None:
        candidate, baseline = panel([1, 1, 1, 1], [-1, -1, -1, -1])
        baseline[(0, "p0")]["deck_ids"] = ["Rally", "Other"]
        with self.assertRaisesRegex(ValueError, "matched CP7 receipt differs"):
            GATE.adjudicate(candidate, baseline)

    def test_candidate_outcome_header_and_rows_validate(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "candidate.jsonl"
            rows = copy.deepcopy(shard_rows(pair=0, base_seed=GATE.PANEL["base_seed"]))
            checkpoint = GATE.expected_checkpoint("candidate")
            for row in rows:
                row["checkpoint"] = copy.deepcopy(checkpoint)
            write_rows(path, rows)
            report = GATE._validate_outcome_shard(
                path, arm="candidate", first_pair=0, pair_count=1
            )
            self.assertEqual(report["episode_count"], 2)
            self.assertEqual(set(report["terminals"]), {(0, "p0"), (0, "p1")})

    def test_baseline_outcome_header_and_rows_validate(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "baseline.jsonl"
            write_rows(path, shard_rows(pair=0, base_seed=GATE.PANEL["base_seed"]))
            report = GATE._validate_outcome_shard(
                path, arm="baseline", first_pair=0, pair_count=1
            )
            self.assertEqual(report["episode_count"], 2)

    def test_candidate_checkpoint_tamper_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "candidate.jsonl"
            rows = copy.deepcopy(shard_rows(pair=0, base_seed=GATE.PANEL["base_seed"]))
            checkpoint = GATE.expected_checkpoint("candidate")
            for row in rows:
                row["checkpoint"] = copy.deepcopy(checkpoint)
            rows[-1]["checkpoint"]["loaded_payload_sha256"] = "0" * 64
            write_rows(path, rows)
            with self.assertRaisesRegex(ValueError, "checkpoint mismatch"):
                GATE._validate_outcome_shard(
                    path, arm="candidate", first_pair=0, pair_count=1
                )

    def test_run_writes_sealed_state_before_any_outcome_parse(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "formal"
            root.mkdir()
            manifest_path = root / "manifest.json"
            manifest_path.write_text("{}\n", encoding="utf-8")
            validated = {
                "manifest": {"prerequisites": {}, "nonclaims": []},
                "root": root,
                "report_path": root / "report.json",
                "packages": {arm: {"checkpoint": GATE.expected_checkpoint(arm)} for arm in GATE.ARM_ORDER},
            }

            def fake_collect(_validated):
                tasks = []
                for arm in GATE.ARM_ORDER:
                    log = root / f"{arm}.log"
                    shard = root / f"{arm}.jsonl"
                    log.write_bytes(b"")
                    shard.write_bytes(b"")
                    tasks.append(
                        {
                            "arm": arm,
                            "worker": 0,
                            "first_pair": 0,
                            "pair_count": GATE.PANEL["pairs"],
                            "game_count": GATE.PANEL["episodes_per_arm"],
                            "elapsed_seconds": 1.0,
                            "log": {"path": str(log), "sha256": GATE.common.sha256(log), "byte_count": 0},
                            "outcome": {"path": str(shard), "sha256": GATE.common.sha256(shard), "byte_count": 0},
                        }
                    )
                return tasks, [{}], 1.0

            def fake_parse(path: Path, *, arm: str, first_pair: int, pair_count: int):
                state = json.loads((root / "state.json").read_text(encoding="utf-8"))
                self.assertFalse(state["outcomes_parsed"])
                terminals = {
                    (pair, seat): terminal(pair, seat, 0)
                    for pair in range(GATE.PANEL["pairs"])
                    for seat in ("p0", "p1")
                }
                return {
                    "sha256": GATE.common.sha256(path),
                    "byte_count": 0,
                    "record_count": 1,
                    "decision_count": 1,
                    "episode_count": len(terminals),
                    "terminals": terminals,
                }

            with (
                mock.patch.object(GATE, "validate_manifest", return_value=validated),
                mock.patch.object(GATE, "_collect", side_effect=fake_collect),
                mock.patch.object(GATE.common, "_validate_log_markers"),
                mock.patch.object(GATE, "_validate_outcome_shard", side_effect=fake_parse),
                mock.patch.object(GATE.common, "_resource_summary", return_value={"status": "test"}),
            ):
                report = GATE.run(manifest_path)
            self.assertEqual(report["status"], "fail")
            state_final = json.loads((root / "state-final.json").read_text(encoding="utf-8"))
            self.assertTrue(state_final["outcomes_parsed"])

    def test_task_log_is_rehashed_before_marker_validation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "task.log"
            path.write_text("original\n", encoding="utf-8")
            task = {
                "first_pair": 0,
                "pair_count": 1,
                "log": {
                    "path": str(path),
                    "sha256": GATE.common.sha256(path),
                    "byte_count": path.stat().st_size,
                },
            }
            path.write_text("tampered\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "changed after collection"):
                GATE._validate_task_log(task)

    def test_failed_gate_returns_distinct_process_status(self) -> None:
        with mock.patch.object(GATE, "run", return_value={"status": "fail"}):
            self.assertEqual(GATE.main(["--manifest", "unused.json"]), 3)


if __name__ == "__main__":
    unittest.main()
