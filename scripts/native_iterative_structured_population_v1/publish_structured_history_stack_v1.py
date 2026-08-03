#!/usr/bin/env python3
"""Publish one strict additive structured-history population stage."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import math
import shutil
import struct
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np
import torch


SCRIPT_DIR = Path(__file__).resolve().parent
ADAPTER_DIR = SCRIPT_DIR.parent / "structured_adapter_screen_v1"
if str(ADAPTER_DIR) not in sys.path:
    sys.path.insert(0, str(ADAPTER_DIR))

import fit_complete_history_live_candidate_v1 as history_publish
import fit_policy_live_candidate as live
import run_scaled_history_outcome_policy_v1 as scaled
import run_screen as screen


STACK_SCHEMA = "mtg-kernel-structured-history-stack-candidate/v1"
PUBLICATION_REPORT_SCHEMA = STACK_SCHEMA + ".publication-report"
PUBLICATION_ENCODING = "json-pretty-sorted-utf8-trailing-lf/v1"
WEIGHTS_ENCODING = "ordered-row-major-finite-f32-little-endian/v1"
STACK_ARCHITECTURE_IDENTITY = (
    "complete-public-history-structured-object-action-attention-policy-value-"
    "additive-stack/v1"
)
MEMBER_ARCHITECTURE_IDENTITY = (
    "complete-public-history-structured-object-action-attention-policy-value-"
    "residual/v1"
)
FOLD_ARCHITECTURE_IDENTITY = (
    "complete-public-history-structured-outcome-policy-residual/v1"
)
FOLD_VALUE_MODEL = "exact-retained-parent-unchanged"
VALUE_MODEL = "joint-terminal-residual/v1"
STAGE_WEIGHTING = "equal-average/v1"
STACK_FILENAME = "structured_history_stack.json"
PARENT_DIRECTORY = "parent"
WEIGHTS_DIRECTORY = "weights"
STAGE_MEMBER_COUNT = 4
PARAMETER_COUNT = 107_378
MEMBER_BYTE_COUNT = PARAMETER_COUNT * 4

STATE_DIM = 219
OBJECT_DIM = 98
EDGE_DIM = 41
ACTION_DIM = 195
REF_DIM = 25
HIDDEN_DIM = 48
CARD_VOCAB = 136
CARD_EMBEDDING_DIM = 24
GROUP_VOCAB = 12
GROUP_EMBEDDING_DIM = 16
HISTORY_LENGTH = 16
HISTORY_FEATURE_DIM = 237
HISTORY_ROLE_DIM = 2

PARAMETER_LAYOUT_DOMAIN = (
    b"mtg-kernel-structured-history-residual-parameter-layout/v1"
)
COMPOSITE_DOMAIN = b"mtg-kernel-structured-history-stack-composite-model/v1"

EXPECTED_PARENT = {
    "manifest_sha256": live.PARENT_MANIFEST_SHA256,
    "payload_sha256": live.PARENT_PAYLOAD_SHA256,
    "native_state_sha256": live.PARENT_NATIVE_STATE_SHA256,
    "model_parameter_sha256": live.PARENT_MODEL_PARAMETER_SHA256,
    "adam_step": live.PARENT_ADAM_STEP,
}


@dataclass(frozen=True)
class ParameterLayout:
    name: str
    shape: tuple[int, ...]
    count_f32: int


@dataclass(frozen=True)
class LoadedFold:
    fold: int
    result_path: Path
    result_sha256: str
    state_path: Path
    state_sha256: str
    heldout_surrogate: float
    calibrated_scale: float
    parameter_count: int
    payload: bytes
    payload_sha256: str
    parameter_bindings: list[dict[str, Any]]


@dataclass(frozen=True)
class PriorStack:
    root: Path
    manifest_sha256: str
    stages: list[dict[str, Any]]
    copied_file_sha256s: dict[str, str]


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _json_bytes(value: Any) -> bytes:
    return (
        json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + "\n"
    ).encode("utf-8")


def _write_new_json(path: Path, value: Any) -> str:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(_json_bytes(value))
    return _sha256(path)


def _write_new_bytes(path: Path, payload: bytes) -> str:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)
    return _sha256(path)


def _copy_file_new(source: Path, destination: Path) -> str:
    if destination.exists():
        _fail(f"refusing to overwrite {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    return _sha256(destination)


def _copy_tree_new(source: Path, destination: Path) -> None:
    if destination.exists():
        _fail(f"refusing to overwrite {destination}")
    shutil.copytree(source, destination)


def _reject_json_constant(value: str) -> None:
    _fail(f"JSON contains non-finite constant {value}")


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            _fail(f"JSON contains duplicate key {key!r}")
        result[key] = value
    return result


def _read_json(path: Path) -> dict[str, Any]:
    value = json.loads(
        path.read_text(encoding="utf-8"),
        object_pairs_hook=_reject_duplicate_keys,
        parse_constant=_reject_json_constant,
    )
    if not isinstance(value, dict):
        _fail(f"JSON root must be an object: {path}")
    return value


def _expect_exact_keys(value: Any, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        _fail(f"{label} fields are not exact")
    return value


def _ensure_lower_sha256(value: Any, label: str) -> str:
    if (
        not isinstance(value, str)
        or len(value) != 64
        or value != value.lower()
        or any(character not in "0123456789abcdef" for character in value)
    ):
        _fail(f"{label} must be a 64-character lowercase SHA-256 hex string")
    return value


def _ensure_lower_commit(value: Any, label: str) -> str:
    if (
        not isinstance(value, str)
        or len(value) != 40
        or value != value.lower()
        or any(character not in "0123456789abcdef" for character in value)
    ):
        _fail(f"{label} must be a full lowercase 40-character git hash")
    return value


def _git_head(repo_root: Path) -> str:
    completed = subprocess.run(
        ["git", "-C", str(repo_root), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    )
    return _ensure_lower_commit(completed.stdout.strip(), "git HEAD")


def _exact_inventory(
    root: Path,
    expected_files: set[str],
    expected_directories: set[str],
    label: str,
) -> None:
    if not root.is_dir() or root.is_symlink():
        _fail(f"{label} is not a directory")
    files: set[str] = set()
    directories: set[str] = set()
    for entry in root.iterdir():
        if entry.is_symlink():
            _fail(f"{label} contains a symlink")
        if entry.is_file():
            files.add(entry.name)
        elif entry.is_dir():
            directories.add(entry.name)
        else:
            _fail(f"{label} contains an invalid inventory type")
    if files != expected_files or directories != expected_directories:
        _fail(f"{label} inventory is not exact")


def _stage_directory_name(ordinal: int) -> str:
    return f"stage-{ordinal:03d}"


def _member_filename(ordinal: int) -> str:
    return f"member-{ordinal:03d}.f32le"


def _atom_update(hasher: Any, tag: bytes, payload: bytes) -> None:
    hasher.update(len(tag).to_bytes(4, "big"))
    hasher.update(tag)
    hasher.update(len(payload).to_bytes(8, "big"))
    hasher.update(payload)


def _fixed_parameter_layout() -> list[ParameterLayout]:
    specifications = [
        ("state.0.weight", (48, 219)),
        ("state.0.bias", (48,)),
        ("state.2.weight", (48, 48)),
        ("state.2.bias", (48,)),
        ("history.weight_ih_l0", (144, 237)),
        ("history.weight_hh_l0", (144, 48)),
        ("history.bias_ih_l0", (144,)),
        ("history.bias_hh_l0", (144,)),
        ("history_mix.weight", (48, 48)),
        ("object.0.weight", (48, 138)),
        ("object.0.bias", (48,)),
        ("card.weight", (136, 24)),
        ("group.weight", (12, 16)),
        ("edge.0.weight", (48, 89)),
        ("edge.0.bias", (48,)),
        ("edge.2.weight", (48, 48)),
        ("edge.2.bias", (48,)),
        ("group_mix.weight", (48, 48)),
        ("action.0.weight", (48, 195)),
        ("action.0.bias", (48,)),
        ("ref.0.weight", (48, 73)),
        ("ref.0.bias", (48,)),
        ("query.weight", (48, 96)),
        ("query.bias", (48,)),
        ("combine.0.weight", (48, 240)),
        ("combine.0.bias", (48,)),
        ("combine.2.weight", (48, 48)),
        ("combine.2.bias", (48,)),
        ("policy_head.weight", (1, 48)),
        ("policy_head.bias", (1,)),
        ("value_head.weight", (1, 144)),
        ("value_head.bias", (1,)),
    ]
    return [
        ParameterLayout(name, shape, math.prod(shape))
        for name, shape in specifications
    ]


def _validate_model_constants() -> None:
    observed = {
        "state_dim": screen.STATE_DIM,
        "object_dim": screen.OBJECT_DIM,
        "edge_dim": screen.EDGE_DIM,
        "action_dim": screen.ACTION_DIM,
        "ref_dim": screen.REF_DIM,
        "hidden_dim": scaled.DIM,
        "card_vocab": scaled.CARD_VOCAB,
        "card_embedding_dim": max(8, scaled.DIM // 2),
        "group_vocab": scaled.GROUP_VOCAB,
        "group_embedding_dim": max(8, scaled.DIM // 3),
        "history_length": scaled.HISTORY_LENGTH,
        "history_feature_dim": scaled.HISTORY_FEATURE_DIM,
        "history_role_dim": history_publish.HISTORY_ROLE_DIM,
        "parameter_count": history_publish.EXPECTED_PARAMETER_COUNT,
        "member_identity": history_publish.ARCHITECTURE,
        "value_model": history_publish.VALUE_MODEL,
    }
    expected = {
        "state_dim": STATE_DIM,
        "object_dim": OBJECT_DIM,
        "edge_dim": EDGE_DIM,
        "action_dim": ACTION_DIM,
        "ref_dim": REF_DIM,
        "hidden_dim": HIDDEN_DIM,
        "card_vocab": CARD_VOCAB,
        "card_embedding_dim": CARD_EMBEDDING_DIM,
        "group_vocab": GROUP_VOCAB,
        "group_embedding_dim": GROUP_EMBEDDING_DIM,
        "history_length": HISTORY_LENGTH,
        "history_feature_dim": HISTORY_FEATURE_DIM,
        "history_role_dim": HISTORY_ROLE_DIM,
        "parameter_count": PARAMETER_COUNT,
        "member_identity": MEMBER_ARCHITECTURE_IDENTITY,
        "value_model": VALUE_MODEL,
    }
    if observed != expected:
        _fail("Python model constants drifted from the strict Rust loader contract")


def _expected_layout() -> list[ParameterLayout]:
    fixed = _fixed_parameter_layout()
    if sum(parameter.count_f32 for parameter in fixed) != PARAMETER_COUNT:
        _fail("fixed parameter layout scalar count mismatch")
    model = screen.StructuredAdapter(
        CARD_VOCAB,
        GROUP_VOCAB,
        HIDDEN_DIM,
        HISTORY_LENGTH,
        HISTORY_FEATURE_DIM,
        False,
    )
    observed = [
        (name, tuple(tensor.shape), tensor.numel())
        for name, tensor in model.state_dict().items()
    ]
    expected = [
        (parameter.name, parameter.shape, parameter.count_f32)
        for parameter in fixed
    ]
    if observed != expected:
        _fail("Python state_dict layout drifted from the strict Rust loader contract")
    return fixed


def _parameter_layout_sha256(layout: list[ParameterLayout]) -> str:
    hasher = hashlib.sha256()
    _atom_update(hasher, b"domain", PARAMETER_LAYOUT_DOMAIN)
    _atom_update(
        hasher,
        b"parameter_count",
        len(layout).to_bytes(8, "big"),
    )
    scalar_count = 0
    for ordinal, parameter in enumerate(layout):
        _atom_update(
            hasher,
            b"parameter_ordinal",
            ordinal.to_bytes(8, "big"),
        )
        _atom_update(hasher, b"parameter_name", parameter.name.encode("utf-8"))
        _atom_update(
            hasher,
            b"parameter_rank",
            len(parameter.shape).to_bytes(8, "big"),
        )
        shape_bytes = b"".join(
            dimension.to_bytes(8, "big") for dimension in parameter.shape
        )
        _atom_update(hasher, b"parameter_shape_u64be", shape_bytes)
        scalar_count += parameter.count_f32
    _atom_update(hasher, b"scalar_count", scalar_count.to_bytes(8, "big"))
    if scalar_count != PARAMETER_COUNT:
        _fail("parameter layout scalar count mismatch")
    return hasher.hexdigest()


def _architecture_manifest() -> dict[str, Any]:
    return {
        "identity": STACK_ARCHITECTURE_IDENTITY,
        "member_identity": MEMBER_ARCHITECTURE_IDENTITY,
        "state_dim": STATE_DIM,
        "object_dim": OBJECT_DIM,
        "edge_dim": EDGE_DIM,
        "action_dim": ACTION_DIM,
        "ref_dim": REF_DIM,
        "hidden_dim": HIDDEN_DIM,
        "card_vocab": CARD_VOCAB,
        "card_embedding_dim": CARD_EMBEDDING_DIM,
        "group_vocab": GROUP_VOCAB,
        "group_embedding_dim": GROUP_EMBEDDING_DIM,
        "history_length": HISTORY_LENGTH,
        "history_feature_dim": HISTORY_FEATURE_DIM,
        "history_role_dim": HISTORY_ROLE_DIM,
        "stage_member_count": STAGE_MEMBER_COUNT,
        "stage_weighting": STAGE_WEIGHTING,
        "value_model": VALUE_MODEL,
    }


def _expected_fold_config() -> dict[str, Any]:
    return {
        "architecture": FOLD_ARCHITECTURE_IDENTITY,
        "seat_conditioned_policy": False,
        "dim": HIDDEN_DIM,
        "card_vocab": CARD_VOCAB,
        "group_vocab": GROUP_VOCAB,
        "history_length": HISTORY_LENGTH,
        "history_feature_dim": HISTORY_FEATURE_DIM,
        "epochs": 5,
        "batch_size_physical_decisions": 64,
        "learning_rate": 3.0e-4,
        "weight_decay": 1.0e-4,
        "ppo_clip": 0.10,
        "gradient_norm_cap": 5.0,
        "seed": 20_260_802,
        "threads": 6,
        "target_fit_mean_total_variation": 0.03,
        "value_model": FOLD_VALUE_MODEL,
    }


def _parent_manifest(parent_info: dict[str, Any]) -> dict[str, Any]:
    return {
        "directory": PARENT_DIRECTORY,
        "manifest_sha256": parent_info["manifest_sha256"],
        "payload_sha256": parent_info["payload_sha256"],
        "native_state_sha256": parent_info["native_state_sha256"],
        "model_parameter_sha256": parent_info["model_parameter_sha256"],
        "adam_step": parent_info["adam_step"],
    }


def _validate_parent_root(parent_root: Path) -> dict[str, Any]:
    manifest_path = parent_root / "checkpoint.json"
    payload_path = parent_root / "checkpoint.state.f32le"
    if not manifest_path.is_file() or not payload_path.is_file():
        _fail("parent root must contain checkpoint.json and checkpoint.state.f32le")
    manifest_sha256 = _sha256(manifest_path)
    payload_sha256 = _sha256(payload_path)
    if (
        manifest_sha256 != EXPECTED_PARENT["manifest_sha256"]
        or payload_sha256 != EXPECTED_PARENT["payload_sha256"]
    ):
        _fail("parent root is not the exact retained parent checkpoint")
    for label in (
        "manifest_sha256",
        "payload_sha256",
        "native_state_sha256",
        "model_parameter_sha256",
    ):
        _ensure_lower_sha256(EXPECTED_PARENT[label], f"retained parent {label}")
    if EXPECTED_PARENT["adam_step"] != 1:
        _fail("retained parent adam_step mismatch")
    return {
        "manifest_path": manifest_path.resolve(),
        "payload_path": payload_path.resolve(),
        **EXPECTED_PARENT,
    }


def _serialize_state_dict(
    state_dict: dict[str, torch.Tensor],
    expected_layout: list[ParameterLayout],
) -> tuple[bytes, list[dict[str, Any]]]:
    actual_names = list(state_dict.keys())
    expected_names = [parameter.name for parameter in expected_layout]
    if actual_names != expected_names:
        _fail("model state_dict keys do not match the exact non-seat-conditioned layout")
    payload = bytearray()
    bindings: list[dict[str, Any]] = []
    offset = 0
    for parameter in expected_layout:
        tensor = state_dict[parameter.name]
        if not isinstance(tensor, torch.Tensor):
            _fail(f"state entry {parameter.name} is not a tensor")
        if tuple(tensor.shape) != parameter.shape:
            _fail(f"state tensor {parameter.name} shape mismatch")
        dense = tensor.detach().cpu().contiguous().float()
        if not bool(torch.isfinite(dense).all()):
            _fail(f"state tensor {parameter.name} contains non-finite values")
        raw = dense.numpy().astype("<f4", copy=False).tobytes(order="C")
        if len(raw) != parameter.count_f32 * 4:
            _fail(f"state tensor {parameter.name} byte count mismatch")
        bindings.append(
            {
                "ordinal": len(bindings),
                "name": parameter.name,
                "shape": list(parameter.shape),
                "offset_f32": offset,
                "count_f32": parameter.count_f32,
            }
        )
        payload.extend(raw)
        offset += parameter.count_f32
    if offset != PARAMETER_COUNT or len(payload) != MEMBER_BYTE_COUNT:
        _fail("serialized parameter count mismatch")
    return bytes(payload), bindings


def _validate_fold_result_schema(
    result: dict[str, Any], expected_cache_sha256: str
) -> None:
    if result.get("schema") != scaled.SCHEMA:
        _fail("fold result schema mismatch")
    if result.get("profile_only") is not False:
        _fail("fold result must not be profile_only")
    if result.get("fold") not in (0, 1, 2, 3):
        _fail("fold result fold must be one of 0, 1, 2, 3")
    source = result.get("source")
    if not isinstance(source, dict):
        _fail("fold result source block missing")
    if source.get("cache_sha256") != expected_cache_sha256:
        _fail("fold result cache SHA-256 mismatch")
    if source.get("pair_count") != 2_048:
        _fail("fold result pair_count must be exactly 2048")
    split = result.get("split")
    if not isinstance(split, dict) or split.get("rule") != "pair_index_mod_4":
        _fail("fold result split rule mismatch")
    if result.get("config") != _expected_fold_config():
        _fail("fold result config does not match the fixed round-1 specification")
    if not isinstance(result.get("model_state"), dict):
        _fail("fold result model_state block missing")
    calibration = result.get("calibration")
    if not isinstance(calibration, dict):
        _fail("fold result calibration block missing")
    scale = calibration.get("scale")
    if (
        not isinstance(scale, (int, float))
        or isinstance(scale, bool)
        or not math.isfinite(float(scale))
        or float(scale) <= 0.0
    ):
        _fail("fold result calibration scale must be positive and finite")
    heldout = result.get("heldout_surrogate")
    overall = heldout.get("overall") if isinstance(heldout, dict) else None
    surrogate = overall.get("surrogate") if isinstance(overall, dict) else None
    if (
        not isinstance(surrogate, (int, float))
        or isinstance(surrogate, bool)
        or not math.isfinite(float(surrogate))
    ):
        _fail("fold result heldout surrogate must be finite")


def _load_fold(
    result_path: Path,
    state_path: Path,
    expected_cache_sha256: str,
    expected_layout: list[ParameterLayout],
) -> LoadedFold:
    result = _read_json(result_path)
    _validate_fold_result_schema(result, expected_cache_sha256)
    model_state = result["model_state"]
    reported_path_value = model_state.get("path")
    if not isinstance(reported_path_value, str) or not reported_path_value:
        _fail("fold result model_state path missing")
    reported_path = Path(reported_path_value)
    if not reported_path.is_absolute():
        reported_path = result_path.parent / reported_path
    explicit_path = state_path.resolve()
    if explicit_path != reported_path.resolve():
        _fail("explicit fold state path does not match the fold result model_state path")
    if not explicit_path.is_file():
        _fail(f"fold state file missing: {explicit_path}")
    reported_sha256 = _ensure_lower_sha256(
        model_state.get("sha256"), "fold result model_state sha256"
    )
    actual_state_sha256 = _sha256(explicit_path)
    if actual_state_sha256 != reported_sha256:
        _fail("fold state SHA-256 mismatch")
    state_dict = torch.load(explicit_path, map_location="cpu", weights_only=False)
    if not isinstance(state_dict, dict) or not state_dict:
        _fail("fold state file does not contain a raw nonempty state_dict")
    payload, bindings = _serialize_state_dict(state_dict, expected_layout)
    return LoadedFold(
        fold=int(result["fold"]),
        result_path=result_path.resolve(),
        result_sha256=_sha256(result_path),
        state_path=explicit_path,
        state_sha256=actual_state_sha256,
        heldout_surrogate=float(result["heldout_surrogate"]["overall"]["surrogate"]),
        calibrated_scale=float(result["calibration"]["scale"]),
        parameter_count=PARAMETER_COUNT,
        payload=payload,
        payload_sha256=hashlib.sha256(payload).hexdigest(),
        parameter_bindings=bindings,
    )


def _validate_aggregate_result(
    aggregate_path: Path,
    loaded_folds: list[LoadedFold],
    expected_cache_sha256: str,
) -> str:
    aggregate = _read_json(aggregate_path)
    if aggregate.get("schema") != scaled.AGGREGATE_SCHEMA:
        _fail("aggregate result schema mismatch")
    if aggregate.get("source_cache_sha256") != expected_cache_sha256:
        _fail("aggregate result cache SHA-256 mismatch")
    if aggregate.get("config") != _expected_fold_config():
        _fail("aggregate result config mismatch")
    if aggregate.get("gate_config") != {
        "min_mean_total_variation": 0.0,
        "max_mean_total_variation": 0.05,
        "max_p90_total_variation": 0.15,
        "max_absolute_joint_log_ratio": 0.75,
    }:
        _fail("aggregate result gate configuration mismatch")
    gates = aggregate.get("gates")
    if (
        aggregate.get("pass") is not True
        or not isinstance(gates, dict)
        or not gates
        or any(value is not True for value in gates.values())
    ):
        _fail("aggregate result did not pass every frozen gate")
    expected_fold_hashes = {fold.result_sha256 for fold in loaded_folds}
    bound_fold_results = aggregate.get("fold_results")
    if (
        not isinstance(bound_fold_results, list)
        or len(bound_fold_results) != STAGE_MEMBER_COUNT
    ):
        _fail("aggregate result must bind exactly four fold results")
    observed_fold_hashes = {
        binding.get("sha256")
        for binding in bound_fold_results
        if isinstance(binding, dict)
    }
    if observed_fold_hashes != expected_fold_hashes:
        _fail("aggregate result does not bind the supplied fold results")
    expected_surrogates = {
        str(fold.fold): fold.heldout_surrogate for fold in loaded_folds
    }
    if aggregate.get("fold_surrogates") != expected_surrogates:
        _fail("aggregate fold surrogates do not match the supplied fold results")
    return _sha256(aggregate_path)


def _payload_is_finite_f32le(payload: bytes) -> bool:
    if len(payload) != MEMBER_BYTE_COUNT:
        return False
    values = np.frombuffer(payload, dtype="<f4")
    return values.size == PARAMETER_COUNT and bool(np.isfinite(values).all())


def _validate_stage_binding(stage: Any, stage_ordinal: int) -> dict[str, Any]:
    if not isinstance(stage, dict):
        _fail("prior stage binding must be an object")
    required = {"ordinal", "directory", "members"}
    if set(stage) not in (required, required | {"scale"}):
        _fail("prior stage binding fields are not exact")
    if (
        stage.get("ordinal") != stage_ordinal
        or stage.get("directory") != _stage_directory_name(stage_ordinal)
    ):
        _fail("prior stage binding mismatch")
    if "scale" in stage:
        scale = stage["scale"]
        if (
            not isinstance(scale, float)
            or struct.pack("<f", scale) != struct.pack("<f", 1.0)
        ):
            _fail("prior stage scale must be omitted or exactly 1.0f32")
    members = stage.get("members")
    if not isinstance(members, list) or len(members) != STAGE_MEMBER_COUNT:
        _fail("prior stage must contain exactly four members")
    for member_ordinal, member in enumerate(members):
        _expect_exact_keys(
            member,
            {"ordinal", "filename", "sha256", "byte_count"},
            "prior member binding",
        )
        if (
            member.get("ordinal") != member_ordinal
            or member.get("filename") != _member_filename(member_ordinal)
            or member.get("byte_count") != MEMBER_BYTE_COUNT
        ):
            _fail("prior member binding mismatch")
        _ensure_lower_sha256(member.get("sha256"), "prior member sha256")
    return stage


def _stack_composite_sha256(
    parent: dict[str, Any],
    parameter_layout_sha256: str,
    weights_sha256: str,
    stages: list[dict[str, Any]],
) -> str:
    layout_hash = bytes.fromhex(
        _ensure_lower_sha256(parameter_layout_sha256, "parameter layout sha256")
    )
    weights_hash = bytes.fromhex(
        _ensure_lower_sha256(weights_sha256, "weights sha256")
    )
    hasher = hashlib.sha256()
    _atom_update(hasher, b"domain", COMPOSITE_DOMAIN)
    _atom_update(
        hasher,
        b"architecture_identity",
        STACK_ARCHITECTURE_IDENTITY.encode("utf-8"),
    )
    for field, tag in (
        ("manifest_sha256", b"parent_manifest_sha256"),
        ("payload_sha256", b"parent_payload_sha256"),
        ("native_state_sha256", b"parent_native_state_sha256"),
        ("model_parameter_sha256", b"parent_model_parameter_sha256"),
    ):
        _atom_update(
            hasher,
            tag,
            bytes.fromhex(_ensure_lower_sha256(parent.get(field), f"parent {field}")),
        )
    adam_step = parent.get("adam_step")
    if not isinstance(adam_step, int) or isinstance(adam_step, bool) or adam_step < 0:
        _fail("parent adam_step must be a nonnegative integer")
    _atom_update(hasher, b"parent_adam_step", adam_step.to_bytes(8, "big"))
    _atom_update(hasher, b"weights_parameter_layout_sha256", layout_hash)
    _atom_update(hasher, b"weights_sha256", weights_hash)
    _atom_update(hasher, b"stage_count", len(stages).to_bytes(8, "big"))
    for stage_ordinal, stage in enumerate(stages):
        if stage.get("ordinal") != stage_ordinal:
            _fail("stage ordinal mismatch during composite framing")
        _atom_update(
            hasher,
            b"stage_ordinal",
            stage_ordinal.to_bytes(8, "big"),
        )
        scale = stage.get("scale", 1.0)
        if not isinstance(scale, (int, float)) or isinstance(scale, bool):
            _fail("stage scale is not numeric")
        scale_bytes = struct.pack("<f", float(scale))
        if scale_bytes != struct.pack("<f", 1.0):
            _fail("stage scale must be exactly 1.0f32")
        _atom_update(hasher, b"stage_scale_f32le", scale_bytes)
        members = stage.get("members")
        if not isinstance(members, list) or len(members) != STAGE_MEMBER_COUNT:
            _fail("stage member count mismatch during composite framing")
        _atom_update(hasher, b"member_count", len(members).to_bytes(8, "big"))
        for member_ordinal, member in enumerate(members):
            if member.get("ordinal") != member_ordinal:
                _fail("member ordinal mismatch during composite framing")
            _atom_update(
                hasher,
                b"member_ordinal",
                member_ordinal.to_bytes(8, "big"),
            )
            member_sha256 = _ensure_lower_sha256(
                member.get("sha256"), "member sha256"
            )
            _atom_update(hasher, b"member_sha256", bytes.fromhex(member_sha256))
    return hasher.hexdigest()


def _validate_and_hash_bound_weights(
    root: Path,
    stages: list[dict[str, Any]],
) -> tuple[str, dict[str, str]]:
    weights_root = root / WEIGHTS_DIRECTORY
    expected_stage_directories = {
        _stage_directory_name(index) for index in range(len(stages))
    }
    _exact_inventory(
        weights_root,
        set(),
        expected_stage_directories,
        "structured history stack weights",
    )
    weights_hasher = hashlib.sha256()
    file_sha256s: dict[str, str] = {}
    for stage_ordinal, stage in enumerate(stages):
        _validate_stage_binding(stage, stage_ordinal)
        stage_root = weights_root / stage["directory"]
        expected_members = {
            _member_filename(index) for index in range(STAGE_MEMBER_COUNT)
        }
        _exact_inventory(
            stage_root,
            expected_members,
            set(),
            "structured history stack stage",
        )
        for member in stage["members"]:
            member_path = stage_root / member["filename"]
            payload = member_path.read_bytes()
            if not _payload_is_finite_f32le(payload):
                _fail("structured history stack member payload is not finite f32le")
            actual_sha256 = hashlib.sha256(payload).hexdigest()
            if (
                actual_sha256 != member["sha256"]
                or len(payload) != member["byte_count"]
            ):
                _fail("structured history stack member digest mismatch")
            weights_hasher.update(payload)
            relative_path = member_path.relative_to(root).as_posix()
            file_sha256s[relative_path] = actual_sha256
    return weights_hasher.hexdigest(), file_sha256s


def _load_prior_stack(
    prior_root: Path | None,
    parameter_layout_sha256: str,
) -> PriorStack | None:
    if prior_root is None:
        return None
    root = prior_root.resolve()
    _exact_inventory(
        root,
        {STACK_FILENAME},
        {PARENT_DIRECTORY, WEIGHTS_DIRECTORY},
        "prior structured history stack root",
    )
    parent_root = root / PARENT_DIRECTORY
    _exact_inventory(
        parent_root,
        {"checkpoint.json", "checkpoint.state.f32le"},
        set(),
        "prior structured history stack parent",
    )
    manifest_path = root / STACK_FILENAME
    manifest = _read_json(manifest_path)
    _expect_exact_keys(
        manifest,
        {
            "schema",
            "publication_encoding",
            "parent",
            "architecture",
            "weights",
            "composite_model_parameter_sha256",
        },
        "prior stack manifest",
    )
    if (
        manifest["schema"] != STACK_SCHEMA
        or manifest["publication_encoding"] != PUBLICATION_ENCODING
    ):
        _fail("prior stack manifest identity mismatch")
    parent = _expect_exact_keys(
        manifest["parent"],
        {
            "directory",
            "manifest_sha256",
            "payload_sha256",
            "native_state_sha256",
            "model_parameter_sha256",
            "adam_step",
        },
        "prior stack parent",
    )
    expected_parent = _parent_manifest(EXPECTED_PARENT)
    if parent != expected_parent:
        _fail("prior stack parent identity mismatch")
    if _sha256(parent_root / "checkpoint.json") != parent["manifest_sha256"]:
        _fail("prior stack parent manifest SHA-256 mismatch")
    if _sha256(parent_root / "checkpoint.state.f32le") != parent["payload_sha256"]:
        _fail("prior stack parent payload SHA-256 mismatch")
    architecture = _expect_exact_keys(
        manifest["architecture"],
        set(_architecture_manifest()),
        "prior stack architecture",
    )
    if architecture != _architecture_manifest():
        _fail("prior stack architecture mismatch")
    weights = _expect_exact_keys(
        manifest["weights"],
        {
            "directory",
            "encoding",
            "sha256",
            "parameter_count",
            "parameter_layout_sha256",
            "stages",
        },
        "prior stack weights",
    )
    if (
        weights["directory"] != WEIGHTS_DIRECTORY
        or weights["encoding"] != WEIGHTS_ENCODING
        or weights["parameter_count"] != PARAMETER_COUNT
        or weights["parameter_layout_sha256"] != parameter_layout_sha256
    ):
        _fail("prior stack weights binding mismatch")
    weights_sha256 = _ensure_lower_sha256(weights["sha256"], "prior weights sha256")
    stages = weights["stages"]
    if not isinstance(stages, list) or not stages:
        _fail("prior stack must contain at least one stage")
    actual_weights_sha256, weight_file_sha256s = _validate_and_hash_bound_weights(
        root, stages
    )
    if actual_weights_sha256 != weights_sha256:
        _fail("prior stack weights SHA-256 mismatch")
    expected_composite = _stack_composite_sha256(
        parent,
        parameter_layout_sha256,
        weights_sha256,
        stages,
    )
    composite = _ensure_lower_sha256(
        manifest["composite_model_parameter_sha256"],
        "prior composite_model_parameter_sha256",
    )
    if composite != expected_composite:
        _fail("prior stack composite SHA-256 mismatch")
    copied_file_sha256s = {
        "parent/checkpoint.json": _sha256(parent_root / "checkpoint.json"),
        "parent/checkpoint.state.f32le": _sha256(
            parent_root / "checkpoint.state.f32le"
        ),
        **weight_file_sha256s,
    }
    return PriorStack(
        root=root,
        manifest_sha256=_sha256(manifest_path),
        stages=copy.deepcopy(stages),
        copied_file_sha256s=copied_file_sha256s,
    )


def _copy_or_create_base(
    output_root: Path,
    prior: PriorStack | None,
    parent_info: dict[str, Any],
) -> None:
    if prior is not None:
        _copy_tree_new(prior.root / PARENT_DIRECTORY, output_root / PARENT_DIRECTORY)
        _copy_tree_new(prior.root / WEIGHTS_DIRECTORY, output_root / WEIGHTS_DIRECTORY)
        for relative_path, expected_sha256 in prior.copied_file_sha256s.items():
            if _sha256(output_root / relative_path) != expected_sha256:
                _fail("prior stack bytes changed while appending")
        return
    parent_output = output_root / PARENT_DIRECTORY
    parent_output.mkdir()
    _copy_file_new(parent_info["manifest_path"], parent_output / "checkpoint.json")
    _copy_file_new(
        parent_info["payload_path"],
        parent_output / "checkpoint.state.f32le",
    )
    (output_root / WEIGHTS_DIRECTORY).mkdir()


def _build_manifest(
    parent_info: dict[str, Any],
    stages: list[dict[str, Any]],
    parameter_layout_sha256: str,
    weights_sha256: str,
) -> dict[str, Any]:
    parent = _parent_manifest(parent_info)
    composite = _stack_composite_sha256(
        parent,
        parameter_layout_sha256,
        weights_sha256,
        stages,
    )
    return {
        "schema": STACK_SCHEMA,
        "publication_encoding": PUBLICATION_ENCODING,
        "parent": parent,
        "architecture": _architecture_manifest(),
        "weights": {
            "directory": WEIGHTS_DIRECTORY,
            "encoding": WEIGHTS_ENCODING,
            "sha256": weights_sha256,
            "parameter_count": PARAMETER_COUNT,
            "parameter_layout_sha256": parameter_layout_sha256,
            "stages": stages,
        },
        "composite_model_parameter_sha256": composite,
    }


def _build_publication_report(
    args: argparse.Namespace,
    current_head: str,
    parent_info: dict[str, Any],
    prior: PriorStack | None,
    aggregate_result_sha256: str,
    expected_layout: list[ParameterLayout],
    parameter_layout_sha256: str,
    manifest: dict[str, Any],
    manifest_sha256: str,
    new_stage: dict[str, Any],
    member_provenance: list[dict[str, Any]],
) -> dict[str, Any]:
    return {
        "schema": PUBLICATION_REPORT_SCHEMA,
        "publication_encoding": PUBLICATION_ENCODING,
        "output": {
            "root": str(args.output_root),
            "manifest_path": str(args.output_root / STACK_FILENAME),
            "manifest_sha256": manifest_sha256,
            "weights_sha256": manifest["weights"]["sha256"],
            "composite_model_parameter_sha256": manifest[
                "composite_model_parameter_sha256"
            ],
            "stage_count": len(manifest["weights"]["stages"]),
        },
        "source": {
            "expected_cache_sha256": args.expected_cache_sha256,
            "expected_source_commit": args.expected_source_commit,
            "publisher_git_head": current_head,
            "source_commit_assumption": (
                "fold screen artifacts do not bind a source commit, so the publisher "
                "confirms the requested commit against the current repository HEAD"
            ),
            "aggregate_result": {
                "path": str(args.aggregate_result),
                "sha256": aggregate_result_sha256,
                "aggregate_pass_required": True,
            },
        },
        "parent": {
            "source_root": str(args.parent_root),
            **_parent_manifest(parent_info),
        },
        "prior_stack": None
        if prior is None
        else {
            "root": str(prior.root),
            "manifest_sha256": prior.manifest_sha256,
            "stage_count": len(prior.stages),
            "preserved_stage_bindings": prior.stages,
            "copied_file_sha256s": prior.copied_file_sha256s,
        },
        "parameter_layout": {
            "domain": PARAMETER_LAYOUT_DOMAIN.decode("ascii"),
            "sha256": parameter_layout_sha256,
            "tensor_count": len(expected_layout),
            "scalar_count": PARAMETER_COUNT,
            "parameters": [
                {
                    "ordinal": ordinal,
                    "name": parameter.name,
                    "shape": list(parameter.shape),
                    "count_f32": parameter.count_f32,
                }
                for ordinal, parameter in enumerate(expected_layout)
            ],
        },
        "new_stage": new_stage,
        "members": member_provenance,
        "manifest": manifest,
        "composite_framing": {
            "domain": COMPOSITE_DOMAIN.decode("ascii"),
            "manifest_sha256_atom": False,
            "stage_scale_f32le": 1.0,
        },
        "non_claims": [
            "publication does not imply live strength",
            "publication does not imply promotion",
            "publication does not imply pro-level play",
        ],
    }


def _validate_path_contract(args: argparse.Namespace) -> None:
    args.output_root = args.output_root.resolve()
    args.publication_report = args.publication_report.resolve()
    args.parent_root = args.parent_root.resolve()
    args.repo_root = args.repo_root.resolve()
    args.aggregate_result = args.aggregate_result.resolve()
    if args.prior_stack_root is not None:
        args.prior_stack_root = args.prior_stack_root.resolve()
    args.fold_result = [path.resolve() for path in args.fold_result]
    args.fold_state = [path.resolve() for path in args.fold_state]
    if args.output_root.exists():
        _fail(f"refusing to overwrite {args.output_root}")
    if args.publication_report.exists():
        _fail(f"refusing to overwrite {args.publication_report}")
    if (
        args.publication_report == args.output_root
        or args.output_root in args.publication_report.parents
    ):
        _fail("publication report must resolve outside output_root")
    if args.publication_report in args.output_root.parents:
        _fail("publication report path conflicts with output_root")


def publish(args: argparse.Namespace) -> dict[str, Any]:
    _validate_path_contract(args)
    expected_cache_sha256 = _ensure_lower_sha256(
        args.expected_cache_sha256, "expected_cache_sha256"
    )
    expected_source_commit = _ensure_lower_commit(
        args.expected_source_commit, "expected_source_commit"
    )
    if (
        len(args.fold_result) != STAGE_MEMBER_COUNT
        or len(args.fold_state) != STAGE_MEMBER_COUNT
    ):
        _fail("publisher requires exactly four fold results and four fold states")
    current_head = _git_head(args.repo_root)
    if current_head != expected_source_commit:
        _fail("current repo git HEAD does not match expected_source_commit")
    _validate_model_constants()
    expected_layout = _expected_layout()
    parameter_layout_sha256 = _parameter_layout_sha256(expected_layout)
    parent_info = _validate_parent_root(args.parent_root)
    prior = _load_prior_stack(args.prior_stack_root, parameter_layout_sha256)

    loaded_folds = [
        _load_fold(
            result_path,
            state_path,
            expected_cache_sha256,
            expected_layout,
        )
        for result_path, state_path in zip(args.fold_result, args.fold_state)
    ]
    if sorted(fold.fold for fold in loaded_folds) != [0, 1, 2, 3]:
        _fail("fold set must be exactly 0, 1, 2, 3")
    by_fold = {fold.fold: fold for fold in loaded_folds}
    if len(by_fold) != STAGE_MEMBER_COUNT:
        _fail("fold ids must be unique")
    aggregate_result_sha256 = _validate_aggregate_result(
        args.aggregate_result,
        loaded_folds,
        expected_cache_sha256,
    )

    args.output_root.mkdir(parents=True, exist_ok=False)
    _copy_or_create_base(args.output_root, prior, parent_info)
    prior_stages = [] if prior is None else prior.stages
    stage_ordinal = len(prior_stages)
    stage_directory = _stage_directory_name(stage_ordinal)
    stage_root = args.output_root / WEIGHTS_DIRECTORY / stage_directory
    stage_root.mkdir()

    member_bindings: list[dict[str, Any]] = []
    member_provenance: list[dict[str, Any]] = []
    for member_ordinal, fold in enumerate(by_fold[index] for index in range(4)):
        filename = _member_filename(member_ordinal)
        output_path = stage_root / filename
        output_sha256 = _write_new_bytes(output_path, fold.payload)
        if output_sha256 != fold.payload_sha256:
            _fail("member payload SHA-256 changed after writing")
        member_binding = {
            "ordinal": member_ordinal,
            "filename": filename,
            "sha256": output_sha256,
            "byte_count": len(fold.payload),
        }
        member_bindings.append(member_binding)
        member_provenance.append(
            {
                "stage_ordinal": stage_ordinal,
                "member_ordinal": member_ordinal,
                "fold": fold.fold,
                "fold_result": {
                    "path": str(fold.result_path),
                    "sha256": fold.result_sha256,
                },
                "calibrated_model_state": {
                    "path": str(fold.state_path),
                    "sha256": fold.state_sha256,
                },
                "heldout_surrogate": fold.heldout_surrogate,
                "calibration_scale": fold.calibrated_scale,
                "output": {
                    "path": output_path.relative_to(args.output_root).as_posix(),
                    **member_binding,
                    "encoding": WEIGHTS_ENCODING,
                    "parameter_count": fold.parameter_count,
                    "parameters": fold.parameter_bindings,
                },
            }
        )

    new_stage = {
        "ordinal": stage_ordinal,
        "directory": stage_directory,
        "members": member_bindings,
    }
    stages = [*prior_stages, new_stage]
    weights_sha256, _ = _validate_and_hash_bound_weights(args.output_root, stages)
    manifest = _build_manifest(
        parent_info,
        stages,
        parameter_layout_sha256,
        weights_sha256,
    )
    manifest_path = args.output_root / STACK_FILENAME
    manifest_sha256 = _write_new_json(manifest_path, manifest)
    _exact_inventory(
        args.output_root,
        {STACK_FILENAME},
        {PARENT_DIRECTORY, WEIGHTS_DIRECTORY},
        "published structured history stack root",
    )
    _exact_inventory(
        args.output_root / PARENT_DIRECTORY,
        {"checkpoint.json", "checkpoint.state.f32le"},
        set(),
        "published structured history stack parent",
    )

    publication_report = _build_publication_report(
        args,
        current_head,
        parent_info,
        prior,
        aggregate_result_sha256,
        expected_layout,
        parameter_layout_sha256,
        manifest,
        manifest_sha256,
        new_stage,
        member_provenance,
    )
    publication_report_sha256 = _write_new_json(
        args.publication_report, publication_report
    )
    return {
        "stack_root": str(args.output_root),
        "stack_manifest_sha256": manifest_sha256,
        "weights_sha256": weights_sha256,
        "parameter_layout_sha256": parameter_layout_sha256,
        "composite_model_parameter_sha256": manifest[
            "composite_model_parameter_sha256"
        ],
        "publication_report": str(args.publication_report),
        "publication_report_sha256": publication_report_sha256,
        "stage_count": len(stages),
        "latest_stage_ordinal": stage_ordinal,
        "latest_stage_member_sha256s": [
            member["sha256"] for member in member_bindings
        ],
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fold-result", action="append", type=Path, required=True)
    parser.add_argument("--fold-state", action="append", type=Path, required=True)
    parser.add_argument("--aggregate-result", type=Path, required=True)
    parser.add_argument("--parent-root", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--publication-report", type=Path, required=True)
    parser.add_argument("--expected-cache-sha256", required=True)
    parser.add_argument("--expected-source-commit", required=True)
    parser.add_argument("--prior-stack-root", type=Path)
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=SCRIPT_DIR.parent.parent,
        help="repo root used to confirm the current git HEAD",
    )
    return parser


def main() -> int:
    args = _parser().parse_args()
    print(json.dumps(publish(args), sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
