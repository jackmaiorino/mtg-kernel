"""The W5 replay sample (tools/w5/replay_sample.py and its supervise integration) on any OS with the stand-in
trainer: label-blind selection keyed on the seed root, staging of every selected X update before rolling retention
(C and Stage 0 never sampled), protection of unstaged selected files, the guard's exact reservation, and the ship to
E: with hash checks and the per-arm quota (the kept set does not depend on the shipping order)."""
import contextlib
import io
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE))
import chains  # noqa: E402
import replay_sample  # noqa: E402
import retention  # noqa: E402
from test_w5_chains import Node, STEP_C, STEP_X, jwrite  # noqa: E402

try:
    replay_sample.zstd_api()
    HAS_ZSTD = True
except replay_sample.SampleError:
    HAS_ZSTD = False

STAGE1A = {'c1': {'arm': 'C', 'slot': 0, 'pair': 1, 'seed_root': 'w5/chain1'},
           'x8-1': {'arm': 'X8', 'slot': 1, 'pair': 1, 'seed_root': 'w5/chain1'},
           'c2': {'arm': 'C', 'slot': 2, 'pair': 2, 'seed_root': 'w5/chain2'},
           'x8-2': {'arm': 'X8', 'slot': 3, 'pair': 2, 'seed_root': 'w5/chain2'}}


class SelectionTests(unittest.TestCase):
    def test_label_blind_selection_keyed_on_the_seed_root(self):
        digest = replay_sample.selection_hash('w5/chain1', 0)
        text = json.dumps(['w5/replay-sample', 'w5/chain1', 0], separators=(',', ':'))
        self.assertEqual(digest, replay_sample.sha_bytes(text.encode()))
        self.assertEqual(replay_sample.selected('0' + 'f' * 63), True)
        self.assertEqual(replay_sample.selected('1' + '0' * 63), False)
        hits = sum(replay_sample.selected(replay_sample.selection_hash('w5/chain3', u)) for u in range(32_000))
        self.assertAlmostEqual(hits / 32_000, 1 / 16, delta=0.006)
        self.assertTrue(replay_sample.sampled('1a', 'X8') and replay_sample.sampled('1b', 'XS'))
        self.assertFalse(replay_sample.sampled('1a', 'C') or replay_sample.sampled('0', 'X8'))


@unittest.skipUnless(HAS_ZSTD, 'no zstd (Python 3.14 or the zstandard package)')
class StagingTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.node = Node(self.tmp.name)
        for patch in (mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X)}),
                      mock.patch.object(chains, 'GUARD_WAIT', 0.0), mock.patch.object(chains, 'GUARD_POLL', 0.0),
                      mock.patch.object(retention, 'TRAJECTORY_SEGMENTS', 2)):
            patch.start()
            self.addCleanup(patch.stop)
        self.path, self.plan = self.node.plan('1a', dict(STAGE1A))

    def tearDown(self):
        self.tmp.cleanup()

    def advance(self, name, segments):
        cdir = self.node.root / 'chains' / name
        self.node.guard(ceilings={name: chains.admitted(cdir) + segments})
        self.assertEqual(self.node.drive(self.plan, name), chains.EXIT_ROOM)

    def cycle(self):
        with contextlib.redirect_stdout(io.StringIO()), mock.patch.object(chains.subprocess, 'Popen'):
            return chains.cycle(chains.load_plan(self.path), self.path)

    def selected_updates(self, records, seed_root='w5/chain1'):
        return [r['update'] for r in records if replay_sample.selected(replay_sample.selection_hash(seed_root,
                                                                                                    r['update']))]

    def test_selected_updates_are_staged_before_rolling_retention(self):
        manifest = self.plan['chains']['x8-1']['manifest']
        for _ in range(12):
            self.advance('x8-1', 2)
            self.advance('c1', 1)
            report = self.cycle()
            self.assertNotIn('error', report['replay_sample'].get('x8-1', {}))
        _, records = retention.index(manifest)
        chosen = self.selected_updates(records)
        self.assertGreaterEqual(len(chosen), 1, 'the history should select at least one update')
        staging = self.node.root / 'replay-sample'
        entries = sorted(staging.glob('X8/x8-1/u*/ENTRY.json'))
        self.assertEqual([json.loads(p.read_text())['update'] for p in entries], chosen)
        self.assertFalse(list(staging.glob('C/**/*')))  # C is not sampled
        _, decompress = replay_sample.zstd_api()[1:]
        for p in entries:
            entry = json.loads(p.read_text())
            names = sorted(i['path'].split('/')[-1] for i in entry['files'])
            self.assertEqual(names, ['collection.json', 'complete.json', 'episode-0000.json', 'ex1-0000.json',
                                     'update.json'])
            for item in entry['files']:
                frame = (p.parent / f'{item["path"]}.zst').read_bytes()
                self.assertEqual(replay_sample.sha_bytes(frame), item['zst_sha256'])
                self.assertEqual(replay_sample.sha_bytes(decompress(frame)), item['sha256'])
        # Rolling retention ran: old trajectories are gone, and the staged copies are what remains of them.
        gone = [r for r in records if not list(Path(chains.read(manifest)['segments'][r['segment']]['root'])
                                                .glob(f'iterations/{r["iteration"]:06d}/attempt-*/collect/episode-*'))]
        self.assertTrue(gone)
        progress = replay_sample.progress(self.node.root, 'X8', 'x8-1')
        self.assertEqual(progress['next_update'], len(records))
        self.assertEqual(progress['missing'], [])

    def test_unstaged_selected_files_survive_retention(self):
        manifest = self.plan['chains']['x8-1']['manifest']
        with mock.patch.object(replay_sample, 'stage_update', side_effect=OSError('disk full')):
            for _ in range(12):
                self.advance('x8-1', 2)
                report = self.cycle()
        self.assertIn('error', report['replay_sample']['x8-1'])
        _, records = retention.index(manifest)
        segs = chains.read(manifest)['segments']
        present = {r['update'] for r in records if list(Path(segs[r['segment']]['root']).glob(
            f'iterations/{r["iteration"]:06d}/attempt-*/collect/episode-*'))}
        chosen = self.selected_updates(records)
        self.assertTrue(chosen)
        self.assertTrue(set(chosen) <= present)  # every selected trajectory kept for the next staging
        self.assertLess(len(present), len(records))  # the others rolled off
        report = self.cycle()  # staging recovers and stages them
        self.assertEqual(report['replay_sample']['x8-1']['staged'], len(chosen))

    def test_the_verdict_reserves_the_selected_frames_it_may_admit(self):
        plan = chains.load_plan(self.path)
        base = chains.guard_reservation(plan, self.node.root)
        length, bpu = plan['arms']['X8']['segment_updates'], plan['arms']['X8']['bytes_per_update']
        expected = 0
        for name, seed in (('x8-1', 'w5/chain1'), ('x8-2', 'w5/chain2')):
            last = chains.GUARD_SEGMENTS * length - 1
            count = sum(replay_sample.selected(replay_sample.selection_hash(seed, u)) for u in range(last + 1))
            expected += count * int(bpu * replay_sample.FRAME_OVERHEAD + 65536)
        segments = sum((chains.GUARD_SEGMENTS + 1) * plan['arms'][c['arm']]['segment_updates'] *
                       plan['arms'][c['arm']]['bytes_per_update'] for c in plan['chains'].values())
        self.assertEqual(base, segments + 2 * 4 * plan['arms']['F']['segment_updates'] *
                         plan['arms']['F']['bytes_per_update'] + expected)

    def test_stage0_is_not_sampled(self):
        node = Node(tempfile.mkdtemp(dir=self.tmp.name))
        path, plan = node.stage0(c_wall=600.0)
        node.guard(ceilings={'x1': 4})
        node.drive(plan, 'x1')
        with contextlib.redirect_stdout(io.StringIO()), mock.patch.object(chains.subprocess, 'Popen'):
            report = chains.cycle(chains.load_plan(path), path)
        self.assertEqual(report['replay_sample'], {})


def fake_entry(staging, arm, chain, seed_root, update, size):
    """A staged entry of one selected update with an incompressible trajectory of `size` bytes."""
    seg = Path(staging).parent / 'segs' / chain
    d = seg / 'iterations' / f'{update:06d}'
    a = d / 'attempt-000000'
    upd = jwrite(a / 'update' / 'update.json', {'after_state_sha256': f'{update:064x}'})
    (a / 'collect').mkdir(parents=True, exist_ok=True)
    (a / 'collect' / 'episode-0000.json').write_text(json.dumps({'x': os.urandom(size // 2).hex()}))
    jwrite(d / 'complete.json', {'update': {'path': upd.as_posix()}})
    rec = {'update': update, 'boundary': update + 1, 'iteration': update, 'after_state_sha256': f'{update:064x}'}
    with mock.patch.object(replay_sample, 'staging_dir', lambda root: Path(staging)):
        return replay_sample.stage_update(None, arm, chain, seed_root, rec, seg, replay_sample.zstd_api())


def selected_updates(seed_root, count):
    out, u = [], 0
    while len(out) < count:
        if replay_sample.selected(replay_sample.selection_hash(seed_root, u)):
            out.append(u)
        u += 1
    return out


@unittest.skipUnless(HAS_ZSTD, 'no zstd (Python 3.14 or the zstandard package)')
class ShipTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.t = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def nodes(self, tag):
        """Two nodes' staging, chains 1 to 3 and 4 to 6 of X8, three selected updates each."""
        out = []
        for node, chains_ in (('n1', (1, 2, 3)), ('n2', (4, 5, 6))):
            staging = self.t / tag / node / 'replay-sample'
            for i in chains_:
                for u in selected_updates(f'w5/chain{i}', 3):
                    fake_entry(staging, 'X8', f'x8-{i}', f'w5/chain{i}', u, 3000)
            out.append(staging)
        return out

    def test_quota_keeps_the_smallest_hashes_whatever_the_order(self):
        a1, b1 = self.nodes('a')
        a2, b2 = self.nodes('b')
        sizes = [json.loads(p.read_text())['zst_bytes'] for p in a1.glob('*/*/u*/ENTRY.json')]
        quota = int(sum(sizes) * 0.8)  # binds on the first node's entries alone
        s1, s2 = self.t / 'store1', self.t / 'store2'
        with contextlib.redirect_stdout(io.StringIO()):
            replay_sample.ship(a1, s1, quota, 4 * quota)
            replay_sample.ship(b1, s1, quota, 4 * quota)
            replay_sample.ship(b2, s2, quota, 4 * quota)
            replay_sample.ship(a2, s2, quota, 4 * quota)
        kept1, cut1, _ = replay_sample.store_state(s1)
        kept2, cut2, _ = replay_sample.store_state(s2)
        self.assertEqual(sorted(kept1['X8']), sorted(kept2['X8']))
        self.assertEqual(cut1, cut2)
        # The kept set is the longest run, in hash order, of every offered update that fits the quota.
        offered = sorted((replay_sample.selection_hash(f'w5/chain{i}', u), ('X8', f'x8-{i}', u))
                         for i in range(1, 7) for u in selected_updates(f'w5/chain{i}', 3))
        sizes = {k: e['zst_bytes'] for k, e in kept1['X8'].items()}
        prefix = [k for _, k in offered][:len(sizes)]
        self.assertEqual(sorted(prefix), sorted(sizes))
        self.assertLessEqual(sum(sizes.values()), quota)
        self.assertEqual(cut1['X8'], offered[len(sizes)][0])
        # Staged copies are gone once their fate is in the manifest; the store verifies.
        self.assertFalse(list(a1.glob('*/*/u*')) + list(b2.glob('*/*/u*')))
        self.assertTrue(replay_sample.verify(s1)['ok'])
        self.assertEqual(sorted(p.parent.name for p in s1.glob('X8/*/u*/ENTRY.json')),
                         sorted(f'u{k[2]:09d}' for k in kept1['X8']))

    def test_a_frame_that_differs_at_the_destination_stays_staged(self):
        staging, _ = self.nodes('c')
        entry = sorted(staging.glob('*/*/u*/ENTRY.json'))[0]
        frame = next(entry.parent.rglob('*.zst'))
        good = frame.read_bytes()
        frame.write_bytes(good[:-1] + bytes([good[-1] ^ 1]))
        store = self.t / 'store'
        report = replay_sample.ship(staging, store)
        self.assertEqual(len(report['failed']), 1)
        self.assertTrue(entry.exists())
        frame.write_bytes(good)
        report = replay_sample.ship(staging, store)
        self.assertEqual((report['failed'], report['shipped']), ([], 1))
        self.assertTrue(replay_sample.verify(store)['ok'])

    def test_refusals_and_reruns(self):
        with self.assertRaisesRegex(replay_sample.SampleError, 'pass the total cap'):
            replay_sample.ship(self.t, self.t / 'store', 60 * 10**9, 100 * 10**9)
        staging, _ = self.nodes('d')
        entry = sorted(staging.glob('*/*/u*/ENTRY.json'))[0]
        kept = json.loads(entry.read_text())
        copy = self.t / 'again' / 'X8' / kept['chain'] / entry.parent.name
        shutil.copytree(entry.parent, copy)
        store = self.t / 'store'
        replay_sample.ship(staging, store)
        report = replay_sample.ship(self.t / 'again', store)  # the same update staged twice ships once
        self.assertEqual((report['duplicates'], report['shipped']), (1, 0))
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(replay_sample.main(['status', '--store', str(store)]), 0)
            self.assertEqual(replay_sample.main(['selected', 'w5/chain1', '0']), 0)


if __name__ == '__main__':
    unittest.main()
