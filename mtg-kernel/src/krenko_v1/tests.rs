use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};

#[test]
fn registered_permanent_count_is_shared_full_width_and_not_creature_only_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        let goblin = put(&mut state, actor, "Goblin Token", Zone::Battlefield);
        put(
            &mut state,
            actor.opponent(),
            "Goblin Token",
            Zone::Battlefield,
        );
        let land = put(&mut state, actor, "Mountain", Zone::Battlefield);
        // Physical subtype boundary, not a claim that a printed producer grants it.
        state
            .objects
            .get_mut(land)
            .v4
            .effective_subtype_ids
            .push(Subtype::Goblin.stable_id());
        assert!(!crate::engine::object_has_type(
            &state,
            land,
            crate::card_def::CardType::Creature
        ));
        assert_eq!(
            members(&state, actor)
                .iter()
                .map(|m| m.object)
                .collect::<Vec<_>>(),
            vec![goblin, land]
        );
        assert_eq!(
            crate::engine::evaluate_dynamic_value(&state, COUNT, actor),
            2
        );
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(goblin, Zone::Hand),
        );
        assert_eq!(members(&state, actor).len(), 1);
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(members(&state, actor), members(&restored, actor));
    }
}

#[test]
fn registered_token_batch_has_one_complete_entry_roster_and_original_controller_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        let token = token_definition().unwrap();
        let before = state.engine.event_history.len();
        crate::event::batch_without_linked_aura_v1(
            &mut state,
            (0..3)
                .map(|_| ProposedEvent::create_token(token, actor))
                .collect(),
        );
        let created = state.engine.event_history[before..]
            .iter()
            .filter_map(|e| match e {
                CommittedEvent::CreateToken {
                    object,
                    token_def,
                    controller,
                } => {
                    assert_eq!((*token_def, *controller), (token, actor));
                    Some(*object)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(created.len(), 3);
        assert_eq!(members(&state, actor).len(), 3);
        for object in created {
            let live = state.objects.get(object);
            assert_eq!(
                (live.owner, live.controller, live.zone),
                (actor, actor, Zone::Battlefield)
            );
            assert!(live.v4.is_token && live.summoning_sick);
        }
        validate_state(&state).unwrap();
    }
}

#[test]
fn unrelated_registered_activation_and_hooks_emit_no_krenko_bytes_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        let source = put(&mut state, actor, "Quirion Ranger", Zone::Battlefield);
        let target = source;
        put(&mut state, actor, "Forest", Zone::Battlefield);
        let before = serde_json::to_vec(&state).unwrap();
        validate_state(&state).unwrap();
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert!(!requires_successor(&state));
        assert!(matches!(crate::engine::advance_until_decision(&mut state),
            crate::engine::Decision::CastSpellOrPass { activatable_abilities, .. }
                if activatable_abilities.contains(&(source, 0))));
        assert!(!tracked(
            &state,
            &crate::state::StackItem {
                source: target,
                controller: actor,
                targets: vec![],
                discarded: vec![],
                inline_effect: None,
                mode_chosen: 0,
                kind: StackItemKind::Spell,
                is_copy: false,
                is_flashback: false,
                madness_offer: false,
                kicked: false,
                v4: Default::default(),
            }
        ));
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod future {
    use super::*;
    use crate::engine::{self, Action, Decision};

    fn paid_primitive(actor: PlayerId) -> (GameState, StackItem) {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        // Genuine registered activation on a physical permanent; full casting
        // and natural untap traversal live in the external printed fixtures.
        let source = put(&mut state, actor, "Krenko, Mob Boss", Zone::Battlefield);
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        let item = state.stack.last().unwrap().clone();
        validate_state(&state).unwrap();
        (state, item)
    }
    fn refuses(mut state: GameState) {
        let bytes = serde_json::to_vec(&state).unwrap();
        assert!(validate_state(&state).is_err());
        assert!(engine::step(&mut state, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        let restored: GameState = serde_json::from_slice(&bytes).unwrap();
        assert!(validate_state(&restored).is_err());
    }

    #[test]
    #[ignore = "Requires genuine parent Krenko registration; no source admission bypass"]
    fn real_paid_private_counter_requires_original_removal_and_cannot_replay_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, item) = paid_primitive(actor);
            let mut missing = state.clone();
            missing.stack.clear();
            refuses(missing);
            let mut closed_only = state.clone();
            closed_only.stack.clear();
            let index = closed_only.engine.event_history.len() as u64;
            closed_only
                .engine
                .event_history
                .push(CommittedEvent::KrenkoCompletedV1 {
                    completed: Box::new(CompletedV1 {
                        index,
                        removed: index,
                        sample: None,
                        token_history: vec![],
                    }),
                });
            refuses(closed_only);
            let mut countered = state;
            assert_eq!(
                engine::counter_stack_item_by_id(&mut countered, item.v4.stack_item_id).unwrap(),
                Some(item.clone())
            );
            validate_state(&countered).unwrap();
            assert!(
                engine::counter_stack_item_by_id(&mut countered, item.v4.stack_item_id)
                    .unwrap()
                    .is_none()
            );
            let mut replay = countered.clone();
            replay.stack.push(item);
            refuses(replay);
            let removal = countered
                .engine
                .event_history
                .iter()
                .position(|e| matches!(e, CommittedEvent::KrenkoRemovedV1 { .. }))
                .unwrap();
            countered.engine.event_history[removal] = CommittedEvent::Tap {
                object: countered.objects.iter().next().unwrap().0,
            };
            refuses(countered);
        }
    }
}
