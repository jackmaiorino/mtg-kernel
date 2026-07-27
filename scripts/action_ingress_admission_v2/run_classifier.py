#!/usr/bin/env python3
"""Run and receipt the complete admission-v2 classifier as a separate process."""

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
    from scripts.action_ingress_admission_v2 import classify_results, contract, run_probe
except ModuleNotFoundError:  # Direct execution from this directory.
    import classify_results  # type: ignore[no-redef]
    import contract  # type: ignore[no-redef]
    import run_probe  # type: ignore[no-redef]


CLASSIFIER_TIMEOUT_SECONDS = 120


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _source_record(path: Path) -> dict[str, Any]:
    return {
        "path": str(path.resolve()),
        "sha256": contract.sha256_file(path),
    }


def _verify_results_against_payloads(
    results: list[Any],
    payloads: list[tuple[contract.ModelSpec, dict[str, Any]]],
) -> None:
    if len(results) != len(payloads) or len(results) != len(contract.MODEL_SPECS):
        contract.fail("classification result/payload cardinality mismatch")
    for spec, raw_result, (payload_spec, payload) in zip(
        contract.MODEL_SPECS, results, payloads, strict=True
    ):
        if payload_spec != spec:
            contract.fail("postflight payload authority order mismatch")
        contrast = payload["effects"]["digest_minus_direct"]
        expected = {
            "authority_kind": spec.kind,
            "classification": run_probe._expected_label(spec.identity, contrast),
            "digest_minus_direct": {
                field: contrast[field] for field in sorted(run_probe.CONTRAST_KEYS)
            },
            "identity": spec.identity,
            "ordinal": spec.ordinal,
        }
        if dict(raw_result) != expected:
            contract.fail(
                f"{spec.identity} classification differs from postflight payload metrics"
            )


def run_classifier(
    *,
    repo: Path,
    artifact_root: Path,
    target_dir: Path,
    completion_path: Path,
    classification_root: Path,
    require_windows: bool = True,
) -> dict[str, Any]:
    if require_windows:
        if os.name != "nt":
            contract.fail("official classification must run with Windows Python")
        contract.verify_official_windows_python()
        contract.require_frozen_windows_path(
            artifact_root, contract.ARTIFACT_ROOT_WINDOWS, "artifact root"
        )
        contract.require_frozen_windows_path(
            target_dir, contract.TARGET_DIR_WINDOWS, "Cargo target"
        )
        contract.require_frozen_windows_path(
            repo, contract.WORKTREE_WINDOWS, "repository worktree"
        )
    if not completion_path.is_file() or run_probe._is_reparse_point(completion_path):
        contract.fail("completion receipt must be a non-reparse regular file")
    repo = repo.resolve()
    artifact_root = artifact_root.resolve()
    target_dir = target_dir.resolve()
    completion_path = completion_path.resolve()
    classification_root = classification_root.resolve()
    if not contract.same_path(classification_root, artifact_root / "classification"):
        contract.fail("classification root must be artifact-root/classification")
    if not contract.same_path(completion_path, artifact_root / "completion-receipt.json"):
        contract.fail("completion receipt path mismatch")
    if classification_root.exists():
        contract.fail(f"classification root must be absent: {classification_root}")
    completion_sha_before = contract.sha256_file(completion_path)
    preserved_v1_before = (
        contract.verify_preserved_v1_evidence() if require_windows else None
    )
    head = contract.require_clean_worktree(repo)
    branch = contract.require_frozen_branch(repo)
    started_utc = _utc_now()
    classification_root.mkdir(exist_ok=False)

    output_path = classification_root / "classification.json"
    stdout_path = classification_root / "classifier.stdout.log"
    stderr_path = classification_root / "classifier.stderr.log"
    classifier_source = (
        repo / "scripts" / "action_ingress_admission_v2" / "classify_results.py"
    )
    command = [
        sys.executable,
        str(classifier_source),
        "--repo",
        str(repo),
        "--artifact-root",
        str(artifact_root),
        "--target-dir",
        str(target_dir),
        "--completion",
        str(completion_path),
        "--output",
        str(output_path),
    ]
    started = time.perf_counter()
    try:
        process = subprocess.run(
            command,
            cwd=repo,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=CLASSIFIER_TIMEOUT_SECONDS,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        contract.write_exclusive(stdout_path, error.stdout or b"")
        contract.write_exclusive(stderr_path, error.stderr or b"")
        contract.fail("classifier exceeded exactly 120 seconds")
    except OSError as error:
        raise contract.AdmissionError(f"could not launch classifier: {error}") from error
    wall_time_ms = round((time.perf_counter() - started) * 1000)
    contract.write_exclusive(stdout_path, process.stdout)
    contract.write_exclusive(stderr_path, process.stderr)
    run_probe._exact_natural(process.returncode, 0, "classifier exit_code")
    if not output_path.is_file():
        contract.fail("classifier omitted its required output; whole screen is INVALID")
    for path in (output_path, stdout_path, stderr_path):
        if run_probe._is_reparse_point(path):
            contract.fail(f"classification output must not be a reparse point: {path}")
    report = contract.read_canonical_record(output_path, "classification output")
    contract.exact_keys(
        report,
        {
            "admission_status",
            "completion_receipt",
            "model_results",
            "no_global_label",
            "nonclaims",
            "payload_sha256",
            "schema",
            "status",
        },
        "classification output",
    )
    contract.require_string(
        report["schema"],
        "classification.schema",
        contract.CLASSIFICATION_SCHEMA,
    )
    contract.require_string(report["status"], "classification.status", "VALID")
    contract.require_string(
        report["admission_status"],
        "classification.admission_status",
        "STATIC-AND-RUNTIME-ADMITTED",
    )
    contract.require_bool(
        report["no_global_label"], "classification.no_global_label", True
    )
    completion_binding = contract.exact_keys(
        report["completion_receipt"],
        {"path", "payload_sha256", "sha256"},
        "classification.completion_receipt",
    )
    completion_document = contract.read_canonical_record(
        completion_path, "classification-bound completion receipt"
    )
    if (
        not contract.same_path(completion_binding["path"], completion_path)
        or contract.require_sha256(
            completion_binding["payload_sha256"],
            "classification.completion_receipt.payload_sha256",
        )
        != completion_document["payload_sha256"]
        or contract.require_sha256(
            completion_binding["sha256"],
            "classification.completion_receipt.sha256",
        )
        != completion_sha_before
    ):
        contract.fail("classifier output completion-receipt binding mismatch")
    if process.stdout != contract.record_bytes(report):
        contract.fail("classifier stdout must equal its canonical report bytes")
    if process.stderr:
        contract.fail("successful classifier stderr must be empty")
    results = contract.require_array(report["model_results"], "classification.model_results", 3)
    expected_identities = [spec.identity for spec in contract.MODEL_SPECS]
    if [result.get("identity") for result in results if isinstance(result, dict)] != expected_identities:
        contract.fail("classification model identities/order mismatch")
    for spec, result in zip(contract.MODEL_SPECS, results, strict=True):
        result = contract.exact_keys(
            result,
            {
                "authority_kind",
                "classification",
                "digest_minus_direct",
                "identity",
                "ordinal",
            },
            f"classification.model_results[{spec.ordinal - 1}]",
        )
        contract.require_string(
            result["authority_kind"],
            f"classification.{spec.identity}.authority_kind",
            spec.kind,
        )
        contract.require_string(
            result["identity"],
            f"classification.{spec.identity}.identity",
            spec.identity,
        )
        run_probe._exact_natural(
            result["ordinal"],
            spec.ordinal,
            f"classification.{spec.identity}.ordinal",
        )
        label = contract.require_string(
            result["classification"],
            f"classification.{spec.identity}.classification",
        )
        allowed = (
            {
                "RAW-INIT-DIGEST-DOMINANT",
                "RAW-INIT-DIRECT-DOMINANT",
                "RAW-INIT-MIXED",
            }
            if spec.kind == "raw"
            else {
                "IMPORTED-DIGEST-DOMINANT",
                "IMPORTED-DIRECT-DOMINANT",
                "IMPORTED-MIXED",
            }
        )
        if label not in allowed:
            contract.fail(f"{spec.identity} classification label is not allowed")
        contrast = contract.exact_keys(
            result["digest_minus_direct"],
            run_probe.CONTRAST_KEYS,
            f"classification.{spec.identity}.digest_minus_direct",
        )
        for field in run_probe.CONTRAST_KEYS:
            run_probe._number(
                contrast[field],
                f"classification.{spec.identity}.digest_minus_direct.{field}",
            )
    nonclaims = contract.require_array(
        report["nonclaims"], "classification.nonclaims"
    )
    if not nonclaims or any(type(value) is not str or not value for value in nonclaims):
        contract.fail("classification.nonclaims must contain nonempty strings")
    if "label" in report or "global_label" in report:
        contract.fail("classification output must not contain a global label")
    postflight_completion, postflight_payloads = classify_results.validate_completion(
        repo=repo,
        artifact_root=artifact_root,
        target_dir=target_dir,
        completion_path=completion_path,
        require_windows=require_windows,
        allow_populated_classification=True,
    )
    if (
        contract.sha256_file(completion_path) != completion_sha_before
        or postflight_completion["payload_sha256"]
        != completion_document["payload_sha256"]
    ):
        contract.fail("completion receipt changed during classification")
    _verify_results_against_payloads(results, postflight_payloads)
    report_after = contract.read_canonical_record(
        output_path, "postflight classification output"
    )
    if any(
        run_probe._is_reparse_point(path)
        for path in (output_path, stdout_path, stderr_path)
    ):
        contract.fail("classification output became a reparse point during postflight")
    if (
        report_after != report
        or stdout_path.read_bytes() != contract.record_bytes(report)
        or stderr_path.read_bytes()
    ):
        contract.fail("classification outputs changed during evidence postflight")
    after_head = contract.require_clean_worktree(repo)
    if contract.require_frozen_branch(repo) != branch:
        contract.fail("source branch changed during classification")
    if after_head != head:
        contract.fail("source commit changed during classification")
    preserved_v1_after = (
        contract.verify_preserved_v1_evidence() if require_windows else None
    )
    if preserved_v1_after != preserved_v1_before:
        contract.fail("preserved v1 evidence changed during v2 classification")

    receipt: dict[str, Any] = {
        "classification": contract.file_record(output_path),
        "command": command,
        "completed_utc": _utc_now(),
        "completion_receipt": {
            "path": str(completion_path),
            "sha256": completion_sha_before,
        },
        "exit_code": process.returncode,
        "git_head": head,
        "git_branch": branch,
        "git_status_clean_before_and_after": True,
        "payload_source_sha256": report["payload_sha256"],
        "preserved_v1_evidence_after": preserved_v1_after,
        "preserved_v1_evidence_before": preserved_v1_before,
        "schema": contract.CLASSIFICATION_RECEIPT_SCHEMA,
        "sources": [
            _source_record(repo / "scripts" / "action_ingress_admission_v2" / name)
            for name in (
                "classify_results.py",
                "contract.py",
                "run_classifier.py",
                "run_probe.py",
            )
        ],
        "started_utc": started_utc,
        "status": "VALID",
        "stderr": contract.file_record(stderr_path),
        "stdout": contract.file_record(stdout_path),
        "timed_out": False,
        "timeout_seconds": CLASSIFIER_TIMEOUT_SECONDS,
        "wall_time_ms": wall_time_ms,
    }
    contract.attach_payload_sha256(receipt)
    receipt_path = classification_root / "classification-receipt.json"
    contract.write_exclusive(receipt_path, contract.record_bytes(receipt))
    actual_names = {path.name for path in classification_root.iterdir()}
    if actual_names != {
        "classification-receipt.json",
        "classification.json",
        "classifier.stderr.log",
        "classifier.stdout.log",
    }:
        contract.fail("classification output file set mismatch")
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
    parser.add_argument("--completion", type=Path)
    parser.add_argument("--classification-root", type=Path)
    args = parser.parse_args()
    run_classifier(
        repo=args.repo,
        artifact_root=args.artifact_root,
        target_dir=args.target_dir,
        completion_path=args.completion
        or args.artifact_root / "completion-receipt.json",
        classification_root=args.classification_root
        or args.artifact_root / "classification",
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(
            f"ACTION_INGRESS_ADMISSION_V2_CLASSIFIER_RECEIPT_INVALID: {error}",
            file=sys.stderr,
        )
        raise
