use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state_for};
use crate::state::{GameState, Zone};
pub(crate) fn ordinary(actor: PlayerId) -> GameState {
    let mut state = ready_state_for(actor);
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
    state
}
pub(crate) fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    match session.current_response() {
        FastActorResponseV1::Decision(expected) => expected,
        other => panic!("{other:?}"),
    }
}
#[test]
fn registered_predecessor_parity_all_parts_raw_order_and_owning_restore_both_seats() {
    if !crate::conqueror_public_v49::meanings().unwrap().is_empty() {
        return;
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = ordinary(actor);
        let mut current = FastActorSessionV1::from_conqueror_v49_fixture_state(state.clone());
        let old = FastActorSessionV1::from_season_v48_fixture_state(state);
        let e = expected(&current);
        let (observation, actions, binding) = current
            .conqueror_current_decision_input_v49(e, actor.into())
            .unwrap();
        let (predecessor, old_actions, _) = old
            .season_current_decision_input_v48(expected(&old), actor.into())
            .unwrap();
        assert_eq!(actions, old_actions);
        assert_eq!(observation.projection, predecessor.projection);
        assert_eq!(observation.families, predecessor.families);
        assert_eq!(observation.season, predecessor.season);
        assert_eq!(observation.divine, predecessor.divine);
        assert_eq!(observation.hunter, predecessor.hunter);
        assert_eq!(observation.eaten, predecessor.eaten);
        assert_eq!(observation.panda, predecessor.panda);
        assert_eq!(observation.ajani, predecessor.ajani);
        assert_eq!(observation.faebloom, predecessor.faebloom);
        assert!(observation.conqueror.occurrences.is_empty());
        observation.validate_artifact().unwrap();
        let saved = current.snapshot_v1();
        for _ in 0..3 {
            current.conqueror_restore_v49(&saved).unwrap();
            let bytes = serde_json::to_vec(&current.state).unwrap();
            assert!(current
                .conqueror_validate_bound_decision_v49(e, actor.into(), &binding)
                .is_err());
            assert_eq!(serde_json::to_vec(&current.state).unwrap(), bytes);
            let live = expected(&current);
            let (_, _, fresh) = current
                .conqueror_current_decision_input_v49(live, actor.into())
                .unwrap();
            current
                .conqueror_validate_bound_decision_v49(live, actor.into(), &fresh)
                .unwrap();
            let mut changed = fresh.clone();
            changed.candidates.reverse();
            assert!(current
                .conqueror_validate_bound_decision_v49(live, actor.into(), &changed)
                .is_err());
            let mut changed = fresh.clone();
            changed.observation.conqueror.card_db_hash ^= 1;
            changed.observation.projection_sha256 =
                crate::conqueror_observation_v49::commitment(&changed.observation).unwrap();
            assert!(current
                .conqueror_validate_bound_decision_v49(live, actor.into(), &changed)
                .is_err());
            assert_eq!(serde_json::to_vec(&current.state).unwrap(), bytes);
        }
        let mut malformed = saved.clone();
        malformed.0.current.as_mut().unwrap().environment_revision += 1;
        let before = serde_json::to_vec(&current.state).unwrap();
        assert!(current.conqueror_restore_v49(&malformed).is_err());
        assert_eq!(serde_json::to_vec(&current.state).unwrap(), before);
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn paid_source_departure_original_ref_genuine_counter_closed_input_and_old_mode_readonly_refusal() {
    use crate::bloodthirsty_conqueror_v49::{self as conqueror, tests::future_printed};
    use crate::event::{self, ProposedEvent};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = future_printed::paid(actor);
        event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 2));
        crate::engine::advance_until_decision(&mut state);
        // Exact physical controller transfer after genuine captured publication.
        // The ability beneficiary remains the original controller.
        state.players[actor.index()]
            .battlefield
            .retain(|id| *id != source);
        state.players[actor.opponent().index()]
            .battlefield
            .push(source);
        state.objects.get_mut(source).controller = actor.opponent();
        let live = FastActorSessionV1::from_conqueror_v49_fixture_state(state.clone());
        let (public, _, _) = live
            .conqueror_current_decision_input_v49(expected(&live), actor.into())
            .unwrap();
        assert_eq!(
            public.conqueror.occurrences[0].original_controller,
            actor.into()
        );
        assert_eq!(
            public.conqueror.occurrences[0]
                .current_original_source
                .as_ref()
                .unwrap()
                .controller,
            actor.opponent().into()
        );
        // Actual committed physical BF departure primitive, not a substituted bounce printing.
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        let session = FastActorSessionV1::from_conqueror_v49_fixture_state(state);
        let e = expected(&session);
        let (public, _, _) = session
            .conqueror_current_decision_input_v49(e, actor.into())
            .unwrap();
        assert_eq!(
            public.conqueror.occurrences[0].current_original_source,
            None
        );
        assert_eq!(
            public.conqueror.occurrences[0].source.zone,
            Zone::Battlefield
        );
        let item = session
            .state
            .stack
            .iter()
            .find(|item| crate::conqueror_public_v49::selected(item))
            .unwrap();
        assert_eq!(
            public.projection.surface.stack.last().unwrap().source,
            public.conqueror.occurrences[0].source
        );
        let mut countered = session.state.clone();
        crate::engine::counter_stack_item_by_id(&mut countered, item.v4.stack_item_id).unwrap();
        conqueror::validate(&countered).unwrap();
        let facts = crate::conqueror_public_v49::capture(&countered).unwrap();
        assert_eq!(
            facts.occurrences[0].phase,
            crate::conqueror_public_v49::PhaseV49::Countered
        );
        assert_eq!(facts.occurrences[0].actual_gain, None);
        let mut old = session.clone();
        old.flat_action_contract_mode = FlatActionContractModeV1::SeasonV48;
        let before = serde_json::to_vec(&old.state).unwrap();
        assert!(old.step(e.episode_id, e.step, 0).is_err());
        assert_eq!(serde_json::to_vec(&old.state).unwrap(), before);
        old.advance_to_decision_or_terminal();
        assert!(old.terminal.is_some());
        assert_eq!(serde_json::to_vec(&old.state).unwrap(), before);
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn eight_genuine_loss_occurrences_stage_full_roster_without_partial_engine_order_after_owning_restore(
) {
    use crate::bloodthirsty_conqueror_v49::tests::future_printed;
    use crate::event::{self, ProposedEvent};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = future_printed::paid(actor);
        for _ in 0..8 {
            event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 1));
        }
        let mut session = FastActorSessionV1::from_conqueror_v49_fixture_state(state);
        for selected in 0..8 {
            let e = expected(&session);
            let (public, actions, binding) = session
                .conqueror_current_decision_input_v49(e, actor.into())
                .unwrap();
            assert_eq!(public.conqueror.trigger_order.len(), 8);
            assert_eq!(actions.len(), 8 - selected);
            let stage = public.trigger_order.unwrap();
            assert_eq!(stage.pending_count, 8);
            assert_eq!(stage.ordered_prefix.len(), selected);
            let saved = session.snapshot_v1();
            let before = serde_json::to_vec(&session.state).unwrap();
            session.conqueror_restore_v49(&saved).unwrap();
            assert!(session
                .conqueror_validate_bound_decision_v49(e, actor.into(), &binding)
                .is_err());
            assert_eq!(serde_json::to_vec(&session.state).unwrap(), before);
            let live = expected(&session);
            session.step(live.episode_id, live.step, 0).unwrap();
            if selected < 7 {
                assert_eq!(serde_json::to_vec(&session.state).unwrap(), before);
            }
        }
        assert!(session.state.engine.pending_triggers.is_empty());
        assert_eq!(
            session
                .state
                .stack
                .iter()
                .filter(|item| crate::conqueror_public_v49::selected(item))
                .count(),
            8
        );
        crate::bloodthirsty_conqueror_v49::validate(&session.state).unwrap();
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn mixed_seven_saved_losses_and_registered_pridemate_stage_one_complete_real_permutation_both_seats(
) {
    use crate::bloodthirsty_conqueror_v49::tests::future_printed;
    use crate::event::{self, ProposedEvent};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = future_printed::paid(actor);
        let pridemate = put(&mut state, actor, "Ajani's Pridemate", Zone::Battlefield);
        for _ in 0..7 {
            event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 1));
        }
        event::propose_and_commit(&mut state, ProposedEvent::life_gain(actor, 1));
        let mut session = FastActorSessionV1::from_conqueror_v49_fixture_state(state);
        for selected in 0..8 {
            let e = expected(&session);
            let (public, actions, binding) = session
                .conqueror_current_decision_input_v49(e, actor.into())
                .unwrap();
            assert_eq!(public.conqueror.trigger_order.len(), 7);
            assert_eq!(public.trigger_order.as_ref().unwrap().pending_count, 8);
            assert_eq!(actions.len(), 8 - selected);
            let before = serde_json::to_vec(&session.state).unwrap();
            let mut altered = binding.clone();
            altered.expected.acting_player = actor.opponent().into();
            assert!(session
                .conqueror_validate_bound_decision_v49(e, actor.into(), &altered)
                .is_err());
            assert_eq!(serde_json::to_vec(&session.state).unwrap(), before);
            session.step(e.episode_id, e.step, 0).unwrap();
            if selected < 7 {
                assert_eq!(serde_json::to_vec(&session.state).unwrap(), before);
            }
        }
        assert_eq!(session.state.stack.len(), 8);
        assert_eq!(
            session
                .state
                .stack
                .iter()
                .filter(|row| crate::conqueror_public_v49::selected(row))
                .count(),
            7
        );
        assert_eq!(
            session
                .state
                .stack
                .iter()
                .filter(|row| row.source == pridemate)
                .count(),
            1
        );
        crate::bloodthirsty_conqueror_v49::validate(&session.state).unwrap();
    }
}

fn choose(
    session: &mut FastActorSessionV1,
    predicate: impl Fn(&crate::rl::ActionSemanticV1) -> bool,
) {
    let e = expected(session);
    let (_, actions, _) = session
        .conqueror_current_decision_input_v49(e, e.acting_player)
        .unwrap();
    let index = actions
        .iter()
        .position(predicate)
        .expect("genuine selected action absent");
    session.step(e.episode_id, e.step, index as u32).unwrap();
}
#[test]
fn real_registered_generic_ward_payment_or_decline_keeps_original_spell_and_full_public_binding_both_seats(
) {
    use crate::mana::ManaColor;
    use crate::rl::ActionSemanticV1;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let mut state = crate::extra_turn_v1::tests::ready(actor);
            let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
            let terror = put(
                &mut state,
                actor.opponent(),
                "Tolarian Terror",
                Zone::Battlefield,
            );
            put(&mut state, actor, "Forest", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
            let mut session = FastActorSessionV1::from_conqueror_v49_fixture_state(state);
            choose(
                &mut session,
                |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==bolt.0),
            );
            choose(
                &mut session,
                |a| matches!(a,ActionSemanticV1::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==terror.0),
            );
            for _ in 0..16 {
                let e = expected(&session);
                let (o, actions, _) = session
                    .conqueror_current_decision_input_v49(e, e.acting_player)
                    .unwrap();
                assert!(o.conqueror.occurrences.is_empty());
                if actions
                    .iter()
                    .any(|a| matches!(a, ActionSemanticV1::ChooseEffectBoolean { .. }))
                {
                    break;
                }
                choose(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
            }
            let saved = session.snapshot_v1();
            let mut replay = session.clone();
            replay.conqueror_restore_v49(&saved).unwrap();
            for current in [&mut session, &mut replay] {
                choose(
                    current,
                    |a| matches!(a,ActionSemanticV1::ChooseEffectBoolean{value,..}if *value==pay),
                );
                for _ in 0..16 {
                    if current.state.stack.is_empty() {
                        break;
                    }
                    choose(current, |a| matches!(a, ActionSemanticV1::Pass { .. }));
                }
                assert_eq!(current.state.objects.get(bolt).zone, Zone::Graveyard);
                assert_eq!(
                    current.state.objects.get(terror).damage,
                    if pay { 3 } else { 0 }
                );
                assert_eq!(
                    current.state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                    if pay { 0 } else { 2 }
                );
                assert!(current.state.stack.is_empty());
            }
            assert_eq!(
                serde_json::to_vec(&session.state).unwrap(),
                serde_json::to_vec(&replay.state).unwrap()
            );
            assert_eq!(session.state.objects.get(terror).zone, Zone::Battlefield);
            assert!(session.state.engine.event_history.iter().any(|event|matches!(event,crate::event::CommittedEvent::Targeted{target,..}if *target==terror)));
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn legacy_episode_profiled_advance_refuses_fresh_and_closed_original_history_readonly_after_restore(
) {
    use crate::bloodthirsty_conqueror_v49::tests::future_printed;
    use crate::event::{self, ProposedEvent};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut original, source) = future_printed::paid(actor);
        event::propose_and_commit(&mut original, ProposedEvent::life_loss(actor.opponent(), 3));
        assert!(original.stack.is_empty());
        assert!(original.engine.event_log.iter().any(|event| matches!(
            event,
            crate::event::CommittedEvent::ConquerorCapturedV49 { .. }
        )));
        for closed in [false, true] {
            let mut state = original.clone();
            if closed {
                assert!(!matches!(
                    crate::engine::advance_until_decision(&mut state),
                    crate::engine::Decision::Halted { .. }
                ));
                let id = state
                    .stack
                    .iter()
                    .find(|item| crate::conqueror_public_v49::selected(item))
                    .unwrap()
                    .v4
                    .stack_item_id;
                crate::engine::counter_stack_item_by_id(&mut state, id).unwrap();
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Hand),
                );
                assert_eq!(
                    crate::conqueror_public_v49::capture(&state)
                        .unwrap()
                        .occurrences[0]
                        .phase,
                    crate::conqueror_public_v49::PhaseV49::Countered
                );
                assert!(state.stack.is_empty());
            }
            crate::bloodthirsty_conqueror_v49::validate(&state).unwrap();
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            let before = serde_json::to_vec(&restored).unwrap();
            let mut episode = RlEpisodeSessionV1::reset_with_limits(23, 91, 10_000, 10_000);
            episode.state = restored;
            episode.current = None;
            episode.terminal = None;
            let counts = (episode.policy_step_count, episode.physical_decision_count);
            episode.advance_to_decision_or_terminal_profiled(None);
            assert!(episode.terminal.is_some() && episode.current.is_none());
            assert_eq!(
                (episode.policy_step_count, episode.physical_decision_count),
                counts
            );
            assert_eq!(serde_json::to_vec(&episode.state).unwrap(), before);
            // Repeat the actual legacy profiled route; no collection, publication,
            // gain or unrelated automatic phase work may precede the refusal.
            episode.terminal = None;
            episode.advance_to_decision_or_terminal_profiled(None);
            assert!(episode.terminal.is_some() && episode.current.is_none());
            assert_eq!(serde_json::to_vec(&episode.state).unwrap(), before);
        }
    }
}
