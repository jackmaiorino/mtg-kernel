from __future__ import annotations

from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from scripts.action_ingress_admission_v1 import contract, run_classifier


class RunClassifierTests(unittest.TestCase):
    def test_fake_classifier_process_is_receipted(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifact = root / "artifact"
            artifact.mkdir()
            completion = artifact / "completion-receipt.json"
            completion_document = {"schema": "fixture-completion"}
            contract.attach_payload_sha256(completion_document)
            completion.write_bytes(contract.record_bytes(completion_document))
            classification_root = artifact / "classification"
            report = {
                "admission_status": "STATIC-AND-RUNTIME-ADMITTED",
                "completion_receipt": {
                    "path": str(completion),
                    "payload_sha256": completion_document["payload_sha256"],
                    "sha256": contract.sha256_file(completion),
                },
                "model_results": [
                    {
                        "authority_kind": spec.kind,
                        "classification": (
                            "RAW-INIT-MIXED"
                            if spec.kind == "raw"
                            else "IMPORTED-MIXED"
                        ),
                        "digest_minus_direct": {
                            "mean_centered_logit_rms_delta": 0.0,
                            "mean_jensen_shannon_nats": 0.0,
                            "top_action_flip_fraction": 0.0,
                        },
                        "identity": spec.identity,
                        "ordinal": spec.ordinal,
                    }
                    for spec in contract.MODEL_SPECS
                ],
                "no_global_label": True,
                "nonclaims": ["fixture"],
                "schema": contract.CLASSIFICATION_SCHEMA,
                "status": "VALID",
            }
            contract.attach_payload_sha256(report)
            stdout = contract.record_bytes(report)
            postflight_payloads = [
                (
                    spec,
                    {
                        "effects": {
                            "digest_minus_direct": {
                                "mean_centered_logit_rms_delta": 0.0,
                                "mean_jensen_shannon_nats": 0.0,
                                "top_action_flip_fraction": 0.0,
                            }
                        }
                    },
                )
                for spec in contract.MODEL_SPECS
            ]

            def fake_run(*args, **kwargs):
                output_index = args[0].index("--output") + 1
                output = Path(args[0][output_index])
                contract.write_exclusive(output, stdout)
                return subprocess.CompletedProcess(args[0], 0, stdout=stdout, stderr=b"")

            with mock.patch.object(
                run_classifier.contract,
                "require_clean_worktree",
                return_value="a" * 40,
            ), mock.patch.object(
                run_classifier.contract,
                "require_frozen_branch",
                return_value=contract.BRANCH,
            ), mock.patch.object(
                run_classifier.subprocess, "run", side_effect=fake_run
            ), mock.patch.object(
                run_classifier.classify_results,
                "validate_completion",
                return_value=(completion_document, postflight_payloads),
            ):
                receipt = run_classifier.run_classifier(
                    repo=Path(__file__).resolve().parents[2],
                    artifact_root=artifact,
                    target_dir=root / "target",
                    completion_path=completion,
                    classification_root=classification_root,
                    require_windows=False,
                )
            self.assertEqual(receipt["status"], "VALID")
            self.assertFalse(receipt["timed_out"])
            self.assertEqual(
                {path.name for path in classification_root.iterdir()},
                {
                    "classification-receipt.json",
                    "classification.json",
                    "classifier.stderr.log",
                    "classifier.stdout.log",
                },
            )

    def test_outer_receipt_rejects_wrong_but_allowed_label(self) -> None:
        contrast = {
            "mean_centered_logit_rms_delta": 1.0,
            "mean_jensen_shannon_nats": 1.0,
            "top_action_flip_fraction": 1.0,
        }
        payloads = [
            (spec, {"effects": {"digest_minus_direct": dict(contrast)}})
            for spec in contract.MODEL_SPECS
        ]
        results = [
            {
                "authority_kind": spec.kind,
                "classification": (
                    "RAW-INIT-DIRECT-DOMINANT"
                    if spec.kind == "raw"
                    else "IMPORTED-DIRECT-DOMINANT"
                ),
                "digest_minus_direct": dict(contrast),
                "identity": spec.identity,
                "ordinal": spec.ordinal,
            }
            for spec in contract.MODEL_SPECS
        ]
        with self.assertRaises(contract.AdmissionError):
            run_classifier._verify_results_against_payloads(results, payloads)

    def test_nonzero_fake_classifier_leaves_no_valid_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifact = root / "artifact"
            artifact.mkdir()
            completion = artifact / "completion-receipt.json"
            completion.write_bytes(b"completion")
            classification_root = artifact / "classification"
            failed = subprocess.CompletedProcess(
                ["fake"], 9, stdout=b"", stderr=b"invalid"
            )
            with mock.patch.object(
                run_classifier.contract,
                "require_clean_worktree",
                return_value="a" * 40,
            ), mock.patch.object(
                run_classifier.contract,
                "require_frozen_branch",
                return_value=contract.BRANCH,
            ), mock.patch.object(
                run_classifier.subprocess, "run", return_value=failed
            ), self.assertRaises(contract.AdmissionError):
                run_classifier.run_classifier(
                    repo=Path(__file__).resolve().parents[2],
                    artifact_root=artifact,
                    target_dir=root / "target",
                    completion_path=completion,
                    classification_root=classification_root,
                    require_windows=False,
                )
            self.assertFalse(
                (classification_root / "classification-receipt.json").exists()
            )
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifact = root / "artifact"
            artifact.mkdir()
            external = root / "external-completion.json"
            document = {"schema": "fixture"}
            contract.attach_payload_sha256(document)
            external.write_bytes(contract.record_bytes(document))
            completion = artifact / "completion-receipt.json"
            try:
                completion.symlink_to(external)
            except OSError:
                return
            with self.assertRaises(contract.AdmissionError):
                run_classifier.run_classifier(
                    repo=Path(__file__).resolve().parents[2],
                    artifact_root=artifact,
                    target_dir=root / "target",
                    completion_path=completion,
                    classification_root=artifact / "classification",
                    require_windows=False,
                )


if __name__ == "__main__":
    unittest.main()
