"""Pooled collection dispatcher tests (stage 1). Hosts are simulated in temporary directories; the
fake collector writes each slot's bytes as a function of its episode alone (as the real collector
does), so any host split, failover or audit must merge to the same files."""
import hashlib
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import pooled_collect as pc  # noqa: E402


def episode_bytes(episode: dict, salt: str = "") -> bytes:
    return f"{episode['id']}:{episode['seed']}{salt}".encode()


class FakeTransport:
    """One simulated host. `fail` makes every shard run fail; `salt` makes this host's bytes differ
    (a divergent architecture)."""

    def __init__(self, host, files: dict, fail=False, salt=""):
        self.host, self.files, self.fail, self.salt = host, files, fail, salt
        self.pushed, self.runs = [], []

    def pin_status(self, pins):
        return {p: ("absent" if p[0] not in self.files else "ok" if self.files[p[0]] == p[1] else "differs")
                for p in pins}

    def push(self, local, remote, digest):
        if remote in self.files:
            if self.files[remote] != digest:
                raise RuntimeError(f"existing pin differs: {remote}")
            return False  # another process installed the same bytes first: not ours
        assert pc.sha256_file(Path(local)) == digest
        self.files[remote] = digest
        self.pushed.append(remote)
        return True

    def put_text(self, remote, text):
        self.files[remote] = text

    def run(self, argv, timeout):
        command = json.loads(self.files[argv[-1]])
        self.runs.append(command["shard"]["first_slot"])
        if self.fail:
            return subprocess.CompletedProcess(argv, 1, "", "host down")
        out = Path(command["output_directory"])
        out.mkdir(parents=True)
        pins = []
        for offset, episode in enumerate(command["episodes"]):
            name = f"episode-{command['shard']['first_slot'] + offset:04d}.json"
            data = episode_bytes(episode, self.salt)
            (out / name).write_bytes(data)
            pins.append({"path": f"/collector/{name}", "sha256": hashlib.sha256(data).hexdigest()})
        (out / "collection.json").write_text(json.dumps({"trajectories": pins, "shard": command["shard"]}))
        return subprocess.CompletedProcess(argv, 0, "", "")

    def remove_file(self, remote):
        self.files.pop(remote, None)
        self.removed = getattr(self, "removed", []) + [remote]

    def fetch_dir(self, remote, local):
        if local.exists():
            shutil.rmtree(local)
        shutil.copytree(remote, local)

    def remove_dir(self, remote):
        shutil.rmtree(remote, ignore_errors=True)


def fake_merge(command_path, directories):
    """Checks contiguity and returns every slot's bytes in schedule order."""
    slots, expected = [], 0
    for directory in directories:
        document = json.loads((directory / "collection.json").read_text())
        assert document["shard"]["first_slot"] == expected
        for pin in document["trajectories"]:
            name = Path(pin["path"]).name
            data = (directory / name).read_bytes()
            assert hashlib.sha256(data).hexdigest() == pin["sha256"]
            slots.append(data.decode())
            expected += 1
    return {"slots": slots}


class PooledCollectTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="pooled-collect-test-"))
        self.checkpoint = self.root / "learner" / "checkpoint.json"
        self.checkpoint.parent.mkdir(parents=True)
        self.checkpoint.write_bytes(b"checkpoint bytes")
        digest = pc.sha256_file(self.checkpoint)
        self.episodes = [{"id": f"e{i}", "seed": 1000 + i} for i in range(23)]
        self.command = {"mode": "collect_parallel", "workers": 4,
                        "source": {"checkpoint": {"path": str(self.checkpoint), "sha256": digest}},
                        "episodes": self.episodes, "output_directory": "unused"}
        attempt = self.root / "run" / "iterations" / "000016" / "attempt-000000"
        attempt.mkdir(parents=True)
        self.command_path = attempt / "collect-command.json"
        self.command_path.write_text(json.dumps(self.command))

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def config(self, specs, audit=None):
        hosts = [pc.Host(name=name, kind="local", collector=f"/bin/{name}", collector_sha256="c" * 64,
                         workers=4, weight=weight, scratch=str(self.root / "scratch" / name), arch=arch)
                 for name, weight, arch in specs]
        return {"hosts": hosts, "audit": audit or {"every": 0, "episodes": 0}}

    def transports(self, config, fail=(), salt=None):
        salt = salt or {}
        return {h.name: FakeTransport(h, {h.collector: h.collector_sha256}, h.name in fail, salt.get(h.name, ""))
                for h in config["hosts"]}

    def expected(self):
        return [episode_bytes(e).decode() for e in self.episodes]

    def test_nested_pins_of_small_json_inputs_are_distributed(self):
        leaf = self.root / "learner" / "parameters.f32le"
        leaf.write_bytes(bytes([0, 1]) + b"binary")
        imp = self.root / "learner" / "import.json"
        imp.write_text(json.dumps({"parameters": {"path": str(leaf), "sha256": pc.sha256_file(leaf)}}))
        self.command["source"]["play_import"] = {"path": str(imp), "sha256": pc.sha256_file(imp)}
        self.command_path.write_text(json.dumps(self.command))
        self.assertIn((str(leaf), pc.sha256_file(leaf)), pc.input_closure(self.command))
        # A provenance pin the learner does not hold (another host's interpreter) is not an input.
        imp.write_text(json.dumps({"parameters": {"path": str(leaf), "sha256": pc.sha256_file(leaf)},
                                   "producer": {"path": "C:/Python314/python.exe", "sha256": "f" * 64}}))
        self.command["source"]["play_import"] = {"path": str(imp), "sha256": pc.sha256_file(imp)}
        self.command_path.write_text(json.dumps(self.command))
        self.assertNotIn(("C:/Python314/python.exe", "f" * 64), pc.input_closure(self.command))
        self.assertIn((str(leaf), pc.sha256_file(leaf)), pc.input_closure(self.command))
        config = self.config([("a", 1.0, "x86")])
        transports = self.transports(config)
        pc.Dispatcher(config, transports).collect(self.command_path, fake_merge)
        self.assertEqual(sorted(transports["a"].pushed), sorted([str(self.checkpoint), str(imp), str(leaf)]))

    def test_plan_sizes_split_by_weight_and_cover_every_episode(self):
        self.assertEqual(pc.plan_sizes(10, [1, 1]), [5, 5])
        self.assertEqual(pc.plan_sizes(480, [2, 1, 1]), [240, 120, 120])
        self.assertEqual(sum(pc.plan_sizes(61, [3, 2.5, 1, 0.5])), 61)
        self.assertEqual(pc.plan_sizes(2, [1, 1, 1]), [1, 1, 0])
        with self.assertRaises(ValueError):
            pc.plan_sizes(0, [1])

    def test_pins_iteration_and_audit_slots(self):
        pins = pc.pins_of({"a": {"path": "/x", "sha256": "1"}, "b": [{"checkpoint": {"path": "/y", "sha256": "2"}}]})
        self.assertEqual(pins, {("/x", "1"), ("/y", "2")})
        self.assertEqual(pc.iteration_of(self.command_path), 16)
        self.assertEqual(pc.audit_slots(16, 480, 4, 16), pc.audit_slots(16, 480, 4, 16))
        self.assertEqual(len(pc.audit_slots(16, 480, 4, 16)), 4)
        self.assertEqual(pc.audit_slots(17, 480, 4, 16), [])
        self.assertEqual(pc.audit_slots(0, 480, 4, 0), [])

    def test_any_split_merges_to_the_same_collection_and_distributes_the_policy(self):
        for specs in ([("a", 1.0, "x86")], [("a", 1.0, "x86"), ("b", 2.0, "x86"), ("c", 0.5, "arm64")]):
            config = self.config(specs)
            transports = self.transports(config)
            receipt = pc.Dispatcher(config, transports).collect(self.command_path, fake_merge)
            self.assertEqual(receipt["merged"]["slots"], self.expected())
            for transport in transports.values():
                if transport.runs:
                    self.assertEqual(transport.pushed, [str(self.checkpoint)])
            self.assertTrue((self.command_path.parent / "pooled-shards" / "dispatch.json").exists())
            self.assertEqual([p.name for p in (self.command_path.parent / "pooled-shards").iterdir()],
                             ["dispatch.json"])

    def test_a_failed_host_moves_its_shard_and_changes_no_byte(self):
        config = self.config([("a", 1.0, "x86"), ("b", 1.0, "x86"), ("c", 1.0, "arm64")])
        transports = self.transports(config, fail={"b"})
        receipt = pc.Dispatcher(config, transports).collect(self.command_path, fake_merge)
        self.assertEqual(receipt["merged"]["slots"], self.expected())
        self.assertEqual(receipt["failed_hosts"], ["b"])
        self.assertTrue(any(s[3] != "b" for s in receipt["shards"]))
        self.assertNotIn("b", [s[3] for s in receipt["shards"]])

    def test_every_host_failing_fails_the_collection(self):
        config = self.config([("a", 1.0, "x86"), ("b", 1.0, "x86")])
        transports = self.transports(config, fail={"a", "b"})
        with self.assertRaises(RuntimeError):
            pc.Dispatcher(config, transports).collect(self.command_path, fake_merge)

    def test_a_whole_command_one_process_refuses_launches_nothing(self):
        config = self.config([("a", 1.0, "x86"), ("b", 1.0, "x86")])
        transports = self.transports(config)
        for change in ({"episodes": self.episodes[:-1] + [dict(self.episodes[0])]}, {"workers": 1},
                       {"workers": 1025}, {"episodes": []}):
            command = dict(self.command, **change)
            self.command_path.write_text(json.dumps(command))
            with self.assertRaises(ValueError):
                pc.Dispatcher(config, transports).collect(self.command_path, fake_merge)
        self.assertEqual([t.runs for t in transports.values()], [[], []])
        self.assertFalse((self.command_path.parent / "pooled-shards").exists())

    def test_an_ancestor_named_iterations_does_not_confuse_the_index(self):
        path = Path("/srv/iterations/run/iterations/000007/attempt-000000/collect-command.json")
        self.assertEqual(pc.iteration_of(path), 7)
        with self.assertRaises(ValueError):
            pc.iteration_of(Path("/srv/iterations/run/collect-command.json"))

    def test_an_ordinary_play_import_distributes_the_files_its_loader_reads(self):
        export = self.root / "learner" / "export"
        export.mkdir()
        for name in ("metadata.json", "parameters.f32le"):
            (export / name).write_bytes(name.encode())
        run_file = self.root / "learner" / "run.json"
        run_file.write_text("{}")
        registry = self.root / "learner" / "registry.json"
        registry.write_text("[]")
        imp = self.root / "learner" / "ordinary-import.json"
        imp.write_text(json.dumps({"export_directory": str(export), "expected_metadata_sha256": "x",
                                   "expected_model_parameter_sha256": "y", "source_run_path": str(run_file),
                                   "source_registry_path": str(registry), "expected_source_registry_sha256": "z",
                                   "source_registry_git_commit": "c", "expected_destination_card_db_hash": "d"}))
        self.command["source"]["play_import"] = {"path": str(imp), "sha256": pc.sha256_file(imp)}
        paths = {p for p, _ in pc.input_closure(self.command)}
        root = str(export).rstrip("/\\")
        for expected in (f"{root}/metadata.json", f"{root}/parameters.f32le", str(run_file), str(registry)):
            self.assertIn(expected, paths)

    def test_a_foreign_file_at_a_pinned_path_is_never_replaced(self):
        config = self.config([("a", 1.0, "x86"), ("b", 1.0, "x86")])
        transports = self.transports(config)
        transports["b"].files[str(self.checkpoint)] = "e" * 64
        receipt = pc.Dispatcher(config, transports).collect(self.command_path, fake_merge)
        self.assertEqual(receipt["merged"]["slots"], self.expected())
        self.assertEqual(receipt["failed_hosts"], ["b"])
        self.assertEqual(transports["b"].pushed, [])
        self.assertEqual(transports["b"].files[str(self.checkpoint)], "e" * 64)

    def test_remote_deletion_stays_inside_the_scratch_root(self):
        host = pc.Host(name="m", kind="ssh", collector="/c", collector_sha256="c" * 64, workers=2, weight=1.0,
                       scratch="/srv/scratch", arch="x86", ssh=["ssh", "m"])
        self.assertEqual(pc.Dispatcher.owned_dir(host, "/srv/scratch/pooled-ab-s000"), "/srv/scratch/pooled-ab-s000")
        for bad in ("/srv/scratch", "/srv/other/pooled-x", "/srv/scratch/run", "/srv/scratch/pooled-x/../.."):
            with self.assertRaises(RuntimeError):
                pc.Dispatcher.owned_dir(host, bad)

    def test_tags_differ_between_runs_sharing_a_scratch_root(self):
        config = self.config([("a", 1.0, "x86")])
        transports = self.transports(config)
        first = pc.Dispatcher(config, transports).collect(self.command_path, fake_merge)
        other = self.root / "run2" / "iterations" / "000016" / "attempt-000000"
        other.mkdir(parents=True)
        (other / "collect-command.json").write_text(json.dumps(self.command))
        second = pc.Dispatcher(config, transports).collect(other / "collect-command.json", fake_merge)
        tags = lambda r: {e["tag"] for e in r["events"] if e["event"] == "shard"}  # noqa: E731
        self.assertFalse(tags(first) & tags(second))

    def test_a_host_without_its_pinned_collector_is_not_used(self):
        config = self.config([("a", 1.0, "x86"), ("b", 1.0, "x86")])
        transports = self.transports(config)
        transports["b"].files[config["hosts"][1].collector] = "d" * 64
        receipt = pc.Dispatcher(config, transports).collect(self.command_path, fake_merge)
        self.assertEqual(receipt["merged"]["slots"], self.expected())
        self.assertEqual(receipt["failed_hosts"], ["b"])
        self.assertEqual(transports["b"].runs, [])

    def test_audit_replays_on_another_architecture_and_catches_divergence(self):
        audit = {"every": 16, "episodes": 3}
        config = self.config([("x", 1.0, "x86"), ("m", 1.0, "arm64")], audit)
        receipt = pc.Dispatcher(config, self.transports(config)).collect(self.command_path, fake_merge)
        audits = [e for e in receipt["events"] if e["event"] == "audit"]
        self.assertEqual(len(audits), 3)
        self.assertTrue(all(e["equal"] and e["played_on"] != e["replayed_on"] for e in audits))
        config = self.config([("x", 1.0, "x86"), ("m", 1.0, "arm64")], audit)
        with self.assertRaisesRegex(RuntimeError, "cross-host divergence"):
            pc.Dispatcher(config, self.transports(config, salt={"m": "-arm"})).collect(self.command_path, fake_merge)

    def test_remote_commands_can_run_inside_a_container(self):
        host = pc.Host(name="m", kind="ssh", collector="/c", collector_sha256="c" * 64, workers=2, weight=1.0,
                       scratch="/s", arch="arm64", ssh=["ssh", "mac"],
                       remote_wrap="docker exec -i node sh -c {cmd}")
        argv = pc.Transport(host)._ssh(["tar -C '/a b' -cf - ."])
        self.assertEqual(argv[:2], ["ssh", "mac"])
        self.assertEqual(argv[2], "docker exec -i node sh -c " + pc.shlex.quote("tar -C '/a b' -cf - ."))
        host.remote_wrap = ""
        self.assertEqual(pc.Transport(host)._ssh(["ls"]), ["ssh", "mac", "ls"])

    def test_a_hosts_file_asking_for_retention_is_refused(self):
        # Remote retention (pruning old pins on collectors) is not part of this version.
        path = self.root / "hosts.json"
        entry = {"name": "a", "kind": "ssh", "collector": "/bin/c", "collector_sha256": "c" * 64, "workers": 4,
                 "weight": 1.0, "scratch": "/s", "arch": "x86", "ssh": ["ssh", "h"], "prune_pushed": True}
        path.write_text(json.dumps({"schema": pc.SCHEMA, "hosts": [entry], "merge_binary": "/bin/m"}))
        with self.assertRaises((TypeError, ValueError)):
            pc.load_hosts(path)

    def test_hosts_file_is_checked(self):
        path = self.root / "hosts.json"
        entry = {"name": "a", "kind": "ssh", "collector": "/bin/c", "collector_sha256": "c" * 64, "workers": 4,
                 "weight": 1.0, "scratch": "/s", "arch": "x86", "ssh": []}
        path.write_text(json.dumps({"schema": pc.SCHEMA, "hosts": [entry]}))
        with self.assertRaises(ValueError):
            pc.load_hosts(path)
        entry["ssh"] = ["ssh", "-o", "BatchMode=yes", "mtg@host"]
        path.write_text(json.dumps({"schema": pc.SCHEMA, "hosts": [entry], "merge_binary": "/bin/m"}))
        self.assertEqual(pc.load_hosts(path)["hosts"][0].name, "a")


if __name__ == "__main__":
    unittest.main()


class PinPublicationTests(unittest.TestCase):
    """Pins are published atomically and never replace an established file (ported from the
    parallel dispatcher series on the same pull request, adapted to `pin_status`)."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="pooled-pin-test-"))
        self.checkpoint = self.root / "learner" / "checkpoint.json"
        self.checkpoint.parent.mkdir(parents=True)
        self.checkpoint.write_bytes(b"checkpoint bytes")
        self.host = pc.Host(name="a", kind="local", collector="/c", collector_sha256="c" * 64, workers=2,
                            weight=1.0, scratch=str(self.root / "scratch"), arch="x86")

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def test_concurrent_pin_uploads_have_separate_temporary_files(self):
        transport = pc.Transport(self.host)
        remote = self.root / "collector" / "pin.json"
        copied = threading.Barrier(2, timeout=10)
        real_copy = shutil.copyfile
        stages = []

        def copy_and_wait(src, dest):
            stages.append(dest)
            real_copy(src, dest)
            copied.wait()

        with mock.patch.object(pc.shutil, "copyfile", side_effect=copy_and_wait), \
                ThreadPoolExecutor(max_workers=2) as pool:
            digest = pc.sha256_file(self.checkpoint)
            tasks = [pool.submit(transport.push, self.checkpoint, str(remote), digest) for _ in range(2)]
            for task in tasks:
                task.result(timeout=15)
        self.assertEqual(remote.read_bytes(), self.checkpoint.read_bytes())
        self.assertEqual(list(remote.parent.iterdir()), [remote])
        self.assertEqual(len(set(stages)), 2)

    def test_failed_pin_upload_cleans_its_temporary_file_and_preserves_the_destination(self):
        transport = pc.Transport(self.host)
        remote = self.root / "collector" / "pin.json"
        remote.parent.mkdir()
        remote.write_bytes(b"existing pin")

        def interrupted(src, dest):
            Path(dest).write_bytes(b"partial upload")
            raise OSError("copy interrupted")

        with mock.patch.object(pc.shutil, "copyfile", side_effect=interrupted):
            with self.assertRaisesRegex(OSError, "copy interrupted"):
                transport.push(self.checkpoint, str(remote), pc.sha256_file(self.checkpoint))
        self.assertEqual(list(remote.parent.iterdir()), [remote])
        self.assertEqual(remote.read_bytes(), b"existing pin")

    def test_two_absent_observations_cannot_replace_a_conflicting_first_pin(self):
        first, second = pc.Transport(self.host), pc.Transport(self.host)
        remote = self.root / "collector" / "pin.json"
        different = self.root / "different.json"
        different.write_bytes(b"other learner's checkpoint")
        first_pin = str(remote), pc.sha256_file(self.checkpoint)
        second_pin = str(remote), pc.sha256_file(different)
        self.assertEqual(first.pin_status({first_pin})[first_pin], "absent")
        self.assertEqual(second.pin_status({second_pin})[second_pin], "absent")
        first.push(self.checkpoint, *first_pin)
        with self.assertRaisesRegex(RuntimeError, "existing pin differs"):
            second.push(different, *second_pin)
        self.assertEqual(second.pin_status({second_pin})[second_pin], "differs")
        self.assertEqual(first.pin_status({first_pin})[first_pin], "ok")
        self.assertEqual(remote.read_bytes(), self.checkpoint.read_bytes())
        self.assertEqual(list(remote.parent.iterdir()), [remote])

    def test_fake_ssh_preserves_a_canonical_pin_installed_after_discovery(self):
        self.host.kind, self.host.ssh = "ssh", ["fake-ssh", "collector"]
        transport = pc.Transport(self.host)
        remote = self.root / "collector's scratch" / "pin.json"
        different = self.root / "different.json"
        different.write_bytes(b"other learner's checkpoint")
        good_pin = remote.as_posix(), pc.sha256_file(self.checkpoint)
        conflicting_pin = remote.as_posix(), pc.sha256_file(different)
        local_run = subprocess.run

        def emulate_ssh(argv, **kwargs):
            self.assertEqual(argv[:-1], self.host.ssh)
            command = shlex.split(argv[-1])
            self.assertEqual(command[:2], ["python3", "-c"])
            return local_run([sys.executable, *command[1:]], **kwargs)

        with mock.patch.object(pc.subprocess, "run", side_effect=emulate_ssh):
            self.assertEqual(transport.pin_status({conflicting_pin})[conflicting_pin], "absent")
            transport.push(self.checkpoint, *good_pin)
            transport.push(self.checkpoint, *good_pin)
            with self.assertRaisesRegex(RuntimeError, "existing pin differs"):
                transport.push(different, *conflicting_pin)
            self.assertEqual(transport.pin_status({good_pin})[good_pin], "ok")
            self.assertEqual(transport.pin_status({conflicting_pin})[conflicting_pin], "differs")
        self.assertEqual(remote.read_bytes(), self.checkpoint.read_bytes())
        self.assertEqual(list(remote.parent.iterdir()), [remote])

    def test_merge_uses_the_native_trainer_mode_in_schedule_order(self):
        command_path = self.root / "collect-command.json"
        shards = [self.root / "first-shard", self.root / "second-shard"]
        trainer = str(self.root / "native-trainer")
        output = {"trajectories": ["first", "second"], "behavior_state_sha256": "state"}
        result = subprocess.CompletedProcess([], 0, json.dumps(output), "")
        with mock.patch.object(pc.subprocess, "run", return_value=result) as run:
            report = pc.merge_with_binary(trainer)(command_path, shards)
        run.assert_called_once_with([trainer, "--merge-collection-shards", str(command_path),
                                     *map(str, shards)], capture_output=True, text=True)
        self.assertEqual(report, {"trajectories": 2, "behavior_state_sha256": "state"})


class ChainsPassThroughTests(unittest.TestCase):
    """The chain driver passes a pooled arm's pinned dispatcher and hosts file to the trainer, and the
    pooled-collection setting stays outside recipe_identity (execution only)."""

    def setUp(self):
        import chains  # noqa: E402  (heavy import, only for these tests)
        self.chains = chains
        self.root = Path(tempfile.mkdtemp(prefix="pooled-chains-test-"))
        for name in ("trainer", "pooled_collect.py", "hosts.json"):
            (self.root / name).write_text(name)
        (self.root / "pooled_collect.py").chmod(0o755)

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def arm(self, pooled=True):
        c = self.chains
        arm = {"arm": "C", "recipe": "C", "base": {"path": "b", "sha256": "1"}, "parent": {"path": "p", "sha256": "2"},
               "x": None, "workers": 4, "segment_updates": 16, "c_wall_seconds": None,
               "trainer": c.pin(self.root / "trainer")}
        if pooled:
            arm["pooled_collect"] = {"dispatcher": c.pin(self.root / "pooled_collect.py"),
                                     "hosts": c.pin(self.root / "hosts.json")}
        return arm

    def test_a_pooled_arm_runs_the_trainer_with_its_dispatcher_and_hosts(self):
        argv, env = self.chains.trainer_command(self.arm(), "/cfg.json")
        self.assertEqual(argv[-3:], ["/cfg.json", "--collect-external", (self.root / "pooled_collect.py").as_posix()])
        self.assertEqual(env[self.chains.POOLED_COLLECT_ENV], (self.root / "hosts.json").as_posix())
        argv, env = self.chains.trainer_command(self.arm(pooled=False), "/cfg.json")
        self.assertEqual(argv[-1], "/cfg.json")
        self.assertIsNone(env)

    @unittest.skipUnless(os.name == "posix", "the native POSIX trainer executes a dispatcher directly")
    def test_dispatcher_must_be_executable_when_the_segment_starts(self):
        arm = self.arm()
        (self.root / "pooled_collect.py").chmod(0o644)
        with self.assertRaisesRegex(self.chains.Stop, "dispatcher must be executable"):
            self.chains.trainer_command(arm, "/cfg.json")

    @unittest.skipUnless(os.name == "posix", "the native POSIX trainer executes a dispatcher directly")
    def test_shipped_dispatcher_reaches_its_usage_check(self):
        result = subprocess.run([str(HERE / "pooled_collect.py")], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage: pooled_collect.py", result.stderr)

    def test_a_changed_dispatcher_is_refused(self):
        arm = self.arm()
        (self.root / "pooled_collect.py").write_text("changed")
        with self.assertRaises(Exception):
            self.chains.trainer_command(arm, "/cfg.json")

    def test_pooled_collection_is_outside_recipe_identity(self):
        self.assertEqual(self.chains.recipe_identity(self.arm()), self.chains.recipe_identity(self.arm(pooled=False)))
