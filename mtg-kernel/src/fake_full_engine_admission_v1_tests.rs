//! Public engine admission from the genuine reference input. Source fixtures
//! only until the integrating owner qualifies the new actual catalog.
use crate::{
    ids::PlayerId,
    limited_session_v1::CustomCardCountV1,
    rl::ActionSemanticV1,
    rl_session::{FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1},
    state::GameState,
};

pub(crate) fn reference_rows() -> Vec<CustomCardCountV1> {
    include_str!("../../data/limited/fdn_v1/FDN_reference_fake_your_own_death_partial.dck")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (count, name) = line
                .trim()
                .split_once(' ')
                .expect("reference count and exact name");
            CustomCardCountV1 {
                name: name.into(),
                count: count.parse().expect("reference count"),
            }
        })
        .collect()
}
fn reference_ids() -> Vec<u16> {
    reference_rows()
        .into_iter()
        .flat_map(|row| {
            let id =
                crate::card_def::card_id_by_name(&row.name).expect("registered reference printing");
            std::iter::repeat_n(id, row.count as usize)
        })
        .collect()
}
fn state(session: &FastActorSessionV1, ravenous: bool) -> &GameState {
    if ravenous {
        session.ravenous_fixture_state()
    } else {
        session.fake_fixture_state()
    }
}
fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(value) = session.current_response() else {
        panic!("public admitted reference decision")
    };
    value
}
fn act(session: &mut FastActorSessionV1, ravenous: bool, keep: bool) {
    let e = expected(session);
    let actions = if ravenous {
        let (_, actions, binding) = session
            .ravenous_current_decision_input_v51(e, e.acting_player)
            .unwrap();
        session
            .ravenous_validate_bound_decision_v51(e, e.acting_player, &binding)
            .unwrap();
        actions
    } else {
        let (_, actions, binding) = session
            .fake_current_decision_input_v50(e, e.acting_player)
            .unwrap();
        session
            .fake_validate_bound_decision_v50(e, e.acting_player, &binding)
            .unwrap();
        actions
    };
    let index = actions
        .iter()
        .position(|action| {
            if keep {
                matches!(
                    action,
                    ActionSemanticV1::ChooseLondonMulligan {
                        mulligan: false,
                        ..
                    }
                )
            } else {
                matches!(action, ActionSemanticV1::Pass { .. })
            }
        })
        .expect("real keep/priority action");
    session.step(e.episode_id, e.step, index as u32).unwrap();
}
fn outcome(session: &FastActorSessionV1, ravenous: bool) -> (Vec<u8>, u64, u64) {
    (
        serde_json::to_vec(state(session, ravenous)).unwrap(),
        session.policy_step_count(),
        session.physical_decision_count(),
    )
}
#[test]
fn public_v50_v51_fake_reference_resets_and_owned_restore_continue_gameplay_both_seats() {
    let ids = reference_ids();
    assert_eq!(ids.len(), 40);
    assert_eq!(ids.iter().filter(|id| **id == 442).count(), 4);
    crate::card_def::preflight_fully_supported_deck(&ids).unwrap();
    for actor in [PlayerId::P0, PlayerId::P1] {
        for ravenous in [false, true] {
            let arguments = (
                77,
                912,
                (10000, 10000),
                [
                    "genuine_fake_reference".into(),
                    "genuine_fake_reference".into(),
                ],
                [ids.clone(), ids.clone()],
                actor,
            );
            let mut session = if ravenous {
                FastActorSessionV1::reset_ravenous_v51(
                    arguments.0,
                    arguments.1,
                    arguments.2,
                    arguments.3,
                    arguments.4,
                    arguments.5,
                )
                .unwrap()
            } else {
                FastActorSessionV1::reset_fake_v50(
                    arguments.0,
                    arguments.1,
                    arguments.2,
                    arguments.3,
                    arguments.4,
                    arguments.5,
                )
                .unwrap()
            };
            assert_eq!(
                expected(&session).acting_player,
                crate::rl::PlayerSeatV1::from(actor)
            );
            for owner in [PlayerId::P0, PlayerId::P1] {
                assert_eq!(
                    state(&session, ravenous)
                        .objects
                        .iter()
                        .filter(|(_, o)| o.owner == owner && o.card_def == 442)
                        .count(),
                    4
                );
                assert_eq!(
                    state(&session, ravenous).players[owner.index()].hand.len(),
                    7
                );
            }
            assert!(state(&session, ravenous)
                .engine
                .combat
                .foundations_v1
                .is_some());
            let snapshot = session.snapshot_v1();
            let stale = expected(&session);
            act(&mut session, ravenous, true);
            act(&mut session, ravenous, true);
            act(&mut session, ravenous, false);
            let continued = outcome(&session, ravenous);
            if ravenous {
                session.ravenous_restore_v51(&snapshot).unwrap();
            } else {
                session.fake_restore_v50(&snapshot).unwrap();
            }
            assert!(expected(&session).environment_revision > stale.environment_revision);
            if ravenous {
                assert!(session
                    .ravenous_current_decision_input_v51(stale, stale.acting_player)
                    .is_err());
            } else {
                assert!(session
                    .fake_current_decision_input_v50(stale, stale.acting_player)
                    .is_err());
            }
            act(&mut session, ravenous, true);
            act(&mut session, ravenous, true);
            act(&mut session, ravenous, false);
            assert_eq!(outcome(&session, ravenous), continued);
            let e = expected(&session);
            assert!(session.flat_policy_observation_v1(e).is_err());
            assert!(session.flat_policy_observation_v2(e).is_err());
            assert!(session.flat_policy_observation_v3(e).is_err());
            assert!(session.flat_policy_observation_v4(e).is_err());
            assert!(session.flat_policy_observation_v5(e).is_err());
            assert!(session.kernel_search_redeterminized_clone_v4(1).is_err());
            for root in [
                crate::rules_meaning_fake_your_own_death_v50::RootV50::InstantSpell,
                crate::rules_meaning_fake_your_own_death_v50::RootV50::GainedDiesAbility,
            ] {
                let meaning =
                    crate::rules_meaning_fake_your_own_death_v50::printed_meaning(442, root)
                        .unwrap();
                assert_eq!(meaning.capability, crate::card_def::CardCapability::Full);
                assert!(!meaning.model_admitted);
            }
        }
    }
}
