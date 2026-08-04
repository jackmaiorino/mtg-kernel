#!/usr/bin/env python3
"""Evaluate the selected dense-KL model on one disjoint CP7 label panel."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import time

os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")

import torch

import run_screen_v1 as base


SCHEMA = "mtg-kernel-dense-kl-recurrent-cp7-fresh-gate/v1"
BASE_SEED = 1_400_001
PAIR_START = 256
PAIR_COUNT = 64
CORPUS_REPORT_SHA256 = "38b0102fe285557be16107894e83e657f3edb34ae5d289cbad79a3d1e5f79303"
HISTORY_CACHE_SHA256 = "05b815ee237043865e23457ba69ec791a5c07aeac6d09778fed90074e8c16278"
MODEL_FILE_SHA256 = "93732c91aee17782441ee7c8276ae4337a093ca643912e8c734df10de511265a"
MODEL_STATE_SHA256 = "0c2f0b83235cde8af05ca98c8ed58c06157ce3de5ff9305145b70f54efedc903"
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
    if base._sha256(args.corpus_report) != CORPUS_REPORT_SHA256:
        base._fail("fresh corpus report identity mismatch")
    if base._sha256(args.history_cache) != HISTORY_CACHE_SHA256:
        base._fail("fresh complete-history cache identity mismatch")
    if base._sha256(args.model) != MODEL_FILE_SHA256:
        base._fail("selected dense-KL model file identity mismatch")
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
        base._fail("fresh panel lacks exact pair coverage")
    payload = torch.load(args.model, map_location="cpu", weights_only=False)
    if (
        payload.get("beta") != 6.0
        or payload.get("selected_epoch") != 8
        or payload.get("log_ratio_budget") != LOG_RATIO_BUDGET
        or payload.get("model_state_sha256") != MODEL_STATE_SHA256
    ):
        base._fail("selected dense-KL model metadata mismatch")
    model = base._new_model(device)
    model.load_state_dict(payload["model_state_dict"], strict=True)
    if base._state_sha256(model) != MODEL_STATE_SHA256:
        base._fail("loaded dense-KL model state mismatch")
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
            "selected_epoch": 8,
            "log_ratio_budget": LOG_RATIO_BUDGET,
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
            "no fitting or selection used the fresh panel",
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
                "corpus_report_sha256": source["corpus_report_sha256"],
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
