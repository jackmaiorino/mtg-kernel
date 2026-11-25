"""Unit tests for tools/cube_v1/deck_signal_summary.py on small deck-signal fixtures."""

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

import deal  # noqa: E402
import deck_signal_summary as dss  # noqa: E402

FIXTURE_A = Path(__file__).resolve().parent / "fixtures" / "deck_signal_a.jsonl"
HALT = {"kind": "terminal", "code": "engine_halted", "classification": "engine_halted",
        "reason": "engine_halted:InvalidEffectContinuation:source:77"}
HALT_KIND = "engine_halted:InvalidEffectContinuation:source:*"


def game(index, *, winner=None, halted=False, draw=False, **fields):
    """One line; `winner` is the winning session player of a completed game."""
    if halted:
        end, score, stop = HALT, None, HALT_KIND
    elif draw:
        end, score, stop = {"kind": "natural", "winner_seat": None, "reason": "draw"}, [0.5, 0.5], None
    else:
        end = {"kind": "natural", "winner_seat": winner, "reason": "life"}
        score, stop = ([1.0, 0.0] if winner == 0 else [0.0, 1.0]), None
    row = {
        "schema": dss.GAME_LINE_SCHEMA, "index": index, "end": end, "completed": not halted,
        "stop_kind": stop, "score": score, "winner_player": None if (halted or draw) else winner,
        "draw": draw, "driver_decisions": 100 + index, "physical_decisions": 90 + index,
        "policy_steps": 110 + index,
    }
    row.update(fields)
    return row


def a_game(pair, pair_game, *, c_result):
    """A part A game: `c_result` is "win", "loss", "draw" or "halt" for C."""
    kinds = ["C", "P"] if pair_game == 0 else ["P", "C"]
    c_seat = kinds.index("C")
    decks = [[1, 2], [3, 4]] if pair_game == 0 else [[3, 4], [1, 2]]
    fields = {"part": "A", "pair": pair, "pair_game": pair_game, "kinds": kinds, "decks": decks}
    index = 2 * pair + pair_game
    if c_result == "halt":
        return game(index, halted=True, **fields)
    if c_result == "draw":
        return game(index, draw=True, **fields)
    return game(index, winner=c_seat if c_result == "win" else 1 - c_seat, **fields)


def a_rows(results):
    return [a_game(pair, g, c_result=r) for pair, two in enumerate(results) for g, r in enumerate(two)]


BASIC = 900


def b_deck(deck):
    """Ring deck `deck`'s cards: one card of its own and a basic land every deck holds."""
    return [100 + deck, BASIC]


def b_rows(matchups, sources=None):
    """Part B lines: `matchups` is a list of ((anchor, other), [anchor result per game]), each
    result "win", "loss", "draw" or "halt" for the anchor; games alternate seat 0 and 1.
    `sources` maps a line index to the halted game's last action source."""
    sources = sources or {}
    rows = []
    for m, ((anchor, other), results) in enumerate(matchups):
        for k, result in enumerate(results):
            seat = k % 2
            seat_decks = [anchor, other] if seat == 0 else [other, anchor]
            fields = {"part": "B", "matchup": m, "matchup_decks": [anchor, other], "deal": k // 2,
                      "seat": seat, "seat_decks": seat_decks, "kinds": ["C", "C"],
                      "decks": [b_deck(seat_decks[0]), b_deck(seat_decks[1])]}
            index = len(rows)
            if result == "halt":
                rows.append(game(index, halted=True, last_action_source=sources.get(index), **fields))
            elif result == "draw":
                rows.append(game(index, draw=True, **fields))
            else:
                rows.append(game(index, winner=seat if result == "win" else 1 - seat, **fields))
    return rows


class BootstrapTest(unittest.TestCase):
    def test_resample_seed_is_deal_seed(self):
        for namespace in ("deck-signal-A", "deck-signal-B"):
            for b, t in ((0, 0), (1, 399), (9999, 31), (123, 7)):
                self.assertEqual(dss.resample_seed(namespace, b, t), deal.seed(["bootstrap", namespace, b, t]))
        self.assertEqual(dss.resample_seed("deck-signal-A", 0, 0), 211135417479132707)
        self.assertEqual(dss.resample_seed("deck-signal-B", 9999, 31), 543775953320694475)
        with self.assertRaises(ValueError):
            dss.bootstrap('bad"name', 3, {"x": lambda idx: 0.0}, 10)

    def test_interval_positions(self):
        self.assertEqual(dss.interval_positions(10_000), (249, 9750))

    def test_interval_matches_a_direct_computation(self):
        values = [0.0, 0.5, 1.0, 0.25, 0.75]
        n = len(values)
        means = sorted(
            sum(values[deal.seed(["bootstrap", "deck-signal-A", b, t]) % n] for t in range(n)) / n
            for b in range(10_000)
        )
        got = dss.bootstrap("deck-signal-A", n, {"mean": lambda idx: sum(values[i] for i in idx) / n})
        self.assertEqual(got["mean"], (means[249], means[9750]))


class PartATest(unittest.TestCase):
    def test_fixture_file_scores_and_reading(self):
        rows = dss.read_lines(FIXTURE_A)
        summary = dss.summarise(rows)
        # Four pairs, not the declared 400: no reading is issued.
        self.assertEqual(summary["reading"], "incomplete")
        self.assertEqual(summary["reading_if_complete"], "technical_failure")
        self.assertFalse(summary["complete"])
        summary = dss.summarise_a(rows)
        self.assertEqual(summary["part"], "A")
        self.assertEqual(summary["pairs"], 4)
        # Pair scores: 1, 0.25, then two pairs with one halted game and one C win.
        means = {name: t["mean_pair_score"] for name, t in summary["treatments"].items()}
        self.assertEqual(means, {"draw": 0.6875, "c_loss": 0.5625, "c_win": 0.8125})
        self.assertEqual(summary["halted"], 2)
        self.assertEqual(summary["halted_by_stop_kind"], {HALT_KIND: 2})
        self.assertEqual(summary["stop_kind_mismatches"], [])
        self.assertEqual(summary["c_results_completed"], {"wins": 4, "draws": 1, "losses": 1})
        # 2 of 8 games halted: more than 5%.
        self.assertEqual(summary["reading"], "technical_failure")
        for treatment in summary["treatments"].values():
            self.assertLessEqual(treatment["lower"], treatment["mean_pair_score"])
            self.assertGreaterEqual(treatment["upper"], treatment["mean_pair_score"])

    def test_readings_follow_the_fixed_threshold(self):
        self.assertEqual(dss.summarise_a(a_rows([("win", "win")] * 30))["reading"], "positive")
        self.assertEqual(dss.summarise_a(a_rows([("loss", "loss")] * 30))["reading"], "negative")
        # Every pair at 0.5: a zero-width interval below 0.55.
        self.assertEqual(dss.summarise_a(a_rows([("win", "loss")] * 30))["reading"], "negative")
        # Pairs at 1 and 0: mean 0.5 with an interval across 0.55.
        wide = dss.summarise_a(a_rows([("win", "win"), ("loss", "loss")] * 15))
        self.assertLess(wide["treatments"]["draw"]["lower"], 0.55)
        self.assertGreater(wide["treatments"]["draw"]["upper"], 0.55)
        self.assertEqual(wide["reading"], "inconclusive")
        self.assertEqual(dss.reading_of(0.551, 0.7), "positive")
        self.assertEqual(dss.reading_of(0.55, 0.7), "inconclusive")
        self.assertEqual(dss.reading_of(0.3, 0.549), "negative")
        self.assertEqual(dss.reading_of(0.3, 0.55), "inconclusive")

    def test_treatments_must_agree(self):
        # 1 halt in 40 games (2.5%). Every other pair scores 0.5, so the halted pair decides:
        # as a C loss the interval is [0.5, 0.5] (negative), as 0.5 it stays below 0.55
        # (negative), as a C win it reaches 0.55 (inconclusive). The treatments disagree.
        rows = a_rows([("win", "loss")] * 19 + [("halt", "win")])
        summary = dss.summarise_a(rows)
        self.assertFalse(summary["halt_share_above_5pct"])
        self.assertEqual(summary["treatments"]["c_loss"]["reading"], "negative")
        self.assertEqual(summary["treatments"]["draw"]["reading"], "negative")
        self.assertEqual(summary["treatments"]["c_win"]["reading"], "inconclusive")
        self.assertEqual(summary["reading"], "inconclusive")

    def test_exactly_five_percent_halted_is_not_a_failure(self):
        rows = a_rows([("halt", "win")] + [("win", "win")] * 9)
        summary = dss.summarise_a(rows)
        self.assertEqual(summary["halt_share"], 0.05)
        self.assertEqual(summary["reading"], "positive")
        rows = a_rows([("halt", "halt")] + [("win", "win")] * 9)
        self.assertEqual(dss.summarise_a(rows)["reading"], "technical_failure")

    def test_refuses_bad_pairs_and_indices(self):
        rows = a_rows([("win", "win")] * 2)
        with self.assertRaises(ValueError):
            dss.summarise(rows[1:])
        swapped = [dict(row) for row in rows]
        swapped[1]["kinds"] = ["C", "P"]
        with self.assertRaises(ValueError):
            dss.summarise(swapped)
        mixed = [dict(row) for row in rows]
        mixed[3]["part"] = "B"
        with self.assertRaises(ValueError):
            dss.summarise(mixed)

    def test_cli_writes_the_summary(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "summary.json"
            record = Path(tmp) / "run.json"
            record.write_text(json.dumps({"part": "A", "fence_sha256": "ab", "seed_namespace": "deck-signal"}))
            with redirect_stdout(io.StringIO()) as printed:
                self.assertEqual(dss.main(["--games", str(FIXTURE_A), "--record", str(record), "--out", str(out)]), 0)
            summary = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(summary["reading"], "incomplete")
            self.assertEqual(summary["run_record"]["fence_sha256"], "ab")
            self.assertEqual(json.loads(printed.getvalue())["reading"], "incomplete")
            with self.assertRaises(SystemExit):
                dss.main(["--games", str(FIXTURE_A), "--out", str(out)])


class PartBTest(unittest.TestCase):
    # Five decks, every pair meets (a ring of five with reach two), 8 games a matchup.
    STRENGTHS = [2 / 16, -2 / 16, 1 / 16, -1 / 16, 0.0]
    PAIRS = [(i, (i + k) % 5) for i in range(5) for k in (1, 2)]

    def results_for(self, anchor, other):
        """8 anchor results whose mean is exactly 0.5 + d[anchor] - d[other]."""
        half_points = round(16 * (0.5 + self.STRENGTHS[anchor] - self.STRENGTHS[other]))
        wins, draw = divmod(half_points, 2)
        return ["win"] * wins + ["draw"] * draw + ["loss"] * (8 - wins - draw)

    def test_fit_recovers_exact_strengths(self):
        rows = b_rows([((a, o), self.results_for(a, o)) for a, o in self.PAIRS])
        summary = dss.summarise(rows)
        self.assertEqual(summary["decks"], 5)
        self.assertEqual(summary["halted"], 0)
        treatments = summary["treatments"]
        for name in ("draw", "culprit_loss", "culprit_win"):
            for got, want in zip(treatments[name]["strengths"], self.STRENGTHS):
                self.assertAlmostEqual(got, want, places=12)
        draw = treatments["draw"]
        raw = (sum(d * d for d in self.STRENGTHS) / 4) ** 0.5
        self.assertAlmostEqual(draw["raw_sd"], raw, places=12)
        self.assertLess(draw["sigma_d"], draw["raw_sd"])
        self.assertLessEqual(draw["raw_sd_lower"], draw["raw_sd_upper"])
        self.assertLessEqual(draw["sigma_d_lower"], draw["sigma_d_upper"])

    def halted_summary(self, source):
        """Game 0 of matchup 0 (anchor 0 on the play against deck 1) halts at `source`."""
        matchups = [((a, o), self.results_for(a, o)) for a, o in self.PAIRS]
        (anchor, other), results = matchups[0]
        self.assertEqual((anchor, other), (0, 1))
        matchups[0] = ((anchor, other), ["halt"] + results[1:])
        return dss.summarise(b_rows(matchups, {0: source}))

    def test_halt_charged_to_the_deck_holding_the_source(self):
        for source, culprit in ((b_deck(0)[0], 0), (b_deck(1)[0], 1)):
            summary = self.halted_summary(source)
            self.assertEqual((summary["halted"], summary["halts_charged_to_a_deck"]), (1, 1))
            t = summary["treatments"]
            loss, draw, win = (t[name]["strengths"][culprit] for name in ("culprit_loss", "draw", "culprit_win"))
            self.assertLess(loss, draw)
            self.assertGreater(win, draw)

    def test_uncharged_halt_counts_half_in_every_treatment(self):
        for source in (BASIC, 12345, None):
            summary = self.halted_summary(source)
            self.assertEqual(summary["halts_uncharged"], 1)
            t = summary["treatments"]
            self.assertEqual(t["culprit_loss"]["strengths"], t["draw"]["strengths"])
            self.assertEqual(t["culprit_win"]["strengths"], t["draw"]["strengths"])

    def test_spread_and_inverse(self):
        raw, corrected = dss.spread([0.1, -0.1, 0.0], [0.001, 0.001, 0.001])
        self.assertAlmostEqual(raw, 0.1)
        self.assertAlmostEqual(corrected, ((0.02 - 0.003) / 2) ** 0.5)
        self.assertEqual(dss.spread([0.0, 0.0, 0.0], [0.1, 0.1, 0.1])[1], 0.0)
        inverse = dss.invert([[2.0, 1.0], [1.0, 3.0]])
        self.assertAlmostEqual(inverse[0][0], 0.6)
        self.assertAlmostEqual(inverse[0][1], -0.2)
        with self.assertRaises(ValueError):
            dss.fit_additive([(0, 1, 1.0), (0, 1, 0.0), (2, 3, 1.0), (2, 3, 0.5)], 4)

    def test_refuses_a_seat_that_does_not_match(self):
        rows = b_rows([((a, o), self.results_for(a, o)) for a, o in self.PAIRS])
        rows[1]["seat_decks"] = rows[1]["matchup_decks"]
        with self.assertRaises(ValueError):
            dss.summarise(rows)


if __name__ == "__main__":
    unittest.main()
