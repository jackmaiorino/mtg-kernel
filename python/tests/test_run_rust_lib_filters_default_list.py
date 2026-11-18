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
# Every test sees this job identity unless it sets its own, so no test runs git or rustc.
PROVENANCE = {"commit": "0" * 40, "features": "default", "rustc_host": "x86_64-unknown-linux-gnu",
              "rustc_release": "1.94.1"}
REAL_PROVENANCE = filters.list_provenance
CURRENT_LIST = {"schema": filters.DEFAULT_LIST_SCHEMA, "provenance": PROVENANCE, "count": 3, "names": NAMES}


def refuse_build():
    raise AssertionError("the default library was built")


class DefaultListTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.path = Path(self.directory.name) / "default-library-list.json"
        provenance = patch.object(filters, "list_provenance", lambda: dict(PROVENANCE))
        provenance.start()
        self.addCleanup(provenance.stop)

    def tearDown(self):
        self.directory.cleanup()

    def test_a_written_list_reads_back_unchanged(self):
        with patch.object(filters, "default_library_names", lambda: list(NAMES)), patch("builtins.print"):
            self.assertEqual(filters.write_default_library_list(self.path), NAMES)
            self.assertEqual(filters.read_default_library_list(self.path), NAMES)

    def test_a_damaged_list_is_refused(self):
        good = CURRENT_LIST
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
        self.path.write_text(json.dumps(CURRENT_LIST), encoding="utf-8")
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
        self.assertEqual(json.loads(self.path.read_text()), CURRENT_LIST)
        for extra in (["--features", "x"], ["some_filter::"], ["--shard-index", "0", "--shard-count", "1"],
                      ["--isolated-cases"], ["--skip-ignored"], ["--nocapture"],
                      ["--filter-timeout-seconds", "1800"], ["--filter-timeout-seconds", "0"],
                      ["--default-library-list", "other.json"],
                      ["--limited-only-workflow", str(ROOT / ".github" / "workflows" / "ci.yml")]):
            with self.subTest(extra=extra), patch("sys.stderr"), self.assertRaises(SystemExit):
                self.run_main("--write-default-library-list", str(self.path), *extra)

    def test_the_list_option_needs_the_limited_only_workflow(self):
        with patch("sys.stderr"), self.assertRaises(SystemExit):
            self.run_main("--features", "x", "--default-library-list", str(self.path), "some_filter::")
        with patch("sys.stderr"), self.assertRaises(SystemExit):
            self.run_main("some_filter::")

    def test_a_list_from_another_revision_is_refused_before_any_case_is_dropped(self):
        # Revision A listed example::case in its default build; at revision B the case is Limited-only. A's
        # list would subtract it from B's Limited inventory, so it would run nowhere. The shard must refuse
        # A's list before it builds or runs anything.
        stale = {"schema": filters.DEFAULT_LIST_SCHEMA, "count": 2, "names": ["alpha::one", "example::case"],
                 "provenance": {"commit": "a" * 40, "features": "default", "rustc_host": "x86_64-unknown-linux-gnu",
                                "rustc_release": "1.94.1"}}
        current = {**stale["provenance"], "commit": "b" * 40}
        self.path.write_text(json.dumps(stale), encoding="utf-8")
        with patch.object(filters, "list_provenance", lambda: current), \
                patch.object(filters, "default_library_names", refuse_build), \
                self.assertRaisesRegex(ValueError, "another revision or build"):
            self.run_main(
                "--features", "limited-fdn-fixtures", "--shard-index", "0", "--shard-count", "3",
                "--isolated-cases", "--skip-ignored", "--limited-only-workflow",
                str(ROOT / ".github" / "workflows" / "ci.yml"), "--default-library-list", str(self.path))

    def test_a_list_from_another_build_identity_is_refused(self):
        provenance = {"commit": "c" * 40, "features": "default", "rustc_host": "x86_64-pc-windows-msvc",
                      "rustc_release": "1.94.1"}
        self.path.write_text(json.dumps({"schema": filters.DEFAULT_LIST_SCHEMA, "count": 3, "names": NAMES,
                                         "provenance": provenance}), encoding="utf-8")
        for change in ({"rustc_host": "x86_64-unknown-linux-gnu"}, {"rustc_release": "1.95.0"},
                       {"features": "limited-fdn-fixtures"}):
            with self.subTest(change=change), patch("builtins.print"), \
                    patch.object(filters, "list_provenance", lambda: {**provenance, **change}), \
                    self.assertRaisesRegex(ValueError, "another revision or build"):
                filters.read_default_library_list(self.path)
        with patch("builtins.print"), patch.object(filters, "list_provenance", lambda: dict(provenance)):
            self.assertEqual(filters.read_default_library_list(self.path), NAMES)

    def test_a_list_is_refused_when_this_checkout_cannot_name_its_revision(self):
        provenance = {"commit": None, "features": "default", "rustc_host": "h", "rustc_release": "r"}
        self.path.write_text(json.dumps({"schema": filters.DEFAULT_LIST_SCHEMA, "count": 3, "names": NAMES,
                                         "provenance": {**provenance, "commit": "d" * 40}}), encoding="utf-8")
        with patch("builtins.print"), patch.object(filters, "list_provenance", lambda: dict(provenance)):
            with patch.dict(filters.os.environ, {}, clear=False), self.assertRaisesRegex(ValueError, "cannot name"):
                filters.os.environ.pop(filters.UNVERIFIED_LIST_ENV, None)
                filters.read_default_library_list(self.path)
            with patch.dict(filters.os.environ, {filters.UNVERIFIED_LIST_ENV: "1"}):
                self.assertEqual(filters.read_default_library_list(self.path), NAMES)

    def test_the_provenance_names_the_commit_and_the_compiler(self):
        version = "rustc 1.94.1 (e408947bf 2026-03-25)\nbinary: rustc\nhost: aarch64-apple-darwin\nrelease: 1.94.1\n"

        def run(command, **_):
            output = version if command[0] == "rustc" else "f" * 40 + "\n"
            return type("Result", (), {"returncode": 0, "stdout": output})()

        with patch.object(filters.subprocess, "run", run), patch.dict(filters.os.environ, {"GITHUB_SHA": "e" * 40}):
            self.assertEqual(REAL_PROVENANCE(), {"commit": "e" * 40, "features": "default",
                                                         "rustc_host": "aarch64-apple-darwin",
                                                         "rustc_release": "1.94.1"})
        with patch.object(filters.subprocess, "run", run), patch.dict(filters.os.environ, {}, clear=False):
            filters.os.environ.pop("GITHUB_SHA", None)
            self.assertEqual(REAL_PROVENANCE()["commit"], "f" * 40)

        def missing(command, **_):
            raise OSError("not installed")

        with patch.object(filters.subprocess, "run", missing), patch.dict(filters.os.environ, {}, clear=False):
            filters.os.environ.pop("GITHUB_SHA", None)
            self.assertEqual(REAL_PROVENANCE(), {"commit": None, "features": "default", "rustc_host": None,
                                                 "rustc_release": None})

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
