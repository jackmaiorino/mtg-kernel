//! MageZero Standard catalog: the frozen Pauper prefix plus
//! `data/standard/magezero_v1/cards_v1.json`, kept apart from FDN Limited ids.
#![cfg(feature = "standard-magezero-fixtures")]

use std::collections::BTreeSet;
use std::path::Path;

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, CardType, Subtype, CARD_DEFS,
    KERNEL_CARDDB_HASH,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

/// Appended after the 192 Pauper definitions, in registry order.
const STANDARD_APPENDED: [&str; 253] = [
    "Plains",
    "Burst Lightning",
    "Shock",
    "Lightning Strike",
    "Negate",
    "Opt",
    "Consider",
    "Impulse",
    "Dissipate",
    "Thirst for Discovery",
    "Flow of Knowledge",
    "Memory Deluge",
    "Destroy Evil",
    "Shoot the Sheriff",
    "Hard-Hitting Question",
    "Fading Hope",
    "Get Lost",
    "Novice Inspector",
    "Sentinel of the Nameless City",
    "Cenote Scout",
    "Gatekeeper of Malakir",
    "Deep-Cavern Bat",
    "Razorkin Needlehead",
    "Ascendant Packleader",
    "Sharp-Eyed Rookie",
    "Evolving Adaptive",
    "Quirion Beastcaller",
    "Unstoppable Slasher",
    "Coppercoat Vanguard",
    "Human Token",
    "Adeline, Resplendent Cathar",
    "Bloodletter of Aclazotz",
    "Thalia, Guardian of Thraben",
    "Haughty Djinn",
    "Hired Claw",
    "Warden of the Inner Sky",
    "Extraction Specialist",
    "Hullbreaker Horror",
    "Recruitment Officer",
    "Emberheart Challenger",
    "Burnout Bashtronaut",
    "Nova Hellkite",
    "Full Bore",
    "Iridescent Vinelasher",
    "Iridescent Vinelasher Offspring Token",
    "Aloe Alchemist",
    "Forsaken Miner",
    "Axebane Ferox",
    "Hopeful Initiate",
    "Chrome Host Seedshark",
    "Incubator Token",
    "Brutal Cathar",
    "Knight-Errant of Eos",
    "Monastery Swiftspear",
    "Heartfire Hero",
    "Slickshot Show-Off",
    "Sanguine Evangelist",
    "Bat Token",
    "Darkstar Augur",
    "Darkstar Augur Offspring Token",
    "Ruin-Lurker Bat",
    "Pawpatch Recruit",
    "Pawpatch Recruit Offspring Token",
    "Manifold Mouse",
    "Manifold Mouse Offspring Token",
    "Yotian Frontliner",
    "Cori-Steel Cutter",
    "Monk Token",
    "Graveyard Trespasser",
    "Overlord of the Mistmoors",
    "White Insect Token",
    "Enduring Curiosity",
    "Enduring Innocence",
    "Make Disappear",
    "Phantom Interference",
    "Spirit Token",
    "Flourishing Bloom-Kin",
    "Mishra's Foundry",
    "Eiganjo, Seat of the Empire",
    "Mirrex",
    "Rockface Village",
    "Phyrexian Mite Token",
    "Adarkar Wastes",
    "Battlefield Forge",
    "Brushland",
    "Caves of Koilos",
    "Karplusan Forest",
    "Llanowar Wastes",
    "Shivan Reef",
    "Sulfurous Springs",
    "Underground River",
    "Yavimaya Coast",
    "Blackcleave Cliffs",
    "Blooming Marsh",
    "Concealed Courtyard",
    "Copperline Gorge",
    "Darkslick Shores",
    "Inspiring Vantage",
    "Razorverge Thicket",
    "Seachrome Coast",
    "Spirebluff Canal",
    "Deserted Beach",
    "Dreamroot Cascade",
    "Haunted Ridge",
    "Overgrown Farmland",
    "Rockfall Vale",
    "Floodfarm Verge",
    "Gloomlake Verge",
    "Hushwood Verge",
    "Riverpyre Verge",
    "Thornspire Verge",
    "Wastewood Verge",
    "Elegant Parlor",
    "Lush Portico",
    "Underground Mortuary",
    "Jetmir's Garden",
    "Spara's Headquarters",
    "Ziatora's Proving Ground",
    "Starting Town",
    "Teferi, Temporal Pilgrim",
    "Teferi Spirit Token",
    "Cecil, Dark Knight",
    "Polukranos Reborn",
    "Phyrexian Hydra Reach Token",
    "Phyrexian Hydra Lifelink Token",
    "Ojer Axonil, Deepest Might",
    "Blue Sun's Twilight",
    "Unholy Annex // Ritual Chamber",
    "Demon Flying Token",
    "Seam Rip",
    "Dusk Rose Reliquary",
    "Sheltered by Ghosts",
    "Hardlight Containment",
    "Basilisk Collar",
    "Candy Trail",
    "Warleader's Call",
    "Lunar Convocation",
    "Bat Flying Token",
    "Simulacrum Synthesizer",
    "Karn Construct Token",
    "Case of the Gateway Express",
    "Innkeeper's Talent",
    "Stormchaser's Talent",
    "Otter Prowess Token",
    "Case of the Uneaten Feast",
    "Liliana of the Veil",
    "Breach the Multiverse",
    "Fable of the Mirror-Breaker",
    "Fable Goblin Shaman Token",
    "Repurposing Bay",
    "The Irencrag",
    "Clay-Fired Bricks",
    "Cosmium Gnome Token",
    "Braided Net",
    "Chandra, Hope's Beacon",
    "Assimilation Aegis",
    "Agatha's Soul Cauldron",
    "Reckoner Bankbuster",
    "Subterranean Schooner",
    "Spring-Loaded Sawblades",
    "Pilot Token",
    "Resolute Reinforcements",
    "Soldier Token",
    "Spyglass Siren",
    "Sheoldred, the Apocalypse",
    "Dark Confidant",
    "Regal Bunnicorn",
    "Abrade",
    "Boltwave",
    "Essence Scatter",
    "Snakeskin Veil",
    "Sleight of Hand",
    "Stock Up",
    "Shore Up",
    "Big Score",
    "Cut Down",
    "Go for the Throat",
    "Anoint with Affliction",
    "Gleeful Demolition",
    "Phyrexian Goblin Token",
    "Soulstone Sanctuary",
    "Otawara, Soaring City",
    "Restless Bivouac",
    "Restless Cottage",
    "Restless Fortress",
    "Restless Reef",
    "Restless Ridgeline",
    "Restless Prairie",
    "Restless Vinestalk",
    "Sheoldred's Edict",
    "Invoke Despair",
    "Kellan, Planar Trailblazer",
    "Surge Engine",
    "Monstrous Rage",
    "Monster Role Token",
    "Gingerbrute",
    "Tough Cookie",
    "Faerie Dreamthief",
    "Teething Wurmlet",
    "Surrak, Elusive Hunter",
    "Bloodtithe Harvester",
    "Sandstorm Salvager",
    "Preacher of the Schism",
    "Golem Token",
    "Vampire Token",
    "United Battlefront",
    "Kayla's Reconstruction",
    "Lupinflower Village",
    "Fomori Vault",
    "Mudflat Village",
    "Boseiju, Who Endures",
    "Takenuma, Abandoned Mire",
    "Sokenzan, Crucible of Defiance",
    "Fountainport",
    "Maelstrom Pulse",
    "Tear Asunder",
    "Witchstalker Frenzy",
    "Colorless Spirit Token",
    "Fish Token",
    "Plaza of Heroes",
    "Gix's Command",
    "Imodane's Recruiter",
    "Virtue of Loyalty",
    "Knight Vigilance Token",
    "Kaito, Bane of Nightmares",
    "Face-down creature",
    "Face-down card",
    "Zoetic Glyph",
    "Collector's Cage",
    "Mosswood Dreadknight",
    "Questing Druid",
    "Floodpits Drowner",
    "Essence Channeler",
    "Brightglass Gearhulk",
    "Jodah, the Unifier",
    "Katilda, Dawnhart Prime",
    "Lagrella, the Magpie",
    "Shanna, Purifying Blade",
    "Melira, the Living Cure",
    "Gwenna, Eyes of Gaea",
    "Hajar, Loyal Bodyguard",
    "Halana and Alena, Partners",
    "Djeru and Hazoret",
    "Ertai Resurrected",
    "Skrelv, Defector Mite",
    "Etali, Primal Conqueror",
    "Tersa Lightshatter",
    "Tishana's Tidebinder",
    "Thousand Moons Smithy",
    "Gnome Soldier Token",
    "Glissa Sunslayer",
    "Tranquil Frillback",
    "Zoraline, Cosmos Caller",
];

/// All distinct nonbasic deck cards admitted by the completion candidate.
/// Runtime acceptance is tracked separately in standard_completion_v1.md.
const SUPPORTED_NONBASIC: [&str; 225] = [
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
];

/// Token and masked-face definitions needed by the Standard rules.
const STANDARD_TOKENS: [&str; 32] = [
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
];

const BASICS: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];

fn deck_dir() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/standard/magezero_v1/decks"
    ))
}

/// Mainboard (count, name) rows of an XMage `.dck` file.
fn mainboard(path: &Path) -> Vec<(u32, String)> {
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("SB:"))
        .map(|line| {
            let (count, rest) = line.split_once(' ').unwrap();
            let name = rest.split_once("] ").map_or(rest, |(_, name)| name).trim();
            (count.parse().unwrap(), name.to_owned())
        })
        .collect()
}

fn decks() -> Vec<(String, Vec<(u32, String)>)> {
    let mut paths: Vec<_> = std::fs::read_dir(deck_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            (name, mainboard(path))
        })
        .collect()
}

fn fully_supported(name: &str) -> bool {
    card_id_by_name(name)
        .is_some_and(|id| CARD_DEFS[id as usize].capability == CardCapability::Full)
}

#[test]
fn standard_registry_appends_to_the_pauper_prefix_without_fdn() {
    assert_eq!(CARD_DEFS.len(), 192 + STANDARD_APPENDED.len());
    for (offset, name) in STANDARD_APPENDED.iter().enumerate() {
        assert_eq!(card_id_by_name(name), Some((192 + offset) as u16), "{name}");
    }
    // FDN-only definitions never enter the Standard catalog.
    assert_eq!(card_id_by_name("Dwynen, Gilt-Leaf Daen"), None);
    assert_eq!(card_id_by_name("Ajani, Caller of the Pride"), None);
    assert!(CARD_DEFS
        .iter()
        .all(|def| def.capability == CardCapability::Full));
}

#[test]
fn standard_catalog_identity_is_frozen() {
    // Integration must freeze the observed native v7 value before delivery.
    // None deliberately fails this acceptance check while that receipt is pending.
    const EXPECTED_STANDARD_V7: Option<u64> = None;
    assert_eq!(
        Some(KERNEL_CARDDB_HASH),
        EXPECTED_STANDARD_V7,
        "Freeze the observed Standard v7 catalog hash {KERNEL_CARDDB_HASH:#018x}"
    );
}

#[test]
fn magezero_pool_is_the_sixteen_sixty_card_decks() {
    let decks = decks();
    assert_eq!(decks.len(), 16);
    for (name, rows) in &decks {
        let copies: u32 = rows.iter().map(|(count, _)| count).sum();
        let expected = if name == "Standard-MonoU" { 62 } else { 60 };
        assert_eq!(copies, expected, "{name}");
    }
}

#[test]
fn supported_deck_cards_match_the_tracked_list() {
    let mut supported = BTreeSet::new();
    for (_, rows) in decks() {
        for (_, name) in rows {
            if !BASICS.contains(&name.as_str()) && fully_supported(&name) {
                supported.insert(name);
            }
        }
    }
    let expected: BTreeSet<String> = SUPPORTED_NONBASIC.iter().map(|n| n.to_string()).collect();
    assert_eq!(supported, expected);
    for basic in BASICS {
        assert!(fully_supported(basic), "{basic}");
    }
    let partial: Vec<u16> = ["Plains", "Mountain", "Burst Lightning"]
        .map(|name| card_id_by_name(name).unwrap())
        .to_vec();
    assert!(preflight_fully_supported_deck(&partial).is_ok());
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(object),
        Zone::Battlefield => seat.battlefield.push(object),
        Zone::Graveyard => seat.graveyard.push(object),
        _ => panic!("helper zone"),
    }
    object
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

#[test]
fn burst_lightning_resolves_in_the_standard_build() {
    let mut state = GameState::new_from_libraries(&[], &[], |_| "Plains".into(), 0x5354_444d);
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
    let burst = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(burst)).unwrap();
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { .. } => engine::step(
                &mut state,
                Action::ChooseTarget(Target::Player(PlayerId::P1)),
            )
            .unwrap(),
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(state.players[1].life, 18);
    assert_eq!(state.objects.get(burst).zone, Zone::Graveyard);
}

/// Main 1 with P0 holding priority, P0's library built from `library`
/// (read back from the state, since construction may reorder it), and P0's
/// pool filled.
fn ready(library: &[&str], pool: &[(ManaColor, u8)]) -> GameState {
    let lib0: Vec<u16> = library
        .iter()
        .map(|name| card_id_by_name(name).unwrap())
        .collect();
    let mut state = GameState::new_from_libraries(
        &lib0,
        &[],
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5354_444d,
    );
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
    for &(color, amount) in pool {
        state.players[0].mana_pool[color.pool_index()] += amount;
    }
    state
}

/// Casts `spell` and passes priority until the stack is empty, answering
/// every other decision with `respond`.
fn cast_and_resolve(
    state: &mut GameState,
    spell: ObjectId,
    respond: &mut dyn FnMut(&mut GameState, Decision) -> Action,
) {
    next(state);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    resolve_stack(state, respond);
}

fn resolve_stack(
    state: &mut GameState,
    respond: &mut dyn FnMut(&mut GameState, Decision) -> Action,
) {
    loop {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => {
                let action = respond(state, other);
                engine::step(state, action).unwrap();
            }
        }
    }
}

fn no_decisions(_: &mut GameState, decision: Decision) -> Action {
    panic!("unexpected decision: {decision:?}")
}

fn target_object(object: ObjectId) -> impl FnMut(&mut GameState, Decision) -> Action {
    move |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(object)));
            Action::ChooseTarget(Target::Object(object))
        }
        other => panic!("unexpected decision: {other:?}"),
    }
}

fn battlefield_count(state: &GameState, player: PlayerId, name: &str) -> usize {
    let card_def = card_id_by_name(name).unwrap();
    state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&object| state.objects.get(object).card_def == card_def)
        .count()
}

#[test]
fn shock_and_lightning_strike_hit_any_target() {
    let mut state = ready(&[], &[(ManaColor::R, 3)]);
    let shock = put(&mut state, PlayerId::P0, "Shock", Zone::Hand);
    let strike = put(&mut state, PlayerId::P0, "Lightning Strike", Zone::Hand);
    let mut face = |_: &mut GameState, decision: Decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(PlayerId::P1)),
        other => panic!("unexpected decision: {other:?}"),
    };
    cast_and_resolve(&mut state, shock, &mut face);
    cast_and_resolve(&mut state, strike, &mut face);
    assert_eq!(state.players[1].life, 15);
    assert_eq!(state.objects.get(strike).zone, Zone::Graveyard);
}

#[test]
fn negate_counters_only_noncreature_spells_and_dissipate_exiles() {
    let mut state = ready(
        &[],
        &[(ManaColor::G, 1), (ManaColor::R, 2), (ManaColor::U, 5)],
    );
    let elves = put(&mut state, PlayerId::P0, "Llanowar Elves", Zone::Hand);
    let shock = put(&mut state, PlayerId::P0, "Shock", Zone::Hand);
    let second_shock = put(&mut state, PlayerId::P0, "Shock", Zone::Hand);
    let negate = put(&mut state, PlayerId::P0, "Negate", Zone::Hand);
    let dissipate = put(&mut state, PlayerId::P0, "Dissipate", Zone::Hand);
    let face = |_: &mut GameState, decision: Decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(PlayerId::P1)),
        other => panic!("unexpected decision: {other:?}"),
    };

    next(&mut state);
    engine::step(&mut state, Action::CastSpell(elves)).unwrap();
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(shock)).unwrap();
    let decision = next(&mut state);
    let action = face(&mut state, decision);
    engine::step(&mut state, action).unwrap();
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(negate)).unwrap();
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(shock)]);
        }
        other => panic!("unexpected decision: {other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(shock))).unwrap();
    resolve_stack(&mut state, &mut no_decisions);
    assert_eq!(state.objects.get(shock).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(elves).zone, Zone::Battlefield);
    assert_eq!(state.players[1].life, 20);

    next(&mut state);
    engine::step(&mut state, Action::CastSpell(second_shock)).unwrap();
    let decision = next(&mut state);
    let action = face(&mut state, decision);
    engine::step(&mut state, action).unwrap();
    cast_and_resolve(&mut state, dissipate, &mut target_object(second_shock));
    assert_eq!(state.objects.get(second_shock).zone, Zone::Exile);
    assert_eq!(state.objects.get(dissipate).zone, Zone::Graveyard);
    assert_eq!(state.players[1].life, 20);
}

#[test]
fn opt_and_consider_filter_then_draw() {
    let mut state = ready(&["Island"; 6], &[(ManaColor::U, 2)]);
    let opt = put(&mut state, PlayerId::P0, "Opt", Zone::Hand);
    let consider = put(&mut state, PlayerId::P0, "Consider", Zone::Hand);
    let library = state.players[0].library.clone();
    // Opt: scry the top card to the bottom, then draw the next one.
    cast_and_resolve(&mut state, opt, &mut |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(library[0])]);
            Action::ChooseEffectTarget(Target::Object(library[0]))
        }
        other => panic!("unexpected decision: {other:?}"),
    });
    assert!(state.players[0].hand.contains(&library[1]));
    assert_eq!(state.players[0].library.last(), Some(&library[0]));
    // Consider: surveil the top card into the graveyard, then draw.
    cast_and_resolve(&mut state, consider, &mut |_, decision| match decision {
        Decision::ChooseEffectOption {
            option_count: 2, ..
        } => Action::ChooseEffectOption(1),
        other => panic!("unexpected decision: {other:?}"),
    });
    assert_eq!(state.objects.get(library[2]).zone, Zone::Graveyard);
    assert!(state.players[0].hand.contains(&library[3]));
}

#[test]
fn impulse_keeps_one_privately_and_orders_the_rest_on_the_bottom() {
    let mut state = ready(&["Island"; 6], &[(ManaColor::U, 2)]);
    let impulse = put(&mut state, PlayerId::P0, "Impulse", Zone::Hand);
    let library = state.players[0].library.clone();
    let mut prompts = Vec::new();
    cast_and_resolve(&mut state, impulse, &mut |state, decision| match decision {
        Decision::ChooseEffectTargets {
            selected_count,
            min_targets,
            max_targets,
            legal_targets,
            ..
        } => {
            prompts.push((selected_count, min_targets, max_targets));
            // Each private stage is a valid, serializable continuation.
            mtg_kernel::effect::validate_pending_effect_choice(state).unwrap();
            let round_trip: GameState =
                serde_json::from_str(&serde_json::to_string(&*state).unwrap()).unwrap();
            assert_eq!(&round_trip, &*state);
            match (min_targets, selected_count) {
                (1, 0) => {
                    assert_eq!(
                        legal_targets,
                        library[..4]
                            .iter()
                            .map(|&o| Target::Object(o))
                            .collect::<Vec<_>>()
                    );
                    Action::ChooseEffectTarget(Target::Object(library[2]))
                }
                (3, 0) => Action::ChooseEffectTarget(Target::Object(library[3])),
                (3, 1) => Action::ChooseEffectTarget(Target::Object(library[0])),
                other => panic!("unexpected prompt {other:?}"),
            }
        }
        other => panic!("unexpected decision: {other:?}"),
    });
    // The last card of the ordered rest needs no prompt.
    assert_eq!(prompts, vec![(0, 1, 1), (0, 3, 3), (1, 3, 3)]);
    assert_eq!(state.objects.get(library[2]).zone, Zone::Hand);
    assert_eq!(
        state.players[0].library,
        vec![library[4], library[5], library[3], library[0], library[1]]
    );
}

#[test]
fn memory_deluge_looks_at_the_mana_spent_and_flashes_back() {
    let mut state = ready(&["Island"; 12], &[(ManaColor::U, 11)]);
    let deluge = put(&mut state, PlayerId::P0, "Memory Deluge", Zone::Hand);
    let library = state.players[0].library.clone();
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(deluge)).unwrap();
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.last().unwrap().v4.mana_spent.0, 4);
    let mut first = |_: &mut GameState, decision: Decision| match decision {
        Decision::ChooseEffectTargets {
            min_targets: 2,
            max_targets: 2,
            legal_targets,
            selected_count,
            ..
        } => {
            assert_eq!(legal_targets.len() + usize::from(selected_count), 4);
            Action::ChooseEffectTarget(Target::Object(
                [library[0], library[3]][usize::from(selected_count)],
            ))
        }
        other => panic!("unexpected decision: {other:?}"),
    };
    resolve_stack(&mut state, &mut first);
    assert_eq!(state.objects.get(library[0]).zone, Zone::Hand);
    assert_eq!(state.objects.get(library[3]).zone, Zone::Hand);
    // Randomization changes only the unselected bottom subset.
    let mut bottom = state.players[0].library[8..].to_vec();
    bottom.sort_unstable();
    let mut expected_bottom = vec![library[1], library[2]];
    expected_bottom.sort_unstable();
    assert_eq!(bottom, expected_bottom);
    assert_eq!(&state.players[0].library[..8], &library[4..]);

    // Flashback for {5}{U}{U} spends seven mana, so it looks at seven.
    assert_eq!(state.objects.get(deluge).zone, Zone::Graveyard);
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(deluge)).unwrap();
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.last().unwrap().v4.mana_spent.0, 7);
    let mut seen = 0;
    let mut second = |_: &mut GameState, decision: Decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count,
            ..
        } => {
            seen = seen.max(legal_targets.len() + usize::from(selected_count));
            Action::ChooseEffectTarget(legal_targets[0])
        }
        other => panic!("unexpected decision: {other:?}"),
    };
    resolve_stack(&mut state, &mut second);
    assert_eq!(seen, 7);
    assert_eq!(state.objects.get(deluge).zone, Zone::Exile);
    assert_eq!(state.players[0].hand.len(), 4);
}

#[test]
fn thirst_for_discovery_discards_a_basic_land_or_two_cards() {
    // Discarding the basic land replaces the two-card discard.
    let mut state = ready(&["Island"; 6], &[(ManaColor::U, 3)]);
    let thirst = put(&mut state, PlayerId::P0, "Thirst for Discovery", Zone::Hand);
    let kept = put(&mut state, PlayerId::P0, "Shock", Zone::Hand);
    let library = state.players[0].library.clone();
    cast_and_resolve(&mut state, thirst, &mut |_, decision| match decision {
        Decision::ChooseEffectTargets {
            min_targets: 0,
            max_targets: 1,
            legal_targets,
            ..
        } => {
            assert_eq!(
                legal_targets,
                library[..3]
                    .iter()
                    .map(|&o| Target::Object(o))
                    .collect::<Vec<_>>()
            );
            Action::ChooseEffectTarget(Target::Object(library[1]))
        }
        other => panic!("unexpected decision: {other:?}"),
    });
    assert_eq!(state.objects.get(library[1]).zone, Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 3);
    assert!(state.players[0].hand.contains(&kept));

    // Declining falls through to discarding two cards.
    let mut state = ready(&["Island"; 6], &[(ManaColor::U, 3)]);
    let thirst = put(&mut state, PlayerId::P0, "Thirst for Discovery", Zone::Hand);
    let kept = put(&mut state, PlayerId::P0, "Shock", Zone::Hand);
    let library = state.players[0].library.clone();
    cast_and_resolve(&mut state, thirst, &mut |_, decision| match decision {
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        Decision::Discard {
            count: 2, choices, ..
        } => {
            assert_eq!(choices.len(), 4);
            Action::Discard(vec![library[0], library[2]])
        }
        other => panic!("unexpected decision: {other:?}"),
    });
    assert_eq!(state.players[0].hand.len(), 2);
    assert!(state.players[0].hand.contains(&kept));

    // With no basic land in hand there is nothing to choose instead.
    let mut state = ready(&["Shock"; 6], &[(ManaColor::U, 3)]);
    let thirst = put(&mut state, PlayerId::P0, "Thirst for Discovery", Zone::Hand);
    cast_and_resolve(&mut state, thirst, &mut |_, decision| match decision {
        Decision::Discard {
            count: 2, choices, ..
        } => Action::Discard(choices[..2].to_vec()),
        other => panic!("unexpected decision: {other:?}"),
    });
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn flow_of_knowledge_draws_per_island_then_discards_two() {
    let mut state = ready(&["Shock"; 6], &[(ManaColor::U, 5)]);
    for _ in 0..3 {
        put(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    }
    put(&mut state, PlayerId::P0, "Plains", Zone::Battlefield);
    put(&mut state, PlayerId::P1, "Island", Zone::Battlefield);
    let flow = put(&mut state, PlayerId::P0, "Flow of Knowledge", Zone::Hand);
    let mut discarded = 0;
    cast_and_resolve(&mut state, flow, &mut |_, decision| match decision {
        Decision::Discard { count, choices, .. } => {
            assert_eq!(choices.len(), 3);
            discarded = count;
            Action::Discard(choices[..2].to_vec())
        }
        other => panic!("unexpected decision: {other:?}"),
    });
    assert_eq!(discarded, 2);
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.players[0].library.len(), 3);
}

#[test]
fn destroy_evil_modes_and_targets() {
    let mut state = ready(&[], &[(ManaColor::W, 4)]);
    let angler = put(&mut state, PlayerId::P1, "Gurmag Angler", Zone::Battlefield);
    let elves = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let munitions = put(
        &mut state,
        PlayerId::P1,
        "Makeshift Munitions",
        Zone::Battlefield,
    );
    let first = put(&mut state, PlayerId::P0, "Destroy Evil", Zone::Hand);
    let second = put(&mut state, PlayerId::P0, "Destroy Evil", Zone::Hand);
    let mut creature_mode = |_: &mut GameState, decision: Decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(0),
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(elves)));
            assert!(!legal_targets.contains(&Target::Object(munitions)));
            Action::ChooseTarget(Target::Object(angler))
        }
        other => panic!("unexpected decision: {other:?}"),
    };
    cast_and_resolve(&mut state, first, &mut creature_mode);
    assert_eq!(state.objects.get(angler).zone, Zone::Graveyard);
    let mut enchantment_mode = |_: &mut GameState, decision: Decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(1),
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(munitions)]);
            Action::ChooseTarget(Target::Object(munitions))
        }
        other => panic!("unexpected decision: {other:?}"),
    };
    cast_and_resolve(&mut state, second, &mut enchantment_mode);
    assert_eq!(state.objects.get(munitions).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(elves).zone, Zone::Battlefield);
}

#[test]
fn shoot_the_sheriff_spares_outlaws() {
    let mut state = ready(&[], &[(ManaColor::B, 2)]);
    let rogue = put(
        &mut state,
        PlayerId::P1,
        "Goblin Tomb Raider",
        Zone::Battlefield,
    );
    let changeling = put(
        &mut state,
        PlayerId::P1,
        "Webweaver Changeling",
        Zone::Battlefield,
    );
    let elves = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let sheriff = put(&mut state, PlayerId::P0, "Shoot the Sheriff", Zone::Hand);
    cast_and_resolve(&mut state, sheriff, &mut |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(rogue)));
            assert!(!legal_targets.contains(&Target::Object(changeling)));
            Action::ChooseTarget(Target::Object(elves))
        }
        other => panic!("unexpected decision: {other:?}"),
    });
    assert_eq!(state.objects.get(elves).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(rogue).zone, Zone::Battlefield);
}

#[test]
fn hard_hitting_question_deals_its_creatures_power() {
    let mut state = ready(&[], &[(ManaColor::G, 1)]);
    let terror = put(
        &mut state,
        PlayerId::P0,
        "Tolarian Terror",
        Zone::Battlefield,
    );
    let victim = put(&mut state, PlayerId::P1, "Gurmag Angler", Zone::Battlefield);
    let question = put(
        &mut state,
        PlayerId::P0,
        "Hard-Hitting Question",
        Zone::Hand,
    );
    let mut picks = [terror, victim].into_iter();
    cast_and_resolve(&mut state, question, &mut |_, decision| match decision {
        Decision::ChooseTargets { .. } => {
            Action::ChooseTarget(Target::Object(picks.next().unwrap()))
        }
        other => panic!("unexpected decision: {other:?}"),
    });
    assert_eq!(state.objects.get(victim).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(terror).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(terror).damage, 0);
}

#[test]
fn fading_hope_scries_only_after_bouncing_a_cheap_creature() {
    let mut state = ready(&["Island"; 3], &[(ManaColor::U, 2)]);
    let elves = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let angler = put(&mut state, PlayerId::P1, "Gurmag Angler", Zone::Battlefield);
    let first = put(&mut state, PlayerId::P0, "Fading Hope", Zone::Hand);
    let second = put(&mut state, PlayerId::P0, "Fading Hope", Zone::Hand);
    let mut scried = false;
    cast_and_resolve(&mut state, first, &mut |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(elves)),
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => {
            scried = true;
            Action::FinishEffectSelection
        }
        other => panic!("unexpected decision: {other:?}"),
    });
    assert!(scried);
    assert_eq!(state.objects.get(elves).zone, Zone::Hand);
    assert!(state.players[1].hand.contains(&elves));
    // Mana value 7: bounced without a scry prompt.
    cast_and_resolve(&mut state, second, &mut target_object(angler));
    assert!(state.players[1].hand.contains(&angler));
}

#[test]
fn get_lost_gives_the_controller_two_maps() {
    let mut state = ready(&[], &[(ManaColor::W, 2)]);
    let munitions = put(
        &mut state,
        PlayerId::P1,
        "Makeshift Munitions",
        Zone::Battlefield,
    );
    let get_lost = put(&mut state, PlayerId::P0, "Get Lost", Zone::Hand);
    cast_and_resolve(&mut state, get_lost, &mut target_object(munitions));
    assert_eq!(state.objects.get(munitions).zone, Zone::Graveyard);
    assert_eq!(battlefield_count(&state, PlayerId::P1, "Map Token"), 2);
    assert_eq!(battlefield_count(&state, PlayerId::P0, "Map Token"), 0);
}

#[test]
fn get_lost_uses_the_live_controller_even_when_destruction_is_prevented() {
    for change_after_targeting in [false, true] {
        for indestructible in [false, true] {
            let mut state = ready(&[], &[(ManaColor::W, 2)]);
            let victim = put(
                &mut state,
                PlayerId::P1,
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let get_lost = put(&mut state, PlayerId::P0, "Get Lost", Zone::Hand);
            if indestructible {
                mtg_kernel::effect::execute(
                    &mtg_kernel::effect::EffectOp::GrantKeywordTargetUntilEndOfTurn {
                        object: mtg_kernel::effect::ObjectRef::ThisSource,
                        keyword: mtg_kernel::card_def::Keywords::INDESTRUCTIBLE,
                    },
                    &mtg_kernel::effect::ExecCtx::no_targets(victim, PlayerId::P1),
                    &mut state,
                );
                assert!(engine::has_effective_keyword(
                    &state,
                    victim,
                    mtg_kernel::card_def::Keywords::INDESTRUCTIBLE,
                ));
            }
            let transfer = |state: &mut GameState| {
                state.players[1]
                    .battlefield
                    .retain(|&object| object != victim);
                state.players[0].battlefield.push(victim);
                state.objects.get_mut(victim).controller = PlayerId::P0;
            };
            if !change_after_targeting {
                transfer(&mut state);
            }
            next(&mut state);
            engine::step(&mut state, Action::CastSpell(get_lost)).unwrap();
            assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
            engine::step(&mut state, Action::ChooseTarget(Target::Object(victim))).unwrap();
            // Choosing the target does not finalize casting. Control may change
            // in the priority window only after the spell is on the stack.
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert!(state.engine.pending_cast.is_none());
            assert_eq!(state.stack.last().unwrap().source, get_lost);
            if change_after_targeting {
                transfer(&mut state);
            }
            resolve_stack(&mut state, &mut no_decisions);
            assert_eq!(battlefield_count(&state, PlayerId::P0, "Map Token"), 2);
            assert_eq!(battlefield_count(&state, PlayerId::P1, "Map Token"), 0);
            assert_eq!(state.objects.get(victim).owner, PlayerId::P1);
            assert_eq!(
                state.objects.get(victim).zone,
                if indestructible {
                    Zone::Battlefield
                } else {
                    Zone::Graveyard
                }
            );
        }
    }
}

#[test]
fn all_sixteen_magezero_decks_pass_full_catalog_admission() {
    for (name, rows) in decks() {
        let mut deck = Vec::new();
        for (count, card_name) in rows {
            let id = card_id_by_name(&card_name).unwrap_or_else(|| panic!("{name}: {card_name}"));
            deck.extend(std::iter::repeat_n(id, count as usize));
        }
        preflight_fully_supported_deck(&deck).unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

#[test]
fn all_standard_tokens_are_full_token_definitions() {
    for name in STANDARD_TOKENS {
        let id = card_id_by_name(name).expect(name);
        let definition = &CARD_DEFS[id as usize];
        assert!(definition.is_token, "{name}");
        assert_eq!(definition.capability, CardCapability::Full, "{name}");
    }
}

#[test]
fn standard_creature_subtypes_are_available_to_type_choices_and_changeling() {
    for name in STANDARD_APPENDED {
        let definition = &CARD_DEFS[card_id_by_name(name).expect(name) as usize];
        if !definition.has_type(CardType::Creature) {
            continue;
        }
        for &subtype in definition.subtypes {
            // Food remains an artifact type on Gingerbrute and Tough Cookie.
            if subtype == Subtype::Food {
                assert!(!subtype.is_creature_type());
                assert!(!Subtype::CREATURE_TYPES.contains(&subtype));
                continue;
            }
            assert!(subtype.is_creature_type(), "{name}: {subtype:?}");
            assert!(
                Subtype::CREATURE_TYPES.contains(&subtype),
                "{name}: {subtype:?}"
            );
        }
    }
    assert!(Subtype::OUTLAW_TYPES.contains(&Subtype::Warlock));
    assert_eq!(Subtype::Elder.stable_id(), 143);
    assert_eq!(Subtype::Wurm.stable_id(), 144);
    assert_eq!(Subtype::Artificer.stable_id(), 145);
    assert_eq!(Subtype::Orc.stable_id(), 146);
}
