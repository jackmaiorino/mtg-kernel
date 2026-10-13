"""Archive games and the bank command of the Spy archive distillation screen (DESIGN.md sections 3-4).

Builds one lineage's ArchiveGameV1 lines from the frozen archive file
(ARCHIVE.jsonl: one row per game with its case fields, both seats' actions,
the replay's focal decision starts and the usability flag) and the raw
rows-frozen files holding each game's budget_game record (its focal
decisions and end). Every usable game of the lineage goes to the engine,
which replays it, checks each record and drops a failing game whole; this
tool only refuses inputs that disagree with each other (a missing, duplicate
or conflicting record), since those mean a wrong file rather than a game
that replays differently. No outcome is printed.

The bank command pins the games file, the archive file and every rows-frozen
file a game was read from.

CLI (JSON on stdout):
  build --lineage r1 --archive ARCHIVE.jsonl --rows-frozen DIR --parent P.json
        --out-dir DIR --output-directory BANK_DIR [--archive-sha256 SHA]
The archive file must match --archive-sha256, by default spy-discovery-archive-v1's
frozen SHA-256 (evidence/FREEZE.json of the archive reset pilot). The completion
set passes its own SHA-256 from its freeze record.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from distill_common import SPY, check_parent, episode, pilot, pin, read_json, read_jsonl, require, write_json  # noqa: E402

V1_ARCHIVE_SHA256 = "5bbb5a3425d398a32b4f3653c1d9084d602b2b379d01e2ad37e9147868adbefa"
START_FIELDS = ("step", "physical_decision_id", "k", "obs_hash", "menu_hash", "state_hash")
DECISION_FIELDS = ("step", "physical_decision_id", "substep", "k", "chooser", "edge_backups",
                   "spy_cast_offered", "spy_cast_chosen", "spy_self_offered", "spy_self_chosen",
                   "dr_giant_offered", "dr_giant_chosen")
CASE_FIELDS = ("lineage", "own", "other", "seat", "seed", "starting_player")
ENDS = ("win", "loss", "draw")


def archive_games(rows, lineage: str) -> list[dict]:
    """The lineage's usable archive games, in file order, each id once."""
    out, seen = [], set()
    for row in rows:
        require(row["id"] not in seen, f"duplicate archive game {row['id']}")
        seen.add(row["id"])
        if row["lineage"] == lineage and row["usable"] is True:
            out.append(row)
    return out


def budget_records(paths, ids: set) -> dict:
    """id -> [(file, game record)] for budget_game rows of the given games."""
    found: dict = {}
    for path in paths:
        for row in read_jsonl(path):
            if row.get("kind") == "budget_game" and row.get("id") in ids:
                found.setdefault(row["id"], []).append((Path(path), row))
    return found


def project(arch: dict, records: list, decks: list[dict]) -> tuple[dict, Path]:
    """One ArchiveGameV1 from its archive row and its budget_game record."""
    gid = arch["id"]
    require(records, f"{gid}: no budget_game record")
    path, record = records[0]
    for _, other in records[1:]:
        require(other == record, f"{gid}: conflicting budget_game records")
    own = arch.get("own", SPY)
    require(own == SPY, f"{gid}: an archive game plays Spy as the focal deck")
    for field in CASE_FIELDS:
        require(record[field] == (own if field == "own" else arch[field]), f"{gid}: {field} differs from its record")
    game = record["game"]
    require(game["valid"] is True and game["end"] in ENDS, f"{gid}: the record is not a valid natural game")
    require(game["actions"] == arch["actions"], f"{gid}: actions differ from the record")
    require(len(arch["starts"]) == game["learner_starts"], f"{gid}: start count differs from the record")
    starts = []
    for start in arch["starts"]:
        require(all(f in start for f in START_FIELDS), f"{gid}: incomplete focal start")
        starts.append({f: start[f] for f in START_FIELDS})
    decisions = []
    for decision in game["decisions"]:
        require(all(f in decision for f in DECISION_FIELDS), f"{gid}: incomplete decision record")
        decisions.append({f: decision[f] for f in DECISION_FIELDS})
    value = {"id": gid, "lineage": arch["lineage"],
             "episode": episode(gid, arch["seed"], own, arch["other"], arch["seat"], arch["starting_player"], decks),
             "actions": list(arch["actions"]), "starts": starts, "decisions": decisions, "end": game["end"]}
    return value, path


def build(lineage: str, archive: Path, rows_frozen: Path, parent: dict, out_dir: Path, output_directory: str,
          archive_sha256: str = V1_ARCHIVE_SHA256, decks: list[dict] | None = None) -> dict:
    check_parent(lineage, parent)
    archive = Path(archive).resolve()
    require(hashlib.sha256(archive.read_bytes()).hexdigest() == archive_sha256,
            "archive file differs from its frozen SHA-256")
    decks = decks if decks is not None else pilot.load_decks()
    games = archive_games(read_jsonl(archive), lineage)
    require(games, f"no usable {lineage} archive games")
    files = sorted(Path(rows_frozen).resolve().glob("*.jsonl"))
    records = budget_records(files, {g["id"] for g in games})
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    lines, used = [], set()
    for arch in games:
        value, path = project(arch, records.get(arch["id"], []), decks)
        lines.append(json.dumps(value, separators=(",", ":")) + "\n")
        used.add(path)
    games_path = (out_dir / f"games-{lineage}.jsonl").resolve()
    games_path.write_text("".join(lines), encoding="utf-8")
    sources = [pin(archive)] + [pin(p) for p in sorted(used)]
    command = {"mode": "archive_distillation_bank", "parent": parent, "lineage": lineage,
               "games": pin(games_path), "sources": sources,
               "output_directory": str(output_directory).replace("\\", "/")}
    cmd = write_json(out_dir / f"bank-{lineage}.command.json", command)
    return {"command": cmd, "games": command["games"], "usable_games": len(lines), "sources": len(sources)}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    b = sub.add_parser("build")
    b.add_argument("--lineage", required=True, choices=pilot.LINEAGES)
    b.add_argument("--archive", type=Path, required=True)
    b.add_argument("--rows-frozen", type=Path, required=True)
    b.add_argument("--parent", type=Path, required=True)
    b.add_argument("--out-dir", type=Path, required=True)
    b.add_argument("--output-directory", required=True)
    b.add_argument("--archive-sha256", default=V1_ARCHIVE_SHA256)
    a = ap.parse_args(argv)
    print(json.dumps(build(a.lineage, a.archive, a.rows_frozen, read_json(a.parent), a.out_dir,
                           a.output_directory, a.archive_sha256)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
