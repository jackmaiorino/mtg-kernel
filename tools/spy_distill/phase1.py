"""Phase-1 and probe commands of the Spy archive distillation screen (DESIGN.md section 5).

Every command runs on the expanded_deck_training_v1 binary. Per lineage and
arm (U uniform, L line decisions): a dry run (census, exposure and the
anchor pre-flight, no step), the full run through its six checkpoints, and
the G3 rerun of the first 32 steps in a fresh directory. One probe per
lineage then reads the parent, O-final and every phase-1 checkpoint offline.

CLI (JSON on stdout):
  phase1 --lineage r1 --arm U|L --parent P.json --games GAMES.jsonl --bank BANK.json
         --anchor ANCHOR.json --output-directory DIR --out CMD.json [--dry-run | --rerun]
  probe  --lineage r1 --parent P.json --o-final O.json --games GAMES.jsonl --bank BANK.json
         [--completion-games GAMES.jsonl --completion-bank BANK.json]
         --anchor ANCHOR.json --offers OFFERS.json --phase1 PHASE1.json [PHASE1.json ...]
         --output-directory DIR --out CMD.json
``--bank`` is the bank command's bank.json; ``--phase1`` the full runs'
phase1.json receipts, whose checkpoint sources become candidates
``U-step-N`` and ``L-step-N``.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from distill_common import (ARM_LETTERS, ARMS, absolute, check_parent, pilot, pin, read_json,  # noqa: E402
                            require, write_json)

REPRODUCTION_STEP = 32
PHASE1_SCHEMA = "mtg-kernel-archive-distillation-phase1/v1"
MAX_CANDIDATES = 64


def phase1_command(lineage: str, arm: str, parent: dict, games: Path, bank: Path, anchor: dict,
                   output_directory: str, dry_run: bool = False, rerun: bool = False) -> dict:
    check_parent(lineage, parent)
    require(arm in ARMS, f"unknown arm {arm!r}")
    require(not (dry_run and rerun), "a dry run takes no step")
    command = {"mode": "archive_distillation_phase1", "parent": parent, "lineage": lineage, "arm": ARMS[arm],
               "games": pin(Path(games).resolve()), "bank": pin(Path(bank).resolve()), "anchor": anchor}
    if rerun:
        command["stop_after_steps"] = REPRODUCTION_STEP
    if dry_run:
        command["dry_run"] = True
    command["output_directory"] = absolute(output_directory)
    return command


def checkpoint_candidates(record: dict, lineage: str) -> list[dict]:
    """A complete full run's checkpoints as probe candidates, in dose order."""
    require(record.get("schema") == PHASE1_SCHEMA and record.get("complete") is True
            and record.get("stop_after_steps") is None and not record.get("dry_run"),
            "a probe reads complete full phase-1 runs")
    identity = record["identity"]
    require(identity["lineage"] == lineage, "phase-1 run from another lineage")
    letter = ARM_LETTERS[identity["arm"]]
    out = []
    for checkpoint in record["checkpoints"]:
        source = read_json(checkpoint["source"]["path"])
        require(pin(checkpoint["source"]["path"])["sha256"] == checkpoint["source"]["sha256"],
                "phase-1 source differs from its receipt")
        out.append({"id": f"{letter}-step-{checkpoint['step']}", "source": source})
    return out


def probe_command(lineage: str, parent: dict, o_final: dict, games: Path, bank: Path, anchor: dict,
                  offers: list, phase1: list[dict], output_directory: str,
                  completion: tuple[Path, Path] | None = None) -> dict:
    check_parent(lineage, parent)
    candidates = [{"id": "parent", "source": parent}, {"id": "O-final", "source": o_final}]
    for record in phase1:
        candidates += checkpoint_candidates(record, lineage)
    ids = [c["id"] for c in candidates]
    require(len(set(ids)) == len(ids) and len(ids) <= MAX_CANDIDATES, "duplicate or too many candidates")
    banks = [{"role": "training", "games": pin(Path(games).resolve()), "bank": pin(Path(bank).resolve())}]
    if completion is not None:
        training_ids = {g["id"] for g in map(json.loads, Path(games).read_text(encoding="utf-8").splitlines())}
        completion_ids = {g["id"] for g in map(json.loads, Path(completion[0]).read_text(encoding="utf-8").splitlines())}
        require(not training_ids & completion_ids, "a completion game is also a training game")
        banks.append({"role": "completion", "games": pin(Path(completion[0]).resolve()),
                      "bank": pin(Path(completion[1]).resolve())})
    return {"mode": "archive_distillation_probe", "parent": parent, "lineage": lineage, "candidates": candidates,
            "banks": banks, "anchor": anchor, "offers": offers,
            "output_directory": absolute(output_directory)}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    p = sub.add_parser("phase1")
    p.add_argument("--arm", required=True, choices=tuple(ARMS))
    mode = p.add_mutually_exclusive_group()
    mode.add_argument("--dry-run", action="store_true")
    mode.add_argument("--rerun", action="store_true")
    q = sub.add_parser("probe")
    q.add_argument("--o-final", type=Path, required=True)
    q.add_argument("--offers", type=Path, required=True)
    q.add_argument("--phase1", type=Path, nargs="+", required=True)
    q.add_argument("--completion-games", type=Path)
    q.add_argument("--completion-bank", type=Path)
    for s in (p, q):
        s.add_argument("--lineage", required=True, choices=pilot.LINEAGES)
        s.add_argument("--parent", type=Path, required=True)
        s.add_argument("--games", type=Path, required=True)
        s.add_argument("--bank", type=Path, required=True)
        s.add_argument("--anchor", type=Path, required=True)
        s.add_argument("--output-directory", required=True)
        s.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    parent, anchor = read_json(a.parent), read_json(a.anchor)
    if a.op == "phase1":
        command = phase1_command(a.lineage, a.arm, parent, a.games, a.bank, anchor, a.output_directory,
                                 a.dry_run, a.rerun)
    else:
        require((a.completion_games is None) == (a.completion_bank is None),
                "a completion bank needs its games and its bank")
        completion = (a.completion_games, a.completion_bank) if a.completion_games else None
        command = probe_command(a.lineage, parent, read_json(a.o_final), a.games, a.bank, anchor,
                                read_json(a.offers), [read_json(x) for x in a.phase1], a.output_directory,
                                completion)
    print(json.dumps(write_json(a.out, command)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
