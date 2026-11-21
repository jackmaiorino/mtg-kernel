"""The collector agent behind a forced ssh command: confinement and the agent-mode dispatcher.
The agent serves exactly one of seven operations per connection and refuses everything else."""
import base64
import hashlib
import io
import json
import os
import shutil
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import pooled_collect as pc  # noqa: E402

FAKE_COLLECTOR = textwrap.dedent('''
    import hashlib, json, os, sys
    assert sys.argv[1] == "--execute-collect-shard"
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

FAKE_SSH = textwrap.dedent('''
    # Stands in for sshd with a forced command: the requested command becomes SSH_ORIGINAL_COMMAND
    # and only the agent runs. argv: fake_ssh.py AGENT_CONFIG REQUESTED_COMMAND
    import os, subprocess, sys
    env = dict(os.environ, SSH_ORIGINAL_COMMAND=sys.argv[2])
    sys.exit(subprocess.run([sys.executable, "-I", os.environ["POOLED_AGENT_SCRIPT"], "agent", sys.argv[1]],
                            env=env).returncode)
''')

TAG = "pooled-000003-" + "a" * 32 + "-s001"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class AgentTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="pooled-agent-test-")).resolve()
        self.pins = self.root / "pins"
        self.scratch = self.root / "scratch"
        self.pins.mkdir()
        self.scratch.mkdir()
        self.collector = self.root / "collector.py"
        self.collector.write_text(FAKE_COLLECTOR)
        self.config_path = self.root / "agent.json"
        self.config_path.write_text(json.dumps({
            "schema": pc.AGENT_SCHEMA, "collector": str(self.collector),
            "collector_sha256": pc.sha256_file(self.collector), "scratch": str(self.scratch),
            "pin_roots": [str(self.pins)], "prefix": [sys.executable]}))

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def serve(self, op=None, payload=b"", raw=None, **args):
        encoded = raw if raw is not None else pc.encode_request(op, **args)
        stdout, stderr = io.BytesIO(), io.StringIO()
        code = pc.agent_main(str(self.config_path), {"SSH_ORIGINAL_COMMAND": encoded}, io.BytesIO(payload),
                             stdout, stderr)
        return code, stdout.getvalue(), stderr.getvalue()

    def assertRefused(self, result, words):
        code, out, err = result
        self.assertEqual(code, 2, err)
        self.assertIn("refused", err)
        self.assertIn(words, err)
        self.assertEqual(out, b"")

    # ------------------------------------------------------------ refusals

    def test_an_operation_outside_the_seven_is_refused(self):
        self.assertRefused(self.serve("shell", command="id"), "operation not served")
        self.assertRefused(self.serve("pin_status", pins=[], extra=1), "takes exactly")

    def test_a_path_outside_the_pin_roots_is_refused(self):
        data = b"bytes"
        outside = str(self.root / "elsewhere.bin")
        self.assertRefused(self.serve("push", data, path=outside, sha256=sha(data)), "outside the pin roots")
        escaping = str(self.pins) + os.sep + ".." + os.sep + "escape.bin"
        self.assertRefused(self.serve("push", data, path=escaping, sha256=sha(data)), "outside the pin roots")
        self.assertRefused(self.serve("remove_pin", path=str(self.config_path), sha256="0" * 64),
                           "outside the pin roots")
        self.assertFalse((self.root / "elsewhere.bin").exists())
        self.assertTrue(self.config_path.exists())

    @unittest.skipUnless(os.name == "posix", "symlinks need privileges on Windows")
    def test_a_path_through_a_symlink_is_refused(self):
        os.symlink(self.root, self.pins / "link")
        data = b"bytes"
        self.assertRefused(self.serve("push", data, path=str(self.pins / "link" / "x.bin"), sha256=sha(data)),
                           "outside the pin roots")

    def test_a_bad_tag_is_refused(self):
        for tag in ("../etc", "pooled-1", TAG + "/x", TAG.replace("-s001", "-s1"), ""):
            self.assertRefused(self.serve("remove_shard", tag=tag), "bad tag")

    def test_oversized_or_malformed_requests_are_refused(self):
        self.assertRefused(self.serve(raw="A" * (pc.AGENT_REQUEST_BYTES + 1)), "oversized")
        self.assertRefused(self.serve(raw=""), "missing")
        self.assertRefused(self.serve(raw="not base64!"), "base64url")
        bad_json = base64.urlsafe_b64encode(b"{not json").decode().rstrip("=")
        self.assertRefused(self.serve(raw=bad_json), "malformed")
        wrong_schema = base64.urlsafe_b64encode(json.dumps({"schema": "x", "op": "pin_status", "args": {}})
                                                .encode()).decode().rstrip("=")
        self.assertRefused(self.serve(raw=wrong_schema), "request/v1")
        self.assertRefused(self.serve("pin_status", pins="x"), "list")
        big = b"x" * (pc.AGENT_SHARD_BYTES + 1)
        self.assertRefused(self.serve("put_shard", big, tag=TAG), "exceeds")

    def test_a_second_request_on_one_connection_is_refused(self):
        one = json.dumps({"schema": pc.AGENT_REQUEST_SCHEMA, "op": "pin_status", "args": {"pins": []}})
        two = base64.urlsafe_b64encode((one + one).encode()).decode().rstrip("=")
        self.assertRefused(self.serve(raw=two), "malformed")
        trailing = pc.encode_request("remove_shard", tag=TAG).encode()
        self.assertRefused(self.serve("pin_status", trailing, pins=[]), "one request per connection")

    def test_a_shard_command_for_another_directory_or_mode_is_refused(self):
        bad = {"mode": "collect_parallel", "shard": {"first_slot": 0}, "output_directory": str(self.root / "x")}
        self.assertRefused(self.serve("put_shard", json.dumps(bad).encode(), tag=TAG), "own scratch")
        bad = {"mode": "update", "shard": {}, "output_directory": str(self.scratch / TAG)}
        self.assertRefused(self.serve("put_shard", json.dumps(bad).encode(), tag=TAG), "own scratch")

    def test_a_changed_collector_is_refused(self):
        self.put_shard()
        self.collector.write_text(FAKE_COLLECTOR + "\n# changed\n")
        self.assertRefused(self.serve("run_shard", tag=TAG), "collector binary differs")

    # ------------------------------------------------------------ served operations

    def put_shard(self):
        command = {"mode": "collect_parallel", "shard": {"first_slot": 2, "schedule_episodes": 4},
                   "episodes": [{"id": "e2", "seed": 7}, {"id": "e3", "seed": 8}],
                   "output_directory": str(self.scratch / TAG)}
        code, out, err = self.serve("put_shard", json.dumps(command).encode(), tag=TAG)
        self.assertEqual((code, json.loads(out)), (0, "stored"), err)

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

    def test_a_shard_is_stored_run_fetched_and_removed(self):
        self.put_shard()
        code, out, err = self.serve("run_shard", tag=TAG)
        self.assertEqual((code, json.loads(out)["code"]), (0, 0), err)
        code, tar_bytes, err = self.serve("fetch_shard", tag=TAG)
        self.assertEqual(code, 0, err)
        import tarfile
        with tarfile.open(fileobj=io.BytesIO(tar_bytes)) as archive:
            self.assertEqual(sorted(archive.getnames()), ["collection.json", "episode-0002.json", "episode-0003.json"])
        self.assertEqual(json.loads(self.serve("remove_shard", tag=TAG)[1]), "removed")
        self.assertEqual(os.listdir(self.scratch), [])

    # ------------------------------------------------------------ the dispatcher in agent mode

    def test_the_dispatcher_collects_through_the_agent_and_merges_the_same_bytes(self):
        sys.path.insert(0, str(HERE / "tests"))
        from test_pooled_collect import fake_merge, episode_bytes  # noqa: E402
        fake_ssh = self.root / "fake_ssh.py"
        fake_ssh.write_text(FAKE_SSH)
        os.environ["POOLED_AGENT_SCRIPT"] = str(HERE / "pooled_collect.py")
        checkpoint = self.root / "learner" / "checkpoint.json"
        checkpoint.parent.mkdir()
        checkpoint.write_bytes(b"policy")
        # The learner's pinned input lives under the agent's pin root, as the namespace requires.
        pinned = self.pins / "run" / "checkpoint.json"
        pinned.parent.mkdir()
        shutil.copyfile(checkpoint, pinned)
        episodes = [{"id": f"e{i}", "seed": 100 + i} for i in range(9)]
        command = {"mode": "collect_parallel", "workers": 2, "episodes": episodes,
                   "source": {"checkpoint": {"path": str(pinned), "sha256": pc.sha256_file(pinned)}},
                   "output_directory": "unused"}
        attempt = self.root / "run" / "iterations" / "000003" / "attempt-000000"
        attempt.mkdir(parents=True)
        command_path = attempt / "collect-command.json"
        command_path.write_text(json.dumps(command))
        host = pc.Host(name="agent-host", kind="ssh", collector=str(self.collector),
                       collector_sha256=pc.sha256_file(self.collector), workers=2, weight=1.0,
                       scratch=str(self.scratch), arch="x86", ssh=[sys.executable, str(fake_ssh), str(self.config_path)],
                       prefix=[sys.executable], transport="agent")
        config = {"hosts": [host], "audit": {"every": 1, "episodes": 2}}
        receipt = pc.Dispatcher(config).collect(command_path, fake_merge)
        self.assertEqual(receipt["merged"]["slots"], [episode_bytes(e).decode() for e in episodes])
        self.assertEqual(receipt["failed_hosts"], [])
        self.assertEqual(os.listdir(self.scratch), [], "every shard and command file removed")


if __name__ == "__main__":
    unittest.main()
