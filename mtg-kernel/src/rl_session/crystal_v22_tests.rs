use super::*;
use crate::policy_observation_v6::tests::put;

#[test]
fn complete_ordered_binding_observation_identity_seat_and_menu_changes_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        put(&mut state, actor, "Mountain", Zone::Hand);
        let mut session = FastActorSessionV1::from_crystal_v22_fixture_state(state);
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("real priority decision");
        };
        let (_, _, binding) = session
            .crystal_current_decision_input_v22(expected, actor.into())
            .unwrap();
        let before = session.privileged_core_environment_hash();
        let mut forged = binding.clone();
        forged.candidates.reverse();
        assert!(session
            .crystal_validate_bound_decision_v22(expected, actor.into(), &forged)
            .is_err());
        let mut forged = binding.clone();
        forged.observation.producer_sha256.clear();
        assert!(session
            .crystal_validate_bound_decision_v22(expected, actor.into(), &forged)
            .is_err());
        assert!(session
            .crystal_current_decision_input_v22(expected, actor.opponent().into())
            .is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
        session.crystal_fixture_state_mut().players[actor.index()].life += 1;
        let before = session.privileged_core_environment_hash();
        assert!(session
            .crystal_validate_bound_decision_v22(expected, actor.into(), &binding)
            .is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
    }
}
