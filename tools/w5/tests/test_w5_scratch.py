"""Scratch segments (chains.py): reaping, persisting and resetting, with the bind mount simulated; and the counting
fixture of scratch_bench.py (mtg-kernel#601)."""
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import chains  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
import scratch_bench  # noqa: E402


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return hashlib.sha256(data).hexdigest()


def iteration(seg, i, done=True, s_state=False):
    it = seg / 'iterations' / f'{i:06d}'
    att = it / 'attempt-000000'
    for j in range(3):
        write(att / 'collect' / f'episode-{j:04d}.json', b'traj' * 100)
    write(att / 'collect' / 'collection.json', b'{}')
    digest = write(att / 'update' / 'checkpoint.json', f'ckpt-{i}'.encode() * 50)
    write(att / 'update' / 'update.json', b'{}' if not s_state else json.dumps(
        {'s_learner': {'state': {'path': (att / 'update' / 's-train-state.bin').as_posix(),
                                 'sha256': write(att / 'update' / 's-train-state.bin', f'S{i}'.encode() * 9)}}}).encode())
    if done:
        write(it / 'complete.json', json.dumps({'update': {'path': (att / 'update' / 'update.json').as_posix()},
                                                'output_identity': {'checkpoint_sha256': digest}}).encode())


@unittest.skipUnless(sys.platform.startswith('linux'), 'scratch segments are Linux only (rename onto an empty directory)')
class Scratch(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        base = Path(self.tmp.name)
        self.cdir = base / 'root' / 'chains' / 'c1'
        self.native = self.cdir / 'seg-00000'
        self.arm = {'scratch_root': (base / 'shm').as_posix()}
        self.target = chains.scratch_dir(self.arm, self.cdir, self.native)
        self.mounts = set()
        # The simulated mount: the native path reads the target while "mounted".
        patches = [mock.patch.object(chains, 'mounted', lambda p: Path(p) in self.mounts),
                   mock.patch.object(chains, 'unmount', self.fake_unmount)]
        for p in patches:
            p.start()
            self.addCleanup(p.stop)
        self.addCleanup(self.tmp.cleanup)

    def fake_unmount(self, native):
        self.mounts.discard(Path(native))
        Path(native).mkdir(parents=True, exist_ok=True)

    def test_reap_keeps_the_newest_completed_iteration(self):
        for i in range(3):
            iteration(self.target, i)
        iteration(self.target, 3, done=False)
        n = chains.reap(self.target)
        self.assertEqual(n, 2 * 4)  # three episodes and a checkpoint for iterations 0 and 1
        for i in (0, 1):
            att = self.target / 'iterations' / f'{i:06d}' / 'attempt-000000'
            self.assertFalse(list(att.rglob('episode-*')))
            self.assertFalse((att / 'update' / 'checkpoint.json').exists())
            self.assertTrue((att / 'update' / 'update.json').exists())
        for i in (2, 3):
            att = self.target / 'iterations' / f'{i:06d}' / 'attempt-000000'
            self.assertTrue((att / 'update' / 'checkpoint.json').exists())
            self.assertEqual(len(list(att.rglob('episode-*'))), 3)

    def test_persist_copies_records_and_the_newest_checkpoint(self):
        for i in range(3):
            iteration(self.target, i)
        iteration(self.target, 3, done=False)  # a wall cut
        write(self.target / 'run.json', b'{"run": 1}')
        self.native.mkdir(parents=True)
        self.mounts.add(self.native)
        chains.persist(self.arm, self.native, self.cdir)
        self.assertFalse(self.target.exists())
        self.assertFalse(chains.mounted(self.native))
        self.assertTrue((self.native / 'run.json.gz').exists())
        self.assertFalse((self.native / 'run.json').exists())
        files = sorted(p.relative_to(self.native).as_posix() for p in self.native.rglob('*') if p.is_file())
        self.assertNotIn('iterations/000003/attempt-000000/update/update.json', files)  # unfinished iteration
        self.assertEqual([f for f in files if f.endswith('checkpoint.json')],
                         ['iterations/000002/attempt-000000/update/checkpoint.json'])
        self.assertFalse([f for f in files if 'episode-' in f])
        for i in range(3):
            self.assertIn(f'iterations/{i:06d}/complete.json', files)
        self.assertFalse((self.cdir / '.seg-00000.persist').exists())

    def test_persist_refuses_a_checkpoint_that_differs_from_its_pin(self):
        iteration(self.target, 0)
        (self.target / 'iterations' / '000000' / 'attempt-000000' / 'update' / 'checkpoint.json').write_bytes(b'x')
        self.native.mkdir(parents=True)
        self.mounts.add(self.native)
        with self.assertRaises(chains.Stop):
            chains.persist(self.arm, self.native, self.cdir)
        self.assertTrue(chains.mounted(self.native))  # nothing swapped

    def test_settle_finishes_a_swap_and_resets_an_unfinished_segment(self):
        root = self.cdir.parent.parent
        seg1 = self.cdir / 'seg-00001'
        iteration(self.target, 0)
        write(self.target / 'completion.json', b'{}')
        self.native.mkdir(parents=True)
        self.mounts.add(self.native)
        chains.persist(self.arm, self.native, self.cdir)
        tmp = self.cdir / '.seg-00000.persist'
        self.native.rename(tmp)
        self.native.mkdir()  # the empty directory an unmount leaves
        iteration(chains.scratch_dir(self.arm, self.cdir, seg1), 0)
        seg1.mkdir(parents=True)
        self.mounts.add(seg1)
        man = {'segments': [{'root': p.as_posix(), 'offset': i, 'updates': 1}
                            for i, p in enumerate((self.native, seg1))]}
        with mock.patch.object(chains, 'log'):
            chains.scratch_settle(self.arm, root, self.cdir, man)
        self.assertTrue((self.native / 'completion.json').exists())
        self.assertFalse(tmp.exists())
        self.assertFalse(chains.mounted(seg1))
        self.assertFalse(chains.scratch_dir(self.arm, self.cdir, seg1).exists())

    def test_persist_keeps_the_newest_s_container(self):
        for i in range(3):
            iteration(self.target, i, s_state=True)
        self.native.mkdir(parents=True)
        self.mounts.add(self.native)
        chains.persist(self.arm, self.native, self.cdir)
        bins = sorted(p.relative_to(self.native).as_posix() for p in self.native.rglob('*.bin'))
        self.assertEqual(bins, ['iterations/000002/attempt-000000/update/s-train-state.bin'])

    def test_settle_discards_an_unmarked_staging_copy(self):
        root = self.cdir.parent.parent
        tmp = self.cdir / '.seg-00000.persist'
        write(tmp / 'completion.json', b'{}')  # a copy cut before its marker
        self.native.mkdir(parents=True)
        man = {'segments': [{'root': self.native.as_posix()}]}
        with mock.patch.object(chains, 'log'):
            chains.scratch_settle(self.arm, root, self.cdir, man)
        self.assertFalse(tmp.exists())
        self.assertFalse((self.native / 'completion.json').exists())

    def test_persist_writes_its_marker(self):
        iteration(self.target, 0)
        self.native.mkdir(parents=True)
        self.mounts.add(self.native)
        chains.persist(self.arm, self.native, self.cdir)
        self.assertTrue((self.native / chains.PERSIST_MARKER).exists())

    def test_read_config_reads_the_gzip_form(self):
        import retention
        cfg = self.cdir / 'segments' / 'seg-00000.json'
        write(cfg, b'{"iterations": [1]}')
        chains.compress_config(cfg)
        self.assertEqual(retention.read_config(cfg), {'iterations': [1]})

    def test_settle_ends_a_persisted_wall_cut(self):
        root = self.cdir.parent.parent
        for i in range(2):
            iteration(self.target, i)
        iteration(self.target, 2, done=False)
        self.native.mkdir(parents=True)
        self.mounts.add(self.native)
        chains.persist(self.arm, self.native, self.cdir)  # the cut was persisted; the driver stopped before END
        man = {'segments': [{'root': self.native.as_posix(), 'offset': 0, 'updates': 256}]}
        arm = dict(self.arm, arm='C')
        with mock.patch.object(chains, 'log'):
            chains.scratch_settle(arm, root, self.cdir, man)
        end = json.loads((self.cdir / 'END.json').read_text())
        self.assertEqual((end['reason'], end['last_valid_boundary']), ('c-wall', 2))
        self.assertTrue((self.native / 'iterations' / '000001' / 'complete.json').exists())

    def test_latest_persisted_skips_a_mounted_segment(self):
        seg0, seg1 = self.cdir / 'seg-00000', self.cdir / 'seg-00001'
        iteration(seg0, 0)
        iteration(seg1, 0)
        self.mounts.add(seg1)
        man = {'segments': [{'root': seg0.as_posix(), 'offset': 0}, {'root': seg1.as_posix(), 'offset': 1}]}
        self.assertEqual(chains.latest(man)[0], 2)
        self.assertEqual(chains.latest(man, persisted=True)[0], 1)

    def test_compress_config(self):
        cfg = self.cdir / 'segments' / 'seg-00000.json'
        write(cfg, b'{"iterations": []}' * 100)
        chains.compress_config(cfg)
        self.assertFalse(cfg.exists())
        import gzip
        self.assertEqual(gzip.open(cfg.with_name('seg-00000.json.gz')).read(), b'{"iterations": []}' * 100)

    def test_plan_accepts_scratch_only_for_c_in_stage_0(self):
        src = Path(chains.__file__).read_text(encoding='utf-8')
        self.assertIn("recipe == 'C' and a != 'F' and stage == '0'", src)


@unittest.skipUnless(sys.platform.startswith('linux'), 'scratch segments are Linux only (rename onto an empty directory)')
class ScratchWork(unittest.TestCase):
    """mtg-kernel#601: the counting fixture behind scratch_bench.py (reap, persist, scratch_settle unchanged)."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.profile = scratch_bench.PROFILES['small']

    def segment(self, name, n):
        sim = scratch_bench.MountSim(chains)
        sim.__enter__()
        self.addCleanup(sim.__exit__)
        seg = scratch_bench.Segment(chains, self.base / name / 'work', self.base / name / 'shm', self.profile, 7)
        seg.build(n, sim)
        return seg

    def test_fixture_is_deterministic_and_location_free(self):
        def tree(seg):
            return {p.relative_to(seg.target).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in seg.target.rglob('*') if p.is_file()}
        a, b = self.segment('a', 3), self.segment('b', 3)
        self.assertEqual(tree(a), tree(b))
        self.assertEqual(a.fixture_digest(), b.fixture_digest())
        self.assertEqual(len([f for f in tree(a) if f.endswith('checkpoint.json')]), 1)  # the newest; others reaped

    def test_ticks_delete_exactly_the_reaped_bulk_and_persist_keeps_every_member(self):
        out = scratch_bench.run_segment(chains, 4, self.profile, 7, self.base / 'shm', self.base / 'work', True)
        bulk = self.profile['episodes'] + 1  # trajectories and the checkpoint
        prod, idle = out['reap']['productive'], out['reap']['idle']
        self.assertEqual(prod['deleted'], [0, bulk, bulk, bulk])
        self.assertEqual(idle['deleted'], [0] * 4)
        self.assertEqual(prod['unlink'], prod['deleted'])  # the counter sees every deletion
        self.assertEqual(prod['new'][0], 0)  # the first tick has nothing to reap
        self.assertTrue(out['persist']['identical'])
        self.assertEqual(out['persist']['logical_digest'], out['persist']['fixture_digest'])
        self.assertEqual(out['persist']['returned_bytes'], out['persist']['physical_bytes'])
        self.assertLess(out['compress_config']['out_bytes'], out['compress_config']['in_bytes'])

    def test_counts_are_deterministic(self):
        runs = [scratch_bench.run_segment(chains, 3, self.profile, 7, self.base / f'shm{k}', self.base / f'work{k}',
                                          True) for k in range(2)]
        self.assertEqual(runs[0]['reap'], runs[1]['reap'])
        for scenario, field in scratch_bench.DETERMINISTIC:
            self.assertEqual(runs[0][scenario][field], runs[1][scenario][field], (scenario, field))

    def test_persist_durability_operations_in_order(self):
        """Every staged member and staging directory is fsynced before the marker, the marker before the swap, the
        swap and the parent fsync under the index lock: milestones 3 and 4 must keep these boundaries."""
        seg = self.segment('p', 3)
        with scratch_bench.Ops(full=True, classify=scratch_bench.persist_classifier(seg), events=True) as ops:
            chains.persist(seg.arm, seg.native, seg.cdir)
        ev = ops.events
        at = {k: [i for i, (op, cat, _) in enumerate(ev) if (cat, op) == k] for k in {(c, o) for o, c, _ in ev}}
        tmp = str(seg.tmp)
        staged = {p for op, cat, p in ev if (cat, op) == ('staging', 'fsync_file')}
        members = {os.path.join(tmp, os.path.relpath(p, seg.native)) for p in map(str, seg.native.rglob('*'))
                   if Path(p).is_file() and not p.endswith(chains.PERSIST_MARKER)}
        self.assertEqual(staged, members)
        synced_dirs = {p for op, cat, p in ev if (cat, op) == ('staging', 'fsync_dir')}
        dirs = {tmp} | {os.path.join(tmp, os.path.relpath(p, seg.native)) for p in map(str, seg.native.rglob('*'))
                        if Path(p).is_dir()}
        self.assertEqual(synced_dirs, dirs)
        marker_write = at[('marker', 'open')][0]
        self.assertGreater(marker_write, max(at[('staging', 'fsync_file')] + at[('staging', 'fsync_dir')][:-1]))
        self.assertGreater(at[('marker', 'fsync_file')][0], marker_write)
        lock, rename = at[('lock', 'flock')][0], at[('staging', 'rename')][0]
        parent, unlock = at[('chain_dir', 'fsync_dir')][0], at[('lock', 'funlock')][0]
        self.assertTrue(at[('marker', 'fsync_file')][0] < at[('staging', 'fsync_dir')][-1] < lock)  # staging root
        self.assertTrue(lock < rename < parent < unlock, (lock, rename, parent, unlock))

    def test_settle_cases_reach_their_states(self):
        out = scratch_bench.run_settle(chains, 3, self.profile, 7, self.base / 'shm', self.base / 'work', False)
        state = {case: out[case]['outcome'] for case in out}
        for case in ('steady', 'swap_finish', 'late_persist'):
            self.assertEqual(state[case], {'segment_persisted': True, 'mounted': False, 'scratch_left': False}, case)
        self.assertEqual(state['reset'], {'segment_persisted': False, 'mounted': False, 'scratch_left': False})

    def test_settle_setup_keeps_two_bulk_iterations_and_one_case_at_a_time(self):
        scratch, work = self.base / 'shm', self.base / 'work'
        live_iterations = peak_bytes = 0
        put = scratch_bench.Segment.put

        def observe(segment, *args, **kwargs):
            nonlocal live_iterations, peak_bytes
            result = put(segment, *args, **kwargs)
            live_iterations = max(live_iterations, len(segment.bulk))
            for parent in (scratch, work):
                if parent.exists():
                    self.assertLessEqual(len(list(parent.iterdir())), 1)
            peak_bytes = max(peak_bytes, sum(
                path.stat().st_size for parent in (scratch, work) for path in parent.rglob('*')
                if path.is_file() and not path.is_symlink()))
            return result

        with mock.patch.object(scratch_bench.Segment, 'put', observe):
            scratch_bench.run_settle(chains, 8, self.profile, 7, scratch, work, False)
        self.assertEqual(live_iterations, 2)
        self.assertLessEqual(peak_bytes, scratch_bench.fixture_bytes(self.profile, 8, settle=True, seed=7))
        self.assertEqual(list(scratch.iterdir()), [])
        self.assertEqual(list(work.iterdir()), [])

    def test_schedule_runs_each_size_a_b_b_a(self):
        order = scratch_bench.schedule([16, 64], ['A', 'B'], 2)
        for n in (16, 64):
            self.assertEqual([v for r, m, v in order if m == n], ['A', 'B', 'B', 'A'])
        self.assertEqual([m for r, m, v in order], [16, 16, 64, 64, 64, 64, 16, 16])

    def test_paired_verdict_needs_a_difference_beyond_each_arm_range(self):
        def receipt(a, b):
            samples = [{'variant': v, 'n': 2, 'round': r, 'reap': {'productive': {'wall_ns': [x]},
                                                                   'idle': {'wall_ns': [0]}},
                        'persist': {'wall_ns': x}} for v, xs in (('A', a), ('B', b)) for r, x in enumerate(xs)]
            return {'sizes': [2], 'rounds': len(a), 'samples': samples}
        same = scratch_bench.paired(receipt([100, 120], [110, 105]), 'A', 'B')['by_size']['2']['persist_ns']
        self.assertEqual(same['verdict'], 'inconclusive')
        fast = scratch_bench.paired(receipt([100, 104], [50, 52]), 'A', 'B')['by_size']['2']['persist_ns']
        self.assertEqual(fast['verdict'], 'b_faster')
        one = scratch_bench.paired(receipt([100], [50]), 'A', 'B')['by_size']['2']['persist_ns']
        self.assertEqual(one['verdict'], 'inconclusive')

    def test_bench_writes_a_complete_receipt(self):
        out = self.base / 'receipt.json'
        rc = scratch_bench.main(['--out', str(out), '--sizes', '2', '--rounds', '2', '--scratch-dir', self.tmp.name,
                                 '--work-dir', self.tmp.name, '--max-wall-seconds', '120', '--quiet'])
        self.assertEqual(rc, 0)
        r = json.loads(out.read_text())
        self.assertEqual((r['schema'], r['complete'], r['sizes']), (scratch_bench.SCHEMA, True, [2]))
        self.assertTrue(r['counts']['A']['2']['deterministic'])
        self.assertEqual([row['status'] for row in r['attempts']], ['complete'] * 4)
        self.assertTrue(r['summary']['A']['2']['counts']['persist_identical'])
        self.assertEqual([s['round'] for s in r['samples']], [0, 1])
        self.assertEqual([len(s['reap']['idle']['wall_us']) for s in r['samples']], [2, 2])  # every tick kept
        self.assertEqual(r['counts']['A']['2']['reap']['productive']['totals']['deleted'],
                         self.profile['episodes'] + 1)
        self.assertEqual(r['variants']['A']['files']['chains.py'], hashlib.sha256(
            Path(chains.__file__).read_bytes()).hexdigest())
        self.assertEqual(sorted(p.name for p in self.base.iterdir()), ['receipt.json'])  # workers clean up

    def test_bench_retains_completed_count_and_cleans_partial_worker_on_failure(self):
        original_run = subprocess.run
        for status in ('timed_out', 'failed'):
            with self.subTest(status=status):
                root = self.base / status
                root.mkdir()
                out = root / 'receipt.json'
                starts = []

                def fail_second(command, *args, **kwargs):
                    if len(command) < 3 or command[2] != scratch_bench.WORKER:
                        return original_run(command, *args, **kwargs)
                    payload = json.loads(command[3])
                    paths = [Path(payload[k]) for k in ('scratch', 'work')]
                    starts.append(paths)
                    self.assertTrue(all(path.parent == root for path in paths))
                    if len(starts) == 1:
                        return original_run(command, *args, **kwargs)
                    for path in paths:
                        (path / 'partial-fixture').write_text('owned sample', encoding='utf-8')
                    if status == 'timed_out':
                        raise subprocess.TimeoutExpired(
                            command, kwargs['timeout'], output=b'partial stdout', stderr=b'fault stderr')
                    raise subprocess.CalledProcessError(
                        7, command, output='partial stdout', stderr='fault stderr')

                with mock.patch.object(scratch_bench.subprocess, 'run', side_effect=fail_second):
                    rc = scratch_bench.main([
                        '--out', str(out), '--sizes', '2', '--rounds', '2',
                        '--scratch-dir', str(root), '--work-dir', str(root),
                        '--max-wall-seconds', '120', '--quiet',
                    ])
                self.assertEqual(rc, 3)
                receipt = json.loads(out.read_text(encoding='utf-8'))
                self.assertFalse(receipt['complete'])
                self.assertIsNone(receipt['counts']['A']['2']['deterministic'])
                self.assertTrue(receipt['counts']['A']['2']['persist']['identical'])
                self.assertEqual([row['status'] for row in receipt['attempts']], ['complete', status])
                failure = receipt['attempts'][1]
                self.assertEqual((failure['variant'], failure['n'], failure['repeat']), ('A', 2, 1))
                self.assertEqual(failure['stdout_tail'], 'partial stdout')
                self.assertEqual(failure['stderr_tail'], 'fault stderr')
                if status == 'failed':
                    self.assertEqual(failure['exit'], 7)
                else:
                    self.assertGreater(failure['timeout_seconds'], 0)
                self.assertEqual(sorted(path.name for path in root.iterdir()), ['receipt.json'])


class ScratchBenchSupervisor(unittest.TestCase):
    def test_parent_removes_only_owned_worker_paths_after_timeout(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            unrelated = root / 'keep'
            unrelated.write_text('outside the worker', encoding='utf-8')

            def interrupted(command, **kwargs):
                payload = json.loads(command[3])
                paths = [Path(payload[key]) for key in ('scratch', 'work')]
                self.assertNotEqual(*paths)
                for path in paths:
                    self.assertEqual(path.parent, root)
                    (path / 'partial').write_text('unfinished fixture', encoding='utf-8')
                raise subprocess.TimeoutExpired(command, kwargs['timeout'])

            with mock.patch.object(scratch_bench.subprocess, 'run', side_effect=interrupted):
                with self.assertRaises(subprocess.TimeoutExpired):
                    scratch_bench.spawn({'scratch': name, 'work': name}, timeout=0.1)
            self.assertEqual(list(root.iterdir()), [unrelated])
            self.assertEqual(unrelated.read_text(encoding='utf-8'), 'outside the worker')


class ScratchStagePinRecovery(unittest.TestCase):
    """Check real stage creation and restart with only the bind mount and directory fsync simulated."""

    def interrupted_swap(self, base, with_updates=True):
        root = base / 'root'
        cdir = root / 'chains' / 'c1'
        previous, native = (cdir / f'seg-{i:05d}' for i in range(2))
        arm = {'arm': 'C', 'scratch_root': (base / 'scratch').as_posix()}
        man = {'segments': [{'root': p.as_posix(), 'offset': i, 'updates': 1}
                            for i, p in enumerate((previous, native))]}

        iteration(previous, 0, s_state=True)
        write(previous / 'completion.json', b'{}')
        chains.record_commit(cdir, man, 0, 'scratch')
        retained = {p: p.read_bytes() for p in previous.rglob('*') if p.is_file()}
        retained[chains.commit_log(cdir)] = chains.commit_log(cdir).read_bytes()

        target = chains.scratch_dir(arm, cdir, native)
        write(target / 'run.json', b'{}')
        if with_updates:
            iteration(target, 0, s_state=True)
            write(target / 'completion.json', b'{}')
        real_fsync = chains.fsync_path

        def sync_file(path):
            if Path(path).is_file():
                real_fsync(path)

        with mock.patch.object(chains, 'mounted', return_value=True), \
                mock.patch.object(chains, 'unmount', side_effect=KeyboardInterrupt('before swap')), \
                mock.patch.object(chains, 'fsync_path', side_effect=sync_file):
            with self.assertRaises(KeyboardInterrupt):
                chains.persist(arm, native, cdir)
        stage = cdir / f'.{native.name}.persist'
        self.assertTrue((stage / chains.PERSIST_MARKER).is_file())
        return (arm, root, cdir, man), native, stage, retained

    def settle(self, args):
        with mock.patch.object(chains, 'mounted', return_value=False), \
                mock.patch.object(chains, 'fsync_path'), mock.patch.object(chains, 'log'):
            chains.scratch_settle(*args)

    def test_corrupted_stage_keeps_previous_durable_boundary_and_failed_bytes(self):
        for name, omit_pin in (('checkpoint.json', False), ('s-train-state.bin', False),
                               ('s-train-state.bin', True)):
            with self.subTest(name=name, omit_pin=omit_pin), tempfile.TemporaryDirectory() as directory:
                args, native, stage, retained = self.interrupted_swap(Path(directory))
                marker = stage / chains.PERSIST_MARKER
                manifest = chains.read(marker)
                rel = next(p for p in manifest['kept'] if Path(p).name == name)
                corrupt = stage / rel
                original = corrupt.read_bytes()
                broken = bytes([original[0] ^ 1]) + original[1:]
                corrupt.write_bytes(broken)  # same length, so count/size checks cannot substitute for a pin
                if omit_pin:
                    del manifest['kept'][rel]
                    marker.write_text(json.dumps(manifest), encoding='utf-8')
                expected_stage = {p: p.read_bytes() for p in stage.rglob('*') if p.is_file()}
                error = None
                try:
                    self.settle(args)
                except chains.Stop as exc:
                    error = str(exc)
                for p, expected in retained.items():
                    self.assertEqual(p.read_bytes(), expected, 'corrupted stage advanced durable state')
                self.assertFalse(native.exists(), 'corrupted stage was promoted')
                self.assertIsNotNone(error, 'corrupted stage was not refused')
                self.assertIn('kept pins' if omit_pin else 'differs from its pin', error)
                self.assertEqual({p: p.read_bytes() for p in stage.rglob('*') if p.is_file()}, expected_stage)

    def test_valid_stage_promotes_checkpoint_and_s_container(self):
        with tempfile.TemporaryDirectory() as directory:
            args, native, stage, _ = self.interrupted_swap(Path(directory))
            pins = chains.read(stage / chains.PERSIST_MARKER)['kept']
            self.assertEqual({Path(p).name for p in pins}, {'checkpoint.json', 's-train-state.bin'})
            self.settle(args)
            self.assertFalse(stage.exists())
            for path, digest in pins.items():
                self.assertEqual(chains.sha(native / path), digest)
            self.assertEqual(chains.committed_through(args[2]), 2)

    def test_empty_marked_wall_cut_keeps_previous_boundary(self):
        with tempfile.TemporaryDirectory() as directory:
            args, native, stage, retained = self.interrupted_swap(Path(directory), with_updates=False)
            self.assertEqual(chains.read(stage / chains.PERSIST_MARKER)['kept'], {})
            self.settle(args)
            self.assertFalse(stage.exists())
            self.assertTrue((native / chains.PERSIST_MARKER).is_file())
            for p, expected in retained.items():
                self.assertEqual(p.read_bytes(), expected)
            self.assertEqual(chains.read(args[2] / 'END.json')['last_valid_boundary'], 1)


if __name__ == '__main__':
    unittest.main()
