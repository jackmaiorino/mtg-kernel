"""Development validation and fresh comparison of the allocator v2 arms (collab
LANES/allocator-redesign-20261012, DESIGN.md stages 2-3 and "Combo and timing mechanism").
Fixed before any outcome is read.

CLI: allocator2_report.py --schedule DIR --panel devval|fresh-panel --rows DIR [DIR ...]
         --arms LABEL ... [--learned LABEL ...] [--control q] [--secondary current]
         [--controllers LABEL:LINEAGE=W.json ...] [--initial LABEL:LINEAGE=W0.json ...]
         [--ledger FILE ...] --out RESULT.json

Rows are queue outputs named ``<panel>-<label>-<index:04d>.jsonl`` (``allocator2.py
panel-jobs``), index the line of ``cases-<panel>.jsonl``. Game score: win 1, draw 0.5, loss 0.
A missing, failed or non-natural game is unknown, never a loss: every mean carries worst
(unknown 0) and best (unknown 1) bounds.

Primary: learned minus q in Spy terminal score with the nine opposing decks weighted equally
(per opponent: mean over its families and both seats), per parent and as the equal-weight
mean of the two parents; current is secondary. With several learned labels (fresh
replications), each is reported and their equal-weight mean, plus the spread across them.
Uncertainty: a 10,000-draw bootstrap over (opponent, family) units, resampling families within
each opponent with the same indices for every arm and both parents (shared controls keep
their covariance; the pooled summary keeps the dependence across parents). Units with an
unknown game in a compared arm are left out of that contrast's bootstrap and counted. This is
evaluation uncertainty conditional on the trained controllers, never controller replication.

Mechanism and diagnostics (retrospective labels only, never inputs or rewards):
- completed self-mill to Dread Return to Giant followed by a natural win, per scheduled game
  (games that never offer Spy stay in the denominator); sequence completion regardless of
  result; wins by other lines; the full offer/choice/resolution funnel with, at each event,
  lookup, support, remaining allowance, request, new search and execution or fallback;
- timing windows: each legal Spy-cast offer's focal physical decision and the preceding two,
  merged; self-target and Dread Return target decisions labeled separately; consultations,
  positive-request rate, requested and expected spend inside and outside;
- early/middle/late thirds of each game's focal physical decisions: consultations, requests,
  searches, spend, supported choices; unspent allowance;
- per-request sequence discoveries in the search's own rollouts (unknown when absent);
- with --controllers and --initial: on the panel's saved q-arm consultation rows (a common
  outcome-blind sample), initial versus trained executable distributions within
  remaining-budget and age bins, and an age/budget-only baseline fitted on a disjoint half
  (split by case hash): the held-out residual of the trained positive-request probability and
  expected spend beyond the bin means.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from collections import Counter, defaultdict
from pathlib import Path

import numpy as np

_spec = importlib.util.spec_from_file_location("allocator2", Path(__file__).with_name("allocator2.py"))
A = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(A)

SCORE = {"win": 1.0, "draw": 0.5, "loss": 0.0}
BOOTSTRAP_DRAWS = 10_000
BOOTSTRAP_SEED = 20261012
BUDGET_BINS = (0.0, 0.25, 0.5, 0.75, 1.0000001)
AGES = ("early", "middle", "late")


def status(row):
    if row is None:
        return "unstarted"
    if row.get("kind") != "allocator2_game":
        return "failed"
    g = row["game"]
    if not g.get("valid"):
        return "failed_search" if "failed search" in str(g.get("invalid_reason")) else "non_natural"
    return "natural"


def score(row):
    return SCORE[row["game"]["end"]] if status(row) == "natural" else None


def mean(xs):
    xs = list(xs)
    return sum(xs) / len(xs) if xs else None


def bounded(values):
    lo = [0.0 if v is None else v for v in values]
    hi = [1.0 if v is None else v for v in values]
    return {"point": mean(v for v in values if v is not None), "worst": mean(lo), "best": mean(hi),
            "n": len(values), "unknown": sum(v is None for v in values)}


def opponent_equal(per_opp: dict, key: str):
    vals = [per_opp[o][key] for o in range(9)]
    return None if any(v is None for v in vals) else sum(vals) / 9


def load_panel(schedule: Path, panel: str, rows_dirs, labels):
    cases = A.load_cases(schedule, panel)
    games = {}  # (label, index) -> row or None
    for label in labels:
        for i in range(len(cases)):
            name = f"{panel}-{label}-{i:04d}.jsonl"
            row = None
            for d in rows_dirs:
                if (Path(d) / name).exists():
                    row = A.read_row(Path(d) / name)
                    break
            games[(label, i)] = row
    return cases, games


def unit_of(c):
    return (c["case"]["other"], c["family"])


def arm_scores(cases, games, label, lineage):
    """{opponent: {family: [scores of both seats]}} of one arm and parent."""
    out = defaultdict(lambda: defaultdict(list))
    for i, c in enumerate(cases):
        if c["lineage"] == lineage:
            out[c["case"]["other"]][c["family"]].append(score(games[(label, i)]))
    return out


def summary(scores):
    per_opp = {o: bounded([v for fam in scores[o].values() for v in fam]) for o in range(9)}
    return {"opponents": per_opp, "spy_score": {k: opponent_equal(per_opp, k) for k in ("point", "worst", "best")}}


def contrast_point(a, b):
    pa, pb = a["spy_score"], b["spy_score"]
    sub = lambda x, y: None if x is None or y is None else x - y  # noqa: E731
    return {"point": sub(pa["point"], pb["point"]), "lower": sub(pa["worst"], pb["best"]),
            "upper": sub(pa["best"], pb["worst"])}


def unit_means(cases, games, labels):
    """{lineage: {label: {(opp, family): mean seat score or None}}}."""
    out = {lin: {lab: {} for lab in labels} for lin in A.LINEAGES}
    for lab in labels:
        for lin in A.LINEAGES:
            sc = arm_scores(cases, games, lab, lin)
            for o in range(9):
                for fam, vals in sc[o].items():
                    out[lin][lab][(o, fam)] = None if any(v is None for v in vals) else sum(vals) / len(vals)
    return out


def bootstrap(units, pairs, families):
    """Joint stratified bootstrap. units: {lineage: {label: {(o, f): value}}}; pairs: [(a, b)].
    Same family indices for every arm and both parents. Returns percentile intervals."""
    rng = np.random.Generator(np.random.PCG64(BOOTSTRAP_SEED))
    fams = sorted(families)
    draws = {f"{a}-{b}": {"r1": [], "r2": [], "pooled": []} for a, b in pairs}
    complete = {}
    for a, b in pairs:
        complete[f"{a}-{b}"] = {lin: sum(1 for k in units[lin][a] if units[lin][a][k] is not None
                                         and units[lin][b].get(k) is not None) for lin in A.LINEAGES}
    for _ in range(BOOTSTRAP_DRAWS):
        idx = {o: rng.integers(0, len(fams), size=len(fams)) for o in range(9)}
        for a, b in pairs:
            per_lin = {}
            for lin in A.LINEAGES:
                opp_means = []
                for o in range(9):
                    diffs = [units[lin][a].get((o, fams[j])) for j in idx[o]]
                    base = [units[lin][b].get((o, fams[j])) for j in idx[o]]
                    d = [x - y for x, y in zip(diffs, base) if x is not None and y is not None]
                    if d:
                        opp_means.append(sum(d) / len(d))
                per_lin[lin] = sum(opp_means) / len(opp_means) if len(opp_means) == 9 else None
            for lin in A.LINEAGES:
                if per_lin[lin] is not None:
                    draws[f"{a}-{b}"][lin].append(per_lin[lin])
            if None not in per_lin.values():
                draws[f"{a}-{b}"]["pooled"].append((per_lin["r1"] + per_lin["r2"]) / 2)
    out = {}
    for k, v in draws.items():
        out[k] = {}
        for lin, xs in v.items():
            if not xs:
                out[k][lin] = None
                continue
            xs = sorted(xs)
            out[k][lin] = {"lo95": xs[int(0.025 * len(xs))], "hi95": xs[int(0.975 * len(xs)) - 1],
                           "se": float(np.std(xs)), "draws": len(xs)}
        out[k]["complete_units"] = complete[k]
    return out


# ---------------------------------------------------------------- mechanism diagnostics

def windows(decisions):
    """Focal physical indices inside a Spy-cast timing window (offer and the preceding two)."""
    inside = set()
    for d in decisions:
        if d.get("spy_cast_offered"):
            p = d.get("focal_physical")
            if p is not None:
                inside.update({p - 2, p - 1, p})
    return inside


def age_bin(p, total):
    if total <= 0:
        return "early"
    f = p / total
    return AGES[0] if f < 1 / 3 else AGES[1] if f < 2 / 3 else AGES[2]


def expected_spend(s):
    return sum(m * a for m, a in zip(s["mu"], s["amounts"]))


def game_mechanism(row):
    g = row["game"]
    dec = g.get("decisions", [])
    seq = g.get("sequence") or {}
    total_phys = 1 + max((d.get("focal_physical", -1) for d in dec), default=-1)
    win_set = windows(dec)
    by_window = {k: Counter() for k in ("inside", "outside")}
    by_age = {k: Counter() for k in AGES}
    events = []
    for d in dec:
        p = d.get("focal_physical")
        s = d.get("scheduler")
        where = "inside" if p in win_set else "outside"
        age = age_bin(p if p is not None else 0, total_phys)
        for acc in (by_window[where], by_age[age]):
            acc["decisions"] += 1
            acc["supported"] += int(d["chooser"] in ("new_root", "reused"))
            acc["searches"] += int(d.get("refreshed") is not None)
            acc["spent"] += d["remaining_before"] - d["remaining_after"]
            if s:
                acc["consultations"] += 1
                acc["positive_requests"] += int(s["amount"] > 0)
                acc["requested"] += s["amount"]
                acc["expected_spend_x1000"] += round(expected_spend(s) * 1000)
        labels = [k for k in ("spy_cast_offered", "spy_cast_chosen", "spy_self_offered", "spy_self_chosen",
                              "dr_giant_offered", "dr_giant_chosen") if d.get(k)]
        if labels:
            events.append({"labels": labels, "focal_physical": p, "lookup": d["lookup"], "chooser": d["chooser"],
                           "remaining_before": d["remaining_before"],
                           "request": None if not s else s["amount"], "new_search": d.get("refreshed")})
    searches = g.get("searches", [])
    disc = [s.get("discovery") for s in searches]
    known = [x for x in disc if x is not None]
    return {
        "sequence_complete": bool(seq.get("complete")),
        "sequence_win": bool(seq.get("complete")) and g.get("end") == "win",
        "win_other_line": g.get("end") == "win" and not seq.get("complete"),
        "funnel": {k: bool(seq.get(k)) for k in ("cast_offered", "cast_chosen", "self_offered", "self_chosen",
                                                  "self_target_resolved", "dr_giant_offered", "dr_giant_chosen",
                                                  "dr_giant_resolved")},
        "events": events, "windows": {k: dict(v) for k, v in by_window.items()},
        "ages": {k: dict(v) for k, v in by_age.items()},
        "unspent": row["cap"] - g.get("transitions_used", 0),
        "searches": len(searches),
        "discoveries": None if len(known) < len(disc) else sum(x["completions_natural"] for x in known),
        "discovering_requests": None if len(known) < len(disc) else sum(int(x["completions_natural"] > 0) for x in known),
    }


def arm_mechanism(cases, games, label, lineage=None):
    n = 0
    out = Counter()
    funnel = Counter()
    win, age = Counter(), {k: Counter() for k in AGES}
    wins = {k: Counter() for k in ("inside", "outside")}
    disc_unknown = 0
    unspent = []
    for i, c in enumerate(cases):
        if lineage and c["lineage"] != lineage:
            continue
        n += 1
        row = games[(label, i)]
        if status(row) != "natural":
            out["unknown"] += 1
            continue
        m = game_mechanism(row)
        out["sequence_complete"] += m["sequence_complete"]
        out["sequence_win"] += m["sequence_win"]
        out["win_other_line"] += m["win_other_line"]
        out[row["game"]["end"]] += 1
        funnel.update({k: int(v) for k, v in m["funnel"].items()})
        for k in AGES:
            age[k].update(m["ages"][k])
        for k in wins:
            wins[k].update(m["windows"][k])
        if m["discoveries"] is None:
            disc_unknown += 1
        else:
            win["discoveries"] += m["discoveries"]
            win["discovering_requests"] += m["discovering_requests"]
        win["searches"] += m["searches"]
        unspent.append(m["unspent"])
    rate = lambda k: out[k] / n if n else None  # noqa: E731
    return {"scheduled": n, "counts": dict(out), "sequence_win_rate": rate("sequence_win"),
            "sequence_win_rate_bounds": {"worst": rate("sequence_win"),
                                         "best": (out["sequence_win"] + out["unknown"]) / n if n else None},
            "sequence_complete_rate": rate("sequence_complete"), "funnel": dict(funnel),
            "windows": {k: dict(v) for k, v in wins.items()}, "ages": {k: dict(v) for k, v in age.items()},
            "search_discovery": dict(win), "discovery_unknown_games": disc_unknown,
            "mean_unspent": mean(unspent)}


# ---------------------------------------------------------------- context dependence

def case_half(cid: str) -> int:
    return hashlib.sha256(cid.encode()).digest()[0] % 2


def context_dependence(cases, games, sample_label, controllers, initial):
    """Initial versus trained executable distributions on the panel's saved ``sample_label``
    consultations, by remaining-budget and age bins, and the held-out residual of an
    age/budget-only bin baseline fitted on the other half (split by case hash)."""
    out = {}
    for key, path in controllers.items():
        label, lin = key
        w = A.read_weights(Path(path).read_bytes())
        w0 = A.read_weights(Path(initial[key]).read_bytes()) if key in initial else None
        rows = []
        for i, c in enumerate(cases):
            if c["lineage"] != lin:
                continue
            row = games.get((sample_label, i))
            if status(row) != "natural":
                continue
            dec = row["game"]["decisions"]
            total = 1 + max((d.get("focal_physical", -1) for d in dec), default=-1)
            for d in dec:
                s = d.get("scheduler")
                if not s or not s.get("nontrivial", True):
                    continue
                q = A.base_q(s["remaining"], s["cap"])
                cs = A.choices(s["remaining"])
                x = A.decision_x(w["arm"], s)
                amounts = np.array([a for a, _ in cs], dtype=float)

                def dist(ww):
                    _, _, actor, _ = A.forward(ww["params"], ww["norm"], x)
                    return A.coalesce(A.policy(q, actor), cs)
                pi = dist(w)
                pi0 = dist(w0) if w0 else A.coalesce(q, cs)
                frac = s["remaining"] / s["cap"]
                rows.append({"half": case_half(c["id"]), "budget": int(np.searchsorted(BUDGET_BINS, frac, side="right") - 1),
                             "age": age_bin(d.get("focal_physical", 0), total),
                             "positive": float(1 - pi[0]), "spend": float((pi * amounts).sum()),
                             "positive0": float(1 - pi0[0]), "spend0": float((pi0 * amounts).sum()),
                             "tv": 0.5 * float(np.abs(pi - pi0).sum())})
        cells = defaultdict(list)
        for r in rows:
            cells[(r["budget"], r["age"])].append(r)
        by_cell = {f"b{b}-{a}": {"rows": len(v), "positive_initial": mean(r["positive0"] for r in v),
                                 "positive_trained": mean(r["positive"] for r in v),
                                 "spend_initial": mean(r["spend0"] for r in v),
                                 "spend_trained": mean(r["spend"] for r in v),
                                 "tv_initial_trained": mean(r["tv"] for r in v)}
                   for (b, a), v in sorted(cells.items())}
        fit = defaultdict(list)
        for r in rows:
            if r["half"] == 0:
                fit[(r["budget"], r["age"])].append(r)
        held = [r for r in rows if r["half"] == 1 and (r["budget"], r["age"]) in fit]
        resid = {}
        for k in ("positive", "spend"):
            if not held:
                resid[k] = None
                continue
            pred = [mean(x[k] for x in fit[(r["budget"], r["age"])]) for r in held]
            vals = [r[k] for r in held]
            mu = mean(vals)
            total = mean((v - mu) ** 2 for v in vals)
            res = mean((v - p) ** 2 for v, p in zip(vals, pred))
            resid[k] = {"held_rows": len(held), "total_variance": total, "residual_variance": res,
                        "share_beyond_age_budget": None if not total else res / total}
        out[f"{label}:{lin}"] = {"rows": len(rows), "cells": by_cell, "age_budget_baseline": resid}
    return out


# ---------------------------------------------------------------- costs

def costs(cases, games, label, ledgers):
    spent, searches, walls, ctl = [], [], [], []
    for i, _ in enumerate(cases):
        row = games[(label, i)]
        if row is None or row.get("kind") != "allocator2_game":
            continue
        g = row["game"]
        spent.append(g.get("transitions_used", 0))
        searches.append(len(g.get("searches", [])))
        walls.append(row.get("wall", 0.0))
        ctl.append((row.get("scheduler") or {}).get("controller_seconds", 0.0))
    cpu = [r["cpu_seconds"] for r in ledgers if r.get("kind") == "process" and f"-{label}-" in r.get("job", "")]
    return {"games": len(spent), "mean_transitions": mean(spent), "mean_searches": mean(searches),
            "mean_wall": mean(walls), "mean_controller_seconds": mean(ctl),
            "process_cpu_seconds": sum(cpu) if cpu else None, "processes": len(cpu)}


def parse_map(items):
    out = {}
    for it in items or []:
        k, path = it.split("=", 1)
        label, lin = k.split(":")
        out[(label, lin)] = path
    return out


def report(schedule, panel, rows_dirs, labels, learned, control, secondary, controllers=None, initial=None,
           ledger_files=()):
    cases, games = load_panel(Path(schedule), panel, rows_dirs, labels)
    statuses = Counter(f"{lab}:{status(games[(lab, i)])}" for lab in labels for i in range(len(cases)))
    arms = {lab: {lin: summary(arm_scores(cases, games, lab, lin)) for lin in A.LINEAGES} for lab in labels}
    terminal = {lab: {lin: dict(Counter((games[(lab, i)] or {}).get("game", {}).get("end", "unknown")
                                        if status(games[(lab, i)]) == "natural" else "unknown"
                                        for i, c in enumerate(cases) if c["lineage"] == lin))
                      for lin in A.LINEAGES} for lab in labels}
    pairs = [(lab, control) for lab in learned] + [(lab, secondary) for lab in learned if secondary]
    if secondary:
        pairs.append((control, secondary))
    contrasts = {}
    for a, b in pairs:
        per = {lin: contrast_point(arms[a][lin], arms[b][lin]) for lin in A.LINEAGES}
        pooled = {k: None if None in (per["r1"][k], per["r2"][k]) else (per["r1"][k] + per["r2"][k]) / 2
                  for k in ("point", "lower", "upper")}
        by_opp = {lin: {o: (None if arms[a][lin]["opponents"][o]["point"] is None
                            or arms[b][lin]["opponents"][o]["point"] is None
                            else arms[a][lin]["opponents"][o]["point"] - arms[b][lin]["opponents"][o]["point"])
                        for o in range(9)} for lin in A.LINEAGES}
        contrasts[f"{a}-{b}"] = {"r1": per["r1"], "r2": per["r2"], "pooled": pooled, "by_opponent": by_opp}
    if len(learned) > 1:
        pts = [contrasts[f"{lab}-{control}"]["pooled"]["point"] for lab in learned]
        known = [p for p in pts if p is not None]
        contrasts["learned_mean-" + control] = {
            "replications": dict(zip(learned, pts)), "mean": mean(known) if len(known) == len(pts) else None,
            "spread_sd": float(np.std(known)) if len(known) == len(pts) else None}
    units = unit_means(cases, games, labels)
    boot = bootstrap(units, pairs, sorted({c["family"] for c in cases}))
    mech = {lab: {"pooled": arm_mechanism(cases, games, lab),
                  **{lin: arm_mechanism(cases, games, lab, lin) for lin in A.LINEAGES}} for lab in labels}
    ledgers = []
    for f in ledger_files:
        ledgers += [json.loads(ln) for ln in Path(f).read_text().splitlines() if ln.strip()]
    out = {"schema": "allocator-redesign-v1/report", "panel": panel, "labels": labels, "learned": learned,
           "control": control, "secondary": secondary, "cases": len(cases), "statuses": dict(statuses),
           "arms": arms, "terminal_counts": terminal, "contrasts": contrasts, "bootstrap": boot,
           "mechanism": mech, "costs": {lab: costs(cases, games, lab, ledgers) for lab in labels}}
    if controllers:
        out["context_dependence"] = context_dependence(cases, games, control, controllers, initial or {})
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--schedule", type=Path, required=True)
    ap.add_argument("--panel", required=True)
    ap.add_argument("--rows", nargs="+", required=True)
    ap.add_argument("--arms", nargs="+", required=True)
    ap.add_argument("--learned", nargs="+", default=["S", "H"])
    ap.add_argument("--control", default="q")
    ap.add_argument("--secondary", default="current")
    ap.add_argument("--controllers", nargs="*")
    ap.add_argument("--initial", nargs="*")
    ap.add_argument("--ledger", nargs="*", default=[])
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    out = report(a.schedule, a.panel, a.rows, a.arms, a.learned, a.control, a.secondary or None,
                 parse_map(a.controllers), parse_map(a.initial), a.ledger)
    a.out.write_text(json.dumps(out, indent=1, default=str) + "\n")
    print(json.dumps({"statuses": out["statuses"],
                      "contrasts": {k: v.get("pooled", v) for k, v in out["contrasts"].items()}}, default=str))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
