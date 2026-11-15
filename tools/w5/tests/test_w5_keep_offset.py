"""The London keep-offset schedule in the chain tools (tools/ex1/qual.py, tools/w5/xact.py, tools/w5/chains.py):
b(u) and its binary32 rounding, the stamping of opening episodes by fork update, every refusal, configs and recipe
identities unchanged without a keep_offset, and the plan's refusal to change the schedule of a started chain."""
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

KEEP = {'b0': 6.0, 'end_update': 8, 'shape': 'linear-v1'}


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def expected(b0, end, u):
    return None if u >= end else f32(float(b0) * (end - u) / end)


def opening_base(kinds=('current', 'fixed'), openings=('london-v1', 'london-v1'), iterations=1):
    """A base whose episodes carry an opening and its mulligan modes (None: the episode has neither), on the plain
    CPU GAE update the keep offset trains on."""
    def episode(seat, opening):
        ep = {'learner_seat': seat}
        if opening is not None:
            ep['opening'] = opening
            ep['mulligan_modes'] = ['learned', 'learned']
        return ep
    return {'schema': 'test-base', 'opponents': [{'id': 'd6107922'}], 'iterations': [
        {'episodes': [{'episode': episode(s, o), 'opponent': {'kind': k}}
                      for s, (k, o) in enumerate(zip(kinds, openings))]} for _ in range(iterations)],
        'loss_selection': {'kind': 'gae_advantage_value_v1', 'gamma': 1.0, 'lambda': 0.9, 'entropy_coefficient': 0.0},
        'update_backend': {'kind': 'cpu'}}


def legacy_build_config(base, dose, root, run, namespace, offset, updates, workers, coefficient=None, kappa=None,
                        prep_workers=None):
    """tools/ex1/qual.py build_config as it was before the keep offset (the reference for 'unchanged')."""
    source = dose["source"]
    if source.get("checkpoint") is not None:
        pinned = source["checkpoint"]
    elif dose.get("fresh") is True:
        assert not source.get("s_learner") and not source.get("w_learner")
        pinned = source.get("play_import")
    else:
        pinned = (source.get("s_learner") or source.get("w_learner") or {}).get("initial")
    assert pinned is not None
    assert qual.sha(pinned["path"]) == pinned["sha256"]
    plan = base["iterations"]
    iterations = []
    for i in range(updates):
        update = offset + i
        item = copy.deepcopy(plan[update % len(plan)])
        for j, entry in enumerate(item["episodes"]):
            episode = entry["episode"]
            episode["id"] = f"{namespace}-{run}-u{update}-j{j}-s{episode['learner_seat']}"
            episode["seed"] = qual.seed([namespace, run, update, j])
        iterations.append(item)
    cfg = {k: copy.deepcopy(v) for k, v in base.items() if k != "iterations"}
    cfg["initial_source"] = source
    cfg["iterations"] = iterations
    cfg["collection_workers"] = workers
    cfg["record_process_cpu"] = True
    cfg["output_directory"] = Path(root).as_posix()
    if coefficient is not None:
        cfg["preparation_workers"] = 1 if prep_workers is None else int(prep_workers)
        cfg["ex1"] = {"run": run, "update_offset": offset, "coefficient": coefficient,
                      "cap": {"mode": "off"} if kappa is None else {"mode": "ratio", "kappa": kappa}}
    return cfg


def legacy_recipe_identity(arm):
    """tools/w5/chains.py recipe_identity as it was before the keep offset."""
    identity = {k: arm[k] for k in ('arm', 'recipe', 'base', 'parent', 'x', 'workers', 'segment_updates',
                                    'c_wall_seconds')} | {'trainer_sha256': arm['trainer']['sha256']}
    if arm.get('record_envelope'):
        identity['record_envelope'] = arm['record_envelope']
    if arm.get('verified_reads'):
        identity['verified_reads'] = arm['verified_reads']
    if arm.get('preparation_workers') is not None:
        identity['preparation_workers'] = arm['preparation_workers']
    if arm.get('ladder'):
        identity['ladder'] = arm['ladder']
    if arm.get('initial_opponent'):
        identity['initial_opponent'] = arm['initial_opponent']
    return identity


def stamps(cfg):
    """Per iteration, per episode: its london_keep_offset or the string 'absent'."""
    return [[e['episode'].get('london_keep_offset', 'absent') for e in item['episodes']]
            for item in cfg['iterations']]


class ValueTests(unittest.TestCase):
    def test_endpoints(self):
        for b0, end in ((6.0, 8), (7.3, 1000), (30, 1), (0.001, 5), (12.5, 1_048_576)):
            self.assertEqual(qual.keep_offset_value({'b0': b0, 'end_update': end, 'shape': 'linear-v1'}, 0), f32(b0))
            last = qual.keep_offset_value({'b0': b0, 'end_update': end, 'shape': 'linear-v1'}, end - 1)
            self.assertEqual(last, expected(b0, end, end - 1))
            self.assertGreater(last, 0.0)
            for u in (end, end + 1, end + 10**6):
                self.assertIsNone(qual.keep_offset_value({'b0': b0, 'end_update': end, 'shape': 'linear-v1'}, u))
        self.assertEqual(qual.keep_offset_value(KEEP, 0), 6.0)
        self.assertEqual(qual.keep_offset_value(KEEP, 4), 3.0)
        self.assertEqual(qual.keep_offset_value(KEEP, 7), 0.75)
        self.assertIsNone(qual.keep_offset_value(KEEP, 8))

    def test_binary32_rounding_is_one_step_from_binary64(self):
        ko = {'b0': 7.3, 'end_update': 1000, 'shape': 'linear-v1'}
        differs = 0
        for u in range(1000):
            value = qual.keep_offset_value(ko, u)
            self.assertEqual(value, f32(7.3 * (1000 - u) / 1000))  # float64 product and quotient, one rounding
            self.assertEqual(struct.unpack('<f', struct.pack('<f', value))[0], value)  # round trip unchanged
            self.assertIs(type(value), float)
            differs += value != 7.3 * (1000 - u) / 1000
        self.assertGreater(differs, 0)  # the rounding is real: most values are not binary32 numbers already

    def test_decay_is_linear_positive_and_never_increases(self):
        ko = {'b0': 9.0, 'end_update': 64, 'shape': 'linear-v1'}
        values = [qual.keep_offset_value(ko, u) for u in range(64)]
        self.assertTrue(all(v > 0 for v in values))
        self.assertTrue(all(a >= b for a, b in zip(values, values[1:])))
        self.assertEqual(values[32], 4.5)

    def test_integer_b0_is_a_float64_input(self):
        self.assertEqual(qual.keep_offset_value({'b0': 6, 'end_update': 8, 'shape': 'linear-v1'}, 2), 4.5)
        self.assertEqual(qual.validate_keep_offset({'b0': 6, 'end_update': 8, 'shape': 'linear-v1'}),
                         {'b0': 6.0, 'end_update': 8, 'shape': 'linear-v1'})

    def test_refusals(self):
        def bad(**change):
            doc = dict(KEEP, **change)
            for k in [k for k, v in change.items() if v is ...]:
                del doc[k]
            return doc
        cases = [None, [], 'linear-v1', 6.0, bad(b0=0), bad(b0=0.0), bad(b0=-1.0), bad(b0=float('nan')),
                 bad(b0=float('inf')), bad(b0=-float('inf')), bad(b0='6'), bad(b0=True), bad(b0=None),
                 bad(b0=1.0e6 * 2), bad(b0=10 ** 400), bad(b0=-10 ** 400), bad(b0=10 ** 6 + 1), bad(b0=...),
                 bad(end_update=0), bad(end_update=-1), bad(end_update=1.5), bad(end_update=8.0), bad(end_update=True), bad(end_update='8'), bad(end_update=None),
                 bad(end_update=2 ** 53 + 1), bad(end_update=...), bad(shape='cosine'), bad(shape='linear'),
                 bad(shape=None), bad(shape=...), bad(start_update=0), bad(extra=1), {}]
        for case in cases:
            with self.subTest(case=case):
                with self.assertRaises(ValueError):
                    qual.validate_keep_offset(case)
                with self.assertRaises(ValueError):
                    qual.keep_offset_value(case, 0)
        for u in (-1, -10, 1.0, True, '0', None):
            with self.subTest(u=u):
                with self.assertRaises(ValueError):
                    qual.keep_offset_value(KEEP, u)

    def test_a_value_the_trainer_would_refuse_is_refused(self):
        # b(T - 1) = 1e-40 / 2^53 rounds to zero in binary32: the trainer accepts only finite positive offsets
        tiny = {'b0': 1e-40, 'end_update': 2 ** 53, 'shape': 'linear-v1'}
        self.assertEqual(qual.keep_offset_value(tiny, 0), f32(1e-40))
        with self.assertRaises(ValueError):
            qual.keep_offset_value(tiny, 2 ** 53 - 1)
        self.assertEqual(qual.keep_offset_value({'b0': 1.0e6, 'end_update': 2, 'shape': 'linear-v1'}, 0), 1.0e6)


class BuildConfigTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.t = Path(self.tmp.name)
        self.dose = json.loads(dose(self.t, 'p').read_text())
        self.out = self.t / 'out'

    def build(self, base, offset=0, updates=4, keep=None, **kwargs):
        return qual.build_config(base, self.dose, self.out, 'r1', 'ns', offset, updates, 2, keep_offset=keep, **kwargs)

    def test_without_keep_offset_every_config_is_unchanged(self):
        bases = [opening_base(), opening_base(openings=(None, None)), opening_base(iterations=3),
                 opening_base(openings=('london-v1', None))]
        for base in bases:
            for kwargs in ({}, {'coefficient': 0.5, 'kappa': 2.0}, {'coefficient': 1.0, 'prep_workers': 3}):
                for offset, updates in ((0, 4), (3, 5), (1_000_003, 2)):
                    want = legacy_build_config(base, self.dose, self.out, 'r1', 'ns', offset, updates, 2, **kwargs)
                    got = self.build(base, offset, updates, **kwargs)
                    self.assertEqual(got, want)
                    self.assertEqual(json.dumps(got, indent=1), json.dumps(want, indent=1))
                    self.assertNotIn('london_keep_offset', json.dumps(got))

    def test_xact_passes_the_schedule_through_and_is_unchanged_without_it(self):
        base = opening_base()
        for budget, coefficient in ((None, None), (16384, 0.5)):
            plain = xact.build_config(base, self.dose, self.out, 'ch', 0, 4, 2, budget, coefficient)
            kappa = xact.DOSES['primary'][1]
            want = legacy_build_config(base, self.dose, self.out, 'ch', 'w5', 0, 4, 2,
                                       coefficient if budget is not None else None, kappa)
            if budget is not None:
                want['ex1']['w5'] = {'budget': budget}
            self.assertEqual(plain, want)
            if budget is not None:  # an X chain's config carries an ex1 block, which the trainer refuses with it
                with self.assertRaisesRegex(ValueError, 'refused with an EX1 coefficient'):
                    xact.build_config(base, self.dose, self.out, 'ch', 0, 6, 2, budget, coefficient,
                                      keep_offset=KEEP)
                continue
            stamped = xact.build_config(base, self.dose, self.out, 'ch', 0, 6, 2, budget, coefficient,
                                        keep_offset=KEEP)
            self.assertEqual(stamps(stamped), [[expected(6.0, 8, u)] * 2 for u in range(6)])
            for item in stamped['iterations']:  # nothing else differs from the plain construction
                for e in item['episodes']:
                    e['episode'].pop('london_keep_offset')
            self.assertEqual(stamped['iterations'][:4], plain['iterations'])
            with self.assertRaises(ValueError):
                xact.build_config(base, self.dose, self.out, 'ch', 0, 4, 2, budget, coefficient,
                                  keep_offset=dict(KEEP, b0=0))

    def test_every_opening_episode_is_stamped_per_update_and_nothing_else_changes(self):
        base = opening_base(iterations=2)
        original = copy.deepcopy(base)
        cfg = self.build(base, 0, 12, KEEP)
        self.assertEqual(base, original)  # the base is not touched
        for u, row in enumerate(stamps(cfg)):
            self.assertEqual(row, [expected(6.0, 8, u)] * 2 if u < 8 else ['absent'] * 2)
        self.assertEqual(stamps(cfg)[0], [6.0, 6.0])
        self.assertEqual(stamps(cfg)[7], [0.75, 0.75])
        for u in (8, 9, 11):
            for e in cfg['iterations'][u]['episodes']:
                self.assertNotIn('london_keep_offset', e['episode'])  # no key at all, not null, not zero
        plain = self.build(base, 0, 12)
        for item in cfg['iterations']:
            for e in item['episodes']:
                e['episode'].pop('london_keep_offset', None)
        self.assertEqual(cfg, plain)

    def test_every_episode_of_every_iteration_needs_an_opening(self):
        # The trainer refuses an update batch that mixes offset and no offset, and trains a batch with no offset and
        # no London pregame silently: a base with any opening-free episode is refused at plan and build time.
        def without_opening(base, i, j):
            ep = base['iterations'][i]['episodes'][j]['episode']
            ep.pop('opening')
            ep.pop('mulligan_modes')
            return base
        null_opening = opening_base()
        null_opening['iterations'][0]['episodes'][1]['episode']['opening'] = None
        bases = [opening_base(openings=('london-v1', None)), null_opening, opening_base(openings=(None, None)),
                 without_opening(opening_base(iterations=3), 2, 0),  # the last iteration only
                 without_opening(opening_base(iterations=3), 1, 1)]
        for base in bases:
            with self.subTest(base=base):
                with self.assertRaisesRegex(ValueError, 'opening on every episode'):
                    qual.validate_keep_offset(KEEP, base)
                for updates in (1, 2, 10):  # even a build that would not reach the iteration
                    with self.assertRaisesRegex(ValueError, 'opening on every episode'):
                        self.build(base, 0, updates, KEEP)
        for u, row in enumerate(stamps(self.build(opening_base(iterations=3), 0, 10, KEEP))):
            self.assertEqual(row, [expected(6.0, 8, u) or 'absent'] * 2)  # all or none per update, never half

    def test_the_opening_needs_its_companion_settings_and_the_plain_cpu_gae_update(self):
        def bad(change, text):
            base = opening_base()
            change(base)
            with self.subTest(text=text):
                with self.assertRaisesRegex(ValueError, text):
                    qual.validate_keep_offset(KEEP, base)
                with self.assertRaisesRegex(ValueError, text):
                    self.build(base, 0, 1, KEEP)
        first = lambda base: base['iterations'][0]['episodes'][0]['episode']  # noqa: E731
        bad(lambda b: first(b).update(opening='london-v2'), "'london-v2' is not 'london-v1'")
        bad(lambda b: first(b).pop('mulligan_modes'), 'needs mulligan_modes')
        bad(lambda b: first(b).update(mulligan_modes=['learned']), 'needs mulligan_modes')
        bad(lambda b: first(b).update(mulligan_modes=['learned', 'random']), 'needs mulligan_modes')
        bad(lambda b: first(b).update(mulligan_modes='learned'), 'needs mulligan_modes')
        bad(lambda b: first(b).update(mulligan_modes=['keep_seven', 'learned']), 'learner seat 0 needs the learned')
        bad(lambda b: first(b).update(learner_seat=2), 'learner seat 2')
        bad(lambda b: first(b).update(learner_seat=True), 'learner seat True')
        bad(lambda b: first(b).update(reset={'prefix': []}), 'refused with reset')
        bad(lambda b: first(b).update(opponent_search={'budget': 1}), 'refused with opponent_search')
        bad(lambda b: b.pop('loss_selection'), 'gae_advantage_value_v1 loss')
        bad(lambda b: b.update(loss_selection={'kind': 'terminal_reinforce_value_v3'}), 'gae_advantage_value_v1 loss')
        bad(lambda b: b.update(update_backend={'kind': 'cuda', 'device_ordinal': 0}), 'CPU update backend')
        for option in ('line_b', 'e_supported', 'soft_q', 'search_targets', 'ex1'):  # the trainer's own list
            bad(lambda b, option=option: b.update({option: {'any': 'block'}}), f"the base's {option} is refused")
        # keep_seven on the opponent's seat is how a fixed opponent plays, an absent backend is the CPU default, and
        # an explicit null option (as chain A's base writes them) is no option
        ok = opening_base()
        first(ok)['mulligan_modes'] = ['learned', 'keep_seven']
        ok.pop('update_backend')
        ok.update({option: None for option in qual.KEEP_OFFSET_REFUSED_OPTIONS})
        self.assertEqual(qual.validate_keep_offset(KEEP, ok), KEEP)
        with self.assertRaisesRegex(ValueError, 'refused with an EX1 coefficient'):  # a direct X-style build
            self.build(opening_base(), 0, 1, KEEP, coefficient=1.0)

    def test_a_segment_spanning_the_end_stamps_each_iteration_by_its_own_update(self):
        base = opening_base()
        cfg = self.build(base, 6, 4, KEEP)  # updates 6, 7, 8, 9
        self.assertEqual(stamps(cfg), [[1.5, 1.5], [0.75, 0.75], ['absent'] * 2, ['absent'] * 2])
        whole = self.build(base, 0, 12, KEEP)['iterations']
        self.assertEqual(self.build(base, 0, 5, KEEP)['iterations'] + self.build(base, 5, 4, KEEP)['iterations'] +
                         self.build(base, 9, 3, KEEP)['iterations'], whole)

    def test_the_base_schedule_cycles_but_the_stamp_follows_the_global_update(self):
        base = opening_base(iterations=3)
        cfg = self.build(base, 5, 4, KEEP)
        self.assertEqual(stamps(cfg), [[expected(6.0, 8, u)] * 2 if u < 8 else ['absent'] * 2 for u in range(5, 9)])

    def test_a_fork_is_numbered_from_zero_whatever_its_parent_reached(self):
        # the chain tools' updates count from 0 at the first segment of a chain, so a keep_offset has no start
        with self.assertRaises(ValueError):
            qual.validate_keep_offset(dict(KEEP, start_update=1_048_576))

    def test_refusals_in_a_build(self):
        with self.assertRaisesRegex(ValueError, 'opening'):
            self.build(opening_base(openings=(None, None)), 0, 2, KEEP)
        carried = opening_base()
        carried['iterations'][0]['episodes'][0]['episode']['london_keep_offset'] = 3.0
        with self.assertRaisesRegex(ValueError, 'already carries'):
            self.build(carried, 0, 2, KEEP)
        with self.assertRaisesRegex(ValueError, 'non-negative'):
            self.build(opening_base(), -1, 2, KEEP)
        for bad in (dict(KEEP, b0=0), dict(KEEP, b0=-2.0), dict(KEEP, b0=float('nan')), dict(KEEP, end_update=0),
                    dict(KEEP, end_update=-5), dict(KEEP, shape='exp'), dict(KEEP, extra=1), {}, []):
            with self.assertRaises(ValueError):
                self.build(opening_base(), 0, 2, bad)
        # without a schedule an opening-free base, a negative offset and the like behave as before
        self.assertEqual(self.build(opening_base(openings=(None, None)), 0, 2)['iterations'][0]['episodes'][0]
                         ['episode'], {'learner_seat': 0, 'id': 'ns-r1-u0-j0-s0', 'seed': qual.seed(['ns', 'r1', 0, 0])})


class ChainTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.node = Node(self.tmp.name)
        self.base = jwrite(self.node.t / 'base-open.json', opening_base())
        self.n = 0

    def inputs(self, keep=None, label='k', specs=None, arms=('C',), x_keep=None):
        specs = specs or {'k1': {'arm': 'C', 'slot': 0, 'seed_root': 'w5/q1'}}
        doc = json.loads(self.node.inputs('0', specs, label=label).read_text())
        for a in arms:
            k = x_keep if a != 'C' else keep
            doc['arms'][a] = dict(doc['arms'][a], base=str(self.base), **({'keep_offset': k} if k is not None else {}))
        return doc

    def plan(self, doc):
        self.n += 1
        return chains.make_plan(jwrite(self.node.t / f'plan-{self.n}.json', doc))

    @staticmethod
    def arm(plan, name='C'):
        return json.loads(Path(plan['arms'][name]['file']['path']).read_text())

    def test_without_keep_offset_the_recipe_identity_and_files_are_unchanged(self):
        for base in (None, self.base):  # the stock base, then a base that has openings but no schedule
            doc = self.inputs(None, label='plain')
            if base is None:
                doc['arms']['C'].pop('base')
            _, plan = self.plan(doc)
            arm = self.arm(plan)
            self.assertNotIn('keep_offset', arm)
            identity = chains.recipe_identity(arm)
            self.assertEqual(identity, legacy_recipe_identity(arm))
            self.assertEqual(sorted(identity), sorted(['arm', 'recipe', 'base', 'parent', 'x', 'workers',
                                                       'segment_updates', 'c_wall_seconds', 'trainer_sha256']))
            manifest = json.loads(Path(plan['chains']['k1']['manifest']).read_text())
            self.assertEqual(manifest['recipe'], json.loads(json.dumps(identity)))
            self.assertNotIn('keep_offset', manifest['recipe'])
            self.assertNotIn('london_keep_offset', json.dumps(chains.arm_config(arm, 'w5/q1', 0, 4, self.node.t / 'o')))
            want = legacy_build_config(json.loads(Path(arm['base']['path']).read_text()),
                                       json.loads(Path(arm['parent']['path']).read_text()), self.node.t / 'o',
                                       'w5/q1', 'w5', 0, 4, arm['workers'], None, xact.DOSES['primary'][1])
            self.assertEqual(chains.arm_config(arm, 'w5/q1', 0, 4, self.node.t / 'o'), want)

    def test_a_planned_schedule_is_in_the_arm_file_the_identity_and_every_segment_config(self):
        _, plain = self.plan(self.inputs(None, label='plain'))
        _, plan = self.plan(self.inputs(KEEP, label='keep'))
        arm = self.arm(plan)
        self.assertEqual(arm['keep_offset'], KEEP)
        self.assertNotEqual(plan['arms']['C']['file']['sha256'], plain['arms']['C']['file']['sha256'])
        self.assertEqual(chains.recipe_identity(arm), dict(legacy_recipe_identity(arm), keep_offset=KEEP))
        manifest = json.loads(Path(plan['chains']['k1']['manifest']).read_text())
        self.assertEqual(manifest['recipe']['keep_offset'], KEEP)
        cfg = chains.arm_config(arm, 'w5/q1', 6, 4, self.node.t / 'o')  # a segment that spans the end
        self.assertEqual(stamps(cfg), [[1.5, 1.5], [0.75, 0.75], ['absent'] * 2, ['absent'] * 2])
        first = chains.arm_config(arm, 'w5/q1', 0, 4, self.node.t / 'o')
        self.assertEqual(stamps(first), [[expected(6.0, 8, u)] * 2 for u in range(4)])
        with self.assertRaises(chains.Stop):
            chains.arm_config(arm, 'w5/q1', -1, 2, self.node.t / 'o')
        # an integer b0 in the inputs is stored as the float it stands for
        _, plan_int = self.plan(self.inputs({'b0': 6, 'end_update': 8, 'shape': 'linear-v1'}, label='int'))
        self.assertEqual(plan_int['arms']['C']['file']['sha256'], plan['arms']['C']['file']['sha256'])

    def test_a_plan_refuses_a_bad_schedule(self):
        for bad in (dict(KEEP, b0=0), dict(KEEP, b0=-1.0), dict(KEEP, b0=float('inf')), dict(KEEP, end_update=0),
                    dict(KEEP, b0=10 ** 400),  # an int no float holds: a Stop, not an OverflowError
                    dict(KEEP, end_update=2.5), dict(KEEP, shape='cosine'), dict(KEEP, start_update=0),
                    {'b0': 6.0, 'end_update': 8}, {}, 'linear-v1', [6.0, 8]):
            with self.subTest(bad=bad), self.assertRaises(chains.Stop):
                self.plan(self.inputs(bad, label='bad'))
        plain_base = self.node.base  # no opening on any episode
        doc = self.inputs(KEEP, label='noopen')
        doc['arms']['C']['base'] = str(plain_base)
        with self.assertRaisesRegex(chains.Stop, 'opening'):
            self.plan(doc)
        # a base with an opening on only some episodes, or on a non-CPU or non-GAE update, is refused at plan time
        mixed = opening_base(openings=('london-v1', None))
        cuda = dict(opening_base(), update_backend={'kind': 'cuda', 'device_ordinal': 0})
        terminal = dict(opening_base(), loss_selection={'kind': 'terminal_reinforce_value_v3'})
        for name, odd, text in (('mixed', mixed, 'opening on every episode'), ('cuda', cuda, 'CPU update backend'),
                                ('terminal', terminal, 'gae_advantage_value_v1 loss')):
            doc = self.inputs(KEEP, label=name)
            doc['arms']['C']['base'] = str(jwrite(self.node.t / f'base-{name}.json', odd))
            with self.subTest(name=name), self.assertRaisesRegex(chains.Stop, text):
                self.plan(doc)
        # an explicit null is the same as no schedule
        _, plan = self.plan(self.inputs(None, label='absent'))  # (labels are directory names: none reserved on Windows)
        doc = self.inputs(None, label='explicit-null')
        doc['arms']['C']['keep_offset'] = None
        _, again = self.plan(doc)
        self.assertEqual(again['arms']['C']['file']['sha256'], plan['arms']['C']['file']['sha256'])

    def test_a_started_chain_refuses_another_schedule(self):
        _, plan = self.plan(self.inputs(KEEP, label='first'))
        path = Path(plan['chains']['k1']['manifest'])
        # before any segment, the manifest takes the new recipe (as for every other recipe key)
        longer = dict(KEEP, end_update=16)
        _, plan = self.plan(self.inputs(longer, label='longer'))
        self.assertEqual(json.loads(path.read_text())['recipe']['keep_offset'], longer)
        man = json.loads(path.read_text())
        man['segments'] = [{'config': (self.node.t / 'seg.json').as_posix(), 'offset': 0,
                            'root': (self.node.t / 'seg').as_posix(), 'updates': 4}]
        chains.write(path, man)
        for other in (KEEP, dict(longer, b0=7.0), dict(longer, end_update=17), None):
            with self.subTest(other=other), self.assertRaisesRegex(chains.Stop, 'recipe changed.*keep_offset'):
                self.plan(self.inputs(other, label='other'))
        # the same schedule resumes
        _, plan = self.plan(self.inputs(longer, label='same'))
        self.assertEqual(json.loads(path.read_text())['recipe']['keep_offset'], longer)
        # and a chain that started without one cannot gain one
        _, plain = self.plan(self.inputs(None, label='plain', specs={'p1': {'arm': 'C', 'slot': 1,
                                                                       'seed_root': 'w5/q2'}}))
        pman = Path(plain['chains']['p1']['manifest'])
        man = json.loads(pman.read_text())
        man['segments'] = [{'config': 'x', 'offset': 0, 'root': 'y', 'updates': 4}]
        chains.write(pman, man)
        with self.assertRaisesRegex(chains.Stop, 'recipe changed.*keep_offset'):
            self.plan(self.inputs(KEEP, label='gain', specs={'p1': {'arm': 'C', 'slot': 1, 'seed_root': 'w5/q2'}}))

    def test_the_arm_check_binds_the_recipe_of_a_running_chain(self):
        _, plan = self.plan(self.inputs(KEEP, label='run'))
        arm = self.arm(plan)
        man = json.loads(Path(plan['chains']['k1']['manifest']).read_text())
        self.assertEqual(man['recipe'], json.loads(json.dumps(chains.recipe_identity(arm))))
        changed = dict(arm, keep_offset=dict(KEEP, b0=7.0))
        self.assertNotEqual(man['recipe'], json.loads(json.dumps(chains.recipe_identity(changed))))

    def test_an_x_arm_cannot_carry_the_schedule_and_a_pair_must_agree(self):
        # The trainer refuses the offset with a search (EX1) block, which every X-recipe arm carries: it would collect
        # and then fail at its first update, so the plan refuses it.
        specs = {'c1': {'arm': 'C', 'slot': 0, 'pair': 1, 'seed_root': 'w5/q1'},
                 'x1': {'arm': 'X8', 'slot': 1, 'pair': 1, 'seed_root': 'w5/q1'}}
        for x_keep in (KEEP, dict(KEEP, b0=7.0)):
            with self.assertRaisesRegex(chains.Stop, 'C-recipe'):
                self.plan(self.inputs(KEEP, label='pair', specs=specs, arms=('C', 'X8'), x_keep=x_keep))
        # an X arm with no schedule beside a C arm with one: the pair's two configs differ, which the plan refuses
        with self.assertRaisesRegex(chains.Stop, 'differ'):
            self.plan(self.inputs(KEEP, label='pair3', specs=specs, arms=('C', 'X8'), x_keep=None))


if __name__ == '__main__':
    unittest.main()
