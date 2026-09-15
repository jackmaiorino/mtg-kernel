"""Pure supervisor contract checks. No Pod/guard process or native launch is simulated."""
import copy
from types import SimpleNamespace
import unittest

import bo3_worker as worker


class WorkerContracts(unittest.TestCase):
    def test_native_plan_schema_and_remaining_batch_bound(self):
        config = {'schema': 'mtg-kernel-native-bo3-training-run/v1', 'batches': [{}, {}, {}]}
        self.assertEqual(worker.remaining_admission(config, 1, 3), 2)
        self.assertEqual(worker.remaining_admission(config, 3, 1), 0)
        for field, value in (('schema', 'mtg-kernel-native-bo3-run/v1'), ('batches', [])):
            changed = dict(config, **{field: value})
            with self.assertRaises(ValueError):
                worker.remaining_admission(changed, 0, 1)
        for completed, maximum in ((4, 1), (0, 0), (0, True)):
            with self.assertRaises(ValueError):
                worker.remaining_admission(config, completed, maximum)

    def test_production_rejected_before_any_platform_or_cloud_work(self):
        with self.assertRaisesRegex(ValueError, 'production'):
            worker.validate_args(SimpleNamespace(mode='production'))

    @staticmethod
    def declarations():
        # Values exercise a pure predicate only. They are not actual guard evidence.
        lease = {'name': 'contract-only', 'deadline_epoch': 1000., 'recovery_seconds': 60.}
        status = {'pod_id': 'contract-only', 'name': 'contract-only', 'epoch': 100., 'pid': 123,
                  'provider_verified': True, 'provider_ok': True, 'funds_verified': True,
                  'allow_new_dispatch': True, 'recovery_funding_available': True,
                  'latched': None, 'release_epoch': None}
        return lease, status

    def test_guard_requires_matching_fresh_funded_unlatched_state(self):
        lease, status = self.declarations()
        worker.guard_dispatch(lease, status, 'contract-only', 101.)
        for key, value in (('pod_id', 'different'), ('name', 'different'), ('epoch', 0.),
                           ('provider_ok', False), ('provider_verified', False), ('funds_verified', False),
                           ('allow_new_dispatch', False), ('recovery_funding_available', False),
                           ('latched', 'deadline'), ('release_epoch', 160.), ('pid', 0)):
            with self.subTest(key=key), self.assertRaises(ValueError):
                worker.guard_dispatch(lease, dict(status, **{key: value}), 'contract-only', 101.)
        with self.assertRaises(ValueError):
            worker.guard_dispatch(lease, status, 'contract-only', 101., True)

    def test_stale_prior_export_does_not_authorize_current_release(self):
        prior = {'index': {'sha256': 'a' * 64}}
        current = prior
        self.assertTrue(worker.release_eligible(current, True, True))
        # Exact dispatch boundary retains the historical pin but clears current state.
        historical, current = current, None
        self.assertIs(historical, prior)
        self.assertFalse(worker.release_eligible(current, True, True))
        self.assertFalse(worker.release_eligible(prior, False, True))
        self.assertFalse(worker.release_eligible(prior, True, False))

    def test_two_low_windows_survive_child_boundary_and_export_gap(self):
        policy = worker.NativeCapacityWindows()
        policy.observe(0., 0., 4., 0, True)
        first = policy.observe(60., 60., 4., 0, False)
        self.assertNotIn('diagnosis', first)
        gap = policy.observe(600., 60., 4., 1, False)
        self.assertEqual(gap['cpu_ready_seconds'], 60.)
        policy.observe(600., 60., 4., 1, True)
        second = policy.observe(660., 120., 4., 1, False)
        self.assertEqual(second['diagnosis'], 'two_full_capacity_windows_below_90_percent')
        self.assertEqual(policy.ready_seconds, 120.)

    def test_heartbeat_without_work_does_not_advance_activity(self):
        first = worker.heartbeat_fields('pure', 100., 90., 95., True, 'native_one_batch')
        second = worker.heartbeat_fields('pure', 150., 90., 95., True, 'native_one_batch')
        self.assertEqual(first['last_activity_epoch'], second['last_activity_epoch'])
        self.assertEqual(first['last_productive_epoch'], second['last_productive_epoch'])
        self.assertGreater(second['epoch'], first['epoch'])
        self.assertTrue(second['native_alive'] and second['queued_work'])
        stopped = worker.heartbeat_fields('pure', 151., 90., 95., False, 'stopped_export')
        self.assertFalse(stopped['queued_work'])

    def test_compact_export_preserves_distinct_ready_and_incomplete_counts(self):
        summary = {'completed_batches': 2, 'planned_batches': 3, 'completed_bo3_updates': 1,
                   'attempted_batches': 2, 'attempted_matches': 3, 'complete_matches': 2,
                   'incomplete_matches': 1, 'eligible_matches': 2, 'adam_step': 484, 'complete': False,
                   'pending_checkpoint_files': ['checkpoint.json'], 'pending_progress_files': []}
        source = {'index': {'path': '/declared/index', 'sha256': 'b' * 64}, 'summary': summary,
                  'read_bytes': 10, 'written_bytes': 20, 'new_blob_bytes': 20, 'indexed_bytes': 30}
        before = copy.deepcopy(source)
        receipt = worker.export_receipt(source)
        self.assertEqual(receipt['summary']['completed_bo3_updates'], 1)
        self.assertEqual(receipt['summary']['attempted_batches'], 2)
        self.assertEqual(receipt['summary']['incomplete_matches'], 1)
        self.assertEqual(receipt['pending_checkpoint_count'], 1)
        self.assertEqual(source, before)
        self.assertFalse(receipt['long_stream_export_throughput_qualified'])

    def test_receipts_cannot_consume_reserved_final_accounting_headroom(self):
        self.assertEqual(worker.receipt_charge(80, 10, 100, 10), 90)
        with self.assertRaises(ValueError):
            worker.receipt_charge(90, 1, 100, 10)
        self.assertEqual(worker.receipt_charge(90, 10, 100, 10, final=True), 100)
        self.assertEqual(worker.receipt_charge(95, 5, 100, 10, final=True), 100)
        with self.assertRaises(ValueError):
            worker.receipt_charge(95, 6, 100, 10, final=True)
        with self.assertRaises(ValueError):
            worker.receipt_charge(80, 11, 100, 10, final=True)

    def test_final_disk_write_preserves_physical_reserve_and_discounts_cached_writes(self):
        free = worker.RESERVE + worker.FINAL_RECEIPT_RESERVE + 10
        worker.require_disk_headroom([free], 10)
        with self.assertRaises(ValueError):
            worker.require_disk_headroom([free], 10, written=1)
        worker.require_disk_headroom([worker.RESERVE + 5], 5, final=True)
        with self.assertRaises(ValueError):
            worker.require_disk_headroom([worker.RESERVE + 4], 5, final=True)


if __name__ == '__main__':
    unittest.main()
