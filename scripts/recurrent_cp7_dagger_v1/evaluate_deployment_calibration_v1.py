#!/usr/bin/env python3
"""Diagnostic evaluation of the fixed 0.97 post-projection deployment scale."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import time

os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")

import torch

import evaluate_full_refit_fresh2_v1 as fixed
import run_screen_v1 as base


SCHEMA = "mtg-kernel-recurrent-cp7-deployment-calibration/v1"
DEPLOYMENT_SCALE = 0.97


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus-report", type=Path, required=True)
    parser.add_argument("--history-cache", type=Path, required=True)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--device", type=int, default=1)
    args = parser.parse_args()
    if args.output_dir.exists() and any(args.output_dir.iterdir()):
        base._fail("output directory must be absent or empty")
    observed = (
        base._sha256(args.corpus_report),
        base._sha256(args.history_cache),
        base._sha256(args.model),
    )
    expected = (
        fixed.CORPUS_REPORT_SHA256,
        fixed.HISTORY_CACHE_SHA256,
        fixed.MODEL_FILE_SHA256,
    )
    if observed != expected:
        base._fail("deployment calibration input identity mismatch")
    device = base._configure(args.device)
    started = time.perf_counter()
    labels, label_metadata = base.dagger._load_labels(
        args.corpus_report,
        base_seed=fixed.BASE_SEED,
        pair_start=fixed.PAIR_START,
        pair_count=fixed.PAIR_COUNT,
    )
    pairs = set(range(fixed.PAIR_START, fixed.PAIR_START + fixed.PAIR_COUNT))
    decisions, cache_metadata, load_timings = base.dagger._load_decisions(
        args.history_cache,
        labels,
        expected_cache_sha256=fixed.HISTORY_CACHE_SHA256,
        selected_pairs=pairs,
    )
    payload = torch.load(args.model, map_location="cpu", weights_only=False)
    if payload.get("model_state_sha256") != fixed.MODEL_STATE_SHA256:
        base._fail("deployment calibration model-state metadata mismatch")
    model = base._new_model(device)
    model.load_state_dict(payload["model_state_dict"], strict=True)
    if base._state_sha256(model) != fixed.MODEL_STATE_SHA256:
        base._fail("deployment calibration loaded model state mismatch")
    metrics = base._evaluate(
        model,
        decisions,
        fixed.BATCH_SIZE,
        device,
        fixed.LOG_RATIO_BUDGET,
        DEPLOYMENT_SCALE,
    )
    gate = base._gate(metrics)
    result = {
        "schema": SCHEMA,
        "status": "MECHANICAL-PASS" if gate["pass"] else "MECHANICAL-REJECT",
        "deployment_scale": DEPLOYMENT_SCALE,
        "model_file_sha256": fixed.MODEL_FILE_SHA256,
        "model_state_sha256": fixed.MODEL_STATE_SHA256,
        "source": {**label_metadata, **cache_metadata},
        "load_timings": load_timings,
        "metrics": metrics,
        "diagnostic_gate": gate,
        "toolchain": base._toolchain(device),
        "git_commit": base._git_head(),
        "total_seconds": time.perf_counter() - started,
        "non_claims": [
            "the panel was revealed before deployment scaling",
            "this is not held-out CP7-label evidence",
            "terminal outcome remains the only promotion measure",
        ],
    }
    base._write_new(args.output_dir / "report.json", result)
    base._write_new(
        args.output_dir / "manifest.json",
        {
            "schema": SCHEMA + ".manifest",
            "git_commit": base._git_head(),
            "inputs": {
                "corpus_report_sha256": fixed.CORPUS_REPORT_SHA256,
                "history_cache_sha256": fixed.HISTORY_CACHE_SHA256,
                "model_file_sha256": fixed.MODEL_FILE_SHA256,
                "model_state_sha256": fixed.MODEL_STATE_SHA256,
            },
            "outputs": {
                "report_sha256": base._sha256(args.output_dir / "report.json")
            },
        },
    )
    print(json.dumps(result, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
