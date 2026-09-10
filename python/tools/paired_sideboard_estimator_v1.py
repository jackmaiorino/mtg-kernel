#!/usr/bin/env python3
"""Independent Python cross-check of the Rust paired-bootstrap estimator
(mtg-kernel/src/paired_bo1_harness_v1.rs). Reimplements the same
case-resampling algorithm from scratch so a Rust bug and a Python bug are
unlikely to agree; the two are compared against each other on real receipts
in W6, not merged into one shared implementation.
"""
from __future__ import annotations

import argparse
import json
import sys


def splitmix64(seed: int):
    state = seed & 0xFFFFFFFFFFFFFFFF

    def next_u64() -> int:
        nonlocal state
        state = (state + 0x9E3779B97F4A7C15) & 0xFFFFFFFFFFFFFFFF
        z = state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & 0xFFFFFFFFFFFFFFFF
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & 0xFFFFFFFFFFFFFFFF
        return z ^ (z >> 31)

    return next_u64


def paired_bootstrap_ci(deltas: list[int], resample_count: int, seed: int, sidedness: str) -> dict:
    if not deltas:
        raise ValueError("bootstrap is undefined over zero deltas")
    if resample_count <= 0:
        raise ValueError("resample_count must be positive")
    mean = sum(deltas) / len(deltas)
    next_u64 = splitmix64(seed)
    resample_means = []
    n = len(deltas)
    for _ in range(resample_count):
        total = 0
        for _ in range(n):
            index = next_u64() % n
            total += deltas[index]
        resample_means.append(total / n)
    resample_means.sort()
    if sidedness == "one_sided_lower":
        alpha_index = min(int(resample_count * 0.05), resample_count - 1)
        lower = resample_means[alpha_index]
        upper = float("inf")
    elif sidedness == "two_sided":
        lower_index = min(int(resample_count * 0.025), resample_count - 1)
        upper_index = min(int(resample_count * 0.975), resample_count - 1)
        lower = resample_means[lower_index]
        upper = resample_means[upper_index]
    else:
        raise ValueError(f"unknown sidedness {sidedness!r}")
    return {
        "mean": mean,
        "lower": lower,
        "upper": upper,
        "resample_count": resample_count,
        "seed": seed,
        "sidedness": sidedness,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--deltas-jsonl", required=True)
    parser.add_argument("--resample-count", type=int, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--sidedness", choices=["one_sided_lower", "two_sided"], required=True)
    args = parser.parse_args(argv)

    deltas = []
    with open(args.deltas_jsonl, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            deltas.append(int(json.loads(line)["delta"]))

    report = paired_bootstrap_ci(deltas, args.resample_count, args.seed, args.sidedness)
    json.dump(report, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
