"""Opt-in reservation pauses; no native MTG/model execution or reservation edits."""
import contextlib
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
from types import SimpleNamespace
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import archive_distillation_dispatch_v1 as dispatch


class PauseChecks(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name).resolve()
        self.guard = dispatch.Guard.__new__(dispatch.Guard)
        self.guard.pause_on_reservation = True
        self.guard.transition_lock = threading.RLock()
        self.guard.claim_identity = None
        self.guard.paused = False
        self.guard.pause_transitions = []
        self.guard.started = time.monotonic()
        self.guard.error = None
        self.guard.root = root / "attempt"
        self.guard.plan = dict(cpu_affinity=[16], stop_files=[str(root / "STOP")],
            storage=dict(accounting_roots=[str(root)], projected_additional_bytes=100,
                         max_physical_bytes=1000, reserve_bytes=60 * 1024**3))
        self.guard.limits = dict(cpu_seconds=10, wall_seconds=20)
        self.guard.tree = mock.Mock()
        self.guard.tree.set_suspended.return_value = True
        self.guard.sample = mock.Mock(return_value=dict(cpu_seconds=1, seconds=2, active_processes=1))
        self.claim = dict(id="owned", pid=123, creation_time=456, cores=[16], priority="below_normal",
                          background=False, state="running")
        patches = [mock.patch.dict(os.environ, {dispatch.slots.CLAIM_ENV: "owned", dispatch.slots.CORES_ENV: "16"}),
            mock.patch.object(dispatch.slots, "Mutex", contextlib.nullcontext),
            mock.patch.object(dispatch.slots, "read_json", side_effect=lambda _: copy.deepcopy(self.claim)),
            mock.patch.object(dispatch.slots, "process_state", return_value="alive"),
            mock.patch.object(dispatch.slots, "claim_conflicts", return_value=False),
            mock.patch.object(dispatch, "physical_tree", return_value=dict(charged_bytes=100)),
            mock.patch.object(dispatch.shutil, "disk_usage", return_value=SimpleNamespace(free=100 * 1024**3))]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)

    def test_default_still_fails_immediately_without_suspension(self):
        self.guard.pause_on_reservation = False
        with mock.patch.object(dispatch.slots, "claim_conflicts", return_value=True):
            with self.assertRaisesRegex(ValueError, "current reservation conflicts"):
                self.guard.check()
        self.guard.tree.set_suspended.assert_not_called()

    @unittest.skipUnless(os.name == "nt", "Windows suspended process creation API")
    def test_default_spawn_retains_suspended_adoption_and_initial_resume(self):
        self.guard.pause_on_reservation = False
        self.guard.check = mock.Mock()
        child = mock.Mock(pid=789, _handle=123)
        with mock.patch.object(dispatch.subprocess, "Popen", return_value=child) as spawn, \
             mock.patch.object(dispatch.slots, "creation_time", return_value=999), \
             mock.patch.object(dispatch.slots, "_NtResumeProcess", return_value=0) as resume:
            self.assertIs(self.guard.spawn(["python"], None, None), child)
        self.guard.check.assert_called_once_with()
        self.guard.tree.adopt.assert_called_once_with(789, 999)
        self.assertTrue(spawn.call_args.kwargs["creationflags"] & 4)
        resume.assert_called_once_with(123)
        self.guard.tree.set_suspended.assert_not_called()

    def test_conflict_pauses_then_verified_clear_resumes_without_clock_reset(self):
        started = self.guard.started
        with mock.patch.object(dispatch.slots, "claim_conflicts", return_value=True):
            self.guard.check()
        self.assertTrue(self.guard.paused)
        self.guard.tree.set_suspended.assert_called_with(True)
        self.guard.check()
        self.assertFalse(self.guard.paused)
        self.assertEqual(self.guard.started, started)
        self.assertEqual([t["paused"] for t in self.guard.pause_transitions], [True, False])

    def test_outer_suspended_claim_is_valid_but_cannot_resume_children(self):
        self.claim["state"] = "suspended"
        self.guard.check()
        self.assertTrue(self.guard.paused)
        self.guard.tree.set_suspended.assert_called_with(True)

    def test_claim_replacement_or_death_is_rejected(self):
        self.guard.check()
        self.claim["creation_time"] += 1
        with self.assertRaisesRegex(ValueError, "claim identity changed"):
            self.guard.check()
        with mock.patch.object(dispatch.slots, "process_state", return_value="absent"):
            with self.assertRaisesRegex(ValueError, "live matching"):
                self.guard.check()

    def test_stop_cpu_and_wall_still_fail_while_paused(self):
        with mock.patch.object(dispatch.slots, "claim_conflicts", return_value=True):
            self.guard.check()
            for field, value, message in [("cpu_seconds", 10, "aggregate process CPU"),
                                           ("seconds", 20, "batch wall")]:
                with self.subTest(field=field):
                    self.guard.sample.return_value = dict(cpu_seconds=1, seconds=2, active_processes=1)
                    self.guard.sample.return_value[field] = value
                    with self.assertRaisesRegex(ValueError, message):
                        self.guard.check()
            self.guard.sample.return_value = dict(cpu_seconds=1, seconds=2, active_processes=1)
            Path(self.guard.plan["stop_files"][0]).touch()
            with self.assertRaisesRegex(ValueError, "STOP requested"):
                self.guard.check()
        self.guard.tree.set_suspended.assert_called_with(True)

    def test_failed_budget_cannot_resume_previously_paused_children(self):
        self.guard.paused = True
        self.guard.sample.return_value["cpu_seconds"] = 11
        with self.assertRaisesRegex(ValueError, "aggregate process CPU"):
            self.guard.check()
        self.guard.tree.set_suspended.assert_not_called()

    def test_storage_admission_and_reserve_still_apply(self):
        with mock.patch.object(dispatch.slots, "claim_conflicts", return_value=True):
            self.guard.plan["storage"]["max_physical_bytes"] = 199
            with self.assertRaisesRegex(ValueError, "physical output cap"):
                self.guard.check(admission=True)
            self.guard.plan["storage"]["max_physical_bytes"] = 1000
            with mock.patch.object(dispatch.shutil, "disk_usage", return_value=SimpleNamespace(free=60 * 1024**3)):
                with self.assertRaisesRegex(ValueError, "reserve unavailable"):
                    self.guard.check(admission=True)

    @unittest.skipUnless(os.name == "nt", "Windows suspended process creation API")
    def test_new_conflict_at_spawn_adds_pause_before_removing_creation_count(self):
        self.guard.check = mock.Mock()
        child = mock.Mock(pid=789, _handle=123)
        with mock.patch.object(dispatch.slots, "claim_conflicts", side_effect=[False, True]), \
             mock.patch.object(dispatch.subprocess, "Popen", return_value=child), \
             mock.patch.object(dispatch.slots, "creation_time", return_value=999), \
             mock.patch.object(dispatch.slots, "_NtResumeProcess", return_value=0) as resume:
            self.assertIs(self.guard.spawn(["python"], None, None), child)
        self.guard.tree.adopt.assert_called_once_with(789, 999)
        self.guard.tree.set_suspended.assert_called_with(True)
        resume.assert_called_once()
        self.assertTrue(self.guard.paused)


@unittest.skipUnless(os.name == "nt" and os.environ.get(dispatch.slots.CLAIM_ENV),
                     "actual Python-child test requires Windows host_slots")
class WindowsSuspensionTest(unittest.TestCase):
    def test_python_child_waits_through_spawn_handoff_and_running_pause(self):
        parent = Path("D:/e-scratch/scorer-only-acquisition-20261015/engineering-validation")
        cores = dispatch.slots.parse_cores(os.environ[dispatch.slots.CORES_ENV])
        root = Path(tempfile.mkdtemp(prefix="archive-pause-test-", dir=parent))
        plan = dict(cpu_affinity=cores, run_limits=dict(cpu_seconds=15, wall_seconds=30),
            storage=dict(accounting_roots=[str(root)], projected_additional_bytes=1024**2,
                max_physical_bytes=4 * 1024**2, reserve_bytes=60 * 1024**3), stop_files=[str(root / "STOP")])
        guard = dispatch.Guard(plan, root / "guard", False, pause_on_reservation=True)
        heartbeat = root / "heartbeat.txt"
        command = [sys.executable, "-B", "-c",
            "import time,sys; f=open(sys.argv[1],'w'); "
            "\nwhile True: f.write('x'); f.flush(); time.sleep(.02)", str(heartbeat)]
        child = None
        record = dict(complete=False, scope="Python child only; injected conflict, no reservation edits")
        try:
            conflict = threading.Event()
            adopt = guard.tree.adopt

            def handoff_after_adoption(*args):
                adopt(*args)
                conflict.set()

            with mock.patch.object(dispatch.slots, "claim_conflicts", side_effect=lambda *_: conflict.is_set()), \
                 mock.patch.object(guard.tree, "adopt", side_effect=handoff_after_adoption):
                with (root / "child.log").open("x") as log:
                    child = guard.spawn(command, log, subprocess.STDOUT)
                time.sleep(.25)
                self.assertFalse(heartbeat.exists(), "child executed during injected spawn handoff")
                self.assertIsNone(child.poll())
                conflict.clear()
                guard.check()
            deadline = time.monotonic() + 5
            while not heartbeat.exists() or heartbeat.stat().st_size < 3:
                self.assertIsNone(child.poll())
                self.assertLess(time.monotonic(), deadline)
                time.sleep(.02)
            with mock.patch.object(dispatch.slots, "claim_conflicts", return_value=True):
                guard.check()
                before = heartbeat.stat().st_size
                time.sleep(.25)
                self.assertEqual(heartbeat.stat().st_size, before)
            guard.check()
            deadline = time.monotonic() + 3
            while heartbeat.stat().st_size == before:
                self.assertLess(time.monotonic(), deadline)
                time.sleep(.02)
            self.assertEqual([t["paused"] for t in guard.pause_transitions], [True, False, True, False])
            record.update(complete=True, spawn_handoff_executed_no_child_code=True,
                paused_heartbeat_bytes=before, resumed_heartbeat_bytes=heartbeat.stat().st_size)
        finally:
            sample = guard.terminate_and_drain()
            if child is not None:
                child.wait(timeout=3)
            guard.close()
            with (root / "RESULT.json").open("x") as out:
                json.dump(dict(**record, guard=sample, pause_transitions=guard.pause_transitions), out, indent=2)
            self.assertEqual(sample["active_processes"], 0)


if __name__ == "__main__":
    unittest.main()
