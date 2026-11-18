"""The Limited-only shards read the default-feature case list that one build job wrote, instead of building it."""
from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1] / "tools"
ROOT = Path(__file__).resolve().parents[2]
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import run_rust_lib_filters as filters  # noqa: E402

NAMES = ["alpha::one", "beta::two", "gamma::three"]


def refuse_build():
    raise AssertionError("the default library was built")


class DefaultListTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.path = Path(self.directory.name) / "default-library-list.json"

    def tearDown(self):
        self.directory.cleanup()

    def test_a_written_list_reads_back_unchanged(self):
        with patch.object(filters, "default_library_names", lambda: list(NAMES)), patch("builtins.print"):
            self.assertEqual(filters.write_default_library_list(self.path), NAMES)
            self.assertEqual(filters.read_default_library_list(self.path), NAMES)

    def test_a_damaged_list_is_refused(self):
        good = {"schema": filters.DEFAULT_LIST_SCHEMA, "count": 3, "names": NAMES}
        for record in ({**good, "schema": "other"}, {**good, "count": 2}, {**good, "names": []},
                       {**good, "count": 2, "names": ["beta::two", "alpha::one"]},
                       {**good, "names": ["a", "a", "b"]}, [NAMES]):
            with self.subTest(record=record):
                self.path.write_text(json.dumps(record), encoding="utf-8")
                with self.assertRaises(ValueError), patch("builtins.print"):
                    filters.read_default_library_list(self.path)

    def run_main(self, *arguments):
        calls = []

        def run(command, env=None, **_):
            calls.append((command, env))
            return type("Result", (), {"returncode": 0})()

        argv = ["run_rust_lib_filters.py", *arguments]
        with patch.object(filters.subprocess, "run", run), patch.object(sys, "argv", argv), \
                patch("builtins.print"):
            code = filters.main()
        return code, calls

    def test_a_limited_only_shard_takes_the_default_cases_from_the_list(self):
        self.path.write_text(json.dumps({"schema": filters.DEFAULT_LIST_SCHEMA, "count": 3, "names": NAMES}),
                             encoding="utf-8")
        with patch.object(filters, "default_library_names", refuse_build):
            code, calls = self.run_main(
                "--features", "limited-fdn-fixtures", "--shard-index", "1", "--shard-count", "3",
                "--isolated-cases", "--skip-ignored", "--limited-only-workflow",
                str(ROOT / ".github" / "workflows" / "ci.yml"), "--default-library-list", str(self.path))
        self.assertEqual(code, 0)
        ((command, environment),) = calls
        self.assertIn("--lib", command)
        selection = json.loads(Path(environment[filters.LIMITED_ONLY_ENV]).read_text())
        self.assertEqual(selection["default"], NAMES)
        self.assertEqual(selection["covered"],
                         filters.workflow_limited_filters(ROOT / ".github" / "workflows" / "ci.yml"))
        self.assertEqual(json.loads(environment[filters.SHARD_ENV]), [1, 3])

    def test_the_write_mode_runs_no_test_and_takes_no_other_option(self):
        with patch.object(filters, "default_library_names", lambda: list(NAMES)):
            code, calls = self.run_main("--write-default-library-list", str(self.path))
        self.assertEqual((code, calls), (0, []))
        self.assertEqual(json.loads(self.path.read_text())["names"], NAMES)
        for extra in (["--features", "x"], ["some_filter::"], ["--shard-index", "0", "--shard-count", "1"]):
            with self.subTest(extra=extra), patch("sys.stderr"), self.assertRaises(SystemExit):
                self.run_main("--write-default-library-list", str(self.path), *extra)

    def test_the_list_option_needs_the_limited_only_workflow(self):
        with patch("sys.stderr"), self.assertRaises(SystemExit):
            self.run_main("--features", "x", "--default-library-list", str(self.path), "some_filter::")
        with patch("sys.stderr"), self.assertRaises(SystemExit):
            self.run_main("some_filter::")

    def test_without_the_list_a_shard_still_builds_the_default_library(self):
        with patch.object(filters, "default_library_names", lambda: list(NAMES)):
            code, calls = self.run_main(
                "--features", "limited-fdn-fixtures", "--shard-index", "0", "--shard-count", "1",
                "--isolated-cases", "--skip-ignored", "--limited-only-workflow",
                str(ROOT / ".github" / "workflows" / "ci.yml"))
        selection = json.loads(Path(calls[0][1][filters.LIMITED_ONLY_ENV]).read_text())
        self.assertEqual((code, selection["default"]), (0, NAMES))


if __name__ == "__main__":
    unittest.main()
