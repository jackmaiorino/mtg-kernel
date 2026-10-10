"""Freeze the confirmation panel (boundary vs boundary + FPU 1.5).

Usage: python select_confirmation.py --corpus r1=PATH --corpus r2=PATH
         --exclude-roots GLOB [...] --per-cell N --out DIR
Reads only eligibility and identity fields of the Stage 4a development
corpus (rows carry no outcome). Excludes every source game whose seed
appears in any earlier roots file (formal, engineering, probe, diagnostic).
Per model, N roots in each of cast/T1, cast/A48, target/T1, target/A48;
eligible unused games ordered by SHA-256 of
`spy-confirmation-v1|model|game_seed|step`; one root per source game. A
short quota stops (exit 3) with no substitution. Combo feasibility and
outcomes are never read.
"""
import argparse
import glob
import hashlib
import json
import sys
from pathlib import Path

CELLS = [("cast", "T1"), ("cast", "A48"), ("target", "T1"), ("target", "A48")]
CORPUS_SHA = {"r1": "8e593fb3504c237eff515924f731279672fef6a5c470c7a89651da0ef4841909",
              "r2": "1757a11428a0ea8efd1e2ccbd63de8e3a9c9600ab045b7fa14798d2e0588b5eb"}


M64 = (1 << 64) - 1


def mix(z):
    """SplitMix64 finalizer, as `regret_census_v1::mix` (game seeds)."""
    z = (z + 0x9E3779B97F4A7C15) & M64
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & M64
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & M64
    return z ^ (z >> 31)


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def key(model, seed, step):
    return hashlib.sha256(f"spy-confirmation-v1|{model}|{seed}|{step}".encode()).hexdigest()


def root_obj(model, stratum, opp, g):
    r = g[f"{stratum}_root"]
    return {"root_id": f"{model}-{stratum}-{opp}-g{g['game']}-s{r['step']}", "model": model,
            "cell": f"{stratum}/{opp}", "stratum": stratum, "game": g["game"], "seed": g["seed"],
            "deck": g["deck"], "opp_deck": g["opp_deck"], "opp_model": g["opp_model"],
            "focal_seat": g["focal_seat"], "starting_player": g["starting_player"],
            **{k: r[k] for k in ("step", "turn", "phase", "k", "physical_decision_id", "substep",
                                 "obs_hash", "menu_hash", "plain_action")}}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", action="append", required=True)
    ap.add_argument("--exclude-roots", action="append", required=True)
    ap.add_argument("--exclude-games", action="append", default=[],
                    help="model=base_seed:first:count, games already used by a diagnostic (e.g. the decision census)")
    ap.add_argument("--per-cell", type=int, required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    excluded, files = set(), {}
    for pattern in a.exclude_roots:
        for f in sorted(glob.glob(pattern)):
            n = 0
            for line in Path(f).read_text(encoding="utf-8").splitlines():
                if line.strip():
                    excluded.add(json.loads(line)["seed"])
                    n += 1
            files[f] = {"sha256": sha(f), "roots": n}
    games_excluded = {}
    for spec in a.exclude_games:
        model, rng = spec.split("=", 1)
        base, first, count = (int(x) for x in rng.split(":"))
        for g in range(first, first + count):
            excluded.add(mix(base ^ mix(g)))
        games_excluded[spec] = count
    record = {"schema": "spy-confirmation-freeze/v1", "per_cell": a.per_cell,
              "cells": [f"{s}/{o}" for s, o in CELLS], "order": "sha256(spy-confirmation-v1|model|seed|step)",
              "excluded_roots_files": files, "excluded_game_ranges": games_excluded, "excluded_seeds": len(excluded), "models": {}}
    for spec in a.corpus:
        model, path = spec.split("=", 1)
        if sha(path) != CORPUS_SHA[model]:
            raise SystemExit(f"{model} corpus hash differs from the Stage 4a manifest")
        games = [json.loads(l) for l in Path(path).read_text(encoding="utf-8").splitlines() if l.strip()]
        games = [g for g in games if g.get("kind") == "s4a_game"]
        eligible = [g for g in games if g["spy_focal"] and g["finished"] and g["seed"] not in excluded]
        used, roots, cells = set(), [], {}
        for stratum, opp in CELLS:
            pool = sorted((g for g in eligible if g[f"{stratum}_root"] and g["opp_model"] == opp
                           and g["game"] not in used),
                          key=lambda g: key(model, g["seed"], g[f"{stratum}_root"]["step"]))
            take = pool[:a.per_cell]
            cells[f"{stratum}/{opp}"] = {"eligible_unused": len(pool), "taken": len(take)}
            if len(take) < a.per_cell:
                record["stop"] = f"{model} {stratum}/{opp}: {len(take)} of {a.per_cell}"
                (out / "FREEZE.json").write_text(json.dumps(record, indent=1))
                return 3
            for g in take:
                used.add(g["game"])
                roots.append(root_obj(model, stratum, opp, g))
        p = out / f"roots-{model}.jsonl"
        p.write_text("".join(json.dumps(r) + "\n" for r in roots), encoding="utf-8", newline="\n")
        record["models"][model] = {"corpus": path, "corpus_sha256": CORPUS_SHA[model], "games": len(games),
                                   "eligible_unused_spy_games": len(eligible), "cells": cells,
                                   "roots_file": p.name, "roots_sha256": sha(p),
                                   "root_ids": [r["root_id"] for r in roots]}
    (out / "FREEZE.json").write_text(json.dumps(record, indent=1))
    print(json.dumps({m: v["cells"] for m, v in record["models"].items()}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
