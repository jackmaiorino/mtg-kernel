"""P-grow (CG) rosters (tools/w5/pgrow.py through chains.py) with the stand-in trainer: plan refusals, roster and
draw determinism, uniformity of the hashed draw, a chain's first segments (ladder b512 only) and its first ladder
additions, resume identity after a crash, and fail-closed refusals of missing or corrupt ladder states."""
import json
import math
import os
import shutil
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE))
import chains  # noqa: E402
import pgrow  # noqa: E402
import retention  # noqa: E402
from test_w5_chains import Node, STEP_C, jwrite  # noqa: E402

SEG, STRIDE = 4, 8


def chi_square(counts, n_bins):
    total = sum(counts.values())
    expected = total / n_bins
    return sum((counts.get(k, 0) - expected) ** 2 / expected for k in range(n_bins))


def chi_square_bound(dof):
    """An upper bound the statistic exceeds with probability about 1e-4 (Wilson-Hilferty, z = 3.72)."""
    z = 3.72
    return dof * (1 - 2 / (9 * dof) + z * math.sqrt(2 / (9 * dof))) ** 3


class Roster(unittest.TestCase):
    def test_roster_and_draws_are_pure_functions(self):
        bounds = list(range(0, 41 * STRIDE, STRIDE))
        a = pgrow.roster('chains-20261107/c1', 40, bounds, 16)
        self.assertEqual(a, pgrow.roster('chains-20261107/c1', 40, list(bounds), 16))
        self.assertEqual(len(a), 16)
        self.assertEqual(a, sorted(a))
        self.assertTrue(set(a) <= set(bounds))
        self.assertNotEqual(a, pgrow.roster('chains-20261107/c1', 41, bounds, 16))  # per segment
        self.assertNotEqual(a, pgrow.roster('chains-20261107/c2', 40, bounds, 16))  # per seed root
        self.assertEqual(pgrow.roster('r', 3, bounds[:5], 16), bounds[:5])  # the whole ladder while it fits
        self.assertEqual([pgrow.draw('r', 1000, j, 7) for j in range(10)],
                         [pgrow.draw('r', 1000, j, 7) for j in range(10)])

    def test_hashed_draw_is_uniform(self):
        for n in (1, 2, 7, 33):
            counts = Counter(pgrow.draw('chains-20261107/c1', u, j, n) for u in range(4000) for j in range(10))
            self.assertEqual(set(counts), set(range(n)))
            if n > 1:
                self.assertLess(chi_square(counts, n), chi_square_bound(n - 1), (n, counts))

    def test_subset_keeps_the_marginal_uniform_over_the_ladder(self):
        """Each segment's roster is a uniform subset (every ladder state equally often a member), and the draw is
        uniform within it, so an episode's opponent is uniform over the ladder."""
        bounds = list(range(0, 20 * STRIDE, STRIDE))  # 20 ladder states, a roster of 5
        counts, sizes = Counter(), set()
        for s in range(4000):
            members = pgrow.roster('chains-20261107/c3', s, bounds, 5)
            sizes.add(len(set(members)))
            counts.update(bounds.index(b) for b in members)
        self.assertEqual(sizes, {5})
        self.assertLess(chi_square(counts, 20), chi_square_bound(19), counts)


def base_cg(t, node):
    """A base like A's: current, the fixed parent (d6107922) twice and a second named opponent per update."""
    def src(name):
        ck = Path(t) / f'{name}.ckpt'
        ck.write_text(f'{name} weights')
        return {'checkpoint': chains.pin(ck), 'play_import': {'path': '/imports/flat-v4.json', 'sha256': '1' * 64}}
    kinds = [{'kind': 'current'}, {'id': 'parent', 'kind': 'fixed'}, {'id': 'a48', 'kind': 'fixed'},
             {'id': 'parent', 'kind': 'fixed'}]
    return jwrite(Path(t) / 'base-cg.json', {'schema': 'test-base', 'opponents': [
        {'id': 'a48', 'source': src('a48')}, {'id': 'parent', 'source': src('d6107922')}], 'iterations': [
        {'episodes': [{'episode': {'learner_seat': s % 2}, 'opponent': k} for s, k in enumerate(kinds)]}]})


class Chains(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.node = Node(self.tmp.name)
        self.base = base_cg(self.tmp.name, self.node)
        env = mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_SLEEP': '0'})
        env.start()
        self.addCleanup(env.stop)
        for name, value in (('GUARD_WAIT', 0.0), ('GUARD_POLL', 0.0)):
            p = mock.patch.object(chains, name, value)
            p.start()
            self.addCleanup(p.stop)

    def plan(self, ladder=None, label='cg', chains_in=None, **arm_extra):
        arm = {'parent': str(self.node.b512), 'workers': 1, 'memory_max_bytes': 1 << 30, 'bytes_per_update': 1000,
               'segment_updates': SEG, 'base': str(self.base),
               'ladder': {'stride': STRIDE, 'test_stride': True} if ladder is None else ladder}
        arm.update(arm_extra)
        specs = chains_in or {'g1': {'arm': 'CG', 'slot': 0, 'seed_root': 'pg/c1'},
                              'g2': {'arm': 'CG', 'slot': 1, 'seed_root': 'pg/c1'}}
        inputs = json.loads(Path(self.node.inputs('0', specs, label=label, c_wall_seconds=3600.0,
                                                  independent=True)).read_text())
        inputs['arms']['CG'] = arm
        return chains.make_plan(jwrite(self.node.t / f'{label}.json', inputs))

    def run_segments(self, plan, name, n):
        """Drives `name` until it has run `n` more segments (the guard admits exactly that many)."""
        cdir = self.node.root / 'chains' / name
        self.node.guard(ceilings={name: chains.admitted(cdir) + n})
        return self.node.drive(plan, name)

    def records(self, plan, name):
        return retention.index(plan['chains'][name]['manifest'])

    def configs(self, name):
        return [json.loads(p.read_text()) for p in sorted((self.node.root / 'chains' / name / 'segments')
                                                          .glob('seg-*.json'))]

    def test_plan_refusals(self):
        for bad, why in (({'stride': 6, 'test_stride': True}, 'multiple of the segment length'),
                         ({'stride': STRIDE}, 'multiple of 4096'),
                         ({'stride': STRIDE, 'test_stride': True, 'replaces': 'nobody'}, 'no named opponent'),
                         ({'stride': STRIDE, 'test_stride': True, 'roster_max': 128}, 'roster_max must be 1 to 127'),
                         ({'stride': STRIDE, 'test_stride': True, 'roster_max': 0}, 'roster_max'),
                         ({'stride': STRIDE, 'test_stride': True, 'extra': 1}, 'unknown ladder keys')):
            with self.assertRaisesRegex(chains.Stop, why):
                self.plan(bad, label='bad')
        with self.assertRaisesRegex(chains.Stop, 'history-relative'):
            doc = json.loads(self.base.read_text())
            doc['iterations'][0]['episodes'][0]['opponent'] = {'kind': 'initial'}
            self.base = jwrite(self.node.t / 'base-hist.json', doc)
            self.plan(label='hist')

    def test_ladder_only_on_cg_and_production_defaults(self):
        _, plan = self.plan({}, label='prod', segment_updates=256)  # the defaults: stride 4096, roster 64
        arm = json.loads(Path(plan['arms']['CG']['file']['path']).read_text())
        self.assertEqual(arm['ladder'], {'stride': 4096, 'roster_max': 64, 'replaces': 'parent', 'test_stride': False})
        man = json.loads(Path(plan['chains']['g1']['manifest']).read_text())
        self.assertEqual(man['recipe']['ladder'], arm['ladder'])
        inputs = json.loads((self.node.t / 'prod.json').read_text())
        inputs['arms']['C'] = dict(inputs['arms']['C'], ladder={})
        inputs['chains'] = {'c1': {'arm': 'C', 'slot': 0, 'seed_root': 'oc/c1'}}
        with self.assertRaisesRegex(chains.Stop, 'only a CG arm'):
            chains.make_plan(jwrite(self.node.t / 'c-ladder.json', dict(inputs, label='cl')))

    def test_first_segments_play_b512_then_the_ladder_grows(self):
        _, plan = self.plan()
        self.assertEqual(self.run_segments(plan, 'g1', 5), chains.EXIT_ROOM)
        cfgs = self.configs('g1')
        self.assertEqual(len(cfgs), 5)
        b512 = json.loads(self.node.b512.read_text())['source']
        chain, records = self.records(plan, 'g1')
        after = {r['boundary']: r for r in records}
        ladder = json.loads((self.node.root / 'chains' / 'g1' / 'LADDER.json').read_text())
        for s, cfg in enumerate(cfgs):
            expect = list(range(0, s * SEG + 1, STRIDE))  # s 0, 1: b512 only; 2, 3: + u8; 4: + u16
            named = {o['id']: o['source'] for o in cfg['opponents']}
            self.assertNotIn('parent', named)
            self.assertIn('a48', named)
            played = [e['opponent'] for it in cfg['iterations'] for e in it['episodes']]
            self.assertEqual(Counter(o.get('id') for o in played)['a48'], SEG)
            ladder_ids = {o['id'] for o in played if o.get('id', '').startswith('ladder-u')}
            self.assertEqual(sum(1 for o in played if o.get('id', '').startswith('ladder-u')), 2 * SEG)
            self.assertTrue(ladder_ids <= {f'ladder-u{b}' for b in expect}, (s, ladder_ids))
            self.assertEqual(set(named) - {'a48'}, ladder_ids)  # only drawn members are loaded
            for b in expect:
                pin = b512['checkpoint'] if b == 0 else None
                if b:
                    rec = after[b]
                    pin = {'path': rec['checkpoint']['path'], 'sha256': rec['checkpoint']['sha256']}
                self.assertEqual(ladder['states'][f'ladder-u{b}']['sha256'], pin['sha256'])
                self.assertEqual(ladder['states'][f'ladder-u{b}']['after_state_sha256'],
                                 after[b]['after_state_sha256'] if b else None)
                if f'ladder-u{b}' in named:
                    self.assertEqual(named[f'ladder-u{b}']['checkpoint'], pin)
        self.assertEqual({o['id'] for it in cfgs[0]['iterations'] for e in it['episodes']
                          for o in [e['opponent']] if 'id' in o}, {'a48', 'ladder-u0'})
        self.assertEqual(sorted(ladder['states']), ['ladder-u0', 'ladder-u16', 'ladder-u8'])
        # Every stored config is the one derived again from the pins and the ladder (a resume's check).
        arm = chains.load_arm(plan['arms']['CG']['file']['path'])
        man = json.loads(Path(plan['chains']['g1']['manifest']).read_text())
        for s, cfg in enumerate(cfgs):
            seg = man['segments'][s]
            source = chains.next_source(man['segments'][s - 1]['root']) if s else None
            again = chains.arm_config(arm, 'pg/c1', s * SEG, SEG, Path(seg['root']), source, None,
                                      chains.chain_ladder(arm, man, self.node.root / 'chains' / 'g1', s * SEG))
            again['chain_ledger'] = cfg['chain_ledger']
            self.assertEqual(again, cfg)

    def test_resume_after_a_crash_reproduces_rosters_and_states(self):
        _, plan = self.plan()
        self.run_segments(plan, 'g1', 6)
        with mock.patch.dict(os.environ, {'FAKE_FAIL_AT': '10'}):  # mid segment 2, after the first addition
            self.assertEqual(self.run_segments(plan, 'g2', 6), chains.EXIT_FAILED)
        record = self.node.root / 'chains' / 'g2' / 'LADDER.json'
        kept = record.read_bytes()
        record.unlink()  # a lost record is refused, never pinned again from the current files
        self.assertEqual(self.run_segments(plan, 'g2', 1), chains.w5box.EXIT_REFUSED)
        record.write_bytes(kept)
        self.assertEqual(self.run_segments(plan, 'g2', 4), chains.EXIT_ROOM)
        _, r1 = self.records(plan, 'g1')
        _, r2 = self.records(plan, 'g2')
        self.assertEqual(len(r1), 6 * SEG)
        self.assertEqual([r['after_state_sha256'] for r in r1], [r['after_state_sha256'] for r in r2])
        strip = lambda cfg: [[e['opponent'] for e in it['episodes']] for it in cfg['iterations']]  # noqa: E731
        self.assertEqual([strip(c) for c in self.configs('g1')], [strip(c) for c in self.configs('g2')])
        hashes = lambda name: {k: v['sha256'] for k, v in json.loads(  # noqa: E731
            (self.node.root / 'chains' / name / 'LADDER.json').read_text())['states'].items()}
        self.assertEqual(hashes('g1'), hashes('g2'))
        self.assertTrue(any((self.node.root / 'chains' / 'g2' / 'interrupted').iterdir()))

    def test_the_roster_changes_the_chain(self):
        """The ladder opponents' hashes reach the states: a chain playing d6107922 (CP-like, the base unchanged)
        leaves the CG chain's states at its first update."""
        _, plan = self.plan()
        self.run_segments(plan, 'g1', 1)
        arm = chains.load_arm(plan['arms']['CG']['file']['path'])
        plain = dict(arm, ladder=None)
        cfg = chains.arm_config(plain, 'pg/c1', 0, 1, self.node.t / 'plain')
        self.assertEqual(chains.run_trainer(plain, jwrite(self.node.t / 'plain.json', cfg))[0], 0)
        _, records = self.records(plan, 'g1')
        done = json.loads((self.node.t / 'plain' / 'iterations' / '000000' / 'complete.json').read_text())
        plain_state = json.loads(Path(done['update']['path']).read_text())['after_state_sha256']
        self.assertNotEqual(plain_state, records[0]['after_state_sha256'])

    def test_missing_or_corrupt_ladder_states_refuse(self):
        _, plan = self.plan()
        self.run_segments(plan, 'g1', 3)  # boundaries to 12; the ladder holds u0 and u8
        arm = chains.load_arm(plan['arms']['CG']['file']['path'])
        cdir = self.node.root / 'chains' / 'g1'
        man = json.loads((cdir / 'chain.json').read_text())
        _, records = self.records(plan, 'g1')
        ck = Path(next(r for r in records if r['boundary'] == 8)['checkpoint']['path'])
        good = ck.read_bytes()
        chains.chain_ladder(arm, man, cdir, 12)  # intact
        ck.write_bytes(good + b' ')
        with self.assertRaisesRegex(chains.Stop, 'ladder state 8'):
            chains.chain_ladder(arm, man, cdir, 12)
        self.assertEqual(self.run_segments(plan, 'g1', 1), chains.w5box.EXIT_REFUSED)  # the driver stops
        ck.unlink()
        with self.assertRaisesRegex(chains.Stop, 'ladder state 8'):
            chains.chain_ladder(arm, man, cdir, 12)
        # A rewritten checkpoint whose receipt was rewritten to match: LADDER.json keeps the first hash.
        ck.write_bytes(b'other weights')
        complete = Path(man['segments'][1]['root']) / 'iterations' / f'{SEG - 1:06d}' / 'complete.json'
        doc = json.loads(complete.read_text())
        doc['output_identity']['checkpoint_sha256'] = chains.sha(ck)
        complete.write_text(json.dumps(doc))
        with self.assertRaisesRegex(chains.Stop, 'recorded'):
            chains.chain_ladder(arm, man, cdir, 12)
        ck.write_bytes(good)
        doc['output_identity']['checkpoint_sha256'] = chains.sha(ck)
        complete.write_text(json.dumps(doc))
        chains.chain_ladder(arm, man, cdir, 12)
        # A ladder state past the chain's segments, an incomplete or a missing ladder segment.
        with self.assertRaisesRegex(chains.Stop, 'no segment ends at ladder boundary 16'):
            chains.chain_ladder(arm, man, cdir, 16)
        (Path(man['segments'][1]['root']) / 'completion.json').unlink()
        with self.assertRaisesRegex(chains.Stop, 'not complete'):
            chains.chain_ladder(arm, man, cdir, 12)
        shutil.rmtree(man['segments'][1]['root'])
        with self.assertRaisesRegex(chains.Stop, 'not complete'):
            chains.chain_ladder(arm, man, cdir, 12)
        # Another chain's or stride's ladder record is refused.
        (cdir / 'LADDER.json').write_text(json.dumps({'schema': pgrow.LADDER_SCHEMA, 'chain': 'g9', 'stride': STRIDE,
                                                      'states': {}}))
        with self.assertRaisesRegex(chains.Stop, 'ladder record'):
            chains.chain_ladder(arm, man, cdir, 0)

    def test_a_segment_start_verifies_the_roster_and_new_states_only(self):
        _, plan = self.plan({'stride': SEG, 'roster_max': 2, 'test_stride': True}, label='small')
        self.assertEqual(self.run_segments(plan, 'g1', 6), chains.EXIT_ROOM)  # ladder 0, 4, ..., 24 at segment 6
        arm = chains.load_arm(plan['arms']['CG']['file']['path'])
        cdir = self.node.root / 'chains' / 'g1'
        man = json.loads((cdir / 'chain.json').read_text())
        for s, cfg in enumerate(self.configs('g1')):
            self.assertLessEqual(len({o['id'] for o in cfg['opponents'] if o['id'].startswith('ladder-u')}), 2)
        bounds = list(range(0, 6 * SEG + 1, SEG))
        members = [int(i.removeprefix('ladder-u')) for i in
                   pgrow.roster('pg/c1', 6, [pgrow.member_id(b) for b in bounds], 2)]
        _, records = self.records(plan, 'g1')
        ck = {r['boundary']: Path(r['checkpoint']['path']) for r in records if r['boundary'] in bounds}
        other = next(b for b in bounds if b and b not in members and b != 6 * SEG)
        ck[other].write_bytes(b'corrupt')  # recorded, not a member: not read
        states = chains.chain_ladder(arm, man, cdir, 6 * SEG)
        self.assertEqual([i for i, _ in states], [pgrow.member_id(b) for b in members])
        ladder = json.loads((cdir / 'LADDER.json').read_text())
        self.assertEqual(sorted(ladder['states']), sorted(pgrow.member_id(b) for b in bounds))  # u24 recorded when new
        member = next(b for b in members if b)
        ck[member].write_bytes(b'corrupt')
        with self.assertRaisesRegex(chains.Stop, f'ladder state {member}'):
            chains.chain_ladder(arm, man, cdir, 6 * SEG)

    def test_large_ladders_cap_the_roster(self):
        states = [(pgrow.member_id(b), {'checkpoint': {'path': f'/x/{b}', 'sha256': f'{b:064x}'}})
                  for b in range(0, 200 * 8, 8)]
        def cfg(others):
            return {'opponents': [{'id': o} for o in others] + [{'id': 'parent'}], 'iterations': [
                {'episodes': [{'opponent': {'id': 'parent', 'kind': 'fixed'}} for _ in range(10)]}
                for _ in range(256)]}
        spec = {'stride': 8, 'roster_max': 127, 'replaces': 'parent', 'test_stride': True}
        one = cfg(['a48'])
        rec = pgrow.apply(one, spec, states, 'pg/c1', 199, 199 * 256)
        self.assertEqual(len(rec['roster']), 127)
        self.assertEqual(len(one['opponents']), 128)  # every member drawn at least once in 2,560 draws
        self.assertEqual(sum(rec['draws'].values()), 2560)
        with self.assertRaisesRegex(pgrow.LadderError, 'trainer'):  # the plan's roster_max check, enforced again
            pgrow.apply(cfg(['a48', 'b']), spec, states, 'pg/c1', 199, 199 * 256)

    def test_segcheck_config_and_paircheck_default_to_the_parent(self):
        _, plan = self.plan()
        arm = chains.load_arm(plan['arms']['CG']['file']['path'])
        a = chains.arm_config(arm, 'w5/segcheck', 0, 2, self.node.t / 'w')
        b = chains.arm_config(arm, 'w5/segcheck', 1, 1, self.node.t / 'b')
        self.assertEqual(a['iterations'][1]['episodes'], b['iterations'][0]['episodes'])
        self.assertEqual({o['id'] for o in a['opponents']}, {'a48', 'ladder-u0'})

    def test_a_fork_carries_its_source_chains_ladder(self):
        """A CG chain forked at boundary 16 of g1 (`pgrow.py history`) plays g1's ladder before 16 (u0 = b512, u8)
        as ladder-h states, then its own (its u0 is g1's state at 16)."""
        _, plan = self.plan()
        self.assertEqual(self.run_segments(plan, 'g1', 5), chains.EXIT_ROOM)
        hist = self.node.t / 'fork' / 'history.json'
        doc, parent = pgrow.write_history(plan['chains']['g1']['manifest'], 16, STRIDE, hist,
                                          copy_to=self.node.t / 'fork' / 'objects')
        self.assertEqual([st['boundary'] for st in doc['states']], [0, 8])
        b512 = json.loads(self.node.b512.read_text())['source']['checkpoint']
        self.assertEqual(doc['states'][0]['checkpoint'], b512)
        _, records = self.records(plan, 'g1')
        after = {r['boundary']: r['after_state_sha256'] for r in records}
        self.assertEqual(doc['states'][1]['after_state_sha256'], after[8])
        self.assertTrue(doc['states'][1]['checkpoint']['path'].startswith((self.node.t / 'fork' / 'objects')
                                                                          .as_posix()))
        fork = json.loads(parent.read_text())
        self.assertEqual(fork['origin'], {'chain': 'g1', 'boundary': 16})
        _, fplan = self.plan({'stride': STRIDE, 'test_stride': True, 'history': str(hist)}, label='fork',
                             chains_in={'f1': {'arm': 'CG', 'slot': 2, 'seed_root': 'pg/f1'}}, parent=str(parent))
        arm = json.loads(Path(fplan['arms']['CG']['file']['path']).read_text())
        self.assertEqual(arm['ladder']['history'], chains.pin(hist))
        self.assertEqual(self.run_segments(fplan, 'f1', 3), chains.EXIT_ROOM)
        cfgs = self.configs('f1')
        ids = [{o['id'] for o in c['opponents'] if o['id'].startswith('ladder-')} for c in cfgs]
        self.assertTrue(ids[0] <= {'ladder-h0', 'ladder-h8', 'ladder-u0'} and 'ladder-h8' in set().union(*ids))
        self.assertTrue(ids[2] <= {'ladder-h0', 'ladder-h8', 'ladder-u0', 'ladder-u8'})
        named = {o['id']: o['source'] for c in cfgs for o in c['opponents']}
        self.assertEqual(named['ladder-u0']['checkpoint'], fork['source']['checkpoint'])
        ladder = json.loads((self.node.root / 'chains' / 'f1' / 'LADDER.json').read_text())
        self.assertEqual(sorted(ladder['states']), ['ladder-h0', 'ladder-h8', 'ladder-u0', 'ladder-u8'])
        # A changed history file refuses at load (its arm pin), and a corrupt history checkpoint refuses at use.
        cp = Path(doc['states'][1]['checkpoint']['path'])
        good = cp.read_bytes()
        cp.write_bytes(b'corrupt')
        man = json.loads(Path(fplan['chains']['f1']['manifest']).read_text())
        with self.assertRaisesRegex(chains.Stop, 'ladder-h8'):
            chains.chain_ladder(chains.load_arm(fplan['arms']['CG']['file']['path']), man,
                                self.node.root / 'chains' / 'f1', 12)
        cp.write_bytes(good)
        hist.write_text(hist.read_text() + ' ')
        with self.assertRaisesRegex(chains.Stop, 'ladder history'):
            chains.load_arm(fplan['arms']['CG']['file']['path'])
        # The plan refuses a history holding the fork state itself, or off-stride boundaries.
        bad = dict(doc, states=doc['states'] + [{'boundary': 16, 'checkpoint': fork['source']['checkpoint'],
                                                  'after_state_sha256': after[16]}])
        with self.assertRaisesRegex(chains.Stop, 'fork state itself'):
            self.plan({'stride': STRIDE, 'test_stride': True, 'history': str(jwrite(self.node.t / 'h1.json', bad))},
                      label='forkbad1', chains_in={'f2': {'arm': 'CG', 'slot': 3, 'seed_root': 'pg/f2'}},
                      parent=str(parent))
        bad = dict(doc, states=[dict(doc['states'][1], boundary=5)])
        with self.assertRaisesRegex(chains.Stop, 'multiples of the stride'):
            self.plan({'stride': STRIDE, 'test_stride': True, 'history': str(jwrite(self.node.t / 'h2.json', bad))},
                      label='forkbad2', chains_in={'f3': {'arm': 'CG', 'slot': 3, 'seed_root': 'pg/f3'}},
                      parent=str(parent))


if __name__ == '__main__':
    unittest.main()
