"""The box runner (`pooled_collect.py runner`, the agent's executor "spool"): the agent keeps every check of
run_shard and hands the run to a runner inside a W5 box through `<scratch>/.spool`. The runner here is a real
process; agents run in-process (sandbox "none") or as their own processes, behind the fake sshd for the
dispatcher. A config without `executor` keeps the original behaviour (test_pooled_agent.py covers it)."""
import io
import json
import os
import signal
import subprocess
import sys
import tarfile
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE / "tests"))
import pooled_collect as pc  # noqa: E402
from test_pooled_agent import FAKE_COLLECTOR, FAKE_SSH, LANDLOCK, TAG, AgentCase, gone  # noqa: E402
from test_pooled_collect import episode_bytes, fake_merge  # noqa: E402

POSIX = os.name == "posix" and hasattr(os, "waitid")
LINUX = sys.platform.startswith("linux")
# The fake collector, plus episode fields "slow" (a delay, then a normal run) and "context" (records its working
# directory and environment in `context.json` there, outside the fetched output).
SLOW_COLLECTOR = FAKE_COLLECTOR.replace(
    'if first.get("id") == "sleep":',
    'if first.get("context"):\n'
    '    json.dump({"cwd": os.getcwd(), "env": dict(os.environ)}, open("context.json", "w"))\n'
    'if first.get("slow"):\n    time.sleep(first["slow"])\n'
    'if first.get("id") == "sleep":', 1)


def wait_for(predicate, seconds=20.0):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


def tar_files(data: bytes) -> dict:
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        return {m.name: archive.extractfile(m).read() for m in archive}


class ExecutorConfigTests(AgentCase):
    """The optional `executor` key, on every platform."""

    def test_without_executor_the_agent_runs_the_collector_itself(self):
        self.assertEqual(pc.load_agent_config(self.config_path)["executor"], {"kind": "direct"})
        self.put_shard()
        code, out, err = self.run_shard()
        self.assertEqual((code, json.loads(out)["code"]), (0, 0), err)
        self.assertFalse((self.scratch / ".spool").exists(), "the direct executor never touches a spool")
        direct = self.write_config(self.root / "direct.json", executor={"kind": "direct"})
        self.assertEqual(pc.load_agent_config(direct)["executor"], {"kind": "direct"})

    def test_executor_values_are_checked(self):
        spool = os.path.join(str(self.scratch), ".spool")
        for bad in ("spool", {"kind": "queue"}, {"kind": "direct", "heartbeat_seconds": 2},
                    {"kind": "spool", "extra": 1}, {"kind": "spool", "heartbeat_seconds": 0},
                    {"kind": "spool", "stop_grace_seconds": True}, {"kind": "spool", "watchdog_grace_seconds": 86401},
                    {"kind": "spool", "stop_file": str(self.root / "STOP")},
                    {"kind": "spool", "end_file": os.path.join(spool, "runner.lock")},
                    {"kind": "spool", "stop_file": os.path.join(spool, TAG + ".result")},
                    {"kind": "spool", "stop_file": os.path.join(spool, "X"), "end_file": os.path.join(spool, "X")}):
            with self.assertRaises(ValueError, msg=bad):
                pc.load_agent_config(self.write_config(self.root / "bad.json", executor=bad))

    def test_a_vector_tanh_directory_is_read_only_and_outside_the_roots(self):
        records = self.root / "vector-tanh"
        records.mkdir()
        config = pc.load_agent_config(self.write_config(self.root / "vt.json", vector_tanh_dir=str(records)))
        rules = {path: access for path, access, _ in pc.collector_rules(config, "/c", "/r")}
        self.assertEqual(rules[str(records)], pc.LL_READ)
        self.assertEqual(pc._direct_env(config)["MTG_KERNEL_VECTOR_TANH_DIR"], str(records))
        self.assertEqual(pc._spool_env(config), dict(pc.SPOOL_ENV, MTG_KERNEL_VECTOR_TANH_DIR=str(records)))
        self.assertIsNone(pc._direct_env(pc.load_agent_config(self.config_path)), "absent: inherited as before")
        self.assertNotEqual(pc._execution_binding(config), pc._execution_binding(pc.load_agent_config(self.config_path)))
        for bad in (str(self.pins / "vt"), "relative", 7, os.path.abspath(os.sep), str(self.root / "missing"),
                    str(self.root)):  # inside a root, relative, not text, "/", absent, above the roots
            with self.assertRaisesRegex(ValueError, "vector_tanh_dir", msg=bad):
                pc.load_agent_config(self.write_config(self.root / "bad-vt.json", vector_tanh_dir=bad))
        self.assertTrue(pc._overlap(Path(self.pins).anchor, str(self.pins)), "the filesystem root lies above all")
        if POSIX:
            os.symlink(records, self.root / "vt-link")
            with self.assertRaisesRegex(ValueError, "vector_tanh_dir"):
                pc.load_agent_config(self.write_config(self.root / "bad-vt.json", vector_tanh_dir=str(self.root / "vt-link")))

    def test_hashing_reports_progress_for_the_runners_heartbeat(self):
        path = self.root / "big.bin"
        path.write_bytes(b"x" * (3 << 20))
        calls = []
        self.assertEqual(pc._hash_nofollow(str(path), lambda: calls.append(1)), pc.sha256_file(path))
        self.assertEqual(len(calls), 3, "one call per block")

    @unittest.skipIf(POSIX, "the refusal applies off POSIX")
    def test_the_spool_executor_needs_a_posix_host(self):
        with self.assertRaisesRegex(ValueError, "POSIX"):
            pc.load_agent_config(self.write_config(self.root / "spool.json", executor={"kind": "spool"}))

    @unittest.skipUnless(POSIX, "the spool executor is POSIX only")
    def test_spool_defaults(self):
        executor = pc.load_agent_config(self.write_config(self.root / "spool.json", executor={"kind": "spool"}))[
            "executor"]
        spool = os.path.join(str(self.scratch), ".spool")
        self.assertEqual(executor, {"kind": "spool", "heartbeat_seconds": 10, "stop_grace_seconds": 300,
                                    "watchdog_grace_seconds": 30, "stop_file": os.path.join(spool, "STOP"),
                                    "end_file": os.path.join(spool, "END.json")})


@unittest.skipUnless(POSIX, "the box runner and the spool executor are POSIX only")
class RunnerCase(AgentCase):
    def setUp(self):
        super().setUp()
        self.collector.write_text(SLOW_COLLECTOR)
        self.config_path = self.write_config(self.root / "agent.json")  # the direct executor
        self.spool = self.scratch / ".spool"
        self.spool_config = self.spool_config_at(self.root / "spool.json")
        self.runners = []

    def spool_config_at(self, path, **executor):
        return self.write_config(path, executor=dict({"kind": "spool", "heartbeat_seconds": 5,
                                                      "stop_grace_seconds": 5}, **executor))

    def tearDown(self):
        for process in self.runners:
            if process.poll() is None:
                process.kill()
                process.wait()
        super().tearDown()

    def start_runner(self, config=None, wait=True):
        log = open(self.root / f"runner-{len(self.runners)}.log", "wb")
        process = subprocess.Popen([sys.executable, "-I", str(HERE / "pooled_collect.py"), "runner",
                                    str(config or self.spool_config)], stdout=subprocess.DEVNULL, stderr=log)
        log.close()
        self.runners.append(process)
        if wait:
            def serving():
                try:
                    return json.loads((self.spool / "runner.heartbeat").read_text())["pid"] == process.pid
                except (OSError, ValueError, KeyError):
                    return process.poll() is not None
            self.assertTrue(wait_for(serving), "the runner never started")
            self.assertIsNone(process.poll(), self.runner_log(process))
        return process

    def runner_log(self, process):
        return (self.root / f"runner-{self.runners.index(process)}.log").read_text(errors="replace")

    def stop_runner(self, process):
        (self.spool / "STOP").write_text("")
        process.wait(60)
        os.unlink(self.spool / "STOP")
        return process.returncode, json.loads((self.spool / "END.json").read_text())

    def spool_leftovers(self):
        return sorted(n for n in os.listdir(self.spool) if n not in ("runner.lock", "runner.heartbeat", "END.json"))

    def in_thread(self, **kwargs):
        """run_shard by an agent in a thread (no request alarm off the main thread)."""
        box = {}
        thread = threading.Thread(target=lambda: box.setdefault("result", self.run_shard(**kwargs)), daemon=True)
        thread.start()
        return thread, box

    def agent_host(self, name, arch, config_path, scratch=None):
        fake_ssh = self.root / "fake_ssh.py"
        fake_ssh.write_text(FAKE_SSH)
        return pc.Host(name=name, kind="ssh", collector=str(self.collector),
                       collector_sha256=pc.sha256_file(self.collector), workers=2, weight=1.0,
                       scratch=str(scratch or self.scratch), arch=arch,
                       ssh=[sys.executable, str(fake_ssh), str(config_path)], prefix=[sys.executable],
                       transport="agent")

    def learner_command(self, count=9):
        pinned = self.pins / "run" / "checkpoint.json"
        if not pinned.exists():
            pinned.parent.mkdir()
            pinned.write_bytes(b"policy")
        self.episodes = [{"id": f"e{i}", "seed": 100 + i} for i in range(count)]
        command = {"mode": "collect_parallel", "workers": 2, "episodes": self.episodes,
                   "source": {"checkpoint": {"path": str(pinned), "sha256": pc.sha256_file(pinned)}},
                   "output_directory": "unused"}
        attempt = self.root / "learner-run" / "iterations" / "000003" / "attempt-000000"
        attempt.mkdir(parents=True, exist_ok=True)
        path = attempt / "collect-command.json"
        path.write_text(json.dumps(command))
        return path


class SpoolServiceTests(RunnerCase):
    def test_a_spooled_shard_has_exactly_the_direct_executors_bytes(self):
        self.put_shard()
        code, out, err = self.run_shard()
        self.assertEqual((code, json.loads(out)["code"]), (0, 0), err)
        code, direct, err = self.serve("fetch_shard", tag=TAG)
        self.assertEqual(code, 0, err)
        self.assertEqual(json.loads(self.serve("remove_shard", tag=TAG)[1]), "removed")
        runner = self.start_runner()
        self.put_shard()  # the same tag, scratch and command: the collector sees identical inputs and argv
        code, out, err = self.run_shard(config=self.spool_config)
        self.assertEqual((code, json.loads(out)["code"]), (0, 0), err)
        code, spooled, err = self.serve("fetch_shard", tag=TAG, config=self.spool_config)
        self.assertEqual(code, 0, err)
        self.assertEqual(tar_files(spooled), tar_files(direct))
        self.assertEqual(sorted(tar_files(spooled)), ["collection.json", "episode-0002.json", "episode-0003.json"])
        self.assertEqual(json.loads(self.serve("remove_shard", tag=TAG, config=self.spool_config)[1]), "removed")
        self.assertEqual(self.spool_leftovers(), [], "every request, claim, result and lock removed")
        self.assertEqual(os.listdir(self.scratch / ".run"), [], "the private command copy is gone")
        code, end = self.stop_runner(runner)
        self.assertEqual(code, 0, self.runner_log(runner))
        self.assertEqual((end["schema"], end["reason"], end["jobs_run"], end["jobs_failed"]),
                         (pc.RUNNER_END_SCHEMA, "stop-file", 1, 0))
        self.assertTrue(end["started_utc"] <= end["ended_utc"])
        self.assertFalse((self.spool / "runner.heartbeat").exists())

    def test_the_dispatcher_collects_through_spooled_agents_with_the_direct_bytes(self):
        def capture(store):
            def merge(command_path, directories):
                for directory in directories:
                    shard = directory.name.rsplit("-", 1)[1]
                    for path in sorted(directory.iterdir()):
                        if path.name == "collection.json":  # its recorded paths name the run's own tag
                            document = json.loads(path.read_text())
                            store[shard, path.name] = ([t["sha256"] for t in document["trajectories"]],
                                                       document["shard"])
                        else:
                            store[shard, path.name] = path.read_bytes()
                return fake_merge(command_path, directories)
            return merge

        command_path = self.learner_command()
        stores, receipts = [], []
        runner = self.start_runner()
        for config in (self.config_path, self.spool_config):
            hosts = [self.agent_host("a", "x86", config), self.agent_host("b", "arm64", config)]
            stores.append({})
            dispatcher = pc.Dispatcher({"hosts": hosts, "audit": {"every": 1, "episodes": 2}}, timeout=100)
            receipts.append(dispatcher.collect(command_path, capture(stores[-1])))
        self.assertEqual(stores[0], stores[1])
        self.assertEqual(len(stores[1]), 2 + len(self.episodes), stores[1].keys())  # two shards
        for receipt in receipts:
            self.assertEqual(receipt["merged"]["slots"], [episode_bytes(e).decode() for e in self.episodes])
            self.assertEqual(receipt["failed_hosts"], [])
            audits = [e for e in receipt["events"] if e["event"] == "audit"]
            self.assertEqual(len(audits), 2)
            self.assertTrue(all(e["equal"] for e in audits))
        self.assertEqual([n for n in os.listdir(self.scratch) if not n.startswith(".")], [])
        self.assertEqual(self.spool_leftovers(), [])
        code, end = self.stop_runner(runner)
        self.assertEqual((code, end["jobs_run"], end["jobs_failed"]), (0, 4, 0), self.runner_log(runner))

    def test_without_a_live_runner_run_shard_refuses_at_once_and_the_dispatcher_fails_over(self):
        boxless_scratch = self.root / "boxless-scratch"
        boxless_scratch.mkdir()
        boxless = self.spool_config_at(self.root / "boxless.json")
        document = json.loads(boxless.read_text())
        boxless.write_text(json.dumps(dict(document, scratch=str(boxless_scratch), executor=dict(
            document["executor"], stop_file=str(boxless_scratch / ".spool" / "STOP"),
            end_file=str(boxless_scratch / ".spool" / "END.json")))))
        hosts = [self.agent_host("boxless", "x86", boxless, boxless_scratch), self.agent_host("b", "x86", self.config_path)]
        started = time.monotonic()
        receipt = pc.Dispatcher({"hosts": hosts}, timeout=100).collect(self.learner_command(4), fake_merge)
        self.assertLess(time.monotonic() - started, 60)
        self.assertEqual(receipt["merged"]["slots"], [episode_bytes(e).decode() for e in self.episodes])
        self.assertEqual(receipt["failed_hosts"], ["boxless"])
        failure = [e for e in receipt["events"] if e["event"] == "host-failed"]
        self.assertIn("no box runner", failure[0]["error"])
        self.assertEqual([n for n in os.listdir(boxless_scratch) if not n.startswith(".")], [], "nothing created")
        # A stale heartbeat, or a runner that is stopping, refuses the same way, before the run directory exists.
        command = self.shard_command(output_directory=str(boxless_scratch) + "/" + TAG)
        self.assertEqual(self.serve("put_shard", json.dumps(command).encode(), tag=TAG, config=boxless)[0], 0)
        spool = boxless_scratch / ".spool"
        spool.mkdir(mode=0o700, exist_ok=True)
        heartbeat = spool / "runner.heartbeat"
        heartbeat.write_text("{}")
        os.utime(heartbeat, (time.time() - 100, time.time() - 100))
        self.assertRefused(self.run_shard(config=boxless), "heartbeat is")
        os.utime(heartbeat)
        (spool / "STOP").write_text("")
        self.assertRefused(self.run_shard(config=boxless), "stopping")
        self.assertFalse((boxless_scratch / TAG).exists(), "nothing ran")

    def test_one_runner_per_spool_and_a_refused_second_one_leaves_the_first_serving(self):
        first = self.start_runner()
        second = self.start_runner(wait=False)
        self.assertEqual(second.wait(30), 1)
        self.assertIn("another box runner", self.runner_log(second))
        self.assertFalse((self.spool / "END.json").exists(), "the end record is the live runner's")
        time.sleep(3)  # beyond a beat (heartbeat_seconds / 4): the first runner must still own its heartbeat
        self.assertIsNone(first.poll(), self.runner_log(first))
        self.assertEqual(json.loads((self.spool / "runner.heartbeat").read_text())["pid"], first.pid)
        self.put_shard()
        code, out, err = self.run_shard(config=self.spool_config)
        self.assertEqual((code, json.loads(out)["code"]), (0, 0), err)
        code, end = self.stop_runner(first)
        self.assertEqual((code, end["reason"], end["jobs_run"]), (0, "stop-file", 1))

    def in_thread_tag(self, tag, config, seconds=60):
        box = {}
        thread = threading.Thread(target=lambda: box.setdefault("result", self.serve(
            "run_shard", tag=tag, seconds=seconds, config=config, collector_sha256=pc.sha256_file(self.collector))),
            daemon=True)
        thread.start()
        return thread, box

    def put_tag(self, tag, **changes):
        command = self.shard_command(output_directory=str(self.scratch) + "/" + tag, **changes)
        code, out, err = self.serve("put_shard", json.dumps(command).encode(), tag=tag)
        self.assertEqual(code, 0, err)

    def test_the_runner_itself_runs_at_most_max_concurrent_runs(self):
        one = self.spool_config_at(self.root / "one.json")  # the agent's config allows 2; the runner's 1
        one.write_text(json.dumps(dict(json.loads(one.read_text()), max_concurrent_runs=1)))
        runner = self.start_runner(one)
        tag2 = TAG.replace("-s001", "-s002")
        self.put_tag(TAG, episodes=[{"id": "e2", "seed": 7, "slow": 3}])
        self.put_tag(tag2)
        first, first_box = self.in_thread_tag(TAG, self.spool_config)
        self.assertTrue(wait_for(lambda: (self.spool / f"{TAG}.running").exists()))
        second, second_box = self.in_thread_tag(tag2, self.spool_config)
        self.assertTrue(wait_for(lambda: (self.spool / f"{tag2}.request").exists()))
        time.sleep(1)
        self.assertTrue((self.spool / f"{TAG}.running").exists())
        self.assertTrue((self.spool / f"{tag2}.request").exists() and not (self.spool / f"{tag2}.running").exists(),
                        "queued until the first job's group is reaped")
        for thread, box in ((first, first_box), (second, second_box)):
            thread.join(60)
            code, out, err = box["result"]
            self.assertEqual((code, json.loads(out)["code"]), (0, 0), err)
        self.assertEqual(self.stop_runner(runner)[1]["jobs_run"], 2)

    def test_a_deadline_that_expires_while_queued_is_refused(self):
        one = self.spool_config_at(self.root / "one.json")
        one.write_text(json.dumps(dict(json.loads(one.read_text()), max_concurrent_runs=1)))
        runner = self.start_runner(one)
        tag2 = TAG.replace("-s001", "-s002")
        self.put_tag(TAG, episodes=[{"id": "e2", "seed": 7, "slow": 4}])
        self.put_tag(tag2)
        first, first_box = self.in_thread_tag(TAG, self.spool_config)
        self.assertTrue(wait_for(lambda: (self.spool / f"{TAG}.running").exists()))
        second, second_box = self.in_thread_tag(tag2, self.spool_config, seconds=1)
        second.join(60)
        self.assertRefused(second_box["result"], "deadline passed while it was queued")
        self.assertFalse((self.scratch / tag2 / "out").exists(), "never started")
        first.join(60)
        self.assertEqual(json.loads(first_box["result"][1])["code"], 0)
        code, end = self.stop_runner(runner)
        self.assertEqual((end["jobs_run"], end["jobs_refused"]), (1, 1))

    def test_a_full_queue_is_refused_at_once(self):
        narrow = self.spool_config_at(self.root / "narrow.json")
        narrow.write_text(json.dumps(dict(json.loads(narrow.read_text()), max_concurrent_runs=1)))
        self.spool.mkdir(mode=0o700)
        pc._spool_write(str(self.spool / "runner.heartbeat"), {"generation": "a" * 32})  # a fresh runner
        (self.spool / f"{TAG.replace('-s001', '-s002')}.request").write_text("{}")  # one job already queued
        self.put_shard()
        self.assertRefused(self.run_shard(config=narrow), "queue is full")
        self.assertFalse((self.spool / f"{TAG}.request").exists() or (self.spool / f"{TAG}.lock").exists())

    def test_removal_racing_submission_and_claim_never_strands_a_job(self):
        runner = self.start_runner()
        for round_ in range(4):
            self.serve("remove_shard", tag=TAG, config=self.spool_config)
            self.put_shard(episodes=[{"id": "e2", "seed": 7, "slow": 0.3}])
            started = time.monotonic()
            thread, box = self.in_thread(config=self.spool_config)
            removals, fetches = [], []
            while thread.is_alive():
                fetches.append(self.serve("fetch_shard", tag=TAG, config=self.spool_config)[0])
                removals.append(self.serve("remove_shard", tag=TAG, config=self.spool_config)[0])
            thread.join()
            self.assertLess(time.monotonic() - started, 20, "a job is answered or refused, never left waiting")
            code, out, err = box["result"]
            self.assertIn(code, (0, 2), err)
            if code == 0:
                self.assertEqual(json.loads(out)["code"], 0, round_)
            self.assertTrue(set(removals) <= {0, 2} and set(fetches) <= {0, 2}, (removals, fetches))
            self.assertIsNone(runner.poll(), self.runner_log(runner))
        self.assertTrue(wait_for(lambda: self.serve("remove_shard", tag=TAG, config=self.spool_config)[0] == 0))
        self.assertTrue(wait_for(lambda: self.spool_leftovers() == [], 15), self.spool_leftovers())
        self.assertEqual(self.stop_runner(runner)[0], 0)

    def check(self, config, *extra, command="runner-check"):
        result = subprocess.run([sys.executable, "-I", str(HERE / "pooled_collect.py"), command, str(config), *extra],
                                capture_output=True, text=True, timeout=60)
        return result.returncode, json.loads(result.stdout or "{}")

    def test_runner_check_reports_the_heartbeat_for_root_and_refuses_odd_configs(self):
        self.assertEqual(self.check(self.spool_config)[0], 3)  # nothing ran
        runner = self.start_runner()
        code, report = self.check(self.spool_config)
        self.assertEqual((code, report["state"], report["pid"], report["runner_lock"], report["lock_pid"]),
                         (0, "fresh", runner.pid, "held", runner.pid))
        self.stop_runner(runner)
        self.assertEqual(self.check(self.spool_config)[1]["state"], "stopped")
        heartbeat = self.spool / "runner.heartbeat"
        heartbeat.write_text("{}")  # a dead runner's leftover: no runner holds the lock, nothing to kill
        os.utime(heartbeat, (time.time() - 100, time.time() - 100))
        code, report = self.check(self.spool_config)
        self.assertEqual((code, report["state"], report["runner_lock"]), (3, "stopped", "free"))
        self.assertEqual(self.check(self.spool_config, "1000")[0], 0)
        self.assertEqual(self.check(self.spool_config, "-1")[0], 2)
        document = json.loads(self.spool_config.read_text())
        for bad in ([], dict(document, executor="spool"), dict(document, executor=dict(document["executor"],
                    heartbeat_seconds=True)), dict(document, executor=dict(document["executor"], heartbeat_seconds=0)),
                    dict(document, scratch="relative"), dict(document, scratch=str(self.scratch) + "/../scratch")):
            path = self.root / "odd.json"
            path.write_text(json.dumps(bad))
            self.assertEqual(self.check(path)[0], 2, bad)
        (self.root / "deep.json").write_text("[" * 100000 + "]" * 100000)
        self.assertEqual(self.check(self.root / "deep.json")[0], 2)
        # Missing config or scratch: unusable, never "stopped".
        self.assertEqual(self.check(self.root / "no-such-config.json")[0], 2)
        path = self.root / "odd.json"
        path.write_text(json.dumps(dict(document, scratch=str(self.root / "no-such-scratch"))))
        self.assertEqual(self.check(path)[0], 2)
        # A config others can write, or reached through a symlink, is refused.
        path.write_text(json.dumps(document))
        path.chmod(0o666)
        self.assertEqual(self.check(path)[0], 2)
        path.chmod(0o644)
        os.symlink(path, self.root / "config-link.json")
        self.assertEqual(self.check(self.root / "config-link.json")[0], 2)
        self.assertEqual(self.check(path)[0], 3)
        # A spool other accounts can reach is refused.
        self.spool.chmod(0o755)
        self.assertEqual(self.check(path)[0], 2)
        self.spool.chmod(0o700)
        # An ancestor of the scratch root swapped for a symlink is never followed.
        alias = self.root / "alias"
        os.symlink(self.root, alias)
        path.write_text(json.dumps(dict(document, scratch=str(alias / "scratch"))))
        self.assertEqual(self.check(path)[0], 2)
        # A symlinked spool is never followed (root reads the collector account's tree).
        os.rename(self.spool, self.root / "real-spool")
        os.symlink(self.root / "real-spool", self.spool)
        self.assertEqual(self.check(self.spool_config)[0], 2)

    def test_runner_watch_tells_root_to_kill_only_a_held_stale_unfrozen_runner_twice(self):
        quick = self.spool_config_at(self.root / "quick.json", heartbeat_seconds=1)
        cgroup = self.root / "cgroup"
        cgroup.mkdir()
        (cgroup / "cgroup.freeze").write_text("0\n")
        (cgroup / "cgroup.events").write_text("populated 1\nfrozen 0\n")
        args = ("--plan", "/p.json", "--chain", "c1", "--cgroup", str(cgroup), "--confirm-seconds", "1")

        def watch(*extra):
            return self.check(quick, *args, *extra, command="runner-watch")
        self.assertEqual(self.check(quick, "--plan", "/p.json", "--chain", "c1", command="runner-watch")[0], 2,
                         "--cgroup is required")
        self.assertEqual(watch()[0], 3, "nothing ran")
        runner = self.start_runner(quick)
        (cgroup / "cgroup.procs").write_text(f"{runner.pid}\n")  # the runner is the chain's process
        self.assertEqual(watch()[0], 0)
        runner.send_signal(signal.SIGSTOP)  # hung: holds the lock, beats no more
        try:
            time.sleep(2)
            code, report = watch()
            self.assertEqual((code, report["state"]), (1, "stale"), report)
            self.assertEqual(report["kill_command"][1:],
                             ["-I", str(HERE / "w5box.py"), "kill", "--plan", "/p.json", "--chain", "c1"])
            (cgroup / "cgroup.procs").write_text("1\n")  # the lock's holder is not in this chain
            code, report = watch()
            self.assertEqual((code, report["state"], "kill_command" in report), (6, "stale-chain-foreign", False))
            (cgroup / "cgroup.procs").write_text(f"{runner.pid}\n")
            code, report = self.check(quick, *args[:5], str(self.root / "no-such-cgroup"), *args[6:],
                                      command="runner-watch")
            self.assertEqual((code, report["state"], "kill_command" in report), (6, "stale-chain-unknown", False))
            (cgroup / "cgroup.events").write_text("populated 1\nfrozen 1\n")
            code, report = watch()
            self.assertEqual((code, report["state"], "kill_command" in report), (4, "stale-frozen", False))
            (cgroup / "cgroup.events").write_text("populated 1\nfrozen 0\n")
            (cgroup / "cgroup.freeze").write_text("1\n")
            self.assertEqual(watch()[0], 4)
            (cgroup / "cgroup.freeze").write_text("0\n")
            # A thaw between the two readings: the runner beats again before the confirming reading.
            pending = subprocess.Popen([sys.executable, "-I", str(HERE / "pooled_collect.py"), "runner-watch",
                                        str(quick), *args[:-1], "3"], stdout=subprocess.PIPE, text=True)
            time.sleep(1)
            runner.send_signal(signal.SIGCONT)
            out, _ = pending.communicate(timeout=60)
            self.assertEqual((pending.returncode, json.loads(out)["first_reading"]), (5, "stale"), out)
            self.assertNotIn("kill_command", json.loads(out))
        finally:
            runner.send_signal(signal.SIGCONT)
        runner.send_signal(signal.SIGKILL)  # dead, its heartbeat left behind: w5box's business, not a kill
        runner.wait()
        time.sleep(1.5)
        self.assertEqual(watch()[0], 3)
        self.assertEqual(watch("--max-age", "-5")[0], 2)
        self.assertEqual(self.check(quick, *args[:-1], "0.5", command="runner-watch")[0], 2, "confirm at least 1 s")

    @unittest.skipUnless(LINUX, "/proc/locks")
    def test_the_lock_holder_is_read_by_device_inode_and_exclusive_mode(self):
        import fcntl
        path = self.root / "probe.lock"
        path.write_text("")
        holder = open(path, "rb")
        probe = os.open(path, os.O_RDONLY)
        try:
            self.assertEqual(pc._flock_holder(probe), ("free", None))
            fcntl.flock(holder.fileno(), fcntl.LOCK_SH)
            self.assertEqual(pc._flock_holder(probe), ("free", None), "a shared lock is no runner's")
            fcntl.flock(holder.fileno(), fcntl.LOCK_EX)
            self.assertEqual(pc._flock_holder(probe), ("held", os.getpid()))
            with mock.patch.object(pc.os, "major", return_value=0xfe):  # /proc and fstat disagree on the device
                self.assertEqual(pc._flock_holder(probe), ("unknown", None))
            os.link(path, self.root / "probe-link.lock")
            self.assertEqual(pc._flock_holder(probe), ("unknown", None), "a hard-linked lock is not trusted")
            os.unlink(self.root / "probe-link.lock")
            with mock.patch.object(pc.Path, "read_text", side_effect=OSError("no /proc")):
                self.assertEqual(pc._flock_holder(probe), ("unknown", None))
        finally:
            os.close(probe)
            holder.close()

    def test_a_lock_that_cannot_be_opened_is_never_a_departed_agent(self):
        with mock.patch.object(pc.os, "open", side_effect=OSError(24, "Too many open files")):
            self.assertFalse(pc._agent_gone(str(self.spool / f"{TAG}.lock")))
        self.assertTrue(pc._agent_gone(str(self.spool / f"{TAG}.lock")), "absent: gone")

    def test_the_heartbeat_exists_before_recovery_runs(self):
        runner = pc.BoxRunner(pc.load_agent_config(self.spool_config))
        seen = []

        def recover():
            seen.append((self.spool / "runner.heartbeat").exists())
            runner.signalled = True  # leave the loop at once
        runner.recover = recover
        self.assertEqual(runner.serve(), "signal")
        self.assertEqual(seen, [True])
        self.assertEqual(json.loads((self.spool / "END.json").read_text())["reason"], "signal")

    def test_sweep_survives_a_stubborn_entry_and_keeps_live_agent_copies(self):
        runner = pc.BoxRunner(pc.load_agent_config(self.spool_config))
        run = self.scratch / ".run"
        run.mkdir(mode=0o700)
        plain = run / f"{TAG}.{'a' * 32}.json"  # an agent's copy (a direct agent sharing the root has no spool state)
        plain.write_text("{}")
        snap = run / f"{TAG.replace('-s001', '-s002')}.{'b' * 32}.snap.json"  # a killed runner's snapshot
        snap.write_text("{}")
        locked = self.scratch / ".trash-stubborn" / "locked"
        locked.mkdir(parents=True)
        (locked / "f").write_text("x")
        locked.chmod(0o500)
        try:
            log = io.StringIO()
            with mock.patch.object(pc, "SPOOL_STAGE_AGE", -1), mock.patch.object(pc.sys, "stderr", log):
                runner.sweep()  # the stubborn trash fails to go; the sweep logs it and goes on
            self.assertFalse(snap.exists())
            self.assertTrue(plain.exists(), "no spool state for its tag: kept")
            if os.geteuid() != 0:
                self.assertTrue((locked / "f").exists())
                self.assertIn("PermissionError", log.getvalue())
            (self.spool / f"{TAG}.lock").write_text("")  # its agent's lock, left by a killed agent
            runner.sweep()
            self.assertFalse(plain.exists() or (self.spool / f"{TAG}.lock").exists())
        finally:
            locked.chmod(0o700)

    def test_the_runner_may_write_only_the_scratch_root(self):
        config = pc.load_agent_config(self.spool_config)
        rules = {path: access for path, access, _ in pc.runner_rules(config)}
        writable = {path for path, access in rules.items() if access & (pc.LL_WRITE_FILE | pc.LL_MAKE_REG)}
        self.assertEqual(writable, {str(self.scratch), "/dev/null"})
        self.assertEqual(rules[str(self.pins)], pc.LL_READ, "pin roots read only")
        self.assertEqual(rules[str(self.collector)], pc.LL_RUN)

    def test_the_collectors_own_alarm_ends_it_when_the_runner_does_not(self):
        config = pc.load_agent_config(self.spool_config)
        run_dir = self.scratch / TAG
        run_dir.mkdir()
        (self.scratch / ".run").mkdir()
        command = self.scratch / ".run" / f"{TAG}.{'0' * 32}.json"
        command.write_text(json.dumps(self.shard_command(episodes=[{"id": "sleep", "seed": 1}],
                                                         output_directory=str(run_dir / "out"))))
        process = pc._launch_spooled(config, str(command), str(run_dir), 0)  # clamped to 1 s; never watched
        try:
            self.assertEqual(process.wait(30), -signal.SIGALRM)
        finally:
            pids = run_dir / "out" / "pids"
            if pids.exists() and len(pids.read_text().split()) == 2:
                try:
                    os.kill(int(pids.read_text().split()[1]), signal.SIGKILL)  # the descendant: w5box's cgroup kill
                except ProcessLookupError:
                    pass
            process.stderr.close()

    def test_remove_shard_refuses_a_waiting_job_and_removes_stale_state(self):
        self.spool.mkdir(mode=0o700)
        self.put_shard()
        (self.scratch / TAG).mkdir()
        stale = self.spool / f"{TAG}.result"
        stale.write_text("{}")
        (self.spool / f"{TAG}.cancel").write_text("{}")
        (self.spool / f"{TAG}.request").write_text("{}")  # submitted by an agent that has gone
        lock = pc._spool_job_lock(str(self.spool), TAG)  # an agent still waits: nothing of the tag is removed
        try:
            self.assertRefused(self.serve("remove_shard", tag=TAG, config=self.spool_config), "is active")
            self.assertTrue(stale.exists() and (self.scratch / TAG).exists() and (self.scratch / f"{TAG}.json").exists())
        finally:
            os.unlink(self.spool / f"{TAG}.lock")
            lock.close()
        self.assertEqual(json.loads(self.serve("remove_shard", tag=TAG, config=self.spool_config)[1]), "removed")
        self.assertEqual(self.spool_leftovers(), [], "the abandoned request and the stale result and cancel")
        self.assertFalse((self.scratch / TAG).exists())

    def test_a_restarted_runner_refuses_an_earlier_generations_request_and_drops_an_abandoned_one(self):
        self.spool.mkdir(mode=0o700)
        tag2, tag3 = TAG.replace("-s001", "-s002"), TAG.replace("-s001", "-s003")
        config = pc.load_agent_config(self.spool_config)
        for tag in (tag2, tag3):
            (self.scratch / tag).mkdir()
            request = {"schema": pc.SPOOL_REQUEST_SCHEMA, "job": "1" * 32, "tag": tag, "command_sha256": "0" * 64,
                       "seconds": 60, "submitted_unix": time.time(), "generation": "2" * 32,
                       "binding": pc._execution_binding(config)}
            pc._spool_write(str(self.spool / f"{tag}.request"), request)
        waiting = pc._spool_job_lock(str(self.spool), tag2)  # tag2's agent waits; tag3's has gone
        try:
            runner = self.start_runner()
            self.assertTrue(wait_for(lambda: (self.spool / f"{tag2}.result").exists()))
            result = pc._spool_take(str(self.spool / f"{tag2}.result"), tag2, "1" * 32, "2" * 32)
            self.assertIn("earlier box runner", result["error"])
            self.assertTrue(wait_for(lambda: not (self.spool / f"{tag3}.request").exists()), "dropped")
            self.assertFalse((self.spool / f"{tag3}.result").exists(), "nobody to answer")
        finally:
            os.unlink(self.spool / f"{tag2}.lock")
            waiting.close()
        code, end = self.stop_runner(runner)
        self.assertEqual((code, end["jobs_run"], end["jobs_refused"]), (0, 0, 1))
        self.assertEqual(self.spool_leftovers(), [])

    def test_a_result_for_another_job_or_malformed_is_refused(self):
        self.spool.mkdir(mode=0o700)
        path = str(self.spool / f"{TAG}.result")
        good = {"schema": pc.SPOOL_RESULT_SCHEMA, "job": "1" * 32, "tag": TAG, "generation": "2" * 32, "code": 0,
                "stderr": "", "seconds": 1.0, "error": None}
        for change in ({"job": "3" * 32}, {"code": True}, {"code": None}, {"code": 0, "error": "x"},
                       {"stderr": "x" * (pc.AGENT_STDERR_TAIL + 1)}, {"extra": 1}, {"generation": "3" * 32},
                       {"generation": "x"}, {"seconds": float("nan")}, {"seconds": True}, {"seconds": -1}):
            pc._spool_write(path, dict(good, **change))
            with self.assertRaises(pc.AgentRefusal, msg=change):
                pc._spool_take(path, TAG, "1" * 32, "2" * 32)
            self.assertFalse(os.path.exists(path))
        Path(path).write_bytes(b"x" * (pc.SPOOL_FILE_BYTES + 1))
        with self.assertRaisesRegex(pc.AgentRefusal, "malformed"):
            pc._spool_take(path, TAG, "1" * 32, "2" * 32)
        pc._spool_write(path, good)
        os.link(path, path + ".second")
        with self.assertRaisesRegex(pc.AgentRefusal, "singly linked"):
            pc._spool_take(path, TAG, "1" * 32, "2" * 32)
        os.unlink(path + ".second")
        pc._spool_write(path, good)
        self.assertEqual(pc._spool_take(path, TAG, "1" * 32, "2" * 32)["code"], 0)

    def test_runner_main_refuses_a_privileged_launch_before_reading_its_config(self):
        for patch in ({"geteuid": 0}, {"getresuid": (1000, 1000, 0)}):
            stderr = io.StringIO()
            with mock.patch.multiple(pc.os, **{k: mock.Mock(return_value=v) for k, v in patch.items()}), \
                    mock.patch.object(pc, "_close_inherited"):
                self.assertEqual(pc.runner_main(str(self.root / "never-read.json"), stderr), 2)
            self.assertIn("never as root", stderr.getvalue())

    def test_remove_shard_moves_the_run_directory_and_leftover_copies_away(self):
        self.spool.mkdir(mode=0o700)
        (self.scratch / ".run").mkdir(mode=0o700)
        self.put_shard()
        (self.scratch / TAG / "out").mkdir(parents=True)
        (self.scratch / TAG / "out" / "x.json").write_text("x")
        for name in (f"{TAG}.{'a' * 32}.json", f"{TAG}.{'b' * 32}.json"):  # a killed agent's and runner's copies
            (self.scratch / ".run" / name).write_text("{}")
        other = self.scratch / ".run" / f"{TAG.replace('-s001', '-s002')}.{'c' * 32}.json"
        other.write_text("{}")
        self.assertEqual(json.loads(self.serve("remove_shard", tag=TAG, config=self.spool_config)[1]), "removed")
        self.assertEqual(sorted(os.listdir(self.scratch / ".run")), [other.name], "only this tag's copies")
        self.assertEqual([n for n in os.listdir(self.scratch) if not n.startswith(".")], [])
        self.assertEqual([n for n in os.listdir(self.scratch) if n.startswith(".trash-")], [])

    def test_the_collector_runs_in_its_run_directory_with_the_declared_environment(self):
        records = self.root / "vector-tanh"
        records.mkdir()
        for config, executor in ((self.spool_config, "spool"), (self.config_path, "direct")):
            for tanh in (None, str(records)):
                document = json.loads(Path(config).read_text())
                if tanh:
                    document["vector_tanh_dir"] = tanh
                variant = self.root / f"context-{executor}-{bool(tanh)}.json"
                variant.write_text(json.dumps(document))
                runner = self.start_runner(variant) if executor == "spool" else None
                self.put_shard(episodes=[{"id": "e2", "seed": 7, "context": True}])
                code, out, err = self.run_shard(config=variant)
                self.assertEqual((code, json.loads(out)["code"]), (0, 0), err)
                context = json.loads((self.scratch / TAG / "context.json").read_text())
                self.assertEqual(context["cwd"], str(self.scratch / TAG))
                if executor == "spool":
                    self.assertEqual(context["env"], pc._spool_env(pc.load_agent_config(variant)))
                    self.stop_runner(runner)
                elif tanh:
                    self.assertEqual(context["env"]["MTG_KERNEL_VECTOR_TANH_DIR"], tanh)
                else:
                    self.assertNotIn("MTG_KERNEL_VECTOR_TANH_DIR", context["env"])
                self.assertEqual(json.loads(self.serve("remove_shard", tag=TAG, config=variant)[1]), "removed")

    def test_an_agent_stops_waiting_at_its_own_deadline_when_nobody_answers(self):
        self.spool.mkdir(mode=0o700)
        heartbeat = self.spool / "runner.heartbeat"
        pc._spool_write(str(heartbeat), {"generation": "a" * 32})  # a runner that beats but never claims
        beating = threading.Event()

        def beat():
            while not beating.is_set():
                os.utime(heartbeat)
                time.sleep(0.2)
        threading.Thread(target=beat, daemon=True).start()
        try:
            self.put_shard()
            started = time.monotonic()
            with mock.patch.object(pc, "SPOOL_RESULT_GRACE", 0):
                code, out, err = self.run_shard(seconds=1, config=self.spool_config)
            self.assertEqual((code, json.loads(out)["code"]), (0, "timeout"), err)
            self.assertLess(time.monotonic() - started, 10)
        finally:
            beating.set()
        self.assertEqual([n for n in os.listdir(self.spool) if n.startswith(TAG)], [], "request withdrawn, lock gone")
        self.assertEqual(os.listdir(self.scratch / ".run"), [])

    def test_the_runner_refuses_a_privileged_or_mismatched_launch(self):
        with mock.patch.object(pc.os, "geteuid", return_value=0):
            with self.assertRaisesRegex(ValueError, "never as root"):
                pc._require_unprivileged()
        other = self.spool_config_at(self.root / "other-prefix.json")
        other.write_text(json.dumps(dict(json.loads(other.read_text()), prefix=[sys.executable, "-u"])))
        self.assertNotEqual(pc._execution_binding(pc.load_agent_config(other)),
                            pc._execution_binding(pc.load_agent_config(self.spool_config)))
        runner = self.start_runner(other)  # an agent prepared for another prefix: refused, never run
        self.put_shard()
        self.assertRefused(self.run_shard(config=self.spool_config), "execution config")
        self.assertFalse((self.scratch / TAG / "out").exists())
        self.assertEqual(self.stop_runner(runner)[1]["jobs_refused"], 1)

    def test_a_late_start_with_the_stop_file_exits_at_once(self):
        self.spool.mkdir(mode=0o700)
        (self.spool / "STOP").write_text("")
        runner = self.start_runner(wait=False)
        self.assertEqual(runner.wait(30), 0, self.runner_log(runner))
        end = json.loads((self.spool / "END.json").read_text())
        self.assertEqual((end["reason"], end["jobs_run"]), ("stop-file", 0))

    def test_the_stop_file_lets_a_running_job_finish(self):
        runner = self.start_runner(self.spool_config_at(self.root / "patient.json", stop_grace_seconds=60))
        self.put_shard(episodes=[{"id": "e2", "seed": 7, "slow": 2}])
        thread, box = self.in_thread(config=self.spool_config)
        self.assertTrue(wait_for(lambda: (self.spool / f"{TAG}.running").exists()))
        (self.spool / "STOP").write_text("")
        thread.join(60)
        code, out, err = box["result"]
        self.assertEqual((code, json.loads(out)["code"]), (0, 0), err)
        self.assertEqual(runner.wait(30), 0)
        self.assertRefused(self.run_shard(config=self.spool_config), "stopping")
        end = json.loads((self.spool / "END.json").read_text())
        self.assertEqual((end["reason"], end["jobs_run"], end["jobs_failed"]), ("stop-file", 1, 0))


@unittest.skipUnless(POSIX and LINUX, "process-group termination checked through /proc")
class SpoolTerminationTests(RunnerCase):
    def pids(self):
        path = self.scratch / TAG / "out" / "pids"
        self.assertTrue(wait_for(lambda: path.exists() and len(path.read_text().split()) == 2), "never started")
        return list(map(int, path.read_text().split()))

    def test_a_run_past_its_seconds_ends_its_whole_process_group(self):
        runner = self.start_runner()
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        started = time.monotonic()
        code, out, err = self.run_shard(seconds=2, config=self.spool_config)
        self.assertEqual((code, json.loads(out)["code"]), (0, "timeout"), err)
        self.assertLess(time.monotonic() - started, 20)
        leader, descendant = self.pids()
        self.assertTrue(gone(leader) and gone(descendant), "collector and its descendant killed")
        self.assertEqual(self.spool_leftovers(), [])
        code, end = self.stop_runner(runner)
        self.assertEqual((end["jobs_run"], end["jobs_failed"]), (1, 1))

    def test_the_request_deadline_cancels_and_the_runner_kills_the_group(self):
        runner = self.start_runner()
        agent = self.spool_config_at(self.root / "deadline.json")
        document = json.loads(agent.read_text())
        agent.write_text(json.dumps(dict(document, request_seconds=1, max_seconds=3)))
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        # As in the direct executor's test: the request alarm (4 s) fires while the agent waits, before the
        # run's own deadline (3 s after a claim delayed 2.5 s by the stalled input).
        from test_pooled_agent import SlowStdin
        started = time.monotonic()
        self.assertRefused(self.run_shard(seconds=60, config=agent, stdin=SlowStdin(2.5)), "request deadline")
        self.assertLess(time.monotonic() - started, 5.4)
        leader, descendant = self.pids()
        self.assertTrue(gone(leader) and gone(descendant), "the cancel ended the group")
        self.assertTrue(wait_for(lambda: self.spool_leftovers() == [], 30), self.spool_leftovers())
        self.assertEqual(self.stop_runner(runner)[0], 0)

    def test_an_agent_that_dies_while_waiting_has_its_run_killed(self):
        runner = self.start_runner()
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        env = dict(os.environ, SSH_ORIGINAL_COMMAND=pc.encode_request(
            "run_shard", tag=TAG, seconds=100, collector_sha256=pc.sha256_file(self.collector)))
        agent = subprocess.Popen([sys.executable, "-I", str(HERE / "pooled_collect.py"), "agent",
                                  str(self.spool_config)], env=env, stdin=subprocess.DEVNULL,
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        leader, descendant = self.pids()
        agent.send_signal(signal.SIGKILL)
        agent.wait()
        self.assertTrue(gone(leader) and gone(descendant), "a departed agent's run is killed")
        self.assertTrue(wait_for(lambda: self.spool_leftovers() == [], 30), self.spool_leftovers())
        self.assertTrue(wait_for(lambda: os.listdir(self.scratch / ".run") == [], 30), "the killed agent's copy swept")
        self.assertEqual(self.stop_runner(runner)[0], 0)

    def test_a_runner_restart_answers_an_orphaned_run_as_failed_and_never_reruns_it(self):
        patient = self.spool_config_at(self.root / "patient.json", heartbeat_seconds=60)
        first = self.start_runner(patient)
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        thread, box = self.in_thread(seconds=100, config=patient)
        leader, descendant = self.pids()
        first.send_signal(signal.SIGKILL)  # the collector dies with it (parent-death signal); w5box kills the rest
        first.wait()
        self.assertTrue(gone(leader))
        os.kill(descendant, signal.SIGKILL)
        second = self.start_runner(patient)
        thread.join(30)
        self.assertRefused(box["result"], "restarted during this run; the shard was not rerun")
        self.assertEqual(self.pids(), [leader, descendant], "not run again")
        self.assertTrue(wait_for(lambda: os.listdir(self.scratch / ".run") == [], 30), "the killed runner's snapshot")
        code, end = self.stop_runner(second)
        self.assertEqual((code, end["orphans_failed"], end["jobs_run"]), (0, 1, 0))
        self.assertEqual(self.spool_leftovers(), [])

    def test_a_running_job_is_neither_removed_nor_fetched(self):
        runner = self.start_runner()
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        thread, box = self.in_thread(seconds=3, config=self.spool_config)
        self.pids()
        self.assertRefused(self.serve("remove_shard", tag=TAG, config=self.spool_config), "is active")
        self.assertRefused(self.serve("fetch_shard", tag=TAG, config=self.spool_config), "is active")
        self.assertTrue((self.scratch / TAG / "out").exists())
        thread.join(60)
        code, out, err = box["result"]
        self.assertEqual((code, json.loads(out)["code"]), (0, "timeout"), err)
        self.assertEqual(json.loads(self.serve("remove_shard", tag=TAG, config=self.spool_config)[1]), "removed")
        self.assertEqual(self.stop_runner(runner)[0], 0)

    def test_a_cancel_kills_the_group_while_the_agent_still_waits(self):
        runner = self.start_runner()
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        thread, box = self.in_thread(seconds=100, config=self.spool_config)
        leader, descendant = self.pids()
        job = json.loads((self.spool / f"{TAG}.running").read_text())["job"]
        pc._spool_write(str(self.spool / f"{TAG}.cancel"), {"job": job})  # the agent keeps its lock meanwhile
        self.assertTrue(gone(leader) and gone(descendant), "the cancel alone ends the group")
        thread.join(30)
        code, out, err = box["result"]
        self.assertEqual((code, json.loads(out)["code"]), (0, "timeout"), err)
        self.assertEqual(self.spool_leftovers(), [])
        self.assertEqual(self.stop_runner(runner)[0], 0)

    def test_a_hung_runner_is_refused_and_its_job_killed_once_it_resumes(self):
        runner = self.start_runner(self.spool_config_at(self.root / "quick.json", heartbeat_seconds=1))
        quick = self.spool_config_at(self.root / "quick.json", heartbeat_seconds=1)
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        thread, box = self.in_thread(seconds=100, config=quick)
        leader, descendant = self.pids()
        runner.send_signal(signal.SIGSTOP)  # a hung runner (or a frozen cgroup): its heartbeat stops
        try:
            started = time.monotonic()
            thread.join(30)
            self.assertRefused(box["result"], "heartbeat is")
            self.assertLess(time.monotonic() - started, 10)
        finally:
            runner.send_signal(signal.SIGCONT)
        self.assertTrue(gone(leader) and gone(descendant), "the resumed runner honours the cancel")
        self.assertTrue(wait_for(lambda: self.spool_leftovers() == [], 30), self.spool_leftovers())
        self.assertEqual(self.stop_runner(runner)[0], 0)

    def test_a_hung_runner_that_never_resumes_still_has_its_collector_end(self):
        quick = self.spool_config_at(self.root / "watchdog.json", heartbeat_seconds=1, watchdog_grace_seconds=1)
        runner = self.start_runner(quick)
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        thread, box = self.in_thread(seconds=3, config=quick)
        leader, descendant = self.pids()
        runner.send_signal(signal.SIGSTOP)  # never continued: killed in tearDown
        try:
            self.assertTrue(gone(leader, 15), "the collector's own alarm (deadline plus grace) ended it")
            thread.join(30)
            self.assertRefused(box["result"], "heartbeat is")
        finally:
            try:
                os.kill(descendant, signal.SIGKILL)  # w5box's cgroup kill ends descendants
            except ProcessLookupError:
                pass

    def test_the_stop_file_cancels_a_running_job_at_its_grace(self):
        runner = self.start_runner(self.spool_config_at(self.root / "brief.json", stop_grace_seconds=1))
        self.put_shard(episodes=[{"id": "sleep", "seed": 1}])
        thread, box = self.in_thread(seconds=100, config=self.spool_config)
        leader, descendant = self.pids()
        (self.spool / "STOP").write_text("")
        thread.join(30)
        self.assertRefused(box["result"], "stopped (stop file, grace over)")
        self.assertEqual(runner.wait(30), 0)
        self.assertTrue(gone(leader) and gone(descendant))
        end = json.loads((self.spool / "END.json").read_text())
        self.assertEqual((end["reason"], end["jobs_run"], end["jobs_failed"]), ("stop-file", 1, 1))


@unittest.skipUnless(POSIX and LANDLOCK >= 1, "the kernel has no Landlock")
class SpoolLandlockTests(RunnerCase):
    def test_a_confined_runner_and_agent_give_the_direct_bytes(self):
        landlock = self.spool_config_at(self.root / "landlock.json")
        landlock.write_text(json.dumps(dict(json.loads(landlock.read_text()), sandbox="landlock")))
        self.put_shard()
        self.assertEqual(json.loads(self.run_shard()[1])["code"], 0)
        direct = tar_files(self.serve("fetch_shard", tag=TAG)[1])
        self.serve("remove_shard", tag=TAG)
        runner = self.start_runner(landlock)
        code, out, err = self.agent_process(landlock, "put_shard", json.dumps(self.shard_command()).encode(), tag=TAG)
        self.assertEqual(code, 0, err)
        code, out, err = self.agent_process(landlock, "run_shard", tag=TAG, seconds=60,
                                            collector_sha256=pc.sha256_file(self.collector))
        self.assertEqual((code, json.loads(out)["code"]), (0, 0), (err, out, self.runner_log(runner)))
        code, out, err = self.agent_process(landlock, "fetch_shard", tag=TAG)
        self.assertEqual(tar_files(out), direct)
        code, end = self.stop_runner(runner)
        self.assertEqual(code, 0)
        self.assertTrue(end["sandbox"].startswith("landlock-abi"), end)

    def test_the_spooled_collector_reaches_nothing_of_the_spool_or_other_tags(self):
        landlock = self.spool_config_at(self.root / "landlock.json")
        landlock.write_text(json.dumps(dict(json.loads(landlock.read_text()), sandbox="landlock")))
        runner = self.start_runner(landlock)
        other = self.scratch / ("pooled-000003-" + "b" * 32 + "-s002")
        other.mkdir()
        (other / "x.json").write_text("another shard")
        (self.scratch / ".run").mkdir(mode=0o700, exist_ok=True)
        stray = self.scratch / ".run" / f"{other.name}.{'d' * 32}.json"
        stray.write_text("{}")
        probes = [["read", str(self.spool / "runner.heartbeat")], ["write", str(self.spool / "planted")],
                  ["read", str(stray)], ["write", str(self.scratch / ".run" / "planted.json")],
                  ["read", str(other / "x.json")], ["write", str(other / "planted.json")],
                  ["write", str(self.pins / "planted.bin")], ["write", str(self.scratch / TAG / "out" / "mine.json")]]
        command = self.shard_command(episodes=[{"id": "probe", "seed": 1, "probes": probes}])
        self.assertEqual(self.agent_process(landlock, "put_shard", json.dumps(command).encode(), tag=TAG)[0], 0)
        code, out, err = self.agent_process(landlock, "run_shard", tag=TAG, seconds=60,
                                            collector_sha256=pc.sha256_file(self.collector))
        self.assertEqual((code, json.loads(out)["code"]), (0, 0), (err, out, self.runner_log(runner)))
        report = json.loads((self.scratch / TAG / "out" / "probe.json").read_text())
        self.assertEqual({k for k, v in report.items() if v == "allowed"},
                         {"write:" + str(self.scratch / TAG / "out" / "mine.json")}, report)
        self.assertTrue(all(v in ("allowed", 13) for v in report.values()), report)
        self.assertEqual(self.stop_runner(runner)[0], 0)

    def test_the_confined_runner_cannot_write_the_pin_roots(self):
        landlock = self.spool_config_at(self.root / "landlock.json")
        landlock.write_text(json.dumps(dict(json.loads(landlock.read_text()), sandbox="landlock")))
        (self.pins / "held.bin").write_bytes(b"pinned")
        probes = [["pin", str(self.pins / "planted.bin"), "wb"], ["scratch", str(self.scratch / "ok.bin"), "wb"],
                  ["read_pin", str(self.pins / "held.bin"), "rb"]]
        script = self.root / "probe_runner.py"
        script.write_text(
            "import json, sys\n"
            f"sys.path.insert(0, {str(HERE)!r})\n"
            "import pooled_collect as pc\n"
            f"config = pc.load_agent_config(pc.Path({str(landlock)!r}))\n"
            "pc.agent_sandbox(config, pc.runner_rules(config))\n"
            "report = {}\n"
            f"for name, path, mode in {probes!r}:\n"
            "    try:\n"
            "        open(path, mode).close()\n"
            "        report[name] = 'allowed'\n"
            "    except OSError as error:\n"
            "        report[name] = error.errno\n"
            "print(json.dumps(report))\n")
        result = subprocess.run([sys.executable, "-I", str(script)], capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), {"pin": 13, "scratch": "allowed", "read_pin": "allowed"})
        self.assertFalse((self.pins / "planted.bin").exists())


if __name__ == "__main__":
    unittest.main()
