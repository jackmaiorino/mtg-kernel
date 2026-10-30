"""W4 Step A, A2 position draw (collab LANES/w4-combat-20261030/DESIGN.md section 4, A2). Measurement only.

usage: a2_positions.py --scores DIR --split split.json --out positions.jsonl

Eligible: the read half's focal blocker-pair decisions, sorted by (game id, decision index) and grouped by the
focal deck. Each deck gets a share of 300 proportional to its eligible count, with a floor of 20 where it has
that many (a deck below its floor is fixed at the floor and the rest is shared again until stable), rounded by
largest remainder (ties by deck name). Within each deck, in deck-name order, decisions are drawn without
replacement by one numpy Generator(PCG64(20261030)). The world-variance check takes 50 of the 300 by
Generator(PCG64(20261033)).choice(300, 50, replace=False) over the 300 in draw order (design section 12).
"""
from __future__ import annotations

import argparse
import json
import math
from collections import defaultdict
from pathlib import Path

import numpy as np

from common import deck, load_scores

TOTAL = 300
FLOOR = 20
WORLD_CHECK = 50
SEED = 20261030
WORLD_SEED = 20261033


def allocate(counts: dict[str, int]) -> dict[str, int]:
    """Proportional shares of TOTAL with a floor of FLOOR where available and no deck above its count:
    a deck below its floor or above its count is fixed there and the rest is shared again until stable;
    then largest-remainder rounding (ties by deck name)."""
    total = min(TOTAL, sum(counts.values()))
    floors = {d: min(FLOOR, n) for d, n in counts.items()}
    fixed: dict[str, float] = {}
    while True:
        free = {d: n for d, n in counts.items() if d not in fixed}
        left = total - sum(fixed.values())
        share = {d: left * n / sum(free.values()) for d, n in free.items()} if free else {}
        change = {d: floors[d] for d, s in share.items() if s < floors[d]}
        change |= {d: counts[d] for d, s in share.items() if s > counts[d]}
        if not change:
            break
        fixed |= change
    quota = {**{d: float(v) for d, v in fixed.items()}, **share}
    alloc = {d: math.floor(q) for d, q in quota.items()}
    for d in sorted(quota, key=lambda d: (-(quota[d] - alloc[d]), d))[:total - sum(alloc.values())]:
        alloc[d] += 1
    return alloc


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--scores", type=Path, required=True)
    ap.add_argument("--split", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    sc = load_scores(a.scores)
    split = json.loads(a.split.read_text())
    eligible = sorted((d["id"], d["index"]) for d in sc.decisions if "block" in d and split[d["id"]] == "read")
    by_deck = defaultdict(list)
    for gid, index in eligible:
        g = sc.games[gid]
        by_deck[deck(g["deck_ids"][g["focal"]])].append((gid, index))
    counts = {d: len(v) for d, v in by_deck.items()}
    alloc = allocate(counts)
    rng = np.random.Generator(np.random.PCG64(SEED))
    drawn = []
    for d in sorted(by_deck):
        for k in rng.choice(len(by_deck[d]), size=alloc[d], replace=False):
            drawn.append((d, *by_deck[d][int(k)]))
    world = {int(k) for k in np.random.Generator(np.random.PCG64(WORLD_SEED)).choice(
        len(drawn), size=min(WORLD_CHECK, len(drawn)), replace=False)}
    with open(a.out, "w", encoding="utf-8") as out:
        for n, (d, gid, index) in enumerate(drawn):
            out.write(json.dumps({"id": gid, "index": index, "worlds": n in world}) + "\n")
    print(json.dumps({"eligible": len(eligible), "counts": counts, "allocation": alloc, "positions": len(drawn)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
