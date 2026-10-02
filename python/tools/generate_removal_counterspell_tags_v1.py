#!/usr/bin/env python3
"""Generates data/pauper_removal_counterspell_tags_v1.json from the
mechanics tags in data/cards_v1.json (design section 3, W4). This is a
disclosed, reviewable mapping from mechanics tags to two boolean fields;
mtg-kernel/tests/removal_counterspell_tags_v1.rs cross-checks every row
against the live CARD_DEFS[card_id].target_spec so a hand-authored tag
cannot silently drift from engine ground truth (design section 9 risk).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTRY_PATH = ROOT / "data/cards_v1.json"
OUTPUT_PATH = ROOT / "data/pauper_removal_counterspell_tags_v1.json"
SCHEMA = "kernel_removal_counterspell_tags/v1"

# A card whose mechanics list intersects this set targets something
# (creature, land, player, artifact, spell on the stack, ...) as part of
# its own effect. This is deliberately narrower than "removal" (mechanics
# tag "removal" also covers untargeted, batch, or triggered removal such as
# Suffocating Fumes' mass pump, which is not a target-requiring spell).
# "counter_spell" is included here (not just "counter_target") because every
# counterspell requires a target -- the spell it counters -- even on the two
# rows (Envelop, Steel Sabotage) whose mechanics tag is "counter_spell" only;
# see the ground-truth comment on IS_A_SPELL_TYPES below for why this whole
# set is still gated by card type.
REQUIRES_TARGET_MECHANICS = frozenset({
    "destroy_target",
    "exile_target",
    "graveyard_target",
    "target_creature",
    "target_opponent",
    "target_player",
    "up_to_two_targets",
    "counter_target",
    "counter_spell",
})

# A card whose mechanics list intersects this set counters a spell on the
# stack. Every counterspell also requires a target (the spell it counters),
# so COUNTERSPELL_MECHANICS is always a subset of REQUIRES_TARGET_MECHANICS.
COUNTERSPELL_MECHANICS = frozenset({
    "counter_target",
    "counter_spell",
})

# mtg_kernel::card_def::TargetSpec ("What a spell/ability needs targeted at
# cast/activation time") is a single field on CardDef, and for a *permanent*
# (Creature/Artifact/Enchantment) it always describes the permanent SPELL's
# own casting, which in this pool's implemented cards never itself has a
# target: the target-bearing behavior mechanics tags such as "destroy_target"
# or "exile_target" describe on these cards is always on a later ETB trigger
# or activated ability (Gorilla Shaman's tap ability, Journey to Nowhere's
# ETB, Spellstutter Sprite's ETB counter, ...), a separate mechanism with its
# own targeting that CardDef.target_spec does not represent. Cross-checked
# against every card_id in mtg-kernel/tests/removal_counterspell_tags_v1.rs:
# for every Instant/Sorcery in this registry with a REQUIRES_TARGET_MECHANICS
# or COUNTERSPELL_MECHANICS tag, target_spec is the matching non-None (and,
# for counterspells, *SpellOnStack) live ground truth; for every Creature,
# Artifact, or Enchantment with such a tag, target_spec is None. This also
# matches GameSummaryV1's own operational definition (design section 3):
# "whether a requires-target removal spell in hand was ever offered as a
# legal cast" is a cast-legality question that only applies to spells whose
# own casting requires a target, i.e. instants and sorceries here, not to a
# permanent whose later ability happens to target something.
IS_A_SPELL_TYPES = frozenset({"Instant", "Sorcery"})


def source_digest() -> str:
    return hashlib.sha256(REGISTRY_PATH.read_bytes()).hexdigest()


def build_document() -> dict:
    registry = json.loads(REGISTRY_PATH.read_text(encoding="utf-8"))
    rows = []
    for card_id, card in enumerate(registry["cards"]):
        if not IS_A_SPELL_TYPES & set(card.get("types", [])):
            continue
        mechanics = set(card.get("mechanics", []))
        requires_target = bool(mechanics & REQUIRES_TARGET_MECHANICS)
        is_counterspell = bool(mechanics & COUNTERSPELL_MECHANICS)
        if not requires_target and not is_counterspell:
            continue
        source_mechanics = sorted(mechanics & (REQUIRES_TARGET_MECHANICS | COUNTERSPELL_MECHANICS))
        rows.append({
            "card_id": card_id,
            "name": card["name"],
            "requires_target": requires_target,
            "is_counterspell": is_counterspell,
            "source_mechanics": source_mechanics,
        })
    rows.sort(key=lambda row: row["card_id"])
    return {
        "schema": SCHEMA,
        "generated_from": "data/cards_v1.json",
        "source_digest_sha256": source_digest(),
        "cards": rows,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", action="store_true")
    group.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)

    document = build_document()
    serialized = json.dumps(document, indent=2, sort_keys=False) + "\n"

    if args.write:
        OUTPUT_PATH.write_text(serialized, encoding="utf-8")
        print(f"wrote {OUTPUT_PATH} ({len(document['cards'])} cards)")
        return 0

    if not OUTPUT_PATH.exists():
        print(f"{OUTPUT_PATH} does not exist; run --write first", file=sys.stderr)
        return 1
    current = OUTPUT_PATH.read_text(encoding="utf-8")
    if current != serialized:
        print(f"{OUTPUT_PATH} is stale relative to data/cards_v1.json; rerun --write", file=sys.stderr)
        return 1
    print(f"{OUTPUT_PATH} is up to date ({len(document['cards'])} cards)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
