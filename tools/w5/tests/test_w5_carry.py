"""W5 carried continuations (tools/w5/chains.py `carry`, the `carry` chain input): a chain that continues another C
chain from its persisted boundary B under a new name and root keeps the global update numbering, the seed root and
the wall, so with the same recipe (only the trainer build differs) it reaches exactly the states the old chain would
have reached. Stand-in trainer (fake_trainer.py), as test_w5_chains.py."""
import contextlib
import io
import json
import os
import shutil
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
import w5box  # noqa: E402
from test_w5_chains import FAKE, STEP_C, Node, jwrite  # noqa: E402

SEED = 'oc/c1'
PATH_KEYS = ('output_directory',)


def after_states(native):
    return [json.loads(Path(json.loads(p.read_text())['update']['path']).read_text())['after_state_sha256']
            for p in sorted((Path(native) / 'iterations').glob('*/complete.json'))]


class CarryTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.node = Node(self.tmp.name)
        self.t = self.node.t
        env = mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_SLEEP': '0.0'})
        env.start()
        self.addCleanup(env.stop)
        for name, value in (('GUARD_WAIT', 0.0), ('GUARD_POLL', 0.0)):
            p = mock.patch.object(chains, name, value)
            p.start()
            self.addCleanup(p.stop)

    def tearDown(self):
        self.tmp.cleanup()

    # ------------------------------------------------------------ helpers

    def old(self, segments, c_wall=600.0, length=4, stop=True):
        """The old chain c1 (`length`-update segments) run for `segments` segments, then held by its STOP file (as
        the operator's stop step leaves it); returns (plan, manifest path)."""
        doc = json.loads(Path(self.node.inputs('0', {'c1': {'arm': 'C', 'slot': 0, 'seed_root': SEED}},
                                               label='old', c_wall_seconds=c_wall, independent=True)).read_text())
        doc['arms']['C']['segment_updates'] = length
        _, plan = chains.make_plan(jwrite(self.t / 'inputs-old.json', doc))
        self.node.guard(ceilings={'c1': segments})
        self.assertEqual(self.node.drive(plan, 'c1'), chains.EXIT_ROOM)
        if stop:
            (self.node.root / 'chains' / 'c1' / 'STOP').write_text('carry stop\n')
        return plan, Path(plan['chains']['c1']['manifest'])

    def trainer_copy(self):
        """Another trainer build (different bytes and hash) that computes the same states."""
        path = self.t / 'trainer-b.py'
        path.write_text(FAKE.read_text() + '\n# another build\n')
        return path

    def carry(self, manifest, boundary, name='carry.json'):
        out = self.t / 'cont-in' / name
        with contextlib.redirect_stdout(io.StringIO()):
            code = chains.main(['carry', '--manifest', str(manifest), '--boundary', str(boundary),
                                '--objects', str(self.t / 'cont-in' / 'objects'), '--out', str(out)])
        self.assertEqual(code, 0)
        return out

    def cont_inputs(self, carry, label='cont', root='cont', c_wall=600.0, seed=SEED, **arm_extra):
        doc = json.loads(Path(self.node.inputs('0', {'c1c': {'arm': 'C', 'slot': 0, 'seed_root': seed,
                                                             'carry': str(carry)}},
                                               label=label, c_wall_seconds=c_wall, independent=True,
                                               trainer=str(self.trainer_copy()))).read_text())
        doc['root'] = (self.t / root).as_posix()
        doc['arms']['C'].update(arm_extra)
        return jwrite(self.t / f'inputs-{label}.json', doc)

    def drive(self, plan, name, root, segments):
        source = Path(root) / 'state' / name / 'ledger-source.json'
        if not source.exists():
            stat = Path(root) / 'state' / name / 'cpu.stat'
            jwrite(stat, {})
            stat.write_text('usage_usec 0\n')
            jwrite(source, {'schema': 'w5-chain-ledger-source/v1', 'closed_usec': 0, 'instance': 0,
                            'cpu_stat': stat.as_posix()})
        jwrite(Path(root) / 'GUARD.json', {'ok': True, 'unix': time.time(), 'used_bytes': 0, 'cap_bytes': 1,
                                           'ceilings': {name: segments}, 'headroom': 10**15})
        c = plan['chains'][name]
        return chains.main(['_drive', '--arm-file', plan['arms'][c['arm']]['file']['path'], '--chain', name,
                            '--seed-root', c['seed_root'], '--state', str(Path(root) / 'state' / name),
                            '--ledger-source', str(source)])

    def whole(self, plan, updates):
        """One uninterrupted run of the old recipe from the parent: the reference after-states 1..updates."""
        arm = chains.load_arm(plan['arms']['C']['file']['path'])
        cfg = chains.arm_config(arm, SEED, 0, updates, self.t / 'whole')
        self.assertEqual(chains.run_trainer(arm, jwrite(self.t / 'whole.json', cfg))[0], 0)
        return after_states(self.t / 'whole')

    # ------------------------------------------------------------ tests

    def test_a_carried_chain_reaches_the_old_chains_states(self):
        old_plan, old_man = self.old(3)  # boundaries 1..12
        ref = self.whole(old_plan, 20)
        _, old_records = retention.index(old_man)
        self.assertEqual([r['after_state_sha256'] for r in old_records], ref[:12])
        carry_path = self.carry(old_man, 8)
        carry = json.loads(carry_path.read_text())
        self.assertEqual((carry['boundary'], carry['seed_root'], carry['after_state_sha256']), (8, SEED, ref[7]))
        self.assertTrue(Path(carry['source']['checkpoint']['path']).is_file())
        self.assertNotEqual(carry['source']['checkpoint']['path'], carry['from']['checkpoint'])
        path, plan = chains.make_plan(self.cont_inputs(carry_path))
        man_path = Path(plan['chains']['c1c']['manifest'])
        man = json.loads(man_path.read_text())
        self.assertEqual(man['carry'], carry)
        self.assertEqual(plan['chains']['c1c']['carry'], chains.pin(carry_path))
        old_recipe = json.loads(old_man.read_text())['recipe']
        self.assertEqual({k for k in man['recipe'] if man['recipe'][k] != old_recipe[k]}, {'trainer_sha256'})
        # Planning again is idempotent; another carry for the same manifest is refused.
        chains.make_plan(self.cont_inputs(carry_path))
        other = self.carry(old_man, 4, name='carry-4.json')
        with self.assertRaisesRegex(chains.Stop, 'another carry'):
            chains.make_plan(self.cont_inputs(other))
        # Before its first update the chain stands at B.
        self.assertEqual(chains.latest(man), (8, None))
        cont_root = self.t / 'cont'
        self.assertEqual(self.drive(plan, 'c1c', cont_root, 3), chains.EXIT_ROOM)
        chain, records = retention.index(man_path)
        self.assertEqual([s['offset'] for s in chain['segments']], [8, 12, 16])
        self.assertEqual([(r['update'], r['boundary'], r['segment'], r['iteration']) for r in records][:5],
                         [(8, 9, 0, 0), (9, 10, 0, 1), (10, 11, 0, 2), (11, 12, 0, 3), (12, 13, 1, 0)])
        got = [r['after_state_sha256'] for r in records]
        self.assertEqual(got, ref[8:20])  # every state the old chain would have reached
        self.assertEqual(got[:4], [r['after_state_sha256'] for r in old_records[8:12]])  # the overlap it did reach
        # The first segment's config is the old chain's next one but for the paths that must change.
        old_doc = json.loads(old_man.read_text())
        old_cfg = retention.read_config(old_doc['segments'][2]['config'])
        new_cfg = retention.read_config(chain['segments'][0]['config'])
        for cfg in (old_cfg, new_cfg):
            cfg.pop('output_directory')
            cfg['initial_source']['checkpoint'].pop('path')
            cfg['chain_ledger'].pop('source')
        self.assertEqual(new_cfg, old_cfg)
        # The incremental cache sees the same records as a whole read, and a supervise cycle runs.
        cache = retention.IndexCache(man_path)
        _, cached = cache.refresh()
        self.assertEqual([r['boundary'] for r in cached], list(range(9, 21)))
        _, again = cache.refresh()
        self.assertEqual(len(again), 12)
        report = chains.cycle(plan, path)
        self.assertIn('c1c', report['retention'])
        self.assertEqual(retention.load_chain(man_path)['segments'][0]['offset'], 8)
        self.assertEqual(chains.committed_through(man_path.parent, 8), 20)  # commits counted from B
        self.assertEqual(chains.committed_through(man_path.parent), 0)  # from 0 they never start

    def test_end_and_wall_count_from_the_carried_chain(self):
        _, old_man = self.old(2)
        carry_path = self.carry(old_man, 8)
        path, plan = chains.make_plan(self.cont_inputs(carry_path))
        cont_root = self.t / 'cont'
        launched = json.loads(old_man.read_text())['launched_unix']
        # The wall is the old chain's: past launched + c_wall, the carried chain ends at B before any segment.
        with mock.patch.object(chains, 'time', wraps=time) as clock:
            clock.time.return_value = launched + 600.0 + 1.0
            self.assertEqual(self.drive(plan, 'c1c', cont_root, 2), 0)
        end = json.loads((cont_root / 'chains' / 'c1c' / 'END.json').read_text())
        self.assertEqual((end['reason'], end['last_valid_boundary']), ('c-wall', 8))
        self.assertEqual(json.loads(Path(plan['chains']['c1c']['manifest']).read_text())['segments'], [])

    def test_operator_end_names_a_boundary_from_b(self):
        _, old_man = self.old(2)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        self.assertEqual(self.drive(plan, 'c1c', self.t / 'cont', 1), chains.EXIT_ROOM)
        with contextlib.redirect_stderr(io.StringIO()), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(chains.main(['end', '--plan', str(path), '--chain', 'c1c', '--reason', 'x',
                                          '--boundary', '7']), w5box.EXIT_REFUSED)
            self.assertEqual(chains.main(['end', '--plan', str(path), '--chain', 'c1c', '--reason', 'x',
                                          '--boundary', '8']), 0)
        end = json.loads((self.t / 'cont' / 'chains' / 'c1c' / 'END.json').read_text())
        self.assertEqual((end['last_valid_boundary'], end['ledger_usec']), (8, None))

    def test_plan_refuses_a_carry_that_would_change_the_states(self):
        _, old_man = self.old(2)
        carry_path = self.carry(old_man, 8)
        cases = [({'seed': 'oc/other'}, 'seed root'),
                 ({'workers': 2}, 'changes the carried recipe'),
                 ({'segment_updates': 8}, 'changes the carried recipe'),
                 ({'c_wall': 900.0}, 'changes the carried recipe')]
        for i, (change, why) in enumerate(cases):
            kw = {k: change.pop(k) for k in ('seed', 'c_wall') if k in change}
            with self.subTest(change=change or kw), self.assertRaisesRegex(chains.Stop, why):
                chains.make_plan(self.cont_inputs(carry_path, label=f'bad{i}', root=f'bad{i}', **kw, **change))
        doc = json.loads(Path(self.cont_inputs(carry_path, label='dep', root='dep')).read_text())
        doc.pop('independent')
        with self.assertRaisesRegex(chains.Stop, 'independent Stage 0'):
            chains.make_plan(jwrite(self.t / 'dep.json', doc))
        # A damaged checkpoint copy is refused.
        copy_path = Path(json.loads(carry_path.read_text())['source']['checkpoint']['path'])
        good = copy_path.read_bytes()
        copy_path.write_bytes(good + b' ')
        with self.assertRaises(chains.Stop):
            chains.make_plan(self.cont_inputs(carry_path, label='dmg', root='dmg'))
        copy_path.write_bytes(good)
        # A carried wall that has passed is refused.
        with mock.patch.object(chains, 'time', wraps=time) as clock:
            clock.time.return_value = json.loads(old_man.read_text())['launched_unix'] + 601.0
            with self.assertRaisesRegex(chains.Stop, 'wall has passed'):
                chains.make_plan(self.cont_inputs(carry_path, label='late', root='late'))

    def test_carry_reads_only_a_persisted_segment_end(self):
        _, old_man = self.old(2)
        for boundary in (6, 0, 12, 4 * 9):
            with self.subTest(boundary=boundary), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(chains.main(['carry', '--manifest', str(old_man), '--boundary', str(boundary),
                                              '--objects', str(self.t / 'o'), '--out', str(self.t / 'c.json')]),
                                 w5box.EXIT_REFUSED)
        before = sorted(p.relative_to(self.node.root).as_posix() for p in self.node.root.rglob('*'))
        self.carry(old_man, 4)
        after = sorted(p.relative_to(self.node.root).as_posix() for p in self.node.root.rglob('*'))
        self.assertEqual(before, after)  # nothing written in the old chain's root

    def test_a_manifest_without_carry_is_unchanged(self):
        _, old_man = self.old(1)
        man = json.loads(old_man.read_text())
        self.assertNotIn('carry', man)
        self.assertEqual(chains.carry_start(man), 0)
        self.assertIsNone(chains.carry_source(man))
        self.assertEqual(chains.wall_origin(man), man['launched_unix'])
        self.assertEqual(chains.committed_through(old_man.parent), 4)  # its durable commits from 0
        # The legacy replay diagnostic of an ordinary chain is unchanged.
        chain = retention.load_chain(old_man)
        spec = {'endpoint_boundary': 4, 'replay': {'from': {'boundary': 2, 'checkpoint': None}}}
        with self.assertRaisesRegex(SystemExit, 'a replay from a nonzero boundary needs its kept checkpoint'):
            retention.replay_config(chain, spec, self.t / 'r')


    def test_a_carried_chain_runs_only_while_its_old_chain_is_held(self):
        _, old_man = self.old(2, stop=False)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(self.drive(plan, 'c1c', self.t / 'cont', 1), w5box.EXIT_REFUSED)
        self.assertEqual(json.loads(Path(plan['chains']['c1c']['manifest']).read_text())['segments'], [])
        (self.node.root / 'chains' / 'c1' / 'STOP').write_text('carry stop\n')
        self.assertEqual(self.drive(plan, 'c1c', self.t / 'cont', 1), chains.EXIT_ROOM)
        # An open w5box instance of the old chain (its trainer still running) also holds the continuation back.
        self.node.ledger('c1', 'open')
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(self.drive(plan, 'c1c', self.t / 'cont', 2), w5box.EXIT_REFUSED)
        self.node.ledger('c1', 'close')
        # The old chain's STOP removed while the continuation runs: it stops at its next segment boundary.
        run_trainer = chains.run_trainer

        def run_then_unstop(*args, **kwargs):
            out = run_trainer(*args, **kwargs)
            (self.node.root / 'chains' / 'c1' / 'STOP').unlink(missing_ok=True)
            return out
        with mock.patch.object(chains, 'run_trainer', side_effect=run_then_unstop):
            self.assertEqual(self.drive(plan, 'c1c', self.t / 'cont', 4), chains.EXIT_STOP)
        self.assertEqual(len(json.loads(Path(plan['chains']['c1c']['manifest']).read_text())['segments']), 2)

    def test_carry_writes_nothing_inside_the_old_root(self):
        _, old_man = self.old(2)
        seg = Path(json.loads(old_man.read_text())['segments'][1]['root'])
        before = {p: p.read_bytes() for p in self.node.root.rglob('*') if p.is_file()}
        for objects, out in ((self.t / 'o', old_man), (self.node.root / 'chains' / 'c1', self.t / 'c.json'),
                             (self.t / 'o', seg / 'carry.json'), (self.node.root, self.t / 'c.json')):
            with self.subTest(objects=objects, out=out), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(chains.main(['carry', '--manifest', str(old_man), '--boundary', '8',
                                              '--objects', str(objects), '--out', str(out)]), w5box.EXIT_REFUSED)
        after = {p: p.read_bytes() for p in self.node.root.rglob('*') if p.is_file()}
        self.assertEqual(before, after)  # names and contents

    def test_long_carried_chain_keeps_every_state_the_old_rule_keeps(self):
        """Realistic shape: 64-update segments, carry at S = 4,160 after the old chain stopped at E = 4,224; the
        continuation is interrupted in its first segment, restarted, runs past 8,192 with supervise cycles. The
        old tree is unchanged, the copy of S intact, every multiple of 4,096 and the recent window kept, unkept
        states pruned as for any chain, the states equal one uninterrupted run, the incremental index a whole read,
        and a replay inside the first carried segment starts at S from the carried source."""
        old_plan, old_man = self.old(66, length=64, c_wall=86400.0)  # E = 4,224 (a long wall: a slow host)
        start = 4160  # not a multiple of 4,096: a stride counted from the start would keep 8,256, not 8,192
        total = start + 64 * 66
        ref = self.whole(old_plan, total)
        carry_path = self.carry(old_man, start)
        old_plan_path = self.node.root / 'plan' / 'old' / 'CHAINS-PLAN.json'
        path, plan = chains.make_plan(self.cont_inputs(carry_path, segment_updates=64, c_wall=86400.0))
        cont_root, man_path = self.t / 'cont', Path(plan['chains']['c1c']['manifest'])
        cache = retention.IndexCache(man_path)
        with mock.patch.dict(os.environ, {'FAKE_FAIL_AT': '4190'}):  # a crash inside the first carried segment
            self.assertEqual(self.drive(plan, 'c1c', cont_root, 1), chains.EXIT_FAILED)
        self.assertEqual(self.drive(plan, 'c1c', cont_root, 3), chains.EXIT_ROOM)  # reruns it, then one more
        _, rows = cache.refresh()  # the failed run used one admission: two segments now
        self.assertEqual([r['after_state_sha256'] for r in rows], ref[start:start + 128])
        _, old_rows = retention.index(old_man)
        self.assertEqual([r['after_state_sha256'] for r in rows[:64]],
                         [r['after_state_sha256'] for r in old_rows[start:start + 64]])
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(chains.main(['end', '--plan', str(old_plan_path), '--chain', 'c1', '--boundary',
                                          '4224', '--reason', 'continued']), 0)
        old_chain = self.node.root / 'chains' / 'c1'
        old_files = {p: chains.sha(p) for p in old_chain.rglob('*') if p.is_file()}
        copy_path = Path(json.loads(carry_path.read_text())['source']['checkpoint']['path'])
        copy_sha = chains.sha(copy_path)
        admitted = 3
        # Strides scaled down (4,096 to 256, so the run leaves the recent window many times): every rule boundary
        # is a global multiple of 256, while a stride counted from S = 4,160 would keep 4,416, 4,672, ...
        with mock.patch.object(chains, 'occupancy', side_effect=AssertionError('no fills')), \
                mock.patch.object(retention, 'C_LONG_STRIDE', 256), mock.patch.object(retention, 'C_RECENT', 256):
            for chunk in range(4):  # 64 more segments in 4 chunks, supervise cycles of both plans after each
                admitted += 16
                self.assertEqual(self.drive(plan, 'c1c', cont_root, admitted), chains.EXIT_ROOM)
                _, rows = cache.refresh()
                chains.cycle(plan, path, caches={man_path.as_posix(): cache})
                chains.cycle(old_plan, old_plan_path)  # the old plan's supervisor, its chain ended
        _, whole = retention.index(man_path)
        self.assertEqual([r['boundary'] for r in rows], [r['boundary'] for r in whole])
        self.assertEqual([r['after_state_sha256'] for r in whole], ref[start:total])
        latest = whole[-1]['boundary']
        self.assertEqual(latest, total)
        kept = {r['boundary'] for r in whole if Path(r['checkpoint']['path']).exists()}
        with mock.patch.object(retention, 'C_LONG_STRIDE', 256), mock.patch.object(retention, 'C_RECENT', 256):
            rule = {r['boundary'] for r in whole if retention.keeps_full_state('C', r['boundary'], latest)}
        self.assertTrue(rule <= kept, sorted(rule - kept)[:5])   # nothing the rule keeps is missing
        self.assertTrue({4352, 4608, 6144, 8192} <= kept)        # global multiples of 256, far from the latest
        self.assertFalse(kept & {4416, 4672, 6208})              # not a stride counted from S
        self.assertFalse(kept & {4200, 4288, 5000})              # unkept states pruned as for any chain
        self.assertTrue(all(b > latest - 128 for b in kept - rule))  # beyond the rule: only the last two segments
        self.assertEqual({p: chains.sha(p) for p in old_chain.rglob('*') if p.is_file()}, old_files)
        self.assertEqual(chains.sha(copy_path), copy_sha)
        # replay selection inside the first carried segment starts at S with the carried source
        b = start + 2
        for r in whole:
            if start < r['boundary'] <= b:
                Path(r['checkpoint']['path']).unlink(missing_ok=True)
        spec = retention.select(retention.load_chain(man_path), whole,
                                next(r['ledger_usec'] for r in whole if r['boundary'] == b))
        self.assertEqual(spec['replay']['from'], {'boundary': start, 'checkpoint': None})
        cfg = retention.replay_config(retention.load_chain(man_path), spec, self.t / 'replay')
        self.assertEqual(cfg['initial_source']['checkpoint']['sha256'], copy_sha)
        arm = chains.load_arm(plan['arms']['C']['file']['path'])
        self.assertEqual(chains.run_trainer(arm, jwrite(self.t / 'replay.json', cfg))[0], 0)
        self.assertTrue(retention.verify_replay(spec, self.t / 'replay')['equal'])


    def test_more_plan_and_carry_refusals(self):
        _, old_man = self.old(3)
        carry_path = self.carry(old_man, 8)
        path, plan = chains.make_plan(self.cont_inputs(carry_path))
        # a re-plan of the started manifest after the wall (for example after a tools fix) is not refused
        with mock.patch.object(chains, 'time', wraps=time) as clock:
            clock.time.return_value = json.loads(old_man.read_text())['launched_unix'] + 601.0
            chains.make_plan(self.cont_inputs(carry_path))
        # the old name or the old root, and look state, are refused
        doc = json.loads(Path(self.cont_inputs(carry_path, label='nm', root='nm')).read_text())
        doc['chains'] = {'c1': dict(doc['chains']['c1c'])}
        with self.assertRaisesRegex(chains.Stop, 'new name'):
            chains.make_plan(jwrite(self.t / 'nm.json', doc))
        doc = json.loads(Path(self.cont_inputs(carry_path, label='rt', root='rt')).read_text())
        doc['root'] = self.node.root.as_posix()
        with self.assertRaisesRegex(chains.Stop, 'new root'):
            chains.make_plan(jwrite(self.t / 'rt.json', doc))
        doc = json.loads(Path(self.cont_inputs(carry_path, label='rn', root='rn')).read_text())
        doc['r_nom_usec'] = 10**9
        with self.assertRaisesRegex(chains.Stop, 'look state'):
            chains.make_plan(jwrite(self.t / 'rn.json', doc))
        # a relative --objects becomes an absolute path in the record
        cwd = os.getcwd()
        try:
            os.chdir(self.t)
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(chains.main(['carry', '--manifest', os.path.relpath(old_man, self.t), '--boundary',
                                              '4', '--objects', 'relobj', '--out', 'carry-rel.json']), 0)
        finally:
            os.chdir(cwd)
        rel = json.loads((self.t / 'carry-rel.json').read_text())
        self.assertTrue(Path(rel['source']['checkpoint']['path']).is_absolute())
        self.assertTrue(Path(rel['from']['manifest']['path']).is_absolute())
        # a new root inside the old root is refused before the plan writes anything there
        doc = json.loads(Path(self.cont_inputs(carry_path, label='in', root='in')).read_text())
        doc['root'] = (self.node.root / 'sub').as_posix()
        before = sorted(p.as_posix() for p in self.node.root.rglob('*'))
        with self.assertRaisesRegex(chains.Stop, 'new root'):
            chains.make_plan(jwrite(self.t / 'in.json', doc))
        self.assertEqual(sorted(p.as_posix() for p in self.node.root.rglob('*')), before)
        # a boundary past the old chain's END is refused
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(chains.main(['end', '--plan', str(self.node.root / 'plan' / 'old' / 'CHAINS-PLAN.json'),
                                          '--chain', 'c1', '--boundary', '8', '--reason', 'x']), 0)
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(chains.main(['carry', '--manifest', str(old_man), '--boundary', '12', '--objects',
                                          str(self.t / 'o'), '--out', str(self.t / 'c12.json')]), w5box.EXIT_REFUSED)
        # an unstarted carried chain reports its start as its latest boundary
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            retention.main(['index', str(Path(plan['chains']['c1c']['manifest']))])
        self.assertIn('"latest_boundary": 8', out.getvalue())


    def test_an_end_below_the_carried_boundary_holds_the_continuation(self):
        _, old_man = self.old(3)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(chains.main(['end', '--plan', str(self.node.root / 'plan' / 'old' / 'CHAINS-PLAN.json'),
                                          '--chain', 'c1', '--boundary', '4', '--reason', 'x']), 0)
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(self.drive(plan, 'c1c', self.t / 'cont', 1), w5box.EXIT_REFUSED)
            self.assertEqual(chains.main(['run', '--plan', str(path), '--chain', 'c1c']), w5box.EXIT_REFUSED)

    def test_the_copy_never_writes_through_an_old_name(self):
        _, old_man = self.old(2)
        seg1 = Path(json.loads(old_man.read_text())['segments'][1]['root'])
        old_ck = next(seg1.glob('iterations/000003/attempt-*/update/checkpoint.json'))  # S itself: the matching hash
        before = old_ck.read_bytes()
        objects = self.t / 'cont-in' / 'objects'
        objects.mkdir(parents=True, exist_ok=True)
        digest = json.loads(next(seg1.glob('iterations/000003/complete.json')).read_text())['output_identity'][
            'checkpoint_sha256']
        self.assertEqual(chains.sha(old_ck), digest)
        trap = objects / f'{digest}.json.tmp'
        final = objects / f'{digest}.json'
        try:
            trap.symlink_to(old_ck)
            final.symlink_to(old_ck)  # even the copy's own name, pointing at an old file, is not reused or written
        except OSError:
            self.skipTest('no symlink permission on this host')
        self.carry(old_man, 8)
        self.assertEqual(old_ck.read_bytes(), before)
        self.assertFalse(final.is_symlink())


    def test_cleanup_trees_and_scratch_roots_stay_apart(self):
        """A valid scratch-compatible carry (32-update segments) where only the location under test changes."""
        _, old_man = self.old(2, length=32)
        carry_path = self.carry(old_man, 32)
        rec = json.loads(carry_path.read_text())

        def inputs(label, root, carry, scratch):
            doc = json.loads(Path(self.cont_inputs(carry, label=label, root=root, segment_updates=32)).read_text())
            doc['scratch_root'] = Path(scratch).as_posix()
            doc['arms']['C']['scratch'] = True
            return jwrite(self.t / f'{label}.json', doc)
        chains.make_plan(inputs('okplan', 'okroot', carry_path, self.t / 'shm'))  # the valid baseline plans
        for label, where in (('wseg', self.t / 'cont2' / 'chains' / 'c1c' / 'seg-00000'), ('wshm', self.t / 'shm2')):
            moved = json.loads(json.dumps(rec))
            target = where / Path(rec['source']['checkpoint']['path']).name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(Path(rec['source']['checkpoint']['path']).read_bytes())
            moved['source']['checkpoint']['path'] = target.as_posix()
            bad = jwrite(self.t / f'carry-{label}.json', moved)
            with self.subTest(where=label), self.assertRaisesRegex(chains.Stop, 'outside the old root, the new root'):
                chains.make_plan(inputs(label, 'cont2', bad, self.t / 'shm2'))
        # a scratch root inside the old root is refused before anything is written there
        before = sorted(p.as_posix() for p in self.node.root.rglob('*'))
        with self.assertRaisesRegex(chains.Stop, 'not apart'):
            chains.make_plan(inputs('scr', 'cont3', carry_path, self.node.root / 'chains'))
        self.assertEqual(sorted(p.as_posix() for p in self.node.root.rglob('*')), before)

    def test_run_refuses_while_the_old_chain_is_not_held(self):
        _, old_man = self.old(3)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        jwrite(self.t / 'cont' / 'plan' / 'cont' / 'LAUNCHED.json', {'opened': [0]})
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(chains.main(['end', '--plan', str(self.node.root / 'plan' / 'old' / 'CHAINS-PLAN.json'),
                                          '--chain', 'c1', '--boundary', '4', '--reason', 'x']), 0)
        err = io.StringIO()
        with mock.patch.object(chains.subprocess, 'call', side_effect=AssertionError('no w5box')), \
                contextlib.redirect_stderr(err):
            self.assertEqual(chains.main(['run', '--plan', str(path), '--chain', 'c1c']), w5box.EXIT_REFUSED)
        self.assertIn('before the carried boundary', err.getvalue())


    def test_the_old_scratch_root_is_protected_and_must_be_known(self):
        _, old_man = self.old(3)
        carry_path = self.carry(old_man, 8)
        # give the old chain a scratch root, as the main chains have (its arm file re-pinned in its manifest)
        man = json.loads(old_man.read_text())
        arm = json.loads(Path(man['arm_file']['path']).read_text())
        arm['scratch_root'] = (self.t / 'oldshm').as_posix()
        new_arm = jwrite(self.t / 'old-arm-scratch.json', arm)
        man['arm_file'] = chains.pin(new_arm)
        old_man.write_text(json.dumps(man))
        (self.t / 'oldshm').mkdir()
        keep = self.t / 'oldshm' / 'keep.json'
        keep.write_text('old scratch bytes')
        for objects, out in ((self.t / 'oldshm' / 'objects', self.t / 'c.json'), (self.t / 'o', keep)):
            with self.subTest(objects=objects, out=out), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(chains.main(['carry', '--manifest', str(old_man), '--boundary', '8',
                                              '--objects', str(objects), '--out', str(out)]), w5box.EXIT_REFUSED)
        self.assertEqual(keep.read_text(), 'old scratch bytes')
        self.assertFalse((self.t / 'oldshm' / 'objects').exists())
        # a carried checkpoint under the old scratch root is refused at plan time
        rec = json.loads(carry_path.read_text())
        target = self.t / 'oldshm' / Path(rec['source']['checkpoint']['path']).name
        target.write_bytes(Path(rec['source']['checkpoint']['path']).read_bytes())
        rec['source']['checkpoint']['path'] = target.as_posix()
        with self.assertRaisesRegex(chains.Stop, 'scratch roots'):
            chains.make_plan(self.cont_inputs(jwrite(self.t / 'carry-oldshm.json', rec), label='os', root='os'))
        # a new scratch root inside the old scratch root is refused
        doc = json.loads(Path(self.cont_inputs(carry_path, label='ns', root='ns')).read_text())
        doc['scratch_root'] = (self.t / 'oldshm' / 'new').as_posix()
        with self.assertRaisesRegex(chains.Stop, 'not apart'):
            chains.make_plan(jwrite(self.t / 'ns.json', doc))
        # an old arm file that no longer matches its pin: the old trees cannot be established, so planning refuses
        new_arm.write_text(new_arm.read_text() + ' ')
        with self.assertRaisesRegex(chains.Stop, 'cannot be established|differs|pin'):
            chains.make_plan(self.cont_inputs(carry_path, label='pn', root='pn'))


    def test_carry_reads_the_chain_in_place_only(self):
        _, old_man = self.old(3)
        # a copied manifest (its segments still in the live tree) is refused
        copy = self.t / 'backup' / 'chains' / 'c1' / 'chain.json'
        copy.parent.mkdir(parents=True)
        copy.write_bytes(old_man.read_bytes())
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(chains.main(['carry', '--manifest', str(copy), '--boundary', '8', '--objects',
                                          str(self.t / 'o'), '--out', str(self.t / 'c.json')]), w5box.EXIT_REFUSED)
        # a carried chain is not carried again
        carry_path = self.carry(old_man, 8)
        path, plan = chains.make_plan(self.cont_inputs(carry_path))
        self.assertEqual(self.drive(plan, 'c1c', self.t / 'cont', 2), chains.EXIT_ROOM)
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(chains.main(['carry', '--manifest', plan['chains']['c1c']['manifest'], '--boundary',
                                          '16', '--objects', str(self.t / 'o2'), '--out', str(self.t / 'c2.json')]),
                             w5box.EXIT_REFUSED)
        # an old root inside the new root is refused before any write
        doc = json.loads(Path(self.cont_inputs(carry_path, label='up', root='up')).read_text())
        doc['root'] = self.node.root.parent.as_posix()
        before = sorted(p.as_posix() for p in self.node.root.parent.glob('plan/**/*'))
        with self.assertRaisesRegex(chains.Stop, 'not apart'):
            chains.make_plan(jwrite(self.t / 'up.json', doc))
        self.assertEqual(sorted(p.as_posix() for p in self.node.root.parent.glob('plan/**/*')), before)
        # a record whose manifest path is not canonical is refused
        rec = json.loads(carry_path.read_text())
        rec['from']['manifest']['path'] = (old_man.parent / 'sub' / '..' / 'chain.json').as_posix()
        with self.assertRaises(chains.Stop):
            chains.make_plan(self.cont_inputs(jwrite(self.t / 'carry-dots.json', rec), label='dt', root='dt'))


    def test_a_paired_chain_is_not_carried(self):
        _, old_man = self.old(2)
        man = json.loads(old_man.read_text())
        carry_path = self.carry(old_man, 8)  # the unpaired control carries
        man['pair'] = 1
        old_man.write_text(json.dumps(man))
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(chains.main(['carry', '--manifest', str(old_man), '--boundary', '8', '--objects',
                                          str(self.t / 'o'), '--out', str(self.t / 'cp.json')]), w5box.EXIT_REFUSED)
        with self.assertRaises(chains.Stop):
            chains.make_plan(self.cont_inputs(carry_path, label='pr', root='pr'))


    def test_a_foreign_record_or_a_changed_receipt_is_refused(self):
        _, old_man = self.old(3)
        other = jwrite(self.t / 'not-a-carry.json', {'schema': 'something-else'})
        before = sorted(p.as_posix() for p in self.t.rglob('*'))
        with self.assertRaisesRegex(chains.Stop, 'not a w5-chain-carry/v1 record'):
            chains.make_plan(self.cont_inputs(other, label='fr', root='fr'))
        self.assertFalse((self.t / 'fr').exists())  # refused before any plan write
        broken = jwrite(self.t / 'broken-carry.json', {'schema': chains.CARRY_SCHEMA, 'from': None})
        with self.assertRaises(chains.Stop):
            chains.make_plan(self.cont_inputs(broken, label='br', root='br'))
        rec = json.loads(self.carry(old_man, 8).read_text())
        rec['from']['chain'] = 'other'
        with self.assertRaisesRegex(chains.Stop, 'names chain other'):
            chains.make_plan(self.cont_inputs(jwrite(self.t / 'carry-other.json', rec), label='ot', root='ot'))
        seg = Path(json.loads(old_man.read_text())['segments'][1]['root'])
        upd = next(seg.glob('iterations/000003/attempt-*/update/update.json'))
        upd.write_text(upd.read_text() + ' ')
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(chains.main(['carry', '--manifest', str(old_man), '--boundary', '8', '--objects',
                                          str(self.t / 'o'), '--out', str(self.t / 'cu.json')]), w5box.EXIT_REFUSED)


    def test_a_link_below_the_new_roots_cannot_reach_the_old_chain(self):
        _, old_man = self.old(2, length=32)
        carry_path = self.carry(old_man, 32)
        shm = self.t / 'shm'
        shm.mkdir()
        try:
            (shm / 'c1c').symlink_to(self.node.root / 'chains' / 'c1', target_is_directory=True)
        except OSError:
            self.skipTest('no symlink permission on this host')
        doc = json.loads(Path(self.cont_inputs(carry_path, label='lk', root='lk', segment_updates=32)).read_text())
        doc['scratch_root'] = shm.as_posix()
        doc['arms']['C']['scratch'] = True
        with self.assertRaisesRegex(chains.Stop, 'resolves to'):
            chains.make_plan(jwrite(self.t / 'lk.json', doc))
        self.assertFalse((self.t / 'lk').exists())  # refused before any plan write
        # the same link placed after planning: the driver refuses at start, before any scratch work
        (shm / 'c1c').unlink()
        path, plan = chains.make_plan(jwrite(self.t / 'lk.json', doc))
        (shm / 'c1c').symlink_to(self.node.root / 'chains' / 'c1', target_is_directory=True)
        before = {p: p.read_bytes() for p in (self.node.root / 'chains' / 'c1').rglob('*') if p.is_file()}
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(self.drive(plan, 'c1c', self.t / 'lk', 1), w5box.EXIT_REFUSED)
        self.assertEqual({p: p.read_bytes() for p in (self.node.root / 'chains' / 'c1').rglob('*') if p.is_file()},
                         before)


    def test_a_segment_link_or_a_release_cannot_reach_the_old_chain(self):
        _, old_man = self.old(2, length=32)
        carry_path = self.carry(old_man, 32)
        shm = self.t / 'shm'
        doc = json.loads(Path(self.cont_inputs(carry_path, label='sg', root='sg', segment_updates=32)).read_text())
        doc['scratch_root'] = shm.as_posix()
        doc['arms']['C']['scratch'] = True
        path, plan = chains.make_plan(jwrite(self.t / 'sg.json', doc))
        old_seg = Path(json.loads(old_man.read_text())['segments'][0]['root'])
        (shm / 'c1c').mkdir(parents=True)
        try:
            (shm / 'c1c' / 'seg-00000').symlink_to(old_seg, target_is_directory=True)
        except OSError:
            self.skipTest('no symlink permission on this host')
        before = {p: p.read_bytes() for p in (self.node.root / 'chains' / 'c1').rglob('*') if p.is_file()}
        with mock.patch.object(chains, 'scratch_link', side_effect=AssertionError('no mount')), \
                contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(self.drive(plan, 'c1c', self.t / 'sg', 1), w5box.EXIT_REFUSED)
        # the supervisor's release of an in-flight segment refuses too, before the unmount and the delete
        man = json.loads(Path(plan['chains']['c1c']['manifest']).read_text())
        self.assertTrue(man['segments'])
        with mock.patch.object(chains, 'mounted', return_value=True), \
                mock.patch.object(chains, 'unmount', side_effect=AssertionError('no unmount')):
            with self.assertRaises(chains.Stop):
                chains.release_scratch(plan, 'c1c', man)
        self.assertEqual({p: p.read_bytes() for p in (self.node.root / 'chains' / 'c1').rglob('*') if p.is_file()},
                         before)


    def test_links_into_the_old_chain_are_refused_by_index_retention_and_persist(self):
        _, old_man = self.old(3)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        cont = self.t / 'cont'
        man_path = Path(plan['chains']['c1c']['manifest'])
        self.assertEqual(self.drive(plan, 'c1c', cont, 3), chains.EXIT_ROOM)
        cache = retention.IndexCache(man_path)
        cache.refresh()
        old_chain = self.node.root / 'chains' / 'c1'
        before = {p: p.read_bytes() for p in old_chain.rglob('*') if p.is_file()}
        seg0 = man_path.parent / 'seg-00000'
        moved = seg0.with_name('seg-00000.real')
        seg0.rename(moved)
        try:
            seg0.symlink_to(old_chain / 'seg-00001', target_is_directory=True)
        except OSError:
            moved.rename(seg0)
            self.skipTest('no symlink permission on this host')
        # a fresh index refuses the linked segment
        retention.index_path(man_path).rename(retention.index_path(man_path).with_suffix('.bak'))
        with self.assertRaises(SystemExit):
            retention.index(man_path)
        retention.index_path(man_path).with_suffix('.bak').rename(retention.index_path(man_path))
        # a supervise cycle with the existing cache refuses to delete through the link
        with mock.patch.object(chains, 'occupancy', side_effect=AssertionError('no fills')):
            with self.assertRaises((chains.Stop, SystemExit)):
                chains.cycle(plan, path, caches={man_path.as_posix(): cache})
        self.assertEqual({p: p.read_bytes() for p in old_chain.rglob('*') if p.is_file()}, before)
        seg0.unlink()
        moved.rename(seg0)
        # a persist staging link refuses the driver before any copy
        (man_path.parent / '.seg-00003.persist').symlink_to(old_chain / 'seg-00001', target_is_directory=True)
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(self.drive(plan, 'c1c', cont, 4), w5box.EXIT_REFUSED)
        self.assertEqual({p: p.read_bytes() for p in old_chain.rglob('*') if p.is_file()}, before)


    def test_internal_aliases_standalone_retention_and_nested_links_are_refused(self):
        _, old_man = self.old(3)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        cont = self.t / 'cont'
        man_path = Path(plan['chains']['c1c']['manifest'])
        self.assertEqual(self.drive(plan, 'c1c', cont, 3), chains.EXIT_ROOM)
        cache = retention.IndexCache(man_path)
        cache.refresh()
        own = man_path.parent
        snapshot = {p: p.read_bytes() for p in list(own.rglob('*')) + list((self.node.root / 'chains' / 'c1').rglob('*'))
                    if p.is_file()}
        seg0, real0 = own / 'seg-00000', own / 'seg-00000.real'
        seg0.rename(real0)
        try:
            seg0.symlink_to(own / 'seg-00002', target_is_directory=True)  # an alias inside the continuation
        except OSError:
            real0.rename(seg0)
            self.skipTest('no symlink permission on this host')
        with mock.patch.object(chains, 'occupancy', side_effect=AssertionError('no fills')), \
                mock.patch.object(retention, 'C_RECENT', 0), mock.patch.object(retention, 'C_LONG_STRIDE', 10**9):
            with self.assertRaises((chains.Stop, SystemExit)):
                chains.cycle(plan, path, caches={man_path.as_posix(): cache})
            with self.assertRaises(SystemExit):  # the standalone plan refuses too
                retention.plan(man_path, None, retention.IndexCache(man_path))
        with self.assertRaises(SystemExit):  # and a plan document naming a path through the link is refused by apply
            retention.apply({'own_dir': own.resolve().as_posix(), 'checkpoints': [{'path': (seg0 / 'x').as_posix()}],
                             'trajectories': []}, self.t / 'apply.log')
        seg0.unlink()
        real0.rename(seg0)
        self.assertEqual({p: p.read_bytes() for p in snapshot}, snapshot)
        # a nested link inside the next segment refuses the driver before quarantine
        nxt = own / 'seg-00003' / 'iterations'
        nxt.mkdir(parents=True)
        old_attempt = next((self.node.root / 'chains' / 'c1' / 'seg-00001' / 'iterations' / '000003').glob('attempt-*'))
        (nxt / '000000').symlink_to(old_attempt, target_is_directory=True)
        with mock.patch.object(chains, 'quarantine', side_effect=AssertionError('no quarantine')), \
                contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(self.drive(plan, 'c1c', cont, 4), w5box.EXIT_REFUSED)
        kept = {p: b for p, b in snapshot.items() if p.name in ("checkpoint.json", "update.json", "complete.json")}
        self.assertEqual({p: p.read_bytes() for p in kept}, kept)  # no state of either chain deleted or changed


    def test_a_linked_chain_directory_is_refused_everywhere(self):
        _, old_man = self.old(3)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        cont = self.t / 'cont'
        self.assertEqual(self.drive(plan, 'c1c', cont, 1), chains.EXIT_ROOM)
        cdir = cont / 'chains' / 'c1c'
        old_chain = self.node.root / 'chains' / 'c1'
        before = {p: p.read_bytes() for p in old_chain.rglob('*') if p.is_file()}
        cdir.rename(cdir.with_name('c1c.real'))
        try:
            cdir.symlink_to(old_chain, target_is_directory=True)
        except OSError:
            cdir.with_name('c1c.real').rename(cdir)
            self.skipTest('no symlink permission on this host')
        jwrite(cont / 'plan' / 'cont' / 'LAUNCHED.json', {'opened': [0]})
        with mock.patch.object(chains, 'occupancy', side_effect=AssertionError('no fills')):
            with self.assertRaises(chains.Stop):
                chains.cycle(plan, path)
        with contextlib.redirect_stderr(io.StringIO()), contextlib.redirect_stdout(io.StringIO()), \
                mock.patch.object(chains.subprocess, 'call', side_effect=AssertionError('no w5box')):
            self.assertEqual(chains.main(['end', '--plan', str(path), '--chain', 'c1c', '--reason', 'x']),
                             w5box.EXIT_REFUSED)
            self.assertEqual(chains.main(['run', '--plan', str(path), '--chain', 'c1c']), w5box.EXIT_REFUSED)
        self.assertFalse((old_chain / 'END.json').exists())
        self.assertEqual({p: p.read_bytes() for p in old_chain.rglob('*') if p.is_file()}, before)

    def test_recipes_and_seed_roots_the_plan_refuses_are_refused_by_carry(self):
        _, old_man = self.old(2)
        good = old_man.read_text()
        for change in ({'recipe': 'initial_opponent'}, {'seed_root': 'w5/chain1'}, {'seed_root': 'w5/0b'}):
            man = json.loads(good)
            if 'recipe' in change:
                man['recipe'][change['recipe']] = {'kind': 'x'}
            else:
                man['seed_root'] = change['seed_root']
            old_man.write_text(json.dumps(man))
            with self.subTest(change=change), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(chains.main(['carry', '--manifest', str(old_man), '--boundary', '8', '--objects',
                                              str(self.t / 'o'), '--out', str(self.t / 'c.json')]), w5box.EXIT_REFUSED)
        old_man.write_text(good)


    def test_log_links_and_ordinary_manifests_are_refused(self):
        _, old_man = self.old(3)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        cont = self.t / 'cont'
        self.assertEqual(self.drive(plan, 'c1c', cont, 1), chains.EXIT_ROOM)
        old_chain = self.node.root / 'chains' / 'c1'
        old_ck = next((old_chain / 'seg-00001' / 'iterations' / '000003').glob('attempt-*/update/checkpoint.json'))
        before = {p: p.read_bytes() for p in old_chain.rglob('*') if p.is_file()}
        log = cont / 'chains' / 'c1c' / 'trainer.out'
        log.unlink(missing_ok=True)
        try:
            log.symlink_to(old_ck)
        except OSError:
            self.skipTest('no symlink permission on this host')
        with mock.patch.object(chains, 'run_trainer', side_effect=AssertionError('no run')), \
                contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(self.drive(plan, 'c1c', cont, 2), w5box.EXIT_REFUSED)
        log.unlink()
        rlog = cont / 'retention' / 'c1c.jsonl'
        rlog.parent.mkdir(exist_ok=True)
        rlog.unlink(missing_ok=True)
        rlog.symlink_to(old_ck)
        with mock.patch.object(chains, 'occupancy', side_effect=AssertionError('no fills')):
            with self.assertRaises(chains.Stop):
                chains.cycle(plan, path)
        with self.assertRaises(SystemExit):
            retention.apply({'own_dir': (cont / 'chains' / 'c1c').resolve().as_posix(), 'checkpoints': [],
                             'trajectories': []}, rlog)
        rlog.unlink()
        self.assertEqual({p: p.read_bytes() for p in old_chain.rglob('*') if p.is_file()}, before)
        # an ordinary manifest under a carried plan entry: run refuses before reading on or dispatching
        man_path = Path(plan['chains']['c1c']['manifest'])
        man = json.loads(man_path.read_text())
        man.pop('carry')
        man_path.write_text(json.dumps(man))
        (old_chain / 'STOP').unlink()
        (self.node.root / 'STOP').write_text('root stop only\n')
        jwrite(cont / 'plan' / 'cont' / 'LAUNCHED.json', {'opened': [0]})
        err = io.StringIO()
        with mock.patch.object(chains.subprocess, 'call', side_effect=AssertionError('no w5box')), \
                contextlib.redirect_stderr(err):
            self.assertEqual(chains.main(['run', '--plan', str(path), '--chain', 'c1c']), w5box.EXIT_REFUSED)
        self.assertIn('is not this carried chain', err.getvalue())


    def test_non_canonical_roots_and_a_swap_before_persist_are_refused(self):
        _, old_man = self.old(2, length=32)
        carry_path = self.carry(old_man, 32)
        real = self.t / 'real-disk'
        real.mkdir()
        try:
            (self.t / 'mnt-link').symlink_to(real, target_is_directory=True)
        except OSError:
            self.skipTest('no symlink permission on this host')
        doc = json.loads(Path(self.cont_inputs(carry_path, label='nc', root='nc', segment_updates=32)).read_text())
        doc['root'] = (self.t / 'mnt-link' / 'cont').as_posix()
        with self.assertRaisesRegex(chains.Stop, 'canonical'):
            chains.make_plan(jwrite(self.t / 'nc.json', doc))
        self.assertFalse((real / 'cont').exists())
        # the scratch parent swapped for a link while the trainer runs: no persist
        shm = self.t / 'shm'
        doc = json.loads(Path(self.cont_inputs(carry_path, label='sw', root='sw', segment_updates=32)).read_text())
        doc['scratch_root'] = shm.as_posix()
        doc['arms']['C']['scratch'] = True
        path, plan = chains.make_plan(jwrite(self.t / 'sw.json', doc))
        old_chain = self.node.root / 'chains' / 'c1'
        before = {p: p.read_bytes() for p in old_chain.rglob('*') if p.is_file()}

        def run_and_swap(arm, config_path, deadline, cdir, native):
            Path(native).mkdir(parents=True, exist_ok=True)
            (Path(native) / 'completion.json').write_text('{}')
            (shm / 'c1c').mkdir(parents=True, exist_ok=True)
            (shm / 'c1c').rename(shm / 'c1c.moved')
            (shm / 'c1c').symlink_to(old_chain, target_is_directory=True)
            return 0, False, 0
        with mock.patch.object(chains, 'scratch_link'), \
                mock.patch.object(chains, 'run_reaped', side_effect=run_and_swap), \
                mock.patch.object(chains, 'persist', side_effect=AssertionError('no persist')), \
                contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(self.drive(plan, 'c1c', self.t / 'sw', 1), w5box.EXIT_REFUSED)
        self.assertEqual({p: p.read_bytes() for p in old_chain.rglob('*') if p.is_file()}, before)


    def test_no_link_anywhere_in_a_carried_root(self):
        _, old_man = self.old(3)
        carry_path = self.carry(old_man, 8)
        old_root = self.node.root
        old_chain = old_root / 'chains' / 'c1'
        old_ck = next((old_chain / 'seg-00001' / 'iterations' / '000003').glob('attempt-*/update/checkpoint.json'))
        snap = lambda: {p: p.read_bytes() for p in old_root.rglob('*') if p.is_file() and not p.is_symlink()}
        before = snap()
        # a plan folder linked into the old root: refused before any plan write
        (self.t / 'pl').mkdir()
        try:
            (self.t / 'pl' / 'plan').symlink_to(old_root / 'plan', target_is_directory=True)
        except OSError:
            self.skipTest('no symlink permission on this host')
        with self.assertRaisesRegex(chains.Stop, 'is a link'):
            chains.make_plan(self.cont_inputs(carry_path, label='old', root='pl'))
        self.assertEqual(snap(), before)
        # links planted at a segment config temp, an instance log and the supervisor lock: driver, cycle and run refuse
        path, plan = chains.make_plan(self.cont_inputs(carry_path))
        cont = self.t / 'cont'
        self.assertEqual(self.drive(plan, 'c1c', cont, 1), chains.EXIT_ROOM)
        for planted in (cont / 'LOG.jsonl', cont / 'chains' / 'c1c' / 'segments' / 'seg-00001.json.gz.tmp',
                        cont / 'state' / 'c1c' / 'instance-0000' / 'chain.log',
                        cont / 'state' / 'c1c' / 'supervisor.lock'):
            planted.parent.mkdir(parents=True, exist_ok=True)
            planted.unlink(missing_ok=True)
            planted.symlink_to(old_ck)
            with self.subTest(planted=planted.name):
                with contextlib.redirect_stderr(io.StringIO()):
                    self.assertIn(self.drive(plan, 'c1c', cont, 9), (w5box.EXIT_REFUSED, chains.EXIT_STOP))
                with mock.patch.object(chains, 'occupancy', side_effect=AssertionError('no fills')), \
                        mock.patch.object(chains, 'recover_unsupervised', side_effect=AssertionError('no recovery')):
                    with self.assertRaises(chains.Stop):
                        chains.cycle(plan, path)
            planted.unlink()
            self.assertEqual(snap(), before)
        log_path = cont / 'LOG.jsonl'

        def plant_during_wait(_seconds):  # the link appears while the driver sleeps between guard polls
            log_path.unlink(missing_ok=True)
            log_path.symlink_to(old_ck)
        with mock.patch.object(chains, 'guard_admit', return_value=(False, 'mocked guard wait', chains.GUARD_POLL)), \
                mock.patch.object(chains.time, 'sleep', side_effect=plant_during_wait), \
                mock.patch.object(chains, 'GUARD_WAIT', 3600.0), \
                mock.patch.object(chains, 'run_reaped', side_effect=AssertionError('no run')), \
                contextlib.redirect_stderr(io.StringIO()):
            code = self.drive(plan, 'c1c', cont, 9)
        self.assertEqual(code, chains.EXIT_STOP)
        log_path.unlink(missing_ok=True)
        self.assertEqual(snap(), before)


    def test_links_planted_while_training_or_before_launch_are_refused(self):
        _, old_man = self.old(3)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        cont = self.t / 'cont'
        self.assertEqual(self.drive(plan, 'c1c', cont, 1), chains.EXIT_ROOM)
        old_root = self.node.root
        old_ck = next((old_root / 'chains' / 'c1' / 'seg-00001' / 'iterations' / '000003').glob(
            'attempt-*/update/checkpoint.json'))
        snap = lambda: {p: p.read_bytes() for p in old_root.rglob('*') if p.is_file() and not p.is_symlink()}
        before = snap()
        cdir = cont / 'chains' / 'c1c'
        targets = (cont / 'LOG.jsonl', cdir / 'durable.jsonl', cdir / 'segments' / 'seg-00001.json.gz.tmp')
        for planted in targets:
            for outcome in ('complete', 'cut', 'failed'):
                made = []

                def train_and_plant(arm, config_path, deadline=None, logs=None):
                    native = Path(json.loads(Path(config_path).read_text())['output_directory'])
                    made.append(native)
                    native.mkdir(parents=True, exist_ok=True)
                    if outcome == 'complete':
                        (native / 'completion.json').write_text('{}')
                    planted.parent.mkdir(parents=True, exist_ok=True)
                    planted.unlink(missing_ok=True)
                    try:
                        planted.symlink_to(old_ck)
                    except OSError:
                        self.skipTest('no symlink permission on this host')
                    return (0 if outcome != 'failed' else 1), outcome == 'cut'
                with self.subTest(planted=planted.name, outcome=outcome), \
                        mock.patch.object(chains, 'run_trainer', side_effect=train_and_plant), \
                        contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(self.drive(plan, 'c1c', cont, 99), chains.EXIT_STOP)
                planted.unlink(missing_ok=True)
                for native in made:  # the fake trainer's output is not a real segment: start the next case clean
                    shutil.rmtree(native, ignore_errors=True)
                self.assertEqual(snap(), before)
        # launch: a node lock linked into the old tree is refused before the witness and the lock
        lock = cont / 'state' / 'occupancy.lock'
        lock.parent.mkdir(parents=True, exist_ok=True)
        lock.unlink(missing_ok=True)
        lock.symlink_to(old_ck)
        err = io.StringIO()
        with mock.patch.object(chains, 'settle_vector_tanh', side_effect=AssertionError('no witness')), \
                contextlib.redirect_stderr(err):
            self.assertEqual(chains.main(['launch', '--plan', str(path)]), w5box.EXIT_REFUSED)
        self.assertIn('is a link', err.getvalue())
        lock.unlink()
        self.assertEqual(snap(), before)


    def test_linked_index_and_late_scratch_links_are_refused(self):
        _, old_man = self.old(3)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        cont = self.t / 'cont'
        self.assertEqual(self.drive(plan, 'c1c', cont, 1), chains.EXIT_ROOM)
        old_root = self.node.root
        old_index = old_root / 'chains' / 'c1' / 'chain.index.jsonl'
        retention.index(old_root / 'chains' / 'c1' / 'chain.json')
        snap = lambda: {p: p.read_bytes() for p in old_root.rglob('*') if p.is_file() and not p.is_symlink()}
        before = snap()
        cman = cont / 'chains' / 'c1c' / 'chain.json'
        cindex = cont / 'chains' / 'c1c' / 'chain.index.jsonl'
        saved = cindex.read_bytes() if cindex.exists() else None
        cindex.unlink(missing_ok=True)
        try:
            cindex.symlink_to(old_index)
        except OSError:
            self.skipTest('no symlink permission on this host')
        with self.assertRaises(SystemExit):
            retention.index(cman)
        with self.assertRaises(SystemExit):
            retention.IndexCache(cman).refresh()
        self.assertEqual(snap(), before)
        cindex.unlink()
        # a copied (not linked) predecessor index: its first row is not the carried boundary
        cindex.write_bytes(old_index.read_bytes())
        with self.assertRaisesRegex(SystemExit, 'carried boundary'):
            retention.index(cman)
        cindex.unlink()
        if saved is not None:
            cindex.write_bytes(saved)
        # scratch recovery: a link appearing during the late persist stops before the commit line
        man = json.loads(cman.read_text())
        old_ck = next((old_root / 'chains' / 'c1' / 'seg-00001' / 'iterations' / '000003').glob(
            'attempt-*/update/checkpoint.json'))
        durable = cont / 'chains' / 'c1c' / 'durable.jsonl'

        def persist_and_plant(arm, native, cdir):
            durable.unlink(missing_ok=True)
            durable.symlink_to(old_ck)
            return 0
        seg = {'root': (cont / 'chains' / 'c1c' / 'seg-00009').as_posix()}
        Path(seg['root']).mkdir()
        (Path(seg['root']) / 'completion.json').write_text('{}')
        man['segments'] = man['segments'] + [dict(man['segments'][-1], root=seg['root'])]
        with mock.patch.object(chains, 'mounted', side_effect=lambda p: Path(p) == Path(seg['root'])), \
                mock.patch.object(chains, 'persist', side_effect=persist_and_plant), \
                mock.patch.object(chains, 'record_commit', side_effect=AssertionError('no commit')):
            with self.assertRaises(chains.Stop):
                chains.scratch_settle({'arm': 'C'}, cont, cont / 'chains' / 'c1c', man)
        durable.unlink()
        self.assertEqual(snap(), before)


    def test_entry_points_intra_segment_links_and_hold_links_are_refused(self):
        _, old_man = self.old(3)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        cont = self.t / 'cont'
        self.assertEqual(self.drive(plan, 'c1c', cont, 1), chains.EXIT_ROOM)
        old_root = self.node.root
        old_ck = next((old_root / 'chains' / 'c1' / 'seg-00001' / 'iterations' / '000003').glob(
            'attempt-*/update/checkpoint.json'))
        snap = lambda: {p: p.read_bytes() for p in old_root.rglob('*') if p.is_file() and not p.is_symlink()}
        before = snap()
        # a dangling SUPERVISE.lock link into the old tree: supervise refuses before the lock
        lock = cont / 'SUPERVISE.lock'
        lock.unlink(missing_ok=True)
        try:
            lock.symlink_to(old_root / 'chains' / 'c1' / 'NEW-FILE')
        except OSError:
            self.skipTest('no symlink permission on this host')
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(chains.main(['supervise', '--plan', str(path)]), w5box.EXIT_REFUSED)
        self.assertFalse((old_root / 'chains' / 'c1' / 'NEW-FILE').exists())
        lock.unlink()
        self.assertEqual(snap(), before)
        # an iteration linked to another iteration of the same segment: indexing refuses
        cman = cont / 'chains' / 'c1c' / 'chain.json'
        cindex = cont / 'chains' / 'c1c' / 'chain.index.jsonl'
        cindex.unlink(missing_ok=True)
        its = cont / 'chains' / 'c1c' / 'seg-00000' / 'iterations'
        names = sorted(p.name for p in its.iterdir())
        victim = its / names[-1]
        victim.rename(its / (names[-1] + '.moved'))
        victim.symlink_to(its / names[0], target_is_directory=True)
        with self.assertRaises(SystemExit):
            retention.index(cman)
        victim.unlink()
        (its / (names[-1] + '.moved')).rename(victim)
        # a carried plan's linked hold folder
        (self.t / 'hold-real').mkdir()
        (self.t / 'hold-link').symlink_to(self.t / 'hold-real', target_is_directory=True)
        with self.assertRaisesRegex(SystemExit, 'hold folder'):
            retention.apply({'own_dir': (cont / 'chains' / 'c1c').resolve().as_posix(), 'checkpoints': [],
                             'trajectories': []}, cont / 'retention' / 'c1c.jsonl', hold=self.t / 'hold-link')
        self.assertEqual(snap(), before)


    def test_cache_copied_index_hold_destination_and_replay_links_are_refused(self):
        _, old_man = self.old(3)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        cont = self.t / 'cont'
        self.assertEqual(self.drive(plan, 'c1c', cont, 1), chains.EXIT_ROOM)
        old_root = self.node.root
        old_ck = next((old_root / 'chains' / 'c1' / 'seg-00001' / 'iterations' / '000003').glob(
            'attempt-*/update/checkpoint.json'))
        snap = lambda: {p: p.read_bytes() for p in old_root.rglob('*') if p.is_file() and not p.is_symlink()}
        retention.index(old_root / 'chains' / 'c1' / 'chain.json')
        before = snap()
        cman = cont / 'chains' / 'c1c' / 'chain.json'
        cindex = cont / 'chains' / 'c1c' / 'chain.index.jsonl'
        cindex.unlink(missing_ok=True)
        cindex.write_bytes((old_root / 'chains' / 'c1' / 'chain.index.jsonl').read_bytes())
        for read_only in (False, True):
            with self.subTest(read_only=read_only):
                with self.assertRaisesRegex(SystemExit, 'carried boundary'):
                    retention.IndexCache(cman, read_only=read_only).refresh()
        cindex.unlink()
        # a good cache, then the file replaced by the copy: the rebuild refuses
        cache = retention.IndexCache(cman)
        cache.refresh()
        cindex.unlink()
        cindex.write_bytes((old_root / 'chains' / 'c1' / 'chain.index.jsonl').read_bytes())
        with self.assertRaisesRegex(SystemExit, 'carried boundary'):
            cache.refresh()
        cindex.unlink()
        # a real hold folder with a link below it, at a destination's parent
        hold = self.t / 'hold'
        victim = cont / 'chains' / 'c1c' / 'victim.json'
        victim.write_text('{}')
        dst_parent = hold / Path(victim).parent.relative_to(Path(victim).anchor)
        dst_parent.parent.mkdir(parents=True)
        try:
            dst_parent.symlink_to(old_ck.parent, target_is_directory=True)
        except OSError:
            self.skipTest('no symlink permission on this host')
        with self.assertRaisesRegex(SystemExit, 'hold destination'):
            retention.apply({'own_dir': (cont / 'chains' / 'c1c').resolve().as_posix(),
                             'checkpoints': [{'path': victim.as_posix()}], 'trajectories': []},
                            cont / 'retention' / 'c1c.jsonl', hold=hold)
        self.assertTrue(victim.exists())
        # the replay worker with a planted trainer.out link in a replay folder of the carried plan
        rdir = cont / 'replay-test'
        rdir.mkdir()
        (rdir / 'trainer.out').symlink_to(old_ck)
        err = io.StringIO()
        with mock.patch.object(chains, 'run_trainer', side_effect=AssertionError('no trainer')), \
                contextlib.redirect_stderr(err):
            self.assertEqual(chains.main(['_replay', '--plan', str(path), '--dir', str(rdir)]), w5box.EXIT_REFUSED)
        self.assertIn('is a link', err.getvalue())
        self.assertEqual(snap(), before)


    def test_carried_retention_log_and_standalone_outputs_are_confined(self):
        _, old_man = self.old(3)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        cont = self.t / 'cont'
        self.assertEqual(self.drive(plan, 'c1c', cont, 1), chains.EXIT_ROOM)
        old_root = self.node.root
        old_ck = next((old_root / 'chains' / 'c1' / 'seg-00001' / 'iterations' / '000003').glob(
            'attempt-*/update/checkpoint.json'))
        snap = lambda: {p: p.read_bytes() for p in old_root.rglob('*') if p.is_file() and not p.is_symlink()}
        before = snap()
        victim = cont / 'chains' / 'c1c' / 'victim.json'
        victim.write_text('{}')
        doc = {'own_dir': (cont / 'chains' / 'c1c').resolve().as_posix(),
               'checkpoints': [{'path': victim.resolve().as_posix()}], 'trajectories': []}
        with self.assertRaisesRegex(SystemExit, 'carried plan logs to'):
            retention.apply(doc, old_ck)
        self.assertTrue(victim.exists())
        self.assertEqual(snap(), before)
        # replay-config with a stale linked temporary output
        out = cont / 'replay-x' / 'config.json'
        out.parent.mkdir()
        try:
            (out.parent / 'config.json.tmp').symlink_to(old_ck)
        except OSError:
            self.skipTest('no symlink permission on this host')
        with self.assertRaisesRegex(SystemExit, 'is a link'):
            retention.carried_output_unlinked(out, out.parent / 'native')
        self.assertEqual(snap(), before)


    def test_delta_head_check_blank_lines_empty_cache_and_dotdot_outputs(self):
        _, old_man = self.old(3)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        cont = self.t / 'cont'
        old_root = self.node.root
        retention.index(old_root / 'chains' / 'c1' / 'chain.json')
        old_rows = (old_root / 'chains' / 'c1' / 'chain.index.jsonl').read_bytes()
        cman = cont / 'chains' / 'c1c' / 'chain.json'
        cindex = cont / 'chains' / 'c1c' / 'chain.index.jsonl'
        # a leading blank line before copied predecessor rows
        cindex.write_bytes(b'\n' + old_rows)
        with self.assertRaisesRegex(SystemExit, 'carried boundary'):
            retention.index(cman)
        with self.assertRaisesRegex(SystemExit, 'carried boundary'):
            retention.IndexCache(cman, read_only=True).refresh()
        # an empty cached index, then predecessor rows written into the same file
        cindex.write_bytes(b'')
        cache = retention.IndexCache(cman)
        cache.refresh()
        with open(cindex, 'ab') as f:
            f.write(old_rows)
        with self.assertRaisesRegex(SystemExit, 'carried boundary'):
            cache.refresh()
        cindex.unlink()
        # standalone outputs with a '..' component
        with self.assertRaisesRegex(SystemExit, 'component'):
            retention.carried_output_unlinked(cont / 'x' / '..' / 'config.json')
        # segcheck refuses a carried plan
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            self.assertEqual(chains.main(['segcheck', '--plan', str(path), '--arm', 'C', '--out', str(self.t / 'sc.json')]), w5box.EXIT_REFUSED)
        self.assertIn('carried plan', err.getvalue())


    def test_delta2_read_only_cache_and_qualification_outputs(self):
        _, old_man = self.old(3)
        path, plan = chains.make_plan(self.cont_inputs(self.carry(old_man, 8)))
        cont = self.t / 'cont'
        self.assertEqual(self.drive(plan, 'c1c', cont, 1), chains.EXIT_ROOM)
        old_root = self.node.root
        retention.index(old_root / 'chains' / 'c1' / 'chain.json')
        old_rows = (old_root / 'chains' / 'c1' / 'chain.index.jsonl').read_bytes()
        cman = cont / 'chains' / 'c1c' / 'chain.json'
        cindex = cont / 'chains' / 'c1c' / 'chain.index.jsonl'
        for empty in (b'', b'\n', b'\n\n'):  # zero bytes, or blank lines only
            with self.subTest(empty=empty):
                cindex.write_bytes(empty)
                ro = retention.IndexCache(cman, read_only=True)
                first = ro.refresh()[1]
                self.assertTrue(len(first) > 0)  # unwritten continuation rows held in memory, no file rows
                with open(cindex, 'ab') as f:
                    f.write(old_rows)
                with self.assertRaisesRegex(SystemExit, 'carried boundary'):
                    ro.refresh()
        cindex.unlink()
        # a qualification record destination through a linked parent is refused before the lock
        (self.t / 'out-link').symlink_to(old_root / 'chains' / 'c1', target_is_directory=True)
        with self.assertRaises(SystemExit):
            chains.throughput_run(str(path), 'C', 'q', 64, str(self.t / 'out-link' / 'rec.json'), [1])
        self.assertFalse((old_root / 'chains' / 'c1' / 'rec.json').exists())


if __name__ == '__main__':
    unittest.main()
