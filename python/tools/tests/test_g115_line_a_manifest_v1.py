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


def exposure_table(template, roster, composition):
    """A structurally valid test table; the real table is the declaration's (R4), never chosen here."""
    members = manifest.composition_members(roster, composition)
    canonical = [m['id'] for m in members if m['role'] in manifest.CANONICAL_ROLES]
    archival = [m['id'] for m in members if m['role'] in manifest.ARCHIVAL_ROLES]
    assignments, next_canonical, next_archival = [], 0, 0
    for episode in manifest.check_template_shape(template):
        if composition == 'E' and manifest.opponent_label(episode) == 'Rally':
            assignments.append(archival[next_archival % len(archival)])
            next_archival += 1
        else:
            assignments.append(canonical[next_canonical % len(canonical)])
            next_canonical += 1
    return dict(schema=manifest.EXPOSURE_SCHEMA, composition=composition, assignments=assignments)


class ManifestTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.seed_manifest = seeds.load_seed_manifest(FIXTURE)
        cls.template = synthetic_template()
        cls.roster = manifest.draft_roster(cls.template['source'])
        cls.result = manifest.build(cls.seed_manifest, cls.template, cls.roster)

    def test_counts_are_the_ruling_counts_for_both_compositions(self):
        self.assertEqual((manifest.PER_FULL_MEMBER, manifest.PER_RALLY_MEMBER), (448, 64))
        expected = {'E': (3200, 16000, 18240), 'B': (2688, 13440, 15680)}
        for composition, (calibration, screen, with_holdout) in expected.items():
            with self.subTest(composition=composition):
                self.assertEqual(self.result['calibration_' + composition]['counts']['bo3'], calibration)
                self.assertEqual(self.result['screen_evaluation_' + composition]['counts'],
                                 dict(bo3=screen, endpoints=5, per_endpoint=calibration))
                self.assertEqual(self.result['screen_evaluation_%s_holdout' % composition]['counts']['bo3'],
                                 with_holdout)
                self.assertEqual(with_holdout - screen, 2240)
                self.assertEqual(self.result['screen_training_' + composition]['counts'], dict(runs=4, games=8000))
        self.assertEqual(self.result['calibration_E']['counts']['full_members'], 6)
        self.assertEqual(self.result['calibration_E']['counts']['rally_members'], 8)
        self.assertEqual(self.result['calibration_B']['counts']['rally_members'], 0)

    def test_deck_order_and_map(self):
        self.assertEqual(manifest.DECKS, ('Affinity', 'Burn', 'Elves', 'Faeries', 'Rally', 'Terror', 'Wildfire'))
        self.assertEqual([manifest.opponent_deck(0, j) for j in range(8)],
                         ['Affinity', 'Burn', 'Elves', 'Faeries', 'Rally', 'Terror', 'Wildfire', 'Affinity'])
        self.assertEqual(manifest.opponent_deck(6, 1), 'Affinity')
        self.assertEqual(manifest.opponent_deck(3, 31), 'Wildfire')  # (3 + 31) mod 7 = 6
        members = manifest.composition_members(self.roster, 'B')
        jobs = manifest.evaluation_jobs('cal', seeds.calibration_seeds(self.seed_manifest), members)
        for job in jobs:
            i = manifest.DECKS.index(job['learner_deck'])
            self.assertEqual(job['opponent_deck'], manifest.DECKS[(i + job['block']) % 7])
        pairs = collections.Counter((job['learner_deck'], job['opponent_deck']) for job in jobs)
        self.assertEqual(len(pairs), 49)
        blocks_per_rotation = [sum(1 for j in range(32) if j % 7 == r) for r in range(7)]
        self.assertEqual(blocks_per_rotation, [5, 5, 5, 5, 4, 4, 4])
        for (learner, opponent), count in pairs.items():
            rotation = (manifest.DECKS.index(opponent) - manifest.DECKS.index(learner)) % 7
            self.assertEqual(count, blocks_per_rotation[rotation] * 6 * 2)

    def test_archival_members_play_only_rally_opponent_cells(self):
        members = manifest.composition_members(self.roster, 'E')
        jobs = manifest.evaluation_jobs('cal', seeds.calibration_seeds(self.seed_manifest), members)
        archival = [job for job in jobs if job['member'].startswith(('population/', 'august/'))]
        self.assertEqual(len(archival), 8 * 64)
        self.assertEqual({job['opponent_deck'] for job in archival}, {'Rally'})
        per_block = collections.Counter((job['member'], job['block']) for job in archival)
        self.assertEqual(set(per_block.values()), {2})  # one learner deck per block, both seats

    def test_every_seed_label_and_value_is_listed(self):
        self.assertEqual(self.result['calibration_E']['seeds'],
                         [dict(label=l, seed=v) for l, v in seeds.calibration_seeds(self.seed_manifest).items()])
        self.assertEqual([s['label'] for s in self.result['screen_evaluation_E']['seeds']],
                         list(seeds.SCREEN_EVALUATION_LABELS))
        self.assertEqual(self.result['screen_training_B']['seeds'], [
            dict(label='screen-training-pair/block/00', seed=5129419802126035267),
            dict(label='screen-training-pair/block/01', seed=11192956680141436156)])

    def test_draft_roster_follows_the_ruling_roles(self):
        roles = collections.Counter(m['role'] for m in self.roster['members'])
        self.assertEqual(roles, {'v3': 1, 'd3-wrapper': 1, 'recent': 4, 'population': 4, 'august': 4, 'holdout': 1})
        self.assertFalse(self.roster['frozen'])
        v3 = self.roster['members'][0]
        self.assertEqual(v3['evaluation']['source']['v3_forced_actions'], True)
        self.assertEqual({m['training']['status'] for m in self.roster['members']}, {'pending'})
        self.assertIn('accepts only V4 opponents', v3['training']['reason'])
        imports = dict(members=[dict(label='refresh-034/current-1', status='playable',
                                     model_source=source('d' * 64, 'current-1'))])
        bound = manifest.draft_roster(self.template['source'], imports)
        current = [m for m in bound['members'] if m['id'] == 'population/current-1'][0]
        self.assertEqual(current['evaluation']['status'], 'bound')

    def test_roster_validation_refuses_changed_roles(self):
        def bind_with_short_pin(roster):
            roster['members'][2]['evaluation'] = dict(status='bound', source=dict(
                kind='public_checkpoint', config=dict(path='E:/c.json', sha256='a' * 64),
                checkpoint=dict(path='E:/k.json', sha256='abc')))

        def train_the_holdout(roster):
            roster['members'][-1]['training'] = dict(status='bound', episode_fields=dict(opponent=source('b' * 64, 'h')))

        for change, message in ((lambda r: r['members'][6].update(scope='full'), 'Role or scope'),
                                (lambda r: r['members'].append(copy.deepcopy(r['members'][0])), 'Duplicate'),
                                (lambda r: r['members'].pop(0), 'Role v3 must carry all 1'),
                                (lambda r: r['members'].pop(3), 'Role recent must carry all 4'),
                                (lambda r: r['members'][4].update(lineage=' '), 'Lineage id required'),
                                (lambda r: r['roles']['recent'].reverse(), 'role table differs'),
                                (bind_with_short_pin, 'Incomplete pin'),
                                (train_the_holdout, 'holdout never receives training exposure')):
            roster = copy.deepcopy(self.roster)
            change(roster)
            with self.assertRaisesRegex(ValueError, message):
                manifest.validate_roster(roster)

    def test_frozen_roster_binds_template_tables_and_exact_weights(self):
        def frozen():
            roster = copy.deepcopy(self.roster)
            roster['frozen'] = True
            roster['exposure_tables'] = {c: dict(path='E:/decl/%s.json' % c, sha256=c.lower() * 64) for c in 'EB'}
            weights = dict(normalization='equal ordered deck pairs within each member, then member weight')
            for composition in 'EB':
                ids = [m['id'] for m in manifest.composition_members(roster, composition)]
                weights[composition] = {i: '1/%d' % len(ids) for i in ids}
            roster['yardstick_weights'] = weights
            return roster

        manifest.validate_roster(frozen())
        roster = frozen()  # CODEX #525's prospective B weights, in its {numerator, denominator} form
        roster['yardstick_weights']['B'] = {
            'v3': dict(numerator=267, denominator=800), 'd3-wrapper': dict(numerator=533, denominator=1600),
            'recent/r4-a': dict(numerator=67, denominator=800), 'recent/r4-b': dict(numerator=67, denominator=800),
            'recent/r5-a': dict(numerator=133, denominator=1600), 'recent/r5-b': dict(numerator=33, denominator=400)}
        manifest.validate_roster(roster)
        for change, message in (
                (lambda r: r['yardstick_weights']['B'].update(v3=dict(numerator=0.5, denominator=1)), 'exact rational'),
                (lambda r: r.update(template=dict(r['template'], sha256='0' * 64)), 'bind the template hash'),
                (lambda r: r['exposure_tables'].pop('B'), 'both exposure tables'),
                (lambda r: r['yardstick_weights'].update(normalization=''), 'normalization declared'),
                (lambda r: r['yardstick_weights']['E'].update({'holdout/b-block48': '1/2'}), 'no holdout'),
                (lambda r: r['yardstick_weights']['B'].update(v3='0.1'), 'exact rational'),
                (lambda r: r['yardstick_weights']['B'].update(v3='1/2'), 'do not sum to one')):
            roster = frozen()
            change(roster)
            with self.assertRaisesRegex(ValueError, message):
                manifest.validate_roster(roster)

    def test_nothing_is_launchable_while_anything_is_pending(self):
        for name, value in self.result.items():
            if name in ('deck_packet', 'roster'):
                continue
            self.assertFalse(value['launchable'], name)
            self.assertIn('roster is a draft', value['blocking'][0 if 'training' not in name else 1])
            self.assertTrue(any('byte worksheet unmeasured' in r for r in value['blocking']))
            self.assertTrue(any('throughput evidence absent' in r for r in value['blocking']))
        self.assertIn('exposure table for composition E pending', self.result['screen_training_E']['blocking'][0])

    def test_exposure_table_checks_the_rally_slot_rule(self):
        for composition in ('E', 'B'):
            table = exposure_table(self.template, self.roster, composition)
            counts = manifest.validate_exposure(table, self.roster, self.template, composition)
            self.assertEqual(sum(counts.values()), 2000)
            if composition == 'B':
                self.assertFalse(any(m.startswith(('population/', 'august/')) for m in counts))
        table = exposure_table(self.template, self.roster, 'E')
        rally = [i for i, e in enumerate(manifest.check_template_shape(self.template))
                 if manifest.opponent_label(e) == 'Rally']
        other = next(i for i in range(2000) if i not in set(rally))
        for index, member in ((rally[0], 'v3'), (other, 'august/historical-0')):
            broken = copy.deepcopy(table)
            broken['assignments'][index] = member
            with self.assertRaisesRegex(ValueError, 'Rally-slot rule'):
                manifest.validate_exposure(broken, self.roster, self.template, 'E')
        with self.assertRaisesRegex(ValueError, 'outside the composition'):
            manifest.validate_exposure(dict(table, composition='B'), self.roster, self.template, 'B')
        with self.assertRaisesRegex(ValueError, 'all 2,000'):
            manifest.validate_exposure(dict(table, assignments=table['assignments'][:-1]), self.roster,
                                       self.template, 'E')

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

    def test_treatment_takes_per_episode_opponent_fields_and_keeps_seeds_and_ids(self):
        schedule = seeds.episode_schedule(self.seed_manifest, 'screen-training-pair/block/00')
        search = dict(opponent=source(manifest.G115_CHECKPOINT_SHA256, 'g115'),
                      opponent_search=dict(path='C:/fixture/descriptor.json', sha256='a' * 64))
        plain = dict(opponent=source('b' * 64, 'member'))
        fields = [search if e % 3 == 0 else plain for e in range(2000)]
        config = manifest.training_config(self.template, schedule, 0, 'treatment', fields)
        episodes = [e for u in config['updates'] for e in u]
        self.assertEqual(episodes[3]['opponent_search'], search['opponent_search'])
        self.assertNotIn('opponent_search', episodes[4])
        self.assertEqual(episodes[4]['opponent'], plain['opponent'])
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
            manifest.screen_training_manifest(self.seed_manifest, template, self.roster, 'E')

    def test_template_pin_is_enforced(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'a.json'
            path.write_text(json.dumps(self.template))
            with self.assertRaisesRegex(ValueError, 'Template bytes differ'):
                manifest.load_template(path)

    def test_manifest_hash_is_deterministic_and_covers_provenance(self):
        template = synthetic_template()
        again = manifest.build(self.seed_manifest, template, manifest.draft_roster(template['source']))
        for name, first in self.result.items():
            if name in ('deck_packet', 'roster'):
                continue
            self.assertEqual(first['manifest_sha256'], again[name]['manifest_sha256'])
            body = {k: v for k, v in first.items() if k != 'manifest_sha256'}
            self.assertEqual(seeds.canonical_sha256(body), first['manifest_sha256'])
            self.assertEqual(first['provenance']['builder']['sha256'],
                             hashlib.sha256(Path(manifest.__file__).read_bytes()).hexdigest())
            self.assertEqual(first['provenance']['roster_sha256'], seeds.canonical_sha256(self.roster))


if __name__ == '__main__':
    unittest.main()
