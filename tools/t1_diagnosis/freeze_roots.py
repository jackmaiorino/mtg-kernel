#!/usr/bin/env python3
"""Freeze the stage-2/3 root corpus from `regret_census_v1 mode=roots` rows.

Representative set: the `--max-strata` most populous phase x menu strata,
`--per-stratum` roots each, half from games the focal seat eventually won and
half from games it lost (the other bucket fills a shortfall). Within a bucket
the draw rotates over focal decks and avoids reusing a game.

`--deck NAME` restricts the representative set to that focal deck, and
`--strata a,b,...` uses that declared stratum list (in that order) instead of
the most populous ones; a declared stratum with no rows yields none. The
manifest records every stratum's corpus row count (`strata_counts`) and its
won/lost/other split (`strata_outcomes`, focal score 1 / 0 / anything else,
over the same deck-filtered rows), which `analyze_search.py --freeze` uses
to reweight representative estimates to corpus win/loss frequencies.

Mechanism set (separate from the representative estimate): for each
`prefix:count[:deck]` quota, rows carrying a tag with that prefix (and, if
given, that focal deck), split evenly over the sub-tags present, half wins and
half losses where available. A sub-tag whose pool has fewer than `--mech-min`
roots (default 10) is reported, and its quota is redistributed evenly over the
other sub-tags of the same prefix (it is not padded from other positions); the
reallocation is written to the manifest. `--mech-min 0` turns the
redistribution off and reproduces the earlier script's even split (and its
draws) exactly. With `--spy-types`, the `spy` quota is split by decision type
instead (cast Balustrade Spy, Balustrade Spy target, Dread Return target,
other `spy`-tagged decisions), evenly over the types present. Menus that only order a forced selection (every candidate is a choice
from one effect and all remaining candidates must be chosen) are skipped for
the mechanism set.

Priority fill: a quota `[name=]first>fallback[>...]:count[:deck]` uses the
first prefix in the chain whose pool has at least `--mech-min` roots, and
only that prefix; when none reaches the minimum it uses the last one. The
chain, every pool size and the prefix used are written to the manifest
(`priority`), with `first_not_probed` set when the first prefix was not
used. The group (`set` = `mechanism:<name>`, output
`mechanism_<name>.jsonl`) is named by `name=`, or by the first prefix when
it is omitted. The design's Strands rule (opponent-combat rows; own-turn
castable rows instead only if fewer than 10 opponent-combat rows exist) is

  strands=cawgates_strands_opp_combat>cawgates_strands_castable_own_turn:50:CawGates

with the default `--mech-min 10`.

Writes roots.jsonl, mechanism.jsonl, mechanism_<name>.jsonl and
freeze_manifest.json into --out-dir and prints each file's SHA-256.

Usage:
  freeze_roots.py ROOT_ROWS.jsonl --out-dir DIR [--seed 20261007]
      [--per-stratum 8] [--max-strata 6] [--strata a,b,...] [--deck NAME]
      [--mech spy:8:Spy,cawgates:8:CawGates] [--mech-min 10] [--spy-types]
  --mech entries: prefix:count[:deck] or [name=]first>fallback:count[:deck]
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


SPY_TYPES = ("spy_cast", "spy_target", "spy_dr_target", "spy_other")


def menu_candidates(r):
    """Parsed menu candidates; a truncated entry is kept as its raw string."""
    out = []
    for m in r.get("menu") or []:
        try:
            out.append(json.loads(m))
        except json.JSONDecodeError:
            out.append(m)
    return out


def offers(cands, kind, source):
    for c in cands:
        if isinstance(c, dict):
            if c.get("action_kind") == kind and c.get("source") == source:
                return True
        elif f'"action_kind":"{kind}"' in c and f'"source":"{source}"' in c:
            return True
    return False


def spy_decision_type(r):
    """Decision type of a `spy`-tagged root (labelling only)."""
    cands = menu_candidates(r)
    if offers(cands, "choose_target", "Balustrade Spy"):
        return "spy_target"
    if offers(cands, "choose_target", "Dread Return"):
        return "spy_dr_target"
    if offers(cands, "cast_spell", "Balustrade Spy"):
        return "spy_cast"
    return "spy_other"


def allocate(count, pools, minimum):
    """Even split of `count` over the sub-tags in `pools` ({subtag: pool
    size}, split in sorted order as before), then each sub-tag whose pool is
    below `minimum` gives its quota evenly to the sub-tags at or above it.
    Returns ({subtag: final quota}, reallocation record)."""
    subtags = sorted(pools)
    base = {
        st: count // len(subtags) + (1 if i < count % len(subtags) else 0)
        for i, st in enumerate(subtags)
    }
    final = dict(base)
    low = [st for st in subtags if pools[st] < minimum]
    keep = [st for st in subtags if pools[st] >= minimum]
    moves = []
    if low and keep:
        for st in low:
            q, final[st] = final[st], 0
            to = {k: q // len(keep) + (1 if i < q % len(keep) else 0) for i, k in enumerate(keep)}
            for k, extra in to.items():
                final[k] += extra
            moves.append({"from": st, "pool": pools[st], "quota": q, "to": to})
    record = {
        "minimum": minimum,
        "base_quota": base,
        "final_quota": final,
        "below_minimum": {st: pools[st] for st in low},
        "redistributed": moves,
        "unredistributed": bool(low and not keep),
    }
    return final, record


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


def mech_pool(rows, prefix, deck, taken):
    """Mechanism candidates: untaken rows with a tag starting with `prefix`
    (and the focal deck `deck`, if given) that are not forced selections."""
    return [
        r
        for r in rows
        if root_id(r) not in taken
        and any(t.startswith(prefix) for t in r["tags"])
        and (deck is None or r["deck"] == deck)
        and not forced_selection(r)
    ]


def parser():
    ap = argparse.ArgumentParser()
    ap.add_argument("rows")
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--seed", type=int, default=20261007)
    ap.add_argument("--per-stratum", type=int, default=8)
    ap.add_argument("--max-strata", type=int, default=6)
    ap.add_argument("--strata", help="declared stratum list (comma-separated) instead of the most populous")
    ap.add_argument("--deck", help="restrict the representative set to this focal deck")
    ap.add_argument("--mech", default="spy:8:Spy,cawgates:8:CawGates")
    ap.add_argument("--mech-min", type=int, default=10,
                    help="sub-tags with a smaller pool give their quota to the other sub-tags, and a "
                         "priority chain skips prefixes with a smaller pool (0 = the earlier even split)")
    ap.add_argument("--spy-types", action="store_true",
                    help="split the spy quota by decision type instead of by tag")
    return ap


def freeze(rows, a):
    """Selects the frozen sets from root rows: (representative, mechanism,
    manifest fields)."""
    rows = sorted(rows, key=lambda r: (r["game"], r["step"]))
    rng = random.Random(a.seed)

    rep_rows = [r for r in rows if a.deck is None or r["deck"] == a.deck]
    by_stratum = defaultdict(list)
    for r in rep_rows:
        by_stratum[r["stratum"]].append(r)
    if a.strata:
        strata = [s for s in a.strata.split(",") if s]
    else:
        strata = sorted(by_stratum, key=lambda s: (-len(by_stratum[s]), s))[: a.max_strata]
    used_games = set()
    rep = []
    for s in strata:
        got = outcome_split(by_stratum.get(s, []), a.per_stratum, rng, used_games, "deck")
        for r in got:
            r = dict(r, set="representative", root_id=root_id(r))
            rep.append(r)

    taken = {r["root_id"] for r in rep}
    mech = []
    mech_groups = {}
    for spec in a.mech.split(","):
        parts = spec.split(":")
        name, count = parts[0], int(parts[1])
        deck = parts[2] if len(parts) > 2 else None
        group_name, _, chain_text = name.rpartition("=")
        chain = chain_text.split(">")
        priority = None
        if len(chain) > 1:
            sizes = {p: len(mech_pool(rows, p, deck, taken)) for p in chain}
            prefix = next((p for p in chain if sizes[p] >= a.mech_min), chain[-1])
            priority = {"chain": chain, "pools": sizes, "minimum": a.mech_min, "used": prefix,
                        "first_not_probed": prefix != chain[0],
                        "none_reached_minimum": all(sizes[p] < a.mech_min for p in chain)}
        else:
            prefix = chain[0]
        group_name = group_name or chain[0]
        pool = mech_pool(rows, prefix, deck, taken)
        by_type = a.spy_types and prefix == "spy"
        if by_type:
            sub_of = {root_id(r): spy_decision_type(r) for r in pool}

            def has(r, st):
                return sub_of[root_id(r)] == st
        else:
            def has(r, st):
                return st in r["tags"]
        if by_type:
            subtags = sorted({sub_of[root_id(r)] for r in pool})
        else:
            subtags = sorted({t for r in pool for t in r["tags"] if t.startswith(prefix)})
        pools = {st: sum(1 for r in pool if has(r, st)) for st in subtags}
        quota, allocation = allocate(count, pools, a.mech_min) if subtags else ({}, None)
        group = []
        group_games = set()
        for st in subtags:
            sub = [r for r in pool if has(r, st) and root_id(r) not in taken]
            got = outcome_split(sub, quota[st], rng, group_games, "opp_deck")
            for r in got:
                taken.add(root_id(r))
                group.append(dict(r, set=f"mechanism:{group_name}", mechanism_tag=st, root_id=root_id(r)))
        mech_groups[group_name] = {
            "requested": count,
            "deck": deck,
            "pool": len(pool),
            "subtags": pools,
            "selected": len(group),
        }
        if allocation is not None:
            mech_groups[group_name]["allocation"] = allocation
            mech_groups[group_name]["selected_by_subtag"] = {
                st: sum(1 for r in group if r["mechanism_tag"] == st) for st in subtags
            }
        if by_type:
            mech_groups[group_name]["split_by"] = "decision_type"
        if priority is not None:
            mech_groups[group_name]["priority"] = priority
        mech += group
    fields = {
        "strata_counts": {s: len(by_stratum[s]) for s in sorted(by_stratum, key=lambda s: -len(by_stratum[s]))},
        "strata_outcomes": {
            s: {
                "won": sum(1 for r in by_stratum[s] if r["focal_score"] == 1.0),
                "lost": sum(1 for r in by_stratum[s] if r["focal_score"] == 0.0),
                "other": sum(1 for r in by_stratum[s] if r["focal_score"] not in (0.0, 1.0)),
            }
            for s in sorted(by_stratum)
        },
        "strata_selected": strata,
        "representative_outcomes": {
            s: [r["focal_score"] for r in rep if r["stratum"] == s] for s in strata
        },
        "mechanism": mech_groups,
    }
    return rep, mech, fields


def main():
    a = parser().parse_args()
    rows = []
    with open(a.rows) as f:
        for line in f:
            r = json.loads(line)
            if r.get("kind") == "root":
                rows.append(r)
    rep, mech, fields = freeze(rows, a)
    mech_groups = fields["mechanism"]

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
    for name in mech_groups:
        write(f"mechanism_{name}.jsonl", [r for r in mech if r["set"] == f"mechanism:{name}"])

    manifest = {
        "input": a.rows,
        "input_sha256": sha256(a.rows),
        "seed": a.seed,
        "per_stratum": a.per_stratum,
        "max_strata": a.max_strata,
        "deck": a.deck,
        "strata_declared": a.strata.split(",") if a.strata else None,
        "mech_spec": a.mech,
        "mech_min": a.mech_min,
        "spy_types": a.spy_types,
        "outputs": outputs,
    }
    manifest.update(fields)
    with open(os.path.join(a.out_dir, "freeze_manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1, sort_keys=True)
    for name, o in outputs.items():
        print(f"{o['sha256']}  {name}  ({o['rows']} roots)")
    for prefix, g in mech_groups.items():
        print(f"mechanism {prefix}: {g}")


if __name__ == "__main__":
    main()
