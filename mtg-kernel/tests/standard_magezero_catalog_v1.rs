//! MageZero Standard catalog: the frozen Pauper prefix plus
//! `data/standard/magezero_v1/cards_v1.json`, kept apart from FDN Limited ids.
#![cfg(feature = "standard-magezero-fixtures")]

use std::collections::BTreeSet;
use std::path::Path;

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, CARD_DEFS, KERNEL_CARDDB_HASH,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

/// Appended after the 192 Pauper definitions, in registry order.
const STANDARD_APPENDED: [&str; 29] = [
    "Plains",
    "Burst Lightning",
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
];

/// Every distinct nonbasic card in the 16 decks that this build fully
/// supports. Each Standard card batch extends this list.
const SUPPORTED_NONBASIC: [&str; 26] = [
    "Basilisk Collar",
    "Blue Sun's Twilight",
    "Burst Lightning",
    "Candy Trail",
    "Case of the Gateway Express",
    "Case of the Uneaten Feast",
    "Cecil, Dark Knight",
    "Duress",
    "Dusk Rose Reliquary",
    "Hardlight Containment",
    "Innkeeper's Talent",
    "Liliana of the Veil",
    "Llanowar Elves",
    "Lunar Convocation",
    "Ojer Axonil, Deepest Might",
    "Polukranos Reborn",
    "Seam Rip",
    "Sheltered by Ghosts",
    "Simulacrum Synthesizer",
    "Spell Pierce",
    "Stormchaser's Talent",
    "Teferi, Temporal Pilgrim",
    "Tolarian Terror",
    "Unholy Annex // Ritual Chamber",
    "Voldaren Epicure",
    "Warleader's Call",
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
        .all(|def| def.capability == CardCapability::Full || def.is_token));
}

#[test]
fn standard_catalog_identity_is_frozen() {
    const EXPECTED_STANDARD_V1: u64 = 0xef7b_203d_802e_09e8;
    assert_eq!(
        KERNEL_CARDDB_HASH, EXPECTED_STANDARD_V1,
        "Standard catalog hash {KERNEL_CARDDB_HASH:#018x}"
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
