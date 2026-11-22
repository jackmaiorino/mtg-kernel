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
    def test_the_table_names_exactly_the_two_slow_cases(self):
        table = runner.case_timeouts()
        self.assertEqual(table, dict.fromkeys(SLOW, 3000))
        raw = json.loads(runner.CASE_TIMEOUTS_PATH.read_text())
        self.assertIn("_comment", raw)

    def test_a_bad_or_missing_table_stops_the_run(self):
        import tempfile
        for content in ('{"a::b": true}', '{"a::b": 0}', '{"a::b": 12.5}', '["a::b"]', None):
            with self.subTest(content=content), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "limits.json"
                if content is not None:
                    path.write_text(content)
                with patch.object(runner, "CASE_TIMEOUTS_PATH", path), self.assertRaises((ValueError, OSError)):
                    runner.case_timeouts()

    def run_cases(self, run_seconds, timeout=None):
        clock = Clock()
        events = []
        with patch.object(runner.subprocess, "Popen", fake_popen_factory(clock, run_seconds)), \
                patch.object(runner.time, "monotonic", clock.monotonic), \
                patch.object(runner, "case_resources", return_value=(1, {})), \
                patch.object(runner.sys.stdout, "write", side_effect=lambda text: events.append(text)):
            code = runner.run_isolated_cases("tests.exe", list(run_seconds), timeout, [])
        rows = [json.loads(line) for text in events for line in text.splitlines() if line.startswith("{")]
        summary = next(row for row in rows if row.get("event") == "rust_isolated_case_summary")
        return code, {row["case"]: row["status"] for row in summary["results"]}, rows

    def test_a_named_case_may_run_past_the_default_but_not_past_its_own_limit(self):
        cases = dict.fromkeys(SLOW, 2500)
        cases["other::case"] = 10
        code, status, rows = self.run_cases(cases)
        self.assertEqual((code, status), (0, dict.fromkeys(cases, "passed")))
        started = {row["case"]: row["timeout_seconds"] for row in rows if row.get("event") == "rust_test_case_started"}
        self.assertEqual(started, {**dict.fromkeys(SLOW, 3000), "other::case": 1800})
        wide = sorted(SLOW)[1]
        code, status, _ = self.run_cases({wide: 3500})
        self.assertEqual((code, status[wide]), (124, "timeout"))

    def test_an_explicit_limit_overrides_a_named_case(self):
        wide = sorted(SLOW)[1]
        for limit in (45, 1800, 3600):
            with self.subTest(limit=limit):
                code, status, rows = self.run_cases({wide: limit + 1, "other::case": 10}, timeout=limit)
                self.assertEqual((code, status[wide], status["other::case"]), (124, "timeout", "passed"))
                started = {row["case"]: row["timeout_seconds"] for row in rows
                           if row.get("event") == "rust_test_case_started"}
                self.assertEqual(started, {wide: limit, "other::case": limit})
        code, status, _ = self.run_cases({wide: 3200}, timeout=3600)
        self.assertEqual((code, status[wide]), (0, "passed"))

    def test_cargo_runner_preserves_whether_the_limit_was_explicit(self):
        wide = sorted(SLOW)[1]
        for limit in (None, 1800):
            with self.subTest(limit=limit):
                args = ["runner", "--features", "limited-fdn-fixtures", "--shard-index", "0", "--shard-count", "1",
                        "--isolated-cases", "human_bo3_v1::"]
                if limit is not None:
                    args.extend(["--filter-timeout-seconds", str(limit)])
                with patch.dict(runner.os.environ, {runner.FILTER_TIMEOUT_ENV: "99"}), \
                        patch.object(sys, "argv", args), patch.object(runner.subprocess, "run") as cargo:
                    cargo.return_value.returncode = 0
                    self.assertEqual(runner.main(), 0)
                env = cargo.call_args.kwargs["env"]
                self.assertEqual(env.get(runner.FILTER_TIMEOUT_ENV), None if limit is None else str(limit))
                with patch.dict(runner.os.environ, env, clear=True), \
                        patch.object(sys, "argv", ["runner", "--run", "tests.exe"]), \
                        patch.object(runner, "compiled_names", side_effect=[[wide], []]), \
                        patch.object(runner, "run_isolated_cases", return_value=0) as isolated, \
                        patch("builtins.print"):
                    self.assertEqual(runner.main(), 0)
                isolated.assert_called_once_with("tests.exe", [wide], limit, [])

    def test_nonisolated_release_filters_keep_the_default_limit(self):
        env = {runner.FILTERS_ENV: json.dumps(["current_mixed_policy_v1::"])}
        with patch.dict(runner.os.environ, env, clear=True), \
                patch.object(sys, "argv", ["runner", "--run", "tests.exe"]), \
                patch.object(runner.subprocess, "run") as run, \
                patch.object(runner, "case_timeouts") as case_timeouts, \
                patch("builtins.print"):
            run.return_value.returncode = 0
            self.assertEqual(runner.main(), 0)
        run.assert_called_once_with(["tests.exe", "current_mixed_policy_v1::"], check=False, timeout=1800)
        case_timeouts.assert_not_called()

    def test_every_other_case_keeps_the_default(self):
        code, status, _ = self.run_cases({"other::slow_case": 1900})
        self.assertEqual((code, status["other::slow_case"]), (124, "timeout"))


if __name__ == "__main__":
    unittest.main()
