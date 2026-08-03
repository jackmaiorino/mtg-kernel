#!/usr/bin/env python3
"""Unit tests for the rapid standalone structured distillation screen."""

from __future__ import annotations

import json
import tempfile
import unittest
import copy
from pathlib import Path
from types import SimpleNamespace

import torch

import run_structured_successor_distillation_v1 as distill


class _Decision:
    def __init__(self, key, episode_key, seat, rows):
        self.key = key
        self.episode_key = episode_key
        self.candidate_seat = seat
        self.rows = rows


class _AbsoluteModel:
    def __init__(self, logits, value):
        self.logits = torch.tensor(logits, dtype=torch.float32)
        self.value = torch.tensor(value, dtype=torch.float32)

    def eval(self):
        return self

    def _one(self, row):
        return self.logits.clone(), self.value.clone()


def _row(old_logits, old_value, index=0):
    return {
        "old_logits": torch.tensor(old_logits, dtype=torch.float32),
        "old_value": torch.tensor(old_value, dtype=torch.float32),
        "selected_index": index,
    }


class DistillationTests(unittest.TestCase):
    def test_metrics_use_absolute_one_outputs_not_parent_plus_residual(self):
        decision = _Decision(
            (0, "e", 0, 0),
            (0, "e", 0),
            0,
            [_row([0.0, 0.0], 0.0)],
        )
        metrics = distill._metrics(_AbsoluteModel([0.0, 0.0], 0.0), [decision])
        self.assertAlmostEqual(metrics["overall"]["weighted_mean_kl"], 0.0)
        self.assertAlmostEqual(metrics["overall"]["mean_total_variation"], 0.0)
        self.assertEqual(metrics["overall"]["top_action_agreement"], 1.0)
        self.assertAlmostEqual(metrics["overall"]["value_rmse"], 0.0)

        shifted = distill._metrics(_AbsoluteModel([4.0, 0.0], 2.0), [decision])
        self.assertGreater(shifted["overall"]["mean_total_variation"], 0.0)
        self.assertGreater(shifted["overall"]["value_rmse"], 1.0)

    def test_episode_physical_and_substep_weights(self):
        decisions = [
            _Decision((0, "a", 0, 0), (0, "a", 0), 0, [_row([0, 0], 0), _row([0, 0], 0)]),
            _Decision((0, "a", 0, 1), (0, "a", 0), 0, [_row([0, 0], 0)]),
            _Decision((1, "b", 1, 0), (1, "b", 1), 1, [_row([0, 0], 0)]),
        ]
        weights = distill._episode_weights(decisions)
        self.assertEqual(weights[decisions[0].key], (0.5, 0.25))
        self.assertEqual(weights[decisions[1].key], (0.5, 0.5))
        self.assertEqual(weights[decisions[2].key], (1.0, 1.0))
        self.assertAlmostEqual(sum(weights[d.key][0] for d in decisions), 2.0)

    def test_weighted_quantile_and_gate_aggregation(self):
        self.assertEqual(
            distill._weighted_quantile([(0.01, 1.0), (0.10, 3.0)], 0.90), 0.10
        )
        base = {
            "policy_kl_numerator": 0.0,
            "tv_numerator": 0.01,
            "top_action_numerator": 0.99,
            "policy_mass": 1.0,
            "value_squared_error_numerator": 0.01,
            "value_mass": 1.0,
            "policy_row_count": 1,
            "physical_decision_count": 1,
            "episode_keys": {("e",)},
            "tv_weighted_samples": [(0.01, 1.0)],
        }
        metric = distill._finish_metric(base)
        self.assertTrue(metric["mean_total_variation"] <= 0.02)
        self.assertTrue(metric["top_action_agreement"] >= 0.98)
        self.assertTrue(metric["value_rmse"] <= 0.10)

    def test_source_and_overwrite_validation(self):
        with self.assertRaises(ValueError):
            distill._validate_source(
                {"cache_sha256": "wrong", "pair_count": 2048},
                distill.EXPECTED_CACHE_SHA256,
                distill.EXPECTED_PAIRS,
            )
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "result.json"
            path.write_text("{}\n", encoding="utf-8")
            with self.assertRaises(ValueError):
                distill._write_new(path, {"x": 1})

    def test_aggregate_gate_rejects_non_profile_and_wrong_source(self):
        result = {
            "schema": distill.SCHEMA,
            "fold": 0,
            "profile_only": True,
            "source": {
                "cache_sha256": distill.EXPECTED_CACHE_SHA256,
                "pair_count": distill.EXPECTED_PAIRS,
            },
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fold.json"
            path.write_text(json.dumps(result), encoding="utf-8")
            args = SimpleNamespace(
                fold_result=[path, path, path, path],
                output=Path(directory) / "aggregate.json",
                expected_cache_sha256=distill.EXPECTED_CACHE_SHA256,
                expected_pairs=distill.EXPECTED_PAIRS,
            )
            with self.assertRaises(ValueError):
                distill.aggregate(args)

    def test_aggregate_applies_frozen_gates_and_exact_p90(self):
        raw = {
            "policy_kl_numerator": 0.0,
            "tv_numerator": 0.01,
            "top_action_numerator": 0.99,
            "policy_mass": 1.0,
            "value_squared_error_numerator": 0.01,
            "value_mass": 1.0,
            "policy_row_count": 1,
            "physical_decision_count": 1,
            "episode_keys": {("e",)},
            "tv_weighted_samples": [(0.01, 1.0)],
        }
        metric = distill._finish_metric(raw)
        metric["tv_weighted_samples"] = [(0.01, 1.0)]
        fold_template = {
            "schema": distill.SCHEMA,
            "profile_only": False,
            "source": {
                "cache_sha256": distill.EXPECTED_CACHE_SHA256,
                "pair_count": distill.EXPECTED_PAIRS,
            },
            "config": {"architecture": "test"},
            "heldout_metrics": {
                "overall": metric,
                "by_candidate_seat": {"0": metric, "1": metric},
            },
        }
        with tempfile.TemporaryDirectory() as directory:
            paths = []
            for fold in range(4):
                value = copy.deepcopy(fold_template)
                value["fold"] = fold
                path = Path(directory) / f"fold-{fold}.json"
                path.write_text(json.dumps(value), encoding="utf-8")
                paths.append(path)
            passing_args = SimpleNamespace(
                fold_result=paths,
                output=Path(directory) / "aggregate-pass.json",
                expected_cache_sha256=distill.EXPECTED_CACHE_SHA256,
                expected_pairs=distill.EXPECTED_PAIRS,
            )
            passing = distill.aggregate(passing_args)
            self.assertTrue(passing["pass"])
            self.assertTrue(passing["gates"]["p90_tv_le_0_05"])

            failing_value = copy.deepcopy(fold_template)
            failing_value["fold"] = 0
            failing_metric = copy.deepcopy(metric)
            failing_metric["tv_weighted_samples"] = [(0.06, 1.0)]
            failing_metric["_sums"]["tv_numerator"] = 0.06
            failing_value["heldout_metrics"] = {
                "overall": failing_metric,
                "by_candidate_seat": {"0": failing_metric, "1": failing_metric},
            }
            paths[0].write_text(json.dumps(failing_value), encoding="utf-8")
            failing_args = SimpleNamespace(
                fold_result=paths,
                output=Path(directory) / "aggregate-fail.json",
                expected_cache_sha256=distill.EXPECTED_CACHE_SHA256,
                expected_pairs=distill.EXPECTED_PAIRS,
            )
            failing = distill.aggregate(failing_args)
            self.assertFalse(failing["pass"])
            self.assertFalse(failing["gates"]["p90_tv_le_0_05"])


if __name__ == "__main__":
    unittest.main()
