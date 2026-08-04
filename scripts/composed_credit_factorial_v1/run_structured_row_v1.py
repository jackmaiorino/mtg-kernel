#!/usr/bin/env python3
"""Run the width-48 structured arm of the composed credit-factorial screen.

This wrapper intentionally owns no training implementation.  It reuses the
validated terminal-rung loader, structured model, alignment, movement, and
physical-decision PPO objective.  The only new objective surface is selecting
the frozen-parent Monte Carlo advantage or the confirmed complete-history
critic plus terminal-only GAE advantage.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import random
import sys
import time
from pathlib import Path
from types import SimpleNamespace
from typing import Any, Callable, Iterable

import torch


HERE = Path(__file__).resolve().parent
SCRIPTS = HERE.parent
STRUCTURED = SCRIPTS / "structured_adapter_screen_v1"
TERMINAL = SCRIPTS / "policy_only_structured_terminal_rung_v1"
CREDIT = SCRIPTS / "terminal_credit_assignment_v1"
for import_root in (STRUCTURED, TERMINAL, CREDIT):
    if str(import_root) not in sys.path:
        sys.path.insert(0, str(import_root))

import fit_policy_only_structured_successor_v1 as initializer  # noqa: E402
import run_pipeline_v1 as terminal  # noqa: E402
import run_screen as structured_screen  # noqa: E402
import run_structured_outcome_policy_v1 as outcome  # noqa: E402
import run_structured_successor_distillation_v1 as distill  # noqa: E402
import screen_v1 as credit_screen  # noqa: E402


SCHEMA = "mtg-kernel-composed-credit-factorial-structured-row/v1"
DEFAULT_CACHE = Path(
    r"D:\mtg-kernel-policy-only-structured-terminal-rung-v1\formal\cache.pt"
)
DEFAULT_INITIALIZER_STATE = Path(
    r"D:\mtg-kernel-policy-only-structured-successor-v1\candidate.state.pt"
)
DEFAULT_VALUE_STATE = Path(
    r"D:\mtg-kernel-bounded-onpolicy-history-value-v1\model.state.pt"
)
DEFAULT_VALUE_CONFIRMATION = Path(
    r"D:\mtg-kernel-bounded-onpolicy-history-value-v1\confirmation.json"
)
EXPECTED_CACHE_SHA256 = (
    "454e4ce1b8f7413839a36c8e2731fc0cb65581ce13e593634bffa70013a6f16d"
)
EXPECTED_INITIALIZER_STATE_SHA256 = (
    "ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0"
)
EXPECTED_INITIALIZER_JSON_SHA256 = (
    "204beb91c1a4b039e0c497f2b420e823b5cc9e2ceb8560f897d0b6251e916b72"
)
EXPECTED_VALUE_STATE_SHA256 = (
    "cae8e19ef825325508de351b883b2df3863dc66f0288be06ad2ccf868e3d7d7c"
)
EXPECTED_VALUE_CONFIRMATION_SHA256 = (
    "716189e49c635eebdf5647e17ef4e3b3ab684c68addbc6b3c94fc3bed46f7539"
)
EXPECTED_VALUE_SOURCE_CACHE_SHA256 = (
    "44eae5bee2b5556faa6293c80a88cb8f67f90d46066ffb5115ced2daac579800"
)
EXPECTED_VALUE_CONFIRMATION_SCHEMA = (
    "mtg-kernel-bounded-onpolicy-history-value-confirmation/v1"
)
EXPECTED_VALUE_STATE_SCHEMA = "mtg-kernel-bounded-onpolicy-history-value-fit/v1.state"
EXPECTED_HISTORY_FEATURE_DIM = 237
EXPECTED_HISTORY_LENGTH = 16
DEFAULT_PAIR_LIMIT = 512
DEFAULT_EPOCHS = 1
DEFAULT_THREADS = 24
DEFAULT_SEED = 20_260_805
DEFAULT_BATCH_SIZE = 64
LEARNING_RATE = 3.0e-4
WEIGHT_DECAY = 0.0
PPO_CLIP = 0.10
GRADIENT_CAP = 5.0
GAE_GAMMA = 1.0
GAE_LAMBDA = 0.95
ALIGNMENT_LIMIT = 3.0e-5


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    if not path.is_file() or path.is_symlink():
        _fail(f"required path is not a regular file: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _json_bytes(value: Any) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n").encode(
        "utf-8"
    )


def _write_new(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite output: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(_json_bytes(value))


def _state_digest(model: torch.nn.Module) -> str:
    digest = hashlib.sha256()
    for name, tensor in model.state_dict().items():
        raw = tensor.detach().cpu().contiguous()
        digest.update(name.encode("utf-8"))
        digest.update(b"\0")
        digest.update(str(tuple(raw.shape)).encode("ascii"))
        digest.update(b"\0")
        digest.update(raw.float().numpy().astype("<f4", copy=False).tobytes())
    return digest.hexdigest()


def _value_head_digest(model: torch.nn.Module) -> str:
    return hashlib.sha256(initializer._value_head_bits(model)).hexdigest()


def terminal_gae(
    values: Iterable[float],
    terminal_reward: float,
    *,
    gamma: float = GAE_GAMMA,
    lam: float = GAE_LAMBDA,
) -> list[float]:
    """Compute terminal-only GAE over physical decisions."""

    values_list = [float(value) for value in values]
    if not values_list:
        _fail("GAE requires at least one physical decision")
    if not math.isfinite(float(terminal_reward)):
        _fail("terminal reward is non-finite")
    if not 0.0 <= gamma <= 1.0 or not 0.0 <= lam <= 1.0:
        _fail("gamma and lambda must be in [0, 1]")
    if any(not math.isfinite(value) for value in values_list):
        _fail("critic value is non-finite")
    result = [0.0] * len(values_list)
    running = 0.0
    for index in range(len(values_list) - 1, -1, -1):
        reward = float(terminal_reward) if index == len(values_list) - 1 else 0.0
        next_value = values_list[index + 1] if index + 1 < len(values_list) else 0.0
        delta = reward + gamma * next_value - values_list[index]
        running = delta + gamma * lam * running
        result[index] = running
    if any(not math.isfinite(value) for value in result):
        _fail("GAE result is non-finite")
    return result


def _decision_key(decision: Any) -> tuple[Any, ...]:
    return tuple(decision.key)


def _validate_history_rows(decisions: list[Any]) -> dict[str, Any]:
    if not decisions:
        _fail("selected decision panel is empty")
    episode_groups: dict[tuple[Any, ...], list[Any]] = {}
    for decision in decisions:
        if decision.candidate_seat not in (0, 1):
            _fail("candidate seat is outside {0, 1}")
        if not math.isfinite(float(decision.episode_weight)) or decision.episode_weight <= 0.0:
            _fail("decision episode weight is invalid")
        episode_groups.setdefault(tuple(decision.episode_key), []).append(decision)
        rewards = {float(row["terminal_reward"]) for row in decision.rows}
        if len(rewards) != 1 or not rewards.issubset({-1.0, 0.0, 1.0}):
            _fail("physical decision has inconsistent or invalid terminal reward")
        histories: list[torch.Tensor] = []
        for row in decision.rows:
            history = row.get("history_features")
            if not isinstance(history, torch.Tensor) or history.ndim != 2:
                _fail("complete history is not a rank-2 tensor")
            if history.shape[0] > EXPECTED_HISTORY_LENGTH or history.shape[1] != EXPECTED_HISTORY_FEATURE_DIM:
                _fail("history length or feature dimension is outside the contract")
            if not bool(torch.isfinite(history).all()):
                _fail("complete history contains a non-finite feature")
            if int(row["acting_seat"]) != decision.candidate_seat:
                _fail("value-lane decision contains an opponent action")
            old_value = float(row["old_value"])
            if not math.isfinite(old_value) or not -1.0 <= old_value <= 1.0:
                _fail("frozen parent value is outside [-1, 1]")
            histories.append(history)
        if any(not torch.equal(histories[0], history) for history in histories[1:]):
            _fail("substeps of one physical decision do not share prior history")
    for episode_key, trajectory in episode_groups.items():
        ordered = sorted(trajectory, key=lambda item: int(item.key[3]))
        groups = [int(item.key[3]) for item in ordered]
        if len(set(groups)) != len(groups) or groups != sorted(groups):
            _fail(f"episode physical decisions are not uniquely ordered: {episode_key}")
        if any(item.episode_key != episode_key for item in ordered):
            _fail("episode grouping key mismatch")
    return {
        "episode_count": len(episode_groups),
        "physical_decision_count": len(decisions),
        "row_count": sum(len(decision.rows) for decision in decisions),
        "history_length_max": max(
            int(row["history_features"].shape[0])
            for decision in decisions
            for row in decision.rows
        ),
        "all_history_before_current_decision": True,
    }


def _critic_value(model: Any, row: dict[str, Any]) -> float:
    with torch.no_grad():
        value = float(credit_screen._bounded_value(model, row))
    if not math.isfinite(value) or not -1.0 <= value <= 1.0:
        _fail("complete-history critic output is outside [-1, 1]")
    return value


def route_advantages(
    decisions: list[Any],
    mode: str,
    *,
    critic_model: Any | None = None,
    value_fn: Callable[[Any, dict[str, Any]], float] | None = None,
) -> dict[str, Any]:
    """Install one arm's raw and seat-standardized advantages in place."""

    if mode not in ("mc", "gae"):
        _fail(f"unknown advantage mode: {mode}")
    history_integrity = _validate_history_rows(decisions)
    trajectories: dict[tuple[Any, ...], list[Any]] = {}
    for decision in decisions:
        trajectories.setdefault(tuple(decision.episode_key), []).append(decision)
    values_seen: list[float] = []
    for episode_key in sorted(trajectories):
        trajectory = sorted(trajectories[episode_key], key=lambda item: int(item.key[3]))
        reward = float(trajectory[-1].rows[0]["terminal_reward"])
        if any(float(item.rows[0]["terminal_reward"]) != reward for item in trajectory):
            _fail("trajectory mixes terminal rewards")
        if mode == "mc":
            for decision in trajectory:
                decision.raw_advantage = reward - float(decision.rows[0]["old_value"])
        else:
            if critic_model is None and value_fn is None:
                _fail("GAE requires a confirmed complete-history critic")
            prediction = value_fn or _critic_value
            values = [prediction(critic_model, decision.rows[0]) for decision in trajectory]
            values_seen.extend(values)
            for decision, advantage in zip(
                trajectory, terminal_gae(values, reward), strict=True
            ):
                decision.raw_advantage = advantage
    statistics = outcome._advantage_statistics(decisions)
    outcome._install_standardized_advantages(decisions, statistics)
    result: dict[str, Any] = {
        "mode": mode,
        "gamma": GAE_GAMMA if mode == "gae" else None,
        "gae_lambda": GAE_LAMBDA if mode == "gae" else None,
        "advantage_statistics_by_candidate_seat": {
            str(seat): values for seat, values in statistics.items()
        },
        "history_integrity": history_integrity,
    }
    if mode == "gae":
        result["critic_integrity"] = {
            "prediction_count": len(values_seen),
            "minimum_prediction": min(values_seen),
            "maximum_prediction": max(values_seen),
            "all_predictions_finite_and_bounded": all(
                math.isfinite(value) and -1.0 <= value <= 1.0 for value in values_seen
            ),
        }
    return result


def _load_initializer_state(path: Path) -> tuple[dict[str, Any], str]:
    observed = _sha256(path)
    if observed != EXPECTED_INITIALIZER_STATE_SHA256:
        _fail("structured initializer state SHA-256 mismatch")
    payload = torch.load(path, map_location="cpu", weights_only=False)
    if payload.get("schema") != initializer.MODEL_STATE_SCHEMA:
        _fail("structured initializer state schema mismatch")
    state = payload.get("model_state_dict")
    if not isinstance(state, dict):
        _fail("structured initializer has no model_state_dict")
    return state, observed


def _new_initializer_model(state: dict[str, Any]) -> Any:
    model = distill._model()
    try:
        model.load_state_dict(state, strict=True)
    except (RuntimeError, TypeError) as error:
        _fail(f"structured initializer layout mismatch: {error}")
    parameters = initializer._policy_parameters(model)
    parameter_ids = {id(parameter) for parameter in parameters}
    names = [
        name
        for name, parameter in model.named_parameters()
        if id(parameter) in parameter_ids
    ]
    expected_names = [
        name for name, _ in model.named_parameters() if not name.startswith("value_head.")
    ]
    if names != expected_names:
        _fail("structured trainable parameter selection drifted")
    if any(
        parameter.requires_grad
        for name, parameter in model.named_parameters()
        if name.startswith("value_head.")
    ):
        _fail("structured value head is not frozen")
    return model


def _load_critic(path: Path, confirmation_path: Path) -> tuple[Any, dict[str, Any]]:
    observed_state = _sha256(path)
    if observed_state != EXPECTED_VALUE_STATE_SHA256:
        _fail("complete-history critic state SHA-256 mismatch")
    observed_confirmation = _sha256(confirmation_path)
    if observed_confirmation != EXPECTED_VALUE_CONFIRMATION_SHA256:
        _fail("complete-history critic confirmation SHA-256 mismatch")
    confirmation = json.loads(confirmation_path.read_text(encoding="utf-8"))
    if confirmation.get("schema") != EXPECTED_VALUE_CONFIRMATION_SCHEMA:
        _fail("complete-history critic confirmation schema mismatch")
    gates = confirmation.get("gates", {})
    if gates.get("bounded_value_confirmation_pass") is not True:
        _fail("complete-history critic confirmation gate is not a pass")
    if gates.get("all_predictions_finite_and_bounded") is not True:
        _fail("complete-history critic finite/bounded gate is not a pass")
    source = confirmation.get("source", {})
    if source.get("cache_sha256") != EXPECTED_VALUE_SOURCE_CACHE_SHA256:
        _fail("complete-history critic source cache identity mismatch")
    initializer_identity = source.get("initializer_identity", {})
    if initializer_identity.get("candidate_json_sha256") != EXPECTED_INITIALIZER_JSON_SHA256:
        _fail("complete-history critic initializer identity mismatch")
    model_identity = confirmation.get("model_state", {})
    if model_identity.get("sha256") != EXPECTED_VALUE_STATE_SHA256:
        _fail("complete-history critic confirmation binds a different state")
    payload = torch.load(path, map_location="cpu", weights_only=False)
    if payload.get("schema") != EXPECTED_VALUE_STATE_SCHEMA:
        _fail("complete-history critic state schema mismatch")
    state = payload.get("model_state_dict")
    if not isinstance(state, dict):
        _fail("complete-history critic has no model_state_dict")
    model = distill._model()
    try:
        model.load_state_dict(state, strict=True)
    except (RuntimeError, TypeError) as error:
        _fail(f"complete-history critic layout mismatch: {error}")
    model.eval()
    return model, {
        "state_path": str(path),
        "state_sha256": observed_state,
        "confirmation_path": str(confirmation_path),
        "confirmation_sha256": observed_confirmation,
        "source_cache_sha256": source["cache_sha256"],
    }


def _train_arm(
    model: Any,
    decisions: list[Any],
    *,
    epochs: int,
    batch_size: int,
    seed: int,
) -> list[dict[str, Any]]:
    parameters = initializer._policy_parameters(model)
    parameter_ids = {id(parameter) for parameter in parameters}
    parameter_names = [
        name
        for name, parameter in model.named_parameters()
        if id(parameter) in parameter_ids
    ]
    expected_names = [
        name for name, _ in model.named_parameters() if not name.startswith("value_head.")
    ]
    if parameter_names != expected_names:
        _fail("arm trainable parameter selection mismatch")
    initial_value_digest = _value_head_digest(model)
    optimizer = torch.optim.AdamW(
        parameters, lr=LEARNING_RATE, weight_decay=WEIGHT_DECAY
    )
    episode_mass = sum(float(decision.episode_weight) for decision in decisions)
    if not math.isfinite(episode_mass) or episode_mass <= 0.0:
        _fail("episode mass is invalid")
    weights = {
        _decision_key(decision): float(decision.episode_weight) * len(decisions) / episode_mass
        for decision in decisions
    }
    rng = random.Random(seed)
    history: list[dict[str, Any]] = []
    for epoch in range(epochs):
        order = list(range(len(decisions)))
        rng.shuffle(order)
        loss_total = 0.0
        clip_total = 0.0
        gradient_norm_max = 0.0
        steps = 0
        model.train()
        for start in range(0, len(order), batch_size):
            batch = [decisions[index] for index in order[start : start + batch_size]]
            surrogates: list[torch.Tensor] = []
            masses: list[float] = []
            clipped = 0
            for decision in batch:
                joint = terminal._absolute_joint_log_probability(model, decision)
                if not bool(torch.isfinite(joint)):
                    _fail("non-finite arm joint log probability")
                log_ratio = joint - decision.old_joint_log_probability
                ratio = torch.exp(log_ratio)
                clipped_ratio = torch.clamp(ratio, 1.0 - PPO_CLIP, 1.0 + PPO_CLIP)
                advantage = float(decision.standardized_advantage)
                if not math.isfinite(advantage):
                    _fail("non-finite standardized advantage")
                surrogates.append(
                    torch.minimum(ratio * advantage, clipped_ratio * advantage)
                )
                masses.append(weights[_decision_key(decision)])
                clipped += int(abs(float(log_ratio.detach())) > math.log1p(PPO_CLIP))
            mass_tensor = torch.tensor(masses, dtype=torch.float32)
            loss = -(torch.stack(surrogates) * mass_tensor).sum() / mass_tensor.sum()
            if not bool(torch.isfinite(loss)):
                _fail("non-finite arm loss")
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient_norm = torch.nn.utils.clip_grad_norm_(parameters, GRADIENT_CAP)
            if not torch.isfinite(gradient_norm):
                _fail("non-finite arm gradient")
            optimizer.step()
            loss_total += float(loss.detach())
            clip_total += clipped / len(batch)
            gradient_norm_max = max(gradient_norm_max, float(gradient_norm))
            steps += 1
        history.append(
            {
                "epoch": epoch + 1,
                "mean_minibatch_loss": loss_total / max(steps, 1),
                "mean_minibatch_clip_fraction": clip_total / max(steps, 1),
                "maximum_preclip_gradient_norm": gradient_norm_max,
                "optimizer_steps": steps,
            }
        )
    final_value_digest = _value_head_digest(model)
    if final_value_digest != initial_value_digest:
        _fail("structured value head changed during arm training")
    return history


def _arm_result(
    model: Any,
    decisions: list[Any],
    route: dict[str, Any],
    training_history: list[dict[str, Any]],
    initial_state_digest: str,
    initial_value_digest: str,
) -> dict[str, Any]:
    movement = terminal._movement(model, decisions)
    names = [
        name for name, parameter in model.named_parameters() if not name.startswith("value_head.")
    ]
    value_digest = _value_head_digest(model)
    finite = all(bool(torch.isfinite(tensor).all()) for tensor in model.state_dict().values())
    return {
        "route": route,
        "training_history": training_history,
        "movement": movement,
        "integrity": {
            "all_model_tensors_finite": finite,
            "value_head_frozen": value_digest == initial_value_digest,
            "value_head_digest": value_digest,
            "initial_state_digest": initial_state_digest,
            "final_state_digest": _state_digest(model),
            "trainable_parameter_names": names,
            "trainable_parameter_count": sum(
                parameter.numel()
                for name, parameter in model.named_parameters()
                if name in names
            ),
        },
    }


def run(args: argparse.Namespace) -> dict[str, Any]:
    if args.pair_limit < 2 or args.pair_limit > 2048:
        _fail("pair limit must be between 2 and 2048")
    if args.epochs < 1 or args.batch_size < 1 or args.threads < 1:
        _fail("epochs, batch size, and threads must be positive")
    started = time.perf_counter()
    cache_sha = _sha256(args.cache)
    if cache_sha != EXPECTED_CACHE_SHA256:
        _fail("terminal-rung cache SHA-256 mismatch")
    initializer_state, initializer_sha = _load_initializer_state(args.initializer_state)
    critic, critic_identity = _load_critic(args.value_state, args.value_confirmation)
    loaded = time.perf_counter()
    structured_screen._configure(args.seed, args.threads)
    decisions, source, load_timings = terminal._load_decisions(args.cache, args.pair_limit)
    history_integrity = _validate_history_rows(decisions)
    if source.get("initializer_identity", {}).get("candidate_json_sha256") != EXPECTED_INITIALIZER_JSON_SHA256:
        _fail("selected cache initializer identity mismatch")
    if source.get("base_seed") != 1660001:
        _fail("selected cache base seed mismatch")
    selected_keys = [_decision_key(decision) for decision in decisions]
    alignment_mc: dict[str, Any]
    alignment_gae: dict[str, Any]
    model_mc = _new_initializer_model(initializer_state)
    model_gae = _new_initializer_model(initializer_state)
    initial_mc_digest = _state_digest(model_mc)
    initial_gae_digest = _state_digest(model_gae)
    initial_mc_value_digest = _value_head_digest(model_mc)
    initial_gae_value_digest = _value_head_digest(model_gae)
    if initial_mc_digest != initial_gae_digest:
        _fail("independent arms do not start from identical state")
    alignment_mc = terminal._alignment(model_mc, decisions)
    alignment_gae = terminal._alignment(model_gae, decisions)
    if not alignment_mc.get("pass") or not alignment_gae.get("pass"):
        _fail("initializer behavior alignment failed")
    prepared = time.perf_counter()

    mc_route = route_advantages(decisions, "mc")
    mc_train = _train_arm(
        model_mc,
        decisions,
        epochs=args.epochs,
        batch_size=args.batch_size,
        seed=args.seed,
    )
    mc_result = _arm_result(
        model_mc,
        decisions,
        mc_route,
        mc_train,
        initial_mc_digest,
        initial_mc_value_digest,
    )
    mc_finished = time.perf_counter()

    gae_route = route_advantages(decisions, "gae", critic_model=critic)
    gae_train = _train_arm(
        model_gae,
        decisions,
        epochs=args.epochs,
        batch_size=args.batch_size,
        seed=args.seed,
    )
    gae_result = _arm_result(
        model_gae,
        decisions,
        gae_route,
        gae_train,
        initial_gae_digest,
        initial_gae_value_digest,
    )
    finished = time.perf_counter()
    if selected_keys != [_decision_key(decision) for decision in decisions]:
        _fail("arm decision panel changed")
    if mc_result["integrity"]["trainable_parameter_names"] != gae_result["integrity"]["trainable_parameter_names"]:
        _fail("arm trainable parameter sets differ")
    if mc_result["integrity"]["trainable_parameter_count"] != gae_result["integrity"]["trainable_parameter_count"]:
        _fail("arm trainable parameter counts differ")
    return {
        "schema": SCHEMA,
        "status": "complete",
        "config": {
            "pair_limit": args.pair_limit,
            "epochs": args.epochs,
            "threads": args.threads,
            "seed": args.seed,
            "batch_size_physical_decisions": args.batch_size,
            "learning_rate": LEARNING_RATE,
            "weight_decay": WEIGHT_DECAY,
            "ppo_clip": PPO_CLIP,
            "gradient_norm_cap": GRADIENT_CAP,
            "terminal_reward": "natural-win-draw-loss-only/v1",
            "nonterminal_reward": 0,
            "mc_advantage": "terminal-reward-minus-frozen-parent-value/v1",
            "gae_advantage": "complete-history-critic-terminal-only-gamma1-lambda0p95/v1",
            "history_length": EXPECTED_HISTORY_LENGTH,
        },
        "inputs": {
            "cache": {"path": str(args.cache), "sha256": cache_sha},
            "initializer_state": {"path": str(args.initializer_state), "sha256": initializer_sha},
            "initializer_json_sha256": EXPECTED_INITIALIZER_JSON_SHA256,
            "complete_history_critic": critic_identity,
            "cache_source": source,
        },
        "selected_panel": {
            **history_integrity,
            "decision_key_sha256": hashlib.sha256(
                _json_bytes([list(key) for key in selected_keys])
            ).hexdigest(),
            "same_decisions_for_both_arms": True,
        },
        "models": {
            "architecture": "width48-complete-public-history-structured/v1",
            "independent_model_instances": model_mc is not model_gae,
            "identical_initializer_state_digest": initial_mc_digest == initial_gae_digest,
            "trainable_parameter_names": mc_result["integrity"]["trainable_parameter_names"],
            "trainable_parameter_count": mc_result["integrity"]["trainable_parameter_count"],
            "value_head_preserved": (
                mc_result["integrity"]["value_head_frozen"]
                and gae_result["integrity"]["value_head_frozen"]
            ),
        },
        "integrity": {
            "initializer_alignment_mc": alignment_mc,
            "initializer_alignment_gae": alignment_gae,
            "alignment_limit": ALIGNMENT_LIMIT,
            "cache_history_integrity": history_integrity,
            "same_shuffle_seed": True,
            "same_optimizer_contract": True,
            "same_clip_and_gradient_cap": True,
            "no_strength_gate_executed": True,
        },
        "arms": {
            "monte_carlo": mc_result,
            "gae_lambda_0p95": gae_result,
        },
        "timings": {
            **load_timings,
            "identity_and_model_load_seconds": loaded - started,
            "prepare_seconds": prepared - loaded,
            "mc_arm_seconds": mc_finished - prepared,
            "gae_arm_seconds": finished - mc_finished,
            "total_seconds": finished - started,
        },
        "nonclaims": [
            "bounded development row only",
            "reuses an existing terminal-rung cache and is not fresh strength evidence",
            "no promotion, pro-level, cross-deck, or human-strength claim",
            "MC versus GAE also changes the value baseline from frozen parent to complete-history critic",
            "cross-architecture factorial interpretation requires separately matched native evaluation",
        ],
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=DEFAULT_CACHE)
    parser.add_argument("--initializer-state", type=Path, default=DEFAULT_INITIALIZER_STATE)
    parser.add_argument("--value-state", type=Path, default=DEFAULT_VALUE_STATE)
    parser.add_argument("--value-confirmation", type=Path, default=DEFAULT_VALUE_CONFIRMATION)
    parser.add_argument("--pair-limit", type=int, default=DEFAULT_PAIR_LIMIT)
    parser.add_argument("--epochs", type=int, default=DEFAULT_EPOCHS)
    parser.add_argument("--threads", type=int, default=DEFAULT_THREADS)
    parser.add_argument("--batch-size", type=int, default=DEFAULT_BATCH_SIZE)
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED)
    parser.add_argument("--output", type=Path, required=True)
    return parser


def main() -> int:
    args = _parser().parse_args()
    result = run(args)
    _write_new(args.output, result)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
