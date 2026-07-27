#!/usr/bin/env python3
"""Run and receipt the authoritative Diagnostic B classifier fail closed."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
from pathlib import PurePosixPath
import subprocess
import sys
import time
from typing import Any, Mapping

try:
    from scripts.observation_diagnostics_v1 import (
        classify_results,
        contract,
        run_probe,
    )
except ModuleNotFoundError:  # Direct execution from this directory.
    import classify_results  # type: ignore[no-redef]
    import contract  # type: ignore[no-redef]
    import run_probe  # type: ignore[no-redef]


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _canonical_receipt(path: Path, where: str) -> dict[str, Any]:
    document = contract.read_json_document(path)
    try:
        raw = path.read_bytes()
    except OSError as error:
        raise contract.DiagnosticError(f"could not read {where}: {error}") from error
    if raw != contract.record_bytes(document):
        contract.fail(f"{where} must be canonical JSON plus one newline")
    contract.verify_payload_sha256(document, where)
    return document


def _source_record(path: Path) -> dict[str, Any]:
    resolved = path.resolve()
    return {"path": str(resolved), "sha256": contract.sha256_file(resolved)}


def _file_record(path: Path) -> dict[str, Any]:
    return {
        "byte_count": path.stat().st_size,
        "path": str(path.resolve()),
        "sha256": contract.sha256_file(path),
    }


def _validate_completion_output_inventory(
    completion: Mapping[str, Any],
    *,
    artifact_root: Path,
    required: bool,
) -> dict[str, Any]:
    raw_inventory = completion.get("output_inventory")
    if raw_inventory is None and not required:
        return {
            "aggregate_sha256": None,
            "file_count": 0,
            "validated": False,
        }
    inventory = contract.require_array(
        raw_inventory,
        "completion receipt.output_inventory",
    )
    if required and len(inventory) != len(contract.PAIR_SPECS) * 5:
        contract.fail(
            "completion receipt.output_inventory must contain exactly "
            f"{len(contract.PAIR_SPECS) * 5} files"
        )
    if not inventory:
        contract.fail("completion receipt.output_inventory must not be empty")

    verified: list[dict[str, Any]] = []
    seen_paths: set[str] = set()
    for index, raw_record in enumerate(inventory):
        where = f"completion receipt.output_inventory[{index}]"
        record = contract.exact_keys(
            raw_record,
            {"byte_count", "path", "sha256"},
            where,
        )
        relative = record["path"]
        if type(relative) is not str or not relative:
            contract.fail(f"{where}.path must be a nonempty string")
        pure = PurePosixPath(relative)
        if (
            pure.is_absolute()
            or str(pure) != relative
            or any(part in ("", ".", "..") for part in pure.parts)
            or ":" in relative
            or "\\" in relative
        ):
            contract.fail(f"{where}.path must be a normalized relative POSIX path")
        if relative in seen_paths:
            contract.fail(f"duplicate completion output_inventory path: {relative}")
        seen_paths.add(relative)
        path = artifact_root.joinpath(*pure.parts).resolve()
        contract.require_descendant_path(path, artifact_root, f"{where}.path")
        expected_size = contract.require_natural(
            record["byte_count"],
            f"{where}.byte_count",
        )
        expected_sha256 = contract.require_sha256(
            record["sha256"],
            f"{where}.sha256",
        )
        try:
            actual_size = path.stat().st_size
        except OSError as error:
            raise contract.DiagnosticError(
                f"could not stat bound completion artifact {path}: {error}"
            ) from error
        if actual_size != expected_size:
            contract.fail(f"{where}.byte_count mismatch after classification")
        if contract.sha256_file(path) != expected_sha256:
            contract.fail(f"{where}.sha256 mismatch after classification")
        verified.append(
            {
                "byte_count": expected_size,
                "path": relative,
                "sha256": expected_sha256,
            }
        )
    verified.sort(key=lambda record: record["path"])
    return {
        "aggregate_sha256": hashlib.sha256(
            contract.canonical_json_bytes(verified)
        ).hexdigest(),
        "file_count": len(verified),
        "validated": True,
    }


def _reserve_classification_root(root: Path) -> None:
    if root.exists():
        contract.fail(f"refusing existing classification output root: {root}")
    try:
        root.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        raise contract.DiagnosticError(
            f"could not reserve classification output root {root}: {error}"
        ) from error


def classifier_command(
    classifier_source: Path,
    completion_receipt: Path,
    output: Path,
) -> list[str]:
    return [
        sys.executable,
        str(classifier_source.resolve()),
        "--completion-receipt",
        str(completion_receipt.resolve()),
        "--output",
        str(output.resolve()),
    ]


def _validate_input_authorities(
    *,
    repo: Path,
    artifact_root: Path,
    completion_path: Path,
    manifest: Path,
    head: str,
    require_full_build_verification: bool,
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any], dict[str, Any]]:
    completion = _canonical_receipt(completion_path, "completion receipt")
    if completion.get("schema") != contract.COMPLETION_RECEIPT_SCHEMA:
        contract.fail("completion receipt schema mismatch")
    if completion.get("label") != contract.LABEL:
        contract.fail("completion receipt label mismatch")
    if completion.get("status") != "COMPLETE":
        contract.fail("completion receipt status must be COMPLETE")
    if completion.get("git_head") != head:
        contract.fail("completion receipt git_head mismatch")
    if completion.get("git_status_clean_before_and_after") is not True:
        contract.fail("completion receipt does not attest a clean worktree")

    manifest_record = contract.exact_keys(
        completion.get("manifest"),
        {"path", "sha256"},
        "completion receipt.manifest",
    )
    if not contract.same_path(manifest_record["path"], manifest):
        contract.fail("completion receipt manifest path mismatch")
    if manifest_record["sha256"] != contract.sha256_file(manifest):
        contract.fail("completion receipt manifest SHA-256 mismatch")

    build_path = (artifact_root / "build" / "build-receipt.json").resolve()
    build_binding = contract.exact_keys(
        completion.get("build_receipt"),
        {"path", "sha256"},
        "completion receipt.build_receipt",
    )
    if not contract.same_path(build_binding["path"], build_path):
        contract.fail("completion receipt build-receipt path mismatch")
    if build_binding["sha256"] != contract.sha256_file(build_path):
        contract.fail("completion receipt build-receipt SHA-256 mismatch")

    build = _canonical_receipt(build_path, "build receipt")
    if build.get("schema") != contract.BUILD_RECEIPT_SCHEMA:
        contract.fail("build receipt schema mismatch")
    if build.get("label") != contract.LABEL:
        contract.fail("build receipt label mismatch")
    if build.get("git_head") != head:
        contract.fail("build receipt git_head mismatch")
    if build.get("manifest") != completion.get("manifest"):
        contract.fail("build and completion manifest bindings differ")
    if require_full_build_verification:
        run_probe.verify_build_receipt(
            build,
            receipt_path=build_path,
            repo=repo,
            manifest=manifest,
            head=head,
            require_frozen_target_dir=True,
        )

    inventory_binding = _validate_completion_output_inventory(
        completion,
        artifact_root=artifact_root,
        required=require_full_build_verification,
    )
    return (
        {
            "path": str(completion_path),
            "payload_sha256": completion["payload_sha256"],
            "sha256": contract.sha256_file(completion_path),
        },
        {
            "path": str(build_path),
            "payload_sha256": build["payload_sha256"],
            "sha256": contract.sha256_file(build_path),
        },
        completion,
        inventory_binding,
    )


def _validate_classification_output(
    *,
    output_path: Path,
    classifier_source: Path,
    completion_binding: Mapping[str, Any],
    contract_source_binding: Mapping[str, Any],
    manifest_sha256: str,
    head: str,
) -> dict[str, Any]:
    document = contract.read_json_document(output_path)
    try:
        raw = output_path.read_bytes()
    except OSError as error:
        raise contract.DiagnosticError(
            f"could not read classification output: {error}"
        ) from error
    expected_raw = classify_results.canonical_json_bytes(document) + b"\n"
    if raw != expected_raw:
        contract.fail("classification output is not exact canonical JSON plus newline")
    expected_payload_sha256 = classify_results.payload_sha256(document)
    if document.get("payload_sha256") != expected_payload_sha256:
        contract.fail("classification output payload SHA-256 mismatch")
    if document.get("schema") != classify_results.REPORT_SCHEMA:
        contract.fail("classification output schema mismatch")
    if document.get("label") != contract.LABEL:
        contract.fail("classification output label mismatch")
    if document.get("input_mode") != "authoritative-launch-completion-receipt":
        contract.fail("classification output is not in authoritative input mode")
    if document.get("classification_authority") != "AUTHORITATIVE-DIAGNOSTIC-READ":
        contract.fail("classification output lacks authoritative classification status")
    if document.get("authoritative_pair_store_binding") is not True:
        contract.fail("classification output lacks authoritative Store binding")
    if document.get("execution_manifest_sha256") != manifest_sha256:
        contract.fail("classification output manifest SHA-256 mismatch")

    source = contract.exact_keys(
        document.get("classifier_source"),
        {"path", "sha256"},
        "classification output.classifier_source",
    )
    expected_source = _source_record(classifier_source)
    if source != expected_source:
        contract.fail("classification output classifier-source binding mismatch")
    if document.get("classifier_source_sha256") != expected_source["sha256"]:
        contract.fail("classification output classifier-source SHA-256 mismatch")
    output_contract = contract.exact_keys(
        document.get("contract_source"),
        {"path", "sha256"},
        "classification output.contract_source",
    )
    if output_contract != contract_source_binding:
        contract.fail("classification output contract-source binding mismatch")

    completion = contract.exact_keys(
        document.get("completion_receipt"),
        {"path", "sha256", "payload_sha256", "git_head"},
        "classification output.completion_receipt",
    )
    expected_completion = {
        **dict(completion_binding),
        "git_head": head,
    }
    if completion != expected_completion:
        contract.fail("classification output completion-receipt binding mismatch")

    return {
        **_file_record(output_path),
        "classification_authority": document["classification_authority"],
        "payload_sha256": document["payload_sha256"],
        "schema": document["schema"],
        "validated": True,
    }


def _partial_output_record(path: Path) -> dict[str, Any]:
    if not path.is_file():
        return {
            "byte_count": None,
            "classification_authority": None,
            "path": str(path.resolve()),
            "payload_sha256": None,
            "schema": None,
            "sha256": None,
            "validated": False,
        }
    return {
        **_file_record(path),
        "classification_authority": None,
        "payload_sha256": None,
        "schema": None,
        "validated": False,
    }


def execute(
    *,
    repo: Path,
    artifact_root: Path,
    require_windows: bool = True,
    classifier_source: Path | None = None,
    timeout_seconds: float = contract.CLASSIFIER_TIMEOUT_SECONDS,
    require_full_build_verification: bool | None = None,
    _test_head_override: str | None = None,
) -> dict[str, Any]:
    if require_windows:
        contract.require_frozen_windows_path(
            artifact_root,
            contract.ARTIFACT_ROOT_WINDOWS,
            "official artifact root",
        )
        if timeout_seconds != contract.CLASSIFIER_TIMEOUT_SECONDS:
            contract.fail("official classifier timeout is frozen at 120 seconds")
        if _test_head_override is not None:
            contract.fail("official classifier runner forbids a HEAD override")
        if require_full_build_verification is False:
            contract.fail("official classifier runner requires full build verification")
        if os.name != "nt":
            contract.fail("the frozen classifier runner must run with Windows Python")
    if (
        timeout_seconds <= 0
        or timeout_seconds > contract.CLASSIFIER_TIMEOUT_SECONDS
        or int(timeout_seconds) != timeout_seconds
    ):
        contract.fail(
            f"classifier timeout must be within "
            f"(0,{contract.CLASSIFIER_TIMEOUT_SECONDS}] whole seconds"
        )

    repo = repo.resolve()
    artifact_root = artifact_root.resolve()
    manifest = (repo / contract.MANIFEST_RELATIVE_PATH).resolve()
    expected_source = (
        repo / "scripts" / "observation_diagnostics_v1" / "classify_results.py"
    ).resolve()
    if classifier_source is None:
        classifier_source = expected_source
    else:
        classifier_source = classifier_source.resolve()
    if require_windows and not contract.same_path(classifier_source, expected_source):
        contract.fail(f"official classifier source must be {expected_source}")
    if not classifier_source.is_file():
        contract.fail(f"missing classifier source: {classifier_source}")
    if not manifest.is_file():
        contract.fail(f"missing execution manifest: {manifest}")

    classification_root = artifact_root / "classification"
    completion_path = (artifact_root / "completion-receipt.json").resolve()
    output_path = (classification_root / "classification.json").resolve()
    stdout_path = (classification_root / "classifier.stdout.log").resolve()
    stderr_path = (classification_root / "classifier.stderr.log").resolve()
    receipt_path = (
        classification_root / "classification-receipt.json"
    ).resolve()
    if require_windows:
        for value, expected, where in (
            (
                completion_path,
                contract.COMPLETION_RECEIPT_WINDOWS,
                "official completion receipt",
            ),
            (classification_root, contract.CLASSIFICATION_ROOT_WINDOWS, "official classification root"),
            (output_path, contract.CLASSIFICATION_OUTPUT_WINDOWS, "official classification output"),
            (stdout_path, contract.CLASSIFICATION_STDOUT_WINDOWS, "official classifier stdout"),
            (stderr_path, contract.CLASSIFICATION_STDERR_WINDOWS, "official classifier stderr"),
            (receipt_path, contract.CLASSIFICATION_RECEIPT_WINDOWS, "official classification receipt"),
        ):
            contract.require_frozen_windows_path(value, expected, where)
    if classification_root.exists():
        contract.fail(
            f"refusing existing classification output root: {classification_root}"
        )

    if _test_head_override is not None:
        if contract.GIT_HEAD_RE.fullmatch(_test_head_override) is None:
            contract.fail("test HEAD override must be a lower-case Git commit")
        head = _test_head_override
    else:
        head = (
            contract.require_clean_worktree(repo)
            if require_windows
            else contract.checked_git(repo, "rev-parse", "HEAD")
        )
    (
        completion_binding,
        build_binding,
        completion_document,
        inventory_preflight,
    ) = _validate_input_authorities(
        repo=repo,
        artifact_root=artifact_root,
        completion_path=completion_path,
        manifest=manifest,
        head=head,
        require_full_build_verification=(
            require_windows
            if require_full_build_verification is None
            else require_full_build_verification
        ),
    )
    command = classifier_command(classifier_source, completion_path, output_path)
    classifier_source_binding = _source_record(classifier_source)
    manifest_binding = _source_record(manifest)
    wrapper_binding = _source_record(Path(__file__))
    contract_binding = _source_record(Path(contract.__file__))

    _reserve_classification_root(classification_root)
    started_utc = _utc_now()
    started = time.perf_counter()
    timed_out = False
    launch_error: str | None = None
    exit_code: int | None = None
    stdout = b""
    stderr = b""
    try:
        process = subprocess.Popen(
            command,
            cwd=repo,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except OSError as error:
        launch_error = f"{type(error).__name__}: {error}"
    else:
        try:
            stdout, stderr = process.communicate(timeout=timeout_seconds)
        except subprocess.TimeoutExpired:
            timed_out = True
            process.kill()
            stdout, stderr = process.communicate()
        exit_code = process.returncode
    wall_time_ms = round((time.perf_counter() - started) * 1000)
    contract.write_exclusive(stdout_path, stdout)
    contract.write_exclusive(stderr_path, stderr)

    failures: list[str] = []
    if len(stdout) > contract.MAX_PROCESS_STREAM_BYTES:
        failures.append("classifier stdout exceeds the frozen size cap")
    if len(stderr) > contract.MAX_PROCESS_STREAM_BYTES:
        failures.append("classifier stderr exceeds the frozen size cap")
    if launch_error is not None:
        failures.append(f"classifier launch failed: {launch_error}")
    if timed_out:
        failures.append(
            f"classifier exceeded the {int(timeout_seconds)}-second timeout"
        )
    if exit_code != 0:
        failures.append(f"classifier exit code was {exit_code!r}, expected 0")

    output_record = _partial_output_record(output_path)
    if not failures:
        try:
            output_record = _validate_classification_output(
                output_path=output_path,
                classifier_source=classifier_source,
                completion_binding=completion_binding,
                contract_source_binding=contract_binding,
                manifest_sha256=manifest_binding["sha256"],
                head=head,
            )
        except Exception as error:
            failures.append(
                f"classification output validation failed: "
                f"{type(error).__name__}: {error}"
            )
            output_record = _partial_output_record(output_path)

    for path, binding, name in (
        (classifier_source, classifier_source_binding, "classifier source"),
        (Path(wrapper_binding["path"]), wrapper_binding, "classifier wrapper source"),
        (Path(contract_binding["path"]), contract_binding, "contract source"),
        (manifest, manifest_binding, "execution manifest"),
        (completion_path, completion_binding, "completion receipt"),
        (Path(build_binding["path"]), build_binding, "build receipt"),
    ):
        try:
            actual = contract.sha256_file(path)
        except Exception as error:
            failures.append(f"{name} postflight hash failed: {error}")
        else:
            if actual != binding["sha256"]:
                failures.append(f"{name} changed during classification")
    inventory_postflight: dict[str, Any]
    try:
        inventory_postflight = _validate_completion_output_inventory(
            completion_document,
            artifact_root=artifact_root,
            required=(
                require_windows
                if require_full_build_verification is None
                else require_full_build_verification
            ),
        )
        if inventory_postflight != inventory_preflight:
            failures.append(
                "completion output_inventory binding changed during classification"
            )
    except Exception as error:
        failures.append(
            "completion output_inventory postflight failed: "
            f"{type(error).__name__}: {error}"
        )
        inventory_postflight = {
            "aggregate_sha256": None,
            "file_count": 0,
            "validated": False,
        }
    worktree_clean_before_and_after = True
    try:
        final_head = (
            head
            if _test_head_override is not None
            else (
                contract.require_clean_worktree(repo)
                if require_windows
                else contract.checked_git(repo, "rev-parse", "HEAD")
            )
        )
        if final_head != head:
            worktree_clean_before_and_after = False
            failures.append(
                f"git HEAD changed during classification: {head} -> {final_head}"
            )
    except Exception as error:
        worktree_clean_before_and_after = False
        failures.append(f"worktree postflight failed: {error}")

    if output_record.get("validated") is True:
        try:
            output_record = _validate_classification_output(
                output_path=output_path,
                classifier_source=classifier_source,
                completion_binding=completion_binding,
                contract_source_binding=contract_binding,
                manifest_sha256=manifest_binding["sha256"],
                head=head,
            )
        except Exception as error:
            failures.append(
                "classification output final snapshot failed: "
                f"{type(error).__name__}: {error}"
            )
            output_record = _partial_output_record(output_path)

    completed_utc = _utc_now()
    stdout_record = _file_record(stdout_path)
    stderr_record = _file_record(stderr_path)
    receipt: dict[str, Any] = {
        "build_receipt": build_binding,
        "classification_output": output_record,
        "classifier_source": classifier_source_binding,
        "command": command,
        "completed_utc": completed_utc,
        "completion_output_inventory_postflight": inventory_postflight,
        "completion_output_inventory_preflight": inventory_preflight,
        "completion_receipt": completion_binding,
        "contract_source": contract_binding,
        "exit_code": exit_code,
        "failure": "; ".join(failures) if failures else None,
        "git_head": head,
        "git_status_clean_before_and_after": worktree_clean_before_and_after,
        "label": contract.LABEL,
        "manifest": manifest_binding,
        "schema": contract.CLASSIFICATION_RECEIPT_SCHEMA,
        "started_utc": started_utc,
        "status": "FAILED" if failures else "COMPLETE",
        "stderr": stderr_record,
        "stdout": stdout_record,
        "timed_out": timed_out,
        "timeout_seconds": int(timeout_seconds),
        "wall_time_ms": wall_time_ms,
        "wrapper_source": wrapper_binding,
    }
    contract.attach_payload_sha256(receipt)
    contract.write_exclusive(receipt_path, contract.record_bytes(receipt))
    if failures:
        contract.fail(
            "classifier execution failed; canonical failure receipt written to "
            f"{receipt_path}: {receipt['failure']}"
        )
    print(
        "OBS_DIAGNOSTICS_CLASSIFICATION_RECEIPT="
        + json.dumps(receipt, sort_keys=True, separators=(",", ":"))
    )
    return receipt


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    args = parser.parse_args()
    execute(
        repo=args.repo,
        artifact_root=Path(contract.ARTIFACT_ROOT_WINDOWS),
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"OBS_DIAGNOSTICS_CLASSIFIER_ABORT: {error}", file=sys.stderr)
        raise
