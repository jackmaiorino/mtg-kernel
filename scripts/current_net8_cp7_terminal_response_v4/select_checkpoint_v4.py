#!/usr/bin/env python3
"""Select and authorize the single global V4 checkpoint.

The two Rust arm reports are trusted for movement metric computation. This
selector independently validates their fixed recipe, re-applies every gate,
checks each staging payload, and applies the frozen cross-arm ordering. Arm
packages use non-runnable candidate authorities. Only this selector writes a
package carrying a terminal-gate authority.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import shutil
import struct
from pathlib import Path
from typing import Any


MANIFEST_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-screen-manifest/v1"
SELECTION_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-selection/v1"
SUMMARY_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-selection-summary/v1"
PACKAGE_SCHEMA = "mtg-kernel-xmage-fixed-native-state/v1"
PAYLOAD_FILENAME = "checkpoint.state.f32le"
PACKAGE_MANIFEST_FILENAME = "fixed_native_state.json"

SOURCE = {
    "authority_kind": "current-net8-gae8-v1",
    "source_result_sha256": "dc581e2bd6548c5fa9c05d9d8ab70a2bd6f213b20dd82a042109ee9a500b628e",
    "manifest_sha256": "d86a430298a5a91be324e22d798c7cdc7f9f5e61e7ffa255a559ff68287e5ab1",
    "payload_sha256": "a0b7752181a562f8e5a0821a490ce20b777b509855d754283536e8242f489b98",
    "native_state_sha256": "ab7dd25ca6619a4a613ca089e1eb8e75981f8e5cfc0bae8535b78cddd7efa952",
    "model_parameter_sha256": "5efe2f167045bde379da3be8af6c480b6702f5d7a849ff8435d8ac6b1d91daa8",
    "adam_step": 520,
}
CORPUS_BASE_SEED_HEX = "00000000001d7311"
CORPUS_PAIR_INDICES = list(range(256))
CORPUS_PAIR_COUNT = 256
CORPUS_EPISODE_COUNT = 512
LEARNING_RATES = [0.0005, 0.0004, 0.0003, 0.0002, 0.00015, 0.0001, 0.000075, 0.00005]
CHECKPOINT_UPDATES = [2, 4, 6, 8]
OBJECTIVE = "ppo_clip_frozen_source_value_standardized_episode_balanced/v1"
ADVANTAGE_IDENTITY = (
    "terminal_reinforce_frozen_source_value_standardized_by_candidate_seat_"
    "episode_balanced/v1"
)
ANCHOR_IDENTITY = "old-policy-forward-kl-every-legal-action-distribution/v1"
ANCHOR_SOURCE = "corpus_old_policy_logits_f32_bits"
PPO_CLIP = 0.10

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
        "report_schema": "mtg-kernel-current-net8-cp7-terminal-response-v4-kl-0.3-training/v1",
        "candidate_authority": "current-net8-cp7-terminal-response-v4-kl-0.3-arm-candidate",
        "final_authority": "current-net8-cp7-terminal-response-v4-kl-0.3",
    },
    "beta-1.0": {
        "beta": 1.0,
        "beta_f32_bits": f32_bits(1.0),
        "report_schema": "mtg-kernel-current-net8-cp7-terminal-response-v4-kl-1.0-training/v1",
        "candidate_authority": "current-net8-cp7-terminal-response-v4-kl-1.0-arm-candidate",
        "final_authority": "current-net8-cp7-terminal-response-v4-kl-1.0",
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


def _bound_path(value: Any, manifest_path: Path, field: str) -> Path:
    if not isinstance(value, str) or not value:
        raise ValueError(f"manifest {field} is not a non-empty path")
    path = Path(value)
    if not path.is_absolute():
        path = manifest_path.parent / path
    return path.resolve(strict=False)


def validate_manifest(
    manifest_path: Path,
    report_paths: dict[str, Path],
    candidate_roots: dict[str, Path],
    selection_report_path: Path,
    final_package_root: Path,
) -> dict[str, Any]:
    manifest = load_json(manifest_path)
    if manifest.get("schema") != MANIFEST_SCHEMA:
        raise ValueError("screen manifest schema mismatch")
    if not isinstance(manifest.get("git_commit"), str) or len(manifest["git_commit"]) != 40:
        raise ValueError("screen manifest git commit is absent")
    toolchain = manifest.get("toolchain")
    if not isinstance(toolchain, dict) or not all(
        isinstance(toolchain.get(field), str) and toolchain[field]
        for field in ("rustc", "cargo", "linker")
    ):
        raise ValueError("screen manifest toolchain is incomplete")

    inputs = manifest.get("inputs")
    if not isinstance(inputs, dict) or inputs.get("source") != SOURCE:
        raise ValueError("screen manifest source binding mismatch")
    corpus = inputs.get("corpus") if isinstance(inputs, dict) else None
    expected_static_corpus = {
        "base_seed_u64_hex": CORPUS_BASE_SEED_HEX,
        "pair_indices": CORPUS_PAIR_INDICES,
        "pair_count": CORPUS_PAIR_COUNT,
        "episode_count": CORPUS_EPISODE_COUNT,
    }
    if not isinstance(corpus, dict) or any(
        corpus.get(field) != value for field, value in expected_static_corpus.items()
    ):
        raise ValueError("screen manifest corpus panel mismatch")
    for field in ("sha256", "path"):
        if not isinstance(corpus.get(field), str) or not corpus[field]:
            raise ValueError(f"screen manifest corpus {field} is absent")
    for field in ("decision_row_count", "physical_group_count"):
        if not isinstance(corpus.get(field), int) or isinstance(corpus[field], bool) or corpus[field] <= 0:
            raise ValueError(f"screen manifest corpus {field} is invalid")
    counts = corpus.get("terminal_return_counts_loss_draw_win")
    if (
        not isinstance(counts, list)
        or len(counts) != 3
        or any(not isinstance(value, int) or isinstance(value, bool) or value < 0 for value in counts)
        or sum(counts) != CORPUS_EPISODE_COUNT
    ):
        raise ValueError("screen manifest terminal counts are invalid")

    training = manifest.get("common_training")
    expected_training = {
        "reward": "natural_terminal_win_draw_loss_only",
        "objective": OBJECTIVE,
        "value_coefficient_f32_bits": f32_bits(0.0),
        "ppo_clip_epsilon_f32_bits": f32_bits(PPO_CLIP),
        "learning_rate_schedule_f32_bits": [f32_bits(value) for value in LEARNING_RATES],
        "checkpoint_updates": CHECKPOINT_UPDATES,
        "optimizer_starting_adam_step": 0,
        "policy_anchor_identity": ANCHOR_IDENTITY,
        "policy_anchor_source": ANCHOR_SOURCE,
    }
    if not isinstance(training, dict) or any(
        training.get(field) != value for field, value in expected_training.items()
    ):
        raise ValueError("screen manifest training recipe mismatch")

    selection = manifest.get("selection")
    if selection != {
        "basis": "movement-only",
        "order": "eligible-only-then-higher-mean-tv-then-higher-beta-then-earlier-update",
        "trusted_metrics": "rust-arm-reports",
        "gate_decision_recomputed": True,
    }:
        raise ValueError("screen manifest selection contract mismatch")

    records = manifest.get("arms")
    if not isinstance(records, list) or len(records) != 2:
        raise ValueError("screen manifest must contain exactly two arms")
    for record, name in zip(records, ARM_ORDER):
        if not isinstance(record, dict) or record.get("name") != name:
            raise ValueError("screen manifest arm order mismatch")
        if record.get("beta_f32_bits") != ARMS[name]["beta_f32_bits"]:
            raise ValueError(f"screen manifest {name} beta mismatch")
        if not _same_path(_bound_path(record.get("report_path"), manifest_path, "report"), report_paths[name]):
            raise ValueError(f"screen manifest {name} report path mismatch")
        if not _same_path(
            _bound_path(record.get("candidate_package_root"), manifest_path, "candidate package"),
            candidate_roots[name],
        ):
            raise ValueError(f"screen manifest {name} candidate package path mismatch")

    outputs = manifest.get("outputs")
    if not isinstance(outputs, dict):
        raise ValueError("screen manifest outputs are absent")
    if not _same_path(
        _bound_path(outputs.get("selection_report_path"), manifest_path, "selection report"),
        selection_report_path,
    ) or not _same_path(
        _bound_path(outputs.get("final_package_root"), manifest_path, "final package"),
        final_package_root,
    ):
        raise ValueError("screen manifest output binding mismatch")
    return {"manifest": manifest, "corpus": corpus}


def _checkpoint_gate(checkpoint: dict[str, Any]) -> tuple[bool, bool, float]:
    movement = checkpoint.get("movement")
    tail = checkpoint.get("tail_shape")
    worst_group = tail.get("worst_group") if isinstance(tail, dict) else None
    if (
        not isinstance(movement, dict)
        or not isinstance(tail, dict)
        or not isinstance(worst_group, dict)
    ):
        raise ValueError("checkpoint movement metrics are absent")
    values = [
        checkpoint.get("parameter_l2_from_gae8"),
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
        worst_group.get("signed_joint_log_likelihood_ratio"),
        worst_group.get("absolute_joint_log_likelihood_ratio"),
        worst_group.get("likelihood_ratio"),
    ]
    finite = all(_finite(value) for value in values)
    count = tail.get("above_absolute_joint_log_ratio_1_count")
    if not isinstance(count, int) or isinstance(count, bool):
        raise ValueError("checkpoint tail count is absent")
    mean_tv = float(movement["mean_action_total_variation"])
    passed = (
        finite
        and checkpoint.get("adam_step") == checkpoint.get("update")
        and float(checkpoint["parameter_l2_from_gae8"]) <= PARAMETER_L2_CAP
        and MEAN_TV_FLOOR <= mean_tv <= MEAN_TV_CAP
        and float(movement["p90_action_total_variation_nearest_rank"]) <= P90_ROW_TV_CAP
        and float(tail["p99_action_total_variation_nearest_rank"]) <= P99_ROW_TV_CAP
        and float(tail["p99_absolute_joint_log_likelihood_ratio_nearest_rank"])
        <= P99_GROUP_LOG_RATIO_CAP
        and float(movement["maximum_absolute_joint_log_likelihood_ratio"])
        <= MAX_GROUP_LOG_RATIO_CAP
        and count == ABOVE_ONE_COUNT_CAP
    )
    return finite, passed, mean_tv


def _validate_candidate_package(
    name: str,
    package_root: Path,
    report_sha256: str,
    candidate: dict[str, Any],
) -> dict[str, str]:
    if not package_root.is_dir():
        raise ValueError(f"{name} eligible arm candidate package is absent")
    entries = sorted(path.name for path in package_root.iterdir())
    if entries != [PAYLOAD_FILENAME, PACKAGE_MANIFEST_FILENAME]:
        raise ValueError(f"{name} candidate package inventory mismatch")
    manifest_path = package_root / PACKAGE_MANIFEST_FILENAME
    payload_path = package_root / PAYLOAD_FILENAME
    manifest = load_json(manifest_path)
    payload = manifest.get("payload")
    expected_payload = {
        "filename": PAYLOAD_FILENAME,
        "byte_count": candidate.get("payload_byte_count"),
        "adam_step": candidate.get("adam_step"),
        "scorer_bias_anchor_f32_bits": candidate.get("scorer_bias_anchor_f32_bits"),
        "payload_sha256": candidate.get("payload_sha256"),
        "parameters_sha256": candidate.get("parameters_sha256"),
        "first_moments_sha256": candidate.get("first_moments_sha256"),
        "second_moments_sha256": candidate.get("second_moments_sha256"),
        "model_parameter_sha256": candidate.get("model_parameter_sha256"),
        "native_state_sha256": candidate.get("native_state_sha256"),
    }
    if (
        manifest.get("schema") != PACKAGE_SCHEMA
        or manifest.get("authority_kind") != ARMS[name]["candidate_authority"]
        or manifest.get("source_result_sha256") != report_sha256
        or payload != expected_payload
        or payload_path.stat().st_size != candidate.get("payload_byte_count")
        or sha256(payload_path) != candidate.get("payload_sha256")
    ):
        raise ValueError(f"{name} candidate package binding mismatch")
    return {
        "manifest_sha256": sha256(manifest_path),
        "payload_path": str(payload_path.resolve()),
    }


def validate_arm(
    name: str,
    report_path: Path,
    candidate_root: Path,
    corpus_contract: dict[str, Any],
) -> dict[str, Any]:
    contract = ARMS[name]
    report = load_json(report_path)
    if report.get("schema") != contract["report_schema"] or report.get("source") != SOURCE:
        raise ValueError(f"{name} report source or schema mismatch")
    report_corpus = report.get("corpus")
    corpus_fields = (
        "sha256",
        "base_seed_u64_hex",
        "pair_indices",
        "pair_count",
        "episode_count",
        "decision_row_count",
        "physical_group_count",
        "terminal_return_counts_loss_draw_win",
    )
    if not isinstance(report_corpus, dict) or any(
        report_corpus.get(field) != corpus_contract.get(field) for field in corpus_fields
    ):
        raise ValueError(f"{name} corpus binding mismatch")

    training = report.get("training")
    anchor = training.get("policy_anchor") if isinstance(training, dict) else None
    reset = training.get("optimizer_reset") if isinstance(training, dict) else None
    if not isinstance(training, dict) or not isinstance(anchor, dict) or not isinstance(reset, dict):
        raise ValueError(f"{name} training recipe is absent")
    if (
        training.get("reward") != "natural_terminal_win_draw_loss_only"
        or training.get("objective") != OBJECTIVE
        or training.get("value_coefficient_f32_bits") != f32_bits(0.0)
        or training.get("ppo_clip_epsilon_f32_bits") != f32_bits(PPO_CLIP)
        or training.get("learning_rate_schedule_f32_bits")
        != [f32_bits(value) for value in LEARNING_RATES]
        or training.get("checkpoint_updates") != CHECKPOINT_UPDATES
        or training.get("advantage_transform", {}).get("identity") != ADVANTAGE_IDENTITY
        or reset.get("source_adam_step") != SOURCE["adam_step"]
        or reset.get("starting_adam_step") != 0
        or reset.get("parameters_preserved_bit_exact") is not True
        or reset.get("first_and_second_moments_all_positive_zero") is not True
        or anchor.get("identity") != ANCHOR_IDENTITY
        or anchor.get("direction") != "old_to_current"
        or anchor.get("old_policy_source") != ANCHOR_SOURCE
        or anchor.get("scope") != "every_legal_action_distribution"
        or anchor.get("coefficient_f32_bits") != contract["beta_f32_bits"]
        or not _finite(anchor.get("coefficient"))
        or f32_bits(float(anchor["coefficient"])) != contract["beta_f32_bits"]
    ):
        raise ValueError(f"{name} training recipe mismatch")
    updates = training.get("updates")
    if not isinstance(updates, list) or len(updates) != 8:
        raise ValueError(f"{name} update schedule is incomplete")
    for expected_update, (update, learning_rate) in enumerate(zip(updates, LEARNING_RATES), 1):
        if (
            not isinstance(update, dict)
            or update.get("update") != expected_update
            or update.get("learning_rate_f32_bits") != f32_bits(learning_rate)
            or update.get("adam_step_before") != expected_update - 1
            or update.get("adam_step_after") != expected_update
        ):
            raise ValueError(f"{name} update schedule mismatch")

    checkpoints = report.get("checkpoints")
    if not isinstance(checkpoints, list) or [row.get("update") for row in checkpoints] != CHECKPOINT_UPDATES:
        raise ValueError(f"{name} checkpoint schedule mismatch")
    eligible: list[dict[str, Any]] = []
    normalized_checkpoints = []
    for checkpoint in checkpoints:
        finite, passed, mean_tv = _checkpoint_gate(checkpoint)
        gate = checkpoint.get("gate")
        if not isinstance(gate, dict) or gate != {"finite": finite, "pass": passed}:
            raise ValueError(f"{name} checkpoint gate disagreement")
        normalized = {
            "update": checkpoint["update"],
            "eligible": passed,
            "mean_action_total_variation": mean_tv,
            "parameter_l2_from_gae8": checkpoint["parameter_l2_from_gae8"],
            "movement": checkpoint["movement"],
            "tail_shape": checkpoint["tail_shape"],
        }
        normalized_checkpoints.append(normalized)
        if passed:
            eligible.append(normalized)
    expected_candidate = (
        None
        if not eligible
        else max(eligible, key=lambda row: (row["mean_action_total_variation"], -row["update"]))
    )
    candidate = report.get("candidate")
    if expected_candidate is None:
        if candidate is not None or report.get("arm_eligibility_pass") is not False:
            raise ValueError(f"{name} ineligible arm candidate mismatch")
        if candidate_root.exists():
            raise ValueError(f"{name} ineligible arm unexpectedly has a candidate package")
        package = None
    else:
        if not isinstance(candidate, dict) or report.get("arm_eligibility_pass") is not True:
            raise ValueError(f"{name} eligible arm candidate is absent")
        if (
            candidate.get("selected_update") != expected_candidate["update"]
            or candidate.get("adam_step") != expected_candidate["update"]
            or candidate.get("parameter_l2_from_gae8")
            != expected_candidate["parameter_l2_from_gae8"]
            or candidate.get("movement") != expected_candidate["movement"]
            or candidate.get("tail_shape") != expected_candidate["tail_shape"]
        ):
            raise ValueError(f"{name} local checkpoint selection mismatch")
        package = _validate_candidate_package(name, candidate_root, sha256(report_path), candidate)

    return {
        "arm": name,
        "beta": contract["beta"],
        "beta_f32_bits": contract["beta_f32_bits"],
        "report_path": str(report_path.resolve()),
        "report_sha256": sha256(report_path),
        "candidate_package_root": str(candidate_root.resolve()),
        "candidate_package": package,
        "checkpoints": normalized_checkpoints,
        "candidate": candidate,
        "eligible": expected_candidate is not None,
        "selected_update": None if expected_candidate is None else expected_candidate["update"],
        "mean_action_total_variation": (
            None if expected_candidate is None else expected_candidate["mean_action_total_variation"]
        ),
    }


def select(arms: list[dict[str, Any]]) -> str | None:
    if [arm.get("arm") for arm in arms] != list(ARM_ORDER):
        raise ValueError("selection requires exactly two arms in fixed order")
    eligible = [arm for arm in arms if arm["eligible"]]
    if not eligible:
        return None
    return max(
        eligible,
        key=lambda arm: (
            float(arm["mean_action_total_variation"]),
            float(arm["beta"]),
            -int(arm["selected_update"]),
        ),
    )["arm"]


def write_new_json(path: Path, value: dict[str, Any]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    data = (json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n").encode("utf-8")
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_BINARY", 0)
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


def authorize(
    manifest_path: Path,
    report_paths: dict[str, Path],
    candidate_roots: dict[str, Path],
    selection_report_path: Path,
    final_package_root: Path,
) -> dict[str, Any]:
    if selection_report_path.exists() or final_package_root.exists():
        raise FileExistsError("selection output already exists")
    validated_manifest = validate_manifest(
        manifest_path,
        report_paths,
        candidate_roots,
        selection_report_path,
        final_package_root,
    )
    arms = [
        validate_arm(name, report_paths[name], candidate_roots[name], validated_manifest["corpus"])
        for name in ARM_ORDER
    ]
    selected_name = select(arms)
    selected = None if selected_name is None else next(arm for arm in arms if arm["arm"] == selected_name)
    selection_report = {
        "schema": SELECTION_SCHEMA,
        "status": "stop" if selected is None else "selected",
        "screen_manifest": {
            "path": str(manifest_path.resolve()),
            "sha256": sha256(manifest_path),
        },
        "selection_rule": {
            "basis": "movement-only",
            "order": "eligible-only-then-higher-mean-tv-then-higher-beta-then-earlier-update",
            "trusted_metrics": "rust-arm-reports",
            "gate_decision_recomputed": True,
        },
        "arms": arms,
        "selected": (
            None
            if selected is None
            else {
                "arm": selected["arm"],
                "beta": selected["beta"],
                "update": selected["selected_update"],
                "mean_action_total_variation": selected["mean_action_total_variation"],
                "final_authority_kind": ARMS[selected["arm"]]["final_authority"],
                "final_package_root": str(final_package_root.resolve()),
                "payload_sha256": selected["candidate"]["payload_sha256"],
            }
        ),
        "publication_pass": selected is not None,
        "nonclaims": [
            "development training metrics are not playing-strength evidence",
            "terminal win/loss/draw remains the only promotion measure",
        ],
    }
    selection_sha = write_new_json(selection_report_path, selection_report)
    final_manifest_sha = None
    if selected is not None:
        final_package_root.mkdir()
        source_payload = Path(selected["candidate_package"]["payload_path"])
        final_payload = final_package_root / PAYLOAD_FILENAME
        with source_payload.open("rb") as source, final_payload.open("xb") as destination:
            shutil.copyfileobj(source, destination, length=1024 * 1024)
            destination.flush()
            os.fsync(destination.fileno())
        if sha256(final_payload) != selected["candidate"]["payload_sha256"]:
            raise ValueError("final selected payload copy mismatch")
        final_manifest = {
            "schema": PACKAGE_SCHEMA,
            "authority_kind": ARMS[selected["arm"]]["final_authority"],
            "source_result_sha256": selection_sha,
            "payload": {
                "filename": PAYLOAD_FILENAME,
                "byte_count": selected["candidate"]["payload_byte_count"],
                "adam_step": selected["candidate"]["adam_step"],
                "scorer_bias_anchor_f32_bits": selected["candidate"]["scorer_bias_anchor_f32_bits"],
                "payload_sha256": selected["candidate"]["payload_sha256"],
                "parameters_sha256": selected["candidate"]["parameters_sha256"],
                "first_moments_sha256": selected["candidate"]["first_moments_sha256"],
                "second_moments_sha256": selected["candidate"]["second_moments_sha256"],
                "model_parameter_sha256": selected["candidate"]["model_parameter_sha256"],
                "native_state_sha256": selected["candidate"]["native_state_sha256"],
            },
            "non_claims": [
                "external software anchor is not professional-level evidence",
                "terminal win/loss/draw is the only playing-strength outcome",
            ],
        }
        final_manifest_sha = write_new_json(
            final_package_root / PACKAGE_MANIFEST_FILENAME,
            final_manifest,
        )
    return {
        "schema": SUMMARY_SCHEMA,
        "publication_pass": selected is not None,
        "selected_arm": selected_name,
        "selected_update": None if selected is None else selected["selected_update"],
        "selection_report_sha256": selection_sha,
        "final_manifest_sha256": final_manifest_sha,
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--beta-0.3-report", dest="beta_03_report", type=Path, required=True)
    parser.add_argument("--beta-0.3-candidate", dest="beta_03_candidate", type=Path, required=True)
    parser.add_argument("--beta-1.0-report", dest="beta_10_report", type=Path, required=True)
    parser.add_argument("--beta-1.0-candidate", dest="beta_10_candidate", type=Path, required=True)
    parser.add_argument("--selection-report", type=Path, required=True)
    parser.add_argument("--final-package-root", type=Path, required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    report_paths = {
        "beta-0.3": args.beta_03_report.resolve(strict=True),
        "beta-1.0": args.beta_10_report.resolve(strict=True),
    }
    candidate_roots = {
        "beta-0.3": args.beta_03_candidate.resolve(strict=False),
        "beta-1.0": args.beta_10_candidate.resolve(strict=False),
    }
    summary = authorize(
        args.manifest.resolve(strict=True),
        report_paths,
        candidate_roots,
        args.selection_report.resolve(),
        args.final_package_root.resolve(),
    )
    print(json.dumps(summary, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"select_checkpoint_v4: ERROR: {error}", file=os.sys.stderr)
        raise SystemExit(2)
