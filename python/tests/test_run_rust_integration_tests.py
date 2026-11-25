"""The integration shards hold every module exactly once, and a shard job runs only its own modules."""
from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1] / "tools"
ROOT = Path(__file__).resolve().parents[2]
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import run_rust_integration_tests as integration  # noqa: E402

RECORDED = {"heavy": 1400.0, "large": 900.0, "middle": 120.0, "small": 40.0, "tiny": 1.0}


def placed(plan):
    return sorted(module for shard in plan for module in shard)


def shard_of(plan):
    return {module: index for index, shard in enumerate(plan) for module in shard}


class ShardPlanTests(unittest.TestCase):
    def assert_partition(self, plan, modules, count):
        self.assertEqual(len(plan), count)
        self.assertEqual(placed(plan), sorted(modules))
        self.assertEqual(len(placed(plan)), len(set(placed(plan))))
        self.assertTrue(all(plan))

    def test_the_shards_are_disjoint_and_their_union_is_the_inventory(self):
        modules = sorted({*RECORDED, "new_a", "new_b", "new_c"})
        for count in range(1, len(modules) + 1):
            with self.subTest(count=count):
                self.assert_partition(integration.shard_plan(modules, count, RECORDED, 500.0), modules, count)

    def test_the_heaviest_modules_go_first_to_the_least_loaded_shard(self):
        plan = integration.shard_plan(sorted(RECORDED), 3, RECORDED, 500.0)
        self.assertEqual(plan, [["heavy"], ["large"], ["middle", "small", "tiny"]])

    def test_the_plan_does_not_depend_on_the_inventory_order(self):
        modules = sorted({*RECORDED, "new_a", "new_b"})
        self.assertEqual(integration.shard_plan(modules, 3, RECORDED, 500.0),
                         integration.shard_plan(list(reversed(modules)), 3, RECORDED, 500.0))

    def test_adding_a_module_keeps_the_partition_and_moves_no_heavier_module(self):
        before = sorted(RECORDED)
        old = integration.shard_plan(before, 3, RECORDED, 500.0)
        for weight_name in ("new_module", "aaa_first_by_name", "zzz_last_by_name"):
            with self.subTest(module=weight_name):
                after = integration.shard_plan(sorted([*before, weight_name]), 3, RECORDED, 500.0)
                self.assert_partition(after, [*before, weight_name], 3)
                # The new module is unrecorded, so it weighs the default (500 s); every module recorded as
                # heavier keeps its shard.
                for module in ("heavy", "large"):
                    self.assertEqual(shard_of(after)[module], shard_of(old)[module])

    def test_an_impossible_or_duplicated_plan_fails(self):
        with self.assertRaises(ValueError):
            integration.shard_plan(["a", "b"], 3, {}, 1.0)
        with self.assertRaises(ValueError):
            integration.shard_plan(["a", "a"], 1, {}, 1.0)
        with self.assertRaises(ValueError):
            integration.shard_plan(["a"], 0, {}, 1.0)

    def test_an_unrecorded_module_weighs_the_mean(self):
        with tempfile.TemporaryDirectory() as directory:
            table = Path(directory) / "seconds.json"
            table.write_text(json.dumps({"suite": {"a": 10, "b": 30}}), encoding="utf-8")
            self.assertEqual(integration.module_seconds("suite", ["a", "b", "new"], table),
                             ({"a": 10, "b": 30}, 20.0))
            self.assertEqual(integration.module_seconds("other", ["a"], table), ({}, 1.0))
            self.assertEqual(integration.module_seconds("suite", ["a"], Path(directory) / "missing.json"), ({}, 1.0))

    def test_a_removed_module_does_not_weigh_in_the_default(self):
        with tempfile.TemporaryDirectory() as directory:
            table = Path(directory) / "seconds.json"
            table.write_text(json.dumps({"suite": {"removed": 1000, "existing": 10}}), encoding="utf-8")
            recorded, default = integration.module_seconds("suite", ["existing", "new"], table)
            self.assertEqual(default, 10.0)
            recorded, default = integration.module_seconds("suite", ["new"], table)
            self.assertEqual(default, 1.0)


class CheckedInTableTests(unittest.TestCase):
    def test_the_eternal_shards_of_ci_cover_the_real_inventory(self):
        modules = integration.suite_modules(ROOT, "eternal_integration")
        recorded, default = integration.module_seconds("eternal_integration", modules)
        self.assertTrue(recorded)
        self.assertTrue(all(isinstance(seconds, int) and seconds >= 1 for seconds in recorded.values()))
        plan = integration.shard_plan(modules, 3, recorded, default)
        self.assertEqual(placed(plan), sorted(modules))

    def test_the_isolated_tests_of_ci_are_units_of_modules_in_the_inventory(self):
        modules = integration.suite_modules(ROOT, "eternal_integration")
        isolated = integration.isolated_tests("eternal_integration")
        self.assertTrue(isolated)
        self.assertLessEqual(set(isolated), set(modules))
        units = integration.plan_units(modules, isolated)
        self.assertEqual(len(units), len(modules) + sum(map(len, isolated.values())))
        recorded, default = integration.module_seconds("eternal_integration", units)
        # Every unit of the plan CI runs has a recorded time, and no recorded time is for anything else.
        self.assertEqual(sorted(recorded), sorted(units))
        plan = integration.shard_plan(units, 8, recorded, default)
        self.assertEqual(placed(plan), sorted(units))
        # An isolated test is the heaviest thing in its shard, which is the point of isolating it.
        for module, names in isolated.items():
            for name in names:
                unit = f"{module}::{name}"
                (shard,) = [members for members in plan if unit in members]
                self.assertEqual(max(shard, key=lambda member: recorded[member]), unit)


class UnitTests(unittest.TestCase):
    TESTS = ["alpha::big", "alpha::one", "alpha::two", "alphabet::one", "beta::one"]
    ISOLATED = {"alpha": ["big"]}

    def test_an_isolated_test_is_a_unit_and_its_module_keeps_the_rest(self):
        units = integration.plan_units(["alpha", "alphabet", "beta"], self.ISOLATED)
        self.assertEqual(units, ["alpha", "alphabet", "beta", "alpha::big"])
        self.assertEqual(integration.unit_tests("alpha::big", units, self.TESTS), ["alpha::big"])
        self.assertEqual(integration.unit_tests("alpha", units, self.TESTS), ["alpha::one", "alpha::two"])
        # A module name that is a prefix of another takes none of the other's tests.
        self.assertEqual(integration.unit_tests("alphabet", units, self.TESTS), ["alphabet::one"])
        self.assertEqual(integration.unit_tests("beta", units, self.TESTS), ["beta::one"])

    def test_an_isolated_test_outside_the_inventory_is_no_unit(self):
        self.assertEqual(integration.plan_units(["beta"], self.ISOLATED), ["beta"])

    def test_an_isolated_name_the_binary_lacks_is_an_empty_unit_and_loses_no_test(self):
        units = integration.plan_units(["alpha"], {"alpha": ["renamed_away"]})
        tests = ["alpha::one", "alpha::renamed"]
        self.assertEqual(integration.unit_tests("alpha::renamed_away", units, tests), [])
        self.assertEqual(integration.unit_tests("alpha", units, tests), tests)
        integration.check_assignment([["alpha"], ["alpha::renamed_away"]], tests)

    def test_every_test_must_be_in_exactly_one_shard(self):
        integration.check_assignment([["alpha", "beta"], ["alpha::big", "alphabet"]], self.TESTS)
        with self.assertRaisesRegex(ValueError, "not in exactly one shard: \\['beta::one'\\]"):
            integration.check_assignment([["alpha"], ["alpha::big", "alphabet"]], self.TESTS)
        with self.assertRaisesRegex(ValueError, "more than one shard"):
            integration.check_assignment([["alpha", "beta"], ["alpha", "alpha::big", "alphabet"]], self.TESTS)
        with self.assertRaisesRegex(ValueError, "lists a test name twice"):
            integration.check_assignment([["beta"]], ["beta::one", "beta::one"])
        # A module that reached the binary without a unit of the plan.
        with self.assertRaisesRegex(ValueError, "not in exactly one shard: \\['gamma::one'\\]"):
            integration.check_assignment([["alpha", "beta"], ["alpha::big", "alphabet"]], [*self.TESTS, "gamma::one"])

    def test_a_malformed_isolated_table_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            table = Path(directory) / "seconds.json"
            for bad in ({"m": []}, {"m": ["a", "a"]}, {"m": ["m::a"]}, {"m": [""]}):
                table.write_text(json.dumps({"isolated": {"suite": bad}}), encoding="utf-8")
                with self.subTest(bad=bad), self.assertRaises(ValueError):
                    integration.isolated_tests("suite", table)
            table.write_text(json.dumps({"isolated": {"suite": {"m": ["a", "b"]}}}), encoding="utf-8")
            self.assertEqual(integration.isolated_tests("suite", table), {"m": ["a", "b"]})
            self.assertEqual(integration.isolated_tests("other", table), {})
            self.assertEqual(integration.isolated_tests("suite", Path(directory) / "missing.json"), {})


class RecordedRunTests(unittest.TestCase):
    def test_the_reported_tests_are_named_and_the_output_passes_through(self):
        script = ("import sys\n"
                  "print('running 4 tests')\n"
                  "print('test alpha::one ... ok')\n"
                  "print('test alpha::slow has been running for over 60 seconds')\n"
                  "print('test alpha::two - should panic ... ok')\n"
                  "print('test alpha::three ... FAILED')\n"
                  "print('test alpha::four ... ignored, needs a GPU')\n"
                  "print('  test alpha::indented ... ok')\n"
                  "print('test result: FAILED. 2 passed; 1 failed')\n"
                  "sys.exit(101)\n")
        written = []
        fake = type("Out", (), {"flush": lambda self: None,
                                "buffer": type("Buffer", (), {"write": lambda self, raw: written.append(raw),
                                                              "flush": lambda self: None})()})()
        with patch.object(integration.sys, "stdout", fake):
            code, reported = integration.run_recorded([sys.executable, "-c", script])
        self.assertEqual(code, 101)
        self.assertEqual(reported, ["alpha::one", "alpha::two", "alpha::three", "alpha::four"])
        self.assertEqual(b"".join(written).decode().replace("\r\n", "\n").count("\n"), 8)

    def reported_by(self, script):
        fake = type("Out", (), {"flush": lambda self: None,
                                "buffer": type("Buffer", (), {"write": lambda self, raw: None,
                                                              "flush": lambda self: None})()})()
        with patch.object(integration.sys, "stdout", fake):
            return integration.run_recorded([sys.executable, "-c", script])

    def test_every_result_shaped_line_is_returned_in_order(self):
        # libtest replays what passing tests printed (--show-output), and a child process can print before
        # the last result: the parser stops nowhere and judges nothing. run_suite selects by exact name and
        # requires a line for each selected test.
        script = ("print('test alpha::one ... ok')\n"
                  "print('successes:')\n"
                  "print('---- alpha::one stdout ----')\n"
                  "print('test alpha::one ... ok')\n"
                  "print('test alpha::two ... ok')\n"
                  "print('test result: ok. 2 passed')\n")
        self.assertEqual(self.reported_by(script), (0, ["alpha::one", "alpha::one", "alpha::two"]))

    def test_a_result_line_written_in_two_pieces_is_one_result(self):
        # One test thread: libtest writes the name when the test starts and the outcome when it ends.
        script = ("import sys, time\n"
                  "sys.stdout.write('test alpha::slow ... ')\n"
                  "sys.stdout.flush()\n"
                  "time.sleep(0.2)\n"
                  "sys.stdout.write('ok\\n')\n"
                  "sys.stdout.flush()\n")
        self.assertEqual(self.reported_by(script), (0, ["alpha::slow"]))


class RunSuiteTests(unittest.TestCase):
    TESTS = ["alpha::one", "alpha::two", "beta::one", "gamma::one"]

    def run_suite(self, selection, returncodes=None, tests=None, **more):
        calls = []

        def run(command, **_):
            calls.append(command[1:])
            return type("Result", (), {"returncode": (returncodes or {}).get(command[-1], 0)})()

        with patch.object(integration, "listed_tests", lambda _: list(tests or self.TESTS)), \
                patch.object(integration.subprocess, "run", run), patch("builtins.print"):
            code = integration.run_suite("suite-binary", selection=selection, **more)
        return code, calls

    def test_without_a_shard_every_module_runs_by_its_prefix(self):
        self.assertEqual(self.run_suite(None), (0, [["alpha::"], ["beta::"], ["gamma::"]]))

    def test_a_shard_runs_only_its_selected_modules_by_exact_test_name(self):
        selection = {"inventory": ["alpha", "beta", "gamma"], "selected": ["alpha", "gamma"], "shard": [1, 2]}
        self.assertEqual(self.run_suite(selection),
                         (0, [["--exact", "alpha::one", "alpha::two"], ["--exact", "gamma::one"]]))

    def test_a_failed_module_fails_the_shard(self):
        selection = {"inventory": ["alpha", "beta", "gamma"], "selected": ["beta"], "shard": [0, 2]}
        self.assertEqual(self.run_suite(selection, {"beta::one": 101}), (1, [["--exact", "beta::one"]]))

    def test_a_compiled_module_outside_the_inventory_fails_before_running(self):
        selection = {"inventory": ["alpha", "beta"], "selected": ["alpha", "beta"], "shard": [0, 1]}
        with self.assertRaisesRegex(ValueError, "in no shard: \\['gamma'\\]"):
            self.run_suite(selection)

    PLAN = [["alpha", "gamma"], ["alpha::two", "beta"]]

    def planned(self, shard, plan=None, inventory=("alpha", "beta", "gamma")):
        plan = plan or self.PLAN
        return {"inventory": list(inventory), "plan": plan, "selected": plan[shard], "shard": [shard, len(plan)]}

    def test_a_part_names_its_tests_exactly(self):
        self.assertEqual(self.run_suite(self.planned(0)),
                         (0, [["--exact", "alpha::one"], ["--exact", "gamma::one"]]))
        self.assertEqual(self.run_suite(self.planned(1)),
                         (0, [["--exact", "alpha::two"], ["--exact", "beta::one"]]))

    def test_a_module_whose_name_contains_another_is_never_selected_with_it(self):
        # libtest filters by substring: the filter `alpha::` also selects `beta_alpha::one`. With the two
        # modules in different parts that test would run in both, so a part never passes a module filter.
        tests = ["alpha::one", "beta_alpha::one"]
        plan = [["alpha"], ["beta_alpha"]]
        ran = []
        for shard in (0, 1):
            code, calls = self.run_suite(self.planned(shard, plan, inventory=("alpha", "beta_alpha")), tests=tests)
            self.assertEqual(code, 0)
            for call in calls:
                self.assertEqual(call[0], "--exact")
                ran += call[1:]
        self.assertEqual(sorted(ran), sorted(tests))

    def test_the_parts_run_every_test_once(self):
        ran = []
        for shard in (0, 1):
            for call in self.run_suite(self.planned(shard))[1]:
                self.assertEqual(call[0], "--exact")
                ran += call[1:]
        self.assertEqual(sorted(ran), sorted(self.TESTS))

    def test_a_selection_too_long_for_one_command_is_refused(self):
        tests = [f"alpha::{'x' * 200}_{index}" for index in range(200)]
        with self.assertRaisesRegex(ValueError, "too long for one command"):
            self.run_suite(self.planned(0, [["alpha"]], inventory=("alpha",)), tests=tests)

    def test_a_plan_that_leaves_a_test_out_or_doubles_it_fails_before_running(self):
        for plan in ([["alpha"], ["alpha::two", "beta"]], [["alpha", "gamma"], ["beta"], ["gamma"]]):
            with self.subTest(plan=plan), self.assertRaises(ValueError):
                self.run_suite(self.planned(0, plan))
        selection = {**self.planned(0), "selected": ["alpha"]}
        with self.assertRaisesRegex(ValueError, "not this shard of the plan"):
            self.run_suite(selection)

    def test_an_isolated_test_missing_from_the_binary_runs_nothing_for_it(self):
        plan = [["alpha", "beta", "gamma"], ["alpha::gone"]]
        self.assertEqual(self.run_suite(self.planned(1, plan)), (0, []))
        self.assertEqual(self.run_suite(self.planned(0, plan)),
                         (0, [["--exact", "alpha::one", "alpha::two"], ["--exact", "beta::one"],
                              ["--exact", "gamma::one"]]))

    def test_the_list_mode_writes_the_names_and_runs_nothing(self):
        with tempfile.TemporaryDirectory() as directory:
            listing = Path(directory) / "tests.json"
            self.assertEqual(self.run_suite(None, list_path=str(listing)), (0, []))
            self.assertEqual(json.loads(listing.read_text(encoding="utf-8")),
                             {"attempt": int(os.environ.get("GITHUB_RUN_ATTEMPT") or 0), "tests": sorted(self.TESTS)})

    def recorded(self, selection, reports, returncode=0):
        def run_recorded(command):
            return returncode, reports(command[1:])

        with tempfile.TemporaryDirectory() as directory, \
                patch.object(integration, "listed_tests", lambda _: list(self.TESTS)), \
                patch.object(integration, "run_recorded", run_recorded), patch("builtins.print"), \
                patch.dict(os.environ, {"GITHUB_RUN_ATTEMPT": "3"}):
            path = Path(directory) / "executed.json"
            code = integration.run_suite("suite-binary", selection=selection, executed_path=str(path))
            return code, json.loads(path.read_text(encoding="utf-8"))

    @staticmethod
    def faithful(arguments):
        if arguments[0] == "--exact":
            return list(arguments[1:])
        return [test for test in RunSuiteTests.TESTS if test.startswith(arguments[0])]

    def test_a_part_records_the_tests_it_executed(self):
        code, record = self.recorded(self.planned(1), self.faithful)
        self.assertEqual(code, 0)
        self.assertEqual(record, {"shard": [1, 2], "attempt": 3, "listed": sorted(self.TESTS),
                                  "executed": ["alpha::two", "beta::one"], "failed": 0})

    def test_a_result_line_replayed_from_a_test_is_one_execution(self):
        # With --show-output a passing test's own printed "test x ... ok" line appears a second time.
        code, record = self.recorded(self.planned(1), lambda arguments: self.faithful(arguments) * 2)
        self.assertEqual((code, record["failed"]), (0, 0))
        self.assertEqual(record["executed"], ["alpha::two", "beta::one"])

    def test_a_passing_process_that_did_not_report_a_selected_test_fails_its_unit(self):
        code, record = self.recorded(self.planned(1), lambda arguments: [])
        self.assertEqual((code, record["failed"]), (1, 2))
        code, record = self.recorded(self.planned(1), lambda arguments: self.faithful(arguments)[:0]
                                     if arguments[-1] == "beta::one" else self.faithful(arguments))
        self.assertEqual((code, record["failed"], record["executed"]), (1, 1, ["alpha::two"]))

    def test_a_passing_process_that_reported_an_unselected_test_fails_its_unit(self):
        code, record = self.recorded(self.planned(1), lambda arguments: [*self.faithful(arguments), "gamma::one"])
        self.assertEqual((code, record["failed"]), (1, 2))

    def test_an_unsharded_recorded_run_catches_a_module_filter_that_ran_another_module(self):
        # Without a plan a module is still selected by its prefix; a recorded run then refuses the overlap.
        def substring(arguments):
            return [test for test in RunSuiteTests.TESTS if arguments[0] in test] + \
                (["gamma::one"] if arguments[0] == "alpha::" else [])

        code, record = self.recorded(None, substring)
        self.assertEqual((code, record["failed"], record["shard"]), (1, 1, None))

    def test_a_failed_process_is_counted_once(self):
        code, record = self.recorded(self.planned(1), lambda arguments: [], returncode=101)
        self.assertEqual((code, record["failed"]), (1, 2))


class MainTests(unittest.TestCase):
    def test_a_shard_job_passes_its_plan_to_the_cargo_runner(self):
        commands = []

        def run(command, env=None, **_):
            commands.append((command, env))
            return type("Result", (), {"returncode": 0})()

        argv = ["run_rust_integration_tests.py", "eternal_integration", "--features", "eternal-fixtures",
                "--shard-index", "2", "--shard-count", "3"]
        with patch.object(integration.subprocess, "run", run), patch.object(sys, "argv", argv), \
                patch("builtins.print"):
            self.assertEqual(integration.main(), 0)
        ((command, environment),) = commands
        self.assertEqual(command[-2:], ["--test", "eternal_integration"])
        shard = json.loads(environment[integration.SHARD_ENV])
        modules = integration.suite_modules(ROOT, "eternal_integration")
        units = integration.plan_units(modules, integration.isolated_tests("eternal_integration"))
        recorded, default = integration.module_seconds("eternal_integration", units)
        plan = integration.shard_plan(units, 3, recorded, default)
        self.assertEqual(shard, {"inventory": modules, "shard": [2, 3], "plan": plan, "selected": plan[2]})
        self.assertNotIn(integration.LIST_ENV, environment)
        self.assertNotIn(integration.EXECUTED_ENV, environment)

    def test_the_list_and_executed_files_reach_the_runner_by_full_path(self):
        for flag, name in (("--list-only", integration.LIST_ENV), ("--executed-list", integration.EXECUTED_ENV)):
            commands = []

            def run(command, env=None, **_):
                commands.append((command, env))
                return type("Result", (), {"returncode": 0})()

            argv = ["run_rust_integration_tests.py", "standard_integration", "--features", "x", flag, "out.json"]
            stale = {integration.LIST_ENV: "stale", integration.EXECUTED_ENV: "stale"}
            with self.subTest(flag=flag), patch.object(integration.subprocess, "run", run), \
                    patch.object(sys, "argv", argv), patch.dict(os.environ, stale):
                self.assertEqual(integration.main(), 0)
                ((command, environment),) = commands
                self.assertEqual(environment[name], str(Path("out.json").resolve()))
                other = integration.EXECUTED_ENV if name == integration.LIST_ENV else integration.LIST_ENV
                self.assertNotIn(other, environment)
                self.assertNotIn("--no-run", command)

    def test_no_run_builds_without_running(self):
        commands = []

        def run(command, env=None, **_):
            commands.append(command)
            return type("Result", (), {"returncode": 0})()

        argv = ["run_rust_integration_tests.py", "eternal_integration", "--features", "x", "--no-run"]
        with patch.object(integration.subprocess, "run", run), patch.object(sys, "argv", argv):
            self.assertEqual(integration.main(), 0)
        self.assertEqual(commands[0][-3:], ["--test", "eternal_integration", "--no-run"])

    def test_an_unsharded_run_passes_no_plan(self):
        commands = []

        def run(command, env=None, **_):
            commands.append(env)
            return type("Result", (), {"returncode": 0})()

        argv = ["run_rust_integration_tests.py", "standard_integration", "--features", "x"]
        with patch.object(integration.subprocess, "run", run), patch.object(sys, "argv", argv), \
                patch.dict(os.environ, {integration.SHARD_ENV: "stale"}):
            self.assertEqual(integration.main(), 0)
        self.assertNotIn(integration.SHARD_ENV, commands[0])

    def test_a_shard_index_outside_the_count_is_refused(self):
        argv = ["run_rust_integration_tests.py", "eternal_integration", "--features", "x",
                "--shard-index", "3", "--shard-count", "3"]
        with patch.object(sys, "argv", argv), patch("sys.stderr"), self.assertRaises(SystemExit):
            integration.main()


if __name__ == "__main__":
    unittest.main()
