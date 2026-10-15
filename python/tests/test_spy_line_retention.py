"""Spy line retention trace tool (tools/spy_distill/retention.py): fixture tests; nothing is played."""
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from python.tests import test_spy_distill_analysis as panels_fixture  # noqa: E402
from tools.spy_distill import analyze, retention  # noqa: E402
from tools.spy_distill import distill_common as common  # noqa: E402

UPDATES = retention.UPDATES


def write(path: Path, value) -> dict:
    path.write_bytes(json.dumps(value).encode())
    return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def source(tag: str, sha: str = "9" * 64, lineage: str = "r1") -> dict:
    return {"play_import": {"path": "import.json", "sha256": "1" * 64},
            "feature_transfer": {"expected_feature_contract_digest": "c", "expected_feature_encoding_digest": "e"},
            "checkpoint": {"path": f"{tag}.json",
                           "sha256": common.PARENT_CHECKPOINT_SHA256[lineage] if tag == "parent" else sha}}


def x1_fields(state: str, index: int) -> dict:
    return {"after_state_sha256": state, "adam_step": 1000 + index, "physical_decisions": 300 + index,
            "policy_substeps": 320 + index, "loss": 0.5, "policy_sum": 1.25, "value_sum": 2.0,
            "advantage_statistics": {"mean": 0.0, "std": 1.0, "min": -2.0, "max": 3.0, "clip_fraction": 0.0}}


def readout(cid, state, adam, self_p=0.2, giant_p=0.8, self_rows_p=0.9):
    training = {"self_chosen": {"rows": 6, "mean_deployed_probability": self_rows_p},
                "giant_chosen": {"rows": 9, "mean_deployed_probability": giant_p,
                                 "mean_deployed_giant_probability": min(1.0, giant_p + 0.1)}}
    completion = {"giant_chosen": {"rows": 6, "mean_deployed_probability": giant_p / 2}}
    return {"id": cid, "state_sha256": state, "adam_step": adam,
            "archive": {"training": {"groups": training}, "completion": {"groups": completion}},
            "anchor_held_out": {"mean_kl_from_parent_non_spy": 0.01, "non_spy_top_agreement": 0.95},
            "learner_offers": {"offers": 223, "mean_deployed_self_probability": self_p}}


def trace_row(chosen, won, normalized, raw=None, decision="self", iteration=0):
    raw = normalized if raw is None else raw
    return {"iteration": iteration, "decision": decision, "chosen": chosen, "terminal_return": 1 if won else -1,
            "normalized_advantage": normalized, "raw_advantage": raw, "td_error": raw / 2, "decisions_left": 30,
            "push": normalized, "slope": normalized}


def trace_episode(offered, chosen, won, completed=False, giant=(0, 0), j=0, seat=0, first=0, draw=False):
    return {"episode": f"spy-line-retention-v1-r1-rep1-o4-t0-j{j}-s{seat}", "learner_seat": seat,
            "starting_player": first, "self": {"offered": offered, "chosen": chosen},
            "giant": {"offered": giant[0], "chosen": giant[1]}, "cast": {"offered": 1, "chosen": 1},
            "line_completed": completed, "terminal_return": 0 if draw else 1 if won else -1}


def empty_trace(push=0.0):
    parts = ("chosen_won", "chosen_lost", "declined_won", "declined_lost")
    return {"iterations": [{"iteration": n, "spy_learner_episodes": 1,
                            "push": {d: {"total": push, "slope_total": push / 2,
                                         **{p: {"push": 0.0, "slope": 0.0, "menus": 0} for p in parts}}
                                     for d in retention.DECISIONS}} for n in range(UPDATES)]}


class Temp(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def run_record(self, name: str, states=None, first=None, changes=None) -> dict:
        """A complete 162-update run's completion record with real receipt,
        update record and checkpoint files; ``changes`` maps an update index
        to update-record fields to change."""
        directory = self.root / name
        directory.mkdir()
        states = states or [f"{name}-state-{n}" for n in range(1, UPDATES + 1)]
        current = first or source("d1l")
        pins = []
        for index in range(UPDATES):
            checkpoint = write(directory / f"ck-{index}.json", {"state_sha256": states[index],
                                                                "adam_step": 1000 + index})
            record = {"checkpoint": checkpoint, **x1_fields(states[index], index), **(changes or {}).get(index, {})}
            update = write(directory / f"update-{index}.json", record)
            receipt = {"schema": retention.ITERATION_SCHEMA, "iteration": index, "source": current,
                       "update": update, "output_identity": {"state_sha256": states[index], "adam_step": 1000 + index,
                                                             "checkpoint_sha256": f"{name}-{index}"}}
            pins.append(write(directory / f"complete-{index}.json", receipt))
            current = {**current, "checkpoint": checkpoint}
        return {"schema": retention.RUN_SCHEMA, "complete": True, "completed_iterations": UPDATES,
                "iterations": pins, "source": current,
                "actual_identity": {"state_sha256": states[-1], "adam_step": 1000 + UPDATES - 1}}


class Updates(Temp):
    def test_updates_follow_the_receipts_and_end_at_the_runs_endpoint(self):
        record = self.run_record("A-s0")
        out = retention.updates(record)
        self.assertEqual([u["update"] for u in out], list(range(1, UPDATES + 1)))
        self.assertEqual(out[0]["source"]["checkpoint"]["path"], str(self.root / "A-s0" / "ck-0.json"))
        self.assertEqual(out[-1]["source"], record["source"])
        self.assertEqual((out[17]["state_sha256"], out[17]["adam_step"]), ("A-s0-state-18", 1017))
        self.assertEqual(out[17]["record"], x1_fields("A-s0-state-18", 17))

    def test_changed_or_misordered_receipts_and_a_foreign_endpoint_are_refused(self):
        record = self.run_record("A-s1")
        changed = json.loads(json.dumps(record))
        Path(changed["iterations"][5]["path"]).write_text("{}")
        with self.assertRaisesRegex(ValueError, "differs from its pin"):
            retention.updates(changed)
        record = self.run_record("A-s2")
        swapped = dict(record, iterations=[record["iterations"][1], record["iterations"][0]] + record["iterations"][2:])
        with self.assertRaisesRegex(ValueError, "out of order"):
            retention.updates(swapped)
        with self.assertRaisesRegex(ValueError, "endpoint"):
            retention.updates(dict(record, actual_identity={"state_sha256": "x", "adam_step": 0}))
        with self.assertRaisesRegex(ValueError, "complete 162-update"):
            retention.updates(dict(record, completed_iterations=161))
        mismatched = self.run_record("A-s3", changes={7: {"after_state_sha256": "elsewhere"}})
        with self.assertRaisesRegex(ValueError, "output identity differs"):
            retention.updates(mismatched)

    def test_s0_runs_read_every_sixth_update_and_the_others_every_eighteenth(self):
        self.assertEqual(retention.read_updates("A-s0"), tuple(range(6, 163, 6)))
        self.assertEqual(retention.read_updates("N-s0"), tuple(range(6, 163, 6)))
        self.assertEqual(len(retention.read_updates("A-s0")), 27)
        self.assertEqual(retention.read_updates("A-s2"), (18, 36, 54, 72, 90, 108, 126, 144, 162))
        with self.assertRaisesRegex(ValueError, "unknown run"):
            retention.read_updates("B-s0")


class Probes(Temp):
    def test_each_probe_holds_the_parent_d1l_and_at_most_62_run_checkpoints(self):
        games, bank, c_games = self.root / "games.jsonl", self.root / "bank.json", self.root / "c-games.jsonl"
        games.write_text('{"id":"g1"}\n')
        c_games.write_text('{"id":"g9"}\n')
        bank.write_text("{}")
        runs = {role: self.run_record(role) for role in retention.RUNS}
        commands = retention.probe_commands("r1", source("parent"), source("d1l"), games, bank, (c_games, bank), {},
                                            [], runs, "/rt1/probe")
        self.assertEqual(len(commands), 2)
        ids = [[c["id"] for c in command["candidates"]] for command in commands]
        for probe_ids in ids:
            self.assertEqual(probe_ids[:2], ["parent", "D1L"])
            self.assertLessEqual(len(probe_ids), 64)
        self.assertEqual([len(i) - 2 for i in ids], [54, 54])
        self.assertEqual(ids[0][2], "A-s0-u006")
        self.assertEqual(ids[0][28], "A-s0-u162")
        self.assertEqual(ids[0][29], "N-s0-u006")
        self.assertEqual(ids[1][2:11], [f"A-s1-u{n:03d}" for n in range(18, 163, 18)])
        self.assertEqual(ids[1][-1], "N-s3-u162")
        self.assertEqual([b["role"] for b in commands[0]["banks"]], ["training", "completion"])
        self.assertEqual(commands[1]["output_directory"], "/rt1/probe/r1-probe-1")
        alone = retention.probe_commands("r1", source("parent"), source("d1l"), games, bank, (c_games, bank), {},
                                         [], {"A-s0": runs["A-s0"]}, "/rt1/probe")
        self.assertEqual([len(c["candidates"]) for c in alone], [29])
        with self.assertRaisesRegex(ValueError, "parent"):
            retention.probe_commands("r1", source("d1l"), source("d1l"), games, bank, (c_games, bank), {}, [],
                                     runs, "d")


class Exactness(Temp):
    def d2l(self, state: str, adam: int) -> Path:
        path = self.root / f"d2l-{state}.json"
        path.write_text(json.dumps({"state_sha256": state, "adam_step": adam}))
        return path

    def test_x1_compares_every_update_record_and_the_endpoint_against_d2l(self):
        screen_states = [f"screen-state-{n}" for n in range(1, UPDATES + 1)]
        screen = self.run_record("screen", states=screen_states)
        same = self.run_record("rerun", states=screen_states)
        d2l = self.d2l("screen-state-162", 1161)
        sha = hashlib.sha256(d2l.read_bytes()).hexdigest()
        with mock.patch.dict(retention.D2L_CHECKPOINT_SHA256, {"r1": sha}):
            result = retention.x1("r1", same, d2l, screen)
            self.assertTrue(result["pass"])
            self.assertEqual(result["differing_updates"], [])
            self.assertTrue(result["endpoint"]["reproduced"])
            states = list(screen_states)
            states[40] = "drift"
            drift = self.run_record("drift", states=states)
            result = retention.x1("r1", drift, d2l, screen)
            self.assertEqual((result["pass"], result["differing_updates"]),
                             (False, [{"update": 41, "fields": ["after_state_sha256"]}]))
            loss = self.run_record("loss", states=screen_states, changes={10: {"loss": 0.75}})
            result = retention.x1("r1", loss, d2l, screen)
            self.assertEqual(result["differing_updates"], [{"update": 11, "fields": ["loss"]}])
            only = retention.x1("r1", same, d2l)
            self.assertTrue(only["pass"])
            self.assertIn("endpoint only", only["compared"])
            moved = self.run_record("moved")
            self.assertFalse(retention.x1("r1", moved, d2l)["pass"])
        with self.assertRaisesRegex(ValueError, "not the recorded r1 checkpoint"):
            retention.x1("r1", same, d2l, screen)

    def test_x3_needs_identical_parent_readouts_and_d1l_equal_to_fit(self):
        fit = {"fits": {"r1": {"L": {"read": [
            {"candidate": "L-step-512", "state_sha256": "other"},
            {"candidate": "L-step-1024", "state_sha256": "d1l-state",
             "learner_offers": {"mean_deployed_self_probability": 0.2},
             "G1": {"self": {"mean_deployed_probability": 0.9}, "giant": {"mean_deployed_probability": 0.8}}}]}}}}

        def probe(parent_p=0.3, d1l_p=0.2, d1l_giant=0.8):
            return {"schema": retention.PROBE_SCHEMA, "complete": True, "lineage": "r1",
                    "candidates": [readout("parent", "p", 5, self_p=parent_p),
                                   readout("D1L", "d1l-state", 9, self_p=d1l_p, giant_p=d1l_giant)]}
        self.assertTrue(retention.x3("r1", [probe(), probe()], fit)["pass"])
        self.assertFalse(retention.x3("r1", [probe(), probe(parent_p=0.31)], fit)["parent_identical"])
        self.assertFalse(retention.x3("r1", [probe(d1l_p=0.21)], fit)["d1l_equals_fit"])
        # The Giant path is the target action's probability, not the summed Giant mass.
        self.assertFalse(retention.x3("r1", [probe(d1l_giant=0.7)], fit)["d1l_equals_fit"])


class Paths(Temp):
    def probe(self, runs, value):
        candidates = [readout("parent", "p", 5), readout("D1L", "d", 9, self_p=0.2, giant_p=0.8)]
        for role, record in runs.items():
            for u in retention.updates(record):
                if u["update"] in retention.read_updates(role):
                    s, g = value(role, u["update"])
                    candidates.append(readout(retention.candidate_id(role, u["update"]), u["state_sha256"],
                                              u["adam_step"], self_p=s, giant_p=g))
        return {"schema": retention.PROBE_SCHEMA, "complete": True, "lineage": "r1", "candidates": candidates}

    def test_rho_classes_and_time_course_follow_the_paths(self):
        runs = {"A-s0": self.run_record("A-s0"), "A-s1": self.run_record("A-s1")}

        def value(role, n):
            if role == "A-s0":
                return 0.2 * (1 - n / 162), 0.8
            return 0.06, 0.2
        out = retention.paths("r1", [self.probe(runs, value)], runs)
        self.assertEqual(out["d1l"], {"self": 0.2, "giant": 0.8})
        self.assertAlmostEqual(out["d1l_context"]["giant_summed_mass"], 0.9)
        self.assertAlmostEqual(out["d1l_context"]["completion_giant"], 0.4)
        a0, a1 = out["runs"]["A-s0"], out["runs"]["A-s1"]
        self.assertEqual(len(a0["series"]), 27)
        self.assertEqual(len(a1["series"]), 9)
        self.assertEqual(a0["class"], {"self": "lost", "giant": "kept"})
        self.assertEqual(a1["class"], {"self": "partly kept", "giant": "partly kept"})
        self.assertAlmostEqual(a1["rho"]["self"], 0.3)
        course = retention.time_course(a0, out["d1l"])
        self.assertEqual(course["self"], {"below_0.5": {"update": 84}, "below_0.2": {"update": 132}})
        self.assertEqual(course["giant"], {"below_0.5": {"update": None}, "below_0.2": {"update": None}})
        course = retention.time_course(a0, out["d1l"], empty_trace(push=0.5))
        self.assertEqual(course["self"]["below_0.5"], {"update": 84, "push_before": 0.5 * 83, "slope_before": 0.25 * 83})

    def test_a_probe_of_another_state_is_refused(self):
        runs = {"A-s1": self.run_record("A-s1")}
        probe = self.probe(runs, lambda role, n: (0.1, 0.1))
        probe["candidates"][-1]["state_sha256"] = "elsewhere"
        with self.assertRaisesRegex(ValueError, "another state"):
            retention.paths("r1", [probe], runs)

    def test_class_boundaries(self):
        self.assertEqual([retention.retained_class(r) for r in (0.5, 0.49, 0.21, 0.2, 0.0, None)],
                         ["kept", "partly kept", "partly kept", "lost", "lost", None])


class Readings(unittest.TestCase):
    def classes(self, a, n=("kept",) * 4):
        return {**{f"A-s{k}": v for k, v in enumerate(a)}, **{f"N-s{k}": v for k, v in enumerate(n)}}

    def test_reading1_names_four_and_three_of_four_and_reports_counts(self):
        def r1(a):
            return retention.reading1(self.classes(a))["reading"]
        text = retention.READING1
        self.assertEqual(r1(("lost",) * 4), text["reliably lost"])
        self.assertEqual(r1(("lost", "lost", "lost", "kept")), text["lost 3 of 4"])
        self.assertEqual(r1(("kept",) * 4), text["reliably kept"])
        self.assertEqual(r1(("kept", "kept", "partly kept", "kept")), text["kept 3 of 4"])
        self.assertEqual(r1(("partly kept",) * 4), text["partly kept"])
        self.assertEqual(r1(("partly kept", "partly kept", "kept", "partly kept")), text["partly kept"])
        self.assertEqual(r1(("lost", "lost", "kept", "kept")), text["other"])
        self.assertEqual(retention.reading1(self.classes(("lost", "lost", "partly kept", None)))["counts"],
                         {"lost": 2, "partly kept": 1, "kept": 0, "unread": 1})
        self.assertIn("in 3 of 4 runs", text["lost 3 of 4"])

    def test_reading2_needs_three_a_losses_and_compares_counts(self):
        def r2(a, n):
            return retention.reading2(self.classes(a, n))["reading"]
        text = retention.READING2
        lost4 = ("lost",) * 4
        # The review's counterexample: two A losses no longer read.
        self.assertEqual(r2(("lost", "lost", "kept", "kept"), ("kept", "kept", "lost", "lost")), text["none"])
        self.assertEqual(r2(lost4, ("kept", "kept", "partly kept", "partly kept")), text["needed"])
        self.assertEqual(r2(("lost", "lost", "lost", "kept"), ("kept", "kept", "partly kept", "lost")),
                         text["needed"])
        self.assertEqual(r2(lost4, ("kept", "lost", "partly kept", "partly kept")), text["other"])
        self.assertEqual(r2(lost4, lost4), text["not_needed"])
        self.assertEqual(r2(("lost", "lost", "lost", "kept"), ("lost", "lost", "kept", "lost")), text["not_needed"])
        self.assertIn("at the fixed states", text["needed"])
        counts = retention.reading2(self.classes(lost4, ("kept", "kept", "partly kept", None)))["counts"]
        self.assertEqual(counts["N"], {"lost": 0, "partly kept": 1, "kept": 2, "unread": 1})

    def test_reading3_reads_the_normalized_advantage_with_minimum_counts(self):
        trace = empty_trace()
        rows = [trace_row(True, True, -0.5, raw=0.2) for _ in range(9)] + [trace_row(True, False, 0.4)]
        episodes = [trace_episode(2, 1, True)] * 3 + [trace_episode(2, 0, True)] * 40 + [trace_episode(1, 0, False)]
        summary = retention.trace_summary([(trace, rows, episodes)])
        self.assertEqual(summary["self"]["chosen_menus"]["won"]["menus"], 9)
        self.assertEqual(retention.credited_statement(summary, "self"), "too few won line attempts to read")
        rows.append(trace_row(True, True, -0.1, raw=0.3))
        summary = retention.trace_summary([(trace, rows, episodes)])
        won = summary["self"]["chosen_menus"]["won"]
        self.assertEqual((won["negative_normalized_share"], won["negative_raw_share"]), (1.0, 0.0))
        self.assertEqual(retention.credited_statement(summary, "self"),
                         "won line attempts were credited negatively at the self-target choice")
        self.assertEqual(retention.raw_statement(summary, "self"),
                         "won line attempts did not draw a negative raw advantage at the self-target choice")
        self.assertEqual(summary["self"]["chosen"]["episodes"], 3)
        self.assertEqual(summary["self"]["declined"]["episodes"], 41)
        self.assertEqual(retention.attempts_statement(summary, "self"), "too few to read")
        positive = [trace_row(True, True, 0.2, decision="giant") for _ in range(10)]
        summary = retention.trace_summary([(trace, positive, episodes)])
        self.assertEqual(retention.credited_statement(summary, "giant"),
                         "won line attempts were not credited negatively at Dread Return on the Giant")
        lost = [trace_episode(1, 1, False, j=4)] * 30 + [trace_episode(1, 0, True, j=8, seat=1, first=1)] * 30
        summary = retention.trace_summary([(trace, [], lost)])
        self.assertEqual(retention.attempts_statement(summary, "self"),
                         "the learner's line attempts at the self-target choice won less than its declines")
        splits = summary["self"]["splits"]
        self.assertEqual(splits["opponent"]["parent"]["chosen"]["episodes"], 30)
        self.assertEqual(splits["opponent"]["A48"]["declined"]["won"], 30)
        self.assertEqual(splits["learner_first"]["true"]["declined"]["episodes"], 30)
        self.assertEqual(retention.attempts_statement(summary, "giant"), "too few to read")

    def test_a_draw_is_not_a_win_and_ids_give_the_opponent_kind(self):
        summary = retention.trace_summary([(empty_trace(), [], [trace_episode(1, 1, True, draw=True)])])
        self.assertEqual(summary["self"]["chosen"]["won"], 0)
        self.assertEqual([retention.opponent_kind(f"x-o4-t1-j{j}-s0") for j in (0, 2, 3, 7, 8, 9)],
                         ["current", "current", "parent", "parent", "A48", "A48"])
        with self.assertRaisesRegex(ValueError, "names no repeat"):
            retention.opponent_kind("x-o4-t1-s0")

    def test_wilson_interval(self):
        low, high = retention.wilson(5, 10)
        self.assertAlmostEqual(low, 0.2365930, places=6)
        self.assertAlmostEqual(high, 0.7634070, places=6)
        self.assertIsNone(retention.wilson(0, 0))

    def test_across_lineages(self):
        self.assertEqual(retention.across({"r1": "a", "r2": "a"}), "a")
        self.assertEqual(retention.across({"r1": "a", "r2": "b"}), "lineage-dependent")
        self.assertEqual(retention.across({"r1": "a"}), "not read in r2")


class Panels(panels_fixture.Fixture):
    def test_x4_then_each_endpoint_is_contrasted_with_d1l_over_the_same_games(self):
        def screen_panel(ck, opp, combo):
            return panels_fixture.panel(self.root, ck, opp, panels_fixture.constant(lambda o, t, k, s: 1, combo))
        half = lambda o, t, k, s: o == analyze.SPY and k < 6  # noqa: E731
        every = lambda o, t, k, s: o == analyze.SPY  # noqa: E731
        never = lambda o, t, k, s: False  # noqa: E731
        screen = {"panels": [screen_panel(f"r2-{role}", opp, half if role == "D1L" else never)
                             for role in ("D1L", "D2L", "parent") for opp in ("T1", "H")]}
        # retention.py imports the analyzer as its own module; both draw alike.
        patcher = mock.patch.object(retention.analyze, "DRAWS", panels_fixture.DRAWS)
        patcher.start()
        self.addCleanup(patcher.stop)
        screen_result = retention.analyze.analyze(screen, None)
        self.assertEqual(screen_result["contrasts"], analyze.analyze(screen, None)["contrasts"])
        entries = {"r2": [screen_panel(f"r2-{role}", opp, combo) for role, combo in (("A-s1", half), ("N-s2", every))
                          for opp in ("T1", "H")]}
        out = retention.panels(screen, screen_result, entries)
        self.assertEqual(out["x4"], {"compared": 6, "differing": [], "pass": True})
        out = out["r2"]
        self.assertEqual(set(out), {"D2L", "A-s1", "N-s2"})
        self.assertEqual(out["D2L"]["reading"], "behavioral loss of the line established")
        self.assertEqual(out["A-s1"]["reading"], "no change established")
        self.assertEqual(out["N-s2"]["reading"], "behavioral gain of the line established")
        self.assertAlmostEqual(out["D2L"]["metrics"]["J"]["worst"]["point"], -0.5)
        self.assertEqual(out["A-s1"]["metrics"]["spy_score"]["worst"]["point"], 0.0)
        self.assertEqual(out["A-s1"]["win_rate"]["spy_score"], "no change established")
        self.assertEqual(out["D2L"]["self_target_chosen_games"], 0)
        counts = retention.panel_counts(out, retention.A_RUNS)
        self.assertEqual((counts["loss"], counts["no change"]), (1, 1))
        self.assertEqual(counts["endpoints choosing the self-target in no panel game"], 1)
        changed = json.loads(json.dumps(screen_result))
        changed["contrasts"]["r2/D2L-D1L/J/pooled/worst"]["ci95"][0] -= 1e-9
        with self.assertRaisesRegex(ValueError, "X4 failed: \\['r2/D2L-D1L/J/pooled/worst'\\]"):
            retention.panels(screen, changed, entries)


if __name__ == "__main__":
    unittest.main()
