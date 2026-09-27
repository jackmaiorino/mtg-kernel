"""Offline launcher tests with a fake native evaluator; no game engine or GPU work."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import g115_line_a_guard_v1 as guard
import g115_line_a_windows_launch_v1 as launcher
from g115_d4_audit_storage_v1 import Ledger

FAKE = HERE / 'fixtures_line_a_fake_evaluator_v1.py'
PREFIX = [sys.executable, '-B', str(FAKE)]
GIT_HEAD = 'f' * 40
LEARNER = dict(kind='legacy', source=dict(checkpoint=dict(path='E:/g115.json', sha256='a' * 64)),
               v3_forced_actions=False)
MEMBERS = {'v3': dict(kind='legacy', source=dict(checkpoint=None), v3_forced_actions=True),
           'g115': LEARNER}


def deck(label):
    return dict(label=label, mainboard=[sum(label.encode()) % 50] * 60, sideboard=[1] * 15)


PACKET = dict(decks={d: deck(d) for d in ('Affinity', 'Burn', 'Elves', 'Faeries', 'Rally', 'Terror', 'Wildfire')},
              postboard={'%s|%s' % (x, y): deck(x) for x in ('Affinity', 'Burn', 'Elves', 'Faeries', 'Rally',
                                                              'Terror', 'Wildfire')
                         for y in ('Affinity', 'Burn', 'Elves', 'Faeries', 'Rally', 'Terror', 'Wildfire')})


def jobs(n=4):
    return [dict(id='q%02d' % k, member=('v3', 'g115')[k % 2], learner_deck='Burn', opponent_deck='Rally',
                 learner_seat=(k // 2) % 2, seed=1000 + k, block=k // 4) for k in range(n)]


def environment_with(**extra):
    base = launcher.Pool.environment

    def environment(self):
        env = base(self)
        env.update(extra)
        return env
    return environment


class RequestAndHashTests(unittest.TestCase):
    def test_learner_sits_at_its_seat_with_one_bo3(self):
        job = dict(learner_deck='Burn', opponent_deck='Rally', learner_seat=1, seed=5)
        request = launcher.evaluation_request(job, PACKET, LEARNER, MEMBERS['v3'], 'D:/out/x')
        self.assertEqual(request['sources'], [MEMBERS['v3'], LEARNER])
        self.assertEqual(len(request['matches']), 1)
        self.assertEqual(request['matches'][0]['config']['deck_ids'], ['Rally', 'Burn'])
        self.assertTrue(request['cross_generation_evaluation'])

    def test_match_is_checked_by_hash_and_refused_on_any_mismatch(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory) / 'out'
            request = launcher.evaluation_request(jobs(1)[0], PACKET, LEARNER, MEMBERS['v3'], folder)
            request_path = Path(directory) / 'request.json'
            request_path.write_text(json.dumps(request))
            subprocess.run(PREFIX + [str(request_path)], check=True)
            digest = launcher.match_hash(folder, request, GIT_HEAD)
            self.assertEqual(digest, guard.sha256_file(folder / 'match-000000.json'))
            with self.assertRaisesRegex(ValueError, 'build commit differs'):
                launcher.match_hash(folder, request, '0' * 40)
            with self.assertRaisesRegex(ValueError, 'start differs'):
                launcher.match_hash(folder, dict(request, capture_decisions=True), GIT_HEAD)
            (folder / 'match-000000.json').write_bytes(b'{}')
            with self.assertRaisesRegex(ValueError, 'differs from its completion hash'):
                launcher.match_hash(folder, request, GIT_HEAD)
            (folder / 'failure.json').write_text('{}')
            with self.assertRaisesRegex(ValueError, 'failure record exists'):
                launcher.match_hash(folder, request, GIT_HEAD)


class PoolTests(unittest.TestCase):
    def pool(self, root, workers=2, cap=10**9, reservation=10**6):
        ledger = Ledger(cap, 1)
        return launcher.Pool('unused.exe', GIT_HEAD, root, workers, ledger, reservation, 60, root,
                             prefix=PREFIX), ledger

    def items(self, root, n=4):
        (Path(root) / 'native').mkdir(parents=True, exist_ok=True)
        return [(job, launcher.evaluation_request(job, PACKET, LEARNER, MEMBERS[job['member']],
                                                  Path(root) / 'native' / job['id'])) for job in jobs(n)]

    def test_jobs_complete_in_fresh_directories_and_seal_their_bytes(self):
        with tempfile.TemporaryDirectory() as directory, \
                patch.object(guard, 'DISK_RESERVE_BYTES', 0):
            pool, ledger = self.pool(directory)
            rows, seconds = pool.map(self.items(directory))
            self.assertTrue(all(row['complete'] for row in rows))
            self.assertEqual(ledger.pending, {})
            self.assertEqual(ledger.committed, 1 + sum(row['charged_bytes'] for row in rows))  # plus control allowance
            self.assertTrue(all(row['charged_bytes'] > 0 for row in rows))
            self.assertEqual(len(list((Path(directory) / 'executions').glob('*.json'))), 4)
            rerun, _ = self.pool(directory)
            again, _ = rerun.map(self.items(directory))
            self.assertFalse(again[0]['complete'])  # never reuse an attempt
            self.assertIn('already exists', again[0]['error'])

    def test_first_failure_stops_dispatch_and_is_retained(self):
        with tempfile.TemporaryDirectory() as directory, \
                patch.object(guard, 'DISK_RESERVE_BYTES', 0), \
                patch.object(launcher.Pool, 'environment', environment_with(FAKE_EVALUATOR_FAIL_SEED='1000')):
            pool, ledger = self.pool(directory, workers=1)
            rows, _ = pool.map(self.items(directory))
            self.assertFalse(rows[0]['complete'])
            self.assertIn('Native match failed', rows[0]['error'])
            self.assertTrue(all('Pool stopped' in row.get('error', '') for row in rows[1:]))
            self.assertTrue(ledger.stopped)
            self.assertTrue((Path(directory) / 'native' / 'q00' / 'failure.json').exists())

    def test_byte_reservation_refuses_past_the_cap(self):
        with tempfile.TemporaryDirectory() as directory, \
                patch.object(guard, 'DISK_RESERVE_BYTES', 0):
            pool, ledger = self.pool(directory, workers=1, cap=10**6, reservation=2 * 10**6)
            rows, _ = pool.map(self.items(directory))
            self.assertFalse(rows[0]['complete'])
            self.assertIn('cap unavailable', rows[0]['error'])
            self.assertFalse((Path(directory) / 'native' / 'q00').exists())
            self.assertTrue(all('Pool stopped' in row['error'] for row in rows[1:]))


class ThroughputModeTests(unittest.TestCase):
    def launch(self, directory, worker_counts=(1, 2), n=4):
        binary = Path(directory) / 'public_feature_evaluation_v1.exe'
        binary.write_bytes(b'stand-in')
        pinned, digest = guard.pin_binary(binary, Path(directory) / 'pinned')
        return dict(schema=launcher.LAUNCH_SCHEMA, mode='throughput',
                    executable=dict(path=str(pinned), sha256=digest, git_head=GIT_HEAD),
                    binding=dict(launcher_sha256='1' * 64, executable_sha256=digest, data_tree_sha256='3' * 64),
                    deck_packet=PACKET, learner_source=LEARNER, member_sources=MEMBERS,
                    throughput=dict(jobs=jobs(n), worker_counts=list(worker_counts), work_class='bo3-ordinary',
                                    cap_bytes=10**9, control_allowance_bytes=1, reservation_bytes=10**6,
                                    job_timeout_seconds=60)), pinned

    def run_throughput(self, launch, root, **env):
        with patch.object(guard, 'DISK_RESERVE_BYTES', 0), patch.object(guard, 'PINNED_ROOT', str(Path(launch['executable']['path']).parents[1])), \
                patch.object(launcher.Pool, 'environment', environment_with(**env)):
            return launcher.throughput(launch, 'jack', root, now=1.0, prefix=PREFIX)

    def test_serial_and_parallel_phases_reproduce_the_same_match_hashes(self):
        with tempfile.TemporaryDirectory() as directory:
            launch, _ = self.launch(directory)
            record = self.run_throughput(launch, Path(directory) / 'run')
            self.assertEqual([p['workers'] for p in record['phases']], [1, 2])
            self.assertEqual(record['phases'][0]['rows'], record['phases'][1]['rows'])
            self.assertTrue(all(p['completed'] == 4 and p['seconds'] > 0 for p in record['phases']))
            self.assertEqual(json.loads((Path(directory) / 'run' / 'host-throughput.json').read_text()), record)

    def test_nondeterministic_bytes_across_worker_counts_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            launch, _ = self.launch(directory)
            original = launcher.Pool.environment
            calls = []

            def varying(self):
                env = original(self)
                env['FAKE_EVALUATOR_SALT'] = str(self.workers)
                calls.append(self.workers)
                return env
            with patch.object(guard, 'DISK_RESERVE_BYTES', 0), \
                    patch.object(guard, 'PINNED_ROOT', str(Path(launch['executable']['path']).parents[1])), \
                    patch.object(launcher.Pool, 'environment', varying):
                with self.assertRaisesRegex(ValueError, 'Worker count 2 changed match bytes'):
                    launcher.throughput(launch, 'jack', Path(directory) / 'run', now=1.0, prefix=PREFIX)

    def test_timing_check_stays_small_and_serial_first(self):
        with tempfile.TemporaryDirectory() as directory:
            launch, _ = self.launch(directory, n=65)
            with self.assertRaisesRegex(ValueError, 'limited to 64 BO3'):
                self.run_throughput(launch, Path(directory) / 'run')
            launch, _ = self.launch(directory, worker_counts=(2, 8))
            with self.assertRaisesRegex(ValueError, 'Serial first'):
                self.run_throughput(launch, Path(directory) / 'run2')

    def test_executable_must_be_the_pinned_copy(self):
        with tempfile.TemporaryDirectory() as directory:
            launch, pinned = self.launch(directory)
            launch['executable']['path'] = str(Path(directory) / 'public_feature_evaluation_v1.exe')
            with patch.object(guard, 'PINNED_ROOT', str(pinned.parents[1])):
                with self.assertRaisesRegex(ValueError, 'pinned copy'):
                    launcher.throughput(launch, 'jack', Path(directory) / 'run', now=1.0, prefix=PREFIX)


if __name__ == '__main__':
    unittest.main()
