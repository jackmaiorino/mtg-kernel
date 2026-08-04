#!/usr/bin/env python3

from __future__ import annotations

from pathlib import Path
import sys
import unittest

import torch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import matched_objective_v1 as subject


class MatchedObjectiveTests(unittest.TestCase):
    def test_bootstrap_identical_positive_difference(self) -> None:
        gae = {index: 0.2 for index in range(16)}
        mc = {index: 0.1 for index in range(16)}
        result = subject._bootstrap_difference(gae, mc)
        self.assertAlmostEqual(result["point_mean"], 0.1)
        self.assertGreater(result["lower_value"], 0.0)

    def test_bootstrap_rejects_mismatched_pairs(self) -> None:
        with self.assertRaises(RuntimeError):
            subject._bootstrap_difference({0: 1.0}, {1: 1.0})

    def test_weighted_quantile(self) -> None:
        self.assertEqual(
            subject._weighted_quantile([(1.0, 1.0), (3.0, 3.0)], 0.5), 3.0
        )

    def test_joint_log_probability_sums_substeps(self) -> None:
        class Decision:
            rows = [
                {
                    "credit_policy_latent": torch.tensor([[1.0], [0.0]]),
                    "selected_index": 0,
                },
                {
                    "credit_policy_latent": torch.tensor([[0.0], [1.0]]),
                    "selected_index": 1,
                },
            ]

        observed = subject._joint_log_probability(
            torch.tensor([1.0]), torch.tensor(0.0), Decision()
        )
        expected = 2.0 * torch.log_softmax(torch.tensor([1.0, 0.0]), dim=0)[0]
        torch.testing.assert_close(observed, expected)


if __name__ == "__main__":
    unittest.main()
