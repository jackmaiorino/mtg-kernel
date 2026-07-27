from __future__ import annotations

import json
from pathlib import Path
import contextlib
import io
import tempfile
import unittest

try:
    from scripts.observation_diagnostics_v1 import (
        classify_results,
        contract,
        run_classifier,
    )
except ModuleNotFoundError:
    import classify_results  # type: ignore[no-redef]
    import contract  # type: ignore[no-redef]
    import run_classifier  # type: ignore[no-redef]


ROOT = Path(__file__).resolve().parents[2]


def _write_authorities(
    artifact_root: Path,
    *,
    head: str = "a" * 40,
) -> tuple[Path, str]:
    manifest = (ROOT / contract.MANIFEST_RELATIVE_PATH).resolve()
    manifest_record = {
        "path": str(manifest),
        "sha256": contract.sha256_file(manifest),
    }
    build_path = artifact_root / "build" / "build-receipt.json"
    build_path.parent.mkdir(parents=True)
    build: dict[str, object] = {
        "git_head": head,
        "label": contract.LABEL,
        "manifest": manifest_record,
        "schema": contract.BUILD_RECEIPT_SCHEMA,
    }
    contract.attach_payload_sha256(build)
    build_path.write_bytes(contract.record_bytes(build))
    bound_artifact = artifact_root / "runs" / "fixture-input.log"
    bound_artifact.parent.mkdir()
    bound_artifact.write_bytes(b"bound diagnostic input\n")
    completion_path = artifact_root / "completion-receipt.json"
    completion: dict[str, object] = {
        "build_receipt": {
            "path": str(build_path.resolve()),
            "sha256": contract.sha256_file(build_path),
        },
        "git_head": head,
        "git_status_clean_before_and_after": True,
        "label": contract.LABEL,
        "manifest": manifest_record,
        "output_inventory": [
            {
                "byte_count": bound_artifact.stat().st_size,
                "path": "runs/fixture-input.log",
                "sha256": contract.sha256_file(bound_artifact),
            }
        ],
        "schema": contract.COMPLETION_RECEIPT_SCHEMA,
        "status": "COMPLETE",
    }
    contract.attach_payload_sha256(completion)
    completion_path.write_bytes(contract.record_bytes(completion))
    return completion_path, head


def _write_success_classifier(
    path: Path,
    *,
    mutate_bound_artifact: bool = False,
) -> None:
    manifest = (ROOT / contract.MANIFEST_RELATIVE_PATH).resolve()
    source = f"""\
import argparse
import hashlib
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--completion-receipt", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
completion_path = args.completion_receipt.resolve()
completion_raw = completion_path.read_bytes()
completion = json.loads(completion_raw)
if {mutate_bound_artifact!r}:
    (completion_path.parent / "runs" / "fixture-input.log").write_bytes(
        b"mutated diagnostic input\\n"
    )
source_path = Path(__file__).resolve()
source_sha256 = hashlib.sha256(source_path.read_bytes()).hexdigest()
manifest_path = Path({str(manifest)!r})
report = {{
    "authoritative_pair_store_binding": True,
    "classification_authority": "AUTHORITATIVE-DIAGNOSTIC-READ",
    "classifier_source": {{
        "path": str(source_path),
        "sha256": source_sha256,
    }},
    "classifier_source_sha256": source_sha256,
    "contract_source": {{
        "path": {str(Path(contract.__file__).resolve())!r},
        "sha256": hashlib.sha256(
            Path({str(Path(contract.__file__).resolve())!r}).read_bytes()
        ).hexdigest(),
    }},
    "completion_receipt": {{
        "git_head": completion["git_head"],
        "path": str(completion_path),
        "payload_sha256": completion["payload_sha256"],
        "sha256": hashlib.sha256(completion_raw).hexdigest(),
    }},
    "execution_manifest_sha256": hashlib.sha256(
        manifest_path.read_bytes()
    ).hexdigest(),
    "input_mode": "authoritative-launch-completion-receipt",
    "label": {contract.LABEL!r},
    "schema": {classify_results.REPORT_SCHEMA!r},
}}
payload = json.dumps(
    report, ensure_ascii=False, sort_keys=True, separators=(",", ":")
).encode("utf-8")
report["payload_sha256"] = hashlib.sha256(payload).hexdigest()
raw = json.dumps(
    report, ensure_ascii=False, sort_keys=True, separators=(",", ":")
).encode("utf-8") + b"\\n"
with args.output.open("xb") as destination:
    destination.write(raw)
print("fixture classifier complete")
"""
    path.write_text(source, encoding="utf-8")


class FrozenClassifierRunnerTests(unittest.TestCase):
    def test_success_records_exact_command_logs_and_authority_bindings(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact_root = Path(directory) / "artifacts"
            artifact_root.mkdir()
            completion_path, head = _write_authorities(artifact_root)
            classifier_source = Path(directory) / "fixture_classifier.py"
            _write_success_classifier(classifier_source)

            with contextlib.redirect_stdout(io.StringIO()):
                receipt = run_classifier.execute(
                    repo=ROOT,
                    artifact_root=artifact_root,
                    require_windows=False,
                    classifier_source=classifier_source,
                    timeout_seconds=5,
                    require_full_build_verification=False,
                    _test_head_override=head,
                )

            classification_root = artifact_root / "classification"
            receipt_path = classification_root / "classification-receipt.json"
            output_path = classification_root / "classification.json"
            self.assertEqual(receipt["status"], "COMPLETE")
            self.assertEqual(receipt["exit_code"], 0)
            self.assertFalse(receipt["timed_out"])
            self.assertEqual(receipt["git_head"], head)
            self.assertEqual(
                receipt["command"],
                run_classifier.classifier_command(
                    classifier_source, completion_path, output_path
                ),
            )
            self.assertTrue(receipt["classification_output"]["validated"])
            self.assertEqual(
                receipt["classification_output"]["classification_authority"],
                "AUTHORITATIVE-DIAGNOSTIC-READ",
            )
            self.assertEqual(
                receipt["classification_output"]["sha256"],
                contract.sha256_file(output_path),
            )
            parsed_receipt = contract.read_json_document(receipt_path)
            contract.verify_payload_sha256(
                parsed_receipt, "classification receipt fixture"
            )
            self.assertEqual(
                receipt_path.read_bytes(),
                contract.record_bytes(parsed_receipt),
            )
            for stream in ("stdout", "stderr"):
                self.assertEqual(
                    receipt[stream]["sha256"],
                    contract.sha256_file(receipt[stream]["path"]),
                )

    def test_nonzero_child_writes_a_canonical_failure_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact_root = Path(directory) / "artifacts"
            artifact_root.mkdir()
            _, head = _write_authorities(artifact_root)
            classifier_source = Path(directory) / "failing_classifier.py"
            classifier_source.write_text(
                "import sys\n"
                "print('fixture failure', file=sys.stderr)\n"
                "raise SystemExit(7)\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                contract.DiagnosticError, "failure receipt written"
            ):
                run_classifier.execute(
                    repo=ROOT,
                    artifact_root=artifact_root,
                    require_windows=False,
                    classifier_source=classifier_source,
                    timeout_seconds=5,
                    require_full_build_verification=False,
                    _test_head_override=head,
                )
            receipt_path = (
                artifact_root
                / "classification"
                / "classification-receipt.json"
            )
            receipt = contract.read_json_document(receipt_path)
            contract.verify_payload_sha256(receipt, "failure receipt fixture")
            self.assertEqual(receipt["status"], "FAILED")
            self.assertEqual(receipt["exit_code"], 7)
            self.assertFalse(receipt["timed_out"])
            self.assertFalse(receipt["classification_output"]["validated"])
            self.assertIn("exit code was 7", receipt["failure"])

    def test_timeout_kills_child_and_records_actual_termination(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact_root = Path(directory) / "artifacts"
            artifact_root.mkdir()
            _, head = _write_authorities(artifact_root)
            classifier_source = Path(directory) / "slow_classifier.py"
            classifier_source.write_text(
                "import time\n"
                "time.sleep(3)\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                contract.DiagnosticError, "failure receipt written"
            ):
                run_classifier.execute(
                    repo=ROOT,
                    artifact_root=artifact_root,
                    require_windows=False,
                    classifier_source=classifier_source,
                    timeout_seconds=1,
                    require_full_build_verification=False,
                    _test_head_override=head,
                )
            receipt = contract.read_json_document(
                artifact_root
                / "classification"
                / "classification-receipt.json"
            )
            self.assertEqual(receipt["status"], "FAILED")
            self.assertTrue(receipt["timed_out"])
            self.assertIsInstance(receipt["exit_code"], int)
            self.assertEqual(receipt["timeout_seconds"], 1)

    def test_zero_exit_with_invalid_output_writes_failure_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact_root = Path(directory) / "artifacts"
            artifact_root.mkdir()
            _, head = _write_authorities(artifact_root)
            classifier_source = Path(directory) / "invalid_classifier.py"
            classifier_source.write_text(
                "import argparse\n"
                "from pathlib import Path\n"
                "parser = argparse.ArgumentParser()\n"
                "parser.add_argument('--completion-receipt')\n"
                "parser.add_argument('--output', type=Path, required=True)\n"
                "args = parser.parse_args()\n"
                "args.output.write_bytes(b'{}\\n')\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                contract.DiagnosticError, "failure receipt written"
            ):
                run_classifier.execute(
                    repo=ROOT,
                    artifact_root=artifact_root,
                    require_windows=False,
                    classifier_source=classifier_source,
                    timeout_seconds=5,
                    require_full_build_verification=False,
                    _test_head_override=head,
                )
            receipt = contract.read_json_document(
                artifact_root
                / "classification"
                / "classification-receipt.json"
            )
            self.assertEqual(receipt["exit_code"], 0)
            self.assertEqual(receipt["status"], "FAILED")
            self.assertFalse(receipt["classification_output"]["validated"])
            self.assertIn("output validation failed", receipt["failure"])

    def test_child_input_mutation_writes_failure_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact_root = Path(directory) / "artifacts"
            artifact_root.mkdir()
            _, head = _write_authorities(artifact_root)
            classifier_source = Path(directory) / "mutating_classifier.py"
            _write_success_classifier(
                classifier_source,
                mutate_bound_artifact=True,
            )
            with self.assertRaisesRegex(
                contract.DiagnosticError,
                "failure receipt written",
            ):
                run_classifier.execute(
                    repo=ROOT,
                    artifact_root=artifact_root,
                    require_windows=False,
                    classifier_source=classifier_source,
                    timeout_seconds=5,
                    require_full_build_verification=False,
                    _test_head_override=head,
                )
            receipt = contract.read_json_document(
                artifact_root
                / "classification"
                / "classification-receipt.json"
            )
            self.assertEqual(receipt["status"], "FAILED")
            self.assertIn(
                "completion output_inventory postflight failed",
                receipt["failure"],
            )
            self.assertFalse(
                receipt["completion_output_inventory_postflight"]["validated"]
            )

    def test_official_override_and_existing_output_are_rejected(self) -> None:
        with self.assertRaisesRegex(
            contract.DiagnosticError, "official artifact root"
        ):
            run_classifier.execute(
                repo=ROOT,
                artifact_root=Path(r"X:\override-artifacts"),
                require_windows=True,
            )
        with tempfile.TemporaryDirectory() as directory:
            artifact_root = Path(directory) / "artifacts"
            (artifact_root / "classification").mkdir(parents=True)
            with self.assertRaisesRegex(
                contract.DiagnosticError,
                "existing classification output root",
            ):
                run_classifier.execute(
                    repo=ROOT,
                    artifact_root=artifact_root,
                    require_windows=False,
                )


if __name__ == "__main__":
    unittest.main()
