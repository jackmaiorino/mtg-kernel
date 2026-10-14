"""Spy archive distillation run tools: schedules, archive projection, commands,
phase-2 opponent mapping and the fit check. Pure; nothing is played."""
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools.spy_distill import bank, fit, phase1, phase2, sets  # noqa: E402
from tools.spy_distill import distill_common as common  # noqa: E402

pilot = common.pilot
DECKS = pilot.load_decks()


def source(lineage="r1", checkpoint=True, tag="parent"):
    value = {"play_import": {"path": f"{tag}-import.json", "sha256": "1" * 64},
             "feature_transfer": {"expected_feature_contract_digest": "c", "expected_feature_encoding_digest": "e"}}
    value["checkpoint"] = ({"path": f"{tag}.json", "sha256": common.PARENT_CHECKPOINT_SHA256[lineage]}
                           if checkpoint else None)
    return value


A48 = {**source(tag="a48"), "checkpoint": {"path": "a48.json", "sha256": "4" * 64}}


class Temp(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)


class Schedules(unittest.TestCase):
    def test_anchor_pool_takes_four_ranked_units_per_pair_and_holds_out_the_fourth(self):
        for lineage in ("r1", "r2"):
            units = sets.anchor_units(lineage)
            self.assertEqual(len(units), 81)
            for (o, t), js in units.items():
                ranked = sorted(range(10), key=lambda j: pilot.sha([common.ANCHOR_NS, lineage, o, t, j]))
                self.assertEqual(js, ranked[:4])
            train = sets.schedule(lineage, "anchor-training")
            held = sets.schedule(lineage, "anchor-held-out")
            self.assertEqual((len(train), len(held)), (486, 162))
            self.assertEqual({(e["own"], e["other"], e["j"]) for e in held},
                             {(o, t, js[3]) for (o, t), js in units.items()})
            for e in train + held:
                self.assertEqual(e["seed"], pilot.seed([common.ANCHOR_NS + "/anchor", lineage, e["own"], e["other"],
                                                        e["j"]]))
                self.assertEqual(e["starting_player"], (e["j"] + e["own"] + e["other"]) % 2)
                self.assertEqual(e["opponent"], "a48" if e["j"] >= 8 else "parent")
        self.assertNotEqual(sets.anchor_units("r1"), sets.anchor_units("r2"))

    def test_offer_set_plays_spy_against_every_deck_twenty_times_in_both_seats(self):
        offers = sets.schedule("r2", "offers")
        self.assertEqual(len(offers), 360)
        for e in offers:
            self.assertEqual(e["own"], pilot.SPY)
            self.assertEqual(e["seed"], pilot.seed([common.OFFERS_NS, "r2", e["other"], e["j"]]))
            self.assertEqual(e["starting_player"], (e["j"] + 4 + e["other"]) % 2)
            self.assertEqual(e["opponent"], "a48" if e["j"] % 10 >= 8 else "parent")
        ids = [e["id"] for lineage in ("r1", "r2") for name in sets.SETS for e in sets.schedule(lineage, name)]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertTrue(all(len(i) <= 128 for i in ids))

    def test_collect_command_pins_the_parent_as_learner_and_each_opponent_by_j(self):
        parent = source("r1")
        command = sets.collect_command("r1", "anchor-held-out", DECKS, parent, A48, 8, "D:/out")
        self.assertEqual((command["mode"], command["workers"], command["source"]), ("collect_parallel", 8, parent))
        self.assertEqual(command["max_non_natural_episode_fraction"], 0.2)
        for e, s in zip(command["episodes"], sets.schedule("r1", "anchor-held-out")):
            self.assertEqual(e["opponent"], A48 if s["j"] >= 8 else parent)
            learner = e["selected"][e["learner_seat"]]
            self.assertEqual(learner, DECKS[s["own"]])
            self.assertEqual(e["selected"][1 - e["learner_seat"]], DECKS[s["other"]])
            self.assertEqual(e["runtime_rules"], "resolution-boundary-v1")
            self.assertNotIn("reset", e)
        serial = sets.collect_command("r1", "offers", DECKS, parent, A48, 1, "out")
        self.assertEqual(serial["mode"], "collect")
        self.assertNotIn("workers", serial)
        with self.assertRaisesRegex(ValueError, "not the r2 parent"):
            sets.collect_command("r2", "offers", DECKS, parent, A48, 1, "out")


class Collected(Temp):
    def test_receipts_must_hold_the_scheduled_episodes_in_order(self):
        parent = source("r1")
        schedule = sets.schedule("r1", "anchor-held-out")
        pins = []
        for i, e in enumerate(schedule):
            path = self.root / f"episode-{i:04d}.json"
            path.write_text(json.dumps({"episode": {"id": e["id"], "learner_seat": e["seat"]}}))
            pins.append(common.pin(path))
        receipt = {"schema": sets.COLLECTION_SCHEMA, "complete": True, "source": parent, "trajectories": pins}
        path = self.root / "collection.json"
        path.write_text(json.dumps(receipt))
        self.assertEqual(sets.collected(path, "r1", "anchor-held-out", parent), pins)
        swapped = dict(receipt, trajectories=[pins[1], pins[0], *pins[2:]])
        path.write_text(json.dumps(swapped))
        with self.assertRaisesRegex(ValueError, "episode differs"):
            sets.collected(path, "r1", "anchor-held-out", parent)
        path.write_text(json.dumps(dict(receipt, trajectories=pins[:-1])))
        with self.assertRaisesRegex(ValueError, "161 trajectories"):
            sets.collected(path, "r1", "anchor-held-out", parent)


DECISION = {"step": 3, "physical_decision_id": 2, "substep": 0, "k": 4, "chooser": "reused", "search": 0,
            "lookup": "hit", "refreshed": None, "edge_backups": 17, "remaining_before": 9, "remaining_after": 9,
            "spy_cast_offered": True, "spy_cast_chosen": False, "spy_self_offered": False, "spy_self_chosen": False,
            "dr_giant_offered": False, "dr_giant_chosen": False}
START = {"step": 1, "physical_decision_id": 1, "k": 1, "obs_hash": "o", "menu_hash": "m", "state_hash": "s"}


class Archive(Temp):
    def write(self, rows, records):
        archive = self.root / "ARCHIVE.jsonl"
        archive.write_text("".join(json.dumps(r) + "\n" for r in rows))
        frozen = self.root / "rows-frozen"
        frozen.mkdir(exist_ok=True)
        for name, items in records.items():
            (frozen / name).write_text("".join(json.dumps(r) + "\n" for r in items))
        return archive, frozen, hashlib.sha256(archive.read_bytes()).hexdigest()

    @staticmethod
    def game(gid, lineage="r1", usable=True, other=6, seat=1):
        return {"id": gid, "lineage": lineage, "own": 4, "other": other, "seat": seat, "seed": 2**64 - 5,
                "starting_player": 0, "actions": [0, 1, 0, 2], "starts": [dict(START, extra=1)], "usable": usable,
                "sequence": {"complete": False}}

    @staticmethod
    def record(arch, **game):
        value = {"valid": True, "end": "loss", "actions": arch["actions"], "decisions": [DECISION],
                 "learner_starts": 1, "searches": [{"big": True}]}
        value.update(game)
        return {"kind": "budget_game", **{k: arch[k] for k in ("id", "lineage", "own", "other", "seat", "seed",
                                                               "starting_player")}, "game": value}

    def test_usable_lineage_games_project_to_archive_games_and_pin_their_sources(self):
        games = [self.game("archive-r1-k0-v6-s1"), self.game("archive-r1-k0-v7-s0", other=7, seat=0),
                 self.game("archive-r2-k0-v6-s1", lineage="r2"), self.game("archive-r1-k1-v6-s1", usable=False)]
        archive, frozen, sha = self.write(games, {"a-r1-1.jsonl": [self.record(games[0])],
                                                  "a-r1-2.jsonl": [self.record(games[1])],
                                                  "a-r2-1.jsonl": [self.record(games[2])]})
        out = bank.build("r1", archive, frozen, source("r1"), self.root / "out", "D:/bank-r1", sha, DECKS)
        self.assertEqual(out["usable_games"], 2)
        lines = [json.loads(x) for x in (self.root / "out" / "games-r1.jsonl").read_text().splitlines()]
        self.assertEqual([g["id"] for g in lines], ["archive-r1-k0-v6-s1", "archive-r1-k0-v7-s0"])
        first = lines[0]
        self.assertEqual(set(first), {"id", "lineage", "episode", "actions", "starts", "decisions", "end"})
        self.assertEqual(first["starts"], [START])
        self.assertEqual(first["decisions"], [{k: DECISION[k] for k in bank.DECISION_FIELDS}])
        self.assertEqual(first["end"], "loss")
        episode = first["episode"]
        self.assertEqual((episode["seed"], episode["starting_player"], episode["learner_seat"]), (2**64 - 5, 0, 1))
        self.assertEqual(episode["selected"], [DECKS[6], DECKS[4]])
        self.assertNotIn("opponent", episode)
        command = json.loads((self.root / "out" / "bank-r1.command.json").read_text())
        self.assertEqual(command["mode"], "archive_distillation_bank")
        self.assertEqual([Path(s["path"]).name for s in command["sources"]],
                         ["ARCHIVE.jsonl", "a-r1-1.jsonl", "a-r1-2.jsonl"])
        # The bank pins the games file by its resolved path, which on Windows
        # spells out short names such as RUNNER~1.
        self.assertEqual(command["games"],
                         common.pin((self.root / "out" / "games-r1.jsonl").resolve()))

    def test_inputs_that_disagree_are_refused(self):
        arch = self.game("archive-r1-k0-v6-s1")
        cases = [({}, "no budget_game record"),
                 ({"a.jsonl": [self.record(arch)], "b.jsonl": [self.record(arch, end="win")]}, "conflicting"),
                 ({"a.jsonl": [self.record(arch, actions=[0, 1, 0])]}, "actions differ"),
                 ({"a.jsonl": [self.record(arch, learner_starts=2)]}, "start count differs"),
                 ({"a.jsonl": [self.record(arch, valid=False)]}, "not a valid natural game"),
                 ({"a.jsonl": [dict(self.record(arch), seed=3)]}, "seed differs")]
        for records, message in cases:
            with self.subTest(message=message):
                for f in (self.root / "rows-frozen").glob("*") if (self.root / "rows-frozen").exists() else ():
                    f.unlink()
                archive, frozen, sha = self.write([arch], records)
                with self.assertRaisesRegex(ValueError, message):
                    bank.build("r1", archive, frozen, source("r1"), self.root / "out", "x", sha, DECKS)
        archive, frozen, _ = self.write([arch], {"a.jsonl": [self.record(arch)]})
        with self.assertRaisesRegex(ValueError, "frozen SHA-256"):
            bank.build("r1", archive, frozen, source("r1"), self.root / "out", "x", "0" * 64, DECKS)


class Commands(Temp):
    def test_phase1_commands_carry_the_arm_and_the_rerun_and_dry_run_flags(self):
        games, bank_file = self.root / "games.jsonl", self.root / "bank.json"
        games.write_text('{"id":"g"}\n')
        bank_file.write_text("{}")
        anchor = {"training": [{"path": "t", "sha256": "0" * 64}], "held_out": [{"path": "h", "sha256": "0" * 64}]}
        full = phase1.phase1_command("r1", "L", source("r1"), games, bank_file, anchor, "D:/p1")
        self.assertEqual(full["arm"], "line_decisions")
        self.assertNotIn("stop_after_steps", full)
        self.assertNotIn("dry_run", full)
        self.assertEqual(phase1.phase1_command("r1", "U", source("r1"), games, bank_file, anchor, "d",
                                               rerun=True)["stop_after_steps"], 32)
        self.assertIs(phase1.phase1_command("r1", "U", source("r1"), games, bank_file, anchor, "d",
                                            dry_run=True)["dry_run"], True)

    def test_probe_reads_parent_o_final_and_every_checkpoint_of_complete_runs(self):
        games, bank_file, other = self.root / "games.jsonl", self.root / "bank.json", self.root / "c-games.jsonl"
        games.write_text('{"id":"g1"}\n{"id":"g2"}\n')
        bank_file.write_text("{}")
        records = []
        for arm, wire in (("U", "uniform"), ("L", "line_decisions")):
            checkpoints = []
            for step in (32, 64):
                path = self.root / f"{arm}-source-{step}.json"
                path.write_text(json.dumps({**source("r1"), "checkpoint": {"path": f"{arm}{step}", "sha256": "9" * 64}}))
                checkpoints.append({"step": step, "source": common.pin(path)})
            records.append({"schema": phase1.PHASE1_SCHEMA, "complete": True, "stop_after_steps": None,
                            "identity": {"lineage": "r1", "arm": wire}, "checkpoints": checkpoints})
        o_final = {**source("r1"), "checkpoint": {"path": "o.json", "sha256": "8" * 64}}
        command = phase1.probe_command("r1", source("r1"), o_final, games, bank_file, {}, [], records, "d")
        self.assertEqual([c["id"] for c in command["candidates"]],
                         ["parent", "O-final", "U-step-32", "U-step-64", "L-step-32", "L-step-64"])
        self.assertEqual([b["role"] for b in command["banks"]], ["training"])
        other.write_text('{"id":"g2"}\n')
        with self.assertRaisesRegex(ValueError, "also a training game"):
            phase1.probe_command("r1", source("r1"), o_final, games, bank_file, {}, [], records, "d",
                                 (other, bank_file))
        rerun = dict(records[0], stop_after_steps=32)
        with self.assertRaisesRegex(ValueError, "complete full phase-1 runs"):
            phase1.probe_command("r1", source("r1"), o_final, games, bank_file, {}, [], [rerun], "d")


def o_config(lineage="r1"):
    """O's recorded config as tools/spy_reset/archive_pilot.py wrote it."""
    its, _ = pilot.iterations(lineage, "O", DECKS, None)
    return {"schema": pilot.ndb.SCHEMA, "initial_source": source(lineage),
            "opponents": [{"id": pilot.FIXED_OPPONENT_ID, "source": A48}], "iterations": its,
            **json.loads(json.dumps(pilot.ndb.FROZEN)), "update_backend": {"kind": "cpu"},
            "collection_workers": 8, "preparation_workers": 8, "output_directory": "D:/archive-reset/r1-O"}


class PhaseTwo(Temp):
    def test_the_named_parent_resolves_to_the_same_source_as_os_initial_opponent(self):
        recorded = o_config()
        distilled = {**source("r1"), "checkpoint": {"path": "p1/checkpoint-step-1024.json", "sha256": "7" * 64}}
        for start in (None, distilled):
            value = phase2.config("r1", recorded, start, 4, "D:/p2", DECKS)
            self.assertEqual(value["initial_source"], start or recorded["initial_source"])
            self.assertEqual(value["opponents"][:1], recorded["opponents"])
            self.assertEqual(value["opponents"][1], {"id": "parent", "source": recorded["initial_source"]})
            named = 0
            for old_update, new_update in zip(recorded["iterations"], value["iterations"]):
                for old, new in zip(old_update["episodes"], new_update["episodes"]):
                    self.assertEqual(old["episode"], new["episode"])
                    current = {"checkpoint": "the run's current checkpoint"}
                    before = phase2.resolved_opponent(recorded, old["opponent"], current)
                    after = phase2.resolved_opponent(value, new["opponent"], current)
                    self.assertEqual(after, before)
                    if old["opponent"] == {"kind": "initial"}:
                        named += 1
                        self.assertEqual(new["opponent"], {"kind": "fixed", "id": "parent"})
                        self.assertEqual(json.dumps(after), json.dumps(recorded["initial_source"]))
                    else:
                        self.assertEqual(new["opponent"], old["opponent"])
            self.assertEqual(named, 810)
            unchanged = {k: v for k, v in value.items()
                         if k not in ("iterations", "opponents", "initial_source", "collection_workers",
                                      "preparation_workers", "output_directory")}
            self.assertEqual(unchanged, {k: recorded[k] for k in unchanged})

    def test_o_configs_off_the_frozen_schedule_are_refused(self):
        changed = o_config()
        changed["iterations"][0]["episodes"][0]["episode"]["seed"] += 1
        with self.assertRaisesRegex(ValueError, "schedule differs"):
            phase2.config("r1", changed, None, 4, "d", DECKS)
        changed = o_config()
        changed["value_coefficient"] = 0.25
        with self.assertRaisesRegex(ValueError, "value_coefficient"):
            phase2.config("r1", changed, None, 4, "d", DECKS)
        with self.assertRaisesRegex(ValueError, "not the r2 parent"):
            phase2.config("r2", o_config("r1"), None, 4, "d", DECKS)
        moved = {**source("r1", tag="other"), "checkpoint": {"path": "x", "sha256": "7" * 64}}
        with self.assertRaisesRegex(ValueError, "import or feature transfer"):
            phase2.config("r1", o_config(), moved, 4, "d", DECKS)

    def test_reproduction_check_compares_the_final_state_with_os_record(self):
        recorded = self.root / "o-final.json"
        recorded.write_text(json.dumps({"state_sha256": "f" * 64, "adam_step": 35_802}))
        final = self.root / "final.json"
        final.write_text(json.dumps({"state_sha256": "f" * 64, "adam_step": 35_802}))
        completion = {"schema": pilot.ndb.SCHEMA, "complete": True, "completed_iterations": 162,
                      "source": {"checkpoint": common.pin(final)}}
        sha = hashlib.sha256(recorded.read_bytes()).hexdigest()
        with mock.patch.dict(phase2.O_FINAL_CHECKPOINT_SHA256, {"r1": sha}):
            self.assertTrue(phase2.check("r1", completion, recorded)["reproduced"])
            final.write_text(json.dumps({"state_sha256": "e" * 64, "adam_step": 35_802}))
            completion["source"]["checkpoint"] = common.pin(final)
            self.assertFalse(phase2.check("r1", completion, recorded)["reproduced"])
        with self.assertRaisesRegex(ValueError, "recorded r1 final checkpoint"):
            phase2.check("r1", completion, recorded)


def readout(cid, g1=(0.1, 0.1), gap=4.0, agree=0.95, kl=0.01, share=(0.4, 1.0), discrimination=0.1,
            self_offered=0.05, offers=150, offer_p=0.02, state="s"):
    groups = {"self_chosen": {"rows": 5, "mean_deployed_probability": g1[0]},
              "giant_chosen": {"rows": 5, "mean_deployed_probability": g1[1]},
              "self_offered": {"rows": 12, "mean_deployed_self_probability": self_offered},
              "all": {"rows": 5700, "mean_deployed_probability": 0.6},
              "line_offered": {"rows": 30, "mean_deployed_probability": 0.4}}
    return {"id": cid, "state_sha256": state,
            "archive": {"training": {"groups": groups, "discrimination": {"self": discrimination, "giant": None},
                                     "tree_chosen_share": {"self": share[0], "giant": share[1]},
                                     "line_rows": []}},
            "anchor_held_out": {"median_clamped_top_two_gap": gap, "non_spy_top_agreement": agree,
                                "mean_kl_from_parent_non_spy": kl},
            "learner_offers": {"offers": offers, "mean_deployed_self_probability": offer_p,
                               "mean_raw_self_probability": offer_p, "self_top_share": 0.0}}


class FitCheck(Temp):
    STEPS = {"U": [179, 358, 716, 1432, 2864, 5728], "L": [32, 64, 128, 256, 512, 1024]}

    def phase1_run(self, lineage, arm, per_checkpoint, rerun_hash="h32", nan_step=None):
        telemetry = [{"anchor_loss": 0.1, "archive_loss": 1.0 if i + 1 != nan_step else None,
                      "anchor_components": {"all": {"auxiliary_l2": 1.0}},
                      "archive_components": {"all": {"auxiliary_l2": 1.0}}} for i in range(self.STEPS[arm][-1])]
        tpath = self.root / f"{lineage}-{arm}-telemetry.json"
        tpath.write_text(json.dumps(telemetry))
        identity = {"lineage": lineage, "arm": common.ARMS[arm], "binary": "b"}
        checkpoints = [{"step": s, "epochs": i, "source": {"path": f"{arm}{s}", "sha256": "0" * 64},
                        "state_sha256": f"{arm}{s}"} for i, s in enumerate(self.STEPS[arm])]
        full = {"schema": fit.PHASE1_SCHEMA, "complete": True, "stop_after_steps": None, "identity": identity,
                "checkpoints": checkpoints, "telemetry": {"path": str(tpath), "sha256": "0" * 64},
                "state_hashes": [{"step": 32, "phase1_state_sha256": "h32"}]}
        rerun = {"schema": fit.PHASE1_SCHEMA, "complete": True, "stop_after_steps": 32, "identity": identity,
                 "state_hashes": [{"step": 32, "phase1_state_sha256": rerun_hash}]}
        paths = {}
        for name, value in (("full", full), ("rerun", rerun)):
            paths[name] = self.root / f"{lineage}-{arm}-{name}.json"
            paths[name].write_text(json.dumps(value))
        candidates = [readout(f"{arm}-step-{s}", state=f"{arm}{s}", **kw)
                      for s, kw in zip(self.STEPS[arm], per_checkpoint)]
        return paths, candidates

    def fit(self, runs: dict):
        spec = {}
        for lineage, arms in runs.items():
            candidates = [readout("parent", gap=4.0), readout("O-final", agree=0.95, kl=0.02)]
            spec[lineage] = {"runs": {}}
            for arm, (per, kw) in arms.items():
                paths, c = self.phase1_run(lineage, arm, per, **kw)
                spec[lineage]["runs"][arm] = {k: str(v) for k, v in paths.items()}
                candidates += c
            probe = self.root / f"{lineage}-probe.json"
            probe.write_text(json.dumps({"schema": fit.PROBE_SCHEMA, "complete": True, "lineage": lineage,
                                         "candidates": candidates}))
            spec[lineage]["probe"] = str(probe)
        return fit.fit(spec)

    def test_selects_the_highest_dose_passing_checkpoint_before_the_stop(self):
        per = [{}, {"g1": (0.6, 0.7)}, {"g1": (0.7, 0.8)}, {"g1": (0.8, 0.9)}, {"g1": (0.9, 0.9), "gap": 1.0}, {}]
        result = self.fit({"r1": {"U": (per, {})}})["fits"]["r1"]["U"]
        self.assertEqual(result["selected"]["step"], 1432)
        self.assertEqual(result["fit_dose"], 358)
        self.assertEqual(result["stopped_at"], 2864)
        self.assertEqual(result["unread_checkpoints"], [5728])
        self.assertEqual(result["reading"], "passes the fit check")
        self.assertEqual(result["explanations"], [])

    def test_each_arm_reports_fit_on_its_own_training_rows(self):
        result = self.fit({"r1": {"U": ([{}] * 6, {}), "L": ([{}] * 6, {})}})["fits"]["r1"]
        self.assertEqual({r["training_rows"]["rows"] for r in result["U"]["read"]}, {5700})
        self.assertEqual({r["training_rows"]["rows"] for r in result["L"]["read"]}, {30})

    def test_fit_readings_follow_the_first_matching_row(self):
        held = [{}] * 6
        cases = {
            "mixed": ("U", [dict(g1=(0.3, 0.9), self_offered=0.25)] * 6, {}),
            "fit_with_collapse": ("U", [{}, {}, dict(g1=(0.6, 0.6), kl=0.05)] + [{}] * 3, {}),
            "collapse_first": ("L", [{}, dict(agree=0.9)] + [{}] * 4, {}),
            "U": ("U", held, {}),
            "L": ("L", held, {}),
            "engineering": ("U", [dict(g1=(0.9, 0.9))] * 6, {"rerun_hash": "other"}),
        }
        for row, (arm, per, kw) in cases.items():
            with self.subTest(row=row):
                result = self.fit({"r1": {arm: (per, kw)}})["fits"]["r1"][arm]
                self.assertIsNone(result["selected"])
                expected = (fit.ENGINEERING, []) if row == "engineering" else fit.READINGS[row]
                self.assertEqual((result["reading"], result["explanations"]), (expected[0], expected[1]))
        # No reading names a single failing component: each failure keeps its
        # competing explanations (amendment, DESIGN.md section 7).
        for row, (text, explanations) in fit.READINGS.items():
            self.assertGreaterEqual(len(explanations), 2, row)
            for word in ("teacher split", "cannot absorb", "representation", "diluted", "flattens"):
                self.assertNotIn(word, text)
        # A non-finite loss before the checkpoint fails G3 there.
        result = self.fit({"r1": {"L": ([dict(g1=(0.9, 0.9))] * 6, {"nan_step": 100})}})["fits"]["r1"]["L"]
        self.assertEqual(result["selected"]["step"], 64)
        self.assertEqual(result["stopped_at"], 128)

    def test_the_mixed_choices_row_needs_declines_low_discrimination_and_movement(self):
        split = dict(g1=(0.3, 0.9), self_offered=0.25)
        for name, change in (("tree rarely declines", dict(share=(0.8, 1.0), self_offered=0.45)),
                             ("policy discriminates", dict(discrimination=0.3)),
                             ("policy did not move", dict(self_offered=0.15))):
            with self.subTest(name):
                per = [dict(split, **change)] * 6
                result = self.fit({"r1": {"U": (per, {})}})["fits"]["r1"]["U"]
                self.assertEqual(result["reading"], fit.READINGS["U"][0])

    def test_lineage_dependence_and_offer_pooling(self):
        good = [dict(g1=(0.9, 0.9), offers=80, offer_p=0.1)] * 6
        result = self.fit({"r1": {"U": (good, {})}, "r2": {"U": ([{}] * 6, {})}})
        self.assertEqual(result["passes"]["U"], ["r1"])
        self.assertTrue(result["lineage_dependent"]["U"])
        result = self.fit({"r1": {"U": (good, {})}, "r2": {"U": ([dict(g1=(0.9, 0.9), offers=120, offer_p=0.3)] * 6,
                                                                 {})}})
        self.assertFalse(result["lineage_dependent"]["U"])
        pooled = result["learner_offers"]["U"]["pooled"]
        self.assertEqual(pooled["offers"], 200)
        self.assertAlmostEqual(pooled["mean_deployed_self_probability"], (80 * 0.1 + 120 * 0.3) / 200)


if __name__ == "__main__":
    unittest.main()
