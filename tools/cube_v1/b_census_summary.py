"""Summarise a cube B-run census (cube_b_census_v1 JSONL, declarations A2 and B).

Reads the game lines (one or more files, concatenated in index order) and reports:
completed and halted games; halted games by stop kind; every card, present in a deck and
charged as the last action's source (A2.5 item 4), with its denominator (the games whose
decks contain it); decision-count percentiles 50, 99 and 99.9; and the 8,192 warning (A2.1).
A panic charges no card (A2.5 item 4); the source of the last action before it is reported
apart, as `panic_last_source`.

Usage:
  b_census_summary.py --games OUT.jsonl [OUT2.jsonl ...] [--record RUN.json ...]
                      --out SUMMARY.json [--markdown SUMMARY.md]

The stop kind is recomputed from each line's raw `end` by the census normaliser
(`cause_of`, copied verbatim from census_summary.py), and every line's `stop_kind` from the
binary is checked against it; any difference is reported under `stop_kind_mismatches`.
"""

from __future__ import annotations

import argparse
import collections
import json
import math
import re
from pathlib import Path
from typing import Any, Iterable

GAME_LINE_SCHEMA = "cube_b_census_game/v1"
PERCENTILES = (50.0, 99.0, 99.9)
COUNTS = ("driver_decisions", "physical_decisions", "policy_steps")
WARNING_DECISIONS = 8192
WARNING_SHARE = 0.001
STACK_OVERFLOW = 3221225725
EXAMPLES = 8
TOP = 5
# As census_summary.py: basic lands are left out of a stop kind's "cards present" list.
BASIC_LAND_NAMES = frozenset({"Plains", "Island", "Swamp", "Mountain", "Forest"})


def cause_of(end):
    """Verbatim from census_summary.py (the earlier census's stop-kind normaliser)."""
    if end.get("end") == "terminal":
        if end.get("classification") == "natural":
            return None
        return re.sub(r"source:\d+", "source:*", end.get("reason", "halted"))
    if end.get("end") == "panic":
        return "panic: " + end.get("message", "")[:160]
    if end.get("end") == "error":
        return f"session error {end.get('code')}: {str(end.get('message', ''))[:160]}"
    return end.get("end", "unknown")


def census_end(end: dict[str, Any]) -> dict[str, Any]:
    """A cube_b_census_v1 `end` in the shape `cause_of` reads."""
    kind = end.get("kind")
    if kind == "terminal":
        return {"end": "terminal", "classification": end.get("classification"), "reason": end.get("reason", "halted")}
    if kind == "panic":
        return {"end": "panic", "message": end.get("message", "")}
    if kind == "session_error":
        return {"end": "error", "code": end.get("code"), "message": end.get("message", "")}
    if kind == "decision_cap":
        return {"end": "driver_decision_cap"}
    return {"end": kind if kind is not None else "unknown"}


def stop_kind(end: dict[str, Any]) -> str | None:
    """The stop kind of a game: None for a completed game (A2 P1: `natural_game_over`, the
    `natural` end), else `cause_of` of the end. Two cases `cause_of` never saw: a process
    death is named as census_summary.py names one, and a terminal reply classified natural
    without `natural_game_over` is halted (P1), named by its normalised reason."""
    kind = end.get("kind")
    if kind == "natural":
        return None
    if kind == "process_died":
        code = end.get("exit_code")
        if code is None:
            return f"process dies: signal {end.get('signal')}"
        return f"process dies: {'stack overflow (0xC00000FD)' if code == STACK_OVERFLOW else f'exit {code}'}"
    cause = cause_of(census_end(end))
    if cause is None:
        return re.sub(r"source:\d+", "source:*", end.get("reason", "halted"))
    return cause


def percentile(sorted_values: list[int], p: float) -> int | None:
    """Nearest rank: the smallest value with at least p% of the values at or below it."""
    if not sorted_values:
        return None
    rank = max(1, math.ceil(p / 100.0 * len(sorted_values) - 1e-9))
    return sorted_values[min(rank, len(sorted_values)) - 1]


def distribution(values: Iterable[int]) -> dict[str, Any]:
    ordered = sorted(values)
    out: dict[str, Any] = {"n": len(ordered)}
    for p in PERCENTILES:
        out[f"p{p:g}"] = percentile(ordered, p)
    out["max"] = ordered[-1] if ordered else None
    return out


def read_lines(paths: Iterable[Path]) -> Iterable[dict[str, Any]]:
    for path in paths:
        with open(path, encoding="utf-8") as handle:
            for number, text in enumerate(handle, 1):
                if not text.strip():
                    continue
                row = json.loads(text)
                if row.get("schema") != GAME_LINE_SCHEMA:
                    raise ValueError(f"{path}:{number}: schema {row.get('schema')!r} is not {GAME_LINE_SCHEMA}")
                yield row


def card_names(records: Iterable[dict[str, Any]]) -> dict[int, str]:
    names: dict[int, str] = {}
    for record in records:
        for key, name in (record.get("card_names") or {}).items():
            names[int(key)] = name
    return names


def summarise(rows: Iterable[dict[str, Any]], names: dict[int, str] | None = None) -> dict[str, Any]:
    names = names or {}
    total = completed = draws = 0
    expected_index: int | None = None
    order_errors: list[str] = []
    mismatches: list[dict[str, Any]] = []
    by_kind: dict[str, list[dict[str, Any]]] = collections.defaultdict(list)
    present = collections.Counter()
    present_halted = collections.Counter()
    charged = collections.Counter()
    charged_recent = collections.Counter()
    panic_source = collections.Counter()
    counts_all: dict[str, list[int]] = {key: [] for key in COUNTS}
    counts_completed: dict[str, list[int]] = {key: [] for key in COUNTS}
    over_warning = 0
    for row in rows:
        index = row["index"]
        if expected_index is not None and index != expected_index:
            order_errors.append(f"index {index} follows {expected_index - 1}")
        expected_index = index + 1
        total += 1
        kind = stop_kind(row["end"])
        if kind != row.get("stop_kind"):
            mismatches.append({"index": index, "line": row.get("stop_kind"), "normaliser": kind})
        if (kind is None) != bool(row.get("completed")):
            order_errors.append(f"index {index}: completed {row.get('completed')!r} but stop kind {kind!r}")
        cards = {card for deck in row["decks"] for card in deck}
        present.update(cards)
        for key in COUNTS:
            if row.get(key) is not None:
                counts_all[key].append(row[key])
        if kind is None:
            completed += 1
            draws += bool(row.get("draw"))
            for key in COUNTS:
                if row.get(key) is not None:
                    counts_completed[key].append(row[key])
            if (row.get("driver_decisions") or 0) > WARNING_DECISIONS:
                over_warning += 1
            continue
        present_halted.update(cards)
        source = row.get("last_action_source")
        if row["end"].get("kind") == "panic":
            # A2.5 item 4 names a panic as a halt that charges no card; the source the
            # driver saw before the panic is kept apart.
            if source is not None:
                panic_source[source] += 1
            source = None
        if source is not None:
            charged[source] += 1
        if row.get("recent_action_source") is not None:
            charged_recent[row["recent_action_source"]] += 1
        by_kind[kind].append({"index": index, "cards": cards, "source": source,
                              "panic_source": row.get("last_action_source") if row["end"].get("kind") == "panic" else None})
    if total == 0:
        raise ValueError("no game lines")

    def name(card: int) -> str:
        return names.get(card, f"#{card}")

    kinds = []
    for kind, games in sorted(by_kind.items(), key=lambda item: (-len(item[1]), item[0])):
        in_kind = collections.Counter(card for game in games for card in game["cards"])
        sources = collections.Counter(game["source"] for game in games if game["source"] is not None)
        kinds.append({
            "stop_kind": kind,
            "games": len(games),
            "share": len(games) / total,
            "replay_index": games[0]["index"],
            "example_indices": [game["index"] for game in games[:EXAMPLES]],
            "cards_present": [
                {"card": card, "name": name(card), "games": k, "share_of_kind_games": k / len(games)}
                for card, k in sorted(in_kind.items(), key=lambda item: (-item[1], item[0]))
                if names.get(card) not in BASIC_LAND_NAMES
            ][:TOP],
            "panic_last_source": [
                {"card": card, "name": name(card), "games": k}
                for card, k in sorted(
                    collections.Counter(g["panic_source"] for g in games if g["panic_source"] is not None).items(),
                    key=lambda item: (-item[1], item[0]),
                )[:TOP]
            ],
            "last_action_source": [
                {"card": card, "name": name(card), "games": k}
                for card, k in sorted(sources.items(), key=lambda item: (-item[1], item[0]))[:TOP]
            ],
        })
    cards = []
    for card in sorted(present, key=lambda c: (-charged[c], -present_halted[c], c)):
        games = present[card]
        cards.append({
            "card": card,
            "name": name(card),
            "games_present": games,
            "halted_present": present_halted[card],
            "halted_present_rate": present_halted[card] / games,
            "charged": charged[card],
            "charged_rate": charged[card] / games,
            "charged_recent": charged_recent[card],
            "panic_last_source": panic_source[card],
        })
    unpresent_charges = sorted(set(charged) - set(present))
    if unpresent_charges:
        order_errors.append(f"charged cards in no deck: {unpresent_charges}")
    return {
        "games": total,
        "completed": completed,
        "halted": total - completed,
        "completed_share": completed / total,
        "halted_share": (total - completed) / total,
        "natural_draws": draws,
        "index_start": expected_index - total if expected_index is not None else None,
        "index_end": expected_index,
        "order_errors": order_errors,
        "stop_kind_mismatches": mismatches,
        "stop_kinds": kinds,
        "cards": cards,
        "percentiles_all_games": {key: distribution(values) for key, values in counts_all.items()},
        "percentiles_completed_games": {key: distribution(values) for key, values in counts_completed.items()},
        "warning_8192": {
            "completed_over": over_warning,
            "share_of_completed": over_warning / completed if completed else None,
            "fires": completed > 0 and over_warning / completed > WARNING_SHARE,
        },
    }


def markdown(summary: dict[str, Any]) -> str:
    g = summary["games"]
    lines = [
        f"# Cube B census: {g:,} games\n",
        f"Completed {summary['completed']:,} ({summary['completed_share']:.3%}), of which natural draws "
        f"{summary['natural_draws']:,}; halted {summary['halted']:,} ({summary['halted_share']:.3%}).\n",
        "| Stop kind | Games | Share | Cards in most of these games | Last action's source | Replay index |",
        "| --- | ---: | ---: | --- | --- | ---: |",
    ]
    for kind in summary["stop_kinds"]:
        present = ", ".join(f"{c['name']} {c['share_of_kind_games']:.0%}" for c in kind["cards_present"][:3])
        sources = ", ".join(f"{c['name']} {c['games']}" for c in kind["last_action_source"][:3])
        lines.append(
            f"| `{kind['stop_kind']}` | {kind['games']:,} | {kind['share']:.3%} | {present} | {sources} | {kind['replay_index']} |"
        )
    lines += ["", "| Count | Games | p50 | p99 | p99.9 | max |", "| --- | --- | ---: | ---: | ---: | ---: |"]
    for scope in ("percentiles_all_games", "percentiles_completed_games"):
        for key, dist in summary[scope].items():
            lines.append(
                f"| {key} | {scope.split('_')[1]} | {dist['p50']} | {dist['p99']} | {dist['p99.9']} | {dist['max']} |"
            )
    charged = [c for c in summary["cards"] if c["charged"]]
    lines += ["", "| Card | Games with it | Halted with it | Charged | Charged rate |", "| --- | ---: | ---: | ---: | ---: |"]
    for card in charged:
        lines.append(
            f"| {card['name']} | {card['games_present']:,} | {card['halted_present']:,} | {card['charged']:,} | {card['charged_rate']:.4%} |"
        )
    warning = summary["warning_8192"]
    lines.append(
        f"\n8,192 warning: {warning['completed_over']:,} completed games over 8,192 driver decisions"
        f"{' (fires)' if warning['fires'] else ''}."
    )
    if summary["order_errors"] or summary["stop_kind_mismatches"]:
        lines.append(
            f"\nChecks failed: {len(summary['order_errors'])} order errors, "
            f"{len(summary['stop_kind_mismatches'])} stop-kind mismatches (see the JSON)."
        )
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--games", nargs="+", required=True, type=Path)
    ap.add_argument("--record", nargs="*", default=[], type=Path)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--markdown", type=Path)
    a = ap.parse_args(argv)
    names = card_names(json.loads(path.read_text(encoding="utf-8")) for path in a.record)
    summary = summarise(read_lines(a.games), names)
    a.out.write_text(json.dumps(summary, indent=1) + "\n", encoding="utf-8", newline="\n")
    if a.markdown is not None:
        a.markdown.write_text(markdown(summary), encoding="utf-8", newline="\n")
    print(json.dumps({"games": summary["games"], "completed": summary["completed"],
                      "halted": summary["halted"], "stop_kinds": len(summary["stop_kinds"]),
                      "order_errors": len(summary["order_errors"]),
                      "stop_kind_mismatches": len(summary["stop_kind_mismatches"])}))
    return 1 if summary["order_errors"] or summary["stop_kind_mismatches"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
