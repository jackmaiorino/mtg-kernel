//! Focused coverage for spell-local dynamic generic-cost reduction.
//! Cryptic Serpent is the first consumer: its printed `{5}{U}{U}` cost is
//! reduced by one generic mana for each instant or sorcery card in its
//! controller's graveyard.

use mtg_kernel::card_def::{card_id_by_name, preflight_fully_supported_deck, CARD_DEFS};
use mtg_kernel::engine::{
    self, Action, Decision, PlayOrCast, PlayPermission, PlayPermissionExpiry,
};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::rl::{legal_action_candidates_v1, ActionSemanticV1};
use mtg_kernel::state::{Counters, GameObject, GameState, Step, Zone};
use mtg_kernel::surface_v2::SurfaceDecision;

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
        name: name.to_string(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: mtg_kernel::state::ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not construct stack objects"),
    }
    id
}

fn ready_state(
    island_count: usize,
    controller_graveyard: &[&str],
    opponent_graveyard: &[&str],
) -> (GameState, ObjectId, Vec<ObjectId>) {
    let p0_library = [card_id("Mountain")];
    let p1_library = [card_id("Snow-Covered Forest")];
    let mut state =
        GameState::new_from_libraries(&p0_library, &p1_library, card_name, 0x4352_5950_5449_4353);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    let serpent = put_object(&mut state, PlayerId::P0, "Cryptic Serpent", Zone::Hand);
    let islands = (0..island_count)
        .map(|_| put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield))
        .collect::<Vec<_>>();
    for &name in controller_graveyard {
        put_object(&mut state, PlayerId::P0, name, Zone::Graveyard);
    }
    for &name in opponent_graveyard {
        put_object(&mut state, PlayerId::P1, name, Zone::Graveyard);
    }
    (state, serpent, islands)
}

fn priority_decision(state: &GameState) -> Decision {
    engine::advance_until_decision(&mut state.clone())
}

fn is_offered(state: &GameState, serpent: ObjectId) -> bool {
    matches!(
        priority_decision(state),
        Decision::CastSpellOrPass { castable_spells, .. }
            if castable_spells.contains(&serpent)
    )
}

fn cast_action_id(state: &GameState, serpent: ObjectId) -> String {
    let decision = priority_decision(state);
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision), state)
        .expect("legal action projection")
        .into_iter()
        .find_map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::CastSpell { source, .. } if source.arena_id == serpent.0 => {
                Some(candidate.record.stable_id)
            }
            _ => None,
        })
        .expect("Cryptic Serpent cast action")
}

fn cast_and_pay(state: &mut GameState, serpent: ObjectId) {
    assert!(is_offered(state, serpent));
    engine::step(state, Action::CastSpell(serpent)).unwrap();
    let decision = engine::advance_until_decision(state);
    assert!(matches!(
        decision,
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    assert_eq!(state.objects.get(serpent).zone, Zone::Stack);
    assert!(state.stack.iter().any(|item| item.source == serpent));
}

fn resolve_top_spell(state: &mut GameState) {
    engine::step(state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(state, Action::Pass).unwrap();
    let _ = engine::advance_until_decision(state);
}

#[test]
fn cryptic_serpent_reduces_generic_for_each_physical_instant_or_sorcery() {
    let cases: &[(&[&str], usize)] = &[
        (&[], 7),
        (&["Lightning Bolt"], 6),
        (
            &[
                "Lightning Bolt",
                "Preordain",
                "Mental Note",
                "Ponder",
                "Thought Scour",
            ],
            2,
        ),
        (
            &[
                "Lightning Bolt",
                "Preordain",
                "Mental Note",
                "Ponder",
                "Thought Scour",
                "Brainstorm",
                "Counterspell",
                "Dispel",
            ],
            2,
        ),
    ];

    for &(graveyard, islands_needed) in cases {
        let (mut state, serpent, islands) = ready_state(islands_needed, graveyard, &[]);
        cast_and_pay(&mut state, serpent);
        assert_eq!(
            islands
                .iter()
                .filter(|&&id| state.objects.get(id).tapped)
                .count(),
            islands_needed,
            "the exact effective cost should be paid for {graveyard:?}"
        );
    }
}

#[test]
fn reduction_ignores_other_types_and_the_opponents_graveyard() {
    let own = ["Lightning Bolt", "Preordain", "Mountain", "Masked Meower"];
    let opposing = ["Lightning Bolt", "Preordain", "Brainstorm"];
    let (state, serpent, _) = ready_state(5, &own, &opposing);
    assert!(
        is_offered(&state, serpent),
        "only the two own spells reduce 5UU to 3UU"
    );

    let (short, serpent, _) = ready_state(4, &own, &opposing);
    assert!(
        !is_offered(&short, serpent),
        "lands, creatures, and the opponent's spells must not supply the missing reduction"
    );
    let before = short.clone();
    let mut rejected = short;
    assert!(engine::step(&mut rejected, Action::CastSpell(serpent)).is_err());
    assert_eq!(rejected, before);
}

#[test]
fn excess_reduction_never_removes_the_two_blue_pips() {
    let graveyard = [
        "Lightning Bolt",
        "Preordain",
        "Mental Note",
        "Ponder",
        "Thought Scour",
        "Brainstorm",
        "Counterspell",
        "Dispel",
    ];
    let (one_blue, serpent, _) = ready_state(1, &graveyard, &[]);
    assert!(!is_offered(&one_blue, serpent));

    let (mut two_blue, serpent, islands) = ready_state(2, &graveyard, &[]);
    cast_and_pay(&mut two_blue, serpent);
    assert!(islands.iter().all(|&id| two_blue.objects.get(id).tapped));
}

#[test]
fn cryptic_serpent_cast_action_is_snapshot_stable_and_cost_neutral() {
    let (mut state, serpent, _) = ready_state(7, &[], &[]);
    preflight_fully_supported_deck(&[state.objects.get(serpent).card_def]).unwrap();
    let stable_id = cast_action_id(&state, serpent);
    let snapshot = state.snapshot();
    let hash = state.state_hash();

    put_object(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Graveyard);
    assert_eq!(
        cast_action_id(&state, serpent),
        stable_id,
        "dynamic affordability must not reinterpret the semantic cast action"
    );

    state.restore(&snapshot);
    assert_eq!(state.state_hash(), hash);
    assert_eq!(cast_action_id(&state, serpent), stable_id);
}

#[test]
fn reduced_cryptic_serpent_resolves_as_the_registered_six_five_creature() {
    let graveyard = [
        "Lightning Bolt",
        "Preordain",
        "Mental Note",
        "Ponder",
        "Thought Scour",
    ];
    let (mut state, serpent, _) = ready_state(2, &graveyard, &[]);
    cast_and_pay(&mut state, serpent);
    resolve_top_spell(&mut state);

    assert_eq!(state.objects.get(serpent).zone, Zone::Battlefield);
    let def = &CARD_DEFS[state.objects.get(serpent).card_def as usize];
    assert_eq!(def.power, Some(6));
    assert_eq!(def.toughness, Some(5));
    assert!(state.players[0].battlefield.contains(&serpent));
}

#[test]
fn ordinary_exile_cast_permission_reuses_the_same_dynamic_cost_path() {
    let graveyard = [
        "Lightning Bolt",
        "Preordain",
        "Mental Note",
        "Ponder",
        "Thought Scour",
    ];
    let (mut state, serpent, islands) = ready_state(2, &graveyard, &[]);
    state.players[0].hand.retain(|&object| object != serpent);
    let serpent_generation = {
        let object = state.objects.get_mut(serpent);
        object.zone = Zone::Exile;
        object.zone_change_count += 1;
        object.zone_change_count
    };
    state.exile.push(serpent);
    state.engine.exile_play_permissions.push(PlayPermission {
        object: serpent,
        holder: PlayerId::P0,
        zone_change_generation: serpent_generation,
        play_or_cast: PlayOrCast::Cast,
        expiry: PlayPermissionExpiry::EndOfTurn,
    });

    cast_and_pay(&mut state, serpent);
    assert!(islands.iter().all(|&id| state.objects.get(id).tapped));
}
