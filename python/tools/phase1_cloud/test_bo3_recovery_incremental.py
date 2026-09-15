"""Small POSIX filesystem checks. No engine, Pod, provider or benchmark work."""
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile
import threading
import unittest

import bo3_recovery as full
import bo3_recovery_incremental as incremental
import test_bo3_recovery as fixtures
from test_bo3_recovery import save


@unittest.skipUnless(os.name == 'posix', 'live cache requires POSIX change-time/lock behavior; frozen Windows fallback is unchanged')
class IncrementalTests(unittest.TestCase):
    # Reuse the existing explicitly opaque filesystem/metadata fixture writer.
    # Its checkpoint bytes are not a trained model or engine outcome.
    fixture = fixtures.RecoveryTests.fixture

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='phase1-bo3-incremental-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.hot, self.durable = self.root / 'hot', self.root / 'durable'
        self.hot.mkdir()
        self.package, self.config = self.fixture(completed=(False, True, True), planned=3)
        self.saved = self.root / 'not-yet-published'
        self.saved.mkdir()
        for index in (1, 2):
            (self.hot / 'run/batches' / f'{index:06d}').rename(self.saved / f'{index:06d}')
        (self.hot / 'run/completion.json').rename(self.saved / 'completion.json')
        self.events, self.stopped = [], True
        self.limits = full.Limits(max_bytes=8 * full.MIB, max_new_bytes=16 * full.MIB,
                                 max_file_bytes=full.MIB, chunk_bytes=1024)
        self.kw = dict(limits=self.limits, assert_stopped=self.assert_stopped,
                       heartbeat=lambda: None, quota=lambda event: self.events.append(event.copy()))
        self.owner = incremental.OwnerLock(self.hot).__enter__()
        self.addCleanup(self.owner.__exit__, None, None, None)
        self.session = incremental.VerifiedPrefixSession(self.owner, self.durable, self.package, **self.kw)

    def assert_stopped(self):
        if not self.stopped:
            raise ValueError('not at stopped fixture boundary')

    def publish_next(self, index):
        destination = self.hot / 'run/batches' / f'{index:06d}'
        source = self.saved / f'{index:06d}'
        if destination.exists():
            shutil.copytree(source, destination, dirs_exist_ok=True)
        else:
            source.rename(destination)
        if index == 2:
            (self.saved / 'completion.json').rename(self.hot / 'run/completion.json')

    def compare_full_index(self, result, generation):
        reference = full.export_stopped(self.hot, self.root / ('full-' + generation), generation,
                                       self.package, **self.kw)
        self.assertEqual(Path(result['index']['path']).read_bytes(), Path(reference['index']['path']).read_bytes())
        self.assertEqual(result['summary'], reference['summary'])

    def test_one_two_three_prefix_indices_equal_frozen_export(self):
        first = self.session.start_full('one')
        self.assertTrue(first['release_eligible'])
        self.compare_full_index(first, 'one')
        self.publish_next(1)
        self.events.clear()
        second = self.session.export_next('two', first['index'])
        self.assertFalse(second['release_eligible'] or second['handoff_eligible'])
        self.assertGreater(second['cached_file_reuses'], 0)
        self.assertGreater(second['cached_logical_bytes_reused'], 0)
        self.assertEqual([event['read_bytes'] for event in self.events], sorted(event['read_bytes'] for event in self.events))
        self.assertEqual(self.events[-1]['read_bytes'], second['read_bytes'])
        self.compare_full_index(second, 'two')
        self.publish_next(2)
        third = self.session.export_next('three', second['index'])
        self.compare_full_index(third, 'three')
        self.assertEqual((third['summary']['completed_batches'], third['summary']['completed_bo3_updates'],
                          third['summary']['attempted_matches'], third['summary']['incomplete_matches']), (3, 2, 3, 1))
        finished = self.session.finish_full('final-full')
        self.assertTrue(finished['release_eligible'] and finished['handoff_eligible'])
        self.compare_full_index(finished, 'final-full')

    def test_empty_origin_admits_first_native_manifest_and_prefix(self):
        saved_run = self.root / 'not-started-run'
        (self.hot / 'run').rename(saved_run)
        empty = self.session.start_full('empty')
        self.assertEqual(empty['summary']['completed_batches'], 0)
        saved_run.rename(self.hot / 'run')
        first = self.session.export_next('first-published', empty['index'])
        self.assertEqual(first['summary']['completed_batches'], 1)
        self.compare_full_index(first, 'first-published')

    def test_changed_backdated_committed_file_rejects_and_invalidates(self):
        first = self.session.start_full('one')
        path = self.hot / 'run/batches/000000/slots/000/attempt-000000/result.json'
        before = path.stat()
        data = bytearray(path.read_bytes()); data[0] ^= 1; path.write_bytes(data)
        os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns))
        with self.assertRaises(ValueError):
            self.session.export_next('changed', first['index'])
        with self.assertRaises(ValueError):
            self.session.finish_full('cannot-reuse-invalid-cache')
        self.assertFalse((self.durable / 'indices/changed.json').exists())

    def test_added_or_deleted_committed_member_rejects(self):
        first = self.session.start_full('one')
        (self.hot / 'run/batches/000000/extra').write_bytes(b'unexpected')
        with self.assertRaisesRegex(ValueError, 'membership|directory'):
            self.session.export_next('added', first['index'])
        # Independent fresh owner session/full snapshot for the removal case.
        (self.hot / 'run/batches/000000/extra').unlink()
        other = incremental.VerifiedPrefixSession(self.owner, self.durable, self.package, **self.kw)
        fresh = other.start_full('fresh')
        (self.hot / 'run/batches/000000/slots/000/attempt-000000/result.json').unlink()
        with self.assertRaises(ValueError):
            other.export_next('deleted', fresh['index'])

    def test_pending_bytes_are_always_rehashed(self):
        path = self.hot / 'run/batches/000001/partial.bin'
        path.parent.mkdir(); path.write_bytes(b'old')
        first = self.session.start_full('one')
        before = path.stat(); path.write_bytes(b'new')
        os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns))
        second = self.session.export_next('pending-new', first['index'])
        index = json.loads(Path(second['index']['path']).read_bytes())
        self.assertEqual(index['files']['batches/000001/partial.bin']['sha256'], hashlib.sha256(b'new').hexdigest())
        self.compare_full_index(second, 'pending-new')

    def test_corrupt_cached_blob_rejects_export_and_frozen_restore(self):
        first = self.session.start_full('one')
        index = json.loads(Path(first['index']['path']).read_bytes())
        row = index['files']['batches/000000/slots/000/attempt-000000/result.json']
        blob = self.durable / 'blobs' / row['sha256']
        before = blob.stat(); data = bytearray(blob.read_bytes()); data[0] ^= 1; blob.write_bytes(data)
        os.utime(blob, ns=(before.st_atime_ns, before.st_mtime_ns))
        with self.assertRaises(ValueError):
            self.session.export_next('damaged', first['index'])
        with self.assertRaises(ValueError):
            full.restore_stopped(self.durable, 'one', first['index']['sha256'], self.hot, self.package, **self.kw)
        restart = incremental.VerifiedPrefixSession(self.owner, self.durable, self.package, **self.kw)
        with self.assertRaises(ValueError):
            restart.start_full('restart-verifies-corruption')

    def test_prior_index_owner_thread_and_restart_boundaries(self):
        first = self.session.start_full('one')
        wrong = copy.deepcopy(first['index']); wrong['sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'last immutable index'):
            self.session.export_next('wrong', wrong)
        restart = incremental.VerifiedPrefixSession(self.owner, self.durable, self.package, **self.kw)
        with self.assertRaisesRegex(ValueError, 'initial full'):
            restart.export_next('uncertified', first['index'])
        restart.start_full('restart-full')
        errors = []
        def other_thread():
            try:
                restart.finish_full('different-thread')
            except ValueError as error:
                errors.append(str(error))
        thread = threading.Thread(target=other_thread); thread.start(); thread.join()
        self.assertTrue(errors)
        fresh = incremental.VerifiedPrefixSession(self.owner, self.durable, self.package, **self.kw)
        fresh.start_full('owner-open')
        self.owner.__exit__(None, None, None)
        with self.assertRaisesRegex(ValueError, 'fresh owner lock'):
            self.owner.__enter__()
        with self.assertRaises(ValueError):
            fresh.finish_full('owner-closed')

    def test_final_full_hashes_still_preserve_prior_unreferenced_history(self):
        self.session.start_full('one')
        (self.hot / 'run/batches/000000/added-orphan.stage').write_bytes(b'changed committed membership')
        with self.assertRaisesRegex(ValueError, 'altered committed prefix'):
            self.session.finish_full('rejected-final')
        self.assertTrue((self.durable / 'indices/rejected-final.json').exists())
        self.assertFalse(self.session.usable)

    def test_bad_new_ledger_or_budget_does_not_publish_index(self):
        first = self.session.start_full('one')
        self.publish_next(1)
        path = self.hot / 'run/batches/000001/complete.json'
        receipt = json.loads(path.read_bytes()); receipt['previous_progress'] = None; save(path, receipt)
        with self.assertRaisesRegex(ValueError, 'verified prefix'):
            self.session.export_next('bad-new', first['index'])
        self.assertFalse((self.durable / 'indices/bad-new.json').exists())

    def test_stop_and_quota_callbacks_cover_reused_entries(self):
        first = self.session.start_full('one')
        self.stopped = False
        with self.assertRaises(ValueError):
            self.session.export_next('not-stopped', first['index'])
        self.stopped = True
        fresh = incremental.VerifiedPrefixSession(self.owner, self.durable, self.package, **self.kw)
        restarted = fresh.start_full('fresh')
        self.publish_next(1)
        def reject_write(event):
            if event['next_write_bytes']:
                raise ValueError('bounded quota reached')
        fresh.quota = reject_write
        with self.assertRaisesRegex(ValueError, 'quota'):
            fresh.export_next('over-budget', restarted['index'])
        self.assertFalse((self.durable / 'indices/over-budget.json').exists())


if __name__ == '__main__':
    unittest.main()
