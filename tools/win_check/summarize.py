"""Missed-forced-win table from `s4a-win-check` rows. Measurement only.

usage: summarize.py ROWS.jsonl... [--labels LABELS.jsonl...] [--json OUT]

Opportunity: one (game, turn) in which at least one focal decision is
certified a win available this turn; the class and line come from the first
certified decision's line (the line the search found first, not every line).
Won that turn: the focal seat won the game before the turn ended, by any
means. Missed: it did not. Oracle and robust estimands are counted separately; a robust
"none" is certified only through the oracle (a world without a win).

Classes (declared, from the certified line and the focal deck), first match:
spy_line (the line uses Balustrade Spy), dread_return_outside_spy (Dread
Return without Spy), rally_elves_kill (focal deck Rally or Elves),
burn_lethal (focal deck Burn, no attackers declared), combat_lethal
(attackers declared), other. Decks or classes without events are "not
measured". Intervals are 95% Wilson.
"""
from __future__ import annotations

import argparse
import json
import math
import re
from collections import Counter, defaultdict
from pathlib import Path

CARDS = json.loads((Path(__file__).resolve().parents[2] / "data" / "cards_v1.json").read_text(encoding="utf-8"))["cards"]
NAME = {i: c["name"] for i, c in enumerate(CARDS)}
ID = {c["name"]: i for i, c in enumerate(CARDS)}


def wilson(k: int, n: int) -> list[float] | None:
    if n == 0:
        return None
    z, p = 1.959964, k / n
    centre = (p + z * z / (2 * n)) / (1 + z * z / n)
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / (1 + z * z / n)
    return [round(max(0.0, centre - half), 4), round(min(1.0, centre + half), 4)]


def cards_in(line: list[str]) -> set[str]:
    return {NAME.get(int(i), f"#{i}") for step in line for i in re.findall(r"card_db_id: (\d+)", step)}


def attacks(line: list[str]) -> bool:
    return any(re.search(r"DeclareAttackers \{[^}]*attackers: \[[^\]]|ChooseAttackerInclusion \{.*include: true", step)
               for step in line)


def classify(line: list[str], deck: str) -> str:
    names = cards_in(line)
    if "Balustrade Spy" in names:
        return "spy_line"
    if "Dread Return" in names:
        return "dread_return_outside_spy"
    if deck in ("Rally", "Elves"):
        return "rally_elves_kill"
    if deck == "Burn" and not attacks(line):
        return "burn_lethal"
    if attacks(line):
        return "combat_lethal"
    return "other"


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("rows", type=Path, nargs="+")
    parser.add_argument("--labels", type=Path, nargs="*", default=[], help="panel labels.jsonl for the agreement check")
    parser.add_argument("--json", type=Path)
    args = parser.parse_args(argv)
    games, decisions, errors, torn = {}, defaultdict(list), [], 0
    for path in args.rows:
        for line in path.read_text(encoding="utf-8").splitlines():
            try:
                row = json.loads(line)
            except json.JSONDecodeError:
                torn += 1  # a line cut by an interrupted writer
                continue
            if row["kind"] == "game":
                games[row["id"]] = row
            elif row["kind"] == "decision":
                decisions[row["id"]].append(row)
            else:
                errors.append(row)
    panel = {}
    for path in args.labels:
        for line in path.read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            panel[row["id"]] = row
    report = {"games": len(games), "replay_errors": len(errors), "torn_lines": torn, "runtime": sorted({json.dumps(g["runtime"]) for g in games.values()})}
    focal = [d for g in games for d in decisions[g]]
    spy_taken = set()
    for estimand in ("oracle", "robust"):
        verdicts = Counter((d[estimand] or {}).get("verdict", "missing") for d in focal)
        reasons = Counter((d[estimand] or {}).get("unknown_reason") for d in focal if (d[estimand] or {}).get("verdict") == "unknown")
        if estimand == "robust":
            # Robust none is certified only by an oracle refutation.
            certified = sum(d["robust"] and (d["robust"]["verdict"] == "proven" or d["oracle"]["verdict"] == "refuted") for d in focal)
        else:
            certified = sum(d["oracle"]["verdict"] != "unknown" for d in focal)
        table = defaultdict(lambda: {"available": 0, "won_that_turn": 0, "missed": 0, "games_with": set(), "missed_refs": [], "won_refs": []})
        for game_id, game in games.items():
            deck = game["deck_ids"][game["focal"]].split("/")[0]
            turns = defaultdict(list)
            for d in decisions[game_id]:
                if (d[estimand] or {}).get("verdict") == "proven":
                    turns[d["turn"]].append(d)
            for turn, rows in sorted(turns.items()):
                cls = classify(rows[0][estimand]["line"], deck)
                cell = table[(deck, cls)]
                cell["available"] += 1
                cell["games_with"].add(game_id)
                won = game["win"] and game["final_turn"] == turn
                cell["won_that_turn" if won else "missed"] += 1
                if won and cls == "spy_line" and estimand == "oracle":
                    spy_taken.add(game_id)
                cell["won_refs" if won else "missed_refs"].append({"game": game_id, "turn": turn, "decision": rows[0]["index"]})
        scheduled = Counter(g["deck_ids"][g["focal"]].split("/")[0] for g in games.values())
        rows = []
        for (deck, cls), cell in sorted(table.items()):
            n = scheduled[deck]
            rows.append({"deck": deck, "class": cls, "scheduled_games": n, "opportunities": cell["available"],
                         "availability_per_1000_games": round(1000 * len(cell["games_with"]) / n, 1),
                         "availability_ci": [round(1000 * x, 1) for x in wilson(len(cell["games_with"]), n)],
                         "won_that_turn": cell["won_that_turn"], "missed": cell["missed"],
                         "miss_rate": round(cell["missed"] / cell["available"], 4), "miss_rate_ci": wilson(cell["missed"], cell["available"]),
                         "missed_per_1000_games": round(1000 * cell["missed"] / n, 1),
                         "won_refs": cell["won_refs"][:20], "missed_refs": cell["missed_refs"][:20]})
        by_type = defaultdict(Counter)
        for d in focal:
            kind = ("own_turn" if d["active"] == d["focal"] else "opponent_turn", "legal>=5" if d["legal"] >= 5 else "legal<5")
            ok = (d["robust"] and (d["robust"]["verdict"] == "proven" or d["oracle"]["verdict"] == "refuted")) if estimand == "robust" else d["oracle"]["verdict"] != "unknown"
            by_type["/".join(kind)]["certified" if ok else "unknown"] += 1
        certified_by_type = {k: {"decisions": v["certified"] + v["unknown"], "certified_share": round(v["certified"] / (v["certified"] + v["unknown"]), 4)}
                             for k, v in sorted(by_type.items())}
        report[estimand] = {"focal_decisions": len(focal), "verdicts": dict(verdicts), "unknown_reasons": dict(reasons),
                            "certified_by_type": certified_by_type,
                            "certified_share": round(certified / max(1, len(focal)), 4), "table": rows,
                            "not_measured": sorted(d for d in scheduled if not any(r["deck"] == d for r in rows))}
    # EX1 menu strata (decisions with two or more legal actions) per deck and seat role.
    strata = defaultdict(Counter)
    for g in games.values():
        decks = [d.split("/")[0] for d in g["deck_ids"]]
        for key, n in (g.get("strata") or {}).items():
            role, stratum = key.split(":")
            deck = decks[g["focal"]] if role == "focal" else decks[1 - g["focal"]]
            strata[f"{deck} ({role})"][stratum] += n
    report["ex1_strata"] = {k: dict(v) for k, v in sorted(strata.items())}
    # Agreement: combo_won games must have a taken spy_line opportunity, and vice versa (oracle).
    disagree = []
    for game_id, game in games.items():
        taken_spy = game_id in spy_taken
        label = panel.get(game_id, {}).get("combo_won", game["combo_won"])
        if label != game["combo_won"] or (label and not taken_spy):
            disagree.append({"game": game_id, "panel_combo_won": label, "replay_combo_won": game["combo_won"], "taken_spy_line": taken_spy})
    report["label_agreement"] = {"games_with_combo_won": sum(g["combo_won"] for g in games.values()), "disagreements": disagree}
    text = json.dumps(report, indent=1)
    if args.json:
        args.json.write_text(text, encoding="utf-8")
    print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
