use mtgo_blackbox_v1::{
    check_untrusted_authorization_correspondence_v1, MtgoAuthorizationCorrespondenceReviewV1,
    MtgoCompetitiveEventKindV1, MTGO_AUTHORIZATION_CORRESPONDENCE_REVIEW_SCHEMA_V1,
};
use sha2::{Digest, Sha256};

const ACCOUNT_ALIAS: &str = "ApprovedMainAccount";
const CORRESPONDENCE: &[u8] =
    b"exact exported private correspondence bytes for structural testing\r\n";

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn review() -> MtgoAuthorizationCorrespondenceReviewV1 {
    MtgoAuthorizationCorrespondenceReviewV1 {
        schema_version: MTGO_AUTHORIZATION_CORRESPONDENCE_REVIEW_SCHEMA_V1,
        review_id: "daybreak-visible-automation-20260810-v1".to_owned(),
        correspondence_sha256: digest(CORRESPONDENCE),
        approved_account_alias_sha256: digest(ACCOUNT_ALIAS.as_bytes()),
        approved_competitive_modes: vec![
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ],
        visible_channels_only: true,
        hidden_information_access_prohibited: true,
        hidden_information_reverse_engineering_prohibited: true,
        cheating_or_hacking_prohibited: true,
        account_owner_event_entry_confirmation_required: true,
        account_owner_spending_confirmation_required: true,
        reviewer_alias_sha256: digest(b"local-owner-reviewer"),
        reviewed_at_utc: "2026-08-10T20:00:00Z".to_owned(),
    }
}

#[test]
fn exact_bytes_and_account_create_only_checked_untrusted_one_mode_scopes() {
    let checked =
        check_untrusted_authorization_correspondence_v1(review(), CORRESPONDENCE, ACCOUNT_ALIAS)
            .unwrap();
    assert_eq!(checked.correspondence_byte_length(), CORRESPONDENCE.len());
    assert_eq!(checked.correspondence_sha256(), digest(CORRESPONDENCE));
    assert_eq!(checked.review_commitment_sha256().len(), 64);
    assert_eq!(checked.approved_competitive_modes().len(), 2);
    assert!(!checked.safe_for_live_input());
    assert!(!checked.permits_event_entry());
    assert!(!checked.permits_spending());

    let league = checked
        .checked_untrusted_scope_for_mode_v1(MtgoCompetitiveEventKindV1::League)
        .unwrap();
    assert!(league.visible_channels_only);
    assert!(league.league_input);
    assert!(!league.challenge_input);
    assert!(!league.shadow_observation);
    assert!(!league.private_match_input);
    assert!(!league.open_play_input);
    assert!(!league.other_prize_event_input);
    assert_eq!(league.written_permission_sha256, digest(CORRESPONDENCE));

    let challenge = checked
        .checked_untrusted_scope_for_mode_v1(MtgoCompetitiveEventKindV1::Challenge)
        .unwrap();
    assert!(!challenge.league_input);
    assert!(challenge.challenge_input);
}

#[test]
fn changed_bytes_or_account_alias_reject() {
    assert_eq!(
        check_untrusted_authorization_correspondence_v1(
            review(),
            b"changed correspondence bytes",
            ACCOUNT_ALIAS,
        )
        .err()
        .unwrap()
        .code(),
        "authorization_correspondence_bytes_mismatch"
    );
    assert_eq!(
        check_untrusted_authorization_correspondence_v1(
            review(),
            CORRESPONDENCE,
            "DifferentAccount",
        )
        .err()
        .unwrap()
        .code(),
        "authorization_correspondence_account_mismatch"
    );
}

#[test]
fn safety_conditions_and_canonical_modes_are_required() {
    let mut hidden = review();
    hidden.visible_channels_only = false;
    assert_eq!(
        check_untrusted_authorization_correspondence_v1(hidden, CORRESPONDENCE, ACCOUNT_ALIAS,)
            .err()
            .unwrap()
            .code(),
        "authorization_correspondence_conditions"
    );

    for modes in [
        vec![],
        vec![
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::League,
        ],
        vec![
            MtgoCompetitiveEventKindV1::Challenge,
            MtgoCompetitiveEventKindV1::League,
        ],
    ] {
        let mut malformed = review();
        malformed.approved_competitive_modes = modes;
        assert_eq!(
            check_untrusted_authorization_correspondence_v1(
                malformed,
                CORRESPONDENCE,
                ACCOUNT_ALIAS,
            )
            .err()
            .unwrap()
            .code(),
            "authorization_correspondence_modes"
        );
    }
}

#[test]
fn unreviewed_mode_and_invalid_timestamp_reject() {
    let mut league_only = review();
    league_only.approved_competitive_modes = vec![MtgoCompetitiveEventKindV1::League];
    let checked =
        check_untrusted_authorization_correspondence_v1(league_only, CORRESPONDENCE, ACCOUNT_ALIAS)
            .unwrap();
    assert_eq!(
        checked
            .checked_untrusted_scope_for_mode_v1(MtgoCompetitiveEventKindV1::Challenge)
            .unwrap_err()
            .code(),
        "authorization_correspondence_mode_not_reviewed"
    );

    let mut bad_time = review();
    bad_time.reviewed_at_utc = "2026-02-31T20:00:00Z".to_owned();
    assert_eq!(
        check_untrusted_authorization_correspondence_v1(bad_time, CORRESPONDENCE, ACCOUNT_ALIAS,)
            .err()
            .unwrap()
            .code(),
        "authorization_correspondence_reviewed_at"
    );
}

#[test]
fn review_json_rejects_unknown_fields() {
    let mut value = serde_json::to_value(review()).unwrap();
    value["sender_was_daybreak"] = serde_json::json!(true);
    assert!(serde_json::from_value::<MtgoAuthorizationCorrespondenceReviewV1>(value).is_err());
}
