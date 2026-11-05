"""Readers outside python/tools take a checkpoint container (the default record format) with the values of the v1
file it came from, with no exporter binary."""
import hashlib
import json
import os
import sys
import unittest
from pathlib import Path
from unittest import mock

FIXTURES = Path(__file__).resolve().parents[1] / 'tools' / 'tests' / 'fixtures' / 'checkpoint_container_v1'
REPO = Path(__file__).resolve().parents[2]


class CheckpointContainerConsumerTests(unittest.TestCase):
    def setUp(self):
        patcher = mock.patch.dict(os.environ, {'MTG_KERNEL_CHECKPOINT_CONTAINER_BIN': ''})
        patcher.start()
        self.addCleanup(patcher.stop)
        self.v1_bytes = (FIXTURES / 'checkpoint-v1.json').read_bytes()
        self.container = (FIXTURES / 'checkpoint.ckpt').read_bytes()

    def test_public_feature_model_reads_a_container(self):
        from mtg_kernel_rl import public_feature_model_v1 as module
        self.assertEqual(module._checkpoint_document(self.container), json.loads(self.v1_bytes))
        self.assertEqual(module._checkpoint_document(self.v1_bytes), json.loads(self.v1_bytes))

    def test_spy_distill_reads_a_container(self):
        sys.path.insert(0, str(REPO / 'tools' / 'spy_distill'))
        try:
            import distill_common
        finally:
            sys.path.pop(0)
        self.assertEqual(distill_common.checkpoint_document(self.container), json.loads(self.v1_bytes))
        self.assertEqual(distill_common.read_checkpoint(FIXTURES / 'checkpoint.ckpt'), json.loads(self.v1_bytes))
        pin = hashlib.sha256(self.container).hexdigest()
        self.assertEqual(distill_common.read_checkpoint(FIXTURES / 'checkpoint.ckpt', pin), json.loads(self.v1_bytes))
        with self.assertRaises(ValueError):
            distill_common.read_checkpoint(FIXTURES / 'checkpoint.ckpt', '0' * 64)


if __name__ == '__main__':
    unittest.main()
