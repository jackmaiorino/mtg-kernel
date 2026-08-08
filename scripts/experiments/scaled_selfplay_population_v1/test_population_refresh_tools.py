#!/usr/bin/env python3
"""Focused tests for payoff and refresh arithmetic."""

from __future__ import annotations

import unittest
from decimal import Decimal, getcontext

from scripts.experiments.scaled_selfplay_population_v1.finalize_population_refresh import (
    ROLE_PAIRS,
    integerize,
    project_constraints,
)


class PopulationRefreshArithmeticTests(unittest.TestCase):
    def setUp(self) -> None:
        getcontext().prec = 60

    def assert_integer_constraints(self, units: list[int]) -> None:
        self.assertEqual(sum(units), 1_000_000)
        self.assertTrue(all(0 < unit <= 250_000 for unit in units))
        self.assertTrue(
            all(units[left] + units[right] >= 200_000 for left, right in ROLE_PAIRS)
        )

    def test_equal_weights_are_unchanged(self) -> None:
        projected, clipping = project_constraints([Decimal("0.125")] * 8)
        self.assertFalse(clipping)
        self.assertEqual(projected, [Decimal("0.125")] * 8)
        units = integerize(projected)
        self.assertEqual(units, [125_000] * 8)

    def test_caps_and_role_floors_are_projected(self) -> None:
        projected, clipping = project_constraints(
            [
                Decimal("0.60"),
                Decimal("0.01"),
                Decimal("0.01"),
                Decimal("0.01"),
                Decimal("0.15"),
                Decimal("0.05"),
                Decimal("0.12"),
                Decimal("0.05"),
            ]
        )
        self.assertTrue(clipping)
        units = integerize(projected)
        self.assert_integer_constraints(units)


if __name__ == "__main__":
    unittest.main()
