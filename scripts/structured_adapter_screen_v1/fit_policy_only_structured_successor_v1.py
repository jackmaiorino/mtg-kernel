#!/usr/bin/env python3
"""Fit and publish the policy-only complete-history structured successor."""

from __future__ import annotations

import argparse
import hashlib
import json
import random
import shutil
import time
from pathlib import Path
from typing import Any

import torch

import fit_complete_history_live_candidate_v1 as history_publish
import fit_policy_live_candidate as live
import run_screen as screen
import run_structured_successor_distillation_v1 as distill


SCHEMA = "mtg-kernel-structured-policy-successor-candidate/v1"
REPORT_SCHEMA = "mtg-kernel-structured-policy-successor-fit/v1"
MODEL_STATE_SCHEMA = REPORT_SCHEMA + ".model-state"
PARITY_SCHEMA = "mtg-kernel-structured-policy-successor-parity-fixture/v1"
ARCHITECTURE = (
    "complete-public-history-structured-policy-successor-frozen-parent-value/v1"
)
VALUE_MODEL = "exact-retained-parent-frozen/v1"
COMPOSITE_DOMAIN = b"mtg-kernel-structured-policy-successor-composite-model/v1"
CANDIDATE_FILENAME = "structured_policy_successor.json"
EXPECTED_THREADS = 12
SEED = 20_260_804
EPOCHS = 5
BATCH_SIZE = 64
LR = 3.0e-4
WEIGHT_DECAY = 1.0e-4
GRAD_CAP = 5.0
MEAN_TV_LIMIT = 0.015
P90_TV_LIMIT = 0.040
TOP_ACTION_FLOOR = 0.990
PROVISIONAL_TRANSPORT_MAX_ERROR = 1.0
PROVISIONAL_PARENT_VALUE_BIT_EXACT = False


def _fail(message: str) -> None:
    raise ValueError(message)


def _write_new(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(history_publish._json_bytes(value))


def _policy_parameters(model: screen.StructuredAdapter) -> list[torch.nn.Parameter]:
    parameters: list[torch.nn.Parameter] = []
    for name, parameter in model.named_parameters():
        if name.startswith("value_head."):
            parameter.requires_grad_(False)
        else:
            parameters.append(parameter)
    if not parameters:
        _fail("policy parameter set is empty")
    return parameters


def _value_head_bits(model: screen.StructuredAdapter) -> bytes:
    payload = bytearray()
    for name, tensor in model.state_dict().items():
        if name.startswith("value_head."):
            payload.extend(
                tensor.detach()
                .cpu()
                .contiguous()
                .float()
                .numpy()
                .astype("<f4", copy=False)
                .tobytes(order="C")
            )
    return bytes(payload)


def _fit_policy_only(
    model: screen.StructuredAdapter,
    decisions: list[Any],
) -> list[dict[str, float | int]]:
    if not decisions:
        _fail("fit decisions are empty")
    weights = distill._episode_weights(decisions)
    parameters = _policy_parameters(model)
    initial_value_bits = _value_head_bits(model)
    optimizer = torch.optim.AdamW(parameters, lr=LR, weight_decay=WEIGHT_DECAY)
    rng = random.Random(SEED)
    history: list[dict[str, float | int]] = []
    for epoch in range(EPOCHS):
        order = list(range(len(decisions)))
        rng.shuffle(order)
        model.train()
        weighted_kl_total = 0.0
        weighted_mass_total = 0.0
        gradient_norm_max = 0.0
        optimizer_steps = 0
        for start in range(0, len(order), BATCH_SIZE):
            batch = [decisions[index] for index in order[start : start + BATCH_SIZE]]
            losses: list[torch.Tensor] = []
            masses: list[float] = []
            for decision in batch:
                _, row_mass = weights[decision.key]
                for row in decision.rows:
                    student_logits, _ = model._one(row)
                    teacher_probability = torch.softmax(row["old_logits"].double(), dim=0)
                    student_log_probability = torch.log_softmax(
                        student_logits.double(), dim=0
                    )
                    losses.append(
                        torch.nn.functional.kl_div(
                            student_log_probability,
                            teacher_probability,
                            reduction="sum",
                        )
                    )
                    masses.append(row_mass)
            mass_tensor = torch.tensor(masses, dtype=torch.float64)
            mass = float(mass_tensor.sum())
            if not losses or mass <= 0.0:
                _fail("policy minibatch has no positive mass")
            loss = (torch.stack(losses) * mass_tensor).sum() / mass_tensor.sum()
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient_norm = torch.nn.utils.clip_grad_norm_(parameters, GRAD_CAP)
            if not torch.isfinite(gradient_norm):
                _fail("non-finite training gradient")
            optimizer.step()
            weighted_kl_total += float(loss.detach()) * mass
            weighted_mass_total += mass
            gradient_norm_max = max(gradient_norm_max, float(gradient_norm))
            optimizer_steps += 1
        history.append(
            {
                "epoch": epoch + 1,
                "weighted_mean_policy_kl": weighted_kl_total
                / max(weighted_mass_total, 1.0e-12),
                "policy_mass": weighted_mass_total,
                "maximum_preclip_gradient_norm": gradient_norm_max,
                "optimizer_steps": optimizer_steps,
            }
        )
    if _value_head_bits(model) != initial_value_bits:
        _fail("frozen value head changed during policy-only fit")
    return history


def _metric_public(metric: dict[str, Any]) -> dict[str, Any]:
    return {
        key: value
        for key, value in metric.items()
        if key not in ("_sums", "tv_weighted_samples", "value_rmse")
    }


def _public_metrics(metrics: dict[str, Any]) -> dict[str, Any]:
    return {
        "overall": _metric_public(metrics["overall"]),
        "by_candidate_seat": {
            seat: _metric_public(metric)
            for seat, metric in metrics["by_candidate_seat"].items()
        },
    }


def _package_metrics(metrics: dict[str, Any]) -> dict[str, Any]:
    def one(metric: dict[str, Any]) -> dict[str, float]:
        return {
            "mean_total_variation": float(metric["mean_total_variation"]),
            "p90_total_variation": float(metric["p90_total_variation"]),
            "top_action_agreement": float(metric["top_action_agreement"]),
        }

    return {
        "overall": one(metrics["overall"]),
        "by_candidate_seat": {
            seat: one(metric)
            for seat, metric in metrics["by_candidate_seat"].items()
        },
    }


def _fit_gate(metrics: dict[str, Any]) -> dict[str, Any]:
    checks: dict[str, bool] = {}
    for label, metric in (
        ("overall", metrics["overall"]),
        ("candidate_seat_0", metrics["by_candidate_seat"]["0"]),
        ("candidate_seat_1", metrics["by_candidate_seat"]["1"]),
    ):
        checks[f"{label}_mean_tv_at_most_0p015"] = (
            float(metric["mean_total_variation"]) <= MEAN_TV_LIMIT
        )
        checks[f"{label}_p90_tv_at_most_0p040"] = (
            float(metric["p90_total_variation"]) <= P90_TV_LIMIT
        )
        checks[f"{label}_top_action_at_least_0p990"] = (
            float(metric["top_action_agreement"]) >= TOP_ACTION_FLOOR
        )
    return {"checks": checks, "decision": "PASS" if all(checks.values()) else "REJECT"}


def _history_bucket(length: int) -> int:
    if length == 0:
        return 0
    if length <= 3:
        return 1
    if length <= 7:
        return 4
    if length <= 15:
        return 8
    return 16


def _parity_fixture(
    model: screen.StructuredAdapter,
    decisions: list[Any],
) -> dict[str, Any]:
    buckets: dict[tuple[int, int], dict[str, Any]] = {}
    for decision in decisions:
        for row in decision.rows:
            seat = int(row["acting_seat"])
            bucket = _history_bucket(int(row["history_features"].shape[0]))
            buckets.setdefault((seat, bucket), row)
    expected = {(seat, bucket) for seat in (0, 1) for bucket in (0, 1, 4, 8, 16)}
    if set(buckets) != expected:
        missing = sorted(expected - set(buckets))
        _fail(f"parity fixture lacks acting-seat/history coverage: {missing}")
    rows: list[dict[str, Any]] = []
    model.eval()
    with torch.no_grad():
        for seat, bucket in sorted(expected):
            example = buckets[(seat, bucket)]
            logits, value_residual = model._one(example)
            if float(value_residual) != 0.0:
                _fail("policy-only parity value residual is not exact zero")
            history_rows = []
            for row in example["history_features"]:
                self_role = float(row[screen.ACTION_EXPLICIT_DIM])
                opponent_role = float(row[screen.ACTION_EXPLICIT_DIM + 1])
                if (self_role, opponent_role) not in ((1.0, 0.0), (0.0, 1.0)):
                    _fail("parity history role is not one-hot")
                history_rows.append(
                    {
                        "acting_player": seat if self_role == 1.0 else 1 - seat,
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
                    "acting_player": seat,
                    "candidate_seat": int(example["candidate_seat"]),
                    "history_length_bucket": bucket,
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
                        "action_ref_action_indices": example[
                            "ref_action_indices"
                        ].tolist(),
                        "action_ref_node_indices": example[
                            "ref_node_indices"
                        ].tolist(),
                    },
                    "history": history_rows,
                    "expected_structured_logits": logits.tolist(),
                    "expected_value_residual_f32_bits": "00000000",
                }
            )
    return {
        "schema": PARITY_SCHEMA,
        "output_semantics": "absolute-structured-logits-and-exact-parent-value/v1",
        "examples": rows,
    }


def _encoded_weights(
    model: screen.StructuredAdapter,
) -> tuple[bytes, list[dict[str, Any]]]:
    payload = bytearray()
    parameters: list[dict[str, Any]] = []
    offset = 0
    for name, state_tensor in model.state_dict().items():
        tensor = state_tensor.detach().cpu().contiguous().float()
        if not torch.isfinite(tensor).all():
            _fail(f"non-finite parameter {name}")
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
    return bytes(payload), parameters


def _publish(
    args: argparse.Namespace,
    model: screen.StructuredAdapter,
    source: dict[str, Any],
    training_history: list[dict[str, float | int]],
    metrics: dict[str, Any],
    gate: dict[str, Any],
    state_path: Path,
    state_sha256: str,
    parity_path: Path,
    parity_sha256: str,
    timings: dict[str, float],
) -> dict[str, Any]:
    if args.output_root.exists():
        _fail(f"refusing to overwrite {args.output_root}")
    payload, parameters = _encoded_weights(model)
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

    weights_path = args.output_root / "weights.f32le"
    weights_path.write_bytes(payload)
    weights_sha256 = live._sha256(weights_path)
    composite_sha256 = hashlib.sha256(
        COMPOSITE_DOMAIN
        + bytes.fromhex(live.PARENT_MODEL_PARAMETER_SHA256)
        + payload
    ).hexdigest()
    report = {
        "schema": REPORT_SCHEMA,
        "source": {
            "cache_sha256": source["cache_sha256"],
            "pair_count": source["pair_count"],
        },
        "config": {
            "architecture": ARCHITECTURE,
            "value_model": VALUE_MODEL,
            "seed": SEED,
            "epochs": EPOCHS,
            "batch_size_physical_decisions": BATCH_SIZE,
            "learning_rate": LR,
            "weight_decay": WEIGHT_DECAY,
            "gradient_norm_cap": GRAD_CAP,
            "history_length": distill.HISTORY_LENGTH,
            "history_feature_dim": distill.HISTORY_FEATURE_DIM,
            "weighting": "equal_episode_mass_equal_physical_decision_mass_equal_substep_mass",
            "objective": "teacher-to-student-policy-kl-only/v1",
        },
        "policy_metrics": _package_metrics(metrics),
        "transport": {
            "maximum_absolute_logit_error": PROVISIONAL_TRANSPORT_MAX_ERROR,
            "parent_value_bit_exact": PROVISIONAL_PARENT_VALUE_BIT_EXACT,
        },
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
    }
    report_path = args.output_root / "report.json"
    report_path.write_bytes(history_publish._json_bytes(report))
    report_sha256 = live._sha256(report_path)
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
            "hidden_dim": distill.DIM,
            "card_vocab": distill.CARD_VOCAB,
            "card_embedding_dim": max(8, distill.DIM // 2),
            "group_vocab": distill.GROUP_VOCAB,
            "group_embedding_dim": max(8, distill.DIM // 3),
            "history_length": distill.HISTORY_LENGTH,
            "history_feature_dim": distill.HISTORY_FEATURE_DIM,
            "history_role_dim": 2,
            "value_model": VALUE_MODEL,
        },
        "weights": {
            "filename": weights_path.name,
            "encoding": "ordered-row-major-finite-f32-little-endian/v1",
            "sha256": weights_sha256,
            "byte_count": len(payload),
            "parameter_count": history_publish.EXPECTED_PARAMETER_COUNT,
            "parameters": parameters,
        },
        "report": {"filename": report_path.name, "sha256": report_sha256},
        "composite_model_parameter_sha256": composite_sha256,
    }
    candidate_path = args.output_root / CANDIDATE_FILENAME
    candidate_path.write_bytes(history_publish._json_bytes(candidate))
    return {
        "decision": "STAGED_PENDING_NATIVE_TRANSPORT",
        "candidate_root": str(args.output_root),
        "candidate_json_sha256": live._sha256(candidate_path),
        "weights_sha256": weights_sha256,
        "report_sha256": report_sha256,
        "composite_model_parameter_sha256": composite_sha256,
        "model_state_sha256": state_sha256,
        "parity_fixture_sha256": parity_sha256,
        "fit_metrics": _public_metrics(metrics),
        "fit_gate": gate,
        "runtime_seconds": sum(timings.values()),
    }


def fit(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    for path in (
        args.output_root,
        Path(str(args.output_root) + ".state.pt"),
        Path(str(args.output_root) + ".parity.json"),
        Path(str(args.output_root) + ".fit.json"),
    ):
        if path.exists():
            _fail(f"refusing to overwrite {path}")
    decisions, source, load_timings = distill._load_decisions(args.cache, None)
    loaded = time.perf_counter()
    screen._configure(SEED, EXPECTED_THREADS)
    model = distill._model()
    initial_value_bits = _value_head_bits(model)
    training_history = _fit_policy_only(model, decisions)
    trained = time.perf_counter()
    metrics = distill._metrics(model, decisions, include_samples=True)
    gate = _fit_gate(metrics)
    measured = time.perf_counter()

    state_path = Path(str(args.output_root) + ".state.pt")
    screen._atomic_torch_save(
        {
            "schema": MODEL_STATE_SCHEMA,
            "source": source,
            "config": {
                "architecture": ARCHITECTURE,
                "objective": "teacher-to-student-policy-kl/v1",
                "epochs": EPOCHS,
                "batch_size_physical_decisions": BATCH_SIZE,
                "learning_rate": LR,
                "weight_decay": WEIGHT_DECAY,
                "gradient_norm_cap": GRAD_CAP,
                "seed": SEED,
                "threads": EXPECTED_THREADS,
                "value_model": VALUE_MODEL,
            },
            "training_history": training_history,
            "fit_metrics": _public_metrics(metrics),
            "fit_gate": gate,
            "model_state_dict": model.state_dict(),
        },
        state_path,
    )
    state_sha256 = live._sha256(state_path)
    parity_path = Path(str(args.output_root) + ".parity.json")
    _write_new(parity_path, _parity_fixture(model, decisions))
    parity_sha256 = live._sha256(parity_path)
    checkpointed = time.perf_counter()
    if _value_head_bits(model) != initial_value_bits:
        _fail("frozen value head changed before publication")

    timings = {
        **load_timings,
        "train_seconds": trained - loaded,
        "full_data_metrics_seconds": measured - trained,
        "checkpoint_and_parity_seconds": checkpointed - measured,
    }
    outcome = {
        "schema": REPORT_SCHEMA + ".fit-outcome",
        "decision": gate["decision"],
        "source": {**source, "source_commit": args.source_commit},
        "training_history": training_history,
        "fit_metrics": _public_metrics(metrics),
        "fit_gate": gate,
        "model_state": {"path": str(state_path), "sha256": state_sha256},
        "parity_fixture": {"path": str(parity_path), "sha256": parity_sha256},
        "runtime_seconds": checkpointed - started,
        "phase_runtime_seconds": timings,
    }
    fit_path = Path(str(args.output_root) + ".fit.json")
    _write_new(fit_path, outcome)
    if gate["decision"] != "PASS":
        return {
            **outcome,
            "fit_outcome_sha256": live._sha256(fit_path),
            "publication": "WITHHELD",
        }
    publication = _publish(
        args,
        model,
        source,
        training_history,
        metrics,
        gate,
        state_path,
        state_sha256,
        parity_path,
        parity_sha256,
        timings,
    )
    publication["fit_outcome_sha256"] = live._sha256(fit_path)
    return publication


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--parent-outcome-root", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--threads", type=int, default=EXPECTED_THREADS)
    args = parser.parse_args()
    if args.threads != EXPECTED_THREADS:
        _fail(f"full fit is fixed to {EXPECTED_THREADS} threads")
    print(json.dumps(fit(args), sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
