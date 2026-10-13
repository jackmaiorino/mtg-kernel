"""Report paired-credit information and measured cost at one frozen controller.

Uses whole factual games as samples. Rows must have the same frozen weight hash;
counterfactual decisions are never gradient samples. Process-ledger CPU includes
all attempts of each named row. A second, matched K=0 collection is needed for an
unpaired CPU comparison; synthetic timing is not substituted for native timing.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path

import numpy as np

spec = importlib.util.spec_from_file_location("allocator2", Path(__file__).with_name("allocator2.py"))
A = importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)


def report(weights, rows, cpu_seconds=None):
    w = A.read_weights(weights)
    gradients, sampled, discordant, extra_search, extra_real = [], 0, 0, 0, 0
    modes = {"paired_credit" in row["game"] for row in rows}
    if len(modes) != 1 or len(rows) < 2:
        raise ValueError("need at least two games from one credit mode")
    paired = modes.pop()
    roots = {row["game"].get("paired_credit", {}).get("max_roots", 0) for row in rows}
    if len(roots) != 1:
        raise ValueError("mixed reservoir capacities")
    for row in rows:
        reward, decisions = A.game_terms(row, w, A.sha(weights))
        if paired:
            A.paired_advantages(row, decisions, reward)
            for s in row["game"]["paired_credit"]["samples"]:
                sampled += 1
                discordant += A.REWARD[s["baseline"]["end"]] != reward
                extra_search += s["extra_search_transitions"]
                extra_real += s["extra_real_transitions"]
        else:
            for d in decisions:
                d.update(adv=reward - d["critic_b"], critic_coef=0.0)
        grad, _ = A.minibatch_grad(w["params"], w["norm"], [{"reward": reward, "decisions": decisions}])
        gradients.append(np.concatenate([grad[k].ravel() for k in ("w1", "b1", "wa", "ba")]))
    matrix = np.asarray(gradients)
    variance = float(np.var(matrix, axis=0, ddof=1).sum())
    cpu = cpu_seconds / len(rows) if cpu_seconds is not None and cpu_seconds > 0 else None
    return {"schema": "allocator-paired-v1/information", "weights_sha256": A.sha(weights),
            "games": len(rows), "credit_roots": roots.pop(), "sampled": sampled,
            "discordant": discordant, "discordant_fraction": discordant / sampled if sampled else None,
            "gradient_mean_norm": float(np.linalg.norm(matrix.mean(axis=0))),
            "per_game_actor_gradient_variance_trace": variance,
            "cpu_seconds_all_attempts": cpu_seconds, "cpu_seconds_per_completed_game": cpu,
            "variance_times_cpu": variance * cpu if cpu is not None else None,
            "extra_search_transitions": extra_search, "extra_real_transitions": extra_real}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--weights", type=Path, required=True)
    p.add_argument("--rows", type=Path, nargs="+", required=True)
    p.add_argument("--ledger", type=Path)
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    cpu = None
    if a.ledger:
        jobs = {path.stem for path in a.rows}
        records = [json.loads(s) for s in a.ledger.read_text().splitlines() if s.strip()]
        records = [r for r in records if r.get("kind") == "process" and r.get("job") in jobs]
        if {r["job"] for r in records} != jobs or any(r["cpu_seconds"] <= 0 for r in records):
            raise ValueError("ledger lacks measured CPU for every row/attempt")
        cpu = sum(r["cpu_seconds"] for r in records)
    result = report(a.weights.read_bytes(), [A.read_row(path) for path in a.rows], cpu)
    a.out.write_text(json.dumps(result, indent=1) + "\n")
    print(json.dumps(result, indent=1))


if __name__ == "__main__":
    main()
