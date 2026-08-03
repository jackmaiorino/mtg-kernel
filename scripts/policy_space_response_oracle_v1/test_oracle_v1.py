#!/usr/bin/env python3

from __future__ import annotations

import unittest

import run_oracle_v1 as oracle


class OracleTests(unittest.TestCase):
    def test_splitmix_is_repeatable_and_antithetic_candidates_are_bounded(self) -> None:
        first = oracle.SplitMix64(oracle.ORACLE_SEED)
        second = oracle.SplitMix64(oracle.ORACLE_SEED)
        observed = [first.normal_approx() for _ in range(oracle.PARAMETER_COUNT)]
        self.assertEqual(observed, [second.normal_approx() for _ in range(oracle.PARAMETER_COUNT)])
        positive = oracle._clamp_delta([oracle.INITIAL_SIGMA * value for value in observed])
        negative = oracle._clamp_delta([-oracle.INITIAL_SIGMA * value for value in observed])
        self.assertTrue(all(abs(value) <= oracle.MAX_ABS_DELTA for value in positive))
        self.assertEqual(positive, [-value for value in negative])

    def test_f32_bits_round_trip(self) -> None:
        for value in (0.0, -0.0, 0.05, -0.05, 0.001):
            bits = oracle._f32_bits(value)
            self.assertEqual(len(bits), 8)
            self.assertEqual(
                oracle._f32(value),
                oracle.struct.unpack("<f", oracle.struct.pack("<I", int(bits, 16)))[0],
            )

    def test_formal_topology_uses_available_cpu_parallelism(self) -> None:
        self.assertEqual(oracle.POPULATION, oracle.MAX_WORKERS)
        self.assertEqual(oracle.POPULATION % 2, 0)
        self.assertLess(oracle.ELITES, oracle.POPULATION)


if __name__ == "__main__":
    unittest.main()
