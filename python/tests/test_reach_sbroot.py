"""s4a-sbroot tooling: candidate descriptions, spec predicates and the native decision-point split."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools" / "reach"))
import sbroot  # noqa: E402
from sbcapture import BRIDGE_KIND, decision_points  # noqa: E402


def rows(*picks):
    return [{"row": r, "seat": 1, "kinds": ["choose_blocker_inclusion"] * 2, "names": [[], []]} for r in picks]


class DescribeTests(unittest.TestCase):
    def test_kinds(self):
        self.assertEqual(sbroot.describe({"kind": "declare_block", "attacker": None}), "no block")
        self.assertEqual(sbroot.describe({"kind": "declare_block", "attacker": {"card_name": "Writhing Chrysalis",
                                                                                "controller_seat": "p0"}}),
                         "block Writhing Chrysalis (p0)")
        self.assertEqual(sbroot.describe({"kind": "choose_target", "target": {"object": {
            "card_name": "Swamp", "controller_seat": "p0"}}}), "target Swamp (p0)")
        self.assertEqual(sbroot.describe({"kind": "cast_spell", "source": {"card_name": "Cast Down"}}),
                         "cast_spell Cast Down")
        self.assertEqual(sbroot.describe({"kind": "pass"}), "pass")

    def test_predicates(self):
        cast = {"describe": "cast_spell Cast Down"}
        self.assertTrue(sbroot.matches(cast, "cast_spell Cast Down"))
        self.assertFalse(sbroot.matches({"not": cast}, "cast_spell Cast Down"))
        self.assertTrue(sbroot.matches({"describe_prefix": "target Swamp (p0)"}, "target Swamp (p0)"))
        self.assertTrue(sbroot.verdict(cast, ["cast_spell Cast Down"]))
        self.assertFalse(sbroot.verdict(cast, ["pass"]))
        self.assertIsNone(sbroot.verdict(cast, ["pass", "cast_spell Cast Down"]))

    def test_bridge_kinds_name_flat_kinds(self):
        self.assertEqual(BRIDGE_KIND["choose_boolean"], "choose_effect_boolean")
        self.assertNotIn("choose_target", BRIDGE_KIND)


class DecisionPointTests(unittest.TestCase):
    def test_single_point(self):
        seqs = {0: rows(0), 1: rows(1)}
        (path, n, labels, recorded), = decision_points(seqs, 50, played=1)
        self.assertEqual((n, labels, recorded, len(path)), (50, {0: [0], 1: [1]}, True, 1))

    def test_block_scans(self):
        # Recorded: exclude the first pair, include the second. One alternative
        # includes the first pair; another excludes both.
        seqs = {0: rows(0, 1), 1: rows(1), 2: rows(0, 0)}
        points = list(decision_points(seqs, 108, played=0))
        self.assertEqual([(n, labels, rec) for _, n, labels, rec in points],
                         [(108, {0: [0, 2], 1: [1]}, True), (109, {1: [0], 0: [2]}, True)])

    def test_point_off_the_recorded_line(self):
        seqs = {0: rows(1), 1: rows(0, 0), 2: rows(0, 1)}
        points = list(decision_points(seqs, 171, played=0))
        self.assertEqual([(n, rec, [r["row"] for r in path[:-1]]) for path, n, _, rec in points],
                         [(171, True, []), (172, False, [0])])
        self.assertEqual(points[0][0][-1]["row"], 1)  # the recorded row at a recorded-line point


if __name__ == "__main__":
    unittest.main()
