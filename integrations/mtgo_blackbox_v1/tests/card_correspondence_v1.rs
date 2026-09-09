use mtg_kernel::card_def::{card_id_by_name, CardCapability, CARD_DEFS, KERNEL_CARDDB_HASH};
use mtgo_blackbox_v1::{
    mtgo_kernel_supported_card_profile_commitment_v1,
    resolve_checked_untrusted_kernel_card_correspondence_v1,
    MtgoKernelCardCorrespondenceDispositionV1,
};

#[test]
fn exact_fully_supported_card_name_resolves_to_compile_bound_id() {
    let checked = resolve_checked_untrusted_kernel_card_correspondence_v1("Island").unwrap();
    assert_eq!(checked.visible_card_name(), "Island");
    assert_eq!(checked.card_db_id(), card_id_by_name("Island").unwrap());
    assert_eq!(checked.kernel_card_db_hash(), KERNEL_CARDDB_HASH);
    assert_eq!(
        checked.disposition(),
        MtgoKernelCardCorrespondenceDispositionV1::FullySupportedDeckCard
    );
    assert_eq!(checked.supported_profile_commitment_sha256().len(), 64);
    assert_eq!(checked.correspondence_commitment_sha256().len(), 64);
    assert!(!checked.safe_for_object_binding());
    assert!(!checked.safe_for_observation_v5());
    assert!(!checked.safe_for_policy_scoring());
    assert!(!checked.safe_for_input());
}

#[test]
fn unknown_and_inexact_names_fail_closed() {
    for (name, expected_code) in [
        ("Not A Real Card", "kernel_card_name_unknown"),
        // Every registry card is CardCapability::Full in the frozen kernel, so no registered
        // name can exercise "kernel_card_not_fully_supported" here either; that branch is
        // covered again once a partial card lands.
        ("Not A Kernel Card", "kernel_card_name_unknown"),
        ("island", "kernel_card_name_unknown"),
        (" Island", "visible_card_name_invalid"),
        ("Island ", "visible_card_name_invalid"),
        ("Island\n", "visible_card_name_invalid"),
        ("", "visible_card_name_invalid"),
    ] {
        assert_eq!(
            resolve_checked_untrusted_kernel_card_correspondence_v1(name)
                .err()
                .unwrap()
                .code(),
            expected_code,
            "name={name:?}"
        );
    }
}

#[test]
fn supported_token_is_distinct_from_a_deck_card() {
    let checked = resolve_checked_untrusted_kernel_card_correspondence_v1("Blood Token").unwrap();
    assert_eq!(
        checked.disposition(),
        MtgoKernelCardCorrespondenceDispositionV1::FullySupportedToken
    );
}

#[test]
fn supported_profile_commitment_is_deterministic_and_source_bound() {
    assert_eq!(CARD_DEFS.len(), 136);
    assert_eq!(
        CARD_DEFS
            .iter()
            .filter(|definition| definition.capability == CardCapability::Full)
            .count(),
        49
    );
    let first = mtgo_kernel_supported_card_profile_commitment_v1().unwrap();
    let second = mtgo_kernel_supported_card_profile_commitment_v1().unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first,
        "42d3820feae9c37be9703ec12372de4fea1bd86e2811123266a39156af634839"
    );
}
