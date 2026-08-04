#!/usr/bin/env python3
"""Focused tests for the recurrent CP7 candidate-state screen."""

from __future__ import annotations

import unittest

import torch

import run_screen_v1 as screen
from model_v1 import ACTION_DIM, EDGE_DIM, HISTORY_DIM, OBJECT_DIM, REF_DIM, STATE_DIM, pack_rows


def _row(seed: int, actions: int = 4) -> dict:
    generator = torch.Generator().manual_seed(seed)
    return {
        "state": torch.randn(STATE_DIM, generator=generator),
        "history_features": torch.randn(3, HISTORY_DIM, generator=generator),
        "object_features": torch.randn(4, OBJECT_DIM, generator=generator),
        "object_card_ids": torch.arange(4),
        "object_groups": torch.arange(4),
        "edge_features": torch.randn(2, EDGE_DIM, generator=generator),
        "edge_src": torch.tensor([0, 1]),
        "edge_tgt": torch.tensor([1, 2]),
        "action_features": torch.randn(actions, ACTION_DIM, generator=generator),
        "action_ref_features": torch.randn(2, REF_DIM, generator=generator),
        "ref_action_indices": torch.tensor([0, 1]),
        "ref_node_indices": torch.tensor([0, 2]),
        "old_logits": torch.randn(actions, generator=generator),
        "old_value": torch.randn((), generator=generator),
        "selected_index": 1,
        "teacher_selected_index": 2,
        "substep_count": 1,
    }


class ScreenTests(unittest.TestCase):
    def test_zero_initialized_residual_exactly_preserves_parent(self) -> None:
        device = torch.device("cpu")
        torch.manual_seed(screen.SEED)
        model = screen.RecurrentStructuredActorCritic(32)
        torch.nn.init.zeros_(model.policy_head.weight)
        torch.nn.init.zeros_(model.policy_head.bias)
        packed = pack_rows([_row(1), _row(2, 5)], device)
        with torch.no_grad():
            candidate, scale = screen._candidate_logits(model, packed)
        for index, actions in enumerate((4, 5)):
            self.assertTrue(
                torch.equal(candidate[index, :actions], packed.parent_logits[index, :actions])
            )
        self.assertTrue(torch.all(scale > 0.999))

    def test_projection_enforces_fixed_log_probability_envelope(self) -> None:
        packed = pack_rows([_row(3)], torch.device("cpu"))
        raw = packed.parent_logits.clone()
        raw[0, 0] += 20.0
        projected, _ = screen._project_with_scale(
            packed.parent_logits, raw, packed.action_mask, packed.substep_count
        )
        delta = (
            torch.log_softmax(projected, dim=1)
            - torch.log_softmax(packed.parent_logits, dim=1)
        ).abs().masked_fill(~packed.action_mask, 0.0)
        self.assertLessEqual(float(delta.max()), 0.4901)

    def test_deployment_scale_interpolates_after_projection(self) -> None:
        packed = pack_rows([_row(8)], torch.device("cpu"))
        torch.manual_seed(10)
        model = screen.RecurrentStructuredActorCritic(32)
        with torch.no_grad():
            full, _ = screen._candidate_logits(model, packed)
            scaled, _ = screen._candidate_logits(
                model, packed, deployment_scale=0.97
            )
        expected = packed.parent_logits + 0.97 * (full - packed.parent_logits)
        self.assertTrue(torch.allclose(scaled, expected, atol=1.0e-6))

    def test_gate_requires_substantive_fit_and_both_seats(self) -> None:
        row = {
            "relative_nll_improvement": 0.06,
            "top1_delta": 0.04,
            "mean_total_variation": 0.02,
            "p90_total_variation": 0.08,
            "maximum_absolute_log_ratio": 0.49,
        }
        metrics = {
            "overall": dict(row),
            "by_candidate_seat": {"0": dict(row), "1": dict(row)},
        }
        self.assertTrue(screen._gate(metrics)["pass"])
        metrics["by_candidate_seat"]["1"]["relative_nll_improvement"] = -0.001
        self.assertFalse(screen._gate(metrics)["pass"])


if __name__ == "__main__":
    unittest.main()
