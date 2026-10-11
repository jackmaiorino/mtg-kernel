//! Future actual-definition qualification, requiring root-owned registration.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::rules_meaning_crystal_barricade_v1::{
    self as meaning, ErrorV1, MeaningV1, RuleV1, StaticRootV1,
};

fn roots() -> Vec<MeaningV1> {
    let id = mtg_kernel::card_def::card_id_by_name("Crystal Barricade")
        .expect("actual root-owned Crystal registration required");
    let selector = meaning::live_selector().unwrap();
    [
        StaticRootV1::ControllerHexproof,
        StaticRootV1::OtherControlledCreatureNoncombatPrevention,
    ]
    .into_iter()
    .map(|root| meaning::meaning(&selector, id, root).unwrap())
    .collect()
}
#[test]
fn actual_selected_roots_roundtrip_and_distinguish_player_targeting_from_prevention() {
    let records = roots();
    assert_ne!(records[0].root_sha256, records[1].root_sha256);
    for record in records {
        meaning::validate(&record).unwrap();
        let bytes = serde_json::to_vec(&record).unwrap();
        let restored: MeaningV1 = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored, record);
        meaning::validate(&restored).unwrap();
        assert!(record.current_controller_is_beneficiary);
        assert!(record.departure_or_ability_suppression_ends_rule_immediately);
        match record.rule {
            RuleV1::ControllerHexproof {
                controller_can_target_themself,
                does_not_grant_object_hexproof,
                target_legality_rechecked_at_resolution,
                ..
            } => {
                assert!(controller_can_target_themself);
                assert!(does_not_grant_object_hexproof);
                assert!(target_legality_rechecked_at_resolution);
            }
            RuleV1::OtherControlledCreatureNoncombatPrevention {
                another_source_can_protect_this_source,
                cannot_be_prevented_overrides_this_rule,
                affected_player_mixed_replacement_ordering_supported,
                ..
            } => {
                assert!(another_source_can_protect_this_source);
                assert!(cannot_be_prevented_overrides_this_rule);
                assert!(!affected_player_mixed_replacement_ordering_supported);
            }
        }
    }
}
#[test]
fn coherent_semantic_rewrites_and_unknown_fields_are_refused() {
    for record in roots() {
        let mut forged = record.clone();
        forged.current_controller_is_beneficiary = false;
        assert_eq!(meaning::validate(&forged), Err(ErrorV1::IdentityMismatch));
        let mut forged = record.clone();
        forged.root_sha256.push('x');
        assert_eq!(meaning::validate(&forged), Err(ErrorV1::IdentityMismatch));
        let mut forged = record.clone();
        forged.static_root = match record.static_root {
            StaticRootV1::ControllerHexproof => {
                StaticRootV1::OtherControlledCreatureNoncombatPrevention
            }
            _ => StaticRootV1::ControllerHexproof,
        };
        assert_eq!(meaning::validate(&forged), Err(ErrorV1::IdentityMismatch));
        let mut value = serde_json::to_value(&record).unwrap();
        let rule = value["rule"].as_object_mut().unwrap();
        let fields = rule.values_mut().next().unwrap().as_object_mut().unwrap();
        // The validator rebuilds every fact from the actual selected rule.
        let keys = fields.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            let mut altered = serde_json::to_value(&record).unwrap();
            let fields = altered["rule"]
                .as_object_mut()
                .unwrap()
                .values_mut()
                .next()
                .unwrap()
                .as_object_mut()
                .unwrap();
            let previous = fields[&key].as_bool().unwrap();
            fields.insert(key, (!previous).into());
            let forged: MeaningV1 = serde_json::from_value(altered).unwrap();
            assert_eq!(meaning::validate(&forged), Err(ErrorV1::IdentityMismatch));
        }
        value["invented_controller"] = "opponent".into();
        assert!(serde_json::from_value::<MeaningV1>(value).is_err());
        let mut value = serde_json::to_value(&record).unwrap();
        value["rule"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap()["invented_player_damage"] = true.into();
        assert!(serde_json::from_value::<MeaningV1>(value).is_err());
    }
}
