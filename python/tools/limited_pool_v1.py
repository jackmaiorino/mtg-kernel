"""Construct an executable FDN deck from an acquired pool and explicit basics.

All inputs use the existing .dck format. The acquired file's two zones together
are inventory; the proposed deck's SB: rows are the reserve. This validates a
partition, not a draft, pack generator, deck recommendation or sideboarding game.
"""

from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import asdict, dataclass
import hashlib
import json
from pathlib import Path
import sys

import fdn_booster_pool_v1 as target
import limited_decks_v1 as limited


@dataclass(frozen=True)
class ConstructedDeck:
    deck: limited.ImportedDeck
    mainboard_card_ids: tuple[int, ...]
    basic_additions: tuple[limited.DeckEntry, ...]


def _counts(entries: tuple[limited.DeckEntry, ...], label: str,
            names: set[str], registry: dict[str, limited.RegistryCard]) -> Counter[str]:
    counts: Counter[str] = Counter()
    for entry in entries:
        if not isinstance(entry, limited.DeckEntry):
            raise ValueError(f"{label}: expected DeckEntry rows")
        if type(entry.count) is not int or entry.count <= 0:
            raise ValueError(f"{label}: counts must be positive integers")
        if not isinstance(entry.name, str) or not entry.name.strip():
            raise ValueError(f"{label}: expected a nonempty exact card name")
        if entry.printing is not None and (not isinstance(entry.printing, str) or not entry.printing.strip()):
            raise ValueError(f"{label}: invalid printing annotation")
        status = limited.card_status(entry.name, registry)
        if status in ("missing", "token"):
            raise ValueError(f"{label}: {entry.name} ({status})")
        if entry.name not in names:
            raise ValueError(f"{label}: {entry.name} is outside the frozen FDN pool")
        counts[entry.name] += entry.count
    return counts


def construct_deck(pool: limited.ImportedDeck, partition: limited.ImportedDeck,
                   basic_additions: tuple[limited.DeckEntry, ...] = (), *,
                   registry: dict[str, limited.RegistryCard] | None = None) -> ConstructedDeck:
    """Validate every copy before returning an immutable executable mainboard.

    Mainboard plus reserve must equal acquired inventory plus exactly the declared
    added basics. Copies are conserved by exact card name, matching existing
    catalog lookup; printing annotations and all zone row/copy order are retained.
    No four-copy limit applies. Partial/no-effect cards may remain in inventory
    and reserve, but resolve_mainboard remains the sole executable-card authority.
    The function never edits its inputs or consumes a caller's inventory.
    """
    if not isinstance(pool, limited.ImportedDeck) or not isinstance(partition, limited.ImportedDeck):
        raise ValueError("pool and partition must be ImportedDeck values")
    document = limited.load_json(target.TARGET.read_bytes())
    target.validate(document)
    names = set(document["card_names"])
    catalog = limited.fdn_registry() if registry is None else registry
    owned = _counts((*pool.mainboard, *pool.sideboard), "acquired pool", names, catalog)
    if not owned:
        raise ValueError("acquired pool must contain cards")
    added = _counts(basic_additions, "basic additions", names, catalog)
    if any(name not in limited.BASIC_LANDS for name in added):
        raise ValueError("basic additions permit only Plains, Island, Swamp, Mountain and Forest")
    mainboard = _counts(partition.mainboard, "mainboard", names, catalog)
    reserve = _counts(partition.sideboard, "reserve", names, catalog)
    supplied = owned + added
    selected = mainboard + reserve
    excess = selected - supplied
    unassigned = supplied - selected
    if excess or unassigned:
        details = []
        for label, counts in (("not owned or explicitly added", excess), ("unassigned copies", unassigned)):
            if counts:
                details.append(label + ": " + ", ".join(f"{count} {name}" for name, count in sorted(counts.items())))
        raise ValueError("invalid pool partition; " + "; ".join(details))
    # Match CustomDeckV1's native input safety bound before materializing IDs.
    if sum(mainboard.values()) > 10_000:
        raise ValueError("Limited mainboard exceeds the native 10000-card limit")
    ids = limited.resolve_mainboard(partition, catalog)
    # Copy row containers too, so a manually constructed ImportedDeck containing
    # lists cannot make a successful result mutable through a later list edit.
    deck = limited.ImportedDeck(partition.name, tuple(partition.mainboard), tuple(partition.sideboard))
    return ConstructedDeck(deck, tuple(ids), tuple(basic_additions))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pool", type=Path, required=True, help="acquired inventory .dck")
    parser.add_argument("--deck", type=Path, required=True, help="proposed mainboard and SB: reserve .dck")
    parser.add_argument("--basic-additions", type=Path, help="explicit additional basic copies in .dck format")
    args = parser.parse_args(argv)
    try:
        raw_pool = args.pool.read_bytes()
        raw_deck = args.deck.read_bytes()
        pool = limited.parse_dck(raw_pool.decode("utf-8-sig"))
        deck = limited.parse_dck(raw_deck.decode("utf-8-sig"))
        raw_basics = None
        additions: tuple[limited.DeckEntry, ...] = ()
        if args.basic_additions is not None:
            raw_basics = args.basic_additions.read_bytes()
            basic_deck = limited.parse_dck(raw_basics.decode("utf-8-sig"))
            additions = (*basic_deck.mainboard, *basic_deck.sideboard)
        raw_registry = (limited.REPO_ROOT / "data/cards_v1.json").read_bytes()
        raw_extension = (limited.REPO_ROOT / "data/limited/fdn_v1/cards_v1.json").read_bytes()
        catalog = limited.combined_registry(raw_registry, [raw_extension])
        built = construct_deck(pool, deck, additions, registry=catalog)
        print(json.dumps({
            "schema": "kernel_limited_pool_construction/v1",
            "name": built.deck.name,
            "card_id_order": "dck-row-then-copy/v1",
            "mainboard": [asdict(entry) for entry in built.deck.mainboard],
            "reserve": [asdict(entry) for entry in built.deck.sideboard],
            "basic_additions": [asdict(entry) for entry in built.basic_additions],
            "mainboard_card_ids": built.mainboard_card_ids,
            "pool_sha256": hashlib.sha256(raw_pool).hexdigest(),
            "deck_sha256": hashlib.sha256(raw_deck).hexdigest(),
            "basic_additions_sha256": hashlib.sha256(raw_basics).hexdigest() if raw_basics is not None else None,
            "registry_sha256": hashlib.sha256(raw_registry).hexdigest(),
            "registry_extensions_sha256": [hashlib.sha256(raw_extension).hexdigest()],
            "membership_sha256": target.MEMBERSHIP_SHA256,
        }, ensure_ascii=False, sort_keys=True, indent=2))
        return 0
    except (OSError, UnicodeError, ValueError) as exc:
        print(json.dumps({"error": str(exc)}, ensure_ascii=False), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
