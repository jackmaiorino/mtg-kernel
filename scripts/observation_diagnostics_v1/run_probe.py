#!/usr/bin/env python3
"""Run the six frozen CPU-only checkpoint-reliance diagnostics fail closed."""

from __future__ import annotations

import argparse
import dataclasses
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
from typing import Any, Mapping

try:
    from scripts.observation_diagnostics_v1 import contract
except ModuleNotFoundError:  # Direct execution from this directory.
    import contract  # type: ignore[no-redef]


TIMEOUT_SECONDS = 120.0
MARKER = b"OBS_RELIANCE_JSON="
TIMING_PREFIX = b"OBS_RELIANCE_TIMING "
HARNESS_MARKER_PREFIX = (
    b"test " + contract.PROBE_TEST.encode("ascii") + b" ... " + MARKER
)
TIMING_RE = re.compile(
    rb"^OBS_RELIANCE_TIMING "
    rb"authority_ms=(?P<authority>[0-9]+) "
    rb"corpus_ms=(?P<corpus>[0-9]+) "
    rb"scoring_ms=(?P<scoring>[0-9]+) "
    rb"total_ms=(?P<total>[0-9]+)$"
)

PAYLOAD_KEYS = {
    "schema",
    "label",
    "test_identity",
    "run_base_seed",
    "model_architecture_version",
    "model_config_fingerprint",
    "feature_contract_digest",
    "feature_encoding_digest",
    "checkpoints",
    "feature_partition",
    "corpus",
    "permutation",
    "ingress_groups",
    "hash_to_direct_ingress_ratios",
    "functional_models",
    "candidate_minus_g0_functional_effects",
    "aggregate_output_stream_sha256",
    "repeat_aggregate_output_stream_bit_exact",
    "output_digest_identity",
    "nonclaims",
}
CHECKPOINT_KEYS = {
    "role",
    "generation_index",
    "run_sha256",
    "checkpoint_manifest_sha256",
    "checkpoint_payload_sha256",
    "train_state_sha256",
    "model_parameter_sha256",
    "adam_step",
    "identity_bundle_sha256",
    "segment_ordinal",
    "segment_manifest_sha256",
    "parent_boundary_head_sha256",
    "boundary_head_sha256",
    "boundary_head_record_sha256",
    "checkpoint_sidecar_sha256",
    "logical_state_sha256",
    "last_update_evidence_sha256",
}
CORPUS_KEYS = {
    "identity",
    "digest_identity",
    "sha256",
    "deck_ids",
    "decision_count",
    "episode_count",
    "decisions_per_episode_cap",
    "multi_action_decision_count",
    "total_action_count",
    "base_episode_id",
    "base_environment_seed",
    "action_selection",
}
EXPECTED_FEATURE_PARTITION = {
    "action_direct_range": [0, 99],
    "action_encoder_first_weight_shape": [64, 259],
    "action_feature_dim": 195,
    "action_legal_hash_range": [99, 195],
    "hash_feature_dim_each": 96,
    "state_direct_range": [0, 123],
    "state_encoder_first_weight_shape": [64, 1499],
    "state_feature_dim": 219,
    "state_observation_hash_range": [123, 219],
    "structured_explicit_inputs_are_a_separate_bucket": True,
}
INTERVENTIONS = {
    "state_hash_permutation",
    "action_hash_permutation",
    "both_hash_permutation",
    "state_direct_permutation",
    "action_direct_permutation",
    "both_direct_permutation",
    "hash_zero_ablation",
    "direct_zero_ablation",
}
INGRESS_GROUPS = {"state_direct", "state_hash", "action_direct", "action_hash"}
EXPECTED_PROBE_TEST_IDENTITY = contract.PROBE_TEST
EXPECTED_MODEL_CONFIG_FINGERPRINT = (
    "f3836afa17acc74b4856fe18222345116f27c12fa5ad18c34b4dec3f04855251"
)
EXPECTED_FEATURE_CONTRACT_DIGEST = (
    "bcc808186e40a1ad6aec679d8a386631cb1226379366a632603f0beb95b47396"
)
EXPECTED_FEATURE_ENCODING_DIGEST = (
    "918e57a0796807e84310026de48d30b500813ef37d939462ea85b7255a39111c"
)

BUILD_RECEIPT_KEYS = {
    "schema",
    "label",
    "started_utc",
    "completed_utc",
    "git_head",
    "git_status_clean_before_and_after",
    "manifest",
    "cargo_lock",
    "build_source",
    "contract_source",
    "cargo",
    "executable",
    "integrity_tests",
    "required_tests",
    "static_audit",
    "test_list",
    "payload_sha256",
}


@dataclasses.dataclass(frozen=True)
class ParsedProbe:
    envelope: dict[str, Any]
    envelope_raw: bytes
    payload: dict[str, Any]
    payload_raw: bytes
    timing: dict[str, int]
    timing_line: str


@dataclasses.dataclass(frozen=True)
class ProcessCapture:
    exit_code: int | None
    stderr: bytes
    stdout: bytes
    timed_out: bool
    wall_time_ms: int


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _require_string(value: Any, where: str, *, expected: str | None = None) -> str:
    if type(value) is not str or not value:
        contract.fail(f"{where} must be a nonempty string")
    if expected is not None and value != expected:
        contract.fail(f"{where} mismatch: expected={expected!r} actual={value!r}")
    return value


def _require_bool(value: Any, where: str, expected: bool) -> None:
    if type(value) is not bool or value is not expected:
        contract.fail(f"{where} must be {expected}")


def _require_exact_array(
    value: Any,
    expected: list[Any],
    where: str,
) -> None:
    if value != expected:
        contract.fail(f"{where} mismatch: expected={expected!r} actual={value!r}")


def parse_probe_output(stdout: bytes, stderr: bytes) -> ParsedProbe:
    if len(stdout) > contract.MAX_PROCESS_STREAM_BYTES:
        contract.fail("probe stdout exceeds the frozen size cap")
    if len(stderr) > contract.MAX_PROCESS_STREAM_BYTES:
        contract.fail("probe stderr exceeds the frozen size cap")
    if stderr.count(MARKER) or stderr.count(TIMING_PREFIX):
        contract.fail("probe marker/timing output is forbidden on stderr")
    if stdout.count(MARKER) != 1:
        contract.fail("probe stdout must contain exactly one OBS_RELIANCE_JSON marker")
    if stdout.count(TIMING_PREFIX) != 1:
        contract.fail("probe stdout must contain exactly one OBS_RELIANCE_TIMING line")

    lines = stdout.splitlines()
    marker_indices = [
        index
        for index, line in enumerate(lines)
        if line.startswith(HARNESS_MARKER_PREFIX)
    ]
    timing_indices = [
        index for index, line in enumerate(lines) if line.startswith(TIMING_PREFIX)
    ]
    if len(marker_indices) != 1 or len(timing_indices) != 1:
        contract.fail(
            "probe marker must follow the exact Windows libtest prefix and "
            "timing must occupy one stdout line"
        )
    marker_index = marker_indices[0]
    timing_index = timing_indices[0]
    if marker_index == 0 or lines[marker_index - 1] != b"running 1 test":
        contract.fail("probe marker must follow the single-test harness header")
    if timing_index != marker_index + 1:
        contract.fail("OBS_RELIANCE_TIMING must immediately follow OBS_RELIANCE_JSON")
    if timing_index + 1 >= len(lines) or lines[timing_index + 1] != b"ok":
        contract.fail("probe timing must be followed by the libtest ok status")

    envelope_raw = lines[marker_index][len(HARNESS_MARKER_PREFIX) :]
    envelope = contract.parse_json_bytes(envelope_raw, "OBS_RELIANCE_JSON envelope")
    contract.exact_keys(
        envelope,
        {"schema", "payload_sha256", "payload"},
        "OBS_RELIANCE_JSON envelope",
    )
    _require_string(
        envelope["schema"],
        "envelope.schema",
        expected=contract.PROBE_ENVELOPE_SCHEMA,
    )
    payload_sha256 = contract.require_sha256(
        envelope["payload_sha256"], "envelope.payload_sha256"
    )

    prefix = (
        b'{"schema":"'
        + contract.PROBE_ENVELOPE_SCHEMA.encode("ascii")
        + b'","payload_sha256":"'
        + payload_sha256.encode("ascii")
        + b'","payload":'
    )
    if not envelope_raw.startswith(prefix) or not envelope_raw.endswith(b"}"):
        contract.fail(
            "probe envelope is not the exact compact Rust field-order serialization"
        )
    payload_raw = envelope_raw[len(prefix) : -1]
    if not payload_raw.startswith(b"{") or not payload_raw.endswith(b"}"):
        contract.fail("probe nested payload is not an object serialization")
    actual_payload_sha256 = contract.sha256_bytes(payload_raw)
    if actual_payload_sha256 != payload_sha256:
        contract.fail(
            "raw nested payload SHA-256 mismatch: "
            f"declared={payload_sha256} actual={actual_payload_sha256}"
        )
    payload = contract.parse_json_bytes(payload_raw, "raw nested probe payload")
    if envelope["payload"] != payload:
        contract.fail("envelope payload parse differs from raw nested payload parse")

    timing_match = TIMING_RE.fullmatch(lines[timing_index])
    if timing_match is None:
        contract.fail("OBS_RELIANCE_TIMING line has the wrong shape")
    timing = {
        name: int(timing_match.group(name))
        for name in ("authority", "corpus", "scoring", "total")
    }
    if timing["total"] < max(
        timing["authority"], timing["corpus"], timing["scoring"]
    ):
        contract.fail("reported total_ms is smaller than a component")
    return ParsedProbe(
        envelope=envelope,
        envelope_raw=envelope_raw,
        payload=payload,
        payload_raw=payload_raw,
        timing=timing,
        timing_line=lines[timing_index].decode("ascii"),
    )


def _verify_checkpoint(
    value: Any,
    *,
    role: str,
    generation: int,
    where: str,
) -> dict[str, Any]:
    checkpoint = dict(contract.exact_keys(value, CHECKPOINT_KEYS, where))
    _require_string(checkpoint["role"], f"{where}.role", expected=role)
    actual_generation = contract.require_natural(
        checkpoint["generation_index"], f"{where}.generation_index"
    )
    if actual_generation != generation:
        contract.fail(
            f"{where}.generation_index mismatch: "
            f"expected={generation} actual={actual_generation}"
        )
    if checkpoint["adam_step"] != generation:
        contract.fail(f"{where}.adam_step must equal generation_index")
    for field in (
        "run_sha256",
        "checkpoint_manifest_sha256",
        "checkpoint_payload_sha256",
        "train_state_sha256",
        "model_parameter_sha256",
        "identity_bundle_sha256",
        "segment_manifest_sha256",
        "boundary_head_sha256",
        "boundary_head_record_sha256",
        "checkpoint_sidecar_sha256",
        "logical_state_sha256",
    ):
        contract.require_sha256(checkpoint[field], f"{where}.{field}")
    segment_ordinal = contract.require_natural(
        checkpoint["segment_ordinal"], f"{where}.segment_ordinal"
    )
    if generation == 0:
        if segment_ordinal != 0:
            contract.fail(f"{where}.segment_ordinal must be zero for genesis")
        for field in (
            "parent_boundary_head_sha256",
            "last_update_evidence_sha256",
        ):
            if checkpoint[field] is not None:
                contract.fail(f"{where}.{field} must be null for genesis")
    else:
        for field in (
            "parent_boundary_head_sha256",
            "last_update_evidence_sha256",
        ):
            contract.require_sha256(checkpoint[field], f"{where}.{field}")
    return checkpoint


def _verify_functional_models(value: Any, pair: contract.PairSpec) -> None:
    models = contract.require_array(value, "payload.functional_models")
    if len(models) != 2:
        contract.fail("payload.functional_models must contain g0 and candidate")
    expected = (("g0", 0), ("candidate", pair.candidate_generation))
    for index, ((role, generation), model_value) in enumerate(zip(expected, models)):
        where = f"payload.functional_models[{index}]"
        model = contract.exact_keys(
            model_value,
            {
                "role",
                "generation_index",
                "baseline_output_sha256",
                "repeat_baseline_bit_exact",
                "effects",
                "hash_minus_direct_contrasts",
            },
            where,
        )
        _require_string(model["role"], f"{where}.role", expected=role)
        if model["generation_index"] != generation:
            contract.fail(f"{where}.generation_index mismatch")
        contract.require_sha256(
            model["baseline_output_sha256"],
            f"{where}.baseline_output_sha256",
        )
        _require_bool(
            model["repeat_baseline_bit_exact"],
            f"{where}.repeat_baseline_bit_exact",
            True,
        )
        effects = contract.require_array(model["effects"], f"{where}.effects")
        names: list[str] = []
        for effect_index, effect_value in enumerate(effects):
            effect_where = f"{where}.effects[{effect_index}]"
            if not isinstance(effect_value, Mapping):
                contract.fail(f"{effect_where} must be an object")
            names.append(
                _require_string(
                    effect_value.get("intervention"),
                    f"{effect_where}.intervention",
                )
            )
            contract.require_sha256(
                effect_value.get("intervention_output_sha256"),
                f"{effect_where}.intervention_output_sha256",
            )
        if len(names) != len(set(names)) or set(names) != INTERVENTIONS:
            contract.fail(f"{where}.effects intervention set mismatch")


def verify_probe_payload(
    payload: dict[str, Any],
    pair: contract.PairSpec,
) -> dict[str, Any]:
    contract.exact_keys(payload, PAYLOAD_KEYS, "payload")
    _require_string(
        payload["schema"],
        "payload.schema",
        expected=contract.PROBE_PAYLOAD_SCHEMA,
    )
    _require_string(payload["label"], "payload.label", expected=contract.LABEL)
    _require_string(
        payload["test_identity"],
        "payload.test_identity",
        expected=EXPECTED_PROBE_TEST_IDENTITY,
    )
    run_base_seed = contract.require_natural(
        payload["run_base_seed"], "payload.run_base_seed"
    )
    if run_base_seed != pair.seed:
        contract.fail(
            "payload.run_base_seed mismatch: "
            f"expected={pair.seed} actual={run_base_seed}"
        )
    _require_string(
        payload["model_architecture_version"],
        "payload.model_architecture_version",
        expected="kernel-policy-value-net-8",
    )
    for field, expected in (
        ("model_config_fingerprint", EXPECTED_MODEL_CONFIG_FINGERPRINT),
        ("feature_contract_digest", EXPECTED_FEATURE_CONTRACT_DIGEST),
        ("feature_encoding_digest", EXPECTED_FEATURE_ENCODING_DIGEST),
    ):
        actual = contract.require_sha256(payload[field], f"payload.{field}")
        if actual != expected:
            contract.fail(
                f"payload.{field} mismatch: expected={expected} actual={actual}"
            )

    checkpoints = contract.require_array(payload["checkpoints"], "payload.checkpoints")
    if len(checkpoints) != 2:
        contract.fail("payload.checkpoints must contain exactly g0 and candidate")
    g0 = _verify_checkpoint(
        checkpoints[0],
        role="g0",
        generation=0,
        where="payload.checkpoints[0]",
    )
    candidate = _verify_checkpoint(
        checkpoints[1],
        role="candidate",
        generation=pair.candidate_generation,
        where="payload.checkpoints[1]",
    )
    if g0["run_sha256"] != candidate["run_sha256"]:
        contract.fail("g0 and candidate checkpoint identities must bind one run")

    feature_partition = payload["feature_partition"]
    if feature_partition != EXPECTED_FEATURE_PARTITION:
        contract.fail(
            "payload.feature_partition mismatch: "
            f"expected={EXPECTED_FEATURE_PARTITION!r} "
            f"actual={feature_partition!r}"
        )

    corpus = dict(
        contract.exact_keys(payload["corpus"], CORPUS_KEYS, "payload.corpus")
    )
    _require_string(
        corpus["identity"],
        "payload.corpus.identity",
        expected=(
            "rally-mirror-splitmix64-modulo-fixed-256-"
            "post-tensorization-v1"
        ),
    )
    _require_string(
        corpus["digest_identity"],
        "payload.corpus.digest_identity",
        expected="sha256-framed-thirteen-native-flat-tensors-v1",
    )
    contract.require_sha256(corpus["sha256"], "payload.corpus.sha256")
    if (
        corpus["sha256"]
        != "72103ea367a662f76675a044ad4efcf4c52bf86d32630df88e5247cf79f5e5e0"
    ):
        contract.fail("payload.corpus.sha256 differs from the pinned Rust corpus")
    _require_exact_array(corpus["deck_ids"], ["Rally", "Rally"], "payload.corpus.deck_ids")
    if corpus["decision_count"] != 256:
        contract.fail("payload.corpus.decision_count must be 256")
    multi_action = contract.require_natural(
        corpus["multi_action_decision_count"],
        "payload.corpus.multi_action_decision_count",
    )
    if multi_action != 256:
        contract.fail("payload.corpus.multi_action_decision_count must be 256")
    if corpus["episode_count"] != 4:
        contract.fail("payload.corpus.episode_count must be 4")
    if corpus["total_action_count"] != 1_115:
        contract.fail("payload.corpus.total_action_count must be 1115")
    if corpus["decisions_per_episode_cap"] != 64:
        contract.fail("payload.corpus.decisions_per_episode_cap must be 64")
    if corpus["base_episode_id"] != 880_000:
        contract.fail("payload.corpus.base_episode_id mismatch")
    if corpus["base_environment_seed"] != 0x6D74_672D_6861_7368:
        contract.fail("payload.corpus.base_environment_seed mismatch")
    _require_string(
        corpus["action_selection"],
        "payload.corpus.action_selection",
        expected="splitmix64-next-modulo-legal-action-count-v1",
    )

    ingress_groups = contract.require_array(
        payload["ingress_groups"], "payload.ingress_groups"
    )
    ingress_names = [
        _require_string(
            value.get("name") if isinstance(value, Mapping) else None,
            f"payload.ingress_groups[{index}].name",
        )
        for index, value in enumerate(ingress_groups)
    ]
    if len(ingress_names) != len(set(ingress_names)) or set(ingress_names) != INGRESS_GROUPS:
        contract.fail("payload.ingress_groups names mismatch")

    ratios = contract.require_array(
        payload["hash_to_direct_ingress_ratios"],
        "payload.hash_to_direct_ingress_ratios",
    )
    ratio_pathways = [
        _require_string(
            value.get("pathway") if isinstance(value, Mapping) else None,
            f"payload.hash_to_direct_ingress_ratios[{index}].pathway",
        )
        for index, value in enumerate(ratios)
    ]
    if len(ratio_pathways) != 2 or set(ratio_pathways) != {"state", "action"}:
        contract.fail("payload hash/direct ingress ratio pathways mismatch")

    _verify_functional_models(payload["functional_models"], pair)
    contrasts = contract.require_array(
        payload["candidate_minus_g0_functional_effects"],
        "payload.candidate_minus_g0_functional_effects",
    )
    contrast_names = [
        _require_string(
            value.get("intervention") if isinstance(value, Mapping) else None,
            f"payload.candidate_minus_g0_functional_effects[{index}].intervention",
        )
        for index, value in enumerate(contrasts)
    ]
    if len(contrast_names) != len(set(contrast_names)) or set(contrast_names) != INTERVENTIONS:
        contract.fail("payload candidate-minus-g0 intervention set mismatch")
    contract.require_sha256(
        payload["aggregate_output_stream_sha256"],
        "payload.aggregate_output_stream_sha256",
    )
    _require_bool(
        payload["repeat_aggregate_output_stream_bit_exact"],
        "payload.repeat_aggregate_output_stream_bit_exact",
        True,
    )
    _require_string(
        payload["output_digest_identity"],
        "payload.output_digest_identity",
        expected="sha256-framed-role-condition-decision-logit-value-f32le-v1",
    )
    nonclaims = contract.require_array(payload["nonclaims"], "payload.nonclaims")
    if not nonclaims or any(type(item) is not str or not item for item in nonclaims):
        contract.fail("payload.nonclaims must contain nonempty strings")

    contract_binding = {
        "feature_contract_digest": payload["feature_contract_digest"],
        "feature_encoding_digest": payload["feature_encoding_digest"],
        "feature_partition": feature_partition,
        "model_architecture_version": payload["model_architecture_version"],
        "model_config_fingerprint": payload["model_config_fingerprint"],
        "permutation_contract": payload["permutation"],
    }
    return {
        "candidate_checkpoint": candidate,
        "contract_binding": contract_binding,
        "corpus_binding": corpus,
        "g0_checkpoint": g0,
    }


def _verify_recorded_command(
    value: Any,
    *,
    expected_command: list[str],
    expected_timeout: int,
    where: str,
    extra_keys: set[str] | None = None,
) -> Mapping[str, Any]:
    keys = {
        "command",
        "exit_code",
        "stderr",
        "stdout",
        "timeout_seconds",
        "wall_time_ms",
    }
    keys.update(extra_keys or set())
    record = contract.exact_keys(value, keys, where)
    if record["command"] != expected_command:
        contract.fail(f"{where}.command mismatch")
    if record["exit_code"] != 0:
        contract.fail(f"{where}.exit_code must be zero")
    if record["timeout_seconds"] != expected_timeout:
        contract.fail(f"{where}.timeout_seconds mismatch")
    contract.require_natural(record["wall_time_ms"], f"{where}.wall_time_ms")
    for stream in ("stdout", "stderr"):
        stream_record = contract.exact_keys(
            record[stream], {"path", "sha256"}, f"{where}.{stream}"
        )
        if stream_record["sha256"] != contract.sha256_file(
            stream_record["path"]
        ):
            contract.fail(f"{where}.{stream} SHA-256 mismatch")
    return record


def _recorded_stream_bytes(
    record: Mapping[str, Any],
    stream: str,
    where: str,
) -> bytes:
    try:
        return Path(str(record[stream]["path"])).read_bytes()
    except OSError as error:
        raise contract.DiagnosticError(
            f"could not read {where} {stream}: {error}"
        ) from error


def _recorded_stdout_bytes(record: Mapping[str, Any], where: str) -> bytes:
    return _recorded_stream_bytes(record, "stdout", where)


def _recorded_stderr_bytes(record: Mapping[str, Any], where: str) -> bytes:
    return _recorded_stream_bytes(record, "stderr", where)


def _executed_test_statuses(raw: bytes) -> dict[str, str]:
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as error:
        raise contract.DiagnosticError(
            "recorded integrity-test stdout was not UTF-8"
        ) from error
    statuses: dict[str, str] = {}
    for line in text.splitlines():
        if not line.startswith("test "):
            continue
        name, separator, status = line[len("test ") :].rpartition(" ... ")
        if not separator:
            continue
        if status == "ok":
            normalized = "ok"
        elif status == "ignored" or status.startswith("ignored,"):
            normalized = "ignored"
        else:
            continue
        if name in statuses:
            contract.fail(f"duplicate recorded integrity-test result: {name}")
        statuses[name] = normalized
    return statuses


def _cargo_json_messages(path: Path) -> list[dict[str, Any]]:
    try:
        raw = path.read_bytes()
    except OSError as error:
        raise contract.DiagnosticError(
            f"could not read recorded Cargo JSONL: {error}"
        ) from error
    if len(raw) > contract.MAX_PROCESS_STREAM_BYTES:
        contract.fail("recorded Cargo JSONL exceeds the frozen size cap")
    messages: list[dict[str, Any]] = []
    for index, line in enumerate(raw.splitlines()):
        if not line.strip():
            continue
        messages.append(
            contract.parse_json_bytes(
                line, f"recorded Cargo JSONL line {index + 1}"
            )
        )
    if not messages:
        contract.fail("recorded Cargo JSONL contains no JSON messages")
    return messages


def verify_build_receipt(
    receipt: dict[str, Any],
    *,
    receipt_path: Path,
    repo: Path,
    manifest: Path,
    head: str,
    require_frozen_target_dir: bool = True,
) -> tuple[Path, dict[str, Any]]:
    try:
        receipt_raw = receipt_path.read_bytes()
    except OSError as error:
        raise contract.DiagnosticError(
            f"could not read build receipt bytes: {error}"
        ) from error
    if receipt_raw != contract.record_bytes(receipt):
        contract.fail("build receipt is not exact canonical JSON plus newline")
    contract.exact_keys(receipt, BUILD_RECEIPT_KEYS, "build receipt")
    contract.verify_payload_sha256(receipt, "build receipt")
    _require_string(
        receipt["schema"],
        "build receipt.schema",
        expected=contract.BUILD_RECEIPT_SCHEMA,
    )
    _require_string(
        receipt["label"], "build receipt.label", expected=contract.LABEL
    )
    if receipt["git_head"] != head:
        contract.fail("build receipt git_head does not match the clean worktree")
    _require_bool(
        receipt["git_status_clean_before_and_after"],
        "build receipt.git_status_clean_before_and_after",
        True,
    )

    manifest_record = contract.exact_keys(
        receipt["manifest"], {"path", "sha256"}, "build receipt.manifest"
    )
    if not contract.same_path(manifest_record["path"], manifest):
        contract.fail("build receipt manifest path mismatch")
    if manifest_record["sha256"] != contract.sha256_file(manifest):
        contract.fail("build receipt manifest SHA-256 mismatch")

    cargo_lock_path = repo / "Cargo.lock"
    cargo_lock = contract.exact_keys(
        receipt["cargo_lock"], {"path", "sha256"}, "build receipt.cargo_lock"
    )
    if not contract.same_path(cargo_lock["path"], cargo_lock_path):
        contract.fail("build receipt Cargo.lock path mismatch")
    if cargo_lock["sha256"] != contract.sha256_file(cargo_lock_path):
        contract.fail("build receipt Cargo.lock SHA-256 mismatch")

    for field, expected_path in (
        (
            "build_source",
            repo
            / "scripts"
            / "observation_diagnostics_v1"
            / "build_probe.py",
        ),
        (
            "contract_source",
            repo
            / "scripts"
            / "observation_diagnostics_v1"
            / "contract.py",
        ),
    ):
        source = contract.exact_keys(
            receipt[field], {"path", "sha256"}, f"build receipt.{field}"
        )
        if not contract.same_path(source["path"], expected_path):
            contract.fail(f"build receipt {field} path mismatch")
        if source["sha256"] != contract.sha256_file(expected_path):
            contract.fail(f"build receipt {field} SHA-256 mismatch")

    cargo = contract.exact_keys(
        receipt["cargo"],
        {
            "command",
            "environment",
            "exit_code",
            "locked",
            "no_default_features",
            "release",
            "requested_features",
            "stderr",
            "stdout",
            "target_dir",
            "wall_time_ms",
        },
        "build receipt.cargo",
    )
    if cargo["command"] != contract.cargo_build_command():
        contract.fail("build receipt Cargo command mismatch")
    for field in ("locked", "no_default_features", "release"):
        _require_bool(cargo[field], f"build receipt.cargo.{field}", True)
    if cargo["requested_features"] != []:
        contract.fail("build receipt must have no requested Cargo features")
    if cargo["exit_code"] != 0:
        contract.fail("build receipt Cargo exit code must be zero")
    if cargo["environment"] != {"CARGO_TARGET_DIR": cargo["target_dir"]}:
        contract.fail("build receipt CARGO_TARGET_DIR binding mismatch")
    if require_frozen_target_dir:
        contract.require_frozen_windows_path(
            cargo["target_dir"],
            contract.TARGET_DIR_WINDOWS,
            "build receipt Cargo target directory",
        )
        contract.require_frozen_windows_path(
            cargo["environment"]["CARGO_TARGET_DIR"],
            contract.TARGET_DIR_WINDOWS,
            "build receipt CARGO_TARGET_DIR environment",
        )
    cargo_stdout_path: Path | None = None
    for stream in ("stdout", "stderr"):
        stream_record = contract.exact_keys(
            cargo[stream], {"path", "sha256"}, f"build receipt.cargo.{stream}"
        )
        stream_path = Path(str(stream_record["path"]))
        if stream_record["sha256"] != contract.sha256_file(stream_path):
            contract.fail(f"build receipt Cargo {stream} hash mismatch")
        if stream == "stdout":
            cargo_stdout_path = stream_path
    assert cargo_stdout_path is not None

    executable_record = contract.exact_keys(
        receipt["executable"],
        {"compiler_artifact_target_kind", "path", "sha256"},
        "build receipt.executable",
    )
    if executable_record["compiler_artifact_target_kind"] != ["lib"]:
        contract.fail("build receipt executable target kind must be lib")
    executable = Path(str(executable_record["path"])).resolve()
    if executable_record["sha256"] != contract.sha256_file(executable):
        contract.fail("build receipt executable SHA-256 mismatch")
    cargo_executable = contract.resolve_lib_test_executable(
        _cargo_json_messages(cargo_stdout_path)
    ).resolve()
    if not contract.same_path(cargo_executable, executable):
        contract.fail(
            "build receipt executable differs from the sole Cargo JSON "
            f"lib-test executable: receipt={executable} cargo={cargo_executable}"
        )
    contract.require_descendant_path(
        cargo_executable,
        cargo["target_dir"],
        "Cargo-derived lib-test executable",
    )

    if receipt["required_tests"] != list(contract.REQUIRED_TESTS):
        contract.fail("build receipt required_tests mismatch")
    test_list = _verify_recorded_command(
        receipt["test_list"],
        expected_command=[str(executable), "--list"],
        expected_timeout=60,
        where="build receipt.test_list",
        extra_keys={"listed_test_count"},
    )
    try:
        listed = _recorded_stdout_bytes(
            test_list, "build receipt.test_list"
        ).decode("utf-8")
    except UnicodeDecodeError as error:
        raise contract.DiagnosticError("recorded test list was not UTF-8") from error
    listed_names = contract.listed_test_names(listed)
    if test_list["listed_test_count"] != len(listed_names):
        contract.fail("build receipt listed_test_count mismatch")
    missing = [name for name in contract.REQUIRED_TESTS if name not in listed_names]
    if missing:
        contract.fail(f"build receipt test list is missing required tests: {missing}")
    listed_module_tests = {
        name
        for name in listed_names
        if name.startswith(contract.TEST_MODULE + "::")
    }
    if listed_module_tests != set(contract.REQUIRED_TESTS):
        contract.fail("build receipt diagnostic module test-list drift")

    integrity = _verify_recorded_command(
        receipt["integrity_tests"],
        expected_command=contract.integrity_test_command(executable),
        expected_timeout=300,
        where="build receipt.integrity_tests",
        extra_keys={"executed_test_statuses"},
    )
    expected_statuses = {
        name: ("ignored" if name == contract.PROBE_TEST else "ok")
        for name in contract.REQUIRED_TESTS
    }
    if integrity["executed_test_statuses"] != expected_statuses:
        contract.fail("build receipt integrity-test status map mismatch")
    if (
        _executed_test_statuses(
            _recorded_stdout_bytes(integrity, "build receipt.integrity_tests")
        )
        != expected_statuses
    ):
        contract.fail("recorded integrity-test stdout/status mismatch")

    static_audit = contract.exact_keys(
        receipt["static_audit"],
        {"check", "report", "tests"},
        "build receipt.static_audit",
    )
    audit_report_path = repo / contract.STATIC_AUDIT_REPORT_RELATIVE_PATH
    report_record = contract.exact_keys(
        static_audit["report"],
        {"path", "payload_sha256", "schema", "sha256", "status"},
        "build receipt.static_audit.report",
    )
    if not contract.same_path(report_record["path"], audit_report_path):
        contract.fail("build receipt static-audit report path mismatch")
    if report_record["sha256"] != contract.sha256_file(audit_report_path):
        contract.fail("build receipt static-audit report SHA-256 mismatch")
    audit_report = contract.read_json_document(audit_report_path)
    contract.verify_payload_sha256(audit_report, "static audit report")
    if (
        report_record["schema"] != contract.STATIC_AUDIT_SCHEMA
        or audit_report.get("schema") != report_record["schema"]
    ):
        contract.fail("build receipt static-audit report schema mismatch")
    if (
        report_record["status"] not in contract.STATIC_AUDIT_STATUSES
        or audit_report.get("status") != report_record["status"]
        or audit_report.get("decision", {}).get("status")
        != report_record["status"]
    ):
        contract.fail("build receipt static-audit report status mismatch")
    if (
        report_record["payload_sha256"] != audit_report.get("payload_sha256")
    ):
        contract.fail("build receipt static-audit payload SHA-256 mismatch")
    audit_check = _verify_recorded_command(
        static_audit["check"],
        expected_command=contract.static_audit_check_command(),
        expected_timeout=60,
        where="build receipt.static_audit.check",
    )
    audit_tests = _verify_recorded_command(
        static_audit["tests"],
        expected_command=contract.static_audit_test_command(),
        expected_timeout=60,
        where="build receipt.static_audit.tests",
        extra_keys={"executed_test_count"},
    )
    if (
        audit_tests["executed_test_count"]
        != contract.STATIC_AUDIT_REQUIRED_TEST_COUNT
    ):
        contract.fail("build receipt static-audit unittest count mismatch")
    if (
        contract.unittest_success_count(
            _recorded_stderr_bytes(
                audit_tests, "build receipt.static_audit.tests"
            ),
            "recorded static-audit unittest stderr",
        )
        != contract.STATIC_AUDIT_REQUIRED_TEST_COUNT
    ):
        contract.fail("recorded static-audit unittest output/count mismatch")
    expected_marker = (
        f"{contract.STATIC_AUDIT_POSITIVE_MARKER} "
        f"status={report_record['status']} "
        f"payload_sha256={report_record['payload_sha256']}"
    )
    try:
        audit_check_text = _recorded_stdout_bytes(
            audit_check, "build receipt.static_audit.check"
        ).decode("utf-8")
    except UnicodeDecodeError as error:
        raise contract.DiagnosticError(
            "recorded static-audit check stdout was not UTF-8"
        ) from error
    if audit_check_text.splitlines().count(expected_marker) != 1:
        contract.fail("recorded static-audit positive marker mismatch")

    return executable, {
        "path": str(receipt_path.resolve()),
        "sha256": contract.sha256_file(receipt_path),
    }


def run_command_capture(
    command: list[str],
    *,
    cwd: Path,
    environment: Mapping[str, str],
    timeout_seconds: float,
) -> ProcessCapture:
    if timeout_seconds <= 0 or timeout_seconds > TIMEOUT_SECONDS:
        contract.fail(f"timeout must be within (0,{TIMEOUT_SECONDS}] seconds")
    started = time.perf_counter()
    try:
        process = subprocess.Popen(
            command,
            cwd=cwd,
            env=dict(environment),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except OSError as error:
        raise contract.DiagnosticError(f"could not launch probe: {error}") from error
    timed_out = False
    try:
        stdout, stderr = process.communicate(timeout=timeout_seconds)
    except subprocess.TimeoutExpired:
        timed_out = True
        process.kill()
        stdout, stderr = process.communicate()
    wall_time_ms = round((time.perf_counter() - started) * 1000)
    if len(stdout) > contract.MAX_PROCESS_STREAM_BYTES:
        contract.fail("captured probe stdout exceeds the size cap")
    if len(stderr) > contract.MAX_PROCESS_STREAM_BYTES:
        contract.fail("captured probe stderr exceeds the size cap")
    return ProcessCapture(
        exit_code=None if timed_out else process.returncode,
        stderr=stderr,
        stdout=stdout,
        timed_out=timed_out,
        wall_time_ms=wall_time_ms,
    )


def _file_record(path: Path, artifact_root: Path) -> dict[str, Any]:
    return {
        "byte_count": path.stat().st_size,
        "path": str(path.relative_to(artifact_root)).replace("\\", "/"),
        "sha256": contract.sha256_file(path),
    }


def _probe_environment(pair: contract.PairSpec) -> tuple[dict[str, str], dict[str, str]]:
    environment = os.environ.copy()
    for key in (
        "CUDA_DEVICE_ORDER",
        "MTG_KERNEL_PILOT_CUDA_ORDINAL",
        "CARGO_FEATURE_CUDA",
    ):
        environment.pop(key, None)
    bindings = {
        "CUDA_VISIBLE_DEVICES": "",
        "OBS_RELIANCE_CANDIDATE_GEN": str(pair.candidate_generation),
        "OBS_RELIANCE_EXPECTED_BASE_SEED": str(pair.seed),
        "OBS_RELIANCE_STORE_ROOT": pair.store_root,
    }
    environment.update(bindings)
    return environment, bindings


def _write_invocation_receipt(
    path: Path,
    receipt: dict[str, Any],
) -> dict[str, Any]:
    contract.attach_payload_sha256(receipt)
    contract.write_exclusive(path, contract.record_bytes(receipt))
    return receipt


def run_all(
    *,
    repo: Path,
    artifact_root: Path,
    manifest: Path,
    build_receipt_path: Path,
    require_windows: bool = True,
) -> dict[str, Any]:
    if require_windows:
        contract.require_frozen_windows_path(
            artifact_root,
            contract.ARTIFACT_ROOT_WINDOWS,
            "official artifact root",
        )
        contract.require_frozen_windows_path(
            build_receipt_path,
            contract.BUILD_RECEIPT_WINDOWS,
            "official build receipt",
        )
        if os.name != "nt":
            contract.fail("the frozen diagnostic runner must run with Windows Python")
    repo = repo.resolve()
    artifact_root = artifact_root.resolve()
    manifest = manifest.resolve()
    build_receipt_path = build_receipt_path.resolve()
    expected_manifest = (repo / contract.MANIFEST_RELATIVE_PATH).resolve()
    if not contract.same_path(manifest, expected_manifest):
        contract.fail(f"manifest must be the repository authority: {expected_manifest}")
    if not manifest.is_file():
        contract.fail(f"missing execution manifest: {manifest}")
    runs_root = artifact_root / "runs"
    completion_path = artifact_root / "completion-receipt.json"
    if runs_root.exists():
        contract.fail(f"refusing existing runs output: {runs_root}")
    if completion_path.exists():
        contract.fail(f"refusing existing completion output: {completion_path}")
    expected_build_receipt = (
        artifact_root / "build" / "build-receipt.json"
    ).resolve()
    if not contract.same_path(build_receipt_path, expected_build_receipt):
        contract.fail(
            f"build receipt must be the artifact-root authority: "
            f"{expected_build_receipt}"
        )

    head = contract.require_clean_worktree(repo)
    build_receipt = contract.read_json_document(build_receipt_path)
    executable, build_binding = verify_build_receipt(
        build_receipt,
        receipt_path=build_receipt_path,
        repo=repo,
        manifest=manifest,
        head=head,
        require_frozen_target_dir=require_windows,
    )
    manifest_sha256 = contract.sha256_file(manifest)
    executable_sha256 = contract.sha256_file(executable)
    runner_source = Path(__file__).resolve()
    contract_source = Path(contract.__file__).resolve()

    try:
        runs_root.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        raise contract.DiagnosticError(
            f"could not reserve runs output {runs_root}: {error}"
        ) from error

    started_utc = _utc_now()
    started = time.perf_counter()
    output_inventory: list[dict[str, Any]] = []
    invocation_summaries: list[dict[str, Any]] = []
    reference_corpus: dict[str, Any] | None = None
    reference_contract: dict[str, Any] | None = None

    for pair in contract.PAIR_SPECS:
        invocation_root = runs_root / pair.name
        invocation_root.mkdir()
        stdout_path = invocation_root / "stdout.log"
        stderr_path = invocation_root / "stderr.log"
        receipt_path = invocation_root / "invocation-receipt.json"
        command = contract.probe_command(executable)
        environment, environment_bindings = _probe_environment(pair)
        invocation_started_utc = _utc_now()
        capture = run_command_capture(
            command,
            cwd=repo,
            environment=environment,
            timeout_seconds=TIMEOUT_SECONDS,
        )
        invocation_completed_utc = _utc_now()
        contract.write_exclusive(stdout_path, capture.stdout)
        contract.write_exclusive(stderr_path, capture.stderr)
        base_receipt: dict[str, Any] = {
            "build_receipt": build_binding,
            "command": command,
            "completed_utc": invocation_completed_utc,
            "contract_environment": environment_bindings,
            "executable": {
                "path": str(executable),
                "sha256": executable_sha256,
            },
            "exit_code": capture.exit_code,
            "git_head": head,
            "label": contract.LABEL,
            "manifest_sha256": manifest_sha256,
            "pair": pair.as_record(),
            "schema": contract.INVOCATION_RECEIPT_SCHEMA,
            "started_utc": invocation_started_utc,
            "stderr": _file_record(stderr_path, artifact_root),
            "stdout": _file_record(stdout_path, artifact_root),
            "timed_out": capture.timed_out,
            "timeout_seconds": int(TIMEOUT_SECONDS),
            "wall_time_ms": capture.wall_time_ms,
        }
        try:
            if capture.timed_out:
                contract.fail(f"{pair.name}: probe exceeded the 120-second cap")
            if capture.exit_code != 0:
                contract.fail(
                    f"{pair.name}: probe exited with code {capture.exit_code}"
                )
            parsed = parse_probe_output(capture.stdout, capture.stderr)
            identity = verify_probe_payload(parsed.payload, pair)
            if reference_corpus is None:
                reference_corpus = identity["corpus_binding"]
                reference_contract = identity["contract_binding"]
            else:
                if identity["corpus_binding"] != reference_corpus:
                    contract.fail(
                        f"{pair.name}: fixed corpus identity differs across pairs"
                    )
                if identity["contract_binding"] != reference_contract:
                    contract.fail(
                        f"{pair.name}: model/feature contract identity differs across pairs"
                    )

            envelope_path = invocation_root / "probe-envelope.json"
            payload_path = invocation_root / "probe-payload.json"
            contract.write_exclusive(envelope_path, parsed.envelope_raw)
            contract.write_exclusive(payload_path, parsed.payload_raw)
            envelope_record = _file_record(envelope_path, artifact_root)
            payload_record = _file_record(payload_path, artifact_root)
            if payload_record["sha256"] != parsed.envelope["payload_sha256"]:
                contract.fail(f"{pair.name}: persisted raw payload SHA-256 mismatch")
            base_receipt.update(
                {
                    "probe": {
                        "aggregate_output_stream_sha256": (
                            parsed.payload["aggregate_output_stream_sha256"]
                        ),
                        "candidate_checkpoint": identity["candidate_checkpoint"],
                        "contract_binding": identity["contract_binding"],
                        "corpus_binding": identity["corpus_binding"],
                        "envelope": envelope_record,
                        "g0_checkpoint": identity["g0_checkpoint"],
                        "marker_count": 1,
                        "payload": payload_record,
                        "timing": parsed.timing,
                        "timing_line": parsed.timing_line,
                        "timing_line_count": 1,
                    },
                    "status": "VALID",
                }
            )
        except Exception as error:
            base_receipt.update(
                {
                    "failure": f"{type(error).__name__}: {error}",
                    "status": "INVALID",
                }
            )
            _write_invocation_receipt(receipt_path, base_receipt)
            raise

        _write_invocation_receipt(receipt_path, base_receipt)
        for path in (stdout_path, stderr_path, envelope_path, payload_path, receipt_path):
            output_inventory.append(_file_record(path, artifact_root))
        invocation_summaries.append(
            {
                "aggregate_output_stream_sha256": (
                    parsed.payload["aggregate_output_stream_sha256"]
                ),
                "candidate_generation": pair.candidate_generation,
                "envelope_sha256": envelope_record["sha256"],
                "invocation_receipt": _file_record(receipt_path, artifact_root),
                "name": pair.name,
                "payload_sha256": payload_record["sha256"],
                "seed": pair.seed,
                "stderr_sha256": base_receipt["stderr"]["sha256"],
                "stdout_sha256": base_receipt["stdout"]["sha256"],
                "wall_time_ms": capture.wall_time_ms,
            }
        )

    if len(invocation_summaries) != len(contract.PAIR_SPECS):
        contract.fail("runner did not complete all six frozen pairs")
    final_head = contract.require_clean_worktree(repo)
    if final_head != head:
        contract.fail(f"git HEAD changed during execution: before={head} after={final_head}")
    assert reference_corpus is not None
    assert reference_contract is not None
    completed_utc = _utc_now()
    completion: dict[str, Any] = {
        "build_receipt": build_binding,
        "completed_utc": completed_utc,
        "cpu_only": {
            "cargo_no_default_features": True,
            "cuda_visible_devices": "",
            "requested_cargo_features": [],
        },
        "cross_pair_invariants": {
            "contract_binding": reference_contract,
            "corpus_binding": reference_corpus,
            "identical_contract_and_config_identities": True,
            "identical_corpus_identity_and_sha256": True,
        },
        "elapsed_ms": round((time.perf_counter() - started) * 1000),
        "evidence_status": "DIAGNOSTIC-NON-EVIDENCE",
        "executable": {
            "path": str(executable),
            "sha256": executable_sha256,
        },
        "fixed_pairs": [pair.as_record() for pair in contract.PAIR_SPECS],
        "git_head": head,
        "git_status_clean_before_and_after": True,
        "invocation_count": len(invocation_summaries),
        "invocations": invocation_summaries,
        "label": contract.LABEL,
        "manifest": {
            "path": str(manifest),
            "sha256": manifest_sha256,
        },
        "output_inventory": sorted(
            output_inventory, key=lambda record: record["path"]
        ),
        "runner_source": {
            "path": str(runner_source),
            "sha256": contract.sha256_file(runner_source),
        },
        "contract_source": {
            "path": str(contract_source),
            "sha256": contract.sha256_file(contract_source),
        },
        "schema": contract.COMPLETION_RECEIPT_SCHEMA,
        "sequential_execution": True,
        "started_utc": started_utc,
        "status": "COMPLETE",
        "timeout_seconds_per_pair": int(TIMEOUT_SECONDS),
    }
    contract.attach_payload_sha256(completion)
    contract.write_exclusive(completion_path, contract.record_bytes(completion))
    print(
        "OBS_DIAGNOSTICS_COMPLETION="
        + json.dumps(completion, sort_keys=True, separators=(",", ":"))
    )
    return completion


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument(
        "--artifact-root",
        type=Path,
        default=Path(contract.ARTIFACT_ROOT_WINDOWS),
    )
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--build-receipt", type=Path)
    args = parser.parse_args()
    manifest = args.manifest or args.repo / contract.MANIFEST_RELATIVE_PATH
    build_receipt = (
        args.build_receipt
        or args.artifact_root / "build" / "build-receipt.json"
    )
    run_all(
        repo=args.repo,
        artifact_root=args.artifact_root,
        manifest=manifest,
        build_receipt_path=build_receipt,
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"OBS_DIAGNOSTICS_RUN_ABORT: {error}", file=sys.stderr)
        raise
