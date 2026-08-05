#!/usr/bin/env python3
"""Select one exact v2 CP7 response arm from movement evidence only."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
from typing import Any


CORPUS_SHA256 = "fe95949e852227259efda060889c2ea707033f77b919f6100f42f5feeef754b4"
SOURCE_MANIFEST_SHA256 = "d86a430298a5a91be324e22d798c7cdc7f9f5e61e7ffa255a559ff68287e5ab1"
SOURCE_PAYLOAD_SHA256 = "a0b7752181a562f8e5a0821a490ce20b777b509855d754283536e8242f489b98"
SOURCE_NATIVE_STATE_SHA256 = "ab7dd25ca6619a4a613ca089e1eb8e75981f8e5cfc0bae8535b78cddd7efa952"
SOURCE_ADAM_STEP = 520
ENDING_ADAM_STEP = 524
LEARNING_RATE_F32_BITS = 981668463
PPO_CLIP_F32_BITS = 1036831949
ARMS = {
    "policy-only": {
        "schema": "mtg-kernel-current-net8-cp7-terminal-response-v2-policy-only-training/v1",
        "authority": "current-net8-cp7-terminal-response-v2-policy-only",
        "value_coefficient_f32_bits": 0,
    },
    "low-value": {
        "schema": "mtg-kernel-current-net8-cp7-terminal-response-v2-low-value-training/v1",
        "authority": "current-net8-cp7-terminal-response-v2-low-value",
        "value_coefficient_f32_bits": 1036831949,
    },
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        raise ValueError(f"{path} is not a JSON object")
    return value


def validate_arm(
    name: str,
    report_path: Path,
    package_root: Path,
) -> dict[str, Any]:
    contract = ARMS[name]
    report = load_json(report_path)
    if report.get("schema") != contract["schema"]:
        raise ValueError(f"{name} report schema mismatch")
    source = report.get("source", {})
    if (
        source.get("authority_kind") != "current-net8-gae8-v1"
        or source.get("manifest_sha256") != SOURCE_MANIFEST_SHA256
        or source.get("payload_sha256") != SOURCE_PAYLOAD_SHA256
        or source.get("native_state_sha256") != SOURCE_NATIVE_STATE_SHA256
        or source.get("adam_step") != SOURCE_ADAM_STEP
    ):
        raise ValueError(f"{name} source binding mismatch")
    corpus = report.get("corpus", {})
    if (
        corpus.get("sha256") != CORPUS_SHA256
        or corpus.get("pair_indices") != list(range(64))
        or corpus.get("pair_count") != 64
        or corpus.get("episode_count") != 128
        or corpus.get("decision_row_count") != 4769
        or corpus.get("terminal_return_counts_loss_draw_win") != [80, 0, 48]
    ):
        raise ValueError(f"{name} corpus binding mismatch")
    training = report.get("training", {})
    if (
        training.get("reward") != "natural_terminal_win_draw_loss_only"
        or training.get("learning_rate_f32_bits") != LEARNING_RATE_F32_BITS
        or training.get("value_coefficient_f32_bits")
        != contract["value_coefficient_f32_bits"]
        or training.get("ppo_clip_epsilon_f32_bits") != PPO_CLIP_F32_BITS
        or training.get("epochs") != 4
        or training.get("starting_adam_step") != SOURCE_ADAM_STEP
        or training.get("ending_adam_step") != ENDING_ADAM_STEP
        or training.get("advantage_transform", {}).get("identity")
        != "terminal_reinforce_frozen_source_value_standardized_by_candidate_seat_episode_balanced/v1"
    ):
        raise ValueError(f"{name} training recipe mismatch")
    candidate = report.get("candidate", {})
    movement = candidate.get("movement", {})
    finite_values = [
        candidate.get("parameter_l2_from_gae8"),
        movement.get("mean_action_total_variation"),
        movement.get("p90_action_total_variation_nearest_rank"),
        movement.get("maximum_absolute_joint_log_likelihood_ratio"),
    ]
    if not all(isinstance(value, (int, float)) for value in finite_values):
        raise ValueError(f"{name} movement fields are absent")
    parameter_l2, mean_tv, p90_tv, max_log_ratio = map(float, finite_values)
    finite = all(
        value == value and value not in (float("inf"), float("-inf"))
        for value in finite_values
    )
    recomputed_pass = (
        finite
        and candidate.get("adam_step") == ENDING_ADAM_STEP
        and parameter_l2 <= 0.75
        and 0.010 <= mean_tv <= 0.050
        and p90_tv <= 0.150
        and max_log_ratio <= 1.0
    )
    gate = report.get("publication_gate", {})
    if (
        gate.get("finite") != finite
        or gate.get("parameter_l2_cap") != 0.75
        or gate.get("mean_action_total_variation_floor") != 0.010
        or gate.get("mean_action_total_variation_cap") != 0.050
        or gate.get("p90_action_total_variation_cap") != 0.150
        or gate.get("maximum_absolute_joint_log_likelihood_ratio_cap") != 1.0
        or gate.get("pass") != recomputed_pass
    ):
        raise ValueError(f"{name} publication gate mismatch")

    report_sha256 = sha256(report_path)
    if recomputed_pass:
        if not package_root.is_dir():
            raise ValueError(f"{name} passing package is absent")
        files = sorted(path.name for path in package_root.iterdir())
        if files != ["checkpoint.state.f32le", "fixed_native_state.json"]:
            raise ValueError(f"{name} package inventory mismatch")
        manifest = load_json(package_root / "fixed_native_state.json")
        payload = manifest.get("payload", {})
        if (
            manifest.get("schema") != "mtg-kernel-xmage-fixed-native-state/v1"
            or manifest.get("authority_kind") != contract["authority"]
            or manifest.get("source_result_sha256") != report_sha256
            or payload.get("filename") != "checkpoint.state.f32le"
            or payload.get("adam_step") != ENDING_ADAM_STEP
            or payload.get("payload_sha256") != candidate.get("payload_sha256")
            or payload.get("native_state_sha256") != candidate.get("native_state_sha256")
            or payload.get("model_parameter_sha256")
            != candidate.get("model_parameter_sha256")
            or sha256(package_root / "checkpoint.state.f32le")
            != candidate.get("payload_sha256")
        ):
            raise ValueError(f"{name} package binding mismatch")
        manifest_sha256 = sha256(package_root / "fixed_native_state.json")
    else:
        if package_root.exists():
            raise ValueError(f"{name} failing arm unexpectedly has a package")
        manifest_sha256 = None

    return {
        "arm": name,
        "authority_kind": contract["authority"],
        "eligible": recomputed_pass,
        "report_path": str(report_path),
        "report_sha256": report_sha256,
        "package_root": str(package_root),
        "manifest_sha256": manifest_sha256,
        "payload_sha256": candidate.get("payload_sha256"),
        "native_state_sha256": candidate.get("native_state_sha256"),
        "model_parameter_sha256": candidate.get("model_parameter_sha256"),
        "parameter_l2_from_gae8": parameter_l2,
        "mean_action_total_variation": mean_tv,
        "p90_action_total_variation": p90_tv,
        "maximum_absolute_joint_log_likelihood_ratio": max_log_ratio,
    }


def select(policy: dict[str, Any], low_value: dict[str, Any]) -> str | None:
    eligible = [arm for arm in [policy, low_value] if arm["eligible"]]
    if not eligible:
        return None
    if len(eligible) == 1:
        return str(eligible[0]["arm"])
    policy_tv = float(policy["mean_action_total_variation"])
    low_value_tv = float(low_value["mean_action_total_variation"])
    return "low-value" if low_value_tv > policy_tv else "policy-only"


def write_new_json(path: Path, value: dict[str, Any]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_BINARY", 0)
    data = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")
    descriptor = os.open(path, flags)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
    except BaseException:
        try:
            path.unlink()
        except FileNotFoundError:
            pass
        raise
    return hashlib.sha256(data).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--policy-report", type=Path, required=True)
    parser.add_argument("--policy-package", type=Path, required=True)
    parser.add_argument("--low-value-report", type=Path, required=True)
    parser.add_argument("--low-value-package", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    policy = validate_arm("policy-only", args.policy_report, args.policy_package)
    low_value = validate_arm("low-value", args.low_value_report, args.low_value_package)
    selected = select(policy, low_value)
    result = {
        "schema": "mtg-kernel-current-net8-cp7-terminal-response-v2-selection/v1",
        "selection_basis": "movement_only_no_fresh_gameplay",
        "arms": [policy, low_value],
        "selected_arm": selected,
        "advance_to_downstream_gates": selected is not None,
        "tie_break": "higher_mean_tv_then_policy_only_on_exact_tie",
        "nonclaims": [
            "movement-only selection is not playing-strength evidence",
            "natural terminal win draw or loss remains the only downstream promotion measure",
        ],
    }
    result_sha256 = write_new_json(args.output, result)
    print(
        json.dumps(
            {
                "schema": "mtg-kernel-current-net8-cp7-terminal-response-v2-selection-summary/v1",
                "selected_arm": selected,
                "advance_to_downstream_gates": selected is not None,
                "report_sha256": result_sha256,
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
