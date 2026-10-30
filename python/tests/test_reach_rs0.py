"""RS0 tooling (collab LANES/reverse-search-20261029/RS0-PLAN.md): the planner's sample rule and the readout."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools.reach import plan, report  # noqa: E402

SPY = "Balustrade Spy"


def spy_line() -> list[str]:
    # The class rule reads card ids out of the line text.
    import summarize  # on sys.path through tools.reach.plan
    return [f"ChooseTarget {{ source: CardStableRefV1 {{ card_db_id: {summarize.ID[SPY]} }} }}"]


def decision(gid, index, turn, active, proven=False, line=None):
    robust = {"verdict": "proven", "line": line or []} if proven else {"verdict": "refuted"}
    return {"kind": "decision", "id": gid, "index": index, "turn": turn, "focal": 0, "active": active,
            "oracle": {"verdict": robust["verdict"], "line": line or []}, "robust": robust}


def game(gid, deck="Spy/abc"):
    return {"kind": "game", "id": gid, "focal": 0, "deck_ids": [deck, "Burn/x"]}


def game_rows(gid, proven_turn, hero_turns=(1, 3, 5, 7)):
    rows = [game(gid)]
    index = 0
    for t in range(1, max(hero_turns) + 2):
        active = 0 if t in hero_turns else 1
        for _ in range(2):
            rows.append(decision(gid, index, t, active, proven=(t == proven_turn and active == 0 and index % 2 == 1),
                                 line=spy_line() if t == proven_turn else None))
            index += 1
    return rows


class PlannerTests(unittest.TestCase):
    def test_opportunity_roots_are_the_first_decisions_of_the_preceding_hero_turns(self):
        games, decisions = plan.read_rows_from(game_rows("g1", proven_turn=7))
        opp = plan.opportunity(games["g1"], decisions["g1"], "spy_line")
        self.assertEqual(opp["turn"], 7)
        firsts = {t: min(r["index"] for r in decisions["g1"] if r["turn"] == t) for t in (3, 5, 7)}
        self.assertEqual(opp["roots"]["k0"], (firsts[7], 7))
        self.assertEqual(opp["roots"]["k1"], (firsts[5], 5))
        self.assertEqual(opp["roots"]["k2"], (firsts[3], 3))
        self.assertEqual(opp["certified"], firsts[7] + 1)

    def test_too_early_opportunities_are_not_eligible(self):
        games, decisions = plan.read_rows_from(game_rows("g2", proven_turn=3))
        self.assertIsNone(plan.opportunity(games["g2"], decisions["g2"], "spy_line"))

    def test_a_hero_turn_without_decisions_makes_the_game_ineligible(self):
        # Hero turn 5 has no recorded decision: k1 must be turn 5, never turn 3.
        rows = [r for r in game_rows("g3", proven_turn=7) if r.get("turn") != 5]
        games, decisions = plan.read_rows_from(rows)
        self.assertIsNone(plan.opportunity(games["g3"], decisions["g3"], "spy_line"))

    def test_selection_order_is_the_game_id_hash_and_refusals_promote_the_next_game(self):
        rows = []
        for gid in ("a", "b", "c", "d"):
            rows += game_rows(gid, proven_turn=7)
        games, decisions = plan.read_rows_from(rows)
        order = sorted(("a", "b", "c", "d"), key=plan.game_key)
        picks, counts = plan.select_class(games, decisions, "spy_line", "Spy", 2, "spy", {})
        self.assertEqual(counts, {"eligible": 4, "selected": 2})
        self.assertEqual([p["game_id"] for p in picks if p["status"] == "selected"], order[:2])
        self.assertTrue(any(r[0] == "cert" for r in picks[0]["roles"]))
        picks, counts = plan.select_class(games, decisions, "spy_line", "Spy", 2, "spy", {order[0]: "replay: x"})
        self.assertEqual([p["game_id"] for p in picks if p["status"] == "selected"], order[1:3])
        self.assertEqual([p["game_id"] for p in picks if p["status"] == "refused"], [order[0]])

    def test_control_panel_uses_hero_turns_and_three_distinct_games_per_deck(self):
        rows = []
        for gid in ("a", "b", "c", "d", "e"):
            rows += game_rows(gid, proven_turn=99)
        games, decisions = plan.read_rows_from(rows)
        picks, _ = plan.select_control([(games, decisions)], {})
        chosen = [p for p in picks if p["status"] == "selected"]
        self.assertEqual(len(chosen), 3)
        self.assertEqual(len({p["game_id"] for p in chosen}), 3)
        by_key = {(r["id"], r["index"]): r for g in decisions.values() for r in g}
        for p in chosen:
            _, index, _ = p["roles"][0]
            self.assertEqual(by_key[(p["game_id"], index)]["active"], 0)

    def test_a_refused_control_root_keeps_its_game_available(self):
        # Three games, three slots: the refused game must still enter through another decision.
        rows = []
        for gid in ("a", "b", "c"):
            rows += game_rows(gid, proven_turn=99)
        games, decisions = plan.read_rows_from(rows)
        first, _ = plan.select_control([(games, decisions)], {})
        top = next(p for p in first if p["status"] == "selected")
        key = f"{top['game_id']}#{top['roles'][0][1]}"
        again, _ = plan.select_control([(games, decisions)], {key: "replay: x"})
        self.assertIn(top["game_id"], {p["game_id"] for p in again if p["status"] == "selected"})
        self.assertEqual(len([p for p in again if p["status"] == "selected"]), 3)


def root(root_id, game_id, role="k2", stratum="spy"):
    return {"root_id": root_id, "game_id": game_id, "roles": [{"stratum": stratum, "role": role, "rank": 0, "selected": True}]}


def search(root_id, seed, hit, budget=64000, failed=False):
    return {"kind": "search", "root_id": root_id, "budget": budget, "seed": seed, "o1_spy": hit, "failed": failed,
            "o2": {"valid": False}, "o3": {"found": False}, "self_chosen_simulations": 0, "hero_wins": 1}


class ReportTests(unittest.TestCase):
    def test_flags_follow_the_closed_form_rule(self):
        self.assertEqual(report.flag(0.81, 0.95), "High")
        self.assertEqual(report.flag(0.70, 0.95), "Uncertain")
        self.assertEqual(report.flag(0.05, 0.20), "Low")
        self.assertEqual(report.flag(0.10, 0.30), "Uncertain")
        self.assertEqual(report.flag(0.30, 0.70), "Intermediate")

    def test_seeds_average_within_a_game_and_the_matrix_is_complete(self):
        roots = [root("g#1", "g")]
        rows = [search("g#1", 0, True), search("g#1", 1, False), search("g#1", 2, False)]
        out = report.summarize(rows, roots, [64000], [0, 1, 2], None)
        cell = out["table"][0]
        self.assertFalse(cell["incomplete"])
        self.assertAlmostEqual(cell["o1"]["mean"], 1 / 3)
        self.assertEqual(out["flags"][0]["budget"], 64000)

    def test_a_missing_search_suppresses_the_estimate_and_the_flag(self):
        roots = [root("g#1", "g"), root("h#1", "h")]
        rows = [search("g#1", s, True) for s in (0, 1, 2)] + [search("h#1", 0, True)]
        out = report.summarize(rows, roots, [64000], [0, 1, 2], None)
        self.assertEqual(len(out["missing_searches"]), 2)
        self.assertTrue(out["table"][0]["incomplete"])
        self.assertIsNone(out["table"][0]["o1"]["mean"])
        self.assertIsNone(out["flags"][0]["flag"])

    def test_duplicates_are_an_error(self):
        with self.assertRaises(SystemExit):
            report.summarize([search("g#1", 0, True), search("g#1", 0, True)], [root("g#1", "g")], [64000], [0], None)

    def test_failed_searches_count_as_not_found(self):
        out = report.summarize([search("g#1", 0, True, failed=True)], [root("g#1", "g")], [64000], [0], None)
        cell = out["table"][0]
        self.assertEqual(cell["o1"]["mean"], 0.0)
        self.assertEqual(cell["failed_searches"], 1)

    def test_the_large_budget_is_expected_for_spy_k2_and_never_flagged(self):
        roots = [root("g#1", "g")]
        rows = [search("g#1", 0, True), search("g#1", 0, True, budget=1_024_000)]
        out = report.summarize(rows, roots, [64000], [0], 1_024_000)
        self.assertEqual(out["missing_searches"], [])
        self.assertEqual([f["budget"] for f in out["flags"]], [64000])

    def test_cells_share_one_set_of_bootstrap_replicates(self):
        games = [f"g{i}" for i in range(30)]
        reps = report.Replicates(games)
        values = {g: float(i % 2) for i, g in enumerate(games)}
        a, b = reps.interval(values), report.Replicates(games).interval(values)
        self.assertEqual(a, b)
        subset = {g: v for g, v in list(values.items())[:10]}
        self.assertEqual(reps.interval(subset)["games"], 10)
        self.assertLess(a["ci95"][0], 0.5)
        self.assertGreater(a["ci95"][1], 0.5)


if __name__ == "__main__":
    unittest.main()
