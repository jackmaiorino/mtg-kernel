"""Host-only generated draft and deck construction records with exact replay.

Only DraftSession.view(seat) belongs to a drafter. These records include all
hidden packs, the generator seed, every pick, and every seat's built deck.
"""

from __future__ import annotations

import argparse
from dataclasses import asdict
import hashlib
import json
from pathlib import Path
import sys

import limited_decks_v1 as limited
import limited_play_booster_v1 as boosters
import limited_pool_v1 as construction
from limited_draft_v1 import DraftSession, _object


def canonical_bytes(document: dict) -> bytes:
    return json.dumps(document, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def _registry_identity() -> dict:
    paths = ("data/cards_v1.json", "data/limited/fdn_v1/cards_v1.json")
    return {path: hashlib.sha256(canonical_bytes(limited.load_json(
        (limited.REPO_ROOT / path).read_bytes()))).hexdigest() for path in paths}


def record_build(session: DraftSession, partitions: tuple[limited.ImportedDeck, ...],
                 basic_additions: tuple[tuple[limited.DeckEntry, ...], ...]) -> dict:
    """Validate all seats before returning a detached complete replay record."""
    if not isinstance(session, DraftSession) or not session.complete:
        raise ValueError("deck construction requires a completed draft")
    if len(partitions) != session.seat_count or len(basic_additions) != session.seat_count:
        raise ValueError("construction needs exactly one partition and basic supply per seat")
    # Admit only an authentic generated draft, not an arbitrary supplied pack
    # with a generator label added afterwards.
    draft_record = session.record()
    boosters.replay_draft(draft_record)
    registry = limited.fdn_registry()
    built = []
    for seat, (partition, basics) in enumerate(zip(partitions, basic_additions)):
        result = construction.construct_deck(session.acquired_pool(seat), partition, basics, registry=registry)
        built.append({"seat": seat, "deck": asdict(result.deck),
                      "basic_additions": [asdict(row) for row in result.basic_additions],
                      "mainboard_card_ids": list(result.mainboard_card_ids)})
    return json.loads(canonical_bytes({"schema": "kernel_limited_draft_build/v1", "draft": draft_record,
                                      "registry": _registry_identity(), "built": built}))


def _rows(value: object) -> tuple[limited.DeckEntry, ...]:
    if type(value) is not list:
        raise ValueError("deck rows must be an array")
    return tuple(limited.DeckEntry(**_object(row, {"count", "name", "printing"}, "deck row")) for row in value)


def _deck(value: dict) -> limited.ImportedDeck:
    value = _object(value, {"name", "mainboard", "sideboard"}, "deck")
    if value["name"] is not None and type(value["name"]) is not str:
        raise ValueError("deck name must be a string or None")
    return limited.ImportedDeck(value["name"], _rows(value["mainboard"]), _rows(value["sideboard"]))


def replay_build(record: dict) -> dict:
    record = _object(record, {"schema", "draft", "registry", "built"}, "draft build")
    if record["schema"] != "kernel_limited_draft_build/v1" or record["registry"] != _registry_identity():
        raise ValueError("draft build schema or registry identity differs")
    session = boosters.replay_draft(record["draft"])
    if type(record["built"]) is not list or len(record["built"]) != session.seat_count:
        raise ValueError("draft build must contain every seat exactly once")
    partitions, basics = [], []
    for seat, built in enumerate(record["built"]):
        built = _object(built, {"seat", "deck", "basic_additions", "mainboard_card_ids"}, "built seat")
        if type(built["seat"]) is not int or built["seat"] != seat:
            raise ValueError("built seats must be in seat order")
        if type(built["mainboard_card_ids"]) is not list or any(type(i) is not int for i in built["mainboard_card_ids"]):
            raise ValueError("mainboard card IDs must be integers")
        partitions.append(_deck(built["deck"]))
        basics.append(_rows(built["basic_additions"]))
    replayed = record_build(session, tuple(partitions), tuple(basics))
    if canonical_bytes(record) != canonical_bytes(replayed):
        raise ValueError("draft build replay differs from recorded decks or card IDs")
    return replayed


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    generate = commands.add_parser("generate", help="write a private initial draft record")
    generate.add_argument("--seed", type=int, required=True)
    generate.add_argument("--seats", type=int, default=8)
    generate.add_argument("--out", type=Path, required=True)
    build = commands.add_parser("build", help="validate explicit partitions for a completed draft")
    build.add_argument("--draft", type=Path, required=True)
    build.add_argument("--partitions", type=Path, required=True,
                       help='JSON array of {"deck": {name, mainboard, sideboard}, "basic_additions": [...]}')
    build.add_argument("--out", type=Path, required=True)
    replay = commands.add_parser("replay", help="replay a complete draft and build; print its SHA-256")
    replay.add_argument("record", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "generate":
            record = boosters.start_draft(args.seed, args.seats).record()
        elif args.command == "build":
            session = boosters.replay_draft(limited.load_json(args.draft.read_bytes()))
            plans = json.loads(args.partitions.read_bytes(), object_pairs_hook=limited._unique_object,
                               parse_constant=limited._reject_constant)
            if type(plans) is not list:
                raise ValueError("partitions must be an array")
            plans = [_object(plan, {"deck", "basic_additions"}, "partition") for plan in plans]
            record = record_build(session, tuple(_deck(plan["deck"]) for plan in plans),
                                  tuple(_rows(plan["basic_additions"]) for plan in plans))
        else:
            record = replay_build(limited.load_json(args.record.read_bytes()))
            print(hashlib.sha256(canonical_bytes(record)).hexdigest())
            return 0
        # Never overwrite an earlier draft or its evidence.
        with args.out.open("xb") as handle:
            handle.write(canonical_bytes(record) + b"\n")
    except (KeyError, TypeError, ValueError, OSError) as exc:
        print(str(exc), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
