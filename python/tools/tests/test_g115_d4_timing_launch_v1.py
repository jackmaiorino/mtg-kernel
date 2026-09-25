"""Offline refusal checks; deliberately never dispatch MTG or GPU work."""
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from g115_d4_timing_launch_v1 import admission, require_idle_gpu


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

    def test_haley_desktop_memory_with_reserve_and_identity(self):
        p = dict(computer_name='HALEYSPC', gpu_ordinal=0, gpu_uuid='GPU-test')
        require_idle_gpu(p, '0, GPU-test, 0, 518, 7439')
        for line in ['0, GPU-other, 0, 518, 7439', '0, GPU-test, 6, 518, 7439', '0, GPU-test, 0, 7000, 1000']:
            with self.assertRaises(ValueError):
                require_idle_gpu(p, line)

    def test_jack_strict_idle_is_unchanged(self):
        p = dict(computer_name='DESKTOP-DJ1C40R', gpu_ordinal=1, gpu_uuid='GPU-test')
        require_idle_gpu(p, '1, GPU-test, 0, 0, 8192')
        for line in ['1, GPU-test, 1, 0, 8192', '1, GPU-test, 0, 518, 7439']:
            with self.assertRaises(ValueError):
                require_idle_gpu(p, line)


if __name__ == '__main__':
    unittest.main()
