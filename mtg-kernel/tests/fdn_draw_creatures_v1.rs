//! Second-draw timing and the resumable tap-to-loot ability.
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
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
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
            assert!(surface.suppressions().is_empty());
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

fn faeries(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    let token = card_id_by_name("Faerie Token").unwrap();
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects.get(id).card_def == token)
        .collect()
}

#[test]
fn printed_cards_cast_for_two_mana_and_append_with_exact_stats_and_keywords() {
    for (id, name, stats, keywords) in [
        (
            190,
            "Strix Lookout",
            (1, 2),
            Keywords::FLYING.0 | Keywords::VIGILANCE.0,
        ),
        (191, "Mischievous Mystic", (2, 1), Keywords::FLYING.0),
    ] {
        assert_eq!(card_id_by_name(name), Some(id));
        preflight_fully_supported_deck(&[id]).unwrap();
        let def = &CARD_DEFS[id as usize];
        assert_eq!((def.power, def.toughness), (Some(stats.0), Some(stats.1)));
        assert_eq!(def.keywords.0, keywords);
        let mut state = ready();
        let mut surf = surface();
        let object = put(&mut state, PlayerId::P0, name, Zone::Hand);
        state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;
        assert!(
            matches!(next(&mut surf, &mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&object))
        );
        apply(&mut surf, &mut state, Action::CastSpell(object));
        drain(&mut surf, &mut state);
        assert_eq!(state.players[0].mana_pool, [0; 6]);
        assert_eq!(state.objects.get(object).zone, Zone::Battlefield);
    }
}

#[test]
fn faerie_token_is_blue_flying_one_one_and_cannot_be_a_mainboard_card() {
    assert_eq!(card_id_by_name("Faerie Token"), Some(192));
    let token = &CARD_DEFS[192];
    assert_eq!((token.power, token.toughness), (Some(1), Some(1)));
    assert_eq!(token.colors, &[ManaColor::U]);
    assert!(token.subtypes.contains(&Subtype::Faerie));
    assert!(token.keywords.has(Keywords::FLYING));
    assert!(preflight_fully_supported_deck(&[192]).is_err());
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Mischievous Mystic",
        Zone::Battlefield,
    );
    draw_batch(&mut state, PlayerId::P0, 2);
    drain(&mut surface(), &mut state);
    let id = faeries(&state, PlayerId::P0)[0];
    assert!(state.objects.get(id).v4.is_token);
}

#[test]
fn only_the_second_draw_triggers_mystic() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Mischievous Mystic",
        Zone::Battlefield,
    );
    for n in 1..=4 {
        draw_batch(&mut state, PlayerId::P0, 1);
        assert_eq!(state.engine.pending_triggers.len(), usize::from(n == 2));
        drain(&mut surface(), &mut state);
        assert_eq!(faeries(&state, PlayerId::P0).len(), usize::from(n >= 2));
    }
}

#[test]
fn batch_of_four_draws_crossing_second_draw_triggers_exactly_once() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Mischievous Mystic",
        Zone::Battlefield,
    );
    draw_batch(&mut state, PlayerId::P0, 4);
    assert_eq!(state.players[0].draws_this_turn, 4);
    assert_eq!(state.engine.pending_triggers.len(), 1);
    drain(&mut surface(), &mut state);
    assert_eq!(faeries(&state, PlayerId::P0).len(), 1);
}

#[test]
fn a_later_draw_batch_uses_each_event_ordinal() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Mischievous Mystic",
        Zone::Battlefield,
    );
    draw_batch(&mut state, PlayerId::P0, 1);
    draw_batch(&mut state, PlayerId::P0, 3);
    assert_eq!(state.engine.pending_triggers.len(), 1);
    drain(&mut surface(), &mut state);
    assert_eq!(faeries(&state, PlayerId::P0).len(), 1);
}

#[test]
fn opponent_draws_do_not_count_toward_the_controllers_second_draw() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Mischievous Mystic",
        Zone::Battlefield,
    );
    draw_batch(&mut state, PlayerId::P1, 2);
    assert!(state.engine.pending_triggers.is_empty());
    draw_batch(&mut state, PlayerId::P0, 2);
    assert_eq!(state.engine.pending_triggers.len(), 1);
    drain(&mut surface(), &mut state);
    assert_eq!(faeries(&state, PlayerId::P0).len(), 1);
    assert!(faeries(&state, PlayerId::P1).is_empty());
}

#[test]
fn mystic_entering_after_the_first_draw_sees_the_second_draw() {
    let mut state = ready();
    draw_batch(&mut state, PlayerId::P0, 1);
    put(
        &mut state,
        PlayerId::P0,
        "Mischievous Mystic",
        Zone::Battlefield,
    );
    draw_batch(&mut state, PlayerId::P0, 1);
    assert_eq!(state.engine.pending_triggers.len(), 1);
    drain(&mut surface(), &mut state);
    assert_eq!(faeries(&state, PlayerId::P0).len(), 1);
}

#[test]
fn mystic_entering_after_the_second_draw_does_not_trigger_for_third_draw() {
    let mut state = ready();
    draw_batch(&mut state, PlayerId::P0, 2);
    put(
        &mut state,
        PlayerId::P0,
        "Mischievous Mystic",
        Zone::Battlefield,
    );
    draw_batch(&mut state, PlayerId::P0, 1);
    assert!(state.engine.pending_triggers.is_empty());
}

#[test]
fn actual_untap_resets_draw_counts_and_opponents_turn_can_trigger_again() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Mischievous Mystic",
        Zone::Battlefield,
    );
    draw_batch(&mut state, PlayerId::P0, 2);
    drain(&mut surface(), &mut state);
    state.step = Step::Cleanup;
    let mut surf = surface();
    assert!(matches!(
        next(&mut surf, &mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.active_player, PlayerId::P1);
    assert_eq!(state.players[0].draws_this_turn, 0);
    draw_batch(&mut state, PlayerId::P0, 2);
    drain(&mut surf, &mut state);
    assert_eq!(faeries(&state, PlayerId::P0).len(), 2);
}

#[test]
fn two_mystics_trigger_independently_and_queued_trigger_survives_departure() {
    let mut state = ready();
    let first = put(
        &mut state,
        PlayerId::P0,
        "Mischievous Mystic",
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
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(first, Zone::Graveyard),
    );
    collect(&mut state);
    drain(&mut surface(), &mut state);
    assert_eq!(faeries(&state, PlayerId::P0).len(), 2);
}

#[test]
fn created_faerie_fires_existing_creature_entry_counter_trigger() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Mischievous Mystic",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Good-Fortune Unicorn",
        Zone::Battlefield,
    );
    draw_batch(&mut state, PlayerId::P0, 2);
    drain(&mut surface(), &mut state);
    assert_eq!(
        state
            .objects
            .get(faeries(&state, PlayerId::P0)[0])
            .counters
            .plus1_plus1,
        1
    );
}

#[test]
fn tapped_sick_or_unaffordable_lookout_cannot_activate_or_change_state() {
    for (tapped, sick, mana) in [
        (true, false, [0, 2, 0, 0, 0, 0]),
        (false, true, [0, 2, 0, 0, 0, 0]),
        (false, false, [0, 1, 0, 0, 0, 0]),
        (false, false, [0, 0, 0, 0, 0, 2]),
    ] {
        let mut state = ready();
        let lookout = put(&mut state, PlayerId::P0, "Strix Lookout", Zone::Battlefield);
        state.objects.get_mut(lookout).tapped = tapped;
        state.objects.get_mut(lookout).summoning_sick = sick;
        state.players[0].mana_pool = mana;
        let before = state.state_hash();
        assert!(engine::step(&mut state, Action::ActivateAbility(lookout, 0)).is_err());
        assert_eq!(state.state_hash(), before);
    }
}

#[test]
fn vigilant_attack_leaves_lookout_available_to_loot_in_actual_second_main() {
    let mut state = ready();
    let lookout = put(&mut state, PlayerId::P0, "Strix Lookout", Zone::Battlefield);
    put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
    state.step = Step::DeclareAttackers;
    engine::step(&mut state, Action::DeclareAttackers(vec![lookout])).unwrap();
    assert!(!state.objects.get(lookout).tapped);
    let mut surf = surface();
    for _ in 0..100 {
        let decision = next(&mut surf, &mut state);
        if state.step == Step::Main2 && matches!(decision, Decision::CastSpellOrPass { .. }) {
            state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;
            apply(&mut surf, &mut state, Action::ActivateAbility(lookout, 0));
            drain(&mut surf, &mut state);
            assert!(state.objects.get(lookout).tapped);
            assert_eq!(state.players[0].hand.len(), 1);
            assert_eq!(state.players[1].life, 19);
            return;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
            other => panic!("unexpected combat decision: {other:?}"),
        };
        apply(&mut surf, &mut state, action);
    }
    panic!("second main was not reached");
}

#[test]
fn lookout_pays_exact_mana_and_tap_draws_then_restores_discard_choice() {
    let mut state = ready();
    let mut surf = surface();
    let lookout = put(&mut state, PlayerId::P0, "Strix Lookout", Zone::Battlefield);
    let retained = put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;
    assert!(matches!(
        next(&mut surf, &mut state),
        Decision::CastSpellOrPass { .. }
    ));
    apply(&mut surf, &mut state, Action::ActivateAbility(lookout, 0));
    assert!(matches!(
        next(&mut surf, &mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.objects.get(lookout).tapped);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    for _ in 0..30 {
        let decision = next(&mut surf, &mut state);
        if let Decision::Discard {
            count, ref choices, ..
        } = decision
        {
            assert_eq!(count, 1);
            assert_eq!(choices.len(), 2);
            assert_eq!(state.players[0].draws_this_turn, 1);
            let drawn = *choices.iter().find(|&&id| id != retained).unwrap();
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            let mut restored_surf = surf.clone();
            assert_eq!(state.state_hash(), restored.state_hash());
            assert_eq!(
                format!("{decision:?}"),
                format!("{:?}", next(&mut restored_surf, &mut restored))
            );
            for (game, surface) in [(&mut state, &mut surf), (&mut restored, &mut restored_surf)] {
                apply(surface, game, Action::Discard(vec![drawn]));
                drain(surface, game);
                assert_eq!(game.objects.get(drawn).zone, Zone::Graveyard);
                assert!(game.players[0].hand.contains(&retained));
                assert_eq!(game.players[0].draws_this_turn, 1);
            }
            assert_eq!(state.state_hash(), restored.state_hash());
            return;
        }
        assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
        apply(&mut surf, &mut state, Action::Pass);
    }
    panic!("discard choice not reached");
}

#[test]
fn lookout_second_draw_waits_for_discard_before_creating_the_faerie() {
    let mut state = ready();
    let mut surf = surface();
    let lookout = put(&mut state, PlayerId::P0, "Strix Lookout", Zone::Battlefield);
    put(
        &mut state,
        PlayerId::P0,
        "Mischievous Mystic",
        Zone::Battlefield,
    );
    draw_batch(&mut state, PlayerId::P0, 1);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;
    next(&mut surf, &mut state);
    apply(&mut surf, &mut state, Action::ActivateAbility(lookout, 0));
    for _ in 0..30 {
        let decision = next(&mut surf, &mut state);
        if let Decision::Discard { choices, .. } = decision {
            assert_eq!(state.players[0].draws_this_turn, 2);
            assert_eq!(state.engine.pending_triggers.len(), 1);
            assert!(faeries(&state, PlayerId::P0).is_empty());
            apply(&mut surf, &mut state, Action::Discard(vec![choices[0]]));
            drain(&mut surf, &mut state);
            assert_eq!(faeries(&state, PlayerId::P0).len(), 1);
            return;
        }
        assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
        apply(&mut surf, &mut state, Action::Pass);
    }
    panic!("discard choice not reached");
}
