#!/usr/bin/env python3
"""Profile, fit, and aggregate the scaled complete-history outcome screen."""

from __future__ import annotations

import argparse
import json
import random
import time
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import torch

import fit_policy_live_candidate as live
import run_screen as screen
import run_structured_outcome_policy_v1 as outcome


SCHEMA = "mtg-kernel-scaled-history-outcome-policy-screen/v1"
AGGREGATE_SCHEMA = SCHEMA + ".aggregate"
EXPECTED_CACHE_SHA256 = (
    "721aeeb8389464676edf1190b4e90d74ced286104cc0fb30deb46d36ffbc8090"
)
DIM = 48
CARD_VOCAB = 136
GROUP_VOCAB = 12
HISTORY_LENGTH = 16
HISTORY_FEATURE_DIM = screen.ACTION_EXPLICIT_DIM + 2 + CARD_VOCAB
EPOCHS = 5
BATCH_SIZE = 64
LR = 3.0e-4
WEIGHT_DECAY = 1.0e-4
CLIP = 0.10
GRAD_CAP = 5.0
SEED = 20_260_802
TARGET_MEAN_TV = 0.03
DIAGNOSTIC_SAMPLE_SIZE = 256
CALIBRATION_DECISIONS = 8_192


def _fail(message: str) -> None:
    raise ValueError(message)


def _write_new(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def _load_decisions(
    cache_path: Path,
    pair_limit: int | None,
    expected_cache_sha256: str,
    expected_pairs: int,
) -> tuple[list[outcome.PhysicalDecision], dict[str, Any], dict[str, float]]:
    started = time.perf_counter()
    cache_sha256 = outcome._sha256(cache_path)
    if cache_sha256 != expected_cache_sha256:
        _fail("complete-history cache SHA-256 mismatch")
    cache = torch.load(cache_path, map_location="cpu", weights_only=False)
    loaded = time.perf_counter()
    if (
        cache.get("version") != screen.SCRIPT_VERSION
        or not cache.get("complete_history_join")
    ):
        _fail("cache is not the validated complete-history corpus")
    examples = cache.get("value")
    if not isinstance(examples, list) or not examples:
        _fail("cache has no outcome examples")
    pair_indices = sorted({int(row["pair_index"]) for row in examples})
    if len(pair_indices) != expected_pairs:
        _fail(f"cache does not contain {expected_pairs} pairs")
    if pair_limit is not None:
        if pair_limit < 8 or pair_limit > len(pair_indices):
            _fail(f"pair limit must be between 8 and {expected_pairs}")
        selected = set(pair_indices[:pair_limit])
        examples = [row for row in examples if int(row["pair_index"]) in selected]
        pair_indices = pair_indices[:pair_limit]
    screen._attach_complete_action_history(
        [], examples, HISTORY_LENGTH, CARD_VOCAB
    )
    history_ready = time.perf_counter()
    decisions = outcome._physical_decisions(examples)
    grouped = time.perf_counter()
    metadata = {
        "cache": str(cache_path),
        "cache_sha256": cache_sha256,
        "outcome_jsonl_sha256": cache["source"]["outcome_sha256"],
        "pair_count": len(pair_indices),
        "episode_count": len({group.episode_key for group in decisions}),
        "row_count": len(examples),
        "physical_decision_count": len(decisions),
    }
    timings = {
        "hash_and_load_seconds": loaded - started,
        "attach_history_seconds": history_ready - loaded,
        "group_decisions_seconds": grouped - history_ready,
    }
    return decisions, metadata, timings


def _fit_args(epochs: int, threads: int) -> SimpleNamespace:
    return SimpleNamespace(
        lr=LR,
        weight_decay=WEIGHT_DECAY,
        seed=SEED,
        epochs=epochs,
        batch_size=BATCH_SIZE,
        clip=CLIP,
        grad_cap=GRAD_CAP,
    )


def _sample_decisions(
    decisions: list[outcome.PhysicalDecision], count: int, seed: int
) -> list[outcome.PhysicalDecision]:
    if len(decisions) <= count:
        return decisions
    return random.Random(seed).sample(decisions, count)


def run_fold(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    decisions, source, timings = _load_decisions(
        args.cache,
        args.pair_limit,
        args.expected_cache_sha256,
        args.expected_pairs,
    )
    fit = [group for group in decisions if group.pair_index % 4 != args.fold]
    heldout = [group for group in decisions if group.pair_index % 4 == args.fold]
    fit_episodes = {group.episode_key for group in fit}
    heldout_episodes = {group.episode_key for group in heldout}
    if not fit or not heldout or fit_episodes.intersection(heldout_episodes):
        _fail("fold lacks a disjoint fit or heldout split")
    statistics = outcome._advantage_statistics(fit)
    outcome._install_standardized_advantages(fit, statistics)
    outcome._install_standardized_advantages(heldout, statistics)
    screen._configure(SEED, args.threads)
    model = screen.StructuredAdapter(
        CARD_VOCAB,
        GROUP_VOCAB,
        DIM,
        HISTORY_LENGTH,
        HISTORY_FEATURE_DIM,
    )
    trained_started = time.perf_counter()
    history = outcome._fit_model(
        model, fit, _fit_args(args.epochs, args.threads)
    )
    trained = time.perf_counter()

    result: dict[str, Any] = {
        "schema": SCHEMA,
        "fold": args.fold,
        "profile_only": bool(args.profile_only),
        "source": source,
        "split": {
            "rule": "pair_index_mod_4",
            "fit_episode_count": len(fit_episodes),
            "heldout_episode_count": len(heldout_episodes),
            "fit_physical_decision_count": len(fit),
            "heldout_physical_decision_count": len(heldout),
        },
        "config": {
            "architecture": "complete-public-history-structured-outcome-policy-residual/v1",
            "dim": DIM,
            "card_vocab": CARD_VOCAB,
            "group_vocab": GROUP_VOCAB,
            "history_length": HISTORY_LENGTH,
            "history_feature_dim": HISTORY_FEATURE_DIM,
            "epochs": args.epochs,
            "batch_size_physical_decisions": BATCH_SIZE,
            "learning_rate": LR,
            "weight_decay": WEIGHT_DECAY,
            "ppo_clip": CLIP,
            "gradient_norm_cap": GRAD_CAP,
            "seed": SEED,
            "threads": args.threads,
            "target_fit_mean_total_variation": TARGET_MEAN_TV,
            "value_model": "exact-retained-parent-unchanged",
        },
        "advantage_statistics_by_candidate_seat": {
            str(key): value for key, value in statistics.items()
        },
        "training_history": history,
        "timings": {
            **timings,
            "train_seconds": trained - trained_started,
        },
    }
    if not args.profile_only:
        calibration = _sample_decisions(
            fit, CALIBRATION_DECISIONS, SEED + args.fold
        )
        parents, residuals, weights = outcome._row_movement_inputs(
            model, calibration
        )
        uncalibrated = live._movement(parents, residuals, weights, 1.0)
        scale, calibrated = live._calibrate(
            parents, residuals, weights, TARGET_MEAN_TV
        )
        with torch.no_grad():
            model.policy_head.weight.mul_(scale)
            model.policy_head.bias.mul_(scale)
        result.update(
            {
                "calibration": {
                    "decision_sample_count": len(calibration),
                    "scale": scale,
                    "uncalibrated_fit_movement": uncalibrated,
                    "calibrated_fit_movement": calibrated,
                },
                "heldout_surrogate": outcome._surrogate(model, heldout),
                "heldout_movement": outcome._movement(model, heldout),
                "diagnostics": outcome._diagnostics(
                    model,
                    heldout,
                    SEED + args.fold + 1_000,
                    DIAGNOSTIC_SAMPLE_SIZE,
                ),
            }
        )
        state_path = args.output.with_suffix(".state.pt")
        if state_path.exists():
            _fail(f"refusing to overwrite {state_path}")
        torch.save(model.state_dict(), state_path)
        result["model_state"] = {
            "path": str(state_path),
            "sha256": outcome._sha256(state_path),
        }
    result["timings"]["total_seconds"] = time.perf_counter() - started
    result["non_claims"] = [
        "development screen only",
        "parent-policy data surrogate is not a live win rate",
        "no promotion or pro-level claim",
    ]
    _write_new(args.output, result)
    return result


def aggregate(args: argparse.Namespace) -> dict[str, Any]:
    results = [json.loads(path.read_text(encoding="utf-8")) for path in args.fold_result]
    if (
        len(results) != 4
        or {result.get("fold") for result in results} != {0, 1, 2, 3}
        or any(result.get("schema") != SCHEMA or result.get("profile_only") for result in results)
    ):
        _fail("aggregate requires four non-profile fold results")
    cache_hashes = {result["source"]["cache_sha256"] for result in results}
    pair_counts = {result["source"]["pair_count"] for result in results}
    configs = {json.dumps(result["config"], sort_keys=True) for result in results}
    if (
        cache_hashes != {args.expected_cache_sha256}
        or pair_counts != {args.expected_pairs}
        or len(configs) != 1
    ):
        _fail("fold source or configuration mismatch")
    overall = outcome._combine_weighted_metric(results, None)
    by_seat = {
        seat: outcome._combine_weighted_metric(results, seat)
        for seat in ("0", "1")
    }
    tv_samples = [
        (float(value), float(weight))
        for result in results
        for value, weight in result["heldout_movement"]["tv_weighted_samples"]
    ]
    movement_weight = sum(weight for _, weight in tv_samples)
    movement = {
        "mean_total_variation": sum(value * weight for value, weight in tv_samples)
        / max(movement_weight, 1e-12),
        "p90_total_variation": outcome._weighted_quantile(tv_samples, 0.90),
        "episode_mass": movement_weight,
        "row_count": len(tv_samples),
        "max_absolute_physical_decision_joint_log_ratio": overall[
            "max_absolute_joint_log_ratio"
        ],
    }
    positive_folds = sum(
        result["heldout_surrogate"]["overall"]["surrogate"] > 0.0
        for result in results
    )
    diagnostics = {
        "permutation_max_logit_delta": max(
            result["diagnostics"]["permutation_max_logit_delta"]
            for result in results
        ),
        "reference_sample_count": sum(
            result["diagnostics"]["reference_sample_count"] for result in results
        ),
        "reference_affected_count": sum(
            result["diagnostics"]["reference_affected_count"] for result in results
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
        "mean_total_variation_ge_min": movement["mean_total_variation"]
        >= args.min_mean_tv,
        "mean_total_variation_le_max": movement["mean_total_variation"]
        <= args.max_mean_tv,
        "p90_total_variation_le_max": movement["p90_total_variation"]
        <= args.max_p90_tv,
        "max_absolute_joint_log_ratio_le_max": movement[
            "max_absolute_physical_decision_joint_log_ratio"
        ] <= args.max_joint_log_ratio,
        "permutation_max_delta_le_1e_5": diagnostics[
            "permutation_max_logit_delta"
        ] <= 1e-5,
        "reference_affected_rate_ge_20pct": diagnostics[
            "reference_affected_rate"
        ] >= 0.20,
    }
    result = {
        "schema": AGGREGATE_SCHEMA,
        "fold_results": [
            {"path": str(path), "sha256": outcome._sha256(path)}
            for path in args.fold_result
        ],
        "source_cache_sha256": args.expected_cache_sha256,
        "config": results[0]["config"],
        "heldout_surrogate": {"overall": overall, "by_candidate_seat": by_seat},
        "positive_fold_count": positive_folds,
        "fold_surrogates": {
            str(result["fold"]): result["heldout_surrogate"]["overall"]["surrogate"]
            for result in results
        },
        "heldout_movement": movement,
        "diagnostics": diagnostics,
        "gate_config": {
            "min_mean_total_variation": args.min_mean_tv,
            "max_mean_total_variation": args.max_mean_tv,
            "max_p90_total_variation": args.max_p90_tv,
            "max_absolute_joint_log_ratio": args.max_joint_log_ratio,
        },
        "gates": gates,
        "pass": all(gates.values()),
        "non_claims": [
            "development screen only",
            "a pass authorizes one fresh rapid matched strength gate, not promotion",
            "no cross-deck or pro-level claim",
        ],
    }
    _write_new(args.output, result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    fold = subparsers.add_parser("fold")
    fold.add_argument("--cache", type=Path, required=True)
    fold.add_argument("--output", type=Path, required=True)
    fold.add_argument("--fold", type=int, choices=range(4), required=True)
    fold.add_argument("--threads", type=int, default=6)
    fold.add_argument("--epochs", type=int, default=EPOCHS)
    fold.add_argument("--pair-limit", type=int)
    fold.add_argument("--profile-only", action="store_true")
    fold.add_argument(
        "--expected-cache-sha256", default=EXPECTED_CACHE_SHA256
    )
    fold.add_argument("--expected-pairs", type=int, default=2_048)
    agg = subparsers.add_parser("aggregate")
    agg.add_argument("--fold-result", action="append", type=Path, required=True)
    agg.add_argument("--output", type=Path, required=True)
    agg.add_argument(
        "--expected-cache-sha256", default=EXPECTED_CACHE_SHA256
    )
    agg.add_argument("--expected-pairs", type=int, default=2_048)
    agg.add_argument("--min-mean-tv", type=float, default=0.0)
    agg.add_argument("--max-mean-tv", type=float, default=0.03)
    agg.add_argument("--max-p90-tv", type=float, default=0.10)
    agg.add_argument("--max-joint-log-ratio", type=float, default=0.50)
    args = parser.parse_args()
    if args.command == "fold":
        if len(args.expected_cache_sha256) != 64 or args.expected_pairs < 8:
            _fail("expected cache SHA-256 or pair count is invalid")
        if args.threads < 1 or args.threads > 24:
            _fail("threads must be between 1 and 24")
        if args.profile_only:
            if args.pair_limit != 128 or args.epochs != 1:
                _fail("profile is fixed to 128 pairs and one epoch")
        elif args.pair_limit is not None or args.epochs != EPOCHS:
            _fail("formal folds require all pairs and five epochs")
        result = run_fold(args)
    else:
        if len(args.expected_cache_sha256) != 64 or args.expected_pairs < 8:
            _fail("expected cache SHA-256 or pair count is invalid")
        if len(args.fold_result) != 4:
            _fail("aggregate requires exactly four fold results")
        if not (
            0.0 <= args.min_mean_tv <= args.max_mean_tv
            and args.max_p90_tv > 0.0
            and args.max_joint_log_ratio > 0.0
        ):
            _fail("aggregate movement gates are invalid")
        result = aggregate(args)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
