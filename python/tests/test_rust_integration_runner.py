"""Coverage and process-isolation checks for the consolidated Cargo runner."""

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import run_rust_integration_tests as runner


class IntegrationRunnerTests(unittest.TestCase):
    def test_inventory_rejects_unregistered_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tests = root / "mtg-kernel" / "tests"
            (tests / "fdn").mkdir(parents=True)
            (tests / "fdn" / "one.rs").write_text("")
            (tests / "fdn" / "two.rs").write_text("")
            (tests / "fdn_integration.rs").write_text("mod one;\n")
            with self.assertRaisesRegex(ValueError, "inventory"):
                runner.suite_modules(root, "fdn_integration")

    def test_real_source_inventory_is_complete(self):
        root = Path(__file__).resolve().parents[2]
        for suite in runner.SUITES:
            with self.subTest(suite=suite):
                self.assertTrue(runner.suite_modules(root, suite))

    def test_fresh_process_per_module_and_failures_continue(self):
        with patch.object(runner, "listed_tests", return_value=[
            "one::a", "one::nested::b", "one_longer::a", "two::ignored_case",
        ]), patch.object(runner.subprocess, "run", side_effect=[
            subprocess.CompletedProcess([], 1), subprocess.CompletedProcess([], 0),
            subprocess.CompletedProcess([], 0),
        ]) as run:
            self.assertEqual(runner.run_suite("suite.exe"), 1)
            self.assertEqual([call.args[0] for call in run.call_args_list], [
                ["suite.exe", "one::"], ["suite.exe", "one_longer::"], ["suite.exe", "two::"],
            ])

    def test_empty_active_suite_refuses_success(self):
        with patch.object(runner, "listed_tests", return_value=[]):
            with self.assertRaisesRegex(ValueError, "no tests"):
                runner.run_suite("suite.exe")
            self.assertEqual(runner.run_suite("suite.exe", allow_empty=True), 0)


if __name__ == "__main__":
    unittest.main()
