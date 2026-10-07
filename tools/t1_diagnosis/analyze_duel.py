"""Summarize paired `duel` census runs: one candidate pilots a deck against a
fixed reference policy on game-index-paired seeds. Reports win rate per
candidate and opponent deck, paired differences against a baseline run with a
game-level bootstrap, and how the pilot used tracked cards.

Usage: python analyze_duel.py BASELINE.jsonl CANDIDATE.jsonl [...]
"""
from __future__ import annotations

import collections
import json
import random
import sys
from pathlib import Path

# Each Gate already taps for one of CawGates' colours and adds one chosen
# colour; the useful choice is the other one (Citadel Gate: W + U, Sea Gate:
# U + W).
USEFUL_GATE_COLOUR = {"Citadel Gate": "U", "Sea Gate": "W"}
# Main damage colour of each opposing deck, for Prismatic Strands' colour
# choice. Affinity (mostly colourless) and Wildfire (mixed) are left out.
STRANDS_COLOUR = {"Burn": "R", "Rally": "R", "Elves": "G", "Spy": "G", "Terror": "U", "Faeries": "U"}
COMBAT = {"DeclareAttackers", "DeclareBlockers", "CombatDamage", "FirstStrikeDamage"}


def load(path):
    rows = {}
    errors = 0
    for line in Path(path).open():
        r = json.loads(line)
        if r["kind"] == "duel":
            rows[r["game"]] = r
        else:
            errors += 1
    return rows, errors


def behaviour(rows):
    n = len(rows)
    c = collections.Counter()
    for r in rows.values():
        k = r["counts"]
        for key, v in k.items():
            parts = key.split("|")
            if parts[0] == "Prismatic Strands" and parts[1] == "cast_spell":
                c["strands_cast"] += v
                if parts[3] == "opp" and parts[2] in COMBAT:
                    c["strands_opp_combat"] += v
                elif parts[3] == "opp":
                    c["strands_opp_other"] += v
                else:
                    c["strands_own_turn"] += v
            if parts[0] == "Basilisk Gate" and parts[1] == "activate_ability":
                c["basilisk_pumps"] += v
            if parts[1] == "choose_effect_color" and parts[0] in USEFUL_GATE_COLOUR:
                c["gate_colour_choices"] += v
                if parts[4] == USEFUL_GATE_COLOUR[parts[0]]:
                    c["gate_colour_useful"] += v
            if (
                parts[0] == "Prismatic Strands"
                and parts[1] == "choose_effect_color"
                and r["opp_deck"] in STRANDS_COLOUR
            ):
                c["strands_colour_choices"] += v
                if parts[4] == STRANDS_COLOUR[r["opp_deck"]]:
                    c["strands_colour_matches_opponent"] += v
        pumps = set(r["pump_turns"])
        c["pump_then_attack_turns"] += len(pumps & set(r["attacked_turns"]))
        c["attack_turns"] += len(r["attacked_turns"])
        c["turns"] += r["turns"]
        c["opp_damage"] += 20 - r["final_life"][1]
        c["decisions"] += r["pilot_decisions"]
    out = {k: v / n for k, v in sorted(c.items())}
    if c["gate_colour_choices"]:
        out["gate_colour_useful_share"] = c["gate_colour_useful"] / c["gate_colour_choices"]
    if c["strands_colour_choices"]:
        out["strands_colour_match_share"] = (
            c["strands_colour_matches_opponent"] / c["strands_colour_choices"]
        )
    return out


def main():
    base_path, *cands = sys.argv[1:]
    base, base_err = load(base_path)
    runs = [(Path(base_path).stem, base, base_err)] + [
        (Path(p).stem, *load(p)) for p in cands
    ]
    games = sorted(set.intersection(*(set(r) for _, r, _ in runs)))
    print(f"paired games: {len(games)}")
    decks = sorted({base[g]["opp_deck"] for g in games})
    header = (
        f"{'run':<14}{'errors':>7}{'win%':>8}{'no-Spy%':>9}"
        + "".join(f"{d[:8]:>10}" for d in decks)
    )
    print(header)
    for name, rows, err in runs:
        score = sum(rows[g]["score"] for g in games) / len(games)
        trained = [g for g in games if rows[g]["opp_deck"] != "Spy"]
        no_spy = sum(rows[g]["score"] for g in trained) / len(trained)
        per = []
        for d in decks:
            gs = [g for g in games if rows[g]["opp_deck"] == d]
            per.append(100 * sum(rows[g]["score"] for g in gs) / len(gs))
        print(
            f"{name:<14}{err:>7}{100 * score:>8.2f}{100 * no_spy:>9.2f}"
            + "".join(f"{x:>10.1f}" for x in per)
        )
    rng = random.Random(2026100711)
    print("\npaired difference vs baseline (pp), 95% game bootstrap")
    diffs_all = []
    for name, rows, _ in runs[1:]:
        d = [rows[g]["score"] - base[g]["score"] for g in games]
        diffs_all.append(d)
        boots = sorted(
            sum(d[rng.randrange(len(d))] for _ in d) / len(d) for _ in range(2000)
        )
        mean = sum(d) / len(d)
        print(f"{name:<14}{100 * mean:+7.2f}  [{100 * boots[50]:+.2f}, {100 * boots[1949]:+.2f}]")
    if len(diffs_all) > 1:
        pooled = [sum(x) / len(x) for x in zip(*diffs_all)]
        boots = sorted(
            sum(pooled[rng.randrange(len(pooled))] for _ in pooled) / len(pooled)
            for _ in range(2000)
        )
        per_run = [100 * sum(d) / len(d) for d in diffs_all]
        print(
            f"{'mean of runs':<14}{100 * sum(pooled) / len(pooled):+7.2f}  "
            f"[{100 * boots[50]:+.2f}, {100 * boots[1949]:+.2f}] (games only; "
            f"run spread {min(per_run):+.2f}..{max(per_run):+.2f})"
        )
    print("\nbehaviour per game")
    table = {name: behaviour({g: rows[g] for g in games}) for name, rows, _ in runs}
    keys = sorted(set().union(*table.values()))
    print(f"{'':<26}" + "".join(f"{n[:10]:>11}" for n, _, _ in runs))
    for k in keys:
        print(f"{k:<26}" + "".join(f"{table[n].get(k, 0):>11.3f}" for n, _, _ in runs))


if __name__ == "__main__":
    main()
