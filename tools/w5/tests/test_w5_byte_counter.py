"""The byte guard's counter for `supervise --loop` (retention.ByteCounter): over scripted chain histories (rolling
retention, a pair's PRUNE moves into the hold folder) it returns exactly what walking the whole root returns; a
deletion outside the supervisor is corrected at the next full recount and reported; and on a large tree of
finished segments its steady call does not walk them."""
import contextlib
import io
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
from test_w5_chains import Node, STEP_C, STEP_X  # noqa: E402


class CounterTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.node = Node(self.tmp.name)
        for patch in (mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X)}),
                      mock.patch.object(chains, 'GUARD_WAIT', 0.0), mock.patch.object(chains, 'GUARD_POLL', 0.0),
                      mock.patch.object(retention, 'TRAJECTORY_SEGMENTS', 2),
                      mock.patch.object(retention, 'FULL_STRIDE', 4)):
            patch.start()
            self.addCleanup(patch.stop)

    def tearDown(self):
        self.tmp.cleanup()

    def cycle(self, path, counter, caches):
        with contextlib.redirect_stdout(io.StringIO()), mock.patch.object(chains.subprocess, 'Popen'):
            return chains.cycle(chains.load_plan(path), path, caches=caches, counter=counter)

    def test_equal_to_walking_everything_over_a_history_with_prunes(self):
        path, plan = self.node.stage0(c_wall=600.0, r_nom_usec=10_000_000)
        counter, caches = retention.ByteCounter(), {}
        root = self.node.root
        for step, burst in enumerate((1, 2, 1, 3, 2, 1, 2)):
            for name in ('c1', 'x1'):
                self.node.guard(ceilings={name: chains.admitted(root / 'chains' / name) + burst})
                self.assertEqual(self.node.drive(plan, name), chains.EXIT_ROOM)
            report = self.cycle(path, counter, caches)
            self.assertEqual(counter.total([root]), retention.tree_bytes(root), f'step {step}')
            self.assertGreater(sum(r['removed'] for r in report['retention'].values()), -1)
        self.assertTrue(counter.sealed)
        # The pair ends: C at its last boundary, X stops there; selection and PRUNE move files out of finished
        # segments into the hold folder.
        _, c_records = retention.index(plan['chains']['c1']['manifest'])
        chains.write_once(root / 'chains' / 'c1' / 'END.json', chains.end_doc(
            'c1', 'C', 'c-wall', c_records[-1]['boundary'], None, 'test'))
        self.node.ledger('c1', 'open', 'close')
        _, x_records = retention.index(plan['chains']['x1']['manifest'])
        chains.write_once(root / 'chains' / 'x1' / 'END.json', chains.end_doc(
            'x1', 'X8', 'chain-failure', x_records[-1]['boundary'], None, 'test'))
        with mock.patch.object(chains, 'r_i', lambda root, c: c_records[-1]['ledger_usec']):
            self.cycle(path, counter, caches)
        self.assertTrue((root / 'hold' / 'MOVES.jsonl').exists())
        self.assertEqual(counter.total([root]), retention.tree_bytes(root))

    def test_an_outside_deletion_is_corrected_at_the_full_recount(self):
        path, plan = self.node.stage0(c_wall=600.0)
        root = self.node.root
        self.node.guard(ceilings={'c1': 3})
        self.node.drive(plan, 'c1')
        counter = retention.ByteCounter(full_seconds=3600)
        t0 = time.time()
        self.assertEqual(counter.total([root], t0), retention.tree_bytes(root))
        victim = next((root / 'chains' / 'c1' / 'seg-00000').rglob('episode-*.json'))
        size = victim.stat().st_size
        victim.unlink()  # not through the supervisor: the cached total is now larger than the truth
        self.assertEqual(counter.total([root], t0 + 1), retention.tree_bytes(root) + size)
        self.assertEqual(counter.total([root], t0 + 3601), retention.tree_bytes(root))
        self.assertEqual(len(counter.mismatches), 1)
        # A deletion reported to it is counted again at once; a removed segment drops out.
        victim = next((root / 'chains' / 'c1' / 'seg-00001').rglob('episode-*.json'))
        victim.unlink()
        counter.invalidate([victim])
        self.assertEqual(counter.total([root], t0 + 3602), retention.tree_bytes(root))
        import shutil
        shutil.rmtree(root / 'chains' / 'c1' / 'seg-00001')
        self.assertEqual(counter.total([root], t0 + 3603), retention.tree_bytes(root))
        self.assertNotIn(retention.segment_key(root / 'chains' / 'c1' / 'seg-00001'), counter.sealed)


class LargeTreeTests(unittest.TestCase):
    SEGMENTS, FILES = 400, 120

    def test_steady_call_skips_finished_segments(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t) / 'node'
            for s in range(self.SEGMENTS + 1):
                seg = root / 'chains' / 'big' / f'seg-{s:05d}'
                for i in range(self.FILES):
                    d = seg / 'iterations' / f'{i:06d}'
                    d.mkdir(parents=True)
                    (d / 'complete.json').write_text('x' * (i % 7 + 1))
                if s < self.SEGMENTS:
                    (seg / 'completion.json').write_text('{}')
            counter = retention.ByteCounter()
            counter.total([root])
            t0 = time.perf_counter()
            walked = retention.tree_bytes(root)
            t1 = time.perf_counter()
            counted = counter.total([root])
            t2 = time.perf_counter()
            self.assertEqual(counted, walked)
            timing = {'files': self.SEGMENTS * self.FILES, 'walk_seconds': round(t1 - t0, 3),
                      'counter_seconds': round(t2 - t1, 3)}
            print(json.dumps(timing), file=sys.stderr)
            self.assertLess(t2 - t1, 0.25 * (t1 - t0), timing)


if __name__ == '__main__':
    unittest.main()
