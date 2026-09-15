"""Synthetic byte-transport cases, never trained models or runtime proof.

Reuse the shared full Net8 wire fixture so the BO3 adapter exercises actual
descriptor, parameter and state validation without model initialization.
"""
import copy
from pathlib import Path
import tempfile
import unittest

from common import pin, read, write
from pack_bo3 import prepare
from test_pack_bo3 import fixture as imported_fixture
from test_fresh_cloud import synthetic_state_digest, synthetic_transport_fixture


def reference(path):
    item = pin(path)
    return {'path': Path(item['path']).as_posix(), 'sha256': item['sha256']}


class FreshBo3CloudTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.spec = imported_fixture(self.root)
        build = read(self.spec['linux_build']['path'])
        descriptor = read(build['feature_descriptor']['path'])
        target = {
            'registry': {'path': 'C:/historical-generator/cards_v1.json', 'sha256': build['source_registry']['sha256']},
            'card_db_hash': '1234567890abcdef',
            'feature_contract_digest': descriptor['feature_contract_digest'],
            'feature_encoding_digest': descriptor['feature_encoding_digest'],
            'features_source_sha256': build['feature_source']['sha256'],
            'feature_descriptor_sha256': build['feature_descriptor']['sha256'],
            'registry_card_count': 184, 'card_token_rule': 'card-token=id+1; padding=0'}
        self.fresh = synthetic_transport_fixture(self.root / 'fresh', target=target)
        self.config = read(self.spec['run_request']['path'])
        self.imported_package = copy.deepcopy(self.config['initial_learner'])
        origin, state = self.fresh['origin'], copy.deepcopy(self.fresh['state'])
        # A valid full-state byte fixture with nonzero moments, not an executed
        # optimizer. Keep parameter bytes unchanged so their identity is exact.
        state['adam_step'] = 2
        state['first_moments'][1]['values'][0] = 1040187392
        state['second_moments'][1]['values'][0] = 1023410176
        state['state_sha256'] = synthetic_state_digest(state)
        self.checkpoint = {
            'schema': 'mtg-kernel-expanded-deck-fresh-checkpoint/v1',
            'source_import': origin, 'card_db_hash': origin['destination_card_db_hash'],
            'feature_contract_digest': origin['feature_contract_digest'],
            'feature_encoding_digest': origin['feature_encoding_digest'],
            **{name: state[name] for name in ('state_sha256', 'adam_step', 'scorer_bias_anchor_bits',
                                            'parameters', 'first_moments', 'second_moments')},
            'loss_identity': 'terminal_reinforce_value/v3', 'trajectories': [],
            'learning_rate_bits': self.config['learning_rate_bits'],
            'value_coefficient_bits': self.config['value_coefficient_bits']}
        self.checkpoint_path = self.root / 'fresh/checkpoint.json'
        write(self.checkpoint_path, self.checkpoint)
        source = copy.deepcopy(self.fresh['source']); source['checkpoint'] = reference(self.checkpoint_path)
        identity = copy.deepcopy(self.fresh['identity'])
        identity['checkpoint_sha256'] = source['checkpoint']['sha256']
        identity['adam_step'] = state['adam_step']; identity['state_sha256'] = state['state_sha256']
        identity['feature_schema_version'] = descriptor['feature_schema_version']
        identity['feature_registry_version'] = descriptor['feature_registry_version']
        self.fresh_package = copy.deepcopy(self.imported_package)
        self.fresh_package['gameplay'] = {'source': source, 'identity': identity}
        self.config['initial_learner'] = copy.deepcopy(self.fresh_package)
        self.config['initial_input']['learner'] = copy.deepcopy(self.fresh_package['gameplay'])
        self.config['opponents'] = [{'id': 'fresh-fixed', 'package': copy.deepcopy(self.fresh_package)},
                                    {'id': 'imported-fixed', 'package': self.imported_package}]
        self.save_config()

    def save_config(self):
        write(self.spec['run_request']['path'], self.config, replace=True)
        self.spec['run_request'] = pin(self.spec['run_request']['path'])

    def update_checkpoint(self, mutate, update_identity=False):
        value = read(self.checkpoint_path); mutate(value)
        write(self.checkpoint_path, value, replace=True)
        for package in [self.config['initial_learner'], self.config['opponents'][0]['package']]:
            behavior = package['gameplay']
            behavior['source']['checkpoint'] = reference(self.checkpoint_path)
            behavior['identity']['checkpoint_sha256'] = behavior['source']['checkpoint']['sha256']
            if update_identity:
                behavior['identity']['source_import'] = copy.deepcopy(value['source_import'])
        self.config['initial_input']['learner'] = copy.deepcopy(self.config['initial_learner']['gameplay'])
        self.save_config()

    def test_fresh_and_imported_roster_preserve_all_bytes_and_inference(self):
        before = {name: Path(item['path']).read_bytes() for name, item in {
            'checkpoint': self.config['initial_learner']['gameplay']['source']['checkpoint'],
            **read(self.fresh['source']['play_import']['path'])}.items() if isinstance(item, dict)}
        result = prepare(self.spec, self.root / 'package')
        manifest = read(result['manifest']['path'])
        relocated = read(self.root / 'package/payload/config/run.json')
        self.assertFalse(manifest['runtime_verified'])
        self.assertFalse(manifest['optimizer_restore_verified'])
        packages = [relocated['initial_learner']] + [row['package'] for row in relocated['opponents']]
        originals = [self.config['initial_learner']] + [row['package'] for row in self.config['opponents']]
        for actual, expected in zip(packages, originals):
            self.assertEqual(actual['gameplay']['identity'], expected['gameplay']['identity'])
        for package in packages[:2]:
            src = package['gameplay']['source']
            def local(item):
                return self.root / 'package/payload' / item['path'].removeprefix(self.spec['remote_root'] + '/')
            descriptor = read(local(src['play_import']))
            self.assertEqual(local(src['checkpoint']).read_bytes(), before['checkpoint'])
            self.assertEqual(local(descriptor['initialization']).read_bytes(), before['initialization'])
            self.assertEqual(local(descriptor['parameters']).read_bytes(), before['parameters'])
            old = read(self.fresh['source']['play_import']['path'])
            expected = copy.deepcopy(old)
            for name in ('initialization', 'parameters'):
                expected[name]['path'] = descriptor[name]['path']
            self.assertEqual(descriptor, expected)
        self.assertEqual(relocated['initial_input']['learner'], relocated['initial_learner']['gameplay'])
        self.assertEqual(len(manifest['model_source_relocations']), 2)
        self.assertTrue((self.root / 'package/payload/provenance/helpers/fresh_source.py').is_file())

    def test_imported_learner_with_fresh_fixed_opponent_is_supported(self):
        self.config['initial_learner'] = copy.deepcopy(self.imported_package)
        self.config['initial_input']['learner'] = copy.deepcopy(self.imported_package['gameplay'])
        self.save_config()
        prepare(self.spec, self.root / 'package')
        relocated = read(self.root / 'package/payload/config/run.json')
        self.assertEqual(relocated['initial_learner']['gameplay']['identity']['schema'], 'mtg-kernel-expanded-deck-inference/v1')
        self.assertEqual(relocated['opponents'][0]['package']['gameplay']['identity']['schema'], 'mtg-kernel-expanded-deck-inference/v2')

    def test_fresh_checkpoint_and_inference_cannot_relabel_initialization(self):
        self.update_checkpoint(lambda value: value['source_import'].update(lineage_id='another-recorded-origin'), update_identity=True)
        with self.assertRaisesRegex(ValueError, 'ancestry differs|initialization origin differs'):
            prepare(self.spec, self.root / 'package')
        self.assertFalse((self.root / 'package').exists())

    def test_fresh_origin_cannot_use_imported_checkpoint_schema(self):
        self.update_checkpoint(lambda value: value.update(schema='mtg-kernel-expanded-deck-checkpoint/v1'))
        with self.assertRaisesRegex(ValueError, 'checkpoint transport|schema/origin'):
            prepare(self.spec, self.root / 'package')
        self.assertFalse((self.root / 'package').exists())

    def test_rehashed_corrupted_moment_array_is_not_transportable(self):
        self.update_checkpoint(lambda value: value['first_moments'][1]['values'].__setitem__(0, 1065353216))
        with self.assertRaisesRegex(ValueError, 'full native state hash differs'):
            prepare(self.spec, self.root / 'package')
        self.assertFalse((self.root / 'package').exists())

    def test_fresh_fixed_opponent_without_checkpoint_rejects(self):
        self.config['opponents'][0]['package']['gameplay']['source']['checkpoint'] = None
        self.save_config()
        with self.assertRaisesRegex(ValueError, 'checkpoint required'):
            prepare(self.spec, self.root / 'package')
        self.assertFalse((self.root / 'package').exists())

    def test_cached_checkpoint_cannot_claim_different_installed_weights(self):
        self.config['opponents'][0]['package']['gameplay']['identity']['model']['weights_sha256'] = 'f' * 64
        self.save_config()
        with self.assertRaisesRegex(ValueError, 'weights differ from validated checkpoint'):
            prepare(self.spec, self.root / 'package')
        self.assertFalse((self.root / 'package').exists())

    def test_preexisting_bo3_descriptor_graph_stays_rejected(self):
        path = self.root / 'preexisting-bo3-source.json'
        write(path, {'schema': 'mtg-kernel-bo3-gameplay-source/v1', 'ordinary_origin': self.fresh_package['gameplay']})
        self.config['initial_learner']['gameplay']['source']['play_import'] = reference(path)
        self.config['initial_input']['learner'] = copy.deepcopy(self.config['initial_learner']['gameplay'])
        self.save_config()
        with self.assertRaisesRegex(ValueError, 'only ordinary imported or fresh-initialization'):
            prepare(self.spec, self.root / 'package')
        self.assertFalse((self.root / 'package').exists())


if __name__ == '__main__':
    unittest.main(verbosity=2)
