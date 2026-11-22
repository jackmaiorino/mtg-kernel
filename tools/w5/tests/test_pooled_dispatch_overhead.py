"""The dispatcher's fixed per-update cost: connection reuse arguments pass through unchanged, requests per
shard (pin knowledge, one-call cleanup off the critical path, concurrent audits) and failures of a reused
connection. Bytes and the merged collection never change: every test compares them. Agent hosts run the
real agent behind a fake ssh (as test_pooled_agent does)."""
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE / "tests"))
import pooled_collect as pc  # noqa: E402
from test_pooled_agent import FAKE_SSH, TAG, AgentCase  # noqa: E402
from test_pooled_collect import FakeTransport, episode_bytes, fake_merge  # noqa: E402

MUX = ["-o", "BatchMode=yes", "-o", "ControlMaster=auto", "-o", "ControlPath=/srv/work/run/.ssh-mux/%C",
       "-o", "ControlPersist=120"]

# A fake ssh with connection reuse: the first call leaves a background "master" (its standard streams on
# /dev/null, as OpenSSH's ControlPersist master has) that outlives the call; a call fails as a dead master
# does (exit 255) while the host's DEAD file exists. argv: mux_ssh.py AGENT_CONFIG STATE_DIR REQUEST
MUX_SSH = textwrap.dedent('''
    import os, subprocess, sys
    config, state, request = sys.argv[1], sys.argv[2], sys.argv[3]
    with open(os.path.join(state, "calls"), "a") as log:
        log.write(request[:0] + "call\\n")
    if os.path.exists(os.path.join(state, "DEAD")):
        sys.stderr.write("mux_client_request_session: read from master failed: Broken pipe\\n")
        sys.exit(255)
    master = os.path.join(state, "master.pid")
    if not os.path.exists(master):
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"],
                                 stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        open(master, "w").write(str(child.pid))
    env = dict(os.environ, SSH_ORIGINAL_COMMAND=request)
    sys.exit(subprocess.run([sys.executable, "-I", os.environ["POOLED_AGENT_SCRIPT"], "agent", config],
                            env=env).returncode)
''')


class CountingAgentTransport(pc.AgentTransport):
    """The real agent transport, counting the operation of every request it sends."""

    def __init__(self, host, counts):
        super().__init__(host)
        self.counts = counts

    def _argv(self, op, **args):
        self.counts.setdefault(self.host.name, []).append(op)
        if op == "pin_status":
            self.counts.setdefault("asked", []).append(sorted(p[0] for p in args["pins"]))
        return super()._argv(op, **args)


class AgentLikeFake(pc.AgentTransport):
    """An agent host simulated in memory: like the agent, storing a shard command checks that every input
    it names is present with its pinned bytes, and refuses otherwise."""

    def __init__(self, host, counts):
        super().__init__(host)
        self.counts, self.files = counts, {}

    def _count(self, op):
        self.counts.setdefault(self.host.name, []).append(op)

    def pin_status(self, pins):
        self._count("pin_status")
        return {p: "absent" if p[0] not in self.files else "ok" if self.files[p[0]] == p[1] else "differs"
                for p in pins}

    def push(self, local, remote, digest):
        self._count("push")
        created = remote not in self.files
        self.files[remote] = digest
        return created

    def put_text(self, remote, text):
        self._count("put_shard")
        for path, digest in pc.pins_of(json.loads(text)):
            if self.files.get(path) != digest:
                raise RuntimeError(f"{self.host.name}: agent put_shard: refused: pinned input absent: {path!r}")
        self.files[remote] = text

    def run(self, argv, timeout):
        self._count("run_shard")
        return FakeTransport.run(types_namespace(files=self.files, runs=[], fail=False, salt=""), argv, timeout)

    def fetch_dir(self, remote, local):
        self._count("fetch_shard")
        shutil.copytree(remote, local)

    def remove_shard(self, out, command_path):
        self._count("remove_shard")
        shutil.rmtree(out, ignore_errors=True)
        self.files.pop(command_path, None)


def types_namespace(**fields):
    import types
    return types.SimpleNamespace(**fields)


class ConnectionReuseArgumentTests(unittest.TestCase):
    def test_the_hosts_file_ssh_arguments_reach_every_call_unchanged(self):
        root = Path(tempfile.mkdtemp(prefix="pooled-mux-args-"))
        try:
            entry = {"name": "n1", "kind": "ssh", "collector": "/c", "collector_sha256": "c" * 64, "workers": 4,
                     "weight": 1.0, "scratch": "/s", "arch": "x86", "ssh": ["ssh", *MUX, "mtg@node1"]}
            path = root / "hosts.json"
            path.write_text(json.dumps({"schema": pc.SCHEMA, "merge_binary": "/m",
                                        "hosts": [entry, dict(entry, name="n2", transport="agent")]}))
            shell, agent = pc.load_hosts(path)["hosts"]
            self.assertEqual(pc.Transport(shell)._ssh(["true"]), ["ssh", *MUX, "mtg@node1", "true"])
            argv = pc.AgentTransport(agent)._argv("pin_status", pins=[])
            self.assertEqual(argv[:-1], ["ssh", *MUX, "mtg@node1"])
            self.assertEqual(pc.decode_request(argv[-1]), ("pin_status", {"pins": []}))
        finally:
            shutil.rmtree(root, ignore_errors=True)


class OverheadCase(AgentCase):
    """Agent hosts over a fake ssh, each with its own scratch and agent config, sharing the pin root."""

    def host(self, name, arch="x86", ssh_script=FAKE_SSH, state=None):
        scratch = self.root / f"scratch-{name}"
        scratch.mkdir(exist_ok=True)
        config = self.write_config(self.root / f"agent-{name}.json", scratch=str(scratch))
        script = self.root / f"ssh-{name}.py"
        script.write_text(ssh_script)
        ssh = [sys.executable, str(script), str(config)] + ([str(state)] if state else [])
        return pc.Host(name=name, kind="ssh", collector=str(self.collector),
                       collector_sha256=pc.sha256_file(self.collector), workers=2, weight=1.0,
                       scratch=str(scratch), arch=arch, ssh=ssh, prefix=[sys.executable], transport="agent")

    def dispatcher(self, hosts, **config):
        self.counts = {}
        transports = {h.name: CountingAgentTransport(h, self.counts) for h in hosts}
        return pc.Dispatcher({"hosts": hosts, **config}, transports, timeout=100)

    def iteration(self, number, checkpoint=b"policy", episodes=6):
        pinned = self.pins / "run" / f"checkpoint-{number}.json"
        pinned.parent.mkdir(exist_ok=True)
        pinned.write_bytes(checkpoint + str(number).encode())
        self.episodes = [{"id": f"e{i}", "seed": 100 + i} for i in range(episodes)]
        command = {"mode": "collect_parallel", "workers": 2, "episodes": self.episodes,
                   "source": {"checkpoint": {"path": str(pinned), "sha256": pc.sha256_file(pinned)}},
                   "opponent": {"checkpoint": {"path": str(self.opponent), "sha256": pc.sha256_file(self.opponent)}},
                   "output_directory": "unused"}
        attempt = self.root / "learner-run" / "iterations" / f"{number:06d}" / "attempt-000000"
        attempt.mkdir(parents=True)
        path = attempt / "collect-command.json"
        path.write_text(json.dumps(command))
        return path

    def setUp(self):
        super().setUp()
        self.opponent = self.pins / "run" / "opponent.json"
        self.opponent.parent.mkdir()
        self.opponent.write_bytes(b"fixed opponent")

    def expected(self):
        return [episode_bytes(e).decode() for e in self.episodes]

    def scratch_empty(self, host):
        return [n for n in os.listdir(host.scratch) if not n.startswith(".")] == []


class RequestsPerShardTests(OverheadCase):
    def test_a_steady_update_sends_one_request_per_step_and_asks_only_for_the_new_checkpoint(self):
        hosts = [self.host("a"), self.host("b")]
        dispatcher = self.dispatcher(hosts)
        receipt = dispatcher.collect(self.iteration(1), fake_merge)
        self.assertEqual(receipt["merged"]["slots"], self.expected())
        # First update: one pin_status for both inputs (present here: the hosts share the learner's pin
        # root, so nothing is pushed), then store, run, fetch and one removal.
        steps = ["pin_status", "put_shard", "run_shard", "fetch_shard", "remove_shard"]
        for host in hosts:
            self.assertEqual(self.counts[host.name], steps)
            self.assertTrue(self.scratch_empty(host), "the shard and its command file are removed")
        self.assertEqual(self.counts["asked"], [sorted([str(self.opponent), str(self.pins / "run" / "checkpoint-1.json")])] * 2)
        self.counts.clear()
        receipt = dispatcher.collect(self.iteration(2), fake_merge)
        self.assertEqual(receipt["merged"]["slots"], self.expected())
        # A steady update: the opponent is known (the agent still checks it at put_shard and run_shard);
        # only the new checkpoint is asked for.
        for host in hosts:
            self.assertEqual(self.counts[host.name], steps)
        self.assertEqual(self.counts["asked"], [[str(self.pins / "run" / "checkpoint-2.json")]] * 2)
        self.assertEqual(receipt["calls"]["a"]["put_shard"][0], 1)
        self.assertEqual(set(receipt["timing"]), {"plan", "shards", "audit", "merge", "staging_removal",
                                                  "cleanup_wait", "prune"})
        first = self.root / "learner-run" / "iterations" / "000001" / "attempt-000000" / "pooled-shards"
        self.assertIn("since_load", json.loads((first / "dispatch.json").read_text())["timing"])
        self.counts.clear()
        receipt = dispatcher.collect(self.iteration(3), fake_merge)
        self.assertEqual(receipt["merged"]["slots"], self.expected())

    def test_an_input_removed_behind_the_dispatchers_back_is_pushed_again_without_failover(self):
        hosts = [self.host("a")]
        dispatcher = self.dispatcher(hosts)

        fake = AgentLikeFake(hosts[0], self.counts)
        dispatcher.transports["a"] = fake
        receipt = dispatcher.collect(self.iteration(1), fake_merge)
        self.assertEqual(receipt["merged"]["slots"], self.expected())
        del fake.files[str(self.opponent)]  # gone on the collector (another lineage's retention, a wiped root)
        self.counts.clear()
        receipt = dispatcher.collect(self.iteration(2), fake_merge)
        self.assertEqual(receipt["merged"]["slots"], self.expected())
        self.assertEqual(receipt["failed_hosts"], [])
        self.assertTrue(any(e["event"] == "pins-rechecked" for e in receipt["events"]))
        self.assertIn(str(self.opponent), fake.files)
        self.assertEqual(self.counts["a"], ["pin_status", "push", "put_shard", "pin_status", "push", "put_shard",
                                            "run_shard", "fetch_shard", "remove_shard"])

    def test_a_nested_input_known_from_an_earlier_collection_is_asked_again(self):
        # The agent refuses a command whose own pins are missing, but lets a missing nested or import file
        # through to the loader: only the command's own pins are trusted across collections.
        hosts = [self.host("a")]
        dispatcher = self.dispatcher(hosts)
        fake = AgentLikeFake(hosts[0], self.counts)
        dispatcher.transports["a"] = fake
        leaf = self.pins / "run" / "parameters.f32le"
        leaf.write_bytes(b"parameters")
        record = self.pins / "run" / "import.json"
        record.write_text(json.dumps({"parameters": {"path": str(leaf), "sha256": pc.sha256_file(leaf)}}))

        def with_import(path):
            command = json.loads(path.read_text())
            command["play_import"] = {"path": str(record), "sha256": pc.sha256_file(record)}
            path.write_text(json.dumps(command))
            return path

        receipt = dispatcher.collect(with_import(self.iteration(1)), fake_merge)
        self.assertEqual(receipt["merged"]["slots"], self.expected())
        del fake.files[str(leaf)]  # removed on the collector by another lineage's retention
        self.counts.clear()
        receipt = dispatcher.collect(with_import(self.iteration(2)), fake_merge)
        self.assertEqual(receipt["merged"]["slots"], self.expected())
        self.assertEqual(self.counts["a"][:2], ["pin_status", "push"])
        self.assertIn(str(leaf), fake.files)
        self.assertNotIn("pins-rechecked", [e["event"] for e in receipt["events"]], "asked, not refused")

    def test_shell_and_local_hosts_ask_again_every_collection(self):
        config = {"hosts": [pc.Host(name="l", kind="local", collector="/bin/l", collector_sha256="c" * 64,
                                    workers=2, weight=1.0, scratch=str(self.root / "scratch-l"), arch="x86")]}
        transport = FakeTransport(config["hosts"][0], {"/bin/l": "c" * 64})
        asked = []
        real = transport.pin_status
        transport.pin_status = lambda pins: asked.append(sorted(pins)) or real(pins)
        dispatcher = pc.Dispatcher(config, {"l": transport})
        for number in (1, 2):
            receipt = dispatcher.collect(self.iteration(number), fake_merge)
            self.assertEqual(receipt["merged"]["slots"], self.expected())
        collector = [p for p in asked if p == [("/bin/l", "c" * 64)]]
        self.assertEqual(len(collector), 2, "the collector pin is checked once per collection")
        self.assertEqual(sum(1 for p in asked if (str(self.opponent), pc.sha256_file(self.opponent)) in p), 2)


class ReusedConnectionTests(OverheadCase):
    def mux_host(self, name, arch="x86"):
        state = self.root / f"mux-{name}"
        state.mkdir()
        return self.host(name, arch, MUX_SSH, state), state

    def tearDown(self):
        for state in self.root.glob("mux-*"):
            pid = state / "master.pid"
            if pid.exists():
                try:
                    os.kill(int(pid.read_text()), 9)
                except OSError:
                    pass
        super().tearDown()

    def test_a_background_master_holding_no_stream_never_holds_a_call(self):
        host, state = self.mux_host("a")
        dispatcher = self.dispatcher([host])
        started = time.monotonic()
        receipt = dispatcher.collect(self.iteration(1), fake_merge)
        self.assertLess(time.monotonic() - started, 30, "no call waited for the master to exit")
        self.assertEqual(receipt["merged"]["slots"], self.expected())
        self.assertTrue((state / "master.pid").exists())

    def test_a_master_dying_mid_update_fails_the_host_over_and_is_replaced_next_time(self):
        (a, state_a), (b, _) = self.mux_host("a"), self.mux_host("b")
        dispatcher = self.dispatcher([a, b])
        dispatcher.collect(self.iteration(1), fake_merge)
        original_run = CountingAgentTransport.run

        def run_then_die(transport, argv, timeout):
            if transport.host.name == "a":
                (state_a / "DEAD").write_text("")  # the master dies under the running request's peers
                raise RuntimeError("a: agent run_shard: mux_client_request_session: read from master failed")
            return original_run(transport, argv, timeout)

        with mock.patch.object(CountingAgentTransport, "run", run_then_die):
            receipt = dispatcher.collect(self.iteration(2), fake_merge)
        self.assertEqual(receipt["merged"]["slots"], self.expected())
        self.assertEqual(receipt["failed_hosts"], ["a"])
        self.assertEqual([s[3] for s in receipt["shards"]], ["b", "b"])
        self.assertTrue(any(e["event"] == "scratch-cleanup-failed" and e["host"] == "a" for e in receipt["events"]))
        self.assertNotIn("a", dispatcher.known, "a failed host's pin knowledge is dropped")
        (state_a / "DEAD").unlink()  # the next call starts a new master
        (state_a / "master.pid").unlink()
        self.counts.clear()
        receipt = dispatcher.collect(self.iteration(3), fake_merge)
        self.assertEqual(receipt["merged"]["slots"], self.expected())
        self.assertEqual(receipt["failed_hosts"], [])
        self.assertEqual(self.counts["a"][0], "pin_status")
        # The stale shard of the dead update is gone or left only on the failed host's scratch.
        self.assertTrue(self.scratch_empty(b))


class ConcurrentAuditTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="pooled-audit-test-"))
        checkpoint = self.root / "learner" / "checkpoint.json"
        checkpoint.parent.mkdir(parents=True)
        checkpoint.write_bytes(b"checkpoint bytes")
        self.episodes = [{"id": f"e{i}", "seed": 1000 + i} for i in range(23)]
        command = {"mode": "collect_parallel", "workers": 4, "episodes": self.episodes,
                   "source": {"checkpoint": {"path": str(checkpoint), "sha256": pc.sha256_file(checkpoint)}},
                   "output_directory": "unused"}
        attempt = self.root / "run" / "iterations" / "000016" / "attempt-000000"
        attempt.mkdir(parents=True)
        self.command_path = attempt / "collect-command.json"
        self.command_path.write_text(json.dumps(command))

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def test_audits_on_different_hosts_run_at_the_same_time(self):
        # Iteration 16 audits slots 8 (played on x, replayed on m) and 20 (played on m, replayed on x).
        self.assertEqual(pc.audit_slots(16, 23, 2, 16), [8, 20])
        hosts = [pc.Host(name=n, kind="local", collector=f"/bin/{n}", collector_sha256="c" * 64, workers=4,
                         weight=1.0, scratch=str(self.root / "scratch" / n), arch=a) for n, a in (("x", "x86"),
                                                                                                 ("m", "arm64"))]
        together = threading.Barrier(2, timeout=10)

        class AuditMeets(FakeTransport):
            def run(self, argv, timeout):
                if "-audit" in Path(argv[-1]).name:
                    together.wait()  # both replays must be running at once
                return super().run(argv, timeout)

        transports = {h.name: AuditMeets(h, {h.collector: h.collector_sha256}) for h in hosts}
        receipt = pc.Dispatcher({"hosts": hosts, "audit": {"every": 16, "episodes": 2}}, transports).collect(
            self.command_path, fake_merge)
        self.assertEqual(receipt["merged"]["slots"], [episode_bytes(e).decode() for e in self.episodes])
        audits = sorted((e["slot"], e["played_on"], e["replayed_on"], e["equal"]) for e in receipt["events"]
                        if e["event"] == "audit")
        self.assertEqual(audits, [(8, "x", "m", True), (20, "m", "x", True)])


    def test_a_malformed_replay_fails_the_collection_before_the_merge(self):
        hosts = [pc.Host(name=n, kind="local", collector=f"/bin/{n}", collector_sha256="c" * 64, workers=4,
                         weight=1.0, scratch=str(self.root / "scratch" / n), arch=a) for n, a in (("x", "x86"),
                                                                                                 ("m", "arm64"))]

        class EmptyReplay(FakeTransport):
            def run(self, argv, timeout):
                result = super().run(argv, timeout)
                if "-audit" in Path(argv[-1]).name:  # a collector that exits 0 and names no trajectory
                    out = Path(json.loads(self.files[argv[-1]])["output_directory"])
                    (out / "collection.json").write_text(json.dumps({"trajectories": []}))
                return result

        transports = {h.name: EmptyReplay(h, {h.collector: h.collector_sha256}) for h in hosts}
        merged = []
        dispatcher = pc.Dispatcher({"hosts": hosts, "audit": {"every": 16, "episodes": 2}}, transports)
        with self.assertRaisesRegex(RuntimeError, "audit of slot 8 on m: IndexError"):
            dispatcher.collect(self.command_path, lambda *a: merged.append(a))
        self.assertEqual(merged, [], "no merge after an audit that compared nothing")
        self.assertFalse((self.command_path.parent / "collect").exists())
        self.assertEqual(dispatcher.known, {}, "a host whose replay failed keeps no pin knowledge")


class LearnerSideTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="pooled-learner-test-"))

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def test_a_known_hash_is_taken_again_once_the_file_changes(self):
        path = self.root / "input.bin"
        path.write_bytes(b"first")
        self.assertEqual(pc.sha256_file_known(path), pc.sha256_file(path))
        with mock.patch.object(pc, "sha256_file", side_effect=AssertionError("rehashed")):
            self.assertEqual(pc.sha256_file_known(path), pc.sha256_file_known(path))
        time.sleep(0.01)
        path.write_bytes(b"second, longer")
        self.assertEqual(pc.sha256_file_known(path), pc.sha256_file(path))

    def test_a_local_shard_moves_into_staging(self):
        host = pc.Host(name="l", kind="local", collector="/c", collector_sha256="0" * 64, workers=2, weight=1.0,
                       scratch=str(self.root / "scratch"), arch="x86")
        out = self.root / "scratch" / "pooled-x"
        out.mkdir(parents=True)
        (out / "episode-0000.json").write_bytes(b"bytes")
        local = self.root / "staging" / "pooled-x"
        local.parent.mkdir()
        pc.Transport(host).fetch_dir(str(out), local)
        self.assertEqual((local / "episode-0000.json").read_bytes(), b"bytes")
        self.assertFalse(out.exists())
        command = self.root / "scratch" / "pooled-x.json"
        command.write_text("{}")
        out.mkdir()
        pc.Transport(host).remove_shard(str(out), str(command))
        self.assertEqual(os.listdir(self.root / "scratch"), [])


if __name__ == "__main__":
    unittest.main()
