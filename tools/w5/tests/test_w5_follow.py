"""Followed trainers (chains.py arm key `follow`, mtg-kernel `--follow SPOOL`) with the stand-in trainer: the plan key
stays out of the recipe identity; a followed chain runs its segments in one process and reaches the plain chain's
states and records; STOP ends a waiting process at every driver exit; a refused config and a crash in a handed
segment fall back to new processes with the same results; the C wall still kills a segment in flight; the commit
lines name each segment's process. On Linux, a followed scratch chain matches the plain chain boundary for
boundary (test_w5_scratch_short's harness)."""
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
import test_w5_scratch_short  # noqa: E402


def follow_plan(node, follow=True, segment_updates=4, c_wall=600.0, label=None):
    """A Stage 0 independent plan with one C chain (c1, seed root oc/c1) in the node's root."""
    inputs = json.loads(Path(node.inputs('0', {'c1': {'arm': 'C', 'slot': 0, 'seed_root': 'oc/c1'}},
                                         label=label or 'follow', c_wall_seconds=c_wall,
                                         independent=True)).read_text())
    inputs['arms']['C'] = dict(inputs['arms']['C'], segment_updates=segment_updates)
    if follow:
        inputs['arms']['C']['follow'] = True
    return chains.make_plan(jwrite(node.t / f'inputs-{label or "follow"}.json', inputs))[1]


class FollowPlan(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)

    def test_follow_is_an_optional_c_arm_key_outside_the_recipe_identity(self):
        a, b = Path(self.tmp.name) / 'a', Path(self.tmp.name) / 'b'
        a.mkdir()
        b.mkdir()
        plain, followed = follow_plan(Node(a), follow=False), follow_plan(Node(b))
        plain_arm = chains.load_arm(plain['arms']['C']['file']['path'])
        followed_arm = chains.load_arm(followed['arms']['C']['file']['path'])
        self.assertNotIn('follow', plain_arm)
        self.assertIs(followed_arm['follow'], True)
        strip = lambda arm: {k: v for k, v in chains.recipe_identity(arm).items() if k != 'base' and k != 'parent'}
        self.assertEqual(strip(plain_arm), strip(followed_arm))
        self.assertEqual(chains.recipe_identity(plain_arm).keys(), chains.recipe_identity(followed_arm).keys())
        node = Node(Path(self.tmp.name))
        for arm, value, why in (('X8', True, 'C-recipe'), ('F', True, 'C-recipe'), ('C', 'yes', 'true or absent'),
                                ('C', 'resident', 'resident')):
            inputs = json.loads(Path(node.inputs('0', {'c1': {'arm': 'C', 'slot': 0, 'pair': 1, 'seed_root': 'w5/q1'},
                                                       'x1': {'arm': 'X8', 'slot': 1, 'pair': 1,
                                                              'seed_root': 'w5/q1'},
                                                       'f2': {'arm': 'F', 'slot': 2, 'seed_root': 'w5/qf2'}},
                                                 c_wall_seconds=600.0, label=f'bad-{arm.lower()}')).read_text())
            if value == 'resident':
                inputs['arms'][arm]['verified_reads'] = 'resident'
                value = True
            inputs['arms'][arm]['follow'] = value
            with self.subTest(arm=arm, why=why), self.assertRaisesRegex(chains.Stop, why):
                chains.make_plan(jwrite(node.t / f'inputs-bad-{arm.lower()}.json', inputs))


class FollowDrive(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        env = mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X),
                                           'FAKE_SLEEP': '0.01'})
        env.start()
        self.addCleanup(env.stop)
        for name, value in (('GUARD_WAIT', 0.0), ('GUARD_POLL', 0.0), ('GUARD_CEILING_POLL', 0.0)):
            p = mock.patch.object(chains, name, value)
            p.start()
            self.addCleanup(p.stop)

    def node(self, name):
        (Path(self.tmp.name) / name).mkdir()
        return Node(Path(self.tmp.name) / name)

    def drive(self, node, plan, admissions):
        """Drives c1 until the guard's ceiling refuses its next segment (EXIT_ROOM)."""
        node.guard(ceilings={'c1': admissions})
        return node.drive(plan, 'c1')

    def rows(self, plan):
        _, records = retention.index(plan['chains']['c1']['manifest'])
        return [(r['boundary'], r['after_state_sha256']) for r in records]

    def events(self, node, event):
        lines = (node.root / 'LOG.jsonl').read_text().splitlines()
        return [row for row in map(json.loads, lines) if row.get('event') == event]

    def records(self, node, plan):
        """Every boundary's receipts (complete.json, update.json, collection.json) and every segment config, with the
        node's directory replaced by a placeholder and the ledger's wall-clock time dropped."""
        man = json.loads(Path(plan['chains']['c1']['manifest']).read_text())
        out = []
        for seg in man['segments']:
            docs = [json.loads(Path(seg['config']).read_text())]
            for it in sorted((Path(seg['root']) / 'iterations').glob('*/complete.json')):
                complete = json.loads(it.read_text())
                att = chains.attempt_dir(it.parent, complete)
                self.assertEqual(complete['update'].pop('sha256'), retention.sha(att / 'update' / 'update.json'))
                docs += [complete, json.loads((att / 'update' / 'update.json').read_text()),
                         json.loads((att / 'collect' / 'collection.json').read_text())]
            doc = json.loads(json.dumps(docs, sort_keys=True).replace(node.t.as_posix(), '<node>'))
            for d in doc:
                d.get('chain_ledger', {}).pop('unix_seconds', None)
            out.append(doc)
        return out

    def commits(self, node):
        return [r for r in chains.read_commits(node.root / 'chains' / 'c1')[0] if r['event'] == 'commit']

    def plain_rows(self, segments):
        node = self.node('plain')
        plan = follow_plan(node, follow=False)
        self.assertEqual(self.drive(node, plan, segments), chains.EXIT_ROOM)
        commits = self.commits(node)
        self.assertTrue(commits and not any('trainer_pid' in r for r in commits))  # plain rows are unchanged
        self.plain_records = self.records(node, plan)
        return self.rows(plan)

    def test_a_followed_chain_runs_its_segments_in_one_process_with_the_plain_chains_states(self):
        plain = self.plain_rows(3)
        node = self.node('followed')
        plan = follow_plan(node)
        with mock.patch.object(chains, 'kill_group', wraps=chains.kill_group) as kill:
            self.assertEqual(self.drive(node, plan, 3), chains.EXIT_ROOM)
        self.assertEqual(self.rows(plan), plain)
        self.assertEqual(len(plain), 12)
        self.assertEqual(self.records(node, plan), self.plain_records)  # configs and receipts, document for document
        commits = self.commits(node)
        self.assertEqual([r['segment'] for r in commits], [0, 1, 2])
        self.assertEqual(len({r['trainer_pid'] for r in commits}), 1)
        self.assertEqual([r['trainer_run'] for r in commits], [0, 1, 2])
        self.assertEqual([r['trainer_run'] for r in self.events(node, 'segment-done')], [0, 1, 2])
        kill.assert_not_called()  # STOP ended the waiting process
        self.assertEqual(list((node.root / 'chains' / 'c1' / chains.FOLLOW_SPOOLS).iterdir()), [])
        # Every config the driver handed is the segment's config file, byte for byte (no follow key anywhere).
        for seg in json.loads(Path(plan['chains']['c1']['manifest']).read_text())['segments']:
            self.assertFalse([k for k in json.loads(Path(seg['config']).read_text()) if 'follow' in k])
        # A later driver start runs a new process and continues with the same states.
        self.assertEqual(self.drive(node, plan, 5), chains.EXIT_ROOM)
        commits = self.commits(node)
        self.assertEqual([r['trainer_run'] for r in commits], [0, 1, 2, 0, 1])
        self.assertEqual(len({r['trainer_pid'] for r in commits[3:]}), 1)
        self.assertNotEqual(commits[3]['trainer_pid'], commits[0]['trainer_pid'])
        self.assertEqual(len(list((node.root / 'chains' / 'c1' / chains.FOLLOW_SPOOLS).iterdir())), 0)
        self.assertEqual(self.rows(plan)[:12], plain)

    def test_a_refused_config_falls_back_to_plain_processes_with_the_same_states(self):
        plain = self.plain_rows(3)
        node = self.node('refused')
        plan = follow_plan(node)
        with mock.patch.dict(os.environ, {'FAKE_FOLLOW_REFUSE': '1'}), \
                mock.patch.object(chains, 'run_trainer', wraps=chains.run_trainer) as fresh:
            self.assertEqual(self.drive(node, plan, 4), chains.EXIT_ROOM)  # one admission more: the refusal
        self.assertEqual(self.rows(plan), plain)
        fallbacks = self.events(node, 'follow-fallback')
        self.assertEqual([(r['segment'], r['exit'], r['refused']) for r in fallbacks],
                         [(1, chains.FOLLOW_REFUSED, True)])
        self.assertEqual(fresh.call_count, 2)  # segments 1 and 2 in plain processes
        self.assertEqual([r.get('trainer_run') for r in self.commits(node)], [0, None, None])
        self.assertFalse((node.root / 'chains' / 'c1' / 'interrupted').exists())  # nothing of it ran
        self.assertEqual(self.records(node, plan), self.plain_records)

    def test_a_crash_in_a_handed_segment_reruns_it_in_a_new_process(self):
        plain = self.plain_rows(3)
        node = self.node('crashed')
        plan = follow_plan(node)
        with mock.patch.dict(os.environ, {'FAKE_FOLLOW_FAIL_AT': '5'}):  # update 5: segment 1, followed runs only
            self.assertEqual(self.drive(node, plan, 4), chains.EXIT_ROOM)
        self.assertEqual(self.rows(plan), plain)
        self.assertEqual([(r['segment'], r['exit'], r['refused']) for r in self.events(node, 'follow-fallback')],
                         [(1, 1, False)])
        commits = self.commits(node)
        self.assertEqual([r['trainer_run'] for r in commits], [0, 0, 1])  # the rerun's process keeps following
        self.assertEqual(self.records(node, plan), self.plain_records)
        # Segment 1's attempts: the handed run (its first iteration completed) and the new process from iteration 1.
        attempts = commits[1]['trainer_attempts']
        self.assertEqual([(a['trainer_run'], a['trainer_from_iteration']) for a in attempts], [(1, 0), (0, 1)])
        self.assertEqual(attempts[0]['trainer_pid'], commits[0]['trainer_pid'])
        self.assertEqual(commits[1]['trainer_pid'], commits[2]['trainer_pid'])
        self.assertNotEqual(commits[0]['trainer_pid'], commits[1]['trainer_pid'])
        moved = list((node.root / 'chains' / 'c1' / 'interrupted').iterdir())
        self.assertEqual(len(moved), 1)  # the crashed iteration, moved aside as at a driver start

    def test_a_crash_in_a_first_run_is_the_usual_failure(self):
        node = self.node('failed')
        plan = follow_plan(node)
        with mock.patch.dict(os.environ, {'FAKE_FAIL_AT': '5'}):
            self.assertEqual(self.drive(node, plan, 4), chains.EXIT_FAILED)
        # Segment 1 failed in its handed run, fell back, and failed again in the new process's first run.
        self.assertEqual(len(self.events(node, 'follow-fallback')), 1)
        failure = json.loads((node.root / 'chains' / 'c1' / 'FAILURE.json').read_text())
        self.assertEqual((failure['segment'], failure['exit'], failure['boundary']), (1, 1, 5))

    def test_the_c_wall_kills_a_followed_segment_in_flight(self):
        node = self.node('wall')
        plan = follow_plan(node, c_wall=600.0)
        node.guard(ceilings={'c1': 10})
        # The clock passes the wall as soon as the second segment is handed (each update sleeps 0.2 s, so the
        # segment is in flight).
        jump, real_time, hand, spool = [0.0], time.time, chains.Follower.hand, []

        def handed(follower, config_path):
            hand(follower, config_path)
            spool.append(follower.spool)

        def clock_time():
            if spool and not (spool[0] / 'next.json').exists():  # the trainer has taken the config: in flight
                jump[0] = 10.0**6
            return real_time() + jump[0]

        with mock.patch.dict(os.environ, {'FAKE_SLEEP': '0.2'}), \
                mock.patch.object(chains, 'time', wraps=time) as clock, \
                mock.patch.object(chains.Follower, 'hand', autospec=True, side_effect=handed), \
                mock.patch.object(chains, 'kill_group', wraps=chains.kill_group) as kill:
            clock.time.side_effect = clock_time
            self.assertEqual(node.drive(plan, 'c1'), 0)
        self.assertEqual(kill.call_count, 1)  # the wall's kill of the segment in flight; nothing else
        end = json.loads((node.root / 'chains' / 'c1' / 'END.json').read_text())
        self.assertEqual(end['reason'], 'c-wall')
        commits = self.commits(node)
        self.assertEqual(commits[0]['how'], 'native')
        self.assertTrue(all(r['how'] == 'native-cut' for r in commits[1:]))
        self.assertTrue(4 <= end['last_valid_boundary'] < 8)
        self.assertEqual(len(self.rows(plan)), end['last_valid_boundary'])
        self.assertFalse((node.root / 'chains' / 'c1' / 'FAILURE.json').exists())

    def test_an_exception_while_a_segment_is_in_flight_kills_without_the_stop_grace(self):
        node = self.node('raise')
        plan = follow_plan(node)
        node.guard(ceilings={'c1': 10})
        hand = chains.Follower.hand

        def failing(follower, config_path):
            hand(follower, config_path)
            raise OSError('directory sync failed')

        started = time.time()
        with mock.patch.object(chains.Follower, 'hand', autospec=True, side_effect=failing),                 mock.patch.object(chains, 'kill_group', wraps=chains.kill_group) as kill,                 self.assertRaisesRegex(OSError, 'directory sync failed'):
            node.drive(plan, 'c1')
        self.assertEqual(kill.call_count, 1)
        self.assertLess(time.time() - started, chains.FOLLOW_STOP_GRACE / 3)
        # An exception abandons the process: nothing is written or deleted in the tree (the next start clears it).
        spools = list((node.root / 'chains' / 'c1' / chains.FOLLOW_SPOOLS).iterdir())
        self.assertEqual(len(spools), 1)
        self.assertFalse((spools[0] / 'STOP').exists())

    def test_a_refused_carried_tree_kills_the_trainer_and_writes_nothing_in_the_tree(self):
        node = self.node('carried')
        plan = follow_plan(node)
        node.guard(ceilings={'c1': 10})
        calls = [0]

        def held(man, root, chain):
            calls[0] += 1
            return calls[0] < 2  # the second check (after the second segment's run) finds the tree suspect

        with mock.patch.object(chains, 'carried_tree_held', side_effect=held),                 mock.patch.object(chains, 'kill_group', wraps=chains.kill_group) as kill:
            self.assertEqual(node.drive(plan, 'c1'), chains.EXIT_STOP)
        self.assertEqual(kill.call_count, 1)  # a kill, not STOP
        spools = list((node.root / 'chains' / 'c1' / chains.FOLLOW_SPOOLS).iterdir())
        self.assertEqual(len(spools), 1)  # left as it was: not removed
        self.assertFalse((spools[0] / 'STOP').exists())  # and no STOP written into it
        self.assertEqual([r['segment'] for r in self.commits(node)], [0])  # no commit for the refused segment

    def test_stop_is_never_written_through_a_link(self):
        node = self.node('link')
        plan = follow_plan(node)
        node.guard(ceilings={'c1': 1})
        victim = node.t / 'victim.bin'  # absent: a dangling link, which a plain create would follow and create
        start = chains.Follower.start

        def planting(follower, config_path):
            start(follower, config_path)
            try:
                os.symlink(victim, follower.spool / 'STOP')
            except (OSError, NotImplementedError):
                self.skipTest('no symlinks here')

        with mock.patch.object(chains.Follower, 'start', autospec=True, side_effect=planting),                 mock.patch.object(chains, 'kill_group', wraps=chains.kill_group) as kill:
            self.assertEqual(node.drive(plan, 'c1'), chains.EXIT_ROOM)
        self.assertFalse(victim.exists())  # nothing was written through the link
        self.assertEqual(kill.call_count, 1)  # no safe STOP: the waiting process is killed instead

    def test_stop_between_segments_ends_a_waiting_process_with_stop(self):
        node = self.node('stop')
        plan = follow_plan(node)
        node.guard(ceilings={'c1': 10})
        calls = [0]
        real = chains.stopped

        def stopped(root, cdir):
            calls[0] += 1
            return calls[0] > 2 or real(root, cdir)

        with mock.patch.object(chains, 'stopped', side_effect=stopped), \
                mock.patch.object(chains, 'kill_group', wraps=chains.kill_group) as kill:
            self.assertEqual(node.drive(plan, 'c1'), chains.EXIT_STOP)
        kill.assert_not_called()
        self.assertEqual([r['trainer_run'] for r in self.commits(node)], [0, 1])


@unittest.skipUnless(sys.platform.startswith('linux'), 'scratch segments are Linux only (rename onto an empty directory)')
class FollowedScratch(test_w5_scratch_short.ShortScratchSegments):
    """test_w5_scratch_short's harness with a followed trainer: the persist of each segment runs while the trainer
    waits, before the next config is handed over."""

    def chain(self, name, length, scratch, segments, follow=False):
        """test_w5_scratch_short's chain, with `follow` on the arm when asked."""
        (Path(self.tmp.name) / name).mkdir()
        node = Node(Path(self.tmp.name) / name)
        inputs = json.loads(Path(node.inputs('0', {'c1': {'arm': 'C', 'slot': 0, 'seed_root': 'oc/c1'}},
                                             label='short', c_wall_seconds=segments - 0.5,
                                             independent=True)).read_text())
        inputs['arms']['C'] = dict(inputs['arms']['C'], segment_updates=length)
        if scratch:
            inputs['scratch_root'] = (node.t / 'shm').as_posix()
            inputs['arms']['C']['scratch'] = True
        if follow:
            inputs['arms']['C']['follow'] = True
        _, plan = chains.make_plan(jwrite(node.t / 'inputs-short.json', inputs))
        node.guard(ceilings={'c1': 10**6})
        return node, plan, [time.time()]

    def drive(self, node, plan, wall, held=None):
        run = chains.Follower.run

        def run_segment(follower, *args, **kwargs):
            self.prune(node, plan)
            if held is not None:
                latest, states = self.held(plan)
                held[latest] = states
            result = run(follower, *args, **kwargs)
            wall[0] += 1.0
            return result

        with mock.patch.object(chains.Follower, 'run', autospec=True, side_effect=run_segment):
            return super().drive(node, plan, wall, held)

    def test_every_boundary_and_retention_decision_matches_the_plain_chain(self):
        length, updates = 4, 64
        segments = updates // length
        p_node, p_plan, p_held = self.plain(length, segments)
        s_node, s_plan, s_wall = self.chain('followed-scratch', length, True, segments, follow=True)
        s_held = {}
        self.assertEqual(self.drive(s_node, s_plan, s_wall, s_held), 0)
        self.assertEqual(self.finish(s_node, s_plan, s_held), updates)
        self.assertEqual(self.records(s_node, s_plan), self.records(p_node, p_plan))
        self.check_held(length, s_held, p_held)
        commits = [r for r in chains.read_commits(s_node.root / 'chains' / 'c1')[0] if r['event'] == 'commit']
        self.assertEqual([r['trainer_run'] for r in commits], list(range(segments)))
        self.assertEqual({r['how'] for r in commits}, {'scratch'})

    def test_each_segment_is_persisted_before_the_next_config_is_handed_over(self):
        """A waiting trainer must not start segment k+1 while segment k's scratch tree is still being copied,
        unmounted and renamed (drive_locked: persist in the completion branch, then the loop's next pass hands the
        next config over): persist(k) has returned before next.json for k+1 exists, for every k."""
        length, segments = 4, 4
        node, plan, wall = self.chain('order', length, True, segments, follow=True)
        events, persist, hand = [], chains.persist, chains.Follower.hand

        def persisting(arm, native, cdir):
            spools = Path(cdir) / chains.FOLLOW_SPOOLS
            pending = [s for s in spools.iterdir() if (s / 'next.json').exists()] if spools.exists() else []
            self.assertEqual(pending, [], f'next.json published while {Path(native).name} is not yet persisted')
            events.append(('persist-start', Path(native).name))
            result = persist(arm, native, cdir)
            events.append(('persist-end', Path(native).name))
            return result

        def handing(follower, config_path):
            events.append(('hand', Path(config_path).stem))
            hand(follower, config_path)

        with mock.patch.object(chains, 'persist', side_effect=persisting), \
                mock.patch.object(chains.Follower, 'hand', autospec=True, side_effect=handing):
            self.assertEqual(self.drive(node, plan, wall), 0)
        order = [e for e in events if e[0] in ('persist-end', 'hand')]
        self.assertEqual(order, [x for s in range(segments) for x in
                                 ([('persist-end', f'seg-{s:05d}')] +
                                  ([('hand', f'seg-{s + 1:05d}')] if s + 1 < segments else []))])

    def test_interruptions_around_promotion_and_the_commit_line(self):
        self.skipTest('covered by test_w5_scratch_short; the followed case is the test above')

    def test_restart_inside_an_unfinished_short_segment(self):
        self.skipTest('covered by test_w5_scratch_short')


if __name__ == '__main__':
    unittest.main()
