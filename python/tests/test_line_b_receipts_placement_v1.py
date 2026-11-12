"""Pre-work barrier of the line (b) receipts driver's spawn (director ruling
CLAUDE #611, CODEX #619): the child runs no instruction before its placement
is read back, and a failed placement kills it before it runs. The children
are short Python processes, never a native engine."""
from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
TOOLS = REPO_ROOT / "python" / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

if os.name == "nt":
    import line_b_receipts_v1 as driver  # noqa: E402


def first_allowed_cpu_mask():
    """The lowest CPU this process may run on, as an affinity mask. A CI job
    confined to some CPUs (a self-hosted runner's job object) cannot place a
    child outside them, so CPU 0 is not always available."""
    import ctypes
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.GetCurrentProcess.restype = ctypes.c_void_p
    kernel.GetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_size_t),
                                              ctypes.POINTER(ctypes.c_size_t)]
    process_mask, system_mask = ctypes.c_size_t(), ctypes.c_size_t()
    if not kernel.GetProcessAffinityMask(kernel.GetCurrentProcess(), ctypes.byref(process_mask),
                                         ctypes.byref(system_mask)):
        raise ctypes.WinError(ctypes.get_last_error())
    return process_mask.value & -process_mask.value


def marker_child(marker):
    """A child whose first statement writes `marker`."""
    return [sys.executable, "-c", f"open({str(marker)!r}, 'w').write('ran')"]


@unittest.skipUnless(os.name == "nt", "Windows placement")
class SpawnPlacedTest(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.marker = Path(self.scratch.name) / "started"

    def tearDown(self):
        self.scratch.cleanup()

    def test_the_child_runs_only_after_the_readback(self):
        resumed = []
        mask = first_allowed_cpu_mask()
        original = driver.placement_api

        def observed_api():
            ctypes, kernel, ntdll = original()

            class Ntdll:
                @staticmethod
                def NtResumeProcess(handle):
                    # The readback is done and the child has not run: the
                    # marker is absent even after a pause.
                    time.sleep(0.5)
                    resumed.append(self.marker.exists())
                    return ntdll.NtResumeProcess(handle)

            return ctypes, kernel, Ntdll

        driver.placement_api = observed_api
        try:
            process, readback = driver.spawn_placed(marker_child(self.marker), sys.executable, mask)
        finally:
            driver.placement_api = original
        self.assertEqual(process.wait(timeout=60), 0)
        self.assertEqual(resumed, [False])
        self.assertTrue(self.marker.exists())
        after = readback["after"]
        self.assertTrue(readback["verified"])
        self.assertTrue(readback["set_before_resume"])
        self.assertEqual(after["affinity_mask"], mask)
        self.assertEqual(after["priority_class"], subprocess.BELOW_NORMAL_PRIORITY_CLASS)
        self.assertTrue(after["power_control_mask"] & 1)
        self.assertFalse(after["power_state_mask"] & 1)

    def test_a_failed_readback_kills_the_child_before_it_runs(self):
        # The owned-handle identity check refuses a different executable.
        other = Path(self.scratch.name) / "other.exe"
        other.write_bytes(b"")
        with self.assertRaises(ValueError):
            driver.spawn_placed(marker_child(self.marker), other, None)
        time.sleep(0.5)
        self.assertFalse(self.marker.exists())

    def test_a_mask_outside_the_host_is_refused_before_the_child_runs(self):
        with self.assertRaises(ValueError):
            driver.spawn_placed(marker_child(self.marker), sys.executable, 1 << 63)
        time.sleep(0.5)
        self.assertFalse(self.marker.exists())


if __name__ == "__main__":
    unittest.main()
