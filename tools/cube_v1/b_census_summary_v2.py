"""Summarize a drafted census and report exact A2.5 eligible card counts.

Use the pinned v1 normalizer and retain the v1 report fields. The versioned
tripwire section contains the eligible denominator and the complete per-card
counts before ranking. Lost and unrun slots do not enter either rate. Supply
missing slot indices from the launcher's records with --excluded-slots.
"""

from __future__ import annotations

import argparse
from collections import Counter
import heapq
import json
from pathlib import Path

import b_census_summary as v1


ELIGIBLE_SCHEMA = "mtg-kernel-census-b-eligible/v2"
EXCLUDED = frozenset({"process_died", "lost", "unrun"})


def summarise(rows, names=None):
    eligible_games = 0
    present = Counter()
    charged = Counter()

    def observe():
        nonlocal eligible_games
        for row in rows:
            end = row["end"]
            if end["kind"] not in EXCLUDED:
                eligible_games += 1
                present.update({card for deck in row["decks"] for card in deck})
                source = row.get("last_action_source")
                if source is not None and end["kind"] != "panic" and v1.stop_kind(end) is not None:
                    charged[source] += 1
            yield row

    summary = v1.summarise(observe(), names)
    summary["tripwire"] = {
        "schema": ELIGIBLE_SCHEMA,
        "games": eligible_games,
        "cards": [
            {"card": card, "games_present": present[card], "charged": charged[card]}
            for card in sorted(present)
        ],
    }
    return summary


def read_excluded_slots(path):
    """Read launcher-confirmed missing indices as {"lost":[...], "unrun":[...]}."""
    record = json.loads(path.read_bytes())
    if not isinstance(record, dict) or set(record) - {"lost", "unrun"}:
        raise ValueError("excluded slots must be an object with lost or unrun index lists")
    slots = {}
    for kind, indices in record.items():
        if not isinstance(indices, list):
            raise ValueError(f"{kind} indices must be a list")
        for index in indices:
            if type(index) is not int or index < 0 or index in slots:
                raise ValueError(f"invalid or duplicate excluded slot index: {index!r}")
            slots[index] = kind
    for index, kind in sorted(slots.items()):
        yield {
            "schema": v1.GAME_LINE_SCHEMA,
            "index": index,
            "end": {"kind": kind},
            "decks": [[], []],
            "completed": False,
            "stop_kind": kind,
        }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--games", nargs="+", required=True, type=Path)
    parser.add_argument("--record", nargs="*", default=[], type=Path)
    parser.add_argument("--excluded-slots", type=Path, help="JSON with missing lost or unrun indices")
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--markdown", type=Path)
    args = parser.parse_args(argv)

    names = v1.card_names(json.loads(path.read_bytes()) for path in args.record)
    rows = v1.read_lines(args.games)
    if args.excluded_slots is not None:
        rows = heapq.merge(rows, read_excluded_slots(args.excluded_slots), key=lambda row: row["index"])
    summary = summarise(rows, names)

    args.out.write_text(json.dumps(summary, indent=1) + "\n", encoding="utf-8", newline="\n")
    if args.markdown is not None:
        args.markdown.write_text(v1.markdown(summary), encoding="utf-8", newline="\n")
    print(json.dumps({key: summary[key] for key in ("games", "completed", "halted")}
                     | {"eligible_games": summary["tripwire"]["games"]}))
    return int(bool(summary["order_errors"] or summary["stop_kind_mismatches"]))


if __name__ == "__main__":
    raise SystemExit(main())
