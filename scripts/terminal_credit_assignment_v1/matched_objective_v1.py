#!/usr/bin/env python3
"""Run a matched head-only Monte Carlo versus GAE objective screen."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import sys
import time
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))
import screen_v1 as credit  # noqa: E402


SCHEMA = "mtg-kernel-native-matched-terminal-credit-objective/v1"
CACHE_SHA256 = "287d509794658bc167a7b61be450fa894d38ad837e7e6b212d49947629d542c6"
FIT_SEED = 20_260_812
EPOCHS = 5
BATCH_SIZE = 64
LEARNING_RATE = 3.0e-4
WEIGHT_DECAY = 1.0e-4
GRADIENT_CAP = 5.0
PPO_CLIP = 0.10
BOOTSTRAP_REPLICATES = 4_096
BOOTSTRAP_SEED = 20_260_813


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _attach_latents_and_advantages(
    decisions: list[Any], policy_model: Any, value_model: Any
) -> dict[str, Any]:
    captured: list[torch.Tensor] = []

    def capture(_module: Any, inputs: tuple[torch.Tensor, ...]) -> None:
        if len(inputs) != 1:
            _fail("policy-head hook input contract mismatch")
        captured.append(inputs[0].detach().clone())

    handle = policy_model.policy_head.register_forward_pre_hook(capture)
    maximum_logit_error = 0.0
    row_count = 0
    try:
        with torch.no_grad():
            for decision in decisions:
                for row in decision.rows:
                    captured.clear()
                    logits, _ = policy_model._one(row)  # noqa: SLF001
                    if len(captured) != 1 or captured[0].shape != (
                        logits.numel(),
                        credit.distill.DIM,
                    ):
                        _fail("policy-head latent capture mismatch")
                    maximum_logit_error = max(
                        maximum_logit_error,
                        float((logits - row["old_logits"]).abs().max()),
                    )
                    row["credit_policy_latent"] = captured[0]
                    row_count += 1
    finally:
        handle.remove()

    trajectories: dict[tuple[int, str, int], list[Any]] = {}
    for decision in decisions:
        trajectories.setdefault(decision.episode_key, []).append(decision)
    maximum_lambda_one_error = 0.0
    minimum_value = 1.0
    maximum_value = -1.0
    with torch.no_grad():
        for episode_key in sorted(trajectories):
            trajectory = sorted(
                trajectories[episode_key], key=lambda item: item.key[3]
            )
            rewards = {
                float(decision.rows[0]["terminal_reward"])
                for decision in trajectory
            }
            if len(rewards) != 1:
                _fail("trajectory mixes terminal rewards")
            reward = next(iter(rewards))
            values = [
                credit._bounded_value(value_model, decision.rows[0])
                for decision in trajectory
            ]
            mc = credit._gae(values, reward, 1.0)
            gae = credit._gae(values, reward, credit.LAMBDA)
            for index, decision in enumerate(trajectory):
                if any(
                    int(row["acting_seat"]) != decision.candidate_seat
                    for row in decision.rows
                ):
                    _fail("outcome trajectory contains a non-candidate decision")
                decision.credit_mc = mc[index]
                decision.credit_gae = gae[index]
                minimum_value = min(minimum_value, values[index])
                maximum_value = max(maximum_value, values[index])
                maximum_lambda_one_error = max(
                    maximum_lambda_one_error,
                    abs(mc[index] - (reward - values[index])),
                )
    return {
        "policy_row_count": row_count,
        "episode_count": len(trajectories),
        "maximum_policy_behavior_logit_error": maximum_logit_error,
        "maximum_lambda_one_monte_carlo_error": maximum_lambda_one_error,
        "minimum_value_prediction": minimum_value,
        "maximum_value_prediction": maximum_value,
        "all_value_predictions_finite_and_bounded": (
            math.isfinite(minimum_value)
            and math.isfinite(maximum_value)
            and -1.0 <= minimum_value <= maximum_value <= 1.0
        ),
    }


def _advantage_statistics(
    decisions: list[Any], attribute: str
) -> dict[int, dict[str, float]]:
    result: dict[int, dict[str, float]] = {}
    for seat in (0, 1):
        subset = [item for item in decisions if item.candidate_seat == seat]
        mass = sum(item.episode_weight for item in subset)
        mean = (
            sum(float(getattr(item, attribute)) * item.episode_weight for item in subset)
            / mass
        )
        variance = (
            sum(
                (float(getattr(item, attribute)) - mean) ** 2 * item.episode_weight
                for item in subset
            )
            / mass
        )
        result[seat] = {
            "mean": mean,
            "standard_deviation": max(math.sqrt(max(variance, 0.0)), 1.0e-9),
            "episode_mass": mass,
            "physical_decision_count": len(subset),
        }
    return result


def _install_advantages(
    decisions: list[Any], attribute: str, statistics: dict[int, dict[str, float]]
) -> None:
    target = f"{attribute}_standardized"
    for decision in decisions:
        seat = statistics[decision.candidate_seat]
        setattr(
            decision,
            target,
            (float(getattr(decision, attribute)) - seat["mean"])
            / seat["standard_deviation"],
        )


def _joint_log_probability(
    weight: torch.Tensor, bias: torch.Tensor, decision: Any
) -> torch.Tensor:
    terms: list[torch.Tensor] = []
    for row in decision.rows:
        logits = row["credit_policy_latent"].mv(weight) + bias
        terms.append(
            torch.log_softmax(logits, dim=0)[int(row["selected_index"])]
        )
    return torch.stack(terms).sum()


def _train_arm(
    initial_weight: torch.Tensor,
    bias: torch.Tensor,
    fit: list[Any],
    advantage_attribute: str,
) -> tuple[torch.Tensor, list[dict[str, Any]]]:
    weight = torch.nn.Parameter(initial_weight.detach().clone())
    optimizer = torch.optim.AdamW(
        [weight], lr=LEARNING_RATE, weight_decay=WEIGHT_DECAY
    )
    episode_mass = sum(decision.episode_weight for decision in fit)
    masses = {
        decision.key: decision.episode_weight * len(fit) / episode_mass
        for decision in fit
    }
    rng = random.Random(FIT_SEED)
    history: list[dict[str, Any]] = []
    for epoch in range(EPOCHS):
        order = list(range(len(fit)))
        rng.shuffle(order)
        loss_total = 0.0
        clip_total = 0.0
        gradient_norm_max = 0.0
        steps = 0
        for start in range(0, len(order), BATCH_SIZE):
            batch = [fit[index] for index in order[start : start + BATCH_SIZE]]
            surrogates: list[torch.Tensor] = []
            batch_masses: list[float] = []
            clipped = 0
            for decision in batch:
                joint = _joint_log_probability(weight, bias, decision)
                log_ratio = joint - decision.old_joint_log_probability
                ratio = torch.exp(log_ratio)
                clipped_ratio = torch.clamp(
                    ratio, 1.0 - PPO_CLIP, 1.0 + PPO_CLIP
                )
                advantage = float(getattr(decision, advantage_attribute))
                surrogates.append(
                    torch.minimum(ratio * advantage, clipped_ratio * advantage)
                )
                batch_masses.append(masses[decision.key])
                clipped += int(abs(float(log_ratio.detach())) > math.log1p(PPO_CLIP))
            mass_tensor = torch.tensor(batch_masses, dtype=torch.float32)
            loss = -(
                torch.stack(surrogates) * mass_tensor
            ).sum() / mass_tensor.sum()
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient_norm = torch.nn.utils.clip_grad_norm_([weight], GRADIENT_CAP)
            if not torch.isfinite(gradient_norm):
                _fail("non-finite policy-head gradient")
            optimizer.step()
            loss_total += float(loss.detach())
            clip_total += clipped / len(batch)
            gradient_norm_max = max(gradient_norm_max, float(gradient_norm))
            steps += 1
        history.append(
            {
                "epoch": epoch + 1,
                "mean_minibatch_loss": loss_total / steps,
                "mean_minibatch_clip_fraction": clip_total / steps,
                "maximum_preclip_gradient_norm": gradient_norm_max,
                "optimizer_steps": steps,
            }
        )
    return weight.detach(), history


def _weighted_quantile(samples: list[tuple[float, float]], quantile: float) -> float:
    ordered = sorted(samples, key=lambda item: item[0])
    target = quantile * sum(weight for _, weight in ordered)
    cumulative = 0.0
    for value, weight in ordered:
        cumulative += weight
        if cumulative >= target:
            return value
    return ordered[-1][0]


def _evaluate(
    weight: torch.Tensor,
    bias: torch.Tensor,
    decisions: list[Any],
    common_advantage_attribute: str,
) -> dict[str, Any]:
    records: list[dict[str, Any]] = []
    with torch.no_grad():
        for decision in decisions:
            joint = 0.0
            row_records: list[tuple[float, float, float]] = []
            for row in decision.rows:
                old_logits = row["old_logits"].double()
                new_logits = (
                    row["credit_policy_latent"].double().mv(weight.double())
                    + bias.double()
                )
                old_probability = torch.softmax(old_logits, dim=0)
                new_probability = torch.softmax(new_logits, dim=0)
                tv = 0.5 * float((old_probability - new_probability).abs().sum())
                kl = float(
                    (
                        old_probability
                        * (
                            torch.log(old_probability.clamp_min(1.0e-300))
                            - torch.log(new_probability.clamp_min(1.0e-300))
                        )
                    ).sum()
                )
                row_records.append((tv, kl, 1.0 / len(decision.rows)))
                joint += float(
                    torch.log_softmax(new_logits, dim=0)[int(row["selected_index"])]
                )
            log_ratio = joint - decision.old_joint_log_probability
            ratio = math.exp(log_ratio)
            gain = (ratio - 1.0) * float(
                getattr(decision, common_advantage_attribute)
            )
            records.append(
                {
                    "pair_index": decision.pair_index,
                    "seat": decision.candidate_seat,
                    "mass": decision.episode_weight,
                    "gain": gain,
                    "absolute_joint_log_ratio": abs(log_ratio),
                    "rows": row_records,
                }
            )

    def summarize(subset: list[dict[str, Any]]) -> dict[str, Any]:
        mass = sum(record["mass"] for record in subset)
        tv_samples: list[tuple[float, float]] = []
        tv_sum = 0.0
        kl_sum = 0.0
        for record in subset:
            for tv, kl, row_fraction in record["rows"]:
                row_mass = record["mass"] * row_fraction
                tv_samples.append((tv, row_mass))
                tv_sum += tv * row_mass
                kl_sum += kl * row_mass
        return {
            "surrogate": sum(record["gain"] * record["mass"] for record in subset)
            / mass,
            "mean_total_variation": tv_sum / mass,
            "p90_total_variation": _weighted_quantile(tv_samples, 0.90),
            "mean_parent_to_candidate_kl": kl_sum / mass,
            "maximum_absolute_joint_log_ratio": max(
                record["absolute_joint_log_ratio"] for record in subset
            ),
            "episode_mass": mass,
            "physical_decision_count": len(subset),
        }

    pair_numerators: dict[int, float] = {}
    pair_masses: dict[int, float] = {}
    for record in records:
        pair = record["pair_index"]
        pair_numerators[pair] = pair_numerators.get(pair, 0.0) + (
            record["gain"] * record["mass"]
        )
        pair_masses[pair] = pair_masses.get(pair, 0.0) + record["mass"]
    return {
        "overall": summarize(records),
        "by_candidate_seat": {
            str(seat): summarize([record for record in records if record["seat"] == seat])
            for seat in (0, 1)
        },
        "_pair_surrogates": {
            pair: pair_numerators[pair] / pair_masses[pair]
            for pair in sorted(pair_numerators)
        },
    }


def _bootstrap_difference(
    gae: dict[int, float], mc: dict[int, float]
) -> dict[str, Any]:
    pairs = sorted(gae)
    if pairs != sorted(mc):
        _fail("bootstrap arms do not cover identical pairs")
    differences = [gae[pair] - mc[pair] for pair in pairs]
    rng = random.Random(BOOTSTRAP_SEED)
    replicates = []
    for _ in range(BOOTSTRAP_REPLICATES):
        replicates.append(
            sum(differences[rng.randrange(len(differences))] for _ in pairs)
            / len(differences)
        )
    replicates.sort()
    lower_index = int(0.05 * BOOTSTRAP_REPLICATES)
    return {
        "unit": "heldout-pair",
        "pair_count": len(pairs),
        "replicates": BOOTSTRAP_REPLICATES,
        "seed": BOOTSTRAP_SEED,
        "point_mean": sum(differences) / len(differences),
        "lower_quantile": 0.05,
        "lower_index": lower_index,
        "lower_value": replicates[lower_index],
    }


def _public_evaluation(value: dict[str, Any]) -> dict[str, Any]:
    return {key: item for key, item in value.items() if not key.startswith("_")}


def run(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    identities = {
        "cache": (args.cache, CACHE_SHA256),
        "policy_state": (args.policy_state, credit.POLICY_STATE_SHA256),
        "value_state": (args.value_state, credit.VALUE_STATE_SHA256),
    }
    for label, (path, expected) in identities.items():
        observed = credit._sha256(path)
        if observed != expected:
            _fail(f"{label} SHA-256 mismatch: {observed}")
    credit.distill.EXPECTED_CACHE_SHA256 = CACHE_SHA256
    credit.distill.EXPECTED_PAIRS = 1_024
    decisions, source, load_timings = credit.distill._load_decisions(
        args.cache, args.pair_limit
    )
    loaded = time.perf_counter()
    credit.terminal.screen._configure(FIT_SEED, credit.THREADS)
    policy_model = credit._load_model(args.policy_state)
    value_model = credit._load_model(args.value_state)
    integrity = _attach_latents_and_advantages(
        decisions, policy_model, value_model
    )
    attached = time.perf_counter()
    fit = [decision for decision in decisions if decision.pair_index % 4 != 3]
    heldout = [decision for decision in decisions if decision.pair_index % 4 == 3]
    if not fit or not heldout:
        _fail("matched objective split is empty")
    if {item.episode_key for item in fit}.intersection(
        {item.episode_key for item in heldout}
    ):
        _fail("fit and heldout episodes overlap")
    statistics = {
        "monte_carlo": _advantage_statistics(fit, "credit_mc"),
        "gae_lambda_0p95": _advantage_statistics(fit, "credit_gae"),
    }
    _install_advantages(fit + heldout, "credit_mc", statistics["monte_carlo"])
    _install_advantages(
        fit + heldout, "credit_gae", statistics["gae_lambda_0p95"]
    )
    initial_weight = policy_model.policy_head.weight.detach().reshape(-1)
    bias = policy_model.policy_head.bias.detach().reshape(())
    mc_weight, mc_history = _train_arm(
        initial_weight, bias, fit, "credit_mc_standardized"
    )
    gae_weight, gae_history = _train_arm(
        initial_weight, bias, fit, "credit_gae_standardized"
    )
    trained = time.perf_counter()
    mc_evaluation = _evaluate(
        mc_weight, bias, heldout, "credit_mc_standardized"
    )
    gae_evaluation = _evaluate(
        gae_weight, bias, heldout, "credit_mc_standardized"
    )
    bootstrap = _bootstrap_difference(
        gae_evaluation["_pair_surrogates"],
        mc_evaluation["_pair_surrogates"],
    )
    evaluated = time.perf_counter()
    mc_public = _public_evaluation(mc_evaluation)
    gae_public = _public_evaluation(gae_evaluation)
    gates: dict[str, bool] = {
        "all_value_predictions_finite_and_bounded": integrity[
            "all_value_predictions_finite_and_bounded"
        ],
        "lambda_one_reproduces_monte_carlo_within_1e_9": integrity[
            "maximum_lambda_one_monte_carlo_error"
        ]
        <= 1.0e-9,
        "policy_initializer_alignment_within_1e_5": integrity[
            "maximum_policy_behavior_logit_error"
        ]
        <= 1.0e-5,
    }
    for arm, evaluation in (("mc", mc_public), ("gae", gae_public)):
        gates[f"{arm}_mean_tv_at_most_0p03"] = (
            evaluation["overall"]["mean_total_variation"] <= 0.03
        )
        gates[f"{arm}_p90_tv_at_most_0p10"] = (
            evaluation["overall"]["p90_total_variation"] <= 0.10
        )
        gates[f"{arm}_max_joint_log_ratio_at_most_0p50"] = (
            evaluation["overall"]["maximum_absolute_joint_log_ratio"] <= 0.50
        )
    gates.update(
        {
            "gae_common_heldout_surrogate_positive_overall": gae_public[
                "overall"
            ]["surrogate"]
            > 0.0,
            "gae_common_heldout_surrogate_positive_both_seats": all(
                gae_public["by_candidate_seat"][str(seat)]["surrogate"] > 0.0
                for seat in (0, 1)
            ),
            "gae_common_heldout_surrogate_exceeds_mc_overall": gae_public[
                "overall"
            ]["surrogate"]
            > mc_public["overall"]["surrogate"],
            "gae_common_heldout_surrogate_exceeds_mc_both_seats": all(
                gae_public["by_candidate_seat"][str(seat)]["surrogate"]
                > mc_public["by_candidate_seat"][str(seat)]["surrogate"]
                for seat in (0, 1)
            ),
            "paired_bootstrap_lower_above_zero": bootstrap["lower_value"] > 0.0,
        }
    )
    decision = "ADVANCE_TO_FULL_FIT" if all(gates.values()) else "REJECT"
    result = {
        "schema": SCHEMA,
        "status": "complete",
        "decision": decision,
        "source": source,
        "inputs": {
            label: {"path": str(path), "sha256": expected}
            for label, (path, expected) in identities.items()
        },
        "split": {
            "rule": "pair-index-mod-4-heldout-3/v1",
            "fit_pairs": len({item.pair_index for item in fit}),
            "heldout_pairs": len({item.pair_index for item in heldout}),
            "fit_physical_decisions": len(fit),
            "heldout_physical_decisions": len(heldout),
        },
        "config": {
            "terminal_reward": "natural-win-draw-loss-only/v1",
            "nonterminal_reward": 0,
            "gamma": 1.0,
            "gae_lambda": credit.LAMBDA,
            "epochs": EPOCHS,
            "batch_size_physical_decisions": BATCH_SIZE,
            "learning_rate": LEARNING_RATE,
            "weight_decay": WEIGHT_DECAY,
            "gradient_norm_cap": GRADIENT_CAP,
            "ppo_clip": PPO_CLIP,
            "seed": FIT_SEED,
            "threads": credit.THREADS,
            "trainable_parameter": "policy_head.weight",
            "trainable_parameter_count": int(initial_weight.numel()),
            "common_heldout_estimator": "width48-monte-carlo/v1",
        },
        "integrity": integrity,
        "advantage_statistics": {
            method: {str(seat): value for seat, value in values.items()}
            for method, values in statistics.items()
        },
        "arms": {
            "monte_carlo": {
                "training_history": mc_history,
                "heldout": mc_public,
                "weight_sha256": hashlib.sha256(
                    mc_weight.float().numpy().astype("<f4", copy=False).tobytes()
                ).hexdigest(),
            },
            "gae_lambda_0p95": {
                "training_history": gae_history,
                "heldout": gae_public,
                "weight_sha256": hashlib.sha256(
                    gae_weight.float().numpy().astype("<f4", copy=False).tobytes()
                ).hexdigest(),
            },
        },
        "paired_bootstrap": bootstrap,
        "gates": gates,
        "timings": {
            **load_timings,
            "load_and_model_seconds": loaded - started,
            "latent_and_value_seconds": attached - loaded,
            "train_both_arms_seconds": trained - attached,
            "evaluate_both_arms_seconds": evaluated - trained,
            "total_seconds": evaluated - started,
        },
        "nonclaims": [
            "offline matched objective screen only",
            "heldout surrogate is not game strength",
            "no promotion or professional-level claim",
        ],
    }
    credit._write_new(args.output, result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--policy-state", type=Path, required=True)
    parser.add_argument("--value-state", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--pair-limit", type=int)
    args = parser.parse_args()
    if args.pair_limit is not None and args.pair_limit < 16:
        _fail("pair limit must be at least sixteen")
    print(json.dumps(run(args), sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
