"""The collector agent behind a forced ssh command: confinement and the agent-mode dispatcher.
The agent serves exactly one of seven operations per connection and refuses everything else.
Kernel confinement (Landlock) is exercised for real where the kernel has it (LandlockEnforcementTests,
in separate processes, since confinement is irreversible); in-process tests run with "sandbox": "none"."""
import base64
import hashlib
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
import types
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE / "tests"))
import pooled_collect as pc  # noqa: E402
from test_pooled_collect import episode_bytes, fake_merge  # noqa: E402

LANDLOCK = pc.landlock_abi()

FAKE_COLLECTOR = textwrap.dedent('''
    import hashlib, json, os, subprocess, sys, time
    assert sys.argv[1] == "--execute-collect-shard"
    command = json.load(open(sys.argv[2]))
    out = command["output_directory"]
    first = command["episodes"][0] if command["episodes"] else {}
    if first.get("id") == "sleep":  # a collector with a descendant, past its deadline
        os.makedirs(out)
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(120)"])
        with open(os.path.join(out, "pids"), "w") as handle:
            handle.write("%d %d" % (os.getpid(), child.pid))
        time.sleep(120)
    if first.get("id") == "probe":  # records which accesses its sandbox allows
        os.makedirs(out)
        results = {}
        for kind, path in first["probes"]:
            try:
                if kind == "read":
                    open(path, "rb").read(1)
                else:
                    open(path, "wb").write(b"x")
                results[kind + ":" + path] = "allowed"
            except OSError as error:
                results[kind + ":" + path] = error.errno
        open(os.path.join(out, "probe.json"), "w").write(json.dumps(results))
        sys.exit(0)
    if first.get("id") == "noisy":
        sys.stderr.write("x" * (1 << 20) + "END")
        sys.stderr.flush()
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


def gone(pid: int, seconds: float = 10.0) -> bool:
    """True once `pid` no longer runs (absent, or a zombie awaiting its reaper). Linux /proc."""
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            state = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[0]
        except (FileNotFoundError, ProcessLookupError):
            return True
        if state == "Z":
            return True
        time.sleep(0.1)
    return False


class SlowStdin:
    """A connection that holds its end-of-input back for a while (a caller stalling the request)."""

    def __init__(self, seconds):
        self.seconds = seconds

    def read(self, n=-1):
        time.sleep(self.seconds)
        return b""


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
                  "pin_roots": [str(self.pins)], "prefix": [sys.executable], "max_seconds": 120,
                  "min_free_bytes": 0,  # the floor's own tests set it; the others never depend on this disk
                  "sandbox": "none"}  # in-process: confining the test runner itself is irreversible
        config.update(changes)
        path.write_text(json.dumps(config))
        return path

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def serve(self, op=None, payload=b"", raw=None, config=None, stdin=None, **args):
        encoded = raw if raw is not None else pc.encode_request(op, **args)
        stdout, stderr = io.BytesIO(), io.StringIO()
        code = pc.agent_main(str(config or self.config_path), {"SSH_ORIGINAL_COMMAND": encoded},
                             stdin or io.BytesIO(payload), stdout, stderr)
        return code, stdout.getvalue(), stderr.getvalue()

    def agent_process(self, config, op, payload=b"", **args):
        """The agent as its own process (as sshd runs it), so its sandbox confines only it."""
        env = dict(os.environ, SSH_ORIGINAL_COMMAND=pc.encode_request(op, **args))
        result = subprocess.run([sys.executable, "-I", str(HERE / "pooled_collect.py"), "agent", str(config)],
                                env=env, input=payload, capture_output=True, timeout=120)
        return result.returncode, result.stdout, result.stderr.decode(errors="replace")

    def assertRefused(self, result, words):
        code, out, err = result
        self.assertEqual(code, 2, err)
        self.assertIn("refused", err)
        self.assertIn(words, err)

    def shard_command(self, episodes=None, **changes):
        command = {"mode": "collect_parallel", "shard": {"first_slot": 2, "schedule_episodes": 4}, "workers": 2,
                   "episodes": episodes or [{"id": "e2", "seed": 7}, {"id": "e3", "seed": 8}],
                   "output_directory": str(self.scratch) + "/" + TAG}
        command.update(changes)
        return command

    def put_shard(self, **changes):
        code, out, err = self.serve("put_shard", json.dumps(self.shard_command(**changes)).encode(), tag=TAG)
        self.assertEqual(code, 0, err)
        self.assertEqual(json.loads(out), "stored")

    def run_shard(self, seconds=60, collector_sha256=None, **kwargs):
        return self.serve("run_shard", tag=TAG, seconds=seconds,
                          collector_sha256=collector_sha256 or pc.sha256_file(self.collector), **kwargs)


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
    def test_static_symlinks_are_refused_by_the_userspace_checks(self):
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

    def test_shard_commands_naming_files_outside_the_roots_are_refused(self):
        bad = self.shard_command(output_directory=str(self.root / "x"))
        self.assertRefused(self.serve("put_shard", json.dumps(bad).encode(), tag=TAG), "own scratch")
        sneaky = self.shard_command(output_directory=str(self.scratch) + "/link/../" + TAG)
        self.assertRefused(self.serve("put_shard", json.dumps(sneaky).encode(), tag=TAG), "own scratch")
        self.assertRefused(self.serve("put_shard", json.dumps(self.shard_command(mode="update")).encode(), tag=TAG),
                           "own scratch")
        for changes in ({"ex1": {"run": "r", "update": 0, "budget": 1}}, {"workers": 4096}, {"workers": True},
                        {"episodes": [{"id": "e", "seed": 1}] * 1025}):
            self.assertRefused(self.serve("put_shard", json.dumps(self.shard_command(**changes)).encode(), tag=TAG),
                               "no EX1 hook")
        outside = self.shard_command(source={"checkpoint": {"path": str(self.root / "agent.json"),
                                                             "sha256": "0" * 64}})
        self.assertRefused(self.serve("put_shard", json.dumps(outside).encode(), tag=TAG), "outside the pin roots")
        imp = self.pins / "import.json"
        imp.write_text(json.dumps({"export_directory": str(self.root), "source_run_path": str(self.pins / "r"),
                                   "source_registry_path": str(self.pins / "g")}))
        nested = self.shard_command(source={"play_import": {"path": str(imp), "sha256": pc.sha256_file(imp)}})
        self.assertRefused(self.serve("put_shard", json.dumps(nested).encode(), tag=TAG), "outside the pin roots")
        # Transitively: an import inside the roots whose run file is itself an import naming an outside file.
        inner = self.pins / "inner.json"
        inner.write_text(json.dumps({"export_directory": str(self.root), "source_run_path": str(self.pins / "r"),
                                     "source_registry_path": str(self.pins / "g")}))
        imp.write_text(json.dumps({"export_directory": str(self.pins / "e"), "source_run_path": str(inner),
                                   "source_registry_path": str(self.pins / "g")}))
        nested = self.shard_command(source={"play_import": {"path": str(imp), "sha256": pc.sha256_file(imp)}})
        self.assertRefused(self.serve("put_shard", json.dumps(nested).encode(), tag=TAG), "outside the pin roots")

    def test_the_commands_own_pins_must_be_present_with_their_pinned_bytes(self):
        absent = self.shard_command(source={"checkpoint": {"path": str(self.pins / "absent.json"),
                                                            "sha256": "0" * 64}})
        self.assertRefused(self.serve("put_shard", json.dumps(absent).encode(), tag=TAG), "pinned input absent")
        held = self.pins / "checkpoint.json"
        held.write_bytes(b"benign")
        other = self.shard_command(source={"checkpoint": {"path": str(held), "sha256": sha(b"malicious")}})
        self.assertRefused(self.serve("put_shard", json.dumps(other).encode(), tag=TAG), "differs from its sha256")
        # The stored command is checked again just before it runs: bytes changed since are refused.
        self.put_shard(source={"checkpoint": {"path": str(held), "sha256": sha(b"benign")}})
        held.unlink()
        held.write_bytes(b"malicious")
        self.assertRefused(self.run_shard(), "differs from its sha256")
        self.assertFalse((self.scratch / TAG).exists(), "nothing ran")

    def test_provenance_pins_inside_a_pinned_record_are_neither_followed_nor_refused(self):
        record = self.pins / "fresh-init.json"
        record.write_text(json.dumps({"producer": {"source_files": [
            {"path": "python/x.py", "sha256": "1" * 64},
            {"path": "C:/Python/python.exe", "sha256": "2" * 64},
            {"path": str(self.root / "agent.json"), "sha256": "3" * 64}]}}))
        command = self.shard_command(source={"checkpoint": {"path": str(record), "sha256": pc.sha256_file(record)}})
        code, out, err = self.serve("put_shard", json.dumps(command).encode(), tag=TAG)
        self.assertEqual(code, 0, err)

    def test_deeply_nested_shard_commands_and_pinned_documents_are_refused(self):
        deep = json.loads("[" * 300 + "]" * 300)
        command = self.shard_command(unused=deep)
        self.assertRefused(self.serve("put_shard", json.dumps(command).encode(), tag=TAG), "nests deeper")
        record = self.pins / "deep.json"
        record.write_text(json.dumps({"history": deep}))
        command = self.shard_command(source={"checkpoint": {"path": str(record), "sha256": pc.sha256_file(record)}})
        self.assertRefused(self.serve("put_shard", json.dumps(command).encode(), tag=TAG), "nests deeper")
        with self.assertRaises(pc.PinTreeTooLarge):
            pc.pins_of([[1, 2], [3, [4]]], max_nodes=5)

    def test_the_learner_and_the_agent_must_expect_the_same_collector(self):
        self.put_shard()
        self.assertRefused(self.run_shard(collector_sha256="0" * 64), "another collector")
        self.assertRefused(self.run_shard(seconds=0), "seconds")
        self.assertRefused(self.run_shard(seconds=True), "seconds")
        configured = json.loads(self.config_path.read_text())["collector_sha256"]
        self.collector.write_text(FAKE_COLLECTOR + "\n# changed\n")
        self.assertRefused(self.run_shard(collector_sha256=configured), "collector binary differs")

    def test_a_hard_linked_output_is_never_fetched(self):
        self.put_shard()
        self.assertEqual(json.loads(self.run_shard()[1])["code"], 0)
        secret = self.root / "secret.txt"
        secret.write_text("not for the learner")
        os.link(secret, self.scratch / TAG / "out" / "leak.txt")
        self.assertRefused(self.serve("fetch_shard", tag=TAG), "singly linked")

    def test_a_run_never_reuses_an_existing_output(self):
        self.put_shard()
        (self.scratch / TAG).mkdir()
        self.assertRefused(self.run_shard(), "already exists")

    def test_the_config_comes_only_from_the_forced_command(self):
        evil_pins = self.root / "evil-pins"
        evil_pins.mkdir()
        evil = self.write_config(self.root / "evil.json", pin_roots=[str(evil_pins)])
        data = b"bytes"
        target = str(evil_pins / "x.bin")
        self.assertRefused(self.serve("push", data, path=target, sha256=sha(data), config_path=str(evil)),
                           "takes exactly")
        with_config = json.dumps({"schema": pc.AGENT_REQUEST_SCHEMA, "op": "pin_status", "args": {"pins": []},
                                  "config": str(evil)})
        self.assertRefused(self.serve(raw=base64.urlsafe_b64encode(with_config.encode()).decode().rstrip("=")),
                           "request/v1 object")
        self.assertRefused(self.serve(raw=f"agent {evil}"), "base64url")
        # The forced command's process: neither the request nor the environment selects another config.
        env = dict(os.environ, SSH_ORIGINAL_COMMAND=pc.encode_request("push", path=target, sha256=sha(data)),
                   MTG_POOLED_COLLECT_CONFIG=str(evil))
        result = subprocess.run([sys.executable, "-I", str(HERE / "pooled_collect.py"), "agent",
                                 str(self.config_path)], env=env, input=data, capture_output=True, timeout=120)
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn(b"outside the pin roots", result.stderr)
        self.assertEqual(list(evil_pins.iterdir()), [])

    def test_non_ascii_paths_are_refused(self):
        data = b"bytes"
        for name in ("café.bin", "tab\there.bin", "nul\x00.bin"):
            self.assertRefused(self.serve("push", data, path=str(self.pins) + "/" + name, sha256=sha(data)),
                               "outside the pin roots")
        self.assertEqual(list(self.pins.iterdir()), [])

    def test_programs_come_from_the_config_without_path_lookup(self):
        relative = self.write_config(self.root / "relative.json", prefix=["python3"])
        code, _, err = self.serve("pin_status", pins=[], config=relative)
        self.assertEqual(code, 1)
        self.assertIn("absolute program", err)
        inside = self.write_config(self.root / "run-inside.json", run_paths=[str(self.pins)])
        code, _, err = self.serve("pin_status", pins=[], config=inside)
        self.assertEqual(code, 1)
        self.assertIn("run_paths", err)

    @unittest.skipUnless(os.name == "posix", "symlinks need privileges on Windows")
    def test_symlinks_anywhere_in_a_declared_path_are_refused(self):
        data = b"bytes"
        secret = self.root / "secret.txt"
        secret.write_bytes(data)
        os.symlink(secret, self.pins / "final-link.bin")  # the final component is a symlink
        self.assertRefused(self.serve("push", data, path=str(self.pins / "final-link.bin"), sha256=sha(data)),
                           "outside the pin roots")
        self.assertRefused(self.serve("pin_status", pins=[[str(self.pins / "final-link.bin"), sha(data)]]),
                           "outside the pin roots")
        self.assertRefused(self.serve("remove_pin", path=str(self.pins / "final-link.bin"), sha256=sha(data)),
                           "outside the pin roots")
        self.assertTrue(secret.exists())
        # A symlinked root in the collector's own config is refused before any request is served.
        os.symlink(self.pins, self.root / "pins-link")
        linked_root = self.write_config(self.root / "linked.json", pin_roots=[str(self.root / "pins-link")])
        code, _, err = self.serve("pin_status", pins=[], config=linked_root)
        self.assertEqual(code, 1)
        self.assertIn("canonical", err)

    @unittest.skipUnless(os.name == "posix", "symlinks need privileges on Windows")
    def test_symlinks_in_a_shard_output_are_never_fetched_or_followed(self):
        secret = self.root / "secret.txt"
        secret.write_bytes(b"not for the learner")
        # A symlink member, or a symlink in place of the run directory, is never fetched.
        self.put_shard()
        self.assertEqual(json.loads(self.run_shard()[1])["code"], 0)
        os.symlink(secret, self.scratch / TAG / "out" / "leak.json")
        self.assertRefused(self.serve("fetch_shard", tag=TAG), "singly linked")
        shutil.rmtree(self.scratch / TAG)
        (self.root / "planted" / "out").mkdir(parents=True)
        (self.root / "planted" / "out" / "x.json").write_bytes(b"planted")
        os.symlink(self.root / "planted", self.scratch / TAG)
        self.assertRefused(self.serve("fetch_shard", tag=TAG), "not a directory")
        self.assertEqual(json.loads(self.serve("remove_shard", tag=TAG)[1]), "removed")
        self.assertTrue((self.root / "planted" / "out" / "x.json").exists(), "removal never follows the link")
        self.assertFalse(os.path.lexists(self.scratch / TAG))

    def test_overlapping_or_containing_roots_are_refused(self):
        overlapping = self.write_config(self.root / "overlap.json", pin_roots=[str(self.root)])
        self.assertEqual(self.serve("pin_status", pins=[], config=overlapping)[0], 1)
        inside = self.write_config(self.root / "inside.json", collector=str(self.pins / "collector.py"))
        self.assertEqual(self.serve("pin_status", pins=[], config=inside)[0], 1)


class SandboxConfigTests(AgentCase):
    """The "sandbox" key on every platform; the kernel calls are stubbed (real ones: below)."""

    def test_the_sandbox_defaults_to_landlock_and_takes_no_other_value(self):
        config = json.loads(self.config_path.read_text())
        del config["sandbox"]
        self.config_path.write_text(json.dumps(config))
        self.assertEqual(pc.load_agent_config(self.config_path)["sandbox"], "landlock")
        other = self.write_config(self.root / "other.json", sandbox="seccomp")
        code, _, err = self.serve("pin_status", pins=[], config=other)
        self.assertEqual(code, 1)
        self.assertIn("sandbox", err)

    def test_landlock_configured_but_unavailable_refuses_every_request(self):
        config = self.write_config(self.root / "landlock.json", sandbox="landlock")
        data = b"bytes"
        with mock.patch.object(pc, "landlock_abi", return_value=0):
            for op, payload, args in (("pin_status", b"", {"pins": []}),
                                      ("push", data, {"path": str(self.pins / "x.bin"), "sha256": sha(data)}),
                                      ("put_shard", json.dumps(self.shard_command()).encode(), {"tag": TAG}),
                                      ("remove_shard", b"", {"tag": TAG})):
                self.assertRefused(self.serve(op, payload, config=config, **args), "has no Landlock")
        self.assertEqual(list(self.pins.iterdir()) + list(self.scratch.iterdir()), [], "nothing served")

    def test_the_explicit_opt_out_is_reported(self):
        code, out, err = self.serve("pin_status", pins=[])
        self.assertEqual((code, json.loads(out)["sandbox"]), (0, "none"), err)

    def test_landlock_confines_the_agent_to_its_roots_before_serving(self):
        config = self.write_config(self.root / "landlock.json", sandbox="landlock")
        calls = []

        def ruleset(rules, abi):
            calls.append(("ruleset", rules, abi))
            return os.open(os.devnull, os.O_RDONLY)

        with mock.patch.object(pc, "landlock_abi", return_value=6), \
                mock.patch.object(pc, "landlock_ruleset", side_effect=ruleset), \
                mock.patch.object(pc, "landlock_restrict", side_effect=lambda fd: calls.append(("restrict",))):
            code, out, err = self.serve("pin_status", pins=[], config=config)
        self.assertEqual((code, json.loads(out)["sandbox"]), (0, "landlock-abi6"), err)
        self.assertEqual([c[0] for c in calls], ["ruleset", "restrict"])
        rules = {path: access for path, access, _ in calls[0][1]}
        writable = {path for path, access in rules.items() if access & pc.LL_WRITE_FILE}
        self.assertEqual(writable, {str(self.pins), str(self.scratch), "/dev/null"})
        self.assertFalse(any(access & (pc.LL_MAKE_SYM | pc.LL_MAKE_CHAR | pc.LL_MAKE_BLOCK | pc.LL_MAKE_FIFO
                                       | pc.LL_MAKE_SOCK) for access in rules.values()), "no special nodes")
        self.assertEqual(rules[str(self.collector)], pc.LL_RUN)
        self.assertNotIn(str(self.config_path), rules)
        self.assertNotIn(str(self.root), rules)

    def test_the_collector_writes_only_its_run_directory(self):
        config = pc.load_agent_config(self.config_path)
        run_dir, command_file = str(self.scratch / TAG), str(self.scratch / ".run" / "c.json")
        rules = {path: access for path, access, _ in pc.collector_rules(config, command_file, run_dir)}
        writable = {path for path, access in rules.items() if access & (pc.LL_WRITE_FILE | pc.LL_MAKE_DIR)}
        self.assertEqual(writable, {run_dir, "/dev/null"})
        self.assertEqual(rules[str(self.pins)], pc.LL_READ, "pin roots read-only")
        self.assertEqual(rules[command_file], pc.LL_READ_FILE)
        self.assertNotIn(str(self.scratch), rules, "nothing else of the scratch root")


@unittest.skipUnless(LANDLOCK >= 1, "the kernel has no Landlock (Windows, or not enabled)")
class LandlockEnforcementTests(AgentCase):
    """Real kernel confinement, each in its own process."""

    def setUp(self):
        super().setUp()
        self.secret = self.root / "outside" / "secret.txt"
        self.secret.parent.mkdir()
        self.secret.write_bytes(b"not for the key holder")
        os.symlink(self.secret.parent, self.pins / "dirlink")  # a namespace race already won
        self.held = self.pins / "held.bin"
        self.held.write_bytes(b"pinned")
        self.landlock = self.write_config(self.root / "landlock.json", sandbox="landlock")

    def test_the_confined_agent_reaches_nothing_outside_even_where_the_userspace_checks_would_accept(self):
        script = self.root / "probe_agent.py"
        script.write_text(textwrap.dedent(f'''
            import io, json, os, sys
            sys.path.insert(0, {str(HERE)!r})
            import pooled_collect as pc
            # Userspace checks switched off: only the kernel stands between a request and the disk.
            pc._canonical = lambda path: path
            pc._strictly_under = lambda path, roots: True

            def serve(op, payload=b"", **args):
                out, err = io.BytesIO(), io.StringIO()
                code = pc.agent_main({str(self.landlock)!r}, {{"SSH_ORIGINAL_COMMAND": pc.encode_request(op, **args)}},
                                     io.BytesIO(payload), out, err)
                return code, out.getvalue().decode(), err.getvalue()

            report = {{}}
            secret, linked = {str(self.secret)!r}, {str(self.pins / "dirlink" / "secret.txt")!r}
            digest = pc.sha256_file(pc.Path(secret))
            report["status"] = serve("pin_status", pins=[[secret, digest], [linked, digest],
                                                         [{str(self.held)!r}, pc.sha256_file(pc.Path({str(self.held)!r}))]])
            report["push_outside"] = serve("push", b"x", path={str(self.root / "planted" / "x.bin")!r},
                                           sha256=pc.hashlib.sha256(b"x").hexdigest())
            report["push_through_link"] = serve("push", b"y", path={str(self.pins / "dirlink" / "y.bin")!r},
                                                sha256=pc.hashlib.sha256(b"y").hexdigest())

            def attempt(name, action):
                try:
                    action()
                    report[name] = "allowed"
                except OSError as error:
                    report[name] = error.errno
            attempt("read_secret", lambda: open(secret, "rb").read())
            attempt("read_through_link", lambda: open(linked, "rb").read())
            attempt("write_outside", lambda: open({str(self.root / "outside" / "new.txt")!r}, "wb").write(b"x"))
            attempt("write_through_link", lambda: open({str(self.pins / "dirlink" / "new.txt")!r}, "wb").write(b"x"))
            attempt("read_config_dir", lambda: os.listdir({str(self.root)!r}))
            attempt("read_passwd", lambda: open("/etc/passwd", "rb").read())
            attempt("symlink_in_pins", lambda: os.symlink(secret, {str(self.pins / "sym")!r}))
            attempt("write_pin_root", lambda: open({str(self.pins / "ok.bin")!r}, "wb").write(b"x"))
            attempt("read_pin", lambda: open({str(self.held)!r}, "rb").read())
            print(json.dumps(report))
        '''))
        result = subprocess.run([sys.executable, "-I", str(script)], capture_output=True, text=True, timeout=120)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        code, out, err = report["status"]
        self.assertEqual(code, 0, err)
        self.assertEqual(json.loads(out)["states"], ["differs", "differs", "ok"], "denied reads never hash")
        self.assertTrue(json.loads(out)["sandbox"].startswith("landlock-abi"))
        self.assertEqual(report["push_outside"][0], 1, report["push_outside"])
        self.assertEqual(report["push_through_link"][0], 1, report["push_through_link"])
        self.assertFalse((self.root / "planted").exists())
        self.assertEqual(sorted(os.listdir(self.secret.parent)), ["secret.txt"], "nothing written outside")
        for name in ("read_secret", "read_through_link", "write_outside", "write_through_link",
                     "read_config_dir", "symlink_in_pins"):
            self.assertEqual(report[name], 13, f"{name}: {report[name]}")  # EACCES
        if os.path.exists("/etc/passwd"):
            self.assertEqual(report["read_passwd"], 13)
        self.assertEqual((report["write_pin_root"], report["read_pin"]), ("allowed", "allowed"))

    def test_the_confined_collector_writes_only_its_run_directory(self):
        other = self.scratch / ("pooled-000003-" + "b" * 32 + "-s002")
        other.mkdir()
        (other / "x.json").write_text("another shard")
        probes = [["read", str(self.secret)], ["read", str(self.pins / "dirlink" / "secret.txt")],
                  ["read", str(other / "x.json")], ["read", str(self.scratch / f"{TAG}.json")],
                  ["write", str(self.pins / "planted.bin")], ["write", str(self.scratch / "planted.json")],
                  ["write", str(other / "planted.json")], ["write", str(self.root / "outside" / "planted")],
                  ["read", str(self.held)], ["write", str(self.scratch / TAG / "out" / "mine.json")]]
        command = self.shard_command(episodes=[{"id": "probe", "seed": 1, "probes": probes}])
        code, out, err = self.agent_process(self.landlock, "put_shard", json.dumps(command).encode(), tag=TAG)
        self.assertEqual(code, 0, err)
        code, out, err = self.agent_process(self.landlock, "run_shard", tag=TAG, seconds=60,
                                            collector_sha256=pc.sha256_file(self.collector))
        self.assertEqual((code, json.loads(out)["code"]), (0, 0), (err, out))
        report = json.loads((self.scratch / TAG / "out" / "probe.json").read_text())
        allowed = {key for key, value in report.items() if value == "allowed"}
        self.assertEqual(allowed, {"read:" + str(self.held), "write:" + str(self.scratch / TAG / "out" / "mine.json")},
                         report)
        self.assertTrue(all(value in ("allowed", 13) for value in report.values()), report)
        for planted in (self.pins / "planted.bin", self.scratch / "planted.json", other / "planted.json"):
            self.assertFalse(planted.exists())

    def test_the_dispatcher_collects_through_confined_agents(self):
        AgentDispatcherTests.collect_with_failover(self, self.landlock)


class AgentServiceTests(AgentCase):
    def test_pins_install_once_never_replace_and_remove_only_the_owned_file(self):
        data, other = b"checkpoint", b"other bytes"
        path = str(self.pins / "run" / "checkpoint.json")
        self.assertEqual(json.loads(self.serve("pin_status", pins=[[path, sha(data)]])[1])["states"], ["absent"])
        self.assertEqual(json.loads(self.serve("push", data, path=path, sha256=sha(data))[1]), "created")
        witness = pc.owned_witness(path, str(self.scratch))
        self.assertTrue(os.path.samefile(witness, path), "the witness is the installed inode")
        self.assertEqual(os.listdir(os.path.dirname(witness)), [os.path.basename(witness)], "no pending link left")
        self.assertEqual(json.loads(self.serve("push", data, path=path, sha256=sha(data))[1]), "existing")
        self.assertRefused(self.serve("push", other, path=path, sha256=sha(other)), "existing pin differs")
        self.assertRefused(self.serve("push", other, path=path, sha256=sha(data)), "payload differs")
        self.assertEqual(json.loads(self.serve("pin_status", pins=[[path, sha(other)]])[1])["states"], ["differs"])
        self.assertEqual(json.loads(self.serve("remove_pin", path=path, sha256=sha(other))[1]), "not-owned")
        self.assertEqual(Path(path).read_bytes(), data, "put back")
        self.assertEqual(json.loads(self.serve("remove_pin", path=path, sha256=sha(data))[1]), "removed")
        self.assertEqual(sorted(os.listdir(self.pins / "run")), [])
        # A restored copy with the same bytes is another file: never removed.
        self.assertEqual(json.loads(self.serve("push", data, path=path, sha256=sha(data))[1]), "created")
        Path(path).unlink()
        Path(path).write_bytes(data)
        self.assertEqual(json.loads(self.serve("remove_pin", path=path, sha256=sha(data))[1]), "not-owned")
        self.assertEqual(Path(path).read_bytes(), data)
        self.assertEqual(os.listdir(self.scratch / ".pooled-owned"), [], "no witness or set-aside file left")

    def test_the_storage_marker_persists_and_is_regenerated_when_deleted(self):
        first = json.loads(self.serve("pin_status", pins=[])[1])["storage"]
        self.assertRegex(first, r"^[0-9a-f]{32}$")
        self.assertEqual(json.loads(self.serve("pin_status", pins=[])[1])["storage"], first)
        (self.scratch / ".pooled-storage-id").unlink()
        self.assertNotEqual(json.loads(self.serve("pin_status", pins=[])[1])["storage"], first)

    def test_concurrent_first_initialization_publishes_one_complete_marker(self):
        # An interrupted earlier publication left only its private stage: never a partial marker.
        (self.scratch / ".pooled-storage-id.0123.stage").write_bytes(b"")
        barrier, replies = threading.Barrier(12), []

        def first_contact():
            barrier.wait()
            replies.append(self.serve("pin_status", pins=[]))

        threads = [threading.Thread(target=first_contact) for _ in range(12)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join(60)
        self.assertEqual(len(replies), 12)
        self.assertTrue(all(code == 0 for code, _, _ in replies), [err for _, _, err in replies])
        markers = {json.loads(out)["storage"] for _, out, _ in replies}
        self.assertEqual(len(markers), 1, markers)
        self.assertRegex(markers.pop(), r"^[0-9a-f]{32}$")

    def test_remove_pin_waits_for_the_per_path_lock(self):
        data = b"checkpoint"
        path = str(self.pins / "run" / "checkpoint.json")
        self.serve("push", data, path=path, sha256=sha(data))
        config = pc.load_agent_config(self.config_path)
        done = threading.Event()

        def remove():
            self.serve("remove_pin", path=path, sha256=sha(data))
            done.set()

        with pc._lock(config, path):  # held as a push holds it while it links and witnesses
            worker = threading.Thread(target=remove)
            worker.start()
            self.assertFalse(done.wait(1.0), "remove_pin must wait for the path's lock")
        worker.join(10)
        self.assertTrue(done.is_set())
        self.assertFalse(Path(path).exists())

    def test_a_shard_is_stored_run_fetched_with_its_bytes_and_removed(self):
        self.put_shard()
        code, out, err = self.run_shard()
        self.assertEqual((code, json.loads(out)["code"]), (0, 0), err)
        code, tar_bytes, err = self.serve("fetch_shard", tag=TAG)
        self.assertEqual(code, 0, err)
        with tarfile.open(fileobj=io.BytesIO(tar_bytes)) as archive:
            files = {m.name: archive.extractfile(m).read() for m in archive}
        self.assertEqual(sorted(files), ["collection.json", "episode-0002.json", "episode-0003.json"])
        self.assertEqual((files["episode-0002.json"], files["episode-0003.json"]), (b"e2:7", b"e3:8"))
        pins = json.loads(files["collection.json"])["trajectories"]
        self.assertEqual([p["sha256"] for p in pins], [sha(b"e2:7"), sha(b"e3:8")])
        code_gz, gz_bytes, err_gz = self.serve("fetch_shard", tag=TAG, compression="gzip1")
        self.assertEqual(code_gz, 0, err_gz)
        self.assertTrue(gz_bytes.startswith(b"\x1f\x8b"))
        with tarfile.open(fileobj=io.BytesIO(gz_bytes), mode="r:gz") as archive:
            gz_files = {m.name: archive.extractfile(m).read() for m in archive}
        self.assertEqual(gz_files, files)
        self.assertRefused(self.serve("fetch_shard", tag=TAG, compression="bzip2"), "unsupported fetch_shard compression")
        self.assertEqual(json.loads(self.serve("remove_shard", tag=TAG)[1]), "removed")
        self.assertEqual([n for n in os.listdir(self.scratch) if not n.startswith(".")], [])
        self.assertEqual(os.listdir(self.scratch / ".run"), [], "the private command copy is gone")

    def test_collector_stderr_is_kept_only_as_a_bounded_tail(self):
        self.put_shard(episodes=[{"id": "noisy", "seed": 1}])
        code, out, err = self.run_shard()
        reply = json.loads(out)
        self.assertEqual((code, reply["code"]), (0, 0), err)
        self.assertLessEqual(len(reply["stderr"]), pc.AGENT_STDERR_TAIL)
        self.assertTrue(reply["stderr"].endswith("END"))

    @unittest.skipUnless(sys.platform.startswith("linux"), "process-group termination checked through /proc")
    def test_a_run_past_its_seconds_ends_its_whole_process_group(self):
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        code, out, err = self.run_shard(seconds=2)
        self.assertEqual((code, json.loads(out)["code"]), (0, "timeout"), err)
        leader, descendant = map(int, (self.scratch / TAG / "out" / "pids").read_text().split())
        self.assertTrue(gone(leader) and gone(descendant), "collector and its descendant killed")

    @unittest.skipUnless(sys.platform.startswith("linux"), "process-group termination checked through /proc")
    def test_the_request_deadline_inside_a_run_ends_its_whole_process_group(self):
        config = self.write_config(self.root / "deadline.json", request_seconds=1, max_seconds=3)
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        # The caller holds its end of input back for 2.5 s; the 4 s request alarm then fires while the
        # agent waits on the collector (whose own 3 s deadline lies beyond it).
        started = time.monotonic()
        result = self.run_shard(seconds=60, config=config, stdin=SlowStdin(2.5))
        self.assertRefused(result, "request deadline")
        self.assertLess(time.monotonic() - started, 5.4, "ended by the alarm, not the run deadline")
        leader, descendant = map(int, (self.scratch / TAG / "out" / "pids").read_text().split())
        self.assertTrue(gone(leader) and gone(descendant), "collector and its descendant killed")

    @unittest.skipUnless(sys.platform.startswith("linux"), "process-group termination checked through /proc")
    def test_an_alarm_due_during_launch_still_ends_the_whole_process_group(self):
        config = self.write_config(self.root / "launch.json", request_seconds=1, max_seconds=1)  # alarm at 2 s
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        real_popen = subprocess.Popen

        def slow_launch(*args, **kwargs):
            process = real_popen(*args, **kwargs)
            time.sleep(3)  # the request alarm comes due here, after the fork, before the guarded wait
            return process

        with mock.patch.object(pc.subprocess, "Popen", side_effect=slow_launch):
            result = self.run_shard(seconds=60, config=config)
        self.assertRefused(result, "request deadline")
        leader, descendant = map(int, (self.scratch / TAG / "out" / "pids").read_text().split())
        self.assertTrue(gone(leader) and gone(descendant), "collector and its descendant killed")

    def test_a_pin_whose_witness_cannot_be_linked_is_installed_unowned(self):
        data = b"checkpoint"
        path = str(self.pins / "run" / "checkpoint.json")
        real_link = os.link

        def no_witness(source, destination, *args, **kwargs):
            if str(destination).endswith(".pending"):
                raise OSError(18, "Invalid cross-device link")  # another filesystem, or Landlock ABI 1
            return real_link(source, destination, *args, **kwargs)

        with mock.patch.object(pc.os, "link", side_effect=no_witness):
            self.assertEqual(json.loads(self.serve("push", data, path=path, sha256=sha(data))[1]), "created-unowned")
        self.assertEqual(Path(path).read_bytes(), data)
        self.assertEqual(json.loads(self.serve("remove_pin", path=path, sha256=sha(data))[1]), "not-owned")
        self.assertEqual(Path(path).read_bytes(), data, "never pruned")
        self.assertEqual(os.listdir(self.scratch / ".pooled-owned"), [], "no witness or pending link left")

    @unittest.skipUnless(os.name == "posix", "process-group termination is POSIX")
    def test_a_request_beyond_the_agent_cap_runs_to_the_cap(self):
        config = json.loads(self.config_path.read_text())
        self.config_path.write_text(json.dumps(dict(config, max_seconds=1)))
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        started = time.time()
        code, out, err = self.run_shard(seconds=10 ** 6)
        self.assertEqual((code, json.loads(out)["code"]), (0, "timeout"), err)
        self.assertLess(time.time() - started, 30)


def fake_statvfs(free):
    """A statvfs stand-in reporting `free` bytes (a callable gives a value per call)."""
    def statvfs(path):
        return types.SimpleNamespace(f_bavail=free() if callable(free) else free, f_frsize=1)
    return statvfs


def fake_statvfs_by_path(free_for):
    """A statvfs stand-in whose free bytes depend on the path measured (one filesystem per rule)."""
    def statvfs(path):
        return types.SimpleNamespace(f_bavail=free_for(str(path)), f_frsize=1)
    return statvfs


class ResourceBoundTests(AgentCase):
    """The disk floor (min_free_bytes) and the run slots (max_concurrent_runs)."""

    TAG2 = TAG.replace("-s001", "-s002")

    def floor_config(self, floor=1000, **changes):
        return self.write_config(self.root / "floor.json", min_free_bytes=floor, **changes)

    def test_the_disk_floor_refuses_push_put_shard_and_run_shard(self):
        config, data = self.floor_config(), b"checkpoint"
        with mock.patch.object(pc.os, "statvfs", fake_statvfs(999), create=True):
            self.assertRefused(self.serve("push", data, path=str(self.pins / "c.bin"), sha256=sha(data),
                                          config=config), "disk floor")
            self.assertRefused(self.serve("put_shard", json.dumps(self.shard_command()).encode(), tag=TAG,
                                          config=config), "disk floor")
        self.assertEqual(list(self.pins.iterdir()), [], "nothing staged or installed")
        with mock.patch.object(pc.os, "statvfs", fake_statvfs(10 ** 6), create=True):
            code, out, err = self.serve("put_shard", json.dumps(self.shard_command()).encode(), tag=TAG,
                                        config=config)
            self.assertEqual(code, 0, err)
        with mock.patch.object(pc.os, "statvfs", fake_statvfs(999), create=True):
            self.assertRefused(self.run_shard(config=config), "disk floor")
        self.assertFalse((self.scratch / TAG).exists(), "nothing ran")

    def test_a_push_stops_once_free_space_falls_under_the_floor(self):
        # Two admission checks (scratch, target), then the stage file's checks: after the first block,
        # every interval, and at the end. Space runs out at each of those transitions in turn.
        config, data, mib = self.floor_config(), os.urandom(3 << 20), 1 << 20
        path = self.pins / "run" / "c.bin"
        for passing, interval, consumed in ((2, mib, mib),            # after the first block
                                            (3, mib, 2 * mib),        # at the next interval
                                            (3, 64 * mib, 3 * mib)):  # at the end (interval never reached)
            calls = iter([10 ** 6] * passing + [999] * 100)
            stdin = io.BytesIO(data)
            with mock.patch.object(pc, "FLOOR_CHECK_BYTES", interval), \
                    mock.patch.object(pc.os, "statvfs", fake_statvfs(lambda: next(calls)), create=True):
                self.assertRefused(self.serve("push", path=str(path), sha256=sha(data), config=config,
                                              stdin=stdin), "disk floor")
            self.assertEqual(stdin.tell(), consumed, (passing, interval))
            self.assertEqual(os.listdir(path.parent), [], "the stage is removed and no pin installed")

    def test_the_floor_measures_the_filesystem_the_bytes_land_on(self):
        config, data = self.floor_config(), b"checkpoint"
        mounted = self.pins / "mounted"  # another, full filesystem below the pin root
        mounted.mkdir()
        full = fake_statvfs_by_path(lambda path: 999 if path.startswith(str(mounted)) else 10 ** 6)
        with mock.patch.object(pc.os, "statvfs", full, create=True):
            self.assertRefused(self.serve("push", data, path=str(mounted / "deep" / "c.bin"), sha256=sha(data),
                                          config=config), "disk floor")
            code, out, err = self.serve("push", data, path=str(self.pins / "other" / "c.bin"), sha256=sha(data),
                                        config=config)
            self.assertEqual((code, json.loads(out)), (0, "created"), err)
        self.assertFalse((mounted / "deep").exists(), "refused before any directory was made there")
        # A small payload is checked on the stage file's own filesystem once complete.
        stage_full = fake_statvfs_by_path(lambda path: 999 if path.endswith(".stage") else 10 ** 6)
        with mock.patch.object(pc.os, "statvfs", stage_full, create=True):
            self.assertRefused(self.serve("push", data, path=str(self.pins / "other" / "d.bin"), sha256=sha(data),
                                          config=config), "disk floor")
        self.assertEqual(sorted(os.listdir(self.pins / "other")), ["c.bin"])
        # put_shard and run_shard measure the pin roots as well as scratch.
        roots_full = fake_statvfs_by_path(lambda path: 999 if path == str(self.pins) else 10 ** 6)
        with mock.patch.object(pc.os, "statvfs", roots_full, create=True):
            self.assertRefused(self.serve("put_shard", json.dumps(self.shard_command()).encode(), tag=TAG,
                                          config=config), "disk floor")
        self.put_shard()
        with mock.patch.object(pc.os, "statvfs", roots_full, create=True):
            self.assertRefused(self.run_shard(config=config), "disk floor")
        # The private command copy's own directory is measured too.
        run_copy_full = fake_statvfs_by_path(lambda path: 999 if path.endswith(".run") else 10 ** 6)
        with mock.patch.object(pc.os, "statvfs", run_copy_full, create=True):
            self.assertRefused(self.run_shard(config=config), "disk floor")
        self.assertEqual(os.listdir(self.scratch / ".run"), [], "no private copy left behind")

    def test_a_command_too_large_once_serialized_leaves_no_file(self):
        command = self.shard_command(pad="é" * (3 << 20))  # about 6 MiB as UTF-8, 18 MiB once escaped
        code, out, err = self.serve("put_shard", json.dumps(command, ensure_ascii=False).encode(), tag=TAG)
        self.assertEqual(code, 0, err)
        self.assertRefused(self.run_shard(), "once serialized")
        self.assertEqual(os.listdir(self.scratch / ".run"), [])

    def test_removal_requests_never_accumulate_lock_files(self):
        for i in range(300):
            code, out, err = self.serve("remove_pin", path=str(self.pins / f"absent-{i}.bin"), sha256="0" * 64)
            self.assertEqual((code, json.loads(out)), (0, "not-owned"), err)
        self.assertLessEqual(len(os.listdir(self.scratch / ".locks")), 256, "a fixed set of striped locks")

    def test_the_floor_defaults_to_20_gib_and_zero_disables_it(self):
        document = json.loads(self.config_path.read_text())
        del document["min_free_bytes"]
        self.config_path.write_text(json.dumps(document))
        self.assertEqual(pc.load_agent_config(self.config_path)["min_free_bytes"], 20 * 1024 ** 3)
        self.assertEqual(pc.load_agent_config(self.config_path)["max_concurrent_runs"], 2)
        for bad in (-1, True, "1"):
            code, _, err = self.serve("pin_status", pins=[], config=self.floor_config(floor=bad))
            self.assertEqual(code, 1)
            self.assertIn("min_free_bytes", err)
        data = b"checkpoint"
        with mock.patch.object(pc.os, "statvfs", fake_statvfs(0), create=True):
            code, out, err = self.serve("push", data, path=str(self.pins / "c.bin"), sha256=sha(data),
                                        config=self.floor_config(floor=0))
        self.assertEqual((code, json.loads(out)), (0, "created"), err)

    def test_a_run_without_a_free_slot_is_refused_not_queued(self):
        config_path = self.write_config(self.root / "one-slot.json", max_concurrent_runs=1)
        self.put_shard()
        with pc._run_slot(pc.load_agent_config(config_path)):  # another run holds the only slot
            started = time.monotonic()
            self.assertRefused(self.run_shard(config=config_path), "collector busy")
            self.assertLess(time.monotonic() - started, 10, "refused at once, never queued")
        self.assertFalse((self.scratch / TAG).exists(), "nothing ran")
        code, out, err = self.run_shard(config=config_path)
        self.assertEqual((code, json.loads(out)["code"]), (0, 0), err)
        bad = self.write_config(self.root / "no-slot.json", max_concurrent_runs=0)
        self.assertEqual(self.serve("pin_status", pins=[], config=bad)[0], 1)

    def test_a_slot_is_released_after_each_run(self):
        config_path = self.write_config(self.root / "one-slot.json", max_concurrent_runs=1)
        self.put_shard()
        self.assertEqual(json.loads(self.run_shard(config=config_path)[1])["code"], 0)
        with pc._run_slot(pc.load_agent_config(config_path)):
            pass  # free again: the finished run released it
        code, out, err = self.serve("put_shard", json.dumps(self.shard_command(
            output_directory=str(self.scratch) + "/" + self.TAG2)).encode(), tag=self.TAG2)
        self.assertEqual(code, 0, err)
        code, out, err = self.serve("run_shard", tag=self.TAG2, seconds=60, config=config_path,
                                    collector_sha256=pc.sha256_file(self.collector))
        self.assertEqual((code, json.loads(out)["code"]), (0, 0), err)

    def test_a_busy_collector_is_a_failed_host_for_the_dispatcher(self):
        # The busy host has its own scratch with its one slot held; the dispatcher moves its shard on.
        busy_scratch = self.root / "busy-scratch"
        busy_scratch.mkdir()
        busy = self.write_config(self.root / "busy.json", scratch=str(busy_scratch), max_concurrent_runs=1)
        fake_ssh = self.root / "fake_ssh.py"
        fake_ssh.write_text(FAKE_SSH)

        def host(name, config_path, scratch):
            return pc.Host(name=name, kind="ssh", collector=str(self.collector),
                           collector_sha256=pc.sha256_file(self.collector), workers=2, weight=1.0,
                           scratch=str(scratch), arch="x86", ssh=[sys.executable, str(fake_ssh), str(config_path)],
                           prefix=[sys.executable], transport="agent")

        pinned = self.pins / "run" / "checkpoint.json"
        pinned.parent.mkdir()
        pinned.write_bytes(b"policy")
        episodes = [{"id": f"e{i}", "seed": 100 + i} for i in range(4)]
        command = {"mode": "collect_parallel", "workers": 2, "episodes": episodes,
                   "source": {"checkpoint": {"path": str(pinned), "sha256": pc.sha256_file(pinned)}},
                   "output_directory": "unused"}
        attempt = self.root / "learner-run" / "iterations" / "000003" / "attempt-000000"
        attempt.mkdir(parents=True)
        command_path = attempt / "collect-command.json"
        command_path.write_text(json.dumps(command))
        dispatcher = pc.Dispatcher({"hosts": [host("busy", busy, busy_scratch),
                                              host("free", self.config_path, self.scratch)]}, timeout=100)
        with pc._run_slot(pc.load_agent_config(busy)):
            receipt = dispatcher.collect(command_path, fake_merge)
        self.assertEqual(receipt["merged"]["slots"], [episode_bytes(e).decode() for e in episodes])
        self.assertEqual(receipt["failed_hosts"], ["busy"])
        self.assertEqual([s[3] for s in receipt["shards"]], ["free", "free"])
        failure = [e for e in receipt["events"] if e["event"] == "host-failed"]
        self.assertIn("collector busy", failure[0]["error"])


class SafeExtractionTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="pooled-extract-test-"))

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def refused(self, entries):
        archive = self.root / "bad.tar"
        with tarfile.open(archive, "w") as tar:
            for entry in entries:
                tar.addfile(entry, io.BytesIO(b"x") if entry.isreg() else None)
        local = self.root / "out"
        local.mkdir()
        try:
            with self.assertRaises(RuntimeError, msg=[e.name for e in entries]):
                pc._extract_shard_archive(archive, local)
        finally:
            shutil.rmtree(local)

    @staticmethod
    def member(name, kind=tarfile.REGTYPE, target=""):
        entry = tarfile.TarInfo(name)
        entry.type, entry.linkname = kind, target
        entry.size = 1 if kind == tarfile.REGTYPE else 0
        return entry

    def test_archives_with_unsafe_names_are_refused(self):
        for name in ("..\\..\\victim", "C:\\victim", "../victim", "a/b", "file:stream", ".hidden",
                     "NUL", "con.txt", "COM1.json", "lpt9", "Aux.tar.gz", "prn", "COM0", "trailing.", "x.json."):
            self.refused([self.member(name)])

    def test_archives_with_special_or_duplicate_members_are_refused(self):
        for kind, target in ((tarfile.SYMTYPE, "/etc/passwd"), (tarfile.LNKTYPE, "a.json"), (tarfile.DIRTYPE, ""),
                             (tarfile.CHRTYPE, ""), (tarfile.FIFOTYPE, "")):
            self.refused([self.member("a.json", kind, target)])
        self.refused([self.member("a.json"), self.member("a.json")])
        self.refused([self.member("a.json"), self.member("A.JSON")])

    def test_portable_names(self):
        for name in ("episode-0001.json", "collection.json", "non-natural.json", "CONSOLE.json", "nul1"):
            self.assertTrue(pc._portable_name(name), name)
        for name in ("NUL", "nul.json", "COM9", "lpt1.x.y", "a.", "", ".x", "a b", "a\\b"):
            self.assertFalse(pc._portable_name(name), name)

    def test_gzip1_archive_extracts_and_enforces_extracted_byte_cap(self):
        archive = self.root / "shard.tar.gz"
        payload = b"A" * 4096
        with tarfile.open(archive, "w:gz", compresslevel=1) as tar:
            entry = tarfile.TarInfo("episode-0000.json")
            entry.size = len(payload)
            tar.addfile(entry, io.BytesIO(payload))
        local = self.root / "out-ok"
        local.mkdir()
        pc._extract_shard_archive(archive, local, compression="gzip1", max_bytes=4096)
        self.assertEqual((local / "episode-0000.json").read_bytes(), payload)
        local_capped = self.root / "out-capped"
        local_capped.mkdir()
        with self.assertRaisesRegex(RuntimeError, "extracted output bound"):
            pc._extract_shard_archive(archive, local_capped, compression="gzip1", max_bytes=2048)


class AgentTransportBoundsTests(unittest.TestCase):
    """The learner's side of each connection: deadlines independent of reads, concurrent draining,
    bounded memory. Fake ssh programs misbehave in each way; works on a Windows learner too."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="pooled-bounds-test-"))

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def transport(self, body):
        script = self.root / "misbehaving_ssh.py"
        script.write_text(textwrap.dedent(body))
        return pc.AgentTransport(pc.Host(name="h", kind="ssh", collector="/c", collector_sha256="0" * 64,
                                         workers=1, weight=1.0, scratch="/s", arch="x86",
                                         ssh=[sys.executable, str(script)], transport="agent"))

    def test_a_silent_connection_is_ended_at_the_deadline(self):
        transport = self.transport("import time\ntime.sleep(60)\n")
        started = time.monotonic()
        with mock.patch.object(pc, "FETCH_SECONDS", 1.5), self.assertRaisesRegex(RuntimeError, "timed out"):
            transport.fetch_dir("/s/" + TAG, self.root / "local")
        with self.assertRaisesRegex(RuntimeError, "timed out"):
            transport._json("pin_status", timeout=1.5, pins=[])
        self.assertLess(time.monotonic() - started, 20)
        self.assertFalse((self.root / "local").exists(), "a failed fetch leaves nothing")

    def test_a_stream_that_stalls_midway_is_ended_at_the_deadline(self):
        transport = self.transport("import sys, time\nsys.stdout.buffer.write(b'x' * 100000)\n"
                                   "sys.stdout.flush()\ntime.sleep(60)\n")
        started = time.monotonic()
        with mock.patch.object(pc, "FETCH_SECONDS", 1.5), self.assertRaisesRegex(RuntimeError, "timed out"):
            transport.fetch_dir("/s/" + TAG, self.root / "local")
        self.assertLess(time.monotonic() - started, 20)

    def test_stderr_flooding_neither_deadlocks_nor_grows_memory(self):
        transport = self.transport("import sys\nfor _ in range(320):\n    sys.stderr.write('e' * 65536)\n"
                                   "sys.stderr.write('LAST')\nsys.stderr.flush()\nprint('\"stored\"')\n")
        self.assertEqual(transport._json("put_shard", b"{}", timeout=60, tag=TAG), "stored")
        code, out, tail = pc._bounded_process(transport._argv("pin_status", pins=[]), "probe", 60, 1 << 20)
        self.assertEqual((code, out.strip()), (0, b'"stored"'))
        self.assertLessEqual(len(tail), pc.AGENT_STDERR_TAIL)
        self.assertTrue(tail.endswith(b"LAST"))

    def test_an_oversized_reply_is_refused(self):
        transport = self.transport("import sys\nsys.stdout.buffer.write(b'1' * (3 << 20))\n")
        with self.assertRaisesRegex(RuntimeError, "output bound"):
            transport._json("pin_status", timeout=60, pins=[])


class AgentDispatcherTests(AgentCase):
    def agent_host(self, name, arch, config_path, **changes):
        fake_ssh = self.root / "fake_ssh.py"
        fake_ssh.write_text(FAKE_SSH)
        return pc.Host(name=name, kind="ssh", collector=str(self.collector),
                       collector_sha256=pc.sha256_file(self.collector), workers=2, weight=1.0,
                       scratch=str(self.scratch), arch=arch, ssh=[sys.executable, str(fake_ssh), str(config_path)],
                       prefix=[sys.executable], transport="agent", **changes)

    def collect_with_failover(self, config_path):
        broken = self.write_config(self.root / "broken.json", collector_sha256="0" * 64)
        hosts = [AgentDispatcherTests.agent_host(self, "broken", "arm64", broken),
                 AgentDispatcherTests.agent_host(self, "good-a", "x86", config_path),
                 AgentDispatcherTests.agent_host(self, "good-b", "arm64", config_path)]
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

    def test_the_dispatcher_collects_through_the_agent_with_audits_and_failover(self):
        self.collect_with_failover(self.config_path)

    def test_agent_mode_uploads_missing_pins_and_removes_only_its_own(self):
        transport = pc.AgentTransport(self.agent_host("a", "x86", self.config_path))
        learner_copy = self.root / "learner-copy.bin"
        learner_copy.write_bytes(b"input bytes")
        remote = str(self.pins / "inputs" / "input.bin")
        self.assertTrue(transport.push(learner_copy, remote, sha(b"input bytes")))
        self.assertFalse(transport.push(learner_copy, remote, sha(b"input bytes")), "existing: not ours")
        self.assertEqual(Path(remote).read_bytes(), b"input bytes")
        self.assertEqual(transport.remove_owned(remote, sha(b"other")), "not-owned")
        self.assertEqual(transport.remove_owned(remote, sha(b"input bytes")), "removed")
        self.assertFalse(Path(remote).exists())

    def test_dispatcher_prune_through_the_agent_removes_only_what_it_installed(self):
        state = self.root / "learner-state" / "retention.json"
        host = self.agent_host("a", "x86", self.config_path, prune_pushed=True)
        dispatcher = pc.Dispatcher({"hosts": [host], "state_path": str(state)})
        transport = dispatcher.transports["a"]
        learner_copy = self.root / "learner-copy.bin"
        learner_copy.write_bytes(b"old checkpoint")
        installed, foreign = str(self.pins / "run" / "old.bin"), str(self.pins / "run" / "foreign.bin")
        self.assertTrue(transport.push(learner_copy, installed, sha(b"old checkpoint")))
        Path(foreign).write_bytes(b"old checkpoint")  # same bytes, never installed by this dispatcher
        key = dispatcher.storage_key_now(host)
        state.parent.mkdir()
        # An earlier collection recorded both (the foreign one as if pushed: a stale or replaced record).
        state.write_text(json.dumps({key: {installed: sha(b"old checkpoint"), foreign: sha(b"old checkpoint")}}))
        later = pc.Dispatcher({"hosts": [host], "state_path": str(state)})
        later.prune(set())  # a later collection that pushed neither
        events = {e["path"]: e["event"] for e in later.log if e["event"] in ("pruned", "not-owned")}
        self.assertEqual(events, {installed: "pruned", foreign: "not-owned"})
        self.assertFalse(Path(installed).exists())
        self.assertEqual(Path(foreign).read_bytes(), b"old checkpoint")
        self.assertEqual(json.loads(state.read_text())[key], {})


if __name__ == "__main__":
    unittest.main()
