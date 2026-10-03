#!/usr/bin/env python3
"""Write an XMage .dek file from a pinned MTGO-export decklist text file.

Usage:
    write_dek_from_mtgo_list_v1.py <list.txt> <out.dek>
        [--substitute "Old Name=New Name"]... [--allow-pending "Name"]...

Input: MTGO export text ("N Card Name" lines; a blank line or a line reading
exactly "Sideboard" starts the sideboard section; " // " split-card suffixes
are stripped, keeping the front face name only).

Substitutions replace a card's name in the list, keeping its quantity and
sideboard flag. A substitution's source name (the "Old Name") must be
present somewhere in the source list, or it is an error. A substitution's
target name (the "New Name") must be present in data/cards_v1.json, unless
it is explicitly allow-listed with --allow-pending (for a card this wave
adds that is not yet registered); otherwise it is an error. Names that are
not a substitution target are written through verbatim, with no registry
check: this tool only writes deck files, it does not register them (a
later task owns the roster and registry).

Output: XMage .dek XML, header identical to the existing files under
oracle/xmage/decks/Pauper, one <Cards CatID="0" .../> row per list line in
list order, CRLF line endings, closing </Deck>. The tool fails closed if
the written deck does not sum to 60 mainboard plus 15 sideboard cards.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_REGISTRY_PATH = ROOT / "data" / "cards_v1.json"

# "4 Lightning Bolt" -> (4, "Lightning Bolt"); tolerant of stray whitespace,
# matching the parsing convention already used by pauper_meta_gap_v1.py.
LINE_RE = re.compile(r"^\s*(\d+)\s+(.+?)\s*$")

HEADER = (
    '<?xml version="1.0" encoding="UTF-8"?>\r\n'
    '<Deck xmlns:xsd="http://www.w3.org/2001/XMLSchema" '
    'xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">\r\n'
    "  <NetDeckID>0</NetDeckID>\r\n"
    "  <PreconstructedDeckID>0</PreconstructedDeckID>\r\n"
)
FOOTER = "</Deck>\r\n"


def escape_attr(value: str) -> str:
    """Escape a value for a double-quoted XML attribute.

    Deliberately always double-quoted (never xml.sax.saxutils.quoteattr's
    single-quote fallback): XMage's own DekDeckImporter does not use a real
    XML parser, it scans each line for a literal ``Name="`` and takes
    everything up to the next literal double quote, so the output must stay
    double-quoted for every value to remain importable.
    """
    return (
        value.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


class DekWriteError(RuntimeError):
    """Raised when the source list, a substitution, or the resulting deck is invalid."""


def parse_substitution(raw: str) -> tuple[str, str]:
    if "=" not in raw:
        raise DekWriteError(f'malformed --substitute {raw!r}: expected "Old Name=New Name"')
    old, new = raw.split("=", 1)
    old, new = old.strip(), new.strip()
    if not old or not new:
        raise DekWriteError(f'malformed --substitute {raw!r}: expected "Old Name=New Name"')
    return old, new


def parse_list(text: str) -> list[tuple[str, int, bool]]:
    """Parse MTGO export text into (name, quantity, sideboard) rows in source order."""
    rows: list[tuple[str, int, bool]] = []
    sideboard = False
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line:
            sideboard = True
            continue
        if line == "Sideboard":
            sideboard = True
            continue
        match = LINE_RE.match(line)
        if not match:
            raise DekWriteError(f"unparseable decklist line: {raw_line!r}")
        quantity = int(match.group(1))
        name = match.group(2).split(" // ", 1)[0].strip()
        rows.append((name, quantity, sideboard))
    return rows


def load_registry_names(registry_path: Path) -> set[str]:
    data = json.loads(registry_path.read_text(encoding="utf-8"))
    return {card["name"] for card in data["cards"]}


def apply_substitutions(
    rows: list[tuple[str, int, bool]],
    substitutions: list[tuple[str, str]],
    registry_names: set[str],
    allow_pending: set[str],
    registry_path: Path,
) -> list[tuple[str, int, bool]]:
    source_names = {name for name, _quantity, _sideboard in rows}
    out = rows
    for old_name, new_name in substitutions:
        if old_name not in source_names:
            raise DekWriteError(f"substitution source not in list: {old_name!r}")
        if new_name not in registry_names and new_name not in allow_pending:
            raise DekWriteError(
                f"substitution target not in {registry_path.as_posix()} "
                f"and not passed via --allow-pending: {new_name!r}"
            )
        out = [
            (new_name, quantity, sideboard) if name == old_name else (name, quantity, sideboard)
            for name, quantity, sideboard in out
        ]
    return out


def render_dek(rows: list[tuple[str, int, bool]]) -> str:
    parts = [HEADER]
    for name, quantity, sideboard in rows:
        side_attr = "true" if sideboard else "false"
        parts.append(
            f'  <Cards CatID="0" Quantity="{quantity}" Sideboard="{side_attr}" '
            f'Name="{escape_attr(name)}" Annotation="0"/>\r\n'
        )
    parts.append(FOOTER)
    return "".join(parts)


def write_dek(
    list_path: Path,
    out_path: Path,
    substitutions: list[tuple[str, str]],
    allow_pending: set[str],
    registry_path: Path = DEFAULT_REGISTRY_PATH,
) -> tuple[int, int]:
    text = list_path.read_text(encoding="utf-8")
    rows = parse_list(text)
    registry_names = load_registry_names(registry_path)
    rows = apply_substitutions(rows, substitutions, registry_names, allow_pending, registry_path)

    mainboard_total = sum(quantity for _name, quantity, sideboard in rows if not sideboard)
    sideboard_total = sum(quantity for _name, quantity, sideboard in rows if sideboard)
    if mainboard_total != 60 or sideboard_total != 15:
        raise DekWriteError(
            f"{list_path}: expected 60 mainboard + 15 sideboard cards, "
            f"got {mainboard_total} + {sideboard_total}"
        )

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(render_dek(rows).encode("utf-8"))
    return mainboard_total, sideboard_total


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("list_path", type=Path)
    parser.add_argument("out_path", type=Path)
    parser.add_argument(
        "--substitute", action="append", default=[], metavar='"Old Name=New Name"'
    )
    parser.add_argument("--allow-pending", action="append", default=[], metavar="Name")
    parser.add_argument("--registry", type=Path, default=DEFAULT_REGISTRY_PATH)
    args = parser.parse_args(argv)

    try:
        substitutions = [parse_substitution(raw) for raw in args.substitute]
        mainboard_total, sideboard_total = write_dek(
            args.list_path,
            args.out_path,
            substitutions,
            set(args.allow_pending),
            args.registry,
        )
    except DekWriteError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print(f"wrote {args.out_path}: {mainboard_total} main + {sideboard_total} side")
    return 0


if __name__ == "__main__":
    sys.exit(main())
