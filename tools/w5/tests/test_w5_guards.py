"""W5 guards for ordinary chains (the `guards: true` chain input): the carried chains' link and containment checks
switched on for an ordinary chain, once and for good, so it can be re-pinned (tools/w5/chains.py `repin`); with the
guards off nothing changes. Stand-in trainer (fake_trainer.py), as test_w5_chains.py."""
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


class GuardsTests(unittest.TestCase):
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

    def build(self, name='trainer-b.py'):
        path = self.t / name
        if not path.exists():
            path.write_text(FAKE.read_text() + f'\n# another build: {name}\n')
        return path

    def inputs(self, trainer=None, guards=False, repin=None, throughput=True):
        spec = {'arm': 'C', 'slot': 0, 'seed_root': SEED}
        if guards:
            spec['guards'] = True
        if repin:
            spec['repin'] = str(repin)
        extra = {'trainer': str(trainer)} if trainer else {}
        doc = json.loads(Path(self.node.inputs('0', {'c1': spec}, label='main', c_wall_seconds=600.0,
                                               independent=True, **extra)).read_text())
        if throughput:
            doc['arms']['C']['throughput'] = str(self.throughput)
        return jwrite(self.t / 'inputs.json', doc)

    def cdir(self):
        return self.node.root / 'chains' / 'c1'

    def man(self):
        return json.loads((self.cdir() / 'chain.json').read_text())

    def drive(self, plan, segments):
        self.node.guard(ceilings={'c1': segments})
        with contextlib.redirect_stderr(io.StringIO()):
            return self.node.drive(plan, 'c1')

    def whole(self, plan, updates):
        arm = chains.load_arm(plan['arms']['C']['file']['path'])
        cfg = chains.arm_config(arm, SEED, 0, updates, self.t / 'whole')
        self.assertEqual(chains.run_trainer(arm, jwrite(self.t / 'whole.json', cfg))[0], 0)
        return after_states(self.t / 'whole')

    def request(self, plan, old, new, rid='g1', guards=True):
        man = self.man()
        segs = man['segments']
        arm = json.loads(Path(plan['arms']['C']['file']['path']).read_text())
        config_sha, source = chains.repin_next_config_sha256(arm, man)
        mp = (self.cdir() / 'chain.json').resolve()
        boundary = segs[-1]['offset'] + segs[-1]['updates']
        proof = jwrite(self.t / f'proof-{rid}.json', {
            'schema': chains.REPIN_PROOF_SCHEMA, 'chain': 'c1', 'manifest': {'path': mp.as_posix(), 'sha256': sha(mp)},
            'boundary': boundary, 'last_segment': len(segs) - 1, 'next_segment': len(segs),
            'old_sha256': sha(old), 'new_sha256': sha(new),
            'gate_a': {'segment': len(segs) - 1, 'updates': 4, 'equal': 4, 'config': chains.pin(segs[-1]['config'])},
            'gate_c': {'segment': len(segs), 'updates': 4, 'equal': 4, 'config_sha256': config_sha,
                       'source_checkpoint': source['checkpoint']}, 'pass': True})
        doc = {'schema': chains.REPIN_SCHEMA, 'chain': 'c1', 'id': rid, 'from_sha256': sha(old),
               'to_sha256': sha(new), 'boundary': boundary, 'segment': len(segs), 'proof': chains.pin(proof)}
        if guards:
            doc['guards'] = True
        return jwrite(self.t / f'req-{rid}.json', doc)

    def repin(self, plan_path, step, request=None):
        args = ['repin', step, '--plan', str(plan_path), '--chain', 'c1'] + \
            (['--request', str(request)] if request else [])
        err = io.StringIO()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(err):
            return chains.main(args), err.getvalue()

    # ------------------------------------------------------------ tests

    def test_an_ordinary_chain_repinned_with_its_guards_reaches_the_states_of_an_uninterrupted_run(self):
        path, plan = chains.make_plan(self.inputs())
        self.assertNotIn('guards', plan['chains']['c1'])
        self.assertEqual(self.drive(plan, 2), chains.EXIT_ROOM)
        ref = self.whole(plan, 20)
        (self.cdir() / 'STOP').write_text('repin\n')
        new = self.build()
        # without `guards` the ordinary chain is refused
        code, err = self.repin(path, 'begin', self.request(plan, FAKE, new, rid='nog', guards=False))
        self.assertEqual(code, w5box.EXIT_REFUSED)
        self.assertIn('guards', err)
        req = self.request(plan, FAKE, new)
        self.assertEqual(self.repin(path, 'begin', req)[0], 0)
        with self.assertRaisesRegex(chains.Stop, 'names `guards: true`'):
            chains.make_plan(self.inputs(trainer=new, repin=req, throughput=False))
        chains.make_plan(self.inputs(trainer=new, repin=req, guards=True, throughput=False))
        path, plan = chains.make_plan(self.inputs(trainer=new, repin=req, guards=True))
        self.assertEqual(self.repin(path, 'end')[0], 0)
        man = self.man()
        self.assertIs(man['guards'], True)
        self.assertEqual(man['recipe']['trainer_sha256'], sha(new))
        self.assertTrue(plan['chains']['c1']['guards'])
        (self.cdir() / 'STOP').unlink()
        self.assertEqual(self.drive(plan, 5), chains.EXIT_ROOM)
        _, records = retention.index(self.cdir() / 'chain.json')
        self.assertEqual([r['after_state_sha256'] for r in records], ref)
        self.assertEqual([chains.trainer_of_segment(man, r['segment']) for r in records],
                         [sha(FAKE)] * 8 + [sha(new)] * 12)
        # once on, never off: a plan without the input keeps the manifest's guards
        chains.make_plan(self.inputs(trainer=new))
        self.assertIs(self.man()['guards'], True)

    def test_a_guarded_ordinary_chain_refuses_links(self):
        path, plan = chains.make_plan(self.inputs(guards=True))
        self.assertEqual(self.drive(plan, 1), chains.EXIT_ROOM)
        self.assertIs(self.man()['guards'], True)
        target = self.t / 'elsewhere.json'
        target.write_text('{}')
        link = self.node.root / 'LOG.jsonl'
        link.unlink(missing_ok=True)
        try:
            link.symlink_to(target)
        except OSError:
            self.skipTest('no symlink permission on this host')
        self.assertEqual(self.drive(plan, 3), w5box.EXIT_REFUSED)
        with self.assertRaises(chains.Stop):
            chains.cycle(plan, path)
        self.assertEqual(target.read_text(), '{}')
        link.unlink()
        # a linked index: retention refuses it for a guarded chain
        index = self.cdir() / 'chain.index.jsonl'
        index.unlink(missing_ok=True)
        other = self.t / 'other.index.jsonl'
        other.write_text('')
        index.symlink_to(other)
        with self.assertRaises(SystemExit):
            retention.index(self.cdir() / 'chain.json')

    def test_without_guards_an_ordinary_chain_is_unchanged(self):
        path, plan = chains.make_plan(self.inputs())
        self.assertEqual(self.drive(plan, 2), chains.EXIT_ROOM)
        self.assertNotIn('guards', self.man())
        target = self.t / 'elsewhere.json'
        target.write_text('x')
        link = self.cdir() / 'trainer.out'
        link.unlink(missing_ok=True)
        try:
            link.symlink_to(target)
        except OSError:
            self.skipTest('no symlink permission on this host')
        # an ordinary chain behaves as before: no tree walk (the driver runs on)
        link.unlink()
        self.assertEqual(self.drive(plan, 3), chains.EXIT_ROOM)


if __name__ == '__main__':
    unittest.main()
