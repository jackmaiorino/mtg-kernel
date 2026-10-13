"""Synthetic calibration of the allocator v2 update (collab LANES/allocator-redesign-20261012,
DESIGN.md stage 1). Synthetic examples never enter Magic training.

Collection uses the Python mirror of the Rust consultation (``allocator2.consult_record``:
dynamic base q, clipping, coalescing, 0.8 pi + 0.2 q behavior, cumulative pick) and the real
update entry point (``allocator2.update``: game checks, frozen advantages, PPO epochs, Adam,
gradient cap, KL stop), with 18 games per parent update.

Tasks (each game: T consultations with the remaining allowance falling by the executed
amounts from 2,048,000; context c in {0, 1, 2} uniform per consultation, encoded one-hot in
three shared-feature slots; every other feature is uniform noise; terminal win probability
known):
- contextual: c0 prefers skip, c1 128k, c2 512k; p(win) = base + effect * (fraction of the
  game's consultations whose executed amount contains the preferred nominal action).
- zero signal: p(win) = 0.5 whatever is chosen.
- perfect: one consultation per game and a deterministic reward (win exactly when the
  preferred amount executes): the reachable-movement ceiling of the optimizer.
Normalization is frozen, as in development, from 36 q-arm synthetic games collected before
initialization.
Readouts on a fixed probe set (each context at full and at half allowance): total variation
of pi and of mu from q, and the probability of the preferred amount per context, after each
update. Plus a finite-difference check of the minibatch gradient through coalescing and the
mixture, at parameters where ratios differ from 1.

CLI: allocator2_calibrate.py --out DIR [--quick]
"""
from __future__ import annotations

import argparse
import copy
import importlib.util
import json
import math
from pathlib import Path

import numpy as np

_spec = importlib.util.spec_from_file_location("allocator2", Path(__file__).with_name("allocator2.py"))
A = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(A)

PARENT = {"lineage": "r1", "weights_sha256": "synthetic", "feature_contract_digest": "synthetic",
          "feature_encoding_digest": "synthetic"}
CONTEXT_SLOTS = (12, 13, 14)
PREFERRED = (0, 2, 3)  # nominal index preferred in context 0, 1, 2
T_DECISIONS = 10
LRS = (3e-4, 1e-3, 3e-3, 1e-2)


def s_row(rng, context: int, remaining: int) -> list:
    s = rng.uniform(0.0, 1.0, size=A.S_FEATURES.__len__())
    s[0] = 1.0
    s[1] = remaining / A.CAP
    s[2] = remaining / A.CAP
    s[3] = 1.0 - remaining / A.CAP
    for j, slot in enumerate(CONTEXT_SLOTS):
        s[slot] = 1.0 if j == context else 0.0
    return s.tolist()


def rep_row(rng, arm: str) -> list:
    return rng.uniform(-1.0, 1.0, size=3 * A.REP).tolist() if arm == "H" else []


def play_game(rng, w, arm, task, effect, base, gid, weights_bytes):
    remaining, records, correct = A.CAP, [], 0
    for i in range(1 if task == "perfect" else T_DECISIONS):
        if remaining <= 0:
            break
        c = int(rng.integers(3))
        rec = A.consult_record(w, s_row(rng, c, remaining), rep_row(rng, arm), remaining, A.CAP,
                               float(rng.uniform()), i)
        correct += int(PREFERRED[c] in rec["members"][rec["chosen"]])
        remaining -= rec["amount"]
        records.append(rec)
    n = max(len(records), 1)
    p_win = 0.5 if task == "zero" else base + effect * correct / n
    if task == "perfect":
        p_win = float(correct == 1)
    end = "win" if rng.uniform() < p_win else "loss"
    return A.synthetic_row(gid, arm, "r1", weights_bytes, records, end), correct / n


def probe_set(arm, seed=12345):
    rng = np.random.default_rng(seed)
    probes = []
    for c in range(3):
        for remaining in (A.CAP, A.CAP // 2):
            for _ in range(20):
                probes.append((c, remaining, s_row(rng, c, remaining), rep_row(rng, arm)))
    return probes


def readout(w, probes, preferred=PREFERRED):
    by = {c: {"p_preferred_pi": [], "p_preferred_mu": [], "p_preferred_q": []} for c in range(3)}
    tv_pi, tv_mu = [], []
    for c, remaining, s, rep in probes:
        q = A.base_q(remaining, A.CAP)
        cs = A.choices(remaining)
        x = A.decision_x(w["arm"], {"s": s, "rep": rep})
        _, _, actor, _ = A.forward(w["params"], w["norm"], x)
        pi_c = A.coalesce(A.policy(q, actor), cs)
        q_c = A.coalesce(q, cs)
        mu_c = (1 - A.EPSILON) * pi_c + A.EPSILON * q_c
        tv_pi.append(0.5 * float(np.abs(pi_c - q_c).sum()))
        tv_mu.append(0.5 * float(np.abs(mu_c - q_c).sum()))
        idx = next(i for i, (_, m) in enumerate(cs) if preferred[c] in m)
        by[c]["p_preferred_pi"].append(float(pi_c[idx]))
        by[c]["p_preferred_mu"].append(float(mu_c[idx]))
        by[c]["p_preferred_q"].append(float(q_c[idx]))
    directed = float(np.mean([np.mean(d["p_preferred_pi"]) - np.mean(d["p_preferred_q"]) for d in by.values()]))
    return {"tv_pi_q_mean": float(np.mean(tv_pi)), "tv_pi_q_max": float(np.max(tv_pi)),
            "directed_gain": directed,
            "tv_mu_q_mean": float(np.mean(tv_mu)),
            "contexts": {str(c): {k: float(np.mean(v)) for k, v in d.items()} for c, d in by.items()}}


def frozen_norm(arm, rng, task):
    """Normalization frozen from 36 q-arm synthetic games (the development protocol)."""
    q_bytes = A.init_weights(arm, PARENT, None, "norm-source")
    rows = [play_game(rng, None, arm, task, 0.0, 0.5, f"q{g}", q_bytes)[0] for g in range(36)]
    for r in rows:
        r["lineage"] = "r1"
    return A.freeze_norm(rows, "r1", arm)


def run(arm, task, effect, base, updates, lr, seed):
    rng = np.random.default_rng(seed)
    wbytes = A.init_weights(arm, PARENT, frozen_norm(arm, rng, task), f"calibration-{seed}")
    adam = None
    probes = probe_set(arm)
    curve = [dict(readout(A.read_weights(wbytes), probes), update=0)]
    stats = []
    for k in range(updates):
        w = A.read_weights(wbytes)
        rows, corr = [], []
        for g in range(A.GAMES_PER_PARENT):
            row, c = play_game(rng, w, arm, task, effect, base, f"u{k}g{g}", wbytes)
            rows.append(row)
            corr.append(c)
        wbytes, adam, rec = A.update(wbytes, adam, rows, k, "r1", lr=lr)
        curve.append(dict(readout(A.read_weights(wbytes), probes), update=k + 1))
        stats.append({"update": k, "mean_return": rec["mean_return"], "epochs": rec["epochs_run"],
                      "kl_last": rec["epochs"][-1]["kl_after"], "correct_rate": float(np.mean(corr)),
                      "grad_norm_max": max(s["grad_norm"] for e in rec["epochs"] for s in e["steps"]),
                      "capped_steps": sum(s["capped"] for e in rec["epochs"] for s in e["steps"]),
                      "clipped_fraction": (sum(s["clipped"] for e in rec["epochs"] for s in e["steps"])
                                           / max(1, sum(s["decisions"] for e in rec["epochs"] for s in e["steps"])))})
    return {"arm": arm, "task": task, "effect": effect, "base": base, "updates": updates, "lr": lr, "seed": seed,
            "final": curve[-1], "curve": curve, "updates_detail": stats}


def finite_difference_check(seed=7):
    """Analytic minibatch gradient versus central differences of the PPO loss, with ratios away
    from 1 (perturbed parameters), coalesced choices (small remaining) and the mixture."""
    rng = np.random.default_rng(seed)
    out = {}
    for arm in ("S", "H"):
        wbytes = A.init_weights(arm, PARENT, None, "fd")
        w = A.read_weights(wbytes)
        games = []
        for g in range(3):
            decisions = []
            for i, remaining in enumerate((A.CAP, 300_000, 100_000, 20_000, 5)[: g + 3]):
                c = int(rng.integers(3))
                rec = A.consult_record(w, s_row(rng, c, remaining), rep_row(rng, arm), remaining, A.CAP,
                                       float(rng.uniform()), i)
                decisions.append({"x": A.decision_x(arm, rec), "q": A.base_q(remaining, A.CAP),
                                  "cs": A.choices(remaining), "chosen": rec["chosen"],
                                  "mu_b": np.array(rec["mu"]), "critic_b": rec["critic"],
                                  "adv": float(rng.choice([-1.4, 0.6, 1.1]))})
            games.append({"reward": float(rng.choice([-1, 1])), "decisions": decisions})
        games.append({"reward": 1.0, "decisions": []})  # zero-decision game in the denominator
        params = {k: v.copy() for k, v in w["params"].items()}
        # Move away from the initialization so that ratios differ from 1 but stay unclipped.
        params["wa"] = rng.normal(0, 0.02, size=params["wa"].shape)
        params["ba"] = rng.normal(0, 0.02, size=params["ba"].shape)

        def loss(p):
            _, st = A.minibatch_grad(p, w["norm"], games)
            return st["actor_loss"] + A.CRITIC_COEF * st["critic_loss"]

        grad, st = A.minibatch_grad(params, w["norm"], games)
        worst, checked = 0.0, 0
        for k in A.PARAMS:
            flat = params[k].reshape(-1)
            idxs = range(flat.size) if flat.size <= 256 else rng.choice(flat.size, 128, replace=False)
            for idx in idxs:
                eps = 1e-6
                up = copy.deepcopy(params)
                dn = copy.deepcopy(params)
                up[k].reshape(-1)[idx] += eps
                dn[k].reshape(-1)[idx] -= eps
                fd = (loss(up) - loss(dn)) / (2 * eps)
                worst = max(worst, abs(fd - grad[k].reshape(-1)[idx]))
                checked += 1
        out[arm] = {"max_abs_error": worst, "entries_checked": checked, "clipped_decisions": st["clipped"],
                    "decisions": st["decisions"]}
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--quick", action="store_true")
    a = ap.parse_args(argv)
    a.out.mkdir(parents=True, exist_ok=True)
    seeds = (1, 2) if a.quick else (1, 2, 3, 4, 5)
    runs = []
    # (arm, task, effect, base, updates, lr): one development cycle is 9 updates.
    configs = []
    for lr in LRS:
        for arm in ("S", "H"):
            configs += [(arm, "perfect", 1.0, 0.0, 9, lr), (arm, "contextual", 0.6, 0.2, 9, lr),
                        (arm, "contextual", 0.3, 0.35, 9, lr), (arm, "zero", 0.0, 0.5, 9, lr)]
    # Two cycles (the development maximum) at the chosen rate.
    for arm in ("S", "H"):
        configs += [(arm, "perfect", 1.0, 0.0, 18, A.LR), (arm, "contextual", 0.6, 0.2, 18, A.LR),
                    (arm, "zero", 0.0, 0.5, 18, A.LR)]
    for cfg in configs:
        for sd in seeds:
            runs.append(run(*cfg, seed=sd))
    fd = finite_difference_check()
    summary = []
    for cfg in configs:
        rs = [r for r in runs if (r["arm"], r["task"], r["effect"], r["base"], r["updates"], r["lr"]) == cfg]
        fin = [r["final"] for r in rs]
        summary.append({"arm": cfg[0], "task": cfg[1], "effect": cfg[2], "base": cfg[3], "updates": cfg[4],
                        "lr": cfg[5], "seeds": len(rs),
                        "tv_pi_q_mean": float(np.mean([f["tv_pi_q_mean"] for f in fin])),
                        "tv_pi_q_sd": float(np.std([f["tv_pi_q_mean"] for f in fin])),
                        "directed_gain_mean": float(np.mean([f["directed_gain"] for f in fin])),
                        "directed_gain_sd": float(np.std([f["directed_gain"] for f in fin])),
                        "tv_mu_q_mean": float(np.mean([f["tv_mu_q_mean"] for f in fin])),
                        "p_preferred_pi": {c: float(np.mean([f["contexts"][c]["p_preferred_pi"] for f in fin]))
                                           for c in ("0", "1", "2")},
                        "p_preferred_q": {c: fin[0]["contexts"][c]["p_preferred_q"] for c in ("0", "1", "2")},
                        "epochs_mean": float(np.mean([u["epochs"] for r in rs for u in r["updates_detail"]])),
                        "kl_stops": sum(u["epochs"] < A.EPOCHS for r in rs for u in r["updates_detail"])})
    doc = {"schema": "allocator-redesign-v1/calibration", "decisions_per_game": T_DECISIONS,
           "games_per_update": A.GAMES_PER_PARENT, "preferred_nominal": PREFERRED,
           "constants": {"lr_design": A.DESIGN_LR, "lr_chosen": A.LR, "epochs": A.EPOCHS, "clip": A.CLIP_RATIO, "grad_cap": A.GRAD_CAP,
                         "critic_coef": A.CRITIC_COEF, "kl_stop": A.KL_STOP, "epsilon": A.EPSILON},
           "finite_difference": fd, "summary": summary,
           # Per-run detail: the readout before and after the dose, and per-update statistics.
           "runs": [dict(r, curve=[r["curve"][0], r["curve"][-1]]) for r in runs]}
    (a.out / "CALIBRATION.json").write_text(json.dumps(doc, separators=(",", ":")) + "\n")
    print(json.dumps({"finite_difference": fd, "summary": summary}, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
