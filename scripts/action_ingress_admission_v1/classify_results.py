#!/usr/bin/env python3
"""Validate a complete admission screen and emit three separate labels."""

from __future__ import annotations

import argparse
import json
from pathlib import Path, PurePosixPath
import sys
from typing import Any, Mapping

try:
    from scripts.action_ingress_admission_v1 import contract, run_probe
except ModuleNotFoundError:  # Direct execution from this directory.
    import contract  # type: ignore[no-redef]
    import run_probe  # type: ignore[no-redef]


COMPLETION_KEYS = {
    "build_receipt",
    "completed_utc",
    "cpu_only",
    "elapsed_ms",
    "executable",
    "git_head",
    "git_branch",
    "git_status_clean_before_and_after",
    "invocation_count",
    "invocations",
    "label",
    "manifest",
    "model_order",
    "output_inventory",
    "payload_sha256",
    "schema",
    "sequential_execution",
    "started_utc",
    "status",
    "store_snapshots_after",
    "store_snapshots_before",
    "timeout_seconds_per_model",
}
INVOCATION_KEYS = {
    "build_receipt",
    "command",
    "completed_utc",
    "contract_environment",
    "executable",
    "exit_code",
    "git_head",
    "git_branch",
    "label",
    "model",
    "payload_sha256",
    "probe",
    "schema",
    "started_utc",
    "status",
    "stderr",
    "stdout",
    "timed_out",
    "timeout_seconds",
    "wall_time_ms",
}
PROBE_BINDING_KEYS = {
    "corpus_sha256",
    "descriptive_label",
    "model_parameter_sha256",
    "repaired_zero_ingress_sha256",
    "repaired_zero_ingress_row_digest_count",
    "semantic_binding_capture_sha256",
}


def _expected_run_files(artifact_root: Path) -> list[Path]:
    names = (
        "invocation-receipt.json",
        "probe-envelope.json",
        "probe-payload.json",
        "probe.stderr.log",
        "probe.stdout.log",
    )
    return [
        artifact_root / "runs" / spec.run_name / name
        for spec in contract.MODEL_SPECS
        for name in names
    ]


def _validate_inventory(
    value: Any,
    *,
    artifact_root: Path,
) -> list[dict[str, Any]]:
    inventory = contract.require_array(value, "completion.output_inventory", 15)
    expected_paths = {
        path.resolve().relative_to(artifact_root.resolve()).as_posix()
        for path in _expected_run_files(artifact_root)
    }
    seen: set[str] = set()
    verified: list[dict[str, Any]] = []
    for index, raw in enumerate(inventory):
        where = f"completion.output_inventory[{index}]"
        record = contract.exact_keys(raw, {"byte_count", "path", "sha256"}, where)
        relative = contract.require_string(record["path"], f"{where}.path")
        pure = PurePosixPath(relative)
        if (
            pure.is_absolute()
            or str(pure) != relative
            or any(part in ("", ".", "..") for part in pure.parts)
            or "\\" in relative
            or ":" in relative
            or relative in seen
        ):
            contract.fail(f"{where}.path is not a unique normalized relative POSIX path")
        seen.add(relative)
        unresolved_path = artifact_root.joinpath(*pure.parts)
        if run_probe._is_reparse_point(unresolved_path):
            contract.fail(f"{where}.path must not be a reparse point")
        path = unresolved_path.resolve()
        contract.require_descendant_path(path, artifact_root, f"{where}.path")
        actual = contract.file_record(path, root=artifact_root)
        run_probe._exact_natural(
            record["byte_count"],
            actual["byte_count"],
            f"{where}.byte_count",
        )
        contract.require_sha256(record["sha256"], f"{where}.sha256")
        if dict(record) != actual:
            contract.fail(f"{where} differs from current file bytes")
        verified.append(actual)
    if seen != expected_paths:
        contract.fail(
            "completion inventory file-set mismatch: "
            f"missing={sorted(expected_paths - seen)} extra={sorted(seen - expected_paths)}"
        )
    verified.sort(key=lambda record: record["path"])
    if inventory != verified:
        contract.fail("completion inventory must be sorted lexicographically")
    for spec in contract.MODEL_SPECS:
        directory = artifact_root / "runs" / spec.run_name
        if run_probe._is_reparse_point(directory):
            contract.fail(f"{spec.identity} output directory must not be a reparse point")
        actual_names = {
            child.name for child in directory.iterdir() if child.is_file()
        }
        expected_names = {
            path.name
            for path in _expected_run_files(artifact_root)
            if path.parent == directory
        }
        if actual_names != expected_names or any(child.is_dir() for child in directory.iterdir()):
            contract.fail(f"{spec.identity} output directory has an unexpected entry")
    run_root = artifact_root / "runs"
    if run_probe._is_reparse_point(run_root):
        contract.fail("runs root must not be a reparse point")
    run_entries = list(run_root.iterdir())
    if (
        {entry.name for entry in run_entries}
        != {spec.run_name for spec in contract.MODEL_SPECS}
        or any(
            not entry.is_dir() or run_probe._is_reparse_point(entry)
            for entry in run_entries
        )
    ):
        contract.fail("runs root has an unexpected or missing model directory")
    run_probe._verify_generated_run_tree(run_root)
    return verified


def _binding_matches_file(
    value: Any,
    *,
    expected_path: Path,
    where: str,
) -> None:
    run_probe._verify_file_binding(
        value,
        expected_path=expected_path,
        where=where,
    )


def _verify_probe_bindings(
    value: Any,
    *,
    expected: Mapping[str, Any],
    where: str,
) -> Mapping[str, Any]:
    bindings = contract.exact_keys(value, PROBE_BINDING_KEYS, where)
    for field in (
        "corpus_sha256",
        "model_parameter_sha256",
        "repaired_zero_ingress_sha256",
        "semantic_binding_capture_sha256",
    ):
        contract.require_sha256(bindings[field], f"{where}.{field}")
    contract.require_string(bindings["descriptive_label"], f"{where}.descriptive_label")
    run_probe._exact_natural(
        bindings["repaired_zero_ingress_row_digest_count"],
        1_115,
        f"{where}.repaired_zero_ingress_row_digest_count",
    )
    if dict(bindings) != dict(expected):
        contract.fail(f"{where} differs from recomputed probe bindings")
    return bindings


def _validate_invocation(
    *,
    spec: contract.ModelSpec,
    invocation_summary: Mapping[str, Any],
    artifact_root: Path,
    build_receipt_path: Path,
    build_receipt: Mapping[str, Any],
    executable: Path,
    head: str,
) -> tuple[dict[str, Any], dict[str, Any]]:
    root = artifact_root / "runs" / spec.run_name
    receipt_path = root / "invocation-receipt.json"
    receipt = contract.read_canonical_record(
        receipt_path, f"{spec.identity} invocation receipt"
    )
    contract.exact_keys(receipt, INVOCATION_KEYS, f"{spec.identity} invocation receipt")
    contract.require_string(
        receipt["schema"],
        f"{spec.identity} invocation schema",
        contract.INVOCATION_RECEIPT_SCHEMA,
    )
    contract.require_string(receipt["label"], f"{spec.identity} label", contract.LABEL)
    contract.require_string(receipt["status"], f"{spec.identity} status", "VALID")
    if receipt["git_head"] != head:
        contract.fail(f"{spec.identity} git head mismatch")
    if receipt["git_branch"] != contract.BRANCH:
        contract.fail(f"{spec.identity} git branch mismatch")
    if receipt["command"] != contract.probe_command(executable):
        contract.fail(f"{spec.identity} command mismatch")
    if receipt["contract_environment"] != run_probe._model_environment(spec):
        contract.fail(f"{spec.identity} environment mismatch")
    if receipt["timed_out"] is not False:
        contract.fail(f"{spec.identity} process did not complete within exact contract")
    run_probe._exact_natural(receipt["exit_code"], 0, f"{spec.identity}.exit_code")
    run_probe._exact_natural(
        receipt["timeout_seconds"], 120, f"{spec.identity}.timeout_seconds"
    )
    contract.require_natural(receipt["wall_time_ms"], f"{spec.identity}.wall_time_ms")
    expected_model = {
        "identity": spec.identity,
        "kind": spec.kind,
        "ordinal": spec.ordinal,
    }
    if receipt["model"] != expected_model:
        contract.fail(f"{spec.identity} receipt model binding mismatch")
    run_probe._exact_natural(
        receipt["model"]["ordinal"], spec.ordinal, f"{spec.identity}.model.ordinal"
    )

    build_binding = contract.exact_keys(
        receipt["build_receipt"],
        {"path", "payload_sha256", "sha256"},
        f"{spec.identity}.build_receipt",
    )
    if (
        not contract.same_path(build_binding["path"], build_receipt_path)
        or build_binding["sha256"] != contract.sha256_file(build_receipt_path)
        or build_binding["payload_sha256"] != build_receipt["payload_sha256"]
    ):
        contract.fail(f"{spec.identity} build-receipt binding mismatch")
    executable_binding = contract.exact_keys(
        receipt["executable"], {"path", "sha256"}, f"{spec.identity}.executable"
    )
    if (
        not contract.same_path(executable_binding["path"], executable)
        or executable_binding["sha256"] != contract.sha256_file(executable)
    ):
        contract.fail(f"{spec.identity} executable binding mismatch")

    stdout_path = root / "probe.stdout.log"
    stderr_path = root / "probe.stderr.log"
    envelope_path = root / "probe-envelope.json"
    payload_path = root / "probe-payload.json"
    _binding_matches_file(
        receipt["stdout"], expected_path=stdout_path, where=f"{spec.identity}.stdout"
    )
    _binding_matches_file(
        receipt["stderr"], expected_path=stderr_path, where=f"{spec.identity}.stderr"
    )
    parsed = run_probe.parse_probe_output(
        stdout_path.read_bytes(), stderr_path.read_bytes()
    )
    if envelope_path.read_bytes() != parsed.envelope_raw + b"\n":
        contract.fail(f"{spec.identity} saved envelope differs from stdout")
    if payload_path.read_bytes() != parsed.payload_raw + b"\n":
        contract.fail(f"{spec.identity} saved payload differs from stdout")
    probe = contract.exact_keys(
        receipt["probe"],
        {
            "bindings",
            "envelope",
            "envelope_payload_sha256",
            "marker_count",
            "payload",
        },
        f"{spec.identity}.probe",
    )
    _binding_matches_file(
        probe["envelope"],
        expected_path=envelope_path,
        where=f"{spec.identity}.probe.envelope",
    )
    _binding_matches_file(
        probe["payload"],
        expected_path=payload_path,
        where=f"{spec.identity}.probe.payload",
    )
    run_probe._exact_natural(
        probe["marker_count"], 1, f"{spec.identity}.probe.marker_count"
    )
    if probe["envelope_payload_sha256"] != parsed.envelope["payload_sha256"]:
        contract.fail(f"{spec.identity} envelope payload hash binding mismatch")
    bindings = run_probe.verify_probe_payload(parsed.payload, spec)
    _verify_probe_bindings(
        probe["bindings"],
        expected=bindings,
        where=f"{spec.identity}.probe.bindings",
    )

    summary = contract.exact_keys(
        invocation_summary,
        {
            "identity",
            "invocation_receipt",
            "ordinal",
            "payload_sha256",
            "wall_time_ms",
        },
        f"{spec.identity} completion invocation summary",
    )
    run_probe._exact_natural(
        summary["ordinal"], spec.ordinal, f"{spec.identity}.summary.ordinal"
    )
    contract.require_natural(
        summary["wall_time_ms"], f"{spec.identity}.summary.wall_time_ms"
    )
    contract.require_sha256(
        summary["payload_sha256"], f"{spec.identity}.summary.payload_sha256"
    )
    _binding_matches_file(
        summary["invocation_receipt"],
        expected_path=receipt_path,
        where=f"{spec.identity}.summary.invocation_receipt",
    )
    expected_summary = {
        "identity": spec.identity,
        "invocation_receipt": contract.file_record(receipt_path),
        "ordinal": spec.ordinal,
        "payload_sha256": parsed.envelope["payload_sha256"],
        "wall_time_ms": receipt["wall_time_ms"],
    }
    if dict(summary) != expected_summary:
        contract.fail(f"{spec.identity} completion invocation summary mismatch")
    return parsed.payload, bindings


def _verify_cross_model_invariants(
    payloads: list[tuple[contract.ModelSpec, dict[str, Any]]],
) -> None:
    if len(payloads) != len(contract.MODEL_SPECS):
        contract.fail("cross-model validation requires exactly three payloads")
    if len({payload["corpus"]["sha256"] for _, payload in payloads}) != 1:
        contract.fail("cross-model corpus binding mismatch")
    corpus_only_paths = (
        ("corpus",),
        ("transform",),
        ("gate",),
        ("admission", "pre_transform_binding"),
        ("input_statistics",),
    )
    first_payload = payloads[0][1]
    for path in corpus_only_paths:
        expected: Any = first_payload
        for component in path:
            expected = expected[component]
        for spec, payload in payloads[1:]:
            actual: Any = payload
            for component in path:
                actual = actual[component]
            if actual != expected:
                contract.fail(
                    f"cross-model corpus-only block differs for {spec.identity}: "
                    f"{'.'.join(path)}"
                )
    first_admission = first_payload["admission"]
    for spec, payload in payloads[1:]:
        admission = payload["admission"]
        for field in (
            "attacker_false_true_pair_count",
            "blocker_false_true_pair_count",
        ):
            if admission[field] != first_admission[field]:
                contract.fail(
                    f"cross-model corpus-only pair count differs for "
                    f"{spec.identity}: {field}"
                )
    parameter_hashes = [
        payload["model"]["model_parameter_sha256"] for _, payload in payloads
    ]
    if len(set(parameter_hashes)) != 3:
        contract.fail("three parameter authorities must be pairwise distinct")


def validate_completion(
    *,
    repo: Path,
    artifact_root: Path,
    target_dir: Path,
    completion_path: Path,
    require_windows: bool = True,
    allow_populated_classification: bool = False,
) -> tuple[dict[str, Any], list[tuple[contract.ModelSpec, dict[str, Any]]]]:
    if not completion_path.is_file() or run_probe._is_reparse_point(completion_path):
        contract.fail("completion receipt must be a non-reparse regular file")
    repo = repo.resolve()
    artifact_root = artifact_root.resolve()
    target_dir = target_dir.resolve()
    completion_path = completion_path.resolve()
    if not contract.same_path(completion_path, artifact_root / "completion-receipt.json"):
        contract.fail("completion receipt must occupy the frozen artifact-root path")
    completion = contract.read_canonical_record(completion_path, "completion receipt")
    contract.exact_keys(completion, COMPLETION_KEYS, "completion receipt")
    contract.require_string(
        completion["schema"], "completion.schema", contract.COMPLETION_RECEIPT_SCHEMA
    )
    root_entries = list(artifact_root.iterdir())
    allowed_names = {"build", "runs", "completion-receipt.json", "classification"}
    if not {entry.name for entry in root_entries}.issubset(allowed_names):
        contract.fail("artifact root contains an unexpected entry")
    required_names = {"build", "runs", "completion-receipt.json"}
    if not required_names.issubset({entry.name for entry in root_entries}):
        contract.fail("artifact root is missing a required execution entry")
    classification_root = artifact_root / "classification"
    if classification_root.exists():
        if (
            not classification_root.is_dir()
            or run_probe._is_reparse_point(classification_root)
        ):
            contract.fail("classification root must be a non-reparse directory")
        names = {entry.name for entry in classification_root.iterdir()}
        if allow_populated_classification:
            if names != {
                "classification.json",
                "classifier.stderr.log",
                "classifier.stdout.log",
            }:
                contract.fail("classification root has an unexpected postflight entry")
        elif names:
            contract.fail("classification root must be empty while validating execution")
    contract.require_string(completion["label"], "completion.label", contract.LABEL)
    contract.require_string(completion["status"], "completion.status", "VALID")
    for field in ("cpu_only", "git_status_clean_before_and_after", "sequential_execution"):
        contract.require_bool(completion[field], f"completion.{field}", True)
    run_probe._exact_natural(
        completion["invocation_count"], 3, "completion.invocation_count"
    )
    run_probe._exact_natural(
        completion["timeout_seconds_per_model"],
        120,
        "completion.timeout_seconds_per_model",
    )
    if completion["model_order"] != [spec.identity for spec in contract.MODEL_SPECS]:
        contract.fail("completion model order mismatch")
    contract.require_natural(completion["elapsed_ms"], "completion.elapsed_ms")
    head = contract.require_clean_worktree(repo)
    branch = contract.require_frozen_branch(repo)
    if completion["git_head"] != head:
        contract.fail("completion git head differs from current clean HEAD")
    if completion["git_branch"] != branch:
        contract.fail("completion git branch mismatch")
    manifest = repo / contract.MANIFEST_RELATIVE_PATH
    manifest_binding = contract.exact_keys(
        completion["manifest"], {"path", "sha256"}, "completion.manifest"
    )
    if (
        not contract.same_path(manifest_binding["path"], manifest)
        or manifest_binding["sha256"] != contract.sha256_file(manifest)
    ):
        contract.fail("completion manifest binding mismatch")

    build_receipt_path = artifact_root / "build" / "build-receipt.json"
    build_receipt, executable, build_head, build_branch = run_probe.verify_build_receipt(
        build_receipt_path,
        repo=repo,
        artifact_root=artifact_root,
        target_dir=target_dir,
        require_windows=require_windows,
    )
    if build_head != head:
        contract.fail("build and completion source commits differ")
    if build_branch != branch:
        contract.fail("build and completion source branches differ")
    build_binding = contract.exact_keys(
        completion["build_receipt"],
        {"path", "payload_sha256", "sha256"},
        "completion.build_receipt",
    )
    if (
        not contract.same_path(build_binding["path"], build_receipt_path)
        or build_binding["payload_sha256"] != build_receipt["payload_sha256"]
        or build_binding["sha256"] != contract.sha256_file(build_receipt_path)
    ):
        contract.fail("completion build-receipt binding mismatch")
    executable_binding = contract.exact_keys(
        completion["executable"], {"path", "sha256"}, "completion.executable"
    )
    if (
        not contract.same_path(executable_binding["path"], executable)
        or executable_binding["sha256"] != contract.sha256_file(executable)
    ):
        contract.fail("completion executable binding mismatch")
    _validate_inventory(completion["output_inventory"], artifact_root=artifact_root)
    expected_store_keys = {
        spec.identity for spec in contract.MODEL_SPECS if spec.store_root is not None
    }
    before = contract.exact_keys(
        completion["store_snapshots_before"],
        expected_store_keys,
        "completion.store_snapshots_before",
    )
    after = contract.exact_keys(
        completion["store_snapshots_after"],
        expected_store_keys,
        "completion.store_snapshots_after",
    )
    for spec in contract.MODEL_SPECS:
        if spec.store_root is not None:
            current = run_probe._store_snapshot(Path(spec.store_root))
            before_record = run_probe.verify_store_snapshot_record(
                before[spec.identity],
                where=f"completion.store_snapshots_before.{spec.identity}",
            )
            after_record = run_probe.verify_store_snapshot_record(
                after[spec.identity],
                where=f"completion.store_snapshots_after.{spec.identity}",
                expected=current,
            )
            if dict(before_record) != dict(after_record):
                contract.fail(f"{spec.identity} Store changed during execution")

    summaries = contract.require_array(completion["invocations"], "completion.invocations", 3)
    payloads: list[tuple[contract.ModelSpec, dict[str, Any]]] = []
    for spec, summary in zip(contract.MODEL_SPECS, summaries, strict=True):
        if not isinstance(summary, Mapping):
            contract.fail(f"completion invocation {spec.identity} must be an object")
        payload, _ = _validate_invocation(
            spec=spec,
            invocation_summary=summary,
            artifact_root=artifact_root,
            build_receipt_path=build_receipt_path,
            build_receipt=build_receipt,
            executable=executable,
            head=head,
        )
        payloads.append((spec, payload))
    _verify_cross_model_invariants(payloads)
    return completion, payloads


def classify(
    *,
    repo: Path,
    artifact_root: Path,
    target_dir: Path,
    completion_path: Path,
    output_path: Path,
    require_windows: bool = True,
) -> dict[str, Any]:
    completion, payloads = validate_completion(
        repo=repo,
        artifact_root=artifact_root,
        target_dir=target_dir,
        completion_path=completion_path,
        require_windows=require_windows,
    )
    results: list[dict[str, Any]] = []
    for spec, payload in payloads:
        contrast = payload["effects"]["digest_minus_direct"]
        label = run_probe._expected_label(spec.identity, contrast)
        if payload["effects"]["descriptive_label"] != label:
            contract.fail(f"{spec.identity} producer/classifier label mismatch")
        results.append(
            {
                "authority_kind": spec.kind,
                "classification": label,
                "digest_minus_direct": {
                    field: contrast[field] for field in sorted(run_probe.CONTRAST_KEYS)
                },
                "identity": spec.identity,
                "ordinal": spec.ordinal,
            }
        )
    report: dict[str, Any] = {
        "admission_status": "STATIC-AND-RUNTIME-ADMITTED",
        "completion_receipt": {
            "path": str(completion_path.resolve()),
            "payload_sha256": completion["payload_sha256"],
            "sha256": contract.sha256_file(completion_path),
        },
        "model_results": results,
        "no_global_label": True,
        "nonclaims": [
            "The three metrics are correlated views of the same logits.",
            "The screen does not establish digest usefulness, harm, or causal attribution.",
            "No model promotion, training authorization, or pro-level-play claim follows.",
        ],
        "schema": contract.CLASSIFICATION_SCHEMA,
        "status": "VALID",
    }
    contract.attach_payload_sha256(report)
    contract.write_exclusive(output_path, contract.record_bytes(report))
    sys.stdout.buffer.write(contract.record_bytes(report))
    return report


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument(
        "--artifact-root", type=Path, default=Path(contract.ARTIFACT_ROOT_WINDOWS)
    )
    parser.add_argument(
        "--target-dir", type=Path, default=Path(contract.TARGET_DIR_WINDOWS)
    )
    parser.add_argument("--completion", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    classify(
        repo=args.repo,
        artifact_root=args.artifact_root,
        target_dir=args.target_dir,
        completion_path=args.completion
        or args.artifact_root / "completion-receipt.json",
        output_path=args.output,
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"ACTION_INGRESS_CLASSIFICATION_INVALID: {error}", file=sys.stderr)
        raise
