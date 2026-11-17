"""The two parts of the workspace suite select disjoint targets."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1] / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import run_rust_workspace_tests as workspace  # noqa: E402

# Cargo flags that select the library in test mode: the library has `test = true` and `bench = true` by
# default, so each of these builds and runs its unit tests.
LIBRARY_TEST_FLAGS = {"--lib", "--tests", "--benches", "--all-targets"}


def target(kind, name, required=()):
    return {"kind": [kind], "name": name, "required-features": list(required)}


METADATA = {"packages": [{
    "features": {"default": ["base"], "base": [], "extra": []},
    "targets": [
        target("lib", "mtg_kernel"), target("bin", "tool"), target("example", "demo"),
        target("custom-build", "build-script-build"),
        target("test", "plain"), target("test", "needs_default", ["base"]), target("test", "needs_extra", ["extra"]),
        target("bench", "speed"), target("bench", "speed_extra", ["extra"]),
    ],
}]}


class WorkspacePartsTests(unittest.TestCase):
    def test_named_targets_are_the_tests_and_benchmarks_the_default_features_enable(self):
        self.assertEqual(workspace.named_targets(METADATA),
                         ["--test", "plain", "--test", "needs_default", "--bench", "speed"])

    def test_a_package_without_default_features_keeps_only_unconditional_targets(self):
        metadata = {"packages": [{"features": {"extra": []}, "targets": METADATA["packages"][0]["targets"]}]}
        self.assertEqual(workspace.named_targets(metadata), ["--test", "plain", "--bench", "speed"])

    def test_only_the_lib_part_selects_the_library_unit_tests(self):
        self.assertEqual(workspace.PARTS["lib"], ["--lib"])
        self.assertFalse(LIBRARY_TEST_FLAGS & set(workspace.PARTS["targets"]))

    def run_part(self, part):
        calls = []

        def run(command, **_):
            calls.append(command)
            return type("Result", (), {"returncode": 0})()

        with patch.object(workspace, "suite_modules"), patch.object(workspace.subprocess, "run", run), \
                patch.object(workspace, "default_named_targets", lambda: workspace.named_targets(METADATA)), \
                patch.object(sys, "argv", ["run_rust_workspace_tests.py", "--part", part, "--no-fail-fast"]):
            self.assertEqual(workspace.main(), 0)
        (command,) = calls
        return command

    def test_the_targets_part_names_each_test_and_benchmark_and_never_the_library(self):
        command = self.run_part("targets")
        self.assertFalse(LIBRARY_TEST_FLAGS & set(command))
        selection = command[command.index("--workspace") + 1:]
        self.assertEqual(selection, ["--bins", "--examples", "--test", "plain", "--test", "needs_default",
                                     "--bench", "speed", "--no-fail-fast"])

    def test_the_lib_part_selects_only_the_library(self):
        command = self.run_part("lib")
        self.assertEqual(command[command.index("--workspace") + 1:], ["--lib", "--no-fail-fast"])


if __name__ == "__main__":
    unittest.main()
