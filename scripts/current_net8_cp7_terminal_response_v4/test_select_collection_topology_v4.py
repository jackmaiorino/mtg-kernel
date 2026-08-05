from __future__ import annotations

import unittest

from collect_corpus_v4 import _select_topology_attempt


def attempt(attempt_id: str, workers: int, task_pairs: int, rate: float) -> dict:
    return {
        "attempt_id": attempt_id,
        "workers": workers,
        "task_pairs": task_pairs,
        "games_per_second": rate,
    }


class SelectCollectionTopologyV4Test(unittest.TestCase):
    def test_slower_alternative_selects_first_attempt(self) -> None:
        selected = _select_topology_attempt(
            [
                attempt("attempt-01", 8, 4, 0.4401880019294101),
                attempt("attempt-02", 16, 2, 0.32349047958145544),
            ]
        )
        self.assertEqual(selected["attempt_id"], "attempt-01")

    def test_faster_alternative_is_selected(self) -> None:
        selected = _select_topology_attempt(
            [
                attempt("attempt-01", 8, 4, 0.4),
                attempt("attempt-02", 16, 2, 0.5),
            ]
        )
        self.assertEqual(selected["attempt_id"], "attempt-02")

    def test_exact_rate_tie_uses_fewer_workers(self) -> None:
        selected = _select_topology_attempt(
            [
                attempt("attempt-01", 8, 4, 0.4),
                attempt("attempt-02", 16, 2, 0.4),
            ]
        )
        self.assertEqual(selected["attempt_id"], "attempt-01")

    def test_alternative_is_rejected_if_first_screen_met_trigger(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "alternative topology was not authorized"):
            _select_topology_attempt(
                [
                    attempt("attempt-01", 8, 4, 0.6),
                    attempt("attempt-02", 16, 2, 0.7),
                ]
            )


if __name__ == "__main__":
    unittest.main()
