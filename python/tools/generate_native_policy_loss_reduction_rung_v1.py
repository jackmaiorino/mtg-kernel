"""Generate the intermediate-scale Torch/Rust scalar-reduction rung.

The frozen 32-group third train-step program is cycled 32 times at the model
state immediately after Adam step two.  Torch remains authoritative for
``torch.stack(...).sum()`` while the artifact also records a straightforward
binary32 sequential reduction over the exact same term bits.  The latter is
the reduction order used by Rust's production train step.

The artifact deliberately stores only one 32-group cycle of term bits plus a
framed digest of the expanded 1,024-group stream.  It does not duplicate model
inputs or alter the frozen train-step artifact.  ``--check`` is portable and
checks all source pins and independently reconstructable arithmetic;
``--authority-check`` regenerates the Torch result on the exact authority
tuple and requires byte identity.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import sys
from typing import Any

import torch


ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / "python" / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))
if str(ROOT / "python") not in sys.path:
    sys.path.insert(0, str(ROOT / "python"))

# Importing the frozen authority generator establishes its declared single-
# thread deterministic Torch topology before any model work occurs.
import generate_native_policy_train_step_v1_goldens as train_fixture  # noqa: E402
import generate_native_policy_value_net_v1_goldens as forward_fixture  # noqa: E402
from mtg_kernel_rl.model import (  # noqa: E402
    INITIALIZER_RUNNER_FIXED_V1,
    KernelPolicyValueNet,
    ModelConfig,
)
from mtg_kernel_rl.trainer import _compute_loss_tensors  # noqa: E402


SCHEMA = "native-policy-loss-reduction-intermediate-rung-v1"
IDENTITY = "torch-stack-vs-rust-sequential-loss-reduction-v1"
OUTPUT = (
    ROOT
    / "data"
    / "native_policy_train_step_v1"
    / "loss_reduction_intermediate_rung_v1.json"
)
GENERATOR = Path(__file__).resolve()
TRAIN_FIXTURE_GENERATOR = Path(train_fixture.__file__).resolve()
FORWARD_FIXTURE_GENERATOR = Path(forward_fixture.__file__).resolve()
BASE_ARTIFACT = train_fixture.OUTPUT
EXPECTED_BASE_ARTIFACT_SHA256 = (
    "7672c87912b6015f393d66921a3e78cb5623dd76582a9513f2d87c560c0f4aa7"
)
EXPECTED_BASE_SCHEMA = train_fixture.SCHEMA
EXPECTED_BASE_IDENTITY = train_fixture.TRAIN_STEP_IDENTITY

BASE_STEP_INDEX_ZERO_BASED = 2
ADAM_STEP_BEFORE = 2
BASE_GROUP_COUNT = 32
BASE_SUBSTEP_COUNT = 40
CYCLE_COUNT = 32
GROUP_COUNT = BASE_GROUP_COUNT * CYCLE_COUNT
SUBSTEP_COUNT = BASE_SUBSTEP_COUNT * CYCLE_COUNT
VALUE_COEFFICIENT = train_fixture.VALUE_COEFFICIENT
LOSS_ABSOLUTE_TOLERANCE = 5.0e-5
LOSS_RELATIVE_TOLERANCE = 5.0e-5
TERM_STREAM_FRAMING = (
    "for group_index in 0..1024: "
    "u32_le(group_index)||u32_le(policy_term_f32_bits)||"
    "u32_le(value_term_f32_bits)"
)


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


def _f32_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]


def _bits_hex(value: float) -> str:
    return f"0x{_f32_bits(value):08x}"


def _from_bits_hex(value: str) -> float:
    if not isinstance(value, str) or len(value) != 10 or not value.startswith("0x"):
        raise RuntimeError(f"invalid f32 bits: {value!r}")
    bits = int(value[2:], 16)
    return struct.unpack("<f", struct.pack("<I", bits))[0]


def _scalar(value: float) -> dict[str, Any]:
    value = _f32(value)
    if not math.isfinite(value):
        raise RuntimeError("scalar reduction result is non-finite")
    return {"value": value, "f32_bits": _bits_hex(value)}


def _f32_add(left: float, right: float) -> float:
    return _f32(float(left) + float(right))


def _f32_mul(left: float, right: float) -> float:
    return _f32(float(left) * float(right))


def _f32_div(left: float, right: float) -> float:
    return _f32(float(left) / float(right))


def _sequential_reduction(
    policy_terms: list[float], value_terms: list[float]
) -> tuple[float, float, float]:
    if len(policy_terms) != GROUP_COUNT or len(value_terms) != GROUP_COUNT:
        raise RuntimeError("sequential reduction received the wrong group count")
    policy_sum = 0.0
    value_sum = 0.0
    for policy_term, value_term in zip(policy_terms, value_terms, strict=True):
        policy_sum = _f32_add(policy_sum, policy_term)
        value_sum = _f32_add(value_sum, value_term)
    weighted_value = _f32_mul(VALUE_COEFFICIENT, value_sum)
    loss = _f32_div(_f32_add(policy_sum, weighted_value), float(GROUP_COUNT))
    return policy_sum, value_sum, loss


def _tolerance_record(expected: float, actual: float) -> dict[str, Any]:
    delta = abs(float(actual) - float(expected))
    allowed = LOSS_ABSOLUTE_TOLERANCE + LOSS_RELATIVE_TOLERANCE * abs(float(expected))
    return {
        "absolute_delta_f64": delta,
        "allowed_delta_f64": allowed,
        "holds": delta <= allowed,
    }


def _load_base_artifact() -> dict[str, Any]:
    actual_sha = _sha256(BASE_ARTIFACT)
    if actual_sha != EXPECTED_BASE_ARTIFACT_SHA256:
        raise RuntimeError(
            "base train-step artifact drift: "
            f"expected={EXPECTED_BASE_ARTIFACT_SHA256} actual={actual_sha}"
        )
    base = json.loads(BASE_ARTIFACT.read_bytes())
    if base.get("schema") != EXPECTED_BASE_SCHEMA:
        raise RuntimeError("base train-step artifact schema drift")
    if base.get("identity") != EXPECTED_BASE_IDENTITY:
        raise RuntimeError("base train-step artifact identity drift")
    steps = base.get("steps")
    if not isinstance(steps, list) or len(steps) != 3:
        raise RuntimeError("base train-step artifact must contain exactly three steps")
    base_step = steps[BASE_STEP_INDEX_ZERO_BASED]
    groups = base_step.get("groups")
    if not isinstance(groups, list) or len(groups) != BASE_GROUP_COUNT:
        raise RuntimeError("base authority step must contain exactly 32 groups")
    substeps = sum(len(group.get("substeps", [])) for group in groups)
    if substeps != BASE_SUBSTEP_COUNT:
        raise RuntimeError("base authority step must contain exactly 40 substeps")
    if base_step.get("step") != ADAM_STEP_BEFORE + 1:
        raise RuntimeError("base authority step does not follow Adam step two")
    return base


def _program_groups(step: dict[str, Any]) -> list[dict[str, Any]]:
    groups = []
    for group in step["groups"]:
        terminal_return = group.get("terminal_return")
        if terminal_return not in (-1, 0, 1):
            raise RuntimeError("base program has an invalid terminal return")
        substeps = []
        for substep in group.get("substeps", []):
            case = substep.get("case")
            selected = substep.get("selected_action_index")
            if not isinstance(case, str) or type(selected) is not int or selected < 0:
                raise RuntimeError("base program has an invalid substep")
            substeps.append((case, selected))
        if not substeps:
            raise RuntimeError("base program has an empty physical decision")
        groups.append({"terminal_return": terminal_return, "substeps": substeps})
    return groups


def _model_terms(
    model: KernelPolicyValueNet,
    config: ModelConfig,
    raw_cases: dict[str, dict[str, Any]],
    groups: list[dict[str, Any]],
) -> list[tuple[torch.Tensor, torch.Tensor, int]]:
    terms = []
    for group in groups:
        joint_log_probability: torch.Tensor | None = None
        first_value: torch.Tensor | None = None
        for case_name, selected_index in group["substeps"]:
            encoded = forward_fixture._encoded(raw_cases[case_name], config)
            logits, value = model(encoded)
            if selected_index >= logits.numel():
                raise RuntimeError("selected action is out of range")
            selected_log_probability = torch.log_softmax(logits, dim=0)[selected_index]
            joint_log_probability = (
                selected_log_probability
                if joint_log_probability is None
                else joint_log_probability + selected_log_probability
            )
            if first_value is None:
                first_value = value
        if joint_log_probability is None or first_value is None:
            raise RuntimeError("physical decision unexpectedly has no model term")
        terms.append(
            (joint_log_probability, first_value, int(group["terminal_return"]))
        )
    return terms


def _advance_authority_step(
    model: KernelPolicyValueNet,
    optimizer: torch.optim.Adam,
    config: ModelConfig,
    raw_cases: dict[str, dict[str, Any]],
    groups: list[dict[str, Any]],
) -> None:
    scorer_bias = dict(model.named_parameters())[train_fixture.CANONICAL_GAUGE_PARAMETERS[0]]
    scorer_bias_before = scorer_bias.detach().clone()
    terms = _model_terms(model, config, raw_cases, groups)
    _policy_sum, _value_sum, loss = _compute_loss_tensors(terms, VALUE_COEFFICIENT)
    optimizer.zero_grad(set_to_none=True)
    loss.backward()
    if scorer_bias.grad is None:
        raise RuntimeError("canonical scorer-bias gradient is missing")
    scorer_bias.grad.zero_()
    optimizer.step()
    scorer_state = optimizer.state[scorer_bias]
    with torch.no_grad():
        scorer_bias.copy_(scorer_bias_before)
        scorer_state["exp_avg"].zero_()
        scorer_state["exp_avg_sq"].zero_()


def _term_values(
    terms: list[tuple[torch.Tensor, torch.Tensor, int]],
) -> tuple[list[torch.Tensor], list[torch.Tensor]]:
    policy_terms = []
    value_terms = []
    for log_probability, value, terminal_return in terms:
        target = torch.tensor(float(terminal_return), dtype=value.dtype)
        advantage = target - value.detach()
        policy_terms.append(-log_probability * advantage)
        value_terms.append((value - target) ** 2)
    return policy_terms, value_terms


def _stream_sha256(base_cycle_terms: list[dict[str, Any]]) -> str:
    if len(base_cycle_terms) != BASE_GROUP_COUNT:
        raise RuntimeError("term stream must contain one exact 32-group cycle")
    digest = hashlib.sha256()
    for group_index in range(GROUP_COUNT):
        term = base_cycle_terms[group_index % BASE_GROUP_COUNT]
        if term.get("base_group_index") != group_index % BASE_GROUP_COUNT:
            raise RuntimeError("base-cycle term indices are not contiguous")
        digest.update(struct.pack("<I", group_index))
        digest.update(struct.pack("<I", _f32_bits(_from_bits_hex(term["policy_term_f32_bits"]))))
        digest.update(struct.pack("<I", _f32_bits(_from_bits_hex(term["value_term_f32_bits"]))))
    return digest.hexdigest()


def _payload() -> dict[str, Any]:
    authority_hashes = train_fixture._validate_authorities()
    base = _load_base_artifact()
    config = ModelConfig()
    model = KernelPolicyValueNet(config, initializer=INITIALIZER_RUNNER_FIXED_V1)
    optimizer = torch.optim.Adam(
        model.parameters(),
        lr=train_fixture.LEARNING_RATE,
        betas=(0.9, 0.999),
        eps=1.0e-8,
        weight_decay=0.0,
        amsgrad=False,
        foreach=False,
        maximize=False,
        capturable=False,
        differentiable=False,
        fused=False,
    )
    raw_cases = {case["name"]: case for case in forward_fixture._case_inputs()}
    for step in base["steps"][:ADAM_STEP_BEFORE]:
        _advance_authority_step(
            model,
            optimizer,
            config,
            raw_cases,
            _program_groups(step),
        )

    state_parameters = train_fixture._ordered_tensor_state(
        model.named_parameters(), nonzero_witness_floor=0.0
    )["sha256"]
    state_first = train_fixture._optimizer_state(optimizer, model, "exp_avg")["sha256"]
    state_second = train_fixture._optimizer_state(optimizer, model, "exp_avg_sq")["sha256"]
    expected_state = base["steps"][ADAM_STEP_BEFORE - 1]
    expected_hashes = (
        expected_state["parameters_after_adam"]["sha256"],
        expected_state["first_moments_after_adam"]["sha256"],
        expected_state["second_moments_after_adam"]["sha256"],
    )
    if (state_parameters, state_first, state_second) != expected_hashes:
        raise RuntimeError("reconstructed Adam-step-two authority state drifted")

    base_groups = _program_groups(base["steps"][BASE_STEP_INDEX_ZERO_BASED])
    with torch.no_grad():
        base_terms = _model_terms(model, config, raw_cases, base_groups)
        base_policy_terms, base_value_terms = _term_values(base_terms)
        policy_terms = base_policy_terms * CYCLE_COUNT
        value_terms = base_value_terms * CYCLE_COUNT
        expanded_terms = base_terms * CYCLE_COUNT
        policy_sum, value_sum, loss = _compute_loss_tensors(
            expanded_terms, VALUE_COEFFICIENT
        )
        direct_policy_sum = torch.stack(policy_terms).sum()
        direct_value_sum = torch.stack(value_terms).sum()
        if policy_sum.detach().view(torch.int32).item() != direct_policy_sum.view(torch.int32).item():
            raise RuntimeError("trainer policy stack reduction changed")
        if value_sum.detach().view(torch.int32).item() != direct_value_sum.view(torch.int32).item():
            raise RuntimeError("trainer value stack reduction changed")

    policy_values = [float(term.item()) for term in policy_terms]
    value_values = [float(term.item()) for term in value_terms]
    sequential = _sequential_reduction(policy_values, value_values)
    torch_stack = (
        float(policy_sum.item()),
        float(value_sum.item()),
        float(loss.item()),
    )
    comparisons = {
        name: _tolerance_record(expected, actual)
        for name, expected, actual in zip(
            ("policy_sum", "value_sum", "loss"),
            torch_stack,
            sequential,
            strict=True,
        )
    }
    if not all(record["holds"] for record in comparisons.values()):
        raise RuntimeError("same-term sequential reduction exceeds the frozen tolerance")

    base_cycle_terms = []
    for group_index, (policy_term, value_term) in enumerate(
        zip(base_policy_terms, base_value_terms, strict=True)
    ):
        base_cycle_terms.append(
            {
                "base_group_index": group_index,
                "policy_term_f32_bits": _bits_hex(float(policy_term.item())),
                "value_term_f32_bits": _bits_hex(float(value_term.item())),
            }
        )
    term_stream_sha256 = _stream_sha256(base_cycle_terms)
    terminal_counts = {
        str(value): sum(group["terminal_return"] == value for group in base_groups)
        * CYCLE_COUNT
        for value in (-1, 0, 1)
    }
    return {
        "schema": SCHEMA,
        "identity": IDENTITY,
        "authority": {
            "generator_path": GENERATOR.relative_to(ROOT).as_posix(),
            "generator_sha256": _sha256(GENERATOR),
            "train_fixture_generator_path": TRAIN_FIXTURE_GENERATOR.relative_to(ROOT).as_posix(),
            "train_fixture_generator_sha256": _sha256(TRAIN_FIXTURE_GENERATOR),
            "forward_fixture_generator_path": FORWARD_FIXTURE_GENERATOR.relative_to(ROOT).as_posix(),
            "forward_fixture_generator_sha256": _sha256(FORWARD_FIXTURE_GENERATOR),
            "base_artifact_path": BASE_ARTIFACT.relative_to(ROOT).as_posix(),
            "base_artifact_sha256": EXPECTED_BASE_ARTIFACT_SHA256,
            "model_path": train_fixture.MODEL_AUTHORITY.relative_to(ROOT).as_posix(),
            "model_sha256": authority_hashes["model_sha256"],
            "trainer_path": train_fixture.TRAINER_AUTHORITY.relative_to(ROOT).as_posix(),
            "trainer_sha256": authority_hashes["trainer_sha256"],
            "forward_fixture_path": train_fixture.FORWARD_FIXTURE.relative_to(ROOT).as_posix(),
            "forward_fixture_sha256": authority_hashes["forward_fixture_sha256"],
            "platform_system": train_fixture.AUTHORITY_PLATFORM_SYSTEM,
            "platform_machine": train_fixture.AUTHORITY_PLATFORM_MACHINE,
            "python_version": train_fixture.AUTHORITY_PYTHON_VERSION,
            "torch_version": train_fixture.AUTHORITY_TORCH_VERSION,
            "torch_num_threads": train_fixture.TORCH_NUM_THREADS,
            "torch_num_interop_threads": train_fixture.TORCH_NUM_INTEROP_THREADS,
            "torch_deterministic_algorithms": True,
            "torch_default_dtype": "torch.float32",
        },
        "provenance": {
            "base_program_json_pointer": "/steps/2/groups",
            "cycle_rule": "rung_group[i] = base_group[i % 32] for i in 0..1024",
            "base_group_count": BASE_GROUP_COUNT,
            "base_substep_count": BASE_SUBSTEP_COUNT,
            "cycle_count": CYCLE_COUNT,
            "learner_physical_decision_group_count": GROUP_COUNT,
            "policy_substep_count": SUBSTEP_COUNT,
            "terminal_return_counts": terminal_counts,
        },
        "model_state": {
            "trainer_algorithm": "terminal_reinforce_value/v3",
            "initializer": INITIALIZER_RUNNER_FIXED_V1,
            "adam_step_before": ADAM_STEP_BEFORE,
            "reconstruction": (
                "initialize runner-fixed-v1, then execute base artifact steps 1 and 2 "
                "with canonical scorer.2.bias gauge before evaluating the cycled step-3 program"
            ),
            "parameters_sha256": state_parameters,
            "first_moments_sha256": state_first,
            "second_moments_sha256": state_second,
        },
        "term_stream": {
            "framing": TERM_STREAM_FRAMING,
            "sha256": term_stream_sha256,
            "base_cycle_terms": base_cycle_terms,
            "policy_nonzero_count": sum(_f32_bits(value) & 0x7FFFFFFF != 0 for value in policy_values),
            "value_nonzero_count": sum(_f32_bits(value) & 0x7FFFFFFF != 0 for value in value_values),
            "policy_positive_count": sum(value > 0.0 for value in policy_values),
            "policy_negative_count": sum(value < 0.0 for value in policy_values),
            "value_positive_count": sum(value > 0.0 for value in value_values),
        },
        "reduction": {
            "torch_operation": (
                "policy_sum=torch.stack(policy_terms).sum(); "
                "value_sum=torch.stack(value_terms).sum(); "
                "loss=(policy_sum+0.5*value_sum)/1024"
            ),
            "rust_operation": (
                "ordered f32 policy_sum += policy_term and value_sum += value_term, "
                "then (policy_sum + 0.5f32 * value_sum) / 1024f32"
            ),
            "torch_stack": {
                "policy_sum": _scalar(torch_stack[0]),
                "value_sum": _scalar(torch_stack[1]),
                "loss": _scalar(torch_stack[2]),
            },
            "sequential_f32_over_same_torch_term_bits": {
                "policy_sum": _scalar(sequential[0]),
                "value_sum": _scalar(sequential[1]),
                "loss": _scalar(sequential[2]),
            },
            "frozen_tolerance": {
                "absolute": LOSS_ABSOLUTE_TOLERANCE,
                "relative": LOSS_RELATIVE_TOLERANCE,
                "comparison_rule": "abs(actual-expected) <= absolute + relative*abs(expected)",
            },
            "same_term_sequential_vs_torch_stack": comparisons,
            "all_same_term_comparisons_hold": all(
                record["holds"] for record in comparisons.values()
            ),
        },
    }


def _encoded_payload(payload: dict[str, Any]) -> bytes:
    return (json.dumps(payload, sort_keys=True, indent=2) + "\n").encode("utf-8")


def _portable_check() -> None:
    base = _load_base_artifact()
    authority_hashes = train_fixture._validate_authorities()
    if not OUTPUT.is_file():
        raise RuntimeError(f"loss-reduction rung is missing: {OUTPUT}")
    checked = json.loads(OUTPUT.read_bytes())
    if checked.get("schema") != SCHEMA or checked.get("identity") != IDENTITY:
        raise RuntimeError("loss-reduction rung schema or identity drift")
    authority = checked.get("authority", {})
    expected_pins = {
        "generator_path": GENERATOR.relative_to(ROOT).as_posix(),
        "generator_sha256": _sha256(GENERATOR),
        "train_fixture_generator_path": TRAIN_FIXTURE_GENERATOR.relative_to(ROOT).as_posix(),
        "train_fixture_generator_sha256": _sha256(TRAIN_FIXTURE_GENERATOR),
        "forward_fixture_generator_path": FORWARD_FIXTURE_GENERATOR.relative_to(ROOT).as_posix(),
        "forward_fixture_generator_sha256": _sha256(FORWARD_FIXTURE_GENERATOR),
        "base_artifact_path": BASE_ARTIFACT.relative_to(ROOT).as_posix(),
        "base_artifact_sha256": EXPECTED_BASE_ARTIFACT_SHA256,
        "model_path": train_fixture.MODEL_AUTHORITY.relative_to(ROOT).as_posix(),
        "model_sha256": authority_hashes["model_sha256"],
        "trainer_path": train_fixture.TRAINER_AUTHORITY.relative_to(ROOT).as_posix(),
        "trainer_sha256": authority_hashes["trainer_sha256"],
        "forward_fixture_path": train_fixture.FORWARD_FIXTURE.relative_to(ROOT).as_posix(),
        "forward_fixture_sha256": authority_hashes["forward_fixture_sha256"],
    }
    for key, expected in expected_pins.items():
        if authority.get(key) != expected:
            raise RuntimeError(f"loss-reduction authority pin drift: {key}")
    provenance = checked.get("provenance", {})
    expected_counts = {
        "base_group_count": BASE_GROUP_COUNT,
        "base_substep_count": BASE_SUBSTEP_COUNT,
        "cycle_count": CYCLE_COUNT,
        "learner_physical_decision_group_count": GROUP_COUNT,
        "policy_substep_count": SUBSTEP_COUNT,
    }
    for key, expected in expected_counts.items():
        if provenance.get(key) != expected:
            raise RuntimeError(f"loss-reduction provenance drift: {key}")
    if len(base["steps"][BASE_STEP_INDEX_ZERO_BASED]["groups"]) != BASE_GROUP_COUNT:
        raise RuntimeError("base program group count changed during portable check")
    term_stream = checked.get("term_stream", {})
    base_cycle_terms = term_stream.get("base_cycle_terms")
    if not isinstance(base_cycle_terms, list):
        raise RuntimeError("loss-reduction base-cycle terms are missing")
    if term_stream.get("framing") != TERM_STREAM_FRAMING:
        raise RuntimeError("loss-reduction term-stream framing drift")
    if term_stream.get("sha256") != _stream_sha256(base_cycle_terms):
        raise RuntimeError("loss-reduction term-stream digest drift")
    policy_terms = []
    value_terms = []
    for group_index in range(GROUP_COUNT):
        term = base_cycle_terms[group_index % BASE_GROUP_COUNT]
        policy_terms.append(_from_bits_hex(term["policy_term_f32_bits"]))
        value_terms.append(_from_bits_hex(term["value_term_f32_bits"]))
    sequential = _sequential_reduction(policy_terms, value_terms)
    reduction = checked.get("reduction", {})
    recorded_sequential = reduction.get("sequential_f32_over_same_torch_term_bits", {})
    for name, value in zip(("policy_sum", "value_sum", "loss"), sequential, strict=True):
        if recorded_sequential.get(name, {}).get("f32_bits") != _bits_hex(value):
            raise RuntimeError(f"loss-reduction sequential result drift: {name}")
    tolerance = reduction.get("frozen_tolerance", {})
    if tolerance.get("absolute") != LOSS_ABSOLUTE_TOLERANCE or tolerance.get("relative") != LOSS_RELATIVE_TOLERANCE:
        raise RuntimeError("loss-reduction frozen tolerance drift")
    torch_stack = reduction.get("torch_stack", {})
    comparisons = reduction.get("same_term_sequential_vs_torch_stack", {})
    for name, actual in zip(("policy_sum", "value_sum", "loss"), sequential, strict=True):
        expected = _from_bits_hex(torch_stack[name]["f32_bits"])
        if comparisons.get(name) != _tolerance_record(expected, actual):
            raise RuntimeError(f"loss-reduction comparison drift: {name}")
    if reduction.get("all_same_term_comparisons_hold") is not True:
        raise RuntimeError("loss-reduction rung does not hold the frozen tolerance")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--check",
        action="store_true",
        help="check source pins and independently reconstructable artifact arithmetic",
    )
    mode.add_argument(
        "--authority-check",
        action="store_true",
        help="on the exact Torch authority tuple, regenerate and require byte identity",
    )
    args = parser.parse_args()
    if args.check:
        _portable_check()
        print(f"PASS portable {OUTPUT.relative_to(ROOT)}")
        return 0
    train_fixture._assert_exact_authority_environment()
    expected = _encoded_payload(_payload())
    if args.authority_check:
        actual = OUTPUT.read_bytes() if OUTPUT.exists() else b""
        if actual != expected:
            raise SystemExit(f"stale loss-reduction authority rung: {OUTPUT}")
        print(f"PASS authority {OUTPUT.relative_to(ROOT)}")
        return 0
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_bytes(expected)
    print(f"wrote {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
