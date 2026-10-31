"""Offline coverage of the workstation-side guard. No provider calls or credentials.

Every run uses a fake clock with a hard sleep cap and a fake provider with a hard
call cap (lease_test_support); a guard that never finishes ends the test with Stop.
"""
import tempfile
import threading
import time
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


def wait_for(path, seconds=5.0):
    """Real time, for a record a helper thread writes."""
    end = time.monotonic() + seconds
    while not path.exists() and time.monotonic() < end:
        time.sleep(0.01)
    return path.exists()


def failing_handoff(ids, first=None, first_only_once=False):
    """Handoffs of `ids` fail with OSError; with `first`, the first such call per ID (or, with
    `first_only_once`, only the very first call) raises that instead."""
    real = eg.handoff_in; fired = []

    def handoff(folder, pod_id, role, epoch, **options):
        if pod_id in ids:
            if first is not None and pod_id not in fired and not (first_only_once and fired):
                fired.append(pod_id)
                raise first()
            raise OSError('read-only store')
        return real(folder, pod_id, role, epoch, **options)
    return mock.patch.object(eg, 'handoff_in', side_effect=handoff)


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

    def test_persisted_release_records_never_end_the_guard(self):
        # round 5 finding 1 (I5): a verify-absent.json or old completion is information only
        self.record(); self.started(); write(self.root / 'release-authorized.json', {})
        write(self.root / 'verify-absent.json', {'released': True})
        write(self.root / 'external-guard-completion.json', {'absent_confirmations': 2})
        world = World([pod()])
        self.once(world)
        self.assertGreater(world.calls, 0)
        # the completion record closes the store, so this pass verifies the release itself (repeated DELETEs)
        self.assertEqual(set(world.deletes), {'pod1'})
        self.assertNotIn('pod1', world.pods)


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


class QuietCreateTests(Base):
    """A create that named no Pod is never resolved by the guard: after unbroken quiet it writes diagnostic
    evidence and an ALERT for the operator and keeps supervising."""

    def ambiguous(self, operator=False):
        write(self.root / 'lease/create-attempt.json', {})
        write(self.root / 'lease/create-ambiguous.json', {'error_type': 'RuntimeError', 'pod_id': None})
        write(self.root / 'lease/create-pod-failure.json', {'definite': False})
        write(self.root / 'lease/funding-at-create.json', {'account_spend_usd_hour': 0.05})
        write(self.root / 'completion.json', {'complete': False})
        if operator:
            write(self.root / eg.OPERATOR_RESOLUTION, {'checked_by': 'operator'})

    def polls(self):
        return int(eg.AMBIGUOUS_QUIET_SECONDS / eg.POLL_SECONDS) + 20

    def steady(self, step=eg.POLL_SECONDS):
        """A monotonic clock that advances `step` per pass, whatever the wall clock does."""
        ticks = iter(range(0, 10**9, int(step)))
        return lambda: float(next(ticks))

    def test_quiet_writes_evidence_and_an_alert_and_never_completes(self):
        self.ambiguous()
        world = World([])
        funds = world.provider('k', 'funding').funds
        self.assertEqual(self.loop(world, polls=self.polls() + 40, funds=funds, monotonic=self.clock), 'running')
        quiet = read(self.root / eg.QUIET_NOTE)
        self.assertGreaterEqual(quiet['quiet_seconds'], eg.AMBIGUOUS_QUIET_SECONDS)
        self.assertEqual((quiet['retained_ids'], quiet['account_now']), ([], eg.QUIET_FUNDS))
        self.assertTrue(wait_for(self.root / eg.QUIET_FUNDS))
        self.assertEqual(read(self.root / eg.QUIET_FUNDS)['account_now']['account_spend_usd_hour'], 0.05)
        self.assertEqual(quiet['account_before_create'], {'account_spend_usd_hour': 0.05})
        self.assertTrue((self.root / eg.QUIET_ALERT).exists())
        self.assertIsNone(self.completion())
        self.assertFalse((self.root / eg.OPERATOR_RESOLUTION).exists())
        self.assertEqual(world.funds_calls, 1)                   # once per quiet window, diagnostic only
        self.assertTrue(self.status()['create_unresolved'])

    def census_failing_at(self, world, calls):
        count = [0]

        def census(current):
            count[0] += 1
            if count[0] in calls:
                raise RuntimeError('census HTTP 503')
            return current
        world.census = census

    def test_a_failed_census_inside_one_run_starts_the_window_again(self):
        self.ambiguous()
        world = World([])
        self.census_failing_at(world, {self.polls() - 30})
        self.assertEqual(self.loop(world, polls=self.polls() + 60, monotonic=self.clock), 'running')
        self.assertFalse((self.root / eg.QUIET_NOTE).exists())
        self.assertEqual(self.loop(world, polls=self.polls(), monotonic=self.clock), 'running')
        self.assertTrue((self.root / eg.QUIET_NOTE).exists())    # a fresh unbroken window does note

    def test_gaps_and_wall_clock_jumps_never_shorten_the_window(self):
        self.ambiguous()
        world = World([])
        jumps = [0]

        def census(current):                                       # the wall clock leaps an hour each pass
            jumps[0] += 1; self.clock.t += 3600
            return current
        world.census = census
        self.loop(world, polls=10, monotonic=self.steady())
        self.assertFalse((self.root / eg.QUIET_NOTE).exists())
        world.census = None
        self.loop(world, polls=self.polls() + 40, monotonic=self.steady(eg.QUIET_GAP_SECONDS + 1))
        self.assertFalse((self.root / eg.QUIET_NOTE).exists())   # every observation follows a gap

    def test_an_unnamed_census_entry_is_malformed_never_quiet(self):
        self.ambiguous()
        world = World([])
        world.census = lambda current: [{'id': 'ours'}]
        self.assertEqual(self.loop(world, polls=self.polls() + 20, monotonic=self.clock), 'running')
        self.assertIn('census:malformed', self.status()['failures'])
        self.assertFalse((self.root / eg.QUIET_NOTE).exists())
        self.assertEqual(eg.census_entries([{'id': 'ours'}], NAME), ([], {'ours'}, True))
        for blank in ('', '   '):
            self.assertEqual(eg.census_entries([{'id': 'ours', 'name': blank}], NAME), ([], {'ours'}, True))
        self.assertEqual(eg.census_entries([{'id': 'theirs', 'name': 'other-lane'}], NAME), ([], {'theirs'}, False))
        world.census = lambda current: [{'id': 'ours', 'name': ''}]
        self.assertEqual(self.loop(world, polls=self.polls() + 20, monotonic=self.clock), 'running')
        self.assertFalse(self.status()['absent_now'])
        self.assertFalse((self.root / eg.QUIET_NOTE).exists())

    def test_the_alert_never_waits_for_the_account_read(self):
        self.ambiguous()
        world = World([])
        release, returned = threading.Event(), threading.Event()

        def funds():                                      # a hung account read
            release.wait(10); returned.set()
            return {'account_spend_usd_hour': 0.07}
        self.assertEqual(self.loop(world, polls=self.polls() + 40, funds=funds, monotonic=self.clock), 'running')
        self.assertFalse(returned.is_set())               # every pass ran while the read was held
        self.assertTrue((self.root / eg.QUIET_ALERT).exists())
        self.assertEqual(read(self.root / eg.QUIET_NOTE)['account_now'], eg.QUIET_FUNDS)
        self.assertFalse((self.root / eg.QUIET_FUNDS).exists())
        release.set()
        self.assertTrue(wait_for(self.root / eg.QUIET_FUNDS))
        self.assertEqual(read(self.root / eg.QUIET_FUNDS)['account_now'], {'account_spend_usd_hour': 0.07})

    def test_a_failed_thread_start_never_ends_supervision(self):
        self.ambiguous()
        world = World([])

        class Unstartable:
            def __init__(self, *args, **kwargs):
                pass

            def start(self):
                raise RuntimeError('cannot start new thread')
        with mock.patch.object(eg, 'threading', mock.Mock(Thread=Unstartable)):
            self.assertEqual(self.loop(world, polls=self.polls() + 40, funds=world.provider('k', 'funding').funds,
                                       monotonic=self.clock), 'running')
        self.assertTrue((self.root / eg.QUIET_ALERT).exists())
        self.assertEqual(read(self.root / eg.QUIET_FUNDS)['error_type'], 'RuntimeError')

    def test_a_failed_alert_write_is_retried_without_a_second_account_read(self):
        self.ambiguous()
        world = World([])
        real = eg.write; failed = []

        def flaky(path, value, replace=False):
            if Path(path).name == eg.QUIET_ALERT and not failed:
                failed.append(path)
                raise OSError('disk busy')
            return real(path, value, replace)
        with mock.patch.object(eg, 'write', side_effect=flaky):
            self.assertEqual(self.loop(world, polls=self.polls() + 40, funds=world.provider('k', 'funding').funds,
                                       monotonic=self.clock), 'running')
        self.assertTrue(failed)
        self.assertTrue((self.root / eg.QUIET_ALERT).exists())
        self.assertTrue(wait_for(self.root / eg.QUIET_FUNDS))
        self.assertEqual(world.funds_calls, 1)


class StoreClosureTests(Base):
    """Completion is the store's closure, written under the lock every handoff and create attempt takes (I4)."""

    def complete(self, at_deadline=False):
        if at_deadline:
            self.clock.t = lease()['deadline_epoch']
        else:                                             # an unused lease, well before its deadline
            write(self.root / 'completion.json', {})
            self.clock.t = lease()['created_epoch'] + eg.CONTROLLER_DONE_SECONDS + 1
        self.assertEqual(self.loop(World([])), 'returned')
        self.clock.max_sleeps = 400; self.clock.sleeps = 0
        return self.completion()

    def injecting(self, inject):
        """complete_store, with `inject(root, epoch)` run once just before its first final check."""
        real = eg.complete_store; injected = []

        def complete(root, memory, basis, settled, errors, record):
            if not injected:
                injected.append(inject(root, record['epoch']))
            return real(root, memory, basis, settled, errors, record)
        return mock.patch.object(eg, 'complete_store', side_effect=complete), injected

    def test_completion_rereads_the_store_under_its_lock(self):
        QuietCreateTests.ambiguous(self, operator=True)
        world = World([])
        patch, injected = self.injecting(lambda root, epoch: eg.remember(root, 'late', 'census', epoch))
        with patch:
            self.assertEqual(self.loop(world, polls=200, monotonic=self.clock), 'returned')
        self.assertEqual(len(injected), 1)
        self.assertIn('late', self.completion()['known_pods'])     # completed only after confirming it absent
        self.assertIn(('GET', 'late'), world.log)

    def test_renewed_presence_of_a_known_id_voids_its_confirmations(self):
        self.clock.t = lease()['deadline_epoch']
        eg.remember(self.root, 'a', 'census', 1.0)
        world = World([])

        def seen_alive(root, epoch):                      # another process saw 'a' alive and handed it off
            eg.handoff(root, 'a', 'census', epoch)
            return len(world.log)
        patch, injected = self.injecting(seen_alive)
        with patch:
            self.assertEqual(self.loop(world, polls=200), 'returned')
        self.assertGreaterEqual(world.log[injected[0]:].count(('GET', 'a')), 2)   # two fresh absences
        self.assertEqual(self.completion()['presence_revisions']['a'], 1)

    def test_a_receipt_written_before_the_final_check_blocks_completion(self):
        write(self.root / 'completion.json', {})
        self.clock.t = lease()['created_epoch'] + eg.CONTROLLER_DONE_SECONDS + 1
        world = World([])

        def receipt_only(root, epoch):                    # the creator's handoff failed, its receipt landed
            write(root / 'lease/create-attempt.json', {'epoch': epoch})
            write(root / 'lease/created-pod.json', {'id': 'r', 'name': NAME})
        patch, injected = self.injecting(receipt_only)
        with patch:
            self.assertEqual(self.loop(world, polls=200), 'returned')
        self.assertIn('r', self.completion()['known_pods'])
        self.assertIn(('GET', 'r'), world.log)

    def store_failing_inside_complete(self, failure):
        real_complete, real_known = eg.complete_store, eg.known_pods; inside = [False]

        def complete(*args):
            inside[0] = True
            try:
                return real_complete(*args)
            finally:
                inside[0] = False

        def known(root, errors=None):
            if inside[0] and failure == 'raise':
                raise OSError('store unreadable')
            result = real_known(root, errors)
            if inside[0] and errors is not None:
                errors.append('late:ValueError')          # an unreadable record seen by the final check
            return result
        return mock.patch.object(eg, 'complete_store', side_effect=complete), \
            mock.patch.object(eg, 'known_pods', side_effect=known)

    def test_a_store_error_at_the_final_check_blocks_completion(self):
        self.clock.t = lease()['deadline_epoch']
        for failure, error in (('raise', 'complete:OSError'), ('record', 'late:ValueError')):
            first, second = self.store_failing_inside_complete(failure)
            with first, second:
                self.assertEqual(self.loop(World([]), polls=20), 'running')
            self.assertIsNone(self.completion())
            self.assertIn(error, self.status()['store_errors'])
            self.assertFalse(eg.store_closed(self.root / eg.KNOWN_DIR))

    def test_a_failed_completion_write_leaves_the_store_open_and_is_retried(self):
        real = eg.write; failed = []

        def flaky(path, value, replace=False):
            if Path(path).name == eg.COMPLETION and not failed:
                failed.append(path)
                raise OSError('disk busy')
            return real(path, value, replace)
        with mock.patch.object(eg, 'write', side_effect=flaky):
            self.complete()
        self.assertTrue(failed)
        self.assertTrue(self.completion()['provider_absent'])

    def test_a_handoff_or_create_attempt_after_completion_is_refused(self):
        self.complete()
        self.assertTrue(eg.store_closed(self.root / eg.KNOWN_DIR))
        with self.assertRaises(eg.StoreClosed):
            eg.handoff(self.root, 'late', 'census', 1.0)
        self.assertFalse((self.root / eg.KNOWN_DIR / 'late.json').exists())
        with self.assertRaises(eg.StoreClosed):
            eg.record_create_attempt(self.root / 'lease', {'epoch': 1.0})
        self.assertFalse((self.root / 'lease/create-attempt.json').exists())

    def test_a_watchdog_pass_after_completion_releases_what_it_finds(self):
        before = self.complete()
        world = World([pod('late')])
        self.once(world)
        self.assertIn(('DELETE', 'late'), world.log)
        self.assertNotIn('late', world.pods)
        self.assertFalse((self.root / eg.KNOWN_DIR / 'late.json').exists())
        self.assertFalse((self.root / 'ALERT-pod-not-released.json').exists())
        self.assertEqual(self.completion(), before)                 # the completion record stands

    def test_a_watchdog_pass_after_completion_that_cannot_delete_alerts(self):
        self.complete(at_deadline=True)                   # the release bound is then this pass's own
        world = World([pod('late')]); world.delete_failures = -1
        with self.assertRaises(eg.AlertError):
            self.once(world)
        self.assertEqual(read(self.root / 'ALERT-pod-not-released.json')['pod_ids'], ['late'])

    def test_a_closed_pass_must_verify_every_stored_id_it_cannot_see(self):
        self.complete(at_deadline=True)
        eg.remember(self.root, 'a', 'census', 1.0)        # a record from before the closure
        world = World([]); world.get_failures = -1; world.delete_failures = -1
        with self.assertRaises(eg.AlertError):
            self.once(world)
        self.assertEqual(read(self.root / 'ALERT-pod-not-released.json')['pod_ids'], ['a'])

    def test_a_cached_handoff_is_void_once_the_store_closes(self):
        eg.remember(self.root, 'a', 'census', 1.0)
        world = World([pod('a')])
        with no_network(), eg.LeaseClient(self.root / eg.KNOWN_DIR, NAME, 'k', world.provider, world.list,
                                          self.clock, self.clock.sleep, self.root / 'ALERT-pod-not-released.json',
                                          min_release_seconds=180) as client:
            self.assertNotIn('a', client.unpersisted)     # handed off before the closure
            write(self.root / eg.COMPLETION, {'provider_absent': True})
            client.census()
            self.assertIn('a', client.unpersisted)        # presence after closure is this process's again
        self.assertIn(('DELETE', 'a'), world.log)
        self.assertNotIn('a', world.pods)

    def test_a_continuous_guard_started_after_completion_releases_and_ends(self):
        self.complete()
        world = World([pod('new')])
        self.assertEqual(self.loop(world), 'returned')
        self.assertNotIn('new', world.pods)
        self.assertFalse((self.root / 'ALERT-pod-not-released.json').exists())

    def lease_client(self, world):
        return eg.LeaseClient(self.root / eg.KNOWN_DIR, NAME, 'k', world.provider, world.list, self.clock,
                              self.clock.sleep, self.root / 'ALERT-pod-not-released.json', min_release_seconds=180)

    def test_closure_inside_a_presence_handoff_keeps_the_obligation(self):
        eg.remember(self.root, 'a', 'census', 1.0)        # handed off earlier; the client caches it
        world = World([pod('a')])
        real = eg.handoff_in

        def completion_wins(folder, pod_id, role, epoch, **options):
            write(self.root / eg.COMPLETION, {'provider_absent': True})   # completion takes the lock first
            return real(folder, pod_id, role, epoch, **options)
        with no_network(), self.lease_client(world) as client, \
                mock.patch.object(eg, 'handoff_in', side_effect=completion_wins):
            client.census()
            self.assertIn('a', client.unpersisted)
        self.assertIn(('DELETE', 'a'), world.log)
        self.assertNotIn('a', world.pods)

    def test_a_failed_presence_handoff_is_never_discharged_by_an_older_record(self):
        for failure in ('presence write', 'lock timeout'):
            with self.subTest(failure=failure):
                eg.remember(self.root, 'a', 'census', 1.0)
                world = World([pod('a')])
                real_write = eg.write

                def no_presence(path, value, replace=False):
                    if Path(path).name.endswith('.presence.json'):
                        raise OSError('disk full')
                    return real_write(path, value, replace)
                if failure == 'presence write':
                    failing = mock.patch.object(eg, 'write', side_effect=no_presence)
                else:
                    failing = mock.patch.object(eg, 'STORE_LOCK_SECONDS', 0.1)
                lock = eg.StoreLock(self.root / eg.KNOWN_DIR) if failure == 'lock timeout' else mock.MagicMock()
                with self.assertRaises(eg.AlertError), no_network(), failing, lock, self.lease_client(world) as client:
                    client.census()
                    self.assertFalse(client.durable_now('a'))
                self.assertEqual(read(self.root / 'ALERT-pod-not-released.json')['pod_ids'], ['a'])
                self.assertNotIn('a', world.deletes)          # the guardian's Pod is reported, never deleted
                (self.root / 'ALERT-pod-not-released.json').unlink()

    def test_a_corrupt_stored_record_is_held_on_a_closed_store(self):
        self.complete(at_deadline=True)
        (self.root / eg.KNOWN_DIR).mkdir(parents=True, exist_ok=True)
        (self.root / eg.KNOWN_DIR / 'a.json').write_text('{not json')
        world = World([]); world.get_failures = -1; world.delete_failures = -1
        with self.assertRaises(eg.AlertError):
            self.once(world)
        self.assertEqual(read(self.root / 'ALERT-pod-not-released.json')['pod_ids'], ['a'])

    def test_a_forged_completion_never_ends_supervision_of_an_unresolved_create(self):
        write(self.root / 'lease/create-attempt.json', {'epoch': 1.0})
        write(self.root / eg.COMPLETION, {'provider_absent': True, 'forged': True})
        world = World([])
        self.assertEqual(self.loop(world, polls=20), 'running')
        world.pods['late'] = pod('late')                    # the pending POST materializes
        self.assertEqual(self.loop(world, polls=20), 'running')
        self.assertIn(('DELETE', 'late'), world.log)

    def test_a_closure_seen_mid_run_ends_the_guard_through_the_bounded_exit(self):
        for case in ('another guard completed', 'readback failed'):
            with self.subTest(case=case):
                temporary = tempfile.TemporaryDirectory(); self.addCleanup(temporary.cleanup)
                self.root = Path(temporary.name); write(self.root / 'lease/lease.json', lease())
                self.clock = FakeClock(lease()['deadline_epoch'])
                world = World([])
                if case == 'another guard completed':
                    calls = [0]

                    def census(current):
                        calls[0] += 1
                        if calls[0] == 2:
                            write(self.root / eg.COMPLETION, {'provider_absent': True, 'by': 'other'})
                        return current
                    world.census = census
                    self.assertEqual(self.loop(world, polls=20), 'returned')
                    self.assertEqual(self.completion()['by'], 'other')     # never overwritten
                else:
                    real_read = eg.read; failed = []

                    def flaky(path):
                        if Path(path).name == eg.COMPLETION and not failed:
                            failed.append(path)
                            raise OSError('read failed')
                        return real_read(path)
                    with mock.patch.object(eg, 'read', side_effect=flaky):
                        self.assertEqual(self.loop(world, polls=20), 'returned')
                    self.assertTrue(failed)

    def test_an_interrupt_while_classifying_a_census_loses_no_id(self):
        eg.remember(self.root, 'a', 'census', 1.0)        # named already; 'n' is brand new
        world = World([pod('a'), pod('n')])
        real = eg.LeaseClient.receipt_ids; fired = []

        def interrupted(client):
            if not fired:
                fired.append(True)
                raise KeyboardInterrupt()
            return real(client)
        with self.assertRaises(KeyboardInterrupt), no_network(), failing_handoff({'a', 'n'}), \
                self.lease_client(world) as client:
            with mock.patch.object(eg.LeaseClient, 'receipt_ids', interrupted):   # inside the census only
                client.census()
        self.assertEqual(fired, [True])
        self.assertIn('n', world.deletes)                 # brand new and never handed off: released
        self.assertNotIn('a', world.deletes)              # named: never deleted here, reported instead
        self.assertIn('a', read(self.root / 'ALERT-pod-not-released.json')['pod_ids'])

    def test_a_closure_mid_pass_holds_stored_ids_the_census_omits(self):
        self.clock.t = lease()['deadline_epoch']
        eg.remember(self.root, 'a', 'census', 1.0)
        world = World([pod('a')]); world.get_failures = -1; world.delete_failures = -1

        def census(current):                              # another guard completes during this census
            write(self.root / eg.COMPLETION, {'provider_absent': True, 'by': 'other'})
            return []
        world.census = census
        with self.assertRaises(eg.AlertError):
            self.loop(world, polls=200)                   # the closed exit's own 180 s release bound
        self.assertEqual(read(self.root / 'ALERT-pod-not-released.json')['pod_ids'], ['a'])

    def test_a_handoff_waits_for_the_store_lock_and_fails_closed_after_the_bound(self):
        folder = self.root / eg.KNOWN_DIR; done = []
        with eg.StoreLock(folder):
            worker = threading.Thread(target=lambda: done.append(eg.handoff(self.root, 'late', 'census', 1.0)))
            worker.start(); worker.join(0.3)
            self.assertEqual(done, [])
            self.assertFalse((folder / 'late.json').exists())
        worker.join(5)
        self.assertEqual(done, [True])
        with eg.StoreLock(folder), mock.patch.object(eg, 'STORE_LOCK_SECONDS', 0.2):
            with self.assertRaises(OSError):
                eg.handoff(self.root, 'later', 'census', 1.0)
        self.assertFalse((folder / 'later.json').exists())


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
        with mock.patch.object(eg, 'handoff_in', side_effect=OSError('read-only store')):
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
        real = eg.handoff_in; failures = [6]; durable_at = []

        def flaky(root, pod_id, role, epoch, **options):
            if pod_id == 'dup2' and failures[0]:
                failures[0] -= 1
                raise OSError('disk full')
            if pod_id == 'dup2' and not durable_at:
                durable_at.append(self.clock.t)
            return real(root, pod_id, role, epoch, **options)
        with mock.patch.object(eg, 'handoff_in', side_effect=flaky):
            self.assertEqual(self.loop(world, polls=40), 'returned')       # one process: retried until durable
        # both Pods were absent from the second pass (1115 s), so only the unpersisted dup2 delayed completion
        self.assertGreater(durable_at[0], 1115 + 30)
        self.assertGreaterEqual(self.completion()['epoch'], durable_at[0])
        self.assertEqual(eg.known_pods(self.root)['dup2'], 'census')
        self.assertIn('dup2', world.deletes)

    def test_unpersisted_id_blocks_completion_while_the_store_stays_unwritable(self):
        self.record('pod1'); self.started(); write(self.root / 'release-authorized.json', {})
        world = World([pod('pod1'), pod('dup2')])
        real = eg.handoff_in

        def never_dup2(root, pod_id, role, epoch, **options):
            if pod_id == 'dup2':
                raise OSError('disk full')
            return real(root, pod_id, role, epoch, **options)
        with mock.patch.object(eg, 'handoff_in', side_effect=never_dup2):
            self.assertEqual(self.loop(world, polls=10), 'running')
        self.assertIsNone(self.completion())
        self.assertIn('dup2', world.deletes)


class ReleaseUnhandedTests(unittest.TestCase):
    """I4: a creator's own release loop deletes and verifies absence from fresh evidence, bounded by time."""

    def run_release(self, world, deadline_after=120, ids=('A',), sleep=None):
        clock = FakeClock(1000.)
        folder = Path(self.temporary.name) / 'known-pods'
        pending = None; returned_at = FakeClock(0.)
        try:
            with no_network(), failing_handoff(set(ids)),                     eg.LeaseClient(folder, NAME, 'k', world.provider, world.list, clock, clock.sleep) as client:
                pending = eg.release_unhanded(client, list(ids), 1000. + deadline_after, sleep or clock.sleep)
                returned_at.t = clock.t           # the release's own end, before the client's exit
        except eg.AlertError:
            self.assertTrue(pending)            # the exit could not release them either and raised loudly
        return pending, returned_at

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(); self.addCleanup(self.temporary.cleanup)

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


class OnceEndingTests(Base):
    """Round 5 finding 2 (I4): a --once pass never ends holding an ID that is neither durable nor verified absent."""

    def failing_dup2(self):
        real = eg.handoff_in

        def handoff(folder, pod_id, role, epoch, **options):
            if pod_id == 'dup2':
                raise OSError('read-only store')
            return real(folder, pod_id, role, epoch, **options)
        return mock.patch.object(eg, 'handoff_in', side_effect=handoff)

    def test_once_releases_an_unpersisted_duplicate_and_verifies_it(self):
        self.record('pod1'); self.started()
        world = World([pod('pod1'), pod('dup2')])
        with self.failing_dup2():
            self.once(world)
        self.assertNotIn('dup2', world.pods)
        self.assertGreaterEqual(self.clock.t - 1100., 30)          # absence verified, not assumed
        # the failed retention is a store failure, so the pass also released the lease (I2, fail closed)
        self.assertEqual(self.status()['reason'], 'guard_failure')

    def test_once_fails_loudly_when_it_cannot_release(self):
        self.record('pod1'); self.started()
        world = World([pod('pod1'), pod('dup2')]); world.delete_failures = -1
        with self.failing_dup2(), self.assertRaisesRegex(RuntimeError, 'ALERT'):
            self.once(world)
        self.assertEqual(read(self.root / 'ALERT-pod-not-released.json')['pod_ids'], ['dup2'])


class ReleaseReappearanceTests(unittest.TestCase):
    """Round 5 finding 3: confirmation is never terminal in release_unhanded; finding 4: its census retains."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(); self.addCleanup(self.temporary.cleanup)
        self.folder = Path(self.temporary.name) / 'known-pods'

    def client(self, world, clock):
        return eg.LeaseClient(self.folder, NAME, 'k', world.provider, world.list, clock, clock.sleep)

    def test_a_reappearing_confirmed_id_is_released_again_before_success(self):
        clock = FakeClock(1000.)
        world = World([pod('A'), pod('B')])
        sleeps = [0]; reappeared = []
        real_delete = world.provider

        def provider(key, pod_id):
            api = real_delete(key, pod_id); call = api.call

            def wrapped(method):
                if method == 'DELETE' and pod_id == 'B' and clock.t < 1100:
                    raise RuntimeError('provider HTTP 500')    # B stays until 1100
                return call(method)
            api.call = wrapped
            return api

        def sleep(seconds):
            sleeps[0] += 1; clock.sleep(seconds)
            if clock.t == 1050:                                 # A was confirmed absent; it comes back
                world.pods['A'] = pod('A'); reappeared.append(len(world.deletes))
        with no_network(), failing_handoff({'A', 'B'}), \
                eg.LeaseClient(self.folder, NAME, 'k', provider, world.list, clock, sleep) as client:
            pending = eg.release_unhanded(client, ['A', 'B'], 1300., sleep)
        self.assertEqual(pending, [])
        self.assertIn('A', world.deletes[reappeared[0]:])          # A was deleted again after reappearing
        self.assertNotIn('A', world.pods)
        self.assertGreaterEqual(clock.t, 1100 + 30)

    def test_a_reappearing_id_that_cannot_be_released_is_reported(self):
        clock = FakeClock(1000.)
        world = World([pod('A')])

        def sleep(seconds):
            clock.sleep(seconds)
            world.pods['A'] = pod('A')                          # A keeps coming back
        with no_network(), failing_handoff({'A'}), self.client(world, clock) as client:
            pending = eg.release_unhanded(client, ['A'], 1120., sleep)
        self.assertEqual(pending, ['A'])   # (the client's exit then releases A once it stops reappearing)

    def test_recovery_census_retains_a_new_owned_id(self):
        clock = FakeClock(1000.)
        world = World([pod('A'), pod('B')])
        with no_network(), failing_handoff({'A'}), self.client(world, clock) as client:
            eg.release_unhanded(client, ['A'], 1300., clock.sleep)
        self.assertEqual(read(self.folder / 'B.json'), {'id': 'B', 'role': 'census',
                                                         'epoch': read(self.folder / 'B.json')['epoch']})

    def test_recovery_census_releases_a_new_owned_id_it_cannot_hand_off(self):
        clock = FakeClock(1000.)
        world = World([pod('A'), pod('B')])
        real = eg.handoff_in

        def handoff(folder, pod_id, role, epoch, **options):
            if pod_id == 'B':
                raise OSError('read-only store')
            return real(folder, pod_id, role, epoch, **options)
        with no_network(), mock.patch.object(eg, 'handoff_in', side_effect=handoff), \
                self.client(world, clock) as client:
            pending = eg.release_unhanded(client, ['A'], 1300., clock.sleep)
        self.assertEqual(pending, [])
        self.assertNotIn('B', world.pods)
        self.assertFalse((self.folder / 'B.json').exists())


class ProviderBoundaryTests(unittest.TestCase):
    """Rounds 5 and 6: the provider boundary, checked statically and at runtime.

    Static, in every lease module: no attribute reference (call or not) to a raw provider method and no
    raw census or poster call outside LeaseClient and the raw clients; urllib, http.client and requests
    only in the modules that implement a raw client, and urlopen only inside those clients; every
    LeaseClient (or lease_client) is entered with `with`, so its exit resolves unhanded IDs.
    Runtime: the raw clients refuse to run outside a LeaseClient call."""

    FORBIDDEN_ATTRS = {'call', 'create', '_call', 'list_pods', '_pods', '_provider', 'urlopen', 'funds'}
    FORBIDDEN_CALL_NAMES = {'pods', 'poster'}
    FORBIDDEN_NAMES = {'list_pods', 'urlopen'}
    ALLOWED_SCOPES = {'LeaseClient', 'Provider', 'PodApi', 'DiagnosedProvider', 'list_pods'}
    # (attribute, receiver name) pairs that are not raw provider methods
    ALLOWED_RECEIVERS = {('create', 'cloud_lease')}             # the lease-level create, which uses the boundary
    # live_funds reads the account balance (no Pod ID) through the resident guard's read-only query
    ALLOWED_IN_SCOPE = {('funds', 'live_funds')}
    NETWORK_MODULES = {'urllib', 'http', 'requests'}
    NETWORK_FILES = {'external_guard.py', 'prepare_lease.py', 'diagnostic_create.py'}
    CLIENT_FACTORIES = {'LeaseClient', 'lease_client'}

    @classmethod
    def violations(cls, source, filename='module.py'):
        import ast
        tree = ast.parse(source)
        found = []
        with_items = {id(item.context_expr) for node in ast.walk(tree) if isinstance(node, (ast.With, ast.AsyncWith))
                      for item in node.items}

        def visit(node, scopes):
            if isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
                scopes = scopes + [node.name]
            inside = set(scopes) & cls.ALLOWED_SCOPES
            if isinstance(node, (ast.Import, ast.ImportFrom)) and filename not in cls.NETWORK_FILES:
                names = [a.name for a in node.names] if isinstance(node, ast.Import) else [node.module or '']
                if any(n.split('.')[0] in cls.NETWORK_MODULES for n in names):
                    found.append((node.lineno, 'import'))
            if isinstance(node, ast.Attribute) and node.attr in cls.FORBIDDEN_ATTRS and not inside:
                receiver = node.value.id if isinstance(node.value, ast.Name) else None
                if (node.attr, receiver) not in cls.ALLOWED_RECEIVERS and \
                        not any((node.attr, scope) in cls.ALLOWED_IN_SCOPE for scope in scopes):
                    found.append((node.lineno, node.attr))
            if isinstance(node, ast.Name) and node.id in cls.FORBIDDEN_NAMES and not inside:
                found.append((node.lineno, node.id))
            if isinstance(node, ast.Call) and not inside:
                if isinstance(node.func, ast.Name) and node.func.id in cls.FORBIDDEN_CALL_NAMES:
                    found.append((node.lineno, node.func.id))
            if isinstance(node, ast.Call):
                func = node.func
                name = func.id if isinstance(func, ast.Name) else func.attr if isinstance(func, ast.Attribute) else None
                if name in cls.CLIENT_FACTORIES and id(node) not in with_items and 'lease_client' not in scopes:
                    found.append((node.lineno, 'unmanaged ' + name))
            for child in ast.iter_child_nodes(node):
                visit(child, scopes)
        visit(tree, [])
        return found

    def modules(self):
        here = Path(eg.__file__).resolve().parent
        return [here / 'external_guard.py', here / 'prepare_lease.py',
                here.parents[2] / 'tools/search_sharing_allocation/cloud_lease.py',
                here.parents[2] / 'tools/search_sharing_allocation/diagnostic_create.py']

    def test_lease_modules_reach_the_provider_only_through_the_boundary(self):
        for module in self.modules():
            with self.subTest(module=module.name):
                self.assertEqual(self.violations(module.read_text(encoding='utf-8'), module.name), [])

    def test_every_entry_point_enters_the_client_with_with(self):
        import ast
        users = []
        for module in self.modules():
            tree = ast.parse(module.read_text(encoding='utf-8'))
            for node in ast.walk(tree):
                if isinstance(node, (ast.With, ast.AsyncWith)):
                    for item in node.items:
                        func = getattr(item.context_expr, 'func', None)
                        name = getattr(func, 'id', None) or getattr(func, 'attr', None)
                        if name in self.CLIENT_FACTORIES:
                            users.append(module.name)
        # the guard (run), prepare_lease create, and cloud_lease create, resolve-ambiguous, verify-absent, status
        self.assertEqual(sorted(users), ['cloud_lease.py'] * 4 + ['external_guard.py', 'prepare_lease.py'])

    def test_the_check_detects_each_escape_form(self):
        cases = {
            'call spelling': "def f(provider, key):\n    provider(key, 'A').call('GET')\n",
            'bound method alias': "def f(provider, key):\n    send = provider(key, 'A').call\n    send('DELETE')\n",
            'census call': "def f(pods, key):\n    pods(key)\n",
            'census reference': "import external_guard\ndef f():\n    g = external_guard.list_pods\n",
            'direct urlopen': "def f(r):\n    urllib.request.urlopen(r)\n",
            'urlopen name': "from urllib.request import urlopen\n",
            'network import': "import requests\n",
            'http.client import': "import http.client\n",
            'unmanaged client': "def f(a):\n    c = LeaseClient(a, 'n', 'k')\n",
            'unmanaged lease_client': "def f(a):\n    c = lease_client(a, {}, 'k')\n",
        }
        for label, source in cases.items():
            with self.subTest(label=label):
                self.assertTrue(self.violations(source, 'cloud_lease.py'), label)
        self.assertEqual(self.violations("def f(a):\n    with LeaseClient(a, 'n', 'k') as c:\n        c.census()\n",
                                         'cloud_lease.py'), [])

    def test_raw_clients_refuse_to_run_outside_the_boundary(self):
        import prepare_lease
        with no_network():
            with self.assertRaisesRegex(RuntimeError, 'must go through LeaseClient'):
                eg.PodApi('k', 'A').call('GET')
            with self.assertRaisesRegex(RuntimeError, 'must go through LeaseClient'):
                eg.list_pods('k')
            with self.assertRaisesRegex(RuntimeError, 'must go through LeaseClient'):
                prepare_lease.Provider('k').create({'name': NAME})

    def test_the_client_refuses_use_outside_its_with_block(self):
        world = World([pod('A')])
        client = eg.LeaseClient(Path(tempfile.mkdtemp()) / 'known-pods', NAME, 'k', world.provider, world.list)
        with self.assertRaisesRegex(RuntimeError, 'outside its with block'):
            client.census()
        self.assertEqual(world.calls, 0)


class InterruptTests(unittest.TestCase):
    """Round 7: an interrupt (KeyboardInterrupt) during a handoff never abandons an owned ID. Every ID is
    an obligation before any handoff I/O; the client's exit deletes and verifies each one, or writes an
    ALERT naming all of them, and then re-raises the interrupt. Later censuses omit the IDs."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(); self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.folder = self.root / 'lease/known-pods'

    def interrupting(self, ids):
        real = eg.handoff_in; fired = []

        def handoff(folder, pod_id, role, epoch, **options):
            if pod_id in ids and not fired:
                fired.append(pod_id)
                raise KeyboardInterrupt()
            return real(folder, pod_id, role, epoch, **options)
        return mock.patch.object(eg, 'handoff_in', side_effect=handoff)

    @staticmethod
    def first_census_only(world):
        calls = [0]

        def census(current):
            calls[0] += 1
            return current if calls[0] == 1 else []
        world.census = census

    def client(self, world, clock):
        return eg.LeaseClient(self.folder, NAME, 'k', world.provider, world.list, clock, clock.sleep,
                              self.root / 'ALERT-pod-not-released.json')

    def test_single_id_interrupted_handoff_is_released_before_the_interrupt_propagates(self):
        world = World([pod('B')]); self.first_census_only(world); clock = FakeClock(1000.)
        with self.assertRaises(KeyboardInterrupt), no_network(), self.interrupting({'B'}),                 self.client(world, clock) as client:
            client.census()
        self.assertIn('B', world.deletes)
        self.assertEqual(world.pods, {})
        self.assertGreaterEqual(clock.t - 1000., 30)                 # absence verified
        self.assertFalse((self.root / 'ALERT-pod-not-released.json').exists())

    def test_multi_id_interrupt_releases_the_later_ids_too(self):
        world = World([pod('B'), pod('C'), pod('D')]); self.first_census_only(world); clock = FakeClock(1000.)
        with self.assertRaises(KeyboardInterrupt), no_network(), self.interrupting({'B', 'C', 'D'}),                 self.client(world, clock) as client:
            client.census()
        self.assertEqual(sorted(set(world.deletes)), ['B', 'C', 'D'])
        self.assertEqual(world.pods, {})

    def test_interrupt_that_cannot_release_writes_an_alert_naming_every_id(self):
        world = World([pod('B'), pod('C'), pod('D')]); world.delete_failures = -1
        self.first_census_only(world); clock = FakeClock(1000.)
        with self.assertRaises(KeyboardInterrupt), no_network(), \
                failing_handoff({'B', 'C', 'D'}, first=KeyboardInterrupt), self.client(world, clock) as client:
            client.census()
        self.assertEqual(read(self.root / 'ALERT-pod-not-released.json')['pod_ids'], ['B', 'C', 'D'])

    def test_interrupted_post_handoff_is_released(self):
        world = World(); self.first_census_only(world); clock = FakeClock(1000.)

        def poster(body):
            world.pods['A'] = pod('A'); return pod('A')
        with self.assertRaises(KeyboardInterrupt), no_network(), failing_handoff({'A'}, first=KeyboardInterrupt),                 self.client(world, clock) as client:
            client.post({'name': NAME}, poster=poster)
        self.assertIn('A', world.deletes)
        self.assertEqual(world.pods, {})

    def test_interrupted_guard_pass_releases_the_new_duplicate_and_keeps_the_receipted_pod(self):
        write(self.root / 'lease/lease.json', lease())
        write(self.root / 'lease/created-pod.json', {'id': 'pod1', 'name': NAME})
        write(self.root / 'worker-started.json', {})
        world = World([pod('pod1'), pod('dup2')]); clock = FakeClock(1100.)
        world.census = lambda current: current if not world.deletes else [p for p in current if p['id'] != 'dup2']
        with self.assertRaises(KeyboardInterrupt), no_network(), failing_handoff({'dup2'}, first=KeyboardInterrupt):
            eg.run(self.root, 'k', pods=world.list, provider=world.provider, clock=clock, sleep=clock.sleep,
                   once=True)
        self.assertNotIn('dup2', world.pods)
        self.assertIn('pod1', world.pods)


class ReconcileTests(unittest.TestCase):
    """Round 8: cleanup reconciles durable ownership before any DELETE, and an interrupted cleanup's
    ALERT names the obligations open at the moment it is written."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(); self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.folder = self.root / 'lease/known-pods'

    def client(self, world, clock):
        return eg.LeaseClient(self.folder, NAME, 'k', world.provider, world.list, clock, clock.sleep,
                              self.root / 'ALERT-pod-not-released.json')

    def test_interrupt_after_a_durable_handoff_before_the_obligation_clears_deletes_nothing(self):
        # finding 1(b): the record is written and reads back, then the interrupt lands
        world = World([pod('B')]); clock = FakeClock(1000.)
        real = eg.handoff_in

        def write_then_interrupt(folder, pod_id, role, epoch, **options):
            real(folder, pod_id, role, epoch, **options)
            raise KeyboardInterrupt()
        with self.assertRaises(KeyboardInterrupt), no_network(), \
                mock.patch.object(eg, 'handoff_in', side_effect=write_then_interrupt), \
                self.client(world, clock) as client:
            client.census()
        self.assertEqual(world.deletes, [])
        self.assertEqual(read(self.folder / 'B.json')['id'], 'B')

    def test_cleanup_census_that_completes_the_handoff_deletes_nothing(self):
        # finding 1: the readback is interrupted; the cleanup's own census then hands B off durably
        world = World([pod('B')]); clock = FakeClock(1000.)
        real = eg.handoff_in; fired = []

        def interrupt_once(folder, pod_id, role, epoch, **options):
            if not fired:
                fired.append(pod_id)
                raise KeyboardInterrupt()
            return real(folder, pod_id, role, epoch, **options)
        with self.assertRaises(KeyboardInterrupt), no_network(), \
                mock.patch.object(eg, 'handoff_in', side_effect=interrupt_once), \
                self.client(world, clock) as client:
            client.census()
        self.assertEqual(world.deletes, [])
        self.assertEqual(read(self.folder / 'B.json')['id'], 'B')

    def test_existing_record_is_never_an_obligation(self):
        # finding 1(a), at the client: a record that reads back is loaded as handed off on entry
        eg.remember(self.root, 'P', 'receipt', 1000.)
        world = World([pod('P')]); clock = FakeClock(1000.)
        with self.assertRaises(KeyboardInterrupt), no_network(), \
                failing_handoff({'P'}, first=KeyboardInterrupt), self.client(world, clock) as client:
            client.census()
        self.assertEqual(world.deletes, [])

    def test_interrupt_during_cleanup_alerts_every_open_obligation(self):
        # finding 2: A is pending; the cleanup's census discovers B and C; a further interrupt lands
        # during B's handoff. The ALERT names A, B and C, and the original interrupt propagates.
        world = World([pod('A'), pod('B'), pod('C')]); clock = FakeClock(1000.)
        calls = [0]

        def census(current):
            calls[0] += 1
            return [p for p in current if p['id'] == 'A'] if calls[0] == 1 else current
        world.census = census
        real = eg.handoff_in; fired = []

        def handoff(folder, pod_id, role, epoch, **options):
            if pod_id in ('A', 'B') and pod_id not in fired:
                fired.append(pod_id)
                raise KeyboardInterrupt('first' if pod_id == 'A' else 'second')
            if pod_id == 'A':
                raise OSError('read-only store')
            return real(folder, pod_id, role, epoch, **options)
        with self.assertRaises(KeyboardInterrupt) as raised, no_network(), \
                mock.patch.object(eg, 'handoff_in', side_effect=handoff), self.client(world, clock) as client:
            client.census()
        self.assertEqual(str(raised.exception), 'first')
        self.assertEqual(read(self.root / 'ALERT-pod-not-released.json')['pod_ids'], ['A', 'B', 'C'])
        self.assertEqual(world.deletes, [])


class RecheckTests(unittest.TestCase):
    """Round 9: durability is rechecked immediately before each DELETE; a failed release that runs out
    its deadline after a single interrupt writes the ALERT."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(); self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.folder = self.root / 'lease/known-pods'

    def test_handoff_published_during_another_targets_provider_call_is_never_deleted(self):
        world = World([pod('A'), pod('B')]); clock = FakeClock(1000.)
        published = []
        real_provider = world.provider

        def provider(key, pod_id):
            api = real_provider(key, pod_id); call = api.call

            def wrapped(method):
                if pod_id == 'A' and not published:   # B's creator publishes its handoff mid-pass
                    published.append(method)
                    eg.remember_in(self.folder, 'B', 'receipt', 1000.)
                    write(self.root / 'lease/created-pod.json', {'id': 'B', 'name': NAME})
                return call(method)
            api.call = wrapped
            return api
        # B was seen alive and its presence handoff never lands: the guardian's Pod is never deleted
        # here, and the unlanded handoff is reported (round 4 finding 1)
        with self.assertRaises(eg.AlertError), no_network(), failing_handoff({'A', 'B'}), \
                eg.LeaseClient(self.folder, NAME, 'k', provider, world.list, clock, clock.sleep,
                               self.root / 'ALERT-pod-not-released.json') as client:
            pending = eg.release_unhanded(client, ['A', 'B'], 1300., clock.sleep)
        self.assertEqual(published, ['DELETE'])
        self.assertEqual(pending, ['B'])
        self.assertEqual(read(self.root / 'ALERT-pod-not-released.json')['pod_ids'], ['B'])
        self.assertNotIn('B', world.deletes)
        self.assertIn('B', world.pods)
        self.assertIn('A', world.deletes)

    def test_failed_release_after_one_interrupt_runs_out_the_deadline_and_alerts(self):
        world = World([pod('B'), pod('C'), pod('D')]); world.delete_failures = -1
        calls = [0]

        def census(current):
            calls[0] += 1
            return current if calls[0] == 1 else []
        world.census = census
        clock = FakeClock(1000.)
        with self.assertRaises(KeyboardInterrupt), no_network(), \
                failing_handoff({'B', 'C', 'D'}, first=KeyboardInterrupt, first_only_once=True), \
                eg.LeaseClient(self.folder, NAME, 'k', world.provider, world.list, clock, clock.sleep,
                               self.root / 'ALERT-pod-not-released.json') as client:
            client.census()
        self.assertGreaterEqual(clock.t, 1000. + 120)                # the deadline ran out
        self.assertEqual(read(self.root / 'ALERT-pod-not-released.json')['pod_ids'], ['B', 'C', 'D'])


if __name__ == '__main__':
    unittest.main()
