use super::*;
use crate::{
    card_def::Keywords,
    effect::thousand_year_storm_v1::JournalV1,
    policy_observation_v6::tests::advance_to_next_main_for,
    policy_surface_v5::{PolicyActionV5, PolicyDecisionV5, PolicySurfaceV5},
    rl::{self, CommonProjectionScopeV42, PolicyObservationStorageV6},
    rl_session,
    state::StackTargetContractV4,
    surface_v2::{PriorityModeV1, SuppressionAuditMode, SurfaceAction},
};

fn current_surface(state: &GameState) -> (PolicySurfaceV5, PolicyDecisionV5) {
    let mut surface = PolicySurfaceV5::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Off,
    );
    surface.enable_hunter_order_v45();
    surface.set_limited_appendix_v1(LimitedAppendixV1::ThousandYearStorm);
    let mut probe = state.clone();
    let origin = surface.next_decision(&mut probe).unwrap();
    assert_eq!(&probe, state);
    (surface, origin)
}

fn common(
    state: &GameState,
    surface: &PolicySurfaceV5,
    viewer: PlayerId,
) -> PolicyObservationStorageV6 {
    rl::common_family_observation_storage_scoped(
        (CommonProjectionScopeV42::KaitoV75, state),
        surface,
        viewer,
        0,
        0,
        (0, 1),
    )
    .unwrap()
}

fn assert_paired_views(left: &GameState, right: &GameState) {
    let (left_surface, left_origin) = current_surface(left);
    let (right_surface, right_origin) = current_surface(right);
    assert_eq!(left_origin, right_origin);
    for viewer in [PlayerId::P0, PlayerId::P1] {
        assert_eq!(
            public::capture(left, viewer).unwrap(),
            public::capture(right, viewer).unwrap()
        );
        let a = common(left, &left_surface, viewer);
        let b = common(right, &right_surface, viewer);
        assert_eq!(a.projection, b.projection);
        assert_eq!(a.extensions, b.extensions);
        assert_eq!(a.own_hand, b.own_hand);
        assert_eq!(a.known_library_cards, b.known_library_cards);
        assert_eq!(a.known_hand_cards, b.known_hand_cards);
        assert_eq!(a.known_face_down_cards, b.known_face_down_cards);
    }
}

fn paid_stale_creature_target(
    actor: PlayerId,
) -> (GameState, ObjectId, ObjectId, StackTargetContractV4) {
    let mut state = native::ready(actor, "Island");
    state.thousand_year_storm_consumer_v1 = true;
    advance_to_next_main_for(&mut state, actor.opponent());
    let creature = native::cast(&mut state, actor.opponent(), "Llanowar Elves", &[], 1);
    native::finish(&mut state);
    advance_to_next_main_for(&mut state, actor);
    native::bolt(&mut state, actor);
    native::paid_storm(&mut state, actor);
    let bolt = native::cast(
        &mut state,
        actor,
        "Lightning Bolt",
        &[Target::Object(creature)],
        1,
    );
    let original = state.stack.iter().find(|row| row.source == bolt).unwrap();
    let original_target = original.v4.target_contracts[0];
    // Hexproof removes the Elf from the native replacement menu while
    // retaining the original target is still legal under CR 115.7d.
    native::cast(
        &mut state,
        actor.opponent(),
        "Snakeskin Veil",
        &[Target::Object(creature)],
        1,
    );
    native::waiting(&mut state);
    assert!(engine::has_effective_keyword(
        &state,
        creature,
        Keywords::HEXPROOF
    ));
    let before = native::next(&mut state);
    assert!(matches!(&before,
        engine::Decision::ChooseEffectTargets { player, legal_targets, can_finish: true, .. }
            if *player == actor && !legal_targets.contains(&Target::Object(creature))));

    // Exercise the real zone commit after the copy menu has already opened.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(creature, Zone::Library),
    );
    assert_eq!(state.objects.get(creature).zone, Zone::Library);
    assert_eq!(native::next(&mut state), before);
    for viewer in [PlayerId::P0, PlayerId::P1] {
        assert!(state
            .known_library_cards(viewer, actor.opponent())
            .iter()
            .all(|entry| entry.object != creature));
    }
    (state, bolt, creature, original_target)
}

fn assert_retained_face_required(state: &GameState, copy: crate::ids::StackItemId) {
    let index = state
        .engine
        .event_history
        .iter()
        .position(|event| {
            matches!(event,
        CommittedEvent::ThousandYearStormV1 { journal: JournalV1::TargetFaces { item, .. } }
            if *item == copy)
        })
        .unwrap();
    let original_event = state.engine.event_history.get(index).unwrap();
    let mut altered = serde_json::to_value(original_event).unwrap();
    let face = &mut altered["ThousandYearStormV1"]["journal"]["TargetFaces"]["faces"][0]
        ["public_card_def"];
    assert_eq!(face.as_u64(), Some(u64::from(id("Llanowar Elves"))));
    *face = serde_json::Value::from(id("Mountain"));

    let restored =
        || -> GameState { serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap() };
    let mut relabeled = restored();
    relabeled.engine.event_history[index] = serde_json::from_value(altered).unwrap();
    assert_eq!(
        public::validate_native(&relabeled).unwrap_err(),
        "Storm retained target face changed printing"
    );

    let mut missing = restored();
    missing.engine.event_history.remove(index);
    assert_eq!(
        public::validate_native(&missing).unwrap_err(),
        "Storm native target faces absent"
    );
}

#[test]
fn stale_creature_copy_target_ignores_later_hidden_printing_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, bolt, creature, original_target) = paid_stale_creature_target(actor);
        // Resample only the new hidden incarnation, preserving the original
        // cast, target contract, actual zone transition, and decision.
        let other = sampled_printings(&state, &[(creature, id("Mountain"))]);
        assert_ne!(state.state_hash(), other.state_hash());
        let mut states = [state, other];
        assert_paired_views(&states[0], &states[1]);

        let mut owners = states
            .each_ref()
            .map(|state| KaitoSession::from_session(raw_owner(state)).unwrap());
        let decisions = owners.each_ref().map(decision);
        assert_eq!(decisions[0].packet(), decisions[1].packet());
        assert_eq!(
            decisions[0].fixture_backend_v75().observation(),
            decisions[1].fixture_backend_v75().observation()
        );

        let StackTargetContractV4::Object {
            zone_change_count,
            card_def,
            ..
        } = original_target
        else {
            panic!("expected actual creature target")
        };
        let mut copy_sources = Vec::new();
        let mut paired_native = None;
        let mut paired_mapping = None;
        for ((state, owner), current) in states.iter_mut().zip(&mut owners).zip(&decisions) {
            let backend = current.fixture_backend_v75();
            let facts = backend.observation().thousand_year_storm.as_ref().unwrap();
            let choice = facts
                .triggers
                .iter()
                .find_map(|row| row.choice.as_ref())
                .unwrap();
            assert!(choice.can_keep_original);
            assert!(matches!(&choice.original_target,
                public::TargetV1::Object { original, current_original: None, .. }
                    if original.object == creature.0 && original.zone == Zone::Battlefield
                        && original.generation == zone_change_count));
            let (surface, origin) = current_surface(state);
            let native = rl_session::kaito_candidates_v75(&origin, state, &surface).unwrap();
            let mapping = backend
                .candidates_v75()
                .candidates
                .iter()
                .map(|row| backend.fixture_original_index_v75(row.ordinal).unwrap())
                .collect::<Vec<_>>();
            if let Some(expected) = &paired_native {
                assert_eq!(&native, expected);
                assert_eq!(Some(&mapping), paired_mapping.as_ref());
            } else {
                paired_native = Some(native.clone());
                paired_mapping = Some(mapping);
            }
            let public_bolt = &backend
                .observation()
                .common
                .common
                .projection
                .surface
                .stack
                .iter()
                .find(|row| row.source.arena_id == bolt.0)
                .unwrap();
            assert!(
                matches!(public_bolt.targets.as_slice(), [TargetRefV1::Object { object }]
                if object.arena_id == creature.0 && object.zone == Zone::Battlefield
                    && object.zone_change_count == zone_change_count && object.card_db_id == card_def)
            );
            let keep = backend
                .candidates_v75()
                .candidates
                .iter()
                .position(|row| {
                    row.thousand_year_storm_action
                        == Some(ActionV1::KeepOriginalTarget { copy: 1, slot: 0 })
                })
                .unwrap() as u32;
            let original_index = backend.fixture_original_index_v75(keep).unwrap();
            assert_eq!(
                native[original_index as usize].policy_action,
                PolicyActionV5::Surface(SurfaceAction::Action(
                    engine::Action::FinishEffectSelection
                ))
            );
            let mut raw = raw_owner(state);
            let expected = backend.expected();
            let applied = raw
                .step(expected.episode_id, expected.step, original_index)
                .unwrap();
            assert_eq!(owner.apply(current, keep).unwrap(), applied);
            assert_eq!(owner.fixture_state(), raw.kaito_fixture_state());
            *state = owner.fixture_state().clone();

            let copy = state
                .engine
                .event_history
                .iter()
                .find_map(|event| match event {
                    CommittedEvent::ThousandYearStormV1 {
                        journal: JournalV1::Copy { item, .. },
                    } => Some(item.as_ref()),
                    _ => None,
                })
                .unwrap();
            assert!(copy.is_copy);
            assert!(state.stack.iter().any(|row| row == copy));
            assert_eq!(copy.targets, [Target::Object(creature)]);
            assert_eq!(copy.v4.target_contracts, [original_target]);
            copy_sources.push(copy.source);
            assert_retained_face_required(state, copy.v4.stack_item_id);
        }
        assert_eq!(copy_sources[0], copy_sources[1]);
        assert_paired_views(&states[0], &states[1]);

        for state in &mut states {
            let history_start = state.engine.event_history.len();
            native::finish(state);
            assert!(state.stack.is_empty());
            assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(creature).zone, Zone::Library);
            assert_eq!(state.objects.get(creature).damage, 0);
            assert!(!state.engine.event_history[history_start..].iter().any(|event| matches!(event,
                CommittedEvent::Damage { source, .. } if *source == bolt || *source == copy_sources[0])));
        }
        assert_paired_views(&states[0], &states[1]);
    }
}
