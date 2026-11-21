"""Check decks and the registries against the committed Pauper legality list.

Offline only: no engine build, process or network. The list is
`data/eternal_v1/pauper_legality_scryfall_20261121.json`, one row for every
non-token card in the `eternal-fixtures` build's three registries.

- ``check-registries``: no stale, duplicate or mislabelled rows; registry cards
  without a row are counted, and are problems only with ``--strict``.
- ``deck FILE``: print each card of a plain decklist that is not ``legal``.
- ``summary``: row counts by registry and status.
"""

from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import sys
from typing import Any

import eternal_meta_census_v1 as census


REPO_ROOT = census.REPO_ROOT
LEGALITY = REPO_ROOT / "data/eternal_v1/pauper_legality_scryfall_20261121.json"
REGISTRY_NAMES = ("pauper_v1", "standard", "eternal_v1")
STATUSES = ("legal", "not_legal", "banned", "restricted")


def load_legality(path: Path = LEGALITY) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def registry_cards() -> dict[str, list[str]]:
    """Non-token card names of each registry, in registry order."""
    cards: dict[str, list[str]] = {}
    for label, path in zip(REGISTRY_NAMES, census.ETERNAL_REGISTRIES):
        rows = json.loads(path.read_text(encoding="utf-8"))["cards"]
        cards[label] = [row["name"] for row in rows if not row.get("is_token")]
    return cards


def unlisted_cards(legality: dict[str, Any], registries: dict[str, list[str]]) -> list[str]:
    """Non-token registry cards that have no legality row, sorted by name."""
    listed = {row["name"] for row in legality["cards"]}
    return sorted({name for names in registries.values() for name in names} - listed)


def registry_problems(legality: dict[str, Any], registries: dict[str, list[str]],
                      strict: bool = False) -> list[str]:
    """Defects of the legality rows against the registries; empty when there are none.

    A registry card without a row is a problem only when ``strict``: card waves append
    registry rows without a legality lookup.
    """
    problems = []
    rows: dict[str, list[dict[str, Any]]] = {}
    for row in legality["cards"]:
        rows.setdefault(row["name"], []).append(row)
        if row["pauper"] not in STATUSES:
            problems.append(f"{row['name']}: unknown status {row['pauper']!r}")
    expected = {name: label for label, names in registries.items() for name in names}
    for name, label in sorted(expected.items()):
        found = rows.get(name, [])
        if len(found) > 1 or (strict and not found):
            problems.append(f"{name} ({label}): {len(found)} rows")
        elif found and found[0]["registry"] != label:
            problems.append(f"{name}: row registry {found[0]['registry']!r}, registry {label!r}")
    for name in sorted(set(rows) - set(expected)):
        problems.append(f"{name}: row without a non-token registry card")
    for name in legality.get("unresolved", []):
        problems.append(f"{name}: unresolved")
    names = [row["name"] for row in legality["cards"]]
    if names != sorted(names):
        problems.append("rows are not sorted by name")
    return problems


def deck_problems(legality: dict[str, Any], text: str) -> list[tuple[str, str]]:
    """Cards of a decklist, mainboard then sideboard, that are not legal, with their status."""
    status = {row["name"]: row["pauper"] for row in legality["cards"]}
    zones = census.parse_list(text)
    seen: set[str] = set()
    out = []
    for zone in ("mainboard", "sideboard"):
        for _, name in zones[zone]:
            state = status.get(name, "unlisted")
            if state != "legal" and name not in seen:
                seen.add(name)
                out.append((name, state))
    return out


def summary(legality: dict[str, Any]) -> dict[str, dict[str, int]]:
    counts: dict[str, Counter[str]] = {label: Counter() for label in REGISTRY_NAMES}
    for row in legality["cards"]:
        counts[row["registry"]][row["pauper"]] += 1
    return {label: dict(sorted(value.items())) for label, value in counts.items()}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--legality", type=Path, default=LEGALITY)
    sub = parser.add_subparsers(dest="command", required=True)
    check = sub.add_parser("check-registries")
    check.add_argument("--strict", action="store_true", help="also fail on registry cards without a row")
    deck = sub.add_parser("deck")
    deck.add_argument("file", type=Path)
    sub.add_parser("summary")
    args = parser.parse_args(argv)
    legality = load_legality(args.legality)
    if args.command == "check-registries":
        registries = registry_cards()
        problems = registry_problems(legality, registries, strict=args.strict)
        for row in problems:
            print(row, file=sys.stderr)
        print(f"{len(legality['cards'])} rows, {len(problems)} problems, "
              f"{len(unlisted_cards(legality, registries))} registry cards without a row")
        return int(bool(problems))
    if args.command == "deck":
        problems = deck_problems(legality, args.file.read_text(encoding="utf-8"))
        for name, state in problems:
            print(f"{name}: {state}")
        return int(bool(problems))
    for label, value in summary(legality).items():
        print(label, sum(value.values()), value)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
