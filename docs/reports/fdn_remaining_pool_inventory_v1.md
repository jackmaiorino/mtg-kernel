# FDN remaining reference pool inventory v1

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

Claimed but not yet merged:

- Equipment/kicker thread, first batch: Evolving Wilds, Burst Lightning, Swiftfoot Boots, Solemn Simulacrum (adds the Golem subtype), Grim Tutor and Quick-Draw Katana; takes the catalog version and ids after `fdn_triggers_tricks_v1`.

## Tier 0

All but Elementalist Adept (prowess) are implemented by `docs/design/fdn_keyword_creatures_v1.md`.

Aegis Turtle, Brazen Scourge, Elementalist Adept, Quakestrider Ceratops, Savannah Lions, Serra Angel, Swiftblade Vindicator, Vampire Nighthawk

## Tier 1

Ajani's Pridemate, Marauding Blight-Priest and Sanguine Syphoner are implemented by `docs/design/fdn_gainlands_lifegain_v1.md`. Burglar Rat, Diregraf Ghoul, Firebrand Archer, Giant Growth, Helpful Hunter, Icewind Elemental, Infestation Sage, Prideful Parent, Spitfire Lagac, Stab, Think Twice and Wary Thespian are implemented by `docs/design/fdn_triggers_tricks_v1.md`.

Adventuring Gear, Aetherize, Affectionate Indrik, Ajani's Pridemate, Akroma's Memorial, Ambush Wolf, An Offer You Can't Refuse, Angel of Finality, Anthem of Champions, Armasaur Guide, Balmor, Battlemage Captain, Banishing Light, Bigfin Bouncer, Bloodthirsty Conqueror, Bloom Tender, Boltwave, Brass's Bounty, Broken Wings, Burglar Rat, Campus Guide, Crackling Cyclops, Crypt Feaster, Crystal Barricade, Dauntless Veteran, Day of Judgment, Diregraf Ghoul, Dragon Trainer, Eager Trufflesnout, Elfsworn Giant, Elvish Regrower, Erudite Wizard, Essence Scatter, Exsanguinate, Felidar Savior, Firebrand Archer, Firespitter Whelp, Giant Growth, Gleaming Barrier, Goblin Surprise, Grim Tutor, Helpful Hunter, Herald of Eternal Dawn, Hero's Downfall, High Fae Trickster, Hungry Ghoul, Icewind Elemental, Infestation Sage, Inspiring Call, Lightshell Duo, Lunar Insight, Macabre Waltz, Make Your Move, Marauding Blight-Priest, Meteor Golem, Mocking Sprite, Omniscience, Phyrexian Arena, Pilfer, Preposterous Proportions, Prideful Parent, Progenitus, Raise the Past, Reassembling Skeleton, Reclamation Sage, Refute, Resolute Reinforcements, Rite of the Dragoncaller, Rune-Scarred Demon, Rune-Sealed Wall, Sanguine Syphoner, Seeker's Folly, Seismic Rupture, Self-Reflection, Shivan Dragon, Sire of Seven Deaths, Slagstorm, Snakeskin Veil, Sower of Chaos, Spitfire Lagac, Stab, Sure Strike, Swiftfoot Boots, Tatyova, Benthic Druid, Temporal Manipulation, Think Twice, Thrill of Possibility, Time Stop, Vanguard Seraph, Wary Thespian, Zombify

## Tier 2

The eight gainlands are implemented by `docs/design/fdn_gainlands_lifegain_v1.md`.

Abrade, Ajani, Caller of the Pride, Apothecary Stomper, Arbiter of Woe, Archmage of Runes, Ashroot Animist, Authority of the Consuls, Axgard Cavalry, Bake into a Pie, Battlesong Berserker, Billowing Shriekmass, Blasphemous Edict, Bloodfell Caves, Brineborn Cutthroat, Burnished Hart, Burst Lightning, Bushwhack, Cephalid Inkmage, Claws Out, Courageous Goblin, Dismal Backwater, Divine Resilience, Eaten Alive, Electroduplicate, Elenda, Saint of Dusk, Empyrean Eagle, Evolving Wilds, Extravagant Replication, Fanatical Firebrand, Flamewake Phoenix, Frenzied Goblin, Goblin Boarders, Goldvein Pick, Gorehorn Raider, Grappling Kraken, Grow from the Ashes, Gutless Plunderer, Heartfire Immolator, Heroic Reinforcements, Incinerating Blast, Involuntary Employment, Juggernaut, Jungle Hollow, Kykar, Zephyr Awakener, Leyline Axe, Micromancer, Mild-Mannered Librarian, Mischievous Pup, Needletooth Pack, Niv-Mizzet, Visionary, Paradise Druid, Quick-Draw Katana, Revenge of the Rats, Rogue's Passage, Ruby, Daring Tracker, Rugged Highlands, Scoured Barrens, Scrawling Crawler, Searslicer Goblin, Skyship Buccaneer, Slumbering Cerberus, Solemn Simulacrum, Soulstone Sanctuary, Spinner of Souls, Squad Rallier, Stroke of Midnight, Stromkirk Bloodthief, Strongbox Raider, Swiftwater Cliffs, Tranquil Cove, Twinblade Blessing, Vampire Gourmand, Vampire Soulcaller, Vengeful Bloodwitch, Vivien Reid, Wardens of the Cycle, Wind-Scarred Crag, Zul Ashur, Lich Lord

## Tier 3

Abyssal Harvester, Alesha, Who Laughs at Fate, Arahbo, the First Fang, Arcane Epiphany, Banner of Kinship, Blanchwood Armor, Bulk Up, Cat Collector, Chandra, Flameshaper, Condemn, Consuming Aberration, Curator of Destinies, Doubling Season, Drake Hatcher, Drakuseth, Maw of Flames, Dreadwing Scavenger, Elvish Archdruid, Embercleave, Etali, Primal Storm, Faebloom Trick, Fake Your Own Death, Fiend Artisan, Fiendish Panda, Fiery Annihilation, Fishing Pole, Garruk's Uprising, Genesis Wave, Ghalta, Primal Hunger, Giada, Font of Hope, Goblin Negotiation, Hare Apparent, Heraldic Banner, Hidetsugu's Second Rite, High-Society Hunter, Imprisoned in the Moon, Infernal Vessel, Inspiration from Beyond, Inspiring Paladin, Kaito, Cunning Infiltrator, Kellan, Planar Trailblazer, Krenko, Mob Boss, Lathril, Blade of the Elves, Liliana, Dreadhorde General, Loot, Exuberant Explorer, Midnight Snack, Muldrotha, the Gravetide, Nessian Hornbeetle, Nine-Lives Familiar, Painful Quandary, Perforating Artist, Quilled Greatwurm, Ravenous Amulet, Rise of the Dark Realms, Run Away Together, Scavenging Ooze, Secluded Courtyard, Skyknight Squire, Soul-Shackled Zombie, Sphinx of Forgotten Lore, Sphinx's Tutelage, Thousand-Year Storm, Tinybones, Bauble Burglar, Tragic Banshee, Twinflame Tyrant, Valkyrie's Call, Wildwood Scourge, Zimone, Paradox Sculptor
