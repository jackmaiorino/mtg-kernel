"""Focused synthetic tests for the structured composed-credit wrapper."""

from __future__ import annotations

import sys
import unittest
from types import SimpleNamespace

import torch

import run_structured_row_v1 as subject


def _row(value: float, reward: float, history_length: int = 0) -> dict:
    return {
        "old_value": torch.tensor(value, dtype=torch.float32),
        "terminal_reward": reward,
        "acting_seat": 0,
        "old_logits": torch.tensor([0.0, 1.0], dtype=torch.float32),
        "selected_index": 0,
        "history_features": torch.zeros(
            (history_length, subject.EXPECTED_HISTORY_FEATURE_DIM), dtype=torch.float32
        ),
        "substep_index": 0,
    }


def _decision(
    index: int, value: float, reward: float, *, seat: int = 0
) -> SimpleNamespace:
    episode = f"episode-{seat}"
    row = _row(value, reward)
    row["acting_seat"] = seat
    return SimpleNamespace(
        key=(0, episode, seat, index),
        episode_key=(0, episode, seat),
        candidate_seat=seat,
        rows=[row],
        episode_weight=1.0 / 3.0,
    )


class StructuredCreditTests(unittest.TestCase):
    def test_terminal_gae_recurrence(self) -> None:
        observed = subject.terminal_gae([0.0, 0.0], 1.0, gamma=1.0, lam=0.5)
        self.assertEqual(observed, [0.5, 1.0])

    def test_terminal_gae_lambda_one_is_terminal_monte_carlo(self) -> None:
        values = [0.1, 0.2, -0.1]
        observed = subject.terminal_gae(values, 1.0, gamma=1.0, lam=1.0)
        self.assertEqual(observed, [0.9, 0.8, 1.1])

    def test_advantage_routing_keeps_mc_parent_value_and_gae_critic(self) -> None:
        decisions = [
            _decision(0, 0.1, 1.0),
            _decision(1, 0.2, 1.0),
            _decision(2, 0.3, 1.0),
            _decision(0, 0.1, 1.0, seat=1),
            _decision(1, 0.2, 1.0, seat=1),
            _decision(2, 0.3, 1.0, seat=1),
        ]
        mc = subject.route_advantages(decisions, "mc")
        self.assertEqual(mc["mode"], "mc")
        for observed, expected in zip(
            [d.raw_advantage for d in decisions], [0.9, 0.8, 0.7, 0.9, 0.8, 0.7]
        ):
            self.assertAlmostEqual(observed, expected, places=6)

        gae = subject.route_advantages(
            decisions,
            "gae",
            critic_model=object(),
            value_fn=lambda _model, row: 0.0,
        )
        self.assertEqual(gae["mode"], "gae")
        self.assertEqual(
            [d.raw_advantage for d in decisions],
            [0.9025, 0.95, 1.0, 0.9025, 0.95, 1.0],
        )
        self.assertEqual(gae["critic_integrity"]["prediction_count"], 6)

    def test_invalid_history_or_missing_critic_fails_closed(self) -> None:
        decision = _decision(0, 0.0, 1.0)
        decision.rows[0]["history_features"] = torch.zeros(
            (subject.EXPECTED_HISTORY_LENGTH + 1, subject.EXPECTED_HISTORY_FEATURE_DIM)
        )
        with self.assertRaises(ValueError):
            subject.route_advantages([decision], "mc")
        with self.assertRaises(ValueError):
            subject.route_advantages([_decision(0, 0.0, 1.0)], "gae")

    def test_mc_preserves_finite_unbounded_raw_parent_value(self) -> None:
        decision = _decision(0, 1.25, -1.0)
        companion = _decision(0, 0.0, -1.0, seat=1)
        route = subject.route_advantages([decision, companion], "mc")
        self.assertAlmostEqual(decision.raw_advantage, -2.25)
        self.assertAlmostEqual(
            route["history_integrity"]["maximum_frozen_parent_value"], 1.25
        )

    def test_width48_trainable_contract(self) -> None:
        subject.structured_screen._configure(subject.DEFAULT_SEED, 1)
        model = subject.distill._model()
        parameters = subject.initializer._policy_parameters(model)
        parameter_ids = {id(parameter) for parameter in parameters}
        names = [
            name
            for name, parameter in model.named_parameters()
            if id(parameter) in parameter_ids
        ]
        self.assertEqual(names, [
            name for name, _ in model.named_parameters() if not name.startswith("value_head.")
        ])
        self.assertEqual(sum(parameter.numel() for parameter in parameters), 107233)
        self.assertFalse(model.value_head.weight.requires_grad)
        self.assertFalse(model.value_head.bias.requires_grad)


if __name__ == "__main__":
    raise SystemExit(unittest.main())
