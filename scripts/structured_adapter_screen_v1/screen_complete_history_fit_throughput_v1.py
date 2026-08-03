#!/usr/bin/env python3
"""Bounded topology screen for the fixed full-corpus history fit workload."""

from __future__ import annotations

import argparse
import json
import math
import os
import random
import time
from pathlib import Path
from typing import Any

import torch

import fit_complete_history_live_candidate_v1 as fit
import run_screen as screen


SCHEMA = "mtg-kernel-complete-history-fit-throughput-screen/v1"
SAMPLE_PER_LANE = 4_096
WARMUP_STEPS = 8
MEASURED_STEPS = 48
THREAD_ARMS = (6, 12, 18, 24)


def _batches(
    policy: list[dict[str, Any]], value: list[dict[str, Any]]
) -> list[tuple[list[dict[str, Any]], list[dict[str, Any]]]]:
    rng = random.Random(fit.SEED + 700)
    policy_order = list(range(len(policy)))
    value_order = list(range(len(value)))
    rng.shuffle(policy_order)
    rng.shuffle(value_order)
    count = WARMUP_STEPS + MEASURED_STEPS
    return [
        (
            [
                policy[policy_order[(step * fit.BATCH_SIZE + index) % len(policy_order)]]
                for index in range(fit.BATCH_SIZE)
            ],
            [
                value[value_order[(step * fit.BATCH_SIZE + index) % len(value_order)]]
                for index in range(fit.BATCH_SIZE)
            ],
        )
        for step in range(count)
    ]


def _run_arm(
    threads: int,
    batches: list[tuple[list[dict[str, Any]], list[dict[str, Any]]]],
) -> dict[str, float | int]:
    screen._configure(fit.SEED, threads)
    model = screen.StructuredAdapter(
        fit.CARD_VOCAB,
        fit.GROUP_VOCAB,
        fit.DIM,
        fit.HISTORY_LENGTH,
        fit.HISTORY_FEATURE_DIM,
    )
    optimizer = torch.optim.AdamW(
        model.parameters(), lr=fit.LR, weight_decay=fit.WEIGHT_DECAY
    )
    model.train()

    def step(batch: tuple[list[dict[str, Any]], list[dict[str, Any]]]) -> None:
        policy_loss, value_loss = screen._batch_loss(model, batch[0], batch[1])
        optimizer.zero_grad(set_to_none=True)
        (policy_loss + value_loss).backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 5.0)
        optimizer.step()

    for batch in batches[:WARMUP_STEPS]:
        step(batch)
    wall_started = time.perf_counter()
    cpu_started = time.process_time()
    for batch in batches[WARMUP_STEPS:]:
        step(batch)
    cpu_seconds = time.process_time() - cpu_started
    wall_seconds = time.perf_counter() - wall_started
    examples = MEASURED_STEPS * fit.BATCH_SIZE * 2
    return {
        "threads": threads,
        "measured_steps": MEASURED_STEPS,
        "policy_examples": MEASURED_STEPS * fit.BATCH_SIZE,
        "value_examples": MEASURED_STEPS * fit.BATCH_SIZE,
        "wall_seconds": wall_seconds,
        "process_cpu_seconds": cpu_seconds,
        "effective_process_cores": cpu_seconds / max(wall_seconds, 1.0e-12),
        "combined_examples_per_second": examples / max(wall_seconds, 1.0e-12),
        "training_steps_per_second": MEASURED_STEPS / max(wall_seconds, 1.0e-12),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    started = time.perf_counter()
    cache_sha256 = fit._sha256(args.cache)
    if cache_sha256 != fit.EXPECTED_CACHE_SHA256:
        raise ValueError("complete-history cache SHA-256 mismatch")
    cache = torch.load(args.cache, map_location="cpu", weights_only=False)
    all_policy = cache["policy"]
    all_value = cache["value"]
    card_vocab, group_vocab = screen._model_vocab(all_policy + all_value)
    if card_vocab != fit.CARD_VOCAB or group_vocab != fit.GROUP_VOCAB:
        raise ValueError("fixed model vocabulary mismatch")
    screen._attach_complete_action_history(
        all_policy, all_value, fit.HISTORY_LENGTH, fit.CARD_VOCAB
    )
    screen._assign_episode_weights(all_value)
    policy = screen._deterministic_sample(
        all_policy, SAMPLE_PER_LANE, fit.SEED + 701
    )
    value = screen._deterministic_sample(
        all_value, SAMPLE_PER_LANE, fit.SEED + 702
    )
    prepared = time.perf_counter()
    batches = _batches(policy, value)
    arms = [_run_arm(threads, batches) for threads in THREAD_ARMS]
    best = max(arms, key=lambda arm: float(arm["training_steps_per_second"]))
    result = {
        "schema": SCHEMA,
        "cache": str(args.cache),
        "cache_sha256": cache_sha256,
        "logical_processors": os.cpu_count(),
        "sample_per_lane": SAMPLE_PER_LANE,
        "warmup_steps": WARMUP_STEPS,
        "measured_steps": MEASURED_STEPS,
        "arms": arms,
        "selected_threads": best["threads"],
        "expected_full_fit_training_wall_seconds": (
            fit.EPOCHS
            * max(
                math.ceil(len(all_policy) / fit.BATCH_SIZE),
                math.ceil(len(all_value) / fit.BATCH_SIZE),
            )
            / float(best["training_steps_per_second"])
        ),
        "load_and_prepare_wall_seconds": prepared - started,
        "runtime_seconds": time.perf_counter() - started,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(result, sort_keys=True, indent=2, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
