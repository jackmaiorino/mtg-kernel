use super::*;
use crate::policy_observation_v6::tests::put;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::state::Zone;

#[test]
fn registered_predecessor_projection_private_fields_and_ordered_actions_are_exact_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        let old = FastActorSessionV1::from_divine_v44_fixture_state(state.clone());
        let new = FastActorSessionV1::from_hunter_v45_fixture_state(state);
        let FastActorResponseV1::Decision(e) = old.current_response() else {
            panic!("old ordinary decision missing")
        };
        let FastActorResponseV1::Decision(n) = new.current_response() else {
            panic!("new ordinary decision missing")
        };
        let (a, actions, _) = old
            .divine_current_decision_input_v44(e, actor.into())
            .unwrap();
        let (b, new_actions, _) = new
            .hunter_current_decision_input_v45(n, actor.into())
            .unwrap();
        assert_eq!(actions, new_actions);
        assert_eq!(a.projection, b.projection);
        assert_eq!(a.extensions, b.extensions);
        assert_eq!(a.own_hand, b.own_hand);
        assert_eq!(a.known_hand_cards, b.known_hand_cards);
        assert_eq!(a.known_library_cards, b.known_library_cards);
        assert_eq!(a.known_face_down_cards, b.known_face_down_cards);
        assert_eq!(a.families, b.families);
        assert_eq!(a.divine, b.divine);
        assert_eq!(a.ajani, b.ajani);
        assert_eq!(a.faebloom, b.faebloom);
        assert!(b.hunter.occurrences.is_empty());
        b.validate_artifact().unwrap();
        let mut bad = b;
        bad.hunter.card_db_hash ^= 1;
        bad.projection_sha256 = commitment(&bad).unwrap();
        assert!(bad.validate_artifact().is_err());
    }
}
