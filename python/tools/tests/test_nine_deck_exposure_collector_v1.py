"""Exposure collector on a synthetic block tree (no native execution)."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

import nine_deck_baseline_v1 as ndb
import nine_deck_exposure_collector_v1 as collector

REPO = Path(__file__).resolve().parents[3]
DECKS = REPO / "data" / "runtime_decks_v1.json"


def pin(path: Path, value) -> dict:
    path.parent.mkdir(parents=True, exist_ok=True)
    data = json.dumps(value).encode()
    path.write_bytes(data)
    return {"path": str(path), "sha256": hashlib.sha256(data).hexdigest()}


class CollectorTests(unittest.TestCase):
    def build(self, root: Path, run="r1", block=1, substeps=lambda own: 3, fail_slot=None):
        decks = ndb.load_decks(DECKS)
        iterations = ndb.block_iterations(run, block, decks)
        pin(root / "run.json", {"config": {"iterations": iterations}})
        items = []
        for index, update in enumerate(iterations):
            base = root / "iterations" / f"{index:06d}"
            pins, ledger, total = [], [], 0
            for slot, item in enumerate(update["episodes"]):
                episode = dict(item["episode"])
                own = collector.parse_episode_id(episode["id"], run, block)[0]
                if fail_slot == (index, slot):
                    ledger.append({"slot": slot, "attempt": 1, "episode_id": episode["id"], "seed": episode["seed"],
                                   "terminal_classification": "max_policy_steps", "terminal_reason": "x"})
                    episode["seed"] = collector.derived_retry_seed(episode["id"], episode["seed"], 1)
                learner = episode["learner_seat"]
                rows = [{"actor": learner, "physical_decision_id": n} for n in range(substeps(own))] + \
                       [{"actor": 1 - learner, "physical_decision_id": 99}]
                total += substeps(own)
                pins.append(pin(base / f"episode-{slot:04d}.json", {
                    "episode": episode, "decisions": rows,
                    "terminal": {"terminal_classification": "natural", "winner": "p0", "terminal_reward": [1, -1],
                                 "policy_step_count": len(rows), "physical_decision_count": len(rows)}}))
            collection = {"complete": True, "trajectories": pins}
            if ledger:
                collection["non_natural_ledger"] = pin(base / "non-natural.json", {"entries": ledger})
            update_record = {"complete": True, "adam_step": 32401 + index, "policy_substeps": total,
                             "physical_decisions": total, "loss": 0.5,
                             "checkpoint": {"path": "x", "sha256": f"{index:064d}"}}
            items.append(pin(base / "complete.json", {
                "iteration": index, "collection": pin(base / "collection.json", collection),
                "update": pin(base / "update.json", update_record)}))
        pin(root / "completion.json", {"complete": True, "completed_iterations": 162, "iterations": items})

    def test_balanced_block_passes(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.build(root / "native", fail_slot=(3, 4))
            summary = collector.collect("r1", 1, root / "native", DECKS, root / "out")
        self.assertEqual(summary["verdict"], "pass")
        self.assertEqual(summary["rows"]["count"], 1620)
        self.assertEqual(len(summary["ledger"]), 1)
        self.assertEqual(summary["checks"]["completed_episodes"]["learner_by_deck"], [180] * 9)
        self.assertEqual(len(summary["retained_updates"]), 4)

    def test_spy_substep_floor_fails(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.build(root / "native", substeps=lambda own: 1 if own == ndb.DECK_NAMES.index("Spy") else 4)
            summary = collector.collect("r1", 1, root / "native", DECKS, root / "out")
        self.assertFalse(summary["checks"]["optimizer_samples"]["pass"])
        self.assertEqual(summary["verdict"], "fail")

    def test_wrong_block_schedule_refused(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.build(root / "native", block=2)
            with self.assertRaises(ValueError):
                collector.collect("r1", 1, root / "native", DECKS, root / "out")

    def test_retry_seed_matches_engine_layout(self):
        payload = b"mtg-kernel-non-natural-retry-seed/v1\0e\0" + (5).to_bytes(8, "big") + (2).to_bytes(4, "big")
        self.assertEqual(collector.derived_retry_seed("e", 5, 2),
                         int.from_bytes(hashlib.sha256(payload).digest()[:8], "big"))

    def test_embedding_gate(self):
        decks = ndb.load_decks(DECKS)
        cards = collector.exclusive_cards(decks)
        self.assertEqual((len(cards["Spy"]), len(cards["CawGates"])), (14, 16))
        rows, width = 200, 2
        start = {"parameters": [{"name": "card_embedding.weight", "shape": [rows, width], "values": [0] * rows * width}]}
        moved = [0] * rows * width
        for card in cards["Spy"] + cards["CawGates"]:
            moved[(card + 1) * width] = 1
        end = {"parameters": [{"name": "card_embedding.weight", "shape": [rows, width], "values": moved}],
               "first_moments": [{"name": "card_embedding.weight", "shape": [rows, width], "values": moved}],
               "second_moments": [{"name": "card_embedding.weight", "shape": [rows, width], "values": moved}]}
        self.assertTrue(collector.embedding_gate(start, end, decks)["pass"])
        end["parameters"][0]["values"] = [0] * rows * width
        self.assertFalse(collector.embedding_gate(start, end, decks)["pass"])


if __name__ == "__main__":
    unittest.main()
