from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = (
    ROOT
    / "python"
    / "tools"
    / "evaluate_native_policy_loss_reduction_true_k512_v1.py"
)
SPEC = importlib.util.spec_from_file_location("true_k512_loss_gate_v1", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("true-K512 gate module cannot be loaded")
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)


class TrueK512LossGateLogicV1Tests(unittest.TestCase):
    @staticmethod
    def _valid_zero_term_capture(
        executable_sha256: str,
    ) -> dict[str, object]:
        episodes: list[dict[str, object]] = []
        for episode_index in range(gate.EXPECTED_K):
            episodes.append(
                {
                    "ordinal": episode_index,
                    "episode_index": episode_index,
                    "environment_seed": gate.derive_train_env_seed(
                        gate.EXPECTED_RUN_BASE_SEED, episode_index // 2
                    ),
                    "deck_hashes": [1, 1],
                    "learner_seat": gate._frozen_learner_seat(episode_index),
                    "learner_return": 0,
                    "terminal_outcome": "draw",
                    "learner_group_count": 1,
                    "learner_policy_step_count": 1,
                    "term_begin_inclusive": episode_index,
                    "term_end_exclusive": episode_index + 1,
                    "full_trajectory_sha256": f"{episode_index:064x}",
                    "full_policy_step_count": 1,
                    "full_physical_decision_count": 1,
                    "opponent_policy_step_count": 0,
                    "opponent_physical_decision_count": 0,
                }
            )
        schedule_proof = gate._episode_schedule_proof(episodes)
        terms = [
            {
                "group_index": group_index,
                "joint_log_probability_f32_bits": "0x00000000",
                "value_f32_bits": "0x00000000",
                "terminal_return": 0,
            }
            for group_index in range(gate.EXPECTED_K)
        ]
        snapshot = {
            "snapshot_sha256": "2" * 64,
            "test_snapshot": "bound",
        }
        return {
            "schema": gate.CAPTURE_SCHEMA,
            "identity": gate.CAPTURE_IDENTITY,
            "nonclaim": "test fixture",
            "source": {
                "strict_source_tree": {
                    "source_tree_recipe_identity": gate.STRICT_SOURCE_RECIPE_IDENTITY,
                    "source_tree_recipe_sha256": gate.STRICT_SOURCE_RECIPE_SHA256,
                    "git_commit": "0" * 40,
                    "source_tree_sha256": "1" * 64,
                    "worktree_clean": True,
                    "git_status_sha256": gate.EMPTY_SHA256,
                },
                "preflight_validated": True,
                "postflight_equality_validated": True,
                "executable_sha256": executable_sha256,
                "capture_harness_sha256": "3" * 64,
                "capture_harness_path": gate.CAPTURE_HARNESS.relative_to(
                    gate.ROOT
                ).as_posix(),
            },
            "workload": {
                "composition_identity": gate.EXPECTED_COMPOSITION_IDENTITY,
                "composition_nonclaim": "no physical group or term was cycled, replayed, expanded, or synthetically generated",
                "trainer_contract_identity": gate.EXPECTED_TRAINER_CONTRACT,
                "numerical_backend_identity": gate.EXPECTED_NUMERICAL_BACKEND,
                "run_base_seed": gate.EXPECTED_RUN_BASE_SEED,
                "batch_episodes": gate.EXPECTED_K,
                "deck_ids": gate.EXPECTED_DECK_IDS,
                "max_physical_decisions": 5_000,
                "max_policy_steps": 640_000,
                "worker_count": 1,
                "sessions_per_worker": 1,
                "logical_actor_count": 1,
                "broker_batch_target": 1,
                "scheduler_timeout_ms": 600_000,
                "measure_broker_service_time": False,
                "value_coefficient_f32_bits": gate.VALUE_COEFFICIENT_BITS,
                "learning_rate_f32_bits": gate.LEARNING_RATE_BITS,
            },
            "snapshot": snapshot,
            "sizing_row": {
                "update_ordinal": 0,
                "outer_update_elapsed_ns": 1,
                "executor_update_elapsed_ns": 1,
                "rollout_elapsed_ns": 1,
                "episode_count": gate.EXPECTED_K,
                "physical_decision_count": gate.EXPECTED_K,
                "policy_step_count": gate.EXPECTED_K,
                "learner_group_count": gate.EXPECTED_K,
                "learner_policy_step_count": gate.EXPECTED_K,
                "scorer_accepted_batch_count": gate.EXPECTED_K,
                "scorer_accepted_decision_count": gate.EXPECTED_K,
                "scored_action_logit_count": gate.EXPECTED_K,
                "model_digest_before": "before",
                "model_digest_after": "after",
                "changed_non_gauge_parameter_count": 1,
                "adam_step_before": 0,
                "adam_step_after": 1,
            },
            "episodes": {
                "framing": gate.EPISODE_STREAM_FRAMING,
                "sha256": gate._episode_stream_digest(episodes),
                "independent_episode_count": gate.EXPECTED_K,
                **schedule_proof,
                "records": episodes,
            },
            "selected_outputs": {
                "framing": gate.SELECTED_STREAM_FRAMING,
                "sha256": "4" * 64,
                "count": gate.EXPECTED_K,
            },
            "term_stream": {
                "framing": gate.TERM_STREAM_FRAMING,
                "sha256": gate._term_stream_digest(terms),
                "learner_physical_decision_group_count": gate.EXPECTED_K,
                "policy_term_count": gate.EXPECTED_K,
                "value_term_count": gate.EXPECTED_K,
                "policy_nonzero_count": 0,
                "value_nonzero_count": 0,
                "terminal_return_counts": [0, gate.EXPECTED_K, 0],
                "terms": terms,
            },
            "rust_production_reduction": {
                "operation": "test ordered f32 zero-term reduction",
                "reconstruction_matches_production_bits": True,
                "policy_sum": {"value": 0.0, "f32_bits": "0x00000000"},
                "value_sum": {"value": 0.0, "f32_bits": "0x00000000"},
                "loss": {"value": 0.0, "f32_bits": "0x00000000"},
            },
        }

    def test_all_scalar_margin_floors_are_fail_closed_without_tolerance_change(self) -> None:
        expected = 100.0
        allowed = gate.LOSS_ABSOLUTE_TOLERANCE + (
            gate.LOSS_RELATIVE_TOLERANCE * abs(expected)
        )
        exact_floor = gate._comparison_record(
            "policy_sum", expected, expected + allowed / gate.MARGIN_FLOOR
        )
        self.assertTrue(exact_floor["tolerance_holds"])
        self.assertGreaterEqual(
            exact_floor["margin_ratio_allowed_over_delta_f64"],
            gate.MARGIN_FLOOR,
        )
        self.assertTrue(exact_floor["gate_holds"])

        below_floor = gate._comparison_record(
            "value_sum", expected, expected + allowed / 4.0
        )
        self.assertTrue(below_floor["tolerance_holds"])
        self.assertFalse(below_floor["margin_floor_holds"])
        self.assertFalse(below_floor["gate_holds"])

        loss_same_margin = gate._comparison_record(
            "loss", expected, expected + allowed / 4.0
        )
        self.assertTrue(loss_same_margin["tolerance_holds"])
        self.assertTrue(loss_same_margin["margin_floor_applies"])
        self.assertFalse(loss_same_margin["margin_floor_holds"])
        self.assertFalse(loss_same_margin["gate_holds"])

    def test_terminal_diagnostic_names_every_triggered_scalar_channel(self) -> None:
        comparisons = {
            "policy_sum": gate._comparison_record("policy_sum", 1.0, 1.00004),
            "value_sum": gate._comparison_record("value_sum", 100.0, 100.0015),
            "loss": gate._comparison_record("loss", 0.25, 0.25002),
        }
        record = gate._gate_record(comparisons)
        self.assertEqual(record["status"], "repair_required")
        self.assertEqual(record["diagnostic_code"], gate.REPAIR_DIAGNOSTIC)
        self.assertIn("policy_sum:margin_below_5x", record["triggered_conditions"])
        self.assertIn("value_sum:margin_below_5x", record["triggered_conditions"])
        self.assertIn("loss:margin_below_5x", record["triggered_conditions"])
        self.assertFalse(record["silent_tolerance_loosening_permitted"])
        self.assertEqual(len(record["predeclared_repair_paths"]), 2)

    def test_portable_f32_reduction_is_ordered_and_count_sensitive(self) -> None:
        policy = [gate._f32(0.1), gate._f32(-0.2), gate._f32(0.3)]
        value = [gate._f32(1.0), gate._f32(0.25), gate._f32(4.0)]
        policy_sum, value_sum, loss = gate._sequential_reduction(policy, value)
        self.assertEqual(
            gate._bits_hex(policy_sum),
            gate._bits_hex(
                gate._f32_add(gate._f32_add(0.0, policy[0]), gate._f32_add(policy[1], 0.0))
                + policy[2]
            ),
        )
        expected_loss = gate._f32_div(
            gate._f32_add(
                policy_sum,
                gate._f32_mul(gate.VALUE_COEFFICIENT, value_sum),
            ),
            gate._f32(3.0),
        )
        self.assertEqual(gate._bits_hex(loss), gate._bits_hex(expected_loss))

    def test_executable_and_term_count_are_rejected_before_numerical_comparison(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            directory_path = Path(directory).resolve()
            executable_bytes = b"exact-test-capture-executable\x00"
            executable = directory_path / "capture-executable.exe"
            executable.write_bytes(executable_bytes)
            capture = self._valid_zero_term_capture(
                hashlib.sha256(executable_bytes).hexdigest()
            )
            snapshot = capture["snapshot"]
            path = directory_path / "capture.json"
            path.write_text(json.dumps(capture), encoding="utf-8")
            authority_fixture = mock.Mock(
                AUTHORITY_PLATFORM_SYSTEM="Windows",
                AUTHORITY_PLATFORM_MACHINE="AMD64",
                AUTHORITY_PYTHON_VERSION="3.12.10",
                AUTHORITY_TORCH_VERSION="2.13.0+cpu",
                TORCH_NUM_THREADS=1,
                TORCH_NUM_INTEROP_THREADS=1,
            )
            authority_fixture._validate_authorities.return_value = {
                "trainer_sha256": gate._sha256(gate.TRAINER_SOURCE)
            }
            with (
                mock.patch.object(
                    gate,
                    "_capture_harness_blob",
                    return_value=(b"", "3" * 64),
                ),
                mock.patch.object(
                    gate, "_snapshot_expectations", return_value=snapshot
                ),
                mock.patch.object(
                    gate,
                    "_term_values",
                    wraps=gate._term_values,
                ) as term_values,
                mock.patch.object(
                    gate,
                    "_authority_reduction",
                    return_value=(
                        (0.0, 0.0, 0.0),
                        gate.EXPECTED_K,
                        gate.EXPECTED_K,
                        gate.EXPECTED_K,
                    ),
                ) as authority_reduction,
                mock.patch.object(
                    gate,
                    "_comparison_record",
                    wraps=gate._comparison_record,
                ) as comparison,
                mock.patch.object(
                    gate,
                    "_load_torch_authority",
                    return_value=(
                        mock.sentinel.torch,
                        authority_fixture,
                        mock.sentinel.compute_loss_tensors,
                    ),
                ),
            ):
                payload = gate._payload(path, executable)
                self.assertEqual(payload["gate"]["status"], "pass")
                term_values.assert_called_once()
                authority_reduction.assert_called_once()
                self.assertEqual(comparison.call_count, len(gate.ALL_CHANNELS))

                executable.write_bytes(b"tampered-capture-executable\x00")
                term_values.reset_mock()
                authority_reduction.reset_mock()
                comparison.reset_mock()
                with self.assertRaisesRegex(
                    RuntimeError,
                    "capture executable SHA-256 binding failed",
                ):
                    gate._payload(path, executable)
                term_values.assert_not_called()
                authority_reduction.assert_not_called()
                comparison.assert_not_called()
                executable.write_bytes(executable_bytes)

                capture["term_stream"]["policy_term_count"] = gate.EXPECTED_K - 1
                path.write_text(json.dumps(capture), encoding="utf-8")
                term_values.reset_mock()
                authority_reduction.reset_mock()
                comparison.reset_mock()
                with self.assertRaisesRegex(
                    RuntimeError,
                    "capture explicit term count drifted: policy_term_count",
                ):
                    gate._payload(path, executable)
                term_values.assert_not_called()
                authority_reduction.assert_not_called()
                comparison.assert_not_called()

    def test_cli_rejects_executable_before_loading_torch_authority(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            directory_path = Path(directory).resolve()
            expected_executable = b"expected-capture-executable\x00"
            executable = directory_path / "capture-executable.exe"
            executable.write_bytes(b"wrong-capture-executable\x00")
            capture = {
                "source": {
                    "executable_sha256": hashlib.sha256(
                        expected_executable
                    ).hexdigest()
                }
            }
            capture_path = directory_path / "capture.json"
            capture_path.write_text(json.dumps(capture), encoding="utf-8")
            output_path = directory_path / "gate.json"

            with (
                mock.patch.object(
                    sys,
                    "argv",
                    [
                        str(gate.GENERATOR),
                        "--capture",
                        str(capture_path),
                        "--capture-executable",
                        str(executable),
                        "--output",
                        str(output_path),
                    ],
                ),
                mock.patch.object(gate, "_load_torch_authority") as load_authority,
            ):
                with self.assertRaisesRegex(
                    RuntimeError, "capture executable SHA-256 binding failed"
                ):
                    gate.main()
            load_authority.assert_not_called()
            self.assertFalse(output_path.exists())

    def test_module_import_does_not_load_torch_before_cli_preflight(self) -> None:
        script = "\n".join(
            [
                "import importlib.util",
                "import sys",
                f"path = {str(MODULE_PATH)!r}",
                "spec = importlib.util.spec_from_file_location('k512_preflight_probe', path)",
                "module = importlib.util.module_from_spec(spec)",
                "assert spec.loader is not None",
                "spec.loader.exec_module(module)",
                "assert 'torch' not in sys.modules, 'Torch loaded before executable preflight'",
            ]
        )
        completed = subprocess.run(
            [sys.executable, "-c", script],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)

    def test_python_git_lookup_strips_ambient_git_routing(self) -> None:
        with mock.patch.dict(
            os.environ,
            {
                "GIT_DIR": "malicious-dir",
                "gIt_work_tree": "malicious-worktree",
                "MTG_GIT_DIR": "preserved-non-routing-name",
            },
            clear=False,
        ):
            sanitized = gate._sanitized_git_environment()
        self.assertFalse(any(name[:4].lower() == "git_" for name in sanitized))
        self.assertEqual(sanitized["MTG_GIT_DIR"], "preserved-non-routing-name")


if __name__ == "__main__":
    unittest.main()
