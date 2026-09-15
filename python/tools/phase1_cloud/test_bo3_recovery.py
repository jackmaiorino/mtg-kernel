"""Filesystem/metadata checks only. No native executable or provider is run."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import bo3_recovery as recovery


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(json.dumps(value, separators=(',', ':')).encode())
    return file_pin(path)


def file_pin(path):
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


class RecoveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='phase1-bo3-recovery-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.hot, self.durable = self.root / 'hot', self.root / 'durable'
        self.hot.mkdir()
        self.events, self.heartbeats, self.stopped = [], 0, True
        self.limits = recovery.Limits(max_bytes=8 * recovery.MIB, max_new_bytes=16 * recovery.MIB,
                                      max_file_bytes=recovery.MIB, chunk_bytes=1024)
        self.kw = dict(limits=self.limits, assert_stopped=self.check_stopped,
                       heartbeat=self.beat, quota=lambda event: self.events.append(event.copy()))

    def check_stopped(self):
        if not self.stopped:
            raise ValueError('owned process has not been reaped')

    def beat(self):
        self.heartbeats += 1

    def fixture(self, completed=(False, True), planned=3):
        """Opaque checkpoint bytes and structural metadata, not native evidence."""
        run = self.hot / 'run'
        run.mkdir()
        initial = {'source': {'play_import': {'path': str(self.hot / 'ordinary.json'), 'sha256': 'a' * 64},
                              'checkpoint': {'path': str(self.hot / 'ordinary-checkpoint.json'), 'sha256': 'b' * 64}},
                   'identity': {'adam_step': 483}}
        config = {'schema': 'mtg-kernel-native-bo3-training-run/v1', 'output_directory': str(run),
                  'previous_progress': None, 'initial_input': {'kind': 'ordinary_checkpoint_transition', 'learner': initial},
                  'initial_learner': {'gameplay': initial}, 'batches': [{'matches': [{}]} for _ in range(planned)],
                  'preparation_limits': {'max_input_bytes': recovery.MIB, 'max_prepared_payload_bytes': recovery.MIB},
                  'learning_rate_bits': 925353388, 'value_coefficient_bits': 1056964608}
        request = save(self.hot / 'config/run.json', config)
        package = save(self.hot / 'manifest.json', {'schema': 'phase1-bo3-cloud-payload/v1',
                        'remote_root': str(self.hot), 'run_request': request})
        run_pin = save(run / 'run.json', {'schema': 'mtg-kernel-native-bo3-run-manifest/v1', 'config': config})
        current, previous, updates, ledger, receipts = copy.deepcopy(initial), None, 0, [], []
        for index, ready in enumerate(completed):
            directory = run / 'batches' / f'{index:06d}'
            operation_root = directory / 'update-attempt-000000/operation'
            capture_request = save(directory / 'slots/000/attempt-000000/request.json', {'opaque': index})
            capture = save(directory / 'slots/000/attempt-000000/result.json', {'opaque-result': index})
            attempt = {'request': capture_request, 'result': capture, 'learner_seat': 'p0'}
            preparation = save(directory / 'preparation.json', {'schema': 'mtg-kernel-bo3-gameplay-preparation/v1',
                'learner': current, 'attempts': [attempt], 'limits': config['preparation_limits']})
            before = {'kind': 'bo3_checkpoint' if updates else 'ordinary_checkpoint_transition', 'learner': current}
            operation = {'schema': 'mtg-kernel-bo3-gameplay-update-request/v1', 'input': before,
                         'preparation_request': preparation, 'previous_progress': previous,
                         'learning_rate_bits': config['learning_rate_bits'], 'value_coefficient_bits': config['value_coefficient_bits'],
                         'output_directory': str(operation_root)}
            update_pin = save(directory / 'update-attempt-000000/update-request.json', operation)
            operation_pin = save(operation_root / 'request.json', operation)
            after = copy.deepcopy(current)
            if ready:
                updates += 1
                after['identity']['adam_step'] += 1
                after['source']['checkpoint'] = save(operation_root / 'checkpoint.json', {'opaque-state': updates})
                after['source']['play_import'] = save(operation_root / 'source.json', {'opaque-origin': 483})
            ledger.append([capture_request['sha256'], capture['sha256'], f'{index + 1:064x}'])
            progress = {'schema': 'mtg-kernel-bo3-gameplay-progress/v1', 'operation_request': operation_pin,
                        'request': operation, 'result': after, 'completed_bo3_updates': updates,
                        'attempt_progress': {'attempted_batches': index + 1, 'ledger': copy.deepcopy(ledger),
                            'preparation': {'disposition': 'ready' if ready else 'no_update', 'attempts': [{}],
                                            'eligible_matches': int(ready), 'complete_matches': int(ready),
                                            'incomplete_matches': int(not ready)}}}
            progress_pin = save(operation_root / 'progress.json', progress)
            result = {'progress': progress_pin, 'learner': after, 'completed_bo3_updates': updates,
                      'attempted_batches': index + 1, 'attempted_matches': index + 1, 'optimizer_updated': ready}
            receipt = {'schema': 'mtg-kernel-native-bo3-batch/v1', 'batch': index, 'manifest': run_pin,
                       'input': before, 'previous_progress': previous, 'attempts': [attempt],
                       'preparation_request': preparation, 'update_request': update_pin, 'result': result}
            receipts.append(save(directory / 'complete.json', receipt))
            current, previous = after, progress_pin
        if len(completed) == planned:
            save(run / 'completion.json', {'schema': 'mtg-kernel-native-bo3-run-result/v1', 'complete': True,
                'completed_batches': planned, 'planned_batches': planned, 'learner': current,
                'previous_progress': previous, 'completed_bo3_updates': updates,
                'attempted_batches': planned, 'attempted_matches': planned, 'batch_receipts': receipts})
        (run / 'run.lock').write_bytes(b'mutable lock excluded')
        save(run / 'telemetry/invocation.json', {'mutable': True})
        return package['sha256'], config

    def export(self, generation='one'):
        return recovery.export_stopped(self.hot, self.durable, generation, self.package, **self.kw)

    def restore(self, exported, generation='one'):
        return recovery.restore_stopped(self.durable, generation, exported['index']['sha256'],
                                        self.hot, self.package, **self.kw)

    def test_no_update_ready_partial_tree_and_existing_file_restore(self):
        self.package, _ = self.fixture()
        orphan = self.hot / 'run/batches/000002/update-attempt-000000/operation/.progress.json.stage'
        orphan.parent.mkdir(parents=True)
        orphan.write_bytes(b'partial original progress')
        (orphan.parent / 'checkpoint.json').write_bytes(b'opaque committed pending checkpoint')
        (self.hot / 'run/batches/000002/slots/000/attempt-000003').mkdir(parents=True)
        exported = self.export()
        self.assertEqual(exported['summary']['completed_batches'], 2)
        self.assertEqual(exported['summary']['completed_bo3_updates'], 1)
        self.assertEqual(exported['summary']['attempted_matches'], 2)
        self.assertEqual(exported['summary']['batch_dispositions'], ['no_update', 'ready'])
        self.assertEqual((exported['summary']['complete_matches'], exported['summary']['incomplete_matches'],
                          exported['summary']['eligible_matches']), (1, 1, 1))
        self.assertEqual(len(exported['summary']['pending_checkpoint_files']), 1)
        index = json.loads(Path(exported['index']['path']).read_bytes())
        self.assertNotIn('run.lock', index['files'])
        self.assertFalse(any(name.startswith('telemetry/') for name in index['files']))
        self.assertIn('batches/000002/slots/000/attempt-000003', index['directories'])
        (self.hot / 'run').rename(self.root / 'preserved-original')
        restored = self.restore(exported)
        self.assertEqual(restored['summary'], exported['summary'])
        self.assertEqual(orphan.read_bytes(), b'partial original progress')
        self.assertEqual(self.restore(exported)['copied_bytes'], 0)
        phases = [event['phase'] for event in self.events]
        self.assertIn('hash-complete-chunk', phases)
        self.assertIn('directory-sync', phases)
        self.assertGreater(self.heartbeats, 100)

    def test_cas_reuses_blobs_and_keeps_stage_generations(self):
        self.package, _ = self.fixture()
        stage = self.hot / 'run/batches/000002/.stage-original'
        stage.parent.mkdir()
        stage.write_bytes(b'first stage contents')
        first = self.export()
        repeated = self.export('two')
        self.assertEqual(repeated['new_blob_bytes'], 0)
        first_index = json.loads(Path(first['index']['path']).read_bytes())
        stage.write_bytes(b'updated stopped stage contents')
        changed = self.export('three')
        self.assertEqual(changed['new_blob_bytes'], len(stage.read_bytes()))
        row = first_index['files']['batches/000002/.stage-original']
        self.assertEqual((self.durable / 'blobs' / row['sha256']).read_bytes(), b'first stage contents')
        with self.assertRaisesRegex(ValueError, 'existing native destination differs'):
            self.restore(first)

    def test_complete_receipts_restore_after_payloads(self):
        self.package, _ = self.fixture(planned=2)
        exported = self.export()
        (self.hot / 'run').rename(self.root / 'preserved')
        self.events.clear()
        self.restore(exported)
        published = [Path(e['path']).name for e in self.events if e['phase'] == 'copy-publish']
        self.assertEqual(published[-1], 'completion.json')
        self.assertLess(published.index('checkpoint.json'), published.index('progress.json'))
        self.assertTrue(exported['summary']['complete'])

    def test_rejects_changed_package_root_unindexed_and_corrupt_blob(self):
        self.package, _ = self.fixture()
        exported = self.export()
        with self.assertRaisesRegex(ValueError, 'identity'):
            recovery.restore_stopped(self.durable, 'one', exported['index']['sha256'], self.hot, '0' * 64, **self.kw)
        other = self.root / 'other-hot'
        shutil.copytree(self.hot, other)
        with self.assertRaisesRegex(ValueError, 'original absolute hot root'):
            recovery.restore_stopped(self.durable, 'one', exported['index']['sha256'], other, self.package, **self.kw)
        extra = self.hot / 'run/unindexed.partial'
        extra.write_bytes(b'untrusted')
        with self.assertRaisesRegex(ValueError, 'unindexed'):
            self.restore(exported)
        extra.rename(self.root / 'preserved-unindexed')
        index = json.loads(Path(exported['index']['path']).read_bytes())
        row = index['files']['run.json']
        (self.durable / 'blobs' / row['sha256']).write_bytes(b'corrupt blob')
        with self.assertRaisesRegex(ValueError, 'blob differs'):
            self.restore(exported)

    def test_stopped_deadline_and_quota_checks_cover_streams(self):
        self.package, _ = self.fixture()
        self.stopped = False
        with self.assertRaisesRegex(ValueError, 'not been reaped'):
            self.export()
        self.assertFalse(self.durable.exists())
        self.stopped = True
        def quota(event):
            self.events.append(event)
            if event['phase'] == 'copy-write':
                raise ValueError('cumulative retained quota exhausted')
        self.kw['quota'] = quota
        with self.assertRaisesRegex(ValueError, 'cumulative'):
            self.export()
        self.assertFalse((self.durable / 'indices/one.json').exists())
        self.kw['quota'] = lambda event: self.events.append(event)
        exported = self.export('two')
        def deadline(event):
            if event['phase'] == 'hash-read' and '/blobs/' in event['path'].replace('\\', '/'):
                raise ValueError('release deadline reached during existing blob verification')
        self.kw['quota'] = deadline
        with self.assertRaisesRegex(ValueError, 'release deadline'):
            self.export('three')
        self.assertFalse((self.durable / 'indices/three.json').exists())
        self.assertTrue(Path(exported['index']['path']).exists())

    def test_inventory_limits_and_gap_or_counter_rejection(self):
        self.package, _ = self.fixture()
        kwargs = {**self.kw, 'limits': recovery.Limits(max_files=1)}
        with self.assertRaisesRegex(ValueError, 'file-count'):
            recovery.export_stopped(self.hot, self.durable, 'limited', self.package, **kwargs)
        receipt = self.hot / 'run/batches/000000/complete.json'
        original = receipt.read_bytes()
        receipt.rename(receipt.with_name('preserved-original'))
        with self.assertRaisesRegex(ValueError, 'contiguous'):
            self.export()
        receipt.write_bytes(original)
        value = json.loads(original)
        value['result']['completed_bo3_updates'] = 1
        save(receipt, value)
        with self.assertRaisesRegex(ValueError, 'conflated'):
            self.export()

    def test_publication_index_is_pinned_and_unindexed_temp_is_not_used(self):
        self.package, _ = self.fixture()
        exported = self.export()
        temporary = self.durable / '.copy-tmp/plausible-checkpoint.partial'
        temporary.parent.mkdir(exist_ok=True)
        temporary.write_bytes(b'never an indexed blob')
        (self.hot / 'run').rename(self.root / 'preserved')
        with self.assertRaisesRegex(ValueError, 'identity'):
            recovery.restore_stopped(self.durable, 'one', 'f' * 64, self.hot, self.package, **self.kw)
        restored = self.restore(exported)
        self.assertEqual(restored['summary'], exported['summary'])
        self.assertNotIn('plausible-checkpoint.partial', [p.name for p in (self.hot / 'run').rglob('*')])

    def test_linked_recovery_parent_is_rejected(self):
        self.package, _ = self.fixture()
        exported = self.export()
        original = self.durable / 'blobs'
        preserved = self.root / 'preserved-blobs'
        original.rename(preserved)
        try:
            original.symlink_to(preserved, target_is_directory=True)
        except OSError:
            preserved.rename(original)
            self.skipTest('this host does not allow creating a test directory symlink')
        with self.assertRaisesRegex(ValueError, 'links/reparse'):
            self.restore(exported)


class ActualSavedMetadataTest(unittest.TestCase):
    def test_existing_three_batch_runner_keeps_two_updates_four_attempts(self):
        run = Path('E:/mtg-kernel-learned-sideboarding-evidence/bo3-post480-preparation-001/phase1-training-qualification-001/bo3-continuation-native-outputs/runner-recovered-001')
        if not (run / 'completion.json').exists():
            self.skipTest('saved Windows engineering evidence is not present on this host')
        guard = recovery._Guard(recovery.Limits(), lambda: None, lambda: None, lambda event: None)
        files, _ = recovery._inventory(run, guard)
        config = json.loads((run / 'run.json').read_bytes())['config']
        summary = recovery._inspect(run, run, files, config, guard)
        self.assertEqual((summary['completed_batches'], summary['completed_bo3_updates'], summary['attempted_matches']), (3, 2, 4))
        self.assertEqual(summary['adam_step'], 485)
        self.assertEqual(summary['batch_dispositions'], ['no_update', 'ready', 'ready'])
        self.assertEqual((summary['complete_matches'], summary['incomplete_matches'], summary['eligible_matches']), (3, 1, 3))


if __name__ == '__main__':
    unittest.main()
