"""Validate the frozen Foundations Play Booster target and report engine coverage.

Uses checked-in metadata only. Card registration does not prove rules correctness.
"""

from __future__ import annotations

from collections import Counter
import hashlib
import json
from pathlib import Path
import sys

import limited_decks_v1 as limited

TARGET = limited.REPO_ROOT / "data/limited/fdn_v1/booster_pool_v1.json"
MEMBERSHIP_SHA256 = "7a6ab02ccd88652bae0e299cdd6b9db33ef1890f1e537b7708c6887a9de84730"
METADATA_SHA256 = "864eb39784605b9a94b0a123c23f705c736a402767509354b52cfa18add9fdb0"


def validate(document: dict) -> None:
    if document.get("schema") != "kernel_limited_card_names/v1" or document.get("scope") != "foundations_play_booster/v1":
        raise ValueError("expected the frozen Foundations Play Booster target")
    names = document["card_names"]
    if not isinstance(names, list) or not all(isinstance(name, str) for name in names):
        raise ValueError("card names must be strings")
    if names != sorted(set(names)) or len(names) != 286:
        raise ValueError("expected 286 sorted unique card names")
    digest = hashlib.sha256(json.dumps(names, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
    if digest != MEMBERSHIP_SHA256 or document.get("card_names_sha256") != digest:
        raise ValueError("frozen target membership changed")
    cards = document["cards"]
    if [card["name"] for card in cards] != names:
        raise ValueError("printing rows must cover every target name exactly once")
    if Counter(card["set"] for card in cards) != {"FDN": 276, "SPG": 10}:
        raise ValueError("expected 276 main-set names and ten Special Guests")
    main = [card for card in cards if card["set"] == "FDN"]
    guests = [card for card in cards if card["set"] == "SPG"]
    if {int(card["collector_number"]) for card in guests} != set(range(74, 84)):
        raise ValueError("Special Guests must be the Foundations printings 74 through 83")
    if any(not 1 <= int(card["collector_number"]) <= 361 for card in main):
        raise ValueError("Starter Collection and Beginner Box printings are outside the booster target")
    if Counter(card["rarity"] for card in main) != {"common": 95, "uncommon": 101, "rare": 60, "mythic": 20}:
        raise ValueError("main-set rarity coverage changed")
    if not set(limited.BASIC_LANDS).issubset(names):
        raise ValueError("all five basic lands are required")
    tokens = document["token_dependencies"]
    if len(tokens) != 24 or len({token["id"] for token in tokens}) != 24:
        raise ValueError("expected 24 distinct token metadata records")
    for token in tokens:
        if not token["required_by"] or not set(token["required_by"]).issubset(names):
            raise ValueError("token dependencies must name target cards")
    metadata = {key: document[key] for key in ("cards", "token_dependencies")}
    metadata_digest = hashlib.sha256(json.dumps(metadata, ensure_ascii=False, sort_keys=True,
                                               separators=(",", ":")).encode()).hexdigest()
    if metadata_digest != METADATA_SHA256 or document.get("metadata_sha256") != metadata_digest:
        raise ValueError("frozen printing identities or token dependencies changed")


def report() -> dict:
    raw = TARGET.read_bytes()
    document = json.loads(raw)
    validate(document)
    registry = limited.combined_registry(
        (limited.REPO_ROOT / "data/cards_v1.json").read_bytes(),
        [(limited.REPO_ROOT / "data/limited/fdn_v1/cards_v1.json").read_bytes()],
    )
    result = limited.inventory(document["card_names"], registry, [])
    result["target_scope"] = document["scope"]
    result["target_sha256"] = hashlib.sha256(raw).hexdigest()
    result["membership_sha256"] = MEMBERSHIP_SHA256
    result["token_dependency_records"] = len(document["token_dependencies"])
    result["coverage_meaning"] = "Registry admission only; rules, interactions, restore and parity require gameplay tests."
    return result


if __name__ == "__main__":
    try:
        print(json.dumps(report(), ensure_ascii=False, sort_keys=True, indent=2))
    except (KeyError, TypeError, ValueError, OSError) as exc:
        print(str(exc), file=sys.stderr)
        raise SystemExit(2)
