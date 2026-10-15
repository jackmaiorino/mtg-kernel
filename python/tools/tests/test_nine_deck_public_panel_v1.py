"""Public panel plan, rows and receipt checks without running the evaluator."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

import nine_deck_baseline_v1 as ndb
import nine_deck_public_panel_v1 as panel

REPO = Path(__file__).resolve().parents[3]
DECKS = REPO / "data" / "runtime_decks_v1.json"


class PublicPanelTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.decks = ndb.load_decks(DECKS)
        self.candidate = {"checkpoint": {"path": "C:/c.json", "sha256": "c" * 64}, "play_import": {"sha256": "i" * 64}}
        self.opponent = {"kind": "legacy", "source": {"checkpoint": None}, "v3_forced_actions": True,
                         "v3_spell_target_reference_adapter": True}

    def test_build_splits_seats_and_orders_sources(self):
        plan = panel.build("p2", self.candidate, self.opponent, self.decks, self.root / "p2", 4)
        self.assertEqual(plan["cases"], 1944)
        self.assertEqual(len(plan["requests"]), 8)
        self.assertNotIn("game_one_only", plan)
        self.assertEqual(sum(len(item["cases"]) for item in plan["requests"]), 1944)
        for item in plan["requests"]:
            request = json.loads(Path(item["request"]["path"]).read_text())
            self.assertNotIn("game_one_only", request)
            seat = item["seat"]
            self.assertEqual(request["sources"][seat]["source"], self.candidate)
            self.assertEqual(request["sources"][1 - seat], self.opponent)
            for case, match in zip(item["cases"], request["matches"]):
                self.assertEqual(case["seat"], seat)
                self.assertEqual(match["config"]["deck_ids"][seat].split("/")[0], ndb.DECK_NAMES[case["own"]])
                self.assertEqual(match["config"]["game_one_chooser"], case["starting_player"])
                self.assertEqual(match["config"]["max_physical_decisions"], panel.MAX_DECISIONS)

    def test_game_one_mode_is_part_of_requests_schedule_and_qualification(self):
        legacy = panel.build("p2", self.candidate, self.opponent, self.decks, self.root / "old", 2, own_decks=(4,))
        plan = panel.build("p2", self.candidate, self.opponent, self.decks, self.root / "one", 2,
                           own_decks=(4,), game_one_only=True)
        self.assertNotEqual(legacy["schedule_sha256"], plan["schedule_sha256"])
        for item in plan["requests"]:
            self.assertIs(panel.request_for_plan(plan, item)["game_one_only"], True)
        executable = self.root / "evaluator.exe"
        executable.write_bytes(b"binary")
        def hashes(request_path):
            request = json.loads(Path(request_path).read_text())
            self.assertIs(request["game_one_only"], True)
            return [str(m["config"]["seed"]) for m in request["matches"]]
        with mock.patch.object(panel, "run_requests", return_value=1.), \
                mock.patch.object(panel, "match_hashes", side_effect=hashes), \
                mock.patch.object(panel, "candidate_layout", return_value="layout"):
            receipt = panel.qualify(self.root / "one/plan.json", 2, [1, 2], self.root / "qual", str(executable))
            self.assertIs(receipt["game_one_only"], True)
            receipt.pop("game_one_only")
            panel.write_json(self.root / "bad-receipt.json", receipt)
            with self.assertRaisesRegex(SystemExit, "qualification receipt"):
                panel.run(self.root / "one/plan.json", self.root / "bad-receipt.json", str(executable))

    def test_game_one_rows_preserve_draws_and_refuse_match_outcomes_or_extra_games(self):
        plan = panel.build("p2", self.candidate, self.opponent, self.decks, self.root / "one", 2,
                           own_decks=(4,), game_one_only=True)
        for item in plan["requests"]:
            request = panel.request_for_plan(plan, item)
            out = Path(request["output_directory"])
            out.mkdir(parents=True)
            panel.write_json(out / "start.json", {"schema": "public-input-evaluation-start/v1", "command": request})
            for index, case in enumerate(item["cases"]):
                game = {"start": {"starting_player": case["starting_player"]}, "winner": None}
                (out / f"match-{index:06d}.json").write_text(json.dumps({
                    "schema": "public-input-evaluation-game-one/v1", "game_one_only": True,
                    "match": {"config": {"seed": case["seed"]}}, "games": [game], "sideboard_decisions": []}))
        result = panel.rows(self.root / "one/plan.json")
        self.assertEqual(len(result), 216)
        self.assertTrue(all(r["complete"] and r["score"] == .5 for r in result))
        first_request = panel.request_for_plan(plan, plan["requests"][0])
        path = Path(first_request["output_directory"]) / "match-000000.json"
        original = json.loads(path.read_text())
        for change in ({"outcome": {"winner": 0}}, {"games": original["games"] * 2}):
            path.write_text(json.dumps(dict(original, **change)))
            with self.assertRaisesRegex(ValueError, "game-one result"):
                panel.rows(self.root / "one/plan.json")

    def test_request_mode_must_match_the_plan(self):
        plan = panel.build("p2", self.candidate, self.opponent, self.decks, self.root / "one", 1,
                           own_decks=(4,), game_one_only=True)
        item = plan["requests"][0]
        request = panel.request_for_plan(plan, item)
        request.pop("game_one_only")
        item["request"] = panel.write_json(Path(item["request"]["path"]), request)
        with self.assertRaisesRegex(ValueError, "evaluation mode"):
            panel.request_for_plan(plan, item)

    def test_completed_bo3_output_cannot_be_reused_for_game_one_request(self):
        output = self.root / "completed"
        output.mkdir()
        panel.write_json(output / "completion.json", {"match_sha256": []})
        request = {"output_directory": str(output), "game_one_only": True, "matches": []}
        path = self.root / "request.json"
        panel.write_json(path, request)
        with self.assertRaisesRegex(ValueError, "completion evaluation mode"):
            panel.match_hashes(path)
        panel.write_json(output / "start.json", {"schema": "public-input-evaluation-start/v1", "command": request})
        with self.assertRaisesRegex(ValueError, "completion evaluation mode"):
            panel.recover("unused", path)

    def test_failed_bo3_directory_and_stale_recovery_pins_are_rejected(self):
        output = self.root / "failed"
        output.mkdir()
        request = {"output_directory": str(output), "game_one_only": True,
                   "matches": [{"config": {"seed": 42}}]}
        path = self.root / "request.json"
        panel.write_json(path, request)
        old = dict(request)
        old.pop("game_one_only")
        panel.write_json(output / "start.json", {"schema": "public-input-evaluation-start/v1", "command": old})
        panel.write_json(output / "failure.json", {"error": "A versus B, match seed 42, game 2, environment seed 123: InvalidReference"})
        with self.assertRaisesRegex(ValueError, "native start command"):
            panel.recover("unused", path)
        panel.write_json(output / "start.json", {"schema": "public-input-evaluation-start/v1", "command": request})
        with self.assertRaisesRegex(ValueError, "physical game 1"):
            panel.recover("unused", path)
        panel.write_json(output / "failure.json", {
            "error": "A versus B, match seed 42, game 1, environment seed 123: InvalidReference"})
        valid = panel.recover("unused", path)
        self.assertEqual(json.loads(Path(valid["path"]).read_text())["errors"][0]["seed"], 42)
        ledger = {"request": panel.pin(path), "segments": [
            {"request": panel.pin(path), "output_directory": str(output), "start": 0, "count": 0}], "errors": []}
        panel.write_json(path.with_suffix(".recovery.json"), ledger)
        self.assertEqual(panel.recovery_for_request(path), ledger)
        ledger["segments"][0]["request"]["sha256"] = "0" * 64
        panel.write_json(path.with_suffix(".recovery.json"), ledger)
        with self.assertRaisesRegex(ValueError, "pinned file changed"):
            panel.recovery_for_request(path)
        ledger["segments"][0]["request"] = panel.pin(path)
        ledger["errors"] = [{"seed": 42, "error": "A versus B, match seed 42, game 2, environment seed 123: InvalidReference"}]
        panel.write_json(path.with_suffix(".recovery.json"), ledger)
        with self.assertRaisesRegex(ValueError, "physical game 1"):
            panel.recovery_for_request(path)
        ledger["errors"] = []
        ledger["request"]["sha256"] = "0" * 64
        panel.write_json(path.with_suffix(".recovery.json"), ledger)
        with self.assertRaisesRegex(ValueError, "recovery request pin"):
            panel.recovery_for_request(path)

    def test_rows_read_game_one_for_the_candidate_seat(self):
        plan = panel.build("p2", self.candidate, self.opponent, self.decks, self.root / "p2", 2,
                           own_decks=(4,))
        for item in plan["requests"]:
            request = json.loads(Path(item["request"]["path"]).read_text())
            out = Path(request["output_directory"]); out.mkdir(parents=True)
            panel.write_json(out / "start.json", {"schema": "public-input-evaluation-start/v1", "command": request})
            for index, case in enumerate(item["cases"]):
                # Every fifth match's game 1 is a natural draw (no winner); match files hold only natural games.
                game = {"start": {"starting_player": case["starting_player"]}, "winner": None if index % 5 == 4 else index % 2}
                (out / f"match-{index:06d}.json").write_text(json.dumps(
                    {"match": {"config": {"seed": case["seed"]}}, "games": [game, {"winner": 0}]}))
        result = panel.rows(self.root / "p2" / "plan.json")
        self.assertEqual(len(result), 216)
        for row in result:
            self.assertTrue(row["complete"])
            self.assertEqual(row["checkpoint_sha256"], "c" * 64)
            self.assertIn(row["score"], (0.0, 0.5, 1.0))
        draws = [row for row in result if row["score"] == 0.5]
        self.assertEqual(len(draws), sum(len(item["cases"][4::5]) for item in plan["requests"]))
        first = next(row for row in result if row["seat"] == 1)
        self.assertEqual(first["score"], 0.0)  # winner 0 is the opponent when the candidate sits in seat 1

    def test_failed_match_is_a_technical_termination_and_the_shard_continues(self):
        # A stand-in evaluator: plays matches in order, stops at a seed listed in fail.json as the
        # public evaluator does (match files before it, then failure.json naming the seed).
        fake = self.root / "fake_evaluator.py"
        fake.write_text("""import json, sys
from pathlib import Path
request = json.loads(Path(sys.argv[1]).read_text())
out = Path(request["output_directory"]); out.mkdir()
(out / "start.json").write_text(json.dumps({"schema": "public-input-evaluation-start/v1", "command": request}))
fail = set(json.loads((Path(sys.argv[0]).parent / "fail.json").read_text()))
for index, match in enumerate(request["matches"]):
    seed = match["config"]["seed"]
    if seed in fail:
        (out / "failure.json").write_text(json.dumps({"error": f"A versus B, match seed {seed}, game 1, environment seed 123: InvalidReference"}))
        sys.exit(1)
    game = {"start": {"starting_player": match["config"]["game_one_chooser"]}, "winner": 0}
    (out / f"match-{index:06d}.json").write_text(json.dumps({"match": {"config": {"seed": seed}}, "games": [game]}))
(out / "completion.json").write_text("{}")
""")
        executable = self.root / "evaluator.bat"
        executable.write_text(f'@"{sys.executable}" -B "{fake}" %*\n')
        plan = panel.build("p2", self.candidate, self.opponent, self.decks, self.root / "p2", 1, own_decks=(4,))
        item = plan["requests"][0]
        failing = [item["cases"][3]["seed"], item["cases"][4]["seed"], item["cases"][-1]["seed"]]
        (self.root / "fail.json").write_text(json.dumps(failing))
        request = Path(item["request"]["path"])
        with mock.patch.object(panel, "held_token", return_value="t"), \
                mock.patch.object(panel.reservations, "record_descendant"):
            self.assertFalse(panel.run_one(str(executable), request, "t"))
            panel.recover(str(executable), request)
        recovery = json.loads(request.with_suffix(".recovery.json").read_text())
        # An interrupted recovery reruns: finished continuations are reused, a partial one is set aside.
        first = Path(recovery["segments"][1]["output_directory"]); last = Path(recovery["segments"][-1]["output_directory"])
        (last / "completion.json").unlink(missing_ok=True); (last / "failure.json").unlink(missing_ok=True)
        with mock.patch.object(panel, "held_token", return_value="t"),                 mock.patch.object(panel.reservations, "record_descendant"):
            self.assertEqual(json.loads(Path(panel.recover(str(executable), request)["path"]).read_text())["errors"],
                             recovery["errors"])
        self.assertTrue(first.exists())
        self.assertEqual(len(list(last.parent.glob(last.name + ".partial-*"))), 1)
        recovery = json.loads(request.with_suffix(".recovery.json").read_text())
        self.assertEqual([e["index"] for e in recovery["errors"]], [3, 4, len(item["cases"]) - 1])
        for other in plan["requests"][1:]:  # seat 1: plain completion
            other_request = Path(other["request"]["path"])
            with mock.patch.object(panel, "held_token", return_value="t"),                     mock.patch.object(panel.reservations, "record_descendant"):
                if not panel.run_one(str(executable), other_request, "t"):
                    panel.recover(str(executable), other_request)
        result = [row for row in panel.rows(self.root / "p2" / "plan.json") if row["seat"] == item["seat"]]
        self.assertEqual(len(result), len(item["cases"]))
        self.assertEqual([row["seed"] for row in result], [case["seed"] for case in item["cases"]])
        incomplete = [index for index, row in enumerate(result) if not row["complete"]]
        self.assertEqual(incomplete, [3, 4, len(item["cases"]) - 1])
        self.assertTrue(all(result[i]["score"] is None and "InvalidReference" in result[i]["technical"]
                            and result[i]["technical_game"] == 1 for i in incomplete))

    def test_run_refuses_receipt_for_other_schedule(self):
        checkpoint = self.root / "c.json"
        checkpoint.write_text(json.dumps({"schema": "s", "source_import": {}, "feature_contract_digest": "a",
                                          "feature_encoding_digest": "b", "card_db_hash": "h",
                                          "parameters": [{"name": "w", "shape": [1], "values": [0]}]}))
        self.candidate["checkpoint"] = {"path": str(checkpoint), "sha256": hashlib.sha256(checkpoint.read_bytes()).hexdigest()}
        plan = panel.build("p2", self.candidate, self.opponent, self.decks, self.root / "p2", 1)
        executable = self.root / "evaluator.exe"; executable.write_bytes(b"binary")
        receipt = {"schema": panel.RECEIPT, "evaluator_sha256": hashlib.sha256(b"binary").hexdigest(),
                   "schedule_sha256": "other", "opponent": self.opponent,
                   "candidate_layout": panel.candidate_layout(self.candidate), "trials": [], "selected_workers": 1}
        (self.root / "receipt.json").write_text(json.dumps(receipt))
        with self.assertRaises(SystemExit):
            panel.run(self.root / "p2" / "plan.json", self.root / "receipt.json", str(executable))


if __name__ == "__main__":
    unittest.main()
