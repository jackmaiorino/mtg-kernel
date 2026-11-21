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

    def test_deck_accepts_export_spellings_of_legal_cards(self):
        legality = legality_tool.load_legality()
        status = {row["name"]: row["pauper"] for row in legality["cards"]}
        for line, registry_name in (("1 Fire/Ice", "Fire // Ice"),
                                    ("4 L\u00f3rien Revealed", "Lorien Revealed"),
                                    ("3 Troll of Khazad-d\u00fbm", "Troll of Khazad-dum")):
            self.assertEqual(status[registry_name], "legal", registry_name)
            self.assertEqual(self.deck_run(line + "\n"), (0, ""), line)
        self.assertEqual(self.deck_run("1 Fire/Ice\n4 L\u00f3rien Revealed\n\nSideboard\n3 Troll of Khazad-d\u00fbm\n"),
                         (0, ""))

    def test_resolve_name_order_is_exact_alias_slash_then_fold(self):
        legality = legality_tool.load_legality()
        for spelled, how in (("Fire // Ice", "exact"), ("Fire/Ice", "alias"), ("Fire / Ice", "slash"),
                             ("L\u00f3rien Revealed", "alias"), ("Lorien Revealed", "exact"),
                             ("Palant\u00edr of Orthanc", "alias"), ("Lightning Bolt", "exact"),
                             ("Lightning B\u00f3lt", "fold")):
            row, found = legality_tool.resolve_name(legality, spelled)
            self.assertIsNotNone(row, spelled)
            self.assertEqual(found, how, spelled)

    def test_deck_prints_the_registry_name_of_a_resolved_card_that_is_not_legal(self):
        legality = {"cards": [{"name": "Troll of Khazad-dum", "registry": "eternal_v1", "pauper": "not_legal"},
                              {"name": "Fire // Ice", "registry": "eternal_v1", "pauper": "banned"}],
                    "aliases": {"Fire/Ice": "Fire // Ice"}}
        text = "1 Troll of Khazad-d\u00fbm\n1 Fire/Ice\n1 Fire / Ice\n"
        self.assertEqual(legality_tool.deck_problems(legality, text),
                         [("Troll of Khazad-d\u00fbm", "not_legal", "Troll of Khazad-dum"),
                          ("Fire/Ice", "banned", "Fire // Ice"),
                          ("Fire / Ice", "banned", None)])
        path = self.write_json(legality)
        deck = Path(path).with_name("deck.txt")
        deck.write_text(text, encoding="utf-8")
        code, out, _ = run_tool("--legality", path, "deck", str(deck))
        self.assertEqual(code, 1)
        self.assertEqual(out.splitlines(), ["Troll of Khazad-d\u00fbm: not_legal (registry name: Troll of Khazad-dum)",
                                            "Fire/Ice: banned (registry name: Fire // Ice)",
                                            "Fire / Ice: banned"])

    def test_deck_keeps_unknown_and_ambiguous_names_unlisted(self):
        code, out = self.deck_run("4 Not A Real Card\n1 Fire/Ice\n")
        self.assertEqual((code, out), (1, "Not A Real Card: unlisted\n"))
        legality = {"cards": [{"name": "Cafe", "registry": "eternal_v1", "pauper": "legal"},
                              {"name": "Caf\u00e9", "registry": "eternal_v1", "pauper": "legal"},
                              {"name": "Ant // Bee", "registry": "eternal_v1", "pauper": "legal"},
                              {"name": "Ant / Bee", "registry": "eternal_v1", "pauper": "legal"}]}
        self.assertEqual(legality_tool.resolve_name(legality, "Caf\u00e8"), (None, "unlisted"))
        self.assertEqual(legality_tool.resolve_name(legality, "Caf\u00e9"), ("Caf\u00e9", "exact"))
        self.assertEqual(legality_tool.resolve_name(legality, "Ant/Bee"), (None, "unlisted"))
        self.assertEqual(legality_tool.deck_problems(legality, "1 Caf\u00e8\n"), [("Caf\u00e8", "unlisted", None)])

    def write_json(self, value: dict) -> str:
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        path = Path(directory.name) / "legality.json"
        path.write_text(json.dumps(value), encoding="utf-8")
        return str(path)

    def test_registry_problems_flag_a_dangling_alias_and_an_alias_equal_to_a_name(self):
        registries = {"pauper_v1": ["Bolt"], "standard": ["Cut Down"], "eternal_v1": []}
        rows = [{"name": "Bolt", "registry": "pauper_v1", "pauper": "legal"},
                {"name": "Cut Down", "registry": "standard", "pauper": "legal"}]
        legality = {"cards": rows,
                    "aliases": {"Lightning Bolt": "Bolt", "Bolt/Cut": "Missing", "Cut Down": "Bolt"}}
        problems = legality_tool.registry_problems(legality, registries)
        self.assertEqual(sorted(problems), ["alias Bolt/Cut: target Missing is not a row",
                                            "alias Cut Down: equals a registry name"])
        self.assertEqual(legality_tool.registry_problems({"cards": rows}, registries), [])

    def test_committed_aliases_point_at_rows_and_cover_the_known_spellings(self):
        legality = legality_tool.load_legality()
        aliases = legality["aliases"]
        rows = {row["name"] for row in legality["cards"]}
        self.assertTrue(set(aliases.values()) <= rows)
        self.assertFalse(set(aliases) & rows)
        self.assertEqual(aliases["Fire/Ice"], "Fire // Ice")
        self.assertEqual(aliases["L\u00f3rien Revealed"], "Lorien Revealed")
        self.assertEqual(aliases["Troll of Khazad-d\u00fbm"], "Troll of Khazad-dum")
        self.assertIn("aliases", legality["note"])


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
