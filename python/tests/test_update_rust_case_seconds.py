"""The case-seconds table refreshes from saved job logs as from a run read through the GitHub CLI."""
from __future__ import annotations

import gzip
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1] / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import update_rust_case_seconds as update  # noqa: E402


def record(case, seconds, status="passed"):
    return ("2026-11-18T19:50:00.0000000Z " + json.dumps({"event": "rust_test_case_finished", "case": case,
                                                          "status": status, "returncode": 0,
                                                          "elapsed_seconds": seconds}) + "\n")


class UpdateTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.table = self.root / "rust_case_seconds.json"
        self.table.write_text(json.dumps({"kept::case": 7, "old::case": 3}), encoding="utf-8")

    def run_main(self, *arguments):
        with patch.object(update, "CASE_SECONDS_PATH", self.table), \
                patch.object(update, "gh", lambda *_: self.fail("the GitHub CLI was called")), \
                patch.object(sys, "argv", ["update_rust_case_seconds.py", *arguments]), patch("builtins.print"):
            return update.main()

    def test_saved_logs_replace_the_times_of_the_cases_they_ran(self):
        plain = self.root / "shard_1.txt"
        plain.write_text("noise\n" + record("old::case", 100.4) + record("new::case", 0.2)
                         + record("failed::case", 9, "failed"), encoding="utf-8")
        packed = self.root / "shard_2.txt.gz"
        with gzip.open(packed, "wt", encoding="utf-8") as handle:
            handle.write(record("old::case", 120.6))
        self.assertEqual(self.run_main("--logs", str(plain), str(packed)), 0)
        self.assertEqual(update.case_times(update.file_logs([str(plain), str(packed)])),
                         {"old::case": [100.4, 120.6], "new::case": [0.2]})
        # A failed case keeps no time; a case under a second counts as one.
        self.assertEqual(json.loads(self.table.read_text()), {"kept::case": 7, "new::case": 1, "old::case": 110})

    def test_logs_without_any_passed_case_change_nothing(self):
        empty = self.root / "empty.txt"
        empty.write_text(record("x::case", 5, "failed"), encoding="utf-8")
        with self.assertRaises(SystemExit):
            self.run_main("--logs", str(empty))
        self.assertEqual(json.loads(self.table.read_text()), {"kept::case": 7, "old::case": 3})


if __name__ == "__main__":
    unittest.main()
