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
  "can't be activated" statics; Karn's wish finds nothing outside the game in
  best-of-one mainboard sessions and so chooses only from exile; Urza's Saga gains its
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
  from 0 to 7. Burning Wish finds nothing outside the game. Cabal Therapy names a card
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
  Extrapolate the Impossible, like Karn's wish, finds nothing outside the game in a
  best-of-one mainboard game. Kozilek's Command is an Eldrazi instant; the Kindred card
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
  finds no Lesson outside the game, so it is "you may discard a card to draw a card".
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
| P1b | Enchantress, Replenish, Oath of Druids, Landstill, Psychatog, Grow; playable after P1a merges | 35 | Premodern 40% to 70% |

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

Ruling notes: Premodern plays current Oracle text, so old cards follow today's wording
(mana burn and damage on the stack do not exist). The snapshot names only legal cards,
so the catalog needs no format legality check.
