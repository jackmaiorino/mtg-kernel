"""W5 trainer re-pin (tools/w5/chains.py `repin`, the `repin` chain input): a running chain switches to another trainer
build at a clean segment end without a new chain, root or index; only the trainer pin changes, the manifest records
it in `trainer_history`, and with a build that computes the same states the chain reaches exactly the states of an
uninterrupted run. Stand-in trainer (fake_trainer.py), as test_w5_chains.py."""
import contextlib
import hashlib
import io
import json
import os
import sys
import tempfile
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

    def tearDown(self):
        self.tmp.cleanup()

    # ------------------------------------------------------------ helpers

    def build(self, name='trainer-b.py'):
        path = self.t / name
        if not path.exists():
            path.write_text(FAKE.read_text() + f'\n# another build: {name}\n')
        return path

    def inputs(self, trainer=None, throughput=False, repin=None, workers=None):
        spec = {'arm': 'C', 'slot': 0, 'seed_root': SEED}
        if repin:
            spec['repin'] = str(repin)
        extra = {'trainer': str(trainer)} if trainer else {}
        doc = json.loads(Path(self.node.inputs('0', {'c1': spec}, label='main', c_wall_seconds=600.0,
                                               independent=True, **extra)).read_text())
        if throughput:
            doc['arms']['C']['throughput'] = str(self.throughput)
        if workers:
            doc['arms']['C']['workers'] = workers
        return jwrite(self.t / 'inputs.json', doc)

    def start(self, segments):
        path, plan = chains.make_plan(self.inputs(throughput=True))
        self.node.guard(ceilings={'c1': segments})
        self.assertEqual(self.node.drive(plan, 'c1'), chains.EXIT_ROOM)
        return path, plan

    def man_path(self):
        return self.node.root / 'chains' / 'c1' / 'chain.json'

    def stop(self):
        (self.node.root / 'chains' / 'c1' / 'STOP').write_text('repin stop\n')

    def request(self, old, new, boundary, segment, rid='r1', gate_a=4, gate_c=4, name='req.json', **proof_over):
        proof = dict({'schema': chains.REPIN_PROOF_SCHEMA, 'chain': 'c1', 'boundary': boundary,
                      'next_segment': segment, 'old_sha256': sha(old), 'new_sha256': sha(new),
                      'gate_a': {'updates': 4, 'equal': gate_a}, 'gate_c': {'updates': 4, 'equal': gate_c},
                      'pass': True}, **proof_over)
        proof_path = jwrite(self.t / f'proof-{name}', proof)
        return jwrite(self.t / name, {'schema': chains.REPIN_SCHEMA, 'chain': 'c1', 'id': rid,
                                      'from_sha256': sha(old), 'to_sha256': sha(new), 'boundary': boundary,
                                      'segment': segment, 'proof': chains.pin(proof_path)})

    def repin(self, plan_path, step, request=None):
        args = ['repin', step, '--plan', str(plan_path), '--chain', 'c1'] + (['--request', str(request)] if request else [])
        err = io.StringIO()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(err):
            code = chains.main(args)
        return code, err.getvalue()

    def whole(self, plan, updates):
        arm = chains.load_arm(plan['arms']['C']['file']['path'])
        cfg = chains.arm_config(arm, SEED, 0, updates, self.t / 'whole')
        self.assertEqual(chains.run_trainer(arm, jwrite(self.t / 'whole.json', cfg))[0], 0)
        return after_states(self.t / 'whole')

    def switch(self, plan_path, old, new, boundary, segment, rid='r1'):
        req = self.request(old, new, boundary, segment, rid=rid, name=f'req-{rid}.json')
        self.assertEqual(self.repin(plan_path, 'begin', req), (0, ''))
        path, plan = chains.make_plan(self.inputs(trainer=new, repin=req))
        path, plan = chains.make_plan(self.inputs(trainer=new, repin=req, throughput=True))  # citing the record
        self.assertEqual(self.repin(path, 'end')[0], 0)
        return path, plan

    # ------------------------------------------------------------ tests

    def test_a_repinned_chain_reaches_the_states_of_an_uninterrupted_run(self):
        path, plan = self.start(3)  # boundaries 1..12 on the first trainer
        ref = self.whole(plan, 24)
        self.stop()
        new = self.build()
        path, plan = self.switch(path, FAKE, new, 12, 3)
        man = json.loads(self.man_path().read_text())
        self.assertEqual(man['recipe']['trainer_sha256'], sha(new))
        self.assertEqual(man['trainer_history_origin'], sha(FAKE))
        self.assertEqual([(e['id'], e['segment'], e['boundary'], e['from_sha256'], e['to_sha256'])
                          for e in man['trainer_history']], [('r1', 3, 12, sha(FAKE), sha(new))])
        self.assertEqual(plan['trainer']['sha256'], sha(new))
        self.assertEqual(plan['chains']['c1']['repin']['sha256'], sha(self.t / 'req-r1.json'))
        (self.node.root / 'chains' / 'c1' / 'STOP').unlink()
        self.node.guard(ceilings={'c1': 6})
        self.assertEqual(self.node.drive(plan, 'c1'), chains.EXIT_ROOM)
        chain, records = retention.index(self.man_path())
        self.assertEqual([r['after_state_sha256'] for r in records], ref)  # one uninterrupted run, both builds
        self.assertEqual([r['boundary'] for r in records], list(range(1, 25)))
        man = json.loads(self.man_path().read_text())
        self.assertEqual([chains.trainer_of_segment(man, r['segment']) for r in records],
                         [sha(FAKE)] * 12 + [sha(new)] * 12)
        # the way back: the same path, one more entry; the chain still continues the same states
        self.stop()
        path, plan = self.switch(path, new, FAKE, 24, 6, rid='r2')
        man = json.loads(self.man_path().read_text())
        self.assertEqual([e['to_sha256'] for e in man['trainer_history']], [sha(new), sha(FAKE)])
        (self.node.root / 'chains' / 'c1' / 'STOP').unlink()
        self.node.guard(ceilings={'c1': 7})
        self.assertEqual(self.node.drive(plan, 'c1'), chains.EXIT_ROOM)
        _, records = retention.index(self.man_path())
        self.assertEqual([r['after_state_sha256'] for r in records], self.whole(plan, 28))
        self.assertEqual(chains.trainer_of_segment(man, 6), sha(FAKE))

    def test_a_repin_is_refused_unless_held_proven_and_for_the_trainer_only(self):
        path, plan = self.start(2)  # boundary 8
        new = self.build()
        req = self.request(FAKE, new, 8, 2)
        self.assertNotEqual(self.repin(path, 'begin', req)[0], 0)  # no STOP: it may run
        self.stop()
        for bad, why in ((self.request(FAKE, new, 4, 1, name='b1.json'), 'stands at 8'),
                         (self.request(FAKE, new, 8, 2, gate_c=3, name='b2.json'), 'gate_c'),
                         (self.request(new, FAKE, 8, 2, name='b3.json'), 're-pins from'),
                         (self.request(FAKE, new, 8, 2, name='b4.json', **{'pass': False}), 'did not pass')):
            code, err = self.repin(path, 'begin', bad)
            self.assertEqual(code, w5box.EXIT_REFUSED, why)
            self.assertIn(why, err)
        self.assertFalse((self.node.root / 'chains' / 'c1' / chains.REPIN_PENDING).exists())
        # the plan refuses a request that is not pending
        with self.assertRaisesRegex(chains.Stop, 'not pending'):
            chains.make_plan(self.inputs(trainer=new, repin=req))
        self.assertEqual(self.repin(path, 'begin', req)[0], 0)
        # anything but the trainer changes: refused, nothing recorded
        with self.assertRaisesRegex(chains.Stop, 'trainer only'):
            chains.make_plan(self.inputs(trainer=new, repin=req, workers=2))
        self.assertNotIn('trainer_history', json.loads(self.man_path().read_text()))
        # the plan trainer is not the request's
        with self.assertRaisesRegex(chains.Stop, "not the request's new trainer"):
            chains.make_plan(self.inputs(trainer=self.build('trainer-c.py'), repin=req))
        # while pending: no driver, run or supervise starts the chain
        (self.node.root / 'chains' / 'c1' / 'STOP').unlink()
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(self.node.drive(plan, 'c1'), w5box.EXIT_REFUSED)
            jwrite(self.node.root / 'plan' / 'main' / 'LAUNCHED.json', {'opened': [0]})
            self.assertEqual(chains.main(['run', '--plan', str(path), '--chain', 'c1']), w5box.EXIT_REFUSED)
        with self.assertRaisesRegex(chains.Stop, 're-pin is pending'):
            chains.cycle(plan, path)
        # abort before the commit: the old pins were never touched
        self.assertEqual(self.repin(path, 'abort')[0], 0)
        self.assertEqual(json.loads(self.man_path().read_text())['recipe']['trainer_sha256'], sha(FAKE))
        self.node.guard(ceilings={'c1': 3})
        self.assertEqual(self.node.drive(plan, 'c1'), chains.EXIT_ROOM)

    def test_commit_is_once_and_end_needs_the_new_pins_and_a_throughput_record(self):
        path, plan = self.start(2)
        self.stop()
        new = self.build()
        req = self.request(FAKE, new, 8, 2)
        self.assertEqual(self.repin(path, 'begin', req)[0], 0)
        self.assertEqual(self.repin(path, 'begin', req)[0], 0)  # the same request again: idempotent
        code, err = self.repin(path, 'end')
        self.assertIn('not committed', err)
        path, plan = chains.make_plan(self.inputs(trainer=new, repin=req))  # no throughput record yet
        path, plan = chains.make_plan(self.inputs(trainer=new, repin=req))  # again: no second entry
        self.assertEqual(len(json.loads(self.man_path().read_text())['trainer_history']), 1)
        code, err = self.repin(path, 'end')
        self.assertIn('throughput record', err)
        code, err = self.repin(path, 'abort')
        self.assertIn('committed', err)
        path, plan = chains.make_plan(self.inputs(trainer=new, repin=req, throughput=True))
        self.assertEqual(self.repin(path, 'end')[0], 0)
        self.assertFalse((self.node.root / 'chains' / 'c1' / chains.REPIN_PENDING).exists())
        # after the end, a plan without the request keeps the new trainer; the old trainer is refused
        chains.make_plan(self.inputs(trainer=new, throughput=True))
        with self.assertRaisesRegex(chains.Stop, 'recipe changed'):
            chains.make_plan(self.inputs(throughput=True))

    def test_links_in_the_chain_tree_are_refused(self):
        path, plan = self.start(2)
        self.stop()
        new = self.build()
        req = self.request(FAKE, new, 8, 2)
        target = self.t / 'elsewhere.json'
        target.write_text('{}')
        link = self.node.root / 'chains' / 'c1' / chains.REPIN_PENDING
        try:
            link.symlink_to(target)
        except OSError:
            self.skipTest('no symlink permission on this host')
        code, err = self.repin(path, 'begin', req)
        self.assertEqual(code, w5box.EXIT_REFUSED)
        self.assertIn('is a link', err)
        self.assertEqual(target.read_text(), '{}')


if __name__ == '__main__':
    unittest.main()
