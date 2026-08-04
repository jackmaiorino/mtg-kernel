#!/usr/bin/env python3
"""Screen an on-policy terminal correction from the CP7 recurrent encoder."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import subprocess
import sys
import time
from typing import Any, Iterable

os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")

import numpy as np
import torch
from torch import Tensor


SCRIPT_DIR = Path(__file__).resolve().parent
REPO = SCRIPT_DIR.parents[1]
RECURRENT_DIR = SCRIPT_DIR.parent / "recurrent_structured_learner_v1"
sys.path.insert(0, str(RECURRENT_DIR))
import run_screen_v1 as recurrent  # noqa: E402
from model_v1 import RecurrentStructuredActorCritic  # noqa: E402


SCHEMA = "mtg-kernel-recurrent-terminal-onpolicy-screen/v1"
CACHE_SCHEMA = "mtg-kernel-recurrent-terminal-onpolicy-cache/v1"
EXPECTED_PAIRS = 512
SEED = 20_260_813
DIM = 128
EPOCHS = 4
BATCH_SIZE = 256
LEARNING_RATE = 3.0e-4
WEIGHT_DECAY = 1.0e-4
GRADIENT_CAP = 5.0
PPO_CLIP = 0.10
KL_COEFFICIENT = 0.01
LOG_RATIO_BUDGET = 0.20
BISECTION_STEPS = 16
PROFILE_BATCHES = (128, 256)
PROFILE_PAIRS = 64
PROFILE_WARMUP_STEPS = 2
PROFILE_MEASURED_STEPS = 8
BOOTSTRAP_REPLICATES = 2_000
SOURCE_MODEL_SHA256 = "6c33f6d449b76e24c00bc7d46052b04488ddb9ec574009831d2fa90ea01bd55d"
SOURCE_STATE_SHA256 = "d736296425de2c438bb9be02ab6c89e51da4c17c1408de6ff3309029b2d06dca"


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _write_new(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def _git_head() -> str:
    return subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=REPO,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout.strip()


def _configure(device_ordinal: int) -> torch.device:
    if not torch.cuda.is_available() or device_ordinal != 1:
        _fail("exclusive physical GPU ordinal 1 is required")
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


def _state_sha256(state: dict[str, Tensor]) -> str:
    digest = hashlib.sha256()
    for name, tensor in sorted(state.items()):
        value = tensor.detach().cpu().contiguous()
        digest.update(name.encode("utf-8") + b"\0")
        digest.update(str(value.dtype).encode("ascii") + b"\0")
        digest.update(str(tuple(value.shape)).encode("ascii") + b"\0")
        digest.update(value.numpy().tobytes())
    return digest.hexdigest()


def _model_state_sha256(model: RecurrentStructuredActorCritic) -> str:
    return _state_sha256(dict(model.state_dict()))


def _new_model(source_model: Path, device: torch.device) -> RecurrentStructuredActorCritic:
    if _sha256(source_model) != SOURCE_MODEL_SHA256:
        _fail("source recurrent model SHA-256 mismatch")
    payload = torch.load(source_model, map_location="cpu", weights_only=False)
    state = payload.get("model_state_dict")
    if not isinstance(state, dict) or _state_sha256(state) != SOURCE_STATE_SHA256:
        _fail("source recurrent state SHA-256 mismatch")
    model = RecurrentStructuredActorCritic(DIM)
    model.load_state_dict(state, strict=True)
    torch.nn.init.zeros_(model.policy_head.weight)
    torch.nn.init.zeros_(model.policy_head.bias)
    for parameter in model.value_head.parameters():
        parameter.requires_grad_(False)
    return model.to(device)


def _load_decisions(
    cache: Path, *, pair_limit: int | None = None
) -> tuple[list[Any], dict[str, Any]]:
    recurrent.EXPECTED_CACHE_SHA256 = _sha256(cache)
    recurrent.EXPECTED_CACHE_SCHEMA = CACHE_SCHEMA
    recurrent.CORPUS_PAIR_COUNT = EXPECTED_PAIRS
    expected = None
    if pair_limit is None:
        expected = (set(range(EXPECTED_PAIRS)) - {58, 143}) | {514, 515}
    return recurrent._load_decisions(
        cache,
        pair_limit=pair_limit,
        expected_pair_indices=expected,
    )


def _project(
    behavior_logits: Tensor,
    raw_logits: Tensor,
    action_mask: Tensor,
    substep_count: Tensor,
) -> tuple[Tensor, Tensor]:
    behavior_logp = torch.log_softmax(behavior_logits, dim=1)
    delta = raw_logits - behavior_logits
    low = torch.zeros((raw_logits.shape[0], 1), device=raw_logits.device)
    high = torch.ones_like(low)
    per_substep = (
        LOG_RATIO_BUDGET / substep_count.to(raw_logits.dtype).clamp_min(1.0)
    ).unsqueeze(1)
    for _ in range(BISECTION_STEPS):
        middle = (low + high) * 0.5
        candidate = behavior_logits + middle * delta
        candidate_logp = torch.log_softmax(candidate, dim=1)
        maximum = (
            (candidate_logp - behavior_logp)
            .abs()
            .masked_fill(~action_mask, 0.0)
            .max(dim=1, keepdim=True)
            .values
        )
        within = maximum <= per_substep
        low = torch.where(within, middle, low)
        high = torch.where(within, high, middle)
    projected = behavior_logits + low.detach() * delta
    return projected.masked_fill(~action_mask, -1.0e9), low.squeeze(1)


def _candidate_logits(
    model: RecurrentStructuredActorCritic, packed: Any
) -> tuple[Tensor, Tensor]:
    residual, _ = model(packed)
    raw = torch.where(
        packed.action_mask,
        packed.parent_logits + residual,
        packed.parent_logits,
    )
    return _project(
        packed.parent_logits,
        raw,
        packed.action_mask,
        packed.substep_count,
    )


def _decision_batch(decisions: list[Any], device: torch.device) -> tuple[Any, dict[str, Tensor]]:
    return recurrent._decision_batch(decisions, device)


def _loss(
    model: RecurrentStructuredActorCritic,
    decisions: list[Any],
    device: torch.device,
) -> tuple[Tensor, dict[str, float]]:
    packed, tensors = _decision_batch(decisions, device)
    logits, _ = _candidate_logits(model, packed)
    selected = torch.log_softmax(logits, dim=1).gather(
        1, packed.selected_index.unsqueeze(1)
    ).squeeze(1)
    joint = torch.zeros((len(decisions),), device=device)
    joint.index_add_(0, tensors["decision_index"], selected)
    ratio = torch.exp(joint - tensors["old_joint"])
    clipped = torch.clamp(ratio, 1.0 - PPO_CLIP, 1.0 + PPO_CLIP)
    surrogate = torch.minimum(
        ratio * tensors["advantage"], clipped * tensors["advantage"]
    )
    weights = tensors["decision_weight"]
    actor = -(surrogate * weights).sum() / weights.sum().clamp_min(1.0e-12)
    behavior_logp = torch.log_softmax(packed.parent_logits, dim=1)
    candidate_logp = torch.log_softmax(logits, dim=1)
    behavior_probability = torch.softmax(packed.parent_logits, dim=1)
    row_kl = (
        behavior_probability * (behavior_logp - candidate_logp)
    ).sum(dim=1)
    row_weights = tensors["row_weight"]
    kl = (row_kl * row_weights).sum() / row_weights.sum().clamp_min(1.0e-12)
    return actor + KL_COEFFICIENT * kl, {
        "actor_loss": float(actor.detach()),
        "behavior_kl": float(kl.detach()),
        "clip_fraction": float(((ratio - 1.0).abs() > PPO_CLIP).float().mean()),
    }


def _batches(
    decisions: list[Any], batch_size: int, rng: random.Random
) -> Iterable[list[Any]]:
    order = list(range(len(decisions)))
    rng.shuffle(order)
    for start in range(0, len(order), batch_size):
        yield [decisions[index] for index in order[start : start + batch_size]]


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
    decisions: list[Any],
    device: torch.device,
) -> tuple[dict[str, Any], list[dict[str, float | int]]]:
    records: list[dict[str, float | int]] = []
    tv_samples: list[tuple[float, float]] = []
    model.eval()
    with torch.no_grad():
        for start in range(0, len(decisions), BATCH_SIZE):
            batch = decisions[start : start + BATCH_SIZE]
            packed, tensors = _decision_batch(batch, device)
            logits, scales = _candidate_logits(model, packed)
            selected = torch.log_softmax(logits, dim=1).gather(
                1, packed.selected_index.unsqueeze(1)
            ).squeeze(1)
            joint = torch.zeros((len(batch),), device=device)
            joint.index_add_(0, tensors["decision_index"], selected)
            log_ratio = joint - tensors["old_joint"]
            ratio = torch.exp(log_ratio)
            behavior_probability = torch.softmax(packed.parent_logits, dim=1)
            candidate_probability = torch.softmax(logits, dim=1)
            tv = 0.5 * (behavior_probability - candidate_probability).abs().sum(dim=1)
            row_cursor = 0
            for index, decision in enumerate(batch):
                weight = float(decision.episode_weight)
                records.append(
                    {
                        "pair": decision.pair_index,
                        "seat": decision.candidate_seat,
                        "gain": float((ratio[index] - 1.0) * tensors["advantage"][index]),
                        "weight": weight,
                        "absolute_log_ratio": abs(float(log_ratio[index])),
                    }
                )
                row_weight = weight / len(decision.rows)
                for offset in range(len(decision.rows)):
                    tv_samples.append((float(tv[row_cursor + offset]), row_weight))
                row_cursor += len(decision.rows)
            if not bool(torch.isfinite(scales).all()):
                _fail("non-finite projection scale")

    def summary(selected_records: list[dict[str, float | int]]) -> dict[str, Any]:
        weight = sum(float(record["weight"]) for record in selected_records)
        numerator = sum(
            float(record["gain"]) * float(record["weight"])
            for record in selected_records
        )
        return {
            "surrogate": numerator / max(weight, 1.0e-12),
            "episode_mass": weight,
            "physical_decisions": len(selected_records),
            "maximum_absolute_joint_log_ratio": max(
                (float(record["absolute_log_ratio"]) for record in selected_records),
                default=0.0,
            ),
        }

    movement_weight = sum(weight for _, weight in tv_samples)
    report = {
        "surrogate": {
            "overall": summary(records),
            "by_candidate_seat": {
                str(seat): summary(
                    [record for record in records if int(record["seat"]) == seat]
                )
                for seat in (0, 1)
            },
        },
        "movement": {
            "mean_total_variation": sum(value * weight for value, weight in tv_samples)
            / max(movement_weight, 1.0e-12),
            "p90_total_variation": _weighted_quantile(tv_samples, 0.90),
            "row_count": len(tv_samples),
        },
    }
    return report, records


def _bootstrap_lower(records: list[dict[str, float | int]]) -> float:
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
    samples = []
    for _ in range(BOOTSTRAP_REPLICATES):
        chosen = [pairs[rng.randrange(len(pairs))] for _ in pairs]
        numerator = sum(by_pair[pair][0] for pair in chosen)
        weight = sum(by_pair[pair][1] for pair in chosen)
        samples.append(numerator / max(weight, 1.0e-12))
    samples.sort()
    return samples[int(0.10 * len(samples))]


def _fit(
    model: RecurrentStructuredActorCritic,
    fit: list[Any],
    selection: list[Any],
    device: torch.device,
) -> tuple[RecurrentStructuredActorCritic, list[dict[str, Any]], int]:
    trainable = [parameter for parameter in model.parameters() if parameter.requires_grad]
    optimizer = torch.optim.AdamW(
        trainable, lr=LEARNING_RATE, weight_decay=WEIGHT_DECAY
    )
    rng = random.Random(SEED)
    initial_state = {
        name: tensor.detach().cpu().clone() for name, tensor in model.state_dict().items()
    }
    checkpoints: list[tuple[tuple[float, float], int, dict[str, Tensor]]] = []
    initial, _ = _evaluate(model, selection, device)
    checkpoints.append(((0.0, 0.0), 0, initial_state))
    history: list[dict[str, Any]] = [{"epoch": 0, "selection": initial}]
    for epoch in range(1, EPOCHS + 1):
        model.train()
        started = time.perf_counter()
        totals = {"loss": 0.0, "actor_loss": 0.0, "behavior_kl": 0.0, "clip_fraction": 0.0}
        gradient_max = 0.0
        steps = 0
        for batch in _batches(fit, BATCH_SIZE, rng):
            loss, parts = _loss(model, batch, device)
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient = torch.nn.utils.clip_grad_norm_(trainable, GRADIENT_CAP)
            if not torch.isfinite(gradient):
                _fail("non-finite terminal correction gradient")
            optimizer.step()
            totals["loss"] += float(loss.detach())
            for key, value in parts.items():
                totals[key] += value
            gradient_max = max(gradient_max, float(gradient))
            steps += 1
        torch.cuda.synchronize(device)
        evaluation, _ = _evaluate(model, selection, device)
        seats = evaluation["surrogate"]["by_candidate_seat"]
        rank = (
            min(float(seats["0"]["surrogate"]), float(seats["1"]["surrogate"])),
            float(evaluation["surrogate"]["overall"]["surrogate"]),
        )
        state = {
            name: tensor.detach().cpu().clone() for name, tensor in model.state_dict().items()
        }
        checkpoints.append((rank, epoch, state))
        history.append(
            {
                "epoch": epoch,
                "seconds_including_selection": time.perf_counter() - started,
                "optimizer_steps": steps,
                "maximum_preclip_gradient_norm": gradient_max,
                **{key: value / max(steps, 1) for key, value in totals.items()},
                "selection": evaluation,
            }
        )
    selected = max(checkpoints, key=lambda row: (row[0], -row[1]))
    model.load_state_dict(selected[2], strict=True)
    return model, history, selected[1]


def _profile_arm(
    source_model: Path,
    decisions: list[Any],
    batch_size: int,
    device: torch.device,
) -> dict[str, Any]:
    model = _new_model(source_model, device)
    recurrent._install_advantages(decisions)
    trainable = [parameter for parameter in model.parameters() if parameter.requires_grad]
    optimizer = torch.optim.AdamW(trainable, lr=LEARNING_RATE, weight_decay=WEIGHT_DECAY)
    batches = list(_batches(decisions, batch_size, random.Random(SEED + batch_size)))
    losses: list[float] = []
    measured = 0
    torch.cuda.empty_cache()
    torch.cuda.reset_peak_memory_stats(device)
    for step in range(PROFILE_WARMUP_STEPS + PROFILE_MEASURED_STEPS):
        if step == PROFILE_WARMUP_STEPS:
            torch.cuda.synchronize(device)
            started = time.perf_counter()
        batch = batches[step % len(batches)]
        loss, _ = _loss(model, batch, device)
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        torch.nn.utils.clip_grad_norm_(trainable, GRADIENT_CAP)
        optimizer.step()
        if step >= PROFILE_WARMUP_STEPS:
            losses.append(float(loss.detach()))
            measured += len(batch)
    torch.cuda.synchronize(device)
    seconds = time.perf_counter() - started
    return {
        "batch_size": batch_size,
        "measured_steps": PROFILE_MEASURED_STEPS,
        "physical_decisions": measured,
        "seconds": seconds,
        "physical_decisions_per_second": measured / seconds,
        "peak_allocated_bytes": torch.cuda.max_memory_allocated(device),
        "loss_trace_sha256": hashlib.sha256(
            np.asarray(losses, dtype="<f8").tobytes()
        ).hexdigest(),
        "model_state_sha256": _model_state_sha256(model),
    }


def _toolchain(device: torch.device) -> dict[str, Any]:
    return {
        "python": platform.python_version(),
        "torch": torch.__version__,
        "cuda": torch.version.cuda,
        "gpu_ordinal": device.index,
        "gpu_name": torch.cuda.get_device_name(device),
        "gpu_total_bytes": torch.cuda.get_device_properties(device).total_memory,
    }


def profile(args: argparse.Namespace) -> int:
    device = _configure(args.device)
    decisions, source = _load_decisions(args.cache)
    profile_pair_indices = sorted({decision.pair_index for decision in decisions})[
        :PROFILE_PAIRS
    ]
    profile_pair_set = set(profile_pair_indices)
    decisions = [
        decision for decision in decisions
        if decision.pair_index in profile_pair_set
    ]
    source = {
        **source,
        "profile_pair_count": len(profile_pair_indices),
        "profile_pair_indices": profile_pair_indices,
    }
    arms = [
        _profile_arm(args.source_model, decisions, batch_size, device)
        for batch_size in PROFILE_BATCHES
    ]
    best_rate = max(float(arm["physical_decisions_per_second"]) for arm in arms)
    qualified = [
        arm for arm in arms
        if float(arm["physical_decisions_per_second"]) >= 0.95 * best_rate
        and int(arm["peak_allocated_bytes"]) <= 5 * 1024**3
    ]
    selected = max(qualified, key=lambda arm: int(arm["batch_size"]))
    repeat = _profile_arm(
        args.source_model, decisions, int(selected["batch_size"]), device
    )
    deterministic = all(
        selected[key] == repeat[key]
        for key in ("loss_trace_sha256", "model_state_sha256")
    )
    report = {
        "schema": SCHEMA + ".profile",
        "source": source,
        "arms": arms,
        "selected_batch_size": selected["batch_size"],
        "repeat": repeat,
        "deterministic_repeat": deterministic,
        "toolchain": _toolchain(device),
        "status": "pass" if deterministic and selected["batch_size"] == BATCH_SIZE else "fail",
    }
    _write_new(args.output, report)
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0 if report["status"] == "pass" else 2


def screen(args: argparse.Namespace) -> int:
    device = _configure(args.device)
    started = time.perf_counter()
    decisions, source = _load_decisions(args.cache)
    fit = [decision for decision in decisions if decision.pair_index % 4 in (1, 2)]
    selection = [decision for decision in decisions if decision.pair_index % 4 == 3]
    heldout = [decision for decision in decisions if decision.pair_index % 4 == 0]
    statistics = recurrent._install_advantages(fit, [*selection, *heldout])
    model = _new_model(args.source_model, device)
    model, history, selected_epoch = _fit(model, fit, selection, device)
    heldout_report, records = _evaluate(model, heldout, device)
    lower = _bootstrap_lower(records)
    heldout_report["pair_bootstrap_80pct_lower_bound"] = lower
    overall = heldout_report["surrogate"]["overall"]
    seats = heldout_report["surrogate"]["by_candidate_seat"]
    movement = heldout_report["movement"]
    gates = {
        "nonzero_selected_epoch": selected_epoch > 0,
        "overall_surrogate_positive": float(overall["surrogate"]) > 0.0,
        "both_seats_surrogate_nonnegative": all(
            float(seats[str(seat)]["surrogate"]) >= 0.0 for seat in (0, 1)
        ),
        "bootstrap_lower_bound_positive": lower > 0.0,
        "mean_tv_in_range": 0.005 <= float(movement["mean_total_variation"]) <= 0.03,
        "p90_tv_at_most_0_10": float(movement["p90_total_variation"]) <= 0.10,
        "hard_joint_log_ratio_at_most_0_20": float(
            overall["maximum_absolute_joint_log_ratio"]
        ) <= LOG_RATIO_BUDGET + 1.0e-5,
    }
    state = {
        name: tensor.detach().cpu() for name, tensor in model.state_dict().items()
    }
    state_sha = _state_sha256(state)
    model_path = args.output_dir / "model.pt"
    args.output_dir.mkdir(parents=True)
    torch.save(
        {
            "schema": SCHEMA + ".model",
            "model_state_dict": state,
            "model_state_sha256": state_sha,
            "source_model_sha256": SOURCE_MODEL_SHA256,
            "selected_epoch": selected_epoch,
            "log_ratio_budget": LOG_RATIO_BUDGET,
        },
        model_path,
    )
    report = {
        "schema": SCHEMA,
        "decision": "PASS" if all(gates.values()) else "REJECT",
        "source": source,
        "split": {
            "fit_pairs": len({decision.pair_index for decision in fit}),
            "selection_pairs": len({decision.pair_index for decision in selection}),
            "heldout_pairs": len({decision.pair_index for decision in heldout}),
            "fit_physical_decisions": len(fit),
            "selection_physical_decisions": len(selection),
            "heldout_physical_decisions": len(heldout),
        },
        "config": {
            "seed": SEED,
            "epochs": EPOCHS,
            "batch_size": BATCH_SIZE,
            "learning_rate": LEARNING_RATE,
            "weight_decay": WEIGHT_DECAY,
            "gradient_cap": GRADIENT_CAP,
            "ppo_clip": PPO_CLIP,
            "kl_coefficient": KL_COEFFICIENT,
            "hard_behavior_log_ratio_budget": LOG_RATIO_BUDGET,
            "initialization": "CP7 recurrent encoder with zero terminal correction head",
            "reward": "natural terminal win draw or loss only",
        },
        "advantage_statistics": {str(key): value for key, value in statistics.items()},
        "selected_epoch": selected_epoch,
        "training_history": history,
        "heldout": heldout_report,
        "gates": gates,
        "model_state_sha256": state_sha,
        "model_file_sha256": _sha256(model_path),
        "git_commit": _git_head(),
        "toolchain": _toolchain(device),
        "total_seconds": time.perf_counter() - started,
        "non_claims": [
            "heldout importance surrogate is not playing-strength evidence",
            "a pass authorizes a full fit and fresh terminal gate only",
            "natural terminal win or loss remains the only promotion measure",
        ],
    }
    _write_new(args.output_dir / "report.json", report)
    _write_new(
        args.output_dir / "manifest.json",
        {
            "schema": SCHEMA + ".manifest",
            "git_commit": _git_head(),
            "seed": SEED,
            "toolchain": _toolchain(device),
            "inputs": {
                "cache_sha256": source["cache_sha256"],
                "source_model_sha256": SOURCE_MODEL_SHA256,
                "source_state_sha256": SOURCE_STATE_SHA256,
            },
            "outputs": {
                "report_sha256": _sha256(args.output_dir / "report.json"),
                "model_file_sha256": _sha256(model_path),
                "model_state_sha256": state_sha,
            },
        },
    )
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0 if report["decision"] == "PASS" else 2


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    profile_parser = subparsers.add_parser("profile")
    profile_parser.add_argument("--cache", type=Path, required=True)
    profile_parser.add_argument("--output", type=Path, required=True)
    screen_parser = subparsers.add_parser("screen")
    screen_parser.add_argument("--cache", type=Path, required=True)
    screen_parser.add_argument("--output-dir", type=Path, required=True)
    for child in (profile_parser, screen_parser):
        child.add_argument(
            "--source-model",
            type=Path,
            default=Path(r"D:\mtg-kernel-recurrent-cp7-dagger-v1\full-refit\model.pt"),
        )
        child.add_argument("--device", type=int, default=1)
    return parser.parse_args()


if __name__ == "__main__":
    try:
        parsed = arguments()
        raise SystemExit(profile(parsed) if parsed.command == "profile" else screen(parsed))
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"recurrent_terminal_onpolicy_screen_v1: ERROR: {error}", file=sys.stderr)
        raise SystemExit(2)
