"""The in-memory index cache of `chains.py supervise --loop` (retention.IndexCache): over scripted chain histories it
returns the same records as `retention.index`, the same rolling plans and byte guard reports as reading everything,
and a supervise loop with it leaves the same files and reports as one without; on a large synthetic index its
steady cycle reads only what is new."""
import contextlib
import io
import json
import os
import shutil
import subprocess
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
from test_w5_chains import CONTAINER, Node, STEP_C, STEP_X, jwrite  # noqa: E402

SMALL = {'FULL_STRIDE': 4, 'C_LONG_STRIDE': 16, 'C_RECENT': 16, 'TRAJECTORY_SEGMENTS': 3}


def small_strides(test):
    """Strides small enough that a test chain crosses every retention rule (the rules read these at call time)."""
    for name, value in SMALL.items():
        patch = mock.patch.object(retention, name, value)
        patch.start()
        test.addCleanup(patch.stop)


class IndexCacheTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.node = Node(self.tmp.name)
        for patch in (mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X)}),
                      mock.patch.object(chains, 'GUARD_WAIT', 0.0), mock.patch.object(chains, 'GUARD_POLL', 0.0)):
            patch.start()
            self.addCleanup(patch.stop)
        small_strides(self)

    def tearDown(self):
        self.tmp.cleanup()

    def advance(self, plan, name, segments):
        """Runs the chain's driver for `segments` more segments (the guard ceiling admits exactly those)."""
        cdir = self.node.root / 'chains' / name
        self.node.guard(ceilings={name: chains.admitted(cdir) + segments})
        self.assertEqual(self.node.drive(plan, name), chains.EXIT_ROOM)

    def test_refresh_returns_the_index_records(self):
        _, plan = self.node.stage0(c_wall=600.0)
        path = plan['chains']['c1']['manifest']
        cache = retention.IndexCache(path)
        self.assertEqual(cache.refresh()[1], [])
        for step in range(6):
            self.advance(plan, 'c1', 1 + step % 2)
            if step % 2:  # the file's other writer goes first: the cache reads its appended lines
                self.assertEqual(cache.refresh()[1], retention.index(path)[1])
            else:  # the cache scans the receipts and appends; index() then reads the same file
                got = cache.refresh()[1]
                self.assertEqual(got, retention.index(path)[1])
            self.assertEqual([r['boundary'] for r in got], list(range(1, len(got) + 1)))
        # A replaced cache file (as the S-container migration writes it) is read whole again.
        index_file = retention.index_path(path)
        lines = index_file.read_text().splitlines()
        index_file.unlink()
        index_file.write_text(''.join(line + '\n' for line in lines[:3]))
        self.assertEqual(cache.refresh()[1], retention.index(path)[1])
        self.assertEqual(len(cache.refresh()[1]), len(lines))  # the receipts past the kept lines are found again

    def test_a_legacy_index_restored_under_an_initialized_cache(self):
        """A cache file without S-container pins (as written before they were tracked) replaces the current one:
        the cache migrates it as index() does, with no handle of its own open (Windows refuses the replace)."""
        _, plan = self.node.stage0(c_wall=600.0)
        path = plan['chains']['c1']['manifest']
        self.advance(plan, 'c1', 2)
        cache = retention.IndexCache(path)
        cache.refresh()
        index_file = retention.index_path(path)
        legacy = [{k: v for k, v in json.loads(x).items() if k != 'state'} for x in index_file.read_text().splitlines()]
        index_file.unlink()
        index_file.write_text(''.join(json.dumps(r) + '\n' for r in legacy))
        got = cache.refresh()[1]
        self.assertTrue(all('state' in r for r in got))
        self.assertEqual(got, retention.index(path)[1])
        # Appended legacy lines past the cursor take the same path.
        self.advance(plan, 'c1', 1)
        retention.index(path)
        lines = index_file.read_text().splitlines()
        last = {k: v for k, v in json.loads(lines[-1]).items() if k != 'state'}
        index_file.write_text(''.join(x + '\n' for x in lines[:-1]) + json.dumps(last) + '\n')
        self.assertEqual(cache.refresh()[1], retention.index(path)[1])

    def test_writers_take_turns_and_never_append_twice(self):
        """index() in another process waits while a refresh holds the index lock, so it reads the refresh's lines
        instead of appending the same records; a line appended outside the lock while the cache scans makes the
        cache append nothing and read the file whole."""
        _, plan = self.node.stage0(c_wall=600.0)
        path = plan['chains']['c1']['manifest']
        self.advance(plan, 'c1', 1)
        cache = retention.IndexCache(path)
        self.assertEqual(cache.refresh()[1], retention.index(path)[1])
        self.advance(plan, 'c1', 1)
        real, other = retention.scan, []
        code = ('import sys; sys.path.insert(0, sys.argv[1]); import retention; '
                'print(len(retention.index(sys.argv[2])[1]))')

        def scanning(chain, known):
            new = real(chain, known)
            if not other:  # a second writer starts while this refresh holds the lock, and must wait for it
                other.append(subprocess.Popen([sys.executable, '-c', code, str(HERE.parent), str(path)],
                                              stdout=subprocess.PIPE, text=True))
                time.sleep(1.0)
                self.assertIsNone(other[0].poll())
            return new
        with mock.patch.object(retention, 'scan', scanning):
            got = cache.refresh()[1]
        out, _ = other[0].communicate(timeout=60)
        self.assertEqual(int(out), len(got))
        lines = retention.index_path(path).read_text().splitlines()
        self.assertEqual([json.loads(x)['boundary'] for x in lines], list(range(1, len(lines) + 1)))
        # A writer that ignores the lock appends while the cache scans: the cache appends nothing and reads whole.
        self.advance(plan, 'c1', 1)
        raced = []

        def racing(chain, known):
            new = real(chain, known)
            if not raced:
                raced.append(1)
                with open(retention.index_path(path), 'ab') as f:
                    f.write(b''.join(json.dumps(r).encode() + b'\n' for r in new))
            return new
        with mock.patch.object(retention, 'scan', racing):
            self.assertEqual(cache.refresh()[1], retention.index(path)[1])
        lines = retention.index_path(path).read_text().splitlines()
        self.assertEqual([json.loads(x)['boundary'] for x in lines], list(range(1, len(lines) + 1)))
        self.advance(plan, 'c1', 1)
        self.assertEqual(cache.refresh()[1], retention.index(path)[1])

    def test_a_last_line_without_its_newline(self):
        """An interrupted append can leave a complete record without its newline, which index() reads: the cache
        reads it the same way, and no later append joins it."""
        _, plan = self.node.stage0(c_wall=600.0)
        path = plan['chains']['c1']['manifest']
        self.advance(plan, 'c1', 1)
        cache = retention.IndexCache(path)
        cache.refresh()
        self.advance(plan, 'c1', 1)
        retention.index(path)
        index_file = retention.index_path(path)
        index_file.write_bytes(index_file.read_bytes().rstrip(b'\n'))
        cache.settle([1])
        got = cache.refresh()[1]
        self.assertEqual(got, retention.index(path)[1])
        # The same cycle's plan sees every unsettled record of that whole read.
        self.assertEqual([r['boundary'] for r in cache.active], [r['boundary'] for r in got][1:])
        self.advance(plan, 'c1', 1)
        for _ in range(2):
            self.assertEqual(cache.refresh()[1], retention.index(path)[1])
        lines = index_file.read_bytes().split(b'\n')
        self.assertEqual(lines[-1], b'')
        self.assertEqual([json.loads(x)['boundary'] for x in lines[:-1]], list(range(1, len(lines))))
        # index() itself no longer joins a new line to an unterminated one.
        index_file.write_bytes(index_file.read_bytes().rstrip(b'\n'))
        self.advance(plan, 'c1', 1)
        records = retention.index(path)[1]
        self.assertEqual([r['boundary'] for r in records], list(range(1, len(records) + 1)))
        self.assertEqual(cache.refresh()[1], records)

    def history(self, plan, name, snapshot_at=None, binary=None):
        """A scripted history: segments in irregular bursts, a rolling prune after most of them, a snapshot window
        opening part way; at every step the cached plan and guard equal the uncached ones."""
        path = plan['chains'][name]['manifest']
        cache = retention.IndexCache(path)
        caches = {Path(path).as_posix(): cache}
        manifests = [c['manifest'] for c in plan['chains'].values() if c['manifest']]
        log = self.node.t / f'{name}-retention.jsonl'
        snapshot_from, compared = None, 0
        for step, burst in enumerate((1, 2, 1, 3, 1, 1, 2, 4, 1, 2, 1, 1)):
            self.advance(plan, name, burst)
            _, records = retention.index(path)
            if snapshot_at and snapshot_from is None and records[-1]['boundary'] >= snapshot_at:
                snapshot_from = snapshot_at
            if snapshot_from is not None and binary:
                retention.snapshot(path, binary, snapshot_from, max_new=2)
            uncached = retention.plan(path, snapshot_from)
            cached = retention.plan(path, snapshot_from, cache)
            self.assertEqual(cached, uncached, f'step {step}')
            g_u = retention.guard([self.node.root], 10**12, 0, manifests)
            g_c = retention.guard([self.node.root], 10**12, 0, manifests, caches)
            self.assertEqual(g_c, g_u, f'step {step}')
            if step % 4 != 3:  # some cycles skip the prune: their files stay for the next plan
                retention.apply(uncached, log)
            compared += 1
        self.assertGreater(len(cache.settled), 0)
        self.assertGreater(len(cache.swept), 0)
        self.assertLess(len(cache.active), len(retention.index(path)[1]))
        return compared

    def test_c_chain_plans_and_guard_equal_over_a_history(self):
        _, plan = self.node.stage0(c_wall=600.0)
        self.assertEqual(self.history(plan, 'c1'), 12)

    def test_x_chain_plans_and_guard_equal_with_a_snapshot_window(self):
        _, plan = self.node.stage0(c_wall=600.0)
        os.chmod(CONTAINER, 0o755)
        binary = str(CONTAINER)
        if os.name == 'nt':  # Windows runs the stand-in container tool through a command file
            binary = self.node.t / 'container.cmd'
            binary.write_text(f'@"{sys.executable}" "{CONTAINER}" %*\n')
            binary = str(binary)
        self.assertEqual(self.history(plan, 'x1', snapshot_at=20, binary=binary), 12)

    def test_supervise_loop_with_the_cache_matches_reading_everything(self):
        """Twin nodes run the same scripted history; one supervise keeps caches across cycles, the other reads
        everything each cycle: the same retention reports, look boundaries and surviving files."""
        twin = Node(tempfile.mkdtemp(dir=self.tmp.name))
        specs = {'c1': {'arm': 'C', 'slot': 0, 'seed_root': 'w5/q1'},
                 'x1': {'arm': 'X8', 'slot': 1, 'seed_root': 'w5/q2'}}
        runs = []
        for node in (self.node, twin):
            path, plan = node.plan('0', dict(specs), c_wall_seconds=600.0, r_nom_usec=60 * STEP_C)
            runs.append((node, path, plan))
        caches = {}
        with contextlib.redirect_stdout(io.StringIO()):
            for burst in (2, 1, 3, 1, 2, 4, 1):
                reports = []
                for node, path, plan in runs:
                    for name in ('c1', 'x1'):
                        cdir = node.root / 'chains' / name
                        node.guard(ceilings={'c1': chains.admitted(node.root / 'chains' / 'c1') + burst,
                                             'x1': chains.admitted(node.root / 'chains' / 'x1') + burst})
                        self.assertEqual(node.drive(plan, name), chains.EXIT_ROOM)
                        self.assertTrue(cdir.exists())
                    with mock.patch.object(chains.subprocess, 'Popen'):  # no fills: nothing has ended
                        report = chains.cycle(chains.load_plan(path), path,
                                              caches=caches if node is self.node else None)
                    reports.append(report)
                self.assertEqual(reports[0]['retention'], reports[1]['retention'])
                self.assertEqual(reports[0]['guard']['ok'], reports[1]['guard']['ok'])
        for name in ('c1', 'x1'):
            files = []
            for node, _, _ in runs:
                base = node.root / 'chains' / name
                files.append(sorted(p.relative_to(base).as_posix() for p in base.rglob('*')
                                    if p.is_file() and 'LOOK' not in p.name and not p.name.endswith('.jsonl')))
                self.assertTrue((base / 'LOOK.json').exists())
            self.assertEqual(files[0], files[1])
            looks = [json.loads((n.root / 'chains' / name / 'LOOK.json').read_text())['boundary'] for n, _, _ in runs]
            self.assertEqual(looks[0], looks[1])
        self.assertTrue(caches)
        shutil.rmtree(twin.t, ignore_errors=True)


class LargeIndexTests(unittest.TestCase):
    """A C chain of 200,192 updates whose old states and trajectories are gone, as rolling retention leaves it
    (records only: no per-update receipts on disk except a new last segment)."""

    SEGMENTS, LENGTH = 782, 256
    UPDATES = SEGMENTS * LENGTH

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        t = Path(self.tmp.name)
        segs = []
        for s in range(self.SEGMENTS + 1):  # the last segment is the live one
            root = t / 'chain' / f'seg-{s:05d}'
            if s < self.SEGMENTS:
                root.mkdir(parents=True)
                (root / 'completion.json').write_text('{}')
            segs.append({'config': (t / 'unused.json').as_posix(), 'offset': s * self.LENGTH,
                         'root': root.as_posix(), 'updates': self.LENGTH})
        self.path = jwrite(t / 'chain' / 'chain.json', {'schema': 'w5-chain/v1', 'chain': 'big', 'arm': 'C',
                                                        'launched_unix': 1.0, 'segments': segs})
        with open(retention.index_path(self.path), 'w', encoding='utf-8') as f:
            for u in range(self.UPDATES):
                s, i = divmod(u, self.LENGTH)
                f.write(json.dumps({'update': u, 'boundary': u + 1, 'segment': s, 'iteration': i,
                                    'after_state_sha256': f'{u:064x}', 'checkpoint': {
                                        'path': f'{segs[s]["root"]}/iterations/{i:06d}/u/checkpoint.json',
                                        'sha256': f'{u:064x}'}, 'state': None, 'ledger_usec': (u + 1) * 500_000,
                                    'unix_seconds': 1.0 + u * 0.5, 'instance': 0}) + '\n')
        self.segs = segs

    def tearDown(self):
        self.tmp.cleanup()

    def add_updates(self, count):
        """New receipts in the last (live) segment, as the driver leaves them."""
        seg = self.segs[-1]
        known = sum(1 for _ in Path(seg['root']).glob('iterations/*/complete.json')) if Path(seg['root']).exists() \
            else 0
        for i in range(known, known + count):
            d = Path(seg['root']) / 'iterations' / f'{i:06d}'
            (d / 'u').mkdir(parents=True)
            ck = jwrite(d / 'u' / 'checkpoint.json', {'i': i})
            upd = jwrite(d / 'u' / 'update.json', {'after_state_sha256': f'new{i}',
                                                   'checkpoint': {'path': ck.as_posix(), 'sha256': retention.sha(ck)}})
            jwrite(d / 'complete.json', {'update': {'path': upd.as_posix()},
                                         'chain_ledger': {'usec': 10**12 + i, 'unix_seconds': 9e9 + i, 'instance': 1}})

    def test_steady_cycle_reads_only_what_is_new(self):
        root = Path(self.tmp.name)

        def uncached():
            return retention.plan(self.path), retention.guard([root], 10**12, 0, [self.path])

        cache = retention.IndexCache(self.path)
        caches = {Path(self.path).as_posix(): cache}

        def cached():
            return retention.plan(self.path, None, cache), retention.guard([root], 10**12, 0, [self.path], caches)

        self.add_updates(3)
        cached()  # the first cycle reads everything and settles the records whose files are gone
        self.add_updates(5)
        t0 = time.perf_counter()
        slow = uncached()
        t1 = time.perf_counter()
        self.add_updates(5)
        t2 = time.perf_counter()
        fast = cached()
        t3 = time.perf_counter()
        _, records = retention.index(self.path)
        self.assertEqual(len(records), self.UPDATES + 13)
        self.assertEqual(fast, uncached())
        self.assertEqual(slow[0]['checkpoints'], [])
        self.assertEqual(len(cache.active), 13)
        timing = {'records': len(records), 'uncached_seconds': round(t1 - t0, 3), 'cached_seconds': round(t3 - t2, 3)}
        print(json.dumps(timing), file=sys.stderr)
        self.assertLess(t3 - t2, 0.25 * (t1 - t0), timing)


if __name__ == '__main__':
    unittest.main()
