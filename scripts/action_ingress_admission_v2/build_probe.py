#!/usr/bin/env python3
"""Build and bind the sole CPU lib-test executable for admission v2."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time
from typing import Any, Mapping

try:
    from scripts.action_ingress_admission_v2 import contract
except ModuleNotFoundError:  # Direct execution from this directory.
    import contract  # type: ignore[no-redef]


BUILD_TIMEOUT_SECONDS = 1_800
CONTROL_TIMEOUT_SECONDS = 300
STATIC_TIMEOUT_SECONDS = 120
LIST_TIMEOUT_SECONDS = 60


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _run_recorded(
    command: list[str],
    *,
    repo: Path,
    stdout_path: Path,
    stderr_path: Path,
    timeout_seconds: int,
    environment: Mapping[str, str] | None = None,
) -> tuple[dict[str, Any], bytes, bytes]:
    started = time.perf_counter()
    try:
        process = subprocess.run(
            command,
            cwd=repo,
            env=dict(environment) if environment is not None else None,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        stdout = error.stdout or b""
        stderr = error.stderr or b""
        contract.write_exclusive(stdout_path, stdout)
        contract.write_exclusive(stderr_path, stderr)
        contract.fail(f"command exceeded {timeout_seconds}s: {command}")
    except OSError as error:
        raise contract.AdmissionError(f"could not execute {command}: {error}") from error
    elapsed_ms = round((time.perf_counter() - started) * 1000)
    contract.write_exclusive(stdout_path, process.stdout)
    contract.write_exclusive(stderr_path, process.stderr)
    record = {
        "command": command,
        "exit_code": process.returncode,
        "stderr": contract.file_record(stderr_path),
        "stdout": contract.file_record(stdout_path),
        "timeout_seconds": timeout_seconds,
        "wall_time_ms": elapsed_ms,
    }
    if process.returncode != 0:
        contract.fail(f"command exited {process.returncode}: {command}")
    return record, process.stdout, process.stderr


def _cargo_messages(stdout: bytes) -> list[dict[str, Any]]:
    messages = [
        contract.parse_json_bytes(line, f"Cargo JSON line {index + 1}")
        for index, line in enumerate(stdout.splitlines())
        if line.strip()
    ]
    if not messages:
        contract.fail("Cargo emitted no JSON messages")
    return messages


def _test_statuses(stdout: bytes) -> dict[str, str]:
    try:
        text = stdout.decode("utf-8")
    except UnicodeDecodeError as error:
        raise contract.AdmissionError("Rust control stdout was not UTF-8") from error
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
        if normalized is None:
            continue
        if name in statuses:
            contract.fail(f"duplicate Rust test status: {name}")
        statuses[name] = normalized
    return statuses


def _verify_static_report(repo: Path) -> dict[str, Any]:
    path = repo / contract.STATIC_REPORT_RELATIVE_PATH
    document = contract.read_json_document(path)
    contract.verify_payload_sha256(document, "static admission report")
    contract.exact_keys(
        document,
        {
            "admission_criteria",
            "authority",
            "canonical_equivalences",
            "collisions",
            "combat_boolean_repair",
            "corpus",
            "coverage",
            "decision",
            "dimensions",
            "frozen_no_repair",
            "inputs",
            "non_claims",
            "payload_sha256",
            "schema",
            "status",
            "structured_aliases",
            "supplemental_case",
            "transform",
        },
        "static admission report",
    )
    if document.get("schema") != contract.STATIC_REPORT_SCHEMA:
        contract.fail("static admission report schema mismatch")
    if contract.sha256_file(path) != contract.STATIC_REPORT_SHA256:
        contract.fail("static admission report file SHA-256 mismatch")
    if document["payload_sha256"] != contract.STATIC_REPORT_PAYLOAD_SHA256:
        contract.fail("static admission report payload SHA-256 mismatch")
    if (
        contract.sha256_file(repo / contract.STATIC_TOOL_RELATIVE_PATH)
        != contract.STATIC_TOOL_SHA256
    ):
        contract.fail("static admission tool SHA-256 mismatch")
    if (
        contract.sha256_file(repo / contract.STATIC_TEST_RELATIVE_PATH)
        != contract.STATIC_TEST_SHA256
    ):
        contract.fail("static admission tests SHA-256 mismatch")
    if document.get("status") != "STATIC-ADMITTED":
        contract.fail("static admission report must have status STATIC-ADMITTED")
    corpus = document.get("corpus")
    if not isinstance(corpus, Mapping):
        contract.fail("static admission report corpus must be an object")
    if (
        corpus.get("frozen_case_count") != 115
        or corpus.get("supplemental_case_count") != 1
        or corpus.get("combined_case_count") != 116
    ):
        contract.fail("static admission corpus must bind exactly 115+1=116 cases")
    if corpus.get("combined_identity") != contract.STATIC_COMBINED_IDENTITY:
        contract.fail("static combined corpus identity mismatch")
    if (
        corpus.get("combined_identity_sha256")
        != contract.STATIC_COMBINED_IDENTITY_SHA256
    ):
        contract.fail("static combined corpus SHA-256 mismatch")
    if corpus.get("case_identity_digest_contract") != contract.STATIC_CASE_DIGEST_CONTRACT:
        contract.fail("static case-identity digest contract mismatch")
    rows = corpus.get("case_identity_rows")
    if not isinstance(rows, list) or len(rows) != 116:
        contract.fail("static report must contain all 116 case-identity rows")
    coverage = document.get("coverage")
    if not isinstance(coverage, Mapping):
        contract.fail("static admission report coverage must be an object")
    augmented = coverage.get("augmented_116_case")
    if not isinstance(augmented, Mapping):
        contract.fail("static augmented coverage must be an object")
    if augmented.get("covered_model_input_atoms") != 202:
        contract.fail("static action witnessed count must be 202")
    if augmented.get("declared_model_input_atoms") != 202:
        contract.fail("static action total count must be 202")
    for field in (
        "unwitnessed_model_input_atoms",
        "boolean_polarity_gaps",
        "optional_presence_gaps",
    ):
        if augmented.get(field) != []:
            contract.fail(f"static coverage {field} must be empty")
    collisions = document.get("collisions")
    if (
        not isinstance(collisions, Mapping)
        or len(collisions) != 4
        or any(value != [] for value in collisions.values())
    ):
        contract.fail("all four static collision lists must be empty")
    aliases = document.get("structured_aliases")
    if not isinstance(aliases, Mapping) or aliases.get("repaired") != []:
        contract.fail("repaired static structured aliases must be empty")
    equivalences = document.get("canonical_equivalences")
    if (
        not isinstance(equivalences, Mapping)
        or equivalences.get("unexpected_groups") != []
        or equivalences.get("observed_groups") != equivalences.get("expected_groups")
        or len(equivalences.get("observed_groups", [])) != 3
    ):
        contract.fail("static canonical equivalence allowlist mismatch")
    criteria = document.get("admission_criteria")
    if (
        not isinstance(criteria, Mapping)
        or not criteria
        or any(type(value) is not bool or value is not True for value in criteria.values())
    ):
        contract.fail("every static admission criterion must be true")
    return {
        "path": str(path.resolve()),
        "payload_sha256": document["payload_sha256"],
        "schema": document["schema"],
        "sha256": contract.sha256_file(path),
        "status": document["status"],
    }


def _positive_static_check(stdout: bytes, payload_sha256: str, where: str) -> None:
    try:
        lines = stdout.decode("utf-8").splitlines()
    except UnicodeDecodeError as error:
        raise contract.AdmissionError(f"{where} stdout was not UTF-8") from error
    marker = (
        f"{contract.STATIC_POSITIVE_MARKER} status=STATIC-ADMITTED "
        f"combined_identity_sha256={contract.STATIC_COMBINED_IDENTITY_SHA256} "
        f"payload_sha256={payload_sha256}"
    )
    if lines.count(marker) != 1:
        contract.fail(f"{where} did not emit its exact positive marker once")


def build(
    *,
    repo: Path,
    artifact_root: Path,
    target_dir: Path,
    manifest: Path,
    require_windows: bool = True,
    wsl_executable: str = "wsl.exe",
) -> dict[str, Any]:
    if require_windows:
        if os.name != "nt":
            contract.fail("official admission build must run with Windows Python")
        contract.verify_official_windows_python()
        contract.require_frozen_windows_path(
            repo, contract.WORKTREE_WINDOWS, "repository worktree"
        )
        contract.require_frozen_windows_path(
            artifact_root, contract.ARTIFACT_ROOT_WINDOWS, "artifact root"
        )
        contract.require_frozen_windows_path(
            target_dir, contract.TARGET_DIR_WINDOWS, "Cargo target"
        )
    if os.path.lexists(artifact_root):
        contract.fail(f"artifact root must be absent: {artifact_root}")
    if os.path.lexists(target_dir):
        contract.fail(f"Cargo target must be absent: {target_dir}")
    repo = repo.resolve()
    artifact_root = artifact_root.resolve()
    target_dir = target_dir.resolve()
    manifest = manifest.resolve()
    if not contract.same_path(manifest, repo / contract.MANIFEST_RELATIVE_PATH):
        contract.fail("manifest is not the repository authority")
    preserved_v1_before = (
        contract.verify_preserved_v1_evidence() if require_windows else None
    )
    head = contract.require_clean_worktree(repo)
    branch = contract.require_frozen_branch(repo)
    licensed_retry_diff = contract.verify_licensed_retry_diff(repo)
    frozen_inputs = contract.verify_frozen_inputs(repo)
    implementation_sources = contract.implementation_source_records(repo)
    rustup = contract.resolved_tool_executable("rustup")
    tool_paths = {
        name: contract.selected_rust_toolchain_executable(name, rustup=rustup)
        for name in ("cargo", "rustc")
    }
    build_environment_policy = contract.build_environment_policy(
        os.environ,
        repo=repo,
        target_dir=target_dir,
        rustc=tool_paths["rustc"],
    )
    started_utc = _utc_now()

    try:
        artifact_root.mkdir(parents=True, exist_ok=False)
        build_root = artifact_root / "build"
        build_root.mkdir(exist_ok=False)
    except OSError as error:
        raise contract.AdmissionError(f"could not reserve artifact root: {error}") from error
    if contract._is_reparse_point(artifact_root) or contract._is_reparse_point(
        build_root
    ):
        contract.fail("new artifact/build roots must be non-reparse directories")

    static_report = _verify_static_report(repo)
    static_runs: list[dict[str, Any]] = []
    commands = [
        ("linux-check", contract.linux_static_commands(wsl_executable)[0], "check"),
        ("linux-tests", contract.linux_static_commands(wsl_executable)[1], "tests"),
        ("windows-check", contract.windows_static_commands(sys.executable)[0], "check"),
        ("windows-tests", contract.windows_static_commands(sys.executable)[1], "tests"),
    ]
    for name, command, kind in commands:
        record, stdout, stderr = _run_recorded(
            command,
            repo=repo,
            stdout_path=build_root / f"{name}.stdout.log",
            stderr_path=build_root / f"{name}.stderr.log",
            timeout_seconds=STATIC_TIMEOUT_SECONDS,
        )
        record["name"] = name
        if kind == "check":
            _positive_static_check(stdout, static_report["payload_sha256"], name)
        else:
            count = contract.unittest_success_count(stderr, f"{name} stderr")
            if (
                contract.STATIC_REQUIRED_TEST_COUNT
                and count != contract.STATIC_REQUIRED_TEST_COUNT
            ):
                contract.fail(
                    f"{name} test count mismatch: "
                    f"expected={contract.STATIC_REQUIRED_TEST_COUNT} actual={count}"
                )
            record["executed_test_count"] = count
        static_runs.append(record)

    packaging_runs: list[dict[str, Any]] = []
    packaging_commands = [
        (
            "linux-packaging-tests",
            contract.linux_packaging_test_command(wsl_executable),
        ),
        (
            "windows-packaging-tests",
            contract.windows_packaging_test_command(sys.executable),
        ),
    ]
    for name, command in packaging_commands:
        record, _, stderr = _run_recorded(
            command,
            repo=repo,
            stdout_path=build_root / f"{name}.stdout.log",
            stderr_path=build_root / f"{name}.stderr.log",
            timeout_seconds=STATIC_TIMEOUT_SECONDS,
        )
        count = contract.unittest_success_count(stderr, f"{name} stderr")
        if count != contract.PACKAGING_REQUIRED_TEST_COUNT:
            contract.fail(
                f"{name} test count mismatch: "
                f"expected={contract.PACKAGING_REQUIRED_TEST_COUNT} actual={count}"
            )
        record["executed_test_count"] = count
        record["name"] = name
        packaging_runs.append(record)

    runtime_commands = {
        name: [str(path), "--version", "--verbose"]
        for name, path in tool_paths.items()
    }
    runtime_tools: dict[str, Any] = {}
    for name, command in runtime_commands.items():
        record, stdout, _ = _run_recorded(
            command,
            repo=repo,
            stdout_path=build_root / f"runtime-{name}.stdout.log",
            stderr_path=build_root / f"runtime-{name}.stderr.log",
            timeout_seconds=60,
        )
        try:
            text = stdout.decode("utf-8")
        except UnicodeDecodeError as error:
            raise contract.AdmissionError(
                f"{name} runtime version was not UTF-8"
            ) from error
        if not text.strip():
            contract.fail(f"{name} runtime version output was empty")
        record["executable"] = contract.file_record(tool_paths[name])
        record["version_stdout"] = text
        runtime_tools[name] = record

    environment = os.environ.copy()
    environment.update(build_environment_policy["effective_overrides"])
    cargo_record, cargo_stdout, _ = _run_recorded(
        contract.cargo_build_command(str(tool_paths["cargo"])),
        repo=repo,
        stdout_path=build_root / "cargo-build.jsonl",
        stderr_path=build_root / "cargo-build.stderr.log",
        timeout_seconds=BUILD_TIMEOUT_SECONDS,
        environment=environment,
    )
    cargo_record["environment"] = {
        key: environment[key]
        for key in (
            "CARGO_TARGET_DIR",
            "CUDA_VISIBLE_DEVICES",
            "MTG_KERNEL_DEVICE",
            "RUSTC",
        )
    }
    cargo_executable = contract.resolve_lib_test_executable(_cargo_messages(cargo_stdout))
    if contract._is_reparse_point(cargo_executable):
        contract.fail("Cargo lib-test executable must not be a reparse point")
    executable = cargo_executable.resolve()
    if not target_dir.is_dir() or contract._is_reparse_point(target_dir):
        contract.fail("Cargo target must be a newly created non-reparse directory")
    contract.require_descendant_path(executable, target_dir, "lib-test executable")

    list_record, list_stdout, _ = _run_recorded(
        [str(executable), "--list"],
        repo=repo,
        stdout_path=build_root / "test-list.stdout.log",
        stderr_path=build_root / "test-list.stderr.log",
        timeout_seconds=LIST_TIMEOUT_SECONDS,
    )
    try:
        listed = contract.listed_test_names(list_stdout.decode("utf-8"))
    except UnicodeDecodeError as error:
        raise contract.AdmissionError("Rust test list was not UTF-8") from error
    module_tests = {name for name in listed if name.startswith(contract.TEST_MODULE + "::")}
    if module_tests != set(contract.REQUIRED_TESTS):
        contract.fail(
            f"action-ingress test module drift: "
            f"expected={sorted(contract.REQUIRED_TESTS)} actual={sorted(module_tests)}"
        )
    list_record["listed_test_count"] = len(listed)

    control_record, control_stdout, _ = _run_recorded(
        contract.integrity_test_command(executable),
        repo=repo,
        stdout_path=build_root / "rust-controls.stdout.log",
        stderr_path=build_root / "rust-controls.stderr.log",
        timeout_seconds=CONTROL_TIMEOUT_SECONDS,
        environment=environment,
    )
    expected_statuses = {
        name: ("ignored" if name == contract.PROBE_TEST else "ok")
        for name in contract.REQUIRED_TESTS
    }
    statuses = _test_statuses(control_stdout)
    if statuses != expected_statuses:
        contract.fail(
            f"Rust controls mismatch: expected={expected_statuses} actual={statuses}"
        )
    control_record["executed_test_statuses"] = statuses

    after_head = contract.require_clean_worktree(repo)
    if contract.require_frozen_branch(repo) != branch:
        contract.fail("git branch changed during build")
    if after_head != head:
        contract.fail(f"git HEAD changed during build: before={head} after={after_head}")
    preserved_v1_after = (
        contract.verify_preserved_v1_evidence() if require_windows else None
    )
    if preserved_v1_after != preserved_v1_before:
        contract.fail("preserved v1 evidence changed during the v2 build")
    build_inventory = [
        contract.file_record(path, root=artifact_root)
        for path in build_root.iterdir()
        if path.is_file()
    ]
    build_inventory.sort(key=lambda record: record["path"])
    if len(build_inventory) != 22:
        contract.fail("build output inventory must contain exactly 22 pre-receipt files")
    receipt: dict[str, Any] = {
        "cargo": cargo_record,
        "build_environment_policy": build_environment_policy,
        "cargo_lock": {
            "path": str((repo / "Cargo.lock").resolve()),
            "sha256": contract.sha256_file(repo / "Cargo.lock"),
        },
        "completed_utc": _utc_now(),
        "cpu_only": True,
        "executable": {
            "path": str(executable),
            "sha256": contract.sha256_file(executable),
        },
        "frozen_inputs": frozen_inputs,
        "git_head": head,
        "git_branch": branch,
        "git_status_clean_before_and_after": True,
        "implementation_sources": implementation_sources,
        "label": contract.LABEL,
        "licensed_retry_diff": licensed_retry_diff,
        "manifest": {
            "path": str(manifest),
            "sha256": contract.sha256_file(manifest),
        },
        "output_inventory": build_inventory,
        "packaging_preflight": {
            "required_test_count": contract.PACKAGING_REQUIRED_TEST_COUNT,
            "runs": packaging_runs,
        },
        "preserved_v1_evidence_after": preserved_v1_after,
        "preserved_v1_evidence_before": preserved_v1_before,
        "required_tests": list(contract.REQUIRED_TESTS),
        "rust_controls": control_record,
        "runtime_tuple": {
            "machine": platform.machine(),
            "os_name": os.name,
            "platform": platform.platform(),
            "python_executable": sys.executable,
            "python_version": sys.version,
            "sys_platform": sys.platform,
            "tools": runtime_tools,
        },
        "schema": contract.BUILD_RECEIPT_SCHEMA,
        "started_utc": started_utc,
        "static_preflight": {
            "report": static_report,
            "runs": static_runs,
        },
        "test_list": list_record,
        "toolchain": {
            "cargo": contract.file_record(tool_paths["cargo"]),
            "channel": contract.RUST_TOOLCHAIN_CHANNEL,
            "rustc": contract.file_record(tool_paths["rustc"]),
            "rustup": contract.file_record(rustup),
        },
    }
    contract.attach_payload_sha256(receipt)
    contract.write_exclusive(
        build_root / "build-receipt.json", contract.record_bytes(receipt)
    )
    print(json.dumps(receipt, sort_keys=True, separators=(",", ":")))
    return receipt


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument(
        "--artifact-root", type=Path, default=Path(contract.ARTIFACT_ROOT_WINDOWS)
    )
    parser.add_argument(
        "--target-dir", type=Path, default=Path(contract.TARGET_DIR_WINDOWS)
    )
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--wsl-executable", default="wsl.exe")
    args = parser.parse_args()
    build(
        repo=args.repo,
        artifact_root=args.artifact_root,
        target_dir=args.target_dir,
        manifest=args.manifest or args.repo / contract.MANIFEST_RELATIVE_PATH,
        wsl_executable=args.wsl_executable,
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"ACTION_INGRESS_ADMISSION_V2_BUILD_INVALID: {error}", file=sys.stderr)
        raise
