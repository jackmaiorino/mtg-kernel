import json
import os
import struct
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import checkpoint_container_v1 as cc  # noqa: E402

FIXTURES = Path(__file__).resolve().parent / 'fixtures' / 'checkpoint_container_v1'

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

    def test_a_container_is_read_in_python_with_the_values_of_the_v1_file_it_came_from(self):
        """The fixture pair is the Rust converter's output (a Rust test keeps it so): decoding the container
        gives the v1 document, key order included, with no binary."""
        v1_bytes = (FIXTURES / 'checkpoint-v1.json').read_bytes()
        container = (FIXTURES / 'checkpoint.ckpt').read_bytes()
        self.assertTrue(cc.is_container(container) and not cc.is_container(v1_bytes))
        expected = json.loads(v1_bytes)
        with mock.patch.dict(os.environ, {cc.BINARY_ENV: ''}):
            document = cc.document_from_bytes(container)
            self.assertEqual(document, expected)
            self.assertEqual(list(document), list(expected))
            self.assertEqual(json.dumps(document, separators=(',', ':'), ensure_ascii=False).encode(), v1_bytes)
            self.assertEqual(cc.document_from_bytes(v1_bytes), expected)
            self.assertTrue(any(len(row['values']) == 0 for row in document['parameters']))
            with tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp) / 'checkpoint.json'
                path.write_bytes(container)
                self.assertEqual(cc.read_document(path), expected)
                self.assertTrue(cc.has_moments(path))

    def test_a_container_that_is_not_the_canonical_layout_is_refused(self):
        container = (FIXTURES / 'checkpoint.ckpt').read_bytes()
        header = struct.unpack_from('<IIQQ', container, 8)
        data_start = -(-(32 + header[2] + header[3]) // 256) * 256
        cases = {
            'truncated': container[:-4],
            'trailing byte': container + b'\x00',
            'version': container[:8] + struct.pack('<I', 2) + container[12:],
            'reserved': container[:12] + struct.pack('<I', 1) + container[16:],
            'header only': container[:20],
            'table padding': container[:data_start - 1] + b'\x01' + container[data_start:],
            'tensor padding': container[:data_start + 24] + b'\x01' + container[data_start + 25:],
            'document with values': cc.MAGIC + container[8:16] + struct.pack('<QQ', 2, 0) + b'{}' + bytes(222),
        }
        for name, data in cases.items():
            with self.subTest(name):
                with self.assertRaises(ValueError):
                    cc.document_from_bytes(data)

    def test_export_v1_of_a_container_fails_by_name_without_the_rust_exporter(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'checkpoint.json'
            path.write_bytes((FIXTURES / 'checkpoint.ckpt').read_bytes())
            with mock.patch.dict(os.environ, {cc.BINARY_ENV: ''}):
                with self.assertRaisesRegex(RuntimeError, cc.BINARY_ENV):
                    cc.export_v1(path)
            v1 = Path(tmp) / 'v1.json'
            v1.write_bytes((FIXTURES / 'checkpoint-v1.json').read_bytes())
            with mock.patch.dict(os.environ, {cc.BINARY_ENV: ''}):
                self.assertEqual(cc.export_v1(v1), v1.read_bytes())

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
