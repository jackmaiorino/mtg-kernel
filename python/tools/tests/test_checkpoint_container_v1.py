import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import checkpoint_container_v1 as cc  # noqa: E402

V1 = {
    'schema': 'mtg-kernel-expanded-deck-fresh-checkpoint/v1',
    'state_sha256': 'f' * 64,
    'parameters': [{'name': 'a', 'shape': [1], 'values': [1]}],
    'first_moments': [{'name': 'a', 'shape': [1], 'values': [0]}],
    'second_moments': [{'name': 'a', 'shape': [1], 'values': [0]}],
}


class CheckpointContainerTests(unittest.TestCase):
    def test_v1_json_passes_through(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'checkpoint.json'
            data = json.dumps(V1, separators=(',', ':')).encode()
            path.write_bytes(data)
            self.assertEqual(cc.export_v1(path), data)
            self.assertEqual(cc.read_document(path), V1)
            self.assertTrue(cc.has_moments(path))
            path.write_bytes(json.dumps(dict(V1, first_moments=[])).encode())
            self.assertFalse(cc.has_moments(path))

    def test_a_container_is_never_read_without_the_rust_exporter(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'checkpoint.json'
            path.write_bytes(cc.MAGIC + b'\x01\x00\x00\x00' + bytes(24))
            with mock.patch.dict(os.environ, {cc.BINARY_ENV: ''}):
                with self.assertRaises(RuntimeError):
                    cc.read_document(path)
                with self.assertRaises(RuntimeError):
                    cc.has_moments(path)


if __name__ == '__main__':
    unittest.main()
