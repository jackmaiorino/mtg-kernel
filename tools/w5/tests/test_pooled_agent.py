"""The collector agent behind a forced ssh command: confinement and the agent-mode dispatcher.
The agent serves exactly one of seven operations per connection and refuses everything else."""
import base64
import hashlib
import io
import json
import os
import shutil
import sys
import tarfile
import tempfile
import textwrap
import threading
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE / "tests"))
import pooled_collect as pc  # noqa: E402
from test_pooled_collect import episode_bytes, fake_merge  # noqa: E402

FAKE_COLLECTOR = textwrap.dedent('''
    import hashlib, json, os, sys, time
    assert sys.argv[1] == "--execute-collect-shard"
    command = json.load(open(sys.argv[2]))
    if command["episodes"] and command["episodes"][0]["id"] == "sleep":
        time.sleep(60)
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

FAKE_SSH = textwrap.dedent('''
    # Stands in for sshd with a forced command: the requested command becomes SSH_ORIGINAL_COMMAND and
    # only the agent runs. argv: fake_ssh.py AGENT_CONFIG REQUESTED_COMMAND
    import os, subprocess, sys
    env = dict(os.environ, SSH_ORIGINAL_COMMAND=sys.argv[2])
    sys.exit(subprocess.run([sys.executable, "-I", os.environ["POOLED_AGENT_SCRIPT"], "agent", sys.argv[1]],
                            env=env).returncode)
''')

TAG = "pooled-000003-" + "a" * 32 + "-s001"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class AgentCase(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="pooled-agent-test-")).resolve()
        self.pins = self.root / "pins"
        self.scratch = self.root / "scratch"
        self.pins.mkdir()
        self.scratch.mkdir()
        self.collector = self.root / "collector.py"
        self.collector.write_text(FAKE_COLLECTOR)
        self.config_path = self.write_config(self.root / "agent.json")
        os.environ["POOLED_AGENT_SCRIPT"] = str(HERE / "pooled_collect.py")

    def write_config(self, path, **changes):
        config = {"schema": pc.AGENT_SCHEMA, "collector": str(self.collector),
                  "collector_sha256": pc.sha256_file(self.collector), "scratch": str(self.scratch),
                  "pin_roots": [str(self.pins)], "prefix": [sys.executable], "max_seconds": 120}
        config.update(changes)
        path.write_text(json.dumps(config))
        return path

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def serve(self, op=None, payload=b"", raw=None, config=None, **args):
        encoded = raw if raw is not None else pc.encode_request(op, **args)
        stdout, stderr = io.BytesIO(), io.StringIO()
        code = pc.agent_main(str(config or self.config_path), {"SSH_ORIGINAL_COMMAND": encoded},
                             io.BytesIO(payload), stdout, stderr)
        return code, stdout.getvalue(), stderr.getvalue()

    def assertRefused(self, result, words):
        code, out, err = result
        self.assertEqual(code, 2, err)
        self.assertIn("refused", err)
        self.assertIn(words, err)

    def shard_command(self, episodes=None, **changes):
        command = {"mode": "collect_parallel", "shard": {"first_slot": 2, "schedule_episodes": 4},
                   "episodes": episodes or [{"id": "e2", "seed": 7}, {"id": "e3", "seed": 8}],
                   "output_directory": str(self.scratch) + "/" + TAG}
        command.update(changes)
        return command

    def put_shard(self, **changes):
        code, out, err = self.serve("put_shard", json.dumps(self.shard_command(**changes)).encode(), tag=TAG)
        self.assertEqual(code, 0, err)
        self.assertEqual(json.loads(out), "stored")

    def run_shard(self, seconds=60, collector_sha256=None):
        return self.serve("run_shard", tag=TAG, seconds=seconds,
                          collector_sha256=collector_sha256 or pc.sha256_file(self.collector))


class AgentRefusalTests(AgentCase):
    def test_an_operation_outside_the_seven_is_refused(self):
        self.assertRefused(self.serve("shell", command="id"), "operation not served")
        self.assertRefused(self.serve("pin_status", pins=[], extra=1), "takes exactly")

    def test_a_path_outside_the_pin_roots_is_refused(self):
        data = b"bytes"
        self.assertRefused(self.serve("push", data, path=str(self.root / "elsewhere.bin"), sha256=sha(data)),
                           "outside the pin roots")
        escaping = str(self.pins) + os.sep + ".." + os.sep + "escape.bin"
        self.assertRefused(self.serve("push", data, path=escaping, sha256=sha(data)), "outside the pin roots")
        self.assertRefused(self.serve("push", data, path=str(self.pins), sha256=sha(data)), "outside the pin roots")
        self.assertRefused(self.serve("remove_pin", path=str(self.config_path), sha256="0" * 64),
                           "outside the pin roots")
        self.assertEqual(sorted(p.name for p in self.root.iterdir()),
                         ["agent.json", "collector.py", "pins", "scratch"], "no file outside the roots")

    @unittest.skipUnless(os.name == "posix", "symlinks need privileges on Windows")
    def test_symlinks_are_never_followed(self):
        os.symlink(self.root, self.pins / "link")
        data = b"bytes"
        self.assertRefused(self.serve("push", data, path=str(self.pins / "link" / "x.bin"), sha256=sha(data)),
                           "outside the pin roots")
        target = self.root / "planted.json"
        target.write_text(json.dumps(self.shard_command(output_directory=str(self.root / "out"))))
        os.symlink(target, self.scratch / f"{TAG}.json")
        self.assertRefused(self.run_shard(), "not a regular file")
        self.assertFalse((self.root / "out").exists())

    def test_a_bad_tag_is_refused(self):
        for tag in ("../etc", "pooled-1", TAG + "/x", TAG.replace("-s001", "-s1"), ""):
            self.assertRefused(self.serve("remove_shard", tag=tag), "bad tag")

    def test_oversized_or_malformed_requests_are_refused(self):
        self.assertRefused(self.serve(raw="A" * (pc.AGENT_REQUEST_BYTES + 1)), "oversized")
        self.assertRefused(self.serve(raw=""), "missing")
        self.assertRefused(self.serve(raw="not base64!"), "base64url")
        bad_json = base64.urlsafe_b64encode(b"{not json").decode().rstrip("=")
        self.assertRefused(self.serve(raw=bad_json), "malformed")
        deep = base64.urlsafe_b64encode(b"[" * 20000 + b"]" * 20000).decode().rstrip("=")
        self.assertRefused(self.serve(raw=deep), "malformed")
        wrong_schema = base64.urlsafe_b64encode(json.dumps({"schema": "x", "op": "pin_status", "args": {}})
                                                .encode()).decode().rstrip("=")
        self.assertRefused(self.serve(raw=wrong_schema), "request/v1")
        self.assertRefused(self.serve("pin_status", pins="x"), "list")
        self.assertRefused(self.serve("put_shard", b"x" * (pc.AGENT_SHARD_BYTES + 1), tag=TAG), "exceeds")

    def test_a_second_request_on_one_connection_is_refused(self):
        one = json.dumps({"schema": pc.AGENT_REQUEST_SCHEMA, "op": "pin_status", "args": {"pins": []}})
        two = base64.urlsafe_b64encode((one + one).encode()).decode().rstrip("=")
        self.assertRefused(self.serve(raw=two), "malformed")
        trailing = pc.encode_request("remove_shard", tag=TAG).encode()
        self.assertRefused(self.serve("pin_status", trailing, pins=[]), "one request per connection")
        self.assertRefused(self.serve("remove_shard", trailing, tag=TAG), "one request per connection")
        # A push payload is file bytes, never a request: an encoded request inside it is only data.
        path = str(self.pins / "payload.bin")
        code, out, err = self.serve("push", trailing, path=path, sha256=sha(trailing))
        self.assertEqual((code, json.loads(out)), (0, "created"), err)
        self.assertEqual(Path(path).read_bytes(), trailing)

    def test_shard_commands_are_confined(self):
        bad = self.shard_command(output_directory=str(self.root / "x"))
        self.assertRefused(self.serve("put_shard", json.dumps(bad).encode(), tag=TAG), "own scratch")
        sneaky = self.shard_command(output_directory=str(self.scratch) + "/link/../" + TAG)
        self.assertRefused(self.serve("put_shard", json.dumps(sneaky).encode(), tag=TAG), "own scratch")
        self.assertRefused(self.serve("put_shard", json.dumps(self.shard_command(mode="update")).encode(), tag=TAG),
                           "own scratch")
        outside = self.shard_command(source={"checkpoint": {"path": str(self.root / "agent.json"),
                                                             "sha256": "0" * 64}})
        self.assertRefused(self.serve("put_shard", json.dumps(outside).encode(), tag=TAG), "outside the pin roots")
        imp = self.pins / "import.json"
        imp.write_text(json.dumps({"export_directory": str(self.root), "source_run_path": str(self.pins / "r"),
                                   "source_registry_path": str(self.pins / "g")}))
        nested = self.shard_command(source={"play_import": {"path": str(imp), "sha256": pc.sha256_file(imp)}})
        self.assertRefused(self.serve("put_shard", json.dumps(nested).encode(), tag=TAG), "outside the pin roots")

    def test_the_learner_and_the_agent_must_expect_the_same_collector(self):
        self.put_shard()
        self.assertRefused(self.run_shard(collector_sha256="0" * 64), "another collector")
        self.assertRefused(self.run_shard(seconds=10 ** 6), "seconds")
        configured = json.loads(self.config_path.read_text())["collector_sha256"]
        self.collector.write_text(FAKE_COLLECTOR + "\n# changed\n")
        self.assertRefused(self.run_shard(collector_sha256=configured), "collector binary differs")

    def test_a_hard_linked_output_is_never_fetched(self):
        self.put_shard()
        self.assertEqual(json.loads(self.run_shard()[1])["code"], 0)
        secret = self.root / "secret.txt"
        secret.write_text("not for the learner")
        os.link(secret, self.scratch / TAG / "leak.txt")
        self.assertRefused(self.serve("fetch_shard", tag=TAG), "singly linked")

    def test_unsafe_configs_are_refused(self):
        overlapping = self.write_config(self.root / "overlap.json", pin_roots=[str(self.root)])
        self.assertEqual(self.serve("pin_status", pins=[], config=overlapping)[0], 1)
        inside = self.write_config(self.root / "inside.json", collector=str(self.pins / "collector.py"))
        self.assertEqual(self.serve("pin_status", pins=[], config=inside)[0], 1)


class AgentServiceTests(AgentCase):
    def test_pins_install_once_never_replace_and_prune_only_their_bytes(self):
        data, other = b"checkpoint", b"other bytes"
        path = str(self.pins / "run" / "checkpoint.json")
        self.assertEqual(json.loads(self.serve("pin_status", pins=[[path, sha(data)]])[1]), ["absent"])
        self.assertEqual(json.loads(self.serve("push", data, path=path, sha256=sha(data))[1]), "created")
        self.assertEqual(json.loads(self.serve("push", data, path=path, sha256=sha(data))[1]), "existing")
        self.assertRefused(self.serve("push", other, path=path, sha256=sha(other)), "existing pin differs")
        self.assertRefused(self.serve("push", other, path=path, sha256=sha(data)), "payload differs")
        self.assertEqual(json.loads(self.serve("pin_status", pins=[[path, sha(other)]])[1]), ["differs"])
        self.assertEqual(json.loads(self.serve("remove_pin", path=path, sha256=sha(other))[1]), "kept")
        self.assertEqual(json.loads(self.serve("remove_pin", path=path, sha256=sha(data))[1]), "removed")
        self.assertEqual(sorted(os.listdir(self.pins / "run")), [])

    def test_remove_pin_waits_for_a_push_on_the_same_path(self):
        data = b"checkpoint"
        path = str(self.pins / "run" / "checkpoint.json")
        self.serve("push", data, path=path, sha256=sha(data))
        config = pc.load_agent_config(self.config_path)
        done = threading.Event()

        def remove():
            self.serve("remove_pin", path=path, sha256=sha(data))
            done.set()

        with pc._lock(config, path):  # another connection is mid-push on this path
            worker = threading.Thread(target=remove)
            worker.start()
            self.assertFalse(done.wait(1.0), "remove_pin must wait for the path's lock")
        worker.join(10)
        self.assertTrue(done.is_set())
        self.assertFalse(Path(path).exists())

    def test_a_shard_is_stored_run_fetched_and_removed(self):
        self.put_shard()
        code, out, err = self.run_shard()
        self.assertEqual((code, json.loads(out)["code"]), (0, 0), err)
        code, tar_bytes, err = self.serve("fetch_shard", tag=TAG)
        self.assertEqual(code, 0, err)
        with tarfile.open(fileobj=io.BytesIO(tar_bytes)) as archive:
            self.assertEqual(sorted(archive.getnames()), ["collection.json", "episode-0002.json", "episode-0003.json"])
        self.assertEqual(json.loads(self.serve("remove_shard", tag=TAG)[1]), "removed")
        self.assertEqual([n for n in os.listdir(self.scratch) if not n.startswith(".")], [])

    @unittest.skipUnless(os.name == "posix", "process-group termination is POSIX")
    def test_a_run_past_its_seconds_is_ended(self):
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        code, out, err = self.run_shard(seconds=1)
        self.assertEqual((code, json.loads(out)["code"]), (0, "timeout"), err)


class SafeExtractionTests(unittest.TestCase):
    def test_archives_with_unsafe_members_are_refused(self):
        root = Path(tempfile.mkdtemp(prefix="pooled-extract-test-"))
        try:
            for name in ("..\\..\\victim", "C:\\victim", "../victim", "a/b", "file:stream", ".hidden"):
                archive = root / "bad.tar"
                with tarfile.open(archive, "w") as tar:
                    entry = tarfile.TarInfo(name)
                    entry.size = 1
                    tar.addfile(entry, io.BytesIO(b"x"))
                local = root / "out"
                local.mkdir()
                with self.assertRaises(RuntimeError):
                    pc._extract_shard_archive(archive, local)
                shutil.rmtree(local)
            archive = root / "dup.tar"
            with tarfile.open(archive, "w") as tar:
                for _ in range(2):
                    entry = tarfile.TarInfo("a.json")
                    entry.size = 1
                    tar.addfile(entry, io.BytesIO(b"x"))
            local = root / "out"
            local.mkdir()
            with self.assertRaises(RuntimeError):
                pc._extract_shard_archive(archive, local)
        finally:
            shutil.rmtree(root, ignore_errors=True)


class AgentDispatcherTests(AgentCase):
    def agent_host(self, name, arch, config_path):
        fake_ssh = self.root / "fake_ssh.py"
        fake_ssh.write_text(FAKE_SSH)
        return pc.Host(name=name, kind="ssh", collector=str(self.collector),
                       collector_sha256=pc.sha256_file(self.collector), workers=2, weight=1.0,
                       scratch=str(self.scratch), arch=arch, ssh=[sys.executable, str(fake_ssh), str(config_path)],
                       prefix=[sys.executable], transport="agent")

    def test_the_dispatcher_collects_through_the_agent_with_audits_and_failover(self):
        broken = self.write_config(self.root / "broken.json", collector_sha256="0" * 64)
        hosts = [self.agent_host("broken", "arm64", broken), self.agent_host("good-a", "x86", self.config_path),
                 self.agent_host("good-b", "arm64", self.config_path)]
        pinned = self.pins / "run" / "checkpoint.json"
        pinned.parent.mkdir()
        pinned.write_bytes(b"policy")
        episodes = [{"id": f"e{i}", "seed": 100 + i} for i in range(9)]
        command = {"mode": "collect_parallel", "workers": 2, "episodes": episodes,
                   "source": {"checkpoint": {"path": str(pinned), "sha256": pc.sha256_file(pinned)}},
                   "output_directory": "unused"}
        attempt = self.root / "learner-run" / "iterations" / "000003" / "attempt-000000"
        attempt.mkdir(parents=True)
        command_path = attempt / "collect-command.json"
        command_path.write_text(json.dumps(command))
        dispatcher = pc.Dispatcher({"hosts": hosts, "audit": {"every": 1, "episodes": 2}}, timeout=100)
        receipt = dispatcher.collect(command_path, fake_merge)
        self.assertEqual(receipt["merged"]["slots"], [episode_bytes(e).decode() for e in episodes])
        self.assertEqual(receipt["failed_hosts"], ["broken"])
        audits = [e for e in receipt["events"] if e["event"] == "audit"]
        self.assertEqual(len(audits), 2)
        self.assertTrue(all(e["equal"] for e in audits))
        self.assertEqual([n for n in os.listdir(self.scratch) if not n.startswith(".")], [],
                         "every shard and command file removed")

    def test_agent_mode_uploads_missing_pins_and_prunes_only_its_own(self):
        transport = pc.AgentTransport(self.agent_host("a", "x86", self.config_path))
        learner_copy = self.root / "learner-copy.bin"
        learner_copy.write_bytes(b"input bytes")
        remote = str(self.pins / "inputs" / "input.bin")
        self.assertTrue(transport.push(learner_copy, remote, sha(b"input bytes")))
        self.assertFalse(transport.push(learner_copy, remote, sha(b"input bytes")), "existing: not ours")
        self.assertEqual(Path(remote).read_bytes(), b"input bytes")
        self.assertFalse(transport.remove_pin(remote, sha(b"other")))
        self.assertTrue(transport.remove_pin(remote, sha(b"input bytes")))
        self.assertFalse(Path(remote).exists())


if __name__ == "__main__":
    unittest.main()
