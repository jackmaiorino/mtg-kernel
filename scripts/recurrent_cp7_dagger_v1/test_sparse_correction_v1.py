#!/usr/bin/env python3
"""Focused tests for sparse CP7 disagreement correction."""

from __future__ import annotations

import unittest

import run_sparse_correction_v1 as sparse


class SparseCorrectionTests(unittest.TestCase):
    def test_passing_checkpoint_ranks_before_reject(self) -> None:
        passing = {
            "overall": {
                "relative_nll_improvement": 0.06,
                "top1_delta": 0.04,
                "mean_total_variation": 0.02,
                "p90_total_variation": 0.08,
                "maximum_absolute_log_ratio": 0.20,
            },
            "by_candidate_seat": {
                str(seat): {
                    "relative_nll_improvement": 0.05,
                    "top1_delta": 0.03,
                    "mean_total_variation": 0.02,
                    "p90_total_variation": 0.08,
                    "maximum_absolute_log_ratio": 0.20,
                }
                for seat in (0, 1)
            },
        }
        rejected = {
            **passing,
            "overall": {**passing["overall"], "top1_delta": 0.01},
        }
        self.assertLess(sparse._checkpoint_rank(passing), sparse._checkpoint_rank(rejected))


if __name__ == "__main__":
    unittest.main()
