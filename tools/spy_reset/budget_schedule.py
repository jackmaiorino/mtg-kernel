"""Frozen schedule of the total-budget screen (collab
LANES/search-budget-discovery-plan-20261011, RUNNER sections 3-4).

Writes per-lineage case files (`s4a-budget` BUDGET_CASES, SourceSpec JSONL)
for the formal panel and the qualification cases, plus a summary with the
cross-tab and an overlap check against earlier seed sets.

Seeds and identities come only from the namespace, domain, lineage and case
fields; never from the budget arm or an outcome.

CLI: budget_schedule.py --out DIR [--exclude-seeds FILE.json ...]
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

NS = "search-total-budget-v1"
LINEAGES = ("r1", "r2")
SPY = 4
DECKS = ("Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries")


def h(*parts) -> bytes:
    return hashlib.sha256(json.dumps(list(parts), separators=(",", ":")).encode()).digest()


def u64(*parts) -> int:
    return int.from_bytes(h(*parts)[:8], "little")


def case(domain: str, lineage: str, own: int, other: int, seat: int, opp: str) -> dict:
    """One source case: the focal player's deck `own` in `seat` against `other`."""
    cid = f"{domain}-{lineage}-o{own}-v{other}-s{seat}"
    key = (NS, domain, lineage, own, other, seat)
    return {"id": cid, "own": own, "other": other, "seat": seat, "opp": opp,
            "seed": u64(*key, "game-chance"),
            "starting_player": (own + other) % 2,
            "source_policy_seeds": [0, 0], "anchor_draw": 0,
            "guide_policy_seeds": [u64(*key, "live-policy-focal"), u64(*key, "live-policy-opponent")],
            "search_root": f"{NS}/{domain}/{cid}"}


def formal(lineage: str) -> list[dict]:
    li = LINEAGES.index(lineage)
    out = []
    for other in range(9):
        opp = "T1" if (other + li) % 2 == 0 else "A48"
        out += [case("formal", lineage, SPY, other, seat, opp) for seat in (0, 1)]
    controls = [o for o in range(9) if o != SPY]
    policy_order = sorted(controls, key=lambda own: h(NS, "control-policy", lineage, own))
    for own in controls:
        other = min(range(9), key=lambda o: h(NS, "control-opponent", lineage, own, o))
        opp = "T1" if policy_order.index(own) < 4 else "A48"
        out += [case("formal", lineage, own, other, seat, opp) for seat in (0, 1)]
    return out


def qualification(lineage: str) -> list[dict]:
    """One Spy and one non-Spy case per lineage, chosen by hash (no outcomes)."""
    out = []
    spy_other = min(range(9), key=lambda o: h(NS, "qual-opponent", lineage, SPY, o))
    own = min((o for o in range(9) if o != SPY), key=lambda o: h(NS, "qual-focal", lineage, o))
    other = min(range(9), key=lambda o: h(NS, "qual-opponent", lineage, own, o))
    for f, v in ((SPY, spy_other), (own, other)):
        seat = h(NS, "qual-seat", lineage, f)[0] % 2
        opp = "T1" if h(NS, "qual-policy", lineage, f)[0] % 2 == 0 else "A48"
        out.append(case("qualification", lineage, f, v, seat, opp))
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--exclude-seeds", type=Path, nargs="*", default=[])
    args = ap.parse_args(argv)
    args.out.mkdir(parents=True, exist_ok=True)
    old = set()
    for p in args.exclude_seeds:
        def walk(v):
            if isinstance(v, dict):
                for k, x in v.items():
                    if k in ("seed", "seeds", "source_policy_seeds", "guide_policy_seeds") or isinstance(x, (dict, list)):
                        walk(x) if isinstance(x, (dict, list)) else old.add(x)
            elif isinstance(v, list):
                for x in v:
                    walk(x) if isinstance(x, (dict, list)) else (old.add(x) if isinstance(x, int) else None)
        text = p.read_text(encoding="utf-8")
        for line in (text.splitlines() if p.suffix == ".jsonl" else [text]):
            if line.strip():
                walk(json.loads(line))
    summary = {"namespace": NS, "decks": DECKS, "lineages": {}}
    all_seeds = []
    for lin in LINEAGES:
        for name, cases in (("formal", formal(lin)), ("qualification", qualification(lin))):
            path = args.out / f"{name}-{lin}.jsonl"
            path.write_text("".join(json.dumps(c, separators=(",", ":")) + "\n" for c in cases))
            all_seeds += [c["seed"] for c in cases] + [s for c in cases for s in c["guide_policy_seeds"]]
            summary["lineages"].setdefault(lin, {})[name] = {
                "file": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "cases": len(cases),
                "crosstab": [[c["own"], c["other"], c["seat"], c["opp"], c["starting_player"]] for c in cases]}
    summary["distinct_new_seeds"] = len(set(all_seeds))
    summary["new_seed_values"] = len(all_seeds)
    summary["overlap_with_excluded"] = len(set(all_seeds) & old)
    summary["excluded_values_checked"] = len(old)
    (args.out / "SCHEDULE.json").write_text(json.dumps(summary, indent=1) + "\n")
    print(json.dumps({k: summary[k] for k in ("distinct_new_seeds", "new_seed_values", "overlap_with_excluded",
                                              "excluded_values_checked")}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
