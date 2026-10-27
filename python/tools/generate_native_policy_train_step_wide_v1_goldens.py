"""Generate Torch-authoritative grouped loss/backward/Adam CPU goldens for the
wide net (kernel-policy-value-net-8w128).

Wide twin of ``generate_native_policy_train_step_v1_goldens.py`` (collab
LANES/throughput-architecture-20261020/WIDE-BACKWARD-DESIGN.md section 5.3):
the same ``terminal_reinforce_value/v3`` loss, explicit single-tensor Adam,
step program and record schema, on the runner-fixed wide model and the
two-case wide forward fixture. The narrow generator, its artifact and its
pinned bytes are untouched: its pure record helpers are imported and its
digest is bound into this fixture's authority.

Generation and ``--authority-check`` require the declared authority tuple
(Windows/AMD64, Python 3.13.14, Torch 2.13.0+cpu, one intraop and one interop
thread, deterministic algorithms). ``--check`` is portable and needs the
generated artifact.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys
from typing import Any

import torch


TORCH_NUM_THREADS = 1
TORCH_NUM_INTEROP_THREADS = 1
if torch.get_num_threads() != TORCH_NUM_THREADS:
    torch.set_num_threads(TORCH_NUM_THREADS)
if torch.get_num_interop_threads() != TORCH_NUM_INTEROP_THREADS:
    torch.set_num_interop_threads(TORCH_NUM_INTEROP_THREADS)
torch.use_deterministic_algorithms(True)


ROOT = Path(__file__).resolve().parents[2]
for path in (ROOT / "python", ROOT / "python" / "tools"):
    if str(path) not in sys.path:
        sys.path.insert(0, str(path))

import generate_native_policy_train_step_v1_goldens as narrow  # noqa: E402
import generate_native_policy_value_net_wide_v1_goldens as forward_fixture  # noqa: E402
from mtg_kernel_rl.model import (  # noqa: E402
    INITIALIZER_RUNNER_FIXED_V1,
    KernelPolicyValueNet,
)
from mtg_kernel_rl.trainer import _compute_loss_tensors  # noqa: E402
from mtg_kernel_rl.wide_model_snapshot_v1 import WideModelConfigV1  # noqa: E402


SCHEMA = "native-policy-value-cpu-train-step-wide-v1-torch-goldens-v1"
NARROW_GENERATOR = ROOT / "python" / "tools" / "generate_native_policy_train_step_v1_goldens.py"
FORWARD_FIXTURE = (
    ROOT
    / "data"
    / "native_policy_value_net_wide_v1"
    / "runner_fixed_forward_goldens_wide_v1.json"
)
OUTPUT = (
    ROOT
    / "data"
    / "native_policy_train_step_wide_v1"
    / "runner_fixed_train_step_goldens_wide_v1.json"
)

EXPECTED_NARROW_GENERATOR_SHA256 = (
    "51c89f1fe6fc46dc6151a9bfaca15e7d0c78b8f731ce537305860a224dedcb53"
)
EXPECTED_FORWARD_FIXTURE_SHA256 = (
    "f665f144ff4dcc084d718b68b960e745550b9517615cf172cf04515ce7032cb3"
)
# Pinned from the first generation on the authority tuple; until then the
# portable check reports the fixture as awaiting generation.
EXPECTED_OUTPUT_SHA256: str | None = None


def _validate_authorities() -> dict[str, str]:
    actual = {
        "model_sha256": narrow._sha256(narrow.MODEL_AUTHORITY),
        "trainer_sha256": narrow._sha256(narrow.TRAINER_AUTHORITY),
        "forward_fixture_sha256": narrow._sha256(FORWARD_FIXTURE),
        "narrow_generator_sha256": narrow._sha256(NARROW_GENERATOR),
    }
    expected = {
        "model_sha256": narrow.EXPECTED_MODEL_AUTHORITY_SHA256,
        "trainer_sha256": narrow.EXPECTED_TRAINER_AUTHORITY_SHA256,
        "forward_fixture_sha256": EXPECTED_FORWARD_FIXTURE_SHA256,
        "narrow_generator_sha256": EXPECTED_NARROW_GENERATOR_SHA256,
    }
    if actual != expected:
        raise RuntimeError(
            f"wide native training authority drift: expected={expected} actual={actual}"
        )
    return actual


def _authority_contract(authority_hashes: dict[str, str]) -> dict[str, Any]:
    contract = narrow._authority_contract(authority_hashes)
    contract["forward_fixture_path"] = FORWARD_FIXTURE.relative_to(ROOT).as_posix()
    contract["narrow_generator_path"] = NARROW_GENERATOR.relative_to(ROOT).as_posix()
    contract["narrow_generator_sha256"] = authority_hashes["narrow_generator_sha256"]
    return contract


def _payload() -> dict[str, Any]:
    authority_hashes = _validate_authorities()
    config = WideModelConfigV1()
    model = KernelPolicyValueNet(config, initializer=INITIALIZER_RUNNER_FIXED_V1)
    optimizer = torch.optim.Adam(
        model.parameters(),
        lr=narrow.LEARNING_RATE,
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
    scorer_bias = dict(model.named_parameters())[narrow.CANONICAL_GAUGE_PARAMETERS[0]]
    initial_parameters = narrow._ordered_tensor_state(
        model.named_parameters(),
        nonzero_witness_floor=0.0,
    )
    steps = []
    for step_index, groups in enumerate(narrow.PROGRAM, start=1):
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
                        "selected_logit": narrow._scalar(logits[selected_index]),
                        "value": narrow._scalar(value),
                        "selected_log_probability": narrow._scalar(selected_log_probability),
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
                    "joint_log_probability": narrow._scalar(joint_log_probability),
                    "value_from_substep_zero": narrow._scalar(first_value),
                }
            )

        policy_sum, value_sum, loss = _compute_loss_tensors(terms, narrow.VALUE_COEFFICIENT)
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        if scorer_bias.grad is None:
            raise RuntimeError("raw scorer-bias gradient is missing")
        raw_scorer_bias_gradient = scorer_bias.grad.detach().clone()
        scorer_bias_gauge = narrow._scorer_bias_gauge_record(
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
        gradient_state = narrow._ordered_tensor_state(
            gradients,
            nonzero_witness_floor=narrow.GRADIENT_NONZERO_WITNESS_FLOOR,
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
                    "policy_sum": narrow._scalar(policy_sum),
                    "value_sum": narrow._scalar(value_sum),
                    "loss": narrow._scalar(loss),
                    "reduction": (
                        "(sum(-joint_log_probability * detached_advantage) + "
                        "value_coefficient * sum((value_from_substep_zero - terminal_return)^2)) "
                        "/ learner_physical_decision_count"
                    ),
                },
                "scorer_bias_gauge": scorer_bias_gauge,
                "gradients_before_adam": gradient_state,
                "first_moments_after_adam": narrow._optimizer_state(optimizer, model, "exp_avg"),
                "second_moments_after_adam": narrow._optimizer_state(
                    optimizer, model, "exp_avg_sq"
                ),
                "parameters_after_adam": narrow._ordered_tensor_state(
                    model.named_parameters(),
                    nonzero_witness_floor=narrow.OPTIMIZER_NONZERO_WITNESS_FLOOR,
                ),
            }
        )

    return {
        "schema": SCHEMA,
        "identity": narrow.TRAIN_STEP_IDENTITY,
        "authority": _authority_contract(authority_hashes),
        "model_config": config.to_dict(),
        "optimizer": narrow._optimizer_contract(),
        "value_coefficient": narrow.VALUE_COEFFICIENT,
        "initial_parameters": initial_parameters,
        "steps": steps,
    }


def _portable_check() -> None:
    if not OUTPUT.is_file():
        raise RuntimeError(
            f"wide train-step fixture awaits generation on the authority tuple: {OUTPUT}"
        )
    checked_bytes = OUTPUT.read_bytes()
    checked_sha256 = hashlib.sha256(checked_bytes).hexdigest()
    if EXPECTED_OUTPUT_SHA256 is None or checked_sha256 != EXPECTED_OUTPUT_SHA256:
        raise RuntimeError(
            "wide train-step artifact digest is not pinned or drifted: "
            f"expected={EXPECTED_OUTPUT_SHA256} actual={checked_sha256}"
        )
    checked = json.loads(checked_bytes)
    authority_hashes = _validate_authorities()
    expected_static = {
        "schema": SCHEMA,
        "identity": narrow.TRAIN_STEP_IDENTITY,
        "authority": _authority_contract(authority_hashes),
        "model_config": WideModelConfigV1().to_dict(),
        "optimizer": narrow._optimizer_contract(),
        "value_coefficient": narrow.VALUE_COEFFICIENT,
    }
    for key, expected in expected_static.items():
        if checked.get(key) != expected:
            raise RuntimeError(f"checked wide train-step fixture static contract drift: {key}")
    narrow._validate_portable_gauge_records(checked)


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
    narrow._assert_exact_authority_environment()
    expected = narrow._encoded_payload(_payload())
    if args.authority_check:
        actual = OUTPUT.read_bytes() if OUTPUT.exists() else b""
        if actual != expected:
            raise SystemExit(f"stale wide native policy train-step authority golden: {OUTPUT}")
        print(f"PASS authority {OUTPUT.relative_to(ROOT)}")
        return 0
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_bytes(expected)
    print(f"wrote {OUTPUT.relative_to(ROOT)}")
    print(f"sha256 {hashlib.sha256(expected).hexdigest()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
