"""Evaluate the genuine native K=512 scalar-loss reduction against Torch.

The Rust capture is a clean-revision, single-update Rally/Rally production
term stream. This authority reconstructs the exact per-group binary32 terms,
requires the portable sequential reconstruction to match the production Rust
bits, then computes the pinned ``torch.stack(...).sum()`` reduction. Frozen
5e-5 absolute/relative tolerances are never changed. Policy and value sum
channels additionally require a predeclared 5x tolerance margin; falling below
that floor produces the stable ``repair_required`` diagnostic after the full
evidence artifact has been written.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import struct
import subprocess
import sys
from typing import Any

import torch


ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / "python" / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))
if str(ROOT / "python") not in sys.path:
    sys.path.insert(0, str(ROOT / "python"))

# This import establishes and verifies the previously frozen single-thread
# deterministic authority topology and gives us the exact production Python
# loss function whose stack/sum order is authoritative.
import generate_native_policy_train_step_v1_goldens as train_fixture  # noqa: E402
from mtg_kernel_rl.trainer import _compute_loss_tensors  # noqa: E402


SCHEMA = "native-policy-loss-reduction-true-k512-gate-v1"
IDENTITY = "torch-stack-vs-rust-production-sequential-genuine-rally-k512-v1"
CAPTURE_SCHEMA = "native-policy-loss-reduction-true-k512-capture-v1"
CAPTURE_IDENTITY = "native-production-sequential-genuine-rally-k512-v1"
CAPTURE_PATH = (
    ROOT
    / "data"
    / "native_policy_train_step_v1"
    / "loss_reduction_true_k512_capture_v1.json"
)
OUTPUT = (
    ROOT
    / "data"
    / "native_policy_train_step_v1"
    / "loss_reduction_true_k512_gate_v1.json"
)
GENERATOR = Path(__file__).resolve()
CAPTURE_HARNESS = (
    ROOT
    / "mtg-kernel"
    / "examples"
    / "native_trainer_true_k512_loss_capture_v1.rs"
)
TRAINER_SOURCE = ROOT / "python" / "mtg_kernel_rl" / "trainer.py"
SNAPSHOT_MANIFEST = ROOT / "data" / "common_model_snapshot_v1" / "manifest.json"
SNAPSHOT_PAYLOAD = ROOT / "data" / "common_model_snapshot_v1" / "parameters.f32le"
INTERMEDIATE_RUNG = (
    ROOT
    / "data"
    / "native_policy_train_step_v1"
    / "loss_reduction_intermediate_rung_v1.json"
)
INTERMEDIATE_GENERATOR = (
    ROOT / "python" / "tools" / "generate_native_policy_loss_reduction_rung_v1.py"
)

EXPECTED_K = 512
EXPECTED_RUN_BASE_SEED = 71_501
EXPECTED_DECK_IDS = ["Rally", "Rally"]
EXPECTED_TRAINER_CONTRACT = "mtg-kernel-native-even-batch-trainer-v2"
EXPECTED_NUMERICAL_BACKEND = (
    "rust-production-native-policy-train-step-v1-cpu-ieee754-binary32-sequential"
)
EXPECTED_COMPOSITION_IDENTITY = (
    "production-native-training-executor-single-update-genuine-rally-v1"
)
VALUE_COEFFICIENT = 0.5
VALUE_COEFFICIENT_BITS = "0x3f000000"
LEARNING_RATE_BITS = "0x3a83126f"
LOSS_ABSOLUTE_TOLERANCE = 5.0e-5
LOSS_RELATIVE_TOLERANCE = 5.0e-5
SUM_MARGIN_FLOOR = 5.0
SUM_CHANNELS = ("policy_sum", "value_sum")
ALL_CHANNELS = ("policy_sum", "value_sum", "loss")
EMPTY_SHA256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
STRICT_SOURCE_RECIPE_IDENTITY = "mtg-kernel-strict-source-tree-sha256-v1"
STRICT_SOURCE_RECIPE_SHA256 = (
    "13ab31b8e4810d683007182d1b5fc3b76db0b9761c877a6e78880c0cadf3fece"
)
TRACKED_TREE_HASH_CONTRACT = (
    "git-ls-tree-r-z-path-mode-type-framed-blob-content-or-gitlink-oid-sha256/v1"
)
REPAIR_DIAGNOSTIC = "true_k512_scalar_loss_reduction_repair_required"
PASS_DIAGNOSTIC = "true_k512_scalar_loss_reduction_pass"
TERM_STREAM_FRAMING = "for group_index in production order: u64be(group_index)||u32be(joint_log_probability_f32_bits)||u32be(value_f32_bits)||i8_twos_complement(terminal_return)"
EPISODE_STREAM_FRAMING = "for episode ordinal in production order: u64be(ordinal)||u64be(episode_index)||u64be(environment_seed)||u64be(deck_hash_p0)||u64be(deck_hash_p1)||u8(learner_seat)||i8_twos_complement(learner_return)||u64be(learner_group_count)||u64be(learner_policy_step_count)||raw32(full_trajectory_sha256)"
SELECTED_STREAM_FRAMING = "for selected output in production order: u64be(group_index)||u64be(substep_index)||u64be(selected_action_index)||u32be(selected_logit_f32_bits)||u32be(value_f32_bits)||u32be(selected_log_probability_f32_bits)"


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise RuntimeError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _load_strict(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_bytes(), object_pairs_hook=_strict_object)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError(f"strict JSON load failed: {path.name}") from error
    if not isinstance(value, dict):
        raise RuntimeError(f"top-level JSON value must be an object: {path.name}")
    return value


def _require_keys(value: dict[str, Any], keys: set[str], context: str) -> None:
    if set(value) != keys:
        raise RuntimeError(f"{context} keys drifted")


def _require_hex(value: Any, length: int, context: str) -> str:
    if (
        not isinstance(value, str)
        or len(value) != length
        or any(character not in "0123456789abcdef" for character in value)
    ):
        raise RuntimeError(f"{context} is not canonical lowercase hex")
    return value


def _require_f32_bits(value: Any, context: str) -> str:
    if (
        not isinstance(value, str)
        or len(value) != 10
        or not value.startswith("0x")
        or any(character not in "0123456789abcdef" for character in value[2:])
    ):
        raise RuntimeError(f"{context} is not canonical f32 bits")
    return value


def _from_bits_hex(value: Any, context: str) -> float:
    bits = _require_f32_bits(value, context)
    scalar = struct.unpack("<f", struct.pack("<I", int(bits[2:], 16)))[0]
    if not math.isfinite(scalar):
        raise RuntimeError(f"{context} is non-finite")
    return scalar


def _f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


def _f32_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]


def _bits_hex(value: float) -> str:
    return f"0x{_f32_bits(value):08x}"


def _f32_add(left: float, right: float) -> float:
    return _f32(float(left) + float(right))


def _f32_sub(left: float, right: float) -> float:
    return _f32(float(left) - float(right))


def _f32_mul(left: float, right: float) -> float:
    return _f32(float(left) * float(right))


def _f32_div(left: float, right: float) -> float:
    return _f32(float(left) / float(right))


def _scalar(value: float) -> dict[str, Any]:
    value = _f32(value)
    if not math.isfinite(value):
        raise RuntimeError("scalar is non-finite")
    return {"value": value, "f32_bits": _bits_hex(value)}


def _sanitized_git_environment() -> dict[str, str]:
    return {
        name: value
        for name, value in os.environ.items()
        if not (len(name) >= 4 and name[:4].lower() == "git_")
    }


def _git_bytes(*args: str, input_bytes: bytes | None = None) -> bytes:
    completed = subprocess.run(
        ["git", "-C", str(ROOT), *args],
        input=input_bytes,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=_sanitized_git_environment(),
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError("capture source Git object is unavailable")
    return completed.stdout


def _git_text(*args: str) -> str:
    try:
        text = _git_bytes(*args).decode("ascii").rstrip("\r\n")
    except UnicodeDecodeError as error:
        raise RuntimeError("capture source Git output is not ASCII") from error
    if not text or "\r" in text or "\n" in text:
        raise RuntimeError("capture source Git output is malformed")
    return text


def _hash_frame(digest: Any, value: bytes) -> None:
    digest.update(len(value).to_bytes(8, "big"))
    digest.update(value)


def _strict_source_projection(
    source_commit: str,
) -> tuple[str, dict[bytes, bytes]]:
    top = Path(_git_text("rev-parse", "--show-toplevel")).resolve()
    if top != ROOT.resolve():
        raise RuntimeError("sanitized Git repository root mismatch")
    actual_commit = _git_text("rev-parse", "--verify", f"{source_commit}^{{commit}}")
    if actual_commit != source_commit:
        raise RuntimeError("capture source commit object mismatch")
    listing = _git_bytes("ls-tree", "-r", "-z", "--full-tree", source_commit)
    if listing and not listing.endswith(b"\0"):
        raise RuntimeError("strict source tree listing lacks NUL terminator")
    records = listing[:-1].split(b"\0") if listing else []
    entries: list[tuple[bytes, bytes, str, bytes]] = []
    seen_paths: set[bytes] = set()
    for record in records:
        if not record or b"\t" not in record:
            raise RuntimeError("strict source tree record is malformed")
        metadata, path = record.split(b"\t", 1)
        fields = metadata.split(b" ")
        if len(fields) != 3 or not path:
            raise RuntimeError("strict source tree metadata is malformed")
        mode, kind, raw_oid = fields
        try:
            oid = raw_oid.decode("ascii")
        except UnicodeDecodeError as error:
            raise RuntimeError("strict source object id is not ASCII") from error
        _require_hex(oid, 40, "strict source object id")
        supported = (kind == b"blob" and mode in (b"100644", b"100755", b"120000")) or (
            kind == b"commit" and mode == b"160000"
        )
        if not supported or path in seen_paths:
            raise RuntimeError("strict source tree entry is unsupported or duplicated")
        seen_paths.add(path)
        entries.append((mode, kind, oid, path))

    blob_entries = [entry for entry in entries if entry[1] == b"blob"]
    batch_input = b"".join(entry[2].encode("ascii") + b"\n" for entry in blob_entries)
    batch_output = (
        _git_bytes("cat-file", "--batch", input_bytes=batch_input)
        if blob_entries
        else b""
    )
    cursor = 0
    blob_contents: dict[str, bytes] = {}
    for _mode, _kind, expected_oid, _path in blob_entries:
        header_end = batch_output.find(b"\n", cursor)
        if header_end < 0:
            raise RuntimeError("strict source blob header is truncated")
        try:
            header = batch_output[cursor:header_end].decode("ascii")
        except UnicodeDecodeError as error:
            raise RuntimeError("strict source blob header is not ASCII") from error
        fields = header.split(" ")
        if len(fields) != 3 or fields[0] != expected_oid or fields[1] != "blob":
            raise RuntimeError("strict source blob metadata mismatch")
        try:
            size = int(fields[2], 10)
        except ValueError as error:
            raise RuntimeError("strict source blob size is invalid") from error
        if size < 0:
            raise RuntimeError("strict source blob size is negative")
        content_start = header_end + 1
        content_end = content_start + size
        if content_end >= len(batch_output) or batch_output[content_end] != 0x0A:
            raise RuntimeError("strict source blob content is truncated")
        blob_contents[expected_oid] = batch_output[content_start:content_end]
        cursor = content_end + 1
    if cursor != len(batch_output):
        raise RuntimeError("strict source blob batch has unconsumed output")

    digest = hashlib.sha256()
    digest.update(TRACKED_TREE_HASH_CONTRACT.encode("ascii"))
    digest.update(b"\0")
    digest.update(len(entries).to_bytes(8, "big"))
    blobs_by_path: dict[bytes, bytes] = {}
    for mode, kind, oid, path in entries:
        _hash_frame(digest, path)
        _hash_frame(digest, mode)
        _hash_frame(digest, kind)
        if kind == b"blob":
            content = blob_contents.get(oid)
            if content is None:
                raise RuntimeError("strict source blob content is missing")
            _hash_frame(digest, content)
            blobs_by_path[path] = content
        else:
            _hash_frame(digest, oid.encode("ascii"))
    return digest.hexdigest(), blobs_by_path


def _capture_harness_blob(
    source_commit: str, source_tree_sha256: str
) -> tuple[bytes, str]:
    actual_tree_sha256, blobs_by_path = _strict_source_projection(source_commit)
    if actual_tree_sha256 != source_tree_sha256:
        raise RuntimeError("capture strict source-tree digest binding failed")
    path = CAPTURE_HARNESS.relative_to(ROOT).as_posix().encode("utf-8")
    blob = blobs_by_path.get(path)
    if blob is None or blob != CAPTURE_HARNESS.read_bytes():
        raise RuntimeError("capture harness differs from source-commit blob")
    return blob, hashlib.sha256(blob).hexdigest()


def _snapshot_expectations() -> dict[str, Any]:
    manifest = _load_strict(SNAPSHOT_MANIFEST)
    return {
        "schema": manifest["schema"],
        "identity": manifest["identity"],
        "snapshot_sha256": manifest["integrity"]["snapshot_sha256"],
        "manifest_file_sha256": _sha256(SNAPSHOT_MANIFEST),
        "manifest_core_sha256": manifest["integrity"]["manifest_core_sha256"],
        "payload_sha256": _sha256(SNAPSHOT_PAYLOAD),
        "payload_byte_count": SNAPSHOT_PAYLOAD.stat().st_size,
        "parameter_layout_sha256": manifest["integrity"]["parameter_layout_sha256"],
        "named_parameter_stream_sha256": manifest["integrity"][
            "named_parameter_stream_sha256"
        ],
        "loaded_named_parameter_stream_sha256": manifest["integrity"][
            "named_parameter_stream_sha256"
        ],
        "model_config_fingerprint": manifest["model"]["model_config_fingerprint"],
        "model_architecture_version": manifest["model"]["model_architecture_version"],
        "feature_contract_digest": manifest["model"]["feature_contract_digest"],
        "feature_encoding_digest": manifest["model"]["feature_encoding_digest"],
        "initializer_identity": manifest["initializer"]["identity"],
        "base_seed": manifest["initializer"]["base_seed"],
        "model_init_seed": manifest["initializer"]["model_init_seed"],
        "trainer_schedule_version": manifest["initializer"]["trainer_schedule_version"],
        "python_reference_seed_version": manifest["initializer"][
            "python_reference_seed_version"
        ],
        "schedule_goldens_sha256": manifest["initializer"]["schedule_goldens_sha256"],
        "authority_source_bundle_sha256": manifest["authority"]["source_bundle_sha256"],
        "authority_runtime_identity": manifest["authority"]["runtime_identity"],
        "loader_identity": "mtg-kernel-rust-common-model-snapshot-loader-v1",
        "optimizer_identity": manifest["optimizer_bootstrap"]["optimizer_identity"],
        "adam_step_initial": manifest["optimizer_bootstrap"]["adam_step"],
        "scorer_bias_anchor_f32_bits": manifest["optimizer_bootstrap"][
            "scorer_bias_anchor_f32_bits"
        ],
    }


def _episode_stream_digest(records: list[dict[str, Any]]) -> str:
    digest = hashlib.sha256()
    for ordinal, record in enumerate(records):
        if record.get("ordinal") != ordinal or record.get("episode_index") != ordinal:
            raise RuntimeError("capture episode ordinals are not contiguous")
        seat = record.get("learner_seat")
        if seat not in ("p0", "p1"):
            raise RuntimeError("capture learner seat is invalid")
        learner_return = record.get("learner_return")
        if type(learner_return) is not int or learner_return not in (-1, 0, 1):
            raise RuntimeError("capture learner return is invalid")
        deck_hashes = record.get("deck_hashes")
        if (
            not isinstance(deck_hashes, list)
            or len(deck_hashes) != 2
            or any(type(value) is not int or value < 0 or value >= 1 << 64 for value in deck_hashes)
        ):
            raise RuntimeError("capture episode deck hashes are invalid")
        trajectory = bytes.fromhex(
            _require_hex(record.get("full_trajectory_sha256"), 64, "trajectory sha256")
        )
        values = (
            ordinal,
            record.get("episode_index"),
            record.get("environment_seed"),
            deck_hashes[0],
            deck_hashes[1],
        )
        if any(type(value) is not int or value < 0 or value >= 1 << 64 for value in values):
            raise RuntimeError("capture episode u64 field is invalid")
        for value in values:
            digest.update(value.to_bytes(8, "big"))
        digest.update(bytes([0 if seat == "p0" else 1]))
        digest.update(bytes([learner_return & 0xFF]))
        for field in ("learner_group_count", "learner_policy_step_count"):
            value = record.get(field)
            if type(value) is not int or value <= 0 or value >= 1 << 64:
                raise RuntimeError(f"capture episode {field} is invalid")
            digest.update(value.to_bytes(8, "big"))
        digest.update(trajectory)
    return digest.hexdigest()


def _term_stream_digest(terms: list[dict[str, Any]]) -> str:
    digest = hashlib.sha256()
    for group_index, term in enumerate(terms):
        if term.get("group_index") != group_index:
            raise RuntimeError("capture term group indices are not contiguous")
        joint_bits = _require_f32_bits(
            term.get("joint_log_probability_f32_bits"), "joint log probability"
        )
        value_bits = _require_f32_bits(term.get("value_f32_bits"), "term value")
        terminal_return = term.get("terminal_return")
        if type(terminal_return) is not int or terminal_return not in (-1, 0, 1):
            raise RuntimeError("capture term return is invalid")
        digest.update(group_index.to_bytes(8, "big"))
        digest.update(int(joint_bits[2:], 16).to_bytes(4, "big"))
        digest.update(int(value_bits[2:], 16).to_bytes(4, "big"))
        digest.update(bytes([terminal_return & 0xFF]))
    return digest.hexdigest()


def _term_values(
    terms: list[dict[str, Any]],
) -> tuple[list[float], list[float], list[tuple[float, float, int]]]:
    policy_terms: list[float] = []
    value_terms: list[float] = []
    raw_terms: list[tuple[float, float, int]] = []
    for ordinal, term in enumerate(terms):
        joint = _from_bits_hex(
            term["joint_log_probability_f32_bits"], f"joint term {ordinal}"
        )
        value = _from_bits_hex(term["value_f32_bits"], f"value term {ordinal}")
        terminal_return = int(term["terminal_return"])
        target = _f32(float(terminal_return))
        advantage = _f32_sub(target, value)
        policy_term = _f32_mul(_f32(-joint), advantage)
        value_error = _f32_sub(value, target)
        value_term = _f32_mul(value_error, value_error)
        if not math.isfinite(policy_term) or not math.isfinite(value_term):
            raise RuntimeError("capture reconstructed term is non-finite")
        policy_terms.append(policy_term)
        value_terms.append(value_term)
        raw_terms.append((joint, value, terminal_return))
    return policy_terms, value_terms, raw_terms


def _sequential_reduction(
    policy_terms: list[float], value_terms: list[float]
) -> tuple[float, float, float]:
    if not policy_terms or len(policy_terms) != len(value_terms):
        raise RuntimeError("sequential reduction term counts are invalid")
    policy_sum = 0.0
    value_sum = 0.0
    for policy_term, value_term in zip(policy_terms, value_terms, strict=True):
        policy_sum = _f32_add(policy_sum, policy_term)
        value_sum = _f32_add(value_sum, value_term)
    loss = _f32_div(
        _f32_add(policy_sum, _f32_mul(VALUE_COEFFICIENT, value_sum)),
        _f32(float(len(policy_terms))),
    )
    return policy_sum, value_sum, loss


def _validate_scalar_record(record: Any, expected: float, context: str) -> None:
    if not isinstance(record, dict) or set(record) != {"value", "f32_bits"}:
        raise RuntimeError(f"{context} scalar record is malformed")
    actual = _from_bits_hex(record["f32_bits"], context)
    if _bits_hex(expected) != record["f32_bits"] or record["value"] != actual:
        raise RuntimeError(f"{context} scalar bits/value drifted")


def _validate_capture(capture_path: Path) -> tuple[dict[str, Any], str, str]:
    if not capture_path.is_file():
        raise RuntimeError(f"K512 capture is missing: {capture_path}")
    capture_sha256 = _sha256(capture_path)
    capture = _load_strict(capture_path)
    _require_keys(
        capture,
        {
            "schema",
            "identity",
            "nonclaim",
            "source",
            "workload",
            "snapshot",
            "sizing_row",
            "episodes",
            "selected_outputs",
            "term_stream",
            "rust_production_reduction",
        },
        "capture",
    )
    if capture["schema"] != CAPTURE_SCHEMA or capture["identity"] != CAPTURE_IDENTITY:
        raise RuntimeError("K512 capture schema or identity drifted")

    source = capture["source"]
    if not isinstance(source, dict):
        raise RuntimeError("capture source record is missing")
    _require_keys(
        source,
        {
            "strict_source_tree",
            "preflight_validated",
            "postflight_equality_validated",
            "executable_sha256",
            "capture_harness_sha256",
            "capture_harness_path",
        },
        "capture source",
    )
    receipt = source.get("strict_source_tree")
    if not isinstance(receipt, dict):
        raise RuntimeError("capture strict source-tree receipt is missing")
    _require_keys(
        receipt,
        {
            "source_tree_recipe_identity",
            "source_tree_recipe_sha256",
            "git_commit",
            "source_tree_sha256",
            "worktree_clean",
            "git_status_sha256",
        },
        "strict source-tree receipt",
    )
    source_commit = _require_hex(receipt.get("git_commit"), 40, "source commit")
    source_tree_sha256 = _require_hex(
        receipt.get("source_tree_sha256"), 64, "strict source-tree sha256"
    )
    _blob, harness_sha256 = _capture_harness_blob(
        source_commit, source_tree_sha256
    )
    if (
        receipt.get("source_tree_recipe_identity") != STRICT_SOURCE_RECIPE_IDENTITY
        or receipt.get("source_tree_recipe_sha256") != STRICT_SOURCE_RECIPE_SHA256
        or receipt.get("git_status_sha256") != EMPTY_SHA256
        or receipt.get("worktree_clean") is not True
        or source.get("preflight_validated") is not True
        or source.get("postflight_equality_validated") is not True
        or source.get("capture_harness_sha256") != harness_sha256
        or source.get("capture_harness_path")
        != CAPTURE_HARNESS.relative_to(ROOT).as_posix()
    ):
        raise RuntimeError("capture source preflight/postflight record drifted")
    _require_hex(source.get("executable_sha256"), 64, "capture executable sha256")

    workload = capture["workload"]
    expected_workload = {
        "composition_identity": EXPECTED_COMPOSITION_IDENTITY,
        "trainer_contract_identity": EXPECTED_TRAINER_CONTRACT,
        "numerical_backend_identity": EXPECTED_NUMERICAL_BACKEND,
        "run_base_seed": EXPECTED_RUN_BASE_SEED,
        "batch_episodes": EXPECTED_K,
        "deck_ids": EXPECTED_DECK_IDS,
        "max_physical_decisions": 5_000,
        "max_policy_steps": 640_000,
        "scheduler_timeout_ms": 600_000,
        "measure_broker_service_time": False,
        "value_coefficient_f32_bits": VALUE_COEFFICIENT_BITS,
        "learning_rate_f32_bits": LEARNING_RATE_BITS,
    }
    for key, expected in expected_workload.items():
        if workload.get(key) != expected:
            raise RuntimeError(f"capture workload field drifted: {key}")
    if (
        "no physical group or term was cycled, replayed, expanded, or synthetically generated"
        not in workload.get("composition_nonclaim", "")
    ):
        raise RuntimeError("capture lacks the genuine non-cycled composition statement")
    for key in ("worker_count", "sessions_per_worker", "logical_actor_count", "broker_batch_target"):
        if type(workload.get(key)) is not int or workload[key] <= 0:
            raise RuntimeError(f"capture topology field is invalid: {key}")
    if (
        workload["logical_actor_count"]
        != workload["worker_count"] * workload["sessions_per_worker"]
        or workload["broker_batch_target"] > workload["logical_actor_count"]
    ):
        raise RuntimeError("capture logical actor topology is inconsistent")

    if capture["snapshot"] != _snapshot_expectations():
        raise RuntimeError("capture snapshot receipt drifted from frozen snapshot")

    sizing = capture["sizing_row"]
    if (
        sizing.get("update_ordinal") != 0
        or sizing.get("episode_count") != EXPECTED_K
        or sizing.get("adam_step_before") != 0
        or sizing.get("adam_step_after") != 1
        or sizing.get("learner_group_count", 0) <= 0
        or sizing.get("learner_policy_step_count", 0) <= 0
        or sizing.get("outer_update_elapsed_ns", 0) <= 0
        or sizing.get("executor_update_elapsed_ns", 0) <= 0
        or sizing.get("model_digest_before") == sizing.get("model_digest_after")
        or sizing.get("changed_non_gauge_parameter_count", 0) <= 0
    ):
        raise RuntimeError("capture sizing row invariant failed")

    episode_stream = capture["episodes"]
    episodes = episode_stream.get("records")
    if not isinstance(episodes, list) or len(episodes) != EXPECTED_K:
        raise RuntimeError("capture does not contain exactly 512 episode receipts")
    if (
        episode_stream.get("framing") != EPISODE_STREAM_FRAMING
        or episode_stream.get("sha256") != _episode_stream_digest(episodes)
        or episode_stream.get("independent_episode_count") != EXPECTED_K
        or episode_stream.get("distinct_environment_seed_count") != EXPECTED_K
    ):
        raise RuntimeError("capture episode stream provenance drifted")

    term_stream = capture["term_stream"]
    terms = term_stream.get("terms")
    if not isinstance(terms, list) or not terms:
        raise RuntimeError("capture term stream is empty")
    group_count = len(terms)
    for count_field in (
        "learner_physical_decision_group_count",
        "policy_term_count",
        "value_term_count",
    ):
        if term_stream.get(count_field) != group_count:
            raise RuntimeError(f"capture explicit term count drifted: {count_field}")
    if sizing.get("learner_group_count") != group_count:
        raise RuntimeError("capture sizing/group term counts disagree")
    if (
        term_stream.get("framing") != TERM_STREAM_FRAMING
        or term_stream.get("sha256") != _term_stream_digest(terms)
    ):
        raise RuntimeError("capture term stream digest drifted")

    term_cursor = 0
    learner_policy_step_sum = 0
    environment_seeds: set[int] = set()
    return_counts = [0, 0, 0]
    for episode in episodes:
        begin = episode.get("term_begin_inclusive")
        end = episode.get("term_end_exclusive")
        if (
            type(begin) is not int
            or type(end) is not int
            or begin != term_cursor
            or end <= begin
            or end > group_count
            or end - begin != episode["learner_group_count"]
        ):
            raise RuntimeError("capture episode term partition drifted")
        if any(
            term["terminal_return"] != episode["learner_return"]
            for term in terms[begin:end]
        ):
            raise RuntimeError("capture episode return differs from its term partition")
        term_cursor = end
        learner_policy_step_sum += episode["learner_policy_step_count"]
        environment_seeds.add(episode["environment_seed"])
    if (
        term_cursor != group_count
        or learner_policy_step_sum != sizing["learner_policy_step_count"]
        or len(environment_seeds) != EXPECTED_K
    ):
        raise RuntimeError("capture episode aggregate counts drifted")
    for term in terms:
        return_counts[term["terminal_return"] + 1] += 1
    if term_stream.get("terminal_return_counts") != return_counts:
        raise RuntimeError("capture terminal-return counts drifted")

    selected = capture["selected_outputs"]
    if (
        selected.get("framing") != SELECTED_STREAM_FRAMING
        or selected.get("count") != sizing["learner_policy_step_count"]
    ):
        raise RuntimeError("capture selected-output count/framing drifted")
    _require_hex(selected.get("sha256"), 64, "selected output stream sha256")

    policy_terms, value_terms, _raw_terms = _term_values(terms)
    if (
        len(policy_terms) != term_stream["policy_term_count"]
        or len(value_terms) != term_stream["value_term_count"]
        or sum(_f32_bits(value) & 0x7FFFFFFF != 0 for value in policy_terms)
        != term_stream.get("policy_nonzero_count")
        or sum(_f32_bits(value) & 0x7FFFFFFF != 0 for value in value_terms)
        != term_stream.get("value_nonzero_count")
    ):
        raise RuntimeError("capture reconstructed term counts drifted")
    sequential = _sequential_reduction(policy_terms, value_terms)
    production = capture["rust_production_reduction"]
    if production.get("reconstruction_matches_production_bits") is not True:
        raise RuntimeError("capture did not establish production scalar reconstruction")
    for name, expected in zip(ALL_CHANNELS, sequential, strict=True):
        _validate_scalar_record(production.get(name), expected, f"production {name}")

    return capture, capture_sha256, harness_sha256


def _tensor_bits(value: torch.Tensor) -> str:
    if value.numel() != 1 or value.dtype != torch.float32 or value.device.type != "cpu":
        raise RuntimeError("Torch authority scalar shape/dtype/device drifted")
    bits = int(value.detach().contiguous().view(torch.int32).item()) & 0xFFFFFFFF
    return f"0x{bits:08x}"


def _authority_reduction(
    capture: dict[str, Any],
) -> tuple[tuple[float, float, float], int, int, int]:
    terms = capture["term_stream"]["terms"]
    policy_portable, value_portable, raw_terms = _term_values(terms)
    torch_terms: list[tuple[torch.Tensor, torch.Tensor, int]] = []
    with torch.no_grad():
        for joint, value, terminal_return in raw_terms:
            torch_terms.append(
                (
                    torch.tensor(joint, dtype=torch.float32),
                    torch.tensor(value, dtype=torch.float32),
                    terminal_return,
                )
            )
        policy_sum, value_sum, loss = _compute_loss_tensors(torch_terms, VALUE_COEFFICIENT)
        policy_terms = []
        value_terms = []
        for joint, value, terminal_return in torch_terms:
            target = torch.tensor(float(terminal_return), dtype=torch.float32)
            policy_terms.append(-joint * (target - value.detach()))
            value_terms.append((value - target) ** 2)
        direct_policy_sum = torch.stack(policy_terms).sum()
        direct_value_sum = torch.stack(value_terms).sum()
        direct_loss = (
            direct_policy_sum + float(VALUE_COEFFICIENT) * direct_value_sum
        ) / len(terms)
    for ordinal, (torch_term, portable) in enumerate(
        zip(policy_terms, policy_portable, strict=True)
    ):
        if _tensor_bits(torch_term) != _bits_hex(portable):
            raise RuntimeError(f"policy term bits differ before reduction at {ordinal}")
    for ordinal, (torch_term, portable) in enumerate(
        zip(value_terms, value_portable, strict=True)
    ):
        if _tensor_bits(torch_term) != _bits_hex(portable):
            raise RuntimeError(f"value term bits differ before reduction at {ordinal}")
    if (
        _tensor_bits(policy_sum) != _tensor_bits(direct_policy_sum)
        or _tensor_bits(value_sum) != _tensor_bits(direct_value_sum)
        or _tensor_bits(loss) != _tensor_bits(direct_loss)
    ):
        raise RuntimeError("pinned trainer no longer uses direct stack/sum reduction")
    values = (float(policy_sum.item()), float(value_sum.item()), float(loss.item()))
    if not all(math.isfinite(value) for value in values):
        raise RuntimeError("Torch authority reduction is non-finite")
    return values, len(terms), len(policy_terms), len(value_terms)


def _comparison_record(name: str, torch_value: float, rust_value: float) -> dict[str, Any]:
    delta = abs(float(rust_value) - float(torch_value))
    allowed = LOSS_ABSOLUTE_TOLERANCE + LOSS_RELATIVE_TOLERANCE * abs(
        float(torch_value)
    )
    tolerance_holds = delta <= allowed
    margin = None if delta == 0.0 else allowed / delta
    floor_applies = name in SUM_CHANNELS
    margin_floor_holds = not floor_applies or margin is None or margin >= SUM_MARGIN_FLOOR
    return {
        "absolute_delta_f64": delta,
        "allowed_delta_f64": allowed,
        "tolerance_holds": tolerance_holds,
        "margin_ratio_allowed_over_delta_f64": margin,
        "margin_floor_applies": floor_applies,
        "required_margin_floor_f64": SUM_MARGIN_FLOOR if floor_applies else None,
        "margin_floor_holds": margin_floor_holds,
        "gate_holds": tolerance_holds and margin_floor_holds,
    }


def _gate_record(comparisons: dict[str, dict[str, Any]]) -> dict[str, Any]:
    triggered: list[str] = []
    for name in ALL_CHANNELS:
        comparison = comparisons[name]
        if not comparison["tolerance_holds"]:
            triggered.append(f"{name}:frozen_tolerance_breach")
        if comparison["margin_floor_applies"] and not comparison["margin_floor_holds"]:
            triggered.append(f"{name}:margin_below_5x")
    repair_required = bool(triggered)
    return {
        "status": "repair_required" if repair_required else "pass",
        "diagnostic_code": REPAIR_DIAGNOSTIC if repair_required else PASS_DIAGNOSTIC,
        "triggered_conditions": triggered,
        "all_frozen_tolerances_hold": all(
            comparisons[name]["tolerance_holds"] for name in ALL_CHANNELS
        ),
        "all_sum_margin_floors_hold": all(
            comparisons[name]["margin_floor_holds"] for name in SUM_CHANNELS
        ),
        "silent_tolerance_loosening_permitted": False,
        "predeclared_repair_paths": [
            "versioned deterministic blocked/pairwise Rust reduction with a new train-step identity and fresh cross-language goldens",
            "separately reviewed scale-aware Higham-style bound for sum channels only while leaving per-group loss tolerance unchanged",
        ],
    }


def _payload(capture_path: Path) -> dict[str, Any]:
    capture, capture_sha256, harness_sha256 = _validate_capture(capture_path)
    source_receipt = capture["source"]["strict_source_tree"]
    torch_stack, group_count, policy_term_count, value_term_count = _authority_reduction(
        capture
    )
    capture_counts = capture["term_stream"]
    if (
        group_count != capture_counts["learner_physical_decision_group_count"]
        or policy_term_count != capture_counts["policy_term_count"]
        or value_term_count != capture_counts["value_term_count"]
        or group_count != capture["sizing_row"]["learner_group_count"]
    ):
        raise RuntimeError("authority term counts disagree before bit comparison")
    production = capture["rust_production_reduction"]
    rust_values = tuple(
        _from_bits_hex(production[name]["f32_bits"], f"production {name}")
        for name in ALL_CHANNELS
    )
    comparisons = {
        name: _comparison_record(name, expected, actual)
        for name, expected, actual in zip(
            ALL_CHANNELS, torch_stack, rust_values, strict=True
        )
    }
    gate = _gate_record(comparisons)
    authority_hashes = train_fixture._validate_authorities()
    return {
        "schema": SCHEMA,
        "identity": IDENTITY,
        "authority": {
            "torch_computation_generator_path": GENERATOR.relative_to(ROOT).as_posix(),
            "torch_computation_generator_sha256": _sha256(GENERATOR),
            "trainer_source_path": TRAINER_SOURCE.relative_to(ROOT).as_posix(),
            "trainer_source_sha256": authority_hashes["trainer_sha256"],
            "capture_harness_path": CAPTURE_HARNESS.relative_to(ROOT).as_posix(),
            "capture_harness_sha256": harness_sha256,
            "capture_artifact_path": CAPTURE_PATH.relative_to(ROOT).as_posix(),
            "capture_artifact_sha256": capture_sha256,
            "source_tree_recipe_identity": source_receipt[
                "source_tree_recipe_identity"
            ],
            "source_tree_recipe_sha256": source_receipt[
                "source_tree_recipe_sha256"
            ],
            "capture_source_git_commit": source_receipt["git_commit"],
            "capture_source_tree_sha256": source_receipt["source_tree_sha256"],
            "intermediate_rung_path": INTERMEDIATE_RUNG.relative_to(ROOT).as_posix(),
            "intermediate_rung_sha256": _sha256(INTERMEDIATE_RUNG),
            "intermediate_generator_path": INTERMEDIATE_GENERATOR.relative_to(ROOT).as_posix(),
            "intermediate_generator_sha256": _sha256(INTERMEDIATE_GENERATOR),
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
            "strict_source_tree": source_receipt,
            "source_preflight_validated": capture["source"]["preflight_validated"],
            "source_postflight_equality_validated": capture["source"][
                "postflight_equality_validated"
            ],
            "composition_identity": capture["workload"]["composition_identity"],
            "composition_nonclaim": capture["workload"]["composition_nonclaim"],
            "trainer_contract_identity": capture["workload"]["trainer_contract_identity"],
            "rust_numerical_backend_identity": capture["workload"][
                "numerical_backend_identity"
            ],
            "torch_numerical_backend_identity": "torch-2.13.0+cpu-windows-amd64-f32-deterministic-threads1-stack-sum",
            "run_base_seed": capture["workload"]["run_base_seed"],
            "batch_episodes": capture["workload"]["batch_episodes"],
            "deck_ids": capture["workload"]["deck_ids"],
            "worker_count": capture["workload"]["worker_count"],
            "sessions_per_worker": capture["workload"]["sessions_per_worker"],
            "logical_actor_count": capture["workload"]["logical_actor_count"],
            "broker_batch_target": capture["workload"]["broker_batch_target"],
            "snapshot_sha256": capture["snapshot"]["snapshot_sha256"],
            "independent_episode_count": capture["episodes"][
                "independent_episode_count"
            ],
            "distinct_environment_seed_count": capture["episodes"][
                "distinct_environment_seed_count"
            ],
            "episode_stream_sha256": capture["episodes"]["sha256"],
            "term_stream_sha256": capture["term_stream"]["sha256"],
        },
        "sizing_row": {
            **capture["sizing_row"],
            "learner_physical_decision_group_count": group_count,
            "policy_term_count": policy_term_count,
            "value_term_count": value_term_count,
            "rust_production_sequential": {
                name: production[name] for name in ALL_CHANNELS
            },
            "torch_stack_sum": {
                name: _scalar(value)
                for name, value in zip(ALL_CHANNELS, torch_stack, strict=True)
            },
            "same_term_rust_sequential_vs_torch_stack": comparisons,
        },
        "reduction_contract": {
            "torch_operation": "policy_sum=torch.stack(policy_terms).sum(); value_sum=torch.stack(value_terms).sum(); loss=(policy_sum+0.5*value_sum)/group_count",
            "rust_operation": capture["rust_production_reduction"]["operation"],
            "same_term_bits_before_reduction": True,
            "group_count": group_count,
            "policy_term_count": policy_term_count,
            "value_term_count": value_term_count,
            "frozen_tolerance": {
                "absolute": LOSS_ABSOLUTE_TOLERANCE,
                "relative": LOSS_RELATIVE_TOLERANCE,
                "comparison_rule": "abs(rust-torch) <= absolute + relative*abs(torch)",
            },
            "sum_margin_floor": {
                "channels": list(SUM_CHANNELS),
                "minimum_allowed_over_delta_ratio": SUM_MARGIN_FLOOR,
                "loss_channel_floor_nonclaim": "loss must hold the frozen tolerance; the predeclared 5x floor applies to policy_sum and value_sum",
            },
        },
        "gate": gate,
        "nonclaim": "one genuine Rally K=512 scalar-loss numerical gate; not learning quality, throughput, XMage comparison, all-deck coverage, or K>512 evidence",
    }


def _encoded_payload(payload: dict[str, Any]) -> bytes:
    return (json.dumps(payload, sort_keys=True, indent=2) + "\n").encode("utf-8")


def _comparison_from_records(
    name: str, torch_record: dict[str, Any], rust_record: dict[str, Any]
) -> dict[str, Any]:
    expected = _from_bits_hex(torch_record["f32_bits"], f"recorded Torch {name}")
    actual = _from_bits_hex(rust_record["f32_bits"], f"recorded Rust {name}")
    _validate_scalar_record(torch_record, expected, f"recorded Torch {name}")
    _validate_scalar_record(rust_record, actual, f"recorded Rust {name}")
    return _comparison_record(name, expected, actual)


def _portable_check() -> None:
    capture, capture_sha256, harness_sha256 = _validate_capture(CAPTURE_PATH)
    source_receipt = capture["source"]["strict_source_tree"]
    checked = _load_strict(OUTPUT)
    if checked.get("schema") != SCHEMA or checked.get("identity") != IDENTITY:
        raise RuntimeError("true-K512 gate schema or identity drifted")
    authority = checked.get("authority", {})
    expected_authority_pins = {
        "torch_computation_generator_path": GENERATOR.relative_to(ROOT).as_posix(),
        "torch_computation_generator_sha256": _sha256(GENERATOR),
        "trainer_source_path": TRAINER_SOURCE.relative_to(ROOT).as_posix(),
        "trainer_source_sha256": _sha256(TRAINER_SOURCE),
        "capture_harness_path": CAPTURE_HARNESS.relative_to(ROOT).as_posix(),
        "capture_harness_sha256": harness_sha256,
        "capture_artifact_path": CAPTURE_PATH.relative_to(ROOT).as_posix(),
        "capture_artifact_sha256": capture_sha256,
        "source_tree_recipe_identity": STRICT_SOURCE_RECIPE_IDENTITY,
        "source_tree_recipe_sha256": STRICT_SOURCE_RECIPE_SHA256,
        "capture_source_git_commit": source_receipt["git_commit"],
        "capture_source_tree_sha256": source_receipt["source_tree_sha256"],
        "intermediate_rung_path": INTERMEDIATE_RUNG.relative_to(ROOT).as_posix(),
        "intermediate_rung_sha256": _sha256(INTERMEDIATE_RUNG),
        "intermediate_generator_path": INTERMEDIATE_GENERATOR.relative_to(ROOT).as_posix(),
        "intermediate_generator_sha256": _sha256(INTERMEDIATE_GENERATOR),
    }
    for key, expected in expected_authority_pins.items():
        if authority.get(key) != expected:
            raise RuntimeError(f"true-K512 authority pin drifted: {key}")
    provenance = checked.get("provenance", {})
    if (
        provenance.get("strict_source_tree") != source_receipt
        or provenance.get("source_preflight_validated") is not True
        or provenance.get("source_postflight_equality_validated") is not True
    ):
        raise RuntimeError("true-K512 strict source provenance drifted")
    sizing = checked.get("sizing_row", {})
    capture_group_count = capture["term_stream"][
        "learner_physical_decision_group_count"
    ]
    for field in (
        "learner_physical_decision_group_count",
        "policy_term_count",
        "value_term_count",
    ):
        if sizing.get(field) != capture_group_count:
            raise RuntimeError(f"true-K512 gate term count drifted: {field}")
    production = sizing.get("rust_production_sequential", {})
    torch_stack = sizing.get("torch_stack_sum", {})
    comparisons = sizing.get("same_term_rust_sequential_vs_torch_stack", {})
    expected_comparisons = {
        name: _comparison_from_records(name, torch_stack[name], production[name])
        for name in ALL_CHANNELS
    }
    if comparisons != expected_comparisons:
        raise RuntimeError("true-K512 recorded deltas/margins drifted")
    reduction = checked.get("reduction_contract", {})
    if (
        reduction.get("group_count") != capture_group_count
        or reduction.get("policy_term_count") != capture_group_count
        or reduction.get("value_term_count") != capture_group_count
        or reduction.get("same_term_bits_before_reduction") is not True
        or reduction.get("frozen_tolerance")
        != {
            "absolute": LOSS_ABSOLUTE_TOLERANCE,
            "relative": LOSS_RELATIVE_TOLERANCE,
            "comparison_rule": "abs(rust-torch) <= absolute + relative*abs(torch)",
        }
        or reduction.get("sum_margin_floor", {}).get(
            "minimum_allowed_over_delta_ratio"
        )
        != SUM_MARGIN_FLOOR
    ):
        raise RuntimeError("true-K512 frozen reduction contract drifted")
    if checked.get("gate") != _gate_record(expected_comparisons):
        raise RuntimeError("true-K512 terminal gate diagnostic drifted")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--capture", type=Path, default=CAPTURE_PATH)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--check",
        action="store_true",
        help="portable source, capture, count, arithmetic, delta, and gate check",
    )
    mode.add_argument(
        "--authority-check",
        action="store_true",
        help="on the exact Torch authority tuple, regenerate and require byte identity",
    )
    args = parser.parse_args()
    if args.check:
        if args.capture != CAPTURE_PATH or args.output != OUTPUT:
            raise RuntimeError("--check only accepts the checked-in capture/output paths")
        _portable_check()
        print(f"PASS portable {OUTPUT.relative_to(ROOT)}")
        return 0

    train_fixture._assert_exact_authority_environment()
    payload = _payload(args.capture)
    encoded = _encoded_payload(payload)
    if args.authority_check:
        if args.capture != CAPTURE_PATH or args.output != OUTPUT:
            raise RuntimeError(
                "--authority-check only accepts the checked-in capture/output paths"
            )
        actual = OUTPUT.read_bytes() if OUTPUT.exists() else b""
        if actual != encoded:
            raise SystemExit(f"stale true-K512 loss gate artifact: {OUTPUT}")
        print(
            f"PASS authority {OUTPUT.relative_to(ROOT)} status={payload['gate']['status']}"
        )
        return 0

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(encoded)
    print(
        f"wrote {args.output} status={payload['gate']['status']} "
        f"diagnostic={payload['gate']['diagnostic_code']}"
    )
    return 3 if payload["gate"]["status"] == "repair_required" else 0


if __name__ == "__main__":
    raise SystemExit(main())
