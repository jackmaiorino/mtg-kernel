from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

try:
    from scripts.observation_diagnostics_v1 import build_probe, contract
except ModuleNotFoundError:
    import build_probe  # type: ignore[no-redef]
    import contract  # type: ignore[no-redef]


class FrozenBuildContractTests(unittest.TestCase):
    def test_cargo_command_is_locked_release_cpu_only_lib_test_build(self) -> None:
        self.assertEqual(
            contract.cargo_build_command(),
            [
                "cargo",
                "test",
                "--release",
                "--locked",
                "-p",
                "mtg-kernel",
                "--lib",
                "--no-run",
                "--no-default-features",
                "--message-format=json",
            ],
        )
        self.assertNotIn("--features", contract.cargo_build_command())

    def test_required_tests_pin_five_controls_and_ignored_probe(self) -> None:
        self.assertEqual(len(contract.REQUIRED_TESTS), 6)
        self.assertEqual(
            contract.REQUIRED_TESTS[-1],
            (
                "native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::"
                "trained_checkpoint_hash_vs_direct_reliance_probe_v1"
            ),
        )
        self.assertIn(
            (
                "native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::"
                "fixed_rally_corpus_is_repeatable_without_external_artifacts_v1"
            ),
            contract.REQUIRED_TESTS,
        )

    def test_preflight_commands_are_single_threaded_and_bind_static_audit(self) -> None:
        executable = Path(r"C:\fixture\mtg_kernel-test.exe")
        self.assertEqual(
            contract.integrity_test_command(executable),
            [
                str(executable),
                contract.TEST_MODULE,
                "--nocapture",
                "--test-threads=1",
            ],
        )
        self.assertEqual(
            contract.static_audit_check_command()[-2:],
            [
                contract.STATIC_AUDIT_TOOL_RELATIVE_PATH.as_posix(),
                "--check",
            ],
        )
        self.assertIn(
            contract.STATIC_AUDIT_TEST_RELATIVE_PATH.name,
            contract.static_audit_test_command(),
        )

    def test_integrity_status_parser_separates_ok_and_ignored(self) -> None:
        stdout = (
            f"test {contract.REQUIRED_TESTS[0]} ... ok\n"
            f"test {contract.PROBE_TEST} ... ignored, external Store diagnostic\n"
        ).encode("utf-8")
        self.assertEqual(
            build_probe._executed_test_statuses(stdout),
            {
                contract.REQUIRED_TESTS[0]: "ok",
                contract.PROBE_TEST: "ignored",
            },
        )

    def test_static_audit_unittest_summary_is_nonempty_and_exact(self) -> None:
        self.assertEqual(
            contract.unittest_success_count(
                b".......\nRan 7 tests in 0.125s\n\nOK\n",
                "fixture",
            ),
            7,
        )
        self.assertEqual(
            contract.unittest_success_count(
                b".......\r\nRan 7 tests in 0.125s\r\n\r\nOK\r\n",
                "Windows fixture",
            ),
            7,
        )
        for invalid in (
            b"Ran 0 tests in 0.000s\n\nOK\n",
            b"Ran 7 tests in 0.125s\n\nFAILED (failures=1)\n",
        ):
            with self.assertRaises(contract.DiagnosticError):
                contract.unittest_success_count(invalid, "fixture")

    def test_official_build_rejects_target_and_artifact_overrides(self) -> None:
        manifest = Path("repo") / contract.MANIFEST_RELATIVE_PATH
        with self.assertRaisesRegex(
            contract.DiagnosticError, "official Cargo target directory"
        ):
            build_probe.build(
                repo=Path("repo"),
                target_dir=Path(r"X:\override-target"),
                artifact_root=Path(contract.ARTIFACT_ROOT_WINDOWS),
                manifest=manifest,
                require_windows=True,
            )
        with self.assertRaisesRegex(
            contract.DiagnosticError, "official artifact root"
        ):
            build_probe.build(
                repo=Path("repo"),
                target_dir=Path(contract.TARGET_DIR_WINDOWS),
                artifact_root=Path(r"X:\override-artifacts"),
                manifest=manifest,
                require_windows=True,
            )

    def test_official_build_reserves_an_entirely_absent_artifact_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact_root = Path(directory) / "official-artifacts"
            build_root = build_probe._reserve_build_root(
                artifact_root, official=True
            )
            self.assertEqual(build_root, artifact_root / "build")
            self.assertTrue(build_root.is_dir())
            with self.assertRaisesRegex(
                contract.DiagnosticError, "existing official artifact root"
            ):
                build_probe._reserve_build_root(artifact_root, official=True)


class CargoArtifactParsingTests(unittest.TestCase):
    def test_resolves_exactly_one_mtg_kernel_lib_test_executable(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "mtg_kernel-test.exe"
            executable.write_bytes(b"fixture")
            messages = [
                {
                    "reason": "compiler-artifact",
                    "profile": {"test": True},
                    "target": {"kind": ["lib"], "name": "mtg_kernel"},
                    "executable": str(executable),
                },
                {
                    "reason": "compiler-artifact",
                    "profile": {"test": True},
                    "target": {"kind": ["bin"], "name": "other"},
                    "executable": str(Path(directory) / "other.exe"),
                },
            ]
            self.assertEqual(
                contract.resolve_lib_test_executable(messages),
                executable,
            )

    def test_rejects_ambiguous_lib_test_executables(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory) / f"test-{index}.exe" for index in range(2)]
            for path in paths:
                path.write_bytes(b"fixture")
            messages = [
                {
                    "reason": "compiler-artifact",
                    "profile": {"test": True},
                    "target": {"kind": ["lib"], "name": "mtg_kernel"},
                    "executable": str(path),
                }
                for path in paths
            ]
            with self.assertRaisesRegex(
                contract.DiagnosticError, "exactly one"
            ):
                contract.resolve_lib_test_executable(messages)

    def test_rejects_malformed_compiler_artifact_shape(self) -> None:
        with self.assertRaisesRegex(
            contract.DiagnosticError, "profile/target must be objects"
        ):
            contract.resolve_lib_test_executable(
                [
                    {
                        "reason": "compiler-artifact",
                        "profile": None,
                        "target": None,
                    }
                ]
            )

    def test_cargo_json_lines_are_strict(self) -> None:
        messages = build_probe._parse_cargo_messages(
            [
                b'{"reason":"build-finished","success":true}\n',
                b"\n",
            ]
        )
        self.assertEqual(messages[0]["reason"], "build-finished")
        with self.assertRaises(contract.DiagnosticError):
            build_probe._parse_cargo_messages(
                [b'{"reason":"a","reason":"b"}\n']
            )


class ListedTestsParsingTests(unittest.TestCase):
    def test_preserves_full_rust_module_paths(self) -> None:
        listed = "\n".join(
            f"{name}: test" for name in contract.REQUIRED_TESTS
        )
        self.assertEqual(
            contract.listed_test_names(listed),
            set(contract.REQUIRED_TESTS),
        )


if __name__ == "__main__":
    unittest.main()
