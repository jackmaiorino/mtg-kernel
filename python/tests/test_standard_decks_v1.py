from __future__ import annotations

from pathlib import Path
import sys
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
import limited_decks_v1 as limited


STANDARD = REPO_ROOT / "data/standard/magezero_v1"
# Each Standard card batch extends this list with the deck cards it supports.
SUPPORTED_NONBASIC = {
    "Burst Lightning", "Consider", "Destroy Evil", "Dissipate", "Duress", "Fading Hope",
    "Flow of Knowledge", "Get Lost", "Hard-Hitting Question", "Impulse", "Lightning Strike",
    "Llanowar Elves", "Negate", "Opt", "Shock", "Shoot the Sheriff",
    "Spell Pierce", "Thirst for Discovery", "Tolarian Terror", "Voldaren Epicure",
}


def standard_registry() -> dict[str, limited.RegistryCard]:
    """Pauper prefix plus the Standard extension, as the opt-in Rust build appends them."""
    return limited.combined_registry(
        (REPO_ROOT / "data/cards_v1.json").read_bytes(),
        [(STANDARD / "cards_v1.json").read_bytes()],
    )


class StandardDeckTest(unittest.TestCase):
    def setUp(self) -> None:
        self.registry = standard_registry()
        self.decks = {path.stem: limited.parse_dck(path.read_text(encoding="utf-8-sig"))
                      for path in sorted((STANDARD / "decks").glob("*.dck"))}

    def test_extension_follows_the_pauper_prefix_without_fdn(self) -> None:
        self.assertEqual(self.registry["Plains"].card_id, 192)
        self.assertEqual(self.registry["Burst Lightning"].card_id, 193)
        self.assertEqual(self.registry["Get Lost"].card_id, 208)
        self.assertNotIn("Dwynen, Gilt-Leaf Daen", self.registry)

    def test_pool_is_sixteen_unsideboarded_decks(self) -> None:
        self.assertEqual(len(self.decks), 16)
        for name, deck in self.decks.items():
            copies = sum(entry.count for entry in deck.mainboard)
            self.assertEqual(copies, 62 if name == "Standard-MonoU" else 60, name)
            self.assertEqual(deck.sideboard, ())

    def test_card_names_cover_every_deck_card(self) -> None:
        names = set(limited.load_json((STANDARD / "card_names.json").read_bytes())["card_names"])
        deck_names = {entry.name for deck in self.decks.values() for entry in deck.mainboard}
        self.assertEqual(names, deck_names - set(limited.BASIC_LANDS))

    def test_supported_cards_match_the_tracked_list(self) -> None:
        names = limited.load_json((STANDARD / "card_names.json").read_bytes())["card_names"]
        report = limited.inventory(names, self.registry, list(self.decks.values()))
        full = {card["name"] for card in report["cards"] if card["status"] == "full"}
        self.assertEqual(full - set(limited.BASIC_LANDS), SUPPORTED_NONBASIC)
        self.assertTrue(set(limited.BASIC_LANDS) <= full)

    def test_unsupported_deck_is_refused(self) -> None:
        with self.assertRaisesRegex(ValueError, "unsupported mainboard"):
            limited.resolve_mainboard(self.decks["Standard-MonoR"], self.registry)

    def test_memory_deluge_partial_card_is_refused(self) -> None:
        with self.assertRaisesRegex(ValueError, "unsupported mainboard"):
            limited.resolve_mainboard(limited.parse_dck("39 Island\n1 Memory Deluge"), self.registry)


if __name__ == "__main__":
    unittest.main()
