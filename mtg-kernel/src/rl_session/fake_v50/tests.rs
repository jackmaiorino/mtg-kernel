use super::*;
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::put;
use crate::state::Zone;

fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    match session.current_response() {
        FastActorResponseV1::Decision(e) => e,
        other => panic!("actual V50 input: {other:?}"),
    }
}
fn fixture(actor: PlayerId) -> (FastActorSessionV1, ObjectId, ObjectId, [ObjectId; 2]) {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    put(&mut state, actor, "Forest", Zone::Hand);
    for player in [actor, actor.opponent()] {
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
        put(&mut state, player, "Mountain", Zone::Battlefield);
    }
    let spell = put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
    let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    let swamps = std::array::from_fn(|_| put(&mut state, actor, "Swamp", Zone::Battlefield));
    (
        FastActorSessionV1::from_fake_v50_fixture_state(state),
        spell,
        target,
        swamps,
    )
}
fn act(session: &mut FastActorSessionV1, choose: impl Fn(&ActionSemanticV1) -> bool) {
    let e = expected(session);
    let (_, actions, binding) = session
        .fake_current_decision_input_v50(e, e.acting_player)
        .unwrap();
    let index = actions
        .iter()
        .position(choose)
        .unwrap_or_else(|| panic!("actual V50 action absent: {actions:?}"));
    session
        .fake_validate_bound_decision_v50(e, e.acting_player, &binding)
        .unwrap();
    session
        .step(e.episode_id, e.step, u32::try_from(index).unwrap())
        .unwrap();
}
fn drain(session: &mut FastActorSessionV1) -> FakeObservationV50 {
    for _ in 0..32 {
        let e = expected(session);
        let public = session
            .fake_current_decision_input_v50(e, e.acting_player)
            .unwrap()
            .0;
        if session.state.stack.is_empty() && session.state.engine.pending_triggers.is_empty() {
            return public;
        }
        act(session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
    }
    panic!("actual V50 stack did not finish");
}
fn gameplay(session: &FastActorSessionV1) -> (Vec<u8>, u64, u64) {
    (
        serde_json::to_vec(session.fake_fixture_state()).unwrap(),
        session.policy_step_count(),
        session.physical_decision_count(),
    )
}

#[test]
fn actual_paid_fake_backend_cast_grant_and_death_return_replay_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut session, spell, target, swamps) = fixture(actor);
        for swamp in swamps {
            act(
                &mut session,
                |a| matches!(a, ActionSemanticV1::ActivateManaAbility { source, .. } if source.arena_id == swamp.0),
            );
        }
        assert_eq!(
            session.state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
            2
        );
        act(
            &mut session,
            |a| matches!(a, ActionSemanticV1::CastSpell { source, .. } if source.arena_id == spell.0),
        );
        act(
            &mut session,
            |a| matches!(a, ActionSemanticV1::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. } if object.arena_id == target.0),
        );
        assert_eq!(
            session.state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
            0
        );
        let snapshot = session.snapshot_v1();
        let public = drain(&mut session);
        assert_eq!(public.fake.grants.len(), 1);
        assert!(public.fake.grants[0].death_ability_active);
        assert_eq!(crate::engine::effective_power(&session.state, target), 3);
        let first = gameplay(&session);
        let revision = expected(&session).environment_revision;
        session.fake_restore_v50(&snapshot).unwrap();
        assert!(expected(&session).environment_revision > revision);
        assert_eq!(drain(&mut session), public);
        assert_eq!(gameplay(&session), first);

        // Ordinary accepted zone change exercises the genuinely paid gained
        // ability. It is not a claim for a separate printed kill spell.
        event::propose_and_commit(
            &mut session.state,
            ProposedEvent::zone_change(target, Zone::Graveyard),
        );
        session.current = None;
        session.environment_revision += 1;
        session.advance_to_decision_or_terminal();
        let death_snapshot = session.snapshot_v1();
        let result = drain(&mut session);
        assert_eq!(result.fake.deaths.len(), 1);
        let death = &result.fake.deaths[0];
        assert_eq!(death.returned_controller, Some(actor.into()));
        assert_eq!(death.treasure_controller, Some(actor.into()));
        assert_eq!(death.treasures.len(), 1);
        assert_eq!(session.state.objects.get(target).zone, Zone::Battlefield);
        assert!(session.state.objects.get(target).tapped);
        let final_gameplay = gameplay(&session);
        session.fake_restore_v50(&death_snapshot).unwrap();
        assert_eq!(drain(&mut session), result);
        assert_eq!(gameplay(&session), final_gameplay);
    }
}

#[test]
fn actual_fake_backend_complete_binding_and_identity_forgeries_refuse_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (session, _, _, _) = fixture(actor);
        let e = expected(&session);
        let (_, _, binding) = session
            .fake_current_decision_input_v50(e, actor.into())
            .unwrap();
        assert!(binding.candidates.len() > 1);
        let before = session.privileged_core_environment_hash();
        let mut reordered = binding.clone();
        reordered.candidates.reverse();
        assert!(session
            .fake_validate_bound_decision_v50(e, actor.into(), &reordered)
            .is_err());
        let mut forged = binding.clone();
        forged.observation.projection_sha256.push('0');
        assert!(session
            .fake_validate_bound_decision_v50(e, actor.into(), &forged)
            .is_err());
        for case in 0..9 {
            let mut stale = e;
            match case {
                0 => stale.episode_id += 1,
                1 => stale.environment_revision += 1,
                2 => stale.step += 1,
                3 => stale.physical_decision_id += 1,
                4 => stale.substep_index += 1,
                5 => stale.substep_count += 1,
                6 => stale.acting_player = actor.opponent().into(),
                7 => stale.legal_action_count += 1,
                8 => stale.step = u64::MAX,
                _ => unreachable!(),
            }
            assert!(session
                .fake_validate_bound_decision_v50(stale, actor.into(), &binding)
                .is_err());
        }
        assert!(session
            .fake_validate_bound_decision_v50(e, actor.opponent().into(), &binding)
            .is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
        let mut altered = session.clone();
        altered.current.as_mut().unwrap().candidates.reverse();
        let before = altered.privileged_core_environment_hash();
        assert!(altered
            .fake_validate_bound_decision_v50(e, actor.into(), &binding)
            .is_err());
        assert_eq!(altered.privileged_core_environment_hash(), before);
    }
}

#[test]
fn generic_restore_preserves_v50_revision_and_cannot_import_mode_or_foreign_episode_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut session, _, _, swamps) = fixture(actor);
        let snapshot = session.snapshot_v1();
        let e = expected(&session);
        let (_, _, binding) = session
            .fake_current_decision_input_v50(e, actor.into())
            .unwrap();
        let original = gameplay(&session);
        act(
            &mut session,
            |a| matches!(a, ActionSemanticV1::ActivateManaAbility { source, .. } if source.arena_id == swamps[0].0),
        );
        let revision = expected(&session).environment_revision;
        session.restore_v1(&snapshot);
        assert!(expected(&session).environment_revision > revision);
        assert_eq!(gameplay(&session), original);
        assert!(session
            .fake_validate_bound_decision_v50(e, actor.into(), &binding)
            .is_err());
        let mut foreign = snapshot.clone();
        foreign.0.episode_id += 1;
        let before = session.privileged_core_environment_hash();
        assert!(session.fake_restore_v50(&foreign).is_err());
        session.restore_v1(&foreign);
        assert_eq!(session.privileged_core_environment_hash(), before);
        let mut old_state = crate::extra_turn_v1::tests::ready(actor);
        put(&mut old_state, actor, "Forest", Zone::Hand);
        let mut old = FastActorSessionV1::from_human_v8_fixture_state(old_state);
        let before = old.privileged_core_environment_hash();
        old.restore_v1(&snapshot);
        assert_eq!(old.privileged_core_environment_hash(), before);
        assert_ne!(
            old.flat_action_contract_mode,
            FlatActionContractModeV1::FakeV50
        );
    }
}

#[test]
fn v50_optin_keeps_partial_deck_rejection_and_legacy_visible_refusal_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (session, spell, _, _) = fixture(actor);
        let definition = session.state.objects.get(spell).card_def;
        assert_eq!(
            crate::card_def::CARD_DEFS[definition as usize].capability,
            crate::card_def::CardCapability::Partial
        );
        assert!(crate::card_def::preflight_fully_supported_deck(&[definition]).is_err());
        let before = serde_json::to_vec(&session.state).unwrap();
        assert!(
            crate::fake_your_own_death_v50::public_v50::require_legacy_visible(
                &session.state,
                Some(actor)
            )
            .is_err()
        );
        assert_eq!(serde_json::to_vec(&session.state).unwrap(), before);
        let old = FastActorSessionV1::from_human_v8_fixture_state(session.state.clone());
        assert!(matches!(
            old.current_response(),
            FastActorResponseV1::Terminal(_)
        ));
        assert_eq!(serde_json::to_vec(&old.state).unwrap(), before);
        assert_eq!(session.state.objects.get(spell).zone, Zone::Hand);
    }
}
