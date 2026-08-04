#!/usr/bin/env python3

from __future__ import annotations

import unittest

import torch

import run_oracle_v1 as oracle


class PolicyBlockOracleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.initial, cls.trained, _ = oracle._base_states()

    def test_blocks_cover_every_policy_tensor_once(self) -> None:
        for name in self.initial["model_state_dict"]:
            if name.startswith("value_head."):
                continue
            self.assertIsInstance(oracle._block_index(name), int)

    def test_projection_is_bounded_and_budgeted(self) -> None:
        values = oracle._project_coefficients([1.0, -1.0] * 5)
        self.assertTrue(all(0.0 <= value <= oracle.COEFFICIENT_MAX for value in values))
        self.assertLessEqual(sum(values), oracle.COEFFICIENT_L1_BUDGET + 1.0e-6)

    def test_uniform_projection_reproduces_qualified_trust_projection(self) -> None:
        model = oracle._model_with_coefficients(
            self.initial,
            self.trained,
            [oracle.UNIFORM_SCALE] * oracle.BLOCK_COUNT,
        )
        expected = torch.load(
            r"D:\mtg-kernel-policy-only-structured-terminal-rung-v1\formal\projected-candidate.state.pt",
            map_location="cpu",
            weights_only=False,
        )["model_state_dict"]
        for name, tensor in model.state_dict().items():
            self.assertTrue(torch.equal(tensor, expected[name]), name)

    def test_topology_matches_population(self) -> None:
        self.assertEqual(oracle.POPULATION, base_workers := oracle.base.MAX_WORKERS)
        self.assertEqual(base_workers, 20)
        self.assertEqual((oracle.POPULATION - 2) % 2, 0)


if __name__ == "__main__":
    unittest.main()
