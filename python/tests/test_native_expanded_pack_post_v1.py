"""The pack tool's post command: it runs on its one post CPU at below-normal
priority (in the tool's process group on Linux, with the wrapper as the
subreaper of all it starts; in its own job on Windows), and every process it
starts, even one in a session of its own, is stopped when it exits, outlives its
timeout or its wrapper is stopped. The commands are short Python processes,
started through this interpreter as a post command would be."""
from __future__ import annotations

import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[2] / "python" / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))
import host_slots_v1 as slots  # noqa: E402

PACK = TOOLS / "native_expanded_pack_v1.py"

# Starts a child that appends a byte every 50 ms (in a session of its own for a
# "session-" mode), records its CPUs, priority, process group and both pids,
# waits for the first byte, then exits 3 or sleeps.
COMMAND = """\
import json, os, subprocess, sys, time
from pathlib import Path
record, counter, mode = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
child = subprocess.Popen([sys.executable, "-c", "import sys, time\\nwhile True:\\n"
                          "    open(sys.argv[1], 'ab').write(b'.')\\n    time.sleep(0.05)", str(counter)],
                         start_new_session=mode.startswith("session-"))
if os.name == "nt":
    import ctypes
    kernel = ctypes.windll.kernel32
    kernel.GetCurrentProcess.restype = ctypes.c_void_p
    kernel.GetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_size_t),
                                              ctypes.POINTER(ctypes.c_size_t)]
    kernel.GetPriorityClass.argtypes = [ctypes.c_void_p]
    mask, system = ctypes.c_size_t(0), ctypes.c_size_t(0)
    assert kernel.GetProcessAffinityMask(kernel.GetCurrentProcess(), ctypes.byref(mask), ctypes.byref(system))
    cpus, priority = [cpu for cpu in range(64) if mask.value >> cpu & 1], kernel.GetPriorityClass(kernel.GetCurrentProcess())
else:
    cpus, priority = sorted(os.sched_getaffinity(0)), os.getpriority(os.PRIO_PROCESS, 0)
group = None if os.name == "nt" else os.getpgrp()
record.write_text(json.dumps([cpus, priority, group, os.getpid(), child.pid]))
while not counter.exists() or not counter.stat().st_size:
    time.sleep(0.02)
if mode.endswith("sleep"):
    time.sleep(600)
sys.exit(3)
"""


def appended(path):
    try:
        return path.stat().st_size
    except FileNotFoundError:
        return 0


class PostTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        self.root = Path(self.scratch.name)
        (self.root / "command.py").write_text(COMMAND, encoding="utf-8")
        self.cpu = slots.current_affinity()[-1]
        self.wrappers = []

    def tearDown(self):
        for wrapper in self.wrappers:
            if wrapper.poll() is None:
                wrapper.kill()
                wrapper.wait()
        if os.name != "nt":  # what a failed test left running: only processes of this scratch
            for record in self.root.glob("*.record"):
                try:
                    pids = json.loads(record.read_text())[3:]
                except ValueError:
                    continue
                for pid in pids:
                    try:
                        if str(self.root).encode() in Path(f"/proc/{pid}/cmdline").read_bytes():
                            os.kill(pid, signal.SIGKILL)
                    except OSError:
                        pass
        self.scratch.cleanup()

    def files(self, label):
        return tuple(self.root / f"{label}.{name}" for name in ("record", "count", "log"))

    def argv(self, mode, timeout, record, counter):
        return [sys.executable, "-B", str(PACK), "_post", str(self.cpu), str(timeout), "--",
                sys.executable, str(self.root / "command.py"), str(record), str(counter), mode]

    def post(self, mode, timeout):
        """The post's exit code and output. The output goes to a file, not a pipe a
        surviving process would hold open."""
        record, counter, log = self.files(mode)
        with open(log, "w") as output:
            code = subprocess.run(self.argv(mode, timeout, record, counter), stdout=output,
                                  stderr=subprocess.STDOUT, timeout=300).returncode
        return code, log.read_text(), record, counter

    def running_post(self, label, mode):
        """A post wrapper in a process group of its own, as the pack tool's is under
        host_slots timed, once its command and the command's child run."""
        record, counter, log = self.files(label)
        with open(log, "w") as output:
            wrapper = subprocess.Popen(self.argv(mode, 600, record, counter), stdout=output,
                                       stderr=subprocess.STDOUT, start_new_session=True)
        self.wrappers.append(wrapper)
        deadline = time.monotonic() + 60
        while not (record.exists() and record.stat().st_size and appended(counter)):
            self.assertIsNone(wrapper.poll(), log.read_text())
            self.assertLess(time.monotonic(), deadline, "the post command did not start")
            time.sleep(0.05)
        return wrapper, record, counter

    def assert_on_its_cpu_below_normal(self, record):
        cpus, priority = json.loads(record.read_text())[:2]
        self.assertEqual(cpus, [self.cpu])
        if os.name == "nt":
            self.assertEqual(priority, subprocess.BELOW_NORMAL_PRIORITY_CLASS)
        else:
            self.assertEqual(priority, min(19, os.getpriority(os.PRIO_PROCESS, 0) + 10))

    def assert_stopped(self, counter):
        time.sleep(0.5)  # killed processes end asynchronously
        before = appended(counter)
        self.assertGreater(before, 0, "the command's child never ran")
        time.sleep(1.0)
        self.assertEqual(appended(counter), before, "a process the post command started still runs")

    def test_what_a_post_started_stops_when_it_exits(self):
        code, output, record, counter = self.post("exit", 120)
        self.assertEqual(code, 3, output)
        self.assert_on_its_cpu_below_normal(record)
        self.assert_stopped(counter)

    def test_a_post_that_outlives_its_timeout_is_stopped_with_what_it_started(self):
        code, output, record, counter = self.post("sleep", 10)
        self.assertEqual(code, 124, output)
        self.assertIn("post command stopped after 10", output)
        self.assert_on_its_cpu_below_normal(record)
        self.assert_stopped(counter)

    def test_a_process_the_post_started_in_a_session_of_its_own_stops_too(self):
        code, output, _, counter = self.post("session-exit", 120)
        self.assertEqual(code, 3, output)
        self.assert_stopped(counter)

    @unittest.skipIf(os.name == "nt", "a process group is the Linux unit of a host_slots tree")
    def test_a_kill_of_the_tools_process_group_reaches_the_post_command(self):
        wrapper, record, counter = self.running_post("group", "sleep")
        self.assertEqual(json.loads(record.read_text())[2], wrapper.pid)  # the command is in the wrapper's group
        os.killpg(wrapper.pid, signal.SIGKILL)
        self.assertEqual(wrapper.wait(timeout=60), -signal.SIGKILL)
        self.assert_stopped(counter)

    @unittest.skipIf(os.name == "nt", "on Windows the wrapper's job is closed when it ends")
    def test_a_post_wrapper_stopped_by_a_signal_stops_what_it_started(self):
        for number in (signal.SIGHUP, signal.SIGINT, signal.SIGTERM):
            with self.subTest(signal=number.name):
                wrapper, _, counter = self.running_post(number.name, "sleep")
                os.kill(wrapper.pid, number)  # the wrapper alone, not its group
                self.assertEqual(wrapper.wait(timeout=60), 128 + number)
                self.assert_stopped(counter)

    @unittest.skipIf(os.name == "nt", "on Windows the wrapper's job is closed when it ends")
    def test_a_second_signal_does_not_cut_the_stop_short(self):
        for attempt in range(3):
            with self.subTest(attempt=attempt):
                wrapper, _, counter = self.running_post(f"twice-{attempt}", "sleep")
                os.kill(wrapper.pid, signal.SIGINT)  # back to back, before the wrapper acts on either
                os.kill(wrapper.pid, signal.SIGTERM)
                self.assertIn(wrapper.wait(timeout=60), (128 + signal.SIGINT, 128 + signal.SIGTERM))
                self.assert_stopped(counter)


if __name__ == "__main__":
    unittest.main()
