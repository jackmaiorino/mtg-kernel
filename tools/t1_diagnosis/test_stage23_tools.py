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

        devs = [dict(attack, counterfactual=cf(1, 3)), dict(attack, counterfactual=cf(2, 1)),
                dict(attack, counterfactual=cf(2, 2)), dict(attack)]
        res = analyze_cross.counterfactual_costs([cross_row(devs)], cross_row([])["cells"])
        self.assertEqual((res["logged"], res["evaluated"], res["transitions"]), (4, 3, 150))
        e = res["classes"]["attacks"]
        self.assertEqual((e["evaluated"], e["helpful"], e["harmful"], e["neutral"]), (3, 1, 1, 1))
        self.assertAlmostEqual(e["cost_win"], (0.5 - 0.25 + 0.0) / 3)
        self.assertEqual(res["classes"]["other"]["evaluated"], 0)


if __name__ == "__main__":
    unittest.main()
