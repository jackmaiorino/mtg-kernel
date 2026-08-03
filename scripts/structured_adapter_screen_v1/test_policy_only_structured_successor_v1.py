#!/usr/bin/env python3
"""Contract tests for the policy-only structured successor fit."""

from __future__ import annotations

import unittest

import torch

import fit_policy_only_structured_successor_v1 as subject
import run_structured_successor_distillation_v1 as distill


def _metric(mean_tv: float, p90_tv: float, top: float) -> dict[str, float]:
    return {
        "mean_total_variation": mean_tv,
        "p90_total_variation": p90_tv,
        "top_action_agreement": top,
    }


class PolicyOnlyStructuredSuccessorTest(unittest.TestCase):
    def test_exact_boundary_passes_all_seats(self) -> None:
        metric = _metric(0.015, 0.040, 0.990)
        gate = subject._fit_gate(
            {
                "overall": dict(metric),
                "by_candidate_seat": {"0": dict(metric), "1": dict(metric)},
            }
        )
        self.assertEqual(gate["decision"], "PASS")
        self.assertTrue(all(gate["checks"].values()))

    def test_one_seat_failure_rejects(self) -> None:
        pass_metric = _metric(0.010, 0.030, 0.995)
        fail_metric = _metric(0.010, 0.041, 0.995)
        gate = subject._fit_gate(
            {
                "overall": dict(pass_metric),
                "by_candidate_seat": {
                    "0": dict(pass_metric),
                    "1": dict(fail_metric),
                },
            }
        )
        self.assertEqual(gate["decision"], "REJECT")
        self.assertFalse(gate["checks"]["candidate_seat_1_p90_tv_at_most_0p040"])

    def test_policy_parameter_selection_freezes_exact_zero_value_head(self) -> None:
        torch.manual_seed(123)
        model = distill._model()
        before = subject._value_head_bits(model)
        parameters = subject._policy_parameters(model)
        self.assertGreater(len(parameters), 0)
        self.assertEqual(before, subject._value_head_bits(model))
        self.assertTrue(all(byte == 0 for byte in before))
        self.assertFalse(model.value_head.weight.requires_grad)
        self.assertFalse(model.value_head.bias.requires_grad)

    def test_weight_publication_has_fixed_parameter_count(self) -> None:
        torch.manual_seed(123)
        payload, parameters = subject._encoded_weights(distill._model())
        self.assertEqual(len(payload), 107_378 * 4)
        self.assertEqual(sum(item["count_f32"] for item in parameters), 107_378)

    def test_history_bucket_contract(self) -> None:
        self.assertEqual(
            [subject._history_bucket(value) for value in (0, 1, 3, 4, 7, 8, 15, 16)],
            [0, 1, 1, 4, 4, 8, 8, 16],
        )


if __name__ == "__main__":
    unittest.main()
