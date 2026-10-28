"""G(i) qualification pieces that need no reservation or native execution."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

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
               "storage": {"accounting_roots": [str(self.root)]}, "wall_seconds": 7200,
               "choice_pins": str(self.root / "pins.json"), "cold_keep_blocks": 1, "stop_after_block": 2,
               "roots": {name: str(self.root / "campaign" / name)
                         for name in ("state", "cold", "retained", "checkpoints", "exposure", "hot", "doses")}
                        | {"hot_by_run": {}}}
        self.path = self.root / "campaign.json"
        self.path.write_text(json.dumps(raw), encoding="utf-8")
        self.campaign = driver.Campaign(self.path)

    def test_plan_writes_trials_and_a_pack_of_the_eight_slots(self):
        inventory = save(self.root / "inventory.json", b"{}")
        spec = {"storage": {"accounting_roots": [str(self.root)]}, "wall_seconds": 3600, "inventory": inventory}
        result = qualify.plan(self.path, spec, self.root / "q")
        value = json.loads(Path(result["path"]).read_text())
        self.assertEqual(sorted(value["trials"]), ["w1", "w3"])
        serial = json.loads(Path(value["trials"]["w1"]["path"]).read_text())
        self.assertEqual((serial["placement"]["workers"], serial["placement"]["preparation_workers"]), (1, 1))
        self.assertEqual(serial["placement"]["cpu_affinity"], [0, 1, 2])
        self.assertEqual(serial["schedule_family"], "permuted-units-own-history-v1")
        self.assertNotIn("choice", serial)
        pack = json.loads(Path(value["pack"]["path"]).read_text())
        self.assertEqual(len(pack["slots"]), 8)
        self.assertEqual(pack["base_request"], value["trials"]["w3"])
        config = json.loads(Path(value["config"]["path"]).read_text())
        self.assertIsNone(config["initial_source"]["checkpoint"])
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
        seeded = qualify.seeded_state(self.campaign, "G1", {1: 1, 2: 2, 3: 2})
        scratch.save_state("G1", seeded)
        spec = driver.block_spec(scratch, "G1", 4, scratch.run_state("G1"), self.root / "n")
        main = self.campaign.run_state("G1")["blocks"]
        self.assertEqual(spec["start_checkpoint"], main["2"]["final_checkpoint"])
        self.assertEqual(sorted(spec["history"]), ["1", "2"])
        config = g.block_config(spec)
        drawn = {item["opponent"].get("id") for u in config["iterations"] for item in u["episodes"]}
        self.assertEqual(drawn - {None}, {"G1-history-b001", "G1-history-b002"})

    def test_compare_and_receipt_fields(self):
        self.seed_main()
        straight = qualify.end_of(self.campaign, "G1", 2)
        rerun = qualify.scratch(self.campaign, "interrupt", 2, self.root / "q")
        state = qualify.seeded_state(self.campaign, "G1", {1: 1})
        main = self.campaign.run_state("G1")["blocks"]["2"]
        state["blocks"]["2"] = dict(main, attempts=[{"attempt": 1, "outcome": "interrupted"},
                                                    {"attempt": 2, "outcome": "complete"}])
        rerun.save_state("G1", state)
        result = qualify.compare(rerun, "G1", 2, straight)
        self.assertTrue(result["byte_identical"] and result["interrupted"])
        state["blocks"]["2"]["final_state_sha256"] = "f" * 64
        rerun.save_state("G1", state)
        self.assertFalse(qualify.compare(rerun, "G1", 2, straight)["byte_identical"])

    def test_refuses_outside_a_reservation(self):
        with self.assertRaises(SystemExit):
            qualify.run(self.path, self.root / "plan.json")


if __name__ == "__main__":
    unittest.main()
