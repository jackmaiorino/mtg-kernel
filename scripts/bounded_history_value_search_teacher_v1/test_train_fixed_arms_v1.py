#!/usr/bin/env python3
"""Synthetic unit tests for the fixed-arm offline training driver."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import train_fixed_arms_v1 as driver


def _label(**overrides):
    torch = __import__("torch")
    value = {
        "schema": driver.LABEL_SCHEMA,
        "pair_index": 0,
        "episode_id": 0,
        "step": 3,
        "physical_decision_id": 7,
        "candidate_seat": "P0",
        "legal_action_count": 2,
        "continuation_steps": 8,
        "information_set_samples": 4,
        "information_set_sample_hashes_u64_hex": ["a", "b", "c", "d"],
        "fallback_selected_index": 1,
        "best_search_index": 0,
        "teacher_selected_index": 0,
        "teacher_differs_from_fallback": True,
        "search_margin_f32_bits_hex": "3f000000",
        "action_values_f32_bits_hex": ["3f000000", "00000000"],
        "branch_count": 8,
        "natural_terminal_branch_count": 4,
        "critic_bootstrap_branch_count": 4,
        "model_input_sha256": "0" * 64,
        "public_history_sha256": driver._public_history_sha256(
            {"history_features": torch.zeros((0, 237), dtype=torch.float32)}
        ),
        "action_semantics": [{}, {}],
    }
    value.update(overrides)
    return value


def _metrics(nll: float, top1: float, *, seat1_nll: float | None = None) -> dict:
    def row(row_nll: float) -> dict:
        return {
            "candidate_selected_index_nll": row_nll,
            "candidate_top1": top1,
            "mean_total_variation": 0.01,
            "p90_total_variation": 0.02,
            "maximum_absolute_log_ratio": 0.48,
        }

    return {
        "overall": row(nll),
        "by_candidate_seat": {
            "0": row(nll),
            "1": row(nll if seat1_nll is None else seat1_nll),
        },
    }


class FixedArmTests(unittest.TestCase):
    def test_public_history_hash_matches_rust_empty_history_fixture(self):
        torch = __import__("torch")
        self.assertEqual(
            driver._public_history_sha256(
                {"history_features": torch.zeros((0, 237), dtype=torch.float32)}
            ),
            "fcb4e1c2d26439461ff869ee0aa61bd942763fcee571beec834e5eff4c4fefc2",
        )

    def test_frozen_settings_have_no_selection_knobs(self):
        self.assertEqual(driver.FROZEN_SETTINGS["beta"], 6.0)
        self.assertEqual(driver.FROZEN_SETTINGS["epochs"], 8)
        self.assertEqual(driver.FROZEN_SETTINGS["batch_size"], 256)
        self.assertEqual(driver.FROZEN_SETTINGS["seed"], 20_260_811)
        self.assertEqual(driver.FROZEN_SETTINGS["checkpoint_policy"], "final_epoch_8_only")
        self.assertEqual(driver.FROZEN_SETTINGS["arm_selection"], "none")

    def test_fixed_arm_executes_multiple_batches_with_sparse_batch_signature(self):
        torch = __import__("torch")
        model = torch.nn.Linear(1, 1)
        seen: list[str] = []

        def batches(decisions, rng):
            self.assertIsNotNone(rng)
            for decision in decisions:
                yield [decision]

        def loss(current_model, batch, device):
            self.assertEqual(device.type, "cpu")
            seen.extend(batch)
            return sum(parameter.square().sum() for parameter in current_model.parameters()), {}

        with (
            mock.patch.object(driver, "EPOCHS", 2),
            mock.patch.object(driver, "_new_model", return_value=model),
            mock.patch.object(driver.sparse, "_batches", side_effect=batches),
            mock.patch.object(driver, "_dense_loss", side_effect=loss),
        ):
            _, metadata = driver._fit_fixed_arm(["first", "second"], torch.device("cpu"))
        self.assertEqual(seen, ["first", "second", "first", "second"])
        self.assertEqual(metadata["final_epoch"], 2)

    def test_cache_report_binds_actual_cache_bytes_bidirectionally(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cache_path = root / "history.pt"
            cache_path.write_bytes(b"actual cache")
            cache_sha256 = hashlib.sha256(cache_path.read_bytes()).hexdigest()
            collector_path = root / "collector.json"
            collector_path.write_text("{}", encoding="utf-8")
            collector_sha256 = hashlib.sha256(collector_path.read_bytes()).hexdigest()
            report_path = root / "history-report.json"
            report_path.write_text(
                json.dumps(
                    {
                        "schema": driver.CACHE_REPORT_SCHEMA,
                        "status": "complete",
                        "cache": str(cache_path),
                        "cache_sha256": cache_sha256,
                        "collector_report": str(collector_path),
                        "collector_report_sha256": collector_sha256,
                        "base_seed": 1_400_001,
                        "pair_start": 0,
                        "pairs": driver.PAIR_COUNT,
                    }
                ),
                encoding="utf-8",
            )
            metadata = driver._load_cache_report(
                report_path, cache_path, cache_sha256, collector_sha256
            )
            self.assertEqual(metadata["history_cache_report_cache_sha256"], cache_sha256)
            cache_path.write_bytes(b"swapped cache")
            with self.assertRaisesRegex(ValueError, "actual cache bytes"):
                driver._load_cache_report(
                    report_path,
                    cache_path,
                    hashlib.sha256(cache_path.read_bytes()).hexdigest(),
                    collector_sha256,
                )

    def test_label_validation_rejects_inconsistent_branch_counts(self):
        row = _label(critic_bootstrap_branch_count=3)
        with self.assertRaisesRegex(ValueError, "branch counts do not close"):
            driver._validate_label_row(row)

    def test_label_validation_recomputes_the_numerical_teacher_rule(self):
        with self.assertRaisesRegex(ValueError, "numerical target rule"):
            driver._validate_label_row(
                _label(teacher_selected_index=1, teacher_differs_from_fallback=False)
            )

    def test_label_parser_rejects_duplicate_join_keys(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "labels.jsonl"
            path.write_text(
                json.dumps(_label()) + "\n" + json.dumps(_label()) + "\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "duplicate search label key"):
                labels = {}
                for row in driver._load_jsonl(path):
                    driver._validate_label_row(row)
                    key = driver._label_key(row)
                    if key in labels:
                        driver._fail(f"duplicate search label key {key}")
                    labels[key] = row

    def test_join_requires_fallback_to_match_cache_selection(self):
        cache_row = {
            "pair_index": 0,
            "episode": "0",
            "candidate_seat": 0,
            "step": 3,
            "physical_group": 7,
            "substep_index": 0,
            "selected_index": 0,
            "substep_count": 1,
            "old_logits": __import__("torch").zeros(2),
            "history_features": __import__("torch").zeros((0, 237)),
        }
        label = _label()
        with self.assertRaisesRegex(ValueError, "fallback action does not join"):
            driver._join_rows([cache_row], {driver._label_key(label): label})

    def test_join_preserves_one_example_for_both_targets(self):
        torch = __import__("torch")
        cache_row = {
            "pair_index": 0,
            "episode": "0",
            "candidate_seat": 0,
            "step": 3,
            "physical_group": 7,
            "substep_index": 0,
            "selected_index": 1,
            "substep_count": 1,
            "old_logits": torch.zeros(2),
            "history_features": torch.zeros((0, 237)),
        }
        label = _label()
        joined = driver._join_rows([cache_row], {driver._label_key(label): label})
        self.assertEqual(len(joined), 1)
        self.assertEqual(joined[0]["fallback_selected_index"], 1)
        self.assertEqual(joined[0]["search_teacher_selected_index"], 0)
        self.assertEqual(driver._cache_key(joined[0]), (0, "0", 0, 3, 7, 0))

    def test_join_binds_the_exact_public_history(self):
        torch = __import__("torch")
        cache_row = {
            "pair_index": 0,
            "episode": "0",
            "candidate_seat": 0,
            "step": 3,
            "physical_group": 7,
            "substep_index": 0,
            "selected_index": 1,
            "substep_count": 1,
            "old_logits": torch.zeros(2),
            "history_features": torch.ones((1, 237)),
        }
        label = _label()
        with self.assertRaisesRegex(ValueError, "public-history hash"):
            driver._join_rows([cache_row], {driver._label_key(label): label})

    def test_two_arms_share_terminal_outcome_and_only_change_policy_target(self):
        torch = __import__("torch")
        row = {
            "pair_index": 0,
            "episode": "0",
            "candidate_seat": 0,
            "acting_seat": 0,
            "episode_key": (0, "0", 0),
            "step": 3,
            "physical_group": 7,
            "substep_index": 0,
            "selected_index": 1,
            "substep_count": 1,
            "old_logits": torch.zeros(2),
            "old_value": torch.tensor(0.0),
            "terminal_reward": 1.0,
        }
        fallback = driver._decisions([{**row, "fallback_selected_index": 1}], "fallback_selected_index")
        search = driver._decisions([{**row, "search_teacher_selected_index": 0}], "search_teacher_selected_index")
        self.assertEqual(fallback[0].key, search[0].key)
        self.assertEqual(fallback[0].raw_advantage, search[0].raw_advantage)
        self.assertEqual(fallback[0].rows[0]["teacher_selected_index"], 1)
        self.assertEqual(search[0].rows[0]["teacher_selected_index"], 0)
        identity_sha256 = driver._require_matched_arms(fallback, search, "test")
        self.assertEqual(len(identity_sha256), 64)
        search[0].episode_weight = 0.5
        with self.assertRaisesRegex(ValueError, "exact ordered examples"):
            driver._require_matched_arms(fallback, search, "test")

    def test_split_keeps_whole_pairs_in_one_lane(self):
        torch = __import__("torch")
        rows = []
        for pair in range(4):
            for seat in (0, 1):
                rows.append(
                    {
                        "pair_index": pair,
                        "episode": str(pair * 2 + seat),
                        "candidate_seat": seat,
                        "acting_seat": seat,
                        "episode_key": (pair, str(pair * 2 + seat), seat),
                        "step": 3,
                        "physical_group": 7,
                        "substep_index": 0,
                        "selected_index": 1,
                        "fallback_selected_index": 1,
                        "substep_count": 1,
                        "old_logits": torch.zeros(2),
                        "old_value": torch.tensor(0.0),
                        "terminal_reward": 1.0,
                    }
                )
        train, selection, heldout = driver._split(
            driver._decisions(rows, "fallback_selected_index")
        )
        self.assertEqual({decision.pair_index for decision in train}, {1, 2})
        self.assertEqual({decision.pair_index for decision in selection}, {3})
        self.assertEqual({decision.pair_index for decision in heldout}, {0})

    def test_report_path_and_hash_are_explicit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            labels_path = root / "labels.jsonl"
            labels_path.write_text(
                json.dumps(_label())
                + "\n"
                + json.dumps(_label(pair_index=1, episode_id=3, candidate_seat="P1"))
                + "\n",
                encoding="utf-8",
            )
            report = {
                "schema": driver.SCHEMA + ".labels-report",
                "status": "complete",
                "pair_count": driver.PAIR_COUNT,
                "history_cache_sha256": "1" * 64,
                "search_label_jsonl": labels_path.name,
                "search_label_sha256": hashlib.sha256(labels_path.read_bytes()).hexdigest(),
                "label_count": 2,
                "target_quality_gate": {"pass": True},
            }
            report_path = root / "report.json"
            report_path.write_text(json.dumps(report), encoding="utf-8")
            labels, metadata = driver._load_search_labels(report_path, "1" * 64)
            self.assertEqual(len(labels), 2)
            self.assertEqual(metadata["label_count"], 2)

    def test_collector_report_reads_hashed_task_log_diagnostics(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log_path = root / "task.log"
            log_path.write_text(
                "noise\n"
                + driver.DIAGNOSTIC_PREFIX
                + json.dumps(_label())
                + "\n"
                + driver.DIAGNOSTIC_PREFIX
                + json.dumps(_label(pair_index=0, episode_id=1, candidate_seat="P1"))
                + "\n",
                encoding="utf-8",
            )
            report = {
                "schema": driver.COLLECTOR_SCHEMA,
                "status": "complete",
                "base_seed": 1_400_001,
                "pair_start": 0,
                "pairs": driver.PAIR_COUNT,
                "search_diagnostics": 2,
                "target_quality_gate": {
                    "minimum_equal_episode_mass_override_rate_per_seat": 0.05,
                    "p0": True,
                    "p1": True,
                    "pass": True,
                },
                "advance_to_training": True,
                "by_candidate_seat": {
                    "p0": {
                        "labels": 1,
                        "overrides": 1,
                        "label_episode_mass": 1.0,
                        "override_episode_mass": 1.0,
                        "equal_episode_mass_override_rate": 1.0,
                    },
                    "p1": {
                        "labels": 1,
                        "overrides": 1,
                        "label_episode_mass": 1.0,
                        "override_episode_mass": 1.0,
                        "equal_episode_mass_override_rate": 1.0,
                    },
                },
                "tasks": [
                    {
                        "first_pair": 0,
                        "pair_count": driver.PAIR_COUNT,
                        "log_path": log_path.name,
                        "log_sha256": hashlib.sha256(log_path.read_bytes()).hexdigest(),
                        "diagnostics": 2,
                    }
                ],
            }
            report_path = root / "collector-report.json"
            report_path.write_text(json.dumps(report), encoding="utf-8")
            labels, metadata = driver._load_search_labels(report_path, "2" * 64)
            self.assertEqual(len(labels), 2)
            self.assertEqual(metadata["label_source"], "collector_task_logs")

    def test_collector_report_cannot_forge_a_pass_over_non_overriding_labels(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rows = [
                _label(
                    episode_id=seat,
                    candidate_seat=f"P{seat}",
                    best_search_index=1,
                    teacher_selected_index=1,
                    teacher_differs_from_fallback=False,
                    search_margin_f32_bits_hex="00000000",
                    action_values_f32_bits_hex=["00000000", "00000000"],
                )
                for seat in (0, 1)
            ]
            log_path = root / "task.log"
            log_path.write_text(
                "".join(
                    driver.DIAGNOSTIC_PREFIX + json.dumps(row) + "\n" for row in rows
                ),
                encoding="utf-8",
            )
            report = {
                "schema": driver.COLLECTOR_SCHEMA,
                "status": "complete",
                "base_seed": 1_400_001,
                "pair_start": 0,
                "pairs": driver.PAIR_COUNT,
                "search_diagnostics": 2,
                "target_quality_gate": {
                    "minimum_equal_episode_mass_override_rate_per_seat": 0.05,
                    "p0": True,
                    "p1": True,
                    "pass": True,
                },
                "advance_to_training": True,
                "by_candidate_seat": {
                    seat: {
                        "labels": 1,
                        "overrides": 1,
                        "label_episode_mass": 1.0,
                        "override_episode_mass": 1.0,
                        "equal_episode_mass_override_rate": 1.0,
                    }
                    for seat in ("p0", "p1")
                },
                "tasks": [{
                    "first_pair": 0,
                    "pair_count": driver.PAIR_COUNT,
                    "log_path": log_path.name,
                    "log_sha256": hashlib.sha256(log_path.read_bytes()).hexdigest(),
                    "diagnostics": 2,
                }],
            }
            report_path = root / "collector-report.json"
            report_path.write_text(json.dumps(report), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "does not match labels"):
                driver._load_search_labels(report_path, "2" * 64)

    def test_prefit_gate_weights_each_label_bearing_episode_equally(self):
        p0_many = [
            _label(
                step=index + 1,
                physical_decision_id=index + 1,
                teacher_selected_index=0 if index == 0 else 1,
                teacher_differs_from_fallback=index == 0,
                best_search_index=0 if index == 0 else 1,
                search_margin_f32_bits_hex="3f000000" if index == 0 else "00000000",
                action_values_f32_bits_hex=(
                    ["3f000000", "00000000"]
                    if index == 0
                    else ["00000000", "00000000"]
                ),
            )
            for index in range(10)
        ]
        p0_single = _label(pair_index=1, episode_id=2)
        p1_single = _label(pair_index=1, episode_id=3, candidate_seat="P1")
        quality = driver._target_quality_from_labels(
            [*p0_many, p0_single, p1_single]
        )
        self.assertAlmostEqual(
            quality["by_candidate_seat"]["p0"]["equal_episode_mass_override_rate"],
            0.55,
        )
        self.assertTrue(quality["target_quality_gate"]["pass"])

    def test_comparison_gate_uses_common_search_targets(self):
        search = _metrics(0.90, 0.60)
        fallback = _metrics(1.00, 0.55)
        training = {"maximum_preclip_gradient_norm": 4.0}
        gate = driver._comparison_gate(search, fallback, training, training)
        self.assertTrue(gate["pass"])
        self.assertAlmostEqual(gate["search_target_relative_nll_improvement"], 0.10)
        self.assertAlmostEqual(gate["search_target_top1_delta"], 0.05)

    def test_comparison_gate_fails_a_candidate_seat_nll_regression(self):
        search = _metrics(0.90, 0.60, seat1_nll=1.01)
        fallback = _metrics(1.00, 0.55)
        training = {"maximum_preclip_gradient_norm": 4.0}
        gate = driver._comparison_gate(search, fallback, training, training)
        self.assertFalse(gate["pass"])
        self.assertFalse(
            gate["checks"]["search_target_nll_nonregressing_at_both_seats"]
        )


if __name__ == "__main__":
    unittest.main()
