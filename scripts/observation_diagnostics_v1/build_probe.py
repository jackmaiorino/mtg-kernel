#!/usr/bin/env python3
"""Build and bind the CPU-only observation-reliance lib-test executable."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from typing import Any

try:
    from scripts.observation_diagnostics_v1 import contract
except ModuleNotFoundError:  # Direct execution from this directory.
    import contract  # type: ignore[no-redef]


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _parse_cargo_messages(raw_lines: list[bytes]) -> list[dict[str, Any]]:
    messages: list[dict[str, Any]] = []
    for index, raw in enumerate(raw_lines):
        if not raw.strip():
            continue
        message = contract.parse_json_bytes(raw, f"cargo stdout line {index + 1}")
        messages.append(message)
    if not messages:
        contract.fail("cargo emitted no JSON messages")
    return messages


def _run_recorded_command(
    command: list[str],
    *,
    repo: Path,
    stdout_path: Path,
    stderr_path: Path,
    timeout_seconds: float,
) -> tuple[dict[str, Any], bytes, bytes]:
    started = time.perf_counter()
    try:
        process = subprocess.run(
            command,
            cwd=repo,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_seconds,
        )
    except subprocess.TimeoutExpired as error:
        stdout = error.stdout or b""
        stderr = error.stderr or b""
        contract.write_exclusive(stdout_path, stdout)
        contract.write_exclusive(stderr_path, stderr)
        contract.fail(
            f"command exceeded {timeout_seconds:g}s: {' '.join(command)}"
        )
    except OSError as error:
        raise contract.DiagnosticError(
            f"could not run {' '.join(command)}: {error}"
        ) from error
    wall_time_ms = round((time.perf_counter() - started) * 1000)
    contract.write_exclusive(stdout_path, process.stdout)
    contract.write_exclusive(stderr_path, process.stderr)
    record = {
        "command": command,
        "exit_code": process.returncode,
        "stderr": {
            "path": str(stderr_path),
            "sha256": contract.sha256_file(stderr_path),
        },
        "stdout": {
            "path": str(stdout_path),
            "sha256": contract.sha256_file(stdout_path),
        },
        "timeout_seconds": int(timeout_seconds),
        "wall_time_ms": wall_time_ms,
    }
    if process.returncode != 0:
        contract.fail(
            f"command exited with code {process.returncode}: {' '.join(command)}"
        )
    return record, process.stdout, process.stderr


def _executed_test_statuses(stdout: bytes) -> dict[str, str]:
    try:
        text = stdout.decode("utf-8")
    except UnicodeDecodeError as error:
        raise contract.DiagnosticError(
            "integrity-test stdout was not UTF-8"
        ) from error
    statuses: dict[str, str] = {}
    for line in text.splitlines():
        if not line.startswith("test "):
            continue
        name_and_status = line[len("test ") :]
        name, separator, status = name_and_status.rpartition(" ... ")
        if not separator:
            continue
        if status == "ok":
            normalized = "ok"
        elif status == "ignored" or status.startswith("ignored,"):
            normalized = "ignored"
        else:
            continue
        if name in statuses:
            contract.fail(f"duplicate integrity-test result line: {name}")
        statuses[name] = normalized
    return statuses


def _reserve_build_root(artifact_root: Path, *, official: bool) -> Path:
    build_root = artifact_root / "build"
    if official and artifact_root.exists():
        contract.fail(f"refusing existing official artifact root: {artifact_root}")
    if not official and build_root.exists():
        contract.fail(f"refusing existing build output: {build_root}")
    try:
        if official:
            artifact_root.mkdir(parents=True, exist_ok=False)
            build_root.mkdir(exist_ok=False)
        else:
            build_root.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        raise contract.DiagnosticError(
            f"could not reserve build output {build_root}: {error}"
        ) from error
    return build_root


def build(
    *,
    repo: Path,
    target_dir: Path,
    artifact_root: Path,
    manifest: Path,
    require_windows: bool = True,
) -> dict[str, Any]:
    if require_windows:
        contract.require_frozen_windows_path(
            target_dir,
            contract.TARGET_DIR_WINDOWS,
            "official Cargo target directory",
        )
        contract.require_frozen_windows_path(
            artifact_root,
            contract.ARTIFACT_ROOT_WINDOWS,
            "official artifact root",
        )
        if os.name != "nt":
            contract.fail("the frozen diagnostic build must run with Windows Python")
    repo = repo.resolve()
    target_dir = target_dir.resolve()
    artifact_root = artifact_root.resolve()
    manifest = manifest.resolve()
    expected_manifest = (repo / contract.MANIFEST_RELATIVE_PATH).resolve()
    if not contract.same_path(manifest, expected_manifest):
        contract.fail(
            f"manifest must be the repository authority: {expected_manifest}"
        )
    if not manifest.is_file():
        contract.fail(f"missing execution manifest: {manifest}")
    cargo_lock = repo / "Cargo.lock"
    if not cargo_lock.is_file():
        contract.fail(f"missing Cargo.lock: {cargo_lock}")

    head = contract.require_clean_worktree(repo)
    build_root = _reserve_build_root(artifact_root, official=require_windows)

    command = contract.cargo_build_command()
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(target_dir)
    stdout_path = build_root / "cargo-build.jsonl"
    stderr_path = build_root / "cargo-build.stderr.log"
    started_utc = _utc_now()
    started = time.perf_counter()
    raw_lines: list[bytes] = []
    with stdout_path.open("xb") as stdout_file, stderr_path.open("xb") as stderr_file:
        try:
            process = subprocess.Popen(
                command,
                cwd=repo,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=stderr_file,
            )
        except OSError as error:
            raise contract.DiagnosticError(f"could not launch Windows Cargo: {error}") from error
        assert process.stdout is not None
        for line in process.stdout:
            stdout_file.write(line)
            raw_lines.append(line)
        return_code = process.wait()
    elapsed_ms = round((time.perf_counter() - started) * 1000)
    if return_code != 0:
        contract.fail(f"cargo build failed with exit code {return_code}")

    messages = _parse_cargo_messages(raw_lines)
    executable = contract.resolve_lib_test_executable(messages).resolve()
    list_command = [str(executable), "--list"]
    test_list_stdout = build_root / "test-list.stdout.txt"
    test_list_stderr = build_root / "test-list.stderr.txt"
    test_list_record, listed_stdout, _ = _run_recorded_command(
        list_command,
        repo=repo,
        stdout_path=test_list_stdout,
        stderr_path=test_list_stderr,
        timeout_seconds=60.0,
    )
    try:
        listed_text = listed_stdout.decode("utf-8")
    except UnicodeDecodeError as error:
        raise contract.DiagnosticError("lib-test --list was not UTF-8") from error
    listed_names = contract.listed_test_names(listed_text)
    missing = [name for name in contract.REQUIRED_TESTS if name not in listed_names]
    if missing:
        contract.fail(f"required diagnostic tests absent from --list: {missing}")
    listed_module_tests = {
        name
        for name in listed_names
        if name.startswith(contract.TEST_MODULE + "::")
    }
    if listed_module_tests != set(contract.REQUIRED_TESTS):
        contract.fail(
            "diagnostic module --list drift: "
            f"expected={sorted(contract.REQUIRED_TESTS)} "
            f"actual={sorted(listed_module_tests)}"
        )
    test_list_record["listed_test_count"] = len(listed_names)

    integrity_command = contract.integrity_test_command(executable)
    integrity_record, integrity_stdout, _ = _run_recorded_command(
        integrity_command,
        repo=repo,
        stdout_path=build_root / "integrity-tests.stdout.log",
        stderr_path=build_root / "integrity-tests.stderr.log",
        timeout_seconds=300.0,
    )
    executed = _executed_test_statuses(integrity_stdout)
    expected_statuses = {
        name: ("ignored" if name == contract.PROBE_TEST else "ok")
        for name in contract.REQUIRED_TESTS
    }
    if executed != expected_statuses:
        contract.fail(
            "final executable integrity-test results mismatch: "
            f"expected={expected_statuses} actual={executed}"
        )
    integrity_record["executed_test_statuses"] = executed

    audit_report_path = repo / contract.STATIC_AUDIT_REPORT_RELATIVE_PATH
    audit_report = contract.read_json_document(audit_report_path)
    contract.verify_payload_sha256(audit_report, "static audit report")
    if audit_report.get("schema") != contract.STATIC_AUDIT_SCHEMA:
        contract.fail("static audit report schema mismatch")
    audit_status = audit_report.get("status")
    if audit_status not in contract.STATIC_AUDIT_STATUSES:
        contract.fail(f"static audit report status is unsupported: {audit_status!r}")
    if audit_report.get("decision", {}).get("status") != audit_status:
        contract.fail("static audit report decision/status mismatch")

    audit_check_record, audit_check_stdout, _ = _run_recorded_command(
        contract.static_audit_check_command(),
        repo=repo,
        stdout_path=build_root / "static-audit-check.stdout.log",
        stderr_path=build_root / "static-audit-check.stderr.log",
        timeout_seconds=60.0,
    )
    try:
        audit_check_text = audit_check_stdout.decode("utf-8")
    except UnicodeDecodeError as error:
        raise contract.DiagnosticError(
            "static audit check stdout was not UTF-8"
        ) from error
    expected_marker = (
        f"{contract.STATIC_AUDIT_POSITIVE_MARKER} status={audit_status} "
        f"payload_sha256={audit_report['payload_sha256']}"
    )
    if audit_check_text.splitlines().count(expected_marker) != 1:
        contract.fail(
            "static audit check did not emit its exact positive marker once"
        )

    audit_test_record, _, audit_test_stderr = _run_recorded_command(
        contract.static_audit_test_command(),
        repo=repo,
        stdout_path=build_root / "static-audit-tests.stdout.log",
        stderr_path=build_root / "static-audit-tests.stderr.log",
        timeout_seconds=60.0,
    )
    audit_test_count = contract.unittest_success_count(
        audit_test_stderr, "static-audit unittest stderr"
    )
    if audit_test_count != contract.STATIC_AUDIT_REQUIRED_TEST_COUNT:
        contract.fail(
            "static-audit unittest count mismatch: "
            f"expected={contract.STATIC_AUDIT_REQUIRED_TEST_COUNT} "
            f"actual={audit_test_count}"
        )
    audit_test_record["executed_test_count"] = audit_test_count

    after_head = contract.require_clean_worktree(repo)
    if after_head != head:
        contract.fail(f"git HEAD changed during build: before={head} after={after_head}")

    build_source = Path(__file__).resolve()
    contract_source = Path(contract.__file__).resolve()
    completed_utc = _utc_now()
    receipt: dict[str, Any] = {
        "build_source": {
            "path": str(build_source),
            "sha256": contract.sha256_file(build_source),
        },
        "cargo": {
            "command": command,
            "environment": {"CARGO_TARGET_DIR": str(target_dir)},
            "exit_code": return_code,
            "locked": True,
            "no_default_features": True,
            "release": True,
            "requested_features": [],
            "stderr": {
                "path": str(stderr_path),
                "sha256": contract.sha256_file(stderr_path),
            },
            "stdout": {
                "path": str(stdout_path),
                "sha256": contract.sha256_file(stdout_path),
            },
            "target_dir": str(target_dir),
            "wall_time_ms": elapsed_ms,
        },
        "cargo_lock": {
            "path": str(cargo_lock.resolve()),
            "sha256": contract.sha256_file(cargo_lock),
        },
        "completed_utc": completed_utc,
        "contract_source": {
            "path": str(contract_source),
            "sha256": contract.sha256_file(contract_source),
        },
        "executable": {
            "compiler_artifact_target_kind": ["lib"],
            "path": str(executable),
            "sha256": contract.sha256_file(executable),
        },
        "git_head": head,
        "git_status_clean_before_and_after": True,
        "integrity_tests": integrity_record,
        "label": contract.LABEL,
        "manifest": {
            "path": str(manifest),
            "sha256": contract.sha256_file(manifest),
        },
        "required_tests": list(contract.REQUIRED_TESTS),
        "schema": contract.BUILD_RECEIPT_SCHEMA,
        "started_utc": started_utc,
        "static_audit": {
            "check": audit_check_record,
            "report": {
                "path": str(audit_report_path.resolve()),
                "payload_sha256": audit_report["payload_sha256"],
                "schema": audit_report["schema"],
                "sha256": contract.sha256_file(audit_report_path),
                "status": audit_status,
            },
            "tests": audit_test_record,
        },
        "test_list": test_list_record,
    }
    contract.attach_payload_sha256(receipt)
    receipt_path = build_root / "build-receipt.json"
    contract.write_exclusive(receipt_path, contract.record_bytes(receipt))
    print(json.dumps(receipt, sort_keys=True, separators=(",", ":")))
    return receipt


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument(
        "--target-dir",
        type=Path,
        default=Path(contract.TARGET_DIR_WINDOWS),
    )
    parser.add_argument(
        "--artifact-root",
        type=Path,
        default=Path(contract.ARTIFACT_ROOT_WINDOWS),
    )
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args()
    manifest = args.manifest or args.repo / contract.MANIFEST_RELATIVE_PATH
    build(
        repo=args.repo,
        target_dir=args.target_dir,
        artifact_root=args.artifact_root,
        manifest=manifest,
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"OBS_DIAGNOSTICS_BUILD_ABORT: {error}", file=sys.stderr)
        raise
