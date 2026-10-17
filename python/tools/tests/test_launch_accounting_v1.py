"""Real nested Windows jobs, using only bounded Python children and configured TEMP."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import archive_distillation_dispatch_v1 as dispatch


BURN = "import time; end=time.process_time()+.15\nwhile time.process_time()<end: pass"
HEARTBEAT = ("import pathlib,sys,time; f=pathlib.Path(sys.argv[1]).open('w')"
             "\nwhile True: f.write('x'); f.flush(); time.sleep(.02)")


@unittest.skipUnless(os.name == "nt" and os.environ.get(dispatch.slots.CLAIM_ENV),
                     "real nested accounting requires Windows host_slots")
class WindowsLaunchAccountingTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="launch-accounting-")).resolve()
        self.cores = dispatch.slots.parse_cores(os.environ[dispatch.slots.CORES_ENV])
        plan = dict(cpu_affinity=self.cores, run_limits=dict(cpu_seconds=20, wall_seconds=40),
            storage=dict(accounting_roots=[str(self.root)], projected_additional_bytes=1024**2,
                max_physical_bytes=8*1024**2, reserve_bytes=60*1024**3), stop_files=[str(self.root/"STOP")])
        self.guard = dispatch.Guard(plan, self.root/"guard", False)
        self.owners, self.children = [], []
        self.addCleanup(self.finish)

    def owner(self, cores=None):
        value = dispatch.LaunchAccounting(self.cores if cores is None else cores)
        self.owners.append(value)
        return value

    def launch(self, owner, code, *arguments):
        with (self.root/f"child-{len(self.children)}.log").open("a") as log:
            child = self.guard.spawn([sys.executable, "-B", "-c", code, *map(str, arguments)],
                                     log, subprocess.STDOUT, accounting=owner)
        self.children.append(child)
        return child

    def wait_for(self, predicate):
        until = time.monotonic()+5
        while not predicate():
            self.guard.check()
            self.assertLess(time.monotonic(), until, "bounded child handshake did not complete")
            time.sleep(.01)

    def finish(self):
        # Keep failure logs and partial accounting under the configured TEMP.
        errors, launches = [], []
        for owner in self.owners:
            try:
                owner.close()
            except BaseException as error:
                errors.append(str(error))
            launches.append(dict(last=owner.last_sample, final=owner.final_sample,
                                 errors=owner.errors, launch_error=owner.launch_error))
        try:
            sample = self.guard.terminate_and_drain()
            for child in self.children:
                child.wait(timeout=3)
        finally:
            self.guard.close()
        (self.root/"RESULT.json").write_text(json.dumps(dict(launches=launches, guard=sample,
            cleanup_errors=errors), indent=2), encoding="utf-8")
        self.assertFalse(errors, "launch cleanup failed: "+repr(errors))
        self.assertEqual(sample["active_processes"], 0)

    def test_descendant_cpu_isolated_from_coordinator_and_cancelled_peer(self):
        peer, survivor, completed = self.owner(), self.owner(), self.owner()
        trigger, ready, done = [self.root/name for name in ("trigger", "ready", "done")]
        peer_code = ("import pathlib,sys,time; trigger,ready,done=map(pathlib.Path,sys.argv[1:]); ready.touch()"
                     "\nwhile not trigger.exists(): time.sleep(.01)"
                     "\nend=time.process_time()+.15"
                     "\nwhile time.process_time()<end: pass"
                     "\ndone.touch()"
                     "\nwhile True: time.sleep(.02)")
        peer_child = self.launch(peer, peer_code, trigger, ready, done)
        heartbeat = self.root/"survivor"
        survivor_child = self.launch(survivor, HEARTBEAT, heartbeat)
        # A console Python descendant otherwise creates an extra conhost process.
        child = self.launch(completed,
            "import subprocess,sys; subprocess.run([sys.executable,'-B','-c',sys.argv[1]],"
            "check=True,creationflags=subprocess.CREATE_NO_WINDOW)", BURN)
        self.assertEqual(child.wait(timeout=8), 0)
        self.wait_for(lambda: ready.exists() and heartbeat.exists())
        # A completed job can be quiet while siblings remain active in Guard.
        result = completed.quiet()
        self.assertEqual(result["total_processes"], 2)
        self.assertEqual(result["active_processes"], 0)
        self.assertGreater(result["cpu_100ns"], 0)
        self.assertEqual(result["cpu_100ns"], result["user_100ns"]+result["kernel_100ns"])
        self.assertEqual(result["cpu_seconds"], result["cpu_100ns"]/1e7)
        self.assertTrue(result["natural_drain"])
        completed.close()
        frozen = completed.sample()
        self.assertTrue(frozen["natural_drain"])
        before = peer.sample()["cpu_100ns"]
        end = time.process_time()+.06
        while time.process_time() < end:
            pass
        trigger.touch()
        self.wait_for(done.exists)
        self.assertGreater(peer.sample()["cpu_100ns"], before)
        self.assertEqual(completed.sample(), frozen)
        killed = peer.terminate_and_drain()
        self.assertEqual(peer_child.wait(timeout=3), 1)
        self.assertEqual(killed["active_processes"], 0)
        self.assertGreater(killed["cpu_100ns"], before)
        self.assertTrue(killed["termination_requested"])
        self.assertFalse(killed["natural_drain"])
        peer.close()
        size = heartbeat.stat().st_size
        self.wait_for(lambda: heartbeat.stat().st_size > size)
        self.assertIsNone(survivor_child.poll())
        self.assertEqual(survivor.sample()["active_processes"], 1)
        closed = survivor.close()
        self.assertEqual(survivor_child.wait(timeout=3), 1)
        self.assertEqual(closed["active_processes"], 0)
        self.assertTrue(closed["termination_requested"])
        self.assertFalse(closed["natural_drain"])

    def test_failed_adoption_and_resume_preserve_identity_without_executing_child(self):
        for stage in ("before_adopt", "after_adopt", "resume"):
            with self.subTest(stage=stage):
                owner, marker = self.owner(), self.root/stage
                adopt = owner.tree.adopt

                def broken_adopt(*args):
                    # The accounting handle cannot be closed during admission.
                    with self.assertRaisesRegex(ValueError, "in flight"):
                        owner.close()
                    if stage == "after_adopt":
                        adopt(*args)
                    raise RuntimeError("injected nested adoption failure")

                patch = (mock.patch.object(dispatch.slots, "_NtResumeProcess", return_value=-1)
                         if stage == "resume" else mock.patch.object(owner.tree, "adopt", side_effect=broken_adopt))
                with patch, self.assertRaisesRegex((RuntimeError, ValueError), "injected|cannot resume"):
                    self.launch(owner, "import pathlib,sys; pathlib.Path(sys.argv[1]).touch()", marker)
                self.assertIsInstance(owner.pid, int)
                self.assertIsInstance(owner.creation_time, int)
                self.assertTrue(owner.launch_error)
                self.assertFalse(marker.exists(), "failed suspended admission executed child code")
                self.assertEqual(owner.sample()["active_processes"], 0)
                self.assertFalse(owner.sample()["natural_drain"])
                self.assertTrue(owner.sample()["termination_requested"])
                self.assertTrue(any(error["phase"] == "launch" for error in owner.errors))
                with mock.patch.object(dispatch.subprocess, "Popen") as spawn:
                    with self.assertRaisesRegex(ValueError, "fresh open"):
                        self.guard.spawn([sys.executable], None, None, accounting=owner)
                    spawn.assert_not_called()
                owner.close()
        self.guard.quiet()

    def test_fresh_exact_core_owner_required_before_creating_process(self):
        self.assertGreaterEqual(len(self.cores), 2, "guarded fixture requires two allocated cores")
        owner = self.owner(self.cores[:1])
        with mock.patch.object(dispatch.subprocess, "Popen") as spawn:
            with self.assertRaisesRegex(ValueError, "exact cores"):
                self.guard.spawn([sys.executable], None, None, accounting=owner)
            with self.assertRaisesRegex(ValueError, "LaunchAccounting owner"):
                self.guard.spawn([sys.executable], None, None, accounting=object())
            owner.close()
            with self.assertRaisesRegex(ValueError, "fresh open"):
                self.guard.spawn([sys.executable], None, None, accounting=owner)
            terminated = self.owner()
            terminated.terminate_and_drain()
            with self.assertRaisesRegex(ValueError, "fresh open"):
                self.guard.spawn([sys.executable], None, None, accounting=terminated)
            spawn.assert_not_called()

    def test_lingering_descendant_rejects_natural_drain_retains_cpu_and_can_be_cancelled(self):
        owner, ready = self.owner(), self.root/"descendant-ready"
        descendant = BURN+"\nimport pathlib,sys; pathlib.Path(sys.argv[1]).touch()\ntime.sleep(20)"
        child = self.launch(owner,
            "import subprocess,sys; subprocess.Popen([sys.executable,'-B','-c',sys.argv[1],sys.argv[2]],"
            "creationflags=subprocess.CREATE_NO_WINDOW)",
            descendant, ready)
        self.assertEqual(child.wait(timeout=5), 0)
        self.wait_for(ready.exists)
        with self.assertRaisesRegex(ValueError, "descendants remain"):
            owner.quiet(timeout=.05)
        partial = owner.sample()
        self.assertEqual(partial["active_processes"], 1)
        self.assertEqual(partial["total_processes"], 2)
        self.assertGreater(partial["cpu_100ns"], 0)
        self.assertFalse(partial["natural_drain"])
        final = owner.terminate_and_drain()
        self.assertEqual(final["active_processes"], 0)
        self.assertGreaterEqual(final["cpu_100ns"], partial["cpu_100ns"])
        self.assertFalse(final["natural_drain"])
        self.assertTrue(any(error["phase"] == "quiet" for error in owner.errors))
        owner.close()
        with self.assertRaisesRegex(ValueError, "running open"):
            owner.quiet()
        following = self.owner()
        self.assertEqual(self.launch(following, "pass").wait(timeout=5), 0)
        self.assertTrue(following.quiet()["natural_drain"])

    def test_query_failure_preserves_last_sample_and_original_close_error(self):
        owner = self.owner()
        before = owner.sample()
        query, close = dispatch.slots._QueryInformationJobObject, dispatch.slots._CloseHandle
        owner_job = owner.tree.job
        with mock.patch.object(dispatch.slots, "_QueryInformationJobObject",
                side_effect=lambda job, *args: 0 if job == owner_job else query(job, *args)):
            with self.assertRaisesRegex(ValueError, "cannot query launch accounting"):
                owner.sample()
            self.assertEqual(owner.last_sample, before)
            with self.assertRaisesRegex(ValueError, "cannot query launch accounting"):
                owner.close()
        self.assertIsNone(owner.final_sample)
        self.assertEqual(owner.last_sample, before)
        with self.assertRaisesRegex(ValueError, "without a valid final sample"):
            owner.sample()
        # Failure was deliberately exercised and recorded; do not close it again in cleanup.
        self.owners.remove(owner)
        failed_close = self.owner()
        failed_job = failed_close.tree.job
        with mock.patch.object(dispatch.slots, "_QueryInformationJobObject",
                side_effect=lambda job, *args: 0 if job == failed_job else query(job, *args)), \
             mock.patch.object(dispatch.slots, "_CloseHandle",
                side_effect=lambda job: 0 if job == failed_job else close(job)):
            with self.assertRaisesRegex(ValueError, "cannot query launch accounting") as caught:
                failed_close.close()
        self.assertTrue(any("close_handle" in note for note in caught.exception.__notes__))
        self.assertEqual([item["phase"] for item in failed_close.errors], ["query", "close", "close_handle"])
        failed_close.close()  # The failed handle close remains retryable after the injected errors.


if __name__ == "__main__":
    unittest.main()
