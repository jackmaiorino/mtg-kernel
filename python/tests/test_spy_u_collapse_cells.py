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
SCHEDULE = [8, 16, 32, 64, 128, 256]


def source(lineage="r1", tag="parent", sha=None):
    return {"play_import": {"path": f"{tag}-import.json", "sha256": "1" * 64},
            "feature_transfer": {"expected_feature_contract_digest": "c", "expected_feature_encoding_digest": "e"},
            "checkpoint": {"path": f"{tag}.json", "sha256": sha or common.PARENT_CHECKPOINT_SHA256[lineage]}}


def readout(cid, g1=(0.1, 0.1), gap=4.0, agree=0.95, kl=0.01, state="s", line_rows=()):
    groups = {"self_chosen": {"rows": 5, "mean_deployed_probability": g1[0]},
              "giant_chosen": {"rows": 5, "mean_deployed_probability": g1[1]},
              "self_offered": {"rows": 12, "mean_deployed_self_probability": 0.05},
              "all": {"rows": 6000, "mean_deployed_probability": 0.4}}
    return {"id": cid, "state_sha256": state,
            "archive": {"training": {"groups": groups, "discrimination": {"self": 0.1, "giant": None},
                                     "tree_chosen_share": {"self": 0.8, "giant": 1.0},
                                     "line_rows": list(line_rows)}},
            "anchor_held_out": {"median_clamped_top_two_gap": gap, "non_spy_top_agreement": agree,
                                "mean_kl_from_parent_non_spy": kl},
            "learner_offers": {"offers": 150, "mean_deployed_self_probability": 0.02,
                               "mean_raw_self_probability": 0.02, "self_top_share": 0.0}}


class Temp(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)


class CellCommands(Temp):
    def files(self):
        games, bank_file = self.root / "games.jsonl", self.root / "bank.json"
        games.write_text('{"id":"g"}\n')
        bank_file.write_text("{}")
        return games, bank_file, {"training": [], "held_out": []}

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
    SCREEN_STEP = 189

    def cell_run(self, lineage, name, per_checkpoint, options=None, steps=None, rerun_hash="h32", kept=()):
        steps = steps or SCHEDULE
        telemetry = [{"anchor_loss": 0.1, "archive_loss": 1.0,
                      "anchor_components": {"all": {"auxiliary_l2": 1.0}},
                      "archive_components": {"all": {"auxiliary_l2": 1.0}}} for _ in range(steps[-1])]
        tpath = self.root / f"{lineage}-{name}-telemetry.json"
        tpath.write_text(json.dumps(telemetry))
        options = {"cell": name, **(options or {})}
        identity = {"lineage": lineage, "arm": "uniform", "recipe": {"checkpoints": steps, "options": options}}
        checkpoints = [{"step": s, "epochs": s / 189, "source": {"path": f"{name}{s}", "sha256": "0" * 64},
                        "state_sha256": f"{lineage}-{name}-{s}"} for s in steps]
        census = {"arm": {"rows": 6030}, "exposure": [{"step": s, "draws_per_row_min": 1} for s in steps]}
        if kept:
            census["kept_line_rows"] = [{"game": g, "step": t} for g, t in kept]
        full = {"schema": fit.PHASE1_SCHEMA, "complete": True, "stop_after_steps": None, "identity": identity,
                "checkpoints": checkpoints, "telemetry": {"path": str(tpath), "sha256": "0" * 64},
                "census": census, "state_hashes": [{"step": 32, "phase1_state_sha256": "h32"}]}
        rerun = {"schema": fit.PHASE1_SCHEMA, "complete": True, "stop_after_steps": 32, "identity": identity,
                 "state_hashes": [{"step": 32, "phase1_state_sha256": rerun_hash}]}
        paths = {}
        for part, value in (("full", full), ("rerun", rerun)):
            paths[part] = self.root / f"{lineage}-{name}-{part}.json"
            paths[part].write_text(json.dumps(value))
        candidates = [readout(f"{name}-step-{s}", state=f"{lineage}-{name}-{s}", **kw)
                      for s, kw in zip(steps, per_checkpoint)]
        return {k: str(v) for k, v in paths.items()}, candidates

    def spec(self, runs: dict, split=False, screen_state=None):
        spec = {}
        screen = {"fits": {}}
        for lineage, named in runs.items():
            references = [readout("parent", gap=4.0), readout("O-final", agree=0.95, kl=0.02)]
            spec[lineage] = {"cells": {}, "probes": []}
            groups = [[]]
            for name, args in named.items():
                paths, candidates = self.cell_run(lineage, name, *args[:1], **args[1])
                spec[lineage]["cells"][name] = paths
                if split and len(groups[-1]) >= 1:
                    groups.append([])
                groups[-1] += candidates
            for index, group in enumerate(groups):
                probe = self.root / f"{lineage}-probe-{index}.json"
                probe.write_text(json.dumps({"schema": fit.PROBE_SCHEMA, "complete": True, "lineage": lineage,
                                             "candidates": references + group}))
                spec[lineage]["probes"].append(str(probe))
            screen["fits"][lineage] = {"U": {"read": [{"step": self.SCREEN_STEP, "state_sha256":
                                                      screen_state or f"{lineage}-U0-{self.SCREEN_STEP}"}]}}
        path = self.root / "screen-fit.json"
        path.write_text(json.dumps(screen))
        spec["screen_fit"] = str(path)
        return spec

    U0_STEPS = [8, 16, 32, 64, 128, 189, 256]

    def u0(self, fail_at=189):
        per = [{"gap": 1.0} if s >= fail_at else {} for s in self.U0_STEPS]
        return (per, {"steps": self.U0_STEPS})

    def test_cells_read_to_their_first_g2_failure_and_select_as_the_screen_does(self):
        good = [{}, {"g1": (0.6, 0.6)}, {"g1": (0.7, 0.7)}, {"g1": (0.8, 0.8)}, {"kl": 0.5}, {}]
        result = cells.fit_cells(self.spec({"r1": {"U0": self.u0(), "U-a4": (good, {"options":
                                                                                      {"anchor_coefficient": 4}})}}))
        a4 = result["fits"]["r1"]["U-a4"]
        self.assertEqual((a4["selected"]["step"], a4["fit_dose"], a4["stopped_at"]), (64, 16, 128))
        self.assertEqual(a4["unread_checkpoints"], [256])
        self.assertEqual(a4["reading"], "required fit without collapse in this cell")
        u0 = result["fits"]["r1"]["U0"]
        self.assertEqual((u0["stopped_at"], u0["reading"]), (189, fit.READINGS["collapse_first"][0]))
        self.assertEqual(result["u0_reproduces_screen"]["r1"]["equal"], True)
        self.assertEqual(result["passes"], {"U-a4": ["r1"], "U0": []})

    def test_a_cell_holding_g2_without_the_fit_reads_not_reached_by_its_last_checkpoint(self):
        result = cells.fit_cells(self.spec({"r1": {"U0": self.u0(), "U-a16": ([{}] * 6, {})}}))
        a16 = result["fits"]["r1"]["U-a16"]
        self.assertEqual((a16["reading"], a16["explanations"]), cells.NOT_FIT)
        self.assertGreaterEqual(len(cells.NOT_FIT[1]), 2)

    def test_split_probes_must_agree_on_the_references_and_u0_must_reproduce_the_screen(self):
        runs = {"r1": {"U0": self.u0(), "U-lr": ([{}] * 6, {})}}
        result = cells.fit_cells(self.spec(runs, split=True))
        self.assertEqual(len(result["fits"]["r1"]), 2)
        spec = self.spec(runs, split=True)
        probe = Path(spec["r1"]["probes"][1])
        value = json.loads(probe.read_text())
        value["candidates"][0]["anchor_held_out"]["median_clamped_top_two_gap"] = 4.5
        probe.write_text(json.dumps(value))
        with self.assertRaisesRegex(ValueError, "differ between probes"):
            cells.fit_cells(spec)
        with self.assertRaisesRegex(ValueError, "does not reproduce the screen"):
            cells.fit_cells(self.spec(runs, screen_state="other"))
        with self.assertRaisesRegex(ValueError, "is cell"):
            spec = self.spec(runs)
            spec["r1"]["cells"]["U-a4"] = spec["r1"]["cells"].pop("U-lr")
            cells.fit_cells(spec)

    def test_subset_cells_split_the_banks_line_rows_into_kept_and_dropped(self):
        line_rows = [{"game": "g", "step": 1, "chosen": True, "self_offered": True, "giant_offered": False,
                      "deployed_probability": 0.9},
                     {"game": "g", "step": 2, "chosen": True, "self_offered": True, "giant_offered": False,
                      "deployed_probability": 0.1},
                     {"game": "g", "step": 3, "chosen": False, "self_offered": True, "giant_offered": False,
                      "deployed_probability": 0.5}]
        per = [{"line_rows": line_rows}] * 6
        result = cells.fit_cells(self.spec({"r1": {"U0": self.u0(), "U-s256": (per, {
            "options": {"min_edge_backups": 256}, "kept": [("g", 1)]})}}))
        s256 = result["fits"]["r1"]["U-s256"]
        split = s256["read"][0]["line_rows_kept_and_dropped"]
        self.assertEqual(split["self_kept"], {"rows": 1, "mean_deployed_probability": 0.9})
        self.assertEqual(split["self_dropped"], {"rows": 1, "mean_deployed_probability": 0.1})
        self.assertEqual(split["giant_kept"]["rows"], 0)
        self.assertEqual(s256["kept_line_rows"], 1)
        self.assertIsNone(result["fits"]["r1"]["U0"]["read"][0]["line_rows_kept_and_dropped"])

    def test_contrasts_compare_each_cell_with_u0_and_each_support_cell_with_its_control(self):
        later = [{}] * 4 + [{"gap": 1.0}, {}]
        runs = {lin: {"U0": self.u0(fail_at=64), "U-a4": (later, {}), "U-lr": ([{}] * 6, {}),
                      "U-s32": (later, {"options": {"min_edge_backups": 32}}),
                      "U-c32": ([{}, {}, {"gap": 1.0}, {}, {}, {}], {"options": {"count_control": 3281}})}
                for lin in ("r1", "r2")}
        out = cells.contrasts(cells.fit_cells(self.spec(runs)))
        a4 = out["against_u0"]["U-a4"]
        self.assertEqual(a4["per_lineage"]["r1"]["reading"], "held G2 longer than the reference")
        self.assertEqual(a4["across_lineages"], "held G2 longer than the reference")
        self.assertEqual([g["step"] for g in a4["per_lineage"]["r1"]["g2_at_shared_steps"]], [8, 16, 32, 64])
        self.assertEqual(out["against_u0"]["U-lr"]["per_lineage"]["r1"]["reading"],
                         "held G2 through its schedule, where the reference had failed")
        control = out["count_controls"]["U-s32"]
        self.assertEqual((control["control"], control["across_lineages"]),
                         ("U-c32", "held G2 longer than the reference"))
        pairs = out["matched_summed_step_size"]["r1"]
        self.assertEqual([(p["u0_step"], p["u_lr_step"]) for p in pairs], [(8, 32), (16, 64), (32, 128), (64, 256)])
        runs["r2"]["U-a4"] = self.u0(fail_at=64)
        runs["r2"]["U-a4"][1]["steps"] = self.U0_STEPS
        mixed = cells.contrasts(cells.fit_cells(self.spec(runs)))
        self.assertEqual(mixed["against_u0"]["U-a4"]["across_lineages"], "lineage-dependent")


if __name__ == "__main__":
    unittest.main()
