from __future__ import annotations

import copy
import math
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from scripts.action_ingress_admission_v1 import (
    classify_results,
    contract,
    test_fixtures,
)


class ClassifyResultsTests(unittest.TestCase):
    def _payloads(self, polarities: tuple[str, str, str]):
        return [
            (spec, test_fixtures.payload_for(spec, polarity=polarity))
            for spec, polarity in zip(contract.MODEL_SPECS, polarities, strict=True)
        ]

    def test_classifier_emits_only_three_authority_qualified_labels(self) -> None:
        completion = {"payload_sha256": "a" * 64}
        payloads = self._payloads(("positive", "negative", "mixed"))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            completion_path = root / "completion-receipt.json"
            completion_path.write_bytes(b"completion")
            output = root / "classification.json"
            with mock.patch.object(
                classify_results,
                "validate_completion",
                return_value=(completion, payloads),
            ):
                report = classify_results.classify(
                    repo=root,
                    artifact_root=root,
                    target_dir=root / "target",
                    completion_path=completion_path,
                    output_path=output,
                    require_windows=False,
                )
            self.assertEqual(
                [result["classification"] for result in report["model_results"]],
                [
                    "RAW-INIT-DIGEST-DOMINANT",
                    "IMPORTED-DIRECT-DOMINANT",
                    "IMPORTED-MIXED",
                ],
            )
            self.assertTrue(report["no_global_label"])
            self.assertNotIn("label", report)
            self.assertNotIn("global_label", report)
            contract.read_canonical_record(output, "classification")

    def test_classifier_rejects_producer_label_disagreement(self) -> None:
        completion = {"payload_sha256": "a" * 64}
        payloads = self._payloads(("positive", "positive", "positive"))
        payloads[1][1]["effects"]["descriptive_label"] = "IMPORTED-MIXED"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            completion_path = root / "completion-receipt.json"
            completion_path.write_bytes(b"completion")
            with mock.patch.object(
                classify_results,
                "validate_completion",
                return_value=(completion, payloads),
            ), self.assertRaises(contract.AdmissionError):
                classify_results.classify(
                    repo=root,
                    artifact_root=root,
                    target_dir=root / "target",
                    completion_path=completion_path,
                    output_path=root / "classification.json",
                    require_windows=False,
                )

    def test_inventory_rejects_omission_extra_and_byte_tamper(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            expected = classify_results._expected_run_files(root)
            for path in expected:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(path.name.encode())
            inventory = [
                contract.file_record(path, root=root) for path in expected
            ]
            inventory.sort(key=lambda row: row["path"])
            classify_results._validate_inventory(inventory, artifact_root=root)
            with self.assertRaises(contract.AdmissionError):
                classify_results._validate_inventory(inventory[:-1], artifact_root=root)
            extra = expected[0].parent / "extra"
            extra.write_bytes(b"x")
            with self.assertRaises(contract.AdmissionError):
                classify_results._validate_inventory(inventory, artifact_root=root)
            extra.unlink()
            runs_extra = root / "runs" / "unexpected"
            runs_extra.mkdir()
            with self.assertRaises(contract.AdmissionError):
                classify_results._validate_inventory(inventory, artifact_root=root)
            runs_extra.rmdir()
            expected[0].write_bytes(b"tampered")
            with self.assertRaises(contract.AdmissionError):
                classify_results._validate_inventory(inventory, artifact_root=root)

    def test_cross_model_corpus_only_blocks_are_exact(self) -> None:
        payloads = self._payloads(("positive", "negative", "mixed"))
        classify_results._verify_cross_model_invariants(payloads)

        binding_drift = copy.deepcopy(payloads)
        pre_transform = binding_drift[1][1]["admission"]["pre_transform_binding"]
        pre_transform["capture_sha256"] = "5" * 64
        pre_transform["revalidated_sha256"] = "5" * 64
        # This remains valid in isolation but cannot describe a different corpus
        # transcript under the same frozen three-model screen.
        run_probe = classify_results.run_probe
        run_probe.verify_probe_payload(binding_drift[1][1], binding_drift[1][0])
        with self.assertRaises(contract.AdmissionError):
            classify_results._verify_cross_model_invariants(binding_drift)

        statistics_drift = copy.deepcopy(payloads)
        statistics = statistics_drift[2][1]["input_statistics"]
        statistics["per_action_row"][0]["direct_squared_norm"] += 1.0
        direct_sum = sum(
            row["direct_squared_norm"] for row in statistics["per_action_row"]
        )
        statistics["direct_value_rms"] = math.sqrt(direct_sum / (1_115 * 99))
        statistics["mean_direct_squared_norm"] = direct_sum / 1_115
        run_probe.verify_probe_payload(statistics_drift[2][1], statistics_drift[2][0])
        with self.assertRaises(contract.AdmissionError):
            classify_results._verify_cross_model_invariants(statistics_drift)

        expected_bindings = run_probe.verify_probe_payload(
            payloads[0][1], payloads[0][0]
        )
        malformed_bindings = dict(expected_bindings)
        malformed_bindings["repaired_zero_ingress_row_digest_count"] = 1_115.0
        with self.assertRaises(contract.AdmissionError):
            classify_results._verify_probe_bindings(
                malformed_bindings,
                expected=expected_bindings,
                where="fixture.probe.bindings",
            )


if __name__ == "__main__":
    unittest.main()
