from __future__ import annotations

import contextlib
import copy
import io
from fractions import Fraction
from itertools import product
import json
import math
from pathlib import Path
import random
import statistics
import sys
import tempfile
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
import limited_campaign_eval_v1 as ev


MASK = (1 << 64) - 1
STEP0, UNIFORM, HEURISTIC, FAIR = ("initial-weights/step-0", "uniform-legal/v1", "sampled-public-heuristic/v1",
                                   "pimc:sims=16:depth=24:closedlist-full-pool-heuristic-v1")
SPECS_B = {STEP0: "learner-checkpoint:run/checkpoints/step_0000.json", UNIFORM: "random",
           HEURISTIC: HEURISTIC, FAIR: FAIR}
CANDIDATE_SPEC = {"u_final": "learner-checkpoint:run/checkpoints/step_0200.json",
                  "u_best_dev": "learner-checkpoint:run/checkpoints/step_0140.json"}
R = 1_000  # small replicate count for the in-process tests; the CLI test uses the design's 10,000


def first_output(seed: int) -> int:
    """SplitMix64's first output, written out separately from the tool."""
    z = (seed + 0x9E3779B97F4A7C15) & MASK
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
    return z ^ (z >> 31)


def stream(seed: int, count: int) -> list[int]:
    return [first_output((seed + k * 0x9E3779B97F4A7C15) & MASK) for k in range(count)]


def make_rows(opponent: str, results: list[tuple[str, str]], candidate: str = "u_final",
              decks: list[str] | None = None) -> list[dict]:
    """Rows as kernel_limited_match_v1 writes them; `results` holds (leg 0, leg 1) per pair."""
    _, _, seed = ev.OPPONENTS[opponent]
    names = decks or EVAL_NAMES
    rows = []
    for pair, legs in enumerate(results):
        deal = [names[pair % 40], names[(pair + 1 + pair // 40) % 40]]
        for leg, result in enumerate(legs):
            side = ["A", "B"] if leg == 0 else ["B", "A"]
            spec = {"A": CANDIDATE_SPEC[candidate], "B": SPECS_B[opponent]}
            winner = side.index(result) if result in ("A", "B") else -1
            rows.append({"game": 2 * pair + leg, "pair": pair, "leg": leg, "seed": seed, "decks": deal,
                         "side": side, "pols": [spec[s] for s in side], "starting": pair % 2, "winner": winner,
                         "result": result, "turns": 12, "decisions": 300 + pair, "rejected_worlds": 0,
                         "rollout_errors": 0, "ms": 1000 + 17 * pair + leg})
    return rows


def make_summary(opponent: str, rows: list[dict], candidate: str = "u_final", workers: int = 4) -> dict:
    _, _, seed = ev.OPPONENTS[opponent]
    count = lambda r: sum(row["result"] == r for row in rows)  # noqa: E731
    pairs = len(rows) // 2
    return {"schema": ev.MATCH_SCHEMA, "engine": "mtg-kernel", "card_db_hash": "00ff", "a": CANDIDATE_SPEC[candidate],
            "b": SPECS_B[opponent], "split": "eval", "seed": seed, "first_pair": 0, "pairs": pairs, "legs": 2,
            "playable_decks": 40, "playable_decks_sha256": "ab", "belief_overlap": 0, "belief_decks": None,
            "games": 2 * pairs, "a_wins": count("A"), "b_wins": count("B"), "draws": count("draw"),
            "stalls": count("stall"), "errors": count("error"), "wall_s": 12.5 * workers,
            "games_per_hour": 99.0 / workers, "workers": workers, "full_validation": False, "session": "FullPoolV1"}


def make_run(opponent: str, results: list[tuple[str, str]], candidate: str = "u_final") -> ev.RunInput:
    rows = make_rows(opponent, results, candidate)
    replay = [dict(r, ms=r["ms"] + 5) for r in rows if r["pair"] < 4]
    return ev.RunInput(candidate, opponent, rows, make_summary(opponent, rows, candidate, 4),
                       replay, make_summary(opponent, replay, candidate, 1))


def pattern(k: int, weights: dict[tuple[str, str], int], seed: int = 1) -> list[tuple[str, str]]:
    """k pairs drawn with a fixed generator from weighted leg outcomes."""
    rng = random.Random(seed)
    choices = [legs for legs, w in weights.items() for _ in range(w)]
    return [rng.choice(choices) for _ in range(k)]


STRONG = {("A", "A"): 8, ("A", "B"): 1, ("A", "draw"): 1}
EVEN = {("A", "B"): 1, ("B", "A"): 1, ("draw", "draw"): 1}


def full_inputs(strength: dict = STRONG, candidate: str = "u_final") -> list[ev.RunInput]:
    return [make_run(opponent, pattern(ev.OPPONENTS[opponent][1], strength, i), candidate)
            for i, opponent in enumerate((STEP0, UNIFORM, HEURISTIC, FAIR))]


INVENTORY: dict = {}
EVAL_NAMES: list[str] = []


def setUpModule() -> None:
    INVENTORY.update(ev.load_inventory(None))
    EVAL_NAMES.extend(ev.eval_subgroups(INVENTORY)["decks"])


class GeneratorTest(unittest.TestCase):
    def test_first_outputs_match_the_rust_tests(self):
        # mtg-kernel/tests/pauper/fast_sampler.rs, splitmix_first_outputs_are_exact
        for seed, expected in ((0, 0xE220_A839_7B1D_CDAF), (1, 0x910A_2DEC_8902_5CC1),
                               (MASK, 0xE4D9_7177_1B65_2C20)):
            self.assertEqual(ev.SplitMix64(seed).next_u64(), expected)

    def test_stream_matches_rust_state_splitmix(self):
        rng = ev.SplitMix64(0)
        self.assertEqual([rng.next_u64() for _ in range(6)],
                         [0xE220_A839_7B1D_CDAF, 0x6E78_9E6A_A1B9_65F4, 0x06C4_5D18_8009_454F,
                          0xF88B_B8A8_724C_81EC, 0x1B39_896A_51A8_749B, 0x53CB_9F0C_747E_A2EA])
        rng = ev.SplitMix64(657_999)
        self.assertEqual([rng.next_u64() for _ in range(50)], stream(657_999, 50))

    def test_unbiased_index_rejects_the_top_bucket(self):
        class Fake:
            def __init__(self, values):
                self.values = list(values)

            def next_u64(self):
                return self.values.pop(0)

        limit = ((1 << 64) // 3) * 3
        fake = Fake([MASK, limit, limit - 1])
        self.assertEqual(ev.unbiased_index(3, fake), (limit - 1) % 3)
        self.assertEqual(fake.values, [])

    def test_bootstrap_indices_follow_the_seeded_stream(self):
        draws = stream(657_999, 40)
        self.assertEqual(list(ev.bootstrap_indices(7, 5, 657_999)), [d % 7 for d in draws[:35]])


class StatisticsTest(unittest.TestCase):
    def test_normal_functions_match_the_standard_library(self):
        reference = statistics.NormalDist()
        for x in ("-6.5", "-1.96", "-0.3", "0", "0.7", "2.2414"):
            self.assertAlmostEqual(float(ev.normal_cdf(ev.Decimal(x))), reference.cdf(float(x)), places=14)
        for p in (Fraction(1, 40), Fraction(1, 80), Fraction(1, 2), Fraction(9_123, 10_000)):
            self.assertAlmostEqual(float(ev.normal_ppf(p)), reference.inv_cdf(float(p)), places=12)

    def test_percentile_indices_at_the_design_count(self):
        self.assertEqual(ev.percentile_indices(10_000, Fraction(1, 20)), (249, 9_750))
        self.assertEqual(ev.percentile_indices(10_000, Fraction(1, 40)), (124, 9_875))

    def test_bootstrap_percentile_and_bca_against_a_float_reference(self):
        rng = random.Random(3)
        totals = [rng.choice([0, 1, 2, 2, 3, 4, 4, 4]) for _ in range(30)]
        result = ev.bootstrap(totals, (Fraction(1, 20), Fraction(1, 40)), R, 657_999)
        draws = stream(657_999, 30 * R + 10)
        sums = [sum(totals[d % 30] for d in draws[r * 30:(r + 1) * 30]) for r in range(R)]
        ordered = sorted(sums)
        observed = sum(totals)
        self.assertEqual(result["observed_sum"], observed)
        below = sum(s < observed for s in sums)
        self.assertEqual(result["replicates_below_observed"], below)
        normal = statistics.NormalDist()
        z0 = normal.inv_cdf(below / R)
        mean = observed / 30
        deviations = [t - mean for t in totals]
        acceleration = sum(d ** 3 for d in deviations) / (6 * sum(d * d for d in deviations) ** 1.5)
        for alpha, z in ((Fraction(1, 20), normal.inv_cdf(0.975)), (Fraction(1, 40), normal.inv_cdf(0.9875))):
            interval = result["intervals"][f"{alpha.numerator}/{alpha.denominator}"]
            low = math.floor(float(alpha / 2) * (R - 1))
            high = math.ceil(float(1 - alpha / 2) * (R - 1))
            self.assertEqual((interval["percentile"]["lower_sum"], interval["percentile"]["upper_sum"]),
                             (ordered[low], ordered[high]))
            a1 = normal.cdf(z0 + (z0 - z) / (1 - acceleration * (z0 - z)))
            a2 = normal.cdf(z0 + (z0 + z) / (1 - acceleration * (z0 + z)))
            bca = interval["bca"]
            self.assertTrue(bca["defined"])
            self.assertEqual((bca["lower_index"], bca["upper_index"]),
                             (math.floor(a1 * (R - 1)), math.ceil(a2 * (R - 1))))
            self.assertEqual(interval["gate_lower_sum"], min(ordered[low], ordered[bca["lower_index"]]))

    def test_bca_is_undefined_when_every_pair_is_equal(self):
        result = ev.bootstrap([4] * 20, (Fraction(1, 20),), R, 657_999)
        interval = result["intervals"]["1/20"]
        self.assertFalse(interval["bca"]["defined"])
        self.assertEqual(interval["gate_lower"], 1.0)

    def test_sign_test_by_enumeration(self):
        for totals in ([4, 3, 3, 1, 2, 2, 4, 0, 3], [4, 4, 4, 4, 4, 4, 4, 3], [2, 2, 2], [3, 1], [0, 0, 1, 4]):
            discordant = [t for t in totals if t != 2]
            n, wins = len(discordant), sum(t > 2 for t in discordant)
            hits = sum(abs(sum(signs) - n / 2) >= abs(wins - n / 2) for signs in product((0, 1), repeat=n))
            result = ev.sign_test(totals)
            self.assertEqual(Fraction(result["p_value_exact"]), Fraction(hits, 2 ** n))
            self.assertEqual((result["better_pairs"], result["tied_pairs"]), (wins, len(totals) - n))

    def test_wilcoxon_by_enumeration_with_midranks(self):
        cases = ([4, 3, 3, 1, 2, 2, 4, 0, 3, 3], [4, 4, 3, 4, 0, 4, 3, 4], [1, 1, 0, 3, 2], [2, 2], [3, 4, 4, 4, 4])
        for totals in cases:
            differences = [Fraction(t, 4) - Fraction(1, 2) for t in totals if t != 2]
            ordered = sorted(abs(d) for d in differences)
            ranks = {}
            for magnitude in set(ordered):
                positions = [i + 1 for i, m in enumerate(ordered) if m == magnitude]
                ranks[magnitude] = Fraction(sum(positions), len(positions))
            weights = [ranks[abs(d)] for d in differences]
            observed = sum(w for w, d in zip(weights, differences) if d > 0)
            center = sum(weights) / 2
            hits = sum(abs(sum(w for w, s in zip(weights, signs) if s) - center) >= abs(observed - center)
                       for signs in product((0, 1), repeat=len(weights)))
            result = ev.wilcoxon_test(totals)
            self.assertEqual(Fraction(result["p_value_exact"]), Fraction(hits, 2 ** len(weights)), totals)
            self.assertEqual(result["w_plus"], float(observed))

    def test_wilson_matches_the_closed_form(self):
        for successes, trials in ((0, 10), (7, 10), (260, 400), (400, 400)):
            p, z = successes / trials, 1.959963984540054
            center = (p + z * z / (2 * trials)) / (1 + z * z / trials)
            margin = z * math.sqrt(p * (1 - p) / trials + z * z / (4 * trials * trials)) / (1 + z * z / trials)
            result = ev.wilson_interval(successes, trials)
            self.assertAlmostEqual(result["lower"], max(0.0, center - margin), places=12)
            self.assertAlmostEqual(result["upper"], min(1.0, center + margin), places=12)
        self.assertIsNone(ev.wilson_interval(0, 0))

    def test_pair_sd_and_elo(self):
        totals = [4, 3, 2, 2, 1, 4, 0, 3]
        self.assertAlmostEqual(ev.pair_sd(totals), statistics.stdev([t / 4 for t in totals]), places=14)
        mean = sum(totals) / 32
        self.assertAlmostEqual(ev.elo_difference(totals), 400 * math.log10(mean / (1 - mean)), places=9)
        self.assertIsNone(ev.elo_difference([4, 4]))


def stats_for(pairs: int, lower_95: int, lower_975: int, half_points: int) -> dict:
    whole = 4 * pairs
    return {"primary": {"pairs": pairs, "half_points": half_points, "bootstrap": {"intervals": {
        "1/20": {"gate_lower_sum": lower_95, "gate_lower": lower_95 / whole},
        "1/40": {"gate_lower_sum": lower_975, "gate_lower": lower_975 / whole}}}}}


class GateTest(unittest.TestCase):
    def roles(self, step0=417, uniform=481, heuristic=401, fair=201) -> dict:
        return {"step_0": stats_for(200, step0, step0, 600), "uniform_legal": stats_for(200, uniform, uniform, 600),
                "heuristic": stats_for(200, heuristic, heuristic, 600), "fair_search": stats_for(100, fair, fair, 300)}

    def verdicts(self, e0=True, **bounds) -> dict:
        tests = ev.lower_bound_tests({"u_final": self.roles(**bounds)}, "primary")["u_final"]
        return {name: gate["pass"] for name, gate in ev.gate_verdicts(tests, e0).items()}

    def test_every_threshold_is_strict_on_both_sides(self):
        self.assertEqual(self.verdicts(), {"E1": True, "E2": True, "E3": True})
        self.assertEqual(self.verdicts(step0=416), {"E1": False, "E2": False, "E3": False})  # 0.52 exactly
        self.assertEqual(self.verdicts(uniform=480), {"E1": False, "E2": False, "E3": False})  # 0.60 exactly
        self.assertEqual(self.verdicts(heuristic=400), {"E1": True, "E2": False, "E3": False})
        self.assertEqual(self.verdicts(fair=200), {"E1": True, "E2": True, "E3": False})
        self.assertEqual(self.verdicts(e0=False), {"E1": False, "E2": False, "E3": False})

    def test_a_missing_run_fails_its_gate(self):
        roles = self.roles()
        del roles["fair_search"]
        tests = ev.lower_bound_tests({"u_final": roles}, "primary")["u_final"]
        gates = ev.gate_verdicts(tests, True)
        self.assertTrue(gates["E2"]["pass"])
        self.assertEqual(gates["E3"]["reasons"], ["no run against fair_search"])

    def test_holm_for_two_candidates(self):
        better = {"step_0": stats_for(200, 430, 420, 640)}
        other = {"step_0": stats_for(200, 418, 410, 620)}
        tests = ev.lower_bound_tests({"u_final": better, "u_best_dev": other}, "primary")
        self.assertEqual((tests["u_final"]["step_0"]["alpha"], tests["u_final"]["step_0"]["pass"]), ("1/40", True))
        self.assertEqual((tests["u_best_dev"]["step_0"]["alpha"], tests["u_best_dev"]["step_0"]["pass"]),
                         ("1/20", True))
        better = {"step_0": stats_for(200, 430, 416, 640)}  # passes at 95% but not at 97.5%
        tests = ev.lower_bound_tests({"u_final": better, "u_best_dev": other}, "primary")
        self.assertFalse(tests["u_final"]["step_0"]["pass"])
        self.assertFalse(tests["u_best_dev"]["step_0"]["pass"])  # Holm stops at the first failure
        tied = {"step_0": stats_for(200, 430, 416, 620)}
        tests = ev.lower_bound_tests({"u_final": tied, "u_best_dev": other}, "primary")
        self.assertEqual({t["step_0"]["alpha"] for t in tests.values()}, {"1/40"})
        self.assertFalse(tests["u_best_dev"]["step_0"]["pass"])


class EvaluationTest(unittest.TestCase):
    def evaluate(self, inputs) -> dict:
        return ev.evaluate(inputs, INVENTORY, replicates=R)

    def test_strong_candidate_passes_every_gate(self):
        result = self.evaluate(full_inputs())
        self.assertTrue(result["gate_e0"]["pass"])
        candidate = result["candidates"]["u_final"]
        self.assertEqual(candidate["highest_gate_passed"], "E3")
        run = candidate["runs"]["step_0"]
        self.assertEqual(run["counts"]["games"], 400)
        self.assertEqual(run["primary"]["pairs"], 200)
        self.assertEqual(result["multiplicity"], {"alpha": "1/20", "candidates_evaluated": 1})

    def test_even_candidate_passes_no_gate(self):
        result = self.evaluate(full_inputs(EVEN))
        self.assertTrue(result["gate_e0"]["pass"])
        self.assertEqual(result["candidates"]["u_final"]["highest_gate_passed"], "none")

    def test_fair_search_report_when_only_e3_fails(self):
        inputs = full_inputs()
        inputs[3] = make_run(FAIR, pattern(100, EVEN, 9))
        candidate = self.evaluate(inputs)["candidates"]["u_final"]
        self.assertEqual(candidate["highest_gate_passed"], "E2")
        report = candidate["fair_search_report"]
        self.assertEqual(report["mean"], candidate["runs"]["fair_search"]["primary"]["mean"])

    def test_scoring_and_stall_sensitivity(self):
        results = [("A", "stall"), ("draw", "B"), ("stall", "stall")] + [("A", "A")] * 197
        inputs = full_inputs()
        inputs[0] = make_run(STEP0, results)
        run = self.evaluate(inputs)["candidates"]["u_final"]["runs"]["step_0"]
        self.assertEqual(run["primary"]["half_points"], 3 + 1 + 2 + 4 * 197)
        self.assertEqual(run["stall_sensitivity"]["half_points"], 2 + 1 + 0 + 4 * 197)
        self.assertEqual(run["counts"], {"games": 400, "win": 395, "draw": 1, "loss": 1, "stall": 3, "error": 0})
        self.assertEqual(run["wilson_descriptive"]["wins_over_decisive_games"]["trials"], 396)

    def test_e0_conditions(self):
        def e0(mutate) -> dict:
            inputs = full_inputs()
            mutate(inputs[1])
            return self.evaluate(inputs)["gate_e0"]

        def stalls(n):
            def mutate(run):
                results = [("stall", "A")] * n + [("A", "A")] * (200 - n)
                fresh = make_run(UNIFORM, results)
                run.rows, run.summary, run.replay_rows, run.replay_summary = (
                    fresh.rows, fresh.summary, fresh.replay_rows, fresh.replay_summary)
            return mutate

        self.assertTrue(e0(stalls(8))["pass"])  # 8 of 400 games is exactly 2%
        self.assertEqual(e0(stalls(9))["failed_runs"], ["u_final vs uniform_legal: stall_rate"])

        def error(run):
            row = run.rows[11]
            row.update(result="error", winner=-1, error="halted")
            run.summary["a_wins"] = sum(r["result"] == "A" for r in run.rows)
            run.summary["b_wins"] = sum(r["result"] == "B" for r in run.rows)
            run.summary["draws"] = sum(r["result"] == "draw" for r in run.rows)
            run.summary["errors"] = 1
        self.assertIn("errors", e0(error)["failed_runs"][0])

        def rollout(run):
            run.rows[50]["rollout_errors"] = 2
        self.assertIn("rollout_errors", e0(rollout)["failed_runs"][0])

        def overlap(run):
            run.summary["belief_overlap"] = 1
        self.assertIn("belief_overlap", e0(overlap)["failed_runs"][0])

        def replay(run):
            run.replay_rows[3]["turns"] += 1
        self.assertIn("replay_not_identical", e0(replay)["failed_runs"][0])

        def workers(run):
            run.replay_summary["workers"] = run.summary["workers"]
        self.assertIn("replay_same_worker_count", e0(workers)["failed_runs"][0])

        inputs = full_inputs()[:3]
        gate = self.evaluate(inputs)["gate_e0"]
        self.assertEqual(gate["absent_runs"], ["u_final vs fair_search"])
        self.assertFalse(gate["pass"])

    def test_canonical_bytes_are_stable(self):
        first = ev.canonical_bytes(self.evaluate(full_inputs()))
        shuffled = full_inputs()
        rng = random.Random(5)
        for run in shuffled:
            rng.shuffle(run.rows)
            rng.shuffle(run.replay_rows)
            for row in run.rows:
                row["ms"] += 3
            run.summary.update(wall_s=1.0, games_per_hour=7.0, workers=1)
            run.replay_summary.update(workers=8)
        second = ev.canonical_bytes(self.evaluate(list(reversed(shuffled))))
        self.assertEqual(first, second)
        text = first.decode()
        for volatile in ('"ms"', '"wall_s"', '"games_per_hour"', '"workers"'):
            self.assertNotIn(volatile, text)
        data = json.loads(text)
        self.assertEqual(text, json.dumps(data, sort_keys=True, indent=2) + "\n")

    def test_subgroup_audit(self):
        run = self.evaluate(full_inputs())["candidates"]["u_final"]["runs"]["step_0"]
        audit = run["subgroups"]
        per_leg = audit["per_leg_by_candidate_deck"]
        self.assertEqual(per_leg["full_only"]["games"] + per_leg["contains_partial"]["games"], 400)
        classes = ev.eval_subgroups(INVENTORY)
        rows = make_rows(STEP0, pattern(200, STRONG, 0))
        expected_full = sum(r["decks"][r["side"].index("A")] in classes["full_only"] for r in rows)
        self.assertEqual(per_leg["full_only"]["games"], expected_full)
        strata = audit["per_pair_stratum"]
        self.assertEqual(sum(s["pairs"] for s in strata.values()), 200)
        both_full = sum(all(d in classes["full_only"] for d in r["decks"]) for r in rows if r["leg"] == 0)
        self.assertEqual(strata["full_vs_full"]["pairs"], both_full)
        drake = sum(bool(set(r["decks"]) & set(ev.DRAKE_DECKS)) for r in rows if r["leg"] == 0)
        self.assertEqual(audit["pairs_with_drake_hatcher_deck"]["pairs"], drake)
        self.assertEqual(set(audit["pairs_with_over_40_deck"]["by_deck"]), set(ev.OVER_40_DECKS))

    def test_two_candidates_use_holm(self):
        inputs = full_inputs() + full_inputs(EVEN, candidate="u_best_dev")
        result = self.evaluate(inputs)
        self.assertEqual(result["multiplicity"]["candidates_evaluated"], 2)
        alphas = {c: {t["alpha"] for t in e["tests"].values()} for c, e in result["candidates"].items()}
        self.assertEqual(alphas, {"u_final": {"1/40"}, "u_best_dev": {"1/20"}})
        self.assertEqual(result["candidates"]["u_final"]["highest_gate_passed"], "E3")
        self.assertEqual(result["candidates"]["u_best_dev"]["highest_gate_passed"], "none")


class RefusalTest(unittest.TestCase):
    def refuses(self, mutate, message: str) -> None:
        inputs = full_inputs()
        mutate(inputs)
        with self.assertRaises(ev.EvaluationInputError) as caught:
            ev.evaluate(inputs, INVENTORY, replicates=R)
        self.assertIn(message, str(caught.exception))

    def test_incomplete_or_malformed_records(self):
        self.refuses(lambda i: i[0].rows.pop(7), "games missing")
        self.refuses(lambda i: i[0].rows.append(dict(i[0].rows[0])), "duplicate record")
        self.refuses(lambda i: i[0].rows[4].update(result="win"), "unknown result")
        self.refuses(lambda i: i[0].rows[4].update(pair="2"), "not an integer")
        self.refuses(lambda i: i[0].rows[4].pop("decks"), "missing decks")
        self.refuses(lambda i: i[0].rows[5].update(side=["A", "B"], pols=i[0].rows[5]["pols"][::-1]), "side")
        self.refuses(lambda i: i[0].rows[4].update(winner=1 - i[0].rows[4]["winner"]), "winner")
        self.refuses(lambda i: i[0].rows[4].update(decks=["FDN_top_01766_UR", i[0].rows[4]["decks"][1]]),
                     "eval decks")
        self.refuses(lambda i: i[0].rows[5].update(starting=1 - i[0].rows[5]["starting"]), "legs differ")
        self.refuses(lambda i: i[0].replay_rows.pop(), "games missing")

    def test_runs_that_are_not_the_designs(self):
        self.refuses(lambda i: i[0].summary.update(split="train"), "split")
        self.refuses(lambda i: i[0].summary.update(pairs=199), "pairs")
        self.refuses(lambda i: i[0].summary.update(max_decisions=20_000), "max_decisions")
        self.refuses(lambda i: i[0].summary.update(seed=657_102), "seed")
        self.refuses(lambda i: i[0].summary.update(extra=1), "unknown keys")
        self.refuses(lambda i: i[0].summary.update(a_wins=0), "a_wins")
        self.refuses(lambda i: i[1].summary.update(b="pimc:sims=16:depth=24:closedlist-full-pool-v1"), "policy B")
        self.refuses(lambda i: setattr(i[0], "opponent", "closedlist-full-pool-v1"), "unknown opponent")
        self.refuses(lambda i: setattr(i[0], "candidate", "u_other"), "unknown candidate")
        self.refuses(lambda i: i.append(make_run(FAIR, pattern(100, STRONG))), "two runs")
        self.refuses(lambda i: [setattr(run, "candidate", "u_best_dev") for run in i], "no run of u_final")

    def test_inventory_must_match_the_design(self):
        inventory = copy.deepcopy(INVENTORY)
        inventory["sample_sha256_in_pool_order"] = "0" * 64
        with self.assertRaises(ev.EvaluationInputError):
            ev.evaluate(full_inputs(), inventory, replicates=R)
        inventory = copy.deepcopy(INVENTORY)
        next(d for d in inventory["decks"] if d["deck"] == "FDN_top_13096_WU")["contains_drake_hatcher"] = False
        with self.assertRaises(ev.EvaluationInputError):
            ev.eval_subgroups(inventory)

    def test_read_jsonl_refuses_a_broken_line(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "games.jsonl"
            path.write_text('{"game":0}\n{"game":1,\n', encoding="utf-8")
            with self.assertRaises(ev.EvaluationInputError):
                ev.read_jsonl(path)


class CommandTest(unittest.TestCase):
    def test_cli_writes_canonical_evaluation_and_timing_sidecar(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            argv = ["evaluate"]
            for index, run in enumerate(full_inputs()):
                paths = []
                for name, rows, summary in (("games", run.rows, run.summary),
                                            ("replay", run.replay_rows, run.replay_summary)):
                    path = root / f"run{index}" / name / "games.jsonl"
                    path.parent.mkdir(parents=True)
                    path.write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
                    Path(f"{path}.summary.json").write_text(json.dumps(summary), encoding="utf-8")
                    paths.append(str(path))
                argv += ["--run", run.candidate, run.opponent, *paths]
            out = root / "out" / "evaluation.json"
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                self.assertEqual(ev.main(argv + ["--out", str(out)]), 0)
            self.assertEqual(json.loads(printed.getvalue())["gates"]["u_final"]["E3"], True)
            data = out.read_bytes()
            evaluation = json.loads(data)
            self.assertEqual(evaluation["bootstrap"]["replicates"], 10_000)
            self.assertEqual(evaluation["bootstrap"]["seed"], 657_999)
            self.assertEqual(data, ev.canonical_bytes(evaluation))
            timing = json.loads((root / "out" / "evaluation.timing.json").read_text(encoding="utf-8"))
            self.assertEqual({r["main"]["workers"] for r in timing["runs"]}, {4})
            self.assertIn("evaluation_seconds", timing)
            Path(argv[4] + ".summary.json").unlink()
            with contextlib.redirect_stderr(io.StringIO()) as refused:
                self.assertEqual(ev.main(argv + ["--out", str(root / "refused.json")]), 2)
            self.assertIn("missing summary", refused.getvalue())
            self.assertFalse((root / "refused.json").exists())


if __name__ == "__main__":
    unittest.main()
