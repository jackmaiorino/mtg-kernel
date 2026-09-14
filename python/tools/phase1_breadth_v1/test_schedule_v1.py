"""Pure compiler tests. All registrations and model pins here are synthetic.

No native legality, runtime qualification, full-field coverage or actual model
initialization is represented by these small fixtures.
"""
from collections import Counter
import copy
from fractions import Fraction
import hashlib
import importlib.util
from pathlib import Path
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('breadth_schedule_under_test', Path(__file__).with_name('schedule_v1.py'))
schedule = importlib.util.module_from_spec(spec)
spec.loader.exec_module(schedule)


def source(name):
    return {'play_import': {'path': '/synthetic/' + name + '-import.json', 'sha256': 'a' * 64},
        'checkpoint': {'path': '/synthetic/' + name + '-state.json', 'sha256': 'b' * 64},
        'feature_transfer': {'expected_feature_contract_digest': 'c' * 64,
                             'expected_feature_encoding_digest': 'd' * 64}}


def archetype(index, field_count):
    main, side = index * 4, index * 4 + 1
    original = {'label': 'synthetic-' + str(index), 'mainboard': [main] * 60, 'sideboard': [side] * 15}
    swapped = {'label': 'synthetic-swapped-' + str(index), 'mainboard': [main] * 59 + [side],
               'sideboard': [side] * 14 + [main]}
    return {'id': 'a' + str(index), 'field_count': field_count, 'train': [
        {'id': 'v' + str(index) + '-one', 'multiplicity': 7, 'registered': original,
         'postboard': [copy.deepcopy(original), copy.deepcopy(swapped)]},
        {'id': 'v' + str(index) + '-two', 'multiplicity': 1, 'registered': swapped,
         'postboard': [copy.deepcopy(original)]}]}


def fixture():
    catalog = {'scope': 'engineering', 'provenance': {'fixture': True, 'missing_field_count': 99},
               'archetypes': [archetype(i, weight) for i, weight in enumerate((8, 3, 1))]}
    settings = {'schema': schedule.SCHEMA, 'lineage_id': 'synthetic-lineage', 'block_index': 0,
        'seed': 123456789, 'phase': 'mixed', 'initial_source': source('initial'),
        'opponents': [{'id': 'frozen', 'source': source('opponent')}],
        'policy_pool': [{'id': 'self', 'weight': 1, 'assignment': {'kind': 'current'}},
                        {'id': 'frozen', 'weight': 2, 'assignment': {'kind': 'fixed', 'id': 'frozen'}},
                        {'id': 'recent', 'weight': 1, 'assignment': {'kind': 'recent_completed', 'lag': 2, 'fallback': 'initial'}}],
        'opponent_archetype_weights': [{'id': 'a' + str(i), 'weight': weight} for i, weight in enumerate((1, 2, 3))],
        'learning_rate': 1e-5, 'value_coefficient': 0.5, 'collection_workers': 10,
        'preparation_workers': 4, 'output_directory': '/synthetic/output', 'excluded_seeds': []}
    return catalog, settings


def rational(value):
    return Fraction(value['numerator'], value['denominator'])


class ScheduleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.catalog, cls.settings = fixture()
        cls.compiled = schedule.compile_block_v1(cls.catalog, cls.settings)

    def test_determinism_pins_input_preservation_and_ordered_dto(self):
        catalog, settings = fixture()
        old = copy.deepcopy((catalog, settings))
        result = schedule.compile_block_v1(catalog, settings)
        self.assertEqual(result, self.compiled)
        self.assertEqual((catalog, settings), old)
        native, report = result['native'], result['report']
        self.assertEqual(report['native_sha256'], hashlib.sha256(schedule._json(native)).hexdigest())
        self.assertEqual(report['native_json_bytes'], len(schedule._json(native)))
        self.assertEqual(native['initial_source'], settings['initial_source'])
        self.assertEqual(native['update_backend'], {'kind': 'cpu'})
        self.assertEqual((native['collection_workers'], native['preparation_workers']), (10, 4))
        self.assertEqual(report['independent_lineages_created'], 0)
        self.assertFalse(report['native_validation_performed'])
        self.assertEqual(report['provenance']['missing_field_count'], 99)
        catalog['archetypes'].reverse()
        for row in catalog['archetypes']:
            row['train'].reverse()
            for variant in row['train']:
                variant['postboard'].reverse()
        settings['policy_pool'].reverse()
        settings['opponent_archetype_weights'].reverse()
        self.assertEqual(schedule.compile_block_v1(catalog, settings)['native'], native)

    def test_exact_hamilton_margins_and_reported_unconfounded_joint_fixture(self):
        native, report = self.compiled['native'], self.compiled['report']
        self.assertEqual((report['updates'], report['games']), (200, 2000))
        field = report['branches']['field']
        self.assertEqual([row['realized_count'] for row in field], [667, 250, 83])
        self.assertEqual([rational(row['target_count']) for row in field], [Fraction(2000, 3), Fraction(250), Fraction(250, 3)])
        self.assertEqual([row['realized_count'] for row in report['branches']['uniform']], [334, 333, 333])
        self.assertEqual([row['games'] for row in report['roles']], [500] * 4)
        self.assertEqual(report['phase_counts'], {'postboard': 1000, 'preboard': 1000})
        rows = report['episode_assignments']
        for index, iteration in enumerate(native['iterations']):
            selected = rows[index * 10:index * 10 + 10]
            self.assertEqual(len(iteration['episodes']), 10)
            self.assertEqual(Counter(row['branch'] for row in selected), {'field': 5, 'uniform': 5})
            self.assertEqual(Counter(row['phase'] for row in selected), {'preboard': 5, 'postboard': 5})
        joint = report['branch_phase_role_contingency']
        self.assertEqual(len(joint), 16)
        # A fixed deterministic fixture, not a probabilistic statistical gate.
        self.assertTrue(all(row['games'] > 0 for row in joint))
        observed = Counter((r['branch'], r['phase'], r['learner_seat'], r['relative_start']) for r in rows)
        self.assertEqual(observed, Counter({(r['branch'], r['phase'], r['learner_seat'], r['relative_start']): r['games'] for r in joint}))
        self.assertEqual(sum(row['games'] for row in report['matchup_counts']), 2000)
        for row in report['selected_configuration_counts']:
            changed = Counter(item[row['role'] + '_configuration_changed']
                              for item in rows if item['phase'] == row['phase'])
            self.assertEqual((row['changed'], row['unchanged']), (changed[True], changed[False]))
            self.assertEqual(row['changed'] + row['unchanged'], 1000)
            if row['phase'] == 'postboard':
                self.assertGreater(row['changed'], 0)
                self.assertGreater(row['unchanged'], 0)
            else:
                self.assertEqual(row['changed'], 0)

    def test_actual_episode_roles_variant_conservation_and_historical_bindings(self):
        variants = {v['id']: v for a in self.catalog['archetypes'] for v in a['train']}
        realized_variants, realized_policies = Counter(), Counter()
        for assignment, scheduled in zip(self.compiled['report']['episode_assignments'],
                (row for iteration in self.compiled['native']['iterations'] for row in iteration['episodes'])):
            episode = scheduled['episode']
            seat = assignment['learner_seat']
            self.assertEqual(episode['id'], assignment['episode_id'])
            self.assertEqual(episode['learner_seat'], seat)
            self.assertEqual(episode['starting_player'] == seat, assignment['relative_start'] == 'play')
            for actor, key in [(seat, 'learner_variant'), (1-seat, 'opponent_variant')]:
                supplied = variants[assignment[key]]
                registered = episode['registered'][actor]
                selected = episode['selected'][actor]
                self.assertEqual(registered, schedule._deck(supplied['registered'])[0])
                self.assertEqual(sorted(registered['mainboard'] + registered['sideboard']), sorted(selected['mainboard'] + selected['sideboard']))
                changed = any(registered[zone] != selected[zone] for zone in ('mainboard', 'sideboard'))
                self.assertEqual(assignment[('learner' if actor == seat else 'opponent') + '_configuration_changed'], changed)
                if not episode['postboard']:
                    self.assertEqual(selected, registered)
                else:
                    self.assertIn(selected, [schedule._deck(deck)[0] for deck in supplied['postboard']])
            if assignment['policy_id'] == 'recent':
                index = assignment['iteration']
                self.assertEqual(scheduled['opponent'], {'kind': 'completed_iteration', 'index': index-2} if index >= 2 else {'kind': 'initial'})
            realized_variants[assignment['learner_variant']] += 1
            realized_policies[assignment['policy_id']] += 1
        self.assertEqual(realized_policies, Counter({r['id']: r['realized_count'] for r in self.compiled['report']['policy_pool']}))
        for row in self.compiled['report']['training_variants']:
            self.assertEqual(row['learner_games'], realized_variants[row['id']])
            self.assertEqual(rational(row['conditional_target_probability']), Fraction(row['multiplicity'], 8))

    def test_seed_exclusion_domain_separation_and_fresh_block(self):
        seeds = self.compiled['report']['episode_seeds']
        self.assertEqual(len(set(seeds)), 2000)
        catalog, settings = fixture()
        settings['excluded_seeds'] = seeds[:20]
        changed = schedule.compile_block_v1(catalog, settings)
        self.assertTrue(set(changed['report']['episode_seeds']).isdisjoint(seeds[:20]))
        self.assertGreaterEqual(changed['report']['rejected_seed_candidates'], 20)
        self.assertEqual(changed['report']['episode_assignments'], self.compiled['report']['episode_assignments'])
        settings['excluded_seeds'] = seeds
        settings['block_index'] = 1
        second = schedule.compile_block_v1(catalog, settings)
        self.assertTrue(set(second['report']['episode_seeds']).isdisjoint(seeds))
        self.assertTrue({r['episode_id'] for r in second['report']['episode_assignments']}.isdisjoint(
            r['episode_id'] for r in self.compiled['report']['episode_assignments']))

    def test_preboard_and_archetype_scaled_block(self):
        catalog, settings = fixture()
        settings['phase'] = 'preboard'
        catalog['archetypes'] += [archetype(i, 1) for i in range(3, 11)]
        settings['opponent_archetype_weights'] += [{'id': 'a' + str(i), 'weight': 1} for i in range(3, 11)]
        for row in catalog['archetypes']:
            for variant in row['train']:
                variant['postboard'] = []
        result = schedule.compile_block_v1(catalog, settings)
        self.assertEqual((result['report']['updates'], result['report']['games']), (220, 2200))
        self.assertEqual(result['report']['phase_counts'], {'preboard': 2200})
        self.assertTrue(all(not row['episode']['postboard'] for it in result['native']['iterations'] for row in it['episodes']))

    def test_reject_duplicate_zoned_lists_but_allow_same75_distinct_zones(self):
        # The base fixture deliberately has two legal zonings of each same75.
        self.assertEqual(len(self.compiled['report']['training_variants']), 6)
        catalog, settings = fixture()
        copied = copy.deepcopy(catalog['archetypes'][0]['train'][0])
        copied['id'] = 'renamed-duplicate'
        copied['registered']['label'] = 'another-label'
        copied['registered']['mainboard'].reverse()
        catalog['archetypes'][1]['train'].append(copied)
        with self.assertRaisesRegex(ValueError, 'duplicate exact zoned'):
            schedule.compile_block_v1(catalog, settings)

    def test_input_rejections(self):
        mutations = [
            lambda c, s: s.update(collection_workers=True),
            lambda c, s: s.update(preparation_workers=33),
            lambda c, s: s.update(learning_rate=float('nan')),
            lambda c, s: s.update(value_coefficient=1e-100),
            lambda c, s: s.update(seed=2**64),
            lambda c, s: s.update(output_directory='relative/run'),
            lambda c, s: s.update(excluded_seeds=[1, 1]),
            lambda c, s: s['policy_pool'][2]['assignment'].update(lag=0),
            lambda c, s: s['policy_pool'][2]['assignment'].update(fallback='current'),
            lambda c, s: s['policy_pool'][1]['assignment'].update(id='unknown'),
            lambda c, s: s['opponent_archetype_weights'].pop(),
            lambda c, s: c['archetypes'][0]['train'][0].update(multiplicity=0),
            lambda c, s: c['archetypes'][0]['train'][0].update(postboard=[]),
            lambda c, s: c['archetypes'][0]['train'][0]['postboard'][0]['sideboard'].__setitem__(0, 65535),
            lambda c, s: c['archetypes'][0]['train'][0]['registered']['mainboard'].__setitem__(0, True),
        ]
        for mutate in mutations:
            catalog, settings = fixture()
            mutate(catalog, settings)
            with self.subTest(mutate=mutate), self.assertRaises(ValueError):
                schedule.compile_block_v1(catalog, settings)

    def test_output_bound_and_unbiased_rejection_sampling(self):
        catalog, settings = fixture()
        cap = max(len(schedule._json(catalog)), len(schedule._json(settings))) + 1
        with mock.patch.object(schedule, 'MAX_JSON_BYTES', cap):
            with self.assertRaisesRegex(ValueError, 'compiled native configuration'):
                schedule.compile_block_v1(catalog, settings)
        rng = schedule._Rng(1, 'fixture', 0, 'test')
        maximum = mock.Mock(digest=lambda: b'\xff' * 32)
        zero = mock.Mock(digest=lambda: b'\0' * 32)
        with mock.patch.object(schedule.hashlib, 'sha256', side_effect=[maximum, zero]):
            self.assertEqual(rng.below(3), 0)
        self.assertEqual(rng.counter, 2)
        left = schedule._Rng(1, 'fixture', 0, 'roles')
        right = schedule._Rng(1, 'fixture', 0, 'phases')
        self.assertNotEqual(left.key, right.key)


if __name__ == '__main__':
    unittest.main()
