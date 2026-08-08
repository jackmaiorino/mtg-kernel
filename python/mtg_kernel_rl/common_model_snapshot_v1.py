"""Strict common initial-model snapshot interchange for matched native trials.

The artifact is deliberately narrower than a checkpoint: Python is the seeded
initializer authority, while both loaders bootstrap a fresh step-zero optimizer.
Portable validation never invokes the seeded initializer.  Authority generation
is tuple-gated and is the only operation which constructs seeded parameters.
"""

from __future__ import annotations

import copy
import hashlib
import json
import math
import os
import platform
import stat
import struct
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Iterable

import torch

from .checkpoint import create_adam
from .determinism import configure_torch_determinism, derive_model_init_seed
from .model import (
    INITIALIZER_RUNNER_FIXED_V1,
    INITIALIZER_TRAINER_SEEDED_V1,
    MODEL_ARCHITECTURE_VERSION,
    KernelPolicyValueNet,
    ModelConfig,
)


SNAPSHOT_SCHEMA_V1 = "mtg-kernel-common-model-snapshot/v1"
SNAPSHOT_IDENTITY_V1 = "mtg-kernel-python-authoritative-common-model-snapshot-v1"
SNAPSHOT_PURPOSE_V1 = "matched-throughput-trial-initial-model-only"
PYTHON_LOADER_IDENTITY_V1 = "mtg-kernel-python-common-model-snapshot-loader-v1"
RUST_LOADER_IDENTITY_V1 = "mtg-kernel-rust-common-model-snapshot-loader-v1"
INITIALIZER_AUTHORITY_V1 = "Python KernelPolicyValueNet.reset_seeded_parameters"
OPTIMIZER_IDENTITY_V1 = "native-adam-canonical-scorer-bias-gauge-v1"
TRAINER_SCHEDULE_VERSION_V1 = "mtg-kernel-native-trainer-schedule-sha256-v1"
PYTHON_REFERENCE_SEED_VERSION_V1 = "kernel-python-rl-trainer-sha256-v2"
SCHEDULE_GOLDENS_SHA256_V1 = "6b2e1edbbe49b4e02f98794f9057f5c2bb8e3079d2ba8cb3e2a4b9ea6c34867c"
MODEL_CONFIG_FINGERPRINT_V1 = "f3836afa17acc74b4856fe18222345116f27c12fa5ad18c34b4dec3f04855251"
FEATURE_CONTRACT_DIGEST_V1 = "bcc808186e40a1ad6aec679d8a386631cb1226379366a632603f0beb95b47396"
FEATURE_ENCODING_DIGEST_V1 = "918e57a0796807e84310026de48d30b500813ef37d939462ea85b7255a39111c"
BASE_SEED_V1 = 0
MODEL_INIT_SEED_V1 = 6_443_515_232_517_447_393
PARAMETER_TENSOR_COUNT_V1 = 33
PARAMETER_ELEMENT_COUNT_V1 = 1_230_994
PAYLOAD_BYTE_COUNT_V1 = 4_923_976
MANIFEST_MAX_BYTES_V1 = 64 * 1024
PAYLOAD_MAX_BYTES_V1 = 8 * 1024 * 1024
SOURCE_MAX_BYTES_V1 = 8 * 1024 * 1024
PAYLOAD_ENCODING_V1 = "ieee-754-binary32-little-endian"
PAYLOAD_LAYOUT_V1 = (
    "torch-named-parameters-c-contiguous-row-major-linear-output-input-no-padding-v1"
)
MOMENT_INITIALIZATION_V1 = "positive-zero-f32"
CANONICAL_GAUGE_PARAMETERS_V1 = ["scorer.2.bias"]
VALUE_HEAD_GAUGE_V1 = "none"
AUTHORITY_RUNTIME_IDENTITY_V1 = (
    "python-torch-windows-amd64-python3.13.14-torch2.13.0+cpu-cpu-f32-"
    "deterministic-threads1-v1"
)
SOURCE_BUNDLE_CONTRACT_V1 = (
    "sha256(repeated(frame(source-relative-path,raw32(source-sha256))))"
)
NONCLAIM_V1 = (
    "Rust does not reproduce the Python trainer-seeded-v1 initializer in this "
    "snapshot configuration; the snapshot proves bit-exact initial parameters only "
    "and does not establish seeded-initializer parity, cross-runtime numerical bit "
    "parity, learning parity, or speedup."
)
LEGACY_OPTIMIZER_NONCLAIM_V1 = (
    "The legacy Python-v3 optimizer is not the matched optimizer lane because it "
    "retains accidental scorer-bias gauge drift."
)
INDEPENDENT_GATES_V1 = [
    "exact Torch initializer reproduction",
    "native checkpoint/resume",
    "learning noninferiority",
    "speed ratio",
]

AUTHORITY_RUNTIME_CONFIGURATION_V1: dict[str, Any] = {
    "byte_order": "little",
    "device": "cpu",
    "platform_machine": "AMD64",
    "platform_system": "Windows",
    "python_version": "3.13.14",
    "torch_default_dtype": "torch.float32",
    "torch_deterministic_algorithms": True,
    "torch_num_interop_threads": 1,
    "torch_num_threads": 1,
    "torch_version": "2.13.0+cpu",
}

AUTHORITY_SOURCE_PATHS_V1 = (
    "python/mtg_kernel_rl/model.py",
    "python/mtg_kernel_rl/features.py",
    "python/mtg_kernel_rl/determinism.py",
    "python/mtg_kernel_rl/common_model_snapshot_v1.py",
)

# (name, shape, element_offset, element_count)
EXPECTED_PARAMETER_LAYOUT_V1: tuple[tuple[str, tuple[int, ...], int, int], ...] = (
    ("card_embedding.weight", (65537, 16), 0, 1048592),
    ("object_encoder.0.weight", (64, 114), 1048592, 7296),
    ("object_encoder.0.bias", (64,), 1055888, 64),
    ("object_encoder.2.weight", (64, 64), 1055952, 4096),
    ("object_encoder.2.bias", (64,), 1060048, 64),
    ("edge_encoder.0.weight", (64, 169), 1060112, 10816),
    ("edge_encoder.0.bias", (64,), 1070928, 64),
    ("edge_encoder.2.weight", (64, 64), 1070992, 4096),
    ("edge_encoder.2.bias", (64,), 1075088, 64),
    ("node_update.0.weight", (64, 128), 1075152, 8192),
    ("node_update.0.bias", (64,), 1083344, 64),
    ("node_update.2.weight", (64, 64), 1083408, 4096),
    ("node_update.2.bias", (64,), 1087504, 64),
    ("state_encoder.0.weight", (64, 1499), 1087568, 95936),
    ("state_encoder.0.bias", (64,), 1183504, 64),
    ("state_encoder.2.weight", (64, 64), 1183568, 4096),
    ("state_encoder.2.bias", (64,), 1187664, 64),
    ("action_ref_encoder.0.weight", (64, 89), 1187728, 5696),
    ("action_ref_encoder.0.bias", (64,), 1193424, 64),
    ("action_ref_encoder.2.weight", (64, 64), 1193488, 4096),
    ("action_ref_encoder.2.bias", (64,), 1197584, 64),
    ("action_encoder.0.weight", (64, 259), 1197648, 16576),
    ("action_encoder.0.bias", (64,), 1214224, 64),
    ("action_encoder.2.weight", (64, 64), 1214288, 4096),
    ("action_encoder.2.bias", (64,), 1218384, 64),
    ("scorer.0.weight", (64, 128), 1218448, 8192),
    ("scorer.0.bias", (64,), 1226640, 64),
    ("scorer.2.weight", (1, 64), 1226704, 64),
    ("scorer.2.bias", (1,), 1226768, 1),
    ("value_head.0.weight", (64, 64), 1226769, 4096),
    ("value_head.0.bias", (64,), 1230865, 64),
    ("value_head.2.weight", (1, 64), 1230929, 64),
    ("value_head.2.bias", (1,), 1230993, 1),
)


class CommonModelSnapshotErrorV1(ValueError):
    """A fail-closed snapshot generation or loading error."""


@dataclass(frozen=True)
class ValidatedCommonModelSnapshotV1:
    manifest: dict[str, Any]
    manifest_file_bytes: bytes
    payload_bytes: bytes
    manifest_file_sha256: str


@dataclass
class PythonCommonSnapshotTrainStateV1:
    """Small mutable holder supporting an atomic model/optimizer swap."""

    model: KernelPolicyValueNet
    optimizer: torch.optim.Adam
    adam_step: int
    scorer_bias_anchor_f32_bits: int
    model_snapshot: dict[str, Any] | None
    counters: dict[str, int]
    publications: list[Any]
    records: list[Any]


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _frame(tag: str, data: bytes) -> bytes:
    if type(tag) is not str or not tag or not tag.isascii():
        raise CommonModelSnapshotErrorV1("digest frame tag must be nonempty ASCII")
    encoded = tag.encode("ascii")
    return len(encoded).to_bytes(4, "big") + encoded + len(data).to_bytes(8, "big") + data


def canonical_json_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
        allow_nan=False,
    ).encode("utf-8")


def _reject_float(_value: str) -> Any:
    raise CommonModelSnapshotErrorV1("manifest JSON floating-point values are forbidden")


def _reject_constant(_value: str) -> Any:
    raise CommonModelSnapshotErrorV1("manifest JSON non-finite values are forbidden")


def _reject_duplicate_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise CommonModelSnapshotErrorV1(f"duplicate manifest key {key!r}")
        result[key] = value
    return result


def _parse_manifest(file_bytes: bytes) -> dict[str, Any]:
    if not file_bytes or len(file_bytes) > MANIFEST_MAX_BYTES_V1:
        raise CommonModelSnapshotErrorV1("manifest size is outside the bounded contract")
    try:
        text = file_bytes.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise CommonModelSnapshotErrorV1("manifest is not UTF-8") from exc
    try:
        value = json.loads(
            text,
            object_pairs_hook=_reject_duplicate_pairs,
            parse_float=_reject_float,
            parse_constant=_reject_constant,
        )
    except (json.JSONDecodeError, TypeError) as exc:
        raise CommonModelSnapshotErrorV1("manifest JSON is invalid") from exc
    if type(value) is not dict:
        raise CommonModelSnapshotErrorV1("manifest root must be an object")
    _require_ascii_strings(value)
    if file_bytes != canonical_json_bytes(value) + b"\n":
        raise CommonModelSnapshotErrorV1("manifest file is not canonical JSON plus one LF")
    return value


def _require_ascii_strings(value: Any) -> None:
    if type(value) is str:
        if not value.isascii():
            raise CommonModelSnapshotErrorV1("manifest strings must be ASCII")
    elif type(value) is list:
        for item in value:
            _require_ascii_strings(item)
    elif type(value) is dict:
        for key, item in value.items():
            if type(key) is not str or not key.isascii():
                raise CommonModelSnapshotErrorV1("manifest object keys must be ASCII strings")
            _require_ascii_strings(item)


def _require_exact_keys(value: Any, expected: set[str], context: str) -> dict[str, Any]:
    if type(value) is not dict:
        raise CommonModelSnapshotErrorV1(f"{context} must be an object")
    actual = set(value)
    if actual != expected:
        raise CommonModelSnapshotErrorV1(
            f"{context} keys mismatch: missing={sorted(expected - actual)} "
            f"extra={sorted(actual - expected)}"
        )
    return value


def _require_str(value: Any, expected: str, context: str) -> None:
    if type(value) is not str or value != expected:
        raise CommonModelSnapshotErrorV1(f"{context} mismatch")


def _require_int(value: Any, expected: int, context: str) -> None:
    if type(value) is not int or value != expected:
        raise CommonModelSnapshotErrorV1(f"{context} mismatch")


def _require_bool(value: Any, expected: bool, context: str) -> None:
    if type(value) is not bool or value is not expected:
        raise CommonModelSnapshotErrorV1(f"{context} mismatch")


def _file_identity(info: os.stat_result) -> tuple[int, ...]:
    # Windows can report a slightly different ctime for lstat(path) and
    # fstat(open_handle) even when the volume/file index is identical.  The
    # stable identity plus size and modification time are compared instead.
    return (
        int(info.st_mode),
        int(info.st_dev),
        int(info.st_ino),
        int(info.st_nlink),
        int(info.st_size),
        int(info.st_mtime_ns),
    )


def _is_reparse(info: os.stat_result) -> bool:
    attributes = getattr(info, "st_file_attributes", 0)
    reparse = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    return bool(attributes & reparse)


def _capture_regular_file(
    path: Path,
    maximum_bytes: int,
    *,
    after_read_hook: Callable[[], None] | None = None,
) -> bytes:
    try:
        path_before = os.lstat(path)
    except OSError as exc:
        raise CommonModelSnapshotErrorV1(f"cannot stat snapshot file {path}") from exc
    if not stat.S_ISREG(path_before.st_mode) or stat.S_ISLNK(path_before.st_mode) or _is_reparse(path_before):
        raise CommonModelSnapshotErrorV1(f"snapshot path is not a regular non-link file: {path}")
    if path_before.st_size < 0 or path_before.st_size > maximum_bytes:
        raise CommonModelSnapshotErrorV1(f"snapshot file exceeds allocation cap: {path}")
    flags = os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise CommonModelSnapshotErrorV1(f"cannot open snapshot file {path}") from exc
    try:
        opened_before = os.fstat(descriptor)
        if not stat.S_ISREG(opened_before.st_mode) or _file_identity(opened_before) != _file_identity(path_before):
            raise CommonModelSnapshotErrorV1(f"snapshot file changed before capture: {path}")
        chunks: list[bytes] = []
        total = 0
        while True:
            chunk = os.read(descriptor, min(64 * 1024, maximum_bytes + 1 - total))
            if not chunk:
                break
            chunks.append(chunk)
            total += len(chunk)
            if total > maximum_bytes:
                raise CommonModelSnapshotErrorV1(f"snapshot file exceeds allocation cap: {path}")
        if after_read_hook is not None:
            after_read_hook()
        opened_after = os.fstat(descriptor)
    finally:
        os.close(descriptor)
    try:
        path_after = os.lstat(path)
    except OSError as exc:
        raise CommonModelSnapshotErrorV1(f"snapshot file disappeared during capture: {path}") from exc
    if (
        _file_identity(opened_before) != _file_identity(opened_after)
        or _file_identity(opened_after) != _file_identity(path_after)
        or not stat.S_ISREG(path_after.st_mode)
        or stat.S_ISLNK(path_after.st_mode)
        or _is_reparse(path_after)
    ):
        raise CommonModelSnapshotErrorV1(f"snapshot file changed during capture: {path}")
    data = b"".join(chunks)
    if len(data) != opened_after.st_size:
        raise CommonModelSnapshotErrorV1(f"snapshot file size changed during capture: {path}")
    return data


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def _source_records(repo_root: Path) -> tuple[list[dict[str, str]], str]:
    records: list[dict[str, str]] = []
    framed = bytearray()
    for relative in AUTHORITY_SOURCE_PATHS_V1:
        data = _capture_regular_file(repo_root / relative, SOURCE_MAX_BYTES_V1)
        digest = _sha256(data)
        records.append({"path": relative, "sha256": digest})
        framed.extend(_frame(relative, bytes.fromhex(digest)))
    return records, _sha256(bytes(framed))


def _runtime_configuration() -> dict[str, Any]:
    return {
        "byte_order": sys.byteorder,
        "device": "cpu",
        "platform_machine": platform.machine(),
        "platform_system": platform.system(),
        "python_version": platform.python_version(),
        "torch_default_dtype": str(torch.get_default_dtype()),
        "torch_deterministic_algorithms": torch.are_deterministic_algorithms_enabled(),
        "torch_num_interop_threads": torch.get_num_interop_threads(),
        "torch_num_threads": torch.get_num_threads(),
        "torch_version": torch.__version__,
    }


def _require_authority_runtime() -> dict[str, Any]:
    configure_torch_determinism()
    actual = _runtime_configuration()
    if actual != AUTHORITY_RUNTIME_CONFIGURATION_V1:
        raise CommonModelSnapshotErrorV1(
            f"authority runtime tuple mismatch: expected={AUTHORITY_RUNTIME_CONFIGURATION_V1!r} "
            f"actual={actual!r}"
        )
    return actual


def _parameter_stream_digest(entries: Iterable[tuple[str, tuple[int, ...], bytes]]) -> str:
    digest = hashlib.sha256()
    for name, shape, tensor_bytes in entries:
        name_bytes = name.encode("ascii")
        digest.update(len(name_bytes).to_bytes(4, "big"))
        digest.update(name_bytes)
        digest.update(len(shape).to_bytes(4, "big"))
        for dimension in shape:
            digest.update(dimension.to_bytes(8, "big"))
        digest.update((len(tensor_bytes) // 4).to_bytes(8, "big"))
        digest.update(tensor_bytes)
    return digest.hexdigest()


def _layout_projection(parameters: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "buffers": [],
        "encoding": PAYLOAD_ENCODING_V1,
        "layout": PAYLOAD_LAYOUT_V1,
        "parameter_element_count": PARAMETER_ELEMENT_COUNT_V1,
        "parameter_tensor_count": PARAMETER_TENSOR_COUNT_V1,
        "parameters": [
            {key: value for key, value in entry.items() if key != "tensor_sha256"}
            for entry in parameters
        ],
        "payload_byte_count": PAYLOAD_BYTE_COUNT_V1,
    }


def _manifest_core_sha256(manifest: dict[str, Any]) -> str:
    core = copy.deepcopy(manifest)
    integrity = _require_exact_keys(
        core["integrity"],
        {
            "manifest_core_sha256",
            "named_parameter_stream_sha256",
            "parameter_layout_sha256",
            "snapshot_sha256",
        },
        "integrity",
    )
    del integrity["manifest_core_sha256"]
    del integrity["snapshot_sha256"]
    return _sha256(
        _frame(
            "mtg-kernel-common-model-snapshot-v1/manifest-core",
            canonical_json_bytes(core),
        )
    )


def _snapshot_sha256(manifest_core_sha256: str, payload_sha256: str) -> str:
    return _sha256(
        _frame(
            "mtg-kernel-common-model-snapshot-v1/manifest-core-sha256",
            bytes.fromhex(manifest_core_sha256),
        )
        + _frame(
            "mtg-kernel-common-model-snapshot-v1/payload-sha256",
            bytes.fromhex(payload_sha256),
        )
    )


def generate_authority_snapshot_v1(repo_root: Path | None = None) -> tuple[bytes, bytes]:
    """Generate canonical bytes, refusing every non-authority runtime first."""

    runtime = _require_authority_runtime()
    root = _repo_root() if repo_root is None else Path(repo_root).resolve()
    if derive_model_init_seed(BASE_SEED_V1) != MODEL_INIT_SEED_V1:
        raise CommonModelSnapshotErrorV1("model-init seed derivation drift")
    config = ModelConfig()
    config.validate()
    if config.contract_fingerprint() != MODEL_CONFIG_FINGERPRINT_V1:
        raise CommonModelSnapshotErrorV1("model config fingerprint drift")
    model = KernelPolicyValueNet(
        config,
        initializer=INITIALIZER_TRAINER_SEEDED_V1,
        initializer_seed=MODEL_INIT_SEED_V1,
        configure_runtime=False,
    )
    if list(model.named_buffers()):
        raise CommonModelSnapshotErrorV1("snapshot model unexpectedly has buffers")

    payload = bytearray()
    parameters: list[dict[str, Any]] = []
    stream_entries: list[tuple[str, tuple[int, ...], bytes]] = []
    named = list(model.named_parameters())
    if len(named) != len(EXPECTED_PARAMETER_LAYOUT_V1):
        raise CommonModelSnapshotErrorV1("authority parameter tensor count drift")
    for ordinal, ((name, parameter), expected) in enumerate(zip(named, EXPECTED_PARAMETER_LAYOUT_V1)):
        expected_name, expected_shape, element_offset, element_count = expected
        shape = tuple(int(value) for value in parameter.shape)
        if name != expected_name or shape != expected_shape or parameter.numel() != element_count:
            raise CommonModelSnapshotErrorV1(f"authority parameter layout drift at ordinal {ordinal}")
        if len(payload) != element_offset * 4:
            raise CommonModelSnapshotErrorV1(f"authority parameter gap at ordinal {ordinal}")
        contiguous = parameter.detach().cpu().contiguous()
        if contiguous.dtype is not torch.float32 or not torch.isfinite(contiguous).all():
            raise CommonModelSnapshotErrorV1(f"authority parameter is not finite f32: {name}")
        tensor_bytes = contiguous.numpy().astype("<f4", copy=False).tobytes(order="C")
        if len(tensor_bytes) != element_count * 4:
            raise CommonModelSnapshotErrorV1(f"authority tensor byte count drift: {name}")
        payload.extend(tensor_bytes)
        stream_entries.append((name, shape, tensor_bytes))
        parameters.append(
            {
                "byte_count": element_count * 4,
                "byte_offset": element_offset * 4,
                "element_count": element_count,
                "element_offset": element_offset,
                "name": name,
                "ordinal": ordinal,
                "shape": list(shape),
                "tensor_sha256": _sha256(tensor_bytes),
            }
        )
    payload_bytes = bytes(payload)
    if len(payload_bytes) != PAYLOAD_BYTE_COUNT_V1:
        raise CommonModelSnapshotErrorV1("authority payload byte count drift")
    if payload_bytes[: 16 * 4] != bytes(16 * 4):
        raise CommonModelSnapshotErrorV1("authority padding embedding row is not positive zero")
    scorer_entry = parameters[28]
    scorer_begin = scorer_entry["byte_offset"]
    scorer_bias_anchor = int.from_bytes(payload_bytes[scorer_begin : scorer_begin + 4], "little")

    sources, source_bundle_sha256 = _source_records(root)
    payload_sha256 = _sha256(payload_bytes)
    parameter_layout_sha256 = _sha256(canonical_json_bytes(_layout_projection(parameters)))
    named_stream_sha256 = _parameter_stream_digest(stream_entries)
    runtime_configuration_sha256 = _sha256(canonical_json_bytes(runtime))
    manifest: dict[str, Any] = {
        "schema": SNAPSHOT_SCHEMA_V1,
        "identity": SNAPSHOT_IDENTITY_V1,
        "purpose": SNAPSHOT_PURPOSE_V1,
        "model": {
            "feature_contract_digest": FEATURE_CONTRACT_DIGEST_V1,
            "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V1,
            "model_architecture_version": MODEL_ARCHITECTURE_VERSION,
            "model_config": config.to_dict(),
            "model_config_fingerprint": MODEL_CONFIG_FINGERPRINT_V1,
        },
        "initializer": {
            "authority": INITIALIZER_AUTHORITY_V1,
            "base_seed": BASE_SEED_V1,
            "identity": INITIALIZER_TRAINER_SEEDED_V1,
            "model_init_seed": MODEL_INIT_SEED_V1,
            "python_reference_seed_version": PYTHON_REFERENCE_SEED_VERSION_V1,
            "schedule_goldens_sha256": SCHEDULE_GOLDENS_SHA256_V1,
            "trainer_schedule_version": TRAINER_SCHEDULE_VERSION_V1,
        },
        "authority": {
            "runtime_configuration": runtime,
            "runtime_configuration_sha256": runtime_configuration_sha256,
            "runtime_identity": AUTHORITY_RUNTIME_IDENTITY_V1,
            "source_bundle_contract": SOURCE_BUNDLE_CONTRACT_V1,
            "source_bundle_sha256": source_bundle_sha256,
            "sources": sources,
        },
        "payload": {
            "buffers": [],
            "encoding": PAYLOAD_ENCODING_V1,
            "layout": PAYLOAD_LAYOUT_V1,
            "parameter_element_count": PARAMETER_ELEMENT_COUNT_V1,
            "parameter_tensor_count": PARAMETER_TENSOR_COUNT_V1,
            "payload_byte_count": PAYLOAD_BYTE_COUNT_V1,
            "sha256": payload_sha256,
        },
        "optimizer_bootstrap": {
            "adam_step": 0,
            "canonical_gauge_parameters": list(CANONICAL_GAUGE_PARAMETERS_V1),
            "moment_initialization": MOMENT_INITIALIZATION_V1,
            "optimizer_identity": OPTIMIZER_IDENTITY_V1,
            "scorer_bias_anchor_f32_bits": scorer_bias_anchor,
            "value_head_gauge": VALUE_HEAD_GAUGE_V1,
        },
        "parameters": parameters,
        "integrity": {
            "manifest_core_sha256": "",
            "named_parameter_stream_sha256": named_stream_sha256,
            "parameter_layout_sha256": parameter_layout_sha256,
            "snapshot_sha256": "",
        },
        "nonclaims": {
            "independent_gates": list(INDEPENDENT_GATES_V1),
            "legacy_optimizer": LEGACY_OPTIMIZER_NONCLAIM_V1,
            "scope": NONCLAIM_V1,
        },
    }
    core_sha256 = _manifest_core_sha256(manifest)
    manifest["integrity"]["manifest_core_sha256"] = core_sha256
    manifest["integrity"]["snapshot_sha256"] = _snapshot_sha256(core_sha256, payload_sha256)
    manifest_bytes = canonical_json_bytes(manifest) + b"\n"
    validate_snapshot_bytes_v1(manifest_bytes, payload_bytes, repo_root=root)
    return manifest_bytes, payload_bytes


def _validate_manifest_schema(manifest: dict[str, Any], payload_bytes: bytes, repo_root: Path) -> None:
    _require_exact_keys(
        manifest,
        {
            "schema",
            "identity",
            "purpose",
            "model",
            "initializer",
            "authority",
            "payload",
            "optimizer_bootstrap",
            "parameters",
            "integrity",
            "nonclaims",
        },
        "manifest",
    )
    _require_str(manifest["schema"], SNAPSHOT_SCHEMA_V1, "schema")
    _require_str(manifest["identity"], SNAPSHOT_IDENTITY_V1, "identity")
    _require_str(manifest["purpose"], SNAPSHOT_PURPOSE_V1, "purpose")

    model = _require_exact_keys(
        manifest["model"],
        {
            "feature_contract_digest",
            "feature_encoding_digest",
            "model_architecture_version",
            "model_config",
            "model_config_fingerprint",
        },
        "model",
    )
    _require_str(model["model_architecture_version"], MODEL_ARCHITECTURE_VERSION, "model architecture")
    _require_str(model["model_config_fingerprint"], MODEL_CONFIG_FINGERPRINT_V1, "model fingerprint")
    _require_str(model["feature_contract_digest"], FEATURE_CONTRACT_DIGEST_V1, "feature contract")
    _require_str(model["feature_encoding_digest"], FEATURE_ENCODING_DIGEST_V1, "feature encoding")
    try:
        config = ModelConfig.from_dict(model["model_config"])
    except (TypeError, ValueError) as exc:
        raise CommonModelSnapshotErrorV1("manifest model config is invalid") from exc
    if config.contract_fingerprint() != MODEL_CONFIG_FINGERPRINT_V1:
        raise CommonModelSnapshotErrorV1("manifest model config fingerprint is inconsistent")

    initializer = _require_exact_keys(
        manifest["initializer"],
        {
            "authority",
            "base_seed",
            "identity",
            "model_init_seed",
            "python_reference_seed_version",
            "schedule_goldens_sha256",
            "trainer_schedule_version",
        },
        "initializer",
    )
    _require_str(initializer["authority"], INITIALIZER_AUTHORITY_V1, "initializer authority")
    _require_str(initializer["identity"], INITIALIZER_TRAINER_SEEDED_V1, "initializer identity")
    _require_int(initializer["base_seed"], BASE_SEED_V1, "base seed")
    _require_int(initializer["model_init_seed"], MODEL_INIT_SEED_V1, "model init seed")
    _require_str(initializer["trainer_schedule_version"], TRAINER_SCHEDULE_VERSION_V1, "schedule")
    _require_str(
        initializer["python_reference_seed_version"],
        PYTHON_REFERENCE_SEED_VERSION_V1,
        "Python seed version",
    )
    _require_str(initializer["schedule_goldens_sha256"], SCHEDULE_GOLDENS_SHA256_V1, "schedule goldens")
    if derive_model_init_seed(BASE_SEED_V1) != MODEL_INIT_SEED_V1:
        raise CommonModelSnapshotErrorV1("current schedule no longer derives the frozen model seed")

    authority = _require_exact_keys(
        manifest["authority"],
        {
            "runtime_configuration",
            "runtime_configuration_sha256",
            "runtime_identity",
            "source_bundle_contract",
            "source_bundle_sha256",
            "sources",
        },
        "authority",
    )
    _require_str(authority["runtime_identity"], AUTHORITY_RUNTIME_IDENTITY_V1, "authority runtime")
    runtime = _require_exact_keys(
        authority["runtime_configuration"],
        set(AUTHORITY_RUNTIME_CONFIGURATION_V1),
        "authority.runtime_configuration",
    )
    for key, expected in AUTHORITY_RUNTIME_CONFIGURATION_V1.items():
        if type(expected) is str:
            _require_str(runtime[key], expected, f"authority runtime {key}")
        elif type(expected) is bool:
            _require_bool(runtime[key], expected, f"authority runtime {key}")
        elif type(expected) is int:
            _require_int(runtime[key], expected, f"authority runtime {key}")
        else:
            raise AssertionError(f"unsupported frozen runtime field {key}")
    expected_runtime_digest = _sha256(canonical_json_bytes(AUTHORITY_RUNTIME_CONFIGURATION_V1))
    _require_str(authority["runtime_configuration_sha256"], expected_runtime_digest, "runtime digest")
    _require_str(authority["source_bundle_contract"], SOURCE_BUNDLE_CONTRACT_V1, "source bundle contract")
    current_sources, current_bundle = _source_records(repo_root)
    if type(authority["sources"]) is not list or len(authority["sources"]) != len(
        current_sources
    ):
        raise CommonModelSnapshotErrorV1("authority source list mismatch")
    for index, source in enumerate(authority["sources"]):
        source = _require_exact_keys(source, {"path", "sha256"}, f"authority.sources[{index}]")
        _require_str(source["path"], current_sources[index]["path"], f"authority source {index} path")
        _require_str(
            source["sha256"],
            current_sources[index]["sha256"],
            f"authority source {index} digest",
        )
    if authority["sources"] != current_sources:
        raise CommonModelSnapshotErrorV1("authority source hashes drifted")
    _require_str(authority["source_bundle_sha256"], current_bundle, "source bundle")

    payload = _require_exact_keys(
        manifest["payload"],
        {
            "buffers",
            "encoding",
            "layout",
            "parameter_element_count",
            "parameter_tensor_count",
            "payload_byte_count",
            "sha256",
        },
        "payload",
    )
    if type(payload["buffers"]) is not list or payload["buffers"]:
        raise CommonModelSnapshotErrorV1("payload buffers must be exactly []")
    _require_str(payload["encoding"], PAYLOAD_ENCODING_V1, "payload encoding")
    _require_str(payload["layout"], PAYLOAD_LAYOUT_V1, "payload layout")
    _require_int(payload["parameter_tensor_count"], PARAMETER_TENSOR_COUNT_V1, "tensor count")
    _require_int(payload["parameter_element_count"], PARAMETER_ELEMENT_COUNT_V1, "element count")
    _require_int(payload["payload_byte_count"], PAYLOAD_BYTE_COUNT_V1, "payload byte count")
    if len(payload_bytes) != PAYLOAD_BYTE_COUNT_V1:
        raise CommonModelSnapshotErrorV1("payload file has the wrong exact size")
    payload_digest = _sha256(payload_bytes)
    _require_str(payload["sha256"], payload_digest, "payload digest")

    parameters = manifest["parameters"]
    if type(parameters) is not list or len(parameters) != PARAMETER_TENSOR_COUNT_V1:
        raise CommonModelSnapshotErrorV1("parameter manifest has the wrong tensor count")
    stream_entries: list[tuple[str, tuple[int, ...], bytes]] = []
    expected_element_offset = 0
    expected_byte_offset = 0
    for ordinal, (entry, expected) in enumerate(zip(parameters, EXPECTED_PARAMETER_LAYOUT_V1)):
        item = _require_exact_keys(
            entry,
            {
                "byte_count",
                "byte_offset",
                "element_count",
                "element_offset",
                "name",
                "ordinal",
                "shape",
                "tensor_sha256",
            },
            f"parameters[{ordinal}]",
        )
        expected_name, expected_shape, frozen_offset, expected_count = expected
        _require_int(item["ordinal"], ordinal, f"parameters[{ordinal}].ordinal")
        _require_str(item["name"], expected_name, f"parameters[{ordinal}].name")
        if type(item["shape"]) is not list or item["shape"] != list(expected_shape):
            raise CommonModelSnapshotErrorV1(f"parameters[{ordinal}].shape mismatch")
        for dimension in item["shape"]:
            if type(dimension) is not int or dimension <= 0:
                raise CommonModelSnapshotErrorV1(f"parameters[{ordinal}] has invalid shape")
        _require_int(item["element_offset"], frozen_offset, f"parameters[{ordinal}].element_offset")
        _require_int(item["element_offset"], expected_element_offset, f"parameters[{ordinal}] contiguity")
        _require_int(item["element_count"], expected_count, f"parameters[{ordinal}].element_count")
        expected_product = math.prod(expected_shape)
        if expected_product != expected_count:
            raise CommonModelSnapshotErrorV1(f"parameters[{ordinal}] shape product mismatch")
        byte_offset = expected_element_offset * 4
        byte_count = expected_count * 4
        if byte_offset > (1 << 64) - 1 or byte_count > (1 << 64) - 1:
            raise CommonModelSnapshotErrorV1("parameter layout overflows u64")
        _require_int(item["byte_offset"], byte_offset, f"parameters[{ordinal}].byte_offset")
        _require_int(item["byte_offset"], expected_byte_offset, f"parameters[{ordinal}] byte contiguity")
        _require_int(item["byte_count"], byte_count, f"parameters[{ordinal}].byte_count")
        end = byte_offset + byte_count
        if end > len(payload_bytes):
            raise CommonModelSnapshotErrorV1(f"parameters[{ordinal}] exceeds payload")
        tensor_bytes = payload_bytes[byte_offset:end]
        _require_str(item["tensor_sha256"], _sha256(tensor_bytes), f"parameters[{ordinal}] digest")
        stream_entries.append((expected_name, expected_shape, tensor_bytes))
        expected_element_offset += expected_count
        expected_byte_offset += byte_count
    if expected_element_offset != PARAMETER_ELEMENT_COUNT_V1 or expected_byte_offset != PAYLOAD_BYTE_COUNT_V1:
        raise CommonModelSnapshotErrorV1("parameter manifest final offset mismatch")
    layout_digest = _sha256(canonical_json_bytes(_layout_projection(parameters)))
    named_digest = _parameter_stream_digest(stream_entries)

    for position, (bits,) in enumerate(struct.iter_unpack("<I", payload_bytes)):
        if bits & 0x7F80_0000 == 0x7F80_0000:
            raise CommonModelSnapshotErrorV1(f"payload has NaN or infinity at element {position}")
    if any(bits != 0 for (bits,) in struct.iter_unpack("<I", payload_bytes[: 16 * 4])):
        raise CommonModelSnapshotErrorV1("padding embedding row is not exact positive zero")

    optimizer = _require_exact_keys(
        manifest["optimizer_bootstrap"],
        {
            "adam_step",
            "canonical_gauge_parameters",
            "moment_initialization",
            "optimizer_identity",
            "scorer_bias_anchor_f32_bits",
            "value_head_gauge",
        },
        "optimizer_bootstrap",
    )
    _require_str(optimizer["optimizer_identity"], OPTIMIZER_IDENTITY_V1, "optimizer identity")
    _require_int(optimizer["adam_step"], 0, "Adam step")
    _require_str(optimizer["moment_initialization"], MOMENT_INITIALIZATION_V1, "moment initialization")
    if optimizer["canonical_gauge_parameters"] != CANONICAL_GAUGE_PARAMETERS_V1:
        raise CommonModelSnapshotErrorV1("canonical gauge set mismatch")
    _require_str(optimizer["value_head_gauge"], VALUE_HEAD_GAUGE_V1, "value-head gauge")
    anchor_offset = parameters[28]["byte_offset"]
    anchor_bits = int.from_bytes(payload_bytes[anchor_offset : anchor_offset + 4], "little")
    _require_int(optimizer["scorer_bias_anchor_f32_bits"], anchor_bits, "scorer-bias anchor")

    nonclaims = _require_exact_keys(
        manifest["nonclaims"],
        {"independent_gates", "legacy_optimizer", "scope"},
        "nonclaims",
    )
    _require_str(nonclaims["scope"], NONCLAIM_V1, "scope nonclaim")
    _require_str(nonclaims["legacy_optimizer"], LEGACY_OPTIMIZER_NONCLAIM_V1, "legacy nonclaim")
    if nonclaims["independent_gates"] != INDEPENDENT_GATES_V1:
        raise CommonModelSnapshotErrorV1("independent gate list mismatch")

    integrity = _require_exact_keys(
        manifest["integrity"],
        {
            "manifest_core_sha256",
            "named_parameter_stream_sha256",
            "parameter_layout_sha256",
            "snapshot_sha256",
        },
        "integrity",
    )
    _require_str(integrity["parameter_layout_sha256"], layout_digest, "parameter layout digest")
    _require_str(integrity["named_parameter_stream_sha256"], named_digest, "named stream digest")
    core_digest = _manifest_core_sha256(manifest)
    _require_str(integrity["manifest_core_sha256"], core_digest, "manifest core digest")
    snapshot_digest = _snapshot_sha256(core_digest, payload_digest)
    _require_str(integrity["snapshot_sha256"], snapshot_digest, "snapshot digest")


def validate_snapshot_bytes_v1(
    manifest_file_bytes: bytes,
    payload_bytes: bytes,
    *,
    repo_root: Path | None = None,
) -> ValidatedCommonModelSnapshotV1:
    root = _repo_root() if repo_root is None else Path(repo_root).resolve()
    manifest = _parse_manifest(manifest_file_bytes)
    _validate_manifest_schema(manifest, payload_bytes, root)
    return ValidatedCommonModelSnapshotV1(
        manifest=manifest,
        manifest_file_bytes=manifest_file_bytes,
        payload_bytes=payload_bytes,
        manifest_file_sha256=_sha256(manifest_file_bytes),
    )


def validate_snapshot_files_v1(
    manifest_path: Path,
    payload_path: Path,
    *,
    repo_root: Path | None = None,
) -> ValidatedCommonModelSnapshotV1:
    manifest_bytes = _capture_regular_file(Path(manifest_path), MANIFEST_MAX_BYTES_V1)
    payload_bytes = _capture_regular_file(Path(payload_path), PAYLOAD_MAX_BYTES_V1)
    return validate_snapshot_bytes_v1(manifest_bytes, payload_bytes, repo_root=repo_root)


def _reexport_model_payload(model: KernelPolicyValueNet) -> tuple[bytes, str]:
    payload = bytearray()
    stream: list[tuple[str, tuple[int, ...], bytes]] = []
    named = list(model.named_parameters())
    if len(named) != PARAMETER_TENSOR_COUNT_V1:
        raise CommonModelSnapshotErrorV1("loaded model parameter count drift")
    for (name, parameter), expected in zip(named, EXPECTED_PARAMETER_LAYOUT_V1):
        expected_name, expected_shape, _offset, expected_count = expected
        shape = tuple(int(value) for value in parameter.shape)
        if name != expected_name or shape != expected_shape or parameter.numel() != expected_count:
            raise CommonModelSnapshotErrorV1("loaded model parameter layout drift")
        if parameter.dtype is not torch.float32 or parameter.device.type != "cpu" or not torch.isfinite(parameter).all():
            raise CommonModelSnapshotErrorV1(f"loaded model parameter invalid: {name}")
        tensor_bytes = (
            parameter.detach().contiguous().numpy().astype("<f4", copy=False).tobytes(order="C")
        )
        payload.extend(tensor_bytes)
        stream.append((name, shape, tensor_bytes))
    return bytes(payload), _parameter_stream_digest(stream)


def _snapshot_record(validated: ValidatedCommonModelSnapshotV1, loader_identity: str) -> dict[str, Any]:
    manifest = validated.manifest
    payload = manifest["payload"]
    initializer = manifest["initializer"]
    authority = manifest["authority"]
    model = manifest["model"]
    optimizer = manifest["optimizer_bootstrap"]
    integrity = manifest["integrity"]
    return {
        "adam_step_initial": optimizer["adam_step"],
        "authority_runtime_identity": authority["runtime_identity"],
        "authority_source_bundle_sha256": authority["source_bundle_sha256"],
        "base_seed": initializer["base_seed"],
        "canonical_gauge_parameters": optimizer["canonical_gauge_parameters"],
        "feature_contract_digest": model["feature_contract_digest"],
        "feature_encoding_digest": model["feature_encoding_digest"],
        "identity": manifest["identity"],
        "initializer_identity": initializer["identity"],
        "loaded_named_parameter_stream_sha256": integrity["named_parameter_stream_sha256"],
        "loader_identity": loader_identity,
        "manifest_core_sha256": integrity["manifest_core_sha256"],
        "manifest_file_sha256": validated.manifest_file_sha256,
        "model_architecture_version": model["model_architecture_version"],
        "model_config_fingerprint": model["model_config_fingerprint"],
        "model_init_seed": initializer["model_init_seed"],
        "moment_initialization": optimizer["moment_initialization"],
        "named_parameter_stream_sha256": integrity["named_parameter_stream_sha256"],
        "nonclaim": NONCLAIM_V1,
        "optimizer_identity": optimizer["optimizer_identity"],
        "parameter_element_count": payload["parameter_element_count"],
        "parameter_layout_sha256": integrity["parameter_layout_sha256"],
        "parameter_tensor_count": payload["parameter_tensor_count"],
        "payload_byte_count": payload["payload_byte_count"],
        "payload_sha256": payload["sha256"],
        "python_reference_seed_version": initializer["python_reference_seed_version"],
        "rust_seeded_initializer_reproduced": False,
        "schedule_goldens_sha256": initializer["schedule_goldens_sha256"],
        "schema": manifest["schema"],
        "scorer_bias_anchor_f32_bits": optimizer["scorer_bias_anchor_f32_bits"],
        "snapshot_load_completed_before_trial_start": True,
        "snapshot_load_timed": False,
        "snapshot_sha256": integrity["snapshot_sha256"],
        "trainer_schedule_version": initializer["trainer_schedule_version"],
    }


def build_python_snapshot_candidate_v1(
    manifest_path: Path,
    payload_path: Path,
    *,
    learning_rate: float,
    repo_root: Path | None = None,
) -> tuple[KernelPolicyValueNet, torch.optim.Adam, dict[str, Any]]:
    """Build a complete private model/optimizer candidate without live mutation."""

    import numpy as np

    if type(learning_rate) is not float or not math.isfinite(learning_rate) or learning_rate <= 0.0:
        raise CommonModelSnapshotErrorV1("learning_rate must be a positive finite float")
    validated = validate_snapshot_files_v1(manifest_path, payload_path, repo_root=repo_root)
    candidate = KernelPolicyValueNet(ModelConfig(), initializer=INITIALIZER_RUNNER_FIXED_V1)
    named = list(candidate.named_parameters())
    with torch.no_grad():
        for (name, parameter), entry in zip(named, validated.manifest["parameters"]):
            if name != entry["name"]:
                raise CommonModelSnapshotErrorV1("candidate parameter order drift")
            begin = entry["byte_offset"]
            end = begin + entry["byte_count"]
            decoded = np.frombuffer(validated.payload_bytes[begin:end], dtype="<f4").astype(
                np.float32, copy=True
            )
            parameter.copy_(torch.from_numpy(decoded).reshape(entry["shape"]))
    reexported, loaded_stream_digest = _reexport_model_payload(candidate)
    payload = validated.manifest["payload"]
    if (
        reexported != validated.payload_bytes
        or loaded_stream_digest
        != validated.manifest["integrity"]["named_parameter_stream_sha256"]
    ):
        raise CommonModelSnapshotErrorV1("Python model re-export differs from the snapshot")
    candidate_optimizer = create_adam(candidate, learning_rate)
    for _name, parameter in candidate.named_parameters():
        state = candidate_optimizer.state[parameter]
        state["step"] = torch.tensor(0.0, dtype=torch.float32, device="cpu")
        state["exp_avg"] = torch.zeros_like(parameter, memory_format=torch.preserve_format)
        state["exp_avg_sq"] = torch.zeros_like(parameter, memory_format=torch.preserve_format)
        for key in ("step", "exp_avg", "exp_avg_sq"):
            tensor = state[key]
            if tensor.dtype is not torch.float32 or tensor.device.type != "cpu" or any(
                bits != 0
                for (bits,) in struct.iter_unpack(
                    "<I", tensor.detach().contiguous().numpy().tobytes(order="C")
                )
            ):
                raise CommonModelSnapshotErrorV1(f"optimizer {key} is not positive-zero f32")
    scorer = dict(candidate.named_parameters())["scorer.2.bias"]
    scorer_bits = int.from_bytes(
        scorer.detach().contiguous().numpy().astype("<f4", copy=False).tobytes(),
        "little",
    )
    anchor = validated.manifest["optimizer_bootstrap"]["scorer_bias_anchor_f32_bits"]
    if scorer_bits != anchor:
        raise CommonModelSnapshotErrorV1("loaded scorer-bias anchor drift")
    record = _snapshot_record(validated, PYTHON_LOADER_IDENTITY_V1)
    record["loaded_named_parameter_stream_sha256"] = loaded_stream_digest
    return candidate, candidate_optimizer, record


def load_python_snapshot_into_state_v1(
    state: PythonCommonSnapshotTrainStateV1,
    manifest_path: Path,
    payload_path: Path,
    *,
    learning_rate: float,
    repo_root: Path | None = None,
) -> dict[str, Any]:
    """Atomically replace only model/optimizer/bootstrap fields after all gates."""

    candidate_model, candidate_optimizer, record = build_python_snapshot_candidate_v1(
        manifest_path,
        payload_path,
        learning_rate=learning_rate,
        repo_root=repo_root,
    )
    candidate_anchor = record["scorer_bias_anchor_f32_bits"]
    state.model = candidate_model
    state.optimizer = candidate_optimizer
    state.adam_step = 0
    state.scorer_bias_anchor_f32_bits = candidate_anchor
    state.model_snapshot = copy.deepcopy(record)
    return record


def common_snapshot_default_paths_v1(repo_root: Path | None = None) -> tuple[Path, Path]:
    root = _repo_root() if repo_root is None else Path(repo_root).resolve()
    directory = root / "data" / "common_model_snapshot_v1"
    return directory / "manifest.json", directory / "parameters.f32le"


def write_authority_snapshot_v1(repo_root: Path | None = None) -> tuple[Path, Path]:
    root = _repo_root() if repo_root is None else Path(repo_root).resolve()
    manifest_path, payload_path = common_snapshot_default_paths_v1(root)
    manifest_bytes, payload_bytes = generate_authority_snapshot_v1(root)
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    for path, data in ((manifest_path, manifest_bytes), (payload_path, payload_bytes)):
        temporary = path.with_name(path.name + ".tmp")
        with temporary.open("wb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    return manifest_path, payload_path


def portable_check_v1(repo_root: Path | None = None) -> ValidatedCommonModelSnapshotV1:
    root = _repo_root() if repo_root is None else Path(repo_root).resolve()
    manifest_path, payload_path = common_snapshot_default_paths_v1(root)
    return validate_snapshot_files_v1(manifest_path, payload_path, repo_root=root)


def authority_check_v1(repo_root: Path | None = None) -> ValidatedCommonModelSnapshotV1:
    root = _repo_root() if repo_root is None else Path(repo_root).resolve()
    committed = portable_check_v1(root)
    generated_manifest, generated_payload = generate_authority_snapshot_v1(root)
    if generated_manifest != committed.manifest_file_bytes or generated_payload != committed.payload_bytes:
        raise CommonModelSnapshotErrorV1("authority regeneration is not byte-identical")
    return committed
