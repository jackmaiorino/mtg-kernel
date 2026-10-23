"""Panel tooling for OH2 (collab LANES/ordinary-scaling-oh2-20261023, DESIGN.md section 3): its three fresh seed
namespaces, and the Spy distillation analyzer's reference list (default T1 and H unchanged; another list pooled as
its equal mean)."""

import json
import random
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tools" / "spy_reset"))
sys.path.insert(0, str(ROOT / "python" / "tools"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from tools.spy_distill import analyze  # noqa: E402
import panel  # noqa: E402
import pilot  # noqa: E402

from test_spy_distill_analysis import constant, panel as write_panel, random_outcome  # noqa: E402

OH2 = (pilot.OH2_PANEL_NS_A, pilot.OH2_PANEL_NS_B, pilot.OH2_SCREEN_NS)


def source(tag):
    return {"play_import": {"path": f"{tag}-import.json", "sha256": "1" * 64},
            "feature_transfer": {"expected_feature_contract_digest": "c", "expected_feature_encoding_digest": "e"},
            "checkpoint": {"path": f"{tag}.json", "sha256": "2" * 64}}


class Oh2Namespaces(unittest.TestCase):
    def test_the_three_namespaces_are_admitted_and_their_seeds_are_fresh(self):
        self.assertEqual(OH2, ("oh2-panel-20261024a", "oh2-panel-20261024b", "oh2-screen-20261024"))
        keys = [(o, t, k) for o in range(pilot.DECKS) for t in range(pilot.DECKS) for k in range(pilot.PANEL_REPEATS)]
        for ns in OH2:
            self.assertIn(ns, pilot.PANEL_NAMESPACES)
            fresh = {pilot.panel_seed(*key, ns) for key in keys}
            self.assertEqual(len(fresh), len(keys))
            for other in pilot.PANEL_NAMESPACES:
                if other != ns:
                    self.assertFalse(fresh & {pilot.panel_seed(*key, other) for key in keys}, (ns, other))

    def test_namespaces_a_and_b_build_different_schedules_for_the_same_pair(self):
        decks = pilot.load_decks()
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            a = panel.build("ck", source("ck"), "Ha", source("h"), decks, tmp / "a", 16, pilot.OH2_PANEL_NS_A)
            b = panel.build("ck", source("ck"), "Hb", source("h"), decks, tmp / "b", 16, pilot.OH2_PANEL_NS_B)
        self.assertEqual((a["seed_namespace"], b["seed_namespace"]), OH2[:2])
        self.assertNotEqual(a["schedule_sha256"], b["schedule_sha256"])


class ReferenceList(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        patcher = mock.patch.object(analyze, "DRAWS", 300)
        patcher.start()
        self.addCleanup(patcher.stop)

    def build(self, refs, roles=("parent", "D1U"), seed=0):
        entries = [write_panel(self.root, f"{lin}-{role}", opp, random_outcome(seed + 97 * n))
                   for n, (lin, role, opp) in enumerate((lin, role, opp) for lin in ("r1", "r2") for role in roles
                                                        for opp in refs)]
        return {"panels": entries}

    def test_two_references_pool_exactly_as_t1_plus_h_over_two(self):
        rng = random.Random(5)
        for _ in range(2000):
            values = {"T1": {"m": rng.choice((-0.0, 0.0, rng.uniform(-1, 1)))},
                      "H": {"m": rng.choice((-0.0, 0.0, rng.uniform(-1, 1)))}}
            got = analyze.pooled(values, "m")
            want = (values["T1"]["m"] + values["H"]["m"]) / 2
            self.assertEqual(json.dumps(got), json.dumps(want))

    def test_default_points_and_intervals_equal_the_explicit_t1_h_list(self):
        data = analyze.load(self.build(("T1", "H")))
        todo = analyze.plan(data, analyze.SCREEN[:1])
        self.assertEqual(todo, analyze.plan(data, analyze.SCREEN[:1], ("T1", "H")))
        metrics = analyze.METRICS + analyze.EXTRA_METRICS
        self.assertEqual(analyze.points(data, todo, metrics), analyze.points(data, todo, metrics, ("T1", "H")))
        self.assertEqual(analyze.bootstrap(data, todo, metrics),
                         analyze.bootstrap(data, todo, metrics, analyze.BOOT_SEED, ("T1", "H")))
        point = analyze.points(data, todo, metrics)
        for key, value in point.items():
            if key[3] == "pooled":
                t1, h = point[key[:3] + ("T1",) + key[4:]], point[key[:3] + ("H",) + key[4:]]
                # (d_T1 + d_H) / 2 - (c_T1 + c_H) / 2 as the literal formula computed it.
                self.assertAlmostEqual(value, (t1 + h) / 2, places=12)

    def test_load_accepts_only_the_given_reference_list(self):
        index = self.build(("T1", "H", "R3"))
        with self.assertRaises(ValueError):
            analyze.load(index)
        data = analyze.load(index, opponents=("T1", "H", "R3"))
        self.assertEqual({opp for _, opp in data}, {"T1", "H", "R3"})
        with self.assertRaises(ValueError):
            analyze.load(index, opponents=("T1", "H"))

    def test_three_references_pool_as_their_equal_mean_on_the_same_draw(self):
        refs = ("T1", "H", "R3")
        cells = {"T1": lambda o, t, k, s: 1, "H": lambda o, t, k, s: 0.5,
                 "R3": lambda o, t, k, s: 1 if k % 2 == 0 else 0}
        entries = []
        for lin in ("r1", "r2"):
            for opp in refs:
                entries.append(write_panel(self.root, f"{lin}-D1U", opp, constant(cells[opp])))
                entries.append(write_panel(self.root, f"{lin}-parent", opp, constant(lambda o, t, k, s: 0.5)))
        data = analyze.load({"panels": entries}, opponents=refs)
        todo = analyze.plan(data, analyze.SCREEN[:1], refs)
        point = analyze.points(data, todo, ("nine_deck",), refs)
        ci = analyze.bootstrap(data, todo, ("nine_deck",), analyze.BOOT_SEED, refs)
        for lin in ("r1", "r2"):
            for bound in analyze.BOUNDS:
                key = lambda opp: (lin, "D1U-parent", "nine_deck", opp, bound)  # noqa: E731
                self.assertEqual([point[key(o)] for o in refs], [0.5, 0.0, 0.0])
                self.assertAlmostEqual(point[key("pooled")], 0.5 / 3, places=15)
                self.assertEqual(ci[key("T1")], [0.5, 0.5])
                self.assertEqual(ci[key("H")], [0.0, 0.0])
                low, high = ci[key("R3")]
                self.assertTrue(-0.1 < low < 0.0 < high < 0.1)
                pl, ph = ci[key("pooled")]
                self.assertAlmostEqual(pl, (0.5 + 0.0 + low) / 3, places=12)
                self.assertAlmostEqual(ph, (0.5 + 0.0 + high) / 3, places=12)


if __name__ == "__main__":
    unittest.main()
