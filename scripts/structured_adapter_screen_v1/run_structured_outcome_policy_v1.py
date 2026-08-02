#!/usr/bin/env python3
"""Fit and aggregate the fixed four-fold structured outcome-policy screen."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import random
import shutil
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import torch

import fit_policy_live_candidate as live
import run_screen as screen


SCHEMA = "mtg-kernel-structured-outcome-policy-screen/v1"
AGGREGATE_SCHEMA = SCHEMA + ".aggregate"


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _write_new_json(path: Path, value: Any, compact: bool = False) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(
        value,
        sort_keys=True,
        indent=None if compact else 2,
        separators=(",", ":") if compact else None,
        allow_nan=False,
    )
    path.write_text(text + "\n", encoding="utf-8", newline="\n")


@dataclass
class PhysicalDecision:
    key: tuple[int, str, int, int]
    episode_key: tuple[int, str, int]
    pair_index: int
    candidate_seat: int
    rows: list[dict[str, Any]]
    raw_advantage: float
    old_joint_log_probability: float
    episode_weight: float = 0.0
    standardized_advantage: float = 0.0


def _joint_log_probability(
    model: screen.StructuredAdapter, group: PhysicalDecision
) -> torch.Tensor:
    terms: list[torch.Tensor] = []
    for row in group.rows:
        logits, _ = model(row)
        terms.append(torch.log_softmax(logits, dim=0)[int(row["selected_index"])])
    return torch.stack(terms).sum()


def _physical_decisions(examples: list[dict[str, Any]]) -> list[PhysicalDecision]:
    grouped: dict[tuple[int, str, int, int], list[dict[str, Any]]] = {}
    for row in examples:
        key = (
            int(row["pair_index"]),
            str(row["episode"]),
            int(row["candidate_seat"]),
            int(row["physical_group"]),
        )
        grouped.setdefault(key, []).append(row)
    decisions: list[PhysicalDecision] = []
    for key in sorted(grouped):
        rows = sorted(grouped[key], key=lambda row: int(row["substep_index"]))
        episode_key = (key[0], key[1], key[2])
        if any(row["episode_key"] != episode_key for row in rows):
            _fail(f"physical decision {key} mixes episodes")
        rewards = {float(row["terminal_reward"]) for row in rows}
        if len(rewards) != 1:
            _fail(f"physical decision {key} mixes terminal rewards")
        old_joint = 0.0
        for row in rows:
            old_joint += float(
                torch.log_softmax(row["old_logits"].double(), dim=0)[
                    int(row["selected_index"])
                ]
            )
        decisions.append(
            PhysicalDecision(
                key=key,
                episode_key=episode_key,
                pair_index=key[0],
                candidate_seat=key[2],
                rows=rows,
                raw_advantage=float(next(iter(rewards)) - float(rows[0]["old_value"])),
                old_joint_log_probability=old_joint,
            )
        )
    episode_counts: dict[tuple[int, str, int], int] = {}
    for decision in decisions:
        episode_counts[decision.episode_key] = episode_counts.get(decision.episode_key, 0) + 1
    for decision in decisions:
        decision.episode_weight = 1.0 / float(episode_counts[decision.episode_key])
    return decisions


def _advantage_statistics(
    fit: list[PhysicalDecision],
) -> dict[int, dict[str, float | int]]:
    result: dict[int, dict[str, float | int]] = {}
    for seat in (0, 1):
        subset = [group for group in fit if group.candidate_seat == seat]
        weight = sum(group.episode_weight for group in subset)
        if weight <= 0:
            _fail(f"fit split has no candidate-seat {seat} decisions")
        mean = sum(group.raw_advantage * group.episode_weight for group in subset) / weight
        variance = (
            sum(
                (group.raw_advantage - mean) ** 2 * group.episode_weight
                for group in subset
            )
            / weight
        )
        result[seat] = {
            "mean": mean,
            "standard_deviation": max(math.sqrt(max(variance, 0.0)), 1e-6),
            "episode_mass": weight,
            "physical_decision_count": len(subset),
        }
    return result


def _install_standardized_advantages(
    decisions: list[PhysicalDecision], statistics: dict[int, dict[str, float | int]]
) -> None:
    for group in decisions:
        seat = statistics[group.candidate_seat]
        group.standardized_advantage = (
            group.raw_advantage - float(seat["mean"])
        ) / float(seat["standard_deviation"])


def _fit_model(
    model: screen.StructuredAdapter,
    fit: list[PhysicalDecision],
    args: argparse.Namespace,
) -> list[dict[str, float | int]]:
    trainable = [
        parameter
        for name, parameter in model.named_parameters()
        if not name.startswith("value_head.")
    ]
    optimizer = torch.optim.AdamW(
        trainable, lr=args.lr, weight_decay=args.weight_decay
    )
    rng = random.Random(args.seed)
    episode_mass = sum(group.episode_weight for group in fit)
    normalized_weights = {
        group.key: group.episode_weight * len(fit) / episode_mass for group in fit
    }
    history: list[dict[str, float | int]] = []
    for epoch in range(args.epochs):
        order = list(range(len(fit)))
        rng.shuffle(order)
        model.train()
        epoch_loss = 0.0
        epoch_steps = 0
        epoch_clip_fraction = 0.0
        for start in range(0, len(order), args.batch_size):
            batch = [fit[index] for index in order[start : start + args.batch_size]]
            surrogates: list[torch.Tensor] = []
            weights: list[float] = []
            clipped = 0
            for group in batch:
                joint = _joint_log_probability(model, group)
                log_ratio = joint - group.old_joint_log_probability
                ratio = torch.exp(log_ratio)
                clipped_ratio = torch.clamp(
                    ratio, 1.0 - args.clip, 1.0 + args.clip
                )
                advantage = group.standardized_advantage
                surrogates.append(
                    torch.minimum(ratio * advantage, clipped_ratio * advantage)
                )
                weights.append(normalized_weights[group.key])
                clipped += int(abs(float(log_ratio.detach())) > math.log1p(args.clip))
            weight_tensor = torch.tensor(weights, dtype=torch.float32)
            loss = -(torch.stack(surrogates) * weight_tensor).mean()
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient_norm = torch.nn.utils.clip_grad_norm_(trainable, args.grad_cap)
            if not torch.isfinite(gradient_norm):
                _fail("non-finite training gradient")
            optimizer.step()
            epoch_loss += float(loss.detach())
            epoch_clip_fraction += clipped / len(batch)
            epoch_steps += 1
        history.append(
            {
                "epoch": epoch + 1,
                "mean_minibatch_loss": epoch_loss / max(epoch_steps, 1),
                "mean_minibatch_clip_fraction": epoch_clip_fraction
                / max(epoch_steps, 1),
                "optimizer_steps": epoch_steps,
            }
        )
    return history


def _row_movement_inputs(
    model: screen.StructuredAdapter, decisions: list[PhysicalDecision]
) -> tuple[list[torch.Tensor], list[torch.Tensor], list[float]]:
    parents: list[torch.Tensor] = []
    residuals: list[torch.Tensor] = []
    weights: list[float] = []
    model.eval()
    with torch.no_grad():
        for group in decisions:
            substep_weight = group.episode_weight / len(group.rows)
            for row in group.rows:
                residual, _ = model._one(row)
                parents.append(row["old_logits"].detach().clone())
                residuals.append(residual.detach().clone())
                weights.append(substep_weight)
    return parents, residuals, weights


def _weighted_quantile(samples: list[tuple[float, float]], quantile: float) -> float:
    ordered = sorted(samples)
    threshold = sum(weight for _, weight in ordered) * quantile
    cumulative = 0.0
    for value, weight in ordered:
        cumulative += weight
        if cumulative >= threshold:
            return value
    return ordered[-1][0]


def _movement(
    model: screen.StructuredAdapter, decisions: list[PhysicalDecision]
) -> dict[str, Any]:
    samples: list[tuple[float, float]] = []
    kl_numerator = 0.0
    argmax_changes = 0
    model.eval()
    with torch.no_grad():
        for group in decisions:
            substep_weight = group.episode_weight / len(group.rows)
            for row in group.rows:
                candidate, _ = model(row)
                parent_probability = torch.softmax(row["old_logits"].double(), dim=0)
                candidate_probability = torch.softmax(candidate.double(), dim=0)
                tv = float(
                    0.5 * (parent_probability - candidate_probability).abs().sum()
                )
                kl = float(
                    (
                        parent_probability
                        * (
                            parent_probability.clamp_min(1e-300).log()
                            - candidate_probability.clamp_min(1e-300).log()
                        )
                    ).sum()
                )
                samples.append((tv, substep_weight))
                kl_numerator += kl * substep_weight
                argmax_changes += int(
                    int(row["old_logits"].argmax()) != int(candidate.argmax())
                )
    weight = sum(sample_weight for _, sample_weight in samples)
    return {
        "mean_total_variation": sum(value * w for value, w in samples)
        / max(weight, 1e-12),
        "p90_total_variation": _weighted_quantile(samples, 0.90),
        "mean_parent_to_candidate_kl": kl_numerator / max(weight, 1e-12),
        "argmax_changes": argmax_changes,
        "row_count": len(samples),
        "episode_mass": weight,
        "tv_weighted_samples": samples,
    }


def _surrogate(
    model: screen.StructuredAdapter, decisions: list[PhysicalDecision]
) -> dict[str, Any]:
    records: list[tuple[int, float, float, float]] = []
    model.eval()
    with torch.no_grad():
        for group in decisions:
            log_ratio = float(
                _joint_log_probability(model, group)
                - group.old_joint_log_probability
            )
            ratio = math.exp(log_ratio)
            gain = (ratio - 1.0) * group.standardized_advantage
            records.append(
                (group.candidate_seat, gain, group.episode_weight, abs(log_ratio))
            )

    def summarize(subset: list[tuple[int, float, float, float]]) -> dict[str, Any]:
        weight = sum(record[2] for record in subset)
        numerator = sum(record[1] * record[2] for record in subset)
        return {
            "surrogate": numerator / max(weight, 1e-12),
            "numerator": numerator,
            "episode_mass": weight,
            "physical_decision_count": len(subset),
            "max_absolute_joint_log_ratio": max(
                (record[3] for record in subset), default=0.0
            ),
        }

    return {
        "overall": summarize(records),
        "by_candidate_seat": {
            str(seat): summarize([record for record in records if record[0] == seat])
            for seat in (0, 1)
        },
    }


def _diagnostics(
    model: screen.StructuredAdapter,
    decisions: list[PhysicalDecision],
    seed: int,
    sample_size: int,
) -> dict[str, Any]:
    rows = [row for group in decisions for row in group.rows]
    rng = random.Random(seed)
    permutation_rows = rng.sample(rows, min(sample_size, len(rows)))
    eligible = [row for row in rows if int(row["action_ref_features"].shape[0]) > 0]
    reference_rows = rng.sample(eligible, min(sample_size, len(eligible)))
    generator = torch.Generator(device="cpu").manual_seed(seed)
    permutation_max = 0.0
    reference_max = 0.0
    reference_affected = 0
    model.eval()
    with torch.no_grad():
        for row in permutation_rows:
            candidate, _ = model(row)
            permuted, _ = model(screen._permuted(row, generator))
            permutation_max = max(
                permutation_max, float((candidate - permuted).abs().max())
            )
        for row in reference_rows:
            candidate, _ = model(row)
            no_refs, _ = model(row, remove_refs=True)
            delta = float((candidate - no_refs).abs().max())
            reference_max = max(reference_max, delta)
            reference_affected += int(delta > 1e-4)
    return {
        "permutation_max_logit_delta": permutation_max,
        "permutation_sample_count": len(permutation_rows),
        "reference_eligible_population": len(eligible),
        "reference_sample_count": len(reference_rows),
        "reference_affected_count": reference_affected,
        "reference_affected_rate": reference_affected
        / max(len(reference_rows), 1),
        "reference_max_logit_delta": reference_max,
    }


def run_fold(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    examples, terminals = screen._load_outcome(args.outcome_jsonl)
    decisions = _physical_decisions(examples)
    fit = [group for group in decisions if group.pair_index % 4 != args.fold]
    heldout = [group for group in decisions if group.pair_index % 4 == args.fold]
    fit_episodes = {group.episode_key for group in fit}
    heldout_episodes = {group.episode_key for group in heldout}
    if (
        len(terminals) != 512
        or len(fit_episodes) != 384
        or len(heldout_episodes) != 128
        or fit_episodes.intersection(heldout_episodes)
    ):
        _fail("fold does not have the fixed 384/128 whole-episode split")

    statistics = _advantage_statistics(fit)
    _install_standardized_advantages(fit, statistics)
    _install_standardized_advantages(heldout, statistics)
    card_vocab, group_vocab = screen._model_vocab(examples)
    screen._configure(args.seed, args.threads)
    model = screen.StructuredAdapter(card_vocab, group_vocab, args.dim)
    history = _fit_model(model, fit, args)

    parents, residuals, weights = _row_movement_inputs(model, fit)
    uncalibrated = live._movement(parents, residuals, weights, 1.0)
    scale, calibrated_fit = live._calibrate(
        parents, residuals, weights, args.target_mean_tv
    )
    with torch.no_grad():
        model.policy_head.weight.mul_(scale)
        model.policy_head.bias.mul_(scale)

    heldout_surrogate = _surrogate(model, heldout)
    heldout_movement = _movement(model, heldout)
    diagnostics = _diagnostics(
        model, heldout, args.seed + args.fold + 1, args.diagnostic_sample_size
    )
    model_state = args.output.with_suffix(".state.pt")
    if model_state.exists():
        _fail(f"refusing to overwrite {model_state}")
    model_state.parent.mkdir(parents=True, exist_ok=True)
    torch.save(model.state_dict(), model_state)

    result = {
        "schema": SCHEMA,
        "fold": args.fold,
        "source": {
            "outcome_jsonl": str(args.outcome_jsonl),
            "outcome_jsonl_sha256": _sha256(args.outcome_jsonl),
            "decision_row_count": len(examples),
            "physical_decision_count": len(decisions),
            "terminal_count": len(terminals),
        },
        "split": {
            "rule": "pair_index_mod_4",
            "fit_episode_count": len(fit_episodes),
            "heldout_episode_count": len(heldout_episodes),
            "fit_physical_decision_count": len(fit),
            "heldout_physical_decision_count": len(heldout),
        },
        "config": {
            "architecture": "stateless-structured-object-action-attention-policy-residual/v1",
            "dim": args.dim,
            "card_vocab": card_vocab,
            "group_vocab": group_vocab,
            "epochs": args.epochs,
            "batch_size_physical_decisions": args.batch_size,
            "learning_rate": args.lr,
            "weight_decay": args.weight_decay,
            "ppo_clip": args.clip,
            "gradient_norm_cap": args.grad_cap,
            "seed": args.seed,
            "threads": args.threads,
            "target_fit_mean_total_variation": args.target_mean_tv,
            "value_model": "exact-retained-parent-unchanged",
        },
        "advantage_statistics_by_candidate_seat": {
            str(key): value for key, value in statistics.items()
        },
        "training_history": history,
        "calibration": {
            "scale": scale,
            "uncalibrated_fit_movement": uncalibrated,
            "calibrated_fit_movement": calibrated_fit,
        },
        "heldout_surrogate": heldout_surrogate,
        "heldout_movement": heldout_movement,
        "diagnostics": diagnostics,
        "model_state": {"path": str(model_state), "sha256": _sha256(model_state)},
        "runtime_seconds": time.perf_counter() - started,
        "non_claims": [
            "development screen only",
            "parent-policy data surrogate is not a live win rate",
            "no promotion or pro-level claim",
        ],
    }
    _write_new_json(args.output, result, compact=True)
    return result


def _combine_weighted_metric(
    results: list[dict[str, Any]], seat: str | None
) -> dict[str, float]:
    records = [
        result["heldout_surrogate"]["overall" if seat is None else "by_candidate_seat"][
            seat
        ]
        if seat is not None
        else result["heldout_surrogate"]["overall"]
        for result in results
    ]
    numerator = sum(float(record["numerator"]) for record in records)
    weight = sum(float(record["episode_mass"]) for record in records)
    return {
        "surrogate": numerator / max(weight, 1e-12),
        "numerator": numerator,
        "episode_mass": weight,
        "max_absolute_joint_log_ratio": max(
            float(record["max_absolute_joint_log_ratio"]) for record in records
        ),
    }


def aggregate(args: argparse.Namespace) -> dict[str, Any]:
    results = [json.loads(path.read_text(encoding="utf-8")) for path in args.fold_result]
    if (
        len(results) != 4
        or {result.get("fold") for result in results} != {0, 1, 2, 3}
        or any(result.get("schema") != SCHEMA for result in results)
    ):
        _fail("aggregate requires exactly folds 0, 1, 2, and 3")
    source_hashes = {
        result["source"]["outcome_jsonl_sha256"] for result in results
    }
    configs = {json.dumps(result["config"], sort_keys=True) for result in results}
    if len(source_hashes) != 1 or len(configs) != 1:
        _fail("fold source or configuration mismatch")

    overall = _combine_weighted_metric(results, None)
    by_seat = {seat: _combine_weighted_metric(results, seat) for seat in ("0", "1")}
    tv_samples = [
        (float(value), float(weight))
        for result in results
        for value, weight in result["heldout_movement"]["tv_weighted_samples"]
    ]
    movement_weight = sum(weight for _, weight in tv_samples)
    movement = {
        "mean_total_variation": sum(value * weight for value, weight in tv_samples)
        / max(movement_weight, 1e-12),
        "p90_total_variation": _weighted_quantile(tv_samples, 0.90),
        "episode_mass": movement_weight,
        "row_count": len(tv_samples),
        "max_absolute_physical_decision_joint_log_ratio": overall[
            "max_absolute_joint_log_ratio"
        ],
    }
    positive_folds = sum(
        float(result["heldout_surrogate"]["overall"]["surrogate"]) > 0.0
        for result in results
    )
    diagnostics = {
        "permutation_max_logit_delta": max(
            float(result["diagnostics"]["permutation_max_logit_delta"])
            for result in results
        ),
        "reference_sample_count": sum(
            int(result["diagnostics"]["reference_sample_count"])
            for result in results
        ),
        "reference_affected_count": sum(
            int(result["diagnostics"]["reference_affected_count"])
            for result in results
        ),
    }
    diagnostics["reference_affected_rate"] = diagnostics[
        "reference_affected_count"
    ] / max(diagnostics["reference_sample_count"], 1)
    gates = {
        "aggregate_surrogate_positive": overall["surrogate"] > 0.0,
        "both_candidate_seats_surrogate_positive": all(
            by_seat[seat]["surrogate"] > 0.0 for seat in ("0", "1")
        ),
        "at_least_three_of_four_folds_positive": positive_folds >= 3,
        "mean_total_variation_le_0_03": movement["mean_total_variation"] <= 0.03,
        "p90_total_variation_le_0_10": movement["p90_total_variation"] <= 0.10,
        "max_absolute_joint_log_ratio_le_0_50": movement[
            "max_absolute_physical_decision_joint_log_ratio"
        ]
        <= 0.50,
        "permutation_max_delta_le_1e_5": diagnostics[
            "permutation_max_logit_delta"
        ]
        <= 1e-5,
        "reference_affected_rate_ge_20pct": diagnostics[
            "reference_affected_rate"
        ]
        >= 0.20,
    }
    result = {
        "schema": AGGREGATE_SCHEMA,
        "fold_results": [
            {"path": str(path), "sha256": _sha256(path)}
            for path in args.fold_result
        ],
        "source_outcome_jsonl_sha256": next(iter(source_hashes)),
        "config": results[0]["config"],
        "heldout_surrogate": {"overall": overall, "by_candidate_seat": by_seat},
        "positive_fold_count": positive_folds,
        "fold_surrogates": {
            str(result["fold"]): result["heldout_surrogate"]["overall"]["surrogate"]
            for result in results
        },
        "heldout_movement": movement,
        "diagnostics": diagnostics,
        "gates": gates,
        "pass": all(gates.values()),
        "non_claims": [
            "development screen only",
            "a pass authorizes one fresh 32-pair strength gate, not promotion",
            "no cross-deck or pro-level claim",
        ],
    }
    _write_new_json(args.output, result)
    return result


def _publish_full_candidate(
    args: argparse.Namespace,
    model: screen.StructuredAdapter,
    card_vocab: int,
    group_vocab: int,
    report: dict[str, Any],
) -> dict[str, Any]:
    output_root: Path = args.output_root
    if output_root.exists():
        _fail(f"refusing to overwrite {output_root}")
    output_root.mkdir(parents=True)
    parent_output = output_root / "parent"
    parent_output.mkdir()
    parent_manifest = args.parent_outcome_root / "checkpoint.json"
    parent_payload = args.parent_outcome_root / "checkpoint.state.f32le"
    if (
        _sha256(parent_manifest) != live.PARENT_MANIFEST_SHA256
        or _sha256(parent_payload) != live.PARENT_PAYLOAD_SHA256
    ):
        _fail("parent root is not the exact retained checkpoint")
    shutil.copyfile(parent_manifest, parent_output / parent_manifest.name)
    shutil.copyfile(parent_payload, parent_output / parent_payload.name)

    state = model.state_dict()
    payload = bytearray()
    parameters: list[dict[str, Any]] = []
    offset = 0
    for name in live.PARAMETER_NAMES:
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
        live.COMPOSITE_DOMAIN
        + bytes.fromhex(live.PARENT_MODEL_PARAMETER_SHA256)
        + bytes(payload)
    ).hexdigest()
    report["weights_sha256"] = weights_sha
    report["composite_model_parameter_sha256"] = composite
    report_path = output_root / "report.json"
    _write_new_json(report_path, report)
    report_sha = _sha256(report_path)
    candidate = {
        "schema": live.SCHEMA,
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
    _write_new_json(candidate_path, candidate)
    return {
        "candidate_root": str(output_root),
        "candidate_json_sha256": _sha256(candidate_path),
        "weights_sha256": weights_sha,
        "report_sha256": report_sha,
        "composite_model_parameter_sha256": composite,
    }


def run_full(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    examples, terminals = screen._load_outcome(args.outcome_jsonl)
    decisions = _physical_decisions(examples)
    episodes = {group.episode_key for group in decisions}
    pairs = {group.pair_index for group in decisions}
    if len(terminals) != 512 or len(episodes) != 512 or pairs != set(range(1, 257)):
        _fail("full fit does not cover the fixed pairs 1 through 256")
    statistics = _advantage_statistics(decisions)
    _install_standardized_advantages(decisions, statistics)
    card_vocab, group_vocab = screen._model_vocab(examples)
    screen._configure(args.seed, args.threads)
    model = screen.StructuredAdapter(card_vocab, group_vocab, args.dim)
    history = _fit_model(model, decisions, args)
    parents, residuals, weights = _row_movement_inputs(model, decisions)
    uncalibrated = live._movement(parents, residuals, weights, 1.0)
    scale, _ = live._calibrate(parents, residuals, weights, args.target_mean_tv)
    with torch.no_grad():
        model.policy_head.weight.mul_(scale)
        model.policy_head.bias.mul_(scale)
    movement = _movement(model, decisions)
    movement.pop("tv_weighted_samples")
    surrogate = _surrogate(model, decisions)
    diagnostics = _diagnostics(
        model, decisions, args.seed + 10, args.diagnostic_sample_size
    )
    report = {
        "schema": "mtg-kernel-structured-outcome-policy-full-fit/v1",
        "source": {
            "source_commit": args.source_commit,
            "outcome_jsonl": str(args.outcome_jsonl),
            "outcome_jsonl_sha256": _sha256(args.outcome_jsonl),
            "decision_row_count": len(examples),
            "physical_decision_count": len(decisions),
            "terminal_count": len(terminals),
            "pair_range": [1, 256],
        },
        "config": {
            "architecture": "stateless-structured-object-action-attention-policy-residual/v1",
            "dim": args.dim,
            "card_vocab": card_vocab,
            "group_vocab": group_vocab,
            "epochs": args.epochs,
            "batch_size_physical_decisions": args.batch_size,
            "learning_rate": args.lr,
            "weight_decay": args.weight_decay,
            "ppo_clip": args.clip,
            "gradient_norm_cap": args.grad_cap,
            "seed": args.seed,
            "threads": args.threads,
            "target_fit_mean_total_variation": args.target_mean_tv,
            "value_model": "exact-retained-parent-unchanged",
        },
        "advantage_statistics_by_candidate_seat": {
            str(key): value for key, value in statistics.items()
        },
        "training_history": history,
        "calibration": {
            "scale": scale,
            "uncalibrated_fit_movement": uncalibrated,
            "calibrated_fit_movement": movement,
        },
        "in_sample_surrogate": surrogate,
        "diagnostics": diagnostics,
        "non_claims": [
            "full-data fit after a separate development gate",
            "in-sample surrogate is not acceptance evidence",
            "no live strength, promotion, cross-deck, or pro-level claim",
        ],
    }
    publication = _publish_full_candidate(
        args, model, card_vocab, group_vocab, report
    )
    publication["runtime_seconds"] = time.perf_counter() - started
    publication["calibration_scale"] = scale
    publication["movement"] = movement
    publication["in_sample_surrogate"] = surrogate
    return publication


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    fold = subparsers.add_parser("fold")
    fold.add_argument("--outcome-jsonl", type=Path, required=True)
    fold.add_argument("--fold", type=int, choices=range(4), required=True)
    fold.add_argument("--output", type=Path, required=True)
    fold.add_argument("--dim", type=int, default=48)
    fold.add_argument("--epochs", type=int, default=10)
    fold.add_argument("--batch-size", type=int, default=32)
    fold.add_argument("--lr", type=float, default=3e-4)
    fold.add_argument("--weight-decay", type=float, default=1e-4)
    fold.add_argument("--clip", type=float, default=0.10)
    fold.add_argument("--grad-cap", type=float, default=5.0)
    fold.add_argument("--seed", type=int, default=20260802)
    fold.add_argument("--threads", type=int, default=6)
    fold.add_argument("--target-mean-tv", type=float, default=0.03)
    fold.add_argument("--diagnostic-sample-size", type=int, default=256)
    aggregate_parser = subparsers.add_parser("aggregate")
    aggregate_parser.add_argument(
        "--fold-result", action="append", type=Path, required=True
    )
    aggregate_parser.add_argument("--output", type=Path, required=True)
    full = subparsers.add_parser("full")
    full.add_argument("--outcome-jsonl", type=Path, required=True)
    full.add_argument("--parent-outcome-root", type=Path, required=True)
    full.add_argument("--source-commit", required=True)
    full.add_argument("--output-root", type=Path, required=True)
    full.add_argument("--dim", type=int, default=48)
    full.add_argument("--epochs", type=int, default=10)
    full.add_argument("--batch-size", type=int, default=32)
    full.add_argument("--lr", type=float, default=3e-4)
    full.add_argument("--weight-decay", type=float, default=1e-4)
    full.add_argument("--clip", type=float, default=0.10)
    full.add_argument("--grad-cap", type=float, default=5.0)
    full.add_argument("--seed", type=int, default=20260802)
    full.add_argument("--threads", type=int, default=12)
    full.add_argument("--target-mean-tv", type=float, default=0.03)
    full.add_argument("--diagnostic-sample-size", type=int, default=256)
    args = parser.parse_args()
    if args.command in ("fold", "full"):
        if (
            args.dim != 48
            or args.epochs != 10
            or args.batch_size != 32
            or args.lr != 3e-4
            or args.weight_decay != 1e-4
            or args.clip != 0.10
            or args.grad_cap != 5.0
            or args.seed != 20260802
            or args.threads < 1
            or args.target_mean_tv != 0.03
            or args.diagnostic_sample_size != 256
        ):
            _fail("fold configuration differs from the fixed development screen")
        result = run_fold(args) if args.command == "fold" else run_full(args)
    elif args.command == "aggregate":
        result = aggregate(args)
    else:
        _fail(f"unknown command {args.command}")
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
