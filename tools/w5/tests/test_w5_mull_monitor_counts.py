"""The per-count readings of the London mulligan monitor (tools/w5/mull_monitor.py, line version 2): announcements
by the acting seat's own count, the mulligans among them, terminal return by kept hand size, the value head's pregame
estimate against the realized result by count, a per-count lean vector against its schedule, and v1 lines still read.
Synthetic trajectories shaped like the trainer's (float32 values as bit patterns, state column 89 for the pregame,
action columns 15, 63, 66 and 69)."""
import contextlib
import io
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
import mull_monitor as mm  # noqa: E402
import qual  # noqa: E402
from test_w5_chains import jwrite  # noqa: E402
from test_w5_mull_monitor import make_iteration, seat, summary, trajectory, write  # noqa: E402

VECTOR = {'b0': [6.0, 12.0, 0.0, 24.0, 3.0, 0.0, 9.0], 'end_update': 8, 'shape': 'linear-v1'}


def bits(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def announce(actor, own, other, value, width=70):
    """One announcement row: two action rows (keep, mulligan), own count in column 63, the other seat's in 66."""
    action = [0] * (2 * width)
    for r, mulligan in ((0, 0.0), (1, 1.0)):
        action[r * width + 15] = bits(1.0)
        action[r * width + 69] = bits(mulligan)
        action[r * width + 63] = bits(own / 16)
        action[r * width + 66] = bits(other / 16)
    state = [0] * 100
    state[89] = bits(1.0)
    return {'actor': actor, 'logits': [bits(0.5), bits(-0.5)], 'value': bits(value), 'step': 0,
            'tensor': {'state': state, 'action_features': action}}


def bottom(actor, cards=3):
    """A put-back decision: pregame state, one action row per card (not an announcement)."""
    state = [0] * 100
    state[89] = bits(1.0)
    return {'actor': actor, 'logits': [bits(0.0)] * cards, 'value': bits(0.0),
            'tensor': {'state': state, 'action_features': [bits(1.0)] * (cards * 70)}}


def game_row(actor):
    state = [0] * 100
    return {'actor': actor, 'logits': [bits(1.0), bits(2.0)], 'value': bits(0.25),
            'tensor': {'state': state, 'action_features': [0] * 140}}


def with_rows(seats, rows, reward=(1.0, -1.0), offset=None, decks=('Wildfire', 'Rally')):
    """A trajectory file's bytes whose decision rows are `rows` (then a few in-game rows)."""
    doc = json.loads(trajectory(seats, decks=decks, offset=offset))
    doc['decisions'] = rows + [game_row(0), game_row(1)]
    doc['terminal'] = {'terminal_reward': list(reward), 'terminal_classification': 'natural'}
    return json.dumps(doc, separators=(',', ':')).encode()


# seat 0 takes two mulligans (announces at 0, 1, 2, keeps five); seat 1 keeps seven (announces at 0). The other seat's
# count in column 66 is never equal to the own count on the rows that matter (own 1, other 0; own 2, other 1).
SEATS = [seat(taken=2), seat(taken=0)]
ROWS = [announce(0, 0, 0, 0.5), announce(1, 0, 0, 0.25), announce(0, 1, 0, -0.5), announce(0, 2, 1, 0.125),
        bottom(0)]


class Announcements(unittest.TestCase):
    def test_the_own_count_is_column_63_and_the_other_seats_is_never_read(self):
        got = mm.read_episode(with_rows(SEATS, ROWS))
        self.assertEqual([tuple(a) for a in got['ann']], [(0, 0, 0.5), (1, 0, 0.25), (0, 1, -0.5), (0, 2, 0.125)])
        self.assertEqual(got['reward'], [1.0, -1.0])
        # the same rows with the other seat's count equal to the own count everywhere give the same reading
        same = [announce(0, 0, 0, 0.5), announce(1, 0, 0, 0.25), announce(0, 1, 1, -0.5), announce(0, 2, 2, 0.125)]
        self.assertEqual(mm.read_episode(with_rows(SEATS, same))['ann'], got['ann'])

    def test_put_backs_and_in_game_rows_are_not_announcements_and_the_scan_stops(self):
        rows = [announce(0, 0, 0, 0.5), bottom(0), game_row(0), announce(0, 5, 5, 9.0)]  # a late "announcement": unread
        got = mm.read_episode(with_rows([seat(taken=0), seat(taken=0)], rows))
        self.assertEqual([tuple(a) for a in got['ann']], [(0, 0, 0.5)])

    def test_columns_that_are_not_k_over_16_make_the_rows_unread_not_the_episode_refused(self):
        for bad in (announce(0, 0.3, 0, 0.5), announce(0, 7, 0, 0.5), announce(0, -1, 0, 0.5)):
            with self.subTest(own=bad['tensor']['action_features'][63]):
                got = mm.read_episode(with_rows([seat(), seat()], [bad]))
                self.assertIsNone(got['ann'])
                self.assertEqual(got['seats'], [seat(), seat()])
        disagree = announce(0, 1, 0, 0.5)
        disagree['tensor']['action_features'][70 + 63] = bits(2 / 16)  # the mulligan row says 2, the keep row 1
        self.assertIsNone(mm.read_episode(with_rows([seat(), seat()], [disagree]))['ann'])

    def test_no_terminal_reward_is_none(self):
        doc = json.loads(with_rows(SEATS, ROWS))
        doc.pop('terminal')
        self.assertIsNone(mm.read_episode(json.dumps(doc).encode())['reward'])


class PerCountLine(unittest.TestCase):
    def line(self, *episodes):
        return mm.iteration_line(0, [mm.read_episode(e) for e in episodes])

    def test_counts_mulligans_and_returns(self):
        line = self.line(with_rows(SEATS, ROWS))
        self.assertEqual(line['v'], 2)
        # seat 0 answered at counts 0, 1, 2 (two mulligans, then the keep); seat 1 answered once at 0
        self.assertEqual(line['dk'], [2, 1, 1, 0, 0, 0, 0])
        self.assertEqual(line['ma'], [1, 1, 0, 0, 0, 0, 0])
        self.assertEqual((line['rk'][5], line['rk'][7], line['rn'][5], line['rn'][7]), (1.0, -1.0, 1, 1))
        self.assertEqual(line['vn'], [2, 1, 1, 0, 0, 0, 0])
        self.assertEqual(line['vk'], [0.75, -0.5, 0.125, 0, 0, 0, 0])
        self.assertEqual(line['vr'], [0.0, 1.0, 1.0, 0, 0, 0, 0])  # k=0: seats 0 and 1 (1 + -1); k=1, 2: seat 0 (1)
        self.assertNotIn('annbad', line)
        self.assertNotIn('norew', line)
        self.assertEqual(json.loads(json.dumps(line)), line)

    def test_a_forced_zero_card_seat_answers_at_all_seven_counts_and_mulligans_each_time(self):
        rows = [announce(0, k, 0, 0.0) for k in range(7)] + [announce(1, 0, 6, 0.0)]
        line = self.line(with_rows([seat(taken=7), seat()], rows, reward=(-1.0, 1.0)))
        self.assertEqual(line['dk'], [2, 1, 1, 1, 1, 1, 1])
        self.assertEqual(line['ma'], [1, 1, 1, 1, 1, 1, 1])
        self.assertEqual((line['rk'][0], line['rn'][0], line['vn']), (-1.0, 1, [2, 1, 1, 1, 1, 1, 1]))
        self.assertNotIn('annbad', line)

    def test_rows_that_do_not_match_the_record_are_counted_and_leave_the_value_sums_alone(self):
        short = with_rows(SEATS, ROWS[:3])  # seat 0 took two mulligans but its row at count 2 is missing
        line = self.line(short, with_rows([seat(), seat()], [announce(0, 0, 0, 0.5), announce(1, 0, 0, 0.5)]))
        self.assertEqual(line['annbad'], 1)
        self.assertEqual(line['vn'], [2, 0, 0, 0, 0, 0, 0])  # only the matching episode's rows
        self.assertEqual(line['dk'][:3], [4, 1, 1])  # the record-derived counts do not depend on the rows
        wrong_actor = with_rows([seat(), seat('keep_seven')], [announce(0, 0, 0, 0.1), announce(1, 0, 0, 0.1)])
        self.assertEqual(self.line(wrong_actor)['annbad'], 1)  # the keep_seven seat is never asked

    def test_a_game_without_a_terminal_reward_leaves_its_seats_out_of_the_returns(self):
        doc = json.loads(with_rows(SEATS, ROWS))
        doc.pop('terminal')
        line = self.line(json.dumps(doc).encode())
        self.assertEqual((line['norew'], sum(line['rn']), line['annbad']), (2, 0, 1))
        self.assertEqual(line['dk'][0], 2)

    def test_a_vector_offset_is_one_list_and_a_disagreement_lists_the_values(self):
        a = with_rows(SEATS, ROWS, offset=[1.5, 3.0, 0.0, 6.0, 0.75, 0.0, 2.25])
        b = with_rows(SEATS, ROWS, offset=[1.5, 3.0, 0.0, 6.0, 0.75, 0.0, 2.25])
        self.assertEqual(self.line(a, b)['off'], [1.5, 3.0, 0.0, 6.0, 0.75, 0.0, 2.25])
        c = with_rows(SEATS, ROWS, offset=[0.5, 3.0, 0.0, 6.0, 0.75, 0.0, 2.25])
        mixed = self.line(a, c)
        self.assertIsNone(mixed['off'])
        self.assertEqual(mixed['offs'], [[0.5, 3.0, 0.0, 6.0, 0.75, 0.0, 2.25], [1.5, 3.0, 0.0, 6.0, 0.75, 0.0, 2.25]])
        self.assertIsNone(self.line(a, with_rows(SEATS, ROWS))['off'])  # a vector and no field disagree
        self.assertIsNone(self.line(with_rows(SEATS, ROWS))['off'])
        self.assertEqual(json.loads(json.dumps(mixed)), mixed)

    def test_the_line_stays_small_with_nine_decks(self):
        decks = ['Wildfire', 'Rally', 'Affinity', 'Elves', 'Spy', 'Burn', 'Terror', 'CawGates', 'Faeries']
        eps = [summary([seat(taken=k % 4), seat(taken=(k + 1) % 5)], (decks[k % 9], decks[(k + 3) % 9]),
                       [0.123456789] * 7, ann=[]) for k in range(4000)]
        size = len(json.dumps(mm.iteration_line(31, eps), sort_keys=True, separators=(',', ':'))) + 1
        self.assertLess(size, 1500)


class Reading(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.t = Path(self.tmp.name)
        self.cdir = self.t / 'chains' / 'n1'

    def chain(self, segments, recipe):
        """segments: [(first update, [(it, line)], complete)]"""
        entries = []
        for s, (offset, lines, complete) in enumerate(segments):
            seg = self.cdir / f'seg-{s:05d}'
            seg.mkdir(parents=True, exist_ok=True)
            for _, line in lines:
                mm.append_line(mm.opening_log_path(seg), line)
            if complete:
                write(seg / 'completion.json', b'{}')
            entries.append({'config': 'x', 'offset': offset, 'root': (self.t / 'elsewhere' / seg.name).as_posix(),
                            'updates': len(lines)})
        jwrite(self.cdir / 'chain.json', {'schema': 'w5-chain/v1', 'chain': 'n1', 'arm_name': 'C', 'recipe': recipe,
                                          'segments': entries})

    def episodes(self, u, schedule=VECTOR, drift=False):
        want = qual.keep_offset_value(schedule, u) if schedule else None
        if want and drift:
            want = [x + 0.5 if x else x for x in want]
        return [mm.read_episode(with_rows(SEATS, ROWS, offset=want))]

    def test_per_count_tables_sum_over_updates_and_a_v1_line_adds_nothing(self):
        lines = [(it, mm.iteration_line(it, self.episodes(it))) for it in range(3)]
        old = {'v': 1, 'it': 3, 'n': 1, 'off': None, 'learned': 2, 'kept': [0, 0, 0, 1, 0, 0, 0, 1], 'exp': 5,
               'fixed': 0, 'decks': {}}
        self.chain([(0, lines + [(3, old)], True)], {'keep_offset': VECTOR, 'pregame_advantage': 'terminal_v1'})
        report = mm.summarize_chain(self.cdir)
        total = report['total']
        self.assertEqual(report['pregame_advantage'], 'terminal_v1')
        self.assertEqual((total['iterations_logged'], total['v2_iterations'], total['learned_seats']), (4, 3, 8))
        by = {r['count']: r for r in total['by_count']}
        self.assertEqual((by[0]['decisions'], by[0]['mulligans'], by[0]['mulligan_again_share']), (6, 3, 0.5))
        self.assertEqual((by[1]['decisions'], by[1]['mulligans'], by[1]['mulligan_again_share']), (3, 3, 1.0))
        self.assertEqual((by[2]['decisions'], by[2]['mulligans'], by[2]['mulligan_again_share']), (3, 0, 0.0))
        self.assertEqual(by[0]['decisions_per_update'], 2.0)  # three v2 updates, not four
        self.assertIsNone(by[3]['mulligan_again_share'])
        self.assertEqual((by[0]['value_rows'], by[0]['mean_value'], by[0]['mean_realized']), (6, 0.375, 0.0))
        self.assertEqual((by[1]['mean_value'], by[1]['mean_realized']), (-0.5, 1.0))
        ret = {r['size']: r for r in total['ret_by_size']}
        self.assertEqual((ret[5]['n'], ret[5]['mean_return']), (3, 1.0))
        self.assertEqual((ret[7]['n'], ret[7]['mean_return']), (3, -1.0))
        self.assertIsNone(ret[2]['mean_return'])
        self.assertEqual(report['segments'][0]['by_count'][0]['decisions'], 6)

    def test_a_vector_schedule_is_compared_entry_by_entry(self):
        good = [(it, mm.iteration_line(it, self.episodes(it))) for it in range(2)]
        bad = [(it, mm.iteration_line(it, self.episodes(2 + it, drift=True))) for it in range(2)]
        self.chain([(0, good, True), (2, bad, True)], {'keep_offset': VECTOR})
        report = mm.summarize_chain(self.cdir)
        self.assertEqual([s['schedule_mismatch_iterations'] for s in report['segments']], [0, 2])
        force = report['lean_vector_in_force']
        self.assertEqual(force['update'], 3)
        self.assertEqual(force['schedule'], qual.keep_offset_value(VECTOR, 3))
        self.assertFalse(force['equal'])
        self.chain([(0, good, True)], {'keep_offset': VECTOR})
        self.assertTrue(mm.summarize_chain(self.cdir)['lean_vector_in_force']['equal'])
        self.chain([(0, good, True)], {})
        self.assertNotIn('lean_vector_in_force', mm.summarize_chain(self.cdir))

    def test_the_lean_vector_ends_with_the_field_absent(self):
        lines = [(it, mm.iteration_line(it, self.episodes(6 + it))) for it in range(4)]  # updates 6, 7, 8, 9
        self.chain([(6, lines, True)], {'keep_offset': VECTOR})
        report = mm.summarize_chain(self.cdir)
        self.assertEqual(report['segments'][0]['schedule_mismatch_iterations'], 0)
        force = report['lean_vector_in_force']
        self.assertEqual((force['update'], force['logged'], force['schedule'], force['equal']), (9, None, None, True))

    def test_text_names_the_vector_the_counts_and_the_returns(self):
        lines = [(it, mm.iteration_line(it, self.episodes(it))) for it in range(2)]
        self.chain([(0, lines, True)], {'keep_offset': VECTOR, 'pregame_advantage': 'terminal_v1'})
        text = mm.format_text(mm.summarize_chain(self.cdir, per_update=True))
        for needle in ('keep offset b0=[6.0, 12.0, 0.0, 24.0, 3.0, 0.0, 9.0]', 'pregame advantage terminal_v1',
                       'per own count k', 'again %', 'terminal return by kept hand size', 'lean vector in force at update 1',
                       'equal True', '6*..'):
            self.assertIn(needle, text)
        self.assertNotIn(chr(0x2014), text)

    def test_text_of_a_v1_only_chain_says_there_are_no_per_count_lines(self):
        old = {'v': 1, 'it': 0, 'n': 1, 'off': 1.0, 'learned': 2, 'kept': [0, 0, 0, 1, 0, 0, 0, 1], 'exp': 5,
               'fixed': 0, 'decks': {}}
        self.chain([(0, [(0, old)], True)], {})
        text = mm.format_text(mm.summarize_chain(self.cdir))
        self.assertIn('no v2 lines', text)
        self.assertEqual(mm.summarize_chain(self.cdir)['total']['v2_iterations'], 0)

    def test_cli_json_has_the_per_count_tables(self):
        lines = [(0, mm.iteration_line(0, self.episodes(0)))]
        self.chain([(0, lines, True)], {'keep_offset': VECTOR})
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(mm.main([str(self.cdir), '--json', '-']), 0)
        doc = json.loads(out.getvalue())
        self.assertEqual(doc['schema'], 'w5-mull-monitor/v2')
        self.assertEqual(len(doc['total']['by_count']), 7)
        self.assertEqual(len(doc['total']['ret_by_size']), 8)


class HookEndToEnd(unittest.TestCase):
    def test_a_recorded_iteration_carries_the_per_count_fields_and_a_vector_offset(self):
        with tempfile.TemporaryDirectory() as tmp:
            seg = Path(tmp) / 'seg-00000'
            off = qual.keep_offset_value(VECTOR, 2)
            it = make_iteration(seg, 2, [with_rows(SEATS, ROWS, offset=off),
                                         with_rows([seat(taken=1), seat('keep_seven')],
                                                   [announce(0, 0, 0, 0.5), announce(0, 1, 0, 0.5)], offset=off)])
            log = seg / mm.LOG_NAME
            self.assertEqual(mm.record_iteration(it, log), 'recorded')
            line = mm.read_log(log)[0][2]
            self.assertEqual((line['v'], line['off'], line['dk'], line['ma']), (2, off, [3, 2, 1, 0, 0, 0, 0],
                                                                                [2, 1, 0, 0, 0, 0, 0]))
            self.assertNotIn('annbad', line)


if __name__ == '__main__':
    unittest.main()
