"""Offline launcher tests with a fake native evaluator; no game engine or GPU work."""
import copy
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
            return launcher.throughput(launch, 'desktop', root, now=1.0, prefix=PREFIX)

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
                    launcher.throughput(launch, 'desktop', Path(directory) / 'run', now=1.0, prefix=PREFIX)

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
                    launcher.throughput(launch, 'desktop', Path(directory) / 'run', now=1.0, prefix=PREFIX)



def qualification_launch(directory):
    """A complete admitted (non-formal) launch manifest around the fake evaluator."""
    import g115_line_a_manifest_v1 as manifest_module
    directory = Path(directory)
    binary = directory / 'public_feature_evaluation_v1.exe'
    binary.write_bytes(b'stand-in')
    pinned, digest = guard.pin_binary(binary, directory / 'pinned')

    def ref(path, value=None):
        if value is not None:
            Path(path).write_text(json.dumps(value))
        return dict(path=str(path), sha256=guard.sha256_file(path))

    job_list = jobs(8)
    body = dict(schema='g115-line-a-qualification-manifest/v1', launchable=True,
                job_list_sha256=manifest_module.canonical_sha256(job_list))
    body['manifest_sha256'] = manifest_module.canonical_sha256(body)
    binding = dict(launcher_sha256='1' * 64, executable_sha256=digest, data_tree_sha256='3' * 64)
    phases = [dict(workers=w, seconds=s, completed=8, rows=[dict(id='q%d' % k, sha256='%064d' % k) for k in range(8)])
              for w, s in ((1, 16.0), (8, 4.0))]
    now = 1_790_600_000.0
    evidence = dict(schema=guard.THROUGHPUT_SCHEMA, binding=dict(binding, work_class='bo3-ordinary'),
                    inventory=dict(desktop=dict(checked_unix=now, eligible=True, reason='idle', competing=[]),
                                   computehost=dict(checked_unix=now, eligible=False, reason='staging not engineered'),
                                   runpod=dict(checked_unix=now, eligible=False, reason='no Linux path')),
                    hosts=dict(desktop=dict(phases=phases, overhead_seconds=5.0)), placements=[], selected='desktop-8')
    for workers in (1, 8):
        allocation = dict(desktop=workers)
        evidence['placements'].append(dict(id='desktop-%d' % workers, allocation=allocation, eligible=True,
                                           ineligibility_reason='',
                                           projected_seconds=guard.projected_seconds(evidence, allocation, 8)))
    items = [dict(category=c, unit_bytes=100_000, units=8, measured_by=dict(path='E:/r.json', sha256='4' * 64))
             for c in guard.WORKSHEET_CATEGORIES]
    worksheet = dict(schema=guard.WORKSHEET_SCHEMA, job_set='calibration', manifest_sha256=body['manifest_sha256'],
                     cap_bytes=guard.JOB_SET_CAPS['calibration'], items=items,
                     projected_bytes=guard.projected_bytes(items))
    job = 'g115-line-a-qualification-test'
    scratch = dict(schema=guard.SCRATCH_SCHEMA, job=job, owner='opus-line-a-launcher', host='desktop',
                   root='D:/e-scratch/%s/' % job, sources=[dict(path='E:/x.json', sha256='a' * 64, bytes=1)],
                   disposable=['native/**'], cap_bytes=10**9)
    launch = dict(schema=launcher.LAUNCH_SCHEMA, mode='qualification', job=job,
                  documents=dict(launcher=ref(Path(launcher.__file__))),
                  executable=dict(path=str(pinned), sha256=digest, git_head=GIT_HEAD), binding=binding,
                  job_manifest=ref(directory / 'job-set.json', body), job_manifest_sha256=body['manifest_sha256'],
                  jobs=job_list, throughput_evidence={'bo3-ordinary': ref(directory / 'evidence.json', evidence)},
                  worksheet=ref(directory / 'worksheet.json', worksheet),
                  scratch_manifest=ref(directory / 'scratch.json', scratch),
                  hosts=dict(desktop=dict(volume=str(directory))), deck_packet=PACKET, learner_source=LEARNER,
                  member_sources=MEMBERS, control_allowance_bytes=1, reservation_bytes=10**6,
                  job_timeout_seconds=60)
    return launch, now


class AdmittedDispatchTests(unittest.TestCase):
    def patches(self, directory):
        return (patch.object(guard, 'PINNED_ROOT', str(Path(directory) / 'pinned')),
                patch.object(guard, 'DISK_RESERVE_BYTES', 0), patch.object(launcher, 'minimum_reserve', lambda h: 0))

    def test_admitted_qualification_runs_at_the_selected_worker_count(self):
        with tempfile.TemporaryDirectory() as directory:
            launch, now = qualification_launch(directory)
            first, second, third = self.patches(directory)
            with first, second, third:
                placements, admitted = launcher.check(launch, 'desktop', now=now)
                self.assertEqual(placements['bo3-ordinary']['id'], 'desktop-8')
                root = Path(directory) / 'run'
                root.mkdir()
                result = launcher.dispatch(launch, 'desktop', root, placements, admitted, prefix=PREFIX)
            self.assertTrue(result['complete'])
            self.assertEqual((result['jobs'], result['completed'], result['not_started']), (8, 8, []))
            self.assertEqual({row['workers'] for row in result['rows']}, {8})
            self.assertFalse(result['outcomes_read'])
            self.assertEqual(json.loads((root / 'completion.json').read_text()), result)

    def test_admission_refuses_missing_worksheet_evidence_or_scope(self):
        with tempfile.TemporaryDirectory() as directory:
            launch, now = qualification_launch(directory)
            first, second, third = self.patches(directory)
            with first, second, third:
                with self.assertRaisesRegex(ValueError, 'worksheet missing'):
                    launcher.check(dict(launch, worksheet=None), 'desktop', now=now)
                with self.assertRaisesRegex(ValueError, 'Throughput evidence missing'):
                    launcher.check(dict(launch, throughput_evidence={}), 'desktop', now=now)
                with self.assertRaisesRegex(ValueError, 'Jobs differ'):
                    launcher.check(dict(launch, jobs=launch['jobs'][:-1]), 'desktop', now=now)
                formal = dict(launch, mode='calibration')
                with self.assertRaisesRegex(ValueError, 'schema differs from the launch mode'):
                    launcher.check(formal, 'desktop', now=now)

    def test_formal_modes_need_a_launchable_set_the_scope_ruling_and_the_yardstick(self):
        import g115_line_a_manifest_v1 as manifest_module
        with tempfile.TemporaryDirectory() as directory:
            launch, now = qualification_launch(directory)
            body = json.loads(Path(launch['job_manifest']['path']).read_text())
            ruling = Path(directory) / 'ruling.md'
            ruling.write_text('Scope ruling: version 1 freezes as staged B.')
            scope = dict(schema=guard.SCOPE_SCHEMA, composition='B',
                         ruling=dict(path=str(ruling), sha256=guard.sha256_file(ruling),
                                     excerpt='version 1 freezes as staged B'))

            def formal(launchable, yardstick, with_scope):
                value = dict(body, schema=manifest_module.SCHEMAS['calibration'], launchable=launchable,
                             composition='B', yardstick_executable_sha256=yardstick)
                value.pop('manifest_sha256')
                value['manifest_sha256'] = manifest_module.canonical_sha256(value)
                path = Path(directory) / ('formal-%s-%s.json' % (launchable, yardstick[:4]))
                path.write_text(json.dumps(value))
                worksheet = json.loads(Path(launch['worksheet']['path']).read_text())
                worksheet['manifest_sha256'] = value['manifest_sha256']
                sheet = Path(directory) / ('sheet-%s-%s.json' % (launchable, yardstick[:4]))
                sheet.write_text(json.dumps(worksheet))
                return dict(launch, mode='calibration', scope=scope if with_scope else None,
                            job_manifest=dict(path=str(path), sha256=guard.sha256_file(path)),
                            job_manifest_sha256=value['manifest_sha256'],
                            worksheet=dict(path=str(sheet), sha256=guard.sha256_file(sheet)))

            first, second, third = self.patches(directory)
            with first, second, third:
                digest = launch['executable']['sha256']
                with self.assertRaisesRegex(ValueError, 'not launchable'):
                    launcher.check(formal(False, digest, True), 'desktop', now=now)
                with self.assertRaisesRegex(ValueError, 'scope ruling record required'):
                    launcher.check(formal(True, digest, False), 'desktop', now=now)
                with self.assertRaisesRegex(ValueError, 'pinned yardstick executable'):
                    launcher.check(formal(True, '9' * 64, True), 'desktop', now=now)
                placements, _ = launcher.check(formal(True, digest, True), 'desktop', now=now)
                self.assertEqual(placements['bo3-ordinary']['id'], 'desktop-8')

    def test_registration_commits_only_the_catalog_file(self):
        calls = []
        launch = dict(mode='qualification', job='j', executable=dict(sha256='a' * 64))
        launcher.register('D:/e-scratch/j/run', 'live', launch, run=lambda args, check: calls.append(args))
        launcher.register('D:/e-scratch/j/run', 'closed', launch, run=lambda args, check: calls.append(args))
        self.assertIn('add', calls[0])
        self.assertEqual(calls[0][calls[0].index('--retention') + 1], 'prunable')
        self.assertIn('update', calls[2])
        self.assertEqual(calls[1][-2:], ['--', 'ARTIFACTS/catalog.jsonl'])
        self.assertEqual(calls[3][-2:], ['--', 'ARTIFACTS/catalog.jsonl'])



def with_search_member(launch, directory, now):
    """Re-pin the qualification launch so half of its jobs use a search-kind member with its own evidence."""
    import g115_line_a_manifest_v1 as manifest_module
    directory = Path(directory)
    launch = copy.deepcopy(launch)
    launch['member_sources']['d3'] = dict(kind='information_set_search_v3', source=LEARNER['source'],
                                          descriptor=dict(schema='fake-descriptor'))
    for job in launch['jobs'][::2]:
        job['member'] = 'd3'
    body = json.loads(Path(launch['job_manifest']['path']).read_text())
    body['job_list_sha256'] = manifest_module.canonical_sha256(launch['jobs'])
    body.pop('manifest_sha256')
    body['manifest_sha256'] = manifest_module.canonical_sha256(body)
    job_path = directory / 'job-set-split.json'
    job_path.write_text(json.dumps(body))
    worksheet = json.loads(Path(launch['worksheet']['path']).read_text())
    worksheet['manifest_sha256'] = body['manifest_sha256']
    sheet = directory / 'worksheet-split.json'
    sheet.write_text(json.dumps(worksheet))
    ordinary = json.loads(Path(launch['throughput_evidence']['bo3-ordinary']['path']).read_text())
    for placement in ordinary['placements']:  # four ordinary jobs remain after the split
        placement['projected_seconds'] = guard.projected_seconds(ordinary, placement['allocation'], 4)
    ordinary_path = directory / 'evidence-ordinary-split.json'
    ordinary_path.write_text(json.dumps(ordinary))
    launch['throughput_evidence']['bo3-ordinary'] = dict(path=str(ordinary_path),
                                                         sha256=guard.sha256_file(ordinary_path))
    search = copy.deepcopy(ordinary)
    search['binding']['work_class'] = 'bo3-search'
    search['hosts']['desktop']['phases'] = [dict(p, seconds=p['seconds'] * (40 if p['workers'] == 1 else 60))
                                         for p in search['hosts']['desktop']['phases']]
    search['placements'] = []
    for workers in (1, 8):
        allocation = dict(desktop=workers)
        search['placements'].append(dict(id='desktop-%d' % workers, allocation=allocation, eligible=True,
                                         ineligibility_reason='',
                                         projected_seconds=guard.projected_seconds(search, allocation, 4)))
    search['selected'] = min(search['placements'], key=lambda p: p['projected_seconds'])['id']
    evidence = directory / 'evidence-search.json'
    evidence.write_text(json.dumps(search))
    launch.update(job_manifest=dict(path=str(job_path), sha256=guard.sha256_file(job_path)),
                  job_manifest_sha256=body['manifest_sha256'],
                  worksheet=dict(path=str(sheet), sha256=guard.sha256_file(sheet)))
    launch['throughput_evidence']['bo3-search'] = dict(path=str(evidence), sha256=guard.sha256_file(evidence))
    return launch, search['selected']


class WorkClassSplitTests(unittest.TestCase):
    def test_each_work_class_runs_at_its_own_selected_worker_count(self):
        with tempfile.TemporaryDirectory() as directory:
            base, now = qualification_launch(directory)
            launch, search_selected = with_search_member(base, directory, now)
            with patch.object(guard, 'PINNED_ROOT', str(Path(directory) / 'pinned')), \
                    patch.object(guard, 'DISK_RESERVE_BYTES', 0), patch.object(launcher, 'minimum_reserve', lambda h: 0):
                placements, admitted = launcher.check(launch, 'desktop', now=now)
                self.assertEqual(set(placements), {'bo3-ordinary', 'bo3-search'})
                self.assertEqual(placements['bo3-search']['id'], search_selected)
                root = Path(directory) / 'run'
                root.mkdir()
                result = launcher.dispatch(launch, 'desktop', root, placements, admitted, prefix=PREFIX)
                with self.assertRaisesRegex(ValueError, 'Throughput evidence missing for bo3-search'):
                    launcher.check(dict(launch, throughput_evidence={'bo3-ordinary': launch['throughput_evidence'][
                        'bo3-ordinary']}), 'desktop', now=now)
            self.assertTrue(result['complete'])
            workers = {row['work_class']: row['workers'] for row in result['rows']}
            self.assertEqual(workers, {'bo3-ordinary': 8, 'bo3-search': placements['bo3-search']['allocation']['desktop']})
            self.assertEqual(sum(row['work_class'] == 'bo3-search' for row in result['rows']), 4)
            admission = json.loads((root / 'admission.json').read_text())
            self.assertEqual(set(admission['placements']), {'bo3-ordinary', 'bo3-search'})
            self.assertIsNone(admission['placements']['bo3-ordinary']['shortfall'])


if __name__ == '__main__':
    unittest.main()
