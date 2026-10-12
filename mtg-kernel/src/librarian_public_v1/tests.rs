use super::*;
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::engine::{self, Action, Decision};
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::ObjectStateV4;

pub(crate) fn ready(actor: PlayerId) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    // Keep actual draw and hidden-library substitution fixtures on live cards.
    for player in [PlayerId::P0, PlayerId::P1] {
        for _ in 0..10 {
            put(&mut state, player, "Plains", Zone::Library);
        }
    }
    state
}
fn observe(state: &GameState, actor: PlayerId) -> LibrarianObservationV1 {
    observe_v1(
        state,
        &PolicySurfaceV5::new_for_session(),
        actor,
        0,
        0,
        0,
        1,
    )
    .unwrap()
}
pub(crate) fn ward_state(actor: PlayerId) -> GameState {
    let mut state = ready(actor);
    let target = put(
        &mut state,
        actor.opponent(),
        "Tolarian Terror",
        Zone::Battlefield,
    );
    let spells =
        ["Lightning Bolt", "Lightning Bolt"].map(|name| put(&mut state, actor, name, Zone::Hand));
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 8;
    for spell in spells {
        crate::engine::step(&mut state, crate::engine::Action::CastSpell(spell)).unwrap();
        crate::engine::step(
            &mut state,
            crate::engine::Action::ChooseTarget(crate::state::Target::Object(target)),
        )
        .unwrap();
        assert!(matches!(
            crate::engine::advance_until_decision(&mut state),
            crate::engine::Decision::CastSpellOrPass { .. }
        ));
    }
    crate::policy_observation_v6::tests::reach_ward_payment(&mut state);
    state
}
#[test]
fn registered_absence_preserves_actual_typed_predecessor_fields_and_private_hidden_substitutions() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        let base = observe(&state, actor);
        let old = crate::policy_observation_v9::observe_policy_v9(
            &state,
            &PolicySurfaceV5::new_for_session(),
            actor,
            0,
            0,
            0,
            1,
        )
        .unwrap();
        assert_eq!(base.projection, old.projection);
        assert_eq!(base.own_hand, old.own_hand);
        assert_eq!(base.known_face_down_cards, old.known_face_down_cards);
        assert_eq!(base.extensions, old.extensions.predecessor);
        assert!(base.activations.is_empty());
        base.validate_artifact().unwrap();
        for field in 0..4 {
            let mut forged = base.clone();
            match field {
                0 => forged.kernel_version.push('x'),
                1 => forged.surface_version ^= 1,
                2 => forged.policy_surface_version ^= 1,
                _ => forged.substep_count = 0,
            }
            forged.projection_sha256 = commitment(&forged).unwrap();
            assert!(forged.validate_artifact().is_err());
        }
        let hidden = state.players[actor.opponent().index()].library[0];
        let mountain = crate::card_def::card_id_by_name("Mountain").unwrap();
        let changed = state.objects.get_mut(hidden);
        changed.card_def = mountain;
        changed.name = "Mountain".into();
        changed.v4 = ObjectStateV4::from_card_def(mountain);
        assert_eq!(base, observe(&state, actor));
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn finish(state: &mut GameState) {
    for _ in 0..30 {
        let decision = engine::advance_until_decision(state);
        assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
        if state.stack.is_empty()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_activation.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("actual source program must complete");
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn paid(actor: PlayerId) -> (GameState, crate::ids::ObjectId) {
    let mut state = ready(actor);
    let source = put(&mut state, actor, "Mild-Mannered Librarian", Zone::Hand);
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::G.pool_index()] = 1;
    assert!(
        matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    finish(&mut state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    // A real instant response keeps the paid activation visible to the actor
    // instead of letting the session automatically skip pure Pass decisions.
    put(&mut state, actor, "Lightning Bolt", Zone::Hand);
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::G.pool_index()] = 4;
    assert!(
        matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, activatable_abilities, .. }
        if player == actor && activatable_abilities.contains(&(source, 0)))
    );
    engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.engine.pending_activation.is_none());
    assert_eq!(
        observe(&state, actor).activations[0].lifecycle,
        LifecycleV1::Paid
    );
    (state, source)
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn future_actual_paid_resolved_departed_reentered_and_current_control_rows_use_original_controller()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..4 {
            let (mut state, source) = paid(actor);
            let hand = state.players[actor.index()].hand.len();
            if mode == 1 {
                // Same explicitly labeled physical control substrate as the
                // printed source fixtures; no printed control producer claimed.
                state.players[actor.index()]
                    .battlefield
                    .retain(|id| *id != source);
                state.players[actor.opponent().index()]
                    .battlefield
                    .push(source);
                state.objects.get_mut(source).controller = actor.opponent();
            } else if mode >= 2 {
                crate::event::propose_and_commit(
                    &mut state,
                    crate::event::ProposedEvent::zone_change(source, Zone::Graveyard),
                );
                if mode == 3 {
                    crate::event::propose_and_commit(
                        &mut state,
                        crate::event::ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                }
            }
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            finish(&mut state);
            finish(&mut restored);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            assert_eq!(state.players[actor.index()].hand.len(), hand + 1);
            for viewer in [actor, actor.opponent()] {
                let public = observe(&state, viewer);
                public.validate_artifact().unwrap();
                let row = &public.activations[0];
                assert_eq!(row.captured_controller, actor.into());
                assert_eq!(row.lifecycle, LifecycleV1::Resolved);
                assert!(row.actual_draw_attempt && row.once_consumed_on_payment);
                assert_eq!(row.original_source_received_upgrade_and_counters, mode < 2);
                assert_eq!(row.current_source.is_some(), mode < 2);
                assert_eq!(row.current_plus_one_counters, (mode < 2).then_some(2));
                if mode == 1 {
                    assert_eq!(
                        row.current_source.as_ref().unwrap().controller,
                        actor.opponent().into()
                    );
                }
            }
        }
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn future_actual_counter_and_erased_original_operation_refuse_projection_and_actions_after_restore()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = paid(actor);
        let id = state
            .stack
            .iter()
            .find(|item| item.source == source)
            .unwrap()
            .v4
            .stack_item_id;
        engine::counter_stack_item_by_id(&mut state, id).unwrap();
        let public = observe(&state, actor);
        assert_eq!(public.activations[0].lifecycle, LifecycleV1::Countered);
        assert!(!public.activations[0].actual_draw_attempt);
        state.engine.event_history.retain(|event| {
            !matches!(
                event,
                CommittedEvent::LibrarianActivationCounterRemovalV1 { .. }
            )
        });
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        assert!(observe_v1(
            &restored,
            &PolicySurfaceV5::new_for_session(),
            actor,
            0,
            0,
            0,
            1
        )
        .is_err());
        assert!(engine::step(&mut restored, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
    }
}
