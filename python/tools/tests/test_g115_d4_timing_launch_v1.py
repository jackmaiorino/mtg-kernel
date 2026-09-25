"""Offline refusal checks; deliberately never dispatch MTG or GPU work."""
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from g115_d4_timing_launch_v1 import admission


class AdmissionTests(unittest.TestCase):
    def setUp(self):
        self.plan = dict(schema='g115-d4-bounded-timing/v1', configs=dict(control={}, broader={}),
                         workers=[1, 4, 8], native_timeout_seconds=300, total_timeout_seconds=3600,
                         binary=dict(path='native_expanded_training_run_v1.exe'), dependencies=[])

    def test_no_formal_or_cloud_mode(self):
        for host in ['runpod', 'unknown']:
            with self.assertRaisesRegex(ValueError, 'bounded D4 Windows'):
                admission(self.plan, host)
        self.plan['schema'] = 'formal'
        with self.assertRaisesRegex(ValueError, 'bounded D4 Windows'):
            admission(self.plan, 'jack')

    def test_fixed_matrix_and_wall_limits(self):
        for key, value in [('workers', [1, 16]), ('native_timeout_seconds', 301), ('total_timeout_seconds', 3601)]:
            plan = copy.deepcopy(self.plan)
            plan[key] = value
            with self.assertRaises(ValueError):
                admission(plan, 'jack')

    def test_no_arbitrary_executable(self):
        self.plan['binary']['path'] = 'other.exe'
        with self.assertRaisesRegex(ValueError, 'expanded trainer binary'):
            admission(self.plan, 'jack')

    def test_helpers_must_be_pinned(self):
        with self.assertRaisesRegex(ValueError, 'helper pins incomplete'):
            admission(self.plan, 'jack')


if __name__ == '__main__':
    unittest.main()
