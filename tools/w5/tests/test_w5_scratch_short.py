"""Scratch chains with segments shorter than 32 updates (chains.py, make_plan's scratch length rule), end to end with
the stand-in trainer (each state a checkpoint and a wide learner's container) and the bind mount simulated by a link.
For segment lengths 4, 8 and 16, against the same chain without scratch: every boundary's persisted records are the
same, and at every segment start (after supervise's rolling prune) the scratch chain holds exactly the states rolling
retention keeps, the plain chain those plus its last segment's other states, with the same hashes. Interruptions
just before and after the promotion and just before and after the commit line, and a failure inside a segment, end
in the same chain. The full-state stride is the real 64; the ladder stride is scaled to 128 (production 4,096), so
256 updates cross a 64-multiple leaving the recent window and two ladder states."""
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

LONG = 128  # scaled ladder and recent window (production 4,096); retention.FULL_STRIDE stays 64


class Crash(Exception):
    pass


@unittest.skipUnless(sys.platform.startswith('linux'), 'scratch segments are Linux only (rename onto an empty directory)')
class ShortScratchSegments(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.mounts = set()
        real_ismount = os.path.ismount
        patches = [mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X),
                                                'FAKE_SLEEP': '0.0', 'FAKE_LEARNER_STATE': '1'}),
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

    def held(self, plan):
        """(latest, boundary -> (checkpoint hash, container hash) for every state on disk). A state is its checkpoint
        and its container together: both present with their pinned hashes, or neither."""
        _, records = retention.index(plan['chains']['c1']['manifest'])
        out = {}
        for r in records:
            ck, st = Path(r['checkpoint']['path']), Path(r['state']['path'])
            self.assertEqual(ck.exists(), st.exists(), r['boundary'])
            if ck.exists():
                self.assertEqual((retention.sha(ck), retention.sha(st)), (r['checkpoint']['sha256'],
                                                                           r['state']['sha256']), r['boundary'])
                out[r['boundary']] = (r['checkpoint']['sha256'], r['state']['sha256'])
        return (records[-1]['boundary'] if records else 0), out

    def prune(self, node, plan):
        retention.apply(retention.plan(plan['chains']['c1']['manifest']), node.root / 'retention.jsonl')

    def drive(self, node, plan, wall, held=None):
        """Drives c1 with `chains.time` advanced one second per segment only (one clock across restarts). Before each
        segment, supervise's rolling prune; with `held`, what the chain then holds is recorded by its latest."""
        run_trainer = chains.run_trainer

        def run_segment(*args, **kwargs):
            self.prune(node, plan)
            if held is not None:
                latest, states = self.held(plan)
                held[latest] = states
            result = run_trainer(*args, **kwargs)
            wall[0] += 1.0
            return result

        with mock.patch.object(chains, 'time', wraps=time) as clock, \
                mock.patch.object(chains, 'run_trainer', side_effect=run_segment):
            clock.time.side_effect = lambda: wall[0]
            return node.drive(plan, 'c1')

    def finish(self, node, plan, held):
        """A last rolling prune (as supervise's next cycle), recorded like the others; returns the final latest."""
        self.prune(node, plan)
        latest, states = self.held(plan)
        held[latest] = states
        return latest

    def records(self, node, plan, ledger=True):
        """Every boundary's persisted receipts (complete.json, update.json, collection.json), with the node's own
        directory replaced by a placeholder and the ledger's wall-clock time dropped."""
        _, records = retention.index(plan['chains']['c1']['manifest'])
        man = json.loads(Path(plan['chains']['c1']['manifest']).read_text())
        out = []
        for r in records:
            it = Path(man['segments'][r['segment']]['root']) / 'iterations' / f'{r["iteration"]:06d}'
            complete = json.loads((it / 'complete.json').read_text())
            att = chains.attempt_dir(it, complete)
            # update.json names its node's paths, so its pin differs between roots: checked here, then compared as
            # the document it pins.
            self.assertEqual(complete['update'].pop('sha256'), retention.sha(att / 'update' / 'update.json'))
            docs = [complete, json.loads((att / 'update' / 'update.json').read_text()),
                    json.loads((att / 'collect' / 'collection.json').read_text())]
            doc = json.loads(json.dumps(docs, sort_keys=True).replace(node.t.as_posix(), '<node>'))
            doc[0].get('chain_ledger', {}).pop('unix_seconds', None)
            if not ledger:
                doc[0].pop('chain_ledger', None)
            out.append((r['boundary'], doc))
        return out

    def kept(self, latest):
        return {b for b in range(1, latest + 1) if retention.keeps_full_state('C', b, latest)} | {latest}

    def check_held(self, length, s_held, p_held):
        """At every segment start and at the end: the scratch chain holds exactly the retention-kept states; the
        plain chain holds them and its last segment's other states (not yet prunable); the hashes agree."""
        self.assertEqual(sorted(s_held), sorted(p_held))
        for latest in sorted(s_held):
            if not latest:
                continue
            kept = self.kept(latest)
            self.assertEqual(set(s_held[latest]), kept, latest)
            self.assertEqual(set(p_held[latest]), kept | set(range(latest - length + 1, latest + 1)), latest)
            self.assertEqual({b: s_held[latest][b] for b in kept}, {b: p_held[latest][b] for b in kept}, latest)

    def plain(self, length, segments):
        node, plan, wall = self.chain(f'plain{length}', length, False, segments)
        held = {}
        self.assertEqual(self.drive(node, plan, wall, held), 0)
        self.finish(node, plan, held)
        return node, plan, held

    def test_every_boundary_and_retention_decision_matches_the_plain_chain(self):
        updates = 2 * LONG
        for length in (4, 8, 16):
            with self.subTest(length=length):
                segments = updates // length
                p_node, p_plan, p_held = self.plain(length, segments)
                s_node, s_plan, s_wall = self.chain(f'scratch{length}', length, True, segments)
                s_held = {}
                self.assertEqual(self.drive(s_node, s_plan, s_wall, s_held), 0)
                self.assertEqual(self.finish(s_node, s_plan, s_held), updates)
                self.assertEqual(self.records(s_node, s_plan), self.records(p_node, p_plan))  # every boundary
                self.check_held(length, s_held, p_held)
                # The decisions crossed what they must: a 64-multiple kept in the recent window and later pruned
                # (64 leaves at latest 192), and both ladder states (128, 256) kept to the end.
                self.assertIn(64, s_held[LONG])
                self.assertNotIn(64, s_held[updates])
                self.assertTrue({LONG, 2 * LONG} <= set(s_held[updates]))
                tree = s_node.root / 'chains' / 'c1'
                self.assertFalse(list(tree.rglob('episode-*')))  # no trajectory persisted
                self.assertFalse(any(p.is_symlink() for p in tree.iterdir()))
                self.assertEqual(len(list(tree.glob('seg-*'))), segments)

    def test_interruptions_around_promotion_and_the_commit_line(self):
        """Segment 15 of a 4-update chain ends at 64, a kept state. The driver is interrupted while persisting it:
        with the staging copy marked but the segment still mounted, after the unmount just before the promotion
        (rename), after the promotion just before the commit line, and just after the commit line. A new driver
        finishes the chain; every boundary, retention decision and commit matches the uninterrupted plain chain, and
        segment 15 holds its checkpoint and container under its marker's pins."""
        length, segments, k = 4, 20, 15
        seg = f'seg-{k:05d}'
        p_node, p_plan, p_held = self.plain(length, segments)
        real_rename, real_commit, real_compress = os.rename, chains.record_commit, chains.compress_config
        armed = []

        def unmount(native, tries=20):
            if armed and Path(native).name == seg and armed[0] == 'before-unmount':
                raise Crash(armed.pop())
            return self.unlink(native, tries)

        def rename(src, dst, *args, **kwargs):
            if armed and Path(src).name == f'.{seg}.persist' and armed[0] == 'before-promotion':
                raise Crash(armed.pop())
            return real_rename(src, dst, *args, **kwargs)

        def record_commit(cdir, man, s, how, *args, **kwargs):
            if armed and s == k and armed[0] == 'after-promotion':
                raise Crash(armed.pop())
            return real_commit(cdir, man, s, how, *args, **kwargs)

        def compress_config(path):
            if armed and Path(path).name == f'{seg}.json' and armed[0] == 'after-commit':
                raise Crash(armed.pop())
            return real_compress(path)

        expected = {'before-unmount': 'scratch-late', 'before-promotion': 'scratch-swap',
                    'after-promotion': 'recovered', 'after-commit': 'scratch'}
        for point, how in expected.items():
            with self.subTest(point), mock.patch.object(chains, 'unmount', side_effect=unmount), \
                    mock.patch('os.rename', side_effect=rename), \
                    mock.patch.object(chains, 'record_commit', side_effect=record_commit), \
                    mock.patch.object(chains, 'compress_config', side_effect=compress_config):
                node, plan, wall = self.chain(f'crash-{point}', length, True, segments)
                held = {}
                armed[:] = [point]
                with self.assertRaises(Crash):
                    self.drive(node, plan, wall, held)
                self.assertEqual(armed, [])  # the interruption happened, at segment 15's persist
                self.assertEqual(self.drive(node, plan, wall, held), 0)
                self.assertEqual(self.finish(node, plan, held), segments * length)
                self.assertEqual(self.records(node, plan), self.records(p_node, p_plan))
                self.check_held(length, held, p_held)
                cdir = node.root / 'chains' / 'c1'
                commits = [json.loads(line) for line in (cdir / chains.COMMIT_LOG).read_text().splitlines()]
                commits = [c for c in commits if c.get('event') == 'commit']
                self.assertEqual([c['segment'] for c in commits], list(range(segments)))  # once each, in order
                self.assertEqual(commits[k]['how'], how)
                self.assertEqual(commits[k]['boundary'], (k + 1) * length)
                chains.validate_scratch_stage(cdir / seg)  # checkpoint and container under the marker's pins
                self.assertIn((k + 1) * length, held[segments * length])  # 64 is still kept at the end
                self.assertFalse((cdir / f'.{seg}.persist').exists())

    def test_restart_inside_an_unfinished_short_segment(self):
        length, segments = 4, 26
        p_node, p_plan, p_held = self.plain(length, segments)
        # The scratch chain fails after two of segment 10's four updates; its second driver resets the segment and
        # reruns it from segment 9's persisted end. The failed segment used one clock second, so the wall has one more.
        node, plan, wall = self.chain('scratch', length, True, segments + 1)
        held = {}
        with mock.patch.dict(os.environ, {'FAKE_FAIL_AT': str(10 * length + 2)}):
            self.assertEqual(self.drive(node, plan, wall, held), chains.EXIT_FAILED)
        seg = node.root / 'chains' / 'c1' / 'seg-00010'
        self.assertIn(seg, self.mounts)
        self.assertEqual(chains.completed(seg), 2)
        self.assertEqual(self.drive(node, plan, wall, held), 0)
        self.assertEqual(self.finish(node, plan, held), segments * length)
        # The two discarded updates count in the CPU ledger, so the ledger is left out of this comparison.
        self.assertEqual(self.records(node, plan, ledger=False), self.records(p_node, p_plan, ledger=False))
        self.check_held(length, held, p_held)
        rows = [json.loads(line) for line in (node.root / 'LOG.jsonl').read_text().splitlines()]
        resets = [r for r in rows if r['event'] == 'scratch-reset']
        self.assertEqual([(r['segment'], r['discarded_iterations']) for r in resets], [('seg-00010', 2)])
        commits = [json.loads(line) for line in (node.root / 'chains' / 'c1' / chains.COMMIT_LOG).read_text()
                   .splitlines()]
        self.assertEqual([c['segment'] for c in commits if c.get('event') == 'commit'], list(range(segments)))


if __name__ == '__main__':
    unittest.main()
