#!/usr/bin/env python3
"""Evaluate the recurrent value ensemble on the disjoint retained corpus."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
import time
from pathlib import Path
from typing import Any

import torch

import run_screen_v1 as screen
from model_v1 import RecurrentStructuredActorCritic, pack_rows


SCHEMA = "mtg-kernel-recurrent-value-external-check/v1"
EXPECTED_CACHE_SHA256 = (
    "44eae5bee2b5556faa6293c80a88cb8f67f90d46066ffb5115ced2daac579800"
)
EXPECTED_STATE_SHA256 = (
    "894d7dfe39c3d0798af3693b6fc2357bf36a237b52e9055dc895caf20ad7788f",
    "9a292849140b319437b163f6316fa7972b4fd3748d387ddafc90993e316414d7",
    "c2a1d3cd5ace37215955485b21b3335297860e6be4b8bd8fa19d260f043576cb",
    "5f11e47572574ecbb47779d65f7e152c1bb585255a1e1c2344364441a001464c",
)
WIDTH48_MSE = {
    "overall": 0.44284559136632723,
    "0": 0.45817498864228884,
    "1": 0.4275161940903657,
}
BATCH_SIZE = 256


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


def _load_models(
    state_paths: list[Path], device: torch.device
) -> list[RecurrentStructuredActorCritic]:
    if len(state_paths) != 4:
        raise ValueError("exactly four recurrent fold states are required")
    models: list[RecurrentStructuredActorCritic] = []
    for index, (path, expected) in enumerate(
        zip(state_paths, EXPECTED_STATE_SHA256)
    ):
        observed = _sha256(path)
        if observed != expected:
            raise ValueError(f"fold {index} state SHA-256 mismatch")
        model = RecurrentStructuredActorCritic(screen.DIM).to(device)
        model.load_state_dict(
            torch.load(path, map_location="cpu", weights_only=True), strict=True
        )
        model.eval()
        models.append(model)
    return models


def _summarize(records: list[dict[str, float | int]]) -> dict[str, Any]:
    weight = sum(float(record["weight"]) for record in records)
    parent_mse = sum(
        float(record["parent_error"]) * float(record["weight"])
        for record in records
    ) / max(weight, 1.0e-12)
    ensemble_mse = sum(
        float(record["ensemble_error"]) * float(record["weight"])
        for record in records
    ) / max(weight, 1.0e-12)
    member_mse = [
        sum(
            float(record[f"member_{index}_error"]) * float(record["weight"])
            for record in records
        )
        / max(weight, 1.0e-12)
        for index in range(4)
    ]
    predictions = [float(record["prediction"]) for record in records]
    return {
        "episode_mass": weight,
        "physical_decisions": len(records),
        "parent_mse": parent_mse,
        "ensemble_mse": ensemble_mse,
        "member_mse": member_mse,
        "relative_improvement_over_parent": (parent_mse - ensemble_mse)
        / max(parent_mse, 1.0e-12),
        "minimum_prediction": min(predictions),
        "maximum_prediction": max(predictions),
        "all_predictions_finite_and_bounded": all(
            math.isfinite(value) and -1.0 <= value <= 1.0 for value in predictions
        ),
    }


def _evaluate(
    models: list[RecurrentStructuredActorCritic],
    decisions: list[screen.PhysicalDecision],
    device: torch.device,
) -> dict[str, Any]:
    records: list[dict[str, float | int]] = []
    with torch.no_grad():
        for start in range(0, len(decisions), BATCH_SIZE):
            selected = decisions[start : start + BATCH_SIZE]
            rows = [decision.rows[0] for decision in selected]
            packed = pack_rows(rows, device)
            predictions = [model(packed)[1] for model in models]
            ensemble = torch.stack(predictions).mean(dim=0)
            for index, decision in enumerate(selected):
                target = float(decision.rows[0]["terminal_reward"])
                parent = float(decision.rows[0]["old_value"])
                prediction = float(ensemble[index])
                record: dict[str, float | int] = {
                    "seat": decision.candidate_seat,
                    "weight": decision.episode_weight,
                    "prediction": prediction,
                    "parent_error": (parent - target) ** 2,
                    "ensemble_error": (prediction - target) ** 2,
                }
                for member, values in enumerate(predictions):
                    member_prediction = float(values[index])
                    record[f"member_{member}_error"] = (
                        member_prediction - target
                    ) ** 2
                records.append(record)
    return {
        "overall": _summarize(records),
        "by_candidate_seat": {
            str(seat): _summarize(
                [record for record in records if record["seat"] == seat]
            )
            for seat in (0, 1)
        },
    }


def run(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    device = screen._configure(args.gpu)
    screen.EXPECTED_CACHE_SHA256 = EXPECTED_CACHE_SHA256
    screen.EXPECTED_CACHE_SCHEMA = None
    screen.CORPUS_PAIR_COUNT = 1_024
    decisions, source = screen._load_decisions(args.cache)
    models = _load_models(args.states, device)
    primary = _evaluate(models, decisions, device)
    repeat = _evaluate(models, decisions, device)
    primary_sha256 = hashlib.sha256(_json_bytes(primary)).hexdigest()
    repeat_sha256 = hashlib.sha256(_json_bytes(repeat)).hexdigest()
    gates = {
        "overall_improves_at_least_5pct_over_width48": primary["overall"][
            "ensemble_mse"
        ]
        <= 0.95 * WIDTH48_MSE["overall"],
        "p0_does_not_regress_vs_width48": primary["by_candidate_seat"]["0"][
            "ensemble_mse"
        ]
        <= WIDTH48_MSE["0"],
        "p1_does_not_regress_vs_width48": primary["by_candidate_seat"]["1"][
            "ensemble_mse"
        ]
        <= WIDTH48_MSE["1"],
        "all_predictions_finite_and_bounded": all(
            primary["by_candidate_seat"][str(seat)][
                "all_predictions_finite_and_bounded"
            ]
            for seat in (0, 1)
        ),
        "exact_repeat": primary_sha256 == repeat_sha256,
    }
    result = {
        "schema": SCHEMA,
        "source": source,
        "states": [
            {"path": str(path), "sha256": expected}
            for path, expected in zip(args.states, EXPECTED_STATE_SHA256)
        ],
        "benchmark": {
            "confirmation_sha256": "716189e49c635eebdf5647e17ef4e3b3ab684c68addbc6b3c94fc3bed46f7539",
            "width48_mse": WIDTH48_MSE,
        },
        "metrics": primary,
        "primary_metric_sha256": primary_sha256,
        "repeat_metric_sha256": repeat_sha256,
        "gates": gates,
        "pass": all(gates.values()),
        "runtime_seconds": time.perf_counter() - started,
        "nonclaims": [
            "reused disjoint value corpus only",
            "no policy, search, or strength result",
            "no promotion or professional-level claim",
        ],
    }
    if args.output.exists():
        raise ValueError(f"refusing to overwrite {args.output}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(_json_bytes(result))
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--states", type=Path, nargs=4, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--gpu", type=int, default=1)
    args = parser.parse_args()
    print(json.dumps(run(args), sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
