"""RS0 readout (collab LANES/reverse-search-20261029/RS0-PLAN.md sections 7 and 8). Measurement only.

usage: report.py ROWS.jsonl... [--json OUT]

Reads `s4a-reach` rows. Unit: the game. For each (stratum, role, budget) a
game's value is the mean of its seeds' indicators; intervals are a
game-cluster percentile bootstrap (10,000 resamples, numpy default_rng
20261029) that resamples whole games. Flags read Spy O1 at k2:
High (lower bound >= 0.8), Low (upper bound <= 0.2), Intermediate (interval
inside (0.2, 0.8)), otherwise Uncertain.
"""
from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path

import numpy as np

RESAMPLES = 10_000
SEED = 20261029
OUTCOMES = ("o1", "o2_valid", "o3", "broad_self_chosen", "broad_hero_win")


def flag(lo: float, hi: float) -> str:
    if lo >= 0.8:
        return "High"
    if hi <= 0.2:
        return "Low"
    if lo > 0.2 and hi < 0.8:
        return "Intermediate"
    return "Uncertain"


def bootstrap(values: dict[str, float]) -> dict:
    """Mean over games with a game-cluster percentile interval."""
    games = sorted(values)
    x = np.array([values[g] for g in games], dtype=float)
    if len(x) == 0:
        return {"games": 0, "mean": None, "ci95": None}
    rng = np.random.default_rng(SEED)
    idx = rng.integers(0, len(x), size=(RESAMPLES, len(x)))
    means = x[idx].mean(axis=1)
    lo, hi = np.percentile(means, [2.5, 97.5])
    return {"games": len(x), "mean": float(x.mean()), "ci95": [float(lo), float(hi)]}


def indicators(row: dict, stratum: str) -> dict:
    """A failed search (incomplete or faulted) counts as not found for every outcome."""
    if row.get("failed"):
        return {name: False for name in OUTCOMES}
    o1 = row["o1_spy"] if stratum != "burn" else bool(row.get("o1_burn"))
    return {"o1": o1, "o2_valid": bool(row["o2"].get("valid")), "o3": bool(row["o3"].get("found")),
            "broad_self_chosen": row.get("self_chosen_simulations", 0) > 0,
            "broad_hero_win": row.get("hero_wins", 0) > 0}


def summarize(rows: list[dict]) -> dict:
    roots = {r["root_id"]: r for r in rows if r["kind"] == "root"}
    errors = [r for r in rows if r["kind"] == "error"]
    # (stratum, role, budget) -> game -> outcome -> [seed values]
    cells: dict = defaultdict(lambda: defaultdict(lambda: defaultdict(list)))
    failed: dict = defaultdict(int)
    for r in rows:
        if r["kind"] != "search":
            continue
        root = roots[r["root_id"]]
        for role in root["roles"]:
            if not role["selected"]:
                continue
            ind = indicators(r, role["stratum"])
            failed[(role["stratum"], role["role"], r["budget"])] += int(bool(r.get("failed")))
            for name, v in ind.items():
                cells[(role["stratum"], role["role"], r["budget"])][root["game_id"]][name].append(float(v))
    table = []
    for (stratum, role, budget), games in sorted(cells.items()):
        seeds = [len(o["o1"]) for o in games.values()]
        # A missing search makes the cell incomplete: rerun, never average the survivors.
        entry = {"stratum": stratum, "role": role, "budget": budget, "failed_searches": failed[(stratum, role, budget)],
                 "incomplete": len(set(seeds)) > 1}
        for name in OUTCOMES:
            per_game = {g: float(np.mean(o[name])) for g, o in games.items() if o[name]}
            entry[name] = bootstrap(per_game)
        table.append(entry)
    flags = []
    for entry in table:
        if entry["stratum"] == "spy" and entry["role"] == "k2" and entry["o1"]["ci95"]:
            lo, hi = entry["o1"]["ci95"]
            flags.append({"budget": entry["budget"], "flag": flag(lo, hi), "mean": entry["o1"]["mean"],
                          "ci95": [lo, hi], "games": entry["o1"]["games"],
                          "wording": f"s4a from 2 hero turns earlier finds a verified completion in "
                                     f"{100 * entry['o1']['mean']:.0f}% of searches (95% CI {100 * lo:.0f}-{100 * hi:.0f}); "
                                     f"availability in the sampled worlds not measured"})
    return {"roots": len(roots), "errors": len(errors), "error_roots": sorted({e["root_id"] for e in errors}),
            "table": table, "flags": flags}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("rows", type=Path, nargs="+")
    ap.add_argument("--json", type=Path)
    a = ap.parse_args(argv)
    rows = [json.loads(line) for p in a.rows for line in p.read_text(encoding="utf-8").splitlines() if line.strip()]
    out = summarize(rows)
    text = json.dumps(out, indent=1, sort_keys=True)
    if a.json:
        a.json.write_text(text + "\n", encoding="utf-8")
    print(json.dumps(out["flags"], indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
