"""Pooled collection dispatcher tests (stage 1). Hosts are simulated in temporary directories; the
fake collector writes each slot's bytes as a function of its episode alone (as the real collector
does), so any host split, failover or audit must merge to the same files."""
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

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

    def missing_pins(self, pins):
        return sorted(p for p in pins if self.files.get(p[0]) != p[1])

    def push(self, local, remote):
        self.files[remote] = pc.sha256_file(Path(local))
        self.pushed.append(remote)

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

    def test_old_pushed_checkpoints_are_pruned_on_ssh_hosts_only(self):
        config = self.config([("a", 1.0, "x86"), ("b", 1.0, "arm64")])
        for host in config["hosts"]:
            host.kind, host.ssh, host.prune_pushed = "ssh", ["ssh", host.name], True
        config["state_path"] = str(self.root / "pushed.json")
        transports = self.transports(config)
        dispatcher = pc.Dispatcher(config, transports)
        dispatcher.collect(self.command_path, fake_merge)
        old = str(self.checkpoint)
        # The next iteration collects from a new checkpoint.
        new = self.root / "learner" / "checkpoint-2.json"
        new.write_bytes(b"next checkpoint")
        self.command["source"]["checkpoint"] = {"path": str(new), "sha256": pc.sha256_file(new)}
        nxt = self.root / "run" / "iterations" / "000017" / "attempt-000000"
        nxt.mkdir(parents=True)
        (nxt / "collect-command.json").write_text(json.dumps(self.command))
        pc.Dispatcher(config, transports).collect(nxt / "collect-command.json", fake_merge)
        for transport in transports.values():
            self.assertEqual(transport.removed, [old])
            self.assertIn(str(new), transport.files)
        state = json.loads((self.root / "pushed.json").read_text())
        self.assertEqual(state["a"], [str(new)])

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


class ChainsPassThroughTests(unittest.TestCase):
    """The chain driver passes a pooled arm's pinned dispatcher and hosts file to the trainer, and the
    pooled-collection setting stays outside recipe_identity (execution only)."""

    def setUp(self):
        import chains  # noqa: E402  (heavy import, only for these tests)
        self.chains = chains
        self.root = Path(tempfile.mkdtemp(prefix="pooled-chains-test-"))
        for name in ("trainer", "pooled_collect.py", "hosts.json"):
            (self.root / name).write_text(name)

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

    def test_a_changed_dispatcher_is_refused(self):
        arm = self.arm()
        (self.root / "pooled_collect.py").write_text("changed")
        with self.assertRaises(Exception):
            self.chains.trainer_command(arm, "/cfg.json")

    def test_pooled_collection_is_outside_recipe_identity(self):
        self.assertEqual(self.chains.recipe_identity(self.arm()), self.chains.recipe_identity(self.arm(pooled=False)))
