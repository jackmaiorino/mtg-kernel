#!/usr/bin/env python3
"""Summarize `regret_census_v1 mode=cond` rows (stage-2 conditions A-D).

Per root and (condition, budget level), the outcome is the share of paired
evaluation playouts that ended in a natural focal win (an independently
verified terminal success; non-natural endings and losses count 0). Mean
score (draws and non-natural endings as 0.5) is reported alongside.

The natural-win share is the primary statistic; non-natural endings are
counted and reported next to it.

Reports, for the representative set (overall and per stratum) and for the
mechanism set (per tag):
- win rate per condition and budget level, and the T1 reference, with
  non-natural counts;
- paired differences B-A, C-A, D-A, D-B, D-C with a 95% bootstrap clustered
  by corpus game (several roots can come from one game; one set of cluster
  draws is shared by every contrast of a summary);
- at the top budget, Holm-adjusted bootstrap p-values for the family B-A,
  C-A, D-B, D-C, and the interaction (D-C)-(B-A); everything else is
  descriptive;
- the share of roots where B is A by construction (coverage_differs false,
  which includes k <= K) and the coverage contrasts on coverage-differs roots;
- with EVAL_CROSS rows, the root-choice x continuation 2x2 per improved
  condition and level (model or search choice, plain or improved follow-up);
- outcome against selection transitions and against selection plus
  evaluation transitions, and against wall time per condition;
- cost: selection and evaluation transitions and wall time, clone failures,
  cap hits and the improved-continuation cost multiplier;
- Spy choice labels when the rows carry them.

`--expect MANIFEST.json` rejects rows whose recorded `config` differs from
the manifest's `config` object (or the whole manifest when it has none) on
any key the manifest lists, and rows without a recorded config.

Usage: analyze_search.py COND.jsonl [MORE.jsonl ...] [--boot 2000] [--seed 7]
       [--expect MANIFEST.json] [--json OUT]
"""

import argparse
import json
import random
import statistics
from collections import defaultdict

CONDS = ["A", "B", "C", "D"]
CONTRASTS = [("B", "A"), ("C", "A"), ("D", "A"), ("D", "B"), ("D", "C")]
HOLM_FAMILY = [("B", "A"), ("C", "A"), ("D", "B"), ("D", "C")]
IMPROVED = ["C", "D"]


def mean(xs):
    xs = list(xs)
    return sum(xs) / len(xs) if xs else float("nan")


def win(scores):
    return mean(1.0 if s == 1.0 else 0.0 for s in scores)


def per_root(row):
    """{label: (natural win share, mean score, n, non-natural count)} for ref
    and every cond@budget (and the EVAL_CROSS labels when present)."""
    jobs = row["eval"]["jobs"]
    out = {}
    for label, j in row["eval_jobs"].items():
        sc = jobs[j]["scores"]
        nn = jobs[j].get("non_natural", 0)
        out[label] = (win(sc), mean(sc), len(sc), nn)
    return out


def expected_config(manifest):
    return manifest.get("config", manifest)


def check_manifest(rows, manifest):
    """(accepted rows, [(row, reasons)]) against the manifest's config."""
    want = expected_config(manifest)
    accepted, rejected = [], []
    for r in rows:
        got = r.get("config")
        if not isinstance(got, dict):
            rejected.append((r, ["no recorded config"]))
            continue
        reasons = [f"{k}: {got.get(k)!r} != {v!r}" for k, v in sorted(want.items()) if got.get(k) != v]
        if reasons:
            rejected.append((r, reasons))
        else:
            accepted.append(r)
    return accepted, rejected


class ClusterBoot:
    """Percentile bootstrap that resamples corpus games (clusters of roots)
    with replacement. One set of draws serves every statistic of a summary."""

    def __init__(self, rows, n_boot, rng):
        games = [r.get("game", i) for i, r in enumerate(rows)]
        order = sorted(set(games), key=str)
        index = {g: i for i, g in enumerate(order)}
        self.members = [[] for _ in order]
        for i, g in enumerate(games):
            self.members[index[g]].append(i)
        k = len(order)
        self.draws = [[rng.randrange(k) for _ in range(k)] for _ in range(n_boot)] if k else []

    def stats(self, vals):
        """Bootstrap means of per-root values (None = root not in the contrast)."""
        out = []
        for draw in self.draws:
            xs = [vals[i] for c in draw for i in self.members[c] if vals[i] is not None]
            if xs:
                out.append(sum(xs) / len(xs))
        return out

    def ci(self, vals):
        st = sorted(self.stats(vals))
        if not st:
            return (float("nan"), float("nan"))
        return (st[int(0.025 * len(st))], st[min(len(st) - 1, int(0.975 * len(st)))])

    def pvalue(self, vals):
        """Two-sided bootstrap p-value for a zero mean."""
        st = self.stats(vals)
        if not st:
            return float("nan")
        lo = sum(1 for x in st if x <= 0) / len(st)
        hi = sum(1 for x in st if x >= 0) / len(st)
        return min(1.0, 2 * min(lo, hi))


def holm(pvals):
    """Holm step-down adjusted p-values ({label: p} -> {label: adjusted})."""
    items = sorted(pvals.items(), key=lambda kv: (kv[1] != kv[1], kv[1]))
    m = len(items)
    out, running = {}, 0.0
    for i, (label, p) in enumerate(items):
        if p != p:
            out[label] = float("nan")
            continue
        running = max(running, min(1.0, (m - i) * p))
        out[label] = running
    return out


def fmt(x, pct=True):
    if x != x:
        return "   n/a"
    return f"{100 * x:6.1f}" if pct else f"{x:8.1f}"


def diff_vals(vals, kx, ky, keep=None):
    """Per-root natural-win differences kx - ky (None where missing or not kept)."""
    out = []
    for i, v in enumerate(vals):
        if (keep is None or keep[i]) and kx in v and ky in v:
            out.append(v[kx][0] - v[ky][0])
        else:
            out.append(None)
    return out


def interaction(vals, b, keep=None):
    """Per-root (D-C)-(B-A) at budget b."""
    out = []
    for i, v in enumerate(vals):
        ks = [f"{c}@{b}" for c in CONDS]
        if (keep is None or keep[i]) and all(k in v for k in ks):
            a, bb, c, d = (v[k][0] for k in ks)
            out.append((d - c) - (bb - a))
        else:
            out.append(None)
    return out


def contrast_entry(boot, d):
    xs = [x for x in d if x is not None]
    lo, hi = boot.ci(d)
    return {"diff": mean(xs), "lo": lo, "hi": hi, "n": len(xs), "p": boot.pvalue(d)}


def two_by_two(vals, boot, budgets):
    """Root choice (model or search) x continuation (plain or improved) per
    improved condition and level, from the EVAL_CROSS labels."""
    out = {}
    for c in IMPROVED:
        for b in budgets:
            kx, kp = f"{c}@{b}", f"{c}@{b}/plain"
            keys = ["ref", "ref_improved", kp, kx]
            if not any(all(k in v for k in keys) for v in vals):
                continue
            cell = {k: mean(v[k][0] for v in vals if all(k2 in v for k2 in keys)) for k in keys}
            parts = {
                "continuation_model_action": diff_vals(vals, "ref_improved", "ref"),
                "continuation_search_action": diff_vals(vals, kx, kp),
                "root_choice_plain": diff_vals(vals, kp, "ref"),
                "root_choice_improved": diff_vals(vals, kx, "ref_improved"),
            }
            parts["interaction"] = [
                None if x is None or y is None else x - y
                for x, y in zip(parts["continuation_search_action"], parts["continuation_model_action"])
            ]
            out[f"{c}@{b}"] = {"cells": cell, "contrasts": {k: contrast_entry(boot, d) for k, d in parts.items()}}
    return out


def summarize(name, rows, budgets, n_boot, seed, out, k_max=4):
    vals = [per_root(r) for r in rows]
    boot = ClusterBoot(rows, n_boot, random.Random(seed))
    games = len({r.get("game") for r in rows})
    print(f"\n== {name}: {len(rows)} roots from {games} corpus games ==")
    header = "             " + "".join(f"{c + '@' + str(b):>12}" for b in budgets for c in CONDS) + "         ref"
    print(header)
    line = "win%        "
    nn_line = "non-natural "
    entry = {"roots": len(rows), "games": games, "win": {}, "mean_score": {}, "non_natural": {},
             "playouts": {}, "contrasts": {}}
    labels = [f"{c}@{b}" for b in budgets for c in CONDS] + ["ref"]
    for k in labels:
        have = [v[k] for v in vals if k in v]
        entry["win"][k] = mean(x[0] for x in have)
        entry["mean_score"][k] = mean(x[1] for x in have)
        entry["non_natural"][k] = sum(x[3] for x in have)
        entry["playouts"][k] = sum(x[2] for x in have)
        line += f"{fmt(entry['win'][k]):>12}"
        nn_line += f"{entry['non_natural'][k]:>12}"
    print(line)
    print(nn_line + f"   (of {entry['playouts']['ref']} playouts per label)")
    print("paired differences (pp, 95% bootstrap clustered by corpus game):")
    for b in budgets:
        for x, y in CONTRASTS + [("A", "ref")]:
            kx = f"{x}@{b}"
            ky = "ref" if y == "ref" else f"{y}@{b}"
            label = f"{x}-{y}@{b}"
            e = contrast_entry(boot, diff_vals(vals, kx, ky))
            entry["contrasts"][label] = e
            print(f"  {label:<12} {fmt(e['diff'])}  [{fmt(e['lo'])}, {fmt(e['hi'])}]")
    top = budgets[-1]
    pv = {f"{x}-{y}@{top}": entry["contrasts"][f"{x}-{y}@{top}"]["p"] for x, y in HOLM_FAMILY}
    adj = holm(pv)
    inter = contrast_entry(boot, interaction(vals, top))
    entry["holm"] = {k: {"p": pv[k], "p_holm": adj[k], "reject_05": adj[k] <= 0.05} for k in pv}
    entry["interaction"] = {f"(D-C)-(B-A)@{top}": inter}
    print(f"confirmatory family at the top budget {top} (Holm over bootstrap p-values; the rest is descriptive):")
    for k in pv:
        h = entry["holm"][k]
        print(f"  {k:<12} p {h['p']:.4f}  Holm {h['p_holm']:.4f}  {'reject' if h['reject_05'] else 'keep'} at 0.05")
    print(f"  interaction (D-C)-(B-A)@{top} {fmt(inter['diff'])}  [{fmt(inter['lo'])}, {fmt(inter['hi'])}]  p {inter['p']:.4f}")
    same = [not r.get("coverage_differs", True) for r in rows]
    small = [r.get("k", k_max + 1) <= k_max for r in rows]
    entry["b_equals_a"] = {"roots": sum(same), "share": mean(1.0 if s else 0.0 for s in same),
                           "k_le_K": sum(small), "K": k_max}
    print(f"B is A by construction on {sum(same)} of {len(rows)} roots "
          f"({fmt(entry['b_equals_a']['share'])}%; {sum(small)} with k <= {k_max}); "
          f"coverage contrasts on the {len(rows) - sum(same)} coverage-differs roots:")
    keep = [not s for s in same]
    entry["coverage_differs_contrasts"] = {}
    for b in budgets:
        for x, y in (("B", "A"), ("D", "C")):
            label = f"{x}-{y}@{b}"
            e = contrast_entry(boot, diff_vals(vals, f"{x}@{b}", f"{y}@{b}", keep))
            entry["coverage_differs_contrasts"][label] = e
            print(f"  {label:<12} {fmt(e['diff'])}  [{fmt(e['lo'])}, {fmt(e['hi'])}]  n {e['n']}")
        e = contrast_entry(boot, interaction(vals, b, keep))
        entry["coverage_differs_contrasts"][f"(D-C)-(B-A)@{b}"] = e
        print(f"  {'(D-C)-(B-A)@' + str(b):<12} {fmt(e['diff'])}  [{fmt(e['lo'])}, {fmt(e['hi'])}]  n {e['n']}")
    tbt = two_by_two(vals, boot, budgets)
    if tbt:
        entry["two_by_two"] = tbt
        print("root choice x continuation 2x2 (EVAL_CROSS; continuation check = ref_improved - ref):")
        for key, t in tbt.items():
            c = t["cells"]
            print(f"  {key}: model/plain {fmt(c['ref'])}  model/improved {fmt(c['ref_improved'])}  "
                  f"search/plain {fmt(c[key + '/plain'])}  search/improved {fmt(c[key])}")
            for k, e in t["contrasts"].items():
                print(f"    {k:<28} {fmt(e['diff'])}  [{fmt(e['lo'])}, {fmt(e['hi'])}]")
    out[name] = entry


def job_transitions(job):
    """Evaluation transitions of one job (its kept playouts)."""
    return job.get("transitions_total", sum(job["transitions"]))


def cost_curves(rows, budgets, out):
    print("\n== outcome against selection cost and selection + evaluation cost (all roots) ==")
    print(f"{'cond@budget':<12} {'sel transitions':>16} {'sel+eval trans':>15} {'sel wall s':>11} {'rounds':>7} {'win%':>7}")
    curves = {}
    vals = [per_root(r) for r in rows]
    ref_eval = mean(job_transitions(r["eval"]["jobs"][r["eval_jobs"]["ref"]]) for r in rows)
    for c in CONDS:
        for i, b in enumerate(budgets):
            lv = [r["selection"][c]["levels"][i] for r in rows]
            t = mean(x["transitions"] for x in lv)
            ev = mean(job_transitions(r["eval"]["jobs"][r["eval_jobs"][f"{c}@{b}"]]) for r in rows)
            wl = mean(x["wall"] for x in lv)
            rd = mean(x["rounds"] for x in lv)
            w = mean(v[f"{c}@{b}"][0] for v in vals)
            curves[f"{c}@{b}"] = {"transitions": t, "eval_transitions": ev, "sel_plus_eval_transitions": t + ev,
                                  "wall": wl, "rounds": rd, "win": w}
            print(f"{c + '@' + str(b):<12} {t:16.0f} {t + ev:15.0f} {wl:11.2f} {rd:7.2f} {fmt(w):>7}")
    w = mean(v["ref"][0] for v in vals)
    curves["ref"] = {"transitions": 0.0, "eval_transitions": ref_eval, "sel_plus_eval_transitions": ref_eval, "win": w}
    print(f"{'ref':<12} {0:16.0f} {ref_eval:15.0f} {'':>11} {'':>7} {fmt(w):>7}")
    totals = [r.get("compute", {}).get("total_transitions") for r in rows]
    if all(t is not None for t in totals):
        print(f"actual selection + evaluation transitions per root (shared playouts once): {mean(totals):.0f}")
        curves["actual_total_transitions_per_root"] = mean(totals)
    out["curves"] = curves


def spy_table(rows, out):
    """Spy choice labels per evaluation label and per selection level."""
    rows = [r for r in rows if any("spy" in j for j in r["eval"]["jobs"])]
    if not rows:
        return
    keys = ["self_offered", "self_chosen", "dr_offered", "dr_chosen", "both_chosen", "both_chosen_natural_win"]
    print(f"\n== Spy choice labels ({len(rows)} Spy-focal roots; evaluation playouts) ==")
    print(f"{'label':<16}{'playouts':>9}" + "".join(f"{k:>24}" for k in keys))
    table = {}
    labels = sorted({k for r in rows for k in r["eval_jobs"]}, key=lambda k: (k != "ref", k))
    for label in labels:
        jobs = [r["eval"]["jobs"][r["eval_jobs"][label]] for r in rows if label in r["eval_jobs"]]
        n = sum(len(j["scores"]) for j in jobs)
        counts = {k: sum(j.get("spy", {}).get(k, 0) for j in jobs) for k in keys}
        table[label] = dict(counts, playouts=n)
        print(f"{label:<16}{n:>9}" + "".join(f"{counts[k]:>24}" for k in keys))
    sel = {}
    for r in rows:
        for c in CONDS:
            for lv in r["selection"][c]["levels"]:
                if "spy" not in lv:
                    continue
                e = sel.setdefault(f"{c}@{lv['budget']}", {k: 0 for k in keys})
                for k in keys:
                    e[k] += sum(lv["spy"][k])
    if sel:
        print("selection playouts in complete rounds (all candidates):")
        for label, e in sorted(sel.items()):
            print(f"  {label:<14}" + "".join(f"{e[k]:>24}" for k in keys))
    out["spy"] = {"eval": table, "selection": sel}


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
    cap = [(j.get("cap_hits", 0), len(j["scores"])) for r in rows for j in r["eval"]["jobs"]
           if j["follow"].startswith("improved") and "cap_hits" in j]
    if cap:
        fails["eval_improved_cap_hits"] = sum(c for c, _ in cap)
        fails["eval_improved_playouts"] = sum(n for _, n in cap)
        menu = [j for r in rows for j in r["eval"]["jobs"] if j["follow"].startswith("improved")]
        print(f"cap hit in {fails['eval_improved_cap_hits']} of {fails['eval_improved_playouts']} improved eval playouts; "
              f"searched menu size mean {sum(j.get('searched_menu_sum', 0) for j in menu) / max(1, sum(j['searched_decisions'] for j in menu)):.1f}, "
              f"max {max((j.get('searched_menu_max', 0) for j in menu), default=0)}")
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


def load(paths):
    rows, errors, seen = [], [], set()
    for path in paths:
        with open(path) as f:
            for line in f:
                r = json.loads(line)
                if r.get("kind") == "cond":
                    key = r.get("root_id", r["root_index"])
                    if key not in seen:
                        seen.add(key)
                        rows.append(r)
                elif r.get("kind") == "error":
                    errors.append(r)
    rows.sort(key=lambda r: r["root_index"])
    return rows, errors


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cond", nargs="+", help="cond output files (rows deduplicated by root_id)")
    ap.add_argument("--boot", type=int, default=2000)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--json")
    ap.add_argument("--expect", help="manifest whose config every row must match")
    ap.add_argument("--project", default="600,100,16", help="representative,mechanism,E for the cost projection")
    a = ap.parse_args()
    rows, errors = load(a.cond)
    print("T1 TOOLING-QUALIFICATION RESULTS - not research conclusions (T1 never trained on Spy or CawGates).")
    print(f"rows {len(rows)}, error rows {len(errors)} (replay mismatches and other errors)")
    for e in errors:
        print(f"  error root {e.get('game')}: {str(e.get('error'))[:200]}")
    out = {"errors": len(errors)}
    if a.expect:
        with open(a.expect) as f:
            manifest = json.load(f)
        rows, rejected = check_manifest(rows, manifest)
        out["rejected"] = len(rejected)
        print(f"manifest {a.expect}: {len(rows)} rows match, {len(rejected)} rejected")
        for r, reasons in rejected[:10]:
            print(f"  rejected {r.get('root_id', r.get('root_index'))}: {'; '.join(reasons)[:300]}")
    else:
        configs = {json.dumps(r.get("config"), sort_keys=True) for r in rows}
        if len(configs) > 1:
            print(f"WARNING: rows carry {len(configs)} distinct configs; pass --expect to keep one")
    if not rows:
        return
    budgets = rows[0]["budgets"]
    k_max = (rows[0].get("config") or {}).get("max_actions", 4)
    print(f"budgets {budgets}, horizon {rows[0]['horizon']}, m_inner {rows[0]['m_inner']}, "
          f"eval playouts per job {rows[0]['eval']['playouts']}")
    print(f"coverage differs from top-K on {sum(r['coverage_differs'] for r in rows)} of {len(rows)} roots")
    print("primary statistic: natural-win share (non-natural endings and losses count 0)")
    rep = [r for r in rows if r.get("set") == "representative"]
    mech = [r for r in rows if str(r.get("set", "")).startswith("mechanism")]
    if rep:
        summarize("representative/all", rep, budgets, a.boot, a.seed, out, k_max)
        for s in sorted({r["stratum"] for r in rep}):
            summarize(f"representative/{s}", [r for r in rep if r["stratum"] == s], budgets, a.boot, a.seed, out, k_max)
    for tag in sorted({r.get("mechanism_tag") or r["set"] for r in mech}):
        summarize(f"mechanism/{tag}", [r for r in mech if (r.get("mechanism_tag") or r["set"]) == tag],
                  budgets, a.boot, a.seed, out, k_max)
    other = [r for r in rows if r not in rep and r not in mech]
    if other:
        summarize("unlabelled", other, budgets, a.boot, a.seed, out, k_max)
    cost_curves(rows, budgets, out)
    cost_table(rows, budgets, out)
    spy_table(rows, out)
    rep_n, mech_n, e_target = (int(x) for x in a.project.split(","))
    projection(rows, rep_n, mech_n, e_target, out)
    if a.json:
        with open(a.json, "w") as f:
            json.dump(out, f, indent=1)


if __name__ == "__main__":
    main()
