"""Describe actual V3 CPU/CUDA probe differences, without declaring acceptance.

Requires the completed device diagnostic manifest and verifies its output pins.
Final parameter values and parameter deltas are reported separately so a small
learning rate cannot conceal a missing update. No model selection occurs here.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct


def decode(bits):
    if any(type(value) is not int or not 0 <= value <= 0xFFFFFFFF for value in bits):
        raise ValueError("invalid binary32 bit encoding")
    values = [struct.unpack("<f", struct.pack("<I", value))[0] for value in bits]
    if any(not math.isfinite(value) for value in values):
        raise ValueError("nonfinite probe value")
    return values


def binary32(value):
    """Restore serde's short f32 decimal to its actual binary32 value."""
    if type(value) not in (int, float) or not math.isfinite(value):
        raise ValueError("finite binary32 scalar required")
    try:
        result = struct.unpack("<f", struct.pack("<f", value))[0]
    except (OverflowError, struct.error) as error:
        raise ValueError("binary32 range exceeded") from error
    if not math.isfinite(result):
        raise ValueError("binary32 range exceeded")
    return result


def validate_objective(device, inputs, first, substep_count):
    if binary32(device["value_coefficient"]) != decode([inputs["value_coefficient_bits"]])[0]:
        raise ValueError("device loss coefficient differs")
    group_count = len(first)
    next_group, next_step, total = 0, 0, 0.0
    if not device["chunks"]:
        raise ValueError("device chunks missing")
    for chunk in device["chunks"]:
        gb, ge, sb, se = [chunk[k] for k in ("group_begin", "group_end", "substep_begin", "substep_end")]
        if any(type(x) is not int for x in (gb, ge, sb, se)) or gb != next_group or sb != next_step or not gb < ge <= group_count or not sb < se <= substep_count:
            raise ValueError("device chunk partition differs")
        if sb != first[gb] or se != (first[ge] if ge < group_count else substep_count):
            raise ValueError("device chunk splits a physical group")
        total += binary32(chunk["device_objective"])
        next_group, next_step = ge, se
    if next_group != group_count or next_step != substep_count or total != device["device_objective_host_sum"]:
        raise ValueError("device chunk aggregate differs")
    return total


def metrics(reference, actual, rms_floor):
    if not reference or len(reference) != len(actual):
        raise ValueError("empty or mismatched vectors")
    if not math.isfinite(rms_floor) or rms_floor <= 0:
        raise ValueError("positive finite RMS floor required")
    if any(not math.isfinite(x) for x in reference + actual):
        raise ValueError("nonfinite comparison")
    differences = [a-r for r, a in zip(reference, actual)]
    ref_rms = math.sqrt(math.fsum(x*x for x in reference)/len(reference))
    error_rms = math.sqrt(math.fsum(x*x for x in differences)/len(differences))
    worst = max(range(len(differences)), key=lambda i: abs(differences[i]))
    return {"count": len(reference), "max_abs_error": abs(differences[worst]),
            "worst_index": worst, "reference_at_worst": reference[worst],
            "actual_at_worst": actual[worst], "reference_rms": ref_rms,
            "error_rms": error_rms, "normalized_rms_error": error_rms/max(ref_rms, rms_floor),
            "normalization_floor_active": ref_rms < rms_floor}


def named_metrics(reference, actual, rms_floor, initial=None):
    if len(reference) != 33 or len(actual) != 33:
        raise ValueError("all 33 named tensors required")
    if initial is not None and len(initial) != 33:
        raise ValueError("all 33 initial tensors required")
    names = [p["name"] for p in reference]
    if len(set(names)) != 33:
        raise ValueError("duplicate tensor names")
    rows = []
    for i, (left, right) in enumerate(zip(reference, actual)):
        sources = [left, right] + ([] if initial is None else [initial[i]])
        if any(p["name"] != left["name"] or p["shape"] != left["shape"] for p in sources):
            raise ValueError("named tensor identity or shape mismatch")
        if not left["shape"] or any(type(n) is not int or n <= 0 for n in left["shape"]):
            raise ValueError("invalid tensor shape")
        size = math.prod(left["shape"])
        if any(len(p["values"]) != size for p in sources):
            raise ValueError("tensor extent mismatch")
        r, a = decode(left["values"]), decode(right["values"])
        if initial is not None:
            origin = decode(initial[i]["values"])
            r, a = [x-y for x, y in zip(r, origin)], [x-y for x, y in zip(a, origin)]
        rows.append({"name": left["name"], "shape": left["shape"], **metrics(r, a, rms_floor)})
    return rows


def selected_policy(logits, offsets, selected):
    if len(offsets) != len(selected)+1 or offsets[0] != 0 or offsets[-1] != len(logits):
        raise ValueError("policy row partition mismatch")
    probabilities, log_probabilities = [], []
    for start, end, choice in zip(offsets, offsets[1:], selected):
        row = logits[start:end]
        if start < 0 or end <= start or type(choice) is not int or not 0 <= choice < len(row) or any(not math.isfinite(x) for x in row):
            raise ValueError("invalid policy row")
        maximum = max(row)
        logp = row[choice]-maximum-math.log(math.fsum(math.exp(x-maximum) for x in row))
        probabilities.append(math.exp(logp))
        log_probabilities.append(logp)
    return probabilities, log_probabilities


def load_outputs(completion_path):
    raw = completion_path.read_bytes()
    completion = json.loads(raw)
    if completion.get("schema") != "mtg-kernel-v3-numerical-probe/v1" or completion.get("complete") is not True or completion.get("device_work_performed") is not True:
        raise ValueError("completed actual-device diagnostic required")
    outputs = {}
    for pin in completion["outputs"]:
        path = Path(pin["path"])
        if path.name in outputs:
            raise ValueError("duplicate probe output name")
        if path.stat().st_size > 512*1024*1024:
            raise ValueError("probe output exceeds bound")
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() != pin["sha256"]:
            raise ValueError("probe output SHA mismatch")
        outputs[path.name] = json.loads(data)
    return completion, outputs, hashlib.sha256(raw).hexdigest()


def compare(completion_path, rms_floor):
    completion, out, manifest_sha = load_outputs(completion_path)
    initial, cpu, gpu = (out[name] for name in ["initial.json", "cpu-after.json", "cuda-after.json"])
    cpu_result, gpu_result, device, inputs = (out[name] for name in ["cpu-result.json", "cuda-result.json", "cuda-device.json", "inputs.json"])
    if initial["state_sha256"] != completion["initial_state_sha256"]:
        raise ValueError("shared initial state identity differs")
    if cpu_result["loss_source"] != "actual-cpu-forward" or gpu_result["loss_source"] != "transported-cpu-outputs":
        raise ValueError("unexpected loss provenance")
    template = [(p["name"], p["shape"]) for p in initial["parameters"]]
    for result, snapshot in [(cpu_result, cpu), (gpu_result, gpu)]:
        if result["adam_step"] != snapshot["adam_step"] or [(p["name"], p["shape"]) for p in result["gradients"]] != template:
            raise ValueError("gradient or optimizer identity differs")
    groups = inputs["groups"]
    rows = [row for group in groups for row in group["rows"]]
    offsets, first, membership = [0], [], []
    for group_index, group in enumerate(groups):
        if not group["rows"] or group["baseline_bits"] != 0:
            raise ValueError("invalid physical group")
        first.append(len(membership))
        for row in group["rows"]:
            offsets.append(offsets[-1]+len(row["logits"]))
            membership.append(group_index)
    expected = {"normalization_group_count": len(groups), "action_offsets": offsets,
                "group_first_substeps": first, "substep_group_indices": membership,
                "selected_action_indices": [row["selected"] for row in rows],
                "terminal_returns": [group["terminal_return"] for group in groups],
                "device_ordinal": completion["gpu_ordinal"]}
    if any(device[key] != value for key, value in expected.items()):
        raise ValueError("device rows do not match CPU inputs")
    cuda_loss = validate_objective(device, inputs, first, len(rows))
    cpu_loss = decode([cpu_result["loss_bits"]])[0]
    cpu_logits = decode([b for row in rows for b in row["logits"]])
    gpu_logits = [binary32(x) for x in device["logit_outputs"]]
    gpu_values = [binary32(x) for x in device["value_outputs"]]
    cpu_p, cpu_logp = selected_policy(cpu_logits, offsets, expected["selected_action_indices"])
    gpu_p, gpu_logp = selected_policy(gpu_logits, offsets, expected["selected_action_indices"])
    summary = {
        "schema": "mtg-kernel-v3-numerical-comparison/v1", "completion_sha256": manifest_sha,
        "claim": "Descriptive numerical differences only; no qualification, speedup or strength decision",
        "rms_floor": rms_floor, "group_count": len(groups), "substep_count": len(rows),
        "logits": metrics(cpu_logits, gpu_logits, rms_floor),
        "selected_probabilities": metrics(cpu_p, gpu_p, rms_floor),
        "selected_log_probabilities": metrics(cpu_logp, gpu_logp, rms_floor),
        "policy_comparison_arithmetic": "Stable host float64 softmax applied separately to CPU and actual CUDA forward outputs",
        "values": metrics(decode([row["value"] for row in rows]), gpu_values, rms_floor),
        "objective": {"cpu": cpu_loss, "cuda_device_scalars_host_sum": cuda_loss, "absolute_error": abs(cuda_loss-cpu_loss)},
        "gradients": named_metrics(cpu_result["gradients"], gpu_result["gradients"], rms_floor),
        "parameters": named_metrics(cpu["parameters"], gpu["parameters"], rms_floor),
        "parameter_deltas": named_metrics(cpu["parameters"], gpu["parameters"], rms_floor, initial["parameters"]),
        "first_moments": named_metrics(cpu["first_moments"], gpu["first_moments"], rms_floor),
        "second_moments": named_metrics(cpu["second_moments"], gpu["second_moments"], rms_floor),
        "exact_invariants": {
            "adam_increment": cpu["adam_step"] == gpu["adam_step"] == initial["adam_step"]+1,
            "scorer_anchor_preserved": cpu["scorer_bias_anchor_bits"] == gpu["scorer_bias_anchor_bits"] == initial["scorer_bias_anchor_bits"],
            "initial_state_matches_manifest": initial["state_sha256"] == completion["initial_state_sha256"],
        },
    }
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("completion", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rms-floor", type=float, default=1e-12)
    args = parser.parse_args()
    result = compare(args.completion, args.rms_floor)
    with args.output.open("x", encoding="utf-8") as handle:
        json.dump(result, handle, indent=2, allow_nan=False)
        handle.write("\n")


if __name__ == "__main__":
    main()
