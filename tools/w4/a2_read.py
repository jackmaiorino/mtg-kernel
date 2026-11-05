"""W4 Step A, A2 read: value head against terminal returns at block choices (nominates V).
Collab LANES/w4-combat-20261030/DESIGN.md section 4, A2. Measurement only.

usage: a2_read.py --scores DIR --split split.json --positions DIR --out RESULT.json

`--positions` holds the `s4a-w4 a2` shards (`a2-*.jsonl`). The A2 calibration is an affine logistic map from
the value-head scalar to the expected terminal score (win 1, draw 0.5, loss 0), fit on the fit half's focal
states by fractional-outcome logistic regression with equal-game weights (C by the same grouped
cross-validation as A1b and A3, on the standardized scalar). A capped or halted continuation or playout scores
0.5; an engine halt withholds the classification. The world-variance share (descriptive) is, over the 50 positions, the mean between-world variance of the
gap (corrected by the mean within-world SE squared, floored at 0) over the variance of the true-world gaps.
"""
from __future__ import annotations

import argparse
import glob
import json
from collections import Counter
from pathlib import Path

import numpy as np
from common import require_complete, bootstrap_groups, choose_c, fit_logistic, load_scores, spearman, standardize

SIGN_SUBSET = 20


def calibration(sc, split):
    rows = [(d["id"], d["value"]) for d in sc.decisions if split[d["id"]] == "fit"]
    ids = np.asarray([r[0] for r in rows])
    v = np.asarray([[r[1]] for r in rows], float)
    score = {g: (0.5 if r["winner"] is None else float(r["winner"] == r["focal"])) for g, r in sc.games.items()}
    y = np.asarray([score[i] for i in ids])
    n = Counter(ids)
    w = np.asarray([1.0 / n[i] for i in ids])
    c, losses, _ = choose_c(v, y, w, ids, w)
    (vf,) = standardize(v, w)
    w1 = w / w.sum()
    mu = float((v[:, 0] * w1).sum())
    sd = float(np.sqrt((((v[:, 0] - mu) ** 2) * w1).sum()))
    sd = sd if sd > 0 else 1.0  # the scale standardize() used
    a, b = fit_logistic(vf, y, w, c)
    # In raw units: p = sigmoid(a + b * (v - mu) / sd).
    return {"C": c, "cv_log_loss": losses, "intercept": float(a), "slope": float(b[0]), "mu": mu, "sd": sd,
            "states": len(rows)}


def score_of(ending, cal):
    if "value" in ending:
        z = cal["intercept"] + cal["slope"] * (ending["value"] - cal["mu"]) / cal["sd"]
        return 1.0 / (1.0 + np.exp(-z))
    if "terminal" in ending:
        return float(ending["terminal"])
    return 0.5


def paired(inc, exc):
    d = np.asarray(inc) - np.asarray(exc)
    return float(d.mean()), float(d.std(ddof=1) / np.sqrt(len(d)))


def signal_share(gaps, ses):
    var = float(np.var(gaps, ddof=1))
    if var == 0:
        return 0.0
    return float(min(1.0, max(0.0, (var - float(np.mean(np.square(ses)))) / var)))


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--scores", type=Path, required=True)
    ap.add_argument("--split", type=Path, required=True)
    ap.add_argument("--positions", type=Path, required=True)
    ap.add_argument("--selected", type=Path, required=True, help="the drawn positions.jsonl")
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    sc = load_scores(a.scores)
    split = json.loads(a.split.read_text())
    require_complete(sc, split)
    cal = calibration(sc, split)
    rows, errors = [], []
    for path in sorted(glob.glob(str(a.positions / "a2-*.jsonl"))):
        for line in open(path, encoding="utf-8"):
            r = json.loads(line)
            (rows if r["kind"] == "position" else errors).append(r)
    # The analysis population is every drawn position, each once and complete.
    want = [json.loads(l) for l in open(a.selected, encoding="utf-8") if l.strip()]
    got = Counter((r["id"], r["index"]) for r in rows)
    def complete(r, worlds):
        ch = r["choices"]
        return all(len(ch[s]["value"]) == 16 and len(ch[s]["playout"]) == 256
                   and (not worlds or (len(ch[s]["worlds"]) == 4
                                       and all(len(w["playouts"]) == 64 for w in ch[s]["worlds"])))
                   for s in ("include", "exclude"))
    by_key = {(r["id"], r["index"]): r for r in rows}
    keys = [(p["id"], p["index"]) for p in want]
    if len(want) != 300 or len(set(keys)) != 300:
        raise SystemExit(f"the A2 selection must be 300 distinct positions, not {len(want)} ({len(set(keys))} distinct)")
    missing = [p for p in want if got[(p["id"], p["index"])] != 1
               or not complete(by_key[(p["id"], p["index"])], p["worlds"])]
    extra = sorted(set(got) - {(p["id"], p["index"]) for p in want})
    halts = sum(1 for r in rows for side in ("include", "exclude")
                for e in (r["choices"][side]["value"] + r["choices"][side]["playout"]
                          + [x for w in r["choices"][side]["worlds"] for x in w["playouts"]])
                if "halt" in e)
    if missing or extra or errors or halts:
        result = {"classification": "withheld",
                  "reason": "engine halts" if halts and not (missing or extra or errors) else "incomplete A2 population",
                  "halted_branches": halts, "missing_or_incomplete": missing, "extra": extra,
                  "position_errors": errors}
        a.out.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
        print(json.dumps({k: (len(v) if isinstance(v, list) else v) for k, v in result.items()}))
        return 1
    rows.sort(key=lambda r: (r["id"], r["index"]))
    endings = Counter()
    out = []
    for r in rows:
        ch = r["choices"]
        for side in ("include", "exclude"):
            for e in ch[side]["value"] + ch[side]["playout"]:
                endings[next(iter(e))] += 1
        vi = [score_of(e, cal) for e in ch["include"]["value"]]
        ve = [score_of(e, cal) for e in ch["exclude"]["value"]]
        pi = [score_of(e, cal) for e in ch["include"]["playout"]]
        pe = [score_of(e, cal) for e in ch["exclude"]["playout"]]
        vg, vse = paired(vi, ve)
        pg, pse = paired(pi, pe)
        _, pse16 = paired(pi[:16], pe[:16])
        taken = "include" if r["selected"] == r["include_row"] else "exclude"
        post = float(np.mean(vi if taken == "include" else ve))
        pre = score_of({"value": r["value_pre"]}, cal)
        worlds = None
        if ch["include"]["worlds"]:
            wg = [paired([score_of(e, cal) for e in wi["playouts"]], [score_of(e, cal) for e in we["playouts"]])
                  for wi, we in zip(ch["include"]["worlds"], ch["exclude"]["worlds"])]
            worlds = {"gaps": [g for g, _ in wg], "ses": [s for _, s in wg]}
        out.append({"id": r["id"], "index": r["index"], "value_gap": vg, "value_se": vse, "playout_gap": pg,
                    "playout_se": pse, "playout_se16": pse16, "pre": pre, "post_taken": post, "worlds": worlds})
    vg = np.asarray([o["value_gap"] for o in out])
    pg = np.asarray([o["playout_gap"] for o in out])
    vse = np.asarray([o["value_se"] for o in out])
    pse = np.asarray([o["playout_se"] for o in out])
    games = [o["id"] for o in out]
    shares = {"playout": signal_share(pg, pse), "value": signal_share(vg, vse)}
    constant = bool(np.all(vg == vg[0]) or np.all(pg == pg[0]))
    rho = None if constant else spearman(vg, pg)
    lo, hi, reps = (None, None, 0) if constant else bootstrap_groups(
        games, lambda idx: (None if np.all(vg[idx] == vg[idx][0]) or np.all(pg[idx] == pg[idx][0])
                            else spearman(vg[idx], pg[idx])), 20261031)
    undefined_z = int((pse == 0).sum())
    with np.errstate(divide="ignore", invalid="ignore"):
        z = np.where(pse > 0, pg / pse, np.nan)
    strong = np.abs(z) > 2
    agree = float(np.mean(np.sign(vg[strong]) == np.sign(pg[strong]))) if strong.any() else None
    median_vse = float(np.median(vse))
    median_pse16 = float(np.median([o["playout_se16"] for o in out]))
    se_ratio = median_vse / median_pse16 if median_pse16 > 0 else None
    crit = {"i": rho is not None and rho >= 0.40 and lo is not None and lo > 0.20,
            "ii": agree is not None and agree >= 0.75, "ii_positions": int(strong.sum()),
            "iii": median_vse <= 0.5 * median_pse16}
    if constant:
        verdict = "inconclusive"
    elif min(shares.values()) < 0.5:
        verdict = "inconclusive"
    elif crit["i"] and crit["iii"] and crit["ii"] and crit["ii_positions"] >= SIGN_SUBSET:
        verdict = "nominated"
    elif hi is not None and hi < 0.20:
        verdict = "not nominated"
    else:
        verdict = "inconclusive"
    world = [o for o in out if o["worlds"]]
    world_share = None
    if len(world) > 1:
        between = [max(0.0, float(np.var(o["worlds"]["gaps"] + [o["playout_gap"]], ddof=1))
                       - float(np.mean(np.square(o["worlds"]["ses"] + [o["playout_se"]])))) for o in world]
        total = float(np.var([o["playout_gap"] for o in world], ddof=1))
        world_share = float(np.mean(between) / total) if total > 0 else None
    bell = (np.corrcoef([o["pre"] for o in out], [o["post_taken"] for o in out])[0, 1] if len(out) > 2 else None)
    result = {"calibration": cal, "positions": len(out), "position_errors": errors, "endings": dict(endings),
              "signal_share": shares, "world_variance_share": world_share, "spearman": rho,
              "spearman_ci95": [lo, hi], "bootstrap_replicates": reps, "sign_agreement": agree,
              "undefined_z": undefined_z, "median_value_se": median_vse,
              "median_playout_se16": median_pse16, "se_ratio": se_ratio,
              "criteria": crit, "classification": verdict,
              "bellman": {"corr_pre_post": None if bell is None else float(bell),
                          "mean_post_minus_pre": float(np.mean([o["post_taken"] - o["pre"] for o in out]))},
              "per_position": out}
    a.out.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
    print(json.dumps({k: result[k] for k in ("positions", "signal_share", "spearman", "spearman_ci95",
                                              "sign_agreement", "se_ratio", "criteria", "classification")}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
