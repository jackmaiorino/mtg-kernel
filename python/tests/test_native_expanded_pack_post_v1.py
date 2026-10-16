"""The pack tool's post command: it runs on its one post CPU at below-normal
priority (its own process group on Linux, its own job on Windows), and every
process it starts is stopped when it exits or outlives its timeout. The
commands are short Python processes, started through this interpreter as a post
command would be."""
from __future__ import annotations

import json
import os
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

# Records its CPUs and priority, starts a child that appends a byte every 50 ms,
# waits for the first byte, then exits 3 or sleeps.
COMMAND = """\
import json, os, subprocess, sys, time
from pathlib import Path
record, counter, mode = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
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
record.write_text(json.dumps([cpus, priority]))
subprocess.Popen([sys.executable, "-c", "import sys, time\\nwhile True:\\n"
                  "    open(sys.argv[1], 'ab').write(b'.')\\n    time.sleep(0.05)", str(counter)])
while not counter.exists() or not counter.stat().st_size:
    time.sleep(0.02)
if mode == "sleep":
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

    def tearDown(self):
        self.scratch.cleanup()

    def post(self, mode, timeout):
        """The post's exit code and output. The output goes to a file, not a pipe a
        surviving process would hold open."""
        record, counter, log = (self.root / f"{mode}.{name}" for name in ("record", "count", "log"))
        with open(log, "w") as output:
            code = subprocess.run([sys.executable, "-B", str(PACK), "_post", str(self.cpu), str(timeout), "--",
                                   sys.executable, str(self.root / "command.py"), str(record), str(counter), mode],
                                  stdout=output, stderr=subprocess.STDOUT, timeout=300).returncode
        return code, log.read_text(), record, counter

    def assert_on_its_cpu_below_normal(self, record):
        cpus, priority = json.loads(record.read_text())
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


if __name__ == "__main__":
    unittest.main()
