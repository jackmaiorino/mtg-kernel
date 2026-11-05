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



class SCandidateLayoutTests(unittest.TestCase):
    """W5 XS: a native S candidate's layout comes from its record and S container (through the Rust binary's
    `s-document`, mocked here), is the same for every checkpoint of a lineage, and refuses foreign records."""

    def setUp(self):
        from unittest import mock
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.config = dict(d_model=8, layers=1, heads=2, ffn=16, card_dim=4, card_vocab=257)
        self.documents = {}
        self.initial = self.container('s0.bin', step=1, state='a'*64)
        self.transfer = dict(expected_feature_contract_digest='contract', expected_feature_encoding_digest='encoding')
        self.source = dict(play_import=panel.write_json(self.root/'import.json', {'fixture': True}),
                           feature_transfer=self.transfer, checkpoint=None,
                           s_learner=dict(schema=panel.S_LEARNER_SOURCE, initial=self.initial))
        patcher = mock.patch.object(panel.checkpoint_container_v1, 'read_s_document',
                                    side_effect=lambda path: self.documents[Path(path).name])
        patcher.start()
        self.addCleanup(patcher.stop)

    def container(self, name, step, state, config=None):
        path = self.root/name
        path.write_bytes(b'MTGCKPT\x00' + name.encode())
        self.documents[name] = dict(schema='s-transformer-train-state-v1', architecture='s-object-transformer-v1',
                                    config=config or self.config, adam_step=step, state_sha256=state,
                                    parameters=[dict(name='ref_bias', shape=[2, 2]),
                                                dict(name='policy.2.weight', shape=[1, 8])])
        return panel.pin(path)

    def record(self, name, state_pin, step, state, **changes):
        record = dict(schema=panel.S_LEARNER_CHECKPOINT, architecture='s-object-transformer-v1',
                      initial_sha256=self.initial['sha256'], feature_contract_digest='contract',
                      feature_encoding_digest='encoding', state=state_pin, state_sha256=state, adam_step=step)
        record.update(changes)
        return panel.write_json(self.root/name, record)

    def test_layout_is_shared_by_every_checkpoint_of_a_lineage(self):
        fresh = panel.candidate_layout(self.source)
        state = self.container('s-train-state.bin', step=9, state='b'*64)
        later = dict(self.source, checkpoint=self.record('checkpoint.json', state, 9, 'b'*64))
        self.assertEqual(panel.candidate_layout(later), fresh)
        # Another config or shell is another layout.
        other = self.container('other.bin', step=9, state='c'*64, config=dict(self.config, d_model=16))
        wider = dict(self.source, checkpoint=self.record('wider.json', other, 9, 'c'*64))
        self.assertNotEqual(panel.candidate_layout(wider), fresh)
        shell = dict(self.source, play_import=panel.write_json(self.root/'import2.json', {'other': True}))
        self.assertNotEqual(panel.candidate_layout(shell), fresh)

    def test_foreign_or_inconsistent_records_are_refused(self):
        state = self.container('s-train-state.bin', step=9, state='b'*64)
        for name, changes in [('initial.json', dict(initial_sha256='0'*64)),
                              ('schema.json', dict(schema='w5-s-learner-checkpoint/v0')),
                              ('digest.json', dict(feature_contract_digest='other'))]:
            source = dict(self.source, checkpoint=self.record(name, state, 9, 'b'*64, **changes))
            with self.assertRaisesRegex(ValueError, 'does not belong'):
                panel.candidate_layout(source)
        for name, step, digest in [('step.json', 8, 'b'*64), ('hash.json', 9, 'd'*64)]:
            source = dict(self.source, checkpoint=self.record(name, state, step, digest))
            with self.assertRaisesRegex(ValueError, 'record and container differ'):
                panel.candidate_layout(source)
        unknown = dict(self.source, s_learner=dict(self.source['s_learner'], schema='w5-s-learner/v0'))
        with self.assertRaisesRegex(ValueError, 'unknown S learner source schema'):
            panel.candidate_layout(unknown)
        (self.root/'s-train-state.bin').write_bytes(b'changed')
        source = dict(self.source, checkpoint=self.record('changed.json', state, 9, 'b'*64))
        with self.assertRaisesRegex(ValueError, 'pinned file changed'):
            panel.candidate_layout(source)


if __name__ == '__main__':
    unittest.main()
