"""Blunder census namespaces and summary helpers (collab LANES/census-20261110)."""
import sys
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(REPO / "tools" / "spy_reset"), str(REPO / "tools" / "blunder_census"), str(REPO / "python" / "tools")]
import pilot  # noqa: E402
import summarize  # noqa: E402

CENSUS = (pilot.CENSUS_PANEL_NS, pilot.CENSUS_SMOKE_NS)
KEYS = [(o, t, k) for o in range(9) for t in range(9) for k in range(12)]


class CensusNamespaces(unittest.TestCase):
    def test_admitted_and_fresh(self):
        self.assertEqual(CENSUS, ("census-20261110/a", "census-20261110/smoke"))
        self.assertEqual(pilot.PANEL_NAMESPACES[-2:], CENSUS)
        train = {row["seed"] for row in pilot.all_seeds()["train"]}
        for ns in CENSUS:
            fresh = {pilot.panel_seed(*key, ns) for key in KEYS}
            self.assertEqual(len(fresh), len(KEYS))
            self.assertFalse(fresh & train)
            for other in pilot.PANEL_NAMESPACES:
                if other != ns:
                    self.assertFalse(fresh & {pilot.panel_seed(*key, other) for key in KEYS}, (ns, other))


class Summary(unittest.TestCase):
    def test_own_deck_and_wilson(self):
        self.assertEqual(summarize.own_deck("b512-vs-b512/o4-t7-k0-s1"), 4)
        self.assertEqual(summarize.DECKS[4], "Spy")
        self.assertIsNone(summarize.wilson(0, 0))
        lo, hi = summarize.wilson(5, 10)
        self.assertLess(lo, 0.5)
        self.assertGreater(hi, 0.5)
        self.assertEqual(summarize.per100(3, 100)["per_100"], 3.0)


if __name__ == "__main__":
    unittest.main()
