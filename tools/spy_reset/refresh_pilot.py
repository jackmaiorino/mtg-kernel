"""Search-refresh pilot: frozen cases and analysis (collab LANES/spy-search-refresh-plan-20261011).

CLI:
  cases    --out CASES.txt                108 source ids: per lineage and own deck, both
                                          seats against the 3 other decks first in the
                                          SHA-256 order of ["spy-search-refresh-pilot-v1", lineage, own, other]
  analyze  --bank r1=BANK --bank r2=BANK --rows ROWS.jsonl [...] --out REPORT.json

Units (PLAN section 1): a real physical decision is a guide-player decision
group whose first substep has two or more legal actions; it is tree-controlled
when that first substep was executed from a tree edge. Forced decisions are
excluded from numerators and denominators. S and P come from the stopped bank.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from collections import Counter
from pathlib import Path

NS = "spy-search-refresh-pilot-v1"
LINEAGES = ("r1", "r2")
DECKS = 9
OTHERS = 3
SPY = 4


def key(parts):
    return hashlib.sha256(json.dumps(parts, separators=(",", ":")).encode()).hexdigest()


def cases() -> list[str]:
    out = []
    for lineage in LINEAGES:
        for own in range(DECKS):
            others = sorted(range(DECKS), key=lambda t: key([NS, lineage, own, t]))[:OTHERS]
            for other in sorted(others):
                for seat in (0, 1):
                    out.append(f"{lineage}-o{own}-t{other}-s{seat}")
    return out


def summarize(decisions: list[dict]) -> dict:
    """Control in the plan's units from per-decision records (real decisions only)."""
    groups = {}
    for d in decisions:
        groups.setdefault(d["physical_decision_id"], []).append(d)
    real = [g for g in groups.values() if g[0]["substep"] == 0]
    tree = [g for g in real if g[0]["chooser"] != "plain"]
    new_root = sum(1 for g in tree if g[0]["chooser"] == "new_root")
    runs, run = [], 0
    for g in real:
        if g[0]["chooser"] != "plain":
            run += 1
        elif run:
            runs.append(run)
            run = 0
    if run:
        runs.append(run)
    spy = [d for d in decisions if d["spy_self_offered"] or d["dr_giant_offered"]]
    return {"real": len(real), "tree": len(tree), "new_root": new_root, "reused": len(tree) - new_root,
            "tree_substeps": sum(1 for d in decisions if d["chooser"] != "plain"),
            "span": (max(g[0]["step"] for g in tree) - min(g[0]["step"] for g in tree)) if tree else 0,
            "runs": runs,
            "spy": {"self_offered": sum(d["spy_self_offered"] for d in spy),
                    "self_chosen_tree": sum(d["spy_self_chosen"] and d["chooser"] != "plain" for d in spy),
                    "self_chosen_plain": sum(d["spy_self_chosen"] and d["chooser"] == "plain" for d in spy),
                    "dr_offered": sum(d["dr_giant_offered"] for d in spy),
                    "dr_chosen_tree": sum(d["dr_giant_chosen"] and d["chooser"] != "plain" for d in spy),
                    "dr_chosen_plain": sum(d["dr_giant_chosen"] and d["chooser"] == "plain" for d in spy)}}


def bank_s(row: dict) -> dict:
    """The bank S guide in the same units: executed depths are the controlled groups.
    Real decisions are counted from the S guide's recorded starts with k >= 2."""
    g = row["guides"]["S"]
    executed = {d for d, _ in g["search"]["execution"]["executed"]}
    real = sum(1 for s in g["starts"] if s["k"] >= 2)
    return {"real": real, "tree": len(executed), "end": g["end"], "learner_starts": len(g["starts"])}


def analyze(banks: dict, rows: list[dict]) -> dict:
    expected = set(cases())
    by_id = {}
    for r in rows:
        if r.get("kind") == "refresh_source":
            by_id[r["id"]] = r
    out = {"cases": len(expected), "rows": len(by_id), "missing": sorted(expected - set(by_id)),
           "invalid": [], "lineages": {}}
    for lineage in LINEAGES:
        pairs = []
        for cid in sorted(c for c in expected if c.startswith(lineage + "-")):
            r, b = by_id.get(cid), banks[lineage][cid]
            if r is None:
                continue
            arms = r["arms"]
            bad = [a for a in ("R", "S128") if not arms[a].get("valid")]
            if bad:
                out["invalid"].append({"id": cid, "arms": bad,
                                       "reasons": [arms[a].get("invalid_reason") for a in bad]})
                continue
            R, S128 = summarize(arms["R"]["decisions"]), summarize(arms["S128"]["decisions"])
            reasons = Counter(s["reason"] for s in arms["R"]["searches"][1:])
            pairs.append({"id": cid, "own": r["own"], "R": R, "S128": S128, "S": bank_s(b),
                          "end": {"R": arms["R"]["end"], "S128": arms["S128"]["end"],
                                  "S": b["guides"]["S"]["end"], "P": b["guides"]["P"]["end"]},
                          "refreshes": len(arms["R"]["searches"]) - 1, "reasons": dict(reasons),
                          "transitions": arms["R"]["transitions_used"],
                          "learner_starts": arms["R"]["learner_starts"]})
        n = len(pairs)
        mean = lambda f: sum(f(p) for p in pairs) / n if n else None  # noqa: E731
        wins = {a: sum(p["end"][a] == "win" for p in pairs) for a in ("R", "S128", "S", "P")}
        lin = {"pairs": n,
               "tree_per_guide": {"R": mean(lambda p: p["R"]["tree"]), "S128": mean(lambda p: p["S128"]["tree"]),
                                  "S": mean(lambda p: p["S"]["tree"])},
               "R_new_root": mean(lambda p: p["R"]["new_root"]), "R_reused": mean(lambda p: p["R"]["reused"]),
               "share_tree": {"R": mean(lambda p: p["R"]["tree"] / max(1, p["R"]["real"])),
                              "S128": mean(lambda p: p["S128"]["tree"] / max(1, p["S128"]["real"])),
                              "S": mean(lambda p: p["S"]["tree"] / max(1, p["S"]["real"]))},
               "refreshes_per_guide": mean(lambda p: p["refreshes"]),
               "refresh_reasons": dict(sum((Counter(p["reasons"]) for p in pairs), Counter())),
               "run_lengths_R": sorted(x for p in pairs for x in p["R"]["runs"]),
               "wins": wins, "transitions_per_guide": mean(lambda p: p["transitions"]),
               "spy": {a: dict(sum((Counter(p[a]["spy"]) for p in pairs if p["own"] == SPY), Counter()))
                       for a in ("R", "S128")},
               "detail": pairs}
        lin["control_extends"] = (n == len([c for c in expected if c.startswith(lineage + "-")])
                                  and lin["tree_per_guide"]["R"] > lin["tree_per_guide"]["S"]
                                  and lin["tree_per_guide"]["R"] > lin["tree_per_guide"]["S128"])
        lin["outcome_guard"] = wins["R"] >= wins["S"] - 3
        out["lineages"][lineage] = lin
    complete = not out["missing"] and not out["invalid"]
    out["complete"] = complete
    out["disposition"] = ("incomplete: no advance" if not complete else
                          "advance to the refreshed-bank proposal" if all(
                              v["control_extends"] and v["outcome_guard"] for v in out["lineages"].values())
                          else "stop: control does not extend or the outcome guard fails")
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    c = sub.add_parser("cases")
    c.add_argument("--out", type=Path, required=True)
    a = sub.add_parser("analyze")
    a.add_argument("--bank", action="append", required=True)
    a.add_argument("--rows", type=Path, nargs="+", required=True)
    a.add_argument("--out", type=Path, required=True)
    args = ap.parse_args(argv)
    if args.op == "cases":
        ids = cases()
        args.out.write_text("".join(i + "\n" for i in ids), encoding="utf-8", newline="\n")
        print(json.dumps({"cases": len(ids), "spy_own": sum(f"-o{SPY}-" in i for i in ids)}))
        return 0
    banks = {}
    for item in args.bank:
        lineage, path = item.split("=", 1)
        banks[lineage] = {json.loads(l)["id"]: json.loads(l) for l in Path(path).read_text().splitlines() if l.strip()}
    rows = [json.loads(l) for p in args.rows for l in Path(p).read_text().splitlines() if l.strip()]
    result = analyze(banks, rows)
    args.out.write_text(json.dumps(result, indent=1) + "\n")
    print(json.dumps({"disposition": result["disposition"],
                      "lineages": {k: {x: v[x] for x in ("pairs", "tree_per_guide", "wins", "control_extends",
                                                          "outcome_guard")} for k, v in result["lineages"].items()}}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
