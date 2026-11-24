"""Build a cube deal pool from a cube list, gap record, kernel id export and fence file.

Usage:
  pool.py --cube-list CUBE.json --gaps GAPS.json --id-export IDS.json --fence FENCE.json
          [--carddb-hash HEX] [--out POOL.json]
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys
from typing import Any

POOL_SCHEMA = "cube_pool/v1"
GAP_KEYS = ("partial", "fdn_only", "missing")
VALID_COLORS = ("W", "U", "B", "R", "G")
BASIC_COLORS: dict[str, str] = {
    "Plains": "W",
    "Island": "U",
    "Swamp": "B",
    "Mountain": "R",
    "Forest": "G",
}
MAX_CARD_ID = (1 << 16) - 1


def _is_card_id(value: Any) -> bool:
    return type(value) is int and 0 <= value <= MAX_CARD_ID


def _read_json_with_sha256(path: Path) -> tuple[Any, str]:
    raw = path.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    try:
        doc = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ValueError(f"failed to parse JSON {path}: {exc}") from exc
    return doc, digest


def _parse_cube_list(doc: Any) -> list[str]:
    if not isinstance(doc, dict) or not isinstance(doc.get("cards"), list):
        raise ValueError("cube list must be a JSON object with a 'cards' list")
    names: list[str] = []
    seen: set[str] = set()
    for index, item in enumerate(doc["cards"]):
        if not isinstance(item, dict) or not isinstance(item.get("name"), str) or not item["name"].strip():
            raise ValueError(f"cube list card at index {index} lacks a non-empty string 'name'")
        name = item["name"]
        if name in seen:
            raise ValueError(f"cube list names card twice: {name!r}")
        seen.add(name)
        names.append(name)
    return names


def _parse_gaps(doc: Any) -> set[str]:
    if not isinstance(doc, dict):
        raise ValueError("gap record must be a JSON object")
    excluded: set[str] = set()
    for key in GAP_KEYS:
        if key not in doc or not isinstance(doc[key], list):
            raise ValueError(f"gap record lacks list field {key!r}")
        for index, name in enumerate(doc[key]):
            if not isinstance(name, str) or not name.strip():
                raise ValueError(f"gap record {key}[{index}] must be a non-empty string")
            excluded.add(name)
    return excluded


def _parse_fence(doc: Any, cube_names: set[str]) -> list[dict[str, str]]:
    # The signed fence files wrap the list as {"cards": [...]} beside provenance keys
    # (schema, cube_list, source); the bare list is accepted too, as CubePool does.
    if isinstance(doc, dict):
        if not isinstance(doc.get("cards"), list):
            raise ValueError("fence object must hold a 'cards' list of {name, reason} objects")
        doc = doc["cards"]
    if not isinstance(doc, list):
        raise ValueError("fence file must be a JSON list of {name, reason} objects")
    entries: list[dict[str, str]] = []
    seen: set[str] = set()
    for index, item in enumerate(doc):
        if not isinstance(item, dict) or set(item.keys()) != {"name", "reason"}:
            raise ValueError(f"fence entry {index} must be an object with exactly 'name' and 'reason'")
        name, reason = item["name"], item["reason"]
        if not isinstance(name, str) or not name.strip():
            raise ValueError(f"fence entry {index} has invalid 'name'")
        if not isinstance(reason, str) or not reason.strip():
            raise ValueError(f"fence entry {index} ({name!r}) must have a non-empty 'reason'")
        if name not in cube_names:
            raise ValueError(f"fenced card is not in the cube list: {name!r}")
        if name in seen:
            raise ValueError(f"fenced card listed twice: {name!r}")
        seen.add(name)
        entries.append({"name": name, "reason": reason})
    return entries


def _parse_id_export(doc: Any) -> tuple[str, dict[str, dict[str, Any]]]:
    if not isinstance(doc, dict):
        raise ValueError("id export must be a JSON object")
    carddb_hash = doc.get("kernel_carddb_hash")
    if not isinstance(carddb_hash, str) or not carddb_hash.strip():
        raise ValueError("id export lacks a non-empty string 'kernel_carddb_hash'")
    cards = doc.get("cards")
    if not isinstance(cards, list):
        raise ValueError("id export lacks a 'cards' list")

    by_name: dict[str, dict[str, Any]] = {}
    seen_ids: dict[int, str] = {}
    valid_color_set = set(VALID_COLORS)
    for index, entry in enumerate(cards):
        if not isinstance(entry, dict):
            raise ValueError(f"id export cards[{index}] must be an object")
        name = entry.get("name")
        if not isinstance(name, str) or not name.strip():
            raise ValueError(f"id export cards[{index}] lacks a non-empty string 'name'")
        if name in by_name:
            raise ValueError(f"id export lists card twice: {name!r}")
        has_definition = entry.get("has_definition")
        is_land = entry.get("is_land")
        colors = entry.get("colors")
        card_id = entry.get("card_id")
        if not isinstance(has_definition, bool):
            raise ValueError(f"id export card {name!r} has non-boolean 'has_definition'")
        if not isinstance(is_land, bool):
            raise ValueError(f"id export card {name!r} has non-boolean 'is_land'")
        if not isinstance(colors, list) or not all(isinstance(c, str) and c in valid_color_set for c in colors):
            raise ValueError(f"id export card {name!r} has invalid 'colors': {colors!r}")
        if has_definition:
            if not _is_card_id(card_id):
                raise ValueError(f"id export card {name!r} has invalid 'card_id': {card_id!r}")
            if card_id in seen_ids:
                raise ValueError(
                    f"id export assigns duplicate card_id {card_id} to {seen_ids[card_id]!r} and {name!r}"
                )
            seen_ids[card_id] = name
        else:
            if card_id is not None:
                raise ValueError(f"id export card {name!r} has no definition but non-null 'card_id'")
        canonical_colors = [c for c in VALID_COLORS if c in colors]
        by_name[name] = {
            "name": name,
            "card_id": card_id,
            "colors": canonical_colors,
            "is_land": is_land,
            "has_definition": has_definition,
        }
    return carddb_hash, by_name


def build_pool(
    cube_doc: Any,
    gaps_doc: Any,
    id_export_doc: Any,
    fence_doc: Any,
    input_sha256: dict[str, str],
    expected_carddb_hash: str | None = None,
) -> dict[str, Any]:
    """Validate all inputs and return the sorted cube pool document."""
    cube_names = _parse_cube_list(cube_doc)
    cube_name_set = set(cube_names)
    gap_names = _parse_gaps(gaps_doc)
    fenced_entries = _parse_fence(fence_doc, cube_name_set)
    fenced_names = {entry["name"] for entry in fenced_entries}
    carddb_hash, id_by_name = _parse_id_export(id_export_doc)

    if expected_carddb_hash is not None:
        if not isinstance(expected_carddb_hash, str) or not expected_carddb_hash.strip():
            raise ValueError("expected carddb hash must be a non-empty string")
        if carddb_hash != expected_carddb_hash:
            raise ValueError(
                f"id export kernel_carddb_hash {carddb_hash!r} differs from expected {expected_carddb_hash!r}"
            )

    spells: list[dict[str, Any]] = []
    nonbasic_lands: list[dict[str, Any]] = []
    for name in cube_names:
        if name in gap_names or name in fenced_names:
            continue
        if name in BASIC_COLORS:
            continue
        record = id_by_name.get(name)
        if record is None or not record["has_definition"]:
            raise ValueError(f"pool card has no definition in id export: {name!r}")
        entry = {
            "card_id": record["card_id"],
            "name": record["name"],
            "colors": list(record["colors"]),
            "is_land": record["is_land"],
        }
        if record["is_land"]:
            nonbasic_lands.append(entry)
        else:
            spells.append(entry)

    basics: list[dict[str, Any]] = []
    for basic_name, color in BASIC_COLORS.items():
        record = id_by_name.get(basic_name)
        if record is None or not record["has_definition"]:
            raise ValueError(f"basic land has no definition in id export: {basic_name!r}")
        if not record["is_land"]:
            raise ValueError(f"basic land is not marked as land in id export: {basic_name!r}")
        basics.append(
            {
                "card_id": record["card_id"],
                "name": record["name"],
                "color": color,
                "colors": list(record["colors"]),
                "is_land": True,
            }
        )

    spells.sort(key=lambda item: item["card_id"])
    nonbasic_lands.sort(key=lambda item: item["card_id"])
    basics.sort(key=lambda item: item["card_id"])

    return {
        "schema": POOL_SCHEMA,
        "kernel_carddb_hash": carddb_hash,
        "input_sha256": {
            "cube_list": input_sha256["cube_list"],
            "gaps": input_sha256["gaps"],
            "id_export": input_sha256["id_export"],
            "fence": input_sha256["fence"],
        },
        "fenced": fenced_entries,
        "spells": spells,
        "nonbasic_lands": nonbasic_lands,
        "basics": basics,
    }


def build_pool_from_paths(
    cube_list_path: Path | str,
    gaps_path: Path | str,
    id_export_path: Path | str,
    fence_path: Path | str,
    expected_carddb_hash: str | None = None,
) -> dict[str, Any]:
    cube_doc, cube_sha = _read_json_with_sha256(Path(cube_list_path))
    gaps_doc, gaps_sha = _read_json_with_sha256(Path(gaps_path))
    export_doc, export_sha = _read_json_with_sha256(Path(id_export_path))
    fence_doc, fence_sha = _read_json_with_sha256(Path(fence_path))
    return build_pool(
        cube_doc,
        gaps_doc,
        export_doc,
        fence_doc,
        input_sha256={
            "cube_list": cube_sha,
            "gaps": gaps_sha,
            "id_export": export_sha,
            "fence": fence_sha,
        },
        expected_carddb_hash=expected_carddb_hash,
    )


def serialize_json(doc: dict[str, Any]) -> str:
    return json.dumps(doc, indent=2, ensure_ascii=False) + "\n"


def write_output(text: str, out: str | Path | None) -> None:
    """Write `text` as UTF-8 bytes with LF line endings on every OS, to `out` or to stdout."""
    data = text.encode("utf-8")
    if out is not None:
        out_path = Path(out)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_bytes(data)
        return
    buffer = getattr(sys.stdout, "buffer", None)
    if buffer is None:
        # A text-only stream such as io.StringIO does no newline translation.
        sys.stdout.write(text)
        return
    sys.stdout.flush()
    buffer.write(data)
    buffer.flush()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cube-list", required=True, help="Cube list JSON file (cube_list/v1)")
    parser.add_argument("--gaps", required=True, help="Coverage gap record JSON file (cube_coverage_gaps/v1)")
    parser.add_argument("--id-export", required=True, help="Kernel card id export JSON from cube_card_ids_v1")
    parser.add_argument("--fence", required=True, help="Fence JSON file (list of {name, reason}, may be empty)")
    parser.add_argument(
        "--carddb-hash",
        default=None,
        help="Expected kernel_carddb_hash; refuses if the export hash differs",
    )
    parser.add_argument("--out", default=None, help="Output pool JSON path (prints to stdout if omitted)")
    args = parser.parse_args(argv)

    try:
        pool = build_pool_from_paths(
            args.cube_list,
            args.gaps,
            args.id_export,
            args.fence,
            expected_carddb_hash=args.carddb_hash,
        )
    except (OSError, ValueError) as exc:
        print(f"pool.py: {exc}", file=sys.stderr)
        return 1

    write_output(serialize_json(pool), args.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
