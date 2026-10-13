import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("practice_report", ROOT / "tools/spy_reset/practice_report.py")
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


class MissingPractice(unittest.TestCase):
    def test_unknown_spy_events_have_full_best_worst_bounds(self):
        cases = {(report.SPY, 0, 0, 0): {"complete": False, "score": None, "labels": None},
                 (report.SPY, 0, 0, 1): {"complete": True, "score": 0.0, "labels": {}}}
        key = (report.SPY, 0, 0)
        self.assertEqual(report.block_stats(cases, "worst")[key], [0, 2, 0, 0, 0, 0])
        self.assertEqual(report.block_stats(cases, "best")[key], [1, 2, 1, 1, 1, 1])
        self.assertEqual(report.block_stats(cases, "observed")[key], [0, 1, 0, 0, 0, 0])

    def test_known_failure_remains_zero_under_both_bounds(self):
        cases = {(report.SPY, 0, 0, 0): {"complete": True, "score": 0.0, "labels": {}}}
        self.assertEqual(report.block_stats(cases, "best"), report.block_stats(cases, "worst"))


if __name__ == "__main__":
    unittest.main()
