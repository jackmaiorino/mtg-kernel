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

    def test_an_s_document_is_read_only_through_the_binary_s_document_mode(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 's-train-state.bin'
            path.write_bytes(cc.MAGIC + b'S container')
            with mock.patch.dict(os.environ, {cc.BINARY_ENV: ''}):
                with self.assertRaises(RuntimeError):
                    cc.read_s_document(path)
            record = Path(tmp) / 'checkpoint.json'
            record.write_text('{}')
            with self.assertRaises(ValueError):
                cc.read_s_document(record)
            calls = []

            def run(args, capture_output, text):
                calls.append(args)
                return type('R', (), {'returncode': 0, 'stdout': json.dumps({'config': {'d_model': 96}}),
                                      'stderr': ''})()
            with mock.patch.dict(os.environ, {cc.BINARY_ENV: 'bin'}), mock.patch.object(cc.subprocess, 'run', run):
                self.assertEqual(cc.read_s_document(path), {'config': {'d_model': 96}})
            import hashlib
            self.assertEqual(calls, [['bin', 's-document', '--input', str(path.resolve()), '--sha256',
                                      hashlib.sha256(path.read_bytes()).hexdigest()]])

            def fail(args, capture_output, text):
                return type('R', (), {'returncode': 1, 'stdout': '', 'stderr': 'not an S checkpoint container'})()
            with mock.patch.dict(os.environ, {cc.BINARY_ENV: 'bin'}), mock.patch.object(cc.subprocess, 'run', fail):
                with self.assertRaisesRegex(ValueError, 'not an S checkpoint container'):
                    cc.read_s_document(path)


if __name__ == '__main__':
    unittest.main()
