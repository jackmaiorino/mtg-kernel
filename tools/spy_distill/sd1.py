"""Search-evidence distillation (SD1): custody, identity checks, records, root maps, targets, selection and gates.

collab design SD1 (search-evidence distillation), sections 3 to 6 and 7.1. Offline:
nothing here plays, searches or trains. The teacher games are the search arm
of the search-player utility screen (`mode=s4a-budget`, one budget_game record
per scheduled case, nine decks, both seats). No label or outcome selects a
game, a row or a target.

Stage 0, before any replay (each writes its record and exits 1 on a failure):
  custody     every input SD1 reads, each against its SHA-256: the members of
              the utility screen's EVIDENCE.tar (one of the durable archives
              its SOURCE-SEAL lists, read beside that seal; never a hot or
              holding copy) extracted through the seal's member map into SD1's
              own directory, the absolute paths inside the job specs and their
              inputs resolved to the extracted copies; and the plain files read
              in place. Every gap and mismatch is listed
  parents     section 3's parent identity: the parents', T1's and A48's
              checkpoints are their recorded pins, and each parent's source in
              the utility screen's inputs is the distillation screen's, paths
              aside
  reference-check  phase-1 exactness's probe references: the stage-0 probe's
              parent and O-final values equal the screen's recorded probe's
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
  configs-check  the phase-2 config gate, as RT1's: placement aside, O-0 is O's
              recorded config with the named parent, O-k differs from it only
              in replicate k's seeds and episode ids, and D2-k is O-k from D1-k
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
  commands --lineage r1 --parent P.json --anchor ANCHOR.json --census CENSUS.json
           --exactness EXACT-r1.json EXACT-r2.json --root DIR --out-dir DIR
  probe --lineage r1 --parent P.json --o-final O.json --anchor ANCHOR.json --offers OFFERS.json
        --census CENSUS.json [--full PHASE1.json] --output-directory DIR --out CMD.json
  select --lineage r1 --seed K --probe PROBE.json --reference REF_PROBE.json --full PHASE1.json
         --rerun RERUN.json [--split SPLIT.json] --out SELECT.json
  phase2 --lineage r1 --o-config O.json --select SELECT-0.json ... SELECT-7.json --workers W
         --root DIR --out-dir DIR
  known-answer --rebuild r1=REBUILD.jsonl [r2=...] --rows r1=ROWS-r1.jsonl [r2=...] --out KNOWN.json
  bt1-regression --rows r1=ROWS-r1.jsonl [r2=...] --out REGRESSION.json
  o-check --lineage r1 --run O-s0/completion.json --recorded O/completion.json
          --o-final O_FINAL_CHECKPOINT.json --out OCHECK.json
  custody --spec CUSTODY-SPEC.json --out-dir DIR --out RECORD-PINS.json
  parents --spec PARENTS-SPEC.json --out PARENTS.json
  reference-check --lineage r1 --probe STAGE0-PROBE.json --screen-probe SCREEN-PROBE.json
                  --screen-probe-sha256 SHA --out REFERENCES.json
  fit --selection SELECT.json ... --probes PROBE.json ... --out FIT.json
  configs-check --lineage r1 --o-config O.json --select SELECT-0.json ... SELECT-7.json
                --config D2-0=CFG.json ... D2-7=CFG.json O-0=CFG.json ... O-7=CFG.json --out CONFIGS.json
SPEC: {"r1": {"split": PIN, "games": PIN (GAMES-r1.json), "bank_check": PIN}, "r2": {...}}
CUSTODY-SPEC and PARENTS-SPEC: see custody() and parents().
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
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


# ------------------------------------------------------- stage 0: custody

CUSTODY_SPEC_SCHEMA = "sd1-v1/custody-spec"
CUSTODY_SCHEMA = "sd1-v1/custody"
# The utility screen's source seal (its seal_evidence.py): each member of
# EVIDENCE.tar with its SHA-256, each retained original path's member, and
# the durable archives (the hot root's copy is not among them).
SEAL_SCHEMA = "search-player-utility-source-seal/v1"
HEX64 = re.compile(r"^[0-9a-f]{64}$")
SAFE_PART = re.compile(r"^[A-Za-z0-9_.-]+$")
WINDOWS_ABSOLUTE = re.compile(r"^[A-Za-z]:[\\/]")
POSIX_ABSOLUTE = re.compile(r"^/[^/]+/[^/]")
# A string may carry several paths ("T1=PATH,A48=PATH", "source=PATH").
PATH_DELIMITERS = re.compile(r"([,;=])")
CHUNK = 1 << 20
CUSTODY_SPEC_KEYS = {"schema", "seal", "archive", "completion", "forbidden", "members", "rewrite", "keep", "files",
                     "counts"}


def path_key(path) -> str:
    """A path's identity for lookups: forward slashes, no trailing slash and
    case folded, as the seal's own lookup folds case."""
    return str(path).replace("\\", "/").rstrip("/").casefold()


def is_under(path, root) -> bool:
    p, r = path_key(path), path_key(root)
    return p == r or p.startswith(r + "/")


def parent_key(path) -> str:
    return path_key(path).rsplit("/", 1)[0]


def is_absolute_token(token) -> bool:
    return isinstance(token, str) and bool(WINDOWS_ABSOLUTE.match(token) or POSIX_ABSOLUTE.match(token))


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


def verify_file(entry, what: str, failures: list, forbidden=()) -> bool:
    """A pinned file in place: present, outside every forbidden root, its
    bytes' SHA-256 the pin's. A failure is listed, not raised."""
    if not is_pin(entry):
        failures.append(f"{what}: not a pin (a path and a SHA-256)")
        return False
    if any(is_under(entry["path"], root) for root in forbidden):
        failures.append(f"{what}: {entry['path']} lies under a hot or holding root, which SD1 never reads")
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


def custody_spec_problems(spec) -> list:
    if not isinstance(spec, dict) or spec.get("schema") != CUSTODY_SPEC_SCHEMA:
        return [f"not an {CUSTODY_SPEC_SCHEMA} spec"]
    problems = [f"unknown field {k!r}" for k in sorted(set(spec) - CUSTODY_SPEC_KEYS)]
    problems += [f"{k}: not a pin" for k in ("seal", "archive") if not is_pin(spec.get(k))]
    if spec.get("completion") is not None and not is_pin(spec["completion"]):
        problems.append("completion: not a pin")
    for key in ("forbidden", "keep"):
        value = spec.get(key, [])
        if not (isinstance(value, list) and all(is_absolute_token(v) for v in value)):
            problems.append(f"{key}: a list of absolute paths")
    rewrite = spec.get("rewrite", [])
    if not (isinstance(rewrite, list) and all(isinstance(r, dict) and set(r) == {"from", "to"}
                                              and is_absolute_token(r["from"]) and is_absolute_token(r["to"])
                                              for r in rewrite)):
        problems.append("rewrite: a list of {from, to} absolute path prefixes")
    members = spec.get("members")
    if not (isinstance(members, list) and members):
        problems.append("members: a nonempty list")
    for i, m in enumerate(members if isinstance(members, list) else []):
        by_name = (isinstance(m, dict) and "member" in m and "original" not in m and safe_name(m["member"])
                   and isinstance(m.get("sha256"), str) and HEX64.match(m["sha256"]) is not None)
        by_original = (isinstance(m, dict) and "original" in m and "member" not in m and "sha256" not in m
                       and is_pin(m["original"]))
        if not ((by_name or by_original) and safe_name(m.get("role")) and "/" not in m["role"]
                and isinstance(m.get("remap", False), bool)
                and set(m) <= {"role", "member", "sha256", "original", "remap"}):
            problems.append(f"members[{i}]: {{role, member, sha256}} or {{role, original}}, with an optional remap")
    files = spec.get("files", [])
    if not isinstance(files, list):
        problems.append("files: a list")
    for i, f in enumerate(files if isinstance(files, list) else []):
        if not (is_pin(f) and set(f) == {"role", "path", "sha256"} and safe_name(f["role"]) and "/" not in f["role"]):
            problems.append(f"files[{i}]: {{role, path, sha256}}")
    counts = spec.get("counts", {})
    if not (isinstance(counts, dict) and all(type(n) is int and n >= 0 for n in counts.values())):
        problems.append("counts: {role: count}")
    return problems


def seal_index(seal: dict, failures: list) -> dict | None:
    """The seal's members by name and by retained original path (aliases
    included), each original bound to its member's SHA-256."""
    files = {}
    for entry in seal.get("files") or []:
        name = entry.get("name")
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
        key = path_key(original["path"])
        if key in by_original and by_original[key][0] != name:
            failures.append(f"the seal maps {original['path']} to two members")
            return None
        by_original[key] = (name, original["sha256"])
        originals_of.setdefault(name, []).append(original["path"])
    return {"files": files, "by_original": by_original, "originals_of": originals_of}


def extract_members(archive: Path, wanted: dict, out_dir: Path, failures: list) -> dict:
    """Each wanted member of the archive, streamed to DIR/members/NAME with its
    size and SHA-256 checked against the seal's."""
    out = {}
    with tarfile.open(archive, mode="r:") as tar:
        entries, repeated = {}, set()
        for info in tar.getmembers():
            if info.name in entries:
                repeated.add(info.name)
            entries[info.name] = info
        for name, want in wanted.items():
            info = entries.get(name)
            if info is None or not info.isfile() or name in repeated:
                failures.append(f"{name}: not one regular file in the archive")
                continue
            if info.size != want["bytes"]:
                failures.append(f"{name}: {info.size} bytes in the archive, not the seal's {want['bytes']}")
                continue
            target = out_dir / "members" / name
            target.parent.mkdir(parents=True, exist_ok=True)
            digest = hashlib.sha256()
            source = tar.extractfile(info)
            with target.open("xb") as handle:
                for block in iter(lambda: source.read(CHUNK), b""):
                    digest.update(block)
                    handle.write(block)
            if digest.hexdigest() != want["sha256"]:
                failures.append(f"{name}: extracted with SHA-256 {digest.hexdigest()}, not {want['sha256']}")
                continue
            out[name] = target
    return out


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
    folded, key = text.replace("\\", "/").casefold(), path_key(root)
    return path_key(text) == key or key + "/" in folded


class Remap:
    """The absolute paths inside the remap members, each resolved by the
    spec's rewrite rules (first match), then the seal's member map (to the
    member's extracted copy, or its remapped copy when it is remapped too),
    then the verified plain files, then a directory of extracted members. A
    pin's SHA-256 must be the sealed bytes' and becomes its new file's. A path
    under a keep prefix stays; any other path that resolves nowhere, or a
    remapped copy that still names a forbidden root, is a failure."""

    def __init__(self, out_dir: Path, index: dict, wanted: dict, extracted: dict, files: list, spec: dict,
                 failures: list):
        self.out_dir, self.index, self.wanted, self.extracted = out_dir, index, wanted, extracted
        self.files = {path_key(f["path"]): f for f in files if f["verified"]}
        self.rewrite, self.keep = spec.get("rewrite", []), spec.get("keep", [])
        self.forbidden, self.failures = spec.get("forbidden", []), failures
        self.done, self.active, self.notes = {}, set(), {}

    def copy_path(self, name: str) -> Path:
        return self.out_dir / ("remapped" if self.wanted[name]["remap"] else "members") / name

    def rewritten(self, token: str) -> str:
        for rule in self.rewrite:
            if is_under(token, rule["from"]):
                head = rule["from"].replace("\\", "/").rstrip("/")
                return rule["to"].replace("\\", "/").rstrip("/") + token.replace("\\", "/")[len(head):]
        return token

    def directory(self, path: str) -> str | None:
        key, prefixes = path_key(path), set()
        for name in self.extracted:
            for original in self.index["originals_of"].get(name, []):
                original = original.replace("\\", "/")
                if path_key(original).startswith(key + "/"):
                    rest = original[len(key) + 1:]
                    prefixes.add(name[:-len(rest) - 1] if name.endswith("/" + rest) else None)
        if len(prefixes) == 1 and None not in prefixes:
            return absolute(self.out_dir / "members" / prefixes.pop())
        return None

    def target(self, member: str, token: str, pinned: bool):
        """(new path, sealed SHA-256, new SHA-256) for one absolute path, or
        None when it stays (kept, or listed as a failure)."""
        if any(is_under(token, root) for root in self.keep):
            self.notes[member]["kept"].append(token)
            return None
        path = self.rewritten(token)
        hit = self.index["by_original"].get(path_key(path))
        if hit is not None:
            name, sealed = hit
            if name not in self.extracted:
                self.failures.append(f"{member}: {token} is sealed as {name}, which custody did not extract")
                return None
            current = sealed
            if self.wanted[name]["remap"] and pinned:
                remapped = self.remapped(name)
                if remapped is None:
                    self.failures.append(f"{member}: {token} names {name}, whose remapped copy failed")
                    return None
                current = remapped["sha256"]
            return absolute(self.copy_path(name)), sealed, current
        plain = self.files.get(path_key(path))
        if plain is not None:
            return plain["path"], plain["sha256"], plain["sha256"]
        directory = None if pinned else self.directory(path)
        if directory is not None:
            return directory, None, None
        self.failures.append(f"{member}: {token} resolves to no extracted member, verified file or directory")
        return None

    def string(self, member: str, text):
        if not isinstance(text, str):
            return text
        parts, changed = PATH_DELIMITERS.split(text), False
        for i in range(0, len(parts), 2):
            if is_absolute_token(parts[i]):
                found = self.target(member, parts[i], False)
                if found is not None:
                    parts[i], changed = found[0], True
                    self.notes[member]["replaced"] += 1
        return "".join(parts) if changed else text

    def value(self, member: str, value):
        if isinstance(value, list):
            return [self.value(member, v) for v in value]
        if not isinstance(value, dict):
            return self.string(member, value)
        pinned = is_pin(value) and is_absolute_token(value["path"])
        path, sha = value.get("path"), value.get("sha256")
        if pinned:
            found = self.target(member, value["path"], True)
            if found is not None and found[1] != value["sha256"]:
                self.failures.append(f"{member}: the pin of {value['path']} has SHA-256 {value['sha256']}, "
                                     f"but the file it names has {found[1]}")
            elif found is not None:
                path, sha = found[0], found[2]
                self.notes[member]["replaced"] += 1
        out = {}
        for key, item in value.items():
            if pinned and key in ("path", "sha256"):
                out[key] = path if key == "path" else sha
            else:
                out[self.string(member, key)] = self.value(member, item)
        return out

    def remapped(self, name: str) -> dict | None:
        """The remapped copy of one member, DIR/remapped/NAME (the extracted
        bytes when nothing in it changes), with its pin."""
        if name in self.done:
            return self.done[name]
        if name in self.active:
            self.failures.append(f"{name}: its pins form a cycle")
            return None
        self.active.add(name)
        self.notes[name] = {"replaced": 0, "kept": []}
        result = None
        try:
            data = self.extracted[name].read_bytes()
            lines = name.endswith(".jsonl")
            if not (lines or name.endswith(".json")):
                self.failures.append(f"{name}: only JSON and JSONL members are remapped")
            else:
                text = data.decode("utf-8")
                values = [json.loads(x) for x in text.splitlines() if x.strip()] if lines else [json.loads(text)]
                new = [self.value(name, v) for v in values]
                for root in self.forbidden:
                    if any(names_root(s, root) for s in strings(new)):
                        self.failures.append(f"{name}: its remapped copy still names {root}")
                target = self.out_dir / "remapped" / name
                target.parent.mkdir(parents=True, exist_ok=True)
                if self.notes[name]["replaced"]:
                    body = ("".join(json.dumps(v, separators=(",", ":")) + "\n" for v in new) if lines
                            else json.dumps(new[0], separators=(",", ":")) + "\n")
                    target.write_bytes(body.encode("utf-8"))
                else:
                    target.write_bytes(data)
                result = {"path": absolute(target), "sha256": hashlib.sha256(target.read_bytes()).hexdigest()}
        except (ValueError, UnicodeDecodeError) as error:
            self.failures.append(f"{name}: not readable JSON ({error})")
        finally:
            self.active.discard(name)
        self.done[name] = result
        return result


def custody(spec: dict, out_dir: Path, spec_pin: dict | None = None) -> dict:
    """Stage 0's custody of SD1's inputs. SPEC (schema sd1-v1/custody-spec):
      seal        PIN of SOURCE-SEAL.json in one durable target
      archive     PIN of the EVIDENCE.tar beside it, one of the seal's archives
      completion  optional PIN of the CUSTODY-COMPLETION.json beside them
      forbidden   optional hot and holding roots: nothing is read from under them
      members     [{role, member, sha256} or {role, original: PIN}, optional
                  remap: true]: each sealed member SD1 reads (the search-arm
                  records, their job specs and inputs, ANALYSIS-SOURCES-001),
                  extracted through the seal's member map to DIR/members; a
                  remap member (JSON or JSONL) also gets DIR/remapped/NAME with
                  the absolute paths inside it resolved (class Remap)
      rewrite     optional [{from, to}] absolute prefixes applied first (for
                  example a computehost bundle's runtime root to that bundle's
                  path on the maintainer's desktop, as the seal knows it)
      keep        optional absolute prefixes left as they are
      files       [{role, path, sha256}]: the plain files SD1 reads in place
      counts      optional {role: n}: exactly n members and files have role
    Every gap and mismatch is listed in the record and fails it; DIR/lists/
    ROLE.txt lists each role's files (remapped copies where remapped)."""
    problems = custody_spec_problems(spec)
    require(not problems, "the custody spec is malformed: " + "; ".join(problems))
    out_dir = Path(out_dir)
    require(not out_dir.exists() or (out_dir.is_dir() and not any(out_dir.iterdir())),
            f"{out_dir} is not a fresh directory")
    forbidden = spec.get("forbidden", [])
    require(not any(is_under(absolute(out_dir), root) for root in forbidden),
            "the output directory lies under a hot or holding root")
    out_dir.mkdir(parents=True, exist_ok=True)
    failures: list = []
    seal, index = None, None
    if verify_file(spec["seal"], "the source seal", failures, forbidden):
        seal = json.loads(Path(spec["seal"]["path"]).read_text(encoding="utf-8"))
        if seal.get("schema") != SEAL_SCHEMA or seal.get("complete") is not True:
            failures.append("the source seal is not a complete search-player-utility source seal")
            seal = None
    archive_ok = False
    if seal is not None:
        index = seal_index(seal, failures)
        durable = [a for a in seal.get("archives") or [] if path_key(a.get("path", "")) ==
                   path_key(spec["archive"]["path"])]
        if len(durable) != 1 or durable[0].get("sha256") != spec["archive"]["sha256"]:
            failures.append(f"{spec['archive']['path']} with this SHA-256 is not one of the seal's durable archives")
        elif parent_key(spec["archive"]["path"]) != parent_key(spec["seal"]["path"]):
            failures.append("the seal does not sit beside the archive")
        else:
            archive_ok = verify_file(spec["archive"], "the durable archive", failures, forbidden)
    completion = spec.get("completion")
    if completion is not None and verify_file(completion, "the custody completion", failures, forbidden):
        done = json.loads(Path(completion["path"]).read_text(encoding="utf-8"))
        seals = [s.get("sha256") for s in done.get("source_seals") or []]
        archives = [a.get("sha256") for a in done.get("archives") or []]
        if not (done.get("complete") is True and spec["seal"]["sha256"] in seals
                and archives and set(archives) == {spec["archive"]["sha256"]}):
            failures.append("the custody completion is not complete for this seal and archive")
        if parent_key(completion["path"]) != parent_key(spec["seal"]["path"]) or (
                seal is not None and path_key(completion["path"]).rsplit("/", 1)[-1]
                != path_key(seal.get("completion_record", ""))):
            failures.append("the custody completion is not the seal's completion record beside it")
    files, seen = [], set()
    for entry in spec.get("files", []):
        if path_key(entry["path"]) in seen:
            failures.append(f"{entry['path']} is listed twice")
            continue
        seen.add(path_key(entry["path"]))
        files.append({**entry, "verified": verify_file(entry, f"{entry['role']} file", failures, forbidden)})
    wanted, roles = {}, Counter(f["role"] for f in spec.get("files", []))
    for i, m in enumerate(spec["members"]):
        roles[m["role"]] += 1
        if index is None:
            continue
        what = f"members[{i}] ({m['role']})"
        if "original" in m:
            hit = index["by_original"].get(path_key(m["original"]["path"]))
            if hit is None or hit[1] != m["original"]["sha256"]:
                failures.append(f"{what}: the seal maps no member with this SHA-256 to {m['original']['path']}")
                continue
            name, sha = hit
        else:
            name, sha = m["member"], m["sha256"]
        sealed = index["files"].get(name)
        if sealed is None or sealed["sha256"] != sha:
            failures.append(f"{what}: the seal lists no member {name} with SHA-256 {sha}")
        elif name in wanted:
            failures.append(f"{what}: {name} is listed twice")
        else:
            wanted[name] = {"role": m["role"], "sha256": sha, "bytes": sealed.get("bytes"),
                            "remap": m.get("remap", False)}
    if index is None or not archive_ok:
        failures.append("no member is extracted: the seal or the archive did not verify")
    extracted = (extract_members(Path(spec["archive"]["path"]), wanted, out_dir, failures)
                 if index is not None and archive_ok else {})
    remap = Remap(out_dir, index or {}, wanted, extracted, files, spec, failures)
    for name, want in wanted.items():
        if want["remap"] and name in extracted:
            remap.remapped(name)
    members, lists = [], {}
    for name, want in wanted.items():
        got = name in extracted
        entry = {"role": want["role"], "member": name, "sha256": want["sha256"],
                 "originals": index["originals_of"].get(name, []),
                 "extracted": {"path": absolute(extracted[name]), "sha256": want["sha256"]} if got else None}
        if want["remap"]:
            entry.update(remapped=remap.done.get(name), **remap.notes.get(name, {"replaced": 0, "kept": []}))
        members.append(entry)
        listed = entry.get("remapped") or entry["extracted"]
        if listed is not None:
            lists.setdefault(want["role"], []).append(listed["path"])
    for f in files:
        if f["verified"]:
            lists.setdefault(f["role"], []).append(f["path"])
    for role, n in spec.get("counts", {}).items():
        if roles[role] != n:
            failures.append(f"role {role}: the spec lists {roles[role]} members and files, not {n}")
    list_pins = {}
    for role, paths in lists.items():
        target = out_dir / "lists" / f"{role}.txt"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("".join(p + "\n" for p in paths), encoding="utf-8", newline="\n")
        list_pins[role] = pin(target)
    return {"schema": CUSTODY_SCHEMA, "spec": spec_pin, "out_dir": absolute(out_dir),
            "seal": {**spec["seal"], "complete": seal is not None}, "archive": {**spec["archive"],
                                                                                  "verified": archive_ok},
            "completion": completion, "forbidden": forbidden, "members": members, "files": files,
            "lists": list_pins, "counts": dict(sorted(roles.items())), "failures": failures, "pass": not failures}


# ------------------------------------------------ stage 0: parent identity

PARENTS_SPEC_SCHEMA = "sd1-v1/parents-spec"
PARENTS_SCHEMA = "sd1-v1/parents"
# The fixed references' checkpoint files (T1 is g115).
T1_CHECKPOINT_SHA256 = "88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1"
A48_CHECKPOINT_SHA256 = "beb86b4116c6eaa8406f12eb391adde1a637e063375740ea0b9812251bcff4a4"
RECORDED_CHECKPOINTS = {**PARENT_CHECKPOINT_SHA256, "T1": T1_CHECKPOINT_SHA256, "A48": A48_CHECKPOINT_SHA256}


def located_source(entry, what: str, failures: list) -> dict | None:
    """The model source a parents-spec entry names: {path, optional sha256,
    optional field}, with field the keys leading to the source inside the
    file (["initial_source"] in O's recorded config)."""
    if not (isinstance(entry, dict) and isinstance(entry.get("path"), str)
            and set(entry) <= {"path", "sha256", "field"} and isinstance(entry.get("field", []), list)):
        failures.append(f"{what}: give {{path, optional sha256, optional field}}")
        return None
    if "sha256" in entry and not verify_file(entry, what, failures):
        return None
    if not Path(entry["path"]).is_file():
        failures.append(f"{what}: {entry['path']} is missing")
        return None
    value = json.loads(Path(entry["path"]).read_text(encoding="utf-8"))
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


def source_identity(source: dict, what: str, failures: list) -> dict:
    """A model source with its paths aside, each pinned file checked in place:
    the checkpoint by its SHA-256, the play-import descriptor by its content
    (whose own pins compare by SHA-256), since the utility screen rewrote the
    descriptor's paths when it staged its inputs."""
    identity = unpathed(source)
    for key in ("checkpoint", "play_import"):
        verify_file(source[key], f"{what} {key}", failures)
    imported = Path(source["play_import"]["path"])
    if imported.is_file():
        try:
            content = json.loads(imported.read_text(encoding="utf-8"))
        except (ValueError, UnicodeDecodeError):
            content = None
        if isinstance(content, dict):
            for key, ref in content.items():
                if is_pin(ref):
                    verify_file(ref, f"{what} play import {key}", failures)
            identity["play_import"] = {"content": unpathed(content)}
    return identity


def parents(spec: dict) -> dict:
    """Section 3's parent identity (stage 0). SPEC (schema sd1-v1/parents-spec):
    {"r1": {"utility": SRC, "screen": SRC}, "r2": {...}, "T1": {"utility": SRC
    [, "screen": SRC]}, "A48": {...}}, SRC as located_source. Every source's
    checkpoint is its recorded pin and hashes to it; each parent's source in
    the utility screen's inputs is the distillation screen's, paths aside
    (and T1's and A48's, when the screen's are given)."""
    require(isinstance(spec, dict) and spec.get("schema") == PARENTS_SPEC_SCHEMA
            and set(spec) == {"schema", *RECORDED_CHECKPOINTS},
            f"not an {PARENTS_SPEC_SCHEMA} spec naming r1, r2, T1 and A48")
    failures, out = [], {}
    for name, recorded in RECORDED_CHECKPOINTS.items():
        entry = spec[name]
        require(isinstance(entry, dict) and "utility" in entry and set(entry) <= {"utility", "screen"}
                and (name not in LINEAGES or "screen" in entry),
                f"{name}: the utility screen's source" + (" and the distillation screen's" if name in LINEAGES
                                                          else ""))
        row, identities = {"recorded_checkpoint_sha256": recorded}, {}
        for side in ("utility", "screen"):
            if side not in entry:
                continue
            what = f"{name}: the {'utility' if side == 'utility' else 'distillation'} screen's source"
            source = located_source(entry[side], what, failures)
            row[side] = None if source is None else {"checkpoint": source["checkpoint"]}
            if source is None:
                continue
            if source["checkpoint"]["sha256"] != recorded:
                failures.append(f"{what}: its checkpoint is {source['checkpoint']['sha256']}, not the recorded "
                                f"{recorded}")
            identities[side] = source_identity(source, what, failures)
        if len(identities) == 2:
            a, b = identities["utility"], identities["screen"]
            differ = sorted(k for k in set(a) | set(b) if canonical(a.get(k)) != canonical(b.get(k)))
            row["sources_match"] = not differ
            if differ:
                failures.append(f"{name}: the utility screen's source differs from the distillation screen's in "
                                f"{differ}")
        out[name] = row
    return {"schema": PARENTS_SCHEMA, "sources": out, "failures": failures, "pass": not failures}


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
    both probes' parent state, anchor pool and offer sets (pins by SHA-256)
    and both candidates' states."""
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
    for field in ("anchor", "offers"):
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
            "banks_equal": canonical(unpathed(probe.get("banks"))) == canonical(unpathed(screen_probe.get("banks"))),
            "pass": not differing}


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


def placement_free(cfg: dict) -> dict:
    return {k: v for k, v in cfg.items() if k not in PLACEMENT_FIELDS}


def config_differences(base: dict, cfg: dict) -> dict:
    """What separates a config from O-0's, placement aside (descriptive)."""
    fields = sorted(k for k in set(base) | set(cfg)
                    if k not in ("iterations", *PLACEMENT_FIELDS) and canonical(base.get(k)) != canonical(cfg.get(k)))
    a = [item for update in base["iterations"] for item in update["episodes"]]
    b = [item for update in cfg.get("iterations", []) for item in update["episodes"]]
    out = {"fields": fields, "units": len(b), "same_updates": [len(u["episodes"]) for u in base["iterations"]]
           == [len(u["episodes"]) for u in cfg.get("iterations", [])]}
    out["episode_ids"] = sum(x["episode"]["id"] != y["episode"]["id"] for x, y in zip(a, b))
    out["seeds"] = sum(x["episode"]["seed"] != y["episode"]["seed"] for x, y in zip(a, b))
    out["other_unit_fields"] = sum(
        canonical({**x, "episode": {k: v for k, v in x["episode"].items() if k not in ("id", "seed")}})
        != canonical({**y, "episode": {k: v for k, v in y["episode"].items() if k not in ("id", "seed")}})
        for x, y in zip(a, b))
    return out


def named_configs(values: list) -> dict:
    out = {}
    for value in values:
        name, _, path = value.partition("=")
        require(name in PHASE2_RUNS and path and name not in out,
                f"--config: give NAME=PATH once for each of {', '.join(PHASE2_RUNS)}")
        out[name] = Path(path)
    return out


def configs_check(lineage: str, o_config: dict, selections: list, cfgs: dict) -> dict:
    """The phase-2 config gate (stage 4), as RT1's: placement (output
    directories and worker counts) aside, O-0's config is O's recorded config
    with the named-parent mapping (phase2.py --reproduce, which O-s0 ran); O-k
    (k = 1 to 7) differs from it only in replicate k's unit seeds and episode
    ids under the sd1 namespace; D2-k is O-k from the selected D1-k (its
    initial source the D1-k source verbatim). Each run writes its own output
    directory."""
    require(set(cfgs) == set(PHASE2_RUNS), "the gate reads D2-0 to D2-7 and O-0 to O-7")
    by_seed = {}
    for selection in selections:
        require(selection.get("schema") == SELECT_SCHEMA and selection.get("lineage") == lineage
                and selection.get("reading") == "passes" and selection["seed"] not in by_seed,
                "each seed's passing selection, once")
        by_seed[selection["seed"]] = selection
    require(sorted(by_seed) == list(SEEDS), "the gate reads every seed's selection")
    base = phase2.config(lineage, o_config, None, 1, "o-0")
    expected = {"O-0": base}
    for k in SEEDS:
        chosen = by_seed[k]["d1_checkpoint"]
        source = read_pinned(chosen["source"], f"{lineage} D1-{k} source")
        require(source.get("checkpoint") == chosen["checkpoint"], f"{lineage} D1-{k}: the source's checkpoint is not "
                "the selected one")
        pinned(chosen["checkpoint"], f"{lineage} D1-{k} checkpoint")
        expected[f"D2-{k}"] = phase2.config(lineage, o_config, source, 1, f"d2-{k}", replicate=k,
                                            namespace=phase2.SD1_NS)
        if k:
            expected[f"O-{k}"] = phase2.config(lineage, o_config, None, 1, f"o-{k}", replicate=k,
                                               namespace=phase2.SD1_NS)
    checks = {}
    for name in PHASE2_RUNS:
        what = ("O's recorded config with the named parent" if name == "O-0" else
                f"{name.replace('D2', 'O')} from D1-{name[-1]}" if name.startswith("D2") else
                f"O-0 on replicate {name[-1]}'s seeds and episode ids")
        checks[f"{name} is {what}"] = canonical(placement_free(cfgs[name])) == canonical(placement_free(expected[name]))
    directories = [path_key(cfgs[name].get("output_directory", "")) for name in PHASE2_RUNS]
    checks["each run writes its own output directory"] = len(set(directories)) == len(directories)
    return {"schema": CONFIGS_SCHEMA, "lineage": lineage, "checks": checks,
            "differences_from_O-0": {name: config_differences(base, cfgs[name]) for name in PHASE2_RUNS},
            "placement": {name: {f: cfgs[name].get(f) for f in PLACEMENT_FIELDS} for name in PHASE2_RUNS},
            "pass": all(checks.values())}


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
    cc.add_argument("--config", nargs="+", required=True, help="NAME=CFG.json for D2-0 to D2-7 and O-0 to O-7")
    cc.add_argument("--out", type=Path, required=True)
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
        value = select(a.lineage, a.seed, read_json(a.probe), read_json(a.reference), read_json(a.full),
                       read_json(a.rerun), read_json(a.split) if a.split else None)
        result, ok = write_json(a.out, value), not value["reading"].startswith("engineering")
    elif a.op == "known-answer":
        value = known_answer(pairs(a.rebuild, "--rebuild"), pairs(a.rows, "--rows"))
        result, ok = write_json(a.out, value), value["pass"]
    elif a.op == "bt1-regression":
        value = bt1_regression(pairs(a.rows, "--rows"))
        result, ok = write_json(a.out, value), value["pass"]
    elif a.op == "custody":
        value = custody(read_json(a.spec), a.out_dir, pin(a.spec))
        result, ok = write_json(a.out, value), value["pass"]
    elif a.op == "parents":
        value = {**parents(read_json(a.spec)), "spec": pin(a.spec)}
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
        value = {**configs_check(a.lineage, read_json(a.o_config), [read_json(p) for p in a.select],
                                 {name: read_json(path) for name, path in paths.items()}),
                 "o_config": pin(a.o_config), "configs": {name: pin(path) for name, path in paths.items()},
                 "selections": [pin(p) for p in a.select]}
        result, ok = write_json(a.out, value), value["pass"]
    else:
        value = o_check(a.lineage, read_json(a.run), read_json(a.recorded), a.o_final)
        result, ok = write_json(a.out, value), value["pass"]
    print(json.dumps(result))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
