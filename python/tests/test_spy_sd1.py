"""Search-evidence distillation (tools/spy_distill/sd1.py): fixture tests. Nothing is searched,
played or trained; every schedule, record, replay, bank, probe and run here is synthetic."""
import copy
import hashlib
import io
import json
import math
import struct
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools.spy_distill import sd1  # noqa: E402
from tools.spy_distill import analyze, phase2  # noqa: E402
from tools.spy_distill import distill_common as common  # noqa: E402
from tools.spy_reset import panel  # noqa: E402

pilot = common.pilot

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
            with mock.patch("sys.stdout"):
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


class SelectFixture(Temp):
    def setUp(self):
        super().setUp()
        telemetry = [{"anchor_loss": 0.1, "archive_loss": 0.2, "anchor_components": {"a": {"auxiliary_l2": 0.0}},
                      "archive_components": {"a": {"auxiliary_l2": 0.0}}} for _ in range(8192)]
        self.telemetry = telemetry
        self.telemetry_pin = write(self.root / "telemetry.json", telemetry)
        identity = {"lineage": "r1", "recipe": {"options": {"cell": LABEL, "keep_sets": KEEP_PIN}}}
        self.full = {"schema": sd1.PHASE1_SCHEMA, "complete": True, "stop_after_steps": None, "identity": identity,
                     "checkpoints": [{"step": s, "state_sha256": f"state-{LABEL}-step-{s}",
                                      "checkpoint": {"path": f"ck-{s}.json", "sha256": f"{s:064d}"},
                                      "source": {"path": f"src-{s}.json", "sha256": f"{s + 1:064d}"}}
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


class Select(SelectFixture):
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
        self.assertEqual(out["d1_checkpoint"], {k: self.full["checkpoints"][3][k]
                                                for k in ("step", "checkpoint", "source", "state_sha256")})
        self.assertEqual(out["d1_checkpoint"]["step"], 64)
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

    def test_pinned_inputs_compare_by_hash_not_path(self):
        probe = self.probe()
        probe["evidence"] = {side: {**pin, "path": f"elsewhere/{side}.json"} for side, pin in probe["evidence"].items()}
        probe["candidates"][0]["source"] = {"checkpoint": {"path": "elsewhere/parent.json", "sha256": "p"}}
        self.reference["candidates"][0]["source"] = {"checkpoint": {"path": "here/parent.json", "sha256": "p"}}
        self.assertTrue(self.select(probe)["identity"]["pass"])
        probe["candidates"][0]["source"]["checkpoint"]["sha256"] = "q"
        self.assertEqual(self.select(probe)["identity"]["differing"], ["parent"])
        probe = self.probe()
        probe["evidence"]["held_out"] = {**probe["evidence"]["held_out"], "sha256": "0" * 64}
        self.assertEqual(self.select(probe)["identity"]["differing"], ["evidence"])

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


# -------------------------------------------------- stage 3 and 4 commands


def model_source(checkpoint) -> dict:
    return {"play_import": {"path": "parent-import.json", "sha256": "1" * 64},
            "feature_transfer": {"expected_feature_contract_digest": "c", "expected_feature_encoding_digest": "e"},
            "checkpoint": checkpoint}


def r1_parent() -> dict:
    return model_source({"path": "parent.json", "sha256": common.PARENT_CHECKPOINT_SHA256["r1"]})


class Commands(Temp):
    def setUp(self):
        super().setUp()
        self.games = write_lines(self.root / "games-r1.jsonl", [{"id": "g0"}, {"id": "g1"}])
        self.bank = write(self.root / "bank-r1.json", {"rows": []})
        self.keep = {side: write(self.root / f"KEEP-{side}-r1.json", {"side": side}) for side in sd1.SPLITS}
        status = write(self.root / "GAMES-r1.json", {"games_file": self.games})
        check = write(self.root / "CHECK-r1.json", {"bank": self.bank})
        self.census = {"schema": sd1.CENSUS_SCHEMA, "stop_rule": {"stop": False},
                       "spec": {"r1": {"games": status, "bank_check": check}},
                       "lineages": {"r1": {"keep_sets": self.keep}}}
        self.exactness = [{"lineage": lin, "arm": "uniform", "pass": True} for lin in common.LINEAGES]
        self.anchor = {"training": [], "held_out": []}
        self.o_final = model_source({"path": "o-final.json", "sha256": "9" * 64})

    def test_each_seed_runs_its_own_cell_on_the_training_keep_sets(self):
        out = sd1.commands("r1", r1_parent(), self.anchor, self.census, self.exactness, "runs")
        self.assertEqual(len(out), 24)
        for seed in sd1.SEEDS:
            label = f"SD1-T-s{seed}"
            full, dry, rerun = out[label], out[f"{label}-dry"], out[f"{label}-rerun"]
            for command in (full, dry, rerun):
                self.assertEqual(command["options"], {"cell": label, "keep_sets": self.keep["training"],
                                                      "checkpoint_steps": list(sd1.CHECKPOINT_STEPS)})
                self.assertEqual(command["arm"], "uniform")
                self.assertEqual((command["games"]["sha256"], command["bank"]["sha256"]),
                                 (self.games["sha256"], self.bank["sha256"]))
            self.assertTrue(dry["dry_run"])
            self.assertEqual(rerun["stop_after_steps"], 32)
            self.assertNotIn("dry_run", full)
            self.assertNotIn("stop_after_steps", full)
            self.assertTrue(full["output_directory"].endswith(f"runs/r1-{label}"))
        stopped = copy.deepcopy(self.census)
        stopped["stop_rule"]["stop"] = True
        with self.assertRaisesRegex(ValueError, "stop rule"):
            sd1.commands("r1", r1_parent(), self.anchor, stopped, self.exactness, "runs")
        with self.assertRaisesRegex(ValueError, "exactness"):
            sd1.commands("r1", r1_parent(), self.anchor, self.census, self.exactness[:1], "runs")
        Path(self.keep["held_out"]["path"]).write_text("changed", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "held_out keep sets"):
            sd1.commands("r1", r1_parent(), self.anchor, self.census, self.exactness, "runs")

    def full_run(self, seed: int, keep=None, bank=None) -> dict:
        checkpoints = []
        for step in sd1.CHECKPOINT_STEPS:
            checkpoint = {"path": f"ck-{step}.json", "sha256": f"{step:064d}"}
            checkpoints.append({"step": step, "checkpoint": checkpoint, "state_sha256": f"s{step}",
                                "source": write(self.root / f"s{seed}" / f"src-{step}.json",
                                                model_source(checkpoint))})
        return {"schema": sd1.PHASE1_SCHEMA, "complete": True, "stop_after_steps": None,
                "identity": {"lineage": "r1", "arm": "uniform", "bank": bank or self.bank,
                             "recipe": {"options": sd1.seed_options(keep or self.keep, seed)}},
                "checkpoints": checkpoints}

    def test_the_reference_and_seed_probes_read_both_keep_set_files(self):
        ref = sd1.probe_command("r1", r1_parent(), self.o_final, self.anchor, [], self.census, None, "probes/ref")
        self.assertEqual([c["id"] for c in ref["candidates"]], ["parent", "O-final"])
        self.assertEqual({side: pin["sha256"] for side, pin in ref["evidence"].items()},
                         {side: self.keep[side]["sha256"] for side in sd1.SPLITS})
        self.assertEqual([b["role"] for b in ref["banks"]], ["training"])
        self.assertEqual(ref["banks"][0]["bank"]["sha256"], self.bank["sha256"])
        seed = sd1.probe_command("r1", r1_parent(), self.o_final, self.anchor, [], self.census, self.full_run(5),
                                 "probes/s5")
        self.assertEqual([c["id"] for c in seed["candidates"]],
                         ["parent", "O-final"] + [f"SD1-T-s5-step-{s}" for s in sd1.CHECKPOINT_STEPS])
        self.assertEqual(seed["evidence"], ref["evidence"])
        other = {**self.keep, "training": {**self.keep["training"], "sha256": "0" * 64}}
        for run in (self.full_run(5, keep=other), self.full_run(5, bank={**self.bank, "sha256": "0" * 64})):
            with self.assertRaisesRegex(ValueError, "not an SD1 seed run"):
                sd1.probe_command("r1", r1_parent(), self.o_final, self.anchor, [], self.census, run, "probes/x")


class PhaseTwoFixture(Temp):
    def setUp(self):
        super().setUp()
        its, _ = pilot.iterations("r1", "O", pilot.load_decks(), None)
        self.o_config = {"schema": pilot.ndb.SCHEMA, "initial_source": r1_parent(),
                         "opponents": [{"id": pilot.FIXED_OPPONENT_ID,
                                        "source": model_source({"path": "a48.json", "sha256": "4" * 64})}],
                         "iterations": its, **json.loads(json.dumps(pilot.ndb.FROZEN)),
                         "update_backend": {"kind": "cpu"}, "collection_workers": 8, "preparation_workers": 8,
                         "output_directory": "runs/r1-O"}
        self.entry = pilot.schedule("r1")["updates"][0][0]

    def d1(self, seed: int, cell=None) -> dict:
        provenance = {"method": "m", "cell": cell if cell is not None else
                      {"cell": f"SD1-T-s{seed}", "keep_sets": {"path": "k.json", "sha256": "k" * 64}}}
        checkpoint = write(self.root / f"d1-{seed}.json", {"archive_distillation": provenance})
        return {"checkpoint": checkpoint, "source": write(self.root / f"d1-{seed}-source.json",
                                                          model_source(checkpoint))}

    def selection(self, seed: int, reading="passes") -> dict:
        d1 = self.d1(seed)
        return {"schema": sd1.SELECT_SCHEMA, "lineage": "r1", "seed": seed, "reading": reading,
                "d1_checkpoint": {"step": 64, "state_sha256": "s", **d1}}

    def first_episode(self, config: dict) -> dict:
        return config["iterations"][0]["episodes"][0]["episode"]


class PhaseTwoConfigs(PhaseTwoFixture):
    def test_d2_k_starts_from_d1_k_and_o_k_from_the_parent_on_seed_k(self):
        out = sd1.phase2_configs("r1", self.o_config, [self.selection(k) for k in sd1.SEEDS], 4, "runs")
        self.assertEqual(sorted(out), sorted([f"D2-{k}" for k in sd1.SEEDS] + [f"O-{k}" for k in range(1, 8)]))
        e = self.entry
        for k in sd1.SEEDS:
            d2 = out[f"D2-{k}"]
            self.assertEqual(d2["initial_source"]["checkpoint"]["path"], str(self.root / f"d1-{k}.json"))
            first = self.first_episode(d2)
            if k == 0:
                self.assertEqual((first["id"], first["seed"]), (phase2.unit_id("r1", e), e["seed"]))
                continue
            self.assertEqual(first["seed"], pilot.seed([f"sd1/train/rep{k}", "r1", e["own"], e["other"], e["j"]]))
            self.assertEqual(first["id"], f"sd1-r1-rep{k}-o{e['own']}-t{e['other']}-j{e['j']}-s{e['seat']}")
            o = out[f"O-{k}"]
            self.assertEqual(o["initial_source"], r1_parent())
            self.assertEqual(o["iterations"], d2["iterations"])
            self.assertTrue(o["output_directory"].endswith(f"runs/r1-O-{k}/native"))
        rt1 = phase2.replicate_seed("r1", 1, e["own"], e["other"], e["j"])
        self.assertNotEqual(self.first_episode(out["D2-1"])["seed"], rt1)

    def test_every_seed_needs_its_d1(self):
        selections = [self.selection(k) for k in sd1.SEEDS]
        selections[6] = self.selection(6, reading="collapse before the required fit")
        with self.assertRaisesRegex(ValueError, r"seeds \[6\] have no D1"):
            sd1.phase2_configs("r1", self.o_config, selections, 4, "runs")
        with self.assertRaisesRegex(ValueError, "every seed"):
            sd1.phase2_configs("r1", self.o_config, selections[:7], 4, "runs")
        swapped = [self.selection(k) for k in sd1.SEEDS]
        swapped[2]["d1_checkpoint"] = swapped[3]["d1_checkpoint"]
        with self.assertRaisesRegex(ValueError, "seed 3"):
            sd1.phase2_configs("r1", self.o_config, swapped, 4, "runs")

    def test_phase2_admits_sd1_cells_only_under_its_namespace(self):
        sd1_start = model_source(self.d1(3)["checkpoint"])
        with self.assertRaisesRegex(ValueError, "does not start phase 2"):
            phase2.config("r1", self.o_config, sd1_start, 4, "runs/x", replicate=3)
        config = phase2.config("r1", self.o_config, sd1_start, 4, "runs/x", replicate=3, namespace=phase2.SD1_NS)
        self.assertEqual(config["initial_source"], sd1_start)
        with self.assertRaisesRegex(ValueError, "seed 3"):
            phase2.config("r1", self.o_config, sd1_start, 4, "runs/x", replicate=2, namespace=phase2.SD1_NS)
        for cell in ({}, {"cell": "BT1-T", "keep_sets": {"path": "k", "sha256": "k"}}, {"cell": "SD1-T-s3"}):
            start = model_source(self.d1(9, cell=cell)["checkpoint"])
            with self.assertRaisesRegex(ValueError, "SD1 cell's checkpoint"):
                phase2.config("r1", self.o_config, start, 4, "runs/x", replicate=3, namespace=phase2.SD1_NS)
        with self.assertRaisesRegex(ValueError, "keeps every unit"):
            phase2.config("r1", self.o_config, sd1_start, 4, "runs/x", replicate=3, namespace=phase2.SD1_NS,
                          drop_spy_learner=True)
        # O-k from the parent: seeds 1 to 7 under SD1's namespace only.
        for k in (1, 7):
            config = phase2.config("r1", self.o_config, None, 4, "runs/o", replicate=k, namespace=phase2.SD1_NS)
            self.assertEqual(config["initial_source"], r1_parent())
        with self.assertRaisesRegex(ValueError, "unknown replicate"):
            phase2.config("r1", self.o_config, None, 4, "runs/o", replicate=8, namespace=phase2.SD1_NS)
        with self.assertRaisesRegex(ValueError, "not to O's reproduction"):
            phase2.config("r1", self.o_config, None, 4, "runs/o", replicate=1)
        with self.assertRaisesRegex(ValueError, "unknown replicate namespace"):
            phase2.config("r1", self.o_config, None, 4, "runs/o", namespace="other")

    def test_the_cli_keeps_its_retention_choices_and_names_sd1s_starts(self):
        o_path = self.root / "O.json"
        o_path.write_text(json.dumps(self.o_config), encoding="utf-8")
        base = ["config", "--lineage", "r1", "--o-config", str(o_path), "--workers", "4",
                "--output-directory", "runs/o", "--out", str(self.root / "cfg.json")]
        with mock.patch("sys.stderr"), self.assertRaises(SystemExit):
            phase2.main(base + ["--from-parent", "--replicate", "5"])
        with self.assertRaisesRegex(ValueError, "--from-parent"):
            phase2.main(base + ["--from-parent", "--replicate", "1"])
        with self.assertRaisesRegex(ValueError, "keeps O's own seeds"):
            phase2.main(base + ["--reproduce", "--replicate", "1", "--namespace", "sd1"])
        with mock.patch("sys.stdout"):
            self.assertEqual(phase2.main(base + ["--from-parent", "--replicate", "5", "--namespace", "sd1"]), 0)
        written = json.loads((self.root / "cfg.json").read_text(encoding="utf-8"))
        e = self.entry
        self.assertEqual(self.first_episode(written)["seed"],
                         pilot.seed(["sd1/train/rep5", "r1", e["own"], e["other"], e["j"]]))


class PanelNamespace(Temp):
    def test_sd1_panels_take_fresh_seeds_and_the_frozen_panel_keeps_its_own(self):
        self.assertEqual(pilot.panel_seed(1, 2, 3), pilot.seed(["spy-reset-pilot-v1/eval", 1, 2, 3]))
        self.assertEqual(pilot.panel_seed(1, 2, 3, pilot.SD1_PANEL_NS), pilot.seed(["sd1-panel-v1", 1, 2, 3]))
        self.assertNotEqual(pilot.panel_seed(1, 2, 3), pilot.panel_seed(1, 2, 3, pilot.SD1_PANEL_NS))
        with self.assertRaisesRegex(ValueError, "unknown panel seed namespace"):
            pilot.panel_seed(1, 2, 3, "sd1-panel-v2")
        frozen, fresh = panel.cases(), panel.cases(pilot.SD1_PANEL_NS)
        self.assertEqual(len(fresh), 1944)
        self.assertEqual([{k: v for k, v in c.items() if k != "seed"} for c in fresh],
                         [{k: v for k, v in c.items() if k != "seed"} for c in frozen])
        self.assertFalse({c["seed"] for c in fresh} & {c["seed"] for c in frozen})
        decks = pilot.load_decks()
        candidate, opponent = {"tag": "candidate"}, {"tag": "opponent"}
        plan = panel.build("r1-parent", candidate, "T1", opponent, decks, self.root / "frozen", 2)
        self.assertNotIn("seed_namespace", plan)
        sd1_plan = panel.build("r1-parent", candidate, "T1", opponent, decks, self.root / "fresh", 2,
                               pilot.SD1_PANEL_NS)
        self.assertEqual(sd1_plan["seed_namespace"], "sd1-panel-v1")
        self.assertNotEqual(sd1_plan["schedule_sha256"], plan["schedule_sha256"])
        request = json.loads(Path(sd1_plan["requests"][0]["request"]["path"]).read_text(encoding="utf-8"))
        first = sd1_plan["requests"][0]["cases"][0]
        self.assertEqual(request["matches"][0]["config"]["seed"],
                         pilot.panel_seed(first["own"], first["other"], first["repeat"], pilot.SD1_PANEL_NS))

    def test_the_analyzer_checks_each_rows_seed_in_its_panels_namespace(self):
        rows, labels = [], []
        name = "r1-parent-vs-T1"
        for o in range(pilot.DECKS):
            for t in range(pilot.DECKS):
                for k in range(pilot.PANEL_REPEATS):
                    for s in (0, 1):
                        rows.append({"panel": name, "checkpoint_sha256": "a" * 64, "own": o, "other": t,
                                     "repeat": k, "seat": s, "seed": pilot.panel_seed(o, t, k, pilot.SD1_PANEL_NS),
                                     "score": 1, "complete": True})
                        if o == pilot.SPY:
                            labels.append({"id": f"{name}/o{o}-t{t}-k{k}-s{s}", "kind": "label_replay",
                                           "focal": s, "winner": s, "win": True, "combo_resolved": False,
                                           "combo_won": False, "labels": {}})
        entry = {"checkpoint": "r1-parent", "opponent": "T1", "checkpoint_sha256": "a" * 64,
                 "rows": str(write_lines(self.root / "rows.jsonl", rows)["path"]),
                 "labels": str(write_lines(self.root / "labels.jsonl", labels)["path"])}
        data = analyze.load({"panels": [entry]}, {"r1-parent"}, pilot.SD1_PANEL_NS)
        self.assertEqual(len(data[("r1-parent", "T1")]), 1944)
        with self.assertRaisesRegex(ValueError, "seed differs"):
            analyze.load({"panels": [entry]}, {"r1-parent"})


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


# ------------------------------------------------------- stage 0: custody

# Synthetic roots: the utility screen's hot root on the maintainer's desktop
# (a Windows spelling) and computehost's runtime and run roots.
HOT = "Q:/utility-hot"
REMOTE = "/srv/computehost/runtime"
REMOTE_RUNS = "/srv/computehost/runs"


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def as_pin(path: str, data: bytes) -> dict:
    return {"path": path, "sha256": digest(data)}


def write_bytes(path: Path, data: bytes) -> dict:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return {"path": str(path), "sha256": digest(data)}


class Custody(Temp):
    """A sealed EVIDENCE.tar like the utility screen's: two input bundles
    (the desktop's and computehost's, staged on the desktop with computehost's
    paths inside), a desktop game and a computehost game with their job
    specs, ANALYSIS-SOURCES-001 and the allocation it pins."""

    def setUp(self):
        super().setUp()
        self.sealed = {}
        self.objects = {}
        for host, runtime in (("desk", f"{HOT}/inputs-001/desk"), ("remote", REMOTE)):
            local = f"{HOT}/inputs-001/{host}"
            checkpoint, parameters = b'{"checkpoint":1}', b"parameters"
            ck, par = digest(checkpoint), digest(parameters)
            self.objects = {"checkpoint": ck, "parameters": par}
            self.seal(f"inputs/{host}/objects/{ck}.json", f"{local}/objects/{ck}.json", checkpoint)
            self.seal(f"inputs/{host}/objects/{par}.bin", f"{local}/objects/{par}.bin", parameters)
            imported = json.dumps({"schema": "import",
                                   "parameters": as_pin(f"{runtime}/objects/{par}.bin", parameters)}).encode()
            self.seal(f"inputs/{host}/metadata/import.json", f"{local}/metadata/import.json", imported)
            source = {"checkpoint": as_pin(f"{runtime}/objects/{ck}.json", checkpoint),
                      "play_import": as_pin(f"{runtime}/metadata/import.json", imported), "feature_transfer": {}}
            self.seal(f"inputs/{host}/sources/r1-source.json", f"{local}/sources/r1-source.json",
                      json.dumps(source).encode())
            self.seal(f"inputs/{host}/bin/regret.exe", f"{local}/bin/regret.exe", b"binary")
        desk_source = f"{HOT}/inputs-001/desk/sources/r1-source.json"
        self.seal("formal/job-001/game.jsonl", f"{HOT}/formal/job-001/game.jsonl",
                  b'{"kind":"budget_game","id":"g1"}\n')
        # Windows spellings: backslashes and another case name the same file.
        self.seal("formal/job-001/command.json", f"{HOT}/formal/job-001/command.json", json.dumps({
            "command": [f"{HOT}/inputs-001/desk/bin/regret.exe".replace("/", "\\"), "source=" + desk_source,
                        "out=" + f"{HOT}/formal/job-001/game.jsonl".replace("/", "\\"), "mode=s4a-budget"],
            "environment": {"OPPONENTS": f"T1={desk_source},A48={desk_source.upper()}", "S4A_MODEL": "r1"},
            "cwd": f"{HOT}/formal/job-001".replace("/", "\\")}).encode())
        self.seal("remote-runs/job-002/game.jsonl", f"{HOT}/remote-runs/job-002/game.jsonl",
                  b'{"kind":"budget_game","id":"g2"}\n')
        self.seal("remote-runs/job-002/command.json", f"{HOT}/remote-runs/job-002/command.json", json.dumps({
            "command": [f"{REMOTE}/bin/regret.exe", f"source={REMOTE}/sources/r1-source.json",
                        f"out={REMOTE_RUNS}/job-002/game.jsonl", "mode=s4a-budget"],
            "environment": {"OPPONENTS": f"T1={REMOTE}/sources/r1-source.json",
                            "TRACE": f"{REMOTE_RUNS}/scratch/trace"},
            "cwd": f"{REMOTE_RUNS}/job-002"}).encode())
        allocation = b'{"allocation":1}'
        self.seal("metadata/allocation.json", f"{HOT}/allocation.json", allocation)
        records = [as_pin(*self.sealed[m]) for m in ("formal/job-001/game.jsonl", "remote-runs/job-002/game.jsonl")]
        self.seal("metadata/ANALYSIS-SOURCES-001.json", f"{HOT}/ANALYSIS-SOURCES-001.json", json.dumps(
            {"records": records, "allocation": as_pin(f"{HOT}/allocation.json", allocation)}).encode())
        self.seal("metadata/stale.json", f"{HOT}/stale.json", json.dumps(
            {"allocation": {"path": f"{HOT}/allocation.json", "sha256": "0" * 64}}).encode())
        build = self.root / "build" / "EVIDENCE.tar"
        build.parent.mkdir()
        with tarfile.open(build, "w", format=tarfile.USTAR_FORMAT) as tar:
            for name, (_, data) in sorted(self.sealed.items()):
                info = tarfile.TarInfo(name)
                info.size, info.mtime = len(data), 0
                tar.addfile(info, io.BytesIO(data))
        archive = build.read_bytes()
        desk_bytes = self.sealed["inputs/desk/sources/r1-source.json"][1]
        self.targets = [self.root / "cold" / "evidence-002", self.root / "recovery" / "evidence-002"]
        seal = {"schema": sd1.SEAL_SCHEMA, "complete": True, "originals_retained": True,
                "files": [{"name": m, "bytes": len(d), "sha256": digest(d)}
                          for m, (_, d) in sorted(self.sealed.items())],
                "original_archive_members": [{"original": as_pin(o, d), "member": m}
                                             for m, (o, d) in sorted(self.sealed.items())]
                # An alias: another retained original path of a sealed member.
                + [{"original": as_pin(f"{HOT}/staged/r1-source.json", desk_bytes),
                    "member": "inputs/desk/sources/r1-source.json"}],
                "archives": [{"path": common.absolute(t / "EVIDENCE.tar"), "sha256": digest(archive)}
                             for t in self.targets],
                "completion_record": "CUSTODY-COMPLETION.json"}
        for t in self.targets:
            write_bytes(t / "EVIDENCE.tar", archive)
            write(t / "SOURCE-SEAL.json", seal)
        seals = [common.pin(t / "SOURCE-SEAL.json") for t in self.targets]
        for t in self.targets:
            write(t / "CUSTODY-COMPLETION.json",
                  {"complete": True, "archives": seal["archives"], "source_seals": seals})
        write_bytes(self.root / "hot-copy" / "EVIDENCE.tar", archive)
        self.parent = write(self.root / "screen" / "parent-r1.json", {"parent": 1})
        self.binary = write_bytes(self.root / "runtime" / "regret_census_v1.exe", b"binary")

    def seal(self, member: str, original: str, data: bytes):
        self.sealed[member] = (original, data)

    def member(self, name: str, role: str, remap: bool = False) -> dict:
        entry = {"role": role, "member": name, "sha256": digest(self.sealed[name][1])}
        return {**entry, "remap": True} if remap else entry

    def spec(self, target: int = 0, **changes) -> dict:
        durable = self.targets[target]
        members = [self.member("formal/job-001/game.jsonl", "record-r1"),
                   {"role": "record-r1", "original": as_pin(*self.sealed["remote-runs/job-002/game.jsonl"])},
                   self.member("formal/job-001/command.json", "job-spec", remap=True),
                   self.member("remote-runs/job-002/command.json", "job-spec", remap=True),
                   self.member("metadata/ANALYSIS-SOURCES-001.json", "analysis-sources", remap=True),
                   self.member("metadata/allocation.json", "allocation")]
        for name in sorted(self.sealed):
            if name.startswith("inputs/") and not name.endswith(".exe"):
                members.append(self.member(name, "input", remap="/sources/" in name or "/metadata/" in name))
        value = {"schema": sd1.CUSTODY_SPEC_SCHEMA, "seal": common.pin(durable / "SOURCE-SEAL.json"),
                 "archive": common.pin(durable / "EVIDENCE.tar"),
                 "completion": common.pin(durable / "CUSTODY-COMPLETION.json"),
                 "forbidden": [HOT, common.absolute(self.root / "hot-copy")], "members": members,
                 "rewrite": [{"from": f"{HOT}/inputs-001/desk/bin/regret.exe", "to": self.binary["path"]},
                             {"from": f"{REMOTE}/bin/regret.exe", "to": self.binary["path"]},
                             {"from": REMOTE, "to": f"{HOT}/inputs-001/remote"},
                             {"from": REMOTE_RUNS, "to": f"{HOT}/remote-runs"}],
                 "keep": [f"{REMOTE_RUNS}/scratch"],
                 "files": [{"role": "parent", **self.parent}, {"role": "utility-binary", **self.binary}],
                 "counts": {"record-r1": 2, "job-spec": 2}}
        value.update(changes)
        return value

    def test_members_extract_through_the_seal_and_inner_paths_name_sd1s_copies(self):
        out_dir = self.root / "sd1" / "custody"
        out = sd1.custody(self.spec(), out_dir)
        self.assertEqual(out["failures"], [])
        self.assertTrue(out["pass"])
        members = {m["member"]: m for m in out["members"]}
        self.assertEqual(len(members), 6 + 2 * 4)
        for name in members:
            self.assertEqual((out_dir / "members" / name).read_bytes(), self.sealed[name][1])

        def mine(*parts):
            return common.absolute(out_dir.joinpath(*parts))

        desk = json.loads((out_dir / "remapped/formal/job-001/command.json").read_text(encoding="utf-8"))
        desk_source = mine("remapped", "inputs/desk/sources/r1-source.json")
        self.assertEqual(desk["command"], [self.binary["path"], "source=" + desk_source,
                                           "out=" + mine("members", "formal/job-001/game.jsonl"), "mode=s4a-budget"])
        self.assertEqual(desk["environment"], {"OPPONENTS": f"T1={desk_source},A48={desk_source}", "S4A_MODEL": "r1"})
        self.assertEqual(desk["cwd"], mine("members", "formal/job-001"))
        remote = json.loads((out_dir / "remapped/remote-runs/job-002/command.json").read_text(encoding="utf-8"))
        remote_source = mine("remapped", "inputs/remote/sources/r1-source.json")
        self.assertEqual(remote["command"], [self.binary["path"], "source=" + remote_source,
                                             "out=" + mine("members", "remote-runs/job-002/game.jsonl"),
                                             "mode=s4a-budget"])
        self.assertEqual(remote["cwd"], mine("members", "remote-runs/job-002"))
        self.assertEqual(remote["environment"]["TRACE"], f"{REMOTE_RUNS}/scratch/trace")
        self.assertEqual(members["remote-runs/job-002/command.json"]["kept"], [f"{REMOTE_RUNS}/scratch/trace"])
        # The source's pins name SD1's copies: the checkpoint's bytes as
        # sealed, the descriptor remapped in turn (so its SHA-256 is new).
        ck, par = self.objects["checkpoint"], self.objects["parameters"]
        source = json.loads(Path(remote_source).read_text(encoding="utf-8"))
        self.assertEqual(source["checkpoint"], {"path": mine("members", f"inputs/remote/objects/{ck}.json"),
                                                "sha256": ck})
        descriptor = members["inputs/remote/metadata/import.json"]["remapped"]
        self.assertEqual(source["play_import"], descriptor)
        self.assertNotEqual(descriptor["sha256"], digest(self.sealed["inputs/remote/metadata/import.json"][1]))
        self.assertEqual(json.loads(Path(descriptor["path"]).read_text(encoding="utf-8"))["parameters"],
                         {"path": mine("members", f"inputs/remote/objects/{par}.bin"), "sha256": par})
        analysis = json.loads((out_dir / "remapped/metadata/ANALYSIS-SOURCES-001.json").read_text(encoding="utf-8"))
        self.assertEqual([r["path"] for r in analysis["records"]],
                         [mine("members", "formal/job-001/game.jsonl"),
                          mine("members", "remote-runs/job-002/game.jsonl")])
        for m in out["members"]:
            if m.get("remapped"):
                self.assertNotIn("utility-hot", Path(m["remapped"]["path"]).read_text(encoding="utf-8").casefold())
        self.assertEqual(Path(out["lists"]["record-r1"]["path"]).read_text(encoding="utf-8").splitlines(),
                         [mine("members", "formal/job-001/game.jsonl"),
                          mine("members", "remote-runs/job-002/game.jsonl")])
        self.assertEqual(Path(out["lists"]["parent"]["path"]).read_text(encoding="utf-8").splitlines(),
                         [self.parent["path"]])
        self.assertEqual(members["remote-runs/job-002/game.jsonl"]["originals"],
                         [f"{HOT}/remote-runs/job-002/game.jsonl"])
        self.assertEqual(members["inputs/desk/sources/r1-source.json"]["originals"],
                         [f"{HOT}/inputs-001/desk/sources/r1-source.json", f"{HOT}/staged/r1-source.json"])
        # The recovery copy is the other durable archive.
        self.assertTrue(sd1.custody(self.spec(target=1), self.root / "sd1" / "again")["pass"])

    def test_every_gap_and_mismatch_is_listed_and_refuses(self):
        spec = self.spec()
        par = self.objects["parameters"]
        spec["members"] = [m for m in spec["members"] if m.get("member") != f"inputs/remote/objects/{par}.bin"]
        spec["members"][0] = {**spec["members"][0], "sha256": "0" * 64}
        spec["members"].append(self.member("metadata/stale.json", "allocation", remap=True))
        spec["files"].append({"role": "parent", "path": str(self.root / "screen" / "missing.json"), "sha256": "1" * 64})
        Path(self.binary["path"]).write_bytes(b"another binary")
        spec["keep"] = []
        spec["counts"]["job-spec"] = 3
        spec_path = write(self.root / "spec.json", spec)["path"]
        with mock.patch("sys.stdout"):
            code = sd1.main(["custody", "--spec", spec_path, "--out-dir", str(self.root / "sd1"),
                             "--out", str(self.root / "RECORD-PINS.json")])
        self.assertEqual(code, 1)
        record = json.loads((self.root / "RECORD-PINS.json").read_text(encoding="utf-8"))
        self.assertFalse(record["pass"])
        failures = "\n".join(record["failures"])
        for needle in ("no member formal/job-001/game.jsonl with SHA-256 " + "0" * 64,
                       "missing.json is missing",
                       "regret_census_v1.exe has SHA-256 " + digest(b"another binary"),
                       f"is sealed as inputs/remote/objects/{par}.bin, which custody did not extract",
                       "metadata/stale.json: the pin of " + f"{HOT}/allocation.json has SHA-256 " + "0" * 64,
                       f"{REMOTE_RUNS}/scratch/trace resolves to no extracted member",
                       f"{REMOTE}/bin/regret.exe resolves to no extracted member",
                       "role job-spec: the spec lists 2 members and files, not 3"):
            self.assertIn(needle, failures)
        self.assertEqual(record["spec"]["sha256"], digest(Path(spec_path).read_bytes()))

    def test_only_a_durable_archive_beside_its_complete_seal_is_read(self):
        out = sd1.custody(self.spec(archive=common.pin(self.root / "hot-copy" / "EVIDENCE.tar")), self.root / "a")
        self.assertIn("is not one of the seal's durable archives", "\n".join(out["failures"]))
        self.assertFalse((self.root / "a" / "members").exists())
        out = sd1.custody(self.spec(seal=common.pin(self.targets[1] / "SOURCE-SEAL.json")), self.root / "b")
        self.assertIn("the seal does not sit beside the archive", out["failures"])
        out = sd1.custody(self.spec(seal={**self.spec()["seal"], "sha256": "0" * 64}), self.root / "c")
        self.assertIn("the source seal: ", out["failures"][0])
        self.assertIn("no member is extracted: the seal or the archive did not verify", out["failures"])
        incomplete = json.loads((self.targets[0] / "SOURCE-SEAL.json").read_text(encoding="utf-8"))
        incomplete["complete"] = False
        out = sd1.custody(self.spec(seal=write(self.targets[0] / "SOURCE-SEAL.json", incomplete)), self.root / "d")
        self.assertIn("the source seal is not a complete search-player-utility source seal", out["failures"])
        other = write(self.targets[0] / "CUSTODY-COMPLETION.json", {"complete": True, "archives": [],
                                                                     "source_seals": []})
        out = sd1.custody(self.spec(completion=other), self.root / "e")
        self.assertIn("the custody completion is not complete for this seal and archive", out["failures"])

    def test_a_malformed_spec_or_a_used_directory_refuses_before_reading(self):
        bad = self.spec()
        bad["members"].append({"role": "input", "member": "../outside", "sha256": "0" * 64})
        with self.assertRaisesRegex(ValueError, r"malformed: members\[14\]"):
            sd1.custody(bad, self.root / "a")
        with self.assertRaisesRegex(ValueError, "unknown field 'extra'"):
            sd1.custody({**self.spec(), "extra": 1}, self.root / "a")
        with self.assertRaisesRegex(ValueError, "rewrite"):
            sd1.custody(self.spec(rewrite=[{"from": "relative/path", "to": HOT}]), self.root / "a")
        used = self.root / "used"
        write(used / "x.json", {})
        with self.assertRaisesRegex(ValueError, "not a fresh directory"):
            sd1.custody(self.spec(), used)
        with self.assertRaisesRegex(ValueError, "hot or holding root"):
            sd1.custody(self.spec(forbidden=[common.absolute(self.root)]), self.root / "new")
        self.assertFalse((self.root / "a").exists())


# ------------------------------------------------ stage 0: parent identity


class Parents(Temp):
    def setUp(self):
        super().setUp()
        self.recorded, self.entries = {}, {}
        for name in ("r1", "r2", "T1", "A48"):
            checkpoint, parameters = f"checkpoint {name}".encode(), f"parameters {name}".encode()
            self.recorded[name] = digest(checkpoint)
            for side in ("utility", "screen"):
                folder = self.root / side / name
                source = self.source(folder, checkpoint, parameters)
                if side == "screen" and name in common.LINEAGES:
                    # The distillation screen's parent is O's recorded initial source.
                    config = write(folder / "O-config.json", {"initial_source": source, "iterations": []})
                    entry = {**config, "field": ["initial_source"]}
                else:
                    entry = {"path": write(folder / "source.json", source)["path"]}
                self.entries.setdefault(name, {})[side] = entry

    def source(self, folder: Path, checkpoint: bytes, parameters: bytes, transfer="c") -> dict:
        descriptor = write(folder / "import.json", {"schema": "import",
                                                    "parameters": write_bytes(folder / "par.bin", parameters)})
        return {"checkpoint": write_bytes(folder / "ck.json", checkpoint), "play_import": descriptor,
                "feature_transfer": {"expected_feature_contract_digest": transfer}}

    def spec(self) -> dict:
        return {"schema": sd1.PARENTS_SPEC_SCHEMA, **copy.deepcopy(self.entries)}

    def parents(self, spec: dict) -> dict:
        with mock.patch.dict(sd1.RECORDED_CHECKPOINTS, self.recorded):
            return sd1.parents(spec)

    def test_the_parents_and_references_are_their_recorded_checkpoints_and_sources(self):
        self.assertEqual(sd1.RECORDED_CHECKPOINTS, {**common.PARENT_CHECKPOINT_SHA256,
                                                    "T1": sd1.T1_CHECKPOINT_SHA256, "A48": sd1.A48_CHECKPOINT_SHA256})
        out = self.parents(self.spec())
        self.assertEqual(out["failures"], [])
        self.assertTrue(out["pass"])
        self.assertTrue(all(out["sources"][name]["sources_match"] for name in self.recorded))
        spec = self.spec()
        del spec["T1"]["screen"], spec["A48"]["screen"]
        out = self.parents(spec)
        self.assertTrue(out["pass"])
        self.assertNotIn("sources_match", out["sources"]["T1"])

    def test_a_different_checkpoint_or_source_fails_with_every_reason(self):
        spec = self.spec()
        # r1: the utility screen's descriptor names other parameters.
        other = self.source(self.root / "other" / "r1", b"checkpoint r1", b"other parameters")
        spec["r1"]["utility"] = {"path": write(self.root / "other" / "r1" / "source.json", other)["path"]}
        # r2: the distillation screen's config changed after its pin.
        Path(spec["r2"]["screen"]["path"]).write_text("{}", encoding="utf-8")
        # T1: its checkpoint's bytes changed in place.
        Path(self.root / "utility" / "T1" / "ck.json").write_bytes(b"changed")
        # A48: a source whose checkpoint is not the recorded A48 (another transfer besides).
        wrong = self.source(self.root / "other" / "A48", b"checkpoint T1", b"parameters A48", transfer="d")
        spec["A48"]["utility"] = {"path": write(self.root / "other" / "A48" / "source.json", wrong)["path"]}
        out_path = self.root / "PARENTS.json"
        with mock.patch.dict(sd1.RECORDED_CHECKPOINTS, self.recorded), mock.patch("sys.stdout"):
            code = sd1.main(["parents", "--spec", write(self.root / "spec.json", spec)["path"], "--out", str(out_path)])
        self.assertEqual(code, 1)
        failures = json.loads(out_path.read_text(encoding="utf-8"))["failures"]
        text = "\n".join(failures)
        self.assertIn("r1: the utility screen's source differs from the distillation screen's in ['play_import']", text)
        self.assertIn("r2: the distillation screen's source: ", text)
        self.assertIn("T1: the utility screen's source checkpoint: ", text)
        self.assertIn(f"A48: the utility screen's source: its checkpoint is {self.recorded['T1']}, not the recorded "
                      f"{self.recorded['A48']}", text)
        self.assertIn("A48: the utility screen's source differs from the distillation screen's in "
                      "['checkpoint', 'feature_transfer']", text)

    def test_the_spec_names_both_screens_sources_of_each_parent(self):
        spec = self.spec()
        del spec["r1"]["screen"]
        with self.assertRaisesRegex(ValueError, "r1: the utility screen's source and the distillation screen's"):
            self.parents(spec)
        spec = self.spec()
        del spec["A48"]
        with self.assertRaisesRegex(ValueError, "naming r1, r2, T1 and A48"):
            self.parents(spec)


# --------------------------------------- stage 0: the probe's references


def reference_readout(cid: str, gap=4.0, agree=0.9, kl=0.05) -> dict:
    offers = {"offers": 2, "mean_deployed_self_probability": 0.25, "mean_raw_self_probability": 0.2,
              "self_top_share": 0.5, "rows": [{"episode": "e1", "step": 3, "deployed_self_probability": 0.3,
                                               "raw_self_probability": 0.2, "self_top": True},
                                              {"episode": "e2", "step": 5, "deployed_self_probability": 0.2,
                                               "raw_self_probability": 0.2, "self_top": False}]}
    return {"id": cid, "state_sha256": f"state-{cid}", "learner_offers": offers,
            "anchor_held_out": {"rows": 10, "median_clamped_top_two_gap": gap, "non_spy_top_agreement": agree,
                                "mean_kl_from_parent_non_spy": kl, "mean_kl_from_parent_all": 0.1}}


class ReferenceCheck(Temp):
    def probe(self, where="screen", extra=()) -> dict:
        candidates = [reference_readout("parent", gap=5.0, agree=1.0, kl=0.0), reference_readout("O-final")]
        return {"schema": sd1.PROBE_SCHEMA, "complete": True, "lineage": "r1", "parent_state_sha256": "ps",
                "anchor": {"held_out": {"path": f"{where}/anchor.json", "sha256": "a" * 64}},
                "offers": [{"path": f"{where}/offers.json", "sha256": "b" * 64}],
                "banks": [{"role": "training", "bank": {"path": f"{where}/bank.json", "sha256": "c" * 64}}],
                "runtime_commit": where, "candidates": candidates + [reference_readout(c) for c in extra]}

    def test_the_stage0_probes_references_equal_the_screens_bit_for_bit(self):
        screen = self.probe(extra=("U-step-1",))
        out = sd1.reference_check("r1", self.probe("sd1"), screen)
        self.assertEqual(out["differing"], [])
        self.assertTrue(out["pass"] and out["banks_equal"])
        self.assertEqual(out["checks"]["parent learner_offers"]["probe"]["offers"], 2)
        changes = {"O-final mean_kl_from_parent_non_spy": lambda p: p["candidates"][1]["anchor_held_out"].update(
                       mean_kl_from_parent_non_spy=math.nextafter(0.05, 1.0)),
                   "parent mean_kl_from_parent_non_spy": lambda p: p["candidates"][0]["anchor_held_out"].update(
                       mean_kl_from_parent_non_spy=-0.0),
                   "parent median_clamped_top_two_gap": lambda p: p["candidates"][0]["anchor_held_out"].update(
                       median_clamped_top_two_gap=None),
                   "parent learner_offers": lambda p: p["candidates"][0]["learner_offers"]["rows"][1].update(
                       self_top=True),
                   "O-final state_sha256": lambda p: p["candidates"][1].update(state_sha256="other"),
                   "anchor": lambda p: p["anchor"]["held_out"].update(sha256="d" * 64)}
        for name, change in changes.items():
            mine = self.probe("sd1")
            change(mine)
            out = sd1.reference_check("r1", mine, screen)
            self.assertEqual(out["differing"], [name])
            self.assertFalse(out["pass"])
        with self.assertRaisesRegex(ValueError, "another lineage"):
            sd1.reference_check("r2", self.probe("sd1"), screen)
        lacking = self.probe("sd1")
        lacking["candidates"] = lacking["candidates"][:1]
        with self.assertRaisesRegex(ValueError, "lacks the O-final"):
            sd1.reference_check("r1", lacking, screen)

    def test_the_cli_reads_the_screens_probe_by_its_recorded_sha256(self):
        mine = write(self.root / "sd1" / "probe.json", self.probe("sd1"))
        screen = write(self.root / "screen" / "probe.json", self.probe())
        base = ["reference-check", "--lineage", "r1", "--probe", mine["path"], "--screen-probe", screen["path"],
                "--out", str(self.root / "REFERENCES.json")]
        with self.assertRaisesRegex(ValueError, "recorded SHA-256"):
            sd1.main(base + ["--screen-probe-sha256", "0" * 64])
        with mock.patch("sys.stdout"):
            self.assertEqual(sd1.main(base + ["--screen-probe-sha256", screen["sha256"]]), 0)
        record = json.loads((self.root / "REFERENCES.json").read_text(encoding="utf-8"))
        # The record's pins spell paths with forward slashes, as pin() does.
        self.assertEqual((record["probe"], record["screen_probe"]),
                         (common.pin(mine["path"]), common.pin(screen["path"])))


# ------------------------------------------------------------- fit table


class FitTable(SelectFixture):
    def seed_inputs(self, seed: int, changes=None) -> tuple:
        label = f"SD1-T-s{seed}"

        def relabel(value):
            return json.loads(json.dumps(value).replace(LABEL, label))

        return relabel(self.full), relabel(self.rerun), relabel(self.probe(changes))

    def selections(self, changes=None) -> tuple:
        selections, probes = [], []
        for k in sd1.SEEDS:
            full, rerun, probe = self.seed_inputs(k, (changes or {}).get(k))
            selections.append(sd1.select("r1", k, probe, self.reference, full, rerun, names=NAMES))
            probes.append(probe)
        return selections, probes

    def test_every_seed_passing_reads_d1_and_the_held_out_values_at_read_checkpoints(self):
        out = sd1.fit_table(*self.selections())
        r1 = out["lineages"]["r1"]
        self.assertEqual(r1["summary"], "passes in 8 of 8 seeds; phase 2 and panels follow")
        self.assertTrue(r1["phase2"])
        self.assertFalse(out["engineering"])
        row = r1["seeds"][3]
        self.assertEqual((row["seed"], row["label"], row["result"]),
                         (3, "SD1-T-s3", "F, G2 and G3 pass at some read gated checkpoint"))
        self.assertEqual(row["reading"], "passes: selected step s = 64, fit dose f = 64, highest passing step h = 8192")
        self.assertEqual({k: row["d1"][k] for k in ("s", "f", "h", "minimizing_step")},
                         {"s": 64, "f": 64, "h": 8192, "minimizing_step": 128})
        self.assertEqual(row["d1"]["checkpoint"]["state_sha256"], "state-SD1-T-s3-step-64")
        self.assertEqual((row["f_first_pass"], row["g2_first_failure"]), (64, None))
        self.assertTrue(row["g3"]["pass"])
        self.assertEqual([c["step"] for c in row["read_checkpoints"]], list(sd1.CHECKPOINT_STEPS))
        at64 = row["read_checkpoints"][3]
        self.assertEqual(at64["held_out"], {"separated_mass": sd1.naive_mean([8 / 64, 6 / 64, 9 / 64, 7 / 64]),
                                            "mean_kl_target": 0.3, "rows": 4})
        self.assertEqual(at64["non_spy"], {"top_agreement": 0.95, "mean_kl_from_parent": 0.01})
        self.assertEqual(r1["non_spy_beside"]["agreement_floor"], 0.9 - sd1.AGREEMENT_MARGIN)
        self.assertEqual(r1["non_spy_beside"]["kl_ceiling"], 0.05)

    def test_seeds_without_d1_read_their_rows_and_the_lineage_runs_no_phase_2(self):
        collapse = dict(training=0.3, held=(24, 16, 20, 20), agree=0.5)
        together = dict(training=0.2, held=(24, 16, 20, 20), agree=0.5)
        unfit = {s: dict(training=0.3, held=(9, 9, 9, 9)) for s in sd1.GATED_STEPS}
        out = sd1.fit_table(*self.selections({5: unfit, 6: {32: collapse}, 7: {32: together}}))
        r1 = out["lineages"]["r1"]
        self.assertEqual(r1["summary"], "passes in 5 of 8 seeds; required fit reached only together with collapse "
                                        "in 1; collapse before the required fit in 1; required fit not reached in 1; "
                                        "no phase 2 or panels")
        self.assertFalse(r1["phase2"])
        five, six, seven = r1["seeds"][5:]
        self.assertEqual(five["result"], "F never passes, with G2 holding through 8,192 steps")
        self.assertEqual(len(five["read_checkpoints"]), len(sd1.CHECKPOINT_STEPS))
        self.assertEqual(six["result"], "G2 fails before any checkpoint passes F")
        self.assertEqual(six["reading"],
                         "collapse before the required fit, at this dose, optimizer, targets and anchor")
        self.assertEqual(six["g2_first_failure"], {"step": 32, "failed": ["agreement"]})
        self.assertIsNone(six["f_first_pass"])
        self.assertEqual([c["step"] for c in six["read_checkpoints"]], [8, 16, 32])
        self.assertEqual(six["read_checkpoints"][-1]["held_out"]["separated_mass"], sd1.naive_mean([24 / 64, 16 / 64,
                                                                                                    20 / 64, 20 / 64]))
        self.assertEqual(seven["result"], "F first passes at the checkpoint where G2 first fails")
        self.assertEqual(seven["f_first_pass"], 32)
        self.assertIn("most plausibly r1's KL", seven["explanations"][-1])
        self.assertNotIn("d1", six)

    def test_an_engineering_stop_is_its_own_row_and_the_cli_exits_1(self):
        selections, probes = self.selections({2: {64: dict(training=float("nan"), held=(8, 6, 9, 7))}})
        out = sd1.fit_table(selections, probes)
        r1 = out["lineages"]["r1"]
        two = r1["seeds"][2]
        self.assertEqual((two["result"], two["cause"]),
                         ("G3 fails", "engineering: G3 failed, so SD1 stops until the cause is fixed"))
        self.assertEqual([c["step"] for c in two["read_checkpoints"]], [8, 16, 32])
        self.assertEqual(r1["summary"], "passes in 7 of 8 seeds; engineering stop in 1; SD1 stops until the cause "
                                        "is fixed")
        self.assertTrue(out["engineering"])
        paths = [write(self.root / f"SELECT-{k}.json", s)["path"] for k, s in enumerate(selections)]
        probe_paths = [write(self.root / f"PROBE-{k}.json", p)["path"] for k, p in enumerate(probes)]
        with mock.patch("sys.stdout"):
            code = sd1.main(["fit", "--selection", *paths, "--probes", *probe_paths, "--out",
                             str(self.root / "FIT.json")])
        self.assertEqual(code, 1)
        record = json.loads((self.root / "FIT.json").read_text(encoding="utf-8"))
        self.assertEqual(record["lineages"]["r1"]["summary"], r1["summary"])
        self.assertEqual(len(record["selections"]), 8)

    def test_every_value_is_read_again_from_the_seeds_probe(self):
        selections, probes = self.selections()
        changed = copy.deepcopy(probes)
        changed[4]["candidates"][-1]["evidence"]["held_out"]["row_values"][0]["separated_mass"] += 1 / 64
        with self.assertRaisesRegex(ValueError, "SD1-T-s4 step 8192: the selection's values are not this probe's"):
            sd1.fit_table(selections, changed)
        with self.assertRaisesRegex(ValueError, "all eight seeds"):
            sd1.fit_table(selections[:7], probes)
        with self.assertRaisesRegex(ValueError, "two probes read this seed"):
            sd1.fit_table(selections, probes + probes[:1])
        with self.assertRaisesRegex(ValueError, "no probe read this seed"):
            sd1.fit_table(selections, probes[1:])


# ----------------------------------------------------- phase-2 config gate


class ConfigsCheck(PhaseTwoFixture):
    def configs(self) -> tuple:
        selections = [self.selection(k) for k in sd1.SEEDS]
        cfgs = sd1.phase2_configs("r1", self.o_config, selections, 4, "runs")
        cfgs["O-0"] = phase2.config("r1", self.o_config, None, 6, "runs/r1-O-s0/native")
        return selections, cfgs

    def test_placement_aside_each_config_differs_from_o_0_only_as_designed(self):
        selections, cfgs = self.configs()
        out = sd1.configs_check("r1", self.o_config, selections, cfgs)
        self.assertTrue(out["pass"], out["checks"])
        self.assertEqual(len(out["checks"]), 17)
        units = sum(len(u["episodes"]) for u in self.o_config["iterations"])
        diff = out["differences_from_O-0"]
        self.assertEqual(diff["O-0"], {"fields": [], "units": units, "same_updates": True, "episode_ids": 0,
                                       "seeds": 0, "other_unit_fields": 0})
        self.assertEqual(diff["O-3"], {**diff["O-0"], "episode_ids": units, "seeds": units})
        self.assertEqual(diff["D2-0"], {**diff["O-0"], "fields": ["initial_source"]})
        self.assertEqual(diff["D2-5"], {**diff["O-3"], "fields": ["initial_source"]})
        self.assertEqual(out["placement"]["O-0"]["collection_workers"], 6)
        self.assertEqual(out["placement"]["D2-1"]["collection_workers"], 4)

    def test_any_other_difference_fails_the_gate(self):
        selections, cfgs = self.configs()
        bad = copy.deepcopy(cfgs)
        bad["O-2"]["iterations"][5]["episodes"][0]["episode"]["starting_player"] ^= 1
        bad["D2-4"] = copy.deepcopy(cfgs["D2-5"])
        bad["O-0"] = copy.deepcopy(cfgs["O-1"])
        bad["O-6"]["update_backend"] = {"kind": "other"}
        bad["O-7"]["output_directory"] = bad["O-5"]["output_directory"]
        out = sd1.configs_check("r1", self.o_config, selections, bad)
        self.assertFalse(out["pass"])
        self.assertEqual(sorted(k for k, v in out["checks"].items() if not v),
                         ["D2-4 is O-4 from D1-4", "O-0 is O's recorded config with the named parent",
                          "O-2 is O-0 on replicate 2's seeds and episode ids",
                          "O-6 is O-0 on replicate 6's seeds and episode ids",
                          "each run writes its own output directory"])
        diff = out["differences_from_O-0"]
        self.assertEqual((diff["O-2"]["other_unit_fields"], diff["O-6"]["fields"]), (1, ["update_backend"]))

    def test_the_cli_reads_all_sixteen_configs_and_every_selection(self):
        selections, cfgs = self.configs()
        o_path = write(self.root / "O.json", self.o_config)["path"]
        select = [write(self.root / f"SELECT-{k}.json", s)["path"] for k, s in enumerate(selections)]
        configs = [f"{name}={write(self.root / f'{name}.json', cfg)['path']}" for name, cfg in sorted(cfgs.items())]
        base = ["configs-check", "--lineage", "r1", "--o-config", o_path, "--select", *select,
                "--out", str(self.root / "CONFIGS.json")]
        with mock.patch("sys.stdout"):
            self.assertEqual(sd1.main(base + ["--config", *configs]), 0)
        record = json.loads((self.root / "CONFIGS.json").read_text(encoding="utf-8"))
        self.assertTrue(record["pass"])
        self.assertEqual(sorted(record["configs"]), sorted(sd1.PHASE2_RUNS))
        with self.assertRaisesRegex(ValueError, "the gate reads D2-0 to D2-7 and O-0 to O-7"):
            sd1.main(base + ["--config", *configs[1:]])
        with self.assertRaisesRegex(ValueError, "once for each"):
            sd1.main(base + ["--config", *configs, configs[0]])
        with self.assertRaisesRegex(ValueError, "every seed's selection"):
            sd1.main(base[:6] + select[:7] + base[-2:] + ["--config", *configs])


if __name__ == "__main__":
    unittest.main()
