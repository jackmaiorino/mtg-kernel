"""W5 trainer re-pin (tools/w5/chains.py `repin`, the `repin` chain input): a running carried chain switches to another
trainer build at a clean segment end without a new chain, root or index; only the trainer pin changes, the manifest
records it in `trainer_history`, and with a build that computes the same states the chain reaches exactly the states
of an uninterrupted run. Stand-in trainer (fake_trainer.py), as test_w5_chains.py."""
import contextlib
import hashlib
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
import w5box  # noqa: E402
from test_w5_chains import FAKE, STEP_C, Node, jwrite  # noqa: E402

SEED = 'oc/c1'


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def after_states(native):
    return [json.loads(Path(json.loads(p.read_text())['update']['path']).read_text())['after_state_sha256']
            for p in sorted((Path(native) / 'iterations').glob('*/complete.json'))]


class RepinTests(unittest.TestCase):
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
        self.throughput = jwrite(self.t / 'throughput.json', {'schema': 'test-throughput-record'})
        # the old chain c1 to boundary 8, held; the carried continuation c1c (the re-pinned chain) from 8
        doc = json.loads(Path(self.node.inputs('0', {'c1': {'arm': 'C', 'slot': 0, 'seed_root': SEED}},
                                               label='old', c_wall_seconds=600.0, independent=True)).read_text())
        _, self.old_plan = chains.make_plan(jwrite(self.t / 'inputs-old.json', doc))
        self.node.guard(ceilings={'c1': 2})
        self.assertEqual(self.node.drive(self.old_plan, 'c1'), chains.EXIT_ROOM)
        (self.node.root / 'chains' / 'c1' / 'STOP').write_text('carried\n')
        self.carry = self.t / 'carry' / 'carry.json'
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(chains.main(['carry', '--manifest', str(self.node.root / 'chains' / 'c1' / 'chain.json'),
                                          '--boundary', '8', '--objects', str(self.t / 'carry' / 'objects'),
                                          '--out', str(self.carry)]), 0)
        self.cont = self.t / 'cont'

    def tearDown(self):
        self.tmp.cleanup()

    # ------------------------------------------------------------ helpers

    def build(self, name='trainer-b.py'):
        path = self.t / name
        if not path.exists():
            path.write_text(FAKE.read_text() + f'\n# another build: {name}\n')
        return path

    def inputs(self, trainer=None, throughput=False, repin=None, bytes_per_update=None, memory=None):
        spec = {'arm': 'C', 'slot': 0, 'seed_root': SEED, 'carry': str(self.carry)}
        if repin:
            spec['repin'] = str(repin)
        extra = {'trainer': str(trainer)} if trainer else {}
        doc = json.loads(Path(self.node.inputs('0', {'c1c': spec}, label='main', c_wall_seconds=600.0,
                                               independent=True, **extra)).read_text())
        doc['root'] = self.cont.as_posix()
        if throughput:
            doc['arms']['C']['throughput'] = str(self.throughput)
        if bytes_per_update:
            doc['arms']['C']['bytes_per_update'] = bytes_per_update
        if memory:
            doc['arms']['C']['memory_max_bytes'] = memory
        return jwrite(self.t / 'inputs.json', doc)

    def drive(self, plan, segments):
        root = self.cont
        source = root / 'state' / 'c1c' / 'ledger-source.json'
        if not source.exists():
            stat = root / 'state' / 'c1c' / 'cpu.stat'
            jwrite(stat, {})
            stat.write_text('usage_usec 0\n')
            jwrite(source, {'schema': 'w5-chain-ledger-source/v1', 'closed_usec': 0, 'instance': 0,
                            'cpu_stat': stat.as_posix()})
        jwrite(root / 'GUARD.json', {'ok': True, 'unix': time.time(), 'used_bytes': 0, 'cap_bytes': 1,
                                     'ceilings': {'c1c': segments}, 'headroom': 10**15})
        c = plan['chains']['c1c']
        with contextlib.redirect_stderr(io.StringIO()):
            return chains.main(['_drive', '--arm-file', plan['arms'][c['arm']]['file']['path'], '--chain', 'c1c',
                                '--seed-root', c['seed_root'], '--state', str(root / 'state' / 'c1c'),
                                '--ledger-source', str(source)])

    def start(self, segments):
        path, plan = chains.make_plan(self.inputs(throughput=True))
        self.assertEqual(self.drive(plan, segments), chains.EXIT_ROOM)
        return path, plan

    def cdir(self):
        return self.cont / 'chains' / 'c1c'

    def man(self):
        return json.loads((self.cdir() / 'chain.json').read_text())

    def stop(self):
        (self.cdir() / 'STOP').write_text('repin stop\n')

    def request(self, plan, old, new, rid='r1', c_equal=4, name=None, **over):
        man = self.man()
        segs = man['segments']
        arm = json.loads(Path(plan['arms']['C']['file']['path']).read_text())
        config_sha, source = chains.repin_next_config_sha256(arm, man)
        mp = (self.cdir() / 'chain.json').resolve()
        boundary = segs[-1]['offset'] + segs[-1]['updates']
        proof = {'schema': chains.REPIN_PROOF_SCHEMA, 'chain': 'c1c',
                 'manifest': {'path': mp.as_posix(), 'sha256': sha(mp)}, 'boundary': boundary,
                 'last_segment': len(segs) - 1, 'next_segment': len(segs), 'old_sha256': sha(old),
                 'new_sha256': sha(new),
                 'gate_a': {'segment': len(segs) - 1, 'updates': 4, 'equal': 4,
                            'config': chains.pin(segs[-1]['config'])},
                 'gate_c': {'segment': len(segs), 'updates': 4, 'equal': c_equal, 'config_sha256': config_sha,
                            'source_checkpoint': source['checkpoint']}, 'pass': True}
        proof.update(over)
        name = name or f'req-{rid}.json'
        proof_path = jwrite(self.t / f'proof-{name}', proof)
        return jwrite(self.t / name, {'schema': chains.REPIN_SCHEMA, 'chain': 'c1c', 'id': rid,
                                      'from_sha256': sha(old), 'to_sha256': sha(new), 'boundary': boundary,
                                      'segment': len(segs), 'proof': chains.pin(proof_path)})

    def repin(self, plan_path, step, request=None):
        args = ['repin', step, '--plan', str(plan_path), '--chain', 'c1c'] + \
            (['--request', str(request)] if request else [])
        err = io.StringIO()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(err):
            code = chains.main(args)
        return code, err.getvalue()

    def whole(self, plan, updates):
        arm = chains.load_arm(plan['arms']['C']['file']['path'])
        cfg = chains.arm_config(arm, SEED, 0, updates, self.t / f'whole-{updates}')
        self.assertEqual(chains.run_trainer(arm, jwrite(self.t / f'whole-{updates}.json', cfg))[0], 0)
        return after_states(self.t / f'whole-{updates}')

    def switch(self, plan, plan_path, old, new, rid='r1'):
        req = self.request(plan, old, new, rid=rid)
        self.assertEqual(self.repin(plan_path, 'begin', req), (0, ''))
        chains.make_plan(self.inputs(trainer=new, repin=req))
        path, plan = chains.make_plan(self.inputs(trainer=new, repin=req, throughput=True))  # citing the record
        self.assertEqual(self.repin(path, 'end')[0], 0)
        return path, plan

    # ------------------------------------------------------------ tests

    def test_a_repinned_chain_reaches_the_states_of_an_uninterrupted_run(self):
        path, plan = self.start(2)  # boundaries 9..16 on the first trainer
        ref = self.whole(plan, 28)
        self.stop()
        new = self.build()
        path, plan = self.switch(plan, path, FAKE, new)
        man = self.man()
        self.assertEqual(man['recipe']['trainer_sha256'], sha(new))
        self.assertEqual(man['trainer_history_origin'], sha(FAKE))
        self.assertEqual([(e['id'], e['segment'], e['boundary'], e['from_sha256'], e['to_sha256'])
                          for e in man['trainer_history']], [('r1', 2, 16, sha(FAKE), sha(new))])
        self.assertEqual(plan['trainer']['sha256'], sha(new))
        self.assertEqual(man['carry']['boundary'], 8)  # numbering, seed root and wall: the carried ones
        (self.cdir() / 'STOP').unlink()
        self.assertEqual(self.drive(plan, 4), chains.EXIT_ROOM)
        _, records = retention.index(self.cdir() / 'chain.json')
        self.assertEqual([r['after_state_sha256'] for r in records], ref[8:24])  # one uninterrupted run
        self.assertEqual([r['boundary'] for r in records], list(range(9, 25)))
        man = self.man()
        self.assertEqual([chains.trainer_of_segment(man, r['segment']) for r in records],
                         [sha(FAKE)] * 8 + [sha(new)] * 8)
        # the way back: the same path, one more entry; the chain continues the same states
        self.stop()
        path, plan = self.switch(plan, path, new, FAKE, rid='r2')
        self.assertEqual([e['to_sha256'] for e in self.man()['trainer_history']], [sha(new), sha(FAKE)])
        (self.cdir() / 'STOP').unlink()
        self.assertEqual(self.drive(plan, 5), chains.EXIT_ROOM)
        _, records = retention.index(self.cdir() / 'chain.json')
        self.assertEqual([r['after_state_sha256'] for r in records], ref[8:28])

    def test_a_repin_is_refused_unless_held_proven_bound_and_for_the_trainer_only(self):
        path, plan = self.start(2)  # c1c at 16
        new = self.build()
        req = self.request(plan, FAKE, new)
        self.assertNotEqual(self.repin(path, 'begin', req)[0], 0)  # no STOP: it may run
        self.stop()
        for over, why in (({'gate_c': dict(json.loads(Path(json.loads(req.read_text())['proof']['path'])
                                                       .read_text())['gate_c'], equal=3)}, 'gate_c'),
                          ({'pass': False}, 'did not pass'),
                          ({'rehearsal': True}, 'rehearsal'),
                          ({'manifest': {'path': '/elsewhere/chain.json', 'sha256': '0' * 64}}, 'another manifest')):
            bad = self.request(plan, FAKE, new, rid='bad', name='bad.json', **over)
            code, err = self.repin(path, 'begin', bad)
            self.assertEqual(code, w5box.EXIT_REFUSED, why)
            self.assertIn(why, err)
        gate_c = json.loads(Path(json.loads(req.read_text())['proof']['path']).read_text())['gate_c']
        bad = self.request(plan, FAKE, new, rid='bad', name='bad2.json',
                           **{'gate_c': dict(gate_c, config_sha256='0' * 64)})
        self.assertIn('another configuration', self.repin(path, 'begin', bad)[1])
        self.assertFalse((self.cdir() / chains.REPIN_PENDING).exists())
        with self.assertRaisesRegex(chains.Stop, 'not pending'):
            chains.make_plan(self.inputs(trainer=new, repin=req))
        self.assertEqual(self.repin(path, 'begin', req)[0], 0)
        # anything but the trainer changes: refused before the commit, nothing recorded
        with self.assertRaisesRegex(chains.Stop, 'would change'):
            chains.make_plan(self.inputs(trainer=new, repin=req, bytes_per_update=999))
        with self.assertRaisesRegex(chains.Stop, 'memory limits would change'):
            chains.make_plan(self.inputs(trainer=new, repin=req, memory=1 << 29))
        self.assertNotIn('trainer_history', self.man())
        with self.assertRaisesRegex(chains.Stop, "not the request's new trainer"):
            chains.make_plan(self.inputs(trainer=self.build('trainer-c.py'), repin=req))
        # other request bytes under the pending id
        req2 = json.loads(req.read_text())
        req2['note'] = 'other bytes'
        with self.assertRaisesRegex(chains.Stop, 'not pending with these request bytes'):
            chains.make_plan(self.inputs(trainer=new, repin=jwrite(self.t / 'req-other.json', req2)))
        # while pending: no driver, run or supervise starts the chain
        (self.cdir() / 'STOP').unlink()
        self.assertEqual(self.drive(plan, 3), w5box.EXIT_REFUSED)
        jwrite(self.cont / 'plan' / 'main' / 'LAUNCHED.json', {'opened': [0]})
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(chains.main(['run', '--plan', str(path), '--chain', 'c1c']), w5box.EXIT_REFUSED)
        with self.assertRaisesRegex(chains.Stop, 're-pin is pending'):
            chains.cycle(plan, path)
        # abort before the commit (held again): the old pins were never touched
        self.stop()
        self.assertEqual(self.repin(path, 'abort')[0], 0)
        self.assertEqual(self.man()['recipe']['trainer_sha256'], sha(FAKE))
        (self.cdir() / 'STOP').unlink()
        self.assertEqual(self.drive(plan, 3), chains.EXIT_ROOM)

    def test_commit_once_rerun_end_and_revert_while_not_run(self):
        path, plan = self.start(2)
        self.stop()
        new = self.build()
        req = self.request(plan, FAKE, new)
        self.assertEqual(self.repin(path, 'begin', req)[0], 0)
        self.assertEqual(self.repin(path, 'begin', req)[0], 0)  # the same request again: idempotent
        self.assertIn('not committed', self.repin(path, 'end')[1])
        chains.make_plan(self.inputs(trainer=new, repin=req))  # the commit (no throughput record yet)
        path, plan = chains.make_plan(self.inputs(trainer=new, repin=req))  # again: no second entry
        self.assertEqual(len(self.man()['trainer_history']), 1)
        self.assertEqual(self.repin(path, 'begin', req)[0], 0)  # a rerun after the commit: recognised first
        self.assertIn('throughput record', self.repin(path, 'end')[1])
        # the qualification failed: revert while the chain still stands at the boundary, then the launched pins;
        # a crash between the revert entry and the pending record's removal is finished by a second abort
        real_unlink = Path.unlink

        def crash(p, *args, **kwargs):
            if p.name == chains.REPIN_PENDING:
                raise OSError('crash before removal')
            return real_unlink(p, *args, **kwargs)
        with mock.patch.object(Path, 'unlink', crash), self.assertRaises(OSError):
            self.repin(path, 'abort')
        self.assertTrue((self.cdir() / chains.REPIN_PENDING).exists())
        code, err = self.repin(path, 'abort')
        self.assertEqual(code, 0, err)
        man = self.man()
        self.assertEqual(man['recipe']['trainer_sha256'], sha(FAKE))
        self.assertEqual([(e['id'], e.get('reverts')) for e in man['trainer_history']], [('r1', None), ('r1-revert', 'r1')])
        self.assertEqual(chains.trainer_of_segment(man, 2), sha(FAKE))
        path, plan = chains.make_plan(self.inputs(throughput=True))
        (self.cdir() / 'STOP').unlink()
        self.assertEqual(self.drive(plan, 3), chains.EXIT_ROOM)
        # a full switch, then end once and a rerun of end after it
        self.stop()
        path, plan = self.switch(plan, path, FAKE, new, rid='r3')
        self.assertEqual(self.repin(path, 'end')[0], 0)
        self.assertFalse((self.cdir() / chains.REPIN_PENDING).exists())
        chains.make_plan(self.inputs(trainer=new, throughput=True))  # without the request: the new trainer stays
        with self.assertRaisesRegex(chains.Stop, 'recipe changed'):
            chains.make_plan(self.inputs(throughput=True))

    def test_an_ordinary_chain_and_links_are_refused(self):
        path, plan = self.start(2)
        self.stop()
        new = self.build()
        req = self.request(plan, FAKE, new)
        target = self.t / 'elsewhere.json'
        target.write_text('{}')
        link = self.cdir() / chains.REPIN_PENDING
        try:
            link.symlink_to(target)
        except OSError:
            self.skipTest('no symlink permission on this host')
        code, err = self.repin(path, 'begin', req)
        self.assertEqual(code, w5box.EXIT_REFUSED)
        self.assertIn('is a link', err)
        self.assertEqual(target.read_text(), '{}')
        link.unlink()
        # the old chain is ordinary: re-pin v1 refuses it
        old_path = self.node.root / 'plan' / 'old' / 'CHAINS-PLAN.json'
        man = json.loads((self.node.root / 'chains' / 'c1' / 'chain.json').read_text())
        req_doc = json.loads(req.read_text())
        req_doc.update(chain='c1', boundary=8, segment=2, from_sha256=man['recipe']['trainer_sha256'])
        err = io.StringIO()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(err):
            code = chains.main(['repin', 'begin', '--plan', str(old_path), '--chain', 'c1',
                                '--request', str(jwrite(self.t / 'req-old.json', req_doc))])
        self.assertEqual(code, w5box.EXIT_REFUSED)
        self.assertIn('carried chains', err.getvalue())


if __name__ == '__main__':
    unittest.main()
