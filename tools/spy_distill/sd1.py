"""Search-evidence distillation (SD1): split, records, root maps, bank, targets, census and D1 selection.

collab design SD1 (search-evidence distillation), sections 3 to 5. Offline:
nothing here plays, searches or trains. The teacher games are the search arm
of the search-player utility screen (`mode=s4a-budget`, one budget_game record
per scheduled case, nine decks, both seats). No label or outcome selects a
game, a row or a target.

Per lineage, in order (each step pins what it read):
  split       the held-out split, fixed on the frozen schedule before any
              replay: within each focal deck the ordered matchups are ordered
              by the SHA-256 of the UTF-8 string
              "sd1-evidence-holdout-v1|LIN|OWN|OTHER" (deck indexes) and the
              first two (a parameter) are held out with both seats; void games
              later leave their side, and the split does not change
  records     custody: exactly one search-arm budget_game record per scheduled
              case, each in a file of its own (as the utility screen wrote
              them), optionally against custody SHA-256s; any gap stops. The
              record list given is the one both replays read (BUDGET_ROWS of
              s4a-budget-replay and s4a-root-rebuild)
  games       stage 1: joins each case's record, the utility screen's pinned
              binary's s4a-budget-replay row (its starts) and the SD1 build's
              s4a-root-rebuild row (run with S4A_ROOT_SCHEDULE set to the frozen
              schedule: the per-game record checks scoped by chooser, the
              search-to-decision correspondence and the schedule match, then
              the replay checks; its maps and starts). A game is void when the
              rebuild voided it, the pinned binary's replay differs from the
              record in its end or line-tracker sequence (`matches_live`), or a
              focal start differs between the two builds, so every void is
              decided before the bank; then writes the games file without the
              void games and the bank command. More than one void game in
              twenty stops SD1 as engineering.
  bank-check  after the bank: the bank read this games file; a game the bank
              builder itself dropped (`kept` false) counts as void, and the
              one-in-twenty rule is read again over every void; the bank's
              new_root rows are exactly the rebuilt roots of its kept games,
              each executing its map's frozen choice
Both lineages:
  targets     stage 2: the mean rule at every root row (the executed edge
              leads; a rival is separated when its one-sided 95% Clopper-Pearson
              upper bound is below the leader's lower bound), two keep-set
              files per lineage in the existing schema (every non-separated
              live action; one for the training games' evidence rows, the only
              rows phase 1 trains, one for the held-out games' rows, read only
              by the probe), ROWS files and the census with the stop rule
              (fewer than 100 training or 20 held-out evidence rows in either
              lineage)
Per lineage and seed run:
  select      stage 3: the probe identity check (the parent's and O-final's
              readouts and the probe's inputs equal stage 2's reference probe's
              bit for bit), G3, then F and G2 (naming each failed part) at each
              checkpoint until the first gated one failing G2 (later ones are
              never selected, counted or reported), the fit reading and D1-k by
              the clustered-SE rule
Checks of the tools and builds:
  known-answer   the rebuild's maps over the Spy archive's games against BT1's
                 committed live maps (the new_root rows of BT1's ROWS in archive
                 games that pass the checks, at least 1,000 per lineage)
  bt1-regression the mean-rule adapter on BT1's committed root counts
                 reproduces every row's recorded mean-rule result
  o-check        O-s0 against the recorded O: its final state and, as far as
                 the retained records allow, the first update whose X1 fields
                 (after_state_sha256 first) differ

CLI (JSON summary on stdout; a failed check exits 1 after writing its record):
  split --lineage r1 --schedule formal-r1.jsonl [--held-out-matchups 2] --out SPLIT.json
  records --lineage r1 --schedule formal-r1.jsonl --record-list LIST --out RECORDS.json
          [--expect-sha256 PINS.json]
  games --records RECORDS.json --replay REPLAY.jsonl --rebuild REBUILD.jsonl --parent P.json
        --out-dir DIR --output-directory BANK_DIR
  bank-check --games-status GAMES.json --bank BANK.json --out CHECK.json
  targets --spec SPEC.json --out-dir DIR
  select --lineage r1 --seed K --probe PROBE.json --reference REF_PROBE.json --full PHASE1.json
         --rerun RERUN.json [--split SPLIT.json] --out SELECT.json
  known-answer --rebuild r1=REBUILD.jsonl [r2=...] --rows r1=ROWS-r1.jsonl [r2=...] --out KNOWN.json
  bt1-regression --rows r1=ROWS-r1.jsonl [r2=...] --out REGRESSION.json
  o-check --lineage r1 --run O-s0/completion.json --recorded O/completion.json
          --o-final O_FINAL_CHECKPOINT.json --out OCHECK.json
SPEC: {"r1": {"split": PIN, "games": PIN (GAMES-r1.json), "bank_check": PIN}, "r2": {...}}
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import sys
import types
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import bt1  # noqa: E402
import fit  # noqa: E402
import phase2  # noqa: E402
from distill_common import (LINEAGES, absolute, check_parent, episode, pilot, pin, read_json,  # noqa: E402
                            read_jsonl, require, write_json)

RESCORE = bt1.RESCORE
HOLDOUT_NS = "sd1-evidence-holdout-v1"
HELD_OUT_MATCHUPS = 2
SPLIT_SCHEMA = "sd1-v1/split"
RECORDS_SCHEMA = "sd1-v1/records"
GAMES_SCHEMA = "sd1-v1/games"
BANK_CHECK_SCHEMA = "sd1-v1/bank-check"
CENSUS_SCHEMA = "sd1-v1/census"
SELECT_SCHEMA = "sd1-v1/select"
KNOWN_SCHEMA = "sd1-v1/known-answer"
REGRESSION_SCHEMA = "sd1-v1/bt1-regression"
O_CHECK_SCHEMA = "sd1-v1/o-check"
REBUILD_SCHEMA = "sd1-root-rebuild-v1/game"
PROBE_SCHEMA = fit.PROBE_SCHEMA
PHASE1_SCHEMA = fit.PHASE1_SCHEMA
KEEP_SETS_SCHEMA = bt1.KEEP_SETS_SCHEMA
BANK_SCHEMA = bt1.BANK_SCHEMA
START_FIELDS = ("step", "physical_decision_id", "k", "obs_hash", "menu_hash", "state_hash")
DECISION_FIELDS = ("step", "physical_decision_id", "substep", "k", "chooser", "edge_backups",
                   "spy_cast_offered", "spy_cast_chosen", "spy_self_offered", "spy_self_chosen",
                   "dr_giant_offered", "dr_giant_chosen")
VIEWS = ("untied", "tie_minimum", "work_limit")
SPLITS = ("training", "held_out")
VOID_RATIO = 20
MIN_TRAINING_EVIDENCE = 100
MIN_HELD_OUT_EVIDENCE = 20
CENSUS_ALPHA = 0.2
MIN_KNOWN_ROOTS = 1000
# Phase 1 (design section 5): seeds 0 to 7 in cells SD1-T-s0 to SD1-T-s7.
SEEDS = tuple(range(8))
CELL_LABEL = re.compile(r"^SD1-T-s([0-7])$")
DESCRIPTIVE_STEPS = (8, 16)
GATED_STEPS = tuple(32 * 2 ** i for i in range(9))  # 32, 64, ..., 8,192
CHECKPOINT_STEPS = DESCRIPTIVE_STEPS + GATED_STEPS
FIT_SHARE = 0.5
GAP_SHARE = fit.GAP_SHARE
AGREEMENT_MARGIN = fit.AGREEMENT_MARGIN
REFERENCE_IDS = ("parent", "O-final")
# A probe's inputs and build, which every SD1 probe shares with stage 2's.
PROBE_INPUTS = ("lineage", "parent", "parent_state_sha256", "banks", "anchor", "offers", "evidence",
                "runtime_commit", "runtime_tree")
# RT1's X1 fields (retention.py); checkpoint, collection, episode and receipt
# hashes carry paths and are not compared.
X1_FIELDS = ("after_state_sha256", "adam_step", "physical_decisions", "policy_substeps", "loss", "policy_sum",
             "value_sum", "advantage_statistics")


def cell_label(seed: int) -> str:
    require(seed in SEEDS, f"seed {seed!r} is not 0 to 7")
    return f"SD1-T-s{seed}"


def sha256_file(path) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def pinned(entry: dict, what: str) -> Path:
    path = Path(entry["path"])
    require(path.is_file() and sha256_file(path) == entry["sha256"], f"{what}: {path} is missing or differs")
    return path


def read_pinned(entry: dict, what: str):
    return json.loads(pinned(entry, what).read_text(encoding="utf-8"))


def write_jsonl(path: Path, rows) -> dict:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as f:
        for r in rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    return pin(path)


def naive_sum(values) -> float:
    """Left-to-right binary64 summation (Python's sum() compensates since
    3.12; this is the order every mean here is defined by)."""
    total = 0.0
    for v in values:
        total += v
    return total


def naive_mean(values: list):
    return naive_sum(values) / len(values) if values else None


def finite(value) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def canonical(value) -> str:
    """JSON text that tells -0.0 from 0.0, for bit-for-bit comparisons."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def schedule_cases(path: Path, lineage: str) -> list:
    """The frozen schedule's cases of one lineage, each id once."""
    pilot.check_lineage(lineage)
    cases = list(read_jsonl(path))
    ids = [c["id"] for c in cases]
    require(cases and len(set(ids)) == len(ids), "the schedule repeats a case or is empty")
    require(all(c["id"].startswith(f"formal-{lineage}-") for c in cases), f"not the {lineage} schedule")
    return cases


def deck_names() -> list:
    return [d["label"] for d in pilot.load_decks()]


# ------------------------------------------------------------------ split


def holdout_key(lineage: str, own: int, other: int) -> str:
    return hashlib.sha256(f"{HOLDOUT_NS}|{lineage}|{own}|{other}".encode("utf-8")).hexdigest()


def split(lineage: str, cases: list, held_out_matchups: int = HELD_OUT_MATCHUPS) -> dict:
    """Section 3's held-out split, by ordered matchup: a matchup's two seats
    share one seed, so they stay on one side. Reads no outcome or label."""
    require(held_out_matchups >= 1, "at least one matchup is held out")
    matchups: dict = {}
    for c in cases:
        matchups.setdefault((c["own"], c["other"]), []).append(c)
    require(all(sorted(x["seat"] for x in v) == [0, 1] and len({x["seed"] for x in v}) == 1
                for v in matchups.values()), "each ordered matchup is two seats of one seed")
    games, decks = {}, {}
    for own in sorted({o for o, _ in matchups}):
        mine = sorted((holdout_key(lineage, own, other), other) for o, other in matchups if o == own)
        require(len(mine) > held_out_matchups, f"deck {own}: too few matchups to hold out {held_out_matchups}")
        held = {other for _, other in mine[:held_out_matchups]}
        decks[str(own)] = {"order": [{"other": other, "sha256": key} for key, other in mine],
                           "held_out": sorted(held)}
        for _, other in mine:
            for c in matchups[(own, other)]:
                games[c["id"]] = "held_out" if other in held else "training"
    return {"schema": SPLIT_SCHEMA, "namespace": HOLDOUT_NS, "lineage": lineage,
            "held_out_matchups": held_out_matchups, "decks": decks, "games": games,
            "matchups": {c["id"]: [c["own"], c["other"]] for c in cases},
            "counts": dict(Counter(games.values()))}


# ---------------------------------------------------------------- records


def budget_rows(path: Path) -> list:
    return [r for r in read_jsonl(path) if r.get("kind") == "budget_game"]


def records(lineage: str, schedule: Path, record_list: Path, expect: dict | None = None) -> dict:
    """Custody: one record file per scheduled case, bound by id, each holding
    exactly one budget_game row (and, when given, its custody SHA-256). The
    record's fields are checked against its case by the root rebuild, where a
    mismatch voids the game."""
    cases = schedule_cases(schedule, lineage)
    files = [line.strip() for line in Path(record_list).read_text(encoding="utf-8").splitlines() if line.strip()]
    require(len(set(files)) == len(files), "the record list repeats a file")
    by_case = {}
    for f in files:
        lines = list(read_jsonl(Path(f)))
        require(len(lines) == 1 and lines[0].get("kind") == "budget_game",
                f"{f}: a record file holds exactly one budget_game row")
        gid = lines[0].get("id")
        require(gid not in by_case, f"{gid}: two record files")
        by_case[gid] = f
    require(set(by_case) == {c["id"] for c in cases}, "the records are not exactly the schedule's cases")
    out = []
    for case in cases:
        f = by_case[case["id"]]
        entry = {"id": case["id"], "record": {"path": f, "sha256": sha256_file(f)}}
        if expect is not None:
            require(expect.get(case["id"]) == entry["record"]["sha256"],
                    f"{case['id']}: the record differs from its custody SHA-256")
        out.append(entry)
    return {"schema": RECORDS_SCHEMA, "lineage": lineage, "schedule": pin(schedule), "record_list": pin(record_list),
            "custody_checked": expect is not None, "cases": out}


# ------------------------------------------------------------------ games


def by_id(rows, what: str) -> dict:
    out = {}
    for r in rows:
        require(r.get("id") not in out, f"{what}: {r.get('id')} appears twice")
        out[r.get("id")] = r
    return out


def void_reason(record: dict, replay: dict | None, rebuild: dict | None) -> str | None:
    """Why a game is void, or None; the order of the checks is fixed."""
    if rebuild is None:
        return "no root-rebuild row"
    if rebuild["void"] is not None:
        return f"root rebuild: {rebuild['void']}"
    if replay is None:
        return "no pinned-binary replay row"
    rep = replay.get("replay") or {}
    if "error" in rep:
        return f"the pinned binary's replay failed: {rep['error']}"
    if replay.get("matches_live") is not True:
        return "the pinned binary's replay differs from the record in its end or line-tracker sequence"
    starts = rep.get("starts")
    if rep.get("actions") != len(record["game"]["actions"]) or not isinstance(starts, list):
        return "the pinned binary's replay is incomplete"
    if len(starts) != record["game"]["learner_starts"]:
        return "the pinned binary's start count differs from the record's"
    if [{f: s.get(f) for f in START_FIELDS} for s in starts] != rebuild["starts"]:
        return "a focal start differs between the pinned binary and the SD1 build"
    return None


def project(record: dict, starts: list, decks: list) -> dict:
    """One ArchiveGameV1 line: the record's actions, decisions and end, with the
    pinned binary's starts."""
    game = record["game"]
    return {"id": record["id"], "lineage": record["lineage"],
            "episode": episode(record["id"], record["seed"], record["own"], record["other"], record["seat"],
                               record["starting_player"], decks),
            "actions": list(game["actions"]), "starts": [{f: s[f] for f in START_FIELDS} for s in starts],
            "decisions": [{f: d[f] for f in DECISION_FIELDS} for d in game["decisions"]], "end": game["end"]}


def games(records_path: Path, replay: Path, rebuild: Path, parent: dict, out_dir: Path, output_directory: str,
          decks: list | None = None) -> dict:
    rec = read_json(records_path)
    require(rec.get("schema") == RECORDS_SCHEMA, "not a records file")
    lineage = rec["lineage"]
    check_parent(lineage, parent)
    decks = decks if decks is not None else pilot.load_decks()
    replays = by_id(read_jsonl(replay), "replay")
    rebuilt = by_id(read_jsonl(rebuild), "rebuild")
    cases = {c["id"] for c in rec["cases"]}
    require(set(replays) <= cases and set(rebuilt) <= cases, "a replay or rebuild row names no scheduled case")
    for row in rebuilt.values():
        require(row.get("schema") == REBUILD_SCHEMA, "not a root-rebuild row")
        require(row.get("schedule_sha256") == rec["schedule"]["sha256"],
                "the root rebuild did not check the records against the frozen schedule")
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    lines, kept, void, sources = [], [], [], []
    for case in rec["cases"]:
        gid, entry = case["id"], case["record"]
        record = budget_rows(pinned(entry, gid))[0]
        sources.append({"path": entry["path"], "sha256": entry["sha256"]})
        for row, what in ((replays.get(gid), "replay"), (rebuilt.get(gid), "rebuild")):
            require(row is None or row.get("file") == entry["path"], f"{gid}: the {what} row read another file")
        reason = void_reason(record, replays.get(gid), rebuilt.get(gid))
        if reason is not None:
            void.append({"id": gid, "reason": reason})
            continue
        kept.append(gid)
        lines.append(project(record, replays[gid]["replay"]["starts"], decks))
    status = {"schema": GAMES_SCHEMA, "lineage": lineage, "records": pin(records_path), "replay": pin(replay),
              "rebuild": pin(rebuild), "games": len(rec["cases"]), "kept": kept, "void": void,
              "stop": len(void) * VOID_RATIO > len(rec["cases"])}
    if not status["stop"]:
        games_pin = write_jsonl(out_dir / f"games-{lineage}.jsonl", lines)
        command = {"mode": "archive_distillation_bank", "parent": parent, "lineage": lineage, "games": games_pin,
                   "sources": sources + [pin(replay), pin(rebuild)], "output_directory": absolute(output_directory)}
        status["games_file"] = games_pin
        status["bank_command"] = write_json(out_dir / f"bank-{lineage}.command.json", command)
    status_pin = write_json(out_dir / f"GAMES-{lineage}.json", status)
    require(not status["stop"], f"engineering: {len(void)} of {len(rec['cases'])} {lineage} games are void")
    return {"status": status_pin, "kept": len(kept), "void": len(void)}


# ------------------------------------------------------------- bank check


def load_bank(path: Path, lineage: str) -> dict:
    bank = json.loads(Path(path).read_text(encoding="utf-8"))
    require(bank.get("schema") == BANK_SCHEMA and bank.get("lineage") == lineage, f"not a {lineage} bank")
    keys = [(r["game"], r["step"]) for r in bank["rows"]]
    require(len(set(keys)) == len(keys), "bank rows repeat a game and step")
    return bank


def bank_check(status_path: Path, bank_path: Path) -> dict:
    """The bank read this games file; the games it dropped itself are void;
    the one-in-twenty rule over every void; and its new_root rows are exactly
    the rebuilt roots of its kept games, each executing its map's frozen
    choice."""
    status = read_json(status_path)
    require(status.get("schema") == GAMES_SCHEMA and not status["stop"], "not a games status that passed")
    lineage = status["lineage"]
    bank = load_bank(bank_path, lineage)
    require(bank["games"]["sha256"] == status["games_file"]["sha256"], "the bank was built from another games file")
    require([g["id"] for g in bank["game_status"]] == status["kept"], "the bank's games are not the games file's")
    dropped = [{"id": g["id"], "reason": f"the bank builder dropped it: {g['reason']}"}
               for g in bank["game_status"] if not g["kept"]]
    kept = [g["id"] for g in bank["game_status"] if g["kept"]]
    rebuilt = by_id(read_jsonl(pinned(status["rebuild"], f"{lineage} rebuild")), "rebuild")
    roots = {(gid, x["step"]): x for gid in kept for x in rebuilt[gid]["roots"]}
    by_chooser: Counter = Counter()
    bank_roots = {}
    for r in bank["rows"]:
        by_chooser[r["chooser"]] += 1
        if r["chooser"] == "new_root":
            bank_roots[(r["game"], r["step"])] = r
    failures = []
    if set(bank_roots) != set(roots):
        failures.append(f"{len(set(bank_roots) ^ set(roots))} root rows differ between the bank and the rebuild")
    for key in sorted(set(bank_roots) & set(roots)):
        b, x = bank_roots[key], roots[key]
        if b["k"] != len(x["map"]) or b["target"] != x["map"][x["frozen_choice"]] or b["target"] != x["action"]:
            failures.append(f"{key[0]} step {key[1]}: the bank row's target is not the map's frozen choice")
    void = status["void"] + dropped
    stop = len(void) * VOID_RATIO > status["games"]
    return {"schema": BANK_CHECK_SCHEMA, "lineage": lineage, "games_status": pin(status_path),
            "bank": pin(bank_path), "bank_rows_sha256": bank["rows_sha256"], "games": status["games"],
            "kept": kept, "void": void, "dropped_by_bank": len(dropped), "stop": stop, "rows": len(bank["rows"]),
            "by_chooser": dict(by_chooser), "roots": len(bank_roots), "failures": failures,
            "pass": not failures and not stop}


# ------------------------------------------------------------ evidence rule


class RootCounts:
    """bt1.evidence()'s rescorer state for one root from a record's own counts:
    under the mean rule (m=None) it reads only n, wins and the skip mask, which
    is empty without interchange (tree.rs)."""

    def __init__(self, n: list, wins: list):
        self.n = [list(n)]
        self.w = [list(wins)]
        self.s = types.SimpleNamespace(skip=[[False] * len(n)])


def mean_rule(n: list, wins: list, x: int) -> dict:
    """The mean rule at a root: the executed edge x leads; bt1.evidence()."""
    return bt1.evidence(RootCounts(n, wins), 0, x, None)


def separated_at(n: list, wins: list, x: int, alpha: float) -> list:
    """Edges other than x separated at a one-sided level alpha (census only)."""
    low = RESCORE.cp_low(wins[x], n[x], alpha)
    return [a for a in range(len(n)) if a != x and RESCORE.cp_up(wins[a], n[a], alpha) < low]


# ---------------------------------------------------------------- targets


def lineage_rows(lineage: str, entry: dict) -> tuple:
    """One lineage's root rows: each bank new_root row joined to its rebuilt
    root, its split side and the mean rule; the kept games' rebuild rows; the
    split; and every void game (before the bank and dropped by it)."""
    check = read_pinned(entry["bank_check"], f"{lineage} bank check")
    require(check.get("schema") == BANK_CHECK_SCHEMA and check["lineage"] == lineage and check["pass"] is True,
            f"{lineage}: the bank check did not pass")
    require(check["games_status"]["sha256"] == entry["games"]["sha256"],
            f"{lineage}: the bank check read another games status")
    status = read_pinned(entry["games"], f"{lineage} games status")
    bank = load_bank(pinned(check["bank"], f"{lineage} bank"), lineage)
    require(bank["rows_sha256"] == check["bank_rows_sha256"], f"{lineage}: the bank differs from its check")
    the_split = read_pinned(entry["split"], f"{lineage} split")
    require(the_split.get("schema") == SPLIT_SCHEMA and the_split["lineage"] == lineage, f"{lineage}: not its split")
    every = by_id(read_jsonl(pinned(status["rebuild"], f"{lineage} rebuild")), "rebuild")
    rebuilt = {gid: every[gid] for gid in check["kept"]}
    require(set(rebuilt) <= set(the_split["games"]), f"{lineage}: a kept game is not in the split")
    roots = {(gid, x["step"]): (row, x) for gid, row in rebuilt.items() for x in row["roots"]}
    rows = []
    for b in bank["rows"]:
        if b["chooser"] != "new_root":
            continue
        row, x = roots.pop((b["game"], b["step"]))
        live = x["map"]
        require(b["k"] == len(live) and b["target"] == live[x["frozen_choice"]], f"{lineage} {b['game']} step "
                f"{b['step']}: the bank row is not its root's executed edge")
        rule = mean_rule(x["n"], x["wins"], x["frozen_choice"])
        rows.append({"game": b["game"], "step": b["step"], "split": the_split["games"][b["game"]], "own": row["own"],
                     "k": b["k"], "view": x["view"], "visits": x["visits"], "n": x["n"], "wins": x["wins"],
                     "x": x["frozen_choice"], "live": live, "target": b["target"], "mean": rule,
                     "first_choice": b["step"] == row["first_choice_step"],
                     "separated_80": separated_at(x["n"], x["wins"], x["frozen_choice"], CENSUS_ALPHA)})
    require(not roots, f"{lineage}: rebuilt roots without a bank row")
    return bank, rebuilt, rows, the_split, check["void"]


def keep(row: dict) -> list:
    """T's kept live actions at an evidence row: every non-separated edge."""
    separated = set(row["mean"]["separated"])
    return sorted(row["live"][e] for e in range(row["k"]) if e not in separated)


def target_measures(row: dict, bank_row: dict) -> dict:
    """The parent's binary64 softmax at an evidence row: its mass on the
    separated edges, whether its top action differs from the leader, and
    KL(q || p) = -log(kept mass)."""
    p = bt1.softmax(bank_row["parent_logits"])
    mass = sum(p[a] for a in keep(row))
    return {"parent_separated_mass": 1.0 - mass, "kl": -math.log(mass),
            "parent_top_differs": bt1.first_max(bank_row["parent_logits"]) != row["live"][row["x"]]}


def summarize(rows: list, measures: dict) -> dict:
    ev = [r for r in rows if r["mean"]["evidence"]]
    key = lambda r: (r["game"], r["step"])  # noqa: E731
    tried = sum(sum(1 for a in range(r["k"]) if a != r["x"] and r["n"][a] > 0) for r in ev)
    separated = sum(len(r["mean"]["separated"]) for r in ev)
    return {"rows": len(rows), "evidence_rows": len(ev), "separated_edges": separated,
            "separated_per_evidence_row": {str(k): v for k, v in
                                           sorted(Counter(len(r["mean"]["separated"]) for r in ev).items())},
            "mean_separated_per_evidence_row": separated / len(ev) if ev else None,
            "tried_rivals_at_evidence_rows": tried, "tried_rivals_separated_share": separated / tried if tried else None,
            "parent_mean_separated_mass": bt1.mean([measures[key(r)]["parent_separated_mass"] for r in ev]),
            "parent_top_differs_from_leader": sum(measures[key(r)]["parent_top_differs"] for r in ev),
            "summed_kl_q_p": sum(measures[key(r)]["kl"] for r in ev),
            "evidence_rows_80": sum(1 for r in rows if r["separated_80"]),
            # Descriptive: where the evidence rows sit in their games.
            "evidence_rows_at_step_0": sum(1 for r in ev if r["step"] == 0),
            "evidence_rows_after_step_0": sum(1 for r in ev if r["step"] != 0),
            "evidence_rows_at_first_choice": sum(1 for r in ev if r["first_choice"]),
            "evidence_rows_after_first_choice": sum(1 for r in ev if not r["first_choice"]),
            "views": {v: sum(r["view"] == v for r in rows) for v in sorted({*VIEWS, *(r["view"] for r in rows)})}}


def plain_kind(p: dict) -> tuple:
    return ("refreshed" if p["refreshed"] is not None else "not_refreshed",
            "remaining" if p["remaining_before"] else "exhausted")


def left_out(rebuilt: list) -> dict:
    """The decisions the evidence rule leaves out: reused-tree and plain choices
    (plain split by refreshed and by remaining allowance) and forced focal
    decisions."""
    plain = [plain_kind(p) for r in rebuilt for p in r["plain"]]
    return {"reused": sum(r["choosers"]["reused"] for r in rebuilt),
            "plain": len(plain),
            "plain_by_refreshed": dict(Counter(k[0] for k in plain)),
            "plain_by_remaining": dict(Counter(k[1] for k in plain)),
            "plain_by_refreshed_and_remaining": dict(Counter("/".join(k) for k in plain)),
            "one_action": sum(r["one_action"] for r in rebuilt)}


def keep_file(lineage: str, bank: dict, rows: list) -> dict:
    order = {(r["game"], r["step"]): i for i, r in enumerate(bank["rows"])}
    ev = sorted((r for r in rows if r["mean"]["evidence"]), key=lambda r: order[(r["game"], r["step"])])
    return {"schema": KEEP_SETS_SCHEMA, "lineage": lineage, "bank_rows_sha256": bank["rows_sha256"],
            "rows": [{"game": r["game"], "step": r["step"], "keep": keep(r)} for r in ev]}


def targets(spec: dict, out_dir: Path, names: list | None = None) -> dict:
    """Writes ROWS-LIN.jsonl, KEEP-training-LIN.json and KEEP-held_out-LIN.json
    per lineage and CENSUS.json with the stop rule."""
    out_dir = Path(out_dir).resolve()
    require(set(spec) == set(LINEAGES), "the spec names both lineages")
    names = names if names is not None else deck_names()
    result = {"schema": CENSUS_SCHEMA, "spec": spec, "lineages": {}}
    for lineage in LINEAGES:
        bank, rebuilt, rows, the_split, void = lineage_rows(lineage, spec[lineage])
        bank_rows = {(r["game"], r["step"]): r for r in bank["rows"]}
        measures = {(r["game"], r["step"]): target_measures(r, bank_rows[(r["game"], r["step"])])
                    for r in rows if r["mean"]["evidence"]}
        for r in rows:
            r.update(measures.get((r["game"], r["step"]), {}))
        voided = {v["id"] for v in void}
        entry = {"rows_file": write_jsonl(out_dir / f"ROWS-{lineage}.jsonl", rows), "keep_sets": {},
                 "games": {s: {"scheduled": sum(1 for side in the_split["games"].values() if side == s),
                               "kept": sum(1 for g in rebuilt if the_split["games"][g] == s),
                               "void": sum(1 for g in voided if the_split["games"][g] == s)} for s in SPLITS},
                 "void": void}
        for side in SPLITS:
            mine = [r for r in rows if r["split"] == side]
            games_here = [g for g in rebuilt.values() if the_split["games"][g["id"]] == side]
            entry[side] = {"all": {**summarize(mine, measures), "left_out": left_out(games_here)}, "by_deck": {}}
            for own in sorted({g["own"] for g in rebuilt.values()}):
                deck_rows = [r for r in mine if r["own"] == own]
                deck_games = [g for g in games_here if g["own"] == own]
                entry[side]["by_deck"][names[own]] = {**summarize(deck_rows, measures),
                                                      "left_out": left_out(deck_games)}
            value = keep_file(lineage, bank, mine)
            entry["keep_sets"][side] = (write_json(out_dir / f"KEEP-{side}-{lineage}.json", value)
                                        if value["rows"] else None)
        result["lineages"][lineage] = entry
    counts = {lin: {side: result["lineages"][lin][side]["all"]["evidence_rows"] for side in SPLITS}
              for lin in LINEAGES}
    result["stop_rule"] = {"evidence_rows": counts, "min_training": MIN_TRAINING_EVIDENCE,
                           "min_held_out": MIN_HELD_OUT_EVIDENCE,
                           "stop": any(c["training"] < MIN_TRAINING_EVIDENCE or c["held_out"] < MIN_HELD_OUT_EVIDENCE
                                       for c in counts.values())}
    census_pin = write_json(out_dir / "CENSUS.json", result)
    return {"census": census_pin, "stop": result["stop_rule"]["stop"], "evidence_rows": counts}


# ------------------------------------------------------------- selection


def readouts(probe: dict) -> dict:
    require(probe.get("schema") == PROBE_SCHEMA and probe.get("complete") is True, "not a complete probe")
    out = {c["id"]: c for c in probe["candidates"]}
    require(len(out) == len(probe["candidates"]), "duplicate probe candidate")
    return out


def probe_identity(probe: dict, reference: dict) -> dict:
    """The parent's and O-final's readouts, and the probe's inputs and build,
    equal stage 2's reference probe's bit for bit."""
    differ = [field for field in PROBE_INPUTS if canonical(probe.get(field)) != canonical(reference.get(field))]
    require(probe.get("evidence") is not None, "an SD1 probe reads the evidence keep-set files")
    mine, ref = readouts(probe), readouts(reference)
    for cid in REFERENCE_IDS:
        require(cid in mine and cid in ref, f"a probe lacks the {cid} candidate")
        if canonical(mine[cid]) != canonical(ref[cid]):
            differ.append(cid)
    return {"differing": differ, "pass": not differ}


def g2_parts(readout: dict, parent: dict, o_final: dict) -> dict:
    """G2 on held-out anchor rows over all nine decks, naming each failed part:
    the median clamped gap at least half the parent's; top-action agreement at
    least O-final's minus 0.02; the mean KL to the parent at most O-final's."""
    a = readout["anchor_held_out_decks"]["all"]
    p = parent["anchor_held_out_decks"]["all"]
    o = o_final["anchor_held_out_decks"]["all"]
    refs = (p["median_clamped_top_two_gap"], o["top_agreement"], o["mean_kl_from_parent"])
    require(all(finite(x) for x in refs), "the parent's or O-final's G2 references are not finite")
    parts = {"gap": {"value": a["median_clamped_top_two_gap"], "floor": GAP_SHARE * refs[0]},
             "agreement": {"value": a["top_agreement"], "floor": refs[1] - AGREEMENT_MARGIN},
             "kl": {"value": a["mean_kl_from_parent"], "ceiling": refs[2]}}
    for name, part in parts.items():
        v = part["value"]
        part["pass"] = finite(v) and (v <= part["ceiling"] if name == "kl" else v >= part["floor"])
    return {"parts": parts, "failed": [n for n, part in parts.items() if not part["pass"]],
            "non_spy": readout["anchor_held_out_decks"]["non_spy"],
            "pass": all(part["pass"] for part in parts.values())}


def held_out_rows(readout: dict) -> list:
    return readout["evidence"]["held_out"]["row_values"]


def clustered_se(rows_c: list, rows_m: list) -> tuple:
    """The cluster-robust SE of a checkpoint's held-out separated mass minus
    the minimizing checkpoint's, clustered by held-out ordered matchup:
    sqrt(G/(G-1) * sum_g (sum_{r in g} (d_r - dbar))^2) / N, where G counts the
    matchups with at least one evidence row. (None, G) when G < 2."""
    require(rows_c and [(r["game"], r["step"], tuple(r["matchup"])) for r in rows_c]
            == [(r["game"], r["step"], tuple(r["matchup"])) for r in rows_m],
            "checkpoints read different held-out rows")
    d = [c["separated_mass"] - m["separated_mass"] for c, m in zip(rows_c, rows_m)]
    n = len(d)
    dbar = naive_sum(d) / n
    clusters: dict = {}
    for r, value in zip(rows_c, d):
        key = tuple(r["matchup"])
        clusters[key] = clusters.get(key, 0.0) + (value - dbar)
    g = len(clusters)
    if g < 2:
        return None, g
    return math.sqrt(g / (g - 1) * naive_sum(s * s for s in clusters.values())) / n, g


def select_d1(passing: list) -> dict:
    """Section 5: among the read gated checkpoints passing F, G2 and G3, m is
    the lowest held-out separated mass (the earliest checkpoint on ties); D1
    is the earliest whose mass is at most m + SE (SE of its difference from
    the minimizing checkpoint); with G < 2, D1 is the minimizing checkpoint.
    Masses are binary64 means of the probe's per-row values."""
    masses = [(c["step"], naive_mean([r["separated_mass"] for r in c["rows"]])) for c in passing]
    m_step, m = min(masses, key=lambda sm: (sm[1], sm[0]))
    minimizing = next(c for c in passing if c["step"] == m_step)
    table, chosen = [], None
    for c, (step, mass) in zip(passing, masses):
        se, g = clustered_se(c["rows"], minimizing["rows"])
        qualifies = step == m_step if se is None else mass <= m + se
        table.append({"step": step, "held_out_separated_mass": mass, "se": se, "matchups": g,
                      "qualifies": qualifies})
        if qualifies and chosen is None:
            chosen = step
    return {"minimizing_step": m_step, "minimum": m, "candidates": table, "selected_step": chosen}


def select(lineage: str, seed: int, probe: dict, reference: dict, full: dict, rerun: dict,
           the_split: dict | None = None, names: list | None = None) -> dict:
    """One seed run's fit reading and D1 (design sections 5 and 7.1)."""
    label = cell_label(seed)
    require(full.get("schema") == PHASE1_SCHEMA and full.get("complete") is True
            and full.get("stop_after_steps") is None and full["identity"]["lineage"] == lineage,
            f"{label}: not a complete {lineage} full run")
    options = full["identity"]["recipe"].get("options") or {}
    require(options.get("cell") == label and options.get("keep_sets") is not None, f"not the {label} keep-set cell")
    require([c["step"] for c in full["checkpoints"]] == list(CHECKPOINT_STEPS),
            f"{label}: the run's checkpoints are not steps 8 to 8,192")
    require(probe.get("lineage") == lineage and reference.get("lineage") == lineage, "a probe of another lineage")
    require(canonical(probe["evidence"]["training"]) == canonical(options["keep_sets"]),
            f"{label}: the probe's training keep-sets are not the run's")
    out = {"schema": SELECT_SCHEMA, "lineage": lineage, "seed": seed, "label": label,
           "identity": probe_identity(probe, reference)}
    if not out["identity"]["pass"]:
        out["reading"] = "engineering: the parent's or O-final's probe values differ from stage 2's references"
        return out
    values = readouts(probe)
    parent, o_final = values["parent"], values["O-final"]
    parent_mass = parent["evidence"]["training"]["mean_separated_mass"]
    require(finite(parent_mass) and parent_mass > 0, "the parent's training separated mass is not positive")
    out["references"] = {cid: {"anchor_held_out_decks": {k: values[cid]["anchor_held_out_decks"][k]
                                                         for k in ("all", "non_spy")},
                               "training_separated_mass": values[cid]["evidence"]["training"]["mean_separated_mass"],
                               "held_out_separated_mass": naive_mean(
                                   [r["separated_mass"] for r in held_out_rows(values[cid])])}
                         for cid in REFERENCE_IDS}
    held_games = None
    if the_split is not None:
        require(the_split.get("schema") == SPLIT_SCHEMA and the_split.get("lineage") == lineage,
                "not this lineage's split")
        names = names if names is not None else deck_names()
        held_games = {g: tuple(names[i] for i in the_split["matchups"][g])
                      for g, side in the_split["games"].items() if side == "held_out"}
    telemetry = read_json(full["telemetry"]["path"])
    reproduced = fit.reproduced(full, rerun)
    entries, stop = [], None
    for c in full["checkpoints"]:
        cid = f"{label}-step-{c['step']}"
        readout = values.get(cid)
        require(readout is not None and readout["state_sha256"] == c["state_sha256"],
                f"{label}: the probe did not read checkpoint step {c['step']}")
        rows = held_out_rows(readout)
        if held_games is not None:
            require(all(held_games.get(r["game"]) == tuple(r["matchup"]) for r in rows),
                    f"{cid}: a held-out row's game or matchup is not the split's")
        g2 = g2_parts(readout, parent, o_final)
        evidence = readout["evidence"]
        mass = evidence["training"]["mean_separated_mass"]
        numbers = [mass, evidence["training"]["mean_kl_target"], evidence["held_out"]["mean_separated_mass"],
                   evidence["held_out"]["mean_kl_target"], *(p["value"] for p in g2["parts"].values()),
                   *(r["separated_mass"] for r in rows)]
        entry = {"step": c["step"], "gated": c["step"] in GATED_STEPS,
                 "finite": all(finite(x) for x in numbers) and fit.telemetry_finite(telemetry, c["step"]),
                 "F": {"training_separated_mass": mass, "ceiling": FIT_SHARE * parent_mass,
                       "pass": finite(mass) and mass <= FIT_SHARE * parent_mass},
                 "G2": g2}
        if not entry["finite"]:
            stop = ("G3", entry)
            break
        if entry["gated"] and not g2["pass"]:
            # The reading stops here: later checkpoints are never selected,
            # counted or reported.
            stop = ("G2", entry)
            break
        entry["training"] = {"separated_mass": mass, "mean_kl_target": evidence["training"]["mean_kl_target"]}
        entry["held_out"] = {"separated_mass": naive_mean([r["separated_mass"] for r in rows]), "rows": len(rows),
                             "mean_kl_target": evidence["held_out"]["mean_kl_target"]}
        entry["rows"] = rows
        entries.append(entry)
    g3_finite = stop is None or stop[0] != "G3"
    out["g3"] = {"rerun_reproduced": reproduced, "finite": g3_finite, "pass": reproduced and g3_finite}
    out["checkpoints"] = [{k: v for k, v in e.items() if k != "rows"} for e in entries]
    if not out["g3"]["pass"]:
        out["g3"]["cause"] = ("the first 32 steps did not reproduce" if not reproduced
                              else f"a non-finite value at step {stop[1]['step']}")
        out["reading"] = "engineering: G3 failed, so SD1 stops until the cause is fixed"
        return out
    if stop is not None:
        out["g2_stop"] = {"step": stop[1]["step"], "failed": stop[1]["G2"]["failed"], "G2": stop[1]["G2"],
                          "F": stop[1]["F"]}
    passing = [e for e in entries if e["gated"] and e["F"]["pass"]]
    if passing:
        d1 = select_d1(passing)
        out.update(reading="passes", d1=d1, selected_step=d1["selected_step"], fit_dose=passing[0]["step"],
                   highest_passing_step=passing[-1]["step"])
    elif stop is not None and stop[1]["F"]["pass"]:
        out["reading"] = "required fit reached only together with collapse"
    elif stop is not None:
        out["reading"] = "collapse before the required fit"
    else:
        out["reading"] = "required fit not reached"
    return out


# ----------------------------------------------------------- tool checks


def pairs(values: list, what: str) -> dict:
    out = {}
    for v in values:
        lineage, _, path = v.partition("=")
        require(lineage in LINEAGES and path and lineage not in out, f"{what}: give LIN=PATH once per lineage")
        out[lineage] = Path(path)
    return out


def known_answer(rebuilds: dict, rows: dict) -> dict:
    """The rebuild's maps over the Spy archive's games against BT1's committed
    live maps: every new_root row of BT1's ROWS in an archive game the SD1
    build kept must match, at least MIN_KNOWN_ROOTS per lineage; the games it
    voided are listed, not counted."""
    out = {"schema": KNOWN_SCHEMA, "lineages": {}}
    for lineage, path in rows.items():
        require(lineage in rebuilds, f"{lineage}: no rebuild output")
        rebuilt = by_id(read_jsonl(rebuilds[lineage]), f"{lineage} rebuild")
        roots = {(gid, x["step"]): x for gid, r in rebuilt.items() if r["void"] is None for x in r["roots"]}
        compared, differ, missing, not_replayed, total = 0, [], [], Counter(), 0
        for r in read_jsonl(path):
            if r["chooser"] != "new_root":
                continue
            total += 1
            game = rebuilt.get(r["game"])
            if game is None or game["void"] is not None:
                not_replayed[r["game"]] += 1
                continue
            x = roots.get((r["game"], r["step"]))
            if x is None:
                missing.append([r["game"], r["step"]])
                continue
            compared += 1
            same = (x["map"] == r["live"] and x["n"] == r["n"] and x["wins"] == r["wins"]
                    and x["frozen_choice"] == r["x"] and x["visits"] == r["visits"])
            if not same:
                differ.append([r["game"], r["step"]])
        voids = [{"id": gid, "reason": g["void"]} for gid, g in sorted(rebuilt.items()) if g["void"] is not None]
        out["lineages"][lineage] = {
            "rebuild": pin(rebuilds[lineage]), "rows": pin(path), "games": len(rebuilt), "void_games": voids,
            "bt1_new_root_rows": total, "roots_compared": compared, "roots_differing": differ,
            "roots_missing": missing, "roots_not_replayed": sum(not_replayed.values()),
            "games_not_replayed": sorted(not_replayed),
            "views": dict(Counter(x["view"] for x in roots.values())),
            "pass": compared >= MIN_KNOWN_ROOTS and not differ and not missing}
    out["pass"] = all(v["pass"] for v in out["lineages"].values())
    return out


def bt1_regression(rows: dict) -> dict:
    """The adapter over BT1's committed root counts reproduces each new_root
    row's recorded mean-rule result, with the leader the executed edge."""
    out = {"schema": REGRESSION_SCHEMA, "lineages": {}}
    for lineage, path in rows.items():
        roots = [r for r in read_jsonl(path) if r["chooser"] == "new_root"]
        got = [mean_rule(r["n"], r["wins"], r["x"]) for r in roots]
        out["lineages"][lineage] = {
            "rows": pin(path), "new_root_rows": len(roots),
            "reproduced": sum(g == r["rules"]["mean"] for g, r in zip(got, roots)),
            "leader_is_executed": sum(r["rules"]["mean"]["leader"] == r["x"] for r in roots),
            "evidence_rows": sum(g["evidence"] for g in got),
            "recorded_evidence_rows": sum(r["rules"]["mean"]["evidence"] for r in roots)}
        v = out["lineages"][lineage]
        v["pass"] = v["reproduced"] == v["leader_is_executed"] == v["new_root_rows"]
    out["pass"] = all(v["pass"] for v in out["lineages"].values())
    return out


def update_records(completion: dict) -> list:
    """A run's update records' X1 fields in order, as far as they are
    retained: up to the first receipt or update record missing or differing
    from its pin."""
    out = []
    for index, receipt_pin in enumerate(completion["iterations"]):
        try:
            receipt = read_pinned(receipt_pin, "receipt")
            update = read_pinned(receipt["update"], "update record")
        except (OSError, ValueError, KeyError):
            break
        require(receipt.get("iteration") == index, "iteration receipts out of order")
        out.append({f: update.get(f) for f in X1_FIELDS})
    return out


def o_check(lineage: str, run: dict, recorded: dict, o_final: Path) -> dict:
    """O-s0 against the recorded O: the final state and Adam step against O's
    final checkpoint and, as far as the recorded update records are retained,
    the first update whose X1 fields differ (after_state_sha256 first)."""
    end = phase2.check(lineage, run, o_final)
    mine, theirs = update_records(run), update_records(recorded)
    require(len(mine) == pilot.UPDATES, "O-s0's own update records are incomplete")
    first = None
    for index, (a, b) in enumerate(zip(mine, theirs)):
        if canonical(a) != canonical(b):
            first = {"update": index + 1, "fields": [f for f in X1_FIELDS if canonical(a[f]) != canonical(b[f])]}
            break
    return {"schema": O_CHECK_SCHEMA, "lineage": lineage, "endpoint": end, "recorded_updates_retained": len(theirs),
            "compared_updates": min(len(mine), len(theirs)), "first_differing_update": first,
            "pass": end["reproduced"] and first is None}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    s = sub.add_parser("split")
    s.add_argument("--lineage", required=True, choices=LINEAGES)
    s.add_argument("--schedule", type=Path, required=True)
    s.add_argument("--held-out-matchups", type=int, default=HELD_OUT_MATCHUPS)
    s.add_argument("--out", type=Path, required=True)
    r = sub.add_parser("records")
    r.add_argument("--lineage", required=True, choices=LINEAGES)
    r.add_argument("--schedule", type=Path, required=True)
    r.add_argument("--record-list", type=Path, required=True)
    r.add_argument("--expect-sha256", type=Path)
    r.add_argument("--out", type=Path, required=True)
    g = sub.add_parser("games")
    g.add_argument("--records", type=Path, required=True)
    g.add_argument("--replay", type=Path, required=True)
    g.add_argument("--rebuild", type=Path, required=True)
    g.add_argument("--parent", type=Path, required=True)
    g.add_argument("--out-dir", type=Path, required=True)
    g.add_argument("--output-directory", required=True)
    b = sub.add_parser("bank-check")
    b.add_argument("--games-status", type=Path, required=True)
    b.add_argument("--bank", type=Path, required=True)
    b.add_argument("--out", type=Path, required=True)
    t = sub.add_parser("targets")
    t.add_argument("--spec", type=Path, required=True)
    t.add_argument("--out-dir", type=Path, required=True)
    e = sub.add_parser("select")
    e.add_argument("--lineage", required=True, choices=LINEAGES)
    e.add_argument("--seed", type=int, required=True, choices=SEEDS)
    e.add_argument("--probe", type=Path, required=True)
    e.add_argument("--reference", type=Path, required=True)
    e.add_argument("--full", type=Path, required=True)
    e.add_argument("--rerun", type=Path, required=True)
    e.add_argument("--split", type=Path)
    e.add_argument("--out", type=Path, required=True)
    k = sub.add_parser("known-answer")
    k.add_argument("--rebuild", nargs="+", required=True)
    k.add_argument("--rows", nargs="+", required=True)
    k.add_argument("--out", type=Path, required=True)
    q = sub.add_parser("bt1-regression")
    q.add_argument("--rows", nargs="+", required=True)
    q.add_argument("--out", type=Path, required=True)
    o = sub.add_parser("o-check")
    o.add_argument("--lineage", required=True, choices=LINEAGES)
    o.add_argument("--run", type=Path, required=True)
    o.add_argument("--recorded", type=Path, required=True)
    o.add_argument("--o-final", type=Path, required=True)
    o.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    ok = True
    if a.op == "split":
        result = write_json(a.out, split(a.lineage, schedule_cases(a.schedule, a.lineage), a.held_out_matchups))
    elif a.op == "records":
        result = write_json(a.out, records(a.lineage, a.schedule, a.record_list,
                                           read_json(a.expect_sha256) if a.expect_sha256 else None))
    elif a.op == "games":
        result = games(a.records, a.replay, a.rebuild, read_json(a.parent), a.out_dir, a.output_directory)
    elif a.op == "bank-check":
        value = bank_check(a.games_status, a.bank)
        result, ok = write_json(a.out, value), value["pass"]
    elif a.op == "targets":
        result = targets(read_json(a.spec), a.out_dir)
    elif a.op == "select":
        value = select(a.lineage, a.seed, read_json(a.probe), read_json(a.reference), read_json(a.full),
                       read_json(a.rerun), read_json(a.split) if a.split else None)
        result, ok = write_json(a.out, value), not value["reading"].startswith("engineering")
    elif a.op == "known-answer":
        value = known_answer(pairs(a.rebuild, "--rebuild"), pairs(a.rows, "--rows"))
        result, ok = write_json(a.out, value), value["pass"]
    elif a.op == "bt1-regression":
        value = bt1_regression(pairs(a.rows, "--rows"))
        result, ok = write_json(a.out, value), value["pass"]
    else:
        value = o_check(a.lineage, read_json(a.run), read_json(a.recorded), a.o_final)
        result, ok = write_json(a.out, value), value["pass"]
    print(json.dumps(result))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
