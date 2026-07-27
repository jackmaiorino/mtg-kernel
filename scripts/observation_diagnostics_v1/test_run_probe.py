from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

try:
    from scripts.observation_diagnostics_v1 import contract, run_probe
except ModuleNotFoundError:
    import contract  # type: ignore[no-redef]
    import run_probe  # type: ignore[no-redef]


ROOT = Path(__file__).resolve().parents[2]


def _checkpoint(role: str, generation: int) -> dict[str, object]:
    return {
        "role": role,
        "generation_index": generation,
        "run_sha256": "1" * 64,
        "checkpoint_manifest_sha256": "2" * 64,
        "checkpoint_payload_sha256": "3" * 64,
        "train_state_sha256": "4" * 64,
        "model_parameter_sha256": "5" * 64,
        "adam_step": generation,
        "identity_bundle_sha256": "c" * 64,
        "segment_ordinal": 0 if generation == 0 else generation // 4,
        "segment_manifest_sha256": "d" * 64,
        "parent_boundary_head_sha256": (
            None if generation == 0 else "e" * 64
        ),
        "boundary_head_sha256": "f" * 64,
        "boundary_head_record_sha256": "0" * 64,
        "checkpoint_sidecar_sha256": "1" * 64,
        "logical_state_sha256": "2" * 64,
        "last_update_evidence_sha256": (
            None if generation == 0 else "3" * 64
        ),
    }


def _functional_model(role: str, generation: int) -> dict[str, object]:
    return {
        "role": role,
        "generation_index": generation,
        "baseline_output_sha256": "6" * 64,
        "repeat_baseline_bit_exact": True,
        "effects": [
            {
                "intervention": intervention,
                "intervention_output_sha256": "7" * 64,
            }
            for intervention in sorted(run_probe.INTERVENTIONS)
        ],
        "hash_minus_direct_contrasts": [],
    }


def synthetic_payload(pair: contract.PairSpec) -> dict[str, object]:
    return {
        "schema": contract.PROBE_PAYLOAD_SCHEMA,
        "label": contract.LABEL,
        "test_identity": contract.PROBE_TEST,
        "run_base_seed": pair.seed,
        "model_architecture_version": "kernel-policy-value-net-8",
        "model_config_fingerprint": (
            run_probe.EXPECTED_MODEL_CONFIG_FINGERPRINT
        ),
        "feature_contract_digest": run_probe.EXPECTED_FEATURE_CONTRACT_DIGEST,
        "feature_encoding_digest": run_probe.EXPECTED_FEATURE_ENCODING_DIGEST,
        "checkpoints": [
            _checkpoint("g0", 0),
            _checkpoint("candidate", pair.candidate_generation),
        ],
        "feature_partition": copy.deepcopy(
            run_probe.EXPECTED_FEATURE_PARTITION
        ),
        "corpus": {
            "identity": (
                "rally-mirror-splitmix64-modulo-fixed-256-"
                "post-tensorization-v1"
            ),
            "digest_identity": (
                "sha256-framed-thirteen-native-flat-tensors-v1"
            ),
            "sha256": (
                "72103ea367a662f76675a044ad4efcf4c52bf86d32630df88e5247cf79f5e5e0"
            ),
            "deck_ids": ["Rally", "Rally"],
            "decision_count": 256,
            "episode_count": 4,
            "decisions_per_episode_cap": 64,
            "multi_action_decision_count": 256,
            "total_action_count": 1115,
            "base_episode_id": 880000,
            "base_environment_seed": 0x6D74_672D_6861_7368,
            "action_selection": (
                "splitmix64-next-modulo-legal-action-count-v1"
            ),
        },
        "permutation": {"integrity_controls": ["fixture"]},
        "ingress_groups": [
            {"name": name} for name in sorted(run_probe.INGRESS_GROUPS)
        ],
        "hash_to_direct_ingress_ratios": [
            {"pathway": "state"},
            {"pathway": "action"},
        ],
        "functional_models": [
            _functional_model("g0", 0),
            _functional_model("candidate", pair.candidate_generation),
        ],
        "candidate_minus_g0_functional_effects": [
            {"intervention": intervention}
            for intervention in sorted(run_probe.INTERVENTIONS)
        ],
        "aggregate_output_stream_sha256": "b" * 64,
        "repeat_aggregate_output_stream_bit_exact": True,
        "output_digest_identity": (
            "sha256-framed-role-condition-decision-logit-value-f32le-v1"
        ),
        "nonclaims": ["fixture is diagnostic only"],
    }


def rust_envelope(payload_raw: bytes) -> bytes:
    digest = hashlib.sha256(payload_raw).hexdigest().encode("ascii")
    return (
        b'{"schema":"'
        + contract.PROBE_ENVELOPE_SCHEMA.encode("ascii")
        + b'","payload_sha256":"'
        + digest
        + b'","payload":'
        + payload_raw
        + b"}"
    )


def probe_stdout(payload_raw: bytes) -> bytes:
    return (
        b"running 1 test\n"
        + run_probe.MARKER
        + rust_envelope(payload_raw)
        + b"\n"
        + b"OBS_RELIANCE_TIMING authority_ms=1 corpus_ms=2 "
        + b"scoring_ms=3 total_ms=7\n"
        + b"test result: ok\n"
    )


def synthetic_build_receipt(
    root: Path,
    *,
    head: str,
) -> tuple[dict[str, object], Path, Path, Path]:
    target_dir = root / "target"
    executable = target_dir / "release" / "deps" / "mtg_kernel-test.exe"
    executable.parent.mkdir(parents=True)
    executable.write_bytes(b"test executable")
    cargo_stdout = root / "cargo.stdout"
    cargo_stderr = root / "cargo.stderr"
    cargo_stdout.write_text(
        json.dumps(
            {
                "executable": str(executable.resolve()),
                "profile": {"test": True},
                "reason": "compiler-artifact",
                "target": {"kind": ["lib"], "name": "mtg_kernel"},
            },
            separators=(",", ":"),
        )
        + "\n"
        + '{"reason":"build-finished","success":true}\n',
        encoding="utf-8",
    )
    cargo_stderr.write_bytes(b"")
    test_stdout = root / "test-list.stdout"
    test_stderr = root / "test-list.stderr"
    test_stdout.write_text(
        "".join(f"{name}: test\n" for name in contract.REQUIRED_TESTS),
        encoding="utf-8",
    )
    test_stderr.write_bytes(b"")
    integrity_stdout = root / "integrity.stdout"
    integrity_stderr = root / "integrity.stderr"
    integrity_statuses = {
        name: ("ignored" if name == contract.PROBE_TEST else "ok")
        for name in contract.REQUIRED_TESTS
    }
    integrity_stdout.write_text(
        "".join(
            f"test {name} ... {status}\n"
            for name, status in integrity_statuses.items()
        ),
        encoding="utf-8",
    )
    integrity_stderr.write_bytes(b"")
    manifest = ROOT / contract.MANIFEST_RELATIVE_PATH
    cargo_lock = ROOT / "Cargo.lock"
    build_source = (
        ROOT / "scripts" / "observation_diagnostics_v1" / "build_probe.py"
    )
    contract_source = (
        ROOT / "scripts" / "observation_diagnostics_v1" / "contract.py"
    )
    audit_report_path = ROOT / contract.STATIC_AUDIT_REPORT_RELATIVE_PATH
    audit_report = contract.read_json_document(audit_report_path)
    audit_check_stdout = root / "audit-check.stdout"
    audit_check_stderr = root / "audit-check.stderr"
    audit_check_stdout.write_text(
        (
            f"{contract.STATIC_AUDIT_POSITIVE_MARKER} "
            f"status={audit_report['status']} "
            f"payload_sha256={audit_report['payload_sha256']}\n"
        ),
        encoding="utf-8",
    )
    audit_check_stderr.write_bytes(b"")
    audit_tests_stdout = root / "audit-tests.stdout"
    audit_tests_stderr = root / "audit-tests.stderr"
    audit_tests_stdout.write_bytes(b"")
    audit_tests_stderr.write_text(
        "Ran 7 tests in 0.125s\n\nOK\n", encoding="utf-8"
    )

    def command_record(
        command: list[str],
        stdout: Path,
        stderr: Path,
        timeout: int,
    ) -> dict[str, object]:
        return {
            "command": command,
            "exit_code": 0,
            "stderr": {
                "path": str(stderr),
                "sha256": contract.sha256_file(stderr),
            },
            "stdout": {
                "path": str(stdout),
                "sha256": contract.sha256_file(stdout),
            },
            "timeout_seconds": timeout,
            "wall_time_ms": 1,
        }

    receipt: dict[str, object] = {
        "schema": contract.BUILD_RECEIPT_SCHEMA,
        "label": contract.LABEL,
        "started_utc": "2026-07-26T00:00:00+00:00",
        "completed_utc": "2026-07-26T00:00:01+00:00",
        "git_head": head,
        "git_status_clean_before_and_after": True,
        "manifest": {
            "path": str(manifest),
            "sha256": contract.sha256_file(manifest),
        },
        "cargo_lock": {
            "path": str(cargo_lock),
            "sha256": contract.sha256_file(cargo_lock),
        },
        "build_source": {
            "path": str(build_source),
            "sha256": contract.sha256_file(build_source),
        },
        "contract_source": {
            "path": str(contract_source),
            "sha256": contract.sha256_file(contract_source),
        },
        "cargo": {
            "command": contract.cargo_build_command(),
            "environment": {"CARGO_TARGET_DIR": str(target_dir)},
            "exit_code": 0,
            "locked": True,
            "no_default_features": True,
            "release": True,
            "requested_features": [],
            "stderr": {
                "path": str(cargo_stderr),
                "sha256": contract.sha256_file(cargo_stderr),
            },
            "stdout": {
                "path": str(cargo_stdout),
                "sha256": contract.sha256_file(cargo_stdout),
            },
            "target_dir": str(target_dir),
            "wall_time_ms": 1000,
        },
        "executable": {
            "compiler_artifact_target_kind": ["lib"],
            "path": str(executable),
            "sha256": contract.sha256_file(executable),
        },
        "integrity_tests": {
            **command_record(
                contract.integrity_test_command(executable.resolve()),
                integrity_stdout,
                integrity_stderr,
                300,
            ),
            "executed_test_statuses": integrity_statuses,
        },
        "required_tests": list(contract.REQUIRED_TESTS),
        "static_audit": {
            "check": command_record(
                contract.static_audit_check_command(),
                audit_check_stdout,
                audit_check_stderr,
                60,
            ),
            "report": {
                "path": str(audit_report_path),
                "payload_sha256": audit_report["payload_sha256"],
                "schema": audit_report["schema"],
                "sha256": contract.sha256_file(audit_report_path),
                "status": audit_report["status"],
            },
            "tests": command_record(
                contract.static_audit_test_command(),
                audit_tests_stdout,
                audit_tests_stderr,
                60,
            )
            | {"executed_test_count": contract.STATIC_AUDIT_REQUIRED_TEST_COUNT},
        },
        "test_list": {
            "command": [str(executable.resolve()), "--list"],
            "exit_code": 0,
            "listed_test_count": len(contract.REQUIRED_TESTS),
            "stderr": {
                "path": str(test_stderr),
                "sha256": contract.sha256_file(test_stderr),
            },
            "stdout": {
                "path": str(test_stdout),
                "sha256": contract.sha256_file(test_stdout),
            },
            "timeout_seconds": 60,
            "wall_time_ms": 1,
        },
    }
    contract.attach_payload_sha256(receipt)
    receipt_path = root / "build-receipt.json"
    receipt_path.write_bytes(contract.record_bytes(receipt))
    return receipt, receipt_path, executable, manifest


class FixedPairContractTests(unittest.TestCase):
    def test_six_store_generation_pairs_match_manifest(self) -> None:
        self.assertEqual(
            [
                (
                    pair.arm,
                    pair.seed,
                    pair.store_root,
                    pair.candidate_generation,
                )
                for pair in contract.PAIR_SPECS
            ],
            [
                (
                    "mirror-start",
                    920013,
                    (
                        r"D:\mtg-kernel-exploiter-v3b-20260726"
                        r"\runs-arm1\dev0\run-0\store"
                    ),
                    256,
                ),
                (
                    "mirror-start",
                    920014,
                    (
                        r"D:\mtg-kernel-exploiter-v3b-20260726"
                        r"\runs-arm1\dev0\run-1\store"
                    ),
                    384,
                ),
                (
                    "mirror-start",
                    920015,
                    (
                        r"D:\mtg-kernel-exploiter-v3b-20260726"
                        r"\runs-arm1\dev0\run-2\store"
                    ),
                    256,
                ),
                (
                    "diverged-start",
                    920016,
                    (
                        r"D:\mtg-kernel-exploiter-v3b-20260726"
                        r"\runs-arm2\dev0\run-0\store"
                    ),
                    256,
                ),
                (
                    "diverged-start",
                    920017,
                    (
                        r"D:\mtg-kernel-exploiter-v3b-20260726"
                        r"\runs-arm2\dev0\run-1\store"
                    ),
                    512,
                ),
                (
                    "diverged-start",
                    920018,
                    (
                        r"D:\mtg-kernel-exploiter-v3b-20260726"
                        r"\runs-arm2\dev0\run-2\store"
                    ),
                    128,
                ),
            ],
        )


class ProbeOutputParsingTests(unittest.TestCase):
    def test_preserves_and_verifies_exact_raw_nested_payload(self) -> None:
        pair = contract.PAIR_SPECS[0]
        payload_raw = json.dumps(
            synthetic_payload(pair),
            separators=(",", ":"),
            ensure_ascii=False,
        ).encode("utf-8")
        parsed = run_probe.parse_probe_output(probe_stdout(payload_raw), b"")
        self.assertEqual(parsed.payload_raw, payload_raw)
        self.assertEqual(
            parsed.envelope["payload_sha256"],
            hashlib.sha256(payload_raw).hexdigest(),
        )
        identity = run_probe.verify_probe_payload(parsed.payload, pair)
        self.assertEqual(
            identity["candidate_checkpoint"]["generation_index"],
            pair.candidate_generation,
        )
        self.assertEqual(parsed.timing["total"], 7)

    def test_duplicate_marker_or_nonfollowing_timing_is_rejected(self) -> None:
        payload_raw = b'{"fixture":true}'
        valid = probe_stdout(payload_raw)
        with self.assertRaisesRegex(
            contract.DiagnosticError, "exactly one"
        ):
            run_probe.parse_probe_output(valid + valid, b"")
        moved = valid.replace(
            b"\nOBS_RELIANCE_TIMING ",
            b"\nintervening output\nOBS_RELIANCE_TIMING ",
        )
        with self.assertRaisesRegex(
            contract.DiagnosticError, "immediately follow"
        ):
            run_probe.parse_probe_output(moved, b"")

    def test_duplicate_key_nonfinite_and_raw_hash_mismatch_are_rejected(self) -> None:
        for payload_raw in (
            b'{"schema":"a","schema":"b"}',
            b'{"metric":NaN}',
        ):
            with self.assertRaises(contract.DiagnosticError):
                run_probe.parse_probe_output(probe_stdout(payload_raw), b"")

        payload_raw = b'{"fixture":true}'
        envelope = rust_envelope(payload_raw)
        tampered = envelope.replace(b'"fixture":true', b'"fixture":false')
        stdout = (
            run_probe.MARKER
            + tampered
            + b"\nOBS_RELIANCE_TIMING authority_ms=1 corpus_ms=1 "
            + b"scoring_ms=1 total_ms=3\n"
        )
        with self.assertRaisesRegex(
            contract.DiagnosticError, "payload SHA-256 mismatch"
        ):
            run_probe.parse_probe_output(stdout, b"")

    def test_wrong_candidate_generation_is_rejected(self) -> None:
        pair = contract.PAIR_SPECS[0]
        payload = synthetic_payload(pair)
        payload["checkpoints"][1]["generation_index"] = 999  # type: ignore[index]
        with self.assertRaisesRegex(
            contract.DiagnosticError, "generation_index mismatch"
        ):
            run_probe.verify_probe_payload(payload, pair)

    def test_wrong_store_base_seed_is_rejected(self) -> None:
        pair = contract.PAIR_SPECS[0]
        payload = synthetic_payload(pair)
        payload["run_base_seed"] = pair.seed + 1
        with self.assertRaisesRegex(
            contract.DiagnosticError, "run_base_seed mismatch"
        ):
            run_probe.verify_probe_payload(payload, pair)

    def test_checkpoint_boundary_provenance_nullability_is_fail_closed(self) -> None:
        pair = contract.PAIR_SPECS[0]
        payload = synthetic_payload(pair)
        payload["checkpoints"][0]["parent_boundary_head_sha256"] = "f" * 64  # type: ignore[index]
        with self.assertRaisesRegex(
            contract.DiagnosticError, "must be null for genesis"
        ):
            run_probe.verify_probe_payload(payload, pair)

        payload = synthetic_payload(pair)
        payload["checkpoints"][1]["last_update_evidence_sha256"] = None  # type: ignore[index]
        with self.assertRaisesRegex(
            contract.DiagnosticError, "must be lower-case SHA-256"
        ):
            run_probe.verify_probe_payload(payload, pair)

    def test_repeat_aggregate_output_stream_must_be_bit_exact(self) -> None:
        pair = contract.PAIR_SPECS[0]
        payload = synthetic_payload(pair)
        payload["repeat_aggregate_output_stream_bit_exact"] = False
        with self.assertRaisesRegex(
            contract.DiagnosticError,
            "repeat_aggregate_output_stream_bit_exact must be True",
        ):
            run_probe.verify_probe_payload(payload, pair)


class BuildReceiptValidationTests(unittest.TestCase):
    def test_valid_receipt_binds_head_manifest_lock_executable_and_tests(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            receipt, receipt_path, executable, manifest = synthetic_build_receipt(
                root, head="a" * 40
            )
            resolved, binding = run_probe.verify_build_receipt(
                receipt,
                receipt_path=receipt_path,
                repo=ROOT,
                manifest=manifest,
                head="a" * 40,
                require_frozen_target_dir=False,
            )
            self.assertEqual(resolved, executable.resolve())
            self.assertEqual(
                binding["sha256"], contract.sha256_file(receipt_path)
            )

            receipt["cargo"]["requested_features"] = ["cuda"]  # type: ignore[index]
            receipt["payload_sha256"] = contract.payload_sha256(receipt)
            receipt_path.write_bytes(contract.record_bytes(receipt))
            with self.assertRaisesRegex(
                contract.DiagnosticError, "no requested Cargo features"
            ):
                run_probe.verify_build_receipt(
                    receipt,
                    receipt_path=receipt_path,
                    repo=ROOT,
                    manifest=manifest,
                    head="a" * 40,
                    require_frozen_target_dir=False,
                )

    def test_receipt_executable_is_derived_from_cargo_json_and_target(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            receipt, receipt_path, executable, manifest = synthetic_build_receipt(
                root, head="a" * 40
            )
            replacement = executable.with_name("replacement-test.exe")
            replacement.write_bytes(b"replacement executable")
            receipt["executable"] = {
                "compiler_artifact_target_kind": ["lib"],
                "path": str(replacement),
                "sha256": contract.sha256_file(replacement),
            }
            receipt["payload_sha256"] = contract.payload_sha256(receipt)
            receipt_path.write_bytes(contract.record_bytes(receipt))
            with self.assertRaisesRegex(
                contract.DiagnosticError, "differs from the sole Cargo JSON"
            ):
                run_probe.verify_build_receipt(
                    receipt,
                    receipt_path=receipt_path,
                    repo=ROOT,
                    manifest=manifest,
                    head="a" * 40,
                    require_frozen_target_dir=False,
                )

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            receipt, receipt_path, _, manifest = synthetic_build_receipt(
                root, head="a" * 40
            )
            other_target = root / "other-target"
            receipt["cargo"]["target_dir"] = str(other_target)  # type: ignore[index]
            receipt["cargo"]["environment"] = {  # type: ignore[index]
                "CARGO_TARGET_DIR": str(other_target)
            }
            receipt["payload_sha256"] = contract.payload_sha256(receipt)
            receipt_path.write_bytes(contract.record_bytes(receipt))
            with self.assertRaisesRegex(
                contract.DiagnosticError, "must be a descendant"
            ):
                run_probe.verify_build_receipt(
                    receipt,
                    receipt_path=receipt_path,
                    repo=ROOT,
                    manifest=manifest,
                    head="a" * 40,
                    require_frozen_target_dir=False,
                )

    def test_official_receipt_pins_target_and_environment(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            receipt, receipt_path, _, manifest = synthetic_build_receipt(
                root, head="a" * 40
            )
            with self.assertRaisesRegex(
                contract.DiagnosticError, "Cargo target directory"
            ):
                run_probe.verify_build_receipt(
                    receipt,
                    receipt_path=receipt_path,
                    repo=ROOT,
                    manifest=manifest,
                    head="a" * 40,
                )

            receipt["cargo"]["target_dir"] = contract.TARGET_DIR_WINDOWS  # type: ignore[index]
            receipt["cargo"]["environment"] = {  # type: ignore[index]
                "CARGO_TARGET_DIR": contract.TARGET_DIR_WINDOWS
            }
            receipt["payload_sha256"] = contract.payload_sha256(receipt)
            receipt_path.write_bytes(contract.record_bytes(receipt))
            with self.assertRaisesRegex(
                contract.DiagnosticError, "must be a descendant"
            ):
                run_probe.verify_build_receipt(
                    receipt,
                    receipt_path=receipt_path,
                    repo=ROOT,
                    manifest=manifest,
                    head="a" * 40,
                )

            receipt["cargo"]["environment"] = {  # type: ignore[index]
                "CARGO_TARGET_DIR": r"X:\override-target"
            }
            receipt["payload_sha256"] = contract.payload_sha256(receipt)
            receipt_path.write_bytes(contract.record_bytes(receipt))
            with self.assertRaisesRegex(
                contract.DiagnosticError, "CARGO_TARGET_DIR binding mismatch"
            ):
                run_probe.verify_build_receipt(
                    receipt,
                    receipt_path=receipt_path,
                    repo=ROOT,
                    manifest=manifest,
                    head="a" * 40,
                )


class ProcessAndOutputAdmissionTests(unittest.TestCase):
    def test_cpu_environment_binds_store_generation_and_expected_seed(self) -> None:
        pair = contract.PAIR_SPECS[4]
        _, bindings = run_probe._probe_environment(pair)
        self.assertEqual(
            bindings,
            {
                "CUDA_VISIBLE_DEVICES": "",
                "OBS_RELIANCE_CANDIDATE_GEN": "512",
                "OBS_RELIANCE_EXPECTED_BASE_SEED": "920017",
                "OBS_RELIANCE_STORE_ROOT": pair.store_root,
            },
        )

    def test_timeout_is_captured_without_a_success_exit_code(self) -> None:
        capture = run_probe.run_command_capture(
            [
                sys.executable,
                "-c",
                "import time; time.sleep(1)",
            ],
            cwd=ROOT,
            environment={},
            timeout_seconds=0.02,
        )
        self.assertTrue(capture.timed_out)
        self.assertIsNone(capture.exit_code)

    def test_existing_runs_output_is_refused_before_execution(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact_root = Path(directory)
            (artifact_root / "runs").mkdir()
            with self.assertRaisesRegex(
                contract.DiagnosticError, "existing runs output"
            ):
                run_probe.run_all(
                    repo=ROOT,
                    artifact_root=artifact_root,
                    manifest=ROOT / contract.MANIFEST_RELATIVE_PATH,
                    build_receipt_path=artifact_root / "missing.json",
                    require_windows=False,
                )

    def test_existing_completion_output_is_refused_before_execution(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact_root = Path(directory)
            (artifact_root / "completion-receipt.json").write_bytes(b"reserved")
            with self.assertRaisesRegex(
                contract.DiagnosticError, "existing completion output"
            ):
                run_probe.run_all(
                    repo=ROOT,
                    artifact_root=artifact_root,
                    manifest=ROOT / contract.MANIFEST_RELATIVE_PATH,
                    build_receipt_path=artifact_root / "missing.json",
                    require_windows=False,
                )

    def test_official_runner_rejects_artifact_and_receipt_overrides(self) -> None:
        manifest = ROOT / contract.MANIFEST_RELATIVE_PATH
        with self.assertRaisesRegex(
            contract.DiagnosticError, "official artifact root"
        ):
            run_probe.run_all(
                repo=ROOT,
                artifact_root=Path(r"X:\override-artifacts"),
                manifest=manifest,
                build_receipt_path=Path(contract.BUILD_RECEIPT_WINDOWS),
                require_windows=True,
            )
        with self.assertRaisesRegex(
            contract.DiagnosticError, "official build receipt"
        ):
            run_probe.run_all(
                repo=ROOT,
                artifact_root=Path(contract.ARTIFACT_ROOT_WINDOWS),
                manifest=manifest,
                build_receipt_path=Path(r"X:\override-build-receipt.json"),
                require_windows=True,
            )


if __name__ == "__main__":
    unittest.main()
