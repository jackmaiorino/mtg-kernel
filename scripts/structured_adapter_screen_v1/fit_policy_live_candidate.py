#!/usr/bin/env python3
"""Fit one policy-only structured residual and publish a strict live package."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import random
import shutil
import struct
import time
from pathlib import Path
from typing import Any

import torch

import run_screen as screen


SCHEMA = "mtg-kernel-structured-policy-residual-candidate/v1"
REPORT_SCHEMA = "mtg-kernel-structured-policy-residual-fit/v1"
COMPOSITE_DOMAIN = b"mtg-kernel-structured-policy-residual-composite-model/v1"
PARENT_MANIFEST_SHA256 = "706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb"
PARENT_PAYLOAD_SHA256 = "eb83be33bcb7418b6f85ec9687da4b7ca5620a1df64721a1942d2793588bbd3c"
PARENT_NATIVE_STATE_SHA256 = "2c55a13abb3157f3f4ba012af663ffa56599c5d6cb90743c1ba6e024ca47a9c8"
PARENT_MODEL_PARAMETER_SHA256 = "883e4882d01d9cb55ecd7a4ae00e3c95793b6147baf3df08650ef1fa7f8e9546"
PARENT_ADAM_STEP = 1
PARAMETER_NAMES = (
    "state.0.weight",
    "state.0.bias",
    "state.2.weight",
    "state.2.bias",
    "object.0.weight",
    "object.0.bias",
    "card.weight",
    "group.weight",
    "edge.0.weight",
    "edge.0.bias",
    "edge.2.weight",
    "edge.2.bias",
    "group_mix.weight",
    "action.0.weight",
    "action.0.bias",
    "ref.0.weight",
    "ref.0.bias",
    "query.weight",
    "query.bias",
    "combine.0.weight",
    "combine.0.bias",
    "combine.2.weight",
    "combine.2.bias",
    "policy_head.weight",
    "policy_head.bias",
)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _json_bytes(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + "\n").encode("utf-8")


def _f32_from_bits(bits: list[int]) -> torch.Tensor:
    raw = b"".join(struct.pack("<I", int(value)) for value in bits)
    return torch.frombuffer(bytearray(raw), dtype=torch.float32).clone()


def _install_retained_parent_logits(
    examples: list[dict[str, Any]], parent_logits_path: Path
) -> dict[str, Any]:
    document = json.loads(parent_logits_path.read_text(encoding="utf-8"))
    if (
        document.get("schema")
        != "mtg-kernel-structured-residual-retained-parent-logits/v1"
        or document.get("retained_parent_manifest_sha256") != PARENT_MANIFEST_SHA256
        or document.get("retained_parent_payload_sha256") != PARENT_PAYLOAD_SHA256
        or document.get("retained_parent_native_state_sha256") != PARENT_NATIVE_STATE_SHA256
        or document.get("retained_parent_model_parameter_sha256")
        != PARENT_MODEL_PARAMETER_SHA256
        or document.get("retained_parent_adam_step") != PARENT_ADAM_STEP
    ):
        raise ValueError("retained parent-logit identity mismatch")
    rows = document.get("rows")
    if not isinstance(rows, list) or len(rows) != len(examples):
        raise ValueError("retained parent-logit row count mismatch")
    for index, (example, row) in enumerate(zip(examples, rows)):
        if row.get("teacher_decision_ordinal") != index:
            raise ValueError("retained parent-logit ordinals are not exact")
        exported = _f32_from_bits(row["exported_g384_logits_f32_bits"])
        retained = _f32_from_bits(row["retained_parent_logits_f32_bits"])
        if (
            exported.shape != example["old_logits"].shape
            or retained.shape != example["old_logits"].shape
            or not torch.equal(exported, example["old_logits"])
            or row.get("legal_action_count") != retained.numel()
        ):
            raise ValueError(f"retained parent-logit row {index} does not bind the cache")
        example["old_logits"] = retained
        example["old_value"] = _f32_from_bits(
            [row["retained_parent_value_f32_bits"]]
        )[0]
    return {
        "path": str(parent_logits_path),
        "sha256": _sha256(parent_logits_path),
        "row_count": len(rows),
        "teacher_jsonl_sha256": document.get("teacher_jsonl_sha256"),
    }


def _softmax(logits: torch.Tensor) -> torch.Tensor:
    return torch.softmax(logits.double(), dim=0)


def _movement(
    parents: list[torch.Tensor],
    residuals: list[torch.Tensor],
    weights: list[float],
    scale: float,
) -> dict[str, float | int]:
    televisions: list[float] = []
    kls: list[float] = []
    changed = 0
    for parent, residual in zip(parents, residuals):
        candidate = parent.double() + residual.double() * scale
        parent_probability = _softmax(parent)
        candidate_probability = _softmax(candidate)
        televisions.append(float(0.5 * (parent_probability - candidate_probability).abs().sum()))
        kls.append(
            float(
                (
                    parent_probability
                    * (parent_probability.clamp_min(1e-300).log() - candidate_probability.clamp_min(1e-300).log())
                ).sum()
            )
        )
        changed += int(int(parent.argmax()) != int(candidate.argmax()))
    denominator = max(sum(weights), 1e-12)
    weighted_tv = sum(value * weight for value, weight in zip(televisions, weights)) / denominator
    weighted_kl = sum(value * weight for value, weight in zip(kls, weights)) / denominator
    ordered = sorted(televisions)
    p90_index = min(len(ordered) - 1, math.ceil(0.90 * len(ordered)) - 1)
    return {
        "mean_total_variation": weighted_tv,
        "p90_total_variation": ordered[p90_index],
        "mean_parent_to_candidate_kl": weighted_kl,
        "argmax_changes": changed,
        "example_count": len(parents),
    }


def _calibrate(
    parents: list[torch.Tensor],
    residuals: list[torch.Tensor],
    weights: list[float],
    target: float,
) -> tuple[float, dict[str, float | int]]:
    full = _movement(parents, residuals, weights, 1.0)
    if float(full["mean_total_variation"]) <= target:
        return 1.0, full
    low = 0.0
    high = 1.0
    for _ in range(48):
        midpoint = (low + high) / 2.0
        observed = _movement(parents, residuals, weights, midpoint)
        if float(observed["mean_total_variation"]) < target:
            low = midpoint
        else:
            high = midpoint
    return high, _movement(parents, residuals, weights, high)


def _policy_metrics(
    examples: list[dict[str, Any]],
    parents: list[torch.Tensor],
    residuals: list[torch.Tensor],
    scale: float,
) -> dict[str, Any]:
    rows: list[tuple[int, float, float, float, bool, bool]] = []
    for example, parent, residual in zip(examples, parents, residuals):
        candidate = parent.double() + residual.double() * scale
        label = int(example["selected_index"])
        weight = screen._policy_weight(example)
        rows.append(
            (
                int(example["acting_seat"]),
                float(-torch.log_softmax(parent.double(), dim=0)[label]),
                float(-torch.log_softmax(candidate, dim=0)[label]),
                weight,
                int(parent.argmax()) == label,
                int(candidate.argmax()) == label,
            )
        )

    def summarize(subset: list[tuple[int, float, float, float, bool, bool]]) -> dict[str, float | int]:
        weight = max(sum(row[3] for row in subset), 1e-12)
        parent_nll = sum(row[1] * row[3] for row in subset) / weight
        candidate_nll = sum(row[2] * row[3] for row in subset) / weight
        return {
            "parent_nll": parent_nll,
            "candidate_nll": candidate_nll,
            "relative_nll_improvement": (parent_nll - candidate_nll) / max(parent_nll, 1e-12),
            "parent_top1": sum(float(row[4]) * row[3] for row in subset) / weight,
            "candidate_top1": sum(float(row[5]) * row[3] for row in subset) / weight,
            "weight": weight,
            "count": len(subset),
        }

    return {
        "overall": summarize(rows),
        "by_acting_seat": {
            str(seat): summarize([row for row in rows if row[0] == seat]) for seat in (0, 1)
        },
    }


def _fit(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    cache = torch.load(args.cache, map_location="cpu", weights_only=False)
    if cache.get("version") != screen.SCRIPT_VERSION:
        raise ValueError("cache version mismatch")
    examples = cache["policy"]
    if not examples:
        raise ValueError("policy cache is empty")
    parent_logits = _install_retained_parent_logits(examples, args.parent_logits_json)
    card_vocab, group_vocab = screen._model_vocab(examples)
    screen._configure(args.seed, args.threads)
    model = screen.StructuredAdapter(card_vocab, group_vocab, args.dim)
    trainable = [parameter for name, parameter in model.named_parameters() if not name.startswith("value_head.")]
    optimizer = torch.optim.AdamW(trainable, lr=args.lr, weight_decay=args.weight_decay)
    rng = random.Random(args.seed)
    history: list[dict[str, float | int]] = []
    for epoch in range(args.epochs):
        order = list(range(len(examples)))
        rng.shuffle(order)
        loss_sum = 0.0
        steps = math.ceil(len(order) / args.batch_size)
        model.train()
        for step in range(steps):
            indices = order[step * args.batch_size : (step + 1) * args.batch_size]
            batch = [examples[index] for index in indices]
            policy_loss, _ = screen._batch_loss(model, batch, [])
            optimizer.zero_grad(set_to_none=True)
            policy_loss.backward()
            torch.nn.utils.clip_grad_norm_(trainable, 5.0)
            optimizer.step()
            loss_sum += float(policy_loss.detach())
        history.append({"epoch": epoch + 1, "policy_nll": loss_sum / steps})

    model.eval()
    parents: list[torch.Tensor] = []
    residuals: list[torch.Tensor] = []
    weights = [screen._policy_weight(example) for example in examples]
    with torch.no_grad():
        for example in examples:
            residual, _ = model._one(example)
            parents.append(example["old_logits"].detach().clone())
            residuals.append(residual.detach().clone())
    full_movement = _movement(parents, residuals, weights, 1.0)
    scale, movement = _calibrate(parents, residuals, weights, args.target_mean_tv)
    metrics = _policy_metrics(examples, parents, residuals, scale)
    with torch.no_grad():
        model.policy_head.weight.mul_(scale)
        model.policy_head.bias.mul_(scale)

    output_root: Path = args.output_root
    output_root.mkdir(parents=True, exist_ok=False)
    parent_output = output_root / "parent"
    parent_output.mkdir()
    parent_manifest = args.parent_outcome_root / "checkpoint.json"
    parent_payload = args.parent_outcome_root / "checkpoint.state.f32le"
    if _sha256(parent_manifest) != PARENT_MANIFEST_SHA256 or _sha256(parent_payload) != PARENT_PAYLOAD_SHA256:
        raise ValueError("parent root is not the exact retained 706b checkpoint")
    shutil.copyfile(parent_manifest, parent_output / parent_manifest.name)
    shutil.copyfile(parent_payload, parent_output / parent_payload.name)

    state = model.state_dict()
    payload = bytearray()
    parameters: list[dict[str, Any]] = []
    offset = 0
    for name in PARAMETER_NAMES:
        tensor = state[name].detach().cpu().contiguous().float()
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
    weights_path = output_root / "weights.f32le"
    weights_path.write_bytes(payload)
    weights_sha = _sha256(weights_path)
    composite = hashlib.sha256(
        COMPOSITE_DOMAIN + bytes.fromhex(PARENT_MODEL_PARAMETER_SHA256) + bytes(payload)
    ).hexdigest()
    report = {
        "schema": REPORT_SCHEMA,
        "source": {
            "cache": str(args.cache),
            "cache_sha256": _sha256(args.cache),
            "teacher_sha256": cache.get("source", {}).get("teacher_sha256"),
            "outcome_sha256": cache.get("source", {}).get("outcome_sha256"),
            "policy_examples": len(examples),
            "retained_parent_logits": parent_logits,
        },
        "config": {
            "architecture": "stateless-structured-object-action-attention-policy-residual/v1",
            "dim": args.dim,
            "card_vocab": card_vocab,
            "group_vocab": group_vocab,
            "epochs": args.epochs,
            "batch_size": args.batch_size,
            "learning_rate": args.lr,
            "weight_decay": args.weight_decay,
            "seed": args.seed,
            "threads": args.threads,
            "value_model": "exact-retained-parent-unchanged",
            "target_mean_total_variation": args.target_mean_tv,
        },
        "training_history": history,
        "uncalibrated_movement": full_movement,
        "calibration_scale": scale,
        "calibrated_movement": movement,
        "policy_metrics": metrics,
        "weights_sha256": weights_sha,
        "composite_model_parameter_sha256": composite,
        "runtime_seconds": time.perf_counter() - started,
        "non_claims": [
            "development teacher corpus reused",
            "no value improvement claim",
            "no live strength evidence",
            "no promotion or pro-level claim",
        ],
    }
    report_bytes = _json_bytes(report)
    report_path = output_root / "report.json"
    report_path.write_bytes(report_bytes)
    report_sha = _sha256(report_path)
    candidate = {
        "schema": SCHEMA,
        "publication_encoding": "json-pretty-sorted-utf8-trailing-lf/v1",
        "parent": {
            "directory": "parent",
            "manifest_sha256": PARENT_MANIFEST_SHA256,
            "payload_sha256": PARENT_PAYLOAD_SHA256,
            "native_state_sha256": PARENT_NATIVE_STATE_SHA256,
            "model_parameter_sha256": PARENT_MODEL_PARAMETER_SHA256,
            "adam_step": PARENT_ADAM_STEP,
        },
        "architecture": {
            "identity": "stateless-structured-object-action-attention-policy-residual/v1",
            "state_dim": screen.STATE_DIM,
            "object_dim": screen.OBJECT_DIM,
            "edge_dim": screen.EDGE_DIM,
            "action_dim": screen.ACTION_DIM,
            "ref_dim": screen.REF_DIM,
            "hidden_dim": args.dim,
            "card_vocab": card_vocab,
            "card_embedding_dim": max(8, args.dim // 2),
            "group_vocab": group_vocab,
            "group_embedding_dim": max(8, args.dim // 3),
            "value_model": "exact-parent-unchanged",
        },
        "weights": {
            "filename": weights_path.name,
            "encoding": "ordered-row-major-finite-f32-little-endian/v1",
            "sha256": weights_sha,
            "byte_count": len(payload),
            "parameter_count": offset,
            "parameters": parameters,
        },
        "report": {"filename": report_path.name, "sha256": report_sha},
        "composite_model_parameter_sha256": composite,
    }
    candidate_path = output_root / "structured_candidate.json"
    candidate_path.write_bytes(_json_bytes(candidate))
    return {
        "candidate_root": str(output_root),
        "candidate_json_sha256": _sha256(candidate_path),
        "weights_sha256": weights_sha,
        "report_sha256": report_sha,
        "composite_model_parameter_sha256": composite,
        "calibration_scale": scale,
        "movement": movement,
        "policy_metrics": metrics,
        "runtime_seconds": time.perf_counter() - started,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--parent-outcome-root", type=Path, required=True)
    parser.add_argument("--parent-logits-json", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--dim", type=int, default=48)
    parser.add_argument("--epochs", type=int, default=20)
    parser.add_argument("--batch-size", type=int, default=32)
    parser.add_argument("--lr", type=float, default=3e-4)
    parser.add_argument("--weight-decay", type=float, default=1e-4)
    parser.add_argument("--target-mean-tv", type=float, default=0.02)
    parser.add_argument("--seed", type=int, default=20260802)
    parser.add_argument("--threads", type=int, default=12)
    args = parser.parse_args()
    if (
        args.dim != 48
        or args.epochs < 1
        or args.batch_size < 1
        or args.lr <= 0
        or args.weight_decay < 0
        or not 0 < args.target_mean_tv <= 0.05
        or args.threads < 1
    ):
        raise ValueError("invalid fixed candidate configuration")
    result = _fit(args)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
