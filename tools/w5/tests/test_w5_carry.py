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

    def old(self, segments, c_wall=600.0):
        """The old chain c1 (4-update segments) run for `segments` segments; returns (plan, manifest path)."""
        _, plan = self.node.plan('0', {'c1': {'arm': 'C', 'slot': 0, 'seed_root': SEED}}, c_wall_seconds=c_wall,
                                 independent=True, label='old')
        self.node.guard(ceilings={'c1': segments})
        self.assertEqual(self.node.drive(plan, 'c1'), chains.EXIT_ROOM)
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
        self.assertEqual(chains.committed_through(man_path.parent), 20)  # commits counted from B

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


if __name__ == '__main__':
    unittest.main()
