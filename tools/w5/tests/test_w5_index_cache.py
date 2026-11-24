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


class CompactRecordsTests(unittest.TestCase):
    """The cache's compact records (`retention.IndexRecords`, `IndexView`): a view reads exactly like the list of
    dicts `index` returns, the compact-field reads of the supervise cycle and the pair code equal their reads of
    every record, odd values read back exactly, and memory stays at the offsets and fields."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.t = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def chain(self, records, segments=1, length=None):
        """A chain manifest whose segments are complete and whose index file holds `records` (dicts or raw lines)."""
        segs = []
        length = length or max(1, len(records))
        for s in range(segments):
            root = self.t / 'chain' / f'seg-{s:05d}'
            root.mkdir(parents=True, exist_ok=True)
            (root / 'completion.json').write_text('{}')
            segs.append({'config': (self.t / 'unused.json').as_posix(), 'offset': s * length,
                         'root': root.as_posix(), 'updates': length})
        path = jwrite(self.t / 'chain' / 'chain.json', {'schema': 'w5-chain/v1', 'chain': 'c', 'arm': 'C',
                                                        'launched_unix': 1.0, 'segments': segs})
        with open(retention.index_path(path), 'wb') as f:
            for r in records:
                f.write(r if isinstance(r, bytes) else json.dumps(r).encode() + b'\n')
        return path

    def record(self, u, **changes):
        ck = self.t / 'ck' / f'{u}.json'
        rec = {'update': u, 'boundary': u + 1, 'segment': 0, 'iteration': u, 'after_state_sha256': f'{u:064x}',
               'checkpoint': {'path': ck.as_posix(), 'sha256': f'{u:064x}'}, 'state': None,
               'ledger_usec': (u + 1) * 1000, 'unix_seconds': 10.0 + u / 4, 'instance': u // 7}
        rec.update(changes)
        return rec

    def test_a_view_reads_as_the_index_list(self):
        recs = [self.record(u) for u in range(40)]
        path = self.chain(recs, length=40)
        (self.t / 'ck').mkdir()
        for u in (0, 7, 39):
            Path(recs[u]['checkpoint']['path']).write_text('x')
        cache = retention.IndexCache(path)
        _, view = cache.refresh()
        listed = retention.index(path)[1]
        self.assertIsInstance(view, retention.IndexView)
        self.assertEqual(view, listed)
        self.assertEqual(listed, list(view))
        self.assertEqual(len(view), 40)
        self.assertEqual(view[-1], listed[-1])
        self.assertEqual(view[5], listed[5])
        self.assertEqual(list(view[-7:]), listed[-7:])
        self.assertEqual(list(view[3:30:4]), listed[3:30:4])
        self.assertEqual(list(reversed(view)), listed[::-1])
        self.assertTrue(view and not view[40:])
        with self.assertRaises(IndexError):
            view[40]
        # A whole read settles nothing (plan and guard settle, as before): `active` is every record, until the guard.
        self.assertEqual(len(cache.active), 40)
        self.assertEqual(len(cache.settled), 0)
        retention.guard([self.t], 10**12, 0, [path], {Path(path).as_posix(): cache})
        self.assertEqual([r['boundary'] for r in cache.active], [1, 8, 40])
        self.assertEqual(len(cache.settled), 37)
        # The supported interface, and nothing else (a list's mutation raises; never equal to a tuple).
        self.assertFalse(view == tuple(listed))
        self.assertTrue(view != tuple(listed))
        self.assertFalse(view != listed)
        self.assertTrue(listed[3] in view)
        self.assertEqual(view.index(listed[3]), 3)
        self.assertEqual(view.count(listed[3]), 1)
        for attempt in (lambda: view.append(1), lambda: view.extend([]), lambda: view.sort(), lambda: view + [],
                        lambda: view.__setitem__(0, {}), lambda: view.__delitem__(0), lambda: view < listed):
            with self.assertRaises((AttributeError, TypeError)):
                attempt()

    def test_compact_reads_equal_reading_every_record(self):
        """first_at_or_past, ledger_series, project, decide, select, valid_records, protected_paths, the pair's
        running evidence and the replay sample's selection, over a view and over the same list."""
        import replay_sample
        (self.t / 'ck').mkdir()
        recs = [self.record(u, instance=u // 9) for u in range(300)]
        recs[11]['ledger_usec'] = None
        recs[12]['unix_seconds'] = None
        recs[13]['instance'] = None
        path = self.chain(recs, length=300)
        for u in range(0, 300, 16):
            Path(recs[u]['checkpoint']['path']).write_text('x')
        _, view = retention.IndexCache(path).refresh()
        listed = list(view)
        self.assertEqual(listed, retention.index(path)[1])
        self.assertIsNotNone(retention.fields(view, 'ledger_usec'))
        for target in (0, 1000, 12_000, 12_001, 150_000, 300_000, 300_001):
            self.assertEqual(retention.first_at_or_past(view, target), retention.first_at_or_past(listed, target))
        self.assertEqual(retention.ledger_series(view), retention.ledger_series(listed))
        self.assertEqual(retention.project(view, 1.0, 100.0), retention.project(listed, 1.0, 100.0))
        chain = retention.load_chain(path)
        for ended in (False, True):
            self.assertEqual(retention.decide(chain, view, chain, view[:200], ended, 5.0, 90.0, 10),
                             retention.decide(chain, listed, chain, listed[:200], ended, 5.0, 90.0, 10))
        for target in (17_000, 100_000, 299_000):
            self.assertEqual(retention.select(chain, view, target, {}), retention.select(chain, listed, target, {}))
        self.assertEqual(retention.prune_manifest(chain, view, 0), retention.prune_manifest(chain, listed, 0))
        for last in (None, 0, 150, 300, 10**6):
            end = None if last is None else {'last_valid_boundary': last}
            got = chains.valid_records({'records': view, 'end': end})
            self.assertEqual(list(got), chains.valid_records({'records': listed, 'end': end}))
        for keep in (set(), {17}, {1, 300, 5000}):
            self.assertEqual(chains.protected_paths({'records': view}, keep),
                             chains.protected_paths({'records': listed}, keep))
        self.assertEqual(chains.ledger_at(view, 77), chains.ledger_at(listed, 77))
        for nxt in (0, 150, 299, 300, 400):
            self.assertEqual(list(replay_sample.since(view, nxt)), replay_sample.since(listed, nxt))
            self.assertEqual(replay_sample.protected(self.t / 'sample', 'X8', 'c', 'w5/q', chain, view),
                             replay_sample.protected(self.t / 'sample', 'X8', 'c', 'w5/q', chain, listed))
        # The pair decision's running evidence: the latest receipt time per instance.
        evidence = {}
        with mock.patch.object(chains, 'running_seconds', side_effect=lambda led, now, fr, st, ev: evidence.update(
                {id(ev): dict(ev)}) or 0.0), mock.patch.object(chains, 'frozen_intervals', return_value=[]):
            for records in (view, listed):
                x = {'records': records, 'ledger': [], 'state': self.t / 'state', 'chain': chain, 'end': None,
                     'name': 'x', 'path': path, 'live': False, 'paused': False, 'stop_at': None}
                c = {'records': [], 'end': None, 'chain': chain}
                chains.pair_decide({'root': self.t.as_posix(), 'chains': {}}, c, x, {}, 100.0, {})
        a, b = evidence.values()
        self.assertEqual(a, b)
        self.assertEqual(a[0], 12.0)  # update 8's receipt; update 12's time is None
        self.assertNotIn(None, a)

    def test_odd_values_read_back_exactly_and_a_missing_field_reads_every_record(self):
        recs = [self.record(0, ledger_usec=2**70, unix_seconds=5, instance=True),
                self.record(1, ledger_usec=7.5, boundary=2.0),
                self.record(2, instance='i')]
        path = self.chain(recs, length=3)
        _, view = retention.IndexCache(path).refresh()
        self.assertEqual(list(retention.fields(view, 'ledger_usec')[0]), [2**70, 7.5, 3000])
        self.assertEqual([type(v) for v in retention.fields(view, 'unix_seconds')[0]], [int, float, float])
        self.assertEqual(list(retention.fields(view, 'instance')[0]), [True, 0, 'i'])
        self.assertIs(list(retention.fields(view, 'instance')[0])[0], True)
        self.assertEqual(view, retention.index(path)[1])
        listed = retention.index(path)[1]
        self.assertEqual(retention.first_at_or_past(view, 7), recs[0])
        self.assertIsNone(retention.first_at_or_past(view, 2**70 + 1))
        self.assertEqual(retention.first_at_or_past(view, 100), recs[0])
        self.assertEqual(retention.first_at_or_past(view, 7), retention.first_at_or_past(listed, 7))
        # A missing field is missing for that record alone: the column still serves every other record.
        recs.append({k: v for k, v in self.record(3).items() if k != 'ledger_usec'})
        recs.append(self.record(4))
        path = self.chain(recs, length=5)
        _, view = retention.IndexCache(path).refresh()
        self.assertEqual(list(retention.fields(view, 'ledger_usec')[0]), [2**70, 7.5, 3000, None, 5000])
        with self.assertRaises(KeyError):
            list(retention.fields(view, 'ledger_usec', absent='raise')[0])
        listed = retention.index(path)[1]
        self.assertEqual(view, listed)
        self.assertEqual(retention.ledger_series(view), retention.ledger_series(listed))
        for target in (4000, 4500, 10**30):
            self.assertEqual(retention.first_at_or_past(view, target), retention.first_at_or_past(listed, target))
        # A missing boundary raises where reading every record raises.
        recs[3] = {k: v for k, v in self.record(3).items() if k != 'boundary'}
        path = self.chain(recs, length=5)
        cache = retention.IndexCache(path)
        with self.assertRaises(KeyError):  # `active` is every record whose boundary is not settled
            cache.refresh()
        listed = retention.index(path)[1]
        with self.assertRaises(KeyError):
            chains.protected_paths({'records': listed}, {5})

    def test_lines_are_split_as_index_reads_them(self):
        """CRLF line ends, empty lines, a line holding two records split by another line break character, and a
        last line without its newline."""
        a, b, c, d = (json.dumps(self.record(u)).encode() for u in range(4))
        path = self.chain([a + b'\r\n', b'\n', b + b'\x1e' + c + b'\n', b'\r\n', d], length=4)
        cache = retention.IndexCache(path)
        _, view = cache.refresh()
        self.assertEqual([r['update'] for r in view], [0, 1, 2, 3])
        self.assertEqual(view, retention.index(path)[1])
        self.assertEqual(view[2], self.record(2))
        self.assertTrue(retention.index_path(path).read_bytes().endswith(d + b'\n'))

    def test_boundaries_behave_as_a_set(self):
        got, want = retention.Boundaries(), set()
        for values in ([5, 9, 5], [0, 2**28 + 3, -1], [True, 2.0, 2.5, 'x', None], range(1000, 1100)):
            got |= values
            want |= set(values)
            self.assertEqual(len(got), len(want))
        for probe in (5, 5.0, 1, True, 2, 2.5, 3, 'x', None, -1, 2**28 + 3, 2**28 + 4, 1050, 10**9, float('nan'),
                      float('inf'), [1]):
            try:
                expected = probe in want
            except TypeError:
                expected = False
            self.assertEqual(probe in got, expected, probe)
        self.assertEqual(set(got), want)

    def test_memory_stays_at_offsets_and_fields(self):
        """200,000 records: the kept records cost under 100 bytes each, and a whole read never holds them parsed
        (its peak stays far below the parsed index's size)."""
        import tracemalloc
        updates = 200_000
        lines = [json.dumps(self.record(u, segment=u // 256, iteration=u % 256)).encode() + b'\n'
                 for u in range(updates)]
        path = self.chain(lines, segments=updates // 256 + 1, length=256)
        del lines
        gc_free = __import__('gc').collect
        gc_free()
        tracemalloc.start()
        try:
            cache = retention.IndexCache(path)
            base, _ = tracemalloc.get_traced_memory()
            tracemalloc.reset_peak()
            _, view = cache.refresh()
            kept, peak = tracemalloc.get_traced_memory()
        finally:
            tracemalloc.stop()
        self.assertEqual(len(view), updates)
        sample = retention.index(path)[1][:1000]
        parsed = sum(len(json.dumps(r)) for r in sample) / len(sample) * 2.5  # a parsed record's dicts and strings
        per_record = (kept - base) / updates
        timing = {'kept_bytes_per_record': round(per_record, 1), 'peak_bytes': peak - base,
                  'parsed_index_estimate_bytes': int(parsed * updates)}
        print(json.dumps(timing), file=sys.stderr)
        self.assertLess(per_record, 100, timing)
        self.assertLess(peak - base, 0.25 * parsed * updates, timing)
        self.assertEqual(len(cache.active), updates)  # nothing is settled before the first plan and guard
        # The first plan and guard over every record stay far below the parsed index too.
        tracemalloc.start()
        try:
            base, _ = tracemalloc.get_traced_memory()
            doc = retention.plan(path, None, cache)
            retention.guard([self.t], 10**12, 0, [path], {Path(path).as_posix(): cache})
            _, first = tracemalloc.get_traced_memory()
        finally:
            tracemalloc.stop()
        self.assertEqual(doc['checkpoints'], [])
        self.assertEqual(len(cache.active), 0)
        self.assertLess(first - base, 0.25 * parsed * updates, {'first_cycle_peak_bytes': first - base})


    def test_duplicate_boundaries_keep_what_the_first_plan_kept(self):
        """Two records at boundary 64, one whose checkpoint is gone and one holding the file `shared`, which
        boundary 65 also names (latest 66): the first cached plan keeps `shared`, as reading everything does."""
        (self.t / 'ck').mkdir()
        shared = self.t / 'ck' / 'shared.json'
        shared.write_text('s')
        pin = {'path': shared.as_posix(), 'sha256': '5' * 64}
        recs = [self.record(u, segment=0) for u in range(63)]
        recs += [self.record(63, segment=0), self.record(63, segment=0, checkpoint=pin),
                 self.record(64, segment=0, checkpoint=pin), self.record(65, segment=1)]
        Path(recs[-1]['checkpoint']['path']).write_text('latest')
        path = self.chain(recs, segments=3, length=66)
        cache = retention.IndexCache(path)
        uncached = retention.plan(path)
        cached = retention.plan(path, None, cache)
        self.assertEqual(cached, uncached)
        self.assertNotIn(shared.as_posix(), [i['path'] for i in cached['checkpoints']])
        self.assertTrue(shared.exists())

    def test_a_line_rewritten_in_place_stops_before_any_decision(self):
        """The index is append-only: a cached line overwritten with one of the same size (here the latest
        boundary, 4097 made 9999) raises IndexChanged at the next refresh, before any plan or deletion. The old
        cache ignored such a rewrite (it kept its parsed records until a whole read)."""
        (self.t / 'ck').mkdir()
        recs = [self.record(63), self.record(4096)]
        for r in recs:
            Path(r['checkpoint']['path']).write_text('x')
        path = self.chain(recs, length=5000)
        cache = retention.IndexCache(path)
        cache.refresh()
        index_file = retention.index_path(path)
        data = index_file.read_bytes()
        self.assertIn(b'"boundary": 4097', data)
        with open(index_file, 'r+b') as f:  # same inode, same size
            f.write(data.replace(b'"boundary": 4097', b'"boundary": 9999'))
        with mock.patch.object(retention, 'apply', side_effect=AssertionError('no deletion')), \
                contextlib.redirect_stderr(io.StringIO()) as err:
            with self.assertRaises(retention.IndexChanged):
                retention.plan(path, None, cache)
        self.assertIn('"boundary": 4097', err.getvalue())  # the log line names the chain and boundary
        self.assertIn('"chain": "chain"', err.getvalue())
        self.assertTrue(all(Path(r['checkpoint']['path']).exists() for r in recs))
        # A rewrite of an older line is caught when that record is read.
        index_file.write_bytes(data)
        cache = retention.IndexCache(path)
        cache.refresh()
        with open(index_file, 'r+b') as f:
            f.write(data.replace(b'"update": 63,', b'"update": 64,'))
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(retention.IndexChanged):
            retention.plan(path, None, cache)

    def test_cr_only_and_long_lines_with_bounded_memory(self):
        """A CR-only index (which index() reads through splitlines) and a 5 MB record line are read as index() reads
        them, holding a line at a time rather than the file."""
        import tracemalloc
        recs = [self.record(u) for u in range(100_000)]
        path = self.chain([json.dumps(r).encode() + b'\r' for r in recs], length=100_000)
        size = retention.index_path(path).stat().st_size
        tracemalloc.start()
        try:
            base, _ = tracemalloc.get_traced_memory()
            cache = retention.IndexCache(path)
            _, view = cache.refresh()
            _, peak = tracemalloc.get_traced_memory()
        finally:
            tracemalloc.stop()
        self.assertEqual(view, retention.index(path)[1])
        self.assertEqual(len(view), 100_000)
        self.assertLess(peak - base, size / 3, {"peak": peak - base, "file": size})
        long = self.record(1, note='n' * 5_000_000)
        path = self.chain([json.dumps(self.record(0)).encode() + b'\n', json.dumps(long).encode() + b'\n',
                           json.dumps(self.record(2)).encode() + b'\n'], length=3)
        tracemalloc.start()
        try:
            base, _ = tracemalloc.get_traced_memory()
            _, view = retention.IndexCache(path).refresh()
            _, peak = tracemalloc.get_traced_memory()
        finally:
            tracemalloc.stop()
        self.assertEqual([r['update'] for r in view], [0, 1, 2])
        self.assertEqual(view[1], long)
        self.assertLess(peak - base, 40_000_000, {'peak': peak - base})

    def test_snapshots_read_the_cache_not_the_whole_index(self):
        """snapshot() with the cache picks the same boundaries as without it and never reads the index whole."""
        (self.t / 'ck').mkdir()
        recs = [self.record(u) for u in range(200)]
        for r in recs:
            Path(r['checkpoint']['path']).write_text('x')

        def fake(args, **kwargs):
            out = Path(args[args.index('--output') + 1])
            out.write_text('w')
            pin = args[args.index('--sha256') + 1]
            return mock.Mock(returncode=0, stdout=json.dumps({
                'cut_from_state_sha256': pin, 'output': {'path': out.as_posix(), 'sha256': retention.sha(out)},
                'snapshot_bytes': 1, 'parameters_sha256': '0' * 64}))
        made = []
        for cached in (False, True):
            path = self.chain(recs, length=200)
            retention.snapshot_index_path(path).unlink(missing_ok=True)
            shutil.rmtree(self.t / 'chain' / 'chain-snapshots', ignore_errors=True)
            cache = retention.IndexCache(path) if cached else None
            if cache:
                cache.refresh()
            whole = mock.patch.object(retention, 'index', side_effect=AssertionError('whole read')) if cached \
                else contextlib.nullcontext()
            with mock.patch.object(retention.subprocess, 'run', side_effect=fake), whole:
                got = [retention.snapshot(path, 'bin', 120, max_new=3, cache=cache) for _ in range(2)]
            made.append([[e['boundary'] for e in batch] for batch in got])
        self.assertEqual(made[0], made[1])
        self.assertEqual(made[0], [[120, 121, 122], [123, 124, 125]])

    def test_a_read_only_cache_writes_nothing(self):
        """A dry cycle's cache: no lock file, no append, no newline; records past the file's end (receipts the
        supervisor has not indexed yet) are the ones index() then appends."""
        node = Node(tempfile.mkdtemp(dir=self.t))
        with mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X)}), \
                mock.patch.object(chains, 'GUARD_WAIT', 0.0), mock.patch.object(chains, 'GUARD_POLL', 0.0):
            _, plan = node.stage0(c_wall=600.0)
            path = plan['chains']['c1']['manifest']
            node.guard(ceilings={'c1': 2})
            self.assertEqual(node.drive(plan, 'c1'), chains.EXIT_ROOM)
            retention.index(path)
            node.guard(ceilings={'c1': chains.admitted(node.root / 'chains' / 'c1') + 1})
            self.assertEqual(node.drive(plan, 'c1'), chains.EXIT_ROOM)
        index_file = retention.index_path(path)
        lock_file = Path(path).with_suffix('.index.lock')
        lock_file.unlink()
        before = (index_file.read_bytes(), index_file.stat().st_mtime_ns)
        ro = retention.IndexCache(path, read_only=True)
        _, dry = ro.refresh()
        _, again = ro.refresh()
        retention.plan(path, None, ro)
        self.assertEqual((index_file.read_bytes(), index_file.stat().st_mtime_ns), before)
        self.assertFalse(lock_file.exists())
        self.assertEqual(list(again), list(dry))
        written = retention.index(path)[1]
        self.assertGreater(len(written), before[0].count(b'\n'))
        self.assertEqual(list(dry), written)

    def test_guard_only_cycle_publishes_the_guard_and_deletes_nothing(self):
        node = Node(tempfile.mkdtemp(dir=self.t))
        for name, value in SMALL.items():
            patch = mock.patch.object(retention, name, value)
            patch.start()
            self.addCleanup(patch.stop)
        with mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X)}), \
                mock.patch.object(chains, 'GUARD_WAIT', 0.0), mock.patch.object(chains, 'GUARD_POLL', 0.0):
            path, plan = node.plan('0', {'c1': {'arm': 'C', 'slot': 0, 'seed_root': 'oc/c1'},
                                         'c2': {'arm': 'C', 'slot': 1, 'seed_root': 'oc/c2'}},
                                   c_wall_seconds=600.0, independent=True, label='g')
            node.guard(ceilings={'c1': 6, 'c2': 0})
            self.assertEqual(node.drive(plan, 'c1'), chains.EXIT_ROOM)
            files = sorted(p for p in (node.root / 'chains').rglob('*') if p.is_file() and 'index' not in p.name)
            (node.root / 'GUARD.json').unlink()
            caches = {}
            report = chains.cycle(chains.load_plan(path), path, caches=caches, guard_only=True)
            after = sorted(p for p in (node.root / 'chains').rglob('*') if p.is_file() and 'index' not in p.name)
            self.assertTrue(report['guard_only'])
            self.assertEqual(report['retention'], {})
            self.assertIsNone(report['purged'])
            self.assertTrue(json.loads((node.root / 'GUARD.json').read_text())['ok'])
            self.assertEqual(set(files) - set(after), set())  # nothing deleted
            self.assertGreater(len(retention.index(plan['chains']['c1']['manifest'])[1]), 0)
            normal = chains.cycle(chains.load_plan(path), path, caches=caches)
            self.assertGreater(normal['retention']['c1']['removed'], 0)  # the same state: a normal cycle prunes

    def test_the_streamed_migration_writes_what_index_writes(self):
        node = Node(tempfile.mkdtemp(dir=self.t))
        with mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X)}), \
                mock.patch.object(chains, 'GUARD_WAIT', 0.0), mock.patch.object(chains, 'GUARD_POLL', 0.0):
            _, plan = node.stage0(c_wall=600.0)
            path = plan['chains']['c1']['manifest']
            node.guard(ceilings={'c1': 3})
            self.assertEqual(node.drive(plan, 'c1'), chains.EXIT_ROOM)
        index_file = retention.index_path(path)
        retention.index(path)
        legacy = ''.join(json.dumps({k: v for k, v in json.loads(x).items() if k != 'state'}) + '\n'
                         for x in index_file.read_text().splitlines()).encode()
        outputs = []
        for read in (lambda: retention.index(path), lambda: retention.IndexCache(path).refresh()):
            index_file.unlink()
            index_file.write_bytes(legacy)
            records = read()[1]
            outputs.append((index_file.read_bytes(), list(records)))
        self.assertEqual(outputs[0], outputs[1])
        self.assertTrue(all('state' in r for r in outputs[1][1]))


    def test_a_checkpoint_path_rewritten_in_place_stops_the_plan(self):
        """Boundary 64 holds /ck/A, prunable 65 also holds /ck/A, latest 66: a rewrite of 64's path to /ck/B (same
        length, the five cached fields unchanged) is caught by the line's CRC-32 before the plan uses it; a rewrite
        of the last line too."""
        ck = self.t / 'ck'
        ck.mkdir()
        a, b = ck / 'A', ck / 'B'
        a.write_text('a')
        latest = ck / 'latest'
        latest.write_text('l')
        pin = {'path': a.as_posix(), 'sha256': '5' * 64}
        recs = [self.record(u, segment=0) for u in range(63)]
        recs += [self.record(63, segment=0, checkpoint=pin), self.record(64, segment=0, checkpoint=pin),
                 self.record(65, segment=1, checkpoint={'path': latest.as_posix(), 'sha256': '6' * 64})]
        path = self.chain(recs, segments=3, length=66)
        index_file = retention.index_path(path)
        data = index_file.read_bytes()
        uncached = retention.plan(path)
        self.assertNotIn(a.as_posix(), [i['path'] for i in uncached['checkpoints']])
        cases = []
        for line in (63, 65):  # boundary 64 (not the last line), boundary 66 (the last)
            index_file.write_bytes(data)
            cache = retention.IndexCache(path)
            cache.refresh()
            lines = data.splitlines(keepends=True)
            old = a.as_posix().encode() if line == 63 else latest.as_posix().encode()
            new = b.as_posix().encode() if line == 63 else (ck / 'latesX').as_posix().encode()
            self.assertEqual(len(old), len(new))
            lines[line] = lines[line].replace(old, new)
            with open(index_file, 'r+b') as f:  # same inode, same size, same five fields
                f.write(b''.join(lines))
            with mock.patch.object(retention, 'apply', side_effect=AssertionError('no deletion')), \
                    contextlib.redirect_stderr(io.StringIO()) as err, self.assertRaises(retention.IndexChanged):
                retention.plan(path, None, cache)
            cases.append(json.loads(err.getvalue().splitlines()[-1])['boundary'])
        self.assertEqual(cases, [64, 66])
        self.assertTrue(a.exists())

    def test_cutting_a_torn_last_line_back_is_safe(self):
        """A supervisor killed inside an append leaves a torn last line, which index() and the cache both recover
        (mtg-kernel#853; test_w5_index_tail.py). Cut back to the previous newline by hand, the next read finds that
        record again from its receipt and writes the same line: the file ends as it would have."""
        node = Node(tempfile.mkdtemp(dir=self.t))
        with mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X)}), \
                mock.patch.object(chains, 'GUARD_WAIT', 0.0), mock.patch.object(chains, 'GUARD_POLL', 0.0):
            _, plan = node.stage0(c_wall=600.0)
            path = plan['chains']['c1']['manifest']
            node.guard(ceilings={'c1': 3})
            self.assertEqual(node.drive(plan, 'c1'), chains.EXIT_ROOM)
        index_file = retention.index_path(path)
        retention.index(path)
        whole = index_file.read_bytes()
        lines = whole.splitlines(keepends=True)
        torn = b''.join(lines[:-1]) + lines[-1][:40]
        for read in (lambda: retention.index(path), lambda: retention.IndexCache(path).refresh()):
            index_file.write_bytes(torn)
            with contextlib.redirect_stderr(io.StringIO()):
                read()
            self.assertEqual(index_file.read_bytes(), whole)
        index_file.write_bytes(torn[:torn.rfind(b'\n') + 1])  # cut_partial_tail
        records = retention.IndexCache(path).refresh()[1]
        self.assertEqual(index_file.read_bytes(), whole)
        self.assertEqual(list(records), [json.loads(x) for x in lines])

    def test_a_read_only_cache_reconciles_an_outside_append(self):
        """One written record and one found but unwritten; another writer then appends record 2 and receipt 3
        appears: the dry cache reads [1, 2, 3], not [1, 2, 2]."""
        segs = []
        root = self.t / 'chain' / 'seg-00000'
        segs.append({'config': (self.t / 'unused.json').as_posix(), 'offset': 0, 'root': root.as_posix(),
                     'updates': 8})
        path = jwrite(self.t / 'chain' / 'chain.json', {'schema': 'w5-chain/v1', 'chain': 'c', 'arm': 'C',
                                                        'launched_unix': 1.0, 'segments': segs})

        def receipt(i):
            d = root / 'iterations' / f'{i:06d}'
            (d / 'u').mkdir(parents=True)
            ck = jwrite(d / 'u' / 'checkpoint.json', {'i': i})
            upd = jwrite(d / 'u' / 'update.json', {'after_state_sha256': f'{i:064x}', 'checkpoint': {
                'path': ck.as_posix(), 'sha256': retention.sha(ck)}})
            jwrite(d / 'complete.json', {'update': {'path': upd.as_posix()},
                                         'chain_ledger': {'usec': 1000 * (i + 1), 'unix_seconds': 5.0 + i,
                                                          'instance': 0}})
        receipt(0)
        retention.index(path)  # record 1 written
        receipt(1)
        ro = retention.IndexCache(path, read_only=True)
        self.assertEqual([r['boundary'] for r in ro.refresh()[1]], [1, 2])
        self.assertEqual(len(ro.records.extra), 1)
        full = retention.index(path)[1]  # the other writer appends record 2
        self.assertEqual(len(full), 2)
        receipt(2)
        got = ro.refresh()[1]
        self.assertEqual([r['boundary'] for r in got], [1, 2, 3])
        self.assertEqual(list(got)[:2], full)
        self.assertEqual(len(ro.records.extra), 1)
        self.assertEqual(sorted(ro.active.numbers), [0, 1, 2])

    def test_records_read_fresh_and_bad_numbers_refused(self):
        """An unwritten (read-only) record is returned as a copy, like a parsed one; check refuses a negative or
        past-the-end record number."""
        recs = [self.record(0)]
        path = self.chain(recs, length=5)
        ro = retention.IndexCache(path, read_only=True)
        _, view = ro.refresh()
        ro.records.append_unwritten(self.record(1))
        first = ro.records.view()[1]
        first['checkpoint']['path'] = 'changed'
        self.assertEqual(ro.records.view()[1], self.record(1))
        for n in (-1, 2):
            with self.assertRaises(IndexError):
                ro.records.check(n)

    def test_a_record_without_its_after_state_hash_raises_as_before(self):
        (self.t / 'ck').mkdir()
        recs = [self.record(u) for u in range(10)]
        del recs[4]['after_state_sha256']
        path = self.chain(recs, segments=3, length=10)
        with self.assertRaises(KeyError):
            retention.plan(path)
        with self.assertRaises(KeyError):
            retention.plan(path, None, retention.IndexCache(path))

    def test_guard_only_refuses_plans_with_pairs_or_fillers(self):
        node = Node(tempfile.mkdtemp(dir=self.t))
        path, plan = node.stage0(c_wall=600.0)  # a C/X pair and a filler box
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            code = chains.main(['supervise', '--plan', str(path), '--guard-only'])
        self.assertEqual(code, chains.w5box.EXIT_REFUSED)
        self.assertFalse((node.root / 'GUARD.json').exists())
        with self.assertRaises(chains.Stop):
            chains.guard_only_allowed(dict(plan, pairs={}, independent=False))
        with self.assertRaises(chains.Stop):  # its filler chain f2
            chains.guard_only_allowed(dict(plan, pairs={}, independent=True))
        chains.guard_only_allowed(dict(plan, pairs={}, independent=True,
                                       chains={n: c for n, c in plan['chains'].items() if c['arm'] != 'F'}))


if __name__ == '__main__':
    unittest.main()
