"""Measure engine card coverage of a Pauper, Legacy and Vintage metagame snapshot.

Input tooling only: no engine build, process or network. A snapshot lists each
format's archetypes with their metagame share and one representative decklist.
Every card is classified against the `eternal-fixtures` build, whose registry
is the Pauper prefix, the Standard registry and `data/eternal_v1/cards_v1.json`:

- ``full``: Full in that build.
- ``fdn_only``: Full only in the FDN registry. Its program exists, so adding it
  to the eternal registry is a copy, not new rules work.
- ``missing``: no Full definition anywhere.

A deck is playable when every mainboard card is ``full``. Shares are the
snapshot's metagame percentages, so the playable share is the fraction of the
field the eternal build can play.
"""

from __future__ import annotations

import argparse
from collections import defaultdict
import json
from pathlib import Path
import re
import sys
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
ETERNAL_REGISTRIES = (
    REPO_ROOT / "data/cards_v1.json",
    REPO_ROOT / "data/standard/magezero_v1/cards_v1.json",
    REPO_ROOT / "data/eternal_v1/cards_v1.json",
)
FDN_REGISTRY = REPO_ROOT / "data/limited/fdn_v1/cards_v1.json"
STATUSES = ("full", "fdn_only", "missing")
ROW = re.compile(r"([1-9][0-9]*)\s+(.+)")


def parse_list(text: str) -> dict[str, list[list[Any]]]:
    """Parse a plain decklist; a blank line or a "Sideboard" row starts the sideboard."""
    zones: dict[str, list[list[Any]]] = {"mainboard": [], "sideboard": []}
    zone = "mainboard"
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.lower().startswith("sideboard"):
            if zones["mainboard"]:
                zone = "sideboard"
            continue
        match = ROW.fullmatch(line)
        if match is None:
            raise ValueError(f"unparsed decklist row {line!r}")
        zones[zone].append([int(match.group(1)), match.group(2).strip()])
    return zones


def full_names(path: Path) -> set[str]:
    cards = json.loads(path.read_text(encoding="utf-8"))["cards"]
    return {card["name"] for card in cards if card.get("engine_capability") == "full"}


def load_status() -> tuple[set[str], set[str]]:
    full: set[str] = set()
    for path in ETERNAL_REGISTRIES:
        full |= full_names(path)
    return full, full_names(FDN_REGISTRY) - full


def classify(name: str, full: set[str], fdn_only: set[str]) -> str:
    if name in full:
        return "full"
    return "fdn_only" if name in fdn_only else "missing"


def census(snapshot: dict[str, Any], full: set[str], fdn_only: set[str]) -> dict[str, Any]:
    formats = {}
    union: dict[str, str] = {}
    for fmt, archetypes in snapshot["formats"].items():
        share_by_card: dict[str, float] = defaultdict(float)
        rows = []
        playable = 0.0
        for archetype in archetypes:
            main = {name for _, name in archetype["mainboard"]}
            side = {name for _, name in archetype["sideboard"]} - main
            status = {name: classify(name, full, fdn_only) for name in main | side}
            union.update(status)
            missing_main = sorted(name for name in main if status[name] != "full")
            if not missing_main:
                playable += archetype["share"]
            for name, value in status.items():
                if value != "full":
                    share_by_card[name] += archetype["share"]
            rows.append({
                "archetype": archetype["archetype"],
                "share": archetype["share"],
                "mainboard_distinct": len(main),
                "mainboard_unsupported": len(missing_main),
                "sideboard_unsupported": sum(1 for name in side if status[name] != "full"),
                "unsupported_mainboard_cards": missing_main,
            })
        distinct = {name for archetype in archetypes for zone in ("mainboard", "sideboard")
                    for _, name in archetype[zone]}
        counts = {key: sum(1 for name in distinct if classify(name, full, fdn_only) == key)
                  for key in STATUSES}
        formats[fmt] = {
            "archetypes": len(archetypes),
            "listed_share": round(sum(a["share"] for a in archetypes), 1),
            "playable_share": round(playable, 1),
            "distinct_cards": len(distinct),
            "card_status": counts,
            "unsupported_by_share": [
                [name, round(share, 1), classify(name, full, fdn_only)]
                for name, share in sorted(share_by_card.items(), key=lambda kv: (-kv[1], kv[0]))
            ],
            "decks": rows,
        }
    totals = {key: sum(1 for value in union.values() if value == key) for key in STATUSES}
    return {"schema": "eternal_meta_census/v1", "source": snapshot["source"],
            "captured": snapshot["captured"], "union_card_status": totals, "formats": formats}


def markdown(report: dict[str, Any], top: int) -> str:
    lines = [f"Snapshot {report['source']} captured {report['captured']}.", "",
             "| Format | Archetypes | Distinct cards | Full | FDN only | Missing | Playable share |",
             "| --- | ---: | ---: | ---: | ---: | ---: | ---: |"]
    for fmt, value in report["formats"].items():
        status = value["card_status"]
        lines.append(f"| {fmt} | {value['archetypes']} | {value['distinct_cards']} | {status['full']} | "
                     f"{status['fdn_only']} | {status['missing']} | {value['playable_share']}% of "
                     f"{value['listed_share']}% |")
    for fmt, value in report["formats"].items():
        lines += ["", f"### {fmt}: unsupported cards by metagame share", ""]
        lines += [f"- {name} ({share}%, {status})" for name, share, status in value["unsupported_by_share"][:top]]
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("snapshot", type=Path)
    parser.add_argument("--json-out", type=Path)
    parser.add_argument("--markdown", action="store_true")
    parser.add_argument("--top", type=int, default=25)
    args = parser.parse_args(argv)
    snapshot = json.loads(args.snapshot.read_text(encoding="utf-8"))
    report = census(snapshot, *load_status())
    if args.json_out:
        args.json_out.write_text(json.dumps(report, indent=1) + "\n", encoding="utf-8")
    if args.markdown:
        sys.stdout.write(markdown(report, args.top))
    else:
        for fmt, value in report["formats"].items():
            print(fmt, value["card_status"], f"playable {value['playable_share']}% of {value['listed_share']}%")
        print("union", report["union_card_status"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
