//! Honest registered-recipient opcode fixtures, not printed card admission.
use super::*;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::policy_observation_v6::tests::{put, ready_state};

fn installed(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
    let recipient = put(
        &mut state,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let mut ctx = ExecCtx::no_targets(source, actor);
    ctx.targets.push(Target::Object(recipient));
    ctx.target_contracts.push(StackTargetContractV4::capture(
        &state,
        Target::Object(recipient),
    ));
    gain_control(&ctx, 0, &mut state);
    authority::validate(&state).unwrap();
    (state, source, recipient)
}
fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
    assert_eq!(state.state_hash(), restored.state_hash());
    restored
}
fn readonly_refusal(state: GameState, source: ObjectId) {
    for mut restored in [restore(&state), restore(&restore(&state))] {
        let before = serde_json::to_vec(&restored).unwrap();
        assert!(authority::validate(&restored).is_err());
        assert!(crate::engine::step(&mut restored, crate::engine::Action::Pass).is_err());
        assert_eq!(before, serde_json::to_vec(&restored).unwrap());
        event::propose_and_commit(
            &mut restored,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        assert_eq!(before, serde_json::to_vec(&restored).unwrap());
    }
}
fn control_row(state: &GameState) -> UntilEndOfTurnEffect {
    state
        .engine
        .until_end_of_turn
        .iter()
        .find(|row| {
            matches!(
                row,
                UntilEndOfTurnEffect::ResolvedObjectControlEffect { .. }
            )
        })
        .unwrap()
        .clone()
}
fn cleanup(state: &mut GameState) {
    state.step = crate::state::Step::End;
    for _ in 0..12 {
        let decision = crate::engine::advance_until_decision(state);
        if !state.engine.until_end_of_turn.iter().any(|row| {
            matches!(
                row,
                UntilEndOfTurnEffect::ResolvedObjectControlEffect { .. }
            )
        }) {
            authority::validate(state).unwrap();
            return;
        }
        match decision {
            crate::engine::Decision::CastSpellOrPass { .. } => {
                crate::engine::step(state, crate::engine::Action::Pass).unwrap();
            }
            other => panic!("unexpected cleanup {other:?}"),
        }
    }
    panic!("cleanup did not expire control");
}

#[test]
fn committed_installation_binds_complete_row_and_refuses_erasure_rewrite_duplicates_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, source, _) = installed(actor);
        for alteration in 0..6 {
            let mut forged = restore(&state);
            let row = control_row(&forged);
            let index = forged
                .engine
                .until_end_of_turn
                .iter()
                .position(|effect| effect == &row)
                .unwrap();
            match alteration {
                0 => {
                    forged.engine.until_end_of_turn.remove(index);
                }
                1 => forged.engine.until_end_of_turn.push(row),
                2 => {
                    let UntilEndOfTurnEffect::ResolvedObjectControlEffect { controller, .. } =
                        &mut forged.engine.until_end_of_turn[index]
                    else {
                        unreachable!()
                    };
                    *controller = actor.opponent();
                }
                3 => {
                    let UntilEndOfTurnEffect::ResolvedObjectControlEffect {
                        object_zone_change_count,
                        ..
                    } = &mut forged.engine.until_end_of_turn[index]
                    else {
                        unreachable!()
                    };
                    *object_zone_change_count += 1;
                }
                4 => forged.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::TemporaryControlInstalledV1 { .. })
                }),
                5 => {
                    let event = forged
                        .engine
                        .event_history
                        .iter()
                        .find(|event| {
                            matches!(event, CommittedEvent::TemporaryControlInstalledV1 { .. })
                        })
                        .unwrap()
                        .clone();
                    forged.engine.event_log.push(event);
                }
                _ => unreachable!(),
            }
            readonly_refusal(forged, source);
        }
    }
}

#[test]
fn actual_cleanup_authority_rejects_expiry_erasure_rewrite_duplicates_and_resurrection_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = installed(actor);
        let saved_row = control_row(&state);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        state.engine.event_log.clear();
        cleanup(&mut state);
        assert_eq!(state.objects.get(recipient).controller, actor.opponent());
        authority::validate(&restore(&state)).unwrap();
        for alteration in 0..5 {
            let mut forged = restore(&state);
            let index = forged
                .engine
                .event_history
                .iter()
                .position(|event| matches!(event, CommittedEvent::TemporaryControlExpiredV1 { .. }))
                .unwrap();
            match alteration {
                0 => {
                    forged.engine.event_history.remove(index);
                }
                1 => forged
                    .engine
                    .event_history
                    .push(forged.engine.event_history[index].clone()),
                2 => {
                    let CommittedEvent::TemporaryControlExpiredV1 {
                        cleanup_history_index,
                        ..
                    } = &mut forged.engine.event_history[index]
                    else {
                        unreachable!()
                    };
                    *cleanup_history_index = 0;
                }
                3 => forged.engine.until_end_of_turn.push(saved_row.clone()),
                4 => forged.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::TemporaryControlCleanupV1 { .. })
                }),
                _ => unreachable!(),
            }
            readonly_refusal(forged, source);
        }
    }
}

#[test]
fn later_same_cleanup_installation_expires_via_real_repeated_cleanup_before_player_flip() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = installed(actor);
        cleanup(&mut state);
        state.step = crate::state::Step::Cleanup;
        state.active_player = actor;
        state.priority_player = actor;
        state.engine.priority_passes = [true; 2];
        let mut ctx = ExecCtx::no_targets(source, actor);
        ctx.targets.push(Target::Object(recipient));
        ctx.target_contracts.push(StackTargetContractV4::capture(
            &state,
            Target::Object(recipient),
        ));
        gain_control(&ctx, 0, &mut state);
        authority::validate(&state).unwrap();
        let mut restored = restore(&state);
        let _ = crate::engine::advance_until_decision(&mut restored);
        assert!(restored
            .engine
            .until_end_of_turn
            .iter()
            .all(|row| !matches!(
                row,
                UntilEndOfTurnEffect::ResolvedObjectControlEffect { .. }
            )));
        assert_eq!(restored.objects.get(recipient).controller, actor.opponent());
        authority::validate(&restored).unwrap();
        assert_eq!(
            restored
                .engine
                .event_history
                .iter()
                .filter(|event| matches!(event, CommittedEvent::TemporaryControlCleanupV1 { .. }))
                .count(),
            2
        );
    }
}

#[test]
fn ordinary_registered_program_emits_no_control_authority_and_retains_restore_bytes() {
    let mut state = ready_state();
    let source = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    execute(
        &EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 1,
        },
        &ExecCtx::no_targets(source, PlayerId::P0),
        &mut state,
    );
    assert!(state.engine.event_history.iter().all(|event| !matches!(
        event,
        CommittedEvent::TemporaryControlResolutionV1 { .. }
            | CommittedEvent::TemporaryControlInstalledV1 { .. }
            | CommittedEvent::TemporaryControlCleanupV1 { .. }
            | CommittedEvent::TemporaryControlExpiredV1 { .. }
    )));
    authority::validate(&state).unwrap();
    let _ = restore(&state);
}
