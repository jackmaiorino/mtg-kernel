"""The per-count lean vector and the pregame advantage mode in the chain tools (tools/ex1/qual.py, tools/w5/xact.py,
tools/w5/chains.py): a `b0` that is a list of 7 (one lean per number of mulligans the acting seat has already taken),
its binary32 schedule and refusals, a scalar and a vector of equal entries stamping the same numbers, the
`pregame_advantage` key stamped into the config's loss_selection, and configs, arm files and recipe identities that are
unchanged when neither is used."""
import copy
import json
import struct
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parent.parent / 'ex1'))
sys.path.insert(0, str(HERE))
import chains  # noqa: E402
import qual  # noqa: E402
import xact  # noqa: E402
from test_w5_chains import Node, dose, jwrite  # noqa: E402
from test_w5_keep_offset import (KEEP, legacy_build_config, legacy_recipe_identity,  # noqa: E402
                                 opening_base)

B0 = [6.0, 12.0, 0.0, 24.0, 3.0, 0.0, 9.0]
VEC = {'b0': B0, 'end_update': 8, 'shape': 'linear-v1'}


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def vec_at(b0, end, u):
    return None if u >= end else [f32(float(b) * (end - u) / end) for b in b0]


def stamps(cfg):
    return [[e['episode'].get('london_keep_offset', 'absent') for e in item['episodes']] for item in cfg['iterations']]


class VectorValue(unittest.TestCase):
    def test_endpoints_zero_entries_and_the_end(self):
        self.assertEqual(qual.keep_offset_value(VEC, 0), B0)
        self.assertEqual(qual.keep_offset_value(VEC, 4), [3.0, 6.0, 0.0, 12.0, 1.5, 0.0, 4.5])
        last = qual.keep_offset_value(VEC, 7)
        self.assertEqual(last, vec_at(B0, 8, 7))
        self.assertEqual(last[2], 0.0)
        self.assertEqual(last[5], 0.0)
        self.assertTrue(all(isinstance(x, float) for x in last))
        for u in (8, 9, 10 ** 6):
            self.assertIsNone(qual.keep_offset_value(VEC, u))

    def test_each_entry_is_one_binary32_rounding_of_its_own_product(self):
        ko = {'b0': [15.3003, 38.6249, 18.0476, 48.2642, 51.9961, 14.9020, 34.6416], 'end_update': 32768,
              'shape': 'linear-v1'}
        for u in (0, 1, 777, 16384, 32767):
            got = qual.keep_offset_value(ko, u)
            self.assertEqual(got, [f32(b * (32768 - u) / 32768) for b in ko['b0']])
            self.assertEqual([struct.unpack('<f', struct.pack('<f', x))[0] for x in got], got)
        self.assertEqual(qual.keep_offset_value(ko, 0)[0], f32(15.3003))

    def test_a_scalar_and_seven_equal_entries_stamp_the_same_numbers(self):
        for s, end in ((15.3003, 32768), (6.0, 8), (0.001, 5), (1e6, 3)):
            vec = {'b0': [s] * 7, 'end_update': end, 'shape': 'linear-v1'}
            scalar = {'b0': s, 'end_update': end, 'shape': 'linear-v1'}
            for u in range(min(end, 40)):
                self.assertEqual(qual.keep_offset_value(vec, u), [qual.keep_offset_value(scalar, u)] * 7)
            self.assertIsNone(qual.keep_offset_value(vec, end))
            self.assertIsNone(qual.keep_offset_value(scalar, end))

    def test_a_scalar_is_still_a_number_not_a_list(self):
        self.assertIs(type(qual.keep_offset_value(KEEP, 3)), float)
        self.assertEqual(qual.validate_keep_offset(KEEP), KEEP)
        self.assertEqual(qual.validate_keep_offset(VEC), VEC)
        ints = {'b0': [6, 12, 0, 24, 3, 0, 9], 'end_update': 8, 'shape': 'linear-v1'}
        self.assertEqual(qual.validate_keep_offset(ints), VEC)
        self.assertTrue(all(type(x) is float for x in qual.validate_keep_offset(ints)['b0']))
        original = copy.deepcopy(VEC)
        qual.validate_keep_offset(VEC)
        self.assertEqual(VEC, original)

    def test_refusals(self):
        def with_b0(b0):
            return dict(VEC, b0=b0)
        cases = [with_b0([]), with_b0([1.0] * 6), with_b0([1.0] * 8), with_b0([0.0] * 7), with_b0([-1.0] + [1.0] * 6),
                 with_b0([1.0] * 6 + [float('nan')]), with_b0([1.0] * 6 + [float('inf')]),
                 with_b0([1.0] * 6 + [1e6 + 1]), with_b0([1.0] * 6 + [10 ** 400]), with_b0([1.0] * 6 + [True]),
                 with_b0([1.0] * 6 + [None]), with_b0([1.0] * 6 + ['1']), with_b0([[1.0]] * 7), with_b0((1.0,) * 7),
                 with_b0({'0': 1.0}), with_b0([-0.0001] + [1.0] * 6)]
        for case in cases:
            with self.subTest(case=case):
                with self.assertRaises(ValueError):
                    qual.validate_keep_offset(case)
                with self.assertRaises(ValueError):
                    qual.keep_offset_value(case, 0)
        for ok in (with_b0([1e6] * 7), with_b0([0.0] * 6 + [1e-30])):
            qual.validate_keep_offset(ok)

    def test_a_negative_zero_entry_is_a_positive_zero(self):
        import math
        spec = qual.validate_keep_offset(dict(VEC, b0=[-0.0, 3.0, -0.0, 0.0, 0.0, 0.0, 0.0]))
        self.assertTrue(all(math.copysign(1.0, x) > 0 for x in spec['b0']))
        stamped = qual.keep_offset_value(dict(VEC, b0=[-0.0, 3.0, -0.0, 0.0, 0.0, 0.0, 0.0]), 2)
        self.assertTrue(all(math.copysign(1.0, x) > 0 for x in stamped))

    def test_a_nonzero_entry_that_would_round_to_zero_is_refused_where_it_does(self):
        tiny = {'b0': [1e-44, 5.0, 0, 0, 0, 0, 0], 'end_update': 10 ** 6, 'shape': 'linear-v1'}
        self.assertEqual(qual.keep_offset_value(tiny, 0)[0], f32(1e-44))
        with self.assertRaisesRegex(ValueError, 'entry 0'):
            qual.keep_offset_value(tiny, 10 ** 6 - 1)
        # a zero entry is never refused for being zero
        self.assertEqual(qual.keep_offset_value(VEC, 7)[2], 0.0)


class VectorBuild(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.t = Path(self.tmp.name)
        self.dose = json.loads(dose(self.t, 'p').read_text())
        self.out = self.t / 'out'

    def build(self, base, offset=0, updates=4, keep=None, **kwargs):
        return qual.build_config(base, self.dose, self.out, 'r1', 'ns', offset, updates, 2, keep_offset=keep, **kwargs)

    def test_opening_episodes_carry_the_list_by_update_and_none_from_the_end(self):
        base = opening_base(iterations=2)
        cfg = self.build(base, 0, 12, VEC)
        for u, row in enumerate(stamps(cfg)):
            self.assertEqual(row, [vec_at(B0, 8, u)] * 2 if u < 8 else ['absent'] * 2)
        for u in (8, 9, 11):
            for e in cfg['iterations'][u]['episodes']:
                self.assertNotIn('london_keep_offset', e['episode'])
        # the list is JSON: a round trip through the config file keeps every entry exactly
        again = json.loads(json.dumps(cfg))
        self.assertEqual(stamps(again), stamps(cfg))
        plain = self.build(base, 0, 12)
        for item in cfg['iterations']:
            for e in item['episodes']:
                e['episode'].pop('london_keep_offset', None)
        self.assertEqual(cfg, plain)

    def test_a_scalar_config_is_what_it_was_and_equal_entries_stamp_seven_copies(self):
        base = opening_base()
        for kwargs in ({}, {'coefficient': 0.5, 'kappa': 2.0}):
            want = legacy_build_config(base, self.dose, self.out, 'r1', 'ns', 3, 5, 2, **kwargs)
            self.assertEqual(self.build(base, 3, 5, **kwargs), want)
        scalar = self.build(base, 0, 8, KEEP)
        vector = self.build(base, 0, 8, {'b0': [6.0] * 7, 'end_update': 8, 'shape': 'linear-v1'})
        self.assertEqual(stamps(vector), [[[x] * 7 for x in row] for row in stamps(scalar)])

    def test_the_base_must_still_be_a_london_base(self):
        with self.assertRaisesRegex(ValueError, 'opening on every episode'):
            self.build(opening_base(openings=('london-v1', None)), 0, 2, VEC)
        with self.assertRaisesRegex(ValueError, 'refused with an EX1 coefficient'):
            self.build(opening_base(), 0, 1, VEC, coefficient=1.0)


class PregameAdvantage(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.t = Path(self.tmp.name)
        self.dose = json.loads(dose(self.t, 'p').read_text())
        self.out = self.t / 'out'

    def build(self, base, offset=0, updates=3, keep=None, mode=None, **kwargs):
        return qual.build_config(base, self.dose, self.out, 'r1', 'ns', offset, updates, 2, keep_offset=keep,
                                 pregame_advantage=mode, **kwargs)

    def test_the_key_goes_into_loss_selection_and_nowhere_else(self):
        base = opening_base(iterations=2)
        original = copy.deepcopy(base)
        on, off = self.build(base, mode='terminal_v1'), self.build(base)
        self.assertEqual(base, original)
        self.assertEqual(on['loss_selection'], dict(base['loss_selection'], pregame_advantage='terminal_v1'))
        self.assertNotIn('pregame_advantage', off['loss_selection'])
        on['loss_selection'] = off['loss_selection']
        self.assertEqual(on, off)
        both = self.build(base, 0, 12, VEC, 'terminal_v1')
        self.assertEqual(both['loss_selection']['pregame_advantage'], 'terminal_v1')
        self.assertEqual(stamps(both)[0], [B0] * 2)

    def test_off_means_the_config_is_byte_identical_to_before(self):
        base = opening_base()
        want = legacy_build_config(base, self.dose, self.out, 'r1', 'ns', 0, 3, 2)
        self.assertEqual(json.dumps(self.build(base), indent=1), json.dumps(want, indent=1))
        self.assertEqual(json.dumps(self.build(base, mode=None), indent=1), json.dumps(want, indent=1))

    def test_refusals(self):
        for mode in ('gae', 'terminal', 'terminal_v2', '', True, 1, ['terminal_v1']):
            with self.subTest(mode=mode), self.assertRaisesRegex(ValueError, 'pregame_advantage must be'):
                self.build(opening_base(), mode=mode)
        for gamma in (0.99, 0.0, 1.5, None, '1.0', True):
            base = opening_base()
            base['loss_selection']['gamma'] = gamma
            with self.subTest(gamma=gamma), self.assertRaisesRegex(ValueError, 'gamma exactly 1.0'):
                self.build(base, mode='terminal_v1')
        base = opening_base()
        base['loss_selection'].pop('gamma')
        with self.assertRaisesRegex(ValueError, 'gamma exactly 1.0'):
            self.build(base, mode='terminal_v1')
        named = opening_base()
        named['loss_selection']['pregame_advantage'] = 'terminal_v1'
        with self.assertRaisesRegex(ValueError, 'already names pregame_advantage'):
            self.build(named, mode='terminal_v1')
        with self.assertRaisesRegex(ValueError, 'opening on every episode'):
            self.build(opening_base(openings=('london-v1', None)), mode='terminal_v1')
        with self.assertRaisesRegex(ValueError, 'gae_advantage_value_v1 loss'):
            self.build(dict(opening_base(), loss_selection={'kind': 'terminal_reinforce_value_v3'}),
                       mode='terminal_v1')
        with self.assertRaisesRegex(ValueError, 'CPU update backend'):
            self.build(dict(opening_base(), update_backend={'kind': 'cuda', 'device_ordinal': 0}), mode='terminal_v1')
        with self.assertRaisesRegex(ValueError, 'EX1'):
            self.build(opening_base(), mode='terminal_v1', coefficient=1.0)
        self.assertEqual(qual.validate_pregame_advantage(None), None)
        self.assertEqual(qual.validate_pregame_advantage('terminal_v1'), 'terminal_v1')

    def test_xact_passes_it_through(self):
        base = opening_base()
        cfg = xact.build_config(base, self.dose, self.out, 'ch', 0, 3, 2, keep_offset=VEC,
                                pregame_advantage='terminal_v1')
        self.assertEqual(cfg['loss_selection']['pregame_advantage'], 'terminal_v1')
        self.assertEqual(stamps(cfg)[1], [vec_at(B0, 8, 1)] * 2)
        plain = xact.build_config(base, self.dose, self.out, 'ch', 0, 3, 2)
        self.assertNotIn('pregame_advantage', plain['loss_selection'])


class Plans(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.node = Node(self.tmp.name)
        self.base = jwrite(self.node.t / 'base-open.json', opening_base())
        self.n = 0

    def inputs(self, label, **arm):
        specs = {'k1': {'arm': 'C', 'slot': 0, 'seed_root': 'w5/q1'}}
        doc = json.loads(self.node.inputs('0', specs, label=label).read_text())
        doc['arms']['C'] = dict(doc['arms']['C'], base=str(self.base), **arm)
        return doc

    def plan(self, doc):
        self.n += 1
        return chains.make_plan(jwrite(self.node.t / f'plan-{self.n}.json', doc))

    @staticmethod
    def arm(plan):
        return json.loads(Path(plan['arms']['C']['file']['path']).read_text())

    def test_a_vector_arm_is_in_the_file_the_identity_and_the_segment_configs(self):
        _, plain = self.plan(self.inputs('plain'))
        _, plan = self.plan(self.inputs('vec', keep_offset=VEC))
        arm = self.arm(plan)
        self.assertEqual(arm['keep_offset'], VEC)
        self.assertEqual(chains.recipe_identity(arm), dict(legacy_recipe_identity(arm), keep_offset=VEC))
        self.assertNotEqual(plan['arms']['C']['file']['sha256'], plain['arms']['C']['file']['sha256'])
        cfg = chains.arm_config(arm, 'w5/q1', 6, 4, self.node.t / 'o')
        self.assertEqual(stamps(cfg), [[vec_at(B0, 8, 6)] * 2, [vec_at(B0, 8, 7)] * 2, ['absent'] * 2, ['absent'] * 2])
        for bad in ({'b0': [1.0] * 6, 'end_update': 8, 'shape': 'linear-v1'}, dict(VEC, b0=[0.0] * 7),
                    dict(VEC, b0=[-1.0] + [1.0] * 6)):
            with self.subTest(bad=bad), self.assertRaises(chains.Stop):
                self.plan(self.inputs('bad', keep_offset=bad))

    def test_the_pregame_advantage_arm(self):
        _, plain = self.plan(self.inputs('plain'))
        _, plan = self.plan(self.inputs('terminal', keep_offset=VEC, pregame_advantage='terminal_v1'))
        arm = self.arm(plan)
        self.assertEqual(arm['pregame_advantage'], 'terminal_v1')
        identity = chains.recipe_identity(arm)
        self.assertEqual(identity['pregame_advantage'], 'terminal_v1')
        self.assertEqual(identity, dict(legacy_recipe_identity(arm), keep_offset=VEC, pregame_advantage='terminal_v1'))
        manifest = json.loads(Path(plan['chains']['k1']['manifest']).read_text())
        self.assertEqual(manifest['recipe']['pregame_advantage'], 'terminal_v1')
        cfg = chains.arm_config(arm, 'w5/q1', 0, 2, self.node.t / 'o')
        self.assertEqual(cfg['loss_selection']['pregame_advantage'], 'terminal_v1')
        self.assertEqual(stamps(cfg)[0], [B0] * 2)
        # the plain arm: no key anywhere, and an explicit null is no key
        plain_arm = self.arm(plain)
        self.assertNotIn('pregame_advantage', plain_arm)
        self.assertNotIn('pregame_advantage', chains.recipe_identity(plain_arm))
        _, null_plan = self.plan(self.inputs('null', pregame_advantage=None))
        self.assertEqual(null_plan['arms']['C']['file']['sha256'], plain['arms']['C']['file']['sha256'])
        self.assertNotIn('pregame_advantage', chains.arm_config(plain_arm, 'w5/q1', 0, 1,
                                                                self.node.t / 'o')['loss_selection'])
        # the mode alone (no lean) is an arm too
        _, alone = self.plan(self.inputs('alone', pregame_advantage='terminal_v1'))
        self.assertNotIn('keep_offset', self.arm(alone))

    def test_plan_refusals(self):
        for bad in ('gae', 'terminal', 1, True, [], {}):
            with self.subTest(bad=bad), self.assertRaises(chains.Stop):
                self.plan(self.inputs('bad', pregame_advantage=bad))
        gamma = opening_base()
        gamma['loss_selection']['gamma'] = 0.99
        doc = self.inputs('gamma', pregame_advantage='terminal_v1')
        doc['arms']['C']['base'] = str(jwrite(self.node.t / 'base-gamma.json', gamma))
        with self.assertRaisesRegex(chains.Stop, 'gamma exactly 1.0'):
            self.plan(doc)
        doc = self.inputs('noopen', pregame_advantage='terminal_v1')
        doc['arms']['C']['base'] = str(self.node.base)
        with self.assertRaisesRegex(chains.Stop, 'opening'):
            self.plan(doc)

    def test_a_started_chain_refuses_a_changed_mode(self):
        _, plan = self.plan(self.inputs('first', keep_offset=VEC, pregame_advantage='terminal_v1'))
        path = Path(plan['chains']['k1']['manifest'])
        man = json.loads(path.read_text())
        man['segments'] = [{'config': 'x', 'offset': 0, 'root': 'y', 'updates': 4}]
        chains.write(path, man)
        with self.assertRaisesRegex(chains.Stop, r'recipe changed.*pregame_advantage differs'):
            self.plan(self.inputs('second', keep_offset=VEC))
        with self.assertRaisesRegex(chains.Stop, 'recipe changed'):
            self.plan(self.inputs('third', keep_offset=dict(VEC, b0=[7.0] + B0[1:]), pregame_advantage='terminal_v1'))
        self.plan(self.inputs('same', keep_offset=VEC, pregame_advantage='terminal_v1'))  # the same recipe resumes


if __name__ == '__main__':
    unittest.main()
