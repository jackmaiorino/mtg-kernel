"""Reproducible FDN Play Booster simulation over the frozen 286-name pool.

This is an explicit name/rarity collation model, not a physical print-sheet
emulator. Foil rarity odds are unpublished: v1 uses the nonfoil wildcard
weights for that slot. Seeds and generated packs belong only to the host.
"""

from __future__ import annotations

from collections import Counter
import hashlib
import json

import fdn_booster_pool_v1 as target
import limited_decks_v1 as limited
from limited_draft_v1 import DraftSession


PROFILE = limited.REPO_ROOT / "data/limited/fdn_v1/play_booster_collation_v1.json"
PROFILE_SHA256 = "5ae2cd2c41c38f5bf65959b15712b0b2e1ba7c4a8b9cf4f9fc281fdda0816516"
GENERATOR = "fdn-play-booster-sha256-counter/v1"
DUALS = (
    "Bloodfell Caves", "Blossoming Sands", "Dismal Backwater", "Jungle Hollow",
    "Rugged Highlands", "Scoured Barrens", "Swiftwater Cliffs", "Thornwood Falls",
    "Tranquil Cove", "Wind-Scarred Crag",
)
# Integer weights out of 10000, preserving the product guide's rounded odds.
RARE = (("rare", 7800), ("mythic", 1280), ("borderless_rare", 770), ("borderless_mythic", 150))
WILDCARD = (
    ("common", 1670), ("uncommon", 5830), ("rare", 1630), ("mythic", 260),
    ("borderless_common", 180), ("borderless_uncommon", 240),
    ("borderless_rare", 160), ("borderless_mythic", 30),
)


def identity() -> dict:
    return {"generator": GENERATOR, "membership_sha256": target.MEMBERSHIP_SHA256,
            "metadata_sha256": target.METADATA_SHA256, "collation_sha256": PROFILE_SHA256}


class _Random:
    """Platform-independent bounded draws, without modulo bias or global RNG."""

    def __init__(self, seed: int):
        if type(seed) is not int or not 0 <= seed < 2**64:
            raise ValueError("seed must be an unsigned 64-bit integer")
        self.seed = seed.to_bytes(8, "little")
        self.counter = 0

    def below(self, bound: int) -> int:
        ceiling = 2**256 - 2**256 % bound
        while True:
            block = hashlib.sha256(GENERATOR.encode() + b"\0" + self.seed
                                   + self.counter.to_bytes(8, "little")).digest()
            self.counter += 1
            value = int.from_bytes(block, "little")
            if value < ceiling:
                return value % bound

    def category(self, weights: tuple) -> str:
        draw = self.below(sum(weight for _, weight in weights))
        for name, weight in weights:
            if draw < weight:
                return name
            draw -= weight
        raise AssertionError("unreachable weighted draw")


def _categories() -> dict[str, tuple[dict, ...]]:
    document = limited.load_json(target.TARGET.read_bytes())
    target.validate(document)
    raw = PROFILE.read_bytes()
    # Normalize checkout line endings, since the profile is a JSON document.
    if hashlib.sha256(raw.replace(b"\r\n", b"\n")).hexdigest() != PROFILE_SHA256:
        raise ValueError("frozen Play Booster collation changed")
    profile = limited.load_json(raw)
    cards = document["cards"]
    by_name = {card["name"]: card for card in cards}
    borderless = profile["borderless"]
    if (len(borderless) != 70 or Counter(card["rarity"] for card in borderless)
            != {"common": 2, "uncommon": 8, "rare": 43, "mythic": 17}
            or any(card["name"] not in by_name for card in borderless)):
        raise ValueError("borderless identities do not match the frozen pool")
    categories = {}
    for rarity in ("common", "uncommon", "rare", "mythic"):
        categories[rarity] = tuple(card for card in cards if card["set"] == "FDN"
                                  and card["rarity"] == rarity
                                  and card["name"] not in (*DUALS, *limited.BASIC_LANDS))
        categories["borderless_" + rarity] = tuple(dict(card, set="FDN") for card in borderless
                                                   if card["rarity"] == rarity)
    # The frozen metadata has 101 uncommons, although the product prose says
    # 100. Preserve the authenticated pool, including every uncommon.
    categories["basic"] = tuple(by_name[name] for name in limited.BASIC_LANDS)
    categories["dual"] = tuple(by_name[name] for name in DUALS)
    categories["guest"] = tuple(card for card in cards if card["set"] == "SPG")
    return categories


def generate_packs(seed: int, seats: int = 8) -> tuple[tuple[limited.ImportedDeck, ...], ...]:
    """Three 14-card packs per seat; the pod may have 2 through 8 seats.

    Seven common slots (one replaced by a Special Guest with probability
    1.5%), three uncommons, rare/mythic, wildcard, foil wildcard, land.
    Main common/uncommon slots are without replacement within their category;
    independently drawn wildcard slots may repeat a name already in the pack.
    """
    rng = _Random(seed)
    if type(seats) is not int or not 2 <= seats <= 8:
        raise ValueError("pod size must be an integer from 2 through 8")
    categories = _categories()

    def choose(category: str) -> dict:
        rows = categories[category]
        return rows[rng.below(len(rows))]

    def entry(card: dict, foil: bool = False) -> limited.DeckEntry:
        printing = f"{card['set']}:{card['collector_number']}" + (":foil" if foil else "")
        return limited.DeckEntry(1, card["name"], printing)

    def distinct(category: str, count: int) -> list:
        available = list(categories[category])
        return [entry(available.pop(rng.below(len(available)))) for _ in range(count)]

    rounds = []
    for round_index in range(3):
        packs = []
        for seat in range(seats):
            guest = rng.below(10000) < 150
            rows = distinct("common", 6 if guest else 7)
            if guest:
                rows.append(entry(choose("guest")))
            rows += distinct("uncommon", 3)
            rows.append(entry(choose(rng.category(RARE))))
            rows.append(entry(choose(rng.category(WILDCARD))))
            rows.append(entry(choose(rng.category(WILDCARD)), foil=True))
            land = choose("basic" if rng.below(2) == 0 else "dual")
            rows.append(entry(land, foil=rng.below(5) == 0))
            packs.append(limited.ImportedDeck(f"FDN round {round_index} seat {seat}", tuple(rows), ()))
        rounds.append(tuple(packs))
    return tuple(rounds)


def start_draft(seed: int, seats: int = 8) -> DraftSession:
    provenance = json.dumps(identity(), sort_keys=True, separators=(",", ":"))
    return DraftSession(generate_packs(seed, seats), seed=seed, provenance=provenance)


def replay_draft(record: dict) -> DraftSession:
    """Verify generation as well as every supplied-pack pick in a host record."""
    restored = DraftSession.replay(record)
    generated = start_draft(record["seed"], restored.seat_count).record()
    for key in ("packs", "seed", "provenance", "input_sha256"):
        if record[key] != generated[key]:
            raise ValueError("draft does not match its frozen seeded pack generator")
    return restored
