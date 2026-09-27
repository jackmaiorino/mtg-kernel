"""Offline checks of the line (a) dry-run manifests; never dispatches native work."""
import collections
import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import g115_line_a_manifest_v1 as manifest
import g115_line_a_seeds_v1 as seeds

FIXTURE = Path(__file__).resolve().parent / 'fixtures' / 'g115_line_a_seed_manifest_v2.json'
LABELS = manifest.DECKS + ('published-44ae71e1e126b63d',)


def registration(label):
    base = sum(label.encode())
    return dict(label=label, mainboard=[base % 97 + k for k in range(60)], sideboard=[base % 89 + k for k in range(15)])


def game_two(label, against):
    deck = registration(label)
    deck['mainboard'] = deck['mainboard'][:-1] + [sum(against.encode()) % 83]
    return deck


def source(checkpoint_sha256, name):
    return dict(checkpoint=dict(path='C:/fixture/%s/checkpoint.json' % name, sha256=checkpoint_sha256),
                feature_transfer=dict(expected_feature_contract_digest='c' * 64,
                                      expected_feature_encoding_digest='e' * 64),
                play_import=dict(path='C:/fixture/%s/import.json' % name, sha256='f' * 64))


def synthetic_template():
    """200 x 10 episodes covering every ordered pair of the eight template decks, postboard on half."""
    pairs = [(x, y) for x in LABELS for y in LABELS]
    updates = []
    for update in range(seeds.UPDATES):
        episodes = []
        for slot in range(seeds.GAMES_PER_UPDATE):
            index = update * seeds.GAMES_PER_UPDATE + slot
            learner, other = pairs[index % len(pairs)]
            seat = index % 2
            by_seat = [learner, other] if seat == 0 else [other, learner]
            postboard = (index // len(pairs)) % 2 == 0
            episodes.append(dict(
                id='cv-i%03d-s%02d' % (update, slot), learner_seat=seat, max_physical_decisions=40000,
                max_policy_steps=80000, postboard=postboard, registered=[registration(d) for d in by_seat],
                seed=index + 1, starting_player=(index // 2) % 2,
                selected=[game_two(by_seat[0], by_seat[1]), game_two(by_seat[1], by_seat[0])] if postboard
                else [registration(d) for d in by_seat],
                opponent=source(manifest.G115_CHECKPOINT_SHA256, 'g115') if index % 2 == 0
                else source(manifest.A48_CHECKPOINT_SHA256, 'a48')))
        updates.append(episodes)
    return dict(source=source(manifest.G115_CHECKPOINT_SHA256, 'g115'), updates=updates, inputs_enabled=False,
                learning_rate=0.0001, value_coefficient=0.5, gamma=1.0, gpu_ordinal=1, max_chunk_substeps=128,
                **{'lambda': 0.9}, projection_mode='state_only', entropy_coefficient=0.0)


class ManifestTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.seed_manifest = seeds.load_seed_manifest(FIXTURE)
        cls.template = synthetic_template()
        cls.result = manifest.build(cls.seed_manifest, cls.template)

    def test_counts_are_the_proposal_counts(self):
        calibration = self.result['calibration']
        self.assertEqual(calibration['counts'], dict(bo3=7168, members=16, decks=7, blocks=32, seats=2, per_member=448))
        self.assertEqual(self.result['screen_training']['counts'], dict(runs=4, games=8000))
        self.assertEqual(self.result['screen_evaluation']['counts'], dict(bo3=35840, endpoints=5, per_endpoint=7168))
        self.assertEqual(len(manifest.evaluation_jobs('cal', seeds.calibration_seeds(self.seed_manifest))), 7168)

    def test_deck_order_and_map(self):
        self.assertEqual(manifest.DECKS, ('Affinity', 'Burn', 'Elves', 'Faeries', 'Rally', 'Terror', 'Wildfire'))
        self.assertEqual([manifest.opponent_deck(0, j) for j in range(8)],
                         ['Affinity', 'Burn', 'Elves', 'Faeries', 'Rally', 'Terror', 'Wildfire', 'Affinity'])
        self.assertEqual(manifest.opponent_deck(6, 1), 'Affinity')
        self.assertEqual(manifest.opponent_deck(3, 31), 'Wildfire')  # (3 + 31) mod 7 = 6
        jobs = manifest.evaluation_jobs('cal', seeds.calibration_seeds(self.seed_manifest))
        for job in jobs:
            i = manifest.DECKS.index(job['learner_deck'])
            self.assertEqual(job['opponent_deck'], manifest.DECKS[(i + job['block']) % 7])
        pairs = collections.Counter((job['learner_deck'], job['opponent_deck']) for job in jobs)
        self.assertEqual(len(pairs), 49)
        for (learner, opponent), count in pairs.items():
            rotation = (manifest.DECKS.index(opponent) - manifest.DECKS.index(learner)) % 7
            self.assertEqual(count, manifest.ROTATION_BLOCKS[rotation] * 16 * 2)

    def test_integer_weights_sum_to_31360_per_endpoint(self):
        self.assertEqual(manifest.ROTATION_BLOCKS, (5, 5, 5, 5, 4, 4, 4))
        self.assertEqual(manifest.WEIGHTS, (4, 4, 4, 4, 5, 5, 5))
        for labelled in (seeds.calibration_seeds(self.seed_manifest), seeds.screen_evaluation_seeds(self.seed_manifest)):
            self.assertEqual(sum(job['weight'] for job in manifest.evaluation_jobs('x', labelled)), 31360)

    def test_every_seed_label_and_value_is_listed(self):
        self.assertEqual(self.result['calibration']['seeds'],
                         [dict(label=l, seed=v) for l, v in seeds.calibration_seeds(self.seed_manifest).items()])
        self.assertEqual([s['label'] for s in self.result['screen_evaluation']['seeds']],
                         list(seeds.SCREEN_EVALUATION_LABELS))
        self.assertEqual(self.result['screen_training']['seeds'], [
            dict(label='screen-training-pair/block/00', seed=5129419802126035267),
            dict(label='screen-training-pair/block/01', seed=11192956680141436156)])

    def test_roster_order_and_pending_slots(self):
        roster = self.result['calibration']['roster']
        self.assertEqual([slot['role'] for slot in roster], list(manifest.ROSTER_ROLES))
        self.assertEqual(manifest.ROSTER_ROLES[:2], ('V3', 'D3 wrapper'))
        self.assertEqual(manifest.ROSTER_ROLES[10:], tuple('exploiter-v3b/arm%d/run-%d' % (a, r)
                                                           for a in (1, 2) for r in range(3)))
        self.assertEqual([slot['status'] for slot in roster], ['bound', 'bound'] + ['pending'] * 14)
        self.assertEqual(roster[0]['source']['v3_forced_actions'], True)
        self.assertEqual(roster[1]['source']['kind'], 'information_set_search_v3')

    def test_playable_import_binds_its_slot_and_order_is_enforced(self):
        imports = dict(members=[dict(roster_index=7, label='refresh-034/current-1', status='playable',
                                     model_source=source('d' * 64, 'current-1'))])
        roster = manifest.evaluation_roster(self.template['source'], imports)
        self.assertEqual(roster[7]['status'], 'bound')
        self.assertEqual(sum(slot['status'] == 'pending' for slot in roster), 13)
        imports['members'][0]['label'] = 'refresh-034/current-0'
        with self.assertRaisesRegex(ValueError, 'order differs'):
            manifest.evaluation_roster(self.template['source'], imports)

    def test_nothing_is_launchable_while_anything_is_pending(self):
        for name in ('calibration', 'screen_training', 'screen_evaluation'):
            value = self.result[name]
            self.assertFalse(value['launchable'])
            self.assertTrue(any('byte worksheet unmeasured' in r for r in value['blocking']))
            self.assertTrue(any('throughput evidence absent' in r for r in value['blocking']))
        training = self.result['screen_training']['roster']
        self.assertEqual({slot['status'] for slot in training}, {'pending'})
        self.assertIn('accepts only V4 opponents', training[0]['reason'])

    def test_control_config_changes_only_seed_and_id(self):
        schedule = seeds.episode_schedule(self.seed_manifest, 'screen-training-pair/block/01')
        config = manifest.training_config(self.template, schedule, 1, 'control')
        original = [e for u in self.template['updates'] for e in u]
        changed = [e for u in config['updates'] for e in u]
        for index, (before, after) in enumerate(zip(original, changed)):
            self.assertEqual({k for k in before if before[k] != after[k]}, {'seed', 'id'})
            self.assertEqual(after['seed'], schedule['seeds'][index])
        self.assertEqual(changed[123]['id'], 'la-b01-i012-s03')
        self.assertEqual({k: v for k, v in config.items() if k != 'updates'},
                         {k: v for k, v in self.template.items() if k != 'updates'})

    def test_treatment_assigns_roster_index_e_mod_16(self):
        schedule = seeds.episode_schedule(self.seed_manifest, 'screen-training-pair/block/00')
        opponents = [source('%064x' % k, 'member-%d' % k) for k in range(16)]
        config = manifest.training_config(self.template, schedule, 0, 'treatment', opponents)
        episodes = [e for u in config['updates'] for e in u]
        counts = collections.Counter(e['opponent']['checkpoint']['sha256'] for e in episodes)
        self.assertEqual(set(counts.values()), {125})
        self.assertEqual(episodes[17]['opponent'], opponents[1])
        control = manifest.training_config(self.template, schedule, 0, 'control')
        self.assertEqual([e['seed'] for e in episodes], [e['seed'] for u in control['updates'] for e in u])
        self.assertEqual([e['id'] for e in episodes], [e['id'] for u in control['updates'] for e in u])

    def test_native_match_orders_decks_and_game_two_by_seat(self):
        packet = self.result['deck_packet']
        job = dict(learner_deck='Burn', opponent_deck='Rally', learner_seat=1, seed=7)
        match = manifest.native_match(job, packet)
        self.assertEqual(match['config']['deck_ids'], ['Rally', 'Burn'])
        self.assertEqual(match['config']['game_one_chooser'], 0)
        self.assertEqual(match['registered'], [packet['decks']['Rally'], packet['decks']['Burn']])
        self.assertEqual(match['postboard'], [packet['postboard']['Rally|Burn'], packet['postboard']['Burn|Rally']])
        self.assertEqual(len(packet['postboard']), 49)

    def test_deck_packet_refuses_an_inconsistent_game_two_deck(self):
        template = copy.deepcopy(self.template)
        for episode in (e for u in template['updates'] for e in u):
            if episode['postboard'] and all(d['label'] in manifest.DECKS for d in episode['registered']):
                episode['selected'][0]['sideboard'] = [1] * 15
                break
        with self.assertRaisesRegex(ValueError, 'Game-two deck differs'):
            manifest.deck_packet(template)

    def test_control_template_must_be_the_historical_mixture(self):
        template = copy.deepcopy(self.template)
        template['updates'][0][1]['opponent'] = source(manifest.G115_CHECKPOINT_SHA256, 'g115')
        with self.assertRaisesRegex(ValueError, '1,000 g115 and 1,000 A48'):
            manifest.screen_training_manifest(self.seed_manifest, template, [])

    def test_template_pin_is_enforced(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'a.json'
            path.write_text(json.dumps(self.template))
            with self.assertRaisesRegex(ValueError, 'Template bytes differ'):
                manifest.load_template(path)

    def test_manifest_hash_is_deterministic_and_covers_provenance(self):
        again = manifest.build(self.seed_manifest, synthetic_template())
        for name in ('calibration', 'screen_training', 'screen_evaluation'):
            first, second = self.result[name], again[name]
            self.assertEqual(first['manifest_sha256'], second['manifest_sha256'])
            body = {k: v for k, v in first.items() if k != 'manifest_sha256'}
            self.assertEqual(seeds.canonical_sha256(body), first['manifest_sha256'])
            self.assertEqual(first['provenance']['builder']['sha256'],
                             hashlib.sha256(Path(manifest.__file__).read_bytes()).hexdigest())


if __name__ == '__main__':
    unittest.main()
