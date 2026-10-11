# MageZero Standard pool inventory

The `standard-magezero-fixtures` completion candidate targets all 16 unchanged MageZero
Standard opponent decks in `data/standard/magezero_v1/`. Its v7 registry contains the
192-definition Pauper prefix and 253 appended definitions, with all 225 distinct nonbasic
deck cards marked Full for acceptance testing. Python admission resolves all 16 decks;
there are zero Partial or missing deck entries. The extension includes 32 token or
masked-face definitions. Existing Standard IDs retain their append order.

**Runtime verification is pending.** Admission flags and source coverage do not prove
complete gameplay support. The integration lead owns compilation, affected behavior tests,
the observed catalog hash, terminal public-session execution/replay, and final review.
The current verification record is `docs/reports/standard_completion_v1.md`.

## Scope and ownership

The completion integrates families A through H below in one deliverable. Lands/spells,
keyword and creature workers supplied bounded source checkpoints; the integration lead
owns cross-family repairs and final acceptance. Shared FDN cards are copied into the
Standard extension so FDN batches cannot shift Standard IDs. Deck files, the Pauper
registry and the FDN registry remain unchanged.

The family lists below index the source coverage. They are not a list of missing cards
or independent per-card verification receipts.

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

The completion includes the shared FDN spells, player-controlled card selection, all six
Gix's Command mode pairs, and seeded random bottom order for Memory Deluge.

- **Removal and burn**: Cut Down, Go for the Throat, Lightning Strike (mono), Shock (mono), Destroy Evil (mono), Sheoldred's Edict, Shoot the Sheriff (mono), Hard-Hitting Question (mono), Maelstrom Pulse, Tear Asunder, Witchstalker Frenzy, Fading Hope (mono), Get Lost (mono), Anoint with Affliction, Gleeful Demolition, Invoke Despair, Gix's Command, Abrade, Boltwave
- **Counterspells**: Dissipate (mono), Essence Scatter (mono), Negate (mono)
- **Card selection and draw**: Consider (mono), Opt, Impulse (mono), Sleight of Hand, Stock Up, Thirst for Discovery (mono), Flow of Knowledge (mono), Memory Deluge (mono), Big Score, United Battlefront, Kayla's Reconstruction
- **Protection tricks**: Shore Up, Snakeskin Veil

### D. New set keywords (2022-2025) (32 cards; 13 needed by a mono deck)

The completion includes the earlier Partial cards plus Monstrous Rage, Zoetic Glyph and
Collector's Cage. Ward, convoke, casualty, disguise, hideaway and discover use explicit
choices; see `docs/design/standard_family_d_keywords_v1.md`.

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

The completion includes the shared FDN creatures, target/allocation choices, departed
creature characteristics, permanent restriction expiry, spell-cost adjustments and
randomized bottom order. See `docs/design/standard_family_g_v1.md`.

- **ETB / dies / attack triggers**: Deep-Cavern Bat (mono), Sentinel of the Nameless City (mono), Spyglass Siren, Bloodtithe Harvester, Brightglass Gearhulk, Extraction Specialist (mono), Faerie Dreamthief, Floodpits Drowner, Gatekeeper of Malakir (mono), Glissa Sunslayer, Hired Claw (mono), Preacher of the Schism, Recruitment Officer (mono), Resolute Reinforcements, Sandstorm Salvager, Tersa Lightshatter, Tishana's Tidebinder, Tranquil Frillback, Unstoppable Slasher (mono), Zoraline, Cosmos Caller, Cenote Scout (mono), Novice Inspector (mono), Sharp-Eyed Rookie (mono), Dark Confidant, Essence Channeler, Kellan, Planar Trailblazer (mono)
- **Spell-cast and counter growth**: Ascendant Packleader (mono), Evolving Adaptive (mono), Quirion Beastcaller (mono), Teething Wurmlet, Hullbreaker Horror (mono), Surrak, Elusive Hunter, Warden of the Inner Sky (mono)
- **Statics and anthems**: Adeline, Resplendent Cathar (mono), Sheoldred, the Apocalypse, Thalia, Guardian of Thraben (mono), Bloodletter of Aclazotz (mono), Coppercoat Vanguard (mono), Haughty Djinn (mono), Razorkin Needlehead (mono), Regal Bunnicorn, Surge Engine, Gingerbrute, Tough Cookie
- **Adventures**: Imodane's Recruiter, Mosswood Dreadknight, Questing Druid, Virtue of Loyalty

### H. Legends for the 5-color deck (11 cards; 0 needed by a mono deck)

- **Legends**: Jodah, the Unifier, Katilda, Dawnhart Prime, Lagrella, the Magpie, Shanna, Purifying Blade, Melira, the Living Cure, Gwenna, Eyes of Gaea, Hajar, Loyal Bodyguard, Halana and Alena, Partners, Djeru and Hazoret, Ertai Resurrected, Skrelv, Defector Mite

Mirrex poison and restricted floating mana for Rockface Village and the other restricted
sources are represented in generic observations. The remaining utility, channel and
Restless lands and Soulstone Sanctuary are included in the candidate; see
`docs/design/standard_lands_v1.md`.
