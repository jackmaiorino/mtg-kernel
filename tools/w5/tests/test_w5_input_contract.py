"""Tests for opt-in W5 `input_contract` ('no-state-digest-v1' / 'no-digests-v1') arm wiring."""
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import chains  # noqa: E402
import w5box  # noqa: E402

FAKE = HERE / 'fake_trainer.py'
CONTAINER = HERE / 'fake_container.py'


def jwrite(path, doc):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(doc), encoding='utf-8')
    return Path(path)


def make_converted_source(root, label, contract='no-state-digest-v1', extra=None):
    base_imp = jwrite(root / f'{label}-base-import.json', {'base': label})
    base_pin = chains.pin(base_imp)
    conv_doc = {
        'schema': chains.INPUT_CONTRACT_CONVERSION_SCHEMA,
        'contract': contract,
        'state_digest_columns': [123, 219],
        'base_play_import': base_pin,
    }
    if contract == 'no-digests-v1':
        conv_doc['action_digest_columns'] = [99, 195]
    conv = jwrite(root / f'{label}-conversion.json', conv_doc)
    imp = jwrite(root / f'{label}-import.json', {
        'schema': chains.INPUT_CONTRACT_IMPORT_SCHEMA,
        'base_play_import': base_pin,
        'conversion': chains.pin(conv),
    })
    ckpt = root / f'{label}.ckpt'
    ckpt.write_text(f'{label} weights', encoding='utf-8')
    source = {'play_import': chains.pin(imp), 'checkpoint': chains.pin(ckpt)}
    if extra:
        source.update(extra)
    return source, conv


def make_unconverted_source(root, label):
    imp = jwrite(root / f'{label}-plain-import.json', {'plain': label})
    ckpt = root / f'{label}-plain.ckpt'
    ckpt.write_text(f'{label} plain', encoding='utf-8')
    return {'play_import': chains.pin(imp), 'checkpoint': chains.pin(ckpt)}


class InputContractArmTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix='w5-input-contract-'))
        self.root = self.tmp / 'node'
        topology = {'online': [0, 1], 'domains': [[[0], [1]]]}
        self.layout = jwrite(self.tmp / 'layout.json', dict(w5box.compute_layout(topology, 1, 0), host_class='test'))
        self.frozen = jwrite(self.tmp / 'FROZEN.json', {'w5': 'frozen constants'})

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _plan_independent(self, parent_src, opp_src, input_contract=None, arm_name='C', recipe_extra=None):
        parent = jwrite(self.tmp / f'parent-{arm_name}.json', {'source': parent_src})
        base = jwrite(self.tmp / f'base-{arm_name}.json', {
            'schema': 'test-base',
            'opponents': [{'id': 'opp0', 'source': opp_src}] if opp_src is not None else [],
            'iterations': [{'episodes': [{'episode': {'learner_seat': 0}, 'opponent': {'kind': 'current'}}]}],
        })
        arm = {'parent': str(parent), 'workers': 1, 'memory_max_bytes': 1 << 30,
               'bytes_per_update': 1000, 'segment_updates': 4}
        if input_contract is not None:
            arm['input_contract'] = input_contract
        if recipe_extra:
            arm.update(recipe_extra)
        arms = {
            arm_name: arm,
            'F': {'parent': str(parent), 'workers': 1, 'memory_max_bytes': 1 << 30,
                  'bytes_per_update': 1000, 'segment_updates': 2},
        }
        doc = {
            'schema': chains.INPUTS_SCHEMA,
            'stage': '0',
            'independent': True,
            'node': 'node1',
            'root': self.root.as_posix(),
            'cgroup_root': (self.tmp / 'cgroup').as_posix(),
            'isolation': 'none',
            'host_class': 'test',
            'layout': str(self.layout),
            'python': sys.executable,
            'trainer': str(FAKE),
            'container_tool': str(CONTAINER),
            'base': str(base),
            'frozen': str(self.frozen),
            'arms': arms,
            'chains': {'c1': {'arm': arm_name, 'slot': 0, 'seed_root': 'w5/ic1'}},
        }
        inputs_path = jwrite(self.tmp / 'inputs.json', doc)
        return chains.make_plan(inputs_path)

    def test_valid_no_state_digest_and_no_digests_contracts_are_pinned_in_recipe(self):
        for idx, contract in enumerate(('no-state-digest-v1', 'no-digests-v1')):
            sub = self.tmp / f'case-{idx}'
            sub.mkdir()
            self.root = sub / 'node'
            parent_src, _ = make_converted_source(sub, 'parent', contract=contract)
            opp_src, _ = make_converted_source(sub, 'opp', contract=contract)
            _, plan = self._plan_independent(parent_src, opp_src, input_contract=contract)
            arm_doc = chains.read(plan['arms']['C']['file']['path'])
            self.assertEqual(arm_doc['input_contract'], contract)
            self.assertEqual(chains.recipe_identity(arm_doc)['input_contract'], contract)
            manifest = chains.read(plan['chains']['c1']['manifest'])
            self.assertEqual(manifest['recipe']['input_contract'], contract)
            # Continuation with matching converted source succeeds; unconverted continuation is refused.
            next_good, _ = make_converted_source(sub, 'cont-ok', contract=contract)
            cfg = chains.arm_config(arm_doc, 'w5/ic1', 4, 4, sub / 'native-ok', source=next_good)
            self.assertEqual(cfg['initial_source'], next_good)
            next_bad = make_unconverted_source(sub, 'cont-bad')
            with self.assertRaisesRegex(chains.Stop, 'mtg-kernel-no-state-digest-import/v1'):
                chains.arm_config(arm_doc, 'w5/ic1', 4, 4, sub / 'native-bad', source=next_bad)

    def test_absent_input_contract_omits_key_from_arm_and_recipe(self):
        plain = make_unconverted_source(self.tmp, 'plain')
        _, plan = self._plan_independent(plain, plain, input_contract=None)
        arm_doc = chains.read(plan['arms']['C']['file']['path'])
        self.assertNotIn('input_contract', arm_doc)
        self.assertNotIn('input_contract', chains.recipe_identity(arm_doc))

    def test_refuses_invalid_contract_mixed_sources_x_arm_and_non_net8(self):
        conv_state, _ = make_converted_source(self.tmp, 'state', contract='no-state-digest-v1')
        conv_both, _ = make_converted_source(self.tmp, 'both', contract='no-digests-v1')
        plain = make_unconverted_source(self.tmp, 'plain')
        with self.assertRaisesRegex(chains.Stop, 'input_contract is one of'):
            self._plan_independent(conv_state, conv_state, input_contract='bogus-v1')
        with self.assertRaisesRegex(chains.Stop, 'C-recipe arm'):
            self._plan_independent(conv_state, conv_state, input_contract='no-state-digest-v1',
                                   arm_name='X8', recipe_extra={'budget': 16384, 'coefficient': 0.5, 'dose': 'primary'})
        with self.assertRaisesRegex(chains.Stop, 'mtg-kernel-no-state-digest-import/v1'):
            self._plan_independent(plain, conv_state, input_contract='no-state-digest-v1')
        with self.assertRaisesRegex(chains.Stop, 'mtg-kernel-no-state-digest-import/v1'):
            self._plan_independent(conv_state, plain, input_contract='no-state-digest-v1')
        with self.assertRaisesRegex(chains.Stop, 'conversion contract is'):
            self._plan_independent(conv_state, conv_both, input_contract='no-state-digest-v1')
        s_conv, _ = make_converted_source(self.tmp, 's-bad', contract='no-state-digest-v1',
                                          extra={'s_learner': {'schema': 'w5-s-learner/v1'}})
        with self.assertRaisesRegex(chains.Stop, 'net-8 only'):
            self._plan_independent(s_conv, conv_state, input_contract='no-state-digest-v1')


if __name__ == '__main__':
    unittest.main()
