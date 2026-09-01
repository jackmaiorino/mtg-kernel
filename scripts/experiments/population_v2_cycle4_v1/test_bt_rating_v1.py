"""Focused tests for the cycle-4 Bradley-Terry rating fit."""

import math
import unittest

from bt_rating_v1 import BtRatingError, fit_bt_ratings, SCHEMA_V1


def doc(pairs, reference_id="ref"):
    return {"schema": SCHEMA_V1, "reference_id": reference_id, "pairs": pairs}


def pair(a, b, a_wins, b_wins, draws=0):
    return {"a_id": a, "b_id": b, "a_wins": a_wins, "b_wins": b_wins, "draws": draws}


class BtRatingTests(unittest.TestCase):
    def test_symmetric_pair_rates_equal(self):
        result = fit_bt_ratings(doc([pair("ref", "x", 50, 50)]))
        self.assertAlmostEqual(result["ratings_log_units"]["ref"], 0.0)
        self.assertAlmostEqual(result["ratings_log_units"]["x"], 0.0, places=9)
        entry = result["expected_scores"][0]
        self.assertEqual((entry["a_id"], entry["b_id"]), ("ref", "x"))
        self.assertAlmostEqual(entry["expected_a_score"], 0.5, places=9)

    def test_near_separated_full_panel_converges(self):
        # Review finding: the real 8-policy 28-matchup G=256 shape with every
        # result 256-0 except one 255-1 pair has a finite MLE and must
        # converge under the log-space criterion.
        ids = [f"m{i}" for i in range(7)] + ["ref"]
        pairs = []
        for i in range(8):
            for j in range(i + 1, 8):
                if (i, j) == (0, 7):
                    pairs.append(pair(ids[i], ids[j], 255, 1))
                else:
                    pairs.append(pair(ids[i], ids[j], 256, 0))
        result = fit_bt_ratings(doc(pairs))
        ratings = result["ratings_log_units"]
        self.assertGreater(ratings["m0"], ratings["m1"])
        self.assertGreater(ratings["m6"], ratings["ref"])
        self.assertEqual(len(result["expected_scores"]), 28)

    def test_known_two_model_rate_recovers_log_odds(self):
        # 75/25 implies strength ratio 3, rating log(3).
        result = fit_bt_ratings(doc([pair("x", "ref", 75, 25)]))
        self.assertAlmostEqual(
            result["ratings_log_units"]["x"], math.log(3.0), places=6
        )

    def test_reference_is_always_zero(self):
        result = fit_bt_ratings(
            doc([pair("ref", "x", 60, 40), pair("x", "y", 70, 30), pair("ref", "y", 80, 20)])
        )
        self.assertEqual(result["ratings_log_units"]["ref"], 0.0)

    def test_transitive_chain_orders_correctly(self):
        result = fit_bt_ratings(
            doc([pair("a", "b", 60, 40), pair("b", "ref", 60, 40), pair("a", "ref", 68, 32)])
        )
        ratings = result["ratings_log_units"]
        self.assertGreater(ratings["a"], ratings["b"])
        self.assertGreater(ratings["b"], ratings["ref"])

    def test_draws_count_half(self):
        # 50 wins + 50 draws vs 0 wins + 50 draws = scores 75 vs 25.
        with_draws = fit_bt_ratings(doc([pair("x", "ref", 50, 0, 50)]))
        pure = fit_bt_ratings(doc([pair("x", "ref", 75, 25)]))
        self.assertAlmostEqual(
            with_draws["ratings_log_units"]["x"],
            pure["ratings_log_units"]["x"],
            places=9,
        )

    def test_determinism(self):
        pairs = [pair("a", "b", 61, 39), pair("b", "ref", 55, 45), pair("a", "ref", 66, 34)]
        first = fit_bt_ratings(doc(pairs))
        second = fit_bt_ratings(doc(pairs))
        self.assertEqual(first, second)

    def test_disconnected_graph_fails_closed(self):
        with self.assertRaises(BtRatingError):
            fit_bt_ratings(doc([pair("ref", "x", 10, 10), pair("y", "z", 10, 10)]))

    def test_degenerate_all_wins_fails_closed(self):
        with self.assertRaises(BtRatingError):
            fit_bt_ratings(doc([pair("x", "ref", 100, 0)]))

    def test_missing_reference_fails_closed(self):
        with self.assertRaises(BtRatingError):
            fit_bt_ratings(doc([pair("a", "b", 10, 10)], reference_id="ref"))

    def test_schema_and_shape_violations_fail_closed(self):
        with self.assertRaises(BtRatingError):
            fit_bt_ratings({"schema": "wrong", "reference_id": "ref", "pairs": []})
        with self.assertRaises(BtRatingError):
            fit_bt_ratings(doc([pair("x", "x", 10, 10)]))
        with self.assertRaises(BtRatingError):
            fit_bt_ratings(doc([{**pair("x", "ref", 10, 10), "extra": 1}]))
        with self.assertRaises(BtRatingError):
            fit_bt_ratings(doc([pair("x", "ref", 0, 0, 0)]))


if __name__ == "__main__":
    unittest.main()
