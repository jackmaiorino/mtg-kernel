"""Freeze the Stage 4a roots (RUNNER.md section 4) from the s4a-corpus rows.

Reads only eligibility and identity fields; corpus rows carry no outcome.
For each model, 25 roots per cell in fixed order cast/T1, cast/A48,
target/T1, target/A48. Within a cell, eligible games are ordered by
SHA-256 of `stage4a-roots-v1|model|game_seed|step` (the cell's earliest
eligible root step) and the first 25 not used by an earlier cell are taken:
one root per source game across all cells. A short quota stops the freeze
(exit 3): no substitution, no corpus extension.

Rejects source games whose seed equals any game seed of the excluded bases
(stage 2-3 corpora, sizing material and earlier probes), and any r1/r2
duplicate seed. Then picks four engineering roots, one per model and
upstream stage, from eligible games outside the formal set (first in the
same hash order).

Usage: python freeze_roots.py --corpus r1=PATH --corpus r2=PATH --out DIR
"""
import argparse
import hashlib
import json
import sys
from pathlib import Path

CELLS = [("cast", "T1"), ("cast", "A48"), ("target", "T1"), ("target", "A48")]
PER_CELL = 25
EXCLUDED_BASES = {
    2026100821: "stage 2-3 r1 corpus",
    2026100822: "stage 2-3 r2 corpus",
    2026100801: "stage 2-3 r1 sizing corpus",
    2026100711: "nine-deck T1 probes",
    2026100700: "nine-deck probes",
}
EXCLUDED_GAMES = 10_000
M64 = (1 << 64) - 1


def mix(z):
    """SplitMix64 finalizer, as `regret_census_v1::mix`."""
    z = (z + 0x9E3779B97F4A7C15) & M64
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & M64
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & M64
    return z ^ (z >> 31)


def game_seed(base, game):
    return mix(base ^ mix(game))


def sha_file(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def order_key(model, seed, step):
    return hashlib.sha256(f"stage4a-roots-v1|{model}|{seed}|{step}".encode()).hexdigest()


def root_obj(model, stratum, opp, g):
    r = g[f"{stratum}_root"]
    return {"root_id": f"{model}-{stratum}-{opp}-g{g['game']}-s{r['step']}", "model": model,
            "cell": f"{stratum}/{opp}", "stratum": stratum, "game": g["game"], "seed": g["seed"],
            "deck": g["deck"], "opp_deck": g["opp_deck"], "opp_model": g["opp_model"],
            "focal_seat": g["focal_seat"], "starting_player": g["starting_player"],
            **{k: r[k] for k in ("step", "turn", "phase", "k", "physical_decision_id", "substep",
                                 "obs_hash", "menu_hash", "plain_action")}}


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", action="append", required=True, help="model=path")
    ap.add_argument("--out", required=True)
    a = ap.parse_args(argv)
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    excluded = {}
    for base, why in EXCLUDED_BASES.items():
        for g in range(EXCLUDED_GAMES):
            excluded[game_seed(base, g)] = f"{why} (base {base}, game {g})"
    record = {"schema": "stage4a-freeze/v1", "per_cell": PER_CELL, "cells": [f"{s}/{o}" for s, o in CELLS],
              "excluded_bases": {str(k): v for k, v in EXCLUDED_BASES.items()},
              "excluded_games_per_base": EXCLUDED_GAMES, "models": {}}
    games_by_model, seeds_by_model = {}, {}
    for spec in a.corpus:
        model, path = spec.split("=", 1)
        rows = [json.loads(l) for l in Path(path).read_text(encoding="utf-8").splitlines() if l.strip()]
        games = [r for r in rows if r.get("kind") == "s4a_game"]
        errors = [r for r in rows if r.get("kind") == "error"]
        seeds = [g["seed"] for g in games]
        record["models"][model] = {"corpus": path, "corpus_sha256": sha_file(path), "games": len(games),
                                   "error_rows": len(errors), "unfinished": sum(not g["finished"] for g in games),
                                   "duplicate_seeds": len(seeds) - len(set(seeds)),
                                   "excluded_overlaps": [excluded[s] for s in seeds if s in excluded]}
        games_by_model[model] = games
        seeds_by_model[model] = set(seeds)
    models = list(games_by_model)
    cross = set.intersection(*seeds_by_model.values()) if len(models) > 1 else set()
    record["cross_model_duplicate_seeds"] = len(cross)
    problems = []
    for m in models:
        info = record["models"][m]
        if info["duplicate_seeds"] or info["excluded_overlaps"] or info["error_rows"]:
            problems.append(f"{m}: duplicates {info['duplicate_seeds']}, excluded overlaps "
                            f"{len(info['excluded_overlaps'])}, error rows {info['error_rows']}")
    if cross:
        problems.append(f"{len(cross)} seeds shared by r1 and r2")
    if problems:
        record["stop"] = problems
        (out / "FREEZE.json").write_text(json.dumps(record, indent=1))
        print("STOP: " + "; ".join(problems))
        return 3
    engineering = []
    for m in models:
        eligible = [g for g in games_by_model[m] if g["spy_focal"] and g["finished"]]
        used, roots, cells = set(), [], {}
        for stratum, opp in CELLS:
            pool = sorted((g for g in eligible if g[f"{stratum}_root"] and g["opp_model"] == opp
                           and g["game"] not in used),
                          key=lambda g: order_key(m, g["seed"], g[f"{stratum}_root"]["step"]))
            take = pool[:PER_CELL]
            cells[f"{stratum}/{opp}"] = {"eligible_unused": len(pool), "taken": len(take),
                                         "seat0": sum(g["focal_seat"] == 0 for g in take),
                                         "focal_starts": sum(g["starting_player"] == g["focal_seat"] for g in take)}
            if len(take) < PER_CELL:
                record["models"][m]["cells"] = cells
                record["stop"] = [f"{m} {stratum}/{opp}: {len(take)} of {PER_CELL} roots (insufficient corpus)"]
                (out / "FREEZE.json").write_text(json.dumps(record, indent=1))
                print("STOP: " + record["stop"][0])
                return 3
            for g in take:
                used.add(g["game"])
                roots.append(root_obj(m, stratum, opp, g))
        record["models"][m]["cells"] = cells
        path = out / f"roots-{m}.jsonl"
        path.write_text("".join(json.dumps(r) + "\n" for r in roots), encoding="utf-8")
        record["models"][m]["roots_file"] = path.name
        record["models"][m]["roots_sha256"] = sha_file(path)
        for stratum in ("cast", "target"):
            pool = sorted((g for g in eligible if g[f"{stratum}_root"] and g["game"] not in used),
                          key=lambda g: order_key(m, g["seed"], g[f"{stratum}_root"]["step"]))
            if not pool:
                record["stop"] = [f"{m}: no engineering root for {stratum}"]
                (out / "FREEZE.json").write_text(json.dumps(record, indent=1))
                return 3
            g = pool[0]
            used.add(g["game"])
            engineering.append(root_obj(m, stratum, g["opp_model"], g))
    path = out / "roots-engineering.jsonl"
    path.write_text("".join(json.dumps(r) + "\n" for r in engineering), encoding="utf-8")
    record["engineering_file"] = path.name
    record["engineering_sha256"] = sha_file(path)
    record["engineering_roots"] = [r["root_id"] for r in engineering]
    (out / "FREEZE.json").write_text(json.dumps(record, indent=1))
    print(json.dumps({m: record["models"][m]["cells"] for m in models}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
