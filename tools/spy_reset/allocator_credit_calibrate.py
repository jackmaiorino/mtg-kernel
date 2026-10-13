"""Cheap paired-credit calibration through allocator2.update; no Magic games.

Ten contexts/choices, one Bernoulli terminal result. The same exogenous table
drives a skip intervention and its factual continuation, including all later
budget-dependent requests. The 0.6 task adds six win-probability points per
correct choice out of ten. Zero and harmful-search tasks share the same noise.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import time

import numpy as np


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


A = load("allocator2", "allocator2.py")
C = load("allocator2_calibrate", "allocator2_calibrate.py")


def collect(wb, rng, task, gid, roots):
    w = A.read_weights(wb)
    arm = w["arm"]
    contexts = rng.integers(3, size=10)
    noise = rng.uniform(0, 1, size=(10, len(A.S_FEATURES)))
    reps = rng.uniform(-1, 1, size=(10, 3 * A.REP)) if arm == "H" else [[]] * 10
    draws = rng.uniform(size=10)
    terminal_u = rng.uniform()
    sample_seed = int(rng.integers(1 << 62))

    def play(skip=None):
        remaining, correct, records = A.CAP, 0, []
        for i, context in enumerate(contexts):
            if not remaining:
                break
            x = noise[i].copy()
            x[:4] = [1, remaining / A.CAP, remaining / A.CAP, 1 - remaining / A.CAP]
            for j, slot in enumerate(C.CONTEXT_SLOTS):
                x[slot] = float(j == context)
            rec = A.consult_record(w, x.tolist(), list(reps[i]), remaining, A.CAP, float(draws[i]), i)
            chosen = 0 if i == skip else rec["chosen"]
            amount, members = rec["amounts"][chosen], rec["members"][chosen]
            preferred = 0 if task == "harmful" else C.PREFERRED[context]
            correct += preferred in members
            remaining -= amount
            records.append(rec)
        pwin = 0.5 if task == "zero" else 0.2 + 0.6 * correct / 10
        return "win" if terminal_u < pwin else "loss", records

    end, records = play()
    row = A.synthetic_row(gid, arm, "r1", wb, records, end)
    if roots:
        count = len(records)
        selected = A.reservoir_indices(sample_seed, count, roots)
        row["game"]["paired_credit"] = {
            "schema": "allocator-paired-v1", "coupling": "event-step-v1", "max_roots": roots,
            "sample_seed": sample_seed, "eligible": count, "samples": [
                {"index": i, "amount": records[i]["amount"],
                 "inclusion_probability": len(selected) / count,
                 "baseline": {"valid": True, "end": play(i)[0]}}
                for i in selected]}
    return row


def run(arm, task, roots, seed, updates):
    rng = np.random.default_rng(seed)
    norm = C.frozen_norm(arm, np.random.default_rng(seed + 1000), "contextual")
    wb = A.init_weights(arm, C.PARENT, norm, f"paired-calibration-{seed}")
    probes = C.probe_set(arm)
    preferred = (0, 0, 0) if task == "harmful" else C.PREFERRED
    initial = C.readout(A.read_weights(wb), probes, preferred)
    adam = None
    discordant = sampled = 0
    started = time.process_time()
    for k in range(updates):
        rows = [collect(wb, rng, task, f"{seed}-{k}-{g}", roots) for g in range(A.GAMES_PER_PARENT)]
        for row in rows:
            for sample in row["game"].get("paired_credit", {}).get("samples", []):
                sampled += 1
                discordant += sample["baseline"]["end"] != row["game"]["end"]
        wb, adam, _ = A.update(wb, adam, rows, k, "r1")
    final = C.readout(A.read_weights(wb), probes, preferred)
    return {"arm": arm, "task": task, "roots": roots, "seed": seed, "updates": updates,
            "games": updates * A.GAMES_PER_PARENT, "initial": initial, "final": final,
            "sampled": sampled, "discordant": discordant, "cpu_seconds": time.process_time() - started}


def variance(seed=765, games=300):
    """Variance trace of the per-game actor gradient before clipping, at fixed weights.

    Cost here is synthetic CPU only; native measured cost must be supplied before a
    real-compute efficiency claim. Whole games, not individual forks, are the samples.
    """
    wb = A.init_weights("S", C.PARENT, None, "variance")
    w = A.read_weights(wb)
    out = []
    for roots in (0, 1, 3):
        rng = np.random.default_rng(seed)
        grads, started = [], time.process_time()
        for i in range(games):
            row = collect(wb, rng, "contextual", str(i), roots)
            reward, decisions = A.game_terms(row, w, A.sha(wb))
            if roots:
                A.paired_advantages(row, decisions, reward)
            else:
                for d in decisions:
                    d.update(adv=reward, critic_coef=0.0)
            grad, _ = A.minibatch_grad(w["params"], w["norm"], [{"reward": reward, "decisions": decisions}])
            grads.append(np.concatenate([grad[k].ravel() for k in ("w1", "b1", "wa", "ba")]))
        elapsed = time.process_time() - started
        var = float(np.var(np.array(grads), axis=0, ddof=1).sum())
        out.append({"roots": roots, "games": games, "gradient_variance_trace": var,
                    "synthetic_cpu_per_game": elapsed / games,
                    "synthetic_variance_times_cost": var * elapsed / games})
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--seeds", type=int, default=5)
    parser.add_argument("--updates", type=int, default=9)
    parser.add_argument("--tasks", nargs="+", choices=("contextual", "zero", "harmful"),
                        default=("contextual", "zero", "harmful"))
    args = parser.parse_args()
    runs = []
    for arm in ("S", "H"):
        for task in args.tasks:
            for roots in (0, 1, 3):
                for seed in range(1, args.seeds + 1):
                    runs.append(run(arm, task, roots, seed, args.updates))
                print(f"finished {arm} {task} roots={roots}", flush=True)
    summary = []
    for arm in ("S", "H"):
        for task in args.tasks:
            for roots in (0, 1, 3):
                rr = [r for r in runs if (r["arm"], r["task"], r["roots"]) == (arm, task, roots)]
                gains = [r["final"]["directed_gain"] for r in rr]
                summary.append({"arm": arm, "task": task, "roots": roots,
                                "directed_mean": float(np.mean(gains)), "directed_sd": float(np.std(gains)),
                                "tv_mean": float(np.mean([r["final"]["tv_pi_q_mean"] for r in rr])),
                                "discordant": sum(r["discordant"] for r in rr)})
    args.out.parent.mkdir(parents=True, exist_ok=True)
    sources = {name: A.sha(Path(__file__).with_name(name).read_bytes()) for name in
               ("allocator_credit_calibrate.py", "allocator2.py", "allocator2_calibrate.py")}
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=Path(__file__).parent, text=True).strip()
    args.out.write_text(json.dumps({"schema": "allocator-paired-v1/calibration", "seeds": args.seeds,
                                   "commit": revision, "sources": sources, "tasks": args.tasks,
                                   "updates": args.updates, "summary": summary, "variance": variance(),
                                   "runs": runs}, indent=1) + "\n")
    print(json.dumps(summary, indent=1))


if __name__ == "__main__":
    main()
