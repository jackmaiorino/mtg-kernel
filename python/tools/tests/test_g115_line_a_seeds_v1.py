"""Offline checks of the line (a) seed rule and block-seed episode schedule."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import g115_line_a_seeds_v1 as seeds

FIXTURE = Path(__file__).resolve().parent / 'fixtures' / 'g115_line_a_seed_manifest_v2.json'
SCHEDULE_SHA256 = {
    'screen-training-pair/block/00': '01d9885bf9f3fca240de30dbbc325f74f4b4ac6a2a68c9697621b1715f384438',
    'screen-training-pair/block/01': 'c425cce8dd953fc995dd84a902e896e5f8a09fb06b643c6187376b7b3a54759a',
}


class SeedRuleTests(unittest.TestCase):
    def setUp(self):
        self.manifest = seeds.load_seed_manifest(FIXTURE)

    def test_fixture_is_the_frozen_manifest(self):
        self.assertEqual(hashlib.sha256(FIXTURE.read_bytes()).hexdigest(), seeds.SEED_MANIFEST_SHA256)

    def test_block_seeds_are_the_stored_values(self):
        self.assertEqual(seeds.training_block_seeds(self.manifest), {
            'screen-training-pair/block/00': 5129419802126035267,
            'screen-training-pair/block/01': 11192956680141436156})
        self.assertEqual(len(seeds.calibration_seeds(self.manifest)), 32)
        self.assertEqual(seeds.calibration_seeds(self.manifest)['calibration/block/00'], 6930492509028685935)

    def test_changed_manifest_bytes_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            changed = Path(directory) / 'seed-manifest.json'
            changed.write_bytes(FIXTURE.read_bytes() + b' ')
            with self.assertRaisesRegex(ValueError, 'frozen pin'):
                seeds.load_seed_manifest(changed)

    def test_splitmix64_matches_the_published_reference_stream(self):
        # Vigna's splitmix64.c from state 0; the kernel's state.rs uses the same constants.
        self.assertEqual(seeds.splitmix64(0, 3), [0xE220A8397B1DCDAF, 0x6E789E6AA1B965F4, 0x06C45D188009454F])
        for invalid in (-1, 1 << 64, True):
            with self.assertRaisesRegex(ValueError, 'u64'):
                seeds.splitmix64(invalid, 1)

    def test_each_block_expands_to_2000_distinct_deterministic_seeds(self):
        for label, expected in SCHEDULE_SHA256.items():
            with self.subTest(label=label):
                schedule = seeds.episode_schedule(self.manifest, label)
                self.assertEqual(len(schedule['seeds']), 2000)
                self.assertEqual(len(set(schedule['seeds'])), 2000)
                self.assertEqual(schedule['seeds'], seeds.splitmix64(schedule['block_seed'], 2000))
                self.assertEqual(schedule, seeds.episode_schedule(copy.deepcopy(self.manifest), label))
                self.assertEqual(seeds.canonical_sha256(schedule), expected)
                self.assertTrue(all(0 <= value < 1 << 64 for value in schedule['seeds']))

    def test_schedules_are_disjoint_across_blocks_and_from_frozen_seeds(self):
        result = seeds.schedules(self.manifest)
        first, second = (set(result[label]['seeds']) for label in seeds.TRAINING_LABELS)
        self.assertFalse(first & second)
        self.assertFalse((first | second) & set(seeds.calibration_seeds(self.manifest).values()))

    def test_unknown_block_label_is_refused(self):
        with self.assertRaisesRegex(ValueError, 'Unknown screen-training block'):
            seeds.episode_schedule(self.manifest, 'screen-training-pair/block/02')

    def test_screen_evaluation_labels_use_the_unchanged_rule(self):
        values = seeds.screen_evaluation_seeds(self.manifest)
        self.assertEqual(list(values), [f'screen-evaluation/block/{j:02d}' for j in range(32)])
        for label, value in values.items():
            digest = hashlib.sha256(('g115-line-a-seed-v2|' + label).encode('ascii')).digest()
            self.assertEqual(value, int.from_bytes(digest[:8], 'big'))

    def test_canonical_bytes_are_compact_sorted_ascii(self):
        self.assertEqual(seeds.canonical_bytes({'b': [18446744073709551615], 'a': 'x'}),
                         b'{"a":"x","b":[18446744073709551615]}')
        schedule = seeds.episode_schedule(self.manifest, 'screen-training-pair/block/00')
        self.assertEqual(json.loads(seeds.canonical_bytes(schedule)), schedule)


if __name__ == '__main__':
    unittest.main()
