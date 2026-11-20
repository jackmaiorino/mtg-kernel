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

    def test_signals_go_through_a_pidfd_checked_after_it_opens(self):
        """Each pid: the pidfd is opened first, then that process's argv is read and checked, then the signal goes
        through the pidfd. A pid now running something else (a reused pid) is refused; a gone one is skipped."""
        table = self.table()
        calls = []

        def opener(pid):
            if pid == 999:
                raise ProcessLookupError
            calls.append(('open', pid))
            return 1000 + pid

        def read_argv(pid):
            calls.append(('argv', pid))
            return table[pid][1] if pid in table else None
        with contextlib.redirect_stderr(io.StringIO()) as err:
            sent, refused = swap.signal_verified(PLAN, 15, [10, 11, 30, 21, 999], opener, read_argv,
                                                 lambda fd, sig: calls.append(('send', fd)), lambda fd: None)
        self.assertEqual((sent, refused), ([10, 11], [30, 21]))
        self.assertEqual(calls[:3], [('open', 10), ('argv', 10), ('send', 1010)])
        self.assertNotIn(('send', 1030), calls)
        self.assertEqual(err.getvalue().count('not signalled'), 2)

    def test_no_pidfd_means_no_signal(self):
        with mock.patch.object(swap, 'os', mock.Mock(spec=['close'])), \
                contextlib.redirect_stderr(io.StringIO()) as err:
            sent, refused = swap.signal_verified(PLAN, 15, [10, 11], read_argv=lambda pid: self.table()[pid][1],
                                                 send=lambda fd, sig: self.fail('signalled'))
        self.assertEqual((sent, refused), ([], [10, 11]))
        self.assertIn('no pidfd_open', err.getvalue())

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
        self.index.write_bytes(b''.join(self.lines) + b'\n\r\n\x0c\n')  # empty lines: index() skips them
        self.assertEqual(swap.tail_state(self.index, block=8), 'ok')
        self.assertEqual(len([t for t in self.index.read_text().splitlines() if t]), len(self.lines))

    def test_a_last_line_of_spaces_is_torn_as_the_reader_sees_it(self):
        """index() and the cache parse a line of spaces (not an empty line) and fail on it: so is it torn here."""
        self.index.write_bytes(b''.join(self.lines) + b'   \n')
        self.assertEqual(swap.tail_state(self.index, block=8), 'torn')
        with open(self.index, 'rb') as f, self.assertRaises(ValueError):
            for text, _, _ in retention.stream_lines(f):
                json.loads(text)

    def test_a_writer_that_finishes_its_line_before_the_lock(self):
        whole = b''.join(self.lines)
        torn = self.lines[-1][:30]
        self.index.write_bytes(b''.join(self.lines[:-1]) + torn)

        def finish():  # the other writer completes its append between the first look and the lock
            with open(self.index, 'ab') as f:
                f.write(self.lines[-1][30:])
        code, text = self.out(swap.cut_tail, self.index, self.records, block=64, before_lock=finish)
        self.assertEqual(code, 0)
        self.assertIn('nothing to cut', text)
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

    def guard(self, unix, gid, ok=True, ceilings=None):
        jwrite(self.root / 'GUARD.json', {'unix': unix, 'id': gid, 'ok': ok,
                                          'ceilings': ceilings or {'c1': 8160, 'c2': 7922}})

    def events(self, *rows):
        with open(self.root / 'LOG.jsonl', 'a') as f:
            for row in rows:
                f.write(json.dumps(row) + '\n')

    def run_watch(self, gid, now, mode='swap'):
        with contextlib.redirect_stdout(io.StringIO()) as buf:
            code = swap.watch(self.root, gid, mode, 723, every=0, log=self.log, rounds=1, now=lambda: now)
        return code, json.loads(buf.getvalue().splitlines()[-1])

    def test_unknowns_and_waits_that_began_before_the_stop(self):
        self.guard(990.0, 7)
        code, state = self.run_watch(7, 1100.0)  # no LOG.jsonl yet
        self.assertEqual((code, state['verdict']), (1, 'WAIT'))
        self.assertEqual(state['chains']['c1']['guard_wait_since'], 'unknown (no LOG.jsonl)')
        self.assertEqual(state['chains']['c1']['may_start'], 4)
        self.assertNotIn('c4', state['chains'])
        self.events({'event': 'segment', 'chain': 'c1', 'unix': 400.0},
                    {'event': 'guard-wait', 'chain': 'c2', 'unix': 500.0},  # before the stop: still open
                    {'event': 'guard-wait', 'chain': 'c1', 'unix': 600.0},
                    {'event': 'no-room', 'chain': 'c1', 'unix': 650.0})  # c1's driver exited: not waiting
        code, state = self.run_watch(7, 1100.0)
        self.assertEqual(state['chains']['c2']['guard_wait_since'], 500.0)
        self.assertEqual(state['chains']['c1']['guard_wait_since'], 'not waiting')
        self.assertEqual(state['first_driver_exit_in_s'], 500 + 1800 - 1100)
        self.assertEqual(state['escalate_in_s'], 500 + 1800 - 723 - 300 - 1100)
        self.assertEqual(self.run_watch(7, 500 + 1800 - 1023)[1]['verdict'], 'ESCALATE')
        with open(self.root / 'LOG.jsonl', 'a') as f:  # a line still being written is read when complete
            f.write('{"event": "segment", "chain": "c2", "un')
        self.assertEqual(self.run_watch(7, 1100.0)[1]['chains']['c2']['guard_wait_since'], 500.0)
        jwrite(self.root / 'chains' / 'c3' / 'chain.json', {})
        self.assertEqual(self.run_watch(7, 1100.0)[1]['chains']['c3']['guard_wait_since'],
                         'unknown (no driver event for this chain)')

    def test_success_means_admission_works(self):
        self.guard(1001.0, 7)
        self.assertEqual(self.run_watch(7, 1002.0)[1]['verdict'], 'WAIT')  # the GUARD recorded after the stop
        self.guard(1001.0, 8, ceilings={'c1': 8156, 'c2': 7922})
        self.assertEqual(self.run_watch(7, 1002.0)[1]['verdict'], 'WAIT')  # c1 has no admission left
        self.guard(1001.0, 8, ok=False)
        self.assertEqual(self.run_watch(7, 1002.0)[1]['verdict'], 'WAIT')
        self.guard(1001.0, 8)
        self.assertEqual(self.run_watch(7, 1001.0 + 900)[1]['verdict'], 'WAIT')  # stale for the drivers
        self.assertEqual(self.run_watch(7, 1002.0), (0, self.run_watch(7, 1002.0)[1]))
        self.assertEqual(self.run_watch(7, 1002.0)[1]['verdict'], 'SUCCESS')

    def test_a_traceback_blocks_success_and_is_shown(self):
        self.guard(1001.0, 8)
        calls = []

        def now():
            calls.append(1)
            if len(calls) == 1:
                with open(self.log, 'a') as f:
                    f.write('Traceback (most recent call last):\n')
            return 1002.0
        with contextlib.redirect_stdout(io.StringIO()) as buf:
            self.assertEqual(swap.watch(self.root, 7, 'swap', 723, every=0, log=self.log, rounds=3, now=now), 2)
        last = json.loads(buf.getvalue().splitlines()[-1])
        self.assertEqual((last['verdict'], last['traceback']), ('ESCALATE', True))
        calls.clear()
        with contextlib.redirect_stdout(io.StringIO()) as buf:  # a later watch starts after that Traceback
            self.assertEqual(swap.watch(self.root, 7, 'recovery', 723, every=0, log=self.log, rounds=3,
                                        now=lambda: 1002.0), 0)

    def test_a_supervisor_exit_without_a_traceback_is_a_failure(self):
        """A supervisor that refuses (`stop:` or `refused:`, no Traceback) exits, and the loop logs it in
        supervise-restarts.log: no SUCCESS, and swap mode escalates on it."""
        self.guard(1001.0, 8)
        restarts = Path(self.tmp.name) / 'supervise-restarts.log'
        restarts.write_text('2026-11-20T10:00:00-05:00 supervise exited 0\n')  # before the watch: not counted
        calls = []

        def now():
            calls.append(1)
            if len(calls) == 1:
                with open(restarts, 'a') as f:
                    f.write('2026-11-20T17:00:00-05:00 supervise exited 2\n')
            return 1002.0
        with contextlib.redirect_stdout(io.StringIO()) as buf:
            code = swap.watch(self.root, 7, 'swap', 723, every=0, log=self.log, rounds=3, now=now, restarts=restarts)
        last = json.loads(buf.getvalue().splitlines()[-1])
        self.assertEqual((code, last['verdict']), (2, 'ESCALATE'))
        self.assertIn('supervise exited 2', last['failure'][0])
        calls.clear()
        with contextlib.redirect_stdout(io.StringIO()) as buf:  # recovery mode: shown, no SUCCESS, no escalation
            code = swap.watch(self.root, 7, 'recovery', 723, every=0, log=self.log, rounds=2, now=now,
                              restarts=restarts)
        self.assertEqual(code, 1)
        self.assertEqual(json.loads(buf.getvalue().splitlines()[-1])['verdict'], 'WAIT')

    def test_after_stop_records_t0_and_the_guard_id(self):
        self.guard(990.0, 123456789012345678)
        with contextlib.redirect_stdout(io.StringIO()) as buf:
            self.assertEqual(swap.main(['after-stop', str(self.root)]), 0)
        t0, gid = buf.getvalue().split()
        self.assertTrue(t0.startswith('T0=') and '.' in t0)
        self.assertEqual(gid, 'GID=123456789012345678')

    def test_recovery_mode_does_not_escalate(self):
        self.guard(990.0, 7)
        self.events({'event': 'guard-wait', 'chain': 'c2', 'unix': 500.0})
        code, state = self.run_watch(7, 500 + 1800 - 1023, mode='recovery')  # past the swap escalation time
        self.assertEqual((code, state['verdict']), (1, 'WAIT'))
        self.assertIsNone(state['escalate_in_s'])
        self.assertEqual(state['first_driver_exit_in_s'], 1023.0)
        self.assertEqual(self.run_watch(7, 2300.0, mode='recovery')[:1], (3,))  # DEADLINE: c2's driver exited
        self.guard(2000.0, 9)
        self.assertEqual(self.run_watch(7, 2001.0, mode='recovery')[1]['verdict'], 'SUCCESS')

    def test_unreadable_files_are_unknown_not_a_crash(self):
        (self.root / 'GUARD.json').write_text('{"unix": 10')
        code, state = self.run_watch(7, 1100.0)
        self.assertEqual((code, state['verdict']), (1, 'WAIT'))
        self.assertEqual(state['guard'], 'unknown (GUARD.json unreadable)')
        (self.root / 'chains' / 'c1' / 'ADMITTED.json').write_text('{"cou')
        self.assertEqual(self.run_watch(7, 1100.0)[1]['chains']['c1']['admitted'], 'unknown')


class CheckTailsLockTests(unittest.TestCase):
    def test_each_tail_is_read_under_its_index_lock(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            for name in ('c1', 'c2'):
                (root / 'chains' / name).mkdir(parents=True)
                (root / 'chains' / name / 'chain.index.jsonl').write_bytes(b'{"a": 1}\n')
            held = []
            real = retention.index_lock

            @contextlib.contextmanager
            def watching(chain_path):
                with real(chain_path):
                    held.append(Path(chain_path).parent.name)
                    yield
            with mock.patch.object(retention, 'index_lock', watching), contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(swap.check_tails(root), 0)
            self.assertEqual(held, ['c1', 'c2'])


@unittest.skipUnless(Path('/proc/self/cmdline').exists() and os.name != 'nt' and hasattr(os, 'pidfd_open'),
                     'needs /proc and pidfd')
class StepFileTests(unittest.TestCase):
    """The step file itself on Linux, with stand-in supervisors."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        t = Path(self.tmp.name)
        self.w5 = t / 'w5'
        self.w5.mkdir()
        self.root = t / 'oc'
        (self.root / 'plan' / 'p').mkdir(parents=True)
        self.plan = str(self.root / 'plan' / 'p' / 'CHAINS-PLAN.json')
        (t / 'records').mkdir()
        self.env = dict(os.environ, C=str(t), ROOT=str(self.root), PY=sys.executable, LOOP='10', PAUSE='3')
        self.procs = []

    def tearDown(self):
        for p in self.procs:
            try:
                os.killpg(p.pid, 9)
            except OSError:
                pass
            p.wait(10)
        self.tmp.cleanup()

    def spawn(self, argv):
        p = subprocess.Popen(argv, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.procs.append(p)
        return p

    def step(self, *args, timeout=120):
        run = subprocess.run(['bash', str(HERE.parent / 'supervise_swap.sh'), *args], env=self.env,
                             capture_output=True, text=True, timeout=timeout)
        return run.returncode, run.stdout + run.stderr

    def test_two_supervisors_refuse_and_nothing_is_signalled(self):
        (self.w5 / 'chains.py').write_text('import time\ntime.sleep(120)\n')
        a = self.spawn([sys.executable, str(self.w5 / 'chains.py'), 'supervise', '--plan', self.plan])
        b = self.spawn([sys.executable, str(self.w5 / 'chains.py'), 'supervise', '--plan', self.plan])
        time.sleep(1)
        code, out = self.step('stop_between_cycles', self.plan)
        self.assertEqual(code, 1, out)
        self.assertIn('STOP REFUSED: nothing signalled, nothing changed', out)
        self.assertIsNone(a.poll())
        self.assertIsNone(b.poll())
        code, out = self.step('stop_failing', self.plan)
        self.assertEqual(code, 0, out)
        self.assertIn('stopped', out)

    def test_never_asleep_means_never_signalled(self):
        """A supervisor that is always busy: its report keeps changing, but asleep never passes on three tries,
        so the stop refuses with nothing signalled (the loop never falls through to the kill)."""
        (self.w5 / 'chains.py').write_text('while True:\n    pass\n')
        sup = Path(self.plan).with_name('SUPERVISE.json')
        sup.write_text('{}')
        loop = self.spawn(['bash', '-c', swap.LOOP_TEXT, 'swap-loop', sys.executable, str(self.w5), self.plan, '10',
                           str(Path(self.env['C']) / 'records'), '3', ''])
        toucher = self.spawn(['bash', '-c', f'while true; do touch {sup}; sleep 1; done'])
        time.sleep(1.5)
        code, out = self.step('stop_between_cycles', self.plan, timeout=200)
        self.assertEqual(code, 1, out)
        self.assertIn('not asleep on three tries', out)
        self.assertIn('STOP REFUSED: nothing signalled, nothing changed', out)
        self.assertIsNone(loop.poll())
        toucher.kill()
        code, out = self.step('stop_failing', self.plan)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.step('pids', self.plan)[1].strip(), 'LOOP_PIDS ; PYTHON_PIDS')

    def test_a_supervisor_that_ignores_sigterm_is_killed_and_the_rescan_decides(self):
        (self.w5 / 'chains.py').write_text('import signal, time\nsignal.signal(signal.SIGTERM, signal.SIG_IGN)\n'
                                           'time.sleep(300)\n')
        self.spawn([sys.executable, str(self.w5 / 'chains.py'), 'supervise', '--plan', self.plan])
        time.sleep(1)
        code, out = self.step('stop_failing', self.plan)
        self.assertEqual(code, 0, out)
        self.assertIn('still running after round 1', out)
        self.assertIn('signal 9', out)
        self.assertEqual(self.step('pids', self.plan)[1].strip(), 'LOOP_PIDS ; PYTHON_PIDS')

    def test_start_loop_refuses_a_running_plan_and_stop_failing_an_idle_one(self):
        (self.w5 / 'chains.py').write_text('import time\ntime.sleep(120)\n')
        self.assertEqual(self.step('stop_failing', self.plan)[0], 1)
        self.spawn([sys.executable, str(self.w5 / 'chains.py'), 'supervise', '--plan', self.plan])
        time.sleep(1)
        code, out = self.step('start_loop', str(self.w5), self.plan)
        self.assertEqual(code, 1)
        self.assertIn('already runs', out)


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
