"""Anchor pools and offer sets of the Spy archive distillation screen (DESIGN.md section 3).

Pure functions of the namespace, lineage and indices; nothing is played here.

Anchor pool, per lineage (namespace spy-archive-distillation-anchor-v1): for
each of the 81 ordered deck pairs, the 4 values of j in 0..9 that rank first
by sha([ns, lineage, own, other, j]); each unit plays both learner seats on
seed([ns + "/anchor", lineage, own, other, j]), starting player
(j + own + other) mod 2, opponent by j (0-7 the parent, 8-9 A48). The first 3
units of each pair train the anchor (486 episodes); the 4th is held out for
the collapse probe (162 episodes).

Offer set, per lineage (namespace spy-archive-distillation-offers-v1): Spy as
the learner's deck against each of the nine decks, j in 0..19, both seats
(360 episodes), seed([ns, lineage, other, j]), starting player
(j + 4 + other) mod 2, opponent by j mod 10 under the same rule.

CLI (JSON on stdout):
  collect --lineage r1 --set anchor-training|anchor-held-out|offers --parent P.json
          --a48 A.json --workers W --output-directory DIR --out CMD.json
  anchor  --lineage r1 --parent P.json --training COLLECTION.json
          --held-out COLLECTION.json --out ANCHOR.json
  offers  --lineage r1 --parent P.json --collection COLLECTION.json --out OFFERS.json
The collect command runs directly on the expanded_deck_training_v1 binary:
it keeps the pilot's non-natural tolerance (0.2, retries recorded in a
ledger), which the guarded launcher admits for training requests only.
anchor and offers turn its receipts into the ArchiveDistillationAnchorV1 and
offer pin lists, checking every trajectory's episode against the schedule.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from distill_common import (ANCHOR_NS, DECKS, NON_NATURAL_FRACTION, OFFERS_NS, SPY, check_lineage,  # noqa: E402
                    check_parent, collection_opponent, episode, pilot, read_json, require, write_json)

SETS = ("anchor-training", "anchor-held-out", "offers")
ANCHOR_UNITS, ANCHOR_TRAINING_UNITS = 4, 3
OFFER_REPEATS = 20
COLLECTION_SCHEMA = "mtg-kernel-expanded-deck-collection/v1"


def anchor_units(lineage: str) -> dict:
    """(own, other) -> the pair's 4 anchor units' j values, in rank order."""
    check_lineage(lineage)
    return {(o, t): sorted(range(pilot.REPEATS), key=lambda j: pilot.sha([ANCHOR_NS, lineage, o, t, j]))[:ANCHOR_UNITS]
            for o in range(DECKS) for t in range(DECKS)}


def anchor_schedule(lineage: str, split: str) -> list[dict]:
    """The split's episodes in a fixed order (pair, unit rank, seat), without sources."""
    require(split in ("anchor-training", "anchor-held-out"), f"unknown anchor split {split!r}")
    out = []
    for (o, t), units in anchor_units(lineage).items():
        chosen = units[:ANCHOR_TRAINING_UNITS] if split == "anchor-training" else units[ANCHOR_TRAINING_UNITS:]
        for j in chosen:
            for seat in (0, 1):
                out.append({"id": f"{ANCHOR_NS}-{lineage}-o{o}-t{t}-j{j}-s{seat}", "own": o, "other": t, "j": j,
                            "seat": seat, "seed": pilot.seed([ANCHOR_NS + "/anchor", lineage, o, t, j]),
                            "starting_player": (j + o + t) % 2, "opponent": collection_opponent(j)})
    return out


def offer_schedule(lineage: str) -> list[dict]:
    check_lineage(lineage)
    return [{"id": f"{OFFERS_NS}-{lineage}-t{t}-j{j}-s{seat}", "own": SPY, "other": t, "j": j, "seat": seat,
             "seed": pilot.seed([OFFERS_NS, lineage, t, j]), "starting_player": (j + SPY + t) % 2,
             "opponent": collection_opponent(j % 10)}
            for t in range(DECKS) for j in range(OFFER_REPEATS) for seat in (0, 1)]


def schedule(lineage: str, name: str) -> list[dict]:
    return offer_schedule(lineage) if name == "offers" else anchor_schedule(lineage, name)


def collect_command(lineage: str, name: str, decks: list[dict], parent: dict, a48: dict, workers: int,
                    output_directory: str) -> dict:
    """The parent's own play of one set: learner the parent, each episode's
    other seat pinned to the parent or A48 by the j rule."""
    check_parent(lineage, parent)
    require(type(workers) is int and workers >= 1, "invalid worker count")
    sources = {"parent": parent, "a48": a48}
    episodes = [episode(e["id"], e["seed"], e["own"], e["other"], e["seat"], e["starting_player"], decks,
                        sources[e["opponent"]]) for e in schedule(lineage, name)]
    command = {"mode": "collect_parallel" if workers > 1 else "collect", "source": parent, "episodes": episodes}
    if workers > 1:
        command["workers"] = workers
    command["max_non_natural_episode_fraction"] = NON_NATURAL_FRACTION
    command["output_directory"] = str(output_directory).replace("\\", "/")
    return command


def collected(path: Path, lineage: str, name: str, parent: dict) -> list[dict]:
    """A complete collection receipt of the set: the parent's own play, one
    trajectory per scheduled episode, in schedule order."""
    check_parent(lineage, parent)
    receipt = read_json(path)
    require(receipt.get("schema") == COLLECTION_SCHEMA and receipt.get("complete") is True,
            f"{path}: not a complete collection receipt")
    require(receipt["source"] == parent, f"{path}: the collection's learner is not the parent")
    expected = schedule(lineage, name)
    pins = receipt["trajectories"]
    require(len(pins) == len(expected), f"{path}: {len(pins)} trajectories for {len(expected)} episodes")
    out = []
    for item, entry in zip(pins, expected):
        require(set(item) == {"path", "sha256"}, f"{path}: invalid trajectory pin")
        data = Path(item["path"]).read_bytes()
        require(hashlib.sha256(data).hexdigest() == item["sha256"], f"{item['path']}: SHA-256 differs")
        trajectory = json.loads(data)
        require(trajectory["episode"]["id"] == entry["id"]
                and trajectory["episode"]["learner_seat"] == entry["seat"]
                and trajectory["episode"].get("reset") is None,
                f"{item['path']}: episode differs from the {name} schedule")
        out.append({"path": item["path"], "sha256": item["sha256"]})
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    c = sub.add_parser("collect")
    c.add_argument("--set", required=True, choices=SETS)
    c.add_argument("--a48", type=Path, required=True)
    c.add_argument("--workers", type=int, required=True)
    c.add_argument("--output-directory", required=True)
    a = sub.add_parser("anchor")
    a.add_argument("--training", type=Path, required=True)
    a.add_argument("--held-out", type=Path, required=True)
    o = sub.add_parser("offers")
    o.add_argument("--collection", type=Path, required=True)
    for s in (c, a, o):
        s.add_argument("--lineage", required=True, choices=pilot.LINEAGES)
        s.add_argument("--parent", type=Path, required=True)
        s.add_argument("--out", type=Path, required=True)
    args = ap.parse_args(argv)
    parent = read_json(args.parent)
    if args.op == "collect":
        command = collect_command(args.lineage, args.set, pilot.load_decks(), parent, read_json(args.a48),
                                  args.workers, args.output_directory)
        print(json.dumps({**write_json(args.out, command), "episodes": len(command["episodes"])}))
    elif args.op == "anchor":
        value = {"training": collected(args.training, args.lineage, "anchor-training", parent),
                 "held_out": collected(args.held_out, args.lineage, "anchor-held-out", parent)}
        print(json.dumps({**write_json(args.out, value), "training": len(value["training"]),
                          "held_out": len(value["held_out"])}))
    else:
        value = collected(args.collection, args.lineage, "offers", parent)
        print(json.dumps({**write_json(args.out, value), "offers": len(value)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
