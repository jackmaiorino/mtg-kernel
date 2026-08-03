#!/usr/bin/env python3
"""Fit and publish the scaled history terminal-outcome live candidate."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import time
from pathlib import Path
from typing import Any

import torch

import fit_complete_history_live_candidate_v1 as history_publish
import fit_policy_live_candidate as live
import run_scaled_history_outcome_policy_v1 as scaled
import run_screen as screen
import run_structured_outcome_policy_v1 as outcome


REPORT_SCHEMA = "mtg-kernel-scaled-history-outcome-policy-residual-fit/v1"
MODEL_STATE_SCHEMA = REPORT_SCHEMA + ".model-state"
METRIC_DECISIONS = 8_192


def _fail(message: str) -> None:
    raise ValueError(message)


def _publish(
    args: argparse.Namespace,
    model: screen.StructuredAdapter,
    cache: dict[str, Any],
    state_path: Path,
    state_sha256: str,
    parity_path: Path,
    parity_sha256: str,
    report_fields: dict[str, Any],
) -> dict[str, Any]:
    if args.output_root.exists():
        _fail(f"refusing to overwrite {args.output_root}")
    args.output_root.mkdir(parents=True)
    parent_output = args.output_root / "parent"
    parent_output.mkdir()
    parent_manifest = args.parent_outcome_root / "checkpoint.json"
    parent_payload = args.parent_outcome_root / "checkpoint.state.f32le"
    if (
        live._sha256(parent_manifest) != live.PARENT_MANIFEST_SHA256
        or live._sha256(parent_payload) != live.PARENT_PAYLOAD_SHA256
    ):
        _fail("parent root is not the exact retained checkpoint")
    shutil.copyfile(parent_manifest, parent_output / parent_manifest.name)
    shutil.copyfile(parent_payload, parent_output / parent_payload.name)

    payload = bytearray()
    parameters: list[dict[str, Any]] = []
    offset = 0
    for name, state_tensor in model.state_dict().items():
        tensor = state_tensor.detach().cpu().contiguous().float()
        raw = tensor.numpy().astype("<f4", copy=False).tobytes(order="C")
        count = tensor.numel()
        parameters.append(
            {
                "name": name,
                "shape": list(tensor.shape),
                "offset_f32": offset,
                "count_f32": count,
            }
        )
        payload.extend(raw)
        offset += count
    if offset != history_publish.EXPECTED_PARAMETER_COUNT:
        _fail("fixed history model parameter count mismatch")
    weights_path = args.output_root / "weights.f32le"
    weights_path.write_bytes(payload)
    weights_sha256 = live._sha256(weights_path)
    composite_sha256 = hashlib.sha256(
        history_publish.COMPOSITE_DOMAIN
        + bytes.fromhex(live.PARENT_MODEL_PARAMETER_SHA256)
        + bytes(payload)
    ).hexdigest()

    report = {
        "schema": REPORT_SCHEMA,
        "source": {
            "cache": str(args.cache),
            "cache_sha256": scaled.EXPECTED_CACHE_SHA256,
            "teacher_sha256": cache.get("source", {}).get("teacher_sha256"),
            "outcome_sha256": cache.get("source", {}).get("outcome_sha256"),
            "model_state": {"path": str(state_path), "sha256": state_sha256},
            "parity_fixture": {"path": str(parity_path), "sha256": parity_sha256},
            "source_commit": args.source_commit,
        },
        **report_fields,
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
        "non_claims": [
            "full development corpus reused for fitting",
            "full-data metrics are descriptive, not held-out evidence",
            "no live strength evidence",
            "no promotion or pro-level claim",
        ],
    }
    report_path = args.output_root / "report.json"
    report_path.write_bytes(history_publish._json_bytes(report))
    report_sha256 = live._sha256(report_path)
    candidate = {
        "schema": history_publish.SCHEMA,
        "publication_encoding": "json-pretty-sorted-utf8-trailing-lf/v1",
        "parent": {
            "directory": "parent",
            "manifest_sha256": live.PARENT_MANIFEST_SHA256,
            "payload_sha256": live.PARENT_PAYLOAD_SHA256,
            "native_state_sha256": live.PARENT_NATIVE_STATE_SHA256,
            "model_parameter_sha256": live.PARENT_MODEL_PARAMETER_SHA256,
            "adam_step": live.PARENT_ADAM_STEP,
        },
        "architecture": {
            "identity": history_publish.ARCHITECTURE,
            "state_dim": screen.STATE_DIM,
            "object_dim": screen.OBJECT_DIM,
            "edge_dim": screen.EDGE_DIM,
            "action_dim": screen.ACTION_DIM,
            "ref_dim": screen.REF_DIM,
            "hidden_dim": scaled.DIM,
            "card_vocab": scaled.CARD_VOCAB,
            "card_embedding_dim": max(8, scaled.DIM // 2),
            "group_vocab": scaled.GROUP_VOCAB,
            "group_embedding_dim": max(8, scaled.DIM // 3),
            "history_length": scaled.HISTORY_LENGTH,
            "history_feature_dim": scaled.HISTORY_FEATURE_DIM,
            "history_role_dim": 2,
            "value_model": history_publish.VALUE_MODEL,
        },
        "weights": {
            "filename": weights_path.name,
            "encoding": "ordered-row-major-finite-f32-little-endian/v1",
            "sha256": weights_sha256,
            "byte_count": len(payload),
            "parameter_count": offset,
            "parameters": parameters,
        },
        "report": {"filename": report_path.name, "sha256": report_sha256},
        "composite_model_parameter_sha256": composite_sha256,
    }
    candidate_path = args.output_root / "structured_history_candidate.json"
    candidate_path.write_bytes(history_publish._json_bytes(candidate))
    return {
        "candidate_root": str(args.output_root),
        "candidate_json_sha256": live._sha256(candidate_path),
        "weights_sha256": weights_sha256,
        "report_sha256": report_sha256,
        "composite_model_parameter_sha256": composite_sha256,
        "model_state_sha256": state_sha256,
        "parity_fixture_sha256": parity_sha256,
    }


def fit(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    if live._sha256(args.cache) != scaled.EXPECTED_CACHE_SHA256:
        _fail("complete-history cache SHA-256 mismatch")
    cache = torch.load(args.cache, map_location="cpu", weights_only=False)
    loaded = time.perf_counter()
    if (
        cache.get("version") != screen.SCRIPT_VERSION
        or not cache.get("complete_history_join")
    ):
        _fail("cache is not the validated complete-history corpus")
    policy = cache.get("policy")
    value = cache.get("value")
    if not isinstance(policy, list) or not policy or not isinstance(value, list) or not value:
        _fail("cache lanes must both be nonempty")
    screen._attach_complete_action_history(
        policy, value, scaled.HISTORY_LENGTH, scaled.CARD_VOCAB
    )
    decisions = outcome._physical_decisions(value)
    statistics = outcome._advantage_statistics(decisions)
    outcome._install_standardized_advantages(decisions, statistics)
    history_ready = time.perf_counter()

    screen._configure(scaled.SEED, args.threads)
    model = screen.StructuredAdapter(
        scaled.CARD_VOCAB,
        scaled.GROUP_VOCAB,
        scaled.DIM,
        scaled.HISTORY_LENGTH,
        scaled.HISTORY_FEATURE_DIM,
    )
    training_history = outcome._fit_model(
        model, decisions, scaled._fit_args(scaled.EPOCHS, args.threads)
    )
    trained = time.perf_counter()

    metric_sample = scaled._sample_decisions(decisions, METRIC_DECISIONS, scaled.SEED + 500)
    parents, residuals, weights = outcome._row_movement_inputs(model, metric_sample)
    uncalibrated = live._movement(parents, residuals, weights, 1.0)
    scale, calibrated = live._calibrate(
        parents, residuals, weights, scaled.TARGET_MEAN_TV
    )
    with torch.no_grad():
        model.policy_head.weight.mul_(scale)
        model.policy_head.bias.mul_(scale)
    calibrated_model = time.perf_counter()

    state_path = Path(str(args.output_root) + ".state.pt")
    parity_path = Path(str(args.output_root) + ".parity.json")
    if state_path.exists() or parity_path.exists():
        _fail("state or parity output already exists")
    screen._atomic_torch_save(
        {
            "schema": MODEL_STATE_SCHEMA,
            "cache_sha256": scaled.EXPECTED_CACHE_SHA256,
            "config": {
                "architecture": history_publish.ARCHITECTURE,
                "objective": "physical-decision-terminal-ppo/v1",
                "epochs": scaled.EPOCHS,
                "batch_size": scaled.BATCH_SIZE,
                "learning_rate": scaled.LR,
                "weight_decay": scaled.WEIGHT_DECAY,
                "ppo_clip": scaled.CLIP,
                "history_length": scaled.HISTORY_LENGTH,
                "seed": scaled.SEED,
                "threads": args.threads,
                "calibration_scale": scale,
            },
            "training_history": training_history,
            "model_state_dict": model.state_dict(),
        },
        state_path,
    )
    state_sha256 = live._sha256(state_path)
    parity_path.write_bytes(
        history_publish._json_bytes(
            history_publish._parity_fixture(model, policy, value)
        )
    )
    parity_sha256 = live._sha256(parity_path)
    checkpointed = time.perf_counter()

    surrogate = outcome._surrogate(model, metric_sample)
    movement = outcome._movement(model, metric_sample)
    diagnostics = outcome._diagnostics(
        model, metric_sample, scaled.SEED + 501, scaled.DIAGNOSTIC_SAMPLE_SIZE
    )
    measured = time.perf_counter()
    report_fields = {
        "config": {
            "architecture": history_publish.ARCHITECTURE,
            "objective": "physical-decision-terminal-ppo/v1",
            "dim": scaled.DIM,
            "card_vocab": scaled.CARD_VOCAB,
            "group_vocab": scaled.GROUP_VOCAB,
            "history_length": scaled.HISTORY_LENGTH,
            "history_feature_dim": scaled.HISTORY_FEATURE_DIM,
            "epochs": scaled.EPOCHS,
            "batch_size_physical_decisions": scaled.BATCH_SIZE,
            "learning_rate": scaled.LR,
            "weight_decay": scaled.WEIGHT_DECAY,
            "ppo_clip": scaled.CLIP,
            "gradient_norm_cap": scaled.GRAD_CAP,
            "seed": scaled.SEED,
            "threads": args.threads,
            "calibration_decision_sample": METRIC_DECISIONS,
            "target_mean_total_variation": scaled.TARGET_MEAN_TV,
            "value_model": "exact-retained-parent-unchanged",
        },
        "counts": {
            "pairs": 2_048,
            "episodes": len({group.episode_key for group in decisions}),
            "rows": len(value),
            "physical_decisions": len(decisions),
        },
        "advantage_statistics_by_candidate_seat": {
            str(key): value for key, value in statistics.items()
        },
        "training_history": training_history,
        "calibration": {
            "scale": scale,
            "uncalibrated_movement": uncalibrated,
            "calibrated_movement": calibrated,
        },
        "descriptive_terminal_surrogate": surrogate,
        "descriptive_movement": movement,
        "diagnostics": diagnostics,
        "runtime_seconds": measured - started,
        "phase_runtime_seconds": {
            "load_cache": loaded - started,
            "attach_history_and_group": history_ready - loaded,
            "train": trained - history_ready,
            "calibrate": calibrated_model - trained,
            "checkpoint": checkpointed - calibrated_model,
            "bounded_metrics": measured - checkpointed,
        },
    }
    publication = _publish(
        args,
        model,
        cache,
        state_path,
        state_sha256,
        parity_path,
        parity_sha256,
        report_fields,
    )
    publication.update(
        {
            "calibration_scale": scale,
            "movement": movement,
            "terminal_surrogate": surrogate,
            "runtime_seconds": time.perf_counter() - started,
        }
    )
    return publication


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--parent-outcome-root", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--threads", type=int, default=24)
    args = parser.parse_args()
    if args.threads != 24:
        _fail("full fit is fixed to 24 threads")
    print(json.dumps(fit(args), sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
