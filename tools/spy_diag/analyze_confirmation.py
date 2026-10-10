"""Analyze the frozen confirmation (collab LANES/spy-confirmation-20261010/PLAN.md).

Usage: python analyze_confirmation.py FREEZE.json ROWS_DIR LEDGER.jsonl OUT_DIR
Reads only s4a_diag_root rows named <T>-s<rep>-<root_id>.jsonl. Units are
source games. Applies the pre-registered decision rule unchanged.
"""
import json
import math
import random
import sys
from pathlib import Path

TREAT = ("B", "F")
REPS = (0, 1)


def load(rows_dir, roots):
    out = {}
    for t in TREAT:
        for rep in REPS:
            for rid in roots:
                p = Path(rows_dir) / f"{t}-s{rep}-{rid}.jsonl"
                if not p.exists():
                    continue
                rows = [json.loads(l) for l in p.read_text(encoding="utf-8").splitlines() if l.strip()]
                rows = [r for r in rows if r.get("kind") == "s4a_diag_root" and r["root_id"] == rid]
                if not rows:
                    continue  # still running or failed: reported as incomplete
                if len(rows) != 1:
                    raise ValueError(f"{p}: expected one row, found {len(rows)}")
                r = rows[0]
                want_select = "fpu-1.5" if t == "F" else None
                if r.get("select_rule") != want_select or r.get("runtime_rules") != "resolution-boundary-v1" \
                        or r.get("search_rep", 0) != rep or r["config"]["limits"]["formal"] is not True:
                    raise ValueError(f"{p}: identity mismatch")
                e = r["arms"]["E"]
                out[(t, rep, rid)] = {"completions": e["selection"]["discovery"]["completions_natural"],
                                      "j": e["summary"]["j"], "w": e["summary"]["w"],
                                      "worlds": [w["world"] for w in r["eval_worlds"]],
                                      "invalid": r["invalid"], "sims": e["selection"]["completed_natural"]}
    return out


def sign_test_p(pos, neg):
    """One-sided exact sign test P(X >= pos) with n = pos + neg, p = 1/2."""
    n = pos + neg
    if n == 0:
        return None
    return sum(math.comb(n, k) for k in range(pos, n + 1)) / 2 ** n


def main():
    freeze = json.loads(Path(sys.argv[1]).read_text())
    roots = [rid for m in freeze["models"].values() for rid in m["root_ids"]]
    cell = {}
    for m, v in freeze["models"].items():
        for rid in v["root_ids"]:
            cell[rid] = f"{m} {rid.split('-')[1]}/{rid.split('-')[2]}"
    data = load(sys.argv[2], roots)
    games = []
    for rid in roots:
        g = {"root_id": rid, "cell": cell[rid]}
        complete = all((t, rep, rid) in data for t in TREAT for rep in REPS)
        g["complete"] = complete
        for t in TREAT:
            reps = [data.get((t, rep, rid)) for rep in REPS]
            have = [x for x in reps if x]
            g[t] = {"discovery_reps": sum(x["completions"] > 0 for x in have),
                    "completions": [x["completions"] for x in have],
                    "J": [x["j"] for x in have], "W": [x["w"] for x in have],
                    "J_mean": sum(x["j"] for x in have) / len(have) if have else None,
                    "W_mean": sum(x["w"] for x in have) / len(have) if have else None,
                    "invalid": any(x["invalid"] for x in have)}
        if complete:
            worlds = {tuple(data[(t, rep, rid)]["worlds"]) for t in TREAT for rep in REPS}
            g["paired_worlds_identical"] = len(worlds) == 1
            g["D_J"] = g["F"]["J_mean"] - g["B"]["J_mean"]
            g["D_W"] = g["F"]["W_mean"] - g["B"]["W_mean"]
        games.append(g)
    done = [g for g in games if g["complete"]]
    pos = sum(g["D_J"] > 0 for g in done)
    neg = sum(g["D_J"] < 0 for g in done)
    p_better = sign_test_p(pos, neg)
    p_worse = sign_test_p(neg, pos)
    dw = [g["D_W"] for g in done]
    mean_dw = sum(dw) / len(dw) if dw else None
    rng = random.Random(2026101020)
    boots = []
    for _ in range(10_000):
        s = [dw[rng.randrange(len(dw))] for _ in dw]
        boots.append(sum(s) / len(s))
    boots.sort()
    ci = [boots[249], boots[9749]] if dw else None
    if p_better is not None and p_better <= 0.05 and mean_dw >= 0:
        verdict = "Confirmed"
    elif p_worse is not None and p_worse <= 0.05:
        verdict = "Harmful"
    else:
        verdict = "Not confirmed"

    def agg(t):
        return {"games_discovered_any_rep": sum(g[t]["discovery_reps"] > 0 for g in done),
                "games_discovered_both_reps": sum(g[t]["discovery_reps"] == 2 for g in done),
                "games_with_J_any_rep": sum(any(j > 0 for j in g[t]["J"]) for g in done),
                "games_with_J_both_reps": sum(all(j > 0 for j in g[t]["J"]) for g in done),
                "sum_J_mean": sum(g[t]["J_mean"] for g in done), "sum_W_mean": sum(g[t]["W_mean"] for g in done),
                "invalid_games": sum(g[t]["invalid"] for g in done)}

    ledger = [json.loads(l) for l in Path(sys.argv[3]).read_text().splitlines() if l.strip()]
    cpu = {t: sum(r["cpu_seconds"] for r in ledger if r["phase"] == "panel" and r["job"].startswith(f"{t}-"))
           for t in TREAT}
    summary = {"games_complete": len(done), "games_planned": len(games), "B": agg("B"), "F": agg("F"),
               "sign_test": {"F_better": pos, "B_better": neg, "ties": len(done) - pos - neg,
                             "p_one_sided_F_better": p_better, "p_one_sided_B_better": p_worse},
               "D_W": {"mean": mean_dw, "bootstrap_95": ci},
               "regressions": {"games_J_lower_under_F": [g["root_id"] for g in done if g["D_J"] < 0],
                               "games_W_drop_2_or_more": [g["root_id"] for g in done if g["D_W"] <= -2]},
               "paired_worlds_identical_all": all(g.get("paired_worlds_identical") for g in done),
               "cpu_seconds": {**cpu, "qualification": sum(r["cpu_seconds"] for r in ledger
                                                         if r["phase"] == "qualification")},
               "verdict": verdict}
    out = Path(sys.argv[4])
    out.mkdir(parents=True, exist_ok=True)
    (out / "games.json").write_text(json.dumps(games, indent=1))
    (out / "summary.json").write_text(json.dumps(summary, indent=1))
    print(json.dumps(summary, indent=1))


if __name__ == "__main__":
    main()
