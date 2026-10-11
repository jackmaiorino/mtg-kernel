from __future__ import annotations

import hashlib
from pathlib import Path
import sys
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
import limited_decks_v1 as limited


STANDARD = REPO_ROOT / "data/standard/magezero_v1"
# Completion-candidate admission. Runtime verification is tracked separately.
SUPPORTED_NONBASIC = {
    "Abrade",
    "Adarkar Wastes",
    "Adeline, Resplendent Cathar",
    "Agatha's Soul Cauldron",
    "Aloe Alchemist",
    "Anoint with Affliction",
    "Ascendant Packleader",
    "Assimilation Aegis",
    "Axebane Ferox",
    "Basilisk Collar",
    "Battlefield Forge",
    "Big Score",
    "Blackcleave Cliffs",
    "Bloodletter of Aclazotz",
    "Bloodtithe Harvester",
    "Blooming Marsh",
    "Blue Sun's Twilight",
    "Boltwave",
    "Boseiju, Who Endures",
    "Braided Net",
    "Breach the Multiverse",
    "Brightglass Gearhulk",
    "Brushland",
    "Brutal Cathar",
    "Burnout Bashtronaut",
    "Burst Lightning",
    "Candy Trail",
    "Case of the Gateway Express",
    "Case of the Uneaten Feast",
    "Caves of Koilos",
    "Cecil, Dark Knight",
    "Cenote Scout",
    "Chandra, Hope's Beacon",
    "Chrome Host Seedshark",
    "Clay-Fired Bricks",
    "Collector's Cage",
    "Concealed Courtyard",
    "Consider",
    "Coppercoat Vanguard",
    "Copperline Gorge",
    "Cori-Steel Cutter",
    "Cut Down",
    "Dark Confidant",
    "Darkslick Shores",
    "Darkstar Augur",
    "Deep-Cavern Bat",
    "Deserted Beach",
    "Destroy Evil",
    "Dissipate",
    "Djeru and Hazoret",
    "Dreamroot Cascade",
    "Duress",
    "Dusk Rose Reliquary",
    "Eiganjo, Seat of the Empire",
    "Elegant Parlor",
    "Emberheart Challenger",
    "Enduring Curiosity",
    "Enduring Innocence",
    "Ertai Resurrected",
    "Essence Channeler",
    "Essence Scatter",
    "Etali, Primal Conqueror",
    "Evolving Adaptive",
    "Extraction Specialist",
    "Fable of the Mirror-Breaker",
    "Fading Hope",
    "Faerie Dreamthief",
    "Floodfarm Verge",
    "Floodpits Drowner",
    "Flourishing Bloom-Kin",
    "Flow of Knowledge",
    "Fomori Vault",
    "Forsaken Miner",
    "Fountainport",
    "Full Bore",
    "Gatekeeper of Malakir",
    "Get Lost",
    "Gingerbrute",
    "Gix's Command",
    "Gleeful Demolition",
    "Glissa Sunslayer",
    "Gloomlake Verge",
    "Go for the Throat",
    "Graveyard Trespasser",
    "Gwenna, Eyes of Gaea",
    "Hajar, Loyal Bodyguard",
    "Halana and Alena, Partners",
    "Hard-Hitting Question",
    "Hardlight Containment",
    "Haughty Djinn",
    "Haunted Ridge",
    "Heartfire Hero",
    "Hired Claw",
    "Hopeful Initiate",
    "Hullbreaker Horror",
    "Hushwood Verge",
    "Imodane's Recruiter",
    "Impulse",
    "Innkeeper's Talent",
    "Inspiring Vantage",
    "Invoke Despair",
    "Iridescent Vinelasher",
    "Jetmir's Garden",
    "Jodah, the Unifier",
    "Kaito, Bane of Nightmares",
    "Karplusan Forest",
    "Katilda, Dawnhart Prime",
    "Kayla's Reconstruction",
    "Kellan, Planar Trailblazer",
    "Knight-Errant of Eos",
    "Lagrella, the Magpie",
    "Lightning Strike",
    "Liliana of the Veil",
    "Llanowar Elves",
    "Llanowar Wastes",
    "Lunar Convocation",
    "Lupinflower Village",
    "Lush Portico",
    "Maelstrom Pulse",
    "Make Disappear",
    "Manifold Mouse",
    "Melira, the Living Cure",
    "Memory Deluge",
    "Mirrex",
    "Mishra's Foundry",
    "Monastery Swiftspear",
    "Monstrous Rage",
    "Mosswood Dreadknight",
    "Mudflat Village",
    "Negate",
    "Nova Hellkite",
    "Novice Inspector",
    "Ojer Axonil, Deepest Might",
    "Opt",
    "Otawara, Soaring City",
    "Overgrown Farmland",
    "Overlord of the Mistmoors",
    "Pawpatch Recruit",
    "Phantom Interference",
    "Plaza of Heroes",
    "Polukranos Reborn",
    "Preacher of the Schism",
    "Questing Druid",
    "Quirion Beastcaller",
    "Razorkin Needlehead",
    "Razorverge Thicket",
    "Reckoner Bankbuster",
    "Recruitment Officer",
    "Regal Bunnicorn",
    "Repurposing Bay",
    "Resolute Reinforcements",
    "Restless Bivouac",
    "Restless Cottage",
    "Restless Fortress",
    "Restless Prairie",
    "Restless Reef",
    "Restless Ridgeline",
    "Restless Vinestalk",
    "Riverpyre Verge",
    "Rockface Village",
    "Rockfall Vale",
    "Ruin-Lurker Bat",
    "Sandstorm Salvager",
    "Sanguine Evangelist",
    "Seachrome Coast",
    "Seam Rip",
    "Sentinel of the Nameless City",
    "Shanna, Purifying Blade",
    "Sharp-Eyed Rookie",
    "Sheltered by Ghosts",
    "Sheoldred's Edict",
    "Sheoldred, the Apocalypse",
    "Shivan Reef",
    "Shock",
    "Shoot the Sheriff",
    "Shore Up",
    "Simulacrum Synthesizer",
    "Skrelv, Defector Mite",
    "Sleight of Hand",
    "Slickshot Show-Off",
    "Snakeskin Veil",
    "Sokenzan, Crucible of Defiance",
    "Soulstone Sanctuary",
    "Spara's Headquarters",
    "Spell Pierce",
    "Spirebluff Canal",
    "Spring-Loaded Sawblades",
    "Spyglass Siren",
    "Starting Town",
    "Stock Up",
    "Stormchaser's Talent",
    "Subterranean Schooner",
    "Sulfurous Springs",
    "Surge Engine",
    "Surrak, Elusive Hunter",
    "Takenuma, Abandoned Mire",
    "Tear Asunder",
    "Teething Wurmlet",
    "Teferi, Temporal Pilgrim",
    "Tersa Lightshatter",
    "Thalia, Guardian of Thraben",
    "The Irencrag",
    "Thirst for Discovery",
    "Thornspire Verge",
    "Thousand Moons Smithy",
    "Tishana's Tidebinder",
    "Tolarian Terror",
    "Tough Cookie",
    "Tranquil Frillback",
    "Underground Mortuary",
    "Underground River",
    "Unholy Annex // Ritual Chamber",
    "United Battlefront",
    "Unstoppable Slasher",
    "Virtue of Loyalty",
    "Voldaren Epicure",
    "Warden of the Inner Sky",
    "Warleader's Call",
    "Wastewood Verge",
    "Witchstalker Frenzy",
    "Yavimaya Coast",
    "Yotian Frontliner",
    "Ziatora's Proving Ground",
    "Zoetic Glyph",
    "Zoraline, Cosmos Caller",
}

STANDARD_TOKENS = {
    "Human Token",
    "Iridescent Vinelasher Offspring Token",
    "Incubator Token",
    "Bat Token",
    "Darkstar Augur Offspring Token",
    "Pawpatch Recruit Offspring Token",
    "Manifold Mouse Offspring Token",
    "Monk Token",
    "White Insect Token",
    "Spirit Token",
    "Phyrexian Mite Token",
    "Teferi Spirit Token",
    "Phyrexian Hydra Reach Token",
    "Phyrexian Hydra Lifelink Token",
    "Demon Flying Token",
    "Bat Flying Token",
    "Karn Construct Token",
    "Otter Prowess Token",
    "Fable Goblin Shaman Token",
    "Cosmium Gnome Token",
    "Pilot Token",
    "Soldier Token",
    "Phyrexian Goblin Token",
    "Monster Role Token",
    "Golem Token",
    "Vampire Token",
    "Colorless Spirit Token",
    "Fish Token",
    "Knight Vigilance Token",
    "Face-down creature",
    "Face-down card",
    "Gnome Soldier Token",
}

# Original MageZero release bytes; deck size and card names are checked below.
DECK_SHA256 = {
    "Standard-MonoB.dck": "f0019124eef0f11a5458b7289344bd4dfbc99fc0fa1343c30ac93112594f17a6",
    "Standard-MonoG.dck": "093d5c57c20ccaf79c48b8707712a0e260eb578a148f31b864610e17e167f50b",
    "Standard-MonoR.dck": "68dac8d49eb93096eee27e5db2582f441334db0bc14345a69894ff089593e386",
    "Standard-MonoU.dck": "e2f14a69fa9a36a0fe7e57bc803ec0034efecfdcd6c6ea8736c8f4ceba35b02b",
    "Standard-MonoW.dck": "b8b416e6acd7e3004a364ace019a5d38d8bc7f0d9c23981efa0dd365e3213ed6",
    "Standard16-5C.dck": "95f90fd18ebac5d293166f4dddef71e93172ffc076f4e461d1b528011c66e6c8",
    "Standard16-BW.dck": "aa160d0ebd6c9aceb9463db60ce9c92713fe5d2254b4674c95ae060a180ec86b",
    "Standard16-GB.dck": "5005a49d5f33498a904c234fae3f18fa1e0117f406f3778cf487033514fe2f76",
    "Standard16-GW.dck": "d04f29eff80ecf1d03cc1382bc061f11ad1428685f2997db3c21cb56f6f215e6",
    "Standard16-RB.dck": "183d4f053aada5112ff42cbcf13e827fe37917d3cb82dd68ab6c8912ce852212",
    "Standard16-RG.dck": "5eeae760a2f7c85117717e8153dd54104a400e2537f18f25b343d1af6b174edd",
    "Standard16-RW.dck": "b32cd25712aa472e75851696e9d944a31184a5402485825923fda5c7a0677a13",
    "Standard16-UB.dck": "246accf8e03e0e1d07fbadef6631118d5b7b06f548216a8e90c07c6e3bb73c00",
    "Standard16-UG.dck": "2d604993eaecdd0a91550d1bdfdf54e65421877f315da751e6f7f2d66f83d444",
    "Standard16-UR.dck": "9fe5248d3c83db907094f0d729c66a32bc5be0aa746adc1c480e729654813fe9",
    "Standard16-UW.dck": "63a3e09868434432e532aeec8562c7d6ea49be2f377a00dd22518a70715b30c3",
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

    def test_all_sixteen_decks_resolve_for_full_admission(self) -> None:
        for name, deck in self.decks.items():
            with self.subTest(deck=name):
                resolved = limited.resolve_mainboard(deck, self.registry)
                self.assertEqual(len(resolved), 62 if name == "Standard-MonoU" else 60)

    def test_original_deck_bytes_are_unchanged(self) -> None:
        actual = {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                  for path in sorted((STANDARD / "decks").glob("*.dck"))}
        self.assertEqual(actual, DECK_SHA256)

    def test_standard_token_support_is_complete(self) -> None:
        rows = limited.load_json((STANDARD / "cards_v1.json").read_bytes())["cards"]
        tokens = {row["name"] for row in rows if row.get("is_token", False)}
        self.assertEqual(tokens, STANDARD_TOKENS)
        for row in rows:
            with self.subTest(card=row["name"]):
                self.assertEqual(row["engine_capability"], "full")
                self.assertTrue(row["decks"] or row.get("is_token", False))
                actual_decks = {name + ".dck" for name, deck in self.decks.items()
                                if any(entry.name == row["name"] for entry in deck.mainboard)}
                if row["name"] == "Witness Protection":
                    # Shared FDN rules fixture exercises Kaito's layer ordering;
                    # it is not a member of any MageZero training deck.
                    fdn = limited.load_json(
                        (REPO_ROOT / "data/limited/fdn_v1/cards_v1.json").read_bytes())
                    self.assertEqual(row, next(card for card in fdn["cards"]
                                               if card["name"] == row["name"]))
                    self.assertEqual(actual_decks, set())
                    continue
                self.assertEqual(set(row["decks"]), actual_decks)

    def test_inventory_has_all_225_nonbasic_cards_and_no_gaps(self) -> None:
        names = limited.load_json((STANDARD / "card_names.json").read_bytes())["card_names"]
        self.assertEqual(len(names), 225)
        report = limited.inventory(names, self.registry, list(self.decks.values()))
        self.assertTrue(all(card["status"] == "full" for card in report["cards"]))


if __name__ == "__main__":
    unittest.main()
