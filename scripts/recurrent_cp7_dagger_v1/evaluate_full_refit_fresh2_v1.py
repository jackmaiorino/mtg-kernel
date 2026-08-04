#!/usr/bin/env python3
"""Evaluate the full-refit dense-KL model on the second disjoint CP7 panel."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import time

os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")

import torch

import run_screen_v1 as base


SCHEMA = "mtg-kernel-dense-kl-recurrent-cp7-fresh2-gate/v1"
BASE_SEED = 1_400_001
PAIR_START = 320
PAIR_COUNT = 64
CORPUS_REPORT_SHA256 = "d53e1afcc4a772d5d7628a94f4a58e3b2adbbdb676d63fb5c478c4649842956c"
HISTORY_CACHE_SHA256 = "e542413e4269daa2176143acebe82a71e0d9f46cc3ebbb0bfd2face8b1390c99"
MODEL_FILE_SHA256 = "6c33f6d449b76e24c00bc7d46052b04488ddb9ec574009831d2fa90ea01bd55d"
MODEL_STATE_SHA256 = "d736296425de2c438bb9be02ab6c89e51da4c17c1408de6ff3309029b2d06dca"
LOG_RATIO_BUDGET = 0.49
BATCH_SIZE = 256


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
    observed = {
        "corpus_report": base._sha256(args.corpus_report),
        "history_cache": base._sha256(args.history_cache),
        "model": base._sha256(args.model),
    }
    expected = {
        "corpus_report": CORPUS_REPORT_SHA256,
        "history_cache": HISTORY_CACHE_SHA256,
        "model": MODEL_FILE_SHA256,
    }
    if observed != expected:
        base._fail("second fresh gate input identity mismatch")
    device = base._configure(args.device)
    started = time.perf_counter()
    labels, label_metadata = base.dagger._load_labels(
        args.corpus_report,
        base_seed=BASE_SEED,
        pair_start=PAIR_START,
        pair_count=PAIR_COUNT,
    )
    selected_pairs = set(range(PAIR_START, PAIR_START + PAIR_COUNT))
    decisions, cache_metadata, load_timings = base.dagger._load_decisions(
        args.history_cache,
        labels,
        expected_cache_sha256=HISTORY_CACHE_SHA256,
        selected_pairs=selected_pairs,
    )
    if {decision.pair_index for decision in decisions} != selected_pairs:
        base._fail("second fresh panel lacks exact pair coverage")
    payload = torch.load(args.model, map_location="cpu", weights_only=False)
    if (
        payload.get("beta") != 6.0
        or payload.get("epochs") != 8
        or payload.get("log_ratio_budget") != LOG_RATIO_BUDGET
        or payload.get("model_state_sha256") != MODEL_STATE_SHA256
    ):
        base._fail("full-refit model metadata mismatch")
    model = base._new_model(device)
    model.load_state_dict(payload["model_state_dict"], strict=True)
    if base._state_sha256(model) != MODEL_STATE_SHA256:
        base._fail("full-refit model state mismatch")
    metrics = base._evaluate(
        model, decisions, BATCH_SIZE, device, LOG_RATIO_BUDGET
    )
    gate = base._gate(metrics)
    source = {
        **label_metadata,
        **cache_metadata,
        "pair_start": PAIR_START,
        "pairs": PAIR_COUNT,
        "history_cache_sha256": HISTORY_CACHE_SHA256,
        "label_bearing_episode_count": len({decision.episode_key for decision in decisions}),
        "episodes_without_usable_labels": PAIR_COUNT * 2
        - len({decision.episode_key for decision in decisions}),
    }
    result = {
        "schema": SCHEMA,
        "decision": "PASS" if gate["pass"] else "REJECT",
        "candidate": {
            "model_file": str(args.model),
            "model_file_sha256": MODEL_FILE_SHA256,
            "model_state_sha256": MODEL_STATE_SHA256,
            "beta": 6.0,
            "epochs": 8,
            "log_ratio_budget": LOG_RATIO_BUDGET,
            "training_labels": 18_002,
        },
        "source": source,
        "load_timings": load_timings,
        "metrics": metrics,
        "gate": gate,
        "toolchain": base._toolchain(device),
        "git_commit": base._git_head(),
        "total_seconds": time.perf_counter() - started,
        "non_claims": [
            "fresh CP7-label generalization is not playing strength",
            "no fitting or selection used the second fresh panel",
            "terminal outcome remains the only promotion measure",
        ],
    }
    base._write_new(args.output_dir / "report.json", result)
    report_sha256 = base._sha256(args.output_dir / "report.json")
    base._write_new(
        args.output_dir / "manifest.json",
        {
            "schema": SCHEMA + ".manifest",
            "git_commit": base._git_head(),
            "toolchain": base._toolchain(device),
            "inputs": {
                "corpus_report_sha256": CORPUS_REPORT_SHA256,
                "history_cache_sha256": HISTORY_CACHE_SHA256,
                "teacher_task_sha256s": source["teacher_task_sha256s"],
                "model_file_sha256": MODEL_FILE_SHA256,
                "model_state_sha256": MODEL_STATE_SHA256,
            },
            "outputs": {"report_sha256": report_sha256},
        },
    )
    print(json.dumps(result, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
