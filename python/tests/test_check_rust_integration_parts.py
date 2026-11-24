"""The parts of a suite must together execute exactly the tests of its binary."""
from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1] / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import check_rust_integration_parts as parts  # noqa: E402

LISTED = ["alpha::one", "alpha::two", "beta::one", "gamma::one"]


def record(index, executed, count=2, listed=LISTED, **more):
    return {"shard": [index, count], "attempt": 1, "listed": list(listed), "executed": list(executed),
            "failed": 0, **more}


class CheckTests(unittest.TestCase):
    def test_disjoint_parts_whose_union_is_the_list_pass(self):
        records = [record(0, ["alpha::one", "alpha::two"]), record(1, ["beta::one", "gamma::one"])]
        self.assertEqual(parts.check(LISTED, records, 2), [])

    def test_a_listed_test_no_part_executed_fails(self):
        records = [record(0, ["alpha::one", "alpha::two"]), record(1, ["beta::one"])]
        (problem,) = parts.check(LISTED, records, 2)
        self.assertIn("1 listed test(s) ran in no part: ['gamma::one']", problem)

    def test_a_test_executed_in_two_parts_fails(self):
        records = [record(0, ["alpha::one", "alpha::two", "beta::one"]), record(1, ["beta::one", "gamma::one"])]
        self.assertIn("beta::one ran in parts 0 and 1", parts.check(LISTED, records, 2))

    def test_a_missing_or_extra_part_fails(self):
        self.assertTrue(any("no report from part(s) [1] of 2" in problem
                            for problem in parts.check(LISTED, [record(0, LISTED)], 2)))
        self.assertTrue(any("expected an index of 2 parts" in problem for problem in parts.check(
            LISTED, [record(0, LISTED[:2]), record(1, LISTED[2:]), record(2, [], count=3)], 2)))
        self.assertTrue(any("expected an index of 2 parts" in problem for problem in parts.check(
            LISTED, [record(0, LISTED[:2]), {"shard": None, "listed": LISTED, "executed": LISTED[2:]}], 2)))

    def test_a_part_that_ran_another_binary_fails(self):
        records = [record(0, LISTED[:2]), record(1, LISTED[2:], listed=[*LISTED, "delta::one"])]
        self.assertIn("part 1 ran a binary with a different test list than the build job's",
                      parts.check(LISTED, records, 2))

    def test_an_executed_test_outside_the_list_fails(self):
        records = [record(0, LISTED[:2]), record(1, [*LISTED[2:], "delta::one"])]
        self.assertTrue(any("1 executed test(s) are not in the list: ['delta::one']" in problem
                            for problem in parts.check(LISTED, records, 2)))

    def test_a_part_reporting_failed_units_or_a_repeat_fails(self):
        self.assertIn("part 1 reported 2 failed unit(s)",
                      parts.check(LISTED, [record(0, LISTED[:2]), record(1, LISTED[2:], failed=2)], 2))
        self.assertIn("part 0 executed a test twice",
                      parts.check(LISTED, [record(0, [*LISTED[:2], LISTED[0]]), record(1, LISTED[2:])], 2))

    def test_an_empty_or_repeating_list_fails(self):
        self.assertIn("the build job listed no tests", parts.check([], [record(0, [], count=1, listed=[])], 1))
        doubled = [*LISTED, LISTED[0]]
        self.assertIn("the build job's list names a test twice",
                      parts.check(doubled, [record(0, LISTED, count=1, listed=doubled)], 1))

    def test_a_rerun_part_counts_by_its_newest_attempt(self):
        stale = record(1, ["beta::one"])
        fresh = {**record(1, ["beta::one", "gamma::one"]), "attempt": 2}
        self.assertEqual(parts.check(LISTED, [record(0, LISTED[:2]), stale, fresh], 2), [])
        self.assertEqual(parts.check(LISTED, [record(0, LISTED[:2]), fresh, stale], 2), [])


class MainTests(unittest.TestCase):
    def run_main(self, records, listed=LISTED, count=2):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "lists" / "list-1").mkdir(parents=True)
            (root / "lists" / "list-1" / "tests.json").write_text(json.dumps({"tests": listed}), encoding="utf-8")
            for number, item in enumerate(records):
                folder = root / "parts" / f"executed-{number}"
                folder.mkdir(parents=True)
                (folder / "executed.json").write_text(json.dumps(item), encoding="utf-8")
            (root / "parts").mkdir(exist_ok=True)
            argv = ["check", "--listed", str(root / "lists"), "--parts", str(root / "parts"), "--count", str(count)]
            with patch.object(sys, "argv", argv), patch("builtins.print") as printed:
                return parts.main(), [call.args[0] for call in printed.call_args_list]

    def test_the_files_of_passing_parts_exit_zero(self):
        code, lines = self.run_main([record(0, LISTED[:2]), record(1, LISTED[2:])])
        self.assertEqual(code, 0)
        self.assertEqual(lines, ["Integration parts: 4 tests of the binary, each executed in exactly one of 2 parts"])

    def test_a_missing_part_file_exits_one(self):
        code, lines = self.run_main([record(0, LISTED[:2])])
        self.assertEqual(code, 1)
        self.assertTrue(any("no report from part(s) [1] of 2" in line for line in lines))

    def test_the_list_of_the_newest_attempt_is_used(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            # Attempt 10 sorts before attempt 9 by name; the attempt number decides.
            for attempt, tests in ((9, ["old::one"]), (10, LISTED)):
                folder = root / "lists" / f"list-{attempt}"
                folder.mkdir(parents=True)
                (folder / "tests.json").write_text(json.dumps({"attempt": attempt, "tests": tests}), encoding="utf-8")
            folder = root / "parts" / "executed-0"
            folder.mkdir(parents=True)
            (folder / "executed.json").write_text(json.dumps(record(0, LISTED, count=1)), encoding="utf-8")
            argv = ["check", "--listed", str(root / "lists"), "--parts", str(root / "parts"), "--count", "1"]
            with patch.object(sys, "argv", argv), patch("builtins.print"):
                self.assertEqual(parts.main(), 0)

    def test_no_list_file_is_an_error(self):
        with tempfile.TemporaryDirectory() as directory:
            argv = ["check", "--listed", directory, "--parts", directory, "--count", "1"]
            with patch.object(sys, "argv", argv), self.assertRaises(ValueError):
                parts.main()


if __name__ == "__main__":
    unittest.main()
