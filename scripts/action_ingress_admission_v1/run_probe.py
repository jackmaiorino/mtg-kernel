#!/usr/bin/env python3
"""Execute exactly three sequential action-ingress model reads fail closed."""

from __future__ import annotations

import argparse
import dataclasses
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import platform
import stat
import subprocess
import sys
import time
from typing import Any, Iterable, Mapping

try:
    from scripts.action_ingress_admission_v1 import contract
except ModuleNotFoundError:  # Direct execution from this directory.
    import contract  # type: ignore[no-redef]


MARKER = b"ACTION_INGRESS_ADMISSION_JSON="
HARNESS_PREFIX = (
    b"test " + contract.PROBE_TEST.encode("ascii") + b" ... " + MARKER
)

PAYLOAD_KEYS = {
    "schema",
    "label",
    "test_identity",
    "model",
    "corpus",
    "transform",
    "gate",
    "admission",
    "input_statistics",
    "first_layer_contribution_rms",
    "effects",
    "output_digests",
    "nonclaims",
}
MODEL_KEYS = {
    "role",
    "kind",
    "generation_index",
    "model_parameter_sha256",
    "parameter_manifest_sha256",
    "initialization_seed",
    "snapshot_identity",
    "snapshot_manifest_file_sha256",
    "snapshot_payload_sha256",
    "named_parameter_stream_sha256",
    "provenance",
    "prior_baseline_output_digest_identity",
    "prior_baseline_output_sha256",
}
PROVENANCE_KEYS = set(contract.MIRROR_PROVENANCE) - {"generation_index"}
CORPUS_KEYS = {
    "identity",
    "digest_identity",
    "sha256",
    "expected_sha256",
    "decision_count",
    "episode_count",
    "multi_action_decision_count",
    "total_action_count",
}
TRANSFORM_KEYS = {
    "structured_repair_identity",
    "slot",
    "effect_boolean_rule",
    "attacker_inclusion_rule",
    "blocker_inclusion_rule",
    "digest_gate_identity",
    "scientific_gate_modes",
    "scaled_gate_scientific_read",
}
GATE_KEYS = {
    "full_copies_digest_without_multiplication",
    "zero_uses_exact_positive_zero",
    "zero_stress_mapping",
    "zero_stress_equals_ordinary_zero",
    "invalid_scale_bits_fail_closed",
}
ADMISSION_KEYS = {
    "admitted",
    "corpus_digest_matches",
    "pre_transform_binding",
    "bitwise_comparison_identity",
    "exact_forward_capture_identity",
    "exact_forward_schema_version",
    "exact_forward_registry_version",
    "exact_forward_contract_digest",
    "exact_forward_encoding_digest",
    "exact_forward_hidden_dim",
    "exact_forward_schema_matches_frozen_contract",
    "exact_forward_capture_decision_count",
    "exact_forward_pooled_value_count",
    "canonical_semantics_pairwise_distinct",
    "repaired_zero_ingress_pairwise_distinct",
    "semantic_inclusion_pairs_complete_one_to_one",
    "semantic_inclusion_pair_direct_slot69_only",
    "semantic_inclusion_pair_pooled_refs_bit_exact",
    "repaired_zero_ingress_dim",
    "repaired_zero_ingress_row_count",
    "repaired_zero_ingress_sha256",
    "repaired_zero_ingress_row_digest_identity",
    "repaired_zero_ingress_row_digests",
    "attacker_false_true_pair_count",
    "blocker_false_true_pair_count",
    "attacker_pairs_witnessed",
    "blocker_pairs_witnessed",
    "non_action_tensors_bit_exact",
    "zero_stress_bit_exact",
    "zero_stress_tensors_bit_exact",
    "zero_stress_outputs_bit_exact",
    "every_action_only_intervention_value_bits_invariant",
    "model_parameters_bit_exact_before_after",
}
PRE_TRANSFORM_BINDING_KEYS = {
    "identity",
    "transcript_encoding",
    "all_rows_passed",
    "decision_count",
    "row_count",
    "action_reference_count",
    "operational_object_count",
    "action_object_projection_count",
    "live_session_semantics_to_core_refs_revalidated_at_capture",
    "live_session_semantics_to_core_refs_revalidated_pre_transform",
    "typed_semantics_exact",
    "production_v2_binding_exact",
    "operational_core_refs_exact",
    "scorer_core_refs_exact",
    "operational_object_to_scorer_model_object_exact",
    "zone_change_count_retained_in_operational_identity",
    "count_and_order_exact",
    "action_kind_exact",
    "action_core_exact",
    "action_references_exact",
    "canonical_model_json_exact",
    "canonical_model_digest_exact",
    "frozen_digest_tail_exact",
    "capture_sha256",
    "revalidated_sha256",
    "capture_matches_revalidation",
}
INPUT_STAT_KEYS = {
    "source_condition",
    "direct_value_count",
    "digest_value_count",
    "direct_value_rms",
    "digest_value_rms",
    "mean_direct_squared_norm",
    "mean_digest_squared_norm",
    "per_action_row",
}
INPUT_ROW_KEYS = {
    "decision_index",
    "action_index",
    "direct_squared_norm",
    "digest_squared_norm",
}
CONTRIBUTION_KEYS = {
    "source_condition",
    "tensor_name",
    "accumulator",
    "hidden_dim",
    "direct_contribution_rms",
    "digest_contribution_rms",
    "per_action_row",
}
CONTRIBUTION_ROW_KEYS = {
    "decision_index",
    "action_index",
    "direct_contribution_rms",
    "digest_contribution_rms",
}
EFFECTS_KEYS = {
    "direct_sibling",
    "digest_sibling",
    "repaired_full_vs_repaired_zero",
    "digest_minus_direct",
    "descriptive_label",
}
EFFECT_KEYS = {
    "name",
    "output_sha256",
    "multi_action_decision_count",
    "mean_jensen_shannon_nats",
    "mean_centered_logit_rms_delta",
    "top_action_flip_count",
    "top_action_flip_fraction",
    "value_bits_invariant",
}
CONTRAST_KEYS = {
    "mean_jensen_shannon_nats",
    "mean_centered_logit_rms_delta",
    "top_action_flip_fraction",
}
OUTPUT_KEYS = {
    "digest_identity",
    "baseline_frozen_full",
    "repaired_full",
    "repaired_zero",
    "repeated_baseline_frozen_full_bit_exact",
    "repeated_repaired_full_bit_exact",
    "repeated_repaired_zero_bit_exact",
    "zero_stress_equals_repaired_zero",
    "repair_only_value_bits_invariant",
    # The Rust producer adds these two fields for imported roles (null for raw)
    # so the prior-v2 stream assertion is externally auditable.
    "prior_baseline_reproduced_sha256",
    "prior_baseline_exact_match",
}


@dataclasses.dataclass(frozen=True)
class ParsedProbe:
    envelope: dict[str, Any]
    envelope_raw: bytes
    payload: dict[str, Any]
    payload_raw: bytes


@dataclasses.dataclass(frozen=True)
class ProcessCapture:
    exit_code: int | None
    stdout: bytes
    stderr: bytes
    timed_out: bool
    wall_time_ms: int


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _number(value: Any, where: str, *, nonnegative: bool = False) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        contract.fail(f"{where} must be a finite JSON number")
    result = float(value)
    if not math.isfinite(result):
        contract.fail(f"{where} must be finite")
    if nonnegative and result < 0.0:
        contract.fail(f"{where} must be nonnegative")
    return result


def _within_ulps(left: float, right: float, *, ulps: int = 64) -> bool:
    if left == right:
        return True
    tolerance = ulps * max(math.ulp(left), math.ulp(right))
    return abs(left - right) <= tolerance


def _exact_natural(value: Any, expected: int, where: str) -> int:
    actual = contract.require_natural(value, where)
    if actual != expected:
        contract.fail(f"{where} mismatch: expected={expected} actual={actual}")
    return actual


def _read_bytes(path: Path, where: str) -> bytes:
    try:
        raw = path.read_bytes()
    except OSError as error:
        raise contract.AdmissionError(f"could not read {where}: {error}") from error
    if len(raw) > contract.MAX_STREAM_BYTES:
        contract.fail(f"{where} exceeds the stream size cap")
    return raw


def _verify_file_binding(
    value: Any,
    *,
    expected_path: Path | None,
    where: str,
) -> Path:
    record = contract.exact_keys(value, {"byte_count", "path", "sha256"}, where)
    path = Path(contract.require_string(record["path"], f"{where}.path")).resolve()
    if expected_path is not None and not contract.same_path(path, expected_path):
        contract.fail(f"{where}.path mismatch")
    size = contract.require_natural(record["byte_count"], f"{where}.byte_count")
    try:
        actual_size = path.stat().st_size
    except OSError as error:
        raise contract.AdmissionError(f"could not stat {where}: {error}") from error
    if actual_size != size:
        contract.fail(f"{where}.byte_count mismatch")
    expected_sha = contract.require_sha256(record["sha256"], f"{where}.sha256")
    if contract.sha256_file(path) != expected_sha:
        contract.fail(f"{where}.sha256 mismatch")
    return path


def _verify_command_record(
    value: Any,
    *,
    expected_command: list[str],
    expected_timeout: int,
    where: str,
    extra_keys: Iterable[str] = (),
    expected_stdout: Path | None = None,
    expected_stderr: Path | None = None,
) -> Mapping[str, Any]:
    record = contract.exact_keys(
        value,
        {
            "command",
            "exit_code",
            "stderr",
            "stdout",
            "timeout_seconds",
            "wall_time_ms",
            *extra_keys,
        },
        where,
    )
    if record["command"] != expected_command:
        contract.fail(f"{where}.command mismatch")
    _exact_natural(record["exit_code"], 0, f"{where}.exit_code")
    _exact_natural(
        record["timeout_seconds"], expected_timeout, f"{where}.timeout_seconds"
    )
    contract.require_natural(record["wall_time_ms"], f"{where}.wall_time_ms")
    for stream, expected_path in (
        ("stdout", expected_stdout),
        ("stderr", expected_stderr),
    ):
        _verify_file_binding(
            record[stream], expected_path=expected_path, where=f"{where}.{stream}"
        )
    return record


def _recorded_test_statuses(stdout: bytes) -> dict[str, str]:
    try:
        text = stdout.decode("utf-8")
    except UnicodeDecodeError as error:
        raise contract.AdmissionError("recorded Rust control stdout is not UTF-8") from error
    statuses: dict[str, str] = {}
    for line in text.splitlines():
        if not line.startswith("test "):
            continue
        name, separator, status = line[len("test ") :].rpartition(" ... ")
        if not separator:
            continue
        normalized = (
            "ok"
            if status == "ok"
            else "ignored"
            if status == "ignored" or status.startswith("ignored,")
            else None
        )
        if normalized is not None:
            if name in statuses:
                contract.fail(f"duplicate recorded Rust test result: {name}")
            statuses[name] = normalized
    return statuses


def verify_build_receipt(
    receipt_path: Path,
    *,
    repo: Path,
    artifact_root: Path,
    target_dir: Path,
    require_windows: bool = True,
) -> tuple[dict[str, Any], Path, str, str]:
    build_root = artifact_root / "build"
    expected_receipt_path = build_root / "build-receipt.json"
    if (
        not build_root.is_dir()
        or _is_reparse_point(build_root)
        or not receipt_path.is_file()
        or _is_reparse_point(receipt_path)
        or not contract.same_path(receipt_path, expected_receipt_path)
    ):
        contract.fail("build root and receipt must occupy non-reparse frozen paths")
    receipt = contract.read_canonical_record(receipt_path, "build receipt")
    contract.exact_keys(
        receipt,
        {
            "cargo",
            "cargo_lock",
            "build_environment_policy",
            "completed_utc",
            "cpu_only",
            "executable",
            "frozen_inputs",
            "git_head",
            "git_branch",
            "git_status_clean_before_and_after",
            "implementation_sources",
            "label",
            "manifest",
            "output_inventory",
            "packaging_preflight",
            "payload_sha256",
            "required_tests",
            "rust_controls",
            "runtime_tuple",
            "schema",
            "started_utc",
            "static_preflight",
            "test_list",
            "toolchain",
        },
        "build receipt",
    )
    contract.require_string(
        receipt["schema"], "build receipt.schema", contract.BUILD_RECEIPT_SCHEMA
    )
    contract.require_string(receipt["label"], "build receipt.label", contract.LABEL)
    contract.require_bool(receipt["cpu_only"], "build receipt.cpu_only", True)
    contract.require_bool(
        receipt["git_status_clean_before_and_after"],
        "build receipt.git_status_clean_before_and_after",
        True,
    )
    head = contract.require_clean_worktree(repo)
    branch = contract.require_frozen_branch(repo)
    if receipt["git_head"] != head:
        contract.fail("build receipt git_head differs from clean current HEAD")
    if receipt["git_branch"] != branch:
        contract.fail("build receipt git_branch mismatch")
    manifest = repo / contract.MANIFEST_RELATIVE_PATH
    manifest_record = contract.exact_keys(
        receipt["manifest"], {"path", "sha256"}, "build receipt.manifest"
    )
    if (
        not contract.same_path(manifest_record["path"], manifest)
        or manifest_record["sha256"] != contract.sha256_file(manifest)
    ):
        contract.fail("build receipt manifest binding mismatch")
    if receipt["frozen_inputs"] != contract.verify_frozen_inputs(repo):
        contract.fail("build receipt frozen-input bindings mismatch")
    if receipt["implementation_sources"] != contract.implementation_source_records(repo):
        contract.fail("build receipt implementation-source bindings mismatch")
    cargo_lock = repo / "Cargo.lock"
    cargo_lock_record = contract.exact_keys(
        receipt["cargo_lock"], {"path", "sha256"}, "build receipt.cargo_lock"
    )
    if (
        not contract.same_path(cargo_lock_record["path"], cargo_lock)
        or cargo_lock_record["sha256"] != contract.sha256_file(cargo_lock)
    ):
        contract.fail("build receipt Cargo.lock binding mismatch")
    if receipt["required_tests"] != list(contract.REQUIRED_TESTS):
        contract.fail("build receipt required_tests mismatch")

    executable_record = contract.exact_keys(
        receipt["executable"], {"path", "sha256"}, "build receipt.executable"
    )
    executable = Path(
        contract.require_string(executable_record["path"], "build receipt executable path")
    ).resolve()
    if executable_record["sha256"] != contract.sha256_file(executable):
        contract.fail("build receipt executable hash mismatch")
    contract.require_descendant_path(executable, target_dir, "build executable")
    if require_windows:
        contract.require_frozen_windows_path(
            repo, contract.WORKTREE_WINDOWS, "repository worktree"
        )
        contract.require_frozen_windows_path(
            artifact_root, contract.ARTIFACT_ROOT_WINDOWS, "artifact root"
        )
        contract.require_frozen_windows_path(
            target_dir, contract.TARGET_DIR_WINDOWS, "Cargo target"
        )

    rustup = contract.resolved_tool_executable("rustup")
    tool_paths = {
        name: contract.selected_rust_toolchain_executable(name, rustup=rustup)
        for name in ("cargo", "rustc")
    }
    expected_environment_policy = contract.build_environment_policy(
        os.environ,
        repo=repo,
        target_dir=target_dir,
        rustc=tool_paths["rustc"],
    )
    if receipt["build_environment_policy"] != expected_environment_policy:
        contract.fail("build receipt environment policy mismatch")
    toolchain = contract.exact_keys(
        receipt["toolchain"],
        {"cargo", "channel", "rustc", "rustup"},
        "build receipt.toolchain",
    )
    contract.require_string(
        toolchain["channel"],
        "build receipt.toolchain.channel",
        contract.RUST_TOOLCHAIN_CHANNEL,
    )
    for name, path in (
        ("cargo", tool_paths["cargo"]),
        ("rustc", tool_paths["rustc"]),
        ("rustup", rustup),
    ):
        _verify_file_binding(
            toolchain[name],
            expected_path=path,
            where=f"build receipt.toolchain.{name}",
        )

    cargo = _verify_command_record(
        receipt["cargo"],
        expected_command=contract.cargo_build_command(str(tool_paths["cargo"])),
        expected_timeout=1_800,
        where="build receipt.cargo",
        extra_keys={"environment"},
        expected_stdout=artifact_root / "build" / "cargo-build.jsonl",
        expected_stderr=artifact_root / "build" / "cargo-build.stderr.log",
    )
    expected_environment = {
        "CARGO_TARGET_DIR": str(target_dir.resolve()),
        "CUDA_VISIBLE_DEVICES": "",
        "MTG_KERNEL_DEVICE": "cpu",
        "RUSTC": str(tool_paths["rustc"].resolve()),
    }
    if cargo["environment"] != expected_environment:
        contract.fail("build receipt Cargo environment mismatch")

    test_list = _verify_command_record(
        receipt["test_list"],
        expected_command=[str(executable), "--list"],
        expected_timeout=60,
        where="build receipt.test_list",
        extra_keys={"listed_test_count"},
        expected_stdout=artifact_root / "build" / "test-list.stdout.log",
        expected_stderr=artifact_root / "build" / "test-list.stderr.log",
    )
    listed_raw = _read_bytes(
        Path(test_list["stdout"]["path"]), "recorded Rust test-list stdout"
    )
    try:
        listed = contract.listed_test_names(listed_raw.decode("utf-8"))
    except UnicodeDecodeError as error:
        raise contract.AdmissionError("recorded test list is not UTF-8") from error
    if test_list["listed_test_count"] != len(listed):
        contract.fail("build receipt listed_test_count mismatch")
    _exact_natural(
        test_list["listed_test_count"],
        len(listed),
        "build receipt.test_list.listed_test_count",
    )
    module_tests = {name for name in listed if name.startswith(contract.TEST_MODULE + "::")}
    if module_tests != set(contract.REQUIRED_TESTS):
        contract.fail("recorded Rust module test list drift")

    controls = _verify_command_record(
        receipt["rust_controls"],
        expected_command=contract.integrity_test_command(executable),
        expected_timeout=300,
        where="build receipt.rust_controls",
        extra_keys={"executed_test_statuses"},
        expected_stdout=artifact_root / "build" / "rust-controls.stdout.log",
        expected_stderr=artifact_root / "build" / "rust-controls.stderr.log",
    )
    expected_statuses = {
        name: ("ignored" if name == contract.PROBE_TEST else "ok")
        for name in contract.REQUIRED_TESTS
    }
    if controls["executed_test_statuses"] != expected_statuses:
        contract.fail("build receipt Rust control statuses mismatch")
    if (
        _recorded_test_statuses(
            _read_bytes(
                Path(controls["stdout"]["path"]),
                "recorded Rust control stdout",
            )
        )
        != expected_statuses
    ):
        contract.fail("recorded Rust control stdout/status mismatch")

    static = contract.exact_keys(
        receipt["static_preflight"], {"report", "runs"}, "build receipt.static_preflight"
    )
    report_record = contract.exact_keys(
        static["report"],
        {"path", "payload_sha256", "schema", "sha256", "status"},
        "build receipt.static_preflight.report",
    )
    static_report_path = repo / contract.STATIC_REPORT_RELATIVE_PATH
    if (
        not contract.same_path(report_record["path"], static_report_path)
        or report_record["sha256"] != contract.STATIC_REPORT_SHA256
        or report_record["payload_sha256"] != contract.STATIC_REPORT_PAYLOAD_SHA256
        or report_record["schema"] != contract.STATIC_REPORT_SCHEMA
        or report_record["status"] != "STATIC-ADMITTED"
        or contract.sha256_file(static_report_path) != report_record["sha256"]
    ):
        contract.fail("build receipt static report binding mismatch")
    static_runs = contract.require_array(static["runs"], "build static runs", 4)
    expected_commands = [
        ("linux-check", contract.linux_static_commands()[0], False),
        ("linux-tests", contract.linux_static_commands()[1], True),
        ("windows-check", contract.windows_static_commands(sys.executable)[0], False),
        ("windows-tests", contract.windows_static_commands(sys.executable)[1], True),
    ]
    for index, (name, command, is_test) in enumerate(expected_commands):
        extra = {"name", *(["executed_test_count"] if is_test else [])}
        record = _verify_command_record(
            static_runs[index],
            expected_command=command,
            expected_timeout=120,
            where=f"build static runs[{index}]",
            extra_keys=extra,
            expected_stdout=artifact_root / "build" / f"{name}.stdout.log",
            expected_stderr=artifact_root / "build" / f"{name}.stderr.log",
        )
        if record["name"] != name:
            contract.fail(f"build static run {index} name mismatch")
        if is_test and record["executed_test_count"] != contract.STATIC_REQUIRED_TEST_COUNT:
            contract.fail(f"build static run {name} count mismatch")
        if is_test:
            _exact_natural(
                record["executed_test_count"],
                contract.STATIC_REQUIRED_TEST_COUNT,
                f"build static run {name}.executed_test_count",
            )
        stdout = _read_bytes(
            Path(record["stdout"]["path"]), f"recorded {name} stdout"
        )
        stderr = _read_bytes(
            Path(record["stderr"]["path"]), f"recorded {name} stderr"
        )
        if is_test:
            if (
                contract.unittest_success_count(stderr, f"recorded {name} stderr")
                != contract.STATIC_REQUIRED_TEST_COUNT
            ):
                contract.fail(f"recorded {name} unittest summary mismatch")
        else:
            expected_marker = (
                f"{contract.STATIC_POSITIVE_MARKER} status=STATIC-ADMITTED "
                f"combined_identity_sha256={contract.STATIC_COMBINED_IDENTITY_SHA256} "
                f"payload_sha256={contract.STATIC_REPORT_PAYLOAD_SHA256}"
            )
            try:
                lines = stdout.decode("utf-8").splitlines()
            except UnicodeDecodeError as error:
                raise contract.AdmissionError(
                    f"recorded {name} stdout is not UTF-8"
                ) from error
            if lines.count(expected_marker) != 1:
                contract.fail(f"recorded {name} positive marker mismatch")

    packaging = contract.exact_keys(
        receipt["packaging_preflight"],
        {"required_test_count", "runs"},
        "build receipt.packaging_preflight",
    )
    _exact_natural(
        packaging["required_test_count"],
        contract.PACKAGING_REQUIRED_TEST_COUNT,
        "build receipt.packaging_preflight.required_test_count",
    )
    packaging_runs = contract.require_array(
        packaging["runs"], "build packaging runs", 2
    )
    expected_packaging_commands = [
        ("linux-packaging-tests", contract.linux_packaging_test_command()),
        (
            "windows-packaging-tests",
            contract.windows_packaging_test_command(sys.executable),
        ),
    ]
    for index, (name, command) in enumerate(expected_packaging_commands):
        record = _verify_command_record(
            packaging_runs[index],
            expected_command=command,
            expected_timeout=120,
            where=f"build packaging runs[{index}]",
            extra_keys={"executed_test_count", "name"},
            expected_stdout=artifact_root / "build" / f"{name}.stdout.log",
            expected_stderr=artifact_root / "build" / f"{name}.stderr.log",
        )
        contract.require_string(record["name"], f"build packaging run {index}.name", name)
        _exact_natural(
            record["executed_test_count"],
            contract.PACKAGING_REQUIRED_TEST_COUNT,
            f"build packaging run {name}.executed_test_count",
        )
        stderr = _read_bytes(
            Path(record["stderr"]["path"]), f"recorded {name} stderr"
        )
        if (
            contract.unittest_success_count(stderr, f"recorded {name} stderr")
            != contract.PACKAGING_REQUIRED_TEST_COUNT
        ):
            contract.fail(f"recorded {name} unittest summary mismatch")

    runtime = contract.exact_keys(
        receipt["runtime_tuple"],
        {
            "machine",
            "os_name",
            "platform",
            "python_executable",
            "python_version",
            "sys_platform",
            "tools",
        },
        "build receipt.runtime_tuple",
    )
    expected_runtime = {
        "machine": platform.machine(),
        "os_name": os.name,
        "platform": platform.platform(),
        "python_executable": sys.executable,
        "python_version": sys.version,
        "sys_platform": sys.platform,
    }
    for field, expected in expected_runtime.items():
        if runtime[field] != expected:
            contract.fail(f"build runtime tuple {field} mismatch")
    tools = contract.exact_keys(
        runtime["tools"], {"cargo", "rustc"}, "build runtime tools"
    )
    for name in ("cargo", "rustc"):
        record = _verify_command_record(
            tools[name],
            expected_command=[str(tool_paths[name]), "--version", "--verbose"],
            expected_timeout=60,
            where=f"build runtime tools.{name}",
            extra_keys={"executable", "version_stdout"},
            expected_stdout=artifact_root / "build" / f"runtime-{name}.stdout.log",
            expected_stderr=artifact_root / "build" / f"runtime-{name}.stderr.log",
        )
        _verify_file_binding(
            record["executable"],
            expected_path=tool_paths[name],
            where=f"build runtime tools.{name}.executable",
        )
        raw = _read_bytes(
            Path(record["stdout"]["path"]), f"recorded runtime {name} stdout"
        )
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError as error:
            raise contract.AdmissionError(
                f"recorded runtime {name} stdout is not UTF-8"
            ) from error
        if not text.strip() or record["version_stdout"] != text:
            contract.fail(f"recorded runtime {name} text mismatch")

    cargo_messages = [
        contract.parse_json_bytes(line, f"recorded Cargo JSON line {index + 1}")
        for index, line in enumerate(
            _read_bytes(Path(cargo["stdout"]["path"]), "recorded Cargo stdout").splitlines()
        )
        if line.strip()
    ]
    cargo_executable = contract.resolve_lib_test_executable(cargo_messages).resolve()
    if not contract.same_path(cargo_executable, executable):
        contract.fail("build executable differs from Cargo compiler artifact")

    expected_build_names = {
        "cargo-build.jsonl",
        "cargo-build.stderr.log",
        "linux-check.stderr.log",
        "linux-check.stdout.log",
        "linux-tests.stderr.log",
        "linux-tests.stdout.log",
        "linux-packaging-tests.stderr.log",
        "linux-packaging-tests.stdout.log",
        "rust-controls.stderr.log",
        "rust-controls.stdout.log",
        "runtime-cargo.stderr.log",
        "runtime-cargo.stdout.log",
        "runtime-rustc.stderr.log",
        "runtime-rustc.stdout.log",
        "test-list.stderr.log",
        "test-list.stdout.log",
        "windows-check.stderr.log",
        "windows-check.stdout.log",
        "windows-tests.stderr.log",
        "windows-tests.stdout.log",
        "windows-packaging-tests.stderr.log",
        "windows-packaging-tests.stdout.log",
    }
    inventory = contract.require_array(
        receipt["output_inventory"], "build receipt.output_inventory", 22
    )
    verified_inventory: list[dict[str, Any]] = []
    for index, raw in enumerate(inventory):
        record = contract.exact_keys(
            raw,
            {"byte_count", "path", "sha256"},
            f"build receipt.output_inventory[{index}]",
        )
        relative = contract.require_string(
            record["path"], f"build receipt.output_inventory[{index}].path"
        )
        expected_relative = PurePosixPath("build") / PurePosixPath(relative).name
        if relative != str(expected_relative):
            contract.fail("build inventory path must be a direct build/ child")
        path = artifact_root / relative
        if _is_reparse_point(path):
            contract.fail("build inventory entry must not be a reparse point")
        actual = contract.file_record(path, root=artifact_root)
        _exact_natural(
            record["byte_count"],
            actual["byte_count"],
            f"build receipt.output_inventory[{index}].byte_count",
        )
        contract.require_sha256(
            record["sha256"],
            f"build receipt.output_inventory[{index}].sha256",
        )
        if dict(record) != actual:
            contract.fail("build inventory byte binding mismatch")
        verified_inventory.append(actual)
    verified_inventory.sort(key=lambda value: value["path"])
    if inventory != verified_inventory:
        contract.fail("build inventory must be sorted")
    if {PurePosixPath(value["path"]).name for value in inventory} != expected_build_names:
        contract.fail("build inventory exact file set mismatch")
    build_entries = list((artifact_root / "build").iterdir())
    actual_build_names = {
        path.name for path in build_entries if path.name != "build-receipt.json"
    }
    if (
        actual_build_names != expected_build_names
        or any(not path.is_file() or _is_reparse_point(path) for path in build_entries)
        or {path.name for path in build_entries}
        != expected_build_names | {"build-receipt.json"}
    ):
        contract.fail("build directory contains an unexpected pre-receipt file")
    return receipt, executable, head, branch


def parse_probe_output(stdout: bytes, stderr: bytes) -> ParsedProbe:
    if len(stdout) > contract.MAX_STREAM_BYTES or len(stderr) > contract.MAX_STREAM_BYTES:
        contract.fail("probe stream exceeds frozen size cap")
    if stderr.count(MARKER):
        contract.fail("probe marker is forbidden on stderr")
    if stdout.count(MARKER) != 1:
        contract.fail("probe stdout must contain exactly one admission marker")
    lines = stdout.splitlines()
    indices = [index for index, line in enumerate(lines) if line.startswith(HARNESS_PREFIX)]
    if len(indices) != 1:
        contract.fail("probe marker must have the exact libtest prefix")
    index = indices[0]
    if index == 0 or lines[index - 1] != b"running 1 test":
        contract.fail("probe marker must follow the one-test harness header")
    if index + 1 >= len(lines) or lines[index + 1] != b"ok":
        contract.fail("probe marker must be followed by libtest ok")
    envelope_raw = lines[index][len(HARNESS_PREFIX) :]
    envelope_decimal = contract.parse_json_bytes(envelope_raw, "probe envelope")
    contract.exact_keys(
        envelope_decimal, {"schema", "payload_sha256", "payload"}, "probe envelope"
    )
    contract.require_string(
        envelope_decimal["schema"], "probe envelope.schema", contract.PROBE_ENVELOPE_SCHEMA
    )
    payload_sha = contract.require_sha256(
        envelope_decimal["payload_sha256"], "probe envelope.payload_sha256"
    )
    prefix = (
        b'{"schema":"'
        + contract.PROBE_ENVELOPE_SCHEMA.encode("ascii")
        + b'","payload_sha256":"'
        + payload_sha.encode("ascii")
        + b'","payload":'
    )
    if not envelope_raw.startswith(prefix) or not envelope_raw.endswith(b"}"):
        contract.fail("probe envelope differs from exact compact Rust field order")
    payload_raw = envelope_raw[len(prefix) : -1]
    if contract.sha256_bytes(payload_raw) != payload_sha:
        contract.fail("raw producer payload SHA-256 mismatch")
    payload = contract.parse_json_bytes_native_numbers(payload_raw, "probe payload")
    envelope = contract.parse_json_bytes_native_numbers(envelope_raw, "probe envelope")
    if envelope["payload"] != payload:
        contract.fail("envelope payload differs from extracted raw payload")
    return ParsedProbe(envelope, envelope_raw, payload, payload_raw)


def _verify_rows(
    rows_value: Any,
    *,
    keys: set[str],
    metric_fields: tuple[str, str],
    where: str,
) -> list[Mapping[str, Any]]:
    rows = contract.require_array(rows_value, where, 1_115)
    last_decision = -1
    next_action = 0
    for index, raw in enumerate(rows):
        row = contract.exact_keys(raw, keys, f"{where}[{index}]")
        decision = contract.require_natural(
            row["decision_index"], f"{where}[{index}].decision_index"
        )
        action = contract.require_natural(
            row["action_index"], f"{where}[{index}].action_index"
        )
        if decision == last_decision:
            if action != next_action:
                contract.fail(f"{where} action indices are not contiguous")
        elif decision == last_decision + 1:
            if action != 0:
                contract.fail(f"{where} first action index must be zero")
            last_decision = decision
        else:
            contract.fail(f"{where} decision indices are not contiguous")
        next_action = action + 1
        for field in metric_fields:
            _number(row[field], f"{where}[{index}].{field}", nonnegative=True)
    if last_decision != 255:
        contract.fail(f"{where} must cover decisions 0..255")
    return rows


def _verify_ingress_digest_rows(value: Any) -> list[Mapping[str, Any]]:
    where = "payload.admission.repaired_zero_ingress_row_digests"
    rows = contract.require_array(value, where, 1_115)
    last_decision = -1
    next_action = 0
    decision_digests: set[str] = set()
    for index, raw in enumerate(rows):
        row = contract.exact_keys(
            raw, {"decision_index", "action_index", "sha256"}, f"{where}[{index}]"
        )
        decision = contract.require_natural(
            row["decision_index"], f"{where}[{index}].decision_index"
        )
        action = contract.require_natural(
            row["action_index"], f"{where}[{index}].action_index"
        )
        if decision == last_decision:
            if action != next_action:
                contract.fail(f"{where} action indices are not contiguous")
        elif decision == last_decision + 1:
            if action != 0:
                contract.fail(f"{where} first action index must be zero")
            last_decision = decision
            decision_digests = set()
        else:
            contract.fail(f"{where} decision indices are not contiguous")
        next_action = action + 1
        digest = contract.require_sha256(row["sha256"], f"{where}[{index}].sha256")
        if digest in decision_digests:
            contract.fail(f"{where} contains a within-decision duplicate row-bit digest")
        decision_digests.add(digest)
    if last_decision != 255:
        contract.fail(f"{where} must cover decisions 0..255")
    return rows


def _expected_label(role: str, contrast: Mapping[str, Any]) -> str:
    values = [_number(contrast[field], f"contrast.{field}") for field in CONTRAST_KEYS]
    if all(value > 0.0 for value in values):
        suffix = "DIGEST-DOMINANT"
    elif all(value < 0.0 for value in values):
        suffix = "DIRECT-DOMINANT"
    else:
        suffix = "MIXED"
    return f"RAW-INIT-{suffix}" if role == "raw-common-snapshot" else f"IMPORTED-{suffix}"


def _verify_effect(value: Any, *, name: str, where: str) -> Mapping[str, Any]:
    effect = contract.exact_keys(value, EFFECT_KEYS, where)
    contract.require_string(effect["name"], f"{where}.name", name)
    contract.require_sha256(effect["output_sha256"], f"{where}.output_sha256")
    _exact_natural(
        effect["multi_action_decision_count"],
        256,
        f"{where}.multi_action_decision_count",
    )
    for field in ("mean_jensen_shannon_nats", "mean_centered_logit_rms_delta"):
        _number(effect[field], f"{where}.{field}", nonnegative=True)
    flips = contract.require_natural(effect["top_action_flip_count"], f"{where}.flip_count")
    if flips > 256:
        contract.fail(f"{where}.top_action_flip_count exceeds 256")
    fraction = _number(
        effect["top_action_flip_fraction"], f"{where}.top_action_flip_fraction", nonnegative=True
    )
    if fraction != flips / 256:
        contract.fail(f"{where}.top_action_flip_fraction is not count/256")
    contract.require_bool(effect["value_bits_invariant"], f"{where}.value_bits_invariant", True)
    return effect


def verify_probe_payload(payload: dict[str, Any], spec: contract.ModelSpec) -> dict[str, Any]:
    contract.exact_keys(payload, PAYLOAD_KEYS, "payload")
    contract.require_string(payload["schema"], "payload.schema", contract.PROBE_PAYLOAD_SCHEMA)
    contract.require_string(payload["label"], "payload.label", contract.LABEL)
    contract.require_string(payload["test_identity"], "payload.test_identity", contract.PROBE_TEST)

    model = contract.exact_keys(payload["model"], MODEL_KEYS, "payload.model")
    contract.require_string(model["role"], "payload.model.role", spec.identity)
    expected_kind = (
        "frozen-common-model-snapshot"
        if spec.kind == "raw"
        else "validated-native-training-store-generation-zero"
    )
    contract.require_string(model["kind"], "payload.model.kind", expected_kind)
    _exact_natural(model["generation_index"], 0, "payload.model.generation_index")
    expected_parameter = (
        contract.COMMON_SNAPSHOT_PARAMETER_STREAM_SHA256
        if spec.kind == "raw"
        else spec.model_parameter_sha256
    )
    for field in ("model_parameter_sha256", "parameter_manifest_sha256"):
        if contract.require_sha256(model[field], f"payload.model.{field}") != expected_parameter:
            contract.fail(f"payload.model.{field} mismatch")
    if spec.kind == "raw":
        _exact_natural(
            model["initialization_seed"],
            contract.COMMON_SNAPSHOT_SEED,
            "payload.model.initialization_seed",
        )
        expected_raw = {
            "snapshot_identity": "mtg-kernel-python-authoritative-common-model-snapshot-v1",
            "snapshot_manifest_file_sha256": contract.COMMON_MANIFEST_SHA256,
            "snapshot_payload_sha256": contract.COMMON_PARAMETERS_SHA256,
            "named_parameter_stream_sha256": contract.COMMON_SNAPSHOT_PARAMETER_STREAM_SHA256,
            "provenance": None,
            "prior_baseline_output_digest_identity": None,
            "prior_baseline_output_sha256": None,
        }
        for field, expected in expected_raw.items():
            if model[field] != expected:
                contract.fail(f"payload.model.{field} raw binding mismatch")
    else:
        for field in (
            "initialization_seed",
            "snapshot_identity",
            "snapshot_manifest_file_sha256",
            "snapshot_payload_sha256",
            "named_parameter_stream_sha256",
        ):
            if model[field] is not None:
                contract.fail(f"payload.model.{field} must be null for imported role")
        provenance = contract.exact_keys(
            model["provenance"], PROVENANCE_KEYS, "payload.model.provenance"
        )
        expected_provenance = dict(spec.provenance or {})
        expected_provenance.pop("generation_index", None)
        for field, expected in expected_provenance.items():
            if field in ("segment_ordinal", "adam_step"):
                _exact_natural(
                    provenance[field],
                    int(expected),
                    f"payload.model.provenance.{field}",
                )
            elif expected is None:
                if provenance[field] is not None:
                    contract.fail(f"payload.model.provenance.{field} must be null")
            else:
                contract.require_sha256(
                    provenance[field], f"payload.model.provenance.{field}"
                )
        if dict(provenance) != expected_provenance:
            contract.fail("payload.model.provenance differs from predeclared Store authority")
        if model["prior_baseline_output_digest_identity"] != contract.OUTPUT_DIGEST_IDENTITY:
            contract.fail("payload model prior baseline digest identity mismatch")
        if model["prior_baseline_output_sha256"] != spec.baseline_output_sha256:
            contract.fail("payload model prior baseline SHA-256 mismatch")

    corpus = contract.exact_keys(payload["corpus"], CORPUS_KEYS, "payload.corpus")
    for field, expected in (
        ("identity", contract.CORPUS_IDENTITY),
        ("digest_identity", "sha256-framed-thirteen-native-flat-tensors-v1"),
    ):
        contract.require_string(corpus[field], f"payload.corpus.{field}", expected)
    for field in ("sha256", "expected_sha256"):
        if (
            contract.require_sha256(corpus[field], f"payload.corpus.{field}")
            != contract.CORPUS_SHA256
        ):
            contract.fail(f"payload.corpus.{field} mismatch")
    for field, expected in (
        ("decision_count", 256),
        ("episode_count", 4),
        ("multi_action_decision_count", 256),
        ("total_action_count", 1_115),
    ):
        _exact_natural(corpus[field], expected, f"payload.corpus.{field}")

    transform = contract.exact_keys(payload["transform"], TRANSFORM_KEYS, "payload.transform")
    expected_transform = {
        "structured_repair_identity": contract.TRANSFORM_IDENTITY,
        "slot": 69,
        "effect_boolean_rule": "retain-frozen-value-bit",
        "attacker_inclusion_rule": "include-true-one-else-positive-zero",
        "blocker_inclusion_rule": "include-true-one-else-positive-zero",
        "digest_gate_identity": contract.GATE_IDENTITY,
        "scientific_gate_modes": ["FULL", "ZERO"],
        "scaled_gate_scientific_read": False,
    }
    if dict(transform) != expected_transform:
        contract.fail("payload.transform mismatch")
    _exact_natural(transform["slot"], 69, "payload.transform.slot")
    contract.require_bool(
        transform["scaled_gate_scientific_read"],
        "payload.transform.scaled_gate_scientific_read",
        False,
    )
    gate = contract.exact_keys(payload["gate"], GATE_KEYS, "payload.gate")
    if (
        any(type(value) is not bool or value is not True for key, value in gate.items() if key != "zero_stress_mapping")
        or gate["zero_stress_mapping"]
        != "within-decision-dst-j-receives-src-(j+1)-mod-n-upstream-then-ZERO"
    ):
        contract.fail("payload.gate invariant mismatch")

    admission = contract.exact_keys(payload["admission"], ADMISSION_KEYS, "payload.admission")
    boolean_fields = ADMISSION_KEYS - {
        "pre_transform_binding",
        "bitwise_comparison_identity",
        "exact_forward_capture_identity",
        "exact_forward_schema_version",
        "exact_forward_registry_version",
        "exact_forward_contract_digest",
        "exact_forward_encoding_digest",
        "exact_forward_hidden_dim",
        "exact_forward_capture_decision_count",
        "exact_forward_pooled_value_count",
        "repaired_zero_ingress_dim",
        "repaired_zero_ingress_row_count",
        "repaired_zero_ingress_sha256",
        "repaired_zero_ingress_row_digest_identity",
        "repaired_zero_ingress_row_digests",
        "attacker_false_true_pair_count",
        "blocker_false_true_pair_count",
    }
    for field in boolean_fields:
        contract.require_bool(admission[field], f"payload.admission.{field}", True)
    pre_transform = contract.exact_keys(
        admission["pre_transform_binding"],
        PRE_TRANSFORM_BINDING_KEYS,
        "payload.admission.pre_transform_binding",
    )
    contract.require_string(
        pre_transform["identity"],
        "payload.admission.pre_transform_binding.identity",
        (
            "sha256-length-framed-retained-action-semantic-operational-core-ref-"
            "object-scorer-projection-canonical-json-tail-v1"
        ),
    )
    contract.require_string(
        pre_transform["transcript_encoding"],
        "payload.admission.pre_transform_binding.transcript_encoding",
        (
            "ordered typed rows; atom=u32be(label_len)||label||u64be(value_len)||value; "
            "integer and f32-bit arrays are little-endian; JSON and digest blocks are "
            "raw bytes"
        ),
    )
    for field in PRE_TRANSFORM_BINDING_KEYS - {
        "identity",
        "transcript_encoding",
        "decision_count",
        "row_count",
        "action_reference_count",
        "operational_object_count",
        "action_object_projection_count",
        "capture_sha256",
        "revalidated_sha256",
    }:
        contract.require_bool(
            pre_transform[field],
            f"payload.admission.pre_transform_binding.{field}",
            True,
        )
    _exact_natural(
        pre_transform["decision_count"],
        256,
        "payload.admission.pre_transform_binding.decision_count",
    )
    _exact_natural(
        pre_transform["row_count"],
        1_115,
        "payload.admission.pre_transform_binding.row_count",
    )
    _exact_natural(
        pre_transform["action_reference_count"],
        contract.CORPUS_ACTION_REFERENCE_COUNT,
        "payload.admission.pre_transform_binding.action_reference_count",
    )
    operational_object_count = contract.require_natural(
        pre_transform["operational_object_count"],
        "payload.admission.pre_transform_binding.operational_object_count",
        positive=True,
    )
    action_object_projection_count = contract.require_natural(
        pre_transform["action_object_projection_count"],
        "payload.admission.pre_transform_binding.action_object_projection_count",
        positive=True,
    )
    if operational_object_count != action_object_projection_count:
        contract.fail("payload pre-transform operational-object projection count mismatch")
    capture_sha = contract.require_sha256(
        pre_transform["capture_sha256"],
        "payload.admission.pre_transform_binding.capture_sha256",
    )
    revalidated_sha = contract.require_sha256(
        pre_transform["revalidated_sha256"],
        "payload.admission.pre_transform_binding.revalidated_sha256",
    )
    if capture_sha != revalidated_sha:
        contract.fail("payload pre-transform capture/revalidation SHA mismatch")
    for field, expected in (
        ("bitwise_comparison_identity", "ieee754-f32-to_bits-exact-v1"),
        (
            "exact_forward_capture_identity",
            "native-policy-value-net8-exact-pre-action-encoder-ingress-v1",
        ),
        ("exact_forward_schema_version", "actor-relative-v5-python-4"),
        (
            "exact_forward_registry_version",
            "rust-observation-v5-action-v5-registry-4",
        ),
    ):
        contract.require_string(admission[field], f"payload.admission.{field}", expected)
    for field, expected in (
        (
            "exact_forward_contract_digest",
            "bcc808186e40a1ad6aec679d8a386631cb1226379366a632603f0beb95b47396",
        ),
        (
            "exact_forward_encoding_digest",
            "918e57a0796807e84310026de48d30b500813ef37d939462ea85b7255a39111c",
        ),
    ):
        if contract.require_sha256(admission[field], f"payload.admission.{field}") != expected:
            contract.fail(f"payload.admission.{field} mismatch")
    _exact_natural(
        admission["exact_forward_hidden_dim"],
        64,
        "payload.admission.exact_forward_hidden_dim",
    )
    _exact_natural(
        admission["exact_forward_capture_decision_count"],
        256,
        "payload.admission.exact_forward_capture_decision_count",
    )
    _exact_natural(
        admission["exact_forward_pooled_value_count"],
        1_115 * 64,
        "payload.admission.exact_forward_pooled_value_count",
    )
    _exact_natural(
        admission["repaired_zero_ingress_dim"],
        163,
        "payload.admission.repaired_zero_ingress_dim",
    )
    _exact_natural(
        admission["repaired_zero_ingress_row_count"],
        1_115,
        "payload.admission.repaired_zero_ingress_row_count",
    )
    contract.require_sha256(
        admission["repaired_zero_ingress_sha256"],
        "payload.admission.repaired_zero_ingress_sha256",
    )
    contract.require_string(
        admission["repaired_zero_ingress_row_digest_identity"],
        "payload.admission.repaired_zero_ingress_row_digest_identity",
        "sha256-f32le-163-v1",
    )
    ingress_digest_rows = _verify_ingress_digest_rows(
        admission["repaired_zero_ingress_row_digests"]
    )
    for field in ("attacker_false_true_pair_count", "blocker_false_true_pair_count"):
        contract.require_natural(admission[field], f"payload.admission.{field}", positive=True)

    statistics = contract.exact_keys(
        payload["input_statistics"], INPUT_STAT_KEYS, "payload.input_statistics"
    )
    contract.require_string(
        statistics["source_condition"], "payload.input_statistics.source_condition", "repaired/FULL"
    )
    _exact_natural(
        statistics["direct_value_count"],
        1_115 * 99,
        "payload.input_statistics.direct_value_count",
    )
    _exact_natural(
        statistics["digest_value_count"],
        1_115 * 96,
        "payload.input_statistics.digest_value_count",
    )
    for field in (
        "direct_value_rms",
        "digest_value_rms",
        "mean_direct_squared_norm",
        "mean_digest_squared_norm",
    ):
        _number(statistics[field], f"payload.input_statistics.{field}", nonnegative=True)
    stat_rows = _verify_rows(
        statistics["per_action_row"],
        keys=INPUT_ROW_KEYS,
        metric_fields=("direct_squared_norm", "digest_squared_norm"),
        where="payload.input_statistics.per_action_row",
    )
    direct_sum = sum(float(row["direct_squared_norm"]) for row in stat_rows)
    digest_sum = sum(float(row["digest_squared_norm"]) for row in stat_rows)
    exact_statistics = {
        "direct_value_rms": math.sqrt(direct_sum / (1_115 * 99)),
        "digest_value_rms": math.sqrt(digest_sum / (1_115 * 96)),
        "mean_direct_squared_norm": direct_sum / 1_115,
        "mean_digest_squared_norm": digest_sum / 1_115,
    }
    for field, expected in exact_statistics.items():
        if float(statistics[field]) != expected:
            contract.fail(f"payload.input_statistics.{field} aggregation mismatch")

    contribution = contract.exact_keys(
        payload["first_layer_contribution_rms"],
        CONTRIBUTION_KEYS,
        "payload.first_layer_contribution_rms",
    )
    expected_text = {
        "source_condition": "repaired/FULL",
        "tensor_name": "action_encoder.0.weight",
        "accumulator": "exact-positive-zero-f32-forward-column-order-bias-excluded",
        "hidden_dim": 64,
    }
    for field, expected in expected_text.items():
        if contribution[field] != expected:
            contract.fail(f"payload first-layer {field} mismatch")
    _exact_natural(
        contribution["hidden_dim"],
        64,
        "payload.first_layer_contribution_rms.hidden_dim",
    )
    for field in ("direct_contribution_rms", "digest_contribution_rms"):
        _number(contribution[field], f"payload.first_layer.{field}", nonnegative=True)
    contribution_rows = _verify_rows(
        contribution["per_action_row"],
        keys=CONTRIBUTION_ROW_KEYS,
        metric_fields=("direct_contribution_rms", "digest_contribution_rms"),
        where="payload.first_layer_contribution_rms.per_action_row",
    )
    for index, (stat, contrib) in enumerate(zip(stat_rows, contribution_rows, strict=True)):
        if (
            stat["decision_index"] != contrib["decision_index"]
            or stat["action_index"] != contrib["action_index"]
        ):
            contract.fail(f"payload per-row identity mismatch at row {index}")
    for field, row_field in (
        ("direct_contribution_rms", "direct_contribution_rms"),
        ("digest_contribution_rms", "digest_contribution_rms"),
    ):
        reconstructed = math.sqrt(
            sum(float(row[row_field]) ** 2 for row in contribution_rows) / 1_115
        )
        actual = float(contribution[field])
        if not _within_ulps(actual, reconstructed):
            contract.fail(
                f"payload first-layer {field} is inconsistent with per-row RMS values"
            )
    for index, (ingress, stat) in enumerate(
        zip(ingress_digest_rows, stat_rows, strict=True)
    ):
        if (
            ingress["decision_index"] != stat["decision_index"]
            or ingress["action_index"] != stat["action_index"]
        ):
            contract.fail(f"payload ingress/stat row identity mismatch at row {index}")

    effects = contract.exact_keys(payload["effects"], EFFECTS_KEYS, "payload.effects")
    direct = _verify_effect(
        effects["direct_sibling"],
        name="repaired_direct_sibling_rotation",
        where="payload.effects.direct_sibling",
    )
    digest = _verify_effect(
        effects["digest_sibling"],
        name="repaired_digest_sibling_rotation",
        where="payload.effects.digest_sibling",
    )
    _verify_effect(
        effects["repaired_full_vs_repaired_zero"],
        name="repaired_full_vs_repaired_zero",
        where="payload.effects.repaired_full_vs_repaired_zero",
    )
    contrast = contract.exact_keys(
        effects["digest_minus_direct"], CONTRAST_KEYS, "payload.effects.digest_minus_direct"
    )
    effect_fields = {
        "mean_jensen_shannon_nats": "mean_jensen_shannon_nats",
        "mean_centered_logit_rms_delta": "mean_centered_logit_rms_delta",
        "top_action_flip_fraction": "top_action_flip_fraction",
    }
    for contrast_field, effect_field in effect_fields.items():
        actual = _number(contrast[contrast_field], f"payload contrast {contrast_field}")
        expected = float(digest[effect_field]) - float(direct[effect_field])
        if actual != expected:
            contract.fail(f"payload contrast {contrast_field} is not digest minus direct")
    expected_label = _expected_label(spec.identity, contrast)
    contract.require_string(
        effects["descriptive_label"], "payload.effects.descriptive_label", expected_label
    )

    output = contract.exact_keys(payload["output_digests"], OUTPUT_KEYS, "payload.output_digests")
    contract.require_string(
        output["digest_identity"], "payload.output_digests.digest_identity", contract.OUTPUT_DIGEST_IDENTITY
    )
    for field in ("baseline_frozen_full", "repaired_full", "repaired_zero"):
        contract.require_sha256(output[field], f"payload.output_digests.{field}")
    for field in (
        "repeated_baseline_frozen_full_bit_exact",
        "repeated_repaired_full_bit_exact",
        "repeated_repaired_zero_bit_exact",
        "zero_stress_equals_repaired_zero",
        "repair_only_value_bits_invariant",
    ):
        contract.require_bool(output[field], f"payload.output_digests.{field}", True)
    if spec.kind == "raw":
        if (
            output["prior_baseline_reproduced_sha256"] is not None
            or output["prior_baseline_exact_match"] is not None
        ):
            contract.fail("raw role prior baseline reproduction fields must be null")
    else:
        if output["prior_baseline_reproduced_sha256"] != spec.baseline_output_sha256:
            contract.fail("imported prior baseline reproduced SHA mismatch")
        contract.require_bool(
            output["prior_baseline_exact_match"],
            "payload.output_digests.prior_baseline_exact_match",
            True,
        )
    nonclaims = contract.require_array(payload["nonclaims"], "payload.nonclaims")
    if not nonclaims or any(type(value) is not str or not value for value in nonclaims):
        contract.fail("payload.nonclaims must contain nonempty strings")
    return {
        "corpus_sha256": corpus["sha256"],
        "descriptive_label": expected_label,
        "model_parameter_sha256": model["model_parameter_sha256"],
        "repaired_zero_ingress_sha256": admission["repaired_zero_ingress_sha256"],
        "repaired_zero_ingress_row_digest_count": len(ingress_digest_rows),
        "semantic_binding_capture_sha256": capture_sha,
    }


def _capture(
    command: list[str],
    *,
    repo: Path,
    environment: Mapping[str, str],
    timeout_seconds: int,
) -> ProcessCapture:
    started = time.perf_counter()
    try:
        process = subprocess.run(
            command,
            cwd=repo,
            env=dict(environment),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=timeout_seconds,
        )
        return ProcessCapture(
            process.returncode,
            process.stdout,
            process.stderr,
            False,
            round((time.perf_counter() - started) * 1000),
        )
    except subprocess.TimeoutExpired as error:
        return ProcessCapture(
            None,
            error.stdout or b"",
            error.stderr or b"",
            True,
            round((time.perf_counter() - started) * 1000),
        )
    except OSError as error:
        raise contract.AdmissionError(f"could not launch probe: {error}") from error


def _is_reparse_point(path: Path) -> bool:
    try:
        attributes = getattr(os.lstat(path), "st_file_attributes", 0)
    except OSError as error:
        raise contract.AdmissionError(
            f"could not inspect Store entry {path}: {error}"
        ) from error
    reparse_flag = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0)
    is_junction = getattr(path, "is_junction", None)
    return bool(
        path.is_symlink()
        or (callable(is_junction) and is_junction())
        or (reparse_flag and attributes & reparse_flag)
    )


def _store_snapshot(root: Path) -> dict[str, Any]:
    if not root.is_dir() or _is_reparse_point(root):
        contract.fail(f"Store root must be an existing non-reparse directory: {root}")
    root = root.resolve()
    rows: list[dict[str, Any]] = []
    for directory, dirnames, filenames in os.walk(root, followlinks=False):
        directory_path = Path(directory)
        dirnames.sort()
        filenames.sort()
        for name in dirnames:
            path = directory_path / name
            if _is_reparse_point(path) or not path.is_dir():
                contract.fail(f"Store tree contains a non-regular directory: {path}")
            rows.append(
                {
                    "kind": "directory",
                    "path": path.relative_to(root).as_posix(),
                }
            )
        for name in filenames:
            path = directory_path / name
            if _is_reparse_point(path) or not path.is_file():
                contract.fail(f"Store tree contains a non-regular file: {path}")
            rows.append(
                {
                    "kind": "file",
                    **contract.file_record(path, root=root),
                }
            )
    rows.sort(key=lambda row: (row["path"], row["kind"]))
    files = [row for row in rows if row["kind"] == "file"]
    return {
        "aggregate_sha256": hashlib.sha256(contract.canonical_json_bytes(rows)).hexdigest(),
        "directory_count": len(rows) - len(files),
        "file_count": len(files),
        "total_byte_count": sum(row["byte_count"] for row in files),
    }


def verify_store_snapshot_record(
    value: Any,
    *,
    where: str,
    expected: Mapping[str, Any] | None = None,
) -> Mapping[str, Any]:
    record = contract.exact_keys(
        value,
        {
            "aggregate_sha256",
            "directory_count",
            "file_count",
            "total_byte_count",
        },
        where,
    )
    contract.require_sha256(record["aggregate_sha256"], f"{where}.aggregate_sha256")
    for field in ("directory_count", "file_count", "total_byte_count"):
        contract.require_natural(record[field], f"{where}.{field}")
    if expected is not None and dict(record) != dict(expected):
        contract.fail(f"{where} differs from the current Store tree")
    return record


def _model_environment(spec: contract.ModelSpec) -> dict[str, str]:
    result = {
        "ACTION_INGRESS_MODEL_ROLE": spec.identity,
        "CUDA_VISIBLE_DEVICES": "",
        "MTG_KERNEL_DEVICE": "cpu",
    }
    if spec.store_root is not None:
        result["ACTION_INGRESS_STORE_ROOT"] = spec.store_root
    return result


def _inventory(run_root: Path, artifact_root: Path) -> list[dict[str, Any]]:
    records = [
        contract.file_record(path, root=artifact_root)
        for path in run_root.rglob("*")
        if path.is_file()
    ]
    records.sort(key=lambda record: record["path"])
    return records


def _verify_generated_run_tree(run_root: Path) -> None:
    if not run_root.is_dir() or _is_reparse_point(run_root):
        contract.fail("runs root must be a non-reparse directory")
    entries = list(run_root.iterdir())
    expected_directories = {spec.run_name for spec in contract.MODEL_SPECS}
    if (
        {entry.name for entry in entries} != expected_directories
        or any(not entry.is_dir() or _is_reparse_point(entry) for entry in entries)
    ):
        contract.fail("runs root must contain exactly the three model directories")
    expected_files = {
        "invocation-receipt.json",
        "probe-envelope.json",
        "probe-payload.json",
        "probe.stderr.log",
        "probe.stdout.log",
    }
    for spec in contract.MODEL_SPECS:
        directory = run_root / spec.run_name
        children = list(directory.iterdir())
        if (
            {child.name for child in children} != expected_files
            or any(not child.is_file() or _is_reparse_point(child) for child in children)
        ):
            contract.fail(f"{spec.identity} run directory has an unexpected entry")


def run(
    *,
    repo: Path,
    artifact_root: Path,
    target_dir: Path,
    build_receipt_path: Path,
    require_windows: bool = True,
) -> dict[str, Any]:
    if require_windows and os.name != "nt":
        contract.fail("official admission execution must run with Windows Python")
    repo = repo.resolve()
    artifact_root = artifact_root.resolve()
    target_dir = target_dir.resolve()
    build_receipt_path = build_receipt_path.resolve()
    expected_build = artifact_root / "build" / "build-receipt.json"
    if not contract.same_path(build_receipt_path, expected_build):
        contract.fail("build receipt path mismatch")
    build_receipt, executable, head, branch = verify_build_receipt(
        build_receipt_path,
        repo=repo,
        artifact_root=artifact_root,
        target_dir=target_dir,
        require_windows=require_windows,
    )
    if (
        {entry.name for entry in artifact_root.iterdir()} != {"build"}
        or not (artifact_root / "build").is_dir()
        or _is_reparse_point(artifact_root / "build")
    ):
        contract.fail("artifact root must contain only the completed build before execution")
    run_root = artifact_root / "runs"
    if run_root.exists():
        contract.fail(f"run output root must be absent: {run_root}")
    run_root.mkdir(exist_ok=False)
    started_utc = _utc_now()
    started = time.perf_counter()
    invocations: list[dict[str, Any]] = []

    store_before = {
        spec.identity: _store_snapshot(Path(spec.store_root))
        for spec in contract.MODEL_SPECS
        if spec.store_root is not None
    }
    base_environment = os.environ.copy()
    for spec in contract.MODEL_SPECS:
        invocation_root = run_root / spec.run_name
        invocation_root.mkdir(exist_ok=False)
        contract_environment = _model_environment(spec)
        environment = base_environment.copy()
        for key in ("ACTION_INGRESS_MODEL_ROLE", "ACTION_INGRESS_STORE_ROOT"):
            environment.pop(key, None)
        environment.update(contract_environment)
        command = contract.probe_command(executable)
        invocation_started = _utc_now()
        capture = _capture(
            command,
            repo=repo,
            environment=environment,
            timeout_seconds=contract.MODEL_TIMEOUT_SECONDS,
        )
        stdout_path = invocation_root / "probe.stdout.log"
        stderr_path = invocation_root / "probe.stderr.log"
        contract.write_exclusive(stdout_path, capture.stdout)
        contract.write_exclusive(stderr_path, capture.stderr)
        if capture.timed_out:
            contract.fail(f"{spec.identity} exceeded exactly 120 seconds")
        if capture.exit_code != 0:
            contract.fail(f"{spec.identity} exited {capture.exit_code}")
        parsed = parse_probe_output(capture.stdout, capture.stderr)
        bindings = verify_probe_payload(parsed.payload, spec)
        envelope_path = invocation_root / "probe-envelope.json"
        payload_path = invocation_root / "probe-payload.json"
        contract.write_exclusive(envelope_path, parsed.envelope_raw + b"\n")
        contract.write_exclusive(payload_path, parsed.payload_raw + b"\n")
        receipt: dict[str, Any] = {
            "build_receipt": {
                "path": str(build_receipt_path),
                "payload_sha256": build_receipt["payload_sha256"],
                "sha256": contract.sha256_file(build_receipt_path),
            },
            "command": command,
            "completed_utc": _utc_now(),
            "contract_environment": contract_environment,
            "executable": {
                "path": str(executable),
                "sha256": contract.sha256_file(executable),
            },
            "exit_code": capture.exit_code,
            "git_head": head,
            "git_branch": branch,
            "label": contract.LABEL,
            "model": {
                "identity": spec.identity,
                "kind": spec.kind,
                "ordinal": spec.ordinal,
            },
            "probe": {
                "bindings": bindings,
                "envelope": contract.file_record(envelope_path),
                "envelope_payload_sha256": parsed.envelope["payload_sha256"],
                "marker_count": 1,
                "payload": contract.file_record(payload_path),
            },
            "schema": contract.INVOCATION_RECEIPT_SCHEMA,
            "started_utc": invocation_started,
            "status": "VALID",
            "stderr": contract.file_record(stderr_path),
            "stdout": contract.file_record(stdout_path),
            "timed_out": False,
            "timeout_seconds": contract.MODEL_TIMEOUT_SECONDS,
            "wall_time_ms": capture.wall_time_ms,
        }
        contract.attach_payload_sha256(receipt)
        receipt_path = invocation_root / "invocation-receipt.json"
        contract.write_exclusive(receipt_path, contract.record_bytes(receipt))
        invocations.append(
            {
                "identity": spec.identity,
                "invocation_receipt": contract.file_record(receipt_path),
                "ordinal": spec.ordinal,
                "payload_sha256": parsed.envelope["payload_sha256"],
                "wall_time_ms": capture.wall_time_ms,
            }
        )

    store_after = {
        spec.identity: _store_snapshot(Path(spec.store_root))
        for spec in contract.MODEL_SPECS
        if spec.store_root is not None
    }
    if store_after != store_before:
        contract.fail("an imported Store changed during the read-only screen")
    after_head = contract.require_clean_worktree(repo)
    if contract.require_frozen_branch(repo) != branch:
        contract.fail("git branch changed during execution")
    if after_head != head:
        contract.fail(f"source HEAD changed during execution: before={head} after={after_head}")
    inventory = _inventory(run_root, artifact_root)
    if len(inventory) != len(contract.MODEL_SPECS) * 5:
        contract.fail("run inventory must contain exactly five files per model")
    _verify_generated_run_tree(run_root)
    completion: dict[str, Any] = {
        "build_receipt": {
            "path": str(build_receipt_path),
            "payload_sha256": build_receipt["payload_sha256"],
            "sha256": contract.sha256_file(build_receipt_path),
        },
        "completed_utc": _utc_now(),
        "cpu_only": True,
        "elapsed_ms": round((time.perf_counter() - started) * 1000),
        "executable": {
            "path": str(executable),
            "sha256": contract.sha256_file(executable),
        },
        "git_head": head,
        "git_branch": branch,
        "git_status_clean_before_and_after": True,
        "invocation_count": 3,
        "invocations": invocations,
        "label": contract.LABEL,
        "manifest": {
            "path": str((repo / contract.MANIFEST_RELATIVE_PATH).resolve()),
            "sha256": contract.sha256_file(repo / contract.MANIFEST_RELATIVE_PATH),
        },
        "model_order": [spec.identity for spec in contract.MODEL_SPECS],
        "output_inventory": inventory,
        "schema": contract.COMPLETION_RECEIPT_SCHEMA,
        "sequential_execution": True,
        "started_utc": started_utc,
        "status": "VALID",
        "store_snapshots_after": store_after,
        "store_snapshots_before": store_before,
        "timeout_seconds_per_model": contract.MODEL_TIMEOUT_SECONDS,
    }
    contract.attach_payload_sha256(completion)
    completion_path = artifact_root / "completion-receipt.json"
    contract.write_exclusive(completion_path, contract.record_bytes(completion))
    print(json.dumps(completion, sort_keys=True, separators=(",", ":")))
    return completion


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument(
        "--artifact-root", type=Path, default=Path(contract.ARTIFACT_ROOT_WINDOWS)
    )
    parser.add_argument(
        "--target-dir", type=Path, default=Path(contract.TARGET_DIR_WINDOWS)
    )
    parser.add_argument("--build-receipt", type=Path)
    args = parser.parse_args()
    build_receipt = args.build_receipt or (
        args.artifact_root / "build" / "build-receipt.json"
    )
    run(
        repo=args.repo,
        artifact_root=args.artifact_root,
        target_dir=args.target_dir,
        build_receipt_path=build_receipt,
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"ACTION_INGRESS_SCREEN_INVALID: {error}", file=sys.stderr)
        raise
