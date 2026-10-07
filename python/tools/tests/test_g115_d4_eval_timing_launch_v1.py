"""Refusal and owned-handle checks. Never dispatch MTG."""
import copy
import os
from pathlib import Path
import subprocess
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from g115_d4_eval_timing_launch_v1 import validate_limits, admission, cpu_seconds


class TimingAdmissionTests(unittest.TestCase):
    def setUp(self):
        self.plan = dict(schema='g115-d4-bounded-eval-timing/v1', formal_measurement=False,
                         workers=[1, 4, 8], job_seconds=180, total_seconds=3600)

    def test_valid_bounds(self):
        for host in ('desktop', 'computehost'):
            validate_limits(self.plan, host)

    def test_formal_cloud_and_altered_bounds_refused(self):
        with self.assertRaises(ValueError):
            validate_limits(self.plan, 'runpod')
        for key, value in [('schema', 'formal'), ('formal_measurement', True),
                           ('workers', [1, 4, 16]), ('job_seconds', 181), ('total_seconds', 3601)]:
            plan = copy.deepcopy(self.plan)
            plan[key] = value
            with self.assertRaises(ValueError):
                validate_limits(plan, 'desktop')

    @unittest.skipUnless(os.name == 'nt', 'Windows handle telemetry')
    def test_owned_handle_cpu_survives_child_exit(self):
        child = subprocess.Popen([sys.executable, '-c', 'import sys;sys.stdin.read();sum(range(1000000))'],
                                 stdin=subprocess.PIPE, creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            before = cpu_seconds(child)
            child.stdin.close()
            child.wait(timeout=10)
            self.assertGreaterEqual(cpu_seconds(child), before)
        finally:
            if child.poll() is None:
                child.kill()
                child.wait()


if __name__ == '__main__':
    unittest.main()
