from __future__ import annotations

from contextlib import redirect_stderr, redirect_stdout
from dataclasses import replace
import hashlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
import limited_decks_v1 as limited


FIXTURES = REPO_ROOT / "data/limited/fdn_v1"

PREPARED_FULL_NAMES = (
    "Elenda, Saint of Dusk", "Flamewake Phoenix", "Spinner of Souls",
    "Painful Quandary", "Drake Hatcher", "Bloodthirsty Conqueror", "Doubling Season",
)


def historical_records(records, count):
    """Reconstruct only previous capability metadata for frozen prefix checks."""
    historical = json.loads(json.dumps(records[:count]))
    for row in historical:
        if row["name"] in PREPARED_FULL_NAMES:
            row["engine_capability"] = "partial"
    return historical


NEXT_FIVE_NAMES = ("Midnight Snack", "Extravagant Replication", "Infernal Vessel", "Giada, Font of Hope", "Goblin Negotiation")
ALL_PARTIAL_NAMES = (*NEXT_FIVE_NAMES, "Alesha, Who Laughs at Fate", "Nine-Lives Familiar", "Valkyrie's Call", "Perforating Artist", "Niv-Mizzet, Visionary", "Embercleave", "Banner of Kinship", "Drakuseth, Maw of Flames", "Zimone, Paradox Sculptor", "Sphinx's Tutelage")


class LimitedDeckTest(unittest.TestCase):
    def setUp(self) -> None:
        self.registry = limited.fdn_registry()

    def test_tutelage_exact_spg_partial_append_and_full_deck_refusal(self) -> None:
        rows = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        row = rows[274]
        self.assertEqual((row["name"], self.registry[row["name"]].card_id,
                          row["engine_capability"], row["mana_cost"], row["mana_value"],
                          row["types"], row["colors"], row["power"], row["toughness"]),
                         ("Sphinx's Tutelage", 466, "partial", "{2}{U}", 3,
                          ["Enchantment"], ["U"], None, None))
        frozen = json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())
        printing, = [card for card in frozen["cards"] if card["name"] == row["name"]]
        self.assertEqual((printing["set"], printing["collector_number"], printing["scryfall_id"]),
                         ("SPG", "75", "94a3673b-6003-4612-a8f6-02829aa9de7d"))
        deck = limited.parse_dck((FIXTURES / "FDN_reference_sphinxs_tutelage_partial.dck").read_text())
        with self.assertRaisesRegex(ValueError, r"Sphinx's Tutelage \(partial\)"):
            limited.resolve_mainboard(deck, self.registry)

    def test_zimone_appends_exact_partial_printing_and_full_only_refuses(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        row = records[273]
        name = "Zimone, Paradox Sculptor"
        self.assertEqual((row["name"], self.registry[name].card_id,
                          row["engine_capability"], row["mana_cost"], row["mana_value"],
                          row["colors"], row["types"], row["subtypes"], row["supertypes"],
                          row["power"], row["toughness"], row["is_token"]),
                         (name, 465, "partial", "{2}{G}{U}", 4, ["G", "U"],
                          ["Creature"], ["Human", "Wizard"], ["Legendary"], 1, 4, False))
        self.assertEqual(row["java_file"],
                         "Mage.Sets/src/mage/cards/z/ZimoneParadoxSculptor.java")
        frozen = json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())
        printing, = [card for card in frozen["cards"] if card["name"] == name]
        self.assertEqual((printing["set"], printing["collector_number"]), ("FDN", "126"))
        deck = limited.parse_dck((FIXTURES / "FDN_reference_zimone_partial.dck").read_text())
        self.assertEqual(sum(card.count for card in deck.mainboard), 40)
        self.assertEqual([(card.name, card.count) for card in deck.mainboard
                          if card.name == name], [(name, 4)])
        with self.assertRaisesRegex(ValueError,
                                    r"unsupported mainboard: Zimone, Paradox Sculptor \(partial\)"):
            limited.resolve_mainboard(deck, self.registry)

    def test_drakuseth_is_actual_partial_append_and_genuine_reference_deck_refuses(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual((len(self.registry), len(records)), (467, 275))
        card = records[272]
        self.assertEqual((card["name"], card["engine_capability"]), ("Drakuseth, Maw of Flames", "partial"))
        self.assertEqual(self.registry[card["name"]].card_id, 464)
        self.assertEqual((card["mana_cost"], card["mana_value"], card["types"], card["subtypes"], card["supertypes"], card["power"], card["toughness"]),
                         ("{4}{R}{R}{R}", 7, ["Creature"], ["Dragon"], ["Legendary"], 7, 7))
        frozen = json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())
        printing = next(row for row in frozen["cards"] if row["name"] == card["name"])
        self.assertEqual((printing["set"], printing["collector_number"], printing["scryfall_id"]),
                         ("FDN", "193", "029b1edb-e1de-4f1c-81df-8d17f4920318"))
        deck = limited.parse_dck((FIXTURES / "FDN_reference_drakuseth_partial.dck").read_text())
        self.assertEqual(sum(entry.count for entry in deck.mainboard), 40)
        with self.assertRaisesRegex(ValueError, r"Drakuseth, Maw of Flames \(partial\)"):
            limited.resolve_mainboard(deck, self.registry)

    def test_alesha_appends_exact_partial_printing_preserves_prefix_and_refuses_full_deck(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual((len(self.registry), len(records)), (467, 275))
        prefix = json.dumps(records[:265], sort_keys=True, separators=(",", ":"),
                            ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(),
                         "459cc18dafa31596fc696ff5d35f8a87b409866142047b85272620dfa699645d")
        card = records[265]
        name = "Alesha, Who Laughs at Fate"
        self.assertEqual((card["name"], self.registry[name].card_id,
                          card["engine_capability"], card["mana_cost"], card["mana_value"],
                          card["types"], card["subtypes"], card["supertypes"],
                          card["power"], card["toughness"], card["is_token"]),
                         (name, 457, "partial", "{1}{B}{R}", 3, ["Creature"],
                          ["Human", "Warrior"], ["Legendary"], 2, 2, False))
        self.assertEqual(card["java_file"], "Mage.Sets/src/mage/cards/a/AleshaWhoLaughsAtFate.java")
        printing, = [row for row in json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())["cards"]
                     if row["name"] == name]
        self.assertEqual((printing["set"], printing["collector_number"], printing["scryfall_id"]),
                         ("FDN", "115", "a93e3406-4e29-4bc0-ae52-cbd2ac1f99a4"))
        self.assertEqual(limited.card_status(name, self.registry), "partial")
        deck = limited.parse_dck((FIXTURES / "FDN_reference_alesha_partial.dck").read_text())
        self.assertEqual(sum(item.count for item in deck.mainboard), 40)
        self.assertEqual([(item.name, item.count) for item in deck.mainboard if item.name == name],
                         [(name, 4)])
        self.assertEqual(sum(item.count for item in deck.mainboard if item.name in {"Swamp", "Mountain"}),
                         32)
        with self.assertRaisesRegex(ValueError, r"unsupported mainboard: Alesha, Who Laughs at Fate \(partial\)"):
            limited.resolve_mainboard(deck, self.registry)

    def test_perforating_artist_appends_exact_partial_printing_and_refuses_full_deck(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual((len(self.registry), len(records)), (467, 275))
        prefix = json.dumps(records[:268], sort_keys=True, separators=(",", ":"),
                            ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(), "03d02956fd7906368cf5abd0220bc8831ceac1b97842efd39ae2d8d1169e6119")
        card = records[268]
        name = "Perforating Artist"
        self.assertEqual((card["name"], self.registry[name].card_id,
                          card["engine_capability"], card["mana_cost"], card["mana_value"],
                          card["types"], card["subtypes"], card["colors"],
                          card["power"], card["toughness"], card["is_token"]),
                         (name, 460, "partial", "{1}{B}{R}", 3, ["Creature"],
                          ["Devil"], ["B", "R"], 3, 2, False))
        printing, = [row for row in json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())["cards"]
                     if row["name"] == name]
        self.assertEqual((printing["set"], printing["collector_number"], printing["scryfall_id"]),
                         ("FDN", "124", "72980409-53f0-43c1-965e-06f22e7bb608"))
        self.assertEqual(card["java_file"], "Mage.Sets/src/mage/cards/p/PerforatingArtist.java")
        self.assertEqual(limited.card_status(name, self.registry), "partial")
        deck = limited.parse_dck((FIXTURES / "FDN_reference_perforating_artist_partial.dck").read_text())
        self.assertEqual(sum(item.count for item in deck.mainboard), 40)
        self.assertEqual([(item.name, item.count) for item in deck.mainboard if item.name == name], [(name, 4)])
        with self.assertRaisesRegex(ValueError, r"unsupported mainboard: Perforating Artist \(partial\)"):
            limited.resolve_mainboard(deck, self.registry)

    def assert_current_full_and_historical_partial_refusal(self, deck):
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(resolved), 40)
        candidates = {entry.name for entry in deck.mainboard} & set(PREPARED_FULL_NAMES)
        self.assertTrue(candidates)
        for name in candidates:
            historical = dict(self.registry)
            historical[name] = replace(historical[name], capability="partial")
            with self.assertRaisesRegex(ValueError, "unsupported mainboard"):
                limited.resolve_mainboard(deck, historical)

    def test_nine_lives_appends_exact_partial_printing_and_refuses_full_deck(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual((len(self.registry), len(records)), (467, 275))
        prefix = json.dumps(records[:266], sort_keys=True, separators=(",", ":"),
                            ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(),
                         "a57f18e629dc2ea3bfcb998fc8036d1c06cc6868f3bbc699a5c79a85015dd5bd")
        card = records[266]
        name = "Nine-Lives Familiar"
        self.assertEqual((card["name"], self.registry[name].card_id,
                          card["engine_capability"], card["mana_cost"], card["mana_value"],
                          card["types"], card["subtypes"], card["power"], card["toughness"],
                          card["is_token"]),
                         (name, 458, "partial", "{1}{B}{B}", 3, ["Creature"], ["Cat"], 1, 1, False))
        printing, = [row for row in json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())["cards"]
                     if row["name"] == name]
        self.assertEqual((printing["set"], printing["collector_number"], printing["scryfall_id"]),
                         ("FDN", "66", "988c23f6-59fe-49f9-a9ce-9881dccb7033"))
        deck = limited.parse_dck((FIXTURES / "FDN_reference_nine_lives_familiar_partial.dck").read_text())
        self.assertEqual(sum(item.count for item in deck.mainboard), 40)
        self.assertEqual([(item.name, item.count) for item in deck.mainboard if item.name == name],
                         [(name, 4)])
        with self.assertRaisesRegex(ValueError, r"unsupported mainboard: Nine-Lives Familiar \(partial\)"):
            limited.resolve_mainboard(deck, self.registry)

    def test_valkyries_call_appends_exact_partial_printing_and_refuses_full_deck(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual((len(self.registry), len(records)), (467, 275))
        prefix = json.dumps(records[:267], sort_keys=True, separators=(",", ":"),
                            ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(),
                         "b4c06d92326090d643c371ac7f45724e470f71fde09fda502bb1b3ef771fbbdd")
        card = records[267]
        name = "Valkyrie's Call"
        self.assertEqual((card["name"], self.registry[name].card_id,
                          card["engine_capability"], card["mana_cost"], card["mana_value"],
                          card["types"], card["subtypes"], card["power"], card["toughness"],
                          card["is_token"]),
                         (name, 459, "partial", "{3}{W}{W}", 5, ["Enchantment"], [],
                          None, None, False))
        printing, = [row for row in json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())["cards"]
                     if row["name"] == name]
        self.assertEqual((printing["set"], printing["collector_number"], printing["scryfall_id"]),
                         ("FDN", "27", "0e1f1ff2-fa8f-4d38-b631-2d6e08e614c8"))
        self.assertEqual(limited.card_status(name, self.registry), "partial")
        deck = limited.parse_dck((FIXTURES / "FDN_reference_valkyries_call_partial.dck").read_text())
        self.assertEqual(sum(item.count for item in deck.mainboard), 40)
        self.assertEqual([(item.name, item.count) for item in deck.mainboard if item.name == name],
                         [(name, 4)])
        with self.assertRaisesRegex(ValueError, r"unsupported mainboard: Valkyrie's Call \(partial\)"):
            limited.resolve_mainboard(deck, self.registry)

    def test_niv_mizzet_appends_exact_partial_printing_and_refuses_full_deck(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual((len(self.registry), len(records)), (467, 275))
        card = records[269]
        name = "Niv-Mizzet, Visionary"
        self.assertEqual((card["name"], self.registry[name].card_id,
                          card["engine_capability"], card["mana_cost"], card["mana_value"],
                          card["types"], card["subtypes"], card["supertypes"],
                          card["power"], card["toughness"], card["is_token"]),
                         (name, 461, "partial", "{4}{U}{R}", 6, ["Creature"],
                          ["Dragon", "Wizard"], ["Legendary"], 5, 5, False))
        self.assertEqual(card["java_file"], "Mage.Sets/src/mage/cards/n/NivMizzetVisionary.java")
        printing, = [row for row in json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())["cards"]
                     if row["name"] == name]
        self.assertEqual((printing["set"], printing["collector_number"], printing["scryfall_id"]),
                         ("FDN", "123", "7a69a618-d588-4745-8ede-0ff0a9f356f1"))
        self.assertEqual(limited.card_status(name, self.registry), "partial")
        deck = limited.parse_dck((FIXTURES / "FDN_reference_niv_mizzet_visionary_partial.dck").read_text())
        self.assertEqual(sum(item.count for item in deck.mainboard), 40)
        self.assertEqual([(item.name, item.count) for item in deck.mainboard if item.name == name],
                         [(name, 4)])
        self.assertTrue(all(limited.card_status(item.name, self.registry) == "full"
                            for item in deck.mainboard if item.name != name))
        with self.assertRaisesRegex(ValueError, r"unsupported mainboard: Niv-Mizzet, Visionary \(partial\)"):
            limited.resolve_mainboard(deck, self.registry)

    def test_embercleave_appends_exact_guest_partial_printing_and_refuses_full_deck(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual((len(self.registry), len(records)), (467, 275))
        card = records[270]
        name = "Embercleave"
        self.assertEqual((card["name"], self.registry[name].card_id,
                          card["engine_capability"], card["mana_cost"], card["mana_value"],
                          card["colors"], card["types"], card["subtypes"], card["supertypes"],
                          card["power"], card["toughness"], card["is_token"]),
                         (name, 462, "partial", "{4}{R}{R}", 6, ["R"], ["Artifact"],
                          ["Equipment"], ["Legendary"], None, None, False))
        self.assertEqual(card["java_file"], "Mage.Sets/src/mage/cards/e/Embercleave.java")
        printing, = [row for row in json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())["cards"]
                     if row["name"] == name]
        self.assertEqual((printing["set"], printing["collector_number"], printing["scryfall_id"],
                          printing["oracle_id"]),
                         ("SPG", "77", "32bee083-859e-407c-99ad-3fb74b1d4233",
                          "4d6120d6-fcce-40bc-9fc6-e1f5beb6c728"))
        self.assertEqual(limited.card_status(name, self.registry), "partial")
        deck = limited.parse_dck((FIXTURES / "FDN_reference_embercleave_partial.dck").read_text())
        self.assertEqual(sum(item.count for item in deck.mainboard), 40)
        self.assertEqual([(item.name, item.count) for item in deck.mainboard if item.name == name],
                         [(name, 4)])
        self.assertTrue(all(limited.card_status(item.name, self.registry) == "full"
                            for item in deck.mainboard if item.name != name))
        with self.assertRaisesRegex(ValueError, r"unsupported mainboard: Embercleave \(partial\)"):
            limited.resolve_mainboard(deck, self.registry)

    def test_banner_of_kinship_exact_frozen_partial_printing_and_full_only_refusal(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual((len(self.registry), len(records)), (467, 275))
        prefix = json.dumps(records[:271], ensure_ascii=False, sort_keys=True,
                            separators=(",", ":")).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(), "58ea10700676a3fbb3e0a4335e5fe05f1f13c469943e53297936bb6d701a751e")
        card = records[271]
        name = "Banner of Kinship"
        self.assertEqual((card["name"], self.registry[name].card_id, card["engine_capability"],
                          card["mana_cost"], card["mana_value"], card["types"], card["subtypes"],
                          card["supertypes"], card["colors"], card["power"], card["toughness"]),
                         (name, 463, "partial", "{5}", 5, ["Artifact"], [], [], [], None, None))
        self.assertEqual(card["java_file"], "Mage.Sets/src/mage/cards/b/BannerOfKinship.java")
        printing, = [r for r in json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())["cards"]
                     if r["name"] == name]
        self.assertEqual((printing["set"], printing["collector_number"], printing["scryfall_id"],
                          printing["oracle_id"]),
                         ("FDN", "127", "a14c16c0-4053-46b0-8fa6-be8b4a7a1c8a",
                          "8c220dbd-6572-4715-aae6-dd09a4252d68"))
        deck = limited.parse_dck((FIXTURES / "FDN_reference_banner_of_kinship_partial.dck").read_text())
        self.assertEqual(sum(item.count for item in deck.mainboard), 40)
        self.assertEqual([(item.name, item.count) for item in deck.mainboard if item.name == name], [(name, 4)])
        self.assertTrue(all(limited.card_status(item.name, self.registry) == "full"
                            for item in deck.mainboard if item.name != name))
        with self.assertRaisesRegex(ValueError, r"unsupported mainboard: Banner of Kinship \(partial\)"):
            limited.resolve_mainboard(deck, self.registry)

    def test_midnight_snack_exact_printing_reuses_food_and_refuses_full_only_decks(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual((len(self.registry), len(records)), (467, 275))
        card = records[260]
        self.assertEqual((card["name"], self.registry[card["name"]].card_id,
                          card["engine_capability"], card["mana_cost"], card["mana_value"],
                          card["types"], card["colors"], card["power"], card["toughness"]),
                         ("Midnight Snack", 452, "partial", "{2}{B}", 3,
                          ["Enchantment"], ["B"], None, None))
        metadata = json.loads((FIXTURES / "midnight_snack_prepared_v1.json").read_bytes())
        self.assertEqual((metadata["printing"]["set"], metadata["printing"]["collector_number"]), ("FDN", "65"))
        self.assertEqual(metadata["mage_commit"], "a5c90fe180021e70e2a644ade00eeab07f857a40")
        self.assertFalse(metadata["model_admitted"])
        self.assertFalse(metadata["runtime_qualified"])
        self.assertEqual(len(metadata["primary_sources"]), 4)
        base = json.loads((REPO_ROOT / "data/cards_v1.json").read_bytes())["cards"]
        food, = [row for row in base + records[:260] if row["name"] == "Food Token"]
        self.assertTrue(food["is_token"])
        self.assertEqual(food["engine_capability"], "full")
        deck = limited.parse_dck((FIXTURES / "FDN_reference_midnight_snack_partial.dck").read_text())
        self.assertEqual(sum(item.count for item in deck.mainboard), 40)
        self.assertTrue(all(limited.card_status(item.name, self.registry) == "full"
                            for item in deck.mainboard if item.name != "Midnight Snack"))
        with self.assertRaisesRegex(ValueError, "unsupported mainboard"):
            limited.resolve_mainboard(deck, self.registry)

    def test_conqueror_season_are_genuine_partial_appendices_with_full_only_deck_refusal(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        historical = historical_records(records, 258)
        historical[250]["engine_capability"] = "partial"
        historical[251]["engine_capability"] = "partial"
        prefix = json.dumps(historical, sort_keys=True, separators=(",", ":"),
                            ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(), "7ed089181b25bce1e63c6db4de9d1cdc60811ce76314b0a5f8ae89123c842f69")
        self.assertEqual((len(self.registry), len(records)), (467, 275))
        frozen = json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())
        self.assertEqual((len(frozen["cards"]), len(frozen["token_dependencies"])), (286, 24))
        for name, card_id, cost, types, pt, collector, slug in [
                ("Bloodthirsty Conqueror", 450, "{3}{B}{B}", ["Creature"], (5, 5), "58", "bloodthirsty_conqueror"),
                ("Doubling Season", 451, "{4}{G}", ["Enchantment"], (None, None), "216", "doubling_season")]:
            with self.subTest(name=name):
                row = records[card_id - 192]
                self.assertEqual(row["name"], name)
                self.assertEqual(self.registry[name].card_id, card_id)
                self.assertEqual((row["engine_capability"], row["mana_cost"], row["mana_value"], row["types"],
                                  row["power"], row["toughness"], row["is_token"]),
                                 ("full", cost, 5, types, *pt, False))
                printing, = [item for item in frozen["cards"] if item["name"] == name]
                self.assertEqual((printing["set"], printing["collector_number"]), ("FDN", collector))
                path = FIXTURES / f"FDN_reference_{slug}_partial.dck"
                self.assertIn(path.name, row["decks"])
                deck = limited.parse_dck(path.read_text(encoding="utf-8"))
                self.assertEqual(sum(item.count for item in deck.mainboard), 40)
                self.assertEqual([(item.name, item.count) for item in deck.mainboard if item.name == name], [(name, 4)])
                self.assertTrue(all(limited.card_status(item.name, self.registry) == "full"
                                    for item in deck.mainboard if item.name != name))
                self.assert_current_full_and_historical_partial_refusal(deck)

    def test_drake_hatcher_appends_genuine_partial_printing_and_exact_token(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        historical = historical_records(records, 252)
        historical[250]["engine_capability"] = "partial"
        historical[251]["engine_capability"] = "partial"
        prefix = json.dumps(historical, sort_keys=True, separators=(",", ":"),
                            ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(),
                         "da28318f5d9ca002b95ab06f83e265824c61982c61609a0b3100fc7a08c97b10")
        self.assertEqual((len(self.registry), len(records)), (467, 275))
        self.assertEqual([(row["name"], row["engine_capability"]) for row in records
                          if row["engine_capability"] != "full"],
                         [(name, "partial") for name in ALL_PARTIAL_NAMES])
        card, token = records[256:258]
        self.assertEqual((card["name"], self.registry[card["name"]].card_id), ("Drake Hatcher", 448))
        self.assertEqual((card["mana_cost"], card["mana_value"], card["power"], card["toughness"],
                          card["colors"], card["types"], card["subtypes"], card["is_token"]),
                         ("{1}{U}", 2, 1, 3, ["U"], ["Creature"], ["Human", "Wizard"], False))
        self.assertEqual(card["java_file"], "Mage.Sets/src/mage/cards/d/DrakeHatcher.java")
        self.assertEqual(card["mechanics"], ["vigilance", "prowess", "combat_damage_incubation_counters",
                                            "remove_source_counters_cost"])
        self.assertEqual(limited.card_status(card["name"], self.registry), "full")
        deck_path = FIXTURES / "FDN_reference_drake_hatcher_partial.dck"
        self.assertIn(deck_path.name, card["decks"])
        deck = limited.parse_dck(deck_path.read_text())
        self.assertEqual(sum(entry.count for entry in deck.mainboard), 40)
        self.assertEqual([(entry.name, entry.count) for entry in deck.mainboard
                          if entry.name == card["name"]], [("Drake Hatcher", 4)])
        self.assertTrue(all(limited.card_status(entry.name, self.registry) == "full"
                            for entry in deck.mainboard if entry.name != card["name"]))
        self.assert_current_full_and_historical_partial_refusal(deck)
        self.assertEqual((token["name"], self.registry[token["name"]].card_id), ("Drake Token", 449))
        self.assertEqual((token["engine_capability"], token["mana_cost"], token["mana_value"],
                          token["power"], token["toughness"], token["colors"], token["types"],
                          token["subtypes"], token["mechanics"], token["is_token"]),
                         ("full", "", 0, 2, 2, ["U"], ["Creature"], ["Drake"], ["flying"], True))
        self.assertEqual(token["java_file"], "Mage/src/main/java/mage/game/permanent/token/DrakeToken.java")
        self.assertEqual(limited.card_status(token["name"], self.registry), "token")
        with self.assertRaisesRegex(ValueError, "token"):
            limited.resolve_mainboard(limited.parse_dck("39 Island\n1 Drake Token\n"), self.registry)
        frozen = json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())
        self.assertEqual((len(frozen["cards"]), len(frozen["token_dependencies"])), (286, 24))
        self.assertEqual([row for row in frozen["cards"] if row["name"] == card["name"]], [{
            "name": "Drake Hatcher", "set": "FDN", "collector_number": "35",
            "scryfall_id": "bcaf4196-6bf3-47fa-b5c7-0e77f45cf820",
            "oracle_id": "4de0ccab-bb5d-4c7c-827e-36aa1d00b182", "rarity": "rare"}])
        self.assertEqual([edge for edge in frozen["token_dependencies"]
                          if card["name"] in edge["required_by"]], [{
            "id": "f4a73034-e20f-4e7e-ac15-3460b1e9c69b", "name": "Drake",
            "type_line": "Token Creature — Drake", "required_by": ["Drake Hatcher"]}])

    def test_quandary_partial_printing_preserves_prefix_and_refuses_full_deck_resolution(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        historical = historical_records(records, 252)
        historical[250]["engine_capability"] = "partial"
        historical[251]["engine_capability"] = "partial"
        prefix = json.dumps(historical, sort_keys=True, separators=(",", ":"),
                            ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(),
                         "da28318f5d9ca002b95ab06f83e265824c61982c61609a0b3100fc7a08c97b10")
        self.assertEqual((len(self.registry), len(records)), (467, 275))
        partial = {name: card.card_id for name, card in self.registry.items()
                   if card.capability == "partial"}
        self.assertEqual(partial, {name: 452 + i for i, name in enumerate(ALL_PARTIAL_NAMES)})
        card = records[255]
        self.assertEqual(card["name"], "Painful Quandary")
        self.assertEqual((card["engine_capability"], card["mana_cost"], card["mana_value"],
                          card["colors"], card["types"]),
                         ("full", "{3}{B}{B}", 5, ["B"], ["Enchantment"]))
        frozen = json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())
        printing, = [row for row in frozen["cards"] if row["name"] == card["name"]]
        self.assertEqual((printing["set"], printing["collector_number"], printing["scryfall_id"]),
                         ("FDN", "179", "05757669-e8a6-4a2f-8479-d4e2ade822ca"))
        self.assertEqual(limited.card_status(card["name"], self.registry), "full")
        deck = limited.parse_dck((FIXTURES / "FDN_reference_painful_quandary_partial.dck").read_text())
        self.assertEqual(sum(entry.count for entry in deck.mainboard), 40)
        self.assertEqual(deck.mainboard[0].name, card["name"])
        self.assertEqual(deck.mainboard[0].count, 4)
        self.assert_current_full_and_historical_partial_refusal(deck)

    def test_spinner_partial_printing_preserves_parent_prefix_and_refuses_full_deck(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        historical = historical_records(records, 252)
        historical[250]["engine_capability"] = "partial"
        historical[251]["engine_capability"] = "partial"
        prefix = json.dumps(historical, sort_keys=True, separators=(",", ":"),
                            ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(),
                         "da28318f5d9ca002b95ab06f83e265824c61982c61609a0b3100fc7a08c97b10")
        self.assertEqual((len(self.registry), len(records)), (467, 275))
        self.assertEqual(records[254]["name"], "Spinner of Souls")
        self.assertEqual(self.registry["Spinner of Souls"].card_id, 446)
        self.assertEqual(limited.card_status("Spinner of Souls", self.registry), "full")
        self.assertEqual(records[254]["other_controlled_nontoken_death_v1"],
                         "may_reveal_creature_to_hand_random_bottom")
        self.assertEqual(records[254]["intrinsic_keywords_v1"], ["reach"])
        self.assertEqual([(name, card.card_id) for name, card in self.registry.items()
                          if card.capability == "partial"],
                         [(name, 452 + i) for i, name in enumerate(ALL_PARTIAL_NAMES)])
        deck = limited.parse_dck((FIXTURES / "FDN_reference_spinner_of_souls_partial.dck")
                                 .read_text(encoding="utf-8"))
        self.assertEqual(sum(entry.count for entry in deck.mainboard), 40)
        self.assert_current_full_and_historical_partial_refusal(deck)

    def test_elenda_composed_partial_uses_next_id_and_refuses_full_deck_resolution(self) -> None:
        self.assertEqual(self.registry["Elenda, Saint of Dusk"].card_id, 444)
        self.assertEqual(limited.card_status("Elenda, Saint of Dusk", self.registry), "full")
        deck = limited.parse_dck((FIXTURES / "FDN_reference_elenda_partial.dck").read_text())
        self.assertEqual(sum(entry.count for entry in deck.mainboard), 40)
        self.assert_current_full_and_historical_partial_refusal(deck)

    def test_flamewake_partial_printing_preserves_prefix_and_refuses_full_deck_resolution(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        historical = historical_records(records, 252)
        historical[250]["engine_capability"] = "partial"
        historical[251]["engine_capability"] = "partial"
        prefix = json.dumps(historical, sort_keys=True, separators=(",", ":"),
                            ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(),
                         "da28318f5d9ca002b95ab06f83e265824c61982c61609a0b3100fc7a08c97b10")
        self.assertEqual(len(records), 275)
        printing = self.registry["Flamewake Phoenix"]
        self.assertEqual(printing.card_id, 445)
        self.assertEqual(limited.card_status("Flamewake Phoenix", self.registry), "full")
        deck = limited.parse_dck((FIXTURES / "FDN_reference_flamewake_phoenix_partial.dck").read_text())
        self.assertEqual(sum(entry.count for entry in deck.mainboard), 40)
        self.assert_current_full_and_historical_partial_refusal(deck)

    def test_ravenous_engine_full_preserves_prefix_and_resolves_reference_deck(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        historical = historical_records(records, 251)
        historical[250]["engine_capability"] = "partial"
        prefix = json.dumps(historical, sort_keys=True, separators=(",", ":"),
                            ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(), "d5bc58f4d8afe65ea7add52d000ea127e66d695779c5250e75cdd64b0ccae6af")
        self.assertEqual(len(records), 275)
        self.assertEqual(self.registry["Ravenous Amulet"].card_id, 443)
        self.assertEqual(self.registry["Fake Your Own Death"].card_id, 442)
        self.assertEqual(limited.card_status("Ravenous Amulet", self.registry), "full")
        self.assertEqual(limited.card_status("Fake Your Own Death", self.registry), "full")
        deck = limited.parse_dck((FIXTURES / "FDN_reference_ravenous_amulet_partial.dck").read_text())
        self.assertEqual(sum(entry.count for entry in deck.mainboard), 40)
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(resolved), 40)
        self.assertEqual(resolved.count(443), 4)

    def test_fake_engine_full_preserves_prior_registry_and_resolves_reference_deck(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        prefix = json.dumps(records[:250], sort_keys=True, separators=(",", ":"),
                            ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(),
                         "06ac37d8b170426de4a6248a3d770b5e912cb816cdea986155e2c3e30fde83b0")
        self.assertEqual(self.registry["Fake Your Own Death"].card_id, 442)
        self.assertEqual(limited.card_status("Fake Your Own Death", self.registry), "full")
        deck = limited.parse_dck((FIXTURES / "FDN_reference_fake_your_own_death_partial.dck").read_text())
        self.assertEqual(sum(entry.count for entry in deck.mainboard), 40)
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(resolved), 40)
        self.assertEqual(resolved.count(442), 4)
        self.assertEqual([(row["name"], row["engine_capability"]) for row in records
                          if row["engine_capability"] != "full"],
                         [(name, "partial") for name in ALL_PARTIAL_NAMES])

    def test_extravagant_replication_is_genuine_partial_suffix_and_full_only_deck_refuses(self) -> None:
        rows=json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual((len(self.registry),len(rows)),(467,275))
        prefix=json.dumps(historical_records(rows, 260),sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(),"47c04b86234154aca529aa0b7e59fccf2c7487989c557ea5972c95e000f99a46")
        row=rows[261]
        self.assertEqual((row["name"],row["engine_capability"],self.registry[row["name"]].card_id),("Extravagant Replication","partial",453))
        self.assertEqual((row["mana_cost"],row["mana_value"],row["types"],row["colors"]),("{4}{U}{U}",6,["Enchantment"],["U"]))
        deck=limited.parse_dck((FIXTURES / "FDN_reference_extravagant_replication_partial.dck").read_text())
        self.assertEqual(sum(entry.count for entry in deck.mainboard),40)
        with self.assertRaisesRegex(ValueError,r"Extravagant Replication \(partial\)"):
            limited.resolve_mainboard(deck,self.registry)

    def test_infernal_vessel_appends_after_enchantments_and_refuses_full_only_decks(self) -> None:
        rows = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual([(row["name"], self.registry[row["name"]].card_id)
                          for row in rows[260:263]],
                         [("Midnight Snack", 452), ("Extravagant Replication", 453),
                          ("Infernal Vessel", 454)])
        self.assertEqual(limited.card_status("Infernal Vessel", self.registry), "partial")
        deck = limited.parse_dck("4 [FDN:63] Infernal Vessel\n36 Swamp\n")
        report = limited.inspect_deck(deck, self.registry)
        self.assertEqual(deck.mainboard[0].printing, "FDN:63")
        self.assertEqual(report["mainboard"]["copies"], 40)
        self.assertEqual(report["mainboard"]["supported_copies"], 36)
        self.assertFalse(report["mainboard"]["registry_ready"])
        with self.assertRaisesRegex(ValueError, r"unsupported mainboard: Infernal Vessel \(partial\)"):
            limited.resolve_mainboard(deck, self.registry)

    def test_giada_appends_partial_angel_and_reference_deck_refuses_full_resolution(self) -> None:
        rows = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual(rows[262]["name"], "Infernal Vessel")
        self.assertEqual((rows[263]["name"], self.registry[rows[263]["name"]].card_id),
                         ("Giada, Font of Hope", 455))
        self.assertEqual(limited.card_status("Giada, Font of Hope", self.registry), "partial")
        deck = limited.parse_dck((FIXTURES / "FDN_reference_giada_partial.dck").read_text(encoding="utf-8"))
        report = limited.inspect_deck(deck, self.registry)
        self.assertEqual(deck.mainboard[0].printing, "FDN:141")
        self.assertEqual(report["mainboard"]["copies"], 40)
        self.assertEqual(report["mainboard"]["supported_copies"], 36)
        self.assertFalse(report["mainboard"]["registry_ready"])
        with self.assertRaisesRegex(ValueError, r"unsupported mainboard: Giada, Font of Hope \(partial\)"):
            limited.resolve_mainboard(deck, self.registry)

    def test_negotiation_appends_after_giada_and_full_only_reference_deck_refuses(self) -> None:
        rows = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual([(row["name"], self.registry[row["name"]].card_id)
                          for row in rows[263:265]],
                         [("Giada, Font of Hope", 455), ("Goblin Negotiation", 456)])
        self.assertEqual(limited.card_status("Goblin Negotiation", self.registry), "partial")
        deck = limited.parse_dck((FIXTURES / "FDN_reference_goblin_negotiation_partial.dck").read_text(encoding="utf-8"))
        report = limited.inspect_deck(deck, self.registry)
        self.assertEqual(deck.mainboard[0].printing, "FDN:88")
        self.assertEqual(report["mainboard"]["copies"], 40)
        self.assertEqual(report["mainboard"]["supported_copies"], 36)
        self.assertFalse(report["mainboard"]["registry_ready"])
        with self.assertRaisesRegex(ValueError, r"unsupported mainboard: Goblin Negotiation \(partial\)"):
            limited.resolve_mainboard(deck, self.registry)

    def test_registry_extension_keeps_all_original_ids_and_resolves_the_new_batch(self) -> None:
        cards = limited.load_json((REPO_ROOT / "data/cards_v1.json").read_bytes())["cards"]
        original_names = json.dumps([card["name"] for card in cards[:162]],
                                    ensure_ascii=False, separators=(",", ":")).encode()
        self.assertEqual(hashlib.sha256(original_names).hexdigest(),
                         "396e3458a989e9b40ca59dabb389bda9dd14b9fa367a871ee84129f292e0c156")
        names = ["Plains", "Healer's Hawk", "Fleeting Distraction", "Cathar Commando",
                 "Spectral Sailor", "Treetop Snarespinner"]
        deck = limited.parse_dck("\n".join(f"10 {name}" for name in names))
        self.assertEqual(limited.resolve_mainboard(deck, self.registry),
                         [self.registry[name].card_id for name in names for _ in range(10)])

    def test_dwynen_resolves_with_the_resumable_legend_rule(self) -> None:
        deck = limited.parse_dck("38 Forest\n2 Dwynen, Gilt-Leaf Daen\n")
        self.assertEqual(limited.card_status("Dwynen, Gilt-Leaf Daen", self.registry), "full")
        self.assertEqual(limited.resolve_mainboard(deck, self.registry),
                         [self.registry["Forest"].card_id] * 38
                         + [self.registry["Dwynen, Gilt-Leaf Daen"].card_id] * 2)

    def test_reference_planeswalker_full_candidate_preserves_id_and_resolves(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_planeswalker.dck").read_text())
        self.assertEqual(limited.card_status("Ajani, Caller of the Pride", self.registry), "full")
        self.assertEqual(len(limited.resolve_mainboard(deck, self.registry)), 40)
        self.assertEqual(self.registry["Ajani, Caller of the Pride"].card_id, 215)

    def test_hunter_eaten_panda_candidate_preserves_prior_registry_and_resolves_real_decks(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        prefix = json.dumps(records[:247], sort_keys=True, separators=(",", ":"),
                            ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(),
                         "106363f2efffdc1206de1a1f3e95b539297bd355ad078cba5a92f6f2c13a9e9e")
        candidates = [("High-Society Hunter", 439, "hunter"),
                      ("Eaten Alive", 440, "eaten_alive"),
                      ("Fiendish Panda", 441, "panda")]
        self.assertEqual([row["name"] for row in records[247:250]],
                         [name for name, _, _ in candidates])
        for name, card_id, slug in candidates:
            with self.subTest(name=name):
                self.assertEqual(self.registry[name].card_id, card_id)
                deck = limited.parse_dck((FIXTURES / f"FDN_reference_{slug}.dck").read_text())
                ids = limited.resolve_mainboard(deck, self.registry)
                self.assertEqual(len(ids), 40)
                self.assertEqual(ids.count(card_id), 4)
                self.assertNotIn(437, ids)

    def test_full_ajani_candidate_preserves_prior_prefix_and_distinct_token_dependency(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual(len(self.registry), 467)
        self.assertEqual(len(records), 275)
        original = json.loads(json.dumps(records[:245]))
        ajani = original[215 - 192]
        self.assertEqual(ajani["name"], "Ajani, Caller of the Pride")
        self.assertEqual(ajani["engine_capability"], "full")
        self.assertEqual(ajani["starting_loyalty"], 4)
        self.assertEqual(ajani["mana_cost"], "{1}{W}{W}")
        ajani["engine_capability"] = "partial"
        ajani["mechanics"] = ["loyalty_reference_only"]
        ajani["decks"].remove("FDN_reference_ajani_full.dck")
        encoded = json.dumps(original, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(encoded).hexdigest(),
                         "729b7c85a9faf915f73681db3e1b6917ce7b92397feaf310a52943d454165ad8")
        token = records[245]
        self.assertEqual(self.registry[token["name"]].card_id, 437)
        self.assertEqual(token["name"], "Ajani Cat Token")
        self.assertEqual(token["object_name"], "Cat Token")
        self.assertEqual((token["power"], token["toughness"], token["colors"], token["subtypes"]),
                         (2, 2, ["W"], ["Cat"]))
        self.assertTrue(token["is_token"])
        old_cat = next(row for row in records if row["name"] == "Cat Token")
        self.assertEqual((old_cat["power"], old_cat["toughness"]), (1, 1))
        with self.assertRaisesRegex(ValueError, "token"):
            limited.resolve_mainboard(limited.parse_dck("39 Plains" + chr(10) + "1 Ajani Cat Token"), self.registry)
        frozen = json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())
        self.assertEqual(len(frozen["card_names"]), 286)
        self.assertEqual(len(frozen["token_dependencies"]), 24)
        self.assertEqual([edge["name"] for edge in frozen["token_dependencies"]
                          if "Ajani, Caller of the Pride" in edge["required_by"]], ["Cat"])
        self.assertNotIn(token["name"], frozen["card_names"])
        for relative, expected in [
            ("data/cards_v1.json", "ef738001c373076039f211582f5fd07b11903788e70641ec7e23c84ba4f69b14"),
            ("data/standard/magezero_v1/cards_v1.json", "577a51fd8f0490a74ffeb49ac1e39d6675955fc53960a7b40a21e684057d9f37"),
            ("data/limited/fdn_v1/booster_pool_v1.json", "3cf2926c75797fb164489128b715da4264793b9b7ca4ad149fef63516ab89155"),
            ("data/limited/fdn_v1/card_names.json", "5d63a3ec34306202897eb413a56dff78d07e55960459a5b5c2934cf3a2db315d"),
        ]:
            self.assertEqual(hashlib.sha256((REPO_ROOT / relative).read_bytes()).hexdigest(), expected)
        deck = limited.parse_dck((FIXTURES / "FDN_reference_ajani_full.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:4], [215] * 4)
        for name in ["Savannah Lions", "Helpful Hunter", "Sun-Blessed Healer", "Banishing Light"]:
            self.assertEqual(ids.count(self.registry[name].card_id), 4)
        self.assertEqual(ids.count(self.registry["Plains"].card_id), 20)
        self.assertNotIn(437, ids)

    def test_faebloom_candidate_appends_exact_printing_and_reuses_existing_faerie(self) -> None:
        records = json.loads((FIXTURES / "cards_v1.json").read_bytes())["cards"]
        self.assertEqual(len(records), 275)
        self.assertEqual(len(self.registry), 467)
        prefix = json.dumps(records[:246], sort_keys=True, separators=(",", ":"),
                            ensure_ascii=False).encode()
        self.assertEqual(hashlib.sha256(prefix).hexdigest(),
                         "8bbb2bb37734dc2d149d852385ce91fd4fcf5ed2234881662610fe242bf73d6c")
        card = records[246]
        self.assertEqual(card["name"], "Faebloom Trick")
        self.assertEqual(self.registry[card["name"]].card_id, 438)
        self.assertEqual(card["engine_capability"], "full")
        self.assertEqual((card["mana_cost"], card["mana_value"], card["colors"], card["types"]),
                         ("{2}{U}", 3, ["U"], ["Instant"]))
        token = next(row for row in records if row["name"] == "Faerie Token")
        self.assertEqual((token["power"], token["toughness"], token["colors"], token["subtypes"]),
                         (1, 1, ["U"], ["Faerie"]))
        self.assertIn("flying", token["mechanics"])
        self.assertTrue(token["is_token"])
        frozen = json.loads((FIXTURES / "booster_pool_v1.json").read_bytes())
        self.assertEqual((len(frozen["card_names"]), len(frozen["token_dependencies"])), (286, 24))
        self.assertIn(card["name"], frozen["card_names"])
        self.assertEqual([edge["name"] for edge in frozen["token_dependencies"]
                          if card["name"] in edge["required_by"]], ["Faerie"])
        deck = limited.parse_dck((FIXTURES / "FDN_reference_faebloom_trick.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:4], [438] * 4)
        self.assertEqual(ids.count(self.registry["Island"].card_id), 20)
        for name in ["Spectral Sailor", "Mocking Sprite", "Fleeting Distraction", "Inspiration from Beyond"]:
            self.assertEqual(ids.count(self.registry[name].card_id), 4)
        self.assertNotIn(self.registry["Faerie Token"].card_id, ids)
        self.assertEqual(self.registry["Ajani Cat Token"].card_id, 437)

    def test_reference_keyword_creatures_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_keyword_creatures.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[33:], list(range(236, 243)))

    def test_reference_gainlands_lifegain_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_gainlands_lifegain.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:8], list(range(243, 251)))
        self.assertEqual(ids[37:], [251, 252, 253])

    def test_reference_triggers_tricks_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_triggers_tricks.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:12], list(range(254, 266)))

    def test_reference_equipment_search_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_equipment_search.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(sorted(ids[32:]), list(range(268, 276)))

    def test_reference_tokens_auras_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_tokens_auras.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:9], list(range(276, 285)))

    def test_reference_removal_tricks_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_removal_tricks.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(sorted(set(ids) - set(range(289))), list(range(289, 307)))

    def test_reference_counters_threats_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_counters_threats.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:10], list(range(307, 317)))

    def test_reference_library_search_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_library_search.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(sorted(ids[36:]), list(range(317, 321)))

    def test_reference_simple_triggers_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_simple_triggers.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:5], list(range(322, 327)))

    def test_reference_activated_combat_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_activated_combat.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:3], list(range(327, 330)))

    def test_reference_surveil_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_surveil.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:2], [330, 331])

    def test_reference_first_lifegain_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_first_lifegain.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:2], [332, 333])

    def test_reference_graveyard_triggers_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_graveyard_triggers.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:8], [334] * 4 + [335] * 4)

    def test_reference_static_team_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_static_team.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:8], [336] * 4 + [337] * 4)

    def test_reference_prepared_families_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_prepared_families.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:31], list(range(338, 369)))

    def test_reference_instant_sorcery_reducers_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_instant_sorcery_reducers.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:8], [369] * 4 + [370] * 4)

    def test_reference_conditional_creatures_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_conditional_flash_power_creatures.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:24], [371] * 4 + [372] * 4 + [373] * 4 + [374] * 4 + [375] * 4 + [376] * 4)

    def test_reference_following_creatures_spells_hare_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_following_creatures_spells_and_hare.dck").read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:16], [i for i in range(377, 385) for _ in range(2)])

    def test_hare_copies_are_unrestricted_and_new_tokens_are_undeckable(self) -> None:
        deck = limited.parse_dck("5 Hare Apparent\n35 Plains\n")
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:5], [384] * 5)
        for name in ("Human Token", "Rabbit Token"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "token"):
                limited.resolve_mainboard(limited.parse_dck("39 Plains\n1 " + name), self.registry)

    def test_extension_rejects_duplicates_and_preserves_base_card_ids(self) -> None:
        base = (REPO_ROOT / "data/cards_v1.json").read_bytes()
        extension = (FIXTURES / "cards_v1.json").read_bytes()
        base_registry = limited.registry_from_json(limited.load_json(base))
        self.assertEqual(len(base_registry), 192)
        self.assertNotIn("Plains", base_registry)
        for name, card in base_registry.items():
            self.assertEqual(self.registry[name], card)
        self.assertEqual(self.registry["Plains"].card_id, 192)
        with self.assertRaisesRegex(ValueError, "duplicate name"):
            limited.combined_registry(base, [extension, extension])
        with self.assertRaisesRegex(ValueError, "registry version 2"):
            limited.combined_registry(base, [b'{"version":3,"cards":[]}'])

    def test_paladin_skyknight_sire_reference_and_appended_ids(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_paladin_skyknight_sire.dck").read_text(encoding="utf-8"))
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(resolved), 40)
        for name, card_id, copies in [
            ("Inspiring Paladin", 387, 4),
            ("Skyknight Squire", 388, 4),
            ("Sire of Seven Deaths", 389, 2),
        ]:
            self.assertEqual(self.registry[name].card_id, card_id)
            self.assertEqual(resolved.count(card_id), copies)

    def test_raid_entry_reference_and_appended_ids(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_raid_entries.dck").read_text(encoding="utf-8"))
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(resolved), 40)
        for name, card_id in [("Skyship Buccaneer", 390), ("Goblin Boarders", 391)]:
            self.assertEqual(self.registry[name].card_id, card_id)
            self.assertEqual(resolved.count(card_id), 4)
        self.assertEqual(self.registry["Gorehorn Raider"].card_id, 412)
        self.assertNotIn(412, resolved)
        self.assertEqual(self.registry["Arahbo, the First Fang"].card_id, 422)
        self.assertEqual(self.registry["Strongbox Raider"].card_id, 423)
        self.assertEqual(self.registry["Fiery Annihilation"].card_id, 424)
        self.assertEqual(self.registry["Crystal Barricade"].card_id, 425)

    def test_crystal_reference_keeps_true_card_counts_and_appended_identity(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_crystal_barricade.dck").read_text(encoding="utf-8"))
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(resolved), 40)
        self.assertEqual(resolved[:4], [425] * 4)
        self.assertEqual(resolved.count(self.registry["Lightning Bolt"].card_id), 4)
        self.assertEqual(resolved.count(self.registry["Viridian Longbow"].card_id), 4)
        self.assertEqual(resolved.count(self.registry["Llanowar Elves"].card_id), 4)
        for name in ["Plains", "Mountain", "Forest"]:
            self.assertEqual(resolved.count(self.registry[name].card_id), 8)

    def test_krenko_reference_resolves_real_goblin_companions_and_appended_identity(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_krenko_mob_boss.dck").read_text(encoding="utf-8"))
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(resolved), 40)
        self.assertEqual(self.registry["Krenko, Mob Boss"].card_id, 426)
        self.assertEqual(resolved[:4], [426] * 4)
        for name in ["Goblin Boarders", "Goblin Surprise", "Lightning Bolt"]:
            self.assertEqual(resolved.count(self.registry[name].card_id), 4)
        self.assertEqual(resolved.count(self.registry["Mountain"].card_id), 24)

    def test_zul_reference_resolves_printed_and_changeling_zombies_with_real_mana(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_zul_ashur.dck").read_text(encoding="utf-8"))
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(resolved), 40)
        self.assertEqual(self.registry["Zul Ashur, Lich Lord"].card_id, 427)
        self.assertEqual(resolved[:4], [427] * 4)
        for name in ["Diregraf Ghoul", "Webweaver Changeling", "Stab"]:
            self.assertEqual(resolved.count(self.registry[name].card_id), 4)
        for name in ["Swamp", "Forest"]:
            self.assertEqual(resolved.count(self.registry[name].card_id), 12)

    def test_soulstone_kellan_reference_uses_registered_cards_and_real_mana(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_soulstone_kellan.dck").read_text(encoding="utf-8"))
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(resolved), 40)
        self.assertEqual(self.registry["Soulstone Sanctuary"].card_id, 428)
        self.assertEqual(self.registry["Kellan, Planar Trailblazer"].card_id, 429)
        self.assertEqual(resolved[:8], [428] * 4 + [429] * 4)
        for name in ["Frenzied Goblin", "Lightning Bolt", "Goblin Boarders"]:
            self.assertEqual(resolved.count(self.registry[name].card_id), 4)
        self.assertEqual(resolved.count(self.registry["Mountain"].card_id), 20)

    def test_banner_reference_resolves_real_color_creatures_and_mana(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_heraldic_banner.dck").read_text(encoding="utf-8"))
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(resolved), 40)
        self.assertEqual(self.registry["Heraldic Banner"].card_id, 430)
        self.assertEqual(resolved[:4], [430] * 4)
        for name in ["Kellan, Planar Trailblazer", "Soulstone Sanctuary", "Goblin Boarders", "Savannah Lions"]:
            self.assertEqual(resolved.count(self.registry[name].card_id), 4)
        for name in ["Mountain", "Plains"]:
            self.assertEqual(resolved.count(self.registry[name].card_id), 10)

    def test_inspiration_reference_uses_real_instant_sorcery_and_mana(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_inspiration_from_beyond.dck").read_text(encoding="utf-8"))
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(resolved), 40)
        self.assertEqual(self.registry["Inspiration from Beyond"].card_id, 431)
        self.assertEqual(resolved[:4], [431] * 4)
        for name in ["Think Twice", "Refute", "Uncharted Voyage"]:
            self.assertEqual(resolved.count(self.registry[name].card_id), 4)
        self.assertEqual(resolved.count(self.registry["Island"].card_id), 24)

    def test_outcome_memorial_fishing_and_twinflame_reference_decks_resolve_reserved_ids(self) -> None:
        cases = [
            ("herald_of_eternal_dawn", "Herald of Eternal Dawn", 432,
             ["Savannah Lions", "Helpful Hunter", "Sun-Blessed Healer", "Banishing Light"], "Plains"),
            ("akromas_memorial", "Akroma's Memorial", 433,
             ["Llanowar Elves", "Webweaver Changeling", "Mossborn Hydra", "Garruk's Uprising"], "Forest"),
            ("fishing_pole", "Fishing Pole", 434,
             ["Llanowar Elves", "Quirion Ranger", "Viridian Longbow", "Scavenging Ooze"], "Forest"),
            ("twinflame_tyrant", "Twinflame Tyrant", 436,
             ["Lightning Bolt", "Fanatical Firebrand", "Heartfire Immolator", "Gorehorn Raider"], "Mountain"),
        ]
        for suffix, name, card_id, spells, land in cases:
            with self.subTest(name=name):
                deck = limited.parse_dck((FIXTURES / f"FDN_reference_{suffix}.dck").read_text(encoding="utf-8"))
                resolved = limited.resolve_mainboard(deck, self.registry)
                self.assertEqual(len(resolved), 40)
                self.assertEqual(self.registry[name].card_id, card_id)
                self.assertEqual(resolved[:4], [card_id] * 4)
                for spell in spells:
                    self.assertEqual(resolved.count(self.registry[spell].card_id), 4)
                self.assertEqual(resolved.count(self.registry[land].card_id), 20)
                self.assertNotIn(self.registry["Fish Token"].card_id, resolved)
        self.assertEqual(self.registry["Fish Token"].card_id, 435)
        self.assertTrue(self.registry["Fish Token"].is_token)

    def test_original_decks_are_40_cards_and_fully_resolve(self) -> None:
        for filename, supported_copies, source_sha256 in [
            ("FDN_top_04956_UG.dck", 40, "bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86"),
            ("FDN_top_20626_WG.dck", 40, "be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7"),
        ]:
            with self.subTest(filename=filename):
                self.assertEqual(hashlib.sha256((FIXTURES / filename).read_bytes()).hexdigest(), source_sha256)
                deck = limited.parse_dck((FIXTURES / filename).read_text(encoding="utf-8"))
                report = limited.inspect_deck(deck, self.registry)
                self.assertEqual(report["mainboard"]["copies"], 40)
                self.assertEqual(report["mainboard"]["supported_copies"], supported_copies)
                self.assertTrue(report["minimum_mainboard_size_met"])
                self.assertEqual(report["mainboard"]["registry_ready"], supported_copies == 40)
                if supported_copies == 40:
                    self.assertEqual(len(limited.resolve_mainboard(deck, self.registry)), 40)
                else:
                    with self.assertRaisesRegex(ValueError, "unsupported mainboard"):
                        limited.resolve_mainboard(deck, self.registry)

    def test_printings_repeat_counts_and_sideboard_are_preserved(self) -> None:
        deck = limited.parse_dck(
            "// comment\r\nNAME:Test\r\n5 [FDN:280] Forest\r\n"
            "35 Forest\r\nSB: 1 Never Implemented\r\n"
        )
        report = limited.inspect_deck(deck, self.registry)
        self.assertEqual(deck.mainboard[0].printing, "FDN:280")
        self.assertEqual(report["mainboard"]["unique_cards"], 1)
        self.assertTrue(report["mainboard"]["registry_ready"])
        self.assertFalse(report["sideboard"]["registry_ready"])
        self.assertEqual(limited.resolve_mainboard(deck, self.registry), [self.registry["Forest"].card_id] * 40)

    def test_arbitrary_decks_keep_row_and_copy_order_and_allow_over_40(self) -> None:
        deck = limited.parse_dck("20 Island\n21 Forest\n")
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(ids, [self.registry["Island"].card_id] * 20 + [self.registry["Forest"].card_id] * 21)
        with self.assertRaisesRegex(ValueError, "at least 40"):
            limited.resolve_mainboard(limited.parse_dck("39 Forest"), self.registry)

    def test_condemn_return_reference_resolves_appended_spells(self) -> None:
        path = FIXTURES / "FDN_reference_condemn_return.dck"
        deck = limited.parse_dck(path.read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids.count(392), 4)
        self.assertEqual(ids.count(393), 4)
        self.assertEqual(self.registry["Condemn"].card_id, 392)
        self.assertEqual(self.registry["Run Away Together"].card_id, 393)

    def test_bloodwitch_reference_resolves_appended_creature(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_bloodwitch.dck").read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids.count(394), 4)
        self.assertEqual(self.registry["Vengeful Bloodwitch"].card_id, 394)

    def test_divine_reference_keeps_prior_rows_and_resolves_appended_spell(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_divine_resilience.dck").read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:4], [395] * 4)
        self.assertEqual(self.registry["Divine Resilience"].card_id, 395)
        self.assertEqual(self.registry["Vengeful Bloodwitch"].card_id, 394)

    def test_ooze_reference_appends_after_divine_without_renumbering(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_scavenging_ooze.dck").read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:4], [396] * 4)
        self.assertEqual(self.registry["Scavenging Ooze"].card_id, 396)
        self.assertEqual(self.registry["Divine Resilience"].card_id, 395)

    def test_zombie_reference_appends_after_ooze_without_renumbering(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_soul_shackled_zombie.dck").read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:4], [397] * 4)
        self.assertEqual(self.registry["Soul-Shackled Zombie"].card_id, 397)
        self.assertEqual(self.registry["Scavenging Ooze"].card_id, 396)

    def test_heartfire_reference_appends_after_zombie_without_renumbering(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_heartfire_immolator.dck").read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:4], [398] * 4)
        self.assertEqual(self.registry["Heartfire Immolator"].card_id, 398)
        self.assertEqual(self.registry["Soul-Shackled Zombie"].card_id, 397)

    def test_next_source_batch_appends_and_internal_copy_is_not_a_booster(self) -> None:
        for slug, name, ident in [("searslicer_goblin", "Searslicer Goblin", 399),
                                  ("self_reflection", "Self-Reflection", 400),
                                  ("squad_rallier", "Squad Rallier", 402),
                                  ("sower_of_chaos", "Sower of Chaos", 403)]:
            deck = limited.parse_dck((FIXTURES / f"FDN_reference_{slug}.dck").read_text(encoding="utf-8"))
            ids = limited.resolve_mainboard(deck, self.registry)
            self.assertEqual(len(ids), 40)
            self.assertEqual(ids[:4], [ident] * 4)
            self.assertEqual(self.registry[name].card_id, ident)
        self.assertEqual(self.registry["Nameless Creature Copy Token"].card_id, 401)
        self.assertEqual(limited.card_status("Nameless Creature Copy Token", self.registry), "token")
        frozen_names = json.loads((FIXTURES / "booster_pool_v1.json").read_text(encoding="utf-8"))["cards"]
        self.assertNotIn("Nameless Creature Copy Token", [card["name"] for card in frozen_names])

    def test_later_source_batch_and_observer_sentinels_preserve_previous_ids(self) -> None:
        for slug, name, ident in [("frenzied_goblin", "Frenzied Goblin", 404),
                                  ("slumbering_cerberus", "Slumbering Cerberus", 405),
                                  ("wildwood_scourge", "Wildwood Scourge", 406)]:
            deck = limited.parse_dck((FIXTURES / f"FDN_reference_{slug}.dck").read_text(encoding="utf-8"))
            self.assertEqual(limited.resolve_mainboard(deck, self.registry)[:4], [ident] * 4)
            self.assertEqual(self.registry[name].card_id, ident)
        for name, ident in [("Face-down creature", 407), ("Face-down card", 408)]:
            self.assertEqual(self.registry[name].card_id, ident)
            with self.assertRaisesRegex(ValueError, "token"):
                limited.resolve_mainboard(limited.parse_dck("39 Plains\n1 " + name), self.registry)
        self.assertEqual(self.registry["Sower of Chaos"].card_id, 403)

    def test_authority_and_employment_candidate_preserves_prior_registry_ids(self) -> None:
        for slug, name, ident in [("authority_of_the_consuls", "Authority of the Consuls", 409),
                                  ("involuntary_employment", "Involuntary Employment", 410)]:
            deck = limited.parse_dck((FIXTURES / f"FDN_reference_{slug}.dck").read_text(encoding="utf-8"))
            self.assertEqual(limited.resolve_mainboard(deck, self.registry)[:4], [ident] * 4)
            self.assertEqual(self.registry[name].card_id, ident)
        self.assertEqual(self.registry["Wildwood Scourge"].card_id, 406)
        self.assertEqual(self.registry["Face-down card"].card_id, 408)

    def test_future_librarian_and_raid_candidates_append_after_employment(self) -> None:
        for slug, name, ident in [("mild_mannered_librarian", "Mild-Mannered Librarian", 411),
                                  ("gorehorn_raider", "Gorehorn Raider", 412),
                                  ("gutless_plunderer", "Gutless Plunderer", 413)]:
            deck = limited.parse_dck((FIXTURES / f"FDN_reference_{slug}.dck").read_text(encoding="utf-8"))
            self.assertEqual(limited.resolve_mainboard(deck, self.registry)[:4], [ident] * 4)
            self.assertEqual(len(limited.resolve_mainboard(deck, self.registry)), 40)
            self.assertEqual(self.registry[name].card_id, ident)
        self.assertEqual(self.registry["Involuntary Employment"].card_id, 410)

    def test_bloom_candidate_appends_after_three_prepared_creatures(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_bloom_tender.dck").read_text(encoding="utf-8"))
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(resolved[:4], [414] * 4)
        self.assertEqual(len(resolved), 40)
        self.assertEqual(self.registry["Bloom Tender"].card_id, 414)
        self.assertEqual(self.registry["Gutless Plunderer"].card_id, 413)

    def test_temporal_candidate_appends_after_bloom_without_reindexing(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_temporal_manipulation.dck").read_text(encoding="utf-8"))
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(resolved[:4], [415] * 4)
        self.assertEqual(len(resolved), 40)
        self.assertEqual(self.registry["Temporal Manipulation"].card_id, 415)
        self.assertEqual(self.registry["Bloom Tender"].card_id, 414)

    def test_unknown_partial_no_effect_and_token_are_all_rejected(self) -> None:
        registry = limited.registry_from_json({"version": 2, "cards": [
            {"name": "Supported", "engine_capability": "full"},
            {"name": "Partial", "engine_capability": "partial"},
            {"name": "Absent Capability"},
            {"name": "Token", "engine_capability": "full", "is_token": True},
        ]})
        for name, status in [("Unknown", "missing"), ("Partial", "partial"),
                             ("Absent Capability", "no_effect"), ("Token", "token")]:
            with self.subTest(name=name):
                deck = limited.parse_dck(f"39 Supported\n1 {name}")
                self.assertEqual(limited.card_status(name, registry), status)
                with self.assertRaisesRegex(ValueError, status):
                    limited.resolve_mainboard(deck, registry)

    def test_bad_deck_rows_do_not_disappear_silently(self) -> None:
        for text in ("", "0 Forest", "-1 Forest", "1.5 Forest", "Forest", "1 [FDN:280]",
                     "NAME:", "NAME:A\nNAME:B\n40 Forest", "SB: 40 Forest"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                limited.parse_dck(text)

    def test_registry_errors_and_duplicate_json_keys_are_rejected(self) -> None:
        for value in ({"version": 1, "cards": []}, {"version": 2, "cards": [None]},
                      {"version": 2, "cards": [{"name": "A"}, {"name": "A"}]},
                      {"version": 2, "cards": [{"name": "A", "is_token": "false"}]},
                      {"version": 2, "cards": [{"name": "A", "engine_capability": "assumed"}]}):
            with self.subTest(value=value), self.assertRaises(ValueError):
                limited.registry_from_json(value)
        for raw in (b'{"version":2,"version":1}', b'{"version":NaN}', b'[]'):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                limited.load_json(raw)

    def test_inventory_includes_basics_and_fixture_cards_beyond_reference(self) -> None:
        report = limited.inventory(["Island"], self.registry, [limited.parse_dck("40 Unknown")])
        by_name = {card["name"]: card for card in report["cards"]}
        self.assertEqual(report["reference_card_count"], 1)
        self.assertEqual(report["required_card_count"], 6)
        self.assertEqual(by_name["Plains"]["status"], "full")
        self.assertFalse(by_name["Unknown"]["in_reference"])
        self.assertEqual(by_name["Unknown"]["fixture_copies"], 40)
        for names in ([], ["A", "A"], [42]):
            with self.subTest(names=names), self.assertRaises(ValueError):
                limited.inventory(names, self.registry, [])

    def test_real_reference_already_contains_all_basic_lands(self) -> None:
        document = limited.load_json((FIXTURES / "card_names.json").read_bytes())
        names = document["card_names"]
        self.assertEqual(len(names), 286)
        self.assertTrue(set(limited.BASIC_LANDS).issubset(names))
        decks = [limited.parse_dck(path.read_text(encoding="utf-8")) for path in sorted(FIXTURES.glob("FDN_top_*.dck"))]
        report = limited.inventory(names, self.registry, decks)
        self.assertEqual(report["reference_card_count"], report["required_card_count"])
        self.assertEqual(sum(card["status"] == "full" for card in report["cards"]), 245)
        self.assertEqual(sum(card["status"] == "partial" for card in report["cards"]), 15)
        self.assertEqual(sum(card["status"] == "missing" for card in report["cards"]), 26)
        self.assertEqual(sum(card["fixture_copies"] > 0 and card["status"] != "full"
                             for card in report["cards"]), 0)

    def test_cli_refusal_has_no_materialized_ids_and_inspection_is_deterministic(self) -> None:
        args = ["--deck", str(FIXTURES / "FDN_top_04956_UG.dck")]
        stdout, stderr = io.StringIO(), io.StringIO()
        with tempfile.TemporaryDirectory() as tmp:
            unsupported = Path(tmp) / "unsupported.dck"
            unsupported.write_text("39 Forest\n1 Never Implemented\n", encoding="utf-8")
            with redirect_stdout(stdout), redirect_stderr(stderr):
                self.assertEqual(limited.main(["resolve", "--deck", str(unsupported)]), 2)
        self.assertEqual(stdout.getvalue(), "")
        self.assertIn("unsupported mainboard", json.loads(stderr.getvalue())["error"])
        outputs = []
        for _ in range(2):
            output = io.StringIO()
            with redirect_stdout(output):
                self.assertEqual(limited.main(["inspect", *args]), 0)
            outputs.append(output.getvalue())
        self.assertEqual(*outputs)
        report = json.loads(outputs[0])
        self.assertEqual(report["deck_sha256"], hashlib.sha256((FIXTURES / "FDN_top_04956_UG.dck").read_bytes()).hexdigest())

    def test_cli_resolves_both_unchanged_original_decks_in_copy_order(self) -> None:
        for filename in ["FDN_top_04956_UG.dck", "FDN_top_20626_WG.dck"]:
            with self.subTest(filename=filename):
                path = FIXTURES / filename
                output = io.StringIO()
                with redirect_stdout(output):
                    self.assertEqual(limited.main(["resolve", "--deck", str(path)]), 0)
                report = json.loads(output.getvalue())
                expected = limited.resolve_mainboard(limited.parse_dck(path.read_text(encoding="utf-8")), self.registry)
                self.assertEqual(report["card_ids"], expected)
                self.assertEqual(len(report["card_ids"]), 40)
                if "UG" in filename:
                    self.assertEqual(report["card_ids"].count(235), 2)

    def test_cli_resolves_supported_deck_and_reports_its_registry_identity(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            deck_path = Path(tmp) / "deck.dck"
            deck_path.write_text("20 Forest\n20 Island\nSB: 1 Missing", encoding="utf-8")
            output = io.StringIO()
            with redirect_stdout(output):
                self.assertEqual(limited.main(["resolve", "--deck", str(deck_path)]), 0)
            report = json.loads(output.getvalue())
            self.assertEqual(len(report["card_ids"]), 40)
            self.assertEqual(report["card_id_order"], "dck-row-then-copy/v1")
            self.assertEqual(report["registry_sha256"], hashlib.sha256((REPO_ROOT / "data/cards_v1.json").read_bytes()).hexdigest())
            self.assertEqual(report["registry_extensions_sha256"],
                             [hashlib.sha256((FIXTURES / "cards_v1.json").read_bytes()).hexdigest()])


if __name__ == "__main__":
    unittest.main()
