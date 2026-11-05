"""W5 box throughput qualification (tools/w5/chains.py `throughput`, `stage0-exit`) on any OS with the stand-in
trainer: phase plans bind the plan's own launch specification, a serial and a full-node phase run fresh chains on
one seed root and stop at N, the record passes w5box's guard rule (and a re-planned plan carrying it), a mismatch
fails it, and the Stage 0 exit numbers follow section 6's declared response. The real launcher path runs in
throughput_acceptance.py on Ubuntu."""
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
import w5box  # noqa: E402
from test_w5_chains import Node, STEP_C, STEP_X, jwrite  # noqa: E402

STAGE1A = {'c1': {'arm': 'C', 'slot': 0, 'pair': 1, 'seed_root': 'w5/chain1'},
           'x8-1': {'arm': 'X8', 'slot': 1, 'pair': 1, 'seed_root': 'w5/chain1'},
           'c2': {'arm': 'C', 'slot': 2, 'pair': 2, 'seed_root': 'w5/chain2'},
           'x8-2': {'arm': 'X8', 'slot': 3, 'pair': 2, 'seed_root': 'w5/chain2'}}


def stand_in_spawn(node):
    """What `chains.py run` does for the measurement, without cgroups: the w5box ledger's open and start events, a
    ledger source over a per-chain cpu.stat the stand-in trainer advances, and the in-box driver."""
    def spawn(plan_path, name, max_wall):
        plan = chains.read(plan_path)
        c = plan['chains'][name]
        node.ledger_source(name)
        led = w5box.Ledger(node.root / 'state' / name / 'ledger.jsonl')
        led.append({'event': 'open', 'instance': 0, 'base_usec': 0, 'cgroup': 'x', 'cgroup_inode': 0,
                    'manifest_sha256': '0'})
        led.append({'event': 'start', 'instance': 0, 'started_frozen': False})
        return subprocess.Popen([sys.executable, str(HERE.parent / 'chains.py'), '_drive', '--arm-file',
                                 plan['arms'][c['arm']]['file']['path'], '--chain', name, '--seed-root',
                                 c['seed_root'], '--state', str(node.root / 'state' / name), '--ledger-source',
                                 str(node.root / 'state' / name / 'ledger-source.json')],
                                stdout=subprocess.DEVNULL, stdin=subprocess.DEVNULL)
    return spawn


def occupancy(path, frozen, thawed):
    """A node log (hash-chained as w5box writes it) whose enforcement froze the node from `frozen` to `thawed`."""
    lines, prev = [], None
    for flag, unix in ((True, frozen), (False, thawed)):
        raw = w5box.canonical({'event': 'enforced', 'frozen': flag, 'unix': unix, 'prev_sha256': prev})
        prev = w5box.sha256_bytes(raw)
        lines.append(raw)
    path.write_bytes(b'\n'.join(lines) + b'\n')


class ThroughputTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.node = Node(self.tmp.name)
        env = mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X),
                                           'FAKE_SLEEP': '0.02'})
        env.start()
        self.addCleanup(env.stop)
        # No w5box supervisors here: the stand-in ledgers are open without one, which is not a dead supervisor.
        # The node log's lock is POSIX: closing the phases' handoff outages runs in throughput_acceptance.py.
        for name, value in (('recover_unsupervised', lambda plan, root: []), ('close_handoffs', lambda plan, root: [])):
            patch = mock.patch.object(chains, name, value)
            patch.start()
            self.addCleanup(patch.stop)

    def tearDown(self):
        self.tmp.cleanup()

    def target(self, **extra):
        return self.node.plan('1a', dict(STAGE1A), **extra)

    def run_throughput(self, path, arm, label, updates, **kw):
        out = self.node.t / f'{label}-{arm}.json'
        with contextlib.redirect_stdout(io.StringIO()):
            passed, record = chains.throughput_run(path, arm, label, updates, out, spawn=stand_in_spawn(self.node),
                                                   launcher=lambda p: None, poll=0.02, **kw)
        return passed, record, out

    def test_phase_plans_bind_the_plans_launch_specification(self):
        path, plan = self.target()
        for arm in ('C', 'X8'):
            target, binding, phases = chains.throughput_plans(path, arm, 'tq', [1, 2, 4])
            self.assertEqual(binding, chains.arm_binding(plan, arm))
            self.assertEqual([p['label'] for _, p in phases], [f'tq-{arm.lower()}-p{k}' for k in (1, 2, 4)])
            last = phases[-1][1]
            self.assertEqual(sorted(c['slot'] for c in last['chains'].values()), [0, 1, 2, 3])
            self.assertEqual({c['seed_root'] for c in last['chains'].values()}, {chains.THROUGHPUT_SEED_ROOT})
            self.assertEqual({c['arm'] for c in last['chains'].values()}, {arm})
            self.assertEqual(last['stage'], '0')
        with self.assertRaisesRegex(chains.Stop, 'used once'):
            chains.throughput_plans(path, 'C', 'tq', [1, 4])
        with self.assertRaisesRegex(chains.Stop, 'rise from 1'):
            chains.throughput_plans(path, 'C', 'tr', [1, 3])
        with self.assertRaisesRegex(chains.Stop, 'not a measured arm'):
            chains.throughput_plans(path, 'F', 'ts', [1, 4])

    def test_serial_and_full_node_record_passes_the_guard(self):
        path, plan = self.target()
        passed, record, out = self.run_throughput(path, 'C', 'tq', 8)
        self.assertTrue(passed, record.get('why'))
        self.assertEqual(json.loads(out.read_text()), json.loads(json.dumps(record)))
        self.assertEqual([p['concurrent_boxes'] for p in record['phases']], [1, 4])
        boxes = [b for p in record['phases'] for b in p['boxes']]
        self.assertEqual({b['ledger_usec'] for b in boxes}, {8 * STEP_C})
        self.assertEqual({b['updates'] for b in boxes}, {8})
        self.assertEqual(len(boxes[0]['identity']), 8)
        self.assertEqual({json.dumps(b['identity'], sort_keys=True) for b in boxes}, {json.dumps(
            boxes[0]['identity'], sort_keys=True)})
        self.assertTrue(all(b['seconds'] > 0 and b['in_box_utilization'] > 0 for b in boxes))
        self.assertEqual(record['qualification']['box_threads'], 1)
        self.assertEqual(set(record['phases'][-1]['exits'].values()), {chains.EXIT_STOP})
        # Each chain stopped at the segment that ends at N; the passing record removed the segment trees.
        for phase in record['phases']:
            for name in phase['exits']:
                man = json.loads((self.node.root / 'chains' / name / 'chain.json').read_text())
                self.assertEqual(len(man['segments']), 2)
                self.assertFalse(list((self.node.root / 'chains' / name).glob('seg-*')))
        # The plan, planned again with the record, binds it: w5box's guard rule accepts it.
        inputs = json.loads(Path(plan['inputs']['path']).read_text())
        inputs['arms']['C']['throughput'] = str(out)
        path2, plan2 = chains.make_plan(jwrite(self.node.t / 'inputs-with-record.json', inputs))
        box = json.loads(Path(plan2['arms']['C']['w5box_plan']['path']).read_text())
        self.assertEqual(box['throughput']['sha256'], chains.sha(out))
        guard = w5box.require_throughput(json.loads(out.read_text()), w5box.throughput_binding(box, box['chains']['c1']))
        self.assertEqual(len(guard['in_box_cpu_per_wall']), 4)
        # ... and not another arm's launch plan.
        xbox = json.loads(Path(plan2['arms']['X8']['w5box_plan']['path']).read_text())
        with self.assertRaises(w5box.Refusal):
            w5box.require_throughput(record, w5box.throughput_binding(xbox, xbox['chains']['x8-1']))

    def test_node_frozen_time_is_not_in_box_wall(self):
        path, _ = self.target()
        # A node freeze recorded by w5box's enforcement over the whole window is subtracted from the wall.
        now = time.time()
        occ = self.node.root / 'state' / 'occupancy.jsonl'
        occ.parent.mkdir(parents=True, exist_ok=True)
        occupancy(occ, now - 3600, now - 3599)
        passed, record, _ = self.run_throughput(path, 'X8', 'tf', 6, keep_trees=True)
        self.assertTrue(passed, record.get('why'))
        self.assertEqual({b['held_seconds'] for p in record['phases'] for b in p['boxes']}, {0.0})
        name = record['phases'][0]['boxes'][0]['chain']
        _, phase = chains.make_plan(Path(record['phases'][0]['plan']['path']).parent / 'INPUTS.json')
        start = record['phases'][0]['boxes'][0]['start_unix']
        occupancy(occ, start - 10, start + 0.01)
        m = chains.box_measurement(phase, name, 6, 1)
        self.assertAlmostEqual(m['held_seconds'], 0.01, places=6)
        self.assertAlmostEqual(m['seconds'], record['phases'][0]['boxes'][0]['seconds'] - 0.01, places=6)

    def test_a_different_output_fails_the_record(self):
        path, _ = self.target()
        original = chains.box_measurement

        def tampered(plan, name, updates, threads, memory_peak=None):
            m = original(plan, name, updates, threads, memory_peak)
            if name.endswith('p4-s3'):  # one box of the full-node phase reached another state at update 5
                m['identity'] = dict(m['identity'], b000000005='0' * 64)
            return m
        with mock.patch.object(chains, 'box_measurement', tampered):
            passed, record, out = self.run_throughput(path, 'C', 'tm', 8)
        self.assertFalse(passed)
        self.assertIn('differs from the single-box reference', record['why'])
        self.assertEqual(record['identity_differences'], [{'chain': 'tm-c-p4-s3', 'first': 'b000000005', 'count': 1}])
        self.assertFalse(out.exists())
        self.assertTrue(Path(f'{out}.failed.json').exists())
        self.assertTrue(list((self.node.root / 'chains' / 'tm-c-p4-s3').glob('seg-*')))  # kept for diagnosis

    def test_a_chain_short_of_n_fails_the_record(self):
        path, _ = self.target()
        with mock.patch.dict(os.environ, {'FAKE_FAIL_AT': '5'}):
            passed, record, out = self.run_throughput(path, 'C', 'tn', 8)
        self.assertFalse(passed)
        self.assertIn('phase tn-c-p1: tn-c-p1-s0 completed 5 of 8 updates', record['why'])
        self.assertEqual(record['phases'], [])
        self.assertTrue(Path(f'{out}.failed.json').exists())
        later = json.loads((self.node.root / 'chains' / 'tn-c-p4-s0' / 'chain.json').read_text())
        self.assertEqual(later['segments'], [])  # no later phase ran

    def test_refusals_before_any_chain(self):
        path, _ = self.target()
        with self.assertRaisesRegex(chains.Stop, 'whole C segments'):
            self.run_throughput(path, 'C', 'tz', 6)
        with self.assertRaisesRegex(chains.Stop, 'whole C segments'):
            self.run_throughput(path, 'C', 'tz', 4)
        (self.node.root / 'state').mkdir(parents=True, exist_ok=True)
        (self.node.root / 'state' / 'box2.owner.json').write_text('{"chain": "c2", "instance": 0}')
        with self.assertRaisesRegex(chains.Stop, 'runs chains'):
            self.run_throughput(path, 'C', 'tz', 8)
        self.assertFalse((self.node.root / 'chains' / 'tz-c-p1-s0').exists())


def record(arm, rates, threads=4, cores=2, serial=None, updates=10):
    boxes = len(rates)
    binding = {'host_class': 'linux-zen5-9950x', 'positions_sha256': 'p' * 64, 'cores_per_box': cores,
               'boxes': boxes, 'arm': arm, 'binary_sha256': 'b' * 64, 'config_sha256': arm.lower() * 4,
               'launch_spec_sha256': 'l' * 64}

    def box(rate):
        return {'seconds': 1000.0, 'updates': updates, 'ledger_usec': int(rate * 1000.0), 'identity': {'b1': 'h'},
                'memory_peak_bytes': 2 << 30}
    return {'schema': w5box.THROUGHPUT_SCHEMA, 'binding': binding, 'qualification': {'box_threads': threads},
            'phases': [{'concurrent_boxes': 1, 'boxes': [box(serial or rates[0])]},
                       {'concurrent_boxes': boxes, 'boxes': [box(r) for r in rates],
                        'node_memory': {'mem_available_min_bytes': 9 << 30}}]}


class Stage0ExitTests(unittest.TestCase):
    def test_days_to_r_nom_and_the_declared_response(self):
        c = record('C', [3.0e6] * 6)
        # X at the same in-box CPU rate as C reaches R_nom in 7 days; at 1.2x in about 5.8.
        out = chains.stage0_exit([c, record('X8', [3.6e6] * 6), record('XS', [3.0e6] * 5 + [2.9e6])])
        self.assertEqual(out['r_nom_source'][:11], 'provisional')
        self.assertAlmostEqual(out['r_nom_usec'], 3.0e6 * 7 * 86400)
        self.assertAlmostEqual(out['arms']['X8']['projected_days_to_r_nom'], 7 / 1.2)
        self.assertAlmostEqual(out['arms']['C']['in_box_utilization_mean'], 0.75)
        self.assertGreater(out['arms']['XS']['projected_days_slowest_box'], out['arms']['XS']['projected_days_to_r_nom'])
        self.assertEqual(out['declared_response_reading']['branch'], 'launch')
        self.assertEqual(out['declared_response_reading']['worst_arm'], 'XS')
        # 12 days: the cap response, ceil(1.2 x 12) = 15 days.
        out = chains.stage0_exit([c, record('X8', [3.0e6 * 7 / 12] * 6)])
        self.assertEqual((out['declared_response_reading']['branch'], out['declared_response_reading']['x_cap_days']),
                         ('cap', 15))
        self.assertEqual(out['x_arms_not_measured'], ['XS'])
        # Beyond 14 days: 3-core boxes from 2-core ones, else the stop for an amendment.
        slow = [3.0e6 * 7 / 15] * 6
        self.assertEqual(chains.stage0_exit([c, record('X8', slow)])['declared_response_reading']['branch'], 'widen')
        c3 = record('C', [4.5e6] * 4, threads=6, cores=3)
        out = chains.stage0_exit([c3, record('X8', [4.5e6 * 7 / 15] * 4, threads=6, cores=3)])
        self.assertEqual(out['declared_response_reading']['branch'], 'stop')
        # A given R_nom replaces the provisional one; X alone then needs no C record.
        out = chains.stage0_exit([record('X8', [3.0e6] * 6)], r_nom_usec=int(3.0e6 * 10 * 86400))
        self.assertAlmostEqual(out['arms']['X8']['projected_days_to_r_nom'], 10.0)
        self.assertEqual(out['declared_response_reading']['branch'], 'launch')

    def test_refusals(self):
        with self.assertRaisesRegex(chains.Stop, 'R_nom needs'):
            chains.stage0_exit([record('X8', [3.0e6] * 6)])
        with self.assertRaisesRegex(chains.Stop, 'different hosts or box layouts'):
            chains.stage0_exit([record('C', [3.0e6] * 6), record('X8', [3.0e6] * 4)])
        bad = record('X8', [3.0e6] * 6)
        bad['phases'][1]['boxes'][2]['identity'] = {'b1': 'other'}
        with self.assertRaisesRegex(chains.Stop, 'not a passing'):
            chains.stage0_exit([record('C', [3.0e6] * 6), bad])

    def test_command(self):
        with tempfile.TemporaryDirectory() as t:
            paths = [jwrite(Path(t) / f'{a}.json', record(a, [3.0e6] * 6)) for a in ('C', 'X8')]
            out = Path(t) / 'exit.json'
            with contextlib.redirect_stdout(io.StringIO()):
                code = chains.main(['stage0-exit', '--record', str(paths[0]), '--record', str(paths[1]), '--out',
                                    str(out)])
            self.assertEqual(code, 0)
            doc = json.loads(out.read_text())
            self.assertEqual(doc['schema'], chains.STAGE0_EXIT_SCHEMA)
            self.assertEqual([r['sha256'] for r in doc['records']], [chains.sha(p) for p in paths])


if __name__ == '__main__':
    unittest.main()
