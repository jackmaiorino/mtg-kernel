#!/usr/bin/env python3
"""Summarize `regret_census_v1 mode=cond` rows (stage-2 conditions A-D).

Per root and (condition, budget level), the outcome is the share of paired
evaluation playouts that ended in a natural focal win (an independently
verified terminal success; non-natural endings and losses count 0). Mean
score (draws and non-natural endings as 0.5) is reported alongside.

Reports, for the representative set (overall and per stratum) and for the
mechanism set (per tag):
- win rate per condition and budget level, and the T1 reference;
- paired differences B-A, C-A, D-A, D-B, D-C with a 95% bootstrap over roots
  (one resample of roots shared by every contrast);
- outcome against mean selection transitions and wall time per condition;
- cost: selection and evaluation transitions and wall time, clone failures,
  and the improved-continuation cost multiplier.

Usage: analyze_search.py COND.jsonl [--boot 2000] [--seed 7] [--json OUT]
"""

import argparse
import json
import random
import statistics
from collections import defaultdict

CONDS = ["A", "B", "C", "D"]
CONTRASTS = [("B", "A"), ("C", "A"), ("D", "A"), ("D", "B"), ("D", "C")]


def mean(xs):
    xs = list(xs)
    return sum(xs) / len(xs) if xs else float("nan")


def win(scores):
    return mean(1.0 if s == 1.0 else 0.0 for s in scores)


def per_root(row):
    """{label: (natural win share, mean score, n)} for ref and every cond@budget."""
    jobs = row["eval"]["jobs"]
    out = {}
    for label, j in row["eval_jobs"].items():
        sc = jobs[j]["scores"]
        out[label] = (win(sc), mean(sc), len(sc))
    return out


def boot_ci(rows_vals, fn, n_boot, rng):
    """95% percentile bootstrap of fn over roots (rows_vals: list per root)."""
    n = len(rows_vals)
    if n == 0:
        return (float("nan"), float("nan"))
    stats = []
    for _ in range(n_boot):
        sample = [rows_vals[rng.randrange(n)] for _ in range(n)]
        v = fn(sample)
        if v == v:
            stats.append(v)
    stats.sort()
    if not stats:
        return (float("nan"), float("nan"))
    return (stats[int(0.025 * len(stats))], stats[min(len(stats) - 1, int(0.975 * len(stats)))])


def fmt(x, pct=True):
    if x != x:
        return "   n/a"
    return f"{100 * x:6.1f}" if pct else f"{x:8.1f}"


def summarize(name, rows, budgets, n_boot, seed, out):
    vals = [per_root(r) for r in rows]
    print(f"\n== {name}: {len(rows)} roots ==")
    header = "             " + "".join(f"{c + '@' + str(b):>12}" for b in budgets for c in CONDS) + "         ref"
    print(header)
    line = "win%        "
    entry = {"roots": len(rows), "win": {}, "mean_score": {}, "contrasts": {}}
    for b in budgets:
        for c in CONDS:
            w = mean(v[f"{c}@{b}"][0] for v in vals if f"{c}@{b}" in v)
            entry["win"][f"{c}@{b}"] = w
            entry["mean_score"][f"{c}@{b}"] = mean(v[f"{c}@{b}"][1] for v in vals if f"{c}@{b}" in v)
            line += f"{fmt(w):>12}"
    w = mean(v["ref"][0] for v in vals)
    entry["win"]["ref"] = w
    entry["mean_score"]["ref"] = mean(v["ref"][1] for v in vals)
    line += f"{fmt(w):>12}"
    print(line)
    rng = random.Random(seed)
    print("paired differences (pp, 95% bootstrap over roots):")
    for b in budgets:
        for x, y in CONTRASTS + [("A", "ref")]:
            kx = f"{x}@{b}"
            ky = "ref" if y == "ref" else f"{y}@{b}"
            diffs = [v[kx][0] - v[ky][0] for v in vals if kx in v and ky in v]
            d = mean(diffs)
            lo, hi = boot_ci(diffs, mean, n_boot, rng)
            label = f"{x}-{y}@{b}"
            entry["contrasts"][label] = {"diff": d, "lo": lo, "hi": hi, "n": len(diffs)}
            print(f"  {label:<12} {fmt(d)}  [{fmt(lo)}, {fmt(hi)}]")
    out[name] = entry


def cost_curves(rows, budgets, out):
    print("\n== outcome against selection cost (all roots) ==")
    print(f"{'cond@budget':<12} {'sel transitions':>16} {'sel wall s':>11} {'rounds':>7} {'win%':>7}")
    curves = {}
    vals = [per_root(r) for r in rows]
    for c in CONDS:
        for i, b in enumerate(budgets):
            lv = [r["selection"][c]["levels"][i] for r in rows]
            t = mean(x["transitions"] for x in lv)
            wl = mean(x["wall"] for x in lv)
            rd = mean(x["rounds"] for x in lv)
            w = mean(v[f"{c}@{b}"][0] for v in vals)
            curves[f"{c}@{b}"] = {"transitions": t, "wall": wl, "rounds": rd, "win": w}
            print(f"{c + '@' + str(b):<12} {t:16.0f} {wl:11.2f} {rd:7.2f} {fmt(w):>7}")
    out["curves"] = curves


def cost_table(rows, budgets, out):
    print("\n== cost per root (nominal = as if each condition ran alone) ==")
    sel = defaultdict(list)
    ev_t = defaultdict(list)
    ev_w = defaultdict(list)
    for r in rows:
        jobs = r["eval"]["jobs"]
        for c in CONDS:
            for i, b in enumerate(budgets):
                lv = r["selection"][c]["levels"][i]
                sel[(c, b)].append((lv["transitions"], lv["wall"]))
                j = jobs[r["eval_jobs"][f"{c}@{b}"]]
                ev_t[(c, b)].append(mean(j["transitions"]))
                ev_w[(c, b)].append(mean(j["wall"]))
        j = jobs[r["eval_jobs"]["ref"]]
        ev_t["ref"].append(mean(j["transitions"]))
        ev_w["ref"].append(mean(j["wall"]))
    print(f"{'cond@budget':<12} {'sel trans':>10} {'sel wall':>9} {'eval trans/playout':>19} {'eval wall/playout':>18}")
    table = {}
    for c in CONDS:
        for b in budgets:
            st = mean(x[0] for x in sel[(c, b)])
            sw = mean(x[1] for x in sel[(c, b)])
            et = mean(ev_t[(c, b)])
            ew = mean(ev_w[(c, b)])
            table[f"{c}@{b}"] = {"sel_transitions": st, "sel_wall": sw, "eval_transitions_per_playout": et, "eval_wall_per_playout": ew}
            print(f"{c + '@' + str(b):<12} {st:10.0f} {sw:9.2f} {et:19.0f} {ew:18.3f}")
    et, ew = mean(ev_t["ref"]), mean(ev_w["ref"])
    table["ref"] = {"eval_transitions_per_playout": et, "eval_wall_per_playout": ew}
    print(f"{'ref':<12} {'':>10} {'':>9} {et:19.0f} {ew:18.3f}")
    # Improved-continuation multiplier: per root, improved eval playouts
    # against plain T1 eval playouts from the same root.
    mult_t, mult_w = [], []
    for r in rows:
        plain_t, plain_w, imp_t, imp_w = [], [], [], []
        for j in r["eval"]["jobs"]:
            (imp_t if j["follow"].startswith("improved") else plain_t).extend(j["transitions"])
            (imp_w if j["follow"].startswith("improved") else plain_w).extend(j["wall"])
        if plain_t and imp_t and mean(plain_t) > 0:
            mult_t.append(mean(imp_t) / mean(plain_t))
            mult_w.append(mean(imp_w) / max(mean(plain_w), 1e-9))
    actual = {
        "selection_transitions": mean(r["selection_actual"]["transitions"] for r in rows),
        "selection_wall": mean(r["selection_actual"]["wall"] for r in rows),
        "eval_transitions": mean(r["eval"]["actual_transitions"] for r in rows),
        "eval_wall": mean(r["eval"]["actual_wall"] for r in rows),
        "root_wall": mean(r.get("root_wall", float("nan")) for r in rows),
        "replay_secs": mean(r["replay_secs"] for r in rows),
    }
    fails = {
        "eval_failed_playouts": sum(r["eval"]["failed_playouts"] for r in rows),
        "eval_discarded_dets": sum(r["eval"]["discarded_dets"] for r in rows),
        "eval_short_roots": sum(1 for r in rows if r["eval"]["playouts"] < max(1, len(r["eval"]["jobs"][0]["scores"]))),
        "selection_failed_rounds": sum(r["selection"][c]["failed_rounds"] for r in rows for c in CONDS),
        "selection_inner_failures": sum(r["selection"][c]["inner_failures"] for r in rows for c in CONDS),
        "eval_inner_failures": sum(j["inner_failures"] for r in rows for j in r["eval"]["jobs"]),
        "eval_non_natural": sum(j["non_natural"] for r in rows for j in r["eval"]["jobs"]),
        "eval_playouts_total": sum(len(j["scores"]) for r in rows for j in r["eval"]["jobs"]),
        "selection_exhausted_levels": sum(
            1 for r in rows for c in CONDS for lv in r["selection"][c]["levels"] if lv["exhausted"]
        ),
    }
    searched = [
        sum(j["searched_decisions"] for j in r["eval"]["jobs"] if j["follow"].startswith("improved"))
        / max(1, sum(len(j["scores"]) for j in r["eval"]["jobs"] if j["follow"].startswith("improved")))
        for r in rows
    ]
    print(f"actual compute per root (shared playouts counted once): {json.dumps({k: round(v, 2) for k, v in actual.items()})}")
    print(
        f"improved continuation cost multiplier vs plain T1 eval playouts: transitions x{mean(mult_t):.1f} "
        f"(median x{statistics.median(mult_t) if mult_t else float('nan'):.1f}), wall x{mean(mult_w):.1f}; "
        f"searched decisions per improved playout {mean(searched):.2f}"
    )
    print(f"failures and outcomes: {json.dumps(fails)}")
    out["cost"] = {"nominal": table, "actual_per_root": actual, "multiplier_transitions": mean(mult_t),
                   "multiplier_wall": mean(mult_w), "searched_per_improved_playout": mean(searched), "failures": fails}


def projection(rows, rep_n, mech_n, e_target, out):
    """Worker-hours for a full run: selection cost is independent of E and
    evaluation cost scales linearly with the playouts per job."""
    print(f"\n== projected full run: {rep_n} representative + {mech_n} mechanism roots, E = {e_target} ==")
    proj = {}
    total = 0.0
    for name, n, sel in (
        ("representative", rep_n, [r for r in rows if r.get("set") == "representative"]),
        ("mechanism", mech_n, [r for r in rows if str(r.get("set", "")).startswith("mechanism")]),
    ):
        if not sel:
            continue
        per = [
            r["replay_secs"] + r["selection_actual"]["wall"]
            + r["eval"]["actual_wall"] * e_target / max(1, r["eval"]["playouts"])
            for r in sel
        ]
        hours = mean(per) * n / 3600
        total += hours
        proj[name] = {"seconds_per_root": mean(per), "worker_hours": hours}
        print(f"{name:<16} {mean(per):8.1f} s/root (max {max(per):.1f}) -> {hours:7.1f} worker-hours")
    print(f"{'total':<16} {'':>8}           -> {total:7.1f} worker-hours "
          f"({total / 3:.1f} h on 3 workers, {total / 32:.1f} h on 32)")
    proj["total_worker_hours"] = total
    out["projection"] = proj


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cond")
    ap.add_argument("--boot", type=int, default=2000)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--json")
    ap.add_argument("--project", default="600,100,16", help="representative,mechanism,E for the cost projection")
    a = ap.parse_args()
    rows, errors = [], []
    with open(a.cond) as f:
        for line in f:
            r = json.loads(line)
            if r.get("kind") == "cond":
                rows.append(r)
            elif r.get("kind") == "error":
                errors.append(r)
    rows.sort(key=lambda r: r["root_index"])
    print("T1 TOOLING-QUALIFICATION RESULTS - not research conclusions (T1 never trained on Spy or CawGates).")
    print(f"rows {len(rows)}, error rows {len(errors)} (replay mismatches and other errors)")
    for e in errors:
        print(f"  error root {e.get('game')}: {str(e.get('error'))[:200]}")
    if not rows:
        return
    budgets = rows[0]["budgets"]
    print(f"budgets {budgets}, horizon {rows[0]['horizon']}, m_inner {rows[0]['m_inner']}, "
          f"eval playouts per job {rows[0]['eval']['playouts']}")
    print(f"coverage differs from top-K on {sum(r['coverage_differs'] for r in rows)} of {len(rows)} roots")
    out = {"errors": len(errors)}
    rep = [r for r in rows if r.get("set") == "representative"]
    mech = [r for r in rows if str(r.get("set", "")).startswith("mechanism")]
    if rep:
        summarize("representative/all", rep, budgets, a.boot, a.seed, out)
        for s in sorted({r["stratum"] for r in rep}):
            summarize(f"representative/{s}", [r for r in rep if r["stratum"] == s], budgets, a.boot, a.seed, out)
    for tag in sorted({r.get("mechanism_tag") or r["set"] for r in mech}):
        summarize(f"mechanism/{tag}", [r for r in mech if (r.get("mechanism_tag") or r["set"]) == tag],
                  budgets, a.boot, a.seed, out)
    other = [r for r in rows if r not in rep and r not in mech]
    if other:
        summarize("unlabelled", other, budgets, a.boot, a.seed, out)
    cost_curves(rows, budgets, out)
    cost_table(rows, budgets, out)
    rep_n, mech_n, e_target = (int(x) for x in a.project.split(","))
    projection(rows, rep_n, mech_n, e_target, out)
    if a.json:
        with open(a.json, "w") as f:
            json.dump(out, f, indent=1)


if __name__ == "__main__":
    main()
