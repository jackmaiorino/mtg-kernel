import sys
from pathlib import Path
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from compute_throughput_v3 import queued_makespan


class DeviceQueueTests(unittest.TestCase):
    def setUp(self):
        self.placements = {'a':dict(host='desktop',gpu_uuid='J'),
                           'b':dict(host='computehost',gpu_uuid='H'),
                           'c':dict(host='desktop',gpu_uuid='J')}
        self.executions = {'a':dict(started_unix=1,finished_unix=3),
                           'b':dict(started_unix=1,finished_unix=5),
                           'c':dict(started_unix=3,finished_unix=6)}

    def test_sums_queue_cost_but_overlaps_distinct_devices(self):
        self.assertEqual(queued_makespan(self.placements,self.executions,{'a':20,'b':40,'c':30}),50)

    def test_same_device_overlap_rejected(self):
        self.executions['c']['started_unix']=2.99
        with self.assertRaisesRegex(ValueError,'overlapped'):
            queued_makespan(self.placements,self.executions,{'a':20,'b':40,'c':30})

    def test_single_job_per_device_is_parallel_makespan(self):
        self.placements['c']=dict(host='desktop',gpu_uuid='J2')
        self.executions['c']['started_unix']=1
        self.assertEqual(queued_makespan(self.placements,self.executions,{'a':20,'b':40,'c':30}),40)
