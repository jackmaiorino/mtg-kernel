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

Current integration owner: Codex issue #110 goal. The v62 surveil batch
merged in [PR #204](https://github.com/jackmaiorino/mtg-kernel/pull/204), and
v63 Vanguard Seraph/Cat Collector merged in
[PR #208](https://github.com/jackmaiorino/mtg-kernel/pull/208) at `bc23a8da`.
V64 Elvish Regrower/Ambush Wolf, IDs334-335, is now accepted in
[PR #210](https://github.com/jackmaiorino/mtg-kernel/pull/210) at53fab604.
All23 reviewed-head checks passed, its actual merge tree matches the reviewed
composition, and34 affected Python cases pass on that default commit.
V65 Anthem/Eagle, IDs336-337, is accepted in
[PR #212](https://github.com/jackmaiorino/mtg-kernel/pull/212) at32080f84878da6224ab5a04522a3086a5963a115.
All23 final reviewed-head checks and35 affected Python cases on the actual
default merge pass. Accepted coverage is138 full, one partial and147 missing
names of286; live v65 identity is42bf6f9ca62d6615.

Current accepted v69 integration: PR222 merged at actual default
`a957c358db9326cb3eacff819d6bcea38678aeb3`. All23 final checks pass;
actual tree `f57692fdaad90102c81f57cd5ae14df7fb4cc4b2` equals the reviewed
composition and preserves launcher PR223. All40 affected Python cases pass
on actual default. The actual frozen286-name inventory reports185 Full,
1 Partial and100 Missing; accepted identity is `b0338cda1845f330`. Frozen
membership/printing identities and24 token dependencies remain fixed.
See [default acceptance](https://github.com/jackmaiorino/mtg-kernel/pull/222#issuecomment-6107089141).
Both repaired Windows publisher cases and73 Limited production-profile cases
pass in hosted CI. Failed Native1/2 and the additional suspended Native3
remain retained; the suspended job is not a pass and remains guarded.
The following v68 and preparation statuses are retained history.

Current accepted v70 integration: PR224 merged at actual default
825a24b919ed586db7e5c144f87e3194513abd26. All23 final9c43 reviewed-head
checks pass; actual default tree9b2b8756eb38cdd8177ffe95b967716330b1844f
equals the reviewed source. All60 affected Python cases pass on that actual
default (35 decks,18 V8,7 V9). Frozen286-name inventory reports188 Full,
1 Partial,97 Missing; membership and24 token dependencies remain fixed.
The v73 candidate's five further names are pending delivery, not accepted
default coverage. Historical failed attempts and partial receipts remain
retained. Full gameplay, consumers and broader Limited milestones remain.

Prior accepted v68 integration: [PR216](https://github.com/jackmaiorino/mtg-kernel/pull/216)
merged atf7d115fa66f2f1efaa49a0d3b52912021d3a2fa1. All23 final checks in
CI38105979002 pass. Actual default treef037d21772cb8e944efacc92ef4254da0ba325d6
matches reviewed headbbe4968146a03f206d9dec85e5d6c22bc1680493 exactly;
all38 affected Python cases pass there. Accepted coverage is177 full, one
partial and108 missing of286 names, plus24 dependency records. Live v68
identity is645f2da1b223a18f. See [default receipt6105370378](https://github.com/jackmaiorino/mtg-kernel/pull/216#issuecomment-6105370378).

Original v68 Native1 passed17 of22 new card groups and failed5 missing
attack-declaration cases; its101 command/guard and logs remain retained.
Repair5f0bdae2 passed all22 groups in actual Native2, plus48 definitions,
23 rules cases(one existing ignore),151 native records(three existing
ignores),69 production-profile cases, Limited strict lint and combined
all-test typecheck/strict lint. Supported guardff99f2e7f20b4de4a50f66a41c9d1ed3
ended0 and released its cores. The composed default preserves this code and
concurrent PR206/219 changes. Historical v67 reads and stale writes/resumes
remain qualified. Issue110 stays open for full-pool and Limited/DraftZero work.

Next eight-card source2b624162 contains Nessian Hornbeetle, Micromancer,
Paradise Druid, Bulk Up, Inspiring Call, Needletooth Pack, Stroke of Midnight
and Hare Apparent. All eight are unregistered;41 actual gameplay groups
remain unrun. Source Native1 failed two registered Hare setup cases and
retains its101 receipt; fixture repair2b uses live printed names and actual
token creation. Source Native2 is queued under actual supervisor37988 and
supported host_slots, with no successful qualification claimed. The owned
admission branch composes source2b with accepted defaultf7, preserving all
19 default paths and27 source paths byte-for-byte before this receipt update.
Registration and all admission/profile checks wait for successful source
qualification. Prepared v69 would append IDs377-384, token IDs385-386,
387 definitions and prospective185 full/1 partial/100 missing coverage.
The prepared scripts are unexecuted. See the eight-card admission design.

The following preparation order and receipts are retained history.

Later prepared families remain retained in their original worktrees: v65
Anthem of Champions/Empyrean Eagle (336-337), then one coherent admission
for prepared IDs338-368, beginning with Balmor/Firespitter Whelp. The later
preparations include Wardens of the Cycle, Arcane Epiphany, Claws Out,
Dreadwing Scavenger, Mischievous Pup, Felidar Savior, Armasaur Guide and
Arbiter of Woe.
They require serial registration, generated
catalog identities, gameplay verification and integration. Source review
identified Vanguard Seraph's missing flying mapping and the graveyard batch's
pending-trigger hash compatibility requirement; both were repaired before
their accepted integration. Standard Wolf subtype support is preserved.
Prepared source does not establish supported gameplay.

The31 prepared names share one later catalog admission after v65 integrates.
Their original tentative version labels describe preparation order, not frozen
catalog identities. Every card still needs focused gameplay/restore execution,
with one generated catalog/profile publication for the coherent batch.
The source port incorporates accepted v65 static-team extraction, live profile
and repaired fixtures. Candidate662b8343 registers all31 new names and passes36
affected Python cases. Its first generator build failed and is retained;
reviewed repaira0032541 generates observed v66 identitye8e2bba9d9071e80.
Reviewed migration97bed882 binds that identity and preserves historical v65
readability and stale-write refusals. Card games, live profile execution, CI
and actual default integration remain pending. Candidate169 is not accepted
coverage. See `docs/design/fdn_prepared_admission_order_v1.md`.

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
