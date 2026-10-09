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


def lock(root, token=TOKEN):
    (root / "TESTHOST.lock").write_text(json.dumps({
        "schema": slots.LOCK_SCHEMA, "token": token, "lane": "board", "work_id": "run-1",
        "acquired_at": "2026-10-07T00:00:00Z"}), encoding="utf-8")


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
            for name in ("a", "b"):
                waiters.append(cli("run", "--wait", "--lane", "l", "--work-id", name, "--cores", "1", "--",
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


if __name__ == "__main__":
    unittest.main()
