"""E11 runtime parity receipt tests. The end-to-end receipt runs on Spellbench's published pauper-ladder-v1 run
(SPELLBENCH_ROOT) against this repository's history (needs the board revision; CI fetches full history)."""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
REPO = HERE.parents[1]
sys.path.insert(0, str(HERE))
import runtime_parity as rp  # noqa: E402

SPELLBENCH = os.environ.get("SPELLBENCH_ROOT")
RUNTIME_DECKS = json.loads((REPO / "data" / "runtime_decks_v1.json").read_text(encoding="utf-8"))
CARD_NAMES = [c["name"] for c in json.loads((REPO / "data" / "cards_v1.json").read_text(encoding="utf-8"))["cards"]]


def deck(deck_id):
    entry = next(d for d in RUNTIME_DECKS["decks"] if d["id"] == deck_id)
    return {"label": deck_id, "mainboard": [row["card_id"] for row in entry["materialized_mainboard"]],
            "sideboard": []}


def training_config(opening=None, runtime_rules=None, tamper=None):
    ids = [d["id"] for d in RUNTIME_DECKS["decks"]]
    eps = []
    for i, a in enumerate(ids):
        b = ids[(i + 1) % len(ids)]
        ep = {"id": f"e{i}", "seed": i, "starting_player": i % 2, "learner_seat": 0, "postboard": False,
              "max_physical_decisions": 4000, "max_policy_steps": 40000,
              "registered": [deck(a), deck(b)], "selected": [deck(a), deck(b)]}
        if opening:
            ep["opening"], ep["mulligan_modes"] = opening, ["learned", "keep_seven"]
        if runtime_rules:
            ep["runtime_rules"] = runtime_rules
        eps.append({"episode": ep})
    if tamper:
        eps[0]["episode"]["selected"][0]["mainboard"][0] = tamper
        eps[-1]["episode"]["selected"][1]["mainboard"][0] = tamper
    return {"iterations": [{"episodes": eps}]}


BOARD_POOL = ["Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "CawGates", "Faeries"]
MANIFEST = {"information_rules": {"rules": {"mulligan": "none", "starting_player": "host_assigned",
                                            "extensions": ["x_kernel_flat_v4"]}, "engine_defaults": {}},
            "engine_profile": {"rules_supported": {"mulligan": ["none"], "starting_player": ["host_assigned"]}},
            "engine": {"source_revision": "x"}}
BRIDGE_OK = {"applies_runtime_rules_profile": False, "refuses_opt_in_semantics": True}


class CompareTests(unittest.TestCase):
    def board(self):
        return rp.board_decks(RUNTIME_DECKS, BOARD_POOL)

    def test_parity_holds_and_terror_is_training_only(self):
        result = rp.compare(rp.training_side([training_config()], CARD_NAMES), MANIFEST, {}, self.board(), BRIDGE_OK)
        self.assertTrue(all(result["checks"].values()), result["checks"])
        self.assertEqual(result["training_only_decks"], ["Terror"])
        self.assertEqual(len(result["deck_matches"]), 8)

    def test_london_or_rules_profile_breaks_parity(self):
        london = rp.compare(rp.training_side([training_config(opening="london-v1")], CARD_NAMES), MANIFEST, {},
                            self.board(), BRIDGE_OK)
        self.assertFalse(london["checks"]["mulligans_off_in_training"])
        rules = rp.compare(rp.training_side([training_config(runtime_rules="resolution_boundary_v1")], CARD_NAMES),
                           MANIFEST, {}, self.board(), BRIDGE_OK)
        self.assertFalse(rules["checks"]["default_rules_in_training"])
        board_london = json.loads(json.dumps(MANIFEST))
        board_london["information_rules"]["rules"]["mulligan"] = "london"
        out = rp.compare(rp.training_side([training_config()], CARD_NAMES), board_london, {}, self.board(), BRIDGE_OK)
        self.assertFalse(out["checks"]["mulligans_off_on_board"])

    def test_a_changed_card_breaks_deck_parity(self):
        other = CARD_NAMES.index(next(n for n in CARD_NAMES if n != CARD_NAMES[deck("Wildfire")["mainboard"][0]]))
        cfg = training_config(tamper=other)
        result = rp.compare(rp.training_side([cfg], CARD_NAMES), MANIFEST, {}, self.board(), BRIDGE_OK)
        self.assertFalse(result["checks"]["every_board_deck_equals_one_training_deck"])
        self.assertEqual(result["unmatched_board_decks"][0]["deck"], "Wildfire")

    def test_inconsistent_training_deck_is_refused(self):
        cfg = training_config()
        cfg["iterations"][0]["episodes"][0]["episode"]["selected"][0]["mainboard"][0] = 0
        with self.assertRaises(rp.ParityError):
            rp.training_side([cfg], CARD_NAMES)

    def test_bridge_facts(self):
        self.assertEqual(rp.bridge_rules("x"), {"applies_runtime_rules_profile": False,
                                                 "refuses_opt_in_semantics": False})


def has_revision(revision):
    return subprocess.run(["git", "-C", str(REPO), "cat-file", "-e", f"{revision}^{{commit}}"],
                          capture_output=True).returncode == 0


@unittest.skipUnless(SPELLBENCH, "SPELLBENCH_ROOT not set")
class EndToEndTests(unittest.TestCase):
    def test_published_v1_run(self):
        run = Path(SPELLBENCH) / "benchmarks" / "pauper-ladder-v1" / "runs" / "2026-11-01"
        revision = json.loads((run / "manifest.json").read_text(encoding="utf-8"))["engine"]["source_revision"]
        if not has_revision(revision):
            self.skipTest(f"board revision {revision} not in this clone")
        with tempfile.TemporaryDirectory() as tmp:
            cfg, out = Path(tmp) / "cfg.json", Path(tmp) / "receipt.json"
            cfg.write_text(json.dumps(training_config()))
            code = rp.main(["--training-config", str(cfg), "--board-run", str(run), "--repo", str(REPO),
                            "--out", str(out)])
            receipt = json.loads(out.read_text())
        self.assertEqual(code, 0, receipt["checks"])
        self.assertEqual(receipt["training_only_decks"], ["Terror"])
        dest = os.environ.get("W5_PARITY_RECEIPT")
        if dest:
            Path(dest).write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    unittest.main()
