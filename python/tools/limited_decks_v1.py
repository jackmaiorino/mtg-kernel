"""Import XMage .dck decks and inspect Limited registry coverage, without a build.

This is input tooling, not a Limited game session or a rules-parity claim.
Run directly with Python 3.11+; no Torch, engine process or network is required.
"""

from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import sys
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
BASIC_LANDS = ("Plains", "Island", "Swamp", "Mountain", "Forest")
CARD_ROW = re.compile(r"([1-9][0-9]*)\s+(?:\[([^\[\]]+)\]\s+)?(.+)")


@dataclass(frozen=True)
class DeckEntry:
    count: int
    name: str
    printing: str | None = None


@dataclass(frozen=True)
class ImportedDeck:
    name: str | None
    mainboard: tuple[DeckEntry, ...]
    sideboard: tuple[DeckEntry, ...]


@dataclass(frozen=True)
class RegistryCard:
    card_id: int
    capability: str
    is_token: bool


def parse_dck(text: str) -> ImportedDeck:
    """Read row/copy order verbatim; SB: rows never enter the mainboard.

    Printed set/collector annotations are retained, but lookup is by exact
    card name. Limited permits more than four copies of a card.
    """
    name = None
    mainboard: list[DeckEntry] = []
    sideboard: list[DeckEntry] = []
    for line_number, raw_line in enumerate(text.splitlines(), 1):
        line = raw_line.strip()
        if not line or line.startswith(("//", "#")):
            continue
        if line.startswith("NAME:"):
            if name is not None or not line[5:].strip():
                raise ValueError(f"line {line_number}: invalid or repeated NAME header")
            name = line[5:].strip()
            continue
        is_sideboard = line.startswith("SB:")
        if is_sideboard:
            line = line[3:].strip()
        match = CARD_ROW.fullmatch(line)
        if match is None or not match[3].strip() or match[3].strip().startswith("["):
            raise ValueError(f"line {line_number}: expected COUNT [SET:NUMBER] CARD or COUNT CARD")
        entry = DeckEntry(int(match[1]), match[3].strip(), match[2])
        (sideboard if is_sideboard else mainboard).append(entry)
    if not mainboard:
        raise ValueError("deck has no mainboard cards")
    return ImportedDeck(name, tuple(mainboard), tuple(sideboard))


def _unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _reject_constant(value: str) -> None:
    raise ValueError(f"invalid JSON constant: {value}")


def load_json(data: bytes) -> dict[str, Any]:
    value = json.loads(data, object_pairs_hook=_unique_object, parse_constant=_reject_constant)
    if not isinstance(value, dict):
        raise ValueError("expected a JSON object")
    return value


def registry_from_json(value: dict[str, Any]) -> dict[str, RegistryCard]:
    """Use the engine's existing array-order u16 ids and capability default."""
    if value.get("version") != 2 or not isinstance(value.get("cards"), list):
        raise ValueError("expected cards registry version 2 with a cards array")
    if len(value["cards"]) > 65_536:
        raise ValueError("registry exceeds the engine's u16 card-id range")
    result = {}
    for card_id, record in enumerate(value["cards"]):
        if not isinstance(record, dict):
            raise ValueError(f"registry card {card_id}: expected an object")
        name = record.get("name")
        capability = record.get("engine_capability", "no_effect")
        is_token = record.get("is_token", False)
        if not isinstance(name, str) or not name.strip() or name in result:
            raise ValueError(f"registry card {card_id}: missing, empty or duplicate name")
        if capability not in ("full", "partial", "no_effect") or type(is_token) is not bool:
            raise ValueError(f"registry card {card_id}: invalid capability or token flag")
        result[name] = RegistryCard(card_id, capability, is_token)
    return result


def card_status(name: str, registry: dict[str, RegistryCard]) -> str:
    card = registry.get(name)
    if card is None:
        return "missing"
    return "token" if card.is_token else card.capability


def combined_registry(base: bytes, extensions: list[bytes]) -> dict[str, RegistryCard]:
    """Append Limited definitions in the same order as the opt-in Rust build."""
    documents = [load_json(data) for data in [base, *extensions]]
    for document in documents:
        registry_from_json(document)
    return registry_from_json({
        "version": 2,
        "cards": [card for document in documents for card in document["cards"]],
    })


def fdn_registry() -> dict[str, RegistryCard]:
    return combined_registry(
        (REPO_ROOT / "data/cards_v1.json").read_bytes(),
        [(REPO_ROOT / "data/limited/fdn_v1/cards_v1.json").read_bytes()],
    )


def zone_report(entries: tuple[DeckEntry, ...], registry: dict[str, RegistryCard]) -> dict[str, Any]:
    counts: Counter[str] = Counter()
    for entry in entries:
        counts[entry.name] += entry.count
    cards = [
        {"name": name, "count": count, "status": card_status(name, registry)}
        for name, count in sorted(counts.items())
    ]
    return {
        "copies": sum(counts.values()),
        "unique_cards": len(counts),
        "supported_copies": sum(card["count"] for card in cards if card["status"] == "full"),
        "registry_ready": all(card["status"] == "full" for card in cards),
        "cards": cards,
    }


def inspect_deck(deck: ImportedDeck, registry: dict[str, RegistryCard]) -> dict[str, Any]:
    mainboard = zone_report(deck.mainboard, registry)
    return {
        "schema": "kernel_limited_deck_inspection/v1",
        "name": deck.name,
        "minimum_mainboard_size_met": mainboard["copies"] >= 40,
        "mainboard": mainboard,
        "sideboard": zone_report(deck.sideboard, registry),
    }


def resolve_mainboard(deck: ImportedDeck, registry: dict[str, RegistryCard]) -> list[int]:
    """Materialize a BO1 deck only after full validation; no partial output."""
    report = inspect_deck(deck, registry)
    if not report["minimum_mainboard_size_met"]:
        raise ValueError("Limited mainboard must contain at least 40 cards")
    blockers = [card for card in report["mainboard"]["cards"] if card["status"] != "full"]
    if blockers:
        raise ValueError("unsupported mainboard: " + "; ".join(
            f"{card['name']} ({card['status']})" for card in blockers
        ))
    return [registry[entry.name].card_id for entry in deck.mainboard for _ in range(entry.count)]


def inventory(card_names: list[str], registry: dict[str, RegistryCard], decks: list[ImportedDeck]) -> dict[str, Any]:
    if not card_names or any(not isinstance(name, str) or not name.strip() for name in card_names):
        raise ValueError("card_names must be a nonempty array of card names")
    if len(set(card_names)) != len(card_names):
        raise ValueError("duplicate set card name")
    reference_names = set(card_names)
    names = reference_names | set(BASIC_LANDS)
    fixture_counts: Counter[str] = Counter()
    for deck in decks:
        for entry in (*deck.mainboard, *deck.sideboard):
            fixture_counts[entry.name] += entry.count
    names.update(fixture_counts)
    cards = [{
        "name": name,
        "in_reference": name in reference_names,
        "fixture_copies": fixture_counts[name],
        "status": card_status(name, registry),
    } for name in sorted(names)]
    return {
        "schema": "kernel_limited_card_inventory/v1",
        "reference_card_count": len(reference_names),
        "required_card_count": len(cards),
        "status_counts": dict(sorted(Counter(card["status"] for card in cards).items())),
        "cards": cards,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("inspect", "resolve", "inventory"))
    parser.add_argument("--registry", type=Path, default=REPO_ROOT / "data/cards_v1.json")
    parser.add_argument("--registry-extension", type=Path, action="append", default=None,
                        help="append definitions; the default base registry includes the FDN extension")
    parser.add_argument("--deck", type=Path, action="append", default=[])
    parser.add_argument("--card-names", type=Path, default=REPO_ROOT / "data/limited/fdn_v1/card_names.json")
    args = parser.parse_args(argv)
    try:
        raw_registry = args.registry.read_bytes()
        extension_paths = args.registry_extension
        if extension_paths is None:
            extension_paths = ([REPO_ROOT / "data/limited/fdn_v1/cards_v1.json"]
                               if args.registry == REPO_ROOT / "data/cards_v1.json" else [])
        raw_extensions = [path.read_bytes() for path in extension_paths]
        registry = combined_registry(raw_registry, raw_extensions)
        raw_decks = [path.read_bytes() for path in args.deck]
        decks = [parse_dck(data.decode("utf-8-sig")) for data in raw_decks]
        if args.command == "inventory":
            raw_names = args.card_names.read_bytes()
            names_document = load_json(raw_names)
            if names_document.get("schema") != "kernel_limited_card_names/v1" or not isinstance(names_document.get("card_names"), list):
                raise ValueError("expected kernel_limited_card_names/v1 with a card_names array")
            result = inventory(names_document["card_names"], registry, decks)
            result["card_names_sha256"] = hashlib.sha256(raw_names).hexdigest()
        else:
            if len(decks) != 1:
                raise ValueError("inspect and resolve require exactly one --deck")
            result = inspect_deck(decks[0], registry)
            result["deck_sha256"] = hashlib.sha256(raw_decks[0]).hexdigest()
            if args.command == "resolve":
                result["schema"] = "kernel_limited_mainboard_ids/v1"
                result["card_id_order"] = "dck-row-then-copy/v1"
                result["card_ids"] = resolve_mainboard(decks[0], registry)
        result["registry_sha256"] = hashlib.sha256(raw_registry).hexdigest()
        result["registry_extensions_sha256"] = [hashlib.sha256(data).hexdigest()
                                               for data in raw_extensions]
        if args.command == "inventory":
            result["deck_sha256s"] = [hashlib.sha256(data).hexdigest() for data in raw_decks]
        print(json.dumps(result, ensure_ascii=False, sort_keys=True, indent=2))
        return 0
    except (OSError, UnicodeError, ValueError) as exc:
        print(json.dumps({"error": str(exc)}, ensure_ascii=False), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
