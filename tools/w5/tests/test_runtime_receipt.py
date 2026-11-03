"""A1 runtime receipt: exactly the three named exceptions, each under its conditions, kept apart from raw
failures; the recorded fields. The end-to-end receipt runs on Spellbench's published pauper-ladder-v1 run and
snapshot when SPELLBENCH_ROOT is set."""
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
REPO = HERE.parents[1]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE / "tests"))
import runtime_parity as rp  # noqa: E402
import runtime_receipt as rr  # noqa: E402
from test_runtime_parity import (BOARD_CONFIG, BOARD_POOL, BRIDGE_OK, CARD_NAMES, MANIFEST,  # noqa: E402
                                 RUNTIME_DECKS, has_revision, training_config)

CARDS = json.loads((REPO / "data" / "cards_v1.json").read_text(encoding="utf-8"))["cards"]
NEUTRAL = json.loads(json.dumps(MANIFEST))
NEUTRAL["engine"]["rules_snapshot_id"] += ":neutral-v2"
WRAPPER = {"buffered_completion_proposals": True, "completion_mapping": rr.MAPPING}
NO_DELVE = {"training": [], "board": []}


def raw_for(cfg, manifest=NEUTRAL, board_config=BOARD_CONFIG):
    return rp.compare(rp.training_side([cfg], CARD_NAMES), manifest, board_config,
                      rp.board_decks(RUNTIME_DECKS, BOARD_POOL, CARD_NAMES), BRIDGE_OK)


def board_runtime(cfg=None, manifest=NEUTRAL, wrapper=WRAPPER, delve=NO_DELVE, census_ok=True, h="resolution-boundary-v1",
                  s0="resolution-boundary-v1", board_config=BOARD_CONFIG):
    cfg = cfg or training_config(runtime_rules="resolution-boundary-v1")
    profile = rr.runtime_profile([cfg])
    s0_cfg = training_config(runtime_rules=s0) if s0 else training_config()
    return rr.evaluate(raw_for(cfg, manifest, board_config), profile, [{"runtime_rules": h}],
                       rr.runtime_profile([s0_cfg]), manifest, wrapper, delve, census_ok)


class ExceptionTests(unittest.TestCase):
    def test_the_three_named_differences_are_accepted_and_kept_apart(self):
        out = board_runtime()
        self.assertEqual(out["raw_failures"], ["neutral_choices_equal", "selection_equal",
                                               "training_rules_profile_is_default"])
        self.assertEqual(sorted(e["exception"] for e in out["accepted_exceptions"]), sorted(rr.EXCEPTIONS))
        self.assertEqual(out["unexcepted_failures"], [])
        self.assertEqual(out["refused_exceptions"], [])
        self.assertFalse(out["raw_checks"]["neutral_choices_equal"])  # the raw result is preserved

    def test_conditions_refuse_their_exception(self):
        cases = {
            "spellbench-neutral-choices": board_runtime(census_ok=False),
            "graveyard-order-profile": board_runtime(delve={"training": ["Gurmag Angler"], "board": []}),
            "buffered-completion-selection": board_runtime(wrapper={"buffered_completion_proposals": True,
                                                                    "completion_mapping": None}),
        }
        for name, out in cases.items():
            self.assertEqual([e["exception"] for e in out["refused_exceptions"]], [name], name)
        other = json.loads(json.dumps(NEUTRAL))
        other["engine"]["rules_snapshot_id"] = other["engine"]["rules_snapshot_id"].replace("neutral-v2", "neutral-v3")
        self.assertEqual([e["exception"] for e in board_runtime(manifest=other)["refused_exceptions"]],
                         ["spellbench-neutral-choices"])

    def test_any_other_difference_stays_a_failure(self):
        out = board_runtime(board_config={"limits": {"max_decisions": 1, "max_steps": 1}})
        self.assertEqual(out["unexcepted_failures"], ["limits_equal"])
        self.assertIn("h_panel_runtime_equals_training", board_runtime(h=None)["unexcepted_failures"])
        self.assertIn("s0_runtime_equals_training", board_runtime(s0=None)["unexcepted_failures"])
        mixed = training_config(runtime_rules="resolution-boundary-v1")
        mixed["iterations"][0]["episodes"][0]["episode"].pop("runtime_rules")
        out = board_runtime(cfg=mixed, h=None)
        self.assertEqual([e["exception"] for e in out["refused_exceptions"]], ["graveyard-order-profile"])

    def test_no_exception_when_nothing_differs(self):
        out = board_runtime(cfg=training_config(), manifest=MANIFEST, h=None, s0=None,
                            wrapper={"buffered_completion_proposals": False, "completion_mapping": None})
        self.assertEqual(out["raw_failures"], [])
        self.assertEqual(out["accepted_exceptions"], [])

    def test_delve_and_wrapper_detection(self):
        angler = next(i for i, c in enumerate(CARDS) if c["name"] == "Gurmag Angler")
        self.assertEqual(rr.delve_cards(CARDS, [0, angler]), ["Gurmag Angler"])
        self.assertEqual(rr.wrapper_selection(f'X = "{rr.PROPOSALS}"; M = "{rr.MAPPING}"'), WRAPPER)
        self.assertFalse(rr.wrapper_selection("plain")["buffered_completion_proposals"])


@unittest.skipUnless(os.environ.get("SPELLBENCH_ROOT"), "SPELLBENCH_ROOT not set")
class EndToEndTests(unittest.TestCase):
    def test_published_v1_board(self):
        sb = Path(os.environ["SPELLBENCH_ROOT"])
        run = sb / "benchmarks" / "pauper-ladder-v1" / "runs" / "2026-11-01"
        snap = sb / "benchmarks" / "pauper-ladder-v1" / "snapshots" / "2026-11-01"
        revision = json.loads((run / "manifest.json").read_text(encoding="utf-8"))["engine"]["source_revision"]
        if not has_revision(revision):
            self.skipTest(f"board revision {revision} not in this clone")
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            files = {}
            for name, value in (("cfg.json", training_config(runtime_rules="resolution-boundary-v1")),
                                ("s0.json", training_config(runtime_rules="resolution-boundary-v1")),
                                ("h.json", {"runtime_rules": "resolution-boundary-v1", "seed_namespace": "x"})):
                files[name] = tmp / name
                files[name].write_text(json.dumps(value))
            amendment = tmp / "RULINGS-r0.md"
            amendment.write_text("## Amendment A1: runtime parity\n\n### A1 correction\n")
            census = tmp / "census.json"
            census.write_text("{}")
            out = tmp / "receipt.json"
            code = rr.main(["--training-config", str(files["cfg.json"]), "--board-run", str(run),
                            "--board-snapshot", str(snap), "--repo", str(REPO), "--amendment", str(amendment),
                            "--amendment-commit", "21d5ded1", "--census", str(census),
                            "--h-plan", str(files["h.json"]), "--s0-config", str(files["s0.json"]),
                            "--engine-binary", sys.executable,
                            "--wrapper", str(sb / "integrations" / "mtg_kernel" / "kernel_engine_v2.py"),
                            "--scorer", sys.executable, "--out", str(out)])
            receipt = json.loads(out.read_text())
        self.assertEqual(code, 1)  # stand-in census and binaries: bindings and the neutral exception refuse
        self.assertEqual(receipt["raw_failures"], ["neutral_choices_equal", "selection_equal",
                                                   "training_rules_profile_is_default"])
        self.assertEqual(sorted(e["exception"] for e in receipt["accepted_exceptions"]),
                         ["buffered-completion-selection", "graveyard-order-profile"])
        self.assertEqual([e["exception"] for e in receipt["refused_exceptions"]], ["spellbench-neutral-choices"])
        self.assertTrue(receipt["bindings"]["run_is_a_snapshot_source"])
        self.assertFalse(receipt["bindings"]["engine_binary_in_board_engine_files"])
        self.assertEqual(receipt["exception_evidence"]["delve_cards"], {"training": [], "board": []})
        for key in ("amendment", "runtimes", "configs", "identities", "board_snapshot", "limitation"):
            self.assertIn(key, receipt)


if __name__ == "__main__":
    unittest.main()
