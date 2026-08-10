use mtg_kernel::card_def::KERNEL_CARDDB_HASH;
use mtgo_blackbox_v1::{
    resolve_checked_untrusted_kernel_card_correspondence_v1,
    MtgoKernelCardCorrespondenceDispositionV1,
};
use serde_json::Value;
use std::collections::HashSet;

const SAMPLE: &str =
    include_str!("../fixtures/offline_kernel_basics_opening_hand_coverage_20260810_v1.json");

#[test]
fn kernel_basics_opening_hand_has_complete_exact_name_coverage() {
    let sample: Value = serde_json::from_str(SAMPLE).unwrap();
    assert_eq!(sample["schema_version"], 1);
    assert_eq!(sample["deck"]["total_cards"], 60);
    assert_eq!(sample["classification"], "match");
    assert_eq!(sample["prospective_keep_size"], 7);
    assert_eq!(sample["matched_identity_count"], 7);
    assert_eq!(sample["fully_supported_kernel_name_count"], 7);
    assert_eq!(
        sample["kernel_card_db_hash_hex"],
        format!("{KERNEL_CARDDB_HASH:016x}")
    );

    let identities = sample["identities"].as_array().unwrap();
    assert_eq!(identities.len(), 7);
    assert_eq!(
        identities
            .iter()
            .map(|identity| identity["visible_card_name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Forest", "Island", "Forest", "Forest", "Forest", "Forest", "Island"]
    );
    let print_ids: HashSet<_> = identities
        .iter()
        .map(|identity| identity["public_print_id"].as_str().unwrap())
        .collect();
    assert_eq!(print_ids.len(), 4);

    for (expected_ordinal, identity) in identities.iter().enumerate() {
        assert_eq!(identity["ordinal"], expected_ordinal as u64);
        assert!(identity["mean_absolute_difference_milli"].as_u64().unwrap() <= 25_000);
        assert!(identity["distinct_name_margin_milli"].as_u64().unwrap() >= 10_000);
        let correspondence = resolve_checked_untrusted_kernel_card_correspondence_v1(
            identity["visible_card_name"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(
            correspondence.disposition(),
            MtgoKernelCardCorrespondenceDispositionV1::FullySupportedDeckCard
        );
        assert!(!correspondence.safe_for_object_binding());
    }
}

#[test]
fn kernel_basics_opening_hand_grants_no_runtime_authority() {
    let sample: Value = serde_json::from_str(SAMPLE).unwrap();
    assert_eq!(
        sample["template_profile"]["status"],
        "checked_untrusted_calibration_not_ratified"
    );
    assert!(sample["nonclaim"]
        .as_str()
        .unwrap()
        .contains("not a heldout accuracy estimate"));
    for field in [
        "safe_for_object_binding",
        "safe_for_observation_v5",
        "safe_for_policy_scoring",
        "safe_for_input",
    ] {
        assert_eq!(sample[field], false, "authority must remain false: {field}");
    }
}
