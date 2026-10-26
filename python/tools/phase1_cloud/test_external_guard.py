"""Offline coverage of the workstation-side guard. No provider calls or credentials.

Every run uses a fake clock with a hard sleep cap and a fake provider with a hard
call cap (lease_test_support); a guard that never finishes ends the test with Stop.
"""
import tempfile
import unittest
from pathlib import Path

import external_guard as eg
from common import read, write
from lease_test_support import FakeClock, Stop, World, no_network
from test_cloud import lease

NAME = lease()['name']


def pod(pid='pod1', **changes):
    return {'id': pid, 'name': NAME, 'costPerHr': 0.96, **changes}


class Base(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(); self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        write(self.root / 'lease/lease.json', lease())
        self.clock = FakeClock(1100.)

    def record(self, pid='pod1'):
        write(self.root / 'lease/created-pod.json', {'id': pid, 'name': NAME}, replace=True)

    def started(self):
        write(self.root / 'worker-started.json', {}, replace=True)

    def status(self):
        return read(self.root / 'external-guard.json')

    def once(self, world, **options):
        with no_network():
            eg.run(self.root, 'k', pods=world.list, provider=world.provider, clock=self.clock,
                   sleep=self.clock.sleep, once=True, **options)
        return self.status()

    def loop(self, world, polls=40, **options):
        """'returned' when the guard completed, 'running' when it was still guarding after `polls`."""
        self.clock.max_sleeps = polls; self.clock.sleeps = 0
        try:
            with no_network():
                eg.run(self.root, 'k', pods=world.list, provider=world.provider, clock=self.clock,
                       sleep=self.clock.sleep, **options)
            return 'returned'
        except Stop:
            return 'running'

    def completion(self):
        path = self.root / 'external-guard-completion.json'
        return read(path) if path.exists() else None


class ExternalGuardTests(Base):
    def test_releases_pod_that_never_starts_its_worker(self):
        self.record(); self.clock.t = 2000.
        world = World([pod()])
        self.assertEqual(self.loop(world), 'returned')
        self.assertEqual(world.deletes[0], 'pod1')
        self.assertEqual(self.status()['reason'], 'startup_idle')
        self.assertEqual(self.completion()['absent_confirmations'], 2)

    def test_started_worker_defers_to_resident_guard_until_release_authorized(self):
        self.record(); self.started(); self.clock.t = 2000.
        world = World([pod()])
        self.assertIsNone(self.once(world)['reason'])
        self.assertEqual(world.deletes, [])
        write(self.root / 'release-authorized.json', {})
        self.assertEqual(self.once(world)['reason'], 'controller_release')
        self.assertEqual(world.deletes, ['pod1'])

    def test_deadline_releases_even_with_worker_running(self):
        self.record(); self.started(); self.clock.t = lease()['deadline_epoch'] - 60
        world = World([pod()])
        self.assertEqual(self.once(world)['reason'], 'deadline')
        self.assertEqual(world.deletes[0], 'pod1')

    def test_adopts_a_single_exact_name_pod_and_never_touches_other_names(self):
        world = World([pod(), {'id': 'other', 'name': 'someone-else', 'costPerHr': 0.5}])
        self.once(world)
        self.assertEqual(read(self.root / 'external-discovered-pod.json')['id'], 'pod1')
        self.clock.t = 2000.   # startup allowance over: the adopted Pod is released, the other never
        self.once(world)
        self.assertEqual(world.deletes, ['pod1'])
        self.assertIn('other', world.pods)

    def test_absent_pod_completes_after_deadline_without_provider_handle(self):
        self.clock.t = lease()['deadline_epoch']
        world = World([])
        self.assertEqual(self.loop(world), 'returned')
        self.assertEqual(world.methods(), [])
        self.assertTrue(self.completion()['provider_absent'])

    def test_unused_lease_completes_after_controller_completion(self):
        write(self.root / 'completion.json', {})
        self.clock.t = lease()['created_epoch'] + eg.CONTROLLER_DONE_SECONDS + 1
        self.assertEqual(self.loop(World([])), 'returned')
        self.assertEqual(self.completion()['absent_confirmations'], 2)

    def test_wrong_identity_at_recorded_id_is_a_guard_failure_that_releases(self):
        self.record(); self.started()
        world = World([pod(name='renamed')])
        self.assertEqual(self.once(world)['reason'], 'guard_failure')
        self.assertEqual(world.deletes, ['pod1'])
        self.assertEqual(read(self.root / 'external-guard-error.json')['error_type'], 'ValueError')

    def test_released_lease_exits_without_deleting(self):
        world = World([pod()]); self.record()
        self.assertEqual(self.loop(world, released=lambda: True), 'returned')
        self.assertEqual(world.calls, 0)


class DuplicateTests(Base):
    """#335 item 1: duplicate and late Pods of this lease are always deleted."""

    def test_duplicate_same_name_deleted_and_recorded_pod_kept(self):
        self.record('pod1'); self.started()
        world = World([pod('pod1'), pod('pod2')])
        self.once(world)
        self.assertEqual(world.deletes, ['pod2'])
        self.assertIn('pod1', world.pods)

    def test_two_unrecorded_same_name_pods_are_both_deleted_once_a_reason_exists(self):
        world = World([pod('x'), pod('y')])
        self.once(world)
        self.assertEqual(world.deletes, [])        # no reason yet: nothing is guessed
        self.clock.t = 2000.
        self.once(world)
        self.assertEqual(sorted(world.deletes), ['x', 'y'])

    def test_ambiguous_create_deletes_the_late_pod_and_never_completes_alone(self):
        write(self.root / 'lease/create-ambiguous.json', {'post_error': 'provider HTTP 503'})
        world = World([pod('late')])
        self.assertEqual(self.once(world)['reason'], 'ambiguous_create')
        self.assertEqual(world.deletes, ['late'])
        self.assertEqual(self.loop(world, polls=20), 'running')
        self.assertIsNone(self.completion())
        write(self.root / eg.OPERATOR_RESOLUTION, {'checked_by': 'operator'})
        self.clock.t = lease()['deadline_epoch']
        self.assertEqual(self.loop(world, polls=20), 'returned')


class PriceTests(Base):
    """#335 item 2: price parsing never blocks release; ceiling and adjustedCostPerHr apply."""

    def test_malformed_price_deletes_at_once(self):
        for bad in ('n/a', None, float('nan'), -1, True, {'x': 1}):
            with self.subTest(bad=bad):
                world = World([pod(costPerHr=bad)]); self.record(); self.started()
                self.assertEqual(self.once(world)['reason'], 'price_malformed')
                self.assertEqual(world.deletes, ['pod1'])

    def test_missing_price_deletes(self):
        missing = pod(); del missing['costPerHr']
        world = World([missing]); self.record(); self.started()
        self.assertEqual(self.once(world)['reason'], 'price_malformed')
        self.assertEqual(world.deletes, ['pod1'])

    def test_adjusted_price_above_ceiling_deletes(self):
        world = World([pod(adjustedCostPerHr=1.5)]); self.record(); self.started()
        self.assertEqual(self.once(world)['reason'], 'rate_above_ceiling')
        self.assertEqual(world.deletes, ['pod1'])

    def test_deadline_before_price_parsing(self):
        world = World([pod(costPerHr='garbage')]); self.record(); self.started()
        self.clock.t = lease()['deadline_epoch'] + 5
        self.assertEqual(self.once(world)['reason'], 'deadline')
        self.assertEqual(world.deletes[0], 'pod1')

    def test_failed_census_after_deadline_still_deletes_recorded(self):
        world = World([pod()]); world.list_fails = True; self.record(); self.started()
        self.clock.t = lease()['deadline_epoch'] + 5
        self.once(world)
        self.assertEqual(world.deletes[0], 'pod1')

    def test_failed_get_is_a_guard_failure_that_deletes(self):
        world = World([pod()]); world.get_failures = -1; self.record(); self.started()
        self.assertEqual(self.once(world)['reason'], 'guard_failure')
        self.assertEqual(world.deletes, ['pod1'])

    def test_healthy_pod_kept(self):
        world = World([pod()]); self.record(); self.started()
        self.assertIsNone(self.once(world)['reason'])
        self.assertEqual(world.deletes, [])


class CleanupTests(Base):
    """#335 item 4: DELETE retries are bounded by the lease, never gated on preservation."""

    def test_delete_failures_near_deadline_no_preservation_and_never_gives_up(self):
        world = World([pod()]); world.delete_failures = -1; self.record(); self.started()
        write(self.root / 'release-authorized.json', {})
        self.clock.t = lease()['deadline_epoch'] - 700     # under 15 min left: no preservation
        calls = []
        self.assertEqual(self.loop(world, polls=60, preserve=lambda pid, t: calls.append(t)), 'running')
        self.assertEqual(calls, [])
        self.assertGreaterEqual(len(world.deletes), 60)    # a DELETE every poll
        self.assertTrue((self.root / 'ALERT-pod-not-released.json').exists())

    def test_preservation_bounded_once_and_never_gates_delete(self):
        world = World([pod()]); world.delete_failures = -1; self.record(); self.started()
        write(self.root / 'release-authorized.json', {})
        self.clock.t = lease()['deadline_epoch'] - 1000
        calls = []

        def slow_failing_preserve(pod_id, timeout):
            calls.append(timeout); self.clock.advance(timeout)   # uses its whole budget, then fails
            raise OSError('scp failed')
        self.loop(world, polls=5, preserve=slow_failing_preserve)
        self.assertEqual(len(calls), 1)
        self.assertLessEqual(calls[0], min(eg.PRESERVE_MAX_SECONDS, 1000 - eg.PRESERVE_MARGIN))
        self.assertGreaterEqual(len(world.deletes), 5)

    def test_successful_delete_after_preservation_completes(self):
        world = World([pod()]); self.record(); self.started()
        write(self.root / 'release-authorized.json', {})
        calls = []
        self.assertEqual(self.loop(world, preserve=lambda pid, t: calls.append(t)), 'returned')
        self.assertEqual(len(calls), 1)
        self.assertEqual(world.deletes[0], 'pod1')


class AbsenceTests(Base):
    """#335 item 5: release completes only after two spaced absent observations."""

    def test_single_absent_observation_does_not_end(self):
        self.record(); self.started(); write(self.root / 'release-authorized.json', {})
        world = World([])     # census empty throughout; only the exact-ID GET flaps
        world.get_script = iter(['absent', 'present', 'absent', 'absent', 'absent', 'absent', 'absent'])
        self.assertEqual(self.loop(world), 'returned')
        self.assertEqual(self.completion()['absent_confirmations'], 2)
        # absent, present, then absent: completes only after 30 s of continuous absence
        self.assertGreaterEqual(self.clock.sleeps, 4)

    def test_one_absent_then_stop_leaves_no_completion(self):
        self.record(); self.started(); write(self.root / 'release-authorized.json', {})
        self.assertEqual(self.loop(World([]), polls=1), 'running')
        self.assertIsNone(self.completion())

    def test_failed_census_is_never_absence(self):
        self.record(); self.started(); write(self.root / 'release-authorized.json', {})
        world = World([]); world.list_fails = True
        self.assertEqual(self.loop(world, polls=10), 'running')
        self.assertIsNone(self.completion())


class StartupTests(Base):
    """#335 item 7: the startup timeout does not end productive work."""

    def test_pod_side_marker_keeps_productive_work(self):
        self.record(); self.clock.t = lease()['created_epoch'] + lease()['startup_idle_seconds'] + 1
        world = World([pod()])
        self.assertIsNone(self.once(world, marker_probe=lambda p: True)['reason'])
        self.assertTrue((self.root / 'worker-started.json').exists())
        self.assertEqual(world.deletes, [])

    def test_unreachable_probe_is_not_ready_and_the_allowance_still_ends_the_lease(self):
        self.record(); world = World([pod()])

        def unreachable(p):
            raise OSError('ssh unavailable')
        self.assertIsNone(self.once(world, marker_probe=unreachable)['reason'])   # inside the allowance
        self.clock.t = lease()['created_epoch'] + lease()['startup_idle_seconds'] + 1
        self.assertEqual(self.once(world, marker_probe=unreachable)['reason'], 'startup_idle')
        self.assertEqual(world.deletes, ['pod1'])

    def test_probe_is_rate_limited(self):
        self.record(); world = World([pod()]); probes = []
        self.loop(world, polls=8, marker_probe=lambda p: probes.append(1) or False)
        self.assertLessEqual(len(probes), 1 + 8 * eg.POLL_SECONDS // eg.MARKER_PROBE_SECONDS)


if __name__ == '__main__':
    unittest.main()
