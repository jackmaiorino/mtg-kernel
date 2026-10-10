#!/usr/bin/env python3
"""Stage 4a frozen analysis (RUNNER.md sections 6-7, DESIGN.md "Frozen decision"). Stdlib only.

Usage: python analyze_stage4a.py --roots FROZEN.jsonl --rows r1.jsonl r2.jsonl --out DIR [--allow-nonformal]
Writes DIR/analysis.json and DIR/analysis.txt. Exit 0 even when gates fail; non-zero only for bad usage or a
non-formal input rejected without --allow-nonformal (exit 2).

Frozen procedure
  Models r1 and r2 are analysed separately, never pooled. Unit = root (= source game); root mean = mean over
  the 16 scheduled worlds. Estimator = cell-weighted mean (each non-empty cell 25%) of the per-cell mean of
  root-level paired differences, in percentage points. Integer events = sum over roots and worlds of
  (E outcome - control outcome) (out of 1,600 for a complete model).
  Observed version: unknown outcomes count 0 (rows must keep w=j=false when unknown).
  Conservative version (used for gates, in addition to observed): E unknown -> 0, control unknown -> 1,
  for both W and J.
  Bootstrap: 10,000 draws. Each model gets its own random.Random(2026100943). Per draw, for each cell in
  fixed order cast/T1, cast/A48, target/T1, target/A48, draw n_cell indices with rng.randrange(n_cell)
  (n_cell = roots present in that cell, 25 when complete), whole roots with replacement; all 8 contrasts x
  {observed, conservative} reuse the same draws (joint resampling). Percentiles: Hyndman-Fan type 7
  (numpy 'linear'), implemented here. Ordinary 95% = quantiles 0.025/0.975; Bonferroni 99.375% =
  0.003125/0.996875.
  Gates: a contrast passes when integer events >= 80 AND the Bonferroni lower bound > 0, on BOTH the observed
  and the conservative version. Selection discovery gate (E): >=50/100 roots and >=25/50 in each stage.
  Independent J coverage gate (E): roots with at least one of 16 eval outcomes j true, same thresholds.

Ambiguities resolved conservatively
  * Validity failures (missing/duplicate/extra root, error row, unparseable line, invalid flag, rejected eval
    worlds, selection fault or incomplete, eval fault, malformed arm/eval list, inconsistent flags such as
    w and unknown, j without w, summary mismatch, non-finite number, row game/seed/cell/stratum differing
    from the frozen roots file, roots file not exactly 100 roots with 25 per cell) -> "Invalid/incomplete".
    Structurally malformed roots are excluded from numbers, which are then labelled descriptive only.
  * Empty cell: its weight is dropped and the others renormalised (descriptive; the model is invalid anyway).
  * Rows with an unknown "kind" are listed as warnings only. Missing config.limits.formal counts as non-formal.
  * cost.selection_transitions / cost.eval_transitions are reported as given (a number, or a dict summed over
    values) next to the per-arm sums; a mismatch is a warning. Per-arm cost uses arms.X.selection.transitions,
    arms.X.selection.inference_calls and the sum of arms.X.eval[].transitions.
  * timing.root_wall is per root over all arms, so "worker-seconds per natural win" divides the all-arm total
    by each arm's natural wins (descriptive; the transitions-per-win column is arm specific).
  * Disposition precedence: Invalid/incomplete (any model) > Discovery inadequate (E selection discovery
    fails in either model) > Discovery qualified, usefulness unproved (J coverage fails or any of eight
    contrasts fails on observed or conservative) > Candidate experience generator qualified.
  * --draws other than 10,000 is for tests only and is stamped NON-FROZEN in the outputs.
"""
import argparse
import json
import math
import os
import random
import sys

CELLS = ["cast/T1", "cast/A48", "target/T1", "target/A48"]
STRATA = ["cast", "target"]
ARMS = ["E", "A", "D"]
MODELS = ["r1", "r2"]
WORLDS = 16
ROOTS_PER_CELL = 25
ROOTS_PER_MODEL = 100
BOOT_SEED = 2026100943
N_BOOT = 10000
ORD_Q = (0.025, 0.975)
BONF_Q = (0.003125, 0.996875)
MIN_EVENTS = 80
GATE_ALL, GATE_STAGE = 50, 25
ENDS = ("win", "nonwin", "nonnatural", "truncated", "fault")
CONTRASTS = [(c, e) for e in ("W", "J") for c in ("A", "D")]  # E-A/W, E-D/W, E-A/J, E-D/J
DISPOSITIONS = ["Invalid/incomplete", "Discovery inadequate",
                "Discovery qualified, usefulness unproved", "Candidate experience generator qualified"]


# ---------------------------------------------------------------- basic helpers
def quantile7(xs, p):
    """Hyndman-Fan type 7 on a sorted list. None for an empty list."""
    n = len(xs)
    if n == 0:
        return None
    h = (n - 1) * p
    lo = int(math.floor(h))
    hi = min(lo + 1, n - 1)
    return xs[lo] + (h - lo) * (xs[hi] - xs[lo])


def _bool(x):
    if isinstance(x, bool):
        return x
    if isinstance(x, int) and x in (0, 1):
        return bool(x)
    raise ValueError("not a boolean: %r" % (x,))


def _num(x):
    if isinstance(x, bool) or not isinstance(x, (int, float)) or not math.isfinite(x):
        raise ValueError("not a finite number: %r" % (x,))
    return x


def _reject_const(name):
    raise ValueError("non-finite JSON constant " + name)


def load_jsonl(path):
    rows, bad = [], 0
    with open(path, "r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line, parse_constant=_reject_const)
                if not isinstance(obj, dict):
                    raise ValueError("not an object")
                rows.append(obj)
            except ValueError:
                bad += 1
    return rows, bad


# ---------------------------------------------------------------- row normalisation
def normalize_arm(a):
    sel, d = a["selection"], a["selection"]["discovery"]
    ev = a["eval"]
    if not isinstance(ev, list) or len(ev) != WORLDS:
        raise ValueError("eval list must have %d entries" % WORLDS)
    worlds = {}
    for e in ev:
        w = e["world"]
        if isinstance(w, bool) or not isinstance(w, int) or not 0 <= w < WORLDS or w in worlds:
            raise ValueError("bad or duplicate eval world %r" % (w,))
        win, j, unk = _bool(e["w"]), _bool(e["j"]), _bool(e["unknown"])
        end = e["end"]
        if end not in ENDS:
            raise ValueError("bad end %r" % (end,))
        if win and unk:
            raise ValueError("world %d: w and unknown both true" % w)
        if j and not win:
            raise ValueError("world %d: j without w" % w)
        if (end == "win") != win or unk != (end in ("nonnatural", "truncated", "fault")):
            raise ValueError("world %d: end=%s inconsistent with w/unknown" % (w, end))
        worlds[w] = (win, j, unk, end, _num(e["transitions"]))
    evs = [worlds[i] for i in range(WORLDS)]
    summ = a.get("summary")
    if isinstance(summ, dict):
        calc = {"w": sum(x[0] for x in evs), "j": sum(x[1] for x in evs), "unknown": sum(x[2] for x in evs),
                "faults": sum(x[3] == "fault" for x in evs)}
        for k, v in calc.items():
            if k in summ and _num(summ[k]) != v:
                raise ValueError("summary.%s=%r != eval-derived %d" % (k, summ[k], v))
    return {"disc": _bool(d["discovered"]),
            "comp_nat": _num(d.get("completions_natural", 0)),
            "comp_win": _num(d.get("completion_wins", 0)),
            "comp_seeds": _num(d.get("completion_distinct_seeds", 0)),
            "sel_incomplete": _bool(sel.get("incomplete", False)),
            "sel_faults": len(sel.get("faults") or []),
            "cap": _bool(sel.get("cap_reached", False)),
            "sel_trans": _num(sel["transitions"]), "infer": _num(sel["inference_calls"]),
            "eval": evs}


def _cost_total(x):
    if isinstance(x, dict):
        return sum(_num(v) for v in x.values())
    return _num(x)


def normalize_row(row):
    """Return (normalised dict, flags) or raise ValueError/KeyError/TypeError for structural defects."""
    arms = row["arms"]
    out = {"arms": {k: normalize_arm(arms[k]) for k in ARMS},
           "invalid": _bool(row.get("invalid", False)),
           "rejected": len(row.get("rejected_eval_worlds") or []),
           "formal": row.get("config", {}).get("limits", {}).get("formal") is True,
           "root_wall": _num(row["timing"]["root_wall"]),
           "cost_sel": _cost_total(row["cost"]["selection_transitions"]),
           "cost_eval": _cost_total(row["cost"]["eval_transitions"])}
    return out


# ---------------------------------------------------------------- ingestion
def ingest(row_sets):
    """row_sets: list of (name, rows, bad_lines). Returns by_key, error_rows, warnings, bad_total."""
    by_key, errors, warnings, bad_total, dups = {}, {}, [], 0, {}
    for name, rows, bad in row_sets:
        bad_total += bad
        for r in rows:
            kind = r.get("kind")
            if kind == "error":
                errors.setdefault(r.get("model"), []).append(str(r.get("message", r.get("error", "")))[:200])
            elif kind == "s4a_root":
                k = (r.get("model"), r.get("root_id"))
                if k in by_key:
                    dups[k] = dups.get(k, 1) + 1
                else:
                    by_key[k] = r
            else:
                warnings.append("%s: ignored row of kind %r" % (name, kind))
    return by_key, errors, warnings, bad_total, dups


# ---------------------------------------------------------------- bootstrap
def cell_weighted(vec, idxs, active, sizes):
    tot = 0.0
    for c in active:
        v = vec[c]
        tot += sum(v[i] for i in idxs[c]) / (sizes[c] * WORLDS)
    return 100.0 * tot / len(active)


def bootstrap(vecs, sizes, n_boot, seed=BOOT_SEED):
    """vecs: key -> {cell: [integer events per root]}. Returns key -> sorted bootstrap statistics (pp)."""
    rng = random.Random(seed)
    active = [c for c in CELLS if sizes[c] > 0]
    keys = list(vecs)
    out = {k: [] for k in keys}
    if not active:
        return out
    for _ in range(n_boot):
        idxs = {c: [rng.randrange(sizes[c]) for _ in range(sizes[c])] for c in active}
        for k in keys:
            out[k].append(cell_weighted(vecs[k], idxs, active, sizes))
    for k in keys:
        out[k].sort()
    return out


# ---------------------------------------------------------------- per-model analysis
def contrast_events(e, c, endpoint, conservative):
    ix = 0 if endpoint == "W" else 1
    tot = 0
    for ee, cc in zip(e["eval"], c["eval"]):
        cv = 1 if (conservative and cc[2]) else cc[ix]
        tot += ee[ix] - cv
    return tot


def analyze_model(model, sched, by_key, errors, dups, n_boot):
    res = {"model": model, "problems": [], "warnings": []}
    prob = res["problems"]
    cells_sched = {c: [r for r in sched if r.get("cell") == c] for c in CELLS}
    if len(sched) != ROOTS_PER_MODEL or any(len(v) != ROOTS_PER_CELL for v in cells_sched.values()):
        prob.append("frozen roots file: %d roots, per cell %s (need %d, 25 per cell)" %
                    (len(sched), {c: len(v) for c, v in cells_sched.items()}, ROOTS_PER_MODEL))
    ids = [r["root_id"] for r in sched]
    if len(set(ids)) != len(ids):
        prob.append("frozen roots file: duplicate root_id")
    for r in sched:
        if r.get("cell") not in CELLS or r.get("stratum") != str(r.get("cell")).split("/")[0]:
            prob.append("frozen roots file: root %s has inconsistent cell/stratum" % r.get("root_id"))
    sched_ids = set(ids)
    extra = [k[1] for k in by_key if k[0] == model and k[1] not in sched_ids]
    if extra:
        prob.append("%d unscheduled root rows for %s (e.g. %s)" % (len(extra), model, extra[:3]))
    for k, n in dups.items():
        if k[0] == model:
            prob.append("root %s appears %d times (first kept)" % (k[1], n))
    nerr = len(errors.get(model, [])) + len(errors.get(None, []))
    if nerr:
        prob.append("%d error rows (%s)" % (nerr, "; ".join((errors.get(model, []) + errors.get(None, []))[:3])))
    res["error_rows"] = nerr

    usable, missing, nonformal, flagged = {}, [], 0, 0
    for r in sched:
        raw = by_key.get((model, r["root_id"]))
        if raw is None:
            missing.append(r["root_id"])
            continue
        try:
            n = normalize_row(raw)
            for fld in ("game", "seed", "cell", "stratum"):
                if fld in r and raw.get(fld) != r[fld]:
                    raise ValueError("%s differs from frozen roots file (%r vs %r)" % (fld, raw.get(fld), r[fld]))
        except (ValueError, KeyError, TypeError) as ex:
            prob.append("root %s malformed: %s" % (r["root_id"], ex))
            continue
        n["cell"], n["stratum"], n["id"] = r["cell"], r["stratum"], r["root_id"]
        usable[r["root_id"]] = n
        nonformal += not n["formal"]
        bad = []
        if n["invalid"]:
            bad.append("invalid flag")
        if n["rejected"]:
            bad.append("rejected eval worlds")
        for a in ARMS:
            ar = n["arms"][a]
            if ar["sel_faults"] or ar["sel_incomplete"]:
                bad.append("arm %s selection fault/incomplete" % a)
            if any(x[3] == "fault" for x in ar["eval"]):
                bad.append("arm %s eval fault" % a)
        if bad:
            flagged += 1
            prob.append("root %s: %s" % (r["root_id"], ", ".join(bad)))
        if abs(n["cost_sel"] - sum(n["arms"][a]["sel_trans"] for a in ARMS)) > 0.5:
            res["warnings"].append("root %s: cost.selection_transitions != sum of arm selection transitions" % r["root_id"])
    if missing:
        prob.append("%d scheduled roots missing (e.g. %s)" % (len(missing), missing[:3]))
    res.update(missing=len(missing), nonformal_rows=nonformal, usable=len(usable), flagged=flagged)

    # finite analysis flag
    by_cell = {c: [u for u in usable.values() if u["cell"] == c] for c in CELLS}
    sizes = {c: len(v) for c, v in by_cell.items()}
    empty = [c for c in CELLS if sizes[c] == 0]
    if empty:
        prob.append("empty cells: %s" % empty)
    res["cell_sizes"] = sizes

    # discovery and J coverage (all arms; gate on E)
    def counts(pred):
        out = {"all": 0, "cast": 0, "target": 0, "cells": {c: 0 for c in CELLS}}
        for u in usable.values():
            if pred(u):
                out["all"] += 1
                out[u["stratum"]] += 1
                out["cells"][u["cell"]] += 1
        return out
    res["discovery"] = {a: counts(lambda u, a=a: u["arms"][a]["disc"]) for a in ARMS}
    res["j_coverage"] = {a: counts(lambda u, a=a: any(x[1] for x in u["arms"][a]["eval"])) for a in ARMS}
    for name in ("discovery", "j_coverage"):
        e = res[name]["E"]
        res[name]["E"]["gate"] = bool(e["all"] >= GATE_ALL and e["cast"] >= GATE_STAGE and e["target"] >= GATE_STAGE)

    # per-cell descriptive W/J world counts per arm
    res["cell_tables"] = {a: {c: {"W": sum(x[0] for u in by_cell[c] for x in u["arms"][a]["eval"]),
                                  "J": sum(x[1] for u in by_cell[c] for x in u["arms"][a]["eval"]),
                                  "worlds": len(by_cell[c]) * WORLDS} for c in CELLS} for a in ARMS}

    # contrasts
    vecs, events, ident = {}, {}, {}
    for ctl, ep in CONTRASTS:
        for ver in ("obs", "cons"):
            key = "E-%s/%s/%s" % (ctl, ep, ver)
            vecs[key] = {c: [contrast_events(u["arms"]["E"], u["arms"][ctl], ep, ver == "cons") for u in by_cell[c]]
                         for c in CELLS}
            events[key] = sum(sum(v) for v in vecs[key].values())
    boot = bootstrap(vecs, sizes, n_boot)
    active = [c for c in CELLS if sizes[c] > 0]
    full = {c: [list(range(sizes[c]))] for c in CELLS}
    contr = {}
    for ctl, ep in CONTRASTS:
        name = "E-%s/%s" % (ctl, ep)
        rec = {"control": ctl, "endpoint": ep}
        for ver in ("obs", "cons"):
            key = name + "/" + ver
            xs = boot[key]
            point = cell_weighted(vecs[key], {c: full[c][0] for c in CELLS}, active, sizes) if active else None
            v = {"events": events[key], "events_of": sum(sizes.values()) * WORLDS, "point_pp": point}
            if xs:
                v["ci95"] = [quantile7(xs, ORD_Q[0]), quantile7(xs, ORD_Q[1])]
                v["bonf_99375"] = [quantile7(xs, BONF_Q[0]), quantile7(xs, BONF_Q[1])]
                v["pass"] = bool(events[key] >= MIN_EVENTS and v["bonf_99375"][0] > 0)
            else:
                v["ci95"] = v["bonf_99375"] = "n/a (no usable roots)"
                v["pass"] = False
            rec[ver] = v
        rec["pass"] = rec["obs"]["pass"] and rec["cons"]["pass"]
        contr[name] = rec
    res["contrasts"] = contr
    # accounting identity (integer events): dW = dJ + d(wins without resolved suffix)
    for ctl in ("A", "D"):
        for ver in ("obs", "cons"):
            dW = events["E-%s/W/%s" % (ctl, ver)]
            dJ = events["E-%s/J/%s" % (ctl, ver)]
            ident["E-%s/%s" % (ctl, ver)] = {"dW": dW, "dJ": dJ, "d_wins_without_suffix": dW - dJ,
                                              }
            nos = 0
            for u in usable.values():
                for ee, cc in zip(u["arms"]["E"]["eval"], u["arms"][ctl]["eval"]):
                    ne, nc = (ee[0] and not ee[1]), (0 if (ver == "cons" and cc[2]) else (cc[0] and not cc[1]))
                    nos += int(ne) - int(nc)
            ident["E-%s/%s" % (ctl, ver)]["d_wins_without_suffix_direct"] = nos
            ident["E-%s/%s" % (ctl, ver)]["identity_ok"] = (dW == dJ + nos)
    res["accounting"] = ident

    # cost and unknowns
    cost = {}
    tot_wall = sum(u["root_wall"] for u in usable.values())
    for a in ARMS:
        us = list(usable.values())
        sel = sum(u["arms"][a]["sel_trans"] for u in us)
        inf = sum(u["arms"][a]["infer"] for u in us)
        evt = sum(x[4] for u in us for x in u["arms"][a]["eval"])
        wins = sum(x[0] for u in us for x in u["arms"][a]["eval"])
        ends = {e: sum(x[3] == e for u in us for x in u["arms"][a]["eval"]) for e in ENDS}
        cost[a] = {"selection_transitions": sel, "inference_calls": inf, "eval_transitions": evt,
                   "natural_wins": wins, "end_counts": ends,
                   "unknown": sum(x[2] for u in us for x in u["arms"][a]["eval"]),
                   "sel_cap_reached_roots": sum(u["arms"][a]["cap"] for u in us),
                   "worker_seconds_all_arms_per_win": (tot_wall / wins) if wins else "n/a (0 natural wins)",
                   "transitions_per_win": ((sel + evt) / wins) if wins else "n/a (0 natural wins)"}
    res["cost"] = {"arms": cost, "worker_seconds_total": tot_wall,
                   "reported_cost_selection_transitions": sum(u["cost_sel"] for u in usable.values()),
                   "reported_cost_eval_transitions": sum(u["cost_eval"] for u in usable.values())}
    res["valid"] = not prob
    return res


def disposition(models_res, nonformal_label):
    if any(not m["valid"] for m in models_res.values()):
        d, why = DISPOSITIONS[0], "validity problems: " + "; ".join(
            "%s: %d" % (k, len(m["problems"])) for k, m in models_res.items() if not m["valid"])
    elif any(not m["discovery"]["E"]["gate"] for m in models_res.values()):
        d, why = DISPOSITIONS[1], "E selection discovery below gate in: " + ", ".join(
            k for k, m in models_res.items() if not m["discovery"]["E"]["gate"])
    else:
        fails = [k for k, m in models_res.items() if not m["j_coverage"]["E"]["gate"]]
        fails += ["%s %s" % (k, n) for k, m in models_res.items() for n, c in m["contrasts"].items() if not c["pass"]]
        if fails:
            d, why = DISPOSITIONS[2], "failed: " + ", ".join("J coverage " + f if f in models_res else "contrast " + f for f in fails)
        else:
            d, why = DISPOSITIONS[3], "both models meet discovery and J coverage; all eight contrasts pass observed and conservative"
    return {"disposition": d, "reason": why, "label": nonformal_label}


# ---------------------------------------------------------------- driver
def run_analysis(roots, row_sets, n_boot=N_BOOT, allow_nonformal=False):
    by_key, errors, warnings, bad_total, dups = ingest(row_sets)
    models_res = {}
    for m in MODELS:
        models_res[m] = analyze_model(m, [r for r in roots if r.get("model") == m], by_key, errors, dups, n_boot)
    others = sorted({str(r.get("model")) for r in roots} - set(MODELS))
    gl = []
    if bad_total:
        gl.append("%d unparseable/non-finite JSON lines" % bad_total)
    if others:
        gl.append("roots file has unknown models %s" % others)
    for k in {k[0] for k in by_key} - set(MODELS):
        gl.append("rows for unknown model %r" % (k,))
    for m in models_res.values():
        if gl:
            m["valid"] = False
            m["problems"] = gl + m["problems"]
    nonformal_rows = sum(m["nonformal_rows"] for m in models_res.values())
    label = None
    if nonformal_rows:
        label = "ENGINEERING, NOT A RESULT" if allow_nonformal else "REJECTED: non-formal rows present"
    out = {"analysis": "stage4a-frozen-v1", "bootstrap": {"draws": n_boot, "frozen_draws": N_BOOT,
           "seed": BOOT_SEED, "frozen": n_boot == N_BOOT,
           "procedure": "per model random.Random(seed); per draw, per cell in order %s, n_cell x rng.randrange(n_cell)" % CELLS},
           "nonformal_rows": nonformal_rows, "warnings": warnings, "global_problems": gl,
           "models": models_res}
    out.update(disposition(models_res, label))
    if label and not allow_nonformal:
        out["disposition"] = "REJECTED (non-formal rows; rerun with --allow-nonformal for engineering output)"
    if n_boot != N_BOOT:
        out["label"] = (label + "; " if label else "") + "NON-FROZEN BOOTSTRAP DRAWS"
    return out


def f(x, nd=2):
    if x is None:
        return "n/a"
    if isinstance(x, float):
        return ("%." + str(nd) + "f") % x
    return str(x)


def render_text(a):
    L = []
    if a.get("label"):
        L.append("*** %s ***" % a["label"])
    L += ["Stage 4a analysis (bootstrap %d draws, seed %d)" % (a["bootstrap"]["draws"], a["bootstrap"]["seed"]),
          "DISPOSITION: %s" % a["disposition"], "Rule: %s" % a["reason"], ""]
    for w in a["warnings"][:10] + a["global_problems"]:
        L.append("warning: " + w)
    for mname, m in a["models"].items():
        L += ["=" * 70, "MODEL %s  valid=%s  usable roots=%d missing=%d error rows=%d" %
              (mname, m["valid"], m["usable"], m["missing"], m["error_rows"])]
        if not m["valid"]:
            L.append("INVALID/INCOMPLETE; numbers below are DESCRIPTIVE only. Problems (%d):" % len(m["problems"]))
            L += ["  - " + p for p in m["problems"][:20]]
        for name, key in (("Selection discovery", "discovery"), ("Independent J root coverage", "j_coverage")):
            L.append("%s (roots; gate E >=50/100 and >=25/50 per stage):" % name)
            for ar in ARMS:
                d = m[key][ar]
                L.append("  %s: %d/100  cast %d/50  target %d/50  cells %s%s" % (
                    ar, d["all"], d["cast"], d["target"], [d["cells"][c] for c in CELLS],
                    ("  GATE " + ("PASS" if d["gate"] else "FAIL")) if ar == "E" else "  (descriptive)"))
        L.append("Contrasts (events out of %d worlds; pp; ordinary 95%% CI; Bonferroni 99.375%% CI):" % (
            sum(m["cell_sizes"].values()) * WORLDS))
        for name, c in m["contrasts"].items():
            for ver in ("obs", "cons"):
                v = c[ver]
                L.append("  %-8s %-4s events=%5d  %7s pp  CI95 %s  BONF %s  %s" % (
                    name, ver, v["events"], f(v["point_pp"]),
                    [f(x) for x in v["ci95"]] if isinstance(v["ci95"], list) else v["ci95"],
                    [f(x) for x in v["bonf_99375"]] if isinstance(v["bonf_99375"], list) else v["bonf_99375"],
                    "pass" if v["pass"] else "fail"))
            L.append("  %-8s BOTH %s" % (name, "PASS" if c["pass"] else "FAIL"))
        L.append("Accounting (dW = dJ + d wins without resolved suffix; descriptive):")
        for k, i in m["accounting"].items():
            L.append("  %s: dW=%d dJ=%d dNoSuffix=%d identity_ok=%s" % (k, i["dW"], i["dJ"], i["d_wins_without_suffix_direct"], i["identity_ok"]))
        L.append("Per-cell world counts W/J (of %s worlds/cell):" % [m["cell_tables"]["E"][c]["worlds"] for c in CELLS])
        for ar in ARMS:
            L.append("  %s: " % ar + "  ".join("%s W=%d J=%d" % (c, m["cell_tables"][ar][c]["W"], m["cell_tables"][ar][c]["J"]) for c in CELLS))
        L.append("Cost (actual) and outcomes per arm:")
        for ar in ARMS:
            c = m["cost"]["arms"][ar]
            L.append("  %s: sel_trans=%s infer=%s eval_trans=%s wins=%s unknown=%s ends=%s cap_roots=%s trans/win=%s wsec(all arms)/win=%s" % (
                ar, c["selection_transitions"], c["inference_calls"], c["eval_transitions"], c["natural_wins"],
                c["unknown"], c["end_counts"], c["sel_cap_reached_roots"], f(c["transitions_per_win"], 1),
                f(c["worker_seconds_all_arms_per_win"], 1)))
        L.append("  total worker-seconds (sum root_wall): %s" % f(m["cost"]["worker_seconds_total"], 1))
        for w in m["warnings"][:5]:
            L.append("  warning: " + w)
        L.append("")
    return "\n".join(L) + "\n"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--roots", required=True)
    ap.add_argument("--rows", nargs="*", default=[])
    ap.add_argument("--out", required=True)
    ap.add_argument("--allow-nonformal", action="store_true")
    ap.add_argument("--draws", type=int, default=N_BOOT, help="TEST ONLY; frozen value is %d" % N_BOOT)
    args = ap.parse_args(argv)
    try:
        roots, bad = load_jsonl(args.roots)
        row_sets = [(p,) + load_jsonl(p) for p in args.rows]
    except OSError as ex:
        print("error: %s" % ex, file=sys.stderr)
        return 2
    if bad:
        print("error: %d unparseable lines in roots file" % bad, file=sys.stderr)
        return 2
    result = run_analysis(roots, row_sets, args.draws, args.allow_nonformal)
    os.makedirs(args.out, exist_ok=True)
    with open(os.path.join(args.out, "analysis.json"), "w", encoding="utf-8") as fh:
        json.dump(result, fh, indent=1, allow_nan=False)
    with open(os.path.join(args.out, "analysis.txt"), "w", encoding="utf-8") as fh:
        fh.write(render_text(result))
    print("%s: %s" % (result["disposition"], result["reason"]))
    if result["nonformal_rows"] and not args.allow_nonformal:
        print("error: non-formal rows present; pass --allow-nonformal for ENGINEERING output", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
