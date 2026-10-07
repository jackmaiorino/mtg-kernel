"""Schedule rules of the nine-deck baseline generator, without native execution."""
import hashlib
import json
from collections import Counter
from pathlib import Path
import tempfile
import unittest

import nine_deck_baseline_v1 as ndb

REPO = Path(__file__).resolve().parents[3]
DECKS = REPO / "data" / "runtime_decks_v1.json"


class ScheduleTests(unittest.TestCase):
    def test_pinned_encoding(self):
        self.assertEqual(ndb.key_bytes(["nine-deck-baseline-v1/order", "r1", 1, 0, 7, 9]),
                         b'["nine-deck-baseline-v1/order","r1",1,0,7,9]')
        digest = hashlib.sha256(b'["nine-deck-baseline-v1/train","r1",1,4,7,3]').digest()
        self.assertEqual(ndb.unit_seed("r1", 1, 4, 7, 3), int.from_bytes(digest[:8], "little") & (2**63 - 1))

    def test_block_balance_and_design_checks(self):
        summary = ndb.block_summary("r1", 1)
        self.assertEqual(summary["updates"], 162)
        self.assertEqual(summary["learner_episodes_by_deck"], [180] * 9)
        self.assertEqual(summary["opponent_episodes_by_deck"], [180] * 9)
        self.assertEqual(summary["episodes_by_kind"], {"current": 486, "fixed": 324, "initial": 810})
        self.assertEqual(summary["episodes_per_ordered_pair"], [20])
        # Figures published in DESIGN.md "Order" for r1, block 1.
        self.assertEqual(summary["updates_mixing_kinds"], 159)
        self.assertEqual(round(summary["mean_distinct_own_decks"], 2), 3.93)

    def test_units_never_split_and_every_unit_once(self):
        updates = ndb.block_units("r3", 17)
        self.assertTrue(all(len(update) == 5 for update in updates))
        units = [unit for update in updates for unit in update]
        self.assertEqual(len(units), 810)
        self.assertEqual(len(set(units)), 810)

    def test_starting_player_balanced_within_kind(self):
        counts = Counter((ndb.opponent_kind(j), ndb.starting_player(o, t, j))
                         for o in range(9) for t in range(9) for j in range(10))
        for kind in ("current", "initial", "fixed"):
            self.assertLessEqual(abs(counts[(kind, 0)] - counts[(kind, 1)]), 1)

    def test_all_training_seeds_distinct_and_masked(self):
        seeds = ndb.all_seeds()
        train = [row["seed"] for row in seeds["train"]]
        self.assertEqual(len(train), 97200)
        self.assertEqual(len(set(train)), 97200)
        self.assertTrue(all(0 <= seed < 2**63 for seed in train))
        self.assertFalse(set(train) & {row["seed"] for row in seeds["dev"]})

    def test_rejects_unknown_run_and_block(self):
        for args in (("r7", 1), ("r1", 0), ("r1", 21)):
            with self.assertRaises(ValueError):
                ndb.block_units(*args)

    def test_block_config_episodes(self):
        with tempfile.TemporaryDirectory() as temp:
            spec = {"run": "r2", "block": 5, "runtime_decks": str(DECKS),
                    "initial_source": {"checkpoint": {"path": "C:/x/init.json", "sha256": "a" * 64}},
                    "fixed_opponent": {"checkpoint": {"path": "C:/x/a48.json", "sha256": "b" * 64}},
                    "update_backend": {"kind": "cpu"}, "collection_workers": 8, "preparation_workers": 8,
                    "output_directory": str(Path(temp) / "out")}
            config = ndb.block_config(spec)
        self.assertEqual(config["max_non_natural_episode_fraction"], 0.2)
        self.assertEqual(config["loss_selection"]["kind"], "gae_advantage_value_v1")
        self.assertEqual([o["id"] for o in config["opponents"]], ["fresh-a-block48-end"])
        self.assertEqual(len(config["iterations"]), 162)
        first = config["iterations"][0]["episodes"]
        self.assertEqual(len(first), 10)
        for pair in (first[0:2], first[2:4]):
            seat0, seat1 = (item["episode"] for item in pair)
            self.assertEqual((seat0["learner_seat"], seat1["learner_seat"]), (0, 1))
            self.assertEqual(seat0["seed"], seat1["seed"])
            self.assertEqual(seat0["registered"][0]["label"], seat1["registered"][1]["label"])
            self.assertEqual(seat0["selected"], seat0["registered"])
            self.assertEqual(seat0["max_physical_decisions"], 100000)
            self.assertEqual(seat0["max_policy_steps"], 200000)
        learner = Counter(item["episode"]["selected"][item["episode"]["learner_seat"]]["label"]
                          for update in config["iterations"] for item in update["episodes"])
        self.assertEqual(set(learner.values()), {180})
        ids = [item["episode"]["id"] for update in config["iterations"] for item in update["episodes"]]
        self.assertEqual(len(set(ids)), 1620)
        json.dumps(config)

    def test_panel_cases(self):
        cases = ndb.panel_cases()
        self.assertEqual(len(cases), 1944)
        self.assertEqual(len(ndb.panel_cases((4, 7))), 432)
        self.assertEqual(len({(c["own"], c["other"], c["repeat"]) for c in cases}), 972)


if __name__ == "__main__":
    unittest.main()
