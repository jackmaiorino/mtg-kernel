"""Unit tests for tools/cube_v1/b_census_summary.py on a small census fixture."""

from __future__ import annotations

import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from contextlib import redirect_stdout

CUBE_V1_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(CUBE_V1_DIR))

import b_census_summary as bcs  # noqa: E402

BASIC = 900


def line(index, end, decks, stop_kind, *, driver=100, physical=None, policy=None, source=None, recent=None,
         draw=False):
    completed = end["kind"] == "natural"
    return {
        "schema": bcs.GAME_LINE_SCHEMA, "mode": "drafted", "index": index, "decks": decks, "end": end,
        "completed": completed, "stop_kind": stop_kind, "driver_decisions": driver,
        "physical_decisions": physical, "policy_steps": policy, "last_action_source": source,
        "recent_action_source": recent, "draw": draw,
    }


def fixture():
    """Six games: three completed (one a draw), one engine halt charged to card 2, one panic
    with no source, one process death; card 1 is in every game, card 3 in two."""
    natural = {"kind": "natural", "winner_seat": 1, "reason": "life"}
    halt = {"kind": "terminal", "code": "engine_halted", "classification": "engine_halted",
            "reason": "engine_halted:InvalidEffectContinuation:source:77"}
    return [
        line(0, natural, [[1, 2, BASIC], [3, BASIC]], None, driver=1000, physical=900, policy=1000),
        line(1, dict(natural, winner_seat=None), [[1, BASIC], [4]], None, driver=9000, physical=8000,
             policy=9000, draw=True),
        line(2, halt, [[1, 2], [5]], "engine_halted:InvalidEffectContinuation:source:*", driver=50, source=2,
             recent=2),
        line(3, {"kind": "panic", "message": "x" * 200}, [[1], [3]], "panic: " + "x" * 160, driver=7, source=3,
             recent=3),
        line(4, {"kind": "process_died", "exit_code": 3221225725}, [[1], [6]],
             "process dies: stack overflow (0xC00000FD)", driver=None),
        line(5, natural, [[1], [2]], None, driver=3000, physical=2000, policy=3000),
    ]


class NormaliserTests(unittest.TestCase):
    def test_stop_kinds_match_the_census_normaliser(self):
        self.assertIsNone(bcs.stop_kind({"kind": "natural", "winner_seat": None, "reason": ""}))
        self.assertEqual(
            bcs.stop_kind({"kind": "terminal", "classification": "fail_closed", "reason": "a source:12 b source:x"}),
            "a source:* b source:x",
        )
        self.assertEqual(bcs.stop_kind({"kind": "decision_cap"}), "driver_decision_cap")
        self.assertEqual(bcs.stop_kind({"kind": "panic", "message": "é" * 170}), "panic: " + "é" * 160)
        self.assertEqual(
            bcs.stop_kind({"kind": "session_error", "code": "stale_environment_binding", "message": "m"}),
            "session error stale_environment_binding: m",
        )
        self.assertEqual(bcs.stop_kind({"kind": "process_died", "exit_code": 3221225725}),
                         "process dies: stack overflow (0xC00000FD)")
        self.assertEqual(bcs.stop_kind({"kind": "process_died", "exit_code": 3}), "process dies: exit 3")
        self.assertEqual(bcs.stop_kind({"kind": "process_died", "signal": 11}), "process dies: signal 11")
        # Classified natural without natural_game_over: halted (P1), named by its reason.
        self.assertEqual(
            bcs.stop_kind({"kind": "terminal", "classification": "natural", "reason": "turn source:3"}),
            "turn source:*",
        )

    def test_census_shapes_agree_with_cause_of(self):
        self.assertEqual(bcs.cause_of({"end": "driver_decision_cap"}), "driver_decision_cap")
        self.assertEqual(bcs.cause_of(bcs.census_end({"kind": "session_error", "code": "c", "message": "m"})),
                         "session error c: m")


class SummaryReviewFixTests(unittest.TestCase):
    def test_a_panic_charges_no_card_in_either_column(self):
        summary = bcs.summarise(fixture())
        card3 = next(c for c in summary["cards"] if c["card"] == 3)
        self.assertEqual(card3["charged"], 0)
        self.assertEqual(card3["charged_recent"], 0)
        self.assertEqual(card3["panic_last_source"], 1)

    def test_a_token_source_is_reported_apart_not_an_order_error(self):
        halt = {"kind": "decision_cap"}
        rows = fixture()[:1] + [dict(line(1, halt, [[1], [2]], "driver_decision_cap", source=4242, recent=4242),
                                     last_action_source_name="Clue")]
        summary = bcs.summarise(rows)
        self.assertEqual(summary["order_errors"], [])
        self.assertEqual(summary["token_sources"], [{"card": 4242, "name": "Clue", "charged": 1}])

    def test_index_start_is_the_first_index_even_with_a_gap(self):
        rows = [line(i, {"kind": "natural", "winner_seat": 0, "reason": "life"}, [[1], [2]], None) for i in (5, 6, 8)]
        summary = bcs.summarise(rows)
        self.assertEqual(summary["index_start"], 5)
        self.assertEqual(summary["order_errors"], ["index 8 follows 6"])


class SummaryTests(unittest.TestCase):
    def setUp(self):
        self.summary = bcs.summarise(fixture(), {1: "One", 2: "Two"})

    def test_completion_and_stop_kinds(self):
        s = self.summary
        self.assertEqual((s["games"], s["completed"], s["halted"], s["natural_draws"]), (6, 3, 3, 1))
        self.assertAlmostEqual(s["halted_share"], 0.5)
        self.assertEqual(s["order_errors"], [])
        self.assertEqual(s["stop_kind_mismatches"], [])
        kinds = {k["stop_kind"]: k for k in s["stop_kinds"]}
        self.assertEqual(len(kinds), 3)
        halt = kinds["engine_halted:InvalidEffectContinuation:source:*"]
        self.assertEqual((halt["games"], halt["replay_index"]), (1, 2))
        self.assertAlmostEqual(halt["share"], 1 / 6)
        self.assertEqual(halt["last_action_source"], [{"card": 2, "name": "Two", "games": 1}])
        panic = kinds["panic: " + "x" * 160]
        self.assertEqual(panic["last_action_source"], [])
        self.assertEqual(panic["panic_last_source"], [{"card": 3, "name": "#3", "games": 1}])

    def test_per_card_denominators_are_games_containing_the_card(self):
        cards = {c["card"]: c for c in self.summary["cards"]}
        self.assertEqual(cards[1]["games_present"], 6)
        self.assertEqual(cards[1]["halted_present"], 3)
        self.assertEqual(cards[1]["charged"], 0)
        two = cards[2]
        self.assertEqual((two["games_present"], two["halted_present"], two["charged"]), (3, 1, 1))
        self.assertAlmostEqual(two["charged_rate"], 1 / 3)
        self.assertEqual(two["charged_recent"], 1)
        self.assertEqual(cards[3]["games_present"], 2)
        self.assertEqual(cards[3]["name"], "#3")
        # A panic charges no card (A2.5 item 4); its last source is reported apart.
        self.assertEqual((cards[3]["charged"], cards[3]["panic_last_source"]), (0, 1))
        # A card counts once per game even when both decks hold it.
        self.assertEqual(cards[BASIC]["games_present"], 2)
        # Charged cards sort first.
        self.assertEqual(self.summary["cards"][0]["card"], 2)

    def test_percentiles_are_nearest_rank(self):
        self.assertEqual(bcs.percentile(list(range(1, 1001)), 99.9), 999)
        self.assertEqual(bcs.percentile(list(range(1, 1001)), 50), 500)
        self.assertEqual(bcs.percentile([5], 99.9), 5)
        self.assertIsNone(bcs.percentile([], 50))
        completed = self.summary["percentiles_completed_games"]["driver_decisions"]
        self.assertEqual((completed["n"], completed["p50"], completed["p99"], completed["max"]), (3, 3000, 9000, 9000))
        everything = self.summary["percentiles_all_games"]["driver_decisions"]
        # The process death has no driver count.
        self.assertEqual(everything["n"], 5)
        self.assertEqual(self.summary["percentiles_all_games"]["physical_decisions"]["n"], 3)

    def test_basic_lands_are_left_out_of_cards_present(self):
        rows = fixture()
        rows[2]["decks"] = [[1, 2, BASIC], [BASIC, 5]]
        s = bcs.summarise(rows, {BASIC: "Plains", 1: "One"})
        halt = next(k for k in s["stop_kinds"] if k["stop_kind"].startswith("engine_halted"))
        self.assertNotIn(BASIC, [c["card"] for c in halt["cards_present"]])
        self.assertEqual([c["card"] for c in halt["cards_present"]], [1, 2, 5])
        # The per-card table still counts the basic land.
        cards = {c["card"]: c for c in s["cards"]}
        self.assertEqual(cards[BASIC]["halted_present"], 1)

    def test_warning_8192(self):
        warning = self.summary["warning_8192"]
        self.assertEqual(warning["completed_over"], 1)
        self.assertTrue(warning["fires"])

    def test_gaps_and_stop_kind_mismatches_are_reported(self):
        rows = fixture()
        del rows[3]
        rows[0]["stop_kind"] = "wrong"
        s = bcs.summarise(rows)
        self.assertEqual(s["order_errors"], ["index 4 follows 2"])
        self.assertEqual(s["stop_kind_mismatches"], [{"index": 0, "line": "wrong", "normaliser": None}])


class CliTests(unittest.TestCase):
    def test_main_writes_json_and_markdown_and_reads_names_from_the_record(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            rows = fixture()
            (tmp / "a.jsonl").write_text("".join(json.dumps(r) + "\n" for r in rows[:4]), encoding="utf-8")
            (tmp / "b.jsonl").write_text("".join(json.dumps(r) + "\n" for r in rows[4:]), encoding="utf-8")
            (tmp / "run.json").write_text(json.dumps({"card_names": {"2": "Two"}}), encoding="utf-8")
            with redirect_stdout(io.StringIO()):
                code = bcs.main(["--games", str(tmp / "a.jsonl"), str(tmp / "b.jsonl"),
                                 "--record", str(tmp / "run.json"),
                                 "--out", str(tmp / "s.json"), "--markdown", str(tmp / "s.md")])
            self.assertEqual(code, 0)
            summary = json.loads((tmp / "s.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["games"], 6)
            self.assertEqual(summary["cards"][0]["name"], "Two")
            text = (tmp / "s.md").read_text(encoding="utf-8")
            self.assertIn("| `process dies: stack overflow (0xC00000FD)` | 1 |", text)

    def test_main_refuses_another_schema(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "x.jsonl"
            path.write_text(json.dumps({"schema": "other"}) + "\n", encoding="utf-8")
            with self.assertRaises(ValueError):
                bcs.main(["--games", str(path), "--out", str(Path(tmp) / "s.json")])


if __name__ == "__main__":
    unittest.main()
