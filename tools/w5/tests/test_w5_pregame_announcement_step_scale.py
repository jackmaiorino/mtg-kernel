"""The pregame announcement step scale in the chain tools (tools/ex1/qual.py, tools/w5/xact.py, tools/w5/chains.py,
tools/w5/mull_monitor.py): the `pregame_announcement_step_scale` arm key stamped into the config's loss_selection,
validated (a number in (0, 1) exclusive, only with `pregame_advantage` terminal_v1), part of the recipe identity only
when present, refused when it changes on a started chain, and exposed by the monitor as the routed announcement rows
and the two gradient buffers' norms. The earlier pre-normalisation key is refused by name. Configs, arm files,
identities and monitor lines are unchanged when the key is not used."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parent.parent / 'ex1'))
sys.path.insert(0, str(HERE))
import chains  # noqa: E402
import mull_monitor as mm  # noqa: E402
import qual  # noqa: E402
import xact  # noqa: E402
from test_w5_chains import Node, dose, jwrite  # noqa: E402
from test_w5_keep_offset import legacy_build_config, legacy_recipe_identity, opening_base  # noqa: E402
from test_w5_mull_monitor_counts import ROWS, SEATS, VECTOR, with_rows  # noqa: E402

B0 = [6.0, 12.0, 0.0, 24.0, 3.0, 0.0, 9.0]
VEC = {'b0': B0, 'end_update': 8, 'shape': 'linear-v1'}


class Validation(unittest.TestCase):
    def test_accepted_and_refused_values(self):
        for good in (0.5, 0.003, 0.03, 0.3, 1e-9, 0.9999999):
            self.assertEqual(qual.validate_pregame_announcement_step_scale(good, 'terminal_v1'), good)
        self.assertIsNone(qual.validate_pregame_announcement_step_scale(None, 'terminal_v1'))
        self.assertIsNone(qual.validate_pregame_announcement_step_scale(None, None))
        for bad in (0, 0.0, -0.5, 1, 1.0, 0.99999999999, 1.0000001, 2, True, False, '0.5', [0.5], float('nan'),
                    float('inf'), object(), 1e-50, 1e50, 10**400):
            with self.subTest(bad=bad), self.assertRaisesRegex(ValueError, r'pregame_announcement_step_scale must be'):
                qual.validate_pregame_announcement_step_scale(bad, 'terminal_v1')
        for mode in (None, 'gae'):
            with self.subTest(mode=mode), self.assertRaisesRegex(ValueError, 'needs pregame_advantage terminal_v1'):
                qual.validate_pregame_announcement_step_scale(0.5, mode)


class Configs(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.t = Path(self.tmp.name)
        self.dose = json.loads(dose(self.t, 'p').read_text())
        self.out = self.t / 'out'

    def build(self, base, **kwargs):
        return qual.build_config(base, self.dose, self.out, 'r1', 'ns', 0, 3, 2, **kwargs)

    def test_a_base_that_names_a_scale_key_never_builds_a_config(self):
        for key, pattern in (('pregame_announcement_step_scale', 'already names pregame_announcement_step_scale'),
                             (qual.RETIRED_PREGAME_SCALE_KEY, 'no trainer takes')):
            base = opening_base(iterations=2)
            base['loss_selection'] = dict(base['loss_selection'], **{key: 0.25})
            for kwargs in ({}, {'pregame_advantage': 'terminal_v1'},
                           {'pregame_advantage': 'terminal_v1', 'pregame_announcement_step_scale': 0.03}):
                with self.subTest(key=key, kwargs=kwargs), self.assertRaisesRegex(ValueError, pattern):
                    self.build(base, **kwargs)
        # a base without them still builds without an arm mode
        self.build(opening_base(iterations=2))

    def test_the_key_goes_into_loss_selection_only_with_the_mode(self):
        base = opening_base(iterations=2)
        plain = self.build(base, pregame_advantage='terminal_v1')
        scaled = self.build(base, pregame_advantage='terminal_v1', pregame_announcement_step_scale=0.25)
        self.assertEqual(scaled['loss_selection'], dict(plain['loss_selection'], pregame_announcement_step_scale=0.25))
        self.assertNotIn('pregame_announcement_step_scale', plain['loss_selection'])
        scaled['loss_selection'] = plain['loss_selection']
        self.assertEqual(scaled, plain)
        with self.assertRaisesRegex(ValueError, 'needs pregame_advantage terminal_v1'):
            self.build(base, pregame_announcement_step_scale=0.25)
        with self.assertRaisesRegex(ValueError, 'must be a number'):
            self.build(base, pregame_advantage='terminal_v1', pregame_announcement_step_scale=1.5)
        inherited = opening_base()
        inherited['loss_selection'] = dict(inherited['loss_selection'], pregame_announcement_step_scale=0.25)
        with self.assertRaisesRegex(ValueError, "already names pregame_announcement_step_scale"):
            self.build(inherited, pregame_advantage='terminal_v1')

    def test_absent_is_byte_identical_to_before(self):
        base = opening_base()
        want = legacy_build_config(base, self.dose, self.out, 'r1', 'ns', 0, 3, 2)
        for kwargs in ({}, {'pregame_announcement_step_scale': None}):
            self.assertEqual(json.dumps(self.build(base, **kwargs), indent=1), json.dumps(want, indent=1))

    def test_xact_passes_it_through(self):
        base = opening_base()
        cfg = xact.build_config(base, self.dose, self.out, 'ch', 0, 3, 2, keep_offset=VEC,
                                pregame_advantage='terminal_v1', pregame_announcement_step_scale=0.1)
        self.assertEqual(cfg['loss_selection']['pregame_announcement_step_scale'], 0.1)
        plain = xact.build_config(base, self.dose, self.out, 'ch', 0, 3, 2, keep_offset=VEC,
                                  pregame_advantage='terminal_v1')
        self.assertNotIn('pregame_announcement_step_scale', plain['loss_selection'])


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

    def test_a_scaled_arm_is_in_the_file_the_identity_and_the_segment_configs(self):
        _, terminal = self.plan(self.inputs('terminal', keep_offset=VEC, pregame_advantage='terminal_v1'))
        _, plan = self.plan(self.inputs('scaled', keep_offset=VEC, pregame_advantage='terminal_v1',
                                        pregame_announcement_step_scale=0.25))
        arm = self.arm(plan)
        self.assertEqual(arm['pregame_announcement_step_scale'], 0.25)
        identity = chains.recipe_identity(arm)
        self.assertEqual(identity['pregame_announcement_step_scale'], 0.25)
        self.assertEqual(identity, dict(legacy_recipe_identity(arm), keep_offset=VEC, pregame_advantage='terminal_v1',
                                        pregame_announcement_step_scale=0.25))
        self.assertNotEqual(plan['arms']['C']['file']['sha256'], terminal['arms']['C']['file']['sha256'])
        manifest = json.loads(Path(plan['chains']['k1']['manifest']).read_text())
        self.assertEqual(manifest['recipe']['pregame_announcement_step_scale'], 0.25)
        cfg = chains.arm_config(arm, 'w5/q1', 0, 2, self.node.t / 'o')
        self.assertEqual(cfg['loss_selection']['pregame_announcement_step_scale'], 0.25)
        # without the key: no key anywhere, the arm file and the identity keep their bytes, null is no key
        terminal_arm = self.arm(terminal)
        self.assertNotIn('pregame_announcement_step_scale', terminal_arm)
        self.assertNotIn('pregame_announcement_step_scale', chains.recipe_identity(terminal_arm))
        self.assertNotIn('pregame_announcement_step_scale', chains.arm_config(terminal_arm, 'w5/q1', 0, 1,
                                                                      self.node.t / 'o')['loss_selection'])
        _, null_plan = self.plan(self.inputs('null', keep_offset=VEC, pregame_advantage='terminal_v1',
                                             pregame_announcement_step_scale=None))
        self.assertEqual(null_plan['arms']['C']['file']['sha256'], terminal['arms']['C']['file']['sha256'])

    def test_plan_refusals(self):
        for bad in (0, 0.0, -1, 1, 1.0, 1.5, True, '0.5', [], {}):
            with self.subTest(bad=bad), self.assertRaises(chains.Stop):
                self.plan(self.inputs('bad', pregame_advantage='terminal_v1', pregame_announcement_step_scale=bad))
        with self.assertRaisesRegex(chains.Stop, 'needs pregame_advantage terminal_v1'):
            self.plan(self.inputs('alone', pregame_announcement_step_scale=0.5))

    def test_the_retired_key_is_refused_by_name(self):
        retired = qual.RETIRED_PREGAME_SCALE_KEY
        self.assertNotEqual(retired, 'pregame_announcement_step_scale')
        # in an arm: alone, with a null value, and beside the new key
        for label, extra in (('old', {retired: 0.25}), ('old-null', {retired: None}),
                             ('both', {retired: 0.25, 'pregame_announcement_step_scale': 0.25})):
            with self.subTest(label), self.assertRaisesRegex(chains.Stop, 'no trainer takes'):
                self.plan(self.inputs(label, keep_offset=VEC, pregame_advantage='terminal_v1', **extra))
        # in a base's own loss_selection
        base = opening_base(iterations=2)
        base['loss_selection'] = dict(base['loss_selection'], **{retired: 0.25})
        with self.assertRaisesRegex(ValueError, 'no trainer takes'):
            qual.validate_pregame_advantage('terminal_v1', base)

    def test_a_base_that_names_a_scale_key_is_refused_whatever_the_arm_names(self):
        # Either key in the base's own loss_selection, with an arm that names no pregame_advantage at all (so the
        # mode's own base check never runs), a terminal arm, and a scaled arm: the plan refuses each.
        retired = qual.RETIRED_PREGAME_SCALE_KEY
        new = 'pregame_announcement_step_scale'
        for tag, key, pattern in (('new', new, f'already names {new}'), ('old', retired, 'no trainer takes')):
            doc = opening_base()
            doc['loss_selection'] = dict(doc['loss_selection'], **{key: 0.25})
            base = jwrite(self.node.t / f'base-{tag}.json', doc)
            for label, arm in (('plain', {}), ('terminal', {'keep_offset': VEC, 'pregame_advantage': 'terminal_v1'}),
                               ('scaled', {'keep_offset': VEC, 'pregame_advantage': 'terminal_v1',
                                           'pregame_announcement_step_scale': 0.03})):
                inputs = self.inputs(f'{label}-{tag}', **arm)
                inputs['arms']['C']['base'] = str(base)
                with self.subTest(key=key, arm=label), self.assertRaisesRegex(chains.Stop, pattern):
                    self.plan(inputs)

    def test_a_started_chain_refuses_a_changed_scale(self):
        _, plan = self.plan(self.inputs('first', keep_offset=VEC, pregame_advantage='terminal_v1',
                                        pregame_announcement_step_scale=0.25))
        path = Path(plan['chains']['k1']['manifest'])
        man = json.loads(path.read_text())
        man['segments'] = [{'config': 'x', 'offset': 0, 'root': 'y', 'updates': 4}]
        chains.write(path, man)
        for label, arm in (('other', dict(pregame_announcement_step_scale=0.5)), ('none', {})):
            with self.subTest(label), self.assertRaisesRegex(chains.Stop,
                                                              r'recipe changed.*pregame_announcement_step_scale differs'):
                self.plan(self.inputs(label, keep_offset=VEC, pregame_advantage='terminal_v1', **arm))
        self.plan(self.inputs('same', keep_offset=VEC, pregame_advantage='terminal_v1',
                              pregame_announcement_step_scale=0.25))  # the same recipe resumes

    def test_a_started_chain_refuses_an_added_scale(self):
        _, plan = self.plan(self.inputs('first', keep_offset=VEC, pregame_advantage='terminal_v1'))
        path = Path(plan['chains']['k1']['manifest'])
        man = json.loads(path.read_text())
        self.assertNotIn('pregame_announcement_step_scale', man['recipe'])
        man['segments'] = [{'config': 'x', 'offset': 0, 'root': 'y', 'updates': 4}]
        chains.write(path, man)
        with self.assertRaisesRegex(chains.Stop, r'recipe changed.*pregame_announcement_step_scale differs'):
            self.plan(self.inputs('added', keep_offset=VEC, pregame_advantage='terminal_v1',
                                  pregame_announcement_step_scale=0.25))
        self.plan(self.inputs('same', keep_offset=VEC, pregame_advantage='terminal_v1'))  # the same recipe resumes

    def test_the_arm_key_reaches_every_segment_config(self):
        # The arm definition's key is carried into the arm document, the recipe identity and the config of every
        # segment (the trainer writes its run manifest and update commands from that config's loss_selection).
        _, plan = self.plan(self.inputs('scaled', keep_offset=VEC, pregame_advantage='terminal_v1',
                                        pregame_announcement_step_scale=0.03))
        arm = self.arm(plan)
        self.assertEqual(arm['pregame_announcement_step_scale'], 0.03)
        self.assertEqual(chains.recipe_identity(arm)['pregame_announcement_step_scale'], 0.03)
        for offset, updates in ((0, 2), (2, 2), (6, 4), (8, 2)):  # before, across and after the lean's end at 8
            cfg = chains.arm_config(arm, 'w5/q1', offset, updates, self.node.t / f'o{offset}')
            self.assertEqual(cfg['loss_selection']['pregame_announcement_step_scale'], 0.03, offset)
            self.assertEqual(cfg['loss_selection']['pregame_advantage'], 'terminal_v1', offset)
            self.assertNotIn(qual.RETIRED_PREGAME_SCALE_KEY, cfg['loss_selection'])


class Monitor(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.t = Path(self.tmp.name)

    def iteration(self, name, receipt):
        d = self.t / 'iterations' / name
        d.mkdir(parents=True)
        update = d / 'update.json'
        doc = {'pregame_advantage': {'mode': 'terminal_v1', 'pregame_groups': 14}}
        if receipt is not None:
            doc['pregame_announcement_step'] = receipt
        update.write_text(json.dumps(doc))
        (d / 'complete.json').write_text(json.dumps({'update': {'path': update.as_posix()}}))
        return d

    def test_the_scaled_fields_are_read_only_when_the_receipt_has_them(self):
        full = {'scale': 0.25, 'routed_groups': 12, 'announcement_gradient_l2': 0.0123456789,
                'rest_gradient_l2': 1.23456789}
        self.assertEqual(mm.read_pregame_scale(self.iteration('000001', full)),
                         {'s': 0.25, 'n': 12, 'ga': 0.0123457, 'gr': 1.23457})
        empty = dict(full, routed_groups=0, announcement_gradient_l2=0.0)
        self.assertEqual(mm.read_pregame_scale(self.iteration('000002', empty)),
                         {'s': 0.25, 'n': 0, 'ga': 0.0, 'gr': 1.23457})
        # an unscaled update's receipt has no such member: nothing is read
        self.assertIsNone(mm.read_pregame_scale(self.iteration('000003', None)))
        self.assertIsNone(mm.read_pregame_scale(self.iteration('000004', {})))
        self.assertIsNone(mm.read_pregame_scale(self.t / 'iterations' / 'missing'))

    def test_the_report_sums_the_means_over_updates_and_an_unscaled_report_has_no_key(self):
        cdir = self.t / 'chain'
        seg = cdir / 'seg-00000'
        seg.mkdir(parents=True)
        episodes = [mm.read_episode(with_rows(SEATS, ROWS, offset=qual.keep_offset_value(VECTOR, 0)))]
        for it, pas in enumerate(({'s': 0.25, 'n': 10, 'ga': 0.02, 'gr': 1.0},
                                  {'s': 0.25, 'n': 30, 'ga': 0.04, 'gr': 2.0}, None)):
            line = mm.iteration_line(it, episodes)
            if pas is not None:
                line['pas'] = pas
            mm.append_line(mm.opening_log_path(seg), line)
        (seg / 'completion.json').write_bytes(b'{}')
        jwrite(cdir / 'chain.json', {'schema': 'w5-chain/v1', 'chain': 'n1', 'arm_name': 'C',
                                     'recipe': {'keep_offset': VECTOR, 'pregame_advantage': 'terminal_v1',
                                                'pregame_announcement_step_scale': 0.25},
                                     'segments': [{'config': 'x', 'offset': 0, 'root': (cdir / 'seg-00000').as_posix(),
                                                   'updates': 3}]})
        report = mm.summarize_chain(cdir)
        scale = report['pregame_scale']
        self.assertEqual(scale['scale'], 0.25)
        self.assertEqual((scale['updates'], scale['announcement_rows']), (2, 40))
        self.assertAlmostEqual(scale['announcement_gradient_l2_mean'], 0.03)
        self.assertAlmostEqual(scale['rest_gradient_l2_mean'], 1.5)
        self.assertIn('pregame announcement step scale 0.25', '\n'.join(mm.format_by_count(report)))
        # no scaled line: no key, no text
        plain = self.t / 'plain'
        pseg = plain / 'seg-00000'
        pseg.mkdir(parents=True)
        mm.append_line(mm.opening_log_path(pseg), mm.iteration_line(0, episodes))
        (pseg / 'completion.json').write_bytes(b'{}')
        jwrite(plain / 'chain.json', {'schema': 'w5-chain/v1', 'chain': 'n2', 'arm_name': 'C',
                                      'recipe': {'keep_offset': VECTOR},
                                      'segments': [{'config': 'x', 'offset': 0, 'root': pseg.as_posix(),
                                                    'updates': 1}]})
        report = mm.summarize_chain(plain)
        self.assertNotIn('pregame_scale', report)
        self.assertNotIn('pregame announcement step scale', '\n'.join(mm.format_by_count(report)))


if __name__ == '__main__':
    unittest.main()
