# FDN remaining reference pool inventory v1

On October 9 Jack selected the full Foundations Play Booster pool as issue
#110's target. The frozen product manifest is
`data/limited/fdn_v1/booster_pool_v1.json`; see
`docs/design/fdn_booster_target_v1.md`. Its 276 main-set names and ten Special
Guests match all 286 historical reference names exactly. The nine names
outside XMage's main `Foundations.java` list are applicable Special Guests,
not missing target decisions. Goblin Bushwhacker is the tenth Special Guest
and was already supported. The older sizing snapshot below is preserved;
its request to freeze the manifest is now satisfied.

Snapshot of the 243 FDN reference names (242 missing plus the partial Ajani) that are not fully supported on `main` at `a4e1474b`, classified by the XMage implementation each card uses (`jackmaiorino/mage` master). The tier is a heuristic over the XMage ability/effect classes imported by each card file and whether the file defines its own effect classes; it estimates engineering size and is not a rules-parity verdict. Nine names (Akroma's Memorial, Bloom Tender, Condemn, Embercleave, Fiend Artisan, Grim Tutor, Paradise Druid, Sphinx's Tutelage, Temporal Manipulation) are absent from XMage's `Foundations.java` set list, so the observed 17lands pool includes cards printed outside the main set and a frozen target manifest is still required (milestone 5).

| Tier | Meaning | Names | Common | Uncommon | Rare | Mythic | Outside set |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | Keyword-only creatures (static keywords the engine already models) | 8 | 2 | 5 | 1 | 0 | 0 |
| 1 | One or two common primitives (ETB/attack/death trigger, single removal, pump, draw, life, simple token) | 90 | 37 | 31 | 12 | 6 | 4 |
| 2 | Several primitives or a new generic one (gainlands/fetch, equipment, auras, kicker, flashback, raid, landfall, surveil, library search, copies) | 78 | 27 | 31 | 16 | 3 | 1 |
| 3 | Custom rules code in XMage, planeswalkers or set-wide replacement/continuous effects | 67 | 5 | 23 | 24 | 11 | 4 |

Most frequent XMage building blocks across the remaining names: enters-the-battlefield triggers (44), static abilities (45), flying (34), token creation (29), activated abilities (26), card draw (21), life gain (17), +1/+1 counters (27 across source/target), attack triggers (13), end-step triggers (10), spell-cast triggers (10), library search (10), equipment (7), flashback (7), raid (9).

## Ownership

Issue 110's remaining work is split by mechanic family so parallel threads do not build the same engine primitive twice. The milestone owner keeps this inventory and the issue checklist current and builds the keyword, gainland and simple trigger cards. Separate threads own removal and pumps, tokens and auras, and equipment, kicker, flashback and library search. Custom rares and planeswalkers wait until those land. Each PR appends card ids and bumps the Limited catalog identity, so batches rebase and take the next version just before opening and merge one at a time.

Merged batches take one catalog version each: keyword creatures and gainlands (no version bump), triggers and tricks v54, equipment and search v55, tokens and Auras v56, removal and pumps v57, counterspells and simple threats v58, library search, kicker and flashback v59, simple triggers and graveyard spells v60.

The v61 activated-combat batch merged in [PR #195](https://github.com/jackmaiorino/mtg-kernel/pull/195).
Its default-branch inventory is 130 full, one partial and 155 missing names.

Current integration owner: Codex issue #110 goal. Catalog v62, Lightshell Duo
and Cephalid Inkmage, IDs 330-331, merged in
[PR #204](https://github.com/jackmaiorino/mtg-kernel/pull/204) at
8e65be54477cb658d0729d998bf7306146371115. All 23 reviewed-head checks passed;
the merge tree matched the reviewed composition and all 32 affected Python
tests passed on that actual default commit. Accepted coverage is 132 full,
one partial and 153 missing names. Original preparation branches remain retained.

Later prepared families remain retained in their original worktrees: v63
Vanguard Seraph/Cat Collector (IDs 332-333), v64 Elvish Regrower/Ambush Wolf
(334-335), v65 Anthem of Champions/Empyrean Eagle (336-337), and v66
Balmor/Firespitter Whelp (338-339). They require serial registration, generated
catalog identities, gameplay verification and integration. Source review
identified Vanguard Seraph's missing flying mapping and the graveyard batch's
pending-trigger hash compatibility requirement; these must be repaired at
integration. Existing Standard Wolf subtype support must be preserved when
integrating Ambush Wolf. Prepared source does not establish supported gameplay.

October 10 delivery plan: retain the explicitly recorded v63, v64 and v65
order. The unregistered source families tentatively labeled v66 through v81
can form one coherent later catalog batch, IDs 338-360, after those prerequisites
integrate. Their tentative version labels are preparation labels, not frozen
catalog identities. This avoids a separate full CI cycle for each one-card
family while preserving focused tests for every card, stable card IDs and one
serial catalog/profile publication. The consolidated batch has no accepted
coverage, registry, fixture or live-profile change yet. Its exact source still
requires native primitive execution, card registration, focused gameplay/restore
qualification, generated identities, current-head review and CI before merging.

Leyline Axe (game-start leyline step) and Electroduplicate (token copies)
remain unclaimed engine work. Full coverage, fair Limited search and DraftZero
acceptance remain issue #110 requirements. Research and experiments remain
owned by Claude.

## Tier 0

All but Elementalist Adept are implemented by `docs/design/fdn_keyword_creatures_v1.md`; Elementalist Adept (flash and prowess) is implemented by `docs/design/fdn_counters_threats_v1.md`.

Aegis Turtle, Brazen Scourge, Elementalist Adept, Quakestrider Ceratops, Savannah Lions, Serra Angel, Swiftblade Vindicator, Vampire Nighthawk

## Tier 1

Ajani's Pridemate, Marauding Blight-Priest and Sanguine Syphoner are implemented by `docs/design/fdn_gainlands_lifegain_v1.md`. Burglar Rat, Diregraf Ghoul, Firebrand Archer, Giant Growth, Helpful Hunter, Icewind Elemental, Infestation Sage, Prideful Parent, Spitfire Lagac, Stab, Think Twice and Wary Thespian are implemented by `docs/design/fdn_triggers_tricks_v1.md`. Dragon Trainer, Eager Trufflesnout, Elfsworn Giant, Goblin Surprise, Resolute Reinforcements and Rite of the Dragoncaller are implemented by `docs/design/fdn_tokens_auras_v1.md`. Boltwave, Broken Wings, Crackling Cyclops, Dauntless Veteran, Day of Judgment, Hero's Downfall, Make Your Move, Meteor Golem, Preposterous Proportions, Reclamation Sage, Seismic Rupture, Slagstorm, Snakeskin Veil and Sure Strike are implemented by `docs/design/fdn_removal_tricks_v1.md`. Angel of Finality, Bigfin Bouncer, Crypt Feaster, Erudite Wizard, Essence Scatter, Gleaming Barrier, Macabre Waltz, Phyrexian Arena and Refute are implemented by `docs/design/fdn_counters_threats_v1.md`. Adventuring Gear, Grim Tutor and Swiftfoot Boots are implemented by `docs/design/fdn_equipment_search_v1.md`. Campus Guide is implemented by `docs/design/fdn_library_search_v1.md`. Rune-Scarred Demon, Rune-Sealed Wall, Tatyova, Benthic Druid, Thrill of Possibility and Zombify are implemented by `docs/design/fdn_simple_triggers_v1.md`.

Adventuring Gear, Aetherize, Affectionate Indrik, Ajani's Pridemate, Akroma's Memorial, Ambush Wolf, An Offer You Can't Refuse, Angel of Finality, Anthem of Champions, Armasaur Guide, Balmor, Battlemage Captain, Banishing Light, Bigfin Bouncer, Bloodthirsty Conqueror, Bloom Tender, Boltwave, Brass's Bounty, Broken Wings, Burglar Rat, Campus Guide, Crackling Cyclops, Crypt Feaster, Crystal Barricade, Dauntless Veteran, Day of Judgment, Diregraf Ghoul, Dragon Trainer, Eager Trufflesnout, Elfsworn Giant, Elvish Regrower, Erudite Wizard, Essence Scatter, Exsanguinate, Felidar Savior, Firebrand Archer, Firespitter Whelp, Giant Growth, Gleaming Barrier, Goblin Surprise, Grim Tutor, Helpful Hunter, Herald of Eternal Dawn, Hero's Downfall, High Fae Trickster, Hungry Ghoul, Icewind Elemental, Infestation Sage, Inspiring Call, Lightshell Duo, Lunar Insight, Macabre Waltz, Make Your Move, Marauding Blight-Priest, Meteor Golem, Mocking Sprite, Omniscience, Phyrexian Arena, Pilfer, Preposterous Proportions, Prideful Parent, Progenitus, Raise the Past, Reassembling Skeleton, Reclamation Sage, Refute, Resolute Reinforcements, Rite of the Dragoncaller, Rune-Scarred Demon, Rune-Sealed Wall, Sanguine Syphoner, Seeker's Folly, Seismic Rupture, Self-Reflection, Shivan Dragon, Sire of Seven Deaths, Slagstorm, Snakeskin Veil, Sower of Chaos, Spitfire Lagac, Stab, Sure Strike, Swiftfoot Boots, Tatyova, Benthic Druid, Temporal Manipulation, Think Twice, Thrill of Possibility, Time Stop, Vanguard Seraph, Wary Thespian, Zombify

## Tier 2

The eight gainlands are implemented by `docs/design/fdn_gainlands_lifegain_v1.md`. Heroic Reinforcements and Twinblade Blessing are implemented by `docs/design/fdn_tokens_auras_v1.md`. Abrade, Bake into a Pie, Fanatical Firebrand and Incinerating Blast are implemented by `docs/design/fdn_removal_tricks_v1.md`. Burst Lightning, Evolving Wilds, Goldvein Pick, Quick-Draw Katana and Solemn Simulacrum are implemented by `docs/design/fdn_equipment_search_v1.md`. Burnished Hart, Grow from the Ashes and Revenge of the Rats are implemented by `docs/design/fdn_library_search_v1.md`.

Abrade, Ajani, Caller of the Pride, Apothecary Stomper, Arbiter of Woe, Archmage of Runes, Ashroot Animist, Authority of the Consuls, Axgard Cavalry, Bake into a Pie, Battlesong Berserker, Billowing Shriekmass, Blasphemous Edict, Bloodfell Caves, Brineborn Cutthroat, Burnished Hart, Burst Lightning, Bushwhack, Cephalid Inkmage, Claws Out, Courageous Goblin, Dismal Backwater, Divine Resilience, Eaten Alive, Electroduplicate, Elenda, Saint of Dusk, Empyrean Eagle, Evolving Wilds, Extravagant Replication, Fanatical Firebrand, Flamewake Phoenix, Frenzied Goblin, Goblin Boarders, Goldvein Pick, Gorehorn Raider, Grappling Kraken, Grow from the Ashes, Gutless Plunderer, Heartfire Immolator, Heroic Reinforcements, Incinerating Blast, Involuntary Employment, Juggernaut, Jungle Hollow, Kykar, Zephyr Awakener, Leyline Axe, Micromancer, Mild-Mannered Librarian, Mischievous Pup, Needletooth Pack, Niv-Mizzet, Visionary, Paradise Druid, Quick-Draw Katana, Revenge of the Rats, Rogue's Passage, Ruby, Daring Tracker, Rugged Highlands, Scoured Barrens, Scrawling Crawler, Searslicer Goblin, Skyship Buccaneer, Slumbering Cerberus, Solemn Simulacrum, Soulstone Sanctuary, Spinner of Souls, Squad Rallier, Stroke of Midnight, Stromkirk Bloodthief, Strongbox Raider, Swiftwater Cliffs, Tranquil Cove, Twinblade Blessing, Vampire Gourmand, Vampire Soulcaller, Vengeful Bloodwitch, Vivien Reid, Wardens of the Cycle, Wind-Scarred Crag, Zul Ashur, Lich Lord

## Tier 3

Blanchwood Armor is implemented by `docs/design/fdn_tokens_auras_v1.md`.

Abyssal Harvester, Alesha, Who Laughs at Fate, Arahbo, the First Fang, Arcane Epiphany, Banner of Kinship, Blanchwood Armor, Bulk Up, Cat Collector, Chandra, Flameshaper, Condemn, Consuming Aberration, Curator of Destinies, Doubling Season, Drake Hatcher, Drakuseth, Maw of Flames, Dreadwing Scavenger, Elvish Archdruid, Embercleave, Etali, Primal Storm, Faebloom Trick, Fake Your Own Death, Fiend Artisan, Fiendish Panda, Fiery Annihilation, Fishing Pole, Garruk's Uprising, Genesis Wave, Ghalta, Primal Hunger, Giada, Font of Hope, Goblin Negotiation, Hare Apparent, Heraldic Banner, Hidetsugu's Second Rite, High-Society Hunter, Imprisoned in the Moon, Infernal Vessel, Inspiration from Beyond, Inspiring Paladin, Kaito, Cunning Infiltrator, Kellan, Planar Trailblazer, Krenko, Mob Boss, Lathril, Blade of the Elves, Liliana, Dreadhorde General, Loot, Exuberant Explorer, Midnight Snack, Muldrotha, the Gravetide, Nessian Hornbeetle, Nine-Lives Familiar, Painful Quandary, Perforating Artist, Quilled Greatwurm, Ravenous Amulet, Rise of the Dark Realms, Run Away Together, Scavenging Ooze, Secluded Courtyard, Skyknight Squire, Soul-Shackled Zombie, Sphinx of Forgotten Lore, Sphinx's Tutelage, Thousand-Year Storm, Tinybones, Bauble Burglar, Tragic Banshee, Twinflame Tyrant, Valkyrie's Call, Wildwood Scourge, Zimone, Paradox Sculptor
