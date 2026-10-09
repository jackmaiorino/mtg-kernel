#!/usr/bin/env python3
"""Unit tests for the stage 2-3 corpus and analysis tools, on synthetic rows.

Run: python3 -m unittest discover -s tools/t1_diagnosis -p 'test_*.py'
"""

import contextlib
import importlib.util
import io
import json
import os
import random
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))


def load(name):
    spec = importlib.util.spec_from_file_location(name, os.path.join(HERE, f"{name}.py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


freeze_roots = load("freeze_roots")
analyze_search = load("analyze_search")
analyze_cross = load("analyze_cross")
compare_rows = load("compare_rows")

SPY_TARGET = json.dumps({"action_kind": "choose_target", "remaining": 1, "source": "Balustrade Spy",
                         "target": {"player": "p0", "target_kind": "player"}}, separators=(",", ":"))
DR_TARGET = json.dumps({"action_kind": "choose_target", "remaining": 1, "source": "Dread Return",
                        "target": {"object": "Lotleth Giant", "target_kind": "object"}}, separators=(",", ":"))
SPY_CAST = json.dumps({"action_kind": "cast_spell", "source": "Balustrade Spy"}, separators=(",", ":"))
OTHER = json.dumps({"action_kind": "cast_spell", "source": "Quirion Ranger"}, separators=(",", ":"))


def root(game, step, stratum="own_main|action", deck="Spy", tags=(), menu=None, score=1.0):
    return {"kind": "root", "game": game, "step": step, "stratum": stratum, "deck": deck,
            "opp_deck": f"opp{game % 3}", "tags": list(tags), "menu": menu, "focal_score": score}


def args(**kw):
    a = freeze_roots.parser().parse_args(["rows.jsonl", "--out-dir", "unused"])
    for k, v in kw.items():
        setattr(a, k, v)
    return a


class FreezeRootsTest(unittest.TestCase):
    def corpus(self):
        rows = []
        for g in range(60):
            deck = ["Spy", "CawGates", "Elves"][g % 3]
            for s, stratum in enumerate(["own_main|action", "opp_turn|other", "own_combat|combat"]):
                rows.append(root(g, s, stratum, deck, score=float(g % 2)))
        return rows

    def test_deck_filter_and_declared_strata(self):
        rows = self.corpus()
        rep, _, fields = freeze_roots.freeze(rows, args(deck="Elves", per_stratum=4, mech="none:0"))
        self.assertTrue(rep and all(r["deck"] == "Elves" for r in rep))
        self.assertEqual(len(rep), 12)
        rep, _, fields = freeze_roots.freeze(
            rows, args(strata="opp_turn|other,never|seen", per_stratum=4, mech="none:0"))
        self.assertEqual(fields["strata_selected"], ["opp_turn|other", "never|seen"])
        self.assertEqual({r["stratum"] for r in rep}, {"opp_turn|other"})
        self.assertEqual(len(rep), 4)

    def test_quota_below_minimum_is_redistributed(self):
        rows = []
        for g in range(80):
            tag = "cawgates_a" if g < 30 else "cawgates_b" if g < 60 else "cawgates_c" if g < 64 else None
            if tag:
                rows.append(root(g, 1, deck="CawGates", tags=[tag], score=float(g % 2)))
        _, mech, fields = freeze_roots.freeze(rows, args(per_stratum=0, mech="cawgates:9:CawGates"))
        group = fields["mechanism"]["cawgates"]
        alloc = group["allocation"]
        self.assertEqual(alloc["base_quota"], {"cawgates_a": 3, "cawgates_b": 3, "cawgates_c": 3})
        self.assertEqual(alloc["final_quota"], {"cawgates_a": 5, "cawgates_b": 4, "cawgates_c": 0})
        self.assertEqual(alloc["below_minimum"], {"cawgates_c": 4})
        self.assertEqual(alloc["redistributed"][0]["to"], {"cawgates_a": 2, "cawgates_b": 1})
        self.assertEqual(group["selected_by_subtag"], {"cawgates_a": 5, "cawgates_b": 4, "cawgates_c": 0})
        self.assertEqual(len(mech), 9)
        # With the minimum lowered, the even split is the earlier behaviour.
        _, mech, fields = freeze_roots.freeze(rows, args(per_stratum=0, mech="cawgates:9:CawGates", mech_min=0))
        self.assertEqual(fields["mechanism"]["cawgates"]["selected_by_subtag"],
                         {"cawgates_a": 3, "cawgates_b": 3, "cawgates_c": 3})

    def test_allocate_edge_cases(self):
        final, rec = freeze_roots.allocate(7, {"x": 50, "y": 50}, 10)
        self.assertEqual(final, {"x": 4, "y": 3})
        self.assertEqual(rec["redistributed"], [])
        final, rec = freeze_roots.allocate(6, {"x": 3, "y": 2}, 10)
        self.assertEqual(final, {"x": 3, "y": 3})
        self.assertTrue(rec["unredistributed"])

    def test_spy_quota_split_by_decision_type(self):
        rows = []
        menus = [[SPY_TARGET], [DR_TARGET], [SPY_CAST, OTHER], [OTHER]]
        for g in range(80):
            rows.append(root(g, 1, tags=["spy"], menu=menus[g % 4], score=float(g % 2)))
        self.assertEqual([freeze_roots.spy_decision_type(r) for r in rows[:4]],
                         ["spy_target", "spy_dr_target", "spy_cast", "spy_other"])
        truncated = root(0, 1, tags=["spy"], menu=[SPY_TARGET[:-20]])
        self.assertEqual(freeze_roots.spy_decision_type(truncated), "spy_target")
        _, mech, fields = freeze_roots.freeze(rows, args(per_stratum=0, mech="spy:8:Spy", spy_types=True))
        self.assertEqual(fields["mechanism"]["spy"]["split_by"], "decision_type")
        self.assertEqual(fields["mechanism"]["spy"]["selected_by_subtag"],
                         {t: 2 for t in freeze_roots.SPY_TYPES})
        self.assertEqual({r["mechanism_tag"] for r in mech}, set(freeze_roots.SPY_TYPES))
        _, mech, _ = freeze_roots.freeze(rows, args(per_stratum=0, mech="spy:8:Spy"))
        self.assertEqual({r["mechanism_tag"] for r in mech}, {"spy"})

    STRANDS = "strands=cawgates_strands_opp_combat>cawgates_strands_castable_own_turn:6:CawGates"

    def strands_rows(self, n_opp, n_own):
        rows = []
        for g in range(n_opp):
            rows.append(root(g, 1, deck="CawGates", tags=["cawgates_strands_opp_combat"], score=float(g % 2)))
        for g in range(100, 100 + n_own):
            rows.append(root(g, 2, deck="CawGates", tags=["cawgates_strands_castable_own_turn"],
                             score=float(g % 2)))
        return rows

    def test_strands_priority_fill_uses_opponent_combat_rows_first(self):
        _, mech, fields = freeze_roots.freeze(self.strands_rows(12, 30), args(per_stratum=0, mech=self.STRANDS))
        group = fields["mechanism"]["strands"]
        self.assertEqual(group["priority"]["used"], "cawgates_strands_opp_combat")
        self.assertFalse(group["priority"]["first_not_probed"])
        self.assertEqual(group["priority"]["pools"],
                         {"cawgates_strands_opp_combat": 12, "cawgates_strands_castable_own_turn": 30})
        self.assertEqual(len(mech), 6)
        self.assertEqual({r["mechanism_tag"] for r in mech}, {"cawgates_strands_opp_combat"})
        self.assertEqual({r["set"] for r in mech}, {"mechanism:strands"})

    def test_strands_priority_fill_falls_back_below_ten_opponent_combat_rows(self):
        _, mech, fields = freeze_roots.freeze(self.strands_rows(9, 30), args(per_stratum=0, mech=self.STRANDS))
        group = fields["mechanism"]["strands"]
        self.assertEqual(group["priority"]["used"], "cawgates_strands_castable_own_turn")
        self.assertTrue(group["priority"]["first_not_probed"])
        self.assertFalse(group["priority"]["none_reached_minimum"])
        self.assertEqual({r["mechanism_tag"] for r in mech}, {"cawgates_strands_castable_own_turn"})
        self.assertEqual(len(mech), 6)
        # Neither pool reaches the minimum: the last prefix of the chain is used.
        _, mech, fields = freeze_roots.freeze(self.strands_rows(3, 5), args(per_stratum=0, mech=self.STRANDS))
        self.assertTrue(fields["mechanism"]["strands"]["priority"]["none_reached_minimum"])
        self.assertEqual({r["mechanism_tag"] for r in mech}, {"cawgates_strands_castable_own_turn"})
        # Without a name the group is named by the first prefix.
        spec = "cawgates_strands_opp_combat>cawgates_strands_castable_own_turn:6:CawGates"
        _, _, fields = freeze_roots.freeze(self.strands_rows(12, 30), args(per_stratum=0, mech=spec))
        self.assertIn("cawgates_strands_opp_combat", fields["mechanism"])

    def test_manifest_records_corpus_win_loss_per_stratum(self):
        rows = self.corpus()
        rows.append(root(99, 9, "own_main|action", "Spy", score=0.5))
        _, _, fields = freeze_roots.freeze(rows, args(per_stratum=4, mech="none:0"))
        self.assertEqual(fields["strata_outcomes"]["own_main|action"], {"won": 30, "lost": 30, "other": 1})
        self.assertEqual(fields["strata_outcomes"]["opp_turn|other"], {"won": 30, "lost": 30, "other": 0})
        _, _, fields = freeze_roots.freeze(rows, args(deck="Elves", per_stratum=4, mech="none:0"))
        self.assertEqual(fields["strata_outcomes"]["own_main|action"], {"won": 10, "lost": 10, "other": 0})


IMP = "improved:decisions:3"


def cond_row(game, index, scores, coverage_differs=True, k=6, config=None):
    """A minimal cond row: `scores` maps eval labels to per-playout scores."""
    jobs, job_of = [], {}
    for label, sc in scores.items():
        follow = IMP if (label[0] in "CD" and not label.endswith("/plain")) or label == "ref_improved" else "t1"
        job_of[label] = len(jobs)
        jobs.append({"action": len(jobs), "follow": follow, "scores": sc, "natural": [True] * len(sc),
                     "non_natural": 0, "transitions": [10] * len(sc), "transitions_total": 10 * len(sc),
                     "wall": [0.0] * len(sc), "inner_failures": 0, "searched_decisions": 0,
                     "cap_hits": 1 if follow == IMP else 0, "deviations": []})
    levels = [{"budget": 100, "transitions": 120, "wall": 0.0, "rounds": 3, "exhausted": False}]
    return {"kind": "cond", "root_index": index, "root_id": f"r{index}", "game": game, "k": k,
            "coverage_differs": coverage_differs, "eval_jobs": job_of, "budgets": [100],
            "eval": {"jobs": jobs, "playouts": 2, "actual_transitions": 0},
            "selection": {c: {"levels": levels} for c in "ABCD"},
            "config": config or {"horizon": "decisions:3", "budgets": [100]}}


class AnalyzeSearchTest(unittest.TestCase):
    def test_manifest_rejects_differing_and_missing_config(self):
        good = cond_row(1, 0, {"ref": [1.0]})
        bad = cond_row(2, 1, {"ref": [1.0]}, config={"horizon": "turn", "budgets": [100]})
        missing = cond_row(3, 2, {"ref": [1.0]})
        del missing["config"]
        accepted, rejected = analyze_search.check_manifest(
            [good, bad, missing], {"config": {"horizon": "decisions:3", "budgets": [100]}})
        self.assertEqual(accepted, [good])
        self.assertEqual(len(rejected), 2)
        self.assertIn("horizon", rejected[0][1][0])
        self.assertEqual(rejected[1][1], ["no recorded config"])

    def test_holm(self):
        adj = analyze_search.holm({"a": 0.01, "b": 0.04, "c": 0.03, "d": 0.5})
        for k, v in {"a": 0.04, "c": 0.09, "b": 0.09, "d": 0.5}.items():
            self.assertAlmostEqual(adj[k], v)

    def test_bootstrap_resamples_whole_games(self):
        rows = [{"game": 1}] * 3 + [{"game": 2}]
        boot = analyze_search.ClusterBoot(rows, 500, random.Random(3))
        stats = set(boot.stats([1.0, 1.0, 1.0, 0.0]))
        self.assertTrue(stats <= {0.0, 0.75, 1.0}, stats)
        self.assertEqual(stats, {0.0, 0.75, 1.0})

    def test_summary_family_interaction_coverage_and_two_by_two(self):
        rows = []
        for i in range(12):
            scores = {"ref": [0.0, 0.0], "A@100": [0.0, 0.0], "B@100": [0.0, 1.0],
                      "C@100": [1.0, 0.0], "D@100": [1.0, 1.0], "ref_improved": [1.0, 0.0],
                      "C@100/plain": [0.0, 0.0], "D@100/plain": [0.0, 1.0]}
            rows.append(cond_row(i // 2, i, scores, coverage_differs=i % 3 != 0, k=6 if i % 3 else 3))
        out = {}
        with contextlib.redirect_stdout(io.StringIO()):
            analyze_search.summarize("t", rows, [100], 300, 7, out)
        e = out["t"]
        self.assertEqual(set(e["holm"]), {"B-A@100", "C-A@100", "D-B@100", "D-C@100"})
        self.assertAlmostEqual(e["contrasts"]["D-A@100"]["diff"], 1.0)
        self.assertAlmostEqual(e["interaction"]["(D-C)-(B-A)@100"]["diff"], 0.0)
        self.assertEqual(e["b_equals_a"]["roots"], 4)
        self.assertEqual(e["b_equals_a"]["k_le_K"], 4)
        self.assertEqual(e["coverage_differs_contrasts"]["B-A@100"]["n"], 8)
        t = e["two_by_two"]["C@100"]
        self.assertAlmostEqual(t["contrasts"]["continuation_model_action"]["diff"], 0.5)
        self.assertAlmostEqual(t["contrasts"]["continuation_search_action"]["diff"], 0.5)
        self.assertAlmostEqual(t["contrasts"]["interaction"]["diff"], 0.0)
        self.assertEqual(e["non_natural"]["ref"], 0)

    def test_cost_curves_add_evaluation_transitions(self):
        rows = [cond_row(0, 0, {"ref": [0.0, 1.0], "A@100": [1.0, 1.0], "B@100": [1.0, 1.0],
                                "C@100": [1.0, 1.0], "D@100": [1.0, 1.0]})]
        out = {}
        with contextlib.redirect_stdout(io.StringIO()):
            analyze_search.cost_curves(rows, [100], out)
        self.assertEqual(out["curves"]["C@100"]["sel_plus_eval_transitions"], 140)
        self.assertEqual(out["curves"]["ref"]["sel_plus_eval_transitions"], 20)


def cross_row(deviations, cap_hits=(0, 0, 2, 0, 0, 1)):
    cells = ["t1|t1", "t1|improved:turn", "t1|improved:turn+12", "alt|t1", "alt|improved:turn",
             "alt|improved:turn+12"]
    jobs = [{"scores": [1.0, 0.0], "cap_hits": cap_hits[i], "deviations": deviations if i == 2 else []}
            for i in range(6)]
    return {"kind": "cross", "cells": cells, "eval": {"jobs": jobs}}


class AnalyzeCrossTest(unittest.TestCase):
    def test_long_arm_and_cap_rates(self):
        row = cross_row([])
        self.assertEqual(analyze_cross.long_arm(row["cells"]), "improved:turn+12")
        self.assertEqual(analyze_cross.long_arm(["t1|t1", "t1|improved:turn", "t1|improved:game"]),
                         "improved:game")
        caps = analyze_cross.cap_rates([row, row], row["cells"])
        self.assertEqual(caps["t1|improved:turn+12"], {"cap_hits": 4, "playouts": 4, "rate": 1.0})
        self.assertEqual(caps["t1|t1"]["rate"], 0.0)

    def test_counterfactual_cost_and_harmful_failures(self):
        attack = {"sampled_sem": json.dumps({"action_kind": "choose_attacker_inclusion"}),
                  "chosen_sem": json.dumps({"action_kind": "choose_attacker_inclusion"})}

        def cf(sampled_wins, chosen_wins, n=4):
            return {"playouts": n, "sampled_natural_wins": sampled_wins, "chosen_natural_wins": chosen_wins,
                    "sampled_mean": sampled_wins / n, "chosen_mean": chosen_wins / n,
                    "harmful": chosen_wins < sampled_wins, "transitions": 50}

        devs = [dict(attack, playout=0, ordinal=1, counterfactual=cf(1, 3)),
                dict(attack, playout=1, ordinal=1, counterfactual=cf(2, 1)),
                dict(attack, playout=1, ordinal=2, counterfactual=cf(2, 2)), dict(attack, playout=0)]
        res = analyze_cross.counterfactual_costs([cross_row(devs)])
        self.assertEqual((res["logged"], res["evaluated"], res["transitions"]), (4, 3, 150))
        e = res["classes"]["attacks"]
        self.assertEqual((e["evaluated"], e["helpful"], e["harmful"], e["neutral"]), (3, 1, 1, 1))
        self.assertAlmostEqual(e["cost_win"], (0.5 - 0.25 + 0.0) / 3)
        self.assertEqual(res["classes"]["other"]["evaluated"], 0)

    def test_nested_arm_counterfactuals_are_counted_once(self):
        # Through the root's turn the turn and turn+G arms of one root action
        # share state and decision seeds, so the in-turn deviation (ordinal 1,
        # cost +1.0) is logged in both; the beyond-turn one (ordinal 4, cost
        # -1.0) only in the long arm. Counted once each: cost 0, 100
        # transitions (not +0.333 and 150).
        attack = {"sampled_sem": json.dumps({"action_kind": "choose_attacker_inclusion"}),
                  "chosen_sem": json.dumps({"action_kind": "choose_attacker_inclusion"})}

        def cf(sampled_wins, chosen_wins, n=4):
            return {"playouts": n, "sampled_natural_wins": sampled_wins, "chosen_natural_wins": chosen_wins,
                    "sampled_mean": sampled_wins / n, "chosen_mean": chosen_wins / n,
                    "harmful": chosen_wins < sampled_wins, "transitions": 50}

        in_turn = dict(attack, playout=0, ordinal=1, counterfactual=cf(0, 4))
        beyond = dict(attack, playout=0, ordinal=4, counterfactual=cf(4, 0))
        row = cross_row([])
        row["root_id"] = "g1s2"
        row["eval"]["jobs"][1]["deviations"] = [dict(in_turn)]
        row["eval"]["jobs"][2]["deviations"] = [dict(in_turn), dict(beyond)]
        res = analyze_cross.counterfactual_costs([row])
        e = res["classes"]["attacks"]
        self.assertEqual((res["logged"], res["evaluated"], res["duplicates_skipped"]), (3, 2, 1))
        self.assertEqual(res["transitions"], 100)
        self.assertAlmostEqual(e["cost_win"], 0.0)
        self.assertEqual((e["helpful"], e["harmful"], e["neutral"]), (1, 1, 0))
        # The other root action, another playout or another root are distinct.
        row["eval"]["jobs"][4]["deviations"] = [dict(in_turn)]
        row["eval"]["jobs"][5]["deviations"] = [dict(in_turn, playout=1)]
        other = json.loads(json.dumps(row))
        other["root_id"] = "g1s9"
        res = analyze_cross.counterfactual_costs([row, other])
        self.assertEqual((res["evaluated"], res["duplicates_skipped"], res["transitions"]), (8, 2, 400))

    def test_harmful_is_decided_on_natural_wins(self):
        dev = {"sampled_sem": "{}", "chosen_sem": "{}", "playout": 0, "ordinal": 1,
               # Higher mean score (draws) but fewer natural wins: harmful.
               "counterfactual": {"playouts": 4, "sampled_natural_wins": 2, "chosen_natural_wins": 1,
                                  "sampled_mean": 0.5, "chosen_mean": 0.75, "harmful": False, "transitions": 1}}
        res = analyze_cross.counterfactual_costs([cross_row([dev])])
        self.assertEqual(res["classes"]["other"]["harmful"], 1)
        self.assertAlmostEqual(res["classes"]["other"]["cost_win"], -0.25)


CELLS = ["t1|t1", "t1|improved:turn", "t1|improved:turn+4", "alt|t1", "alt|improved:turn", "alt|improved:turn+4"]


def full_cross_row(i, game, wins, forced=False, searched=(0, 2, 4, 0, 2, 4), caps=(0, 0, 0, 0, 0, 0),
                   order=None, config=None, n=4):
    """A cross row whose cell c wins `wins[c]` of n playouts; `order` permutes the cells."""
    order = order or list(range(6))
    cells = [CELLS[j] for j in order]
    jobs = [{"scores": [1.0] * wins[CELLS[j]] + [0.0] * (n - wins[CELLS[j]]), "cap_hits": caps[j],
             "searched_decisions": searched[j], "deviations": [], "transitions": [10] * n, "wall": [0.0] * n,
             "inner_failures": 0} for j in order]
    return {"kind": "cross", "root_id": f"r{i}", "root_index": i, "game": game, "turn": 3, "cells": cells,
            "alt_forced": forced, "selection_wall": 0.0,
            "eval": {"jobs": jobs, "actual_wall": 0.0, "failed_playouts": 0},
            "config": config or {"mode": "cross", "horizon_long": "turn+4"}}


class AnalyzeCrossStage3Test(unittest.TestCase):
    def wins(self, **kw):
        w = {c: 0 for c in CELLS}
        w.update({k.replace("__", "|").replace("_turn4", ":turn+4").replace("_turn", ":turn"): v
                  for k, v in kw.items()})
        return w

    def test_cells_are_looked_up_by_name_in_each_row(self):
        w = {"t1|t1": 0, "t1|improved:turn": 1, "t1|improved:turn+4": 2, "alt|t1": 3, "alt|improved:turn": 4,
             "alt|improved:turn+4": 4}
        a = full_cross_row(0, 1, w)
        b = full_cross_row(1, 2, w, order=[5, 4, 3, 2, 1, 0])
        self.assertEqual(analyze_cross.cell_wins(a), analyze_cross.cell_wins(b))
        res, _, _ = analyze_cross.analyze([a, b], 200, 7)
        self.assertAlmostEqual(res["cells"]["alt|t1"], 0.75)
        self.assertAlmostEqual(res["reading_contrasts"]["beyond_turn"]["diff"], 0.25)

    def test_bootstrap_clusters_by_corpus_game(self):
        w_hi = {c: 4 if c == "t1|improved:turn+4" else 0 for c in CELLS}
        w_lo = {c: 0 for c in CELLS}
        rows = [full_cross_row(0, 1, w_hi), full_cross_row(1, 1, w_hi), full_cross_row(2, 1, w_hi),
                full_cross_row(3, 2, w_lo)]
        res, _, _ = analyze_cross.analyze(rows, 500, 3)
        self.assertEqual(res["games"], 2)
        boot = analyze_cross.ClusterBoot(rows, 500, random.Random(3))
        stats = set(boot.stats([1.0, 1.0, 1.0, 0.0]))
        self.assertEqual(stats, {0.0, 0.75, 1.0})

    def test_adequacy_holm_reading_and_primary(self):
        # 30 roots in 30 games: the long arm adds a win at every root, the
        # turn arm and the alternative add nothing.
        w = {c: 0 for c in CELLS}
        w["t1|improved:turn+4"] = 1
        w["alt|improved:turn+4"] = 1
        rows = [full_cross_row(i, i, w) for i in range(30)]
        res, _, _ = analyze_cross.analyze(rows, 500, 7)
        a = res["adequacy"]
        self.assertTrue(a["a_cap_hit_rate"]["passed"] and a["b_beyond_turn"]["passed"])
        self.assertTrue(a["c_turn_continuation"]["passed"] and a["met"])
        self.assertEqual(res["primary"]["disposition"], analyze_search.HELPS)
        self.assertTrue(res["reading_contrasts"]["beyond_turn"]["helps"])
        self.assertEqual(res["reading"]["row"], "beyond_turn")
        # The other three are exactly 0: evidence against with adequacy met.
        for k in ("isolated_correction", "within_turn", "alt_turn"):
            self.assertEqual(res["reading_contrasts"][k]["disposition"], analyze_search.AGAINST, k)
        # The cap binds in a third of long-arm playouts and the long arm
        # searches nothing beyond the turn: adequacy fails, so no evidence against.
        rows = [full_cross_row(i, i, w, caps=(0, 0, 2, 0, 0, 0), searched=(0, 2, 2, 0, 2, 2)) for i in range(30)]
        res, _, _ = analyze_cross.analyze(rows, 500, 7)
        a = res["adequacy"]
        self.assertFalse(a["a_cap_hit_rate"]["passed"])
        self.assertAlmostEqual(a["a_cap_hit_rate"]["rate"], 0.5)
        self.assertFalse(a["b_beyond_turn"]["passed"])
        self.assertFalse(a["met"])
        self.assertEqual(res["reading_contrasts"]["within_turn"]["disposition"], analyze_search.INCONCLUSIVE)

    def test_isolated_correction_uses_unforced_roots(self):
        w = {c: 0 for c in CELLS}
        w["alt|t1"] = 2
        rows = [full_cross_row(i, i, w, forced=i < 10) for i in range(30)]
        w_forced = dict(w, **{"alt|t1": 0})
        rows[:10] = [full_cross_row(i, i, w_forced, forced=True) for i in range(10)]
        res, _, _ = analyze_cross.analyze(rows, 500, 7)
        iso = res["reading_contrasts"]["isolated_correction"]
        self.assertEqual(iso["n"], 20)
        self.assertAlmostEqual(iso["diff"], 0.5)
        self.assertAlmostEqual(res["descriptive"]["isolated_correction_all_roots"]["diff"], 1 / 3)
        self.assertEqual(res["reading"]["row"], "isolated_correction")

    def test_reading_table_first_match_wins(self):
        def h(iso=False, within=False, beyond=False, alt=False):
            return analyze_cross.read_table({"isolated_correction": iso, "within_turn": within,
                                             "beyond_turn": beyond, "alt_turn": alt})[0]
        self.assertEqual(h(iso=True, within=True), "isolated_correction")
        self.assertEqual(h(iso=True, beyond=True), "beyond_turn")
        self.assertEqual(h(within=True, alt=True), "within_turn")
        self.assertEqual(h(within=True, beyond=True), "beyond_turn")
        self.assertEqual(h(alt=True), "alt_turn")
        self.assertEqual(h(iso=True, alt=True), "isolated_correction")
        self.assertIsNone(h())

    def test_main_rejects_rows_whose_config_differs(self):
        w = {c: 1 for c in CELLS}
        good = [full_cross_row(i, i, w) for i in range(4)]
        bad = full_cross_row(9, 9, w, config={"mode": "cross", "horizon_long": "turn+8"})
        bad["cells"] = [c.replace("turn+4", "turn+8") for c in bad["cells"]]
        with tempfile.TemporaryDirectory() as d:
            rows_path, man, out = (os.path.join(d, x) for x in ("cross.jsonl", "m.json", "out.json"))
            with open(rows_path, "w") as f:
                for r in good + [bad]:
                    f.write(json.dumps(r) + "\n")
            with open(man, "w") as f:
                json.dump({"config": {"mode": "cross", "horizon_long": "turn+4"}}, f)
            with contextlib.redirect_stdout(io.StringIO()):
                analyze_cross_main([rows_path, "--expect", man, "--json", out, "--boot", "50"])
            with open(out) as f:
                res = json.load(f)
        self.assertEqual((res["rejected"], res["roots"], res["long_follow"]), (1, 4, "improved:turn+4"))


def analyze_cross_main(argv):
    import sys
    old = sys.argv
    sys.argv = ["analyze_cross.py"] + argv
    try:
        analyze_cross.main()
    finally:
        sys.argv = old


BUDGETS = [100, 200, 400]


def adequate_row(i, game, wins, spy=None, rounds=10, means=(0.2, 0.6, 0.4, 0.4), chosen=(1, 1),
                 stratum="own_main|action", score=1.0, n=4):
    """A cond row with levels at 100/200/400: label L wins `wins[L]` of n
    evaluation playouts (missing labels win 0); D's means at 400 and D's
    choice at 200 and 400 are given; `spy` = D's per-candidate Spy counts at 400."""
    labels = ["ref", "ref_improved"] + [f"{c}@{b}" for c in "ABCD" for b in BUDGETS]
    jobs, job_of = [], {}
    for j, label in enumerate(labels):
        follow = IMP if label[0] in "CD" or label == "ref_improved" else "t1"
        sc = [1.0] * wins.get(label, 0) + [0.0] * (n - wins.get(label, 0))
        job_of[label] = j
        jobs.append({"action": j, "follow": follow, "scores": sc, "natural": [True] * n, "non_natural": 0,
                     "transitions": [10] * n, "transitions_total": 10 * n, "wall": [0.0] * n,
                     "inner_failures": 0, "searched_decisions": 0, "deviations": []})
    selection = {}
    for c in "ABCD":
        levels = []
        for b in BUDGETS:
            lv = {"budget": b, "transitions": b + 5, "wall": 0.0, "rounds": rounds, "exhausted": False,
                  "means": [0.5, 0.5, 0.5, 0.5], "chosen": 0}
            if c == "D" and b == 400:
                lv["means"], lv["chosen"] = list(means), chosen[1]
                if spy is not None:
                    lv["spy"] = spy
            if c == "D" and b == 200:
                lv["chosen"] = chosen[0]
            levels.append(lv)
        selection[c] = {"levels": levels, "failed_rounds": 0, "inner_failures": 0}
    return {"kind": "cond", "root_index": i, "root_id": f"r{i}", "game": game, "k": 6, "coverage_differs": True,
            "set": "representative", "stratum": stratum, "focal_score": score, "budgets": BUDGETS,
            "horizon": "decisions:3", "m_inner": 1, "eval_jobs": job_of, "spy_labels": spy is not None,
            "eval": {"jobs": jobs, "playouts": n, "actual_transitions": 0, "actual_wall": 0.0,
                     "failed_playouts": 0, "discarded_dets": 0},
            "selection": selection, "selection_actual": {"transitions": 0, "wall": 0.0}, "replay_secs": 0.0,
            "root_wall": 0.0, "config": {"horizon": "decisions:3"}}


def spy_counts(offered, joint):
    return {"self_offered": [1 if offered else 0, 0], "self_chosen": [1 if joint else 0, 0],
            "dr_offered": [0, 0], "dr_chosen": [1 if joint else 0, 0], "both_chosen": [1 if joint else 0, 0],
            "both_chosen_natural_win": [0, 0]}


# D helps at every level (+1 of 4), B and C equal A, the continuation check is 0.
BASE_WINS = {f"D@{b}": 1 for b in BUDGETS}


class DispositionsAndAdequacyTest(unittest.TestCase):
    def run_summary(self, rows, weights=None):
        out = {}
        with contextlib.redirect_stdout(io.StringIO()):
            analyze_search.summarize("t", rows, BUDGETS, 400, 7, out, weights=weights)
        return out["t"]

    def test_dispositions_use_sign_holm_and_adequacy(self):
        wins = dict(BASE_WINS, **{"C@400": 0, "A@400": 1, "B@400": 1})  # C-A = -0.25 at 400
        wins.update({"D@400": 2, "A@100": 1, "D@100": 2})  # D-A = +0.25 at 100 and 400
        rows = [adequate_row(i, i, wins) for i in range(30)]
        e = self.run_summary(rows)
        a = e["adequacy"]
        for k in ("1_rounds", "2_stable_choice", "3_no_growth", "4_continuation"):
            self.assertTrue(a[k]["passed"], (k, a[k]))
        self.assertTrue(a["met"])
        self.assertNotIn("5_line_sampled", a)
        c = e["contrasts"]
        self.assertEqual(c["D-A@400"]["disposition"], analyze_search.HELPS)
        self.assertEqual(c["D-C@400"]["disposition"], analyze_search.HELPS)
        self.assertIn("lo_holm", c["D-C@400"])
        # A negative effect is never "helps" (a two-sided rejection is not a benefit).
        self.assertLess(c["C-A@400"]["hi"], 0)
        self.assertEqual(c["C-A@400"]["disposition"], analyze_search.AGAINST)
        self.assertEqual(c["B-A@400"]["disposition"], analyze_search.AGAINST)
        self.assertEqual(e["holm"]["C-A@400"]["disposition"], analyze_search.AGAINST)
        self.assertFalse(e["holm"]["C-A@400"]["helps"])
        self.assertNotIn("reject_05", e["holm"]["C-A@400"])
        only = e["only_d_helps"]
        self.assertTrue(only["D-A helps"] and not only["B-A helps"] and not only["C-A helps"])
        self.assertTrue(only["interaction lower bound above 0"] and only["holds"])

    def test_each_adequacy_criterion_can_fail_alone(self):
        def adequacy(rows):
            return self.run_summary(rows)["adequacy"]

        ok = [adequate_row(i, i, BASE_WINS) for i in range(30)]
        self.assertTrue(adequacy(ok)["met"])
        # (1) 8 of 30 roots complete only 7 rounds: 73% < 75%.
        rows = [adequate_row(i, i, BASE_WINS, rounds=7 if i < 8 else 10) for i in range(30)]
        a = adequacy(rows)
        self.assertFalse(a["1_rounds"]["passed"])
        self.assertTrue(a["2_stable_choice"]["passed"] and a["3_no_growth"]["passed"])
        self.assertFalse(a["met"])
        # (2) Only 19 roots separate: fails even though all 19 are stable.
        rows = [adequate_row(i, i, BASE_WINS, means=(0.5, 0.5, 0.5, 0.5) if i >= 19 else (0.2, 0.6, 0.4, 0.4))
                for i in range(30)]
        a = adequacy(rows)
        self.assertEqual((a["2_stable_choice"]["separating"], a["2_stable_choice"]["same"]), (19, 19))
        self.assertFalse(a["2_stable_choice"]["passed"])
        self.assertTrue(a["1_rounds"]["passed"])
        # (2) 25 separate, 6 change their choice: 76% < 80%.
        rows = [adequate_row(i, i, BASE_WINS, means=(0.5,) * 4 if i >= 25 else (0.2, 0.6, 0.4, 0.4),
                             chosen=(2, 1) if i < 6 else (1, 1)) for i in range(30)]
        a = adequacy(rows)
        self.assertEqual((a["2_stable_choice"]["separating"], a["2_stable_choice"]["same"]), (25, 19))
        self.assertFalse(a["2_stable_choice"]["passed"])
        # (3) D-A grows by 0.25 between 100 and 400.
        rows = [adequate_row(i, i, dict(BASE_WINS, **{"D@100": 0})) for i in range(30)]
        a = adequacy(rows)
        self.assertFalse(a["3_no_growth"]["passed"])
        self.assertAlmostEqual(a["3_no_growth"]["diff"], 0.25)
        # (4) The improved continuation loses with the model's action.
        rows = [adequate_row(i, i, dict(BASE_WINS, ref=1)) for i in range(30)]
        a = adequacy(rows)
        self.assertFalse(a["4_continuation"]["passed"])
        self.assertTrue(a["3_no_growth"]["passed"])
        # (4) No ref_improved label: not evaluable, fails.
        for r in rows:
            del r["eval_jobs"]["ref_improved"]
        self.assertIn("EVAL_CROSS", adequacy(rows)["4_continuation"]["note"])

    def test_spy_line_criterion_limits_the_outcomes(self):
        # Offered at 20 roots, the joint sequence chosen at 9 of them (45%).
        rows = [adequate_row(i, i, BASE_WINS, spy=spy_counts(i < 20, i < 9)) for i in range(30)]
        e = self.run_summary(rows)
        a = e["adequacy"]
        self.assertTrue(a["spy"])
        self.assertEqual((a["5_line_sampled"]["offered"], a["5_line_sampled"]["joint"]), (20, 9))
        self.assertFalse(a["5_line_sampled"]["passed"])
        self.assertFalse(a["line_sampled"] or a["met"])
        self.assertEqual(e["contrasts"]["D-A@400"]["disposition"], analyze_search.HELPS)
        self.assertEqual(e["contrasts"]["B-A@400"]["disposition"], analyze_search.NOT_SAMPLED)
        # At 10 of 20 (50%) the line was sampled: evidence against is possible again.
        rows = [adequate_row(i, i, BASE_WINS, spy=spy_counts(i < 20, i < 10)) for i in range(30)]
        e = self.run_summary(rows)
        self.assertTrue(e["adequacy"]["5_line_sampled"]["passed"] and e["adequacy"]["met"])
        self.assertEqual(e["contrasts"]["B-A@400"]["disposition"], analyze_search.AGAINST)
        # A Spy row without the level's counts is reported and fails.
        rows[0]["selection"]["D"]["levels"][2].pop("spy")
        a = self.run_summary(rows)["adequacy"]
        self.assertIn("lack", a["5_line_sampled"]["note"])
        self.assertFalse(a["5_line_sampled"]["passed"])

    def test_holm_step_down_bounds(self):
        class FakeBoot:
            table = {"a": (0.001, (0.01, 0.2)), "b": (0.02, (-0.01, 0.1)), "c": (0.03, (0.001, 0.1)),
                     "d": (0.5, (-0.1, 0.1)), "neg": (0.0001, (-0.3, -0.1))}

            def pvalue(self, k):
                return self.table[k][0]

            def ci(self, k, alpha):
                return self.table[k][1]

        res = analyze_search.holm_bounds(FakeBoot(), {k: k for k in ("a", "b", "c", "d")})
        self.assertEqual([res[k]["rank"] for k in "abcd"], [0, 1, 2, 3])
        self.assertAlmostEqual(res["a"]["alpha"], 0.0125)
        self.assertAlmostEqual(res["b"]["alpha"], 0.05 / 3)
        self.assertEqual([res[k]["helps"] for k in "abcd"], [True, False, False, False])
        # c's interval excludes 0, but the step-down stopped at b.
        self.assertFalse(res["c"]["rejected"])
        res = analyze_search.holm_bounds(FakeBoot(), {k: k for k in ("neg", "a")})
        self.assertTrue(res["neg"]["rejected"] and not res["neg"]["helps"])
        self.assertTrue(res["a"]["helps"])

    def test_representative_estimates_reweight_win_loss_to_the_corpus(self):
        # Sampled half won, half lost; the corpus is 75% won.
        rows = [adequate_row(i, i, dict(BASE_WINS, **{"D@400": 4 if i % 2 == 0 else 0}),
                             score=1.0 if i % 2 == 0 else 0.0) for i in range(40)]
        weights = analyze_search.representative_weights(rows, {"own_main|action": {"won": 300, "lost": 100,
                                                                                   "other": 7}})
        self.assertAlmostEqual(weights[0], 1.5)
        self.assertAlmostEqual(weights[1], 0.5)
        self.assertAlmostEqual(sum(weights), 40)
        e = self.run_summary(rows, weights)
        self.assertTrue(e["weighted"])
        self.assertAlmostEqual(e["win"]["D@400"], 0.75)
        self.assertAlmostEqual(e["contrasts"]["D-A@400"]["diff"], 0.75)
        self.assertAlmostEqual(self.run_summary(rows)["win"]["D@400"], 0.5)
        with self.assertRaises(SystemExit):
            analyze_search.representative_weights(rows, {"other|stratum": {"won": 1, "lost": 1}})

    def test_main_end_to_end_with_freeze_manifest(self):
        rows = [adequate_row(i, i // 2, BASE_WINS, score=float(i % 2)) for i in range(24)]
        mech = [dict(adequate_row(100 + i, 100 + i, BASE_WINS), set="mechanism:spy",
                     mechanism_tag=["spy_cast", "spy_target"][i % 2]) for i in range(8)]
        with tempfile.TemporaryDirectory() as d:
            path, freeze, out = (os.path.join(d, x) for x in ("cond.jsonl", "freeze.json", "out.json"))
            with open(path, "w") as f:
                for r in rows + mech:
                    f.write(json.dumps(r) + "\n")
            with open(freeze, "w") as f:
                json.dump({"strata_outcomes": {"own_main|action": {"won": 10, "lost": 30, "other": 0}}}, f)
            old = __import__("sys").argv
            __import__("sys").argv = ["analyze_search.py", path, "--freeze", freeze, "--json", out, "--boot", "50"]
            try:
                with contextlib.redirect_stdout(io.StringIO()):
                    analyze_search.main()
            finally:
                __import__("sys").argv = old
            with open(out) as f:
                res = json.load(f)
        self.assertTrue(res["representative_weighted"])
        self.assertTrue(res["representative/all"]["weighted"])
        self.assertEqual(res["mechanism/spy"]["roots"], 8)
        self.assertEqual(res["mechanism/spy_cast"]["roots"], 4)
        self.assertIn("adequacy", res["mechanism/spy"])


def compare_files(old_rows, new_rows, *extra):
    with tempfile.TemporaryDirectory() as d:
        paths = []
        for name, rows in (("old.jsonl", old_rows), ("new.jsonl", new_rows)):
            p = os.path.join(d, name)
            with open(p, "w") as f:
                for r in rows:
                    f.write(json.dumps(r) + "\n")
            paths.append(p)
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            code = compare_rows.main(paths + list(extra))
    return code, buf.getvalue()


def pr164_cond_row(index=0):
    """A cond row as the PR #164 binary wrote it."""
    job = {"action": 0, "follow": "t1", "scores": [1.0, 0.0], "natural": [True, True], "transitions": [10, 12],
           "wall": [0.5, 0.4], "non_natural": 0, "inner_failures": 0, "searched_decisions": 0, "deviations": []}
    selection = {c: {"follow": "t1", "candidates": [0, 1], "candidate_probs": [0.6, 0.4],
                     "levels": [{"budget": 100, "transitions": 120, "wall": 1.5, "rounds": 3, "means": [0.5, 0.25],
                                 "chosen": 0, "chosen_prob": 0.6, "chosen_sem": "x", "exhausted": False}],
                     "rounds_attempted": 3, "failed_rounds": 0, "failures": [], "inner_failures": 0,
                     "searched_decisions": 0} for c in "ABCD"}
    return {"kind": "cond", "root_index": index, "root_id": f"g1s{index}", "game": 1, "step": index,
            "t1_action": 0, "horizon": "decisions:3", "m_inner": 1, "budgets": [100], "selection": selection,
            "selection_wall": 7.0, "selection_actual": {"transitions": 480, "wall": 6.0, "playouts": 12},
            "eval_jobs": {"ref": 0}, "replay_secs": 0.1, "root_wall": 9.0,
            "eval": {"playouts": 2, "attempted": 2, "discarded_dets": 0, "failed_playouts": 0, "failures": [],
                     "actual_transitions": 22, "actual_wall": 0.9, "jobs": [job]}}


def current_cond_row(index=0, spy=True):
    """The same row as the current binary writes it with no new variable set."""
    r = json.loads(json.dumps(pr164_cond_row(index)))
    r.update({"budget_stop": "round", "config": {"mode": "cond"}, "spy_labels": spy,
              "compute": {"total_transitions": 502}, "root_wall": 9.7, "selection_wall": 6.5, "replay_secs": 0.2})
    for c in "ABCD":
        s = r["selection"][c]
        s.update({"budget_stop": "round", "cap_hit_playouts": 0, "searched_menu_sum": 0, "searched_menu_max": 0})
        s["levels"][0]["wall"] = 1.7
        if spy:
            s["levels"][0]["spy"] = {"self_offered": [0, 0], "both_chosen": [0, 0]}
    job = r["eval"]["jobs"][0]
    job.update({"transitions_total": 22, "cap_hits": 0, "searched_menu_sum": 0, "searched_menu_max": 0,
                "wall": [0.6, 0.3]})
    if spy:
        job["spy"] = {"self_offered": 0, "both_chosen": 0}
    r["eval"]["actual_wall"] = 1.1
    return r


class CompareRowsTest(unittest.TestCase):
    def test_added_keys_and_wall_fields_are_excluded(self):
        old = [pr164_cond_row(0), pr164_cond_row(1)]
        new = [current_cond_row(1), current_cond_row(0, spy=False)]  # order does not matter
        code, text = compare_files(old, new)
        self.assertEqual(code, 0, text)
        self.assertIn("all rows identical", text)

    def test_any_other_difference_fails(self):
        old = [pr164_cond_row(0)]
        cases = []
        r = current_cond_row(0)
        r["eval"]["jobs"][0]["scores"] = [1.0, 1.0]  # a changed existing field
        cases.append((r, "scores"))
        r = current_cond_row(0)
        r["eval"]["jobs"][0]["transitions"] = [10.0, 12]  # same value, other number type
        cases.append((r, "transitions"))
        r = current_cond_row(0)
        r["selection"]["A"]["levels"][0]["chosen"] = 1
        cases.append((r, "chosen"))
        r = current_cond_row(0)
        r["unlisted_key"] = 1  # a new key not in ADDED_KEYS
        cases.append((r, "unlisted_key"))
        r = current_cond_row(0)
        r["eval"]["cap_hits"] = 0  # an added key name at an unlisted position
        cases.append((r, "cap_hits"))
        r = current_cond_row(0)
        del r["eval"]["jobs"][0]["searched_decisions"]  # an existing key removed
        cases.append((r, "searched_decisions"))
        r = current_cond_row(0)
        r["selection"]["E"] = r["selection"]["A"]  # an unlisted condition
        cases.append((r, "selection.E"))
        for row, needle in cases:
            code, text = compare_files(old, [row])
            self.assertEqual(code, 1, needle)
            self.assertIn(needle, text)
        code, text = compare_files(old, [])
        self.assertEqual(code, 1)
        self.assertIn("only in OLD", text)
        code, text = compare_files(old, [current_cond_row(0), current_cond_row(0)])
        self.assertEqual(code, 1)
        self.assertIn("duplicate key in NEW", text)
        # --wall-only keeps the added keys, for two runs of the same binary.
        code, _ = compare_files([current_cond_row(0)], [current_cond_row(0)], "--wall-only")
        self.assertEqual(code, 0)
        code, _ = compare_files(old, [current_cond_row(0)], "--wall-only")
        self.assertEqual(code, 1)

    def test_roots_rows_drop_only_the_own_turn_strands_tag(self):
        menu = ['{"action_kind":"cast_spell","source":"Prismatic Strands"}']
        old = [{"kind": "root", "game": 1, "step": 4, "tags": [], "menu": None, "k": 3},
               {"kind": "root", "game": 1, "step": 9, "tags": ["cawgates_gate_colour"], "menu": menu, "k": 3},
               {"kind": "roots_game", "game": 1, "roots": 2}]
        new = [dict(old[0], tags=["cawgates_strands_castable_own_turn"], menu=menu),
               dict(old[1], tags=["cawgates_gate_colour", "cawgates_strands_castable_own_turn"]),
               dict(old[2])]
        code, text = compare_files(old, new)
        self.assertEqual(code, 0, text)
        # A tagged row's menu is not reset, and other tag changes are reported.
        bad = [new[0], dict(new[1], menu=None), new[2]]
        self.assertEqual(compare_files(old, bad)[0], 1)
        bad = [dict(new[0], tags=["cawgates_basilisk"]), new[1], new[2]]
        self.assertEqual(compare_files(old, bad)[0], 1)

    def test_added_key_paths_are_exact(self):
        for kind, paths in compare_rows.ADDED_KEYS.items():
            for path in paths:
                self.assertTrue(all(isinstance(s, str) and s and "*" not in s for s in path), (kind, path))
        self.assertNotIn(("cap_hits",), compare_rows.ADDED_KEYS["cond"])


if __name__ == "__main__":
    unittest.main()
