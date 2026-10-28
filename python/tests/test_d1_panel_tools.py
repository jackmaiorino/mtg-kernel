"""Panel tooling for D1 (collab LANES/lr-decay-d1-20261025, DESIGN.md sections 2 and 3): its three fresh seed
namespaces (two reference namespaces and the head-to-head namespace), the refusal of an unregistered namespace, and
the Spy distillation analyzer loading a head-to-head panel key whose opponent differs per chain (each decay endpoint
against its own chain's control endpoint; the opponent's identity is checked by the lane's custody, not by load)."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tools" / "spy_reset"))
sys.path.insert(0, str(ROOT / "python" / "tools"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from tools.spy_distill import analyze  # noqa: E402
import panel  # noqa: E402
import pilot  # noqa: E402

D1 = (pilot.D1_PANEL_NS_A, pilot.D1_PANEL_NS_B, pilot.D1_H2H_NS)
KEYS = [(o, t, k) for o in range(pilot.DECKS) for t in range(pilot.DECKS) for k in range(pilot.PANEL_REPEATS)]


def source(tag, sha="2" * 64):
    return {"play_import": {"path": f"{tag}-import.json", "sha256": "1" * 64},
            "feature_transfer": {"expected_feature_contract_digest": "c", "expected_feature_encoding_digest": "e"},
            "checkpoint": {"path": f"{tag}.json", "sha256": sha}}


def write_panel(root, ck, opp, namespace, score, sha):
    """One panel's rows and labels on ``namespace``'s seeds; ``score(o, t, k, s)`` in {0, 0.5, 1}."""
    name = f"{ck}-vs-{opp}"
    rows, labels = [], []
    for o, t, k in KEYS:
        for s in (0, 1):
            v = score(o, t, k, s)
            rows.append({"panel": name, "checkpoint_sha256": sha, "own": o, "other": t, "repeat": k, "seat": s,
                         "seed": pilot.panel_seed(o, t, k, namespace), "score": v, "complete": True})
            if o == pilot.SPY:
                winner = None if v == 0.5 else (s if v == 1 else 1 - s)
                labels.append({"id": f"{name}/o{o}-t{t}-k{k}-s{s}", "kind": "label_replay", "focal": s,
                               "winner": winner, "win": winner == s, "combo_resolved": False, "combo_won": False})
    d = root / name
    d.mkdir(parents=True)
    for file, items in (("rows.jsonl", rows), ("labels.jsonl", labels)):
        (d / file).write_text("".join(json.dumps(r) + "\n" for r in items), encoding="utf-8")
    return {"checkpoint": ck, "opponent": opp, "checkpoint_sha256": sha, "rows": str(d / "rows.jsonl"),
            "labels": str(d / "labels.jsonl")}


class D1Namespaces(unittest.TestCase):
    def test_the_three_namespaces_are_admitted_and_their_seeds_are_fresh(self):
        self.assertEqual(D1, ("d1-panel-20261025a", "d1-panel-20261025b", "d1-h2h-20261025"))
        for ns in D1:
            self.assertIn(ns, pilot.PANEL_NAMESPACES)
            fresh = {pilot.panel_seed(*key, ns) for key in KEYS}
            self.assertEqual(len(fresh), len(KEYS))
            for other in pilot.PANEL_NAMESPACES:
                if other != ns:
                    self.assertFalse(fresh & {pilot.panel_seed(*key, other) for key in KEYS}, (ns, other))

    def test_earlier_namespaces_are_kept_in_order(self):
        self.assertEqual(pilot.PANEL_NAMESPACES[7:10],
                         (pilot.OH2_PANEL_NS_A, pilot.OH2_PANEL_NS_B, pilot.OH2_SCREEN_NS))
        self.assertEqual(pilot.PANEL_NAMESPACES[10:13], D1)
        self.assertEqual(len(set(pilot.PANEL_NAMESPACES)), len(pilot.PANEL_NAMESPACES))

    def test_an_unregistered_namespace_is_refused(self):
        for ns in ("d1-panel-20261025c", "d1-h2h-20261026", "d1-20261025"):
            with self.assertRaises(ValueError):
                pilot.panel_seed(0, 0, 0, ns)
        with self.assertRaises(ValueError):
            panel.cases("d1-panel-20261025c")

    def test_the_three_namespaces_build_different_schedules_for_the_same_pair(self):
        decks = pilot.load_decks()
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            plans = [panel.build("ck", source("ck"), key, source("opp"), decks, tmp / key, 16, ns)
                     for key, ns in zip(("Ha", "Hb", "H2H"), D1)]
        self.assertEqual(tuple(p["seed_namespace"] for p in plans), D1)
        self.assertEqual(len({p["schedule_sha256"] for p in plans}), 3)

    def test_head_to_head_plans_name_each_chains_own_opponent(self):
        decks = pilot.load_decks()
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            plans = [panel.build(f"r2-s{k}-d-b640", source(f"d{k}", f"{k}d".ljust(64, "0")), "H2H",
                                 panel.wrap(source(f"c{k}", f"{k}c".ljust(64, "0"))), decks, tmp / str(k), 16,
                                 pilot.D1_H2H_NS) for k in (1, 2)]
        opp = [p["opponent"]["source"]["checkpoint"]["sha256"] if "source" in p["opponent"]
               else p["opponent"]["checkpoint"]["sha256"] for p in plans]
        self.assertNotEqual(opp[0], opp[1])
        self.assertNotEqual(plans[0]["schedule_sha256"], plans[1]["schedule_sha256"])   # the opponent is scheduled


class PerChainHeadToHead(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def test_one_head_to_head_key_loads_for_every_decay_endpoint(self):
        entries = [write_panel(self.root, f"r2-s{k}-d-b640", "H2H", pilot.D1_H2H_NS,
                               lambda o, t, k_, s: 1 if o > t else (0 if o < t else 0.5), f"{k}".ljust(64, "a"))
                   for k in range(1, 9)]
        allowed = {e["checkpoint"] for e in entries}
        data = analyze.load({"panels": entries}, allowed, pilot.D1_H2H_NS, opponents=("H2H",))
        self.assertEqual(sorted(data), sorted((e["checkpoint"], "H2H") for e in entries))
        for cases in data.values():
            nine = analyze.summary(cases)["nine_deck"]
            # A score that is antisymmetric in the ordered deck pair is zero-centered over all nine focal decks.
            self.assertEqual(2 * nine - 1, 0.0)

    def test_a_head_to_head_panel_on_a_reference_namespace_is_refused(self):
        entry = write_panel(self.root, "r2-s1-d-b640", "H2H", pilot.D1_PANEL_NS_A, lambda *a: 0.5, "1" * 64)
        with self.assertRaises(ValueError):
            analyze.load({"panels": [entry]}, {"r2-s1-d-b640"}, pilot.D1_H2H_NS, opponents=("H2H",))


if __name__ == "__main__":
    unittest.main()
