"""Panel evaluation rows and sample selection without native execution."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

import nine_deck_baseline_v1 as ndb
import nine_deck_eval_driver_v1 as evaluation

REPO = Path(__file__).resolve().parents[3]


class EvalDriverTests(unittest.TestCase):
    def test_rows_score_learner_seat_and_flag_technical_terminals(self):
        decks = ndb.load_decks(REPO / "data" / "runtime_decks_v1.json")
        source = {"checkpoint": {"path": "C:/c.json", "sha256": "c" * 64}}
        config = ndb.panel_config("p3", source, source, decks, "D:/x", 1, own_decks=(4, 7))
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            pins = []
            for index, episode in enumerate(config["episodes"]):
                winner = "p0" if index % 3 else "p1"
                terminal = {"terminal_classification": "max_policy_steps" if index == 5 else "natural", "winner": winner}
                path = root / f"episode-{index:04d}.json"
                path.write_text(json.dumps({"episode": episode, "terminal": terminal}))
                pins.append({"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
            (root / "collection.json").write_text(json.dumps({"complete": True, "trajectories": pins}))
            rows = evaluation.rows_from(root / "collection.json", {"id": "j", "panel": "p3", "source": source})
        self.assertEqual(len(rows), 432)
        self.assertIsNone(rows[5]["score"])
        for index, row in enumerate(rows):
            if index == 5:
                continue
            winner_seat = 0 if index % 3 else 1
            self.assertEqual(row["score"], float(winner_seat == row["seat"]))
            self.assertIn(row["own"], (4, 7))

    def test_retained_sample_is_about_two_percent(self):
        ids = [episode["id"] for episode in ndb.panel_config("p1", {}, {}, ndb.load_decks(REPO / "data" / "runtime_decks_v1.json"),
                                                             "D:/x", 1)["episodes"]]
        kept = sum(evaluation.kept_case(case) for case in ids)
        self.assertTrue(20 <= kept <= 60, kept)


if __name__ == "__main__":
    unittest.main()
