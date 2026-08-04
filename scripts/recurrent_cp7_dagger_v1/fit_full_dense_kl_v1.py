#!/usr/bin/env python3
"""Refit the fixed beta-6 recurrent CP7 residual on all available labels."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import random
import time
from typing import Any

os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")

import torch

import evaluate_fresh_panel_v1 as fresh
import run_dense_kl_v1 as dense
import run_screen_v1 as base
import run_sparse_correction_v1 as sparse


SCHEMA = "mtg-kernel-dense-kl-recurrent-cp7-full-refit/v1"
BETA = 6.0
EPOCHS = 8
BATCH_SIZE = 256
LOG_RATIO_BUDGET = 0.49


def _load_fresh(
    corpus_report: Path, history_cache: Path
) -> tuple[list[Any], dict[str, Any], dict[str, float]]:
    if base._sha256(corpus_report) != fresh.CORPUS_REPORT_SHA256:
        base._fail("first fresh corpus report identity mismatch")
    if base._sha256(history_cache) != fresh.HISTORY_CACHE_SHA256:
        base._fail("first fresh cache identity mismatch")
    labels, label_metadata = base.dagger._load_labels(
        corpus_report,
        base_seed=fresh.BASE_SEED,
        pair_start=fresh.PAIR_START,
        pair_count=fresh.PAIR_COUNT,
    )
    pairs = set(range(fresh.PAIR_START, fresh.PAIR_START + fresh.PAIR_COUNT))
    decisions, cache_metadata, timings = base.dagger._load_decisions(
        history_cache,
        labels,
        expected_cache_sha256=fresh.HISTORY_CACHE_SHA256,
        selected_pairs=pairs,
    )
    return decisions, {**label_metadata, **cache_metadata}, timings


def _fit(decisions: list[Any], device: torch.device) -> tuple[Any, list[dict[str, Any]]]:
    model = base._new_model(device)
    parameters = [parameter for parameter in model.parameters() if parameter.requires_grad]
    optimizer = torch.optim.AdamW(
        parameters, lr=base.LEARNING_RATE, weight_decay=base.WEIGHT_DECAY
    )
    rng = random.Random(base.SEED)
    history: list[dict[str, Any]] = []
    for epoch in range(1, EPOCHS + 1):
        model.train()
        started = time.perf_counter()
        sums = {"loss": 0.0, "cross_entropy": 0.0, "parent_kl": 0.0}
        gradient_max = 0.0
        steps = 0
        for batch in sparse._batches(decisions, rng):
            loss, parts = dense._loss(model, batch, BETA, device)
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient = torch.nn.utils.clip_grad_norm_(parameters, base.GRADIENT_CAP)
            if not torch.isfinite(gradient):
                base._fail("non-finite full-refit gradient")
            optimizer.step()
            sums["loss"] += float(loss.detach())
            sums["cross_entropy"] += parts["cross_entropy"]
            sums["parent_kl"] += parts["parent_kl"]
            gradient_max = max(gradient_max, float(gradient))
            steps += 1
        torch.cuda.synchronize(device)
        history.append(
            {
                "epoch": epoch,
                "optimizer_steps": steps,
                "seconds": time.perf_counter() - started,
                "maximum_preclip_gradient_norm": gradient_max,
                "mean_loss": sums["loss"] / steps,
                "mean_cross_entropy": sums["cross_entropy"] / steps,
                "mean_parent_kl": sums["parent_kl"] / steps,
            }
        )
    return model, history


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--original-corpus-report", type=Path, required=True)
    parser.add_argument("--original-history-cache", type=Path, required=True)
    parser.add_argument("--fresh-corpus-report", type=Path, required=True)
    parser.add_argument("--fresh-history-cache", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--device", type=int, default=1)
    args = parser.parse_args()
    if args.output_dir.exists() and any(args.output_dir.iterdir()):
        base._fail("output directory must be absent or empty")
    device = base._configure(args.device)
    started = time.perf_counter()
    original, original_source, original_timings = base._load(
        args.original_corpus_report, args.original_history_cache
    )
    first_fresh, fresh_source, fresh_timings = _load_fresh(
        args.fresh_corpus_report, args.fresh_history_cache
    )
    decisions = original + first_fresh
    keys = [decision.key for decision in decisions]
    if len(keys) != len(set(keys)):
        base._fail("combined full-refit corpus has duplicate decisions")
    if {decision.pair_index for decision in decisions} != set(range(320)):
        base._fail("combined full-refit corpus lacks exact pair coverage")
    model, history = _fit(decisions, device)
    fit_metrics = base._evaluate(
        model, decisions, BATCH_SIZE, device, LOG_RATIO_BUDGET
    )
    state_sha256 = base._state_sha256(model)
    model_path = args.output_dir / "model.pt"
    model_path.parent.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "schema": SCHEMA + ".model",
            "beta": BETA,
            "epochs": EPOCHS,
            "log_ratio_budget": LOG_RATIO_BUDGET,
            "model_state_dict": {
                name: tensor.detach().cpu() for name, tensor in model.state_dict().items()
            },
            "model_state_sha256": state_sha256,
        },
        model_path,
    )
    result = {
        "schema": SCHEMA,
        "status": "complete",
        "config": {
            "beta": BETA,
            "epochs": EPOCHS,
            "batch_size": BATCH_SIZE,
            "log_ratio_budget": LOG_RATIO_BUDGET,
            "seed": base.SEED,
        },
        "source": {
            "original": original_source,
            "first_fresh": fresh_source,
            "pairs": 320,
            "games": 640,
            "labels": len(decisions),
            "label_bearing_episodes": len({decision.episode_key for decision in decisions}),
        },
        "load_timings": {"original": original_timings, "first_fresh": fresh_timings},
        "training_history": history,
        "fit_diagnostics": fit_metrics,
        "model_state_sha256": state_sha256,
        "toolchain": base._toolchain(device),
        "git_commit": base._git_head(),
        "total_seconds": time.perf_counter() - started,
        "non_claims": [
            "fit diagnostics are not held-out evidence",
            "the first fresh panel is now training data",
            "terminal outcome remains the only promotion measure",
        ],
    }
    base._write_new(args.output_dir / "report.json", result)
    outputs = {
        "report_sha256": base._sha256(args.output_dir / "report.json"),
        "model_file_sha256": base._sha256(model_path),
        "model_state_sha256": state_sha256,
    }
    base._write_new(
        args.output_dir / "manifest.json",
        {
            "schema": SCHEMA + ".manifest",
            "git_commit": base._git_head(),
            "seed": base.SEED,
            "toolchain": base._toolchain(device),
            "inputs": {
                "original_corpus_report_sha256": original_source["corpus_report_sha256"],
                "original_history_cache_sha256": original_source["history_cache_sha256"],
                "fresh_corpus_report_sha256": fresh_source["corpus_report_sha256"],
                "fresh_history_cache_sha256": fresh_source["history_cache_sha256"],
            },
            "outputs": outputs,
        },
    )
    print(json.dumps(result, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
