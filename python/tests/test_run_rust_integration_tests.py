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
            self.assertEqual(integration.module_seconds("suite", table), ({"a": 10, "b": 30}, 20.0))
            self.assertEqual(integration.module_seconds("other", table), ({}, 1.0))
            self.assertEqual(integration.module_seconds("suite", Path(directory) / "missing.json"), ({}, 1.0))


class CheckedInTableTests(unittest.TestCase):
    def test_the_eternal_shards_of_ci_cover_the_real_inventory(self):
        modules = integration.suite_modules(ROOT, "eternal_integration")
        recorded, default = integration.module_seconds("eternal_integration")
        self.assertTrue(recorded)
        self.assertTrue(all(isinstance(seconds, int) and seconds >= 1 for seconds in recorded.values()))
        plan = integration.shard_plan(modules, 3, recorded, default)
        self.assertEqual(placed(plan), sorted(modules))


class RunSuiteTests(unittest.TestCase):
    TESTS = ["alpha::one", "alpha::two", "beta::one", "gamma::one"]

    def run_suite(self, selection, returncodes=None):
        calls = []

        def run(command, **_):
            calls.append(command[1])
            return type("Result", (), {"returncode": (returncodes or {}).get(command[1], 0)})()

        with patch.object(integration, "listed_tests", lambda _: list(self.TESTS)), \
                patch.object(integration.subprocess, "run", run), patch("builtins.print"):
            code = integration.run_suite("suite-binary", selection=selection)
        return code, calls

    def test_without_a_shard_every_module_runs(self):
        self.assertEqual(self.run_suite(None), (0, ["alpha::", "beta::", "gamma::"]))

    def test_a_shard_runs_only_its_selected_modules(self):
        selection = {"inventory": ["alpha", "beta", "gamma"], "selected": ["alpha", "gamma"], "shard": [1, 2]}
        self.assertEqual(self.run_suite(selection), (0, ["alpha::", "gamma::"]))

    def test_a_failed_module_fails_the_shard(self):
        selection = {"inventory": ["alpha", "beta", "gamma"], "selected": ["beta"], "shard": [0, 2]}
        self.assertEqual(self.run_suite(selection, {"beta::": 101}), (1, ["beta::"]))

    def test_a_compiled_module_outside_the_inventory_fails_before_running(self):
        selection = {"inventory": ["alpha", "beta"], "selected": ["alpha", "beta"], "shard": [0, 1]}
        with self.assertRaisesRegex(ValueError, "in no shard: \\['gamma'\\]"):
            self.run_suite(selection)


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
        recorded, default = integration.module_seconds("eternal_integration")
        self.assertEqual(shard, {"inventory": modules, "shard": [2, 3],
                                 "selected": integration.shard_plan(modules, 3, recorded, default)[2]})

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
