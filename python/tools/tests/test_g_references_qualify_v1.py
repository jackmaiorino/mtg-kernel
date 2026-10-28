"""G(i) qualification pieces that need no reservation or native execution."""
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import g_references_campaign_v1 as driver
import g_references_qualify_v1 as qualify
import g_references_v1 as g

REPO = Path(__file__).resolve().parents[3]


def save(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return {"path": str(path), "sha256": hashlib.sha256(data).hexdigest()}


class QualifyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        play = {run: {"path": f"C:/in/{run}.json", "sha256": str(i) * 64} for i, run in enumerate(g.RUNS, 1)}
        slots = {run: {"host": "desktop", "cpu_affinity": [3 * i, 3 * i + 1, 3 * i + 2], "workers": 3,
                       "preparation_workers": 3, "memory_bytes": 4 << 30} for i, run in enumerate(g.RUNS)}
        raw = {"recipe": g.PREFIX, "lane": "gi", "decks": str(REPO / "data" / "runtime_decks_v1.json"),
               "play_imports": play, "placement": {**slots["G1"], "cpu_affinity": list(range(24))},
               "placement_by_run": slots, "runtime": {"path": "r", "sha256": "r" * 64},
               "storage": {"accounting_roots": [str(self.root)], "max_logical_bytes": 120 * 10**9},
               "wall_seconds": 7200,
               "choice_pins": str(self.root / "pins.json"), "cold_keep_blocks": 1, "stop_after_block": 2,
               "roots": {name: str(self.root / "campaign" / name)
                         for name in ("state", "cold", "retained", "checkpoints", "exposure", "hot", "doses")}
                        | {"hot_by_run": {}}}
        self.path = self.root / "campaign.json"
        self.path.write_text(json.dumps(raw), encoding="utf-8")
        self.campaign = driver.Campaign(self.path)

    def test_plan_writes_the_empty_pool_family_and_keeps_the_spec(self):
        inventory = save(self.root / "inventory.json", b"{}")
        spec = {"storage": {"accounting_roots": [str(self.root)]}, "wall_seconds": 3600, "inventory": inventory}
        result = qualify.plan(self.path, spec, self.root / "q")
        value = json.loads(Path(result["path"]).read_text())
        self.assertEqual(sorted(value["families"]), ["empty-pool"])
        self.assertEqual(value["spec"], spec)
        family = value["families"]["empty-pool"]
        self.assertEqual(sorted(family["trials"]), ["w1", "w3"])
        serial = json.loads(Path(family["trials"]["w1"]["path"]).read_text())
        self.assertEqual((serial["placement"]["workers"], serial["placement"]["preparation_workers"]), (1, 1))
        self.assertEqual(serial["placement"]["cpu_affinity"], [0, 1, 2])
        self.assertEqual(serial["schedule_family"], "permuted-units-own-history-v1")
        self.assertNotIn("choice", serial)
        pack = json.loads(Path(family["pack"]["path"]).read_text())
        self.assertEqual(len(pack["slots"]), 8)
        self.assertEqual(pack["base_request"], family["trials"]["w3"])
        config = json.loads(Path(family["config"]["path"]).read_text())
        self.assertIsNone(config["initial_source"]["checkpoint"])
        self.assertEqual(g.require_runtime_rules(config), 1620)
        self.assertEqual(value["interrupt_at_update"], 81)

    def seed_main(self):
        state = {"run": "G1", "status": "complete", "blocks": {}}
        for block in (1, 2):
            item = save(driver.checkpoint_path(self.campaign, "G1", block), json.dumps({"b": block}).encode())
            state["blocks"][str(block)] = {"attempts": [], "done": True, "final_checkpoint": item,
                                           "final_adam_step": 162 * block, "final_state_sha256": f"{block:064d}",
                                           "checkpoint_state": "kept"}
        self.campaign.save_state("G1", state)

    def test_scratch_campaigns_and_seeded_states(self):
        self.seed_main()
        scratch = qualify.scratch(self.campaign, "synthetic", 4, self.root / "q")
        self.assertEqual(scratch.stop_after, 4)
        self.assertNotEqual(scratch.state, self.campaign.state)
        self.assertEqual(scratch.raw["choice_pins"], self.campaign.raw["choice_pins"])
        qualify.seeded(self.campaign, scratch, "G1", {1: 1, 2: 2, 3: 2})
        state = scratch.run_state("G1")
        state["blocks"]["4"] = {"attempts": [{"attempt": 1, "outcome": "interrupted"}]}
        scratch.save_state("G1", state)
        qualify.seeded(self.campaign, scratch, "G1", {1: 1, 2: 2, 3: 2})  # a restart keeps the scratch attempts
        self.assertIn("4", scratch.run_state("G1")["blocks"])
        spec = driver.block_spec(scratch, "G1", 4, scratch.run_state("G1"), self.root / "n")
        main = self.campaign.run_state("G1")["blocks"]
        self.assertEqual(spec["start_checkpoint"], main["2"]["final_checkpoint"])
        self.assertEqual(sorted(spec["history"]), ["1", "2"])
        config = g.block_config(spec)
        drawn = {item["opponent"].get("id") for u in config["iterations"] for item in u["episodes"]}
        self.assertEqual(drawn - {None}, {"G1-history-b001", "G1-history-b002"})

    def checkpoint(self, name, trajectory_root, state="s" * 64):
        value = {"state_sha256": state, "adam_step": 324, "parameters": [1, 2],
                 "trajectories": [{"path": f"{trajectory_root}/t{i}.json", "sha256": f"{i}" * 64} for i in range(3)]}
        return save(self.root / f"{name}.json", json.dumps(value).encode())

    def test_identity_ignores_output_paths_but_not_content(self):
        straight = qualify.checkpoint_identity(self.checkpoint("a", "D:/main"))
        moved = qualify.checkpoint_identity(self.checkpoint("b", "D:/scratch/x"))
        self.assertNotEqual(straight["file_sha256"], moved["file_sha256"])
        self.assertTrue(all(straight[k] == moved[k] for k in qualify.IDENTITY_KEYS))
        other = qualify.checkpoint_identity(self.checkpoint("c", "D:/main", state="t" * 64))
        self.assertNotEqual(other["state_sha256"], straight["state_sha256"])

    def test_compare_needs_identity_and_an_interruption(self):
        self.seed_main()
        rerun = qualify.scratch(self.campaign, "interrupt", 2, self.root / "q")
        qualify.seeded(self.campaign, rerun, "G1", {1: 1})
        straight = self.checkpoint("straight", "D:/main")
        state = rerun.run_state("G1")
        state["blocks"]["2"] = {"done": True, "final_checkpoint": self.checkpoint("rerun", "D:/scratch"),
                                "attempts": [{"attempt": 1, "outcome": "interrupted"},
                                             {"attempt": 2, "outcome": "complete"}]}
        rerun.save_state("G1", state)
        result = qualify.compare(rerun, "G1", 2, qualify.checkpoint_identity(straight))
        self.assertTrue(result["identical"] and result["interrupted"])
        state["blocks"]["2"]["attempts"] = [{"attempt": 1, "outcome": "complete"}]
        rerun.save_state("G1", state)
        self.assertFalse(qualify.compare(rerun, "G1", 2, qualify.checkpoint_identity(straight))["interrupted"])

    def test_kill_evidence_survives_a_restart(self):
        path = self.root / "q" / "stages" / "item3-kill.json"
        driver.write_json(path, {"killed": True, "pid": 7, "completed_updates": 81})
        self.assertIsNone(qualify.killer(self.campaign, "G1", 2, 81, path))  # no second watcher
        self.assertEqual(qualify.killed(None, path)["completed_updates"], 81)
        self.assertEqual(qualify.killed(None, self.root / "none.json")["killed"], False)
        with self.assertRaises(ValueError):
            qualify.scratch(self.campaign, "synthetic", 741, self.root / "q", synthetic=True)
        copy_ = qualify.scratch(self.campaign, "synthetic", 4, self.root / "q", synthetic=True)
        self.assertEqual(copy_.wall_root(), self.campaign.wall_root())

    def test_trials_are_bounded_by_the_claim_time_left(self):
        inventory = save(self.root / "inventory.json", b"{}")
        spec = {"storage": {"accounting_roots": [str(self.root)]}, "wall_seconds": 3600, "inventory": inventory}
        value = json.loads(Path(qualify.plan(self.path, spec, self.root / "q")["path"]).read_text())
        request = value["families"]["empty-pool"]["trials"]["w1"]
        self.assertEqual(qualify.bounded(self.campaign, request), request)  # plenty of time: unchanged
        wall = self.campaign.wall_root()
        wall.mkdir(parents=True)
        (wall / "1-1.json").write_text(json.dumps({"start": 0, "last": 60 * 3600 - 900}))
        bounded = qualify.bounded(self.campaign, request)
        self.assertEqual(json.loads(Path(bounded["path"]).read_text())["wall_seconds"], 300)
        (wall / "1-1.json").write_text(json.dumps({"start": 0, "last": 60 * 3600 - 630}))
        with self.assertRaises(SystemExit):
            qualify.bounded(self.campaign, request)
        # The pack: nine sequential phases at most (eight solo slots, then all at once) share the time left.
        (wall / "1-1.json").write_text(json.dumps({"start": 0, "last": 60 * 3600 - 1800}))
        family = value["families"]["empty-pool"]
        pack_root = Path(json.loads(Path(family["pack"]["path"]).read_text())["root"])
        calls = []

        def fake_run(args, **kwargs):
            calls.append(json.loads(Path(args[-1]).read_text()))
            pack_root.mkdir(parents=True, exist_ok=True)
            (pack_root / "pack.json").write_text("{}")
            return subprocess.CompletedProcess(args, 0, stdout="", stderr="")
        for name in family["trials"]:
            report = Path(json.loads(Path(family["trials"][name]["path"]).read_text())["root"]) / "report.json"
            report.parent.mkdir(parents=True, exist_ok=True)
            report.write_text("{}")
        with patch.object(qualify.subprocess, "run", side_effect=fake_run):
            qualify.trials_and_pack(self.campaign, family, self.root / "q", lambda **fields: None)
        self.assertEqual(calls[0]["timeout_seconds"], (1800 - 600) // 9)
        (pack_root / "pack.json").unlink()

        def slow(args, **kwargs):  # busy retries outlasting the claim time end the whole pack
            self.assertEqual(kwargs["timeout"], 1200)
            raise subprocess.TimeoutExpired(args, kwargs["timeout"])
        with patch.object(qualify.subprocess, "run", side_effect=slow), self.assertRaises(SystemExit):
            qualify.trials_and_pack(self.campaign, family, self.root / "q", lambda **fields: None)
        self.assertLessEqual(json.loads(Path(calls[0]["base_request"]["path"]).read_text())["wall_seconds"], 133)

    def test_refuses_outside_a_reservation(self):
        with self.assertRaises(SystemExit):
            qualify.run(self.path, self.root / "plan.json")

    def test_refuses_to_start_with_under_24_claim_hours_left(self):
        inventory = save(self.root / "inventory.json", b"{}")
        spec = {"storage": {"accounting_roots": [str(self.root)]}, "wall_seconds": 3600, "inventory": inventory}
        plan_pin = qualify.plan(self.path, spec, self.root / "q")
        wall = self.campaign.wall_root()
        wall.mkdir(parents=True)
        (wall / "1-1.json").write_text(json.dumps({"start": 0, "last": 37 * 3600}))
        with patch.object(qualify.reservations, "status", return_value={"token_fate": "holds"}),                 patch.dict(qualify.os.environ, {qualify.reservations.TOKEN_ENV: "t"}),                 self.assertRaisesRegex(SystemExit, "24 claim hours"):
            qualify.run(self.path, Path(plan_pin["path"]))


if __name__ == "__main__":
    unittest.main()
