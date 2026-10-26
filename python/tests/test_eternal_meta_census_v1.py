from __future__ import annotations

from pathlib import Path
import sys
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
import eternal_meta_census_v1 as census


class EternalMetaCensusTest(unittest.TestCase):
    def test_parse_list_splits_at_blank_line_and_sideboard_row(self):
        self.assertEqual(
            census.parse_list("4 Lightning Bolt\n20 Mountain\n\n2 Smash to Smithereens\n"),
            {"mainboard": [[4, "Lightning Bolt"], [20, "Mountain"]],
             "sideboard": [[2, "Smash to Smithereens"]]},
        )
        self.assertEqual(
            census.parse_list("4 Counterspell\nSideboard\n1 Hydroblast"),
            {"mainboard": [[4, "Counterspell"]], "sideboard": [[1, "Hydroblast"]]},
        )
        with self.assertRaises(ValueError):
            census.parse_list("Lightning Bolt")

    def test_playable_share_counts_only_fully_supported_mainboards(self):
        snapshot = {"source": "fixture", "captured": "2026-10-26", "formats": {"legacy": [
            {"archetype": "a", "share": 10.0, "mainboard": [[4, "Bolt"]], "sideboard": [[1, "Pyre"]]},
            {"archetype": "b", "share": 5.0, "mainboard": [[4, "Bolt"], [4, "Ported"]], "sideboard": []},
            {"archetype": "c", "share": 2.0, "mainboard": [[4, "Daze"]], "sideboard": []},
            {"archetype": "d", "share": 1.0, "mainboard": [[4, "Bolt"]], "sideboard": [[1, "Lurrus"]],
             "companion": "Lurrus"},
        ]}}
        report = census.census(snapshot, full={"Bolt"}, fdn_only={"Ported"})
        legacy = report["formats"]["legacy"]
        self.assertEqual(legacy["playable_share"], 10.0)
        self.assertEqual(legacy["card_status"], {"full": 1, "fdn_only": 1, "missing": 3})
        self.assertEqual(legacy["unsupported_by_share"],
                         [["Pyre", 10.0, "missing"], ["Ported", 5.0, "fdn_only"], ["Daze", 2.0, "missing"],
                          ["Lurrus", 1.0, "missing"]])
        # The companion is required, so deck d is not playable.
        self.assertEqual(legacy["decks"][3]["unsupported_mainboard_cards"], ["Lurrus"])
        self.assertEqual(report["union_card_status"], {"full": 1, "fdn_only": 1, "missing": 3})

    def test_registries_load_with_disjoint_status(self):
        full, fdn_only = census.load_status()
        self.assertIn("Lightning Bolt", full)
        self.assertIn("Cut Down", full)  # Standard
        self.assertFalse(full & fdn_only)


if __name__ == "__main__":
    unittest.main()
