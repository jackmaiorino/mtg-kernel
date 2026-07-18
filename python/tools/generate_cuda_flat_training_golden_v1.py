#!/usr/bin/env python3
"""Generate the independent CUDA flat-training small golden.

This oracle intentionally uses only the Python standard library and float64
arithmetic for the model equations. Model parameters and synthetic inputs are
rounded once to the Rust contract's f32 storage format before evaluation; all
forward, detached-loss, backward, and Adam equations then run independently in
Python float64.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import struct
import sys
import tempfile
from typing import Sequence


SCHEMA_VERSION = "cuda-flat-training-independent-golden-v1"
MODEL_CONTRACT_VERSION = "cuda-flat-training-capacity-v1"
OPTIMIZER_CONTRACT_VERSION = "native-adam-epsilon-1e-5-v1"
STATE_DIM = 2_048
ACTION_DIM = 128
HIDDEN = 64
WEIGHT_SEED = 0x4207_C0DE_7150_1009
VALUE_COEFFICIENT = 0.5
LEARNING_RATE = 1.0e-3
ADAM_BETA1 = 0.9
ADAM_BETA2 = 0.999
ADAM_EPSILON = 1.0e-5
MASK64 = (1 << 64) - 1
GENERATOR_RELATIVE_PATH = "python/tools/generate_cuda_flat_training_golden_v1.py"
FIXTURE_RELATIVE_PATH = (
    "mtg-kernel/examples/data/cuda_flat_training_independent_golden_v1.json"
)
ORDER_PROJECTION_SEEDS = [
    0x243F_6A88_85A3_08D3,
    0x1319_8A2E_0370_7344,
    0xA409_3822_299F_31D0,
]
ORDER_FINGERPRINT_DEADBAND = 1.0e-4


def f32(value: float) -> float:
    """Round a Python number to IEEE-754 binary32 and return it as float64."""

    return struct.unpack("<f", struct.pack("<f", value))[0]


def mix64(value: int) -> int:
    value &= MASK64
    value = ((value ^ (value >> 30)) * 0xBF58_476D_1CE4_E5B9) & MASK64
    value = ((value ^ (value >> 27)) * 0x94D0_49BB_1331_11EB) & MASK64
    return (value ^ (value >> 31)) & MASK64


class SplitMix64:
    def __init__(self, state: int) -> None:
        self.state = state & MASK64

    def next(self) -> int:
        self.state = (self.state + 0x9E37_79B9_7F4A_7C15) & MASK64
        return mix64(self.state)

    def signed_unit_f32(self) -> float:
        fraction = f32(float(self.next() >> 40) / 16_777_216.0)
        # The operands make the exact real result representable before the one
        # f32 boundary, matching Rust's mul_add storage contract.
        return f32(fraction * 2.0 - 1.0)


def deterministic_feature(decision: int, logical_action: int | None, index: int) -> float:
    if logical_action is None:
        action = 0xD1B5_4A32_D192_ED03
    else:
        action = ((logical_action + 1) * 0x94D0_49BB_1331_11EB) & MASK64
    mixed = mix64(
        (
            decision * 0x9E37_79B9_7F4A_7C15
            + action
            + index * 0xBF58_476D_1CE4_E5B9
        )
        & MASK64
    )
    fraction = f32(float(mixed >> 40) / 16_777_216.0)
    return f32(fraction * 1.5 - 0.75)


def initialized_matrix(fan_in: int, fan_out: int, rng: SplitMix64) -> list[float]:
    quotient = f32(f32(6.0) / f32(float(fan_in + fan_out)))
    scale = f32(math.sqrt(quotient))
    return [f32(rng.signed_unit_f32() * scale) for _ in range(fan_in * fan_out)]


def initialized_bias(length: int, rng: SplitMix64) -> list[float]:
    scale = f32(0.01)
    return [f32(rng.signed_unit_f32() * scale) for _ in range(length)]


def make_model() -> dict[str, list[float]]:
    rng = SplitMix64(WEIGHT_SEED)
    scorer_combined = None
    model: dict[str, list[float]] = {}
    model["state_w1"] = initialized_matrix(STATE_DIM, HIDDEN, rng)
    model["state_b1"] = initialized_bias(HIDDEN, rng)
    model["state_w2"] = initialized_matrix(HIDDEN, HIDDEN, rng)
    model["state_b2"] = initialized_bias(HIDDEN, rng)
    model["action_w"] = initialized_matrix(ACTION_DIM, HIDDEN, rng)
    model["action_b"] = initialized_bias(HIDDEN, rng)
    scorer_combined = initialized_matrix(HIDDEN * 2, HIDDEN, rng)
    split = HIDDEN * HIDDEN
    model["scorer_state_w"] = scorer_combined[:split]
    model["scorer_action_w"] = scorer_combined[split:]
    model["scorer_b"] = initialized_bias(HIDDEN, rng)
    model["scorer_out_w"] = initialized_matrix(HIDDEN, 1, rng)
    model["value_w1"] = initialized_matrix(HIDDEN, HIDDEN, rng)
    model["value_b1"] = initialized_bias(HIDDEN, rng)
    model["value_out_w"] = initialized_matrix(HIDDEN, 1, rng)
    model["value_out_b"] = initialized_bias(1, rng)
    return model


def make_batch() -> dict[str, list[int] | list[float]]:
    counts = [2, 3, 4]
    selected_local = [0, 2, 1]
    terminal_returns = [1.0, -1.0, 0.0]
    offsets = [0]
    action_owner: list[int] = []
    selected_global: list[int] = []
    states: list[float] = []
    actions: list[float] = []
    total_actions = 0
    for decision, (count, selected) in enumerate(zip(counts, selected_local, strict=True)):
        selected_global.append(total_actions + selected)
        action_owner.extend([decision] * count)
        total_actions += count
        offsets.append(total_actions)
        logical = 100_003 + decision * 97
        states.extend(deterministic_feature(logical, None, feature) for feature in range(STATE_DIM))
        for action in range(count):
            actions.extend(
                deterministic_feature(logical, action, feature) for feature in range(ACTION_DIM)
            )
    return {
        "counts": counts,
        "offsets": offsets,
        "action_owner": action_owner,
        "states": states,
        "actions": actions,
        "selected_global": selected_global,
        "terminal_returns": terminal_returns,
    }


def matmul(
    left: Sequence[float], rows: int, inner: int, right: Sequence[float], columns: int
) -> list[float]:
    output = [0.0] * (rows * columns)
    for row in range(rows):
        left_base = row * inner
        output_base = row * columns
        for shared in range(inner):
            source = left[left_base + shared]
            weight_base = shared * columns
            for column in range(columns):
                output[output_base + column] += source * right[weight_base + column]
    return output


def matmul_tn(
    left: Sequence[float], right: Sequence[float], rows: int, left_columns: int, right_columns: int
) -> list[float]:
    output = [0.0] * (left_columns * right_columns)
    for row in range(rows):
        left_base = row * left_columns
        right_base = row * right_columns
        for left_column in range(left_columns):
            source = left[left_base + left_column]
            output_base = left_column * right_columns
            for right_column in range(right_columns):
                output[output_base + right_column] += source * right[right_base + right_column]
    return output


def matmul_nt(
    left: Sequence[float], right: Sequence[float], rows: int, inner: int, output_columns: int
) -> list[float]:
    output = [0.0] * (rows * output_columns)
    for row in range(rows):
        left_base = row * inner
        for output_column in range(output_columns):
            right_base = output_column * inner
            output[row * output_columns + output_column] = math.fsum(
                left[left_base + shared] * right[right_base + shared] for shared in range(inner)
            )
    return output


def add_bias_relu(values: list[float], bias: Sequence[float], columns: int, relu: bool) -> None:
    for index, value in enumerate(values):
        value += bias[index % columns]
        values[index] = max(value, 0.0) if relu else value


def linear_relu(
    values: Sequence[float], rows: int, inner: int, weight: Sequence[float], bias: Sequence[float], columns: int
) -> list[float]:
    output = matmul(values, rows, inner, weight, columns)
    add_bias_relu(output, bias, columns, True)
    return output


def forward(model: dict[str, list[float]], batch: dict[str, list[int] | list[float]]) -> dict[str, list[float]]:
    states = batch["states"]
    actions = batch["actions"]
    action_owner = batch["action_owner"]
    assert isinstance(states, list) and isinstance(actions, list) and isinstance(action_owner, list)
    decisions = len(batch["terminal_returns"])
    total_actions = len(action_owner)
    state_h1 = linear_relu(states, decisions, STATE_DIM, model["state_w1"], model["state_b1"], HIDDEN)
    state_h2 = linear_relu(state_h1, decisions, HIDDEN, model["state_w2"], model["state_b2"], HIDDEN)
    action_h = linear_relu(actions, total_actions, ACTION_DIM, model["action_w"], model["action_b"], HIDDEN)
    state_for_actions: list[float] = []
    for owner in action_owner:
        state_for_actions.extend(state_h2[owner * HIDDEN : (owner + 1) * HIDDEN])
    scorer_h = matmul(state_for_actions, total_actions, HIDDEN, model["scorer_state_w"], HIDDEN)
    action_term = matmul(action_h, total_actions, HIDDEN, model["scorer_action_w"], HIDDEN)
    scorer_h = [left + right for left, right in zip(scorer_h, action_term, strict=True)]
    add_bias_relu(scorer_h, model["scorer_b"], HIDDEN, True)
    logits = matmul(scorer_h, total_actions, HIDDEN, model["scorer_out_w"], 1)
    value_h = linear_relu(state_h2, decisions, HIDDEN, model["value_w1"], model["value_b1"], HIDDEN)
    values = matmul(value_h, decisions, HIDDEN, model["value_out_w"], 1)
    add_bias_relu(values, model["value_out_b"], 1, False)
    return {
        "state_h1": state_h1,
        "state_h2": state_h2,
        "action_h": action_h,
        "state_for_actions": state_for_actions,
        "scorer_h": scorer_h,
        "logits": logits,
        "value_h": value_h,
        "values": values,
    }


def detached_loss_and_output_gradients(
    activations: dict[str, list[float]], batch: dict[str, list[int] | list[float]]
) -> tuple[dict[str, float], list[float], list[float]]:
    logits = activations["logits"]
    values = activations["values"]
    offsets = batch["offsets"]
    selected_global = batch["selected_global"]
    terminal_returns = batch["terminal_returns"]
    assert isinstance(offsets, list) and isinstance(selected_global, list)
    assert isinstance(terminal_returns, list)
    decisions = len(terminal_returns)
    d_logits = [0.0] * len(logits)
    d_values = [0.0] * decisions
    policy_terms: list[float] = []
    value_terms: list[float] = []
    detached_advantages = [target - value for target, value in zip(terminal_returns, values, strict=True)]
    for decision in range(decisions):
        begin, end = offsets[decision], offsets[decision + 1]
        selected = selected_global[decision]
        maximum = max(logits[begin:end])
        exponentials = [math.exp(logits[action] - maximum) for action in range(begin, end)]
        denominator = math.fsum(exponentials)
        log_denominator = math.log(denominator)
        advantage = detached_advantages[decision]
        for local, action in enumerate(range(begin, end)):
            probability = exponentials[local] / denominator
            d_logits[action] = advantage * (probability - (1.0 if action == selected else 0.0)) / decisions
        selected_log_probability = logits[selected] - maximum - log_denominator
        policy_terms.append(-selected_log_probability * advantage)
        value_error = values[decision] - terminal_returns[decision]
        value_terms.append(value_error * value_error)
        d_values[decision] = 2.0 * VALUE_COEFFICIENT * value_error / decisions
    policy_sum = math.fsum(policy_terms)
    value_sum = math.fsum(value_terms)
    return (
        {
            "policy_sum": policy_sum,
            "value_sum": value_sum,
            "loss": (policy_sum + VALUE_COEFFICIENT * value_sum) / decisions,
        },
        d_logits,
        d_values,
    )


def column_sum(values: Sequence[float], rows: int, columns: int) -> list[float]:
    return [math.fsum(values[row * columns + column] for row in range(rows)) for column in range(columns)]


def backward(
    model: dict[str, list[float]],
    batch: dict[str, list[int] | list[float]],
    activations: dict[str, list[float]],
    d_logits: Sequence[float],
    d_values: Sequence[float],
) -> dict[str, list[float]]:
    action_owner = batch["action_owner"]
    offsets = batch["offsets"]
    states = batch["states"]
    actions = batch["actions"]
    assert isinstance(action_owner, list) and isinstance(offsets, list)
    assert isinstance(states, list) and isinstance(actions, list)
    decisions = len(batch["terminal_returns"])
    total_actions = len(action_owner)
    gradients: dict[str, list[float]] = {}
    gradients["scorer_out_w"] = matmul_tn(activations["scorer_h"], d_logits, total_actions, HIDDEN, 1)
    d_scorer_pre = [0.0] * (total_actions * HIDDEN)
    for action in range(total_actions):
        for hidden in range(HIDDEN):
            index = action * HIDDEN + hidden
            if activations["scorer_h"][index] > 0.0:
                d_scorer_pre[index] = d_logits[action] * model["scorer_out_w"][hidden]
    gradients["scorer_state_w"] = matmul_tn(
        activations["state_for_actions"], d_scorer_pre, total_actions, HIDDEN, HIDDEN
    )
    gradients["scorer_action_w"] = matmul_tn(
        activations["action_h"], d_scorer_pre, total_actions, HIDDEN, HIDDEN
    )
    gradients["scorer_b"] = column_sum(d_scorer_pre, total_actions, HIDDEN)
    d_state_for_actions = matmul_nt(
        d_scorer_pre, model["scorer_state_w"], total_actions, HIDDEN, HIDDEN
    )
    d_action_h = matmul_nt(d_scorer_pre, model["scorer_action_w"], total_actions, HIDDEN, HIDDEN)
    d_action_h = [
        derivative if activation > 0.0 else 0.0
        for derivative, activation in zip(d_action_h, activations["action_h"], strict=True)
    ]
    gradients["action_w"] = matmul_tn(actions, d_action_h, total_actions, ACTION_DIM, HIDDEN)
    gradients["action_b"] = column_sum(d_action_h, total_actions, HIDDEN)
    d_h2_policy = [0.0] * (decisions * HIDDEN)
    for decision in range(decisions):
        for action in range(offsets[decision], offsets[decision + 1]):
            for hidden in range(HIDDEN):
                d_h2_policy[decision * HIDDEN + hidden] += d_state_for_actions[action * HIDDEN + hidden]
    gradients["value_out_w"] = matmul_tn(activations["value_h"], d_values, decisions, HIDDEN, 1)
    gradients["value_out_b"] = column_sum(d_values, decisions, 1)
    d_value_h = [0.0] * (decisions * HIDDEN)
    for decision in range(decisions):
        for hidden in range(HIDDEN):
            index = decision * HIDDEN + hidden
            if activations["value_h"][index] > 0.0:
                d_value_h[index] = d_values[decision] * model["value_out_w"][hidden]
    gradients["value_w1"] = matmul_tn(activations["state_h2"], d_value_h, decisions, HIDDEN, HIDDEN)
    gradients["value_b1"] = column_sum(d_value_h, decisions, HIDDEN)
    d_h2_value = matmul_nt(d_value_h, model["value_w1"], decisions, HIDDEN, HIDDEN)
    d_state2_pre = [
        left + right if activation > 0.0 else 0.0
        for left, right, activation in zip(
            d_h2_policy, d_h2_value, activations["state_h2"], strict=True
        )
    ]
    gradients["state_w2"] = matmul_tn(
        activations["state_h1"], d_state2_pre, decisions, HIDDEN, HIDDEN
    )
    gradients["state_b2"] = column_sum(d_state2_pre, decisions, HIDDEN)
    d_state_h1 = matmul_nt(d_state2_pre, model["state_w2"], decisions, HIDDEN, HIDDEN)
    d_state_h1 = [
        derivative if activation > 0.0 else 0.0
        for derivative, activation in zip(d_state_h1, activations["state_h1"], strict=True)
    ]
    gradients["state_w1"] = matmul_tn(states, d_state_h1, decisions, STATE_DIM, HIDDEN)
    gradients["state_b1"] = column_sum(d_state_h1, decisions, HIDDEN)
    return {name: gradients[name] for name in model}


def adam_step(
    model: dict[str, list[float]], gradients: dict[str, list[float]]
) -> tuple[dict[str, list[float]], dict[str, list[float]], dict[str, list[float]]]:
    updated: dict[str, list[float]] = {}
    first_moments: dict[str, list[float]] = {}
    second_moments: dict[str, list[float]] = {}
    for name, values in model.items():
        gradient = gradients[name]
        first = [(1.0 - ADAM_BETA1) * value for value in gradient]
        second = [(1.0 - ADAM_BETA2) * value * value for value in gradient]
        corrected_first = [value / (1.0 - ADAM_BETA1) for value in first]
        corrected_second = [value / (1.0 - ADAM_BETA2) for value in second]
        next_values = [
            value - LEARNING_RATE * first_hat / (math.sqrt(second_hat) + ADAM_EPSILON)
            for value, first_hat, second_hat in zip(
                values, corrected_first, corrected_second, strict=True
            )
        ]
        updated[name] = next_values
        first_moments[name] = first
        second_moments[name] = second
    return updated, first_moments, second_moments


def vector_summary(values: Sequence[float]) -> dict[str, float | int]:
    if not values or not all(math.isfinite(value) for value in values):
        raise ValueError("summary input must be non-empty and finite")
    length = len(values)
    return {
        "length": length,
        "mean": math.fsum(values) / length,
        "mean_abs": math.fsum(abs(value) for value in values) / length,
        "rms": math.sqrt(math.fsum(value * value for value in values) / length),
        "minimum": min(values),
        "maximum": max(values),
    }


def order_weight(index: int, seed: int) -> float:
    mixed = mix64((((index + 1) * 0x9E37_79B9_7F4A_7C15) & MASK64) ^ seed)
    fraction = float(mixed >> 11) / float(1 << 53)
    return fraction * 2.0 - 1.0


def order_evidence(values: Sequence[float]) -> dict[str, object]:
    projections = []
    for seed in ORDER_PROJECTION_SEEDS:
        projection = 0.0
        for index, value in enumerate(values):
            projection += value * order_weight(index, seed)
        projections.append({"seed": seed, "value": projection})
    quantized = bytes(
        0
        if abs(value) <= ORDER_FINGERPRINT_DEADBAND
        else (1 if value < 0.0 else 2)
        for value in values
    )
    return {
        "projection_contract": (
            "sum(value[i] * signed_unit(mix64((i+1)*golden_ratio xor seed))) in index order"
        ),
        "projections": projections,
        "quantized_fingerprint_contract": "sha256(sign with abs(value)<=1e-4 encoded as zero)",
        "quantized_fingerprint_sha256": hashlib.sha256(quantized).hexdigest(),
    }


def representative_indices(values: Sequence[float]) -> list[int]:
    candidates = [0, len(values) // 2, len(values) - 1]
    candidates.append(max(range(len(values)), key=lambda index: (abs(values[index]), -index)))
    return list(dict.fromkeys(candidates))


def canonical_bytes(value: object) -> bytes:
    return (json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + "\n").encode("utf-8")


def generate_fixture(generator_path: Path | None = None) -> dict[str, object]:
    if generator_path is None:
        generator_path = Path(__file__).resolve()
    python_implementation = platform.python_implementation()
    if python_implementation != "CPython":
        raise RuntimeError(
            "golden generation requires CPython, "
            f"found {python_implementation}"
        )
    configured_python_version = (
        generator_path.parents[2] / ".python-version"
    ).read_text(encoding="utf-8").strip()
    actual_python_version = platform.python_version()
    if actual_python_version != configured_python_version:
        raise RuntimeError(
            "golden generation requires the repository-pinned Python "
            f"{configured_python_version}, found {actual_python_version}"
        )
    model = make_model()
    batch = make_batch()
    activations = forward(model, batch)
    loss, d_logits, d_values = detached_loss_and_output_gradients(activations, batch)
    gradients = backward(model, batch, activations, d_logits, d_values)
    updated, first_moments, second_moments = adam_step(model, gradients)
    tensor_records = []
    for name, values in model.items():
        representatives = []
        for index in representative_indices(gradients[name]):
            representatives.append(
                {
                    "index": index,
                    "initial_value": values[index],
                    "gradient": gradients[name][index],
                    "first_moment": first_moments[name][index],
                    "second_moment": second_moments[name][index],
                    "updated_value": updated[name][index],
                }
            )
        tensor_records.append(
            {
                "name": name,
                "length": len(values),
                "gradient_summary": vector_summary(gradients[name]),
                "gradient_order_evidence": order_evidence(gradients[name]),
                "updated_value_summary": vector_summary(updated[name]),
                "updated_value_order_evidence": order_evidence(updated[name]),
                "first_moment_summary": vector_summary(first_moments[name]),
                "first_moment_order_evidence": order_evidence(first_moments[name]),
                "second_moment_summary": vector_summary(second_moments[name]),
                "second_moment_order_evidence": order_evidence(second_moments[name]),
                "representatives": representatives,
            }
        )
    generator_sha256 = hashlib.sha256(generator_path.read_bytes()).hexdigest()
    payload: dict[str, object] = {
        "schema_version": SCHEMA_VERSION,
        "contract": {
            "model_contract_version": MODEL_CONTRACT_VERSION,
            "optimizer_contract_version": OPTIMIZER_CONTRACT_VERSION,
            "state_dim": STATE_DIM,
            "action_dim": ACTION_DIM,
            "hidden_dim": HIDDEN,
            "weight_seed": WEIGHT_SEED,
            "value_coefficient": VALUE_COEFFICIENT,
            "learning_rate": LEARNING_RATE,
            "beta1": ADAM_BETA1,
            "beta2": ADAM_BETA2,
            "epsilon": ADAM_EPSILON,
            "adam_step": 1,
            "parameter_count": sum(len(values) for values in model.values()),
            "advantage_detached": True,
        },
        "batch": {
            "counts": batch["counts"],
            "offsets": batch["offsets"],
            "action_owner": batch["action_owner"],
            "selected_global": batch["selected_global"],
            "terminal_returns": batch["terminal_returns"],
        },
        "expected": {
            "forward": {"logits": activations["logits"], "values": activations["values"]},
            "detached_loss": loss,
            "output_gradients": {"d_logits": d_logits, "d_values": d_values},
            "tensors": tensor_records,
        },
        "provenance": {
            "generator_relative_path": GENERATOR_RELATIVE_PATH,
            "fixture_relative_path": FIXTURE_RELATIVE_PATH,
            "generator_sha256": generator_sha256,
            "generator_language": "Python 3 standard library only",
            "python_implementation": python_implementation,
            "python_version": actual_python_version,
            "configured_python_version_file": ".python-version",
            "third_party_dependencies": [],
            "generation_command": (
                "python python/tools/generate_cuda_flat_training_golden_v1.py"
            ),
            "check_command": (
                "python python/tools/generate_cuda_flat_training_golden_v1.py --check"
            ),
            "arithmetic": (
                "contract inputs and parameters rounded to binary32 storage; "
                "forward, detached loss, backward, and Adam evaluated in Python float64"
            ),
            "claim_scope": "small synthetic correctness oracle only; no production or performance claim",
        },
    }
    payload_sha256 = hashlib.sha256(canonical_bytes(payload)).hexdigest()
    fixture = copy.deepcopy(payload)
    fixture["integrity"] = {
        "algorithm": "sha256",
        "canonicalization": "UTF-8 JSON, sorted keys, indent=2, trailing LF, integrity field omitted",
        "payload_sha256": payload_sha256,
    }
    return fixture


def validate_fixture_integrity(fixture: dict[str, object]) -> bool:
    candidate = copy.deepcopy(fixture)
    integrity = candidate.pop("integrity", None)
    if not isinstance(integrity, dict) or integrity.get("algorithm") != "sha256":
        return False
    expected = integrity.get("payload_sha256")
    if not isinstance(expected, str):
        return False
    return hashlib.sha256(canonical_bytes(candidate)).hexdigest() == expected


def fixture_matches(path: Path, generator_path: Path | None = None) -> bool:
    try:
        actual = path.read_bytes()
    except OSError:
        return False
    return actual == canonical_bytes(generate_fixture(generator_path))


def default_fixture_path() -> Path:
    return Path(__file__).resolve().parents[2] / FIXTURE_RELATIVE_PATH


def write_atomic(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary_name, path)
    except BaseException:
        try:
            os.unlink(temporary_name)
        except FileNotFoundError:
            pass
        raise


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if the checked fixture drifts")
    parser.add_argument("--output", type=Path, default=default_fixture_path())
    arguments = parser.parse_args(argv)
    try:
        expected = canonical_bytes(generate_fixture(Path(__file__).resolve()))
    except (OSError, RuntimeError, ValueError) as error:
        print(f"fixture generation failed: {error}", file=sys.stderr)
        return 2
    if arguments.check:
        if arguments.output.read_bytes() != expected:
            print(f"fixture drift: {arguments.output}", file=sys.stderr)
            return 1
        fixture = json.loads(expected)
        if not validate_fixture_integrity(fixture):
            print("generated fixture integrity check failed", file=sys.stderr)
            return 1
        print(f"fixture check passed: {arguments.output}")
        return 0
    write_atomic(arguments.output, expected)
    print(f"wrote {arguments.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
