#!/usr/bin/env python3
"""Focused offline checks for the population Store CP7 panel runner."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


MODULE = Path(__file__).with_name("run_cp7_store_panel_v1.py")
SPEC = importlib.util.spec_from_file_location("panel", MODULE)
assert SPEC is not None and SPEC.loader is not None
panel = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(panel)


class PanelRunnerTest(unittest.TestCase):
    def test_terminal_wdl_aggregation(self) -> None:
        summary = panel.aggregate_terminal_wdl([
            {"label": "a", "by_seat": {"p0": "win", "p1": "loss"}},
            {"label": "a", "by_seat": {"p0": "draw", "p1": "draw"}},
        ], ["a"])
        self.assertEqual(summary["a"]["overall_wdl"], {"win": 1, "draw": 2, "loss": 1})
        self.assertEqual(summary["a"]["by_seat_wdl"]["p0"],
                         {"win": 1, "draw": 1, "loss": 0})

    def test_duplicate_model_labels_fail_before_store_access(self) -> None:
        with self.assertRaises(ValueError):
            panel.main([
                "--evidence-root", "new", "--generation", "1024", "--mode", "smoke",
                "--base-seed", "1", "--pairs", "1", "--scorer-exe", "scorer",
                "--mage-repo", "mage", "--source-database", "cards", "--maven", "mvn",
                "--model", "same=one", "--model", "same=two", "--model", "third=three",
            ])

    def test_duplicate_model_roots_fail_before_store_access(self) -> None:
        with self.assertRaises(ValueError):
            panel.main([
                "--evidence-root", "new", "--generation", "1024", "--mode", "smoke",
                "--base-seed", "1", "--pairs", "1", "--scorer-exe", "scorer",
                "--mage-repo", "mage", "--source-database", "cards", "--maven", "mvn",
                "--model", "one=root", "--model", "two=root", "--model", "three=other",
            ])


if __name__ == "__main__":
    unittest.main()
