"""Blunder census namespaces and summary helpers (collab LANES/census-20261110)."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(REPO / "tools" / "spy_reset"), str(REPO / "tools" / "blunder_census"), str(REPO / "python" / "tools")]
import blunder_census as census  # noqa: E402
import pilot  # noqa: E402
import blunder_summary as summarize  # noqa: E402

CENSUS = (pilot.CENSUS_PANEL_NS, pilot.CENSUS_SMOKE_NS)
KEYS = [(o, t, k) for o in range(9) for t in range(9) for k in range(12)]


class CensusNamespaces(unittest.TestCase):
    def test_admitted_and_fresh(self):
        self.assertEqual(CENSUS, ("census-20261110/a", "census-20261110/smoke"))
        self.assertEqual(pilot.PANEL_NAMESPACES[18:20], CENSUS)
        train = {row["seed"] for row in pilot.all_seeds()["train"]}
        for ns in CENSUS:
            fresh = {pilot.panel_seed(*key, ns) for key in KEYS}
            self.assertEqual(len(fresh), len(KEYS))
            self.assertFalse(fresh & train)
            for other in pilot.PANEL_NAMESPACES:
                if other != ns:
                    self.assertFalse(fresh & {pilot.panel_seed(*key, other) for key in KEYS}, (ns, other))


class OpponentsNamespace(unittest.TestCase):
    def test_admitted_and_fresh(self):
        ns = pilot.OPPONENTS_PANEL_NS
        self.assertEqual(ns, "opponents-20261111/a")
        self.assertEqual(pilot.PANEL_NAMESPACES[20], ns)
        fresh = {pilot.panel_seed(*key, ns) for key in KEYS}
        self.assertEqual(len(fresh), len(KEYS))
        self.assertFalse(fresh & {row["seed"] for row in pilot.all_seeds()["train"]})
        for other in pilot.PANEL_NAMESPACES:
            if other != ns:
                self.assertFalse(fresh & {pilot.panel_seed(*key, other) for key in KEYS}, other)


class FreshInitNamespace(unittest.TestCase):
    def test_admitted_and_fresh(self):
        ns = pilot.FRESHINIT_PANEL_NS
        self.assertEqual(ns, "freshinit-20261111/a")
        self.assertEqual(pilot.PANEL_NAMESPACES[21], ns)
        fresh = {pilot.panel_seed(*key, ns) for key in KEYS}
        self.assertEqual(len(fresh), len(KEYS))
        self.assertFalse(fresh & {row["seed"] for row in pilot.all_seeds()["train"]})
        for other in pilot.PANEL_NAMESPACES:
            if other != ns:
                self.assertFalse(fresh & {pilot.panel_seed(*key, other) for key in KEYS}, other)


class AnchorNamespace(unittest.TestCase):
    def test_admitted_and_fresh(self):
        ns = pilot.ANCHOR_PANEL_NS
        self.assertEqual(ns, "anchor-20261113/a")
        self.assertEqual(pilot.PANEL_NAMESPACES[-1], ns)
        fresh = {pilot.panel_seed(*key, ns) for key in KEYS}
        self.assertEqual(len(fresh), len(KEYS))
        self.assertFalse(fresh & {row["seed"] for row in pilot.all_seeds()["train"]})
        for other in pilot.PANEL_NAMESPACES[:-1]:
            self.assertFalse(fresh & {pilot.panel_seed(*key, other) for key in KEYS}, other)


class Summary(unittest.TestCase):
    def test_own_deck_and_wilson(self):
        self.assertEqual(summarize.own_deck("b512-vs-b512/o4-t7-k0-s1"), 4)
        self.assertEqual(summarize.DECKS[4], "Spy")
        self.assertIsNone(summarize.wilson(0, 0))
        lo, hi = summarize.wilson(5, 10)
        self.assertLess(lo, 0.5)
        self.assertGreater(hi, 0.5)
        self.assertEqual(summarize.per100(3, 100)["per_100"], 3.0)
        lo, hi = summarize.poisson(10)
        self.assertLess(lo, 10)
        self.assertGreater(hi, 10)
        self.assertEqual(summarize.poisson(0)[0], 0.0)

    def test_a_pass_that_does_not_cover_its_jobs_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            (d / "census-bcd.json").write_text(json.dumps({"expected_jobs": 3}))
            rows = [{"kind": "header"}, {"kind": "game", "id": "p/o0-t0-k0-s0"}, {"kind": "error", "id": "p/o0-t1-k0-s0"}]
            (d / "games-bcd-00.jsonl").write_text("".join(json.dumps(r) + "\n" for r in rows))
            with self.assertRaises(SystemExit):
                summarize.covered(d, "bcd")
            (d / "census-bcd.json").write_text(json.dumps({"expected_jobs": 2}))
            summarize.covered(d, "bcd")


class CensusPass(unittest.TestCase):
    def test_a_failing_shard_fails_the_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            jobs = d / "jobs.jsonl"
            jobs.write_text("".join(json.dumps({"id": f"p/o0-t{t}-k{t}-s0"}) + "\n" for t in range(3)))
            source = d / "source.json"
            source.write_text("{}")
            # The interpreter is not a census binary: every shard exits nonzero.
            with self.assertRaises(SystemExit):
                census.census(jobs, source, sys.executable, d / "out", 2, False, 2, 5000)
            self.assertTrue((d / "out" / "census-bcd.failed.json").exists())
            self.assertFalse((d / "out" / "census-bcd.json").exists())
            # The (a) pass keeps only the sample's seed families.
            with self.assertRaises(SystemExit):
                census.census(jobs, source, sys.executable, d / "win", 1, True, 2, 5000)
            record = json.loads((d / "win" / "census-win.failed.json").read_text())
            self.assertEqual(record["expected_jobs"], 2)


if __name__ == "__main__":
    unittest.main()
