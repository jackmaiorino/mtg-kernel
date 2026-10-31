"""G(i) campaign driver pieces that need no reservation or native execution."""
import gzip
import hashlib
import json
import os
import shutil
from pathlib import Path
import struct
import tempfile
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
            self.assertRegex(driver.admit(self.campaign, self.root / "container" / "c"), "^retained: retained plus")

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

        def attempt(name, outcome, files):
            root = self.root / name
            root.mkdir()
            for file, value in files.items():
                (root / file).write_text(json.dumps(value))
            return {"attempt": 1, "root": str(root), "outcome": outcome}
        killed_after_run = attempt("a", "interrupted", {"execution.json": {"exit_code": 0, "error": None}})
        killed_in_run = attempt("b", "interrupted", {"execution.json": {
            "exit_code": -9, "error": "retained plus projected storage exceeds allowance"}})
        recorded = attempt("c", "interrupted", {"execution.json": {"exit_code": 0, "error": None},
                                                "failure.json": {"error": "ValueError: [Errno 28] No space left on device"}})
        native_failed = attempt("d", "interrupted", {"execution.json": {"exit_code": 1, "error": "native process failed"}})
        other_failure = attempt("e", "interrupted", {"execution.json": {"exit_code": 0, "error": None},
                                                     "failure.json": {"error": "RuntimeError: retained copy differs"}})
        self.assertEqual([driver.counts(a) for a in (killed_after_run, killed_in_run, recorded, native_failed,
                                                     other_failure)], [False, False, False, True, True])
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

    def test_status_and_run_labels(self):
        self.assertEqual(len(driver.status_lines(self.campaign)), 8)
        for runs in ("r1", "G1,G1", "G9"):
            with self.assertRaises(SystemExit):
                driver.main(["run", "--campaign", str(self.path), "--runs", runs])


if __name__ == "__main__":
    unittest.main()
