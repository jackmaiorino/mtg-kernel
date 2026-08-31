from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from scripts.action_ingress_admission_v2 import contract


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
            r"D:\mtg-kernel-action-ingress-admission-v2-20260727",
        )
        self.assertEqual(
            contract.TARGET_DIR_WINDOWS,
            r"E:\cargo-target-action-ingress-admission-v2-20260727",
        )
        self.assertEqual(
            contract.OFFICIAL_WINDOWS_PYTHON,
            r"D:\mtg-kernel-clean-venv-019f63a2\Scripts\python.exe",
        )
        self.assertEqual(
            contract.OFFICIAL_WINDOWS_PYTHON_VERSION,
            "3.13.14 (main, Jun 23 2026, 15:19:27) "
            "[MSC v.1944 64 bit (AMD64)]",
        )
        self.assertEqual(
            contract.PRESERVED_V1_TREE_EXPECTATIONS["artifact_root"],
            {
                "aggregate_sha256": (
                    "5488ffd74443833a28c44d63ebea0be27a770684caf78f0be41f57d48a248bc6"
                ),
                "directory_count": 3,
                "file_count": 25,
                "total_byte_count": 584_131,
            },
        )
        self.assertEqual(
            contract.PRESERVED_V1_TREE_EXPECTATIONS["cargo_target"],
            {
                "aggregate_sha256": (
                    "0c1680d0b4c72f4dd7e4b8b739f30fba3f0acd732e949d9fa90d08aef8312aa0"
                ),
                "directory_count": 118,
                "file_count": 560,
                "total_byte_count": 191_861_359,
            },
        )
        self.assertEqual(len(contract.V2_PACKAGE_FILENAMES), 12)
        self.assertEqual(len(contract.LICENSED_RETRY_DIFF), 15)
        name_status = "".join(
            f"{status}\t{path}\n"
            for status, path in contract.LICENSED_RETRY_DIFF
        ).encode()
        base_parent = b"fixture\nmod action_ingress_admission_v1;\n"
        manifest_bytes = (
            Path(__file__).resolve().parents[2] / contract.MANIFEST_RELATIVE_PATH
        ).read_bytes()
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            manifest_path = repo / contract.MANIFEST_RELATIVE_PATH
            manifest_path.parent.mkdir(parents=True)
            manifest_path.write_bytes(manifest_bytes)
            parent = repo / contract.PARENT_PROBE_RELATIVE_PATH
            parent.parent.mkdir(parents=True)
            parent.write_bytes(
                base_parent + b"mod action_ingress_admission_v2;\n"
            )

            def fake_git_bytes(_repo: Path, *arguments: str) -> bytes:
                if arguments[0] == "diff":
                    return name_status
                if arguments[0] == "show" and ":" in arguments[-1]:
                    return (
                        manifest_bytes
                        if arguments[-1].endswith(
                            f":{contract.V2_MANIFEST_GIT_PATH.as_posix()}"
                        )
                        else base_parent
                    )
                raise AssertionError(arguments)

            def fake_git(_repo: Path, *arguments: str) -> str:
                if arguments[0] == "show":
                    return f"A\t{contract.V2_MANIFEST_GIT_PATH.as_posix()}"
                if arguments[:2] == ("merge-base", "--is-ancestor"):
                    return ""
                raise AssertionError(arguments)

            with mock.patch.object(
                contract, "checked_git_bytes", side_effect=fake_git_bytes
            ), mock.patch.object(contract, "checked_git", side_effect=fake_git):
                record = contract.verify_licensed_retry_diff(repo)
                self.assertEqual(
                    record["base_commit"], contract.RETRY_DIFF_BASE_COMMIT
                )
                parent.write_bytes(
                    base_parent
                    + b"mod action_ingress_admission_v2;\n"
                    + b"mod action_ingress_admission_v2;\n"
                )
                with self.assertRaises(contract.AdmissionError):
                    contract.verify_licensed_retry_diff(repo)

    def test_relocated_v1_contract_preserves_historical_hash(self) -> None:
        relative = "scripts/action_ingress_admission_v1/contract.py"
        root = Path(__file__).resolve().parents[2]
        historical = contract.FROZEN_INPUTS[relative]
        current = contract.CURRENT_FROZEN_INPUT_HASH_OVERRIDES[relative]
        self.assertNotEqual(current, historical)
        self.assertEqual(contract.sha256_file(root / relative), current)

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
