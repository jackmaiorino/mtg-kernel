#!/usr/bin/env python3
"""GPU screen for a hard-projected recurrent residual on candidate-state CP7 labels."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
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
REPO = SCRIPT_DIR.parent.parent
RECURRENT_DIR = SCRIPT_DIR.parent / "recurrent_structured_learner_v1"
DAGGER_DIR = SCRIPT_DIR.parent / "cp7_dagger_shadow_v1"
sys.path.insert(0, str(RECURRENT_DIR))
sys.path.insert(0, str(DAGGER_DIR))

import fit_cp7_dagger_residual_v1 as dagger  # noqa: E402
from model_v1 import RecurrentStructuredActorCritic, pack_rows  # noqa: E402
from model_v2 import BISECTION_STEPS, JOINT_LOG_RATIO_BUDGET  # noqa: E402


SCHEMA = "mtg-kernel-recurrent-cp7-candidate-state-screen/v1"
SEED = 20_260_810
DIM = 128
EPOCHS = 12
LEARNING_RATE = 3.0e-4
WEIGHT_DECAY = 1.0e-4
GRADIENT_CAP = 5.0
PROFILE_BATCHES = (64, 128, 256)
PROFILE_PAIR_LIMIT = 64
PROFILE_WARMUP_STEPS = 2
PROFILE_MEASURED_STEPS = 8
PROFILE_MEMORY_LIMIT = 5 * 1024**3
RELATIVE_NLL_MIN = 0.05
TOP1_DELTA_MIN = 0.03
MEAN_TV_MAX = 0.03
P90_TV_MAX = 0.10
LOG_RATIO_MAX = 0.50


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _json_bytes(value: Any) -> bytes:
    return (
        json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n"
    ).encode("utf-8")


def _write_new(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(_json_bytes(value))


def _git_head() -> str:
    completed = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=REPO,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    return completed.stdout.strip()


def _configure(device_ordinal: int) -> torch.device:
    if not torch.cuda.is_available():
        _fail("CUDA is required")
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


def _load(
    corpus_report: Path, history_cache: Path, pair_limit: int | None = None
) -> tuple[list[Any], dict[str, Any], dict[str, float]]:
    started = time.perf_counter()
    labels, label_metadata = dagger._load_labels(corpus_report)
    decisions, cache_metadata, timings = dagger._load_decisions(history_cache, labels)
    if pair_limit is not None:
        decisions = [decision for decision in decisions if decision.pair_index < pair_limit]
    expected_pairs = set(range(pair_limit if pair_limit is not None else dagger.PAIR_COUNT))
    if {decision.pair_index for decision in decisions} != expected_pairs:
        _fail("loaded decision panel lacks exact pair coverage")
    if any(len(decision.rows) != 1 for decision in decisions):
        _fail("candidate-state corpus unexpectedly contains multi-substep decisions")
    metadata = {
        **label_metadata,
        **cache_metadata,
        "selected_pair_count": len(expected_pairs),
        "selected_label_count": len(decisions),
        "label_bearing_episode_count": len({decision.episode_key for decision in decisions}),
        "episodes_without_usable_labels": 2 * len(expected_pairs)
        - len({decision.episode_key for decision in decisions}),
        "weighting": "equal mass among label-bearing episodes",
    }
    timings["load_total_seconds"] = time.perf_counter() - started
    return decisions, metadata, timings


def _split(decisions: Iterable[Any]) -> tuple[list[Any], list[Any], list[Any]]:
    train = [decision for decision in decisions if decision.pair_index % 4 in (1, 2)]
    selection = [decision for decision in decisions if decision.pair_index % 4 == 3]
    heldout = [decision for decision in decisions if decision.pair_index % 4 == 0]
    keys = [set(decision.key for decision in lane) for lane in (train, selection, heldout)]
    if not train or not selection or not heldout or any(keys[i] & keys[j] for i in range(3) for j in range(i)):
        _fail("whole-pair split is empty or overlapping")
    return train, selection, heldout


def _new_model(device: torch.device) -> RecurrentStructuredActorCritic:
    torch.manual_seed(SEED)
    torch.cuda.manual_seed_all(SEED)
    model = RecurrentStructuredActorCritic(DIM).to(device)
    torch.nn.init.zeros_(model.policy_head.weight)
    torch.nn.init.zeros_(model.policy_head.bias)
    for parameter in model.value_head.parameters():
        parameter.requires_grad_(False)
    return model


def _project_with_scale(
    parent_logits: Tensor,
    raw_logits: Tensor,
    action_mask: Tensor,
    substep_count: Tensor,
) -> tuple[Tensor, Tensor]:
    parent_log_probability = torch.log_softmax(parent_logits, dim=1)
    delta = raw_logits - parent_logits
    low = torch.zeros((raw_logits.shape[0], 1), device=raw_logits.device)
    high = torch.ones_like(low)
    budget = (
        JOINT_LOG_RATIO_BUDGET
        / substep_count.to(raw_logits.dtype).clamp_min(1.0)
    ).unsqueeze(1)
    for _ in range(BISECTION_STEPS):
        middle = (low + high) * 0.5
        candidate = parent_logits + middle * delta
        candidate_log_probability = torch.log_softmax(candidate, dim=1)
        maximum = (
            (candidate_log_probability - parent_log_probability)
            .abs()
            .masked_fill(~action_mask, 0.0)
            .max(dim=1, keepdim=True)
            .values
        )
        within = maximum <= budget
        low = torch.where(within, middle, low)
        high = torch.where(within, high, middle)
    scale = low.detach()
    projected = parent_logits + scale * delta
    return projected.masked_fill(~action_mask, -1.0e9), scale.squeeze(1)


def _candidate_logits(
    model: RecurrentStructuredActorCritic, packed: Any
) -> tuple[Tensor, Tensor]:
    residual, _ = model(packed)
    raw = torch.where(
        packed.action_mask,
        packed.parent_logits + residual,
        packed.parent_logits,
    )
    return _project_with_scale(
        packed.parent_logits,
        raw,
        packed.action_mask,
        packed.substep_count,
    )


def _batch_loss(
    model: RecurrentStructuredActorCritic,
    decisions: list[Any],
    device: torch.device,
) -> Tensor:
    rows = [decision.rows[0] for decision in decisions]
    packed = pack_rows(rows, device)
    logits, _ = _candidate_logits(model, packed)
    teacher = torch.tensor(
        [int(row["teacher_selected_index"]) for row in rows],
        dtype=torch.long,
        device=device,
    )
    weights = torch.tensor(
        [float(decision.episode_weight) for decision in decisions],
        dtype=torch.float32,
        device=device,
    )
    nll = -torch.log_softmax(logits, dim=1).gather(1, teacher.unsqueeze(1)).squeeze(1)
    return (nll * weights).sum() / weights.sum().clamp_min(1.0e-12)


def _batches(
    decisions: list[Any], batch_size: int, rng: random.Random
) -> Iterable[list[Any]]:
    order = list(range(len(decisions)))
    rng.shuffle(order)
    for start in range(0, len(order), batch_size):
        yield [decisions[index] for index in order[start : start + batch_size]]


def _state_sha256(model: RecurrentStructuredActorCritic) -> str:
    digest = hashlib.sha256()
    for name, tensor in sorted(model.state_dict().items()):
        value = tensor.detach().cpu().contiguous()
        digest.update(name.encode("utf-8") + b"\0")
        digest.update(str(value.dtype).encode("ascii") + b"\0")
        digest.update(str(tuple(value.shape)).encode("ascii") + b"\0")
        digest.update(value.numpy().tobytes())
    return digest.hexdigest()


def _profile_arm(
    train: list[Any], batch_size: int, device: torch.device
) -> dict[str, Any]:
    model = _new_model(device)
    parameters = [parameter for parameter in model.parameters() if parameter.requires_grad]
    optimizer = torch.optim.AdamW(
        parameters, lr=LEARNING_RATE, weight_decay=WEIGHT_DECAY
    )
    rng = random.Random(SEED + batch_size)
    order = list(_batches(train, batch_size, rng))
    if not order:
        _fail("profile arm has no batches")
    torch.cuda.empty_cache()
    torch.cuda.reset_peak_memory_stats(device)
    losses: list[float] = []
    measured_rows = 0
    measured_seconds = 0.0
    for step in range(PROFILE_WARMUP_STEPS + PROFILE_MEASURED_STEPS):
        batch = order[step % len(order)]
        if step == PROFILE_WARMUP_STEPS:
            torch.cuda.synchronize(device)
            started = time.perf_counter()
        loss = _batch_loss(model, batch, device)
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        torch.nn.utils.clip_grad_norm_(parameters, GRADIENT_CAP)
        optimizer.step()
        if step >= PROFILE_WARMUP_STEPS:
            losses.append(float(loss.detach()))
            measured_rows += len(batch)
    torch.cuda.synchronize(device)
    measured_seconds = time.perf_counter() - started
    trace = hashlib.sha256(np.asarray(losses, dtype="<f8").tobytes()).hexdigest()
    return {
        "batch_size": batch_size,
        "measured_steps": PROFILE_MEASURED_STEPS,
        "physical_decisions": measured_rows,
        "seconds": measured_seconds,
        "physical_decisions_per_second": measured_rows / measured_seconds,
        "peak_allocated_bytes": torch.cuda.max_memory_allocated(device),
        "loss_trace_sha256": trace,
        "model_state_sha256": _state_sha256(model),
    }


def _weighted_quantile(samples: list[tuple[float, float]], quantile: float) -> float:
    ordered = sorted(samples)
    target = quantile * sum(weight for _, weight in ordered)
    cumulative = 0.0
    for value, weight in ordered:
        cumulative += weight
        if cumulative >= target:
            return value
    return ordered[-1][0]


def _empty_metric() -> dict[str, Any]:
    return {
        "parent_nll": 0.0,
        "candidate_nll": 0.0,
        "parent_top1": 0.0,
        "candidate_top1": 0.0,
        "tv": 0.0,
        "mass": 0.0,
        "tv_samples": [],
        "maximum_absolute_log_ratio": 0.0,
        "projection_scale": 0.0,
        "projection_scale_below_point_1": 0.0,
        "projection_scale_samples": [],
        "decisions": 0,
    }


def _finish_metric(raw: dict[str, Any]) -> dict[str, Any]:
    mass = max(float(raw["mass"]), 1.0e-12)
    parent_nll = float(raw["parent_nll"]) / mass
    candidate_nll = float(raw["candidate_nll"]) / mass
    parent_top1 = float(raw["parent_top1"]) / mass
    candidate_top1 = float(raw["candidate_top1"]) / mass
    return {
        "parent_selected_index_nll": parent_nll,
        "candidate_selected_index_nll": candidate_nll,
        "relative_nll_improvement": (parent_nll - candidate_nll) / max(parent_nll, 1.0e-12),
        "parent_top1": parent_top1,
        "candidate_top1": candidate_top1,
        "top1_delta": candidate_top1 - parent_top1,
        "mean_total_variation": float(raw["tv"]) / mass,
        "p90_total_variation": _weighted_quantile(raw["tv_samples"], 0.90),
        "maximum_absolute_log_ratio": float(raw["maximum_absolute_log_ratio"]),
        "mean_projection_scale": float(raw["projection_scale"]) / mass,
        "p10_projection_scale": _weighted_quantile(raw["projection_scale_samples"], 0.10),
        "p50_projection_scale": _weighted_quantile(raw["projection_scale_samples"], 0.50),
        "p90_projection_scale": _weighted_quantile(raw["projection_scale_samples"], 0.90),
        "fraction_projection_scale_below_point_1": float(
            raw["projection_scale_below_point_1"]
        ) / mass,
        "episode_mass": float(raw["mass"]),
        "physical_decisions": int(raw["decisions"]),
    }


def _evaluate(
    model: RecurrentStructuredActorCritic,
    decisions: list[Any],
    batch_size: int,
    device: torch.device,
) -> dict[str, Any]:
    raw = {"overall": _empty_metric(), "seats": {0: _empty_metric(), 1: _empty_metric()}}
    model.eval()
    with torch.no_grad():
        for start in range(0, len(decisions), batch_size):
            selected = decisions[start : start + batch_size]
            rows = [decision.rows[0] for decision in selected]
            packed = pack_rows(rows, device)
            candidate_logits, projection_scale = _candidate_logits(model, packed)
            parent_logp = torch.log_softmax(packed.parent_logits, dim=1)
            candidate_logp = torch.log_softmax(candidate_logits, dim=1)
            parent_probability = torch.softmax(packed.parent_logits, dim=1)
            candidate_probability = torch.softmax(candidate_logits, dim=1)
            teacher = torch.tensor(
                [int(row["teacher_selected_index"]) for row in rows],
                dtype=torch.long,
                device=device,
            )
            parent_nll = -parent_logp.gather(1, teacher.unsqueeze(1)).squeeze(1)
            candidate_nll = -candidate_logp.gather(1, teacher.unsqueeze(1)).squeeze(1)
            parent_top = packed.parent_logits.argmax(dim=1)
            candidate_top = candidate_logits.argmax(dim=1)
            tv = 0.5 * (parent_probability - candidate_probability).abs().sum(dim=1)
            maximum = (
                (candidate_logp - parent_logp)
                .abs()
                .masked_fill(~packed.action_mask, 0.0)
                .max(dim=1)
                .values
            )
            arrays = [
                value.detach().cpu().tolist()
                for value in (
                    parent_nll,
                    candidate_nll,
                    parent_top,
                    candidate_top,
                    tv,
                    maximum,
                    projection_scale,
                )
            ]
            for index, decision in enumerate(selected):
                weight = float(decision.episode_weight)
                label = int(rows[index]["teacher_selected_index"])
                for target in (raw["overall"], raw["seats"][decision.candidate_seat]):
                    target["parent_nll"] += arrays[0][index] * weight
                    target["candidate_nll"] += arrays[1][index] * weight
                    target["parent_top1"] += float(arrays[2][index] == label) * weight
                    target["candidate_top1"] += float(arrays[3][index] == label) * weight
                    target["tv"] += arrays[4][index] * weight
                    target["mass"] += weight
                    target["tv_samples"].append((arrays[4][index], weight))
                    target["maximum_absolute_log_ratio"] = max(
                        target["maximum_absolute_log_ratio"], arrays[5][index]
                    )
                    target["projection_scale"] += arrays[6][index] * weight
                    target["projection_scale_below_point_1"] += float(
                        arrays[6][index] < 0.1
                    ) * weight
                    target["projection_scale_samples"].append((arrays[6][index], weight))
                    target["decisions"] += 1
    return {
        "overall": _finish_metric(raw["overall"]),
        "by_candidate_seat": {
            str(seat): _finish_metric(raw["seats"][seat]) for seat in (0, 1)
        },
    }


def _gate(metrics: dict[str, Any]) -> dict[str, Any]:
    overall = metrics["overall"]
    seats = [metrics["by_candidate_seat"][str(seat)] for seat in (0, 1)]
    checks = {
        "overall_relative_nll_at_least_5_percent": overall["relative_nll_improvement"] >= RELATIVE_NLL_MIN,
        "overall_top1_delta_at_least_3pp": overall["top1_delta"] >= TOP1_DELTA_MIN,
        "both_seats_nonnegative_nll_improvement": all(row["relative_nll_improvement"] >= 0.0 for row in seats),
        "mean_tv_within_envelope": all(row["mean_total_variation"] <= MEAN_TV_MAX for row in [overall, *seats]),
        "p90_tv_within_envelope": all(row["p90_total_variation"] <= P90_TV_MAX for row in [overall, *seats]),
        "hard_log_ratio_envelope": max(row["maximum_absolute_log_ratio"] for row in [overall, *seats]) <= LOG_RATIO_MAX,
    }
    return {"checks": checks, "pass": all(checks.values())}


def _fit(
    train: list[Any],
    selection: list[Any],
    batch_size: int,
    device: torch.device,
) -> tuple[RecurrentStructuredActorCritic, list[dict[str, Any]], int]:
    model = _new_model(device)
    parameters = [parameter for parameter in model.parameters() if parameter.requires_grad]
    optimizer = torch.optim.AdamW(
        parameters, lr=LEARNING_RATE, weight_decay=WEIGHT_DECAY
    )
    rng = random.Random(SEED)
    best_score = (float("inf"), float("inf"))
    best_epoch = 0
    best_state: dict[str, Tensor] | None = None
    history: list[dict[str, Any]] = []
    for epoch in range(1, EPOCHS + 1):
        model.train()
        started = time.perf_counter()
        weighted_loss = 0.0
        weighted_mass = 0.0
        gradient_max = 0.0
        steps = 0
        for batch in _batches(train, batch_size, rng):
            loss = _batch_loss(model, batch, device)
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient = torch.nn.utils.clip_grad_norm_(parameters, GRADIENT_CAP)
            if not torch.isfinite(gradient):
                _fail("non-finite recurrent CP7 gradient")
            optimizer.step()
            mass = sum(float(decision.episode_weight) for decision in batch)
            weighted_loss += float(loss.detach()) * mass
            weighted_mass += mass
            gradient_max = max(gradient_max, float(gradient))
            steps += 1
        torch.cuda.synchronize(device)
        selection_metrics = _evaluate(model, selection, batch_size, device)
        overall_selection_nll = selection_metrics["overall"][
            "candidate_selected_index_nll"
        ]
        worst_seat_ratio = max(
            selection_metrics["by_candidate_seat"][str(seat)][
                "candidate_selected_index_nll"
            ]
            / selection_metrics["by_candidate_seat"][str(seat)][
                "parent_selected_index_nll"
            ]
            for seat in (0, 1)
        )
        checkpoint_score = (worst_seat_ratio, overall_selection_nll)
        if checkpoint_score < best_score:
            best_score = checkpoint_score
            best_epoch = epoch
            best_state = {
                name: tensor.detach().cpu().clone()
                for name, tensor in model.state_dict().items()
            }
        history.append(
            {
                "epoch": epoch,
                "train_selected_index_cross_entropy": weighted_loss / weighted_mass,
                "optimizer_steps": steps,
                "maximum_preclip_gradient_norm": gradient_max,
                "seconds_including_selection": time.perf_counter() - started,
                "selection": selection_metrics,
            }
        )
    if best_state is None:
        _fail("no checkpoint was selected")
    model.load_state_dict(best_state, strict=True)
    return model, history, best_epoch


def _toolchain(device: torch.device) -> dict[str, Any]:
    return {
        "python": platform.python_version(),
        "torch": torch.__version__,
        "cuda": torch.version.cuda,
        "cudnn": torch.backends.cudnn.version(),
        "gpu_ordinal": device.index,
        "gpu_name": torch.cuda.get_device_name(device),
        "gpu_total_bytes": torch.cuda.get_device_properties(device).total_memory,
    }


def _save_manifest(
    output_dir: Path,
    mode: str,
    source: dict[str, Any],
    device: torch.device,
    outputs: dict[str, str],
) -> None:
    manifest = {
        "schema": SCHEMA + ".manifest",
        "mode": mode,
        "git_commit": _git_head(),
        "seed": SEED,
        "toolchain": _toolchain(device),
        "inputs": {
            "corpus_report_sha256": source["corpus_report_sha256"],
            "history_cache_sha256": source["history_cache_sha256"],
            "teacher_task_sha256s": source["teacher_task_sha256s"],
        },
        "outputs": outputs,
    }
    _write_new(output_dir / "manifest.json", manifest)


def _run_profile(args: argparse.Namespace, device: torch.device) -> dict[str, Any]:
    decisions, source, load_timings = _load(
        args.corpus_report, args.history_cache, PROFILE_PAIR_LIMIT
    )
    train, selection, heldout = _split(decisions)
    arms = [_profile_arm(train, size, device) for size in PROFILE_BATCHES]
    best_rate = max(arm["physical_decisions_per_second"] for arm in arms)
    qualified = [
        arm for arm in arms
        if arm["peak_allocated_bytes"] <= PROFILE_MEMORY_LIMIT
        and arm["physical_decisions_per_second"] >= 0.95 * best_rate
    ]
    if not qualified:
        _fail("no throughput arm qualified")
    selected = max(qualified, key=lambda arm: arm["batch_size"])
    repeated = _profile_arm(train, selected["batch_size"], device)
    reproducible = (
        selected["loss_trace_sha256"] == repeated["loss_trace_sha256"]
        and selected["model_state_sha256"] == repeated["model_state_sha256"]
    )
    result = {
        "schema": SCHEMA + ".profile",
        "status": "PASS" if reproducible else "FAIL",
        "source": source,
        "load_timings": load_timings,
        "split": {
            "train": len(train), "selection": len(selection), "heldout": len(heldout)
        },
        "arms": arms,
        "selected_batch_size": selected["batch_size"],
        "selected_repeat": repeated,
        "exact_repeat": reproducible,
        "toolchain": _toolchain(device),
    }
    _write_new(args.output_dir / "report.json", result)
    report_sha = _sha256(args.output_dir / "report.json")
    _save_manifest(args.output_dir, "profile", source, device, {"report_sha256": report_sha})
    return result


def _run_formal(args: argparse.Namespace, device: torch.device) -> dict[str, Any]:
    started = time.perf_counter()
    decisions, source, load_timings = _load(args.corpus_report, args.history_cache)
    train, selection, heldout = _split(decisions)
    model, history, selected_epoch = _fit(
        train, selection, args.batch_size, device
    )
    heldout_metrics = _evaluate(model, heldout, args.batch_size, device)
    gate = _gate(heldout_metrics)
    state_sha = _state_sha256(model)
    model_path = args.output_dir / "model.pt"
    if model_path.exists():
        _fail(f"refusing to overwrite {model_path}")
    model_path.parent.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "schema": SCHEMA + ".model",
            "model_state_dict": {
                name: tensor.detach().cpu() for name, tensor in model.state_dict().items()
            },
            "model_state_sha256": state_sha,
            "selected_epoch": selected_epoch,
            "config": {"dim": DIM, "hard_log_ratio_budget": 0.49},
        },
        model_path,
    )
    result = {
        "schema": SCHEMA,
        "decision": "PASS" if gate["pass"] else "REJECT",
        "source": source,
        "load_timings": load_timings,
        "split": {
            "train_decisions": len(train),
            "selection_decisions": len(selection),
            "heldout_decisions": len(heldout),
            "train_pairs": len({decision.pair_index for decision in train}),
            "selection_pairs": len({decision.pair_index for decision in selection}),
            "heldout_pairs": len({decision.pair_index for decision in heldout}),
        },
        "config": {
            "seed": SEED,
            "dim": DIM,
            "epochs": EPOCHS,
            "batch_size": args.batch_size,
            "learning_rate": LEARNING_RATE,
            "weight_decay": WEIGHT_DECAY,
            "gradient_cap": GRADIENT_CAP,
            "hard_log_ratio_budget": 0.49,
            "checkpoint_selection": "lowest worst-seat candidate-to-parent selection NLL ratio, overall NLL tiebreak",
        },
        "selected_epoch": selected_epoch,
        "training_history": history,
        "heldout": heldout_metrics,
        "gate": gate,
        "model_state_sha256": state_sha,
        "toolchain": _toolchain(device),
        "total_seconds": time.perf_counter() - started,
        "non_claims": [
            "CP7 labels are supervision and not reward",
            "candidate-state imitation is not playing strength",
            "a pass authorizes larger label collection only",
            "terminal outcome remains the only promotion measure",
        ],
    }
    _write_new(args.output_dir / "report.json", result)
    outputs = {
        "report_sha256": _sha256(args.output_dir / "report.json"),
        "model_file_sha256": _sha256(model_path),
        "model_state_sha256": state_sha,
    }
    _save_manifest(args.output_dir, "formal", source, device, outputs)
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("profile", "formal"), required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument(
        "--corpus-report",
        type=Path,
        default=Path(r"D:\mtg-kernel-cp7-dagger-shadow-v1-synchronized\report.json"),
    )
    parser.add_argument(
        "--history-cache",
        type=Path,
        default=Path(r"D:\mtg-kernel-cp7-dagger-shadow-v1-synchronized\complete-history-cache.pt"),
    )
    parser.add_argument("--device", type=int, default=1)
    parser.add_argument("--batch-size", type=int, choices=PROFILE_BATCHES)
    args = parser.parse_args()
    if args.output_dir.exists() and any(args.output_dir.iterdir()):
        _fail("output directory must be absent or empty")
    if args.mode == "formal" and args.batch_size is None:
        _fail("formal mode requires the profiled --batch-size")
    device = _configure(args.device)
    result = _run_profile(args, device) if args.mode == "profile" else _run_formal(args, device)
    print(json.dumps(result, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
