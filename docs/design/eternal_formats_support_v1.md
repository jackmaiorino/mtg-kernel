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
| E5a | Vintage Raker Shops, Oath of Druids, Paradoxical Outcome, Blue Tinker and Beseech Storm, Legacy Mystic Forge, and with E5b's cards Legacy Eldrazi and Tron: Mystic Forge, Bolas's Citadel, Yawgmoth's Will, Chalice of the Void, Sensei's Divining Top, Oath of Druids, Oko, Ugin, Kozilek's Command, Talon Gates of Madara and the rest of the lists | 28 and a token | Vintage 62.3% to 80.0%, Legacy 49.4% to 59.6% (after E5b) |
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
