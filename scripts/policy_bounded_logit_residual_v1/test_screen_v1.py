#!/usr/bin/env python3

from __future__ import annotations

import unittest

import torch

import screen_v1 as screen


class BoundedLogitTests(unittest.TestCase):
    def test_zero_clip_is_exact_initializer(self) -> None:
        initial = torch.tensor([1.0, -0.5, 0.25], dtype=torch.float32)
        trained = torch.tensor([-2.0, 3.0, 0.5], dtype=torch.float32)
        actual = screen.bounded_logits(initial, trained, 0.0)
        self.assertTrue(torch.equal(actual, initial.double()))

    def test_residual_is_bounded_after_weighted_centering(self) -> None:
        initial = torch.tensor([1.0, -0.5, 0.25], dtype=torch.float32)
        trained = torch.tensor([-2.0, 3.0, 0.5], dtype=torch.float32)
        actual = screen.bounded_logits(initial, trained, 0.06)
        self.assertLessEqual(float((actual - initial.double()).abs().max()), 0.0600001)
        self.assertTrue(torch.isfinite(actual).all())

    def test_common_trained_logit_shift_does_not_change_output(self) -> None:
        initial = torch.tensor([1.0, -0.5, 0.25], dtype=torch.float32)
        trained = torch.tensor([-2.0, 3.0, 0.5], dtype=torch.float32)
        one = screen.bounded_logits(initial, trained, 0.06)
        two = screen.bounded_logits(initial, trained + 17.0, 0.06)
        self.assertTrue(torch.allclose(one, two, atol=1.0e-7, rtol=0.0))

    def test_invalid_clip_rejected(self) -> None:
        with self.assertRaises(ValueError):
            screen.bounded_logits(torch.ones(2), torch.ones(2), -0.1)

    def test_fixed_grid_reaches_beyond_preflight_boundary(self) -> None:
        self.assertEqual(screen.CLIP_GRID[-1], 0.40)
        self.assertEqual(len(screen.CLIP_GRID), 13)


if __name__ == "__main__":
    unittest.main()
