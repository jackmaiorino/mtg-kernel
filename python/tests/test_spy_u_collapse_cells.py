"""U collapse diagnostic tools: cell commands, candidate labels, the phase-2
refusal, cell fit readings and one-factor contrasts. Pure; nothing is played."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools.spy_distill import cells, fit, phase1, phase2  # noqa: E402
from tools.spy_distill import distill_common as common  # noqa: E402

pilot = common.pilot
DECKS = pilot.load_decks()
# The engine records the f32 step size; JSON may spell it as its f64 expansion.
LR_AS_RECORDED = 2.499999936844688e-05
SUBSET_FLOOR = {"U-s32": 32, "U-c32": 32, "U-s256": 256, "U-c256": 256}


def source(lineage="r1", tag="parent", sha=None):
    return {"play_import": {"path": f"{tag}-import.json", "sha256": "1" * 64},
            "feature_transfer": {"expected_feature_contract_digest": "c", "expected_feature_encoding_digest": "e"},
            "checkpoint": {"path": f"{tag}.json", "sha256": sha or common.PARENT_CHECKPOINT_SHA256[lineage]}}


def readout(cid, g1=(0.1, 0.1), gap=4.0, agree=0.95, kl=0.01, state="s", line_rows=(), share=0.8, disc=0.1,
            offered=0.05):
    groups = {"self_chosen": {"rows": 5, "mean_deployed_probability": g1[0]},
              "giant_chosen": {"rows": 5, "mean_deployed_probability": g1[1]},
              "self_offered": {"rows": 12, "mean_deployed_self_probability": offered},
              "all": {"rows": 6000, "mean_deployed_probability": 0.4, "top_agreement": 0.35}}
    return {"id": cid, "state_sha256": state,
            "archive": {"training": {"groups": groups, "discrimination": {"self": disc, "giant": None},
                                     "tree_chosen_share": {"self": share, "giant": 1.0},
                                     "line_rows": list(line_rows)}},
            "anchor_held_out": {"median_clamped_top_two_gap": gap, "non_spy_top_agreement": agree,
                                "mean_kl_from_parent_non_spy": kl},
            "learner_offers": {"offers": 150, "mean_deployed_self_probability": 0.02,
                               "mean_raw_self_probability": 0.02, "self_top_share": 0.0}}


def line_row(step, chosen, decision="self", deployed=0.5, line=0.5):
    return {"game": "g", "step": step, "chosen": chosen, "self_offered": decision == "self",
            "giant_offered": decision == "giant", "deployed_probability": deployed, "raw_probability": deployed,
            "raw_gap": 0.0, "self_deployed_probability": line if decision == "self" else None,
            "giant_deployed_probability": line if decision == "giant" else None}


def census_line(row):
    self_offered, giant_offered = row["self_offered"], row["giant_offered"]
    return {"game": row["game"], "step": row["step"], "edge_backups": 300,
            "line": {"cast_offered": False, "cast_chosen": False, "self_offered": self_offered,
                     "self_chosen": self_offered and row["chosen"], "giant_offered": giant_offered,
                     "giant_chosen": giant_offered and row["chosen"]}}


class Temp(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def files(self):
        games, bank_file = self.root / "games.jsonl", self.root / "bank.json"
        games.write_text('{"id":"g"}\n')
        bank_file.write_text("{}")
        return games, bank_file, {"training": [], "held_out": []}


class CellCommands(Temp):
    def test_a_cell_is_arm_u_with_its_label_and_only_the_fields_it_changes(self):
        games, bank_file, anchor = self.files()
        options = phase1.cell_options("U-s32", min_edge_backups=32, checkpoint_steps=[8, 16, 32])
        self.assertEqual(options, {"cell": "U-s32", "min_edge_backups": 32, "checkpoint_steps": [8, 16, 32]})
        command = phase1.phase1_command("r1", "U", source(), games, bank_file, anchor, "d", options=options)
        self.assertEqual(command["options"], options)
        self.assertNotIn("options", phase1.phase1_command("r1", "U", source(), games, bank_file, anchor, "d"))
        self.assertIsNone(phase1.cell_options(None))
        with self.assertRaisesRegex(ValueError, "need --cell"):
            phase1.cell_options(None, learning_rate=2.5e-5)
        with self.assertRaisesRegex(ValueError, "arm U"):
            phase1.phase1_command("r1", "L", source(), games, bank_file, anchor, "d", options=options)

    def test_the_cli_writes_the_cell_command(self):
        games, bank_file, anchor = self.files()
        parent, anchor_file, out = self.root / "p.json", self.root / "a.json", self.root / "cmd.json"
        parent.write_text(json.dumps(source()))
        anchor_file.write_text(json.dumps(anchor))
        phase1.main(["phase1", "--lineage", "r1", "--arm", "U", "--parent", str(parent), "--games", str(games),
                     "--bank", str(bank_file), "--anchor", str(anchor_file), "--output-directory", "d",
                     "--out", str(out), "--cell", "U-lr", "--learning-rate", "2.5e-5",
                     "--checkpoint-steps", "8,16,32"])
        command = json.loads(out.read_text())
        self.assertEqual(command["options"], {"cell": "U-lr", "learning_rate": 2.5e-5, "checkpoint_steps": [8, 16, 32]})

    def test_the_design_gives_each_cell_its_options_and_schedule(self):
        shared = [8, 16, 32, 64, 128, 189, 256, 512, 1024, 2048, 4096, 8192]
        self.assertEqual(cells.schedule("r1", "U0"), shared)
        self.assertEqual(cells.schedule("r1", "U-lr"), shared[:8] + [756] + shared[8:])
        self.assertEqual(cells.schedule("r2", "U-s256"), [s if s != 189 else 239 for s in shared])
        self.assertEqual(cells.schedule("r2", "U-lr")[8], 956)
        self.assertEqual(cells.design_options("r2", "U-c256"),
                         {"cell": "U-c256", "count_control": 731, "checkpoint_steps": cells.schedule("r2", "U-c256")})
        self.assertEqual(cells.design_options("r1", "U-a16")["anchor_coefficient"], 16.0)
        self.assertEqual(cells.comparable({"learning_rate": LR_AS_RECORDED}),
                         {"learning_rate": cells.f32_bits(2.5e-5)})
        self.assertEqual(cells.f32_bits(2.5e-5), cells.f32_bits(1e-4 / 4))
        with self.assertRaisesRegex(ValueError, "unknown cell"):
            cells.design_options("r1", "U-a8")

    def test_commands_write_every_cells_dry_run_full_run_and_rerun(self):
        games, bank_file, anchor = self.files()
        written = cells.commands("r1", source(), games, bank_file, anchor, str(self.root / "phase1"))
        self.assertEqual(len(written), 3 * len(cells.CELLS))
        dry, full, rerun = written["U-c32-dry"], written["U-c32"], written["U-c32-rerun"]
        self.assertTrue(dry["dry_run"] and "dry_run" not in full and "stop_after_steps" not in full)
        self.assertEqual(rerun["stop_after_steps"], 32)
        self.assertEqual(full["options"], cells.design_options("r1", "U-c32"))
        self.assertEqual(full["options"]["count_control"], 3281)
        self.assertTrue(rerun["output_directory"].endswith("/phase1/r1-U-c32-rerun"))
        self.assertEqual({c["arm"] for c in written.values()}, {"uniform"})
        parent, anchor_file = self.root / "p.json", self.root / "a.json"
        parent.write_text(json.dumps(source()))
        anchor_file.write_text(json.dumps(anchor))
        cells.main(["commands", "--lineage", "r1", "--parent", str(parent), "--games", str(games), "--bank",
                    str(bank_file), "--anchor", str(anchor_file), "--root", str(self.root / "phase1"),
                    "--out-dir", str(self.root / "cmds")])
        self.assertEqual(json.loads((self.root / "cmds" / "r1-U-lr.command.json").read_text())["options"]
                         ["checkpoint_steps"], cells.schedule("r1", "U-lr"))

    def test_probe_candidates_carry_the_cell_label(self):
        games, bank_file, _ = self.files()
        records = []
        for label in ("U0", "U-a4"):
            checkpoints = []
            for step in (8, 16):
                path = self.root / f"{label}-source-{step}.json"
                path.write_text(json.dumps(source(sha="9" * 64)))
                checkpoints.append({"step": step, "source": common.pin(path)})
            records.append({"schema": phase1.PHASE1_SCHEMA, "complete": True, "stop_after_steps": None,
                            "identity": {"lineage": "r1", "arm": "uniform",
                                         "recipe": {"options": {"cell": label}}},
                            "checkpoints": checkpoints})
        o_final = source(tag="o", sha="8" * 64)
        command = phase1.probe_command("r1", source(), o_final, games, bank_file, {}, [], records, "d")
        self.assertEqual([c["id"] for c in command["candidates"]],
                         ["parent", "O-final", "U0-step-8", "U0-step-16", "U-a4-step-8", "U-a4-step-16"])


class Exactness(Temp):
    def records(self):
        identity = {"lineage": "r1", "arm": "uniform", "recipe": {"checkpoints": [189]},
                    "runtime_commit": "a", "runtime_tree": "b", "runtime_binary_sha256": "c"}
        rebuilt = {**identity, "runtime_commit": "d", "runtime_tree": "e", "runtime_binary_sha256": "f"}
        dry = {"schema": fit.PHASE1_SCHEMA, "dry_run": True, "identity": identity,
               "census": {"arm": {"rows": 6030}}, "anchor_preflight_rows": 39161}
        full = {"schema": fit.PHASE1_SCHEMA, "complete": True, "stop_after_steps": None, "identity": identity,
                "state_hashes": [{"step": 32, "phase1_state_sha256": "h"}, {"step": 189, "phase1_state_sha256": "i"}]}
        rerun = {"schema": fit.PHASE1_SCHEMA, "complete": True, "stop_after_steps": 32, "identity": rebuilt,
                 "state_hashes": [{"step": 32, "phase1_state_sha256": "h"}]}
        return dry, {**dry, "identity": rebuilt}, full, rerun

    def test_a_default_run_under_the_new_build_must_reproduce_the_screens_records(self):
        dry, new_dry, full, rerun = self.records()
        self.assertTrue(cells.exactness(dry, new_dry, full, rerun)["pass"])
        changed = cells.exactness(dry, {**new_dry, "census": {"arm": {"rows": 6029}}}, full, rerun)
        self.assertEqual((changed["pass"], changed["dry_run_differs_in"]), (False, ["census"]))
        moved = {**rerun, "state_hashes": [{"step": 32, "phase1_state_sha256": "other"}]}
        self.assertFalse(cells.exactness(dry, new_dry, full, moved)["pass"])
        cell = {**rerun, "identity": {**rerun["identity"], "recipe": {"checkpoints": [189], "options": {"cell": "U0"}}}}
        self.assertFalse(cells.exactness(dry, new_dry, full, cell)["pass"])
        paths = {}
        for name, value in (("screen-dry", dry), ("dry", new_dry), ("screen-full", full), ("rerun", rerun),
                            ("moved", moved)):
            paths[name] = self.root / f"{name}.json"
            paths[name].write_text(json.dumps(value))
        args = ["exactness", "--screen-dry", str(paths["screen-dry"]), "--dry", str(paths["dry"]),
                "--screen-full", str(paths["screen-full"]), "--out", str(self.root / "out.json"), "--rerun"]
        self.assertEqual(cells.main(args + [str(paths["rerun"])]), 0)
        self.assertEqual(cells.main(args + [str(paths["moved"])]), 1)

    def test_u0_state_compares_u0_with_the_screens_first_u_checkpoint(self):
        screen_fit = {"fits": {"r1": {"U": {"read": [{"step": 189, "state_sha256": "x"}]}}}}
        u0 = {"identity": {"lineage": "r1", "arm": "uniform", "recipe": {"options": {"cell": "U0"}}},
              "checkpoints": [{"step": 128, "state_sha256": "w"}, {"step": 189, "state_sha256": "x"}]}
        self.assertTrue(cells.u0_state(screen_fit, u0)["equal"])
        self.assertFalse(cells.u0_state(screen_fit, {**u0, "checkpoints": u0["checkpoints"][:1]})["equal"])
        with self.assertRaisesRegex(ValueError, "not a U0 run"):
            cells.u0_state(screen_fit, {**u0, "identity": {**u0["identity"], "recipe": {"options": {"cell": "U-a4"}}}})


class PhaseTwoRefusal(Temp):
    def test_a_cell_checkpoint_does_not_start_phase_2(self):
        its, _ = pilot.iterations("r1", "O", DECKS, None)
        recorded = {"schema": pilot.ndb.SCHEMA, "initial_source": source(),
                    "opponents": [{"id": pilot.FIXED_OPPONENT_ID, "source": source(tag="a48", sha="4" * 64)}],
                    "iterations": its, **json.loads(json.dumps(pilot.ndb.FROZEN)),
                    "update_backend": {"kind": "cpu"}, "collection_workers": 8, "preparation_workers": 8,
                    "output_directory": "D:/r1-O"}
        for name, provenance in (("screen", {"method": "m"}), ("cell", {"method": "m", "cell": {"cell": "U-a4"}})):
            path = self.root / f"{name}.json"
            path.write_text(json.dumps({"archive_distillation": provenance}))
            start = {**source(), "checkpoint": {"path": str(path), "sha256": "7" * 64}}
            if name == "screen":
                self.assertEqual(phase2.config("r1", recorded, start, 4, "d", DECKS)["initial_source"], start)
            else:
                with self.assertRaisesRegex(ValueError, "does not start phase 2"):
                    phase2.config("r1", recorded, start, 4, "d", DECKS)


class CellFit(Temp):
    def setUp(self):
        super().setUp()
        telemetry = self.root / "telemetry.json"
        telemetry.write_text(json.dumps([{"anchor_loss": 0.1, "archive_loss": 1.0,
                                          "anchor_components": {"all": {"auxiliary_l2": 1.0}},
                                          "archive_components": {"all": {"auxiliary_l2": 1.0}}}] * 8192))
        self.telemetry = str(telemetry)

    def cell_run(self, lineage, name, every=None, at=None, fail_from=None, kept=(), rows=None, options=None,
                 steps=None, rerun_hash="h32", disagreement=0.6):
        steps = steps or cells.schedule(lineage, name)
        options = options or cells.design_options(lineage, name)
        if "learning_rate" in options:
            options = {**options, "learning_rate": LR_AS_RECORDED}
        identity = {"lineage": lineage, "arm": "uniform", "recipe": {"checkpoints": steps, "options": options}}
        checkpoints = [{"step": s, "epochs": s / 189, "source": {"path": f"{name}{s}", "sha256": "0" * 64},
                        "state_sha256": f"{lineage}-{name}-{s}"} for s in steps]
        floor = SUBSET_FLOOR.get(name)
        counts = {"self_chosen": 13, "self_declined": 26, "giant_chosen": 8, "giant_declined": 3}
        census = {"arm": {"rows": rows or (cells.SUPPORT_ROWS[lineage][floor] if floor else 6030), **counts,
                          "parent_disagreement": disagreement},
                  "bank": {"rows": 6030, **counts, "parent_disagreement": 0.65},
                  "exposure": [{"step": s, "draws_per_row_min": 1} for s in steps],
                  "kept_line_rows": [census_line(r) for r in kept]}
        full = {"schema": fit.PHASE1_SCHEMA, "complete": True, "stop_after_steps": None, "identity": identity,
                "checkpoints": checkpoints, "telemetry": {"path": self.telemetry, "sha256": "0" * 64},
                "census": census, "state_hashes": [{"step": 32, "phase1_state_sha256": "h32"}]}
        rerun = {"schema": fit.PHASE1_SCHEMA, "complete": True, "stop_after_steps": 32, "identity": identity,
                 "state_hashes": [{"step": 32, "phase1_state_sha256": rerun_hash}]}
        paths = {}
        for part, value in (("full", full), ("rerun", rerun)):
            paths[part] = self.root / f"{lineage}-{name}-{part}.json"
            paths[part].write_text(json.dumps(value))
        candidates = []
        for s in steps:
            kwargs = {**(every or {}), **({"gap": 1.0} if fail_from and s >= fail_from else {}),
                      **(at or {}).get(s, {})}
            candidates.append(readout(f"{name}-step-{s}", state=f"{lineage}-{name}-{s}", **kwargs))
        return {k: str(v) for k, v in paths.items()}, candidates

    def spec(self, runs: dict, split=False, screen_state=None, screen_floors=None):
        spec, screen = {}, {"fits": {}}
        for lineage, named in runs.items():
            parent_ref, o_ref = readout("parent", gap=4.0), readout("O-final", agree=0.95, kl=0.02)
            spec[lineage] = {"cells": {}, "probes": []}
            groups = [[]]
            for name, kwargs in named.items():
                paths, candidates = self.cell_run(lineage, name, **kwargs)
                spec[lineage]["cells"][name] = paths
                if split and groups[-1]:
                    groups.append([])
                groups[-1] += candidates
            for index, group in enumerate(groups):
                probe = self.root / f"{lineage}-probe-{index}.json"
                probe.write_text(json.dumps({"schema": fit.PROBE_SCHEMA, "complete": True, "lineage": lineage,
                                             "candidates": [parent_ref, o_ref] + group}))
                spec[lineage]["probes"].append(str(probe))
            floors = screen_floors or fit.g2(parent_ref, parent_ref, o_ref)
            first = cells.FIRST_U_STEP[lineage]
            screen["fits"][lineage] = {"U": {"read": [{
                "step": first, "state_sha256": screen_state or f"{lineage}-U0-{first}",
                "G2": {k: floors[k] for k in cells.G2_REFERENCES}}]}}
        path = self.root / "screen-fit.json"
        path.write_text(json.dumps(screen))
        spec["screen_fit"] = str(path)
        return spec

    def test_cells_read_gated_checkpoints_to_their_first_g2_failure_and_select_as_the_screen_does(self):
        good = {8: {"gap": 1.0}, 16: {"gap": 1.0}, 64: {"g1": (0.6, 0.6)}, 128: {"g1": (0.7, 0.7)},
                189: {"g1": (0.8, 0.8)}, 256: {"g1": (0.8, 0.8)}, 512: {"kl": 0.5}}
        result = cells.fit_cells(self.spec({"r1": {"U0": {"fail_from": 189}, "U-a4": {"at": good}}}))
        a4 = result["fits"]["r1"]["U-a4"]
        self.assertEqual((a4["selected"]["step"], a4["fit_dose"], a4["stopped_at"]), (256, 64, 512))
        self.assertEqual([r["step"] for r in a4["read"]], [32, 64, 128, 189, 256, 512])
        self.assertEqual(a4["unread_checkpoints"], [1024, 2048, 4096, 8192])
        self.assertEqual((a4["reading"], a4["explanations"]), (cells.PASS, cells.PASS_EXPLANATIONS))
        self.assertEqual([d["step"] for d in a4["descriptive"]], [8, 16])
        self.assertNotIn("pass", a4["descriptive"][0]["G2"])
        self.assertNotIn("G3", a4["descriptive"][0])
        u0 = result["fits"]["r1"]["U0"]
        self.assertEqual((u0["stopped_at"], u0["reading"]), (189, fit.READINGS["collapse_first"][0]))
        self.assertTrue(result["u0_reproduces_screen"]["r1"]["equal"])
        self.assertTrue(result["g2_references"]["r1"]["equal"])
        self.assertEqual(result["passes"], {"U-a4": ["r1"], "U0": []})

    def test_an_all_row_cell_holding_g2_without_the_fit_reads_the_dose_limit(self):
        result = cells.fit_cells(self.spec({"r1": {"U0": {"fail_from": 189}, "U-a16": {}}}))
        a16 = result["fits"]["r1"]["U-a16"]
        self.assertEqual((a16["reading"], a16["explanations"]), (cells.NOT_FIT_ALL[0], cells.NOT_FIT_ALL[1]))
        self.assertEqual((a16["stopped_at"], a16["unread_checkpoints"]), (None, []))
        self.assertIn("dose limit", a16["explanations"][0])

    def test_the_mixed_choices_row_applies_to_all_row_cells_only(self):
        mixed = {"share": 0.5, "disc": 0.1, "offered": 0.3}
        result = cells.fit_cells(self.spec({"r1": {"U0": {"fail_from": 189}, "U-a4": {"every": mixed},
                                                   "U-s32": {"every": mixed}}}))
        self.assertEqual(result["fits"]["r1"]["U-a4"]["reading"], fit.READINGS["mixed"][0])
        s32 = result["fits"]["r1"]["U-s32"]
        self.assertEqual((s32["reading"], s32["explanations"]),
                         (cells.NOT_FIT_TEXT, cells.NOT_FIT_SUBSET[1] + cells.SUBSET_EXPLANATIONS))

    def test_subset_readings_carry_their_competing_explanations(self):
        result = cells.fit_cells(self.spec({"r1": {"U0": {"fail_from": 189}, "U-s256": {"fail_from": 64},
                                                   "U-c256": {"at": {64: {"g1": (0.9, 0.9)}}}}}))
        s256, c256 = result["fits"]["r1"]["U-s256"], result["fits"]["r1"]["U-c256"]
        self.assertEqual(s256["reading"], fit.READINGS["collapse_first"][0])
        self.assertEqual(s256["explanations"][-2:], cells.SUBSET_EXPLANATIONS)
        self.assertEqual(c256["reading"], cells.PASS)
        self.assertEqual(c256["explanations"],
                         cells.PASS_EXPLANATIONS + cells.SUBSET_PASS_EXPLANATIONS + cells.SUBSET_EXPLANATIONS)

    def test_split_probes_must_agree_and_match_the_screens_g2_references(self):
        runs = {"r1": {"U0": {"fail_from": 189}, "U-lr": {}}}
        result = cells.fit_cells(self.spec(runs, split=True))
        self.assertEqual(len(result["fits"]["r1"]), 2)
        spec = self.spec(runs, split=True)
        probe = Path(spec["r1"]["probes"][1])
        value = json.loads(probe.read_text())
        value["candidates"][0]["anchor_held_out"]["median_clamped_top_two_gap"] = 4.5
        probe.write_text(json.dumps(value))
        with self.assertRaisesRegex(ValueError, "differ between probes"):
            cells.fit_cells(spec)
        floors = {"gap_floor": 2.5, "agreement_floor": 0.93, "kl_ceiling": 0.02}
        with self.assertRaisesRegex(ValueError, "G2 references differ from the screen's"):
            cells.fit_cells(self.spec(runs, screen_floors=floors))

    def test_u0_must_reproduce_the_screen_and_every_run_must_be_its_designed_cell(self):
        runs = {"r1": {"U0": {"fail_from": 189}, "U-lr": {}}}
        with self.assertRaisesRegex(ValueError, "does not reproduce the screen"):
            cells.fit_cells(self.spec(runs, screen_state="other"))
        spec = self.spec(runs)
        spec["r1"]["cells"]["U-a4"] = spec["r1"]["cells"].pop("U-lr")
        with self.assertRaisesRegex(ValueError, "is cell"):
            cells.fit_cells(spec)
        wrong = {**cells.design_options("r1", "U-a4"), "anchor_coefficient": 8.0}
        with self.assertRaisesRegex(ValueError, "options differ from the design's"):
            cells.fit_cells(self.spec({"r1": {"U0": {}, "U-a4": {"options": wrong}}}))
        short = [s for s in cells.schedule("r1", "U0") if s != 189]
        with self.assertRaisesRegex(ValueError, "differ from its schedule"):
            cells.fit_cells(self.spec({"r1": {"U0": {"steps": short}}}))
        with self.assertRaisesRegex(ValueError, "no U0"):
            cells.fit_cells(self.spec({"r1": {"U-a4": {}}}))

    def test_subset_cells_report_kept_and_dropped_line_rows_and_parent_disagreement(self):
        r1, r2 = line_row(1, True, deployed=0.9, line=0.9), line_row(2, True, deployed=0.1, line=0.1)
        r3, r4 = line_row(3, False, deployed=0.6, line=0.2), line_row(4, True, "giant", deployed=0.7, line=0.7)
        every = {"line_rows": [r1, r2, r3, r4]}
        result = cells.fit_cells(self.spec({"r1": {"U0": {"fail_from": 189, "every": every},
                                                   "U-s256": {"every": every, "kept": [r1, r3]}}}))
        s256 = result["fits"]["r1"]["U-s256"]
        split = s256["read"][0]["line_rows_kept_and_dropped"]
        self.assertEqual(split["kept"]["self"], {"chosen": 1, "declined": 1, "mean_deployed_probability": 0.9,
                                                 "discrimination": 0.9 - 0.2})
        self.assertEqual(split["kept"]["giant"], {"chosen": 0, "declined": 0, "mean_deployed_probability": None,
                                                  "discrimination": None})
        self.assertEqual(split["dropped"]["self"]["mean_deployed_probability"], 0.1)
        self.assertEqual(split["dropped"]["giant"]["chosen"], 1)
        self.assertEqual(s256["parent_disagreement"], {"kept": 0.6, "bank": 0.65})
        self.assertEqual(s256["line_rows"]["kept"]["self_declined"], 26)
        self.assertIsNone(s256["read"][0]["training_rows"])
        self.assertEqual(s256["read"][0]["bank_rows"]["rows"], 6000)
        u0 = result["fits"]["r1"]["U0"]
        self.assertIsNone(u0["read"][0]["line_rows_kept_and_dropped"])
        self.assertIsNone(u0["parent_disagreement"])
        self.assertEqual(u0["read"][0]["training_rows"], u0["read"][0]["bank_rows"])
        with self.assertRaisesRegex(ValueError, "flags differ"):
            cells.fit_cells(self.spec({"r1": {"U0": {}, "U-s256": {"every": every,
                                                                   "kept": [{**r1, "chosen": False}]}}}))
        with self.assertRaisesRegex(ValueError, "not among the probe's line rows"):
            cells.fit_cells(self.spec({"r1": {"U0": {}, "U-s256": {"every": every, "kept": [line_row(9, True)]}}}))

    def test_count_controls_must_keep_their_support_cells_row_count(self):
        cells.fit_cells(self.spec({"r1": {"U0": {}, "U-s32": {}, "U-c32": {}}}))
        with self.assertRaisesRegex(ValueError, "do not both keep 3281 rows"):
            cells.fit_cells(self.spec({"r1": {"U0": {}, "U-s32": {}, "U-c32": {"rows": 3000}}}))

    def runs(self, **override):
        base = {"U0": {"fail_from": 64}, "U-a4": {"fail_from": 256}, "U-a16": {"fail_from": 64}, "U-lr": {},
                "U-s32": {"fail_from": 256}, "U-c32": {"fail_from": 128}}
        return {lin: {**base, **override} for lin in ("r1", "r2")}

    def test_contrasts_order_collapses_by_the_checkpoints_both_runs_read(self):
        out = cells.contrasts(cells.fit_cells(self.spec(self.runs())))
        a4 = out["against_u0"]["U-a4"]
        self.assertEqual((a4["per_lineage"]["r1"]["ordering"], a4["across_lineages"]), (cells.LATER, cells.LATER))
        self.assertEqual(a4["per_lineage"]["r1"]["wording"], cells.WORDING["anchor"][cells.LATER])
        self.assertEqual([g["step"] for g in a4["per_lineage"]["r1"]["g2_at_shared_steps"]], [32, 64])
        self.assertEqual((a4["per_lineage"]["r1"]["collapse_step"], a4["per_lineage"]["r1"]["reference_collapse_step"]),
                         (256, 64))
        self.assertEqual(out["against_u0"]["U-a16"]["per_lineage"]["r1"]["ordering"], cells.SAME)
        self.assertIsNone(out["against_u0"]["U-a16"]["per_lineage"]["r1"]["wording"])
        lr = out["against_u0"]["U-lr"]["per_lineage"]["r1"]
        self.assertEqual((lr["ordering"], lr["wording"]), (cells.LATER, cells.WORDING["step_size"][cells.LATER]))
        self.assertEqual(out["against_u0"]["U-s32"]["per_lineage"]["r1"]["ordering"], cells.LATER)
        control = out["count_controls"]["U-s32"]
        self.assertEqual((control["control"], control["across_lineages"]), ("U-c32", cells.LATER))
        self.assertEqual(control["per_lineage"]["r1"]["wording"], cells.WORDING["count_control"][cells.LATER])
        self.assertNotIn("selected", control["per_lineage"]["r1"])
        self.assertEqual(control["per_lineage"]["r1"]["parent_disagreement"],
                         {"U-s32": {"kept": 0.6, "bank": 0.65}, "U-c32": {"kept": 0.6, "bank": 0.65}})
        self.assertEqual(control["explanations"], cells.COUNT_CONTROL_EXPLANATIONS)
        pairs = out["matched_summed_step_size"]["r1"]
        self.assertEqual([(p["u0_step"], p["u_lr_step"]) for p in pairs], [(32, 128), (64, 256)])
        self.assertEqual(sorted(out["collapse_against_fit"]), ["U-a16", "U-a4", "U-lr", "U0"])
        self.assertEqual(out["collapse_against_fit"]["U0"]["r1"][0]["training_top_agreement"], 0.35)
        runs = self.runs()
        runs["r2"]["U-a4"] = {"fail_from": 64}
        mixed = cells.contrasts(cells.fit_cells(self.spec(runs)))
        self.assertEqual(mixed["against_u0"]["U-a4"]["across_lineages"], "lineage-dependent")

    def test_collapses_on_different_checkpoints_are_ordered_only_when_the_reads_order_them(self):
        runs = self.runs(**{"U0": {"fail_from": 1024}, "U-lr": {"fail_from": 756}, "U-a4": {},
                            "U-a16": {"fail_from": 4096}})
        runs["r2"]["U-lr"] = {"fail_from": 128}
        out = cells.contrasts(cells.fit_cells(self.spec(runs)))
        r1 = {name: v["per_lineage"]["r1"]["ordering"] for name, v in out["against_u0"].items()}
        self.assertEqual((r1["U-lr"], r1["U-a4"], r1["U-a16"]), (cells.NOT_ORDERED, cells.LATER, cells.LATER))
        self.assertEqual(out["against_u0"]["U-lr"]["per_lineage"]["r2"]["ordering"], cells.EARLIER)
        self.assertEqual(out["against_u0"]["U-lr"]["across_lineages"], "not ordered in r1")
        held = cells.contrasts(cells.fit_cells(self.spec(self.runs(**{"U0": {}, "U-a4": {}}))))
        self.assertEqual(held["against_u0"]["U-a4"]["across_lineages"], cells.NEITHER)
        broken = cells.contrasts(cells.fit_cells(self.spec(self.runs(**{"U-a4": {"rerun_hash": "other"}}))))
        self.assertEqual(broken["against_u0"]["U-a4"]["per_lineage"]["r1"]["ordering"], cells.NOT_EVIDENCE)
        self.assertEqual(broken["against_u0"]["U-a4"]["across_lineages"], "not evidence in r1 and r2")
        runs = self.runs()
        runs["r2"]["U-a4"] = {"rerun_hash": "other"}
        one = cells.contrasts(cells.fit_cells(self.spec(runs)))
        self.assertEqual(one["against_u0"]["U-a4"]["across_lineages"], "not evidence in r2")

    def test_a_shared_collapse_interval_reads_as_a_change_only_if_the_support_cell_moved_against_u0(self):
        def control(**override):
            out = cells.contrasts(cells.fit_cells(self.spec({"r1": {"U0": {"fail_from": 189}, **override}})))
            return out["count_controls"]["U-s32"]["per_lineage"]["r1"]
        null = control(**{"U-s32": {"fail_from": 189}, "U-c32": {"fail_from": 189}})
        self.assertEqual((null["ordering"], null["support_against_u0"], null["wording"]),
                         (cells.SAME, cells.SAME, cells.NO_CHANGE))
        moved = control(**{"U-s32": {"fail_from": 512}, "U-c32": {"fail_from": 512}})
        self.assertEqual((moved["ordering"], moved["support_against_u0"], moved["wording"]),
                         (cells.SAME, cells.LATER, cells.WORDING["count_control"][cells.SAME]))
        earlier = control(**{"U-s32": {"fail_from": 64}, "U-c32": {"fail_from": 64}})
        self.assertEqual((earlier["support_against_u0"], earlier["wording"]),
                         (cells.EARLIER, cells.WORDING["count_control"][cells.SAME]))
        neither = control(**{"U-s32": {}, "U-c32": {}})
        self.assertEqual((neither["ordering"], neither["support_against_u0"], neither["wording"]),
                         (cells.NEITHER, cells.LATER, cells.WORDING["count_control"][cells.NEITHER]))
        later = control(**{"U-s32": {"fail_from": 512}, "U-c32": {"fail_from": 189}})
        self.assertEqual((later["support_against_u0"], later["wording"]),
                         (cells.LATER, cells.WORDING["count_control"][cells.LATER]))
        # The support cell stays in U0's interval: the control's collapse moved, not the support rule's.
        control_earlier = control(**{"U-s32": {"fail_from": 189}, "U-c32": {"fail_from": 64}})
        self.assertEqual((control_earlier["ordering"], control_earlier["support_against_u0"],
                          control_earlier["wording"]), (cells.LATER, cells.SAME, cells.CONTROL_MOVED[cells.LATER]))
        control_later = control(**{"U-s32": {"fail_from": 189}, "U-c32": {"fail_from": 512}})
        self.assertEqual((control_later["ordering"], control_later["support_against_u0"],
                          control_later["wording"]), (cells.EARLIER, cells.SAME, cells.CONTROL_MOVED[cells.EARLIER]))
        both_earlier = control(**{"U-s32": {"fail_from": 128}, "U-c32": {"fail_from": 64}})
        self.assertEqual((both_earlier["ordering"], both_earlier["support_against_u0"]), (cells.LATER, cells.EARLIER))
        self.assertEqual(both_earlier["wording"], 'the support cell collapsed later than the same number of random '
                                                  'rows; against U0 it reads "collapsed earlier"')
        support_earlier = control(**{"U-s32": {"fail_from": 64}, "U-c32": {"fail_from": 512}})
        self.assertEqual((support_earlier["ordering"], support_earlier["support_against_u0"],
                          support_earlier["wording"]),
                         (cells.EARLIER, cells.EARLIER, cells.WORDING["count_control"][cells.EARLIER]))
        out = cells.contrasts(cells.fit_cells(self.spec({"r1": {"U0": {"fail_from": 189, "rerun_hash": "other"},
                                                                "U-s32": {"fail_from": 512},
                                                                "U-c32": {"fail_from": 512}}})))
        broken = out["count_controls"]["U-s32"]["per_lineage"]["r1"]
        self.assertEqual((broken["ordering"], broken["support_against_u0"]), (cells.SAME, cells.NOT_EVIDENCE))
        self.assertIn("same checkpoint interval", broken["wording"])
        self.assertIn(cells.NOT_EVIDENCE, broken["wording"])

    def test_a_contrast_holds_across_lineages_only_with_the_same_ordering_and_wording(self):
        runs = {"r1": {"U0": {"fail_from": 189}, "U-s32": {"fail_from": 189}, "U-c32": {"fail_from": 189}},
                "r2": {"U0": {"fail_from": 64}, "U-s32": {"fail_from": 239}, "U-c32": {"fail_from": 239}}}
        out = cells.contrasts(cells.fit_cells(self.spec(runs)))["count_controls"]["U-s32"]
        self.assertEqual({lin: v["ordering"] for lin, v in out["per_lineage"].items()},
                         {"r1": cells.SAME, "r2": cells.SAME})
        self.assertEqual((out["per_lineage"]["r1"]["wording"], out["per_lineage"]["r2"]["wording"]),
                         (cells.NO_CHANGE, cells.WORDING["count_control"][cells.SAME]))
        self.assertEqual(out["across_lineages"], "lineage-dependent")
        runs["r2"]["U0"] = {"fail_from": 239}
        same = cells.contrasts(cells.fit_cells(self.spec(runs)))["count_controls"]["U-s32"]
        self.assertEqual(same["across_lineages"], cells.SAME)


if __name__ == "__main__":
    unittest.main()
