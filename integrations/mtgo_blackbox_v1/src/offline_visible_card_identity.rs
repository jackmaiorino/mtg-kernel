use crate::{
    classify_untrusted_offline_bottom_six_state_candidate_v1,
    classify_untrusted_offline_bottom_six_state_candidate_v2,
    classify_untrusted_offline_bottom_six_state_candidate_v3,
    classify_untrusted_offline_mulligan_ladder_candidate_v2,
    offline_bottom_six_reflow::card_art_regions_v1, CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    MtgoContractErrorV1, MtgoOfflineBottomSixStateClassificationV1,
    MtgoOfflineBottomSixStateClassificationV2, MtgoOfflineBottomSixStateClassificationV3,
    MtgoOfflineMulliganLadderClassificationV1, MtgoRectPxV1, MtgoSizePxV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

const PROFILE_SCHEMA_V1: u32 = 1;
const MATCHER_VERSION_V1: &str = "mean_absolute_bgr96x60-v1";
const ART_WIDTH_V1: u32 = 96;
const ART_HEIGHT_V1: u32 = 60;
const TEMPLATE_CHANNELS_V1: usize = 3;
const MAX_DISTANCE_MILLI_V1: u32 = 25_000;
const MIN_DISTINCT_NAME_MARGIN_MILLI_V1: u32 = 10_000;
const PROFILE_DOMAIN_V1: &[u8] = b"mtgo-offline-visible-card-template-profile-v1";
const CANDIDATE_DOMAIN_V1: &[u8] = b"mtgo-offline-visible-card-identity-candidate-v1";
const CANDIDATE_DOMAIN_V2: &[u8] = b"mtgo-offline-visible-card-identity-candidate-v2";
const CANDIDATE_DOMAIN_V3: &[u8] = b"mtgo-offline-visible-card-identity-candidate-v3";
const MULLIGAN_CANDIDATE_DOMAIN_V1: &[u8] =
    b"mtgo-offline-mulligan-visible-card-identity-candidate-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoOfflineVisibleCardTemplateV1 {
    pub template_id: String,
    pub visible_card_name: String,
    pub public_print_id: String,
    pub public_oracle_id: String,
    pub set_code: String,
    pub collector_number: String,
    pub public_reference_image_sha256: String,
    pub source_capture_manifest_sha256: String,
    pub reference_bgr8_sha256: String,
    pub reference_bgr8: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoOfflineVisibleCardTemplateProfileV1 {
    pub schema_version: u32,
    pub profile_id: String,
    pub matcher_version: String,
    pub deck_manifest_sha256: String,
    pub output_identity_sha256: String,
    pub client_size_px: MtgoSizePxV1,
    pub maximum_mean_absolute_difference_milli: u32,
    pub minimum_distinct_name_margin_milli: u32,
    pub templates: Vec<MtgoOfflineVisibleCardTemplateV1>,
}

/// Structurally checked caller-supplied deck templates.
///
/// The caller controls the labels and reference bytes. This type is therefore
/// checked-untrusted and grants no semantic, observation, scoring, or input
/// authority. It intentionally exposes no template pixels.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1;
/// fn pixel_escape(value: &CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1) {
///     let _ = value.templates_v1();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1;
/// fn require_debug<T: core::fmt::Debug>() {}
/// require_debug::<CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1>();
/// ```
pub struct CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1 {
    profile: MtgoOfflineVisibleCardTemplateProfileV1,
    profile_commitment_sha256: String,
    distinct_visible_card_name_count: usize,
}

impl CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1 {
    pub fn profile_commitment_sha256(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn deck_manifest_sha256(&self) -> &str {
        &self.profile.deck_manifest_sha256
    }

    pub fn template_count(&self) -> usize {
        self.profile.templates.len()
    }

    pub fn distinct_visible_card_name_count(&self) -> usize {
        self.distinct_visible_card_name_count
    }

    pub fn maximum_mean_absolute_difference_milli(&self) -> u32 {
        self.profile.maximum_mean_absolute_difference_milli
    }

    pub fn minimum_distinct_name_margin_milli(&self) -> u32 {
        self.profile.minimum_distinct_name_margin_milli
    }

    pub fn safe_for_semantic_evidence(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoOfflineVisibleCardIdentityClassificationV1 {
    Match,
    NoMatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOfflineVisibleCardIdentityV1 {
    ordinal: u8,
    visible_card_name: String,
    winning_template_id: String,
    mean_absolute_difference_milli: u32,
    runner_up_distinct_name: String,
    runner_up_mean_absolute_difference_milli: u32,
    distinct_name_margin_milli: u32,
}

impl MtgoOfflineVisibleCardIdentityV1 {
    pub fn ordinal(&self) -> u8 {
        self.ordinal
    }

    pub fn visible_card_name(&self) -> &str {
        &self.visible_card_name
    }

    pub fn winning_template_id(&self) -> &str {
        &self.winning_template_id
    }

    pub fn mean_absolute_difference_milli(&self) -> u32 {
        self.mean_absolute_difference_milli
    }

    pub fn runner_up_distinct_name(&self) -> &str {
        &self.runner_up_distinct_name
    }

    pub fn runner_up_mean_absolute_difference_milli(&self) -> u32 {
        self.runner_up_mean_absolute_difference_milli
    }

    pub fn distinct_name_margin_milli(&self) -> u32 {
        self.distinct_name_margin_milli
    }
}

/// Checked-untrusted identity labels for one complete visible bottom-six hand.
///
/// A Match means every visible ordinal selected one distinct semantic name
/// under the fixed distance ceiling and margin. The result retains no pixels,
/// card coordinates, template bytes, or kernel object bindings.
pub struct CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV1 {
    classification: MtgoOfflineVisibleCardIdentityClassificationV1,
    source_manifest_sha256: String,
    source_stage_commitment_sha256: String,
    profile_commitment_sha256: String,
    visible_hand_count: Option<u8>,
    matched_identity_count: u8,
    identities: Vec<MtgoOfflineVisibleCardIdentityV1>,
    candidate_commitment_sha256: String,
}

/// Checked-untrusted identity coverage for one v2-gated bottom-six hand.
///
/// Every visible card region is evaluated independently. A partial result
/// exposes only the passing count and no semantic labels. Labels are exposed
/// only when every visible card passes the fixed distance and distinct-name
/// margin contract. This type grants no semantic evidence, observation,
/// scoring, or input authority.
pub struct CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV2 {
    classification: MtgoOfflineVisibleCardIdentityClassificationV1,
    source_manifest_sha256: String,
    source_stage_commitment_sha256: String,
    profile_commitment_sha256: String,
    visible_hand_count: Option<u8>,
    matched_identity_count: u8,
    identities: Vec<MtgoOfflineVisibleCardIdentityV1>,
    candidate_commitment_sha256: String,
}

/// Checked-untrusted identity coverage for one v3-gated bottom-six hand.
///
/// Partial coverage exposes only a count. Semantic labels remain hidden until
/// every visible card passes the fixed distance and distinct-name margin.
pub struct CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV3 {
    classification: MtgoOfflineVisibleCardIdentityClassificationV1,
    source_manifest_sha256: String,
    source_stage_commitment_sha256: String,
    profile_commitment_sha256: String,
    visible_hand_count: Option<u8>,
    matched_identity_count: u8,
    identities: Vec<MtgoOfflineVisibleCardIdentityV1>,
    candidate_commitment_sha256: String,
}

/// Checked-untrusted identity coverage for all seven cards visible at one
/// London mulligan prompt.
///
/// MTGO displays seven cards at every prompt while the prospective keep size
/// descends from seven to one. Labels are exposed only when the current v2
/// ladder prompt has exactly one match and all seven visible card regions pass
/// the fixed template distance and distinct-name margin contract. This type
/// retains no pixels or coordinates and grants no semantic evidence,
/// observation, model-scoring, or input authority.
pub struct CheckedUntrustedMtgoOfflineMulliganVisibleCardIdentityCandidateV1 {
    classification: MtgoOfflineVisibleCardIdentityClassificationV1,
    source_manifest_sha256: String,
    source_frame_sha256: String,
    source_ladder_commitment_sha256: String,
    profile_commitment_sha256: String,
    prospective_keep_size: Option<u8>,
    visible_hand_count: Option<u8>,
    matched_identity_count: u8,
    identities: Vec<MtgoOfflineVisibleCardIdentityV1>,
    candidate_commitment_sha256: String,
}

impl CheckedUntrustedMtgoOfflineMulliganVisibleCardIdentityCandidateV1 {
    pub fn classification(&self) -> MtgoOfflineVisibleCardIdentityClassificationV1 {
        self.classification
    }

    pub fn source_manifest_sha256(&self) -> &str {
        &self.source_manifest_sha256
    }

    pub fn source_frame_sha256(&self) -> &str {
        &self.source_frame_sha256
    }

    pub fn source_ladder_commitment_sha256(&self) -> &str {
        &self.source_ladder_commitment_sha256
    }

    pub fn profile_commitment_sha256(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn prospective_keep_size(&self) -> Option<u8> {
        self.prospective_keep_size
    }

    pub fn visible_hand_count(&self) -> Option<u8> {
        self.visible_hand_count
    }

    pub fn matched_identity_count(&self) -> u8 {
        self.matched_identity_count
    }

    pub fn identities(&self) -> &[MtgoOfflineVisibleCardIdentityV1] {
        &self.identities
    }

    pub fn candidate_commitment_sha256(&self) -> &str {
        &self.candidate_commitment_sha256
    }

    pub fn safe_for_semantic_evidence(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

impl CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV3 {
    pub fn classification(&self) -> MtgoOfflineVisibleCardIdentityClassificationV1 {
        self.classification
    }

    pub fn source_manifest_sha256(&self) -> &str {
        &self.source_manifest_sha256
    }

    pub fn source_stage_commitment_sha256(&self) -> &str {
        &self.source_stage_commitment_sha256
    }

    pub fn profile_commitment_sha256(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn visible_hand_count(&self) -> Option<u8> {
        self.visible_hand_count
    }

    pub fn matched_identity_count(&self) -> u8 {
        self.matched_identity_count
    }

    pub fn identities(&self) -> &[MtgoOfflineVisibleCardIdentityV1] {
        &self.identities
    }

    pub fn candidate_commitment_sha256(&self) -> &str {
        &self.candidate_commitment_sha256
    }

    pub fn safe_for_semantic_evidence(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

impl CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV2 {
    pub fn classification(&self) -> MtgoOfflineVisibleCardIdentityClassificationV1 {
        self.classification
    }

    pub fn source_manifest_sha256(&self) -> &str {
        &self.source_manifest_sha256
    }

    pub fn source_stage_commitment_sha256(&self) -> &str {
        &self.source_stage_commitment_sha256
    }

    pub fn profile_commitment_sha256(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn visible_hand_count(&self) -> Option<u8> {
        self.visible_hand_count
    }

    pub fn matched_identity_count(&self) -> u8 {
        self.matched_identity_count
    }

    pub fn identities(&self) -> &[MtgoOfflineVisibleCardIdentityV1] {
        &self.identities
    }

    pub fn candidate_commitment_sha256(&self) -> &str {
        &self.candidate_commitment_sha256
    }

    pub fn safe_for_semantic_evidence(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

impl CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV1 {
    pub fn classification(&self) -> MtgoOfflineVisibleCardIdentityClassificationV1 {
        self.classification
    }

    pub fn source_manifest_sha256(&self) -> &str {
        &self.source_manifest_sha256
    }

    pub fn source_stage_commitment_sha256(&self) -> &str {
        &self.source_stage_commitment_sha256
    }

    pub fn profile_commitment_sha256(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn visible_hand_count(&self) -> Option<u8> {
        self.visible_hand_count
    }

    pub fn matched_identity_count(&self) -> u8 {
        self.matched_identity_count
    }

    pub fn identities(&self) -> &[MtgoOfflineVisibleCardIdentityV1] {
        &self.identities
    }

    pub fn candidate_commitment_sha256(&self) -> &str {
        &self.candidate_commitment_sha256
    }

    pub fn safe_for_semantic_evidence(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn check_untrusted_offline_visible_card_template_profile_v1(
    profile: MtgoOfflineVisibleCardTemplateProfileV1,
) -> Result<CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1, MtgoContractErrorV1> {
    validate_profile_v1(&profile)?;
    let distinct_visible_card_name_count = profile
        .templates
        .iter()
        .map(|template| template.visible_card_name.as_str())
        .collect::<HashSet<_>>()
        .len();
    let bytes = serde_json::to_vec(&profile).map_err(|error| {
        error_v1(
            "offline_visible_card_profile_serialization",
            error.to_string(),
        )
    })?;
    let profile_commitment_sha256 = commitment_v1(PROFILE_DOMAIN_V1, &[&bytes]);
    Ok(CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1 {
        profile,
        profile_commitment_sha256,
        distinct_visible_card_name_count,
    })
}

pub fn classify_untrusted_offline_mulligan_visible_card_identities_v1(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
    profile: &CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
) -> Result<CheckedUntrustedMtgoOfflineMulliganVisibleCardIdentityCandidateV1, MtgoContractErrorV1>
{
    if checked.client_size_px() != &profile.profile.client_size_px
        || checked.output_identity_sha256() != profile.profile.output_identity_sha256
    {
        return Err(error_v1(
            "offline_visible_card_profile_layout",
            "capture layout and output must match the template profile",
        ));
    }

    let ladder = classify_untrusted_offline_mulligan_ladder_candidate_v2(checked, canonical_bgra8)?;
    let ladder_matches =
        ladder.classification() == MtgoOfflineMulliganLadderClassificationV1::Match;
    let prospective_keep_size = ladder.prospective_keep_size();
    let visible_hand_count = ladder_matches.then_some(7);
    let mut identities = Vec::new();
    if ladder_matches {
        for (ordinal, rect) in card_art_regions_v1(7)?.iter().enumerate() {
            let Some(identity) = classify_region_v1(
                canonical_bgra8,
                checked.client_size_px(),
                rect,
                &profile.profile.templates,
            )?
            else {
                continue;
            };
            identities.push(MtgoOfflineVisibleCardIdentityV1 {
                ordinal: u8::try_from(ordinal).map_err(|_| {
                    error_v1("offline_visible_card_ordinal", "card ordinal overflow")
                })?,
                ..identity
            });
        }
    }

    let matched_identity_count = u8::try_from(identities.len()).unwrap_or(u8::MAX);
    let full_match = prospective_keep_size.is_some()
        && visible_hand_count == Some(7)
        && matched_identity_count == 7;
    let classification = if full_match {
        MtgoOfflineVisibleCardIdentityClassificationV1::Match
    } else {
        identities.clear();
        MtgoOfflineVisibleCardIdentityClassificationV1::NoMatch
    };

    let mut parts: Vec<Vec<u8>> = vec![
        checked.manifest_sha256().as_bytes().to_vec(),
        checked.canonical_bgra8_sha256().as_bytes().to_vec(),
        ladder.candidate_commitment_sha256().as_bytes().to_vec(),
        profile.profile_commitment_sha256.as_bytes().to_vec(),
        match classification {
            MtgoOfflineVisibleCardIdentityClassificationV1::Match => b"match".to_vec(),
            MtgoOfflineVisibleCardIdentityClassificationV1::NoMatch => b"no_match".to_vec(),
        },
        vec![prospective_keep_size.unwrap_or(u8::MAX)],
        vec![visible_hand_count.unwrap_or(u8::MAX)],
        vec![matched_identity_count],
    ];
    for identity in &identities {
        parts.push(identity_commitment_ordinal_v2(identity));
    }
    let borrowed: Vec<_> = parts.iter().map(Vec::as_slice).collect();
    let candidate_commitment_sha256 = commitment_v1(MULLIGAN_CANDIDATE_DOMAIN_V1, &borrowed);

    Ok(
        CheckedUntrustedMtgoOfflineMulliganVisibleCardIdentityCandidateV1 {
            classification,
            source_manifest_sha256: checked.manifest_sha256().to_owned(),
            source_frame_sha256: checked.canonical_bgra8_sha256().to_owned(),
            source_ladder_commitment_sha256: ladder.candidate_commitment_sha256().to_owned(),
            profile_commitment_sha256: profile.profile_commitment_sha256.clone(),
            prospective_keep_size,
            visible_hand_count,
            matched_identity_count,
            identities,
            candidate_commitment_sha256,
        },
    )
}

pub fn classify_untrusted_offline_bottom_six_visible_card_identities_v1(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
    profile: &CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
) -> Result<CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV1, MtgoContractErrorV1> {
    if checked.client_size_px() != &profile.profile.client_size_px
        || checked.output_identity_sha256() != profile.profile.output_identity_sha256
    {
        return Err(error_v1(
            "offline_visible_card_profile_layout",
            "capture layout and output must match the template profile",
        ));
    }
    let stage = classify_untrusted_offline_bottom_six_state_candidate_v1(checked, canonical_bgra8)?;
    let visible_hand_count = stage.visible_hand_count();
    let mut identities = Vec::new();
    if stage.classification() == MtgoOfflineBottomSixStateClassificationV1::Match {
        for (ordinal, rect) in card_art_regions_v1(visible_hand_count.unwrap_or(0))?
            .iter()
            .enumerate()
        {
            let Some(identity) = classify_region_v1(
                canonical_bgra8,
                checked.client_size_px(),
                rect,
                &profile.profile.templates,
            )?
            else {
                break;
            };
            identities.push(MtgoOfflineVisibleCardIdentityV1 {
                ordinal: u8::try_from(ordinal).map_err(|_| {
                    error_v1("offline_visible_card_ordinal", "card ordinal overflow")
                })?,
                ..identity
            });
        }
    }
    let matched_identity_count = u8::try_from(identities.len()).unwrap_or(u8::MAX);
    let full_match = visible_hand_count.is_some_and(|count| count == matched_identity_count)
        && matched_identity_count > 0;
    let classification = if full_match {
        MtgoOfflineVisibleCardIdentityClassificationV1::Match
    } else {
        identities.clear();
        MtgoOfflineVisibleCardIdentityClassificationV1::NoMatch
    };

    let mut parts: Vec<Vec<u8>> = vec![
        checked.manifest_sha256().as_bytes().to_vec(),
        checked.canonical_bgra8_sha256().as_bytes().to_vec(),
        stage.candidate_commitment_sha256().as_bytes().to_vec(),
        profile.profile_commitment_sha256.as_bytes().to_vec(),
        match classification {
            MtgoOfflineVisibleCardIdentityClassificationV1::Match => b"match".to_vec(),
            MtgoOfflineVisibleCardIdentityClassificationV1::NoMatch => b"no_match".to_vec(),
        },
        vec![visible_hand_count.unwrap_or(u8::MAX)],
        vec![matched_identity_count],
    ];
    for identity in &identities {
        parts.extend([
            vec![identity.ordinal],
            identity.visible_card_name.as_bytes().to_vec(),
            identity.winning_template_id.as_bytes().to_vec(),
            identity
                .mean_absolute_difference_milli
                .to_be_bytes()
                .to_vec(),
            identity.runner_up_distinct_name.as_bytes().to_vec(),
            identity
                .runner_up_mean_absolute_difference_milli
                .to_be_bytes()
                .to_vec(),
            identity.distinct_name_margin_milli.to_be_bytes().to_vec(),
        ]);
    }
    let borrowed: Vec<_> = parts.iter().map(Vec::as_slice).collect();
    let candidate_commitment_sha256 = commitment_v1(CANDIDATE_DOMAIN_V1, &borrowed);

    Ok(CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV1 {
        classification,
        source_manifest_sha256: checked.manifest_sha256().to_owned(),
        source_stage_commitment_sha256: stage.candidate_commitment_sha256().to_owned(),
        profile_commitment_sha256: profile.profile_commitment_sha256.clone(),
        visible_hand_count,
        matched_identity_count,
        identities,
        candidate_commitment_sha256,
    })
}

pub fn classify_untrusted_offline_bottom_six_visible_card_identities_v2(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
    profile: &CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
) -> Result<CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV2, MtgoContractErrorV1> {
    if checked.client_size_px() != &profile.profile.client_size_px
        || checked.output_identity_sha256() != profile.profile.output_identity_sha256
    {
        return Err(error_v1(
            "offline_visible_card_profile_layout",
            "capture layout and output must match the template profile",
        ));
    }
    let stage = classify_untrusted_offline_bottom_six_state_candidate_v2(checked, canonical_bgra8)?;
    let visible_hand_count = stage.visible_hand_count();
    let mut identities = Vec::new();
    if stage.classification() == MtgoOfflineBottomSixStateClassificationV2::Match {
        for (ordinal, rect) in card_art_regions_v1(visible_hand_count.unwrap_or(0))?
            .iter()
            .enumerate()
        {
            let Some(identity) = classify_region_v1(
                canonical_bgra8,
                checked.client_size_px(),
                rect,
                &profile.profile.templates,
            )?
            else {
                continue;
            };
            identities.push(MtgoOfflineVisibleCardIdentityV1 {
                ordinal: u8::try_from(ordinal).map_err(|_| {
                    error_v1("offline_visible_card_ordinal", "card ordinal overflow")
                })?,
                ..identity
            });
        }
    }
    let matched_identity_count = u8::try_from(identities.len()).unwrap_or(u8::MAX);
    let full_match = visible_hand_count.is_some_and(|count| count == matched_identity_count)
        && matched_identity_count > 0;
    let classification = if full_match {
        MtgoOfflineVisibleCardIdentityClassificationV1::Match
    } else {
        identities.clear();
        MtgoOfflineVisibleCardIdentityClassificationV1::NoMatch
    };

    let mut parts: Vec<Vec<u8>> = vec![
        checked.manifest_sha256().as_bytes().to_vec(),
        checked.canonical_bgra8_sha256().as_bytes().to_vec(),
        stage.candidate_commitment_sha256().as_bytes().to_vec(),
        profile.profile_commitment_sha256.as_bytes().to_vec(),
        match classification {
            MtgoOfflineVisibleCardIdentityClassificationV1::Match => b"match".to_vec(),
            MtgoOfflineVisibleCardIdentityClassificationV1::NoMatch => b"no_match".to_vec(),
        },
        vec![visible_hand_count.unwrap_or(u8::MAX)],
        vec![matched_identity_count],
    ];
    for identity in &identities {
        parts.extend([identity_commitment_ordinal_v2(identity)]);
    }
    let borrowed: Vec<_> = parts.iter().map(Vec::as_slice).collect();
    let candidate_commitment_sha256 = commitment_v1(CANDIDATE_DOMAIN_V2, &borrowed);

    Ok(CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV2 {
        classification,
        source_manifest_sha256: checked.manifest_sha256().to_owned(),
        source_stage_commitment_sha256: stage.candidate_commitment_sha256().to_owned(),
        profile_commitment_sha256: profile.profile_commitment_sha256.clone(),
        visible_hand_count,
        matched_identity_count,
        identities,
        candidate_commitment_sha256,
    })
}

pub fn classify_untrusted_offline_bottom_six_visible_card_identities_v3(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
    profile: &CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
) -> Result<CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV3, MtgoContractErrorV1> {
    if checked.client_size_px() != &profile.profile.client_size_px
        || checked.output_identity_sha256() != profile.profile.output_identity_sha256
    {
        return Err(error_v1(
            "offline_visible_card_profile_layout",
            "capture layout and output must match the template profile",
        ));
    }
    let stage = classify_untrusted_offline_bottom_six_state_candidate_v3(checked, canonical_bgra8)?;
    let visible_hand_count = stage.visible_hand_count();
    let mut identities = Vec::new();
    if stage.classification() == MtgoOfflineBottomSixStateClassificationV3::Match {
        for (ordinal, rect) in card_art_regions_v1(visible_hand_count.unwrap_or(0))?
            .iter()
            .enumerate()
        {
            let Some(identity) = classify_region_v1(
                canonical_bgra8,
                checked.client_size_px(),
                rect,
                &profile.profile.templates,
            )?
            else {
                continue;
            };
            identities.push(MtgoOfflineVisibleCardIdentityV1 {
                ordinal: u8::try_from(ordinal).map_err(|_| {
                    error_v1("offline_visible_card_ordinal", "card ordinal overflow")
                })?,
                ..identity
            });
        }
    }
    let matched_identity_count = u8::try_from(identities.len()).unwrap_or(u8::MAX);
    let full_match = visible_hand_count.is_some_and(|count| count == matched_identity_count)
        && matched_identity_count > 0;
    let classification = if full_match {
        MtgoOfflineVisibleCardIdentityClassificationV1::Match
    } else {
        identities.clear();
        MtgoOfflineVisibleCardIdentityClassificationV1::NoMatch
    };

    let mut parts: Vec<Vec<u8>> = vec![
        checked.manifest_sha256().as_bytes().to_vec(),
        checked.canonical_bgra8_sha256().as_bytes().to_vec(),
        stage.candidate_commitment_sha256().as_bytes().to_vec(),
        profile.profile_commitment_sha256.as_bytes().to_vec(),
        match classification {
            MtgoOfflineVisibleCardIdentityClassificationV1::Match => b"match".to_vec(),
            MtgoOfflineVisibleCardIdentityClassificationV1::NoMatch => b"no_match".to_vec(),
        },
        vec![visible_hand_count.unwrap_or(u8::MAX)],
        vec![matched_identity_count],
    ];
    for identity in &identities {
        parts.push(identity_commitment_ordinal_v2(identity));
    }
    let borrowed: Vec<_> = parts.iter().map(Vec::as_slice).collect();
    let candidate_commitment_sha256 = commitment_v1(CANDIDATE_DOMAIN_V3, &borrowed);

    Ok(CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV3 {
        classification,
        source_manifest_sha256: checked.manifest_sha256().to_owned(),
        source_stage_commitment_sha256: stage.candidate_commitment_sha256().to_owned(),
        profile_commitment_sha256: profile.profile_commitment_sha256.clone(),
        visible_hand_count,
        matched_identity_count,
        identities,
        candidate_commitment_sha256,
    })
}

fn identity_commitment_ordinal_v2(identity: &MtgoOfflineVisibleCardIdentityV1) -> Vec<u8> {
    let mut bytes = Vec::new();
    for part in [
        &[identity.ordinal][..],
        identity.visible_card_name.as_bytes(),
        identity.winning_template_id.as_bytes(),
        identity
            .mean_absolute_difference_milli
            .to_be_bytes()
            .as_slice(),
        identity.runner_up_distinct_name.as_bytes(),
        identity
            .runner_up_mean_absolute_difference_milli
            .to_be_bytes()
            .as_slice(),
        identity.distinct_name_margin_milli.to_be_bytes().as_slice(),
    ] {
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part);
    }
    bytes
}

fn validate_profile_v1(
    profile: &MtgoOfflineVisibleCardTemplateProfileV1,
) -> Result<(), MtgoContractErrorV1> {
    if profile.schema_version != PROFILE_SCHEMA_V1
        || profile.matcher_version != MATCHER_VERSION_V1
        || profile.maximum_mean_absolute_difference_milli != MAX_DISTANCE_MILLI_V1
        || profile.minimum_distinct_name_margin_milli != MIN_DISTINCT_NAME_MARGIN_MILLI_V1
    {
        return Err(error_v1(
            "offline_visible_card_profile_contract",
            "profile schema, matcher, and thresholds must match v1 exactly",
        ));
    }
    validate_label_v1("profile_id", &profile.profile_id, 128)?;
    for (field, digest) in [
        ("deck_manifest_sha256", &profile.deck_manifest_sha256),
        ("output_identity_sha256", &profile.output_identity_sha256),
    ] {
        if !is_lower_sha256_v1(digest) {
            return Err(error_v1(
                "offline_visible_card_profile_digest",
                format!("{field} must be lowercase SHA-256"),
            ));
        }
    }
    if profile.client_size_px.width != 1_550 || profile.client_size_px.height != 925 {
        return Err(error_v1(
            "offline_visible_card_profile_size",
            "v1 supports only the reviewed 1550 by 925 client layout",
        ));
    }
    if !(2..=128).contains(&profile.templates.len()) {
        return Err(error_v1(
            "offline_visible_card_profile_templates",
            "profile must contain 2 through 128 templates",
        ));
    }
    let expected_bytes = usize::try_from(ART_WIDTH_V1)
        .ok()
        .and_then(|width| {
            usize::try_from(ART_HEIGHT_V1)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(TEMPLATE_CHANNELS_V1))
        .ok_or_else(|| {
            error_v1(
                "offline_visible_card_template_size",
                "template size overflow",
            )
        })?;
    let mut template_ids = HashSet::new();
    let mut names = HashSet::new();
    let mut oracle_by_name = BTreeMap::new();
    for template in &profile.templates {
        validate_label_v1("template_id", &template.template_id, 128)?;
        validate_label_v1("visible_card_name", &template.visible_card_name, 256)?;
        validate_label_v1("set_code", &template.set_code, 16)?;
        validate_label_v1("collector_number", &template.collector_number, 32)?;
        if !template_ids.insert(template.template_id.as_str()) {
            return Err(error_v1(
                "offline_visible_card_template_id",
                "template IDs must be unique",
            ));
        }
        names.insert(template.visible_card_name.as_str());
        if !is_uuid_v1(&template.public_print_id) || !is_uuid_v1(&template.public_oracle_id) {
            return Err(error_v1(
                "offline_visible_card_public_id",
                "public print and Oracle IDs must be lowercase UUIDs",
            ));
        }
        if oracle_by_name
            .insert(
                template.visible_card_name.as_str(),
                template.public_oracle_id.as_str(),
            )
            .is_some_and(|previous| previous != template.public_oracle_id)
        {
            return Err(error_v1(
                "offline_visible_card_name_oracle",
                "all templates for one visible card name must share one Oracle ID",
            ));
        }
        for digest in [
            &template.public_reference_image_sha256,
            &template.source_capture_manifest_sha256,
            &template.reference_bgr8_sha256,
        ] {
            if !is_lower_sha256_v1(digest) {
                return Err(error_v1(
                    "offline_visible_card_template_digest",
                    "template commitments must be lowercase SHA-256",
                ));
            }
        }
        if template.reference_bgr8.len() != expected_bytes
            || sha256_v1(&template.reference_bgr8) != template.reference_bgr8_sha256
        {
            return Err(error_v1(
                "offline_visible_card_template_bytes",
                "template bytes must be exact reviewed 96 by 60 BGR with matching hash",
            ));
        }
    }
    if names.len() < 2 {
        return Err(error_v1(
            "offline_visible_card_profile_names",
            "profile must discriminate at least two distinct visible card names",
        ));
    }
    Ok(())
}

fn classify_region_v1(
    observed_bgra8: &[u8],
    size: &MtgoSizePxV1,
    rect: &MtgoRectPxV1,
    templates: &[MtgoOfflineVisibleCardTemplateV1],
) -> Result<Option<MtgoOfflineVisibleCardIdentityV1>, MtgoContractErrorV1> {
    let mut by_name: BTreeMap<&str, (u32, &str)> = BTreeMap::new();
    for template in templates {
        let distance = mean_absolute_bgr_difference_milli_v1(
            observed_bgra8,
            size,
            rect,
            &template.reference_bgr8,
        )?;
        let entry = by_name
            .entry(template.visible_card_name.as_str())
            .or_insert((distance, template.template_id.as_str()));
        if (distance, template.template_id.as_str()) < *entry {
            *entry = (distance, template.template_id.as_str());
        }
    }
    let mut ranked: Vec<_> = by_name
        .into_iter()
        .map(|(name, (distance, template_id))| (distance, name, template_id))
        .collect();
    ranked.sort_unstable();
    if ranked.len() < 2 {
        return Ok(None);
    }
    let (winner_distance, winner_name, winner_template) = ranked[0];
    let (runner_distance, runner_name, _) = ranked[1];
    let margin = runner_distance.saturating_sub(winner_distance);
    if winner_distance > MAX_DISTANCE_MILLI_V1 || margin < MIN_DISTINCT_NAME_MARGIN_MILLI_V1 {
        return Ok(None);
    }
    Ok(Some(MtgoOfflineVisibleCardIdentityV1 {
        ordinal: 0,
        visible_card_name: winner_name.to_owned(),
        winning_template_id: winner_template.to_owned(),
        mean_absolute_difference_milli: winner_distance,
        runner_up_distinct_name: runner_name.to_owned(),
        runner_up_mean_absolute_difference_milli: runner_distance,
        distinct_name_margin_milli: margin,
    }))
}

fn mean_absolute_bgr_difference_milli_v1(
    observed_bgra8: &[u8],
    size: &MtgoSizePxV1,
    rect: &MtgoRectPxV1,
    reference_bgr8: &[u8],
) -> Result<u32, MtgoContractErrorV1> {
    if rect.width != ART_WIDTH_V1 || rect.height != ART_HEIGHT_V1 {
        return Err(error_v1(
            "offline_visible_card_region",
            "visible card art region must be 96 by 60",
        ));
    }
    let expected_frame = usize::try_from(size.width)
        .ok()
        .and_then(|width| {
            usize::try_from(size.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| error_v1("offline_visible_card_pixels", "frame size overflow"))?;
    if observed_bgra8.len() != expected_frame {
        return Err(error_v1(
            "offline_visible_card_pixels",
            "observed pixels must match the reviewed frame size",
        ));
    }
    let expected_reference = usize::try_from(rect.width)
        .ok()
        .and_then(|width| {
            usize::try_from(rect.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(TEMPLATE_CHANNELS_V1))
        .ok_or_else(|| error_v1("offline_visible_card_template", "template size overflow"))?;
    if reference_bgr8.len() != expected_reference
        || rect
            .x
            .checked_add(rect.width)
            .is_none_or(|right| right > size.width)
        || rect
            .y
            .checked_add(rect.height)
            .is_none_or(|bottom| bottom > size.height)
    {
        return Err(error_v1(
            "offline_visible_card_template",
            "template or visible region has invalid dimensions",
        ));
    }
    let stride = usize::try_from(size.width)
        .map_err(|_| error_v1("offline_visible_card_pixels", "frame width overflow"))?;
    let mut total = 0_u64;
    let mut reference_offset = 0_usize;
    for row in 0..rect.height {
        for column in 0..rect.width {
            let observed_offset = usize::try_from(rect.y + row)
                .ok()
                .and_then(|y| y.checked_mul(stride))
                .and_then(|base| {
                    usize::try_from(rect.x + column)
                        .ok()
                        .and_then(|x| base.checked_add(x))
                })
                .and_then(|pixel| pixel.checked_mul(4))
                .ok_or_else(|| error_v1("offline_visible_card_pixels", "pixel offset overflow"))?;
            for channel in 0..TEMPLATE_CHANNELS_V1 {
                total += u64::from(observed_bgra8[observed_offset + channel])
                    .abs_diff(u64::from(reference_bgr8[reference_offset + channel]));
            }
            reference_offset += TEMPLATE_CHANNELS_V1;
        }
    }
    let samples = u64::from(rect.width) * u64::from(rect.height) * 3;
    u32::try_from((total * 1_000 + samples / 2) / samples).map_err(|_| {
        error_v1(
            "offline_visible_card_distance",
            "mean absolute difference overflow",
        )
    })
}

fn validate_label_v1(
    field: &'static str,
    value: &str,
    maximum_len: usize,
) -> Result<(), MtgoContractErrorV1> {
    if value.trim() != value
        || value.is_empty()
        || value.len() > maximum_len
        || value.chars().any(char::is_control)
    {
        return Err(error_v1(
            "offline_visible_card_label",
            format!("{field} must be a bounded visible label"),
        ));
    }
    Ok(())
}

fn is_lower_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_uuid_v1(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
            }
        })
}

fn sha256_v1(bytes: &[u8]) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn template_v1(id: &str, name: &str, value: u8) -> MtgoOfflineVisibleCardTemplateV1 {
        let reference_bgr8 = vec![value; 96 * 60 * 3];
        MtgoOfflineVisibleCardTemplateV1 {
            template_id: id.to_owned(),
            visible_card_name: name.to_owned(),
            public_print_id: "1069841a-0642-4fb6-b831-d45ff7fda3af".to_owned(),
            public_oracle_id: "b34bb2dc-c1af-4d77-b0b3-a0fb342a5fc6".to_owned(),
            set_code: "tst".to_owned(),
            collector_number: "1".to_owned(),
            public_reference_image_sha256: "1".repeat(64),
            source_capture_manifest_sha256: "2".repeat(64),
            reference_bgr8_sha256: sha256_v1(&reference_bgr8),
            reference_bgr8,
        }
    }

    fn profile_v1() -> MtgoOfflineVisibleCardTemplateProfileV1 {
        MtgoOfflineVisibleCardTemplateProfileV1 {
            schema_version: 1,
            profile_id: "synthetic-two-name-profile-v1".to_owned(),
            matcher_version: MATCHER_VERSION_V1.to_owned(),
            deck_manifest_sha256: "3".repeat(64),
            output_identity_sha256: "4".repeat(64),
            client_size_px: MtgoSizePxV1 {
                width: 1_550,
                height: 925,
            },
            maximum_mean_absolute_difference_milli: MAX_DISTANCE_MILLI_V1,
            minimum_distinct_name_margin_milli: MIN_DISTINCT_NAME_MARGIN_MILLI_V1,
            templates: vec![
                template_v1("dark", "Dark Card", 10),
                template_v1("light", "Light Card", 200),
            ],
        }
    }

    fn observed_frame_v1(rect: &MtgoRectPxV1, value: u8) -> Vec<u8> {
        let size = MtgoSizePxV1 {
            width: 1_550,
            height: 925,
        };
        let mut pixels = vec![0_u8; 1_550 * 925 * 4];
        for row in rect.y..rect.y + rect.height {
            for column in rect.x..rect.x + rect.width {
                let offset = (usize::try_from(row).unwrap() * usize::try_from(size.width).unwrap()
                    + usize::try_from(column).unwrap())
                    * 4;
                pixels[offset..offset + 3].fill(value);
                pixels[offset + 3] = 255;
            }
        }
        pixels
    }

    #[test]
    fn checked_profile_binds_exact_fixed_contract_and_bytes() {
        let checked =
            check_untrusted_offline_visible_card_template_profile_v1(profile_v1()).unwrap();
        assert_eq!(checked.template_count(), 2);
        assert_eq!(checked.distinct_visible_card_name_count(), 2);
        assert_eq!(checked.maximum_mean_absolute_difference_milli(), 25_000);
        assert_eq!(checked.minimum_distinct_name_margin_milli(), 10_000);
        assert!(!checked.safe_for_semantic_evidence());
        assert!(!checked.safe_for_observation_v5());
        assert!(!checked.safe_for_policy_scoring());
        assert!(!checked.safe_for_input());

        let mut wrong_threshold = profile_v1();
        wrong_threshold.maximum_mean_absolute_difference_milli += 1;
        assert!(check_untrusted_offline_visible_card_template_profile_v1(wrong_threshold).is_err());
        let mut wrong_hash = profile_v1();
        wrong_hash.templates[0].reference_bgr8_sha256 = "0".repeat(64);
        assert!(check_untrusted_offline_visible_card_template_profile_v1(wrong_hash).is_err());
        let mut duplicate = profile_v1();
        duplicate.templates[1].template_id = duplicate.templates[0].template_id.clone();
        assert!(check_untrusted_offline_visible_card_template_profile_v1(duplicate).is_err());
        let mut conflicting_oracle = profile_v1();
        conflicting_oracle.templates[1].visible_card_name =
            conflicting_oracle.templates[0].visible_card_name.clone();
        conflicting_oracle.templates[1].public_oracle_id =
            "56719f6a-1a6c-4c0a-8d21-18f7d7350b68".to_owned();
        assert!(
            check_untrusted_offline_visible_card_template_profile_v1(conflicting_oracle).is_err()
        );
    }

    #[test]
    fn exact_region_selects_one_distinct_name() {
        let rect = MtgoRectPxV1 {
            x: 330,
            y: 760,
            width: 96,
            height: 60,
        };
        let pixels = observed_frame_v1(&rect, 10);
        let profile = profile_v1();
        let identity =
            classify_region_v1(&pixels, &profile.client_size_px, &rect, &profile.templates)
                .unwrap()
                .unwrap();
        assert_eq!(identity.visible_card_name(), "Dark Card");
        assert_eq!(identity.mean_absolute_difference_milli(), 0);
        assert_eq!(identity.runner_up_mean_absolute_difference_milli(), 190_000);
        assert_eq!(identity.distinct_name_margin_milli(), 190_000);
    }

    #[test]
    fn duplicate_same_name_collapses_but_distinct_name_tie_rejects() {
        let rect = MtgoRectPxV1 {
            x: 330,
            y: 760,
            width: 96,
            height: 60,
        };
        let pixels = observed_frame_v1(&rect, 10);
        let mut same_name = profile_v1().templates;
        same_name.push(template_v1("dark-alt", "Dark Card", 11));
        assert_eq!(
            classify_region_v1(
                &pixels,
                &MtgoSizePxV1 {
                    width: 1_550,
                    height: 925,
                },
                &rect,
                &same_name,
            )
            .unwrap()
            .unwrap()
            .winning_template_id(),
            "dark"
        );

        let ambiguous = vec![
            template_v1("a", "Card A", 10),
            template_v1("b", "Card B", 10),
        ];
        assert!(classify_region_v1(
            &pixels,
            &MtgoSizePxV1 {
                width: 1_550,
                height: 925,
            },
            &rect,
            &ambiguous,
        )
        .unwrap()
        .is_none());
    }
}
