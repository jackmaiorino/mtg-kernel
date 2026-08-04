#!/usr/bin/env python3
"""Compare terminal Monte Carlo and GAE policy-head credit assignment."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import math
from pathlib import Path
import sys
import time
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
SCRIPTS_DIR = SCRIPT_DIR.parent
TERMINAL_DIR = SCRIPTS_DIR / "policy_only_structured_terminal_rung_v1"
STRUCTURED_DIR = SCRIPTS_DIR / "structured_adapter_screen_v1"
VALUE_DIR = SCRIPTS_DIR / "bounded_onpolicy_history_value_v1"
for directory in (TERMINAL_DIR, STRUCTURED_DIR, VALUE_DIR):
    sys.path.insert(0, str(directory))

import fit_and_confirm_v1 as bounded  # noqa: E402
import run_pipeline_v1 as terminal  # noqa: E402
import run_structured_successor_distillation_v1 as distill  # noqa: E402


SCHEMA = "mtg-kernel-native-terminal-credit-assignment-screen/v1"
CACHE_SHA256 = "44eae5bee2b5556faa6293c80a88cb8f67f90d46066ffb5115ced2daac579800"
POLICY_STATE_SHA256 = "ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0"
VALUE_STATE_SHA256 = "cae8e19ef825325508de351b883b2df3863dc66f0288be06ad2ccf868e3d7d7c"
CONFIRMATION_SHA256 = "716189e49c635eebdf5647e17ef4e3b3ab684c68addbc6b3c94fc3bed46f7539"
LAMBDA = 0.95
MATERIAL_TD_THRESHOLD = 0.25
PREDICTION_EPSILON = 1.0e-3
THREADS = 24


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


@dataclass
class CreditRecord:
    pair_index: int
    candidate_seat: int
    episode_key: tuple[int, str, int]
    episode_weight: float
    position: int
    episode_length: int
    terminal_reward: float
    value: float
    td_residual: float
    score: torch.Tensor
    mc: float = 0.0
    gae: float = 0.0


def _gae(values: list[float], terminal_reward: float, lam: float) -> list[float]:
    if not values:
        _fail("cannot estimate advantages for an empty trajectory")
    if not 0.0 <= lam <= 1.0:
        _fail("lambda must be in [0, 1]")
    result = [0.0] * len(values)
    running = 0.0
    for index in range(len(values) - 1, -1, -1):
        reward = terminal_reward if index == len(values) - 1 else 0.0
        next_value = values[index + 1] if index + 1 < len(values) else 0.0
        delta = reward + next_value - values[index]
        running = delta + lam * running
        result[index] = running
    return result


def _score_vector(latent: torch.Tensor, logits: torch.Tensor, selected: int) -> torch.Tensor:
    if latent.ndim != 2 or logits.ndim != 1 or latent.shape[0] != logits.numel():
        _fail("policy latent or logit shape mismatch")
    if not 0 <= selected < logits.numel():
        _fail("selected action is outside logits")
    probability = torch.softmax(logits.double(), dim=0)
    latent64 = latent.double()
    return latent64[selected] - probability @ latent64


def _load_model(path: Path) -> Any:
    payload = torch.load(path, map_location="cpu", weights_only=False)
    state = payload.get("model_state_dict")
    if not isinstance(state, dict):
        _fail(f"model state payload lacks model_state_dict: {path}")
    model = distill._model()
    model.load_state_dict(state, strict=True)
    model.eval()
    return model


def _bounded_value(model: Any, row: dict[str, Any]) -> float:
    _, raw_residual = model._one(row)  # noqa: SLF001
    parent = row["old_value"].clamp(
        -1.0 + PREDICTION_EPSILON, 1.0 - PREDICTION_EPSILON
    )
    shift = torch.tanh(raw_residual)
    prediction = (parent + shift) / (1.0 + parent * shift)
    value = float(prediction)
    if not math.isfinite(value) or not -1.0 <= value <= 1.0:
        _fail("bounded value prediction is non-finite or outside [-1, 1]")
    return value


def _capture_records(
    decisions: list[Any], policy_model: Any, value_model: Any
) -> tuple[list[CreditRecord], dict[str, Any]]:
    trajectories: dict[tuple[int, str, int], list[Any]] = {}
    for decision in decisions:
        trajectories.setdefault(decision.episode_key, []).append(decision)
    captured: list[torch.Tensor] = []

    def capture(_module: Any, inputs: tuple[torch.Tensor, ...]) -> None:
        if len(inputs) != 1:
            _fail("policy-head hook input contract mismatch")
        captured.append(inputs[0].detach().clone())

    handle = policy_model.policy_head.register_forward_pre_hook(capture)
    records: list[CreditRecord] = []
    maximum_policy_logit_error = 0.0
    try:
        with torch.no_grad():
            for episode_key in sorted(trajectories):
                trajectory = sorted(
                    trajectories[episode_key], key=lambda item: item.key[3]
                )
                reward_set = {
                    float(decision.rows[0]["terminal_reward"])
                    for decision in trajectory
                }
                if len(reward_set) != 1:
                    _fail("trajectory mixes terminal rewards")
                reward = next(iter(reward_set))
                values: list[float] = []
                scores: list[torch.Tensor] = []
                for decision in trajectory:
                    if any(
                        int(row["acting_seat"]) != decision.candidate_seat
                        for row in decision.rows
                    ):
                        _fail("outcome trajectory contains a non-candidate decision")
                    value = _bounded_value(value_model, decision.rows[0])
                    score = torch.zeros(distill.DIM, dtype=torch.float64)
                    for row in decision.rows:
                        captured.clear()
                        logits, _ = policy_model._one(row)  # noqa: SLF001
                        if len(captured) != 1 or captured[0].shape != (
                            logits.numel(),
                            distill.DIM,
                        ):
                            _fail("policy-head latent capture mismatch")
                        maximum_policy_logit_error = max(
                            maximum_policy_logit_error,
                            float((logits - row["old_logits"]).abs().max()),
                        )
                        score += _score_vector(
                            captured[0], row["old_logits"], int(row["selected_index"])
                        )
                    values.append(value)
                    scores.append(score)
                mc = _gae(values, reward, 1.0)
                gae = _gae(values, reward, LAMBDA)
                for index, decision in enumerate(trajectory):
                    next_value = values[index + 1] if index + 1 < len(values) else 0.0
                    transition_reward = reward if index + 1 == len(values) else 0.0
                    records.append(
                        CreditRecord(
                            pair_index=decision.pair_index,
                            candidate_seat=decision.candidate_seat,
                            episode_key=episode_key,
                            episode_weight=decision.episode_weight,
                            position=index,
                            episode_length=len(trajectory),
                            terminal_reward=reward,
                            value=values[index],
                            td_residual=transition_reward + next_value - values[index],
                            score=scores[index],
                            mc=mc[index],
                            gae=gae[index],
                        )
                    )
    finally:
        handle.remove()
    maximum_lambda_one_error = max(
        abs(record.mc - (record.terminal_reward - record.value)) for record in records
    )
    return records, {
        "episode_count": len(trajectories),
        "physical_decision_count": len(records),
        "maximum_policy_behavior_logit_error": maximum_policy_logit_error,
        "maximum_lambda_one_monte_carlo_error": maximum_lambda_one_error,
        "all_predictions_finite_and_bounded": all(
            math.isfinite(record.value) and -1.0 <= record.value <= 1.0
            for record in records
        ),
    }


def _weighted_summary(values: list[tuple[float, float]]) -> dict[str, float]:
    mass = sum(weight for _, weight in values)
    if mass <= 0.0:
        _fail("weighted summary has no mass")
    mean = sum(value * weight for value, weight in values) / mass
    variance = sum((value - mean) ** 2 * weight for value, weight in values) / mass
    return {
        "mean": mean,
        "variance": variance,
        "standard_deviation": math.sqrt(max(variance, 0.0)),
        "mean_absolute": sum(abs(value) * weight for value, weight in values) / mass,
        "mass": mass,
    }


def _standardization(
    records: list[CreditRecord], attribute: str
) -> dict[int, dict[str, float]]:
    result: dict[int, dict[str, float]] = {}
    for seat in (0, 1):
        summary = _weighted_summary(
            [
                (float(getattr(record, attribute)), record.episode_weight)
                for record in records
                if record.candidate_seat == seat
            ]
        )
        result[seat] = {
            "mean": summary["mean"],
            "standard_deviation": max(summary["standard_deviation"], 1.0e-9),
        }
    return result


def _cosine(left: torch.Tensor, right: torch.Tensor) -> float:
    denominator = float(torch.linalg.vector_norm(left) * torch.linalg.vector_norm(right))
    if denominator <= 0.0:
        return 0.0
    return float(torch.dot(left, right) / denominator)


def _gradient_metrics(vectors: list[tuple[int, int, torch.Tensor]]) -> dict[str, Any]:
    if len(vectors) < 4:
        _fail("gradient panel is too small")
    matrix = torch.stack([vector for _, _, vector in vectors])
    mean = matrix.mean(dim=0)
    variance = matrix.var(dim=0, unbiased=True)
    standard_error = math.sqrt(float(variance.sum()) / len(vectors))
    even = torch.stack(
        [vector for pair, _, vector in vectors if pair % 2 == 0]
    ).mean(dim=0)
    odd = torch.stack(
        [vector for pair, _, vector in vectors if pair % 2 == 1]
    ).mean(dim=0)
    norm = float(torch.linalg.vector_norm(mean))
    return {
        "pair_vector_count": len(vectors),
        "mean_gradient_l2": norm,
        "gradient_standard_error_l2": standard_error,
        "signal_to_noise": norm / max(standard_error, 1.0e-15),
        "even_odd_cosine": _cosine(even, odd),
    }


def _method_metrics(records: list[CreditRecord], attribute: str) -> dict[str, Any]:
    statistics = _standardization(records, attribute)
    pair_vectors: dict[tuple[int, int], torch.Tensor] = {}
    pair_masses: dict[tuple[int, int], float] = {}
    material: list[tuple[float, float]] = []
    other: list[tuple[float, float]] = []
    for record in records:
        raw = float(getattr(record, attribute))
        seat_stats = statistics[record.candidate_seat]
        advantage = (raw - seat_stats["mean"]) / seat_stats["standard_deviation"]
        contribution = advantage * record.score
        key = (record.pair_index, record.candidate_seat)
        pair_vectors[key] = pair_vectors.get(
            key, torch.zeros(distill.DIM, dtype=torch.float64)
        ) + record.episode_weight * contribution
        pair_masses[key] = pair_masses.get(key, 0.0) + record.episode_weight
        weighted_signal = abs(advantage) * float(torch.linalg.vector_norm(record.score))
        target = material if abs(record.td_residual) >= MATERIAL_TD_THRESHOLD else other
        target.append((weighted_signal, record.episode_weight))
    seat_vectors = [
        (pair, seat, vector / pair_masses[(pair, seat)])
        for (pair, seat), vector in sorted(pair_vectors.items())
    ]
    combined: list[tuple[int, int, torch.Tensor]] = []
    for pair in sorted({item[0] for item in seat_vectors}):
        members = [item[2] for item in seat_vectors if item[0] == pair]
        if len(members) != 2:
            _fail("pair does not contain both seat-swapped episodes")
        combined.append((pair, -1, torch.stack(members).mean(dim=0)))
    material_summary = _weighted_summary(material)
    other_summary = _weighted_summary(other)
    return {
        "advantage": {
            "overall": _weighted_summary(
                [(float(getattr(record, attribute)), record.episode_weight) for record in records]
            ),
            "by_candidate_seat": {
                str(seat): _weighted_summary(
                    [
                        (float(getattr(record, attribute)), record.episode_weight)
                        for record in records
                        if record.candidate_seat == seat
                    ]
                )
                for seat in (0, 1)
            },
            "standardization": {str(key): value for key, value in statistics.items()},
        },
        "gradient": {
            "overall": _gradient_metrics(combined),
            "by_candidate_seat": {
                str(seat): _gradient_metrics(
                    [item for item in seat_vectors if item[1] == seat]
                )
                for seat in (0, 1)
            },
        },
        "material_transition": {
            "absolute_td_threshold": MATERIAL_TD_THRESHOLD,
            "material_decision_count": len(material),
            "other_decision_count": len(other),
            "material_mean_score_weighted_absolute_signal": material_summary["mean"],
            "other_mean_score_weighted_absolute_signal": other_summary["mean"],
            "material_to_other_contrast": material_summary["mean"]
            / max(other_summary["mean"], 1.0e-15),
        },
        "_overall_gradient": torch.stack([item[2] for item in combined]).mean(dim=0),
    }


def _public_method(value: dict[str, Any]) -> dict[str, Any]:
    return {key: item for key, item in value.items() if not key.startswith("_")}


def _metric_digest(methods: dict[str, dict[str, Any]], integrity: dict[str, Any]) -> str:
    public = {key: _public_method(value) for key, value in methods.items()}
    return hashlib.sha256(_json_bytes({"integrity": integrity, "methods": public})).hexdigest()


def run(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    identities = {
        "cache": (args.cache, CACHE_SHA256),
        "policy_state": (args.policy_state, POLICY_STATE_SHA256),
        "value_state": (args.value_state, VALUE_STATE_SHA256),
        "value_confirmation": (args.value_confirmation, CONFIRMATION_SHA256),
    }
    for label, (path, expected) in identities.items():
        observed = _sha256(path)
        if observed != expected:
            _fail(f"{label} SHA-256 mismatch: {observed}")
    confirmation = json.loads(args.value_confirmation.read_text(encoding="utf-8"))
    if not confirmation.get("gates", {}).get("bounded_value_confirmation_pass"):
        _fail("bounded value confirmation is not a pass")
    decisions, source, load_timings = terminal._load_decisions(
        args.cache, args.pair_limit
    )
    loaded = time.perf_counter()
    terminal.screen._configure(args.seed, THREADS)
    policy_model = _load_model(args.policy_state)
    value_model = _load_model(args.value_state)
    records, integrity = _capture_records(decisions, policy_model, value_model)
    captured = time.perf_counter()
    methods = {
        "monte_carlo": _method_metrics(records, "mc"),
        "gae_lambda_0p95": _method_metrics(records, "gae"),
    }
    measured = time.perf_counter()
    mc = methods["monte_carlo"]
    gae = methods["gae_lambda_0p95"]
    mc_gradient = mc["_overall_gradient"]
    gae_gradient = gae["_overall_gradient"]
    mc_overall = mc["gradient"]["overall"]
    gae_overall = gae["gradient"]["overall"]
    gates = {
        "width48_external_value_confirmation_pass": True,
        "all_predictions_finite_and_bounded": integrity[
            "all_predictions_finite_and_bounded"
        ],
        "lambda_one_reproduces_monte_carlo_within_1e_9": integrity[
            "maximum_lambda_one_monte_carlo_error"
        ]
        <= 1.0e-9,
        "policy_initializer_alignment_within_1e_5": integrity[
            "maximum_policy_behavior_logit_error"
        ]
        <= 1.0e-5,
        "gae_raw_advantage_variance_below_monte_carlo": gae["advantage"][
            "overall"
        ]["variance"]
        < mc["advantage"]["overall"]["variance"],
        "gae_even_odd_gradient_cosine_nonnegative": gae_overall[
            "even_odd_cosine"
        ]
        >= 0.0,
        "gae_even_odd_gradient_cosine_improves_by_0p05": gae_overall[
            "even_odd_cosine"
        ]
        >= mc_overall["even_odd_cosine"] + 0.05,
        "gae_overall_gradient_snr_at_least_1p10x_monte_carlo": gae_overall[
            "signal_to_noise"
        ]
        >= 1.10 * mc_overall["signal_to_noise"],
        "gae_both_seat_gradient_snr_at_least_0p90x_monte_carlo": all(
            gae["gradient"]["by_candidate_seat"][str(seat)]["signal_to_noise"]
            >= 0.90
            * mc["gradient"]["by_candidate_seat"][str(seat)]["signal_to_noise"]
            for seat in (0, 1)
        ),
        "gae_material_transition_contrast_at_least_monte_carlo": gae[
            "material_transition"
        ]["material_to_other_contrast"]
        >= mc["material_transition"]["material_to_other_contrast"],
    }
    decision = (
        "ADVANCE_TO_MATCHED_OBJECTIVE" if all(gates.values()) else "REJECT"
    )
    metric_digest = _metric_digest(methods, integrity)
    result = {
        "schema": SCHEMA,
        "status": "complete",
        "decision": decision,
        "source": source,
        "inputs": {
            label: {"path": str(path), "sha256": expected}
            for label, (path, expected) in identities.items()
        },
        "config": {
            "gamma": 1.0,
            "gae_lambda": LAMBDA,
            "nonterminal_reward": 0,
            "terminal_reward": "natural-win-draw-loss-only/v1",
            "threads": THREADS,
            "seed": args.seed,
            "pair_limit": args.pair_limit,
            "policy_gradient_basis": "frozen-structured-final-head-weight-48/v1",
            "weighting": "equal-episode-equal-physical-decision/v1",
            "independent_halves": "pair-index-even-versus-odd/v1",
        },
        "value_confirmation": {
            "status": confirmation["status"],
            "overall_mse": confirmation["metrics"]["overall"]["candidate_mse"],
            "p0_mse": confirmation["metrics"]["by_candidate_seat"]["0"][
                "candidate_mse"
            ],
            "p1_mse": confirmation["metrics"]["by_candidate_seat"]["1"][
                "candidate_mse"
            ],
        },
        "integrity": integrity,
        "methods": {key: _public_method(value) for key, value in methods.items()},
        "comparison": {
            "overall_gradient_cosine_monte_carlo_to_gae": _cosine(
                mc_gradient, gae_gradient
            ),
            "gae_to_monte_carlo_raw_variance_ratio": gae["advantage"]["overall"][
                "variance"
            ]
            / mc["advantage"]["overall"]["variance"],
            "gae_to_monte_carlo_gradient_snr_ratio": gae_overall[
                "signal_to_noise"
            ]
            / max(mc_overall["signal_to_noise"], 1.0e-15),
            "gae_minus_monte_carlo_even_odd_cosine": gae_overall[
                "even_odd_cosine"
            ]
            - mc_overall["even_odd_cosine"],
        },
        "gates": gates,
        "metric_digest": metric_digest,
        "timings": {
            **load_timings,
            "load_and_model_seconds": loaded - started,
            "capture_seconds": captured - loaded,
            "metric_seconds": measured - captured,
            "total_seconds": measured - started,
        },
        "nonclaims": [
            "frozen-trajectory mechanism diagnostic only",
            "no policy fit or game-strength result",
            "no promotion or professional-level claim",
        ],
    }
    _write_new(args.output, result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--policy-state", type=Path, required=True)
    parser.add_argument("--value-state", type=Path, required=True)
    parser.add_argument("--value-confirmation", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--pair-limit", type=int)
    parser.add_argument("--seed", type=int, default=20_260_811)
    args = parser.parse_args()
    if args.pair_limit is not None and args.pair_limit < 4:
        _fail("pair limit must be at least four")
    print(json.dumps(run(args), sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
