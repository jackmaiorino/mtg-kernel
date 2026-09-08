use crate::{
    check_untrusted_competitive_pregame_public_context_v1,
    AdmittedMtgoCompetitivePregamePublicContextProfileV1,
    CheckedUntrustedMtgoCompetitivePregameClassificationV1,
    CheckedUntrustedMtgoCompetitivePregamePublicContextV1, MtgoCompetitivePregamePlayDrawV1,
    MtgoCompetitivePregamePublicContextCandidateV1, MtgoContractErrorV1, MtgoSizePxV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CLASSIFIER_SCHEMA_V1: u32 = 1;
pub const MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CLASSIFIER_PROTOCOL_V1: &str =
    "mtgo_visible_competitive_pregame_public_context_v1";

const PUBLIC_CONTEXT_CLASSIFIER_REQUEST_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-pregame-public-context-classifier-request-v1";
const PUBLIC_CONTEXT_CLASSIFIER_RESULT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-pregame-public-context-classifier-result-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregamePublicContextClassifierRequestHeaderV1 {
    pub schema_version: u32,
    pub protocol: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub canonical_width: u32,
    pub canonical_height: u32,
    pub canonical_stride: u32,
    pub canonical_byte_length: usize,
    pub canonical_bgra8_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_frame_profile_binding_sha256: String,
    pub pregame_classification_commitment_sha256: String,
    pub visible_interaction_commitment_sha256: String,
    pub pregame_evaluation_commitment_sha256: String,
    pub pregame_profile_admission_commitment_sha256: String,
    pub public_context_evaluation_commitment_sha256: String,
    pub public_context_profile_admission_commitment_sha256: String,
    pub classifier_binary_sha256: String,
    pub classifier_runtime_identity_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregamePublicContextClassifierResponseV1 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub public_context: MtgoCompetitivePregamePublicContextCandidateV1,
}

pub struct CheckedUntrustedMtgoCompetitivePregamePublicContextClassifierRequestV1 {
    header: MtgoCompetitivePregamePublicContextClassifierRequestHeaderV1,
    request_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitivePregamePublicContextClassifierRequestV1 {
    pub fn request_commitment_sha256(&self) -> &str {
        &self.request_commitment_sha256
    }

    pub fn frame_id(&self) -> u64 {
        self.header.frame_id
    }

    pub fn frame_sequence(&self) -> u64 {
        self.header.frame_sequence
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }
}

/// Pixel-checked public pregame context produced for the exact already
/// classified frame and exact admitted evaluation profile. It remains
/// non-actionable until the Windows runtime separately proves it invoked the
/// evaluated binary and binds this result back into the move-only source.
pub struct CheckedUntrustedMtgoCompetitivePregamePublicContextClassificationV1 {
    public_context: CheckedUntrustedMtgoCompetitivePregamePublicContextV1,
    request_commitment_sha256: String,
    result_commitment_sha256: String,
    public_context_commitment_sha256: String,
    public_context_evaluation_commitment_sha256: String,
    public_context_profile_admission_commitment_sha256: String,
    classifier_binary_sha256: String,
    classifier_runtime_identity_commitment_sha256: String,
    frame_id: u64,
    frame_sequence: u64,
    game_number: u8,
    play_draw: MtgoCompetitivePregamePlayDrawV1,
    acting_player_games_won: u8,
    opponent_games_won: u8,
}

impl CheckedUntrustedMtgoCompetitivePregamePublicContextClassificationV1 {
    pub fn request_commitment_sha256(&self) -> &str {
        &self.request_commitment_sha256
    }

    pub fn result_commitment_sha256(&self) -> &str {
        &self.result_commitment_sha256
    }

    pub fn public_context_commitment_sha256(&self) -> &str {
        &self.public_context_commitment_sha256
    }

    pub fn public_context_evaluation_commitment_sha256(&self) -> &str {
        &self.public_context_evaluation_commitment_sha256
    }

    pub fn public_context_profile_admission_commitment_sha256(&self) -> &str {
        &self.public_context_profile_admission_commitment_sha256
    }

    pub fn classifier_binary_sha256(&self) -> &str {
        &self.classifier_binary_sha256
    }

    pub fn classifier_runtime_identity_commitment_sha256(&self) -> &str {
        &self.classifier_runtime_identity_commitment_sha256
    }

    pub fn frame_id(&self) -> u64 {
        self.frame_id
    }

    pub fn frame_sequence(&self) -> u64 {
        self.frame_sequence
    }

    pub fn game_number(&self) -> u8 {
        self.game_number
    }

    pub fn play_draw(&self) -> MtgoCompetitivePregamePlayDrawV1 {
        self.play_draw
    }

    pub fn match_score(&self) -> (u8, u8) {
        (self.acting_player_games_won, self.opponent_games_won)
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn into_public_context_v1(self) -> CheckedUntrustedMtgoCompetitivePregamePublicContextV1 {
        self.public_context
    }
}

pub fn check_untrusted_competitive_pregame_public_context_classifier_request_v1(
    classification: &CheckedUntrustedMtgoCompetitivePregameClassificationV1,
    profile: &AdmittedMtgoCompetitivePregamePublicContextProfileV1,
    canonical_header_json: &[u8],
    canonical_bgra8: &[u8],
) -> Result<
    CheckedUntrustedMtgoCompetitivePregamePublicContextClassifierRequestV1,
    MtgoContractErrorV1,
> {
    let header: MtgoCompetitivePregamePublicContextClassifierRequestHeaderV1 =
        parse_canonical_json_v1(canonical_header_json, "request")?;
    validate_header_v1(&header, classification, profile, canonical_bgra8)?;
    Ok(
        CheckedUntrustedMtgoCompetitivePregamePublicContextClassifierRequestV1 {
            header,
            request_commitment_sha256: commitment_v1(
                PUBLIC_CONTEXT_CLASSIFIER_REQUEST_DOMAIN_V1,
                &[canonical_header_json, canonical_bgra8],
            ),
        },
    )
}

pub fn check_untrusted_competitive_pregame_public_context_classifier_response_v1(
    profile: &AdmittedMtgoCompetitivePregamePublicContextProfileV1,
    request: &CheckedUntrustedMtgoCompetitivePregamePublicContextClassifierRequestV1,
    canonical_bgra8: &[u8],
    canonical_response_json: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitivePregamePublicContextClassificationV1, MtgoContractErrorV1>
{
    if request.header.canonical_byte_length != canonical_bgra8.len()
        || request.header.canonical_bgra8_sha256 != sha256_hex_v1(canonical_bgra8)
        || request.header.public_context_evaluation_commitment_sha256
            != profile.evaluation_commitment_sha256()
        || request
            .header
            .public_context_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
        || request.header.classifier_binary_sha256 != profile.classifier_binary_sha256()
    {
        return Err(error_v1(
            "competitive_pregame_public_context_classifier_response_source_mismatch",
            "response source differs from the checked request or admitted context profile",
        ));
    }
    let response: MtgoCompetitivePregamePublicContextClassifierResponseV1 =
        parse_canonical_json_v1(canonical_response_json, "response")?;
    if response.schema_version != MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CLASSIFIER_SCHEMA_V1
        || response.request_commitment_sha256 != request.request_commitment_sha256
    {
        return Err(error_v1(
            "competitive_pregame_public_context_classifier_response_invalid",
            "response schema or request binding is invalid",
        ));
    }
    let context = &response.public_context;
    if context.source_capture_commitment_sha256 != request.header.source_capture_commitment_sha256
        || context.source_frame_profile_binding_sha256
            != request.header.source_frame_profile_binding_sha256
        || context.pregame_classification_commitment_sha256
            != request.header.pregame_classification_commitment_sha256
        || context.visible_interaction_commitment_sha256
            != request.header.visible_interaction_commitment_sha256
        || context.frame_id != request.header.frame_id
        || context.frame_sequence != request.header.frame_sequence
    {
        return Err(error_v1(
            "competitive_pregame_public_context_classifier_lineage_mismatch",
            "public context does not describe the exact checked pregame frame",
        ));
    }
    let checked_context = check_untrusted_competitive_pregame_public_context_v1(
        response.public_context,
        canonical_bgra8,
        MtgoSizePxV1 {
            width: request.header.canonical_width,
            height: request.header.canonical_height,
        },
    )?;
    let public_context_commitment_sha256 = checked_context
        .public_context_commitment_sha256()
        .to_owned();
    let game_number = checked_context.game_number();
    let play_draw = checked_context.play_draw();
    let acting_player_games_won = checked_context.acting_player_games_won();
    let opponent_games_won = checked_context.opponent_games_won();
    let result_commitment_sha256 = commitment_v1(
        PUBLIC_CONTEXT_CLASSIFIER_RESULT_DOMAIN_V1,
        &[
            request.request_commitment_sha256.as_bytes(),
            canonical_response_json,
            profile.evaluation_commitment_sha256().as_bytes(),
            profile.admission_commitment_sha256().as_bytes(),
            profile.classifier_binary_sha256().as_bytes(),
            request
                .header
                .classifier_runtime_identity_commitment_sha256
                .as_bytes(),
            b"pixel_checked_exact_frame_context_no_model_or_input_authority",
        ],
    );
    Ok(
        CheckedUntrustedMtgoCompetitivePregamePublicContextClassificationV1 {
            public_context: checked_context,
            request_commitment_sha256: request.request_commitment_sha256.clone(),
            result_commitment_sha256,
            public_context_commitment_sha256,
            public_context_evaluation_commitment_sha256: profile
                .evaluation_commitment_sha256()
                .to_owned(),
            public_context_profile_admission_commitment_sha256: profile
                .admission_commitment_sha256()
                .to_owned(),
            classifier_binary_sha256: profile.classifier_binary_sha256().to_owned(),
            classifier_runtime_identity_commitment_sha256: request
                .header
                .classifier_runtime_identity_commitment_sha256
                .clone(),
            frame_id: request.header.frame_id,
            frame_sequence: request.header.frame_sequence,
            game_number,
            play_draw,
            acting_player_games_won,
            opponent_games_won,
        },
    )
}

fn validate_header_v1(
    header: &MtgoCompetitivePregamePublicContextClassifierRequestHeaderV1,
    classification: &CheckedUntrustedMtgoCompetitivePregameClassificationV1,
    profile: &AdmittedMtgoCompetitivePregamePublicContextProfileV1,
    canonical_bgra8: &[u8],
) -> Result<(), MtgoContractErrorV1> {
    let expected_stride = header.canonical_width.checked_mul(4).ok_or_else(|| {
        error_v1(
            "competitive_pregame_public_context_classifier_geometry_invalid",
            "stride overflow",
        )
    })?;
    let expected_length = usize::try_from(header.canonical_width)
        .ok()
        .and_then(|width| {
            usize::try_from(header.canonical_height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            error_v1(
                "competitive_pregame_public_context_classifier_geometry_invalid",
                "pixel length overflow",
            )
        })?;
    if header.schema_version != MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CLASSIFIER_SCHEMA_V1
        || header.protocol != MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CLASSIFIER_PROTOCOL_V1
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
        || header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || canonical_bgra8.len() != expected_length
        || header.canonical_bgra8_sha256 != sha256_hex_v1(canonical_bgra8)
    {
        return Err(error_v1(
            "competitive_pregame_public_context_classifier_request_invalid",
            "request identity, geometry, or pixels are invalid",
        ));
    }
    for digest in [
        header.canonical_bgra8_sha256.as_str(),
        header.source_capture_commitment_sha256.as_str(),
        header.source_frame_profile_binding_sha256.as_str(),
        header.pregame_classification_commitment_sha256.as_str(),
        header.visible_interaction_commitment_sha256.as_str(),
        header.pregame_evaluation_commitment_sha256.as_str(),
        header.pregame_profile_admission_commitment_sha256.as_str(),
        header.public_context_evaluation_commitment_sha256.as_str(),
        header
            .public_context_profile_admission_commitment_sha256
            .as_str(),
        header.classifier_binary_sha256.as_str(),
        header
            .classifier_runtime_identity_commitment_sha256
            .as_str(),
    ] {
        validate_lower_sha256_v1(digest)?;
    }
    if header.source_capture_commitment_sha256 != classification.source_capture_commitment_sha256()
        || header.source_frame_profile_binding_sha256
            != classification.source_frame_profile_binding_sha256()
        || header.pregame_classification_commitment_sha256
            != classification.classification_commitment_sha256()
        || header.visible_interaction_commitment_sha256
            != classification.visible_interaction_commitment_sha256()
        || header.pregame_evaluation_commitment_sha256
            != classification.pregame_evaluation_commitment_sha256()
        || header.pregame_profile_admission_commitment_sha256
            != classification.pregame_profile_admission_commitment_sha256()
        || header.frame_id != classification.frame_id()
        || header.frame_sequence != classification.frame_sequence()
        || header.classifier_runtime_identity_commitment_sha256
            != classification.classifier_runtime_identity_commitment_sha256()
        || profile.pregame_evaluation_commitment_sha256()
            != classification.pregame_evaluation_commitment_sha256()
        || profile.pregame_profile_admission_commitment_sha256()
            != classification.pregame_profile_admission_commitment_sha256()
        || header.public_context_evaluation_commitment_sha256
            != profile.evaluation_commitment_sha256()
        || header.public_context_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
        || header.classifier_binary_sha256 != profile.classifier_binary_sha256()
    {
        return Err(error_v1(
            "competitive_pregame_public_context_classifier_profile_mismatch",
            "request does not bind the exact pregame classification and context admission",
        ));
    }
    Ok(())
}

fn parse_canonical_json_v1<T>(bytes: &[u8], label: &'static str) -> Result<T, MtgoContractErrorV1>
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    if bytes.is_empty() || bytes.len() > 1_048_576 {
        return Err(error_v1(
            "competitive_pregame_public_context_classifier_json_invalid",
            label,
        ));
    }
    let value: T = serde_json::from_slice(bytes).map_err(|error| {
        error_v1(
            "competitive_pregame_public_context_classifier_json_invalid",
            format!("{label}: {error}"),
        )
    })?;
    if serde_json::to_vec(&value).map_err(|error| {
        error_v1(
            "competitive_pregame_public_context_classifier_json_invalid",
            format!("{label}: {error}"),
        )
    })? != bytes
    {
        return Err(error_v1(
            "competitive_pregame_public_context_classifier_json_noncanonical",
            label,
        ));
    }
    Ok(value)
}

fn validate_lower_sha256_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(
            "competitive_pregame_public_context_classifier_sha256_invalid",
            value,
        ));
    }
    Ok(())
}

fn sha256_hex_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_be_bytes());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        checked_untrusted_competitive_pregame_classification_for_context_test_v1,
        competitive_pregame_public_context_commitment_v1,
        competitive_pregame_public_context_profile_admitted_for_test_v1,
        visible_frame_region_content_sha256_v1, MtgoCompetitivePregamePublicContextFactKindV1,
        MtgoCompetitivePregamePublicContextFactV1, MtgoRectPxV1,
    };

    fn pixels_v1() -> Vec<u8> {
        (0_u16..256).map(|value| value as u8).collect()
    }

    fn classification_v1() -> CheckedUntrustedMtgoCompetitivePregameClassificationV1 {
        checked_untrusted_competitive_pregame_classification_for_context_test_v1(
            "1".repeat(64),
            "2".repeat(64),
            "3".repeat(64),
            "4".repeat(64),
            10,
            20,
        )
    }

    fn profile_v1() -> AdmittedMtgoCompetitivePregamePublicContextProfileV1 {
        competitive_pregame_public_context_profile_admitted_for_test_v1(
            &"f".repeat(64),
            &"1".repeat(64),
            "5".repeat(64),
        )
    }

    fn header_v1(
        classification: &CheckedUntrustedMtgoCompetitivePregameClassificationV1,
        profile: &AdmittedMtgoCompetitivePregamePublicContextProfileV1,
        pixels: &[u8],
    ) -> MtgoCompetitivePregamePublicContextClassifierRequestHeaderV1 {
        MtgoCompetitivePregamePublicContextClassifierRequestHeaderV1 {
            schema_version: MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CLASSIFIER_SCHEMA_V1,
            protocol: MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CLASSIFIER_PROTOCOL_V1.to_owned(),
            frame_id: classification.frame_id(),
            frame_sequence: classification.frame_sequence(),
            canonical_width: 8,
            canonical_height: 8,
            canonical_stride: 32,
            canonical_byte_length: pixels.len(),
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_capture_commitment_sha256: classification
                .source_capture_commitment_sha256()
                .to_owned(),
            source_frame_profile_binding_sha256: classification
                .source_frame_profile_binding_sha256()
                .to_owned(),
            pregame_classification_commitment_sha256: classification
                .classification_commitment_sha256()
                .to_owned(),
            visible_interaction_commitment_sha256: classification
                .visible_interaction_commitment_sha256()
                .to_owned(),
            pregame_evaluation_commitment_sha256: classification
                .pregame_evaluation_commitment_sha256()
                .to_owned(),
            pregame_profile_admission_commitment_sha256: classification
                .pregame_profile_admission_commitment_sha256()
                .to_owned(),
            public_context_evaluation_commitment_sha256: profile
                .evaluation_commitment_sha256()
                .to_owned(),
            public_context_profile_admission_commitment_sha256: profile
                .admission_commitment_sha256()
                .to_owned(),
            classifier_binary_sha256: profile.classifier_binary_sha256().to_owned(),
            classifier_runtime_identity_commitment_sha256: classification
                .classifier_runtime_identity_commitment_sha256()
                .to_owned(),
        }
    }

    fn response_v1(
        header_json: &[u8],
        pixels: &[u8],
    ) -> MtgoCompetitivePregamePublicContextClassifierResponseV1 {
        let size = MtgoSizePxV1 {
            width: 8,
            height: 8,
        };
        let rects = [
            MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            MtgoRectPxV1 {
                x: 3,
                y: 0,
                width: 2,
                height: 2,
            },
            MtgoRectPxV1 {
                x: 6,
                y: 0,
                width: 2,
                height: 2,
            },
        ];
        let mut public_context = MtgoCompetitivePregamePublicContextCandidateV1 {
            schema_version: 1,
            source_capture_commitment_sha256: "1".repeat(64),
            source_frame_profile_binding_sha256: "2".repeat(64),
            pregame_classification_commitment_sha256: "3".repeat(64),
            visible_interaction_commitment_sha256: "4".repeat(64),
            frame_id: 10,
            frame_sequence: 20,
            game_number: 2,
            play_draw: MtgoCompetitivePregamePlayDrawV1::OnDraw,
            acting_player_games_won: 1,
            opponent_games_won: 0,
            visible_facts: [
                MtgoCompetitivePregamePublicContextFactKindV1::PlayDrawIndicator,
                MtgoCompetitivePregamePublicContextFactKindV1::ActingPlayerMatchScore,
                MtgoCompetitivePregamePublicContextFactKindV1::OpponentMatchScore,
            ]
            .into_iter()
            .zip(rects)
            .map(
                |(kind, rect_client_px)| MtgoCompetitivePregamePublicContextFactV1 {
                    kind,
                    visible_content_sha256: visible_frame_region_content_sha256_v1(
                        pixels,
                        &size,
                        &rect_client_px,
                    )
                    .unwrap(),
                    rect_client_px,
                    confidence_bps: 10_000,
                },
            )
            .collect(),
            public_context_commitment_sha256: String::new(),
        };
        public_context.public_context_commitment_sha256 =
            competitive_pregame_public_context_commitment_v1(&public_context).unwrap();
        MtgoCompetitivePregamePublicContextClassifierResponseV1 {
            schema_version: MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CLASSIFIER_SCHEMA_V1,
            request_commitment_sha256: commitment_v1(
                PUBLIC_CONTEXT_CLASSIFIER_REQUEST_DOMAIN_V1,
                &[header_json, pixels],
            ),
            public_context,
        }
    }

    #[test]
    fn exact_same_frame_context_exchange_is_checked_but_non_actionable() {
        let pixels = pixels_v1();
        let classification = classification_v1();
        let profile = profile_v1();
        let header_json =
            serde_json::to_vec(&header_v1(&classification, &profile, &pixels)).unwrap();
        let request = check_untrusted_competitive_pregame_public_context_classifier_request_v1(
            &classification,
            &profile,
            &header_json,
            &pixels,
        )
        .unwrap();
        let response_json = serde_json::to_vec(&response_v1(&header_json, &pixels)).unwrap();
        let checked = check_untrusted_competitive_pregame_public_context_classifier_response_v1(
            &profile,
            &request,
            &pixels,
            &response_json,
        )
        .unwrap();
        assert_eq!(checked.frame_id(), 10);
        assert_eq!(checked.game_number(), 2);
        assert_eq!(
            checked.play_draw(),
            MtgoCompetitivePregamePlayDrawV1::OnDraw
        );
        assert_eq!(checked.match_score(), (1, 0));
        assert!(!checked.safe_for_model_scoring_v1());
        assert!(!checked.safe_for_input_v1());
    }

    #[test]
    fn crossed_classification_profile_or_runtime_identity_rejects() {
        let pixels = pixels_v1();
        let classification = classification_v1();
        let profile = profile_v1();
        let mut header = header_v1(&classification, &profile, &pixels);
        for mutation in 0..3 {
            let mut crossed = header.clone();
            match mutation {
                0 => crossed.pregame_classification_commitment_sha256 = "9".repeat(64),
                1 => crossed.public_context_profile_admission_commitment_sha256 = "9".repeat(64),
                _ => crossed.classifier_runtime_identity_commitment_sha256 = "9".repeat(64),
            }
            assert!(
                check_untrusted_competitive_pregame_public_context_classifier_request_v1(
                    &classification,
                    &profile,
                    &serde_json::to_vec(&crossed).unwrap(),
                    &pixels,
                )
                .is_err()
            );
        }
        header.canonical_bgra8_sha256 = "9".repeat(64);
        assert!(
            check_untrusted_competitive_pregame_public_context_classifier_request_v1(
                &classification,
                &profile,
                &serde_json::to_vec(&header).unwrap(),
                &pixels,
            )
            .is_err()
        );
    }

    #[test]
    fn response_lineage_pixels_and_canonical_json_are_fail_closed() {
        let pixels = pixels_v1();
        let classification = classification_v1();
        let profile = profile_v1();
        let header_json =
            serde_json::to_vec(&header_v1(&classification, &profile, &pixels)).unwrap();
        let request = check_untrusted_competitive_pregame_public_context_classifier_request_v1(
            &classification,
            &profile,
            &header_json,
            &pixels,
        )
        .unwrap();
        let mut response = response_v1(&header_json, &pixels);
        response.public_context.frame_sequence += 1;
        response.public_context.public_context_commitment_sha256 =
            competitive_pregame_public_context_commitment_v1(&response.public_context).unwrap();
        assert!(
            check_untrusted_competitive_pregame_public_context_classifier_response_v1(
                &profile,
                &request,
                &pixels,
                &serde_json::to_vec(&response).unwrap(),
            )
            .is_err()
        );

        let response = response_v1(&header_json, &pixels);
        let pretty = serde_json::to_string_pretty(&response).unwrap();
        assert!(
            check_untrusted_competitive_pregame_public_context_classifier_response_v1(
                &profile,
                &request,
                &pixels,
                pretty.as_bytes(),
            )
            .is_err()
        );

        let mut changed_pixels = pixels;
        changed_pixels[0] ^= 1;
        assert!(
            check_untrusted_competitive_pregame_public_context_classifier_response_v1(
                &profile,
                &request,
                &changed_pixels,
                &serde_json::to_vec(&response).unwrap(),
            )
            .is_err()
        );
    }
}
