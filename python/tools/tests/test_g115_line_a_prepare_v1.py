"""Offline checks of the throughput preparation's job set and engineering seeds."""
import collections
import hashlib
from pathlib import Path
import sys
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import g115_line_a_guard_v1 as guard
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

    def test_evidence_compares_every_measured_allocation_and_selects_the_fastest(self):
        binding = dict(launcher_sha256='1' * 64, executable_sha256='2' * 64, data_tree_sha256='3' * 64)
        rows = [dict(id='thr-%02d' % k, sha256='%064x' % k) for k in range(56)]

        def record(host, timings):
            return dict(host=host, work_class='bo3-ordinary', binding=binding,
                        phases=[dict(workers=w, seconds=t, completed=56, rows=rows) for w, t in timings])

        now = time.time()
        inventory = dict(jack=dict(checked_unix=now, eligible=True, reason='idle', competing=[]),
                         haleyspc=dict(checked_unix=now, eligible=True, reason='idle', competing=[]),
                         runpod=dict(checked_unix=now, eligible=False, reason='Windows-only transport'))
        evidence = prepare.throughput_evidence(
            [record('jack', ((1, 112.0), (8, 16.0), (24, 9.0))), record('haleyspc', ((1, 140.0), (8, 24.0)))],
            inventory, dict(jack=20.0, haleyspc=90.0), 3200, binding, 'bo3-ordinary')
        ids = [p['id'] for p in evidence['placements']]
        self.assertEqual(ids, ['jack-1', 'jack-8', 'jack-24', 'haleyspc-1', 'haleyspc-8', 'haleyspc-8+jack-24'])
        selected = guard.require_throughput(evidence, 'bo3-ordinary', 3200, binding, now=now)
        self.assertEqual(selected['id'], evidence['selected'])
        best = min(evidence['placements'], key=lambda p: p['projected_seconds'])
        self.assertEqual(selected['id'], best['id'])
        self.assertEqual(selected['id'], 'haleyspc-8+jack-24')  # 90 s startup is repaid at 3,200 BO3


if __name__ == '__main__':
    unittest.main()
