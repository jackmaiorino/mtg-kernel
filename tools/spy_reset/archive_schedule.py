"""Spy discovery archive schedule (data collection, not a hypothesis test).

2,048,000-transition total-budget games (`s4a-budget`, repaired keys) with Spy as the
focal deck against every opposing deck, both seats, K seed families per pairing, in a
fresh namespace. Complete histories and outcomes are kept for a later learning
experiment; no case is chosen or dropped by outcome.

CLI: archive_schedule.py --out DIR [--families K] [--exclude-seeds FILE ...]
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

NS = "spy-discovery-archive-v1"
SPY = 4


def h(*parts) -> bytes:
    return hashlib.sha256(json.dumps(list(parts), separators=(",", ":")).encode()).digest()


def u64(*parts) -> int:
    return int.from_bytes(h(*parts)[:8], "little")


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--families", type=int, default=12)
    ap.add_argument("--exclude-seeds", type=Path, nargs="*", default=[])
    args = ap.parse_args(argv)
    args.out.mkdir(parents=True, exist_ok=True)
    seen = set()
    for p in args.exclude_seeds:
        for line in p.read_text().splitlines():
            if line.strip():
                c = json.loads(line)
                seen |= {c.get("seed"), *c.get("guide_policy_seeds", [])}
    rows = {"r1": [], "r2": []}
    for li, lin in enumerate(("r1", "r2")):
        for k in range(args.families):
            for other in range(9):
                opp = "T1" if (other + li + k) % 2 == 0 else "A48"
                for seat in (0, 1):
                    key = (NS, "archive", lin, SPY, other, k, seat)
                    cid = f"archive-{lin}-k{k}-v{other}-s{seat}"
                    rows[lin].append({"id": cid, "own": SPY, "other": other, "seat": seat, "opp": opp,
                                      "seed": u64(*key, "game-chance"), "starting_player": (SPY + other + k) % 2,
                                      "source_policy_seeds": [0, 0], "anchor_draw": 0,
                                      "guide_policy_seeds": [u64(*key, "live-policy-focal"),
                                                             u64(*key, "live-policy-opponent")],
                                      "search_root": f"{NS}/archive/{cid}"})
    new = {s for v in rows.values() for c in v for s in (c["seed"], *c["guide_policy_seeds"])}
    for lin, v in rows.items():
        (args.out / f"archive-{lin}.jsonl").write_text("".join(json.dumps(c, separators=(",", ":")) + "\n" for c in v))
    # Execution order: seed family first, so an early stop leaves balanced coverage.
    order = sorted(((lin, i) for lin, v in rows.items() for i in range(len(v))),
                   key=lambda x: (int(rows[x[0]][x[1]]["id"].split("-k")[1].split("-")[0]), h(NS, "order", *x)))
    (args.out / "ORDER.json").write_text(json.dumps(order) + "\n")
    print(json.dumps({"cases": sum(len(v) for v in rows.values()), "overlap_with_excluded": len(new & seen)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
