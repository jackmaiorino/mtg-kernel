"""Search-evidence distillation (SD1): custody, identity checks, records, root maps, targets, selection and gates.

collab design SD1 (search-evidence distillation), sections 3 to 6 and 7.1. Offline:
nothing here plays, searches or trains. The teacher games are the search arm
of the search-player utility screen (`mode=s4a-budget`, one budget_game record
per scheduled case, nine decks, both seats). No label or outcome selects a
game, a row or a target.

Stage 0, before any replay (each writes its record and exits 1 on a failure):
  custody     every input SD1 reads, each against its SHA-256, nothing read
              from under a hot or holding root (by spelling, real path or file
              identity) or through a link: both durable copies of the utility
              screen's seal, archive and completion record under the seal's
              completion rule; the members of its EVIDENCE.tar extracted
              through the seal's member map into SD1's own directory, derived
              from its pinned formal index (the search arm's records, each with
              its job spec, and the inputs those name), the absolute paths
              inside the job specs and their inputs resolved to the extracted
              copies; within a lineage one source, bank spec and pair of
              opponents, paths aside; ANALYSIS-SOURCES-001 and the index
              bound to each other, with one allocation; the known answer's
              archive records and inputs from
              BT1's JOBS.json and CASE-PINS.json; and the plain files read in
              place. Every gap and mismatch is listed
  parents     section 3's parent identity, per lineage: its parent's, T1's and
              A48's checkpoints (each lineage's own copies) are their recorded
              pins, and each source in the utility screen's inputs is the
              distillation screen's, paths aside; every file read is pinned,
              none under custody's forbidden roots or through a link
  reference-check  phase-1 exactness's probe references: the stage-0 probe's
              banks, parent and O-final values equal the screen's recorded
              probe's
Per lineage, in order (each step pins what it read):
  split       the held-out split, fixed on the frozen schedule before any
              replay: within each focal deck the ordered matchups are ordered
              by the SHA-256 of the UTF-8 string
              "sd1-evidence-holdout-v1|LIN|OWN|OTHER" (deck indexes) and the
              first two (a parameter) are held out with both seats; void games
              later leave their side, and the split does not change
  records     custody: exactly one search-arm budget_game record per scheduled
              case, each in a file of its own (as the utility screen wrote
              them), each file one of custody's extracted copies of the role's
              members and its SHA-256 that member's; any gap stops. The record
              list given is the one both replays read (BUDGET_ROWS of
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
Per lineage:
  commands    stage 3: each seed's dry run, full run and 32-step rerun (cell
              SD1-T-sK, K = 0 to 7, on the training keep-set file, steps 8 to
              8,192), on the census's games file and bank, once its stop rule is
              open and phase-1 exactness passed in both lineages
  probe       stage 2's reference probe (the parent and O-final) or one seed
              run's probe (its eleven checkpoints besides, 13 candidates), with
              both keep-set files as the probe's evidence inputs
Per lineage and seed run:
  select      stage 3: the probe identity check (the parent's and O-final's
              readouts and the probe's inputs equal stage 2's reference probe's
              bit for bit, file paths aside), G3, then F and G2 (naming each
              failed part) at each checkpoint until the first gated one failing
              G2 (later ones are never selected, counted or reported), the fit
              reading and D1-k by the clustered-SE rule, with D1-k's checkpoint
Per lineage, once every seed is selected:
  fit         section 7.1's fit table: per seed the G3 result, F's first pass,
              G2's first failure and its failing parts, the reading row and D1
              with s, f and h; the lineage's summary in the fixed form; the
              held-out separated mass and KL at every read checkpoint only, each
              value read again from the seed's probe. Reads no panel
Per lineage, once all eight seeds have a D1:
  phase2      stage 4: D2-k from D1-k and O-k (k >= 1) from the parent, each on
              schedule seed k under the sd1 namespace (phase2.py); O-0 is
              stage 0's O-s0
  configs-check  the phase-2 config gate, as RT1's, by the design's properties
              unit by unit: placement aside, O-0 (stage 0's O-s0) is O's
              recorded config with the named parent, O-k differs from it only
              in replicate k's seeds and episode ids, and D2-k is O-k from D1-k;
              each run's output directory, O-0's included, is
              ROOT/LIN-NAME/native; production configs equal their
              qualification configs, worker counts and output directories aside
Checks of the tools and builds:
  known-answer   the rebuild's maps over the Spy archive's games against BT1's
                 committed live maps (the new_root rows of BT1's ROWS in archive
                 games that pass the checks, at least 1,000 per lineage), each
                 rebuild row checked against the archive's schedule and key mode;
                 a game BT1's ROWS name that the rebuild lacks fails it. Its
                 JOBS.json and each archive schedule are custody's copies
  bt1-regression the mean-rule adapter on BT1's committed root counts
                 reproduces every row's recorded mean-rule result
  o-check        O-s0 against the recorded O: its final state and, as far as
                 the retained records allow, the first update whose X1 fields
                 (after_state_sha256 first) differ

CLI (JSON summary on stdout; a failed check exits 1 after writing its record):
  split --lineage r1 --schedule formal-r1.jsonl [--held-out-matchups 2] --out SPLIT.json
  records --lineage r1 --schedule formal-r1.jsonl --record-list LIST --custody CUSTODY.json
          --role r1-records --out RECORDS.json
  games --records RECORDS.json --replay REPLAY.jsonl --rebuild REBUILD.jsonl --parent P.json
        --out-dir DIR --output-directory BANK_DIR
  bank-check --games-status GAMES.json --bank BANK.json --out CHECK.json
  targets --spec SPEC.json --out-dir DIR
  commands --lineage r1 --parent P.json --anchor ANCHOR.json --census CENSUS.json
           --exactness EXACT-r1.json EXACT-r2.json --root DIR --out-dir DIR
  probe --lineage r1 --parent P.json --o-final O.json --anchor ANCHOR.json --offers OFFERS.json
        --census CENSUS.json [--full PHASE1.json] --output-directory DIR --out CMD.json
  select --lineage r1 --seed K --probe PROBE.json --reference REF_PROBE.json --full PHASE1.json
         --rerun RERUN.json --split SPLIT.json --out SELECT.json
  phase2 --lineage r1 --o-config O.json --select SELECT-0.json ... SELECT-7.json --workers W
         --root DIR --out-dir DIR
  known-answer --rebuild r1=REBUILD.jsonl [r2=...] --rows r1=ROWS-r1.jsonl [r2=...]
               --schedule r1=archive-r1.jsonl [r2=...] --jobs JOBS.json --custody CUSTODY.json
               --out KNOWN.json
  bt1-regression --rows r1=ROWS-r1.jsonl [r2=...] --out REGRESSION.json
  o-check --lineage r1 --run O-s0/completion.json --recorded O/completion.json
          --o-final O_FINAL_CHECKPOINT.json --out OCHECK.json
  custody --spec CUSTODY-SPEC.json --out-dir DIR --out CUSTODY.json
  parents --spec PARENTS-SPEC.json --custody CUSTODY.json --out PARENTS.json
  reference-check --lineage r1 --probe STAGE0-PROBE.json --screen-probe SCREEN-PROBE.json
                  --screen-probe-sha256 SHA --out REFERENCES.json
  fit --selection SELECT.json ... --probes PROBE.json ... --out FIT.json
  configs-check --lineage r1 --o-config O.json --select SELECT-0.json ... SELECT-7.json --root DIR
                --config D2-0=CFG.json ... D2-7=CFG.json O-0=O-s0/CONFIG.json O-1=CFG.json ... O-7=CFG.json
                [--qualification D2-0=CFG.json ... O-7=CFG.json] --out CONFIGS.json
SPEC: {"r1": {"split": PIN, "games": PIN (GAMES-r1.json), "bank_check": PIN}, "r2": {...}}
CUSTODY-SPEC and PARENTS-SPEC: see custody() and parents().
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import ntpath
import os
import posixpath
import re
import sys
import tarfile
import types
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import bt1  # noqa: E402
import fit  # noqa: E402
import phase1  # noqa: E402
import phase2  # noqa: E402
from distill_common import (LINEAGES, PARENT_CHECKPOINT_SHA256, absolute, check_parent, episode, pilot,  # noqa: E402
                            pin, read_json, read_jsonl, require, write_json)

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
# The search arm's key mode (the archive and the utility screen both ran
# BUDGET_KEY_MODE=repaired, which records it as repaired-v2).
KEY_MODE = "repaired-v2"
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


def custody_record(path: Path) -> dict:
    """Stage 0's custody record (custody's --out), which must have passed."""
    value = read_json(path)
    require(isinstance(value, dict) and value.get("schema") == CUSTODY_SCHEMA and value.get("pass") is True,
            f"{path} is not a custody record that passed")
    return value


def role_copies(custody_value: dict, role: str) -> dict:
    """A custody role's copies by path_key: each member's extracted copy and,
    when it was remapped, its remapped copy, each with its SHA-256."""
    out = {}
    for m in custody_value["members"]:
        if m["role"] != role:
            continue
        for kind in ("extracted", "remapped"):
            copy = m.get(kind)
            if copy is not None:
                out[path_key(copy["path"])] = {"sha256": copy["sha256"], "member": m["member"], "id": m.get("id")}
    return out


def records(lineage: str, schedule: Path, record_list: Path, custody_path: Path, role: str) -> dict:
    """Custody: one record file per scheduled case, bound by id, each holding
    exactly one budget_game row. Each listed file is one of custody's copies
    of the role's members (an extracted or remapped copy, by path_key), read
    as an absolute path with no link on it and outside custody's forbidden
    roots, and its bytes have that copy's SHA-256; the schedule is the one
    custody verified for the lineage. The record's fields are checked against
    its case by the root rebuild, where a mismatch voids the game."""
    cases = schedule_cases(schedule, lineage)
    held = custody_record(custody_path)
    roots = Roots(held["forbidden"])
    frozen = [f for f in held["files"] if f["role"] == f"{lineage}-schedule" and f.get("verified")]
    require(len(frozen) == 1 and frozen[0]["sha256"] == sha256_file(schedule),
            f"{schedule} is not the {lineage} schedule custody verified")
    copies = role_copies(held, role)
    require(copies, f"custody holds no copy of role {role!r}")
    files = [line.strip() for line in Path(record_list).read_text(encoding="utf-8").splitlines() if line.strip()]
    for f in files:
        require(os.path.isabs(f), f"{record_list}: {f} is not an absolute path")
    require(len({path_key(f) for f in files}) == len(files), "the record list repeats a file")
    by_case = {}
    for f in files:
        copy = copies.get(path_key(f))
        require(copy is not None, f"{f} is not one of custody's {role} copies")
        problem = path_problem(f, roots)
        require(problem is None, f"{f}: {problem}")
        try:
            data = Path(f).read_bytes()
        except OSError as error:
            raise ValueError(f"{f} cannot be read: {error}") from None
        got = hashlib.sha256(data).hexdigest()
        require(got == copy["sha256"],
                f"{f} has SHA-256 {got}, not custody's {copy['sha256']} for member {copy['member']}")
        try:
            lines = [json.loads(x) for x in data.decode("utf-8").splitlines() if x.strip()]
        except (ValueError, UnicodeDecodeError) as error:
            raise ValueError(f"{f} is not JSON lines: {error}") from None
        require(len(lines) == 1 and isinstance(lines[0], dict) and lines[0].get("kind") == "budget_game",
                f"{f}: a record file holds exactly one budget_game row")
        gid = lines[0].get("id")
        require(copy["id"] is None or gid == copy["id"], f"{f}: its record is {gid!r}, not custody's case {copy['id']}")
        require(gid not in by_case, f"{gid}: two record files")
        by_case[gid] = {"path": f, "sha256": got}
    require(set(by_case) == {c["id"] for c in cases}, "the records are not exactly the schedule's cases")
    return {"schema": RECORDS_SCHEMA, "lineage": lineage, "schedule": pin(schedule), "record_list": pin(record_list),
            "custody": pin(custody_path), "role": role,
            "cases": [{"id": c["id"], "record": by_case[c["id"]]} for c in cases]}


# ------------------------------------------------------------------ games


def by_id(rows, what: str) -> dict:
    out = {}
    for r in rows:
        require(r.get("id") not in out, f"{what}: {r.get('id')} appears twice")
        out[r.get("id")] = r
    return out


def rebuild_rows(path: Path, what: str) -> tuple:
    """A root rebuild's rows by id, and its input void rows by file: a listed
    file it could not read, or a line of one that is not JSON, gives a row
    with a null id, the list's line as its file and the reason as its void
    (several lines of one file give several rows)."""
    rows, unread = {}, {}
    for r in read_jsonl(path):
        require(isinstance(r, dict) and r.get("schema") == REBUILD_SCHEMA, f"{what}: not a root-rebuild row")
        if r.get("id") is None:
            require(isinstance(r.get("file"), str) and isinstance(r.get("void"), str),
                    f"{what}: a row without an id is not an input void row (file and reason)")
            unread.setdefault(r["file"], []).append(r["void"])
            continue
        require(r["id"] not in rows, f"{what}: {r['id']} appears twice")
        rows[r["id"]] = r
    return rows, unread


def void_reason(record: dict, replay: dict | None, rebuild: dict | None, unread: list | None = None) -> str | None:
    """Why a game is void, or None; the order of the checks is fixed."""
    if unread:
        return f"root rebuild: {unread[0]}"
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
    rebuilt, unread = rebuild_rows(rebuild, "rebuild")
    cases = {c["id"] for c in rec["cases"]}
    require(set(replays) <= cases and set(rebuilt) <= cases, "a replay or rebuild row names no scheduled case")
    require(set(unread) <= {c["record"]["path"] for c in rec["cases"]},
            "an input void row of the rebuild names no listed record file")
    for row in rebuilt.values():
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
        reason = void_reason(record, replays.get(gid), rebuilt.get(gid), unread.get(entry["path"]))
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
    rebuilt, _ = rebuild_rows(pinned(status["rebuild"], f"{lineage} rebuild"), "rebuild")
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
    every, _ = rebuild_rows(pinned(status["rebuild"], f"{lineage} rebuild"), "rebuild")
    require(is_pin(the_split.get("schedule")), f"{lineage}: the split pins no schedule")
    require(all(r.get("schedule_sha256") == the_split["schedule"]["sha256"] for r in every.values()),
            f"{lineage}: the split was fixed on another schedule than the one the rebuild checked")
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
            "tried_rivals_at_evidence_rows": tried,
            "tried_rivals_separated_share": separated / tried if tried else None,
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


def unpathed(value):
    """``value`` without its file paths (every "path" key), so that pinned
    inputs compare by their SHA-256 alone."""
    if isinstance(value, dict):
        return {k: unpathed(v) for k, v in value.items() if k != "path"}
    if isinstance(value, list):
        return [unpathed(v) for v in value]
    return value


def probe_identity(probe: dict, reference: dict) -> dict:
    """The parent's and O-final's readouts, and the probe's inputs and build,
    equal stage 2's reference probe's bit for bit; pinned files compare by
    their SHA-256, not their paths."""
    differ = [field for field in PROBE_INPUTS
              if canonical(unpathed(probe.get(field))) != canonical(unpathed(reference.get(field)))]
    require(probe.get("evidence") is not None, "an SD1 probe reads the evidence keep-set files")
    mine, ref = readouts(probe), readouts(reference)
    for cid in REFERENCE_IDS:
        require(cid in mine and cid in ref, f"a probe lacks the {cid} candidate")
        if canonical(unpathed(mine[cid])) != canonical(unpathed(ref[cid])):
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


def select(lineage: str, seed: int, probe: dict, reference: dict, full: dict, rerun: dict, the_split: dict,
           names: list | None = None) -> dict:
    """One seed run's fit reading and D1 (design sections 5 and 7.1). Every
    held-out row's game and matchup must be the split's; the run's telemetry
    is read against its pin."""
    label = cell_label(seed)
    require(full.get("schema") == PHASE1_SCHEMA and full.get("complete") is True
            and full.get("stop_after_steps") is None and full["identity"]["lineage"] == lineage,
            f"{label}: not a complete {lineage} full run")
    options = full["identity"]["recipe"].get("options") or {}
    require(options.get("cell") == label and options.get("keep_sets") is not None, f"not the {label} keep-set cell")
    require([c["step"] for c in full["checkpoints"]] == list(CHECKPOINT_STEPS),
            f"{label}: the run's checkpoints are not steps 8 to 8,192")
    require(probe.get("lineage") == lineage and reference.get("lineage") == lineage, "a probe of another lineage")
    require(probe["evidence"]["training"]["sha256"] == options["keep_sets"]["sha256"],
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
    require(isinstance(the_split, dict) and the_split.get("schema") == SPLIT_SCHEMA
            and the_split.get("lineage") == lineage, "not this lineage's split")
    names = names if names is not None else deck_names()
    held_games = {g: tuple(names[i] for i in the_split["matchups"][g])
                  for g, side in the_split["games"].items() if side == "held_out"}
    telemetry = read_pinned(full["telemetry"], f"{label} telemetry")
    reproduced = fit.reproduced(full, rerun)
    entries, stop = [], None
    for c in full["checkpoints"]:
        cid = f"{label}-step-{c['step']}"
        readout = values.get(cid)
        require(readout is not None and readout["state_sha256"] == c["state_sha256"],
                f"{label}: the probe did not read checkpoint step {c['step']}")
        rows = held_out_rows(readout)
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
        chosen = next(c for c in full["checkpoints"] if c["step"] == d1["selected_step"])
        out.update(reading="passes", d1=d1, selected_step=d1["selected_step"], fit_dose=passing[0]["step"],
                   highest_passing_step=passing[-1]["step"],
                   d1_checkpoint={k: chosen[k] for k in ("step", "checkpoint", "source", "state_sha256")})
    elif stop is not None and stop[1]["F"]["pass"]:
        out["reading"] = "required fit reached only together with collapse"
    elif stop is not None:
        out["reading"] = "collapse before the required fit"
    else:
        out["reading"] = "required fit not reached"
    return out


# ------------------------------------------------- phase-1 and probe commands


def census_inputs(census_record: dict, lineage: str) -> dict:
    """The stage-2 census's inputs of one lineage, once its stop rule is
    open: the games file and bank it read and its two keep-set files, each
    checked against its pin."""
    require(census_record.get("schema") == CENSUS_SCHEMA, "not an SD1 census")
    require(census_record["stop_rule"]["stop"] is False, "the census's stop rule stopped SD1")
    entry = census_record["lineages"][lineage]
    keep = entry["keep_sets"]
    require(all(keep.get(side) is not None for side in SPLITS), f"{lineage}: a keep-set file is missing")
    for side in SPLITS:
        pinned(keep[side], f"{lineage} {side} keep sets")
    check = read_pinned(census_record["spec"][lineage]["bank_check"], f"{lineage} bank check")
    status = read_pinned(census_record["spec"][lineage]["games"], f"{lineage} games status")
    games_file, bank = status["games_file"], check["bank"]
    pinned(games_file, f"{lineage} games file")
    pinned(bank, f"{lineage} bank")
    return {"games": games_file, "bank": bank, "keep_sets": keep}


def seed_options(keep_sets: dict, seed: int) -> dict:
    """A seed run's phase-1 options (section 5): its cell label, the training
    keep-set file and the read checkpoint steps. Nothing else changes from
    the screen's arm U recipe; the engine reads the seed from the label."""
    return {"cell": cell_label(seed), "keep_sets": keep_sets["training"], "checkpoint_steps": list(CHECKPOINT_STEPS)}


def commands(lineage: str, parent: dict, anchor: dict, census_record: dict, exactness: list, root: str) -> dict:
    """Each seed's dry-run, full-run and rerun phase-1 command, on the
    census's games file and bank, once the stop rule is open and phase-1
    exactness has passed in both lineages."""
    inputs = census_inputs(census_record, lineage)
    bt1.exact_in_both_lineages(exactness)
    out = {}
    for seed in SEEDS:
        label = cell_label(seed)
        base = f"{root.rstrip('/')}/{lineage}-{label}"
        for suffix, extra in (("-dry", {"dry_run": True}), ("", {}), ("-rerun", {"rerun": True})):
            out[label + suffix] = phase1.phase1_command(
                lineage, "U", parent, Path(inputs["games"]["path"]), Path(inputs["bank"]["path"]), anchor,
                base + suffix, options=seed_options(inputs["keep_sets"], seed), **extra)
    return out


def probe_command(lineage: str, parent: dict, o_final: dict, anchor: dict, offers: list, census_record: dict,
                  full: dict | None, output_directory: str) -> dict:
    """Stage 2's reference probe (the parent and O-final, ``full`` None) or
    one seed run's probe (its eleven checkpoints besides), on the census's
    games file and bank, with both keep-set files as evidence inputs."""
    inputs = census_inputs(census_record, lineage)
    runs = []
    if full is not None:
        options = full["identity"]["recipe"].get("options") or {}
        require(CELL_LABEL.match(str(options.get("cell", ""))) is not None
                and (options.get("keep_sets") or {}).get("sha256") == inputs["keep_sets"]["training"]["sha256"]
                and options.get("checkpoint_steps") == list(CHECKPOINT_STEPS)
                and full["identity"]["bank"]["sha256"] == inputs["bank"]["sha256"],
                "not an SD1 seed run on the census's bank and training keep sets")
        runs.append(full)
    command = phase1.probe_command(lineage, parent, o_final, Path(inputs["games"]["path"]),
                                   Path(inputs["bank"]["path"]), anchor, offers, runs, output_directory,
                                   evidence=(Path(inputs["keep_sets"]["training"]["path"]),
                                             Path(inputs["keep_sets"]["held_out"]["path"])))
    require(len(command["candidates"]) == len(REFERENCE_IDS) + (len(CHECKPOINT_STEPS) if runs else 0),
            "a seed run's probe reads the parent, O-final and its eleven checkpoints")
    return command


def phase2_configs(lineage: str, o_config: dict, selections: list, workers: int, root: str) -> dict:
    """Stage 4's configs (section 6), once all eight seeds have a D1: D2-k
    from D1-k's checkpoint and O-k (k = 1 to 7) from the parent, each on
    schedule seed k under the sd1 namespace. O-0 is stage 0's O-s0."""
    by_seed = {}
    for selection in selections:
        require(selection.get("schema") == SELECT_SCHEMA and selection.get("lineage") == lineage,
                "not this lineage's selection")
        require(selection["seed"] not in by_seed, f"seed {selection['seed']} is selected twice")
        by_seed[selection["seed"]] = selection
    require(sorted(by_seed) == list(SEEDS), "phase 2 needs every seed's selection")
    missing = [k for k in SEEDS if by_seed[k]["reading"] != "passes"]
    require(not missing, f"{lineage}: seeds {missing} have no D1, so the lineage runs no phase 2 or panels")
    out = {}
    for seed in SEEDS:
        chosen = by_seed[seed]["d1_checkpoint"]
        source = read_pinned(chosen["source"], f"{lineage} D1-{seed} source")
        require(source.get("checkpoint") == chosen["checkpoint"], f"{lineage} D1-{seed}: the source's checkpoint "
                "is not the selected one")
        pinned(chosen["checkpoint"], f"{lineage} D1-{seed} checkpoint")
        base = f"{root.rstrip('/')}/{lineage}"
        out[f"D2-{seed}"] = phase2.config(lineage, o_config, source, workers, f"{base}-D2-{seed}/native",
                                          replicate=seed, namespace=phase2.SD1_NS)
        if seed:
            out[f"O-{seed}"] = phase2.config(lineage, o_config, None, workers, f"{base}-O-{seed}/native",
                                             replicate=seed, namespace=phase2.SD1_NS)
    return out


# ----------------------------------------------------------- tool checks


def pairs(values: list, what: str) -> dict:
    out = {}
    for v in values:
        lineage, _, path = v.partition("=")
        require(lineage in LINEAGES and path and lineage not in out, f"{what}: give LIN=PATH once per lineage")
        out[lineage] = Path(path)
    return out


def archive_jobs(jobs: dict) -> dict:
    """BT1's stage-1 jobs (its JOBS.json) per lineage: each archive game's id
    and the record file it was read from."""
    out = {lineage: {} for lineage in LINEAGES}
    require(isinstance(jobs, dict) and isinstance(jobs.get("jobs"), list), "not BT1's JOBS.json")
    for job in jobs["jobs"]:
        require(isinstance(job, dict) and job.get("lineage") in LINEAGES and isinstance(job.get("id"), str)
                and isinstance(job.get("record"), str), "not BT1's JOBS.json")
        require(job["id"] not in out[job["lineage"]], f"{job['id']}: two jobs")
        out[job["lineage"]][job["id"]] = job["record"]
    return out


def known_inputs(custody_path: Path, jobs: Path, schedules: dict) -> dict:
    """known-answer's inputs as custody verified them, the way records binds
    its list: the JOBS.json given is custody's (its known_answer jobs pin) and
    each archive schedule given is custody's LIN-archive-schedule copy, each
    by path_key and SHA-256, read outside custody's forbidden roots and
    through no link. Returns the custody record's pin."""
    held = custody_record(custody_path)
    roots = Roots(held["forbidden"])
    want = (held.get("known_answer") or {}).get("jobs")
    given = {"JOBS.json": (jobs, want if is_pin(want) else None)}
    for lineage, path in schedules.items():
        copies = [f for f in held["files"] if f["role"] == f"{lineage}-archive-schedule" and f.get("verified")]
        given[f"{lineage}-archive-schedule"] = (path, copies[0] if len(copies) == 1 else None)
    for what, (path, copy) in given.items():
        require(copy is not None and path_key(path) == path_key(copy["path"]),
                f"{path} is not custody's {what}" + (f" {copy['path']}" if copy else ""))
        problem = path_problem(str(path), roots)
        require(problem is None, f"{path}: {problem}")
        got = sha256_file(path)
        require(got == copy["sha256"], f"{path} has SHA-256 {got}, not custody's {copy['sha256']} for its {what}")
    return pin(custody_path)


def known_answer(rebuilds: dict, rows: dict, schedules: dict, jobs: dict) -> dict:
    """The rebuild's maps over the Spy archive's games against BT1's committed
    live maps: every new_root row of BT1's ROWS in an archive game the SD1
    build kept must match, at least MIN_KNOWN_ROOTS per lineage. Every rebuild
    row must name one of BT1's jobs (JOBS.json) and have read its record file,
    every row with an id must have been checked against the archive's
    schedule (the file given, which is pinned) and every kept game read in
    key mode repaired-v2. The games the rebuild voided, by id or as a record
    file it could not read (an input void row, matched by file), are listed
    and not counted (not replayed); a game BT1's ROWS name that has no rebuild
    row is absent, and fails the check."""
    by_lineage = archive_jobs(jobs)
    out = {"schema": KNOWN_SCHEMA, "lineages": {}}
    for lineage, path in rows.items():
        require(lineage in rebuilds and lineage in schedules, f"{lineage}: give its rebuild output and schedule")
        schedule_sha256 = sha256_file(schedules[lineage])
        rebuilt, unread = rebuild_rows(rebuilds[lineage], f"{lineage} rebuild")
        mine = by_lineage[lineage]
        failures = []
        for gid, r in sorted(rebuilt.items()):
            if gid not in mine:
                failures.append(f"{gid}: not one of BT1's {lineage} jobs")
            elif path_key(r.get("file", "")) != path_key(mine[gid]):
                failures.append(f"{gid}: the rebuild read {r.get('file')!r}, not its JOBS.json record")
            if r.get("schedule_sha256") != schedule_sha256:
                failures.append(f"{gid}: not checked against the schedule {schedules[lineage]}")
            if r["void"] is None and r.get("key_mode") != KEY_MODE:
                failures.append(f"{gid}: read in key mode {r.get('key_mode')!r}, not {KEY_MODE}")
        files = {path_key(f): gid for gid, f in mine.items()}
        unreadable = {}
        for f, reasons in sorted(unread.items()):
            gid = files.get(path_key(f))
            if gid is None:
                failures.append(f"{f}: an input void row names no BT1 {lineage} job's record")
            else:
                unreadable[gid] = {"id": gid, "file": f, "reason": reasons[0]}
        roots = {(gid, x["step"]): x for gid, r in rebuilt.items()
                 if r["void"] is None and gid not in unreadable for x in r["roots"]}
        compared, differ, missing, not_replayed, absent, total = 0, [], [], Counter(), Counter(), 0
        for r in read_jsonl(path):
            if r["chooser"] != "new_root":
                continue
            total += 1
            game = rebuilt.get(r["game"])
            if r["game"] in unreadable or (game is not None and game["void"] is not None):
                not_replayed[r["game"]] += 1
                continue
            if game is None:
                absent[r["game"]] += 1
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
            "rebuild": pin(rebuilds[lineage]), "rows": pin(path), "schedule": pin(schedules[lineage]),
            "games": len(rebuilt), "void_games": voids, "unreadable_games": sorted(unreadable.values(),
                                                                                  key=lambda v: v["id"]),
            "jobs_without_rows": sorted(set(mine) - set(rebuilt) - set(unreadable)),
            "bt1_new_root_rows": total, "roots_compared": compared, "roots_differing": differ,
            "roots_missing": missing, "roots_not_replayed": sum(not_replayed.values()),
            "games_not_replayed": sorted(not_replayed), "roots_absent": sum(absent.values()),
            "games_absent": sorted(absent), "failures": failures,
            "views": dict(Counter(x["view"] for x in roots.values())),
            "pass": compared >= MIN_KNOWN_ROOTS and not differ and not missing and not absent and not failures}
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


def retained(entry) -> tuple:
    """(the JSON a pin names, None) when its file is retained as pinned;
    (None, "missing") when the file is not retained; (None, the reason)
    when it differs from its pin or is not JSON."""
    if not is_pin(entry):
        return None, "not a pin"
    path = Path(entry["path"])
    if not path.exists():
        return None, "missing"
    try:
        data = path.read_bytes()
    except OSError as error:
        return None, f"{path} cannot be read ({error})"
    got = hashlib.sha256(data).hexdigest()
    if got != entry["sha256"]:
        return None, f"{path} has SHA-256 {got}, not its pin's {entry['sha256']}"
    try:
        return json.loads(data), None
    except (ValueError, UnicodeDecodeError) as error:
        return None, f"{path} is not JSON ({error})"


def update_records(completion: dict) -> tuple:
    """A run's update records' X1 fields in order, as far as they are
    retained, and where the sequence ends: None when every receipt and
    update record is read, else {iteration, problem} at the first one that
    is missing ("missing": not retained) or differs from its pin."""
    out = []
    for index, receipt_pin in enumerate(completion["iterations"]):
        receipt, problem = retained(receipt_pin)
        update = None
        if problem is None:
            if isinstance(receipt, dict):
                update, problem = retained(receipt.get("update"))
            else:
                problem = "the receipt is not a JSON object"
        if problem is not None:
            return out, {"iteration": index, "problem": problem}
        require(receipt.get("iteration") == index, "iteration receipts out of order")
        out.append({f: update.get(f) for f in X1_FIELDS})
    return out, None


def o_check(lineage: str, run: dict, recorded: dict, o_final: Path) -> dict:
    """O-s0 against the recorded O: the final state and Adam step against O's
    final checkpoint and, as far as the recorded update records are retained,
    the first update whose X1 fields differ (after_state_sha256 first). A
    recorded receipt or update record that is retained but differs from its
    pin fails the check; one that is missing ends the comparison."""
    end = phase2.check(lineage, run, o_final)
    mine, mine_end = update_records(run)
    require(len(mine) == pilot.UPDATES and mine_end is None, "O-s0's own update records are incomplete")
    theirs, theirs_end = update_records(recorded)
    first = None
    for index, (a, b) in enumerate(zip(mine, theirs)):
        if canonical(a) != canonical(b):
            first = {"update": index + 1, "fields": [f for f in X1_FIELDS if canonical(a[f]) != canonical(b[f])]}
            break
    mismatch = theirs_end is not None and theirs_end["problem"] != "missing"
    return {"schema": O_CHECK_SCHEMA, "lineage": lineage, "endpoint": end, "recorded_updates_retained": len(theirs),
            "recorded_end": theirs_end, "recorded_mismatch": mismatch,
            "compared_updates": min(len(mine), len(theirs)), "first_differing_update": first,
            "pass": end["reproduced"] and first is None and not mismatch}


# ------------------------------------------------------- stage 0: custody

CUSTODY_SPEC_SCHEMA = "sd1-v1/custody-spec"
CUSTODY_SCHEMA = "sd1-v1/custody"
# The utility screen's source seal and custody completion (its
# seal_evidence.py): each member of EVIDENCE.tar with its SHA-256, each
# retained original path's member, the two durable archives (the hot root's
# copy is not among them) and the rule that binds the two durable copies.
SEAL_SCHEMA = "search-player-utility-source-seal/v1"
COMPLETION_SCHEMA = "search-player-utility-custody/v1"
COMPLETION_RULE = ("Both durable completion records must be identical and complete, and their source_seals pins "
                   "must match both complete SOURCE-SEAL copies.")
# The utility screen's formal analysis index (its analysis_index.py): one
# entry per job id CASE-ARM; a search-arm entry's row pins the job's
# game.jsonl, and the job spec (utility_queue.py) is the command.json beside it.
INDEX_SCHEMA = "search-player-utility-analysis-index/v1"
# The utility screen's analysis_index.py builds the index from this input spec
# (ANALYSIS-SOURCES-001) and pins it in the index as source_spec.
ANALYSIS_SOURCES_SCHEMA = "search-player-utility-analysis-sources/v1"
SEARCH_ARM = "-search-v1"
JOB_SPEC = "command.json"
# The inputs a job spec names that custody checks per lineage, each with the
# name of its list (DIR/lists/LIN-NAME.txt).
INPUT_LISTS = {"source": "source", "bank_spec": "bank-spec", "T1": "t1", "A48": "a48"}
# The roles custody derives; a spec's own members and files take others.
DERIVED_ROLES = frozenset({"schedule", "known-answer", "budget-inputs", "index", "analysis-sources", "allocation",
                           "inputs"} | {f"{lineage}-{suffix}" for lineage in LINEAGES
                                        for suffix in ("records", "jobs", "schedule", "archive", "archive-schedule",
                                                       *INPUT_LISTS.values())})
HEX64 = re.compile(r"^[0-9a-f]{64}$")
SAFE_PART = re.compile(r"^[A-Za-z0-9_.-]+$")
WINDOWS_ABSOLUTE = re.compile(r"^[A-Za-z]:[\\/]")
POSIX_ABSOLUTE = re.compile(r"^/[^/]+/[^/]")
# UNC and verbatim spellings: two slashes or two backslashes first.
DOUBLE_ABSOLUTE = re.compile(r"^(?:\\\\|//)[^\\/]")
VERBATIM = re.compile(r"^//[?.]/(unc/)?", re.IGNORECASE)
WINDOWS_SPELLING = re.compile(r"^(?:[A-Za-z]:(?:/|$)|//)")
# A string may carry several paths ("T1=PATH,A48=PATH", "source=PATH").
PATH_DELIMITERS = re.compile(r"([,;=])")
CHUNK = 1 << 20
CUSTODY_SPEC_REQUIRED = {"schema", "durable", "forbidden", "index", "analysis_sources", "schedule", "known_answer"}
CUSTODY_SPEC_KEYS = CUSTODY_SPEC_REQUIRED | {"rewrite", "keep", "members", "files", "counts"}
# The kinds of sealed file custody remaps: a job spec, the model sources and
# bank spec it names, a model source's play-import descriptor, and a spec
# member marked remap. Every other kind (records, case files, binaries,
# checkpoints, parameter objects) is extracted byte for byte.
REMAPPED_KINDS = ("job", "source", "bank_spec", "import", "explicit")
# The files a job spec names that its job, SD1's replay or its rebuild reads:
# all but its working directory and the other paths it names (outputs and
# logs), which alone a keep prefix may hold.
JOB_READS = ("binary", "source", "record", "bank_spec", "cases")
# The remapped inputs a job reads: every path inside one, and inside each
# remapped member it names in turn, is read too.
INPUT_KINDS = ("source", "bank_spec", "import")


def path_key(path) -> str:
    """A path's identity for lookups: forward slashes, a verbatim prefix
    dropped (a drive's verbatim spelling is the drive's, a UNC one's is
    //host/share), no trailing slash and case folded, as the seal's own
    lookup folds case."""
    text = str(path).replace("\\", "/")
    verbatim = VERBATIM.match(text)
    if verbatim:
        text = ("//" if verbatim.group(1) else "") + text[verbatim.end():]
    return text.rstrip("/").casefold()


def norm_spelling(path) -> str:
    """A path as path_key spells it, case kept, with ".", ".." and repeated
    separators collapsed; for a drive or UNC spelling (ntpath's rules), each
    name's trailing dots and spaces dropped too, as Windows drops them."""
    text = str(path).replace("\\", "/")
    verbatim = VERBATIM.match(text)
    if verbatim:
        text = ("//" if verbatim.group(1) else "") + text[verbatim.end():]
    text = text.rstrip("/")
    if not text:
        return text
    if WINDOWS_SPELLING.match(text):
        parts = ntpath.normpath(text).replace("\\", "/").split("/")
        return "/".join(p if p in (".", "..") else p.rstrip(". ") for p in parts).rstrip("/")
    return posixpath.normpath(text)


def norm_key(path) -> str:
    return norm_spelling(path).casefold()


def under(key: str, root_key: str) -> bool:
    return key == root_key or key.startswith(root_key + "/")


def parent_key(path) -> str:
    return norm_key(path).rsplit("/", 1)[0]


def is_absolute_token(token) -> bool:
    return isinstance(token, str) and bool(WINDOWS_ABSOLUTE.match(token) or POSIX_ABSOLUTE.match(token)
                                           or DOUBLE_ABSOLUTE.match(token))


def safe_name(name) -> bool:
    return isinstance(name, str) and all(SAFE_PART.match(p) and p not in (".", "..") for p in name.split("/"))


def stream_sha256(path) -> str:
    digest = hashlib.sha256()
    with Path(path).open("rb") as handle:
        for block in iter(lambda: handle.read(CHUNK), b""):
            digest.update(block)
    return digest.hexdigest()


def is_pin(value) -> bool:
    return (isinstance(value, dict) and isinstance(value.get("path"), str)
            and isinstance(value.get("sha256"), str) and HEX64.match(value["sha256"]) is not None)


def beside(path: str, name: str) -> str:
    """``name`` in ``path``'s directory, in ``path``'s own spelling."""
    cut = max(path.rfind("/"), path.rfind("\\"))
    return path[:cut + 1] + name


def link_on(path: Path):
    """The first symbolic link or junction on ``path`` or any of its
    ancestors, or None."""
    for p in (path, *path.parents):
        if p.is_symlink() or p.is_junction():
            return p
    return None


class Roots:
    """The hot and holding roots SD1 never reads from, each by its spelling,
    its real path and, where it exists, its file identity (which catches an
    8.3 short name, a SUBST drive or another mount of the same directory)."""

    def __init__(self, roots):
        self.roots = list(roots)
        self.keys, self.stats = [], []
        for root in self.roots:
            keys, st = {norm_key(root)}, None
            if os.path.isabs(root):
                keys.add(norm_key(os.path.realpath(root)))
                try:
                    st = os.stat(root)
                except (OSError, ValueError):
                    st = None
            self.keys.append(keys)
            # Without file ids (st_ino 0) identity cannot tell directories apart.
            self.stats.append(st if st is not None and st.st_ino else None)

    def containing(self, path) -> str | None:
        """The root ``path`` lies under by its normalized spelling or its
        real path, or because an ancestor of its real path is that root's
        directory; None when it lies under none."""
        keys = {norm_key(path)}
        real = os.path.realpath(path) if os.path.isabs(path) else None
        if real is not None:
            keys.add(norm_key(real))
        for root, root_keys in zip(self.roots, self.keys):
            if any(under(k, r) for k in keys for r in root_keys):
                return root
        if real is not None and any(s is not None for s in self.stats):
            for ancestor in (Path(real), *Path(real).parents):
                try:
                    st = os.stat(ancestor)
                except (OSError, ValueError):
                    continue
                for root, root_stat in zip(self.roots, self.stats):
                    if root_stat is not None and os.path.samestat(st, root_stat):
                        return root
        return None


def path_problem(path, roots: Roots) -> str | None:
    """Why a file may not be read: a path that is not absolute, one under a
    forbidden root (Roots.containing), or a link on it or on any ancestor
    (custody follows none, as the utility screen's own tools refuse them);
    None when it may be read."""
    if not isinstance(path, str) or not os.path.isabs(path):
        return f"{path} is not an absolute path"
    root = roots.containing(path)
    if root is not None:
        return f"{path} lies under the hot or holding root {root}, which SD1 never reads"
    link = link_on(Path(path))
    if link is not None:
        return f"{path}: {link} is a symbolic link or junction, which custody does not follow"
    return None


def verify_file(entry, what: str, failures: list, roots: Roots) -> bool:
    """A pinned file in place: an absolute path outside every forbidden root
    with no link on it, present, its bytes' SHA-256 the pin's. A failure is
    listed, not raised."""
    if not is_pin(entry):
        failures.append(f"{what}: not a pin (a path and a SHA-256)")
        return False
    problem = path_problem(entry["path"], roots)
    if problem is not None:
        failures.append(f"{what}: {problem}")
        return False
    path = Path(entry["path"])
    if not path.is_file():
        failures.append(f"{what}: {entry['path']} is missing")
        return False
    got = stream_sha256(path)
    if got != entry["sha256"]:
        failures.append(f"{what}: {entry['path']} has SHA-256 {got}, not {entry['sha256']}")
        return False
    return True


def read_verified(entry, what: str, failures: list, roots: Roots):
    """verify_file's checks on one read of a small file: its bytes, or None
    (listed). An entry without a SHA-256 is read as it is."""
    if not (isinstance(entry, dict) and isinstance(entry.get("path"), str)
            and ("sha256" not in entry or is_pin(entry))):
        failures.append(f"{what}: not a pin (a path and a SHA-256)")
        return None
    problem = path_problem(entry["path"], roots)
    if problem is not None:
        failures.append(f"{what}: {problem}")
        return None
    try:
        data = Path(entry["path"]).read_bytes()
    except FileNotFoundError:
        failures.append(f"{what}: {entry['path']} is missing")
        return None
    except OSError as error:
        failures.append(f"{what}: {entry['path']} cannot be read ({error})")
        return None
    got = hashlib.sha256(data).hexdigest()
    if "sha256" in entry and got != entry["sha256"]:
        failures.append(f"{what}: {entry['path']} has SHA-256 {got}, not {entry['sha256']}")
        return None
    return data


def parse_json(data, what: str, failures: list):
    """JSON from bytes already read, or None (listed when unreadable)."""
    if data is None:
        return None
    try:
        return json.loads(data)
    except (ValueError, UnicodeDecodeError) as error:
        failures.append(f"{what}: not readable JSON ({error})")
        return None


def own_role(role) -> bool:
    return safe_name(role) and "/" not in role and role not in DERIVED_ROLES


def custody_spec_problems(spec) -> list:
    if not isinstance(spec, dict) or spec.get("schema") != CUSTODY_SPEC_SCHEMA:
        return [f"not an {CUSTODY_SPEC_SCHEMA} spec"]
    problems = [f"unknown field {k!r}" for k in sorted(set(spec) - CUSTODY_SPEC_KEYS)]
    problems += [f"missing field {k!r}" for k in sorted(CUSTODY_SPEC_REQUIRED - set(spec))]
    durable = spec.get("durable")
    if "durable" in spec and not (isinstance(durable, list) and len(durable) == 2 and all(
            isinstance(d, dict) and set(d) == {"seal", "archive", "completion"} and all(is_pin(v) for v in d.values())
            for d in durable)):
        problems.append("durable: both durable copies, each {seal, archive, completion} pins")
    forbidden = spec.get("forbidden")
    forbidden_ok = isinstance(forbidden, list) and bool(forbidden) and all(is_absolute_token(v) for v in forbidden)
    if "forbidden" in spec and not forbidden_ok:
        problems.append("forbidden: a nonempty list of absolute paths (the hot and holding roots)")
    for key in ("index", "analysis_sources", "schedule"):
        if key in spec and not is_pin(spec[key]):
            problems.append(f"{key}: a pin")
    answer = spec.get("known_answer")
    if "known_answer" in spec and not (isinstance(answer, dict) and set(answer) == {"jobs", "case_pins", "budget_root"}
                                       and is_pin(answer["jobs"]) and is_pin(answer["case_pins"])
                                       and is_absolute_token(answer["budget_root"])):
        problems.append("known_answer: {jobs: PIN, case_pins: PIN, budget_root: an absolute path}")
    keep = spec.get("keep", [])
    if not (isinstance(keep, list) and all(is_absolute_token(v) for v in keep)):
        problems.append("keep: a list of absolute paths")
    elif forbidden_ok:
        problems += [f"keep: {k} lies under the forbidden root {root}" for k in keep for root in forbidden
                     if under(norm_key(k), norm_key(root))]
    rewrite = spec.get("rewrite", [])
    if not (isinstance(rewrite, list) and all(isinstance(r, dict) and set(r) == {"from", "to"}
                                              and is_absolute_token(r["from"]) and is_absolute_token(r["to"])
                                              for r in rewrite)):
        problems.append("rewrite: a list of {from, to} absolute path prefixes")
    elif len({norm_key(r["from"]) for r in rewrite}) != len(rewrite):
        problems.append("rewrite: two rules rewrite the same prefix")
    members = spec.get("members", [])
    if not isinstance(members, list):
        problems.append("members: a list")
    for i, m in enumerate(members if isinstance(members, list) else []):
        by_name = (isinstance(m, dict) and "member" in m and "original" not in m and safe_name(m["member"])
                   and isinstance(m.get("sha256"), str) and HEX64.match(m["sha256"]) is not None)
        by_original = (isinstance(m, dict) and "original" in m and "member" not in m and "sha256" not in m
                       and is_pin(m["original"]))
        if not ((by_name or by_original) and own_role(m.get("role")) and isinstance(m.get("remap", False), bool)
                and set(m) <= {"role", "member", "sha256", "original", "remap"}):
            problems.append(f"members[{i}]: {{role, member, sha256}} or {{role, original}}, with an optional remap "
                            "and a role custody does not derive")
    files = spec.get("files", [])
    if not isinstance(files, list):
        problems.append("files: a list")
    for i, f in enumerate(files if isinstance(files, list) else []):
        if not (is_pin(f) and set(f) == {"role", "path", "sha256"} and own_role(f["role"])
                and is_absolute_token(f["path"])):
            problems.append(f"files[{i}]: {{role, path, sha256}}, the path absolute and a role custody does not "
                            "derive")
    counts = spec.get("counts", {})
    if not (isinstance(counts, dict) and all(type(n) is int and n >= 0 for n in counts.values())):
        problems.append("counts: {role: count}")
    return problems


def seal_index(seal: dict, failures: list) -> dict | None:
    """The seal's members by name and by retained original path (aliases
    included, by norm_key), each original bound to its member's SHA-256."""
    files = {}
    for entry in seal.get("files") or []:
        name = entry.get("name") if isinstance(entry, dict) else None
        if not safe_name(name) or name in files or not HEX64.match(str(entry.get("sha256", ""))):
            failures.append(f"the seal lists an unsafe, repeated or unhashed member {name!r}")
            return None
        files[name] = entry
    by_original, originals_of = {}, {}
    for row in seal.get("original_archive_members") or []:
        original, name = row.get("original") or {}, row.get("member")
        if name not in files or original.get("sha256") != files[name]["sha256"] or not isinstance(
                original.get("path"), str):
            failures.append(f"the seal maps {original.get('path')!r} to {name!r} with another SHA-256")
            return None
        key = norm_key(original["path"])
        if key in by_original and by_original[key][0] != name:
            failures.append(f"the seal maps {original['path']} to two members")
            return None
        by_original[key] = (name, original["sha256"])
        originals_of.setdefault(name, []).append(original["path"])
    return {"files": files, "by_original": by_original, "originals_of": originals_of}


class Archive:
    """A durable EVIDENCE.tar, its headers read once; each wanted member is
    streamed to DIR/members/NAME with its size and SHA-256 checked against
    the seal's (a copy that does not match is removed)."""

    def __init__(self, path: Path, out_dir: Path, failures: list):
        self.out_dir, self.failures = Path(out_dir), failures
        self.tar = tarfile.open(path, mode="r:")
        self.entries, self.repeated = {}, set()
        for info in self.tar.getmembers():
            if info.name in self.entries:
                self.repeated.add(info.name)
            self.entries[info.name] = info

    def extract(self, name: str, want: dict) -> Path | None:
        info = self.entries.get(name)
        if info is None or not info.isfile() or name in self.repeated:
            self.failures.append(f"{name}: not one regular file in the archive")
            return None
        if info.size != want["bytes"]:
            self.failures.append(f"{name}: {info.size} bytes in the archive, not the seal's {want['bytes']}")
            return None
        target = self.out_dir / "members" / name
        digest = hashlib.sha256()
        source = self.tar.extractfile(info)
        try:
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open("xb") as handle:
                for block in iter(lambda: source.read(CHUNK), b""):
                    digest.update(block)
                    handle.write(block)
        except OSError as error:
            self.failures.append(f"{name}: cannot be written to {target} ({error})")
            return None
        if digest.hexdigest() != want["sha256"]:
            self.failures.append(f"{name}: extracted with SHA-256 {digest.hexdigest()}, not {want['sha256']}")
            target.unlink()
            return None
        return target

    def close(self):
        self.tar.close()


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for v in value:
            yield from strings(v)
    elif isinstance(value, dict):
        for k, v in value.items():
            yield from strings(k)
            yield from strings(v)


def names_root(text: str, root: str) -> bool:
    """Whether a string names a root: as its text (either slash, any case)
    or as one of its paths, normalized."""
    folded, key = text.replace("\\", "/").casefold(), norm_key(root)
    if key + "/" in folded or norm_key(text) == key:
        return True
    return any(is_absolute_token(p) and under(norm_key(p), key) for p in PATH_DELIMITERS.split(text))


def child_kind(kind: str, key, index, prefix) -> str:
    """The kind of the file a path inside a member of ``kind`` names: a job
    spec's (utility_queue.py's command.json) binary, source, record, bank
    spec, opponents' sources, case file and working directory; a model
    source's play-import descriptor; anything else an object."""
    if kind == "job":
        if key == "command":
            return "binary" if index == 0 else {"source": "source", "out": "record"}.get(prefix, "object")
        return {"BANK_SPEC": "bank_spec", "OPPONENTS": "source", "BUDGET_CASES": "cases",
                "cwd": "directory"}.get(key, "object")
    if kind == "source" and key == "play_import":
        return "import"
    return "object"


def job_inputs(value) -> dict | None:
    """A utility job spec's paths: its source= and out= and its BANK_SPEC
    and OPPONENTS' T1 and A48 (None when it is not a job spec)."""
    if not (isinstance(value, dict) and isinstance(value.get("command"), list)
            and isinstance(value.get("environment"), dict)):
        return None
    out = {}
    for item in value["command"][1:]:
        if isinstance(item, str):
            head, sep, rest = item.partition("=")
            if sep and head in ("source", "out"):
                out[head] = rest
    environment = value["environment"]
    if isinstance(environment.get("BANK_SPEC"), str):
        out["bank_spec"] = environment["BANK_SPEC"]
    for part in str(environment.get("OPPONENTS", "")).split(","):
        name, sep, path = part.partition("=")
        if sep and name in ("T1", "A48"):
            out[name] = path
    return out


class Remap:
    """The absolute paths inside remapped members. A path under a keep prefix
    stays as it is. Any other is rewritten by the spec's longest matching
    rewrite prefix, if one matches; a rewritten path under a keep prefix is
    kept as rewritten (so a path the seal does not hold can name SD1's own
    root, never read). Otherwise it resolves through the seal's member map
    (a member is extracted when first named, as the kind its reference gives
    it, and remapped in turn when that kind is remapped, so that its
    remapped copy is named), then the verified plain files, then a directory
    of extracted members. A pin's SHA-256 must be the sealed bytes' and
    becomes its new file's. A path that resolves nowhere, a kept path that a
    job reads (reads()), or a remapped copy whose final text still names a
    forbidden root, is a failure. Each change is recorded (from, to and the
    SHA-256 of the bytes it names). identity() gives a member's identity with
    paths aside."""

    def __init__(self, run):
        self.run, self.out_dir, self.failures = run, run.out_dir, run.failures
        self.index = run.index
        self.files = {norm_key(f["path"]): f for f in run.files if f["verified"]}
        rules = [(norm_key(r["from"]), norm_spelling(r["to"])) for r in run.spec.get("rewrite", [])]
        self.rewrite = sorted(rules, key=lambda rule: -len(rule[0].split("/")))
        self.keep = [norm_key(k) for k in run.spec.get("keep", [])]
        self.forbidden = run.spec["forbidden"]
        self.done, self.active, self.notes, self.parsed = {}, set(), {}, {}
        self.ids, self.id_active, self.refused = {}, set(), set()

    def rewritten(self, token: str) -> str | None:
        """``token`` after the longest rewrite prefix it lies under, or None."""
        spelling = norm_spelling(token)
        key = spelling.casefold()
        for head, to in self.rewrite:
            if under(key, head):
                return "/".join([to, *spelling.split("/")[len(head.split("/")):]])
        return None

    def kept(self, key: str) -> bool:
        return any(under(key, k) for k in self.keep)

    def reads(self, member: str, kind: str) -> bool:
        """Whether a job reads the file a path inside ``member`` names, as
        ``kind``: a job spec's inputs (JOB_READS), and every path inside an
        input (INPUT_KINDS), through each remapped member it names in turn."""
        own = self.run.members[member]["kind"]
        return own in INPUT_KINDS or (own == "job" and kind in JOB_READS)

    def locate(self, token: str, pinned: bool):
        """Where one absolute path resolves: ("kept", None) as it is,
        ("kept", PATH) as rewritten, ("member", NAME), ("file", ENTRY),
        ("directory", PATH) or None."""
        key = norm_key(token)
        if self.kept(key):
            return "kept", None
        moved = self.rewritten(token)
        if moved is not None:
            key = norm_key(moved)
            if self.kept(key):
                return "kept", moved
        hit = self.index["by_original"].get(key)
        if hit is not None:
            return "member", hit[0]
        plain = self.files.get(key)
        if plain is not None:
            return "file", plain
        directory = None if pinned else self.directory(key)
        return None if directory is None else ("directory", directory)

    def directory(self, key: str) -> str | None:
        prefixes = set()
        for name, extracted in self.run.extracted.items():
            if extracted is None:
                continue
            for original in self.index["originals_of"].get(name, []):
                original = norm_key(original)
                if original.startswith(key + "/"):
                    rest = original[len(key) + 1:]
                    prefixes.add(name[:-len(rest) - 1] if name.casefold().endswith("/" + rest) else None)
        if len(prefixes) == 1 and None not in prefixes:
            return absolute(self.out_dir / "members" / prefixes.pop())
        return None

    def target(self, member: str, token: str, pinned: bool, kind: str) -> dict | None:
        """Where one absolute path goes: {place, to, sealed, sha256} (to
        None when it stays as it is; sealed, the SHA-256 a pin of it must
        have, and sha256, its new file's, both None for a kept path or a
        directory), or None for a failure (listed)."""
        where = self.locate(token, pinned)
        if where is None:
            self.failures.append(f"{member}: {token} resolves to no sealed member, verified file or directory")
            return None
        place, value = where
        if place == "kept":
            self.notes[member]["kept"].append({"from": token, "to": value or token})
            if self.reads(member, kind) and (member, token) not in self.refused:
                self.refused.add((member, token))
                self.failures.append(f"{member}: {token} is kept" + (f" as {value}" if value else "")
                                     + ", but a job reads it (a keep holds only outputs, logs and working "
                                     "directories)")
            return {"place": place, "to": value, "sealed": None, "sha256": None}
        if place == "file":
            return {"place": place, "to": value["path"], "sealed": value["sha256"], "sha256": value["sha256"]}
        if place == "directory":
            return {"place": place, "to": value, "sealed": None, "sha256": None}
        if not self.run.include(value, kind, member):
            return None
        entry = self.run.members[value]
        if entry["remap"]:
            remapped = self.remapped(value)
            if remapped is None:
                self.failures.append(f"{member}: {token} names {value}, whose remapped copy failed")
                return None
            return {"place": place, "to": remapped["path"], "sealed": entry["sha256"], "sha256": remapped["sha256"]}
        return {"place": place, "to": absolute(self.run.extracted[value]), "sealed": entry["sha256"],
                "sha256": entry["sha256"]}

    def walk(self, member: str, value, kind: str, on_pin, on_token, key=None, index=None):
        """``value`` with each absolute path replaced: a pin's by on_pin
        (path, SHA-256) and any other by on_token (a string, or None to keep
        it), each told the kind its reference gives the file it names."""
        if isinstance(value, list):
            return [self.walk(member, v, kind, on_pin, on_token, key, i) for i, v in enumerate(value)]
        if isinstance(value, str):
            parts, changed = PATH_DELIMITERS.split(value), False
            for i in range(0, len(parts), 2):
                if is_absolute_token(parts[i]):
                    prefix = parts[i - 2] if i >= 2 and parts[i - 1] == "=" else None
                    new = on_token(member, parts[i], child_kind(kind, key, index, prefix))
                    if new is not None:
                        parts[i], changed = new, True
            return "".join(parts) if changed else value
        if not isinstance(value, dict):
            return value
        pinned = is_pin(value) and is_absolute_token(value["path"])
        new_pin = on_pin(member, value, child_kind(kind, key, index, None)) if pinned else None
        out = {}
        for k, item in value.items():
            if pinned and k in ("path", "sha256"):
                out[k] = new_pin[0] if k == "path" else new_pin[1]
            else:
                out[self.walk(member, k, kind, on_pin, on_token)] = self.walk(member, item, kind, on_pin, on_token, k)
        return out

    def remap_pin(self, member: str, value: dict, kind: str) -> tuple:
        found = self.target(member, value["path"], True, kind)
        if found is None or found["to"] is None:
            return value["path"], value["sha256"]
        if found["place"] == "kept":
            # Rewritten into a kept prefix: never read, so its SHA-256 stays.
            return found["to"], value["sha256"]
        if found["sealed"] != value["sha256"]:
            self.failures.append(f"{member}: the pin of {value['path']} has SHA-256 {value['sha256']}, "
                                 f"but the file it names has {found['sealed']}")
            return value["path"], value["sha256"]
        self.notes[member]["replaced"].append({"from": value["path"], "to": found["to"], "sha256": found["sha256"]})
        return found["to"], found["sha256"]

    def remap_token(self, member: str, token: str, kind: str):
        found = self.target(member, token, False, kind)
        if found is None or found["to"] is None:
            return None
        if found["place"] != "kept":
            self.notes[member]["replaced"].append({"from": token, "to": found["to"], "sha256": found["sha256"]})
        return found["to"]

    def values(self, name: str) -> list | None:
        """A remapped member's JSON values (one, or one per JSONL line), or
        None (listed once)."""
        if name not in self.parsed:
            lines, result = name.endswith(".jsonl"), None
            if not (lines or name.endswith(".json")):
                self.failures.append(f"{name}: only JSON and JSONL members are remapped")
            else:
                try:
                    text = self.run.extracted[name].read_bytes().decode("utf-8")
                    result = [json.loads(x) for x in text.splitlines() if x.strip()] if lines else [json.loads(text)]
                except (ValueError, UnicodeDecodeError) as error:
                    self.failures.append(f"{name}: not readable JSON ({error})")
            self.parsed[name] = result
        return self.parsed[name]

    def remapped(self, name: str) -> dict | None:
        """The remapped copy of one member, DIR/remapped/NAME (the extracted
        bytes when nothing in it changes), with its pin."""
        if name in self.done:
            return self.done[name]
        if name in self.active:
            self.failures.append(f"{name}: its pins form a cycle")
            return None
        self.active.add(name)
        self.notes[name] = {"replaced": [], "kept": []}
        result = None
        try:
            values = self.values(name)
            if values is not None:
                kind = self.run.members[name]["kind"]
                new = [self.walk(name, v, kind, self.remap_pin, self.remap_token) for v in values]
                for root in self.forbidden:
                    if any(names_root(s, root) for s in strings(new)):
                        self.failures.append(f"{name}: its remapped copy still names {root}")
                target = self.out_dir / "remapped" / name
                target.parent.mkdir(parents=True, exist_ok=True)
                if canonical(new) != canonical(values):
                    body = ("".join(json.dumps(v, separators=(",", ":")) + "\n" for v in new)
                            if name.endswith(".jsonl") else json.dumps(new[0], separators=(",", ":")) + "\n")
                    target.write_bytes(body.encode("utf-8"))
                else:
                    target.write_bytes(self.run.extracted[name].read_bytes())
                result = {"path": absolute(target), "sha256": hashlib.sha256(target.read_bytes()).hexdigest()}
        finally:
            self.active.discard(name)
        self.done[name] = result
        return result

    def identity(self, name: str) -> str | None:
        """A member's identity with paths aside: its sealed SHA-256 unless it
        is remapped; a remapped member's is the SHA-256 of its JSON with each
        pin's path set aside and its SHA-256 replaced by the identity of the
        file it names, and each other path by that file's identity, in turn.
        So two hosts' copies of one input, each spelling its own bundle's
        paths, have one identity when they say the same and name the same
        bytes (checkpoints, import descriptors and the objects those pin)."""
        entry = self.run.members.get(name)
        if entry is None:
            return None
        if not entry["remap"]:
            return entry["sha256"]
        if name in self.ids:
            return self.ids[name]
        if name in self.id_active:
            return None
        self.id_active.add(name)
        result = None
        try:
            values = self.values(name)
            if values is not None:
                stripped = [self.walk(name, v, entry["kind"], self.identity_pin, self.identity_token) for v in values]
                result = hashlib.sha256(canonical(stripped).encode("utf-8")).hexdigest()
        finally:
            self.id_active.discard(name)
        self.ids[name] = result
        return result

    def located_identity(self, token: str, pinned: bool):
        where = self.locate(token, pinned)
        if where is None:
            return None
        place, value = where
        if place == "member":
            return self.identity(value)
        if place == "file":
            return value["sha256"]
        return "directory" if place == "directory" else f"kept {value or token}"

    def identity_pin(self, member: str, value: dict, kind: str) -> tuple:
        return None, self.located_identity(value["path"], True)

    def identity_token(self, member: str, token: str, kind: str) -> str:
        return f"<{self.located_identity(token, False)}>"


class Custody:
    """One custody run (custody())."""

    def __init__(self, spec: dict, out_dir: Path, roots: Roots):
        self.spec, self.out_dir, self.roots = spec, out_dir, roots
        self.failures = []
        self.copies, self.rule = [], None
        self.seal = self.index = self.archive = self.archive_pin = self.remap = None
        self.files, self.file_keys = [], set()
        self.members, self.extracted = {}, {}
        self.cases, self.inputs = {}, {}
        self.index_value = self.index_record = self.analysis = self.answer = None

    # ----------------------------------------------------- plain files

    def plain(self, role: str, entry: dict, read: bool = False, **extra):
        """One plain file read in place (verify_file); with ``read``, its
        bytes from that one read, or None."""
        record = {"role": role, "path": entry.get("path"), "sha256": entry.get("sha256"), **extra}
        key = norm_key(entry.get("path", ""))
        if key in self.file_keys:
            self.failures.append(f"{entry.get('path')} is listed twice")
            return None
        self.file_keys.add(key)
        data = None
        if read:
            data = read_verified(entry, f"{role} file", self.failures, self.roots)
            record["verified"] = data is not None
        else:
            record["verified"] = verify_file(entry, f"{role} file", self.failures, self.roots)
        self.files.append(record)
        return data

    def check_schedule(self):
        """The utility screen's SCHEDULE.json and the frozen formal schedule
        files beside it, each against its files pin."""
        entry = self.spec["schedule"]
        value = parse_json(self.plain("schedule", entry, read=True), "SCHEDULE.json", self.failures)
        if value is None:
            return
        pins = value.get("files") if isinstance(value, dict) else None
        for lineage in LINEAGES:
            name = f"formal-{lineage}.jsonl"
            want = pins.get(name) if isinstance(pins, dict) else None
            if not (isinstance(want, dict) and isinstance(want.get("sha256"), str)):
                self.failures.append(f"SCHEDULE.json pins no {name}")
                continue
            data = self.plain(f"{lineage}-schedule", {"path": beside(entry["path"], name), "sha256": want["sha256"]},
                              read=True)
            if data is not None:
                try:
                    self.cases[lineage] = [json.loads(x)["id"] for x in data.decode("utf-8").splitlines() if x.strip()]
                except (ValueError, UnicodeDecodeError, KeyError, TypeError) as error:
                    self.failures.append(f"{name}: not a schedule ({error!r})")

    def check_known_answer(self):
        """BT1's JOBS.json and CASE-PINS.json read in place; each archive
        record JOBS.json names against its record_sha256, with its id, the
        BUDGET inputs CASE-PINS.json pins (relative to budget_root) and the
        archive's schedule files against theirs. JOBS.json's games are the
        games CASE-PINS.json checked, each in its lineage's schedule."""
        answer = self.spec["known_answer"]
        jobs = parse_json(self.plain("known-answer", answer["jobs"], read=True), "JOBS.json", self.failures)
        pins = parse_json(self.plain("known-answer", answer["case_pins"], read=True), "CASE-PINS.json",
                          self.failures)
        root = answer["budget_root"].rstrip("/\\")
        ids = {lineage: [] for lineage in LINEAGES}
        if jobs is not None:
            listed = jobs.get("jobs") if isinstance(jobs, dict) else None
            if not isinstance(listed, list):
                self.failures.append("JOBS.json lists no jobs")
                listed = []
            for i, job in enumerate(listed):
                if not (isinstance(job, dict) and job.get("lineage") in LINEAGES and isinstance(job.get("id"), str)
                        and isinstance(job.get("record"), str) and HEX64.match(str(job.get("record_sha256", "")))
                        and job["id"] not in ids[job["lineage"]]):
                    self.failures.append(f"JOBS.json jobs[{i}]: not a job (lineage, id, record and record_sha256) "
                                         "or a repeated id")
                    continue
                ids[job["lineage"]].append(job["id"])
                self.plain(f"{job['lineage']}-archive", {"path": job["record"], "sha256": job["record_sha256"]},
                           id=job["id"])
        if pins is not None:
            inputs = pins.get("input_pins") if isinstance(pins, dict) else None
            cases = pins.get("case_files") if isinstance(pins, dict) else None
            if not (isinstance(pins, dict) and pins.get("passed") is True and isinstance(inputs, dict)
                    and isinstance(cases, dict) and set(cases) == set(LINEAGES)):
                self.failures.append("CASE-PINS.json is not a passed case check with input_pins and both lineages' "
                                     "case_files")
            else:
                games = sum(len(v) for v in ids.values())
                if jobs is not None and pins.get("games_checked") != games:
                    self.failures.append(f"CASE-PINS.json checked {pins.get('games_checked')} games, not JOBS.json's "
                                         f"{games}")
                for rel, sha in sorted(inputs.items()):
                    if not safe_name(rel):
                        self.failures.append(f"CASE-PINS.json: {rel!r} is not a path under the budget root")
                        continue
                    self.plain("budget-inputs", {"path": f"{root}/{rel}", "sha256": sha})
                for lineage in LINEAGES:
                    data = self.plain(f"{lineage}-archive-schedule",
                                      {"path": f"{root}/schedule/archive-{lineage}.jsonl", "sha256": cases[lineage]},
                                      read=True)
                    if data is None or jobs is None:
                        continue
                    try:
                        scheduled = {json.loads(x)["id"] for x in data.decode("utf-8").splitlines() if x.strip()}
                    except (ValueError, UnicodeDecodeError, KeyError, TypeError) as error:
                        self.failures.append(f"archive-{lineage}.jsonl: not a schedule ({error!r})")
                        continue
                    stray = sorted(set(ids[lineage]) - scheduled)
                    if stray:
                        self.failures.append(f"{lineage}: JOBS.json names {len(stray)} games outside the archive's "
                                             f"schedule, first {stray[0]}")
        self.answer = {"jobs": answer["jobs"], "case_pins": answer["case_pins"], "budget_root": answer["budget_root"],
                       "games": {lineage: len(v) for lineage, v in ids.items()}}

    # ---------------------------------------------------------- the seal

    def check_durable(self):
        """Both durable copies, each copy's seal, archive and completion
        record side by side, under the seal's completion rule; members are
        extracted from the first copy whose seal and archive verify."""
        copies = []
        for number, copy in enumerate(self.spec["durable"], 1):
            what = f"durable copy {number}"
            seal = parse_json(read_verified(copy["seal"], f"{what}: the source seal", self.failures, self.roots),
                              f"{what}: the source seal", self.failures)
            if seal is not None and not (isinstance(seal, dict) and seal.get("schema") == SEAL_SCHEMA
                                         and seal.get("complete") is True):
                self.failures.append(f"{what}: the source seal is not a complete search-player-utility source seal")
                seal = None
            done = parse_json(read_verified(copy["completion"], f"{what}: the custody completion", self.failures,
                                            self.roots), f"{what}: the custody completion", self.failures)
            if done is not None and not (isinstance(done, dict) and done.get("schema") == COMPLETION_SCHEMA
                                         and done.get("complete") is True):
                self.failures.append(f"{what}: the custody completion is not complete")
                done = None
            side_by_side = len({parent_key(copy[k]["path"]) for k in ("seal", "archive", "completion")}) == 1
            if not side_by_side:
                self.failures.append(f"{what}: its seal, archive and completion record are not side by side")
            elif seal is not None and (norm_key(copy["completion"]["path"]).rsplit("/", 1)[-1]
                                       != norm_key(str(seal.get("completion_record", "")))):
                self.failures.append(f"{what}: its completion record is not the seal's "
                                     f"{seal.get('completion_record')!r}")
            copies.append({"copy": copy, "seal": seal, "completion": done, "side_by_side": side_by_side,
                           "listed": False, "archive": False})
        self.copies = copies

        def pairs(entries) -> set:
            return {(norm_key(e.get("path", "")), e.get("sha256")) for e in entries or [] if isinstance(e, dict)}
        given_seals = pairs(c["copy"]["seal"] for c in copies)
        given_archives = pairs(c["copy"]["archive"] for c in copies)
        rule = []
        seals = [c["seal"] for c in copies]
        if None in seals:
            rule.append("a SOURCE-SEAL copy is not complete")
        else:
            if len({c["copy"]["seal"]["sha256"] for c in copies}) != 1:
                rule.append("the two SOURCE-SEAL copies differ")
            if seals[0].get("completion_rule") != COMPLETION_RULE:
                rule.append("the seal states another completion rule than the one custody checks")
            if pairs(seals[0].get("archives")) != given_archives:
                rule.append("the seal's durable archives are not the two archives given")
        dones = [c["completion"] for c in copies]
        if None in dones:
            rule.append("a completion record is not complete")
        else:
            if len({c["copy"]["completion"]["sha256"] for c in copies}) != 1:
                rule.append("the two completion records differ")
            if pairs(dones[0].get("source_seals")) != given_seals:
                rule.append("the completion record's source_seals are not both SOURCE-SEAL copies")
            if pairs(dones[0].get("archives")) != given_archives:
                rule.append("the completion record's archives are not the two archives given")
        self.failures += [f"the seal's completion rule: {r}" for r in rule]
        self.rule = {"rule": COMPLETION_RULE, "failures": rule, "holds": not rule}
        self.seal = next((s for s in seals if s is not None), None)
        if self.seal is not None:
            self.index = seal_index(self.seal, self.failures)
        for number, c in enumerate(copies, 1):
            if c["seal"] is None:
                continue
            c["listed"] = pairs([c["copy"]["archive"]]) <= pairs(c["seal"].get("archives"))
            if not c["listed"]:
                self.failures.append(f"durable copy {number}: {c['copy']['archive']['path']} with this SHA-256 is not "
                                     "one of the seal's durable archives")
            else:
                c["archive"] = verify_file(c["copy"]["archive"], f"durable copy {number}: the archive", self.failures,
                                           self.roots)
        chosen = next((c for c in copies if c["seal"] is not None and c["archive"] and c["side_by_side"]), None)
        if self.index is not None and chosen is not None:
            try:
                self.archive = Archive(Path(chosen["copy"]["archive"]["path"]), self.out_dir, self.failures)
                self.archive_pin = chosen["copy"]["archive"]
            except (tarfile.TarError, OSError) as error:
                self.failures.append(f"{chosen['copy']['archive']['path']}: not a readable tar archive ({error})")

    # ------------------------------------------------------------ members

    def sealed(self, entry: dict, what: str) -> str | None:
        """The member the seal maps an original path to, with the pin's
        SHA-256; None (listed) when it maps none."""
        hit = self.index["by_original"].get(norm_key(entry["path"]))
        if hit is None or hit[1] != entry["sha256"]:
            self.failures.append(f"{what}: the seal maps no member with SHA-256 {entry['sha256']} to {entry['path']}")
            return None
        return hit[0]

    def register(self, name: str, role: str, kind: str, what: str, **extra) -> bool:
        """A sealed member SD1 reads, by its role, extracted."""
        if name in self.members:
            self.failures.append(f"{what}: {name} is listed twice")
            return False
        sealed = self.index["files"][name]
        self.members[name] = {"role": role, "kind": kind, "remap": kind in REMAPPED_KINDS,
                              "sha256": sealed["sha256"], "bytes": sealed.get("bytes"), **extra}
        return self.extract(name)

    def include(self, name: str, kind: str, referrer: str) -> bool:
        """A sealed member a remapped member names, read as the kind that
        reference gives it (role inputs when first named), extracted."""
        entry = self.members.get(name)
        if entry is None:
            sealed = self.index["files"][name]
            self.members[name] = {"role": "inputs", "kind": kind, "remap": kind in REMAPPED_KINDS,
                                  "sha256": sealed["sha256"], "bytes": sealed.get("bytes")}
        elif entry["remap"] != (kind in REMAPPED_KINDS):
            self.failures.append(f"{referrer}: names {name} as a {kind}, which custody reads as a {entry['kind']}")
            return False
        return self.extract(name)

    def extract(self, name: str) -> bool:
        if name not in self.extracted:
            entry = self.members[name]
            self.extracted[name] = self.archive.extract(name, {"sha256": entry["sha256"], "bytes": entry["bytes"]})
        return self.extracted[name] is not None

    def member_json(self, name: str, what: str):
        try:
            return json.loads(self.extracted[name].read_bytes())
        except (ValueError, UnicodeDecodeError, OSError) as error:
            self.failures.append(f"{what}: not readable JSON ({error})")
            return None

    def check_index(self):
        """The formal analysis index (by its original path) and, from its
        search-arm entries, each case's record (its row pin), holding one
        budget_game row of its case, and the job spec beside it; the records
        are exactly the frozen schedule's cases."""
        name = self.sealed(self.spec["index"], "the formal analysis index")
        if name is None or not self.register(name, "index", "object", "the formal analysis index"):
            return
        index = self.member_json(name, "the formal analysis index")
        if not (isinstance(index, dict) and index.get("schema") == INDEX_SCHEMA
                and isinstance(index.get("entries"), list)):
            self.failures.append("the formal analysis index is not a search-player-utility analysis index")
            return
        self.index_value = index
        if (index.get("schedule") or {}).get("sha256") != self.spec["schedule"]["sha256"]:
            self.failures.append("the index's frozen schedule is not the SCHEDULE.json given")
        found = {lineage: [] for lineage in LINEAGES}
        for entry in index["entries"]:
            eid = entry.get("id") if isinstance(entry, dict) else None
            if not (isinstance(eid, str) and eid.endswith(SEARCH_ARM)):
                continue
            case = eid[:-len(SEARCH_ARM)]
            lineage = next((lin for lin in LINEAGES if case.startswith(f"formal-{lin}-")), None)
            row = entry.get("row")
            if lineage is None or not is_pin(row):
                self.failures.append(f"{eid}: the index names no search-arm record of a lineage")
                continue
            found[lineage].append(case)
            record = self.sealed(row, f"{eid}'s record")
            if record is None or not self.register(record, f"{lineage}-records", "record", f"{eid}'s record",
                                                   id=case):
                continue
            try:
                rows = [json.loads(x) for x in self.extracted[record].read_bytes().decode("utf-8").splitlines()
                        if x.strip()]
            except (ValueError, UnicodeDecodeError):
                rows = None
            if not (rows and len(rows) == 1 and isinstance(rows[0], dict) and rows[0].get("kind") == "budget_game"
                    and rows[0].get("id") == case):
                self.failures.append(f"{record}: not one budget_game row of {case}")
            job = beside(row["path"], JOB_SPEC)
            hit = self.index["by_original"].get(norm_key(job))
            if hit is None:
                self.failures.append(f"{eid}: its job spec {job} is not sealed")
                continue
            self.register(hit[0], f"{lineage}-jobs", "job", f"{eid}'s job spec", id=case, record=record)
        for lineage in LINEAGES:
            if lineage in self.cases and sorted(found[lineage]) != sorted(self.cases[lineage]):
                self.failures.append(f"{lineage}: the index's {len(found[lineage])} search-arm records are not exactly "
                                     f"the frozen schedule's {len(self.cases[lineage])} cases")
        self.index_record = {"member": name, "sha256": self.spec["index"]["sha256"], "entries": len(index["entries"]),
                             "search_records": {lineage: len(v) for lineage, v in found.items()}}

    def check_analysis_sources(self):
        """ANALYSIS-SOURCES-001 (by its original path) and the index bind each
        other, one way or the other: the index pins it as its source_spec
        ("index-pins-sources", as the utility screen's analysis_index.py
        builds the index from that input spec), or else it holds the index's
        SHA-256 ("sources-pin-index"). Either way its allocation pin has the
        SHA-256 of the allocation the index pins, which is extracted too."""
        name = self.sealed(self.spec["analysis_sources"], "ANALYSIS-SOURCES-001")
        if name is None or not self.register(name, "analysis-sources", "object", "ANALYSIS-SOURCES-001"):
            return
        value = self.member_json(name, "ANALYSIS-SOURCES-001")
        sealed_sha256, index_sha256 = self.members[name]["sha256"], self.spec["index"]["sha256"]
        index = self.index_value or {}
        source_spec = index.get("source_spec")
        if is_pin(source_spec) and source_spec["sha256"] == sealed_sha256:
            direction = "index-pins-sources"
        elif index_sha256 in {s for s in strings(value) if HEX64.match(s)}:
            direction = "sources-pin-index"
        else:
            direction = None
            self.failures.append(f"ANALYSIS-SOURCES-001 ({sealed_sha256}) and the index ({index_sha256}) do not "
                                 "bind each other: the index's source_spec does not pin it, and it does not hold "
                                 "the index's SHA-256")
        binds = direction is not None
        allocation = index.get("allocation")
        allocation_sha256 = allocation["sha256"] if is_pin(allocation) else None
        own = value.get("allocation") if isinstance(value, dict) else None
        own_sha256 = own["sha256"] if is_pin(own) else None
        if allocation_sha256 is None:
            binds = False
            self.failures.append("the index pins no allocation")
        else:
            if own_sha256 != allocation_sha256:
                binds = False
                self.failures.append(f"ANALYSIS-SOURCES-001's allocation is {own_sha256}, not the index's "
                                     f"allocation {allocation_sha256}")
            allocation_name = self.sealed(allocation, "the allocation the index pins")
            if allocation_name is not None:
                self.register(allocation_name, "allocation", "object", "the allocation the index pins")
        self.analysis = {"member": name, "sha256": sealed_sha256,
                         "schema": value.get("schema") if isinstance(value, dict) else None,
                         "index_sha256": index_sha256, "allocation_sha256": allocation_sha256,
                         "direction": direction, "binds": binds}

    def explicit_members(self):
        for i, m in enumerate(self.spec.get("members", [])):
            what = f"members[{i}] ({m['role']})"
            if "original" in m:
                name = self.sealed(m["original"], what)
                if name is None:
                    continue
            else:
                name, sealed = m["member"], self.index["files"].get(m["member"])
                if sealed is None or sealed["sha256"] != m["sha256"]:
                    self.failures.append(f"{what}: the seal lists no member {name} with SHA-256 {m['sha256']}")
                    continue
            self.register(name, m["role"], "explicit" if m.get("remap") else "object", what)

    def copy_pin(self, name: str) -> dict | None:
        entry = self.members[name]
        if entry["remap"]:
            return self.remap.done.get(name)
        extracted = self.extracted.get(name)
        return None if extracted is None else {"path": absolute(extracted), "sha256": entry["sha256"]}

    def lineage_inputs(self):
        """Within a lineage each job spec's out= names its own record, and
        its jobs name one source, bank spec and pair of opponents by
        identity (Remap.identity: the same content naming the same bytes,
        paths aside, since each host bundle's copies spell its own paths),
        the raw SHA-256s beside; SD1's input set for the lineage is the first
        job's copies of them (index order)."""
        for lineage in LINEAGES:
            jobs = [n for n, m in self.members.items() if m["role"] == f"{lineage}-jobs"]
            if not jobs and self.index_value is not None:
                self.failures.append(f"{lineage}: custody holds no job spec")
            seen, raw, chosen = {f: {} for f in INPUT_LISTS}, {f: set() for f in INPUT_LISTS}, {}
            for name in jobs:
                if self.extracted.get(name) is None:
                    continue
                refs = job_inputs(self.member_json(name, name))
                if refs is None:
                    self.failures.append(f"{name}: not a utility job spec (command and environment)")
                    continue
                out = refs.get("out")
                if not (is_absolute_token(out) and self.remap.locate(out, True)
                        == ("member", self.members[name]["record"])):
                    self.failures.append(f"{name}: its out= {out!r} is not its own record "
                                         f"{self.members[name]['record']}")
                for field in INPUT_LISTS:
                    token = refs.get(field)
                    where = self.remap.locate(token, True) if is_absolute_token(token) else None
                    if where is None or where[0] not in ("member", "file") or (
                            where[0] == "member" and where[1] not in self.members):
                        self.failures.append(f"{name}: its {field} input {token!r} resolves to no sealed member or "
                                             "verified file")
                        continue
                    if where[0] == "member":
                        identity, sha256 = self.remap.identity(where[1]), self.members[where[1]]["sha256"]
                        copy = self.copy_pin(where[1])
                    else:
                        identity = sha256 = where[1]["sha256"]
                        copy = {"path": where[1]["path"], "sha256": sha256}
                    seen[field].setdefault(str(identity), []).append(name)
                    raw[field].add(sha256)
                    chosen.setdefault(field, copy)
            one = bool(jobs) and all(len(seen[f]) == 1 and chosen.get(f) is not None for f in INPUT_LISTS)
            for field in INPUT_LISTS:
                if len(seen[field]) > 1:
                    self.failures.append(f"{lineage}: its jobs name {len(seen[field])} different {field} inputs, "
                                         "paths aside, not one")
            self.inputs[lineage] = {"jobs": len(jobs), "one_input_set": one,
                                    "identity_sha256": {f: sorted(seen[f]) for f in INPUT_LISTS},
                                    "raw_sha256": {f: sorted(raw[f]) for f in INPUT_LISTS},
                                    "inputs": chosen if one else None}

    # ------------------------------------------------------------- run

    def execute(self):
        self.check_durable()
        self.check_schedule()
        self.check_known_answer()
        for entry in self.spec.get("files", []):
            self.plain(entry["role"], {"path": entry["path"], "sha256": entry["sha256"]})
        if self.index is None or self.archive is None:
            self.failures.append("no member is extracted: the seal or the archive did not verify")
            return
        self.check_index()
        self.check_analysis_sources()
        self.explicit_members()
        self.remap = Remap(self)
        for name, entry in list(self.members.items()):
            if entry["remap"] and self.extracted.get(name) is not None:
                self.remap.remapped(name)
        self.lineage_inputs()

    def close(self):
        if self.archive is not None:
            self.archive.close()

    def record(self, spec_pin: dict | None) -> dict:
        members, lists = [], {}
        for name, m in self.members.items():
            got = self.extracted.get(name)
            entry = {"role": m["role"], "member": name, "kind": m["kind"], "sha256": m["sha256"],
                     "originals": self.index["originals_of"].get(name, []),
                     "extracted": None if got is None else {"path": absolute(got), "sha256": m["sha256"]}}
            entry.update({k: m[k] for k in ("id", "record") if k in m})
            if m["remap"]:
                notes = (self.remap.notes if self.remap else {}).get(name, {"replaced": [], "kept": []})
                entry.update(remapped=self.remap.done.get(name) if self.remap else None, **notes)
            members.append(entry)
            listed = entry.get("remapped") or entry["extracted"]
            if listed is not None:
                lists.setdefault(m["role"], []).append(listed["path"])
        for f in self.files:
            if f["verified"]:
                lists.setdefault(f["role"], []).append(f["path"])
        for lineage, held in self.inputs.items():
            for field, copy in (held["inputs"] or {}).items():
                lists[f"{lineage}-{INPUT_LISTS[field]}"] = [copy["path"]]
        roles = Counter(m["role"] for m in self.members.values())
        roles.update(f["role"] for f in self.files)
        for role, n in self.spec.get("counts", {}).items():
            if roles[role] != n:
                self.failures.append(f"role {role}: custody holds {roles[role]} members and files, not {n}")
        list_pins = {}
        for role, paths in lists.items():
            target = self.out_dir / "lists" / f"{role}.txt"
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text("".join(p + "\n" for p in paths), encoding="utf-8", newline="\n")
            list_pins[role] = pin(target)
        durable = [{"seal": {**c["copy"]["seal"], "complete": c["seal"] is not None},
                    "archive": {**c["copy"]["archive"], "listed": c["listed"], "verified": c["archive"]},
                    "completion": {**c["copy"]["completion"], "complete": c["completion"] is not None},
                    "side_by_side": c["side_by_side"]} for c in self.copies]
        return {"schema": CUSTODY_SCHEMA, "spec": spec_pin, "out_dir": absolute(self.out_dir), "durable": durable,
                "completion_rule": self.rule, "extracted_from": self.archive_pin, "forbidden": self.spec["forbidden"],
                "schedule": self.spec["schedule"], "index": self.index_record, "analysis_sources": self.analysis,
                "known_answer": self.answer, "inputs": self.inputs, "members": members, "files": self.files,
                "lists": list_pins, "counts": dict(sorted(roles.items())), "failures": self.failures,
                "pass": not self.failures}


def custody(spec: dict, out_dir: Path, spec_pin: dict | None = None) -> dict:
    """Stage 0's custody of SD1's inputs. SPEC (schema sd1-v1/custody-spec):
      durable     [{seal, archive, completion}, {...}]: PINs of both durable
                  copies of the utility screen's SOURCE-SEAL.json,
                  EVIDENCE.tar and CUSTODY-COMPLETION.json, each copy's three
                  side by side. The seal's completion rule must hold: both
                  seals complete and identical, both completion records
                  identical and complete, their source_seals both seals and
                  their archives the seal's two. Members are extracted from
                  the first copy whose seal and archive verify
      forbidden   the hot and holding roots (nonempty): no file under them is
                  read, by its spelling, real path or file identity, and no
                  file is read through a link
      index       PIN of the utility screen's formal INDEX.json by its
                  original path, as the seal's member map knows it (looked
                  up, never read in place). Its search-arm entries give each
                  case's record (role LIN-records) and the job spec beside it
                  (LIN-jobs, remapped); the records are exactly the frozen
                  schedule's cases
      analysis_sources  PIN of ANALYSIS-SOURCES-001.json by its original
                  path: the utility screen's analysis input spec (schema
                  search-player-utility-analysis-sources/v1), which the index
                  pins as its source_spec (direction index-pins-sources); a
                  file that instead holds the index's SHA-256 binds too
                  (sources-pin-index). Either way its allocation pin must
                  have the SHA-256 of the index's allocation pin (extracted as
                  allocation); the record gives the direction beside binds
      schedule    PIN of the utility screen's SCHEDULE.json read in place: the
                  index pins it, and formal-LIN.jsonl beside it must have its
                  files pins (roles schedule and LIN-schedule)
      known_answer  {jobs, case_pins, budget_root}: PINs of BT1's JOBS.json and
                  CASE-PINS.json read in place (role known-answer); each
                  archive record JOBS.json names against its record_sha256
                  (LIN-archive, with its id), the BUDGET inputs CASE-PINS.json
                  pins under budget_root (budget-inputs) and the archive's
                  schedule files (LIN-archive-schedule)
      rewrite     optional [{from, to}] absolute prefixes; the longest that
                  matches a path is applied, once (for example computehost's
                  runtime root to that bundle's path on the maintainer's
                  desktop, as the seal knows it, or a prefix under a hot root
                  that the seal does not hold to the same path in SD1's own
                  root, with a keep prefix there)
      keep        optional absolute prefixes, none under a forbidden root: a
                  path under one, as written or as rewritten, is kept (never
                  resolved or read). Only a job's outputs, logs and working
                  directory may be: a kept path among a job spec's inputs (its
                  binary, source, record, bank spec, opponents and case file),
                  or anywhere inside an input it names, through each remapped
                  member in turn, is a failure
      members     optional further sealed members [{role, member, sha256} or
                  {role, original: PIN}, optional remap: true]
      files       optional [{role, path, sha256}]: further plain files read
                  in place
      counts      optional {role: n}: exactly n members and files have role
    A spec's own members and files may not take a role custody derives
    (DERIVED_ROLES). Every path inside a job spec and the inputs it names is
    resolved (class Remap), each sealed file it names extracted in turn
    (role inputs). Within a lineage each job's out= is its record and the
    jobs name one source, bank spec and pair of opponents by identity,
    paths aside; SD1's input set is the first job's copies, listed as
    LIN-source, LIN-bank-spec, LIN-t1 and LIN-a48. Every gap and mismatch is
    listed in the record and fails it; DIR/members holds the extracted
    members, DIR/remapped the remapped copies, and DIR/lists/ROLE.txt each
    role's files (remapped copies where remapped)."""
    problems = custody_spec_problems(spec)
    require(not problems, "the custody spec is malformed: " + "; ".join(problems))
    roots = Roots(spec["forbidden"])
    for prefix in spec.get("keep", []):
        root = roots.containing(prefix)
        require(root is None, f"the custody spec is malformed: keep: {prefix} lies under the forbidden root {root}")
    out_dir = Path(out_dir)
    problem = path_problem(str(out_dir), roots)
    require(problem is None, f"the output directory: {problem}")
    require(not out_dir.exists() or (out_dir.is_dir() and not any(out_dir.iterdir())),
            f"{out_dir} is not a fresh directory")
    out_dir.mkdir(parents=True, exist_ok=True)
    run = Custody(spec, out_dir, roots)
    try:
        run.execute()
    finally:
        run.close()
    return run.record(spec_pin)


# ------------------------------------------------ stage 0: parent identity

PARENTS_SPEC_SCHEMA = "sd1-v1/parents-spec"
PARENTS_SCHEMA = "sd1-v1/parents"
# The fixed references' checkpoint files (T1 is g115).
T1_CHECKPOINT_SHA256 = "88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1"
A48_CHECKPOINT_SHA256 = "beb86b4116c6eaa8406f12eb391adde1a637e063375740ea0b9812251bcff4a4"
REFERENCE_CHECKPOINTS = {"T1": T1_CHECKPOINT_SHA256, "A48": A48_CHECKPOINT_SHA256}
PARENT_SOURCES = ("parent", *REFERENCE_CHECKPOINTS)


def located_source(entry, what: str, failures: list, roots: Roots, read: list) -> dict | None:
    """The model source a parents-spec entry names: {path, optional sha256,
    optional field}, with field the keys leading to the source inside the
    file (["initial_source"] in O's recorded config). The file is read once
    under custody's path checks and pinned in ``read``."""
    if not (isinstance(entry, dict) and isinstance(entry.get("path"), str)
            and set(entry) <= {"path", "sha256", "field"} and isinstance(entry.get("field", []), list)):
        failures.append(f"{what}: give {{path, optional sha256, optional field}}")
        return None
    data = read_verified({k: entry[k] for k in ("path", "sha256") if k in entry}, what, failures, roots)
    if data is None:
        return None
    read.append({"path": entry["path"], "sha256": hashlib.sha256(data).hexdigest()})
    value = parse_json(data, what, failures)
    if value is None:
        return None
    for key in entry.get("field", []):
        try:
            value = value[key]
        except (KeyError, IndexError, TypeError):
            failures.append(f"{what}: {entry['path']} has no field {entry['field']}")
            return None
    if not (isinstance(value, dict) and is_pin(value.get("checkpoint")) and is_pin(value.get("play_import"))):
        failures.append(f"{what}: not a model source with checkpoint and play_import pins")
        return None
    return value


def source_identity(source: dict, what: str, failures: list, roots: Roots, read: list) -> dict:
    """A model source with its paths aside, each pinned file checked in place
    under custody's path checks and pinned in ``read``: the checkpoint by its
    SHA-256, the play-import descriptor by its content (whose own pins
    compare by SHA-256), since the utility screen rewrote the descriptor's
    paths when it staged its inputs."""
    identity = unpathed(source)
    if verify_file(source["checkpoint"], f"{what} checkpoint", failures, roots):
        read.append({"path": source["checkpoint"]["path"], "sha256": source["checkpoint"]["sha256"]})
    data = read_verified(source["play_import"], f"{what} play_import", failures, roots)
    if data is not None:
        read.append({"path": source["play_import"]["path"], "sha256": source["play_import"]["sha256"]})
        try:
            content = json.loads(data)
        except (ValueError, UnicodeDecodeError):
            content = None
        if isinstance(content, dict):
            for key, ref in content.items():
                if is_pin(ref) and verify_file(ref, f"{what} play import {key}", failures, roots):
                    read.append({"path": ref["path"], "sha256": ref["sha256"]})
            identity["play_import"] = {"content": unpathed(content)}
    return identity


def parents(spec: dict, forbidden: list) -> dict:
    """Section 3's parent identity (stage 0). SPEC (schema sd1-v1/parents-spec):
    {"r1": {"parent": {"utility": SRC, "screen": SRC}, "T1": {"utility": SRC
    [, "screen": SRC]}, "A48": {...}}, "r2": {...}}, SRC as located_source:
    each lineage's own copies of its parent and of T1 and A48, from the
    utility screen's inputs and the distillation screen's. ``forbidden`` is
    custody's forbidden roots: no file read may lie under one or be reached
    through a link. Every source's checkpoint is its recorded pin and hashes
    to it; each utility-screen source is the distillation screen's, paths
    aside (T1's and A48's when the screen's are given). Each side's row pins
    every file it read."""
    require(isinstance(spec, dict) and spec.get("schema") == PARENTS_SPEC_SCHEMA
            and set(spec) == {"schema", *LINEAGES}, f"not an {PARENTS_SPEC_SCHEMA} spec naming r1 and r2")
    require(isinstance(forbidden, list) and bool(forbidden), "parents reads custody's forbidden roots")
    roots = Roots(forbidden)
    failures, out = [], {}
    for lineage in LINEAGES:
        require(isinstance(spec[lineage], dict) and set(spec[lineage]) == set(PARENT_SOURCES),
                f"{lineage}: name its parent, T1 and A48")
        out[lineage] = {}
        for name in PARENT_SOURCES:
            recorded = PARENT_CHECKPOINT_SHA256[lineage] if name == "parent" else REFERENCE_CHECKPOINTS[name]
            entry = spec[lineage][name]
            require(isinstance(entry, dict) and "utility" in entry and set(entry) <= {"utility", "screen"}
                    and (name != "parent" or "screen" in entry),
                    f"{lineage} {name}: the utility screen's source" + (" and the distillation screen's"
                                                                         if name == "parent" else ""))
            row, identities = {"recorded_checkpoint_sha256": recorded}, {}
            for side in ("utility", "screen"):
                if side not in entry:
                    continue
                what = f"{lineage} {name}: the {'utility' if side == 'utility' else 'distillation'} screen's source"
                read = []
                source = located_source(entry[side], what, failures, roots, read)
                row[side] = {"checkpoint": None if source is None else source["checkpoint"], "read": read}
                if source is None:
                    continue
                if source["checkpoint"]["sha256"] != recorded:
                    failures.append(f"{what}: its checkpoint is {source['checkpoint']['sha256']}, not the recorded "
                                    f"{recorded}")
                identities[side] = source_identity(source, what, failures, roots, read)
            if len(identities) == 2:
                a, b = identities["utility"], identities["screen"]
                differ = sorted(k for k in set(a) | set(b) if canonical(a.get(k)) != canonical(b.get(k)))
                row["sources_match"] = not differ
                if differ:
                    failures.append(f"{lineage} {name}: the utility screen's source differs from the distillation "
                                    f"screen's in {differ}")
            out[lineage][name] = row
    return {"schema": PARENTS_SCHEMA, "forbidden": forbidden, "lineages": out, "failures": failures,
            "pass": not failures}


# --------------------------------------- stage 0: the probe's references

REFERENCE_CHECK_SCHEMA = "sd1-v1/reference-check"
# BT1's G2 references, read on every probe's held-out anchor rows.
ANCHOR_REFERENCES = ("median_clamped_top_two_gap", "non_spy_top_agreement", "mean_kl_from_parent_non_spy")
OFFER_SUMMARY = ("offers", "mean_deployed_self_probability", "mean_raw_self_probability", "self_top_share")


def reference_check(lineage: str, probe: dict, screen_probe: dict) -> dict:
    """Phase-1 exactness's probe references (stage 0): the SD1 build's probe
    on the distillation screen's bank against the screen's recorded probe.
    The parent's and O-final's held-out median clamped gap, non-Spy top-action
    agreement and non-Spy KL, and the parent's learner-offer values (every
    field, each offer's row included), must be equal bit for bit, and so must
    both probes' banks, parent state, anchor pool and offer sets (pins by
    SHA-256) and both candidates' states."""
    mine, theirs = readouts(probe), readouts(screen_probe)
    require(probe.get("lineage") == lineage and screen_probe.get("lineage") == lineage, "a probe of another lineage")
    for cid in REFERENCE_IDS:
        require(cid in mine and cid in theirs, f"a probe lacks the {cid} candidate")
    checks = {}

    def compare(name, a, b, show=True):
        same = canonical(a) == canonical(b)
        checks[name] = ({"probe": a, "screen": b, "equal": same} if show else
                        {"probe_sha256": hashlib.sha256(canonical(a).encode()).hexdigest(),
                         "screen_sha256": hashlib.sha256(canonical(b).encode()).hexdigest(), "equal": same})

    compare("parent_state_sha256", probe.get("parent_state_sha256"), screen_probe.get("parent_state_sha256"))
    for field in ("banks", "anchor", "offers"):
        compare(field, unpathed(probe.get(field)), unpathed(screen_probe.get(field)), show=False)
    for cid in REFERENCE_IDS:
        compare(f"{cid} state_sha256", mine[cid].get("state_sha256"), theirs[cid].get("state_sha256"))
        for field in ANCHOR_REFERENCES:
            compare(f"{cid} {field}", (mine[cid].get("anchor_held_out") or {}).get(field),
                    (theirs[cid].get("anchor_held_out") or {}).get(field))
    a, b = mine["parent"].get("learner_offers"), theirs["parent"].get("learner_offers")
    checks["parent learner_offers"] = {"probe": {k: (a or {}).get(k) for k in OFFER_SUMMARY},
                                       "screen": {k: (b or {}).get(k) for k in OFFER_SUMMARY},
                                       "equal": a is not None and canonical(a) == canonical(b)}
    differing = [name for name, c in checks.items() if not c["equal"]]
    return {"schema": REFERENCE_CHECK_SCHEMA, "lineage": lineage, "checks": checks, "differing": differing,
            "banks_equal": checks["banks"]["equal"], "pass": not differing}


# ------------------------------------------------------------- fit table

FIT_SCHEMA = "sd1-v1/fit"
COLLAPSE_EXPLANATIONS = (
    "the evidence rows may move shared parameters",
    "the anchor may be too weak",
    "the step size may be too large",
    "G2's references come from ordinary training (O-final), and the intended change near evidence states may "
    "exceed them, most plausibly r1's KL")
# Section 7.1: each selection reading's row (what was observed, the reading,
# its competing explanations) and its label in the lineage's summary.
FIT_ROWS = {
    "engineering": ("G3 fails", "engineering: SD1 stops until the cause is fixed; the seed's rerun is a recorded "
                    "repeat", (), "engineering stop"),
    "required fit reached only together with collapse": (
        "F first passes at the checkpoint where G2 first fails",
        "required fit reached only together with collapse, at this dose, optimizer, targets and anchor",
        COLLAPSE_EXPLANATIONS, "required fit reached only together with collapse"),
    "collapse before the required fit": (
        "G2 fails before any checkpoint passes F",
        "collapse before the required fit, at this dose, optimizer, targets and anchor",
        COLLAPSE_EXPLANATIONS, "collapse before the required fit"),
    "required fit not reached": (
        "F never passes, with G2 holding through 8,192 steps",
        "required fit not reached at this dose, optimizer, targets and anchor",
        ("the evidence rows may be too few against the anchor",
         "the student's input may not separate them from similar rows"), "required fit not reached"),
    "passes": ("F, G2 and G3 pass at some read gated checkpoint", "passes", (), "passes"),
}
SUMMARY_ORDER = ("passes", "required fit reached only together with collapse", "collapse before the required fit",
                 "required fit not reached", "engineering")


def reading_kind(reading: str) -> str:
    kind = "engineering" if reading.startswith("engineering") else reading
    require(kind in FIT_ROWS, f"unknown selection reading {reading!r}")
    return kind


def read_checkpoints(selection: dict, values: dict | None) -> list:
    """Every read checkpoint of a selection (the first gated G2 failure
    included, nothing after it), each F and G2 value read again from the
    seed's probe and required to equal the selection's, with the held-out
    separated mass and KL(q || p_theta) and the non-Spy values beside."""
    label = selection["label"]
    entries = [dict(e) for e in selection.get("checkpoints", [])]
    stop = selection.get("g2_stop")
    if stop is not None:
        entries.append({"step": stop["step"], "gated": True, "F": stop["F"], "G2": stop["G2"]})
    require(not entries or values is not None, f"{selection['lineage']} {label}: no probe read this seed")
    out = []
    for e in entries:
        readout = values.get(f"{label}-step-{e['step']}")
        require(readout is not None, f"{label}: the probe did not read checkpoint step {e['step']}")
        g2 = g2_parts(readout, values["parent"], values["O-final"])
        mass = readout["evidence"]["training"]["mean_separated_mass"]
        rows = held_out_rows(readout)
        held = {"separated_mass": naive_mean([r["separated_mass"] for r in rows]),
                "mean_kl_target": readout["evidence"]["held_out"]["mean_kl_target"], "rows": len(rows)}
        same = (canonical(e["F"]["training_separated_mass"]) == canonical(mass)
                and canonical(e["G2"]["parts"]) == canonical(g2["parts"])
                and ("held_out" not in e or canonical(e["held_out"]) == canonical(held)))
        require(same, f"{label} step {e['step']}: the selection's values are not this probe's")
        out.append({"step": e["step"], "gated": e["gated"], "F": e["F"],
                    "G2": {"failed": g2["failed"], "pass": g2["pass"]}, "held_out": held,
                    "non_spy": {k: g2["non_spy"].get(k) for k in ("top_agreement", "mean_kl_from_parent")}})
    return out


def fit_row(selection: dict, values: dict | None) -> dict:
    kind = reading_kind(selection["reading"])
    result, reading, explanations, _ = FIT_ROWS[kind]
    row = {"seed": selection["seed"], "label": selection["label"], "kind": kind, "result": result,
           "reading": reading, "explanations": list(explanations), "probe_identity": selection["identity"]["pass"],
           "g3": selection.get("g3")}
    if kind == "engineering":
        row["cause"] = selection["reading"]
    checkpoints = read_checkpoints(selection, values)
    stop = selection.get("g2_stop")
    row["f_first_pass"] = next((c["step"] for c in checkpoints if c["gated"] and c["F"]["pass"]), None)
    row["g2_first_failure"] = None if stop is None else {"step": stop["step"], "failed": stop["failed"]}
    if kind == "passes":
        s, f, h = selection["selected_step"], selection["fit_dose"], selection["highest_passing_step"]
        row["reading"] = f"passes: selected step s = {s}, fit dose f = {f}, highest passing step h = {h}"
        row["d1"] = {"s": s, "f": f, "h": h, "minimizing_step": selection["d1"]["minimizing_step"],
                     "checkpoint": selection["d1_checkpoint"]}
    row["read_checkpoints"] = checkpoints
    return row


def lineage_summary(rows: list) -> str:
    """The fixed form, for example "passes in 7 of 8 seeds; collapse before
    the required fit in 1; no phase 2 or panels"."""
    counts = Counter(r["kind"] for r in rows)
    parts = [f"passes in {counts['passes']} of {len(rows)} seeds"]
    parts += [f"{FIT_ROWS[k][3]} in {counts[k]}" for k in SUMMARY_ORDER[1:] if counts[k]]
    if counts["engineering"]:
        parts.append("SD1 stops until the cause is fixed")
    else:
        parts.append("phase 2 and panels follow" if counts["passes"] == len(rows) else "no phase 2 or panels")
    return "; ".join(parts)


def fit_table(selections: list, probes: list) -> dict:
    """Section 7.1's fit table, per lineage and seed, from each seed's
    selection (``select``) and its probe: the G3 result, F's first pass at a
    gated checkpoint, G2's first failure and its failing parts, the reading
    row with its explanations, D1 with s, f and h, and the held-out separated
    mass and KL(q || p_theta) at every read checkpoint and no other; then
    each lineage's summary in the fixed form with counts. Reads no panel."""
    by_lineage: dict = {}
    for s in selections:
        require(s.get("schema") == SELECT_SCHEMA and s.get("lineage") in LINEAGES and s.get("seed") in SEEDS,
                "not an SD1 selection")
        seeds = by_lineage.setdefault(s["lineage"], {})
        require(s["seed"] not in seeds, f"{s['lineage']} seed {s['seed']} is selected twice")
        seeds[s["seed"]] = s
    for lineage, seeds in by_lineage.items():
        require(sorted(seeds) == list(SEEDS), f"{lineage}: the fit table reads all eight seeds")
    probe_of: dict = {}
    for p in probes:
        values = readouts(p)
        for label in sorted({cid.rsplit("-step-", 1)[0] for cid in values if "-step-" in cid}):
            key = (p.get("lineage"), label)
            require(key not in probe_of, f"{key[0]} {label}: two probes read this seed")
            probe_of[key] = values
    out = {"schema": FIT_SCHEMA, "lineages": {}}
    for lineage in LINEAGES:
        if lineage not in by_lineage:
            continue
        rows = [fit_row(by_lineage[lineage][k], probe_of.get((lineage, cell_label(k)))) for k in SEEDS]
        references = next((by_lineage[lineage][k]["references"] for k in SEEDS
                           if "references" in by_lineage[lineage][k]), None)
        beside = None
        if references is not None:
            o_non_spy = references["O-final"]["anchor_held_out_decks"]["non_spy"]
            beside = {"agreement_floor": o_non_spy["top_agreement"] - AGREEMENT_MARGIN,
                      "kl_ceiling": o_non_spy["mean_kl_from_parent"], "references": references}
        passes = sum(r["kind"] == "passes" for r in rows)
        out["lineages"][lineage] = {"summary": lineage_summary(rows), "phase2": passes == len(SEEDS),
                                    "engineering": any(r["kind"] == "engineering" for r in rows), "seeds": rows,
                                    "non_spy_beside": beside}
    out["engineering"] = any(v["engineering"] for v in out["lineages"].values())
    return out


# ----------------------------------------------------- phase-2 config gate

CONFIGS_SCHEMA = "sd1-v1/configs-check"
PHASE2_RUNS = tuple(f"D2-{k}" for k in SEEDS) + tuple(f"O-{k}" for k in SEEDS)
# RT1's placement fields (retention.py): set aside by the gate.
PLACEMENT_FIELDS = ("output_directory", "collection_workers", "preparation_workers")
# The design's named parent (section 6): O's initial units play the fixed
# opponent "parent", whose source is O's initial source.
INITIAL_OPPONENT = {"kind": "initial"}
NAMED_PARENT = {"kind": "fixed", "id": "parent"}


def placement_free(cfg: dict) -> dict:
    return {k: v for k, v in cfg.items() if k not in PLACEMENT_FIELDS}


def unit_id_seed(lineage: str, k: int, entry: dict) -> tuple:
    """A unit's episode id and seed on schedule seed k (design section 6),
    from the design's own formulas rather than phase2.py's: O's own
    (spy-reset-pilot-v1's ids, the pilot's unit seeds) for k = 0; for k of 1
    to 7, ids sd1-LIN-repK-... and seeds from sd1/train/repK."""
    tail = f"o{entry['own']}-t{entry['other']}-j{entry['j']}-s{entry['seat']}"
    if k == 0:
        return f"{pilot.NS}-{lineage}-{tail}", entry["seed"]
    return (f"sd1-{lineage}-rep{k}-{tail}",
            pilot.seed([f"sd1/train/rep{k}", lineage, entry["own"], entry["other"], entry["j"]]))


def replicate_problem(lineage: str, k: int, cfg: dict, base: dict) -> str | None:
    """Why ``cfg`` is not ``base`` on seed k's units, unit by unit, or None:
    placement aside, it may differ from ``base`` only in each unit's episode
    id and seed, which must be seed k's (unit_id_seed) for the unit the
    pilot's schedule puts there."""
    a, b = placement_free(cfg), placement_free(base)
    differ = sorted(f for f in set(a) | set(b) if f != "iterations" and canonical(a.get(f)) != canonical(b.get(f)))
    if differ:
        return f"it differs in {differ}"
    plan = pilot.schedule(lineage)["updates"]
    mine, theirs = a.get("iterations"), b.get("iterations")
    if not (isinstance(mine, list) and isinstance(theirs, list) and len(mine) == len(theirs) == len(plan)):
        return f"it does not hold the schedule's {len(plan)} updates"
    for u, (entries, new, old) in enumerate(zip(plan, mine, theirs)):
        if not (isinstance(new, dict) and isinstance(old, dict) and set(new) == set(old) == {"episodes"}
                and len(entries) == len(new["episodes"]) == len(old["episodes"])):
            return f"update {u} does not hold the schedule's units"
        for i, (entry, x, y) in enumerate(zip(entries, new["episodes"], old["episodes"])):
            where = f"update {u} unit {i}"
            if not (isinstance(x, dict) and isinstance(y, dict) and isinstance(x.get("episode"), dict)
                    and isinstance(y.get("episode"), dict)):
                return f"{where} is not a unit with an episode"
            if canonical({**x, "episode": None}) != canonical({**y, "episode": None}):
                return f"{where} differs outside its episode"
            ex, ey = x["episode"], y["episode"]
            if (canonical({f: v for f, v in ex.items() if f not in ("id", "seed")})
                    != canonical({f: v for f, v in ey.items() if f not in ("id", "seed")})):
                return f"{where}: its episode differs in more than its id and seed"
            want = unit_id_seed(lineage, k, entry)
            if (ex.get("id"), ex.get("seed")) != want:
                return (f"{where}: its id and seed are {ex.get('id')!r} and {ex.get('seed')!r}, not seed {k}'s "
                        f"{want[0]!r} and {want[1]!r}")
    return None


def named_parent_config(lineage: str, o_config: dict) -> tuple:
    """O's recorded config with the named parent (design section 6),
    restated: each unit O's config gives the initial opponent plays the
    fixed opponent parent, whose source is O's initial source verbatim, and
    nothing else changes. Returns it and the number of units changed."""
    require(o_config.get("schema") == pilot.ndb.SCHEMA, "not O's recorded run config")
    check_parent(lineage, o_config["initial_source"])
    value = json.loads(json.dumps(o_config))
    changed = 0
    for update in value["iterations"]:
        for item in update["episodes"]:
            if item.get("opponent") == INITIAL_OPPONENT:
                item["opponent"] = dict(NAMED_PARENT)
                changed += 1
    value["opponents"] = value["opponents"] + [{"id": NAMED_PARENT["id"], "source": o_config["initial_source"]}]
    return value, changed


def d2_problem(cfg: dict, o_k: dict, source: dict) -> str | None:
    """Why D2-k is not O-k from D1-k, or None: placement aside, it may
    differ from O-k only in its initial source, which must be D1-k's source
    verbatim."""
    a, b = placement_free(cfg), placement_free(o_k)
    differ = sorted(f for f in set(a) | set(b) if f != "initial_source" and canonical(a.get(f)) != canonical(b.get(f)))
    if differ:
        return f"it differs from O-k in {differ}"
    if canonical(cfg.get("initial_source")) != canonical(source):
        return "its initial source is not D1-k's source verbatim"
    return None


def config_differences(base: dict, cfg: dict) -> dict:
    """What separates a config from O-0's, placement aside (descriptive; a
    malformed update or unit, which the gate itself refuses, is counted as it
    stands rather than read)."""
    fields = sorted(k for k in set(base) | set(cfg)
                    if k not in ("iterations", *PLACEMENT_FIELDS) and canonical(base.get(k)) != canonical(cfg.get(k)))

    def updates(config) -> list:
        return [u.get("episodes") if isinstance(u, dict) and isinstance(u.get("episodes"), list) else []
                for u in (config.get("iterations") if isinstance(config.get("iterations"), list) else [])]

    def episode(item) -> dict:
        return item["episode"] if isinstance(item, dict) and isinstance(item.get("episode"), dict) else {}

    def rest(item) -> str:
        if not isinstance(item, dict):
            return canonical(item)
        return canonical({**item, "episode": {k: v for k, v in episode(item).items() if k not in ("id", "seed")}})
    a = [item for units in updates(base) for item in units]
    b = [item for units in updates(cfg) for item in units]
    out = {"fields": fields, "units": len(b),
           "same_updates": [len(u) for u in updates(base)] == [len(u) for u in updates(cfg)]}
    out["episode_ids"] = sum(episode(x).get("id") != episode(y).get("id") for x, y in zip(a, b))
    out["seeds"] = sum(episode(x).get("seed") != episode(y).get("seed") for x, y in zip(a, b))
    out["other_unit_fields"] = sum(rest(x) != rest(y) for x, y in zip(a, b))
    return out


def named_configs(values: list, flag: str = "--config") -> dict:
    out = {}
    for value in values:
        name, _, path = value.partition("=")
        require(name in PHASE2_RUNS and path and name not in out,
                f"{flag}: give NAME=PATH once for each of {', '.join(PHASE2_RUNS)}")
        out[name] = Path(path)
    require(set(out) == set(PHASE2_RUNS), f"{flag}: give NAME=PATH once for each of {', '.join(PHASE2_RUNS)}")
    return out


def configs_check(lineage: str, o_config: dict, selections: list, cfgs: dict, root: str,
                  qualification: dict | None = None) -> dict:
    """The phase-2 config gate (stage 4), as RT1's: the design's properties,
    checked unit by unit with the design's own formulas (not phase2.py's),
    placement (output directories and worker counts) aside. O-0 (stage 0's
    O-s0) is O's recorded config with the named parent, its units O's own;
    O-k (k = 1 to 7) differs from O-0 only in each unit's episode id and
    seed, which are seed k's under sd1; D2-k equals O-k but for its initial
    source, D1-k's source verbatim (so D2-0's updates are O-0's), and D1-k's
    source is the parent's but for its selected checkpoint. Each config's
    output directory is ROOT/LIN-NAME/native (normalized, case folded).
    With ``qualification`` ({name: config}), each config equals its
    qualification config, worker counts and output directories aside."""
    require(set(cfgs) == set(PHASE2_RUNS), "the gate reads D2-0 to D2-7 and O-0 to O-7")
    require(qualification is None or set(qualification) == set(PHASE2_RUNS),
            "the qualification set names D2-0 to D2-7 and O-0 to O-7")
    by_seed = {}
    for selection in selections:
        require(selection.get("schema") == SELECT_SCHEMA and selection.get("lineage") == lineage
                and selection.get("reading") == "passes" and selection["seed"] not in by_seed,
                "each seed's passing selection, once")
        by_seed[selection["seed"]] = selection
    require(sorted(by_seed) == list(SEEDS), "the gate reads every seed's selection")
    expected, changed = named_parent_config(lineage, o_config)
    parent = o_config["initial_source"]
    checks, problems = {}, {}

    def gate(name: str, problem: str | None):
        checks[name] = problem is None
        if problem is not None:
            problems[name] = problem

    initial_units = pilot.DECKS * pilot.DECKS * 5 * 2
    gate("O-0 is O's recorded config with the named parent, its units O's own",
         replicate_problem(lineage, 0, cfgs["O-0"], expected) if changed == initial_units else
         f"O's config gives the initial opponent to {changed} units, not the {initial_units} of j = 3 to 7")
    for k in SEEDS:
        chosen = by_seed[k]["d1_checkpoint"]
        source = read_pinned(chosen["source"], f"{lineage} D1-{k} source")
        pinned(chosen["checkpoint"], f"{lineage} D1-{k} checkpoint")
        gate(f"D1-{k}'s source is the parent's but for its selected checkpoint",
             None if source.get("checkpoint") == chosen["checkpoint"] and canonical(
                 {f: v for f, v in source.items() if f != "checkpoint"})
             == canonical({f: v for f, v in parent.items() if f != "checkpoint"})
             else "it names another checkpoint or differs from the parent's import or feature transfer")
        if k:
            gate(f"O-{k} is O-0 on seed {k}'s unit seeds and episode ids",
                 replicate_problem(lineage, k, cfgs[f"O-{k}"], cfgs["O-0"]))
        gate(f"D2-{k} is O-{k} from D1-{k}", d2_problem(cfgs[f"D2-{k}"], cfgs[f"O-{k}"], source))
    base = absolute(root).rstrip("/")
    directories = {}
    for name in PHASE2_RUNS:
        want = f"{base}/{lineage}-{name}/native"
        got = cfgs[name].get("output_directory")
        directories[name] = norm_key(got) if isinstance(got, str) else None
        gate(f"{name} writes {lineage}-{name}/native under the root",
             None if directories[name] == norm_key(want) else f"its output directory is {got!r}, not {want}")
    checks["each run writes its own output directory"] = len(set(directories.values())) == len(directories)
    if qualification is not None:
        for name in PHASE2_RUNS:
            a, b = placement_free(cfgs[name]), placement_free(qualification[name])
            differ = sorted(f for f in set(a) | set(b) if canonical(a.get(f)) != canonical(b.get(f)))
            gate(f"{name} is its qualification config, worker counts and output directory aside",
                 None if not differ else f"it differs in {differ}")
    return {"schema": CONFIGS_SCHEMA, "lineage": lineage, "root": base, "checks": checks, "problems": problems,
            "differences_from_O-0": {name: config_differences(cfgs["O-0"], cfgs[name]) for name in PHASE2_RUNS},
            "placement": {name: {f: cfgs[name].get(f) for f in PLACEMENT_FIELDS} for name in PHASE2_RUNS},
            "qualification_compared": qualification is not None, "pass": all(checks.values())}


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
    r.add_argument("--custody", type=Path, required=True)
    r.add_argument("--role", required=True)
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
    e.add_argument("--split", type=Path, required=True)
    e.add_argument("--out", type=Path, required=True)
    m = sub.add_parser("commands")
    m.add_argument("--lineage", required=True, choices=LINEAGES)
    for flag in ("--parent", "--anchor", "--census", "--out-dir"):
        m.add_argument(flag, type=Path, required=True)
    m.add_argument("--exactness", type=Path, nargs="+", required=True)
    m.add_argument("--root", required=True)
    pr = sub.add_parser("probe")
    pr.add_argument("--lineage", required=True, choices=LINEAGES)
    for flag in ("--parent", "--o-final", "--anchor", "--offers", "--census", "--out"):
        pr.add_argument(flag, type=Path, required=True)
    pr.add_argument("--full", type=Path)
    pr.add_argument("--output-directory", required=True)
    p2 = sub.add_parser("phase2")
    p2.add_argument("--lineage", required=True, choices=LINEAGES)
    p2.add_argument("--o-config", type=Path, required=True)
    p2.add_argument("--select", type=Path, nargs="+", required=True)
    p2.add_argument("--workers", type=int, required=True)
    p2.add_argument("--root", required=True)
    p2.add_argument("--out-dir", type=Path, required=True)
    k = sub.add_parser("known-answer")
    k.add_argument("--rebuild", nargs="+", required=True)
    k.add_argument("--rows", nargs="+", required=True)
    k.add_argument("--schedule", nargs="+", required=True)
    k.add_argument("--jobs", type=Path, required=True)
    k.add_argument("--custody", type=Path, required=True)
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
    cu = sub.add_parser("custody")
    cu.add_argument("--spec", type=Path, required=True)
    cu.add_argument("--out-dir", type=Path, required=True)
    cu.add_argument("--out", type=Path, required=True)
    pa = sub.add_parser("parents")
    pa.add_argument("--spec", type=Path, required=True)
    pa.add_argument("--custody", type=Path, required=True)
    pa.add_argument("--out", type=Path, required=True)
    rc = sub.add_parser("reference-check")
    rc.add_argument("--lineage", required=True, choices=LINEAGES)
    rc.add_argument("--probe", type=Path, required=True)
    rc.add_argument("--screen-probe", type=Path, required=True)
    rc.add_argument("--screen-probe-sha256", required=True)
    rc.add_argument("--out", type=Path, required=True)
    fi = sub.add_parser("fit")
    fi.add_argument("--selection", type=Path, nargs="+", required=True)
    fi.add_argument("--probes", type=Path, nargs="+", required=True)
    fi.add_argument("--out", type=Path, required=True)
    cc = sub.add_parser("configs-check")
    cc.add_argument("--lineage", required=True, choices=LINEAGES)
    cc.add_argument("--o-config", type=Path, required=True)
    cc.add_argument("--select", type=Path, nargs="+", required=True)
    cc.add_argument("--root", required=True)
    cc.add_argument("--config", nargs="+", required=True, help="NAME=CFG.json for D2-0 to D2-7 and O-0 to O-7")
    cc.add_argument("--qualification", nargs="+", help="NAME=CFG.json for the same sixteen names")
    cc.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    ok = True
    if a.op == "split":
        value = {**split(a.lineage, schedule_cases(a.schedule, a.lineage), a.held_out_matchups),
                 "schedule": pin(a.schedule)}
        result = write_json(a.out, value)
    elif a.op == "records":
        result = write_json(a.out, records(a.lineage, a.schedule, a.record_list, a.custody, a.role))
    elif a.op == "games":
        result = games(a.records, a.replay, a.rebuild, read_json(a.parent), a.out_dir, a.output_directory)
    elif a.op == "bank-check":
        value = bank_check(a.games_status, a.bank)
        result, ok = write_json(a.out, value), value["pass"]
    elif a.op == "targets":
        result = targets(read_json(a.spec), a.out_dir)
    elif a.op == "commands":
        result = {name: write_json(a.out_dir / f"{a.lineage}-{name}.command.json", command)
                  for name, command in commands(a.lineage, read_json(a.parent), read_json(a.anchor),
                                                read_json(a.census), [read_json(p) for p in a.exactness],
                                                a.root).items()}
    elif a.op == "probe":
        result = write_json(a.out, probe_command(a.lineage, read_json(a.parent), read_json(a.o_final),
                                                 read_json(a.anchor), read_json(a.offers), read_json(a.census),
                                                 read_json(a.full) if a.full else None, a.output_directory))
    elif a.op == "phase2":
        result = {name: write_json(a.out_dir / f"{a.lineage}-{name}.config.json", value)
                  for name, value in phase2_configs(a.lineage, read_json(a.o_config),
                                                    [read_json(p) for p in a.select], a.workers,
                                                    a.root).items()}
    elif a.op == "select":
        value = {**select(a.lineage, a.seed, read_json(a.probe), read_json(a.reference), read_json(a.full),
                          read_json(a.rerun), read_json(a.split)),
                 "inputs": {name: pin(path) for name, path in (("probe", a.probe), ("reference", a.reference),
                                                               ("full", a.full), ("rerun", a.rerun),
                                                               ("split", a.split))}}
        result, ok = write_json(a.out, value), not value["reading"].startswith("engineering")
    elif a.op == "known-answer":
        schedules = pairs(a.schedule, "--schedule")
        held = known_inputs(a.custody, a.jobs, schedules)
        value = known_answer(pairs(a.rebuild, "--rebuild"), pairs(a.rows, "--rows"), schedules, read_json(a.jobs))
        result, ok = write_json(a.out, {**value, "jobs": pin(a.jobs), "custody": held}), value["pass"]
    elif a.op == "bt1-regression":
        value = bt1_regression(pairs(a.rows, "--rows"))
        result, ok = write_json(a.out, value), value["pass"]
    elif a.op == "custody":
        value = custody(read_json(a.spec), a.out_dir, pin(a.spec))
        result, ok = write_json(a.out, value), value["pass"]
    elif a.op == "parents":
        value = {**parents(read_json(a.spec), custody_record(a.custody)["forbidden"]), "spec": pin(a.spec),
                 "custody": pin(a.custody)}
        result, ok = write_json(a.out, value), value["pass"]
    elif a.op == "reference-check":
        require(stream_sha256(a.screen_probe) == a.screen_probe_sha256,
                "the screen's probe differs from its recorded SHA-256")
        value = {**reference_check(a.lineage, read_json(a.probe), read_json(a.screen_probe)),
                 "probe": pin(a.probe), "screen_probe": pin(a.screen_probe)}
        result, ok = write_json(a.out, value), value["pass"]
    elif a.op == "fit":
        value = {**fit_table([read_json(p) for p in a.selection], [read_json(p) for p in a.probes]),
                 "selections": [pin(p) for p in a.selection], "probes": [pin(p) for p in a.probes]}
        result, ok = write_json(a.out, value), not value["engineering"]
    elif a.op == "configs-check":
        paths = named_configs(a.config)
        qualification = named_configs(a.qualification, "--qualification") if a.qualification else None
        value = {**configs_check(a.lineage, read_json(a.o_config), [read_json(p) for p in a.select],
                                 {name: read_json(path) for name, path in paths.items()}, a.root,
                                 None if qualification is None else
                                 {name: read_json(path) for name, path in qualification.items()}),
                 "o_config": pin(a.o_config), "configs": {name: pin(path) for name, path in paths.items()},
                 "qualification": None if qualification is None else
                 {name: pin(path) for name, path in qualification.items()},
                 "selections": [pin(p) for p in a.select]}
        result, ok = write_json(a.out, value), value["pass"]
    else:
        value = o_check(a.lineage, read_json(a.run), read_json(a.recorded), a.o_final)
        result, ok = write_json(a.out, value), value["pass"]
    print(json.dumps(result))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
