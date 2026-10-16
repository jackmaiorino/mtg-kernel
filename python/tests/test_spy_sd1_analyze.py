"""SD1 panel analysis (tools/spy_distill/sd1_analyze.py): fixture tests. Nothing is played, searched
or trained; every panel, selection and run record here is synthetic."""
import contextlib
import copy
import functools
import hashlib
import io
import json
import math
import statistics
import sys
import tempfile
import unittest
from decimal import Decimal, getcontext
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools.spy_distill import sd1_analyze as S  # noqa: E402

# The modules sd1_analyze itself reads (its own imports, not package copies).
A, sd1, pilot, phase2 = S.analyze, S.sd1, S.pilot, S.phase2
DECKS, REPEATS, SPY = A.DECKS, A.REPEATS, A.SPY
SEEDS, OPPONENTS = S.SEEDS, A.OPPONENTS
DRAWS = 40


def sha(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def file_pin(path) -> dict:
    return {"path": str(path), "sha256": hashlib.sha256(Path(path).read_bytes()).hexdigest()}


def write(path: Path, value) -> dict:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")
    return file_pin(path)


def pattern(w: int, salt: int = 0, gone=()):
    """Each own deck, other deck and seat wins exactly ``w`` of its 12
    repeats (so every deck scores w/12); ``gone`` cases are missing."""
    def outcome(o, t, k, s):
        if (o, t, k, s) in gone:
            return None, False
        score = 1 if (o * 7 + t * 3 + k * 5 + s + salt) % 12 < w else 0
        return score, o == SPY and score == 1 and k % 3 == 0
    return outcome


def spy_label(s: int, score, combo: bool) -> dict:
    winner = None if score == 0.5 else (s if score == 1 else 1 - s)
    win = winner == s
    return {"kind": "label_replay", "focal": s, "winner": winner, "win": win, "combo_resolved": combo,
            "combo_won": bool(combo and win),
            "labels": {"self_offered": combo, "self_chosen": combo, "dr_giant_offered": combo,
                       "dr_giant_chosen": combo}}


def cases(outcome) -> dict:
    """A panel as analyze.load returns it."""
    out = {}
    for o in range(DECKS):
        for t in range(DECKS):
            for k in range(REPEATS):
                for s in (0, 1):
                    score, combo = outcome(o, t, k, s)
                    case = {"complete": score is not None, "score": score}
                    if o == SPY:
                        case["labels"] = None if score is None else spy_label(s, score, combo)
                    out[(o, t, k, s)] = case
    return out


def write_panel(root: Path, ck: str, opp: str, outcome, checkpoint_sha256: str,
                namespace: str = pilot.SD1_PANEL_NS) -> dict:
    name = f"{ck}-vs-{opp}"
    rows, labels = [], []
    for (o, t, k, s), case in cases(outcome).items():
        rows.append({"panel": name, "checkpoint_sha256": checkpoint_sha256, "own": o, "other": t, "repeat": k,
                     "seat": s, "seed": pilot.panel_seed(o, t, k, namespace), "score": case["score"],
                     "complete": case["complete"]})
        if case.get("labels"):
            labels.append({"id": f"{name}/o{o}-t{t}-k{k}-s{s}", **case["labels"]})
    directory = root / name
    directory.mkdir(parents=True)
    for file, items in (("rows.jsonl", rows), ("labels.jsonl", labels)):
        (directory / file).write_text("".join(json.dumps(r) + "\n" for r in items), encoding="utf-8")
    return {"checkpoint": ck, "opponent": opp, "checkpoint_sha256": checkpoint_sha256,
            "rows": str(directory / "rows.jsonl"), "labels": str(directory / "labels.jsonl")}


def as_pins(panel: dict) -> dict:
    """A panel index entry (rows and labels by path, as the screen's index
    gives them) as SD1's spec gives it: rows and labels as pins."""
    return {**panel, "rows": file_pin(panel["rows"]), "labels": file_pin(panel["labels"])}


def gain(k, opp):
    return 10 + k % 2


def outcomes(lineage: str, d2=gain, o=lambda k, opp: 6, d1=lambda k, opp: 9, parent=lambda opp: 6,
             gone=None) -> dict:
    """(endpoint, opponent) -> outcome, each endpoint winning w/12 of every
    deck's games; D2-k and O-k share their pattern's salt."""
    gone = gone or {}
    out = {}
    for j, opp in enumerate(OPPONENTS):
        out[(f"{lineage}-parent", opp)] = pattern(parent(opp), 11 + j, gone.get((f"{lineage}-parent", opp), ()))
        for k in SEEDS:
            for role, w in (("D1", d1(k, opp)), ("D2", d2(k, opp)), ("O", o(k, opp))):
                ck = f"{lineage}-{role}-{k}"
                out[(ck, opp)] = pattern(w, k + j, gone.get((ck, opp), ()))
    return out


def loaded(*lineage_outcomes) -> dict:
    return {key: cases(outcome) for each in lineage_outcomes for key, outcome in each.items()}


def model_source(lineage: str, checkpoint: dict) -> dict:
    """A model source: the lineage's import and feature transfer, and a
    checkpoint pin."""
    return {"play_import": {"path": f"/srv/computehost/imports/{lineage}.json", "sha256": sha(f"import-{lineage}")},
            "feature_transfer": {"expected_feature_contract_digest": sha("contract"),
                                 "expected_feature_encoding_digest": sha("encoding")},
            "checkpoint": checkpoint}


def parent_source(lineage: str) -> dict:
    return model_source(lineage, {"path": f"/srv/computehost/{lineage}/parent.json",
                                  "sha256": S.PARENT_CHECKPOINT_SHA256[lineage]})


@functools.lru_cache(maxsize=None)
def units(lineage: str, k: int) -> str:
    """A run plan's iterations, as JSON text: seed k's episode id and seed in
    every unit of the pilot's schedule (O's own for 0, sd1-LIN-repK's
    otherwise)."""
    plan = pilot.schedule(lineage)["updates"]
    return json.dumps([{"episodes": [{"episode": dict(zip(("id", "seed"), sd1.unit_id_seed(lineage, k, e))),
                                      "opponent": {"kind": "current"}} for e in update]} for update in plan])


def write_run(directory: Path, lineage: str, k: int, initial: dict, final_sha256: str) -> dict:
    """A run's plan (run.json) on seed k's units from ``initial``, and its
    completion, whose pin is returned."""
    write(directory / "run.json", {"schema": phase2.RUN_SCHEMA, "git_commit": "0" * 40,
                                   "config": {"schema": phase2.RUN_SCHEMA, "initial_source": initial,
                                              "iterations": json.loads(units(lineage, k))}})
    return write(directory / "completion.json", {
        "schema": phase2.RUN_SCHEMA, "complete": True, "completed_iterations": pilot.UPDATES,
        "planned_iterations": pilot.UPDATES,
        "source": {"checkpoint": {"path": f"{directory.name}/final.json", "sha256": final_sha256}}})


def custody(root: Path, lineage: str) -> tuple:
    """A lineage's eight selections with their D1 sources, and sixteen runs
    (each run.json beside its completion), and each endpoint's checkpoint
    SHA-256."""
    shas = {ck: S.PARENT_CHECKPOINT_SHA256[lineage] if ck.endswith("-parent") else sha(ck)
            for ck in S.endpoint_ids(lineage)}
    sources, select = {}, []
    for k in SEEDS:
        checkpoint = {"path": f"/srv/computehost/{lineage}/D1-{k}/checkpoint.json",
                      "sha256": shas[f"{lineage}-D1-{k}"]}
        sources[k] = model_source(lineage, checkpoint)
        source_pin = write(root / lineage / f"D1-{k}" / "source.json", sources[k])
        select.append(write(root / lineage / f"select-{k}.json", {
            "schema": sd1.SELECT_SCHEMA, "lineage": lineage, "seed": k, "reading": "passes",
            "d1_checkpoint": {"step": 64, "checkpoint": checkpoint, "source": source_pin,
                              "state_sha256": sha(f"state-{k}")}}))
    runs = {f"D2-{k}": write_run(root / lineage / f"D2-{k}", lineage, k, sources[k], shas[f"{lineage}-D2-{k}"])
            for k in SEEDS}
    runs.update({f"O-{k}": write_run(root / lineage / f"O-{k}", lineage, k, parent_source(lineage),
                                     shas[f"{lineage}-O-{k}"]) for k in SEEDS if k})
    o_s0 = write_run(root / lineage / "O-s0", lineage, 0, parent_source(lineage), shas[f"{lineage}-O-0"])
    return {"select": select, "o_s0": o_s0, "phase2": runs}, shas


def screen_pins(root: Path, passed: bool = True) -> dict:
    index = write(root / "screen" / "index.json", {"panels": []})
    result = write(root / "screen" / "RESULT.json", {"contrasts": {}})
    record = write(root / "screen" / "X4.json", {"schema": S.X4_SCHEMA, "screen_index": index,
                                                 "screen_result": result, "seed": A.BOOT_SEED, "compared": 0,
                                                 "differing": [], "pass": passed})
    return {"screen_index": index, "screen_result": result, "x4": record}


def entries(root: Path, lineage: str, shas: dict) -> list:
    """Panel spec entries whose rows a patched loader supplies: each pins
    one placeholder file as its rows and labels."""
    placeholder = root / "placeholder.jsonl"
    placeholder.write_text("{}\n", encoding="utf-8")
    return [{"checkpoint": ck, "opponent": opp, "checkpoint_sha256": shas[ck], "rows": file_pin(placeholder),
             "labels": file_pin(placeholder)} for ck in S.endpoint_ids(lineage) for opp in OPPONENTS]


def loadable(panels: list) -> list:
    """Spec panels as the screen's analyzer loads them: rows and labels by
    path."""
    return [{**p, "rows": p["rows"]["path"], "labels": p["labels"]["path"]} for p in panels]


class Temp(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        patcher = mock.patch.object(A, "DRAWS", DRAWS)
        patcher.start()
        self.addCleanup(patcher.stop)


# ------------------------------------------------------------- t interval


def decimal_atan(x: Decimal) -> Decimal:
    halvings = 0
    while abs(x) > Decimal("0.05"):
        x = x / (1 + (1 + x * x).sqrt())
        halvings += 1
    total, term, k = Decimal(0), x, 0
    while abs(term) > Decimal(10) ** -70:
        total += term / (2 * k + 1) * (1 if k % 2 == 0 else -1)
        term *= x * x
        k += 1
    return total * 2 ** halvings


def two_sided_t7(t: Decimal) -> Decimal:
    """P(|T| <= t) for Student's t with 7 degrees of freedom (closed form)."""
    pi = 4 * (4 * decimal_atan(Decimal(1) / 5) - decimal_atan(Decimal(1) / 239))
    x = t / Decimal(7).sqrt()
    c = 1 / (1 + x * x).sqrt()
    return 2 / pi * (decimal_atan(x) + x * c * (c + Decimal(2) / 3 * c ** 3 + Decimal(8) / 15 * c ** 5))


class TInterval(unittest.TestCase):
    def test_the_quantile_is_the_binary64_nearest_the_7_df_95_percent_point(self):
        getcontext().prec = 60
        t = Decimal(S.T_975_7)
        half_ulp = Decimal(math.ulp(S.T_975_7)) / 2
        self.assertLess(two_sided_t7(t - half_ulp), Decimal("0.95"))
        self.assertGreater(two_sided_t7(t + half_ulp), Decimal("0.95"))

    def test_mean_plus_or_minus_the_quantile_times_the_standard_error(self):
        values = [0.013, -0.002, 0.021, 0.008, 0.0, 0.017, -0.006, 0.011]
        low, high = S.t_interval(values)
        half = S.T_975_7 * statistics.stdev(values) / math.sqrt(8)
        self.assertAlmostEqual(low, statistics.mean(values) - half, places=15)
        self.assertAlmostEqual(high, statistics.mean(values) + half, places=15)
        self.assertEqual(S.t_interval([0.25] * 8), [0.25, 0.25])
        with self.assertRaisesRegex(ValueError, "eight pairs"):
            S.t_interval(values[:7])


# ------------------------------------------------------------------ values


class Values(Temp):
    def setUp(self):
        super().setUp()
        gone = {("r1-D2-0", "T1"): {(0, 1, 2, 0), (5, 5, 5, 1)}, ("r1-parent", "H"): {(SPY, 3, 3, 1)}}
        self.data = loaded(outcomes("r1", gone=gone))
        self.table = S.draw_table(self.data, S.BOOT_SEED, DRAWS)
        self.result = S.lineage_result(self.data, self.table, "r1")

    def value(self, ck: str, opp: str, fill: float, metric: str = "nine_deck") -> float:
        return S.panel_values(self.data[(ck, opp)], fill)[metric]

    def test_pairs_follow_each_contrast_and_fill(self):
        c = self.result["contrasts"]
        v = self.value
        # d_0 against T1: D2-0's two missing cases at 0 (least favorable) or 1, O-0 complete.
        self.assertEqual(c["d"]["nine_deck"]["T1"]["least"]["pairs"][0],
                         v("r1-D2-0", "T1", 0.0) - v("r1-O-0", "T1", 1.0))
        self.assertEqual(c["d"]["nine_deck"]["T1"]["most"]["pairs"][0],
                         v("r1-D2-0", "T1", 1.0) - v("r1-O-0", "T1", 0.0))
        self.assertLess(c["d"]["nine_deck"]["T1"]["least"]["pairs"][0],
                        c["d"]["nine_deck"]["T1"]["most"]["pairs"][0])
        # e_0 against H: the parent's missing Spy case at 1 (least favorable to D1) or 0.
        self.assertEqual(c["e"]["spy_score"]["H"]["least"]["pairs"][0],
                         v("r1-D1-0", "H", 0.0, "spy_score") - v("r1-parent", "H", 1.0, "spy_score"))
        self.assertEqual(c["e"]["J"]["H"]["most"]["pairs"][0],
                         v("r1-D1-0", "H", 1.0, "J") - v("r1-parent", "H", 0.0, "J"))
        # f_0: d's least favorable fill minus e's most favorable one, and the reverse.
        for fill, other in (("least", "most"), ("most", "least")):
            self.assertEqual(c["f"]["nine_deck"]["H"][fill]["pairs"],
                             [d - e for d, e in zip(c["d"]["nine_deck"]["H"][fill]["pairs"],
                                                    c["e"]["nine_deck"]["H"][other]["pairs"])])
        self.assertEqual(c["D2-D1"]["deck_3"]["T1"]["least"]["pairs"][5],
                         v("r1-D2-5", "T1", 0.0, "deck_3") - v("r1-D1-5", "T1", 1.0, "deck_3"))
        # Every deck wins w/12 of its games when nothing is missing.
        self.assertEqual(c["d"]["eight_deck"]["H"]["least"]["pairs"], [gain(k, "H") / 12 - 6 / 12 for k in SEEDS])

    def test_pooled_is_the_mean_of_the_two_opponents_values(self):
        c = self.result["contrasts"]
        v = self.value
        for k in SEEDS:
            self.assertEqual(c["d"]["nine_deck"]["pooled"]["least"]["pairs"][k],
                             (v(f"r1-D2-{k}", "T1", 0.0) + v(f"r1-D2-{k}", "H", 0.0)) / 2
                             - (v(f"r1-O-{k}", "T1", 1.0) + v(f"r1-O-{k}", "H", 1.0)) / 2)

    def test_mean_and_t_interval_are_over_the_eight_pairs(self):
        entry = self.result["contrasts"]["d"]["nine_deck"]["pooled"]["least"]
        total = 0.0
        for p in entry["pairs"]:
            total += p
        self.assertEqual(entry["mean"], total / 8)
        self.assertEqual(entry["t"], S.t_interval(entry["pairs"]))
        self.assertEqual(entry["label"], S.fill_label(entry["t"], entry["bootstrap"]))


# --------------------------------------------------------------- bootstrap


class Bootstrap(Temp):
    def test_draw_values_are_the_screens_draw_statistics(self):
        import numpy as np
        data = loaded({("r1-D2-0", "T1"): pattern(7, 3, {(SPY, 2, 4, 1)})})
        rng = np.random.default_rng(5)
        for fill in (0.0, 1.0):
            arrs = A.arrays(data[("r1-D2-0", "T1")], fill)
            for _ in range(5):
                idx = rng.integers(0, REPEATS, size=(DECKS, DECKS, REPEATS))
                score_idx, spy_idx = idx[:, :, :, None].repeat(2, axis=3), idx[SPY][:, :, None].repeat(2, axis=2)
                mine, screen = S.draw_values(*arrs, score_idx, spy_idx), A.draw_stats(*arrs, score_idx, spy_idx)
                for m in ("nine_deck", "eight_deck", "spy_score", "J", "combo_resolved"):
                    self.assertEqual(mine[m], screen[m])
                self.assertEqual(mine[f"deck_{SPY}"], screen["spy_score"])
                self.assertEqual(mine["deck_7"], screen["cawgates"])

    def test_one_pair_reproduces_the_screens_bootstrap_with_the_same_seed(self):
        out = outcomes("r1", gone={("r1-O-0", "H"): {(1, 2, 3, 0)}, ("r1-D2-0", "T1"): {(SPY, 0, 0, 0)}})
        data = loaded({key: f for key, f in out.items() if key[0] in ("r1-D2-0", "r1-O-0")})
        metrics = ("nine_deck", "eight_deck", "spy_score", "J", "combo_resolved")
        screen = A.bootstrap(data, [("r1", "pair", "D2-0", "O-0")], metrics, S.BOOT_SEED)
        table = S.draw_table(data, S.BOOT_SEED, DRAWS)
        for bound, fill in (("worst", "least"), ("best", "most")):
            for m in metrics:
                for opp in (*OPPONENTS, "pooled"):
                    column = S.pair_value(lambda ck, o, value: table[(ck, o, value)], "r1", "d", 0, m, opp, fill)
                    self.assertEqual(S.quantiles(column), screen[("r1", "pair", m, opp, bound)])

    def test_draws_are_shared_by_every_panel_of_both_lineages(self):
        r1 = loaded(outcomes("r1"))
        both = {**r1, **loaded(outcomes("r2", d2=lambda k, opp: 5))}
        alone, joint = S.draw_table(r1, S.BOOT_SEED, DRAWS), S.draw_table(both, S.BOOT_SEED, DRAWS)
        for key, columns in alone.items():
            for m, values in columns.items():
                self.assertTrue((values == joint[key][m]).all(), (key, m))

    def test_interval_is_of_the_eight_pair_mean_on_each_draw(self):
        import numpy as np
        data = loaded(outcomes("r1", gone={("r1-O-3", "T1"): {(2, 2, 2, 1)}}))
        result = S.lineage_result(data, S.draw_table(data, S.BOOT_SEED, DRAWS), "r1")
        arrays = {key: {v: A.arrays(cases, v) for v in (0.0, 1.0)} for key, cases in data.items()}
        rng = np.random.default_rng(S.BOOT_SEED)
        means = []
        for _ in range(DRAWS):
            idx = rng.integers(0, REPEATS, size=(DECKS, DECKS, REPEATS))
            score_idx, spy_idx = idx[:, :, :, None].repeat(2, axis=3), idx[SPY][:, :, None].repeat(2, axis=2)

            def nine(ck, value):
                return A.draw_stats(*arrays[(ck, "T1")][value], score_idx, spy_idx)["nine_deck"]
            total = 0.0
            for k in SEEDS:
                total += nine(f"r1-D2-{k}", 0.0) - nine(f"r1-O-{k}", 1.0)
            means.append(total / 8)
        means = sorted(means)
        self.assertEqual(result["contrasts"]["d"]["nine_deck"]["T1"]["least"]["bootstrap"],
                         [means[int(0.025 * DRAWS)], means[int(0.975 * DRAWS) - 1]])


# ---------------------------------------------------------------- readings


def entry(least, most) -> dict:
    """least and most: (t, bootstrap) intervals."""
    return {f: {"t": t, "bootstrap": b, "label": S.fill_label(t, b)} for f, (t, b) in (("least", least),
                                                                                     ("most", most))}


UP, DOWN, ZERO = [0.01, 0.03], [-0.03, -0.01], [-0.01, 0.01]


class Readings(unittest.TestCase):
    def test_a_fill_reads_improvement_or_harm_only_when_both_intervals_agree(self):
        self.assertEqual(S.fill_label(UP, UP), "improvement")
        self.assertEqual(S.fill_label(DOWN, DOWN), "harm")
        self.assertEqual(S.fill_label(UP, ZERO), "no difference")
        self.assertEqual(S.fill_label(ZERO, DOWN), "no difference")
        self.assertEqual(S.fill_label([0.0, 0.02], UP), "no difference")

    def test_the_reading_needs_both_fills(self):
        self.assertEqual(S.reading(entry((UP, UP), (UP, UP)), "d"), (S.IMPROVEMENT, []))
        self.assertEqual(S.reading(entry((DOWN, DOWN), (DOWN, DOWN)), "e"), (S.HARM, []))
        self.assertEqual(S.reading(entry((ZERO, UP), (UP, UP)), "d"), (S.INCONCLUSIVE, []))
        self.assertEqual(S.reading(entry((DOWN, DOWN), (ZERO, ZERO)), "d")[0], "inconclusive: missing cases decide")
        read, notes = S.reading(entry((ZERO, ZERO), (ZERO, ZERO)), "d")
        self.assertEqual((read, notes), (S.NO_DIFFERENCE, ["not evidence that strength held"]))

    def test_no_difference_notes_an_interval_that_alone_excluded_zero(self):
        read, notes = S.reading(entry((UP, ZERO), (ZERO, DOWN)), "e")
        self.assertEqual(read, "no difference established")
        self.assertEqual(notes, ["not evidence that strength held",
                                 "the t interval alone excluded 0 (above) under the fill least favorable to D",
                                 "the bootstrap interval alone excluded 0 (below) under the fill most favorable to D"])

    def test_the_change_across_phase_two_has_its_own_words(self):
        self.assertEqual(S.reading(entry((UP, UP), (UP, UP)), "f")[0], "effect larger after phase 2 established")
        self.assertEqual(S.reading(entry((DOWN, DOWN), (DOWN, DOWN)), "f")[0],
                         "effect smaller after phase 2 established")
        self.assertEqual(S.reading(entry((ZERO, ZERO), (ZERO, ZERO)), "f"),
                         ("change across phase 2 not established", ["not evidence that the effect persisted"]))
        self.assertEqual(S.reading(entry((UP, UP), (ZERO, UP)), "f")[0], S.INCONCLUSIVE)
        self.assertEqual(S.only_at(S.IMPROVEMENT, S.NO_DIFFERENCE), "improvement established only at D1")
        self.assertEqual(S.only_at(S.INCONCLUSIVE, S.HARM), "harm established only at D2")
        self.assertIsNone(S.only_at(S.IMPROVEMENT, S.IMPROVEMENT))
        self.assertIsNone(S.only_at(S.NO_DIFFERENCE, S.INCONCLUSIVE))

    def test_across_lineages(self):
        one = ("(lineage-dependent: established in one lineage, which is not an established difference between "
               "lineages)")
        self.assertEqual(S.across_lineages({"r1": S.IMPROVEMENT, "r2": S.IMPROVEMENT}, {}, "d"),
                         "improvement established in both lineages")
        self.assertEqual(S.across_lineages({"r1": S.NO_DIFFERENCE, "r2": S.HARM}, {}, "e"),
                         f"harm established in r2 only {one}")
        self.assertEqual(S.across_lineages({"r1": S.IMPROVEMENT, "r2": S.HARM}, {}, "d"),
                         "in opposite directions (lineage-dependent)")
        self.assertEqual(S.across_lineages({"r1": S.NO_DIFFERENCE, "r2": S.NO_DIFFERENCE}, {}, "d"), "in neither")
        self.assertEqual(S.across_lineages({"r1": S.IMPROVEMENT, "r2": S.INCONCLUSIVE}, {}, "d"),
                         f"improvement established in r1 only {one}; inconclusive in r2 (missing cases decide)")
        self.assertEqual(S.across_lineages({"r2": S.IMPROVEMENT}, {"r1": "fit"}, "d"),
                         f"improvement established in r2 only {one}; not run in r1 (fit)")
        self.assertEqual(S.across_lineages({}, {"r1": "fit", "r2": "engineering"}, "d"),
                         "not run in r1 (fit); not run in r2 (engineering)")
        larger = "effect larger after phase 2 established"
        self.assertEqual(S.across_lineages({"r1": larger, "r2": larger}, {}, "f"), f"{larger} in both lineages")

    def test_across_opponents(self):
        self.assertIsNone(S.across_opponents({"T1": S.IMPROVEMENT, "H": S.IMPROVEMENT}, "d"))
        self.assertIsNone(S.across_opponents({"T1": S.NO_DIFFERENCE, "H": S.INCONCLUSIVE}, "d"))
        self.assertEqual(S.across_opponents({"T1": S.IMPROVEMENT, "H": S.NO_DIFFERENCE}, "d"),
                         "improvement established against T1 only (not an established difference between opponents)")
        self.assertEqual(S.across_opponents({"T1": S.HARM, "H": S.IMPROVEMENT}, "e"),
                         "in opposite directions against T1 and H (opponent-dependent)")

    def headline(self, pooled: str, h_mean: float, eight: str) -> dict:
        contrasts = {"d": {"nine_deck": {"pooled": {"reading": pooled}, "H": {"least": {"mean": h_mean}}},
                           "eight_deck": {"pooled": {"least": {"label": eight}}}}}
        return S.headline(contrasts, "d")

    def test_the_primary_needs_the_pooled_reading_hs_mean_and_no_eight_deck_harm(self):
        ok = self.headline(S.IMPROVEMENT, 0.004, "no difference")
        self.assertEqual(ok["reading"], S.IMPROVEMENT)
        self.assertTrue(all(ok["conditions"].values()))
        self.assertEqual(ok["explanations"], ["improvement"])
        not_h = self.headline(S.IMPROVEMENT, 0.0, "improvement")
        self.assertEqual(not_h["reading"], "improvement established pooled over T1 and H, not shown against H, "
                                           "the opponent the teacher did not model")
        self.assertFalse(not_h["conditions"]["h_nine_deck_mean_above_0"])
        harm = self.headline(S.IMPROVEMENT, 0.01, "harm")
        self.assertEqual(harm["reading"], S.IMPROVEMENT)
        self.assertFalse(harm["conditions"]["pooled_eight_deck_not_harm"])
        none = self.headline(S.NO_DIFFERENCE, 0.01, "no difference")
        self.assertEqual((none["reading"], none["explanations"]), (S.NO_DIFFERENCE, ["no_difference_or_harm"]))
        self.assertFalse(none["conditions"]["pooled_nine_deck_improvement_established"])
        self.assertEqual(self.headline(S.HARM, -0.01, "harm")["explanations"], ["no_difference_or_harm", "harm"])


# ----------------------------------------------------------------- custody


class Custody(Temp):
    def test_each_endpoint_binds_to_its_checkpoint(self):
        entry, shas = custody(self.root, "r1")
        endpoints, read = S.expected_endpoints("r1", entry)
        self.assertEqual(endpoints, shas)
        self.assertEqual(len(shas), 25)
        self.assertEqual(shas["r1-parent"], S.PARENT_CHECKPOINT_SHA256["r1"])
        self.assertEqual(set(read), set(shas) - {"r1-parent"})
        self.assertEqual(read["r1-D1-3"], {"selection": entry["select"][3],
                                           "source": file_pin(self.root / "r1" / "D1-3" / "source.json")})
        for name, pinned in [("D2-5", entry["phase2"]["D2-5"]), ("O-0", entry["o_s0"])]:
            self.assertEqual(read[f"r1-{name}"], {"completion": pinned, "run": file_pin(
                Path(pinned["path"]).parent / "run.json")})
        panels = entries(self.root, "r1", shas)
        self.assertEqual(S.lineage_panels("r1", {**entry, "panels": panels}),
                         (loadable(panels), {"endpoints": read, "panels": {
                             f"{p['checkpoint']}/{p['opponent']}": {"rows": p["rows"], "labels": p["labels"]}
                             for p in panels}}))

    def refuses(self, message: str, entry: dict) -> None:
        with self.assertRaisesRegex(ValueError, message):
            S.lineage_panels("r1", entry)

    def test_refusals(self):
        entry, shas = custody(self.root, "r1")
        panels = entries(self.root, "r1", shas)
        base = {**entry, "panels": panels}
        self.refuses("every endpoint's panel against each opponent", {**base, "panels": panels[1:]})
        wrong = copy.deepcopy(panels)
        wrong[7]["checkpoint_sha256"] = sha("another")
        self.refuses(f"{wrong[7]['checkpoint']}-vs-{wrong[7]['opponent']}: not the endpoint's checkpoint",
                     {**base, "panels": wrong})
        self.refuses("every seed's selection is needed", {**base, "select": entry["select"][1:]})
        self.refuses("selected twice", {**base, "select": entry["select"] + entry["select"][:1]})
        no_d1 = json.loads(Path(entry["select"][3]["path"]).read_text(encoding="utf-8"))
        no_d1.update(reading="collapse before the required fit")
        no_d1.pop("d1_checkpoint")
        self.refuses("seed 3 has no D1", {**base, "select": entry["select"][:3] + [
            write(self.root / "no-d1.json", no_d1)] + entry["select"][4:]})
        self.refuses("phase 2 is D2-0 to D2-7 and O-1 to O-7",
                     {**base, "phase2": {**entry["phase2"], "O-0": entry["o_s0"]}})
        partial = json.loads(Path(entry["phase2"]["D2-4"]["path"]).read_text(encoding="utf-8"))
        partial.update(complete=False, completed_iterations=100)
        self.refuses("r1 D2-4: not a complete 162-update run",
                     {**base, "phase2": {**entry["phase2"], "D2-4": write(self.root / "partial.json", partial)}})
        changed = dict(entry["o_s0"], sha256=sha("changed"))
        self.refuses("r1 O-s0: .* is missing or differs", {**base, "o_s0": changed})
        # Two runs whose final checkpoints are one file.
        twin = json.loads(Path(entry["phase2"]["D2-6"]["path"]).read_text(encoding="utf-8"))
        twin["source"]["checkpoint"]["sha256"] = shas["r1-O-6"]
        (self.root / "twin").mkdir()
        (self.root / "twin" / "run.json").write_bytes((self.root / "r1" / "D2-6" / "run.json").read_bytes())
        self.refuses("two endpoints name the same checkpoint",
                     {**base, "phase2": {**entry["phase2"], "D2-6": write(self.root / "twin" / "completion.json",
                                                                          twin)}})

    def test_a_run_is_the_run_its_name_says(self):
        entry, shas = custody(self.root, "r1")
        base = {**entry, "panels": entries(self.root, "r1", shas)}
        runs = entry["phase2"]

        def moved(name: str, completion: dict, plan_from: str | None) -> dict:
            """``completion``'s bytes in a new directory beside another run's
            run.json (or none)."""
            directory = self.root / "moved" / name
            directory.mkdir(parents=True)
            (directory / "completion.json").write_bytes(Path(completion["path"]).read_bytes())
            if plan_from is not None:
                (directory / "run.json").write_bytes((self.root / "r1" / plan_from / "run.json").read_bytes())
            return file_pin(directory / "completion.json")

        def plan(name: str, value) -> dict:
            directory = self.root / "plans" / name
            write(directory / "run.json", value)
            return write(directory / "completion.json", json.loads(Path(runs["D2-5"]["path"]).read_text(
                encoding="utf-8")))

        # Its plan is read beside its completion.
        self.refuses("r1 D2-2: .*run.json is missing", {**base, "phase2": {**runs, "D2-2": moved("D2-2", runs["D2-2"],
                                                                                                 None)}})
        # Its units carry its own seed's episode ids and seeds.
        self.refuses("r1 D2-3: not seed 3's run: update 0 unit 0: its episode id and seed are not seed 3's",
                     {**base, "phase2": {**runs, "D2-3": moved("D2-3", runs["D2-3"], "D2-4")}})
        self.refuses("r1 O-s0: not seed 0's run",
                     {**base, "o_s0": moved("O-s0", entry["o_s0"], "O-1")})
        self.refuses("r1 O-1: not seed 1's run", {**base, "phase2": {**runs, "O-1": moved("O-1", runs["O-1"], "D2-0")}})
        recorded = json.loads((self.root / "r1" / "D2-5" / "run.json").read_text(encoding="utf-8"))
        late = copy.deepcopy(recorded)
        late["config"]["iterations"][161]["episodes"][3]["episode"]["seed"] += 1
        self.refuses("r1 D2-5: not seed 5's run: update 161 unit 3: its episode id and seed are not seed 5's",
                     {**base, "phase2": {**runs, "D2-5": plan("late", late)}})
        short = copy.deepcopy(recorded)
        short["config"]["iterations"].pop()
        self.refuses("r1 D2-5: not seed 5's run: it does not hold the schedule's 162 updates",
                     {**base, "phase2": {**runs, "D2-5": plan("short", short)}})
        for name, value in (("list", [recorded]), ("other", {**recorded, "schema": "another"}),
                            ("bare", {"schema": phase2.RUN_SCHEMA})):
            self.refuses("r1 D2-5: the run.json beside it is not a run's plan",
                         {**base, "phase2": {**runs, "D2-5": plan(name, value)}})
        # D2-k starts from D1-k's source, verbatim; O-k from the parent.
        for initial in (parent_source("r1"), json.loads((self.root / "r1" / "D1-4" / "source.json").read_text(
                encoding="utf-8")), {**recorded["config"]["initial_source"], "checkpoint": {
                    **recorded["config"]["initial_source"]["checkpoint"], "path": "Q:/elsewhere/checkpoint.json"}}):
            other = copy.deepcopy(recorded)
            other["config"]["initial_source"] = initial
            self.refuses("r1 D2-5: its initial source is not D1-5's source verbatim",
                         {**base, "phase2": {**runs, "D2-5": plan("initial", other)}})
        from_d1 = json.loads((self.root / "r1" / "O-2" / "run.json").read_text(encoding="utf-8"))
        from_d1["config"]["initial_source"] = json.loads((self.root / "r1" / "D1-2" / "source.json").read_text(
            encoding="utf-8"))
        directory = self.root / "from-d1"
        write(directory / "run.json", from_d1)
        o_2 = write(directory / "completion.json", json.loads(Path(runs["O-2"]["path"]).read_text(encoding="utf-8")))
        self.refuses("r1 O-2: its initial source is not the parent", {**base, "phase2": {**runs, "O-2": o_2}})

    def test_d1_sources_and_panel_files_are_pinned(self):
        entry, shas = custody(self.root, "r1")
        panels = entries(self.root, "r1", shas)
        base = {**entry, "panels": panels}
        source = self.root / "r1" / "D1-3" / "source.json"
        source.write_text(source.read_text(encoding="utf-8") + " ", encoding="utf-8")
        self.refuses("r1 D1-3 source: .* is missing or differs", base)
        entry, shas = custody(self.root / "again", "r1")
        selection = json.loads(Path(entry["select"][2]["path"]).read_text(encoding="utf-8"))
        selection["d1_checkpoint"]["source"] = write(self.root / "elsewhere.json", model_source(
            "r1", {"path": "/srv/computehost/r1/D1-6/checkpoint.json", "sha256": shas["r1-D1-6"]}))
        self.refuses("r1 D1-2: the source's checkpoint is not the selected one",
                     {**entry, "panels": panels, "select": entry["select"][:2] + [
                         write(self.root / "select-2.json", selection)] + entry["select"][3:]})
        panels = entries(self.root / "again", "r1", shas)
        base = {**entry, "panels": panels}
        Path(panels[0]["rows"]["path"]).write_text("{}\n{}\n", encoding="utf-8")
        self.refuses("r1-parent-vs-T1 rows: .* is missing or differs", base)
        panels = entries(self.root / "again", "r1", shas)
        bare = copy.deepcopy(panels)
        bare[5]["labels"] = bare[5]["labels"]["path"]
        self.refuses(f"{bare[5]['checkpoint']}-vs-{bare[5]['opponent']}: its rows and labels are pins",
                     {**entry, "panels": bare})


# --------------------------------------------------------------------- X4


class X4(Temp):
    def setUp(self):
        super().setUp()
        panels = []
        for lineage in ("r1", "r2"):
            for j, role in enumerate(("parent", "O", "D1L", "D2L")):
                for opp in OPPONENTS:
                    w = 5 + j + (lineage == "r2")
                    panels.append(write_panel(self.root / "screen", f"{lineage}-{role}", opp,
                                              pattern(w, len(panels), {(SPY, 1, 1, 1)} if j == 3 else ()),
                                              sha(f"{lineage}-{role}"), pilot.PANEL_NS))
        self.index = self.root / "screen-index.json"
        self.index.write_text(json.dumps({"panels": panels}), encoding="utf-8")
        self.result = self.root / "screen-RESULT.json"
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(A.main(["analyze", "--index", str(self.index), "--out", str(self.result)]), 0)
        self.recorded = json.loads(self.result.read_text(encoding="utf-8"))

    def x4_with(self, recorded: dict) -> dict:
        path = self.root / "changed-RESULT.json"
        path.write_text(json.dumps(recorded, indent=1) + "\n", encoding="utf-8")
        return S.x4(self.index, path)

    def test_the_screens_analyzer_reproduces_its_recorded_result(self):
        out = S.x4(self.index, self.result)
        self.assertTrue(out["pass"])
        # 4 contrasts, 7 metrics, 3 opponent sets, 2 bounds, 2 lineages.
        self.assertEqual(out["compared"], 336)
        self.assertEqual(out["screen_result"]["sha256"], hashlib.sha256(self.result.read_bytes()).hexdigest())
        self.assertEqual(out["seed"], 2026101320)

    def test_any_difference_fails(self):
        key = "r2/D2L-O/nine_deck/H/best"
        changed = copy.deepcopy(self.recorded)
        changed["contrasts"][key]["ci95"][0] = math.nextafter(changed["contrasts"][key]["ci95"][0], math.inf)
        self.assertEqual(self.x4_with(changed)["differing"], [key])
        changed = copy.deepcopy(self.recorded)
        changed["contrasts"][key]["point"] += 1e-12
        self.assertEqual(self.x4_with(changed)["differing"], [key])
        missing = copy.deepcopy(self.recorded)
        del missing["contrasts"][key]
        self.assertEqual(self.x4_with(missing)["differing"], [key])
        extra = copy.deepcopy(self.recorded)
        extra["contrasts"]["r1/D1U-parent/J/T1/worst"] = {"point": 0.0, "ci95": [0.0, 0.0]}
        out = self.x4_with(extra)
        self.assertEqual((out["differing"], out["pass"]), (["r1/D1U-parent/J/T1/worst"], False))
        with self.assertRaisesRegex(ValueError, "another seed or draw count"):
            self.x4_with({**self.recorded, "seed": S.BOOT_SEED})

    def test_cli_writes_the_record_and_exits_nonzero_when_x4_fails(self):
        out = self.root / "X4.json"
        argv = ["x4", "--screen-index", str(self.index), "--screen-result", str(self.result), "--out", str(out)]
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(S.main(argv), 0)
        self.assertTrue(json.loads(out.read_text(encoding="utf-8"))["pass"])
        changed = copy.deepcopy(self.recorded)
        changed["contrasts"]["r1/O-parent/J/pooled/worst"]["point"] += 1e-9
        self.result.write_text(json.dumps(changed), encoding="utf-8")
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(S.main(argv), 1)
        self.assertEqual(json.loads(out.read_text(encoding="utf-8"))["differing"], ["r1/O-parent/J/pooled/worst"])


# ---------------------------------------------------------------- analysis


class Analysis(Temp):
    def spec(self, lineages: dict, passed: bool = True) -> dict:
        return {**screen_pins(self.root, passed), "lineages": lineages}

    def test_one_lineage_from_panel_files(self):
        entry, shas = custody(self.root, "r1")
        panels = [as_pins(write_panel(self.root / "panels", ck, opp, outcome, shas[ck]))
                  for (ck, opp), outcome in outcomes("r1").items()]
        spec = self.spec({"r1": {**entry, "panels": panels}, "r2": {"not_run": "fit"}})
        out = S.analysis(spec, draws=DRAWS)
        # The record pins everything read.
        endpoints, read = S.expected_endpoints("r1", entry)
        self.assertEqual(out["inputs"], {
            "spec": None, **{f: spec[f] for f in ("screen_index", "screen_result", "x4")},
            "lineages": {"r1": {"endpoints": read, "panels": {
                f"{p['checkpoint']}/{p['opponent']}": {"rows": p["rows"], "labels": p["labels"]} for p in panels}}}})
        self.assertEqual(len(out["inputs"]["lineages"]["r1"]["panels"]), 50)
        self.assertEqual(out["inputs"]["lineages"]["r1"]["endpoints"]["r1-O-0"]["completion"], entry["o_s0"])
        self.assertEqual((out["seed"], out["draws"], out["not_run"]), (2026101601, DRAWS, {"r2": "fit"}))
        r1 = out["lineages"]["r1"]
        self.assertEqual(r1["primary"]["reading"], "improvement established")
        self.assertTrue(r1["primary"]["positive"])
        self.assertEqual(r1["secondary"]["reading"], "improvement established")
        self.assertNotIn("positive", r1["secondary"])
        self.assertEqual(out["lineages"]["r2"], {"not_run": "fit", "primary": {"reading": "not answered",
                                                                               "positive": False},
                                                 "secondary": {"reading": "not answered"}})
        self.assertFalse(out["method_screen_positive"])
        self.assertEqual(out["across_lineages"]["d/nine_deck/pooled"]["reading"],
                         "improvement established in r1 only (lineage-dependent: established in one lineage, which "
                         "is not an established difference between lineages); not run in r2 (fit)")
        self.assertEqual(len(r1["panels"]), 50)
        self.assertEqual(r1["panels"]["r1-D2-1/H"]["nine_deck"], 11 / 12)
        self.assertEqual({m["count"] for m in r1["missing"].values()}, {0})
        self.assertEqual(r1["contrasts"]["d"]["eight_deck"]["T1"]["least"]["pairs"],
                         [gain(k, "T1") / 12 - 6 / 12 for k in SEEDS])
        self.assertEqual(set(r1["contrasts"]), {"d", "e", "f", "D2-D1"})
        self.assertNotIn("reading", r1["contrasts"]["D2-D1"]["nine_deck"]["pooled"])
        self.assertNotIn("reading", r1["contrasts"]["d"]["deck_2"]["pooled"])
        self.assertNotIn("reading", r1["contrasts"]["d"]["J"]["pooled"])
        self.assertEqual(r1["change"]["nine_deck/pooled"]["reading"], r1["contrasts"]["f"]["nine_deck"]["pooled"]
                         ["reading"])
        self.assertEqual(len(out["deck_labels"]), DECKS)
        json.dumps(out)
        # A panel drawn under the frozen panel's seeds is refused.
        frozen = as_pins(write_panel(self.root / "frozen", "r1-O-2", "T1", outcomes("r1")[("r1-O-2", "T1")],
                                     shas["r1-O-2"], pilot.PANEL_NS))
        swapped = [frozen if (p["checkpoint"], p["opponent"]) == ("r1-O-2", "T1") else p for p in panels]
        with self.assertRaisesRegex(ValueError, "panel, checkpoint or seed differs"):
            S.analysis(self.spec({"r1": {**entry, "panels": swapped}, "r2": {"not_run": "fit"}}), draws=DRAWS)

    def both(self, r2: dict) -> dict:
        r1_entry, r1_shas = custody(self.root, "r1")
        r2_entry, r2_shas = custody(self.root, "r2")
        data = loaded(outcomes("r1"), r2)

        def load(index, allowed, namespace):
            self.assertEqual(namespace, "sd1-panel-v1")
            self.assertEqual({(p["checkpoint"], p["opponent"]) for p in index["panels"]}, set(data))
            self.assertTrue(all(ck in allowed for ck, _ in data))
            return data
        spec = self.spec({"r1": {**r1_entry, "panels": entries(self.root, "r1", r1_shas)},
                          "r2": {**r2_entry, "panels": entries(self.root, "r2", r2_shas)}})
        with mock.patch.object(A, "load", load):
            return S.analysis(spec, draws=DRAWS)

    def test_the_method_screen_is_positive_only_when_both_lineages_are(self):
        out = self.both(outcomes("r2"))
        self.assertTrue(out["method_screen_positive"])
        self.assertEqual(out["not_run"], {})
        self.assertEqual(out["across_lineages"]["d/nine_deck/pooled"]["reading"],
                         "improvement established in both lineages")
        null = self.both(outcomes("r2", d2=lambda k, opp: 6))
        self.assertFalse(null["method_screen_positive"])
        self.assertEqual(null["lineages"]["r2"]["primary"]["reading"], "no difference established")
        self.assertEqual(null["lineages"]["r2"]["contrasts"]["d"]["nine_deck"]["pooled"]["least"]["bootstrap"],
                         [0.0, 0.0])
        self.assertTrue(null["across_lineages"]["d/nine_deck/T1"]["reading"].startswith(
            "improvement established in r1 only"))
        harm = self.both(outcomes("r2", d2=lambda k, opp: 3))
        self.assertEqual(harm["lineages"]["r2"]["primary"]["reading"], "harm established")
        self.assertEqual(harm["across_lineages"]["d/nine_deck/pooled"]["reading"],
                         "in opposite directions (lineage-dependent)")

    def test_pooled_improvement_not_shown_against_h(self):
        out = self.both(outcomes("r2", d2=lambda k, opp: 11 if opp == "T1" else 5))
        r2 = out["lineages"]["r2"]
        self.assertEqual(r2["primary"]["pooled_nine_deck"], "improvement established")
        self.assertAlmostEqual(r2["primary"]["h_nine_deck_mean_least_favorable"], -1 / 12, places=15)
        self.assertEqual(r2["primary"]["reading"], S.NOT_SHOWN_AGAINST_H)
        self.assertFalse(r2["primary"]["positive"])
        self.assertFalse(out["method_screen_positive"])
        self.assertEqual(r2["contrasts"]["d"]["nine_deck"]["across_opponents"],
                         "in opposite directions against T1 and H (opponent-dependent)")

    def test_missing_cases_can_decide(self):
        # r2's D2 equals O, but D2-k's whole Spy row against T1 is missing in every pair.
        gone = {(f"r2-D2-{k}", "T1"): {(SPY, t, rep, s) for t in range(DECKS) for rep in range(REPEATS)
                                       for s in (0, 1)} for k in SEEDS}
        out = self.both(outcomes("r2", d2=lambda k, opp: 6, gone=gone))
        spy = out["lineages"]["r2"]["contrasts"]["d"]["spy_score"]["T1"]
        self.assertEqual((spy["least"]["label"], spy["most"]["label"]), ("harm", "improvement"))
        self.assertEqual(spy["reading"], "inconclusive: missing cases decide")
        self.assertIn("inconclusive in r2 (missing cases decide)", out["across_lineages"]["d/spy_score/T1"]["reading"])
        self.assertEqual(out["lineages"]["r2"]["missing"]["r2-D2-0/T1"]["count"], DECKS * REPEATS * 2)

    def test_refusals(self):
        entry, shas = custody(self.root, "r1")
        lineages = {"r1": {**entry, "panels": entries(self.root, "r1", shas)}, "r2": {"not_run": "fit"}}
        with self.assertRaisesRegex(ValueError, "X4 did not pass"):
            S.analysis(self.spec(lineages, passed=False), draws=DRAWS)
        spec = self.spec(lineages)
        spec["screen_result"] = write(self.root / "other-RESULT.json", {"contrasts": {"x": 1}})
        with self.assertRaisesRegex(ValueError, "X4 did not pass"):
            S.analysis(spec, draws=DRAWS)
        for bad in ({"not_run": "tired"}, {"not_run": "fit", "panels": []}):
            with self.assertRaisesRegex(ValueError, "names why"):
                S.analysis(self.spec({**lineages, "r2": bad}), draws=DRAWS)
        with self.assertRaisesRegex(ValueError, "both lineages"):
            S.analysis(self.spec({"r1": lineages["r1"]}), draws=DRAWS)

    def test_neither_lineage_ran(self):
        out = S.analysis(self.spec({"r1": {"not_run": "fit"}, "r2": {"not_run": "engineering"}}), draws=DRAWS)
        self.assertFalse(out["method_screen_positive"])
        self.assertEqual(out["across_lineages"]["e/eight_deck/H"]["reading"],
                         "not run in r1 (fit); not run in r2 (engineering)")

    def test_cli(self):
        spec_path = write(self.root / "SPEC.json", self.spec({"r1": {"not_run": "fit"}, "r2": {"not_run": "fit"}}))
        out = self.root / "RESULT.json"
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            self.assertEqual(S.main(["analyze", "--spec", spec_path["path"], "--out", str(out)]), 0)
        self.assertEqual(json.loads(stdout.getvalue()), {"method_screen_positive": False,
                                                          "primary": {"r1": "not answered", "r2": "not answered"}})
        result = json.loads(out.read_text(encoding="utf-8"))
        self.assertEqual(result["draws"], DRAWS)
        self.assertEqual(result["inputs"]["spec"], {"path": S.absolute(spec_path["path"]),
                                                    "sha256": spec_path["sha256"]})
        self.assertEqual(result["inputs"]["lineages"], {})


if __name__ == "__main__":
    unittest.main()
