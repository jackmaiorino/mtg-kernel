"""Capacity-experiment wide-net (kernel-policy-value-net-8w128) snapshot fork.

Stage-3 Capacity Experiment Contract (hidden 64->128, card embedding 16->32).
This module is a PARAMETERIZE-BY-FORK sibling of ``common_model_snapshot_v1``:
it constructs and validates a second, independent authority snapshot for the
wide architecture identity ``kernel-policy-value-net-8w128``. It does not
import, modify, or otherwise touch the frozen
``kernel-policy-value-net-8`` snapshot, its manifest, or its constants, and it
does not alter ``model.py``, ``features.py``, ``determinism.py``, or
``common_model_snapshot_v1.py`` in any way (those frozen sources' bytes, and
hence their pinned sha256 digests inside the FROZEN manifest, are unaffected
by this file's existence).

Diagnostic, non-evidence: this snapshot does not mint a qualified numerical
identity or a production model. See CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md.
"""

from __future__ import annotations

import copy
import hashlib
import math
import os
import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import torch

from .checkpoint import create_adam
from .determinism import derive_model_init_seed
from .model import (
    INITIALIZER_RUNNER_FIXED_V1,
    INITIALIZER_TRAINER_SEEDED_V1,
    KernelPolicyValueNet,
)
from .features import (
    ACTION_FEATURE_DIM,
    ACTION_REF_FEATURE_DIM,
    CARD_TOKEN_VOCAB_SIZE,
    EDGE_FEATURE_DIM,
    FEATURE_REGISTRY_VERSION,
    FEATURE_SCHEMA_VERSION,
    OBJECT_FEATURE_DIM,
    OBJECT_GROUPS,
    STATE_FEATURE_DIM,
    encoding_contract_fingerprint,
    feature_contract_fingerprint,
)
from .common_model_snapshot_v1 import (
    AUTHORITY_RUNTIME_CONFIGURATION_V1,
    AUTHORITY_RUNTIME_IDENTITY_V1,
    BASE_SEED_V1,
    CANONICAL_GAUGE_PARAMETERS_V1,
    INITIALIZER_AUTHORITY_V1,
    MANIFEST_MAX_BYTES_V1,
    MODEL_INIT_SEED_V1,
    MOMENT_INITIALIZATION_V1,
    OPTIMIZER_IDENTITY_V1,
    PYTHON_REFERENCE_SEED_VERSION_V1,
    SCHEDULE_GOLDENS_SHA256_V1,
    TRAINER_SCHEDULE_VERSION_V1,
    VALUE_HEAD_GAUGE_V1,
    CommonModelSnapshotErrorV1,
    _capture_regular_file,
    _frame,
    _parameter_stream_digest,
    _parse_manifest,
    _repo_root,
    _require_authority_runtime,
    _require_bool,
    _require_exact_keys,
    _require_int,
    _require_str,
    _sha256,
    canonical_json_bytes,
)

# ---------------------------------------------------------------------------
# Wide architecture identity and dimensions (Section 2 of the contract).
# ---------------------------------------------------------------------------
WIDE_MODEL_ARCHITECTURE_VERSION_V1 = "kernel-policy-value-net-8w128"
WIDE_MODEL_CONFIG_SCHEMA_VERSION_V1 = 5
WIDE_HIDDEN_DIM_V1 = 128
WIDE_CARD_EMBEDDING_DIM_V1 = 32
WIDE_MODEL_CONFIG_FINGERPRINT_V1 = (
    "b34c87f46e7709d8b03ee21710d7f0345ff0fcf49ec3d09cf25b94cfe71bf1c6"
)

WIDE_SNAPSHOT_SCHEMA_V1 = "mtg-kernel-common-model-snapshot/v1"
WIDE_SNAPSHOT_IDENTITY_V1 = (
    "mtg-kernel-python-authoritative-wide-model-experiment-snapshot-v1"
)
WIDE_SNAPSHOT_PURPOSE_V1 = (
    "capacity-experiment-wide-net-w128-diagnostic-non-evidence"
)
PYTHON_LOADER_IDENTITY_V1 = "mtg-kernel-python-wide-model-snapshot-loader-v1"
RUST_LOADER_IDENTITY_V1 = "mtg-kernel-rust-wide-model-snapshot-loader-v1"
WIDE_ARCHITECTURE_LABEL_V1 = "WIDE-DIAGNOSTIC-NON-EVIDENCE"

WIDE_PARAMETER_TENSOR_COUNT_V1 = 33
WIDE_PARAMETER_ELEMENT_COUNT_V1 = 2_750_754
WIDE_PAYLOAD_BYTE_COUNT_V1 = 11_003_016
# The frozen PAYLOAD_MAX_BYTES_V1 (8 MiB) is sized for the Net8 4,923,976-byte
# payload; the wide payload (11,003,016 bytes) needs its own, larger cap.
WIDE_PAYLOAD_MAX_BYTES_V1 = 16 * 1024 * 1024
PAYLOAD_ENCODING_V1 = "ieee-754-binary32-little-endian"
PAYLOAD_LAYOUT_V1 = (
    "torch-named-parameters-c-contiguous-row-major-linear-output-input-no-padding-v1"
)
SOURCE_MAX_BYTES_V1 = 8 * 1024 * 1024
SOURCE_BUNDLE_CONTRACT_V1 = (
    "sha256(repeated(frame(source-relative-path,raw32(source-sha256))))"
)
WIDE_NONCLAIM_V1 = (
    "Rust does not reproduce the Python trainer-seeded-v1 initializer in this "
    "snapshot configuration; the snapshot proves bit-exact initial parameters "
    "only and does not establish seeded-initializer parity, cross-runtime "
    "numerical bit parity, learning parity, or speedup. This is the Stage-3 "
    "capacity experiment's wide-net (kernel-policy-value-net-8w128) fork: "
    "diagnostic and non-evidence, not a qualified numerical identity or a "
    "production model. Label: " + WIDE_ARCHITECTURE_LABEL_V1
)
WIDE_LEGACY_OPTIMIZER_NONCLAIM_V1 = (
    "The legacy Python-v3 optimizer is not the matched optimizer lane because it "
    "retains accidental scorer-bias gauge drift."
)
WIDE_INDEPENDENT_GATES_V1 = [
    "exact Torch initializer reproduction",
    "native checkpoint/resume",
    "learning noninferiority",
    "speed ratio",
]

WIDE_AUTHORITY_SOURCE_PATHS_V1 = (
    "python/mtg_kernel_rl/model.py",
    "python/mtg_kernel_rl/features.py",
    "python/mtg_kernel_rl/determinism.py",
    "python/mtg_kernel_rl/wide_model_snapshot_v1.py",
)

# (name, shape, element_offset, element_count) -- computed at
# hidden_dim=128, card_embedding_dim=32, all other dims unchanged from the
# frozen contract. Verified against a live torch construction; see
# python/tools/generate_wide_model_snapshot_v1.py.
WIDE_EXPECTED_PARAMETER_LAYOUT_V1: tuple[tuple[str, tuple[int, ...], int, int], ...] = (
    ("card_embedding.weight", (65537, 32), 0, 2097184),
    ("object_encoder.0.weight", (128, 130), 2097184, 16640),
    ("object_encoder.0.bias", (128,), 2113824, 128),
    ("object_encoder.2.weight", (128, 128), 2113952, 16384),
    ("object_encoder.2.bias", (128,), 2130336, 128),
    ("edge_encoder.0.weight", (128, 297), 2130464, 38016),
    ("edge_encoder.0.bias", (128,), 2168480, 128),
    ("edge_encoder.2.weight", (128, 128), 2168608, 16384),
    ("edge_encoder.2.bias", (128,), 2184992, 128),
    ("node_update.0.weight", (128, 256), 2185120, 32768),
    ("node_update.0.bias", (128,), 2217888, 128),
    ("node_update.2.weight", (128, 128), 2218016, 16384),
    ("node_update.2.bias", (128,), 2234400, 128),
    ("state_encoder.0.weight", (128, 2779), 2234528, 355712),
    ("state_encoder.0.bias", (128,), 2590240, 128),
    ("state_encoder.2.weight", (128, 128), 2590368, 16384),
    ("state_encoder.2.bias", (128,), 2606752, 128),
    ("action_ref_encoder.0.weight", (128, 153), 2606880, 19584),
    ("action_ref_encoder.0.bias", (128,), 2626464, 128),
    ("action_ref_encoder.2.weight", (128, 128), 2626592, 16384),
    ("action_ref_encoder.2.bias", (128,), 2642976, 128),
    ("action_encoder.0.weight", (128, 323), 2643104, 41344),
    ("action_encoder.0.bias", (128,), 2684448, 128),
    ("action_encoder.2.weight", (128, 128), 2684576, 16384),
    ("action_encoder.2.bias", (128,), 2700960, 128),
    ("scorer.0.weight", (128, 256), 2701088, 32768),
    ("scorer.0.bias", (128,), 2733856, 128),
    ("scorer.2.weight", (1, 128), 2733984, 128),
    ("scorer.2.bias", (1,), 2734112, 1),
    ("value_head.0.weight", (128, 128), 2734113, 16384),
    ("value_head.0.bias", (128,), 2750497, 128),
    ("value_head.2.weight", (1, 128), 2750625, 128),
    ("value_head.2.bias", (1,), 2750753, 1),
)

# Ordinal of the scorer-bias canonical gauge parameter. Topology-identical to
# the frozen net (same layer ordering), so this stays 28.
_SCORER_BIAS_ORDINAL_V1 = 28


class WideModelSnapshotErrorV1(CommonModelSnapshotErrorV1):
    """A fail-closed wide-snapshot generation or loading error."""


@dataclass(frozen=True)
class WideModelConfigV1:
    """Duck-type-compatible sibling of ``model.ModelConfig`` for the wide net.

    ``KernelPolicyValueNet.__init__`` only ever accesses ``config`` by
    attribute and calls ``config.validate()``; it does not require
    ``isinstance(config, ModelConfig)``. This dataclass mirrors every field
    ModelConfig has, so the frozen ``KernelPolicyValueNet`` class is reusable
    unmodified for the wide architecture.
    """

    schema_version: int = WIDE_MODEL_CONFIG_SCHEMA_VERSION_V1
    model_architecture_version: str = WIDE_MODEL_ARCHITECTURE_VERSION_V1
    feature_schema_version: str = FEATURE_SCHEMA_VERSION
    feature_registry_version: str = FEATURE_REGISTRY_VERSION
    feature_contract_digest: str = feature_contract_fingerprint()
    feature_encoding_digest: str = encoding_contract_fingerprint()
    card_vocab_size: int = CARD_TOKEN_VOCAB_SIZE
    card_embedding_dim: int = WIDE_CARD_EMBEDDING_DIM_V1
    hidden_dim: int = WIDE_HIDDEN_DIM_V1
    state_dim: int = STATE_FEATURE_DIM
    object_feature_dim: int = OBJECT_FEATURE_DIM
    edge_feature_dim: int = EDGE_FEATURE_DIM
    action_feature_dim: int = ACTION_FEATURE_DIM
    object_group_count: int = len(OBJECT_GROUPS)
    action_ref_feature_dim: int = ACTION_REF_FEATURE_DIM

    def to_dict(self) -> dict[str, int | str]:
        return dict(self.__dict__)

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> "WideModelConfigV1":
        if not isinstance(value, dict):
            raise TypeError("WideModelConfigV1 input must be a primitive dictionary")
        fields = cls.__dataclass_fields__
        expected = set(fields)
        actual = set(value)
        if expected != actual:
            raise ValueError(
                f"WideModelConfigV1 fields mismatch: missing={sorted(expected - actual)} "
                f"extra={sorted(actual - expected)}"
            )
        int_fields = {
            "schema_version",
            "card_vocab_size",
            "card_embedding_dim",
            "hidden_dim",
            "state_dim",
            "object_feature_dim",
            "edge_feature_dim",
            "action_feature_dim",
            "object_group_count",
            "action_ref_feature_dim",
        }
        str_fields = set(fields) - int_fields
        kwargs: dict[str, Any] = {}
        for key in fields:
            raw = value[key]
            if key in int_fields:
                if type(raw) is not int:
                    raise TypeError(f"WideModelConfigV1.{key} must be int")
            elif key in str_fields:
                if type(raw) is not str:
                    raise TypeError(f"WideModelConfigV1.{key} must be str")
            else:
                raise TypeError(f"unsupported WideModelConfigV1 field {key}")
            kwargs[key] = raw
        config = cls(**kwargs)
        config.validate()
        return config

    def validate(self) -> None:
        if self.schema_version != WIDE_MODEL_CONFIG_SCHEMA_VERSION_V1:
            raise ValueError("unsupported WideModelConfigV1 schema_version")
        if self.model_architecture_version != WIDE_MODEL_ARCHITECTURE_VERSION_V1:
            raise ValueError("unsupported wide model architecture version")
        if self.feature_schema_version != FEATURE_SCHEMA_VERSION:
            raise ValueError("feature schema version mismatch")
        if self.feature_registry_version != FEATURE_REGISTRY_VERSION:
            raise ValueError("feature registry version mismatch")
        if self.feature_contract_digest != feature_contract_fingerprint():
            raise ValueError("feature contract digest mismatch")
        if self.feature_encoding_digest != encoding_contract_fingerprint():
            raise ValueError("feature encoding digest mismatch")
        exact_ints = {
            "card_vocab_size": (self.card_vocab_size, CARD_TOKEN_VOCAB_SIZE),
            "card_embedding_dim": (self.card_embedding_dim, WIDE_CARD_EMBEDDING_DIM_V1),
            "hidden_dim": (self.hidden_dim, WIDE_HIDDEN_DIM_V1),
            "state_dim": (self.state_dim, STATE_FEATURE_DIM),
            "object_feature_dim": (self.object_feature_dim, OBJECT_FEATURE_DIM),
            "edge_feature_dim": (self.edge_feature_dim, EDGE_FEATURE_DIM),
            "action_feature_dim": (self.action_feature_dim, ACTION_FEATURE_DIM),
            "object_group_count": (self.object_group_count, len(OBJECT_GROUPS)),
            "action_ref_feature_dim": (self.action_ref_feature_dim, ACTION_REF_FEATURE_DIM),
        }
        for key, (raw, expected) in exact_ints.items():
            if type(raw) is not int or raw != expected:
                raise ValueError(f"WideModelConfigV1.{key} must equal contract value {expected}")

    def contract_fingerprint(self) -> str:
        payload = self.to_dict()
        return hashlib.sha256(
            json_dumps_canonical(payload).encode("utf-8")
        ).hexdigest()


def json_dumps_canonical(payload: dict[str, Any]) -> str:
    import json

    return json.dumps(payload, sort_keys=True, separators=(",", ":"))


@dataclass(frozen=True)
class ValidatedWideModelSnapshotV1:
    manifest: dict[str, Any]
    manifest_file_bytes: bytes
    payload_bytes: bytes
    manifest_file_sha256: str


def _wide_source_records(repo_root: Path) -> tuple[list[dict[str, str]], str]:
    records: list[dict[str, str]] = []
    framed = bytearray()
    for relative in WIDE_AUTHORITY_SOURCE_PATHS_V1:
        data = _capture_regular_file(repo_root / relative, SOURCE_MAX_BYTES_V1)
        digest = _sha256(data)
        records.append({"path": relative, "sha256": digest})
        framed.extend(_frame(relative, bytes.fromhex(digest)))
    return records, _sha256(bytes(framed))


def _wide_layout_projection(parameters: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "buffers": [],
        "encoding": PAYLOAD_ENCODING_V1,
        "layout": PAYLOAD_LAYOUT_V1,
        "parameter_element_count": WIDE_PARAMETER_ELEMENT_COUNT_V1,
        "parameter_tensor_count": WIDE_PARAMETER_TENSOR_COUNT_V1,
        "parameters": [
            {key: value for key, value in entry.items() if key != "tensor_sha256"}
            for entry in parameters
        ],
        "payload_byte_count": WIDE_PAYLOAD_BYTE_COUNT_V1,
    }


def _wide_manifest_core_sha256(manifest: dict[str, Any]) -> str:
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
            "mtg-kernel-wide-model-experiment-v1/manifest-core",
            canonical_json_bytes(core),
        )
    )


def _wide_snapshot_sha256(manifest_core_sha256: str, payload_sha256: str) -> str:
    return _sha256(
        _frame(
            "mtg-kernel-wide-model-experiment-v1/manifest-core-sha256",
            bytes.fromhex(manifest_core_sha256),
        )
        + _frame(
            "mtg-kernel-wide-model-experiment-v1/payload-sha256",
            bytes.fromhex(payload_sha256),
        )
    )


def generate_wide_authority_snapshot_v1(repo_root: Path | None = None) -> tuple[bytes, bytes]:
    """Generate canonical wide-net snapshot bytes, mirroring the frozen path.

    Refuses every non-authority runtime first. Does not touch the frozen
    common-model-snapshot generator, its manifest, or its constants.
    """

    runtime = _require_authority_runtime()
    root = _repo_root() if repo_root is None else Path(repo_root).resolve()
    if derive_model_init_seed(BASE_SEED_V1) != MODEL_INIT_SEED_V1:
        raise WideModelSnapshotErrorV1("model-init seed derivation drift")
    config = WideModelConfigV1()
    config.validate()
    if config.contract_fingerprint() != WIDE_MODEL_CONFIG_FINGERPRINT_V1:
        raise WideModelSnapshotErrorV1("wide model config fingerprint drift")
    model = KernelPolicyValueNet(
        config,
        initializer=INITIALIZER_TRAINER_SEEDED_V1,
        initializer_seed=MODEL_INIT_SEED_V1,
        configure_runtime=False,
    )
    if list(model.named_buffers()):
        raise WideModelSnapshotErrorV1("wide snapshot model unexpectedly has buffers")

    payload = bytearray()
    parameters: list[dict[str, Any]] = []
    stream_entries: list[tuple[str, tuple[int, ...], bytes]] = []
    named = list(model.named_parameters())
    if len(named) != len(WIDE_EXPECTED_PARAMETER_LAYOUT_V1):
        raise WideModelSnapshotErrorV1("wide authority parameter tensor count drift")
    for ordinal, ((name, parameter), expected) in enumerate(
        zip(named, WIDE_EXPECTED_PARAMETER_LAYOUT_V1)
    ):
        expected_name, expected_shape, element_offset, element_count = expected
        shape = tuple(int(value) for value in parameter.shape)
        if name != expected_name or shape != expected_shape or parameter.numel() != element_count:
            raise WideModelSnapshotErrorV1(f"wide authority parameter layout drift at ordinal {ordinal}")
        if len(payload) != element_offset * 4:
            raise WideModelSnapshotErrorV1(f"wide authority parameter gap at ordinal {ordinal}")
        contiguous = parameter.detach().cpu().contiguous()
        if contiguous.dtype is not torch.float32 or not torch.isfinite(contiguous).all():
            raise WideModelSnapshotErrorV1(f"wide authority parameter is not finite f32: {name}")
        tensor_bytes = contiguous.numpy().astype("<f4", copy=False).tobytes(order="C")
        if len(tensor_bytes) != element_count * 4:
            raise WideModelSnapshotErrorV1(f"wide authority tensor byte count drift: {name}")
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
    if len(payload_bytes) != WIDE_PAYLOAD_BYTE_COUNT_V1:
        raise WideModelSnapshotErrorV1("wide authority payload byte count drift")
    if payload_bytes[: WIDE_CARD_EMBEDDING_DIM_V1 * 4] != bytes(WIDE_CARD_EMBEDDING_DIM_V1 * 4):
        raise WideModelSnapshotErrorV1("wide authority padding embedding row is not positive zero")
    scorer_entry = parameters[_SCORER_BIAS_ORDINAL_V1]
    scorer_begin = scorer_entry["byte_offset"]
    scorer_bias_anchor = int.from_bytes(payload_bytes[scorer_begin : scorer_begin + 4], "little")

    sources, source_bundle_sha256 = _wide_source_records(root)
    payload_sha256 = _sha256(payload_bytes)
    parameter_layout_sha256 = _sha256(canonical_json_bytes(_wide_layout_projection(parameters)))
    named_stream_sha256 = _parameter_stream_digest(stream_entries)
    runtime_configuration_sha256 = _sha256(canonical_json_bytes(runtime))
    manifest: dict[str, Any] = {
        "schema": WIDE_SNAPSHOT_SCHEMA_V1,
        "identity": WIDE_SNAPSHOT_IDENTITY_V1,
        "purpose": WIDE_SNAPSHOT_PURPOSE_V1,
        "model": {
            "feature_contract_digest": config.feature_contract_digest,
            "feature_encoding_digest": config.feature_encoding_digest,
            "model_architecture_version": WIDE_MODEL_ARCHITECTURE_VERSION_V1,
            "model_config": config.to_dict(),
            "model_config_fingerprint": WIDE_MODEL_CONFIG_FINGERPRINT_V1,
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
            "parameter_element_count": WIDE_PARAMETER_ELEMENT_COUNT_V1,
            "parameter_tensor_count": WIDE_PARAMETER_TENSOR_COUNT_V1,
            "payload_byte_count": WIDE_PAYLOAD_BYTE_COUNT_V1,
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
            "independent_gates": list(WIDE_INDEPENDENT_GATES_V1),
            "legacy_optimizer": WIDE_LEGACY_OPTIMIZER_NONCLAIM_V1,
            "scope": WIDE_NONCLAIM_V1,
        },
    }
    core_sha256 = _wide_manifest_core_sha256(manifest)
    manifest["integrity"]["manifest_core_sha256"] = core_sha256
    manifest["integrity"]["snapshot_sha256"] = _wide_snapshot_sha256(core_sha256, payload_sha256)
    manifest_bytes = canonical_json_bytes(manifest) + b"\n"
    validate_wide_snapshot_bytes_v1(manifest_bytes, payload_bytes, repo_root=root)
    return manifest_bytes, payload_bytes


def _validate_wide_manifest_schema(
    manifest: dict[str, Any], payload_bytes: bytes, repo_root: Path
) -> None:
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
    _require_str(manifest["schema"], WIDE_SNAPSHOT_SCHEMA_V1, "schema")
    _require_str(manifest["identity"], WIDE_SNAPSHOT_IDENTITY_V1, "identity")
    _require_str(manifest["purpose"], WIDE_SNAPSHOT_PURPOSE_V1, "purpose")

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
    _require_str(model["model_architecture_version"], WIDE_MODEL_ARCHITECTURE_VERSION_V1, "model architecture")
    _require_str(model["model_config_fingerprint"], WIDE_MODEL_CONFIG_FINGERPRINT_V1, "model fingerprint")
    _require_str(model["feature_contract_digest"], feature_contract_fingerprint(), "feature contract")
    _require_str(model["feature_encoding_digest"], encoding_contract_fingerprint(), "feature encoding")
    try:
        config = WideModelConfigV1.from_dict(model["model_config"])
    except (TypeError, ValueError) as exc:
        raise WideModelSnapshotErrorV1("manifest wide model config is invalid") from exc
    if config.contract_fingerprint() != WIDE_MODEL_CONFIG_FINGERPRINT_V1:
        raise WideModelSnapshotErrorV1("manifest wide model config fingerprint is inconsistent")

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
        raise WideModelSnapshotErrorV1("current schedule no longer derives the frozen model seed")

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
    current_sources, current_bundle = _wide_source_records(repo_root)
    if type(authority["sources"]) is not list or len(authority["sources"]) != len(current_sources):
        raise WideModelSnapshotErrorV1("wide authority source list mismatch")
    for index, source in enumerate(authority["sources"]):
        source = _require_exact_keys(source, {"path", "sha256"}, f"authority.sources[{index}]")
        _require_str(source["path"], current_sources[index]["path"], f"authority source {index} path")
        _require_str(
            source["sha256"],
            current_sources[index]["sha256"],
            f"authority source {index} digest",
        )
    if authority["sources"] != current_sources:
        raise WideModelSnapshotErrorV1("wide authority source hashes drifted")
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
        raise WideModelSnapshotErrorV1("payload buffers must be exactly []")
    _require_str(payload["encoding"], PAYLOAD_ENCODING_V1, "payload encoding")
    _require_str(payload["layout"], PAYLOAD_LAYOUT_V1, "payload layout")
    _require_int(payload["parameter_tensor_count"], WIDE_PARAMETER_TENSOR_COUNT_V1, "tensor count")
    _require_int(payload["parameter_element_count"], WIDE_PARAMETER_ELEMENT_COUNT_V1, "element count")
    _require_int(payload["payload_byte_count"], WIDE_PAYLOAD_BYTE_COUNT_V1, "payload byte count")
    if len(payload_bytes) != WIDE_PAYLOAD_BYTE_COUNT_V1:
        raise WideModelSnapshotErrorV1("payload file has the wrong exact size")
    payload_digest = _sha256(payload_bytes)
    _require_str(payload["sha256"], payload_digest, "payload digest")

    parameters = manifest["parameters"]
    if type(parameters) is not list or len(parameters) != WIDE_PARAMETER_TENSOR_COUNT_V1:
        raise WideModelSnapshotErrorV1("parameter manifest has the wrong tensor count")
    stream_entries: list[tuple[str, tuple[int, ...], bytes]] = []
    expected_element_offset = 0
    expected_byte_offset = 0
    for ordinal, (entry, expected) in enumerate(zip(parameters, WIDE_EXPECTED_PARAMETER_LAYOUT_V1)):
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
            raise WideModelSnapshotErrorV1(f"parameters[{ordinal}].shape mismatch")
        for dimension in item["shape"]:
            if type(dimension) is not int or dimension <= 0:
                raise WideModelSnapshotErrorV1(f"parameters[{ordinal}] has invalid shape")
        _require_int(item["element_offset"], frozen_offset, f"parameters[{ordinal}].element_offset")
        _require_int(item["element_offset"], expected_element_offset, f"parameters[{ordinal}] contiguity")
        _require_int(item["element_count"], expected_count, f"parameters[{ordinal}].element_count")
        expected_product = math.prod(expected_shape)
        if expected_product != expected_count:
            raise WideModelSnapshotErrorV1(f"parameters[{ordinal}] shape product mismatch")
        byte_offset = expected_element_offset * 4
        byte_count = expected_count * 4
        if byte_offset > (1 << 64) - 1 or byte_count > (1 << 64) - 1:
            raise WideModelSnapshotErrorV1("parameter layout overflows u64")
        _require_int(item["byte_offset"], byte_offset, f"parameters[{ordinal}].byte_offset")
        _require_int(item["byte_offset"], expected_byte_offset, f"parameters[{ordinal}] byte contiguity")
        _require_int(item["byte_count"], byte_count, f"parameters[{ordinal}].byte_count")
        end = byte_offset + byte_count
        if end > len(payload_bytes):
            raise WideModelSnapshotErrorV1(f"parameters[{ordinal}] exceeds payload")
        tensor_bytes = payload_bytes[byte_offset:end]
        _require_str(item["tensor_sha256"], _sha256(tensor_bytes), f"parameters[{ordinal}] digest")
        stream_entries.append((expected_name, expected_shape, tensor_bytes))
        expected_element_offset += expected_count
        expected_byte_offset += byte_count
    if (
        expected_element_offset != WIDE_PARAMETER_ELEMENT_COUNT_V1
        or expected_byte_offset != WIDE_PAYLOAD_BYTE_COUNT_V1
    ):
        raise WideModelSnapshotErrorV1("parameter manifest final offset mismatch")
    layout_digest = _sha256(canonical_json_bytes(_wide_layout_projection(parameters)))
    named_digest = _parameter_stream_digest(stream_entries)

    for position, (bits,) in enumerate(struct.iter_unpack("<I", payload_bytes)):
        if bits & 0x7F80_0000 == 0x7F80_0000:
            raise WideModelSnapshotErrorV1(f"payload has NaN or infinity at element {position}")
    padding_bytes = WIDE_CARD_EMBEDDING_DIM_V1 * 4
    if any(bits != 0 for (bits,) in struct.iter_unpack("<I", payload_bytes[:padding_bytes])):
        raise WideModelSnapshotErrorV1("padding embedding row is not exact positive zero")

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
        raise WideModelSnapshotErrorV1("canonical gauge set mismatch")
    _require_str(optimizer["value_head_gauge"], VALUE_HEAD_GAUGE_V1, "value-head gauge")
    anchor_offset = parameters[_SCORER_BIAS_ORDINAL_V1]["byte_offset"]
    anchor_bits = int.from_bytes(payload_bytes[anchor_offset : anchor_offset + 4], "little")
    _require_int(optimizer["scorer_bias_anchor_f32_bits"], anchor_bits, "scorer-bias anchor")

    nonclaims = _require_exact_keys(
        manifest["nonclaims"],
        {"independent_gates", "legacy_optimizer", "scope"},
        "nonclaims",
    )
    _require_str(nonclaims["scope"], WIDE_NONCLAIM_V1, "scope nonclaim")
    _require_str(nonclaims["legacy_optimizer"], WIDE_LEGACY_OPTIMIZER_NONCLAIM_V1, "legacy nonclaim")
    if nonclaims["independent_gates"] != WIDE_INDEPENDENT_GATES_V1:
        raise WideModelSnapshotErrorV1("independent gate list mismatch")

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
    core_digest = _wide_manifest_core_sha256(manifest)
    _require_str(integrity["manifest_core_sha256"], core_digest, "manifest core digest")
    snapshot_digest = _wide_snapshot_sha256(core_digest, payload_digest)
    _require_str(integrity["snapshot_sha256"], snapshot_digest, "snapshot digest")


def validate_wide_snapshot_bytes_v1(
    manifest_file_bytes: bytes,
    payload_bytes: bytes,
    *,
    repo_root: Path | None = None,
) -> ValidatedWideModelSnapshotV1:
    root = _repo_root() if repo_root is None else Path(repo_root).resolve()
    manifest = _parse_manifest(manifest_file_bytes)
    _validate_wide_manifest_schema(manifest, payload_bytes, root)
    return ValidatedWideModelSnapshotV1(
        manifest=manifest,
        manifest_file_bytes=manifest_file_bytes,
        payload_bytes=payload_bytes,
        manifest_file_sha256=_sha256(manifest_file_bytes),
    )


def validate_wide_snapshot_files_v1(
    manifest_path: Path,
    payload_path: Path,
    *,
    repo_root: Path | None = None,
) -> ValidatedWideModelSnapshotV1:
    manifest_bytes = _capture_regular_file(Path(manifest_path), MANIFEST_MAX_BYTES_V1)
    payload_bytes = _capture_regular_file(Path(payload_path), WIDE_PAYLOAD_MAX_BYTES_V1)
    return validate_wide_snapshot_bytes_v1(manifest_bytes, payload_bytes, repo_root=repo_root)


def wide_snapshot_default_paths_v1(repo_root: Path | None = None) -> tuple[Path, Path]:
    root = _repo_root() if repo_root is None else Path(repo_root).resolve()
    directory = root / "data" / "wide_model_snapshot_w128"
    return directory / "manifest.json", directory / "parameters.f32le"


def write_wide_authority_snapshot_v1(repo_root: Path | None = None) -> tuple[Path, Path]:
    root = _repo_root() if repo_root is None else Path(repo_root).resolve()
    manifest_path, payload_path = wide_snapshot_default_paths_v1(root)
    manifest_bytes, payload_bytes = generate_wide_authority_snapshot_v1(root)
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    for path, data in ((manifest_path, manifest_bytes), (payload_path, payload_bytes)):
        temporary = path.with_name(path.name + ".tmp")
        with temporary.open("wb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    return manifest_path, payload_path


def wide_portable_check_v1(repo_root: Path | None = None) -> ValidatedWideModelSnapshotV1:
    root = _repo_root() if repo_root is None else Path(repo_root).resolve()
    manifest_path, payload_path = wide_snapshot_default_paths_v1(root)
    return validate_wide_snapshot_files_v1(manifest_path, payload_path, repo_root=root)


def wide_authority_check_v1(repo_root: Path | None = None) -> ValidatedWideModelSnapshotV1:
    root = _repo_root() if repo_root is None else Path(repo_root).resolve()
    committed = wide_portable_check_v1(root)
    generated_manifest, generated_payload = generate_wide_authority_snapshot_v1(root)
    if generated_manifest != committed.manifest_file_bytes or generated_payload != committed.payload_bytes:
        raise WideModelSnapshotErrorV1("wide authority regeneration is not byte-identical")
    return committed


def build_wide_python_snapshot_candidate_v1(
    manifest_path: Path,
    payload_path: Path,
    *,
    learning_rate: float,
    repo_root: Path | None = None,
) -> tuple[KernelPolicyValueNet, torch.optim.Adam, dict[str, Any]]:
    """Build a complete private wide model/optimizer candidate, no live mutation."""

    import numpy as np

    if type(learning_rate) is not float or not math.isfinite(learning_rate) or learning_rate <= 0.0:
        raise WideModelSnapshotErrorV1("learning_rate must be a positive finite float")
    validated = validate_wide_snapshot_files_v1(manifest_path, payload_path, repo_root=repo_root)
    candidate = KernelPolicyValueNet(WideModelConfigV1(), initializer=INITIALIZER_RUNNER_FIXED_V1)
    named = list(candidate.named_parameters())
    with torch.no_grad():
        for (name, parameter), entry in zip(named, validated.manifest["parameters"]):
            if name != entry["name"]:
                raise WideModelSnapshotErrorV1("candidate parameter order drift")
            begin = entry["byte_offset"]
            end = begin + entry["byte_count"]
            decoded = np.frombuffer(validated.payload_bytes[begin:end], dtype="<f4").astype(
                np.float32, copy=True
            )
            parameter.copy_(torch.from_numpy(decoded).reshape(entry["shape"]))
    reexported, loaded_stream_digest = _reexport_wide_model_payload(candidate)
    if (
        reexported != validated.payload_bytes
        or loaded_stream_digest != validated.manifest["integrity"]["named_parameter_stream_sha256"]
    ):
        raise WideModelSnapshotErrorV1("Python wide model re-export differs from the snapshot")
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
                raise WideModelSnapshotErrorV1(f"optimizer {key} is not positive-zero f32")
    scorer = dict(candidate.named_parameters())["scorer.2.bias"]
    scorer_bits = int.from_bytes(
        scorer.detach().contiguous().numpy().astype("<f4", copy=False).tobytes(),
        "little",
    )
    anchor = validated.manifest["optimizer_bootstrap"]["scorer_bias_anchor_f32_bits"]
    if scorer_bits != anchor:
        raise WideModelSnapshotErrorV1("loaded wide scorer-bias anchor drift")
    record = _wide_snapshot_record(validated, PYTHON_LOADER_IDENTITY_V1)
    record["loaded_named_parameter_stream_sha256"] = loaded_stream_digest
    return candidate, candidate_optimizer, record


def _reexport_wide_model_payload(model: KernelPolicyValueNet) -> tuple[bytes, str]:
    payload = bytearray()
    stream: list[tuple[str, tuple[int, ...], bytes]] = []
    named = list(model.named_parameters())
    if len(named) != WIDE_PARAMETER_TENSOR_COUNT_V1:
        raise WideModelSnapshotErrorV1("loaded wide model parameter count drift")
    for (name, parameter), expected in zip(named, WIDE_EXPECTED_PARAMETER_LAYOUT_V1):
        expected_name, expected_shape, _offset, expected_count = expected
        shape = tuple(int(value) for value in parameter.shape)
        if name != expected_name or shape != expected_shape or parameter.numel() != expected_count:
            raise WideModelSnapshotErrorV1("loaded wide model parameter layout drift")
        if parameter.dtype is not torch.float32 or parameter.device.type != "cpu" or not torch.isfinite(parameter).all():
            raise WideModelSnapshotErrorV1(f"loaded wide model parameter invalid: {name}")
        tensor_bytes = (
            parameter.detach().contiguous().numpy().astype("<f4", copy=False).tobytes(order="C")
        )
        payload.extend(tensor_bytes)
        stream.append((name, shape, tensor_bytes))
    return bytes(payload), _parameter_stream_digest(stream)


def _wide_snapshot_record(validated: ValidatedWideModelSnapshotV1, loader_identity: str) -> dict[str, Any]:
    manifest = validated.manifest
    payload = manifest["payload"]
    initializer = manifest["initializer"]
    authority = manifest["authority"]
    model = manifest["model"]
    optimizer = manifest["optimizer_bootstrap"]
    integrity = manifest["integrity"]
    return {
        "adam_step_initial": optimizer["adam_step"],
        "architecture_label": WIDE_ARCHITECTURE_LABEL_V1,
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
        "nonclaim": WIDE_NONCLAIM_V1,
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
