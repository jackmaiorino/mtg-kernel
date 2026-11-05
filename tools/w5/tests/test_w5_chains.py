"""W5 chain controller (tools/w5/chains.py) on any OS with a stand-in trainer (fake_trainer.py): plan rules and
pins, the in-box driver's segments, C wall, stop at R_i, restarts, STOP and the byte guard, the pair decisions,
and a pair's selection with a replay, the PRUNE manifest and the hold folder. The real cgroup path (w5box) runs in
chains_acceptance.py on Ubuntu."""
import contextlib
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
import chains  # noqa: E402
import retention  # noqa: E402
import w5box  # noqa: E402

FAKE = HERE / 'fake_trainer.py'
CONTAINER = HERE / 'fake_container.py'
STEP_C, STEP_X = 1_000_000, 3_000_000


def jwrite(path, doc):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(doc), encoding='utf-8')
    return Path(path)


def dose(t, name):
    ck = Path(t) / f'{name}.ckpt'
    ck.write_text(f'{name} weights')
    return jwrite(Path(t) / f'{name}-dose.json', {'source': {'checkpoint': chains.pin(ck), 'play_import': {
        'path': '/imports/flat-v4.json', 'sha256': '1' * 64}}})


def base_doc(kinds=('current', 'fixed')):
    return {'schema': 'test-base', 'opponents': [{'id': 'd6107922'}], 'iterations': [
        {'episodes': [{'episode': {'learner_seat': s}, 'opponent': {'kind': k}} for s, k in enumerate(kinds)]}]}


class Node:
    """A node root with inputs for a plan."""

    def __init__(self, tmp, boxes=4):
        self.t = Path(tmp)
        self.root = self.t / 'node'
        self.b512, self.s0 = dose(self.t, 'b512'), dose(self.t, 's0')
        self.base = jwrite(self.t / 'base.json', base_doc())
        topology = {'online': list(range(boxes)), 'domains': [[[i] for i in range(boxes)]]}
        self.layout = jwrite(self.t / 'layout.json', dict(w5box.compute_layout(topology, 1, 0), host_class='test'))
        self.frozen = jwrite(self.t / 'FROZEN.json', {'w5': 'frozen constants'})

    def inputs(self, stage, chain_specs, label=None, **extra):
        arm = {'parent': str(self.b512), 'workers': 1, 'memory_max_bytes': 1 << 30, 'bytes_per_update': 1000}
        doc = {'schema': chains.INPUTS_SCHEMA, 'stage': stage, 'node': 'node1', 'root': self.root.as_posix(),
               'cgroup_root': (self.t / 'cgroup').as_posix(), 'isolation': 'none', 'host_class': 'test',
               'layout': str(self.layout), 'python': sys.executable, 'trainer': str(FAKE),
               'container_tool': str(CONTAINER), 'base': str(self.base), 'frozen': str(self.frozen),
               'arms': {'C': dict(arm, segment_updates=4), 'F': dict(arm, segment_updates=2),
                        'X8': dict(arm, segment_updates=3, budget=16384, coefficient=0.5, dose='primary'),
                        'XS': dict(arm, parent=str(self.s0), segment_updates=3, budget=16384, coefficient=0.7,
                                   dose='primary')},
               'chains': chain_specs}
        if label:
            doc['label'] = label
        doc.update(extra)
        return jwrite(self.t / f'inputs-{label or stage}.json', doc)

    def plan(self, stage, chain_specs, **extra):
        if stage != '0':
            extra.setdefault('r_nom_usec', 10**9)
        path, plan = chains.make_plan(self.inputs(stage, chain_specs, **extra))
        return path, plan

    def stage0(self, c_wall=1.5, **extra):
        return self.plan('0', {'c1': {'arm': 'C', 'slot': 0, 'pair': 1, 'seed_root': 'w5/q1'},
                               'x1': {'arm': 'X8', 'slot': 1, 'pair': 1, 'seed_root': 'w5/q1'},
                               'f2': {'arm': 'F', 'slot': 2, 'seed_root': 'w5/qf2'}},
                         c_wall_seconds=c_wall, **extra)

    def ledger_source(self, name, stop=None, closed=0):
        state = self.root / 'state' / name
        stat = state / 'cpu.stat'
        if not stat.exists():
            stat.parent.mkdir(parents=True, exist_ok=True)
            stat.write_text('usage_usec 0\n')
        doc = {'schema': 'w5-chain-ledger-source/v1', 'closed_usec': closed, 'instance': 0,
               'cpu_stat': stat.as_posix()}
        if stop is not None:
            doc['stop_at_usec'] = stop
        return jwrite(state / 'ledger-source.json', doc)

    def guard(self, ok=True, ceilings=None):
        jwrite(self.root / 'GUARD.json', {'ok': ok, 'unix': time.time(), 'used_bytes': 0, 'cap_bytes': 1,
                                          'ceilings': ceilings or {'c1': 10**6, 'x1': 10**6}})

    def ledger(self, name, *events):
        led = w5box.Ledger(self.root / 'state' / name / 'ledger.jsonl')
        (self.root / 'state' / name / 'control').mkdir(parents=True, exist_ok=True)
        for event in events:
            if event == 'open':
                led.append({'event': 'open', 'instance': 0, 'base_usec': 0, 'cgroup': 'x', 'cgroup_inode': 0,
                            'manifest_sha256': '0'})
            else:
                led.append({'event': 'close', 'instance': 0, 'final_usec': 0, 'closed_total_usec': 0})

    def drive(self, plan, name):
        c = plan['chains'][name]
        source = self.root / 'state' / name / 'ledger-source.json'
        if not source.exists():
            self.ledger_source(name)
        return chains.main(['_drive', '--arm-file', plan['arms'][c['arm']]['file']['path'], '--chain', name,
                            '--seed-root', c['seed_root'], '--state', str(self.root / 'state' / name),
                            '--ledger-source', str(source)])


class ChainTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.node = Node(self.tmp.name)
        env = mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X),
                                           'FAKE_SLEEP': '0.01'})
        env.start()
        self.addCleanup(env.stop)

    def tearDown(self):
        self.tmp.cleanup()

    # ------------------------------------------------------------ plan

    def test_plan_pins_the_arm_files_and_is_idempotent(self):
        path, plan = self.node.stage0()
        first = chains.sha(path)
        path2, _ = self.node.stage0()
        self.assertEqual(chains.sha(path2), first)
        box = json.loads(Path(plan['arms']['X8']['w5box_plan']['path']).read_text())
        spec = box['chains']['x1']
        self.assertEqual(spec['argv'][0], Path(sys.executable).as_posix())
        self.assertEqual(spec['binary']['sha256'], chains.sha(sys.executable))
        self.assertEqual(spec['config'], plan['arms']['X8']['file'])
        self.assertIn(plan['arms']['X8']['file']['path'], spec['argv'])
        self.assertEqual(box['filler']['config'], plan['arms']['F']['file'])
        # w5box's own pin check accepts the launch (argv[0] is the pinned binary, the arm file is in argv)
        w5box.check_launch(spec, w5box.chain_argv(spec, 'x1', Path(box['state_root']) / 'x1'), 'chain')
        w5box.check_launch(box['filler'], w5box.filler_argv(box['filler'], 'w5/filler0', 0, 0), 'filler')
        arm = json.loads(Path(plan['arms']['X8']['file']['path']).read_text())
        self.assertEqual(arm['x'], {'budget': 16384, 'coefficient': 0.5, 'dose': 'primary'})
        self.assertEqual(arm['tools'], chains.tools_shas())
        man = json.loads(Path(plan['chains']['x1']['manifest']).read_text())
        self.assertEqual((man['arm'], man['arm_name'], man['seed_root'], man['segments']), ('X', 'X8', 'w5/q1', []))
        self.assertEqual(plan['pairs'], {'1': {'C': 'c1', 'X': 'x1', 'c_external': False}})

    def test_r_nom_is_frozen_in_the_manifests(self):
        self.node.stage0(r_nom_usec=100)
        with self.assertRaisesRegex(chains.Stop, 'R_nom is frozen'):
            self.node.stage0(r_nom_usec=200)
        _, plan = self.node.stage0(r_nom_usec=100)
        self.assertEqual(json.loads(Path(plan['chains']['c1']['manifest']).read_text())['r_nom_usec'], 100)

    def test_stage0_unpaired_chains_share_a_seed_root(self):
        _, plan = self.node.plan('0', {f'tc{k}': {'arm': 'C', 'slot': k, 'seed_root': 'w5/q-throughput'}
                                       for k in range(4)}, c_wall_seconds=60.0)
        self.assertEqual(plan['pairs'], {})
        box = json.loads(Path(plan['arms']['C']['w5box_plan']['path']).read_text())
        self.assertEqual(len({json.dumps(dict(spec, box_slot=None)) for spec in box['chains'].values()}), 1)
        with self.assertRaises(chains.Stop):  # paired chains keep their own seed roots
            self.node.plan('0', {'c1': {'arm': 'C', 'slot': 0, 'pair': 1, 'seed_root': 'w5/q1'},
                                 'c2': {'arm': 'C', 'slot': 1, 'pair': 2, 'seed_root': 'w5/q1'}})

    def stage1a(self, **changes):
        specs = {'c1': {'arm': 'C', 'slot': 0, 'pair': 1, 'seed_root': 'w5/chain1'},
                 'x8-1': {'arm': 'X8', 'slot': 1, 'pair': 1, 'seed_root': 'w5/chain1'},
                 'c2': {'arm': 'C', 'slot': 2, 'pair': 2, 'seed_root': 'w5/chain2'},
                 'x8-2': {'arm': 'X8', 'slot': 3, 'pair': 2, 'seed_root': 'w5/chain2'}}
        for name, spec in changes.items():
            if spec is None:
                specs.pop(name)
            else:
                specs[name] = spec
        return self.node.plan('1a', specs)

    def test_stage1_rules(self):
        _, plan = self.stage1a()
        self.assertEqual(sorted(plan['pairs']), ['1', '2'])
        for bad in ({'x8-2': {'arm': 'X8', 'slot': 3, 'pair': 2, 'seed_root': 'w5/chain7'}},
                    {'x8-2': None},
                    {'x8-2': {'arm': 'C', 'slot': 3, 'pair': 3, 'seed_root': 'w5/chain3'}}):
            with self.assertRaises(chains.Stop):
                self.stage1a(**bad)
        with self.assertRaises(chains.Stop):  # Stage 1 needs the freeze record and the 7-day C wall
            self.node.plan('1a', {}, c_wall_seconds=60.0)

    def test_x8_differs_from_c_only_in_the_search_block(self):
        other = jwrite(self.node.t / 'other-base.json', base_doc(('fixed', 'current')))
        inputs = json.loads(self.node.inputs('1a', {}, r_nom_usec=10**9).read_text())
        inputs['arms']['X8']['base'] = str(other)
        inputs['chains'] = {'c1': {'arm': 'C', 'slot': 0, 'pair': 1, 'seed_root': 'w5/chain1'},
                            'x8-1': {'arm': 'X8', 'slot': 1, 'pair': 1, 'seed_root': 'w5/chain1'},
                            'c2': {'arm': 'C', 'slot': 2, 'pair': 2, 'seed_root': 'w5/chain2'},
                            'x8-2': {'arm': 'X8', 'slot': 3, 'pair': 2, 'seed_root': 'w5/chain2'}}
        with self.assertRaisesRegex(chains.Stop, 'opponent schedules'):
            chains.make_plan(jwrite(self.node.t / 'bad.json', inputs))
        # A five-update schedule that differs only at update 2 is refused too (the whole cycle is compared).
        five = base_doc()
        five['iterations'] = five['iterations'] * 5
        jwrite(self.node.base, five)
        changed = json.loads(json.dumps(five))
        changed['iterations'][2]['episodes'][1]['opponent']['kind'] = 'current'
        inputs['arms']['X8']['base'] = str(jwrite(self.node.t / 'five-x.json', changed))
        with self.assertRaisesRegex(chains.Stop, 'opponent schedules'):
            chains.make_plan(jwrite(self.node.t / 'bad5.json', inputs))
        jwrite(self.node.base, base_doc())
        jwrite(self.node.base, base_doc(('current', 'initial')))
        with self.assertRaisesRegex(chains.Stop, 'history-relative'):
            self.stage1a()

    def test_stage1b_pairs_xs_with_ended_c_chains(self):
        self.stage1a()
        specs = {'xs-1': {'arm': 'XS', 'slot': 0, 'pair': 1, 'seed_root': 'w5/chain1', 'partner': 'c1'},
                 'xs-2': {'arm': 'XS', 'slot': 1, 'pair': 2, 'seed_root': 'w5/chain2', 'partner': 'c2'},
                 'f2': {'arm': 'F', 'slot': 2, 'seed_root': 'w5/filler2'},
                 'f3': {'arm': 'F', 'slot': 3, 'seed_root': 'w5/filler3'}}
        with self.assertRaisesRegex(chains.Stop, 'has not ended'):
            self.node.plan('1b', specs)
        for c in ('c1', 'c2'):
            chains.write_once(self.node.root / 'chains' / c / 'END.json', {'reason': 'c-wall'})
        _, plan = self.node.plan('1b', specs)
        self.assertEqual(plan['pairs']['1'], {'C': 'c1', 'X': 'xs-1', 'c_external': True})
        self.assertEqual(plan['xs_recipe_differences'], {'1': ['initial_source'], '2': ['initial_source']})

    # ------------------------------------------------------------ the in-box driver

    def test_c_chain_runs_segments_to_its_wall_and_matches_one_run(self):
        _, plan = self.node.stage0(c_wall=1.5)
        self.node.guard()
        self.assertEqual(self.node.drive(plan, 'c1'), 0)
        root = self.node.root
        end = json.loads((root / 'chains' / 'c1' / 'END.json').read_text())
        chain, records = retention.index(plan['chains']['c1']['manifest'])
        self.assertEqual(end['reason'], 'c-wall')
        self.assertGreaterEqual(len(records), 8)
        self.assertEqual(end['last_valid_boundary'], records[-1]['boundary'])
        self.assertEqual(end['ledger_usec'], records[-1]['ledger_usec'])
        self.assertEqual([r['ledger_usec'] for r in records], [STEP_C * (i + 1) for i in range(len(records))])
        self.assertEqual([s['offset'] for s in chain['segments']], [4 * i for i in range(len(chain['segments']))])
        # Segmented equals one run: the first 8 updates as a single native run reach the same states.
        arm = chains.load_arm(plan['arms']['C']['file']['path'])
        cfg = chains.arm_config(arm, 'w5/q1', 0, 8, self.node.t / 'whole')
        path = jwrite(self.node.t / 'whole.json', cfg)
        self.assertEqual(chains.run_trainer(arm, path)[0], 0)
        whole = [json.loads(Path(json.loads(p.read_text())['update']['path']).read_text())['after_state_sha256']
                 for p in sorted((self.node.t / 'whole' / 'iterations').glob('*/complete.json'))]
        self.assertEqual(whole, [r['after_state_sha256'] for r in records[:8]])
        # An ended chain does nothing more.
        self.assertEqual(self.node.drive(plan, 'c1'), 0)

    def test_x_chain_restarts_after_a_failure_and_stops_at_r(self):
        _, plan = self.node.stage0()
        self.node.guard()
        with mock.patch.dict(os.environ, {'FAKE_FAIL_AT': '5'}):
            self.assertEqual(self.node.drive(plan, 'x1'), chains.EXIT_FAILED)
        cdir = self.node.root / 'chains' / 'x1'
        self.assertEqual(json.loads((cdir / 'FAILURE.json').read_text())['boundary'], 5)
        self.node.ledger_source('x1', stop=8 * STEP_X)
        self.assertEqual(self.node.drive(plan, 'x1'), 0)
        end = json.loads((cdir / 'END.json').read_text())
        self.assertEqual((end['reason'], end['last_valid_boundary'], end['ledger_usec']), ('x-stop-at', 8, 8 * STEP_X))
        self.assertTrue(list((cdir / 'interrupted').iterdir()))  # the crashed iteration was moved aside
        _, records = retention.index(plan['chains']['x1']['manifest'])
        self.assertEqual(len(records), 8)
        cfg = json.loads(Path(json.loads(Path(plan['chains']['x1']['manifest']).read_text())
                              ['segments'][2]['config']).read_text())
        self.assertEqual(cfg['ex1']['update_offset'], 6)
        self.assertEqual(cfg['ex1']['w5'], {'budget': 16384})
        self.assertEqual(cfg['chain_ledger']['source'], (self.node.root / 'state' / 'x1' /
                                                         'ledger-source.json').as_posix())

    def test_stop_file_guard_and_pins(self):
        _, plan = self.node.stage0()
        self.node.guard()
        (self.node.root / 'STOP').write_text('')
        self.assertEqual(self.node.drive(plan, 'c1'), chains.EXIT_STOP)
        (self.node.root / 'STOP').unlink()
        self.node.guard(ok=False)
        with mock.patch.object(chains, 'GUARD_WAIT', 0.0), mock.patch.object(chains, 'GUARD_POLL', 0.0):
            self.assertEqual(self.node.drive(plan, 'c1'), chains.EXIT_ROOM)
        self.assertEqual(json.loads(Path(plan['chains']['c1']['manifest']).read_text())['segments'], [])
        trainer = Path(self.tmp.name) / 'trainer-copy.py'
        trainer.write_text(FAKE.read_text())
        _, plan = self.node.stage0(trainer=str(trainer))
        trainer.write_text(FAKE.read_text() + '\n# changed\n')
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(self.node.drive(plan, 'c1'), w5box.EXIT_REFUSED)
            self.assertEqual(chains.main(['run', '--plan', str(self.node.root / 'plan' / 'stage0' /
                                                               'CHAINS-PLAN.json'), '--chain', 'c1']),
                             w5box.EXIT_REFUSED)

    def test_a_guard_verdict_admits_a_bounded_number_of_segments(self):
        _, plan = self.node.stage0(c_wall=600.0)
        self.node.guard(ceilings={'c1': 2, 'x1': 1})
        with mock.patch.object(chains, 'GUARD_WAIT', 0.0), mock.patch.object(chains, 'GUARD_POLL', 0.0):
            self.assertEqual(self.node.drive(plan, 'c1'), chains.EXIT_ROOM)
            self.assertEqual(len(json.loads(Path(plan['chains']['c1']['manifest']).read_text())['segments']), 2)
            # A rerun of an unfinished segment is an admission too (its moved-aside attempt stays on disk).
            with mock.patch.dict(os.environ, {'FAKE_FAIL_AT': '1'}):
                self.assertEqual(self.node.drive(plan, 'x1'), chains.EXIT_FAILED)
                self.assertEqual(self.node.drive(plan, 'x1'), chains.EXIT_ROOM)
            self.assertEqual(chains.admitted(self.node.root / 'chains' / 'x1'), 1)

    def test_filler_keeps_only_the_live_segment(self):
        _, plan = self.node.stage0()
        arm = chains.load_arm(plan['arms']['F']['file']['path'])
        out = self.node.root / 'fillers' / 'slot1'
        (out / 'run-old').mkdir(parents=True)
        calls = []
        code = chains.run_filler(arm, out, 'w5/filler1', lambda: calls.append(1) or len(calls) > 3)
        self.assertEqual(code, chains.EXIT_STOP)
        runs = list(out.glob('run-*'))
        self.assertEqual(len(runs), 1)
        self.assertEqual(sorted(p.name for p in runs[0].glob('seg-*')), ['seg-00002'])

    def test_segcheck(self):
        path, _ = self.node.stage0()
        for arm in ('C', 'X8'):
            out = self.node.t / f'segcheck-{arm}.json'
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(chains.main(['segcheck', '--plan', str(path), '--arm', arm, '--out', str(out)]), 0)
            self.assertTrue(json.loads(out.read_text())['pass'])

    # ------------------------------------------------------------ run and end

    def test_run_records_an_oom_kill_and_end_is_once(self):
        path, plan = self.node.stage0()
        self.node.guard()
        with mock.patch.dict(os.environ, {'FAKE_FAIL_AT': '3'}):
            self.node.drive(plan, 'c1')
        with mock.patch.object(chains.subprocess, 'call', return_value=w5box.EXIT_OOM) as call:
            self.assertEqual(chains.main(['run', '--plan', str(path), '--chain', 'c1']), w5box.EXIT_OOM)
        self.assertEqual(call.call_args[0][0][2:7], ['run', '--plan', plan['arms']['C']['w5box_plan']['path'],
                                                     '--chain', 'c1'])
        end = json.loads((self.node.root / 'chains' / 'c1' / 'END.json').read_text())
        self.assertEqual((end['reason'], end['last_valid_boundary'], end['detail']), ('chain-failure', 3,
                                                                                       {'cause': 'oom kill'}))
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(chains.main(['run', '--plan', str(path), '--chain', 'c1']), w5box.EXIT_REFUSED)
        with mock.patch.dict(os.environ, {'FAKE_FAIL_AT': '2'}):
            self.node.drive(plan, 'x1')
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(chains.main(['end', '--plan', str(path), '--chain', 'x1', '--reason', 'quality gate',
                                          '--boundary', '1']), 0)
            self.assertEqual(chains.main(['end', '--plan', str(path), '--chain', 'x1', '--reason', 'again']), 1)
        end = json.loads((self.node.root / 'chains' / 'x1' / 'END.json').read_text())
        self.assertEqual((end['reason'], end['last_valid_boundary'], end['ledger_usec']), ('operator', 1, STEP_X))

    # ------------------------------------------------------------ supervise

    def test_running_seconds(self):
        events = [{'event': 'open', 'unix': 0}, {'event': 'pause', 'unix': 10}, {'event': 'resume', 'unix': 25},
                  {'event': 'close', 'unix': 30}, {'event': 'open', 'unix': 100}, {'event': 'pause', 'unix': 104}]
        self.assertEqual(chains.running_seconds(events, 200), 15 + 4)
        self.assertEqual(chains.running_seconds(events[:5], 110), 15 + 10)

    def synthetic(self, plan, c_end=None, x_ledgers=(), paused=False, stop_at=None, x_cap=1e6, opened=0.0):
        t0 = 1.0e9
        c_records = [{'boundary': b, 'ledger_usec': b * 1_000_000, 'unix_seconds': t0 + b} for b in range(1, 51)]
        c = {'name': 'c1', 'chain': {'launched_unix': t0, 'c_wall_seconds': 100.0}, 'records': c_records,
             'end': c_end, 'live': c_end is None}
        x_records = [{'boundary': i + 1, 'ledger_usec': v, 'unix_seconds': t0 + i, 'checkpoint': {
            'path': '/missing', 'sha256': '0'}} for i, v in enumerate(x_ledgers)]
        state = self.node.root / 'state' / 'x1'
        (state / 'control').mkdir(parents=True, exist_ok=True)
        if paused:
            jwrite(state / 'paused.json', {})
        x = {'name': 'x1', 'path': Path(plan['chains']['x1']['manifest']), 'chain': {'x_cap_seconds': x_cap},
             'records': x_records, 'end': None, 'live': True, 'paused': paused, 'state': state, 'stop_at': stop_at,
             'ledger': [{'event': 'open', 'unix': t0 + opened}]}
        return c, x, t0 + 50

    def control(self):
        return sorted(p.name for p in (self.node.root / 'state' / 'x1' / 'control').iterdir())

    def test_pair_decisions_drive_w5box(self):
        _, plan = self.node.stage0()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            # R-hat = 50e6 + 1e6/s x 50 s = 100e6. Inside the snapshot window (0.85 to 1.10 R-hat):
            st = {'probe_bytes': 10}
            c, x, now = self.synthetic(plan, x_ledgers=[40e6, 80e6, 90e6])
            self.assertEqual(chains.pair_decide(plan, c, x, st, now)['action'], 'snapshot')
            self.assertEqual((st['snapshot_from'], st['snapshotting']), (3, True))
            self.assertEqual(self.control(), [])
            # Past 1.10 R-hat: one controller pause.
            c, x, now = self.synthetic(plan, x_ledgers=[40e6, 112e6])
            self.assertEqual(chains.pair_decide(plan, c, x, st, now)['action'], 'pause')
            self.assertEqual(self.control(), ['pause.now'])
            chains.pair_decide(plan, c, x, st, now)
            self.assertEqual(self.control(), ['pause.now'])
            (self.node.root / 'state' / 'x1' / 'control' / 'pause.now').unlink()
            # C ended at R = 50e6: a paused X below R resumes with stop_at_usec = R.
            c_end = {'reason': 'c-wall', 'last_valid_boundary': 50}
            c, x, now = self.synthetic(plan, c_end, x_ledgers=[40e6], paused=True)
            self.assertEqual(chains.pair_decide(plan, c, x, {}, now)['action'], 'resume_to')
            req = json.loads((self.node.root / 'state' / 'x1' / 'control' / 'resume.request').read_text())
            self.assertEqual(req['stop_at_usec'], 50_000_000)
            (self.node.root / 'state' / 'x1' / 'control' / 'resume.request').unlink()
            (self.node.root / 'state' / 'x1' / 'paused.json').unlink()
            # A running X without that stop: paused, to be resumed with it; with it: left alone.
            c, x, now = self.synthetic(plan, c_end, x_ledgers=[40e6])
            chains.pair_decide(plan, c, x, {}, now)
            self.assertEqual(self.control(), ['pause.now'])
            (self.node.root / 'state' / 'x1' / 'control' / 'pause.now').unlink()
            c, x, now = self.synthetic(plan, c_end, x_ledgers=[40e6], stop_at=50_000_000)
            chains.pair_decide(plan, c, x, {}, now)
            self.assertEqual(self.control(), [])
            # An open window stays open while R_i is not final, even when the projection rises.
            st = {'probe_bytes': 10, 'snapshot_from': 3}
            c, x, now = self.synthetic(plan, x_ledgers=[40e6])
            self.assertEqual(chains.pair_decide(plan, c, x, st, now)['action'], 'run')
            self.assertTrue(st['snapshotting'])
            # A C failure sets R at its last valid boundary: X is past it, so X ends and is killed.
            c, x, now = self.synthetic(plan, {'reason': 'operator', 'last_valid_boundary': 30},
                                       x_ledgers=[20e6, 40e6])
            self.assertEqual(chains.pair_decide(plan, c, x, {}, now)['r_usec'], 30_000_000)
            self.assertEqual(self.control(), ['kill.request'])
            end = json.loads((self.node.root / 'chains' / 'x1' / 'END.json').read_text())
            self.assertEqual(end['reason'], 'reached-r')
        (self.node.root / 'chains' / 'x1' / 'END.json').unlink()
        (self.node.root / 'state' / 'x1' / 'control' / 'kill.request').unlink()
        with contextlib.redirect_stdout(out):
            # The X cap while C runs: X stops, labelled later.
            c, x, now = self.synthetic(plan, x_ledgers=[10e6], x_cap=40.0)
            self.assertEqual(chains.pair_decide(plan, c, x, {}, now)['action'], 'stop_under_budget')
            self.assertEqual(json.loads((self.node.root / 'chains' / 'x1' / 'END.json').read_text())['reason'],
                             'x-cap')

    def test_x_cap_before_the_first_update_and_kill_retries(self):
        _, plan = self.node.stage0()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            c, x, now = self.synthetic(plan, x_ledgers=[], x_cap=40.0)
            c['records'] = []
            self.assertEqual(chains.pair_decide(plan, c, x, {}, now)['action'], 'stop_under_budget')
            self.assertEqual(self.control(), ['kill.request'])
            (self.node.root / 'state' / 'x1' / 'control' / 'kill.request').unlink()
            # The kill did not land (X still open): it is requested again once the END is old enough.
            self.node.ledger('x1', 'open')
            self.assertEqual(chains.enforce_ends(plan, self.node.root), [])
            end_path = self.node.root / 'chains' / 'x1' / 'END.json'
            end = json.loads(end_path.read_text())
            end['unix'] -= 2 * chains.END_KILL_GRACE
            end_path.write_text(json.dumps(end))
            self.assertEqual(chains.enforce_ends(plan, self.node.root), ['x1'])
            self.assertEqual(self.control(), ['kill.request'])

    def test_an_ended_chains_box_holds_the_filler(self):
        path, plan = self.node.stage0()
        root = self.node.root
        chains.write_once(root / 'chains' / 'c1' / 'END.json', {'reason': 'c-wall', 'unix': time.time()})
        self.node.ledger('c1', 'open', 'close')
        self.node.ledger('x1', 'open')
        with mock.patch.object(chains.subprocess, 'Popen') as popen, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(chains.occupancy(plan, root), {'started': [0], 'stopped': [], 'killed': []})
            cmd = popen.call_args[0][0]
            stop = root / 'plan' / 'stage0' / 'fills' / 'STOP'
            self.assertEqual(cmd[1:9], [str(chains.HERE / 'w5box.py'), 'fill', '--plan',
                                        plan['arms']['C']['w5box_plan']['path'], '--slot', '0', '--stop', str(stop)])
            self.assertEqual(cmd[9], '--empty-since')
            self.assertEqual(chains.occupancy(plan, root)['started'], [])  # starting
            # Every measured chain ended and closed: the fill stops and the F chains go.
            (root / 'state' / 'box0.fill.json').write_text(json.dumps({'pid': 1, 'unix': time.time()}))
            chains.write_once(root / 'chains' / 'x1' / 'END.json', {'reason': 'x', 'unix': time.time()})
            self.node.ledger('x1', 'close')
            self.node.ledger('f2', 'open')
            with mock.patch.object(chains, 'fill_running', lambda root, slot: slot == 0):
                self.assertEqual(chains.occupancy(plan, root), {'started': [], 'stopped': [0], 'killed': ['f2']})
        self.assertTrue(stop.exists())
        self.assertTrue((root / 'state' / 'f2' / 'control' / 'kill.request').exists())

    def test_guard_reservation(self):
        _, plan = self.node.stage0()
        c, x, f = (plan['arms'][a]['segment_updates'] * 1000 for a in ('C', 'X8', 'F'))
        self.assertEqual(chains.guard_reservation(plan, self.node.root), 5 * (c + x) + 2 * 3 * f)
        chains.write_once(self.node.root / 'chains' / 'c1' / 'END.json', {'reason': 'c-wall'})
        self.assertEqual(chains.guard_reservation(plan, self.node.root), 5 * x + 2 * 3 * f)
        self.node.ledger('c1', 'open')  # ended but still open: it may still write
        self.assertEqual(chains.guard_reservation(plan, self.node.root), 5 * (c + x) + 2 * 3 * f)

    def test_supervise_selects_replays_and_prunes_a_pair(self):
        path, plan = self.node.stage0(c_wall=1.5, r_nom_usec=10_000_000)
        self.node.guard()
        self.assertEqual(self.node.drive(plan, 'c1'), 0)
        _, c_records = retention.index(plan['chains']['c1']['manifest'])
        r = c_records[-1]['ledger_usec']
        stop = (r // STEP_X + 3) * STEP_X
        self.node.ledger_source('x1', stop=stop)
        self.assertEqual(self.node.drive(plan, 'x1'), 0)
        _, x_records = retention.index(plan['chains']['x1']['manifest'])
        hit = retention.first_at_or_past(x_records, r)
        self.assertLess(hit['boundary'], x_records[-1]['boundary'])
        Path(hit['checkpoint']['path']).unlink()  # as a rolling prune leaves it: the endpoint needs a replay
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            report = chains.cycle(chains.load_plan(path), path)
            self.assertIn('pending', report['pairs']['1']['waiting'])
            rdir = self.node.root / 'chains' / 'x1' / 'replay' / f'b{hit["boundary"]:09d}'
            deadline = time.time() + 60
            while not (rdir / 'EXIT.json').exists() and time.time() < deadline:
                time.sleep(0.2)
            report = chains.cycle(chains.load_plan(path), path)
        pair = json.loads((self.node.root / 'pairs' / 'c1--x1' / 'PAIR.json').read_text())
        self.assertTrue(pair['done'], pair)
        self.assertEqual(pair['label'], 'matched')
        self.assertEqual(pair['endpoints'], {'c1': c_records[-1]['boundary'], 'x1': hit['boundary']})
        self.assertEqual(pair['selected']['x1']['kind'], 'replay')
        self.assertTrue(pair['selected']['x1']['receipt']['equal'])
        self.assertEqual(pair['selected']['c1']['kind'], 'checkpoint')
        # The looks (first boundary at 0.5 R_nom) stay; every other non-selected full state went to the hold.
        looks = {n: json.loads((self.node.root / 'chains' / n / 'LOOK.json').read_text())['boundary']
                 for n in ('c1', 'x1')}
        self.assertEqual(looks, {'c1': 5, 'x1': 2})
        for name, records, keep in (('c1', c_records, {5, c_records[-1]['boundary']}), ('x1', x_records, {2})):
            for rec in records:
                self.assertEqual(Path(rec['checkpoint']['path']).exists(), rec['boundary'] in keep,
                                 (name, rec['boundary']))
        moves = (self.node.root / 'hold' / 'MOVES.jsonl').read_text().splitlines()
        self.assertEqual(len(moves), len(c_records) - 2 + len(x_records) - 2)
        self.assertTrue(all(Path(json.loads(m)['to']).exists() for m in moves))
        self.assertTrue(json.loads((self.node.root / 'GUARD.json').read_text())['ok'])
        self.assertEqual(chains.purge_hold(self.node.root, 7, time.time()), 0)
        self.assertEqual(chains.purge_hold(self.node.root, 7, time.time() + 8 * 86400), len(moves))
        with contextlib.redirect_stdout(io.StringIO()) as status:
            chains.main(['status', '--plan', str(path)])
        doc = json.loads(status.getvalue())
        self.assertEqual(doc['chains']['x1']['end'], 'x-stop-at')
        self.assertTrue(doc['pairs']['1']['done'])

    def test_rolling_retention_keeps_strides_and_the_look(self):
        path, plan = self.node.stage0(c_wall=2.0, r_nom_usec=10_000_000)
        self.node.guard()
        self.node.ledger_source('x1', stop=30 * STEP_X)
        self.assertEqual(self.node.drive(plan, 'x1'), 0)
        (self.node.root / 'chains' / 'x1' / 'END.json').unlink()  # as if still running
        _, records = retention.index(plan['chains']['x1']['manifest'])
        with contextlib.redirect_stdout(io.StringIO()):
            chains.cycle(chains.load_plan(path), path)
        present = {r['boundary'] for r in records if Path(r['checkpoint']['path']).exists()}
        # Segments of 3: a segment's checkpoints go once the next segment is complete, so only the last segment
        # (28 to 30) keeps them; the look (boundary 2) stays.
        self.assertEqual(present, {2, 28, 29, 30})

    def test_load_chain_trusts_entry_fields(self):
        _, plan = self.node.stage0()
        self.node.guard()
        self.node.ledger_source('x1', stop=4 * STEP_X)
        self.node.drive(plan, 'x1')
        man = json.loads(Path(plan['chains']['x1']['manifest']).read_text())
        for seg in man['segments']:
            Path(seg['config']).unlink()
        chain = retention.load_chain(plan['chains']['x1']['manifest'])
        self.assertEqual([s['updates'] for s in chain['segments']], [3, 3])


if __name__ == '__main__':
    unittest.main()
