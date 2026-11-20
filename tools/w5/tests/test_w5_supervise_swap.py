"""The supervisor swap helpers (tools/w5/supervise_swap.py) and the guard-only checks: exact process identity, index
tail checks and the locked cut of a torn last line, the swap watch, guard-only refusal at every cycle, and the
read-only cache's cursor."""
import contextlib
import io
import json
import os
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
import supervise_swap as swap  # noqa: E402
from test_w5_chains import Node, jwrite  # noqa: E402

PLAN = '/srv/x/oc/plan/oc-node1/CHAINS-PLAN.json'
TWIN = '/srv/x/oc/plan/oc-node1/CHAINS-PLANXjson'  # one character apart


def old_loop(plan):
    return ['bash', '-c', f'while true; do python3 /srv/x/tools-src/tools/w5/chains.py supervise --plan {plan} '
                          f'--loop 60 >> /srv/x/records/supervise.log 2>&1; echo "$(date -Is) supervise exited $?" '
                          f'>> /srv/x/records/supervise-restarts.log; sleep 30; done']


def new_loop(plan, extra=''):
    return ['bash', '-c', swap.LOOP_TEXT, 'swap-loop', '/usr/bin/python3.11', '/srv/x/tools-mem/tools/w5', plan, '60',
            '/srv/x/records', '30', extra]


def supervisor(plan, w5='/srv/x/tools-mem/tools/w5', py='/usr/bin/python3.11', extra=()):
    return [py, f'{w5}/chains.py', 'supervise', '--plan', plan, '--loop', '60', *extra]


class ProcessIdentityTests(unittest.TestCase):
    def table(self):
        return {
            10: (1, old_loop(PLAN)), 11: (10, ['python3', '/srv/x/tools-src/tools/w5/chains.py', 'supervise', '--plan',
                                               PLAN, '--loop', '60']),
            20: (1, new_loop(TWIN)), 21: (20, supervisor(TWIN)),
            30: (1, ['bash', '-c', f"grep 'supervise --plan {PLAN}' /srv/x/records/supervise.log; sleep 100"]),
            31: (1, ['bash', '-c', 'echo ' + old_loop(PLAN)[2]]),
            32: (1, ['ssh', 'node', f'python3 chains.py supervise --plan {PLAN}']),
            33: (1, ['python3', '/home/u/chains.py.bak', 'supervise', '--plan', PLAN]),
            34: (1, ['python3', '/srv/x/tools/w5/chains.py', 'status', '--plan', PLAN]),
            40: (1, ['bash', '-c', f'sleep 5; {" ".join(supervisor(PLAN))}']),
            41: (40, supervisor(PLAN, extra=('--guard-only',))),
        }

    def test_exactly_the_plan_and_its_loops(self):
        table = self.table()
        self.assertEqual(swap.select(table, PLAN), ([10], [11, 41]))  # 40 is no loop: its parent is not signalled
        self.assertEqual(swap.select(table, TWIN), ([20], [21]))
        self.assertEqual(swap.select(table, '/srv/x/oc/plan/oc-node1/CHAINS-PLAN'), ([], []))
        del table[11], table[21]  # no supervisor alive: the loops are still found by their own argv
        self.assertEqual(swap.select(table, PLAN), ([10], [41]))
        self.assertEqual(swap.select(table, TWIN), ([20], []))

    def test_a_new_loop_with_guard_only_and_odd_paths(self):
        plan = "/srv/a b/it's/CHAINS-PLAN.json"
        table = {5: (1, new_loop(plan, '--guard-only')), 6: (5, supervisor(plan, extra=('--guard-only',)))}
        self.assertEqual(swap.select(table, plan), ([5], [6]))
        self.assertEqual(swap.select(table, plan[:-1]), ([], []))

    def test_node1_shapes(self):
        """The real node1 loop and supervisor (pid 1568722 and 1568730), argv as /proc shows them."""
        c = '/srv/work/chains-20261107'
        plan = f'{c}/oc/plan/oc-node1/CHAINS-PLAN.json'
        loop = ['bash', '-c', f'while true; do python3 {c}/tools-src/tools/w5/chains.py supervise --plan {plan} --loop '
                              f'60 >> {c}/records/supervise.log 2>&1; echo "$(date -Is) supervise exited $?" >> '
                              f'{c}/records/supervise-restarts.log; sleep 30; done']
        py = ['python3', f'{c}/tools-src/tools/w5/chains.py', 'supervise', '--plan', plan, '--loop', '60']
        copy = f'{c}/oc/plan/oc-node1/CHAINS-PLAN.supervise-92d226374.json'
        table = {1568722: (1, loop), 1568730: (1568722, py),
                 1751025: (1, ['bash', '-c', f'fails=(); while true; do /usr/bin/python3.11 -B /srv/work/batch-ladder/'
                                             f'tools/w5/chains.py supervise --plan /srv/work/batch-ladder/wf/oc/plan/'
                                             f'bladder-f/CHAINS-PLAN.json --loop 60; done'])}
        self.assertEqual(swap.select(table, plan), ([1568722], [1568730]))
        self.assertEqual(swap.select(table, copy), ([], []))
        del table[1568730]  # in the loop's 30 s pause: the loop alone, still found
        self.assertEqual(swap.select(table, plan), ([1568722], []))

    def test_a_recycled_pid_is_never_signalled(self):
        table = self.table()
        sent = []
        with contextlib.redirect_stderr(io.StringIO()) as err:
            got = swap.signal_verified(PLAN, 15, [10, 11, 30, 21, 999], table, lambda pid, sig: sent.append(pid))
        self.assertEqual(got, [10, 11])
        self.assertEqual(sent, [10, 11])
        self.assertEqual(err.getvalue().count('not signalled'), 3)

    @unittest.skipUnless(Path('/proc/self/cmdline').exists() and os.name != 'nt', 'needs /proc')
    def test_real_processes(self):
        """Real processes: a loop of the step file's form running a stand-in chains.py, the same for a plan one
        character apart, and bystanders whose command lines hold the matching text."""
        with tempfile.TemporaryDirectory() as t:
            t = Path(t)
            w5 = t / 'w5'
            w5.mkdir()
            (w5 / 'chains.py').write_text('import time\ntime.sleep(60)\n')
            records = t / 'records'
            records.mkdir()
            plans = [str(t / 'CHAINS-PLAN.json'), str(t / 'CHAINS-PLANXjson')]
            procs = []
            try:
                for plan in plans:
                    procs.append(subprocess.Popen(['bash', '-c', swap.LOOP_TEXT, 'swap-loop', sys.executable, str(w5),
                                                   plan, '60', str(records), '30', ''], start_new_session=True))
                procs.append(subprocess.Popen(['bash', '-c', f"echo 'supervise --plan {plans[0]}'; sleep 60"],
                                              start_new_session=True))
                time.sleep(1.5)
                loops, pys = swap.select(swap.process_table(), plans[0])
                self.assertEqual(loops, [procs[0].pid])
                self.assertEqual(len(pys), 1)
                self.assertEqual(swap.process_table()[pys[0]][0], procs[0].pid)
                self.assertEqual(swap.select(swap.process_table(), plans[1])[0], [procs[1].pid])
            finally:
                for p in procs:
                    os.killpg(p.pid, 15)
                    p.wait(10)


class TailTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.t = Path(self.tmp.name)
        self.chain = self.t / 'chains' / 'c1'
        self.chain.mkdir(parents=True)
        self.records = self.t / 'records'
        self.records.mkdir()
        self.index = self.chain / 'chain.index.jsonl'
        self.lines = [json.dumps({'update': u, 'boundary': u + 1, 'pad': 'x' * (u % 50)}).encode() + b'\n'
                      for u in range(300)]
        self.index.write_bytes(b''.join(self.lines))

    def tearDown(self):
        self.tmp.cleanup()

    def out(self, fn, *args, **kwargs):
        with contextlib.redirect_stdout(io.StringIO()) as buf:
            code = fn(*args, **kwargs)
        return code, buf.getvalue()

    def test_a_torn_tail_on_a_file_larger_than_the_block(self):
        whole = self.index.read_bytes()
        self.assertGreater(len(whole), 64 * 100)
        torn = self.lines[-1][:23]
        with open(self.index, 'ab') as f:
            f.write(torn)
        self.assertEqual(swap.tail_state(self.index, block=64), 'torn')
        self.assertEqual(self.out(swap.check_tails, self.t)[0], 1)
        reads = []
        real_open = open

        class Counting:  # every read of the index, by size: the cut never reads the file whole
            def __init__(self, f):
                self.f = f

            def read(self, n=-1):
                reads.append(n)
                return self.f.read(n)

            def __getattr__(self, name):
                return getattr(self.f, name)

            def __enter__(self):
                return self

            def __exit__(self, *exc):
                return self.f.__exit__(*exc)

        def counting_open(path, mode='r', *a, **k):
            f = real_open(path, mode, *a, **k)
            return Counting(f) if Path(path) == self.index and mode == 'rb' else f
        with mock.patch('builtins.open', counting_open):
            code, text = self.out(swap.cut_tail, self.index, self.records, block=64)
        self.assertEqual(code, 0, text)
        self.assertNotIn(-1, reads)
        self.assertLessEqual(max(reads), 64 + len(torn))
        self.assertEqual(self.index.read_bytes(), whole)
        saved = list(self.records.glob('torn-tail-c1-*.bin'))
        self.assertEqual([p.read_bytes() for p in saved], [torn])
        self.assertEqual(swap.tail_state(self.index, block=64), 'ok')
        self.assertEqual(self.out(swap.check_tails, self.t)[0], 0)

    def test_a_whole_line_without_its_newline_is_ok_and_not_cut(self):
        data = b''.join(self.lines)[:-1]
        self.index.write_bytes(data)
        self.assertEqual(swap.tail_state(self.index, block=64), 'ok')
        self.assertEqual(self.out(swap.check_tails, self.t)[0], 0)
        self.assertEqual(self.out(swap.cut_tail, self.index, self.records, block=64)[0], 0)
        self.assertEqual(self.index.read_bytes(), data)
        self.assertEqual(list(self.records.iterdir()), [])
        self.index.write_bytes(b''.join(self.lines) + b'\n\r\n  \n')  # blank trailing lines: index() skips them
        self.assertEqual(swap.tail_state(self.index, block=8), 'ok')

    def test_a_writer_that_finishes_its_line_before_the_lock(self):
        whole = b''.join(self.lines)
        torn = self.lines[-1][:30]
        self.index.write_bytes(b''.join(self.lines[:-1]) + torn)

        def finish():  # the other writer completes its append between the first look and the lock
            with open(self.index, 'ab') as f:
                f.write(self.lines[-1][30:])
        code, text = self.out(swap.cut_tail, self.index, self.records, block=64, before_lock=finish)
        self.assertEqual(code, 0)
        self.assertIn('whole now', text)
        self.assertEqual(self.index.read_bytes(), whole)
        self.assertEqual(list(self.records.iterdir()), [])

    def test_the_cut_holds_the_index_lock(self):
        torn = self.lines[-1][:30]
        self.index.write_bytes(b''.join(self.lines[:-1]) + torn)
        held = []
        real = retention.index_lock

        @contextlib.contextmanager
        def watching(chain_path):
            with real(chain_path):
                held.append(Path(chain_path))
                yield
                held.append(self.index.stat().st_size)  # the truncate happened before the lock is released
        with mock.patch.object(retention, 'index_lock', watching):
            self.assertEqual(self.out(swap.cut_tail, self.index, self.records, block=64)[0], 0)
        self.assertEqual(held, [self.chain / 'chain.json', len(b''.join(self.lines[:-1]))])

    def test_a_terminated_unparseable_line_is_not_cut(self):
        self.index.write_bytes(b''.join(self.lines[:-1]) + b'{"update": 2\n')
        self.assertEqual(swap.tail_state(self.index), 'torn')
        code, text = self.out(swap.cut_tail, self.index, self.records)
        self.assertEqual(code, 1)
        self.assertIn('not a torn append', text)


class WatchTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name) / 'oc'
        for name, admitted in (('c1', 8156), ('c2', 7918)):
            jwrite(self.root / 'chains' / name / 'chain.json', {})
            jwrite(self.root / 'chains' / name / 'ADMITTED.json', {'count': admitted})
        jwrite(self.root / 'chains' / 'c4' / 'chain.json', {})
        jwrite(self.root / 'chains' / 'c4' / 'END.json', {})
        self.log = Path(self.tmp.name) / 'supervise.log'
        self.log.write_text('old Traceback before the watch\n')

    def tearDown(self):
        self.tmp.cleanup()

    def guard(self, unix, ok=True, ceilings=None):
        jwrite(self.root / 'GUARD.json', {'unix': unix, 'ok': ok, 'ceilings': ceilings or {'c1': 8160, 'c2': 7922}})

    def run_watch(self, t0, now):
        with contextlib.redirect_stdout(io.StringIO()) as buf:
            code = swap.watch(self.root, 'plan', t0, 723, every=0, log=self.log, rounds=1, now=lambda: now)
        return code, json.loads(buf.getvalue().splitlines()[-1])

    def test_wait_escalate_and_success(self):
        t0 = 1000.0
        self.guard(990.0)
        code, state = self.run_watch(t0, 1100.0)
        self.assertEqual((code, state['verdict']), (1, 'WAIT'))
        self.assertEqual(state['chains']['c1'], {'admitted': 8156, 'ceiling': 8160, 'may_start': 4,
                                                 'guard_wait_since': None})
        self.assertNotIn('c4', state['chains'])
        self.assertIsNone(state['escalate_in_s'])
        with open(self.root / 'LOG.jsonl', 'a') as f:  # c2 refused at 1600, c1 at 1700 then admitted again
            for row in ({'event': 'guard-wait', 'chain': 'c2', 'unix': 1600.0},
                        {'event': 'guard-wait', 'chain': 'c1', 'unix': 1700.0},
                        {'event': 'segment', 'chain': 'c1', 'unix': 1720.0},
                        {'event': 'guard-wait', 'chain': 'c1', 'unix': 500.0}):  # before T0: not this swap's
                f.write(json.dumps(row) + '\n')
        code, state = self.run_watch(t0, 1800.0)
        self.assertEqual(state['chains']['c2']['guard_wait_since'], 1600.0)
        self.assertIsNone(state['chains']['c1']['guard_wait_since'])
        self.assertEqual(state['escalate_in_s'], 1600 + 1800 - 723 - 300 - 1800)
        code, state = self.run_watch(t0, 1600 + 1800 - 723 - 300)
        self.assertEqual((code, state['verdict']), (2, 'ESCALATE'))
        self.guard(2000.0, ceilings={'c1': 8160})
        self.assertEqual(self.run_watch(t0, 2000.5)[1]['verdict'], 'WAIT')  # c2 has no ceiling: no success
        self.guard(2000.0)
        self.assertEqual(self.run_watch(t0, 2000.5)[0], 0)
        self.guard(2000.0, ok=False)
        self.assertNotEqual(self.run_watch(t0, 2000.5)[1]['verdict'], 'SUCCESS')

    def test_a_traceback_after_the_start_escalates(self):
        self.guard(990.0)
        calls = []

        def now():
            calls.append(1)
            if len(calls) == 1:
                with open(self.log, 'a') as f:
                    f.write('Traceback (most recent call last):\n')
            return 1100.0
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(swap.watch(self.root, 'plan', 1000.0, 723, every=0, log=self.log, rounds=3,
                                        now=now), 2)


class GuardOnlyTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.node = Node(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_every_guard_only_cycle_checks_the_plan(self):
        path, plan = self.node.plan('0', {'c1': {'arm': 'C', 'slot': 0, 'seed_root': 'oc/c1'}},
                                    c_wall_seconds=600.0, independent=True, label='g')
        _, pair_plan = Node(tempfile.mkdtemp(dir=self.tmp.name)).stage0(c_wall=600.0)  # a C/X pair and a filler
        pair_plan = dict(pair_plan, root=plan['root'])
        for bad in (pair_plan, dict(plan, pairs={'1': {'C': 'c1', 'X': 'x1'}}), dict(plan, independent=None),
                    dict(plan, chains=dict(plan['chains'], f9=dict(plan['chains']['c1'], arm='F')))):
            with self.assertRaises(chains.Stop):
                chains.cycle(bad, path, guard_only=True)
        self.assertFalse((self.node.root / 'GUARD.json').exists())
        # A loop whose plan is reloaded as a pair plan refuses at that cycle.
        loads = iter([plan, plan, pair_plan])
        args = mock.Mock(plan=str(path), loop=0.01, guard_only=True)
        with mock.patch.object(chains, 'load_plan', side_effect=lambda *a, **k: next(loads)), \
                mock.patch.object(chains.time, 'sleep'), contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(chains.Stop):
                chains.supervise(args)
        self.assertTrue((self.node.root / 'GUARD.json').exists())  # the allowed plan's cycle published


class ReadOnlyCursorTests(unittest.TestCase):
    def test_a_row_appended_while_the_receipts_are_scanned(self):
        """Physical record 1; another writer appends 2 while the dry cache scans the receipts; then 3: the dry
        cache reads [1, 2, 3], not [1, 3, 3]."""
        with tempfile.TemporaryDirectory() as t:
            t = Path(t)
            root = t / 'chain' / 'seg-00000'
            path = jwrite(t / 'chain' / 'chain.json', {'schema': 'w5-chain/v1', 'chain': 'c', 'arm': 'C',
                                                       'launched_unix': 1.0, 'segments': [{
                                                           'config': (t / 'u.json').as_posix(), 'offset': 0,
                                                           'root': root.as_posix(), 'updates': 8}]})

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
            retention.index(path)
            receipt(1)
            real_scan = retention.scan

            def scanning(chain, known):
                new = real_scan(chain, known)
                with open(retention.index_path(path), 'ab') as f:  # the other writer appends row 2 meanwhile
                    f.write(b''.join(json.dumps(r).encode() + bytes([10]) for r in new))
                return new
            ro = retention.IndexCache(path, read_only=True)
            with mock.patch.object(retention, 'scan', scanning):
                self.assertEqual([r['boundary'] for r in ro.refresh()[1]], [1, 2])
            receipt(2)
            retention.index(path)  # and row 3
            got = ro.refresh()[1]
            self.assertEqual([r['boundary'] for r in got], [1, 2, 3])
            self.assertEqual(list(got), retention.index(path)[1])
            self.assertEqual(ro.records.extra, {})


if __name__ == '__main__':
    unittest.main()
