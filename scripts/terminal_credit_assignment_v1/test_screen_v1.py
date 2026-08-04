#!/usr/bin/env python3

from __future__ import annotations

import math
from pathlib import Path
import sys
import unittest

import torch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import screen_v1 as subject


class TerminalCreditAssignmentTests(unittest.TestCase):
    def test_lambda_one_is_terminal_monte_carlo(self) -> None:
        values = [0.1, 0.2, -0.1]
        observed = subject._gae(values, 1.0, 1.0)
        expected = [0.9, 0.8, 1.1]
        for left, right in zip(observed, expected, strict=True):
            self.assertAlmostEqual(left, right, places=12)

    def test_lambda_0p95_uses_only_terminal_reward_and_values(self) -> None:
        observed = subject._gae([0.1, 0.2, -0.1], 1.0, 0.95)
        expected = [0.80775, 0.745, 1.1]
        for left, right in zip(observed, expected, strict=True):
            self.assertAlmostEqual(left, right, places=12)

    def test_score_vector_matches_softmax_gradient(self) -> None:
        latent = torch.tensor([[1.0, 0.0], [0.0, 1.0]])
        logits = torch.tensor([0.0, math.log(3.0)])
        observed = subject._score_vector(latent, logits, 0)
        torch.testing.assert_close(
            observed, torch.tensor([0.75, -0.75], dtype=torch.float64)
        )

    def test_gae_rejects_invalid_inputs(self) -> None:
        with self.assertRaises(RuntimeError):
            subject._gae([], 1.0, 0.95)
        with self.assertRaises(RuntimeError):
            subject._gae([0.0], 1.0, 1.01)


if __name__ == "__main__":
    unittest.main()
