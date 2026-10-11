"""Pure input-integrity regressions; no panel, replay or model execution."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools" / "spy_reset"))
import analyze
import pilot


class AnalysisInputs(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.panel = {"checkpoint": "r1-S", "opponent": "T1", "checkpoint_sha256": "a" * 64,
                      "rows": str(root / "rows.jsonl"), "labels": str(root / "labels.jsonl")}
        self.rows, self.labels = [], []
        for o in range(pilot.DECKS):
            for t in range(pilot.DECKS):
                for k in range(pilot.PANEL_REPEATS):
                    for s in (0, 1):
                        self.rows.append({"panel": "r1-S-vs-T1", "checkpoint_sha256": "a" * 64,
                                          "own": o, "other": t, "repeat": k, "seat": s,
                                          "seed": pilot.panel_seed(o, t, k), "score": 1., "complete": True})
                        if o == pilot.SPY:
                            self.labels.append({"id": f"r1-S-vs-T1/o{o}-t{t}-k{k}-s{s}",
                                                "kind": "label_replay", "focal": s, "winner": s,
                                                "win": True, "combo_resolved": False, "combo_won": False,
                                                "labels": {"self_chosen": False, "self_target_resolved": False,
                                                           "dr_giant_chosen": False, "dr_giant_offered": False}})

    def load(self, panels=None):
        for name, rows in (("rows", self.rows), ("labels", self.labels)):
            Path(self.panel[name]).write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
        return analyze.load({"panels": panels if panels is not None else [self.panel]})

    def test_complete_panel_retains_identity_and_labels(self):
        data = self.load()[("r1-S", "T1")]
        self.assertEqual(len(data), 1944)
        case = data[(pilot.SPY, 0, 0, 0)]
        self.assertEqual(case["checkpoint_sha256"], "a" * 64)
        self.assertEqual(case["labels"]["id"], "r1-S-vs-T1/o4-t0-k0-s0")
        self.assertEqual(analyze.summary(data)["spy_score"], 1.)

    def test_swapped_label_panel_refused(self):
        self.labels[0]["id"] = self.labels[0]["id"].replace("r1-S", "r1-P")
        with self.assertRaisesRegex(ValueError, "foreign label"):
            self.load()

    def test_wrong_row_panel_refused(self):
        self.rows[0]["panel"] = "r1-P-vs-T1"
        with self.assertRaisesRegex(ValueError, "panel, checkpoint or seed"):
            self.load()

    def test_wrong_checkpoint_refused(self):
        self.rows[0]["checkpoint_sha256"] = "b" * 64
        with self.assertRaisesRegex(ValueError, "panel, checkpoint or seed"):
            self.load()

    def test_wrong_seed_refused(self):
        self.rows[0]["seed"] += 1
        with self.assertRaisesRegex(ValueError, "panel, checkpoint or seed"):
            self.load()

    def test_duplicate_rows_cannot_hide_missing_case(self):
        self.rows[-1] = self.rows[0].copy()
        with self.assertRaisesRegex(ValueError, "duplicate or unknown case"):
            self.load()

    def test_unknown_case_refused(self):
        self.rows[0]["own"] = pilot.DECKS
        with self.assertRaisesRegex(ValueError, "duplicate or unknown case"):
            self.load()

    def test_missing_case_refused(self):
        self.rows.pop()
        with self.assertRaisesRegex(ValueError, "exact case roster"):
            self.load()

    def test_duplicate_panels_refused(self):
        with self.assertRaisesRegex(ValueError, "duplicate or unknown panel"):
            self.load([self.panel, self.panel])

    def test_duplicate_labels_refused(self):
        self.labels.append(self.labels[0].copy())
        with self.assertRaisesRegex(ValueError, "duplicate or foreign label"):
            self.load()

    def test_missing_completed_label_refused(self):
        self.labels.pop(0)
        with self.assertRaisesRegex(ValueError, "missing label"):
            self.load()

    def test_wrong_focal_refused(self):
        self.labels[0]["focal"] = 1
        with self.assertRaisesRegex(ValueError, "focal, winner or score"):
            self.load()

    def test_winner_conflicting_with_score_refused(self):
        self.labels[0]["winner"] = 1
        self.labels[0]["win"] = False
        with self.assertRaisesRegex(ValueError, "focal, winner or score"):
            self.load()

    def test_combo_conflicting_with_win_refused(self):
        self.labels[0]["combo_won"] = True
        with self.assertRaisesRegex(ValueError, "focal, winner or score"):
            self.load()

    def test_missing_label_for_technical_case_allowed(self):
        row = next(r for r in self.rows if r["own"] == pilot.SPY)
        row.update(complete=False, score=None)
        self.labels.pop(0)
        data = self.load()[("r1-S", "T1")]
        self.assertIsNone(data[(pilot.SPY, 0, 0, 0)]["labels"])
        self.assertEqual(analyze.missing(data)["count"], 1)

    def test_expected_checkpoint_hash_required(self):
        self.panel.pop("checkpoint_sha256")
        with self.assertRaisesRegex(ValueError, "checkpoint_sha256 is required"):
            self.load()


if __name__ == "__main__":
    unittest.main()
