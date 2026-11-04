import argparse
import hashlib
import importlib.util
import json
import math
import os
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

TOOLS = Path(__file__).parents[1] / 'tools'
sys.path.insert(0, str(TOOLS))
spec = importlib.util.spec_from_file_location('checkpoint_v2_acceptance_v1', TOOLS / 'checkpoint_v2_acceptance_v1.py')
acc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acc)


def group(phase, pattern, couple, rates, contaminated=(), d_read=1e6, index=0):
    blocks = []
    for pos, label in enumerate(pattern):
        blocks.append({'label': label, 'throughput': rates[label] if isinstance(rates, dict) else rates[pos],
                       'contaminated': pos in contaminated, 'reasons': [], 'd_read_mean': d_read})
    return {'phase': phase, 'pattern': pattern, 'couple': couple, 'blocks': blocks, 'index': index}


def drive(state, rates_for, contaminate=lambda phase, k: ()):
    """Runs the schedule to its outcome with synthetic block rates; returns the group sequence."""
    seen = []
    while True:
        g = acc.next_group(state)
        if g is None:
            return seen
        seen.append((g['phase'], g['pattern'], g['couple']))
        acc.record_group(state, group(g['phase'], g['pattern'], g['couple'], rates_for(g, len(seen)),
                                      contaminate(g['phase'], len(seen)), index=len(seen) - 1))


class StatisticsTests(unittest.TestCase):
    def test_sign_flip_needs_six_couples(self):
        self.assertEqual(acc.sign_flip_p([0.1, 0.2, 0.3, 0.4, 0.5, 0.6]), 1 / 64)
        self.assertEqual(acc.sign_flip_p([0.1, 0.2, 0.3, 0.4, 0.5]), 1 / 32)
        self.assertGreater(acc.sign_flip_p([0.1, 0.2, 0.3, 0.4, 0.5]), acc.GUARD_ALPHA)

    def test_sign_flip_is_exact_for_tiny_values(self):
        self.assertEqual(acc.sign_flip_p([1e-13] * 6), 1 / 64)
        self.assertEqual(acc.sign_flip_p([1e-13, -1e-13]), 3 / 4)

    def test_sign_flip_counts_ties(self):
        self.assertEqual(acc.sign_flip_p([0.0] * 6), 1.0)
        self.assertEqual(acc.sign_flip_p([0.5] * 6), 1 / 64)
        self.assertEqual(acc.sign_flip_p([0.5, -0.5]), 3 / 4)

    def test_choose_n(self):
        self.assertEqual(acc.choose_n(0.0), 6)
        s6 = math.log(1.05) * math.sqrt(6) / acc.T95[5]
        self.assertEqual(acc.choose_n(s6 * 0.999), 6)
        self.assertEqual(acc.choose_n(s6 * 1.001), 7)
        s10 = math.log(1.05) * math.sqrt(10) / acc.T95[9]
        self.assertEqual(acc.choose_n(s10 * 0.999), 10)
        self.assertIsNone(acc.choose_n(s10 * 1.001))

    def test_pairs_and_contrast(self):
        abba = [('A', 100.0), ('B', 110.0), ('B', 121.0), ('A', 110.0)]
        self.assertEqual(acc.pair_logs(abba), [math.log(1.1), math.log(1.1)])
        baab = [('B', 110.0), ('A', 100.0), ('A', 100.0), ('B', 105.0)]
        c = acc.couple_contrast(abba, baab)
        self.assertAlmostEqual(c, (3 * math.log(1.1) + math.log(1.05)) / 4)
        with self.assertRaises(acc.Refused):
            acc.pair_logs([('A', 1.0), ('A', 1.0), ('B', 1.0), ('A', 1.0)])

    def test_calibration_rules(self):
        cal = acc.calibrate([0.01, -0.01, 0.005, -0.004])
        self.assertTrue(cal['valid'])
        self.assertEqual(cal['n'], 6)
        self.assertTrue(cal['passed'])
        biased = acc.calibrate([0.05, 0.051, 0.049, 0.05])
        self.assertFalse(biased['valid'])
        self.assertFalse(biased['passed'])
        noisy = acc.calibrate([0.2, -0.2, 0.25, -0.25])
        self.assertTrue(noisy['valid'])
        self.assertIsNone(noisy['n'])
        self.assertFalse(noisy['passed'])

    def test_decision(self):
        accept = acc.decide([math.log(1.10)] * 5 + [math.log(1.11)])
        self.assertTrue(accept['accept'])
        self.assertEqual(accept['sign_flip_p'], 1 / 64)
        small = acc.decide([math.log(1.02)] * 6)
        self.assertFalse(small['interval_condition'])
        self.assertFalse(small['accept'])
        one_low = acc.decide([math.log(1.2)] * 5 + [math.log(1.0)])
        self.assertFalse(one_low['guard_condition'])


class ScheduleTests(unittest.TestCase):
    def bits(self, first=1):
        return [first, 0] * 32

    def test_clean_attempt_accepts(self):
        state = acc.new_state(self.bits())

        def rates(g, k):
            if g['phase'] == 'comparison':
                return {'A': 100.0, 'B': 110.0}
            return {'A': 100.0, 'B': 100.0}

        seen = drive(state, rates)
        self.assertEqual(seen[0], ('warmup', 'ABBA', None))
        cal = [s for s in seen if s[0] == 'calibration']
        self.assertEqual(len(cal), 8)
        self.assertEqual([s[1] for s in cal[:4]], ['ABBA', 'BAAB', 'BAAB', 'ABBA'])
        self.assertEqual(state['calibration']['n'], 6)
        self.assertEqual(len([s for s in seen if s[0] == 'comparison']), 12)
        self.assertEqual(state['outcome']['outcome'], 'accept')
        self.assertEqual(state['next_bit'], 10)

    def test_no_effect_is_a_result(self):
        state = acc.new_state(self.bits())
        drive(state, lambda g, k: {'A': 100.0, 'B': 100.0})
        self.assertEqual(state['outcome']['outcome'], 'not-accepted')

    def test_calibration_discards_rerun_then_stop(self):
        state = acc.new_state(self.bits())
        hits = []

        def contaminate(phase, k):
            if phase == 'calibration' and k in (2, 4, 6):
                hits.append(k)
                return (1,)
            return ()

        drive(state, lambda g, k: {'A': 100.0, 'B': 100.0}, contaminate)
        self.assertEqual(state['outcome']['outcome'], 'inconclusive')
        self.assertIn('discarded', state['outcome']['reason'])
        self.assertEqual(sum(c['discarded'] for c in state['calibration_couples']), 3)

    def test_two_discards_are_replaced(self):
        state = acc.new_state(self.bits())
        seen = drive(state, lambda g, k: {'A': 100.0, 'B': 100.0},
                     lambda phase, k: (0,) if phase == 'calibration' and k in (3, 5) else ())
        self.assertEqual(len([s for s in seen if s[0] == 'calibration']), 12)
        self.assertEqual(state['calibration']['n'], 6)
        self.assertEqual(state['outcome']['outcome'], 'not-accepted')

    def test_warmups(self):
        state = acc.new_state(self.bits())
        drive(state, lambda g, k: {'A': 1.0, 'B': 1.0}, lambda phase, k: (0,) if phase == 'warmup' else ())
        self.assertEqual(len(state['warmups']), 3)
        self.assertEqual(state['outcome']['outcome'], 'inconclusive')
        state = acc.new_state(self.bits())
        seen = drive(state, lambda g, k: {'A': 1.0, 'B': 1.0}, lambda phase, k: (0,) if k == 1 else ())
        self.assertEqual(seen[:2], [('warmup', 'ABBA', None), ('warmup', 'ABBA', None)])

    def test_contaminated_comparison_block_stops(self):
        state = acc.new_state(self.bits())
        seen = drive(state, lambda g, k: {'A': 100.0, 'B': 100.0},
                     lambda phase, k: (2,) if phase == 'comparison' else ())
        self.assertEqual(seen[-1][0], 'comparison')
        self.assertEqual(len([s for s in seen if s[0] == 'comparison']), 1)
        self.assertEqual(state['outcome']['reason'], 'a contaminated comparison block')

    def test_invalid_calibration_stops_before_v2(self):
        state = acc.new_state(self.bits())
        seen = drive(state, lambda g, k: {'A': 100.0, 'B': 103.0})
        self.assertFalse(any(s[0] == 'comparison' for s in seen))
        self.assertIn('validity', state['outcome']['reason'])

    def test_interruptions(self):
        state = acc.new_state(self.bits())
        g = acc.next_group(state)
        state['in_group'] = dict(g, index=0)
        acc.interrupted(state)
        self.assertTrue(state['warmups'][0]['contaminated'])
        self.assertIsNone(state['outcome'])
        acc.record_group(state, group('warmup', 'ABBA', None, {'A': 1.0, 'B': 1.0}, index=1))
        g = acc.next_group(state)
        self.assertEqual(g['phase'], 'calibration')
        state['in_group'] = dict(g, index=2)
        acc.interrupted(state)
        self.assertEqual(state['outcome']['outcome'], 'inconclusive')

    def test_bits_are_not_redrawn_on_resume(self):
        state = acc.new_state(self.bits())
        acc.record_group(state, group('warmup', 'ABBA', None, {'A': 1.0, 'B': 1.0}))
        g1 = acc.next_group(state)
        again = acc.next_group(json.loads(json.dumps(state)))
        self.assertEqual(g1, again)
        self.assertEqual(state['next_bit'], 1)
        acc.record_group(state, group(g1['phase'], g1['pattern'], g1['couple'], {'A': 1.0, 'B': 1.0}))
        self.assertTrue(acc.couple_open(state))


BASE = {
    'schema': 'mtg-kernel-native-expanded-training-run/v1',
    'initial_source': {'checkpoint': {'path': 'X:/parent.json', 'sha256': 'p' * 64},
                       'feature_transfer': {'expected_feature_contract_digest': 'c'},
                       'play_import': {'path': 'X:/imp.json', 'sha256': 'i' * 64}},
    'opponents': [{'id': 'parent', 'source': {'checkpoint': {'path': 'X:/parent.json', 'sha256': 'p' * 64},
                                              'play_import': {'path': 'X:/imp.json', 'sha256': 'i' * 64}}}],
    'iterations': [{'episodes': [{'episode': {'id': f'x{u}-{j}', 'seed': u * 10 + j, 'learner_seat': j % 2},
                                  'opponent': {'kind': 'fixed', 'id': 'parent'}} for j in range(3)]}
                   for u in range(30)],
    'learning_rate': 0.0001,
    'collection_workers': 1,
    'output_directory': '<OUTPUT>',
}


class ConfigTests(unittest.TestCase):
    def test_run_config_and_v2_config(self):
        start = {'path': 'S:/start.json', 'sha256': 's' * 64}
        cfg = acc.run_config(BASE, 'r2-s1-c', start, 0, 18, 4)
        self.assertEqual(list(cfg)[:len(BASE)], list(BASE))
        self.assertEqual(len(cfg['iterations']), 18)
        self.assertEqual(cfg['initial_source']['checkpoint'], start)
        self.assertEqual(cfg['initial_source']['feature_transfer'], BASE['initial_source']['feature_transfer'])
        self.assertTrue(cfg['record_process_cpu'])
        self.assertEqual(cfg['collection_workers'], 4)
        ep = cfg['iterations'][3]['episodes'][1]['episode']
        self.assertEqual(ep['id'], 'ckpt-v2-acceptance-r2-s1-c-u03-e01')
        self.assertEqual(ep['seed'], acc.episode_seed('r2-s1-c', 3, 1))
        self.assertLess(ep['seed'], 2 ** 63)
        other = acc.run_config(BASE, 'r2-s2-c', start, 0, 18, 4)
        self.assertNotEqual(other['iterations'][3]['episodes'][1]['episode']['seed'], ep['seed'])
        self.assertEqual(BASE['iterations'][3]['episodes'][1]['episode']['id'], 'x3-1')
        container = {'path': 'P:/parent.ckpt', 'sha256': 'q' * 64}
        cfg2 = acc.v2_config(cfg, {'p' * 64: container})
        self.assertEqual(cfg2['record_format'], 'v2')
        self.assertEqual(cfg2['opponents'][0]['source']['checkpoint'], container)
        stripped = dict(cfg2)
        del stripped['record_format']
        stripped['opponents'] = cfg['opponents']
        self.assertEqual(stripped, cfg)
        with self.assertRaises(acc.Refused):
            acc.v2_config(cfg, {})

    def test_prepare_validation(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            parent = tmp / 'in' / 'parent.json'
            parent.parent.mkdir()
            parent.write_text(json.dumps({'first_moments': [1], 'second_moments': [1]}))
            imp = tmp / 'in' / 'imp.json'
            imp.write_text('{}')
            base = json.loads(json.dumps(BASE))
            base['opponents'][0]['source'] = {'checkpoint': {'path': 'X:/parent.json', 'sha256': acc.sha256_file(parent)},
                                              'play_import': {'path': 'X:/imp.json', 'sha256': acc.sha256_file(imp)}}
            base['initial_source']['play_import'] = base['opponents'][0]['source']['play_import']
            base_path = tmp / 'base.json'
            base_path.write_text(json.dumps(base))
            exe = tmp / 'bin.exe'
            exe.write_bytes(b'x')
            spec_doc = {'binary': acc.pin(exe), 'converter': acc.pin(exe), 'bases': {'c': acc.pin(base_path)},
                        'path_map': [['X:/', (tmp / 'in').as_posix() + '/']],
                        'starts': [{'run': 'a', 'base': 'c', 'checkpoint': acc.pin(parent)},
                                   {'run': 'b', 'base': 'c', 'checkpoint': acc.pin(parent)}],
                        'updates': 4}
            spec_path = tmp / 'spec.json'
            spec_path.write_text(json.dumps(spec_doc))
            fake = {'container': {'path': 'C:/x.ckpt', 'sha256': 'f' * 64}, 'receipt': {}, 'state_sha256': 'z'}
            with mock.patch.object(acc, 'convert_opponent', return_value=fake):
                with self.assertRaises(acc.Refused):
                    acc.prepare(spec_path, tmp / 'root')
                manifest = acc.prepare(spec_path, tmp / 'root', validation=True)
            self.assertEqual(manifest['runs'], ['a', 'b'])
            cfg2 = acc.read_json(manifest['configs']['a']['v2']['path'])
            self.assertEqual(cfg2['opponents'][0]['source']['checkpoint'], fake['container'])
            self.assertEqual(len(cfg2['iterations']), 4)


class RetentionTests(unittest.TestCase):
    def make_run(self, native, updates, complete=None):
        complete = updates if complete is None else complete
        for i in range(updates):
            att = native / 'iterations' / f'{i:06d}' / 'attempt-000000'
            (att / 'collect').mkdir(parents=True)
            (att / 'update').mkdir(parents=True)
            (att / 'update' / 'checkpoint.json').write_text('c' * 100)
            (att / 'collect' / 'non-natural.json').write_text('{}')
            if i >= complete:
                (att / 'collect' / 'episode-x.json').write_text('e')
                continue
            traj = att / 'collect' / 'episode-0.json'
            traj.write_text('e' * 100)
            (att / 'collect' / 'collection.json').write_text(json.dumps({'trajectories': [{'path': str(traj)}]}))
            (att / 'update' / 'update.json').write_text(json.dumps(
                {'checkpoint': {'path': str(att / 'update' / 'checkpoint.json')}}))
            (native / 'iterations' / f'{i:06d}' / 'complete.json').write_text(json.dumps(
                {'update': {'path': str(att / 'update' / 'update.json')}}))

    def test_partial_group_keeps_latest_published_and_receipts(self):
        with tempfile.TemporaryDirectory() as tmp:
            g = Path(tmp) / 'g000'
            self.make_run(g / 'b1-A-v1' / 'r' / 'native', 3)
            self.make_run(g / 'b2-B-v1' / 'r' / 'native', 3, complete=1)
            acc.prune_group(g, ['r'], 3, Path(tmp) / 'p.jsonl')
            kept = sorted(p.relative_to(g).as_posix() for p in g.rglob('checkpoint.json'))
            self.assertEqual(kept, ['b2-B-v1/r/native/iterations/000000/attempt-000000/update/checkpoint.json'])
            self.assertEqual(len(list(g.rglob('non-natural.json'))), 6)
            self.assertEqual(len(list(g.rglob('episode-x.json'))), 2)
            self.assertEqual(list(g.rglob('episode-0.json')), [])

    def test_move_is_restartable(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            src = tmp / 'hot' / 'g000'
            (src / 'a').mkdir(parents=True)
            (src / 'a' / 'f.json').write_text('x' * 10)
            dest_root = tmp / 'art'
            staging = dest_root / 'g000.partial'
            staging.mkdir(parents=True)
            (staging / 'junk').write_text('j')
            acc.move_retained(src, dest_root, tmp / 'm.jsonl')
            self.assertFalse(src.exists())
            self.assertFalse(staging.exists())
            self.assertEqual(acc.tree_manifest(dest_root / 'g000'), [('a/f.json', 10)])
            # Interrupted after publication, part way through deleting the source.
            src2 = tmp / 'hot' / 'g001'
            (src2 / 'a').mkdir(parents=True)
            (src2 / 'a' / 'f.json').write_text('y')
            (src2 / 'b.json').write_text('z')
            real_rmtree = acc.shutil.rmtree
            with mock.patch.object(acc.shutil, 'rmtree', side_effect=OSError('interrupted')):
                with self.assertRaises(OSError):
                    acc.move_retained(src2, dest_root, tmp / 'm.jsonl')
            (src2 / 'b.json').unlink()
            acc.move_retained(src2, dest_root, tmp / 'm.jsonl')
            self.assertFalse(src2.exists())
            self.assertEqual(acc.tree_manifest(dest_root / 'g001'), [('a/f.json', 1), ('b.json', 1)])
            # Interrupted after deleting the source, before logging.
            acc.move_retained(tmp / 'hot' / 'g001', dest_root, tmp / 'm.jsonl')
            # A destination without its verified manifest is refused.
            (dest_root / 'g002').mkdir()
            with self.assertRaises(acc.Refused):
                acc.move_retained(tmp / 'hot' / 'g002', dest_root, tmp / 'm.jsonl')
            self.assertIs(acc.shutil.rmtree, real_rmtree)

    def test_correspondence_and_formal_placement(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'summary.json'
            path.write_text(json.dumps({'ok': True, 'A': True, 'B': True, 'conv': True, 'V7': True, 'V8': True}))
            self.assertTrue(acc.correspondence_passed(path))
            path.write_text(json.dumps({'ok': True, 'A': True, 'B': True, 'conv': True, 'V7': True}))
            self.assertFalse(acc.correspondence_passed(path))
            path.write_text('{}')
            self.assertFalse(acc.correspondence_passed(path))
        args = argparse.Namespace(precondition_seconds=acc.PRECONDITION_SECONDS, settle_seconds=acc.SETTLE_SECONDS,
                                  pool='4-23', run_disk='D', artifact_disk='E', root='D:/x', artifact_root='E:/y')
        # Amendment 2: exactly 4-23. The design's 0-19, a subset, a superset and 'own' are refused.
        for change in ({'artifact_disk': ''}, {'pool': '0-19'}, {'pool': '0-15'}, {'pool': '4-22'}, {'pool': '0-23'},
                       {'pool': '3-23'}, {'pool': 'own'}, {'settle_seconds': 1.0}, {'artifact_root': 'D:/y'}):
            with self.assertRaises(acc.Refused):
                acc.formal_placement(argparse.Namespace(**dict(vars(args), **change)))
        if os.name == 'nt':
            acc.formal_placement(args)


class CountersTests(unittest.TestCase):
    HEADER = ['(PDH-CSV 4.0)', r'\\H\PhysicalDisk(0 C:)\% Idle Time', r'\\H\PhysicalDisk(1 E:)\% Idle Time',
              r'\\H\PhysicalDisk(2 D:)\% Idle Time', r'\\H\PhysicalDisk(_Total)\% Idle Time',
              r'\\H\PhysicalDisk(0 C:)\Disk Read Bytes/sec', r'\\H\PhysicalDisk(1 E:)\Disk Read Bytes/sec',
              r'\\H\PhysicalDisk(2 D:)\Disk Read Bytes/sec']

    def test_typeperf_parsing(self):
        self.assertEqual(acc.disk_columns(self.HEADER, 'D'), (self.HEADER[3], self.HEADER[7]))
        self.assertEqual(acc.disk_columns(self.HEADER, 'E')[0], self.HEADER[2])
        self.assertEqual(acc.disk_columns(self.HEADER, ''), (None, None))
        line = '"11/03/2026 07:36:15.927","95.5","40.0","99.7","98.4","1.0","2.0","3000000.5"\n'
        at, values = acc.parse_typeperf_line(line, self.HEADER)
        self.assertAlmostEqual(at % 1, 0.927, places=3)
        self.assertEqual(values[self.HEADER[7]], 3000000.5)
        self.assertIsNone(acc.parse_typeperf_line('Exiting, please wait...', self.HEADER))

    def test_typeperf_row_width(self):
        # A host's typeperf left a counter out of the header while rows kept its value: refuse, do not drop rows.
        acc.check_typeperf_row_width('"11/03/2026 07:36:15.927","95.5","40.0","99.7","98.4","1.0","2.0","3.5"\n',
                                     self.HEADER)
        acc.check_typeperf_row_width('Exiting, please wait...\n', self.HEADER)
        wide = '"11/03/2026 07:36:15.927","95.5","40.0","99.7","98.4","1.0","2.0","3.5","12.0"'
        with self.assertRaises(acc.Refused):
            acc.check_typeperf_row_width(wide + '\n', self.HEADER)
        # A final fragment cut by terminate (no newline) is not a complete row: no refusal.
        acc.check_typeperf_row_width('"11/03/2026 07:36:20.9', self.HEADER)
        acc.check_typeperf_row_width(wide, self.HEADER)
        self.assertNotIn(r'\Processor Information(_Total)\% Processor Time', acc.Sampler.COUNTERS)

    def test_contamination(self):
        rows = [(t, 10e6, 10.0) for t in range(0, 100, 5)]
        stats = acc.window_stats(rows, 0, 100, 5)
        claims = acc.claim_stats([(t, [], True) for t in range(0, 100, 15)], 0, 100)
        self.assertEqual(acc.block_contamination(stats, claims, None), [])
        # Amendment 1: the relative threshold is twice the larger of the baseline and the 10 MB/s floor.
        self.assertEqual(acc.block_contamination(stats, claims, 4e6), [])
        self.assertEqual(acc.block_contamination(stats, claims, 0.12e6), [])
        self.assertEqual(acc.relative_read_threshold(0.12e6), 20e6)
        self.assertEqual(acc.relative_read_threshold(15e6), 30e6)
        reads21 = acc.window_stats([(t, 21e6, 10.0) for t in range(0, 100, 5)], 0, 100, 5)
        self.assertEqual(len(acc.block_contamination(reads21, claims, 4e6)), 1)
        self.assertEqual(acc.block_contamination(reads21, claims, 15e6), [])
        sparse = acc.window_stats(rows[:17], 0, 100, 5)
        self.assertTrue(acc.block_contamination(sparse, claims, None))
        busy = acc.window_stats([(t, 10e6, 60.0) for t in range(0, 100, 5)], 0, 100, 5)
        self.assertTrue(acc.block_contamination(busy, claims, None))
        self.assertEqual(acc.block_contamination(busy, claims, None, artifact_disk=False), [])
        reads = acc.window_stats([(t, 61e6, 0.0) for t in range(0, 100, 5)], 0, 100, 5)
        self.assertTrue(acc.block_contamination(reads, claims, None))
        gap = acc.claim_stats([(0, [], True), (90, [], True)], 0, 100)
        self.assertTrue(acc.block_contamination(stats, gap, None))
        overlap = acc.claim_stats([(t, [{'lane': 'x'}] if t == 30 else [], True) for t in range(0, 100, 15)], 0, 100)
        self.assertTrue(acc.block_contamination(stats, overlap, None))

    def test_missing_counters_and_start_window(self):
        rows = [(t, None, None) for t in range(0, 100, 5)]
        stats = acc.window_stats(rows, 0, 100, 5)
        self.assertEqual(stats['missing_fraction'], 1.0)
        self.assertTrue(acc.block_contamination(stats, acc.claim_stats([(0, [], True)], 0, 50), None))
        no_e = acc.window_stats([(t, 1e6, None) for t in range(0, 100, 5)], 0, 100, 5)
        self.assertEqual(no_e['missing_fraction'], 1.0)
        self.assertEqual(acc.window_stats([(t, 1e6, None) for t in range(0, 100, 5)], 0, 100, 5,
                                          artifact_disk=False)['missing_fraction'], 0.0)
        self.assertAlmostEqual(acc.window_stats([(t, 1e6, 1.0) for t in range(22)], 0, 125, 5)['missing_fraction'],
                               0.12)
        spike = [(t, 1e6, 1.0) for t in range(0, 600, 5)]
        spike[-1] = (595, 1e6, 100.0)
        stats = acc.window_stats(spike, 0, 600, 5)
        self.assertEqual(acc.block_contamination(stats, acc.claim_stats([(0, [], True)], 0, 50), None), [])
        self.assertTrue(acc.start_window_failures(stats))
        self.assertEqual(acc.start_window_failures(stats, artifact_disk=False), [])
        reads = [(t, 1e6, 1.0) for t in range(0, 600, 5)]
        reads[3] = (15, 61e6, 1.0)
        self.assertTrue(acc.start_window_failures(acc.window_stats(reads, 0, 600, 5)))

    def test_counters_are_summarized_separately(self):
        rows = [(t, 1e6, 49.0) for t in range(90)] + [(t, None, 100.0) for t in range(90, 100)]
        stats = acc.window_stats(rows, 0, 100, 1)
        self.assertAlmostEqual(stats['e_busy_mean'], 54.1)
        self.assertAlmostEqual(stats['missing_fraction'], 0.10)
        self.assertTrue(acc.block_contamination(stats, acc.claim_stats([(0, [], True)], 0, 50), None))
        spike = [(t, 1e6, 1.0) for t in range(100)] + [(100, 90e6, None)]
        self.assertTrue(acc.start_window_failures(acc.window_stats(spike, 0, 101, 1)))
        self.assertEqual(acc.window_stats([(0, float('nan'), float('inf'))], 0, 1, 1)['rows'], 0)

    def test_own_claim(self):
        status = {'claims': [{'id': 'm', 'pid': 7, 'cores': list(range(20))}, {'id': 'x', 'pid': 8, 'cores': [21]}]}
        self.assertEqual(acc.own_claim(status, {7}), ('m', set(range(20))))
        self.assertIsNone(acc.own_claim(status, {9}))
        lost = acc.claim_stats([(0, [], True), (15, [], False)], 0, 30)
        self.assertTrue(lost['own_claim_lost'])
        self.assertIn('the measurement lost its own claim', acc.block_contamination(
            acc.window_stats([(t, 1.0, 1.0) for t in range(30)], 0, 30, 1), lost, None))
        failed_poll = acc.claim_stats([(0, None, True), (15, [], True)], 0, 30)
        self.assertFalse(failed_poll['own_claim_lost'])

    def test_foreign_claims(self):
        status = {'claims': [{'pid': 1, 'cores': [0, 1]}, {'pid': 2, 'cores': [20, 21]},
                             {'pid': 3, 'cores': [19, 20], 'lane': 'ex1'}],
                  'timed': {'held': False, 'cores': []}}
        pool = acc.parse_cores('0-19')
        self.assertEqual([c['pid'] for c in acc.foreign_claims(status, pool, {1})], [3])
        status['timed'] = {'held': True, 'cores': None}
        self.assertEqual(len(acc.foreign_claims(status, pool, {1, 3})), 1)

    def test_amendment_2_pool(self):
        # Claims on 0-3 (a GPU campaign beside the measurement) are outside the amended pool; any core of 4-23 is not.
        self.assertEqual(acc.parse_cores(acc.POOL_CORES), set(range(4, 24)))
        pool = acc.parse_cores(acc.POOL_CORES)
        status = {'claims': [{'pid': 7, 'cores': [0, 1]}, {'pid': 8, 'cores': [2, 3]},
                             {'pid': 1, 'cores': list(range(4, 24))}],
                  'timed': {'held': False, 'cores': []}}
        self.assertEqual(acc.foreign_claims(status, pool, {1}), [])
        for cores in ([3, 4], [23], [12]):
            status['claims'].append({'pid': 9, 'cores': cores})
            self.assertEqual([c['pid'] for c in acc.foreign_claims(status, pool, {1})], [9])
            status['claims'].pop()
        status['timed'] = {'held': True, 'cores': [0, 1, 2, 3]}
        self.assertEqual(acc.foreign_claims(status, pool, {1}), [])
        status['timed'] = {'held': True, 'cores': [3, 4]}
        self.assertEqual(len(acc.foreign_claims(status, pool, {1})), 1)


FAKE_BINARY = r'''
import hashlib, json, os, sys, time
cfg = json.load(open(sys.argv[1]))
out = cfg['output_directory']
v2 = cfg.get('record_format') == 'v2'
diverge = v2 and os.environ.get('FAKE_DIVERGE') == '1'
wall = 1.0 if v2 else 1.1
t0 = time.time() - 100
start = cfg['initial_source']['checkpoint']['sha256']
for i, it in enumerate(cfg['iterations']):
    att = os.path.join(out, 'iterations', '%06d' % i, 'attempt-000000')
    os.makedirs(os.path.join(att, 'collect'))
    os.makedirs(os.path.join(att, 'update'))
    traj = os.path.join(att, 'collect', 'episode-0.json')
    open(traj, 'w').write('x' * 1000)
    coll = os.path.join(att, 'collect', 'collection.json')
    json.dump({'trajectories': [{'path': traj, 'sha256': 'x'}]}, open(coll, 'w'))
    ck = os.path.join(att, 'update', 'checkpoint.json')
    open(ck, 'wb').write((b'MTGCKPT\x00' if v2 else b'{') + b'0' * 500)
    state = hashlib.sha256((start + str(i) + ('d' if diverge else '')).encode()).hexdigest()
    upd = os.path.join(att, 'update', 'update.json')
    json.dump({'after_state_sha256': state, 'checkpoint': {'path': ck, 'sha256': 'x'}}, open(upd, 'w'))
    comp = os.path.join(out, 'iterations', '%06d' % i, 'complete.json')
    json.dump({'update': {'path': upd}, 'collection': {'path': coll}, 'driver_process_cpu_seconds': 2.0 * i},
              open(comp, 'w'))
    os.utime(comp, (t0 + wall * i, t0 + wall * i))
json.dump({}, open(os.path.join(out, 'completion.json'), 'w'))
'''


class FakeSamples:
    interval = 1.0

    def window(self, start, end):
        return [(t, 5e6, 5.0) for t in range(int(start) + 1, int(end))] or [(start, 5e6, 5.0)]


class FakeWatch:
    error = None

    def __init__(self):
        self.stopping = threading.Event()

    def start(self):
        pass

    def stop(self):
        pass

    def is_alive(self):
        return True

    def window(self, start, end):
        return [(start + k, [], True) for k in range(max(1, int(end - start)))]


class FakeMeasurement(acc.Measurement):
    def __init__(self, *a, **k):
        super().__init__(*a, **k)
        self.samples = FakeSamples()

    def make_watchers(self):
        return FakeWatch(), FakeWatch()


class EndToEndTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        script = self.root / 'fake_binary.py'
        script.write_text(FAKE_BINARY)
        prepared = self.root / 'm' / 'prepared'
        configs = {}
        for run in ('a', 'b'):
            cfg = acc.run_config(BASE, run, {'path': 'S:/s.json', 'sha256': run * 64}, 0, 3, 1)
            configs[run] = {'v1': acc.write_json(prepared / f'{run}-v1.json', cfg),
                            'v2': acc.write_json(prepared / f'{run}-v2.json', dict(cfg, record_format='v2'))}
        self.prepared = {'validation': True, 'runs': ['a', 'b'], 'updates': 3, 'configs': configs,
                         'binary': acc.pin(script), 'correspondence_receipt': None}
        acc.write_json(prepared / 'PREPARED.json', self.prepared)
        self.args = argparse.Namespace(pool='0-3', run_disk='C', artifact_disk='', keep_free_gib=0.0,
                                       precondition_seconds=0.0, settle_seconds=0.0, host_slots='unused',
                                       artifact_root=str(self.root / 'art'))
        self.patches = [mock.patch.object(acc, 'argv_for', lambda b, c: [sys.executable, str(b), str(c)]),
                        mock.patch.object(acc, 'STAGGER_SECONDS', 0.0)]
        for p in self.patches:
            p.start()

    def tearDown(self):
        for p in self.patches:
            p.stop()
        self.tmp.cleanup()

    def test_accepts_a_faster_v2_and_retains_by_rule(self):
        state = FakeMeasurement(self.root / 'm', 1, self.args, self.prepared).execute()
        self.assertEqual(state['outcome']['outcome'], 'accept')
        self.assertAlmostEqual(state['decision']['geometric_mean_ratio'], 1.1)
        self.assertEqual(state['calibration']['sd'], 0.0)
        result = acc.read_json(self.root / 'm' / 'attempt-001' / 'RESULT.json')
        self.assertFalse(result['decisive'])
        manifest = acc.read_json(self.root / 'm' / 'attempt-001' / 'MANIFEST.json')
        self.assertEqual(len(manifest['order_bits']), 64)
        self.assertEqual(manifest['calibration']['n'], 6)
        self.assertEqual(manifest['calibration']['d_read_baseline'], 5e6)
        self.assertEqual(manifest['calibration']['relative_read_threshold'], 20e6)
        groups = sorted((self.root / 'art' / 'attempt-001' / 'groups').iterdir())
        self.assertEqual(len(groups), 21)
        self.assertTrue(all(g.is_dir() for g in groups))
        manifests = sorted((self.root / 'art' / 'attempt-001' / 'groups-manifests').iterdir())
        self.assertEqual([m.name for m in manifests], [g.name + '.json' for g in groups])
        self.assertFalse((self.root / 'm' / 'attempt-001' / 'groups').exists() and
                         any((self.root / 'm' / 'attempt-001' / 'groups').iterdir()))
        last = groups[-1]
        kept = sorted(p.relative_to(last).as_posix() for p in last.rglob('checkpoint.json'))
        self.assertEqual(kept, ['b4-' + last.name[-1] + '-' + ('v2' if last.name[-1] == 'B' else 'v1')
                                + f'/{r}/native/iterations/000002/attempt-000000/update/checkpoint.json'
                                for r in ('a', 'b')])
        self.assertEqual(list(last.rglob('episode-0.json')), [])
        self.assertTrue(list(last.rglob('complete.json')))
        with self.assertRaises(acc.Refused):
            FakeMeasurement(self.root / 'm', 1, self.args, self.prepared).execute()

    def test_divergent_states_void_the_attempt(self):
        with mock.patch.dict(os.environ, {'FAKE_DIVERGE': '1'}):
            state = FakeMeasurement(self.root / 'm', 1, self.args, self.prepared).execute()
        self.assertEqual(state['outcome']['outcome'], 'void')
        self.assertEqual(state['outcome']['runs'], ['a', 'b'])

    def test_failed_run_in_calibration_is_inconclusive(self):
        m = FakeMeasurement(self.root / 'm', 1, self.args, self.prepared)
        calls = {'n': 0}
        real = acc.launch_block

        def flaky(*a, **k):
            calls['n'] += 1
            codes = real(*a, **k)
            return [1] + codes[1:] if calls['n'] == 6 else codes

        with mock.patch.object(acc, 'launch_block', flaky):
            state = m.execute()
        self.assertEqual(state['outcome']['outcome'], 'inconclusive')
        self.assertIn('calibration', state['outcome']['reason'])

    def test_terminal_state_without_result_is_published(self):
        m = FakeMeasurement(self.root / 'm', 1, self.args, self.prepared)
        m.dir.mkdir(parents=True)
        state = acc.new_state([1] * 64)
        acc.finish(state, 'inconclusive', 'test')
        acc.write_json(m.manifest_path, {'schema': acc.SCHEMA})
        acc.write_json(m.state_path, state)
        out = m.execute()
        self.assertEqual(out['outcome']['reason'], 'test')
        self.assertTrue((m.dir / 'RESULT.json').exists())
        with self.assertRaises(acc.Refused):
            FakeMeasurement(self.root / 'm', 1, self.args, self.prepared).execute()

    def test_pending_retention_is_completed_on_restart(self):
        m = FakeMeasurement(self.root / 'm', 1, self.args, self.prepared)
        g = m.dir / 'groups' / 'g000-warmup-ABBA'
        (g / 'b1-A-v1').mkdir(parents=True)
        state = acc.new_state([1] * 64)
        acc.record_group(state, group('warmup', 'ABBA', None, {'A': 1.0, 'B': 1.0}))
        state['pending_retention'] = g.as_posix()
        acc.write_json(m.manifest_path, {'schema': acc.SCHEMA})
        acc.write_json(m.state_path, state)
        acc.finish(state, 'inconclusive', 'stop here')
        acc.write_json(m.state_path, state)
        m.execute()
        self.assertTrue((self.root / 'art' / 'attempt-001' / 'groups' / 'g000-warmup-ABBA').is_dir())
        self.assertFalse(g.exists())

    def test_second_attempt_only_after_inconclusive(self):
        with self.assertRaises(acc.Refused):
            acc.attempt_allowed(self.root / 'm', 2)
        acc.write_json(self.root / 'm' / 'attempt-001' / 'RESULT.json', {'outcome': {'outcome': 'not-accepted'}})
        with self.assertRaises(acc.Refused):
            acc.attempt_allowed(self.root / 'm', 2)
        acc.write_json(self.root / 'm' / 'attempt-001' / 'RESULT.json', {'outcome': {'outcome': 'inconclusive'}})
        acc.attempt_allowed(self.root / 'm', 2)
        with self.assertRaises(acc.Refused):
            acc.attempt_allowed(self.root / 'm', 3)


if __name__ == '__main__':
    unittest.main()
