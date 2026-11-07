"""Scratch segments (chains.py): reaping, persisting and resetting, with the bind mount simulated."""
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import chains  # noqa: E402


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
        tmp = self.cdir / '.seg-00000.persist'
        write(tmp / 'completion.json', b'{}')
        write(tmp / chains.PERSIST_MARKER, b'{}')
        self.native.mkdir(parents=True)  # the empty directory an unmount leaves
        iteration(chains.scratch_dir(self.arm, self.cdir, seg1), 0)
        seg1.mkdir(parents=True)
        self.mounts.add(seg1)
        man = {'segments': [{'root': self.native.as_posix()}, {'root': seg1.as_posix()}]}
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


if __name__ == '__main__':
    unittest.main()
