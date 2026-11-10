"""Checkpoint consumers read the values of a v1 file from either format without an exporter binary."""
import hashlib
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

REPO = Path(__file__).resolve().parents[2]
TOOLS = REPO / 'python' / 'tools'
FIXTURES = TOOLS / 'tests' / 'fixtures' / 'checkpoint_container_v1'


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

    def test_postboard_pilot_endpoint_accepts_both_formats(self):
        with mock.patch.object(sys, 'path', [str(TOOLS), *sys.path]):
            from prepare_postboard_qualification_v1 import pin, read_pinned_checkpoint
            from run_postboard_pilot_v1 import endpoint

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            arm = root / 'pilot'
            checkpoint = arm / 'iterations/000199/attempt-000000/update/checkpoint.json'
            checkpoint.parent.mkdir(parents=True)
            expected = json.loads(self.v1_bytes)
            initial = {'play_import': {'path': 'model.json'}, 'feature_transfer': 'fixture'}
            manifest = {'initial_source': initial, 'final_adam_step': expected['adam_step']}

            for data in (self.v1_bytes, self.container):
                checkpoint.write_bytes(data)
                source = dict(initial, checkpoint=pin(checkpoint))
                (arm / 'training-audit.json').write_text(json.dumps({
                    'complete': True, 'updates': 200, 'source': source,
                }))
                self.assertEqual(endpoint(root, manifest, 'pilot'), source)
                self.assertEqual(read_pinned_checkpoint(source['checkpoint']), expected)

            checkpoint.write_bytes(self.v1_bytes)
            with self.assertRaises(AssertionError):
                endpoint(root, manifest, 'pilot')

    def test_postboard_reader_preserves_strict_legacy_parsing(self):
        with mock.patch.object(sys, 'path', [str(TOOLS), *sys.path]):
            from prepare_postboard_qualification_v1 import pin, read_pinned_checkpoint

        with tempfile.TemporaryDirectory() as tmp:
            checkpoint = Path(tmp) / 'checkpoint.json'
            for raw, message in ((b'{"adam_step":0,"adam_step":1}', 'Duplicate JSON key'),
                                 (b'{"adam_step":NaN}', 'Nonfinite JSON')):
                with self.subTest(raw=raw):
                    checkpoint.write_bytes(raw)
                    with self.assertRaisesRegex(ValueError, message):
                        read_pinned_checkpoint(pin(checkpoint))


if __name__ == '__main__':
    unittest.main()
