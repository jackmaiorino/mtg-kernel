"""E11 runtime parity receipt tests. The end-to-end receipt runs on Spellbench's published pauper-ladder-v1 run
(SPELLBENCH_ROOT) against this repository's history (needs the board revision; CI fetches it)."""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
REPO = HERE.parents[1]
sys.path.insert(0, str(HERE))
import runtime_parity as rp  # noqa: E402

SPELLBENCH = os.environ.get("SPELLBENCH_ROOT")
RUNTIME_DECKS = json.loads((REPO / "data" / "runtime_decks_v1.json").read_text(encoding="utf-8"))
CARD_NAMES = [c["name"] for c in json.loads((REPO / "data" / "cards_v1.json").read_text(encoding="utf-8"))["cards"]]
LIMITS = {"max_decisions": 10000, "max_steps": 100000}


def deck(deck_id):
    entry = next(d for d in RUNTIME_DECKS["decks"] if d["id"] == deck_id)
    return {"label": deck_id, "mainboard": [row["card_id"] for row in entry["materialized_mainboard"]],
            "sideboard": []}


def training_config(opening=None, runtime_rules=None, tamper=None, starts=(0, 1), reset=False):
    ids = [d["id"] for d in RUNTIME_DECKS["decks"]]
    eps = []
    for i, a in enumerate(ids):
        b = ids[(i + 1) % len(ids)]
        ep = {"id": f"e{i}", "seed": i, "starting_player": starts[i % len(starts)], "learner_seat": 0,
              "postboard": False, "max_physical_decisions": LIMITS["max_decisions"],
              "max_policy_steps": LIMITS["max_steps"], "registered": [deck(a), deck(b)],
              "selected": [deck(a), deck(b)]}
        if opening:
            ep["opening"], ep["mulligan_modes"] = opening, ["learned", "keep_seven"]
        if runtime_rules:
            ep["runtime_rules"] = runtime_rules
        if reset and i == 0:
            ep["reset"] = {"prefix": []}
        eps.append({"episode": ep})
    if tamper is not None:
        for ep, seat in ((eps[0]["episode"], 0), (eps[-1]["episode"], 1)):
            ep["selected"][seat]["mainboard"][0] = tamper
            ep["registered"][seat]["mainboard"][0] = tamper
    return {"iterations": [{"episodes": eps}]}


BOARD_POOL = ["Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "CawGates", "Faeries"]
MANIFEST = {"information_rules": {"rules": {"mulligan": "none", "starting_player": "host_assigned",
                                            "extensions": ["x_kernel_flat_v4"]}, "engine_defaults": {}},
            "engine_profile": {"rules_supported": {"mulligan": ["none", "london"],
                                                   "starting_player": ["host_assigned"]}},
            "engine": {"source_revision": "x", "rules_snapshot_id": "mtg-kernel-rules/x/carddb-y"}}
BOARD_CONFIG = {"limits": LIMITS}
BRIDGE_OK = {"calls_set_runtime_rules": False}


def run(cfg, manifest=MANIFEST, board_config=BOARD_CONFIG, bridge=BRIDGE_OK):
    return rp.compare(rp.training_side([cfg], CARD_NAMES), manifest, board_config,
                      rp.board_decks(RUNTIME_DECKS, BOARD_POOL, CARD_NAMES), bridge)


class CompareTests(unittest.TestCase):
    def test_parity_holds_and_terror_is_training_only(self):
        result = run(training_config())
        self.assertTrue(all(result["checks"].values()), result["checks"])
        self.assertEqual(result["training_only_decks"], ["Terror"])
        self.assertEqual(len(result["deck_matches"]), 8)

    def test_each_difference_fails_its_check(self):
        cases = {
            "mulligans_off_in_training": run(training_config(opening="london-v1")),
            "training_rules_profile_is_default": run(training_config(runtime_rules="resolution-boundary-v1")),
            "board_rules_profile_is_default": run(training_config(), bridge={"calls_set_runtime_rules": True}),
            "starting_player_assigned_both_seats": run(training_config(starts=(1,))),
            "ordinary_starts_game_one": run(training_config(reset=True)),
            "limits_equal": run(training_config(), board_config={"limits": {"max_decisions": 1, "max_steps": 1}}),
        }
        neutral = json.loads(json.dumps(MANIFEST))
        neutral["engine"]["rules_snapshot_id"] += ":neutral-v2"
        cases["neutral_choices_equal"] = run(training_config(), manifest=neutral)
        london = json.loads(json.dumps(MANIFEST))
        london["information_rules"]["rules"]["mulligan"] = "london"
        cases["mulligans_off_on_board"] = run(training_config(), manifest=london)
        for check, result in cases.items():
            failed = {k for k, v in result["checks"].items() if not v}
            self.assertEqual(failed, {check}, check)

    def test_a_changed_card_breaks_deck_parity(self):
        first = deck("Wildfire")["mainboard"][0]
        other = next(i for i, n in enumerate(CARD_NAMES) if n != CARD_NAMES[first])
        result = run(training_config(tamper=other))
        self.assertFalse(result["checks"]["every_board_deck_equals_a_training_deck"])
        self.assertEqual(result["unmatched_board_decks"], ["Wildfire"])
        if CARD_NAMES[first] != CARD_NAMES[0]:  # a falsy replacement id still applies
            self.assertEqual(run(training_config(tamper=0))["unmatched_board_decks"], ["Wildfire"])

    def test_same_name_different_id_breaks_deck_parity(self):
        renamed = list(CARD_NAMES)
        first = deck("Wildfire")["mainboard"][0]
        renamed.append(CARD_NAMES[first])  # the same name under a new native id
        cfg = training_config(tamper=len(renamed) - 1)
        result = rp.compare(rp.training_side([cfg], renamed), MANIFEST, BOARD_CONFIG,
                            rp.board_decks(RUNTIME_DECKS, BOARD_POOL, CARD_NAMES), BRIDGE_OK)
        self.assertEqual(result["unmatched_board_decks"], ["Wildfire"])

    def test_aliases_are_reported_not_failed(self):
        cfg = training_config()
        alias = json.loads(json.dumps(cfg["iterations"][0]["episodes"][0]))
        for side in ("selected", "registered"):
            alias["episode"][side][0]["label"] = "Wildfire-alias"
        cfg["iterations"][0]["episodes"].append(alias)
        result = run(cfg)
        self.assertTrue(result["checks"]["every_board_deck_equals_a_training_deck"])
        self.assertEqual(result["deck_matches"]["Wildfire"], ["Wildfire", "Wildfire-alias"])

    def test_inconsistent_training_deck_is_refused(self):
        cfg = training_config()
        cfg["iterations"][0]["episodes"][0]["episode"]["selected"][0]["mainboard"][0] = 0
        with self.assertRaises(rp.ParityError):
            rp.training_side([cfg], CARD_NAMES)

    def test_bridge_call_detection_ignores_comments(self):
        self.assertFalse(rp.bridge_rules(["// session.set_runtime_rules_v1(x)"])["calls_set_runtime_rules"])
        self.assertTrue(rp.bridge_rules(["    session.set_runtime_rules_v1(rules)?;"])["calls_set_runtime_rules"])


def has_revision(revision):
    return subprocess.run(["git", "-C", str(REPO), "cat-file", "-e", f"{revision}^{{commit}}"],
                          capture_output=True).returncode == 0


@unittest.skipUnless(SPELLBENCH, "SPELLBENCH_ROOT not set")
class EndToEndTests(unittest.TestCase):
    def test_published_v1_run(self):
        """On the published v1 board, decks, mulligans, rules profile and limits match a default-rules nine-deck
        config, and the board's neutral choices (`:neutral-v2`) are the one runtime difference."""
        run_dir = Path(SPELLBENCH) / "benchmarks" / "pauper-ladder-v1" / "runs" / "2026-11-01"
        revision = json.loads((run_dir / "manifest.json").read_text(encoding="utf-8"))["engine"]["source_revision"]
        if not has_revision(revision):
            self.skipTest(f"board revision {revision} not in this clone")
        with tempfile.TemporaryDirectory() as tmp:
            cfg, out = Path(tmp) / "cfg.json", Path(tmp) / "receipt.json"
            cfg.write_text(json.dumps(training_config()))
            code = rp.main(["--training-config", str(cfg), "--board-run", str(run_dir), "--repo", str(REPO),
                            "--out", str(out)])
            receipt = json.loads(out.read_text())
        failed = {k for k, v in receipt["checks"].items() if not v}
        self.assertEqual(failed, {"neutral_choices_equal"}, receipt["checks"])
        self.assertEqual(code, 1)
        self.assertEqual(receipt["training_only_decks"], ["Terror"])
        dest = os.environ.get("W5_PARITY_RECEIPT")
        if dest:
            Path(dest).write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    unittest.main()
