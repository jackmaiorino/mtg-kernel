from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import unittest

from scripts.action_ingress_admission_v1 import build_probe, contract, run_probe


class BuildProbeTests(unittest.TestCase):
    def test_cargo_message_resolution_uses_only_lib_test(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            executable = Path(temporary) / "mtg_kernel.exe"
            executable.write_bytes(b"fake")
            messages = [
                {
                    "reason": "compiler-artifact",
                    "profile": {"test": False},
                    "target": {"kind": ["lib"], "name": "mtg_kernel"},
                    "executable": str(executable),
                },
                {
                    "reason": "compiler-artifact",
                    "profile": {"test": True},
                    "target": {"kind": ["lib"], "name": "mtg_kernel"},
                    "executable": str(executable),
                },
            ]
            self.assertEqual(
                contract.resolve_lib_test_executable(messages), executable
            )
            second = Path(temporary) / "other.exe"
            second.write_bytes(b"other")
            duplicate = dict(messages[-1])
            duplicate["executable"] = str(second)
            with self.assertRaises(contract.AdmissionError):
                contract.resolve_lib_test_executable(messages + [duplicate])

    def test_fake_command_is_receipted_and_nonzero_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            record, stdout, stderr = build_probe._run_recorded(
                [sys.executable, "-c", "print('ok')"],
                repo=root,
                stdout_path=root / "stdout",
                stderr_path=root / "stderr",
                timeout_seconds=2,
            )
            self.assertEqual(record["exit_code"], 0)
            self.assertEqual(stdout.splitlines(), [b"ok"])
            self.assertEqual(stderr, b"")
            with self.assertRaises(contract.AdmissionError):
                build_probe._run_recorded(
                    [sys.executable, "-c", "raise SystemExit(7)"],
                    repo=root,
                    stdout_path=root / "bad.stdout",
                    stderr_path=root / "bad.stderr",
                    timeout_seconds=2,
                )
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifact = root / "artifact"
            artifact.mkdir()
            external = root / "external-build"
            external.mkdir()
            (external / "build-receipt.json").write_bytes(b"{}")
            try:
                (artifact / "build").symlink_to(external, target_is_directory=True)
            except OSError:
                return
            with self.assertRaises(contract.AdmissionError):
                run_probe.verify_build_receipt(
                    artifact / "build" / "build-receipt.json",
                    repo=root,
                    artifact_root=artifact,
                    target_dir=root / "target",
                    require_windows=False,
                )

    def test_test_status_parser_binds_all_four_names(self) -> None:
        stdout = b"".join(
            (
                f"test {name} ... "
                f"{'ignored' if name == contract.PROBE_TEST else 'ok'}\n"
            ).encode()
            for name in contract.REQUIRED_TESTS
        )
        expected = {
            name: ("ignored" if name == contract.PROBE_TEST else "ok")
            for name in contract.REQUIRED_TESTS
        }
        self.assertEqual(build_probe._test_statuses(stdout), expected)

    def test_static_report_and_positive_marker_are_exact(self) -> None:
        repo = Path(__file__).resolve().parents[2]
        record = build_probe._verify_static_report(repo)
        marker = (
            f"{contract.STATIC_POSITIVE_MARKER} status=STATIC-ADMITTED "
            f"combined_identity_sha256={contract.STATIC_COMBINED_IDENTITY_SHA256} "
            f"payload_sha256={contract.STATIC_REPORT_PAYLOAD_SHA256}\n"
        ).encode()
        build_probe._positive_static_check(
            marker, record["payload_sha256"], "fixture"
        )
        with self.assertRaises(contract.AdmissionError):
            build_probe._positive_static_check(
                marker.replace(b"STATIC-ADMITTED", b"ADMITTED"),
                record["payload_sha256"],
                "fixture",
            )

    def test_build_commands_are_locked_cpu_release_and_two_platform_static(self) -> None:
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
        windows = contract.windows_static_commands("python.exe")
        linux = contract.linux_static_commands("wsl.exe")
        self.assertEqual(len(windows), 2)
        self.assertEqual(len(linux), 2)
        self.assertIn("--check", windows[0])
        self.assertEqual(linux[0][:4], ["wsl.exe", "--cd", contract.LINUX_REPO, "python3"])
        self.assertEqual(
            contract.windows_packaging_test_command("python.exe")[:3],
            ["python.exe", "-m", "unittest"],
        )
        self.assertEqual(
            contract.linux_packaging_test_command("wsl.exe")[:4],
            ["wsl.exe", "--cd", contract.LINUX_REPO, "python3"],
        )
        self.assertEqual(contract.PACKAGING_REQUIRED_TEST_COUNT, 26)
        with self.assertRaises(contract.AdmissionError):
            contract.build_environment_policy(
                {"RUSTFLAGS": "-C target-cpu=native"},
                repo=Path("."),
                target_dir=Path("target"),
                rustc=Path("rustc"),
            )


if __name__ == "__main__":
    unittest.main()
