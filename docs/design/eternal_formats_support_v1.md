# Pauper, Legacy and Vintage card support

Status: design, 2026-10-26, revised after an adversarial review (Fable, same day; all six
required changes applied). Codex's FDN work (#110, PR #308) is unaffected.

## Goal and measure

"Support a format" means the kernel can play the decks people actually play in it. The
measure is the **playable metagame share**: the summed metagame percentage of archetypes
whose whole mainboard, and companion if the deck has one, is Full in the `eternal-fixtures`
build. Best-of-one mainboards come first, as in the roadmap; sideboards follow once a
format's mainboards are playable.

`python/tools/eternal_meta_census_v1.py` computes it from a committed snapshot
(`data/eternal_v1/meta_snapshot_mtggoldfish_20261026.json`: 153 archetypes, one
representative MTGGoldfish paper list each, with the companion named where a sideboard
holds one). The snapshot is refreshed when the metagame moves, and each wave's PR reports
the census before and after.

## Baseline (snapshot 2026-10-26, eternal build with an empty eternal registry)

| Format | Archetypes | Distinct cards | Full | FDN only | Missing | Playable share |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Pauper | 60 | 535 | 173 | 4 | 358 | 20.9% |
| Legacy | 60 | 563 | 62 | 1 | 500 | 0% |
| Vintage | 33 | 427 | 38 | 1 | 388 | 0% |
| All three | | 1,225 | 200 | 6 | 1,019 | |

"Full" counts the Pauper and Standard registries. "FDN only" cards have a program that has
to be copied into the eternal registry. Completing decks greedily by share per new card
needs these card counts (companions not yet required in this count):

| Format | 50% of field | 75% | 90% |
| --- | ---: | ---: | ---: |
| Pauper | 13 | 76 | 148 |
| Legacy | 95 | 181 | 277 |
| Vintage | 82 | 111 | 154 |

Legacy and Vintage share most staples, so the joint count is lower than the sum. About
37% of the Vintage field plays Lurrus as a companion, so Vintage's curve depends on
companion support.

## Catalog

All three formats share one build, `eternal-fixtures`. Its registry is the frozen 192-card
Pauper prefix, then the Standard registry unchanged, then `data/eternal_v1/cards_v1.json`.

- The feature enables `standard-magezero-fixtures` (and through it FDN rules code), as
  Standard enables FDN. Every Standard card keeps its id and program; the catalog hash
  differs (`kernel_carddb_eternal/v1`), so Standard identity tests are gated off in this
  build while the rest of the Standard suite runs on it in CI.
- The default Pauper build, its hash and every frozen store are unchanged. New Pauper cards
  go in the eternal file, since the Pauper registry is frozen.
- Eternal ids follow the 254 Standard definitions; the catalog test pins that count. A
  Standard append would shift eternal ids, which is acceptable only before the first
  eternal model trains. The eternal hash is pinned in a test once CI first reports it.
- FDN-only cards are copied into the eternal file. Some FDN programs are compiled only when
  `standard-magezero-fixtures` is off, so a copy could stay flagged Full with no behavior.
  Every copy therefore ships with a rules test compiled under `eternal-fixtures`, and the
  FDN cards gated that way (Herald of Eternal Dawn, Bloodthirsty Conqueror, Twinflame
  Tyrant, Elenda, Heraldic Banner, Ajani, Dwynen) are not copied until their gate is
  widened.

## Work order

Waves are ordered by share gained per card. Each wave is one PR (one coherent deliverable),
built and tested on hosted CI or under a HaleysPC reservation while the desktop is
reserved.

| Wave | Content | New cards | Result |
| --- | --- | ---: | --- |
| E0 | Feature, empty eternal registry, catalog test, session games, CI group, census | 0 | Infrastructure |
| E1 | Pauper close-ins: Mono-Red Madness, Rally, Grixis Affinity, Dimir Faeries, Grixis and Dimir Control | 14 | Pauper 21% to about 53% |
| E2 | Legacy Dimir Tempo and Izzet Delver: original duals, surveil lands, fetchlands, Wasteland, Force of Will, Daze, Thoughtseize, Fatal Push, Unholy Heat, Prismari Charm, the Baubles, Brazen Borrower, Dragon's Rage Channeler, Murktide Regent, Moonshadow, Nethergoyf, Orcish Bowmasters, Tamiyo, Bilbo | 27 and a token | Legacy 0% to 19.3% |
| E3 | Legacy combo decks: Doomsday, Sneak and Show, Aluren, Omni-Tell (Show and Tell, Sneak Attack, Omniscience, Aluren, Doomsday, Thassa's Oracle, Jace, Emrakul, Atraxa, Acererak and the three Forgotten Realms dungeons, Chrome Mox, Lion's Eye Diamond, The One Ring, Cavern of Souls, Misdirection, Flusterstorm, Veil of Summer) | 27 and four tokens | Legacy 19.3% to 33.5% |
| E4 | Vintage Jewel Shops and Mono-White Initiative: Power Nine (with Mana Crypt, Mana Vault and Sol Ring), Workshop, Academy, Strip Mine, Karakas, Urza's Saga, Time Vault, Tinker, Karn, Tezzeret, Phyrexian Metamorph, Coveted Jewel, Trinisphere, the Initiative creatures, Archon of Emeria, Clarion Conqueror, Chancellor of the Annex, Anointed Peacekeeper, Witch Enchanter; six shared cards come from E3 | 36 | Vintage 0% to 28.4% |
| E4b | Vintage Lurrus lists (Esper and Dimir Lurrus Control, Lurrus PO, Lurrus Breach, Jeskai Lurrus, 4c Psychic Frog): companions, Lurrus of the Dream-Den, Demonic Tutor, Mana Drain, Force of Negation, Gush, Dig Through Time, Treasure Cruise, Snapcaster Mage, Underworld Breach, Brain Freeze, Psychic Frog, Hexing Squelcher, Null Rod, Pithing Needle, Mox Opal and the rest of the lists | 29 and two tokens | Vintage 28.4% to 62.3% |
| E5b | Legacy Boros Energy, Beanstalk Control, Azorius and Jeskai Tempo, Esper Control, and the Eldrazi and Tron cards outside E5a: Plateau, Savannah, Scrubland, Snow-Covered Plains, Arid Mesa, Windswept Heath, Goblin Bombardment, Up the Beanstalk, Stifle, Consign to Memory, Galvanic Discharge, Wrath of the Skies, Terminus, Leyline Binding, Static Prison, Phelia, Quantum Riddler, Guide of Souls, Ocelot Pride, Ajani, Amped Raptor, Phlage, Sand Scout, Voice of Victory, Lazotep Quarry, Thought-Knot Seer, Reality Smasher, It That Heralds the End, Eldrazi Linebreaker, Wastescape Battlemage, Eldrazi Temple, Eye of Ugin, Abundant Countryside, Petrified Hamlet, Eldrazi Confluence | 35 and six tokens | Legacy 33.5% to 49.4% |
| E6a | Pauper Tron, Esper, Dimir and 4c Affinity, Esper Ephemerate, Mono-Blue Faeries, Mardu Synthesizer (and Azorius Gates): Abjure, Ancient Den, Archaeomancer, Behold the Multiverse, Bender's Waterskin, Bramble Wurm, Brinebarrow Intruder, Crop Rotation, Cryoshatter, Dispatch, Energy Refractor, Ephemerate, Ethereal Haze, Gearseeker Serpent, Ghostly Flicker, Glacial Floodplain, Grim Bauble, Hard Evidence, Kor Skyfisher, Moment's Peace, Mulldrifter, Mystical Teachings, Omen of the Dead, Prohibit, Prophetic Prism, Razortide Bridge, Remote Isle, Rustvale Bridge, Sewer-veillance Cam, Shattered Landscape, Stream of Thought, Tangle, Tangled Islet, Tithing Blade, Union of the Third Path | 35 and a token | Pauper 53.5% to 69.0% |
| E5a | Vintage Raker Shops, Oath of Druids, Paradoxical Outcome, Blue Tinker and Beseech Storm, Legacy Mystic Forge, and with E5b's cards Legacy Eldrazi and Tron: Mystic Forge, Bolas's Citadel, Yawgmoth's Will, Chalice of the Void, Sensei's Divining Top, Oath of Druids, Oko, Ugin, Kozilek's Command, Talon Gates of Madara and the rest of the lists | 28 and a token | Vintage 62.3% to 80.0%, Legacy 49.4% to 59.6% (after E5b) |
| E6b | Pauper Bogles, White Aggro, Rakdos Madness, Boros Moxite, Mono-White Aggro, Snacker Gates and Boros Synthesizer (and Azorius Faeries): Abundant Growth, Alms of the Vein, Ancestral Mask, Angelic Gift, Armadillo Cloak, Asgardian Citadel, Ash Barrens, Battle Screech, Benevolent Blessing, Boros Garrison, Bushwhack, Cartouche of Solidarity, Cliffgate, Dawnbringer Cleric, Demand Answers, Eagles of the North, Ethereal Armor, Fortress Kin-Guard, Gladecover Scout, Greatsword of Tyr, Guardians' Pledge, Heliod's Pilgrim, Idyllic Grange, Jagged Barrens, Kitchen Imp, Leonardo, Lunarch Veteran, Malevolent Rumble, Mana Tithe, Manor Gate, Martyr's Soul, Pursue the Past, Raffine's Informant, Ragged Short Spear, Rakdos Carnarium, Ramosian Rally, Rancor, Salt Road Packbeast, Search Party Captain, Sheltering Landscape, Silhana Ledgewalker, Slippery Bogle, Spider-Man, Spirit Link, Thraben Inspector, Utopia Sprawl, Vampire's Kiss, Wind-Scarred Crag | 48 and three tokens | Pauper 69.0% to 80.5% |
| E7a | Legacy Sewer-Cam combo, Rakdos Reanimator, The Epic Storm, Stiflenought, Mardu Energy, Affinity Stompy and Lands: Abhorrent Oculus, Animate Dead, Arena of Glory, Archon of Cruelty, Badlands, Blast Zone, Burning Wish, Cabal Therapy, Chromatic Star, Curie, Dark Depths, Dress Down, Echo of Eons, Emry, Exploration, Gaea's Will, Galvanic Relay, Gamble, Ghost Quarter, Goblin Engineer, Goblin Welder, Grafdigger's Cage, Griselbrand, Icetill Explorer, Kappa Cannoneer, Legion Extruder, Life from the Loam, Maze of Ith, Memory Guardian, Mox Diamond, Patchwork Automaton, Phyrexian Dragon Engine, Phyrexian Dreadnought, Pinnacle Emissary, Raucous Theater, Reanimate, Shadowspear, Shadowy Backstreet, Shallow Grave, Silent Clearing, Skateboard, Song of Creation, Sphere of Resistance, Taiga, The Tabernacle at Pendrell Vale, Thespian's Stage, Unmask, Verdant Catacombs, Wooded Foothills, Yavimaya | 50 and two tokens | Legacy 59.6% to 73.4% |
| E7b | Vintage Dredge, Counter-Vine, Blue Tempo, Lurrus DRS, Doomsday and RUG Control, and the artifact cards of Stiflenought, Sphere Shops and Sultai Psychic Frog: Abrupt Decay, Assassin's Trophy, Avatar of Burgeoning Echoes, Basking Rootwalla, Bayou, Bazaar of Baghdad, Blazing Rootwalla, Carnivorous Cultivator, Chain of Vapor, Collector Ouphe, Commandeer, Creeping Chill, Crucible of Worlds, Dack Fayden, Deathrite Shaman, Demonic Consultation, Golgari Grave-Troll, Golgari Thug, Golos, Grief, Hollow One, Ichorid, Karn, Argent Defender, Kishla Skimmer, Lodestone Golem, Master of Death, Mindbreak Trap, Minsc & Boo, Narcomoeba, Noxious Revival, Phyrexian Revoker, Prized Amalgam, Ragavan, Serum Powder, Shambling Shell, Silversmote Ghoul, Squee, Stinkweed Imp, Stonecoil Serpent, Subtlety, Thorn of Amethyst, Torpor Orb, Uro, Vengevine, Wrenn and Six | 45, two tokens and Enroot | Vintage 80.0% to 95.4% after E7a |
| E8b | Pauper Ruby Storm, Golgari Gardens, Familiars, Jeskai Ephemerate, Glintblade, Izzet Terror, Burn and Boros Bully (and Izzet Faeries, 4c Synthesizer): Accursed Marauder, Azorius Chancery, Contaminated Landscape, Crimson Fleet Commodore, Crossover Collaboration, Crypt Rats, Defile, Desert, Desperate Ritual, First Day of Class, Geothermal Crevice, Glimpse the Impossible, Glorious Gale, God-Pharaoh's Faithful, Goblin Anarchomancer, Goldmire Bridge, Goliath Paladin, Golgari Rot Farm, Haunted Mire, Heaped Harvest, Hickory Woodlot, Khalni Garden, Lonely Arroyo, Manamorphose, Meeting of Minds, Mirran Banesplitter, Mortuary Mire, Oroku Saki, Orzhov Basilica, Perilous Landscape, Rally the Peasants, Rancid Earth, Reckless Lackey, Ride's End, Sandstone Needle, Sea Gate Oracle, Seething Song, Seize the Storm, Skred, Snow-Covered Mountain, Sunscape Familiar, Thornscape Familiar, Twinned Vision, Volatile Fjord, Witch's Cottage, Wrenn's Resolve | 46 and two tokens | Pauper 80.5% to 90.4% |
| E8a | Legacy Death and Taxes (Yorion, black-white and mono-white), Hogaak Moonshadow, Jeskai, Azorius and Dimir Control, Saga and Beseech Storm, Reanimator and 8-Cast: Ad Nauseam, Aether Vial, Back to Basics, Bitter Triumph, Cabal Ritual, Canoptek Scarab Swarm, Chandra, Chill of Compliance, Cloak and Dagger, Dark Matter Manipulator, Dryad Arbor, Emperor of Bones, Erode, Flagstones of Trokair, Flickerwisp, Harbinger of the Seas, Hogaak, Infernal Tutor, Lich's Relic, Lion Sash, Loran of the Third Path, Meteor Sword, Mystic Sanctuary, Once Upon a Time, Overlord of the Balemurk, Pre-War Formalwear, Prismatic Vista, Rain of Filth, Recruiter of the Guard, Skyclave Apparition, Snow-Covered Swamp, Solitude, Stitcher's Supplier, Stoneforge Mystic, Supreme Verdict, Sylvan Safekeeper, Teferi, Time Raveler, Thought Monitor, Valgavoth's Lair, White Orchid Phantom, Wight of the Reliquary, Wishclaw Talisman, Yorion | 42 and two tokens | Legacy 73.4% to 83.5% |
| E9b | Pauper Turbo Fog, Gruul Ponza, Gruul Ramp, Food Gardens, 4c Pestilence and Esper Faeries, Pauper 4c Ephemerate (with E8a's cards), Vintage Scam, Eldrazi, Red Prison (with E8a's Once Upon a Time), Grixis Lurrus and Underworld Breach: Annoyed Altisaur, Arbor Elf, Arcane Denial, Bannerhide Krushok, Barrowgoyf, Black Dragon Gate, Boarding Party, Boggart Trawler, Boomerang Basics, Bridgeworks Battle, Castigate, Cauldron Familiar, Dauthi Voidwalker, Echoing Truth, Eldrazi Repurposer, Elvish Spirit Guide, Fog, Grapple with Death, Growth Spiral, Heritage Reclamation, Highland Forest, Jewel Thief, Mai, Momentum Breaker, Mwonvuli Acid-Moss, Mystical Tutor, Nutrient Block, Oswald Fiddlebender, Pestilence, Pieces of the Puzzle, Radiant Fountain, Ram Through, Respite, Secluded Courtyard, Simic Growth Chamber, Sowing Mycospawn, Step Through, Thermokarst, Thief of Existence, Thornwood Falls, Vampire Sovereign, Vraska's Final Mercy, Wild Growth, You Meet in a Tavern | 44 | Pauper 90.4% to 95.2%, Vintage 95.4% to 98.1% |
| E9a | Legacy Selesnya Depths, Red Stompy, LED Dredge, Cephalid Breakfast and Painter, and with E9b's cards Naya Initiative, Mono-Black Midrange and 60-card Death and Taxes: Anger, Ashen Rider, Blood Moon, Breakthrough, Bridge from Below, Broadside Bombardiers, Caves of Chaos Adventurer, Cephalid Coliseum, Cephalid Illusionist, Chaos Defiler, Echoing Deeps, Elvish Reclaimer, Endurance, Ensnaring Bridge, Eumidian Hatchery, Fury, Goblin Rabblemaster, Green Sun's Zenith, Grindstone, Horizon Canopy, Inquisition of Kozilek, Knight of the Reliquary, Legolas's Quick Reflexes, Loki, God of Mischief, Magus of the Moon, Mishra's Research Desk, Nomads en-Kor, Orim's Chant, Otherworldly Gaze, Ox of Agonas, Painter's Servant, Poxwalkers, Samwise the Stouthearted, Sejiri Steppe, Shatterskull Smashing, Shuko, Simian Spirit Guide, Sundering Eruption, Super Shredder, Tolaria, Undermountain Adventurer | 41 and two tokens | Legacy 83.5% to 90.0% (88.3% before E9b merged) |
| E5 | Pauper to 90%, Legacy and Vintage to 75%, then sideboards | about 350 | |

## Rules primitives

Derived from the missing cards of each wave's decks. Already in the engine and reused:
extra turns, initiative and dungeons, monarch, Phyrexian mana, delve, planeswalkers,
Sagas, transform, temporary control, flashback, affinity, ninjutsu, kicker, madness.

New, each built once in the first wave that needs it:

- E1: energy counters; replicate; cast-from-graveyard conditions (Plunder the Trollshaws);
  karoo lands; two-target spells. Built in `src/eternal_cards_v1.rs` and
  `build_eternal_v1.rs`: energy is `GameState::eternal_v1` with the
  `CostComponent::PayEnergyV1` cost; replicate announces its payment count like X and
  adds one cast trigger that copies the spell per payment; `StandardTargetV1::TwoCreaturesV1`
  lets two target words name one creature; `ManaAbilityAmountDef::FixedColorsV1` adds a
  fixed color bundle; the karoo return is a land choice on resolution.
- E2, built in `src/eternal_cards_v1.rs` and `build_eternal_v1.rs`: original duals use
  the existing two-type land mana; `LibraryCardFilter::CardWithAnySubtypeV1` lets
  fetchlands find any card with either land type; alternative costs that return a
  permanent (Daze) or exile a colored card from hand (Force of Will,
  `CostComponent::ExileCardFromHandWithColorV1`) are staged like other cast object
  costs; Nethergoyf's escape exiles cards until four card types are covered
  (`CostComponent::ExileOtherGraveyardCardsWithCardTypesV1`); delayed upkeep draws
  and Tamiyo's "until your next turn" attack trigger are delayed triggers owned by
  their source; delirium, revolt and Bowmasters' first-draw exception read live state
  or event history; planeswalker back faces enter with their loyalty; Bilbo casts from
  the graveyard through the resolution-cast path with costs paid. Moxen, Lotus,
  restricted mana and seeded coin flips move to E4.
- E3, built in `src/eternal_cards_v1/e3.rs`, `e3/dungeons.rs` and `build_eternal_v1.rs`:
  one typed resolution choice (`E3ChoiceV1`) that the interpreter stages through
  `EffectTargetSelectionPurpose::EternalV1`; targeted Storm copies that may choose new
  targets (Flusterstorm); spell retargeting (Misdirection); Doomsday and Thassa's Oracle
  library construction; Show and Tell's choices in turn order; free hand casts
  (`HandFreeCastV1`: Omniscience, Aluren with its flash timing); Sneak Attack's haste and
  end-step sacrifice; Emrakul's uncounterable cast trigger owning an extra turn like
  Temporal Manipulation, protection from colored spells, annihilator and graveyard
  shuffle; venturing through Lost Mine of Phandelver, Dungeon of the Mad Mage and Tomb
  of Annihilation (rooms sourced by the venturing card, completed on entering the
  bottommost room, Mad Wizard's Lair casting from hand during the resolution); modal
  double-faced land backs (Sink into Stupor); as-enters choices (Cavern of Souls'
  creature type and its uncounterable restricted mana, Soporific Springs); imprint
  (Chrome Mox); Lion's Eye Diamond's instant-speed mana; The One Ring's protection and
  burden counters. Chalice of the Void, naming a card and cost-tax statics are left to
  later waves.
- E4, built in `src/eternal_vintage_v1.rs` and `build_eternal_vintage_v1.rs`: fixed
  multi-mana rocks use the conditional tap yield (Tolarian Academy adds nothing without
  an artifact); Black Lotus and Coveted Jewel are rich mana abilities; Mana Crypt flips a
  seeded coin; Workshop's mana becomes restricted units only artifact spells spend
  (`ManaSpendRestrictionDef::ArtifactSpellV1`); Time Walk joins Temporal Manipulation's
  extra-turn scheduler and Time Vault's ability is scheduled the same way, while Time
  Vault's skip is chosen at the end step before its controller's turn and applied when
  that turn would begin; "doesn't untap" for both Vaults; Coveted Jewel's unblocked-attack
  trigger and Mana Vault's draw-step trigger are queued by engine hooks; Vexing Bauble and
  Void Mirror read the mana each cast spent; Trinisphere raises a spell's total to three;
  Archon of Emeria's cast limit and tapped nonbasic lands; Clarion Conqueror and Karn's
  "can't be activated" statics; Karn's wish reads S3's outside-the-game zone beside
  exile (empty in best-of-one mainboard sessions); Urza's Saga gains its
  chapter abilities; Phyrexian Metamorph enters as a copy the way Assimilation Aegis
  copies; Witch Enchanter's land face reuses E3's modal land back (Sink into Stupor);
  Anointed Peacekeeper names any registry card by letter, then name; Chancellor of the
  Annex's opening-hand reveal is offered as the game's first upkeep begins. Companions
  (reveal, pay 3, put into hand) and Lurrus's graveyard casting move to E4b.
- E4b, built in `src/eternal_lurrus_v1.rs` and `build_eternal_lurrus_v1.rs`: a custom
  deck may name a companion (`CustomDeckV1::companion`, absent from the wire and every
  hash when unset), checked against its deck condition at reset and placed face up in
  exile, where observation v5 shows it; putting it into hand for {3} at sorcery timing is
  a special action offered as the companion's ability 0. Lurrus's once-per-turn
  graveyard permission is a `GraveyardPermissionV1` cast route tracked per Lurrus.
  Snapcaster Mage's flashback and Underworld Breach's escape are granted costs equal to
  the card's mana cost (plus three graveyard cards for escape); when several graveyard
  routes apply, one is offered (flashback, then permission, then escape). New cost
  families: returning N permanents with a filter as an alternative cost (Gush,
  `CostComponent::ReturnControlledPermanentsToOwnersHandV1`), "not your turn" alternative
  cost conditions (Force of Negation) and exiling graveyard cards for an activation
  (Psychic Frog). Converge reads the colors a cast spent (Prismatic Ending); per-slot
  two-target modes let a Wizard's controller choose two Flame of Anor modes; Mana Drain
  and Forth Eorlingas! queue delayed triggers from main-phase and combat-damage hooks;
  Hexing Squelcher grants "Ward--Pay 2 life" and makes its controller's spells
  uncounterable; Null Rod and Pithing Needle prohibit activations (the Needle names a
  card visible to its controller); Mox Opal's metalcraft gates its mana ability.
- E5b, built in `src/eternal_legacy_e5b_v1.rs` and `build_eternal_e5b_v1.rs`: casting
  during resolution with a cost (miracle, Amped Raptor) pays the mana or energy first,
  then casts through a typed selection; a miracle card's first draw offers its owner a
  reveal decision before its trigger is queued. Eldrazi Temple's second mana is a
  restricted unit only colorless Eldrazi spells spend
  (`ManaSpendRestrictionDef::ColorlessEldraziSpellV1`); sacrificing a creature is a mana
  ability cost (`ManaAbilityCostDef::TapSelfSacrificeControlledCreatureV1`, Lazotep
  Quarry), and an activation's X can read a graveyard card's mana value. Lazotep's
  copies and Guide of Souls' flying counter use the creature upgrade layer. Thought-Knot
  Seer reuses the linked hand exile; Leyline Binding and Static Prison reuse exile until
  the source leaves. New target shapes: up to one other nonland permanent, a triggered
  ability or colorless spell (Consign to Memory), and creatures then nonland permanents
  (Eldrazi Confluence). Reality Smasher's "becomes the target" trigger, Quantum Riddler's
  draw replacement, Voice of Victory's cast prohibition, Petrified Hamlet's named land
  (abilities off, taps for colorless), Static Prison's upkeep payment and Ocelot Pride's
  city's blessing are engine hooks. Wastescape Battlemage's {1}{U} kicker is modeled as
  an alternative cost of {2}{C}{U}, so a free cast cannot add only that kicker.
- E6a, built in `src/eternal_pauper_e6a_v1.rs` and `build_eternal_e6a_v1.rs`: fogs are
  turn-bound prevention records the damage replacement pass reads (all combat damage, or
  all damage creatures would deal); Tangle also marks each attacking creature to skip its
  next untap. Flicker exiles, then returns under the owner (Ephemerate) or the spell's
  controller (Ghostly Flicker, with a new two-distinct-target shape over artifacts,
  creatures and lands you control). Rebound marks a resolving spell cast from hand, the
  spell departure hook exiles it, and its owner's next upkeep queues a trigger that offers
  E5b's paid resolution cast. Evoke is an alternative cost plus an entry trigger whose
  condition reads the recorded evoked cast. Foretell is a special action offered as the
  hand card's ability 0 (as companions are): it pays {2} during its owner's turn, exiles
  the card face down (only its owner may look) and grants a later-turn exile cast
  permission; `AltCostCondition::ForetoldV1` and a quote hook make the foretell cost the
  only way to cast it from exile. Bender's Waterskin untaps in the other player's untap
  step; Tithing Blade's craft takes a creature as material
  (`ExileCraftArtifactMaterial` reads the material type per card); Cryoshatter's trigger
  on its creature becoming tapped or being dealt damage is queued by a hook. Other new
  shapes: sacrificing a blue permanent (`PermanentFilter::BlueV1`), an instant-or-flash
  library search (`LibraryCardFilter::InstantOrFlashV1`) and Stream of Thought's choice
  of up to four graveyard cards to shuffle back (`E3ChoiceKindV1::E6aV1`).
- E7a, built in `src/eternal_legacy_e7a_v1.rs` and `build_eternal_e7a_v1.rs`: land
  play limits and graveyard land plays (Exploration, Icetill Explorer, Song of Creation)
  are engine hooks, as are Sphere of Resistance's spell tax, Grafdigger's Cage's entry
  replacement and cast prohibition, Maze of Ith's two-way combat damage prevention,
  Shadowspear's keyword loss, Dress Down's removal timestamp and Yavimaya's granted
  Forest type and {G}. "Becomes a copy" of a battlefield permanent swaps the card
  definition and keeps a copy record that reverts before the permanent leaves;
  Thespian's Stage keeps its own ability one past the copied ability list, and Curie
  keeps its draw trigger. Dark Depths' ice counters live in module state and its state
  trigger is pushed directly to the pending triggers. Animate Dead enters enchanting a
  graveyard card (an SBA exemption) and its entry trigger returns and attaches it.
  Life from the Loam's dredge 3 is E7b's dredge (`eternal_vintage_e7b_v1::dredge`), and
  Sphere of Resistance's tax shares E7b's spell-tax hook; Mox Diamond's land discard is
  an entry-choice decision, just after it enters. Suspend (Gaea's
  Will) and Arena of Glory's exert mana are no-stack actions offered as ability 0;
  Arena's {R}{R} goes to the restricted pool so a creature spell paid with it gains
  haste. Manifest dread puts a face-down 2/2 without ward. Improvise is the convoke
  payment over artifacts paying generic mana only. Blast Zone's X is one ability per X
  from 0 to 7. Burning Wish reads S3's outside-the-game zone. Cabal Therapy names a card
  by first letter, then name. Resolution choices (Ghost Quarter, Gamble, Goblin
  Engineer, Archon of Cruelty, Phyrexian Dreadnought, manifest dread) use
  `E3ChoiceKindV1::E7aV1`.
- E5a, built in `src/eternal_e5a_v1.rs` (with `choices`, `permissions`, `phasing`,
  `statics` and `triggers` submodules) and `build_eternal_e5a_v1.rs`: casting the top
  card of the library is a new cast route (`SpellCastRouteV4::LibraryPermissionV1`,
  recorded per library incarnation so a spell keeps it on the stack); Mystic Forge
  allows artifact and colorless spells, Bolas's Citadel any spell for life equal to its
  mana value (the mana cost is waived like Omniscience's, X is 0) and land plays from
  the top. Yawgmoth's Will joins the graveyard cast grants, adds graveyard land plays
  and exiles cards headed to its controller's graveyard through the zone-change
  replacement hook Suplex's "exile it instead" also uses. Phasing moves a permanent and
  its attachments out of every zone list under the command-zone marker, keeping its
  incarnation, until its controller's next untap step. Serra's Emissary's protection
  from a card type covers targeting, damage and blocking for its controller and their
  creatures. Oko's Elks lose their abilities through the removal timestamp, become
  creatures only, green Elks and base 3/3. Chalice of the Void enters with X charge
  counters and reuses Vexing Bauble's "counter that spell" binding. Kozilek's Command's
  six pairs of modes use per-slot targets, and X may not be less than the targeted
  creature's mana value or the number of graveyard targets (X is announced after
  targets here). The E3 choice framework carries this wave's resolution choices
  (Beseech the Mirror's search and free cast from the hand, Flash, Vampiric Tutor,
  Ugin's search). Oath of Druids and Forbidden Orchard are hook-queued triggers;
  Urza's Workshop's two colorless abilities are one with the larger yield.
  Extrapolate the Impossible, like Karn's wish, reads S3's outside-the-game zone (empty
  in a best-of-one mainboard game). Kozilek's Command is an Eldrazi instant; the Kindred card
  type is not modeled.
- E8b, built in `src/eternal_pauper_e8b_v1.rs` and `build_eternal_e8b_v1.rs`: splice
  onto Arcane is announced like replicate's count (the spliced Desperate Rituals in hand,
  capped by that count), paid as one {1}{R} per splice, revealed to the opponent, and
  every Arcane spell's program ends with the spliced "Add {R}{R}{R}" leaf. Teamwork is an
  alternative cost (`AltCostCondition::TeamworkV1`) whose creatures, total power N or
  more, are chosen through the convoke selection and pay no mana; the spell reads its
  finalized method. Static reductions by spell color (Goblin Anarchomancer, the
  Familiars) join each selected cost's own reduction, so convoke, flashback and
  alternative costs get them too; Ride's End's reduction is a target-dependent
  `DynamicCountDef::SpellTargetsTappedPermanentV1`. Depletion counters live in module
  state: the lands enter with two, their rich mana ability is blocked without one and
  removes one after tapping, then sacrifices the land at zero. "The Ring tempts you"
  keeps a per-player level and Ring-bearer: legendary (an `effective_supertypes` hook)
  and unblockable by greater power, the attack loot, the end-of-combat sacrifice of its
  blockers (a new end-of-combat step hook) and the combat damage drain. First Day of
  Class registers an "enters this turn" trigger; Glimpse the Impossible's next-end-step
  trigger moves its still-exiled cards and makes a Spawn for each; Heaped Harvest's
  "when you sacrifice it" reads `Sacrificed` events; Seize the Storm's Elemental is a
  characteristic-defining token; Desert's ability is gated to the end of combat step;
  Twinned Vision's discard belongs to its flashback only and it reads the cast origin;
  Geothermal Crevice's sacrifice ability is an additional fixed-bundle mana ability.
  Crypt Rats' X is one ability per X from 1 to 10, each costing X black mana. Learn
  reads S3's outside-the-game zone for a Lesson, or discards a card to draw a card.
- E6b, built in `src/eternal_pauper_e6b_v1.rs` and `build_eternal_e6b_v1.rs`: creature
  Auras reuse `AuraCreatureStatic`, and an engine hook multiplies Ancestral Mask's and
  Ethereal Armor's bonus by their enchantment counts. Auras that enchant a land or a
  creature you control (Abundant Growth, Utopia Sprawl, Cartouche of Solidarity) use the
  Standard Aura entry with an enchant-rule hook; Abundant Growth grants its land an
  any-color mana ability through the granted-mana hook, and Utopia Sprawl adds its chosen
  color whenever its Forest is tapped for mana (a triggered mana ability, resolved at
  once). "As this Aura enters, choose a color" is an option list before the Aura enters.
  Benevolent Blessing's protection from a color is checked for targeting, blocking and
  damage, and its 704.5m check keeps the Blessing controller's own Auras and Equipment.
  Spirit Link's and Armadillo Cloak's damage triggers and Rancor's return are queued or
  ordinary triggers. New alternative-cost conditions: web-slinging (return a tapped
  creature), sneak (castable during the declare blockers step by returning an unblocked
  attacker; the spell enters tapped and attacking) and disturb (a graveyard permission
  whose only cost is the disturb cost; the spell enters transformed, and Luminous Phantom
  is exiled instead of going to a graveyard). `CostComponent::TapUntappedControlledPermanentsV1`
  pays Battle Screech's flashback; Demand Answers' "sacrifice an artifact or discard a
  card" is its discard additional cost, or an alternative cost of the same mana plus the
  sacrifice that skips the discard. Connive and Malevolent Rumble's reveal use
  `E3ChoiceKindV1::E6bV1`; endure is an option list; Dawnbringer Cleric is a modal entry
  trigger; Greatsword of Tyr's attack trigger, Idyllic Grange's "enters untapped" and
  Martyr's Soul's "no tapped lands" are E6b trigger conditions. New target shapes: a
  Forest, and up to one creature the defending player controls.
- E7b, built in `src/eternal_vintage_e7b_v1.rs` (with `dredge`, `statics` and `targets`
  submodules) and `build_eternal_e7b_v1.rs`: dredge (702.52) replaces any draw an effect
  makes inside the resumable interpreter (a drawing program moves there while some
  player can dredge) with a typed choice, and the draw step's draw waits as an as-enters
  style decision; the mill is one batch, then the card returns to hand. Cards milled from
  the library trigger from the graveyard (Narcomoeba, Creeping Chill); graveyard upkeep
  and end-step triggers return Ichorid, Squee, Master of Death and Silversmote Ghoul;
  Prized Amalgam, Vengevine and Kishla Skimmer are hook-queued triggers, and "the next end
  step" returns (Amalgam, dash) wait in state. Evoke by exiling a colored card is an
  alternative cost plus E6a's evoked-entry sacrifice (Grief, Subtlety); dash and "an
  opponent cast three spells" are new alternative-cost conditions, and
  `CostComponent::ExileCardsFromHandWithColorV1` pays Commandeer. Commandeer changes a
  spell's controller (its cast route keeps the caster) and offers each target slot again;
  Mindbreak Trap exiles spells; Subtlety's spell departure can go to the library bottom;
  Chain of Vapor copies the resolving spell for the permanent's controller. Statics:
  Thorn of Amethyst and Lodestone Golem tax spells by type, Torpor Orb and Karn, Argent
  Defender suppress entry triggers in the trigger collection, Collector Ouphe and
  Phyrexian Revoker prohibit activations (mana abilities included), Crucible of Worlds
  adds graveyard land plays, Stonecoil Serpent has protection from multicolored for
  targeting, blocking and damage, and Golgari Grave-Troll's regeneration shield replaces
  destruction by effects and by lethal damage. Planeswalkers: Dack Fayden's and Wrenn and
  Six's emblems, Minsc & Boo's reflexive trigger, empower Jace's planeswalker token, and
  Avatar of Burgeoning Echoes' -10 granted one index past a planeswalker's printed
  abilities. Hollow One counts discards (cycling included) per turn. New target shapes
  cover graveyard cards of a type, any number of spells, a creature or planeswalker spell,
  a permanent an opponent controls and a creature with trample or haste. Serum Powder's
  exile-and-redraw is offered before each London mulligan choice while it is in hand.
  Carnivorous Cultivator enters prepared: a copy of its prepare spell (Enroot, a registry
  card in no deck) is created in exile, castable by the creature's controller through an
  exile play permission; casting it unprepares the creature, and the copy ceases to exist
  when the creature leaves or the copy leaves exile and the stack. Library searches gained
  a graveyard destination. Wrenn and Six's emblem grants retrace as a graveyard cast permission whose quote adds
  `CostComponent::DiscardLandCardsV1` (a land card chosen privately from hand); a card
  another permission already lets its owner cast (Snapcaster's flashback, Yawgmoth's
  Will) is cast through that permission instead, and retrace is not combined with a
  second object-choosing cost (an alternative or additional cost that picks objects).
- E8a, built in `src/eternal_legacy_e8a_v1.rs` (with `statics` and `targets` submodules)
  and `build_eternal_e8a_v1.rs`: Yorion's companion condition (80 cards) reuses the Lurrus
  companion action. "Return at the beginning of the next end step" (Flickerwisp, Yorion)
  and Emperor of Bones' end-step sacrifice wait in module state and are queued as delayed
  triggers; one created during an end step waits for the next. Hogaak's "you can't spend
  mana" is a convoke alternative cost whose payment taps creatures for its hybrid pips and
  generic mana and delves graveyard cards for the rest, plus a graveyard cast permission.
  Teferi, Time Raveler restricts opponents' cast timing and gives sorcery flash; Harbinger
  of the Seas reuses the removal timestamp and land subtype hooks and grants {U}; Back to
  Basics is an untap-step prohibition; Rain of Filth grants its lands a sacrifice mana
  ability one index past their printed abilities, performed without the stack. Chandra,
  Chill of Compliance adds restricted mana (`ManaSpendRestrictionDef::NoncreatureSpellV1`),
  stun counters and a spell-cast emblem. Once Upon a Time adds
  `AltCostCondition::FirstSpellThisGameV1`. Reconfigure (Lion Sash) is attach and unattach
  abilities with a type override while attached; Wishclaw Talisman's wish counters live in
  module state and are removed as the ability is paid. Cloak and Dagger's target pair (an
  opponent, then up to one of their creatures) is a per-slot target shape, and its exile
  lasts until it leaves the battlefield; Skyclave Apparition's Illusion takes its power and
  toughness from the exiled card's mana value. Resolution choices (Infernal Tutor,
  Stoneforge Mystic, Recruiter of the Guard, Aether Vial, Yorion, Emperor of Bones, Erode,
  Overlord of the Balemurk, Wishclaw Talisman) use `E3ChoiceKindV1::E8aV1`. Valgavoth's
  Lair's color is an entry-choice decision.
- E9b, built in `src/eternal_e9b_v1.rs` (with a `targets` submodule) and
  `build_eternal_e9b_v1.rs`: cascade is a cast trigger that exiles until a nonland card
  with a lower mana value, offers E5b's resolution cast without paying its mana cost,
  then puts every card still exiled on the bottom in a random order. Dauthi Voidwalker's
  replacement joins the zone-change hook Yawgmoth's Will uses and records void counters
  by exile incarnation; its ability grants a free exile play permission (a land play for
  a land). Shadow is a blocker rule (`blocker_allowed`). Elvish Spirit Guide's mana
  ability is performed without the stack, offered as its hand ability 0 like Arena of
  Glory's. Mai's "whenever a player casts", Barrowgoyf's combat damage mill, Pestilence's
  end step sacrifice, Thief of Existence's gained leaves trigger and Arcane Denial's
  next-upkeep draws are hook-queued triggers; Barrowgoyf's stats are a
  characteristic-defining ability over all graveyards. Momentum Breaker's "Start your
  engines!" is the Standard speed designation, so speed increases through its trigger.
  Sowing Mycospawn's kicked cast trigger reuses E5b's kicked cast condition; Wild Growth
  joins E6b's land Auras and Utopia Sprawl's mana hook; Boggart Trawler and Bridgeworks
  Battle reuse E3's modal double-faced land backs. Secluded Courtyard chooses its creature
  type as Cavern of Souls does (from the creature cards its controller owns) and its
  restricted mana pays for creature spells of that type the same way, or for an ability of
  a creature source of that type through the activation planner. New target shapes: a creature you
  control then up to one you don't, two different creatures, and up to one opponent's
  noncreature, nonland permanent with mana value N or less; new library filters: an
  instant or sorcery card and a card with a subtype (wizardcycling). Resolution choices
  use `E3ChoiceKindV1::E9bV1`. Arcum's Astrolabe is not in this wave (its {S} cost needs
  mana from a snow source, which the mana pool does not track), so Vintage Lurrus Vault
  Key stays unplayable.

- E9a, built in `src/eternal_legacy_e9a_v1.rs` (with `statics` and `targets` submodules)
  and `build_eternal_e9a_v1.rs`: divided damage is announced with the targets, one point
  per pick (601.2d): Fury picks four times among creatures and planeswalkers, a creature
  picked again taking another point; Shatterskull Smashing, whose X is announced after its
  targets here, picks its first target k times and may pick a second target once, which is
  dealt the rest, and X may not leave the second target nothing (`minimum_x`; at most 16
  picks). Blood Moon and Magus of the Moon share wave E8a's land-type change with Harbinger
  of the Seas, the later timestamp winning. Painter's Servant's color is an as-enters choice
  (Valgavoth's Lair's) that `object_color_mask`, the Blasts' color checks and the other
  direct color reads add. Split second blocks casting and every activation but mana
  abilities and special actions. Cephalid Illusionist's and Loki's "becomes the target"
  triggers read each finished targeting, players included; Legolas's Quick Reflexes grants a
  "becomes tapped" trigger to any creature. Nomads en-Kor's shield moves one point of the
  next damage to its target; Cephalid Illusionist reuses the fog-style combat prevention;
  Sejiri Steppe adds to Benevolent Blessing's protection mask. Ensnaring Bridge and kicked
  Orim's Chant are attack restrictions, Orim's Chant and split second cast prohibitions,
  Goblin Rabblemaster a combat requirement and Sundering Eruption a block restriction.
  Echoing Deeps' copy is chosen before trigger collection, so the copied land's entry
  triggers and counters apply. Eumidian Hatchery's hatchling counters, Ox of Agonas' escape
  counter, Green Sun's Zenith's shuffle into the library (a spell-departure hook), Simian
  Spirit Guide's hand mana (a special action like Elvish Spirit Guide's), Grindstone's
  repeated mill, Breakthrough's keep and Mishra's Research Desk's play permission are
  module hooks; Anger's haste works from the graveyard. Resolution choices use
  `E3ChoiceKindV1::E9aV1`. Tolaria's banding loss changes nothing: no card here has
  banding.

- S1a, built in `src/eternal_sideboard_s1a_v1.rs` (with `statics` and `targets` submodules)
  and `build_eternal_s1a_v1.rs`, linked late in the recipe chains (before S1b and E9b): Legacy
  sideboard cards. Rest in Peace, Leyline of the Void and Containment Priest join the zone-change
  replacement hook Dauthi Voidwalker uses (exile instead of a graveyard; exile a nontoken
  creature entering from anywhere but the stack). Leyline's opening-hand permission is offered
  as the first upkeep begins, as Chancellor of the Annex's reveal is. Disruptor Flute names a
  card as Anointed Peacekeeper does: spells with the name cost {3} more (own-spell increase) and
  its non-mana activated abilities are prohibited like Pithing Needle's. Choke is an untap-step
  prohibition, Defense Grid a spell tax by the caster's turn, Dread of Night a power/toughness
  modifier. Mass effects (Rough // Tumble, a split card through the Adventure form; Brotherhood's
  End; Meltdown; Serenity; Massacre's -2/-2 on exact incarnations) are `S1aOpV1` leaves, and
  Massacre adds `AltCostCondition::OpponentPlainsYouSwampV1`. Lavinia's cast restriction is a
  cast prohibition (X judged as 0), and her counter reads the mana a cast spent
  (`payment.commit`), recorded only while she is in play. Carpet of Flowers is queued by the
  main-phase hook as a choice of color and amount, clamped to the Islands at resolution.
  Surgical Extraction now offers a native optional subset of matching cards in the owner's
  graveyard, hand and library, then shuffles even when zero cards were chosen. Its scoped
  JSONL search look contains the chooser's full hand and library name multisets; surviving
  hand knowledge uses the existing observer-only ledger. Native qualification is pending.
  New target shapes: noncreature artifact or enchantment, black or red permanent,
  creature with power or toughness 1 or less, up to two artifacts and/or enchantments, an
  enchantment, instant or sorcery spell, and a non-basic-land card in a graveyard.

  Partial: Surgical Extraction remains unqualified for ordinary Full-deck admission while
  its native choice, restore, disclosure and transport checks are pending. Frozen infoset
  and search consumers refuse its active search. Lavinia's source repair checks selected X and spell
  characteristics in the shared payment quote, including casts made during resolution. Its paid
  both-seat X, Bestow, split, overload, Room, cascade, Madness and graveyard-cast checks remain unqualified;
  Lavinia stays Partial. Mana value ignores alternative payments and includes announced X, as stated
  in the [Ravnica Remastered rulings](https://media.wizards.com/2023/downloads/RVR_Release_Notes_XrN0zlJaFn/EN_MTGRVR_ReleaseNotes_20231120.pdf).
  The native opening profile puts Leyline of the Void's opening action before
  the first turn under rule 103.6. Legacy starts retain their first-upkeep behavior.
  [Comprehensive Rules](https://media.wizards.com/2026/downloads/MagicCompRules%2020261106.txt)

  Left out of the wave: Damping Sphere (its land replacement spans the mana planner and
  every mana yield path), Collective
  Brutality (escalate), Stronghold Gambit, Alpine Moon, Ghost Vacuum, Drop of Honey, Triumph
  of Saint Katherine (miracle is Terminus-only and its cost needs generic mana) and From the
  Ashes.
- S1b, built in `src/eternal_sideboard_s1b_v1.rs` (with a `targets` submodule) and
  `build_eternal_s1b_v1.rs`, linked last in the recipe chains (after S1a, before E9b; E6a's
  kicker cost after S1d; E7a's alternative and ward costs): 26 of the 37 remaining Legacy
  sideboard cards, the ones whose programs fit the existing primitives. Storm gains Grapeshot and Empty the Warrens
  (E3's list) and dredge gains Shenanigans (E7b's table). Toxic Deluge's "pay X life" is
  announced like Sickening Dreams' discard count (X up to the caster's life) and the life is
  paid as the cast completes. Stony Silence reuses Null Rod's activation prohibition, and
  Metallic Rebuke joins Kappa Cannoneer's improvise. Deafening Silence counts noncreature
  spells per player and turn in module state (spells cast before it entered count);
  Gaddock Teeg (new Kithkin and Advisor subtypes, stable ids 320 and 321), Meddling Mage and
  Deafening Silence add to the E7a cast prohibition hook. Meddling Mage names a nonland card as
  its resolving creature spell enters (Anointed Peacekeeper's letter, then name choice), so a
  Mage that enters any other way names nothing. Sailors' Bane's reduction joins the spell cost
  reductions of E8b and P1a. Temporary Lockdown and Perilous Snare use the linked "until this
  leaves" exile (Lockdown records one link per exiled permanent). Perilous Snare starts the
  Standard speed designation and its counter ability is gated to speed 4 by the activation
  restriction hook. Tourach, Dread Cantor sees opponents' discards through the E7b discard
  hook, its kicker uses E6a's kicker table, and protection from white joins the printed
  protection mask. Extirpate's split second joins Legolas's Quick Reflexes' check. Leyline of
  Sanctity is offered from an opening hand as the first upkeep begins (Chancellor of the
  Annex's shape) and gives its controller hexproof through the E3 player targeting check.
  Engineered Explosives records the colors its cast spent (Prismatic Ending's hook) as its
  sunburst charge counters and keeps the count for the sacrificed incarnation's ability.
  Liquimetal Coating records permanents that are artifacts this turn; the effective type
  check reads it. New target filters: four-point divided damage among creatures (Pyrokinesis)
  and a creature or Vehicle you control (Perilous Snare); Stern Dismissal and Extirpate use
  S1c's and S1a's filters. Meddling Mage, Extirpate and Leyline of Sanctity are Partial.
  The Meddling Mage candidate asks for a name before committing the actual entry.
  A resumed entry checks the original operation, confirmed catalog choice, controller,
  and native entry occurrence. The named-spell restriction uses the same quote and
  payment gates for ordinary casts and resolution casts. The revised native menu names
  individual split halves and Room doors, alternate Adventure and Omen spells, and
  nonland back faces in the executable registry. It checks the selected spell name,
  including disturb. Existing authenticated numeric choices retain their original
  menu, serialization and hash. A legacy combined split name remains its literal
  combined string and matches neither cast half. Names outside the registry
  and the frozen observation paths remain outside this change. A pre-entry-owner
  save without authenticated choice evidence still requires its historical engine. Leyline has
  the same native opening path as Leyline of the Void.
  A paid physical Phyrexian Metamorph copying a visible Mage chooses its own name
  before entry. The entry keeps Metamorph's physical cast receipt, commits the Mage
  characteristics and artifact type, and restores Metamorph's printing on departure.
  Paid Reanimate uses a native pre-entry copy choice across both battlefields,
  then installs the supported face-up Mage or Wall of Blossoms form before the
  Mage name choice or the Wall's draw trigger. An unsupported legal copy
  refuses the entire prompt. The saved copy binds the actual Reanimate cast
  and entry window. The native runner authenticates its original graveyard
  target during execution. Under the public native Eternal opening profile,
  Aether Vial with four charge counters uses the same prospective copy owner.
  The native replay binds the actual paid tap, reads the original live or
  last-known charge count at resolution, and authenticates the controller's
  private hand choice. Its supported face-up donors are Mage, Wall of Blossoms and the
  printed Aether Vial. A Vial copy enters with zero counters. Frozen viewers
  refuse the native profile from setup, independently of the hand or its
  choice. Other noncast copy-entry producers remain outside this path.
  Dress Down suppresses the physical copy choice before entry. Native
  qualification is pending.
  Extirpate uses the shared extraction continuation to require all
  public graveyard matches, offer any subset of the hidden matches, and provide the caster
  a private look at the full searched hand and library. Native qualification of this
  repair is pending. Rule 701.23 allows fail-to-find in the hidden zones.
  [Comprehensive Rules](https://media.wizards.com/2026/downloads/MagicCompRules%2020261106.txt)
  Not in this wave: Mycosynth Lattice (mana of any color needs a payment-planner change),
  Extinguisher Battleship (station), Cityscape Leveler (unearth, Powerstone), Invasion
  Submersible (waterbend, exhaust), Summon: Bahamut, Keen-Eyed Curator, Outland Liberator
  (day and night), Fiery Confluence (choose three with repeats and per-instance targets),
  Kiora's Dismissal (strive), Eowyn (a target filter that reads the source's power) and
  Unlicensed Hearse (a single-graveyard target filter).
- S1c (issue #365, Vintage-only sideboard cards), built in `src/eternal_sideboard_s1c_v1.rs`
  (with a `targets` submodule) and `build_eternal_s1c_v1.rs`, linked late in the recipe
  chains (after E8a and S1d, before S1a, S1b and E9b). Bounce-all (Hurkyl's Recall, Rebuild with
  its cycling), Virtue's Ruin's destroy-all, Feed the Swarm's life loss (the destroyed
  permanent's mana value), Lithomantic Barrage's 5 damage to a white and/or blue target,
  Dark Petition's spell mastery mana and Wurmcoil Engine's two tokens (deathtouch and
  lifelink Wurms) are module leaves. Spell Snare is a mana value 2 spell target; Mystical
  Dispute's {2} reduction is `DynamicCountDef::SpellTargetsBlueSpellV1`; Snapback reuses
  the blue-card exile alternative cost; Ravenous Trap adds
  `AltCostCondition::OpponentThreeCardsToGraveyardThisTurnV1`, which counts the
  opponent's cards moved to a graveyard since the last upkeep began. New target shapes
  (`StandardTargetV1`): a creature or planeswalker with mana value N or less (Long
  Goodbye) and a creature or enchantment an opponent controls (Feed the Swarm). Light of
  Day's attack and block ban are engine hooks, Urborg extends Yavimaya's extra land type
  and mana ability to Swamps, and Sphinx of the Steel Wind adds protection from red and
  green to the color protection mask. Hullbreacher replaces an opponent's draw (except
  their first actual draw in their draw step, tracked by Bowmasters' draw ordinal) with a
  Treasure for its controller in `propose_and_commit_inner`. Argentum Masticore's upkeep trigger is
  two selections (`E3ChoiceKindV1::S1cV1`): the card to discard (or none, sacrificing
  it), then the reflexive destroy target among the opponent's nonland permanents with
  mana value at most the discarded card's; it shares Stonecoil Serpent's protection from
  multicolored and uses the Masticore creature type (stable id 260 from P2b). Not in this wave:
  Archon of Absolution and Overencumbered (attack costs and taxes have no engine path),
  Azure Beastbinder, Unable to Scream and Suspend (timed or aura-based ability removal
  with new characteristics), Apostle's Blessing (protection from artifacts), Sickening
  Shoal (X tied to an exiled card), Darksteel Angel (cannot lose the game) and Portal to
  Phyrexia (subtype grant on reanimation).
  Partial: the drawing player can choose dredge or let Hullbreacher replace the draw (616.1) on
  resumable draw programs, including the monarch's draw, both Baubles' delayed
  upkeep draws, Coveted Jewel's entry and unblocked combat draws, Paradoxical
  Outcome, Psychic Frog, Plunder the Trollshaws, Tamiyo's -7, Twinned Vision,
  The Endstone's land-play draw, Kishla Skimmer's graveyard-departure draw,
  Chandra, Chill of Compliance's emblem draw, Bonder's Ornament, and both
  selected-discard draw paths. Sensei's Divining Top moves only the original
  permanent to its owner's library after its draw attempt, including when a
  replacement prevents the draw. The One Ring counts its live burden increment
  or the departed incarnation's retained burden once before its first attempt.
  Veil of Summer applies its protection after the conditional draw attempt, or
  immediately when the draw condition is false. Phyrexian Dragon Engine
  discards the hand once after its original optional choice, then checks for
  replacements using the resulting graveyard. A terminal Jace win skips
  Top's move, Veil's protection and remaining Ring or Dragon Engine attempts,
  with or without a replacement choice. The Jewel
  combat continuation binds draw progress to the original trigger and transfers
  only its original battlefield incarnation after the third attempt. Outcome
  returns its legal targets in one batch and derives draw progress from actual
  original-incarnation arrivals into the spell controller's hand. Both progress
  checks validate continuation consistency. Plunder and Twinned Vision retain
  their cast-origin conditions. Tamiyo's count uses the library size when her
  ability starts, and the hand-size effect follows the last draw attempt if
  play continues. The owned draw instructions journal their start and
  actual completed attempts when the replacement continuation runs. Restore
  checks saved progress against that native evidence.
  An affected player can also choose between Jace's empty-library win and
  Hullbreacher on fixed-count draw instructions and the covered owned draws.
  Niv-Mizzet, Visionary and Hullbreacher are in separate shipped registries.
  Their damage-draw interaction has no native qualification.
  Some specialized synchronous draws and unbound generated or dynamic draw instructions
  still lack a replacement choice. Quantum Riddler's extra draws stop at a
  terminal Jace win. They are committed inside one shared draw event and still
  lack individual replacement choices. Native qualification of the repairs is
  pending.
- S1d, built in `src/eternal_sideboard_s1d_v1.rs` (with a `targets` submodule and
  `statics`) and `build_eternal_s1d_v1.rs`, linked late in the recipe chains (before
  S1c, S1a, S1b and E9b): the 31 Pauper sideboard cards of the metagame snapshot. Overload
  (Vandalblast, Electrickery) is the Adventure form of the card that split cards
  use to represent the paid overload cost from hand, a normal graveyard permission, or
  Future Sight's paid library permission,
  with no targets and the printed mana value on the stack (118.9c). Circle of
  Protection: Blue offers a choice among the blue objects in the public zones and
  records a shield (source incarnation, player, turn) that the damage replacement
  pass uses up. Flagbearer (Standard Bearer, Coalition Honor Guard) constrains
  target selection when an opponent announces a spell or activated ability.
  Stonehorn Dignitary's skip is a pending flag the step machine reads as the
  combat phase would begin.
  Spreading Seas joins wave E8a's land-type setter (the enchanted land is an
  Island and loses its other land types and abilities); Navigator's Compass adds
  a land type and its mana ability until end of turn, the way Yavimaya does for
  Forests. Waterbend is an option list (tap an artifact or creature, or pay the
  rest with mana) that offers only completable branches. Martyr of Sands has one
  native activation with a private selection of any number of white cards from
  the payer's hand. Finishing reveals the selected cards, pays {1}, and
  sacrifices the source. The paid reveal count sets the life gain on resolution.
  Call Damage Control is one spell
  with up to two target cards of different types from the graveyard (artifact,
  creature, enchantment or land), each returned independently. New target shapes
  are `StandardTargetV1::S1dV1`. Rhino and Flagbearer use new stable ids 361 and 362;
  Nylea's Disciple uses Centaur's existing stable id 281 from P2c.
  Partial: Martyr of Sands passed its native paid reveal and restore fixture;
  event-history consumer qualification remains pending. Standard Bearer and Coalition Honor Guard now share bounded
  complete-assignment checks for optional target counts, including early Finish,
  restored selections and payable activations. All five optional-target native
  cases passed. Goblin Welder uses the same check
  for its two dependent slots. An artifact Flagbearer is required only if its
  controller's graveyard contains a legal second target. Shatterskull Smashing
  checks whether its announced allocation can include a Flagbearer at a payable
  X. An empty allocation can still finish, and each selected target requires
  at least one damage. Chandra, Flameshaper and the Flagbearers are in separate
  shipped registries. Native qualification and broader targeting coverage are
  pending. Vandalblast and Electrickery use the existing graveyard
  permission for a paid overload and retain Retrace's land discard. A free-cast
  permission offers the ordinary spell form with its original target requirement.
  Omniscience can still cast the ordinary form for free or let its controller pay
  for overload. Bolas's Citadel offers the ordinary form with its life payment.
  If both Future Sight and Bolas's Citadel grant the library cast, the ordinary
  form can still pay life and the overloaded form pays only its mana cost.
  The paid overload retains its exact Future Sight permission in the finalized
  cast publication. The current pending cast menu has no interleaved mana
  activation action and these overload payments use the ordinary mana plan.
  The affected overload fixtures and consumers still need qualification.
  Circle of Protection: Blue now rechecks blue at damage time and matches genuine
  historical damage to the chosen source incarnation. A native Stack-to-Battlefield
  entry records the exact permanent that a chosen physical cast becomes, bound
  to its published cast identity and first committed departure; later blinks do
  not retarget its shield. Virtual permanent spell copies still need this binding.
  The choice menu includes blue permanents, stack spells, face-up command
  objects and supported public references from stack items and waiting effects.
  Flickerwisp and Yorion's unchanged face-up exile links remain eligible
  when their waiting return moves onto the stack. Parallax Wave and Petradon
  contribute their exact live linked exiles only once the matching return
  trigger is on the stack.
  It uses the original incarnation and retained color for authenticated departed
  damage sources. Eternal records the physical Mogg Fanatic's source immediately
  before paying its sacrifice cost, even if Circle is absent. A later color or
  zone change cannot affect that damage. Legacy unmarked Fanatic activations retain
  the earlier execution path. Goblin Bombardment's native activation sacrificing
  another creature uses its live source while it remains on the battlefield.
  If it leaves with that activation waiting, the actual departure captures its
  last-known color and other damage characteristics, including when Circle enters
  later. Newly paid activations carry a marker and refuse restore after departure
  if the receipt is missing. Older unmarked rows retain their earlier behavior.
  Eternal excludes the Crystal and Twinflame damage owners by catalog. An
  animated Bombardment sacrificed as its own cost is outside this capture path.
  Unrelated graveyard and exile cards are excluded. Other
  embedded effect references need explicit bindings, departed references need
  last-known color and visibility evidence, and contextual hand and library
  sources remain unsupported. An in-flight saved Circle menu with the old
  broad roster refuses restore when it differs from the legal roster, preserving
  the meaning of its original option indices. Paid source-menu,
  Fanatic/Painter and blink/restore qualification is pending. Policy observation
  v5 omits the shields.
  [Comprehensive Rules, 609.7 and 702.96](https://media.wizards.com/2026/downloads/MagicCompRules%2020261106.txt)

The Extirpate, Martyr and Lavinia repair batch is composed at `e8d06349e2956e3a386d10a77295ebcc928a991d`.
The pinned Rust 1.94.1/MSVC standalone generator produced catalog hash
`e19d5ece9b17ec1e` from that source; the prior capability-only hash was `a07edccb09460703`.
Generator receipt SHA-256: `3fa633076153461982c151eaf6bc225d4dd73ef8feddd0b00a2784e3b7b97249`.
The guarded native batch at `90bda0c0f` completed 24 passes and four fixture
failures. Surgical, Extirpate, Martyr and the five optional Flagbearer cases
passed. The Lavinia Bestow fixture accidentally provided an extra mana creature;
the host is now summoning sick. Two Hullbreacher fixtures now use the available
Opt, and the Niv/Hull combination is documented as absent from shipped catalogs.
Those changed fixtures await rerun. The genuine `MartyrRevealPaidV1` sample is
appended with its passing producer receipt in `data/event_history_v1/README.md`;
the three event-history consumer checks remain pending.
No capability labels or playable-deck counts are promoted by this source batch.

The successor combines pre-entry Meddling Mage choices, the native Eternal
opening window, paid overload permissions and Circle/Flagbearer source repairs.
At generator source `2dfff92a7d76c620342bf7682e0e24d4a16d4217`, the pinned
Rust 1.94.1/MSVC generator produced `0255dfef9c57f234` for the revised overload
recipe. Receipt SHA-256:
`a8df5a2a37bbfa58becd863e8e6efc8d7b562293517e8c0789957fca994f3f9c`.
The following Circle menu change does not alter the registry or recipe inputs.
Native qualification and genuine Mage/opening event samples remain pending.

## Card contract and verification

Each card follows the existing contract: `engine_capability` Full only when every
reachable branch is implemented, a typed program (no name-only behavior), and a native
rules test for every new card (not only per family), modeled on `tests/standard/*`. Every
wave also adds:

- The newly playable decks to `PLAYABLE` in `tests/eternal/eternal_catalog_v1.rs`, which
  resolves them, preflights them and plays each against the next with a ranked action
  preference and with seeded random actions (so lands, mana rocks and activated abilities
  run), checking natural terminals and transcript replay. Decks with a companion play
  with it, and the games take the companion action.
- The census output, before and after, in the PR description.

Gameplay support is engineering, not playing-strength evidence. Policy encoders, training
and Spellbench entries for eternal formats are separate work; frozen Pauper encoders keep
refusing unsupported state.

## Sideboards

Once a deck's mainboard plays, its sideboard is next. The **full 75 playable share** is the
summed share of playable decks whose sideboard is Full too. The census reports it with the
missing sideboard cards of playable decks, ranked by the summed share of the decks that list
them (`sideboard_unsupported_by_share`, with each card's deck count).

The snapshot lists a companion among the sideboard's 15 cards, as tournament decklists do:
all twelve companion decks (two Legacy Yorion lists with 80-card mainboards, ten Vintage
Lurrus lists) have 15-card sideboards that include it. The census counts the companion once,
as part of the deck. Other snapshot facts a best-of-three builder has to accept: two
sideboards have 14 cards, seven mainboards have 61 or 62, and a card can take two rows of one
zone (the counts add).

Baseline (main after E9a and E9b; S0):

| Format | Playable | Full 75 | Missing sideboard cards |
| --- | ---: | ---: | ---: |
| Pauper | 95.2% | 45.5% | 32 |
| Legacy | 90.0% | 5.9% | 69 |
| Vintage | 98.1% | 0.7% | 36 |

That is 126 distinct cards. Completing decks greedily by share per new card, 45 of them reach
81% (Pauper), 53% (Legacy) and 87% (Vintage); all 126 reach the playable shares.

Stages, one PR each (S1 is one PR per wave):

- S0: the census measure above.
- S1: sideboard cards in waves, Legacy first, with the card contract of E1 to E9.
- S2: an eternal best-of-three harness under `eternal-fixtures`: format-aware deck size
  bounds (at least 60 mainboard cards, at most 15 sideboard cards with the companion
  among them), a companion-aware game builder and a catalog best-of-three test with a
  seeded random sideboarding policy. Pauper's exact 60/15 registration and its frozen
  sideboard policy are unchanged.
- S3: an owner-private outside-the-game zone for wish effects (Burning Wish, Karn, the
  Great Creator's -2, Learn, Extrapolate the Impossible), replacing the best-of-one stubs.

S2 is `src/eternal_bo3_v1.rs`. `EternalRegisteredDeckV1` admits a deck's 75 for its format
(`EternalFormatV1`: at least 60 mainboard cards, at most 15 sideboard cards, four copies of a
nonbasic card across both zones, every card Full, a named companion in the sideboard with its
condition met). The mainboard also follows the custom-deck session's 10,000-card input limit.
Each game's configuration must hold the same 75 within the same bounds, and
game one uses the registered deck. The game builder gives the session the mainboard and the
companion only while it is still in the sideboard and the mainboard meets its condition;
otherwise it is an ordinary sideboard card that game. The rest of the sideboard is the game's
outside-the-game cards (S3). Match state is `bo3_match`'s. The custom-deck session seats its
first deck as the starting player, so a prepared game lists the starting player's deck first.
`RandomSideboardPolicyV1` exchanges up to N random sideboard cards (never the companion) for
mainboard cards before games two and three, from the registered configuration each time.
`tests/eternal/eternal_bo3_v1.rs` tracks the decks whose full 75 is Full (`PLAYABLE_75`) and
plays each against the next deck of its format in a seeded match with random actions, checking
natural terminals, replay and that sided-in cards are cast.

For the opt-in native opening window, register both decks with
`EternalRegisteredDeckV1::new_with_native_opening_v1`, then call
`EternalBestOfThreeMatchV1::start_game_with_native_opening_v1` for each game.
Drive the returned `GameState` with `engine::advance_until_decision` and `engine::step`.
It uses the ordinary shuffle and alternating deal, London mulligans, then kept-hand
choices before untap. The prepared game's seat map still identifies the starting
match player as native seat P0. Companion and outside-game setup retain their
admitted configuration. This explicit registration admits the two executable
Partial Leylines alongside Chancellor of the Annex and otherwise requires Full.
The ordinary registration and catalog counts still require Full, and the legacy
start refuses these opt-in registrations. Native qualification is pending.

S3 is `src/eternal_outside_game_v1.rs`. Each player's cards outside the game (a best-of-three
game's sideboard without a companion in use) are a sorted list in the eternal module state,
set at reset from the custom-deck session's optional `outside_the_game` rows in the eternal
build with the London schema. The list is excluded from public deck identities and zone
projections. Burning Wish, Karn, the Great Creator's -2,
learn (First Day of Class) and Extrapolate the Impossible offer their eligible cards as
effect options in card id order, the decline first where the effect is optional (Karn's
exile choice is its first option and may choose nothing; learn adds "discard a card, then
draw a card" while its controller has a card in hand). Karn's exile card and learn's discard
are this module's typed resolution choices (`OutsideGameChoiceV1`), because the engine's
generic object selection and nonterminal discard must sit at a fixed position of the card's
own program, while these options are built as the effect resolves. Learn uses the normal
discard path for madness and runs the draw in the interpreter so dredge can replace it. The chosen card leaves the list and enters its
owner's hand as a new object that the opponent knows. Extrapolate's controller
picks two different names one at a time and the opponent picks between them by index. The
Limited reply includes `outside_game_options` for the chooser in effect-option order. The
opponent's Extrapolate menu lists the two revealed card definitions, and the other sideboard
cards remain private.

An explicit `outside_the_game` reset field enables the public
`outside_game_rules: "outside-the-game/v1"` capability, including when both
lists are empty. The capability persists after cards leave the lists and
through snapshot restore. Omitting the reset field preserves legacy
best-of-one snapshots and hashes. Identities and counts never appear in the
capability. Infoset exporters refuse this profile with one fixed R2 detail
before projection: their canonical allocator does not yet represent hidden
sideboards. This refusal depends on the public capability, not the remaining
private cards. An empty, absent module extension contributes no hash bytes.

Historical S1 census with the original capability labels. Each entry gives the
full 75 share and the number of missing sideboard cards. The S1b row counted 82
lists before the documented Partial cards were excluded.

| Wave | Cards | Pauper | Legacy | Vintage |
| --- | ---: | --- | --- | --- |
| Baseline | | 45.5%, 32 | 5.9%, 69 | 0.7%, 36 |
| S1d (Pauper) | 31 | 94.9%, 1 | 5.9%, 69 | 0.7%, 36 |
| S1c (Vintage) | 17 | 94.9%, 1 | 5.9%, 69 | 5.9%, 19 |
| S1a (Legacy) | 24 | 95.2%, 0 | 46.0%, 46 | 25.7%, 14 |
| S1b (Legacy) | 26 | 95.2%, 0 | 52.7%, 20 | 44.3%, 10 |

The corrected census classifies 13 S1 definitions as Partial and counts 49
full-75 lists across Pauper, Legacy, and Vintage. Their mainboards and sideboards
were measured against the original snapshots with only these capability labels
changed. This is registry coverage. The combined candidate still needs its affected
native checks before these match paths are qualified.

| Format | Playable mainboard share | Full 75 share | Non-Full sideboard cards in mainboard-playable decks |
| --- | ---: | ---: | ---: |
| Pauper | 95.2% | 64.8% | 6 |
| Legacy | 90.0% | 33.8% | 25 |
| Vintage | 98.1% | 41.9% | 12 |
| Premodern | 86.0% | 0.0% | 43 |

The Legacy Bant Control mainboard leaves the playable inventory because it
contains Lavinia. Across the three MTGGoldfish formats, 41 distinct
sideboard cards remain outside Full.

## Execution

Family workers implement bounded card groups in their own worktrees; one integrator owns
cross-family repairs, CI and the wave PR. No local build runs while `host_slots` shows the
desktop claimed. Cards absent from the local XMage data (59, mostly 2026 sets) take Oracle
text from Scryfall at implementation time.

Out of scope here: cards outside the snapshot (a later Oracle-template generator can cover
vanilla and templated cards), Commander-only mechanics, and model work.

## Premodern

Premodern (#384, tracked in #460) uses the cards printed from Fourth Edition through Scourge,
minus its ban list. It joins the same `eternal-fixtures` build and registry with its own
snapshot, `data/eternal_v1/meta_snapshot_mtgtop8_premodern_20261101.json`: 24 archetypes
with 90% of mtgtop8's last-two-months field (2,283 decks), the most recent list of each.
MTGGoldfish does not track Premodern. `python/tools/premodern_meta_snapshot_v1.py`
recaptures it; mtgtop8 rounds shares to whole percents and omits archetypes under 1%.

Baseline on main (2026-11-01): 76 of the 305 distinct cards are Full and no deck is
playable. 174 mainboard cards would cover all 24 lists. The first target is 70% of the
field from 88 cards in two waves, with the same contract, catalog games and census
reporting as E1 to E9:

| Wave | Content | New cards | Result |
| --- | --- | ---: | --- |
| P0 | Snapshot, capture tool, the catalog reads both snapshots | 0 | Premodern 0% |
| P1a | Stiflenought, Sligh, RG Aggro, Goblins, Terrageddon, Mono-Black Aggro, plus the 11 cards they share with P1b (Armageddon, Disenchant, Fire // Ice, Foil, Gemstone Mine, Naturalize, Portent, Rishadan Port, Smother, Sylvan Library, Terravore) | 53 | Premodern 0% to 40% |
| P1b | Enchantress, Replenish, Oath of Druids, Landstill, Psychatog, Grow | 35 | Premodern 40% to 70% by the census; Enchantress plays through sessions once schema 5 reports shroud |
| P2a | The Rock, Mono-Black Control, BW Control, Stasis, Other Control | 31 | Premodern 70% to 79% by the census |
| P2d | White Weenie, Mono-Red Control (Beloved Chaplain, Devoted Caretaker, Glowrider, Mother of Runes, Nova Cleric, Shared Triumph, Soltari Monk, Soltari Priest, Avalanche Riders, Earthquake, Firebolt, Ghitu Encampment, Pillage, Stone Rain, Veteran Brawlers) | 15 | two decks, 2% of the field |
| P2b | Survival, Madness Threshold, and the cards Elves shares with them (17 of 19; Birchlore Rangers and Thornscape Battlemage stay out) | 17 | Survival and Madness Threshold playable (4%); Elves needs Birchlore Rangers, Thornscape Battlemage and wave P2a's Genesis and Wall of Blossoms |
| P2c | Reanimator, Full English Breakfast (20 of 21 cards; Volrath's Shapeshifter stays out) | 20 | Reanimator playable; Full English Breakfast needs Volrath's Shapeshifter (see below) |

P1b is built in `src/eternal_premodern_p1b_v1.rs` (with `statics` and `targets` submodules)
and `build_eternal_p1b_v1.rs`. Opalescence makes each other non-Aura enchantment a creature
(a type override) with base power and toughness equal to its mana value, a layer 7b setting
with the timestamp recorded as it enters. Parallax Wave's fade counters, Legacy's Allure's
treasure counters and Decree of Silence's depletion counters live in module state; fading is
an upkeep trigger, Parallax Wave's exiles are recorded by its battlefield incarnation and its
leaves trigger returns the cards still in exile, so an exile that resolves after the trigger
is permanent. Legacy's Allure's target is bounded by its own counters: its live ones while it
is offered and targeted, then those recorded as it was sacrificed, kept while its ability is
on the stack. Costs paid as an ability is put on the stack (Wishclaw Talisman's hook) return
Attunement to its owner's hand and remove Parallax Wave's fade counter; the same hook queues
the Decrees' "When you cycle" triggers above the cycling ability. Decree of Justice's trigger
offers each X its controller can pay. "Whenever you cast an enchantment spell", Standstill's
"when a player casts a spell", Decree of Silence's counter and City of Brass's "becomes
tapped" are hook-queued triggers. Fact or Fiction's piles and Intuition's pick are
resolution choices made by the opponent (`E3ChoiceKindV1::P1bV1`); the caster takes a Fact or
Fiction pile by pointing at one of its cards, or at none to take an empty pile. Replenish
chooses each Aura's host among the permanents already on the battlefield, then returns every
card in one event and attaches the Auras; an Aura with no legal host stays. Sickening Dreams'
"discard X cards" announces X like replicate (at most the cards in hand), also when it is cast
without paying its mana cost, since X counts an additional cost. Frantic Search chooses its
lands before its discard; the two choices are independent, so the order is unobservable.
Solitary Confinement skips its controller's draw step and gives them shroud and damage
prevention; Sterling Grove grants shroud (the Groves are recorded as they enter, so games
without one skip the check); Tsabo's Web keeps lands with nonmana activated abilities tapped;
Aura of Silence taxes its opponents' artifact and enchantment spells. Mishra's Factory and
Treetop Village reuse the Standard manland animation, Serra's Sanctum Tolarian Academy's
yield, and Skycloud Expanse is a paid additional mana ability adding a fixed bundle.

The schema 4 public-session observation cannot represent shroud, so a session refused games
where Argothian Enchantress or Sterling Grove gave a permanent shroud. The catalog games now
run on Limited schema 5, the [public shroud contract](limited_public_shroud_v1.md), whose
policy observation reports shroud on permanents and players, and Enchantress is in `PLAYABLE`.
Schema 4 still refuses those games, and now also refuses Solitary Confinement's player shroud,
which it never represented.

Known gaps:
- Fade, treasure and depletion counters live in module state: effects that read or move
  counters generally (none in the catalog) and policy observations don't see them.
- Replenish leaves in the graveyard an Aura whose enchant rule neither `AttachmentDef` nor
  the Standard Aura entry models (Animate Dead's "enchant creature card in a graveyard").
- Casting Sickening Dreams needs another card in hand even for X = 0, since the cast checks
  accept a one-card discard as the cost's shape.
- Decree of Justice's cycling {X} is paid by the automatic mana planner (no choice of
  sources), and X is at most 20.
- Tsabo's Web reads printed abilities only, so a land that gains a nonmana activated ability
  still untaps.
- Dress Down reads printed creature type, so Opalescence's enchantment creatures keep their
  abilities under it.
- Parallax Wave's leaves trigger does not check whether its abilities were active as it left.
- An Opalescence or Sterling Grove entering by a path that skips the entry-counter hook is
  recorded at the next trigger collection instead.

P2c is built in `src/eternal_premodern_p2c_v1.rs` (with `statics` and `targets` submodules) and
`build_eternal_p2c_v1.rs`, which `build.rs` asks before `build_eternal_v1` and whose runtime
tables head the `eternal_cards_v1` chains (subtype ids from 280: Saproling, Centaur). Its state
(`P2cStateV1`) is created only when one of its cards needs it. New primitives:

- Undiscovered Paradise records each tap for mana and returns to hand as its controller's untap
  step begins (a hook in the untap loop, skipped while Stasis skips the whole step, so the land
  returns in the next untap step that happens); Reflecting Pool has one ability per mana type,
  each gated by `ManaAbilityConditionDef::ControllerOtherLandCouldProduceV1` (another land, not a
  Reflecting Pool, whose current mana abilities could add the type: printed colors, restricted
  colors and gated abilities with their conditions and costs ignored, plus abilities granted by
  land type changes, moons and the like, tapped or not), plus a planner hook like the Tainted
  lands'.
- Exhume is a Show-and-Tell shaped choice (active player first, a lone creature card is taken
  without a decision, then both enter at once); Death reanimates and loses life equal to the
  card's mana value; Life animates every land its controller controls through a type override
  and a 7b base of 1/1 until the turn ends (`split_card` covers Life // Death).
- Avatar of Woe: fear as a blocker restriction, `DynamicCountDef::TenCreatureCardsInAllGraveyardsV1`
  as the {6} cost reduction, destruction that cannot be regenerated (Pillage's mechanism).
- Petradon exiles two target lands (`StandardTargetV1::TwoLandsV1`) and remembers them by its
  battlefield incarnation; its leaves trigger returns those still exiled under their owners'
  control, so an exile that resolves after Petradon left is permanent.
- Phantom Nishoba: seven entry counters (none while its abilities are removed, 614.12), damage to
  it is prevented and removes a counter (a hook after the protection check in `event.rs`; one
  counter per batch of simultaneous damage, tracked by `begin_damage_batch`/`end_damage_batch`
  around the batch replacement, and damage that can't be prevented is dealt while the counter
  still comes off), and a queued life-gain trigger for the damage it dealt (wave P1a's queue),
  neither of which applies while its abilities are removed. Palinchron's bounce
  (`P2cOpV1::ReturnSourceV1`) acts only on the live incarnation that activated it.
- Putrid Imp: threshold boost and "can't block" hooks, flying until end of turn through the
  engine's keyword effect. Hermit Druid reveals to the first basic land (library knowledge is
  public), the land to hand, the rest to the graveyard.
- Phyrexian Devourer: `CostComponent::ExileTopCardOfLibraryV1` freezes the exiled card in the
  ability's paid-cost provenance and the counters read its mana value; "power 7 or greater" is
  a state trigger queued once until that instance leaves the stack (resolved or countered, 603.8)
  and queued again while the power is still 7 or more.
- Battlefield Scrounger: `CostComponent::PutOtherCardsFromOwnGraveyardOnLibraryBottomV1` (the
  exile-from-graveyard cost's choice machinery, moving to the library bottom), a threshold
  activation gate and the once-per-turn limit.
- Krosan Reclamation: `StandardTargetV1::PlayerThenUpToTwoOfTheirGraveyardCardsV1` over wave
  P2a's shuffle; flashback {1}{G}. Karmic Guide: echo generalizes wave P2d's (`echo_cost`),
  flying, protection from black and the reanimation trigger.

Known gaps (P2c): Volrath's Shapeshifter stays out. It has the full text of the top creature
card of its controller's graveyard, which needs a permanent whose definition, abilities, triggers
and activated abilities follow another card while the top card changes; the engine reads all of
these from the object's own card definition and has no copiable-values layer for permanents
(only token copies at creation), so Full English Breakfast is not playable. Reanimator is
playable. Phantom Nishoba's life gain reads the creature when its trigger is collected, so a
Nishoba that has lost its abilities (Dress Down) and dies from the same damage it deals still
gains that life.

Ruling notes: Premodern plays current Oracle text, so old cards follow today's wording
(mana burn and damage on the stack do not exist). The snapshot names only legal cards,
so the catalog needs no format legality check.

P2d is built in `src/eternal_premodern_p2d_v1.rs` (with `statics` and `targets` submodules)
and `build_eternal_p2d_v1.rs`, whose tables sit between P1b's and P1a's (subtype ids from 240;
P2a takes 209 upward). New primitives:

- Shadow (Soltari Monk and Priest) joins Dauthi Voidwalker's rule in `eternal_e9b_v1`: a
  shadow creature blocks and is blocked only by shadow creatures. Printed protection from a
  color joins P1a's mask (`eternal_legacy_e9a_v1::statics::protection_mask`), and Beloved
  Chaplain's protection from creatures joins Seasoned Dungeoneer's checks (targeting by a
  creature's ability, damage from a creature, blocking by a creature).
- Mother of Runes reuses wave E9a's protection record (Sejiri Steppe) with the color chosen on
  resolution. Devoted Caretaker records protection from instant and sorcery spells until end
  of turn per permanent (`P2dStateV1::spell_protection`); it stops targeting by such a spell
  (also while the card is being cast) and damage from it.
- Shared Triumph chooses its creature type as it enters (the as-enters chain of
  `eternal_cards_v1::e3`, one option per creature type among all objects in the game) and
  boosts creatures of that type through the controlled-boost hook. Glowrider adds a generic
  cost for every noncreature spell of either player through the spell cost modifiers.
- Echo (Avalanche Riders): entering records that the creature owes echo; the controller's next
  upkeep queues one trigger (a hook, so there is no trigger on later upkeeps) that offers
  paying {3}{R} (taken from the automatic mana planner, or the creature is sacrificed if it
  cannot be paid) or sacrificing.
- Veteran Brawlers' restrictions read untapped lands when attackers and blockers are declared.
- Pillage and Devoted Caretaker use new `StandardTargetV1` filters (an artifact or land; a
  permanent you control). Earthquake damages in one batch; Nova Cleric destroys in one batch.
  Stone Rain is the Standard destroy-land spell, Firebolt a 2-damage burn spell with flashback.
  Ghitu Encampment reuses the Standard manland animation.

Observation: shadow and printed protection are not keyword flags of the schema 4 policy
observation, but like Dauthi Voidwalker's shadow and Knight of Stromgald's protection they are
printed on the card the observation names, and every blocking and targeting choice offered is
already legal under them. Protection that Mother of Runes or Devoted Caretaker grants until end
of turn is not visible in the observation (as Sejiri Steppe's is not); no game step depends on
it, because only legal actions are offered. Representing granted protection and shadow as
public facts is a protocol change outside P2d.

Known gaps (P2d):
- Echo is recorded for a new controller when `standard_cards_v1::gain_control` moves the
  creature; other control changes (the temporary control effects) do not record it. Its cost is
  paid by the automatic mana planner without a choice of sources.
- Shared Triumph offers the creature types of the creature cards its controller owns, as Cavern
  of Souls does, so the option count reveals nothing hidden; a type no card of theirs has cannot
  be named.
- Protection from instant and sorcery spells counts a card in hand, graveyard, exile or
  library being cast as a spell, so an instant's activated ability from those zones (none in
  the catalog) would be treated as one.

P1a is built in `src/eternal_premodern_p1a_v1.rs` (with `statics` and `targets` submodules)
and `build_eternal_p1a_v1.rs`, whose tables wave P1b's fall through to. New primitives:

- Split cards (Fire // Ice): the second half is the card's Adventure form, cast from hand by
  choosing it as the spell's form, and it goes to the graveyard instead of exile. On the
  stack a half has only its own colors and mana value; elsewhere the card has both colors
  and mana value 4. Fire's divided damage is announced with its targets, two picks among
  creatures, planeswalkers and players, as Fury's is.
- Morph (Exalted Angel) reuses disguise's face-down cast for {3} without the ward; turning
  it face up for the morph cost is the special action manifests already use.
- "Exile X cards from your graveyard" (Skeletal Scrying's additional cost, Flash of
  Insight's flashback with blue cards only) reuses Nethergoyf's open-ended graveyard
  exile: the caster picks cards while one more keeps the cast payable, then X is the
  number picked. Foil's "discard an Island card and another card" is an alternative cost
  picked Island first. Skirk Prospector's mana ability sacrifices a Goblin, itself
  included, as its cost target.
- Printed protection from a color (Goblin Piledriver, Knight of Stromgald, Mystic Enforcer)
  joins Sejiri Steppe's protection mask. Funeral Charm's swampwalk, Goblin Goon's attack
  and block restrictions, Goblin Warchief's cost reduction and haste, threshold (Mystic
  Enforcer, Barbarian Ring), Terravore's characteristic-defining size, Sulfuric Vortex's
  "players gain no life" (lifelink included), Goblin Vandal's "assigns no combat damage",
  Goblin Sharpshooter's untap step and "can't be regenerated" (Incinerate for the turn,
  Smother for its own destruction) are hooks.
- Gemstone Mine's mining counters work like wave E8b's depletion counters; Powder Keg's
  fuse counters are read as its ability resolves, from the incarnation that was
  sacrificed. Rith's Grove's entry trigger returns a chosen non-Lair land or sacrifices it.
- Hook-queued triggers: Gempalm Incinerator's cycle trigger (from the cycling discard),
  damage dealt by Goblin Lackey and Hypnotic Specter and to Jackal Pup, "whenever a
  creature dies" (Goblin Sharpshooter), Ball Lightning's end step, Anurid Brushhopper's
  end-step return, Sulfuric Vortex's upkeeps, Goblin Vandal's "attacks and isn't blocked"
  (as blockers are declared) and Sylvan Library's draw step (put on the stack after the
  draw). Sylvan Library's "cards drawn this turn" are the turn's draws since its upkeep
  that are still in hand.
- Cursed Scroll names a card by first letter, then name, among every registry card name,
  and reveals a card at random from its controller's hand. Portent's controller orders
  the top three cards of the target player's library. Vision Charm's land type change
  chooses one of the 18 modeled land types, then a basic land type, and fixes the affected
  lands as it resolves (611.2c); wave E8a's land-type change applies it with Harbinger of
  the Seas and the moons, the latest timestamp winning. Its phasing is wave E5a's.

Known gaps: Ice can be cast only from hand, as an Adventure form can, so a split card cast
from another zone (a flashback grant, Yawgmoth's Will) is cast as Fire; naming effects name
the whole card ("Fire // Ice"), not one half. Morph is modeled for Exalted Angel only.
Wild Mongrel's chosen color is ordered by timestamp against Painter's Servant only; a
later color-setting effect from another wave (Oko's Elk) does not override it. Exalted
Angel's life gain trigger reads its printed ability with last-known information, so damage
an effect deals for it after it left the battlefield (Legolas's Quick Reflexes) still
triggers it. Neither interaction needs a Premodern card.

### P2a

P2a is built in `src/eternal_premodern_p2a_v1.rs` (with `statics` and `targets` submodules) and
`build_eternal_p2a_v1.rs`, which sit between E9a's tables and wave P1b's. No card needs a new
creature type. New primitives:

- Morph with an X cost (Bane of the Living) extends P1a's morph. Turning it face up pays {B}{B}
  as the special action, then its controller announces X, from 0 to the generic mana they can
  still pay, as an as-enters style choice (`entry_choice_decision`), pays it, and a
  hook-queued "turned face up" trigger gives every creature -X/-X until end of turn. With no mana
  to spare no choice is offered.
- Contamination replaces the mana a land adds: the colors a land offers, the activation and the
  payment planner all see a single {B}. The land's own ability still runs (City of Brass still
  deals damage, Gemstone Mine still loses a counter); mana already floating is unchanged.
- Stasis ("players skip their untap steps") and Forsaken City ("doesn't untap during your untap
  step") are untap-step restrictions beside Tsabo's Web; Root Maze joins Archon of Emeria's tapped
  entries. Forsaken City's upkeep trigger offers a card from hand to exile, and none declines.
- Future Sight extends Bolas's Citadel's library permissions: the top card is revealed to both
  players (each observer's knowledge is refreshed before a priority decision, as for the Citadel),
  and its controller may play a land from the top or cast any other card from the top for its mana
  cost. A controller who also has a Citadel casts through the Citadel while they can pay the life.
- Cards in a graveyard act through hook-queued triggers: Genesis at its owner's upkeep (target
  creature card announced as the trigger is put on the stack, {2}{G} paid on resolution), Nether
  Spirit (only while it is the only creature card there, checked again on resolution), Gaea's
  Blessing when it goes from a library to a graveyard, and Eternal Dragon's activated return, which
  is allowed only in its owner's upkeep. Academy Rector and Veteran Explorer use dies triggers
  that function from the graveyard and the engine's library searches to the battlefield.
- Pernicious Deed's {X} is one ability per X from 0 to 8, the largest mana value in the Premodern
  pool; the controller picks which to activate. The engine's announced X for activated abilities
  belongs to a Standard card.
- Black Vise records its chosen opponent as it enters (the one opponent of this kernel) and its
  trigger is queued as that player's upkeep begins. Gerrard's Verdict's discards are chosen by
  the target player (all of a hand of two or fewer), then the caster gains 3 life per land. Gaea's
  Blessing's targets are a player and up to three cards in that player's graveyard
  (`StandardTargetV1::PlayerThenUpToThreeOfTheirGraveyardCardsV1`). Stasis's and Contamination's
  upkeep costs are choices between the cost and a sacrifice. Krosan Tusker's "When you cycle"
  trigger is queued as the cycling cost is paid, like the Decrees'.
- Diabolic Tutor reuses Demonic Tutor's program; Wall of Blossoms, Ravenous Rats, Ravenous
  Baloth, Vindicate, Innocent Blood, Zombie Infestation, High Market, Phyrexian Tower and the Tainted
  lands use existing recipes (the Tainted lands are a Verge with "a Swamp" as both subtypes).

Known gaps:
- Pernicious Deed cannot pay an X above 8.
- Under Contamination a land's "{T}, Sacrifice a creature" ability (Phyrexian Tower) is not
  offered, since it adds the same single {B} as tapping the land.
- Stasis and Forsaken City stop permanents untapping; other things an untap step does (phasing in,
  day and night) still happen.
- Academy Rector's search may find an Aura, which the engine puts onto the battlefield with no
  host rather than leaving it in the library (no Aura is in Other Control's list).
- Veteran Explorer's search is not optional: each player searches, choosing no card is a search
  that finds nothing, and every player shuffles.
- The Tainted lands' second ability is an automatic payment source while a Swamp is controlled
  (the planner reads the gated colors from `statics::tainted_colors`); a Verge's is not.
- Black Vise's chosen opponent is not a choice, and its record does not follow a change of control.

P2b is built in `src/eternal_premodern_p2b_v1.rs` (with a `statics` submodule) and
`build_eternal_p2b_v1.rs`, the tail of the recipe chain that wave P1a's tables fall through to.
Its cards: Birds of Paradise, Caller of the Claw, Deranged Hermit, Gaea's Cradle, Multani's
Acolyte, Nantuko Vigilante, Survival of the Fittest, Wirewood Symbiote, Yavimaya Granger,
Masticore, Arrogant Wurm, Blastoderm, Chatter of the Squirrel, Glory, Tireless Tribe, Vintara
Snapper and Yavimaya Hollow, plus the Bear token. New primitives:

- Echo (Deranged Hermit, Multani's Acolyte, Yavimaya Granger): a permanent that enters is
  recorded, and at its controller's next upkeep the hook queues a trigger that pays the echo
  cost (the automatic mana planner pays) or sacrifices it, one option when the cost can be paid
  and a plain sacrifice when it cannot. Fading 3 (Blastoderm) keeps its own fade counters in
  module state, as P1b's Parallax Wave does.
- Gaea's Cradle is a printed mana ability whose amount is the creatures its controller
  controls (the amount Priest of Titania counts for Elves). Yavimaya Hollow regenerates its
  target with wave E7b's regeneration shield. Glory's ability activates from the graveyard and
  records protection from the chosen color for every creature its controller then controls
  (wave E9a's protection records). Masticore's upkeep is Solitary Confinement's choice with a
  sacrifice option; its regeneration is wave E7b's.
- Survival of the Fittest's cost is an ordinary one-card discard whose candidates are limited
  to creature cards where the discard is chosen, counted and checked
  (`statics::discard_cost_allows`). Wirewood Symbiote's "Return an Elf you control" is the
  existing return-a-permanent cost with a new subtype filter (`PermanentFilterDef::WithSubtypeV1`).
- Caller of the Claw counts the nontoken creatures put into each player's graveyard from the
  battlefield per turn; the hook reads each batch while the creature's last-known information
  is on record.
- Morph is now keyed by name in `statics::is_morph` and `statics::morph_cost`, so Nantuko
  Vigilante ({1}{G}) joins Exalted Angel; its turned-face-up trigger is queued from the
  `TurnedFaceUp` event. Deranged Hermit's Squirrel lord and Vintara Snapper's conditional
  shroud are P1a-chain hooks (`controlled_boost`, `granted_keywords`). Masticore is the first
  Masticore creature type (stable id 260).

Known gaps: Birchlore Rangers is left out: "Tap two untapped Elves you control" is a mana
ability with two object costs, but a mana-ability action carries one cost target
(`Action::ActivateManaAbilityWithCostTarget`, read by the flat policy encodings), so it cannot be
announced faithfully. Thornscape Battlemage is left out: it has two independent kickers ({R}
and/or {W}), and a card has one kicker cost and one kicked flag. Elves therefore stays out of
`PLAYABLE` (it also needs Genesis and Wall of Blossoms from wave P2a). Echo ignores a change of
control between entering and the upkeep. Glory's protection applies to the creatures its
controller controls as it resolves. Madness Threshold's Blastoderm and Vintara Snapper have
shroud, so its games need the public shroud representation of Limited schema 5.
