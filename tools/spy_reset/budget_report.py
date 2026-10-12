"""Analysis of the total-budget screen (collab LANES/search-budget-discovery-plan-20261011,
RUNNER sections 5-6). Frozen before formal outcomes.

CLI: budget_report.py --schedule DIR --rows DIR [DIR ...] --ledger FILE [FILE ...] --out RESULT.json

Rows are `s4a-budget` outputs named `f-<lineage>-<index>-<arm>.jsonl` (arm L = 512,000,
H = 2,048,000). Primary J: the ordered resolved Spy-to-Giant sequence completed and the
focal player won naturally. C: the sequence completed, any outcome. Unknown indicators
(missing, invalid or interrupted cases) are never losses: each count is reported with
lower = known/N and upper = (known + unresolved)/N.
"""
from __future__ import annotations

import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path

ARMS = {"L": 512_000, "H": 2_048_000}
SPY = 4


def load_rows(dirs):
    out = {}
    for d in dirs:
        for p in Path(d).glob("f-*.jsonl"):
            lines = [l for l in p.read_text(encoding="utf-8").splitlines() if l.strip()]
            if lines:
                out[p.stem] = json.loads(lines[0])
    return out


def status(row):
    if row is None:
        return "unstarted"
    if row.get("kind") != "budget_game":
        return "failed"
    g = row["game"]
    if not g.get("valid"):
        return "failed_search" if "failed search" in str(g.get("invalid_reason")) else "non_natural"
    return "natural"


def indicators(row):
    """(J, C) as True/False, or None when unknown."""
    if status(row) != "natural":
        seq = (row or {}).get("game", {}).get("sequence") or {}
        c = True if seq.get("complete") else None
        return None, c
    g = row["game"]
    c = bool((g.get("sequence") or {}).get("complete"))
    return c and g["end"] == "win", c


def game_metrics(row):
    g = row["game"]
    dec = g["decisions"]
    exhausted = next((i for i, d in enumerate(dec) if d.get("remaining_after") == 0), None)
    choosers = Counter(d["chooser"] for d in dec)
    spy_menus = [d for d in dec if d.get("spy_cast_offered") or d.get("spy_self_offered") or d.get("dr_giant_offered")]
    return {
        "end": g["end"], "transitions": g["transitions_used"], "searches": len(g["searches"]),
        "refresh_reasons": Counter(s["reason"] for s in g["searches"]),
        "decisions": len(dec), "choosers": choosers,
        "first_exhausted_decision": exhausted,
        "decisions_after_exhaustion": (len(dec) - exhausted - 1) if exhausted is not None else 0,
        "supported_fraction": (choosers["new_root"] + choosers["reused"]) / max(1, len(dec)),
        "spy_menus": [{k: d.get(k) for k in ("step", "physical_decision_id", "substep", "chooser", "lookup", "edge_backups",
                                              "remaining_before", "spy_cast_offered", "spy_cast_chosen", "spy_self_offered",
                                              "spy_self_chosen", "dr_giant_offered", "dr_giant_chosen")} for d in spy_menus],
        "sequence": g.get("sequence"), "real_transitions": g.get("real_transitions"),
        "keys": g.get("search_keys"),
    }


def shared_prefix(rl, rh):
    """L/H must share every action until the first search whose budget or step differs."""
    if status(rl) == "unstarted" or status(rh) == "unstarted":
        return None
    ls, hs = rl["game"]["searches"], rh["game"]["searches"]
    k = next((i for i, (a, b) in enumerate(zip(ls, hs)) if (a["budget"], a["step"]) != (b["budget"], b["step"])),
             min(len(ls), len(hs)))
    cut = None
    if k < len(ls):
        cut = ls[k]["step"]
    elif k < len(hs):
        cut = hs[k]["step"]
    la, ha = rl["game"]["actions"], rh["game"]["actions"]
    n = len(la) if cut is None else cut
    same = la[:n] == ha[:n] and (cut is not None or la == ha)
    return {"first_differing_search": k, "at_step": cut, "prefix_identical": same}


def bounds(values, n):
    known = sum(1 for v in values if v is True)
    unknown = sum(1 for v in values if v is None)
    return {"known": known, "unknown": unknown, "n": n, "lower": known / n if n else None,
            "upper": (known + unknown) / n if n else None}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--schedule", type=Path, required=True)
    ap.add_argument("--rows", nargs="+", required=True)
    ap.add_argument("--ledger", nargs="+", default=[])
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args(argv)
    rows = load_rows(args.rows)
    result = {"schema": "search-total-budget-v1/result", "lineages": {}, "statuses": Counter(), "prefix": []}
    for lin in ("r1", "r2"):
        cases = [json.loads(l) for l in (args.schedule / f"formal-{lin}.jsonl").read_text().splitlines() if l.strip()]
        per = {}
        for i, c in enumerate(cases):
            rl, rh = rows.get(f"f-{lin}-{i}-L"), rows.get(f"f-{lin}-{i}-H")
            per[i] = {"case": c, "L": rl, "H": rh}
            for arm, r in (("L", rl), ("H", rh)):
                result["statuses"][f"{arm}:{status(r)}"] += 1
            p = shared_prefix(rl, rh)
            if p is not None:
                result["prefix"].append({"case": c["id"], **p})
        spy = {i: v for i, v in per.items() if v["case"]["own"] == SPY}
        units = defaultdict(list)
        for i, v in spy.items():
            units[v["case"]["other"]].append(v)
        lin_out = {"spy": {}, "spy_units": [], "controls": {}, "games": {}}
        for arm in ARMS:
            js = [indicators(v[arm])[0] for v in spy.values()]
            cs = [indicators(v[arm])[1] for v in spy.values()]
            ends = Counter((game_metrics(v[arm])["end"] if status(v[arm]) == "natural" else status(v[arm])) for v in spy.values())
            lin_out["spy"][arm] = {"J": bounds(js, len(spy)), "C": bounds(cs, len(spy)), "ends": ends,
                                   "conditional": {k: sum(1 for v in spy.values() if status(v[arm]) == "natural"
                                                          and any(m.get(k) for m in game_metrics(v[arm])["spy_menus"]))
                                                   for k in ("spy_cast_offered", "spy_cast_chosen", "spy_self_offered",
                                                             "spy_self_chosen", "dr_giant_offered", "dr_giant_chosen")}}
        for other, vs in sorted(units.items()):
            u = {"other": other}
            for arm in ARMS:
                ind = [indicators(v[arm]) for v in vs]
                u[arm] = {"J": [x[0] for x in ind], "C": [x[1] for x in ind]}
            lin_out["spy_units"].append(u)
        for own in sorted({v["case"]["own"] for v in per.values()} - {SPY}):
            vs = [v for v in per.values() if v["case"]["own"] == own]
            lin_out["controls"][own] = {arm: Counter((game_metrics(v[arm])["end"] if status(v[arm]) == "natural"
                                                      else status(v[arm])) for v in vs) for arm in ARMS}
        for i, v in per.items():
            lin_out["games"][v["case"]["id"]] = {arm: (game_metrics(v[arm]) if status(v[arm]) == "natural" else
                                                       {"status": status(v[arm])}) for arm in ARMS}
        result["lineages"][lin] = lin_out
    ledger = []
    for p in args.ledger:
        ledger += [json.loads(l) for l in Path(p).read_text().splitlines() if l.strip()]
    result["cpu"] = {"by_host_arm": {}, "total_seconds": sum(r["cpu_seconds"] for r in ledger if r["phase"] == "formal")}
    for r in ledger:
        if r["phase"] == "formal":
            k = f'{r["host"]}:{"H" if r["total"] == ARMS["H"] else "L"}'
            result["cpu"]["by_host_arm"][k] = result["cpu"]["by_host_arm"].get(k, 0) + r["cpu_seconds"]
    args.out.write_text(json.dumps(result, indent=1, default=lambda o: dict(o)) + "\n")
    for lin, v in result["lineages"].items():
        print(lin, {arm: (v["spy"][arm]["J"], v["spy"][arm]["C"]) for arm in ARMS})
    print("statuses", dict(result["statuses"]), "prefix ok", all(p["prefix_identical"] for p in result["prefix"]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
