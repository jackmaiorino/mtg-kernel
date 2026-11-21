"""Scratch chains with segments shorter than 32 updates (chains.py, make_plan's scratch length rule), end to end with
the stand-in trainer and the bind mount simulated by a link: the scratch chain reaches the same after-states as the
same chain without scratch, keeps exactly the states rolling retention keeps (every ladder state among them) while
the plain chain also holds its last segment's other checkpoints, and a restart inside an unfinished segment reruns
that segment from the last persisted end to the same states. Retention's strides are scaled down (full-state stride
16, ladder 32) so a short run crosses both; a segment length of 4 or 8 relates to them as 4, 8 or 16 does to 64."""
import json
import os
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE))
import chains  # noqa: E402
import retention  # noqa: E402
from test_w5_chains import Node, STEP_C, STEP_X, jwrite  # noqa: E402

FULL, LONG = 16, 32  # scaled retention strides (production: 64 and 4,096)


@unittest.skipUnless(sys.platform.startswith('linux'), 'scratch segments are Linux only (rename onto an empty directory)')
class ShortScratchSegments(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.mounts = set()
        real_ismount = os.path.ismount
        patches = [mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X),
                                                'FAKE_SLEEP': '0.0'}),
                   mock.patch.object(retention, 'FULL_STRIDE', FULL),
                   mock.patch.object(retention, 'C_LONG_STRIDE', LONG),
                   mock.patch.object(retention, 'C_RECENT', LONG),
                   # The simulated bind mount: the segment path is a link to its scratch directory while "mounted".
                   mock.patch('os.path.ismount', side_effect=lambda p: Path(p) in self.mounts or real_ismount(p)),
                   mock.patch.object(chains, 'scratch_link', side_effect=self.link),
                   mock.patch.object(chains, 'unmount', side_effect=self.unlink)]
        for p in patches:
            p.start()
            self.addCleanup(p.stop)

    def link(self, arm, cdir, native):
        target = chains.scratch_dir(arm, cdir, native)
        target.mkdir(parents=True, exist_ok=True)
        native = Path(native)
        if native in self.mounts:
            return
        if native.exists():
            self.assertFalse(any(native.iterdir()), f'{native} is not empty')
            native.rmdir()
        native.parent.mkdir(parents=True, exist_ok=True)
        native.symlink_to(target, target_is_directory=True)
        self.mounts.add(native)

    def unlink(self, native, tries=20):
        native = Path(native)
        if native in self.mounts:
            native.unlink()
            native.mkdir()  # the empty directory an unmount leaves
            self.mounts.discard(native)

    def chain(self, name, length, scratch, segments):
        """A Stage 0 independent plan with one C chain on seed root oc/c1 in its own node root."""
        (Path(self.tmp.name) / name).mkdir()
        node = Node(Path(self.tmp.name) / name)
        inputs = json.loads(Path(node.inputs('0', {'c1': {'arm': 'C', 'slot': 0, 'seed_root': 'oc/c1'}},
                                             label='short', c_wall_seconds=segments - 0.5,
                                             independent=True)).read_text())
        inputs['arms']['C'] = dict(inputs['arms']['C'], segment_updates=length)
        if scratch:
            inputs['scratch_root'] = (node.t / 'shm').as_posix()
            inputs['arms']['C']['scratch'] = True
        _, plan = chains.make_plan(jwrite(node.t / 'inputs-short.json', inputs))
        node.guard(ceilings={'c1': 10**6})
        return node, plan, [time.time()]

    def drive(self, node, plan, wall):
        """Drives c1 with `chains.time` advanced one second per segment only (one clock across restarts), and the
        rolling retention supervise applies run before each segment, so prunes happen as the chain grows."""
        manifest = plan['chains']['c1']['manifest']
        run_trainer = chains.run_trainer

        def run_segment(*args, **kwargs):
            retention.apply(retention.plan(manifest), node.root / 'retention.jsonl')
            result = run_trainer(*args, **kwargs)
            wall[0] += 1.0
            return result

        with mock.patch.object(chains, 'time', wraps=time) as clock, \
                mock.patch.object(chains, 'run_trainer', side_effect=run_segment):
            clock.time.side_effect = lambda: wall[0]
            return node.drive(plan, 'c1')

    def finish(self, plan, root):
        """A last rolling prune (as supervise's next cycle), then (after-states, boundaries with a full state on
        disk and their checkpoint hashes, latest)."""
        manifest = plan['chains']['c1']['manifest']
        retention.apply(retention.plan(manifest), Path(root) / 'retention.jsonl')
        _, records = retention.index(manifest)
        present = {r['boundary']: retention.sha(r['checkpoint']['path']) for r in records
                   if Path(r['checkpoint']['path']).exists()}
        return [r['after_state_sha256'] for r in records], present, records[-1]['boundary']

    def kept(self, latest):
        return {b for b in range(1, latest + 1) if retention.keeps_full_state('C', b, latest)} | {latest}

    def test_short_scratch_keeps_what_the_plain_chain_keeps(self):
        for length in (4, 8):
            with self.subTest(length=length):
                segments = 104 // length  # 104 updates: three ladder states (32, 64, 96), a recent window, a tail
                plain, pplan, pwall = self.chain(f'plain{length}', length, False, segments)
                scratch, splan, swall = self.chain(f'scratch{length}', length, True, segments)
                self.assertEqual(self.drive(plain, pplan, pwall), 0)
                self.assertEqual(self.drive(scratch, splan, swall), 0)
                p_states, p_present, p_latest = self.finish(pplan, plain.root)
                s_states, s_present, s_latest = self.finish(splan, scratch.root)
                self.assertEqual((p_latest, s_latest), (104, 104))
                self.assertEqual(s_states, p_states)  # every after-state
                kept = self.kept(104)
                self.assertTrue({32, 64, 96} <= kept)
                self.assertEqual(set(s_present), kept)  # exactly the retention-kept states
                self.assertEqual(set(p_present), kept | set(range(104 - length + 1, 105)))  # plus its last segment
                self.assertEqual({b: s_present[b] for b in kept}, {b: p_present[b] for b in kept})
                # Persisted ends only: no trajectory and no mid-segment checkpoint survived the scratch run.
                tree = scratch.root / 'chains' / 'c1'
                self.assertFalse(list(tree.rglob('episode-*')))
                self.assertFalse(any(p.is_symlink() for p in tree.iterdir()))
                self.assertEqual(len(list(tree.glob('seg-*'))), segments)

    def test_restart_inside_an_unfinished_short_segment(self):
        length, segments = 4, 26
        plain, pplan, pwall = self.chain('plain', length, False, segments)
        self.assertEqual(self.drive(plain, pplan, pwall), 0)
        p_states, p_present, _ = self.finish(pplan, plain.root)
        # The scratch chain fails after two of segment 10's four updates; its second driver resets the segment and
        # reruns it from segment 9's persisted end. The failed segment used one clock second, so the wall has one more.
        scratch, splan, swall = self.chain('scratch', length, True, segments + 1)
        with mock.patch.dict(os.environ, {'FAKE_FAIL_AT': str(10 * length + 2)}):
            self.assertEqual(self.drive(scratch, splan, swall), chains.EXIT_FAILED)
        seg = scratch.root / 'chains' / 'c1' / 'seg-00010'
        self.assertIn(seg, self.mounts)
        self.assertEqual(chains.completed(seg), 2)
        self.assertEqual(self.drive(scratch, splan, swall), 0)
        s_states, s_present, s_latest = self.finish(splan, scratch.root)
        self.assertEqual(s_latest, segments * length)
        self.assertEqual(s_states, p_states)
        self.assertEqual(set(s_present), self.kept(s_latest))
        self.assertEqual(s_present, {b: p_present[b] for b in self.kept(s_latest)})
        rows = [json.loads(line) for line in (scratch.root / 'LOG.jsonl').read_text().splitlines()]
        resets = [r for r in rows if r['event'] == 'scratch-reset']
        self.assertEqual([(r['segment'], r['discarded_iterations']) for r in resets], [('seg-00010', 2)])
        commits = [json.loads(line) for line in (scratch.root / 'chains' / 'c1' / chains.COMMIT_LOG).read_text()
                   .splitlines()]
        self.assertEqual(sorted(c['segment'] for c in commits if c.get('event') == 'commit'), list(range(segments)))


if __name__ == '__main__':
    unittest.main()
