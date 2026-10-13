"""Spy archive distillation analyzer: fixture tests (no panel, replay or model execution)."""
import json
import random
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools.spy_distill import analyze  # noqa: E402
from tools.spy_reset import analyze as pilot_analyze  # noqa: E402

pilot = analyze.pilot
SPY, DECKS, REPEATS = analyze.SPY, analyze.DECKS, analyze.REPEATS
DRAWS = 300


def panel(root: Path, ck: str, opp: str, outcome, sha: str = "a" * 64) -> dict:
    """Writes one panel's rows and labels. ``outcome(o, t, k, s)`` gives
    (score or None for a missing case, combo_resolved)."""
    name = f"{ck}-vs-{opp}"
    rows, labels = [], []
    for o in range(DECKS):
        for t in range(DECKS):
            for k in range(REPEATS):
                for s in (0, 1):
                    score, combo = outcome(o, t, k, s)
                    rows.append({"panel": name, "checkpoint_sha256": sha, "own": o, "other": t, "repeat": k,
                                 "seat": s, "seed": pilot.panel_seed(o, t, k), "score": score,
                                 "complete": score is not None})
                    if o == SPY and score is not None:
                        winner = None if score == 0.5 else (s if score == 1 else 1 - s)
                        win = winner == s
                        labels.append({"id": f"{name}/o{o}-t{t}-k{k}-s{s}", "kind": "label_replay", "focal": s,
                                       "winner": winner, "win": win, "combo_resolved": combo,
                                       "combo_won": combo and win,
                                       "labels": {"self_offered": combo or (t + k) % 3 == 0, "self_chosen": combo,
                                                  "self_target_resolved": combo, "dr_giant_offered": combo,
                                                  "dr_giant_chosen": combo}})
    directory = root / name
    directory.mkdir(parents=True)
    for file, items in (("rows.jsonl", rows), ("labels.jsonl", labels)):
        (directory / file).write_text("".join(json.dumps(r) + "\n" for r in items), encoding="utf-8")
    return {"checkpoint": ck, "opponent": opp, "checkpoint_sha256": sha,
            "rows": str(directory / "rows.jsonl"), "labels": str(directory / "labels.jsonl")}


def random_outcome(seed: int):
    rng = random.Random(seed)
    table = {}

    def outcome(o, t, k, s):
        key = (o, t, k, s)
        if key not in table:
            score = rng.choice((0, 0.5, 1, 1))
            table[key] = (score, o == SPY and score == 1 and rng.random() < 0.3)
        return table[key]
    return outcome


def constant(score, combo=lambda o, t, k, s: False, gone=()):
    return lambda o, t, k, s: (None if (o, t, k, s) in gone else score(o, t, k, s), combo(o, t, k, s))


class Fixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        patcher = mock.patch.object(analyze, "DRAWS", DRAWS)
        patcher.start()
        self.addCleanup(patcher.stop)


class ReproducesTheArchivePilotAnalyzer(Fixture):
    def test_o_minus_parent_points_and_intervals_equal_the_frozen_pilot_analyzer(self):
        entries = []
        for lin in ("r1", "r2"):
            for role in ("parent", "O", "P", "S"):
                for opp in ("T1", "H"):
                    entries.append(panel(self.root, f"{lin}-{role}", opp, random_outcome(len(entries))))
        for opp in ("T1", "H"):
            entries.append(panel(self.root, "T1", opp, random_outcome(7 + len(opp))))
        index = {"panels": entries}
        data = pilot_analyze.load(index)
        with mock.patch.object(pilot_analyze, "DRAWS", DRAWS):
            intervals = pilot_analyze.bootstrap(data)
        point = pilot_analyze.contrasts(data)
        recorded = {"contrasts": {"/".join(k): {"point": v, "ci95": intervals.get(k)} for k, v in point.items()}}
        self.assertTrue(any(v["ci95"][0] != v["ci95"][1] for k, v in recorded["contrasts"].items()
                            if "O-parent/combo_won" in k), "fixture exercises the J interval")
        self.assertEqual(analyze.validate(index, recorded), [])
        # A one-ulp change to a recorded bound is caught.
        key = "r2/H/O-parent/spy_score"
        recorded["contrasts"][key]["ci95"][1] += 1e-15
        bad = analyze.validate(index, recorded)
        self.assertEqual([b["contrast"] for b in bad], [key])

    def test_validation_refuses_missing_cases(self):
        entries = [panel(self.root, f"{lin}-{role}", opp, constant(lambda *_: 1, gone={(0, 0, 0, 0)}))
                   for lin in ("r1", "r2") for role in ("parent", "O") for opp in ("T1", "H")]
        self.assertEqual(analyze.validate({"panels": entries}, {"contrasts": {}}),
                         ["validation panels have missing cases"])


class Contrasts(Fixture):
    def build(self, cells: dict, missing: dict | None = None):
        """cells[(lineage, role, opp)] = (deck score function, combo function)."""
        missing = missing or {}
        entries = [panel(self.root, f"{lin}-{role}", opp,
                         constant(score, combo, missing.get((lin, role, opp), ())))
                   for (lin, role, opp), (score, combo) in cells.items()]
        return analyze.load({"panels": entries})

    def test_pooled_contrast_is_the_mean_of_the_opponents_values_on_the_same_draw(self):
        # Spy wins everything for D; the control wins Spy games only against T1.
        # D's J: every Spy game combos (T1) or half of them (H, by repeat parity).
        cells = {}
        for lin in ("r1", "r2"):
            cells[(lin, "D1U", "T1")] = (lambda o, t, k, s: 1, lambda o, t, k, s: True)
            cells[(lin, "D1U", "H")] = (lambda o, t, k, s: 1, lambda o, t, k, s: k % 2 == 0)
            cells[(lin, "parent", "T1")] = (lambda o, t, k, s: 1 if o == SPY else 0.5, lambda *_: False)
            cells[(lin, "parent", "H")] = (lambda o, t, k, s: 0 if o == SPY else 0.5, lambda *_: False)
        data = self.build(cells)
        todo = analyze.plan(data, analyze.SCREEN)
        self.assertEqual(todo, [(lin, "D1U-parent", "D1U", "parent") for lin in ("r1", "r2")])
        point = analyze.points(data, todo, analyze.METRICS)
        ci = analyze.bootstrap(data, todo, analyze.METRICS)
        for lin in ("r1", "r2"):
            for bound in analyze.BOUNDS:
                self.assertEqual(point[(lin, "D1U-parent", "spy_score", "T1", bound)], 0.0)
                self.assertEqual(point[(lin, "D1U-parent", "spy_score", "H", bound)], 1.0)
                self.assertEqual(point[(lin, "D1U-parent", "spy_score", "pooled", bound)], 0.5)
                self.assertEqual(point[(lin, "D1U-parent", "eight_deck", "pooled", bound)], 0.5)
                self.assertEqual(point[(lin, "D1U-parent", "J", "T1", bound)], 1.0)
                self.assertEqual(point[(lin, "D1U-parent", "J", "H", bound)], 0.5)
                self.assertEqual(point[(lin, "D1U-parent", "J", "pooled", bound)], 0.75)
                # Constant per repeat-parity: the H interval varies, T1's does not.
                self.assertEqual(ci[(lin, "D1U-parent", "J", "T1", bound)], [1.0, 1.0])
                low, high = ci[(lin, "D1U-parent", "J", "H", bound)]
                self.assertTrue(0.3 < low < 0.5 < high < 0.7)
                pooled = ci[(lin, "D1U-parent", "J", "pooled", bound)]
                self.assertEqual(pooled, [(1 + low) / 2, (1 + high) / 2])

    def test_missing_cases_take_their_bound_within_each_draw(self):
        cells = {(lin, role, opp): (lambda o, t, k, s: 0.5, lambda *_: False)
                 for lin in ("r1", "r2") for role in ("D2L", "O") for opp in ("T1", "H")}
        cells.update({(lin, "D2L", opp): (lambda o, t, k, s: 1 if o == SPY else 0.5, lambda o, t, k, s: o == SPY)
                      for lin in ("r1", "r2") for opp in ("T1", "H")})
        gone_d = {(SPY, t, 0, 0) for t in range(DECKS)}  # 9 of D's 216 Spy cases
        gone_c = {(0, 0, 0, 0)}                          # one of the control's Wildfire cases
        data = self.build(cells, {("r1", "D2L", "T1"): gone_d, ("r1", "O", "T1"): gone_c})
        todo = analyze.plan(data, analyze.SCREEN)
        point = analyze.points(data, todo, analyze.METRICS)
        ci = analyze.bootstrap(data, todo, analyze.METRICS)
        key = ("r1", "D2L-O")
        # Least favorable to D: D's missing Spy games score 0 with no J;
        # the control's missing game scores 1. Most favorable: the reverse.
        self.assertEqual(point[(*key, "J", "T1", "worst")], 207 / 216)
        self.assertEqual(point[(*key, "J", "T1", "best")], 1.0)
        self.assertEqual(point[(*key, "spy_score", "T1", "worst")], 207 / 216 - 0.5)
        self.assertEqual(point[(*key, "spy_score", "T1", "best")], 0.5)
        wildfire = (215 * 0.5 + 1) / 216
        self.assertAlmostEqual(point[(*key, "eight_deck", "T1", "worst")], -(wildfire - 0.5) / 8, places=15)
        self.assertAlmostEqual(point[(*key, "eight_deck", "T1", "best")], (0.5 - (215 * 0.5) / 216) / 8, places=15)
        # The missing cases are filled, not dropped, inside every draw.
        low, high = ci[(*key, "J", "T1", "worst")]
        self.assertTrue(low < 207 / 216 < high <= 1.0)
        self.assertEqual(ci[(*key, "J", "T1", "best")], [1.0, 1.0])
        self.assertEqual(ci[("r2", "D2L-O", "J", "T1", "worst")], [1.0, 1.0])
        self.assertEqual(analyze.missing(data[("r1-D2L", "T1")])["count"], 9)


class Readings(unittest.TestCase):
    @staticmethod
    def ci(j=(0.1, 0.3), spy=(-0.01, 0.05), eight=(-0.01, 0.01), arms=("U",), worst=None):
        out = {}
        for lin in ("r1", "r2"):
            for arm in arms:
                for name in (f"D1{arm}-parent", f"D2{arm}-O"):
                    for bound in analyze.BOUNDS:
                        values = {"J": j, "spy_score": spy, "eight_deck": eight}
                        if bound == "worst" and worst:
                            values.update(worst)
                        for metric, v in values.items():
                            out[(lin, name, metric, "pooled", bound)] = list(v)
        return out

    def test_retained_improvement_needs_j_and_both_guards_in_both_lineages(self):
        result = analyze.screen(self.ci(), None)
        self.assertEqual(result["U"]["transfer"], True)
        self.assertEqual(result["U"]["persistence"], True)
        self.assertEqual(result["U"]["reading"], "search-found line acquired and kept under ordinary training")
        self.assertEqual(set(result["U"]["guards"].values()), {"guards hold"})
        self.assertEqual(result["L"]["reading"], "not run: the arm passed the fit check in neither lineage")

    def test_a_retention_failure_blocks_a_positive_screen(self):
        result = analyze.screen(self.ci(eight=(-0.03, 0.01)), None)
        self.assertEqual(result["U"]["transfer"], False)
        self.assertEqual(set(result["U"]["guards"].values()), {"retention not shown (inconclusive)"})
        result = analyze.screen(self.ci(spy=(-0.08, -0.01)), None)
        self.assertEqual(set(result["U"]["guards"].values()), {"interference (detected harm)"})

    def test_bounds_that_disagree_read_inconclusive(self):
        result = analyze.screen(self.ci(worst={"J": (0.0, 0.2)}), None)
        self.assertEqual(result["U"]["transfer"], analyze.INCONCLUSIVE)
        self.assertEqual(result["U"]["reading"], analyze.INCONCLUSIVE)

    def test_no_transfer_reads_by_held_out_offer_probability(self):
        fit = {"learner_offers": {"U": {"pooled": None, "per_lineage": {
            "r1": {"offers": 150, "mean_deployed_self_probability": 0.01},
            "r2": {"offers": 140, "mean_deployed_self_probability": 0.02}}}}}
        result = analyze.screen(self.ci(j=(0.0, 0.1)), fit)
        self.assertEqual(result["U"]["reading"], "local fit without transfer")
        fit["learner_offers"]["U"]["per_lineage"]["r2"]["mean_deployed_self_probability"] = 0.2
        result = analyze.screen(self.ci(j=(0.0, 0.1)), fit)
        self.assertTrue(result["U"]["reading"].startswith("partial transfer"))

    def test_one_lineage_is_descriptive(self):
        ci = {k: v for k, v in self.ci().items() if k[0] == "r1"}
        result = analyze.screen(ci, None)
        self.assertEqual(result["U"]["reading"], "descriptive only: one lineage passed the fit check")
        self.assertNotIn("transfer", result["U"])


if __name__ == "__main__":
    unittest.main()
