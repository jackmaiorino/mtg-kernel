"""The phase 1 cloud tools read a checkpoint in either record format (v1 JSON or the checkpoint container, the
default for ordinary fresh checkpoints) with the same values, and need no exporter binary."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from common import pin, read, read_checkpoint
from parity_evidence import Artifacts
from reference import checked_checkpoint

FIXTURES = Path(__file__).resolve().parents[1] / 'tests' / 'fixtures' / 'checkpoint_container_v1'


class CheckpointContainerReaderTests(unittest.TestCase):
    def setUp(self):
        patcher = patch.dict(os.environ, {'MTG_KERNEL_CHECKPOINT_CONTAINER_BIN': ''})
        patcher.start()
        self.addCleanup(patcher.stop)
        self.v1 = FIXTURES / 'checkpoint-v1.json'
        self.container = FIXTURES / 'checkpoint.ckpt'

    def test_read_checkpoint_gives_the_v1_values_for_a_container(self):
        self.assertEqual(read_checkpoint(self.container), read(self.v1))
        self.assertEqual(read_checkpoint(self.v1), read(self.v1))

    def test_artifacts_and_reference_readers_take_a_container_by_its_pin(self):
        container_pin = pin(self.container)
        expected = read(self.v1)
        self.assertEqual(Artifacts().checked_checkpoint(container_pin), expected)
        self.assertEqual(checked_checkpoint(container_pin), expected)
        with self.assertRaises(ValueError):
            checked_checkpoint(dict(container_pin, sha256='0' * 64))

    def test_strict_json_rules_still_apply_to_the_document(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'checkpoint.json'
            path.write_text('{"a": 1, "a": 2}')
            with self.assertRaisesRegex(ValueError, 'duplicate JSON key'):
                read_checkpoint(path)
            path.write_text('{"a": NaN}')
            with self.assertRaisesRegex(ValueError, 'non-finite'):
                read_checkpoint(path)


if __name__ == '__main__':
    unittest.main()
