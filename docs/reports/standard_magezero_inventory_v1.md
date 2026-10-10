# MageZero Standard pool inventory

The `standard-magezero-fixtures` catalog targets the 16-deck Standard opponent pool MageZero trains against (`data/standard/magezero_v1/`). As of `kernel_carddb_standard/v4`, the decks use 225 distinct nonbasic cards: 52 are Full, 19 are Partial and 154 are missing. No deck resolves yet. `mtg-kernel/tests/standard_magezero_catalog_v1.rs` and `python/tests/test_standard_decks_v1.py` track Full admission.

## Order of work

The five mono-color decks come first: they are MageZero's original opponents and together need 61 cards, marked "(mono)" below. Each two-color deck then needs 12 to 16 more cards and the 5-color deck 18.

## Ownership

Work is split by mechanic family so threads do not build the same engine primitive twice. Every batch appends to `data/standard/magezero_v1/cards_v1.json` and bumps the Standard identity (`kernel_carddb_standard/vN` in `build.rs`, the frozen hash and `STANDARD_APPENDED`/`SUPPORTED_NONBASIC` in the catalog test, and the Python test's list), so batches rebase and take the next version just before opening and merge one at a time. These bumps never touch the FDN Limited identity.

Initial threads and merge order (each tells the next when it merges; later batches take the next free version):

1. Removal, counters and card selection (family C), v2.
2. Creatures with triggered and static abilities (family G), v3.
3. New set keywords (family D), v4.
4. Lands (families A and B), v5. Mono-deck lands first; dual lands are needed by every two-color deck.
5. Non-creature permanents, planeswalkers and transforming legends (families E and F), v6.

The 5-color deck's legends (family H) wait until those land.

**Cards shared with FDN.** Abrade, Boltwave, Essence Scatter, Kellan, Planar Trailblazer, Resolute Reinforcements, Snakeskin Veil and Soulstone Sanctuary are also in the FDN reference pool. Their behavior is owned by the FDN threads (`docs/reports/fdn_remaining_pool_inventory_v1.md`). Once one merges there, a Standard batch only copies its registry entry into the Standard file, since `build.rs` keys card behavior by name.

## Families
### A. Dual lands (37 cards; 0 needed by a mono deck)

- **Painlands**: Adarkar Wastes, Caves of Koilos, Karplusan Forest, Llanowar Wastes, Yavimaya Coast, Battlefield Forge, Brushland, Shivan Reef, Sulfurous Springs, Underground River
- **Fastlands**: Copperline Gorge, Razorverge Thicket, Blackcleave Cliffs, Blooming Marsh, Concealed Courtyard, Darkslick Shores, Inspiring Vantage, Seachrome Coast, Spirebluff Canal
- **Slowlands**: Dreamroot Cascade, Deserted Beach, Haunted Ridge, Overgrown Farmland, Rockfall Vale
- **Verges**: Floodfarm Verge, Gloomlake Verge, Hushwood Verge, Riverpyre Verge, Thornspire Verge, Wastewood Verge
- **Surveil lands**: Elegant Parlor, Lush Portico, Underground Mortuary
- **Triomes (cycling)**: Jetmir's Garden, Spara's Headquarters, Ziatora's Proving Ground
- **Other**: Starting Town

### B. Creature lands and utility lands (21 cards; 5 needed by a mono deck)

- **Restless creature lands**: Restless Bivouac, Restless Cottage, Restless Fortress, Restless Prairie, Restless Reef, Restless Ridgeline, Restless Vinestalk
- **Other creature lands**: Mishra's Foundry (mono), Soulstone Sanctuary (mono)
- **Channel lands (Kamigawa)**: Eiganjo, Seat of the Empire (mono), Sokenzan, Crucible of Defiance, Boseiju, Who Endures, Otawara, Soaring City, Takenuma, Abandoned Mire
- **Utility lands**: Fountainport, Mirrex (mono), Fomori Vault, Rockface Village (mono), Lupinflower Village, Mudflat Village, Plaza of Heroes

### C. Removal, counters and card selection (35 cards; 15 needed by a mono deck)

`kernel_carddb_standard/v2` fully supports the mono cards here except the FDN-owned Essence Scatter and Partial Memory Deluge, plus Opt. Shoot the Sheriff counts every outlaw subtype: Pirate, Rogue and Warlock, plus Assassin and Mercenary since family G appended them in v3. Memory Deluge's bottom-order approximation is excluded from full deck admission.

- **Removal and burn**: Cut Down, Go for the Throat, Lightning Strike (mono), Shock (mono), Destroy Evil (mono), Sheoldred's Edict, Shoot the Sheriff (mono), Hard-Hitting Question (mono), Maelstrom Pulse, Tear Asunder, Witchstalker Frenzy, Fading Hope (mono), Get Lost (mono), Anoint with Affliction, Gleeful Demolition, Invoke Despair, Gix's Command, Abrade, Boltwave
- **Counterspells**: Dissipate (mono), Essence Scatter (mono), Negate (mono)
- **Card selection and draw**: Consider (mono), Opt, Impulse (mono), Sleight of Hand, Stock Up, Thirst for Discovery (mono), Flow of Knowledge (mono), Memory Deluge (mono), Big Score, United Battlefront, Kayla's Reconstruction
- **Protection tricks**: Shore Up, Snakeskin Veil

### D. New set keywords (2022-2025) (32 cards; 13 needed by a mono deck)

Family D adds 18 Full cards and retains 11 Partial definitions. Full deck admission refuses Axebane Ferox, Brutal Cathar, Burnout Bashtronaut, Enduring Curiosity, Enduring Innocence, Flourishing Bloom-Kin, Graveyard Trespasser, Hopeful Initiate, Knight-Errant of Eos, Make Disappear and Overlord of the Mistmoors. Their missing choices, timing and effective-type handling are recorded in `docs/design/standard_family_d_keywords_v1.md`. Monstrous Rage, Zoetic Glyph and Collector's Cage remain deferred.

- **Offspring**: Pawpatch Recruit, Darkstar Augur, Iridescent Vinelasher (mono), Manifold Mouse
- **Valiant / prowess**: Emberheart Challenger (mono), Heartfire Hero, Monastery Swiftspear
- **Plot**: Aloe Alchemist (mono), Slickshot Show-Off
- **Warp / Spree / Flurry / Start your engines**: Nova Hellkite (mono), Full Bore (mono), Phantom Interference, Cori-Steel Cutter, Burnout Bashtronaut (mono)
- **Older set keywords**: Make Disappear, Monstrous Rage, Hopeful Initiate (mono), Axebane Ferox (mono), Yotian Frontliner, Knight-Errant of Eos (mono), Sanguine Evangelist, Ruin-Lurker Bat, Forsaken Miner (mono), Chrome Host Seedshark (mono), Zoetic Glyph, Collector's Cage
- **Enduring / impending / disguise**: Enduring Curiosity, Enduring Innocence, Overlord of the Mistmoors, Flourishing Bloom-Kin (mono)
- **Daybound / nightbound**: Brutal Cathar (mono), Graveyard Trespasser

### E. Non-creature permanent frameworks (25 cards; 1 needed by a mono deck)

- **Cases, Classes, Rooms, Sagas**: Case of the Gateway Express, Case of the Uneaten Feast, Innkeeper's Talent, Stormchaser's Talent, Unholy Annex (mono), Fable of the Mirror-Breaker
- **Vehicles and Craft**: Reckoner Bankbuster, Subterranean Schooner, Spring-Loaded Sawblades, Braided Net, Clay-Fired Bricks, Thousand Moons Smithy
- **Equipment**: Basilisk Collar, Assimilation Aegis, The Irencrag
- **Exile-until-leaves (O-ring style)**: Hardlight Containment, Sheltered by Ghosts, Seam Rip, Dusk Rose Reliquary
- **Other artifacts and enchantments**: Candy Trail, Agatha's Soul Cauldron, Repurposing Bay, Simulacrum Synthesizer, Warleader's Call, Lunar Convocation

### F. Planeswalkers, transforming legends and big spells (10 cards; 5 needed by a mono deck)

- **Planeswalkers**: Chandra, Hope's Beacon, Kaito, Bane of Nightmares, Liliana of the Veil, Teferi, Temporal Pilgrim (mono)
- **Transforming legends**: Cecil, Dark Knight (mono), Etali, Primal Conqueror, Ojer Axonil, Deepest Might (mono), Polukranos Reborn (mono)
- **Big spells**: Breach the Multiverse, Blue Sun's Twilight (mono)

### G. Creatures with triggered and static abilities (48 cards; 22 needed by a mono deck)

`kernel_carddb_standard/v3` adds 14 Full and seven Partial family G definitions. Kellan, Planar Trailblazer remains FDN-owned. Recruitment Officer, Quirion Beastcaller, Extraction Specialist, Sharp-Eyed Rookie, Evolving Adaptive, Thalia and Haughty Djinn are excluded from full deck admission until their printed behavior is complete (`docs/design/standard_family_g_v1.md`).

- **ETB / dies / attack triggers**: Deep-Cavern Bat (mono), Sentinel of the Nameless City (mono), Spyglass Siren, Bloodtithe Harvester, Brightglass Gearhulk, Extraction Specialist (mono), Faerie Dreamthief, Floodpits Drowner, Gatekeeper of Malakir (mono), Glissa Sunslayer, Hired Claw (mono), Preacher of the Schism, Recruitment Officer (mono), Resolute Reinforcements, Sandstorm Salvager, Tersa Lightshatter, Tishana's Tidebinder, Tranquil Frillback, Unstoppable Slasher (mono), Zoraline, Cosmos Caller, Cenote Scout (mono), Novice Inspector (mono), Sharp-Eyed Rookie (mono), Dark Confidant, Essence Channeler, Kellan, Planar Trailblazer (mono)
- **Spell-cast and counter growth**: Ascendant Packleader (mono), Evolving Adaptive (mono), Quirion Beastcaller (mono), Teething Wurmlet, Hullbreaker Horror (mono), Surrak, Elusive Hunter, Warden of the Inner Sky (mono)
- **Statics and anthems**: Adeline, Resplendent Cathar (mono), Sheoldred, the Apocalypse, Thalia, Guardian of Thraben (mono), Bloodletter of Aclazotz (mono), Coppercoat Vanguard (mono), Haughty Djinn (mono), Razorkin Needlehead (mono), Regal Bunnicorn, Surge Engine, Gingerbrute, Tough Cookie
- **Adventures**: Imodane's Recruiter, Mosswood Dreadknight, Questing Druid, Virtue of Loyalty

### H. Legends for the 5-color deck (11 cards; 0 needed by a mono deck)

- **Legends**: Jodah, the Unifier, Katilda, Dawnhart Prime, Lagrella, the Magpie, Shanna, Purifying Blade, Melira, the Living Cure, Gwenna, Eyes of Gaea, Hajar, Loyal Bodyguard, Halana and Alena, Partners, Djeru and Hazoret, Ertai Resurrected, Skrelv, Defector Mite

Memory Deluge is Partial: its current implementation keeps the bottomed cards in looked-at order, so full deck admission refuses it until subset randomization is implemented. The registry retains its definition and flashback behavior for development.
