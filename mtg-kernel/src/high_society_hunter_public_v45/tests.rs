#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::put;

#[test]
fn registered_absence_public_facts_do_not_read_either_private_hand_or_library_and_refuse_forgery() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut left = crate::extra_turn_v1::tests::ready(actor);
        let mut right = left.clone();
        put(&mut left, actor.opponent(), "Island", Zone::Hand);
        put(&mut right, actor.opponent(), "Forest", Zone::Hand);
        right.players[actor.opponent().index()].library.reverse();
        let original = serde_json::to_vec(&left).unwrap();
        let facts = capture(&left).unwrap();
        assert_eq!(facts, capture(&right).unwrap());
        validate(&left, &facts).unwrap();
        let mut bad = facts.clone();
        bad.card_db_hash ^= 1;
        assert!(validate(&left, &bad).is_err());
        let mut bad = facts.clone();
        bad.producer_sha256.push('0');
        assert!(validate(&left, &bad).is_err());
        let mut unknown = serde_json::to_value(&facts).unwrap();
        unknown["private_library"] = serde_json::json!([]);
        assert!(serde_json::from_value::<FactsV45>(unknown).is_err());
        assert_eq!(serde_json::to_vec(&left).unwrap(), original);
        assert!(facts.occurrences.is_empty());
        assert!(facts.menu.is_none());
    }
}
