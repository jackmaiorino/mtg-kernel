from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
import eternal_meta_census_v1 as census
import pauper_legality_v1 as legality_tool


FIELD_SNAPSHOT = REPO_ROOT / "data/eternal_v1/meta_snapshot_mtgtop8_pauper_20261121.json"


def run_tool(*argv: str) -> tuple[int, str, str]:
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = legality_tool.main(list(argv))
    return code, out.getvalue(), err.getvalue()


class PauperLegalityTest(unittest.TestCase):
    def test_committed_list_has_no_stale_duplicate_or_mislabelled_rows(self):
        legality = legality_tool.load_legality()
        registries = legality_tool.registry_cards()
        self.assertEqual(legality["schema"], "pauper_legality/v1")
        self.assertEqual(legality_tool.registry_problems(legality, registries), [])
        known = {name for names in registries.values() for name in names}
        self.assertTrue({row["name"] for row in legality["cards"]} <= known)
        code, out, err = run_tool("check-registries")
        self.assertEqual((code, err), (0, ""))
        self.assertIn(f"{len(legality_tool.unlisted_cards(legality, registries))} registry cards without a row",
                      out)

    def test_problems_name_a_missing_duplicate_extra_and_unresolved_row(self):
        registries = {"pauper_v1": ["Bolt", "Island"], "standard": ["Cut Down"], "eternal_v1": ["Daze"]}
        rows = [
            {"name": "Bolt", "registry": "pauper_v1", "pauper": "legal"},
            {"name": "Cut Down", "registry": "standard", "pauper": "legal"},
            {"name": "Cut Down", "registry": "standard", "pauper": "legal"},
            {"name": "Daze", "registry": "standard", "pauper": "banned"},
            {"name": "Extra", "registry": "eternal_v1", "pauper": "maybe"},
        ]
        legality = {"cards": rows, "unresolved": ["Odd"]}
        self.assertEqual(legality_tool.unlisted_cards(legality, registries), ["Island"])
        self.assertNotIn("Island (pauper_v1): 0 rows", legality_tool.registry_problems(legality, registries))
        problems = legality_tool.registry_problems(legality, registries, strict=True)
        self.assertIn("Island (pauper_v1): 0 rows", problems)
        self.assertIn("Cut Down (standard): 2 rows", problems)
        self.assertIn("Daze: row registry 'standard', registry 'eternal_v1'", problems)
        self.assertIn("Extra: row without a non-token registry card", problems)
        self.assertIn("Extra: unknown status 'maybe'", problems)
        self.assertIn("Odd: unresolved", problems)

    def test_committed_list_marks_known_cards(self):
        status = {row["name"]: row["pauper"] for row in legality_tool.load_legality()["cards"]}
        self.assertEqual(status["Lightning Bolt"], "legal")
        self.assertEqual(status["Black Lotus"], "not_legal")
        self.assertEqual(status["Daze"], "banned")

    def test_summary_counts_by_registry_and_status(self):
        rows = [{"name": "A", "registry": "standard", "pauper": "legal"},
                {"name": "B", "registry": "standard", "pauper": "banned"},
                {"name": "C", "registry": "eternal_v1", "pauper": "legal"}]
        self.assertEqual(legality_tool.summary({"cards": rows}),
                         {"pauper_v1": {}, "standard": {"banned": 1, "legal": 1}, "eternal_v1": {"legal": 1}})
        code, out, _ = run_tool("summary")
        self.assertEqual(code, 0)
        self.assertIn("eternal_v1", out)

    def deck_run(self, text: str) -> tuple[int, str]:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "deck.txt"
            path.write_text(text, encoding="utf-8")
            code, out, _ = run_tool("deck", str(path))
        return code, out

    def test_deck_flags_a_not_legal_eternal_card_and_passes_basic_lands(self):
        code, out = self.deck_run("4 Black Lotus\n20 Mountain\n\n2 Daze\n")
        self.assertEqual(code, 1)
        self.assertEqual(out.splitlines(), ["Black Lotus: not_legal", "Daze: banned"])
        code, out = self.deck_run("20 Mountain\n20 Island\n\nSideboard\n4 Swamp\n")
        self.assertEqual((code, out), (0, ""))

    def test_deck_reports_a_card_missing_from_the_list(self):
        code, out = self.deck_run("4 Not A Real Card\n")
        self.assertEqual((code, out), (1, "Not A Real Card: unlisted\n"))


class PauperFieldSnapshotTest(unittest.TestCase):
    def test_snapshot_parses_and_the_census_runs_on_it(self):
        snapshot = json.loads(FIELD_SNAPSHOT.read_text(encoding="utf-8"))
        self.assertEqual(snapshot["schema"], "eternal_meta_snapshot/v1")
        self.assertEqual(snapshot["captured"], "2026-11-21")
        self.assertEqual(list(snapshot["formats"]), ["pauper"])
        decks = snapshot["formats"]["pauper"]
        self.assertEqual(len(decks), 18)
        self.assertEqual(len({deck["archetype"] for deck in decks}), 18)
        for deck in decks:
            self.assertTrue(deck["archetype"].startswith("pauper-"))
            self.assertEqual(sum(count for count, _ in deck["sideboard"]), 15, deck["archetype"])
            expected = 61 if deck["archetype"] == "pauper-golgari-jund-garden" else 60
            self.assertEqual(sum(count for count, _ in deck["mainboard"]), expected, deck["archetype"])
        report = census.census(snapshot, *census.load_status())
        pauper = report["formats"]["pauper"]
        self.assertEqual(pauper["archetypes"], 18)
        self.assertEqual(pauper["listed_share"], round(sum(deck["share"] for deck in decks), 1))
        self.assertIn("| pauper | 18 |", census.markdown(report, top=5))

    def test_snapshot_cards_with_a_legality_row_are_legal(self):
        status = {row["name"]: row["pauper"] for row in legality_tool.load_legality()["cards"]}
        snapshot = json.loads(FIELD_SNAPSHOT.read_text(encoding="utf-8"))
        for deck in snapshot["formats"]["pauper"]:
            for zone in ("mainboard", "sideboard"):
                for _, name in deck[zone]:
                    if name in status:
                        self.assertEqual(status[name], "legal", f"{deck['archetype']}: {name}")


if __name__ == "__main__":
    unittest.main()
