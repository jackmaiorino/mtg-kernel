"""RS0 readout (collab LANES/reverse-search-20261029/RS0-PLAN.md sections 7 and 8). Measurement only.

usage: report.py ROWS.jsonl... --roots ROOTS.jsonl [--budgets 64000,256000] [--seeds 0,1,2]
                 [--large-budget 1024000] [--json OUT]

Reads `s4a-reach` rows and the selected roots they were run from, and checks
the expected search matrix first: every selected root needs exactly one
search row per (budget, seed), and the Spy k2 roots one more at the large
budget with seed 0. A duplicate is an error; a missing search makes its
cells incomplete, and an incomplete cell gets no estimate, interval or flag.

Unit: the game. A game's value in a (stratum, role, budget) cell is the mean
of its seeds' indicators; a failed search (incomplete or faulted) counts as
not found. Intervals come from one shared game-cluster percentile
bootstrap (10,000 replicates, numpy default_rng 20261029): each replicate
resamples the union of games once, and every cell's statistic is read from
the same replicate, so cells stay paired. Flags read Spy O1 at k2 at the
declared budgets only (64,000 and 256,000): High (lower bound >= 0.8), Low
(upper bound <= 0.2), Intermediate (interval inside (0.2, 0.8)), otherwise
Uncertain.
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
FLAG_BUDGETS = (64_000, 256_000)


def flag(lo: float, hi: float) -> str:
    if lo >= 0.8:
        return "High"
    if hi <= 0.2:
        return "Low"
    if lo > 0.2 and hi < 0.8:
        return "Intermediate"
    return "Uncertain"


def indicators(row: dict, stratum: str) -> dict:
    """A failed search (incomplete or faulted) counts as not found for every outcome."""
    if row.get("failed"):
        return {name: False for name in OUTCOMES}
    o1 = row["o1_spy"] if stratum != "burn" else bool(row.get("o1_burn"))
    return {"o1": o1, "o2_valid": bool(row["o2"].get("valid")), "o3": bool(row["o3"].get("found")),
            "broad_self_chosen": row.get("self_chosen_simulations", 0) > 0,
            "broad_hero_win": row.get("hero_wins", 0) > 0}


def spec(roles: list[dict], opportunity_turn, certified_index) -> list:
    """What a search row measured: its root's roles, T and c. A row is reused only
    when this equals the current root's (RS0-PLAN.md section 3)."""
    return [sorted([r["stratum"], r["role"]] for r in roles), opportunity_turn, certified_index]


def root_spec(root: dict) -> list:
    t = next((r.get("opportunity_turn") for r in root["roles"] if r.get("opportunity_turn") is not None), None)
    c = next((r.get("certified_index") for r in root["roles"] if r.get("certified_index") is not None), None)
    return spec(root["roles"], t, c)


def expected_searches(roots: list[dict], budgets: list[int], seeds: list[int], large: int | None) -> set:
    want = set()
    for r in roots:
        for b in budgets:
            for s in seeds:
                want.add((r["root_id"], b, s))
        if large is not None and any(x["stratum"] == "spy" and x["role"] == "k2" for x in r["roles"]):
            want.add((r["root_id"], large, 0))
    return want


class Replicates:
    """One shared set of game-cluster bootstrap replicates over the union of games."""

    def __init__(self, games: list[str]):
        self.games = sorted(set(games))
        self.pos = {g: i for i, g in enumerate(self.games)}
        n = len(self.games)
        rng = np.random.default_rng(SEED)
        draws = rng.integers(0, max(n, 1), size=(RESAMPLES, n))
        self.counts = np.zeros((RESAMPLES, n))
        np.add.at(self.counts, (np.repeat(np.arange(RESAMPLES), n), draws.ravel()), 1)

    def interval(self, values: dict[str, float]) -> dict:
        if not values:
            return {"games": 0, "mean": None, "ci95": None}
        cols = np.array([self.pos[g] for g in values])
        v = np.array(list(values.values()), dtype=float)
        weights = self.counts[:, cols]
        total = weights.sum(axis=1)
        keep = total > 0
        means = (weights[keep] @ v) / total[keep]
        lo, hi = np.percentile(means, [2.5, 97.5])
        return {"games": len(v), "mean": float(v.mean()), "ci95": [float(lo), float(hi)],
                "replicates_used": int(keep.sum())}


def summarize(rows: list[dict], roots: list[dict], budgets: list[int], seeds: list[int], large: int | None) -> dict:
    by_id = {r["root_id"]: r for r in roots}
    searches, duplicates, stale = {}, [], []
    for r in rows:
        if r["kind"] != "search":
            continue
        root = by_id.get(r["root_id"])
        if root is not None and spec(r.get("roles", []), r.get("opportunity_turn"), r.get("certified_index")) != root_spec(root):
            # Measured under a different root entry: superseded, never reused.
            stale.append([r["root_id"], r["budget"], r["seed"]])
            continue
        key = (r["root_id"], r["budget"], r["seed"])
        if key in searches:
            duplicates.append(list(key))
        searches[key] = r
    if duplicates:
        raise SystemExit(f"duplicate search rows: {duplicates[:5]}")
    want = expected_searches(roots, budgets, seeds, large)
    missing = sorted(want - set(searches))
    unexpected = sorted(set(searches) - want)
    errors = [r for r in rows if r["kind"] == "error"]
    cells: dict = defaultdict(lambda: defaultdict(lambda: defaultdict(list)))
    failed: dict = defaultdict(int)
    incomplete: set = set()
    for root_id, budget, seed in sorted(want):
        root = by_id[root_id]
        for role in root["roles"]:
            if large is not None and budget == large and (role["stratum"], role["role"]) != ("spy", "k2"):
                continue  # the large budget belongs to the Spy k2 role only
            cell = (role["stratum"], role["role"], budget)
            row = searches.get((root_id, budget, seed))
            if row is None:
                incomplete.add(cell)
                continue
            failed[cell] += int(bool(row.get("failed")))
            for name, v in indicators(row, role["stratum"]).items():
                cells[cell][root["game_id"]][name].append(float(v))
    reps = Replicates([g for c in cells.values() for g in c])
    table = []
    for cell in sorted(set(cells) | incomplete):
        stratum, role, budget = cell
        entry = {"stratum": stratum, "role": role, "budget": budget,
                 "failed_searches": failed[cell], "incomplete": cell in incomplete}
        for name in OUTCOMES:
            per_game = {g: float(np.mean(o[name])) for g, o in cells[cell].items() if o[name]}
            if entry["incomplete"]:
                entry[name] = {"games": len(per_game), "mean": None, "ci95": None}
            else:
                entry[name] = reps.interval(per_game)
        table.append(entry)
    flags = []
    for entry in table:
        if entry["stratum"] != "spy" or entry["role"] != "k2" or entry["budget"] not in FLAG_BUDGETS:
            continue
        if entry["incomplete"] or entry["o1"]["ci95"] is None:
            flags.append({"budget": entry["budget"], "flag": None, "reason": "incomplete"})
            continue
        lo, hi = entry["o1"]["ci95"]
        flags.append({"budget": entry["budget"], "flag": flag(lo, hi), "mean": entry["o1"]["mean"],
                      "ci95": [lo, hi], "games": entry["o1"]["games"],
                      "wording": f"s4a from 2 hero turns earlier finds a verified completion in "
                                 f"{100 * entry['o1']['mean']:.0f}% of searches (95% CI {100 * lo:.0f}-{100 * hi:.0f}); "
                                 f"availability in the sampled worlds not measured"})
    return {"roots": len(roots), "searches_expected": len(want), "searches_found": len(want) - len(missing),
            "missing_searches": [list(m) for m in missing], "unexpected_searches": [list(u) for u in unexpected],
            "stale_searches": stale,
            "errors": len(errors), "error_roots": sorted({e["root_id"] for e in errors}),
            "table": table, "flags": flags}


def ints(text: str) -> list[int]:
    return [int(x) for x in text.split(",") if x.strip()]


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("rows", type=Path, nargs="+")
    ap.add_argument("--roots", type=Path, required=True)
    ap.add_argument("--budgets", default="64000,256000")
    ap.add_argument("--seeds", default="0,1,2")
    ap.add_argument("--large-budget", type=int, default=1_024_000)
    ap.add_argument("--json", type=Path)
    a = ap.parse_args(argv)
    rows = [json.loads(line) for p in a.rows for line in p.read_text(encoding="utf-8").splitlines() if line.strip()]
    roots = [json.loads(line) for line in a.roots.read_text(encoding="utf-8").splitlines() if line.strip()]
    out = summarize(rows, roots, ints(a.budgets), ints(a.seeds), a.large_budget)
    text = json.dumps(out, indent=1, sort_keys=True)
    if a.json:
        a.json.write_text(text + "\n", encoding="utf-8")
    print(json.dumps({"flags": out["flags"], "missing": len(out["missing_searches"]), "errors": out["errors"]}, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
