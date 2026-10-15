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
from python.tests import test_spy_distill_tools as tools_fixture  # noqa: E402
from tools.spy_distill import analyze, phase2, retention  # noqa: E402
from tools.spy_distill import distill_common as common  # noqa: E402

UPDATES = retention.UPDATES
HEAD, TREE = "c" * 40, "t" * 64


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
            "learner_offers": {"offers": 223, "mean_deployed_self_probability": self_p,
                               "mean_raw_self_probability": self_p / 2}}


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


def run_record(root: Path, name: str, states=None, first=None, changes=None) -> dict:
    """A complete 162-update run's completion record with real receipt,
    update record and checkpoint files; ``changes`` maps an update index to
    update-record fields to change."""
    directory = root / name
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


def probe_of(lineage: str, runs: dict, value, d1l_state: str = "d") -> dict:
    """A probe of the parent, D1L and every read update of the runs given;
    ``value(role, n)`` gives (s, g)."""
    candidates = [readout("parent", "p", 5), readout("D1L", d1l_state, 9, self_p=0.2, giant_p=0.8)]
    for role, record in runs.items():
        for u in retention.updates(record):
            if u["update"] in retention.read_updates(role):
                s, g = value(role, u["update"])
                candidates.append(readout(retention.candidate_id(role, u["update"]), u["state_sha256"],
                                          u["adam_step"], self_p=s, giant_p=g))
    return {"schema": retention.PROBE_SCHEMA, "complete": True, "lineage": lineage, "candidates": candidates,
            "runtime_commit": HEAD, "runtime_tree": TREE}


class Temp(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def run_record(self, name: str, states=None, first=None, changes=None) -> dict:
        return run_record(self.root, name, states, first, changes)


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
        # Output directories are absolute, so "/rt1" takes the drive on Windows.
        self.assertEqual(commands[1]["output_directory"], common.absolute("/rt1/probe/r1-probe-1"))
        alone = retention.probe_commands("r1", source("parent"), source("d1l"), games, bank, (c_games, bank), {},
                                         [], {"A-s0": runs["A-s0"]}, "/rt1/probe")
        self.assertEqual([len(c["candidates"]) for c in alone], [29])
        # With no runs, the reference probe reads the parent and D1L alone.
        reference = retention.probe_commands("r1", source("parent"), source("d1l"), games, bank, (c_games, bank),
                                             {}, [], {}, "/rt1/probe")
        self.assertEqual([[c["id"] for c in command["candidates"]] for command in reference], [["parent", "D1L"]])
        self.assertEqual(reference[0]["output_directory"], common.absolute("/rt1/probe/r1-probe-ref"))
        with self.assertRaisesRegex(ValueError, "parent"):
            retention.probe_commands("r1", source("d1l"), source("d1l"), games, bank, (c_games, bank), {}, [],
                                     runs, "d")

    def test_the_trace_command_pins_one_complete_runs_manifest_and_completion(self):
        record = self.run_record("A-s1")
        run = self.root / "A-s1-native"
        run.mkdir()
        manifest = write(run / "run.json", {"schema": retention.RUN_SCHEMA})
        completion = write(run / "completion.json", record)
        command = retention.trace_command(run, str(self.root / "trace" / "r1-A-s1"))
        self.assertEqual(command["mode"], "line_retention_trace")
        self.assertEqual(command["run"], {**manifest, "path": manifest["path"].replace("\\", "/")})
        self.assertEqual(command["completion"], {**completion, "path": completion["path"].replace("\\", "/")})
        self.assertTrue(command["output_directory"].endswith("/trace/r1-A-s1"))
        write(run / "completion.json", dict(record, completed_iterations=161))
        with self.assertRaisesRegex(ValueError, "complete 162-update"):
            retention.trace_command(run, str(self.root / "trace" / "r1-A-s1"))


class Keep(Temp):
    def test_the_keep_list_holds_every_file_the_read_updates_pin_and_checks_it(self):
        imported = write(self.root / "import.json", {"import": 1})
        record = self.run_record("A-s1", first={**source("d1l"), "play_import": imported})
        completion = write(self.root / "A-s1-completion.json", record)
        out = self.root / "keep.json"
        args = ["keep", "--role", "A-s1", "--run", completion["path"], "--out", str(out)]
        with mock.patch("sys.stdout"):
            self.assertEqual(retention.main(args), 0)
        kept = json.loads(out.read_text())
        self.assertTrue(kept["pass"])
        # Updates 18, 36, ..., 162: the checkpoints after updates 18 to 162 (the
        # endpoint's included), and the file they import.
        expected = [str(self.root / "A-s1" / f"ck-{n - 1}.json") for n in range(18, UPDATES + 1, 18)]
        self.assertEqual(sorted(f["path"] for f in kept["files"]), sorted(expected + [imported["path"]]))
        self.assertEqual(record["source"]["checkpoint"]["path"], expected[-1])
        Path(expected[3]).unlink()
        with mock.patch("sys.stdout"):
            self.assertEqual(retention.main(args), 1)
        self.assertEqual([f["path"] for f in json.loads(out.read_text())["files"] if not f["present"]], [expected[3]])


class Configs(unittest.TestCase):
    """The config gate on configs written by phase2.py from O's recorded config."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        saved = Path(self.temp.name) / "checkpoint-step-1024.json"
        saved.write_text(json.dumps({"archive_distillation": {"method": "spy-archive-distillation-phase1-v1"}}))
        self.d1l = {**tools_fixture.source("r1"), "checkpoint": {"path": str(saved), "sha256": "7" * 64}}

    def config(self, replicate=0, drop=False, workers=4, directory="D:/rt1/run"):
        return phase2.config("r1", tools_fixture.o_config(), self.d1l, workers, directory, tools_fixture.DECKS,
                             replicate=replicate, drop_spy_learner=drop)

    def configs(self):
        return {f"{arm}-s{k}": self.config(k, arm == "N", directory=f"D:/rt1/{arm}-s{k}")
                for arm in "AN" for k in range(4)}

    def test_the_gate_passes_phase2s_configs_against_the_screens_d2l_config(self):
        screen = self.config(workers=8, directory="D:/spy-distill/train/r1-D2L/native")
        result = retention.configs("r1", screen, self.configs(), tools_fixture.DECKS)
        self.assertTrue(result["pass"], result)
        self.assertEqual(len(result), 9)

    def test_any_other_difference_fails_the_gate(self):
        screen = self.config(workers=8, directory="D:/spy-distill/train/r1-D2L/native")
        changed = dict(screen, value_coefficient=0.25)
        self.assertFalse(retention.configs("r1", changed, self.configs(), tools_fixture.DECKS)
                         ["A-s0 equals the screen's D2L config"])
        cfgs = self.configs()
        # A Spy learner episode's seed: N-s2 does not hold it, so only A-s2 fails.
        spy = tools_fixture.DECKS[tools_fixture.pilot.SPY]
        update = cfgs["A-s2"]["iterations"][0]["episodes"]
        next(item for item in update if retention.learner_deck(item) == spy)["episode"]["seed"] += 1
        result = retention.configs("r1", screen, cfgs, tools_fixture.DECKS)
        self.assertEqual([k for k, v in result.items() if not v], ["A-s2 replicates A-s0", "pass"])
        cfgs = self.configs()
        cfgs["A-s2"]["iterations"][5]["episodes"][1]["episode"]["seed"] += 1
        result = retention.configs("r1", screen, cfgs, tools_fixture.DECKS)
        self.assertFalse(result["A-s2 replicates A-s0"])
        cfgs = self.configs()
        cfgs["A-s3"]["iterations"][7]["episodes"][0]["opponent"] = {"kind": "fixed", "id": "elsewhere"}
        self.assertFalse(retention.configs("r1", screen, cfgs, tools_fixture.DECKS)["A-s3 replicates A-s0"])
        cfgs = self.configs()
        cfgs["A-s1"] = self.config(2, directory="D:/rt1/A-s1")
        self.assertFalse(retention.configs("r1", screen, cfgs, tools_fixture.DECKS)["A-s1 replicates A-s0"])
        cfgs = self.configs()
        cfgs["N-s1"] = self.config(1, directory="D:/rt1/N-s1")
        result = retention.configs("r1", screen, cfgs, tools_fixture.DECKS)
        self.assertEqual([k for k, v in result.items() if not v], ["N-s1 drops the Spy learner from A-s1", "pass"])
        cfgs = self.configs()
        cfgs["N-s0"]["iterations"][3]["episodes"].pop()
        self.assertFalse(retention.configs("r1", screen, cfgs, tools_fixture.DECKS)
                         ["N-s0 drops the Spy learner from A-s0"])
        cfgs = self.configs()
        del cfgs["N-s3"]
        with self.assertRaisesRegex(ValueError, "all eight configs"):
            retention.configs("r1", screen, cfgs, tools_fixture.DECKS)


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
            self.assertEqual(only["compared"], "the endpoint only (no screen run given)")
            moved = self.run_record("moved")
            self.assertFalse(retention.x1("r1", moved, d2l)["pass"])
        with self.assertRaisesRegex(ValueError, "not the recorded r1 checkpoint"):
            retention.x1("r1", same, d2l, screen)

    def test_the_first_wave_checks_x1_alone(self):
        screen_states = [f"screen-state-{n}" for n in range(1, UPDATES + 1)]
        rerun, screen = self.root / "rerun.json", self.root / "screen.json"
        rerun.write_text(json.dumps(self.run_record("rerun", states=screen_states)))
        screen.write_text(json.dumps(self.run_record("screen", states=screen_states)))
        d2l = self.d2l("screen-state-162", 1161)
        out = self.root / "x1.json"
        args = ["exactness", "--lineage", "r1", "--rerun", str(rerun), "--d2l-checkpoint", str(d2l),
                "--screen", str(screen), "--out", str(out)]
        with mock.patch.dict(retention.D2L_CHECKPOINT_SHA256, {"r1": hashlib.sha256(d2l.read_bytes()).hexdigest()}), \
                mock.patch("sys.stdout"):
            self.assertEqual(retention.main(args), 0)
            result = json.loads(out.read_text())
            self.assertEqual((result["pass"], "x3" in result), (True, False))
            self.assertEqual(result["x1"]["compared"], "every update record and the endpoint")
            self.assertEqual(result["rerun"]["sha256"], hashlib.sha256(rerun.read_bytes()).hexdigest())
            with self.assertRaisesRegex(ValueError, "together"):
                retention.main(args + ["--probe", str(out)])

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
        # Before the first dispatch: the reference probe alone, by the x3 command.
        fit_path, probe_path, out = self.root / "fit.json", self.root / "ref.json", self.root / "x3.json"
        fit_path.write_text(json.dumps(fit))
        probe_path.write_text(json.dumps(probe()))
        args = ["x3", "--lineage", "r1", "--screen-fit", str(fit_path), "--probe", str(probe_path), "--out", str(out)]
        with mock.patch("sys.stdout"):
            self.assertEqual(retention.main(args), 0)
            self.assertTrue(json.loads(out.read_text())["pass"])
            probe_path.write_text(json.dumps(probe(d1l_p=0.21)))
            self.assertEqual(retention.main(args), 1)
        self.assertFalse(retention.x3("r1", [probe(), probe(parent_p=0.31)], fit)["parent_identical"])
        self.assertFalse(retention.x3("r1", [probe(d1l_p=0.21)], fit)["d1l_equals_fit"])
        # The Giant path is the target action's probability, not the summed Giant mass.
        self.assertFalse(retention.x3("r1", [probe(d1l_giant=0.7)], fit)["d1l_equals_fit"])


class Paths(Temp):
    def probe(self, runs, value):
        return probe_of("r1", runs, value)

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
        self.assertAlmostEqual(out["d1l_context"]["raw_self_probability"], 0.1)
        a0, a1 = out["runs"]["A-s0"], out["runs"]["A-s1"]
        self.assertAlmostEqual(a1["series"][0]["context"]["raw_self_probability"], 0.03)
        self.assertEqual(len(a0["series"]), 27)
        self.assertEqual(len(a1["series"]), 9)
        self.assertEqual(a0["class"], {"self": "lost", "giant": "kept"})
        self.assertEqual(a1["class"], {"self": "partly kept", "giant": "partly kept"})
        self.assertAlmostEqual(a1["rho"]["self"], 0.3)
        course = retention.time_course(a0, out["d1l"])
        self.assertEqual(course["self"], {"below_0.5": {"update": 84}, "below_0.2": {"update": 132}})
        self.assertEqual(course["giant"], {"below_0.5": {"update": None}, "below_0.2": {"update": None}})
        # Update 84's step is in the state read at update 84.
        course = retention.time_course(a0, out["d1l"], empty_trace(push=0.5))
        self.assertEqual(course["self"]["below_0.5"],
                         {"update": 84, "push_through": 0.5 * 84, "slope_through": 0.25 * 84})

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
        kept = retention.READING1["reliably kept"]
        needed = retention.READING2["needed"]
        self.assertEqual(retention.across({"r1": kept, "r2": kept}),
                         "ordinary training from D1L reliably kept this decision's probability at the fixed states, "
                         "in both lineages")
        self.assertEqual(retention.across({"r1": kept, "r2": retention.READING1["other"]}), "lineage-dependent")
        self.assertEqual(retention.across({"r1": kept}), "not read in r2")
        # A lineage where the reading does not apply is named.
        self.assertEqual(retention.across({"r1": retention.READING2["none"], "r2": needed}),
                         "applies in r2 only: " + needed.removesuffix(", in this lineage"))
        self.assertEqual(retention.across({"r1": "not separated", "r2": "too few to read"}),
                         "applies in r1 only: not separated")
        self.assertEqual(retention.across({"r1": "too few won line attempts to read",
                                           "r2": "too few won line attempts to read"}),
                         "too few won line attempts to read, in both lineages")


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
        self.assertEqual(out["x4"], {"compared": 30, "differing": [], "pass": True})
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
        for key in ("r2/D2L-D1L/J/pooled/worst", "r2/D2L-D1L/nine_deck/H/best"):
            changed = json.loads(json.dumps(screen_result))
            changed["contrasts"][key]["ci95"][0] -= 1e-9
            with self.assertRaisesRegex(ValueError, f"X4 failed: \\['{key}'\\]"):
                retention.panels(screen, changed, entries)

    def test_x4_alone_reads_the_screens_own_panels_of_both_lineages(self):
        half = lambda o, t, k, s: o == analyze.SPY and k < 6  # noqa: E731
        never = lambda o, t, k, s: False  # noqa: E731
        screen = {"panels": [panels_fixture.panel(self.root, f"{lin}-{role}", opp, panels_fixture.constant(
            lambda o, t, k, s: 1, half if role == "D1L" else never))
            for lin in ("r1", "r2") for role in ("D1L", "D2L", "parent") for opp in ("T1", "H")]}
        patcher = mock.patch.object(retention.analyze, "DRAWS", panels_fixture.DRAWS)
        patcher.start()
        self.addCleanup(patcher.stop)
        index, result, out = self.root / "index.json", self.root / "result.json", self.root / "x4.json"
        index.write_text(json.dumps(screen))
        screen_result = retention.analyze.analyze(screen, None)
        result.write_text(json.dumps(screen_result))
        args = ["x4", "--screen-index", str(index), "--screen-result", str(result), "--out", str(out)]
        with mock.patch("sys.stdout"):
            self.assertEqual(retention.main(args), 0)
        self.assertEqual(json.loads(out.read_text())["x4"], {"compared": 60, "differing": [], "pass": True})
        screen_result["contrasts"]["r1/D2L-D1L/J/T1/best"]["point"] += 1e-9
        result.write_text(json.dumps(screen_result))
        with mock.patch("sys.stdout"):
            self.assertEqual(retention.main(args), 1)
        self.assertEqual(json.loads(out.read_text())["x4"]["differing"], ["r1/D2L-D1L/J/T1/best"])


def write_lines(path: Path, items) -> dict:
    path.write_bytes("".join(json.dumps(item) + "\n" for item in items).encode())
    return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def sha256(path) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class Analysis(unittest.TestCase):
    """analyze_spec on one lineage, each run bound to its role through its
    config, completion, trace, X1 result and panels."""

    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = root = Path(cls.temp.name)
        cls.draws = mock.patch.object(retention.analyze, "DRAWS", panels_fixture.DRAWS)
        cls.draws.start()
        decks = tools_fixture.DECKS
        cls.spy = decks[tools_fixture.pilot.SPY]
        saved = root / "checkpoint-step-1024.json"
        saved.write_text(json.dumps({"archive_distillation": {"method": "spy-archive-distillation-phase1-v1"}}))
        d1l = {**tools_fixture.source("r1"), "checkpoint": {"path": str(saved), "sha256": "7" * 64}}
        o = tools_fixture.o_config()
        cls.cfgs = {role: phase2.config("r1", o, d1l, 4, f"D:/rt1/train/r1-{role}/native", decks,
                                        replicate=int(role[-1]), drop_spy_learner=role.startswith("N"))
                    for role in retention.RUNS}
        part = {"screen_config": cls.json("screen-config.json",
                                          phase2.config("r1", o, d1l, 8, "D:/screen/train/r1-D2L/native", decks)),
                "configs": {role: cls.json(f"{role}-config.json", cfg) for role, cfg in cls.cfgs.items()}}
        cls.runs = {role: run_record(root, role) for role in retention.RUNS}
        part["runs"] = {role: cls.json(f"{role}-completion.json", run) for role, run in cls.runs.items()}
        part["traces"] = {role: cls.trace(role, part["runs"][role]) for role in retention.RUNS}
        part["reports"] = {role: cls.report(role, part["traces"][role], part["configs"][role])
                           for role in retention.RUNS}
        part["x1"] = cls.x1_result(part["runs"]["A-s0"], True)

        def value(role, n):
            return (0.2 * (1 - n / UPDATES), 0.8) if role.startswith("A") else (0.19, 0.79)
        part["probes"] = [cls.json("probe.json", probe_of("r1", cls.runs, value, d1l_state="d1l-state"))]
        fit = {"fits": {"r1": {"L": {"read": [
            {"candidate": "L-step-1024", "state_sha256": "d1l-state",
             "learner_offers": {"mean_deployed_self_probability": 0.2},
             "G1": {"self": {"mean_deployed_probability": 0.9}, "giant": {"mean_deployed_probability": 0.8}}}]}}}}
        half = lambda o, t, k, s: o == analyze.SPY and k < 6  # noqa: E731
        every = lambda o, t, k, s: o == analyze.SPY  # noqa: E731
        never = lambda o, t, k, s: False  # noqa: E731

        def panel(name, opp, combo, sha="a" * 64):
            return panels_fixture.panel(root / "panels", name, opp, panels_fixture.constant(lambda o, t, k, s: 1, combo),
                                        sha)
        screen = {"panels": [panel(f"r1-{role}", opp, half if role == "D1L" else never)
                             for role in ("D1L", "D2L", "parent") for opp in ("T1", "H")]}
        combos = {"A-s1": half, "A-s2": never, "A-s3": half, "N-s0": every, "N-s1": half, "N-s2": half, "N-s3": half}
        part["panels"] = [panel(f"r1-{role}", opp, combo, cls.runs[role]["source"]["checkpoint"]["sha256"])
                          for role, combo in combos.items() for opp in ("T1", "H")]
        cls.spec = {"screen_fit": cls.json("fit.json", fit), "screen_index": cls.json("index.json", screen),
                    "screen_result": cls.json("result.json", retention.analyze.analyze(screen, None)),
                    "head_commit": HEAD, "r1": part}

    @classmethod
    def tearDownClass(cls):
        cls.draws.stop()
        cls.temp.cleanup()

    @classmethod
    def json(cls, name: str, value) -> str:
        path = cls.root / name
        path.write_text(json.dumps(value))
        return str(path)

    @classmethod
    def trace(cls, role: str, completion: str, drop: int = 0, name: str = "") -> str:
        directory = cls.root / f"trace-{role}{name}"
        directory.mkdir()
        manifest = write(directory / "run.json", {"schema": retention.RUN_SCHEMA, "config": cls.cfgs[role]})
        ids = retention.spy_learner_ids(cls.cfgs[role], cls.spy)[drop:]
        episodes = [{"episode": eid, "learner_seat": int(eid[-1]), "starting_player": k % 2,
                     "self": {"offered": 1, "chosen": int(k % 3 == 0)},
                     "giant": {"offered": int(k % 3 == 0), "chosen": int(k % 6 == 0)},
                     "cast": {"offered": 1, "chosen": 1}, "line_completed": k % 6 == 0,
                     "terminal_return": 1 if k % 2 == 0 else -1} for k, eid in enumerate(ids)]
        rows = [trace_row(True, k % 2 == 0, -0.1 if k % 2 == 0 else 0.3) for k in range(12)] if ids else []
        trace = {"schema": retention.TRACE_SCHEMA, "complete": True, "x2": "passed", "run": manifest,
                 "completion": {"path": completion, "sha256": sha256(completion)},
                 "iterations": empty_trace(push=0.01)["iterations"],
                 "rows": write_lines(directory / "rows.jsonl", rows),
                 "episodes": write_lines(directory / "episodes.jsonl", episodes), "spy_learner_episodes": len(ids),
                 "runtime_commit": HEAD, "runtime_tree": TREE}
        return cls.json(f"trace-{role}{name}.json", trace)

    @classmethod
    def report(cls, role: str, trace: str, config: str, name: str = "") -> str:
        """The launcher's production receipt: the run the trace read, from a
        request that pins the config file."""
        request = cls.json(f"request-{role}{name}.json", {"config": {"path": config, "sha256": sha256(config)}})
        return cls.json(f"report-{role}{name}.json",
                        {"complete": True, "qualification": False, "run": json.loads(Path(trace).read_text())["run"],
                         "request": {"path": request, "sha256": sha256(request)}})

    @classmethod
    def x1_result(cls, rerun: str, passed: bool, name: str = "x1.json") -> str:
        return cls.json(name, {"lineage": "r1", "rerun": {"path": rerun, "sha256": sha256(rerun)},
                               "x1": {"pass": passed, "compared": "every update record and the endpoint",
                                      "endpoint": {"reproduced": passed}}})

    def changed(self, **fields) -> dict:
        spec = json.loads(json.dumps(self.spec))
        spec["r1"].update(fields)
        return spec

    def test_every_run_is_read_through_its_bindings(self):
        result = retention.analyze_spec(self.spec, tools_fixture.DECKS)
        out = result["lineages"]["r1"]
        self.assertEqual((out["configs"]["pass"], out["x1"]["pass"], out["x3"]["pass"]), (True, True, True))
        self.assertEqual(result["build"], {"commit": HEAD, "tree": TREE, "outputs": 9})
        self.assertEqual(result["x4"], {"compared": 30, "differing": [], "pass": True})
        self.assertEqual(out["reading1"]["self"]["reading"], retention.READING1["reliably lost"])
        self.assertEqual(out["reading1"]["giant"]["reading"], retention.READING1["reliably kept"])
        self.assertEqual(out["reading2"]["self"]["reading"], retention.READING2["needed"])
        self.assertEqual(out["reading2"]["giant"]["reading"], retention.READING2["none"])
        self.assertEqual({run: r["J"] for run, r in out["panel_readings"].items()},
                         {"A-s0": "behavioral loss of the line established",
                          "A-s1": "no change established", "A-s2": "behavioral loss of the line established",
                          "A-s3": "no change established", "N-s0": "behavioral gain of the line established",
                          "N-s1": "no change established", "N-s2": "no change established",
                          "N-s3": "no change established"})
        self.assertEqual(set(out["panel_readings"]["A-s1"]["combo_resolved"]), {"worst", "best"})
        self.assertEqual(set(out["panel_readings"]["A-s1"]["self_target"]), {"T1", "H"})
        self.assertEqual({k: out["panel_counts"]["A"][k] for k in ("loss", "gain", "no change", "unread")},
                         {"loss": 2, "gain": 0, "no change": 2, "unread": 0})
        self.assertEqual(out["panel_counts"]["N"]["gain"], 1)
        summary = out["reading3"]["summary"]
        self.assertEqual(summary["spy_learner_episodes"], 4 * phase2.SPY_LEARNER_EPISODES)
        self.assertEqual(set(summary["push_by_run"]), set(retention.A_RUNS))
        self.assertAlmostEqual(out["time_course"]["A-s0"]["self"]["below_0.5"]["push_through"],
                               0.01 * out["time_course"]["A-s0"]["self"]["below_0.5"]["update"])
        self.assertEqual(result["across"]["reading1"]["self"], "not read in r2")

    def test_a_trace_bound_to_another_run_is_refused(self):
        part = self.spec["r1"]
        traces = dict(part["traces"], **{"A-s1": part["traces"]["N-s1"]})
        with self.assertRaisesRegex(ValueError, "A-s1: the trace reads another run"):
            retention.analyze_spec(self.changed(traces=traces), tools_fixture.DECKS)
        # Swapping two runs with their traces leaves each completion with its
        # trace, but not with its role's episodes.
        runs = dict(part["runs"], **{"A-s1": part["runs"]["A-s2"], "A-s2": part["runs"]["A-s1"]})
        traces = dict(part["traces"], **{"A-s1": part["traces"]["A-s2"], "A-s2": part["traces"]["A-s1"]})
        with self.assertRaisesRegex(ValueError, "A-s1: the run did not play its config's episodes"):
            retention.analyze_spec(self.changed(runs=runs, traces=traces), tools_fixture.DECKS)
        short = self.trace("A-s3", part["runs"]["A-s3"], drop=1, name="-short")
        with self.assertRaisesRegex(ValueError, "A-s3: the traced Spy learner episodes are not the run's"):
            retention.analyze_spec(self.changed(traces=dict(part["traces"], **{"A-s3": short})), tools_fixture.DECKS)

    def test_x1_must_hold_for_this_a_s0_run(self):
        part = self.spec["r1"]
        failed = self.x1_result(part["runs"]["A-s0"], False, "x1-failed.json")
        with self.assertRaisesRegex(ValueError, "r1: X1 failed"):
            retention.analyze_spec(self.changed(x1=failed), tools_fixture.DECKS)
        other = self.x1_result(part["runs"]["A-s1"], True, "x1-other.json")
        with self.assertRaisesRegex(ValueError, "not this A-s0 run's"):
            retention.analyze_spec(self.changed(x1=other), tools_fixture.DECKS)

    def test_panels_must_be_each_runs_own_and_complete(self):
        entries = self.spec["r1"]["panels"]
        wrong = json.loads(json.dumps(entries))
        wrong[0]["checkpoint_sha256"] = "b" * 64
        with self.assertRaisesRegex(ValueError, "r1-A-s1-vs-T1: not that run's endpoint"):
            retention.analyze_spec(self.changed(panels=wrong), tools_fixture.DECKS)
        missing = [e for e in entries if (e["checkpoint"], e["opponent"]) != ("r1-N-s2", "H")]
        with self.assertRaisesRegex(ValueError, "no panel for \\['N-s2-vs-H'\\]"):
            retention.analyze_spec(self.changed(panels=missing), tools_fixture.DECKS)
        a0 = entries + [dict(entries[0], checkpoint="r1-A-s0")]
        with self.assertRaisesRegex(ValueError, "r1-A-s0: not one of this lineage's new panels"):
            retention.analyze_spec(self.changed(panels=a0), tools_fixture.DECKS)

    def test_the_launcher_receipt_binds_each_run_to_its_config_file(self):
        part = self.spec["r1"]
        reports = dict(part["reports"], **{"A-s1": part["reports"]["A-s2"]})
        with self.assertRaisesRegex(ValueError, "A-s1: the report is another run's"):
            retention.analyze_spec(self.changed(reports=reports), tools_fixture.DECKS)
        # A config rewritten after its dispatch, here the same content in new
        # bytes, passes the config gate but is not the one the run was given.
        rewritten = self.json("A-s1-config-rewritten.json", None)
        Path(rewritten).write_text(json.dumps(self.cfgs["A-s1"], indent=1))
        configs = dict(part["configs"], **{"A-s1": rewritten})
        with self.assertRaisesRegex(ValueError, "A-s1: the run was given another config"):
            retention.analyze_spec(self.changed(configs=configs), tools_fixture.DECKS)
        qualification = self.json("report-qual.json", {**json.loads(Path(part["reports"]["N-s0"]).read_text()),
                                                       "qualification": True})
        with self.assertRaisesRegex(ValueError, "N-s0: the report is not a complete production receipt"):
            retention.analyze_spec(self.changed(reports=dict(part["reports"], **{"N-s0": qualification})),
                                   tools_fixture.DECKS)

    def test_every_trace_and_probe_comes_from_the_given_build(self):
        with self.assertRaisesRegex(ValueError, "from another build"):
            retention.analyze_spec({**self.spec, "head_commit": "d" * 40}, tools_fixture.DECKS)
        with self.assertRaisesRegex(ValueError, "full commit SHA"):
            retention.analyze_spec({key: v for key, v in self.spec.items() if key != "head_commit"},
                                   tools_fixture.DECKS)
        part = self.spec["r1"]
        other = self.json("trace-N-s1-tree.json", {**json.loads(Path(part["traces"]["N-s1"]).read_text()),
                                                    "runtime_tree": "u" * 64})
        with self.assertRaisesRegex(ValueError, "disagree on the build's tree"):
            retention.analyze_spec(self.changed(traces=dict(part["traces"], **{"N-s1": other})), tools_fixture.DECKS)

    def test_the_config_gate_runs_on_the_production_configs(self):
        configs = dict(self.spec["r1"]["configs"], **{"A-s2": self.spec["r1"]["configs"]["A-s1"]})
        with self.assertRaisesRegex(ValueError, "r1: the config gate failed"):
            retention.analyze_spec(self.changed(configs=configs), tools_fixture.DECKS)


if __name__ == "__main__":
    unittest.main()
