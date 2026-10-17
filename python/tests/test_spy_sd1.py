"""Search-evidence distillation (tools/spy_distill/sd1.py): fixture tests. Nothing is searched,
played or trained; every schedule, record, replay, bank, probe and run here is synthetic."""
import copy
import hashlib
import io
import json
import math
import os
import shutil
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
        # Custody reads no file through a link, so the fixtures live under the directory's real path (a temporary
        # directory may itself sit under one, as /var does on macOS).
        self.root = Path(os.path.realpath(self.temp.name))

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
            written = json.loads((Path(d) / "S.json").read_text())
            self.assertEqual(written["counts"]["held_out"], 36)
            # The split pins the schedule it was fixed on.
            self.assertEqual(written["schedule"], common.pin(path))
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
    """A lineage's record files as custody's copies of its role r1-records,
    and the schedule custody verified."""

    def setUp(self):
        super().setUp()
        self.cases = schedule()
        self.sched = write_lines(self.root / "screen" / "formal-r1.jsonl", self.cases)
        self.paths, members = [], []
        for c in self.cases:
            copy_pin = write_lines(self.root / "custody" / "members" / "formal" / c["id"] / "game.jsonl", [record(c)])
            self.paths.append(copy_pin["path"])
            members.append({"role": "r1-records", "member": f"formal/{c['id']}/game.jsonl", "kind": "record",
                            "sha256": copy_pin["sha256"], "id": c["id"], "extracted": copy_pin})
        self.custody = {"schema": sd1.CUSTODY_SCHEMA, "pass": True, "forbidden": [HOT],
                        "files": [{"role": "r1-schedule", **self.sched, "verified": True}], "members": members}
        self.custody_path = self.write_custody(self.custody)
        self.listing = self.list_files(self.paths)

    def fresh(self, name: str) -> Path:
        """A new file name for each call, so earlier files stay as written."""
        self.written = getattr(self, "written", 0) + 1
        return self.root / "inputs" / f"{self.written}-{name}"

    def write_custody(self, value: dict) -> Path:
        return Path(write(self.fresh("CUSTODY.json"), value)["path"])

    def list_files(self, paths) -> Path:
        listing = self.fresh("list.txt")
        listing.parent.mkdir(parents=True, exist_ok=True)
        listing.write_text("".join(p + "\n" for p in paths), encoding="utf-8")
        return listing

    def records(self, listing=None, custody=None, schedule_path=None, role="r1-records"):
        return sd1.records("r1", Path(schedule_path or self.sched["path"]), listing or self.listing,
                           custody or self.custody_path, role)

    def recustody(self, index: int, rows: list) -> Path:
        """Custody whose copy ``index`` holds ``rows``, with their SHA-256."""
        value = copy.deepcopy(self.custody)
        copy_pin = write_lines(Path(self.paths[index]), rows)
        value["members"][index].update(sha256=copy_pin["sha256"], extracted=copy_pin)
        return self.write_custody(value)

    def test_each_scheduled_case_binds_one_custody_copy(self):
        out = self.records()
        self.assertEqual([c["id"] for c in out["cases"]], [c["id"] for c in self.cases])
        self.assertEqual(out["cases"][0]["record"], {"path": self.paths[0],
                                                     "sha256": self.custody["members"][0]["sha256"]})
        self.assertEqual((out["schedule"], out["record_list"], out["custody"], out["role"]),
                         (common.pin(self.sched["path"]), common.pin(self.listing), common.pin(self.custody_path),
                          "r1-records"))
        with mock.patch("sys.stdout"):
            self.assertEqual(sd1.main(["records", "--lineage", "r1", "--schedule", self.sched["path"],
                                       "--record-list", str(self.listing), "--custody", str(self.custody_path),
                                       "--role", "r1-records", "--out", str(self.root / "RECORDS.json")]), 0)
        self.assertEqual(json.loads((self.root / "RECORDS.json").read_text(encoding="utf-8")), out)

    def test_a_custody_gap_or_mismatch_stops(self):
        with self.assertRaisesRegex(ValueError, "not exactly the schedule's cases"):
            self.records(self.list_files(self.paths[:-1]))
        with self.assertRaisesRegex(ValueError, "the record list repeats a file"):
            self.records(self.list_files(self.paths + self.paths[3:4]))
        with self.assertRaisesRegex(ValueError, "is not an absolute path"):
            self.records(self.list_files([f"formal/{self.cases[0]['id']}/game.jsonl"] + self.paths[1:]))
        stray = write_lines(self.root / "elsewhere" / "game.jsonl", [record(self.cases[0])])
        with self.assertRaisesRegex(ValueError, "is not one of custody's r1-records copies"):
            self.records(self.list_files([stray["path"]] + self.paths[1:]))
        # A copy whose bytes changed after custody.
        write_lines(Path(self.paths[5]), [record(self.cases[5], end="loss")])
        with self.assertRaisesRegex(ValueError, f"not custody's {self.custody['members'][5]['sha256']} for member "
                                                f"formal/{self.cases[5]['id']}/game.jsonl"):
            self.records()
        write_lines(Path(self.paths[5]), [record(self.cases[5])])
        self.assertEqual(len(self.records()["cases"]), len(self.cases))
        with self.assertRaisesRegex(ValueError, "exactly one budget_game row"):
            self.records(custody=self.recustody(3, [record(self.cases[3]), record(self.cases[3])]))
        with self.assertRaisesRegex(ValueError, f"its record is '{self.cases[4]['id']}', not custody's case "
                                                f"{self.cases[3]['id']}"):
            self.records(custody=self.recustody(3, [record(self.cases[4])]))
        write_lines(Path(self.paths[3]), [record(self.cases[3])])
        # A copy custody holds with no case id: the record's own id binds it, once.
        value = copy.deepcopy(self.custody)
        twin = write_lines(Path(self.paths[4]), [record(self.cases[3])])
        value["members"][4].update(id=None, sha256=twin["sha256"], extracted=twin)
        with self.assertRaisesRegex(ValueError, f"{self.cases[3]['id']}: two record files"):
            self.records(custody=self.write_custody(value))

    def test_custody_binds_the_schedule_the_role_and_the_roots(self):
        with self.assertRaisesRegex(ValueError, "is not the r1 schedule custody verified"):
            self.records(schedule_path=write_lines(self.root / "other" / "formal-r1.jsonl", self.cases[::-1])["path"])
        with self.assertRaisesRegex(ValueError, "is not a custody record that passed"):
            self.records(custody=self.write_custody({**self.custody, "pass": False}))
        with self.assertRaisesRegex(ValueError, "custody holds no copy of role 'r2-records'"):
            self.records(role="r2-records")
        holding = common.absolute(self.root / "custody" / "members")
        with self.assertRaisesRegex(ValueError, f"lies under the hot or holding root {holding}"):
            self.records(custody=self.write_custody({**self.custody, "forbidden": [HOT, holding]}))


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

    def test_a_record_file_the_rebuild_could_not_read_voids_its_game_by_file(self):
        # The engine's input void row: the list's line as its file, a null id and the reason.
        unread = {"schema": sd1.REBUILD_SCHEMA, "file": self.replays[0]["file"], "id": None,
                  "void": "the listed file cannot be read: access denied"}
        self.rebuilds[0] = unread
        out = self.run_games()
        self.assertEqual((out["kept"], out["void"]), (19, 1))
        self.assertEqual(self.status()["void"], [{"id": self.cases[0]["id"],
                                                  "reason": "root rebuild: the listed file cannot be read: access "
                                                            "denied"}])
        # A malformed line of a file the rebuild read gives its own row, and voids that game too.
        self.rebuilds.append({**unread, "file": self.replays[1]["file"], "void": "line 2 is not JSON"})
        with self.assertRaisesRegex(ValueError, "engineering: 2 of 20"):
            self.run_games()
        self.assertEqual(self.status()["void"][1], {"id": self.cases[1]["id"], "reason": "root rebuild: line 2 is not "
                                                                                          "JSON"})
        self.rebuilds[-1] = {**unread, "file": str(self.root / "unlisted.jsonl")}
        with self.assertRaisesRegex(ValueError, "an input void row of the rebuild names no listed record file"):
            self.run_games()
        self.rebuilds[-1] = {"schema": sd1.REBUILD_SCHEMA, "file": self.replays[1]["file"], "id": None}
        with self.assertRaisesRegex(ValueError, "not an input void row"):
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
        # The split pins the schedule it was fixed on (as the split command writes it), the one the rebuild checked.
        sched = write_lines(self.root / lineage / f"formal-{lineage}.jsonl", cases)
        the_split = {**sd1.split(lineage, cases), "schedule": sched}
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
                                 "schedule_sha256": sched["sha256"], "roots": roots, "first_choice_step": first,
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
                             "schedule_sha256": sched["sha256"], "void": "root rebuild: x"})
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

    def test_targets_read_the_split_fixed_on_the_rebuilds_schedule(self):
        fixtures = self.checks(games_count=40)
        spec = {lin: {"split": fx["split"], "games": fx["games"], "bank_check": fx["check_pin"]}
                for lin, fx in fixtures.items()}
        the_split = json.loads(Path(fixtures["r2"]["split"]["path"]).read_text(encoding="utf-8"))
        other = write_lines(self.root / "other" / "formal-r2.jsonl", schedule("r2")[::-1])
        spec["r2"]["split"] = write(self.root / "other" / "SPLIT.json", {**the_split, "schedule": other})
        with self.assertRaisesRegex(ValueError, "r2: the split was fixed on another schedule than the one the "
                                                "rebuild checked"):
            sd1.targets(spec, self.root / "stage2", names=NAMES)
        del the_split["schedule"]
        spec["r2"]["split"] = write(self.root / "other" / "SPLIT.json", the_split)
        with self.assertRaisesRegex(ValueError, "r2: the split pins no schedule"):
            sd1.targets(spec, self.root / "stage2", names=NAMES)

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
        # The split's held-out games and their matchups, as the probe's held-out rows name them.
        self.split = {"schema": sd1.SPLIT_SCHEMA, "lineage": "r1",
                      "games": {f"g{i}": "held_out" for i in range(len(MATCHUPS))},
                      "matchups": {f"g{i}": [NAMES.index(own), NAMES.index(other)]
                                   for i, (own, other) in enumerate(MATCHUPS)}}

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
                          split or self.split, names=NAMES)


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
        self.full["telemetry"] = write(Path(self.telemetry_pin["path"]), self.telemetry)
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

    def test_the_runs_telemetry_is_read_against_its_pin_and_the_split_is_this_lineages(self):
        self.telemetry[100]["archive_loss"] = float("inf")
        write(Path(self.telemetry_pin["path"]), self.telemetry)
        with self.assertRaisesRegex(ValueError, "SD1-T-s3 telemetry: .* is missing or differs"):
            self.select()
        self.telemetry[100]["archive_loss"] = 0.2
        write(Path(self.telemetry_pin["path"]), self.telemetry)
        self.assertEqual(self.select()["reading"], "passes")
        with self.assertRaisesRegex(ValueError, "not this lineage's split"):
            self.select(split={**self.split, "lineage": "r2"})

    def test_the_cli_pins_every_input(self):
        names = ("probe", "reference", "full", "rerun", "split")
        paths = {name: write(self.root / f"{name}.json", value)["path"]
                 for name, value in zip(names, (self.probe(), self.reference, self.full, self.rerun, self.split))}
        argv = ["select", "--lineage", "r1", "--seed", "3", *(x for name in names for x in (f"--{name}", paths[name])),
                "--out", str(self.root / "SELECT.json")]
        with mock.patch("sys.stdout"), mock.patch.object(sd1, "deck_names", return_value=NAMES):
            self.assertEqual(sd1.main(argv), 0)
        record = json.loads((self.root / "SELECT.json").read_text(encoding="utf-8"))
        self.assertEqual(record["reading"], "passes")
        self.assertEqual(record["inputs"], {name: common.pin(paths[name]) for name in names})
        with mock.patch("sys.stderr"), self.assertRaises(SystemExit):
            sd1.main([a for a in argv if a not in ("--split", paths["split"])])

    def test_the_run_must_be_this_seeds_keep_set_cell(self):
        with self.assertRaisesRegex(ValueError, "SD1-T-s4"):
            sd1.select("r1", 4, self.probe(), self.reference, self.full, self.rerun, self.split, names=NAMES)
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
    def known(self):
        """BT1's five r1 archive jobs and their records; the rebuild kept a, voided b by id, could not read c's
        file (an input void row) and wrote no row for d or e."""
        self.records = {g: str(self.root / "archive" / g / "game.jsonl") for g in "abcde"}
        self.jobs = {"jobs": [{"label": f"BT1-{g}", "lineage": "r1", "id": g, "index": i, "record": self.records[g],
                               "record_sha256": "0" * 64} for i, g in enumerate("abcde")]}
        self.archive = write_lines(self.root / "archive-r1.jsonl", [{"id": g} for g in "abcde"])
        base = {"schema": sd1.REBUILD_SCHEMA, "schedule_sha256": self.archive["sha256"], "key_mode": sd1.KEY_MODE}
        self.rebuild = [{**base, "id": "a", "file": self.records["a"], "void": None,
                         "roots": [root(1, [1, 0], [10, 2], [6, 0], 0), root(5, [0, 1], [10, 2], [6, 0], 0,
                                                                             view="tie_minimum")]},
                        {**base, "id": "b", "file": self.records["b"], "void": "step 3: a reused", "roots": []},
                        {"schema": sd1.REBUILD_SCHEMA, "file": self.records["c"], "id": None,
                         "void": "the listed file cannot be read"}]
        new_root = {"chooser": "new_root", "live": [1, 0], "n": [10, 2], "wins": [6, 0], "x": 0, "visits": 12}
        self.bt1_rows = [{"game": "a", "step": 1, **new_root}, {"game": "a", "step": 5, **new_root},
                         {"game": "a", "step": 7, "chooser": "reused"}, {"game": "b", "step": 2, **new_root},
                         {"game": "c", "step": 3, **new_root}, {"game": "d", "step": 4, **new_root}]

    def run_known(self, rebuild=None, rows=None, jobs=None):
        paths = {"r1": Path(write_lines(self.root / "rebuild.jsonl", rebuild or self.rebuild)["path"])}
        bt1 = {"r1": Path(write_lines(self.root / "ROWS-r1.jsonl", rows or self.bt1_rows)["path"])}
        with mock.patch.object(sd1, "MIN_KNOWN_ROOTS", 1):
            return sd1.known_answer(paths, bt1, {"r1": Path(self.archive["path"])}, jobs or self.jobs)

    def test_known_answer_compares_every_kept_games_root_with_bt1s_live_map(self):
        self.known()
        out = self.run_known()
        r1 = out["lineages"]["r1"]
        self.assertEqual((r1["bt1_new_root_rows"], r1["roots_compared"], r1["roots_not_replayed"], r1["roots_absent"]),
                         (5, 2, 2, 1))
        self.assertEqual(r1["roots_differing"], [["a", 5]])
        self.assertEqual(r1["void_games"], [{"id": "b", "reason": "step 3: a reused"}])
        self.assertEqual(r1["unreadable_games"], [{"id": "c", "file": self.records["c"],
                                                   "reason": "the listed file cannot be read"}])
        self.assertEqual((r1["games_not_replayed"], r1["games_absent"], r1["jobs_without_rows"]),
                         (["b", "c"], ["d"], ["d", "e"]))
        self.assertEqual((r1["failures"], r1["schedule"]), ([], common.pin(self.archive["path"])))
        self.assertFalse(out["pass"])
        # Without the differing root and the absent game, every kept root matches.
        rows = [r for r in self.bt1_rows if (r["game"], r["step"]) not in (("a", 5), ("d", 4))]
        self.assertTrue(self.run_known(rows=rows)["pass"])
        # A root of BT1's rows in a game the rebuild kept, which the rebuild did not rebuild, is missing.
        out = self.run_known(rows=rows + [{**rows[0], "step": 9}])
        self.assertEqual(out["lineages"]["r1"]["roots_missing"], [["a", 9]])
        self.assertFalse(out["pass"])

    def test_known_answer_binds_each_rebuild_row_to_bt1s_job_schedule_and_key_mode(self):
        self.known()
        rows = [r for r in self.bt1_rows if (r["game"], r["step"]) not in (("a", 5), ("d", 4))]
        changes = {
            "x: not one of BT1's r1 jobs": lambda rb: rb.append({**rb[0], "id": "x"}),
            "a: the rebuild read": lambda rb: rb[0].update(file=self.records["b"]),
            "a: not checked against the schedule": lambda rb: rb[0].update(schedule_sha256=None),
            "a: read in key mode 'repaired-v1', not repaired-v2": lambda rb: rb[0].update(key_mode="repaired-v1"),
            "an input void row names no BT1 r1 job's record": lambda rb: rb[2].update(file=str(self.root / "z.jsonl"))}
        for needle, change in changes.items():
            with self.subTest(needle):
                rebuild = copy.deepcopy(self.rebuild)
                change(rebuild)
                out = self.run_known(rebuild=rebuild, rows=rows)
                self.assertIn(needle, "\n".join(out["lineages"]["r1"]["failures"]))
                self.assertFalse(out["pass"])
        # A voided game needs no key mode; a spelling of its record path with backslashes is the same file.
        rebuild = copy.deepcopy(self.rebuild)
        del rebuild[1]["key_mode"]
        rebuild[0]["file"] = self.records["a"].replace("/", "\\")
        self.assertTrue(self.run_known(rebuild=rebuild, rows=rows)["pass"])
        with self.assertRaisesRegex(ValueError, "a: two jobs"):
            self.run_known(rows=rows, jobs={"jobs": self.jobs["jobs"] + self.jobs["jobs"][:1]})

    def known_custody(self, jobs: str, schedules: dict) -> str:
        """A custody record that verified the known answer's JOBS.json and each lineage's archive schedule."""
        files = [{"role": f"{lineage}-archive-schedule", **common.pin(path), "verified": True}
                 for lineage, path in schedules.items()]
        return write(self.root / "CUSTODY.json", {"schema": sd1.CUSTODY_SCHEMA, "pass": True, "forbidden": [HOT],
                                                  "known_answer": {"jobs": common.pin(jobs)}, "files": files,
                                                  "members": []})["path"]

    def test_the_known_answer_cli_pins_the_jobs_and_each_lineages_files(self):
        self.known()
        rows = [r for r in self.bt1_rows if (r["game"], r["step"]) not in (("a", 5), ("d", 4))]
        rebuild = write_lines(self.root / "rebuild.jsonl", self.rebuild)["path"]
        bt1 = write_lines(self.root / "ROWS-r1.jsonl", rows)["path"]
        jobs = write(self.root / "JOBS.json", self.jobs)["path"]
        archive_r2 = write_lines(self.root / "archive-r2.jsonl", [{"id": "z"}])["path"]
        held = self.known_custody(jobs, {"r1": self.archive["path"], "r2": archive_r2})
        argv = ["known-answer", "--rebuild", f"r1={rebuild}", "--rows", f"r1={bt1}", "--schedule",
                f"r1={self.archive['path']}", "--jobs", jobs, "--custody", held, "--out", str(self.root / "KNOWN.json")]
        with mock.patch("sys.stdout"), mock.patch.object(sd1, "MIN_KNOWN_ROOTS", 1):
            self.assertEqual(sd1.main(argv), 0)
        record = json.loads((self.root / "KNOWN.json").read_text(encoding="utf-8"))
        self.assertEqual((record["jobs"], record["custody"]), (common.pin(jobs), common.pin(held)))
        self.assertEqual(record["lineages"]["r1"]["rebuild"], common.pin(rebuild))
        with mock.patch("sys.stdout"):
            self.assertEqual(sd1.main(argv), 1, "1,000 roots per lineage")
        with self.assertRaisesRegex(ValueError, "give its rebuild output and schedule"):
            sd1.main([a for a in argv if not a.startswith("--schedule") and a != f"r1={self.archive['path']}"]
                     + ["--schedule", f"r2={archive_r2}"])
        with self.assertRaises(SystemExit), mock.patch("sys.stderr"):
            sd1.main(argv[:argv.index("--custody")] + argv[argv.index("--custody") + 2:])

    def test_known_answer_reads_only_custodys_copies_of_its_jobs_and_schedules(self):
        self.known()
        rebuild = write_lines(self.root / "rebuild.jsonl", self.rebuild)["path"]
        bt1 = write_lines(self.root / "ROWS-r1.jsonl", self.bt1_rows)["path"]
        jobs = write(self.root / "JOBS.json", self.jobs)["path"]
        held = self.known_custody(jobs, {"r1": self.archive["path"]})

        def run(jobs_path, schedule_path):
            sd1.main(["known-answer", "--rebuild", f"r1={rebuild}", "--rows", f"r1={bt1}", "--schedule",
                      f"r1={schedule_path}", "--jobs", jobs_path, "--custody", held, "--out",
                      str(self.root / "KNOWN.json")])
        # Another copy of JOBS.json, its bytes the same, is not custody's.
        elsewhere = write_bytes(self.root / "elsewhere" / "JOBS.json", Path(jobs).read_bytes())["path"]
        with self.assertRaises(ValueError) as caught:
            run(elsewhere, self.archive["path"])
        self.assertIn(f"{elsewhere} is not custody's JOBS.json", str(caught.exception))
        # Custody's archive schedule, changed since custody read it, is not custody's copy.
        Path(self.archive["path"]).write_text('{"id": "a"}\n', encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "not custody's .* for its r1-archive-schedule"):
            run(jobs, self.archive["path"])
        self.assertFalse((self.root / "KNOWN.json").exists())

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
            # A recorded receipt that is retained but differs from its pin fails the check; a missing one ends the
            # comparison.
            self.assertEqual((out["recorded_end"], out["recorded_mismatch"]), (None, False))
            out = sd1.o_check("r1", mine, same, Path("o.json"))
            self.assertEqual(out["recorded_end"], {"iteration": 40, "problem": "missing"})
            changed = self.completion("changed", [f"s{i}" for i in range(162)])
            Path(changed["iterations"][60]["path"]).write_text("{}", encoding="utf-8")
            out = sd1.o_check("r1", mine, changed, Path("o.json"))
            self.assertEqual((out["recorded_end"]["iteration"], out["recorded_mismatch"], out["compared_updates"]),
                             (60, True, 60))
            self.assertIn("not its pin's", out["recorded_end"]["problem"])
            self.assertIsNone(out["first_differing_update"])
            self.assertFalse(out["pass"])


# ------------------------------------------------------- stage 0: custody

# Synthetic roots: the utility screen's hot root on the maintainer's desktop
# (a Windows spelling) and computehost's runtime and run roots.
HOT = "Q:/utility-hot"
REMOTE = "/srv/computehost/runtime"
REMOTE_RUNS = "/srv/computehost/runs"


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def link_directory(link: Path, target: Path) -> None:
    """A directory link: a junction on Windows (which needs no privilege), a symbolic link elsewhere."""
    if sys.platform == "win32":
        import _winapi
        _winapi.CreateJunction(str(target), str(link))
    else:
        os.symlink(target, link, target_is_directory=True)


def as_pin(path: str, data: bytes) -> dict:
    return {"path": path, "sha256": digest(data)}


def write_bytes(path: Path, data: bytes) -> dict:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return {"path": str(path), "sha256": digest(data)}


# The utility screen's models: each lineage's parent and the fixed references, one checkpoint file each.
MODELS = {"r1": b'{"checkpoint":"r1"}', "r2": b'{"checkpoint":"r2"}', "t1": b'{"checkpoint":"T1"}',
          "a48": b'{"checkpoint":"A48"}'}


class Custody(Temp):
    """The utility screen's sealed evidence, synthetic. EVIDENCE.tar holds two input bundles (the desktop's,
    and computehost's staged on the desktop with computehost's own paths inside, so that the two differ only
    in paths), three formal search jobs with their records and job specs (r1's first on the desktop and its
    second on computehost, r2's on the desktop), the formal analysis index, ANALYSIS-SOURCES-001 and the
    allocation. Its source seal and custody completion sit beside it in two durable copies; the hot root keeps
    a third copy, which SD1 never reads. SCHEDULE.json with the frozen schedules, and BT1's known answer, are
    read in place."""

    CASES = {"r1": ["formal-r1-o0-v1-s0", "formal-r1-o2-v3-s1"], "r2": ["formal-r2-o4-v5-s0"]}
    REMOTE_CASES = ("formal-r1-o2-v3-s1",)

    def setUp(self):
        super().setUp()
        self.out_dir = self.root / "sd1" / "custody"
        # Paths the seal does not hold are rewritten into SD1's own root and kept there, never read.
        self.unsealed = common.absolute(self.out_dir / "unsealed")
        self.build()

    def seal(self, member: str, original: str, data: bytes):
        self.sealed[member] = (original, data)

    def seal_json(self, member: str, original: str, value: dict) -> bytes:
        data = json.dumps(self.edits.get(member, lambda v: v)(value)).encode()
        self.seal(member, original, data)
        return data

    def build(self, name="screen", remote_parameters=b"parameters", desk_environment=None, scheduled=None,
              analysis="index-pins-sources", analysis_allocation=None, edits=None, index_edit=None, drop=()):
        """The evidence under ROOT/NAME. ``remote_parameters`` are the bytes computehost's import descriptor
        pins; ``desk_environment`` adds to r1's desktop job spec's environment; ``scheduled`` replaces the
        frozen schedules' cases. ``analysis`` is how ANALYSIS-SOURCES-001 and the index bind: the index pins it
        as its source_spec ("index-pins-sources", as the utility screen's analysis_index.py writes it), it pins
        the index ("sources-pin-index"), or neither; ``analysis_allocation`` gives its allocation pin other
        bytes. ``edits`` maps a sealed import descriptor, source or bank spec to a function changing its JSON,
        ``index_edit`` changes the index's, and the members ``drop`` names are left out of the seal."""
        self.base = base = self.root / name
        self.sealed, self.edits = {}, edits or {}
        for host in ("desk", "remote"):
            local = f"{HOT}/inputs-001/{host}"
            runtime = local if host == "desk" else REMOTE
            parameters = b"parameters" if host == "desk" else remote_parameters
            par = digest(parameters)
            self.seal(f"inputs/{host}/objects/{par}.bin", f"{local}/objects/{par}.bin", parameters)
            imported = self.seal_json(f"inputs/{host}/metadata/import.json", f"{local}/metadata/import.json",
                                      {"schema": "import",
                                       "parameters": as_pin(f"{runtime}/objects/{par}.bin", parameters)})
            for model, data in MODELS.items():
                ck = digest(data)
                self.seal(f"inputs/{host}/objects/{ck}.json", f"{local}/objects/{ck}.json", data)
                source = {"checkpoint": as_pin(f"{runtime}/objects/{ck}.json", data),
                          "play_import": as_pin(f"{runtime}/metadata/import.json", imported),
                          "feature_transfer": {"expected_feature_contract_digest": "c"}}
                self.seal_json(f"inputs/{host}/sources/{model}-source.json", f"{local}/sources/{model}-source.json",
                               source)
            for lineage in common.LINEAGES:
                games = f'{{"games":"{lineage}"}}\n'.encode()
                self.seal(f"inputs/{host}/frozen/games-{lineage}.jsonl", f"{local}/frozen/games-{lineage}.jsonl",
                          games)
                bank_spec = {"lineage": lineage, "games": as_pin(f"{runtime}/frozen/games-{lineage}.jsonl", games)}
                self.seal_json(f"inputs/{host}/frozen/bank-spec-{lineage}.json",
                               f"{local}/frozen/bank-spec-{lineage}.json", bank_spec)
                self.seal(f"inputs/{host}/cases/{lineage}-cases.json", f"{local}/cases/{lineage}-cases.json",
                          f"cases {lineage}".encode())
            self.seal(f"inputs/{host}/bin/regret.exe", f"{local}/bin/regret.exe", b"binary")
        # An alias: another retained original path of a sealed member.
        self.aliases = [(f"{HOT}/staged/r1-source.json", "inputs/desk/sources/r1-source.json")]
        # SCHEDULE.json and the frozen schedules beside it, read in place.
        scheduled = scheduled or self.CASES
        files = {}
        for lineage, cases in scheduled.items():
            frozen = write_lines(base / "utility" / f"formal-{lineage}.jsonl",
                                 [{"id": case, "lineage": lineage} for case in cases])
            files[f"formal-{lineage}.jsonl"] = {"sha256": frozen["sha256"], "cases": len(cases)}
        schedule_pin = write(base / "utility" / "SCHEDULE.json", {"schema": "schedule", "files": files})
        entries = []
        for lineage, cases in self.CASES.items():
            for i, case in enumerate(cases):
                remote = case in self.REMOTE_CASES
                member = f"{'remote-runs' if remote else 'formal'}/{case}-search-v1"
                folder_original = f"{HOT}/{member}"
                folder = f"{REMOTE_RUNS}/{case}-search-v1" if remote else folder_original
                runtime = REMOTE if remote else f"{HOT}/inputs-001/desk"
                self.seal(f"{member}/game.jsonl", f"{folder_original}/game.jsonl",
                          json.dumps({"kind": "budget_game", "id": case, "lineage": lineage}).encode() + b"\n")
                command = [f"{runtime}/bin/regret.exe", f"source={runtime}/sources/{lineage}-source.json",
                           f"out={folder}/game.jsonl", "mode=s4a-budget", "workers=1", f"first_game={i}", "games=1"]
                environment = {"BANK_SPEC": f"{runtime}/frozen/bank-spec-{lineage}.json", "S4A_MODEL": lineage,
                               "OPPONENTS": f"T1={runtime}/sources/t1-source.json,"
                                            f"A48={runtime}/sources/a48-source.json",
                               "BUDGET_CASES": f"{runtime}/cases/{lineage}-cases.json", "BUDGET_TOTAL": "1",
                               "BUDGET_ARM": "search-v1", "BUDGET_KEY_MODE": "repaired"}
                cwd = folder
                if remote:
                    # computehost's scratch, which the seal does not hold.
                    environment["TRACE"] = f"{REMOTE_RUNS}/scratch/{case}"
                elif lineage == "r1":
                    # Windows spellings (backslashes, another case) name the same files; the scratch under the
                    # hot root is not sealed.
                    command[0] = command[0].replace("/", "\\")
                    command[2] = "out=" + f"{folder}/game.jsonl".replace("/", "\\")
                    environment["OPPONENTS"] = (f"T1={runtime}/sources/t1-source.json,"
                                                f"A48={runtime.upper()}/sources/a48-source.json")
                    environment["TRACE"] = f"{HOT}/formal/scratch/{case}"
                    environment.update(desk_environment or {})
                    cwd = folder.replace("/", "\\")
                self.seal(f"{member}/command.json", f"{folder_original}/command.json",
                          json.dumps({"command": command, "environment": environment, "cwd": cwd}).encode())
                entries.append({"id": f"{case}-search-v1", "row": as_pin(*self.sealed[f"{member}/game.jsonl"]),
                                "host": "computehost" if remote else "desktop", "classification": "formal"})
                entries.append({"id": f"{case}-plain-v1", "row": None, "host": "desktop", "classification": "formal"})
        allocation = b'{"allocation":1}'
        self.seal("metadata/allocation.json", f"{HOT}/allocation.json", allocation)
        allocation_pin = as_pin(f"{HOT}/allocation.json", allocation)
        own_allocation = allocation_pin if analysis_allocation is None else as_pin(f"{HOT}/allocation.json",
                                                                                     analysis_allocation)
        index = {"schema": sd1.INDEX_SCHEMA, "allocation": allocation_pin, "schedule": schedule_pin,
                 "entries": entries}

        def encoded(value):
            return json.dumps((index_edit or (lambda v: v))(value)).encode()
        if analysis == "sources-pin-index":
            self.index_bytes = encoded(index)
            self.analysis_bytes = json.dumps({"allocation": own_allocation,
                                              "index": as_pin(f"{HOT}/analysis/INDEX.json", self.index_bytes)}).encode()
        else:
            # The analysis input spec the index is built from: each active host's whole formal source.
            self.analysis_bytes = json.dumps({
                "schema": sd1.ANALYSIS_SOURCES_SCHEMA, "allocation": own_allocation,
                "sources": {"desktop": {"kind": "local", "root": f"{HOT}/formal"},
                            "computehost": {"kind": "collection",
                                            "receipt": as_pin(f"{HOT}/remote-runs/COLLECTION.json", b"c")}}}).encode()
            if analysis == "index-pins-sources":
                index["source_spec"] = as_pin(f"{HOT}/ANALYSIS-SOURCES-001.json", self.analysis_bytes)
            self.index_bytes = encoded(index)
        self.seal("metadata/INDEX.json", f"{HOT}/analysis/INDEX.json", self.index_bytes)
        self.seal("metadata/ANALYSIS-SOURCES-001.json", f"{HOT}/ANALYSIS-SOURCES-001.json", self.analysis_bytes)
        self.seal("metadata/stale.json", f"{HOT}/stale.json",
                  json.dumps({"allocation": {"path": f"{HOT}/allocation.json", "sha256": "0" * 64}}).encode())
        self.seal("metadata/notes.txt", f"{HOT}/notes.txt", b"notes")
        self.seal("metadata/broken.json", f"{HOT}/broken.json", b"{broken")
        for member in drop:
            del self.sealed[member]
        tar_path = base / "build" / "EVIDENCE.tar"
        tar_path.parent.mkdir(parents=True)
        with tarfile.open(tar_path, "w", format=tarfile.USTAR_FORMAT) as tar:
            for member, (_, data) in sorted(self.sealed.items()):
                info = tarfile.TarInfo(member)
                info.size, info.mtime = len(data), 0
                tar.addfile(info, io.BytesIO(data))
        archive = tar_path.read_bytes()
        self.targets = [base / "cold" / "evidence-002", base / "recovery" / "evidence-002"]
        self.seal_value = {
            "schema": sd1.SEAL_SCHEMA, "complete": True, "originals_retained": True,
            "files": [{"name": m, "bytes": len(d), "sha256": digest(d)} for m, (_, d) in sorted(self.sealed.items())],
            "original_archive_members": [{"original": as_pin(o, d), "member": m}
                                         for m, (o, d) in sorted(self.sealed.items())]
            + [{"original": as_pin(o, self.sealed[m][1]), "member": m} for o, m in self.aliases],
            "archives": [{"path": common.absolute(t / "EVIDENCE.tar"), "sha256": digest(archive)}
                         for t in self.targets],
            "completion_record": "CUSTODY-COMPLETION.json", "completion_rule": sd1.COMPLETION_RULE}
        for t in self.targets:
            write_bytes(t / "EVIDENCE.tar", archive)
        write_bytes(base / "hot-copy" / "EVIDENCE.tar", archive)
        # BT1's known answer, read in place: its jobs' archive records, the BUDGET inputs and the archive's
        # schedules under its budget root.
        budget = base / "bt1" / "budget"
        inputs = {rel: write(budget / rel, {"input": rel})["sha256"]
                  for rel in ("frozen/bank-spec-r1.json", "frozen/bank-spec-r2.json", "sources/a48-source.json",
                              "sources/r1-source.json", "sources/r2-source.json", "sources/t1-source.json")}
        archive_games = {"r1": ["archive-r1-g0", "archive-r1-g1"], "r2": ["archive-r2-g0"]}
        case_files, jobs = {}, []
        for lineage, ids in archive_games.items():
            case_files[lineage] = write_lines(budget / "schedule" / f"archive-{lineage}.jsonl",
                                              [{"id": gid} for gid in ids])["sha256"]
            for gid in ids:
                record_pin = write_lines(base / "bt1" / "archive" / gid / "game.jsonl", [{"id": gid}])
                jobs.append({"label": f"BT1-{gid}", "lineage": lineage, "id": gid, "index": len(jobs),
                             "record": common.absolute(record_pin["path"]), "record_sha256": record_pin["sha256"],
                             "archive_host": "desktop", "archive_cpu_seconds": 1.0})
        write(base / "bt1" / "JOBS.json", {"jobs": jobs})
        write(base / "bt1" / "CASE-PINS.json", {"bank_binding": "bank", "input_pins": inputs,
                                                 "case_files": case_files, "games_checked": len(jobs),
                                                 "fields": ["id"], "passed": True})
        self.parent = common.pin(write(base / "distillation" / "parent-r1.json", {"parent": 1})["path"])
        self.reseal(lambda seal: None)

    def reseal(self, change) -> dict:
        """Both durable copies' seals, changed by ``change``, and completion records pinning them; the spec."""
        seal = copy.deepcopy(self.seal_value)
        change(seal)
        for t in self.targets:
            write(t / "SOURCE-SEAL.json", seal)
        seals = [common.pin(t / "SOURCE-SEAL.json") for t in self.targets]
        for t in self.targets:
            write(t / "CUSTODY-COMPLETION.json", {"schema": sd1.COMPLETION_SCHEMA, "complete": True,
                                                  "archives": seal["archives"], "source_seals": seals})
        return self.spec()

    def retar(self, entries) -> dict:
        """Both durable archives rewritten with ``entries`` [(name, bytes, or None for a directory)] and both
        seals resealed to pin them; the spec."""
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w", format=tarfile.USTAR_FORMAT) as tar:
            for name, data in entries:
                info = tarfile.TarInfo(name)
                info.mtime = 0
                if data is None:
                    info.type = tarfile.DIRTYPE
                else:
                    info.size = len(data)
                tar.addfile(info, None if data is None else io.BytesIO(data))
        archive = buffer.getvalue()
        for t in self.targets:
            write_bytes(t / "EVIDENCE.tar", archive)

        def change(seal):
            for entry in seal["archives"]:
                entry["sha256"] = digest(archive)
        return self.reseal(change)

    def spec(self, **changes) -> dict:
        base = self.base
        value = {"schema": sd1.CUSTODY_SPEC_SCHEMA,
                 "durable": [{"seal": common.pin(t / "SOURCE-SEAL.json"), "archive": common.pin(t / "EVIDENCE.tar"),
                              "completion": common.pin(t / "CUSTODY-COMPLETION.json")} for t in self.targets],
                 "forbidden": [HOT, common.absolute(base / "hot-copy")],
                 "index": as_pin(f"{HOT}/analysis/INDEX.json", self.index_bytes),
                 "analysis_sources": as_pin(f"{HOT}/ANALYSIS-SOURCES-001.json", self.analysis_bytes),
                 "schedule": common.pin(base / "utility" / "SCHEDULE.json"),
                 "known_answer": {"jobs": common.pin(base / "bt1" / "JOBS.json"),
                                  "case_pins": common.pin(base / "bt1" / "CASE-PINS.json"),
                                  "budget_root": common.absolute(base / "bt1" / "budget")},
                 "rewrite": [{"from": REMOTE, "to": f"{HOT}/inputs-001/remote"},
                             {"from": REMOTE_RUNS, "to": f"{HOT}/remote-runs"},
                             {"from": f"{REMOTE_RUNS}/scratch", "to": f"{self.unsealed}/remote-scratch"},
                             {"from": f"{HOT}/formal/scratch", "to": f"{self.unsealed}/formal-scratch"}],
                 "keep": [self.unsealed],
                 "files": [{"role": "parent", **self.parent}],
                 "counts": {"r1-records": 2, "r1-jobs": 2, "r2-records": 1, "r2-jobs": 1, "parent": 1}}
        value.update(changes)
        return value

    def mine(self, out_dir: Path, *parts) -> str:
        return common.absolute(Path(out_dir).joinpath(*parts))

    def listed(self, out: dict, role: str) -> list:
        return Path(out["lists"][role]["path"]).read_text(encoding="utf-8").splitlines()

    def test_members_extract_through_the_seal_and_inner_paths_name_sd1s_copies(self):
        spec = self.spec()
        out = sd1.custody(spec, self.out_dir)
        self.assertEqual(out["failures"], [])
        self.assertTrue(out["pass"])
        self.assertEqual(out["extracted_from"], spec["durable"][0]["archive"])
        self.assertTrue(out["completion_rule"]["holds"])
        members = {m["member"]: m for m in out["members"]}
        for name in members:
            self.assertEqual((self.out_dir / "members" / name).read_bytes(), self.sealed[name][1])
        roles = {role: sum(m["role"] == role for m in out["members"]) for role in
                 ("index", "analysis-sources", "allocation", "r1-records", "r1-jobs", "r2-records", "r2-jobs")}
        self.assertEqual(roles, {"index": 1, "analysis-sources": 1, "allocation": 1, "r1-records": 2, "r1-jobs": 2,
                                 "r2-records": 1, "r2-jobs": 1})
        self.assertEqual(members["inputs/desk/sources/r1-source.json"]["originals"],
                         [f"{HOT}/inputs-001/desk/sources/r1-source.json", f"{HOT}/staged/r1-source.json"])
        self.assertEqual(out["index"]["search_records"], {"r1": 2, "r2": 1})
        self.assertEqual((out["analysis_sources"]["direction"], out["analysis_sources"]["binds"]),
                         ("index-pins-sources", True))
        self.assertEqual(out["known_answer"]["games"], {"r1": 2, "r2": 1})
        file_roles = {}
        for f in out["files"]:
            self.assertTrue(f["verified"], f)
            file_roles[f["role"]] = file_roles.get(f["role"], 0) + 1
        self.assertEqual(file_roles, {"schedule": 1, "r1-schedule": 1, "r2-schedule": 1, "known-answer": 2,
                                      "r1-archive": 2, "r2-archive": 1, "budget-inputs": 6,
                                      "r1-archive-schedule": 1, "r2-archive-schedule": 1, "parent": 1})

        def mine(*parts):
            return self.mine(self.out_dir, *parts)

        # The desktop's job spec: every path names SD1's copies, whatever its spelling.
        desk_case, remote_case = self.CASES["r1"]
        desk = json.loads((self.out_dir / "remapped" / "formal" / f"{desk_case}-search-v1" / "command.json")
                          .read_text(encoding="utf-8"))
        self.assertEqual(desk["command"], [mine("members", "inputs/desk/bin/regret.exe"),
                                           "source=" + mine("remapped", "inputs/desk/sources/r1-source.json"),
                                           "out=" + mine("members", f"formal/{desk_case}-search-v1/game.jsonl"),
                                           "mode=s4a-budget", "workers=1", "first_game=0", "games=1"])
        self.assertEqual(desk["environment"]["OPPONENTS"],
                         f"T1={mine('remapped', 'inputs/desk/sources/t1-source.json')},"
                         f"A48={mine('remapped', 'inputs/desk/sources/a48-source.json')}")
        self.assertEqual(desk["environment"]["BANK_SPEC"], mine("remapped", "inputs/desk/frozen/bank-spec-r1.json"))
        self.assertEqual(desk["environment"]["BUDGET_CASES"], mine("members", "inputs/desk/cases/r1-cases.json"))
        self.assertEqual(desk["cwd"], mine("members", f"formal/{desk_case}-search-v1"))
        # An unsealed path under the hot root is rewritten into SD1's own root and kept, never read.
        self.assertEqual(desk["environment"]["TRACE"], f"{self.unsealed}/formal-scratch/{desk_case}")
        self.assertEqual(members[f"formal/{desk_case}-search-v1/command.json"]["kept"],
                         [{"from": f"{HOT}/formal/scratch/{desk_case}",
                           "to": f"{self.unsealed}/formal-scratch/{desk_case}"}])
        # computehost's job spec, staged on the desktop: its runtime and run roots go through the rewrite rules,
        # and its scratch takes the longest rule, into SD1's own root.
        remote = json.loads((self.out_dir / "remapped" / "remote-runs" / f"{remote_case}-search-v1" / "command.json")
                            .read_text(encoding="utf-8"))
        self.assertEqual(remote["command"][:3], [mine("members", "inputs/remote/bin/regret.exe"),
                                                 "source=" + mine("remapped", "inputs/remote/sources/r1-source.json"),
                                                 "out=" + mine("members",
                                                               f"remote-runs/{remote_case}-search-v1/game.jsonl")])
        self.assertEqual(remote["cwd"], mine("members", f"remote-runs/{remote_case}-search-v1"))
        self.assertEqual(remote["environment"]["TRACE"], f"{self.unsealed}/remote-scratch/{remote_case}")
        self.assertEqual(members[f"remote-runs/{remote_case}-search-v1/command.json"]["kept"],
                         [{"from": f"{REMOTE_RUNS}/scratch/{remote_case}",
                           "to": f"{self.unsealed}/remote-scratch/{remote_case}"}])
        self.assertFalse(Path(self.unsealed).exists())
        # The source's pins name SD1's copies: the checkpoint's bytes as sealed, the descriptor remapped in turn
        # (so its SHA-256 is new).
        ck, par = digest(MODELS["r1"]), digest(b"parameters")
        source = json.loads(Path(mine("remapped", "inputs/remote/sources/r1-source.json")).read_text(encoding="utf-8"))
        self.assertEqual(source["checkpoint"], {"path": mine("members", f"inputs/remote/objects/{ck}.json"),
                                                "sha256": ck})
        descriptor = members["inputs/remote/metadata/import.json"]["remapped"]
        self.assertEqual(source["play_import"], descriptor)
        self.assertNotEqual(descriptor["sha256"], digest(self.sealed["inputs/remote/metadata/import.json"][1]))
        self.assertEqual(json.loads(Path(descriptor["path"]).read_text(encoding="utf-8"))["parameters"],
                         {"path": mine("members", f"inputs/remote/objects/{par}.bin"), "sha256": par})
        for m in out["members"]:
            if m.get("remapped"):
                text = Path(m["remapped"]["path"]).read_text(encoding="utf-8").casefold()
                self.assertNotIn("utility-hot", text, m["member"])
                self.assertNotIn("/srv/computehost", text, m["member"])
        # The two hosts' inputs are one input set, paths aside: their bytes differ only in the paths they spell.
        r1 = out["inputs"]["r1"]
        self.assertTrue(r1["one_input_set"])
        self.assertEqual({f: len(v) for f, v in r1["raw_sha256"].items()}, {f: 2 for f in sd1.INPUT_LISTS})
        self.assertEqual({f: len(v) for f, v in r1["identity_sha256"].items()}, {f: 1 for f in sd1.INPUT_LISTS})
        self.assertEqual(r1["inputs"]["source"], members["inputs/desk/sources/r1-source.json"]["remapped"])
        self.assertEqual(self.listed(out, "r1-source"), [mine("remapped", "inputs/desk/sources/r1-source.json")])
        self.assertEqual(self.listed(out, "r2-a48"), [mine("remapped", "inputs/desk/sources/a48-source.json")])
        self.assertEqual(self.listed(out, "r1-bank-spec"), [mine("remapped", "inputs/desk/frozen/bank-spec-r1.json")])
        self.assertEqual(self.listed(out, "r1-records"),
                         [mine("members", f"formal/{desk_case}-search-v1/game.jsonl"),
                          mine("members", f"remote-runs/{remote_case}-search-v1/game.jsonl")])
        self.assertEqual(self.listed(out, "r1-jobs"),
                         [mine("remapped", f"formal/{desk_case}-search-v1/command.json"),
                          mine("remapped", f"remote-runs/{remote_case}-search-v1/command.json")])
        self.assertEqual(self.listed(out, "parent"), [self.parent["path"]])
        # The recovery copy serves as well.
        swapped = self.spec(durable=spec["durable"][::-1])
        again = sd1.custody(swapped, self.root / "sd1" / "again")
        self.assertTrue(again["pass"], again["failures"])
        self.assertEqual(again["extracted_from"], swapped["durable"][0]["archive"])

    def test_records_reads_custodys_copies_of_each_lineages_records(self):
        out = sd1.custody(self.spec(), self.out_dir)
        held = write(self.root / "CUSTODY.json", out)["path"]
        for lineage, cases in self.CASES.items():
            listing = Path(out["lists"][f"{lineage}-records"]["path"])
            read = sd1.records(lineage, self.base / "utility" / f"formal-{lineage}.jsonl", listing, Path(held),
                               f"{lineage}-records")
            self.assertEqual([c["id"] for c in read["cases"]], cases)
            self.assertEqual([c["record"]["path"] for c in read["cases"]],
                             listing.read_text(encoding="utf-8").splitlines())
        # Another copy of the same bytes is not one of custody's copies.
        elsewhere = self.root / "elsewhere" / "game.jsonl"
        write_bytes(elsewhere, Path(self.listed(out, "r2-records")[0]).read_bytes())
        listing = self.root / "elsewhere-list.txt"
        listing.write_text(common.absolute(elsewhere) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "is not one of custody's r2-records copies"):
            sd1.records("r2", self.base / "utility" / "formal-r2.jsonl", listing, Path(held), "r2-records")

    def test_one_source_across_hosts_compares_what_each_descriptor_names_paths_aside(self):
        # computehost's import descriptor pins other parameters: its sources are other inputs.
        self.build("differs", remote_parameters=b"other parameters")
        out = sd1.custody(self.spec(), self.out_dir)
        r1 = out["inputs"]["r1"]
        self.assertFalse(r1["one_input_set"])
        self.assertIsNone(r1["inputs"])
        self.assertEqual({f: len(v) for f, v in r1["identity_sha256"].items()},
                         {"source": 2, "bank_spec": 1, "T1": 2, "A48": 2})
        failures = "\n".join(out["failures"])
        for field in ("source", "T1", "A48"):
            self.assertIn(f"r1: its jobs name 2 different {field} inputs, paths aside, not one", failures)
        self.assertNotIn("bank_spec inputs", failures)
        self.assertNotIn("r1-source", out["lists"])
        self.assertTrue(out["inputs"]["r2"]["one_input_set"])
        self.assertFalse(out["pass"])

    def test_a_kept_path_a_job_reads_is_a_failure(self):
        """A keep holds only outputs, logs and working directories. Any kept path a job reads is listed: among a
        job spec's inputs, and anywhere inside an input it names, through each remapped member in turn."""
        scratch, ck = f"{HOT}/formal/scratch", digest(MODELS["r2"])
        desk_job = f"formal/{self.CASES['r1'][0]}-search-v1/command.json"

        def parameters_kept(descriptor):
            return {**descriptor, "parameters": as_pin(f"{self.unsealed}/parameters.bin", b"p")}
        self.build("kept", desk_environment={"BUDGET_CASES": f"{scratch}/cases.json"}, edits={
            # r2's source pins its checkpoint under the hot root's scratch, which the rules keep in SD1's root.
            "inputs/desk/sources/r2-source.json":
                lambda source: {**source, "checkpoint": as_pin(f"{scratch}/{ck}.json", MODELS["r2"])},
            # Each host's import descriptor, which its sources name, pins its parameters under the keep prefix.
            "inputs/desk/metadata/import.json": parameters_kept,
            "inputs/remote/metadata/import.json": parameters_kept})
        out = sd1.custody(self.spec(), self.out_dir)
        reads = ", but a job reads it (a keep holds only outputs, logs and working directories)"
        self.assertEqual(sorted(out["failures"]), sorted([
            f"inputs/desk/sources/r2-source.json: {scratch}/{ck}.json is kept as "
            f"{self.unsealed}/formal-scratch/{ck}.json" + reads,
            f"inputs/desk/metadata/import.json: {self.unsealed}/parameters.bin is kept" + reads,
            f"inputs/remote/metadata/import.json: {self.unsealed}/parameters.bin is kept" + reads,
            f"{desk_job}: {scratch}/cases.json is kept as {self.unsealed}/formal-scratch/cases.json" + reads]))
        # The job's trace, an output, stays kept.
        self.assertEqual(json.loads((self.out_dir / "remapped" / desk_job).read_text(encoding="utf-8"))
                         ["environment"]["TRACE"], f"{self.unsealed}/formal-scratch/{self.CASES['r1'][0]}")
        self.assertFalse(out["pass"])

    def test_an_unsealed_path_is_kept_only_when_rewritten_into_sd1s_root(self):
        desk_case, remote_case = self.CASES["r1"]
        desk_job = f"formal/{desk_case}-search-v1/command.json"
        remote_job = f"remote-runs/{remote_case}-search-v1/command.json"
        # Without the keep prefix the rewritten paths resolve nowhere.
        failures = "\n".join(sd1.custody(self.spec(keep=[]), self.root / "a")["failures"])
        self.assertIn(f"{desk_job}: {HOT}/formal/scratch/{desk_case} resolves to no sealed member, verified file or "
                      "directory", failures)
        self.assertIn(f"{remote_job}: {REMOTE_RUNS}/scratch/{remote_case} resolves to no", failures)
        # Without the rules into SD1's root the hot root's scratch stays in the text, which the scan, run on the
        # final text, refuses.
        rules = [r for r in self.spec()["rewrite"] if "scratch" not in r["from"]]
        failures = "\n".join(sd1.custody(self.spec(rewrite=rules), self.root / "b")["failures"])
        self.assertIn(f"{desk_job}: {HOT}/formal/scratch/{desk_case} resolves to no", failures)
        self.assertIn(f"{desk_job}: its remapped copy still names {HOT}", failures)
        self.assertIn(f"{remote_job}: {REMOTE_RUNS}/scratch/{remote_case} resolves to no", failures)
        # A keep prefix under a forbidden root is refused before anything is read.
        with self.assertRaisesRegex(ValueError, f"keep: {HOT}/formal/scratch lies under the forbidden root {HOT}"):
            sd1.custody(self.spec(keep=[f"{HOT}/formal/scratch"]), self.root / "c")
        self.assertFalse((self.root / "c").exists())

    def test_the_longest_rewrite_prefix_applies_once(self):
        # The rules' order does not matter, and a rewritten path is not rewritten again: computehost's runtime
        # goes to its bundle on the desktop, not on to the last rule's target.
        rules = self.spec()["rewrite"][::-1] + [{"from": f"{HOT}/inputs-001/remote", "to": "/srv/elsewhere"}]
        out = sd1.custody(self.spec(rewrite=rules), self.out_dir)
        self.assertEqual(out["failures"], [])
        remote_case = self.CASES["r1"][1]
        remote = json.loads((self.out_dir / "remapped" / "remote-runs" / f"{remote_case}-search-v1" / "command.json")
                            .read_text(encoding="utf-8"))
        self.assertEqual(remote["environment"]["TRACE"], f"{self.unsealed}/remote-scratch/{remote_case}")
        # Without the rules computehost's runtime resolves nowhere.
        failures = "\n".join(sd1.custody(self.spec(rewrite=[]), self.root / "none")["failures"])
        self.assertIn(f"remote-runs/{remote_case}-search-v1/command.json: {REMOTE}/bin/regret.exe resolves to no "
                      "sealed member, verified file or directory", failures)

    def test_unc_and_verbatim_spellings_are_paths(self):
        desk_case = self.CASES["r1"][0]
        record = f"{HOT}/formal/{desk_case}-search-v1/game.jsonl"
        self.build("unc", desk_environment={"COPY": "\\\\?\\" + record.replace("/", "\\"),
                                            "SHARE": "\\\\fileserver\\share\\notes.json",
                                            "MIRROR": "//fileserver/share/notes.json"})
        out = sd1.custody(self.spec(), self.out_dir)
        job = f"formal/{desk_case}-search-v1/command.json"
        failures = "\n".join(out["failures"])
        self.assertIn(f"{job}: \\\\fileserver\\share\\notes.json resolves to no sealed member", failures)
        self.assertIn(f"{job}: //fileserver/share/notes.json resolves to no sealed member", failures)
        remapped = json.loads((self.out_dir / "remapped" / job).read_text(encoding="utf-8"))
        # A verbatim spelling of a sealed original names that member.
        self.assertEqual(remapped["environment"]["COPY"],
                         self.mine(self.out_dir, "members", f"formal/{desk_case}-search-v1/game.jsonl"))

    def test_every_gap_and_mismatch_is_listed_and_the_cli_exits_1(self):
        spec = self.spec()
        jobs = json.loads(Path(spec["known_answer"]["jobs"]["path"]).read_text(encoding="utf-8"))["jobs"]
        Path(jobs[0]["record"]).write_bytes(b"changed")
        missing_input = self.base / "bt1" / "budget" / "sources" / "t1-source.json"
        missing_input.unlink()
        spec["files"].append({"role": "notes", "path": common.absolute(self.base / "missing.json"), "sha256": "1" * 64})
        spec["members"] = [{"role": "stale", "member": "metadata/stale.json",
                            "sha256": digest(self.sealed["metadata/stale.json"][1]), "remap": True},
                           {"role": "notes", "member": "metadata/allocation.json", "sha256": "0" * 64}]
        spec["counts"] = {**spec["counts"], "r1-records": 3}
        spec_path = write(self.root / "spec.json", spec)["path"]
        with mock.patch("sys.stdout"):
            code = sd1.main(["custody", "--spec", spec_path, "--out-dir", str(self.out_dir),
                             "--out", str(self.root / "CUSTODY.json")])
        self.assertEqual(code, 1)
        record = json.loads((self.root / "CUSTODY.json").read_text(encoding="utf-8"))
        self.assertFalse(record["pass"])
        self.assertEqual(record["spec"], common.pin(spec_path))
        failures = "\n".join(record["failures"])
        for needle in (f"r1-archive file: {jobs[0]['record']} has SHA-256 {digest(b'changed')}",
                       f"budget-inputs file: {common.absolute(missing_input)} is missing",
                       f"notes file: {common.absolute(self.base / 'missing.json')} is missing",
                       f"metadata/stale.json: the pin of {HOT}/allocation.json has SHA-256 {'0' * 64}, but the file "
                       f"it names has {digest(self.sealed['metadata/allocation.json'][1])}",
                       f"members[1] (notes): the seal lists no member metadata/allocation.json with SHA-256 {'0' * 64}",
                       "role r1-records: custody holds 2 members and files, not 3"):
            self.assertIn(needle, failures)

    def test_the_index_is_the_schedules_and_names_sealed_records_and_job_specs(self):
        other = as_pin(f"{HOT}/analysis/INDEX.json", b"another index")
        out = sd1.custody(self.spec(index=other), self.root / "other")
        self.assertIn(f"the formal analysis index: the seal maps no member with SHA-256 {other['sha256']} to "
                      f"{HOT}/analysis/INDEX.json", out["failures"])
        self.build("short", scheduled={"r1": self.CASES["r1"], "r2": self.CASES["r2"] + ["formal-r2-o6-v7-s1"]})
        failures = "\n".join(sd1.custody(self.spec(), self.root / "a")["failures"])
        self.assertIn("r2: the index's 1 search-arm records are not exactly the frozen schedule's 2 cases", failures)
        case = f"{self.CASES['r1'][0]}-search-v1"
        given = [
            (dict(index_edit=lambda index: {**index, "schema": "another-index/v1"}),
             "the formal analysis index is not a search-player-utility analysis index"),
            (dict(index_edit=lambda index: {**index, "schedule": {**index["schedule"], "sha256": "0" * 64}}),
             "the index's frozen schedule is not the SCHEDULE.json given"),
            (dict(index_edit=lambda index: {**index, "entries": index["entries"] + [
                {"id": "formal-r9-o0-v0-s0-search-v1", "row": None}]}),
             "formal-r9-o0-v0-s0-search-v1: the index names no search-arm record of a lineage"),
            (dict(drop=[f"formal/{case}/command.json"]),
             f"{case}: its job spec {HOT}/formal/{case}/command.json is not sealed"),
        ]
        for i, (change, needle) in enumerate(given):
            with self.subTest(needle):
                self.build(f"index-{i}", **change)
                self.assertIn(needle, sd1.custody(self.spec(), self.root / "sd1" / f"index-{i}")["failures"])

    def test_analysis_sources_and_the_index_bind_either_way_with_one_allocation(self):
        # The utility screen's form: the index pins its input spec, ANALYSIS-SOURCES-001, as its source_spec.
        out = sd1.custody(self.spec(), self.root / "real")
        self.assertEqual(out["failures"], [])
        self.assertEqual(out["analysis_sources"], {
            "member": "metadata/ANALYSIS-SOURCES-001.json", "sha256": digest(self.analysis_bytes),
            "schema": sd1.ANALYSIS_SOURCES_SCHEMA, "index_sha256": digest(self.index_bytes),
            "allocation_sha256": digest(self.sealed["metadata/allocation.json"][1]),
            "direction": "index-pins-sources", "binds": True})
        # The other way: ANALYSIS-SOURCES-001 holds the index's SHA-256.
        self.build("other-way", analysis="sources-pin-index")
        out = sd1.custody(self.spec(), self.root / "sd1" / "other-way")
        self.assertEqual(out["failures"], [])
        self.assertEqual((out["analysis_sources"]["direction"], out["analysis_sources"]["binds"]),
                         ("sources-pin-index", True))
        # Either way its allocation must be the index's.
        allocation = digest(self.sealed["metadata/allocation.json"][1])
        for way in ("index-pins-sources", "sources-pin-index"):
            with self.subTest(way):
                self.build(f"allocation-{way}", analysis=way, analysis_allocation=b"another allocation")
                out = sd1.custody(self.spec(), self.root / "sd1" / f"allocation-{way}")
                self.assertEqual(out["failures"], [f"ANALYSIS-SOURCES-001's allocation is "
                                                   f"{digest(b'another allocation')}, not the index's allocation "
                                                   f"{allocation}"])
                self.assertEqual((out["analysis_sources"]["direction"], out["analysis_sources"]["binds"]),
                                 (way, False))
        # Neither way.
        self.build("neither", analysis="neither")
        out = sd1.custody(self.spec(), self.root / "sd1" / "neither")
        self.assertEqual(out["failures"], [f"ANALYSIS-SOURCES-001 ({digest(self.analysis_bytes)}) and the index "
                                           f"({digest(self.index_bytes)}) do not bind each other: the index's "
                                           "source_spec does not pin it, and it does not hold the index's SHA-256"])
        self.assertEqual((out["analysis_sources"]["direction"], out["analysis_sources"]["binds"]), (None, False))
        # An index that pins no allocation.
        self.build("no-allocation", index_edit=lambda index: {k: v for k, v in index.items() if k != "allocation"})
        out = sd1.custody(self.spec(), self.root / "sd1" / "no-allocation")
        self.assertIn("the index pins no allocation", out["failures"])
        self.assertFalse(out["analysis_sources"]["binds"])

    def test_a_known_answer_that_is_not_bt1s_checked_jobs_is_a_failure(self):
        bt1 = self.base / "bt1"
        jobs = json.loads((bt1 / "JOBS.json").read_text(encoding="utf-8"))
        pins = json.loads((bt1 / "CASE-PINS.json").read_text(encoding="utf-8"))

        def answer(i, jobs_value=jobs, pins_value=pins, schedule_r1=None):
            folder = self.root / f"answer-{i}"
            known = {"jobs": write(folder / "JOBS.json", jobs_value),
                     "budget_root": self.spec()["known_answer"]["budget_root"]}
            if schedule_r1 is not None:
                shutil.copytree(bt1 / "budget", folder / "budget")
                schedule = write_bytes(folder / "budget" / "schedule" / "archive-r1.jsonl", schedule_r1)
                pins_value = {**pins_value, "case_files": {**pins_value["case_files"], "r1": schedule["sha256"]}}
                known["budget_root"] = common.absolute(folder / "budget")
            known["case_pins"] = write(folder / "CASE-PINS.json", pins_value)
            return "\n".join(sd1.custody(self.spec(known_answer=known), folder / "custody")["failures"])
        given = [
            (dict(jobs_value={"jobs": "none"}), "JOBS.json lists no jobs"),
            (dict(jobs_value={"jobs": jobs["jobs"] + jobs["jobs"][:1]}),
             f"JOBS.json jobs[{len(jobs['jobs'])}]: not a job (lineage, id, record and record_sha256) or a repeated "
             "id"),
            (dict(pins_value={**pins, "passed": False}),
             "CASE-PINS.json is not a passed case check with input_pins and both lineages' case_files"),
            (dict(pins_value={**pins, "games_checked": 2}), "CASE-PINS.json checked 2 games, not JOBS.json's 3"),
            (dict(pins_value={**pins, "input_pins": {**pins["input_pins"], "../outside.json": "0" * 64}}),
             "CASE-PINS.json: '../outside.json' is not a path under the budget root"),
            (dict(schedule_r1=b"not a schedule\n"), "archive-r1.jsonl: not a schedule ("),
            (dict(schedule_r1=b'{"id": "archive-r1-g0"}\n'),
             "r1: JOBS.json names 1 games outside the archive's schedule, first archive-r1-g1"),
        ]
        for i, (change, needle) in enumerate(given):
            with self.subTest(needle):
                self.assertIn(needle, answer(i, **change))

    def test_a_job_input_read_in_place_is_its_own_sha256(self):
        # r1's desktop job names T1 as a plain file read in place, which computehost's does not.
        plain = write_bytes(self.root / "opponents" / "t1-source.json",
                            self.sealed["inputs/desk/sources/t1-source.json"][1])
        path = common.absolute(plain["path"])
        self.build("plain", desk_environment={
            "OPPONENTS": f"T1={path},A48={HOT}/inputs-001/desk/sources/a48-source.json"})
        out = sd1.custody(self.spec(files=[{"role": "parent", **self.parent},
                                           {"role": "opponents", "path": path, "sha256": plain["sha256"]}]),
                          self.out_dir)
        self.assertIn(plain["sha256"], out["inputs"]["r1"]["identity_sha256"]["T1"])
        self.assertIn(plain["sha256"], out["inputs"]["r1"]["raw_sha256"]["T1"])
        self.assertEqual(out["failures"], ["r1: its jobs name 2 different T1 inputs, paths aside, not one"])
        desk = json.loads((self.out_dir / "remapped" / "formal" / f"{self.CASES['r1'][0]}-search-v1" /
                           "command.json").read_text(encoding="utf-8"))
        self.assertEqual(desk["environment"]["OPPONENTS"],
                         f"T1={path},A48={self.mine(self.out_dir, 'remapped', 'inputs/desk/sources/a48-source.json')}")

    def test_an_incomplete_completion_or_two_different_seals_break_the_rule(self):
        first, second = self.spec()["durable"]
        done = json.loads((self.targets[0] / "CUSTODY-COMPLETION.json").read_text(encoding="utf-8"))
        incomplete = write(self.targets[0] / "CUSTODY-COMPLETION.json", {**done, "complete": False})
        out = sd1.custody(self.spec(durable=[{**first, "completion": incomplete}, second]), self.root / "incomplete")
        self.assertIn("durable copy 1: the custody completion is not complete", out["failures"])
        self.assertIn("the seal's completion rule: a completion record is not complete", out["failures"])
        first, second = self.reseal(lambda seal: None)["durable"]
        seal = json.loads((self.targets[1] / "SOURCE-SEAL.json").read_text(encoding="utf-8"))
        other = write(self.targets[1] / "SOURCE-SEAL.json", {**seal, "note": 1})
        out = sd1.custody(self.spec(durable=[first, {**second, "seal": other}]), self.root / "differ")
        self.assertIn("the seal's completion rule: the two SOURCE-SEAL copies differ", out["failures"])
        self.assertFalse(out["pass"])

    def test_only_a_durable_archive_under_the_seals_completion_rule_is_read(self):
        first, second = self.spec()["durable"]
        # The hot root's copy is not one of the seal's durable archives; members come from the copy that verifies.
        hot = common.pin(self.base / "hot-copy" / "EVIDENCE.tar")
        out = sd1.custody(self.spec(durable=[{**first, "archive": hot}, second]), self.root / "hot")
        self.assertIn(f"durable copy 1: {hot['path']} with this SHA-256 is not one of the seal's durable archives",
                      out["failures"])
        self.assertIn("the seal's completion rule: the seal's durable archives are not the two archives given",
                      out["failures"])
        self.assertEqual(out["extracted_from"], second["archive"])
        self.assertFalse(out["pass"])
        wrong = {"sha256": "0" * 64}
        given = [
            ([{**first, "seal": {**first["seal"], **wrong}}, second],
             f"durable copy 1: the source seal: {first['seal']['path']} has SHA-256 {first['seal']['sha256']}"),
            ([first, {**second, "completion": first["completion"]}],
             "durable copy 2: its seal, archive and completion record are not side by side"),
            ([{**first, "seal": {**first["seal"], **wrong}}, {**second, "seal": {**second["seal"], **wrong}}],
             "no member is extracted: the seal or the archive did not verify"),
        ]
        for i, (durable, needle) in enumerate(given):
            with self.subTest(needle):
                out = sd1.custody(self.spec(durable=durable), self.root / f"given-{i}")
                self.assertIn(needle, "\n".join(out["failures"]))
                self.assertFalse(out["pass"])
        self.assertEqual(out["members"], [])
        self.assertFalse((self.root / f"given-{len(given) - 1}" / "members").exists())
        # The two completion records differ.
        other = write(self.targets[1] / "CUSTODY-COMPLETION.json", {
            **json.loads((self.targets[1] / "CUSTODY-COMPLETION.json").read_text(encoding="utf-8")), "note": 1})
        out = sd1.custody(self.spec(durable=[first, {**second, "completion": other}]), self.root / "differ")
        self.assertIn("the seal's completion rule: the two completion records differ", out["failures"])
        # Seals that say otherwise, in both durable copies.
        position = next(i for i, f in enumerate(self.seal_value["files"]) if f["name"] == "metadata/INDEX.json")
        size = len(self.index_bytes)

        def longer(seal):
            seal["files"][position]["bytes"] += 1
        sealed = [
            (lambda seal: seal.update(completion_rule="Either copy will do."),
             "the seal's completion rule: the seal states another completion rule than the one custody checks"),
            (lambda seal: seal.update(completion_record="OTHER.json"),
             "durable copy 1: its completion record is not the seal's 'OTHER.json'"),
            (lambda seal: seal["files"].append(seal["files"][0]),
             f"the seal lists an unsafe, repeated or unhashed member {self.seal_value['files'][0]['name']!r}"),
            (lambda seal: seal.update(complete=False),
             "durable copy 1: the source seal is not a complete search-player-utility source seal"),
            (longer, f"metadata/INDEX.json: {size} bytes in the archive, not the seal's {size + 1}"),
        ]
        for i, (change, needle) in enumerate(sealed):
            with self.subTest(needle):
                out = sd1.custody(self.reseal(change), self.root / f"sealed-{i}")
                self.assertIn(needle, "\n".join(out["failures"]))
                self.assertFalse(out["pass"])

    def test_an_archive_member_unlike_the_seals_is_not_extracted(self):
        allocation = self.sealed["metadata/allocation.json"][1]
        other = b"x" * len(allocation)
        changed = {"metadata/allocation.json": other, "inputs/desk/bin/regret.exe": None}
        entries = [(m, changed.get(m, d)) for m, (_, d) in sorted(self.sealed.items())]
        entries.append(("inputs/desk/cases/r2-cases.json", self.sealed["inputs/desk/cases/r2-cases.json"][1]))
        out = sd1.custody(self.retar(entries), self.out_dir)
        for needle in (f"metadata/allocation.json: extracted with SHA-256 {digest(other)}, not {digest(allocation)}",
                       "inputs/desk/bin/regret.exe: not one regular file in the archive",
                       "inputs/desk/cases/r2-cases.json: not one regular file in the archive"):
            self.assertIn(needle, out["failures"])
        self.assertFalse((self.out_dir / "members" / "metadata" / "allocation.json").exists())
        self.assertFalse(out["pass"])

    def test_a_seal_that_maps_an_original_two_ways_is_not_read(self):
        stale = self.sealed["metadata/stale.json"][1]
        first = self.seal_value["original_archive_members"][0]
        given = [
            (lambda seal: seal["original_archive_members"][0]["original"].update(sha256="0" * 64),
             f"the seal maps {first['original']['path']!r} to {first['member']!r} with another SHA-256"),
            (lambda seal: seal["original_archive_members"].append(
                {"original": as_pin(f"{HOT}/allocation.json", stale), "member": "metadata/stale.json"}),
             f"the seal maps {HOT}/allocation.json to two members"),
        ]
        for i, (change, needle) in enumerate(given):
            with self.subTest(needle):
                out = sd1.custody(self.reseal(change), self.root / f"seal-{i}")
                self.assertIn(needle, out["failures"])
                self.assertIn("no member is extracted: the seal or the archive did not verify", out["failures"])

    def test_a_member_custody_cannot_remap_or_a_repeated_entry_is_a_failure(self):
        job = f"formal/{self.CASES['r1'][0]}-search-v1/command.json"
        # r1's desktop job names itself as its bank spec.
        self.build("cycle", desk_environment={"BANK_SPEC": f"{HOT}/{job}"})
        spec = self.spec(members=[
            {"role": "notes", "member": "metadata/notes.txt", "sha256": digest(b"notes"), "remap": True},
            {"role": "notes", "member": "metadata/broken.json", "sha256": digest(b"{broken"), "remap": True},
            {"role": "notes", "original": as_pin(f"{HOT}/notes.txt", b"notes")}],
            files=[{"role": "parent", **self.parent}, {"role": "parent", **self.parent}])
        failures = "\n".join(sd1.custody(spec, self.out_dir)["failures"])
        for needle in (f"{job}: its pins form a cycle",
                       "metadata/notes.txt: only JSON and JSONL members are remapped",
                       "metadata/broken.json: not readable JSON (",
                       "members[2] (notes): metadata/notes.txt is listed twice",
                       f"{self.parent['path']} is listed twice"):
            self.assertIn(needle, failures)

    def test_a_malformed_spec_or_a_used_directory_refuses_before_reading(self):
        known = self.spec()["known_answer"]
        cases = {
            r"members\[0\]": self.spec(members=[{"role": "notes", "member": "../outside", "sha256": "0" * 64}]),
            "unknown field 'extra'": {**self.spec(), "extra": 1},
            "missing field 'known_answer'": {k: v for k, v in self.spec().items() if k != "known_answer"},
            "durable: both durable copies": self.spec(durable=self.spec()["durable"][:1]),
            "forbidden: a nonempty list of absolute paths": self.spec(forbidden=[]),
            "known_answer: ": self.spec(known_answer={**known, "budget_root": "relative/root"}),
            "index: a pin": self.spec(index={"path": f"{HOT}/analysis/INDEX.json"}),
            "rewrite: a list of": self.spec(rewrite=[{"from": "relative/path", "to": HOT}]),
            "rewrite: two rules rewrite the same prefix": self.spec(rewrite=[{"from": REMOTE, "to": "/srv/a"},
                                                                             {"from": REMOTE + "/", "to": "/srv/b"}]),
            f"keep: {HOT}/x lies under the forbidden root {HOT}": self.spec(keep=[f"{HOT}/x"]),
            r"members\[0\]: .*a role custody does not derive": self.spec(members=[
                {"role": "r1-records", "member": "metadata/stale.json", "sha256": "0" * 64}]),
            r"files\[0\]: \{role, path, sha256\}, the path absolute": self.spec(files=[
                {"role": "parent", "path": "relative/parent.json", "sha256": "0" * 64}]),
            r"files\[0\]: .*a role custody does not derive": self.spec(files=[{"role": "schedule", **self.parent}]),
            "counts: ": self.spec(counts={"parent": -1}),
        }
        for needle, spec in cases.items():
            with self.subTest(needle):
                with self.assertRaisesRegex(ValueError, "malformed: .*" + needle):
                    sd1.custody(spec, self.root / "a")
        used = self.root / "used"
        write(used / "x.json", {})
        with self.assertRaisesRegex(ValueError, "not a fresh directory"):
            sd1.custody(self.spec(), used)
        with self.assertRaisesRegex(ValueError, "the output directory: .* lies under the hot or holding root"):
            sd1.custody(self.spec(forbidden=[HOT, common.absolute(self.root)], keep=[]), self.root / "new")
        # A keep prefix reached through a link into a forbidden root is under it.
        link_directory(self.root / "alias", self.base / "hot-copy")
        with self.assertRaisesRegex(ValueError, "malformed: keep: .* lies under the forbidden root"):
            sd1.custody(self.spec(keep=[common.absolute(self.root / "alias" / "kept")]), self.root / "a")
        self.assertFalse((self.root / "a").exists())
        self.assertFalse((self.root / "new").exists())

    def test_no_file_is_read_under_a_forbidden_root_or_through_a_link(self):
        spec = self.spec()
        # BT1's archive records reached through a directory link.
        link_directory(self.base / "linked", self.base / "bt1" / "archive")
        jobs = json.loads(Path(spec["known_answer"]["jobs"]["path"]).read_text(encoding="utf-8"))
        jobs["jobs"][0]["record"] = common.absolute(self.base / "linked" / jobs["jobs"][0]["id"] / "game.jsonl")
        spec["known_answer"] = {**spec["known_answer"], "jobs": write(self.base / "bt1" / "JOBS.json", jobs)}
        # The hot root's copy, by another spelling and through a link.
        link_directory(self.base / "alias-hot", self.base / "hot-copy")
        tar_sha = digest((self.base / "hot-copy" / "EVIDENCE.tar").read_bytes())
        spec["files"] += [{"role": "notes", "path": common.absolute(self.base / "bt1") + "/../hot-copy/EVIDENCE.tar",
                           "sha256": tar_sha},
                          {"role": "notes", "path": common.absolute(self.base / "alias-hot" / "EVIDENCE.tar"),
                           "sha256": tar_sha}]
        out = sd1.custody(spec, self.out_dir)
        failures = "\n".join(out["failures"])
        hot_copy = common.absolute(self.base / "hot-copy")
        self.assertIn(f"r1-archive file: {jobs['jobs'][0]['record']}: {self.base / 'linked'} is a symbolic link or "
                      "junction, which custody does not follow", failures)
        self.assertIn(f"notes file: {spec['files'][1]['path']} lies under the hot or holding root {hot_copy}", failures)
        self.assertIn(f"notes file: {spec['files'][2]['path']} lies under the hot or holding root {hot_copy}", failures)
        self.assertFalse(out["pass"])

    def test_roots_know_a_forbidden_directory_by_any_spelling_real_path_or_identity(self):
        hot = self.root / "hot"
        hot.mkdir()
        roots = sd1.Roots([common.absolute(hot), HOT])
        self.assertEqual(roots.containing(common.absolute(hot) + "/x/../y"), common.absolute(hot))
        self.assertEqual(roots.containing("q:\\UTILITY-HOT\\formal\\x"), HOT)
        self.assertEqual(roots.containing("//?/Q:/utility-hot/x"), HOT)
        self.assertEqual(roots.containing("Q:/utility-hot. /x"), HOT)
        self.assertIsNone(roots.containing(common.absolute(self.root / "hotter" / "x")))
        self.assertIsNone(roots.containing("Q:/utility-hotter/x"))
        link_directory(self.root / "alias", hot)
        self.assertEqual(roots.containing(str(self.root / "alias" / "x")), common.absolute(hot))
        # Another mount or short name of the directory has its file identity.
        other = self.root / "mount"
        other.mkdir()
        target, real_stat = os.path.realpath(other), os.stat
        hot_stat = real_stat(hot)

        def stat(path, *args, **kwargs):
            return hot_stat if os.fspath(path) == target else real_stat(path, *args, **kwargs)
        with mock.patch.object(sd1.os, "stat", stat):
            self.assertEqual(roots.containing(str(other / "x" / "y")), common.absolute(hot))
        self.assertIsNone(roots.containing(str(other / "x")))
        # A path is read only when it is absolute, outside every root, with no link on it.
        self.assertEqual(sd1.path_problem("relative/x.json", roots), "relative/x.json is not an absolute path")
        (hot.parent / "real" / "sub").mkdir(parents=True)
        link_directory(self.root / "link", self.root / "real")
        inside = str(self.root / "link" / "sub" / "f.json")
        self.assertEqual(sd1.path_problem(inside, roots),
                         f"{inside}: {self.root / 'link'} is a symbolic link or junction, which custody does not "
                         "follow")
        self.assertIsNone(sd1.path_problem(str(self.root / "real" / "sub" / "f.json"), roots))


# ------------------------------------------------ stage 0: parent identity


class Parents(Temp):
    """Each lineage's own copies of its parent, T1 and A48: the utility screen's input sources and the
    distillation screen's (its parent is O's recorded initial source). T1's and A48's checkpoints are one file
    each across lineages."""

    def setUp(self):
        super().setUp()
        self.recorded, self.entries = {}, {}
        for lineage in common.LINEAGES:
            for name in sd1.PARENT_SOURCES:
                key = lineage if name == "parent" else name
                checkpoint, parameters = f"checkpoint {key}".encode(), f"parameters {key}".encode()
                self.recorded[key] = digest(checkpoint)
                for side in ("utility", "screen"):
                    folder = self.root / side / lineage / name
                    source = self.source(folder, checkpoint, parameters)
                    if side == "screen" and name == "parent":
                        # The distillation screen's parent is O's recorded initial source.
                        config = write(folder / "O-config.json", {"initial_source": source, "iterations": []})
                        entry = {**config, "field": ["initial_source"]}
                    else:
                        entry = {"path": write(folder / "source.json", source)["path"]}
                    self.entries.setdefault(lineage, {}).setdefault(name, {})[side] = entry

    def source(self, folder: Path, checkpoint: bytes, parameters: bytes, transfer="c") -> dict:
        descriptor = write(folder / "import.json", {"schema": "import",
                                                    "parameters": write_bytes(folder / "par.bin", parameters)})
        return {"checkpoint": write_bytes(folder / "ck.json", checkpoint), "play_import": descriptor,
                "feature_transfer": {"expected_feature_contract_digest": transfer}}

    def spec(self) -> dict:
        return {"schema": sd1.PARENTS_SPEC_SCHEMA, **copy.deepcopy(self.entries)}

    def recorded_pins(self):
        return mock.patch.multiple(sd1, PARENT_CHECKPOINT_SHA256={lin: self.recorded[lin] for lin in common.LINEAGES},
                                   REFERENCE_CHECKPOINTS={name: self.recorded[name] for name in ("T1", "A48")})

    def parents(self, spec: dict, forbidden=(HOT,)) -> dict:
        with self.recorded_pins():
            return sd1.parents(spec, list(forbidden))

    def test_each_lineages_parent_and_references_are_their_recorded_checkpoints_and_sources(self):
        self.assertEqual(sd1.REFERENCE_CHECKPOINTS, {"T1": sd1.T1_CHECKPOINT_SHA256, "A48": sd1.A48_CHECKPOINT_SHA256})
        self.assertEqual(sd1.PARENT_SOURCES, ("parent", "T1", "A48"))
        out = self.parents(self.spec())
        self.assertEqual(out["failures"], [])
        self.assertTrue(out["pass"])
        self.assertEqual(out["forbidden"], [HOT])
        for lineage in common.LINEAGES:
            self.assertEqual(sorted(out["lineages"][lineage]), sorted(sd1.PARENT_SOURCES))
            for name, row in out["lineages"][lineage].items():
                self.assertTrue(row["sources_match"])
                recorded = self.recorded[lineage if name == "parent" else name]
                self.assertEqual(row["recorded_checkpoint_sha256"], recorded)
                self.assertEqual(row["utility"]["checkpoint"]["sha256"], row["recorded_checkpoint_sha256"])
        # Every file each side read is pinned: the source file, its checkpoint, its import descriptor and the
        # parameters that descriptor pins, each path spelled as the spec or the pin that named it (the fixture
        # writes native spellings, which differ from pin()'s on Windows).
        def as_written(path: Path) -> dict:
            return {"path": str(path), "sha256": digest(path.read_bytes())}
        folder = self.root / "screen" / "r2" / "A48"
        self.assertEqual(out["lineages"]["r2"]["A48"]["screen"]["read"],
                         [as_written(folder / name) for name in ("source.json", "ck.json", "import.json", "par.bin")])
        self.assertEqual(out["lineages"]["r1"]["parent"]["screen"]["read"][0],
                         as_written(self.root / "screen" / "r1" / "parent" / "O-config.json"))
        # T1 and A48 need only the utility screen's sources.
        spec = self.spec()
        del spec["r1"]["T1"]["screen"], spec["r2"]["A48"]["screen"]
        out = self.parents(spec)
        self.assertTrue(out["pass"])
        self.assertNotIn("sources_match", out["lineages"]["r1"]["T1"])
        self.assertNotIn("screen", out["lineages"]["r2"]["A48"])

    def test_a_different_checkpoint_or_source_fails_with_every_reason(self):
        spec = self.spec()
        # r1's parent: the utility screen's descriptor names other parameters.
        other = self.source(self.root / "other" / "r1", b"checkpoint r1", b"other parameters")
        spec["r1"]["parent"]["utility"] = {"path": write(self.root / "other" / "r1" / "source.json", other)["path"]}
        # r2's parent: the distillation screen's config changed after its pin.
        Path(spec["r2"]["parent"]["screen"]["path"]).write_text("{}", encoding="utf-8")
        # r1's T1: its checkpoint's bytes changed in place.
        Path(self.root / "utility" / "r1" / "T1" / "ck.json").write_bytes(b"changed")
        # r2's A48: a source whose checkpoint is T1's (another transfer besides).
        wrong = self.source(self.root / "other" / "A48", b"checkpoint T1", b"parameters A48", transfer="d")
        spec["r2"]["A48"]["utility"] = {"path": write(self.root / "other" / "A48" / "source.json", wrong)["path"]}
        custody = write(self.root / "CUSTODY.json", {"schema": sd1.CUSTODY_SCHEMA, "pass": True, "forbidden": [HOT]})
        out_path = self.root / "PARENTS.json"
        spec_pin = write(self.root / "spec.json", spec)
        with self.recorded_pins(), mock.patch("sys.stdout"):
            code = sd1.main(["parents", "--spec", spec_pin["path"], "--custody", custody["path"], "--out",
                             str(out_path)])
        self.assertEqual(code, 1)
        record = json.loads(out_path.read_text(encoding="utf-8"))
        self.assertEqual((record["spec"], record["custody"]), (common.pin(spec_pin["path"]),
                                                               common.pin(custody["path"])))
        text = "\n".join(record["failures"])
        self.assertIn("r1 parent: the utility screen's source differs from the distillation screen's in "
                      "['play_import']", text)
        self.assertIn("r2 parent: the distillation screen's source: ", text)
        self.assertIn("r1 T1: the utility screen's source checkpoint: ", text)
        self.assertIn(f"r2 A48: the utility screen's source: its checkpoint is {self.recorded['T1']}, not the "
                      f"recorded {self.recorded['A48']}", text)
        self.assertIn("r2 A48: the utility screen's source differs from the distillation screen's in "
                      "['checkpoint', 'feature_transfer']", text)
        self.assertEqual(len(record["failures"]), 5, text)

    def test_no_file_is_read_under_a_forbidden_root_or_through_a_link(self):
        held = common.absolute(self.root / "screen" / "r1")
        out = self.parents(self.spec(), forbidden=(HOT, held))
        self.assertIn(f"r1 parent: the distillation screen's source: {self.entries['r1']['parent']['screen']['path']} "
                      f"lies under the hot or holding root {held}, which SD1 never reads", out["failures"])
        self.assertEqual(sum(f.startswith("r1 ") for f in out["failures"]), 3)
        self.assertFalse(any(f.startswith("r2 ") for f in out["failures"]))
        # A copy reached through a directory link is refused.
        link = self.root / "linked"
        link_directory(link, self.root / "utility" / "r2")
        spec = self.spec()
        spec["r2"]["T1"]["utility"] = {"path": str(link / "T1" / "source.json")}
        out = self.parents(spec)
        self.assertIn(f"r2 T1: the utility screen's source: {link / 'T1' / 'source.json'}: {link} is a symbolic link "
                      "or junction, which custody does not follow", out["failures"])

    def test_the_spec_names_each_lineages_three_sources(self):
        spec = self.spec()
        del spec["r1"]["parent"]["screen"]
        with self.assertRaisesRegex(ValueError, "r1 parent: the utility screen's source and the distillation"):
            self.parents(spec)
        spec = self.spec()
        del spec["r2"]["A48"]
        with self.assertRaisesRegex(ValueError, "r2: name its parent, T1 and A48"):
            self.parents(spec)
        spec = self.spec()
        del spec["r2"]
        with self.assertRaisesRegex(ValueError, "naming r1 and r2"):
            self.parents(spec)
        spec = self.spec()
        spec["r1"]["T1"]["utility"]["extra"] = 1
        out = self.parents(spec)
        self.assertIn("r1 T1: the utility screen's source: give {path, optional sha256, optional field}",
                      out["failures"])
        spec = self.spec()
        missing = common.absolute(self.root / "nowhere" / "source.json")
        spec["r1"]["A48"]["utility"] = {"path": missing}
        spec["r2"]["parent"]["screen"]["field"] = ["other_source"]
        spec["r2"]["T1"]["utility"] = {"path": write(self.root / "not-a-source.json", {"checkpoint": 1})["path"]}
        out = self.parents(spec)
        self.assertIn(f"r1 A48: the utility screen's source: {missing} is missing", out["failures"])
        self.assertIn(f"r2 parent: the distillation screen's source: {spec['r2']['parent']['screen']['path']} has no "
                      "field ['other_source']", out["failures"])
        self.assertIn("r2 T1: the utility screen's source: not a model source with checkpoint and play_import pins",
                      out["failures"])
        self.assertEqual(len(out["failures"]), 3, out["failures"])
        with self.assertRaisesRegex(ValueError, "parents reads custody's forbidden roots"):
            self.parents(self.spec(), forbidden=())


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
                   "anchor": lambda p: p["anchor"]["held_out"].update(sha256="d" * 64),
                   "offers": lambda p: p["offers"].append({"path": "sd1/more-offers.json", "sha256": "e" * 64}),
                   # The probe must read the screen's bank: another bank fails the check.
                   "banks": lambda p: p["banks"][0]["bank"].update(sha256="f" * 64)}
        for name, change in changes.items():
            mine = self.probe("sd1")
            change(mine)
            out = sd1.reference_check("r1", mine, screen)
            self.assertEqual(out["differing"], [name])
            self.assertFalse(out["pass"])
            self.assertEqual(out["banks_equal"], name != "banks")
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
            selections.append(sd1.select("r1", k, probe, self.reference, full, rerun, self.split, names=NAMES))
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
    @classmethod
    def setUpClass(cls):
        """Every seed's selection and the sixteen configs under one root, built once for the class: D2-k and O-k
        (k from 1) from sd1.py phase2, and O-0, stage 0's O-s0, from phase2.py's reproduction of O. "qualify" is
        the same set at other worker counts under another root; "retention" is built with phase2.py's replicate
        seeds and ids fallen back to the retention trace's namespace (sd1.py's phase2 is the module it imports)."""
        super().setUpClass()
        shared = tempfile.TemporaryDirectory()
        cls.addClassCleanup(shared.cleanup)
        builder = PhaseTwoFixture("setUp")
        builder.setUp()
        builder.root = Path(shared.name)
        used = sd1.phase2
        real_id, real_seed = used.replicate_id, used.replicate_seed

        def retention_id(lineage, replicate, entry, namespace=used.RETENTION_NS):
            return real_id(lineage, replicate, entry, used.RETENTION_NS)

        def retention_seed(lineage, replicate, own, other, j, namespace=used.RETENTION_NS):
            return real_seed(lineage, replicate, own, other, j, used.RETENTION_NS)

        def build(workers: int, name: str) -> str:
            root = common.absolute(builder.root / name)
            selections = [builder.selection(k) for k in sd1.SEEDS]
            cfgs = sd1.phase2_configs("r1", builder.o_config, selections, workers, root)
            cfgs["O-0"] = phase2.config("r1", builder.o_config, None, workers + 2, f"{root}/r1-O-0/native")
            return json.dumps([selections, cfgs, root])
        try:
            cls.built = {"train": build(4, "train"), "qualify": build(8, "qualify")}
            with mock.patch.object(used, "replicate_id", retention_id), \
                    mock.patch.object(used, "replicate_seed", retention_seed):
                cls.built["retention"] = build(4, "train")
        finally:
            builder.tearDown()

    def configs(self, which: str = "train") -> tuple:
        """A fresh copy of a set built in setUpClass: (selections, configs, root)."""
        return tuple(json.loads(self.built[which]))

    def failed(self, out: dict) -> list:
        return sorted(k for k, v in out["checks"].items() if not v)

    def test_placement_aside_each_config_differs_from_o_0_only_as_designed(self):
        selections, cfgs, root = self.configs()
        out = sd1.configs_check("r1", self.o_config, selections, cfgs, root)
        self.assertTrue(out["pass"], out["problems"])
        # O-0; D1-k's source, D2-k and the directory of each of sixteen runs; O-k from 1; distinct directories.
        self.assertEqual(len(out["checks"]), 1 + 8 + 8 + 7 + 16 + 1)
        self.assertEqual((out["root"], out["qualification_compared"]), (root, False))
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
        selections, cfgs, root = self.configs()
        bad = copy.deepcopy(cfgs)
        bad["O-2"]["iterations"][5]["episodes"][0]["episode"]["starting_player"] ^= 1
        bad["D2-4"] = copy.deepcopy(cfgs["D2-5"])
        bad["O-0"] = copy.deepcopy(cfgs["O-1"])
        bad["O-6"]["update_backend"] = {"kind": "other"}
        bad["O-7"]["output_directory"] = bad["O-5"]["output_directory"]
        out = sd1.configs_check("r1", self.o_config, selections, bad, root)
        self.assertFalse(out["pass"])
        # D2-k is compared with O-k, so a changed O-k fails its pair too.
        self.assertEqual(self.failed(out),
                         ["D2-0 is O-0 from D1-0", "D2-2 is O-2 from D1-2",
                          "D2-4 is O-4 from D1-4", "D2-4 writes r1-D2-4/native under the root",
                          "D2-6 is O-6 from D1-6",
                          "O-0 is O's recorded config with the named parent, its units O's own",
                          "O-0 writes r1-O-0/native under the root",
                          "O-2 is O-0 on seed 2's unit seeds and episode ids",
                          "O-6 is O-0 on seed 6's unit seeds and episode ids",
                          "O-7 writes r1-O-7/native under the root",
                          "each run writes its own output directory"])
        self.assertIn("update 5 unit 0: its episode differs in more than its id and seed",
                      out["problems"]["O-2 is O-0 on seed 2's unit seeds and episode ids"])
        diff = out["differences_from_O-0"]
        self.assertEqual((diff["O-2"]["other_unit_fields"], diff["O-6"]["fields"]), (1, ["update_backend"]))

    def test_each_way_a_config_departs_from_the_design_is_named(self):
        selections, cfgs, root = self.configs()
        # O's recorded config gives the initial opponent to one unit fewer than j = 3 to 7's.
        units = pilot.DECKS * pilot.DECKS * 5 * 2
        o_config = copy.deepcopy(self.o_config)
        first = next(item for update in o_config["iterations"] for item in update["episodes"]
                     if item.get("opponent") == sd1.INITIAL_OPPONENT)
        first["opponent"] = {"kind": "fixed", "id": "other"}
        out = sd1.configs_check("r1", o_config, selections, cfgs, root)
        self.assertEqual(out["problems"]["O-0 is O's recorded config with the named parent, its units O's own"],
                         f"O's config gives the initial opponent to {units - 1} units, not the {units} of j = 3 to 7")
        # Each structural departure of O-k from O-0; D2-7 from another source; D1-1's source with another
        # feature transfer.
        bad = copy.deepcopy(cfgs)
        bad["O-2"]["note"] = 1
        bad["O-3"]["iterations"] = bad["O-3"]["iterations"][:-1]
        bad["O-4"]["iterations"][0]["episodes"] = bad["O-4"]["iterations"][0]["episodes"][:-1]
        bad["O-5"]["iterations"][1]["episodes"][0]["episode"] = None
        bad["O-6"]["iterations"][2]["episodes"][3]["opponent"] = {"kind": "fixed", "id": "other"}
        bad["D2-7"]["initial_source"] = copy.deepcopy(self.o_config["initial_source"])
        chosen = selections[1]["d1_checkpoint"]
        source = json.loads(Path(chosen["source"]["path"]).read_text(encoding="utf-8"))
        chosen["source"] = write(self.root / "d1-1-source.json",
                                 {**source, "feature_transfer": {"expected_feature_contract_digest": "other"}})
        out = sd1.configs_check("r1", self.o_config, selections, bad, root)
        replicate = {k: out["problems"][f"O-{k} is O-0 on seed {k}'s unit seeds and episode ids"] for k in range(2, 7)}
        self.assertEqual(replicate, {2: "it differs in ['note']", 3: "it does not hold the schedule's 162 updates",
                                     4: "update 0 does not hold the schedule's units",
                                     5: "update 1 unit 0 is not a unit with an episode",
                                     6: "update 2 unit 3 differs outside its episode"})
        self.assertEqual(out["problems"]["D2-7 is O-7 from D1-7"], "its initial source is not D1-k's source verbatim")
        self.assertEqual(out["problems"]["D1-1's source is the parent's but for its selected checkpoint"],
                         "it names another checkpoint or differs from the parent's import or feature transfer")
        self.assertEqual(out["differences_from_O-0"]["O-5"]["other_unit_fields"], 1)
        self.assertFalse(out["pass"])

    def test_each_run_writes_its_own_directory_under_the_root(self):
        selections, cfgs, root = self.configs()
        # Two runs swap directories: each fails its own, though no two runs share one.
        cfgs["D2-1"]["output_directory"], cfgs["O-1"]["output_directory"] = (
            cfgs["O-1"]["output_directory"], cfgs["D2-1"]["output_directory"])
        # O-0 is held to the same rule: stage 0's O-s0 runs in ROOT/LIN-O-0/native.
        cfgs["O-0"]["output_directory"] = f"{root}/r1-O-s0/native"
        # Another spelling of the same directory is the same directory.
        cfgs["D2-2"]["output_directory"] = f"{root}/scratch/../r1-D2-2/native/"
        out = sd1.configs_check("r1", self.o_config, selections, cfgs, root)
        self.assertEqual(self.failed(out), ["D2-1 writes r1-D2-1/native under the root",
                                            "O-0 writes r1-O-0/native under the root",
                                            "O-1 writes r1-O-1/native under the root"])
        self.assertTrue(out["checks"]["each run writes its own output directory"])
        self.assertEqual(out["problems"]["O-0 writes r1-O-0/native under the root"],
                         f"its output directory is '{root}/r1-O-s0/native', not {root}/r1-O-0/native")

    def test_the_gate_restates_the_designs_seeds_and_ids(self):
        # Configs whose replicate seeds and ids fell back to the retention trace's namespace pass phase2.py's own
        # checks but not the gate's, which derives them from the design's formulas.
        selections, cfgs, root = self.configs("retention")
        self.assertTrue(self.first_episode(cfgs["O-3"])["id"].startswith(f"{phase2.RETENTION_NS}-r1-rep3-"))
        out = sd1.configs_check("r1", self.o_config, selections, cfgs, root)
        self.assertEqual(self.failed(out), sorted(f"O-{k} is O-0 on seed {k}'s unit seeds and episode ids"
                                                  for k in range(1, 8)))
        self.assertIn("not seed 3's 'sd1-r1-rep3-",
                      out["problems"]["O-3 is O-0 on seed 3's unit seeds and episode ids"])

    def test_each_config_is_its_qualification_config_but_for_placement(self):
        selections, cfgs, root = self.configs()
        _, qualification, _ = self.configs("qualify")
        out = sd1.configs_check("r1", self.o_config, selections, cfgs, root, qualification)
        self.assertTrue(out["pass"], out["problems"])
        self.assertTrue(out["qualification_compared"])
        self.assertEqual(len(out["checks"]), 41 + 16)
        qualification["D2-3"]["learning_rate"] = 0.5
        out = sd1.configs_check("r1", self.o_config, selections, cfgs, root, qualification)
        self.assertEqual(self.failed(out), ["D2-3 is its qualification config, worker counts and output directory "
                                            "aside"])
        self.assertEqual(out["problems"][self.failed(out)[0]], "it differs in ['learning_rate']")
        with self.assertRaisesRegex(ValueError, "the qualification set names D2-0 to D2-7 and O-0 to O-7"):
            sd1.configs_check("r1", self.o_config, selections, cfgs, root,
                              {k: v for k, v in qualification.items() if k != "O-0"})

    def test_the_cli_reads_all_sixteen_configs_and_every_selection(self):
        selections, cfgs, root = self.configs()
        _, qualification, _ = self.configs("qualify")
        o_path = write(self.root / "O.json", self.o_config)["path"]
        select = [write(self.root / f"SELECT-{k}.json", s)["path"] for k, s in enumerate(selections)]
        configs = [f"{name}={write(self.root / 'cfg' / f'{name}.json', cfg)['path']}"
                   for name, cfg in sorted(cfgs.items())]
        qualify = [f"{name}={write(self.root / 'qual' / f'{name}.json', cfg)['path']}"
                   for name, cfg in sorted(qualification.items())]
        base = ["configs-check", "--lineage", "r1", "--o-config", o_path, "--select", *select, "--root", root,
                "--out", str(self.root / "CONFIGS.json")]
        with mock.patch("sys.stdout"):
            self.assertEqual(sd1.main(base + ["--config", *configs]), 0)
        record = json.loads((self.root / "CONFIGS.json").read_text(encoding="utf-8"))
        self.assertTrue(record["pass"])
        self.assertEqual(sorted(record["configs"]), sorted(sd1.PHASE2_RUNS))
        self.assertEqual(record["configs"]["O-0"], common.pin(self.root / "cfg" / "O-0.json"))
        self.assertIsNone(record["qualification"])
        with mock.patch("sys.stdout"):
            self.assertEqual(sd1.main(base + ["--config", *configs, "--qualification", *qualify]), 0)
        record = json.loads((self.root / "CONFIGS.json").read_text(encoding="utf-8"))
        self.assertEqual(record["qualification"]["D2-7"], common.pin(self.root / "qual" / "D2-7.json"))
        self.assertTrue(record["qualification_compared"])
        with self.assertRaisesRegex(ValueError, "--config: give NAME=PATH once for each of D2-0"):
            sd1.main(base + ["--config", *configs[1:]])
        with self.assertRaisesRegex(ValueError, "--config: give NAME=PATH once"):
            sd1.main(base + ["--config", *configs, configs[0]])
        with self.assertRaisesRegex(ValueError, "--qualification: give NAME=PATH once"):
            sd1.main(base + ["--config", *configs, "--qualification", *qualify[1:]])
        with self.assertRaisesRegex(ValueError, "every seed's selection"):
            sd1.main(base[:6] + select[:7] + base[-4:] + ["--config", *configs])
        with mock.patch("sys.stderr"), self.assertRaises(SystemExit):
            sd1.main(base[:-4] + base[-2:] + ["--config", *configs])


if __name__ == "__main__":
    unittest.main()
