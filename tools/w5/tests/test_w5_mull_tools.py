"""The London mulligan measurement tools (tools/w5/mull_probe.py, mull_calibrate.py, mull_start_replication.py,
mull_fixedhand.py, mull_paired.py), against a stand-in trainer (fake_collect_trainer.py) and a stand-in scorer
(fake_scorer.py) that follow the real trajectory, collection and scorer layouts. What only the real trainer can show
(the real trainer's handling of the vector, the pregame mode and a lean of 1e6 on a learned seat) is not exercised here."""
import contextlib
import io
import json
import math
import struct
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parent.parent / 'ex1'))
sys.path.insert(0, str(HERE))
import fake_collect_trainer as fake  # noqa: E402
import mull_calibrate as cal  # noqa: E402
import mull_fixedhand as fh  # noqa: E402
import mull_paired as pair  # noqa: E402
import mull_probe as mp  # noqa: E402
import mull_start_replication as sr  # noqa: E402
import qual  # noqa: E402
from test_w5_chains import dose  # noqa: E402

TRAINER = str(HERE / 'fake_collect_trainer.py')
SCORER = str(HERE / 'fake_scorer.py')
# the parent's saturated gaps by own count (the earlier lockstep probe's medians), used as the stand-in policy
GAPS = [13.1031, 36.4, 15.9, 46.1, 49.8, 12.7, 32.4]


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def reg(name):
    return {'label': f'{name}/main', 'cards': ['x']}


def london_base(iterations=10, **fake_knobs):
    """A London base of `iterations` updates of ten games: three self-play (learned, learned) and seven against a
    keep-seven fixed seat, nine decks in rotation, with the stand-in policy's knobs in `fake`."""
    decks = mp.DEFAULT_DECKS

    def episode(i, j):
        own, other = decks[(i + j) % 9], decks[(i + 2 * j + 1) % 9]
        seat = j % 2
        regs = [reg(own), reg(other)] if seat == 0 else [reg(other), reg(own)]
        selfplay = j < 3
        modes = ['learned', 'learned'] if selfplay else (['learned', 'keep_seven'] if seat == 0 else ['keep_seven', 'learned'])
        opp = {'kind': 'current'} if selfplay else {'kind': 'fixed', 'id': 'd6107922' if j % 2 else 'A48'}
        return {'episode': {'learner_seat': seat, 'registered': regs, 'selected': regs, 'starting_player': 0,
                            'postboard': False, 'max_physical_decisions': 100000, 'max_policy_steps': 200070,
                            'runtime_rules': 'resolution-boundary-v1', 'opening': 'london-v1', 'mulligan_modes': modes},
                'opponent': opp}
    knobs = dict({'gaps': GAPS, 'slope': 0.0, 'noise': 0.0}, **fake_knobs)
    return {'schema': 'test-base', 'opponents': [], 'learning_rate': 1e-4, 'value_coefficient': 0.5,
            'loss_selection': {'kind': 'gae_advantage_value_v1', 'gamma': 1.0, 'lambda': 0.9, 'entropy_coefficient': 0.0},
            'update_backend': {'kind': 'cpu'}, 'fake': knobs,
            'iterations': [{'episodes': [episode(i, j) for j in range(10)]} for i in range(iterations)]}


class Fixture(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.t = Path(self.tmp.name)
        self.dose_path = dose(self.t, 'p')

    def base(self, **kw):
        path = self.t / 'base.json'
        path.write_text(json.dumps(london_base(**kw)), encoding='utf-8')
        return path

    def quiet(self, fn, *a, **k):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = fn(*a, **k)
        return code, out.getvalue(), err.getvalue()


class Statistics(unittest.TestCase):
    def test_quantile_is_the_linear_one(self):
        xs = [1.0, 2.0, 4.0, 8.0]
        self.assertEqual(mp.quantile(xs, 0.5), 3.0)
        self.assertEqual(mp.quantile(xs, 0.0), 1.0)
        self.assertEqual(mp.quantile(xs, 1.0), 8.0)
        self.assertAlmostEqual(mp.quantile(xs, 0.1), 1.3)
        with self.assertRaises(ValueError):
            mp.quantile([], 0.5)

    def test_weighted_median(self):
        self.assertEqual(mp.weighted_median([3, 1, 2], [1, 1, 1]), 2)
        self.assertEqual(mp.weighted_median([1, 2, 3, 4], [1, 1, 1, 1]), 2.5)   # exactly half: the midpoint
        self.assertEqual(mp.weighted_median([1, 2, 3, 4], [1, 1, 1, 1.0000001]), 3)
        self.assertEqual(mp.weighted_median([0, 10], [0.7, 0.3]), 0)
        self.assertEqual(mp.weighted_median([0, 10], [0.3, 0.7]), 10)
        self.assertEqual(mp.weighted_median([5.0], [0.2]), 5.0)
        with self.assertRaises(ValueError):
            mp.weighted_median([], [])
        with self.assertRaises(ValueError):
            mp.weighted_median([1], [0])

    def test_entropy_and_sd(self):
        self.assertAlmostEqual(mp.entropy_nats([0.0, 0.0, 0.0, 0.0]), math.log(4))
        self.assertAlmostEqual(mp.entropy_nats([50.0, 0.0]), 0.0, places=6)
        self.assertIsNone(mp.sd([1.0]))
        self.assertAlmostEqual(mp.sd([1.0, 3.0]), math.sqrt(2))


class Rows(unittest.TestCase):
    def test_an_announcement_is_read_from_its_own_count_column_only(self):
        row = fake.announcement(0, 2, 5, 7.5, False, 0.25)   # own count 2, the other seat's 5
        a = mp.announcement(row)
        self.assertEqual((a['actor'], a['k'], a['gap'], a['value']), (0, 2, 7.5, 0.25))
        self.assertEqual((a['keep_row'], a['mull_row']), (0, 1))
        swapped = mp.announcement(fake.announcement(1, 6, 0, -3.0, True, 0.0))
        self.assertEqual((swapped['mull_row'], swapped['keep_row'], swapped['k'], swapped['gap']), (0, 1, 6, -3.0))
        bad = fake.announcement(0, 2, 2, 1.0, False, 0.0)
        bad['tensor']['action_features'][63] = fake.bits(0.3)
        with self.assertRaises(ValueError):
            mp.announcement(bad)
        bad = fake.announcement(0, 2, 2, 1.0, False, 0.0)
        bad['tensor']['action_features'][70 + 63] = fake.bits(3 / 16)
        with self.assertRaises(ValueError):
            mp.announcement(bad)

    def test_other_rows_are_not_announcements(self):
        self.assertIsNone(mp.announcement(fake.row(0, False, [0] * 140, [1.0, 2.0], 0.1)))
        bottom = fake.row(0, True, [fake.bits(1.0)] * (4 * 70), [0.1, 0.2, 0.3, 0.4], 0.0)
        self.assertIsNone(mp.announcement(bottom))
        traj = {'decisions': [fake.announcement(0, 0, 0, 1.0, False, 0.5), bottom,
                              fake.row(0, False, [0] * 140, [1.0, 2.0], 0.1)]}
        anns, bottoms = mp.pregame_decisions(traj)
        self.assertEqual((len(anns), len(bottoms), bottoms[0]['n']), (1, 1, 4))


class Collect(Fixture):
    def test_chunks_cover_every_episode_in_order_and_the_runs_are_removed(self):
        base = json.loads(self.base().read_text())
        template, entries = mp.base_entries(base, json.loads(self.dose_path.read_text()), 'r1', 3)
        self.assertEqual(len(entries), 30)
        seen = []
        work = self.t / 'work'
        work.mkdir()
        for entry, traj, path in mp.collect(TRAINER, template, entries, work, chunk_size=8):
            self.assertEqual(traj['episode']['id'], entry['episode']['id'])
            self.assertTrue(path.exists())
            seen.append(entry['episode']['id'])
        self.assertEqual(seen, [e['episode']['id'] for e in entries])
        self.assertEqual([p.name for p in (work / 'run').iterdir() if p.is_dir()], [])   # run directories deleted
        self.assertEqual(len(list((work / 'run').glob('*.config.json'))), 4)
        cfg = json.loads(next((work / 'run').glob('*.config.json')).read_text())
        self.assertEqual(len(cfg['iterations']), 1)
        self.assertEqual(cfg['max_non_natural_episode_fraction'], 0.5)

    def test_a_failing_trainer_is_an_error_not_an_empty_result(self):
        base = json.loads(self.base(fail=True).read_text())
        template, entries = mp.base_entries(base, json.loads(self.dose_path.read_text()), 'r1', 1)
        with self.assertRaisesRegex(RuntimeError, 'trainer failed'):
            list(mp.collect(TRAINER, template, entries, self.t, chunk_size=5))

    def test_a_trajectory_that_differs_from_its_pin_is_refused(self):
        base = json.loads(self.base().read_text())
        template, entries = mp.base_entries(base, json.loads(self.dose_path.read_text()), 'r1', 1)
        gen = mp.collect(TRAINER, template, entries, self.t, chunk_size=10)
        entry, traj, path = next(gen)
        gen.close()
        # a second run whose first trajectory is tampered with after the trainer wrote it
        import mull_probe
        orig = mull_probe.sha256_file
        mull_probe.sha256_file = lambda p: 'bad'
        try:
            with self.assertRaisesRegex(RuntimeError, 'differs from its pin'):
                list(mp.collect(TRAINER, template, entries, self.t / 'again', chunk_size=10))
        finally:
            mull_probe.sha256_file = orig

    def test_a_fixed_update_stamps_every_episode_with_that_updates_lean(self):
        base = json.loads(self.base().read_text())
        ko = {'b0': [7.0] * 7, 'end_update': 100, 'shape': 'linear-v1'}
        d = json.loads(self.dose_path.read_text())
        _, scheduled = mp.base_entries(base, d, 'r1', 4, keep_offset=ko)
        _, fixed = mp.base_entries(base, d, 'r1', 4, keep_offset=ko, fixed_update=0)
        self.assertNotEqual([e['episode']['london_keep_offset'] for e in scheduled],
                            [e['episode']['london_keep_offset'] for e in fixed])
        self.assertTrue(all(e['episode']['london_keep_offset'] == [7.0] * 7 for e in fixed))
        self.assertEqual([e['episode']['id'] for e in scheduled], [e['episode']['id'] for e in fixed])


class Calibrate(Fixture):
    def run_cal(self, updates=10, extra=()):
        out = self.t / 'cal'
        argv = ['run', '--trainer', TRAINER, '--base', str(self.base()), '--dose', str(self.dose_path), '--out-dir',
                str(out), '--updates', str(updates), '--min-announcements', '50', *extra]
        return out, self.quiet(cal.main, argv)

    def test_the_vector_is_median_plus_ln9_at_every_count(self):
        out, (code, text, err) = self.run_cal()
        self.assertEqual(code, 0, err)
        doc = json.loads((out / 'calibration.json').read_text())
        self.assertEqual(doc['vector'], [round(g + math.log(9), 4) for g in GAPS])
        self.assertEqual(doc['vector'][0], 15.3003)
        for row in doc['per_count']:
            self.assertEqual(row['n'], 130)             # thirteen learned seats per update, all walk to zero cards
            self.assertEqual((row['n_keep_seven'], row['n_self_play']), (70, 60))
        self.assertIn('vector b = [15.3003', text)
        # the start predicted from the walks: each count mulligans with probability 0.1 on these hands
        ps = doc['predicted_start']
        self.assertEqual((ps['complete_seats'], ps['incomplete_seats']), (130, 0))
        self.assertAlmostEqual(ps['kept_size_share']['7'], 0.9, places=3)
        self.assertAlmostEqual(ps['kept_size_share']['6'], 0.09, places=3)
        self.assertAlmostEqual(ps['share_mulligan_the_seven'], 0.1, places=3)
        self.assertLess(ps['zero_card_share_of_mulliganers'], 1e-4)
        self.assertAlmostEqual(sum(ps['kept_size_share'].values()), 1.0)
        self.assertAlmostEqual(ps['predicted_decisions_per_update'][0], 13.0)
        self.assertAlmostEqual(ps['predicted_decisions_per_update'][1], 1.3, places=2)
        # every announcement is kept with its tensor, the field was absent in the walk
        rows = [json.loads(line) for line in (out / 'announcements.jsonl').read_text().splitlines()]
        self.assertEqual(len(rows), 130 * 7)
        self.assertTrue(all('tensor' in r for r in rows))
        cfg = json.loads(next((out / 'run').glob('*.config.json')).read_text())
        self.assertFalse(any('london_keep_offset' in e['episode'] for e in cfg['iterations'][0]['episodes']))
        self.assertNotIn('pregame_advantage', cfg['loss_selection'])

    def test_groups_are_weighted_seventy_thirty_not_by_announcement_count(self):
        recs = []
        for k in range(7):
            recs += [{'game': f'a{n}', 'actor': 0, 'k': k, 'gap': 10.0, 'group': 'keep_seven'} for n in range(10)]
            recs += [{'game': f's{n}', 'actor': 0, 'k': k, 'gap': 20.0, 'group': 'self_play'} for n in range(190)]
        rows = cal.per_count(recs, 100)
        self.assertEqual(rows[0]['median_weighted'], 10.0)          # 70% of the weight sits on the keep-seven value
        self.assertEqual(rows[0]['median_pooled'], 20.0)             # an unweighted median would say the other
        self.assertEqual(rows[0]['b'], round(10.0 + math.log(9), 4))

    def test_a_negative_median_is_floored_at_zero_and_an_all_zero_vector_is_refused(self):
        self.assertEqual(cal.b_value(-5.0), 0.0)
        self.assertEqual(cal.b_value(-2.1972), 0.0)
        self.assertEqual(cal.b_value(0.0), 2.1972)
        recs = []
        for k in range(7):
            recs += [{'game': f'a{n}', 'actor': 0, 'k': k, 'gap': -9.0, 'group': 'keep_seven'} for n in range(70)]
            recs += [{'game': f's{n}', 'actor': 0, 'k': k, 'gap': -9.0, 'group': 'self_play'} for n in range(30)]
        with self.assertRaisesRegex(ValueError, 'every entry'):
            cal.analyze(recs)
        recs[0]['gap'] = 99.0
        mixed = [dict(r, gap=(30.0 if r['k'] == 3 else -9.0)) for r in recs]
        self.assertEqual(cal.analyze(mixed)['vector'], [0, 0, 0, round(30 + math.log(9), 4), 0, 0, 0])

    def test_an_unreached_count_stops_the_tool(self):
        recs = []
        for k in range(7):
            n = 120 if k < 5 else 60
            recs += [{'game': f'a{k}{i}', 'actor': 0, 'k': k, 'gap': 8.0, 'group': 'keep_seven'} for i in range(n // 2)]
            recs += [{'game': f's{k}{i}', 'actor': 0, 'k': k, 'gap': 8.0, 'group': 'self_play'} for i in range(n // 2)]
        with self.assertRaisesRegex(cal.Unreached, r'UNREACHED 5: under 100 \(60 announcements\)'):
            cal.analyze(recs)
        only_keep = [r for r in recs if r['group'] == 'keep_seven' or r['k'] >= 5]
        with self.assertRaisesRegex(cal.Unreached, 'UNREACHED 0: no announcements in group self_play'):
            cal.per_count([r for r in only_keep if not (r['k'] == 0 and r['group'] == 'self_play')], 10)
        path = self.t / 'ann.jsonl'
        path.write_text('\n'.join(json.dumps(r) for r in recs) + '\n')
        code, out, err = self.quiet(cal.main, ['analyze', '--announcements', str(path), '--out', str(self.t / 'o.json')])
        self.assertEqual(code, 3)
        self.assertTrue(err.startswith('UNREACHED 5'))
        self.assertFalse((self.t / 'o.json').exists())

    def test_the_lockstep_numbers_are_printed_beside_and_choose_nothing(self):
        lock = self.t / 'lock.jsonl'
        lock.write_text('\n'.join(json.dumps({'hand_size': 7 - k, 'gap': g - 1.0}) for k, g in enumerate(GAPS)
                                  for _ in range(3)) + '\n')
        out, (code, text, err) = self.run_cal(extra=('--lockstep-scores', str(lock)))
        self.assertEqual(code, 0, err)
        doc = json.loads((out / 'calibration.json').read_text())
        self.assertEqual(doc['vector'], [round(g + math.log(9), 4) for g in GAPS])      # unchanged by the lockstep file
        self.assertEqual([r['median'] for r in doc['lockstep']], [g - 1.0 for g in GAPS])
        self.assertTrue(all(abs(s - 1.0) < 1e-4 for s in doc['shift_in_context_minus_lockstep']))
        self.assertIn('lockstep median', text)


class StartReplication(Fixture):
    VECTOR = [round(g + math.log(9), 4) for g in GAPS]

    def run_sr(self, pregame='terminal_v1', keep=True, updates=10):
        out = self.t / 'sr'
        argv = ['run', '--trainer', TRAINER, '--base', str(self.base()), '--dose', str(self.dose_path), '--out-dir', str(out),
                '--b0', ','.join(str(x) for x in self.VECTOR), '--updates', str(updates), '--pregame-advantage', pregame]
        if keep:
            argv.append('--keep-trajectories')
        return out, self.quiet(sr.main, argv)

    def test_run_stamps_the_full_vector_and_the_mode_on_every_game_and_keeps_the_trajectories(self):
        out, (code, text, err) = self.run_sr()
        self.assertEqual(code, 0, err)
        cfgs = sorted((out / 'run').glob('*.config.json'))
        want = [f32(b) for b in self.VECTOR]
        for path in cfgs:
            cfg = json.loads(path.read_text())
            self.assertEqual(cfg['loss_selection']['pregame_advantage'], 'terminal_v1')
            self.assertTrue(all(e['episode']['london_keep_offset'] == want for e in cfg['iterations'][0]['episodes']))
        rep = json.loads((out / 'report.json').read_text())
        self.assertEqual(rep['keep_offset']['b0'], self.VECTOR)
        self.assertEqual(rep['pregame_advantage'], 'terminal_v1')
        self.assertEqual(rep['launch_gate_numbers']['learned_seats'], 130)
        self.assertEqual(sum(rep['launch_gate_numbers']['kept_size_count']), 130)
        pins = json.loads((out / 'trajectories.json').read_text())
        self.assertEqual(len(pins), 100)
        for pin in list(pins.values())[:5]:
            self.assertEqual(mp.sha256_file(out / 'trajectories' / pin['file']), pin['sha256'])
        self.assertIn('launch gate', text)
        self.assertEqual(len((out / 'seats.jsonl').read_text().splitlines()), 130)

    def test_no_mode_and_no_kept_trajectories(self):
        out, (code, _, err) = self.run_sr(pregame='none', keep=False)
        self.assertEqual(code, 0, err)
        cfg = json.loads(next((out / 'run').glob('*.config.json')).read_text())
        self.assertNotIn('pregame_advantage', cfg['loss_selection'])
        self.assertFalse((out / 'trajectories').exists())
        self.assertIsNone(json.loads((out / 'report.json').read_text())['pregame_advantage'])

    def seats(self, spec):
        """spec: list of (kept, taken, reward, deck, opponent, seat)."""
        return [{'game': f'g{i}', 'i': i, 'seat': s, 'deck': d, 'opp_deck': 'X', 'opponent': o, 'group': 'keep_seven',
                 'kept': kept, 'taken': taken, 'reward': r, 'natural': True, 'anns': [], 'bottoms': []}
                for i, (kept, taken, r, d, o, s) in enumerate(spec)]

    def test_launch_gate_numbers_and_verdicts(self):
        spec = [(7, 0, 1, 'A', 'o', 0)] * 16 + [(6, 1, 1, 'A', 'o', 0)] * 2 + [(5, 2, -1, 'A', 'o', 0), (0, 7, -1, 'A', 'o', 0)]
        nums = sr.gate_numbers(self.seats(spec))
        self.assertEqual((nums['learned_seats'], nums['mulligan_the_seven']), (20, 4))
        self.assertEqual(nums['share_mulligan_the_seven'], 0.2)
        self.assertEqual(nums['zero_card_share_of_mulliganers'], 0.25)
        self.assertEqual(nums['five_or_six_share_of_mulliganers'], 0.75)
        self.assertEqual(nums['kept_size_count'], [1, 0, 0, 0, 0, 1, 2, 16])
        gate = sr.evaluate_gate(nums, sr.GATE_DEFAULTS)
        self.assertFalse(gate['pass'])                    # the zero-card share is 25%, above 10%
        self.assertEqual([c['pass'] for c in gate['checks']], [True, False, True])
        relaxed = sr.evaluate_gate(nums, dict(sr.GATE_DEFAULTS, zero_max=0.25))
        self.assertTrue(relaxed['pass'])
        low = sr.evaluate_gate(dict(nums, share_mulligan_the_seven=0.149), sr.GATE_DEFAULTS)
        self.assertFalse(low['checks'][0]['pass'])
        high = sr.evaluate_gate(dict(nums, share_mulligan_the_seven=0.351), sr.GATE_DEFAULTS)
        self.assertFalse(high['checks'][0]['pass'])
        empty = sr.gate_numbers([])
        self.assertIsNone(empty['share_mulligan_the_seven'])
        self.assertFalse(sr.evaluate_gate(empty, sr.GATE_DEFAULTS)['pass'])

    def test_value_price_is_matched_by_stratum_and_weighted_by_six_card_keeps(self):
        spec = ([(6, 1, 1, 'A', 'o1', 0)] * 3 + [(6, 1, -1, 'A', 'o1', 0)] +      # stratum 1: six-keeps mean 0.5
                [(7, 0, 1, 'A', 'o1', 0)] * 2 + [(7, 0, -1, 'A', 'o1', 0)] * 2 +   # sevens mean 0.0
                [(6, 1, -1, 'B', 'o1', 1)] * 2 +                                   # stratum 2: six-keeps mean -1
                [(7, 0, 1, 'B', 'o1', 1)] * 3 + [(7, 0, -1, 'B', 'o1', 1)] +      # sevens mean 0.5
                [(6, 1, 1, 'C', 'o2', 0)])                                         # no seven-keep in its stratum
        vp = sr.value_price(self.seats(spec))['realized_return']
        self.assertEqual((vp['six_card_keeps_matched'], vp['strata_used'], vp['six_card_keeps_without_a_seven_keep']),
                         (6, 2, 1))
        self.assertAlmostEqual(vp['difference_six_minus_seven'], (4 * 0.5 + 2 * -1.5) / 6)
        # standard error: stratum 1 six-keep var 1.0 (3 of 1, 1 of -1: sample var 1.0), seven var 4/3; stratum 2 var 0 and 1
        v1 = 16 * (1.0 / 4 + (4 / 3) / 4)
        v2 = 4 * (0.0 / 2 + 1.0 / 4)
        self.assertAlmostEqual(vp['standard_error'], math.sqrt(v1 + v2) / 6)
        self.assertIsNone(sr.value_price(self.seats([(7, 0, 1, 'A', 'o', 0)]))['realized_return'])

    def test_the_heads_price_is_read_on_the_same_seats(self):
        seats = self.seats([(6, 1, 1, 'A', 'o', 0), (6, 1, 1, 'A', 'o', 0), (5, 2, -1, 'A', 'o', 0), (7, 0, 1, 'A', 'o', 0)])
        seats[0]['anns'] = [[0, 0.6, 5.0], [1, 0.3, -2.0]]
        seats[1]['anns'] = [[0, 0.5, 5.0], [1, 0.2, -2.0]]
        seats[2]['anns'] = [[0, 0.6, 5.0], [1, 0.4, 5.0], [2, 0.1, -1.0]]
        seats[3]['anns'] = [[0, 0.9, -5.0]]
        head = sr.value_price(seats)['value_head']
        self.assertEqual((head['seats_with_a_mulligan'], head['seats_keeping_six']), (3, 2))
        self.assertAlmostEqual(head['mean_v_six_minus_v_seven_six_keepers'], -0.3)
        self.assertAlmostEqual(head['mean_v_six_minus_v_seven_all_mulliganers'], (-0.3 - 0.3 - 0.2) / 3)

    def test_bottom_entropy(self):
        seats = self.seats([(6, 1, 1, 'A', 'o', 0)])
        uniform = [fake.bits(0.0)] * 5
        sharp = [fake.bits(40.0), fake.bits(0.0), fake.bits(0.0)]
        seats[0]['bottoms'] = [[5, uniform], [3, sharp]]
        b = sr.bottom_entropy(seats)
        self.assertEqual(b['decisions'], 2)
        self.assertAlmostEqual(b['by_choices']['5']['mean_share_of_ln_n'], 1.0)
        self.assertAlmostEqual(b['by_choices']['3']['mean_entropy_nats'], 0.0, places=6)
        self.assertAlmostEqual(b['mean_share_of_ln_n'], 0.5, places=5)
        self.assertEqual(sr.bottom_entropy(self.seats([(7, 0, 1, 'A', 'o', 0)]))['decisions'], 0)

    def test_by_count_shares(self):
        seats = self.seats([(6, 1, 1, 'A', 'o', 0), (7, 0, -1, 'A', 'o', 0), (5, 2, 1, 'A', 'o', 0)])
        seats[0]['anns'] = [[0, 0.5, 4.0], [1, 0.2, -4.0]]
        seats[1]['anns'] = [[0, 0.7, -4.0]]
        seats[2]['anns'] = [[0, 0.5, 4.0], [1, 0.1, 4.0], [2, 0.0, -4.0]]
        rows = sr.by_count(seats, updates=3)
        self.assertEqual([(r['decisions'], r['mulligans']) for r in rows[:4]], [(3, 2), (2, 1), (1, 0), (0, 0)])
        self.assertAlmostEqual(rows[0]['mulligan_again_share'], 2 / 3)
        self.assertAlmostEqual(rows[0]['decisions_per_update'], 1.0)
        self.assertAlmostEqual(rows[0]['mean_value'], (0.5 + 0.7 + 0.5) / 3)
        self.assertAlmostEqual(rows[0]['mean_realized_return'], 1 / 3)
        self.assertIsNone(rows[3]['mulligan_again_share'])

    def test_analyze_from_the_seats_file(self):
        out, (code, _, _) = self.run_sr(keep=False)
        again = self.t / 'again.json'
        code, text, err = self.quiet(sr.main, ['analyze', '--seats', str(out / 'seats.jsonl'), '--out', str(again),
                                               '--updates', '10'])
        self.assertEqual(code, 0, err)
        a, b = json.loads(again.read_text()), json.loads((out / 'report.json').read_text())
        for key in ('launch_gate_numbers', 'launch_gate', 'value_price', 'bottom_decisions', 'by_count'):
            self.assertEqual(a[key], b[key])

    def test_on_the_stand_in_the_start_is_what_the_vector_predicts(self):
        out, (code, _, _) = self.run_sr(keep=False, updates=30)
        rep = json.loads((out / 'report.json').read_text())
        n = rep['launch_gate_numbers']
        self.assertEqual(n['learned_seats'], 390)
        self.assertAlmostEqual(n['share_mulligan_the_seven'], 0.1, delta=0.04)
        self.assertGreater(rep['bottom_decisions']['decisions'], 0)
        six = rep['value_price']['value_head']
        self.assertAlmostEqual(six['mean_v_six_minus_v_seven_six_keepers'], -0.33, places=4)   # the stand-in's price


class FixedHand(Fixture):
    def announcements(self):
        out = self.t / 'cal'
        code, _, err = self.quiet(cal.main, ['run', '--trainer', TRAINER, '--base', str(self.base()), '--dose',
                                             str(self.dose_path), '--out-dir', str(out), '--updates', '5',
                                             '--min-announcements', '20'])
        self.assertEqual(code, 0, err)
        return out / 'announcements.jsonl'

    def models(self, **gaps):
        paths = {}
        for name, g in gaps.items():
            p = self.t / f'{name}.json'
            p.write_text(json.dumps({'gap': g}))
            paths[name] = p
        return paths

    def args(self, ann, models, **kw):
        import argparse
        imp = self.t / 'import.json'
        imp.write_text('{}')
        return argparse.Namespace(announcements=str(ann), scorer=[sys.executable, SCORER], play_import=str(imp),
                                  model=[f'{n}={p}' for n, p in models.items()], out=str(self.t / 'table.json'),
                                  limit_per_count=kw.get('limit'), workdir=str(self.t / 'wd'))

    def test_the_table_by_count_of_two_states(self):
        ann = self.announcements()
        models = self.models(low=-10.0, high=5.0)
        code, text, err = self.quiet(fh.run, self.args(ann, models))
        self.assertEqual(code, 0, err)
        doc = json.loads((self.t / 'table.json').read_text())
        low, high = doc['models']['low']['by_count'], doc['models']['high']['by_count']
        for k in range(7):
            self.assertEqual(low[k]['n'], 65)
            self.assertAlmostEqual(low[k]['median'], -10.0 + k)           # the stand-in scorer: gap = X + k
            self.assertAlmostEqual(low[k]['spread'], 0.0)
            self.assertAlmostEqual(low[k]['mean_value'], 0.1 * k, places=6)
            self.assertAlmostEqual(high[k]['median'], 5.0 + k)
        self.assertEqual([r['share_lt_0'] for r in low], [1.0] * 7)                       # -10 + k < 0 for k <= 6
        self.assertEqual([r['share_gt_6_9'] for r in high], [0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0])  # 5+2 = 7 > 6.9
        self.assertEqual(doc['announcements']['requests_scored'], 455)
        self.assertEqual(doc['models']['low']['model_state_sha256'], 'd' * 64)
        self.assertIn('spread', text)

    def test_a_limit_takes_the_first_of_each_count_and_a_bad_answer_is_an_error(self):
        ann = self.announcements()
        rows = [json.loads(line) for line in ann.read_text().splitlines()]
        picked = fh.select_requests(rows, 4)
        self.assertEqual([sum(1 for r in picked if r['k'] == k) for k in range(7)], [4] * 7)
        self.assertEqual(picked[0], rows[0])
        self.assertEqual(len(fh.select_requests(rows, None)), len(rows))
        code, _, _ = self.quiet(fh.run, self.args(ann, self.models(a=1.0), limit=3))
        self.assertEqual(code, 0)
        self.assertEqual(json.loads((self.t / 'table.json').read_text())['announcements']['requests_scored'], 21)
        no_scorer = self.args(ann, self.models(a=1.0))
        no_scorer.scorer = [sys.executable, '-c', 'print("not json")']
        with self.assertRaises((RuntimeError, ValueError)):
            self.quiet(fh.run, no_scorer)

    def test_recorded_logits_are_compared_bit_for_bit(self):
        ann = self.announcements()
        code, _, _ = self.quiet(fh.run, self.args(ann, self.models(a=1.0)))
        doc = json.loads((self.t / 'table.json').read_text())
        self.assertEqual(doc['models']['a']['logits_equal_recorded'], 0)     # the stand-in scorer is not the trainer

    def test_announcements_without_tensors_are_refused(self):
        out = self.t / 'cal2'
        self.quiet(cal.main, ['run', '--trainer', TRAINER, '--base', str(self.base()), '--dose', str(self.dose_path),
                              '--out-dir', str(out), '--updates', '5', '--min-announcements', '20', '--no-tensors'])
        with self.assertRaises(SystemExit):
            self.quiet(fh.run, self.args(out / 'announcements.jsonl', self.models(a=1.0)))


class Paired(Fixture):
    def run_pair(self, force='offset', deals=162, **fake_knobs):
        knobs = dict({'gaps': [0.0, -20.0, -20.0, -20.0, -20.0, -20.0, -20.0], 'slope': 8.0, 'noise': 0.0}, **fake_knobs)
        out = self.t / f'pair-{force}'
        argv = ['run', '--trainer', TRAINER, '--base', str(self.base(**knobs)), '--dose', str(self.dose_path), '--out-dir',
                str(out), '--deals', str(deals), '--force', force]
        return out, self.quiet(pair.main, argv)

    def test_pairs_are_the_deals_the_state_mulligans_played_twice_on_one_seed(self):
        out, (code, text, err) = self.run_pair()
        self.assertEqual(code, 0, err)
        rep = json.loads((out / 'report.json').read_text())
        rows = [json.loads(line) for line in (out / 'pairs.jsonl').read_text().splitlines()]
        screened = json.loads((out / 'screen.json').read_text())
        self.assertEqual(len(screened), 162)
        self.assertEqual(rep['affected_deals'], len(rows))
        self.assertEqual(len(rows), sum(1 for s in screened if s['taken'] >= 1))
        self.assertGreater(len(rows), 20)
        self.assertLess(len(rows), 162)
        self.assertEqual(rep['own_replay_differs_from_screen'], 0)        # the own line is reproduced exactly
        self.assertEqual(rep['forced_game_did_not_keep_seven'], 0)
        for r in rows:
            self.assertGreaterEqual(r['screen']['taken'], 1)
            self.assertEqual(r['keep']['taken'], 0)
            self.assertEqual(r['keep']['kept'], 7)
        diffs = [r['own']['reward'] - r['keep']['reward'] for r in rows]
        iv = rep['paired_difference_own_minus_forced_keep']
        self.assertEqual(iv['n'], len(diffs))
        self.assertAlmostEqual(iv['mean'], sum(diffs) / len(diffs))
        self.assertAlmostEqual(iv['se'], mp.sd(diffs) / math.sqrt(len(diffs)))
        self.assertAlmostEqual(iv['lower95'], iv['mean'] - 1.96 * iv['se'])
        self.assertGreater(iv['mean'], 0)          # on the stand-in, mulliganing bad hands into fresh ones helps
        self.assertIn('lower > 0', text)

    def test_the_default_forces_the_keep_with_a_lean_and_the_learner_stays_learned(self):
        b, (code_b, _, err_b) = self.run_pair()
        self.assertEqual(code_b, 0, err_b)
        cfg = json.loads(sorted((b / 'keep' / 'run').glob('*.config.json'))[0].read_text())
        eps = [e['episode'] for e in cfg['iterations'][0]['episodes']]
        self.assertTrue(all(e['london_keep_offset'] == 1.0e6 for e in eps))
        self.assertTrue(all(e['mulligan_modes'][e['learner_seat']] == 'learned' for e in eps))
        for name in ('screen', 'own'):          # the own line carries no lean at all
            cfg = json.loads(sorted((b / name / 'run').glob('*.config.json'))[0].read_text())
            self.assertFalse(any('london_keep_offset' in e['episode'] for e in cfg['iterations'][0]['episodes']))

    def test_a_keep_seven_learner_seat_is_what_the_trainer_refuses(self):
        # the trainer's opening validation refuses it, so the tool must not default to it (the stand-in refuses too)
        with self.assertRaises(RuntimeError):
            self.run_pair('keep_seven')

    def test_same_seed_ids_differ_and_decks_seats_and_starts_are_shared(self):
        out, _ = self.run_pair()
        own = json.loads(sorted((out / 'own' / 'run').glob('*.config.json'))[0].read_text())['iterations'][0]['episodes']
        keep = json.loads(sorted((out / 'keep' / 'run').glob('*.config.json'))[0].read_text())['iterations'][0]['episodes']
        for o, k in zip(own, keep):
            self.assertEqual(o['episode']['seed'], k['episode']['seed'])
            self.assertEqual({x: o['episode'][x] for x in ('registered', 'learner_seat', 'starting_player')},
                             {x: k['episode'][x] for x in ('registered', 'learner_seat', 'starting_player')})
            self.assertTrue(o['episode']['id'].endswith('-own') and k['episode']['id'].endswith('-keep'))
        seeds = {s['seed'] for s in json.loads((out / 'screen.json').read_text())}
        self.assertTrue({o['episode']['seed'] for o in own} <= seeds)

    def test_interval_arithmetic(self):
        def pair_row(own, keep, nat=True, kept=6):
            return {'screen': {'kept': kept, 'reward': own}, 'own': {'reward': own, 'kept': kept, 'natural': nat, 'taken': 1},
                    'keep': {'reward': keep, 'kept': 7, 'natural': True, 'taken': 0}}
        rows = [pair_row(1, -1), pair_row(1, 1), pair_row(-1, 1), pair_row(1, -1), pair_row(1, 1, nat=False, kept=5)]
        rep = pair.analyze(rows, deals_screened=50)
        iv = rep['paired_difference_own_minus_forced_keep']
        diffs = [2, 0, -2, 2, 0]
        self.assertEqual((iv['n'], iv['mean']), (5, 0.4))
        self.assertAlmostEqual(iv['se'], mp.sd(diffs) / math.sqrt(5))
        self.assertFalse(iv['lower_bound_above_0'])
        self.assertEqual(rep['natural_pairs_only']['n'], 4)
        self.assertEqual(rep['affected_share'], 0.1)
        self.assertEqual(sorted(rep['by_own_kept_size']), ['5', '6'])
        sure = pair.analyze([pair_row(1, -1)] * 30 + [pair_row(1, 1)] * 10, 100)['paired_difference_own_minus_forced_keep']
        self.assertTrue(sure['lower_bound_above_0'])
        self.assertEqual(pair.analyze([], 10)['paired_difference_own_minus_forced_keep'], {'n': 0})
        one = pair.analyze([pair_row(1, -1)], 10)['paired_difference_own_minus_forced_keep']
        self.assertIsNone(one['se'])
        # a pair with a missing reward is counted, not silently dropped into the mean
        missing = pair_row(None, 1)
        self.assertEqual(pair.analyze([pair_row(1, -1), pair_row(1, -1), missing], 9)['pairs_without_a_reward'], 1)

    def test_pairs_can_be_reanalyzed_from_the_file(self):
        out, _ = self.run_pair()
        code, text, err = self.quiet(pair.main, ['analyze', '--pairs', str(out / 'pairs.jsonl'), '--deals-screened', '162',
                                                 '--out', str(self.t / 're.json')])
        self.assertEqual(code, 0, err)
        again = json.loads((self.t / 're.json').read_text())
        self.assertEqual(again['paired_difference_own_minus_forced_keep'],
                         json.loads((out / 'report.json').read_text())['paired_difference_own_minus_forced_keep'])


if __name__ == '__main__':
    unittest.main()
