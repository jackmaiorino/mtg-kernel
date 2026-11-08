"""Durable throughput (tools/w5/chains.py module docstring, "Durable throughput"; mtg-kernel#600): the driver's
commit log and the companion durable metric of a phase. The metric is a pure function of recorded observations, so
these fixtures give it a fake clock and their expected numbers are worked out by hand in the comments, independent
of the code under test. The stand-in trainer runs it end to end in test_w5_throughput.py."""
import hashlib
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import chains  # noqa: E402

T0 = 1000.0  # the common start of every fake-clock window below


def commit(segment, unix, length=4, boundary=None, instance=0, how='native', state=None):
    """A commit line of segment `segment` (offset segment x length), as record_commit writes it."""
    offset = segment * length
    return {'schema': chains.COMMIT_SCHEMA, 'event': 'commit', 'segment': segment, 'offset': offset,
            'updates': length, 'boundary': offset + length if boundary is None else boundary, 'state': state,
            'cut': boundary is not None and boundary < offset + length, 'how': how, 'instance': instance,
            'unix': unix}


def chain(target, commits, starts=((0, T0),), arm='C', **extra):
    return dict(extra, arm=arm, target=target, commits=list(commits), starts=[list(s) for s in starts])


class DurableWindowTests(unittest.TestCase):
    """Hand-computed windows: every expected value is arithmetic on the fixture's own times."""

    def test_staggered_starts(self):
        # a starts 0.5 s and b 3.0 s after the common start; their boundary 8 commits land at 1020 and 1025.
        w = chains.durable_window(T0, {
            'a': chain(8, [commit(0, 1010.0), commit(1, 1020.0)], starts=[(0, 1000.5)]),
            'b': chain(8, [commit(0, 1013.0), commit(1, 1025.0)], starts=[(0, 1003.0)])})
        self.assertTrue(w['complete'])
        self.assertEqual((w['end_unix'], w['elapsed_seconds'], w['updates']), (1025.0, 25.0, 16))
        self.assertAlmostEqual(w['updates_per_hour'], 16 / 25 * 3600)  # 2304
        self.assertEqual(w['completion_skew_seconds'], 5.0)  # 1025 - 1020
        self.assertEqual(w['launch_skew_seconds'], 2.5)  # 1003.0 - 1000.5
        a, b = w['per_chain']['a'], w['per_chain']['b']
        self.assertEqual((a['launch_offset_seconds'], b['launch_offset_seconds']), (0.5, 3.0))
        self.assertEqual((a['elapsed_seconds'], b['elapsed_seconds']), (20.0, 25.0))
        self.assertEqual((a['overshoot_updates'], a['restarts'], a['instances']), (0, 0, [0]))
        self.assertEqual(w['per_arm']['C']['updates'], 16)
        self.assertAlmostEqual(w['per_arm']['C']['updates_per_hour'], 2304.0)
        self.assertEqual(w['failures'], [])

    def test_unequal_targets(self):
        # a's 8 updates end at 1020, b's 12 (three segments) at 1036: 20 updates over 36 s.
        w = chains.durable_window(T0, {
            'a': chain(8, [commit(0, 1010.0), commit(1, 1020.0)]),
            'b': chain(12, [commit(0, 1012.0), commit(1, 1024.0), commit(2, 1036.0)])})
        self.assertEqual((w['end_unix'], w['elapsed_seconds'], w['updates']), (1036.0, 36.0, 20))
        self.assertAlmostEqual(w['updates_per_hour'], 20 / 36 * 3600)  # 2000
        self.assertEqual(w['completion_skew_seconds'], 16.0)
        self.assertEqual(w['target_updates'], 20)

    def test_delayed_publication(self):
        # a's native receipt of boundary 8 is at 1019.0 but its segment is durable only at 1027.5 (a slow persist):
        # the window ends at the commit, 8.5 s after a receipt-based end would.
        w = chains.durable_window(T0, {
            'a': chain(8, [commit(0, 1010.0), commit(1, 1027.5, how='scratch')], receipt_unix=1019.0),
            'b': chain(8, [commit(0, 1009.0), commit(1, 1018.4)], receipt_unix=1018.0)})
        self.assertEqual((w['end_unix'], w['elapsed_seconds']), (1027.5, 27.5))
        self.assertAlmostEqual(w['updates_per_hour'], 16 / 27.5 * 3600)  # 2094.5...
        self.assertAlmostEqual(w['per_chain']['a']['publication_lag_seconds'], 8.5)
        self.assertAlmostEqual(w['per_chain']['b']['publication_lag_seconds'], 0.4)
        self.assertEqual(w['per_chain']['a']['how'], 'scratch')

    def test_interrupted_and_restarted_attempt(self):
        # Instance 0 commits segment 0 at 1010 and dies; instance 1 starts at 1030 and commits segment 1 at 1045:
        # the 20 s outage and the rerun are inside the 45 s window.
        w = chains.durable_window(T0, {'a': chain(8, [commit(0, 1010.0), commit(1, 1045.0, instance=1)],
                                                  starts=[(0, 1000.2), (1, 1030.0)])})
        a = w['per_chain']['a']
        self.assertEqual((w['elapsed_seconds'], a['instances'], a['restarts']), (45.0, [0, 1], 1))
        self.assertAlmostEqual(w['updates_per_hour'], 8 / 45 * 3600)  # 640
        # A promotion whose line was lost: the next driver commits it as recovered at its own time (1030.5), a bound
        # after the real one.
        w = chains.durable_window(T0, {'a': chain(8, [commit(0, 1010.0), commit(1, 1030.5, instance=1,
                                                                                 how='recovered')],
                                                  starts=[(0, 1000.2), (1, 1030.0)])})
        self.assertEqual((w['elapsed_seconds'], w['per_chain']['a']['how']), (30.5, 'recovered'))
        # A recovered line for an earlier segment written after a later one: the boundary is durable at the latest
        # of the commits that hold it (1031, not 1020).
        w = chains.durable_window(T0, {'a': chain(8, [commit(1, 1020.0), commit(0, 1031.0, how='recovered')])})
        self.assertEqual(w['elapsed_seconds'], 31.0)

    def test_double_observations(self):
        # Segment 0 twice at 1010, segment 1 at 1020 and again at 1031: the first observation of each counts.
        w = chains.durable_window(T0, {'a': chain(8, [commit(0, 1010.0), commit(0, 1010.0), commit(1, 1031.0),
                                                      commit(1, 1020.0)])})
        self.assertEqual((w['elapsed_seconds'], w['repeated_observations']), (20.0, 2))
        self.assertEqual(w['per_chain']['a']['repeated_observations'], 2)
        # Two observations of one segment that disagree stop: the log contradicts itself.
        with self.assertRaisesRegex(chains.Stop, r'two commits of segment 1 differ in \[\'boundary\'\]'):
            chains.durable_window(T0, {'a': chain(8, [commit(0, 1010.0), commit(1, 1020.0),
                                                      commit(1, 1021.0, boundary=7)])})
        with self.assertRaisesRegex(chains.Stop, r"differ in \['state'\]"):
            chains.durable_window(T0, {'a': chain(8, [commit(0, 1010.0, state='a' * 64),
                                                      commit(0, 1011.0, state='b' * 64)])})

    def test_fast_chain_overshoot(self):
        # a commits 16 boundaries by 1020, b reaches 8 at 1024: 8 + 8 count, a's 8 extra are overshoot, and a's
        # durable time is its boundary 8 commit (1010), not its last.
        w = chains.durable_window(T0, {
            'a': chain(8, [commit(s, 1005.0 + 5 * s) for s in range(4)]),
            'b': chain(8, [commit(0, 1012.0), commit(1, 1024.0)])})
        a = w['per_chain']['a']
        self.assertEqual((w['updates'], w['end_unix'], w['elapsed_seconds']), (16, 1024.0, 24.0))
        self.assertAlmostEqual(w['updates_per_hour'], 2400.0)
        self.assertEqual((a['durable_unix'], a['committed_through'], a['overshoot_updates']), (1010.0, 16, 8))
        # A target inside a segment counts only the target; the rest of the segment is overshoot.
        w = chains.durable_window(T0, {'a': chain(6, [commit(0, 1010.0), commit(1, 1020.0)])})
        self.assertEqual((w['updates'], w['elapsed_seconds'], w['per_chain']['a']['overshoot_updates']),
                         (6, 20.0, 2))

    def test_a_short_chain_leaves_the_window_incomplete(self):
        w = chains.durable_window(T0, {'a': chain(8, [commit(0, 1010.0), commit(1, 1020.0)]),
                                       'b': chain(8, [commit(0, 1013.0)])}, ended=1040.0)
        self.assertFalse(w['complete'])
        self.assertEqual((w['updates'], w['updates_per_hour'], w['end_unix'], w['elapsed_seconds']),
                         (None, None, None, None))
        self.assertEqual(w['failures'], [{'chain': 'b', 'reason': 'committed through boundary 4 of 8'}])
        self.assertTrue(w['per_chain']['a']['complete'])
        self.assertEqual(w['per_chain']['b']['committed_through'], 4)
        self.assertFalse(w['per_arm']['C']['complete'])
        self.assertIsNone(w['per_arm']['C']['updates_per_hour'])
        self.assertIsNone(w['teardown_seconds'])
        # A missing segment ends the durable prefix (segment 2 cannot count without 1), as does a cut segment.
        w = chains.durable_window(T0, {'a': chain(12, [commit(0, 1010.0), commit(2, 1030.0)]),
                                       'b': chain(8, [commit(0, 1010.0), commit(1, 1015.0, boundary=6)])})
        self.assertEqual([f['reason'] for f in w['failures']],
                         ['committed through boundary 4 of 12', 'committed through boundary 6 of 8'])
        # No commits at all, and no chains.
        self.assertFalse(chains.durable_window(T0, {'a': chain(8, [])})['complete'])
        self.assertFalse(chains.durable_window(T0, {})['complete'])

    def test_work_committed_before_the_window_never_counts(self):
        w = chains.durable_window(T0, {'a': chain(8, [commit(0, 999.0), commit(1, 1020.0)])})
        self.assertFalse(w['complete'])
        self.assertEqual(w['failures'], [{'chain': 'a', 'reason': 'segments [0] committed before the window start'}])

    def test_holds_and_teardown_are_reported_not_subtracted(self):
        # Node freezes 1005-1007 and 1024-1030 overlap the window [1000, 1025] for 2 + 1 s; the window still spans
        # 25 s, and the 3 s from its end to the last chain's end (1028) are teardown.
        w = chains.durable_window(T0, {'a': chain(8, [commit(0, 1010.0), commit(1, 1025.0)])},
                                  holds=[(1005.0, 1007.0), (1024.0, 1030.0)], ended=1028.0, pre_launch=0.25,
                                  observation={'poll_seconds': 0.5})
        self.assertEqual((w['elapsed_seconds'], w['held_seconds'], w['teardown_seconds']), (25.0, 3.0, 3.0))
        self.assertEqual((w['pre_launch_seconds'], w['observation']), (0.25, {'poll_seconds': 0.5}))
        # An incomplete window reports holds up to when its chains had ended.
        w = chains.durable_window(T0, {'a': chain(8, [commit(0, 1010.0)])}, holds=[(1005.0, 1007.0)], ended=1028.0)
        self.assertEqual(w['held_seconds'], 2.0)

    def test_housekeeping_is_reported_beside_the_commit(self):
        rows = [commit(0, 1010.0, how='scratch'),
                {'schema': chains.COMMIT_SCHEMA, 'event': 'housekeeping', 'segment': 0, 'seconds': 0.75,
                 'unix': 1010.8},
                commit(1, 1020.0, how='scratch'),
                {'schema': chains.COMMIT_SCHEMA, 'event': 'housekeeping', 'segment': 1, 'seconds': 0.5,
                 'unix': 1020.6}]
        w = chains.durable_window(T0, {'a': chain(8, rows)})
        self.assertEqual(w['elapsed_seconds'], 20.0)  # the final gzip (0.5 s) is after the window
        self.assertEqual((w['per_chain']['a']['housekeeping_seconds'], w['final_housekeeping_seconds']), (0.5, 0.5))

    def test_the_commit_must_name_the_indexed_state(self):
        rows = [commit(0, 1010.0), commit(1, 1020.0, state='c' * 64)]
        self.assertTrue(chains.durable_window(T0, {'a': chain(8, rows, state='c' * 64)})['complete'])
        with self.assertRaisesRegex(chains.Stop, 'names after-state'):
            chains.durable_window(T0, {'a': chain(8, rows, state='d' * 64)})

    def test_per_arm_results_on_a_mixed_window(self):
        # C's two chains are done at 1020 and 1024, CS's one at 1030: per arm 16 updates over 24 s and 8 over 30 s;
        # the whole window is 24 updates over 30 s.
        w = chains.durable_window(T0, {
            'c1': chain(8, [commit(0, 1010.0), commit(1, 1020.0)]),
            'c2': chain(8, [commit(0, 1012.0), commit(1, 1024.0)]),
            's1': chain(8, [commit(0, 1015.0), commit(1, 1030.0)], arm='CS')})
        self.assertAlmostEqual(w['updates_per_hour'], 24 / 30 * 3600)
        self.assertEqual((w['per_arm']['C']['updates'], w['per_arm']['C']['elapsed_seconds']), (16, 24.0))
        self.assertAlmostEqual(w['per_arm']['C']['updates_per_hour'], 16 / 24 * 3600)
        self.assertAlmostEqual(w['per_arm']['C']['updates_per_hour_over_window'], 16 / 30 * 3600)
        self.assertAlmostEqual(w['per_arm']['CS']['updates_per_hour'], 8 / 30 * 3600)


def h(data):
    return hashlib.sha256(data).hexdigest()


def native_segment(root, iterations, state_prefix='s', complete=True):
    """A finished native segment as the trainer leaves it: per iteration an attempt with update.json (its after-state)
    and complete.json pointing at it, then completion.json."""
    for i in range(iterations):
        it = Path(root) / 'iterations' / f'{i:06d}'
        upd = it / 'attempt-000000' / 'update' / 'update.json'
        upd.parent.mkdir(parents=True, exist_ok=True)
        upd.write_text(json.dumps({'after_state_sha256': h(f'{state_prefix}{i}'.encode())}))
        (it / 'complete.json').write_text(json.dumps({'update': {'path': upd.as_posix()}}))
    if complete:
        (Path(root) / 'completion.json').write_text('{}')


class CommitLogTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.cdir = Path(self.tmp.name) / 'root' / 'chains' / 'c1'
        self.cdir.mkdir(parents=True)
        self.man = {'segments': [{'root': (self.cdir / f'seg-{s:05d}').as_posix(), 'offset': 4 * s, 'updates': 4}
                                 for s in range(2)]}

    def test_record_commit_names_the_boundary_and_its_state(self):
        native_segment(self.man['segments'][0]['root'], 4)
        row = chains.record_commit(self.cdir, self.man, 0, 'native', instance=3)
        self.assertEqual((row['segment'], row['offset'], row['boundary'], row['cut'], row['instance']),
                         (0, 0, 4, False, 3))
        self.assertEqual(row['state'], h(b's3'))
        chains.record_housekeeping(self.cdir, 0, 0.125, config_gz_bytes=10)
        rows, bad = chains.read_commits(self.cdir)
        self.assertEqual(([r['event'] for r in rows], bad), (['commit', 'housekeeping'], 0))
        self.assertEqual(chains.committed_through(self.cdir), 4)
        # A cut segment commits its short boundary; a segment with no completed iteration commits nothing.
        native_segment(self.man['segments'][1]['root'], 2, complete=False)
        row = chains.record_commit(self.cdir, self.man, 1, 'native-cut')
        self.assertEqual((row['boundary'], row['cut'], row['state']), (6, True, h(b's1')))
        self.assertEqual(chains.committed_through(self.cdir), 6)
        empty = {'segments': [{'root': (self.cdir / 'seg-x').as_posix(), 'offset': 0, 'updates': 4}]}
        self.assertIsNone(chains.record_commit(self.cdir, empty, 0, 'native'))

    def test_a_torn_line_is_counted_and_never_read_as_a_commit(self):
        native_segment(self.man['segments'][0]['root'], 4)
        chains.record_commit(self.cdir, self.man, 0, 'native')
        log = chains.commit_log(self.cdir)
        with open(log, 'ab') as f:
            f.write(b'{"schema": "w5-segment-commit/v1", "event": "commit", "segm')  # a kill mid-append
        self.assertEqual(chains.read_commits(self.cdir)[1], 1)
        native_segment(self.man['segments'][1]['root'], 4, state_prefix='t')
        chains.record_commit(self.cdir, self.man, 1, 'native')  # the next append starts on its own line
        rows, bad = chains.read_commits(self.cdir)
        self.assertEqual(([r['segment'] for r in rows], bad), ([0, 1], 1))
        self.assertEqual(chains.committed_through(self.cdir), 8)
        self.assertEqual(len(log.read_bytes().splitlines()), 3)

    def test_a_driver_start_commits_a_durable_segment_left_without_its_line(self):
        native_segment(self.man['segments'][0]['root'], 4)
        chains.record_commit(self.cdir, self.man, 0, 'native')
        native_segment(self.man['segments'][1]['root'], 4, state_prefix='t')  # durable, its line lost
        chains.commit_recovered(self.cdir, self.man, instance=1)
        rows, _ = chains.read_commits(self.cdir)
        self.assertEqual([(r['segment'], r['how'], r['instance']) for r in rows],
                         [(0, 'native', None), (1, 'recovered', 1)])
        chains.commit_recovered(self.cdir, self.man, instance=2)  # nothing left to recover
        self.assertEqual(len(chains.read_commits(self.cdir)[0]), 2)
        # An unfinished segment (no completion.json) is not durable work to commit.
        man = {'segments': self.man['segments'] + [{'root': (self.cdir / 'seg-00002').as_posix(), 'offset': 8,
                                                    'updates': 4}]}
        native_segment(man['segments'][2]['root'], 3, complete=False)
        chains.commit_recovered(self.cdir, man, instance=2)
        self.assertEqual(chains.committed_through(self.cdir), 8)

    def test_reached_reads_the_commit_log(self):
        plan = {'root': self.cdir.parent.parent, 'chains': {'c1': {'arm': 'C'}},
                'arms': {'C': {'segment_updates': 4}}}
        native_segment(self.man['segments'][0]['root'], 4)
        native_segment(self.man['segments'][1]['root'], 4, state_prefix='t')
        # Both segments' receipts and completion.json exist, but only a commit line makes a boundary durable here.
        self.assertFalse(chains.reached(plan, 'c1', 4))
        chains.record_commit(self.cdir, self.man, 0, 'native')
        self.assertTrue(chains.reached(plan, 'c1', 4))
        self.assertFalse(chains.reached(plan, 'c1', 8))
        chains.record_commit(self.cdir, self.man, 1, 'native')
        self.assertTrue(chains.reached(plan, 'c1', 8))


@unittest.skipUnless(sys.platform.startswith('linux'),
                     'scratch segments are Linux only (rename onto an empty directory)')
class ScratchCommitTests(unittest.TestCase):
    """The unmount/rename window of a scratch persist, with the bind mount simulated as in test_w5_scratch.py."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        base = Path(self.tmp.name)
        self.root = base / 'root'
        self.cdir = self.root / 'chains' / 'c1'
        self.native = self.cdir / 'seg-00000'
        self.arm = {'scratch_root': (base / 'shm').as_posix()}
        self.target = chains.scratch_dir(self.arm, self.cdir, self.native)
        self.mounts, self.probe = set(), None
        for p in (mock.patch.object(chains, 'mounted', lambda p: Path(p) in self.mounts),
                  mock.patch.object(chains, 'unmount', self.fake_unmount)):
            p.start()
            self.addCleanup(p.stop)
        self.man = {'segments': [{'root': self.native.as_posix(), 'offset': 0, 'updates': 4}]}
        self.plan = {'root': self.root, 'chains': {'c1': {'arm': 'C'}}, 'arms': {'C': {'segment_updates': 4}}}

    def fake_unmount(self, native):
        # Just before the unmount the segment's last receipt reads through the mount; just after it the segment is
        # not mounted and its directory is empty until persist renames the copy in.
        through_mount = (self.target / 'iterations' / '000003' / 'complete.json').exists()
        self.mounts.discard(Path(native))
        shutil.rmtree(native, ignore_errors=True)  # what the mount showed there goes with it
        Path(native).mkdir(parents=True, exist_ok=True)
        if self.probe is not None:
            self.probe(through_mount)

    def finished_scratch_segment(self):
        for i in range(4):
            it = self.target / 'iterations' / f'{i:06d}'
            upd = it / 'attempt-000000' / 'update' / 'update.json'
            upd.parent.mkdir(parents=True, exist_ok=True)
            upd.write_text(json.dumps({'after_state_sha256': h(f's{i}'.encode())}))
            ck = upd.with_name('checkpoint.json')
            ck.write_bytes(f'ckpt-{i}'.encode())
            # Receipt paths are native paths, as the bind mount presents them to the trainer.
            native_upd = self.native / upd.relative_to(self.target)
            (it / 'complete.json').write_text(json.dumps({'update': {'path': native_upd.as_posix()},
                                                          'output_identity': {'checkpoint_sha256': h(
                                                              f'ckpt-{i}'.encode())}}))
        (self.target / 'completion.json').write_text('{}')
        self.native.mkdir(parents=True)
        self.mounts.add(self.native)

    def test_the_unmount_rename_window_is_not_reached(self):
        self.finished_scratch_segment()
        seen = {}

        def probe(through_mount):
            # The file test the poller used: complete.json seen (through the mount, a moment ago) and the segment
            # no longer mounted. It passes here, while the copy is not yet renamed in.
            seen['file_test'] = through_mount and not chains.mounted(self.native)
            seen['copy_in_place'] = (self.native / 'iterations').exists()
            seen['reached'] = chains.reached(self.plan, 'c1', 4)
        self.probe = probe
        chains.persist(self.arm, self.native, self.cdir)
        self.assertEqual(seen, {'file_test': True, 'copy_in_place': False, 'reached': False})
        self.assertFalse(chains.reached(self.plan, 'c1', 4))  # durable, but the driver has not written its line
        row = chains.record_commit(self.cdir, self.man, 0, 'scratch')
        self.assertTrue(chains.reached(self.plan, 'c1', 4))
        self.assertEqual((row['boundary'], row['state']), (4, h(b's3')))

    def test_a_finished_swap_syncs_the_parent_before_it_commits(self):
        # A driver stopped between persist's unmount and its rename: the marked staging copy is in place.
        self.finished_scratch_segment()

        def die(_):
            raise KeyboardInterrupt('the driver dies before the rename')
        self.probe = die
        with self.assertRaises(KeyboardInterrupt):
            chains.persist(self.arm, self.native, self.cdir)
        self.assertTrue((self.cdir / '.seg-00000.persist' / chains.PERSIST_MARKER).exists())
        self.probe, events = None, []
        real_fsync = chains.fsync_path

        def fsync(path):
            events.append(('fsync', Path(path)))
            real_fsync(path)

        def commit(*a, **kw):
            events.append(('commit', a[3]))
            return real_commit(*a, **kw)
        real_commit = chains.record_commit
        with mock.patch.object(chains, 'fsync_path', fsync), mock.patch.object(chains, 'record_commit', commit), \
                mock.patch.object(chains, 'log'):
            chains.scratch_settle(self.arm, self.root, self.cdir, self.man, instance=1)
        self.assertEqual(events, [('fsync', self.cdir), ('commit', 'scratch-swap')])
        rows, _ = chains.read_commits(self.cdir)
        self.assertEqual([(r['segment'], r['how'], r['boundary'], r['instance']) for r in rows],
                         [(0, 'scratch-swap', 4, 1)])

    def test_a_late_persist_commits_then_records_its_housekeeping(self):
        # A driver stopped after its trainer finished the segment: settle persists it late. Through the mount, the
        # native path shows the finished run's completion.json.
        self.finished_scratch_segment()
        (self.native / 'completion.json').write_text('{}')
        cfg = self.cdir / 'segments' / 'seg-00000.json'
        cfg.parent.mkdir(parents=True)
        cfg.write_text('{"iterations": []}')
        self.man['segments'][0]['config'] = cfg.as_posix()
        with mock.patch.object(chains, 'log'):
            chains.scratch_settle(self.arm, self.root, self.cdir, self.man)
        rows, _ = chains.read_commits(self.cdir)
        self.assertEqual([(r['event'], r.get('how')) for r in rows], [('commit', 'scratch-late'),
                                                                       ('housekeeping', None)])
        self.assertGreater(rows[0]['persist_bytes'], 0)
        self.assertTrue(cfg.with_name('seg-00000.json.gz').exists())
        self.assertEqual(chains.committed_through(self.cdir), 4)


if __name__ == '__main__':
    unittest.main()
