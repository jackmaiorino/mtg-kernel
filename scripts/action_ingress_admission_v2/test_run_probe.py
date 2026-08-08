from __future__ import annotations

import ast
import copy
import inspect
import math
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest

from scripts.action_ingress_admission_v2 import contract, run_probe, test_fixtures


class RunProbeTests(unittest.TestCase):
    def test_ordered_f64_reconstruction_has_frozen_rust_semantics(self) -> None:
        positive_adversarial = [1.0, 2.0**-53, 2.0**-53]
        self.assertEqual(
            run_probe._ordered_f64_sum(positive_adversarial).hex(),
            "0x1.0000000000000p+0",
        )
        self.assertEqual(math.fsum(positive_adversarial).hex(), "0x1.0000000000001p+0")

        verifier_tree = ast.parse(
            textwrap.dedent(inspect.getsource(run_probe.verify_probe_payload))
        )
        builtin_sum_calls = [
            node
            for node in ast.walk(verifier_tree)
            if isinstance(node, ast.Call)
            and isinstance(node.func, ast.Name)
            and node.func.id == "sum"
        ]
        power_operations = [
            node
            for node in ast.walk(verifier_tree)
            if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Pow)
        ]
        self.assertEqual(builtin_sum_calls, [])
        self.assertEqual(power_operations, [])

    def test_input_aggregation_accepts_rust_order_and_rejects_compensation(
        self,
    ) -> None:
        spec = contract.MODEL_SPECS[0]
        payload = test_fixtures.payload_for(spec)
        rows = payload["input_statistics"]["per_action_row"]
        norms = [1.0, 2.0**-53, 2.0**-53] + [0.0] * (len(rows) - 3)
        for row, norm in zip(rows, norms, strict=True):
            row["direct_squared_norm"] = norm
        ordered = run_probe._ordered_f64_sum(norms)
        payload["input_statistics"]["direct_value_rms"] = math.sqrt(
            ordered / (1_115 * 99)
        )
        payload["input_statistics"]["mean_direct_squared_norm"] = ordered / 1_115
        run_probe.verify_probe_payload(payload, spec)

        compensated_payload = copy.deepcopy(payload)
        compensated = math.fsum(norms)
        self.assertNotEqual(ordered, compensated)
        compensated_payload["input_statistics"]["direct_value_rms"] = math.sqrt(
            compensated / (1_115 * 99)
        )
        compensated_payload["input_statistics"][
            "mean_direct_squared_norm"
        ] = compensated / 1_115
        with self.assertRaises(contract.AdmissionError):
            run_probe.verify_probe_payload(compensated_payload, spec)

    def test_contribution_raw_norms_are_exact_and_ordered(self) -> None:
        spec = contract.MODEL_SPECS[0]
        payload = test_fixtures.payload_for(spec)
        rows = payload["first_layer_contribution_rms"]["per_action_row"]
        squared_norms = [1.0] + [2.0**-54] * (len(rows) - 1)
        for row, squared_norm in zip(rows, squared_norms, strict=True):
            row["direct_contribution_squared_norm"] = squared_norm
            row["direct_contribution_rms"] = math.sqrt(squared_norm / 64)
        ordered = run_probe._ordered_f64_sum(squared_norms)
        ordered_rms = math.sqrt(ordered / (1_115 * 64))
        self.assertEqual(ordered_rms.hex(), "0x1.eaa97ed1aa905p-9")
        payload["first_layer_contribution_rms"][
            "direct_contribution_rms"
        ] = ordered_rms
        run_probe.verify_probe_payload(payload, spec)

        compensated_payload = copy.deepcopy(payload)
        compensated_rms = math.sqrt(math.fsum(squared_norms) / (1_115 * 64))
        self.assertEqual(compensated_rms.hex(), "0x1.eaa97ed1aaa0fp-9")
        compensated_payload["first_layer_contribution_rms"][
            "direct_contribution_rms"
        ] = compensated_rms
        with self.assertRaises(contract.AdmissionError):
            run_probe.verify_probe_payload(compensated_payload, spec)

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
        preserved_root = (
            Path(contract.PRESERVED_V1_ARTIFACT_ROOT_WINDOWS)
            if sys.platform == "win32"
            else Path("/mnt/d/mtg-kernel-action-ingress-admission-v1-20260726")
        )
        preserved = run_probe._parse_probe_output(
            (
                preserved_root / run_probe.PRESERVED_V1_RAW_STDOUT_RELATIVE_PATH
            ).read_bytes(),
            (
                preserved_root / run_probe.PRESERVED_V1_RAW_STDERR_RELATIVE_PATH
            ).read_bytes(),
            marker=run_probe.PRESERVED_V1_MARKER,
            test_identity=run_probe.PRESERVED_V1_PROBE_TEST,
            envelope_schema=run_probe.PRESERVED_V1_ENVELOPE_SCHEMA,
            where="preserved v1 fixture",
        )
        projection = run_probe._canonical_retry_projection(
            preserved.payload, is_v2=False
        )
        self.assertEqual(len(projection), run_probe.RETRY_PROJECTION_BYTE_COUNT)
        self.assertEqual(
            contract.sha256_bytes(projection),
            run_probe.RETRY_PROJECTION_SHA256,
        )
        v2_payload = copy.deepcopy(preserved.payload)
        v2_payload["schema"] = contract.PROBE_PAYLOAD_SCHEMA
        v2_payload["label"] = contract.LABEL
        v2_payload["test_identity"] = contract.PROBE_TEST
        for row in v2_payload["first_layer_contribution_rms"]["per_action_row"]:
            row["direct_contribution_squared_norm"] = 0.0
            row["digest_contribution_squared_norm"] = 0.0
        self.assertEqual(
            run_probe._canonical_retry_projection(v2_payload, is_v2=True),
            projection,
        )
        v2_payload["corpus"]["decision_count"] += 1
        self.assertNotEqual(
            run_probe._canonical_retry_projection(v2_payload, is_v2=True),
            projection,
        )

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
        missing_contribution_norm = test_fixtures.payload_for(spec)
        missing_contribution_norm["first_layer_contribution_rms"]["per_action_row"][
            0
        ].pop("direct_contribution_squared_norm")
        cases.append(("missing-contribution-squared-norm", missing_contribution_norm))
        wrong_contribution_norm = test_fixtures.payload_for(spec)
        wrong_contribution_norm["first_layer_contribution_rms"]["per_action_row"][0][
            "direct_contribution_squared_norm"
        ] += 1.0
        cases.append(("wrong-contribution-squared-norm", wrong_contribution_norm))
        wrong_contribution_row_rms = test_fixtures.payload_for(spec)
        wrong_contribution_row_rms["first_layer_contribution_rms"]["per_action_row"][
            0
        ]["direct_contribution_rms"] += 1.0
        cases.append(("wrong-contribution-row-rms", wrong_contribution_row_rms))
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
            linked = root / "linked-stdout"
            try:
                linked.symlink_to(stdout)
            except OSError:
                pass
            else:
                with self.assertRaises(contract.AdmissionError):
                    run_probe._verify_file_binding(
                        {
                            "byte_count": 0,
                            "path": str(linked),
                            "sha256": contract.sha256_file(stdout),
                        },
                        expected_path=stdout,
                        where="linked fixture",
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
