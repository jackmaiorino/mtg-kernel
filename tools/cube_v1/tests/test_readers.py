"""Tests of the M4 readers (D7, D7a): pod_record.py, deal_common.py, read_plans.py and readers.py.

Every record here is synthetic, written to the frozen schema in the `test` namespace.
"""

from __future__ import annotations

import copy
import hashlib
import json
import math
from decimal import Decimal
from pathlib import Path
import sys
import tempfile
from typing import Callable
import unittest

TESTS_DIR = Path(__file__).resolve().parent
CUBE_V1_DIR = TESTS_DIR.parent
sys.path.insert(0, str(CUBE_V1_DIR))
sys.path.insert(0, str(TESTS_DIR))

import cube_synth as synth  # noqa: E402
import deal  # noqa: E402
import deal_common  # noqa: E402
import pod_record  # noqa: E402
import read_plans  # noqa: E402
import readers  # noqa: E402

FIXTURES_DIR = TESTS_DIR / "fixtures"
D5_UNIFORM_FIXTURE = FIXTURES_DIR / "d5_uniform_pod_record.json"
TEST_DECLARATION = FIXTURES_DIR / "read_declaration_test.json"


def _declaration(**overrides) -> tuple[dict, str]:
    """The fixture declaration, with section overrides given as {"c.sizing.spread": value}."""
    doc = json.loads(TEST_DECLARATION.read_text(encoding="utf-8"), parse_float=Decimal)
    for dotted, value in overrides.items():
        node = doc["reads"]
        *head, last = dotted.split(".")
        for key in head:
            node = node[key]
        node[last] = value
    return readers.validate_declaration(doc), "0" * 64


def _files(records: list[dict]) -> list[tuple[list[dict], str]]:
    return [([r], hashlib.sha256(json.dumps(r).encode()).hexdigest()) for r in records]


def _plans(plans: list[dict]) -> list[tuple[dict, str]]:
    return [(p, hashlib.sha256(json.dumps(p).encode()).hexdigest()) for p in plans]


def _c_records(count: int, planted_k: Callable | None = None, halts=None, kind: str = "paired_build"):
    """(c)-shaped records with draft statistic k(d) / 64 exactly. Candidate games in slots 0 to 2
    carry the planted difference (a win or loss instead of a draw); slot 3 carries results shared by
    both coupled games, which cancel in the difference. `halts(d, seat, slot, arm)` halts a game."""
    records, plans = [], []
    for d in range(count):
        plan = synth.paired_plan(d, kind)
        k = planted_k(d)
        changed = {}
        for i in range(abs(k)):
            seat, slot = i % 8, i // 8
            changed[(seat, slot)] = "win" if k > 0 else "loss"

        def result_of(game, position, d=d, changed=changed):
            seat, slot = game["group"] // 4, game["group"] % 4
            arm = game["game"] % 2
            if halts is not None and halts(d, seat, slot, arm):
                return "halt"
            if slot == 3:
                return ("win", "loss", "draw")[(d + seat) % 3]
            if arm == 0 and (seat, slot) in changed:
                return changed[(seat, slot)]
            return "draw"

        records.append(synth.record_from_plan(plan, result_of))
        plans.append(plan)
    return records, plans



def _planted(value: float, spread_k: int = 4) -> Callable[[int], int]:
    return lambda d: round(value * 64 + (spread_k if d % 2 == 0 else -spread_k))


class SeedAndIndexTests(unittest.TestCase):
    def test_fast_resample_indices_equal_deal_seed(self):
        root = readers.bootstrap_root("test", "c", 1)
        self.assertEqual(root, deal.seed(["test", "bootstrap", "c", 1]))
        for b in (0, 1, 9999):
            idx = readers.resample_indices(root, b, 37)
            self.assertEqual(idx, [deal.seed([root, b, t]) % 37 for t in range(37)])

    def test_percentile_interpolates_at_p_times_b_minus_one(self):
        values = [float(v) for v in range(11)]
        self.assertEqual(readers.percentile(values, 0.25), 2.5)
        self.assertEqual(readers.percentile(values, 0.0), 0.0)
        self.assertEqual(readers.percentile(values, 1.0), 10.0)


class SizingTests(unittest.TestCase):
    def test_formula_cap_floor_and_exact_rounding(self):
        decl, _ = _declaration()
        sizing = dict(decl["reads"]["c"]["sizing"])
        sizing.update(spread=Decimal("0.0625"), effect=Decimal("0.02"), cap=10_000, floor=2)
        self.assertEqual(readers.first_look_units(sizing), 77)  # ceil(8.75^2 = 76.5625)
        # (2.8 x 0.09 / 0.036)^2 is exactly 49; binary floating point rounds above it and gives 50.
        sizing.update(spread=Decimal("0.09"), effect=Decimal("0.036"))
        self.assertEqual(math.ceil(((1.96 + 0.84) * 0.09 / 0.036) ** 2), 50)
        self.assertEqual(readers.first_look_units(sizing), 49)
        sizing.update(effect=Decimal("0.02"))
        self.assertEqual(readers.first_look_units(sizing), 159)  # ceil(12.6^2 = 158.76)
        sizing.update(cap=40)
        self.assertEqual(readers.first_look_units(sizing), 40)
        sizing.update(spread=Decimal("0.001"), cap=10_000, floor=5)
        self.assertEqual(readers.first_look_units(sizing), 5)

    def test_extension_is_declared_multiple_and_third_look_refused(self):
        decl, _ = _declaration(**{"c.sizing.spread": Decimal("0.05")})
        section = decl["reads"]["c"]
        self.assertEqual(readers.look_units(section, 1), 49)
        self.assertEqual(readers.look_units(section, 2), 49 + 2 * 49)
        with self.assertRaisesRegex(readers.ReadError, "at most 2 looks"):
            readers.look_units(section, 3)

    def test_unrecorded_spread_refuses_to_size(self):
        decl, _ = _declaration(**{"c.sizing.spread": None})
        with self.assertRaisesRegex(readers.ReadError, "not recorded"):
            readers.look_units(decl["reads"]["c"], 1)


class DeclarationTests(unittest.TestCase):
    def _doc(self):
        return json.loads(TEST_DECLARATION.read_text(encoding="utf-8"), parse_float=Decimal)

    def test_fixture_declaration_is_valid_and_test_only(self):
        decl, _ = readers.load_declaration(TEST_DECLARATION)
        self.assertIsNone(decl["a6_sha256"])
        self.assertEqual(decl["resamples"], 10_000)
        for read in ("a", "a_early", "b", "c", "dose", "aa"):
            self.assertEqual(decl["reads"][read]["namespace"], "test")

    def test_rejects_missing_extra_and_mistyped_constants(self):
        cases = []
        doc = self._doc()
        del doc["reads"]["c"]["sizing"]["cap"]
        cases.append(doc)
        doc = self._doc()
        doc["reads"]["b"]["delta"] = Decimal("0.02")
        cases.append(doc)
        doc = self._doc()
        doc["reads"]["c"]["extension_factor"] = True
        cases.append(doc)
        doc = self._doc()
        doc["reads"]["c"]["levels"] = [Decimal("0.025")]
        cases.append(doc)
        doc = self._doc()
        doc["reads"]["c"]["levels"] = [Decimal("0.025"), Decimal("0.6")]
        cases.append(doc)
        doc = self._doc()
        doc["draw_score"] = Decimal("0.4")
        cases.append(doc)
        doc = self._doc()
        doc["reads"]["c"]["loss_if_upper_below"] = Decimal("0.5")
        cases.append(doc)
        doc = self._doc()
        doc["reads"]["a"]["scale"] = "learned_win_fraction"
        cases.append(doc)
        doc = self._doc()
        doc["schema"] = "mtg-kernel-cube-read-declaration/v0"
        cases.append(doc)
        doc = self._doc()
        doc["reads"]["c"]["sizing"]["effect"] = 0.02  # a float that bypassed exact parsing
        cases.append(doc)
        for i, doc in enumerate(cases):
            with self.subTest(case=i), self.assertRaises(readers.DeclarationError):
                readers.validate_declaration(doc)

    def test_namespaces_single_use_outside_test(self):
        doc = self._doc()
        for read in ("a", "a_early", "b", "c", "dose", "aa"):
            doc["reads"][read]["namespace"] = f"ns-{read.replace('_', '-')}"
        doc["reads"]["c"]["namespace"] = "ns-b"
        readers.validate_declaration(copy.deepcopy(doc))
        doc["reads"]["dose"]["namespace"] = "ns-a"
        with self.assertRaisesRegex(readers.DeclarationError, "already"):
            readers.validate_declaration(doc)

    def test_unsigned_declaration_refuses_read_data(self):
        decl, sha = _declaration(**{"c.namespace": "read-bc", "c.sizing.spread": Decimal("0.02")})
        with self.assertRaisesRegex(readers.ReadError, "no signed A6"):
            readers.run_read(decl, sha, "c", 1, [], [])


class CommonOpponentTests(unittest.TestCase):
    def setUp(self):
        self.pool_state = deal.validate_pool(synth.pool_doc())
        self.constants = deal.validate_constants(synth.CONSTANTS)
        self.basics = set(self.pool_state["basic_id_by_color"].values())

    def test_fifty_pools_same_opponent_for_both_builds_and_no_shared_card(self):
        checked = 0
        for d in range(7):
            plan = synth.paired_plan(d)
            pools = synth.draft_pools(d)
            # A plan built from other builds of the same pools deals the same C(P) on the same seeds.
            other = read_plans.build_read_c_plan(
                synth.pool_doc(), synth.CONSTANTS, synth.NS, synth.RUN, d, pools,
                [synth.build_from_pool(p, 1) for p in pools], [synth.build_from_pool(p, 0) for p in pools],
                synth.UNIFORM)
            for seat, entry in enumerate(plan["paired_builds"]):
                if checked == 50:
                    break
                checked += 1
                games = {g["game"]: g for g in plan["games"]}
                opp_x = {games[g]["decks"][1 - games[g]["decks"].index(entry["deck_x"])] for g in entry["games_x"]}
                opp_y = {games[g]["decks"][1 - games[g]["decks"].index(entry["deck_y"])] for g in entry["games_y"]}
                self.assertEqual(opp_x, opp_y)
                self.assertEqual(opp_x, set(entry["opponents"]))
                c_cards = plan["decks"][entry["opponents"][0]]["cards"]
                self.assertEqual(c_cards, other["decks"][entry["opponents"][0]]["cards"])
                self.assertEqual(
                    [g["game_seed"] for g in plan["games"] if g["group"] // 4 == seat],
                    [g["game_seed"] for g in other["games"] if g["group"] // 4 == seat])
                self.assertFalse({c for c in c_cards if c not in self.basics} & set(pools[seat]))
                direct = deal_common.deal_common_opponent(
                    self.pool_state, self.constants, pools[seat], synth.NS, d, seat, 0)
                self.assertEqual(direct["mainboard"], c_cards)
                self.assertEqual(len(c_cards), 40)
        self.assertEqual(checked, 50)

    def test_rule_matches_one_read_b_seat_and_key_matters(self):
        pool = synth.draft_pools(0)[0]
        deck = deal_common.deal_common_opponent(self.pool_state, self.constants, pool, "test", 0, 0, 0)["mainboard"]
        spells = {c["card_id"]: c for c in self.pool_state["spells"]}
        colours = set()
        for c in deck:
            if c in self.basics:
                colours.add(deal.COLORS[c - synth.BASIC_BASE])
        self.assertEqual(len(colours), 2)
        self.assertEqual(sum(1 for c in deck if c in spells), 23)
        self.assertTrue(all(set(spells[c]["colors"]) <= colours for c in deck if c in spells))
        self.assertEqual(sum(1 for c in deck if c in self.basics), 16)
        keys = [("test", 0, 0, 1), ("test", 1, 0, 0), ("test", 0, 1, 0), ("other", 0, 0, 0)]
        decks = {tuple(deal_common.deal_common_opponent(self.pool_state, self.constants, pool, *k)["mainboard"])
                 for k in keys}
        self.assertGreater(len(decks), 1)

    def test_game_seeds_and_seats(self):
        games = deal_common.common_games("test", 3, 5, 0)
        self.assertEqual([g["game_seed"] for g in games],
                         [deal.seed(["cube_deal_common_v1", "test", 3, 5, 0, s]) for s in range(4)])
        self.assertEqual([g["build_seat"] for g in games], [0, 1, 0, 1])

    def test_refuses_bad_pools(self):
        pool = synth.draft_pools(0)[0]
        for bad in (pool[:44], pool[:44] + [5000], pool[:44] + [pool[0]], "x"):
            with self.assertRaises(ValueError):
                deal_common.deal_common_opponent(self.pool_state, self.constants, bad, "test", 0, 0, 0)
        with self.assertRaises(ValueError):
            deal_common.deal_common_opponent(self.pool_state, self.constants, pool, "test", 0, 8, 0)

    def test_deal_py_keeps_its_bytes(self):
        # deal.py is pinned by A1.2; this job adds deal_common.py beside it and never edits it.
        text = (CUBE_V1_DIR / "deal.py").read_bytes().replace(b"\r\n", b"\n")
        self.assertEqual(hashlib.sha256(text).hexdigest(), DEAL_PY_SHA256)


DEAL_PY_SHA256 = "0e60d49f40eb9509210f4e2f71e8d2cd7dac65e393696427043978556e5af497"


class ReadBPlanTests(unittest.TestCase):
    def test_seeds_seats_and_opponents_equal_build_read_b(self):
        for d in (0, 3):
            plan = synth.read_b_plan(d)
            pools = synth.draft_pools(d)
            for seat in range(8):
                deck = synth.build_from_pool(pools[seat], 0)
                index = read_plans.read_b_index(d, seat)
                self.assertEqual(index, 8 * d + seat)
                ref = deal.build_read_b(synth.pool_doc(), synth.CONSTANTS, deck, synth.NS, synth.RUN, index)
                episodes = [e["episode"] for e in ref["iterations"][0]["episodes"]]
                games = [g for g in plan["games"] if g["group"] // 4 == seat]
                self.assertEqual([g["game_seed"] for g in games], [e["seed"] for e in episodes])
                d_id = 2 * seat
                self.assertEqual([g["decks"].index(d_id) for g in games], [e["learner_seat"] for e in episodes])
                for g, e in zip(games, episodes):
                    self.assertEqual(e["starting_player"], 0)
                    cards = [plan["decks"][x]["cards"] for x in g["decks"]]
                    self.assertEqual(cards, [deck_["mainboard"] for deck_ in e["selected"]])
                self.assertEqual(plan["decks"][2 * seat + 1]["dealt"],
                                 {"function": "cube_deal_b_v1", "key": [synth.NS, synth.RUN, index]})

    def test_plan_refuses_a_deck_outside_its_pool(self):
        pools = synth.draft_pools(0)
        decks = [synth.build_from_pool(p, 0) for p in pools]
        decks[2] = list(decks[1])
        with self.assertRaisesRegex(ValueError, "pool holds it 0 times"):
            read_plans.build_read_b_plan(synth.pool_doc(), synth.CONSTANTS, "test", "synth", 0, pools, decks,
                                         synth.UNIFORM)


class SchemaTests(unittest.TestCase):
    def setUp(self):
        self.plan = synth.paired_plan(0)
        self.record = synth.record_from_plan(self.plan, lambda g, p: "win")

    def _rejects(self, mutate, pattern):
        rec = copy.deepcopy(self.record)
        mutate(rec)
        with self.assertRaisesRegex(pod_record.RecordError, pattern):
            pod_record.validate_record(rec)

    def test_valid_record_passes_and_keys_are_in_frozen_order(self):
        pod_record.validate_record(copy.deepcopy(self.record))
        self.assertEqual(list(self.record.keys()), list(pod_record.RECORD_KEYS))

    def test_missing_extra_reordered_and_wrong_version(self):
        self._rejects(lambda r: r.pop("halts"), "missing")
        self._rejects(lambda r: r.update(extra=1), "extra")
        self._rejects(lambda r: r["games"][0].pop("episode"), "missing")
        self._rejects(lambda r: r["games"][0]["attempts"][0].update(note="x"), "extra")
        self._rejects(lambda r: r["decks"][0].pop("dealt"), "missing")

        def reorder(r):
            r["seeds"] = {"pod_seed": r["seeds"]["pod_seed"], "draft_seed": r["seeds"]["draft_seed"],
                          "league_seed": None}
        self._rejects(reorder, "order")
        self._rejects(lambda r: r.update(schema="mtg-kernel-cube-pod-record/v2"), "unknown version")
        self._rejects(lambda r: r.update(interfaces="m4-interfaces/v2"), "unknown version")

    def test_two_attempts_in_a_read_plan_game(self):
        def two(r):
            g = r["games"][0]
            first = copy.deepcopy(g["attempts"][0])
            first["end"] = {"kind": "decision_cap"}
            first["halted"] = True
            first["stop_kind"] = "decision_cap"
            second = copy.deepcopy(g["attempts"][0])
            second["attempt"] = 1
            g["attempts"] = [first, second]
            r["caps"]["max_replays"] = 3
            r["halts"] = [{"game": 0, "attempt": 0, "stop_kind": "decision_cap", "last_action_source": None,
                           "deal_seed": first["deal_seed"]}]
        self._rejects(two, "exactly one attempt")

    def test_types_ids_and_consistency(self):
        self._rejects(lambda r: r.update(pod_index=True), "integer")
        self._rejects(lambda r: r["decks"][1].update(deck=0), "its index")
        self._rejects(lambda r: r["games"][0].update(chairs=[1, None]), "chair")
        self._rejects(lambda r: r["games"][0].update(decks=[0, 99]), "not in the record")
        self._rejects(lambda r: r["games"][0].update(scores=[0, 0]), "winner")
        self._rejects(lambda r: r["games"][0].update(scores=[1, 1]), "scores")
        self._rejects(lambda r: r["seeds"].update(draft_seed=5), "differs")
        self._rejects(lambda r: r["decks"][0]["cards"].reverse(), "canonical")
        self._rejects(lambda r: r["decks"][0].update(build_rows=None), "build rows")
        def swap(r):
            ys = r["paired_builds"][0]["games_y"]
            ys[0], ys[2] = ys[2], ys[0]
        self._rejects(swap, "not coupled")
        self._rejects(lambda r: r["paired_builds"][0]["games_y"].reverse(), "different seats")
        self._rejects(lambda r: r["games"][0]["attempts"][0].update(stop_kind="natural"), "stop_kind")
        self._rejects(lambda r: r.update(namespace="Test"), "a-z0-9")

    def test_halt_ledger_must_match(self):
        plan = synth.paired_plan(1)
        rec = synth.record_from_plan(plan, lambda g, p: "halt" if g["game"] == 5 else "loss")
        self.assertEqual([h["game"] for h in rec["halts"]], [5])
        rec["halts"] = []
        with self.assertRaisesRegex(pod_record.RecordError, "ledger"):
            pod_record.validate_record(rec)

    def test_win_fractions_and_halted_game(self):
        plan = synth.paired_plan(1)
        rec = synth.record_from_plan(plan, lambda g, p: {0: "win", 1: "draw", 2: "loss"}.get(g["game"], "halt"))
        fractions = [pod_record.win_fraction(rec["games"][g], rec["games"][g]["decks"][
            [0, 1].index(next(i for i, d in enumerate(rec["games"][g]["decks"]) if d % 3 != 2))]) for g in range(4)]
        self.assertEqual(fractions, [1.0, 0.5, 0.0, None])

    def test_record_must_follow_its_plan(self):
        rec = copy.deepcopy(self.record)
        rec["games"][3]["game_seed"] += 1
        rec["games"][2]["game_seed"] += 1
        with self.assertRaisesRegex(pod_record.RecordError, "differs from the plan"):
            read_plans.check_record_against_plan(rec, self.plan)

    def test_load_records_accepts_one_record_or_an_array(self):
        with tempfile.TemporaryDirectory() as tmp:
            one = Path(tmp) / "one.json"
            many = Path(tmp) / "pods.json"
            one.write_text(json.dumps(self.record), encoding="utf-8")
            many.write_text(json.dumps([self.record, self.record]), encoding="utf-8")
            self.assertEqual(len(pod_record.load_records(one)[0]), 1)
            self.assertEqual(len(pod_record.load_records(many)[0]), 2)


class ReadCTests(unittest.TestCase):
    """(c): non-inferiority of candidate minus stated against C(P), by draft."""

    SPREAD = Decimal("0.0625")  # the declared spread s of the planted draft statistics

    def _read(self, records, plans, look=1, **overrides):
        decl, sha = _declaration(**{"c.sizing.spread": self.SPREAD, **overrides})
        return readers.run_read(decl, sha, "c", look, _files(records), _plans(plans))

    def test_known_answers_at_the_helper_size(self):
        decl, _ = _declaration(**{"c.sizing.spread": self.SPREAD})
        n = readers.look_units(decl["reads"]["c"], 1)
        self.assertEqual(n, 77)
        expected = {0.0: "win", 0.05: "win", -0.05: "loss"}
        for planted, word in expected.items():
            records, plans = _c_records(n, _planted(planted))
            report = self._read(records, plans)
            with self.subTest(planted=planted):
                self.assertEqual(report["verdict"], word, report["statistics"])
                drop = report["statistics"]["candidate_minus_stated"]["drop"]
                self.assertAlmostEqual(drop["estimate"], planted, delta=0.004)
                self.assertEqual(drop["units_scored"], n)

    def test_signed_imputation_voids_only_under_the_non_inferiority_sign(self):
        n = 77
        # 40 halted candidate games (one in each of the first 40 drafts, a draw otherwise).
        records, plans = _c_records(n, _planted(0.0), halts=lambda d, s, slot, arm: d < 40 and (s, slot, arm) == (7, 2, 0))
        report = self._read(records, plans)
        by = report["verdicts_by_scoring"]
        self.assertEqual(report["games"]["halted"], 40)
        self.assertEqual(by["drop"], "win")
        self.assertEqual(by["halts_against"], "inconclusive")  # candidate halts lost, stated halts won
        self.assertEqual(by["halts_for"], "win")  # the reversed imputation keeps the verdict
        self.assertEqual(report["void_test"]["tested"], ["halts_against"])
        self.assertTrue(report["void_test"]["void"])
        self.assertEqual(report["verdict"], "void")
        # The same halts on fewer drafts cannot flip it: not void.
        records, plans = _c_records(n, _planted(0.0), halts=lambda d, s, slot, arm: d < 4 and (s, slot, arm) == (7, 2, 0))
        report = self._read(records, plans)
        self.assertFalse(report["void_test"]["void"])
        self.assertEqual(report["verdict"], "win")

    def test_inferior_verdict_is_tested_under_the_reverse_imputation(self):
        n = 77
        halt = lambda d, s, slot, arm: d < 60 and (s, slot) in ((6, 2), (7, 2)) and arm == 0  # noqa: E731
        records, plans = _c_records(n, _planted(-0.05), halts=halt)
        report = self._read(records, plans)
        self.assertEqual(report["verdicts_by_scoring"]["drop"], "loss")
        self.assertEqual(report["void_test"]["tested"], ["halts_for"])
        self.assertEqual(report["verdicts_by_scoring"]["halts_against"], "loss")
        self.assertNotEqual(report["verdicts_by_scoring"]["halts_for"], "loss")
        self.assertEqual(report["verdict"], "void")

    def test_draft_clustering_widens_and_matches_the_analytic_error(self):
        drafts = 40
        rng_effect = [round(synth.unit_draw("effect", d) * 12) - 6 for d in range(drafts)]

        def pool_half_points(d, seat):
            return rng_effect[d] + (round(synth.unit_draw("noise", d, seat) * 2) - 1)

        records, plans = [], []
        for d in range(drafts):
            plan = synth.paired_plan(d)
            targets = {seat: pool_half_points(d, seat) for seat in range(8)}

            def result_of(game, position, targets=targets):
                seat, slot, arm = game["group"] // 4, game["group"] % 4, game["game"] % 2
                m = targets[seat]
                if m >= 0:
                    c_h = 4 + (m + 1) // 2
                    s_h = c_h - m
                else:
                    s_h = 4 + (-m + 1) // 2
                    c_h = s_h + m
                h = c_h if arm == 0 else s_h
                wins, draws = h // 2, h % 2
                return "win" if slot < wins else ("draw" if slot < wins + draws else "loss")

            records.append(synth.record_from_plan(plan, result_of))
            plans.append(plan)
        decl, sha = _declaration(**{"c.sizing.spread": Decimal("0.0001"), "c.sizing.floor": drafts,
                                    "c.sizing.cap": drafts})
        report = readers.run_read(decl, sha, "c", 1, _files(records), _plans(plans))
        drop = report["statistics"]["candidate_minus_stated"]["drop"]
        width = drop["interval"][1] - drop["interval"][0]
        # Per-pool differences are planted exactly: m / 8.
        pools = [pool_half_points(d, s) / 8 for d in range(drafts) for s in range(8)]
        draft_stats = [math.fsum(pools[8 * d: 8 * d + 8]) / 8 for d in range(drafts)]
        self.assertAlmostEqual(drop["estimate"], math.fsum(draft_stats) / drafts, places=12)
        se = drop["unit_spread"] / math.sqrt(drafts)
        analytic = 2 * 1.959963984540054 * se
        self.assertLess(abs(width / analytic - 1), 0.10, (width, analytic))
        pool_interval = readers.bootstrap_interval(pools, readers.bootstrap_root("test", "pool-level", 1),
                                                   10_000, 0.025)
        self.assertLess(pool_interval[1] - pool_interval[0], width)

    def test_two_looks_pool_both_looks_and_a_third_is_refused(self):
        overrides = {"c.sizing.spread": Decimal("0.02")}
        decl, sha = _declaration(**overrides)
        n1 = readers.look_units(decl["reads"]["c"], 1)
        n2 = readers.look_units(decl["reads"]["c"], 2)
        self.assertEqual((n1, n2), (8, 24))
        records, plans = _c_records(n2, _planted(0.0, spread_k=1))
        first = readers.run_read(decl, sha, "c", 1, _files(records[:n1]), _plans(plans[:n1]))
        second = readers.run_read(decl, sha, "c", 2, _files(records), _plans(plans))
        self.assertEqual(first["units"]["count"], 8)
        self.assertEqual(second["units"]["count"], 24)
        self.assertEqual(second["bootstrap"]["level"], 0.025)
        self.assertEqual(second["bootstrap"]["root_seed"], deal.seed(["test", "bootstrap", "c", 2]))
        stats = [(1 if d % 2 == 0 else -1) / 64 for d in range(24)]
        self.assertEqual(second["statistics"]["candidate_minus_stated"]["drop"]["estimate"],
                         math.fsum(stats) / 24)
        with self.assertRaisesRegex(readers.ReadError, "units without"):
            readers.run_read(decl, sha, "c", 2, _files(records[:n1]), _plans(plans[:n1]))
        with self.assertRaisesRegex(readers.ReadError, "beyond"):
            readers.run_read(decl, sha, "c", 1, _files(records), _plans(plans))
        with self.assertRaisesRegex(readers.ReadError, "look 3 is refused"):
            readers.run_read(decl, sha, "c", 3, _files(records), _plans(plans))

    def test_b_prime_reports_both_bounds_and_no_verdict(self):
        records, plans = _c_records(8, _planted(0.0, 1),
                                    halts=lambda d, s, slot, arm: arm == 1 and slot == 0 and s < 2)
        decl, sha = _declaration(**{"c.sizing.spread": Decimal("0.02")})
        report = readers.run_read(decl, sha, "b_prime", 1, _files(records), _plans(plans))
        self.assertIsNone(report["verdict"])
        block = report["statistics"]["stated_win_fraction"]
        self.assertLess(block["halts_against"]["estimate"], block["drop"]["estimate"])
        self.assertLess(block["drop"]["estimate"], block["halts_for"]["estimate"])
        self.assertEqual(report["bootstrap"]["root_seed"], deal.seed(["test", "bootstrap", "b_prime", 1]))

    def test_unit_with_every_game_halted(self):
        records, plans = _c_records(8, _planted(0.0, 1), halts=lambda d, s, slot, arm: d == 0)
        decl, sha = _declaration(**{"c.sizing.spread": Decimal("0.02")})
        report = readers.run_read(decl, sha, "c", 1, _files(records), _plans(plans))
        self.assertEqual(report["statistics"]["candidate_minus_stated"]["drop"]["units_scored"], 7)
        self.assertEqual(report["statistics"]["candidate_minus_stated"]["halts_against"]["units_scored"], 8)


class ReadBTests(unittest.TestCase):
    def _records(self, count, p_win, halt=None):
        records, plans = [], []
        for d in range(count):
            plan = synth.read_b_plan(d)

            def result_of(game, position, d=d):
                if halt is not None and halt(d, game["game"]):
                    return "halt"
                return "win" if synth.unit_draw("b", d, game["game"]) < p_win else "loss"

            records.append(synth.record_from_plan(plan, result_of))
            plans.append(plan)
        return records, plans

    def test_planted_win_and_loss(self):
        decl, sha = _declaration(**{"b.sizing.spread": Decimal("0.1"), "b.sizing.effect": Decimal("0.1")})
        n = readers.look_units(decl["reads"]["b"], 1)
        self.assertEqual(n, 8)
        for p, word in ((0.8, "win"), (0.2, "loss")):
            records, plans = self._records(n, p)
            report = readers.run_read(decl, sha, "b", 1, _files(records), _plans(plans))
            self.assertEqual(report["verdict"], word)
            self.assertEqual(report["thresholds"]["win_if_lower_above"], 0.5)

    def test_halt_signs(self):
        unit = [[1.0, None, 0.0, 1.0]] + [[0.5] * 4] * 7
        self.assertEqual(readers._stat_b(unit, "drop"), (2 / 3 + 0.5 * 7) / 8)
        self.assertEqual(readers._stat_b(unit, "halts_against"), (0.5 + 0.5 * 7) / 8)
        self.assertEqual(readers._stat_b(unit, "halts_for"), (0.75 + 0.5 * 7) / 8)
        decl, sha = _declaration(**{"b.sizing.spread": Decimal("0.1"), "b.sizing.effect": Decimal("0.1")})
        records, plans = self._records(8, 0.8, halt=lambda d, g: g % 4 != 0)
        report = readers.run_read(decl, sha, "b", 1, _files(records), _plans(plans))
        self.assertEqual(report["verdicts_by_scoring"]["drop"], "win")
        self.assertEqual(report["void_test"]["tested"], ["halts_against"])
        self.assertEqual(report["verdict"], "void")

    def test_record_from_another_read_is_refused(self):
        records, plans = _c_records(8, _planted(0.0, 1))
        decl, sha = _declaration(**{"b.sizing.spread": Decimal("0.1"), "b.sizing.effect": Decimal("0.1")})
        with self.assertRaisesRegex(readers.ReadError, "use 'paired_build'"):
            readers.run_read(decl, sha, "b", 1, _files(records), _plans(plans))


class DoseTests(unittest.TestCase):
    def test_dose_is_a_report_with_signed_bounds(self):
        records, plans = [], []
        for d in range(4):
            plan = synth.paired_plan(d, "dose")

            def result_of(game, position, d=d):
                arm = game["game"] % 2
                if game["game"] in (0, 1):
                    return "halt"
                return "win" if arm == 0 and synth.unit_draw("dose", d, game["game"]) < 0.7 else "draw"

            records.append(synth.record_from_plan(plan, result_of))
            plans.append(plan)
        decl, sha = _declaration()
        report = readers.run_read(decl, sha, "dose", 3, _files(records), _plans(plans))
        self.assertIsNone(report["verdict"])
        self.assertEqual(report["units"]["count"], 4)
        block = report["statistics"]["current_minus_initial"]
        self.assertLess(block["halts_against"]["estimate"], block["drop"]["estimate"])
        self.assertLess(block["drop"]["estimate"], block["halts_for"]["estimate"])
        self.assertEqual(report["bootstrap"]["root_seed"], deal.seed(["test", "bootstrap", "dose", 3]))


class ReadATests(unittest.TestCase):
    def _pairs(self, count, p_candidate, p_stated, halt=None):
        records = []
        for pair in range(count):
            def result_of(orientation, arm, g, position, pair=pair):
                if halt is not None and halt(pair, orientation, arm, g):
                    return "halt"
                p = p_candidate if arm == "candidate" else p_stated
                return "win" if synth.unit_draw("a", pair, orientation, arm, g) < p else "loss"
            records.extend(synth.read_a_pair(pair, result_of))
        return records

    def test_planted_asymmetry_is_detected(self):
        decl, sha = _declaration(**{"a.sizing.spread": Decimal("0.2")})
        n = readers.look_units(decl["reads"]["a"], 1)
        self.assertEqual(n, 13)
        report = readers.run_read(decl, sha, "a", 1, _files(self._pairs(n, 0.7, 0.5)), [])
        self.assertEqual(report["verdict"], "win")
        stats = report["statistics"]
        self.assertGreater(stats["candidate_contrast"]["drop"]["interval"][0], 0.0)
        self.assertLess(stats["stated_contrast"]["drop"]["interval"][0], 0.0)
        self.assertGreater(stats["stated_contrast"]["drop"]["interval"][1], 0.0)
        self.assertGreater(stats["contrast_difference"]["drop"]["estimate"], 0.2)
        # A null effect is not a win.
        report = readers.run_read(decl, sha, "a", 1, _files(self._pairs(n, 0.5, 0.5)), [])
        self.assertIn(report["verdict"], ("extend", "loss"))

    def test_difference_takes_opposite_imputations_in_its_two_arms(self):
        unit = {"candidate": [1.0, None], "stated": [0.0, None]}
        self.assertEqual(readers._stat_a_difference(unit, "drop"), 2.0)
        self.assertEqual(readers._stat_a_difference(unit, "halts_against"), 0.0)
        self.assertEqual(readers._stat_a_difference(unit, "halts_for"), 2.0)
        # Imputing both arms the same way would give 1.0 here and bound nothing.
        same = readers._contrast(unit["candidate"], "halts_against", 0.0) - readers._contrast(
            unit["stated"], "halts_against", 0.0)
        self.assertEqual(same, 1.0)
        self.assertEqual(readers._stat_a_candidate(unit, "halts_against"), 0.0)
        self.assertEqual(readers._stat_a_candidate(unit, "halts_for"), 1.0)

    def test_early_pod_read_uses_its_own_section(self):
        decl, sha = _declaration(**{"a_early.sizing.spread": Decimal("0.2"), "a_early.run": "synth"})
        report = readers.run_read(decl, sha, "a_early", 1, _files(self._pairs(13, 0.7, 0.5)), [])
        self.assertEqual(report["read"], "a_early")
        self.assertEqual(report["bootstrap"]["root_seed"], deal.seed(["test", "bootstrap", "a_early", 1]))

    def test_pair_shape_is_checked(self):
        records = self._pairs(1, 0.5, 0.5)
        decl, sha = _declaration(**{"aa.pairs": 1})
        with self.assertRaisesRegex(readers.ReadError, "exactly 2"):
            readers.run_read(decl, sha, "aa", 1, _files(records[:1]), [])
        bad = copy.deepcopy(records)
        bad[1]["chairs"][1]["role"] = "fixture"
        bad[1]["chairs"][0]["role"] = "learning"
        with self.assertRaisesRegex(readers.ReadError, "even chairs"):
            readers.run_read(decl, sha, "aa", 1, _files(bad), [])


class AATests(unittest.TestCase):
    """Read (a)'s A/A: identical policies on role-swapped pairs, exact symmetry on a few pairs."""

    @staticmethod
    def _symmetric(pair, orientation, arm, g, position):
        # Identical policies: the game depends on the pairing, builder and seats, not on the roles.
        # `position` is the learned seat's session seat; the result is fixed for session seat 0.
        r = synth.unit_draw("aa", pair, arm, g)
        p0 = "halt" if r < 0.05 else ("draw" if r < 0.15 else ("win" if r < 0.6 else "loss"))
        if p0 in ("halt", "draw") or position == 0:
            return p0
        return "loss" if p0 == "win" else "win"

    def _records(self, pairs=4):
        records = []
        for pair in range(pairs):
            records.extend(synth.read_a_pair(pair, lambda o, a, g, pos, pair=pair: self._symmetric(pair, o, a, g, pos)))
        return records

    def test_exact_symmetry_on_a_few_pairs(self):
        records = self._records()
        decl, sha = _declaration()
        report = readers.run_read(decl, sha, "aa", 1, _files(records), [])
        self.assertTrue(report["symmetry"]["symmetric"], report["symmetry"])
        self.assertEqual(report["symmetry"]["games_checked"], 4 * 32)
        self.assertEqual(report["verdict"], "symmetric")
        self.assertGreater(report["games"]["halted"], 0)
        for name in ("candidate_contrast", "stated_contrast", "contrast_difference"):
            drop = report["statistics"][name]["drop"]
            self.assertEqual(drop["estimate"], 0.0)
            self.assertEqual(drop["interval"], [0.0, 0.0])
        self.assertLess(report["statistics"]["candidate_contrast"]["halts_against"]["estimate"], 0.0)

    def test_any_asymmetry_is_reported(self):
        for case in ("score", "cards", "halt"):
            records = self._records()
            odd = next(r for r in records if r["pod_index"] == 2 and r["chairs"][1]["role"] == "learning")
            game = next(g for g in odd["games"] if g["scores"] not in (None, [0, 0]))
            if case == "score":
                game["scores"] = [-s for s in game["scores"]]
                end = game["attempts"][0]["end"]
                end["winner_seat"] = 1 - end["winner_seat"]
            elif case == "cards":
                odd["decks"][game["decks"][0]]["cards"][0] = 7
                odd["decks"][game["decks"][0]]["cards"].sort()
            else:
                attempt = game["attempts"][0]
                attempt["end"] = {"kind": "panic", "message": "synthetic"}
                attempt["halted"] = True
                attempt["stop_kind"] = "panic"
                game["scores"] = None
                odd["halts"] = []
                for g in odd["games"]:
                    for a in g["attempts"]:
                        if a["halted"]:
                            odd["halts"].append({"game": g["game"], "attempt": 0, "stop_kind": a["stop_kind"],
                                                 "last_action_source": None, "deal_seed": a["deal_seed"]})
            pod_record.validate_record(odd)
            decl, sha = _declaration()
            report = readers.run_read(decl, sha, "aa", 1, _files(records), [])
            with self.subTest(case=case):
                self.assertFalse(report["symmetry"]["symmetric"])
                self.assertEqual(report["verdict"], "asymmetric")
                # A changed card changes every game of that deck (4 cross games); the others one game.
                self.assertEqual(report["symmetry"]["mismatch_count"], 4 if case == "cards" else 1)


class DeterminismTests(unittest.TestCase):
    def test_reports_are_byte_identical(self):
        records, plans = _c_records(8, _planted(0.0, 1), halts=lambda d, s, slot, arm: (d, s, slot) == (2, 3, 1))
        decl, sha = _declaration(**{"c.sizing.spread": Decimal("0.02")})
        first = readers.serialize_report(readers.run_read(decl, sha, "c", 1, _files(records), _plans(plans)))
        shuffled = list(reversed(records))
        second = readers.serialize_report(
            readers.run_read(decl, sha, "c", 1, _files(shuffled), _plans(list(reversed(plans)))))
        self.assertEqual(first, second)
        # A pinned golden: any change to the seed rule, the unit order, the arithmetic or the report
        # layout changes these bytes, and needs a declared new version.
        self.assertEqual(hashlib.sha256(first.encode()).hexdigest(), REPORT_GOLDEN_SHA256)

    def test_cli_writes_identical_bytes(self):
        records, plans = _c_records(8, _planted(0.0, 1))
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            decl_doc = json.loads(TEST_DECLARATION.read_text(encoding="utf-8"))
            decl_doc["reads"]["c"]["sizing"]["spread"] = 0.02
            decl_path = tmp / "decl.json"
            decl_path.write_text(json.dumps(decl_doc), encoding="utf-8")
            (tmp / "pods.json").write_text(json.dumps(records), encoding="utf-8")
            plan_paths = []
            for i, plan in enumerate(plans):
                path = tmp / f"plan{i}.json"
                path.write_text(read_plans.serialize_plan(plan), encoding="utf-8")
                plan_paths.append(str(path))
            outs = []
            for k in range(2):
                out = tmp / f"report{k}.json"
                args = ["--declaration", str(decl_path), "--read", "c", "--look", "1", "--records",
                        str(tmp / "pods.json"), "--plans", *(plan_paths if k == 0 else plan_paths[::-1]),
                        "--out", str(out)]
                self.assertEqual(readers.main(args), 0)
                outs.append(out.read_bytes())
            self.assertEqual(outs[0], outs[1])
            report = json.loads(outs[0])
            self.assertEqual(report["declaration_sha256"], hashlib.sha256(decl_path.read_bytes()).hexdigest())
            self.assertEqual(report["inputs"]["records"],
                             [hashlib.sha256((tmp / "pods.json").read_bytes()).hexdigest()])


REPORT_GOLDEN_SHA256 = "2a74cded447c82d5cbb8aa890e0bb8d7c7ab64b59c967c3d1e97f4196c46d8bb"


class UniformPodRecordTests(unittest.TestCase):
    @unittest.skipUnless(D5_UNIFORM_FIXTURE.exists(),
                         "D5 has not yet produced a uniform pod record; its fixture is added when D5 lands")
    def test_parses_a_uniform_pod_record_from_d5(self):
        records, _ = pod_record.load_records(D5_UNIFORM_FIXTURE)
        for record in records:
            self.assertEqual(record["pilot"]["kind"], "uniform")


if __name__ == "__main__":
    unittest.main()
