"""Core slots beside the v1 host reservation: claims, queue, suspension under timed work, declarations.

Ported from spellbench python/tests/test_host_slots.py (pytest) to unittest, which the kernel's shard runner
discovers. Test bodies keep spellbench's plain asserts so the two files compare line by line; spellbench's
usable_cpus() test is left out because it imports spellbench. Every test uses its own temporary root."""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch

TOOLS = Path(__file__).resolve().parents[1] / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))
import host_slots_v1 as slots

TOKEN = "0123456789abcdef0123456789abcdef"
CPUS = slots.current_affinity()
needs_two = unittest.skipIf(len(CPUS) < 2, "needs at least two usable CPUs")
needs_three = unittest.skipIf(len(CPUS) < 3, "needs at least three usable CPUs")


def lock(root, token=TOKEN, cores=None):
    record = {"schema": slots.LOCK_SCHEMA, "token": token, "lane": "board", "work_id": "run-1",
              "acquired_at": "2026-10-07T00:00:00Z"}
    if cores is not None:
        record["cores"] = cores
    (root / "TESTHOST.lock").write_text(json.dumps(record), encoding="utf-8")


def declare(root, cores, token=TOKEN):
    pid, creation = slots.self_identity()
    digest = slots.token_hash(token)
    slots.write_json(root / f"TESTHOST.slots/share-{digest[:16]}.json", {
        "schema": slots.SCHEMA, "token_sha256": digest, "cores": cores, "pid": pid, "creation_time": creation})


def script(tmp_path, name, body):
    path = tmp_path / name
    path.write_text(f"import json, os, sys, time\nsys.path.insert(0, {str(TOOLS)!r})\nimport host_slots_v1 as h\n"
                    + body, encoding="utf-8")
    return str(path)


def cli(*args, **kwargs):
    return subprocess.Popen([sys.executable, str(TOOLS / "host_slots_v1.py"), *args], stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True, **kwargs)


def wait_for(predicate, timeout=20.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


def counter(path):
    try:
        return int(Path(path).read_text() or 0)
    except (OSError, ValueError):
        return 0


class SlotMutexDeletionTests(unittest.TestCase):
    def test_transient_windows_denial_waits_for_atomic_acquisition(self):
        denied = PermissionError("transient directory acquisition denial")
        denied.winerror = 5
        with tempfile.TemporaryDirectory() as temp, \
                patch.object(slots, "slots_dir", return_value=Path(temp)):
            mutex = slots.Mutex()
            mkdir = Path.mkdir
            attempts = []

            def acquire(path, *args, **kwargs):
                if path == mutex.path:
                    attempts.append(path)
                    self.assertFalse((path / "owner.json").exists())
                    if len(attempts) < 3:
                        raise denied
                return mkdir(path, *args, **kwargs)

            with patch.object(slots, "os", wraps=os) as platform, \
                    patch.object(slots, "self_identity", return_value=(os.getpid(), "creation")), \
                    patch.object(Path, "mkdir", acquire), \
                    patch.object(slots.time, "sleep") as sleep:
                platform.name = "nt"
                with mutex:
                    self.assertEqual(len(attempts), 3)
                    owner = slots.read_json(mutex.path / "owner.json")
                    self.assertEqual(owner["pid"], os.getpid())
                self.assertEqual(sleep.call_count, 2)
            self.assertFalse(mutex.path.exists())

    def test_live_holder_excludes_a_second_holder_until_release(self):
        import threading
        acquired, release, done = (threading.Event() for _ in range(3))
        failures = []
        with tempfile.TemporaryDirectory() as temp, \
                patch.object(slots, "slots_dir", return_value=Path(temp)), \
                patch.object(slots, "MUTEX_TIMEOUT_SECONDS", 2):
            old = slots.Mutex().__enter__()
            thread = None
            try:
                def contender():
                    try:
                        with slots.Mutex():
                            acquired.set()
                            release.wait(2)
                    except BaseException as error:
                        failures.append(error)
                    finally:
                        done.set()

                thread = threading.Thread(target=contender, daemon=True)
                thread.start()
                self.assertFalse(acquired.wait(0.1), "a live holder must exclude the contender")
                self.assertFalse(done.is_set(), f"contender failed rather than waited: {failures}")
                old.__exit__()
                old = None
                self.assertTrue(acquired.wait(2), f"contender did not acquire after deletion: {failures}")
            finally:
                if old is not None:
                    old.__exit__()
                release.set()
                if thread is not None:
                    thread.join(3)
            self.assertFalse(thread.is_alive())
            self.assertEqual(failures, [])

    def test_persistent_windows_denial_expires_without_writing_an_owner(self):
        denied = PermissionError("persistent permission denial")
        denied.winerror = 5
        mutex = object.__new__(slots.Mutex)
        mutex.path = MagicMock()
        mutex.path.mkdir.side_effect = denied
        with patch.object(slots, "os") as platform, \
                patch.object(slots, "self_identity", return_value=(123, "creation")), \
                patch.object(slots, "MUTEX_TIMEOUT_SECONDS", 0.01), \
                patch.object(slots.time, "monotonic", side_effect=[0, 0.02]), \
                patch.object(slots, "write_json") as write:
            platform.name = "nt"
            with self.assertRaises(PermissionError) as failure:
                mutex.__enter__()
            self.assertIs(failure.exception, denied)
            write.assert_not_called()

    def test_stale_directory_rename_denial_cannot_spin_past_deadline(self):
        mutex = object.__new__(slots.Mutex)
        mutex.path = MagicMock()
        mutex.path.mkdir.side_effect = FileExistsError("stale directory")
        with patch.object(slots, "os") as platform, \
                patch.object(slots, "self_identity", return_value=(123, "creation")), \
                patch.object(slots, "MUTEX_TIMEOUT_SECONDS", 0.01), \
                patch.object(mutex, "_stale", return_value=True), \
                patch.object(slots.time, "monotonic", side_effect=[0, 0, 0.02]), \
                patch.object(slots.time, "sleep"), \
                patch.object(slots, "write_json") as write:
            platform.name = "nt"
            platform.rename.side_effect = PermissionError("stale rename denied")
            with self.assertRaises(TimeoutError):
                mutex.__enter__()
            platform.rename.assert_called_once()
            write.assert_not_called()

    def test_other_permission_errors_do_not_retry_or_write_an_owner(self):
        for platform_name, winerror in [("posix", 5), ("nt", 87)]:
            with self.subTest(platform=platform_name, winerror=winerror):
                denied = PermissionError("different refusal")
                denied.winerror = winerror
                mutex = object.__new__(slots.Mutex)
                mutex.path = MagicMock()
                mutex.path.mkdir.side_effect = denied
                with patch.object(slots, "os") as platform, \
                        patch.object(slots, "self_identity", return_value=(123, "creation")), \
                        patch.object(slots, "write_json") as write, \
                        patch.object(slots.time, "sleep") as sleep:
                    platform.name = platform_name
                    with self.assertRaises(PermissionError):
                        mutex.__enter__()
                    write.assert_not_called()
                    sleep.assert_not_called()


class SuspensionHandshakeTests(unittest.TestCase):
    def test_failed_windows_suspend_never_acknowledges_vacated_cores(self):
        tree = object.__new__(slots.Tree)
        tree.suspended = set()
        tree.members = MagicMock(return_value=[11, 12])
        with patch.object(slots, "os", name="nt") as fake_os, \
                patch.object(slots, "_suspend_pid", side_effect=lambda pid, suspend: pid == 11, create=True):
            fake_os.name = "nt"
            self.assertFalse(tree.set_suspended(True))
            self.assertEqual(tree.suspended, {11})
        with patch.object(slots, "os") as fake_os, \
                patch.object(slots, "_suspend_pid", return_value=True, create=True):
            fake_os.name = "nt"
            self.assertTrue(tree.set_suspended(True))
            self.assertEqual(tree.suspended, {11, 12})

    def test_failed_resume_remains_tracked_for_the_next_attempt(self):
        tree = object.__new__(slots.Tree)
        tree.suspended = {11, 12}
        tree.members = MagicMock(return_value=[11, 12])
        with patch.object(slots, "os") as fake_os, \
                patch.object(slots, "_suspend_pid", side_effect=lambda pid, suspend: pid == 11, create=True):
            fake_os.name = "nt"
            self.assertFalse(tree.set_suspended(False))
            self.assertEqual(tree.suspended, {12})
        with patch.object(slots, "os") as fake_os, \
                patch.object(slots, "_suspend_pid", return_value=True, create=True):
            fake_os.name = "nt"
            self.assertTrue(tree.set_suspended(False))
            self.assertEqual(tree.suspended, set())

    def test_start_resume_and_acknowledgement_share_the_admission_mutex(self):
        claim = Path("unused-claim.json")
        for suspend_succeeds in (False, True):
            with self.subTest(suspend_succeeds=suspend_succeeds):
                locked = [False]
                states = []
                class TestMutex:
                    def __enter__(inner):
                        self.assertFalse(locked[0])
                        locked[0] = True
                    def __exit__(inner, *_):
                        locked[0] = False
                child = MagicMock()
                child.wait.side_effect = [subprocess.TimeoutExpired("probe", 0), 0]
                tree = MagicMock()
                tree.members.side_effect = [[11], []]
                def start(*_):
                    self.assertTrue(locked[0])
                    return child
                def transition(suspend):
                    self.assertTrue(locked[0])
                    return suspend_succeeds if suspend else True
                def publish(_claim, state):
                    self.assertTrue(locked[0])
                    states.append(state)
                tree.start.side_effect = start
                tree.set_suspended.side_effect = transition
                with patch.object(slots, "Tree", return_value=tree), \
                        patch.object(slots, "Mutex", TestMutex), \
                        patch.object(slots, "conflicts", side_effect=[False, True, False]), \
                        patch.object(slots, "_set_state_locked", side_effect=publish), \
                        patch.object(slots.time, "sleep"), \
                        patch.object(slots.signal, "signal"):
                    self.assertEqual(slots._run_claimed(claim, [0], "normal", ["probe"], None, "x"), 0)
                self.assertEqual(states, ["running", "suspended" if suspend_succeeds else "running", "running"])
                tree.kill.assert_not_called()


class CoreListTests(unittest.TestCase):
    def test_core_lists_round_trip(self):
        assert slots.parse_cores("0-3,8,11-10") == [0, 1, 2, 3, 8, 11, 10]
        assert slots.format_cores([8, 0, 1, 2, 5, 7]) == "0-2,5,7-8"
        with self.assertRaises(ValueError):
            slots.parse_cores("a-b")

    def test_available_respects_timed_declarations_claims_and_keep_free(self):
        config = {"cores": [7, 6, 5, 4, 3, 2, 1, 0], "keep_free": 2, "priority": "below_normal"}
        free = {"held": False, "cores": []}
        assert slots.available(config, free, []) == [7, 6, 5, 4, 3, 2]
        assert slots.available(config, {"held": True, "cores": None}, []) == []
        assert slots.available(config, {"held": True, "cores": [0, 1, 2, 3]}, []) == [7, 6, 5, 4]
        claims = [{"cores": [7, 6]}]
        assert slots.available(config, {"held": True, "cores": [0, 1, 2, 3]}, claims) == [5, 4]

    def test_topology_lists_every_cpu_once(self):
        rows = slots.topology()
        listed = [cpu for row in rows for cpu in slots.parse_cores(row["cpus"])]
        assert sorted(listed) == list(range(slots.cpu_total()))


class HostSlotsTests(unittest.TestCase):
    def setUp(self):
        tmp_path = Path(tempfile.mkdtemp(prefix="host-slots-"))
        self.addCleanup(shutil.rmtree, tmp_path, ignore_errors=True)
        root = tmp_path / "host-lock"
        root.mkdir()
        env = patch.dict(os.environ, {slots.ROOT_ENV: str(root), slots.HOST_ENV: "TESTHOST", slots.POLL_ENV: "0.1"})
        env.start()
        self.addCleanup(env.stop)
        os.environ.pop(slots.TOKEN_ENV, None)
        (root / "TESTHOST.slots").mkdir()
        slots.write_json(root / "TESTHOST.slots/config.json", {"cores": ",".join(map(str, reversed(CPUS)))})
        self.host, self.tmp_path = root, tmp_path

    @needs_two
    def test_request_core_pool_is_contained_and_cannot_borrow_timed_cores(self):
        chosen = CPUS[0]
        slots.write_json(self.host / "TESTHOST.slots/config.json",
                         {"cores": ",".join(map(str, reversed(CPUS))), "keep_free": 1})
        probe = script(self.tmp_path, "pool.py", "print(json.dumps({'affinity': h.current_affinity()}))\n")
        arguments = ["run", "--lane", "pool", "--work-id", "bounded", "--cores", "1",
                     "--core-pool", str(chosen), "--", sys.executable, probe]
        proc = cli(*arguments)
        out, err = proc.communicate(timeout=60)
        self.assertEqual(proc.returncode, 0, err)
        rows = [json.loads(line) for line in out.splitlines()]
        self.assertEqual(rows[0]["cores"], str(chosen))
        self.assertEqual(rows[1]["affinity"], [chosen])
        lock(self.host)
        declare(self.host, [chosen])
        proc = cli(*arguments)
        _, err = proc.communicate(timeout=20)
        self.assertEqual(proc.returncode, slots.EXIT_HELD, err)
        self.assertFalse(list((self.host / "TESTHOST.slots").glob("claim-*.json")))
        self.assertFalse(list((self.host / "TESTHOST.slots").glob("queue-*.json")))

    def test_timed_state_is_the_whole_host_unless_a_live_declaration_says_otherwise(self):
        host = self.host
        assert slots.timed_state()["held"] is False
        lock(host)
        state = slots.timed_state()
        assert state["held"] and state["cores"] is None and state["lane"] == "board"
        declare(host, [0])
        assert slots.timed_state()["cores"] == [0]
        declare(host, [0], token="f" * 32)  # a declaration for another reservation counts for nothing
        (host / f"TESTHOST.slots/share-{slots.token_hash(TOKEN)[:16]}.json").unlink()
        assert slots.timed_state()["cores"] is None
        (host / "TESTHOST.lock").write_text("not json")
        assert slots.timed_state() == {"held": True, "cores": None, "why": "lock record unreadable or incomplete"}

    def test_cores_recorded_at_acquire_count_before_and_after_the_timed_declaration(self):
        host = self.host
        lock(host, cores=[0, 1])
        state = slots.timed_state()
        assert state["held"] and state["cores"] == [0, 1] and "acquired" in state["why"]
        assert slots.conflicts([1]) and not slots.conflicts([2])
        config = {"cores": [3, 2, 1, 0], "keep_free": 0, "priority": "below_normal"}
        assert slots.available(config, state, []) == [3, 2]
        declare(host, [1])  # a live declaration by timed wins while it lasts
        assert slots.timed_state()["cores"] == [1]
        (host / f"TESTHOST.slots/share-{slots.token_hash(TOKEN)[:16]}.json").unlink()
        assert slots.timed_state()["cores"] == [0, 1]  # after timed ends: the recorded cores, not the whole host
        for bad in ([], [-1], [0, 0], ["0"], [True], "0-1"):
            lock(host, cores=bad)
            assert slots.timed_state()["cores"] is None, bad

    def test_declaration_whose_declarer_exited_reserves_the_whole_host(self):
        host = self.host
        lock(host)
        gone = subprocess.Popen([sys.executable, "-c", "pass"])
        gone.wait()
        digest = slots.token_hash(TOKEN)
        slots.write_json(host / f"TESTHOST.slots/share-{digest[:16]}.json", {
            "token_sha256": digest, "cores": [0], "pid": gone.pid, "creation_time": 1})
        assert slots.timed_state()["cores"] is None

    def test_run_pins_the_command_and_frees_its_claim(self):
        host = self.host
        tmp_path = self.tmp_path
        probe = script(tmp_path, "probe.py", "print(json.dumps({'affinity': h.current_affinity(), "
                                             "'claim': os.environ.get('HOST_SLOTS_CLAIM'), "
                                             "'cores': os.environ.get('HOST_SLOTS_CORES')}))\n")
        proc = cli("run", "--lane", "smoke", "--work-id", "w1", "--cores", "1", "--", sys.executable, probe)
        out, err = proc.communicate(timeout=60)
        assert proc.returncode == 0, err
        lines = [json.loads(line) for line in out.splitlines()]
        claimed, seen = lines[0], lines[1]
        assert seen["affinity"] == [int(claimed["cores"])] == [CPUS[-1]]  # highest-numbered first
        assert seen["claim"] == claimed["claimed"] and seen["cores"] == claimed["cores"]
        assert not list((host / "TESTHOST.slots").glob("claim-*.json"))

    @unittest.skipIf(os.name == "nt", "nice values are POSIX")
    def test_run_lowers_priority(self):
        tmp_path = self.tmp_path
        probe = script(tmp_path, "nice.py", "print(os.getpriority(os.PRIO_PROCESS, 0))\n")
        out = subprocess.run([sys.executable, str(TOOLS / "host_slots_v1.py"), "run", "--lane", "l", "--work-id", "w",
                              "--cores", "1", "--", sys.executable, probe], capture_output=True, text=True, timeout=60)
        assert int(out.stdout.splitlines()[1]) >= 10

    def test_no_room_without_wait_and_fifo_queue_with_wait(self):
        host = self.host
        tmp_path = self.tmp_path
        hold = script(tmp_path, "hold.py", "Path = __import__('pathlib').Path\nPath(sys.argv[1]).write_text('up')\n"
                                           "while not Path(sys.argv[2]).exists(): time.sleep(0.05)\n")
        up, release = tmp_path / "up", tmp_path / "release"
        first = cli("run", "--lane", "l", "--work-id", "first", "--cores", str(len(CPUS)), "--",
                    sys.executable, hold, str(up), str(release))
        try:
            assert wait_for(up.exists)
            refused = subprocess.run([sys.executable, str(TOOLS / "host_slots_v1.py"), "run", "--lane", "l",
                                      "--work-id", "second", "--cores", "1", "--", sys.executable, "-c", "pass"],
                                     capture_output=True, text=True, timeout=60)
            assert refused.returncode == slots.EXIT_HELD
            mark = script(tmp_path, "mark.py", "open(sys.argv[1], 'a').write(sys.argv[2] + '\\n')\n")
            order = tmp_path / "order"
            waiters = []
            # Full-capacity waiters make completion order an observable admission order.
            # One-core waiters can run together and finish in either order after FIFO admission.
            for name in ("a", "b"):
                waiters.append(cli("run", "--wait", "--lane", "l", "--work-id", name, "--cores", str(len(CPUS)), "--",
                                   sys.executable, mark, str(order), name))
                assert wait_for(lambda n=len(waiters): len(list((host / "TESTHOST.slots").glob("queue-*.json"))) == n)
            status = slots.status()
            assert [t["work_id"] for t in status["queue"]] == ["a", "b"] and status["free"] == ""
            release.write_text("go")
            for waiter in waiters:
                assert waiter.wait(timeout=60) == 0
            assert order.read_text().split() == ["a", "b"]
        finally:
            release.write_text("go")
            first.wait(timeout=60)

    def test_ordinary_admission_bypasses_background_and_preserves_ordinary_fifo(self):
        claim_id = "a" * 32
        background = {"id": "b" * 32, "background": True}
        earlier = {"id": "c" * 32}
        own = {"id": claim_id}
        with patch.object(slots, "live_tickets", side_effect=[
                [background, earlier, own], [background, own]]), \
                patch.object(slots, "_run_claimed", return_value=0) as launch, \
                patch.object(slots.time, "sleep") as sleep:
            result = slots.run("probe", "ordinary", 1, ["unused"], wait=True, claim_id=claim_id)
        self.assertEqual(result, 0)
        self.assertEqual(sleep.call_count, 1, "the earlier ordinary request must enter first")
        launch.assert_called_once()
        self.assertEqual(launch.call_args.args[-1], claim_id)
        self.assertFalse(list((self.host / "TESTHOST.slots").glob("claim-*.json")))
        self.assertFalse(list((self.host / "TESTHOST.slots").glob("queue-*.json")))

    def test_background_queue_does_not_deadlock_a_legacy_fifo_launcher(self):
        host, tmp_path = self.host, self.tmp_path
        lock(host)
        mark = script(tmp_path, "mixed-mark.py", "open(sys.argv[1], 'a').write(sys.argv[2] + '\\n')\n")
        legacy = script(tmp_path, "legacy-fifo.py", "class FilenameFIFO(list):\n"
                        "    def sort(self, *args, **kwargs): pass\n"
                        "read_tickets = h.live_tickets\n"
                        "h.live_tickets = lambda: FilenameFIFO(read_tickets())\n"
                        "raise SystemExit(h.main())\n")
        order = tmp_path / "mixed-order"
        ci = cli("run", "--wait", "--background", "--priority", "idle", "--lane", "ci",
                 "--work-id", "older-ci", "--cores", str(len(CPUS)), "--",
                 sys.executable, mark, str(order), "ci")
        foreground = None
        try:
            self.assertTrue(wait_for(lambda: len(slots.status()["queue"]) == 1))
            foreground = subprocess.Popen([sys.executable, legacy, "run", "--wait", "--lane", "build",
                          "--work-id", "legacy-build", "--cores", str(len(CPUS)), "--",
                          sys.executable, mark, str(order), "build"], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            self.assertTrue(wait_for(lambda: len(slots.status()["queue"]) == 2))
            (host / "TESTHOST.lock").unlink()
            foreground.communicate(timeout=30)
            ci.communicate(timeout=30)
            self.assertEqual(foreground.returncode, 0)
            self.assertEqual(ci.returncode, 0)
            self.assertEqual(order.read_text().split(), ["build", "ci"])
        finally:
            (host / "TESTHOST.lock").unlink(missing_ok=True)
            for process in (foreground, ci):
                if process and process.poll() is None:
                    process.terminate()
                    process.communicate(timeout=30)

    def test_claims_suspend_while_timed_work_leaves_no_room(self):
        host = self.host
        tmp_path = self.tmp_path
        count = tmp_path / "count"
        ticker = script(tmp_path, "tick.py", "n = 0\nwhile True:\n    n += 1\n    open(sys.argv[1], 'w').write(str(n))\n"
                                             "    time.sleep(0.02)\n")
        runner = cli("run", "--lane", "l", "--work-id", "tick", "--cores", "1", "--", sys.executable, ticker, str(count))
        try:
            assert wait_for(lambda: counter(count) > 5)
            lock(host)  # no declaration: the whole host
            assert wait_for(lambda: slots.status()["claims"][0]["state"] == "suspended")
            time.sleep(0.5)
            frozen = counter(count)
            time.sleep(1.0)
            assert counter(count) == frozen
            claimed = slots.status()["claims"][0]["cores"]
            others = [cpu for cpu in CPUS if cpu not in claimed]
            if others:  # a declaration that leaves the claim's core free lets it resume
                declare(host, others[:1])
                assert wait_for(lambda: counter(count) > frozen + 5)
                declare(host, claimed)  # an overlapping one suspends it again
                assert wait_for(lambda: slots.status()["claims"][0]["state"] == "suspended")
                time.sleep(0.5)
                frozen = counter(count)
            (host / "TESTHOST.lock").unlink()
            assert wait_for(lambda: counter(count) > frozen + 5)
        finally:
            runner.terminate()
            runner.wait(timeout=60)
        assert wait_for(lambda: not list((host / "TESTHOST.slots").glob("claim-*.json")) or slots.status()["claims"] == [])
        settled = counter(count)
        time.sleep(0.5)
        assert counter(count) == settled  # stopping the runner ended its whole tree

    @needs_two
    def test_timed_declares_pins_and_waits_for_overlapping_claims(self):
        host = self.host
        tmp_path = self.tmp_path
        lock(host)
        probe = script(tmp_path, "probe.py", "print(json.dumps({'affinity': h.current_affinity(), "
                                             "'timed': h.timed_state()['cores']}))\n")
        env = dict(os.environ, **{slots.TOKEN_ENV: TOKEN})
        proc = cli("timed", "--cores", str(CPUS[0]), "--", sys.executable, probe, env=env)
        out, err = proc.communicate(timeout=60)
        assert proc.returncode == 0, err
        seen = json.loads(out)
        assert seen == {"affinity": [CPUS[0]], "timed": [CPUS[0]]}
        assert slots.timed_state()["cores"] is None  # the declaration ends with the command
        wrong = cli("timed", "--cores", str(CPUS[0]), "--", sys.executable, "-c", "pass",
                    env=dict(os.environ, **{slots.TOKEN_ENV: "f" * 32}))
        assert wrong.wait(timeout=60) == slots.EXIT_REFUSED

    def test_dead_claims_and_tickets_are_reaped(self):
        host = self.host
        gone = subprocess.Popen([sys.executable, "-c", "pass"])
        gone.wait()
        for name in ("claim-" + "a" * 32, "queue-00000000000000000001-" + "b" * 32):
            slots.write_json(host / f"TESTHOST.slots/{name}.json", {
                "id": name[-32:], "pid": gone.pid, "creation_time": 1, "cores": [CPUS[0]], "cores_wanted": 1})
        status = slots.status()
        assert status["claims"] == [] and status["queue"] == []
        assert len(slots.parse_cores(status["free"])) == len(CPUS)


class BackgroundPolicyTests(unittest.TestCase):
    def test_ordinary_claims_can_reclaim_background_cores(self):
        config = {"cores": [0, 1, 2, 3], "keep_free": 0}
        timed = {"held": True, "cores": [0]}
        claims = [{"cores": [1, 2], "background": True}, {"cores": [3]}]
        self.assertEqual(slots.available(config, timed, claims), [])
        self.assertEqual(slots.available(config, timed, claims, background=False), [1, 2])

    def test_background_conflicts_with_live_ordinary_claims_even_before_start(self):
        with patch.object(slots, "conflicts", return_value=False), \
                patch.object(slots, "live_claims", return_value=[{"cores": [1], "state": "suspended"}]):
            self.assertTrue(slots.claim_conflicts([1], True))
            self.assertFalse(slots.claim_conflicts([2], True))
            self.assertFalse(slots.claim_conflicts([1], False))

    def test_partial_suspension_cannot_admit_foreground_work(self):
        with patch.object(slots, "live_claims", return_value=[
                {"id": "ci", "cores": [1], "background": True, "state": "running"}]):
            self.assertTrue(slots.background_busy([1], "build"))
        with patch.object(slots, "live_claims", return_value=[
                {"id": "ci", "cores": [1], "background": True, "state": "suspended"}]):
            self.assertFalse(slots.background_busy([1], "build"))


class BackgroundProcessTests(unittest.TestCase):
    setUp = HostSlotsTests.setUp
    tearDown = HostSlotsTests.tearDown
    def test_ci_suspends_for_ordinary_work_and_resumes_after_release(self):
        count = self.tmp_path / "background-count"
        release = self.tmp_path / "foreground-release"
        active = self.tmp_path / "foreground-active"
        ticker = script(self.tmp_path, "background-tick.py", "n = 0\nwhile True:\n    n += 1\n"
                        "    open(sys.argv[1], 'w').write(str(n))\n    time.sleep(0.02)\n")
        hold = script(self.tmp_path, "foreground-hold.py", "from pathlib import Path\n"
                      "Path(sys.argv[1]).write_text('up')\n"
                      "while not Path(sys.argv[2]).exists(): time.sleep(0.02)\n")
        ci = cli("run", "--background", "--priority", "idle", "--lane", "ci", "--work-id", "ci",
                 "--cores", "1", "--", sys.executable, ticker, str(count))
        foreground = None
        try:
            self.assertTrue(wait_for(lambda: counter(count) > 5))
            foreground = cli("run", "--lane", "build", "--work-id", "build", "--cores", str(len(CPUS)),
                             "--", sys.executable, hold, str(active), str(release))
            self.assertTrue(wait_for(active.exists))
            ci_claim = next(c for c in slots.status()["claims"] if c["lane"] == "ci")
            self.assertEqual(ci_claim["state"], "suspended")
            frozen = counter(count)
            time.sleep(0.2)
            self.assertEqual(counter(count), frozen)
            release.write_text("go")
            self.assertEqual(foreground.wait(timeout=30), 0)
            self.assertTrue(wait_for(lambda: counter(count) > frozen + 5))
        finally:
            release.write_text("go")
            if foreground:
                foreground.wait(timeout=30)
            ci.terminate()
            ci.wait(timeout=30)

    @unittest.skipUnless(os.name == "nt", "existing process adoption uses Windows jobs")
    def test_existing_worker_is_pinned_idle_and_claim_releases_at_exit(self):
        from ctypes import wintypes
        started, release, ready = (self.tmp_path / name for name in ("worker-started", "worker-release", "adopt-ready"))
        body = script(self.tmp_path, "existing-worker.py", "from pathlib import Path\n"
                      "Path(sys.argv[1]).write_text('up')\n"
                      "while not Path(sys.argv[2]).exists(): time.sleep(0.02)\n")
        # The fixture is an existing native worker, not the Windows venv
        # redirector and its separately contained interpreter descendant.
        worker = subprocess.Popen([getattr(sys, "_base_executable", sys.executable), body, str(started), str(release)])
        controller = None
        try:
            self.assertTrue(wait_for(started.exists))
            stamp = slots.creation_time(worker.pid)
            identity = self.tmp_path / "worker.json"
            slots.write_json(identity, {"pid": worker.pid, "creation": stamp, "descendants": []})
            controller = cli("adopt", "--background", "--priority", "idle", "--lane", "ci", "--work-id", "adopt",
                             "--cores", "1", "--worker", str(identity), "--ready", str(ready))
            wait_for(lambda: ready.exists() or controller.poll() is not None)
            if not ready.exists():
                if controller.poll() is None:
                    controller.terminate()
                out, err = controller.communicate(timeout=30)
                self.fail(f"adoption did not become ready; exit={controller.returncode}; stdout={out}; stderr={err}")
            admitted = slots.read_json(ready)
            self.assertEqual(admitted["worker_creation"], stamp)
            handle = slots._OpenProcess(slots.PROCESS_QUERY_LIMITED_INFORMATION, False, worker.pid)
            try:
                affinity, system = slots.ctypes.c_size_t(0), slots.ctypes.c_size_t(0)
                self.assertTrue(slots._GetProcessAffinityMask(handle, slots.ctypes.byref(affinity), slots.ctypes.byref(system)))
                self.assertEqual(affinity.value, 1 << CPUS[-1])
                get_priority = slots._fn(slots._k32, "GetPriorityClass", wintypes.DWORD, wintypes.HANDLE)
                self.assertEqual(get_priority(handle), slots.PRIORITY_CLASS["idle"])
            finally:
                slots._CloseHandle(handle)
            release.write_text("go")
            self.assertEqual(worker.wait(timeout=30), 0)
            out, err = controller.communicate(timeout=30)
            self.assertEqual(controller.returncode, 0, f"stdout={out}; stderr={err}")
            self.assertEqual(slots.status()["claims"], [])
            self.assertFalse(ready.exists())
        finally:
            release.write_text("go")
            worker.wait(timeout=30)
            if controller and controller.poll() is None:
                controller.terminate()
                controller.wait(timeout=30)


def config8(keep_free=0):
    return {"cores": [7, 6, 5, 4, 3, 2, 1, 0], "keep_free": keep_free, "priority": "below_normal"}


FREE = {"held": False, "cores": []}


def ticket(name, wanted, max_seconds=60.0, **extra):
    record = {"id": name, "cores_wanted": wanted, "background": False, **extra}
    if max_seconds is not None:
        record["max_seconds"] = max_seconds
    return record


def admits(config, timed, claims, tickets, claim_id):
    """backfill_admits with the head queued under this config, as run() records it."""
    tickets = [{**tickets[0], "config_sha256": slots.config_digest(config)}, *tickets[1:]]
    return slots.backfill_admits(config, timed, claims, tickets, claim_id)


class BackfillPolicyTests(unittest.TestCase):
    """Backfill: a request may start before the queue head only if no release order of the running claims
    lets the head start without backfills but not with them."""

    def test_head_with_room_is_never_backfilled_around(self):
        # the head fits now: a backfill that takes its room is refused (the empty release set)
        claims = [{"cores": [7, 6, 5, 4]}]
        assert admits(config8(), FREE, claims, [ticket("head", 4, None), ticket("t", 2)], "t") is None
        assert admits(config8(), FREE, [], [ticket("head", 4, None)], "head") is None

    def test_request_without_declared_limit_never_backfills(self):
        claims = [{"cores": [7, 6, 5, 4, 3, 2]}]
        tickets = [ticket("head", 4, None), ticket("t", 2, None)]
        assert admits(config8(), FREE, claims, tickets, "t") is None
        for bad in (0, -1, True, float("inf"), float("nan"), "60", 1e300, slots.BACKFILL_MAX_SECONDS + 1):
            assert slots._backfill_cores(config8(), FREE, claims, tickets[0], ticket("t", 2, bad)) is None

    def test_one_releasing_claim_admits_a_backfill_into_free_cores(self):
        # the 18:17 case scaled down: one large claim, two free cores, the head wants four
        claims = [{"cores": [7, 6, 5, 4, 3, 2]}]
        tickets = [ticket("head", 4, None), ticket("t", 2)]
        assert admits(config8(), FREE, claims, tickets, "t") == [0, 1]

    def test_a_release_subset_that_the_backfill_would_delay_refuses(self):
        # releasing either claim alone lets the head start (5 >= 4) without the backfill, not with it (3 < 4)
        claims = [{"cores": [7, 6, 5]}, {"cores": [4, 3, 2]}]
        tickets = [ticket("head", 4, None), ticket("t", 2)]
        assert admits(config8(), FREE, claims, tickets, "t") is None
        assert admits(config8(), FREE, claims, [tickets[0], ticket("t", 1)], "t") == [1]

    def test_existing_backfills_count_cumulatively(self):
        claims = [{"cores": [7, 6, 5, 4, 3]}, {"cores": [2], "backfill": True}]
        head = ticket("head", 6, None)
        assert admits(config8(), FREE, claims, [head, ticket("t", 2)], "t") is None
        assert admits(config8(), FREE, claims, [head, ticket("t", 1)], "t") == [1]

    def test_backfill_stays_in_the_head_class(self):
        claims = [{"cores": [7, 6, 5, 4, 3, 2]}]
        head = ticket("head", 4, None)
        assert admits(config8(), FREE, claims, [head, ticket("t", 2, background=True)], "t") is None
        background_head = ticket("head", 4, None, background=True)
        assert admits(config8(), FREE, claims, [background_head, ticket("t", 2, background=True)],
                                     "t") == [0, 1]
        # an ordinary head ignores background claims, exactly as its own admission does, but a backfill
        # never takes their cores (it would suspend them)
        claims = [{"cores": [7, 6, 5, 4, 3, 2]}, {"cores": [1], "background": True}]
        assert admits(config8(), FREE, claims, [head, ticket("t", 1)], "t") == [0]
        assert admits(config8(), FREE, claims, [head, ticket("t", 2)], "t") is None

    def test_no_backfill_while_a_timed_reservation_is_held(self):
        # review r1: the reservation may end before the claim does, and the head would then need core 1
        timed = {"held": True, "cores": [0]}
        config = {"cores": [3, 2, 1, 0], "keep_free": 0, "priority": "below_normal"}
        claims = [{"cores": [3, 2]}]
        assert admits(config, timed, claims, [ticket("head", 2, None), ticket("t", 1)], "t") is None
        assert admits(config, FREE, claims, [ticket("head", 2, None), ticket("t", 1)], "t") is None
        assert admits(config, FREE, [{"cores": [3, 2, 1]}], [ticket("head", 3, None),
                                                                           ticket("t", 1)], "t") == [0]

    def test_core_pools_count_exactly(self):
        # a head restricted to cores 0-3 never uses 7 and 6, so a backfill there cannot delay it
        claims = [{"cores": [3, 2]}, {"cores": [5, 4]}]
        head = ticket("head", 4, None, core_pool=[0, 1, 2, 3])
        assert admits(config8(), FREE, claims, [head, ticket("t", 2)], "t") == [6, 7]
        assert admits(config8(), FREE, claims, [head, ticket("t", 2, core_pool=[0, 1])], "t") is None

    def test_keep_free_counts_exactly(self):
        claims = [{"cores": [7, 6, 5, 4]}]
        # keep_free 2: two cores hand out now; releasing the claim leaves room for six, so 2 + 3 fit
        assert admits(config8(2), FREE, claims, [ticket("head", 3, None), ticket("t", 2)], "t") == [2, 3]
        assert admits(config8(2), FREE, claims, [ticket("head", 5, None), ticket("t", 2)], "t") is None

    def test_only_the_first_eligible_request_behind_the_head_starts(self):
        claims = [{"cores": [7, 6, 5, 4, 3, 2]}]
        tickets = [ticket("head", 4, None), ticket("big", 3), ticket("a", 1), ticket("b", 1)]
        assert admits(config8(), FREE, claims, tickets, "big") is None  # does not fit
        assert admits(config8(), FREE, claims, tickets, "a") == [1]
        assert admits(config8(), FREE, claims, tickets, "b") is None

    def test_more_live_claims_than_the_cap_refuses(self):
        claims = [{"cores": [7, 6, 5]}, {"cores": [4, 3, 2]}]
        tickets = [ticket("head", 2, None, core_pool=[7, 6, 5, 4, 3, 2]), ticket("t", 1)]
        assert admits(config8(), FREE, claims, tickets, "t") == [1]
        with patch.object(slots, "BACKFILL_MAX_RUNNING", 1):
            assert admits(config8(), FREE, claims, tickets, "t") is None
        # every live claim counts toward the cap: backfills and background claims too
        with patch.object(slots, "BACKFILL_MAX_RUNNING", 2):
            assert admits(config8(), FREE, claims, tickets, "t") == [1]
            for extra in ({"cores": [0], "backfill": True}, {"cores": [0], "background": True}):
                assert admits(config8(), FREE, claims + [extra], tickets, "t") is None

    def test_only_a_head_queued_under_the_current_config_is_backfilled_around(self):
        # review r2: the head admits with the config it read before queueing
        claims = [{"cores": [7, 6, 5, 4, 3, 2]}]
        head, small = ticket("head", 4, None), ticket("t", 2)
        assert slots.backfill_admits(config8(), FREE, claims, [{**head, "config_sha256": slots.config_digest(
            config8())}, small], "t") == [0, 1]
        assert slots.backfill_admits(config8(), FREE, claims, [{**head, "config_sha256": slots.config_digest(
            config8(1))}, small], "t") is None
        # review r1: a ticket from an older launcher (no background field, no config) is never jumped
        assert slots.backfill_admits(config8(), FREE, claims, [{"id": "head", "cores_wanted": 4}, small], "t") is None

    def test_a_backfill_never_takes_or_moves_the_head_onto_background_cores(self):
        # review r2: preference 0 to 4, ordinary C on 0, 1 and 4, background G on 3, a three-core head
        config = {"cores": [0, 1, 2, 3, 4], "keep_free": 0, "priority": "below_normal"}
        claims = [{"cores": [0, 1, 4]}, {"cores": [3], "background": True}]
        head = ticket("head", 3, None)
        assert admits(config, FREE, claims, [head, ticket("a", 1, core_pool=[3])], "a") is None  # G's core
        # after C ends the head takes 0 to 2 alone, but 0, 1 and 3 beside a backfill on 2: it would wait for G
        assert admits(config, FREE, claims, [head, ticket("b", 1, core_pool=[2])], "b") is None

    def test_a_backfill_never_moves_an_ordinary_head_onto_background_cores(self):
        # review r1: an ordinary head admitted onto a background claim's cores waits for it to suspend.
        # Without the backfill the head takes 3, 2 and 1 at once; with it, it would need core 0.
        config = {"cores": [3, 2, 1, 0], "keep_free": 0, "priority": "below_normal"}
        claims = [{"cores": [0], "background": True, "state": "running"}]
        assert admits(config, FREE, claims, [ticket("head", 3, None), ticket("t", 1)], "t") is None
        # after the ordinary claim ends, the head takes 5 to 1 alone but would need core 0 beside the
        # backfill; a four-core head fits on 5 to 2 either way
        config6 = {"cores": [5, 4, 3, 2, 1, 0], "keep_free": 0, "priority": "below_normal"}
        claims = [{"cores": [0], "background": True}, {"cores": [5, 4, 3, 2]}]
        assert admits(config6, FREE, claims, [ticket("head", 5, None), ticket("t", 1)], "t") is None
        assert admits(config6, FREE, claims, [ticket("head", 4, None), ticket("t", 1)], "t") == [1]


class BackfillAdmissionTests(unittest.TestCase):
    setUp = HostSlotsTests.setUp

    @needs_three
    def test_run_records_the_backfill_and_the_head_it_jumped(self):
        own = "a" * 32
        head = {"id": "b" * 32, "cores_wanted": len(CPUS), "background": False,
                "config_sha256": slots.config_digest(slots.load_config())}
        tickets = [head, {"id": own, "cores_wanted": 1, "background": False, "max_seconds": 30.0}]
        holder = {"id": "c" * 32, "cores": CPUS[1:], "background": False}
        seen, announced = [], []
        with patch.object(slots, "live_tickets", return_value=tickets), \
                patch.object(slots, "live_claims", return_value=[holder]), \
                patch.object(slots, "_run_claimed", side_effect=lambda claim, *a, **k: seen.append(
                    slots.read_json(claim)) or 0):
            # the head wants every CPU: with the backfill still running it could never start, so no backfill
            with self.assertRaises(slots.NoRoom):
                slots.run("probe", "small", 1, ["unused"], claim_id=own, max_seconds=30.0)
            head["cores_wanted"] = len(CPUS) - 1
            self.assertEqual(slots.run("probe", "small", 1, ["unused"], claim_id=own, max_seconds=30.0,
                                       on_claim=announced.append), 0)
        record = seen[0]
        self.assertTrue(record["backfill"])
        self.assertEqual((record["head_id"], record["max_seconds"], record["cores"]), ("b" * 32, 30.0, [CPUS[0]]))
        self.assertTrue(announced[0]["backfill"])
        self.assertFalse(list((self.host / "TESTHOST.slots").glob("claim-*.json")))

    def test_an_invalid_limit_is_refused_before_queueing_or_launching(self):
        for bad in (0, -1, float("nan"), slots.BACKFILL_MAX_SECONDS * 2):
            with self.assertRaises(ValueError):
                slots.run("probe", "bad", 1, ["unused"], max_seconds=bad)
        self.assertFalse(list((self.host / "TESTHOST.slots").glob("queue-*.json")))
        log = self.tmp_path / "submit.log"
        out = subprocess.run([sys.executable, str(TOOLS / "host_slots_v1.py"), "submit", "--log", str(log),
                              "--max-seconds", "0", "--lane", "l", "--work-id", "w", "--cores", "1", "--",
                              sys.executable, "-c", "pass"], capture_output=True, text=True, timeout=60)
        self.assertEqual(out.returncode, slots.EXIT_USAGE, out.stderr)
        self.assertFalse(log.exists())

    def test_ordinary_claim_records_are_unchanged(self):
        seen = []
        with patch.object(slots, "_run_claimed", side_effect=lambda claim, *a, **k: seen.append(
                slots.read_json(claim)) or 0):
            slots.run("probe", "plain", 1, ["unused"], claim_id="d" * 32)
        self.assertEqual(set(seen[0]), {"schema", "id", "lane", "work_id", "pid", "creation_time", "background",
                                        "cores", "priority", "state", "acquired_at"})

    def test_status_flags_an_overdue_backfill_and_keeps_it(self):
        pid, creation = slots.self_identity()
        base = {"schema": slots.SCHEMA, "pid": pid, "creation_time": creation, "lane": "l", "work_id": "w",
                "background": False, "priority": "below_normal", "state": "running", "acquired_at": slots.now_utc()}
        slots.write_json(self.host / f"TESTHOST.slots/claim-{'e' * 32}.json", {
            **base, "id": "e" * 32, "cores": [CPUS[-1]], "backfill": True, "max_seconds": 1.0,
            "head_id": "f" * 32, "deadline_at": "2026-01-01T00:00:00.000+00:00"})
        slots.write_json(self.host / f"TESTHOST.slots/claim-{'1' * 32}.json", {
            **base, "id": "1" * 32, "cores": [CPUS[0]]})  # a record from before backfill: not a backfill
        status = slots.status()
        self.assertEqual([(b["id"], b["overdue"], b["head_id"]) for b in status["backfill"]],
                         [("e" * 32, True, "f" * 32)])
        self.assertEqual(len(status["claims"]), 2)  # flagged, never removed or stopped


class BackfillProcessTests(unittest.TestCase):
    setUp = HostSlotsTests.setUp

    @needs_three
    def test_small_job_runs_while_the_head_waits_and_the_head_still_starts(self):
        host, tmp_path = self.host, self.tmp_path
        hold = script(tmp_path, "bf-hold.py", "Path = __import__('pathlib').Path\nPath(sys.argv[1]).write_text('up')\n"
                                              "while not Path(sys.argv[2]).exists(): time.sleep(0.05)\n")
        up, release = tmp_path / "up", tmp_path / "release"
        small_up, small_release = tmp_path / "small-up", tmp_path / "small-release"
        holder = cli("run", "--lane", "l", "--work-id", "holder", "--cores", str(len(CPUS) - 1), "--",
                     sys.executable, hold, str(up), str(release))
        waiters = []
        try:
            assert wait_for(up.exists)
            head = cli("run", "--wait", "--lane", "l", "--work-id", "head", "--cores", str(len(CPUS) - 1),
                       "--", sys.executable, "-c", "pass")
            waiters.append(head)
            assert wait_for(lambda: len(list((host / "TESTHOST.slots").glob("queue-*.json"))) == 1)
            plain = cli("run", "--wait", "--lane", "l", "--work-id", "plain", "--cores", "1", "--",
                        sys.executable, "-c", "pass")
            waiters.append(plain)
            assert wait_for(lambda: len(list((host / "TESTHOST.slots").glob("queue-*.json"))) == 2)
            small = cli("run", "--wait", "--max-seconds", "120", "--lane", "l", "--work-id", "small", "--cores", "1",
                        "--", sys.executable, hold, str(small_up), str(small_release))
            waiters.append(small)
            assert wait_for(small_up.exists)
            self.assertTrue(json.loads(small.stdout.readline())["backfill"])
            status = slots.status()
            self.assertEqual([b["work_id"] for b in status["backfill"]], ["small"])
            # the request without a declared limit is still queued behind the head
            self.assertEqual([t["work_id"] for t in status["queue"]], ["head", "plain"])
            # the holder ends while the backfill still runs: the head must start at once
            release.write_text("go")
            self.assertEqual(head.wait(timeout=60), 0)
            self.assertIsNone(small.poll())
            small_release.write_text("go")
            for waiter in waiters:
                self.assertEqual(waiter.wait(timeout=60), 0)
        finally:
            release.write_text("go")
            small_release.write_text("go")
            holder.wait(timeout=60)
            for waiter in waiters:
                if waiter.poll() is None:
                    waiter.kill()
                    waiter.wait(timeout=30)

if __name__ == "__main__":
    unittest.main()
