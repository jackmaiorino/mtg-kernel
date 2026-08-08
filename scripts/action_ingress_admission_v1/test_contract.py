from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from scripts.action_ingress_admission_v1 import contract


class ContractTests(unittest.TestCase):
    def test_payload_hash_round_trip_and_tamper(self) -> None:
        document = {"schema": "fixture", "nested": {"value": 3}}
        contract.attach_payload_sha256(document)
        contract.verify_payload_sha256(document, "fixture")
        document["nested"]["value"] = 4
        with self.assertRaises(contract.AdmissionError):
            contract.verify_payload_sha256(document, "fixture")

    def test_strict_parser_rejects_duplicate_and_nonfinite(self) -> None:
        for raw in (b'{"a":1,"a":2}', b'{"a":NaN}', b'{"a":Infinity}'):
            with self.subTest(raw=raw), self.assertRaises(contract.AdmissionError):
                contract.parse_json_bytes(raw, "fixture")

    def test_native_number_parser_keeps_finite_exponents_and_negatives(self) -> None:
        parsed = contract.parse_json_bytes_native_numbers(
            b'{"tiny":1.23456789012345e-12,"negative":-0.67773218,"integer":2}',
            "metrics",
        )
        self.assertIsInstance(parsed["tiny"], float)
        self.assertLess(parsed["negative"], 0)
        # Regression: the producer verifier never tries to json.dumps Decimal.
        self.assertEqual(
            json.loads(contract.canonical_json_bytes(parsed)),
            parsed,
        )

    def test_canonical_record_is_exclusive_and_hash_bound(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "record.json"
            document = {"schema": "fixture"}
            contract.attach_payload_sha256(document)
            contract.write_exclusive(path, contract.record_bytes(document))
            self.assertEqual(contract.read_canonical_record(path, "fixture"), document)
            with self.assertRaises(contract.AdmissionError):
                contract.write_exclusive(path, b"replacement")

    def test_frozen_model_order_and_timeout(self) -> None:
        self.assertEqual(
            [spec.identity for spec in contract.MODEL_SPECS],
            [
                "raw-common-snapshot",
                "imported-mirror-g0",
                "imported-diverged-g0",
            ],
        )
        self.assertEqual(contract.MODEL_TIMEOUT_SECONDS, 120)
        self.assertEqual(
            contract.ARTIFACT_ROOT_WINDOWS,
            r"D:\mtg-kernel-action-ingress-admission-v1-20260726",
        )
        self.assertEqual(
            contract.TARGET_DIR_WINDOWS,
            r"E:\cargo-target-action-ingress-admission-v1",
        )

    def test_unittest_summary_accepts_lf_and_windows_crlf_only(self) -> None:
        for ending in (b"\n", b"\r\n"):
            stderr = ending.join(
                (
                    b".......",
                    b"----------------------------------------------------------------------",
                    b"Ran 7 tests in 1.000s",
                    b"",
                    b"OK",
                    b"",
                )
            )
            with self.subTest(ending=ending):
                self.assertEqual(
                    contract.unittest_success_count(stderr, "fixture"),
                    7,
                )
        with self.assertRaises(contract.AdmissionError):
            contract.unittest_success_count(
                b"Ran 7 tests in 1.000s\r\nFAILED\r\n",
                "fixture",
            )


if __name__ == "__main__":
    unittest.main()
