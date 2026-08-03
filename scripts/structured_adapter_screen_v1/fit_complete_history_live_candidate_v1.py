#!/usr/bin/env python3
"""Fit and publish the fixed full-corpus complete-history live candidate."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import random
import shutil
import time
from pathlib import Path
from typing import Any

import torch

import fit_policy_live_candidate as live
import run_screen as screen


SCHEMA = "mtg-kernel-structured-history-policy-value-residual-candidate/v1"
REPORT_SCHEMA = "mtg-kernel-structured-history-policy-value-residual-fit/v1"
ARCHITECTURE = (
    "complete-public-history-structured-object-action-attention-policy-value-residual/v1"
)
VALUE_MODEL = "joint-terminal-residual/v1"
COMPOSITE_DOMAIN = (
    b"mtg-kernel-structured-history-policy-value-residual-composite-model/v1"
)
EXPECTED_CACHE_SHA256 = "721aeeb8389464676edf1190b4e90d74ced286104cc0fb30deb46d36ffbc8090"
DIM = 48
CARD_VOCAB = 136
GROUP_VOCAB = 12
HISTORY_LENGTH = 16
HISTORY_ROLE_DIM = 2
HISTORY_FEATURE_DIM = screen.ACTION_EXPLICIT_DIM + HISTORY_ROLE_DIM + CARD_VOCAB
EPOCHS = 5
BATCH_SIZE = 64
LR = 3.0e-4
WEIGHT_DECAY = 1.0e-4
SEED = 20_260_802
METRIC_SAMPLE_SIZE = 8_192
EXPECTED_PARAMETER_COUNT = 107_378
PARITY_SCHEMA = "mtg-kernel-structured-history-residual-parity-fixture/v1"


def _sha256(path: Path) -> str:
    return live._sha256(path)


def _json_bytes(value: Any) -> bytes:
    return live._json_bytes(value)


def _parity_fixture(
    model: screen.StructuredAdapter,
    policy: list[dict[str, Any]],
    value: list[dict[str, Any]],
) -> dict[str, Any]:
    buckets: dict[int, tuple[str, dict[str, Any]]] = {}
    for lane, examples in (("policy", policy), ("value", value)):
        for example in examples:
            length = int(example["history_features"].shape[0])
            bucket = 0 if length == 0 else 1 if length <= 3 else 4 if length <= 7 else 8 if length <= 15 else 16
            buckets.setdefault(bucket, (lane, example))
            if len(buckets) == 5:
                break
        if len(buckets) == 5:
            break
    if set(buckets) != {0, 1, 4, 8, 16}:
        raise ValueError("parity fixture lacks required history-length coverage")
    rows: list[dict[str, Any]] = []
    model.eval()
    with torch.no_grad():
        for bucket in sorted(buckets):
            lane, example = buckets[bucket]
            residual_logits, residual_value = model._one(example)
            history = example["history_features"]
            history_rows = []
            for row in history:
                self_role = float(row[screen.ACTION_EXPLICIT_DIM])
                opponent_role = float(row[screen.ACTION_EXPLICIT_DIM + 1])
                if (self_role, opponent_role) not in ((1.0, 0.0), (0.0, 1.0)):
                    raise ValueError("parity history role is not one-hot")
                history_rows.append(
                    {
                        "acting_player": 0 if self_role == 1.0 else 1,
                        "action_explicit_features": row[
                            : screen.ACTION_EXPLICIT_DIM
                        ].tolist(),
                        "public_card_histogram": row[
                            screen.ACTION_EXPLICIT_DIM + 2 :
                        ].tolist(),
                    }
                )
            rows.append(
                {
                    "lane": lane,
                    "history_length_bucket": bucket,
                    "acting_player": 0,
                    "tensor": {
                        "state": example["state"].tolist(),
                        "object_features": example["object_features"].tolist(),
                        "object_card_ids": example["object_card_ids"].tolist(),
                        "object_groups": example["object_groups"].tolist(),
                        "object_node_ids": [],
                        "edge_features": example["edge_features"].tolist(),
                        "edge_source_indices": example["edge_src"].tolist(),
                        "edge_target_indices": example["edge_tgt"].tolist(),
                        "action_features": example["action_features"].tolist(),
                        "action_ref_features": example["action_ref_features"].tolist(),
                        "action_ref_card_ids": example["ref_card_ids"].tolist(),
                        "action_ref_action_indices": example["ref_action_indices"].tolist(),
                        "action_ref_node_indices": example["ref_node_indices"].tolist(),
                    },
                    "history": history_rows,
                    "expected_residual_logits": residual_logits.tolist(),
                    "expected_residual_value": float(residual_value),
                }
            )
    return {"schema": PARITY_SCHEMA, "acting_player_convention": "fixture-current-actor-is-zero/v1", "examples": rows}


def _fit(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    cache_sha256 = _sha256(args.cache)
    if cache_sha256 != EXPECTED_CACHE_SHA256:
        raise ValueError("complete-history cache SHA-256 mismatch")
    cache = torch.load(args.cache, map_location="cpu", weights_only=False)
    loaded = time.perf_counter()
    if cache.get("version") != screen.SCRIPT_VERSION:
        raise ValueError("cache version mismatch")
    policy = cache.get("policy")
    value = cache.get("value")
    if not isinstance(policy, list) or not policy or not isinstance(value, list) or not value:
        raise ValueError("policy and value caches must both be nonempty")
    card_vocab, group_vocab = screen._model_vocab(policy + value)
    if card_vocab != CARD_VOCAB or group_vocab != GROUP_VOCAB:
        raise ValueError("fixed model vocabulary mismatch")

    screen._attach_complete_action_history(
        policy, value, HISTORY_LENGTH, CARD_VOCAB
    )
    screen._assign_episode_weights(value)
    history_ready = time.perf_counter()
    screen._configure(SEED, args.threads)
    model = screen.StructuredAdapter(
        CARD_VOCAB,
        GROUP_VOCAB,
        DIM,
        HISTORY_LENGTH,
        HISTORY_FEATURE_DIM,
    )
    optimizer = torch.optim.AdamW(model.parameters(), lr=LR, weight_decay=WEIGHT_DECAY)
    rng = random.Random(SEED)
    steps_per_epoch = max(
        math.ceil(len(policy) / BATCH_SIZE), math.ceil(len(value) / BATCH_SIZE)
    )
    training_history: list[dict[str, float | int]] = []
    for epoch in range(EPOCHS):
        model.train()
        policy_order = list(range(len(policy)))
        value_order = list(range(len(value)))
        rng.shuffle(policy_order)
        rng.shuffle(value_order)
        policy_loss_total = 0.0
        value_loss_total = 0.0
        for step in range(steps_per_epoch):
            policy_batch = [
                policy[policy_order[(step * BATCH_SIZE + index) % len(policy_order)]]
                for index in range(min(BATCH_SIZE, len(policy_order)))
            ]
            value_batch = [
                value[value_order[(step * BATCH_SIZE + index) % len(value_order)]]
                for index in range(min(BATCH_SIZE, len(value_order)))
            ]
            policy_loss, value_loss = screen._batch_loss(
                model, policy_batch, value_batch
            )
            optimizer.zero_grad(set_to_none=True)
            (policy_loss + value_loss).backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 5.0)
            optimizer.step()
            policy_loss_total += float(policy_loss.detach())
            value_loss_total += float(value_loss.detach())
        training_history.append(
            {
                "epoch": epoch + 1,
                "policy_nll": policy_loss_total / steps_per_epoch,
                "value_mse": value_loss_total / steps_per_epoch,
            }
        )
    trained = time.perf_counter()

    state_path = Path(str(args.output_root) + ".state.pt")
    if state_path.exists():
        raise ValueError("model state output already exists")
    screen._atomic_torch_save(
        {
            "schema": REPORT_SCHEMA + ".model-state",
            "cache_sha256": cache_sha256,
            "config": {
                "architecture": ARCHITECTURE,
                "dim": DIM,
                "card_vocab": CARD_VOCAB,
                "group_vocab": GROUP_VOCAB,
                "history_length": HISTORY_LENGTH,
                "history_feature_dim": HISTORY_FEATURE_DIM,
                "epochs": EPOCHS,
                "batch_size": BATCH_SIZE,
                "learning_rate": LR,
                "weight_decay": WEIGHT_DECAY,
                "seed": SEED,
                "threads": args.threads,
            },
            "training_history": training_history,
            "model_state_dict": model.state_dict(),
        },
        state_path,
    )
    state_sha256 = _sha256(state_path)
    parity_path = Path(str(args.output_root) + ".parity.json")
    if parity_path.exists():
        raise ValueError("parity fixture output already exists")
    parity_path.write_bytes(_json_bytes(_parity_fixture(model, policy, value)))
    parity_sha256 = _sha256(parity_path)
    checkpointed = time.perf_counter()

    policy_sample = screen._deterministic_sample(
        policy, METRIC_SAMPLE_SIZE, SEED + 101
    )
    value_sample = screen._deterministic_sample(
        value, METRIC_SAMPLE_SIZE, SEED + 102
    )
    model.eval()
    parents: list[torch.Tensor] = []
    residuals: list[torch.Tensor] = []
    weights: list[float] = []
    with torch.no_grad():
        for example in policy_sample:
            residual, _ = model._one(example)
            parents.append(example["old_logits"].detach().clone())
            residuals.append(residual.detach().clone())
            weights.append(screen._policy_weight(example))
    movement = live._movement(parents, residuals, weights, 1.0)
    policy_metrics = live._policy_metrics(
        policy_sample, parents, residuals, 1.0
    )
    value_metrics = screen._summarize_value(
        screen._metric_sums(model, value_sample, "value")["records"]
    )
    measured = time.perf_counter()

    args.output_root.mkdir(parents=True, exist_ok=False)
    parent_output = args.output_root / "parent"
    parent_output.mkdir()
    parent_manifest = args.parent_outcome_root / "checkpoint.json"
    parent_payload = args.parent_outcome_root / "checkpoint.state.f32le"
    if (
        _sha256(parent_manifest) != live.PARENT_MANIFEST_SHA256
        or _sha256(parent_payload) != live.PARENT_PAYLOAD_SHA256
    ):
        raise ValueError("parent root is not the exact retained 706b checkpoint")
    shutil.copyfile(parent_manifest, parent_output / parent_manifest.name)
    shutil.copyfile(parent_payload, parent_output / parent_payload.name)

    state = model.state_dict()
    payload = bytearray()
    parameters: list[dict[str, Any]] = []
    offset = 0
    for name, state_tensor in state.items():
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
    if offset != EXPECTED_PARAMETER_COUNT:
        raise ValueError("fixed history model parameter count mismatch")
    weights_path = args.output_root / "weights.f32le"
    weights_path.write_bytes(payload)
    weights_sha256 = _sha256(weights_path)
    composite_sha256 = hashlib.sha256(
        COMPOSITE_DOMAIN
        + bytes.fromhex(live.PARENT_MODEL_PARAMETER_SHA256)
        + bytes(payload)
    ).hexdigest()
    report = {
        "schema": REPORT_SCHEMA,
        "source": {
            "cache": str(args.cache),
            "cache_sha256": cache_sha256,
            "teacher_sha256": cache.get("source", {}).get("teacher_sha256"),
            "outcome_sha256": cache.get("source", {}).get("outcome_sha256"),
            "policy_examples": len(policy),
            "value_examples": len(value),
            "model_state": {"path": str(state_path), "sha256": state_sha256},
            "parity_fixture": {"path": str(parity_path), "sha256": parity_sha256},
        },
        "config": {
            "architecture": ARCHITECTURE,
            "dim": DIM,
            "card_vocab": CARD_VOCAB,
            "group_vocab": GROUP_VOCAB,
            "history_length": HISTORY_LENGTH,
            "history_feature_dim": HISTORY_FEATURE_DIM,
            "epochs": EPOCHS,
            "batch_size": BATCH_SIZE,
            "learning_rate": LR,
            "weight_decay": WEIGHT_DECAY,
            "seed": SEED,
            "threads": args.threads,
            "value_model": VALUE_MODEL,
            "residual_scale": 1.0,
            "metric_sample_size_per_lane": METRIC_SAMPLE_SIZE,
        },
        "training_history": training_history,
        "calibrated_movement": movement,
        "policy_metrics": policy_metrics,
        "value_metrics": value_metrics,
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
        "runtime_seconds": time.perf_counter() - started,
        "phase_runtime_seconds": {
            "load_cache": loaded - started,
            "attach_history": history_ready - loaded,
            "train": trained - history_ready,
            "checkpoint": checkpointed - trained,
            "bounded_metrics": measured - checkpointed,
        },
        "non_claims": [
            "development corpus reused for the full fit",
            "bounded metrics are descriptive, not held-out evidence",
            "no live strength evidence",
            "no promotion or pro-level claim",
        ],
    }
    report_path = args.output_root / "report.json"
    report_path.write_bytes(_json_bytes(report))
    report_sha256 = _sha256(report_path)
    candidate = {
        "schema": SCHEMA,
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
            "identity": ARCHITECTURE,
            "state_dim": screen.STATE_DIM,
            "object_dim": screen.OBJECT_DIM,
            "edge_dim": screen.EDGE_DIM,
            "action_dim": screen.ACTION_DIM,
            "ref_dim": screen.REF_DIM,
            "hidden_dim": DIM,
            "card_vocab": CARD_VOCAB,
            "card_embedding_dim": max(8, DIM // 2),
            "group_vocab": GROUP_VOCAB,
            "group_embedding_dim": max(8, DIM // 3),
            "history_length": HISTORY_LENGTH,
            "history_feature_dim": HISTORY_FEATURE_DIM,
            "history_role_dim": HISTORY_ROLE_DIM,
            "value_model": VALUE_MODEL,
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
    candidate_path.write_bytes(_json_bytes(candidate))
    return {
        "candidate_root": str(args.output_root),
        "candidate_json_sha256": _sha256(candidate_path),
        "weights_sha256": weights_sha256,
        "report_sha256": report_sha256,
        "composite_model_parameter_sha256": composite_sha256,
        "model_state_sha256": state_sha256,
        "parity_fixture_sha256": parity_sha256,
        "movement": movement,
        "policy_metrics": policy_metrics,
        "value_metrics": value_metrics,
        "runtime_seconds": time.perf_counter() - started,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--parent-outcome-root", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--threads", type=int, default=12)
    args = parser.parse_args()
    if args.threads < 1:
        raise ValueError("threads must be positive")
    result = _fit(args)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
