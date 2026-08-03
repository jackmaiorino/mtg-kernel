#!/usr/bin/env python3
"""Fit one terminal-only PPO update on the structured policy head alone."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import shutil
import sys
import time
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import run_pipeline_v1 as pipeline  # noqa: E402


FIT_SCHEMA = "mtg-kernel-structured-policy-terminal-head-only-fit/v1"
MODEL_STATE_SCHEMA = FIT_SCHEMA + ".model-state"
CANDIDATE_SCHEMA = "mtg-kernel-structured-policy-successor-candidate/v4"
REPORT_SCHEMA = "mtg-kernel-structured-policy-terminal-head-only-report/v1"
PARITY_SCHEMA = "mtg-kernel-structured-policy-terminal-head-only-parity-fixture/v1"
ARCHITECTURE = (
    "complete-public-history-structured-policy-terminal-head-only-"
    "frozen-parent-value/v1"
)
COMPOSITE_DOMAIN = (
    b"mtg-kernel-structured-policy-terminal-head-only-composite-model/v1"
)
OBJECTIVE = "terminal-candidate-reward-only-clipped-ppo-policy-head-weight-only/v1"
FIT_SEED = 20_260_806
TRAINABLE_PARAMETER = "policy_head.weight"
SOURCE_CACHE_SHA256 = (
    "454e4ce1b8f7413839a36c8e2731fc0cb65581ce13e593634bffa70013a6f16d"
)


def _fail(message: str) -> None:
    raise ValueError(message)


def _frozen_state(model: Any) -> dict[str, bytes]:
    return {
        name: tensor.detach().cpu().contiguous().numpy().tobytes()
        for name, tensor in model.state_dict().items()
        if name != TRAINABLE_PARAMETER
    }


def _attach_policy_latents(model: Any, decisions: list[Any]) -> int:
    captured: list[torch.Tensor] = []

    def capture(_module: Any, inputs: tuple[torch.Tensor, ...]) -> None:
        if len(inputs) != 1:
            _fail("policy-head hook input contract mismatch")
        captured.append(inputs[0].detach().clone())

    handle = model.policy_head.register_forward_pre_hook(capture)
    rows = 0
    model.eval()
    try:
        with torch.no_grad():
            for decision in decisions:
                for row in decision.rows:
                    captured.clear()
                    logits, _ = model._one(row)
                    if len(captured) != 1 or captured[0].shape != (
                        logits.numel(),
                        pipeline.distill.DIM,
                    ):
                        _fail("policy-head latent capture mismatch")
                    reproduced = model.policy_head(captured[0]).squeeze(-1)
                    if not torch.equal(reproduced, logits):
                        _fail("policy-head latent does not reproduce initializer logits")
                    row["terminal_head_only_latent"] = captured[0]
                    rows += 1
    finally:
        handle.remove()
    return rows


def _joint_log_probability(model: Any, decision: Any) -> torch.Tensor:
    weight = model.policy_head.weight.reshape(-1)
    bias = model.policy_head.bias.detach().reshape(())
    terms = []
    for row in decision.rows:
        latent = row["terminal_head_only_latent"]
        logits = latent.mv(weight) + bias
        terms.append(torch.log_softmax(logits, dim=0)[int(row["selected_index"])])
    return torch.stack(terms).sum()


def _fit(model: Any, decisions: list[Any]) -> list[dict[str, Any]]:
    for parameter in model.parameters():
        parameter.requires_grad_(False)
    model.policy_head.weight.requires_grad_(True)
    parameters = [model.policy_head.weight]
    statistics = pipeline.outcome._advantage_statistics(decisions)
    pipeline.outcome._install_standardized_advantages(decisions, statistics)
    optimizer = torch.optim.AdamW(
        parameters, lr=pipeline.LR, weight_decay=pipeline.WEIGHT_DECAY
    )
    rng = random.Random(FIT_SEED)
    episode_mass = sum(decision.episode_weight for decision in decisions)
    weights = {
        decision.key: decision.episode_weight * len(decisions) / episode_mass
        for decision in decisions
    }
    history: list[dict[str, Any]] = []
    for epoch in range(pipeline.FIT_EPOCHS):
        order = list(range(len(decisions)))
        rng.shuffle(order)
        loss_total = 0.0
        clip_total = 0.0
        gradient_norm_max = 0.0
        steps = 0
        for start in range(0, len(order), pipeline.BATCH_SIZE):
            batch = [
                decisions[index]
                for index in order[start : start + pipeline.BATCH_SIZE]
            ]
            surrogates = []
            masses = []
            clipped = 0
            for decision in batch:
                joint = _joint_log_probability(model, decision)
                log_ratio = joint - decision.old_joint_log_probability
                ratio = torch.exp(log_ratio)
                clipped_ratio = torch.clamp(
                    ratio, 1.0 - pipeline.CLIP, 1.0 + pipeline.CLIP
                )
                advantage = decision.standardized_advantage
                surrogates.append(
                    torch.minimum(ratio * advantage, clipped_ratio * advantage)
                )
                masses.append(weights[decision.key])
                clipped += int(
                    abs(float(log_ratio.detach())) > math.log1p(pipeline.CLIP)
                )
            mass_tensor = torch.tensor(masses, dtype=torch.float32)
            loss = -(torch.stack(surrogates) * mass_tensor).sum() / mass_tensor.sum()
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient_norm = torch.nn.utils.clip_grad_norm_(
                parameters, pipeline.GRAD_CAP
            )
            if not torch.isfinite(gradient_norm):
                _fail("non-finite head-only gradient")
            optimizer.step()
            loss_total += float(loss.detach())
            clip_total += clipped / len(batch)
            gradient_norm_max = max(gradient_norm_max, float(gradient_norm))
            steps += 1
        history.append(
            {
                "epoch": epoch + 1,
                "mean_minibatch_loss": loss_total / max(steps, 1),
                "mean_minibatch_clip_fraction": clip_total / max(steps, 1),
                "maximum_preclip_gradient_norm": gradient_norm_max,
                "optimizer_steps": steps,
            }
        )
    return history


def _publish(
    args: argparse.Namespace,
    model: Any,
    source: dict[str, Any],
    movement: dict[str, Any],
) -> dict[str, Any]:
    if args.output_root.exists():
        _fail("head-only candidate output root already exists")
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
        },
        "config": {
            "architecture": ARCHITECTURE,
            "value_model": pipeline.VALUE_MODEL,
            "seed": FIT_SEED,
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
            "trainable_parameter": TRAINABLE_PARAMETER,
            "trainable_parameter_count": model.policy_head.weight.numel(),
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


def fit(args: argparse.Namespace) -> dict[str, Any]:
    for path in (args.output, args.output_root, args.output_state, args.output_parity):
        if path.exists():
            _fail(f"head-only output already exists: {path}")
    if (
        pipeline._sha256(args.cache) != SOURCE_CACHE_SHA256
        or pipeline._sha256(args.initializer_state)
        != pipeline.INITIALIZER_STATE_SHA256
    ):
        _fail("head-only source identity mismatch")
    started = time.perf_counter()
    decisions, source, timings = pipeline._load_decisions(args.cache, None)
    if (
        source["pair_count"] != pipeline.FORMAL_PAIRS
        or source["base_seed"] != pipeline.FORMAL_BASE_SEED
    ):
        _fail("head-only cache panel mismatch")
    state_payload = torch.load(
        args.initializer_state, map_location="cpu", weights_only=False
    )
    if state_payload.get("schema") != pipeline.initializer.MODEL_STATE_SCHEMA:
        _fail("head-only initializer model-state schema mismatch")
    pipeline.screen._configure(FIT_SEED, args.threads)
    model = pipeline.distill._model()
    model.load_state_dict(state_payload["model_state_dict"], strict=True)
    initial_value_bits = pipeline.initializer._value_head_bits(model)
    frozen_before = _frozen_state(model)
    alignment = pipeline._alignment(model, decisions)
    if not alignment["pass"]:
        _fail("head-only initializer no longer matches behavior logits")
    aligned = time.perf_counter()
    latent_rows = _attach_policy_latents(model, decisions)
    attached = time.perf_counter()
    training_history = _fit(model, decisions)
    trained = time.perf_counter()
    if _frozen_state(model) != frozen_before:
        _fail("head-only fit changed a frozen tensor")
    if pipeline.initializer._value_head_bits(model) != initial_value_bits:
        _fail("head-only fit changed the frozen value head")
    movement = pipeline._movement(model, decisions)
    gate = pipeline._fit_gate(movement)
    measured = time.perf_counter()
    result: dict[str, Any] = {
        "schema": FIT_SCHEMA,
        "decision": gate["decision"],
        "source": source,
        "config": {
            "threads": args.threads,
            "seed": FIT_SEED,
            "epochs": pipeline.FIT_EPOCHS,
            "batch_size_physical_decisions": pipeline.BATCH_SIZE,
            "learning_rate": pipeline.LR,
            "weight_decay": pipeline.WEIGHT_DECAY,
            "ppo_clip": pipeline.CLIP,
            "gradient_norm_cap": pipeline.GRAD_CAP,
            "trainable_parameter": TRAINABLE_PARAMETER,
            "trainable_parameter_count": model.policy_head.weight.numel(),
        },
        "initializer_alignment": alignment,
        "latent_rows": latent_rows,
        "advantage_statistics_by_candidate_seat": (
            pipeline.outcome._advantage_statistics(decisions)
        ),
        "training_history": training_history,
        "movement": movement,
        "fit_gate": gate,
        "phase_runtime_seconds": {
            **timings,
            "alignment_seconds": aligned - (started + sum(timings.values())),
            "latent_precompute_seconds": attached - aligned,
            "train_seconds": trained - attached,
            "movement_seconds": measured - trained,
        },
        "runtime_seconds": measured - started,
    }
    pipeline.screen._atomic_torch_save(
        {
            "schema": MODEL_STATE_SCHEMA,
            "source": source,
            "config": result["config"],
            "training_history": training_history,
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
    parser.add_argument("--parent-outcome-root", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--output-state", type=Path, required=True)
    parser.add_argument("--output-parity", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--threads", type=int, choices=pipeline.THREAD_CHOICES, required=True)
    print(json.dumps(fit(parser.parse_args()), sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
