"""Generate Torch-authoritative grouped loss/backward/Adam CPU goldens.

The fixture binds the frozen ``terminal_reinforce_value/v3`` loss and an
explicit single-tensor (``foreach=False``, ``fused=False``) Adam path.  It is
compact: every full tensor is SHA-256 bound while numerical cross-language
checks use declared tolerances, summaries, and deterministic probes.

The raw numerical bytes are generated and byte-checked only on the declared
Windows authority tuple. ``--check`` is intentionally portable: it checks the
pinned artifact and source bindings plus reconstructable contract invariants,
without asking a different host to claim Torch bit parity.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import platform
import struct
import sys
from typing import Any, Iterable

import torch


# This subprocess is the numerical authority, so its execution topology is
# explicit instead of inherited from the host or CI runner.
TORCH_NUM_THREADS = 1
TORCH_NUM_INTEROP_THREADS = 1
torch.set_num_threads(TORCH_NUM_THREADS)
torch.set_num_interop_threads(TORCH_NUM_INTEROP_THREADS)
torch.use_deterministic_algorithms(True)


ROOT = Path(__file__).resolve().parents[2]
if str(ROOT / "python") not in sys.path:
    sys.path.insert(0, str(ROOT / "python"))

import generate_native_policy_value_net_v1_goldens as forward_fixture  # noqa: E402
from mtg_kernel_rl.model import (  # noqa: E402
    INITIALIZER_RUNNER_FIXED_V1,
    KernelPolicyValueNet,
    ModelConfig,
)
from mtg_kernel_rl.trainer import _compute_loss_tensors  # noqa: E402


SCHEMA = "native-policy-value-cpu-train-step-v1-torch-goldens-v1"
TRAIN_STEP_IDENTITY = "native-policy-value-cpu-train-step-v1"
NATIVE_OPTIMIZER_IDENTITY = "native-adam-canonical-scorer-bias-gauge-v1"
CANONICAL_GAUGE_PARAMETERS = ["scorer.2.bias"]
LEGACY_GAUGE_NONCLAIM = (
    "no exact optimizer-state parity claim for legacy terminal_reinforce_value/v3 "
    "scorer.2.bias f32 gauge drift"
)
MODEL_AUTHORITY = ROOT / "python" / "mtg_kernel_rl" / "model.py"
TRAINER_AUTHORITY = ROOT / "python" / "mtg_kernel_rl" / "trainer.py"
FORWARD_FIXTURE = (
    ROOT
    / "data"
    / "native_policy_value_net_v1"
    / "runner_fixed_forward_goldens_v1.json"
)
OUTPUT = (
    ROOT
    / "data"
    / "native_policy_train_step_v1"
    / "runner_fixed_train_step_goldens_v1.json"
)

EXPECTED_MODEL_AUTHORITY_SHA256 = (
    "2e3e830d4212b8c8f8085861b2508c49a6d7192b9621cef087dd396e22d12c59"
)
EXPECTED_TRAINER_AUTHORITY_SHA256 = (
    "47cd76a243f1b58a695cc0ffe257e8737dcd7bea0b68140349b80139409683ad"
)
EXPECTED_FORWARD_FIXTURE_SHA256 = (
    "c3c5e864f9666cba73b15dc5a038cd57c7d9a46aaccc2b8d3c3c16e956efe9ec"
)
EXPECTED_OUTPUT_SHA256 = "7672c87912b6015f393d66921a3e78cb5623dd76582a9513f2d87c560c0f4aa7"

AUTHORITY_PLATFORM_SYSTEM = "Windows"
AUTHORITY_PLATFORM_MACHINE = "AMD64"
AUTHORITY_PYTHON_VERSION = "3.13.14"
AUTHORITY_TORCH_VERSION = "2.13.0+cpu"

LEARNING_RATE = 1.0e-3
VALUE_COEFFICIENT = 0.5
GRADIENT_NONZERO_WITNESS_FLOOR = 3.0e-7
OPTIMIZER_NONZERO_WITNESS_FLOOR = 5.0e-8
F32_UNIT_ROUNDOFF = float(torch.finfo(torch.float32).eps) / 2.0
LARGE_BATCH_GROUP_COUNT = 32


def _large_batch_program_v1() -> list[dict[str, Any]]:
    """Build one realistic-scale, deterministic cross-language train step."""
    groups: list[dict[str, Any]] = []
    terminal_returns = (-1, 0, 1)
    for group_index in range(LARGE_BATCH_GROUP_COUNT):
        if group_index % 2 == 0:
            substeps = [("zero_edges_zero_action_refs", (group_index // 2) % 2)]
        else:
            substeps = [("ordered_edges_and_action_refs", group_index % 3)]
        if group_index % 4 == 0:
            substeps.append(
                ("ordered_edges_and_action_refs", (group_index + 1) % 3)
            )
        groups.append(
            {
                "terminal_return": terminal_returns[group_index % len(terminal_returns)],
                "substeps": substeps,
            }
        )
    return groups

# Each tuple is (fixture case name, selected action index).  A group is one
# learner physical decision and may contain multiple policy substeps.
PROGRAM: list[list[dict[str, Any]]] = [
    [
        {
            "terminal_return": 1,
            "substeps": [
                ("zero_edges_zero_action_refs", 1),
                ("ordered_edges_and_action_refs", 2),
            ],
        },
        {
            "terminal_return": -1,
            "substeps": [("ordered_edges_and_action_refs", 0)],
        },
    ],
    [
        {
            "terminal_return": 0,
            "substeps": [("zero_edges_zero_action_refs", 0)],
        },
        {
            "terminal_return": 1,
            "substeps": [
                ("ordered_edges_and_action_refs", 1),
                ("zero_edges_zero_action_refs", 1),
            ],
        },
        {
            "terminal_return": -1,
            "substeps": [("ordered_edges_and_action_refs", 2)],
        },
    ],
    _large_batch_program_v1(),
]


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _f32_bits(value: float) -> str:
    return f"0x{struct.unpack('<I', struct.pack('<f', value))[0]:08x}"


def _scalar(tensor: torch.Tensor) -> dict[str, Any]:
    value = float(tensor.detach().item())
    return {"value": value, "f32_bits": _f32_bits(value)}


def _gamma(operation_count: int) -> float:
    if type(operation_count) is not int or operation_count < 0:
        raise ValueError("gamma operation count must be a nonnegative integer")
    scaled = operation_count * F32_UNIT_ROUNDOFF
    if not scaled < 1.0:
        raise ValueError("gamma operation count exceeds the f32 error-bound domain")
    return scaled / (1.0 - scaled)


def _scorer_bias_gauge_record(
    inputs: list[tuple[torch.Tensor, int, float]],
    raw_gradient_residual: torch.Tensor,
    parameter_before: torch.Tensor,
) -> dict[str, Any]:
    """Record the same scale-derived binary32 residual bound as Rust.

    For unit roundoff ``u=eps/2`` and ``gamma(k)=k*u/(1-k*u)``, each
    ``(coefficient, action width n)`` contributes ``abs(c)*gamma(8*n+8)``;
    accumulating ``m`` substeps adds ``gamma(m-1)*2*sum(abs(c))``.
    """
    if not inputs:
        raise ValueError("gauge record requires at least one policy substep")
    ordered_inputs = list(reversed(inputs))
    sum_abs_coefficients = sum(
        abs(coefficient) for _logits, _selected, coefficient in ordered_inputs
    )
    substep_bounds = []
    for logits, _selected, coefficient in ordered_inputs:
        operation_count = 8 * logits.numel() + 8
        gamma = _gamma(operation_count)
        component = abs(coefficient) * gamma
        substep_bounds.append(
            {
                "action_count": logits.numel(),
                "abs_policy_coefficient_f64": abs(coefficient),
                "gamma_operation_count": operation_count,
                "gamma_f64": gamma,
                "bound_component_f64": component,
            }
        )
    per_substep_bound = sum(item["bound_component_f64"] for item in substep_bounds)
    cross_substep_bound = _gamma(len(inputs) - 1) * 2.0 * sum_abs_coefficients
    derived_absolute_bound = per_substep_bound + cross_substep_bound
    raw_value = float(raw_gradient_residual.item())
    if not abs(raw_value) <= derived_absolute_bound:
        raise RuntimeError(
            "raw scorer-bias residual exceeds its derived f32 bound: "
            f"raw={raw_value} bound={derived_absolute_bound}"
        )

    high_precision_residual = 0.0
    for logits, selected, coefficient in ordered_inputs:
        logits_f64 = logits.detach().to(dtype=torch.float64)
        log_probabilities = torch.log_softmax(logits_f64, dim=0)
        grad_output = torch.zeros_like(log_probabilities)
        grad_output[selected] = coefficient
        d_logits = grad_output - torch.exp(log_probabilities) * torch.sum(grad_output)
        for value in reversed(d_logits.tolist()):
            high_precision_residual += float(value)
    if raw_value != 0.0 and not abs(high_precision_residual) < abs(raw_value):
        raise RuntimeError("f64 scorer-bias diagnostic did not shrink the f32 residual")
    return {
        "parameter_name": CANONICAL_GAUGE_PARAMETERS[0],
        "substep_count": len(inputs),
        "total_action_count": sum(item["action_count"] for item in substep_bounds),
        "max_action_count": max(item["action_count"] for item in substep_bounds),
        "sum_abs_policy_coefficients_f64": sum_abs_coefficients,
        "substep_bounds": substep_bounds,
        "per_substep_bound_sum_f64": per_substep_bound,
        "cross_substep_bound_f64": cross_substep_bound,
        "derived_absolute_bound_f64": derived_absolute_bound,
        "raw_gradient_residual": _scalar(raw_gradient_residual),
        "high_precision_residual_f64": high_precision_residual,
        "canonical_gradient": {"value": 0.0, "f32_bits": "0x00000000"},
        "parameter_before_bits": _f32_bits(float(parameter_before.item())),
        "parameter_after_bits": _f32_bits(float(parameter_before.item())),
    }


def _raw_f32(tensor: torch.Tensor) -> bytes:
    contiguous = tensor.detach().cpu().to(dtype=torch.float32).contiguous()
    return contiguous.numpy().astype("<f4", copy=False).tobytes(order="C")


def _tensor_record(
    name: str,
    tensor: torch.Tensor,
    *,
    nonzero_witness_floor: float,
) -> dict[str, Any]:
    contiguous = tensor.detach().cpu().to(dtype=torch.float32).contiguous()
    flat = contiguous.reshape(-1)
    count = flat.numel()
    if count == 0:
        raise RuntimeError(f"golden tensor {name} must be nonempty")
    nonzero_indices = torch.nonzero(flat != 0.0, as_tuple=False).reshape(-1)
    probe_indices = {0, count // 3, count // 2, count - 1}
    probe_indices.add(int(torch.argmax(torch.abs(flat)).item()))
    if nonzero_indices.numel() > 0:
        probe_indices.add(int(nonzero_indices[0].item()))
        probe_indices.add(int(nonzero_indices[-1].item()))
    probes = []
    for index in sorted(probe_indices):
        value = float(flat[index].item())
        probes.append(
            {
                "index": index,
                "value": value,
                "f32_bits": _f32_bits(value),
            }
        )
    double = flat.to(dtype=torch.float64)
    return {
        "name": name,
        "shape": list(contiguous.shape),
        "count": count,
        "sha256_f32_le": hashlib.sha256(_raw_f32(contiguous)).hexdigest(),
        "statistics": {
            "sum_f64": float(double.sum().item()),
            "sum_abs_f64": float(torch.abs(double).sum().item()),
            "sum_squares_f64": float(torch.square(double).sum().item()),
            "minimum_f32": float(torch.min(flat).item()),
            "maximum_f32": float(torch.max(flat).item()),
            "nonzero_count": int(nonzero_indices.numel()),
            "nonzero_witness_floor": nonzero_witness_floor,
            "nonzero_witness_count": int(
                torch.count_nonzero(torch.abs(flat) > nonzero_witness_floor).item()
            ),
        },
        "probes": probes,
    }


def _ordered_tensor_state(
    tensors: Iterable[tuple[str, torch.Tensor]],
    *,
    nonzero_witness_floor: float,
) -> dict[str, Any]:
    digest = hashlib.sha256()
    records = []
    total_count = 0
    for name, tensor in tensors:
        contiguous = tensor.detach().cpu().to(dtype=torch.float32).contiguous()
        shape = list(contiguous.shape)
        raw = _raw_f32(contiguous)
        name_bytes = name.encode("utf-8")
        digest.update(struct.pack(">I", len(name_bytes)))
        digest.update(name_bytes)
        digest.update(struct.pack(">I", len(shape)))
        for dimension in shape:
            digest.update(struct.pack(">Q", dimension))
        digest.update(struct.pack(">Q", contiguous.numel()))
        digest.update(raw)
        records.append(
            _tensor_record(
                name,
                contiguous,
                nonzero_witness_floor=nonzero_witness_floor,
            )
        )
        total_count += contiguous.numel()
    return {
        "digest_contract": (
            "sha256(u32_be(name_len)||name||u32_be(rank)||u64_be(dims...)||"
            "u64_be(count)||f32_le_bytes), named_parameters order"
        ),
        "sha256": digest.hexdigest(),
        "tensor_count": len(records),
        "element_count": total_count,
        "ordered": records,
    }


def _optimizer_state(
    optimizer: torch.optim.Adam,
    model: KernelPolicyValueNet,
    key: str,
) -> dict[str, Any]:
    tensors = []
    for name, parameter in model.named_parameters():
        state = optimizer.state[parameter]
        if key not in state:
            raise RuntimeError(f"Adam state {key} missing for {name}")
        tensors.append((name, state[key]))
    return _ordered_tensor_state(
        tensors,
        nonzero_witness_floor=OPTIMIZER_NONZERO_WITNESS_FLOOR,
    )


def _validate_authorities() -> dict[str, str]:
    actual = {
        "model_sha256": _sha256(MODEL_AUTHORITY),
        "trainer_sha256": _sha256(TRAINER_AUTHORITY),
        "forward_fixture_sha256": _sha256(FORWARD_FIXTURE),
    }
    expected = {
        "model_sha256": EXPECTED_MODEL_AUTHORITY_SHA256,
        "trainer_sha256": EXPECTED_TRAINER_AUTHORITY_SHA256,
        "forward_fixture_sha256": EXPECTED_FORWARD_FIXTURE_SHA256,
    }
    if actual != expected:
        raise RuntimeError(f"native training authority drift: expected={expected} actual={actual}")
    return actual


def _authority_contract(authority_hashes: dict[str, str]) -> dict[str, Any]:
    return {
        "model_path": MODEL_AUTHORITY.relative_to(ROOT).as_posix(),
        "model_sha256": authority_hashes["model_sha256"],
        "trainer_path": TRAINER_AUTHORITY.relative_to(ROOT).as_posix(),
        "trainer_sha256": authority_hashes["trainer_sha256"],
        "forward_fixture_path": FORWARD_FIXTURE.relative_to(ROOT).as_posix(),
        "forward_fixture_sha256": authority_hashes["forward_fixture_sha256"],
        "torch_version": AUTHORITY_TORCH_VERSION,
        "initializer": INITIALIZER_RUNNER_FIXED_V1,
        "numerical_claim": (
            "Rust reproduces Torch CPU selected outputs and grouped loss within declared "
            "tolerances; for gradients, moments, and updated parameters Rust verifies every "
            "tensor name/shape/count plus aggregate statistics, raw nonzero count, extrema, "
            "and deterministic probes within field-specific tolerances after both authorities "
            "apply the versioned canonical scorer-bias gauge. Raw Torch f32 log-softmax "
            "residuals remain separately bound and recorded. Full Torch tensors are "
            "SHA-256 bound as authority evidence, but no every-Rust-element tolerance or "
            "cross-runtime bit-parity claim is made"
        ),
        "exact_authority_scope": (
            "--authority-check and fixture generation require Windows/AMD64, Python 3.13.14, "
            "Torch 2.13.0+cpu, one intraop thread, one interop thread, and deterministic algorithms"
        ),
        "portable_check_scope": (
            "--check validates the checked artifact digest, source authority hashes, schema, "
            "settings, and gauge-bound records without regenerating host-sensitive numerical bytes"
        ),
        "authority_platform_system": AUTHORITY_PLATFORM_SYSTEM,
        "authority_platform_machine": AUTHORITY_PLATFORM_MACHINE,
        "authority_python_version": AUTHORITY_PYTHON_VERSION,
        "selected_output_absolute_tolerance": 3.0e-5,
        "selected_output_relative_tolerance": 3.0e-5,
        "loss_absolute_tolerance": 5.0e-5,
        "loss_relative_tolerance": 5.0e-5,
        "gradient_absolute_tolerance": 3.0e-4,
        "gradient_relative_tolerance": 3.0e-4,
        "optimizer_absolute_tolerance": 5.0e-5,
        "optimizer_relative_tolerance": 5.0e-5,
        "torch_num_threads": TORCH_NUM_THREADS,
        "torch_num_interop_threads": TORCH_NUM_INTEROP_THREADS,
        "torch_deterministic_algorithms": True,
        "torch_default_dtype": "torch.float32",
    }


def _optimizer_contract() -> dict[str, Any]:
    return {
        "name": "Adam",
        "identity": NATIVE_OPTIMIZER_IDENTITY,
        "trainer_algorithm": "terminal_reinforce_value/v3",
        "canonical_gauge_parameters": CANONICAL_GAUGE_PARAMETERS,
        "legacy_gauge_nonclaim": LEGACY_GAUGE_NONCLAIM,
        "exact_arithmetic_proof": (
            "scorer.2.bias adds one common scalar to every legal-action logit; "
            "log-softmax is translation-invariant, so dL/db = c*(1-sum(softmax)) = 0"
        ),
        "value_head_gauge": (
            "none; value_head.2.bias changes the scalar value prediction and is optimized normally"
        ),
        "learning_rate": LEARNING_RATE,
        "betas": [0.9, 0.999],
        "epsilon": 1.0e-8,
        "weight_decay": 0.0,
        "amsgrad": False,
        "foreach": False,
        "maximize": False,
        "capturable": False,
        "differentiable": False,
        "fused": False,
    }


def _assert_exact_authority_environment() -> None:
    actual = {
        "platform_system": platform.system(),
        "platform_machine": platform.machine(),
        "python_version": platform.python_version(),
        "torch_version": torch.__version__,
        "torch_num_threads": torch.get_num_threads(),
        "torch_num_interop_threads": torch.get_num_interop_threads(),
        "torch_deterministic_algorithms": torch.are_deterministic_algorithms_enabled(),
        "torch_default_dtype": str(torch.get_default_dtype()),
    }
    expected = {
        "platform_system": AUTHORITY_PLATFORM_SYSTEM,
        "platform_machine": AUTHORITY_PLATFORM_MACHINE,
        "python_version": AUTHORITY_PYTHON_VERSION,
        "torch_version": AUTHORITY_TORCH_VERSION,
        "torch_num_threads": TORCH_NUM_THREADS,
        "torch_num_interop_threads": TORCH_NUM_INTEROP_THREADS,
        "torch_deterministic_algorithms": True,
        "torch_default_dtype": "torch.float32",
    }
    if actual != expected:
        raise RuntimeError(
            "native train-step exact authority environment mismatch: "
            f"expected={expected} actual={actual}"
        )


def _payload() -> dict[str, Any]:
    authority_hashes = _validate_authorities()
    config = ModelConfig()
    model = KernelPolicyValueNet(config, initializer=INITIALIZER_RUNNER_FIXED_V1)
    optimizer = torch.optim.Adam(
        model.parameters(),
        lr=LEARNING_RATE,
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
    scorer_bias = dict(model.named_parameters())[CANONICAL_GAUGE_PARAMETERS[0]]
    initial_parameters = _ordered_tensor_state(
        model.named_parameters(),
        nonzero_witness_floor=0.0,
    )
    steps = []
    for step_index, groups in enumerate(PROGRAM, start=1):
        scorer_bias_before = scorer_bias.detach().clone()
        existing_scorer_state = optimizer.state.get(scorer_bias, {})
        for key in ("exp_avg", "exp_avg_sq"):
            if key in existing_scorer_state and not torch.equal(
                existing_scorer_state[key], torch.zeros_like(existing_scorer_state[key])
            ):
                raise RuntimeError(f"canonical scorer-bias {key} must be exact zero")
        terms: list[tuple[torch.Tensor, torch.Tensor, int]] = []
        gauge_inputs: list[tuple[torch.Tensor, int, float]] = []
        selected_outputs = []
        program_groups = []
        for group_index, group in enumerate(groups):
            joint_log_probability: torch.Tensor | None = None
            first_value: torch.Tensor | None = None
            program_substeps = []
            group_gauge_inputs: list[tuple[torch.Tensor, int]] = []
            for substep_index, (case_name, selected_index) in enumerate(group["substeps"]):
                encoded = forward_fixture._encoded(raw_cases[case_name], config)
                logits, value = model(encoded)
                if selected_index < 0 or selected_index >= logits.numel():
                    raise RuntimeError("golden selected action is out of range")
                selected_log_probability = torch.log_softmax(logits, dim=0)[selected_index]
                joint_log_probability = (
                    selected_log_probability
                    if joint_log_probability is None
                    else joint_log_probability + selected_log_probability
                )
                if first_value is None:
                    first_value = value
                selected_outputs.append(
                    {
                        "group_index": group_index,
                        "substep_index": substep_index,
                        "case": case_name,
                        "selected_action_index": selected_index,
                        "selected_logit": _scalar(logits[selected_index]),
                        "value": _scalar(value),
                        "selected_log_probability": _scalar(selected_log_probability),
                    }
                )
                program_substeps.append(
                    {"case": case_name, "selected_action_index": selected_index}
                )
                group_gauge_inputs.append((logits.detach().clone(), selected_index))
            if joint_log_probability is None or first_value is None:
                raise RuntimeError("golden physical decision must be nonempty")
            terminal_return = int(group["terminal_return"])
            terms.append((joint_log_probability, first_value, terminal_return))
            target = torch.tensor(float(terminal_return), dtype=first_value.dtype)
            policy_coefficient = float((-(target - first_value.detach()) / len(groups)).item())
            gauge_inputs.extend(
                (logits, selected, policy_coefficient)
                for logits, selected in group_gauge_inputs
            )
            program_groups.append(
                {
                    "terminal_return": terminal_return,
                    "substeps": program_substeps,
                    "joint_log_probability": _scalar(joint_log_probability),
                    "value_from_substep_zero": _scalar(first_value),
                }
            )

        policy_sum, value_sum, loss = _compute_loss_tensors(terms, VALUE_COEFFICIENT)
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        if scorer_bias.grad is None:
            raise RuntimeError("raw scorer-bias gradient is missing")
        raw_scorer_bias_gradient = scorer_bias.grad.detach().clone()
        scorer_bias_gauge = _scorer_bias_gauge_record(
            gauge_inputs,
            raw_scorer_bias_gradient,
            scorer_bias_before,
        )
        scorer_bias.grad.zero_()
        gradients = []
        for name, parameter in model.named_parameters():
            if parameter.grad is None:
                raise RuntimeError(f"golden gradient missing for {name}")
            if not torch.isfinite(parameter.grad).all():
                raise RuntimeError(f"golden gradient is non-finite for {name}")
            gradients.append((name, parameter.grad))
        gradient_state = _ordered_tensor_state(
            gradients,
            nonzero_witness_floor=GRADIENT_NONZERO_WITNESS_FLOOR,
        )
        optimizer.step()
        scorer_state = optimizer.state[scorer_bias]
        with torch.no_grad():
            scorer_bias.copy_(scorer_bias_before)
            scorer_state["exp_avg"].zero_()
            scorer_state["exp_avg_sq"].zero_()
        if not torch.equal(scorer_bias, scorer_bias_before):
            raise RuntimeError("canonical scorer-bias parameter drifted")
        for name, parameter in model.named_parameters():
            if not torch.isfinite(parameter).all():
                raise RuntimeError(f"golden parameter is non-finite for {name}")
        steps.append(
            {
                "step": step_index,
                "groups": program_groups,
                "selected_outputs": selected_outputs,
                "loss": {
                    "policy_sum": _scalar(policy_sum),
                    "value_sum": _scalar(value_sum),
                    "loss": _scalar(loss),
                    "reduction": (
                        "(sum(-joint_log_probability * detached_advantage) + "
                        "value_coefficient * sum((value_from_substep_zero - terminal_return)^2)) "
                        "/ learner_physical_decision_count"
                    ),
                },
                "scorer_bias_gauge": scorer_bias_gauge,
                "gradients_before_adam": gradient_state,
                "first_moments_after_adam": _optimizer_state(optimizer, model, "exp_avg"),
                "second_moments_after_adam": _optimizer_state(
                    optimizer, model, "exp_avg_sq"
                ),
                "parameters_after_adam": _ordered_tensor_state(
                    model.named_parameters(),
                    nonzero_witness_floor=OPTIMIZER_NONZERO_WITNESS_FLOOR,
                ),
            }
        )

    return {
        "schema": SCHEMA,
        "identity": TRAIN_STEP_IDENTITY,
        "authority": _authority_contract(authority_hashes),
        "model_config": config.to_dict(),
        "optimizer": _optimizer_contract(),
        "value_coefficient": VALUE_COEFFICIENT,
        "initial_parameters": initial_parameters,
        "steps": steps,
    }


def _encoded_payload(payload: dict[str, Any]) -> bytes:
    return (json.dumps(payload, sort_keys=True, indent=2) + "\n").encode("utf-8")


def _validate_portable_gauge_records(payload: dict[str, Any]) -> None:
    shrink_witness = False
    steps = payload.get("steps")
    if not isinstance(steps, list) or len(steps) != len(PROGRAM):
        raise RuntimeError("checked train-step fixture has an invalid step program")
    if not any(
        isinstance(step.get("groups"), list)
        and len(step["groups"]) == LARGE_BATCH_GROUP_COUNT
        for step in steps
    ):
        raise RuntimeError("checked train-step fixture lacks the large-batch witness")
    for expected_step, step in enumerate(steps, start=1):
        if step.get("step") != expected_step:
            raise RuntimeError("checked train-step fixture has noncontiguous steps")
        record = step.get("scorer_bias_gauge")
        if not isinstance(record, dict):
            raise RuntimeError("checked train-step fixture is missing a gauge record")
        substeps = record.get("substep_bounds")
        if not isinstance(substeps, list) or not substeps:
            raise RuntimeError("checked gauge record has no substep bounds")
        if record.get("parameter_name") != CANONICAL_GAUGE_PARAMETERS[0]:
            raise RuntimeError("checked gauge record names the wrong parameter")
        if record.get("substep_count") != len(substeps):
            raise RuntimeError("checked gauge substep count is inconsistent")

        total_actions = 0
        max_actions = 0
        coefficient_sum = 0.0
        per_substep_sum = 0.0
        for substep in substeps:
            action_count = substep.get("action_count")
            coefficient = substep.get("abs_policy_coefficient_f64")
            operation_count = substep.get("gamma_operation_count")
            gamma = substep.get("gamma_f64")
            component = substep.get("bound_component_f64")
            if type(action_count) is not int or action_count <= 0:
                raise RuntimeError("checked gauge action count is invalid")
            if not isinstance(coefficient, (int, float)) or not math.isfinite(coefficient):
                raise RuntimeError("checked gauge coefficient is invalid")
            expected_operation_count = 8 * action_count + 8
            expected_gamma = _gamma(expected_operation_count)
            if operation_count != expected_operation_count or not math.isclose(
                gamma, expected_gamma, rel_tol=1.0e-15, abs_tol=1.0e-18
            ):
                raise RuntimeError("checked gauge gamma derivation is invalid")
            expected_component = abs(float(coefficient)) * expected_gamma
            if not math.isclose(
                component, expected_component, rel_tol=1.0e-15, abs_tol=1.0e-18
            ):
                raise RuntimeError("checked gauge substep bound is invalid")
            total_actions += action_count
            max_actions = max(max_actions, action_count)
            coefficient_sum += abs(float(coefficient))
            per_substep_sum += float(component)

        if record.get("total_action_count") != total_actions:
            raise RuntimeError("checked gauge total action count is inconsistent")
        if record.get("max_action_count") != max_actions:
            raise RuntimeError("checked gauge maximum action count is inconsistent")
        if not math.isclose(
            record.get("sum_abs_policy_coefficients_f64"),
            coefficient_sum,
            rel_tol=1.0e-15,
            abs_tol=1.0e-18,
        ):
            raise RuntimeError("checked gauge coefficient sum is inconsistent")
        if not math.isclose(
            record.get("per_substep_bound_sum_f64"),
            per_substep_sum,
            rel_tol=1.0e-15,
            abs_tol=1.0e-18,
        ):
            raise RuntimeError("checked gauge per-substep bound sum is inconsistent")
        cross_bound = _gamma(len(substeps) - 1) * 2.0 * coefficient_sum
        if not math.isclose(
            record.get("cross_substep_bound_f64"),
            cross_bound,
            rel_tol=1.0e-15,
            abs_tol=1.0e-18,
        ):
            raise RuntimeError("checked gauge cross-substep bound is inconsistent")
        derived_bound = per_substep_sum + cross_bound
        if not math.isclose(
            record.get("derived_absolute_bound_f64"),
            derived_bound,
            rel_tol=1.0e-15,
            abs_tol=1.0e-18,
        ):
            raise RuntimeError("checked gauge total bound is inconsistent")

        raw = float(record["raw_gradient_residual"]["value"])
        high_precision = float(record["high_precision_residual_f64"])
        if not math.isfinite(raw) or abs(raw) > derived_bound:
            raise RuntimeError("checked raw gauge residual exceeds its derived bound")
        if not math.isfinite(high_precision):
            raise RuntimeError("checked high-precision gauge residual is non-finite")
        if raw != 0.0:
            if not abs(high_precision) < abs(raw):
                raise RuntimeError("checked high-precision residual does not shrink")
            shrink_witness = True
        if record.get("canonical_gradient") != {
            "value": 0.0,
            "f32_bits": "0x00000000",
        }:
            raise RuntimeError("checked canonical gauge gradient is not exact zero")
        if record.get("parameter_before_bits") != record.get("parameter_after_bits"):
            raise RuntimeError("checked canonical gauge parameter drifted")
    if not shrink_witness:
        raise RuntimeError("checked gauge records lack a nonzero f32 shrink witness")


def _portable_check() -> None:
    portable_runtime = {
        "python_version": platform.python_version(),
        "torch_version": torch.__version__,
        "torch_num_threads": torch.get_num_threads(),
        "torch_num_interop_threads": torch.get_num_interop_threads(),
        "torch_deterministic_algorithms": torch.are_deterministic_algorithms_enabled(),
        "torch_default_dtype": str(torch.get_default_dtype()),
    }
    expected_runtime = {
        "python_version": AUTHORITY_PYTHON_VERSION,
        "torch_version": AUTHORITY_TORCH_VERSION,
        "torch_num_threads": TORCH_NUM_THREADS,
        "torch_num_interop_threads": TORCH_NUM_INTEROP_THREADS,
        "torch_deterministic_algorithms": True,
        "torch_default_dtype": "torch.float32",
    }
    if portable_runtime != expected_runtime:
        raise RuntimeError(
            "native train-step portable checker runtime mismatch: "
            f"expected={expected_runtime} actual={portable_runtime}"
        )
    if not OUTPUT.is_file():
        raise RuntimeError(f"checked train-step fixture is missing: {OUTPUT}")
    checked_bytes = OUTPUT.read_bytes()
    checked_sha256 = hashlib.sha256(checked_bytes).hexdigest()
    if checked_sha256 != EXPECTED_OUTPUT_SHA256:
        raise RuntimeError(
            "checked train-step artifact digest drift: "
            f"expected={EXPECTED_OUTPUT_SHA256} actual={checked_sha256}"
        )
    checked = json.loads(checked_bytes)
    authority_hashes = _validate_authorities()
    expected_static = {
        "schema": SCHEMA,
        "identity": TRAIN_STEP_IDENTITY,
        "authority": _authority_contract(authority_hashes),
        "model_config": ModelConfig().to_dict(),
        "optimizer": _optimizer_contract(),
        "value_coefficient": VALUE_COEFFICIENT,
    }
    for key, expected in expected_static.items():
        if checked.get(key) != expected:
            raise RuntimeError(f"checked train-step fixture static contract drift: {key}")
    _validate_portable_gauge_records(checked)


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--check",
        action="store_true",
        help="portable artifact, source-binding, schema, setting, and bound check",
    )
    mode.add_argument(
        "--authority-check",
        action="store_true",
        help="on the declared authority tuple, regenerate and require byte identity",
    )
    args = parser.parse_args()
    if args.check:
        _portable_check()
        print(f"PASS portable {OUTPUT.relative_to(ROOT)}")
        return 0
    _assert_exact_authority_environment()
    expected = _encoded_payload(_payload())
    if args.authority_check:
        actual = OUTPUT.read_bytes() if OUTPUT.exists() else b""
        if actual != expected:
            raise SystemExit(f"stale native policy train-step authority golden: {OUTPUT}")
        print(f"PASS authority {OUTPUT.relative_to(ROOT)}")
        return 0
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_bytes(expected)
    print(f"wrote {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
