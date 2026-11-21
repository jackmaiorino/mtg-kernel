"""The priority pass-offset schedule in the chain tools (tools/ex1/qual.py, tools/w5/xact.py, tools/w5/chains.py):
b(u) and its binary32 rounding, the stamping of episodes with priority_windows by fork update, every refusal, configs
and recipe identities unchanged without a pass_offset, and the plan's refusal to change the schedule of a started
chain. It mirrors test_w5_keep_offset.py."""
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
from test_w5_keep_offset import KEEP, legacy_build_config, legacy_recipe_identity, opening_base  # noqa: E402

PASS = {'b0': 6.0, 'end_update': 8, 'shape': 'linear-v1'}
ACTION = 'action-windows-v1'
END_STEP = 'action-windows-end-step-v1'
HARNESS = 'harness-v2'


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def expected(b0, end, u):
    return None if u >= end else f32(float(b0) * (end - u) / end)


def windows_base(windows=((ACTION, HARNESS), (HARNESS, END_STEP)), iterations=1, opening=False):
    """A base whose episodes carry priority_windows (None: the episode has none), on the plain CPU GAE update the
    pass offset trains on; with `opening`, also the London opening."""
    base = opening_base(iterations=iterations) if opening else opening_base(openings=(None, None),
                                                                           iterations=iterations)
    for item in base['iterations']:
        for entry, rules in zip(item['episodes'], windows):
            if rules is not None:
                entry['episode']['priority_windows'] = list(rules)
    return base


def stamps(cfg, key='priority_pass_offset'):
    """Per iteration, per episode: its priority_pass_offset (or `key`) or the string 'absent'."""
    return [[e['episode'].get(key, 'absent') for e in item['episodes']] for item in cfg['iterations']]


def without_pass_offset(cfg):
    cfg = copy.deepcopy(cfg)
    for item in cfg['iterations']:
        for e in item['episodes']:
            e['episode'].pop('priority_pass_offset', None)
    return cfg


class ValueTests(unittest.TestCase):
    def test_endpoints_and_rounding(self):
        for b0, end in ((6.0, 8), (7.3, 1000), (30, 1), (0.001, 5), (1.0e6, 2)):
            po = {'b0': b0, 'end_update': end, 'shape': 'linear-v1'}
            self.assertEqual(qual.pass_offset_value(po, 0), f32(b0))
            self.assertEqual(qual.pass_offset_value(po, end - 1), expected(b0, end, end - 1))
            for u in (end, end + 1, end + 10 ** 6):
                self.assertIsNone(qual.pass_offset_value(po, u))
        self.assertEqual([qual.pass_offset_value(PASS, u) for u in (0, 4, 7, 8)], [6.0, 3.0, 0.75, None])
        po = {'b0': 7.3, 'end_update': 1000, 'shape': 'linear-v1'}
        for u in range(1000):
            value = qual.pass_offset_value(po, u)
            self.assertEqual(value, f32(7.3 * (1000 - u) / 1000))
            self.assertEqual(struct.unpack('<f', struct.pack('<f', value))[0], value)
            self.assertIs(type(value), float)
        # the same arithmetic as a scalar keep offset
        for u in range(9):
            self.assertEqual(qual.pass_offset_value(PASS, u), qual.keep_offset_value(KEEP, u))

    def test_refusals(self):
        def bad(**change):
            doc = dict(PASS, **change)
            for k in [k for k, v in change.items() if v is ...]:
                del doc[k]
            return doc
        cases = [None, [], 'linear-v1', 6.0, bad(b0=0), bad(b0=0.0), bad(b0=-1.0), bad(b0=float('nan')),
                 bad(b0=float('inf')), bad(b0='6'), bad(b0=True), bad(b0=None), bad(b0=[6.0] * 7),
                 bad(b0=1.0e6 * 2), bad(b0=10 ** 400), bad(b0=...), bad(end_update=0), bad(end_update=1.5),
                 bad(end_update=8.0), bad(end_update=True), bad(end_update=2 ** 53 + 1), bad(end_update=...),
                 bad(shape='cosine'), bad(shape=...), bad(start_update=0), bad(extra=1), {}]
        for case in cases:
            with self.subTest(case=case):
                with self.assertRaises(ValueError):
                    qual.validate_pass_offset(case)
                with self.assertRaises(ValueError):
                    qual.pass_offset_value(case, 0)
        for u in (-1, 1.0, True, '0', None):
            with self.subTest(u=u), self.assertRaises(ValueError):
                qual.pass_offset_value(PASS, u)
        self.assertEqual(qual.validate_pass_offset({'b0': 6, 'end_update': 8, 'shape': 'linear-v1'}), PASS)


class BuildConfigTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.t = Path(self.tmp.name)
        self.dose = json.loads(dose(self.t, 'p').read_text())
        self.out = self.t / 'out'

    def build(self, base, offset=0, updates=4, pass_offset=None, **kwargs):
        return qual.build_config(base, self.dose, self.out, 'r1', 'ns', offset, updates, 2, pass_offset=pass_offset,
                                 **kwargs)

    def test_without_pass_offset_every_config_is_unchanged(self):
        for base in (windows_base(), windows_base(iterations=3), opening_base(openings=(None, None))):
            for kwargs in ({}, {'coefficient': 0.5, 'kappa': 2.0}):
                for offset, updates in ((0, 4), (3, 5)):
                    want = legacy_build_config(base, self.dose, self.out, 'r1', 'ns', offset, updates, 2, **kwargs)
                    got = self.build(base, offset, updates, **kwargs)
                    self.assertEqual(json.dumps(got, indent=1), json.dumps(want, indent=1))
                    self.assertNotIn('priority_pass_offset', json.dumps(got))

    def test_every_windows_episode_is_stamped_per_update_and_nothing_else_changes(self):
        base = windows_base(iterations=2)
        original = copy.deepcopy(base)
        cfg = self.build(base, 0, 12, PASS)
        self.assertEqual(base, original)
        for u, row in enumerate(stamps(cfg)):
            self.assertEqual(row, [expected(6.0, 8, u)] * 2 if u < 8 else ['absent'] * 2)
        self.assertEqual(without_pass_offset(cfg), self.build(base, 0, 12))
        # a segment spanning the end stamps each iteration by its own update
        self.assertEqual(stamps(self.build(base, 6, 4, PASS)), [[1.5, 1.5], [0.75, 0.75], ['absent'] * 2,
                                                                ['absent'] * 2])

    def test_with_the_keep_offset_both_fields_follow_their_own_schedule(self):
        base = windows_base(opening=True)
        cfg = self.build(base, 0, 10, PASS, keep_offset=dict(KEEP, end_update=4))
        for u in range(10):
            self.assertEqual(stamps(cfg)[u], [expected(6.0, 8, u) or 'absent'] * 2)
            self.assertEqual(stamps(cfg, 'london_keep_offset')[u], [expected(6.0, 4, u) or 'absent'] * 2)

    def test_xact_passes_the_schedule_through(self):
        base = windows_base()
        plain = xact.build_config(base, self.dose, self.out, 'ch', 0, 4, 2)
        stamped = xact.build_config(base, self.dose, self.out, 'ch', 0, 4, 2, pass_offset=PASS)
        self.assertEqual(stamps(stamped), [[expected(6.0, 8, u)] * 2 for u in range(4)])
        self.assertEqual(without_pass_offset(stamped), plain)
        with self.assertRaisesRegex(ValueError, 'refused with an EX1 coefficient'):
            xact.build_config(base, self.dose, self.out, 'ch', 0, 4, 2, 16384, 0.5, pass_offset=PASS)

    def test_the_base_needs_action_windows_on_every_episode_and_the_plain_cpu_gae_update(self):
        def bad(base, text):
            with self.subTest(text=text):
                with self.assertRaisesRegex(ValueError, text):
                    qual.validate_pass_offset(PASS, base)
                with self.assertRaisesRegex(ValueError, text):
                    self.build(base, 0, 1, PASS)
        bad(windows_base(((ACTION, HARNESS), None)), 'priority_windows on every episode')
        bad(windows_base(iterations=3) | {'iterations': windows_base(iterations=2)['iterations'] +
                                          windows_base(((ACTION, HARNESS), None))['iterations']},
            'priority_windows on every episode')
        bad(windows_base(((ACTION, HARNESS), (HARNESS, HARNESS))), 'action-window rule')
        bad(windows_base(((ACTION, 'action-windows-v2'), (ACTION, HARNESS))), 'must be two of')
        bad(windows_base(((ACTION,), (ACTION, HARNESS))), 'must be two of')
        first = lambda base: base['iterations'][0]['episodes'][0]['episode']  # noqa: E731
        for key, text in (('priority_pass_offset', 'already carries'), ('reset', 'refused with reset'),
                          ('opponent_search', 'refused with opponent_search')):
            base = windows_base()
            first(base)[key] = 3.0 if key == 'priority_pass_offset' else {'any': 1}
            bad(base, text)
        bad(dict(windows_base(), loss_selection={'kind': 'terminal_reinforce_value_v3'}), 'gae_advantage_value_v1')
        bad(dict(windows_base(), update_backend={'kind': 'cuda', 'device_ordinal': 0}), 'CPU update backend')
        for option in qual.KEEP_OFFSET_REFUSED_OPTIONS:
            bad(dict(windows_base(), **{option: {'any': 'block'}}), f"the base's {option} is refused")
        with self.assertRaisesRegex(ValueError, 'non-negative'):
            self.build(windows_base(), -1, 2, PASS)
        # without a schedule such bases build as before
        self.build(windows_base(((ACTION, HARNESS), None)), 0, 2)


class ChainTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.node = Node(self.tmp.name)
        self.base = jwrite(self.node.t / 'base-windows.json', windows_base())
        self.n = 0

    def inputs(self, pass_offset=None, label='p', specs=None, arms=('C',), x_pass=None):
        specs = specs or {'k1': {'arm': 'C', 'slot': 0, 'seed_root': 'w5/q1'}}
        doc = json.loads(self.node.inputs('0', specs, label=label).read_text())
        for a in arms:
            p = x_pass if a != 'C' else pass_offset
            doc['arms'][a] = dict(doc['arms'][a], base=str(self.base),
                                  **({'pass_offset': p} if p is not None else {}))
        return doc

    def plan(self, doc):
        self.n += 1
        return chains.make_plan(jwrite(self.node.t / f'plan-{self.n}.json', doc))

    @staticmethod
    def arm(plan, name='C'):
        return json.loads(Path(plan['arms'][name]['file']['path']).read_text())

    def test_without_pass_offset_the_recipe_identity_and_files_are_unchanged(self):
        _, plan = self.plan(self.inputs(None, label='plain'))
        arm = self.arm(plan)
        self.assertNotIn('pass_offset', arm)
        self.assertEqual(chains.recipe_identity(arm), legacy_recipe_identity(arm))
        manifest = json.loads(Path(plan['chains']['k1']['manifest']).read_text())
        self.assertNotIn('pass_offset', manifest['recipe'])
        cfg = chains.arm_config(arm, 'w5/q1', 0, 4, self.node.t / 'o')
        self.assertNotIn('priority_pass_offset', json.dumps(cfg))
        doc = self.inputs(None, label='explicit-null')
        doc['arms']['C']['pass_offset'] = None
        _, again = self.plan(doc)
        self.assertEqual(again['arms']['C']['file']['sha256'], plan['arms']['C']['file']['sha256'])

    def test_a_planned_schedule_is_in_the_arm_file_the_identity_and_every_segment_config(self):
        _, plain = self.plan(self.inputs(None, label='plain'))
        _, plan = self.plan(self.inputs(PASS, label='pass'))
        arm = self.arm(plan)
        self.assertEqual(arm['pass_offset'], PASS)
        self.assertNotEqual(plan['arms']['C']['file']['sha256'], plain['arms']['C']['file']['sha256'])
        self.assertEqual(chains.recipe_identity(arm), dict(legacy_recipe_identity(arm), pass_offset=PASS))
        manifest = json.loads(Path(plan['chains']['k1']['manifest']).read_text())
        self.assertEqual(manifest['recipe']['pass_offset'], PASS)
        cfg = chains.arm_config(arm, 'w5/q1', 6, 4, self.node.t / 'o')
        self.assertEqual(stamps(cfg), [[1.5, 1.5], [0.75, 0.75], ['absent'] * 2, ['absent'] * 2])
        with self.assertRaises(chains.Stop):
            chains.arm_config(arm, 'w5/q1', -1, 2, self.node.t / 'o')
        _, plan_int = self.plan(self.inputs({'b0': 6, 'end_update': 8, 'shape': 'linear-v1'}, label='int'))
        self.assertEqual(plan_int['arms']['C']['file']['sha256'], plan['arms']['C']['file']['sha256'])

    def test_a_plan_refuses_a_bad_schedule_or_base(self):
        for bad in (dict(PASS, b0=0), dict(PASS, b0=10 ** 400), dict(PASS, b0=[6.0] * 7), dict(PASS, end_update=0),
                    dict(PASS, shape='cosine'), {}, 'linear-v1'):
            with self.subTest(bad=bad), self.assertRaises(chains.Stop):
                self.plan(self.inputs(bad, label='bad'))
        for name, odd, text in (('nowin', windows_base(((ACTION, HARNESS), None)), 'priority_windows on every'),
                                ('harness', windows_base(((HARNESS, HARNESS), (ACTION, HARNESS))), 'action-window'),
                                ('cuda', dict(windows_base(), update_backend={'kind': 'cuda', 'device_ordinal': 0}),
                                 'CPU update backend')):
            doc = self.inputs(PASS, label=name)
            doc['arms']['C']['base'] = str(jwrite(self.node.t / f'base-{name}.json', odd))
            with self.subTest(name=name), self.assertRaisesRegex(chains.Stop, text):
                self.plan(doc)
        specs = {'c1': {'arm': 'C', 'slot': 0, 'pair': 1, 'seed_root': 'w5/q1'},
                 'x1': {'arm': 'X8', 'slot': 1, 'pair': 1, 'seed_root': 'w5/q1'}}
        with self.assertRaisesRegex(chains.Stop, 'C-recipe'):
            self.plan(self.inputs(PASS, label='pair', specs=specs, arms=('C', 'X8'), x_pass=PASS))

    def test_a_started_chain_refuses_another_schedule(self):
        _, plan = self.plan(self.inputs(PASS, label='first'))
        path = Path(plan['chains']['k1']['manifest'])
        man = json.loads(path.read_text())
        man['segments'] = [{'config': (self.node.t / 'seg.json').as_posix(), 'offset': 0,
                            'root': (self.node.t / 'seg').as_posix(), 'updates': 4}]
        chains.write(path, man)
        for other in (dict(PASS, b0=7.0), dict(PASS, end_update=9), None):
            with self.subTest(other=other), self.assertRaisesRegex(chains.Stop, 'recipe changed.*pass_offset'):
                self.plan(self.inputs(other, label='other'))
        _, plan = self.plan(self.inputs(PASS, label='same'))
        self.assertEqual(json.loads(path.read_text())['recipe']['pass_offset'], PASS)


if __name__ == '__main__':
    unittest.main()
