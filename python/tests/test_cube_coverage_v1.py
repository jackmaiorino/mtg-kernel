from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path
import sys
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
import cube_coverage_v1 as cube_coverage


VINTAGE_CUBE = REPO_ROOT / "data/cube_v1/mtgo_vintage_cube_20261007.json"
VINTAGE_CUBE_GAPS = REPO_ROOT / "data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"


class CubeCoverageTest(unittest.TestCase):
    def test_classify_prefers_the_eternal_build_then_the_fdn_copy(self):
        eternal = {"Bolt": "full", "Breacher": "partial", "Both": "partial"}
        fdn_full = {"Commando", "Both"}
        self.assertEqual(cube_coverage.classify("Bolt", eternal, fdn_full), "full")
        self.assertEqual(cube_coverage.classify("Breacher", eternal, fdn_full), "partial")
        self.assertEqual(cube_coverage.classify("Commando", eternal, fdn_full), "fdn_only")
        self.assertEqual(cube_coverage.classify("Both", eternal, fdn_full), "fdn_only")
        self.assertEqual(cube_coverage.classify("Balance", eternal, fdn_full), "missing")

    def test_coverage_counts_each_group_and_lists_the_gaps(self):
        cube = {"cube": "Fixture Cube", "effective": "2026-10-07", "cards": [
            {"name": "Bolt", "group": "Red"}, {"name": "Balance", "group": "White"},
            {"name": "Commando", "group": "White"}, {"name": "Breacher", "group": "Blue"},
        ]}
        report = cube_coverage.coverage(cube, {"Bolt": "full", "Breacher": "partial"}, {"Commando"})
        self.assertEqual(report["card_status"], {"full": 1, "partial": 1, "fdn_only": 1, "missing": 1})
        self.assertEqual(report["groups"]["White"], {"full": 0, "partial": 0, "fdn_only": 1, "missing": 1})
        self.assertEqual(report["missing"], ["Balance"])
        self.assertEqual(cube_coverage.summary(report),
                         "Fixture Cube (2026-10-07): 1 of 4 Full, 1 partial, 1 FDN only, 1 missing")
        self.assertEqual(
            cube_coverage.differences(report, {"partial": ["Breacher"], "fdn_only": [], "missing": ["Balance", "Bolt"]}),
            ["fdn_only: Commando is not in the record", "missing: Bolt is recorded but no longer missing"],
        )
        cube["cards"].append({"name": "Bolt", "group": "Red"})
        with self.assertRaises(ValueError):
            cube_coverage.coverage(cube, {}, set())

    def test_vintage_cube_gaps_match_the_committed_record(self):
        cube = json.loads(VINTAGE_CUBE.read_text(encoding="utf-8"))
        self.assertEqual(len(cube["cards"]), 540)
        for card in cube["cards"]:
            self.assertTrue(card["name"].isascii(), card["name"])
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            status = cube_coverage.main([str(VINTAGE_CUBE), "--check", str(VINTAGE_CUBE_GAPS)])
        # The census line, for the test log.
        print(output.getvalue().strip())
        self.assertEqual(status, 0, output.getvalue())
        gaps = json.loads(VINTAGE_CUBE_GAPS.read_text(encoding="utf-8"))
        full = 540 - sum(len(gaps[key]) for key in cube_coverage.GAPS)
        self.assertIn(f"{full} of 540 Full", output.getvalue())


if __name__ == "__main__":
    unittest.main()
