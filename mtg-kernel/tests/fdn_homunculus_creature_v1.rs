//! Homunculus Horde copiable characteristics and second-draw timing.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SuppressionReason, SurfaceAction,
    SurfaceDecision,
};
use mtg_kernel::trigger;

fn ready() -> GameState {
    let island = card_id_by_name("Island").unwrap();
    let mut state =
        GameState::new_from_libraries(&[island; 40], &[island; 40], |_| "Island".into(), 123);
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let object = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Battlefield => state.players[player.index()].battlefield.push(object),
        Zone::Hand => state.players[player.index()].hand.push(object),
        _ => panic!("helper zone"),
    }
    object
}

fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(decision @ Decision::Halted { .. }) => panic!("{decision:?}"),
        SurfaceDecision::Decision(decision) => decision,
        other => panic!("expected decision: {other:?}"),
    }
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

fn drain(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    for _ in 0..100 {
        let decision = next(surface, state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            assert!(state.engine.pending_triggers.is_empty());
            assert!(state.engine.pending_discard.is_none());
            // Combat may have auto-declared the only legal empty block set.
            // Actual priority and card choices must remain available.
            assert!(
                surface
                    .suppressions()
                    .iter()
                    .all(|entry| entry.reason == SuppressionReason::NoEligibleBlockersForAttacker),
                "{:?}",
                surface.suppressions()
            );
            return;
        }
        let action = match decision {
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::Discard { choices, count, .. } => {
                Action::Discard(choices.into_iter().take(count as usize).collect())
            }
            Decision::CastSpellOrPass { .. } => Action::Pass,
            other => panic!("unexpected decision: {other:?}"),
        };
        apply(surface, state, action);
    }
    panic!("did not finish resolving");
}

fn collect(state: &mut GameState) {
    let pending = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(pending);
}

fn draw_batch(state: &mut GameState, player: PlayerId, count: usize) {
    for _ in 0..count {
        event::propose_and_commit(state, ProposedEvent::draw(player));
    }
    collect(state);
}

fn hordes(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| {
            CARD_DEFS[state.objects.get(id).card_def as usize].object_name == "Homunculus Horde"
        })
        .collect()
}

#[test]
fn printed_and_copy_definitions_preserve_all_copiable_characteristics() {
    assert_eq!(card_id_by_name("Homunculus Horde"), Some(223));
    assert_eq!(card_id_by_name("Homunculus Horde Token"), Some(224));
    let original = &CARD_DEFS[223];
    let token = &CARD_DEFS[224];
    for def in [original, token] {
        assert_eq!(def.object_name, "Homunculus Horde");
        assert_eq!((def.power, def.toughness), (Some(2), Some(2)));
        assert_eq!(def.mana_value, 4);
        assert_eq!(def.colors, &[ManaColor::U]);
        assert_eq!(def.subtypes, &[Subtype::Homunculus]);
        assert_eq!(def.keywords.0, Keywords::NONE.0);
    }
    assert_eq!(original.cost, token.cost);
    assert_eq!(original.types, token.types);
    assert_eq!(original.supertypes, token.supertypes);
    assert!(!original.is_token);
    assert!(token.is_token);
    preflight_fully_supported_deck(&[223]).unwrap();
    assert!(preflight_fully_supported_deck(&[224]).is_err());
}

#[test]
fn horde_casts_for_exactly_four_mana() {
    let mut state = ready();
    let horde = put(&mut state, PlayerId::P0, "Homunculus Horde", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 3;
    let before = state.state_hash();
    assert!(engine::step(&mut state, Action::CastSpell(horde)).is_err());
    assert_eq!(state.state_hash(), before);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 4;
    let mut surf = surface();
    next(&mut surf, &mut state);
    apply(&mut surf, &mut state, Action::CastSpell(horde));
    drain(&mut surf, &mut state);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert_eq!(state.objects.get(horde).zone, Zone::Battlefield);
}

#[test]
fn only_the_second_draw_creates_one_copy_without_retroactive_triggers() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Homunculus Horde",
        Zone::Battlefield,
    );
    for count in 1..=4 {
        draw_batch(&mut state, PlayerId::P0, 1);
        assert_eq!(state.engine.pending_triggers.len(), usize::from(count == 2));
        drain(&mut surface(), &mut state);
        assert_eq!(
            hordes(&state, PlayerId::P0).len(),
            if count < 2 { 1 } else { 2 }
        );
    }
}

#[test]
fn four_card_batch_creates_one_copy_that_keeps_its_trigger() {
    let mut state = ready();
    let source = put(
        &mut state,
        PlayerId::P0,
        "Homunculus Horde",
        Zone::Battlefield,
    );
    draw_batch(&mut state, PlayerId::P0, 4);
    drain(&mut surface(), &mut state);
    let copies = hordes(&state, PlayerId::P0);
    assert_eq!(copies.len(), 2);
    let copied = state
        .objects
        .get(*copies.iter().find(|&&id| id != source).unwrap());
    assert!(copied.v4.is_token);
    assert!(copied.summoning_sick);
    assert_eq!(copied.name, "Homunculus Horde");
    assert_eq!(trigger::triggers_for(copied.card_def).len(), 1);
    assert_eq!(copied.v4.entered_battlefield_turn, Some(state.turn));
}

#[test]
fn two_hordes_each_create_a_copy_from_the_same_second_draw() {
    let mut state = ready();
    for _ in 0..2 {
        put(
            &mut state,
            PlayerId::P0,
            "Homunculus Horde",
            Zone::Battlefield,
        );
    }
    draw_batch(&mut state, PlayerId::P0, 2);
    assert_eq!(state.engine.pending_triggers.len(), 2);
    drain(&mut surface(), &mut state);
    assert_eq!(hordes(&state, PlayerId::P0).len(), 4);
}

#[test]
fn original_and_copy_can_both_trigger_on_the_opponents_turn() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Homunculus Horde",
        Zone::Battlefield,
    );
    draw_batch(&mut state, PlayerId::P0, 2);
    drain(&mut surface(), &mut state);
    state.step = Step::Cleanup;
    let mut surf = surface();
    next(&mut surf, &mut state);
    assert_eq!(state.active_player, PlayerId::P1);
    assert_eq!(state.players[0].draws_this_turn, 0);
    draw_batch(&mut state, PlayerId::P0, 2);
    assert_eq!(state.engine.pending_triggers.len(), 2);
    drain(&mut surf, &mut state);
    assert_eq!(hordes(&state, PlayerId::P0).len(), 4);
}

#[test]
fn counters_damage_and_tapped_status_are_not_copied() {
    let mut state = ready();
    let source = put(
        &mut state,
        PlayerId::P0,
        "Homunculus Horde",
        Zone::Battlefield,
    );
    state.objects.get_mut(source).counters.plus1_plus1 = 3;
    state.objects.get_mut(source).damage = 1;
    state.objects.get_mut(source).tapped = true;
    draw_batch(&mut state, PlayerId::P0, 2);
    drain(&mut surface(), &mut state);
    let copies = hordes(&state, PlayerId::P0);
    assert_eq!(copies.len(), 2);
    let copied = state
        .objects
        .get(*copies.iter().find(|&&id| id != source).unwrap());
    assert_eq!(copied.counters.plus1_plus1, 0);
    assert_eq!(copied.damage, 0);
    assert!(!copied.tapped);
    assert!(copied.attachments.is_empty());
    assert_eq!(state.objects.get(source).counters.plus1_plus1, 3);
}

#[test]
fn a_queued_trigger_creates_a_copy_after_its_source_leaves() {
    let mut state = ready();
    let source = put(
        &mut state,
        PlayerId::P0,
        "Homunculus Horde",
        Zone::Battlefield,
    );
    draw_batch(&mut state, PlayerId::P0, 2);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Graveyard),
    );
    collect(&mut state);
    drain(&mut surface(), &mut state);
    assert_eq!(hordes(&state, PlayerId::P0).len(), 1);
    assert!(
        state
            .objects
            .get(hordes(&state, PlayerId::P0)[0])
            .v4
            .is_token
    );
    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
}

#[test]
fn copied_tokens_cease_to_exist_in_hand_or_graveyard() {
    for destination in [Zone::Hand, Zone::Graveyard] {
        let mut state = ready();
        let source = put(
            &mut state,
            PlayerId::P0,
            "Homunculus Horde",
            Zone::Battlefield,
        );
        draw_batch(&mut state, PlayerId::P0, 2);
        drain(&mut surface(), &mut state);
        let copied = *hordes(&state, PlayerId::P0)
            .iter()
            .find(|&&id| id != source)
            .unwrap();
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(copied, destination));
        collect(&mut state);
        assert!(!state.players[0].hand.contains(&copied));
        assert!(!state.players[0].graveyard.contains(&copied));
        assert_eq!(hordes(&state, PlayerId::P0), vec![source]);
    }
}

#[test]
fn restoring_ordered_horde_and_mystic_triggers_preserves_the_next_state() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Homunculus Horde",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Mischievous Mystic",
        Zone::Battlefield,
    );
    draw_batch(&mut state, PlayerId::P0, 2);
    assert_eq!(state.engine.pending_triggers.len(), 2);
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    drain(&mut surface(), &mut state);
    drain(&mut surface(), &mut restored);
    assert_eq!(state.state_hash(), restored.state_hash());
    assert_eq!(hordes(&state, PlayerId::P0).len(), 2);
    assert!(state.players[0]
        .battlefield
        .iter()
        .any(|&id| state.objects.get(id).card_def == 222));
}
