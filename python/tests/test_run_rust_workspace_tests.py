"""The two parts of the workspace suite select disjoint targets, and the library's parts run each test once."""
from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1] / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import check_rust_integration_parts as inventory  # noqa: E402
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



TIMING = workspace.TIMING_TEST
# Module names that are substrings of each other, a name inside another name, and the timing gate.
LIBRARY = sorted(["alpha::one", "alpha::two", "beta_alpha::one", "gamma::slow", "gamma::slow_more", "delta::one",
                  "delta::two", "epsilon::one", "zeta::x::one", "zeta::y::one", "snapshot::tests::fast", TIMING])
TIMES = {"gamma::slow": 300, "zeta::x::one": 40, "alpha::two": 30}


def libtest(tests, swap=None):
    """`listed_tests` of a binary with libtest's selection: substring filters or-ed, then substring skips.

    `swap` = (a, b): the binary selects b wherever the filters select a, and a wherever they select b.
    """
    def listed(_executable, *selection):
        filters, skips, words = [], [], list(selection)
        while words:
            word = words.pop(0)
            if word == "--skip":
                skips.append(words.pop(0))
            else:
                filters.append(word)
        chosen = [test for test in tests if (not filters or any(item in test for item in filters))
                  and not any(item in test for item in skips)]
        if swap:
            chosen = sorted({swap[1] if test == swap[0] else swap[0] if test == swap[1] else test
                             for test in chosen})
        return chosen
    return listed


def plan_patches(tests=LIBRARY, **libtest_options):
    return (patch.object(workspace, "listed_tests", libtest(tests, **libtest_options)),
            patch.object(workspace.lib_plan, "load_weights", lambda: lambda name: TIMES.get(name, 1.0)),
            patch("builtins.print"))


class LibraryPlanTests(unittest.TestCase):
    def plan(self, count=4, tests=LIBRARY, **options):
        first, second, third = plan_patches(tests, **options)
        with first, second, third:
            return workspace.library_plan("mtg_kernel-1", count)

    def test_the_binary_lists_every_test_but_the_timing_gate_in_exactly_one_part(self):
        listed, plan, selections = self.plan()
        self.assertEqual(listed, LIBRARY)
        self.assertEqual(len(plan), 4)
        self.assertEqual(sorted(test for selection in selections for test in selection),
                         [test for test in LIBRARY if test != TIMING])
        # Units: `alpha::one` selects beta_alpha::one, `gamma::slow` selects gamma::slow_more.
        (alpha,) = [filters for filters in plan if "alpha::one" in filters]
        self.assertIn("beta_alpha::", alpha)
        (gamma,) = [selection for selection in selections if "gamma::slow" in selection]
        self.assertIn("gamma::slow_more", gamma)

    def test_a_binary_that_selects_otherwise_than_the_model_stops_every_part(self):
        # The two parts still hold every test once between them, but not as the plan says.
        _, plan, selections = self.plan()
        first, second = selections[0][0], selections[1][0]
        with self.assertRaisesRegex(ValueError, "other tests for part 0 than the plan's model"):
            self.plan(swap=(first, second))

    def test_a_plan_that_gives_a_test_to_two_parts_stops_every_part(self):
        overlapping = ([["alpha::", "beta_alpha::", "delta::", "epsilon::", "gamma::", "snapshot::", "zeta::"],
                        ["delta::two"]], [1.0, 1.0])
        with patch.object(workspace.lib_plan, "plan_parts", lambda *_args: overlapping):
            with self.assertRaisesRegex(ValueError, "not in exactly one part: \\['delta::two'\\]"):
                self.plan(count=2)

    def test_a_part_without_filters_or_with_an_option_stops_every_part(self):
        for broken in ([[], ["alpha::"]], [["--include-ignored"], ["alpha::"]]):
            with self.subTest(plan=broken), patch.object(workspace.lib_plan, "plan_parts",
                                                         lambda *_args, plan=broken: (plan, [1.0, 1.0])):
                with self.assertRaisesRegex(ValueError, "no filters or an option"):
                    self.plan(count=2)

    def test_the_timing_gate_must_be_a_test_and_hide_no_other(self):
        with self.assertRaisesRegex(ValueError, "is not a test of the binary"):
            self.plan(tests=[test for test in LIBRARY if test != TIMING])
        with self.assertRaisesRegex(ValueError, "would hide"):
            self.plan(tests=[*LIBRARY, TIMING + "_too"])

    def test_filters_too_long_for_one_command_stop_every_part(self):
        with patch.object(workspace, "MAX_COMMAND", 40), self.assertRaisesRegex(ValueError, "too long"):
            self.plan()


class LibraryPartTests(unittest.TestCase):
    def run_part(self, index, count=4, reports=None, returncode=0, gate=(0, True)):
        """(exit code, record, gate calls) of one part; `reports` maps the shard command to its result lines."""
        first, second, third = plan_patches()
        listed = libtest(LIBRARY)
        gates = []

        def run_recorded(command):
            selected = listed(None, *command[1:])
            return returncode, (reports or (lambda names: names))(selected)

        def run_timing_gate(executable, count_listed):
            gates.append(count_listed)
            return gate

        with tempfile.TemporaryDirectory() as directory, first, second, third, \
                patch.object(workspace, "run_recorded", run_recorded), \
                patch.object(workspace, "run_timing_gate", run_timing_gate), \
                patch.dict(os.environ, {"GITHUB_RUN_ATTEMPT": "2"}):
            path = Path(directory) / "executed.json"
            code = workspace.run_library_part("mtg_kernel-1", index, count, str(path))
            return code, json.loads(path.read_text(encoding="utf-8")), gates

    def test_the_parts_together_execute_every_listed_test_once(self):
        records = []
        for index in range(4):
            code, record, gates = self.run_part(index)
            self.assertEqual(code, 0)
            self.assertEqual(record["failed"], 0)
            # Only part 0 runs the timing gate, after its tests.
            self.assertEqual(gates, [len(LIBRARY)] if index == 0 else [])
            self.assertEqual(TIMING in record["executed"], index == 0)
            records.append(record)
        self.assertEqual(inventory.check(LIBRARY, records, 4), [])

    def test_a_selected_test_without_a_result_line_fails_the_part(self):
        code, record, gates = self.run_part(0, reports=lambda names: names[1:])
        self.assertEqual((code, record["failed"]), (1, 1))
        self.assertEqual(gates, [])

    def test_a_result_line_for_a_test_of_another_part_fails_the_part(self):
        own = set(self.run_part(1)[1]["executed"])
        foreign = [test for test in LIBRARY if test not in own and test != TIMING][:2]
        code, record, _ = self.run_part(1, reports=lambda names: [*names, *foreign])
        self.assertEqual((code, record["failed"]), (1, 1))
        self.assertEqual(set(record["executed"]), own)

    def test_a_replayed_result_line_is_one_execution(self):
        code, record, _ = self.run_part(2, reports=lambda names: names * 2)
        self.assertEqual(code, 0)
        self.assertEqual(len(record["executed"]), len(set(record["executed"])))

    def test_a_failed_part_skips_the_timing_gate(self):
        code, record, gates = self.run_part(0, returncode=101)
        self.assertEqual((code, record["failed"], gates), (1, 1, []))
        self.assertNotIn(TIMING, record["executed"])

    def test_a_failed_or_unproven_timing_gate_fails_part_0(self):
        for gate in ((1, False), (0, False), (1, True)):
            with self.subTest(gate=gate):
                code, record, gates = self.run_part(0, gate=gate)
                self.assertEqual((code, record["failed"]), (1, 1))
                self.assertEqual(TIMING in record["executed"], gate[1])


class TimingGateTests(unittest.TestCase):
    def gate(self, output, listed=10, exit_code=0):
        script = f"import sys\nsys.stdout.write({output!r})\nsys.exit({exit_code})\n"
        fake = type("Out", (), {"flush": lambda self: None,
                                "buffer": type("Buffer", (), {"write": lambda self, raw: None,
                                                              "flush": lambda self: None})()})()
        with patch.object(workspace, "timing_command", lambda _: [sys.executable, "-c", script]), \
                patch.object(workspace, "timing_options", dict), patch.object(workspace.sys, "stdout", fake), \
                patch("builtins.print"):
            return workspace.run_timing_gate("mtg_kernel-1", listed)

    PASS = (f"\nrunning 1 test\ntest {TIMING} ... snapshot clone: 2.172us/call over 2000 iterations\nok\n\n"
            "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.01s\n")

    def test_the_gate_alone_passing_is_proven_by_its_start_line_and_the_summary(self):
        # With --nocapture the test's own output splits libtest's result line.
        self.assertEqual(self.gate(self.PASS), (0, True))

    def test_anything_else_is_not_proof(self):
        for output, listed in ((self.PASS.replace("1 passed", "0 passed"), 10),
                               (self.PASS.replace("0 ignored", "1 ignored"), 10),
                               (self.PASS, 11),
                               (self.PASS.replace(TIMING, "snapshot::tests::other"), 10),
                               (self.PASS.replace("\nok\n", "\nok\ntest snapshot::tests::other ... ok\n"), 10),
                               ("\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; "
                                "10 filtered out\n", 10)):
            with self.subTest(output=output, listed=listed):
                self.assertEqual(self.gate(output, listed), (0, False))
        self.assertEqual(self.gate(self.PASS.replace("result: ok", "result: FAILED"), exit_code=101), (101, False))


class LibraryRunnerTests(unittest.TestCase):
    def dispatch(self, environment):
        calls = []
        with patch.object(workspace, "list_library", lambda *args: calls.append(("list", *args)) or 0), \
                patch.object(workspace, "run_library_part", lambda *args: calls.append(("part", *args)) or 0), \
                patch.object(workspace, "run_library", lambda *args: calls.append(("all", *args)) or 0), \
                patch.dict(os.environ, environment):
            for name in (workspace.LIST_ENV, workspace.SHARD_ENV, workspace.EXECUTED_ENV):
                if name not in environment:
                    os.environ.pop(name, None)
            self.assertEqual(workspace.run_tests("target/release/deps/mtg_kernel-abc", []), 0)
        return calls

    def test_the_cargo_runner_lists_runs_a_part_or_runs_everything(self):
        self.assertEqual(self.dispatch({workspace.LIST_ENV: "list.json"}),
                         [("list", "target/release/deps/mtg_kernel-abc", "list.json")])
        self.assertEqual(self.dispatch({workspace.SHARD_ENV: "[2, 4]", workspace.EXECUTED_ENV: "out.json"}),
                         [("part", "target/release/deps/mtg_kernel-abc", 2, 4, "out.json")])
        self.assertEqual(self.dispatch({}), [("all", "target/release/deps/mtg_kernel-abc")])

    def test_the_list_mode_writes_the_names_and_runs_nothing(self):
        with tempfile.TemporaryDirectory() as directory, \
                patch.object(workspace, "listed_tests", libtest(LIBRARY)), patch("builtins.print"), \
                patch.dict(os.environ, {"GITHUB_RUN_ATTEMPT": "3"}):
            path = Path(directory) / "tests.json"
            self.assertEqual(workspace.list_library("mtg_kernel-1", str(path)), 0)
            self.assertEqual(json.loads(path.read_text(encoding="utf-8")), {"attempt": 3, "tests": LIBRARY})

    def cargo(self, *arguments, environment=None):
        commands = []

        def run(command, env=None, **_):
            commands.append((command, env))
            return type("Result", (), {"returncode": 0})()

        stale = {workspace.SHARD_ENV: "[0, 1]", workspace.LIST_ENV: "stale", workspace.EXECUTED_ENV: "stale"}
        with patch.object(workspace, "suite_modules"), patch.object(workspace.subprocess, "run", run), \
                patch.object(sys, "argv", ["run_rust_workspace_tests.py", *arguments]), \
                patch.dict(os.environ, environment if environment is not None else stale):
            self.assertEqual(workspace.main(), 0)
        ((command, env),) = commands
        return command, env

    def test_the_build_the_check_and_the_parts_run_one_cargo_command(self):
        listing, listing_env = self.cargo("--part", "lib", "--list-only", "tests.json")
        check, check_env = self.cargo("--part", "lib", "--no-run")
        part, part_env = self.cargo("--part", "lib", "--shard-index", "3", "--shard-count", "4",
                                    "--executed-list", "executed.json")
        # Cargo reuses the build job's output only for the same command; --no-run only stops before running.
        self.assertEqual(check[-1], "--no-run")
        self.assertEqual(listing, check[:-1])
        self.assertEqual(listing, part)
        self.assertEqual(listing[listing.index("--workspace"):], ["--workspace", "--lib"])
        self.assertEqual(listing_env[workspace.LIST_ENV], str(Path("tests.json").resolve()))
        self.assertEqual(json.loads(part_env[workspace.SHARD_ENV]), [3, 4])
        self.assertEqual(part_env[workspace.EXECUTED_ENV], str(Path("executed.json").resolve()))
        # Stale settings from the job's environment never reach the runner.
        for env, kept in ((listing_env, {workspace.LIST_ENV}), (check_env, set()),
                          (part_env, {workspace.SHARD_ENV, workspace.EXECUTED_ENV})):
            for name in {workspace.SHARD_ENV, workspace.LIST_ENV, workspace.EXECUTED_ENV} - kept:
                self.assertNotIn(name, env)

    def test_inconsistent_options_are_refused(self):
        for arguments in (["--part", "targets", "--no-run"], ["--list-only", "x.json"],
                          ["--part", "lib", "--shard-index", "0", "--shard-count", "4"],
                          ["--part", "lib", "--shard-index", "4", "--shard-count", "4", "--executed-list", "x"],
                          ["--part", "lib", "--shard-index", "0", "--executed-list", "x"],
                          ["--part", "lib", "--executed-list", "x"],
                          ["--part", "lib", "--no-run", "--list-only", "x.json"],
                          ["--part", "lib", "--no-run", "--shard-index", "0", "--shard-count", "4",
                           "--executed-list", "x"],
                          ["--part", "other"]):
            with self.subTest(arguments=arguments), patch.object(sys, "argv", ["run", *arguments]), \
                    patch("sys.stderr"), self.assertRaises(SystemExit):
                workspace.main()


if __name__ == "__main__":
    unittest.main()
