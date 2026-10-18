"""Public endpoint dependency, layout and unchanged panel semantics checks."""
import copy
import json
from pathlib import Path
import tempfile
import unittest

import nine_deck_baseline_v1 as ndb
import nine_deck_public_panel_v1 as panel


class PublicCheckpointTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        initialization = self.save('initial.json', {'fixture': True})
        parameters = self.save('parameters.json', {'fixture': True})
        imported = self.save('import.json', dict(initialization=initialization, parameters=parameters))
        # These old provenance pins are not loader dependencies.
        self.parent = dict(play_import=imported, checkpoint=self.save('parent.json', dict(
            adam_step=35640, trajectories=[dict(path='absent-history.json', sha256='0'*64)])))
        self.config = dict(source=self.parent, updates=[[], []], inputs_enabled=False,
                           ppo=dict(epochs=2, minibatches=1, clip=.2, shuffle_seed=1))
        self.optimizer = dict(schema='mtg-kernel-public-input-optimizer-state/v1',
            feature_contract='contract', feature_encoding='encoding', auxiliary_contract='aux', registry='registry',
            legacy_adam_step=35644, parameters=[dict(name='tensor', shape=[1], values=[1])],
            public={key: [0] for key in ('object', 'state', 'object_first', 'object_second', 'state_first', 'state_second')})
        self.optimizer['public']['adam_step'] = 4
        self.source = self.endpoint()

    def save(self, name, value):
        return panel.write_json(self.root/name, value)

    def endpoint(self):
        optimizer = self.save('optimizer.json', self.optimizer)
        checkpoint = self.save('checkpoint.json', dict(schema='mtg-kernel-public-input-checkpoint/v1',
            optimizer_file='optimizer.json', optimizer_sha256=optimizer['sha256'], next_update=2,
            config_sha256='native-canonical-config-hash'))
        return dict(kind='public_checkpoint', config=self.save('config.json', self.config), checkpoint=checkpoint)

    def test_public_descriptor_and_recursive_runtime_pins(self):
        self.assertEqual(panel.candidate_source(self.source), self.source)
        panel.check_request_pins(self.source)
        copied = panel.candidate_source(self.source)
        copied['checkpoint']['path'] = 'changed'
        self.assertNotEqual(copied, self.source)
        (self.root/'parameters.json').write_text('changed')
        with self.assertRaisesRegex(ValueError, 'pinned file changed'):
            panel.check_request_pins(self.source)

    def test_sibling_optimizer_and_ages_are_bound(self):
        (self.root/'optimizer.json').write_text('changed')
        with self.assertRaisesRegex(ValueError, 'pinned file changed'):
            panel.candidate_layout(self.source)
        self.optimizer['legacy_adam_step'] = 35642
        self.source = self.endpoint()
        with self.assertRaisesRegex(ValueError, 'optimizer ages differ'):
            panel.candidate_layout(self.source)
        self.optimizer['legacy_adam_step'] = 35644
        self.optimizer['public']['adam_step'] = 2
        self.source = self.endpoint()
        with self.assertRaisesRegex(ValueError, 'optimizer ages differ'):
            panel.candidate_layout(self.source)

    def test_disabled_state_and_boundary_are_validated(self):
        self.optimizer['public']['state_first'][0] = 1
        self.source = self.endpoint()
        with self.assertRaisesRegex(ValueError, 'projections are nonzero'):
            panel.candidate_layout(self.source)
        self.optimizer['public']['state_first'][0] = 0
        self.source = self.endpoint()
        checkpoint = json.loads((self.root/'checkpoint.json').read_text())
        checkpoint['optimizer_file'] = '../other.json'
        self.source['checkpoint'] = self.save('checkpoint.json', checkpoint)
        with self.assertRaisesRegex(ValueError, 'checkpoint/update boundary differs'):
            panel.candidate_layout(self.source)

    def test_layout_ignores_endpoint_weights_and_retains_inference_contract(self):
        layout = panel.candidate_layout(self.source)
        self.optimizer['parameters'][0]['values'] = [2]
        self.source = self.endpoint()
        self.assertEqual(panel.candidate_layout(self.source), layout)
        self.optimizer['parameters'][0]['shape'] = [1, 1]
        self.source = self.endpoint()
        self.assertNotEqual(panel.candidate_layout(self.source), layout)
        self.optimizer['parameters'][0]['shape'] = [1]
        self.config['inputs_enabled'] = True
        self.source = self.endpoint()
        self.assertNotEqual(panel.candidate_layout(self.source), layout)

    def test_gae_age_and_legacy_layout_are_unchanged(self):
        self.config.pop('ppo')
        self.optimizer['legacy_adam_step'] = 35642
        self.optimizer['public']['adam_step'] = 2
        self.source = self.endpoint()
        panel.candidate_layout(self.source)
        ordinary = dict(schema='ordinary-fixture', source_import={}, feature_contract_digest='c',
                        feature_encoding_digest='e', card_db_hash='r', parameters=[dict(name='x', shape=[1])])
        legacy = dict(play_import=self.parent['play_import'], checkpoint=self.save('legacy.json', ordinary))
        expected = {key: ordinary[key] for key in ('schema', 'source_import', 'feature_contract_digest',
                                                  'feature_encoding_digest', 'card_db_hash')}
        expected['parameters'] = [('x', [1])]
        expected['play_import'] = legacy['play_import']['sha256']
        self.assertEqual(panel.candidate_layout(legacy), panel.sha256_bytes(json.dumps(expected, sort_keys=True).encode()))
        self.assertEqual(panel.candidate_source(legacy), dict(kind='legacy', source=legacy, v3_forced_actions=False))

    def test_public_and_legacy_requests_keep_same_cases_decks_roles_and_seeds(self):
        decks = ndb.load_decks(Path(__file__).resolve().parents[3]/'data/runtime_decks_v1.json')
        opponent = dict(kind='legacy', source=self.parent, v3_forced_actions=False)
        old = panel.build('old', self.parent, opponent, decks, self.root/'old', 2,
                          own_decks=(4,), game_one_only=True)
        new = panel.build('new', self.source, opponent, decks, self.root/'new', 2,
                          own_decks=(4,), game_one_only=True)
        self.assertEqual(old['schedule_sha256'], new['schedule_sha256'])
        for a, b in zip(old['requests'], new['requests']):
            before, after = panel.request_for_plan(old, a), panel.request_for_plan(new, b)
            self.assertEqual(before['matches'], after['matches'])
            self.assertEqual(after['sources'][b['seat']], self.source)
            self.assertEqual(after['sources'][1-b['seat']], opponent)
            before.pop('output_directory'); after.pop('output_directory')
            before['sources'][b['seat']] = copy.deepcopy(self.source)
            self.assertEqual(before, after)


if __name__ == '__main__':
    unittest.main()
