//! Actual canonical programs retain every ordered member and semantic boolean.
//! The spelling conversion grants no exception to unrelated raw identity keys.
use super::*;
use crate::current_mixed_policy_v1::{normalize, Source};
use serde_json::json;
fn check(mut value: Value, member: Option<&str>, names: &[(&str, &str)]) {
    let original = value.clone();
    let mut expected = value.clone();
    let fields = if let Some(key) = member {
        &mut expected[key]
    } else {
        &mut expected
    }
    .as_object_mut()
    .unwrap();
    for (old, new) in names {
        let boolean = fields.remove(*old).unwrap();
        assert!(boolean.is_boolean());
        fields.insert((*new).into(), boolean);
    }
    prepare(&mut value).unwrap();
    assert_eq!(value, expected); // No other field, array order or value changed.
    normalize::encode(
        Source::FakeV50,
        original.clone(),
        vec![json!({"kind":"Pass"})],
    )
    .unwrap();
    for (old, _) in names {
        for wrong in [
            json!(37),
            json!(!member.map_or(&original, |key| &original[key])[*old]
                .as_bool()
                .unwrap()),
        ] {
            let mut forged = original.clone();
            if let Some(key) = member {
                forged[key][*old] = wrong;
            } else {
                forged[*old] = wrong;
            }
            assert!(
                normalize::encode(Source::FakeV50, forged, vec![json!({"kind":"Pass"})]).is_err()
            );
        }
        // The identical spelling outside a canonical meaning remains rejected,
        // even when its value happens to be a boolean.
        let raw = json!({(*old):true});
        assert!(normalize::encode(Source::FakeV50, raw, vec![json!({"kind":"Pass"})]).is_err());
    }
    for key in ["unknown_generation", "zone_change_count", "unknown_raw_id"] {
        let mut forged = original.clone();
        forged[key] = json!(23);
        assert!(normalize::encode(Source::FakeV50, forged, vec![json!({"kind":"Pass"})]).is_err());
        assert!(normalize::encode(
            Source::FakeV50,
            json!({(key):23}),
            vec![json!({"kind":"Pass"})]
        )
        .is_err());
    }
}
#[test]
fn all_five_canonical_semantic_booleans_preserve_programs_and_reject_forged_or_unpaired_operands() {
    use crate::card_def::card_id_by_name as id;
    let ash = crate::rules_meaning_ashroot_v1::meaning(
        &crate::rules_meaning_ashroot_v1::live_selector().unwrap(),
        id("Ashroot Animist").unwrap(),
        0,
    )
    .unwrap();
    check(
        serde_json::to_value(ash).unwrap(),
        Some("rule"),
        &[
            (
                "original_source_owner_battlefield_generation",
                "original_source_owner_battlefield_incarnation",
            ),
            (
                "exact_recipient_generation_power_and_toughness_boost",
                "exact_recipient_incarnation_power_and_toughness_boost",
            ),
        ],
    );
    use crate::rules_meaning_fiendish_panda_v1 as panda;
    for root in [
        panda::RootV1::CreatureSpell,
        panda::RootV1::ControllerLifeGain,
        panda::RootV1::SourceDeath,
    ] {
        check(
            serde_json::to_value(
                panda::printed_meaning(id("Fiendish Panda").unwrap(), root).unwrap(),
            )
            .unwrap(),
            None,
            &[(
                "target_excludes_physical_source_id",
                "target_excludes_same_physical_source",
            )],
        );
    }
    use crate::rules_meaning_strongbox_v1 as strongbox;
    check(
        serde_json::to_value(
            strongbox::meaning(
                &strongbox::live_selector().unwrap(),
                id("Strongbox Raider").unwrap(),
                0,
            )
            .unwrap(),
        )
        .unwrap(),
        None,
        &[("exact_new_exile_generation", "exact_new_exile_incarnation")],
    );
    use crate::rules_meaning_ravenous_amulet_v51 as ravenous;
    for root in [ravenous::RootV51::DrawAndSoul, ravenous::RootV51::SoulLoss] {
        check(
            serde_json::to_value(
                ravenous::printed_meaning(id("Ravenous Amulet").unwrap(), root).unwrap(),
            )
            .unwrap(),
            None,
            &[(
                "loss_uses_original_generation_lki",
                "loss_uses_original_incarnation_lki",
            )],
        );
    }
}
