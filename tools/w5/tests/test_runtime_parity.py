"""E11 runtime parity receipt tests. The end-to-end receipt runs on Spellbench's published pauper-ladder-v1 run
(SPELLBENCH_ROOT) against this repository's history (needs the board revision; CI fetches it)."""
import contextlib
import io
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
        calls = lambda src: rp.bridge_rules([src])["calls_set_runtime_rules"]  # noqa: E731
        self.assertFalse(calls("// session.set_runtime_rules_v1(x)"))
        self.assertFalse(calls("/* a block\n session.set_runtime_rules_v1(x)\n */ let y = 1;"))
        self.assertFalse(calls('let s = "set_runtime_rules_v1(";'))
        self.assertTrue(calls('let s = br#"""#; session.set_runtime_rules_v1(x); let _ = "";'))
        self.assertFalse(calls('let s = br"set_runtime_rules_v1(";'))
        self.assertTrue(calls("let _ = ('\\'','\"'); session.set_runtime_rules_v1(x); let _ = \"\";"))
        self.assertTrue(calls("let _ = (b'\\'',b'\"'); session.set_runtime_rules_v1(x); let _ = \"\";"))
        self.assertTrue(calls("let c = '\\u{1F600}'; fn f<'a>(x: &'a str) { session.set_runtime_rules_v1(x) }"))
        self.assertTrue(calls('let s = cr#"""#; session.set_runtime_rules_v1(x); let _ = "";'))
        self.assertTrue(calls("    session.set_runtime_rules_v1(rules)?;"))
        self.assertTrue(calls("session /* rules */ .set_runtime_rules_v1(rules)?;"))
        self.assertTrue(calls('/* outer /* inner */ " */ session.set_runtime_rules_v1(x); let _ = "";'))
        self.assertFalse(calls("/* outer /* inner */ session.set_runtime_rules_v1(x) */ fn f<'a>(x: &'a str) {}"))
        self.assertFalse(calls('let s = r#"set_runtime_rules_v1("#;'))

    def test_learner_must_play_first_and_second(self):
        cfg = training_config()
        for entry in cfg["iterations"][0]["episodes"]:
            entry["episode"]["learner_seat"] = entry["episode"]["starting_player"]
        self.assertFalse(run(cfg)["checks"]["starting_player_assigned_both_seats"])

    def test_registrations_must_be_complete(self):
        cfg = training_config()
        cfg["iterations"][0]["episodes"][0]["episode"]["registered"] = []
        self.assertFalse(run(cfg)["checks"]["ordinary_starts_game_one"])


def london_board():
    manifest = json.loads(json.dumps(MANIFEST))
    manifest["information_rules"]["rules"]["mulligan"] = "london"
    return manifest


def mixed_config(opening="london-v1"):
    cfg = training_config(opening=opening)
    for key in ("opening", "mulligan_modes"):
        del cfg["iterations"][0]["episodes"][0]["episode"][key]
    return cfg


def failed(result):
    return {k for k, v in result["checks"].items() if not v}


class MulliganParityTests(unittest.TestCase):
    REASON = "no London board exists yet"

    def test_no_opening_receipt_is_unchanged(self):
        """A keep-seven run adds no key and the flag changes nothing, whether or not it is given."""
        cfg = training_config()
        plain, flagged = run(cfg), rp.compare(
            rp.training_side([cfg], CARD_NAMES), MANIFEST, BOARD_CONFIG,
            rp.board_decks(RUNTIME_DECKS, BOARD_POOL, CARD_NAMES), BRIDGE_OK, self.REASON)
        self.assertEqual(json.dumps(plain, indent=2), json.dumps(flagged, indent=2))
        self.assertEqual(list(plain), ["checks", "deck_matches", "unmatched_board_decks", "training_only_decks",
                                       "recorded"])
        self.assertEqual(list(plain["checks"]), [
            "mulligans_off_on_board", "mulligans_off_in_training", "training_rules_profile_is_default",
            "board_rules_profile_is_default", "neutral_choices_equal", "ordinary_starts_game_one",
            "starting_player_assigned_both_seats", "limits_equal", "every_board_deck_equals_a_training_deck"])
        self.assertTrue(rp.passes(plain))
        profiles = rp.training_side([cfg], CARD_NAMES)["mulligan_profiles"]
        self.assertEqual(profiles, {"none": len(RUNTIME_DECKS["decks"])})

    def test_matching_opening_passes_without_a_flag(self):
        result = run(training_config(opening="london-v1"), manifest=london_board())
        self.assertEqual(failed(result), set())
        self.assertTrue(rp.passes(result))
        self.assertNotIn("parity", result)
        self.assertEqual(result["mulligan"], {"board_rule": "london", "board_rule_supported": True,
                                              "training_rule": "london", "rules_equal": True, "exception": None})

    def test_mismatch_is_refused_without_a_flag(self):
        result = run(training_config(opening="london-v1"))  # board keeps seven
        self.assertEqual(failed(result), {"mulligans_off_in_training"})
        self.assertFalse(rp.passes(result))
        self.assertIsNone(result["mulligan"]["exception"])
        self.assertNotIn("parity", result)
        # a London board against keep-seven training stays a refusal and is not covered by the flag
        result = run(training_config(), manifest=london_board(), )
        self.assertEqual(failed(result), {"mulligans_off_on_board"})
        self.assertFalse(rp.passes(result))

    def test_flag_with_reason_records_the_exception(self):
        cfg = training_config(opening="london-v1")
        result = rp.compare(rp.training_side([cfg], CARD_NAMES), MANIFEST, BOARD_CONFIG,
                            rp.board_decks(RUNTIME_DECKS, BOARD_POOL, CARD_NAMES), BRIDGE_OK, f"  {self.REASON}  ")
        self.assertEqual(failed(result), {"mulligans_off_in_training"})  # the check itself stays honest
        self.assertTrue(rp.passes(result))
        exception = result["mulligan"]["exception"]
        self.assertEqual(exception, {"flag": "--allow-london-training", "reason": self.REASON,
                                     "excepted_checks": ["mulligans_off_in_training"],
                                     "training_openings": ["london-v1"], "board_rule": "none"})
        self.assertIn("holds only with a recorded exception", result["parity"])
        self.assertIn(self.REASON, result["parity"])

    def test_empty_reason_is_refused(self):
        cfg = training_config(opening="london-v1")
        for reason in ("", "   ", "\n"):
            with self.assertRaises(rp.ParityError):
                rp.compare(rp.training_side([cfg], CARD_NAMES), MANIFEST, BOARD_CONFIG,
                           rp.board_decks(RUNTIME_DECKS, BOARD_POOL, CARD_NAMES), BRIDGE_OK, reason)
            # main refuses before it reads any input, even on a keep-seven run
            err = io.StringIO()
            with contextlib.redirect_stderr(err):
                code = rp.main(["--training-config", "t.json", "--board-run", "b", "--repo", "r",
                                "--allow-london-training", reason])
            self.assertEqual(code, 1)
            self.assertIn("non-empty reason", err.getvalue())

    def test_mixed_set_is_refused_unless_flagged(self):
        cfg = mixed_config()
        profiles = rp.training_side([cfg], CARD_NAMES)["mulligan_profiles"]
        self.assertEqual(profiles, {"none": 1, "opening=london-v1": len(RUNTIME_DECKS["decks"]) - 1})
        for manifest in (MANIFEST, london_board()):
            result = run(cfg, manifest=manifest)
            self.assertIn("mulligans_off_in_training", failed(result))
            self.assertFalse(rp.passes(result))
            self.assertEqual(result["mulligan"]["training_rule"], "mixed")
            self.assertIsNone(result["mulligan"]["exception"])
            flagged = rp.compare(rp.training_side([cfg], CARD_NAMES), manifest, BOARD_CONFIG,
                                 rp.board_decks(RUNTIME_DECKS, BOARD_POOL, CARD_NAMES), BRIDGE_OK, self.REASON)
            self.assertTrue(rp.passes(flagged))
            self.assertEqual(flagged["mulligan"]["exception"]["training_openings"], ["london-v1", "none"])
            self.assertEqual(flagged["mulligan"]["exception"]["board_rule"],
                             manifest["information_rules"]["rules"]["mulligan"])

    def test_flag_does_not_cover_other_openings_or_other_failures(self):
        flagged = lambda cfg, **kw: rp.compare(  # noqa: E731
            rp.training_side([cfg], CARD_NAMES), kw.get("manifest", MANIFEST), BOARD_CONFIG,
            rp.board_decks(RUNTIME_DECKS, BOARD_POOL, CARD_NAMES), kw.get("bridge", BRIDGE_OK), self.REASON)
        result = flagged(training_config(opening="shroud-v9"))
        self.assertFalse(rp.passes(result))
        self.assertIsNone(result["mulligan"]["exception"])
        self.assertEqual(result["mulligan"]["training_rule"], "unknown")
        result = flagged(training_config(opening="london-v1"), bridge={"calls_set_runtime_rules": True})
        self.assertFalse(rp.passes(result))  # the exception covers the mulligan check only
        self.assertNotIn("parity", result)
        unsupported = json.loads(json.dumps(MANIFEST))
        unsupported["engine_profile"]["rules_supported"]["mulligan"] = ["none"]
        unsupported["information_rules"]["rules"]["mulligan"] = "london"
        self.assertFalse(rp.passes(flagged(training_config(opening="london-v1"), manifest=unsupported)))

    def test_an_opening_nobody_mulligans_is_not_the_boards_london_rule(self):
        # The profile once keyed on the protocol name alone: london-v1 with both seats keep_seven passed a London
        # board. The learner seat's mode must be learned (the trainer's own rule), and the mode mix is recorded.
        def with_modes(modes, seat=0):
            cfg = training_config(opening="london-v1")
            for entry in cfg["iterations"][0]["episodes"]:
                entry["episode"]["mulligan_modes"], entry["episode"]["learner_seat"] = modes, seat
            return cfg
        n = len(RUNTIME_DECKS["decks"])
        for modes, seat in ((["keep_seven", "keep_seven"], 0), (["keep_seven", "learned"], 0),
                            (["learned", "keep_seven"], 1), (["learned"], 0), ("learned", 0), (None, 0)):
            cfg = with_modes(modes, seat)
            profile = rp.training_side([cfg], CARD_NAMES)["mulligan_profiles"]
            self.assertEqual(profile, {"opening=london-v1;learner-mode-not-learned": n}, (modes, seat))
            result = run(cfg, manifest=london_board())
            self.assertIn("mulligans_off_in_training", failed(result), (modes, seat))
            self.assertFalse(rp.passes(result))
            self.assertEqual(result["mulligan"]["training_rule"], "unknown")
            flagged = rp.compare(rp.training_side([cfg], CARD_NAMES), london_board(), BOARD_CONFIG,
                                 rp.board_decks(RUNTIME_DECKS, BOARD_POOL, CARD_NAMES), BRIDGE_OK, self.REASON)
            self.assertFalse(rp.passes(flagged))  # the flag does not cover it
            self.assertIsNone(flagged["mulligan"]["exception"])
        for modes, seat in ((["learned", "learned"], 0), (["learned", "keep_seven"], 0),
                            (["keep_seven", "learned"], 1)):
            side = rp.training_side([with_modes(modes, seat)], CARD_NAMES)
            self.assertEqual(side["mulligan_profiles"], {"opening=london-v1": n})
            self.assertEqual(side["mulligan_modes"], {"/".join(modes): n})  # the mix is in the receipt
            self.assertEqual(failed(run(with_modes(modes, seat), manifest=london_board())), set())
        self.assertNotIn("mulligan_modes", rp.training_side([training_config()], CARD_NAMES))


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
