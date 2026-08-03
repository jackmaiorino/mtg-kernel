#!/usr/bin/env python3
"""Rapid dense distillation screen for a standalone structured successor."""

from __future__ import annotations

import argparse
import json
import math
import random
import sys
import time
from pathlib import Path
from types import SimpleNamespace
from typing import Any, Iterable

import torch

import run_scaled_history_outcome_policy_v1 as scaled
import run_screen as screen


SCHEMA = "mtg-kernel-structured-successor-distillation-screen/v1"
AGGREGATE_SCHEMA = SCHEMA + ".aggregate"
EXPECTED_CACHE_SHA256 = (
    "280e34cd7f685beaf52c1cab3b41c53613a5029c063871942f48c063b6f5996f"
)
EXPECTED_PAIRS = 2_048
DIM = 48
CARD_VOCAB = 136
GROUP_VOCAB = 12
HISTORY_LENGTH = 16
HISTORY_FEATURE_DIM = screen.ACTION_EXPLICIT_DIM + 2 + CARD_VOCAB
EPOCHS = 5
BATCH_SIZE = 64
LR = 3.0e-4
WEIGHT_DECAY = 1.0e-4
GRAD_CAP = 5.0
SEED = 20_260_803
FOLDS = 4


def _fail(message: str) -> None:
    raise ValueError(message)


def _write_new(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, sort_keys=True, allow_nan=False, indent=2) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def _episode_weights(decisions: Iterable[Any]) -> dict[tuple[Any, ...], tuple[float, float]]:
    """Return (decision mass, substep mass) for each physical decision."""
    materialized = list(decisions)
    counts: dict[Any, int] = {}
    for decision in materialized:
        counts[decision.episode_key] = counts.get(decision.episode_key, 0) + 1
    result: dict[tuple[Any, ...], tuple[float, float]] = {}
    for decision in materialized:
        decision_mass = 1.0 / float(counts[decision.episode_key])
        result[decision.key] = (
            decision_mass,
            decision_mass / float(len(decision.rows)),
        )
    return result


def _model() -> screen.StructuredAdapter:
    return screen.StructuredAdapter(
        CARD_VOCAB,
        GROUP_VOCAB,
        DIM,
        HISTORY_LENGTH,
        HISTORY_FEATURE_DIM,
        False,
    )


def _load_decisions(
    cache_path: Path,
    pair_limit: int | None,
) -> tuple[list[Any], dict[str, Any], dict[str, float]]:
    started = time.perf_counter()
    cache_sha256 = scaled.outcome._sha256(cache_path)
    if cache_sha256 != EXPECTED_CACHE_SHA256:
        _fail("complete-history cache SHA-256 mismatch")
    cache = torch.load(cache_path, map_location="cpu", weights_only=False)
    loaded = time.perf_counter()
    if (
        cache.get("version") != screen.SCRIPT_VERSION
        or not cache.get("complete_history_join")
    ):
        _fail("cache is not the validated complete-history corpus")
    policy = cache.get("policy")
    value = cache.get("value")
    if not isinstance(policy, list) or not policy or not isinstance(value, list) or not value:
        _fail("cache must contain both public-action lanes")
    policy_pairs = sorted({int(row["pair_index"]) for row in policy})
    value_pairs = sorted({int(row["pair_index"]) for row in value})
    if policy_pairs != list(range(EXPECTED_PAIRS)) or value_pairs != policy_pairs:
        _fail("cache lanes do not contain the exact 2,048-pair panel")
    selected_pairs = policy_pairs
    if pair_limit is not None:
        if pair_limit < 8 or pair_limit > EXPECTED_PAIRS:
            _fail("pair limit must be between 8 and 2,048")
        selected_pairs = policy_pairs[:pair_limit]
        selected = set(selected_pairs)
        policy = [row for row in policy if int(row["pair_index"]) in selected]
        value = [row for row in value if int(row["pair_index"]) in selected]
    screen._attach_complete_action_history(
        policy, value, HISTORY_LENGTH, CARD_VOCAB
    )
    history_ready = time.perf_counter()
    decisions = scaled.outcome._physical_decisions(value)
    grouped = time.perf_counter()
    metadata = {
        "cache": str(cache_path),
        "cache_sha256": cache_sha256,
        "teacher_jsonl_sha256": cache["source"]["teacher_sha256"],
        "outcome_jsonl_sha256": cache["source"]["outcome_sha256"],
        "history_sources": "candidate_and_population_public_actions",
        "pair_count": len(selected_pairs),
        "episode_count": len({group.episode_key for group in decisions}),
        "row_count": len(value),
        "physical_decision_count": len(decisions),
    }
    timings = {
        "hash_and_load_seconds": loaded - started,
        "attach_history_seconds": history_ready - loaded,
        "group_decisions_seconds": grouped - history_ready,
    }
    return decisions, metadata, timings


def _fit_args(args: argparse.Namespace) -> SimpleNamespace:
    return SimpleNamespace(
        epochs=args.epochs,
        batch_size=BATCH_SIZE,
        lr=LR,
        weight_decay=WEIGHT_DECAY,
        grad_cap=GRAD_CAP,
    )


def _fit_model(
    model: screen.StructuredAdapter,
    decisions: list[Any],
    args: argparse.Namespace,
) -> list[dict[str, float | int]]:
    if not decisions:
        _fail("fit split is empty")
    weights = _episode_weights(decisions)
    optimizer = torch.optim.AdamW(
        model.parameters(), lr=LR, weight_decay=WEIGHT_DECAY
    )
    rng = random.Random(args.seed)
    history: list[dict[str, float | int]] = []
    for epoch in range(args.epochs):
        order = list(range(len(decisions)))
        rng.shuffle(order)
        model.train()
        policy_total = 0.0
        value_total = 0.0
        steps = 0
        for start in range(0, len(order), BATCH_SIZE):
            batch = [decisions[index] for index in order[start : start + BATCH_SIZE]]
            policy_losses: list[torch.Tensor] = []
            value_losses: list[torch.Tensor] = []
            policy_weights: list[float] = []
            value_weights: list[float] = []
            for decision in batch:
                decision_mass, row_mass = weights[decision.key]
                for row_index, row in enumerate(decision.rows):
                    # _one is deliberately used instead of forward. Its output is
                    # the standalone student's absolute logits and value; forward
                    # would add the retained parent and create a residual model.
                    student_logits, student_value = model._one(row)
                    teacher_probability = torch.softmax(row["old_logits"].double(), dim=0)
                    student_log_probability = torch.log_softmax(
                        student_logits.double(), dim=0
                    )
                    policy_losses.append(
                        torch.nn.functional.kl_div(
                            student_log_probability,
                            teacher_probability,
                            reduction="sum",
                        )
                    )
                    policy_weights.append(row_mass)
                    if row_index == 0:
                        value_losses.append(
                            torch.nn.functional.mse_loss(
                                student_value.float(), row["old_value"].float()
                            )
                        )
                        value_weights.append(decision_mass)
            policy_weight_tensor = torch.tensor(policy_weights, dtype=torch.float32)
            value_weight_tensor = torch.tensor(value_weights, dtype=torch.float32)
            policy_loss = (torch.stack(policy_losses) * policy_weight_tensor).sum()
            policy_loss = policy_loss / policy_weight_tensor.sum()
            value_loss = (torch.stack(value_losses) * value_weight_tensor).sum()
            value_loss = value_loss / value_weight_tensor.sum()
            loss = policy_loss + value_loss
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient_norm = torch.nn.utils.clip_grad_norm_(
                list(model.parameters()), GRAD_CAP
            )
            if not torch.isfinite(gradient_norm):
                _fail("non-finite training gradient")
            optimizer.step()
            policy_total += float(policy_loss.detach())
            value_total += float(value_loss.detach())
            steps += 1
        history.append(
            {
                "epoch": epoch + 1,
                "mean_weighted_policy_kl": policy_total / max(steps, 1),
                "mean_weighted_value_mse": value_total / max(steps, 1),
                "optimizer_steps": steps,
            }
        )
    return history


def _weighted_quantile(samples: list[tuple[float, float]], quantile: float) -> float:
    if not samples:
        _fail("weighted quantile requires samples")
    if not 0.0 <= quantile <= 1.0:
        _fail("quantile must be between zero and one")
    ordered = sorted((float(value), float(weight)) for value, weight in samples)
    total = sum(weight for _, weight in ordered)
    if total <= 0.0:
        _fail("weighted quantile requires positive mass")
    threshold = total * quantile
    cumulative = 0.0
    for value, weight in ordered:
        cumulative += weight
        if cumulative >= threshold:
            return value
    return ordered[-1][0]


def _empty_metric() -> dict[str, Any]:
    return {
        "policy_kl_numerator": 0.0,
        "tv_numerator": 0.0,
        "top_action_numerator": 0.0,
        "policy_mass": 0.0,
        "value_squared_error_numerator": 0.0,
        "value_mass": 0.0,
        "policy_row_count": 0,
        "physical_decision_count": 0,
        "episode_keys": set(),
        "tv_weighted_samples": [],
    }


def _finish_metric(raw: dict[str, Any]) -> dict[str, Any]:
    policy_mass = float(raw["policy_mass"])
    value_mass = float(raw["value_mass"])
    if policy_mass <= 0.0:
        p90 = 0.0
    else:
        p90 = _weighted_quantile(raw["tv_weighted_samples"], 0.90)
    return {
        "weighted_mean_kl": raw["policy_kl_numerator"] / max(policy_mass, 1e-12),
        "mean_total_variation": raw["tv_numerator"] / max(policy_mass, 1e-12),
        "p90_total_variation": p90,
        "top_action_agreement": raw["top_action_numerator"] / max(policy_mass, 1e-12),
        "value_rmse": math.sqrt(
            raw["value_squared_error_numerator"] / max(value_mass, 1e-12)
        ),
        "counts": {
            "episodes": len(raw["episode_keys"]),
            "physical_decisions": raw["physical_decision_count"],
            "policy_rows": raw["policy_row_count"],
        },
        "mass": {
            "policy": policy_mass,
            "value": value_mass,
        },
        "_sums": {
            key: raw[key]
            for key in (
                "policy_kl_numerator",
                "tv_numerator",
                "top_action_numerator",
                "policy_mass",
                "value_squared_error_numerator",
                "value_mass",
            )
        },
    }


def _metrics(
    model: screen.StructuredAdapter,
    decisions: list[Any],
    include_samples: bool = True,
) -> dict[str, Any]:
    if not decisions:
        _fail("heldout split is empty")
    weights = _episode_weights(decisions)
    raw_by_seat = {0: _empty_metric(), 1: _empty_metric()}
    raw_overall = _empty_metric()
    model.eval()
    with torch.no_grad():
        for decision in decisions:
            decision_mass, row_mass = weights[decision.key]
            target = raw_by_seat[decision.candidate_seat]
            for row_index, row in enumerate(decision.rows):
                student_logits, student_value = model._one(row)
                teacher_probability = torch.softmax(row["old_logits"].double(), dim=0)
                student_probability = torch.softmax(student_logits.double(), dim=0)
                kl = float(
                    (
                        teacher_probability
                        * (
                            teacher_probability.clamp_min(1e-300).log()
                            - student_probability.clamp_min(1e-300).log()
                        )
                    ).sum()
                )
                tv = float(0.5 * (teacher_probability - student_probability).abs().sum())
                top = float(int(student_logits.argmax()) == int(row["old_logits"].argmax()))
                for raw in (target, raw_overall):
                    raw["policy_kl_numerator"] += kl * row_mass
                    raw["tv_numerator"] += tv * row_mass
                    raw["top_action_numerator"] += top * row_mass
                    raw["policy_mass"] += row_mass
                    raw["policy_row_count"] += 1
                    raw["episode_keys"].add(decision.episode_key)
                    if include_samples:
                        raw["tv_weighted_samples"].append((tv, row_mass))
                if row_index == 0:
                    squared_error = float(
                        torch.nn.functional.mse_loss(
                            student_value.float(), row["old_value"].float()
                        )
                    )
                    for raw in (target, raw_overall):
                        raw["value_squared_error_numerator"] += squared_error * decision_mass
                        raw["value_mass"] += decision_mass
            target["physical_decision_count"] += 1
            raw_overall["physical_decision_count"] += 1
    result = {
        "overall": _finish_metric(raw_overall),
        "by_candidate_seat": {
            str(seat): _finish_metric(raw_by_seat[seat]) for seat in (0, 1)
        },
    }
    if not include_samples:
        result["overall"]["tv_weighted_samples"] = []
        for metric in result["by_candidate_seat"].values():
            metric["tv_weighted_samples"] = []
    else:
        result["overall"]["tv_weighted_samples"] = raw_overall["tv_weighted_samples"]
        for seat in (0, 1):
            result["by_candidate_seat"][str(seat)]["tv_weighted_samples"] = raw_by_seat[
                seat
            ]["tv_weighted_samples"]
    return result


def _combine_metric_records(records: list[dict[str, Any]]) -> dict[str, Any]:
    raw = _empty_metric()
    for record in records:
        sums = record["_sums"]
        for key in (
            "policy_kl_numerator",
            "tv_numerator",
            "top_action_numerator",
            "policy_mass",
            "value_squared_error_numerator",
            "value_mass",
        ):
            raw[key] += float(sums[key])
        raw["policy_row_count"] += int(record["counts"]["policy_rows"])
        raw["physical_decision_count"] += int(record["counts"]["physical_decisions"])
        raw["episode_keys"].update(range(int(record["counts"]["episodes"])))
        raw["tv_weighted_samples"].extend(
            (float(value), float(weight))
            for value, weight in record.get("tv_weighted_samples", [])
        )
    return _finish_metric(raw)


def _validate_source(source: dict[str, Any], expected_hash: str, expected_pairs: int) -> None:
    if source.get("cache_sha256") != expected_hash:
        _fail("fold source cache SHA-256 mismatch")
    if source.get("pair_count") != expected_pairs:
        _fail("fold source pair count mismatch")


def run_fold(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    if args.output.exists():
        _fail(f"refusing to overwrite {args.output}")
    if not args.profile_only and args.output.with_suffix(".state.pt").exists():
        _fail(f"refusing to overwrite {args.output.with_suffix('.state.pt')}")
    decisions, source, load_timings = _load_decisions(args.cache, args.pair_limit)
    fit = [decision for decision in decisions if decision.pair_index % FOLDS != args.fold]
    heldout = [decision for decision in decisions if decision.pair_index % FOLDS == args.fold]
    if not fit or not heldout:
        _fail("fold lacks a fit or heldout split")
    screen._configure(args.seed, args.threads)
    model = _model()
    train_started = time.perf_counter()
    history = _fit_model(model, fit, args)
    train_seconds = time.perf_counter() - train_started
    result: dict[str, Any] = {
        "schema": SCHEMA,
        "fold": args.fold,
        "profile_only": bool(args.profile_only),
        "source": source,
        "split": {
            "rule": "pair_index_mod_4",
            "fit_pair_count": len({d.pair_index for d in fit}),
            "heldout_pair_count": len({d.pair_index for d in heldout}),
            "heldout_pair_indices": sorted({d.pair_index for d in heldout}),
            "fit_physical_decision_count": len(fit),
            "heldout_physical_decision_count": len(heldout),
            "fit_episode_count": len({d.episode_key for d in fit}),
            "heldout_episode_count": len({d.episode_key for d in heldout}),
        },
        "config": {
            "architecture": "complete-public-history-structured-successor-distillation/v1",
            "output_semantics": "absolute-student-outputs-from-model._one",
            "dim": DIM,
            "card_vocab": CARD_VOCAB,
            "group_vocab": GROUP_VOCAB,
            "history_length": HISTORY_LENGTH,
            "history_feature_dim": HISTORY_FEATURE_DIM,
            "epochs": args.epochs,
            "batch_size_physical_decisions": BATCH_SIZE,
            "learning_rate": LR,
            "weight_decay": WEIGHT_DECAY,
            "gradient_norm_cap": GRAD_CAP,
            "seed": args.seed,
            "threads": args.threads,
            "loss": "teacher_to_student_kl_plus_value_mse",
            "weighting": "equal_episode_mass_equal_physical_decision_mass_equal_substep_mass",
        },
        "training_history": history,
        "timings": {
            **load_timings,
            "train_seconds": train_seconds,
            "total_seconds": time.perf_counter() - started,
        },
    }
    if args.profile_only:
        result["profile"] = {
            "pair_limit": args.pair_limit,
            "physical_decisions_per_train_second": len(fit) / max(train_seconds, 1e-12),
            "physical_decisions_per_total_second": len(decisions)
            / max(result["timings"]["total_seconds"], 1e-12),
        }
    else:
        result["heldout_metrics"] = _metrics(model, heldout)
        state_path = args.output.with_suffix(".state.pt")
        torch.save(model.state_dict(), state_path)
        result["model_state"] = {
            "path": str(state_path),
            "sha256": scaled.outcome._sha256(state_path),
        }
    result["non_claims"] = [
        "development screen only",
        "distillation parity is not a live win rate",
        "no promotion or pro-level claim",
    ]
    _write_new(args.output, result)
    return result


def aggregate(args: argparse.Namespace) -> dict[str, Any]:
    if args.output.exists():
        _fail(f"refusing to overwrite {args.output}")
    if len(args.fold_result) != FOLDS:
        _fail("aggregate requires exactly four fold results")
    results = [json.loads(path.read_text(encoding="utf-8")) for path in args.fold_result]
    if (
        {result.get("fold") for result in results} != set(range(FOLDS))
        or any(result.get("schema") != SCHEMA for result in results)
        or any(result.get("profile_only") for result in results)
    ):
        _fail("aggregate requires four non-profile fold results")
    for result in results:
        _validate_source(result.get("source", {}), args.expected_cache_sha256, args.expected_pairs)
    heldout_sets: list[set[int]] = []
    for result in results:
        split = result.get("split", {})
        fold = int(result["fold"])
        heldout = split.get("heldout_pair_indices")
        if (
            split.get("rule") != "pair_index_mod_4"
            or not isinstance(heldout, list)
            or heldout != sorted(set(heldout))
            or any(not isinstance(pair, int) or pair % FOLDS != fold for pair in heldout)
            or split.get("heldout_pair_count") != len(heldout)
            or split.get("fit_pair_count") != EXPECTED_PAIRS - len(heldout)
        ):
            _fail("fold split provenance mismatch")
        heldout_sets.append(set(heldout))
    if (
        set.union(*heldout_sets) != set(range(EXPECTED_PAIRS))
        or sum(len(values) for values in heldout_sets) != EXPECTED_PAIRS
    ):
        _fail("heldout fold panels are not an exact disjoint partition")
    configs = {json.dumps(result["config"], sort_keys=True) for result in results}
    if len(configs) != 1:
        _fail("fold configuration mismatch")
    overall = _combine_metric_records(
        [result["heldout_metrics"]["overall"] for result in results]
    )
    by_seat = {
        str(seat): _combine_metric_records(
            [result["heldout_metrics"]["by_candidate_seat"][str(seat)] for result in results]
        )
        for seat in (0, 1)
    }
    tv_samples = [
        (float(value), float(weight))
        for result in results
        for value, weight in result["heldout_metrics"]["overall"]["tv_weighted_samples"]
    ]
    overall["p90_total_variation"] = _weighted_quantile(tv_samples, 0.90)
    fold_mean_gates = {
        str(result["fold"]): result["heldout_metrics"]["overall"]["mean_total_variation"]
        <= 0.025
        for result in results
    }
    gates = {
        "mean_tv_le_0_02": overall["mean_total_variation"] <= 0.02,
        "p90_tv_le_0_05": overall["p90_total_variation"] <= 0.05,
        "seat_0_mean_tv_le_0_025": by_seat["0"]["mean_total_variation"] <= 0.025,
        "seat_1_mean_tv_le_0_025": by_seat["1"]["mean_total_variation"] <= 0.025,
        "overall_top_action_agreement_ge_0_98": overall["top_action_agreement"] >= 0.98,
        "seat_0_top_action_agreement_ge_0_97": by_seat["0"]["top_action_agreement"] >= 0.97,
        "seat_1_top_action_agreement_ge_0_97": by_seat["1"]["top_action_agreement"] >= 0.97,
        "overall_value_rmse_le_0_10": overall["value_rmse"] <= 0.10,
        "seat_0_value_rmse_le_0_12": by_seat["0"]["value_rmse"] <= 0.12,
        "seat_1_value_rmse_le_0_12": by_seat["1"]["value_rmse"] <= 0.12,
        "every_fold_mean_tv_le_0_025": all(fold_mean_gates.values()),
    }
    result = {
        "schema": AGGREGATE_SCHEMA,
        "fold_results": [{"path": str(path)} for path in args.fold_result],
        "source": {
            "cache_sha256": args.expected_cache_sha256,
            "pair_count": args.expected_pairs,
        },
        "config": results[0]["config"],
        "heldout_metrics": {
            "overall": overall,
            "by_candidate_seat": by_seat,
            "tv_weighted_samples": tv_samples,
        },
        "fold_mean_tv_gates": fold_mean_gates,
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


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    fold = subparsers.add_parser("fold")
    fold.add_argument("--cache", type=Path, required=True)
    fold.add_argument("--output", type=Path, required=True)
    fold.add_argument("--fold", type=int, choices=range(FOLDS), required=True)
    fold.add_argument("--threads", type=int, default=6)
    fold.add_argument("--epochs", type=int, default=EPOCHS)
    fold.add_argument("--seed", type=int, default=SEED)
    fold.add_argument("--pair-limit", type=int)
    fold.add_argument("--profile-only", action="store_true")
    fold.add_argument("--expected-cache-sha256", default=EXPECTED_CACHE_SHA256)
    fold.add_argument("--expected-pairs", type=int, default=EXPECTED_PAIRS)
    aggregate_parser = subparsers.add_parser("aggregate")
    aggregate_parser.add_argument("--fold-result", action="append", type=Path, required=True)
    aggregate_parser.add_argument("--output", type=Path, required=True)
    aggregate_parser.add_argument("--expected-cache-sha256", default=EXPECTED_CACHE_SHA256)
    aggregate_parser.add_argument("--expected-pairs", type=int, default=EXPECTED_PAIRS)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if (
        args.expected_cache_sha256 != EXPECTED_CACHE_SHA256
        or args.expected_pairs != EXPECTED_PAIRS
    ):
        _fail("source cache identity and pair count are frozen")
    if args.command == "fold":
        if (
            args.threads < 1
            or args.threads > 24
            or args.epochs < 1
            or args.seed != SEED
        ):
            _fail("invalid fold training configuration")
        if args.profile_only and args.pair_limit is None:
            _fail("profile-only fold requires --pair-limit")
        if not args.profile_only and args.threads != 6:
            _fail("formal folds require the frozen six-thread topology")
        if not args.profile_only and (args.pair_limit is not None or args.epochs != EPOCHS):
            _fail("formal folds require all pairs and five epochs")
        result = run_fold(args)
    else:
        result = aggregate(args)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
