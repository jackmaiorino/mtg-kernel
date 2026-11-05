"""Panel tooling for G(i) (collab LANES/g-references-20261028, DESIGN.md section 5): the two fresh seed namespaces
(the panel name in every seed key, disjoint from every registered panel namespace and from the study's training
namespaces), the mirror panel shape (the eight Terror-free mirrors x 48 seed families x 2 seats = 768 cases), the
unchanged cross shape (81 x 12 x 2 = 1,944) and its unchanged default plans, and plan annotations."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools" / "spy_reset"))
sys.path.insert(0, str(ROOT / "python" / "tools"))
import panel  # noqa: E402
import pilot  # noqa: E402

G = (pilot.G_MIRROR_NS, pilot.G_CROSS_NS)
TRAINING = tuple(f"g-references-v1/{name}" for name in ("order", "train", "history", "init", "panel"))
CROSS_KEYS = [(o, t, k) for o in range(pilot.DECKS) for t in range(pilot.DECKS) for k in range(pilot.PANEL_REPEATS)]
MIRROR_KEYS = [(d, d, k) for d in panel.MIRROR_DECKS for k in range(panel.MIRROR_FAMILIES)]


def source(tag, sha="2" * 64):
    return {"play_import": {"path": f"{tag}-import.json", "sha256": "1" * 64},
            "feature_transfer": {"expected_feature_contract_digest": "c", "expected_feature_encoding_digest": "e"},
            "checkpoint": {"path": f"{tag}.json", "sha256": sha}}


class GNamespaces(unittest.TestCase):
    def test_the_two_namespaces_are_registered_after_every_earlier_one(self):
        self.assertEqual(G, ("g-references-v1/panel/mirror", "g-references-v1/panel/cross"))
        self.assertEqual(pilot.PANEL_NAMESPACES[13:15], G)
        self.assertEqual(pilot.PANEL_NAMESPACES[10:13], (pilot.D1_PANEL_NS_A, pilot.D1_PANEL_NS_B, pilot.D1_H2H_NS))
        self.assertEqual(len(set(pilot.PANEL_NAMESPACES)), len(pilot.PANEL_NAMESPACES))

    def test_the_names_differ_from_the_training_namespaces(self):
        for ns in G:
            self.assertNotIn(ns, TRAINING)
            self.assertNotIn(ns, pilot.PANEL_NAMESPACES[:13])

    def test_the_seeds_are_fresh_and_never_shared_between_the_two_panels(self):
        mirror = {pilot.panel_seed(*key, pilot.G_MIRROR_NS) for key in MIRROR_KEYS}
        cross = {pilot.panel_seed(*key, pilot.G_CROSS_NS) for key in CROSS_KEYS}
        self.assertEqual((len(mirror), len(cross)), (len(MIRROR_KEYS), len(CROSS_KEYS)))
        self.assertFalse(mirror & cross)
        for other in pilot.PANEL_NAMESPACES[:13]:
            earlier = {pilot.panel_seed(*key, other) for key in CROSS_KEYS + MIRROR_KEYS}
            self.assertFalse((mirror | cross) & earlier, other)
        training = {pilot.seed([ns, o, t, k]) for ns in TRAINING for o, t, k in CROSS_KEYS + MIRROR_KEYS}
        self.assertFalse((mirror | cross) & training)

    def test_an_unregistered_namespace_is_refused(self):
        for ns in ("g-references-v1/panel", "g-references-v1/panel/mirrors", "g-references-v1/train"):
            with self.assertRaises(ValueError):
                pilot.panel_seed(0, 0, 0, ns)
            with self.assertRaises(ValueError):
                panel.mirror_cases(ns)


class MirrorShape(unittest.TestCase):
    def test_the_mirror_roster(self):
        self.assertEqual(pilot.ndb.DECK_NAMES[panel.TERROR], "Terror")
        self.assertEqual(panel.TERROR, 6)
        self.assertEqual(panel.MIRROR_DECKS, (0, 1, 2, 3, 4, 5, 7, 8))
        cases = panel.mirror_cases(pilot.G_MIRROR_NS)
        self.assertEqual(len(cases), 768)
        self.assertEqual({(c["own"], c["other"], c["repeat"]) for c in cases}, set(MIRROR_KEYS))
        self.assertEqual(len({(c["own"], c["repeat"], c["seat"]) for c in cases}), 768)
        for c in cases:
            self.assertEqual(c["own"], c["other"])
            self.assertEqual(c["seed"], pilot.seed([pilot.G_MIRROR_NS, c["own"], c["own"], c["repeat"]]))
            self.assertEqual(c["starting_player"], c["repeat"] % 2)

    def test_a_mirror_plan_records_its_shape_and_768_cases(self):
        decks = pilot.load_decks()
        with tempfile.TemporaryDirectory() as tmp:
            plan = panel.build("G1-b0247", source("g"), "r2-s1-mirror", panel.wrap(source("b", "3" * 64)), decks,
                               Path(tmp) / "m", 16, pilot.G_MIRROR_NS, shape="mirror")
            requests = [json.loads(Path(item["request"]["path"]).read_text()) for item in plan["requests"]]
        self.assertEqual((plan["shape"], plan["seed_namespace"], plan["cases"]), ("mirror", pilot.G_MIRROR_NS, 768))
        planned = [c for item in plan["requests"] for c in item["cases"]]
        self.assertEqual(sorted(json.dumps(c, sort_keys=True) for c in planned),
                         sorted(json.dumps(c, sort_keys=True) for c in panel.mirror_cases(pilot.G_MIRROR_NS)))
        for item, request in zip(plan["requests"], requests):
            for case, match in zip(item["cases"], request["matches"]):
                self.assertEqual(match["config"]["deck_ids"][0], match["config"]["deck_ids"][1])
                self.assertEqual(match["config"]["seed"], case["seed"])
            self.assertTrue(all(len(item["cases"]) <= 1024 for item in plan["requests"]))

    def test_the_cross_shape_and_its_default_plans_are_unchanged(self):
        decks = pilot.load_decks()
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            default = panel.build("ck", source("ck"), "H2H", source("opp"), decks, tmp / "a", 16, pilot.D1_H2H_NS)
            a = (tmp / "a" / "plan.json").read_bytes()
            panel.build("ck", source("ck"), "H2H", source("opp"), decks, tmp / "a", 16, pilot.D1_H2H_NS,
                        shape="cross", annotations=None)
            b = (tmp / "a" / "plan.json").read_bytes()
            cross = panel.build("ck", source("ck"), "x", source("opp"), decks, tmp / "c", 16, pilot.G_CROSS_NS)
        self.assertEqual(a, b)
        self.assertNotIn("shape", default)
        self.assertEqual(default["cases"], 1944)
        self.assertEqual(panel.cases(pilot.D1_H2H_NS), panel.shape_cases("cross", pilot.D1_H2H_NS))
        self.assertEqual((cross["cases"], cross["seed_namespace"]), (1944, pilot.G_CROSS_NS))
        with self.assertRaises(ValueError):
            panel.shape_cases("square", pilot.G_CROSS_NS)

    def test_annotations_are_added_and_never_replace_a_plan_field(self):
        decks = pilot.load_decks()
        with tempfile.TemporaryDirectory() as tmp:
            plan = panel.build("ck", source("ck"), "k", source("opp"), decks, Path(tmp) / "a", 16, pilot.G_MIRROR_NS,
                               shape="mirror", annotations={"b512_seat": "opponent"})
            self.assertEqual(json.loads((Path(tmp) / "a" / "plan.json").read_text())["b512_seat"], "opponent")
            self.assertEqual(plan["b512_seat"], "opponent")
            with self.assertRaises(ValueError):
                panel.build("ck", source("ck"), "k", source("opp"), decks, Path(tmp) / "b", 16, pilot.G_MIRROR_NS,
                            shape="mirror", annotations={"cases": 1})


if __name__ == "__main__":
    unittest.main()
