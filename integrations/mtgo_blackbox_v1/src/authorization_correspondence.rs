use crate::{
    MtgoAuthorizationScopeV1, MtgoCompetitiveEventKindV1, MtgoContractErrorV1,
    MTGO_AUTHORIZATION_SCHEMA_V1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MTGO_AUTHORIZATION_CORRESPONDENCE_REVIEW_SCHEMA_V1: u32 = 1;

const MAX_CORRESPONDENCE_BYTES_V1: usize = 4 * 1024 * 1024;
const AUTHORIZATION_CORRESPONDENCE_REVIEW_DOMAIN_V1: &[u8] =
    b"mtgo-authorization-correspondence-review-v1";

/// Human-reviewed claims about one exact private correspondence artifact.
///
/// The exact message bytes are not stored here. The checker recomputes their
/// SHA-256 and binds it to this review. These claims remain checked-untrusted:
/// code cannot prove that the review accurately paraphrases the message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoAuthorizationCorrespondenceReviewV1 {
    pub schema_version: u32,
    pub review_id: String,
    pub correspondence_sha256: String,
    pub approved_account_alias_sha256: String,
    pub approved_competitive_modes: Vec<MtgoCompetitiveEventKindV1>,
    pub visible_channels_only: bool,
    pub hidden_information_access_prohibited: bool,
    pub hidden_information_reverse_engineering_prohibited: bool,
    pub cheating_or_hacking_prohibited: bool,
    pub account_owner_event_entry_confirmation_required: bool,
    pub account_owner_spending_confirmation_required: bool,
    pub reviewer_alias_sha256: String,
    pub reviewed_at_utc: String,
}

/// Structurally checked review of exact correspondence bytes. This type is
/// not a trusted interpretation of the words and cannot ratify live input.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoAuthorizationCorrespondenceV1;
/// fn cannot_authorize(value: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1) {
///     let _ = value.ratify_live_input();
///     let _ = value.enter_event();
/// }
/// ```
pub struct CheckedUntrustedMtgoAuthorizationCorrespondenceV1 {
    review: MtgoAuthorizationCorrespondenceReviewV1,
    correspondence_byte_length: usize,
    review_commitment_sha256: String,
}

impl CheckedUntrustedMtgoAuthorizationCorrespondenceV1 {
    pub fn correspondence_sha256(&self) -> &str {
        &self.review.correspondence_sha256
    }

    pub fn approved_account_alias_sha256(&self) -> &str {
        &self.review.approved_account_alias_sha256
    }

    pub fn approved_competitive_modes(&self) -> &[MtgoCompetitiveEventKindV1] {
        &self.review.approved_competitive_modes
    }

    pub fn correspondence_byte_length(&self) -> usize {
        self.correspondence_byte_length
    }

    pub fn review_commitment_sha256(&self) -> &str {
        &self.review_commitment_sha256
    }

    /// Produces one coordinate-free, one-mode scope for later commitment
    /// calculation. The scope remains ordinary data and grants no input by
    /// itself. The Windows actuator still requires a separately compile-pinned
    /// general permission ratification.
    pub fn checked_untrusted_scope_for_mode_v1(
        &self,
        event_kind: MtgoCompetitiveEventKindV1,
    ) -> Result<MtgoAuthorizationScopeV1, MtgoContractErrorV1> {
        if !self.review.approved_competitive_modes.contains(&event_kind) {
            return Err(error_v1(
                "authorization_correspondence_mode_not_reviewed",
                "the reviewed correspondence does not include the selected mode",
            ));
        }
        Ok(MtgoAuthorizationScopeV1 {
            schema_version: MTGO_AUTHORIZATION_SCHEMA_V1,
            account_alias_sha256: self.review.approved_account_alias_sha256.clone(),
            written_permission_sha256: self.review.correspondence_sha256.clone(),
            visible_channels_only: true,
            league_input: event_kind == MtgoCompetitiveEventKindV1::League,
            challenge_input: event_kind == MtgoCompetitiveEventKindV1::Challenge,
            ..MtgoAuthorizationScopeV1::default()
        })
    }

    pub fn safe_for_live_input(&self) -> bool {
        false
    }

    pub fn permits_event_entry(&self) -> bool {
        false
    }

    pub fn permits_spending(&self) -> bool {
        false
    }
}

/// Recomputes the exact private correspondence and account-alias digests,
/// then checks that the human review declares the narrow visible-only safety
/// conditions. It deliberately does not parse or semantically interpret the
/// message text.
pub fn check_untrusted_authorization_correspondence_v1(
    review: MtgoAuthorizationCorrespondenceReviewV1,
    exact_correspondence_bytes: &[u8],
    visible_account_alias: &str,
) -> Result<CheckedUntrustedMtgoAuthorizationCorrespondenceV1, MtgoContractErrorV1> {
    if review.schema_version != MTGO_AUTHORIZATION_CORRESPONDENCE_REVIEW_SCHEMA_V1 {
        return Err(error_v1(
            "authorization_correspondence_schema",
            review.schema_version.to_string(),
        ));
    }
    validate_identifier_v1(&review.review_id, "authorization_correspondence_review_id")?;
    if exact_correspondence_bytes.is_empty()
        || exact_correspondence_bytes.len() > MAX_CORRESPONDENCE_BYTES_V1
    {
        return Err(error_v1(
            "authorization_correspondence_length",
            exact_correspondence_bytes.len().to_string(),
        ));
    }
    validate_account_alias_v1(visible_account_alias)?;
    for (value, code) in [
        (
            review.correspondence_sha256.as_str(),
            "authorization_correspondence_hash",
        ),
        (
            review.approved_account_alias_sha256.as_str(),
            "authorization_correspondence_account_hash",
        ),
        (
            review.reviewer_alias_sha256.as_str(),
            "authorization_correspondence_reviewer_hash",
        ),
    ] {
        validate_sha256_v1(value, code)?;
    }
    if sha256_hex_v1(exact_correspondence_bytes) != review.correspondence_sha256 {
        return Err(error_v1(
            "authorization_correspondence_bytes_mismatch",
            "the exact correspondence bytes do not match the reviewed digest",
        ));
    }
    if sha256_hex_v1(visible_account_alias.as_bytes()) != review.approved_account_alias_sha256 {
        return Err(error_v1(
            "authorization_correspondence_account_mismatch",
            "the visible account alias does not match the reviewed account",
        ));
    }
    validate_competitive_modes_v1(&review.approved_competitive_modes)?;
    if !review.visible_channels_only
        || !review.hidden_information_access_prohibited
        || !review.hidden_information_reverse_engineering_prohibited
        || !review.cheating_or_hacking_prohibited
        || !review.account_owner_event_entry_confirmation_required
        || !review.account_owner_spending_confirmation_required
    {
        return Err(error_v1(
            "authorization_correspondence_conditions",
            "all visible-only and owner-confirmation conditions must be retained",
        ));
    }
    validate_reviewed_at_utc_v1(&review.reviewed_at_utc)?;

    let review_json = serde_json::to_vec(&review).map_err(|error| {
        error_v1(
            "authorization_correspondence_serialization",
            error.to_string(),
        )
    })?;
    let byte_length = u64::try_from(exact_correspondence_bytes.len()).map_err(|_| {
        error_v1(
            "authorization_correspondence_length",
            "correspondence byte length does not fit u64",
        )
    })?;
    let review_commitment_sha256 = commitment_v1(
        AUTHORIZATION_CORRESPONDENCE_REVIEW_DOMAIN_V1,
        &[
            review_json.as_slice(),
            byte_length.to_be_bytes().as_slice(),
            visible_account_alias.as_bytes(),
            b"checked_untrusted_human_review_no_live_input_or_event_entry_authority",
        ],
    );
    Ok(CheckedUntrustedMtgoAuthorizationCorrespondenceV1 {
        review,
        correspondence_byte_length: exact_correspondence_bytes.len(),
        review_commitment_sha256,
    })
}

fn validate_competitive_modes_v1(
    modes: &[MtgoCompetitiveEventKindV1],
) -> Result<(), MtgoContractErrorV1> {
    if modes.is_empty() || modes.len() > 2 {
        return Err(error_v1(
            "authorization_correspondence_modes",
            "one or two competitive modes must be reviewed",
        ));
    }
    for pair in modes.windows(2) {
        let ordered = matches!(
            pair,
            [
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEventKindV1::Challenge
            ]
        );
        if !ordered {
            return Err(error_v1(
                "authorization_correspondence_modes",
                "competitive modes must be unique and ordered League then Challenge",
            ));
        }
    }
    Ok(())
}

fn validate_account_alias_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty() || value.len() > 64 || value.chars().any(char::is_control) {
        return Err(error_v1(
            "authorization_correspondence_account_alias",
            "account alias must be nonempty, bounded, and contain no control characters",
        ));
    }
    Ok(())
}

fn validate_identifier_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(error_v1(code, "identifier syntax is invalid"));
    }
    Ok(())
}

fn validate_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(code, "expected lowercase SHA-256 hex"));
    }
    Ok(())
}

fn validate_reviewed_at_utc_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
        || bytes.iter().enumerate().any(|(index, byte)| {
            !matches!(index, 4 | 7 | 10 | 13 | 16 | 19) && !byte.is_ascii_digit()
        })
    {
        return Err(error_v1(
            "authorization_correspondence_reviewed_at",
            "review timestamp must use YYYY-MM-DDTHH:MM:SSZ",
        ));
    }
    let parse = |range: std::ops::Range<usize>| -> u32 {
        bytes[range]
            .iter()
            .fold(0_u32, |value, byte| value * 10 + u32::from(byte - b'0'))
    };
    let year = parse(0..4);
    let month = parse(5..7);
    let day = parse(8..10);
    let hour = parse(11..13);
    let minute = parse(14..16);
    let second = parse(17..19);
    let leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => 0,
    };
    if year < 2000 || day == 0 || day > days_in_month || hour > 23 || minute > 59 || second > 59 {
        return Err(error_v1(
            "authorization_correspondence_reviewed_at",
            "review timestamp is not a valid UTC date and time",
        ));
    }
    Ok(())
}

fn sha256_hex_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}
