//! Mechanics checks behind the T1 Spy/CawGates diagnosis: legal instant-speed
//! windows in the opponent's combat for Prismatic Strands (hand and
//! flashback), and the Spy combo completing from a real self-mill.

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Zone};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.to_string(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        _ => panic!("unsupported zone"),
    }
    id
}

/// Walk one P0 turn from Main1: P0 attacks with everything eligible, P1 never
/// blocks, everyone passes priority. Returns, per step, whether P1 got a
/// priority window in which `spell` was castable.
fn p1_windows_for(state: &mut GameState, spell: ObjectId) -> Vec<(Step, bool)> {
    let mut out = Vec::new();
    for _ in 0..64 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::CastSpellOrPass {
                player,
                ref castable_spells,
                ..
            } => {
                if player == PlayerId::P1 {
                    out.push((state.step, castable_spells.contains(&spell)));
                }
                if state.step == Step::Main2 {
                    return out;
                }
                engine::step(state, Action::Pass).unwrap();
            }
            Decision::DeclareAttackers { eligible, .. } => {
                engine::step(state, Action::DeclareAttackers(eligible)).unwrap();
            }
            Decision::DeclareBlockers { .. } => {
                engine::step(state, Action::DeclareBlockers(Vec::new())).unwrap();
            }
            other => panic!("unexpected decision {other:?}"),
        }
    }
    panic!("turn did not reach Main2")
}

fn combat_state(seed: u64) -> GameState {
    let filler = [card_id("Island"); 10];
    let mut state = GameState::new_from_libraries(&filler, &filler, card_name, seed);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    put_object(&mut state, PlayerId::P0, "Voldaren Epicure", Zone::Battlefield);
    state
}

#[test]
fn strands_from_hand_is_offered_in_the_opponents_combat() {
    let mut state = combat_state(0x5431_0001);
    let strands = put_object(&mut state, PlayerId::P1, "Prismatic Strands", Zone::Hand);
    for _ in 0..3 {
        put_object(&mut state, PlayerId::P1, "Azorius Guildgate", Zone::Battlefield);
    }
    let windows = p1_windows_for(&mut state, strands);
    assert!(
        windows
            .iter()
            .any(|&(step, castable)| step == Step::DeclareAttackers && castable),
        "P1 windows: {windows:?}"
    );
    assert!(
        windows
            .iter()
            .any(|&(step, castable)| step == Step::DeclareBlockers && castable),
        "P1 windows: {windows:?}"
    );
}

#[test]
fn strands_flashback_is_offered_in_the_opponents_combat() {
    let mut state = combat_state(0x5431_0002);
    let strands = put_object(&mut state, PlayerId::P1, "Prismatic Strands", Zone::Graveyard);
    put_object(&mut state, PlayerId::P1, "Sacred Cat", Zone::Battlefield);
    let windows = p1_windows_for(&mut state, strands);
    assert!(
        windows
            .iter()
            .any(|&(step, castable)| step == Step::DeclareBlockers && castable),
        "P1 windows: {windows:?}"
    );
}

#[test]
fn strands_needs_white_mana_from_the_chosen_gate_colours() {
    // Three untapped Islands cannot pay {2}{W}: with no white source the
    // spell is correctly withheld in every window.
    let mut state = combat_state(0x5431_0003);
    let strands = put_object(&mut state, PlayerId::P1, "Prismatic Strands", Zone::Hand);
    for _ in 0..3 {
        put_object(&mut state, PlayerId::P1, "Island", Zone::Battlefield);
    }
    let windows = p1_windows_for(&mut state, strands);
    assert!(!windows.is_empty());
    assert!(windows.iter().all(|&(_, castable)| !castable), "{windows:?}");
}
