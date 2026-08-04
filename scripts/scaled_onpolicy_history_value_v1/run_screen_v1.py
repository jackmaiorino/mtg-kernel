#!/usr/bin/env python3
"""Run the scaled on-policy complete-history value-bootstrap screen."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys
import time
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
STRUCTURED_DIR = SCRIPT_DIR.parent / "structured_adapter_screen_v1"
sys.path.insert(0, str(STRUCTURED_DIR))

import run_screen as screen  # noqa: E402
import run_structured_outcome_policy_v1 as outcome  # noqa: E402
import run_structured_successor_distillation_v1 as distill  # noqa: E402
import run_structured_value_bootstrap_v1 as prior_value  # noqa: E402


SCHEMA = "mtg-kernel-scaled-onpolicy-history-value-screen/v1"
AGGREGATE_SCHEMA = SCHEMA + ".aggregate"
CACHE_SHA256 = "454e4ce1b8f7413839a36c8e2731fc0cb65581ce13e593634bffa70013a6f16d"
INITIALIZER_STATE_SHA256 = "ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0"
PAIR_COUNT = 2_048
EPOCHS = 5
BATCH_SIZE = 32
LR = 3.0e-4
WEIGHT_DECAY = 1.0e-4
GRAD_CAP = 5.0
SEED = 20_260_809
DIAGNOSTIC_SAMPLE_SIZE = 256


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _write_new(path: Path, value: Any, compact: bool = False) -> None:
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


def _load_initializer(path: Path) -> tuple[Any, dict[str, Any]]:
    observed_sha256 = _sha256(path)
    if observed_sha256 != INITIALIZER_STATE_SHA256:
        _fail("qualified structured initializer SHA-256 mismatch")
    payload = torch.load(path, map_location="cpu", weights_only=False)
    state = payload.get("model_state_dict")
    if not isinstance(state, dict):
        _fail("qualified structured initializer lacks model_state_dict")
    model = distill._model()
    model.load_state_dict(state, strict=True)
    value_tensors = {
        name: tensor
        for name, tensor in model.state_dict().items()
        if name.startswith("value_head.")
    }
    if not value_tensors or any(torch.count_nonzero(tensor).item() for tensor in value_tensors.values()):
        _fail("qualified initializer value residual is not exactly zero")
    return model, {
        "path": str(path),
        "sha256": observed_sha256,
        "value_residual_zero": True,
        "architecture": "qualified-policy-only-structured-successor/v1",
    }


def _fit_args(args: argparse.Namespace) -> argparse.Namespace:
    return argparse.Namespace(
        lr=LR,
        weight_decay=WEIGHT_DECAY,
        seed=SEED,
        epochs=args.epochs,
        batch_size=BATCH_SIZE,
        grad_cap=GRAD_CAP,
    )


def _load_decisions(
    cache_path: Path, pair_limit: int | None
) -> tuple[list[Any], dict[str, Any], dict[str, float]]:
    started = time.perf_counter()
    cache_sha256 = _sha256(cache_path)
    if cache_sha256 != CACHE_SHA256:
        _fail("complete-history Pool3 cache SHA-256 mismatch")
    cache = torch.load(cache_path, map_location="cpu", weights_only=False)
    loaded = time.perf_counter()
    if cache.get("version") != screen.SCRIPT_VERSION or not cache.get(
        "complete_history_join"
    ):
        _fail("Pool3 cache is not the validated complete-history corpus")
    examples = cache.get("value")
    if not isinstance(examples, list) or not examples:
        _fail("Pool3 cache has no candidate value examples")
    pair_indices = sorted({int(row["pair_index"]) for row in examples})
    if len(pair_indices) != PAIR_COUNT:
        _fail("Pool3 cache does not contain exactly 2,048 pairs")
    if pair_limit is not None:
        selected = set(pair_indices[:pair_limit])
        examples = [row for row in examples if int(row["pair_index"]) in selected]
        pair_indices = pair_indices[:pair_limit]
    screen._attach_complete_action_history(  # noqa: SLF001
        [], examples, distill.HISTORY_LENGTH, distill.CARD_VOCAB
    )
    history_ready = time.perf_counter()
    decisions = outcome._physical_decisions(examples)  # noqa: SLF001
    grouped = time.perf_counter()
    return decisions, {
        "cache": str(cache_path),
        "cache_sha256": cache_sha256,
        "pair_count": len(pair_indices),
        "episode_count": len({decision.episode_key for decision in decisions}),
        "row_count": len(examples),
        "physical_decision_count": len(decisions),
        "cache_source_keys": sorted((cache.get("source") or {}).keys()),
    }, {
        "hash_and_load_seconds": loaded - started,
        "attach_history_seconds": history_ready - loaded,
        "group_decisions_seconds": grouped - history_ready,
    }


def run_fold(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    decisions, source, timings = _load_decisions(args.cache, args.pair_limit)
    pair_indices = sorted({decision.pair_index for decision in decisions})
    fit = [decision for decision in decisions if decision.pair_index % 4 != args.fold]
    heldout = [decision for decision in decisions if decision.pair_index % 4 == args.fold]
    fit_episodes = {decision.episode_key for decision in fit}
    heldout_episodes = {decision.episode_key for decision in heldout}
    expected_heldout_pairs = sum(index % 4 == args.fold for index in pair_indices)
    if (
        not fit
        or not heldout
        or fit_episodes.intersection(heldout_episodes)
        or len(heldout_episodes) != expected_heldout_pairs * 2
        or len(fit_episodes) != (len(pair_indices) - expected_heldout_pairs) * 2
    ):
        _fail("scaled value fold is not an exact disjoint whole-pair split")

    screen._configure(SEED, args.threads)  # noqa: SLF001
    model, initializer = _load_initializer(args.initializer_state)
    trained_started = time.perf_counter()
    training_history = prior_value._fit_value(model, fit, _fit_args(args))  # noqa: SLF001
    trained = time.perf_counter()
    heldout_metrics = prior_value._value_metrics(model, heldout)  # noqa: SLF001
    metrics_ready = time.perf_counter()
    diagnostics = prior_value._diagnostics(  # noqa: SLF001
        model,
        heldout,
        SEED + args.fold + 101,
        DIAGNOSTIC_SAMPLE_SIZE,
    )
    diagnostics_ready = time.perf_counter()

    state_path = args.output.with_suffix(".state.pt")
    if state_path.exists():
        _fail(f"refusing to overwrite {state_path}")
    state_path.parent.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "schema": SCHEMA + ".state",
            "fold": args.fold,
            "model_state_dict": model.state_dict(),
            "initializer_state_sha256": INITIALIZER_STATE_SHA256,
        },
        state_path,
    )
    result = {
        "schema": SCHEMA,
        "status": "complete",
        "fold": args.fold,
        "profile_only": bool(args.profile),
        "source": source,
        "initializer": initializer,
        "split": {
            "rule": "whole-pair-index-modulo-4/v1",
            "pair_count": len(pair_indices),
            "fit_episode_count": len(fit_episodes),
            "heldout_episode_count": len(heldout_episodes),
            "fit_physical_decision_count": len(fit),
            "heldout_physical_decision_count": len(heldout),
        },
        "config": {
            "architecture": "width48-complete-public-history-structured-terminal-value-residual/v1",
            "history_length": distill.HISTORY_LENGTH,
            "history_feature_dim": distill.HISTORY_FEATURE_DIM,
            "epochs": args.epochs,
            "batch_size_physical_decisions": BATCH_SIZE,
            "learning_rate": LR,
            "weight_decay": WEIGHT_DECAY,
            "gradient_norm_cap": GRAD_CAP,
            "seed": SEED,
            "threads": args.threads,
            "target": "actor-relative-natural-terminal-win-loss-draw-only/v1",
            "baseline": "exact-retained-parent-value",
            "policy_head": "frozen-separate-value-model",
        },
        "training_history": training_history,
        "heldout": heldout_metrics,
        "diagnostics": diagnostics,
        "model_state": {"path": str(state_path), "sha256": _sha256(state_path)},
        "timings": {
            **timings,
            "train_seconds": trained - trained_started,
            "heldout_metrics_seconds": metrics_ready - trained,
            "diagnostics_seconds": diagnostics_ready - metrics_ready,
            "total_seconds": time.perf_counter() - started,
        },
        "nonclaims": [
            "development value bootstrap only",
            "no policy package or strength evidence",
            "no promotion or pro-level claim",
        ],
    }
    _write_new(args.output, result, compact=True)
    return result


def _aggregate_subset(results: list[dict[str, Any]], seat: str | None) -> dict[str, float]:
    records = [
        result["heldout"]["overall"]
        if seat is None
        else result["heldout"]["by_candidate_seat"][seat]
        for result in results
    ]
    weight = sum(float(record["episode_mass"]) for record in records)
    parent_numerator = sum(float(record["parent_numerator"]) for record in records)
    candidate_numerator = sum(float(record["candidate_numerator"]) for record in records)
    parent_mse = parent_numerator / weight
    candidate_mse = candidate_numerator / weight
    return {
        "parent_mse": parent_mse,
        "candidate_mse": candidate_mse,
        "relative_improvement": (parent_mse - candidate_mse) / parent_mse,
        "episode_mass": weight,
        "max_absolute_prediction": max(float(record["max_absolute_prediction"]) for record in records),
    }


def aggregate(args: argparse.Namespace) -> dict[str, Any]:
    results = [json.loads(path.read_text(encoding="utf-8")) for path in args.fold_result]
    if (
        len(results) != 4
        or {result.get("fold") for result in results} != {0, 1, 2, 3}
        or any(result.get("schema") != SCHEMA for result in results)
        or any(result.get("profile_only") for result in results)
        or any(result["split"]["pair_count"] != PAIR_COUNT for result in results)
    ):
        _fail("aggregate requires exactly four complete formal folds")
    configs = [
        {key: value for key, value in result["config"].items() if key != "threads"}
        for result in results
    ]
    if len({json.dumps(config, sort_keys=True) for config in configs}) != 1:
        _fail("formal value fold configurations differ")

    overall = _aggregate_subset(results, None)
    by_seat = {seat: _aggregate_subset(results, seat) for seat in ("0", "1")}
    movement_samples = [
        (float(value), float(weight))
        for result in results
        for value, weight in result["heldout"]["movement"]["weighted_samples"]
    ]
    movement_weight = sum(weight for _, weight in movement_samples)
    movement = {
        "mean_absolute_residual": sum(value * weight for value, weight in movement_samples)
        / movement_weight,
        "p90_absolute_residual": outcome._weighted_quantile(movement_samples, 0.90),  # noqa: SLF001
        "episode_mass": movement_weight,
    }
    fold_improvements = {
        str(result["fold"]): float(result["heldout"]["overall"]["relative_improvement"])
        for result in results
    }
    positive_folds = sum(value > 0.0 for value in fold_improvements.values())
    diagnostics = {
        "permutation_max_value_delta": max(
            float(result["diagnostics"]["permutation_max_value_delta"])
            for result in results
        ),
        "reference_sample_count": sum(
            int(result["diagnostics"]["reference_sample_count"]) for result in results
        ),
        "reference_affected_count": sum(
            int(result["diagnostics"]["reference_affected_count"]) for result in results
        ),
    }
    diagnostics["reference_affected_rate"] = diagnostics["reference_affected_count"] / diagnostics[
        "reference_sample_count"
    ]
    gates = {
        "aggregate_mse_improvement_at_least_5_percent": overall["relative_improvement"] >= 0.05,
        "no_candidate_seat_regression_over_2_percent": all(
            by_seat[seat]["relative_improvement"] >= -0.02 for seat in ("0", "1")
        ),
        "at_least_three_of_four_folds_positive": positive_folds >= 3,
        "mean_absolute_residual_at_most_0p25": movement["mean_absolute_residual"] <= 0.25,
        "p90_absolute_residual_at_most_0p50": movement["p90_absolute_residual"] <= 0.50,
        "maximum_absolute_prediction_at_most_1p50": overall["max_absolute_prediction"] <= 1.50,
        "permutation_max_delta_at_most_1e_5": diagnostics["permutation_max_value_delta"] <= 1e-5,
        "reference_affected_rate_at_least_20_percent": diagnostics["reference_affected_rate"] >= 0.20,
    }
    gates = {name: bool(value) for name, value in gates.items()}
    result = {
        "schema": AGGREGATE_SCHEMA,
        "status": "pass" if all(gates.values()) else "reject",
        "fold_results": [
            {"path": str(path), "sha256": _sha256(path)} for path in args.fold_result
        ],
        "config": configs[0],
        "heldout": {"overall": overall, "by_candidate_seat": by_seat},
        "fold_improvements": fold_improvements,
        "positive_fold_count": positive_folds,
        "movement": movement,
        "diagnostics": diagnostics,
        "gates": {**gates, "value_bootstrap_pass": all(gates.values())},
        "total_fold_wall_seconds": max(float(result["timings"]["total_seconds"]) for result in results),
        "interpretation": (
            "Pass authorizes a fresh learned-value short-horizon search mechanism screen only."
            if all(gates.values())
            else "The scaled width-48 complete-history value bootstrap is rejected."
        ),
        "nonclaims": [
            "terminal-value development screen only",
            "no policy or playing-strength result",
            "no promotion or pro-level claim",
        ],
    }
    _write_new(args.output, result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    fold = subparsers.add_parser("fold")
    fold.add_argument("--cache", type=Path, required=True)
    fold.add_argument("--initializer-state", type=Path, required=True)
    fold.add_argument("--fold", type=int, choices=range(4), required=True)
    fold.add_argument("--output", type=Path, required=True)
    fold.add_argument("--threads", type=int, required=True)
    fold.add_argument("--epochs", type=int, required=True)
    fold.add_argument("--pair-limit", type=int)
    fold.add_argument("--profile", action="store_true")
    aggregate_parser = subparsers.add_parser("aggregate")
    aggregate_parser.add_argument("--fold-result", action="append", type=Path, required=True)
    aggregate_parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    if args.command == "fold":
        profile_ok = args.profile and args.pair_limit == 128 and args.epochs == 1
        formal_ok = not args.profile and args.pair_limit is None and args.epochs == EPOCHS
        if (
            not (profile_ok or formal_ok)
            or args.threads < 1
            or args.cache != Path(r"D:\mtg-kernel-policy-only-structured-terminal-rung-v1\formal\cache.pt")
            or args.initializer_state
            != Path(r"D:\mtg-kernel-policy-only-structured-successor-v1\candidate.state.pt")
        ):
            _fail("value fold invocation differs from the fixed profile or formal contract")
        result = run_fold(args)
        summary = {
            "fold": result["fold"],
            "profile_only": result["profile_only"],
            "relative_improvement": result["heldout"]["overall"]["relative_improvement"],
            "train_seconds": result["timings"]["train_seconds"],
            "total_seconds": result["timings"]["total_seconds"],
            "output": str(args.output),
        }
    else:
        result = aggregate(args)
        summary = {
            "status": result["status"],
            "relative_improvement": result["heldout"]["overall"]["relative_improvement"],
            "positive_folds": result["positive_fold_count"],
            "output": str(args.output),
        }
    print(json.dumps(summary, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
