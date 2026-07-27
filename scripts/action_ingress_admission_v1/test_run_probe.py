from __future__ import annotations

import copy
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from scripts.action_ingress_admission_v1 import contract, run_probe, test_fixtures


class RunProbeTests(unittest.TestCase):
    def test_exact_envelope_and_all_three_payloads_validate(self) -> None:
        for spec in contract.MODEL_SPECS:
            with self.subTest(role=spec.identity):
                payload = test_fixtures.payload_for(spec)
                stdout, stderr, envelope_raw, payload_raw = test_fixtures.probe_streams(
                    payload
                )
                parsed = run_probe.parse_probe_output(stdout, stderr)
                self.assertEqual(parsed.envelope_raw, envelope_raw)
                self.assertEqual(parsed.payload_raw, payload_raw)
                bindings = run_probe.verify_probe_payload(parsed.payload, spec)
                self.assertEqual(
                    bindings["descriptive_label"],
                    payload["effects"]["descriptive_label"],
                )

    def test_parser_rejects_duplicate_marker_bad_hash_and_stderr_marker(self) -> None:
        payload = test_fixtures.payload_for(contract.MODEL_SPECS[0])
        stdout, _, _, _ = test_fixtures.probe_streams(payload)
        with self.assertRaises(contract.AdmissionError):
            run_probe.parse_probe_output(stdout + stdout, b"")
        corrupted = stdout.replace(b'"payload_sha256":"', b'"payload_sha256":"0', 1)
        with self.assertRaises(contract.AdmissionError):
            run_probe.parse_probe_output(corrupted, b"")
        with self.assertRaises(contract.AdmissionError):
            run_probe.parse_probe_output(stdout, run_probe.MARKER)

    def test_payload_rejects_omission_authority_drift_and_false_invariant(self) -> None:
        spec = contract.MODEL_SPECS[1]
        cases: list[tuple[str, dict]] = []
        omitted = test_fixtures.payload_for(spec)
        omitted.pop("gate")
        cases.append(("omitted-gate", omitted))
        provenance = test_fixtures.payload_for(spec)
        provenance["model"]["provenance"]["run_sha256"] = "0" * 64
        cases.append(("provenance-authority", provenance))
        invariant = test_fixtures.payload_for(spec)
        invariant["admission"]["model_parameters_bit_exact_before_after"] = False
        cases.append(("false-invariant", invariant))
        corpus = test_fixtures.payload_for(spec)
        corpus["corpus"]["decision_count"] = 255
        cases.append(("wrong-corpus-count", corpus))
        float_count = test_fixtures.payload_for(spec)
        float_count["corpus"]["decision_count"] = 256.0
        cases.append(("integral-float-count", float_count))
        generation_bool = test_fixtures.payload_for(spec)
        generation_bool["model"]["generation_index"] = False
        cases.append(("boolean-generation", generation_bool))
        segment_bool = test_fixtures.payload_for(spec)
        segment_bool["model"]["provenance"]["segment_ordinal"] = False
        cases.append(("boolean-segment", segment_bool))
        adam_bool = test_fixtures.payload_for(spec)
        adam_bool["model"]["provenance"]["adam_step"] = False
        cases.append(("boolean-adam-step", adam_bool))
        scaled_gate_integer = test_fixtures.payload_for(spec)
        scaled_gate_integer["transform"]["scaled_gate_scientific_read"] = 0
        cases.append(("integer-transform-boolean", scaled_gate_integer))
        pre_transform_boolean = test_fixtures.payload_for(spec)
        pre_transform_boolean["admission"]["pre_transform_binding"][
            "all_rows_passed"
        ] = 1
        cases.append(("integer-pre-transform-boolean", pre_transform_boolean))
        pre_transform_count = test_fixtures.payload_for(spec)
        pre_transform_count["admission"]["pre_transform_binding"][
            "decision_count"
        ] = 256.0
        cases.append(("float-pre-transform-count", pre_transform_count))
        reference_count = test_fixtures.payload_for(spec)
        reference_count["admission"]["pre_transform_binding"][
            "action_reference_count"
        ] = contract.CORPUS_ACTION_REFERENCE_COUNT - 1
        cases.append(("wrong-reference-count", reference_count))
        binding_hash = test_fixtures.payload_for(spec)
        binding_hash["admission"]["pre_transform_binding"][
            "revalidated_sha256"
        ] = "5" * 64
        cases.append(("binding-hash-mismatch", binding_hash))
        projection_count = test_fixtures.payload_for(spec)
        projection_count["admission"]["pre_transform_binding"][
            "action_object_projection_count"
        ] += 1
        cases.append(("operational-projection-count-mismatch", projection_count))
        exact_forward_count = test_fixtures.payload_for(spec)
        exact_forward_count["admission"][
            "exact_forward_capture_decision_count"
        ] = 256.0
        cases.append(("float-exact-forward-count", exact_forward_count))
        semantic_pair_proof = test_fixtures.payload_for(spec)
        semantic_pair_proof["admission"][
            "semantic_inclusion_pairs_complete_one_to_one"
        ] = False
        cases.append(("false-semantic-pair-proof", semantic_pair_proof))
        duplicate_digest = test_fixtures.payload_for(spec)
        duplicate_digest["admission"]["repaired_zero_ingress_row_digests"][1][
            "sha256"
        ] = duplicate_digest["admission"]["repaired_zero_ingress_row_digests"][0][
            "sha256"
        ]
        cases.append(("duplicate-within-decision-ingress", duplicate_digest))
        wrong_mean = test_fixtures.payload_for(spec)
        wrong_mean["input_statistics"]["mean_direct_squared_norm"] += 1.0
        cases.append(("wrong-input-aggregate", wrong_mean))
        wrong_contribution = test_fixtures.payload_for(spec)
        wrong_contribution["first_layer_contribution_rms"][
            "direct_contribution_rms"
        ] = 999.0
        cases.append(("wrong-contribution-aggregate", wrong_contribution))
        for name, payload in cases:
            with self.subTest(case=name), self.assertRaises(
                contract.AdmissionError
            ):
                run_probe.verify_probe_payload(payload, spec)

    def test_command_receipt_rejects_boolean_exit_and_timeout(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            stdout = root / "stdout"
            stderr = root / "stderr"
            stdout.write_bytes(b"")
            stderr.write_bytes(b"")
            base = {
                "command": ["fixture"],
                "exit_code": 0,
                "stderr": contract.file_record(stderr),
                "stdout": contract.file_record(stdout),
                "timeout_seconds": 120,
                "wall_time_ms": 0,
            }
            for field, value in (("exit_code", False), ("timeout_seconds", True)):
                record = copy.deepcopy(base)
                record[field] = value
                with self.subTest(field=field), self.assertRaises(
                    contract.AdmissionError
                ):
                    run_probe._verify_command_record(
                        record,
                        expected_command=["fixture"],
                        expected_timeout=120,
                        where="fixture receipt",
                        expected_stdout=stdout,
                        expected_stderr=stderr,
                    )

    def test_label_sign_rules_include_exact_zero_as_mixed(self) -> None:
        fields = {
            "mean_jensen_shannon_nats": 1.0,
            "mean_centered_logit_rms_delta": 2.0,
            "top_action_flip_fraction": 0.0,
        }
        self.assertEqual(
            run_probe._expected_label("raw-common-snapshot", fields),
            "RAW-INIT-MIXED",
        )
        fields["top_action_flip_fraction"] = 3.0
        self.assertEqual(
            run_probe._expected_label("raw-common-snapshot", fields),
            "RAW-INIT-DIGEST-DOMINANT",
        )
        negative = {key: -value for key, value in fields.items()}
        self.assertEqual(
            run_probe._expected_label("imported-mirror-g0", negative),
            "IMPORTED-DIRECT-DOMINANT",
        )

    def test_fake_executable_capture_and_timeout(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            capture = run_probe._capture(
                [sys.executable, "-c", "print('fake executable')"],
                repo=root,
                environment={},
                timeout_seconds=1,
            )
            self.assertEqual(capture.exit_code, 0)
            self.assertFalse(capture.timed_out)
            self.assertEqual(capture.stdout.splitlines(), [b"fake executable"])
            timed = run_probe._capture(
                [sys.executable, "-c", "import time; time.sleep(0.2)"],
                repo=root,
                environment={},
                timeout_seconds=0.01,  # type: ignore[arg-type]
            )
            self.assertTrue(timed.timed_out)

    def test_store_snapshot_detects_byte_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / "run.json"
            path.write_bytes(b"one")
            before = run_probe._store_snapshot(root)
            path.write_bytes(b"two")
            after = run_probe._store_snapshot(root)
            self.assertNotEqual(before["aggregate_sha256"], after["aggregate_sha256"])

    def test_store_snapshot_detects_empty_directory_and_rejects_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            before = run_probe._store_snapshot(root)
            empty = root / "empty"
            empty.mkdir()
            after = run_probe._store_snapshot(root)
            self.assertEqual(before["directory_count"], 0)
            self.assertEqual(after["directory_count"], 1)
            self.assertNotEqual(before["aggregate_sha256"], after["aggregate_sha256"])
            empty.rmdir()
            target = root / "target"
            target.write_bytes(b"target")
            link = root / "link"
            try:
                link.symlink_to(target)
            except OSError:
                self.skipTest("symlink creation is unavailable on this platform")
            with self.assertRaises(contract.AdmissionError):
                run_probe._store_snapshot(root)


if __name__ == "__main__":
    unittest.main()
