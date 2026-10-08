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
        self.assertEqual(sum(len(item["cases"]) for item in plan["requests"]), 1944)
        for item in plan["requests"]:
            request = json.loads(Path(item["request"]["path"]).read_text())
            seat = item["seat"]
            self.assertEqual(request["sources"][seat]["source"], self.candidate)
            self.assertEqual(request["sources"][1 - seat], self.opponent)
            for case, match in zip(item["cases"], request["matches"]):
                self.assertEqual(case["seat"], seat)
                self.assertEqual(match["config"]["deck_ids"][seat].split("/")[0], ndb.DECK_NAMES[case["own"]])
                self.assertEqual(match["config"]["game_one_chooser"], case["starting_player"])
                self.assertEqual(match["config"]["max_physical_decisions"], panel.MAX_DECISIONS)

    def test_rows_read_game_one_for_the_candidate_seat(self):
        plan = panel.build("p2", self.candidate, self.opponent, self.decks, self.root / "p2", 2,
                           own_decks=(4,))
        for item in plan["requests"]:
            request = json.loads(Path(item["request"]["path"]).read_text())
            out = Path(request["output_directory"]); out.mkdir(parents=True)
            for index, case in enumerate(item["cases"]):
                game = {"start": {"starting_player": case["starting_player"]}, "winner": index % 2}
                (out / f"match-{index:06d}.json").write_text(json.dumps(
                    {"match": {"config": {"seed": case["seed"]}}, "games": [game, {"winner": 0}]}))
        result = panel.rows(self.root / "p2" / "plan.json")
        self.assertEqual(len(result), 216)
        for row in result:
            self.assertTrue(row["complete"])
            self.assertIn(row["score"], (0.0, 1.0))
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
fail = set(json.loads((Path(sys.argv[0]).parent / "fail.json").read_text()))
for index, match in enumerate(request["matches"]):
    seed = match["config"]["seed"]
    if seed in fail:
        (out / "failure.json").write_text(json.dumps({"error": f"A versus B, match seed {seed}, game 1: InvalidReference"}))
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
        self.assertTrue(all(result[i]["score"] is None and "InvalidReference" in result[i]["technical"] for i in incomplete))

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
