"""Search-evidence distillation (tools/spy_distill/sd1.py): fixture tests. Nothing is searched,
played or trained; every schedule, record, replay, bank, probe and run here is synthetic."""
import copy
import hashlib
import json
import math
import struct
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools.spy_distill import sd1  # noqa: E402
from tools.spy_distill import distill_common as common  # noqa: E402

NAMES = [f"Deck{i}/{i:012x}" for i in range(9)]
DECKS = [{"label": name} for name in NAMES]
PARENT = {"checkpoint": {"path": "parent.json", "sha256": common.PARENT_CHECKPOINT_SHA256["r1"]}}
START = {"physical_decision_id": 1, "k": 3, "obs_hash": "o", "menu_hash": "m", "state_hash": "s"}


def f32_bits(values) -> list:
    return [struct.unpack("<I", struct.pack("<f", v))[0] for v in values]


def write(path: Path, value) -> dict:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")
    return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def write_lines(path: Path, rows) -> dict:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
    return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def schedule(lineage="r1") -> list:
    """Nine decks, 81 ordered matchups, both seats sharing one seed."""
    return [{"id": f"formal-{lineage}-o{own}-v{other}-s{seat}", "own": own, "other": other, "seat": seat,
             "opp": "T1" if (own + other) % 2 == 0 else "A48", "seed": 1000 + 9 * own + other,
             "starting_player": seat, "source_policy_seeds": [1, 2], "anchor_draw": 0,
             "guide_policy_seeds": [3, 4], "search_root": f"{lineage}-root"}
            for own in range(9) for other in range(9) for seat in (0, 1)]


class Temp(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()


# ------------------------------------------------------------------ split


class Split(unittest.TestCase):
    def test_two_matchups_per_focal_deck_are_held_out_with_both_seats(self):
        cases = schedule()
        out = sd1.split("r1", cases)
        self.assertEqual(out["counts"], {"training": 126, "held_out": 36})
        for own in range(9):
            order = out["decks"][str(own)]["order"]
            keys = [sd1.holdout_key("r1", own, o["other"]) for o in order]
            self.assertEqual(keys, sorted(keys))
            self.assertEqual(sorted(o["other"] for o in order[:2]), out["decks"][str(own)]["held_out"])
        for c in cases:
            side = out["games"][c["id"]]
            twin = f"{c['id'][:-1]}{1 - c['seat']}"
            self.assertEqual(out["games"][twin], side)
            self.assertEqual(side == "held_out", c["other"] in out["decks"][str(c["own"])]["held_out"])
            self.assertEqual(out["matchups"][c["id"]], [c["own"], c["other"]])

    def test_the_hash_input_is_the_designs_utf8_string(self):
        self.assertEqual(sd1.holdout_key("r2", 4, 7),
                         hashlib.sha256("sd1-evidence-holdout-v1|r2|4|7".encode("utf-8")).hexdigest())
        self.assertNotEqual(sd1.split("r1", schedule("r1"))["games"],
                            {k.replace("r2", "r1"): v for k, v in sd1.split("r2", schedule("r2"))["games"].items()})

    def test_the_held_out_count_is_a_parameter_and_seats_stay_together(self):
        self.assertEqual(sd1.split("r1", schedule(), 3)["counts"], {"training": 108, "held_out": 54})
        cases = schedule()
        cases[1] = {**cases[1], "seed": 7}
        with self.assertRaisesRegex(ValueError, "two seats of one seed"):
            sd1.split("r1", cases)

    def test_the_cli_reads_the_schedule_file(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "formal-r1.jsonl"
            write_lines(path, schedule())
            sd1.main(["split", "--lineage", "r1", "--schedule", str(path), "--out", str(Path(d) / "S.json")])
            self.assertEqual(json.loads((Path(d) / "S.json").read_text())["counts"]["held_out"], 36)
            with self.assertRaisesRegex(ValueError, "not the r2 schedule"):
                sd1.schedule_cases(path, "r2")


# ---------------------------------------------------------------- records


def record(case, lineage="r1", actions=(0, 1, 0, 2), end="win"):
    return {"kind": "budget_game", "schema": "search-total-budget-v1/game", "id": case["id"], "lineage": lineage,
            "own": case["own"], "other": case["other"], "seat": case["seat"], "opp": case["opp"],
            "seed": case["seed"], "starting_player": case["starting_player"],
            "live_policy_seeds": case["guide_policy_seeds"], "search_root": case["search_root"],
            "game": {"valid": True, "end": end, "actions": list(actions), "learner_starts": 2,
                     "decisions": [{"step": 1, "physical_decision_id": 1, "substep": 0, "k": 3,
                                    "chooser": "new_root", "edge_backups": 10, "spy_cast_offered": False,
                                    "spy_cast_chosen": False, "spy_self_offered": False, "spy_self_chosen": False,
                                    "dr_giant_offered": False, "dr_giant_chosen": False, "search": 0,
                                    "refreshed": "initial"}]}}


class Records(Temp):
    def files(self, cases):
        paths = []
        for c in cases:
            paths.append(str(self.root / "records" / c["id"] / "game.jsonl"))
            write_lines(Path(paths[-1]), [record(c)])
        listing = self.root / "list.txt"
        listing.write_text("\n".join(paths) + "\n", encoding="utf-8")
        return paths, listing

    def test_each_scheduled_case_binds_one_record_file(self):
        cases = schedule()
        sched = self.root / "formal-r1.jsonl"
        write_lines(sched, cases)
        paths, listing = self.files(cases)
        out = sd1.records("r1", sched, listing)
        self.assertEqual([c["id"] for c in out["cases"]], [c["id"] for c in cases])
        self.assertEqual(out["cases"][0]["record"]["path"], paths[0])
        self.assertFalse(out["custody_checked"])
        expect = {c["id"]: c["record"]["sha256"] for c in out["cases"]}
        self.assertTrue(sd1.records("r1", sched, listing, expect)["custody_checked"])
        with self.assertRaisesRegex(ValueError, "custody SHA-256"):
            sd1.records("r1", sched, listing, {**expect, cases[5]["id"]: "0" * 64})

    def test_a_custody_gap_stops(self):
        cases = schedule()
        sched = self.root / "formal-r1.jsonl"
        write_lines(sched, cases)
        paths, listing = self.files(cases[:-1])
        with self.assertRaisesRegex(ValueError, "not exactly the schedule's cases"):
            sd1.records("r1", sched, listing)
        paths, listing = self.files(cases)
        write_lines(Path(paths[3]), [record(cases[3]), record(cases[3])])
        with self.assertRaisesRegex(ValueError, "exactly one budget_game row"):
            sd1.records("r1", sched, listing)
        write_lines(Path(paths[3]), [record(cases[4])])
        with self.assertRaisesRegex(ValueError, "two record files"):
            sd1.records("r1", sched, listing)


# ------------------------------------------------------------------ games


class Games(Temp):
    """Twenty scheduled games with their records, pinned-binary replays and
    rebuilds; game 0 is the one the cases below change."""

    def setUp(self):
        super().setUp()
        self.cases = schedule()[:20]
        sched = write_lines(self.root / "formal-r1.jsonl", schedule())
        entries = []
        self.replays, self.rebuilds = [], []
        for c in self.cases:
            path = self.root / "records" / c["id"] / "game.jsonl"
            entries.append({"id": c["id"], "record": write_lines(path, [record(c)])})
            starts = [{"step": 0, **START}, {"step": 2, **START, "state_hash": "t"}]
            self.replays.append({"file": str(path), "id": c["id"], "matches_live": True,
                                 "replay": {"id": c["id"], "end": "win", "sequence": [], "actions": 4,
                                            "starts": starts, "witnesses": []}})
            self.rebuilds.append({"kind": "root_rebuild", "schema": sd1.REBUILD_SCHEMA, "id": c["id"],
                                  "file": str(path), "own": c["own"], "void": None,
                                  "starts": [dict(s) for s in starts],
                                  "schedule_sha256": sched["sha256"], "roots": [], "plain": [],
                                  "choosers": {"new_root": 1, "reused": 0, "plain": 0}, "one_action": 0,
                                  "first_choice_step": 1})
        self.records = self.root / "RECORDS.json"
        write(self.records, {"schema": sd1.RECORDS_SCHEMA, "lineage": "r1", "schedule": sched, "cases": entries})

    def run_games(self):
        replay = write_lines(self.root / "replay.jsonl", self.replays)
        rebuild = write_lines(self.root / "rebuild.jsonl", self.rebuilds)
        return sd1.games(self.records, Path(replay["path"]), Path(rebuild["path"]), PARENT, self.root / "out",
                         str(self.root / "bank"), decks=DECKS)

    def status(self):
        return json.loads((self.root / "out" / "GAMES-r1.json").read_text())

    def test_kept_games_project_the_record_with_the_pinned_binarys_starts(self):
        out = self.run_games()
        self.assertEqual((out["kept"], out["void"]), (20, 0))
        lines = [json.loads(x) for x in (self.root / "out" / "games-r1.jsonl").read_text().splitlines()]
        first = lines[0]
        self.assertEqual(first["id"], self.cases[0]["id"])
        self.assertEqual(first["starts"], self.replays[0]["replay"]["starts"])
        self.assertEqual(first["episode"]["seed"], self.cases[0]["seed"])
        self.assertEqual(set(first["decisions"][0]), set(sd1.DECISION_FIELDS))
        seat1 = lines[1]
        self.assertEqual(seat1["episode"]["learner_seat"], 1)
        self.assertEqual([d["label"] for d in seat1["episode"]["selected"]],
                         [NAMES[self.cases[1]["other"]], NAMES[self.cases[1]["own"]]])
        command = json.loads((self.root / "out" / "bank-r1.command.json").read_text())
        self.assertEqual(command["mode"], "archive_distillation_bank")
        self.assertEqual(len(command["sources"]), 22)
        self.assertEqual(self.status()["games_file"]["sha256"], command["games"]["sha256"])

    def test_each_void_reason_is_decided_before_the_bank(self):
        changes = [
            ("root rebuild: step 3: a reused", lambda: self.rebuilds[0].update(void="step 3: a reused")),
            ("no root-rebuild row", lambda: self.rebuilds.pop(0)),
            ("no pinned-binary replay row", lambda: self.replays.pop(0)),
            ("replay failed", lambda: self.replays[0].update(replay={"error": "x"}, matches_live=False)),
            ("end or line-tracker sequence", lambda: self.replays[0].update(matches_live=False)),
            ("start count", lambda: self.replays[0]["replay"].update(starts=self.replays[0]["replay"]["starts"][:1])),
            ("a focal start differs", lambda: self.rebuilds[0]["starts"][1].update(state_hash="u")),
        ]
        for expected, change in changes:
            with self.subTest(expected):
                self.tearDown()
                self.setUp()
                change()
                out = self.run_games()
                self.assertEqual((out["kept"], out["void"]), (19, 1))
                void = self.status()["void"]
                self.assertEqual(void[0]["id"], self.cases[0]["id"])
                self.assertIn(expected, void[0]["reason"])

    def test_more_than_one_void_in_twenty_stops(self):
        self.rebuilds[0]["void"] = "x"
        self.rebuilds[1]["void"] = "y"
        with self.assertRaisesRegex(ValueError, "engineering: 2 of 20"):
            self.run_games()
        self.assertTrue(self.status()["stop"])
        self.assertFalse((self.root / "out" / "games-r1.jsonl").exists())

    def test_the_rebuild_must_have_checked_the_frozen_schedule(self):
        self.rebuilds[3]["schedule_sha256"] = None
        with self.assertRaisesRegex(ValueError, "frozen schedule"):
            self.run_games()


# ------------------------------------------------------- bank and targets


def root(step, live, n, wins, x, view="untied"):
    return {"step": step, "map": live, "n": n, "wins": wins, "frozen_choice": x, "action": live[x],
            "visits": sum(n), "view": view, "search": 0}


# Root shapes: an evidence root (edge 1 separated, edge 2 untried) and one
# without evidence.
EVIDENCE = dict(n=[40, 40, 0], wins=[30, 5, 0], x=0)
FLAT = dict(n=[10, 10], wins=[5, 5], x=0)


class Pipeline(Temp):
    """Per lineage: the split of the synthetic schedule; four kept games, two
    training and two held out, each with an evidence root at its first choice
    (step 0 for one game) and a flat root later; a fifth game void before the
    bank and a sixth dropped by the bank builder."""

    def lineage(self, lineage):
        cases = schedule(lineage)
        the_split = sd1.split(lineage, cases)
        held = [c for c in cases if the_split["games"][c["id"]] == "held_out"]
        train = [c for c in cases if the_split["games"][c["id"]] == "training"]
        chosen = [train[0], train[2], held[0], held[2]]
        void_case, dropped_case = train[4], held[4]
        split_pin = write(self.root / lineage / "SPLIT.json", the_split)
        rebuild_rows, bank_rows, kept = [], [], []
        for i, c in enumerate(chosen + [dropped_case]):
            first = 0 if i == 0 else 3
            roots = [root(first, [2, 0, 1], **EVIDENCE, view="tie_minimum" if i == 1 else "untied"),
                     root(9, [1, 0], **FLAT, view="work_limit" if i == 2 else "untied")]
            rebuild_rows.append({"schema": sd1.REBUILD_SCHEMA, "id": c["id"], "own": c["own"], "void": None,
                                 "roots": roots, "first_choice_step": first,
                                 "plain": [{"step": 20, "refreshed": None, "remaining_before": 0},
                                           {"step": 22, "refreshed": "miss", "remaining_before": 64000}],
                                 "choosers": {"new_root": 2, "reused": 3, "plain": 2}, "one_action": 1})
            if c is dropped_case:
                continue
            kept.append(c["id"])
            for r in roots:
                bank_rows.append({"game": c["id"], "step": r["step"], "k": len(r["map"]), "chooser": "new_root",
                                  "target": r["action"], "parent_logits": f32_bits([0.5, 2.0, 1.0][:len(r["map"])])})
            bank_rows.append({"game": c["id"], "step": 12, "k": 2, "chooser": "reused", "target": 0,
                              "parent_logits": f32_bits([0.0, 0.0])})
        rebuild_rows.append({"schema": sd1.REBUILD_SCHEMA, "id": void_case["id"], "own": void_case["own"],
                             "void": "root rebuild: x"})
        rebuild = write_lines(self.root / lineage / "rebuild.jsonl", rebuild_rows)
        games_file = write_lines(self.root / lineage / "games.jsonl", [{"id": g} for g in kept])
        status = {"schema": sd1.GAMES_SCHEMA, "lineage": lineage, "rebuild": rebuild, "games": 6,
                  "kept": kept + [dropped_case["id"]], "void": [{"id": void_case["id"], "reason": "root rebuild: x"}],
                  "stop": False, "games_file": games_file}
        status_pin = write(self.root / lineage / "GAMES.json", status)
        bank = {"schema": sd1.BANK_SCHEMA, "lineage": lineage, "games": games_file,
                "game_status": [{"id": g, "kept": True, "reason": None, "rows": 3} for g in kept]
                + [{"id": dropped_case["id"], "kept": False, "reason": "start hash differs", "rows": 0}],
                "rows": bank_rows, "rows_sha256": f"rows-{lineage}"}
        bank_pin = write(self.root / lineage / "bank.json", bank)
        return {"split": split_pin, "games": status_pin, "bank": bank_pin, "chosen": chosen,
                "dropped": dropped_case, "void": void_case}

    def checks(self, games_count=None):
        out = {}
        for lineage in common.LINEAGES:
            fx = self.lineage(lineage)
            if games_count is not None:
                status = json.loads(Path(fx["games"]["path"]).read_text())
                status["games"] = games_count
                fx["games"] = write(Path(fx["games"]["path"]), status)
            check = sd1.bank_check(Path(fx["games"]["path"]), Path(fx["bank"]["path"]))
            fx["check"] = check
            fx["check_pin"] = write(self.root / lineage / "BANKCHECK.json", check)
            out[lineage] = fx
        return out

    def test_bank_drops_count_as_void_and_the_rule_is_read_again(self):
        fx = self.checks(games_count=40)["r1"]
        check = fx["check"]
        self.assertEqual(check["dropped_by_bank"], 1)
        self.assertEqual([v["id"] for v in check["void"]], [fx["void"]["id"], fx["dropped"]["id"]])
        self.assertIn("the bank builder dropped it: start hash differs", check["void"][1]["reason"])
        self.assertEqual((check["stop"], check["failures"], check["pass"]), (False, [], True))
        self.assertEqual(check["by_chooser"], {"new_root": 8, "reused": 4})
        fx = self.checks(games_count=39)["r1"]
        self.assertTrue(fx["check"]["stop"])
        self.assertFalse(fx["check"]["pass"])

    def test_the_bank_check_flags_a_root_the_map_does_not_execute(self):
        fx = self.lineage("r1")
        bank = json.loads(Path(fx["bank"]["path"]).read_text())
        bank["rows"][0]["target"] = 0
        write(Path(fx["bank"]["path"]), bank)
        status = json.loads(Path(fx["games"]["path"]).read_text())
        status["games"] = 40
        write(Path(fx["games"]["path"]), status)
        check = sd1.bank_check(Path(fx["games"]["path"]), Path(fx["bank"]["path"]))
        self.assertFalse(check["pass"])
        self.assertIn("not the map's frozen choice", check["failures"][0])

    def test_targets_write_two_keep_set_files_and_the_census(self):
        fixtures = self.checks(games_count=40)
        spec = {lin: {"split": fx["split"], "games": fx["games"], "bank_check": fx["check_pin"]}
                for lin, fx in fixtures.items()}
        result = sd1.targets(spec, self.root / "stage2", names=NAMES)
        self.assertTrue(result["stop"], "a census far below 100 training rows stops")
        self.assertEqual(result["evidence_rows"], {"r1": {"training": 2, "held_out": 2},
                                                   "r2": {"training": 2, "held_out": 2}})
        census = json.loads((self.root / "stage2" / "CENSUS.json").read_text())
        r1 = census["lineages"]["r1"]
        fx = fixtures["r1"]
        for side, games in (("training", fx["chosen"][:2]), ("held_out", fx["chosen"][2:])):
            keep = json.loads(Path(r1["keep_sets"][side]["path"]).read_text())
            self.assertEqual(keep["schema"], sd1.KEEP_SETS_SCHEMA)
            self.assertEqual(keep["bank_rows_sha256"], "rows-r1")
            self.assertEqual([r["game"] for r in keep["rows"]], [g["id"] for g in games])
            # Live map [2, 0, 1]: edge 1 (live 0) separated; edges 0 and 2 kept.
            self.assertEqual({tuple(r["keep"]) for r in keep["rows"]}, {(1, 2)})
        training = r1["training"]["all"]
        self.assertEqual((training["rows"], training["evidence_rows"], training["separated_edges"]), (4, 2, 2))
        self.assertEqual(training["tried_rivals_separated_share"], 1.0)
        self.assertEqual((training["evidence_rows_at_step_0"], training["evidence_rows_after_step_0"]), (1, 1))
        self.assertEqual((training["evidence_rows_at_first_choice"], training["evidence_rows_after_first_choice"]),
                         (2, 0))
        self.assertEqual(training["views"], {"tie_minimum": 1, "untied": 3, "work_limit": 0})
        self.assertEqual(r1["held_out"]["all"]["views"]["work_limit"], 1)
        # The parent's softmax over live actions 0..2 of logits [0.5, 2.0, 1.0]:
        # the separated live action 0 has the smallest mass.
        p = [math.exp(v) for v in (0.5, 2.0, 1.0)]
        self.assertAlmostEqual(training["parent_mean_separated_mass"], p[0] / sum(p), places=6)
        self.assertAlmostEqual(training["summed_kl_q_p"], -2 * math.log((p[1] + p[2]) / sum(p)), places=6)
        self.assertEqual(training["parent_top_differs_from_leader"], 2)
        left = training["left_out"]
        self.assertEqual((left["reused"], left["plain"], left["one_action"]), (6, 4, 2))
        self.assertEqual(left["plain_by_refreshed_and_remaining"], {"not_refreshed/exhausted": 2,
                                                                    "refreshed/remaining": 2})
        self.assertEqual(r1["games"], {"training": {"scheduled": 126, "kept": 2, "void": 1},
                                       "held_out": {"scheduled": 36, "kept": 2, "void": 1}})
        deck = NAMES[fx["chosen"][0]["own"]]
        self.assertGreaterEqual(r1["training"]["by_deck"][deck]["rows"], 2)
        rows = [json.loads(x) for x in Path(r1["rows_file"]["path"]).read_text().splitlines()]
        self.assertEqual(len(rows), 8)
        self.assertTrue(all(r["mean"]["leader"] == r["x"] for r in rows))

    def test_the_mean_rule_adapter_reads_the_record_counts(self):
        rule = sd1.mean_rule(**EVIDENCE)
        self.assertEqual((rule["leader"], rule["separated"], rule["evidence"]), (0, [1], True))
        self.assertFalse(sd1.mean_rule(**FLAT)["evidence"])
        # An untried edge has upper bound 1 and is never separated.
        self.assertEqual(sd1.mean_rule([40, 0], [40, 0], 0)["separated"], [])


# ------------------------------------------------------------- selection


LABEL = "SD1-T-s3"
KEEP_PIN = {"path": "KEEP-training-r1.json", "sha256": "a" * 64}
HELD_PIN = {"path": "KEEP-held_out-r1.json", "sha256": "b" * 64}
# Held-out evidence rows: two in matchup A, one each in B and C.
MATCHUPS = [[NAMES[0], NAMES[6]], [NAMES[0], NAMES[6]], [NAMES[1], NAMES[2]], [NAMES[2], NAMES[6]]]


def readout(cid, training=0.1, held=(7, 7, 7, 7), gap=4.0, agree=0.95, kl=0.01, state=None):
    rows = [{"game": f"g{i}", "step": 3 + i, "matchup": m, "separated_mass": h / 64, "kl_target": 0.1}
            for i, (m, h) in enumerate(zip(MATCHUPS, held))]
    anchor = {"rows": 100, "median_clamped_top_two_gap": gap, "top_agreement": agree, "mean_kl_from_parent": kl}
    return {"id": cid, "state_sha256": state or f"state-{cid}",
            "evidence": {"training": {"rows": 10, "mean_separated_mass": training, "mean_kl_target": 0.2,
                                      "by_deck": {}},
                         "held_out": {"rows": 4, "mean_separated_mass": sum(held) / 4 / 64, "mean_kl_target": 0.3,
                                      "by_deck": {}, "row_values": rows}},
            "anchor_held_out_decks": {"all": anchor, "non_spy": dict(anchor), "by_deck": {}}}


class Select(Temp):
    def setUp(self):
        super().setUp()
        telemetry = [{"anchor_loss": 0.1, "archive_loss": 0.2, "anchor_components": {"a": {"auxiliary_l2": 0.0}},
                      "archive_components": {"a": {"auxiliary_l2": 0.0}}} for _ in range(8192)]
        self.telemetry = telemetry
        self.telemetry_pin = write(self.root / "telemetry.json", telemetry)
        identity = {"lineage": "r1", "recipe": {"options": {"cell": LABEL, "keep_sets": KEEP_PIN}}}
        self.full = {"schema": sd1.PHASE1_SCHEMA, "complete": True, "stop_after_steps": None, "identity": identity,
                     "checkpoints": [{"step": s, "state_sha256": f"state-{LABEL}-step-{s}"}
                                     for s in sd1.CHECKPOINT_STEPS],
                     "telemetry": self.telemetry_pin, "state_hashes": [{"step": 32, "phase1_state_sha256": "h32"}]}
        self.rerun = {"schema": sd1.PHASE1_SCHEMA, "complete": True, "stop_after_steps": 32, "identity": identity,
                      "state_hashes": [{"step": 32, "phase1_state_sha256": "h32"}]}
        refs = [readout("parent", training=0.4, held=(20, 20, 20, 20), gap=4.0, agree=1.0, kl=0.0),
                readout("O-final", training=0.38, held=(19, 19, 19, 19), gap=3.0, agree=0.9, kl=0.05)]
        self.reference = self.probe_of(refs)
        # Steps: 32 fails F; 64 is within one SE of the minimum at 128; 256
        # ties 128; later checkpoints are higher.
        self.points = {8: dict(training=0.35, held=(30, 30, 30, 30)), 16: dict(training=0.3, held=(26, 26, 26, 26)),
                       32: dict(training=0.3, held=(24, 16, 20, 20)), 64: dict(training=0.15, held=(8, 6, 9, 7)),
                       128: dict(training=0.1, held=(8, 6, 7, 7)), 256: dict(training=0.1, held=(7, 7, 7, 7))}
        for s in sd1.GATED_STEPS[4:]:
            self.points[s] = dict(training=0.1, held=(9, 9, 9, 9))

    def probe_of(self, candidates):
        return {"schema": sd1.PROBE_SCHEMA, "complete": True, "lineage": "r1", "parent": {"p": 1},
                "parent_state_sha256": "ps", "banks": [{"role": "training"}], "anchor": {"a": 1}, "offers": [],
                "evidence": {"training": KEEP_PIN, "held_out": HELD_PIN}, "runtime_commit": "c",
                "runtime_tree": "t", "candidates": candidates}

    def probe(self, changes=None):
        points = {**self.points, **(changes or {})}
        candidates = copy.deepcopy(self.reference["candidates"])
        candidates += [readout(f"{LABEL}-step-{s}", **points[s]) for s in sd1.CHECKPOINT_STEPS]
        return self.probe_of(candidates)

    def select(self, probe=None, rerun=None, split=None):
        return sd1.select("r1", 3, probe or self.probe(), self.reference, self.full, rerun or self.rerun,
                          split, names=NAMES)

    def test_d1_is_the_earliest_checkpoint_within_one_clustered_se_of_the_minimum(self):
        out = self.select()
        self.assertEqual(out["reading"], "passes")
        self.assertTrue(out["identity"]["pass"] and out["g3"]["pass"])
        d1 = out["d1"]
        self.assertEqual(d1["minimizing_step"], 128, "256 ties 128: the earliest minimizes")
        self.assertEqual(d1["minimum"], 7 / 64)
        # Step 64 against 128: d = (0, 0, 2, 0)/64, dbar = 0.5/64; cluster
        # sums -1, 1.5 and -0.5 (/64); G = 3.
        se = math.sqrt(3 / 2 * (1 + 2.25 + 0.25)) / 64 / 4
        first = d1["candidates"][0]
        self.assertEqual((first["step"], first["matchups"]), (64, 3))
        self.assertAlmostEqual(first["se"], se, places=15)
        self.assertTrue(7.5 / 64 <= 7 / 64 + se)
        self.assertEqual((out["selected_step"], out["fit_dose"], out["highest_passing_step"]), (64, 64, 8192))
        self.assertEqual([c["step"] for c in out["checkpoints"]], list(sd1.CHECKPOINT_STEPS))
        self.assertEqual(out["references"]["parent"]["training_separated_mass"], 0.4)

    def test_with_one_rows_matchup_the_clustered_se_is_the_sample_sd_over_root_n(self):
        rows_c = [{"game": f"g{i}", "step": i, "matchup": [f"m{i}", "x"], "separated_mass": v}
                  for i, v in enumerate((0.3, 0.1, 0.2))]
        rows_m = [{**r, "separated_mass": 0.0} for r in rows_c]
        se, g = sd1.clustered_se(rows_c, rows_m)
        d = [0.3, 0.1, 0.2]
        sd = math.sqrt(sum((x - 0.2) ** 2 for x in d) / 2)
        self.assertEqual(g, 3)
        self.assertAlmostEqual(se, sd / math.sqrt(3), places=15)

    def test_with_fewer_than_two_matchups_d1_is_the_minimizing_checkpoint(self):
        global MATCHUPS
        saved = MATCHUPS
        MATCHUPS = [[NAMES[0], NAMES[6]]] * 4
        try:
            self.tearDown()
            self.setUp()
            out = self.select()
        finally:
            MATCHUPS = saved
        self.assertEqual(out["d1"]["candidates"][0]["se"], None)
        self.assertEqual(out["selected_step"], 128)

    def test_the_reading_stops_at_the_first_gated_g2_failure(self):
        out = self.select(self.probe({256: dict(training=0.05, held=(1, 1, 1, 1), gap=1.0, kl=0.06)}))
        self.assertEqual(out["g2_stop"]["step"], 256)
        self.assertEqual(out["g2_stop"]["failed"], ["gap", "kl"])
        self.assertEqual([c["step"] for c in out["checkpoints"]], [8, 16, 32, 64, 128])
        self.assertEqual((out["selected_step"], out["highest_passing_step"]), (64, 128))
        self.assertNotIn("512", json.dumps(out["d1"]))

    def test_a_seed_failing_g2_before_any_f_pass_has_no_d1(self):
        out = self.select(self.probe({32: dict(training=0.3, held=(24, 16, 20, 20), agree=0.5)}))
        self.assertEqual(out["reading"], "collapse before the required fit")
        self.assertEqual(out["g2_stop"]["failed"], ["agreement"])
        self.assertNotIn("d1", out)
        out = self.select(self.probe({32: dict(training=0.2, held=(24, 16, 20, 20), agree=0.5)}))
        self.assertEqual(out["reading"], "required fit reached only together with collapse")

    def test_descriptive_checkpoints_do_not_stop_the_reading(self):
        out = self.select(self.probe({8: dict(training=0.35, held=(30, 30, 30, 30), gap=0.1)}))
        self.assertEqual(out["reading"], "passes")
        self.assertEqual(out["checkpoints"][0]["G2"]["failed"], ["gap"])

    def test_f_never_passing_with_g2_holding(self):
        changes = {s: dict(training=0.3, held=(9, 9, 9, 9)) for s in sd1.GATED_STEPS}
        self.assertEqual(self.select(self.probe(changes))["reading"], "required fit not reached")

    def test_g3_and_probe_identity_failures_are_engineering_stops(self):
        out = self.select(rerun={**self.rerun, "state_hashes": [{"step": 32, "phase1_state_sha256": "other"}]})
        self.assertTrue(out["reading"].startswith("engineering: G3"))
        self.assertEqual(out["g3"]["cause"], "the first 32 steps did not reproduce")
        out = self.select(self.probe({64: dict(training=float("nan"), held=(8, 6, 9, 7))}))
        self.assertEqual(out["g3"]["cause"], "a non-finite value at step 64")
        self.telemetry[100]["archive_loss"] = float("inf")
        write(Path(self.telemetry_pin["path"]), self.telemetry)
        out = self.select()
        self.assertEqual(out["g3"]["cause"], "a non-finite value at step 128")
        probe = self.probe()
        probe["candidates"][1]["anchor_held_out_decks"]["all"]["mean_kl_from_parent"] = 0.0500000001
        out = self.select(probe)
        self.assertEqual(out["identity"]["differing"], ["O-final"])
        self.assertTrue(out["reading"].startswith("engineering"))
        probe = self.probe()
        probe["candidates"][0]["anchor_held_out_decks"]["all"]["mean_kl_from_parent"] = -0.0
        self.assertEqual(self.select(probe)["identity"]["differing"], ["parent"])
        probe = self.probe()
        probe["runtime_commit"] = "d"
        self.assertEqual(self.select(probe)["identity"]["differing"], ["runtime_commit"])

    def test_the_split_binds_each_held_out_rows_matchup(self):
        the_split = {"schema": sd1.SPLIT_SCHEMA, "lineage": "r1",
                     "games": {"g0": "held_out", "g1": "held_out", "g2": "held_out", "g3": "held_out"},
                     "matchups": {"g0": [0, 6], "g1": [0, 6], "g2": [1, 2], "g3": [2, 6]}}
        self.assertEqual(self.select(split=the_split)["reading"], "passes")
        the_split["matchups"]["g2"] = [1, 3]
        with self.assertRaisesRegex(ValueError, "not the split's"):
            self.select(split=the_split)

    def test_the_run_must_be_this_seeds_keep_set_cell(self):
        with self.assertRaisesRegex(ValueError, "SD1-T-s4"):
            sd1.select("r1", 4, self.probe(), self.reference, self.full, self.rerun, names=NAMES)
        self.assertEqual(sd1.cell_label(0), "SD1-T-s0")
        with self.assertRaises(ValueError):
            sd1.cell_label(8)
        self.assertTrue(sd1.CELL_LABEL.match("SD1-T-s7"))
        for label in ("SD1-T-s8", "SD1-T-s07", "U-s32", "BT1-T", "SD1-T-s1x"):
            self.assertIsNone(sd1.CELL_LABEL.match(label))


# ----------------------------------------------------------- tool checks


class ToolChecks(Temp):
    def test_known_answer_compares_every_kept_games_root_with_bt1s_live_map(self):
        rebuild = [{"id": "a", "void": None, "roots": [root(1, [1, 0], [10, 2], [6, 0], 0),
                                                      root(5, [0, 1], [10, 2], [6, 0], 0, view="tie_minimum")]},
                   {"id": "b", "void": "root rebuild: x", "roots": []}]
        bt1_rows = [{"game": "a", "step": 1, "chooser": "new_root", "live": [1, 0], "n": [10, 2], "wins": [6, 0],
                     "x": 0, "visits": 12},
                    {"game": "a", "step": 5, "chooser": "new_root", "live": [1, 0], "n": [10, 2], "wins": [6, 0],
                     "x": 0, "visits": 12},
                    {"game": "a", "step": 7, "chooser": "reused"},
                    {"game": "b", "step": 2, "chooser": "new_root"}]
        paths = {"r1": Path(write_lines(self.root / "rebuild.jsonl", rebuild)["path"])}
        rows = {"r1": Path(write_lines(self.root / "ROWS-r1.jsonl", bt1_rows)["path"])}
        with mock.patch.object(sd1, "MIN_KNOWN_ROOTS", 1):
            out = sd1.known_answer(paths, rows)
        r1 = out["lineages"]["r1"]
        self.assertEqual((r1["bt1_new_root_rows"], r1["roots_compared"], r1["roots_not_replayed"]), (3, 2, 1))
        self.assertEqual(r1["roots_differing"], [["a", 5]])
        self.assertEqual(r1["void_games"], [{"id": "b", "reason": "root rebuild: x"}])
        self.assertFalse(out["pass"])

    def test_bt1_regression_counts_reproduced_rows(self):
        rows = [{"chooser": "new_root", **EVIDENCE, "rules": {"mean": sd1.mean_rule(**EVIDENCE)}},
                {"chooser": "new_root", **FLAT, "rules": {"mean": sd1.mean_rule(**FLAT)}},
                {"chooser": "reused"}]
        path = Path(write_lines(self.root / "ROWS-r1.jsonl", rows)["path"])
        out = sd1.bt1_regression({"r1": path})
        r1 = out["lineages"]["r1"]
        self.assertEqual((r1["new_root_rows"], r1["reproduced"], r1["evidence_rows"]), (2, 2, 1))
        self.assertTrue(out["pass"])

    def completion(self, name, values, keep=None):
        iterations = []
        for i, v in enumerate(values):
            if keep is not None and i >= keep:
                iterations.append({"path": str(self.root / name / f"missing{i}.json"), "sha256": "0" * 64})
                continue
            update = write(self.root / name / f"update{i}.json",
                           {f: v if f == "after_state_sha256" else i for f in sd1.X1_FIELDS})
            iterations.append(write(self.root / name / f"receipt{i}.json", {"iteration": i, "update": update}))
        return {"iterations": iterations}

    def test_o_check_locates_the_first_differing_update_as_far_as_records_allow(self):
        mine = self.completion("mine", [f"s{i}" for i in range(162)])
        same = self.completion("same", [f"s{i}" for i in range(162)], keep=40)
        moved = self.completion("moved", [f"s{i}" if i < 7 else "x" for i in range(162)])
        end = {"reproduced": True}
        with mock.patch.object(sd1.phase2, "check", return_value=end):
            out = sd1.o_check("r1", mine, same, Path("o.json"))
            self.assertEqual((out["recorded_updates_retained"], out["compared_updates"]), (40, 40))
            self.assertIsNone(out["first_differing_update"])
            self.assertTrue(out["pass"])
            out = sd1.o_check("r1", mine, moved, Path("o.json"))
            self.assertEqual(out["first_differing_update"], {"update": 8, "fields": ["after_state_sha256"]})
            self.assertFalse(out["pass"])


if __name__ == "__main__":
    unittest.main()
