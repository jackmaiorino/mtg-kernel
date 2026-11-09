"""Mixed-occupancy runs and baseline/candidate comparisons (tools/w5/chains.py `throughput-mix`,
`throughput-compare`, mtg-kernel#600 milestone 4) on any OS with the stand-in trainer: serial phases per arm and
mixed phases on the named slots, identity parity within each arm (never across arms), per-arm and total durable
numbers, a record no guard reads, and comparisons that hold the workload fixed, let only the code differ, check
every chain's outputs and call a speed difference only when the ranges separate."""
import contextlib
import copy
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
import w5box  # noqa: E402
from test_w5_chains import FAKE, Node, STEP_C, STEP_X, dose, jwrite  # noqa: E402
from test_w5_throughput import stand_in_spawn  # noqa: E402

SLOTS = ['C', 'CP', 'C', 'CP']


class MixedBase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.node = Node(self.tmp.name)
        self.other = dose(self.node.t, 'p512')  # CP's own parent, so its outputs differ from C's
        env = mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X),
                                           'FAKE_SLEEP': '0.02'})
        env.start()
        self.addCleanup(env.stop)
        # As test_w5_throughput: no w5box supervisors, POSIX-only handoff closing, and a wide guard ceiling.
        for name, value in (('recover_unsupervised', lambda plan, root: []), ('close_handoffs', lambda plan, root: []),
                            ('GUARD_SEGMENTS', 10**6)):
            patch = mock.patch.object(chains, name, value)
            patch.start()
            self.addCleanup(patch.stop)

    def tearDown(self):
        self.tmp.cleanup()

    def target(self, label, trainer=FAKE, workers=1):
        """An independent Stage 0 plan with C and CP chains (so each arm has its w5box launch plan)."""
        specs = {f'{label}-c': {'arm': 'C', 'slot': 0, 'seed_root': f'oc/{label}-c'},
                 f'{label}-p': {'arm': 'CP', 'slot': 1, 'seed_root': f'oc/{label}-p'}}
        inputs = json.loads(Path(self.node.inputs('0', specs, label=label, c_wall_seconds=600,
                                                  independent=True)).read_text())
        inputs['trainer'] = str(trainer)
        inputs['arms']['C']['workers'] = workers
        inputs['arms']['CP'] = dict(inputs['arms']['C'], parent=str(self.other))
        path, _ = chains.make_plan(jwrite(self.node.t / f'{label}-inputs.json', inputs))
        return path

    def candidate_trainer(self):
        """The stand-in trainer with other bytes and the same behavior: a code-only candidate."""
        path = self.node.t / 'candidate_trainer.py'
        path.write_text(FAKE.read_text() + '\n# a candidate build: other bytes, the same states\n')
        return path

    def mix(self, plan, label, counts=None):
        with contextlib.redirect_stdout(io.StringIO()):
            return chains.mix_run(plan, SLOTS, label, 8, self.node.t / f'{label}.json', counts,
                                  spawn=stand_in_spawn(self.node), launcher=lambda p: None, poll=0.02)

    def homogeneous(self, plan, label):
        with contextlib.redirect_stdout(io.StringIO()):
            return chains.throughput_run(plan, 'C', label, 8, self.node.t / f'{label}.json',
                                         spawn=stand_in_spawn(self.node), launcher=lambda p: None, poll=0.02)

    def copy_record(self, record, name, change):
        doc = copy.deepcopy(record)
        change(doc)
        return jwrite(self.node.t / name, doc)


class MixedRunTests(MixedBase):
    def test_serial_and_mixed_phases_measure_each_arm(self):
        path = self.target('mixplan')
        passed, record = self.mix(path, 'mx', counts=[2, 4])
        self.assertTrue(passed, record.get('why'))
        self.assertEqual(record['schema'], chains.MIX_SCHEMA)
        self.assertTrue((self.node.t / 'mx.json').exists())
        self.assertEqual([chains.phase_key(record, p) for p in record['phases']],
                         ['serial-C', 'serial-CP', 'mix-2', 'mix-4'])
        self.assertEqual([p['concurrent_boxes'] for p in record['phases']], [1, 1, 2, 4])
        self.assertEqual(sorted(record['bindings']), ['C', 'CP'])
        for p in record['phases']:  # placement: a serial phase's arm on slot 0, else slot k runs SLOTS[k]
            self.assertEqual([b['slot'] for b in p['boxes']], list(range(p['concurrent_boxes'])))
            for b in p['boxes']:
                self.assertEqual(p['arms'][b['chain']], p['mix_phase'].get('arm') or SLOTS[b['slot']])
        # Parity within each arm; the arms' own outputs differ and are never compared.
        ident = {p['mix_phase']['arm']: p['boxes'][0]['identity'] for p in record['phases'][:2]}
        self.assertNotEqual(ident['C'], ident['CP'])
        self.assertEqual(len(ident['C']), 8)
        for p in record['phases']:
            for b in p['boxes']:
                self.assertEqual(b['identity'], ident[p['arms'][b['chain']]])
        # Durable numbers: every phase complete, the full layout's per-arm and total counts are N per chain.
        for p in record['phases']:
            self.assertTrue(p['durable']['complete'], p['durable']['failures'])
        full = record['phases'][-1]['durable']
        self.assertEqual(full['updates'], 8 * 4)
        self.assertEqual({a: v['updates'] for a, v in full['per_arm'].items()}, {'C': 16, 'CP': 16})
        s = record['summary']
        self.assertEqual((s['boxes'], s['durable_updates_per_hour']), (4, full['updates_per_hour']))
        self.assertAlmostEqual(s['cpu_seconds_per_update'], STEP_C / 1e6)
        for a in ('C', 'CP'):
            arm = s['per_arm'][a]
            self.assertEqual(arm['boxes'], 2)
            self.assertEqual(arm['durable_updates_per_hour'], full['per_arm'][a]['updates_per_hour'])
            self.assertAlmostEqual(arm['mean_box_vs_serial'], sum(arm['box_updates_per_wall_second']) / 2
                                   / arm['serial_updates_per_wall_second'])
            self.assertAlmostEqual(arm['cpu_seconds_per_update'], STEP_C / 1e6)
            self.assertLessEqual(arm['durable_updates_per_hour'], arm['box_rate_sum_updates_per_hour'])
        self.assertEqual([r['mix_phase'] for r in s['phases']], [p['mix_phase'] for p in record['phases']])
        with self.assertRaisesRegex(w5box.Refusal, 'schema'):  # a companion record: no guard reads it
            w5box.require_throughput(record, record['bindings']['C'])

    def test_a_box_that_differs_within_its_arm_fails_the_record(self):
        c = {'b000000001': 'c1', 'b000000002': 'c2'}
        p = {'b000000001': 'p1', 'b000000002': 'p2'}

        def phase(label, key, boxes):
            return {'label': label, 'mix_phase': key, 'durable': {'complete': True},
                    'arms': {f'{label}-s{k}': arm for k, (arm, _) in enumerate(boxes)},
                    'boxes': [{'chain': f'{label}-s{k}', 'slot': k, 'identity': rows}
                              for k, (_, rows) in enumerate(boxes)]}
        phases = [phase('x-c-solo', {'kind': 'serial', 'arm': 'C'}, [('C', c)]),
                  phase('x-cp-solo', {'kind': 'serial', 'arm': 'CP'}, [('CP', p)]),
                  phase('x-mix2', {'kind': 'mix', 'count': 2}, [('C', c), ('CP', dict(p, b000000002='zz'))])]
        self.assertEqual(chains.mix_identity_differences(phases),
                         [{'chain': 'x-mix2-s1', 'arm': 'CP', 'first': 'b000000002', 'count': 1}])
        with self.assertRaisesRegex(chains.Stop, 'within an arm'):
            chains.mix_record({}, phases, {'slots': ['C', 'CP']})
        phases[2]['boxes'][1]['identity'] = p  # C and CP differ at every boundary: no difference within an arm
        self.assertEqual(chains.mix_identity_differences(phases), [])
        phases[2]['durable'] = {'complete': False, 'failures': [{'chain': 'x-mix2-s1', 'reason': 'short'}]}
        with self.assertRaisesRegex(chains.Stop, 'incomplete'):
            chains.mix_record({}, phases, {'slots': ['C', 'CP']})

    def test_refusals_before_any_chain(self):
        path = self.target('refplan')
        cases = ((['C', 'C', 'C', 'C'], 8, None, 'rf1', 'at least two arms'),
                 (['C', 'CP', 'C'], 8, None, 'rf2', 'names 3 boxes'),
                 (SLOTS, 6, None, 'rf3', 'whole C segments'),
                 (SLOTS, 4, None, 'rf4', 'at least two'),
                 (SLOTS, 8, [1, 4], 'rf5', 'rise from 2'),
                 (SLOTS, 8, [2, 3], 'rf6', 'rise from 2'),
                 (['C', 'CS', 'C', 'CS'], 8, None, 'rf7', 'no arm CS'),
                 (SLOTS, 8, None, 'x' * 30, 'lowercase'))
        for slots, n, counts, label, why in cases:
            with self.subTest(why=why, label=label[:4]):
                with self.assertRaisesRegex(chains.Stop, why):
                    chains.mix_run(path, slots, label, n, self.node.t / f'{label}.json', counts,
                                   spawn=lambda *a: self.fail('no chain starts'), launcher=lambda p: None)
                self.assertFalse((self.node.t / f'{label}.json').exists())
                self.assertFalse((self.node.t / f'{label}.json.failed.json').exists())


class CompareTests(MixedBase):
    def test_a_code_only_candidate_is_compared_chain_by_chain(self):
        base_plan, cand_plan = self.target('base'), self.target('cand', trainer=self.candidate_trainer())
        ok_b, base = self.homogeneous(base_plan, 'tb')
        ok_c, cand = self.homogeneous(cand_plan, 'tc')
        self.assertTrue(ok_b and ok_c, (base.get('why'), cand.get('why')))
        b, c = self.node.t / 'tb.json', self.node.t / 'tc.json'
        passed, report = chains.compare_records([b], [c])
        self.assertTrue(passed, report.get('why'))
        self.assertEqual(report['schema'], chains.COMPARE_SCHEMA)
        self.assertEqual((report['order'], report['code']['differs']), ('AB', ['trainer']))
        self.assertEqual(report['parity'], {'pass': True, 'differences': []})
        self.assertEqual(sorted(report['metrics']), ['C.cpu_seconds_per_update', 'C.durable_updates_per_hour',
                                                     'cpu_seconds_per_update', 'durable_updates_per_hour'])
        rate = report['metrics']['durable_updates_per_hour']
        self.assertEqual(rate['baseline'], [base['phases'][-1]['durable']['updates_per_hour']])
        self.assertEqual(rate['candidate'], [cand['phases'][-1]['durable']['updates_per_hour']])
        self.assertEqual(rate['verdict'], 'too few samples')
        self.assertAlmostEqual(report['metrics']['cpu_seconds_per_update']['candidate_over_baseline'], 1.0)
        # Balanced order needs two actual runs per side, each with fresh phase plans and chains.
        ok_c2, cand2 = self.homogeneous(cand_plan, 'tc2')
        ok_b2, base2 = self.homogeneous(base_plan, 'tb2')
        self.assertTrue(ok_c2 and ok_b2, (base2.get('why'), cand2.get('why')))
        b2, c2 = self.node.t / 'tb2.json', self.node.t / 'tc2.json'
        passed, report = chains.compare_records([b, b2], [c, c2])
        self.assertTrue(passed)
        self.assertEqual(report['order'], 'ABBA')
        rate = report['metrics']['durable_updates_per_hour']
        self.assertEqual(rate['verdict'], chains.spread_verdict(rate['baseline'], rate['candidate']))
        # Repeated paths, byte copies, and changed timestamps still represent the same measured phase plans.
        copied = self.copy_record(base, 'copy.json', lambda d: None)
        retimed = self.copy_record(base, 'retimed.json', lambda d: d['qualification'].update(
            started_unix=base2['qualification']['started_unix'] + 1))
        for duplicate in (b, copied, retimed):
            with self.subTest(duplicate=duplicate):
                with self.assertRaisesRegex(chains.Stop, 'independent samples require distinct runs'):
                    chains.compare_records([b, duplicate], [c, c2])
        for field in ('hostname', 'kernel'):
            changed_host = self.copy_record(cand, f'changed-{field}.json', lambda d: d['qualification'].update(
                {field: 'another-environment'}))
            with self.assertRaisesRegex(chains.Stop, f'environment\\.{field}'):
                chains.compare_records([b], [changed_host])
        # A candidate whose outputs differ fails, with no speed metrics.
        bad = self.copy_record(cand, 'bad.json', lambda d: d['phases'][-1]['boxes'][2]['identity'].update(
            b000000005='0' * 64))
        passed, report = chains.compare_records([b], [bad])
        self.assertFalse(passed)
        self.assertIsNone(report['metrics'])
        self.assertEqual(report['parity']['differences'],
                         [{'record': str(bad), 'phase': 'p4', 'slot': 2, 'first': 'b000000005', 'count': 1}])
        # The workload is held fixed: another N, another worker count, mixed code on a side, a failed record.
        longer = self.copy_record(cand, 'longer.json', lambda d: d['qualification'].update(updates=12))
        with self.assertRaisesRegex(chains.Stop, r'run\.updates'):
            chains.compare_records([b], [longer])
        wide = self.target('wide', trainer=self.candidate_trainer(), workers=2)
        _, _, derived = chains.throughput_plans(wide, 'C', 'tw', [1, 4])  # phase plans only: no chain runs
        pins = {len(phase['chains']): chains.pin(p) for p, phase in derived}
        other = self.copy_record(cand, 'other.json', lambda d: [p.update(plan=pins[p['concurrent_boxes']])
                                                                 for p in d['phases']])
        with self.assertRaisesRegex(chains.Stop, r'inputs\.arms\.C\.workers'):
            chains.compare_records([b], [other])
        with self.assertRaisesRegex(chains.Stop, 'baseline records ran different code'):
            chains.compare_records([b, c], [c])
        failed = self.copy_record(cand, 'failed.json', lambda d: d.update({'pass': False}))
        with self.assertRaisesRegex(chains.Stop, 'only passing'):
            chains.compare_records([b], [failed])
        # The command writes the report and exits 0 for a passing comparison, 1 for a parity failure.
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(chains.main(['throughput-compare', '--baseline', str(b), '--candidate', str(c),
                                          '--out', str(self.node.t / 'cmp.json')]), 0)
            self.assertEqual(chains.main(['throughput-compare', '--baseline', str(b), '--candidate', str(bad),
                                          '--out', str(self.node.t / 'cmp-bad.json')]), 1)
        self.assertTrue(json.loads((self.node.t / 'cmp.json').read_text())['pass'])
        self.assertFalse(json.loads((self.node.t / 'cmp-bad.json').read_text())['pass'])

    def test_mixed_records_are_compared_per_phase_slot_and_arm(self):
        base_plan, cand_plan = self.target('base'), self.target('cand', trainer=self.candidate_trainer())
        ok_b, base = self.mix(base_plan, 'mb')
        ok_c, cand = self.mix(cand_plan, 'mc')
        self.assertTrue(ok_b and ok_c, (base.get('why'), cand.get('why')))
        self.assertEqual([chains.phase_key(base, p) for p in base['phases']], ['serial-C', 'serial-CP', 'mix-4'])
        b, c = self.node.t / 'mb.json', self.node.t / 'mc.json'
        passed, report = chains.compare_records([b], [c])
        self.assertTrue(passed, report.get('why'))
        self.assertEqual(report['code']['differs'], ['trainer'])
        self.assertEqual(report['workload']['run']['slots'], SLOTS)
        self.assertEqual(sorted(report['workload']['layout']), ['C', 'CP'])
        self.assertEqual(sorted(report['metrics']), ['C.cpu_seconds_per_update', 'C.durable_updates_per_hour',
                                                     'CP.cpu_seconds_per_update', 'CP.durable_updates_per_hour',
                                                     'cpu_seconds_per_update', 'durable_updates_per_hour'])
        self.assertEqual(report['metrics']['CP.durable_updates_per_hour']['baseline'],
                         [base['phases'][-1]['durable']['per_arm']['CP']['updates_per_hour']])
        # A CP chain whose outputs differ is named by phase and slot.
        bad = self.copy_record(cand, 'mbad.json', lambda d: d['phases'][-1]['boxes'][3]['identity'].update(
            b000000001='0' * 64))
        passed, report = chains.compare_records([b], [bad])
        self.assertFalse(passed)
        self.assertEqual([(d['phase'], d['slot']) for d in report['parity']['differences']], [('mix-4', 3)])
        # A box-throughput record and a mixed record are never compared.
        homog = self.copy_record(cand, 'homog.json', lambda d: d.update(schema=w5box.THROUGHPUT_SCHEMA))
        with self.assertRaisesRegex(chains.Stop, 'one throughput record schema'):
            chains.compare_records([b], [homog])

    def test_verdicts_need_an_effect_beyond_the_spread_and_two_records_a_side(self):
        self.assertEqual(chains.spread_verdict([1.0, 1.1], [1.2, 1.3]), 'higher')
        self.assertEqual(chains.spread_verdict([1.0, 1.1], [0.8, 0.9]), 'lower')
        self.assertEqual(chains.spread_verdict([1.0, 1.2], [1.1, 1.3]), 'inconclusive')  # overlapping ranges
        self.assertEqual(chains.spread_verdict([1.0, 1.1], [1.1, 1.3]), 'inconclusive')  # touching ranges
        self.assertEqual(chains.spread_verdict([1.0, 1.5], [1.6, 1.7]), 'inconclusive')  # effect 0.4, spread 0.5
        self.assertEqual(chains.spread_verdict([1.0], [2.0, 2.1]), 'too few samples')
        m = chains.compare_metric([100.0, 104.0], [110.0, 112.0, 111.0])
        self.assertEqual((m['baseline_median'], m['candidate_median']), (102.0, 111.0))
        self.assertAlmostEqual(m['candidate_over_baseline'], 111.0 / 102.0)
        self.assertEqual((m['baseline_range'], m['candidate_range'], m['verdict']),
                         ([100.0, 104.0], [110.0, 112.0], 'higher'))
        self.assertAlmostEqual(m['separation_p'], 2 / 10)  # 2 of the C(5, 2) splits separate this completely
        self.assertAlmostEqual(chains.compare_metric([1.0, 1.1], [1.2, 1.3])['separation_p'], 1 / 3)  # one ABBA
        self.assertIsNone(chains.compare_metric([1.0, 1.2], [1.1, 1.3])['separation_p'])


if __name__ == '__main__':
    unittest.main()
