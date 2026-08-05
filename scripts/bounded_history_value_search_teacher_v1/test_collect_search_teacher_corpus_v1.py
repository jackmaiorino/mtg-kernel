from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("collect_search_teacher_corpus_v1.py")
SPEC = importlib.util.spec_from_file_location("search_teacher_collector", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
collector = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(collector)


def _action(name: str) -> dict[str, str]:
    return {"kind": name}


def _header(contract: str, selection_source: str) -> dict:
    return {
        "record_type": "header",
        "record_ordinal": 0,
        "export_contract": contract,
        "selection_source": selection_source,
        "checkpoint": {
            "authority_kind": collector.EXPECTED_AUTHORITY_KIND,
            "loaded_checkpoint_sha256": collector.EXPECTED_PACKAGE_MANIFEST_SHA256,
        },
    }


def _decision(
    episode: int,
    seat: str,
    acting: str,
    selected: int,
    *,
    record_ordinal: int,
    decision_ordinal: int,
    opponent: bool,
) -> dict:
    semantics = [_action("pass"), _action("cast")]
    return {
        "record_type": "decision",
        "record_ordinal": record_ordinal,
        "pair_index": 0,
        "episode_id": episode,
        "step": 3,
        "environment_revision": 7,
        "physical_decision_id": 2,
        "substep_index": 0,
        "substep_count": 1,
        "actor_physical_decision_ordinal": 20,
        "candidate_seat": seat,
        "acting_player": acting,
        "legal_action_count": 2,
        "selected_index": selected,
        "selected_semantic": semantics[selected],
        "action_semantics": semantics,
        "model_input_sha256": f"{episode:064x}",
        "candidate_order_commitment_128_hex": f"{episode + 1:032x}",
        ("teacher_decision_ordinal" if opponent else "outcome_decision_ordinal"): decision_ordinal,
    }


def _terminal(pair: int, seat: str, record_ordinal: int, decision_ordinal: int) -> dict:
    rewards = [1, -1]
    return {
        "record_type": "terminal",
        "record_ordinal": record_ordinal,
        "pair_index": pair,
        "episode_id": pair * 2 + int(seat[-1]),
        "candidate_seat": seat,
        "first_outcome_decision_ordinal": decision_ordinal,
        "outcome_decision_count": 1,
        "candidate_terminal_reward": rewards[int(seat[-1])],
        "terminal": {
            "terminal_classification": "natural",
            "terminal_code": "natural_game_over",
            "terminal_reward": rewards,
        },
    }


def _fixture() -> tuple[list[dict], list[dict], list[dict]]:
    outcome = [
        _header(
            "mtg-kernel-xmage-cp7-outcome-jsonl/v2",
            collector.EXPECTED_OUTCOME_SELECTION_SOURCE,
        )
    ]
    teacher = [
        _header(
            "mtg-kernel-xmage-cp7-teacher-jsonl/v1",
            collector.EXPECTED_TEACHER_SELECTION_SOURCE,
        )
    ]
    diagnostics = []
    for seat_number in (0, 1):
        seat = f"p{seat_number}"
        episode = seat_number
        outcome_row = _decision(
            episode,
            seat,
            seat,
            1,
            record_ordinal=seat_number * 2 + 1,
            decision_ordinal=seat_number,
            opponent=False,
        )
        outcome.append(outcome_row)
        teacher.append(
            _decision(
                episode,
                seat,
                f"p{1 - seat_number}",
                0,
                record_ordinal=seat_number * 2 + 1,
                decision_ordinal=seat_number,
                opponent=True,
            )
        )
        diagnostics.append(
            {
                "schema": collector.DIAGNOSTIC_SCHEMA,
                "pair_index": 0,
                "episode_id": episode,
                "step": 3,
                "environment_revision": 7,
                "physical_decision_id": 2,
                "substep_index": 0,
                "substep_count": 1,
                "actor_physical_decision_ordinal": 20,
                "candidate_seat": seat,
                "legal_action_count": 2,
                "continuation_steps": 8,
                "information_set_samples": 4,
                "information_set_sample_hashes_u64_hex": [f"{i:016x}" for i in range(4)],
                "fallback_selected_index": 1,
                "best_search_index": 0,
                "teacher_selected_index": 0,
                "teacher_differs_from_fallback": True,
                "search_margin_f32_bits_hex": "3e800000",
                "action_values_f32_bits_hex": ["3ec00000", "3e000000"],
                "branch_count": 8,
                "natural_terminal_branch_count": 3,
                "critic_bootstrap_branch_count": 5,
                "candidate_action_seed_u64_hex": f"{episode + 9:016x}",
                "candidate_order_commitment_128_hex": outcome_row[
                    "candidate_order_commitment_128_hex"
                ],
                "model_input_sha256": outcome_row["model_input_sha256"],
                "public_history_sha256": f"{episode + 17:064x}",
                "diagnostic_state_hash_u64_hex": f"{episode + 11:016x}",
                "core_environment_hash_u64_hex": f"{episode + 13:016x}",
                "action_semantics": outcome_row["action_semantics"],
            }
        )
        outcome.append(_terminal(0, seat, seat_number * 2 + 2, seat_number))
        teacher.append(_terminal(0, seat, seat_number * 2 + 2, seat_number))
    return outcome, teacher, diagnostics


class CollectorTests(unittest.TestCase):
    def test_environment_binds_exact_bounded_package_outcome_identity(self) -> None:
        environment = collector._environment(Path("worker-db"))
        expected = {
            "xmage.rally.cp7Outcome.authorityKind": collector.EXPECTED_OUTCOME_IDENTITY[
                "authority_kind"
            ],
            "xmage.rally.cp7Outcome.adamStep": collector.EXPECTED_OUTCOME_IDENTITY[
                "adam_step"
            ],
            "xmage.rally.cp7Outcome.manifestSha256": collector.EXPECTED_OUTCOME_IDENTITY[
                "manifest"
            ],
            "xmage.rally.cp7Outcome.payloadSha256": collector.EXPECTED_OUTCOME_IDENTITY[
                "payload"
            ],
            "xmage.rally.cp7Outcome.trainStateSha256": collector.EXPECTED_OUTCOME_IDENTITY[
                "train_state"
            ],
            "xmage.rally.cp7Outcome.modelParameterSha256": collector.EXPECTED_OUTCOME_IDENTITY[
                "model"
            ],
        }
        observed = {
            item[2:].split("=", 1)[0]: item.split("=", 1)[1]
            for item in environment["MAVEN_OPTS"].split()
        }
        self.assertEqual(observed, expected)

    def test_validates_search_diagnostics_and_aggregates_by_seat(self) -> None:
        outcome, teacher, diagnostics = _fixture()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outcome_path = root / "outcome.jsonl"
            teacher_path = root / "teacher.jsonl"
            log_path = root / "task.log"
            for path, rows in ((outcome_path, outcome), (teacher_path, teacher)):
                path.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")
            log_path.write_text(
                "[bridge] "
                + collector.DIAGNOSTIC_PREFIX
                + json.dumps(diagnostics[0])
                + "\n"
                + collector.DIAGNOSTIC_PREFIX
                + json.dumps(diagnostics[1])
                + "\n",
                encoding="utf-8",
            )
            result = collector._validate_task_outputs(log_path, outcome_path, teacher_path, 0, 1)
        self.assertEqual(result["diagnostics"], 2)
        self.assertEqual(result["by_candidate_seat"]["p0"]["overrides"], 1)
        self.assertEqual(result["by_candidate_seat"]["p1"]["override_rate"], 1.0)

    def test_rejects_duplicate_information_set_hash(self) -> None:
        outcome, teacher, diagnostics = _fixture()
        diagnostics[0]["information_set_sample_hashes_u64_hex"][1] = "0000000000000000"
        diagnostics[0]["information_set_sample_hashes_u64_hex"][0] = "0000000000000000"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outcome_path = root / "outcome.jsonl"
            teacher_path = root / "teacher.jsonl"
            log_path = root / "task.log"
            for path, rows in ((outcome_path, outcome), (teacher_path, teacher)):
                path.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")
            log_path.write_text(collector.DIAGNOSTIC_PREFIX + json.dumps(diagnostics[0]) + "\n", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "information-set hash coverage"):
                collector._validate_task_outputs(log_path, outcome_path, teacher_path, 0, 1)

    def test_rejects_fallback_that_differs_from_exact_outcome_selection(self) -> None:
        outcome, teacher, diagnostics = _fixture()
        diagnostics[0]["fallback_selected_index"] = 0
        diagnostics[0]["search_margin_f32_bits_hex"] = "00000000"
        diagnostics[0]["teacher_differs_from_fallback"] = False
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outcome_path = root / "outcome.jsonl"
            teacher_path = root / "teacher.jsonl"
            log_path = root / "task.log"
            for path, rows in ((outcome_path, outcome), (teacher_path, teacher)):
                path.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")
            log_path.write_text(collector.DIAGNOSTIC_PREFIX + json.dumps(diagnostics[0]) + "\n", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "exactly match outcome"):
                collector._validate_task_outputs(log_path, outcome_path, teacher_path, 0, 1)

    def test_rejects_teacher_that_violates_fixed_margin_rule(self) -> None:
        outcome, teacher, diagnostics = _fixture()
        diagnostics[0]["search_margin_f32_bits_hex"] = "3e7fffff"
        diagnostics[0]["teacher_selected_index"] = 1
        diagnostics[0]["teacher_differs_from_fallback"] = False
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outcome_path = root / "outcome.jsonl"
            teacher_path = root / "teacher.jsonl"
            log_path = root / "task.log"
            for path, rows in ((outcome_path, outcome), (teacher_path, teacher)):
                path.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")
            log_path.write_text(
                collector.DIAGNOSTIC_PREFIX + json.dumps(diagnostics[0]) + "\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(RuntimeError, "search target rule mismatch"):
                collector._validate_task_outputs(log_path, outcome_path, teacher_path, 0, 1)

    def test_topology_screen_builds_only_one_two_four_worker_commands(self) -> None:
        args = type(
            "Args",
            (),
            {
                "revealed_root": Path("revealed"),
                "mage_repo": Path("mage"),
                "scorer_exe": Path("scorer"),
                "reference_scorer_exe": Path("reference-scorer"),
                "outcome_root": Path("outcomes"),
                "source_database": Path("cards.h2.mv.db"),
                "maven": Path("mvn"),
                "base_seed": 17,
                "pair_start": 0,
                "topology_pairs": 4,
                "task_pairs": 2,
            },
        )()
        commands = collector.build_topology_screen_commands(args)
        self.assertEqual([command[command.index("--workers") + 1] for command in commands], ["1", "2", "4"])
        self.assertTrue(all("--pairs" in command for command in commands))

    def test_atomic_report_refuses_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            path.write_text("old", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "overwrite"):
                collector._atomic_fresh_json(path, {"status": "complete"})

    def test_shadow_noninterference_requires_byte_identical_exports(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            reference_outcome = root / "reference.outcome"
            outcome = root / "shadow.outcome"
            reference_teacher = root / "reference.teacher"
            teacher = root / "shadow.teacher"
            for path in (reference_outcome, outcome, reference_teacher, teacher):
                path.write_bytes(b"identical\n")
            collector._require_identical_shadow_exports(
                "fixture",
                reference_outcome,
                outcome,
                reference_teacher,
                teacher,
            )
            outcome.write_bytes(b"changed\n")
            with self.assertRaisesRegex(RuntimeError, "changed the live trajectory"):
                collector._require_identical_shadow_exports(
                    "fixture",
                    reference_outcome,
                    outcome,
                    reference_teacher,
                    teacher,
                )


if __name__ == "__main__":
    unittest.main()
