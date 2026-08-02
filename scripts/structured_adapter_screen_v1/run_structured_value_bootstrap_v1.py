#!/usr/bin/env python3
"""Run the fixed four-fold structured value-bootstrap development screen."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import random
import time
from pathlib import Path
from typing import Any

import torch

import run_screen as screen
import run_structured_outcome_policy_v1 as outcome


SCHEMA = "mtg-kernel-structured-value-bootstrap-screen/v1"
AGGREGATE_SCHEMA = SCHEMA + ".aggregate"
DATA_SHA256 = "317148bc19c6b33214181ed807d672b1a6f135cb6cbee1b5f9139667382fa9b0"


def _fail(message: str) -> None:
    raise ValueError(message)


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


def _fit_value(
    model: screen.StructuredAdapter,
    fit: list[outcome.PhysicalDecision],
    args: argparse.Namespace,
) -> list[dict[str, float | int]]:
    trainable = [
        parameter
        for name, parameter in model.named_parameters()
        if not name.startswith("policy_head.")
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
        epoch_loss = 0.0
        steps = 0
        model.train()
        for start in range(0, len(order), args.batch_size):
            batch = [fit[index] for index in order[start : start + args.batch_size]]
            terms: list[torch.Tensor] = []
            weights: list[float] = []
            for group in batch:
                row = group.rows[0]
                _, prediction = model(row)
                terms.append((prediction - float(row["terminal_reward"])) ** 2)
                weights.append(normalized_weights[group.key])
            loss = (
                torch.stack(terms) * torch.tensor(weights, dtype=torch.float32)
            ).mean()
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient_norm = torch.nn.utils.clip_grad_norm_(trainable, args.grad_cap)
            if not torch.isfinite(gradient_norm):
                _fail("non-finite value gradient")
            optimizer.step()
            epoch_loss += float(loss.detach())
            steps += 1
        history.append(
            {
                "epoch": epoch + 1,
                "mean_minibatch_loss": epoch_loss / max(steps, 1),
                "optimizer_steps": steps,
            }
        )
    return history


def _value_metrics(
    model: screen.StructuredAdapter,
    groups: list[outcome.PhysicalDecision],
) -> dict[str, Any]:
    records: list[tuple[int, float, float, float, float, float]] = []
    model.eval()
    with torch.no_grad():
        for group in groups:
            row = group.rows[0]
            parent = float(row["old_value"])
            target = float(row["terminal_reward"])
            _, candidate_tensor = model(row)
            candidate = float(candidate_tensor)
            records.append(
                (
                    group.candidate_seat,
                    (parent - target) ** 2,
                    (candidate - target) ** 2,
                    group.episode_weight,
                    abs(candidate - parent),
                    abs(candidate),
                )
            )

    def summarize(subset: list[tuple[int, float, float, float, float, float]]) -> dict[str, Any]:
        weight = sum(record[3] for record in subset)
        parent_numerator = sum(record[1] * record[3] for record in subset)
        candidate_numerator = sum(record[2] * record[3] for record in subset)
        parent_mse = parent_numerator / max(weight, 1e-12)
        candidate_mse = candidate_numerator / max(weight, 1e-12)
        return {
            "parent_mse": parent_mse,
            "candidate_mse": candidate_mse,
            "relative_improvement": (parent_mse - candidate_mse)
            / max(parent_mse, 1e-12),
            "parent_numerator": parent_numerator,
            "candidate_numerator": candidate_numerator,
            "episode_mass": weight,
            "physical_decision_count": len(subset),
            "max_absolute_prediction": max((record[5] for record in subset), default=0.0),
        }

    movement_samples = [(record[4], record[3]) for record in records]
    movement_weight = sum(weight for _, weight in movement_samples)
    return {
        "overall": summarize(records),
        "by_candidate_seat": {
            str(seat): summarize([record for record in records if record[0] == seat])
            for seat in (0, 1)
        },
        "movement": {
            "mean_absolute_residual": sum(
                value * weight for value, weight in movement_samples
            )
            / max(movement_weight, 1e-12),
            "p90_absolute_residual": outcome._weighted_quantile(
                movement_samples, 0.90
            ),
            "weighted_samples": movement_samples,
            "episode_mass": movement_weight,
        },
    }


def _diagnostics(
    model: screen.StructuredAdapter,
    groups: list[outcome.PhysicalDecision],
    seed: int,
    sample_size: int,
) -> dict[str, Any]:
    rows = [group.rows[0] for group in groups]
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
            _, candidate = model(row)
            _, permuted = model(screen._permuted(row, generator))
            permutation_max = max(
                permutation_max, abs(float(candidate - permuted))
            )
        for row in reference_rows:
            _, candidate = model(row)
            _, no_refs = model(row, remove_refs=True)
            delta = abs(float(candidate - no_refs))
            reference_max = max(reference_max, delta)
            reference_affected += int(delta > 1e-4)
    return {
        "permutation_max_value_delta": permutation_max,
        "permutation_sample_count": len(permutation_rows),
        "reference_eligible_population": len(eligible),
        "reference_sample_count": len(reference_rows),
        "reference_affected_count": reference_affected,
        "reference_affected_rate": reference_affected
        / max(len(reference_rows), 1),
        "reference_max_value_delta": reference_max,
    }


def run_fold(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    if _sha256(args.outcome_jsonl) != DATA_SHA256:
        _fail("outcome corpus hash mismatch")
    examples, terminals = screen._load_outcome(args.outcome_jsonl)
    groups = outcome._physical_decisions(examples)
    fit = [group for group in groups if group.pair_index % 4 != args.fold]
    heldout = [group for group in groups if group.pair_index % 4 == args.fold]
    fit_episodes = {group.episode_key for group in fit}
    heldout_episodes = {group.episode_key for group in heldout}
    if (
        len(terminals) != 512
        or len(fit_episodes) != 384
        or len(heldout_episodes) != 128
        or fit_episodes.intersection(heldout_episodes)
    ):
        _fail("value fold does not have the fixed whole-episode split")
    card_vocab, group_vocab = screen._model_vocab(examples)
    screen._configure(args.seed, args.threads)
    model = screen.StructuredAdapter(card_vocab, group_vocab, args.dim)
    history = _fit_value(model, fit, args)
    metrics = _value_metrics(model, heldout)
    diagnostics = _diagnostics(
        model, heldout, args.seed + args.fold + 101, args.diagnostic_sample_size
    )
    state_path = args.output.with_suffix(".state.pt")
    if state_path.exists():
        _fail(f"refusing to overwrite {state_path}")
    state_path.parent.mkdir(parents=True, exist_ok=True)
    torch.save(model.state_dict(), state_path)
    result = {
        "schema": SCHEMA,
        "fold": args.fold,
        "source": {
            "outcome_jsonl": str(args.outcome_jsonl),
            "outcome_jsonl_sha256": DATA_SHA256,
            "terminal_count": len(terminals),
            "physical_decision_count": len(groups),
        },
        "split": {
            "rule": "pair_index_mod_4",
            "fit_episode_count": len(fit_episodes),
            "heldout_episode_count": len(heldout_episodes),
            "fit_physical_decision_count": len(fit),
            "heldout_physical_decision_count": len(heldout),
        },
        "config": {
            "architecture": "stateless-structured-object-action-attention-value-residual/v1",
            "dim": args.dim,
            "card_vocab": card_vocab,
            "group_vocab": group_vocab,
            "epochs": args.epochs,
            "batch_size_physical_decisions": args.batch_size,
            "learning_rate": args.lr,
            "weight_decay": args.weight_decay,
            "gradient_norm_cap": args.grad_cap,
            "seed": args.seed,
            "threads": args.threads,
            "policy_model": "exact-retained-parent-unchanged",
        },
        "training_history": history,
        "heldout": metrics,
        "diagnostics": diagnostics,
        "model_state": {"path": str(state_path), "sha256": _sha256(state_path)},
        "runtime_seconds": time.perf_counter() - started,
        "non_claims": [
            "reused development corpus",
            "no live policy or strength evidence",
            "no promotion or pro-level claim",
        ],
    }
    _write_new(args.output, result, compact=True)
    return result


def _aggregate_subset(
    results: list[dict[str, Any]], seat: str | None
) -> dict[str, float]:
    records = [
        result["heldout"]["overall"]
        if seat is None
        else result["heldout"]["by_candidate_seat"][seat]
        for result in results
    ]
    weight = sum(float(record["episode_mass"]) for record in records)
    parent_numerator = sum(float(record["parent_numerator"]) for record in records)
    candidate_numerator = sum(
        float(record["candidate_numerator"]) for record in records
    )
    parent_mse = parent_numerator / max(weight, 1e-12)
    candidate_mse = candidate_numerator / max(weight, 1e-12)
    return {
        "parent_mse": parent_mse,
        "candidate_mse": candidate_mse,
        "relative_improvement": (parent_mse - candidate_mse)
        / max(parent_mse, 1e-12),
        "episode_mass": weight,
        "max_absolute_prediction": max(
            float(record["max_absolute_prediction"]) for record in records
        ),
    }


def aggregate(args: argparse.Namespace) -> dict[str, Any]:
    results = [json.loads(path.read_text(encoding="utf-8")) for path in args.fold_result]
    if (
        len(results) != 4
        or {result.get("fold") for result in results} != {0, 1, 2, 3}
        or any(result.get("schema") != SCHEMA for result in results)
    ):
        _fail("aggregate requires exactly value folds 0, 1, 2, and 3")
    if len({json.dumps(result["config"], sort_keys=True) for result in results}) != 1:
        _fail("value fold configuration mismatch")
    overall = _aggregate_subset(results, None)
    by_seat = {seat: _aggregate_subset(results, seat) for seat in ("0", "1")}
    samples = [
        (float(value), float(weight))
        for result in results
        for value, weight in result["heldout"]["movement"]["weighted_samples"]
    ]
    movement_weight = sum(weight for _, weight in samples)
    movement = {
        "mean_absolute_residual": sum(value * weight for value, weight in samples)
        / max(movement_weight, 1e-12),
        "p90_absolute_residual": outcome._weighted_quantile(samples, 0.90),
        "episode_mass": movement_weight,
    }
    positive_folds = sum(
        float(result["heldout"]["overall"]["relative_improvement"]) > 0.0
        for result in results
    )
    diagnostics = {
        "permutation_max_value_delta": max(
            float(result["diagnostics"]["permutation_max_value_delta"])
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
        "aggregate_mse_improvement_ge_5pct": overall["relative_improvement"]
        >= 0.05,
        "no_candidate_seat_regression_over_2pct": all(
            by_seat[seat]["relative_improvement"] >= -0.02
            for seat in ("0", "1")
        ),
        "at_least_three_of_four_folds_positive": positive_folds >= 3,
        "mean_absolute_residual_le_0_25": movement["mean_absolute_residual"]
        <= 0.25,
        "p90_absolute_residual_le_0_50": movement["p90_absolute_residual"]
        <= 0.50,
        "max_absolute_prediction_le_1_50": overall["max_absolute_prediction"]
        <= 1.50,
        "permutation_max_delta_le_1e_5": diagnostics[
            "permutation_max_value_delta"
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
        "config": results[0]["config"],
        "heldout": {"overall": overall, "by_candidate_seat": by_seat},
        "fold_improvements": {
            str(result["fold"]): result["heldout"]["overall"][
                "relative_improvement"
            ]
            for result in results
        },
        "positive_fold_count": positive_folds,
        "movement": movement,
        "diagnostics": diagnostics,
        "gates": gates,
        "pass": all(gates.values()),
        "non_claims": [
            "development value screen only",
            "a pass authorizes a bootstrap-search screen, not live play",
            "no promotion or pro-level claim",
        ],
    }
    _write_new(args.output, result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    fold = subparsers.add_parser("fold")
    fold.add_argument("--outcome-jsonl", type=Path, required=True)
    fold.add_argument("--fold", type=int, choices=range(4), required=True)
    fold.add_argument("--output", type=Path, required=True)
    fold.add_argument("--dim", type=int, default=48)
    fold.add_argument("--epochs", type=int, default=20)
    fold.add_argument("--batch-size", type=int, default=32)
    fold.add_argument("--lr", type=float, default=3e-4)
    fold.add_argument("--weight-decay", type=float, default=1e-4)
    fold.add_argument("--grad-cap", type=float, default=5.0)
    fold.add_argument("--seed", type=int, default=20260802)
    fold.add_argument("--threads", type=int, default=6)
    fold.add_argument("--diagnostic-sample-size", type=int, default=256)
    aggregate_parser = subparsers.add_parser("aggregate")
    aggregate_parser.add_argument(
        "--fold-result", action="append", type=Path, required=True
    )
    aggregate_parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "fold":
        if (
            args.dim != 48
            or args.epochs != 20
            or args.batch_size != 32
            or args.lr != 3e-4
            or args.weight_decay != 1e-4
            or args.grad_cap != 5.0
            or args.seed != 20260802
            or args.threads < 1
            or args.diagnostic_sample_size != 256
        ):
            _fail("fold configuration differs from the fixed value screen")
        result = run_fold(args)
    else:
        result = aggregate(args)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
