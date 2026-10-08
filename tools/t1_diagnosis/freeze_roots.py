#!/usr/bin/env python3
"""Freeze the stage-2/3 root corpus from `regret_census_v1 mode=roots` rows.

Representative set: the `--max-strata` most populous phase x menu strata,
`--per-stratum` roots each, half from games the focal seat eventually won and
half from games it lost (the other bucket fills a shortfall). Within a bucket
the draw rotates over focal decks and avoids reusing a game.

Mechanism set (separate from the representative estimate): for each
`prefix:count[:deck]` quota, rows carrying a tag with that prefix (and, if
given, that focal deck), split evenly over the sub-tags present, half wins and
half losses where available. Menus that only order a forced selection (every
candidate is a choice from one effect and all remaining candidates must be
chosen) are skipped for the mechanism set.

Writes roots.jsonl, mechanism.jsonl, mechanism_<prefix>.jsonl and
freeze_manifest.json into --out-dir and prints each file's SHA-256.

Usage:
  freeze_roots.py ROOT_ROWS.jsonl --out-dir DIR [--seed 20261007]
      [--per-stratum 8] [--max-strata 6] [--mech spy:8:Spy,cawgates:8:CawGates]
"""

import argparse
import hashlib
import json
import os
import random
from collections import defaultdict


def sha256(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def root_id(r):
    return f"g{r['game']}s{r['step']}"


def forced_selection(r):
    """True when the menu only orders a forced selection."""
    menu = r.get("menu") or []
    try:
        cands = [json.loads(m) for m in menu]
    except json.JSONDecodeError:
        return False  # truncated semantics: keep the row
    if not cands or any(c.get("action_kind") != "choose_effect_target" for c in cands):
        return False
    if len({c.get("source") for c in cands}) != 1:
        return False
    need = cands[0].get("min_targets", 0) - cands[0].get("selected_count", 0)
    return need >= len(cands)


def pick(pool, n, rng, used_games, spread_key):
    """Draw n rows, rotating over spread_key values and preferring unused games."""
    pool = list(pool)
    rng.shuffle(pool)
    groups = defaultdict(list)
    for r in pool:
        groups[r[spread_key]].append(r)
    keys = sorted(groups)
    if keys:
        start = rng.randrange(len(keys))
        keys = keys[start:] + keys[:start]
    chosen = []
    for allow_reuse in (False, True):
        progress = True
        while len(chosen) < n and progress:
            progress = False
            for k in keys:
                if len(chosen) >= n:
                    break
                for i, r in enumerate(groups[k]):
                    if allow_reuse or r["game"] not in used_games:
                        chosen.append(groups[k].pop(i))
                        used_games.add(r["game"])
                        progress = True
                        break
    return chosen


def outcome_split(pool, n, rng, used_games, spread_key):
    wins = [r for r in pool if r["focal_score"] == 1.0]
    losses = [r for r in pool if r["focal_score"] == 0.0]
    draws = [r for r in pool if r["focal_score"] not in (0.0, 1.0)]
    want_w = n // 2
    got = pick(wins, want_w, rng, used_games, spread_key)
    got += pick(losses, n - len(got), rng, used_games, spread_key)
    if len(got) < n:
        got += pick([r for r in wins if r not in got], n - len(got), rng, used_games, spread_key)
    if len(got) < n:
        got += pick(draws, n - len(got), rng, used_games, spread_key)
    return got


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("rows")
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--seed", type=int, default=20261007)
    ap.add_argument("--per-stratum", type=int, default=8)
    ap.add_argument("--max-strata", type=int, default=6)
    ap.add_argument("--mech", default="spy:8:Spy,cawgates:8:CawGates")
    a = ap.parse_args()

    rows = []
    with open(a.rows) as f:
        for line in f:
            r = json.loads(line)
            if r.get("kind") == "root":
                rows.append(r)
    rows.sort(key=lambda r: (r["game"], r["step"]))
    rng = random.Random(a.seed)

    by_stratum = defaultdict(list)
    for r in rows:
        by_stratum[r["stratum"]].append(r)
    strata = sorted(by_stratum, key=lambda s: (-len(by_stratum[s]), s))[: a.max_strata]
    used_games = set()
    rep = []
    for s in strata:
        got = outcome_split(by_stratum[s], a.per_stratum, rng, used_games, "deck")
        for r in got:
            r = dict(r, set="representative", root_id=root_id(r))
            rep.append(r)

    taken = {r["root_id"] for r in rep}
    mech = []
    mech_groups = {}
    for spec in a.mech.split(","):
        parts = spec.split(":")
        prefix, count = parts[0], int(parts[1])
        deck = parts[2] if len(parts) > 2 else None
        pool = [
            r
            for r in rows
            if root_id(r) not in taken
            and any(t.startswith(prefix) for t in r["tags"])
            and (deck is None or r["deck"] == deck)
            and not forced_selection(r)
        ]
        subtags = sorted({t for r in pool for t in r["tags"] if t.startswith(prefix)})
        group = []
        group_games = set()
        for i, st in enumerate(subtags):
            share = count // len(subtags) + (1 if i < count % len(subtags) else 0)
            sub = [r for r in pool if st in r["tags"] and root_id(r) not in taken]
            got = outcome_split(sub, share, rng, group_games, "opp_deck")
            for r in got:
                taken.add(root_id(r))
                group.append(dict(r, set=f"mechanism:{prefix}", mechanism_tag=st, root_id=root_id(r)))
        mech_groups[prefix] = {
            "requested": count,
            "deck": deck,
            "pool": len(pool),
            "subtags": {st: sum(1 for r in pool if st in r["tags"]) for st in subtags},
            "selected": len(group),
        }
        mech += group

    os.makedirs(a.out_dir, exist_ok=True)
    outputs = {}

    def write(name, items):
        path = os.path.join(a.out_dir, name)
        with open(path, "w") as f:
            for r in items:
                f.write(json.dumps(r, sort_keys=True) + "\n")
        outputs[name] = {"rows": len(items), "sha256": sha256(path)}

    write("roots.jsonl", rep)
    write("mechanism.jsonl", mech)
    for prefix in mech_groups:
        write(f"mechanism_{prefix}.jsonl", [r for r in mech if r["set"] == f"mechanism:{prefix}"])

    manifest = {
        "input": a.rows,
        "input_sha256": sha256(a.rows),
        "seed": a.seed,
        "per_stratum": a.per_stratum,
        "max_strata": a.max_strata,
        "strata_counts": {s: len(by_stratum[s]) for s in sorted(by_stratum, key=lambda s: -len(by_stratum[s]))},
        "strata_selected": strata,
        "representative_outcomes": {
            s: [r["focal_score"] for r in rep if r["stratum"] == s] for s in strata
        },
        "mechanism": mech_groups,
        "outputs": outputs,
    }
    with open(os.path.join(a.out_dir, "freeze_manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1, sort_keys=True)
    for name, o in outputs.items():
        print(f"{o['sha256']}  {name}  ({o['rows']} roots)")
    for prefix, g in mech_groups.items():
        print(f"mechanism {prefix}: {g}")


if __name__ == "__main__":
    main()
