"""W4 Step A, A1 (inclusion decisions, nominates P) and A3 (value head against a material ruler, nominates T).
Collab LANES/w4-combat-20261030/DESIGN.md section 4. Measurement only: the probes and the ruler are rulers and
never become a target, reward, prior or feature.

usage: a1a3.py --scores DIR --split split.json --out RESULT.json

Inputs are the `s4a-w4 score` shards (`score-*.jsonl` with `.f32` hidden files) and the game split.
Implementation choices the design leaves open (recorded in the result): every probe and ruler input is
standardized by its fit rows' (weighted) mean and SD; grouped folds deal groups in SHA-256 order; AUC is
Mann-Whitney with ties half; A1c's target is "the current attacker already has a blocker chosen earlier in
this declaration".
"""
from __future__ import annotations

import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path

import numpy as np
from common import (ACTION_DIM, HIDDEN, OBJECT_DIM, auc, bootstrap_groups, choose_c, deck, fit_logistic,
                    load_scores, log_loss, predict, standardize, template_half)

MIN_MINORITY = 20
# Standard normal quantiles for a two-sided 5% test at 80% power.
Z_975, Z_80 = 1.959963984540054, 0.8416212335729143
A3_EFFECT = 0.005
TURN_BUCKETS = ((1, 4, "1-4"), (5, 8, "5-8"), (9, 10 ** 9, "9+"))


def probe(x, y, fit, read, groups):
    """L2 logistic probe: C by grouped CV on the fit rows (unweighted), AUC on the read rows."""
    y = np.asarray(y, float)
    w = np.ones(fit.sum())
    c, losses, _ = choose_c(x[fit], y[fit], w, np.asarray(groups)[fit], w)
    xf, xr = standardize(x[fit], w, x[read])
    model = fit_logistic(xf, y[fit], w, c)
    yr = y[read]
    minority = int(min(yr.sum(), len(yr) - yr.sum()))
    value = auc(yr, predict(model, xr))
    ok = minority >= MIN_MINORITY and value is not None
    return {"auc": value, "C": c, "cv_log_loss": losses, "fit_rows": int(fit.sum()), "read_rows": int(read.sum()),
            "read_minority": minority, "valid": ok}


def a1(sc, split):
    blocks = [d for d in sc.decisions if "block" in d]
    rows = []
    for d in blocks:
        b = d["block"]
        # Facts are null unless the rules simulation reached the pair and finished combat.
        logits = np.asarray(d["logits"], float)
        p = np.exp(logits - logits.max())
        p /= p.sum()
        f = b["facts"]
        rows.append({"d": d, "b": b, "half": split[d["id"]], "p_include": float(p[b["include_row"]]),
                     "blocker_dies": f["blocker_dies"], "attacker_dies": f["attacker_dies"],
                     "lethal": f["lethal_on_board"]})
    # A1a: include probability against the facts.
    table = defaultdict(list)
    for r in rows:
        if r["blocker_dies"] is not None:
            table[(r["blocker_dies"], r["attacker_dies"], r["lethal"])].append(r["p_include"])
    a1a = [{"blocker_dies": k[0], "attacker_dies": k[1], "lethal_on_board": k[2], "pairs": len(v),
            "mean_p_include": float(np.mean(v)), "median_p_include": float(np.median(v))}
           for k, v in sorted(table.items(), key=lambda kv: str(kv[0]))]
    unreached = sum(r["blocker_dies"] is None for r in rows)

    # Include-flag probe: both rows of every pair, split by template over all games.
    flag_x = {"b512": [], "random": [], "raw_action": []}
    flag_y, flag_t, flag_half = [], [], []
    for r in rows:
        b = r["b"]
        key_half = template_half(b["attacker_card"], b["blocker_card"], 0)
        tkey = f"a{b['attacker_card']}:b{b['blocker_card']}:f0"
        for row in (0, 1):
            flag_x["b512"].append(sc.vec(r["d"], r["d"]["scorer_hidden"] + row * HIDDEN, HIDDEN))
            flag_x["random"].append(sc.vec(r["d"], b["random_scorer_hidden"] + row * HIDDEN, HIDDEN))
            flag_x["raw_action"].append(sc.vec(r["d"], b["action_features"] + row * ACTION_DIM, ACTION_DIM))
            flag_y.append(row == b["include_row"])
            flag_t.append(tkey)
            flag_half.append(key_half)
    fh = np.asarray(flag_half)
    flag = {k: probe(np.asarray(v), flag_y, fh == "fit", fh == "read", flag_t) for k, v in flag_x.items()}

    # Stakes probes at the include row, split by game.
    reached = [r for r in rows if r["blocker_dies"] is not None]
    gh = np.asarray([r["half"] for r in reached])
    games = [r["d"]["id"] for r in reached]
    inc = lambda r, key: sc.vec(r["d"], (r["d"][key] if key == "scorer_hidden" else r["b"][key])
                                 + r["b"]["include_row"] * HIDDEN, HIDDEN)
    xs = {"b512": np.asarray([inc(r, "scorer_hidden") for r in reached]),
          "random": np.asarray([inc(r, "random_scorer_hidden") for r in reached])}
    ref_counts = Counter(len(r["b"]["ref_nodes"]) for r in reached)
    two = max(ref_counts, key=ref_counts.get) if ref_counts else 0
    xs["raw_object"] = np.asarray([
        np.pad(sc.vec(r["d"], r["b"]["ref_object_features"], OBJECT_DIM * len(r["b"]["ref_nodes"])),
               (0, max(0, OBJECT_DIM * two - OBJECT_DIM * len(r["b"]["ref_nodes"]))))[:OBJECT_DIM * two]
        for r in reached])
    stakes = {}
    for target in ("blocker_dies", "attacker_dies"):
        y = [bool(r[target]) for r in reached]
        stakes[target] = {k: probe(x, y, gh == "fit", gh == "read", games) for k, x in xs.items()}

    criterion = [flag["b512"], stakes["blocker_dies"]["b512"], stakes["attacker_dies"]["b512"]]
    valid = [p for p in criterion if p["valid"]]
    if any(p["auc"] < 0.70 for p in valid):
        verdict = "nominated"
    elif len(valid) == 3 and all(p["auc"] >= 0.85 for p in valid):
        verdict = "not nominated"
    else:
        verdict = "inconclusive"

    # A1c (descriptive): state hidden at second-and-later pair decisions.
    later = [r for r in rows if r["b"]["earlier_pairs"]]
    a1c = None
    if later:
        y = [any(e[0] == r["b"]["attacker"] and e[2] for e in r["b"]["earlier_pairs"]) for r in later]
        lh = np.asarray([r["half"] for r in later])
        x = np.asarray([sc.vec(r["d"], r["b"]["state_hidden"], HIDDEN) for r in later])
        xr = np.asarray([sc.vec(r["d"], r["b"]["random_state_hidden"], HIDDEN) for r in later])
        g = [r["d"]["id"] for r in later]
        a1c = {"target": "current attacker already has a blocker chosen earlier in this declaration",
               "rows": len(later), "positives": int(sum(y)),
               "b512": probe(x, y, lh == "fit", lh == "read", g), "random": probe(xr, y, lh == "fit", lh == "read", g)}
    return {"pairs": len(rows), "pairs_fact_unreached": unreached, "ref_counts": dict(ref_counts), "a1a": a1a,
            "include_flag": flag, "stakes": stakes, "criterion_aucs": [p["auc"] for p in criterion],
            "classification": verdict, "a1c": a1c}


def a3(sc, split):
    feats = []
    for d in sc.decisions:
        g = sc.games[d["id"]]
        if g["winner"] is None:
            continue
        m = d["material"]
        x = [m[s][k] for s in ("focal", "opponent")
             for k in ("life", "creatures", "power", "toughness", "untapped_power", "hand")] + [m["turn"]]
        feats.append((d["id"], d["value"], x, 1.0 if g["winner"] == g["focal"] else 0.0, m["turn"],
                      deck(g["deck_ids"][g["focal"]])))
    draws = sorted({gid for gid, g in sc.games.items() if g["winner"] is None})
    ids = np.asarray([f[0] for f in feats])
    v = np.asarray([[f[1]] for f in feats], float)
    x = np.asarray([f[2] for f in feats], float)
    y = np.asarray([f[3] for f in feats])
    per_game = Counter(ids)
    w = np.asarray([1.0 / per_game[i] for i in ids])
    fit = np.asarray([split[i] == "fit" for i in ids])
    read = ~fit
    v1 = np.hstack([v, x])
    c0, l0, oof0 = choose_c(v[fit], y[fit], w[fit], ids[fit], w[fit])
    c1, l1, oof1 = choose_c(v1[fit], y[fit], w[fit], ids[fit], w[fit])
    v0f, v0r = standardize(v[fit], w[fit], v[read])
    v1f, v1r = standardize(v1[fit], w[fit], v1[read])
    m0, m1 = fit_logistic(v0f, y[fit], w[fit], c0), fit_logistic(v1f, y[fit], w[fit], c1)

    def per_game_diff(idx_ids, yy, p0, p1):
        d = log_loss(yy, p0) - log_loss(yy, p1)
        out = defaultdict(list)
        for gid, val in zip(idx_ids, d):
            out[gid].append(val)
        return {g: float(np.mean(vs)) for g, vs in out.items()}

    # Power on the fit half: per-game SD of the out-of-fold difference.
    fit_diff = per_game_diff(ids[fit], y[fit], oof0, oof1)
    sd = float(np.std(list(fit_diff.values()), ddof=1))
    read_diff = per_game_diff(ids[read], y[read], predict(m0, v0r), predict(m1, v1r))
    n_read = len(read_diff)
    mde = float((Z_975 + Z_80) * sd / np.sqrt(n_read))
    games = sorted(read_diff)
    vals = np.asarray([read_diff[g] for g in games])
    point = float(vals.mean())
    lo, hi, _ = bootstrap_groups(games, lambda idx: float(vals[idx].mean()), 20261032)
    if lo > 0 and point >= A3_EFFECT:
        verdict = "nominated"
    elif mde <= A3_EFFECT and hi < A3_EFFECT:
        verdict = "not nominated"
    else:
        verdict = "inconclusive"

    # Cells (descriptive): per deck and turn bucket on the read half.
    p0r, p1r = predict(m0, v0r), predict(m1, v1r)
    cells = {}
    rid, ry = ids[read], y[read]
    meta = [(f[4], f[5]) for f, r in zip(feats, read) if r]
    for name, key in (("deck", lambda t, dk: dk),
                      ("turn", lambda t, dk: next(b[2] for b in TURN_BUCKETS if b[0] <= t <= b[1]))):
        groups = defaultdict(list)
        for k, (t, dk) in enumerate(meta):
            groups[key(t, dk)].append(k)
        for cell, idx in sorted(groups.items()):
            idx = np.asarray(idx)
            wins = {g: ry[i] for g, i in zip(rid[idx], idx)}
            minority = min(sum(wins.values()), len(wins) - sum(wins.values()))
            if minority < 8:
                cells[f"{name}:{cell}"] = {"pooled": True, "games": len(wins), "minority": int(minority)}
                continue
            dd = per_game_diff(rid[idx], ry[idx], p0r[idx], p1r[idx])
            cells[f"{name}:{cell}"] = {"games": len(dd), "minority": int(minority),
                                       "mean_diff": float(np.mean(list(dd.values())))}
    return {"states": len(feats), "games_fit": len(fit_diff), "games_read": n_read, "draws_excluded": len(draws),
            "C_p0": c0, "C_p1": c1, "cv_log_loss_p0": l0, "cv_log_loss_p1": l1,
            "fit_per_game_sd": sd, "mde": mde, "point": point, "ci95": [lo, hi], "classification": verdict,
            "coefficients_p1": {"intercept": float(m1[0]), "beta": m1[1].tolist()}, "cells": cells}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--scores", type=Path, required=True)
    ap.add_argument("--split", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    sc = load_scores(a.scores)
    split = json.loads(a.split.read_text())
    result = {"games_scored": len(sc.games), "score_errors": sc.errors, "focal_decisions": len(sc.decisions),
              "A1": a1(sc, split), "A3": a3(sc, split)}
    a.out.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
    print(json.dumps({"A1": result["A1"]["classification"], "A1_aucs": result["A1"]["criterion_aucs"],
                      "A3": result["A3"]["classification"], "A3_point": result["A3"]["point"],
                      "A3_ci": result["A3"]["ci95"], "A3_mde": result["A3"]["mde"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
