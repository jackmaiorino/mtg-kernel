"""Panel seed namespaces for W4 Step C (collab LANES/w4-combat-20261030, STEP-C-DESIGN.md section 3): two reference
namespaces and the head-to-head namespace, each fresh against every registered namespace, and the refusal of an
unregistered one."""

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools" / "spy_reset"))
import pilot  # noqa: E402

W4C = (pilot.W4C_PANEL_NS_A, pilot.W4C_PANEL_NS_B, pilot.W4C_H2H_NS)
KEYS = [(o, t, k) for o in range(pilot.DECKS) for t in range(pilot.DECKS) for k in range(pilot.PANEL_REPEATS)]


class W4CNamespaces(unittest.TestCase):
    def test_the_three_namespaces_are_admitted_and_their_seeds_are_fresh(self):
        self.assertEqual(W4C, ("w4c-panel-20261031a", "w4c-panel-20261031b", "w4c-h2h-20261031"))
        self.assertEqual(pilot.PANEL_NAMESPACES[-3:], W4C)
        for ns in W4C:
            fresh = {pilot.panel_seed(*key, ns) for key in KEYS}
            self.assertEqual(len(fresh), len(KEYS))
            for other in pilot.PANEL_NAMESPACES:
                if other != ns:
                    self.assertFalse(fresh & {pilot.panel_seed(*key, other) for key in KEYS}, (ns, other))

    def test_an_unregistered_namespace_is_refused(self):
        with self.assertRaises(ValueError):
            pilot.panel_seed(0, 0, 0, "w4c-panel-20261031c")


if __name__ == "__main__":
    unittest.main()
