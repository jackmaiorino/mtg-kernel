"""Tests for host_reservation_v1 (contract item 9 of the 2026-09-28 01:35 goal
amendment): concurrent acquire, owner-only release, reclaim on positive
evidence only, unknown records, nested sharing, a supervisor that refuses a
token that no longer holds, and the WMI lifecycle with real children (a
supervised success; an A to B to C chain with B gone, released only after C
ends and taken down whole with an abandoned supervisor; nested dispatch that
carries the token and holds the outer lock, also after a lost response with
the parent gone; a late nested supervisor refused; a lost creation response;
two competing reclaimers). Every test uses its own temporary root. On Linux
the same tests run against the Linux backend (a detached session stands in
for WMI), plus tests of its own identity, locking and containment."""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
import uuid
from unittest.mock import patch
from datetime import datetime
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

LINUX = sys.platform.startswith("linux")
SUPPORTED = os.name == "nt" or LINUX
if SUPPORTED:
    import host_reservation_v1 as hr

PRELUDE = (
    "import json, os, sys, time\n"
    f"sys.path.insert(0, {str(TOOLS)!r})\n"
    "import host_reservation_v1 as hr\n"
)
SLEEP = "import time; time.sleep({})"
# WMI cannot start an app-execution alias (the Microsoft Store Python).
WMI_STARTABLE = "windowsapps" not in os.path.normcase(sys.executable)
NO_WMI = "WMI cannot start this interpreter (an app-execution alias)"


def wait_until(predicate, timeout, step=0.2):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(step)
    return predicate()


@unittest.skipUnless(SUPPORTED, "the host reservation supports Windows and Linux")
class HostReservationTests(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="host-reservation-test-")
        self.saved = {k: os.environ.get(k) for k in (hr.TEST_ROOT_ENV, hr.TOKEN_ENV)}
        os.environ[hr.TEST_ROOT_ENV] = self.root
        os.environ.pop(hr.TOKEN_ENV, None)
        self.children = []

    def tearDown(self):
        # End every process a test started or a reservation recorded.
        for proc in self.children:
            if proc.poll() is None:
                proc.kill()
                proc.wait(10)
            for stream in (proc.stdout, proc.stderr):
                if stream is not None:
                    stream.close()
        for token_dir in Path(self.root).glob(f"{hr.HOST}.events/*"):
            events, _ = hr.read_events(token_dir.name)
            roots = [(e["pid"], e["creation_time"]) for e in events
                     if e.get("kind") in ("adopt", "handoff", "descendant") and e.get("creation_time")]
            for d in hr.live_descendants(roots):
                if d["creation_time"]:
                    hr.terminate(d["pid"], d["creation_time"])
            for pid, creation in roots:
                hr.terminate(pid, creation)
        for key, value in self.saved.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
        time.sleep(0.2)
        shutil.rmtree(self.root, ignore_errors=True)

    def spawn(self, code, **extra_env):
        env = dict(os.environ, **{hr.TEST_ROOT_ENV: self.root})
        env.pop(hr.TOKEN_ENV, None)
        env.update(extra_env)
        proc = subprocess.Popen([sys.executable, "-c", code], env=env, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, text=True)
        self.children.append(proc)
        return proc

    @staticmethod
    def last_json(proc, timeout=90):
        out, err = proc.communicate(timeout=timeout)
        lines = out.strip().splitlines()
        if not lines:
            raise AssertionError(f"no output; stderr: {err}")
        return json.loads(lines[-1])

    def events(self, token):
        return hr.read_events(token)[0]

    def try_reclaim(self):
        try:
            return hr.reclaim("test", "recovery", "released by the test")
        except hr.Refused:
            return None

    def record(self, **fields):
        pid, creation = hr.self_identity()
        record = {
            "schema": hr.SCHEMA, "token": uuid.uuid4().hex, "generation": 1, "lane": "test", "host": hr.HOST,
            "work_id": "crafted", "owner_pid": pid, "owner_process_creation_time": creation,
            "boot_id": hr.boot_id(), "acquired_at": "2020-01-01T00:00:00.000+00:00", "transport_record": {},
            "release_condition": "crafted by the test",
        }
        record.update(fields)
        return record

    # ------------------------------------------------------------ exclusion

    def test_concurrent_acquire_exactly_one_wins(self):
        for round_ in range(8):
            go = Path(self.root) / f"go-{round_}"
            code = PRELUDE + (
                f"go = {str(go)!r}\n"
                "while not os.path.exists(go):\n"
                "    time.sleep(0.001)\n"
                "try:\n"
                "    r = hr.acquire('test', 'race', 'released by the test')\n"
                "    print(json.dumps({'won': r['token']}))\n"
                "except hr.Held as e:\n"
                "    print(json.dumps({'held': (e.holder or {}).get('token')}))\n"
            )
            procs = [self.spawn(code) for _ in range(2)]
            time.sleep(1.0)
            go.write_text("go")
            outs = [self.last_json(p) for p in procs]
            winners = [o["won"] for o in outs if "won" in o]
            self.assertEqual(len(winners), 1, outs)
            self.assertEqual([o["held"] for o in outs if "held" in o], winners, "the loser prints the holder")
            hr.release(winners[0], "cancelled", "end of round")
        self.assertEqual(hr.status()["state"], "free")

    def test_release_is_owner_only_and_waits_for_live_work(self):
        r = hr.acquire("test", "owner", "released by the test")
        with self.assertRaises(hr.Refused):
            hr.release(uuid.uuid4().hex, "success")
        self.assertEqual(hr.status()["record"]["token"], r["token"])
        worker = self.spawn(SLEEP.format(30))
        time.sleep(0.3)
        hr.record_descendant(r["token"], worker.pid)
        with self.assertRaises(hr.Refused):
            hr.release(r["token"], "cancelled")
        worker.kill()
        worker.wait(10)
        self.assertEqual(hr.release(r["token"], "cancelled")["outcome"], "cancelled")
        self.assertEqual(hr.status(r["token"])["token_fate"], "released")

    def test_reclaim_needs_positive_evidence(self):
        owner = self.spawn(SLEEP.format(30))
        time.sleep(0.3)
        old = hr.acquire("test", "abandoned", "never released", owner_pid=owner.pid)
        with self.assertRaises(hr.Refused):
            hr.reclaim("test", "recovery", "must be refused")
        owner.kill()
        owner.wait(10)
        new = hr.reclaim("test", "recovery", "released by the test")
        self.assertGreater(new["generation"], old["generation"])
        self.assertEqual(new["reclaimed"]["token"], old["token"])
        self.assertIn("absent by pid and creation time", new["reclaimed"]["evidence"])
        self.assertEqual(len(list(Path(self.root).glob(f"{hr.HOST}.g*.{old['token']}.reclaimed-*.json"))), 1)
        self.assertEqual(self.events(old["token"])[-1]["kind"], "reclaim")
        hr.release(new["token"], "cancelled")

    def test_unreadable_incomplete_or_old_records_are_never_dead(self):
        lock = hr.lock_path()
        lock.write_bytes(b"{not json")
        self.assertEqual(hr.status()["state"], "unknown")
        with self.assertRaises(hr.Refused):
            hr.reclaim("test", "recovery", "must be refused")
        incomplete = self.record()
        del incomplete["owner_process_creation_time"]
        lock.write_bytes(json.dumps(incomplete).encode())
        self.assertEqual(hr.status()["state"], "unknown")
        with self.assertRaises(hr.Refused):
            hr.reclaim("test", "recovery", "must be refused")
        # Six years old with a live owner: age alone never reclaims.
        lock.write_bytes(json.dumps(self.record()).encode())
        self.assertEqual(hr.status()["state"], "held")
        with self.assertRaises(hr.Refused):
            hr.reclaim("test", "recovery", "must be refused")
        # A changed boot id is positive evidence.
        other = "bootid:1" if hr.boot_id() != "bootid:1" else "bootid:2"
        lock.write_bytes(json.dumps(self.record(boot_id=other)).encode())
        new = hr.reclaim("test", "after-reboot", "released by the test")
        self.assertIn("boot id changed", new["reclaimed"]["evidence"])
        hr.release(new["token"], "cancelled")

    def test_nested_wrappers_share_the_token(self):
        r = hr.acquire("test", "outer", "released by the test")
        inner = PRELUDE + "r = hr.acquire('test', 'inner', 'nested')\nprint(json.dumps({'token': r['token'], 'shared': r.get('shared')}))\n"
        self.assertEqual(self.last_json(self.spawn(inner, **{hr.TOKEN_ENV: r["token"]})),
                         {"token": r["token"], "shared": True})
        stale = PRELUDE + "try:\n    hr.acquire('test', 'inner', 'nested')\n    print(json.dumps('acquired'))\nexcept hr.Held:\n    print(json.dumps('held'))\n"
        self.assertEqual(self.last_json(self.spawn(stale, **{hr.TOKEN_ENV: uuid.uuid4().hex})), "held")
        hr.release(r["token"], "cancelled")

    def test_supervisor_refuses_a_token_that_no_longer_holds(self):
        r = hr.acquire("test", "late", "released by the test")
        hr.release(r["token"], "cancelled")
        marker = Path(self.root) / "work-ran"
        code = hr.supervise(r["token"], [sys.executable, "-c", f"open({str(marker)!r}, 'w').close()"])
        self.assertEqual(code, hr.EXIT_REFUSED)
        self.assertFalse(marker.exists())

    @unittest.skipUnless(os.name == "nt", "app-execution aliases exist only on Windows")
    def test_app_execution_alias_is_refused_before_acquiring(self):
        alias = "C:/Users/someone/AppData/Local/Microsoft/WindowsApps/python.exe"
        with self.assertRaises(ValueError):
            hr.supervisor_command(uuid.uuid4().hex, [sys.executable, "-c", "pass"], python=alias)
        with self.assertRaises(ValueError):
            hr.dispatch("test", "alias", "never acquired", [sys.executable, "-c", "pass"], cwd=self.root,
                        python=alias)
        self.assertEqual(hr.status()["state"], "free")

    def test_supervisor_uses_base_interpreter_without_changing_work_command(self):
        redirector = "C:/test/venv/Scripts/python.exe"
        base = "C:/test/base/python.exe"
        work = [redirector, "-c", "pass"]
        with patch.object(sys, "executable", redirector), patch.object(sys, "_base_executable", base):
            self.assertEqual(hr.supervisor_python(), os.path.abspath(base))
            self.assertEqual(hr.supervisor_python(redirector), os.path.abspath(base))
            line = hr.supervisor_command(uuid.uuid4().hex, work)
            self.assertTrue(line.startswith(hr.command_line([os.path.abspath(base)])))
            self.assertTrue(line.endswith(hr.command_line(work)))
            explicit = "C:/test/other/python.exe"
            self.assertEqual(hr.supervisor_python(explicit), os.path.abspath(explicit))

    def test_supervisor_interpreter_falls_back_without_base_executable(self):
        with patch.object(sys, "_base_executable", None):
            self.assertEqual(hr.supervisor_python(), os.path.abspath(sys.executable))

    # ------------------------------------------------------------ WMI lifecycle
    # (on Linux, spawn_detached creates the supervisor in place of WMI)

    @unittest.skipUnless(WMI_STARTABLE, NO_WMI)
    def test_wmi_supervisor_holds_until_its_child_ends_then_releases(self):
        result = hr.dispatch("test", "wmi-success", "the supervisor releases at exit",
                             [sys.executable, "-c", SLEEP.format(4)], cwd=self.root,
                             busy_pattern=r"^no-such-image-for-host-reservation-tests\.exe$")
        self.assertEqual(result["state"], "dispatched", result)
        token = result["token"]
        adopt = wait_until(lambda: [e for e in self.events(token) if e["kind"] == "adopt"], 30)
        self.assertTrue(adopt, self.events(token))
        self.assertEqual(adopt[0]["pid"], result["pid"])
        self.assertEqual(hr.status()["state"], "held")
        self.assertIn(result["pid"], hr.exempt_pids(token))
        with self.assertRaises(hr.Held):
            hr.acquire("test", "second", "must be refused")
        self.assertTrue(wait_until(lambda: hr.status()["state"] == "free", 60))
        last = hr.status(token)["token_events"][-1]
        self.assertEqual((last["kind"], last["outcome"]), ("release", "success"))

    def chain(self, seconds, *, release_file=None):
        """A spawns B and waits for it; B spawns C and exits at once; so A and
        B are gone while C lives. C waits for time or an explicit release."""
        pid_file, done_file = Path(self.root) / "c.pid", Path(self.root) / "c.done"
        c = ("import os, time\n"
             f"open({str(pid_file)!r}, 'w').write(str(os.getpid()))\n")
        if release_file is None:
            c += f"time.sleep({seconds})\n"
        else:
            c += (f"deadline = time.monotonic() + {seconds}\n"
                  f"while not os.path.exists({str(release_file)!r}):\n"
                  "    if time.monotonic() >= deadline:\n"
                  "        raise TimeoutError('test did not release C')\n"
                  "    time.sleep(0.05)\n")
        c += f"open({str(done_file)!r}, 'w').write('done')"
        b = f"import subprocess, sys; subprocess.Popen([sys.executable, '-c', {c!r}])"
        a = f"import subprocess, sys; subprocess.Popen([sys.executable, '-c', {b!r}]).wait()"
        return [sys.executable, "-c", a], pid_file, done_file

    @unittest.skipUnless(WMI_STARTABLE, NO_WMI)
    def test_wmi_chain_with_a_gone_intermediate_holds_until_the_last_process_ends(self):
        # Keep C alive through WMI inspection, even on a busy runner. The
        # reservation must stay held until this test explicitly ends C.
        release_file = Path(self.root) / "c.release"
        command, pid_file, done_file = self.chain(120, release_file=release_file)
        result = hr.dispatch("test", "wmi-chain", "released after the last process ends", command, cwd=self.root)
        token = result["token"]
        self.assertTrue(wait_until(pid_file.exists, 30))
        c_pid = int(pid_file.read_text())
        self.assertTrue(wait_until(lambda: any(e["kind"] == "members" for e in self.events(token)), 30))
        members = [m["pid"] for e in self.events(token) if e["kind"] == "members" for m in e["members"]]
        self.assertIn(c_pid, members, "C is listed although nothing recorded its parent")
        adopt = [e for e in self.events(token) if e["kind"] == "adopt"][0]
        self.assertTrue(adopt["contained"])
        self.assertEqual(hr.status()["state"], "held")
        self.assertFalse(done_file.exists())
        release_file.write_text("release")
        self.assertTrue(wait_until(lambda: hr.status()["state"] == "free", 60))
        last = hr.status(token)["token_events"][-1]
        self.assertEqual((last["kind"], last["outcome"]), ("release", "success"))
        released_at = datetime.fromisoformat(last["at"]).timestamp()
        self.assertGreaterEqual(released_at, done_file.stat().st_mtime - 0.01, "released only after C ended")

    @unittest.skipUnless(WMI_STARTABLE, NO_WMI)
    def test_wmi_abandoned_supervisor_takes_its_whole_chain_down(self):
        command, pid_file, done_file = self.chain(30)
        result = hr.dispatch("test", "wmi-chain-abandoned", "reclaimed by the test", command, cwd=self.root)
        token = result["token"]
        self.assertTrue(wait_until(pid_file.exists, 30))
        c_pid = int(pid_file.read_text())
        c_creation = hr.creation_time(c_pid)
        self.assertEqual(hr.process_state(c_pid, c_creation), "alive")
        adopt = wait_until(lambda: [e for e in self.events(token) if e["kind"] == "adopt"], 30)[0]
        with self.assertRaises(hr.Refused):
            hr.reclaim("test", "recovery", "must be refused")
        self.assertTrue(hr.terminate(adopt["pid"], adopt["creation_time"]))
        # The job dies with its last handle: C ends although no event names it
        # (the supervisor may not have listed members yet).
        self.assertTrue(wait_until(lambda: hr.process_state(c_pid, c_creation) == "absent", 10))
        new = wait_until(self.try_reclaim, 20, step=0.5)
        self.assertTrue(new, hr.status())
        self.assertEqual(new["reclaimed"]["token"], token)
        self.assertFalse(done_file.exists())
        hr.release(new["token"], "cancelled")

    def nested_outer(self, lossy: bool):
        """Outer work that dispatches nested work and then exits. With lossy,
        the creation response is lost and the parent exits once the nested
        supervisor has adopted (so no handoff ever names it)."""
        token_file, done_file = Path(self.root) / "nested.token", Path(self.root) / "nested.done"
        result_file = Path(self.root) / "nested.result"
        nested_work = (f"import os, time; open({str(token_file)!r}, 'w').write(os.environ.get({hr.TOKEN_ENV!r}, '')); "
                       f"time.sleep(5); open({str(done_file)!r}, 'w').write('done')")
        outer = PRELUDE + (
            "def lossy(line, cwd):\n"
            "    hr.create_process(line, cwd)\n"
            "    raise TimeoutError('creation response lost')\n"
            f"r = hr.dispatch('test', 'nested', 'held by the outer supervisor', [sys.executable, '-c', {nested_work!r}], "
            f"cwd={self.root!r}, create={'lossy' if lossy else 'None'})\n"
            f"open({str(result_file)!r}, 'w').write(json.dumps(r))\n"
            "if r['state'] == 'spawn-unconfirmed':\n"
            "    token = os.environ[hr.TOKEN_ENV]\n"
            "    while not any(e['kind'] == 'adopt' and e.get('nested') for e in hr.read_events(token)[0]):\n"
            "        time.sleep(0.1)\n"
        )
        return [sys.executable, "-c", outer], token_file, done_file, result_file

    def check_nested(self, lossy: bool):
        command, token_file, done_file, result_file = self.nested_outer(lossy)
        result = hr.dispatch("test", "outer", "released after the nested work ends", command, cwd=self.root)
        token = result["token"]
        self.assertTrue(wait_until(result_file.exists, 60))
        nested = json.loads(result_file.read_text())
        self.assertEqual((nested["nested"], nested["token"]), (True, token))
        self.assertEqual(nested["state"], "spawn-unconfirmed" if lossy else "dispatched")
        adopts = wait_until(lambda: [e for e in self.events(token) if e["kind"] == "adopt" and e.get("nested")], 30)
        self.assertTrue(adopts and adopts[0]["contained"], self.events(token))
        self.assertTrue(wait_until(token_file.exists, 30))
        self.assertEqual(token_file.read_text(), token, "the nested work sees the shared token")
        handed = [e.get("pid") for e in self.events(token) if e["kind"] == "handoff"]
        if lossy:
            self.assertNotIn(adopts[0]["pid"], handed, "no handoff names the nested supervisor")
        else:
            self.assertIn(adopts[0]["pid"], handed)
        # The outer work has ended; the nested supervisor holds the outer lock.
        self.assertEqual(hr.status()["state"], "held")
        self.assertFalse(done_file.exists())
        self.assertTrue(wait_until(lambda: hr.status()["state"] == "free", 60))
        self.assertTrue(done_file.exists())
        kinds = [e["kind"] for e in hr.status(token)["token_events"]]
        self.assertLess(kinds.index("nested-done"), kinds.index("release"))

    @unittest.skipUnless(WMI_STARTABLE, NO_WMI)
    def test_wmi_nested_dispatch_carries_the_token_and_holds_the_outer_lock(self):
        self.check_nested(lossy=False)

    @unittest.skipUnless(WMI_STARTABLE, NO_WMI)
    def test_wmi_nested_lost_response_with_the_parent_gone_still_holds(self):
        self.check_nested(lossy=True)

    @unittest.skipUnless(WMI_STARTABLE, NO_WMI)
    def test_wmi_late_nested_supervisor_refuses_to_run(self):
        r = hr.acquire("test", "outer", "released by the test")
        hr.release(r["token"], "cancelled")
        marker = Path(self.root) / "late.ran"
        line = hr.supervisor_command(r["token"], [sys.executable, "-c", f"open({str(marker)!r}, 'w').close()"], nested=True)
        created = hr.create_process(line, self.root)
        self.assertEqual(created["return_value"], 0)
        creation = hr.creation_time(created["pid"])
        if creation is not None:
            self.assertTrue(wait_until(lambda: hr.process_state(created["pid"], creation) == "absent", 30))
        else:
            time.sleep(2)
        self.assertFalse(marker.exists())
        self.assertNotIn("adopt", [e["kind"] for e in self.events(r["token"])])

    @unittest.skipUnless(WMI_STARTABLE, NO_WMI)
    def test_wmi_lost_creation_response_is_never_redispatched(self):
        def lossy(line, cwd):
            hr.create_process(line, cwd)  # the process is created ...
            raise TimeoutError("creation response lost")  # ... but the caller never hears
        result = hr.dispatch("test", "wmi-lost", "the supervisor releases at exit",
                             [sys.executable, "-c", SLEEP.format(4)], cwd=self.root, create=lossy)
        self.assertEqual(result["state"], "spawn-unconfirmed")
        token = result["token"]
        self.assertIn("spawn-unconfirmed", [e["kind"] for e in self.events(token)])
        with self.assertRaises(hr.Held):
            hr.dispatch("test", "wmi-lost-retry", "must be refused", [sys.executable, "-c", "pass"], cwd=self.root)
        # The supervisor names itself, so the work is visible, then terminal.
        self.assertTrue(wait_until(lambda: any(e["kind"] == "adopt" for e in self.events(token)), 30))
        self.assertTrue(wait_until(lambda: hr.status(token)["token_fate"] == "released", 60))
        # No process at all: the caller keeps the lock (it lives) and may cancel it.
        def nothing(line, cwd):
            raise TimeoutError("no response")
        result = hr.dispatch("test", "wmi-none", "cancelled by the test", [sys.executable, "-c", "pass"],
                             cwd=self.root, create=nothing)
        self.assertEqual(result["state"], "spawn-unconfirmed")
        with self.assertRaises(hr.Refused):
            hr.reclaim("test", "recovery", "must be refused")
        hr.release(result["token"], "cancelled", "no process was created")

    def test_two_competing_reclaimers_one_wins(self):
        owner = self.spawn(SLEEP.format(30))
        time.sleep(0.3)
        old = hr.acquire("test", "abandoned", "never released", owner_pid=owner.pid)
        owner.kill()
        owner.wait(10)
        go = Path(self.root) / "go"
        code = PRELUDE + (
            f"go = {str(go)!r}\n"
            "while not os.path.exists(go):\n"
            "    time.sleep(0.001)\n"
            "try:\n"
            "    r = hr.reclaim('test', 'recovery', 'released by the test')\n"
            "    print(json.dumps({'won': r['token']}), flush=True)\n"
            "    time.sleep(6)\n"  # the new owner stays alive, as a real reclaimer does
            "except (hr.Refused, hr.Held) as e:\n"
            "    print(json.dumps({'refused': str(e)}), flush=True)\n"
        )
        procs = [self.spawn(code) for _ in range(2)]
        time.sleep(1.0)
        go.write_text("go")
        outs = [self.last_json(p) for p in procs]
        winners = [o["won"] for o in outs if "won" in o]
        self.assertEqual(len(winners), 1, outs)
        self.assertEqual(len(list(Path(self.root).glob(f"{hr.HOST}.g*.{old['token']}.reclaimed-*.json"))), 1)
        self.assertEqual(hr.status()["record"]["token"], winners[0])
        hr.release(winners[0], "cancelled")

    # ------------------------------------------------------------ Linux backend

    def work_with_children(self, info_file):
        """Work that starts three sleepers (in its group, in another group of its
        session, and in a session of its own), records the ids, then sleeps."""
        sleeper = f"[sys.executable, '-c', {SLEEP.format(120)!r}]"
        code = (
            "import json, os, subprocess, sys, time\n"
            f"same = subprocess.Popen({sleeper})\n"
            f"moved = subprocess.Popen({sleeper}, process_group=0)\n"
            f"escaped = subprocess.Popen({sleeper}, start_new_session=True)\n"
            "info = {'pid': os.getpid(), 'sid': os.getsid(0), 'pgid': os.getpgrp(),\n"
            "        'children': {'same': same.pid, 'moved': moved.pid, 'escaped': escaped.pid}}\n"
            f"open({str(info_file)!r} + '.tmp', 'w').write(json.dumps(info))\n"
            f"os.rename({str(info_file)!r} + '.tmp', {str(info_file)!r})\n"
            "time.sleep(120)\n"
        )
        return [sys.executable, "-c", code]

    def read_work(self, info_file):
        """The work's ids; its processes are killed after the test even if it fails
        (an escaped sleeper is outside every session tearDown can see)."""
        info = json.loads(info_file.read_text())
        named = {p: hr.creation_time(p) for p in [info["pid"], *info["children"].values()]}

        def kill_survivors():
            for pid, born in named.items():
                if born is not None and hr.process_state(pid, born) == "alive":
                    os.kill(pid, 9)
        self.addCleanup(kill_survivors)
        return info

    @staticmethod
    def session_and_group(pid):
        row = hr._stat(pid)
        return row.session, row.pgrp

    @unittest.skipUnless(LINUX, "Linux backend")
    def test_linux_second_acquire_fails_while_held_and_never_replaces_the_lock(self):
        first = hr.acquire("test", "first", "released by the test")
        before = hr.lock_path().read_bytes()
        with self.assertRaises(hr.Held) as caught:
            hr.acquire("test", "second", "must be refused")
        self.assertEqual(caught.exception.holder["token"], first["token"])
        cli = subprocess.run([sys.executable, str(TOOLS / "host_reservation_v1.py"), "acquire", "--lane", "test",
                              "--work-id", "cli", "--release-condition", "must be refused"],
                             env=dict(os.environ), capture_output=True, text=True, timeout=60)
        self.assertEqual(cli.returncode, hr.EXIT_HELD, cli.stderr)
        self.assertEqual(json.loads(cli.stdout)["holder"]["token"], first["token"])
        self.assertEqual(hr.lock_path().read_bytes(), before, "a refused acquire never replaces the lock")
        self.assertEqual(list(Path(self.root).glob(".*.tmp")), [], "the loser removes its temp file")
        # POSIX rename would silently replace the lock; publishing refuses.
        other = Path(self.root) / "other.tmp"
        other.write_bytes(b"other")
        with self.assertRaises(FileExistsError):
            hr._publish(other, hr.lock_path())
        self.assertEqual(hr.lock_path().read_bytes(), before)
        hr.release(first["token"], "cancelled")
        second = hr.acquire("test", "second", "released by the test")
        self.assertEqual(second["generation"], first["generation"] + 1)
        hr.release(second["token"], "cancelled")

    @unittest.skipUnless(LINUX, "Linux backend")
    def test_linux_dead_exited_or_restarted_holder_is_stale_by_pid_and_start_time(self):
        pid, start = hr.self_identity()
        stat = Path(f"/proc/{pid}/stat").read_text()
        self.assertEqual(start, int(stat[stat.rindex(")") + 2:].split()[19]), "field 22, start time")
        self.assertEqual(hr.process_state(pid, start), "alive")
        dead = self.spawn(SLEEP.format(30))
        dead_start = wait_until(lambda: hr.creation_time(dead.pid), 10)
        dead.kill()
        dead.wait(10)  # reaped: the pid is gone (or names a newer process)
        exited = self.spawn("pass")
        exited_start = hr.creation_time(exited.pid)
        self.assertTrue(wait_until(lambda: hr._stat(exited.pid).state == "Z", 10), "exited, not yet reaped")
        self.assertEqual(hr.creation_time(exited.pid), exited_start, "an unreaped process keeps its start time")
        stale = {"dead": (dead.pid, dead_start), "exited": (exited.pid, exited_start), "restarted": (pid, start + 1)}
        for label, (owner_pid, owner_start) in stale.items():
            self.assertEqual(hr.process_state(owner_pid, owner_start), "absent", label)
            crafted = self.record(owner_pid=owner_pid, owner_process_creation_time=owner_start)
            hr.lock_path().write_bytes(json.dumps(crafted).encode())
            self.assertEqual(hr.status()["state"], "reclaimable", label)
            new = hr.reclaim("test", f"after-{label}", "released by the test")
            self.assertEqual(new["reclaimed"]["token"], crafted["token"])
            self.assertIn("absent by pid and creation time", new["reclaimed"]["evidence"])
            hr.release(new["token"], "cancelled")
        hr.lock_path().write_bytes(json.dumps(self.record()).encode())
        self.assertEqual(hr.status()["state"], "held", "a live owner with its own start time holds")
        with self.assertRaises(hr.Refused):
            hr.reclaim("test", "recovery", "must be refused")

    @unittest.skipUnless(LINUX, "Linux backend")
    def test_linux_supervisor_runs_work_in_its_own_session_and_stop_kills_the_whole_group(self):
        info_file = Path(self.root) / "work.json"
        result = hr.dispatch("test", "linux-session", "reclaimed by the test", self.work_with_children(info_file),
                             cwd=self.root)
        self.assertEqual(result["state"], "dispatched", result)
        token = result["token"]
        self.assertTrue(wait_until(info_file.exists, 30))
        info, events = self.read_work(info_file), self.events(token)
        adopt = [e for e in events if e["kind"] == "adopt"][0]
        self.assertTrue(adopt["contained"])
        self.assertEqual([e["pid"] for e in events if e["kind"] == "descendant"], [info["pid"]])
        # The supervisor was created detached (its own session), the work in another.
        self.assertEqual(self.session_and_group(adopt["pid"]), (adopt["pid"], adopt["pid"]))
        self.assertNotEqual(adopt["pid"], os.getsid(0), "not the caller's session")
        self.assertEqual((info["sid"], info["pgid"]), (info["pid"], info["pid"]))
        kids = info["children"]
        self.assertEqual(self.session_and_group(kids["same"]), (info["pid"], info["pid"]))
        self.assertEqual(self.session_and_group(kids["moved"]), (info["pid"], kids["moved"]))
        self.assertEqual(self.session_and_group(kids["escaped"]), (kids["escaped"], kids["escaped"]))
        work = {"work": info["pid"], **kids}
        work = {name: (p, hr.creation_time(p)) for name, p in work.items()}
        self.assertEqual(hr.status()["state"], "held")
        # Stop the supervisor: it kills its whole job, also the process that left the session.
        self.assertTrue(hr.terminate(adopt["pid"], adopt["creation_time"]))
        for name, (p, born) in work.items():
            self.assertTrue(wait_until(lambda: hr.process_state(p, born) == "absent", 15), name)
        # Like a terminated Windows supervisor: no release, positive evidence to reclaim.
        self.assertEqual(hr.status()["state"], "reclaimable")
        new = hr.reclaim("test", "recovery", "released by the test")
        self.assertEqual(new["reclaimed"]["token"], token)
        hr.release(new["token"], "cancelled")

    @unittest.skipUnless(LINUX, "Linux backend")
    def test_linux_killed_supervisor_leaves_work_that_keeps_the_host_held(self):
        # Weaker than a job object: SIGKILL of the supervisor kills nothing, but
        # the surviving work session still holds the reservation.
        info_file = Path(self.root) / "work.json"
        result = hr.dispatch("test", "linux-sigkill", "reclaimed by the test", self.work_with_children(info_file),
                             cwd=self.root)
        token = result["token"]
        self.assertTrue(wait_until(info_file.exists, 30))
        info = self.read_work(info_file)
        adopt = [e for e in self.events(token) if e["kind"] == "adopt"][0]
        work = (info["pid"], hr.creation_time(info["pid"]))
        same = (info["children"]["same"], hr.creation_time(info["children"]["same"]))
        os.kill(adopt["pid"], 9)
        self.assertTrue(wait_until(lambda: hr.process_state(adopt["pid"], adopt["creation_time"]) == "absent", 10))
        self.assertEqual(hr.process_state(*work), "alive")
        self.assertEqual(hr.status()["state"], "held")
        os.kill(work[0], 9)
        self.assertTrue(wait_until(lambda: hr.process_state(*work) == "absent", 10))
        # Only the session now names the surviving sleeper: still held, never reclaimed.
        status = hr.status()
        self.assertEqual(status["state"], "held", status)
        self.assertIn(same[0], [d["pid"] for d in status["live_descendants"]])
        with self.assertRaises(hr.Refused):
            hr.reclaim("test", "recovery", "must be refused")
        for name in ("same", "moved", "escaped"):
            child = info["children"][name]
            hr.terminate(child, hr.creation_time(child))
        new = wait_until(self.try_reclaim, 20, step=0.5)
        self.assertTrue(new, hr.status())
        hr.release(new["token"], "cancelled")

    @unittest.skipUnless(LINUX, "Linux backend")
    def test_linux_boot_id_is_the_kernel_boot_id(self):
        raw = Path("/proc/sys/kernel/random/boot_id").read_text(encoding="ascii").strip()
        self.assertRegex(raw, r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
        self.assertEqual(hr.boot_id(), f"bootid:{raw}")
        r = hr.acquire("test", "boot", "released by the test")
        self.assertEqual(r["boot_id"], f"bootid:{raw}")
        hr.release(r["token"], "cancelled")
        with patch.object(hr, "BOOT_ID_PATH", str(Path(self.root) / "missing")):
            self.assertEqual(hr.boot_id(), "unavailable")
            # An unreadable boot id is never evidence of a reboot.
            hr.lock_path().write_bytes(json.dumps(self.record(boot_id="bootid:another-boot")).encode())
            self.assertEqual(hr.status()["state"], "held")

    @unittest.skipUnless(LINUX, "Linux backend")
    def test_linux_busy_scan_matches_image_names_from_proc(self):
        worker = self.spawn(SLEEP.format(30))
        image = os.path.basename(os.path.realpath(sys.executable))
        pattern = f"^{re.escape(image)}$"
        self.assertTrue(wait_until(lambda: worker.pid in [r["ProcessId"] for r in hr.busy_processes(pattern, [])], 10))
        self.assertIn({"ProcessId": worker.pid, "Name": image}, hr.busy_processes(pattern.upper(), []),
                      "case-insensitive, like PowerShell -match")
        self.assertNotIn(worker.pid, [r["ProcessId"] for r in hr.busy_processes(pattern, [worker.pid])])

    @unittest.skipUnless(LINUX, "Linux backend")
    def test_linux_lock_root_default_and_override(self):
        with patch.dict(os.environ):
            os.environ.pop(hr.TEST_ROOT_ENV)
            os.environ.pop(hr.ROOT_ENV, None)
            self.assertEqual(hr.root_dir(), Path("/var/lib/mtg-node/host-lock"))
            os.environ[hr.ROOT_ENV] = self.root
            self.assertEqual(hr.lock_path(), Path(self.root) / f"{hr.HOST}.lock")
            os.environ[hr.TEST_ROOT_ENV] = str(Path(self.root) / "test")
            self.assertEqual(hr.root_dir(), Path(self.root) / "test", "the test root still wins")


@unittest.skipUnless(os.name == "nt", "Windows backend")
class WindowsProcessListTests(unittest.TestCase):
    def test_system_process_list_has_this_process_with_its_creation_time(self):
        pid, creation = hr.self_identity()
        self.assertEqual(hr.system_creation_times()[pid], creation)

    def test_a_refused_pid_is_judged_by_the_system_process_list(self):
        pid, creation = hr.self_identity()
        with patch.object(hr, "_OpenProcess", return_value=None), \
                patch.object(hr.ctypes, "get_last_error", return_value=hr.ERROR_ACCESS_DENIED):
            self.assertEqual(hr.process_state(pid, creation), "alive")
            self.assertEqual(hr.process_state(pid, creation + 1), "absent", "pid reused by another process")
            self.assertEqual(hr.creation_time(pid), creation)
            with patch.object(hr, "system_creation_times", return_value={}):
                self.assertEqual(hr.process_state(pid, creation), "absent", "pid not listed")
            with patch.object(hr, "system_creation_times", return_value=None):
                self.assertEqual(hr.process_state(pid, creation), "unknown", "list unavailable")


if __name__ == "__main__":
    unittest.main()
