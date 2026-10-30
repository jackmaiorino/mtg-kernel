"""RS0 tooling (collab LANES/reverse-search-20261029/RS0-PLAN.md): the planner's sample rule and the readout's flags."""
import json
import sys
import tempfile
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


class PlannerTests(unittest.TestCase):
    def rows(self, gid, proven_turn, hero_turns=(1, 3, 5, 7)):
        rows = [game(gid)]
        index = 0
        for t in range(1, max(hero_turns) + 2):
            active = 0 if t in hero_turns else 1
            for _ in range(2):
                rows.append(decision(gid, index, t, active, proven=(t == proven_turn and active == 0 and index % 2 == 1),
                                     line=spy_line() if t == proven_turn else None))
                index += 1
        return rows

    def test_opportunity_roots_are_the_first_decisions_of_earlier_hero_turns(self):
        rows = self.rows("g1", proven_turn=7)
        games, decisions = plan.read_rows_from(rows)
        opp = plan.opportunity(games["g1"], decisions["g1"], "spy_line")
        self.assertEqual(opp["turn"], 7)
        firsts = {t: min(r["index"] for r in decisions["g1"] if r["turn"] == t) for t in (3, 5, 7)}
        self.assertEqual(opp["roots"]["k0"], (firsts[7], 7))
        self.assertEqual(opp["roots"]["k1"], (firsts[5], 5))
        self.assertEqual(opp["roots"]["k2"], (firsts[3], 3))
        self.assertEqual(opp["certified"], firsts[7] + 1)

    def test_too_early_opportunities_are_not_eligible(self):
        rows = self.rows("g2", proven_turn=3)
        games, decisions = plan.read_rows_from(rows)
        self.assertIsNone(plan.opportunity(games["g2"], decisions["g2"], "spy_line"))

    def test_a_hero_turn_without_decisions_makes_the_game_ineligible(self):
        # Hero turn 5 has no recorded focal decision: k1 must be turn 5, not turn 3.
        rows = [r for r in self.rows("g3", proven_turn=7) if not (r.get("turn") == 5)]
        games, decisions = plan.read_rows_from(rows)
        self.assertIsNone(plan.opportunity(games["g3"], decisions["g3"], "spy_line"))

    def test_selection_order_is_the_game_id_hash(self):
        rows = []
        for gid in ("a", "b", "c", "d"):
            rows += self.rows(gid, proven_turn=7)
        games, decisions = plan.read_rows_from(rows)
        picks, counts = plan.select_class(games, decisions, "spy_line", "Spy", 2, "spy")
        self.assertEqual(counts, {"eligible": 4, "selected": 2})
        order = sorted(("a", "b", "c", "d"), key=plan.game_key)
        self.assertEqual([p["game_id"] for p in picks if p["selected"]], order[:2])
        self.assertTrue(any(r[0] == "cert" for r in picks[0]["roles"]))

    def test_control_panel_takes_three_distinct_games_per_deck(self):
        rows = []
        for gid in ("a", "b", "c", "d", "e"):
            rows += self.rows(gid, proven_turn=99)
        games, decisions = plan.read_rows_from(rows)
        picks = plan.select_control([(games, decisions)])
        chosen = [p for p in picks if p["selected"]]
        self.assertEqual(len(chosen), 3)
        self.assertEqual(len({p["game_id"] for p in chosen}), 3)


class ReportTests(unittest.TestCase):
    def test_flags_follow_the_closed_form_rule(self):
        self.assertEqual(report.flag(0.81, 0.95), "High")
        self.assertEqual(report.flag(0.70, 0.95), "Uncertain")
        self.assertEqual(report.flag(0.05, 0.20), "Low")
        self.assertEqual(report.flag(0.10, 0.30), "Uncertain")
        self.assertEqual(report.flag(0.30, 0.70), "Intermediate")

    def test_bootstrap_is_deterministic_and_game_clustered(self):
        values = {f"g{i}": float(i % 2) for i in range(20)}
        a, b = report.bootstrap(values), report.bootstrap(values)
        self.assertEqual(a, b)
        self.assertEqual(a["games"], 20)
        self.assertAlmostEqual(a["mean"], 0.5)
        self.assertLess(a["ci95"][0], 0.5)
        self.assertGreater(a["ci95"][1], 0.5)

    def test_seeds_average_within_a_game_before_the_interval(self):
        root = {"kind": "root", "root_id": "g#1", "game_id": "g",
                "roles": [{"stratum": "spy", "role": "k2", "rank": 0, "selected": True}]}
        def search(seed, hit):
            return {"kind": "search", "root_id": "g#1", "budget": 64000, "seed": seed, "o1_spy": hit,
                    "o2": {"valid": False}, "o3": {"found": False}, "self_chosen_simulations": 0, "hero_wins": 1}
        out = report.summarize([root, search(0, True), search(1, False), search(2, False)])
        cell = out["table"][0]
        self.assertEqual(cell["o1"]["games"], 1)
        self.assertAlmostEqual(cell["o1"]["mean"], 1 / 3)
        self.assertEqual(out["flags"][0]["budget"], 64000)
        self.assertFalse(cell["incomplete"])

    def test_failed_searches_count_as_not_found(self):
        root = {"kind": "root", "root_id": "g#1", "game_id": "g",
                "roles": [{"stratum": "spy", "role": "k2", "rank": 0, "selected": True}]}
        row = {"kind": "search", "root_id": "g#1", "budget": 64000, "seed": 0, "failed": True, "o1_spy": True,
               "o2": {"valid": False}, "o3": {"found": True}, "self_chosen_simulations": 3, "hero_wins": 2}
        cell = report.summarize([root, row])["table"][0]
        self.assertEqual(cell["o1"]["mean"], 0.0)
        self.assertEqual(cell["failed_searches"], 1)


if __name__ == "__main__":
    unittest.main()
