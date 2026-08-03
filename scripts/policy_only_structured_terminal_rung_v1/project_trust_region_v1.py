#!/usr/bin/env python3
"""Project the rejected terminal-PPO direction into one fixed trust region."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys
import time
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import run_pipeline_v1 as pipeline  # noqa: E402


RESULT_SCHEMA = "mtg-kernel-structured-policy-terminal-trust-projection/v1"
MODEL_STATE_SCHEMA = RESULT_SCHEMA + ".model-state"
CANDIDATE_SCHEMA = "mtg-kernel-structured-policy-successor-candidate/v3"
REPORT_SCHEMA = "mtg-kernel-structured-policy-terminal-trust-projection-report/v1"
PARITY_SCHEMA = "mtg-kernel-structured-policy-terminal-trust-projection-parity-fixture/v1"
ARCHITECTURE = (
    "complete-public-history-structured-policy-terminal-rung-projected-"
    "frozen-parent-value/v1"
)
COMPOSITE_DOMAIN = (
    b"mtg-kernel-structured-policy-terminal-trust-projection-composite-model/v1"
)
PROJECTION_SCALE = 1.0 / 16.0
PROJECTION_METHOD = "linear-parameter-displacement-from-qualified-initializer/v1"
OBJECTIVE = "terminal-candidate-reward-only-clipped-ppo-trust-projection/v1"
SOURCE_FIT_REPORT_SHA256 = (
    "355c1b179ccd5de5d16f0aeb39dc101ae97a876208a2315358f98b06dcc30a81"
)
SOURCE_MODEL_STATE_SHA256 = (
    "4d1e9853d3472eb8817c10051c5ff779258bc1fc26130e956492ad598c877fe9"
)
SOURCE_CACHE_SHA256 = (
    "454e4ce1b8f7413839a36c8e2731fc0cb65581ce13e593634bffa70013a6f16d"
)


def _fail(message: str) -> None:
    raise ValueError(message)


def _read(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def _interpolate_state(
    initial: dict[str, torch.Tensor],
    trained: dict[str, torch.Tensor],
    scale: float = PROJECTION_SCALE,
) -> dict[str, torch.Tensor]:
    if initial.keys() != trained.keys() or not 0.0 < scale < 1.0:
        _fail("projection state contract mismatch")
    projected: dict[str, torch.Tensor] = {}
    for name in initial:
        before = initial[name]
        after = trained[name]
        if before.shape != after.shape or before.dtype != after.dtype:
            _fail(f"projection tensor contract mismatch: {name}")
        if torch.is_floating_point(before):
            projected[name] = before + (after - before) * scale
        else:
            if not torch.equal(before, after):
                _fail(f"non-floating projection tensor changed: {name}")
            projected[name] = before.clone()
    return projected


def _publish(
    args: argparse.Namespace,
    model: Any,
    source: dict[str, Any],
    movement: dict[str, Any],
) -> dict[str, Any]:
    if args.output_root.exists():
        _fail("projected candidate output root already exists")
    payload, parameters = pipeline.initializer._encoded_weights(model)
    args.output_root.mkdir(parents=True)
    parent_output = args.output_root / "parent"
    parent_output.mkdir()
    parent_manifest = args.parent_outcome_root / "checkpoint.json"
    parent_payload = args.parent_outcome_root / "checkpoint.state.f32le"
    if (
        pipeline._sha256(parent_manifest) != pipeline.live.PARENT_MANIFEST_SHA256
        or pipeline._sha256(parent_payload) != pipeline.live.PARENT_PAYLOAD_SHA256
    ):
        _fail("retained parent root identity mismatch")
    shutil.copyfile(parent_manifest, parent_output / parent_manifest.name)
    shutil.copyfile(parent_payload, parent_output / parent_payload.name)
    weights_path = args.output_root / "weights.f32le"
    weights_path.write_bytes(payload)
    weights_sha256 = pipeline._sha256(weights_path)
    composite_sha256 = hashlib.sha256(
        COMPOSITE_DOMAIN
        + bytes.fromhex(pipeline.live.PARENT_MODEL_PARAMETER_SHA256)
        + payload
    ).hexdigest()
    identity = source["initializer_identity"]
    report = {
        "schema": REPORT_SCHEMA,
        "initializer": {
            "candidate_json_sha256": identity["candidate_json_sha256"],
            "report_sha256": identity["report_sha256"],
            "weights_sha256": identity["weights_sha256"],
            "composite_model_parameter_sha256": identity[
                "composite_model_parameter_sha256"
            ],
            "model_state_sha256": pipeline.INITIALIZER_STATE_SHA256,
        },
        "source": {
            "cache_sha256": source["cache_sha256"],
            "pair_count": source["pair_count"],
            "base_seed": source["base_seed"],
            "pool_json_sha256": source["pool_json_sha256"],
            "source_commit": args.source_commit,
            "rejected_fit_report_sha256": SOURCE_FIT_REPORT_SHA256,
            "rejected_model_state_sha256": SOURCE_MODEL_STATE_SHA256,
        },
        "config": {
            "architecture": ARCHITECTURE,
            "value_model": pipeline.VALUE_MODEL,
            "seed": pipeline.FIT_SEED,
            "epochs": pipeline.FIT_EPOCHS,
            "batch_size_physical_decisions": pipeline.BATCH_SIZE,
            "learning_rate": pipeline.LR,
            "weight_decay": pipeline.WEIGHT_DECAY,
            "gradient_norm_cap": pipeline.GRAD_CAP,
            "ppo_clip": pipeline.CLIP,
            "history_length": pipeline.distill.HISTORY_LENGTH,
            "history_feature_dim": pipeline.distill.HISTORY_FEATURE_DIM,
            "weighting": "equal-episode-equal-physical-decision-joint-substep-ratio/v1",
            "advantage": "terminal-reward-minus-frozen-parent-value-seat-standardized/v1",
            "objective": OBJECTIVE,
            "projection_method": PROJECTION_METHOD,
            "projection_scale": PROJECTION_SCALE,
        },
        "movement": movement,
        "transport": {
            "maximum_absolute_logit_error": pipeline.PROVISIONAL_TRANSPORT_ERROR,
            "parent_value_bit_exact": False,
        },
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
    }
    report_path = args.output_root / "report.json"
    report_path.write_bytes(pipeline.history_publish._json_bytes(report))
    report_sha256 = pipeline._sha256(report_path)
    candidate = {
        "schema": CANDIDATE_SCHEMA,
        "publication_encoding": "json-pretty-sorted-utf8-trailing-lf/v1",
        "parent": {
            "directory": "parent",
            "manifest_sha256": pipeline.live.PARENT_MANIFEST_SHA256,
            "payload_sha256": pipeline.live.PARENT_PAYLOAD_SHA256,
            "native_state_sha256": pipeline.live.PARENT_NATIVE_STATE_SHA256,
            "model_parameter_sha256": pipeline.live.PARENT_MODEL_PARAMETER_SHA256,
            "adam_step": pipeline.live.PARENT_ADAM_STEP,
        },
        "architecture": {
            "identity": ARCHITECTURE,
            "state_dim": pipeline.screen.STATE_DIM,
            "object_dim": pipeline.screen.OBJECT_DIM,
            "edge_dim": pipeline.screen.EDGE_DIM,
            "action_dim": pipeline.screen.ACTION_DIM,
            "ref_dim": pipeline.screen.REF_DIM,
            "hidden_dim": pipeline.distill.DIM,
            "card_vocab": pipeline.distill.CARD_VOCAB,
            "card_embedding_dim": max(8, pipeline.distill.DIM // 2),
            "group_vocab": pipeline.distill.GROUP_VOCAB,
            "group_embedding_dim": max(8, pipeline.distill.DIM // 3),
            "history_length": pipeline.distill.HISTORY_LENGTH,
            "history_feature_dim": pipeline.distill.HISTORY_FEATURE_DIM,
            "history_role_dim": 2,
            "value_model": pipeline.VALUE_MODEL,
        },
        "weights": {
            "filename": weights_path.name,
            "encoding": "ordered-row-major-finite-f32-little-endian/v1",
            "sha256": weights_sha256,
            "byte_count": len(payload),
            "parameter_count": pipeline.history_publish.EXPECTED_PARAMETER_COUNT,
            "parameters": parameters,
        },
        "report": {"filename": report_path.name, "sha256": report_sha256},
        "composite_model_parameter_sha256": composite_sha256,
    }
    candidate_path = args.output_root / pipeline.CANDIDATE_FILENAME
    candidate_path.write_bytes(pipeline.history_publish._json_bytes(candidate))
    return {
        "decision": "STAGED_PENDING_NATIVE_TRANSPORT",
        "candidate_root": str(args.output_root),
        "candidate_json_sha256": pipeline._sha256(candidate_path),
        "report_sha256": report_sha256,
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
    }


def project(args: argparse.Namespace) -> dict[str, Any]:
    for path in (args.output, args.output_root, args.output_state, args.output_parity):
        if path.exists():
            _fail(f"projection output already exists: {path}")
    if (
        pipeline._sha256(args.cache) != SOURCE_CACHE_SHA256
        or pipeline._sha256(args.initializer_state)
        != pipeline.INITIALIZER_STATE_SHA256
        or pipeline._sha256(args.trained_state) != SOURCE_MODEL_STATE_SHA256
        or pipeline._sha256(args.fit_report) != SOURCE_FIT_REPORT_SHA256
    ):
        _fail("projection source identity mismatch")
    fit_report = _read(args.fit_report)
    if (
        fit_report.get("decision") != "REJECT"
        or fit_report.get("publication") != "WITHHELD"
        or fit_report.get("fit_gate", {}).get("checks", {}).get(
            "maximum_absolute_joint_log_ratio_at_most_0p50"
        )
        is not False
    ):
        _fail("projection source is not the expected rejected fit")
    started = time.perf_counter()
    decisions, source, timings = pipeline._load_decisions(args.cache, None)
    if (
        source["pair_count"] != pipeline.FORMAL_PAIRS
        or source["base_seed"] != pipeline.FORMAL_BASE_SEED
    ):
        _fail("projection cache panel mismatch")
    initial_payload = torch.load(
        args.initializer_state, map_location="cpu", weights_only=False
    )
    trained_payload = torch.load(
        args.trained_state, map_location="cpu", weights_only=False
    )
    if (
        initial_payload.get("schema") != pipeline.initializer.MODEL_STATE_SCHEMA
        or trained_payload.get("schema") != pipeline.MODEL_STATE_SCHEMA
    ):
        _fail("projection model-state schema mismatch")
    pipeline.screen._configure(pipeline.FIT_SEED, args.threads)
    initial_model = pipeline.distill._model()
    initial_model.load_state_dict(initial_payload["model_state_dict"], strict=True)
    initial_value_bits = pipeline.initializer._value_head_bits(initial_model)
    alignment = pipeline._alignment(initial_model, decisions)
    if not alignment["pass"]:
        _fail("qualified initializer no longer matches behavior logits")
    model = pipeline.distill._model()
    model.load_state_dict(
        _interpolate_state(
            initial_payload["model_state_dict"],
            trained_payload["model_state_dict"],
        ),
        strict=True,
    )
    if pipeline.initializer._value_head_bits(model) != initial_value_bits:
        _fail("projection changed the frozen value head")
    loaded = time.perf_counter()
    movement = pipeline._movement(model, decisions)
    gate = pipeline._fit_gate(movement)
    measured = time.perf_counter()
    result: dict[str, Any] = {
        "schema": RESULT_SCHEMA,
        "decision": gate["decision"],
        "source": source,
        "config": {
            "threads": args.threads,
            "projection_scale": PROJECTION_SCALE,
            "projection_method": PROJECTION_METHOD,
            "source_fit_report_sha256": SOURCE_FIT_REPORT_SHA256,
            "source_model_state_sha256": SOURCE_MODEL_STATE_SHA256,
        },
        "initializer_alignment": alignment,
        "movement": movement,
        "fit_gate": gate,
        "phase_runtime_seconds": {
            **timings,
            "load_and_project_seconds": loaded - (started + sum(timings.values())),
            "movement_seconds": measured - loaded,
        },
        "runtime_seconds": measured - started,
    }
    pipeline.screen._atomic_torch_save(
        {
            "schema": MODEL_STATE_SCHEMA,
            "source": source,
            "config": result["config"],
            "movement": movement,
            "fit_gate": gate,
            "model_state_dict": model.state_dict(),
        },
        args.output_state,
    )
    parity = pipeline.initializer._parity_fixture(model, decisions)
    parity["schema"] = PARITY_SCHEMA
    pipeline._write_new_json(args.output_parity, parity)
    result["model_state"] = {
        "path": str(args.output_state),
        "sha256": pipeline._sha256(args.output_state),
    }
    result["parity_fixture"] = {
        "path": str(args.output_parity),
        "sha256": pipeline._sha256(args.output_parity),
    }
    if gate["decision"] == "PASS":
        result["publication"] = _publish(args, model, source, movement)
    else:
        result["publication"] = "WITHHELD"
    pipeline._write_new_json(args.output, result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--initializer-state", type=Path, required=True)
    parser.add_argument("--trained-state", type=Path, required=True)
    parser.add_argument("--fit-report", type=Path, required=True)
    parser.add_argument("--parent-outcome-root", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--output-state", type=Path, required=True)
    parser.add_argument("--output-parity", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--threads", type=int, choices=pipeline.THREAD_CHOICES, required=True)
    print(json.dumps(project(parser.parse_args()), sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
