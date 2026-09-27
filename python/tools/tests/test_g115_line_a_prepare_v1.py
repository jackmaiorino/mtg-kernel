"""Offline checks of the throughput preparation's job set and engineering seeds."""
import collections
import hashlib
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import g115_line_a_manifest_v1 as manifest
import g115_line_a_prepare_v1 as prepare
import g115_line_a_seeds_v1 as seeds

FIXTURE = Path(__file__).resolve().parent / 'fixtures' / 'g115_line_a_seed_manifest_v2.json'


class PrepareTests(unittest.TestCase):
    def test_representative_set_is_56_bo3_on_the_calibration_deck_map(self):
        jobs = prepare.representative_jobs(('v3', 'g115'), (0, 1))
        self.assertEqual(len(jobs), 56)
        self.assertEqual(len({job['id'] for job in jobs}), 56)
        self.assertEqual(collections.Counter(job['member'] for job in jobs), {'v3': 28, 'g115': 28})
        self.assertEqual(collections.Counter(job['learner_seat'] for job in jobs), {0: 28, 1: 28})
        for job in jobs:
            i = manifest.DECKS.index(job['learner_deck'])
            self.assertEqual(job['opponent_deck'], manifest.DECKS[(i + job['block']) % 7])

    def test_engineering_seeds_never_touch_a_frozen_packet(self):
        frozen_manifest = seeds.load_seed_manifest(FIXTURE)
        frozen = (set(seeds.calibration_seeds(frozen_manifest).values()) |
                  set(seeds.training_block_seeds(frozen_manifest).values()) |
                  set(seeds.screen_evaluation_seeds(frozen_manifest).values()))
        values = {job['seed'] for job in prepare.representative_jobs(('v3', 'g115'), (0, 1))}
        self.assertEqual(len(values), 2)
        self.assertFalse(values & frozen)
        digest = hashlib.sha256(b'g115-line-a-engineering-v1|throughput/block/00').digest()
        self.assertEqual(prepare.engineering_seed('throughput/block/00'), int.from_bytes(digest[:8], 'big'))


if __name__ == '__main__':
    unittest.main()
