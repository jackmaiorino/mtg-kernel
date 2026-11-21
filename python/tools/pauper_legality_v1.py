"""Check decks and the registries against the committed Pauper legality list.

Offline only: no engine build, process or network. The list is
`data/eternal_v1/pauper_legality_scryfall_20261121.json`, one row for every
non-token card in the `eternal-fixtures` build's three registries.

- ``check-registries``: no stale, duplicate or mislabelled rows; registry cards
  without a row are counted, and are problems only with ``--strict``.
- ``deck FILE``: print each card of a plain decklist that is not ``legal``. A name is
  resolved as spelled, then through the file's ``aliases``, then with split-card slash
  spacing normalized, then with diacritics folded; a fuzzy step must give exactly one
  registry name, else the card is ``unlisted``.
- ``summary``: row counts by registry and status.
"""

from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import re
import sys
import unicodedata
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
    row_names = set(rows)
    for alias, target in sorted(legality.get("aliases", {}).items()):
        if target not in row_names:
            problems.append(f"alias {alias}: target {target} is not a row")
        if alias in expected or alias in row_names:
            problems.append(f"alias {alias}: equals a registry name")
    for name in legality.get("unresolved", []):
        problems.append(f"{name}: unresolved")
    names = [row["name"] for row in legality["cards"]]
    if names != sorted(names):
        problems.append("rows are not sorted by name")
    return problems


def slash_form(name: str) -> str:
    """Split-card spacing: ``A/B``, ``A / B`` and ``A // B`` all become ``A // B``."""
    return re.sub(r"\s*/{1,2}\s*", " // ", name)


def ascii_fold(name: str) -> str:
    """Strip diacritics by Unicode decomposition."""
    return "".join(ch for ch in unicodedata.normalize("NFD", name) if not unicodedata.combining(ch))


def resolve_name(legality: dict[str, Any], name: str) -> tuple[str | None, str]:
    """Registry row a decklist spelling means, and how it was found.

    Order: exact name, alias, slash-spacing normalization, ASCII fold. The last two count
    only when they give exactly one row name; otherwise the result is ``(None, "unlisted")``.
    """
    rows = {row["name"] for row in legality["cards"]}
    if name in rows:
        return name, "exact"
    target = legality.get("aliases", {}).get(name)
    if target in rows:
        return target, "alias"
    slashed = slash_form(name)
    found = {row for row in rows if slash_form(row) == slashed}
    if len(found) == 1:
        return found.pop(), "slash"
    folded = ascii_fold(slashed)
    found = {row for row in rows if ascii_fold(slash_form(row)) == folded}
    if len(found) == 1:
        return found.pop(), "fold"
    return None, "unlisted"


def deck_problems(legality: dict[str, Any], text: str) -> list[tuple[str, str, str | None]]:
    """Cards of a decklist, mainboard then sideboard, that are not legal.

    Each entry is the name as the decklist spelled it, its status and, when an alias or
    an ASCII fold resolved it, the registry name.
    """
    status = {row["name"]: row["pauper"] for row in legality["cards"]}
    zones = census.parse_list(text)
    seen: set[str] = set()
    out = []
    for zone in ("mainboard", "sideboard"):
        for _, name in zones[zone]:
            row, how = resolve_name(legality, name)
            state = status[row] if row is not None else "unlisted"
            if state != "legal" and name not in seen:
                seen.add(name)
                out.append((name, state, row if how in ("alias", "fold") else None))
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
        for name, state, row in problems:
            print(f"{name}: {state}" + (f" (registry name: {row})" if row else ""))
        return int(bool(problems))
    for label, value in summary(legality).items():
        print(label, sum(value.values()), value)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
