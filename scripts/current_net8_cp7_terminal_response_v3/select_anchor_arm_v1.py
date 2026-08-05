#!/usr/bin/env python3
"""Select one exact V3 CP7 response arm from movement evidence only."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import struct
from pathlib import Path
from typing import Any


MANIFEST_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v3-screen-manifest/v1"
SELECTION_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v3-selection/v1"
SELECTION_SUMMARY_SCHEMA = (
    "mtg-kernel-current-net8-cp7-terminal-response-v3-selection-summary/v1"
)
SOURCE_AUTHORITY = "current-net8-gae8-v1"
SOURCE_RESULT_SHA256 = "dc581e2bd6548c5fa9c05d9d8ab70a2bd6f213b20dd82a042109ee9a500b628e"
SOURCE_MANIFEST_SHA256 = "d86a430298a5a91be324e22d798c7cdc7f9f5e61e7ffa255a559ff68287e5ab1"
SOURCE_PAYLOAD_SHA256 = "a0b7752181a562f8e5a0821a490ce20b777b509855d754283536e8242f489b98"
SOURCE_NATIVE_STATE_SHA256 = "ab7dd25ca6619a4a613ca089e1eb8e75981f8e5cfc0bae8535b78cddd7efa952"
SOURCE_MODEL_PARAMETER_SHA256 = "5efe2f167045bde379da3be8af6c480b6702f5d7a849ff8435d8ac6b1d91daa8"
SOURCE_ADAM_STEP = 520
ENDING_ADAM_STEP = 524
CORPUS_SHA256 = "fe95949e852227259efda060889c2ea707033f77b919f6100f42f5feeef754b4"
CORPUS_PAIR_INDICES = list(range(64))
CORPUS_PAIR_COUNT = 64
CORPUS_EPISODE_COUNT = 128
CORPUS_DECISION_ROW_COUNT = 4769
CORPUS_PHYSICAL_GROUP_COUNT = 4173
CORPUS_TERMINAL_RETURN_COUNTS = [80, 0, 48]
LEARNING_RATE_F32_BITS = 981668463
VALUE_COEFFICIENT_F32_BITS = 0
PPO_CLIP_F32_BITS = 1036831949
ANCHOR_IDENTITY = "old-policy-forward-kl-every-legal-action-distribution/v1"
ANCHOR_DIRECTION = "old_to_current"
ANCHOR_SOURCE = "corpus_old_policy_logits_f32_bits"
ANCHOR_SCOPE = "every_legal_action_distribution"

PARAMETER_L2_CAP = 0.75
MEAN_TV_FLOOR = 0.010
MEAN_TV_CAP = 0.050
P90_ROW_TV_CAP = 0.150
P99_ROW_TV_CAP = 0.150
P99_GROUP_LOG_RATIO_CAP = 0.75
MAX_GROUP_LOG_RATIO_CAP = 1.0
ABOVE_ONE_COUNT_CAP = 0


def f32_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]


ARMS = {
    "beta-0.3": {
        "beta": 0.3,
        "beta_f32_bits": f32_bits(0.3),
        "authority": "current-net8-cp7-terminal-response-v3-kl-0.3",
        "schema": "mtg-kernel-current-net8-cp7-terminal-response-v3-kl-0.3-training/v1",
    },
    "beta-1.0": {
        "beta": 1.0,
        "beta_f32_bits": f32_bits(1.0),
        "authority": "current-net8-cp7-terminal-response-v3-kl-1.0",
        "schema": "mtg-kernel-current-net8-cp7-terminal-response-v3-kl-1.0-training/v1",
    },
    "beta-3.0": {
        "beta": 3.0,
        "beta_f32_bits": f32_bits(3.0),
        "authority": "current-net8-cp7-terminal-response-v3-kl-3.0",
        "schema": "mtg-kernel-current-net8-cp7-terminal-response-v3-kl-3.0-training/v1",
    },
    "beta-10.0": {
        "beta": 10.0,
        "beta_f32_bits": f32_bits(10.0),
        "authority": "current-net8-cp7-terminal-response-v3-kl-10.0",
        "schema": "mtg-kernel-current-net8-cp7-terminal-response-v3-kl-10.0-training/v1",
    },
}
ARM_ORDER = tuple(ARMS)


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


def _number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def _finite(value: Any) -> bool:
    return _number(value) and math.isfinite(float(value))


def _same_path(left: Path, right: Path) -> bool:
    return left.resolve(strict=False) == right.resolve(strict=False)


def _manifest_path(value: Any, manifest_path: Path, field: str) -> Path:
    if not isinstance(value, str) or not value:
        raise ValueError(f"manifest {field} is not a non-empty path")
    path = Path(value)
    if not path.is_absolute():
        path = manifest_path.parent / path
    return path.resolve(strict=False)


def validate_manifest(manifest_path: Path, report_paths: dict[str, Path]) -> dict[str, Any]:
    manifest = load_json(manifest_path)
    if manifest.get("schema") != MANIFEST_SCHEMA:
        raise ValueError("manifest schema mismatch")
    inputs = manifest.get("inputs")
    if not isinstance(inputs, dict):
        raise ValueError("manifest inputs are absent")
    expected_inputs = {
        "corpus_sha256": CORPUS_SHA256,
        "source_manifest_sha256": SOURCE_MANIFEST_SHA256,
        "source_payload_sha256": SOURCE_PAYLOAD_SHA256,
        "source_native_state_sha256": SOURCE_NATIVE_STATE_SHA256,
        "source_adam_step": SOURCE_ADAM_STEP,
    }
    if any(inputs.get(key) != value for key, value in expected_inputs.items()):
        raise ValueError("manifest source or corpus binding mismatch")

    common_training = manifest.get("common_training")
    if not isinstance(common_training, dict):
        raise ValueError("manifest common training is absent")
    expected_training = {
        "reward": "natural-terminal-win-draw-loss-only",
        "learning_rate_f32_bits": LEARNING_RATE_F32_BITS,
        "value_coefficient_f32_bits": VALUE_COEFFICIENT_F32_BITS,
        "epochs": 4,
        "ppo_clip_epsilon_f32_bits": PPO_CLIP_F32_BITS,
        "pi_old_adam_step": SOURCE_ADAM_STEP,
        "expected_ending_adam_step": ENDING_ADAM_STEP,
        "anchor_identity": ANCHOR_IDENTITY,
        "anchor_direction": ANCHOR_DIRECTION,
        "anchor_source": ANCHOR_SOURCE,
        "anchor_scope": ANCHOR_SCOPE,
    }
    if any(common_training.get(key) != value for key, value in expected_training.items()):
        raise ValueError("manifest common training binding mismatch")

    selection = manifest.get("selection")
    if not isinstance(selection, dict):
        raise ValueError("manifest selection is absent")
    if (
        selection.get("basis") != "movement-only"
        or selection.get("order")
        != "eligible-only-then-higher-mean-tv-then-higher-beta-on-exact-tie"
    ):
        raise ValueError("manifest selection binding mismatch")

    records = manifest.get("arms")
    if not isinstance(records, list) or len(records) != len(ARM_ORDER):
        raise ValueError("manifest must contain exactly four arms")
    normalized: dict[str, dict[str, Any]] = {}
    for record, name in zip(records, ARM_ORDER):
        contract = ARMS[name]
        if not isinstance(record, dict) or record.get("name") != name:
            raise ValueError("manifest arm order or name mismatch")
        if (
            record.get("beta_f32_bits") != contract["beta_f32_bits"]
            or record.get("authority_kind") != contract["authority"]
        ):
            raise ValueError(f"manifest {name} authority binding mismatch")
        report_path = _manifest_path(record.get("report_path"), manifest_path, f"{name}.report_path")
        if not _same_path(report_path, report_paths[name]):
            raise ValueError(f"manifest {name} report binding mismatch")
        package_value = record.get("package_root")
        if package_value is not None and not isinstance(package_value, str):
            raise ValueError(f"manifest {name} package binding is invalid")
        package_root = (
            None
            if package_value is None
            else _manifest_path(package_value, manifest_path, f"{name}.package_root")
        )
        normalized[name] = {
            "report_path": report_path,
            "package_root": package_root,
        }
    return {"arms": normalized}


def _validate_package(
    name: str,
    package_root: Path,
    report: dict[str, Any],
    report_sha256: str,
) -> str:
    candidate = report["candidate"]
    if not package_root.is_dir():
        raise ValueError(f"{name} passing package is absent")
    entries = sorted(package_root.iterdir(), key=lambda path: path.name)
    if [path.name for path in entries] != ["checkpoint.state.f32le", "fixed_native_state.json"]:
        raise ValueError(f"{name} package inventory mismatch")
    manifest_path = package_root / "fixed_native_state.json"
    payload_path = package_root / "checkpoint.state.f32le"
    package_manifest = load_json(manifest_path)
    payload = package_manifest.get("payload")
    if not isinstance(payload, dict):
        raise ValueError(f"{name} package payload manifest is absent")
    if (
        package_manifest.get("schema") != "mtg-kernel-xmage-fixed-native-state/v1"
        or package_manifest.get("authority_kind") != ARMS[name]["authority"]
        or package_manifest.get("source_result_sha256") != report_sha256
        or payload.get("filename") != "checkpoint.state.f32le"
        or payload.get("byte_count") != candidate.get("payload_byte_count")
        or payload.get("adam_step") != ENDING_ADAM_STEP
        or payload.get("scorer_bias_anchor_f32_bits")
        != candidate.get("scorer_bias_anchor_f32_bits")
        or payload.get("payload_sha256") != candidate.get("payload_sha256")
        or payload.get("parameters_sha256") != candidate.get("parameters_sha256")
        or payload.get("first_moments_sha256") != candidate.get("first_moments_sha256")
        or payload.get("second_moments_sha256") != candidate.get("second_moments_sha256")
        or payload.get("native_state_sha256") != candidate.get("native_state_sha256")
        or payload.get("model_parameter_sha256") != candidate.get("model_parameter_sha256")
        or payload_path.stat().st_size != candidate.get("payload_byte_count")
        or sha256(payload_path) != candidate.get("payload_sha256")
    ):
        raise ValueError(f"{name} package binding mismatch")
    return sha256(manifest_path)


def validate_arm(
    name: str,
    report_path: Path,
    package_root: Path | None,
) -> dict[str, Any]:
    contract = ARMS.get(name)
    if contract is None:
        raise ValueError(f"unknown V3 arm {name}")
    report = load_json(report_path)
    if report.get("schema") != contract["schema"]:
        raise ValueError(f"{name} report schema mismatch")
    source = report.get("source")
    if not isinstance(source, dict) or {
        "authority_kind": source.get("authority_kind"),
        "source_result_sha256": source.get("source_result_sha256"),
        "manifest_sha256": source.get("manifest_sha256"),
        "payload_sha256": source.get("payload_sha256"),
        "native_state_sha256": source.get("native_state_sha256"),
        "model_parameter_sha256": source.get("model_parameter_sha256"),
        "adam_step": source.get("adam_step"),
    } != {
        "authority_kind": SOURCE_AUTHORITY,
        "source_result_sha256": SOURCE_RESULT_SHA256,
        "manifest_sha256": SOURCE_MANIFEST_SHA256,
        "payload_sha256": SOURCE_PAYLOAD_SHA256,
        "native_state_sha256": SOURCE_NATIVE_STATE_SHA256,
        "model_parameter_sha256": SOURCE_MODEL_PARAMETER_SHA256,
        "adam_step": SOURCE_ADAM_STEP,
    }:
        raise ValueError(f"{name} source binding mismatch")

    corpus = report.get("corpus")
    if not isinstance(corpus, dict) or any(
        corpus.get(key) != value
        for key, value in {
            "sha256": CORPUS_SHA256,
            "pair_indices": CORPUS_PAIR_INDICES,
            "pair_count": CORPUS_PAIR_COUNT,
            "episode_count": CORPUS_EPISODE_COUNT,
            "decision_row_count": CORPUS_DECISION_ROW_COUNT,
            "physical_group_count": CORPUS_PHYSICAL_GROUP_COUNT,
            "terminal_return_counts_loss_draw_win": CORPUS_TERMINAL_RETURN_COUNTS,
        }.items()
    ):
        raise ValueError(f"{name} corpus binding mismatch")

    training = report.get("training")
    anchor = training.get("policy_anchor") if isinstance(training, dict) else None
    if not isinstance(training, dict) or not isinstance(anchor, dict):
        raise ValueError(f"{name} training recipe mismatch")
    if (
        training.get("reward") != "natural_terminal_win_draw_loss_only"
        or training.get("learning_rate_f32_bits") != LEARNING_RATE_F32_BITS
        or training.get("value_coefficient_f32_bits") != VALUE_COEFFICIENT_F32_BITS
        or training.get("ppo_clip_epsilon_f32_bits") != PPO_CLIP_F32_BITS
        or training.get("epochs") != 4
        or training.get("starting_adam_step") != SOURCE_ADAM_STEP
        or training.get("ending_adam_step") != ENDING_ADAM_STEP
        or training.get("advantage_transform", {}).get("identity")
        != "terminal_reinforce_frozen_source_value_standardized_by_candidate_seat_episode_balanced/v1"
        or anchor.get("identity") != ANCHOR_IDENTITY
        or anchor.get("direction") != ANCHOR_DIRECTION
        or anchor.get("old_policy_source") != ANCHOR_SOURCE
        or anchor.get("scope") != ANCHOR_SCOPE
        or anchor.get("coefficient_f32_bits") != contract["beta_f32_bits"]
        or not _finite(anchor.get("coefficient"))
        or f32_bits(float(anchor["coefficient"])) != contract["beta_f32_bits"]
    ):
        raise ValueError(f"{name} training recipe mismatch")

    candidate = report.get("candidate")
    movement = candidate.get("movement") if isinstance(candidate, dict) else None
    tail = candidate.get("tail_shape") if isinstance(candidate, dict) else None
    if not isinstance(candidate, dict) or not isinstance(movement, dict) or not isinstance(tail, dict):
        raise ValueError(f"{name} candidate movement is absent")
    finite_values = [
        candidate.get("parameter_l2_from_gae8"),
        movement.get("minimum_likelihood_ratio"),
        movement.get("maximum_likelihood_ratio"),
        movement.get("mean_likelihood_ratio"),
        movement.get("mean_absolute_log_likelihood_ratio"),
        movement.get("maximum_absolute_joint_log_likelihood_ratio"),
        movement.get("mean_old_to_current_forward_kl"),
        movement.get("mean_action_total_variation"),
        movement.get("p90_action_total_variation_nearest_rank"),
        movement.get("mean_policy_surrogate"),
        tail.get("p99_action_total_variation_nearest_rank"),
        tail.get("maximum_action_total_variation"),
        tail.get("p99_absolute_joint_log_likelihood_ratio_nearest_rank"),
    ]
    finite = all(_finite(value) for value in finite_values)
    if not finite:
        raise ValueError(f"{name} movement fields are absent or non-finite")
    if not isinstance(tail.get("above_absolute_joint_log_ratio_1_count"), int) or isinstance(
        tail.get("above_absolute_joint_log_ratio_1_count"), bool
    ):
        raise ValueError(f"{name} tail count is absent")
    parameter_l2 = float(candidate["parameter_l2_from_gae8"])
    mean_tv = float(movement["mean_action_total_variation"])
    p90_tv = float(movement["p90_action_total_variation_nearest_rank"])
    p99_tv = float(tail["p99_action_total_variation_nearest_rank"])
    p99_log_ratio = float(tail["p99_absolute_joint_log_likelihood_ratio_nearest_rank"])
    max_log_ratio = float(movement["maximum_absolute_joint_log_likelihood_ratio"])
    above_one_count = tail["above_absolute_joint_log_ratio_1_count"]
    recomputed_pass = (
        finite
        and candidate.get("adam_step") == ENDING_ADAM_STEP
        and parameter_l2 <= PARAMETER_L2_CAP
        and MEAN_TV_FLOOR <= mean_tv <= MEAN_TV_CAP
        and p90_tv <= P90_ROW_TV_CAP
        and p99_tv <= P99_ROW_TV_CAP
        and p99_log_ratio <= P99_GROUP_LOG_RATIO_CAP
        and max_log_ratio <= MAX_GROUP_LOG_RATIO_CAP
        and above_one_count == ABOVE_ONE_COUNT_CAP
    )
    duplicated_tail = {
        "p99_action_total_variation": candidate.get("p99_action_total_variation"),
        "maximum_action_total_variation": candidate.get("maximum_action_total_variation"),
        "p99_absolute_joint_log_likelihood_ratio": candidate.get(
            "p99_absolute_joint_log_likelihood_ratio"
        ),
        "above_absolute_joint_log_ratio_1_count": candidate.get(
            "above_absolute_joint_log_ratio_1_count"
        ),
    }
    if duplicated_tail != {
        "p99_action_total_variation": tail["p99_action_total_variation_nearest_rank"],
        "maximum_action_total_variation": tail["maximum_action_total_variation"],
        "p99_absolute_joint_log_likelihood_ratio": tail[
            "p99_absolute_joint_log_likelihood_ratio_nearest_rank"
        ],
        "above_absolute_joint_log_ratio_1_count": above_one_count,
    }:
        raise ValueError(f"{name} duplicated tail metrics mismatch")
    gate = report.get("publication_gate")
    if not isinstance(gate, dict) or (
        gate.get("finite") != finite
        or gate.get("parameter_l2_cap") != PARAMETER_L2_CAP
        or gate.get("mean_action_total_variation_floor") != MEAN_TV_FLOOR
        or gate.get("mean_action_total_variation_cap") != MEAN_TV_CAP
        or gate.get("p90_action_total_variation_cap") != P90_ROW_TV_CAP
        or gate.get("p99_action_total_variation_cap") != P99_ROW_TV_CAP
        or gate.get("p99_absolute_joint_log_likelihood_ratio_cap") != P99_GROUP_LOG_RATIO_CAP
        or gate.get("maximum_absolute_joint_log_likelihood_ratio_cap") != MAX_GROUP_LOG_RATIO_CAP
        or gate.get("above_absolute_joint_log_ratio_1_count_cap") != ABOVE_ONE_COUNT_CAP
        or gate.get("v3_tail_caps_required") is not True
        or gate.get("pass") != recomputed_pass
    ):
        raise ValueError(f"{name} publication gate mismatch")

    report_sha256 = sha256(report_path)
    if recomputed_pass:
        if package_root is None:
            raise ValueError(f"{name} passing package binding is absent")
        manifest_sha256 = _validate_package(name, package_root, report, report_sha256)
    else:
        if package_root is not None and package_root.exists():
            raise ValueError(f"{name} failing arm unexpectedly has a package")
        manifest_sha256 = None

    return {
        "arm": name,
        "beta": contract["beta"],
        "authority_kind": contract["authority"],
        "eligible": recomputed_pass,
        "report_path": str(report_path),
        "report_sha256": report_sha256,
        "package_root": None if package_root is None else str(package_root),
        "manifest_sha256": manifest_sha256,
        "parameter_l2_from_gae8": parameter_l2,
        "mean_action_total_variation": mean_tv,
        "p90_action_total_variation": p90_tv,
        "p99_action_total_variation": p99_tv,
        "p99_absolute_joint_log_likelihood_ratio": p99_log_ratio,
        "maximum_absolute_joint_log_likelihood_ratio": max_log_ratio,
        "above_absolute_joint_log_ratio_1_count": above_one_count,
    }


def select(*arms: dict[str, Any] | list[dict[str, Any]]) -> str | None:
    if len(arms) == 1 and isinstance(arms[0], list):
        arms = tuple(arms[0])
    if [arm.get("arm") for arm in arms] != list(ARM_ORDER):
        raise ValueError("selection requires exactly four arms in fixed order")
    eligible = [arm for arm in arms if arm["eligible"]]
    if not eligible:
        return None
    return max(
        eligible,
        key=lambda arm: (float(arm["mean_action_total_variation"]), float(arm["beta"])),
    )["arm"]


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


def _package_argument(manifest_value: Path | None, cli_value: Path | None, name: str) -> Path | None:
    if manifest_value is None:
        if cli_value is not None:
            raise ValueError(f"{name} package binding is not authorized by manifest")
        return None
    if cli_value is not None and not _same_path(manifest_value, cli_value):
        raise ValueError(f"{name} package binding mismatch")
    return manifest_value


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, required=True)
    for name, cli_name in (
        ("beta-0.3", "beta-0-3"),
        ("beta-1.0", "beta-1"),
        ("beta-3.0", "beta-3"),
        ("beta-10.0", "beta-10"),
    ):
        parser.add_argument(f"--{cli_name}-report", f"--{name}-report", dest=f"{cli_name}_report", type=Path, required=True)
        parser.add_argument(f"--{cli_name}-package", f"--{name}-package", dest=f"{cli_name}_package", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    return parser


def main() -> int:
    args = _parser().parse_args()
    report_paths = {
        "beta-0.3": args.beta_0_3_report,
        "beta-1.0": args.beta_1_report,
        "beta-3.0": args.beta_3_report,
        "beta-10.0": args.beta_10_report,
    }
    manifest_info = validate_manifest(args.manifest, report_paths)
    cli_packages = {
        "beta-0.3": args.beta_0_3_package,
        "beta-1.0": args.beta_1_package,
        "beta-3.0": args.beta_3_package,
        "beta-10.0": args.beta_10_package,
    }
    arms = []
    for name in ARM_ORDER:
        package_root = _package_argument(
            manifest_info["arms"][name]["package_root"], cli_packages[name], name
        )
        arms.append(validate_arm(name, report_paths[name], package_root))
    selected = select(arms)
    result = {
        "schema": SELECTION_SCHEMA,
        "manifest_path": str(args.manifest),
        "selection_basis": "movement_only_no_fresh_gameplay_no_outcome_selection",
        "arms": arms,
        "selected_arm": selected,
        "advance_to_downstream_gates": selected is not None,
        "tie_break": "higher_mean_tv_then_higher_beta_on_exact_tie",
        "nonclaims": [
            "movement-only selection is not playing-strength evidence",
            "natural terminal win draw or loss remains the only downstream promotion measure",
        ],
    }
    result_sha256 = write_new_json(args.output, result)
    print(
        json.dumps(
            {
                "schema": SELECTION_SUMMARY_SCHEMA,
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
