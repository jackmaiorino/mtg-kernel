"""The resident dispatcher: the per-update program becomes a thin client of one dispatcher that stays up for
the run. Its answer is the one-shot program's (exit code, stderr, published collection, same bytes); with no
server the client collects in its own process; after committing it never does (a lost server fails closed).
POSIX only (unix sockets)."""
import io
import json
import os
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import textwrap
import threading
import time
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE / "tests"))
import pooled_collect as pc  # noqa: E402
from test_pooled_collect import FakeTransport, episode_bytes, fake_merge  # noqa: E402

POSIX = os.name == "posix" and hasattr(socket, "AF_UNIX")

# A collector program for local hosts: writes each slot's bytes as a function of its episode alone.
COLLECTOR = textwrap.dedent('''
    import hashlib, json, os, sys
    command = json.load(open(sys.argv[2]))
    out = command["output_directory"]
    os.makedirs(out)
    pins = []
    for offset, episode in enumerate(command["episodes"]):
        name = "episode-%04d.json" % (command["shard"]["first_slot"] + offset)
        data = ("%s:%s" % (episode["id"], episode["seed"])).encode()
        open(os.path.join(out, name), "wb").write(data)
        pins.append({"path": os.path.join(out, name), "sha256": hashlib.sha256(data).hexdigest()})
    json.dump({"trajectories": pins, "shard": command["shard"]}, open(os.path.join(out, "collection.json"), "w"))
''')

# The trainer's --merge-collection-shards, reduced: copies every trajectory into the attempt's collect/.
MERGE = textwrap.dedent('''\
    #!PYTHON
    import hashlib, json, os, shutil, sys
    assert sys.argv[1] == "--merge-collection-shards"
    out = os.path.join(os.path.dirname(sys.argv[2]), "collect")
    os.makedirs(out)  # refuses an existing collect/, as the trainer does
    names = []
    for directory in sys.argv[3:]:
        for pin in json.load(open(os.path.join(directory, "collection.json")))["trajectories"]:
            name = os.path.basename(pin["path"])
            shutil.copyfile(os.path.join(directory, name), os.path.join(out, name))
            names.append(name)
    json.dump({"trajectories": names}, open(os.path.join(out, "collection.json"), "w"))
    print(json.dumps({"trajectories": names, "behavior_state_sha256": "s"}))
''')


def wait_for(predicate, seconds=20.0):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


class ResidentCase(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="pooled-resident-test-")).resolve()
        self.sockets = self.root / "run" / ".resident"
        checkpoint = self.root / "learner" / "checkpoint.json"
        checkpoint.parent.mkdir(parents=True)
        checkpoint.write_bytes(b"checkpoint bytes")
        self.checkpoint = checkpoint
        self.episodes = [{"id": f"e{i}", "seed": 1000 + i} for i in range(9)]
        collector = self.root / "collector.py"
        collector.write_text(COLLECTOR)
        merge = self.root / "merge.py"
        merge.write_text(MERGE.replace("PYTHON", sys.executable))
        merge.chmod(0o755)
        hosts = [{"name": n, "kind": "local", "collector": str(collector), "collector_sha256": pc.sha256_file(collector),
                  "workers": 2, "weight": 1.0, "scratch": str(self.root / f"scratch-{n}"), "arch": a,
                  "prefix": [sys.executable]} for n, a in (("a", "x86"), ("b", "arm64"))]
        self.hosts_path = self.root / "hosts.json"
        self.hosts_path.write_text(json.dumps({"schema": pc.SCHEMA, "merge_binary": str(merge), "hosts": hosts,
                                               "audit": {"every": 2, "episodes": 2}}))

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def command(self, iteration, run="run"):
        command = {"mode": "collect_parallel", "workers": 4, "episodes": self.episodes,
                   "source": {"checkpoint": {"path": str(self.checkpoint), "sha256": pc.sha256_file(self.checkpoint)}},
                   "output_directory": "unused"}
        attempt = self.root / run / "iterations" / f"{iteration:06d}" / "attempt-000000"
        attempt.mkdir(parents=True)
        path = attempt / "collect-command.json"
        path.write_text(json.dumps(command))
        return path

    def expected(self):
        return {f"episode-{i:04d}.json": episode_bytes(e) for i, e in enumerate(self.episodes)}

    def collected(self, command_path):
        out = command_path.parent / "collect"
        return {p.name: p.read_bytes() for p in out.iterdir() if p.name != "collection.json"}

    def start_server(self, **kwargs):
        stderr = io.StringIO()
        server = pc.ResidentServer(str(self.sockets), str(self.hosts_path), stderr=stderr, **kwargs)
        thread = threading.Thread(target=server.serve, daemon=True)
        thread.start()
        self.assertTrue(wait_for(lambda: (self.sockets / pc.RESIDENT_SOCKET).exists()))
        self.addCleanup(lambda: (server.stop(), thread.join(10)))
        return server, thread

    def client(self, command_path, hosts=None):
        stderr = io.StringIO()
        code = pc.resident_client(str(command_path), str(self.sockets), str(hosts or self.hosts_path), stderr)
        return code, stderr.getvalue()


@unittest.skipUnless(POSIX, "unix sockets")
class ResidentServiceTests(ResidentCase):
    def test_the_client_returns_the_one_shot_answer_and_bytes(self):
        server, _ = self.start_server()
        for iteration in (1, 2, 3):
            path = self.command(iteration)
            code, text = self.client(path)
            self.assertEqual(code, 0, text)
            summary = json.loads(text)
            self.assertEqual((summary["iteration"], summary["episodes"], summary["failed_hosts"]), (iteration, 9, []))
            self.assertEqual(self.collected(path), self.expected())
            receipt = json.loads((path.parent / "pooled-shards" / "dispatch.json").read_text())
            self.assertNotIn("since_load", receipt["timing"])
        self.assertEqual(server.dispatcher.collections, 3)
        # The same commands one-shot, in another run tree: the same files.
        for iteration in (1, 2):
            path = self.command(iteration, run="oneshot")
            os.environ["MTG_POOLED_COLLECT_CONFIG"] = str(self.hosts_path)
            try:
                self.assertEqual(pc.main(["pooled_collect.py", str(path)]), 0)
            finally:
                del os.environ["MTG_POOLED_COLLECT_CONFIG"]
            self.assertEqual(self.collected(path), self.expected())

    def test_a_failed_collection_is_exit_1_with_the_one_shot_message(self):
        self.start_server()
        path = self.command(1)
        (path.parent / "collect").mkdir()  # the merge refuses an existing collect/
        code, text = self.client(path)
        self.assertEqual(code, 1)
        self.assertTrue(text.startswith("pooled collection failed: merge failed"), text)

    def test_without_a_server_the_client_starts_nothing_and_hands_back(self):
        path = self.command(1)
        started = time.monotonic()
        code, text = self.client(path)
        self.assertIsNone(code)
        self.assertIn("collecting in this process", text)
        self.assertLess(time.monotonic() - started, 5)
        self.assertFalse((path.parent / "pooled-shards").exists())

    def test_another_hosts_file_or_changed_bytes_are_refused_before_anything_starts(self):
        self.start_server()
        path = self.command(1)
        document = json.loads(self.hosts_path.read_text())
        document["audit"] = {"every": 1, "episodes": 2}
        self.hosts_path.write_text(json.dumps(document))
        code, text = self.client(path)
        self.assertIsNone(code)
        self.assertIn("another hosts file", text)
        self.assertFalse((path.parent / "pooled-shards").exists())

    def test_a_server_lost_after_the_commit_fails_closed(self):
        # A stand-in server that accepts the request and the commit, then closes without an answer.
        self.sockets.mkdir(parents=True, mode=0o700)
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        listener.bind(str(self.sockets / pc.RESIDENT_SOCKET))
        listener.listen(1)

        def serve_once():
            conn, _ = listener.accept()
            pc._read_line(conn)
            pc._send_line(conn, {"ready": True})
            pc._read_line(conn)
            conn.close()

        thread = threading.Thread(target=serve_once, daemon=True)
        thread.start()
        try:
            code, text = self.client(self.command(1))
        finally:
            thread.join(10)
            listener.close()
        self.assertEqual(code, 1)
        self.assertIn("pooled collection failed", text)

    def test_a_client_that_never_commits_starts_nothing(self):
        server, _ = self.start_server()
        path = self.command(1)
        conn = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        conn.connect(str(self.sockets / pc.RESIDENT_SOCKET))
        pc._send_line(conn, {"schema": pc.RESIDENT_SCHEMA, "command": str(path), "hosts": str(self.hosts_path),
                             "hosts_sha256": pc._file_sha256(str(self.hosts_path)),
                             "source_sha256": pc._file_sha256(os.path.realpath(pc.__file__))})
        self.assertEqual(pc._read_line(conn), {"ready": True})
        conn.close()  # gone before "go"
        time.sleep(0.5)
        self.assertFalse((path.parent / "pooled-shards").exists())
        self.assertEqual(server.dispatcher.collections, 0)

    def test_the_socket_directory_is_private_and_served_once(self):
        self.sockets.mkdir(parents=True)
        os.chmod(self.sockets, 0o750)
        with self.assertRaisesRegex(ValueError, "mode 700"):
            pc.ResidentServer(str(self.sockets), str(self.hosts_path), stderr=io.StringIO()).serve()
        os.chmod(self.sockets, 0o700)
        self.start_server()
        self.assertEqual(stat.S_IMODE(os.stat(self.sockets / pc.RESIDENT_SOCKET).st_mode), 0o600)
        second = io.StringIO()
        self.assertEqual(pc.ResidentServer(str(self.sockets), str(self.hosts_path), stderr=second).serve(), 2)
        self.assertIn("another resident dispatcher", second.getvalue())

    def test_the_stop_file_and_idle_time_end_the_server(self):
        server, thread = self.start_server()
        (self.sockets / "STOP").write_text("")
        thread.join(10)
        self.assertFalse(thread.is_alive())
        self.assertFalse((self.sockets / pc.RESIDENT_SOCKET).exists())
        (self.sockets / "STOP").unlink()
        server, thread = self.start_server(idle_seconds=0.5)
        thread.join(10)
        self.assertFalse(thread.is_alive())

    def request(self, path, **changes):
        request = {"schema": pc.RESIDENT_SCHEMA, "command": str(path), "hosts": str(self.hosts_path),
                   "hosts_sha256": pc._file_sha256(str(self.hosts_path)),
                   "source_sha256": pc._file_sha256(os.path.realpath(pc.__file__)), "context": pc._resident_context()}
        request.update(changes)
        conn = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        conn.connect(str(self.sockets / pc.RESIDENT_SOCKET))
        pc._send_line(conn, request)
        answer = pc._read_line(conn)
        conn.close()
        return answer

    def test_another_process_context_is_refused_before_anything_starts(self):
        self.start_server()
        path = self.command(1)
        context = pc._resident_context()
        for changed in (dict(context, cwd="/elsewhere"), dict(context, env=dict(context["env"], OMP_NUM_THREADS="3")),
                        dict(context, cpus=[0])):
            answer = self.request(path, context=changed)
            self.assertFalse(answer["ready"])
            self.assertIn("another context", answer["reason"])
        self.assertIn("$OMP_NUM_THREADS", self.request(path, context=dict(
            context, env=dict(context["env"], OMP_NUM_THREADS="3")))["reason"])
        self.assertTrue(self.request(path)["ready"])  # never committed: nothing starts
        self.assertFalse((path.parent / "pooled-shards").exists())

    def test_relative_merge_or_state_paths_are_refused_by_the_server(self):
        document = json.loads(self.hosts_path.read_text())
        for key, value in (("merge_binary", "merge.py"), ("state_path", "pushed.json")):
            self.hosts_path.write_text(json.dumps(dict(document, **{key: value})))
            with self.assertRaisesRegex(ValueError, f"absolute {key}"):
                pc.ResidentServer(str(self.sockets), str(self.hosts_path), stderr=io.StringIO())

    def test_a_retention_failure_is_exit_1_as_one_shot(self):
        state = self.root / "state-is-a-directory"
        state.mkdir()
        document = json.loads(self.hosts_path.read_text())
        self.hosts_path.write_text(json.dumps(dict(document, state_path=str(state))))
        os.environ["MTG_POOLED_COLLECT_CONFIG"] = str(self.hosts_path)
        try:
            one_shot = pc.main(["pooled_collect.py", str(self.command(1, run="oneshot"))])
        finally:
            del os.environ["MTG_POOLED_COLLECT_CONFIG"]
        self.start_server()
        code, text = self.client(self.command(1))
        self.assertEqual((one_shot, code), (1, 1))
        self.assertTrue(text.startswith("pooled collection failed"), text)

    def test_the_client_hands_back_for_a_socket_directory_it_does_not_own_privately(self):
        self.start_server()
        os.chmod(self.sockets, 0o755)
        try:
            code, text = self.client(self.command(1))
        finally:
            os.chmod(self.sockets, 0o700)
        self.assertIsNone(code)
        self.assertIn("mode 700", text)

    def test_pin_knowledge_and_retention_carry_across_updates(self):
        hosts = [pc.Host(name="a", kind="ssh", collector="/bin/a", collector_sha256="c" * 64, workers=2, weight=1.0,
                         scratch=str(self.root / "scratch-fake"), arch="x86", ssh=["ssh", "a"], transport="agent",
                         prune_pushed=True)]
        transport = FakeTransport(hosts[0], {"/bin/a": "c" * 64})
        asked = []
        real = transport.pin_status
        transport.pin_status = lambda pins: asked.append(sorted(p[0] for p in pins)) or real(pins)
        dispatcher = pc.Dispatcher({"hosts": hosts, "state_path": str(self.root / "pushed.json")}, {"a": transport})
        self.start_server(dispatcher=dispatcher, merge=fake_merge)
        checkpoints = []
        for iteration in (1, 2, 3):
            self.checkpoint = self.root / "learner" / f"checkpoint-{iteration}.json"
            self.checkpoint.write_bytes(b"policy %d" % iteration)
            checkpoints.append(str(self.checkpoint))
            code, text = self.client(self.command(iteration))
            self.assertEqual(code, 0, text)
        self.assertEqual(asked, [["/bin/a"]] + [[c] for c in checkpoints], "only each new checkpoint is asked for")
        self.assertTrue(wait_for(lambda: str(checkpoints[1]) not in transport.files))
        self.assertNotIn(checkpoints[0], transport.files, "earlier checkpoints are pruned after the answer")
        self.assertIn(checkpoints[2], transport.files)


@unittest.skipUnless(POSIX, "unix sockets")
class ResidentProgramTests(ResidentCase):
    """The real programs: `serve` in its own process, and the per-update program the trainer starts."""

    def run_program(self, path, **env):
        environment = dict(os.environ, MTG_POOLED_COLLECT_CONFIG=str(self.hosts_path), **env)
        return subprocess.run([sys.executable, str(HERE / "pooled_collect.py"), str(path)], env=environment,
                              capture_output=True, text=True, timeout=120)

    def test_the_per_update_program_hands_off_and_falls_back_when_no_server_runs(self):
        server = subprocess.Popen([sys.executable, str(HERE / "pooled_collect.py"), "serve", str(self.sockets)],
                                  env=dict(os.environ, MTG_POOLED_COLLECT_CONFIG=str(self.hosts_path)),
                                  stderr=subprocess.PIPE, text=True)
        try:
            self.assertTrue(wait_for(lambda: (self.sockets / pc.RESIDENT_SOCKET).exists()))
            for iteration in (1, 2):
                path = self.command(iteration)
                result = self.run_program(path, MTG_POOLED_COLLECT_RESIDENT=str(self.sockets))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertNotIn("collecting in this process", result.stderr)
                self.assertEqual(json.loads(result.stderr)["iteration"], iteration)
                self.assertEqual(self.collected(path), self.expected())
        finally:
            server.terminate()
            server.communicate(timeout=20)
            self.assertEqual(server.returncode, 0)
        self.assertFalse((self.sockets / pc.RESIDENT_SOCKET).exists())
        path = self.command(3)
        result = self.run_program(path, MTG_POOLED_COLLECT_RESIDENT=str(self.sockets))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("collecting in this process", result.stderr)
        self.assertEqual(self.collected(path), self.expected())
        path = self.command(4)
        result = self.run_program(path, MTG_POOLED_COLLECT_RESIDENT=str(self.sockets),
                                  MTG_POOLED_COLLECT_RESIDENT_ONLY="1")
        self.assertEqual(result.returncode, 1)
        self.assertFalse((path.parent / "pooled-shards").exists())


class ResidentPlatformTests(unittest.TestCase):
    @unittest.skipIf(POSIX, "a platform without unix sockets")
    def test_without_unix_sockets_the_client_hands_back(self):
        stderr = io.StringIO()
        self.assertIsNone(pc.resident_client("C:/run/iterations/000001/attempt-000000/collect-command.json",
                                             "C:/run/.resident", None, stderr))


if __name__ == "__main__":
    unittest.main()
