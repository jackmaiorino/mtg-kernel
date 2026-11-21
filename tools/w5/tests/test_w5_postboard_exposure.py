"""The post-board exposure in the chain tools (tools/ex1/qual.py, tools/w5/xact.py, tools/w5/chains.py,
tools/w5/runtime_parity.py): the declared share of post-board games and their exchanges, every refusal, configs,
arm files and recipe identities unchanged without a postboard_exposure, the plan's refusal to change the exposure of
a started chain, and the parity receipt's recorded exception. It mirrors test_w5_pass_offset.py."""
import copy
import hashlib
import json
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parent.parent / 'ex1'))
sys.path.insert(0, str(HERE))
import chains  # noqa: E402
import qual  # noqa: E402
import runtime_parity as rp  # noqa: E402
import xact  # noqa: E402
from test_w5_chains import Node, dose, jwrite  # noqa: E402
from test_w5_keep_offset import legacy_build_config, legacy_recipe_identity, opening_base  # noqa: E402
from test_w5_pass_offset import PASS, windows_base  # noqa: E402

LANDS = [0, 1]
EXPOSURE = {'share': [1, 4], 'max_swaps': 6, 'rule': 'uniform-swaps-v1', 'seats': 'learner-and-current-v1',
            'keep_ids': LANDS}
ALWAYS = dict(EXPOSURE, share=[1, 1])
# The frozen draws: sha256 of the canonical JSON of the 64 iterations `pinned_config` builds. A change to any draw
# (the gate, a count, a card out or in) or to the seat rule changes it; a started chain depends on these bytes.
PINNED_SHA256 = 'b51cdf800961c678066b5bb35ace07c143e9e46c2dd08d0ed7975687697d4e63'


def deck(label, shift=0, overlap=True):
    """An exact 60 and 15 registration, sorted: 20 lands (ids 0 and 1), ten spells four times each, and a sideboard
    of five cards three times each. With `overlap` one of them (the first spell) is also in the mainboard, so an
    exchange of that card for itself can leave a deck unchanged."""
    spells = [10 + shift + i for i in range(10)]
    side = [spells[0] if overlap else 44 + shift] + [40 + shift + i for i in range(4)]
    return {'label': label, 'mainboard': sorted([0] * 12 + [1] * 8 + spells * 4), 'sideboard': sorted(side * 3)}


def deck_base(kinds=('current', 'fixed'), iterations=1, opening=True, windows=False, overlap=True):
    """A game-one base whose episodes carry two exact registrations (learner seat 0 in the first episode, 1 in the
    second), with the London opening and optionally priority windows."""
    base = windows_base(iterations=iterations, opening=opening) if windows else \
        opening_base(kinds=kinds, iterations=iterations) if opening else \
        opening_base(kinds=kinds, openings=(None,) * len(kinds), iterations=iterations)
    for item in base['iterations']:
        for entry in item['episodes']:
            decks = [deck('A/aaaa', overlap=overlap), deck('B/bbbb', shift=20, overlap=overlap)]
            entry['episode'].update(registered=copy.deepcopy(decks), selected=copy.deepcopy(decks), postboard=False,
                                    starting_player=0, max_physical_decisions=100, max_policy_steps=200)
    return base


def entries(cfg):
    return [entry for item in cfg['iterations'] for entry in item['episodes']]


def multiset(d):
    return Counter(d['mainboard']) + Counter(d['sideboard'])


def strip_exposure(cfg):
    """The config with every post-board game turned back into its game one."""
    cfg = copy.deepcopy(cfg)
    for entry in entries(cfg):
        entry['episode']['selected'] = copy.deepcopy(entry['episode']['registered'])
        entry['episode']['postboard'] = False
    return cfg


class ValidationTests(unittest.TestCase):
    def test_the_normal_form(self):
        self.assertEqual(qual.validate_postboard_exposure(EXPOSURE), EXPOSURE)
        self.assertIsNot(qual.validate_postboard_exposure(EXPOSURE), EXPOSURE)
        self.assertEqual(qual.validate_postboard_exposure(dict(EXPOSURE, keep_ids=[])), dict(EXPOSURE, keep_ids=[]))
        for share in ([1, 1], [1, 2 ** 20], [2 ** 20, 2 ** 20]):
            self.assertEqual(qual.validate_postboard_exposure(dict(EXPOSURE, share=share))['share'], share)
        for swaps in (1, 15):
            self.assertEqual(qual.validate_postboard_exposure(dict(EXPOSURE, max_swaps=swaps))['max_swaps'], swaps)

    def test_refusals(self):
        def bad(**change):
            doc = dict(EXPOSURE, **change)
            for k in [k for k, v in change.items() if v is ...]:
                del doc[k]
            return doc
        cases = [None, [], 'uniform-swaps-v1', 0.25, {}, bad(share=0.25), bad(share=[1]), bad(share=[0, 4]),
                 bad(share=[5, 4]), bad(share=[1, 0]), bad(share=[1.0, 4]), bad(share=[True, 4]),
                 bad(share=[1, 2 ** 20 + 1]), bad(share=(1, 4)), bad(share=...), bad(max_swaps=0), bad(max_swaps=16),
                 bad(max_swaps=2.0), bad(max_swaps=True), bad(max_swaps=...), bad(rule='uniform-nonland-swaps-v1'),
                 bad(rule=...), bad(seats='both'), bad(seats=...), bad(keep_ids=None), bad(keep_ids=[1, 0]),
                 bad(keep_ids=[0, 0]), bad(keep_ids=[-1]), bad(keep_ids=[65536]), bad(keep_ids=[0.0]),
                 bad(keep_ids=[True]), bad(keep_ids=...), bad(end_update=8), bad(extra=1)]
        for case in cases:
            with self.subTest(case=case), self.assertRaises(ValueError):
                qual.validate_postboard_exposure(case)

    def test_the_base_must_be_a_game_one_base_with_exact_registrations(self):
        def bad(change, text):
            base = deck_base()
            change(base['iterations'][0]['episodes'][1])
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, text):
                qual.validate_postboard_exposure(EXPOSURE, base)
        qual.validate_postboard_exposure(EXPOSURE, deck_base())
        bad(lambda e: e['episode'].update(postboard=True), 'game-one base')
        bad(lambda e: e['episode'].pop('postboard'), 'game-one base')
        bad(lambda e: e['episode']['selected'][0]['mainboard'].reverse(), 'selected equal to registered')
        bad(lambda e: e['episode'].update(registered=e['episode']['registered'][:1],
                                          selected=e['episode']['selected'][:1]), 'two registrations')

        def short(e):
            for key in ('registered', 'selected'):
                e['episode'][key][1]['sideboard'].pop()
        bad(short, 'exactly 60 mainboard and 15 sideboard')

        def long(e):
            for key in ('registered', 'selected'):
                e['episode'][key][0]['mainboard'].append(0)
        bad(long, 'exactly 60 mainboard and 15 sideboard')

        def odd_id(e):
            for key in ('registered', 'selected'):
                e['episode'][key][0]['mainboard'][0] = 70000
        bad(odd_id, 'exactly 60 mainboard and 15 sideboard')
        bad(lambda e: e['episode'].update(learner_seat=2), 'learner seat')
        bad(lambda e: e['episode'].update(reset={'prefix': []}), 'refused with reset')
        bad(lambda e: e['episode'].update(opponent_search={'any': 1}), 'refused with opponent_search')
        bad(lambda e: e.update(opponent={'kind': 'recent'}), 'opponent kind')
        bad(lambda e: e.pop('opponent'), 'opponent kind')


class ExchangeTests(unittest.TestCase):
    def test_an_exchange_conserves_the_75_keeps_the_lands_and_sorts(self):
        registered = deck('A/aaaa')
        counts, sizes = Counter(), Counter()
        for n in range(400):
            swapped = qual.postboard_exposure_swap(registered, EXPOSURE, ['t', n])
            if swapped is None:
                counts['unchanged'] += 1
                continue
            self.assertEqual(swapped['label'], registered['label'])
            self.assertEqual((len(swapped['mainboard']), len(swapped['sideboard'])), (60, 15))
            self.assertEqual(multiset(swapped), multiset(registered))
            self.assertEqual(swapped['mainboard'], sorted(swapped['mainboard']))
            self.assertEqual(swapped['sideboard'], sorted(swapped['sideboard']))
            for land in LANDS:
                self.assertEqual(swapped['mainboard'].count(land), registered['mainboard'].count(land))
            moved = sum((Counter(swapped['mainboard']) - Counter(registered['mainboard'])).values())
            self.assertTrue(1 <= moved <= EXPOSURE['max_swaps'])
            sizes[moved] += 1
        self.assertEqual(registered, deck('A/aaaa'))  # the registration itself is never touched
        # every exchange size occurs, and an exchange of a card for itself is possible (one id is in both lists)
        self.assertEqual(sorted(sizes), [1, 2, 3, 4, 5, 6])
        self.assertTrue(all(sizes[k] > 10 for k in sizes), sizes)

    def test_without_keep_ids_lands_may_leave_and_one_swap_moves_one_card(self):
        registered = deck('A/aaaa')
        spec = dict(EXPOSURE, keep_ids=[], max_swaps=1)
        lands_left = 0
        for n in range(300):
            swapped = qual.postboard_exposure_swap(registered, spec, ['t', n])
            if swapped is None:
                continue
            gone = Counter(registered['mainboard']) - Counter(swapped['mainboard'])
            self.assertEqual(sum(gone.values()), 1)
            lands_left += any(card in LANDS for card in gone)
        self.assertTrue(40 < lands_left < 160, lands_left)  # about a third of the mainboard is land

    def test_a_deck_with_nothing_movable_is_left_alone(self):
        registered = deck('A/aaaa')
        spec = dict(EXPOSURE, keep_ids=sorted(set(registered['mainboard'])))
        self.assertIsNone(qual.postboard_exposure_swap(registered, spec, ['t', 0]))


class BuildConfigTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.t = Path(self.tmp.name)
        self.dose = json.loads(dose(self.t, 'p').read_text())
        self.out = self.t / 'out'

    def build(self, base, offset=0, updates=4, exposure=None, **kwargs):
        if exposure is not None:
            kwargs['postboard_exposure'] = exposure
        return qual.build_config(base, self.dose, self.out, 'r1', 'ns', offset, updates, 2, **kwargs)

    def test_without_an_exposure_every_config_is_unchanged(self):
        for base in (deck_base(), deck_base(iterations=3), deck_base(opening=False), opening_base()):
            for kwargs in ({}, {'coefficient': 0.5, 'kappa': 2.0}):
                for offset, updates in ((0, 4), (3, 5)):
                    want = legacy_build_config(base, self.dose, self.out, 'r1', 'ns', offset, updates, 2, **kwargs)
                    got = self.build(base, offset, updates, **kwargs)
                    self.assertEqual(json.dumps(got, indent=1), json.dumps(want, indent=1))
                    self.assertNotIn('"postboard": true', json.dumps(got))

    def test_the_declared_share_of_games_is_post_board_and_nothing_else_changes(self):
        base = deck_base(iterations=2)
        original = copy.deepcopy(base)
        cfg = self.build(base, 0, 2000, EXPOSURE)
        self.assertEqual(base, original)
        plain = self.build(base, 0, 2000)
        self.assertEqual(strip_exposure(cfg), plain)
        rows = entries(cfg)
        post = [e for e in rows if e['episode']['postboard']]
        self.assertTrue(0.22 < len(post) / len(rows) < 0.28, len(post))  # the share is 1 in 4 of 4,000 games
        for entry in rows:
            ep = entry['episode']
            learner = ep['learner_seat']
            exposed = [learner, 1 - learner] if entry['opponent']['kind'] == 'current' else [learner]
            changed = [seat for seat in (0, 1) if ep['selected'][seat] != ep['registered'][seat]]
            self.assertEqual(ep['postboard'], bool(changed))
            self.assertTrue(set(changed) <= set(exposed), (changed, exposed))
            for seat in (0, 1):
                self.assertEqual(multiset(ep['selected'][seat]), multiset(ep['registered'][seat]))
                self.assertEqual(ep['selected'][seat]['label'], ep['registered'][seat]['label'])
        # both exposed seats of a self-play game change (independently), a fixed opponent's seat never does
        self.assertTrue(any(len([s for s in (0, 1) if e['episode']['selected'][s] != e['episode']['registered'][s]])
                            == 2 for e in post if e['opponent']['kind'] == 'current'))
        self.assertTrue(all(e['episode']['selected'][1 - e['episode']['learner_seat']]
                            == e['episode']['registered'][1 - e['episode']['learner_seat']]
                            for e in post if e['opponent']['kind'] == 'fixed'))

    def test_a_share_of_one_exposes_every_game_and_an_initial_opponent_keeps_its_60(self):
        cfg = self.build(deck_base(kinds=('initial', 'current'), overlap=False), 0, 50, ALWAYS)
        for entry in entries(cfg):
            ep = entry['episode']
            self.assertTrue(ep['postboard'])
            if entry['opponent']['kind'] == 'initial':
                self.assertEqual(ep['selected'][1 - ep['learner_seat']], ep['registered'][1 - ep['learner_seat']])

    def test_a_game_does_not_depend_on_where_its_segment_starts(self):
        base = deck_base(iterations=3)
        whole = self.build(base, 0, 40, EXPOSURE)['iterations']
        for offset, updates in ((0, 7), (7, 9), (16, 24), (39, 1)):
            part = self.build(base, offset, updates, EXPOSURE)['iterations']
            self.assertEqual(part, whole[offset:offset + updates])
        other = qual.build_config(base, self.dose, self.out, 'r2', 'ns', 0, 40, 2, postboard_exposure=EXPOSURE)
        self.assertNotEqual([e['episode']['selected'] for e in entries(other)],
                            [e['episode']['selected'] for item in whole for e in item['episodes']])

    def test_the_draws_are_frozen(self):
        cfg = self.build(deck_base(iterations=2), 0, 64, EXPOSURE)
        text = json.dumps(cfg['iterations'], sort_keys=True, separators=(',', ':'))
        self.assertEqual(hashlib.sha256(text.encode()).hexdigest(), PINNED_SHA256)

    def test_with_the_offsets_every_field_follows_its_own_rule(self):
        base = deck_base(windows=True, overlap=False)
        cfg = self.build(base, 0, 12, ALWAYS, pass_offset=PASS, keep_offset=dict(PASS, end_update=4))
        for u, item in enumerate(cfg['iterations']):
            for entry in item['episodes']:
                ep = entry['episode']
                self.assertTrue(ep['postboard'])
                self.assertEqual('priority_pass_offset' in ep, u < 8)
                self.assertEqual('london_keep_offset' in ep, u < 4)
        only = self.build(base, 0, 12, ALWAYS)
        self.assertEqual([e['episode']['selected'] for e in entries(cfg)],
                         [e['episode']['selected'] for e in entries(only)])

    def test_xact_passes_the_exposure_through_and_refuses_it_with_search(self):
        base = deck_base()
        plain = xact.build_config(base, self.dose, self.out, 'ch', 0, 40, 2)
        exposed = xact.build_config(base, self.dose, self.out, 'ch', 0, 40, 2, postboard_exposure=EXPOSURE)
        self.assertEqual(strip_exposure(exposed), plain)
        self.assertTrue(any(e['episode']['postboard'] for e in entries(exposed)))
        direct = qual.build_config(base, self.dose, self.out, 'ch', 'w5', 0, 40, 2, postboard_exposure=EXPOSURE)
        self.assertEqual(exposed['iterations'], direct['iterations'])
        with self.assertRaisesRegex(ValueError, 'refused with an EX1 coefficient'):
            xact.build_config(base, self.dose, self.out, 'ch', 0, 4, 2, 16384, 0.5, postboard_exposure=EXPOSURE)
        with self.assertRaisesRegex(ValueError, 'non-negative'):
            self.build(base, -1, 2, EXPOSURE)
        with self.assertRaisesRegex(ValueError, 'game-one base'):
            self.build(strip_exposure(exposed) | {'iterations': exposed['iterations']}, 0, 2, EXPOSURE)


class ChainTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.node = Node(self.tmp.name)
        self.base = jwrite(self.node.t / 'base-decks.json', deck_base(opening=False))
        self.n = 0

    def inputs(self, exposure=None, label='p', specs=None, arms=('C',)):
        specs = specs or {'k1': {'arm': 'C', 'slot': 0, 'seed_root': 'w5/q1'}}
        doc = json.loads(self.node.inputs('0', specs, label=label).read_text())
        for a in arms:
            doc['arms'][a] = dict(doc['arms'][a], base=str(self.base),
                                  **({'postboard_exposure': exposure} if exposure is not None else {}))
        return doc

    def plan(self, doc):
        self.n += 1
        return chains.make_plan(jwrite(self.node.t / f'plan-{self.n}.json', doc))

    @staticmethod
    def arm(plan, name='C'):
        return json.loads(Path(plan['arms'][name]['file']['path']).read_text())

    def test_without_an_exposure_the_recipe_identity_and_files_are_unchanged(self):
        _, plan = self.plan(self.inputs(None, label='plain'))
        arm = self.arm(plan)
        self.assertNotIn('postboard_exposure', arm)
        self.assertEqual(chains.recipe_identity(arm), legacy_recipe_identity(arm))
        manifest = json.loads(Path(plan['chains']['k1']['manifest']).read_text())
        self.assertNotIn('postboard_exposure', manifest['recipe'])
        cfg = chains.arm_config(arm, 'w5/q1', 0, 40, self.node.t / 'o')
        self.assertFalse(any(e['episode']['postboard'] for e in entries(cfg)))
        doc = self.inputs(None, label='explicit-null')
        doc['arms']['C']['postboard_exposure'] = None
        _, again = self.plan(doc)
        self.assertEqual(again['arms']['C']['file']['sha256'], plan['arms']['C']['file']['sha256'])

    def test_a_planned_exposure_is_in_the_arm_file_the_identity_and_every_segment_config(self):
        _, plain = self.plan(self.inputs(None, label='plain'))
        _, plan = self.plan(self.inputs(EXPOSURE, label='exposed'))
        arm = self.arm(plan)
        self.assertEqual(arm['postboard_exposure'], EXPOSURE)
        self.assertNotEqual(plan['arms']['C']['file']['sha256'], plain['arms']['C']['file']['sha256'])
        self.assertEqual(chains.recipe_identity(arm), dict(legacy_recipe_identity(arm), postboard_exposure=EXPOSURE))
        manifest = json.loads(Path(plan['chains']['k1']['manifest']).read_text())
        self.assertEqual(manifest['recipe']['postboard_exposure'], EXPOSURE)
        whole = chains.arm_config(arm, 'w5/q1', 0, 40, self.node.t / 'o')
        self.assertTrue(any(e['episode']['postboard'] for e in entries(whole)))
        self.assertEqual(strip_exposure(whole), chains.arm_config(self.arm(plain), 'w5/q1', 0, 40, self.node.t / 'o'))
        part = chains.arm_config(arm, 'w5/q1', 16, 8, self.node.t / 'o')
        self.assertEqual(part['iterations'], whole['iterations'][16:24])
        with self.assertRaises(chains.Stop):
            chains.arm_config(arm, 'w5/q1', -1, 2, self.node.t / 'o')

    def test_a_plan_refuses_a_bad_exposure_or_base(self):
        for bad in (dict(EXPOSURE, share=[0, 4]), dict(EXPOSURE, max_swaps=16), dict(EXPOSURE, rule='other'),
                    dict(EXPOSURE, keep_ids=[1, 0]), {}, 'uniform-swaps-v1'):
            with self.subTest(bad=bad), self.assertRaises(chains.Stop):
                self.plan(self.inputs(bad, label='bad'))
        doc = self.inputs(EXPOSURE, label='nodecks')
        doc['arms']['C']['base'] = str(jwrite(self.node.t / 'base-nodecks.json',
                                              opening_base(openings=(None, None))))
        with self.assertRaisesRegex(chains.Stop, 'game-one base'):
            self.plan(doc)
        specs = {'c1': {'arm': 'C', 'slot': 0, 'pair': 1, 'seed_root': 'w5/q1'},
                 'x1': {'arm': 'X8', 'slot': 1, 'pair': 1, 'seed_root': 'w5/q1'}}
        with self.assertRaisesRegex(chains.Stop, 'C-recipe'):
            self.plan(self.inputs(EXPOSURE, label='pair', specs=specs, arms=('C', 'X8')))

    def test_a_started_chain_refuses_another_exposure(self):
        _, plan = self.plan(self.inputs(EXPOSURE, label='first'))
        path = Path(plan['chains']['k1']['manifest'])
        man = json.loads(path.read_text())
        man['segments'] = [{'config': (self.node.t / 'seg.json').as_posix(), 'offset': 0,
                            'root': (self.node.t / 'seg').as_posix(), 'updates': 4}]
        chains.write(path, man)
        for other in (dict(EXPOSURE, share=[1, 2]), dict(EXPOSURE, max_swaps=5), dict(EXPOSURE, keep_ids=[0]), None):
            with self.subTest(other=other), self.assertRaisesRegex(chains.Stop,
                                                                   'recipe changed.*postboard_exposure'):
                self.plan(self.inputs(other, label='other'))
        _, plan = self.plan(self.inputs(EXPOSURE, label='same'))
        self.assertEqual(json.loads(path.read_text())['recipe']['postboard_exposure'], EXPOSURE)


class ParityTests(unittest.TestCase):
    """runtime_parity.py: a set with post-board games fails ordinary starts unless the flag records it."""
    MANIFEST = {'information_rules': {'rules': {'mulligan': 'none', 'starting_player': 'host_assigned'},
                                      'engine_defaults': {}},
                'engine_profile': {'rules_supported': {'mulligan': ['none', 'london'],
                                                       'starting_player': ['host_assigned']}},
                'engine': {'source_revision': 'x', 'rules_snapshot_id': 'mtg-kernel-rules/x/carddb-y'}}
    LIMITS = {'max_decisions': 100, 'max_steps': 200}

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dose = json.loads(dose(Path(self.tmp.name), 'p').read_text())

    def config(self, exposure=None):
        base = deck_base(opening=False, overlap=False)
        kwargs = {} if exposure is None else {'postboard_exposure': exposure}
        cfg = qual.build_config(base, self.dose, Path(self.tmp.name) / 'o', 'r1', 'ns', 0, 40, 2, **kwargs)
        for n, entry in enumerate(entries(cfg)):  # both starting seats, the learner first and second
            entry['episode']['starting_player'] = (n // 2) % 2
        return cfg

    def compare(self, cfg, reason=None):
        names = [f'card{i}' for i in range(80)]
        training = rp.training_side([cfg], names)
        pool = {label: rp.deck_key(d['mainboard'], names)
                for label, d in (('A', deck('A/aaaa', overlap=False)), ('B', deck('B/bbbb', shift=20, overlap=False)))}
        result = rp.compare(training, self.MANIFEST, {'limits': self.LIMITS}, pool,
                            {'calls_set_runtime_rules': False}, allow_postboard_training=reason)
        return training, result

    def test_a_game_one_set_has_no_post_board_block(self):
        training, result = self.compare(self.config())
        self.assertNotIn('postboard', training)
        self.assertNotIn('postboard', result)
        self.assertNotIn('parity', result)
        self.assertTrue(rp.passes(result))
        _, flagged = self.compare(self.config(), 'not needed')
        self.assertEqual(flagged, result)

    def test_post_board_games_fail_ordinary_starts_unless_the_flag_records_them(self):
        cfg = self.config(ALWAYS)
        training, result = self.compare(cfg)
        self.assertEqual(training['postboard']['episodes'], 80)
        self.assertEqual(training['postboard']['conserving_episodes'], 80)
        self.assertFalse(result['checks']['ordinary_starts_game_one'])
        self.assertTrue(result['checks']['every_board_deck_equals_a_training_deck'])  # by the registered 60
        self.assertEqual(result['deck_matches'], {'A': ['A/aaaa'], 'B': ['B/bbbb']})
        self.assertIsNone(result['postboard']['exception'])
        self.assertTrue(result['postboard']['covered_by_the_flag'])
        self.assertFalse(rp.passes(result))
        _, flagged = self.compare(cfg, ' stage 1 exposure ')
        self.assertFalse(flagged['checks']['ordinary_starts_game_one'])  # the check itself stays honest
        self.assertEqual(flagged['postboard']['exception'],
                         {'flag': '--allow-postboard-training', 'reason': 'stage 1 exposure',
                          'excepted_checks': ['ordinary_starts_game_one']})
        self.assertTrue(rp.passes(flagged))
        self.assertEqual(flagged['parity'], 'holds only with a recorded exception: ordinary_starts_game_one '
                                            '(--allow-postboard-training: stage 1 exposure)')
        with self.assertRaises(rp.ParityError):
            self.compare(cfg, '  ')

    def test_the_flag_covers_only_conserving_post_board_games(self):
        def flagged(change):
            cfg = self.config(ALWAYS)
            change(entries(cfg))
            return self.compare(cfg, 'reason')[1]
        cases = {
            'a reset start': lambda rows: rows[0]['episode'].update(reset={'prefix': []}),
            'a changed 75': lambda rows: rows[0]['episode']['selected'][rows[0]['episode']['learner_seat']][
                'mainboard'].__setitem__(59, 77),
            'a changed label': lambda rows: rows[0]['episode']['selected'][0].update(label='C/cccc'),
            'a short sideboard': lambda rows: rows[0]['episode']['selected'][0]['sideboard'].pop(),
        }
        for name, change in cases.items():
            with self.subTest(name=name):
                result = flagged(change)
                self.assertIsNone(result['postboard']['exception'])
                self.assertFalse(result['postboard']['covered_by_the_flag'])
                self.assertFalse(rp.passes(result))
                self.assertNotIn('parity', result)
        # a game one whose selected 60 is not its registration is refused outright, as before
        cfg = self.config(ALWAYS)
        entries(cfg)[0]['episode']['postboard'] = False
        with self.assertRaisesRegex(rp.ParityError, 'two different mainboards'):
            self.compare(cfg, 'reason')


if __name__ == '__main__':
    unittest.main()
