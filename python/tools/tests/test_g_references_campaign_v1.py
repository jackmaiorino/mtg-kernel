"""G(i) campaign driver pieces that need no reservation or native execution."""
import gzip
import hashlib
import json
import os
import shutil
import signal
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

import g_references_campaign_v1 as driver
import g_references_v1 as g
from test_nine_deck_campaign_v1 import lease_guard_files

REPO = Path(__file__).resolve().parents[3]


def save(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return {"path": str(path), "sha256": hashlib.sha256(data).hexdigest()}


class DriverTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        driver.VOLUMES.clear()  # the shared volume walks start empty in every test
        self.root = Path(self.temp.name)
        self.play = {run: {"path": f"C:/in/{run}/play-import.json", "sha256": str(index) * 64}
                     for index, run in enumerate(g.RUNS, 1)}
        raw = {"recipe": "g-references-v1", "lane": "gi", "decks": str(REPO / "data" / "runtime_decks_v1.json"),
               "play_imports": self.play, "placement": {"host": "desktop", "workers": 8, "preparation_workers": 8},
               "runtime": {"path": "r", "sha256": "r" * 64}, "storage": {"max_logical_bytes": 120 * 10**9},
               "wall_seconds": 10,
               "choice": {"path": "c"}, "choice_verification": {"path": "v"}, "cold_keep_blocks": 1,
               "roots": {name: str(self.root / name) for name in ("state", "cold", "retained", "checkpoints",
                                                                    "exposure", "hot", "doses")} | {"hot_by_run": {}}}
        self.path = self.root / "campaign.json"
        self.write(raw)

    def write(self, raw):
        self.path.write_text(json.dumps(raw), encoding="utf-8")
        self.campaign = driver.Campaign(self.path)

    def kept(self, run, blocks):
        state = {"run": run, "status": "running", "blocks": {}}
        for block in blocks:
            data = json.dumps({"state_sha256": f"{block:064d}", "block": block}).encode()
            item = save(driver.checkpoint_path(self.campaign, run, block), data)
            state["blocks"][str(block)] = {"attempts": [], "done": True, "final_checkpoint": item,
                                           "checkpoint_state": "kept", "final_adam_step": block * 162}
        return state

    def test_recipe_and_stop_block(self):
        self.assertEqual(self.campaign.stop_after, 494)
        raw = json.loads(self.path.read_text())
        raw["recipe"] = "nine-deck-baseline-v1"
        with self.assertRaises(SystemExit):
            self.write(raw)
        raw["recipe"], raw["stop_after_block"] = "g-references-v1", 742
        with self.assertRaises(SystemExit):
            self.write(raw)
        raw["stop_after_block"] = 2
        self.write(raw)
        self.assertEqual(self.campaign.stop_after, 2)

    def test_block_specs_chain_and_draw_history(self):
        state = self.kept("G2", range(1, 20))
        first = driver.block_spec(self.campaign, "G2", 1, {"blocks": {}}, self.root / "n")
        self.assertIsNone(first["start_checkpoint"])
        self.assertEqual(first["history"], {})
        self.assertEqual(first["play_import"], self.play["G2"])
        spec = driver.block_spec(self.campaign, "G2", 20, state, self.root / "n")
        self.assertEqual(spec["start_checkpoint"], state["blocks"]["19"]["final_checkpoint"])
        self.assertEqual(sorted(map(int, spec["history"])), [1, 2, 4, 8, 16])
        config = g.block_config(spec)
        self.assertEqual(config["initial_source"]["checkpoint"], state["blocks"]["19"]["final_checkpoint"])
        self.assertEqual(len(config["opponents"]), 5)
        state["blocks"]["16"]["checkpoint_state"] = "compressed"
        with self.assertRaises(RuntimeError):
            driver.block_spec(self.campaign, "G2", 20, state, self.root / "n")

    def test_request_uses_the_own_history_family_and_choice_pins(self):
        request = driver.request_for(self.campaign, "G1", {"path": "x", "sha256": "y"}, self.root / "a", self.root / "b")
        self.assertEqual(request["schedule_family"], "permuted-units-own-history-v1")
        self.assertEqual(request["non_natural_tolerance"], 0.2)
        self.assertEqual(request["choice"], {"path": "c"})
        pins = self.root / "choice-pins.json"
        pins.write_text(json.dumps({"choice": {"path": "c2"}, "choice_verification": {"path": "v2"}}))
        raw = json.loads(self.path.read_text())
        raw["choice_pins"] = str(pins)
        self.write(raw)
        request = driver.request_for(self.campaign, "G1", {"path": "x", "sha256": "y"}, self.root / "a", self.root / "b")
        self.assertEqual((request["choice"], request["choice_verification"]), ({"path": "c2"}, {"path": "v2"}))

    def test_ancestry_is_compressed_without_a_receipt(self):
        state = self.kept("G1", range(1, 21))
        driver.retire(self.campaign, "G1", 20, state)
        states = {int(k): v["checkpoint_state"] for k, v in state["blocks"].items()}
        self.assertEqual({b for b, s in states.items() if s == "kept"}, {1, 2, 4, 8, 16, 19, 20})
        self.assertEqual({b for b, s in states.items() if s == "compressed"},
                         set(range(1, 19)) - {1, 2, 4, 8, 16})
        entry = state["blocks"]["5"]
        self.assertFalse(Path(entry["final_checkpoint"]["path"]).exists())
        with gzip.open(entry["compressed_checkpoint"]["path"], "rb") as stream:
            self.assertEqual(hashlib.sha256(stream.read()).hexdigest(), entry["final_checkpoint"]["sha256"])
        log = [json.loads(line) for line in self.campaign.prune_log.read_text().splitlines()]
        five = next(row for row in log if row["block"] == 5)
        self.assertEqual(five["regenerate"]["from_permanent_block"], 4)
        self.assertEqual(five["checkpoint_sha256"], entry["final_checkpoint"]["sha256"])
        # A second pass changes nothing without a receipt.
        driver.retire(self.campaign, "G1", 20, state)
        self.assertEqual(len(self.campaign.prune_log.read_text().splitlines()), len(log))

    def test_ancestry_is_deleted_under_a_receipt(self):
        state = self.kept("G1", range(1, 8))
        driver.retire(self.campaign, "G1", 7, state)
        self.assertEqual(state["blocks"]["3"]["checkpoint_state"], "compressed")
        bad = save(self.root / "receipt.json", json.dumps({"schema": driver.RECEIPT_SCHEMA, "byte_identical": False,
                                                           "runtime": "r" * 64}).encode())
        raw = json.loads(self.path.read_text())
        raw["ancestry_receipt"] = bad
        self.write(raw)
        with self.assertRaises(SystemExit):
            driver.retire(self.campaign, "G1", 7, state)
        good = save(self.root / "receipt2.json", json.dumps({"schema": driver.RECEIPT_SCHEMA, "byte_identical": True, "interrupted": True,
                                                             "recipe": "g-references-v1", "host": "desktop",
                                                             "runtime": "r" * 64}).encode())
        raw["ancestry_receipt"] = good
        self.write(raw)
        driver.retire(self.campaign, "G1", 7, state)
        states = {int(k): v["checkpoint_state"] for k, v in state["blocks"].items()}
        self.assertEqual(states, {1: "kept", 2: "kept", 3: "deleted", 4: "kept", 5: "deleted", 6: "kept", 7: "kept"})
        log = [json.loads(line) for line in self.campaign.prune_log.read_text().splitlines()]
        deleted = [row for row in log if row.get("pruned")]
        self.assertEqual(sorted(row["block"] for row in deleted), [3, 5])
        self.assertTrue(all(row["receipt"] == {"path": good["path"], "sha256": good["sha256"]} for row in deleted))
        self.assertTrue(all(row["state_sha256"] is None or len(row["state_sha256"]) == 64 for row in deleted))
        raw["ancestry_receipt"]["sha256"] = "0" * 64
        self.write(raw)
        with self.assertRaises(ValueError):
            driver.receipt(self.campaign)

    def test_dose_record(self):
        state = self.kept("G3", [247])
        record = driver.write_dose(self.campaign, "G3", 247, state["blocks"]["247"])
        value = json.loads(Path(record["path"]).read_text())
        self.assertEqual(Path(record["path"]).name, "G3-b0247.json")
        self.assertEqual((value["schema"], value["dose"], value["block"], value["updates"]),
                         ("g-references-v1/dose", "D_a", 247, 40014))
        self.assertEqual(value["source"]["play_import"], self.play["G3"])
        self.assertEqual(value["source"]["checkpoint"], state["blocks"]["247"]["final_checkpoint"])
        self.assertEqual(value["source"]["feature_transfer"], g.FEATURE_TRANSFER)

    def test_fresh_embedding_reads_the_initialization_payload(self):
        values = [0x3F800000 + index for index in range(3 * 2)]
        payload = struct.pack("<2I", 7, 7) + struct.pack(f"<{len(values)}I", *values)
        parameters = save(self.root / "init" / "parameters.f32le", payload)
        manifest = {"parameters": [{"name": "other", "byte_offset": 0, "byte_count": 8, "shape": [2]},
                                   {"name": "card_embedding.weight", "byte_offset": 8, "byte_count": 24,
                                    "shape": [3, 2]}]}
        initialization = save(self.root / "init" / "initialization.json", json.dumps(manifest).encode())
        descriptor = save(self.root / "init" / "play-import.json", json.dumps(
            {"schema": g.FRESH_SOURCE_SCHEMA, "initialization": initialization, "parameters": parameters}).encode())
        tensor = driver.fresh_embedding(descriptor)["parameters"][0]
        self.assertEqual((tensor["name"], tensor["shape"], tensor["values"]), ("card_embedding.weight", [3, 2], values))

    def test_completed_updates_counts_update_records(self):
        native = self.root / "native"
        self.assertEqual(driver.completed_updates(native), 0)
        for update in range(3):
            folder = native / "iterations" / f"{update:06d}" / "attempt-000000"
            (folder / "collect").mkdir(parents=True)
            if update < 2:
                (folder / "update").mkdir()
                (folder / "update" / "update.json").write_text("{}")
        self.assertEqual(driver.completed_updates(native), 2)

    def test_lease_activity_counts_updates_steps_and_cpu_work(self):
        guard = self.root / "guard"
        lease_guard_files(guard)
        native = self.root / "hot" / "native"
        dispatch = self.root / "hot" / "dispatch"
        dispatch.mkdir(parents=True)
        (dispatch / "started.json").write_text(json.dumps({"pid": 2 ** 22 + 7}))  # native child not readable
        with patch.dict(os.environ, {"RUNPOD_POD_ID": "testpod", "MTG_LEASE_GUARD_DIR": str(guard)}):
            progress = driver.LeaseProgress(self.campaign)
        attempt = {"root": str(dispatch), "native": str(native),
                   "dispatch": {"state": "dispatched", "pid": 900, "token": "t900"}}
        progress.active["G1"] = {"attempts": [attempt]}
        me, clock, ticks = os.getpid(), {"wall": 2000., "cpu": 10.}, {"worker": 0}

        def table():  # the reservation supervisor (900) and its fingerprint/archive worker (901)
            return {me: (1, 0), 900: (me, 0), 901: (900, ticks["worker"])}

        def beat(wall=15., cpu=0., worker=0):
            clock["wall"] += wall; clock["cpu"] += cpu; ticks["worker"] += worker
            with patch.object(driver.time, "time", return_value=clock["wall"]),                     patch.object(driver.base.time, "time", return_value=clock["wall"]),                     patch.object(driver.base, "driver_cpu_seconds", return_value=clock["cpu"]),                     patch.object(driver.base, "process_table", side_effect=table), \
                    patch.object(driver.base, "supervisor_owned", side_effect=lambda pid, token: token == "t900"):
                progress.beat()
            return json.loads((guard / "progress.json").read_text())
        progress.cpu, progress.activity = (clock["wall"], clock["cpu"]), 0
        beat()
        self.assertEqual(beat()["last_activity_epoch"], 0)                     # a beat alone is not activity
        folder = native / "iterations" / "000000" / "attempt-000000" / "update"
        folder.mkdir(parents=True)
        (folder / "update.json").write_text("{}")
        record = beat()
        self.assertEqual((record["last_activity_epoch"], record["completed_updates"]), (clock["wall"], 1))
        progress.activity = 0
        (dispatch / "execution.json").write_text("{}")                        # the native part ended
        self.assertEqual(beat()["last_activity_epoch"], 0)                     # the supervised baseline
        self.assertEqual(beat(worker=4)["last_activity_epoch"], clock["wall"])  # fingerprints and archive
        progress.activity = 0
        attempt["outcome"] = "complete"
        self.assertEqual(beat(cpu=4.0)["last_activity_epoch"], clock["wall"])  # the driver's exposure collection
        progress.activity = 0
        self.assertEqual(beat(cpu=0.5)["last_activity_epoch"], 0)              # under a quarter of a core: idle
        progress.touch()
        self.assertGreater(beat()["last_activity_epoch"], 0)
        self.assertEqual(beat()["completed_updates"], 1)
        for marker in ("finished", "outcome"):  # a terminal attempt's later updates are never counted
            terminal = {"root": str(dispatch), "native": str(native), "dispatch": attempt["dispatch"], marker: "x"}
            progress.active["G1"] = {"attempts": [terminal]}
            folder = native / "iterations" / f"00000{2 if marker == 'finished' else 3}" / "attempt-000000" / "update"
            folder.mkdir(parents=True)
            (folder / "update.json").write_text("{}")
            progress.activity = 0
            record = beat()
            self.assertEqual((record["last_activity_epoch"], record["completed_updates"]), (0, 1))

    def lease_clock(self, work_idle=None):
        sys.path.insert(0, str(Path(driver.__file__).resolve().parent / "phase1_cloud"))
        import lease_guard
        guard = self.root / "guard"
        lease_guard_files(guard)
        lease = json.loads((guard / "lease.json").read_text())
        if work_idle is not None:
            lease["work_idle_seconds"] = work_idle
            (guard / "lease.json").write_text(json.dumps(lease))
        with patch.dict(os.environ, {"RUNPOD_POD_ID": "testpod", "MTG_LEASE_GUARD_DIR": str(guard)}):
            progress = driver.LeaseProgress(self.campaign)
        return progress, lease, guard, lease_guard

    def campaign_with(self, lease):
        campaign = driver.Campaign(self.path)
        campaign.lease = lease
        return campaign

    def test_a_watched_trial_longer_than_the_work_idle_bound_keeps_the_lease_active(self):
        """2026-11-01: a qualification trial (a detached reservation supervisor, no child of the driver) ran past
        work_idle_seconds and the guard latched native_work_stalled_or_idle. Watched, it is observed like an
        in-flight attempt: CPU under its supervisor outside its native part, the native child's CPU inside."""
        progress, lease, guard, lease_guard = self.lease_clock()
        me, clock, ticks = os.getpid(), {"wall": lease["created_epoch"] + 60, "cpu": 10.}, {"trial": 0, "native": 0}
        root = self.root / "trial-w1"; root.mkdir()

        def table():  # the trial's supervisor (950, detached: not under this driver) and its dispatcher (951)
            return {me: (1, 0), 950: (1, 0), 951: (950, ticks["trial"])}

        def beat(trial=0, native=0, owner="t950", start=100.0):
            clock["wall"] += 15.; ticks["trial"] += trial; ticks["native"] += native
            in_native = (root / "started.json").exists() and not (root / "execution.json").exists()
            with patch.object(driver.time, "time", return_value=clock["wall"]), \
                    patch.object(driver.base.time, "time", return_value=clock["wall"]), \
                    patch.object(driver.base, "driver_cpu_seconds", return_value=clock["cpu"]), \
                    patch.object(driver.base, "process_table", side_effect=table), \
                    patch.object(driver.base, "process_start_unix", return_value=start), \
                    patch.object(driver.base, "native_ticks",
                                 side_effect=lambda r: ticks["native"] if in_native and Path(r) == root else None), \
                    patch.object(driver.base, "supervisor_owned", side_effect=lambda pid, token: token == owner):
                progress.beat()
            record = json.loads((guard / "progress.json").read_text())
            return lease_guard.decision(lease, clock["wall"], 1.0, record)["reason"]

        progress.cpu, progress.activity = (clock["wall"], clock["cpu"]), clock["wall"]
        trial = {"state": "dispatched", "pid": 950, "token": "t950"}
        steps = lease["work_idle_seconds"] // 15
        proc = patch.object(driver, "proc_readable", return_value=True)  # the Pod's /proc, on any test host
        proc.start()
        self.addCleanup(proc.stop)
        with patch.object(driver.base, "process_start_unix", return_value=100.0):
            watched = driver.watching(self.campaign_with(progress), "trial:w1", trial, root=root)
            watched.__enter__()
        self.assertEqual({beat(trial=40) for _ in range(2 * steps)}, {None})  # preparation: supervised CPU
        (root / "started.json").write_text("{}")                              # its native part
        self.assertEqual({beat(native=40) for _ in range(2 * steps)}, {None})  # the native child computes
        self.assertEqual([beat(trial=40) for _ in range(steps)][-1],           # a stalled native child with a
                         "native_work_stalled_or_idle")                         # busy dispatcher still goes idle
        (root / "execution.json").write_text("{}")
        progress.activity = clock["wall"]
        self.assertEqual({beat(trial=40) for _ in range(2 * steps)}, {None})  # after it: supervised CPU again
        progress.activity = clock["wall"]
        self.assertEqual([beat(trial=40, start=200.0) for _ in range(steps)][-1],  # a reused PID, same token
                         "native_work_stalled_or_idle")
        progress.activity = clock["wall"]
        self.assertEqual([beat(trial=40, owner="t-other") for _ in range(steps)][-1],  # another token
                         "native_work_stalled_or_idle")
        watched.__exit__(None, None, None)
        self.assertEqual(progress.watched, {})
        self.assertFalse([name for name in progress.ticks if name.startswith("watched:")])
        progress.activity = clock["wall"]
        self.assertEqual([beat(trial=40) for _ in range(steps)][-1], "native_work_stalled_or_idle")  # unwatched
        with driver.watching(self.campaign_with(None), "trial:w3", trial):  # no lease: a no-op
            pass

    def test_a_watched_pack_counts_its_trials_native_parts(self):
        progress, lease, guard, lease_guard = self.lease_clock()
        pack = self.root / "pack"; pack.mkdir()
        roots = [self.root / "packed-0", self.root / "packed-1"]
        for index, root in enumerate(roots):
            (pack / f"packed-{index}.request.json").write_text(json.dumps({"root": str(root)}))
        (pack / "broken.request.json").write_text("{torn")
        clock, ticks, me = {"wall": lease["created_epoch"] + 60}, {str(roots[1]): 0, "pack": 0}, os.getpid()

        def table():  # the pack process (777) is the driver's child; it polls its trials
            return {me: (1, 0), 777: (me, ticks["pack"])}

        def beat(native=0):
            clock["wall"] += 15.; ticks[str(roots[1])] += native; ticks["pack"] += 3
            with patch.object(driver.time, "time", return_value=clock["wall"]), \
                    patch.object(driver.base.time, "time", return_value=clock["wall"]), \
                    patch.object(driver.base, "driver_cpu_seconds", return_value=progress.cpu[1]), \
                    patch.object(driver.base, "process_table", side_effect=table), \
                    patch.object(driver.base, "native_ticks", side_effect=lambda r: ticks.get(str(r))):
                progress.beat()
            record = json.loads((guard / "progress.json").read_text())
            return record, lease_guard.decision(lease, clock["wall"], 1.0, record)["reason"]

        progress.activity = clock["wall"]
        with driver.watching(self.campaign_with(progress), "pack", pack=pack) as watched:
            watched.exclude_child(777)
            results = [beat(native=40) for _ in range(2 * lease["work_idle_seconds"] // 15)]
            self.assertEqual({reason for _, reason in results}, {None})
            self.assertTrue(results[-1][0]["native_alive"])
            self.assertEqual([beat() for _ in range(lease["work_idle_seconds"] // 15)][-1][1],
                             "native_work_stalled_or_idle")  # stalled packed trials go idle, polling or not
        self.assertEqual(progress.excluded_children(), frozenset())

    def replay(self, seconds, setup, step):
        """Beat the lease clock every 15 s over a scripted timeline (``step(t, state)`` sets the processes, native
        ticks and records at t seconds) and return the guard's decision after each beat (lease_guard.decision,
        as the resident guard runs it on progress.json)."""
        progress, lease, guard, lease_guard = self.lease_clock()
        proc = patch.object(driver, "proc_readable", return_value=True)
        proc.start()
        self.addCleanup(proc.stop)
        me, t0 = os.getpid(), lease["created_epoch"] + 60
        state = {"table": {me: (1, 0)}, "native": {}, "commands": {}, "owners": {}}
        patches = [patch.object(driver.base, "driver_cpu_seconds", return_value=10.0),
                   patch.object(driver.base, "process_table", side_effect=lambda: dict(state["table"])),
                   patch.object(driver, "process_commands", side_effect=lambda table: dict(state["commands"])),
                   patch.object(driver.base, "native_ticks", side_effect=lambda root: state["native"].get(str(root))),
                   patch.object(driver.base, "process_start_unix", return_value=100.0),
                   patch.object(driver.base, "supervisor_owned",
                                side_effect=lambda pid, token: pid in state["table"] and state["owners"].get(pid) == token)]
        for item in patches:
            item.start()
            self.addCleanup(item.stop)
        progress.cpu, progress.activity = (t0, 10.0), t0
        context = setup(progress, state)
        reasons, t = [], 0
        while t < seconds:
            t += 15
            step(t, state)
            with patch.object(driver.time, "time", return_value=t0 + t), \
                    patch.object(driver.base.time, "time", return_value=t0 + t):
                progress.beat()
            record = json.loads((guard / "progress.json").read_text())
            reasons.append((t, lease_guard.decision(lease, t0 + t, 1.0, record)["reason"]))
        context.__exit__(None, None, None)
        return reasons

    @staticmethod
    def write_mark(root, name):
        Path(root).mkdir(parents=True, exist_ok=True)
        (Path(root) / name).write_text("{}")

    def test_replay_lease_2302_history_trial_w1(self):
        """Lease 20261101-2302: item 6's history trial w1 took 317 s, 12 s of them native (execution.json); it
        latched at the 300 s work-idle bound. Watched, its preparation and its native part are activity."""
        root = self.root / "history" / "trials" / "w1"

        def setup(progress, state):
            state["table"].update({950: (1, 0), 951: (950, 0)})  # its supervisor (detached) and dispatcher
            state["owners"][950] = "t950"
            context = driver.watching(self.campaign_with(progress), "trial:w1", {"pid": 950, "token": "t950"}, root=root)
            context.__enter__()
            return context

        def step(t, state):
            if t <= 305:  # preparation: the dispatcher computes (about a core)
                state["table"][951] = (950, state["table"][951][1] + 1400)
            elif t <= 317:  # the native part
                self.write_mark(root, "started.json")
                state["native"][str(root)] = state["native"].get(str(root), 0) + 1100
            else:
                state["native"].pop(str(root), None)
                for name in ("execution.json", "report.json"):
                    self.write_mark(root, name)
        reasons = self.replay(330, setup, step)
        self.assertEqual({reason for _, reason in reasons}, {None})

    def pack_requests(self, names, barrier=False):
        pack = self.root / "history" / "pack"
        pack.mkdir(parents=True, exist_ok=True)
        roots = {}
        for name in names:
            roots[name] = self.root / "history" / "trials" / f"w4-plan-{name}"
            value = {"root": str(roots[name])}
            if barrier and name.startswith("packed-"):
                value["start_barrier"] = {"ready": str(pack / f"{name}.ready.json"), "release": str(pack / "release.json")}
            (pack / f"{name}.request.json").write_text(json.dumps(value))
        return pack, roots

    def pack_setup(self, pack):
        def setup(progress, state):
            state["table"][777] = (os.getpid(), 0)  # the pack process, the driver's child
            context = driver.watching(self.campaign_with(progress), "pack", pack=pack)
            context.__enter__()
            context.exclude_child(777)
            return context
        return setup

    @staticmethod
    def trial_processes(state, request, supervisor, compute=0, parent=1):
        """A packed or solo trial's supervisor (its real command-line shape) and dispatcher, the dispatcher
        computing ``compute`` ticks this beat."""
        dispatcher = supervisor + 1
        state["table"].setdefault(supervisor, (parent, 0))
        state["table"].setdefault(dispatcher, (supervisor, 0))
        state["commands"][supervisor] = ["python3", "host_reservation_v1.py", "supervise", "--token", f"tok{supervisor}",
                                         "--nested", "--", "python3", "-B", "native_expanded_dispatch_v1.py",
                                         "_qualify", request]
        state["commands"][dispatcher] = ["python3", "-B", "native_expanded_dispatch_v1.py", "_qualify", request]
        state["table"][dispatcher] = (supervisor, state["table"][dispatcher][1] + compute)

    @staticmethod
    def end_processes(state, *pids):
        for pid in pids:
            state["table"].pop(pid, None)
            state["commands"].pop(pid, None)

    def lease_2146_timeline(self):
        """The measured shape of lease 20261102-2146's history pack, extended through its packed trials: the
        declaration (76 s, the pack computing), the solo trial (prepared to 158 s, native for 4.4 s between two
        beats, its report by 166 s), a quiet wait (the pack polling, 285 s), the pack's storage-admission walk
        (400 s, mostly I/O wait, 3 % of a core), the eight packed trials' preparation (330 s), their start
        barrier (dispatchers polling every 20 ms), the release, native parts and reports."""
        names = ["solo-0"] + [f"packed-{i}" for i in range(8)]
        pack, roots = self.pack_requests(names, barrier=True)
        request = {name: str(pack / f"{name}.request.json") for name in names}

        def step(t, state):
            pid, ticks = state["table"][777]
            walking = t <= 76 or 450 < t <= 850
            state["table"][777] = (pid, ticks + (45 if walking else 1))  # 3 % of a core, or polling
            if 76 < t <= 158:
                self.trial_processes(state, request["solo-0"], 960, compute=1400)
            elif 158 < t <= 166:  # native for 4.4 s between two beats, then its records
                for record in ("started.json", "execution.json", "report.json"):
                    self.write_mark(roots["solo-0"], record)
                self.end_processes(state, 960, 961)
            if t > 850:
                for i in range(8):
                    name = f"packed-{i}"
                    if t <= 1180:  # preparation
                        self.trial_processes(state, request[name], 1000 + 2 * i, compute=1400)
                    elif t <= 1200:  # at the start barrier, polling
                        self.write_mark(Path(pack / f"{name}.ready.json").parent, f"{name}.ready.json")
                        self.trial_processes(state, request[name], 1000 + 2 * i, compute=3)
                    elif t <= 1260:  # released: the native part
                        self.write_mark(pack, "release.json")
                        self.write_mark(roots[name], "started.json")
                        state["native"][str(roots[name])] = state["native"].get(str(roots[name]), 0) + 1400
                    else:
                        state["native"].pop(str(roots[name]), None)
                        for record in ("execution.json", "report.json"):
                            self.write_mark(roots[name], record)
                        self.end_processes(state, 1000 + 2 * i, 1001 + 2 * i)
        return pack, step

    def test_replay_lease_2146_history_pack(self):
        """No latch over the whole pack (lease 20261102-2146 latched at about 300 s after the w4 report)."""
        pack, step = self.lease_2146_timeline()
        reasons = self.replay(1290, self.pack_setup(pack), step)
        self.assertEqual({reason for _, reason in reasons}, {None})

    def test_replay_lease_2146_needs_each_signal(self):
        """The timeline holds only with every signal: without the new records (the solo trial's 4.4 s native
        part between beats), without the pack's own work (its admission walk), or without the trials' processes
        (their preparation), the guard latches."""
        for name, ablation in (
                ("records", patch.object(driver.LeaseProgress, "marked", lambda self, name, root, stamp: None)),
                ("pack work", patch.object(driver, "PACK_BUSY_SHARE", 10.0)),
                ("trial processes", patch.object(driver, "supervises", return_value=False))):
            with self.subTest(signal=name):
                shutil.rmtree(self.root / "history", ignore_errors=True)
                shutil.rmtree(self.root / "guard", ignore_errors=True)
                pack, step = self.lease_2146_timeline()
                with ablation:
                    reasons = self.replay(1290, self.pack_setup(pack), step)
                self.assertIn("native_work_stalled_or_idle", {reason for _, reason in reasons})

    def test_replay_stalled_packs_still_latch(self):
        """A truly idle pack goes idle at the bound: trials waiting at a barrier that is never released (their
        dispatchers polling), an orphan dispatcher (its supervisor gone) still computing, a finished trial's
        lingering processes, and the pack process polling."""
        pack, roots = self.pack_requests(["solo-0", "packed-0", "packed-1"], barrier=True)

        def step(t, state):
            pid, ticks = state["table"][777]
            state["table"][777] = (pid, ticks + 1)
            self.write_mark(pack, "packed-0.ready.json")
            self.trial_processes(state, str(pack / "packed-0.request.json"), 1000, compute=3)  # at the barrier
            state["table"][1003] = (1, state["table"].get(1003, (1, 0))[1] + 1400)  # an orphan dispatcher
            state["commands"][1003] = ["python3", "-B", "native_expanded_dispatch_v1.py", "_qualify",
                                       str(pack / "packed-1.request.json")]
            self.write_mark(roots["solo-0"], "report.json")  # finished, its supervisor lingering and computing
            self.trial_processes(state, str(pack / "solo-0.request.json"), 960, compute=1400)
        reasons = self.replay(330, self.pack_setup(pack), step)
        latched = [t for t, reason in reasons if reason == "native_work_stalled_or_idle"]
        self.assertTrue(latched)
        self.assertGreaterEqual(latched[0], 300)  # not before the bound
        self.assertLessEqual(latched[0], 315)

    def test_supervisor_command_lines(self):
        request = "/w/pack/packed-0.request.json"
        line = ["python3", "host_reservation_v1.py", "supervise", "--token", "t1", "--", "python3", "_qualify", request]
        self.assertTrue(driver.supervises(line, request))
        self.assertTrue(driver.supervises(line[:5] + ["--nested"] + line[5:], request))
        self.assertFalse(driver.supervises(["python3", "_qualify", request], request))  # the dispatcher
        self.assertFalse(driver.supervises(line, request + "x"))
        self.assertFalse(driver.supervises(["python3", "supervise", "--token", "", "--", request], request))
        self.assertFalse(driver.supervises(["python3", "supervise", "--", request, "--token", "t"], request))

    @unittest.skipUnless(sys.platform.startswith("linux"), "reads /proc")
    def test_a_real_detached_trial_past_the_bound_on_linux(self):
        """Real processes: a supervisor naming its token, reparented away from this driver (as a WMI or setsid
        launch is), whose child computes for longer than a 3 s work-idle bound. Unwatched the guard latches;
        watched it does not."""
        def trial(seconds, ready):
            # The launcher exits at once, so the supervisor is reparented: no descendant of this test process.
            burn = f"import time\nend = time.time() + {seconds}\nwhile time.time() < end: pass\n"
            supervisor = (f"import os, subprocess, sys\n"
                          f"child = subprocess.Popen([sys.executable, '-c', {burn!r}])\n"
                          f"open({str(ready)!r} + '.partial', 'w').write(str(os.getpid()))\n"
                          f"os.replace({str(ready)!r} + '.partial', {str(ready)!r})\n"
                          f"child.wait()\n")
            launcher = ("import subprocess, sys\n"
                        f"subprocess.Popen([sys.executable, '-c', {supervisor!r}, 'tok-trial-1101'], "
                        "start_new_session=True)\n")
            subprocess.run([sys.executable, "-c", launcher], check=True)
            for _ in range(100):
                if ready.exists():
                    return int(ready.read_text())
                time.sleep(0.1)
            self.fail("the supervisor did not start")

        def run(watched):
            progress, lease, guard, lease_guard = self.lease_clock(work_idle=3)
            ready = self.root / f"ready-{watched}"
            pid = trial(8, ready)
            try:
                children = [p for p, (parent, _) in driver.base.process_table().items() if parent == os.getpid()]
                self.assertNotIn(pid, children)  # detached, as on the Pod
                progress.start()
                reasons = []
                context = driver.watching(self.campaign_with(progress if watched else None), "trial:w1",
                                          {"pid": pid, "token": "tok-trial-1101"}, root=self.root / "no-native")
                with context:
                    end = time.time() + 6
                    while time.time() < end:
                        time.sleep(0.5)
                        record = json.loads((guard / "progress.json").read_text())
                        reasons.append(lease_guard.decision(lease, time.time(), 1.0, record)["reason"])
                return reasons
            finally:
                progress.done.set()
                if progress.thread.is_alive():
                    progress.thread.join()
                try:
                    os.killpg(os.getpgid(pid), signal.SIGKILL)
                except (ProcessLookupError, PermissionError):
                    pass
                shutil.rmtree(self.root / "guard", ignore_errors=True)

        self.assertIn("native_work_stalled_or_idle", run(False))  # the 2026-11-01 failure, reproduced
        watched = run(True)
        self.assertTrue(watched)
        self.assertNotIn("native_work_stalled_or_idle", watched)

    def test_pack_slots_and_native_root(self):
        raw = json.loads(self.path.read_text())
        raw["placement_by_run"] = {run: {"host": "desktop", "cpu_affinity": [2 * i, 2 * i + 1], "workers": 2,
                                         "preparation_workers": 2, "memory_bytes": 1}
                                   for i, run in enumerate(g.RUNS)}
        raw["roots"]["native"] = str(self.root / "container")
        tool = self.root / "host_slots_v1.py"; tool.write_text("tool")
        raw["host_slots"] = {"cores": "0-15", "tool": driver.pin(tool)}
        self.write(raw)
        self.assertEqual(driver.declared_cores(self.campaign)[-4:], ["timed", "--cores", "0-15", "--"])
        spec = driver.block_spec(self.campaign, "G4", 1, {"blocks": {}}, self.root / "n")
        self.assertEqual((spec["collection_workers"], spec["preparation_workers"]), (2, 2))
        request = driver.request_for(self.campaign, "G4", {"path": "x", "sha256": "y"}, self.root / "a", self.root / "b")
        self.assertEqual(request["placement"]["cpu_affinity"], [6, 7])
        self.assertEqual(self.campaign.native("G4"), self.root / "container")
        raw["host_slots"]["cores"] = "0-7"
        self.write(raw)
        with self.assertRaises(SystemExit):
            driver.declared_cores(self.campaign)

    def test_storage_cap_gates_and_wall_clock(self):
        raw = json.loads(self.path.read_text())
        raw["storage"]["max_logical_bytes"] = 121 * 10**9
        with self.assertRaises(SystemExit):
            self.write(raw)
        raw["storage"]["max_logical_bytes"] = 100 * 10**9
        raw["stop_after_block"] = 741
        self.write(raw)
        self.assertIsNone(self.campaign.admits(2))
        self.assertIn("placement", self.campaign.admits(3))
        runtime = {"path": "r", "sha256": "r" * 64}
        imports = {run: item["sha256"] for run, item in self.play.items()}
        block2 = {run: f"{index:064d}" for index, run in enumerate(g.RUNS)}
        record = lambda name, **fields: save(self.root / f"{name}.json", json.dumps(fields).encode())
        qualification = record("q", schema=driver.QUALIFICATION_SCHEMA, host="desktop", runtime=runtime,
                               play_imports=imports, block2_checkpoints=block2, **{"pass": True})
        failed = record("qf", schema=driver.QUALIFICATION_SCHEMA, host="desktop", runtime=runtime,
                        play_imports=imports, block2_checkpoints=block2, **{"pass": False})
        other = record("qo", schema=driver.QUALIFICATION_SCHEMA, host="desktop", runtime=dict(runtime, sha256="o" * 64),
                       play_imports=imports, block2_checkpoints=block2, **{"pass": True})
        inputs = record("qi", schema=driver.QUALIFICATION_SCHEMA, host="desktop", runtime=runtime,
                        play_imports=dict(imports, G1="9" * 64), block2_checkpoints=block2, **{"pass": True})
        four = lambda name, **changes: record(name, **({
            "schema": driver.ITEM4_SCHEMA, "run": "G1", "passed": True,
            "runtime_rules": {"value": "resolution-boundary-v1"},
            "checkpoint": {"recorded": {"sha256": block2["G1"]}},
            "play_import": {"recorded": {"sha256": imports["G1"]}}} | changes))
        five = lambda name, **changes: record(name, **({
            "schema": driver.ITEM5_SCHEMA, "run": "G1", "passed": True, "checkpoint_sha256": block2["G1"],
            "play_import_sha256": imports["G1"], "games_completed": 1} | changes))
        good4, good5 = four("i4"), five("i5")
        cases = ((qualification, "desktop", good4, good5, True), (failed, "desktop", good4, good5, False),
                 (qualification, "runpod", good4, good5, False), (other, "desktop", good4, good5, False),
                 (inputs, "desktop", good4, good5, False), (qualification, "desktop", good4, None, False),
                 (qualification, "desktop", qualification, qualification, False),  # untyped stand-ins
                 (qualification, "desktop", four("b4", runtime_rules={"value": None}), good5, False),
                 (qualification, "desktop", four("c4", checkpoint={"recorded": {"sha256": "x" * 64}}), good5, False),
                 (qualification, "desktop", good4, five("b5", passed=False), False),
                 (qualification, "desktop", good4, five("c5", play_import_sha256="y" * 64), False),
                 (qualification, "desktop", record("e4", schema=driver.ITEM4_SCHEMA, passed=True,
                                                   runtime_rules={"value": "resolution-boundary-v1"}),
                  record("e5", schema=driver.ITEM5_SCHEMA, passed=True, games_completed=1), False),  # no identities
                 (qualification, "desktop", four("g2", run="G2", checkpoint={"recorded": {"sha256": block2["G2"]}},
                                                 play_import={"recorded": {"sha256": imports["G2"]}}), good5, True),
                 (qualification, "desktop", good4, five("f5", run="G2", checkpoint_sha256=block2["G2"],
                                                        play_import_sha256=imports["G2"]), False))  # item 5 is G1's
        for index, (cited, host, item4, item5, admitted) in enumerate(cases):
            raw["placement_record"] = record(f"p{index}", schema=driver.PLACEMENT_SCHEMA, host=host,
                                             qualification=cited, item4=item4, item5=item5)
            self.write(raw)
            self.assertEqual(self.campaign.admits(3) is None, admitted, index)
        raw["placement_record"] = record("p", schema=driver.PLACEMENT_SCHEMA, host="desktop",
                                         qualification=qualification, item4=good4, item5=good5)
        self.write(raw)
        self.assertIsNone(self.campaign.admits(494))
        self.assertIn("continuation", self.campaign.admits(495))
        for decision, admitted in (("stop", False), ("continue", True)):
            raw["continuation"] = save(self.root / f"c-{decision}.json", json.dumps(
                {"schema": driver.CONTINUATION_SCHEMA, "decision": decision}).encode())
            self.write(raw)
            self.assertEqual(self.campaign.admits(495) is None, admitted)
        # Claim time across sessions: 59 hours used leaves room for the 10 s estimate, not for 1.25 x 3,000 s.
        wall = self.campaign.state / "wall"
        wall.mkdir(parents=True)
        (wall / "1-1.json").write_text(json.dumps({"start": 0, "last": 59 * 3600}))
        self.assertIsNone(self.campaign.wall_stop())
        self.campaign.save_state("G1", {"run": "G1", "status": "running", "blocks": {
            "1": {"attempts": [{"host": "desktop", "seconds": 3000}]}}})
        self.assertEqual(self.campaign.wall_stop(), "study_wall_cap")
        request = driver.request_for(self.campaign, "G1", {"path": "x", "sha256": "y"}, self.root / "a", self.root / "b")
        self.assertLessEqual(request["wall_seconds"], 3600 - 600)  # the launcher's timeout ends inside the claim
        clock = driver.WallClock(self.campaign, interval=3600)
        clock.start()
        clock.stop()
        self.assertEqual(len(list(wall.glob("*.json"))), 2)

    def test_only_the_synthetic_scratch_block_skips_the_placement_gate(self):
        raw = json.loads(self.path.read_text())
        raw.update(stop_after_block=4, qualification_synthetic_block=4)
        self.write(raw)
        self.assertIsNone(self.campaign.admits(4))
        self.assertIn("placement", self.campaign.admits(3))
        raw.update(stop_after_block=5)
        self.write(raw)
        self.assertIn("placement", self.campaign.admits(4))
        raw.update(stop_after_block=741, qualification_synthetic_block=741)
        self.write(raw)
        self.assertIn("placement", self.campaign.admits(741))

    def test_retirement_finishes_a_stopped_pass(self):
        state = self.kept("G1", range(1, 8))
        entry = state["blocks"]["3"]
        raw = Path(entry["final_checkpoint"]["path"])
        stale = raw.with_name(raw.name + ".gz.partial")
        stale.write_bytes(b"half")  # a pass stopped while compressing
        driver.retire(self.campaign, "G1", 7, state)
        self.assertFalse(stale.exists())
        self.assertEqual(entry["checkpoint_state"], "compressed")
        raw.write_bytes(b"left behind")  # a pass stopped after saving the state, before removing the raw file
        driver.retire(self.campaign, "G1", 7, state)
        self.assertFalse(raw.exists())
        good = save(self.root / "receipt.json", json.dumps({"schema": driver.RECEIPT_SCHEMA, "byte_identical": True, "interrupted": True,
                                                             "recipe": "g-references-v1", "host": "desktop",
                                                            "runtime": "r" * 64}).encode())
        raw_campaign = json.loads(self.path.read_text())
        raw_campaign["ancestry_receipt"] = good
        self.write(raw_campaign)
        state["blocks"]["5"]["checkpoint_state"] = "deleting"  # a pass stopped mid-deletion
        driver.retire(self.campaign, "G1", 7, state)
        self.assertEqual({state["blocks"][k]["checkpoint_state"] for k in ("3", "5")}, {"deleted"})
        self.assertEqual(self.campaign.run_state("G1")["blocks"]["5"]["checkpoint_state"], "deleted")

    def test_failed_attempt_keeps_its_records(self):
        native = self.root / "native-a1"
        (native / "iterations" / "000000" / "attempt-000000" / "update").mkdir(parents=True)
        (native / "iterations" / "000000" / "attempt-000000" / "update" / "update.json").write_text("{}")
        (native / "iterations" / "000000" / "attempt-000000" / "episode-0000.json").write_text("trajectory")
        (native / "run.json").write_text("{}")
        attempt = {"attempt": 1, "native": str(native)}
        driver.discard(self.campaign, "G2", 7, attempt, "test")
        self.assertFalse(native.exists())
        kept = json.loads(Path(attempt["failed_records"]["path"]).read_text())["files"]
        self.assertEqual(sorted(Path(item["to"]).name for item in kept), ["run.json", "update.json"])
        log = json.loads(self.campaign.prune_log.read_text().splitlines()[-1])
        self.assertEqual(log["retained_records"], attempt["failed_records"])

    def test_a_completed_block_whose_native_tree_went_with_its_pod_reruns(self):
        native = self.root / "container" / "G1" / "b001-a1" / "native"
        state = {"run": "G1", "status": "running", "blocks": {"1": {"attempts": [
            {"attempt": 1, "native": str(native), "outcome": "complete"}]}}}
        finished = []
        with patch.object(driver, "finish_block", lambda *a: finished.append(a) or "complete"), \
                patch.object(driver, "admit", return_value=None), patch.object(driver, "release"), \
                patch.object(driver, "block_spec", return_value={}), \
                patch.object(driver.g, "block_config", return_value={}), \
                patch.object(driver.g, "require_runtime_rules"), \
                patch.object(driver, "request_for", return_value={}), \
                patch.object(driver.Campaign, "wall_stop", return_value=None), \
                patch.object(driver.base, "dispatch_block", return_value={"state": "refused"}):
            native.mkdir(parents=True)  # still on this pod's disk: finished as before
            self.assertEqual(driver.run_block(self.campaign, "G1", 1, state), "complete")
            native.rmdir()
            entry = state["blocks"]["1"]
            entry.update(exposure={}, final_checkpoint={}, retained={})  # retained: finishing reads no native tree
            self.assertEqual(driver.run_block(self.campaign, "G1", 1, state), "complete")
            del entry["retained"]
            self.assertEqual(driver.run_block(self.campaign, "G1", 1, state), "stopped")
        self.assertEqual(len(finished), 2)
        self.assertEqual([a.get("outcome") for a in entry["attempts"]], ["lost", "spawn-refused"])
        self.assertNotIn("exposure", entry)
        self.assertNotIn("final_checkpoint", entry)
        events = [json.loads(line) for line in self.campaign.events.read_text().splitlines()]
        self.assertIn("native-lost", [e["kind"] for e in events])

    def scratch_campaign(self, cap=10**9, projected=1000):
        """A campaign whose native root is admitted block scratch (AMENDMENT-2.md)."""
        raw = json.loads(self.path.read_text())
        container = self.root / "container"
        raw["roots"]["native"] = str(container)
        raw["storage"] = {"max_logical_bytes": cap, "reserve_bytes": driver.native_dispatch.DISK_RESERVE_BYTES,
                          "projected_additional_bytes": projected, "accounting_roots": [str(self.root / "volume")],
                          "projected_volume_bytes": {self.root.drive.upper(): projected},
                          "scratch_roots": [str(container)]}
        raw["scratch"] = {"block_peak_bytes": 100, "factor": 1.25, "margin_fraction": 0.1}
        self.write(raw)
        return container

    def disk(self, free, total=1000):
        # the scratch volume (total, free) and a volume tree with its 60 GiB reserve free
        def usage(path):
            if Path(path).is_relative_to(self.root / "container"):
                return type("Disk", (), {"free": free, "total": total})()
            return type("Disk", (), {"free": 100 << 30, "total": 200 << 30})()
        return patch.object(driver.shutil, "disk_usage", usage)

    def test_scratch_admission_counts_every_in_flight_tree(self):
        self.addCleanup(driver.SCRATCH_INFLIGHT.clear)
        container = self.scratch_campaign()
        trees = [container / "G1" / f"b001-a{k}" / "native" for k in (1, 2, 3)]
        container.mkdir()
        with self.disk(free=400):  # 400 free less a 100 B margin: two blocks of 125 B (100 B x 1.25), not three
            self.assertIsNone(driver.admit(self.campaign, trees[0]))
            self.assertIsNone(driver.admit(self.campaign, trees[1]))
            self.assertRegex(driver.admit(self.campaign, trees[2]), "^scratch: 3 blocks need 375 B")
            trees[0].mkdir(parents=True)
            (trees[0] / "big").write_bytes(b"x" * 200)  # a tree past its peak counts at its size
            with self.disk(free=200):
                self.assertRegex(driver.admit(self.campaign, trees[2]), "^scratch: 3 blocks need 450 B")
            shutil.rmtree(trees[0])
            driver.release(self.campaign, "G1", 1, trees[0])  # its prune verified: the slot frees
            self.assertIsNone(driver.admit(self.campaign, trees[2]))
        self.assertEqual(driver.SCRATCH_INFLIGHT, {str(trees[1]), str(trees[2])})

    def test_retained_admission_projects_every_in_flight_block(self):
        self.addCleanup(driver.SCRATCH_INFLIGHT.clear)
        self.scratch_campaign(cap=10**9, projected=4 * 10**8)
        (self.root / "volume").mkdir()
        (self.root / "volume" / "kept").write_bytes(b"x" * 1000)
        with self.disk(free=10**6, total=10**6):
            self.assertIsNone(driver.admit(self.campaign, self.root / "container" / "a"))
            self.assertIsNone(driver.admit(self.campaign, self.root / "container" / "b"))
            self.assertRegex(driver.admit(self.campaign, self.root / "container" / "c"),
                             "^retained: 1000 B at the last walk plus 1200000000 B projected")

    def test_one_volume_walk_serves_admissions_and_counts_blocks_finished_since(self):
        self.addCleanup(driver.SCRATCH_INFLIGHT.clear)
        self.scratch_campaign(cap=8 * 10**8, projected=3 * 10**8)  # two blocks fit, three do not
        (self.root / "volume").mkdir()
        walks, touched = [], []
        real = driver.native_dispatch.accounting_bytes

        def counting(storage):
            walks.append(1)
            return real(storage)
        with self.disk(free=10**6, total=10**6), patch.object(driver.native_dispatch, "accounting_bytes", counting), \
                patch.object(driver, "touch", lambda campaign: touched.append(1)):
            a, b, c = (self.root / "container" / name for name in "abc")
            self.assertIsNone(driver.admit(self.campaign, a))
            self.assertIsNone(driver.admit(self.campaign, b))
            self.assertEqual((len(walks), len(touched)), (1, 1))  # one walk for both, a driver step
            driver.release(self.campaign, "G1", 1, a)  # finished since the walk: still projected
            self.assertEqual(driver.volume(self.campaign)["finished"], 1)
            self.assertRegex(driver.admit(self.campaign, c), "^retained: 0 B at the last walk plus 900000000 B")
            driver.volume(self.campaign)["walked"] -= driver.VOLUME_REWALK_SECONDS + 1
            self.assertIsNone(driver.admit(self.campaign, c))  # the refusal rested on a finished block: walked again
            self.assertEqual((len(walks), driver.volume(self.campaign)["finished"]), (2, 0))
        events = [json.loads(line) for line in self.campaign.events.read_text().splitlines()]
        self.assertEqual([e["kind"] for e in events].count("volume-walk"), 2)  # each walk measured

    def test_a_dispatch_request_carries_its_admission_walk(self):
        """#463: the walk a block was admitted on goes in its request (the dispatcher checks the same numbers
        without walking), and each walk is published for the dispatchers' emergency samples."""
        self.addCleanup(driver.SCRATCH_INFLIGHT.clear)
        self.scratch_campaign(projected=1000)
        (self.root / "volume").mkdir()
        (self.root / "volume" / "kept").write_bytes(b"x" * 300)
        state = {"run": "G4", "status": "running", "blocks": {}}
        requests = []

        def dispatched(path, host):
            requests.append(json.loads(Path(path).read_text(encoding="utf-8")))
            return {"state": "refused"}
        with self.disk(free=10**6, total=10**6),                 patch.object(driver, "block_spec", return_value={}),                 patch.object(driver.g, "block_config", return_value={}),                 patch.object(driver.g, "require_runtime_rules"),                 patch.object(driver, "request_for", return_value={"storage": self.campaign.raw["storage"]}),                 patch.object(driver.Campaign, "wall_stop", return_value=None),                 patch.object(driver.base, "dispatch_block", side_effect=dispatched):
            self.assertEqual(driver.run_block(self.campaign, "G4", 1, state), "stopped")
        admission = requests[0]["admission"]
        shared = self.campaign.state / driver.SHARED_WALK
        self.assertEqual((admission["logical_bytes"], admission["finished_since"], admission["shared_walk"],
                          admission["in_flight"]), (300, 0, str(shared), 0))  # no other block in flight
        self.assertEqual(driver.native_dispatch.admitted_bytes(admission, self.campaign.raw["storage"]), 300)
        self.assertEqual(driver.native_dispatch.shared_walk(shared)[0], 300)
        driver.volume(self.campaign)["finished"] = 2  # blocks finished since the walk count at their projection
        self.assertEqual(driver.native_dispatch.admitted_bytes(driver.admitted(self.campaign),
                                                               self.campaign.raw["storage"]), 300 + 2 * 1000)
        driver.VOLUMES.clear()
        self.assertIsNone(driver.admitted(self.campaign))  # nothing walked: the dispatcher walks
        with self.disk(free=10**6, total=10**6):
            self.assertEqual(driver.admitted(self.campaign, walk=True)["logical_bytes"], 300)  # the choice check

    def test_one_background_walker_publishes_while_blocks_are_in_flight(self):
        """#463: one walker per volume in the driver serves every dispatcher's emergency samples."""
        self.addCleanup(driver.SCRATCH_INFLIGHT.clear)
        self.addCleanup(driver.stop_walkers)
        self.scratch_campaign()
        raw = json.loads(self.path.read_text())
        raw["storage"]["emergency"] = {"volume_bytes": 10**12, "margin_fraction": 0.1}
        self.write(raw)
        (self.root / "volume").mkdir()
        (self.root / "volume" / "kept").write_bytes(b"x" * 300)
        shared = self.campaign.state / driver.SHARED_WALK
        touched = []
        with patch.object(driver, "WALKER_POLL_SECONDS", 0.05),                 patch.object(driver, "touch", lambda campaign: touched.append(1)):
            starters = [threading.Thread(target=driver.start_walker, args=(self.campaign,)) for _ in range(8)]
            for starter in starters:  # eight admissions at once start one walker
                starter.start()
            for starter in starters:
                starter.join()
            self.assertEqual(len(driver.WALKERS), 1)
            self.assertEqual(sum(thread.name == "volume-walker" for thread in threading.enumerate()), 1)
            time.sleep(0.3)
            self.assertIsNone(driver.native_dispatch.shared_walk(shared))  # nothing in flight: no walk
            driver.SCRATCH_INFLIGHT.add(str(self.root / "container" / "a"))
            deadline = time.monotonic() + 10
            while driver.native_dispatch.shared_walk(shared) is None and time.monotonic() < deadline:
                time.sleep(0.05)
            found = driver.native_dispatch.shared_walk(shared)
            self.assertEqual((found[0], found[3]), (300, 1))  # its bytes and the blocks in flight at its start
            time.sleep(0.3)
            self.assertEqual(driver.native_dispatch.shared_walk(shared), found)  # fresh: not walked again
        self.assertEqual(touched, [])  # its walks are not the lease's progress
        driver.stop_walkers()
        self.assertEqual(driver.WALKERS, {})

    def test_a_long_volume_walk_records_driver_work_while_it_runs(self):
        self.scratch_campaign()
        touched = []

        def slow(storage):
            time.sleep(0.35)
            return [], 7
        with patch.object(driver.native_dispatch, "accounting_bytes", slow), \
                patch.object(driver, "touch", lambda campaign: touched.append(time.monotonic())), \
                patch.object(driver, "WALK_HEARTBEAT_SECONDS", 0.05):
            self.assertEqual(driver.volume_logical(self.campaign), (7, 0))
        self.assertGreaterEqual(len(touched), 3)  # heartbeats during the walk, then its end
        touched.clear()
        driver.VOLUMES.clear()
        with patch.object(driver.native_dispatch, "accounting_bytes", slow), \
                patch.object(driver, "touch", lambda campaign: touched.append(time.monotonic())), \
                patch.object(driver, "WALK_HEARTBEAT_SECONDS", 0.05), patch.object(driver, "WALK_HEARTBEAT_LIMIT_SECONDS", 0):
            driver.volume_logical(self.campaign)
        self.assertEqual(len(touched), 1)  # past the limit a hung walk records nothing until it ends

    def test_an_unverified_prune_latches_every_dispatch(self):
        self.addCleanup(driver.SCRATCH_INFLIGHT.clear)
        self.addCleanup(driver.SCRATCH_LATCHED.clear)
        container = self.scratch_campaign()
        tree = container / "G2" / "b001-a1" / "native"
        with self.disk(free=10**6, total=10**6):
            self.assertIsNone(driver.admit(self.campaign, tree))
            tree.mkdir(parents=True)  # the prune left it behind
            driver.release(self.campaign, "G2", 1, tree)
            self.assertIn(str(tree), driver.SCRATCH_INFLIGHT)  # its slot stays taken
            self.assertTrue((self.campaign.state / driver.SCRATCH_LATCH).exists())
            self.assertRegex(driver.admit(self.campaign, container / "other"), "^latched: scratch tree not pruned")
            state = {"run": "G3", "status": "running", "blocks": {}}
            self.assertEqual(driver.run_block(self.campaign, "G3", 1, state), "stopped")
        self.assertEqual(state["blocks"]["1"]["attempts"], [])  # no attempt was made
        self.assertTrue(state["stop_reason"].startswith("storage admission: latched"))

    def test_a_storage_refusal_or_kill_does_not_consume_an_attempt(self):
        self.addCleanup(driver.SCRATCH_INFLIGHT.clear)
        self.scratch_campaign()
        state = {"run": "G4", "status": "running", "blocks": {}}
        refused = RuntimeError("dispatch refused: ValueError: retained plus projected storage exceeds allowance")
        with patch.object(driver, "admit", return_value=None), \
                patch.object(driver, "block_spec", return_value={}), \
                patch.object(driver.g, "block_config", return_value={}), \
                patch.object(driver.g, "require_runtime_rules"), \
                patch.object(driver, "request_for", return_value={}), \
                patch.object(driver.Campaign, "wall_stop", return_value=None), \
                patch.object(driver.base, "dispatch_block", side_effect=refused):
            # nothing else in flight could free space: the run stops at the block boundary, uncounted
            self.assertEqual(driver.run_block(self.campaign, "G4", 1, state), "stopped")
        entry = state["blocks"]["1"]
        self.assertEqual([a["outcome"] for a in entry["attempts"]], ["storage-refused"])
        self.assertEqual(driver.counted(entry), 0)
        self.assertTrue(state["stop_reason"].startswith("storage admission: "))

        def attempt(name, outcome, files, started=True):
            root = self.root / "campaign" / "hot" / name / "b001-a1" / "dispatch"
            root.mkdir(parents=True)
            for file, value in (files | ({"started.json": {}} if started else {})).items():
                (root / file).write_text(json.dumps(value))
            return {"attempt": 1, "root": str(root), "outcome": outcome}
        clean_exit = {"execution.json": {"exit_code": 0, "error": None}}
        killed_1613 = attempt("G3", "interrupted", clean_exit)  # one of lease 1613's five post-run kills
        clean_elsewhere = attempt("R1", "interrupted", clean_exit)  # any other clean exit without a record counts
        killed_in_run = attempt("R2", "interrupted", {"execution.json": {
            "exit_code": -9, "error": "retained plus projected storage exceeds allowance"}})
        recorded = attempt("R4", "interrupted", clean_exit | {
            "failure.json": {"error": "ValueError: [Errno 28] No space left on device"}})
        emergency = attempt("R5", "interrupted", clean_exit | {
            "failure.json": {"error": "ValueError: storage emergency: scratch volume free space below its margin"}})
        native_failed = attempt("R6", "interrupted", {"execution.json": {"exit_code": 1, "error": "native process failed"}})
        other_failure = attempt("R7", "interrupted", clean_exit | {
            "failure.json": {"error": "RuntimeError: retained copy differs"}})
        never_started = attempt("R8", "interrupted", {}, started=False)
        same_root_crash = attempt("G4", "interrupted", {"execution.json": {"exit_code": 1, "error": "native process failed"}})
        cases = (killed_1613, clean_elsewhere, killed_in_run, recorded, emergency, native_failed, other_failure,
                 never_started, same_root_crash)
        with patch.object(driver, "LEASE_1613_ROOT", (self.root / "campaign").as_posix() + "/"):
            # a 1613 root without the evidence's signature (exit 0, no error, no report, no failure.json) counts
            self.assertEqual([driver.counts(a) for a in cases],
                             [False, True, False, False, False, True, True, False, True])
        self.assertTrue(driver.counts(killed_1613))  # the exception names lease 1613's own volume paths only
        self.assertFalse(driver.counts({"outcome": "unspawned"}))
        self.assertTrue(driver.counts({"outcome": "lost"}))

    def test_an_attempt_that_never_started_is_unspawned_on_resume(self):
        self.addCleanup(driver.SCRATCH_INFLIGHT.clear)
        (self.root / "started").mkdir()
        (self.root / "started" / "started.json").write_text("{}")
        state = {"run": "G5", "status": "running", "blocks": {"1": {"attempts": [
            {"attempt": 1, "root": str(self.root / "never"), "native": str(self.root / "n1")},
            {"attempt": 2, "root": str(self.root / "started"), "native": str(self.root / "n2")}]}}}
        entry = state["blocks"]["1"]
        with patch.object(driver, "admit", return_value="scratch: full"):
            driver.run_block(self.campaign, "G5", 1, {**state, "blocks": {"1": {"attempts": [entry["attempts"][0]]}}})
            driver.run_block(self.campaign, "G5", 1, state)
        self.assertEqual([a["outcome"] for a in entry["attempts"]], ["unspawned", "interrupted"])

    def test_block_scratch_needs_its_admission_spec(self):
        self.scratch_campaign()
        raw = json.loads(self.path.read_text())
        del raw["scratch"]
        with self.assertRaisesRegex(SystemExit, "block scratch needs"):
            self.write(raw)
        raw["scratch"] = {"block_peak_bytes": 100, "factor": 1.25, "margin_fraction": 0.1}
        raw["storage"]["scratch_roots"] = [str(self.root / "elsewhere")]
        with self.assertRaisesRegex(SystemExit, "block scratch needs"):
            self.write(raw)

    def records_campaign(self, window=1, stall=60):
        raw = json.loads(self.path.read_text())
        raw["records"] = {"window_blocks": window, "stall_seconds": stall}
        self.write(raw)

    def test_a_finished_block_past_qualification_is_sealed_with_its_records(self):
        self.records_campaign()
        files = {"retained/G2/b03/run.json": b"{}", "retained/G2/b03/iterations/000001/episode-0.json": b"e",
                 "retained/G2/b003-a1-failed/RETAINED.json": b"f", "hot/G2/b003-a1/dispatch/report.json": b"r",
                 "hot/G2/b003-a2/dispatch/report.json": b"s", "exposure/G2-b003-episodes.jsonl": b"row",
                 "state/configs/G2-b003-a1.json": b"c", "state/requests/G2-b003-a2.json": b"q",
                 "exposure/G2-b003-exposure.json": b"summary", "retained/G2/b02/run.json": b"{}",
                 "hot/G2/b004-a1/dispatch/report.json": b"next"}
        for name, data in files.items():
            (self.root / name).parent.mkdir(parents=True, exist_ok=True)
            (self.root / name).write_bytes(data)
        state = {"run": "G2", "blocks": {"2": {"attempts": [], "done": True}, "3": {"attempts": [], "done": True}}}
        driver.seal(self.campaign, "G2", 2, state)  # qualification's block stays on the volume
        driver.seal(self.campaign, "G2", 3, state)
        self.assertNotIn("sealed", state["blocks"]["2"])
        value = json.loads(Path(state["blocks"]["3"]["sealed"]["path"]).read_text())
        sealed = {item["path"]: item["sha256"] for item in value["files"]}
        expected = {name: hashlib.sha256(data).hexdigest() for name, data in files.items()
                    if "b003" in name or "/b03/" in name}
        expected.pop("exposure/G2-b003-exposure.json")  # the summary stays for the driver
        self.assertEqual(sealed, expected)
        self.assertEqual((value["root"], value["block"], value["bytes"]), (str(self.root), 3, 11))
        self.assertEqual(Path(state["blocks"]["3"]["sealed"]["path"]),
                         (self.campaign.state / "seals" / "G2-b003.json").resolve())  # its own place
        for name, data in files.items():  # no source file was touched
            self.assertEqual((self.root / name).read_bytes(), data)
        self.assertEqual(self.campaign.run_state("G2")["blocks"]["3"]["sealed"], state["blocks"]["3"]["sealed"])

    def test_the_copy_back_window_and_its_stall_hold_new_blocks(self):
        self.records_campaign(window=1, stall=60)
        seals, moved = self.campaign.state / "seals", self.campaign.state / "moved"
        seals.mkdir(parents=True)
        for k in range(8):  # one window block per lineage
            (seals / f"G{k + 1}-b003.json").write_text("{}")
        self.assertRegex(driver.records_hold(self.campaign), r"^records: window full, 8 sealed and 0 in flight")
        moved.mkdir()
        (moved / "G1-b003.json").write_text('{"deleted": false}')  # copied, not yet gone: still pending
        self.assertRegex(driver.records_hold(self.campaign), r"^records: window full, 8 sealed")
        (moved / "G1-b003.json").write_text('{"deleted": true}')
        with patch.object(driver, "DRIVER_STARTED", time.time() - 1000):
            os.utime(moved / "G1-b003.json", (time.time() - 1000, time.time() - 1000))
            self.assertRegex(driver.records_hold(self.campaign), "^records: copy-back stalled, 7 sealed blocks wait")
            self.assertRegex(driver.admit(self.campaign, self.root / "n"), "^records: copy-back stalled")
            state = {"run": "G2", "status": "running", "blocks": {}}
            self.assertEqual(driver.run_block(self.campaign, "G2", 3, state), "stopped")  # nothing in flight
            self.assertEqual(state["blocks"]["3"]["attempts"], [])
            (moved / "G2-b003.json").write_text('{"deleted": true}')  # the copy-back moved a block just now
            self.assertIsNone(driver.records_hold(self.campaign))

    def sealed_tree(self, blocks, runs=("G2", "G3")):
        """Block records for ``blocks`` of ``runs`` (retained iterations, a failed attempt, dispatch folders,
        exposure rows, configs and requests), each sealed as the driver seals it."""
        for run in runs:
            state = {"run": run, "blocks": {}}
            for block in blocks:
                name = driver.label(block)
                files = {f"retained/{run}/b{block:02d}/run.json": b"{}" * block,
                         f"retained/{run}/{name}-a1-failed/RETAINED.json": b"f" * block,
                         f"hot/{run}/{name}-a2/dispatch/report.json": b"r" * (block + 3),
                         f"exposure/{run}-{name}-episodes.jsonl": b"row\n" * block,
                         f"state/configs/{run}-{name}-a2.json": b"c" * 50,
                         f"state/requests/{run}-{name}-a2.json": b"q" * 20}
                for update in range(6):
                    for leaf in ("collect/collection.json", "update/update.json"):
                        files[f"retained/{run}/b{block:02d}/iterations/{update:06d}/attempt-000000/{leaf}"] = \
                            b"x" * (update + block)
                for name_, data in files.items():
                    (self.root / name_).parent.mkdir(parents=True, exist_ok=True)
                    (self.root / name_).write_bytes(data)
                state["blocks"][str(block)] = {"attempts": [], "done": True}
                driver.seal(self.campaign, run, block, state)

    def walking_campaign(self):
        self.records_campaign()
        raw = json.loads(self.path.read_text())
        raw["storage"]["accounting_roots"] = [str(self.root)]
        raw["static_roots"] = [str(self.root / "qualification")]
        self.write(raw)
        for k in range(5):
            name = self.root / "qualification" / "scratch" / f"s{k}" / "state" / "G1.json"
            name.parent.mkdir(parents=True, exist_ok=True)
            name.write_bytes(b"q" * (k + 10))
        driver.WALK_STATE.clear()
        self.addCleanup(driver.WALK_STATE.clear)

    def full_walk(self):
        return driver.native_dispatch.accounting_bytes(self.campaign.raw["storage"])[1]

    def test_the_incremental_walk_equals_the_full_walk_and_skips_sealed_folders(self):
        self.walking_campaign()
        self.sealed_tree([3, 4, 5])
        moved = self.campaign.state / "moved"
        moved.mkdir(parents=True)
        seal = json.loads((self.campaign.state / "seals" / "G2-b003.json").read_text())
        for item in seal["files"]:  # the copy-back moved G2 block 3: its files are gone, its folders stay
            (self.root / item["path"]).unlink()
        (moved / "G2-b003.json").write_text('{"deleted": true}')
        self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())  # the first walk: full, checked
        self.assertFalse(driver.WALK_STATE[(str(self.root),)]["latched"])
        listed = []
        real = os.scandir

        def scandir(path):
            listed.append(Path(path))
            return real(path)
        expected = self.full_walk()
        with patch.object(driver.os, "scandir", side_effect=scandir):  # os is one module: no full walk inside
            self.assertEqual(driver.accounted_bytes(self.campaign), expected)
        sealed = [self.root / "retained" / "G3" / "b04", self.root / "hot" / "G2" / "b005-a2",
                  self.root / "retained" / "G2" / "b03", self.root / "qualification"]
        for folder in sealed:  # counted by their seal (or the first walk), not walked
            self.assertFalse([path for path in listed if path == folder or folder in path.parents], folder)
        # New, unsealed work is walked: the next block's records and the growing state.
        (self.root / "retained" / "G2" / "b06" / "run.json").parent.mkdir(parents=True)
        (self.root / "retained" / "G2" / "b06" / "run.json").write_bytes(b"new" * 100)
        (self.root / "state" / "events.jsonl").write_bytes(b"e" * 999)
        self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())
        self.sealed_tree([6], runs=("G2",))  # and once sealed, counted by its seal
        self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())

    def test_a_sealed_folder_that_changed_keeps_the_walks_full(self):
        self.walking_campaign()
        self.sealed_tree([3, 4])
        changed = self.root / "retained" / "G3" / "b04" / "iterations" / "000002" / "attempt-000000" / "extra.json"
        changed.write_bytes(b"z" * 77)  # written after its seal: the seal no longer describes the folder
        events = []
        with patch.object(driver.Campaign, "event", lambda self, **fields: events.append(fields)):
            self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())
            self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())  # latched: full for good
        self.assertTrue(driver.WALK_STATE[(str(self.root),)]["latched"])
        self.assertEqual([event["kind"] for event in events], ["volume-walk-seal-mismatch"])
        self.assertIn(driver.folder_key(self.root / "retained" / "G3" / "b04"), events[0]["folders"])

    def test_a_later_attempt_folder_the_seal_does_not_list_is_walked(self):
        self.walking_campaign()
        self.sealed_tree([3])
        self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())
        later = self.root / "hot" / "G2" / "b003-a3" / "dispatch" / "report.json"  # not in the seal
        later.parent.mkdir(parents=True)
        later.write_bytes(b"l" * 64)
        self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())

    def test_a_new_seal_is_measured_before_it_is_trusted_and_trust_expires(self):
        self.walking_campaign()
        self.sealed_tree([3])
        self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())
        self.sealed_tree([4], runs=("G2",))  # published, then written again before any walk measured it
        extra = self.root / "retained" / "G2" / "b04" / "late.json"
        extra.write_bytes(b"x" * 33)
        with patch.object(driver.Campaign, "event", lambda self, **fields: None):
            self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())
        self.assertTrue(driver.WALK_STATE[(str(self.root),)]["latched"])
        # Growth inside a trusted folder is found when its trust expires (REVERIFY_SECONDS) and latches.
        driver.WALK_STATE.clear()
        extra.unlink()
        self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())
        self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())  # now trusted
        grown = self.root / "retained" / "G3" / "b03" / "grown.json"
        grown.write_bytes(b"g" * 21)
        later = time.time() + driver.REVERIFY_SECONDS + 1
        with patch.object(driver.time, "time", return_value=later),                 patch.object(driver.Campaign, "event", lambda self, **fields: None):
            self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())
        self.assertTrue(driver.WALK_STATE[(str(self.root),)]["latched"])

    def test_a_moved_folder_is_measured_empty_before_it_counts_nothing(self):
        self.walking_campaign()
        self.sealed_tree([3])
        self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())
        seal = json.loads((self.campaign.state / "seals" / "G2-b003.json").read_text())
        for item in seal["files"]:
            (self.root / item["path"]).unlink()
        stray = self.root / "retained" / "G2" / "b03" / "stray.json"  # not the seal's: the copy-back leaves it
        stray.write_bytes(b"s" * 27)
        (self.campaign.state / "moved").mkdir(parents=True)
        (self.campaign.state / "moved" / "G2-b003.json").write_text('{"deleted": true}')
        with patch.object(driver.Campaign, "event", lambda self, **fields: None):
            self.assertEqual(driver.accounted_bytes(self.campaign), self.full_walk())
        self.assertTrue(driver.WALK_STATE[(str(self.root),)]["latched"])

    def test_walks_of_one_process_run_one_at_a_time(self):
        self.walking_campaign()
        self.sealed_tree([3])
        entered, release, order = threading.Event(), threading.Event(), []
        real = driver.walk_trees

        def slow(*args):
            order.append("start")
            entered.set()
            release.wait(5)
            result = real(*args)
            order.append("end")
            return result
        with patch.object(driver, "walk_trees", side_effect=slow):
            first = threading.Thread(target=driver.accounted_bytes, args=(self.campaign,))
            first.start()
            entered.wait(5)
            second = threading.Thread(target=driver.accounted_bytes, args=(self.campaign,))
            second.start()
            second.join(0.3)
            self.assertEqual(order, ["start"])  # the second walk waits for the first
            release.set()
            first.join(5); second.join(5)
        self.assertEqual(order, ["start", "end", "start", "end"])

    def test_a_latch_is_never_undone_by_a_later_agreeing_walk(self):
        self.walking_campaign()
        self.sealed_tree([3])
        driver.WALK_STATE[(str(self.root),)] = {"latched": True, "folders": {}, "moved": set(), "static": {},
                                                "static_at": 0.0}
        calls, expected = [], self.full_walk()
        real = driver.native_dispatch.accounting_bytes
        with patch.object(driver.native_dispatch, "accounting_bytes", lambda storage: calls.append(1) or real(storage)):
            self.assertEqual(driver.accounted_bytes(self.campaign), expected)
        self.assertEqual(calls, [1])  # latched: the full walk
        self.assertTrue(driver.WALK_STATE[(str(self.root),)]["latched"])

    def test_without_seals_or_static_roots_the_walk_is_the_full_walk(self):
        raw = json.loads(self.path.read_text())
        raw["storage"]["accounting_roots"] = [str(self.root)]
        self.write(raw)
        driver.WALK_STATE.clear()
        self.addCleanup(driver.WALK_STATE.clear)
        calls, expected = [], self.full_walk()
        real = driver.native_dispatch.accounting_bytes
        with patch.object(driver.native_dispatch, "accounting_bytes", lambda storage: calls.append(1) or real(storage)):
            self.assertEqual(driver.accounted_bytes(self.campaign), expected)
        self.assertEqual(calls, [1])

    def test_a_published_seal_is_adopted_not_rebuilt(self):
        self.records_campaign()
        published = self.campaign.state / "seals" / "G2-b003.json"
        published.parent.mkdir(parents=True)
        published.write_text(json.dumps({"schema": driver.RECORDS_SCHEMA, "run": "G2", "block": 3, "files": [
            {"path": "retained/G2/b03/run.json", "bytes": 2, "sha256": "0" * 64}]}))
        before = published.read_bytes()  # the copy-back may already have moved its files
        state = {"run": "G2", "blocks": {"3": {"attempts": [], "done": True}}}
        driver.seal(self.campaign, "G2", 3, state)
        self.assertEqual(published.read_bytes(), before)
        self.assertEqual(state["blocks"]["3"]["sealed"]["path"], str(published.resolve()))
        published.write_text(json.dumps({"schema": driver.RECORDS_SCHEMA, "run": "G5", "block": 3}))
        with self.assertRaisesRegex(RuntimeError, "a different seal"):
            driver.seal(self.campaign, "G2", 3, {"run": "G2", "blocks": {"3": {"attempts": [], "done": True}}})

    def test_a_raised_finishing_step_keeps_the_slot_and_latches(self):
        self.addCleanup(driver.SCRATCH_INFLIGHT.clear)
        self.addCleanup(driver.SCRATCH_LATCHED.clear)
        tree = self.root / "container" / "G1" / "b001-a1" / "native"
        tree.mkdir(parents=True)
        driver.SCRATCH_INFLIGHT.add(str(tree))
        with patch.object(driver, "finish_block", side_effect=OSError("prune failed")):
            with self.assertRaises(OSError):
                driver.finished(self.campaign, "G1", 1, {"blocks": {"1": {}}}, {}, {"native": str(tree)})
        self.assertIn(str(tree), driver.SCRATCH_INFLIGHT)
        self.assertTrue((self.campaign.state / driver.SCRATCH_LATCH).exists())
        with patch.object(driver, "discard", side_effect=OSError("discard failed")):
            with self.assertRaises(OSError):
                driver.discarded(self.campaign, "G1", 1, {"native": str(tree)}, "test")
        self.assertIn(str(tree), driver.SCRATCH_INFLIGHT)

    def test_the_window_counts_blocks_in_flight_and_a_full_one_drains(self):
        self.addCleanup(driver.SCRATCH_INFLIGHT.clear)
        self.records_campaign(window=1, stall=60)
        seals = self.campaign.state / "seals"
        seals.mkdir(parents=True)
        for k in range(5):
            (seals / f"G{k + 1}-b003.json").write_text("{}")
        self.assertIsNone(driver.records_hold(self.campaign, in_flight=2))
        self.assertRegex(driver.records_hold(self.campaign, in_flight=3), "^records: window full, 5 sealed and 3")
        state = {"run": "G6", "status": "running", "blocks": {}}
        with patch.object(driver.time, "sleep") as slept:  # nothing in flight: a full window waits for the drain
            self.assertTrue(driver.hold(self.campaign, "G6", 3, state, "records: window full, 8 sealed", True))
            self.assertFalse(driver.hold(self.campaign, "G6", 3, state, "records: copy-back stalled, 8 wait", True))
        self.assertEqual(slept.call_count, 1)
        self.assertTrue(state["stop_reason"].startswith("storage admission: records: copy-back stalled"))

    def test_a_drain_without_progress_stops_before_the_lease_guard_and_progress_counts_as_work(self):
        self.addCleanup(driver.DRAIN.update, {"moved": -1, "at": driver.DRIVER_STARTED})
        self.records_campaign()
        markers = self.campaign.state / "moved"
        markers.mkdir(parents=True)
        state = {"run": "G6", "status": "running", "blocks": {}}
        touched = []
        with patch.object(driver, "touch", lambda campaign: touched.append(1)), patch.object(driver.time, "sleep"):
            driver.DRAIN.update(moved=-1, at=time.time())
            self.assertTrue(driver.hold(self.campaign, "G6", 3, state, "records: window full, 64 sealed", True))
            self.assertEqual(len(touched), 1)  # the first count it sees
            (markers / "G1-b003.json").write_text('{"deleted": true}')
            self.assertTrue(driver.hold(self.campaign, "G6", 3, state, "records: window full, 63 sealed", False))
            self.assertEqual(len(touched), 2)  # a move it saw is driver work for the guard
            driver.DRAIN["at"] = time.time() - driver.DRAIN_IDLE_SECONDS - 1
            self.assertFalse(driver.hold(self.campaign, "G6", 3, state, "records: window full, 63 sealed", False))
        self.assertIn("window full, 63 sealed; no copy-back progress in 180 s", state["stop_reason"])

    def test_a_retained_refusal_drains_while_sealed_blocks_wait(self):
        self.records_campaign()
        state = {"run": "G6", "status": "running", "blocks": {}}
        with patch.object(driver.time, "sleep"):
            self.assertFalse(driver.hold(self.campaign, "G6", 3, state, "retained: over the cap", True))  # none sealed
            (self.campaign.state / "seals").mkdir(parents=True)
            (self.campaign.state / "seals" / "G1-b003.json").write_text("{}")
            driver.DRAIN.update(moved=-1, at=time.time())
            self.addCleanup(driver.DRAIN.update, {"moved": -1, "at": driver.DRIVER_STARTED})
            self.assertTrue(driver.hold(self.campaign, "G6", 3, state, "retained: over the cap", True))

    def test_a_restart_reconciles_seals_and_reserves_resumed_completions_first(self):
        self.addCleanup(driver.SCRATCH_INFLIGHT.clear)
        self.records_campaign()
        for run, blocks in (("G1", {"3": {"attempts": [], "done": True}}),
                            ("G2", {"3": {"attempts": [{"attempt": 1, "native": "n-G2", "outcome": "complete"}]}}),
                            ("G3", {"3": {"attempts": [{"attempt": 1, "native": "n-G3", "outcome": "complete"}],
                                          "disposition": "failed: invalid block (non-finite losses)"}})):
            self.campaign.save_state(run, {"run": run, "status": "running", "blocks": blocks})
        sealed = []
        with patch.object(driver, "seal", lambda campaign, run, block, state: sealed.append((run, block))):
            driver.reconcile(self.campaign, ["G1", "G2", "G3"])
        self.assertEqual(sealed, [("G1", 3)])
        self.assertEqual(driver.SCRATCH_INFLIGHT, {"n-G2"})  # counted before any admission; G3's ended for good

    def test_a_finished_block_is_sealed_before_its_slot_frees(self):
        order = []
        with patch.object(driver, "finish_block", lambda *a: order.append("finish") or "complete"), \
                patch.object(driver, "seal", lambda *a: order.append("seal")), \
                patch.object(driver, "release", lambda *a: order.append("release")):
            driver.finished(self.campaign, "G1", 3, {"blocks": {"3": {}}}, {}, {"native": "n"})
        self.assertEqual(order, ["finish", "seal", "release"])

    def test_uncounted_storage_attempts_are_bounded_per_session(self):
        self.addCleanup(driver.SCRATCH_INFLIGHT.clear)
        state = {"run": "G4", "status": "running", "blocks": {}}
        refused = RuntimeError("dispatch refused: ValueError: retained plus projected storage exceeds allowance")
        driver.SCRATCH_INFLIGHT.add("another block in flight")  # so each refusal waits and retries
        with patch.object(driver, "admit", return_value=None), \
                patch.object(driver, "release"), \
                patch.object(driver, "block_spec", return_value={}), \
                patch.object(driver.g, "block_config", return_value={}), \
                patch.object(driver.g, "require_runtime_rules"), \
                patch.object(driver, "request_for", return_value={}), \
                patch.object(driver.Campaign, "wall_stop", return_value=None), \
                patch.object(driver.time, "sleep"), \
                patch.object(driver.base, "dispatch_block", side_effect=refused):
            self.assertEqual(driver.run_block(self.campaign, "G4", 1, state), "stopped")
        entry = state["blocks"]["1"]
        self.assertEqual([a["outcome"] for a in entry["attempts"]], ["storage-refused"] * 3)
        self.assertEqual(driver.counted(entry), 0)
        self.assertEqual(state["stop_reason"], "storage admission: repeated storage refusals or kills this session")

    def test_status_and_run_labels(self):
        self.assertEqual(len(driver.status_lines(self.campaign)), 8)
        for runs in ("r1", "G1,G1", "G9"):
            with self.assertRaises(SystemExit):
                driver.main(["run", "--campaign", str(self.path), "--runs", runs])


if __name__ == "__main__":
    unittest.main()
