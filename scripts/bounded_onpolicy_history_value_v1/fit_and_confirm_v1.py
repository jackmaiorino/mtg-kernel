#!/usr/bin/env python3
"""Fit the bounded history value model and evaluate one fresh panel."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import random
import sys
import time
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
SCRIPTS_DIR = SCRIPT_DIR.parent
STRUCTURED_DIR = SCRIPTS_DIR / "structured_adapter_screen_v1"
TERMINAL_DIR = SCRIPTS_DIR / "policy_only_structured_terminal_rung_v1"
for directory in (STRUCTURED_DIR, TERMINAL_DIR):
    sys.path.insert(0, str(directory))

import run_pipeline_v1 as terminal  # noqa: E402
import run_screen as screen  # noqa: E402
import run_structured_outcome_policy_v1 as outcome  # noqa: E402
import run_structured_successor_distillation_v1 as distill  # noqa: E402


FIT_SCHEMA = "mtg-kernel-bounded-onpolicy-history-value-fit/v1"
CONFIRM_SCHEMA = "mtg-kernel-bounded-onpolicy-history-value-confirmation/v1"
DEVELOPMENT_CACHE_SHA256 = "454e4ce1b8f7413839a36c8e2731fc0cb65581ce13e593634bffa70013a6f16d"
INITIALIZER_STATE_SHA256 = "ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0"
FRESH_SCORER_SHA256 = "8af1ffabe836cfe53d9b62edb98943e68183825e332cd47070ea20e93ae5c990"
FRESH_BASE_SEED = 1_690_001
FRESH_PAIR_COUNT = 1_024
EPOCHS = 5
BATCH_SIZE = 32
LR = 3.0e-4
WEIGHT_DECAY = 1.0e-4
GRAD_CAP = 5.0
SEED = 20_260_810
THREADS = 24
DIAGNOSTIC_SAMPLE_SIZE = 1_024
PARENT_BOUND_EPSILON = 1.0e-3


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


def _load_initializer(path: Path) -> tuple[Any, dict[str, Any]]:
    observed_sha256 = _sha256(path)
    if observed_sha256 != INITIALIZER_STATE_SHA256:
        _fail("bounded value initializer SHA-256 mismatch")
    payload = torch.load(path, map_location="cpu", weights_only=False)
    state = payload.get("model_state_dict")
    if not isinstance(state, dict):
        _fail("bounded value initializer lacks model_state_dict")
    model = distill._model()
    model.load_state_dict(state, strict=True)
    if any(
        torch.count_nonzero(tensor).item()
        for name, tensor in model.state_dict().items()
        if name.startswith("value_head.")
    ):
        _fail("bounded value initializer residual is not exactly zero")
    return model, {"path": str(path), "sha256": observed_sha256}


def _bounded_prediction(
    model: Any, row: dict[str, Any], remove_refs: bool = False
) -> torch.Tensor:
    parent = row["old_value"]
    if not bool(torch.isfinite(parent)):
        _fail("retained parent value is non-finite")
    bounded_parent = parent.clamp(
        -1.0 + PARENT_BOUND_EPSILON, 1.0 - PARENT_BOUND_EPSILON
    )
    _, raw_residual = model._one(row, remove_refs=remove_refs)  # noqa: SLF001
    shift = torch.tanh(raw_residual)
    denominator = 1.0 + bounded_parent * shift
    prediction = (bounded_parent + shift) / denominator
    if not bool(torch.isfinite(prediction)):
        _fail("bounded value prediction is non-finite")
    return prediction


def _initial_alignment(model: Any, decisions: list[Any]) -> dict[str, Any]:
    sampled = random.Random(SEED).sample(decisions, min(256, len(decisions)))
    maximum = 0.0
    with torch.no_grad():
        for decision in sampled:
            row = decision.rows[0]
            maximum = max(
                maximum,
                abs(
                    float(
                        _bounded_prediction(model, row)
                        - row["old_value"].clamp(
                            -1.0 + PARENT_BOUND_EPSILON,
                            1.0 - PARENT_BOUND_EPSILON,
                        )
                    )
                ),
            )
    return {
        "sampled_physical_decisions": len(sampled),
        "maximum_absolute_projected_parent_reproduction_error": maximum,
        "pass": maximum <= 1.0e-6,
    }


def _fit(model: Any, decisions: list[Any]) -> list[dict[str, Any]]:
    trainable = [
        parameter
        for name, parameter in model.named_parameters()
        if not name.startswith("policy_head.")
    ]
    optimizer = torch.optim.AdamW(trainable, lr=LR, weight_decay=WEIGHT_DECAY)
    episode_mass = sum(decision.episode_weight for decision in decisions)
    weights = {
        decision.key: decision.episode_weight * len(decisions) / episode_mass
        for decision in decisions
    }
    rng = random.Random(SEED)
    history = []
    for epoch in range(EPOCHS):
        order = list(range(len(decisions)))
        rng.shuffle(order)
        loss_total = 0.0
        gradient_norm_max = 0.0
        steps = 0
        model.train()
        for start in range(0, len(order), BATCH_SIZE):
            batch = [decisions[index] for index in order[start : start + BATCH_SIZE]]
            terms = []
            masses = []
            for decision in batch:
                row = decision.rows[0]
                prediction = _bounded_prediction(model, row)
                terms.append((prediction - float(row["terminal_reward"])) ** 2)
                masses.append(weights[decision.key])
            mass_tensor = torch.tensor(masses, dtype=torch.float32)
            loss = (torch.stack(terms) * mass_tensor).sum() / mass_tensor.sum()
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient_norm = torch.nn.utils.clip_grad_norm_(trainable, GRAD_CAP)
            if not torch.isfinite(gradient_norm):
                _fail("non-finite bounded-value gradient")
            optimizer.step()
            loss_total += float(loss.detach())
            gradient_norm_max = max(gradient_norm_max, float(gradient_norm))
            steps += 1
        history.append(
            {
                "epoch": epoch + 1,
                "mean_minibatch_loss": loss_total / steps,
                "maximum_preclip_gradient_norm": gradient_norm_max,
                "optimizer_steps": steps,
            }
        )
    return history


def _metrics(model: Any, decisions: list[Any]) -> dict[str, Any]:
    records = []
    model.eval()
    with torch.no_grad():
        for decision in decisions:
            row = decision.rows[0]
            raw_parent = float(row["old_value"])
            parent = max(
                -1.0 + PARENT_BOUND_EPSILON,
                min(1.0 - PARENT_BOUND_EPSILON, raw_parent),
            )
            target = float(row["terminal_reward"])
            candidate = float(_bounded_prediction(model, row))
            records.append(
                (
                    decision.candidate_seat,
                    (raw_parent - target) ** 2,
                    (parent - target) ** 2,
                    (candidate - target) ** 2,
                    decision.episode_weight,
                    candidate,
                    parent,
                )
            )

    def summarize(
        subset: list[tuple[int, float, float, float, float, float, float]]
    ) -> dict[str, Any]:
        mass = sum(record[4] for record in subset)
        raw_parent_numerator = sum(record[1] * record[4] for record in subset)
        parent_numerator = sum(record[2] * record[4] for record in subset)
        candidate_numerator = sum(record[3] * record[4] for record in subset)
        raw_parent_mse = raw_parent_numerator / mass
        parent_mse = parent_numerator / mass
        candidate_mse = candidate_numerator / mass
        return {
            "raw_parent_mse": raw_parent_mse,
            "parent_mse": parent_mse,
            "candidate_mse": candidate_mse,
            "relative_improvement": (parent_mse - candidate_mse) / parent_mse,
            "relative_improvement_over_raw_parent": (
                raw_parent_mse - candidate_mse
            )
            / raw_parent_mse,
            "episode_mass": mass,
            "physical_decision_count": len(subset),
            "minimum_parent_prediction": min(record[6] for record in subset),
            "maximum_parent_prediction": max(record[6] for record in subset),
            "minimum_prediction": min(record[5] for record in subset),
            "maximum_prediction": max(record[5] for record in subset),
            "all_predictions_finite_and_bounded": all(
                torch.isfinite(torch.tensor(record[5])).item() and -1.0 <= record[5] <= 1.0
                for record in subset
            ),
        }

    return {
        "overall": summarize(records),
        "by_candidate_seat": {
            str(seat): summarize([record for record in records if record[0] == seat])
            for seat in (0, 1)
        },
    }


def _diagnostics(model: Any, decisions: list[Any]) -> dict[str, Any]:
    rows = [decision.rows[0] for decision in decisions]
    rng = random.Random(SEED + 101)
    permutation_rows = (
        rng.sample(rows, DIAGNOSTIC_SAMPLE_SIZE)
        if len(rows) >= DIAGNOSTIC_SAMPLE_SIZE
        else []
    )
    reference_eligible = [row for row in rows if int(row["action_ref_features"].shape[0]) > 0]
    reference_rows = (
        rng.sample(reference_eligible, DIAGNOSTIC_SAMPLE_SIZE)
        if len(reference_eligible) >= DIAGNOSTIC_SAMPLE_SIZE
        else []
    )
    generator = torch.Generator(device="cpu").manual_seed(SEED + 101)
    permutation_max = 0.0
    reference_affected = 0
    reference_max = 0.0
    model.eval()
    with torch.no_grad():
        for row in permutation_rows:
            candidate = _bounded_prediction(model, row)
            permuted = _bounded_prediction(model, screen._permuted(row, generator))  # noqa: SLF001
            permutation_max = max(permutation_max, abs(float(candidate - permuted)))
        for row in reference_rows:
            candidate = _bounded_prediction(model, row)
            no_refs = _bounded_prediction(model, row, remove_refs=True)
            delta = abs(float(candidate - no_refs))
            reference_max = max(reference_max, delta)
            reference_affected += int(delta > 1.0e-4)
    return {
        "permutation_sample_count": len(permutation_rows),
        "permutation_max_value_delta": permutation_max,
        "reference_eligible_population": len(reference_eligible),
        "reference_sample_count": len(reference_rows),
        "reference_affected_count": reference_affected,
        "reference_affected_rate": (
            reference_affected / len(reference_rows) if reference_rows else 0.0
        ),
        "reference_max_value_delta": reference_max,
    }


def fit(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    if _sha256(args.development_cache) != DEVELOPMENT_CACHE_SHA256:
        _fail("bounded value development cache SHA-256 mismatch")
    decisions, source, timings = terminal._load_decisions(  # noqa: SLF001
        args.development_cache, None
    )
    if source["pair_count"] != 2_048 or source["episode_count"] != 4_096:
        _fail("bounded value development panel is incomplete")
    screen._configure(SEED, THREADS)  # noqa: SLF001
    model, initializer = _load_initializer(args.initializer_state)
    alignment = _initial_alignment(model, decisions)
    if not alignment["pass"]:
        _fail("bounded parameterization is not parent-preserving")
    trained_started = time.perf_counter()
    training_history = _fit(model, decisions)
    trained = time.perf_counter()
    if args.model_state.exists():
        _fail(f"refusing to overwrite {args.model_state}")
    args.model_state.parent.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "schema": FIT_SCHEMA + ".state",
            "model_state_dict": model.state_dict(),
            "initializer_state_sha256": INITIALIZER_STATE_SHA256,
            "development_cache_sha256": DEVELOPMENT_CACHE_SHA256,
        },
        args.model_state,
    )
    report = {
        "schema": FIT_SCHEMA,
        "status": "complete",
        "source": source,
        "initializer": initializer,
        "parameterization": "tanh-addition-projected-parent-bounded-value/v1",
        "config": {
            "epochs": EPOCHS,
            "batch_size_physical_decisions": BATCH_SIZE,
            "learning_rate": LR,
            "weight_decay": WEIGHT_DECAY,
            "gradient_norm_cap": GRAD_CAP,
            "seed": SEED,
            "threads": THREADS,
            "parent_projection_epsilon": PARENT_BOUND_EPSILON,
            "target": "actor-relative-natural-terminal-win-loss-draw-only/v1",
        },
        "initial_alignment": alignment,
        "training_history": training_history,
        "model_state": {"path": str(args.model_state), "sha256": _sha256(args.model_state)},
        "timings": {
            **timings,
            "train_seconds": trained - trained_started,
            "total_seconds": time.perf_counter() - started,
        },
        "nonclaims": [
            "development fit only",
            "fresh confirmation not yet touched",
            "no policy or strength result",
        ],
    }
    _write_new(args.output, report)
    return report


def confirm(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    fit_report = json.loads(args.fit_report.read_text(encoding="utf-8"))
    if (
        fit_report.get("schema") != FIT_SCHEMA
        or fit_report.get("status") != "complete"
        or fit_report.get("source", {}).get("cache_sha256")
        != DEVELOPMENT_CACHE_SHA256
        or fit_report.get("initializer", {}).get("sha256")
        != INITIALIZER_STATE_SHA256
        or fit_report.get("model_state", {}).get("sha256") != _sha256(args.model_state)
    ):
        _fail("bounded value fit report or state binding mismatch")
    fresh_collection = json.loads(args.fresh_collection.read_text(encoding="utf-8"))
    fresh_collection_sha256 = _sha256(args.fresh_collection)
    if (
        fresh_collection.get("schema") != terminal.COLLECTION_SCHEMA
        or fresh_collection.get("status") != "pass"
        or fresh_collection.get("base_seed") != FRESH_BASE_SEED
        or fresh_collection.get("pair_count") != FRESH_PAIR_COUNT
        or fresh_collection.get("scorer_sha256") != FRESH_SCORER_SHA256
    ):
        _fail("fresh bounded-value collection report mismatch")
    decisions, source, timings = terminal._load_decisions(args.fresh_cache, None)  # noqa: SLF001
    if (
        source["base_seed"] != FRESH_BASE_SEED
        or source["pair_count"] != FRESH_PAIR_COUNT
        or source["collection_sha256"] != fresh_collection_sha256
    ):
        _fail("fresh bounded-value confirmation panel identity mismatch")
    screen._configure(SEED, THREADS)  # noqa: SLF001
    payload = torch.load(args.model_state, map_location="cpu", weights_only=False)
    if (
        payload.get("schema") != FIT_SCHEMA + ".state"
        or payload.get("development_cache_sha256") != DEVELOPMENT_CACHE_SHA256
        or payload.get("initializer_state_sha256") != INITIALIZER_STATE_SHA256
    ):
        _fail("bounded value state provenance mismatch")
    model = distill._model()
    model.load_state_dict(payload["model_state_dict"], strict=True)
    metrics = _metrics(model, decisions)
    diagnostics = _diagnostics(model, decisions)
    overall = metrics["overall"]
    seats = metrics["by_candidate_seat"]
    gates = {
        "overall_mse_improvement_at_least_10_percent": overall["relative_improvement"] >= 0.10,
        "p0_mse_improvement_at_least_5_percent": seats["0"]["relative_improvement"] >= 0.05,
        "p1_mse_improvement_at_least_5_percent": seats["1"]["relative_improvement"] >= 0.05,
        "all_predictions_finite_and_bounded": overall["all_predictions_finite_and_bounded"],
        "initial_projected_parent_reproduction_within_1e_6": fit_report[
            "initial_alignment"
        ]["pass"],
        "permutation_sample_count_is_1024": diagnostics["permutation_sample_count"]
        == DIAGNOSTIC_SAMPLE_SIZE,
        "permutation_max_delta_at_most_1e_5": diagnostics[
            "permutation_max_value_delta"
        ]
        <= 1.0e-5,
        "reference_sample_count_is_1024": diagnostics["reference_sample_count"]
        == DIAGNOSTIC_SAMPLE_SIZE,
        "reference_affected_rate_at_least_20_percent": diagnostics[
            "reference_affected_rate"
        ]
        >= 0.20,
    }
    gates = {name: bool(value) for name, value in gates.items()}
    report = {
        "schema": CONFIRM_SCHEMA,
        "status": "pass" if all(gates.values()) else "reject",
        "source": {
            **source,
            "fresh_cache_sha256": _sha256(args.fresh_cache),
            "fresh_collection": str(args.fresh_collection),
            "fresh_collection_sha256": fresh_collection_sha256,
            "scorer_sha256": fresh_collection["scorer_sha256"],
        },
        "fit": {"path": str(args.fit_report), "sha256": _sha256(args.fit_report)},
        "model_state": {"path": str(args.model_state), "sha256": _sha256(args.model_state)},
        "metrics": metrics,
        "diagnostics": diagnostics,
        "gates": {**gates, "bounded_value_confirmation_pass": all(gates.values())},
        "timings": {**timings, "total_seconds": time.perf_counter() - started},
        "interpretation": (
            "Pass authorizes a fresh learned-value short-horizon search mechanism screen only."
            if all(gates.values())
            else "Bounded width-48 learned-value search is not authorized."
        ),
        "nonclaims": [
            "value-prediction confirmation only",
            "no search, policy, or strength result",
            "no promotion or pro-level claim",
        ],
    }
    _write_new(args.output, report)
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    fit_parser = subparsers.add_parser("fit")
    fit_parser.add_argument("--development-cache", type=Path, required=True)
    fit_parser.add_argument("--initializer-state", type=Path, required=True)
    fit_parser.add_argument("--model-state", type=Path, required=True)
    fit_parser.add_argument("--output", type=Path, required=True)
    confirm_parser = subparsers.add_parser("confirm")
    confirm_parser.add_argument("--fresh-cache", type=Path, required=True)
    confirm_parser.add_argument("--fresh-collection", type=Path, required=True)
    confirm_parser.add_argument("--fit-report", type=Path, required=True)
    confirm_parser.add_argument("--model-state", type=Path, required=True)
    confirm_parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = fit(args) if args.command == "fit" else confirm(args)
    if args.command == "fit":
        summary = {
            "status": report["status"],
            "train_seconds": report["timings"]["train_seconds"],
            "output": str(args.output),
        }
    else:
        summary = {
            "status": report["status"],
            "overall_mse_improvement": report["metrics"]["overall"]["relative_improvement"],
            "p0_mse_improvement": report["metrics"]["by_candidate_seat"]["0"][
                "relative_improvement"
            ],
            "p1_mse_improvement": report["metrics"]["by_candidate_seat"]["1"][
                "relative_improvement"
            ],
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
