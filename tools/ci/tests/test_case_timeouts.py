"""Only the named isolated Rust cases get a longer time limit; every other case keeps the default."""
import json
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[3] / "python" / "tools"
sys.path.insert(0, str(TOOLS))
import run_rust_lib_filters as runner  # noqa: E402

SLOW = {"human_bo3_v1::next_five_v59::wide_tests::paid_wide_human_and_opponent_original_indices_native_parity_and_stale_restore_both_seats",
        "current_mixed_policy_v1::liliana::tests::paid_numeric_all_three_original_loyalty_roots_and_native_choice_restore_both_seats"}


class Clock:
    def __init__(self):
        self.now = 0.0

    def monotonic(self):
        return self.now


def fake_popen_factory(clock, run_seconds):
    """A child that exits 0 once its case has run `run_seconds[case]` fake seconds."""
    class Child:
        def __init__(self, command):
            self.case = command[command.index("--exact") + 1]
            self.end = clock.now + run_seconds[self.case]
            self.pid, self.returncode, self.killed = 1, None, False

        def poll(self):
            return self.returncode

        def wait(self, timeout=None):
            if self.returncode is not None:
                return self.returncode
            step = timeout if timeout is not None else self.end - clock.now
            if clock.now + step >= self.end:
                clock.now = self.end
                self.returncode = 0
                return 0
            clock.now += step
            raise subprocess.TimeoutExpired("case", timeout)

        def kill(self):
            self.killed, self.returncode = True, -9
    return Child


class CaseTimeoutTests(unittest.TestCase):
    def test_the_table_names_exactly_the_two_slow_cases_with_a_limit_above_their_slowest_pass(self):
        table = runner.case_timeouts()
        self.assertEqual(set(table), SLOW)
        for name, limit in table.items():
            self.assertGreaterEqual(limit, 1750 * 1.5, name)   # slowest observed pass: 1750 s
        raw = json.loads(runner.CASE_TIMEOUTS_PATH.read_text())
        self.assertIn("_comment", raw)

    def run_cases(self, run_seconds, default=1800):
        clock = Clock()
        events = []
        with patch.object(runner.subprocess, "Popen", fake_popen_factory(clock, run_seconds)), \
                patch.object(runner.time, "monotonic", clock.monotonic), \
                patch.object(runner, "case_resources", return_value=(1, {})), \
                patch.object(runner.sys.stdout, "write", side_effect=lambda text: events.append(text)):
            code = runner.run_isolated_cases("tests.exe", list(run_seconds), default, [])
        rows = [json.loads(line) for text in events for line in text.splitlines() if line.startswith("{")]
        summary = next(row for row in rows if row.get("event") == "rust_isolated_case_summary")
        return code, {row["case"]: row["status"] for row in summary["results"]}, rows

    def test_a_named_case_may_run_past_the_default_but_not_past_its_own_limit(self):
        wide = sorted(SLOW)[1]
        code, status, rows = self.run_cases({wide: 2500, "other::case": 10})
        self.assertEqual((code, status[wide], status["other::case"]), (0, "passed", "passed"))
        started = {row["case"]: row["timeout_seconds"] for row in rows if row.get("event") == "rust_test_case_started"}
        self.assertEqual(started, {wide: 3000, "other::case": 1800})
        code, status, _ = self.run_cases({wide: 3500})
        self.assertEqual((code, status[wide]), (124, "timeout"))

    def test_every_other_case_keeps_the_default(self):
        code, status, _ = self.run_cases({"other::slow_case": 1900})
        self.assertEqual((code, status["other::slow_case"]), (124, "timeout"))


if __name__ == "__main__":
    unittest.main()
