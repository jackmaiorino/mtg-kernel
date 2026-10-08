"""Campaign driver pieces that need no reservation or native execution."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import nine_deck_campaign_v1 as driver

REPO = Path(__file__).resolve().parents[3]
LINUX = sys.platform.startswith("linux")


def lease_guard_files(folder):
    """A valid lease (lease_guard.validate) and a fresh unlatched guard.json for pod testpod."""
    now = time.time()
    lease = {"schema": "phase1-cloud-lease/v1", "name": "nine-deck-test", "network_volume_id": "volume",
             "created_epoch": now - 600, "deadline_epoch": now - 600 + 4 * 3600, "increment_cap_usd": 10,
             "total_cap_usd": 100, "prior_conservative_usd": 0, "rate_ceiling_usd_hour": 1.0,
             "storage_usd_hour": 0.1, "postrun_storage_reserve_usd": 0.5, "recovery_reserve_usd": 0.5,
             "funded_balance_usd": 50, "safety_multiplier": 1.25, "startup_idle_seconds": 900,
             "work_idle_seconds": 300, "recovery_seconds": 600, "poll_seconds": 30}
    Path(folder).mkdir(parents=True)
    (Path(folder) / "lease.json").write_text(json.dumps(lease), encoding="utf-8")
    (Path(folder) / "guard.json").write_text(json.dumps({"pod_id": "testpod", "name": lease["name"], "epoch": now,
                                                          "latched": None, "release_epoch": None}), encoding="utf-8")
    return lease


class DriverTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        t1 = {"checkpoint": {"path": "C:/in/t1.json", "sha256": "a" * 64},
              "feature_transfer": {"expected_feature_contract_digest": "c"}, "play_import": {"path": "C:/in/i.json"}}
        raw = {"lane": "test", "decks": str(REPO / "data" / "runtime_decks_v1.json"), "t1_source": t1,
               "a48_source": {"checkpoint": {"path": "C:/in/a48.json", "sha256": "b" * 64}},
               "placement": {"host": "desktop", "workers": 8, "preparation_workers": 8},
               "runtime": {"path": "r", "sha256": "r"}, "storage": {}, "wall_seconds": 10,
               "choice": {"path": "c"}, "choice_verification": {"path": "v"}, "cold_keep_blocks": 2,
               "roots": {name: str(self.root / name) for name in ("state", "cold", "retained", "checkpoints",
                                                                    "exposure", "hot")} | {"hot_by_run": {}}}
        path = self.root / "campaign.json"
        path.write_text(json.dumps(raw), encoding="utf-8")
        self.campaign = driver.Campaign(path)

    def test_chained_sources(self):
        state = {"blocks": {"3": {"final_checkpoint": {"path": "D:/k/r1-b3.json", "sha256": "d" * 64}}}}
        self.assertEqual(driver.chained_source(self.campaign, state, 1), self.campaign.raw["t1_source"])
        source = driver.chained_source(self.campaign, state, 4)
        self.assertEqual(source["checkpoint"]["sha256"], "d" * 64)
        self.assertEqual(source["play_import"], self.campaign.raw["t1_source"]["play_import"])

    def test_block_config_and_request(self):
        config = driver.block_config(self.campaign, "r2", 1, {"blocks": {}}, self.root / "native")
        self.assertEqual(config["collection_workers"], 8)
        self.assertEqual(config["max_non_natural_episode_fraction"], 0.2)
        request = driver.request_for(self.campaign, "r2", 1, {"path": "x", "sha256": "y"}, self.root / "a",
                                     self.root / "b")
        self.assertEqual(request["schedule_family"], "permuted-units-v1")
        self.assertEqual(request["non_natural_tolerance"], config["max_non_natural_episode_fraction"])

    def test_failure_classification(self):
        (self.root / "execution.json").write_text(json.dumps({"error": "native process failed"}))
        (self.root / "stderr.log").write_text("collection: non-natural episode fraction 0.3 exceeds 0.2")
        self.assertEqual(driver.classify_failure(self.root), "invalid")
        (self.root / "stderr.log").write_text("os error 1450")
        self.assertEqual(driver.classify_failure(self.root), "interrupted")

    def test_retain_keeps_records_and_sampled_trajectories_then_prunes(self):
        native = self.root / "native"
        for update in range(3):
            base = native / "iterations" / f"{update:06d}" / "attempt-000000"
            (base / "collect").mkdir(parents=True)
            (base / "update").mkdir(parents=True)
            (base / "collect" / "episode-0000.json").write_text(f"trajectory {update}")
            (base / "collect" / "collection.json").write_text("{}")
            (base / "update" / "update.json").write_text("{}")
            (base / "update" / "checkpoint.json").write_text("big")
        (native / "run.json").write_text("{}")
        manifest = driver.retain_block(self.campaign, "r1", 1, native, {"retained_updates": [1]})
        self.assertFalse(native.exists())
        kept = {Path(item["to"]).relative_to(self.root / "retained" / "r1" / "b01").as_posix()
                for item in json.loads(Path(manifest["path"]).read_text())["files"]}
        self.assertIn("run.json", kept)
        self.assertIn("iterations/000001/attempt-000000/collect/episode-0000.json", kept)
        self.assertNotIn("iterations/000000/attempt-000000/collect/episode-0000.json", kept)
        self.assertFalse(any(name.endswith("checkpoint.json") for name in kept))
        self.assertEqual(sum(name.endswith("update.json") for name in kept), 3)
        log = [json.loads(line) for line in self.campaign.prune_log.read_text().splitlines()]
        self.assertEqual(log[0]["files"], 13)


    def test_declared_cores_must_match_placement_and_pin(self):
        self.assertEqual(driver.declared_cores(self.campaign), [])
        tool = self.root / "host_slots_v1.py"; tool.write_text("tool")
        self.campaign.raw["host_slots"] = {"cores": "0-3", "tool": driver.pin(tool)}
        self.campaign.raw["placement"]["cpu_affinity"] = [0, 1, 2, 3]
        prefix = driver.declared_cores(self.campaign)
        self.assertEqual(prefix[-4:], ["timed", "--cores", "0-3", "--"])
        self.campaign.raw["placement"]["cpu_affinity"] = list(range(8))
        with self.assertRaises(SystemExit):
            driver.declared_cores(self.campaign)
        self.campaign.raw["placement"]["cpu_affinity"] = [0, 1, 2, 3]
        tool.write_text("changed")
        with self.assertRaises(SystemExit):
            driver.declared_cores(self.campaign)

    # ---------------------------------------------------------------- runpod

    def runpod(self):
        self.campaign.raw["placement"]["host"] = "runpod"
        self.campaign.raw["wall_seconds"] = 7200

    def test_runpod_block_estimate_uses_measured_runpod_blocks(self):
        self.runpod()
        self.assertEqual(driver.block_estimate(self.campaign), 7200)
        self.campaign.raw["lease_block_seconds"] = 3000
        self.assertEqual(driver.block_estimate(self.campaign), 3000)
        attempts = lambda *rows: {"attempts": [dict(zip(("host", "seconds", "outcome"), row)) for row in rows]}
        self.campaign.save_state("r1", {"run": "r1", "status": "running", "blocks": {
            "1": attempts(("desktop", 9000, "complete")), "2": attempts((None, None, "interrupted"),
                                                                        ("runpod", 400, "complete"))}})
        self.campaign.save_state("r3", {"run": "r3", "status": "running", "blocks": {
            "1": attempts(("runpod", 800, "complete"))}})
        self.assertEqual(driver.block_estimate(self.campaign), 1000)
        request = driver.request_for(self.campaign, "r1", 3, {"path": "x", "sha256": "y"}, self.root / "a",
                                     self.root / "b", 1000.2)
        self.assertEqual(request["lease_work_seconds"], 1001)
        self.assertNotIn("lease_work_seconds", driver.request_for(self.campaign, "r1", 3, {"path": "x"},
                                                                   self.root / "a", self.root / "b"))

    def test_runpod_stops_at_the_block_boundary_when_the_lease_cannot_cover_a_block(self):
        self.runpod()
        self.campaign.raw["lease_block_seconds"] = 3000
        for error, reason in (("lease time exhausted", "lease_time_exhausted"),
                              ("lease guard is stale", "lease_guard_refused: lease guard is stale")):
            state = {"run": "r1", "status": "running", "blocks": {}}
            with patch.object(driver.native_dispatch, "runpod_lease", side_effect=ValueError(error)) as lease, \
                    patch.object(driver, "dispatch_block", side_effect=AssertionError("dispatched")):
                self.assertEqual(driver.run_block(self.campaign, "r1", 1, state), "stopped")
            lease.assert_called_once_with(3000 + driver.LEASE_MARGIN_SECONDS)
            self.assertEqual(state["stop_reason"], reason)
            self.assertEqual(state["blocks"]["1"], {"attempts": []})  # no attempt is spent
            self.assertNotIn("disposition", state["blocks"]["1"])
        events = [json.loads(line) for line in self.campaign.events.read_text().splitlines()]
        self.assertEqual([e["reason"] for e in events if e["kind"] == "lease-stop"],
                         ["lease_time_exhausted", "lease_guard_refused: lease guard is stale"])
        self.campaign.save_state("r1", state)
        self.assertIn("stop_reason=lease_guard_refused", driver.status_lines(self.campaign)[0])
        with patch.object(driver, "run_block", return_value="complete"):
            driver.drive(self.campaign, "r1")
        resumed = self.campaign.run_state("r1")
        self.assertEqual(resumed["status"], "complete")
        self.assertNotIn("stop_reason", resumed)

    def test_desktop_campaign_never_consults_a_lease(self):
        with patch.object(driver.native_dispatch, "runpod_lease", side_effect=AssertionError("lease")), \
                patch.object(driver, "block_config", side_effect=RuntimeError("past the boundary")), \
                self.assertRaisesRegex(RuntimeError, "past the boundary"):
            driver.run_block(self.campaign, "r1", 1, {"run": "r1", "blocks": {}})

    def test_launch_busy_pattern_names_the_linux_binary(self):
        runtime = self.root / "runtime.json"
        runtime.write_text(json.dumps({"binary": {"path": "/opt/bin/nine-deck-trainer"}}))
        self.campaign.raw["runtime"] = {"path": str(runtime)}
        self.campaign.path.write_text(json.dumps(self.campaign.raw), encoding="utf-8")
        with patch.object(driver.native_dispatch, "WINDOWS", False):
            self.assertTrue(re.search(driver.busy_pattern(self.campaign), "nine-deck-trainer"))
        with patch.object(driver.native_dispatch, "WINDOWS", True):
            self.assertEqual(driver.busy_pattern(self.campaign),
                             r"native_expanded_training|expanded_deck_training|cargo|rustc|trainer\.exe")
        with patch.object(driver.reservations, "dispatch", return_value={}) as dispatch:
            driver.launch(self.campaign.path, ["r1"])
        self.assertEqual(dispatch.call_args.kwargs["busy_pattern"], driver.busy_pattern(self.campaign))

    @unittest.skipUnless(LINUX, "lease progress reads native CPU time from /proc")
    def test_lease_progress_keeps_the_guard_satisfied_and_finishes(self):
        driver.native_dispatch.lease_guard()
        import lease_guard
        self.runpod()
        folder = self.root / "guard"
        lease = lease_guard_files(folder)
        with patch.dict(os.environ, {"RUNPOD_POD_ID": "testpod", driver.native_dispatch.LEASE_GUARD_ENV: str(folder)}):
            progress = driver.LeaseProgress(self.campaign)
        self.assertEqual(progress.interval, driver.HEARTBEAT_SECONDS)
        def record():
            value = json.loads((folder / "progress.json").read_text())
            self.assertEqual(value["pod_id"], "testpod")
            self.assertTrue(lease["created_epoch"] <= value["last_productive_epoch"] <= time.time() + 5)
            return value
        progress.start()
        value = record()
        self.assertIsNone(lease_guard.decision(lease, time.time(), 1.0, value)["reason"])
        self.assertFalse(value["native_alive"] or value["queued_work"] or value["finished"])
        # A sleeping native process is alive but idle; a computing one is active.
        children = {}
        for name, code in (("idle", "import time; time.sleep(30)"), ("busy", "while True: pass")):
            children[name] = subprocess.Popen([sys.executable, "-c", code])
            self.addCleanup(children[name].wait)
            self.addCleanup(children[name].kill)
            root = self.root / name; root.mkdir()
            (root / "started.json").write_text(json.dumps({"pid": children[name].pid}))
        time.sleep(1)  # interpreter start-up is over
        progress.active["r1"] = {"attempts": [{"root": str(self.root / "idle")}]}
        progress.beat(); time.sleep(0.3); progress.beat()
        value = record()
        self.assertTrue(value["native_alive"] and value["queued_work"])
        self.assertLess(value["last_activity_epoch"], value["epoch"])
        progress.active["r1"] = {"attempts": [{"root": str(self.root / "busy")}]}
        progress.beat(); time.sleep(0.3); progress.beat()
        value = record()
        self.assertEqual(value["last_activity_epoch"], value["epoch"])
        self.assertIsNone(lease_guard.decision(lease, time.time(), 1.0, value)["reason"])
        (self.root / "busy" / "execution.json").write_text("{}")  # archiving or finishing: the driver works
        progress.completed()
        value = record()
        self.assertFalse(value["native_alive"])
        self.assertEqual(value["last_productive_epoch"], progress.productive)
        progress.active.clear()
        progress.stop()
        value = record()
        self.assertTrue(value["finished"])
        self.assertEqual(lease_guard.decision(lease, time.time(), 1.0, value)["reason"], "worker_finished")
        self.assertFalse(progress.thread.is_alive())

    def test_runpod_progress_needs_the_pod_and_guard(self):
        self.runpod()
        with patch.dict(os.environ, {}, clear=True), self.assertRaises(SystemExit):
            driver.LeaseProgress(self.campaign)


if __name__ == "__main__":
    unittest.main()
