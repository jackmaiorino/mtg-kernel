import json
import struct
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import checkpoint_container_v1 as cc  # noqa: E402


def v1_document():
    # Field order as the Rust struct serializes it (the tensor lists sit between the anchor and the trajectories).
    return {
        'schema': 'mtg-kernel-expanded-deck-fresh-checkpoint/v1',
        'feature_contract_digest': 'c' * 64,
        'feature_encoding_digest': 'e' * 64,
        'card_db_hash': '064a7c989255ab3c',
        'source_import': {'kind': 'fresh'},
        'state_sha256': 'f' * 64,
        'adam_step': 7,
        'scorer_bias_anchor_bits': 0,
        'parameters': [{'name': 'a.weight', 'shape': [2, 3], 'values': [1, 2, 3, 4, 5, 4294967295]},
                       {'name': 'a.bias', 'shape': [2], 'values': [0, 2147483648]}],
        'first_moments': [{'name': 'a.weight', 'shape': [2, 3], 'values': [9, 8, 7, 6, 5, 4]},
                          {'name': 'a.bias', 'shape': [2], 'values': [3, 2]}],
        'second_moments': [{'name': 'a.weight', 'shape': [2, 3], 'values': [1, 1, 1, 1, 1, 1]},
                           {'name': 'a.bias', 'shape': [2], 'values': [0, 0]}],
        'trajectories': [{'path': 'x/episode-0000.json', 'sha256': '0' * 64}],
        'loss_identity': 'gae_advantage_value/v1',
        'learning_rate_bits': 953267991,
        'value_coefficient_bits': 1056964608,
    }


def data_start(container):
    doc_len, table_len = struct.unpack_from('<QQ', container, 16)
    return -(-(32 + doc_len + table_len) // 256) * 256


class CheckpointContainerTests(unittest.TestCase):
    def test_round_trip_and_readers(self):
        doc = v1_document()
        v1 = json.dumps(doc, separators=(',', ':')).encode()
        container = cc.encode(doc)
        self.assertTrue(cc.is_container(container))
        self.assertEqual(data_start(container) % 256, 0)
        with tempfile.TemporaryDirectory() as tmp:
            c_path = Path(tmp) / 'checkpoint.ckpt'
            j_path = Path(tmp) / 'checkpoint.json'
            c_path.write_bytes(container)
            j_path.write_bytes(v1)
            self.assertEqual(cc.export_v1(c_path), v1)
            self.assertEqual(cc.export_v1(j_path), v1)
            self.assertTrue(cc.has_moments(c_path) and cc.has_moments(j_path))
            meta = cc.read_document(c_path)
            self.assertEqual(meta['state_sha256'], doc['state_sha256'])
            self.assertEqual(meta['parameters'], [])
            no_moments = dict(doc, first_moments=[], second_moments=[])
            c_path.write_bytes(cc.encode(no_moments))
            self.assertFalse(cc.has_moments(c_path))

    def test_tampering_is_refused(self):
        container = bytearray(cc.encode(v1_document()))
        padded = bytearray(container)
        padded[data_start(bytes(container)) - 1] = 1
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'c'
            path.write_bytes(bytes(padded))
            with self.assertRaises(ValueError):
                cc.export_v1(path)
        with self.assertRaises(ValueError):
            cc.decode(bytes(container[:-1]))
        bad = bytearray(container)
        bad[8] = 2
        with self.assertRaises(ValueError):
            cc.decode(bytes(bad))
        # Top-level members out of the struct's order are not canonical.
        doc = v1_document()
        reordered = {'card_db_hash': doc['card_db_hash'], **{k: v for k, v in doc.items() if k != 'card_db_hash'}}
        with self.assertRaises(ValueError):
            cc.decode(cc.encode(reordered))
        # A container cut at its data start (every tensor missing) is refused by the metadata readers too.
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'cut'
            path.write_bytes(bytes(container[:data_start(bytes(container))]))
            with self.assertRaises(ValueError):
                cc.read_document(path)
            with self.assertRaises(ValueError):
                cc.has_moments(path)


if __name__ == '__main__':
    unittest.main()
