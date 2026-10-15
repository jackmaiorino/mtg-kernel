//! Genuine public deck admission. These source fixtures await successor
//! catalog and native qualification; they grant no model or search admission.
use crate::{
    ids::PlayerId,
    limited_session_v1::CustomCardCountV1,
    rl::ActionSemanticV1,
    rl_session::{FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1},
};

pub(crate) fn reference_rows() -> Vec<CustomCardCountV1> {
    include_str!("../../data/limited/fdn_v1/FDN_reference_ravenous_amulet_partial.dck")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (count, name) = line.trim().split_once(' ').expect("reference count/name");
            CustomCardCountV1 {
                name: name.into(),
                count: count.parse().expect("reference count"),
            }
        })
        .collect()
}
fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(value) = session.current_response() else {
        panic!("public admitted reference decision")
    };
    value
}
fn act(session: &mut FastActorSessionV1, keep: bool) {
    let e = expected(session);
    let (_, actions, binding) = session
        .ravenous_current_decision_input_v51(e, e.acting_player)
        .unwrap();
    session
        .ravenous_validate_bound_decision_v51(e, e.acting_player, &binding)
        .unwrap();
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
fn outcome(session: &FastActorSessionV1) -> (Vec<u8>, u64, u64) {
    (
        serde_json::to_vec(session.ravenous_fixture_state()).unwrap(),
        session.policy_step_count(),
        session.physical_decision_count(),
    )
}
#[test]
fn public_v51_ravenous_reference_reset_owned_restore_and_gameplay_both_seats() {
    let amulet = crate::card_def::card_id_by_name("Ravenous Amulet").unwrap();
    assert_eq!(amulet, 443);
    let ids: Vec<_> = reference_rows()
        .into_iter()
        .flat_map(|row| {
            let id = crate::card_def::card_id_by_name(&row.name).expect("genuine printing");
            std::iter::repeat_n(id, row.count as usize)
        })
        .collect();
    assert_eq!(ids.len(), 40);
    assert_eq!(ids.iter().filter(|id| **id == amulet).count(), 4);
    crate::card_def::preflight_fully_supported_deck(&ids).unwrap();
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session = FastActorSessionV1::reset_ravenous_v51(
            77,
            912,
            (10000, 10000),
            [
                "genuine_ravenous_reference".into(),
                "genuine_ravenous_reference".into(),
            ],
            [ids.clone(), ids.clone()],
            actor,
        )
        .unwrap();
        assert_eq!(
            expected(&session).acting_player,
            crate::rl::PlayerSeatV1::from(actor)
        );
        for owner in [PlayerId::P0, PlayerId::P1] {
            let state = session.ravenous_fixture_state();
            assert_eq!(state.players[owner.index()].hand.len(), 7);
            assert_eq!(
                state
                    .objects
                    .iter()
                    .filter(|(_, object)| object.owner == owner && object.card_def == amulet)
                    .count(),
                4
            );
        }
        assert!(session
            .ravenous_fixture_state()
            .engine
            .combat
            .foundations_v1
            .is_some());
        let snapshot = session.snapshot_v1();
        let stale = expected(&session);
        let (_, _, old_binding) = session
            .ravenous_current_decision_input_v51(stale, stale.acting_player)
            .unwrap();
        act(&mut session, true);
        act(&mut session, true);
        act(&mut session, false);
        let continued = outcome(&session);
        session.ravenous_restore_v51(&snapshot).unwrap();
        assert!(expected(&session).environment_revision > stale.environment_revision);
        let before = outcome(&session);
        assert!(session
            .ravenous_validate_bound_decision_v51(stale, stale.acting_player, &old_binding)
            .is_err());
        assert_eq!(outcome(&session), before);
        act(&mut session, true);
        act(&mut session, true);
        act(&mut session, false);
        assert_eq!(outcome(&session), continued);
        let e = expected(&session);
        assert!(session.flat_policy_observation_v1(e).is_err());
        assert!(session.flat_policy_observation_v2(e).is_err());
        assert!(session.flat_policy_observation_v3(e).is_err());
        assert!(session.flat_policy_observation_v4(e).is_err());
        assert!(session.flat_policy_observation_v5(e).is_err());
        assert!(session.kernel_search_redeterminized_clone_v4(1).is_err());
        for root in [
            crate::rules_meaning_ravenous_amulet_v51::RootV51::DrawAndSoul,
            crate::rules_meaning_ravenous_amulet_v51::RootV51::SoulLoss,
        ] {
            let meaning =
                crate::rules_meaning_ravenous_amulet_v51::printed_meaning(amulet, root).unwrap();
            assert_eq!(meaning.capability, crate::card_def::CardCapability::Full);
            assert!(!meaning.model_admitted);
        }
    }
}
