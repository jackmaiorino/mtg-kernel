"""Blunder pricing driver: validity, manifest-only rows, coverage (collab LANES/pricing-20261111)."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(REPO / "tools" / "blunder_census")]
import blunder_price as bp  # noqa: E402


def write_pass(d: Path, tag: str, shard_rows: list[list[dict]], expected: int, complete=True, asked=None):
    shards = []
    for k, rows in enumerate(shard_rows):
        name = f"rows-{tag}-{k:02d}.jsonl"
        (d / name).write_text("".join(json.dumps(r) + "\n" for r in rows))
        cases = asked[k] if asked else [{"id": r["id"], "index": r["index"]} for r in rows]
        (d / f"cases-{tag}-{k:02d}.jsonl").write_text("".join(json.dumps(c) + "\n" for c in cases))
        shards.append({"shard": k, "file": name, "cases_file": f"cases-{tag}-{k:02d}.jsonl"})
    (d / f"run-{tag}.json").write_text(json.dumps({"complete": complete, "expected": expected, "shards": shards}))


def price_row(i, chosen, alt, invalid=(0, 0), unnatural=(0, 0)):
    arms = {"chosen": chosen, "alt": alt, "invalid": list(invalid), "unnatural": list(unnatural)}
    return {"kind": "price", "id": f"p/o0-t0-k0-s{i}", "class": "c", "index": i, "worlds": arms, "true": arms}


class Gap(unittest.TestCase):
    def test_paired_gap_and_labels(self):
        g = bp.case_gap({"chosen": [0, 0, 0, 0], "alt": [1, 1, 1, 1]})
        self.assertEqual(g["gap"], 1.0)
        self.assertEqual(g["label"], "clearly_positive")
        g = bp.case_gap({"chosen": [0, 1, 0, 1], "alt": [0, 1, 0, 1]})
        self.assertEqual(g["label"], "near_zero")
        self.assertTrue(g["value_chosen"] == 0.5 and not g["decided"])

    def test_any_unnatural_or_invalid_rollout_invalidates_the_case(self):
        self.assertIsNone(bp.case_gap({"chosen": [0, None], "alt": [1, 1]}))
        self.assertIsNone(bp.case_gap({"chosen": [0.5, 0.5], "alt": [0.5, 0.5], "invalid": [0, 2]}))
        self.assertIsNone(bp.case_gap({"chosen": [0.5, 0.5], "alt": [0.5, 0.5], "unnatural": [1, 0]}))
        # A natural draw keeps draw credit.
        self.assertEqual(bp.case_gap({"chosen": [0.5, 0.5], "alt": [1, 1]})["gap"], 0.5)


class Manifest(unittest.TestCase):
    def test_only_listed_files_count_and_coverage_is_checked(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            write_pass(d, "price", [[price_row(0, [0, 0], [1, 1])], [price_row(1, [0, 1], [0, 1])]], 2)
            (d / "rows-price-07.jsonl").write_text(json.dumps(price_row(9, [0, 0], [0, 0])) + "\n")  # stale
            self.assertEqual(len(bp.rows(d, "price")), 2)
            recs = bp.per_case(d, "price")
            self.assertEqual(len(recs), 4)
            table = bp.summarize_price(d, "price")
            self.assertEqual(table["c/worlds"]["cases"], 2)

    def test_duplicates_errors_and_short_coverage_are_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            write_pass(d, "dup", [[price_row(0, [0, 0], [1, 1])], [price_row(0, [0, 0], [1, 1])]], 2)
            with self.assertRaises(SystemExit):
                bp.rows(d, "dup")
            write_pass(d, "err", [[price_row(0, [0, 0], [1, 1]), {"kind": "error", "id": "x", "index": 1}]], 2)
            with self.assertRaises(SystemExit):
                bp.rows(d, "err")
            write_pass(d, "short", [[price_row(0, [0, 0], [1, 1])]], 2)
            with self.assertRaises(SystemExit):
                bp.rows(d, "short")
            write_pass(d, "failed", [[price_row(0, [0, 0], [1, 1])]], 1, complete=False)
            with self.assertRaises(SystemExit):
                bp.rows(d, "failed")

    def test_invalid_cases_are_counted_not_averaged(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            write_pass(d, "price", [[price_row(0, [0, 0], [1, 1]), price_row(1, [0.5, 0.5], [0.5, 0.5], invalid=(0, 2))]], 2)
            t = bp.summarize_price(d, "price")["c/worlds"]
            self.assertEqual((t["cases"], t["invalid_cases"], t["mean_gap"]), (1, 1, 1.0))


class ReviewRound2(unittest.TestCase):
    def test_rows_must_be_exactly_the_requested_cases(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            # Requested A and B, returned A and C: same count, wrong identity.
            a, c = price_row(0, [0, 0], [1, 1]), price_row(2, [0, 0], [1, 1])
            write_pass(d, "swap", [[a, c]], 2, asked=[[{"id": a["id"], "index": 0}, {"id": "p/o0-t0-k0-s1", "index": 1}]])
            with self.assertRaises(SystemExit):
                bp.rows(d, "swap")

    def test_a_class_with_only_invalid_cases_is_reported(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            write_pass(d, "price", [[price_row(0, [0.5, 0.5], [0.5, 0.5], unnatural=(1, 0))]], 1)
            t = bp.summarize_price(d, "price")
            self.assertEqual((t["c/worlds"]["cases"], t["c/worlds"]["invalid_cases"]), (0, 1))
            self.assertIsNone(t["c/worlds"]["mean_gap"])


class Bootstrap(unittest.TestCase):
    def test_fixed_seed(self):
        self.assertEqual(bp.bootstrap([0.0, 1.0, 0.5]), bp.bootstrap([0.0, 1.0, 0.5]))
        self.assertIsNone(bp.bootstrap([]))


if __name__ == "__main__":
    unittest.main()
