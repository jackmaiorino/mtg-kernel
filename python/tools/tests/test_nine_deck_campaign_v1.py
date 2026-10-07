"""Campaign driver pieces that need no reservation or native execution."""
import json
from pathlib import Path
import tempfile
import unittest

import nine_deck_campaign_v1 as driver

REPO = Path(__file__).resolve().parents[3]


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


if __name__ == "__main__":
    unittest.main()
