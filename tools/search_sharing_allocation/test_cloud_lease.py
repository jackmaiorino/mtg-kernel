"""Offline coverage of the guarded lease adapter. No provider, network or subprocess call.

Provider and account calls go to bounded fakes (lease_test_support); every guard loop runs on
a fake clock with a hard sleep cap.
"""
import io
import json
import subprocess
import sys
import tempfile
import threading
import time
import unittest
import urllib.error
import urllib.request
from pathlib import Path
from unittest.mock import MagicMock, patch

import cloud_lease as lease
import diagnostic_create
from lease_test_support import FUNDS_OK, FakeClock, Stop, World, no_network

NAME = "search-sharing-allocation-20261013-test"


def pod(pid="A", **changes):
    return {"id": pid, "name": NAME, "costPerHr": 0.96, "cpuFlavorId": "cpu3c", "vcpuCount": 32,
            "desiredStatus": "RUNNING", **changes}


def funds_ok(key):
    return dict(FUNDS_OK)


class Base(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(); self.addCleanup(self.temporary.cleanup)
        self.folder = Path(self.temporary.name)
        for patcher in (patch.object(lease.host_slots_v1, "process_state", return_value="alive"),
                        # keep external_guard.write intact for every test
                        patch.object(lease.external_guard, "write", lease.external_guard.write)):
            patcher.start(); self.addCleanup(patcher.stop)
        network = no_network(); network.__enter__(); self.addCleanup(network.__exit__, None, None, None)

    def control(self):
        root = self.folder / "control"
        key = self.folder / "existing.pub"
        key.write_text("ssh-ed25519 ZmFrZQ== existing")
        lease.prepare(root, NAME, key, "US-MD-1")
        lease.write(root / "guardian-ready.json", {"name": NAME, "pid": 1, "creation_time": 1, "epoch": time.time()})
        return root

    def create(self, root, world, poster, **options):
        options.setdefault("funds", funds_ok)
        return lease.create(root, True, key="memory-only-key", pods=world.list, poster=poster,
                            provider=world.provider, **options)


class PreparationTests(Base):
    def test_provider_diagnostic_keeps_safe_status_and_redacts_credentials(self):
        diagnostic_create.DiagnosedProvider.control = self.folder
        api = diagnostic_create.DiagnosedProvider("memory-only-secret")
        error = urllib.error.HTTPError("https://example.invalid", 500, "failure", {},
                io.BytesIO(json.dumps({"error": "no capacity", "detail": "memory-only-secret",
                                       "env": {"RUNPOD_API_KEY": "must-not-save"}}).encode()))
        with patch.object(diagnostic_create.urllib.request, "urlopen", side_effect=error):
            with self.assertRaisesRegex(RuntimeError, "provider HTTP 500"):
                api._call(urllib.request.Request("https://example.invalid"))
        receipt = next(self.folder.glob("create-http-error-*.json")).read_text()
        self.assertNotIn("memory-only-secret", receipt)
        self.assertNotIn("must-not-save", receipt)
        self.assertEqual(json.loads(receipt)["http_status"], 500)
        self.assertEqual(json.loads(receipt)["safe_error_fields"]["error"], "no capacity")

    def test_replacement_preserves_absolute_deadline_and_cumulative_cap(self):
        key = self.folder / "existing.pub"
        key.write_text("ssh-ed25519 ZmFrZQ== existing")
        now, deadline = 10000, 20000
        root = self.folder / "replacement"
        result = lease.prepare(root, "search-sharing-allocation-20261013-replacement", key,
                               "US-MD-1", now, deadline_epoch=deadline, prior_conservative_usd=1)
        saved = lease.read(root / "lease/lease.json")
        self.assertEqual(saved["deadline_epoch"], deadline)
        self.assertEqual(saved["total_cap_usd"], 5)
        self.assertEqual(saved["increment_cap_usd"], 4)
        self.assertEqual(saved["prior_conservative_usd"], 1)
        self.assertLessEqual(result["projected_cumulative_conservative_usd"], 5)
        self.assertEqual(lease.lease_guard.decision(saved, deadline - 600, 1)["reason"], "deadline")
        for options in ({"prior_conservative_usd": 1},
                        {"deadline_epoch": now + 14401, "prior_conservative_usd": 1},
                        {"deadline_epoch": deadline, "prior_conservative_usd": 4},
                        {"deadline_epoch": deadline, "prior_conservative_usd": float("nan")}):
            rejected = self.folder / "rejected"
            with self.assertRaises(ValueError):
                lease.prepare(rejected, "search-sharing-allocation-20261013-rejected", key,
                              "US-MD-1", now, **options)
            self.assertFalse(rejected.exists())


class CreateTests(Base):
    """#335 item 1 (lock, idempotency, duplicates), item 2 (price at create), item 3 (fresh funding)."""

    def test_concurrent_creates_post_once(self):
        root = self.control(); world = World(); posts = []; errors = []

        def poster(body):
            posts.append(1); time.sleep(0.2)
            world.pods["A"] = pod("A"); return pod("A")

        def go():
            try:
                self.create(root, world, poster)
            except ValueError as error:
                errors.append(str(error))
        threads = [threading.Thread(target=go) for _ in range(2)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join(timeout=30)
        self.assertEqual(len(posts), 1)
        self.assertEqual(len(errors), 1)
        self.assertIn("already attempted", errors[0])
        with self.assertRaisesRegex(ValueError, "already attempted"):
            self.create(root, world, poster)
        self.assertEqual(len(posts), 1)

    def test_existing_pod_with_the_name_refuses_before_post_and_persists_no_key(self):
        root = self.control(); posts = []
        world = World([pod("owned123", env={"RUNPOD_API_KEY": "must-never-be-persisted"})])
        with self.assertRaisesRegex(ValueError, "already exists"):
            self.create(root, world, lambda body: posts.append(1))
        self.assertEqual(posts, [])
        for path in root.rglob("*"):
            if path.is_file():
                self.assertNotIn(b"memory-only-key", path.read_bytes())
                self.assertNotIn(b"must-never-be-persisted", path.read_bytes())

    def test_duplicate_after_post_is_deleted(self):
        root = self.control(); world = World()

        def poster(body):
            world.pods["A"] = pod("A"); world.pods["B"] = pod("B")   # two same-name Pods
            return pod("A")
        result = self.create(root, world, poster)
        self.assertEqual(result["id"], "A")
        self.assertEqual(world.deletes, ["B"])
        self.assertEqual(set(world.pods), {"A"})
        self.assertNotIn("memory-only-key", (root / "lease/created-pod.json").read_text())

    def test_ambiguous_post_adopts_one_and_deletes_the_rest(self):
        root = self.control(); world = World()

        def poster(body):
            world.pods["B"] = pod("B"); world.pods["C"] = pod("C")
            raise RuntimeError("provider HTTP 500")
        self.assertEqual(self.create(root, world, poster)["id"], "B")
        self.assertEqual(world.deletes, ["C"])
        self.assertTrue(lease.read(root / "lease/created-pod.json")["reconciled_exact_name"])

    def test_ambiguous_post_with_no_pod_stays_guarded(self):
        root = self.control(); world = World()

        def poster(body):
            raise RuntimeError("provider HTTP 503")
        with self.assertRaisesRegex(RuntimeError, "ambiguous"):
            self.create(root, world, poster)
        self.assertTrue(lease.external_guard.ambiguous_unresolved(root))
        self.assertFalse((root / "lease/create-failure.json").exists())

    def test_explicit_4xx_with_empty_census_is_definite(self):
        root = self.control(); world = World()

        def poster(body):
            raise RuntimeError("provider HTTP 400")
        with self.assertRaisesRegex(RuntimeError, "refused by the provider"):
            self.create(root, world, poster)
        self.assertTrue((root / "lease/create-failure.json").exists())
        self.assertFalse(lease.external_guard.ambiguous_unresolved(root))

    def test_funding_outage_refuses_before_post(self):
        root = self.control(); posts = []

        def down(key):
            raise RuntimeError("account HTTP 503")
        with self.assertRaisesRegex(ValueError, "funding service unavailable"):
            self.create(root, World(), lambda body: posts.append(1), funds=down)
        self.assertEqual(posts, [])
        self.assertFalse((root / "lease/create-attempt.json").exists())

    def test_under_or_low_balance_refuses(self):
        for funds in ({"balance_usd": 164.0, "under_balance": True}, {"balance_usd": 1.0, "under_balance": False},
                      {"balance_usd": float("nan"), "under_balance": False}):
            with self.subTest(funds=funds):
                root = self.control(); posts = []
                with self.assertRaises(ValueError):
                    self.create(root, World(), lambda body: posts.append(1), funds=lambda key: funds)
                self.assertEqual(posts, [])
                import shutil
                shutil.rmtree(root)

    def test_stale_funding_refuses(self):
        root = self.control(); clock = FakeClock(time.time()); world = World(); posts = []

        def slow_census(key):
            clock.advance(121); return world.list(key)
        with self.assertRaisesRegex(ValueError, "older than 120 s"):
            lease.create(root, True, key="k", pods=slow_census, poster=lambda body: posts.append(1),
                         funds=funds_ok, provider=world.provider, clock=clock)
        self.assertEqual(posts, [])

    def test_malformed_or_above_ceiling_price_at_create_deletes(self):
        for returned in (pod(costPerHr="n/a"), pod(costPerHr=None), pod(adjustedCostPerHr=1.5), pod(vcpuCount=64)):
            with self.subTest(returned=returned):
                root = self.control(); world = World()

                def poster(body):
                    world.pods["A"] = dict(returned); return dict(returned)
                with self.assertRaisesRegex(ValueError, "price or allocation invalid"):
                    self.create(root, world, poster)
                self.assertEqual(world.deletes, ["A"])
                self.assertTrue((root / "release-authorized.json").exists())
                self.assertEqual(lease.read(root / "lease/created-pod.json")["id"], "A")
                import shutil
                shutil.rmtree(root)


class GuardianTests(Base):
    """#335 item 4 (DELETE never gated on preservation, bounded) and item 7 (Pod-side marker)."""

    def guardian(self, root, world, clock, polls=40, **options):
        clock.max_sleeps = polls

        def run_guard(root, key, **kw):
            return lease.external_guard.run(root, key, pods=world.list, provider=world.provider, clock=clock,
                                            sleep=clock.sleep, **kw)
        with patch.object(lease.host_slots_v1, "self_identity", return_value=(1, 1)):
            try:
                lease.guardian(root, key="k", run_guard=run_guard, **options)
                return "returned"
            except Stop:
                return "running"

    def test_release_deletes_after_one_bounded_preservation_that_times_out(self):
        root = self.control(); world = World([pod("A")])
        lease.write(root / "lease/created-pod.json", {"id": "A", "name": NAME})
        lease.write(root / "worker-started.json", {})
        lease.write(root / "release-authorized.json", {})
        lease.write(root / "connection.json", {"pod_id": "A", "host": "192.0.2.1", "port": 22,
                    "private_key_path": "unused-key", "remote_output": "/workspace/search-sharing-allocation/run"})
        prepared = lease.read(root / "lease/lease.json")
        clock = FakeClock(prepared["created_epoch"] + 60); copies = []

        def scp(command, timeout, capture_output):
            copies.append(timeout); clock.advance(timeout)
            raise subprocess.TimeoutExpired(command, timeout)
        with patch.object(lease.subprocess, "run", scp):
            self.assertEqual(self.guardian(root, world, clock), "returned")
        self.assertEqual(len(copies), 1)
        self.assertLessEqual(copies[0], lease.external_guard.PRESERVE_MAX_SECONDS)
        self.assertEqual(world.deletes[0], "A")
        self.assertEqual(lease.read(root / "preservation.json")["copied"], False)
        self.assertEqual(lease.read(root / "external-guard-completion.json")["absent_confirmations"], 2)

    def test_watchdog_pass_never_preserves(self):
        root = self.control(); seen = {}

        def run_guard(root, key, **kw):
            seen.update(kw)
        lease.guardian(root, once=True, key="k", run_guard=run_guard)
        self.assertIsNone(seen["preserve"])
        self.assertTrue(seen["once"])
        self.assertNotIn("released", seen)    # no persisted result can end a guard (I5)

    def test_marker_probe_reads_resident_state_and_failures_mean_not_ready(self):
        root = self.control(); calls = []

        def ssh(command, **kw):
            calls.append(command)
            return subprocess.CompletedProcess(command, 0, stdout="STARTED\n", stderr="")
        probe = lease.pod_marker_probe(root, run=ssh)
        published = {"publicIp": "192.0.2.1", "portMappings": {"22": 40022}}
        self.assertTrue(probe(published))
        self.assertIn("/run/phase1/" + NAME + "/progress.json", calls[0][-1])
        self.assertFalse(probe({"publicIp": None, "portMappings": {}}))

        def broken(command, **kw):
            raise OSError("ssh unavailable")
        self.assertFalse(lease.pod_marker_probe(root, run=broken)(published))
        self.assertFalse(lease.pod_marker_probe(self.folder / "missing", run=ssh)(published))

    def test_preserve_refuses_a_foreign_connection_and_has_no_time_without_budget(self):
        root = self.control()
        lease.write(root / "connection.json", {"pod_id": "foreign123"})
        with self.assertRaisesRegex(ValueError, "another Pod"):
            lease.preserve(root, "owned123", 60)
        self.assertFalse(lease.preserve(root, "foreign123", 0)["copied"])
        with self.assertRaises(ValueError):
            lease.safe_pod({"id": "foreign123", "name": "another-lane"}, "owned-lane")


class ReleaseVerificationTests(Base):
    """#335 item 5: two spaced absent observations and the guardian's completion gate task removal."""

    def check(self, root, world):
        clock = FakeClock(1000.0)
        return lease.verify_absent(root, key="k", pods=world.list, provider=world.provider, clock=clock,
                                   sleep=clock.advance)

    def recorded(self):
        root = self.control()
        lease.write(root / "lease/create-attempt.json", {})
        lease.write(root / "lease/created-pod.json", {"id": "A", "name": NAME})
        return root

    def test_present_fails_and_alerts(self):
        root = self.recorded()
        self.assertFalse(self.check(root, World([pod("A")])))
        self.assertTrue((root / "ALERT-pod-not-released.json").exists())

    def test_absent_without_guardian_completion_fails(self):
        self.assertFalse(self.check(self.recorded(), World([])))

    def test_absent_twice_with_completion_passes(self):
        root = self.recorded()
        lease.write(root / "external-guard-completion.json", {"absent_confirmations": 2})
        self.assertTrue(self.check(root, World([])))
        observations = lease.read(root / "verify-absent.json")["observations"]
        self.assertGreaterEqual(observations[1]["epoch"] - observations[0]["epoch"], lease.ABSENT_SPACING_SECONDS)

    def test_failed_census_is_never_absence(self):
        root = self.recorded()
        lease.write(root / "external-guard-completion.json", {"absent_confirmations": 2})
        world = World([]); world.list_fails = True
        self.assertFalse(self.check(root, world))

    def test_ambiguous_create_blocks_release_until_operator_resolution(self):
        root = self.control()
        lease.write(root / "lease/create-attempt.json", {})
        lease.write(root / "lease/create-ambiguous.json", {})
        lease.write(root / "external-guard-completion.json", {"absent_confirmations": 2})
        self.assertFalse(self.check(root, World([])))
        with self.assertRaisesRegex(ValueError, "still visible"):
            lease.resolve_ambiguous(root, "operator checked console 12:00", key="k", pods=World([pod("Z")]).list)
        with self.assertRaisesRegex(ValueError, "state who checked"):
            lease.resolve_ambiguous(root, "short", key="k", pods=World([]).list)
        self.assertTrue(lease.resolve_ambiguous(root, "operator checked console 12:00", key="k",
                                                pods=World([]).list))
        self.assertTrue(self.check(root, World([])))

    def test_retained_duplicate_present_by_id_blocks_release_despite_an_empty_census(self):
        root = self.recorded()
        lease.external_guard.remember(root, "B", "duplicate", 1000.0)
        lease.write(root / "external-guard-completion.json", {"absent_confirmations": 2})
        world = World([pod("B")]); world.census = lambda current: []    # census omits B, GET still finds it
        self.assertFalse(self.check(root, world))

    def test_census_listing_the_recorded_pod_blocks_release_despite_a_404(self):
        root = self.recorded()
        lease.write(root / "external-guard-completion.json", {"absent_confirmations": 2})
        world = World([pod("A")]); world.get_script = iter(["absent"] * 4)
        self.assertFalse(self.check(root, world))

    def test_census_listing_a_retained_id_under_another_name_blocks_release(self):
        root = self.recorded()
        lease.write(root / "external-guard-completion.json", {"absent_confirmations": 2})
        for entry in ({"id": "A", "name": "renamed"}, {"id": "A"}):
            with self.subTest(entry=entry):
                world = World([]); world.census = lambda current, entry=entry: [dict(entry)]
                self.assertFalse(self.check(root, world))

    def test_successful_null_get_is_never_absence(self):
        root = self.recorded()
        lease.write(root / "external-guard-completion.json", {"absent_confirmations": 2})
        response = MagicMock()
        response.__enter__.return_value.read.return_value = b"null"
        with patch("urllib.request.urlopen", return_value=response):
            clock = FakeClock(1000.0)
            released = lease.verify_absent(root, key="k", pods=World([]).list, provider=lease.external_guard.PodApi,
                                           clock=clock, sleep=clock.advance)
        self.assertFalse(released)

    def test_unreadable_retained_record_blocks_release(self):
        root = self.recorded()
        lease.write(root / "external-guard-completion.json", {"absent_confirmations": 2})
        lease.external_guard.remember(root, "B", "duplicate", 1000.0)
        (root / lease.external_guard.KNOWN_DIR / "B.json").write_text("{broken")
        self.assertFalse(self.check(root, World([])))

    def test_attempt_without_receipt_is_unresolved_until_operator_resolution(self):
        root = self.control()
        lease.write(root / "lease/create-attempt.json", {})
        lease.write(root / "external-guard-completion.json", {"absent_confirmations": 2})
        self.assertFalse(self.check(root, World([])))
        self.assertTrue(lease.resolve_ambiguous(root, "operator checked console 12:00", key="k",
                                                pods=World([]).list))
        self.assertTrue(self.check(root, World([])))


class ReviewR1CreateTests(Base):
    """Review r1 findings 1 and 5 on the create path."""

    def test_failed_duplicate_delete_is_retained_for_the_guardian(self):
        root = self.control(); world = World(); world.delete_failures = -1

        def poster(body):
            world.pods["A"] = pod("A"); world.pods["B"] = pod("B")
            return pod("A")
        self.assertEqual(self.create(root, world, poster)["id"], "A")
        self.assertEqual(world.deletes, ["B"])
        self.assertEqual(lease.external_guard.known_pods(root)["B"], "census")   # retained by the census, not a create result
        self.assertEqual(lease.external_guard.known_pods(root)["A"], "receipt")

    def test_post_creation_diagnostics_never_gate_the_delete(self):
        failing = {"post-census-error.json", "duplicates-deleted.json", "created-pod.json",
                   "created-pod-detail.json", "release-authorized.json"}
        original = lease.write

        def write(path, value, **kw):
            if Path(path).name in failing:
                raise OSError("disk full")
            return original(path, value, **kw)
        # census works before the POST, then fails; the returned Pod is overpriced
        root = self.control(); world = World(); calls = [0]

        def census(key):
            calls[0] += 1
            if calls[0] > 1:
                raise RuntimeError("census HTTP 503")
            return world.list(key)

        def poster(body):
            world.pods["A"] = pod("A", costPerHr=9.0); return pod("A", costPerHr=9.0)
        with patch.object(lease, "write", write):
            with self.assertRaisesRegex(ValueError, "price or allocation invalid"):
                lease.create(root, True, key="k", pods=census, poster=poster, funds=funds_ok,
                             provider=world.provider)
        self.assertEqual(world.deletes, ["A"])
        self.assertIn("A", lease.external_guard.known_pods(root))
        # duplicates are deleted even when every post-creation receipt fails
        import shutil
        shutil.rmtree(root)
        root = self.control(); world = World()

        def two(body):
            world.pods["A"] = pod("A"); world.pods["B"] = pod("B"); return pod("A")
        with patch.object(lease, "write", write):
            self.assertEqual(self.create(root, world, two)["id"], "A")
        self.assertEqual(world.deletes, ["B"])


class InvariantTests(Base):
    """Round 3 review: I1 retain first (no coercion), I2 fail closed, I3 primary recovery."""

    def test_malformed_returned_id_is_never_a_receipt(self):
        import shutil
        for bad in (None, True, 7):
            with self.subTest(bad=bad):
                root = self.control(); world = World()

                def poster(body, bad=bad):
                    world.pods["A"] = pod("A"); return dict(pod("A"), id=bad)
                self.assertEqual(self.create(root, world, poster)["id"], "A")   # reconciled from the census
                self.assertEqual(set(lease.external_guard.known_pods(root)), {"A"})
                self.assertEqual(lease.read(root / "lease/created-pod.json")["id"], "A")
                shutil.rmtree(root)
        root = self.control(); world = World()
        with self.assertRaisesRegex(RuntimeError, "ambiguous"):
            self.create(root, world, lambda body: dict(pod("A"), id=None))
        self.assertFalse((root / "lease/created-pod.json").exists())
        self.assertEqual(lease.external_guard.known_pods(root), {})

    def test_guardian_recovers_the_primary_when_receipt_and_release_writes_failed(self):
        failing = {"created-pod.json", "release-authorized.json"}
        original = lease.write

        def write(path, value, **kw):
            if Path(path).name in failing:
                raise OSError("disk full")
            return original(path, value, **kw)
        root = self.control(); world = World(); world.delete_failures = -1

        def poster(body):
            world.pods["A"] = pod("A", costPerHr=9.0); return pod("A", costPerHr=9.0)
        with patch.object(lease, "write", write):
            with self.assertRaisesRegex(ValueError, "price or allocation invalid"):
                self.create(root, world, poster)
        self.assertFalse((root / "lease/created-pod.json").exists())
        self.assertEqual(world.deletes, ["A"])
        clock = FakeClock(lease.read(root / "lease/lease.json")["created_epoch"] + 60)
        lease.external_guard.run(root, "k", pods=world.list, provider=world.provider, clock=clock,
                                 sleep=clock.sleep, once=True)
        status = lease.read(root / "external-guard.json")
        self.assertEqual((status["pod_id"], status["reason"]), ("A", "rate_above_ceiling"))
        self.assertEqual(world.deletes, ["A", "A"])

    def test_unreadable_receipt_blocks_release(self):
        root = self.control()
        lease.write(root / "lease/create-attempt.json", {})
        (root / "lease/created-pod.json").write_text("{broken")
        lease.write(root / "external-guard-completion.json", {"absent_confirmations": 2})
        clock = FakeClock(1000.0)
        self.assertFalse(lease.verify_absent(root, key="k", pods=World([]).list, provider=World([]).provider,
                                             clock=clock, sleep=clock.advance))

    def test_fallback_id_in_an_ambiguity_record_is_checked_by_verify_absent(self):
        root = self.control()
        lease.write(root / "lease/create-attempt.json", {})
        lease.write(root / "lease/create-ambiguous.json", {"pod_id": "A"})
        lease.write(root / lease.external_guard.OPERATOR_RESOLUTION, {"checked_by": "operator"})
        lease.write(root / "external-guard-completion.json", {"absent_confirmations": 2})
        world = World([pod("A")]); world.census = lambda current: []
        clock = FakeClock(1000.0)
        self.assertFalse(lease.verify_absent(root, key="k", pods=world.list, provider=world.provider,
                                             clock=clock, sleep=clock.advance))


class ResponsibilityTests(Base):
    """Round 4: I1 retains the raw POST ID and each valid census entry first; I4 keeps responsibility."""

    def create_with(self, root, world, poster, failing_ids=(), **options):
        real = lease.external_guard.handoff_in

        def handoff(folder, pod_id, role, epoch):
            if pod_id in failing_ids:
                raise OSError("read-only store")
            return real(folder, pod_id, role, epoch)
        prepared = lease.read(root / "lease/lease.json")
        clock = FakeClock(prepared["created_epoch"] + 10)

        def slow_poster(body):   # the POST returns near the deadline, which bounds any release loop
            result = poster(body); clock.t = prepared["deadline_epoch"] - 60
            return result
        with patch.object(lease.external_guard, "handoff_in", side_effect=handoff):
            return lease.create(root, True, key="k", pods=world.list, poster=slow_poster, funds=funds_ok,
                                provider=world.provider, clock=clock, sleep=clock.sleep, **options)

    def two(self, world):
        def poster(body):
            world.pods["A"] = pod("A"); world.pods["B"] = pod("B"); return pod("A")
        return poster

    def test_unhanded_duplicate_is_deleted_and_verified_by_the_creator(self):
        # finding 2: B's retention fails; the creator releases B itself before returning
        root = self.control(); world = World()
        self.assertEqual(self.create_with(root, world, self.two(world), failing_ids=("B",))["id"], "A")
        self.assertNotIn("B", world.pods)
        self.assertNotIn("B", lease.external_guard.known_pods(root))
        self.assertIn("A", world.pods)

    def test_unhanded_duplicate_that_cannot_be_released_fails_loudly(self):
        root = self.control(); world = World(); world.delete_failures = -1
        with self.assertRaisesRegex(RuntimeError, "ALERT"):
            self.create_with(root, world, self.two(world), failing_ids=("B",))
        self.assertEqual(lease.read(root / "ALERT-pod-not-released.json")["pod_ids"], ["B"])

    def test_unhanded_primary_is_deleted_and_verified_before_the_create_fails(self):
        # finding 3: no durable handoff for the only ID: the creator deletes it and verifies absence
        root = self.control(); world = World()

        def poster(body):
            world.pods["A"] = pod("A"); return pod("A")
        with self.assertRaisesRegex(RuntimeError, "deleted and verified absent"):
            self.create_with(root, world, poster, failing_ids=("A",))
        self.assertNotIn("A", world.pods)

    def test_mixed_validity_census_keeps_each_valid_entry(self):
        # finding 4: a malformed entry never discards valid duplicate B
        root = self.control(); world = World()
        world.census = lambda current: current + ([{"name": NAME, "id": None}] if current else [])
        self.assertEqual(self.create_with(root, world, self.two(world))["id"], "A")
        self.assertEqual(lease.external_guard.known_pods(root)["B"], "census")   # retained by the census, not a create result
        self.assertNotIn("B", world.pods)

    def test_post_id_is_retained_whatever_the_name(self):
        # finding 5: {"id": "A"} with no name is retained, then rejected and deleted
        root = self.control(); world = World()

        def poster(body):
            world.pods["A"] = pod("A"); return {"id": "A"}
        world.census = lambda current: []
        with self.assertRaisesRegex(ValueError, "allocation invalid"):
            self.create_with(root, world, poster)
        self.assertEqual(lease.external_guard.known_pods(root)["A"], "receipt")
        self.assertEqual(world.deletes, ["A"])


class RoundFiveTests(Base):
    """Round 5: persisted results are information only; every census path retains what it sees."""

    def test_persisted_verify_absent_never_ends_the_guardian(self):
        root = self.control(); world = World([pod("A")])
        lease.write(root / "lease/created-pod.json", {"id": "A", "name": NAME})
        lease.write(root / "release-authorized.json", {})
        lease.write(root / "worker-started.json", {})
        lease.write(root / "verify-absent.json", {"released": True})
        clock = FakeClock(lease.read(root / "lease/lease.json")["created_epoch"] + 60)

        def run_guard(root, key, **kw):
            return lease.external_guard.run(root, key, pods=world.list, provider=world.provider, clock=clock,
                                            sleep=clock.sleep, **kw)
        lease.guardian(root, once=True, key="k", run_guard=run_guard)
        self.assertEqual(world.deletes, ["A"])

    def test_pre_post_refusal_retains_the_existing_owned_pod(self):
        root = self.control(); world = World([pod("B")]); posts = []
        with self.assertRaisesRegex(ValueError, "already exists"):
            self.create(root, world, lambda body: posts.append(1))
        self.assertEqual(posts, [])
        self.assertEqual(lease.external_guard.known_pods(root), {"B": "census"})

    def test_pre_post_refusal_releases_an_existing_pod_it_cannot_hand_off(self):
        root = self.control(); world = World([pod("B")])
        prepared = lease.read(root / "lease/lease.json")
        clock = FakeClock(prepared["created_epoch"] + 10)
        real = lease.external_guard.handoff_in

        def handoff(folder, pod_id, role, epoch):
            if pod_id == "B":
                raise OSError("read-only store")
            return real(folder, pod_id, role, epoch)

        def funds(key):   # the funding read happens just before the census; move near the deadline there
            clock.t = prepared["deadline_epoch"] - 60
            return funds_ok(key)
        with patch.object(lease.external_guard, "handoff_in", side_effect=handoff), \
                self.assertRaisesRegex(ValueError, "already exists"):
            lease.create(root, True, key="k", pods=world.list, poster=lambda body: None, funds=funds,
                         provider=world.provider, clock=clock, sleep=clock.sleep)
        self.assertNotIn("B", world.pods)

    def test_verification_retains_a_new_owned_pod_and_checks_it_on_retry(self):
        root = self.control()
        lease.write(root / "lease/create-attempt.json", {})
        lease.write(root / "lease/created-pod.json", {"id": "A", "name": NAME})
        lease.write(root / "external-guard-completion.json", {"absent_confirmations": 2})
        world = World([pod("B")])                        # A is gone; duplicate B is billed
        clock = FakeClock(1000.0)

        def check():
            return lease.verify_absent(root, key="k", pods=world.list, provider=world.provider, clock=clock,
                                       sleep=clock.advance)
        self.assertFalse(check())                       # B discovered by the census
        self.assertEqual(lease.external_guard.known_pods(root)["B"], "census")
        world.census = lambda current: []                # the retry's census omits B
        self.assertFalse(check())                       # B is still GET-checked and present
        self.assertTrue(lease.read(root / "verify-absent.json")["observations"][0]["present_by_id"]["B"])
        world.pods.clear()
        self.assertTrue(check())


class StatusCommandTests(Base):
    """Round 6 finding 1: the status command ends through the boundary's exit, whatever happens."""

    def run_status(self, root, world, failing=("B",), late=True, **patches):
        real = lease.external_guard.handoff_in

        def handoff(folder, pod_id, role, epoch):
            if pod_id in failing:
                raise OSError("read-only store")
            return real(folder, pod_id, role, epoch)
        prepared = lease.read(root / "lease/lease.json")
        clock = FakeClock(prepared["deadline_epoch"] - 60 if late else 1000.0)
        out = io.StringIO()
        with patch.object(sys, "argv", ["cloud_lease.py", "status", "--control", str(root)]),                 patch.object(lease.external_guard, "api_key", return_value="k"),                 patch.object(lease.external_guard, "list_pods", world.list),                 patch.object(lease.external_guard, "PodApi", world.provider),                 patch.object(lease.external_guard, "handoff_in", side_effect=handoff),                 patch.object(lease.time, "time", clock), patch.object(lease.time, "sleep", clock.sleep),                 patch("sys.stdout", out):
            lease.main()
        return out.getvalue()

    def test_status_releases_an_owned_pod_it_could_not_hand_off(self):
        root = self.control(); world = World([pod("B")])
        printed = self.run_status(root, world)
        self.assertIn('"B"', printed)
        self.assertNotIn("B", world.pods)                    # deleted and verified absent before exiting

    def test_status_fails_loudly_when_it_cannot_release(self):
        root = self.control(); world = World([pod("B")]); world.delete_failures = -1
        with self.assertRaisesRegex(RuntimeError, "ALERT"):
            self.run_status(root, world)
        self.assertEqual(lease.read(root / "ALERT-pod-not-released.json")["pod_ids"], ["B"])

    def test_status_formatting_failure_still_resolves(self):
        root = self.control(); world = World([pod("B")])
        with patch.object(lease, "safe_pod", side_effect=ValueError("formatting failed")),                 self.assertRaisesRegex(ValueError, "formatting failed"):
            self.run_status(root, world)
        self.assertNotIn("B", world.pods)


class CreateInterruptTests(Base):
    """Round 7: an interrupt during the POST ID's handoff releases the Pod before propagating."""

    def test_interrupted_post_handoff_deletes_and_verifies_before_the_interrupt(self):
        root = self.control(); world = World()
        prepared = lease.read(root / "lease/lease.json")
        clock = FakeClock(prepared["created_epoch"] + 10)
        real = lease.external_guard.handoff_in; fired = []

        def handoff(folder, pod_id, role, epoch):
            if pod_id == "A" and not fired:
                fired.append(pod_id)
                raise KeyboardInterrupt()
            return real(folder, pod_id, role, epoch)

        def poster(body):
            world.pods["A"] = pod("A"); clock.t = prepared["deadline_epoch"] - 60
            return pod("A")
        world.census = lambda current: current if not world.pods.get("A") or not fired else []
        with patch.object(lease.external_guard, "handoff_in", side_effect=handoff),                 self.assertRaises(KeyboardInterrupt):
            lease.create(root, True, key="k", pods=world.list, poster=poster, funds=funds_ok,
                         provider=world.provider, clock=clock, sleep=clock.sleep)
        self.assertIn("A", world.deletes)
        self.assertNotIn("A", world.pods)


if __name__ == "__main__":
    unittest.main()
