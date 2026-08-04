#!/usr/bin/env python3
"""Profile and run the fixed recurrent structured learner screen."""

from __future__ import annotations

import argparse
import gc
import hashlib
import json
import math
import os
import random
import sys
import time
from pathlib import Path
from typing import Any, Iterable

os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")

import numpy as np
import torch
from torch import Tensor


HERE = Path(__file__).resolve().parent
STRUCTURED = HERE.parent / "structured_adapter_screen_v1"
sys.path.insert(0, str(STRUCTURED))

import run_screen as legacy_screen  # noqa: E402
import run_structured_outcome_policy_v1 as legacy_outcome  # noqa: E402

from model_v1 import PackedRows, RecurrentStructuredActorCritic, pack_rows  # noqa: E402


SCHEMA = "mtg-kernel-recurrent-structured-learner-screen/v1"
PROFILE_SCHEMA = SCHEMA + ".profile"
EXPECTED_CACHE_SHA256 = (
    "454e4ce1b8f7413839a36c8e2731fc0cb65581ce13e593634bffa70013a6f16d"
)
CORPUS_PAIR_COUNT = 2_048
EXPECTED_CACHE_SCHEMA: str | None = "mtg-kernel-structured-policy-terminal-rung-cache/v1"
SEED = 20_260_804
DIM = 128
HISTORY_LENGTH = 32
DISTILL_EPOCHS = 1
OUTCOME_EPOCHS = 3
LEARNING_RATE = 3.0e-4
WEIGHT_DECAY = 1.0e-4
PPO_CLIP = 0.10
KL_COEFFICIENT = 0.01
VALUE_COEFFICIENT = 0.5
GRADIENT_CAP = 5.0
PROFILE_PAIR_COUNT = 128
PROFILE_WARMUP_STEPS = 20
PROFILE_MEASURED_STEPS = 100
PROFILE_BATCH_SIZES = (64, 128, 256)
PROFILE_MEMORY_LIMIT = 5 * 1024**3
DIAGNOSTIC_SAMPLE_SIZE = 256
BOOTSTRAP_REPLICATES = 2_000


PhysicalDecision = legacy_outcome.PhysicalDecision


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _json_bytes(value: Any) -> bytes:
    return (
        json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)
        + "\n"
    ).encode("utf-8")


def _write_new(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(_json_bytes(value))


def _configure(device_ordinal: int) -> torch.device:
    if not torch.cuda.is_available():
        _fail("CUDA is required for the recurrent learner screen")
    if device_ordinal != 1:
        _fail("the fixed screen requires physical GPU ordinal 1")
    if torch.cuda.device_count() <= device_ordinal:
        _fail("GPU ordinal 1 is unavailable")
    random.seed(SEED)
    np.random.seed(SEED)
    torch.manual_seed(SEED)
    torch.cuda.manual_seed_all(SEED)
    torch.use_deterministic_algorithms(True)
    torch.backends.cudnn.benchmark = False
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.set_num_threads(8)
    device = torch.device(f"cuda:{device_ordinal}")
    torch.cuda.set_device(device)
    return device


def _load_decisions(
    cache_path: Path, *, pair_limit: int | None = None
) -> tuple[list[PhysicalDecision], dict[str, Any]]:
    started = time.perf_counter()
    observed_sha256 = _sha256(cache_path)
    if observed_sha256 != EXPECTED_CACHE_SHA256:
        _fail("terminal-rung cache SHA-256 mismatch")
    cache = torch.load(cache_path, map_location="cpu", weights_only=False)
    loaded = time.perf_counter()
    if (
        cache.get("version") != legacy_screen.SCRIPT_VERSION
        or (
            EXPECTED_CACHE_SCHEMA is not None
            and cache.get("schema") != EXPECTED_CACHE_SCHEMA
        )
        or not cache.get("complete_history_join")
    ):
        _fail("cache is not the fixed complete-history terminal-rung corpus")
    value = cache.get("value")
    if not isinstance(value, list) or not value:
        _fail("cache has no value lane")
    if pair_limit is not None:
        value = [row for row in value if int(row["pair_index"]) < pair_limit]
    cache.clear()
    gc.collect()
    legacy_screen._attach_complete_action_history(
        [], value, HISTORY_LENGTH, 136
    )
    history_ready = time.perf_counter()
    decisions = legacy_outcome._physical_decisions(value)
    value.clear()
    gc.collect()
    expected_pairs = pair_limit if pair_limit is not None else CORPUS_PAIR_COUNT
    observed_pairs = {decision.pair_index for decision in decisions}
    observed_episodes = {decision.episode_key for decision in decisions}
    if observed_pairs != set(range(expected_pairs)) or len(observed_episodes) != 2 * expected_pairs:
        _fail("decision panel does not have exact pair and episode coverage")
    metadata = {
        "cache": str(cache_path),
        "cache_sha256": observed_sha256,
        "pairs": len(observed_pairs),
        "episodes": len(observed_episodes),
        "physical_decisions": len(decisions),
        "rows": sum(len(decision.rows) for decision in decisions),
        "load_seconds": loaded - started,
        "history_and_group_seconds": history_ready - loaded,
        "decision_build_seconds": time.perf_counter() - history_ready,
    }
    return decisions, metadata


def _install_advantages(
    fit: list[PhysicalDecision], evaluate: Iterable[PhysicalDecision] = ()
) -> dict[int, dict[str, float | int]]:
    statistics = legacy_outcome._advantage_statistics(fit)
    legacy_outcome._install_standardized_advantages(fit, statistics)
    legacy_outcome._install_standardized_advantages(list(evaluate), statistics)
    return statistics


def _flatten(decisions: list[PhysicalDecision]) -> tuple[list[dict[str, Any]], Tensor, Tensor]:
    rows: list[dict[str, Any]] = []
    decision_index: list[int] = []
    first_index: list[int] = []
    for index, decision in enumerate(decisions):
        first_index.append(len(rows))
        rows.extend(decision.rows)
        decision_index.extend([index] * len(decision.rows))
    return (
        rows,
        torch.tensor(decision_index, dtype=torch.long),
        torch.tensor(first_index, dtype=torch.long),
    )


def _decision_batch(
    decisions: list[PhysicalDecision], device: torch.device
) -> tuple[PackedRows, dict[str, Tensor]]:
    rows, decision_index, first_index = _flatten(decisions)
    packed = pack_rows(rows, device)
    row_weights: list[float] = []
    for decision in decisions:
        row_weights.extend(
            [decision.episode_weight / len(decision.rows)] * len(decision.rows)
        )
    tensors = {
        "decision_index": decision_index.to(device),
        "first_index": first_index.to(device),
        "old_joint": torch.tensor(
            [decision.old_joint_log_probability for decision in decisions],
            dtype=torch.float32,
            device=device,
        ),
        "advantage": torch.tensor(
            [decision.standardized_advantage for decision in decisions],
            dtype=torch.float32,
            device=device,
        ),
        "decision_weight": torch.tensor(
            [decision.episode_weight for decision in decisions],
            dtype=torch.float32,
            device=device,
        ),
        "row_weight": torch.tensor(row_weights, dtype=torch.float32, device=device),
        "terminal": torch.tensor(
            [float(decision.rows[0]["terminal_reward"]) for decision in decisions],
            dtype=torch.float32,
            device=device,
        ),
    }
    return packed, tensors


def _selected_row_log_probability(logits: Tensor, packed: PackedRows) -> Tensor:
    return torch.log_softmax(logits, dim=1).gather(
        1, packed.selected_index.unsqueeze(1)
    ).squeeze(1)


def _loss(
    model: RecurrentStructuredActorCritic,
    decisions: list[PhysicalDecision],
    device: torch.device,
    phase: str,
) -> tuple[Tensor, dict[str, float]]:
    packed, tensors = _decision_batch(decisions, device)
    logits, values = model(packed)
    candidate_log_probability = torch.log_softmax(logits, dim=1)
    parent_log_probability = torch.log_softmax(packed.parent_logits, dim=1)
    parent_probability = torch.softmax(packed.parent_logits, dim=1)
    row_kl = (
        parent_probability * (parent_log_probability - candidate_log_probability)
    ).sum(dim=1)
    row_weight = tensors["row_weight"]
    behavior_kl = (row_kl * row_weight).sum() / row_weight.sum().clamp_min(1.0e-12)
    if phase == "distill":
        value_error = (values - packed.parent_value).square()
        value_loss = (value_error * row_weight).sum() / row_weight.sum().clamp_min(1.0e-12)
        total = behavior_kl + VALUE_COEFFICIENT * value_loss
        return total, {
            "policy_kl": float(behavior_kl.detach()),
            "value_loss": float(value_loss.detach()),
            "actor_loss": 0.0,
        }
    if phase != "outcome":
        _fail(f"unknown training phase {phase}")
    row_selected = _selected_row_log_probability(logits, packed)
    joint = torch.zeros((len(decisions),), device=device)
    joint.index_add_(0, tensors["decision_index"], row_selected)
    ratio = torch.exp(joint - tensors["old_joint"])
    clipped_ratio = torch.clamp(ratio, 1.0 - PPO_CLIP, 1.0 + PPO_CLIP)
    surrogate = torch.minimum(
        ratio * tensors["advantage"], clipped_ratio * tensors["advantage"]
    )
    decision_weight = tensors["decision_weight"]
    actor_loss = -(surrogate * decision_weight).sum() / decision_weight.sum().clamp_min(1.0e-12)
    first_values = values.index_select(0, tensors["first_index"])
    value_error = (first_values - tensors["terminal"]).square()
    value_loss = (value_error * decision_weight).sum() / decision_weight.sum().clamp_min(1.0e-12)
    total = actor_loss + VALUE_COEFFICIENT * value_loss + KL_COEFFICIENT * behavior_kl
    return total, {
        "policy_kl": float(behavior_kl.detach()),
        "value_loss": float(value_loss.detach()),
        "actor_loss": float(actor_loss.detach()),
    }


def _state_sha256(model: RecurrentStructuredActorCritic) -> str:
    digest = hashlib.sha256()
    with torch.no_grad():
        for name, value in sorted(model.state_dict().items()):
            array = value.detach().cpu().contiguous().numpy()
            digest.update(name.encode("utf-8") + b"\0")
            digest.update(str(array.dtype).encode("ascii") + b"\0")
            digest.update(np.asarray(array.shape, dtype="<i8").tobytes())
            digest.update(array.tobytes(order="C"))
    return digest.hexdigest()


def _packed_sha256(packed: PackedRows) -> str:
    digest = hashlib.sha256()
    for name, value in sorted(vars(packed).items()):
        array = value.detach().cpu().contiguous().numpy()
        digest.update(name.encode("utf-8") + b"\0")
        digest.update(str(array.dtype).encode("ascii") + b"\0")
        digest.update(np.asarray(array.shape, dtype="<i8").tobytes())
        digest.update(array.tobytes(order="C"))
    return digest.hexdigest()


def _profile_arm(
    decisions: list[PhysicalDecision], batch_size: int, device: torch.device
) -> dict[str, Any]:
    torch.manual_seed(SEED)
    torch.cuda.manual_seed_all(SEED)
    model = RecurrentStructuredActorCritic(DIM).to(device)
    optimizer = torch.optim.AdamW(
        model.parameters(), lr=LEARNING_RATE, weight_decay=WEIGHT_DECAY
    )
    order = list(range(len(decisions)))
    rng = random.Random(SEED + batch_size)
    rng.shuffle(order)
    cursor = 0
    losses: list[float] = []
    input_hash = None
    torch.cuda.empty_cache()
    torch.cuda.reset_peak_memory_stats(device)
    measured_decisions = 0
    measured_started = 0.0
    total_steps = PROFILE_WARMUP_STEPS + PROFILE_MEASURED_STEPS
    for step in range(total_steps):
        if cursor + batch_size > len(order):
            rng.shuffle(order)
            cursor = 0
        batch = [decisions[index] for index in order[cursor : cursor + batch_size]]
        cursor += batch_size
        if input_hash is None:
            packed, _ = _decision_batch(batch, device)
            input_hash = _packed_sha256(packed)
            del packed
        if step == PROFILE_WARMUP_STEPS:
            torch.cuda.synchronize(device)
            measured_started = time.perf_counter()
        loss, _ = _loss(model, batch, device, "outcome")
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        gradient_norm = torch.nn.utils.clip_grad_norm_(
            model.parameters(), GRADIENT_CAP
        )
        if not torch.isfinite(gradient_norm):
            _fail("non-finite profile gradient")
        optimizer.step()
        if step >= PROFILE_WARMUP_STEPS:
            measured_decisions += len(batch)
            losses.append(float(loss.detach()))
    torch.cuda.synchronize(device)
    elapsed = time.perf_counter() - measured_started
    result = {
        "batch_size": batch_size,
        "warmup_steps": PROFILE_WARMUP_STEPS,
        "measured_steps": PROFILE_MEASURED_STEPS,
        "measured_physical_decisions": measured_decisions,
        "measured_seconds": elapsed,
        "physical_decisions_per_second": measured_decisions / elapsed,
        "peak_allocated_bytes": torch.cuda.max_memory_allocated(device),
        "packed_input_sha256": input_hash,
        "loss_trace_sha256": hashlib.sha256(
            np.asarray(losses, dtype="<f8").tobytes()
        ).hexdigest(),
        "model_state_sha256": _state_sha256(model),
    }
    del optimizer, model
    gc.collect()
    torch.cuda.empty_cache()
    return result


def run_profile(args: argparse.Namespace) -> dict[str, Any]:
    device = _configure(args.gpu)
    started = time.perf_counter()
    decisions, source = _load_decisions(
        args.cache, pair_limit=PROFILE_PAIR_COUNT
    )
    _install_advantages(decisions)
    arms = [_profile_arm(decisions, size, device) for size in PROFILE_BATCH_SIZES]
    best_rate = max(float(arm["physical_decisions_per_second"]) for arm in arms)
    eligible = [
        arm
        for arm in arms
        if int(arm["peak_allocated_bytes"]) <= PROFILE_MEMORY_LIMIT
        and float(arm["physical_decisions_per_second"]) >= 0.95 * best_rate
    ]
    if not eligible:
        _fail("no throughput arm is eligible")
    selected = max(eligible, key=lambda arm: int(arm["batch_size"]))
    repeat = _profile_arm(decisions, int(selected["batch_size"]), device)
    deterministic = all(
        selected[key] == repeat[key]
        for key in (
            "packed_input_sha256",
            "loss_trace_sha256",
            "model_state_sha256",
        )
    )
    result = {
        "schema": PROFILE_SCHEMA,
        "source": source,
        "toolchain": {
            "python": sys.version.split()[0],
            "torch": torch.__version__,
            "cuda": torch.version.cuda,
            "gpu_ordinal": args.gpu,
            "gpu_name": torch.cuda.get_device_name(device),
            "gpu_total_bytes": torch.cuda.get_device_properties(device).total_memory,
        },
        "fixed": {
            "model_dim": DIM,
            "history_length": HISTORY_LENGTH,
            "warmup_steps": PROFILE_WARMUP_STEPS,
            "measured_steps": PROFILE_MEASURED_STEPS,
            "memory_limit_bytes": PROFILE_MEMORY_LIMIT,
        },
        "arms": arms,
        "selected_batch_size": int(selected["batch_size"]),
        "repeat": repeat,
        "deterministic_repeat": deterministic,
        "status": "pass" if deterministic else "fail",
        "runtime_seconds": time.perf_counter() - started,
    }
    _write_new(args.output, result)
    return result


def _batches(
    decisions: list[PhysicalDecision], batch_size: int, rng: random.Random
) -> Iterable[list[PhysicalDecision]]:
    order = list(range(len(decisions)))
    rng.shuffle(order)
    for start in range(0, len(order), batch_size):
        yield [decisions[index] for index in order[start : start + batch_size]]


def _fit_fold(
    fit: list[PhysicalDecision], batch_size: int, device: torch.device, fold: int
) -> tuple[RecurrentStructuredActorCritic, list[dict[str, Any]]]:
    torch.manual_seed(SEED + fold)
    torch.cuda.manual_seed_all(SEED + fold)
    model = RecurrentStructuredActorCritic(DIM).to(device)
    optimizer = torch.optim.AdamW(
        model.parameters(), lr=LEARNING_RATE, weight_decay=WEIGHT_DECAY
    )
    history: list[dict[str, Any]] = []
    rng = random.Random(SEED + fold)
    phases = [("distill", DISTILL_EPOCHS), ("outcome", OUTCOME_EPOCHS)]
    for phase, epochs in phases:
        for epoch in range(epochs):
            model.train()
            sums = {"loss": 0.0, "policy_kl": 0.0, "value_loss": 0.0, "actor_loss": 0.0}
            steps = 0
            started = time.perf_counter()
            for batch in _batches(fit, batch_size, rng):
                loss, parts = _loss(model, batch, device, phase)
                optimizer.zero_grad(set_to_none=True)
                loss.backward()
                gradient_norm = torch.nn.utils.clip_grad_norm_(
                    model.parameters(), GRADIENT_CAP
                )
                if not torch.isfinite(gradient_norm):
                    _fail("non-finite fold gradient")
                optimizer.step()
                sums["loss"] += float(loss.detach())
                for key, value in parts.items():
                    sums[key] += value
                steps += 1
            torch.cuda.synchronize(device)
            history.append(
                {
                    "phase": phase,
                    "epoch": epoch + 1,
                    "steps": steps,
                    "seconds": time.perf_counter() - started,
                    **{key: value / max(steps, 1) for key, value in sums.items()},
                }
            )
    return model, history


def _weighted_quantile(samples: list[tuple[float, float]], quantile: float) -> float:
    ordered = sorted(samples)
    target = quantile * sum(weight for _, weight in ordered)
    cumulative = 0.0
    for value, weight in ordered:
        cumulative += weight
        if cumulative >= target:
            return value
    return ordered[-1][0]


def _evaluate(
    model: RecurrentStructuredActorCritic,
    decisions: list[PhysicalDecision],
    batch_size: int,
    device: torch.device,
) -> tuple[dict[str, Any], dict[str, Any]]:
    surrogate_records: list[dict[str, float | int]] = []
    tv_samples: list[tuple[float, float]] = []
    value_records: list[dict[str, float | int]] = []
    model.eval()
    with torch.no_grad():
        for start in range(0, len(decisions), batch_size):
            selected = decisions[start : start + batch_size]
            packed, tensors = _decision_batch(selected, device)
            logits, values = model(packed)
            selected_log = _selected_row_log_probability(logits, packed)
            joint = torch.zeros((len(selected),), device=device)
            joint.index_add_(0, tensors["decision_index"], selected_log)
            log_ratio = joint - tensors["old_joint"]
            ratio = torch.exp(log_ratio)
            first_values = values.index_select(0, tensors["first_index"])
            candidate_probability = torch.softmax(logits, dim=1)
            parent_probability = torch.softmax(packed.parent_logits, dim=1)
            row_tv = 0.5 * (candidate_probability - parent_probability).abs().sum(dim=1)
            row_cursor = 0
            for index, decision in enumerate(selected):
                gain = float((ratio[index] - 1.0) * tensors["advantage"][index])
                surrogate_records.append(
                    {
                        "pair": decision.pair_index,
                        "seat": decision.candidate_seat,
                        "gain": gain,
                        "weight": decision.episode_weight,
                        "absolute_log_ratio": abs(float(log_ratio[index])),
                    }
                )
                substep_weight = decision.episode_weight / len(decision.rows)
                for offset in range(len(decision.rows)):
                    tv_samples.append(
                        (float(row_tv[row_cursor + offset]), substep_weight)
                    )
                row_cursor += len(decision.rows)
                target = float(decision.rows[0]["terminal_reward"])
                parent = float(decision.rows[0]["old_value"])
                candidate = float(first_values[index])
                value_records.append(
                    {
                        "seat": decision.candidate_seat,
                        "weight": decision.episode_weight,
                        "parent_error": (parent - target) ** 2,
                        "candidate_error": (candidate - target) ** 2,
                    }
                )

    def surrogate_summary(records: list[dict[str, float | int]]) -> dict[str, Any]:
        weight = sum(float(record["weight"]) for record in records)
        numerator = sum(
            float(record["gain"]) * float(record["weight"]) for record in records
        )
        return {
            "surrogate": numerator / max(weight, 1.0e-12),
            "numerator": numerator,
            "episode_mass": weight,
            "physical_decisions": len(records),
            "max_absolute_joint_log_ratio": max(
                (float(record["absolute_log_ratio"]) for record in records), default=0.0
            ),
        }

    def value_summary(records: list[dict[str, float | int]]) -> dict[str, Any]:
        weight = sum(float(record["weight"]) for record in records)
        parent = sum(
            float(record["parent_error"]) * float(record["weight"])
            for record in records
        ) / max(weight, 1.0e-12)
        candidate = sum(
            float(record["candidate_error"]) * float(record["weight"])
            for record in records
        ) / max(weight, 1.0e-12)
        return {
            "parent_mse": parent,
            "candidate_mse": candidate,
            "relative_improvement": (parent - candidate) / max(parent, 1.0e-12),
            "episode_mass": weight,
        }

    weight = sum(sample_weight for _, sample_weight in tv_samples)
    report = {
        "surrogate": {
            "overall": surrogate_summary(surrogate_records),
            "by_candidate_seat": {
                str(seat): surrogate_summary(
                    [record for record in surrogate_records if record["seat"] == seat]
                )
                for seat in (0, 1)
            },
        },
        "movement": {
            "mean_total_variation": sum(value * w for value, w in tv_samples)
            / max(weight, 1.0e-12),
            "p90_total_variation": _weighted_quantile(tv_samples, 0.90),
            "episode_mass": weight,
            "row_count": len(tv_samples),
        },
        "value": {
            "overall": value_summary(value_records),
            "by_candidate_seat": {
                str(seat): value_summary(
                    [record for record in value_records if record["seat"] == seat]
                )
                for seat in (0, 1)
            },
        },
    }
    internal = {
        "surrogate_records": surrogate_records,
        "tv_samples": tv_samples,
        "value_records": value_records,
    }
    return report, internal


def _diagnostics(
    model: RecurrentStructuredActorCritic,
    decisions: list[PhysicalDecision],
    fold: int,
    device: torch.device,
) -> dict[str, Any]:
    rows = [row for decision in decisions for row in decision.rows]
    rng = random.Random(SEED + 100 + fold)
    permutation_rows = rng.sample(rows, min(DIAGNOSTIC_SAMPLE_SIZE, len(rows)))
    reference_population = [
        row for row in rows if int(row["action_ref_features"].shape[0]) > 0
    ]
    reference_rows = rng.sample(
        reference_population, min(DIAGNOSTIC_SAMPLE_SIZE, len(reference_population))
    )
    digest_rows = rng.sample(rows, min(DIAGNOSTIC_SAMPLE_SIZE, len(rows)))
    generator = torch.Generator(device="cpu").manual_seed(SEED + 100 + fold)
    permutation_max = 0.0
    reference_affected = 0
    reference_max = 0.0
    digest_affected = 0
    digest_max = 0.0
    model.eval()
    with torch.no_grad():
        for row in permutation_rows:
            original, _ = model(pack_rows([row], device))
            permuted, _ = model(
                pack_rows([legacy_screen._permuted(row, generator)], device)
            )
            permutation_max = max(
                permutation_max, float((original - permuted).abs().max())
            )
        for row in reference_rows:
            original, _ = model(pack_rows([row], device))
            no_refs, _ = model(pack_rows([row], device, remove_refs=True))
            delta = float((original - no_refs).abs().max())
            reference_max = max(reference_max, delta)
            reference_affected += int(delta > 1.0e-4)
        for row in digest_rows:
            packed = pack_rows([row], device)
            original_logits, original_value = model(packed)
            no_digest_logits, no_digest_value = model(packed, remove_digest=True)
            delta = max(
                float((original_logits - no_digest_logits).abs().max()),
                float((original_value - no_digest_value).abs().max()),
            )
            digest_max = max(digest_max, delta)
            digest_affected += int(delta > 1.0e-4)
    return {
        "permutation_max_logit_delta": permutation_max,
        "permutation_sample_count": len(permutation_rows),
        "reference_eligible_population": len(reference_population),
        "reference_sample_count": len(reference_rows),
        "reference_affected_count": reference_affected,
        "reference_affected_rate": reference_affected / max(len(reference_rows), 1),
        "reference_max_logit_delta": reference_max,
        "digest_sample_count": len(digest_rows),
        "digest_affected_count": digest_affected,
        "digest_affected_rate": digest_affected / max(len(digest_rows), 1),
        "digest_max_output_delta": digest_max,
    }


def _bootstrap_lower_bound(records: list[dict[str, float | int]]) -> float:
    by_pair: dict[int, tuple[float, float]] = {}
    for record in records:
        pair = int(record["pair"])
        numerator, weight = by_pair.get(pair, (0.0, 0.0))
        row_weight = float(record["weight"])
        by_pair[pair] = (
            numerator + float(record["gain"]) * row_weight,
            weight + row_weight,
        )
    pairs = sorted(by_pair)
    rng = random.Random(SEED + 1_000)
    samples: list[float] = []
    for _ in range(BOOTSTRAP_REPLICATES):
        chosen = [pairs[rng.randrange(len(pairs))] for _ in pairs]
        numerator = sum(by_pair[pair][0] for pair in chosen)
        weight = sum(by_pair[pair][1] for pair in chosen)
        samples.append(numerator / max(weight, 1.0e-12))
    samples.sort()
    return samples[int(0.05 * len(samples))]


def run_screen(args: argparse.Namespace) -> dict[str, Any]:
    device = _configure(args.gpu)
    started = time.perf_counter()
    decisions, source = _load_decisions(args.cache)
    if args.batch_size not in PROFILE_BATCH_SIZES:
        _fail("batch size must be selected from the frozen throughput arms")
    root = args.output_root
    if root.exists():
        _fail(f"refusing to overwrite {root}")
    root.mkdir(parents=True)
    fold_reports: list[dict[str, Any]] = []
    internals: list[dict[str, Any]] = []
    for fold in range(4):
        fit = [decision for decision in decisions if decision.pair_index % 4 != fold]
        heldout = [decision for decision in decisions if decision.pair_index % 4 == fold]
        statistics = _install_advantages(fit, heldout)
        fold_started = time.perf_counter()
        model, training = _fit_fold(fit, args.batch_size, device, fold)
        trained = time.perf_counter()
        evaluation, internal = _evaluate(model, heldout, args.batch_size, device)
        diagnostics = _diagnostics(model, heldout, fold, device)
        state_path = root / f"fold-{fold}.state.pt"
        torch.save(model.state_dict(), state_path)
        report = {
            "schema": SCHEMA + ".fold",
            "fold": fold,
            "source_cache_sha256": EXPECTED_CACHE_SHA256,
            "fit_pairs": CORPUS_PAIR_COUNT * 3 // 4,
            "heldout_pairs": CORPUS_PAIR_COUNT // 4,
            "advantage_statistics": {str(k): v for k, v in statistics.items()},
            "training": training,
            "evaluation": evaluation,
            "diagnostics": diagnostics,
            "model_state_sha256": _state_sha256(model),
            "state_file_sha256": _sha256(state_path),
            "phase_seconds": {
                "train": trained - fold_started,
                "evaluate_and_checkpoint": time.perf_counter() - trained,
            },
        }
        _write_new(root / f"fold-{fold}.json", report)
        fold_reports.append(report)
        internals.append(internal)
        del model
        gc.collect()
        torch.cuda.empty_cache()

    surrogate_records = [
        record for internal in internals for record in internal["surrogate_records"]
    ]
    tv_samples = [sample for internal in internals for sample in internal["tv_samples"]]
    value_records = [
        record for internal in internals for record in internal["value_records"]
    ]

    def aggregate_surrogate(records: list[dict[str, float | int]]) -> dict[str, Any]:
        weight = sum(float(record["weight"]) for record in records)
        numerator = sum(
            float(record["gain"]) * float(record["weight"]) for record in records
        )
        return {
            "surrogate": numerator / max(weight, 1.0e-12),
            "numerator": numerator,
            "episode_mass": weight,
            "max_absolute_joint_log_ratio": max(
                (float(record["absolute_log_ratio"]) for record in records), default=0.0
            ),
        }

    def aggregate_value(records: list[dict[str, float | int]]) -> dict[str, Any]:
        weight = sum(float(record["weight"]) for record in records)
        parent = sum(
            float(record["parent_error"]) * float(record["weight"])
            for record in records
        ) / max(weight, 1.0e-12)
        candidate = sum(
            float(record["candidate_error"]) * float(record["weight"])
            for record in records
        ) / max(weight, 1.0e-12)
        return {
            "parent_mse": parent,
            "candidate_mse": candidate,
            "relative_improvement": (parent - candidate) / max(parent, 1.0e-12),
            "episode_mass": weight,
        }

    surrogate = {
        "overall": aggregate_surrogate(surrogate_records),
        "by_candidate_seat": {
            str(seat): aggregate_surrogate(
                [record for record in surrogate_records if record["seat"] == seat]
            )
            for seat in (0, 1)
        },
        "pair_bootstrap_90pct_lower_bound": _bootstrap_lower_bound(surrogate_records),
    }
    tv_weight = sum(weight for _, weight in tv_samples)
    movement = {
        "mean_total_variation": sum(value * weight for value, weight in tv_samples)
        / max(tv_weight, 1.0e-12),
        "p90_total_variation": _weighted_quantile(tv_samples, 0.90),
        "row_count": len(tv_samples),
    }
    value = {
        "overall": aggregate_value(value_records),
        "by_candidate_seat": {
            str(seat): aggregate_value(
                [record for record in value_records if record["seat"] == seat]
            )
            for seat in (0, 1)
        },
    }
    diagnostics = {
        "permutation_max_logit_delta": max(
            report["diagnostics"]["permutation_max_logit_delta"]
            for report in fold_reports
        ),
        "reference_affected_count": sum(
            report["diagnostics"]["reference_affected_count"]
            for report in fold_reports
        ),
        "reference_sample_count": sum(
            report["diagnostics"]["reference_sample_count"] for report in fold_reports
        ),
        "digest_affected_count": sum(
            report["diagnostics"]["digest_affected_count"] for report in fold_reports
        ),
        "digest_sample_count": sum(
            report["diagnostics"]["digest_sample_count"] for report in fold_reports
        ),
    }
    diagnostics["reference_affected_rate"] = diagnostics[
        "reference_affected_count"
    ] / max(diagnostics["reference_sample_count"], 1)
    diagnostics["digest_affected_rate"] = diagnostics["digest_affected_count"] / max(
        diagnostics["digest_sample_count"], 1
    )
    fold_surrogates = [
        report["evaluation"]["surrogate"]["overall"]["surrogate"]
        for report in fold_reports
    ]
    gates = {
        "overall_surrogate_positive": surrogate["overall"]["surrogate"] > 0.0,
        "both_seats_surrogate_nonnegative": all(
            surrogate["by_candidate_seat"][str(seat)]["surrogate"] >= 0.0
            for seat in (0, 1)
        ),
        "at_least_three_folds_positive": sum(value > 0.0 for value in fold_surrogates)
        >= 3,
        "bootstrap_lower_bound_positive": surrogate[
            "pair_bootstrap_90pct_lower_bound"
        ]
        > 0.0,
        "mean_tv_in_range": 0.01 <= movement["mean_total_variation"] <= 0.05,
        "p90_tv_at_most_0_15": movement["p90_total_variation"] <= 0.15,
        "max_joint_log_ratio_at_most_0_50": surrogate["overall"][
            "max_absolute_joint_log_ratio"
        ]
        <= 0.50,
        "value_improves_at_least_5pct": value["overall"]["relative_improvement"]
        >= 0.05,
        "neither_seat_value_regresses": all(
            value["by_candidate_seat"][str(seat)]["relative_improvement"] >= 0.0
            for seat in (0, 1)
        ),
        "permutation_delta_at_most_1e_5": diagnostics[
            "permutation_max_logit_delta"
        ]
        <= 1.0e-5,
        "reference_rate_at_least_20pct": diagnostics["reference_affected_rate"]
        >= 0.20,
        "digest_rate_at_least_20pct": diagnostics["digest_affected_rate"] >= 0.20,
    }
    aggregate = {
        "schema": SCHEMA + ".aggregate",
        "source": source,
        "config": {
            "dim": DIM,
            "history_length": HISTORY_LENGTH,
            "distill_epochs": DISTILL_EPOCHS,
            "outcome_epochs": OUTCOME_EPOCHS,
            "batch_size": args.batch_size,
            "learning_rate": LEARNING_RATE,
            "weight_decay": WEIGHT_DECAY,
            "ppo_clip": PPO_CLIP,
            "kl_coefficient": KL_COEFFICIENT,
            "value_coefficient": VALUE_COEFFICIENT,
            "seed": SEED,
            "gpu_ordinal": args.gpu,
        },
        "fold_surrogates": fold_surrogates,
        "surrogate": surrogate,
        "movement": movement,
        "value": value,
        "diagnostics": diagnostics,
        "gates": gates,
        "pass": all(gates.values()),
        "runtime_seconds": time.perf_counter() - started,
        "non_claims": [
            "fixed-corpus architecture screen only",
            "no native strength or promotion evidence",
            "no professional-level claim",
        ],
    }
    _write_new(root / "aggregate.json", aggregate)
    return aggregate


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    profile = subparsers.add_parser("profile")
    profile.add_argument("--cache", type=Path, required=True)
    profile.add_argument("--output", type=Path, required=True)
    profile.add_argument("--gpu", type=int, default=1)
    screen = subparsers.add_parser("screen")
    screen.add_argument("--cache", type=Path, required=True)
    screen.add_argument("--output-root", type=Path, required=True)
    screen.add_argument("--batch-size", type=int, required=True)
    screen.add_argument("--gpu", type=int, default=1)
    args = parser.parse_args()
    result = run_profile(args) if args.command == "profile" else run_screen(args)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
