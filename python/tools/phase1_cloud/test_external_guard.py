"""Offline coverage of the workstation-side guard. No provider calls or credentials.

Every run uses a fake clock with a hard sleep cap and a fake provider with a hard
call cap (lease_test_support); a guard that never finishes ends the test with Stop.
"""
import tempfile
import unittest
from pathlib import Path
from unittest import mock

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


class RetentionTests(Base):
    """Review r1 finding 1: every discovered or duplicate ID is retained until it alone is confirmed absent."""

    def test_adopted_pod_is_still_deleted_after_a_census_outage(self):
        world = World([pod()])
        self.once(world)                                   # adopted, retained durably
        self.assertIn('pod1', eg.known_pods(self.root))
        world.list_fails = True
        self.clock.t = lease()['deadline_epoch'] + 5
        self.once(world)                                   # a later pass (a fresh process would do the same)
        self.assertEqual(world.deletes[0], 'pod1')

    def test_retained_duplicate_missing_from_the_census_is_deleted_and_blocks_completion(self):
        self.record('pod1'); self.started(); write(self.root / 'release-authorized.json', {})
        eg.remember(self.root, 'dup2', 'duplicate', 1100.)
        world = World([pod('pod1'), pod('dup2')]); world.delete_failures = -1
        world.census = lambda current: [p for p in current if p['id'] != 'dup2']   # stale census omits dup2
        self.assertEqual(self.loop(world, polls=10), 'running')
        self.assertGreaterEqual(world.deletes.count('dup2'), 10)
        self.assertIsNone(self.completion())

    def test_each_retained_id_needs_its_own_two_absences(self):
        self.record('pod1'); self.started(); write(self.root / 'release-authorized.json', {})
        eg.remember(self.root, 'dup2', 'duplicate', 1100.)
        world = World([pod('pod1'), pod('dup2')])
        self.assertEqual(self.loop(world), 'returned')
        self.assertEqual(sorted(world.deletes[:2]), ['dup2', 'pod1'])
        self.assertEqual(self.completion()['known_pods'], ['dup2', 'pod1'])


class UnresolvedCreateTests(Base):
    """Review r1 finding 2: release authorization never proves that a create failed."""

    def test_timed_out_create_with_release_authorized_never_completes_and_deletes_a_late_pod(self):
        write(self.root / 'lease/create-attempt.json', {})
        write(self.root / 'lease/create-ambiguous.json', {'error_type': 'RuntimeError'})
        write(self.root / 'lease/create-pod-failure.json', {'definite': False})
        write(self.root / 'release-authorized.json', {})
        world = World([])
        self.assertEqual(self.loop(world, polls=10), 'running')
        self.assertIsNone(self.completion())
        world.pods['late'] = pod('late')                   # the delayed Pod appears
        self.loop(world, polls=3)
        self.assertIn('late', world.deletes)
        self.assertIsNone(self.completion())

    def test_attempt_without_receipt_blocks_completion_until_a_definite_refusal(self):
        write(self.root / 'lease/create-attempt.json', {})
        self.clock.t = lease()['deadline_epoch']
        self.assertEqual(self.loop(World([]), polls=10), 'running')
        self.assertIsNone(self.completion())
        write(self.root / 'lease/create-pod-failure.json', {'definite': True})
        self.assertEqual(self.loop(World([]), polls=10), 'returned')


class CensusAbsenceTests(Base):
    """Review r1 finding 3: absence needs a 404 for the exact ID AND a census that does not list it."""

    def test_census_listing_the_recorded_pod_is_never_absence(self):
        self.record(); self.started(); write(self.root / 'release-authorized.json', {})
        world = World([pod()]); world.delete_failures = -1
        world.get_script = iter(['absent'] * 100)          # GET says 404 while the census still lists pod1
        self.assertEqual(self.loop(world, polls=10), 'running')
        self.assertIsNone(self.completion())
        self.assertGreaterEqual(world.deletes.count('pod1'), 10)


class MalformedProviderDataTests(Base):
    """Review r1 finding 4: malformed provider data never stops the DELETE of known IDs."""

    def test_malformed_get_response_still_deletes(self):
        for raw in ([], b'text', 42, {'id': 'pod1'}):
            with self.subTest(raw=raw):
                self.record(); self.started()
                world = World([pod()]); world.get_script = iter([raw])
                status = self.once(world)
                self.assertIn(status['reason'], ('guard_failure', 'price_malformed'))
                self.assertEqual(world.deletes, ['pod1'])

    def test_malformed_census_is_a_failure_never_absence(self):
        for census in (lambda current: current + [{'name': NAME}], lambda current: current + ['junk'],
                       lambda current: {'pods': current}):
            with self.subTest(census=census):
                self.record(); self.started()
                world = World([pod()]); world.census = census
                self.assertEqual(self.once(world)['reason'], 'guard_failure')
                self.assertEqual(world.deletes, ['pod1'])
                world.pods.clear()
                self.assertEqual(self.loop(world, polls=5), 'running')
                self.assertIsNone(self.completion())

    def test_adopted_pod_is_deleted_at_the_deadline_despite_a_malformed_census(self):
        world = World([pod()])
        self.once(world)
        world.census = lambda current: [{'name': NAME}]
        self.clock.t = lease()['deadline_epoch'] + 5
        self.once(world)
        self.assertEqual(world.deletes[0], 'pod1')


class StudyTagTests(Base):
    """A study tag prefix is never ownership in the shared guard (no ownership source exists here)."""

    def test_tag_prefixed_pod_of_another_lease_is_never_a_target(self):
        write(self.root / 'lease/lease.json', lease() | {'study_tag': 'phase1-qual'}, replace=True)
        other = {'id': 'other', 'name': 'phase1-qual-other-lease', 'costPerHr': 0.96}
        world = World([other])
        self.clock.t = lease()['deadline_epoch'] + 5
        self.assertEqual(self.loop(world), 'returned')
        self.assertEqual(world.deletes, [])
        self.assertIn('other', world.pods)
        self.assertEqual(eg.known_pods(self.root), {})


class ReviewR2Tests(Base):
    """Round 2 review findings 1 to 3."""

    def confirmed_duplicate(self, world):
        self.record('pod1'); self.started()
        eg.remember(self.root, 'dup2', 'duplicate', 1100.)
        self.assertEqual(self.loop(world, polls=4), 'running')    # 60 s: dup2 confirmed absent, pod1 healthy
        self.assertIn('dup2', self.status()['confirmed_absent'])

    def test_confirmed_duplicate_that_reappears_in_the_census_is_deleted_again(self):
        world = World([pod('pod1')]); self.confirmed_duplicate(world)
        world.pods['dup2'] = pod('dup2')
        self.loop(world, polls=1)
        self.assertIn('dup2', world.deletes)

    def test_confirmed_duplicate_returned_by_get_only_is_deleted_again(self):
        world = World([pod('pod1')]); self.confirmed_duplicate(world)
        world.census = lambda current: [p for p in current if p['id'] != 'dup2']
        world.pods['dup2'] = pod('dup2')
        self.loop(world, polls=1)
        self.assertIn('dup2', world.deletes)

    def test_unreadable_store_record_keeps_its_id_and_blocks_completion(self):
        self.record('pod1'); self.started(); write(self.root / 'release-authorized.json', {})
        eg.remember(self.root, 'dup2', 'duplicate', 1100.)
        (self.root / eg.KNOWN_DIR / 'dup2.json').write_text('{broken')
        world = World([pod('pod1'), pod('dup2')])
        world.census = lambda current: [p for p in current if p['id'] != 'dup2']
        self.assertEqual(self.loop(world, polls=10), 'running')
        self.assertIn('dup2', world.deletes)
        self.assertIn('pod1', world.deletes)
        self.assertIsNone(self.completion())

    def test_known_pods_reads_records_individually(self):
        for pod_id in ('a1', 'b2'):
            eg.remember(self.root, pod_id, 'duplicate', 1100.)
        (self.root / eg.KNOWN_DIR / 'a1.json').write_text('[]')
        errors = []
        known = eg.known_pods(self.root, errors)
        self.assertEqual(sorted(known), ['a1', 'b2'])
        self.assertIsNone(known['a1'])
        self.assertEqual(known['b2'], 'duplicate')
        self.assertEqual(len(errors), 1)
        with self.assertRaises(ValueError):
            eg.known_pods(self.root)

    def test_census_entry_with_changed_or_missing_name_is_presence(self):
        for entry in ({'id': 'pod1', 'name': 'renamed'}, {'id': 'pod1'}):
            with self.subTest(entry=entry):
                self.record(); self.started(); write(self.root / 'release-authorized.json', {}, replace=True)
                world = World([pod()]); world.delete_failures = -1
                world.get_script = iter(['absent'] * 100)      # GET says 404 while the census lists pod1
                world.census = lambda current, entry=entry: [dict(entry)]
                self.assertEqual(self.loop(world, polls=10), 'running')
                self.assertIsNone(self.completion())
                self.assertGreaterEqual(world.deletes.count('pod1'), 10)


class InvariantTests(Base):
    """Round 3 review: I1 retain first, I2 fail closed, I3 primary recovery."""

    def test_unreadable_receipt_never_resolves_the_create_or_permits_completion(self):
        # finding 2 (I2): receipt unreadable before the guard retained its ID, census omits the Pod
        write(self.root / 'lease/create-attempt.json', {})
        (self.root / 'lease/created-pod.json').write_text('{broken')
        write(self.root / 'release-authorized.json', {})
        self.assertTrue(eg.create_unresolved(self.root))
        self.clock.t = lease()['deadline_epoch']
        self.assertEqual(self.loop(World([]), polls=10), 'running')
        self.assertIsNone(self.completion())
        self.assertIn('receipt:', ','.join(self.status()['store_errors']))

    def test_retained_receipt_record_is_the_primary_without_created_pod_json(self):
        # finding 4 (I3): price checks and release apply to the retained create result
        eg.remember(self.root, 'pod1', 'receipt', 1100.)
        self.started()
        world = World([pod('pod1', costPerHr=5.0)])
        status = self.once(world)
        self.assertEqual(status['pod_id'], 'pod1')
        self.assertEqual(status['reason'], 'rate_above_ceiling')
        self.assertEqual(world.deletes, ['pod1'])

    def test_receipt_role_outranks_an_earlier_census_record(self):
        eg.remember(self.root, 'pod1', 'census', 1100.)
        eg.remember(self.root, 'pod1', 'receipt', 1101.)
        self.assertEqual(eg.known_pods(self.root)['pod1'], 'receipt')

    def test_fallback_id_in_an_ambiguity_record_is_retained_and_guarded(self):
        # finding 5 (I1): the create could not retain or write its receipt; the ID survives in the record
        write(self.root / 'lease/create-attempt.json', {})
        write(self.root / 'lease/create-ambiguous.json', {'pod_id': 'pod1'})
        self.started()
        world = World([pod('pod1', costPerHr='bad')])
        world.census = lambda current: []                           # census omits the Pod
        status = self.once(world)
        self.assertEqual(status['pod_id'], 'pod1')
        self.assertEqual(status['reason'], 'price_malformed')
        self.assertEqual(world.deletes, ['pod1'])
        self.assertEqual(eg.known_pods(self.root)['pod1'], 'receipt')

    def test_failed_retention_write_is_a_store_failure_that_blocks_completion(self):
        # I1: the ID is kept in memory; the pass counts as a store failure
        self.record('pod1'); self.started(); write(self.root / 'release-authorized.json', {})
        world = World([pod('pod1')])
        with mock.patch.object(eg, 'handoff', side_effect=OSError('read-only store')):
            self.assertEqual(self.loop(world, polls=10), 'running')
        self.assertEqual(world.deletes[0], 'pod1')
        self.assertIsNone(self.completion())
        self.assertTrue(any(e.startswith('retain:pod1') for e in self.status()['store_errors']))


class EvidenceTests(Base):
    """Round 4, I5: absence is established only by evidence observed in the current process."""

    def hide_dup2_and_fail_gets(self, world):
        world.census = lambda current: [p for p in current if p['id'] != 'dup2']
        world.get_failures = -1
        write(self.root / 'release-authorized.json', {}, replace=True)

    def test_restart_after_a_reappearance_needs_fresh_evidence(self):
        # finding 1: process 1 confirms dup2 absent; it reappears and its DELETE fails (process 2);
        # after a restart (process 3) the census omits it and GETs fail: it stays a target
        self.record('pod1'); self.started()
        eg.remember(self.root, 'dup2', 'duplicate', 1100.)
        world = World([pod('pod1')])
        self.assertEqual(self.loop(world, polls=4), 'running')
        self.assertIn('dup2', self.status()['confirmed_absent'])
        world.pods['dup2'] = pod('dup2'); world.delete_failures = -1
        self.loop(world, polls=1)
        self.hide_dup2_and_fail_gets(world)
        self.assertEqual(self.loop(world, polls=10), 'running')
        self.assertIsNone(self.completion())
        self.assertNotIn('dup2', self.status()['confirmed_absent'])
        self.assertGreaterEqual(world.deletes.count('dup2'), 10)

    def test_persisted_absence_markers_are_never_evidence(self):
        # finding 1: a marker of an earlier version, readable or not, never confirms absence
        self.record('pod1'); self.started()
        eg.remember(self.root, 'dup2', 'duplicate', 1100.)
        folder = self.root / eg.KNOWN_DIR
        for text in ('{"id": "dup2", "absent_confirmations": 2}', '{broken'):
            with self.subTest(marker=text):
                (folder / 'dup2.absent.json').write_text(text)
                world = World([pod('pod1'), pod('dup2')]); world.delete_failures = -1
                self.hide_dup2_and_fail_gets(world)
                self.assertEqual(self.loop(world, polls=5), 'running')
                self.assertNotIn('dup2', self.status()['confirmed_absent'])
                self.assertGreaterEqual(world.deletes.count('dup2'), 5)

    def test_a_restarted_guard_completes_only_after_fresh_spaced_absences(self):
        self.record('pod1'); self.started(); write(self.root / 'release-authorized.json', {})
        world = World([pod('pod1')])
        self.assertEqual(self.loop(world), 'returned')
        (self.root / 'external-guard-completion.json').unlink()
        start = self.clock.t
        self.assertEqual(self.loop(world), 'returned')                   # a restart over the same records
        self.assertGreaterEqual(self.clock.t - start, 30)


class PersistenceTests(Base):
    """Round 4 finding 2 (guard side), I2 and I4: an ID whose record is not durable is retried every pass."""

    def test_failed_census_id_write_is_retried_and_blocks_completion_until_durable(self):
        self.record('pod1'); self.started(); write(self.root / 'release-authorized.json', {})
        world = World([pod('pod1'), pod('dup2')])
        real = eg.handoff; failures = [6]; durable_at = []

        def flaky(root, pod_id, role, epoch):
            if pod_id == 'dup2' and failures[0]:
                failures[0] -= 1
                raise OSError('disk full')
            if pod_id == 'dup2' and not durable_at:
                durable_at.append(self.clock.t)
            return real(root, pod_id, role, epoch)
        with mock.patch.object(eg, 'handoff', side_effect=flaky):
            self.assertEqual(self.loop(world, polls=40), 'returned')       # one process: retried until durable
        # both Pods were absent from the second pass (1115 s), so only the unpersisted dup2 delayed completion
        self.assertGreater(durable_at[0], 1115 + 30)
        self.assertGreaterEqual(self.completion()['epoch'], durable_at[0])
        self.assertEqual(eg.known_pods(self.root)['dup2'], 'census')
        self.assertIn('dup2', world.deletes)

    def test_unpersisted_id_blocks_completion_while_the_store_stays_unwritable(self):
        self.record('pod1'); self.started(); write(self.root / 'release-authorized.json', {})
        world = World([pod('pod1'), pod('dup2')])
        real = eg.handoff

        def never_dup2(root, pod_id, role, epoch):
            if pod_id == 'dup2':
                raise OSError('disk full')
            return real(root, pod_id, role, epoch)
        with mock.patch.object(eg, 'handoff', side_effect=never_dup2):
            self.assertEqual(self.loop(world, polls=10), 'running')
        self.assertIsNone(self.completion())
        self.assertIn('dup2', world.deletes)


class ReleaseUnhandedTests(unittest.TestCase):
    """I4: a creator's own release loop deletes and verifies absence from fresh evidence, bounded by time."""

    def run_release(self, world, deadline_after=120):
        clock = FakeClock(1000.)
        with no_network():
            pending = eg.release_unhanded('k', ['A'], NAME, 1000. + deadline_after, provider=world.provider,
                                          pods=world.list, clock=clock, sleep=clock.sleep)
        return pending, clock

    def test_deletes_and_needs_two_spaced_absences(self):
        world = World([pod('A')])
        pending, clock = self.run_release(world)
        self.assertEqual(pending, [])
        self.assertEqual(world.deletes[0], 'A')
        self.assertGreaterEqual(clock.t - 1000., 30)

    def test_census_presence_or_failed_gets_never_confirm_and_time_bounds_the_loop(self):
        for setup in (lambda w: setattr(w, 'get_failures', -1),
                      lambda w: setattr(w, 'census', lambda current: [{'id': 'A', 'name': 'renamed'}])):
            with self.subTest(setup=setup):
                world = World([pod('A')]); setup(world)
                pending, clock = self.run_release(world)
                self.assertEqual(pending, ['A'])
                self.assertLessEqual(clock.t, 1000. + 120 + eg.RELEASE_POLL_SECONDS)


if __name__ == '__main__':
    unittest.main()
