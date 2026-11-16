"""The memory guard kills the largest job process only when memory and swap are both nearly gone."""
import contextlib
import io
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import memory_guard as guard  # noqa: E402

GIB = 1024 * 1024  # kB


def meminfo(available_gib, swap_free_gib, total_gib=16, swap_total_gib=16):
    return (f"MemTotal:       {int(total_gib * GIB)} kB\nMemFree:  100 kB\n"
            f"MemAvailable:   {int(available_gib * GIB)} kB\nSwapTotal:      {int(swap_total_gib * GIB)} kB\n"
            f"SwapFree:       {int(swap_free_gib * GIB)} kB\nHugePages_Total:       0\n")


def status(name, rss_gib, swap_gib=0.0):
    return f"Name:\t{name}\nPid:\t1\nVmRSS:\t{int(rss_gib * GIB)} kB\nVmSwap:\t{int(swap_gib * GIB)} kB\n"


def fake_proc(root, meminfo_text, statuses):
    (root / "pressure").mkdir(parents=True)
    (root / "pressure" / "memory").write_text(
        "some avg10=12.50 avg60=1.00 avg300=0.10 total=1\nfull avg10=41.20 avg60=3.00 avg300=0.50 total=1\n")
    (root / "meminfo").write_text(meminfo_text)
    (root / "self").mkdir()  # not a process directory
    for pid, text in statuses.items():
        (root / str(pid)).mkdir()
        (root / str(pid) / "status").write_text(text)
    return root


class ParsingTests(unittest.TestCase):
    def test_meminfo_fields_are_read_in_kb(self):
        fields = guard.parse_meminfo(meminfo(2, 1))
        self.assertEqual((fields["MemTotal"], fields["MemAvailable"], fields["SwapFree"]), (16 * GIB, 2 * GIB, GIB))
        self.assertEqual(fields["HugePages_Total"], 0)

    def test_pressure_is_the_ten_second_full_average(self):
        self.assertEqual(guard.parse_pressure("some avg10=9.00 avg60=1.00\nfull avg10=41.20 avg60=3.00\n"), 41.2)
        self.assertEqual(guard.parse_pressure(""), 0.0)

    def test_a_kernel_thread_has_no_memory_to_report(self):
        self.assertIsNone(guard.parse_status("Name:\tkswapd0\nPid:\t80\n"))
        self.assertEqual(guard.parse_status(status("rustc", 9, 3)), ("rustc", 9 * GIB, 3 * GIB))


class DecisionTests(unittest.TestCase):
    def test_starved_needs_both_memory_and_swap_under_their_floors(self):
        cases = [(1.0, 1.0, True), (1.0, 4.0, False), (4.0, 1.0, False), (1.3, 1.3, False), (1.27, 1.27, True)]
        for available, swap_free, expected in cases:
            with self.subTest(available=available, swap_free=swap_free):
                self.assertEqual(guard.starved(guard.parse_meminfo(meminfo(available, swap_free)), 8, 8), expected)

    def test_without_swap_only_memory_decides(self):
        self.assertTrue(guard.starved(guard.parse_meminfo(meminfo(1, 0, swap_total_gib=0)), 8, 8))
        self.assertFalse(guard.starved(guard.parse_meminfo(meminfo(2, 0, swap_total_gib=0)), 8, 8))

    def test_the_victim_is_the_largest_unprotected_process_counting_swap(self):
        rows = [(1, "init", 30 * GIB, 0), (50, "Runner.Listener", 40 * GIB, 0), (51, "Runner.Worker", 40 * GIB, 0),
                (60, "rustc", 8 * GIB, 0), (61, "rustc", 5 * GIB, 6 * GIB), (62, "hosted-compute-", 50 * GIB, 0),
                (99, "python3", 60 * GIB, 0)]
        self.assertEqual(guard.choose_victim(rows, own_pid=99), (61, "rustc", 5 * GIB, 6 * GIB))

    def test_no_process_is_killed_when_none_holds_a_gibibyte(self):
        self.assertIsNone(guard.choose_victim([(60, "cargo", GIB // 2, 0), (1, "init", 9 * GIB, 0)], own_pid=99))
        self.assertIsNone(guard.choose_victim([], own_pid=99))


class WatchTests(unittest.TestCase):
    def watch(self, meminfo_text, statuses, **options):
        with tempfile.TemporaryDirectory() as folder:
            proc = fake_proc(Path(folder) / "proc", meminfo_text, statuses)
            log, killed, slept = Path(folder) / "guard.log", [], []
            guard.watch(log, 2.0, options.pop("record_every", 3600.0), 8.0, 8.0, proc=proc,
                        kill=lambda pid, sig: killed.append((pid, sig)), sleep=slept.append, **options)
            return log.read_text(encoding="utf-8").splitlines(), killed, slept

    def test_a_starved_runner_loses_its_largest_process_and_the_log_names_it(self):
        lines, killed, slept = self.watch(meminfo(0.5, 0.25), {
            1: status("systemd", 20), 300: status("Runner.Worker", 12), 400: status("rustc", 9, 14),
            401: status("eternal_integra", 6)}, rounds=1)
        self.assertEqual([pid for pid, _ in killed], [400])
        self.assertEqual(slept, [5.0])
        (line,) = [line for line in lines if f" {guard.KILLED} " in line]
        self.assertIn("pid=400 name=rustc rss=9216 swap=14336 outcome=killed available=512 swap_free=256", line)
        self.assertIn("largest_rss+swap=rustc:9216+14336,systemd:20480+0", line)

    def test_a_healthy_runner_is_only_recorded(self):
        lines, killed, _ = self.watch(meminfo(9, 15), {400: status("rustc", 5, 1)}, rounds=2, record_every=0.0)
        self.assertEqual(killed, [])
        records = [line for line in lines if "available_min=" in line]
        self.assertEqual(len(records), 2)
        self.assertIn("available_min=9216 swap_used_max=1024 memory_total=16384 swap_total=16384 "
                      "pressure_full10=41 largest_rss+swap=rustc:5120+1024", records[0])

    def test_a_failed_kill_is_recorded_and_the_guard_keeps_watching(self):
        def refuse(pid, sig):
            raise PermissionError(1, "Operation not permitted")

        with tempfile.TemporaryDirectory() as folder:
            proc = fake_proc(Path(folder) / "proc", meminfo(0.5, 0.25), {400: status("rustc", 9, 14)})
            log = Path(folder) / "guard.log"
            self.assertEqual(guard.watch(log, 2.0, 3600.0, 8.0, 8.0, proc=proc, kill=refuse, sleep=lambda _: None,
                                         rounds=2), 0)
            kills = [line for line in log.read_text(encoding="utf-8").splitlines() if f" {guard.KILLED} " in line]
        self.assertEqual(len(kills), 2)
        self.assertIn("outcome=kill failed (Operation not permitted)", kills[0])


class ReportTests(unittest.TestCase):
    RECORD = ["10:00:00 watching: kill the largest process when available memory is under 8% and free swap is under 8%",
              "10:01:00 available_min=9000 swap_used_max=10 memory_total=15701 swap_total=15359 pressure_full10=0 "
              "largest_rss+swap=rustc:5000+0",
              "10:02:00 available_min=310 swap_used_max=14100 memory_total=15701 swap_total=15359 pressure_full10=77 "
              "largest_rss+swap=rustc:9000+9000,rustc:5000+5000"]
    KILL = f"10:02:30 {guard.KILLED} pid=400 name=rustc rss=9000 swap=9000 outcome=killed available=300 swap_free=100"

    def test_the_summary_gives_the_peaks_of_the_record(self):
        summary, kills = guard.summarize(self.RECORD)
        self.assertEqual(summary, ["Memory guard: lowest available memory 310 MiB, highest swap use 14100 MiB "
                                   "of 15701 MiB memory and 15359 MiB swap."])
        self.assertEqual(kills, [])
        self.assertEqual(guard.summarize(self.RECORD[:1])[0], ["Memory guard: no record was written."])

    def test_a_kill_is_reported_as_an_error_and_in_the_step_summary(self):
        with tempfile.TemporaryDirectory() as folder:
            log, step_summary = Path(folder) / "guard.log", Path(folder) / "summary.md"
            log.write_text("\n".join(self.RECORD + [self.KILL]) + "\n", encoding="utf-8")
            printed = io.StringIO()
            with patch.dict(os.environ, {"GITHUB_STEP_SUMMARY": str(step_summary)}), \
                    contextlib.redirect_stdout(printed):
                self.assertEqual(guard.main(["report", "--log", str(log)]), 0)
            self.assertIn(f"::error title=Memory guard::{self.KILL}", printed.getvalue())
            written = step_summary.read_text(encoding="utf-8")
        self.assertIn("Memory guard killed 1 process(es) because memory and swap ran out", written)
        self.assertIn(f"- `{self.KILL}`", written)

    def test_a_missing_log_is_not_a_failure(self):
        printed = io.StringIO()
        with contextlib.redirect_stdout(printed):
            self.assertEqual(guard.main(["report", "--log", str(Path(tempfile.gettempdir()) / "no-such-guard.log")]), 0)
        self.assertIn("no log at", printed.getvalue())


if __name__ == "__main__":
    unittest.main()
