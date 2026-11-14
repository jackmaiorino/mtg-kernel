"""The London mulligan monitor (tools/w5/mull_monitor.py) and its hook in the scratch reaper (chains.py): one opening
line per iteration kept before the trajectories go, descriptive readings of those lines, and arm files, tool pins
and reaped trees unchanged when the hook is off. Synthetic records shaped like the trainer's trajectories."""
import contextlib
import hashlib
import io
import json
import shutil
import struct
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parent.parent / 'ex1'))
sys.path.insert(0, str(HERE))
import chains  # noqa: E402
import mull_monitor as mm  # noqa: E402
import qual  # noqa: E402
from test_w5_chains import Node, jwrite  # noqa: E402
from test_w5_keep_offset import opening_base  # noqa: E402

KEEP = {'b0': 6.0, 'end_update': 8, 'shape': 'linear-v1'}


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return hashlib.sha256(data).hexdigest()


def seat(mode='learned', taken=0, exposed=None):
    """One seat of an opening record, as the trainer writes it."""
    if exposed is None:
        exposed = 2 * taken + 1 if mode == 'learned' else 0
    return {'mode': mode, 'mulligans_taken': taken, 'kept_hand_size': 7 - taken, 'exposed_pregame_decisions': exposed,
            'forced_zero_card_keep': taken == 7}


def summary(seats, decks=('Wildfire', 'Rally'), offset=None, opening=True, record=True):
    """A `read_episode` result."""
    return {'opening': opening, 'offset': offset, 'decks': list(decks), 'seats': list(seats) if record else None}


def trajectory(seats, decks=('Wildfire', 'Rally'), offset=None, opening=True, decisions=40, decoy=False,
               order='serde'):
    """The bytes of a trajectory file: the members the monitor reads among thousands of decision rows."""
    episode = {'id': 'ns-r-u0-j0-s0', 'seed': 7, 'learner_seat': 0, 'starting_player': 1,
               'registered': [{'label': f'{d}/main', 'cards': ['x'] * 3} for d in decks],
               'selected': [{'label': f'{d}/main'} for d in decks], 'postboard': False,
               'max_physical_decisions': 100000, 'runtime_rules': 'resolution-boundary-v1'}
    if opening:
        episode['opening'] = 'london-v1'
        episode['mulligan_modes'] = [s['mode'] for s in seats]
    if offset is not None:
        episode['london_keep_offset'] = offset
    rows = [{'actor': i % 2, 'logits': [1, 2], 'tensor': {'state': list(range(30)), 'action_features': [3] * 20}}
            for i in range(decisions)]
    source = {'path': '/imports/flat-v7.json', 'sha256': 'a' * 64}
    if decoy:  # an earlier member that has a key of the same name, with another shape
        source['episode'] = 'not an episode'
    doc = {'feature_contract_digest': 'f' * 64, 'source_import': source, 'episode': episode,
           'configuration_sha256': ['c' * 64, 'd' * 64], 'decisions': rows,
           'terminal': {'terminal_reward': [1.0, -1.0], 'terminal_classification': 'natural'}}
    if opening:
        doc['opening_record'] = {'opening': 'london-v1', 'seats': list(seats)}
    doc['search_played'] = [{'step': 3, 'policy_selected': 1}]
    if order == 'reversed':
        doc = dict(reversed(list(doc.items())))
    return json.dumps(doc, separators=(',', ':')).encode()


def make_iteration(seg, i, trajs, done=True):
    """A completed (or not) iteration of a segment as the trainer lays it out."""
    it = Path(seg) / 'iterations' / f'{i:06d}'
    att = it / 'attempt-000000'
    for j, data in enumerate(trajs):
        write(att / 'collect' / f'episode-{j:04d}.json', data)
    write(att / 'collect' / 'collection.json', b'{}')
    digest = write(att / 'update' / 'checkpoint.json', f'ckpt-{i}'.encode() * 50)
    update = att / 'update' / 'update.json'
    update_hash = write(update, b'{}')
    if done:
        write(it / 'complete.json', json.dumps({'update': {'path': update.as_posix(), 'sha256': update_hash},
                                                'output_identity': {'checkpoint_sha256': digest}}).encode())
    return it


def mixed_iteration(i):
    """Three trajectories: a self-play game, a game against a keep-seven opponent, a game without an opening."""
    return [trajectory([seat(taken=0), seat(taken=1)], offset=6.0),
            trajectory([seat(taken=7), seat('keep_seven')], decks=('Wildfire', 'Burn'), offset=6.0),
            trajectory([], opening=False)]


def snapshot(root):
    return {p.relative_to(root).as_posix(): p.read_bytes() for p in sorted(Path(root).rglob('*')) if p.is_file()}


class ReadEpisode(unittest.TestCase):
    def test_the_two_members_are_parsed_alone_and_agree_with_a_full_parse(self):
        cases = [([seat(taken=0), seat(taken=2)], {}), ([seat(taken=7), seat('keep_seven')], {'offset': 0.4375}),
                 ([seat(), seat()], {'offset': 0.4, 'order': 'reversed'}), ([], {'opening': False})]
        for seats, kw in cases:
            with self.subTest(kw=kw):
                data = trajectory(seats, **kw)
                with mock.patch.object(mm.json, 'loads', side_effect=AssertionError('full parse')):
                    got = mm.read_episode(data)
                self.assertEqual(got, summary(seats, offset=kw.get('offset'), opening=kw.get('opening', True),
                                              record=kw.get('opening', True)))

    def test_an_earlier_member_of_the_same_name_falls_back_to_a_full_parse(self):
        data = trajectory([seat(taken=3), seat()], decoy=True, offset=2.5)
        self.assertEqual(mm.read_episode(data), summary([seat(taken=3), seat()], offset=2.5))

    def test_a_sealed_record_is_opened(self):
        raw = trajectory([seat(taken=1), seat()])
        try:
            from compression import zstd
            sealed = zstd.compress(raw)
        except ImportError:
            try:
                import zstandard
                sealed = zstandard.ZstdCompressor().compress(raw)
            except ImportError:
                self.skipTest('no zstd encoder here')
        try:
            self.assertEqual(qual.open_record(sealed), raw)
        except ValueError:
            self.skipTest('this encoder does not declare the content size an envelope needs')
        self.assertEqual(mm.read_episode(sealed), mm.read_episode(raw))


class Lines(unittest.TestCase):
    def test_tally_of_a_mixed_iteration(self):
        eps = [summary([seat(taken=0), seat(taken=1)], ('Wildfire', 'Rally'), 6.0),
               summary([seat(taken=7), seat('keep_seven')], ('Wildfire', 'Burn'), 6.0),
               summary([], opening=False, record=False)]
        line = mm.iteration_line(5, eps)
        self.assertEqual(line, {'v': 1, 'it': 5, 'n': 2, 'off': 6.0, 'learned': 3, 'kept': [1, 0, 0, 0, 0, 0, 1, 1],
                                'exp': 1 + 3 + 15, 'fixed': 1, 'plain': 1,
                                'decks': {'Wildfire': [1, 0, 0, 0, 0, 0, 0, 1], 'Rally': [0, 0, 0, 0, 0, 0, 1, 0]}})
        self.assertEqual(json.loads(json.dumps(line)), line)

    def test_the_offset_in_force_and_disagreement(self):
        two = lambda a, b: mm.iteration_line(0, [summary([seat(), seat()], offset=a),  # noqa: E731
                                                 summary([seat(), seat()], offset=b)])
        self.assertEqual(two(1.5, 1.5)['off'], 1.5)
        self.assertIsNone(two(None, None)['off'])
        self.assertNotIn('offs', two(None, None))
        mixed = two(1.5, 0.5)
        self.assertEqual((mixed['off'], mixed['offs']), (None, [0.5, 1.5]))
        mixed = two(1.5, None)
        self.assertEqual((mixed['off'], mixed['offs']), (None, [1.5, None]))
        none_open = mm.iteration_line(0, [summary([], opening=False, record=False)])
        self.assertEqual((none_open['n'], none_open['plain'], none_open['learned'], none_open['off']), (0, 1, 0, None))

    def test_an_opening_episode_without_a_record_is_counted_not_hidden(self):
        line = mm.iteration_line(0, [summary([seat(), seat()]), summary([], record=False)])
        self.assertEqual((line['n'], line['norec'], line['learned']), (2, 1, 2))

    def test_a_keep_seven_seat_that_did_not_keep_seven_is_flagged(self):
        bad = dict(seat('keep_seven'), mulligans_taken=1, kept_hand_size=6)
        line = mm.iteration_line(0, [summary([seat(), bad])])
        self.assertEqual((line['fixed'], line['fixed_not7']), (1, 1))

    def test_a_record_the_trainer_would_refuse_is_refused(self):
        for broken in (dict(seat(taken=2), kept_hand_size=4), dict(seat(taken=7), forced_zero_card_keep=False),
                       dict(seat(), forced_zero_card_keep=True), dict(seat(), mode='frozen'),
                       dict(seat(), kept_hand_size=8, mulligans_taken=-1), dict(seat(), kept_hand_size=7.0),
                       dict(seat(), exposed_pregame_decisions=-1), {}):
            with self.subTest(broken=broken), self.assertRaises(ValueError):
                mm.iteration_line(0, [summary([seat(), broken])])
        with self.assertRaises(ValueError):
            mm.iteration_line(0, [summary([seat()], decks=('A', 'B'))])  # one seat record, two decks

    def test_the_line_is_small(self):
        decks = ['Wildfire', 'Rally', 'Affinity', 'Elves', 'Spy', 'Burn', 'Terror', 'CawGates', 'Faeries']
        eps = [summary([seat(taken=k % 4), seat(taken=(k + 1) % 5)], (decks[k % 9], decks[(k + 3) % 9]), 0.123456789)
               for k in range(4000)]
        size = len(json.dumps(mm.iteration_line(31, eps), sort_keys=True, separators=(',', ':'))) + 1
        self.assertLess(size, 700)


class Rates(unittest.TestCase):
    def test_hand_computed(self):
        sums = mm.empty_sums()
        mm.add_line(sums, mm.iteration_line(0, [summary([seat(taken=0), seat(taken=1)], ('Wildfire', 'Rally'), 6.0),
                                                summary([seat(taken=7), seat('keep_seven')], ('Wildfire', 'Burn'))]))
        r = mm.rates(sums)
        self.assertEqual((r['learned_seats'], r['mulligans'], r['announcements']), (3, 8, 10))
        self.assertEqual(r['mulligan_rate'], 0.8)  # 8 mulligans over 10 announcements (7 forced ones count once each)
        self.assertEqual(r['first_mulligan_share'], round(2 / 3, 6))
        self.assertEqual(r['zero_card_share'], round(1 / 3, 6))
        self.assertEqual(r['mean_kept'], round(13 / 3, 6))
        self.assertEqual(r['kept_count'], [1, 0, 0, 0, 0, 0, 1, 1])
        self.assertEqual(r['exposed_per_seat'], round(19 / 3, 6))
        decks = mm.by_deck(sums)
        self.assertEqual(decks['Wildfire']['mulligan_rate'], 0.875)  # kept 7 and 0: 7 mulligans, 8 announcements
        self.assertEqual(decks['Rally']['mulligan_rate'], 0.5)
        self.assertNotIn('Burn', decks)  # a keep_seven seat is not a learned seat

    def test_an_empty_tally_has_no_rates_instead_of_zeros(self):
        r = mm.rates(mm.empty_sums())
        self.assertEqual((r['mulligan_rate'], r['zero_card_share'], r['mean_kept']), (None, None, None))

    def test_never_mulligan_is_one_announcement_per_seat(self):
        sums = mm.empty_sums()
        mm.add_line(sums, mm.iteration_line(0, [summary([seat(), seat()])] * 3))
        r = mm.rates(sums)
        self.assertEqual((r['announcements'], r['mulligans'], r['mulligan_rate']), (6, 0, 0.0))


class Hook(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.t = Path(self.tmp.name)
        self.seg = self.t / 'seg-00000'
        self.log = self.seg / mm.LOG_NAME

    def test_one_line_per_completed_iteration_once(self):
        it = make_iteration(self.seg, 3, mixed_iteration(3))
        self.assertEqual(mm.record_iteration(it, self.log), 'recorded')
        self.assertEqual(mm.record_iteration(it, self.log), 'skipped')
        lines = self.log.read_text().splitlines()
        self.assertEqual(len(lines), 1)
        self.assertEqual(json.loads(lines[0])['it'], 3)
        self.assertEqual(json.loads(lines[0])['n'], 2)

    def test_it_reads_only_and_an_iteration_without_trajectories_writes_nothing(self):
        it = make_iteration(self.seg, 0, mixed_iteration(0))
        before = snapshot(self.seg)
        mm.record_iteration(it, self.log)
        after = snapshot(self.seg)
        self.assertEqual({k: v for k, v in after.items() if k != mm.LOG_NAME}, before)
        for f in it.rglob('episode-*'):
            f.unlink()
        gone = make_iteration(self.seg, 1, [])
        self.assertEqual(mm.record_iteration(gone, self.log), 'empty')
        self.assertEqual(mm.record_iteration(self.seg / 'iterations' / '000009', self.log), 'empty')  # no complete.json
        self.assertEqual(len(self.log.read_text().splitlines()), 1)

    def test_an_unreadable_iteration_becomes_an_error_line_and_a_later_pass_replaces_it(self):
        trajs = mixed_iteration(0)
        trajs[1] = b'{"episode": truncated'
        it = make_iteration(self.seg, 0, trajs)
        self.assertEqual(mm.record_iteration(it, self.log), 'error')
        lines, errors, bad = mm.read_log(self.log)
        self.assertEqual((lines, bad, list(errors)), ({}, 0, [0]))
        self.assertLess(len(errors[0]), 200)
        (it / 'attempt-000000' / 'collect' / 'episode-0001.json').write_bytes(mixed_iteration(0)[1])
        self.assertEqual(mm.record_iteration(it, self.log), 'recorded')
        lines, errors, _ = mm.read_log(self.log)
        self.assertEqual((sorted(lines), errors), ([0], {}))

    def test_a_broken_opening_record_is_an_error_not_a_crash(self):
        trajs = [trajectory([seat(taken=2), dict(seat(), kept_hand_size=1)])]
        self.assertEqual(mm.record_iteration(make_iteration(self.seg, 0, trajs), self.log), 'error')

    def test_a_torn_last_line_is_tolerated_and_its_iteration_rerecorded(self):
        for i in range(2):
            mm.record_iteration(make_iteration(self.seg, i, mixed_iteration(i)), self.log)
        raw = self.log.read_bytes()
        self.log.write_bytes(raw[:-20])  # the second line is cut
        lines, errors, bad = mm.read_log(self.log)
        self.assertEqual((sorted(lines), bad), ([0], 1))
        self.assertEqual(mm.record_iteration(self.seg / 'iterations' / '000001', self.log), 'recorded')
        lines, errors, bad = mm.read_log(self.log)
        self.assertEqual((sorted(lines), errors, bad), ([0, 1], {}, 1))

    def test_a_log_that_cannot_be_written_never_raises(self):
        it = make_iteration(self.seg, 0, mixed_iteration(0))
        self.assertEqual(mm.record_iteration(it, self.t / 'no-such-dir' / mm.LOG_NAME), 'failed')
        self.assertEqual(mm.record_iteration(self.seg / 'iterations' / 'not-a-number', self.log), 'skipped')

    def test_the_log_is_never_appended_through_a_link_or_to_a_shared_file(self):
        import os
        victim = self.t / 'checkpoint.json'
        victim.write_bytes(b'{"state": "pinned"}\n')
        self.seg.mkdir(parents=True)
        try:
            os.symlink(victim, self.log)
        except (OSError, NotImplementedError):
            self.skipTest('no symlinks here')
        it = make_iteration(self.seg, 0, mixed_iteration(0))
        self.assertEqual(mm.record_iteration(it, self.log), 'failed')  # the log is refused, never followed
        with self.assertRaises(OSError):
            mm.append_line(self.log, {'v': 1, 'it': 9})
        self.assertEqual(victim.read_bytes(), b'{"state": "pinned"}\n')
        self.log.unlink()
        self.log.mkdir()  # a directory in its place
        with self.assertRaises(OSError):
            mm.append_line(self.log, {'v': 1, 'it': 9})
        self.log.rmdir()
        os.link(victim, self.log)  # a second name of a file that is not the log's own
        with self.assertRaises(OSError):
            mm.append_line(self.log, {'v': 1, 'it': 9})
        self.assertEqual(victim.read_bytes(), b'{"state": "pinned"}\n')
        self.log.unlink()
        self.assertEqual(mm.record_iteration(it, self.log), 'recorded')  # an ordinary file works as before
        mm.append_line(self.log, {'v': 1, 'it': 7})
        self.assertEqual(sorted(mm.read_log(self.log)[0]), [0, 7])

    def test_record_all_visits_the_completed_iterations_only(self):
        for i in range(3):
            make_iteration(self.seg, i, mixed_iteration(i))
        make_iteration(self.seg, 3, mixed_iteration(3), done=False)
        self.assertEqual(mm.record_all(self.seg, self.log), {'recorded': 3})
        self.assertEqual(mm.record_all(self.seg, self.log), {'skipped': 3})
        self.assertEqual(sorted(mm.read_log(self.log)[0]), [0, 1, 2])
        self.assertEqual(mm.record_all(self.t / 'nothing', self.log), {})


class ReaperHook(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.t = Path(self.tmp.name)

    def segment(self, name, n=4):
        seg = self.t / name
        for i in range(n):
            make_iteration(seg, i, mixed_iteration(i))
        make_iteration(seg, n, mixed_iteration(n), done=False)
        return seg

    def test_the_hook_off_changes_nothing(self):
        seg = self.segment('off')
        before = snapshot(seg)
        self.assertEqual(chains.reap(seg), 3 * 4)  # three iterations of three trajectories and a checkpoint
        self.assertEqual(chains.reap_iteration(seg / 'iterations' / '000003'), 3 + 1)
        gone = set(before) - set(snapshot(seg))
        self.assertTrue(gone and all(('episode-' in g or g.endswith('checkpoint.json')) for g in gone), gone)
        self.assertFalse(list(seg.rglob(mm.LOG_NAME)))

    def test_plain_directories_are_reaped_exactly_as_before_the_link_guard(self):
        """The reaper's deletion as it was before the link guard and the hook (a frozen copy), against today's, on
        the plain directories the trainer writes: the same count and the same tree, byte for byte."""
        def old_reap_iteration(iteration):
            n = 0
            for f in Path(iteration).rglob('*'):
                if f.is_file() and not f.is_symlink() and chains.scratch_bulk(f):
                    try:
                        f.unlink()
                        n += 1
                    except FileNotFoundError:
                        pass
            return n

        def old_reap(native):
            its = Path(native) / 'iterations'
            if not its.exists():
                return 0
            done = sorted(p for p in its.iterdir() if (p / 'complete.json').exists())
            return sum(old_reap_iteration(d) for d in done[:-1])

        def tree(seg):  # complete.json names its own segment path; the rest is compared byte for byte
            return {k: v.replace(seg.as_posix().encode(), b'SEG') for k, v in snapshot(seg).items()}
        old, new, ticked = self.segment('old'), self.segment('new'), self.segment('ticked')
        self.assertEqual(tree(old), tree(new))
        n_old, n_new = old_reap(old), chains.reap(new)
        self.assertEqual(n_old, 3 * 4)
        self.assertEqual(n_new, n_old)
        self.assertEqual(tree(new), tree(old))
        self.assertEqual(chains.Reaper(ticked).tick(), n_old)  # the thread's cursor reaps the same files
        self.assertEqual(tree(ticked), tree(old))
        self.assertEqual(old_reap_iteration(old / 'iterations' / '000003'),
                         chains.reap_iteration(new / 'iterations' / '000003'))
        self.assertEqual(tree(new), tree(old))
        self.assertEqual(old_reap(self.t / 'absent'), chains.reap(self.t / 'absent'))  # no iterations directory: 0

    def test_the_hook_on_deletes_the_same_files_and_adds_one_log(self):
        off, on = self.segment('a'), self.segment('b')
        log = on / mm.LOG_NAME
        n_off, n_on = chains.reap(off), chains.reap(on, log)
        self.assertEqual(n_off, n_on)

        def tree(seg):  # complete.json names its own segment path; the rest is compared byte for byte
            return {k: v.replace(seg.as_posix().encode(), b'SEG') for k, v in snapshot(seg).items()}
        tree_off, tree_on = tree(off), tree(on)
        self.assertEqual(set(tree_on) - set(tree_off), {mm.LOG_NAME})
        self.assertEqual({k: v for k, v in tree_on.items() if k != mm.LOG_NAME}, tree_off)  # every other byte alike
        self.assertEqual(sorted(mm.read_log(log)[0]), [0, 1, 2])  # the newest completed iteration is kept, unread
        self.assertTrue(list((on / 'iterations' / '000003').rglob('episode-*')))

    def test_a_reaper_logs_each_iteration_it_releases(self):
        seg = self.segment('r', n=5)
        log = seg / mm.LOG_NAME
        reaper = chains.Reaper(seg, log)
        self.assertEqual(reaper.tick(), 4 * 4)
        self.assertEqual(sorted(mm.read_log(log)[0]), [0, 1, 2, 3])
        self.assertEqual(reaper.tick(), 0)
        self.assertEqual(len(log.read_text().splitlines()), 4)
        self.assertEqual(chains.Reaper(seg, log).tick(), 0)  # a new reaper finds nothing left and logs nothing again
        self.assertEqual(len(log.read_text().splitlines()), 4)

    def test_a_failing_monitor_does_not_stop_the_deletion(self):
        seg = self.segment('f')
        write(seg / 'iterations' / '000000' / 'attempt-000000' / 'collect' / 'episode-0000.json', b'not json')
        n = chains.reap(seg, seg / mm.LOG_NAME)
        self.assertEqual(n, 3 * 4)
        self.assertFalse(list((seg / 'iterations' / '000000').rglob('episode-*')))
        self.assertEqual(list(mm.read_log(seg / mm.LOG_NAME)[1]), [0])

    def test_a_linked_iteration_directory_is_never_reaped(self):
        import os
        outside = self.t / 'elsewhere'
        make_iteration(outside, 0, mixed_iteration(0))  # a completed iteration of another tree, with bulk
        before = snapshot(outside)
        seg = self.t / 'linked'
        (seg / 'iterations').mkdir(parents=True)
        try:
            os.symlink(outside / 'iterations' / '000000', seg / 'iterations' / '000000', target_is_directory=True)
        except (OSError, NotImplementedError):
            self.skipTest('no symlinks here')
        make_iteration(seg, 1, mixed_iteration(1))
        for log in (None, seg / mm.LOG_NAME):
            self.assertEqual(chains.reap_iteration(seg / 'iterations' / '000000', log), 0)
            self.assertEqual(chains.Reaper(seg, log).tick(), 0)  # visits the linked 0, which is released and left
            self.assertEqual(snapshot(outside), before)  # not a byte of the other tree is touched
        self.assertFalse((seg / mm.LOG_NAME).exists())  # and nothing was read from it into the log
        # a linked `iterations` directory is skipped whole, including its genuine iterations
        real = self.segment('real')
        moved = self.t / 'moved-iterations'
        (real / 'iterations').rename(moved)
        os.symlink(moved, real / 'iterations', target_is_directory=True)
        kept = snapshot(self.t / 'moved-iterations')
        self.assertEqual(chains.reap(real), 0)
        self.assertEqual(chains.reap_iteration(real / 'iterations' / '000000'), 0)
        self.assertEqual(snapshot(self.t / 'moved-iterations'), kept)

    @unittest.skipUnless(sys.platform.startswith('linux'), 'scratch segments are Linux only')
    def test_persist_logs_the_newest_iteration_and_copies_the_log(self):
        cdir = self.t / 'root' / 'chains' / 'c1'
        native = cdir / 'seg-00000'
        for monitor in (True, False):
            arm = {'scratch_root': (self.t / f'shm-{monitor}').as_posix(), 'mull_monitor': monitor}
            target = chains.scratch_dir(arm, cdir, native)
            for i in range(3):
                make_iteration(target, i, mixed_iteration(i))
            make_iteration(target, 3, mixed_iteration(3), done=False)  # a wall cut
            reaper = chains.Reaper(target, mm.opening_log_path(target) if monitor else None)
            reaper.tick()  # iterations 0 and 1 while the trainer ran
            mounts = {native}
            native.mkdir(parents=True, exist_ok=True)
            with mock.patch.object(chains, 'mounted', lambda p: Path(p) in mounts), \
                    mock.patch.object(chains, 'unmount', lambda p: mounts.discard(Path(p))):
                chains.persist(arm, native, cdir)
            log = native / mm.LOG_NAME
            if monitor:
                lines, errors, bad = mm.read_log(log)
                self.assertEqual((sorted(lines), errors, bad), ([0, 1, 2], {}, 0))
                self.assertEqual(len(log.read_text().splitlines()), 3)
                self.assertEqual(lines[2]['n'], 2)  # the newest iteration's trajectories were still there
                marker = json.loads((native / chains.PERSIST_MARKER).read_text())
                self.assertIn(mm.LOG_NAME, [p.name for p in native.iterdir()])
                self.assertEqual(marker['files'], len([p for p in native.rglob('*') if p.is_file()]) - 1)
            else:
                self.assertFalse(log.exists())
            self.assertFalse([p for p in native.rglob('episode-*')])
            shutil.rmtree(native)

    def test_a_late_start_logs_only_what_is_left(self):
        seg = self.segment('late', n=3)
        chains.reap(seg)  # the monitor was off while iterations 0 and 1 were reaped
        log = seg / mm.LOG_NAME
        self.assertEqual(mm.record_all(seg, log), {'empty': 2, 'recorded': 1})
        self.assertEqual(sorted(mm.read_log(log)[0]), [2])


class Plan(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.node = Node(self.tmp.name)
        self.base = jwrite(self.node.t / 'base-open.json', opening_base())
        self.n = 0

    def plan(self, scratch=True, base=None, **arm):
        self.n += 1
        specs = {'c1': {'arm': 'C', 'slot': 0, 'seed_root': 'oc/c1'}}
        doc = json.loads(Path(self.node.inputs('0', specs, label=f'm{self.n}', c_wall_seconds=1.5,
                                               independent=True)).read_text())
        doc['scratch_root'] = (self.node.t / 'shm').as_posix()
        doc['arms']['C'] = dict(doc['arms']['C'], segment_updates=32, base=str(base or self.base),
                                **({'scratch': True} if scratch else {}), **arm)
        return chains.make_plan(jwrite(self.node.t / f'plan-{self.n}.json', doc))[1]

    @staticmethod
    def arm(plan):
        return Path(plan['arms']['C']['file']['path'])

    def test_off_by_default_and_the_arm_file_does_not_change(self):
        absent = self.plan()
        for off in (None, False):
            again = self.plan(mull_monitor=off)
            self.assertEqual(again['arms']['C']['file']['sha256'], absent['arms']['C']['file']['sha256'])
        arm = json.loads(self.arm(absent).read_text())
        self.assertNotIn('mull_monitor', arm)
        self.assertEqual(arm['tools'], chains.tools_shas())
        self.assertNotIn('mull_monitor.py', arm['tools'])
        self.assertNotIn('mull_monitor.py', absent['tools'])
        self.assertEqual(chains.load_arm(self.arm(absent))['tools'], chains.tools_shas())

    def test_on_pins_the_monitor_file_and_leaves_the_recipe_alone(self):
        plain, plan = self.plan(), self.plan(mull_monitor=True)
        arm = json.loads(self.arm(plan).read_text())
        self.assertIs(arm['mull_monitor'], True)
        monitor_sha = chains.sha(HERE.parent / 'mull_monitor.py')
        self.assertEqual(arm['tools'], dict(chains.tools_shas(), **{'mull_monitor.py': monitor_sha}))
        self.assertEqual(plan['tools'], chains.tools_shas())
        self.assertNotEqual(plan['arms']['C']['file']['sha256'], plain['arms']['C']['file']['sha256'])
        self.assertEqual(chains.load_arm(self.arm(plan))['mull_monitor'], True)
        self.assertEqual(chains.recipe_identity(arm), chains.recipe_identity(json.loads(self.arm(plain).read_text())))
        self.assertNotIn('mull_monitor', chains.recipe_identity(arm))
        # with a keep offset the identity is the keep-offset arm's, still without the monitor
        both = self.plan(mull_monitor=True, keep_offset=KEEP)
        self.assertEqual(json.loads(self.arm(both).read_text())['keep_offset'], KEEP)
        # an arm file whose flag was removed no longer matches its tool pins
        stripped = json.loads(self.arm(plan).read_text())
        stripped.pop('mull_monitor')
        path = self.node.t / 'stripped.json'
        jwrite(path, stripped)
        with self.assertRaises(chains.Stop):
            chains.load_arm(path)

    def test_refusals(self):
        with self.assertRaisesRegex(chains.Stop, 'scratch'):
            self.plan(scratch=False, mull_monitor=True)
        with self.assertRaisesRegex(chains.Stop, 'opening'):
            self.plan(mull_monitor=True, base=self.node.base)
        for bad in ('yes', 1, 0, [], {}):
            with self.subTest(bad=bad), self.assertRaisesRegex(chains.Stop, 'mull_monitor'):
                self.plan(mull_monitor=bad)


class Reading(unittest.TestCase):
    """A chain directory of three segments: the first complete and on schedule, the second with a gap and one update
    off the schedule, the third in flight past the end of the schedule."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.t = Path(self.tmp.name)
        self.cdir = self.t / 'chains' / 'm1'
        segs = []
        for s, (offset, its, complete) in enumerate([(0, [0, 1, 2, 3], True), (4, [0, 1, 3], True), (8, [0], False)]):
            seg = self.cdir / f'seg-{s:05d}'
            seg.mkdir(parents=True)
            for it in its:
                u = offset + it
                off = qual.keep_offset_value(KEEP, u)
                if (s, it) == (1, 3):
                    off = 0.5  # not the schedule's 0.75
                eps = [summary([seat(taken=u % 3), seat(taken=0)], ('Wildfire', 'Rally'), off),
                       summary([seat(taken=1), seat('keep_seven')], ('Wildfire', 'Burn'), off)]
                mm.append_line(mm.opening_log_path(seg), mm.iteration_line(it, eps))
            if complete:
                write(seg / 'completion.json', b'{}')
            segs.append({'config': 'x', 'offset': offset, 'root': (self.t / 'elsewhere' / seg.name).as_posix(),
                         'updates': 4})
        jwrite(self.cdir / 'chain.json', {'schema': 'w5-chain/v1', 'chain': 'm1', 'arm_name': 'C',
                                          'recipe': {'keep_offset': KEEP}, 'segments': segs})

    def test_segment_and_total_numbers(self):
        report = mm.summarize_chain(self.cdir)
        self.assertEqual(report['schema'], mm.SCHEMA)
        self.assertEqual(report['keep_offset_schedule'], KEEP)
        s0, s1, s2 = report['segments']
        self.assertEqual((s0['first_update'], s0['last_update'], s0['iterations_logged'], s0['missing_iterations']),
                         (0, 3, 4, []))
        self.assertEqual((s0['offset_first'], s0['offset_last']), (6.0, 3.75))
        self.assertEqual(s0['schedule_mismatch_iterations'], 0)
        self.assertEqual((s0['learned_seats'], s0['fixed_seats']), (12, 4))
        self.assertEqual(s1['missing_iterations'], [2])
        self.assertEqual(s1['schedule_mismatch_iterations'], 1)
        self.assertEqual((s1['first_update'], s1['last_update']), (4, 7))
        self.assertEqual((s2['offset_first'], s2['complete'], s2['schedule_mismatch_iterations']), (None, False, 0))
        self.assertEqual(s2['missing_iterations'], [])
        total = report['total']
        self.assertEqual(total['iterations_logged'], 8)
        self.assertEqual(total['learned_seats'], sum(s['learned_seats'] for s in report['segments']))
        self.assertEqual(total['kept_count'], [sum(s['kept_count'][k] for s in report['segments']) for k in range(8)])
        self.assertEqual(sorted(total['by_deck']), ['Rally', 'Wildfire'])

    def test_per_update_rows_and_the_last_option(self):
        report = mm.summarize_chain(self.cdir, per_update=True, last=2)
        self.assertEqual(report['segments_read'], ['seg-00001', 'seg-00002'])
        rows = report['per_update']
        self.assertEqual([r['update'] for r in rows], [4, 5, 7, 8])
        self.assertEqual([(r['offset'], r['expected_offset']) for r in rows],
                         [(3.0, 3.0), (2.25, 2.25), (0.5, 0.75), (None, None)])

    def test_no_schedule_means_no_comparison(self):
        man = json.loads((self.cdir / 'chain.json').read_text())
        man['recipe'] = {}
        jwrite(self.cdir / 'chain.json', man)
        report = mm.summarize_chain(self.cdir)
        self.assertEqual([s['schedule_mismatch_iterations'] for s in report['segments']], [None] * 3)
        self.assertIn('no keep offset in the recipe', mm.format_text(report))

    def test_a_segment_without_a_log_is_reported_as_such(self):
        mm.opening_log_path(self.cdir / 'seg-00000').unlink()
        s0 = mm.summarize_chain(self.cdir)['segments'][0]
        self.assertFalse(s0['log_present'])
        self.assertEqual((s0['iterations_logged'], s0['learned_seats'], s0['mulligan_rate']), (0, 0, None))
        self.assertEqual(s0['missing_iterations'], [0, 1, 2, 3])
        self.assertIn('no log', mm.format_text(mm.summarize_chain(self.cdir)))

    def test_text_table(self):
        text = mm.format_text(mm.summarize_chain(self.cdir, per_update=True))
        for needle in ('chain m1 (C); keep offset b0=6.0, end_update=8, linear-v1', 'mull/ann %', 'seg-00001',
                       'missing 1', 'offset differs from schedule 1', 'in flight or cut', 'by deck', 'per update'):
            self.assertIn(needle, text)
        self.assertNotIn('—', text)

    def test_cli_reads_and_writes_nothing_in_the_chain_directory(self):
        before = snapshot(self.cdir)
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(mm.main([str(self.cdir)]), 0)
        self.assertIn('seg-00000', out.getvalue())
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(mm.main([str(self.cdir), '--json', '-']), 0)
        self.assertEqual(json.loads(out.getvalue())['total']['iterations_logged'], 8)
        target = self.t / 'out' / 'report.json'
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(mm.main([str(self.cdir), '--json', str(target), '--per-update', '--no-decks']), 0)
        self.assertIn('per_update', json.loads(target.read_text()))
        with contextlib.redirect_stderr(io.StringIO()) as err:
            self.assertEqual(mm.main([str(self.cdir), '--json', str(self.cdir / 'report.json')]), 2)
        self.assertIn('inside the chain directory', err.getvalue())
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(mm.main([str(self.t)]), 2)  # not a chain directory
        self.assertEqual(snapshot(self.cdir), before)

    def test_the_report_carries_readings_and_no_verdicts(self):
        text = json.dumps(mm.summarize_chain(self.cdir, per_update=True)).lower()
        for word in ('stop', 'alert', 'verdict', 'threshold', 'pass'):
            self.assertNotIn(word, text)


if __name__ == '__main__':
    unittest.main()
