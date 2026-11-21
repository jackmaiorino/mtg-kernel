"""Waiting on the byte guard (tools/w5/chains.py). With FAST_GUARD_WAITS (set here explicitly, so these run on every
platform): a driver refused only at its ceiling in a fresh ok verdict looks again every second, any other refusal
every GUARD_POLL seconds up to GUARD_WAIT; `supervise --loop` starts its next cycle early (never sooner than
SUPERVISE_FLOOR) once a chain that may still run nears the ceiling of the fresh ok verdict it just wrote, and every
verdict reserves and admits exactly what it did before. Without it (Windows) the waits are main's. End to end where
it is on by default, a chain whose segments are much shorter than the loop never waits out the loop."""
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
from test_w5_chains import Node, STEP_C, STEP_X, jwrite  # noqa: E402

FAST_BY_DEFAULT = chains.FAST_GUARD_WAITS  # this platform's value, before any test patches it


class Halt(Exception):
    pass


def events(root, name):
    path = Path(root) / 'LOG.jsonl'
    rows = [json.loads(line) for line in path.read_text(encoding='utf-8').splitlines()] if path.exists() else []
    return [r for r in rows if r.get('event') == name]


class GuardWaitTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.node = Node(self.tmp.name)
        env = mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X),
                                           'FAKE_SLEEP': '0.01'})
        env.start()
        self.addCleanup(env.stop)
        fast = mock.patch.object(chains, 'FAST_GUARD_WAITS', True)
        fast.start()
        self.addCleanup(fast.stop)

    def tearDown(self):
        self.tmp.cleanup()

    def segments(self, plan, name):
        return len(json.loads(Path(plan['chains'][name]['manifest']).read_text())['segments'])

    def indep(self, c_wall=600.0):
        return self.node.plan('0', {'c1': {'arm': 'C', 'slot': 0, 'seed_root': 'oc/c1'}},
                              c_wall_seconds=c_wall, independent=True, label='indep')

    # ------------------------------------------------------------ the driver

    def test_missing_chain_keeps_the_normal_poll(self):
        self.node.stage0(c_wall=600.0)
        cdir = chains.chain_dir(self.node.root, 'c1')
        for ceilings, poll in (({'x1': 4}, chains.GUARD_POLL), ({'c1': 0}, chains.GUARD_CEILING_POLL)):
            with self.subTest(ceilings=ceilings):
                self.node.guard(ceilings=ceilings)
                ok, _, delay = chains.guard_admit(self.node.root, cdir, 'c1')
                self.assertFalse(ok)
                self.assertEqual(delay, poll)
                self.assertEqual(chains.admitted(cdir), 0)

    def test_driver_at_its_ceiling_looks_again_every_second(self):
        _, plan = self.node.stage0(c_wall=600.0)
        self.node.guard(ceilings={'c1': 0, 'x1': 0})
        sleeps = []

        def sleep(seconds):
            sleeps.append(seconds)
            if len(sleeps) == 1:
                self.node.guard(ceilings={'c1': 1, 'x1': 0})  # supervise's next verdict
            else:
                raise Halt

        with mock.patch.object(chains, 'time', wraps=time) as clock, self.assertRaises(Halt):
            clock.sleep.side_effect = sleep
            self.node.drive(plan, 'c1')
        self.assertEqual(sleeps, [chains.GUARD_CEILING_POLL] * 2)
        self.assertEqual(self.segments(plan, 'c1'), 1)
        self.assertEqual(len(events(self.node.root, 'guard-wait')), 2)  # one per wait

    def run_out(self, ok, ceilings):
        """Drives c1 on a fake clock until it exits; returns (exit code, sleeps, refusal reasons logged)."""
        _, plan = self.node.stage0(c_wall=10**6)
        self.node.guard(ok=ok, ceilings=ceilings)
        wall, sleeps = [time.time()], []

        def sleep(seconds):
            sleeps.append(seconds)
            wall[0] += seconds

        with mock.patch.object(chains, 'time', wraps=time) as clock:
            clock.time.side_effect = lambda: wall[0]
            clock.sleep.side_effect = sleep
            code = self.node.drive(plan, 'c1')
        self.assertEqual(self.segments(plan, 'c1'), 0)
        return code, sleeps, [r['reason'] for r in events(self.node.root, 'guard-wait') + events(self.node.root,
                                                                                                  'no-room')]

    def test_a_refusing_guard_still_waits_at_guard_poll_and_exits(self):
        code, sleeps, reasons = self.run_out(False, None)
        self.assertEqual(code, chains.EXIT_ROOM)
        self.assertEqual(set(sleeps), {chains.GUARD_POLL})
        self.assertEqual(len(sleeps), int(chains.GUARD_WAIT / chains.GUARD_POLL))
        self.assertEqual(len(reasons), 2)  # one guard-wait, one no-room
        self.assertIn('refuses', reasons[0])

    def test_a_ceiling_wait_turns_to_guard_poll_once_the_verdict_is_stale(self):
        code, sleeps, reasons = self.run_out(True, {'c1': 0, 'x1': 0})
        self.assertEqual(code, chains.EXIT_ROOM)
        fast = int(chains.GUARD_MAX_AGE / chains.GUARD_CEILING_POLL)
        self.assertEqual(set(sleeps[:fast]), {chains.GUARD_CEILING_POLL})
        self.assertEqual(set(sleeps[fast + 1:]), {chains.GUARD_POLL})
        self.assertAlmostEqual(sum(sleeps), chains.GUARD_WAIT, delta=chains.GUARD_POLL)
        self.assertIn('ceiling', reasons[0])
        self.assertIn('stale', reasons[1])

    # ------------------------------------------------------------ supervise

    def supervise(self, path, plan, script=(), after_cycle=None, cycles=3, loop='60', guard_ok=True):
        """Runs `supervise --loop` on a fake monotonic clock. `script`: (time, chain, count) admissions applied as
        the clock passes them. Returns per cycle (start time, verdict ceilings, census counts, next_bytes)."""
        clock_now, todo, out = [0.0], sorted(script), []
        real_cycle, real_guard = chains.cycle, retention.guard

        def sleep(seconds):
            clock_now[0] += seconds
            while todo and todo[0][0] <= clock_now[0]:
                _, n, count = todo.pop(0)
                chains.write(chains.chain_dir(plan['root'], n) / 'ADMITTED.json', {'count': count})

        def cycle(*args, **kwargs):
            census = {n: chains.admitted(chains.chain_dir(plan['root'], n)) for n in plan['chains']}
            report = real_cycle(*args, **kwargs)
            g = json.loads((Path(plan['root']) / 'GUARD.json').read_text())
            out.append((clock_now[0], g['ceilings'], census, g['next_bytes']))
            if after_cycle:
                after_cycle(g)
            if len(out) >= cycles:
                raise Halt
            return report

        def guard(*args, **kwargs):
            return dict(real_guard(*args, **kwargs), ok=guard_ok)

        argv = ['supervise', '--plan', str(path)] + (['--loop', loop] if loop else [])
        with mock.patch.object(chains, 'time', wraps=time) as clock, \
                mock.patch.object(chains, 'cycle', side_effect=cycle), \
                mock.patch.object(chains.retention, 'guard', side_effect=guard), \
                mock.patch.object(chains.subprocess, 'Popen'), contextlib.redirect_stdout(io.StringIO()):
            clock.monotonic.side_effect = lambda: clock_now[0]
            clock.sleep.side_effect = sleep
            try:
                code = chains.main(argv)
            except Halt:
                code = None
        return code, out

    def test_supervise_wakes_early_and_each_verdict_reserves_the_same(self):
        path, plan = self.indep()
        reserve = chains.guard_reservation(plan, plan['root'])
        _, out = self.supervise(path, plan, script=[(5.0, 'c1', 3)])
        self.assertEqual([t for t, *_ in out], [0.0, 5.0, 65.0])  # woken one admission from the ceiling
        for _, ceilings, census, next_bytes in out:
            self.assertEqual(ceilings, {'c1': census['c1'] + chains.GUARD_SEGMENTS})
            self.assertEqual(next_bytes, reserve)
        self.assertEqual([c['c1'] for _, c, _, _ in out], [4, 7, 7])

    def test_supervise_sleeps_the_loop_for_refused_ended_or_stopped_chains(self):
        path, plan = self.indep()
        _, out = self.supervise(path, plan, script=[(5.0, 'c1', 4)], guard_ok=False)
        self.assertEqual([t for t, *_ in out], [0.0, 60.0, 120.0])
        cdir = chains.chain_dir(plan['root'], 'c1')
        (cdir / 'ADMITTED.json').unlink()

        def age(g):  # a verdict older than GUARD_MAX_AGE (its supervise stalled): slept out, not woken on
            jwrite(Path(plan['root']) / 'GUARD.json', dict(g, unix=time.time() - chains.GUARD_MAX_AGE - 1))

        _, out = self.supervise(path, plan, script=[(5.0, 'c1', 4)], after_cycle=age, cycles=2)
        self.assertEqual([t for t, *_ in out], [0.0, 60.0])
        for marker in ('STOP', 'END.json'):  # written after the verdict that still names the chain
            (cdir / 'ADMITTED.json').unlink()

            def mark(g, marker=marker):
                if not (cdir / marker).exists():
                    self.assertIn('c1', g['ceilings'])
                    (cdir / marker).write_text('{"reason": "operator", "unix": 0}')

            _, out = self.supervise(path, plan, script=[(5.0, 'c1', 4)], after_cycle=mark, cycles=2)
            self.assertEqual([t for t, *_ in out], [0.0, 60.0], marker)
            (cdir / marker).unlink()

    def test_supervise_floor_between_cycles(self):
        path, plan = self.indep()

        def exhaust(g):  # a chain that is always at its ceiling again right after a cycle
            chains.write(chains.chain_dir(plan['root'], 'c1') / 'ADMITTED.json', {'count': g['ceilings']['c1']})

        _, out = self.supervise(path, plan, after_cycle=exhaust, cycles=4)
        self.assertEqual([t for t, *_ in out], [0.0, 2.0, 4.0, 6.0])
        _, out = self.supervise(path, plan, after_cycle=exhaust, cycles=3, loop='0.5')  # loop stays the bound
        self.assertEqual([t for t, *_ in out], [0.0, 0.5, 1.0])

    def test_admissions_between_census_and_byte_count_stay_inside_the_reservation(self):
        """A driver admits under the previous verdict while a cycle runs between its census and its byte count, then
        under the new verdict: after the census at most GUARD_SEGMENTS more are admitted, so with the one in flight at
        most GUARD_SEGMENTS + 1 segments are written past the counted bytes, which the reservation covers."""
        path, plan = self.indep()
        root, cdir = Path(plan['root']), chains.chain_dir(plan['root'], 'c1')
        c, f = (plan['arms'][a]['segment_updates'] * plan['arms'][a]['bytes_per_update'] for a in ('C', 'F'))
        real_sample = chains.sample_cycle

        def admit_all():
            n = 0
            while chains.guard_admit(root, cdir, 'c1')[0]:
                n += 1
            return n

        for start in (0, 2):
            with self.subTest(start=start), contextlib.redirect_stdout(io.StringIO()), \
                    mock.patch.object(chains.subprocess, 'Popen'):
                chains.write(cdir / 'ADMITTED.json', {'count': 0})
                chains.cycle(plan, path)  # the previous verdict: ceiling 4
                chains.write(cdir / 'ADMITTED.json', {'count': start})  # admitted under it before the next census
                gap = []

                def sample(*args, **kwargs):  # runs after the census, before retention and the byte count
                    gap.append(admit_all())  # GUARD.json on disk is still the previous verdict
                    return real_sample(*args, **kwargs)

                with mock.patch.object(chains, 'sample_cycle', side_effect=sample):
                    chains.cycle(plan, path)
                verdict = json.loads((root / 'GUARD.json').read_text())
                self.assertEqual(verdict['ceilings'], {'c1': start + chains.GUARD_SEGMENTS})
                self.assertEqual(gap, [chains.GUARD_SEGMENTS - start])
                after = admit_all()  # under the new verdict
                self.assertEqual(gap[0] + after, chains.GUARD_SEGMENTS)
                self.assertLessEqual(1 + gap[0] + after, chains.GUARD_SEGMENTS + 1)
                self.assertEqual(verdict['next_bytes'], (chains.GUARD_SEGMENTS + 1) * c + 2 * f)

    def wait(self, plan, verdict=None, guard_text=None, admitted_text=None, loop=60.0):
        """chains.supervise_wait on a fake clock (monotonic and wall advance with each sleep). `verdict` (its `unix`
        relative to the start) or raw `guard_text` is GUARD.json; `admitted_text` is c1's ADMITTED.json. Returns
        (the chain that woke it, seconds slept)."""
        root, now, t0 = Path(plan['root']), [0.0], time.time()
        (root / 'GUARD.json').unlink(missing_ok=True)
        if verdict is not None:
            jwrite(root / 'GUARD.json', dict(verdict, unix=t0 + verdict['unix']))
        elif guard_text is not None:
            (root / 'GUARD.json').write_text(guard_text)
        adm = chains.chain_dir(root, 'c1') / 'ADMITTED.json'
        adm.unlink(missing_ok=True)
        if admitted_text is not None:
            adm.parent.mkdir(parents=True, exist_ok=True)
            adm.write_text(admitted_text)

        def sleep(seconds):
            now[0] += seconds

        with mock.patch.object(chains, 'time', wraps=time) as clock:
            clock.monotonic.side_effect = lambda: now[0]
            clock.time.side_effect = lambda: t0 + now[0]
            clock.sleep.side_effect = sleep
            woke = chains.supervise_wait(plan, root, loop)
        return woke, now[0]

    def test_supervise_wait_reads_only_a_fresh_ok_verdict_of_its_plan(self):
        _, plan = self.indep()
        ours = {'ok': True, 'plan': plan['label'], 'unix': 0.0, 'ceilings': {'c1': 4}}
        three = '{"count": 3}'
        self.assertEqual(self.wait(plan, ours, admitted_text=three), ('c1', 2.0))  # control: one from the ceiling
        self.assertEqual(self.wait(plan, dict(ours, plan='another'), admitted_text=three), (None, 60.0))
        self.assertEqual(self.wait(plan, None, admitted_text=three), (None, 60.0))  # no GUARD.json
        self.assertEqual(self.wait(plan, guard_text='{"ok": true, "pl', admitted_text=three), (None, 60.0))
        self.assertEqual(self.wait(plan, ours, admitted_text=None), (None, 60.0))  # no ADMITTED.json: count 0
        self.assertEqual(self.wait(plan, ours, admitted_text='{"cou'), (None, 60.0))  # truncated: skipped
        # Stale during the wait: fresh at entry (aged 899 s), stale by the first look at 2 s, so no early wake.
        age = chains.GUARD_MAX_AGE
        self.assertEqual(self.wait(plan, dict(ours, unix=-(age - 1)), admitted_text=three), (None, 60.0))
        self.assertEqual(self.wait(plan, dict(ours, unix=-(age - 3)), admitted_text=three), ('c1', 2.0))

    def test_one_shot_supervise_is_unchanged(self):
        path, plan = self.indep()
        with mock.patch.object(chains, 'supervise_wait', side_effect=AssertionError('no wait')):
            code, out = self.supervise(path, plan, cycles=2, loop=None)
        self.assertEqual((code, len(out)), (0, 1))

    def test_without_fast_guard_waits_the_waits_are_mains(self):
        with mock.patch.object(chains, 'FAST_GUARD_WAITS', False):
            code, sleeps, reasons = self.run_out(True, {'c1': 0, 'x1': 0})
            self.assertEqual(code, chains.EXIT_ROOM)
            self.assertEqual(sleeps, [chains.GUARD_POLL] * int(chains.GUARD_WAIT / chains.GUARD_POLL))
            self.assertIn('ceiling', reasons[0])

    def test_without_fast_guard_waits_supervise_sleeps_the_loop(self):
        path, plan = self.indep()
        with mock.patch.object(chains, 'FAST_GUARD_WAITS', False), \
                mock.patch.object(chains, 'supervise_wait', side_effect=AssertionError('no early wake')):
            _, out = self.supervise(path, plan, script=[(5.0, 'c1', 3)])
        self.assertEqual([t for t, *_ in out], [0.0, 60.0, 120.0])

    # ------------------------------------------------------------ end to end

    @unittest.skipUnless(FAST_BY_DEFAULT, 'the faster guard waits are off on this platform (Windows)')
    def test_short_segments_never_wait_out_the_loop(self):
        """Real processes: `supervise --loop 60` and a driver whose segments take well under a second. Before this
        change the fourth segment waited for the next verdict, a loop later; now every wait is a few seconds."""
        path, plan = self.indep()
        c = plan['chains']['c1']
        self.node.ledger_source('c1')
        tool = str(Path(chains.__file__).resolve())
        logs = Path(self.tmp.name)
        with open(logs / 'sup.log', 'wb') as sup_log, open(logs / 'drive.log', 'wb') as drive_log:
            sup = subprocess.Popen([sys.executable, tool, 'supervise', '--plan', str(path), '--loop', '60'],
                                   stdout=sup_log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL)
            drive = None
            try:
                deadline = time.time() + 30
                while not (self.node.root / 'GUARD.json').exists() and time.time() < deadline:
                    time.sleep(0.1)
                drive = subprocess.Popen([sys.executable, tool, '_drive', '--arm-file',
                                          plan['arms']['C']['file']['path'], '--chain', 'c1', '--seed-root',
                                          c['seed_root'], '--state', str(self.node.root / 'state' / 'c1'),
                                          '--ledger-source', str(self.node.root / 'state' / 'c1' /
                                                                 'ledger-source.json')],
                                         stdout=drive_log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL)
                want, deadline = 3 * chains.GUARD_SEGMENTS, time.time() + 90
                while self.segments(plan, 'c1') < want and time.time() < deadline and drive.poll() is None:
                    time.sleep(0.2)
                done = self.segments(plan, 'c1')
                (chains.chain_dir(self.node.root, 'c1') / 'STOP').write_text('')
                drive.wait(timeout=60)
            finally:
                for proc in (drive, sup):
                    if proc is not None and proc.poll() is None:
                        proc.kill()
                        proc.wait()
        self.assertGreaterEqual(done, want, (logs / 'drive.log').read_text(errors='replace')[-2000:])
        rows = [json.loads(line) for line in (self.node.root / 'LOG.jsonl').read_text().splitlines()]
        waits = []
        for i, row in enumerate(rows):
            if row.get('event') == 'guard-wait':
                nxt = next((r for r in rows[i + 1:] if r.get('chain') == 'c1' and r['event'] in ('segment', 'stop')),
                           None)
                if nxt is not None:
                    waits.append(nxt['unix'] - row['unix'])
        # Early replenishment can let every segment start without waiting.
        for elapsed in waits:
            self.assertLess(elapsed, 15.0, waits)


if __name__ == '__main__':
    unittest.main()
