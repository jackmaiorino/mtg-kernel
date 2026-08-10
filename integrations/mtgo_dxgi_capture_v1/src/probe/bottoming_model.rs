use super::*;
use mtgo_blackbox_v1::{
    classify_untrusted_offline_bottom_six_visible_card_identities_v3,
    MtgoOfflineBottomingActionSemanticV1,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

mod action_plan;
pub use action_plan::*;

pub const MTGO_BOTTOMING_CARD_AWARE_SCORING_SCHEMA_V5: u32 = 5;

const BOTTOMING_STABLE_OBJECT_DOMAIN_V5: &[u8] = b"mtgo-bottoming-stable-object-v5";
const BOTTOMING_HISTORY_DOMAIN_V5: &[u8] = b"mtgo-bottoming-action-bound-history-v5";
const BOTTOMING_SCORING_REQUEST_DOMAIN_V5: &[u8] = b"mtgo-bottoming-card-aware-scoring-request-v5";
const BOTTOMING_MODEL_SELECTION_DOMAIN_V5: &[u8] = b"mtgo-bottoming-card-aware-model-selection-v5";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoBottomingCardIdentitySourceV5 {
    CurrentVisibleTemplate,
    ConfirmedActionHistory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoBottomingVisibleCardV5 {
    pub adapter_object_id: String,
    pub original_hand_ordinal: u8,
    pub current_visible_ordinal: u8,
    pub visible_card_name: String,
    pub identity_source: MtgoBottomingCardIdentitySourceV5,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoBottomingConfirmedCardV5 {
    pub adapter_object_id: String,
    pub original_hand_ordinal: u8,
    pub selection_ordinal: u8,
    pub visible_card_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCardAwareBottomingScoringRequestV5 {
    pub schema_version: u32,
    pub source_capture_commitment_sha256: String,
    pub state_measurement_commitment_sha256: String,
    pub current_identity_measurement_commitment_sha256: Option<String>,
    pub state_profile_commitment_sha256: String,
    pub visible_card_profile_commitment_sha256: String,
    pub history_commitment_sha256: String,
    pub required_bottom_count: u8,
    pub selected_count: u8,
    pub ordered_visible_cards: Vec<MtgoBottomingVisibleCardV5>,
    pub ordered_confirmed_bottomed_cards: Vec<MtgoBottomingConfirmedCardV5>,
    pub ordered_actions: Vec<MtgoOfflineBottomingActionSemanticV1>,
    pub deployment_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCardAwareBottomingScoreResponseV5 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub logits_f32_bits: Vec<u32>,
    pub value_f32_bits: u32,
}

/// An external bottoming scorer receives only ordered card labels, stable
/// adapter-local object IDs, confirmed action history, legal adapter-local
/// actions, and immutable commitments. It receives no pixels, coordinates,
/// process handles, authorization, or input capability. Card labels remain
/// checked-untrusted.
pub trait MtgoExternalCardAwareBottomingScorerV5 {
    fn score_card_aware_bottoming_v5(
        &mut self,
        request: &MtgoCardAwareBottomingScoringRequestV5,
    ) -> Result<MtgoCardAwareBottomingScoreResponseV5, String>;
}

#[derive(Serialize)]
struct StableBottomingCardV5 {
    adapter_object_id: String,
    original_hand_ordinal: u8,
    visible_card_name: String,
}

#[derive(Serialize)]
struct ConfirmedBottomedCardV5 {
    card: StableBottomingCardV5,
    selection_ordinal: u8,
}

/// An opaque action-bound London-bottoming history. It starts only from a
/// complete seven-card, zero-selected visible measurement. A later state can
/// advance it only through `confirm_card_aware_bottom_selection_v5`, which
/// binds one selected object to a strictly newer one-count transition and
/// rechecks every still-visible identity when the current renderer permits it.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCardAwareBottomingSessionV5;
/// let _forged = OpaqueMtgoCardAwareBottomingSessionV5 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCardAwareBottomingSessionV5;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoCardAwareBottomingSessionV5>();
/// ```
pub struct OpaqueMtgoCardAwareBottomingSessionV5 {
    current_state: OpaqueMtgoDxgiBottomSixStateMeasurementV3,
    profile: CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
    current_identity: Option<CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV3>,
    current_cards: Vec<StableBottomingCardV5>,
    confirmed_bottomed_cards: Vec<ConfirmedBottomedCardV5>,
    history_commitment_sha256: String,
}

impl OpaqueMtgoCardAwareBottomingSessionV5 {
    pub fn required_bottom_count_v5(&self) -> u8 {
        6
    }

    pub fn selected_count_v5(&self) -> u8 {
        u8::try_from(self.confirmed_bottomed_cards.len()).unwrap_or(u8::MAX)
    }

    pub fn visible_card_count_v5(&self) -> u8 {
        u8::try_from(self.current_cards.len()).unwrap_or(u8::MAX)
    }

    pub fn history_commitment_sha256_v5(&self) -> &str {
        &self.history_commitment_sha256
    }

    pub fn state_measurement_commitment_sha256_v5(&self) -> &str {
        self.current_state.measurement_commitment_sha256_v3()
    }

    pub fn current_identity_measurement_commitment_sha256_v5(&self) -> Option<&str> {
        self.current_identity
            .as_ref()
            .map(|identity| identity.candidate_commitment_sha256())
    }

    pub fn ready_to_submit_v5(&self) -> bool {
        self.selected_count_v5() == 6
    }

    pub fn safe_for_semantic_evidence_v5(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v5(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v5(&self) -> bool {
        false
    }

    pub fn safe_for_input_v5(&self) -> bool {
        false
    }
}

/// A response-bound model selection that owns its exact bottoming history.
/// It has no coordinate, action-plan, actuator, or live-input conversion.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCardAwareBottomingModelSelectionV5;
/// let _forged = OpaqueMtgoCardAwareBottomingModelSelectionV5 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCardAwareBottomingModelSelectionV5;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoCardAwareBottomingModelSelectionV5>();
/// ```
pub struct OpaqueMtgoCardAwareBottomingModelSelectionV5 {
    session: OpaqueMtgoCardAwareBottomingSessionV5,
    request: MtgoCardAwareBottomingScoringRequestV5,
    response: MtgoCardAwareBottomingScoreResponseV5,
    selected_index: usize,
    selected_semantic: MtgoOfflineBottomingActionSemanticV1,
    selection_commitment_sha256: String,
}

impl OpaqueMtgoCardAwareBottomingModelSelectionV5 {
    pub fn selected_index_v5(&self) -> usize {
        self.selected_index
    }

    pub fn selected_semantic_v5(&self) -> &MtgoOfflineBottomingActionSemanticV1 {
        &self.selected_semantic
    }

    pub fn selected_logit_f32_bits_v5(&self) -> u32 {
        self.response.logits_f32_bits[self.selected_index]
    }

    pub fn value_f32_bits_v5(&self) -> u32 {
        self.response.value_f32_bits
    }

    pub fn request_commitment_sha256_v5(&self) -> &str {
        &self.response.request_commitment_sha256
    }

    pub fn deployment_commitment_sha256_v5(&self) -> &str {
        &self.request.deployment_commitment_sha256
    }

    pub fn selection_commitment_sha256_v5(&self) -> &str {
        &self.selection_commitment_sha256
    }

    pub fn history_commitment_sha256_v5(&self) -> &str {
        self.session.history_commitment_sha256_v5()
    }

    pub fn safe_for_live_input_v5(&self) -> bool {
        false
    }
}

pub fn start_card_aware_bottoming_session_v5(
    measurement: OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3,
) -> Result<OpaqueMtgoCardAwareBottomingSessionV5, String> {
    let OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3 {
        source,
        profile,
        measurement,
    } = measurement;
    if source.classification_v3() != MtgoOfflineBottomSixStateClassificationV3::Match
        || source.required_bottom_count_v3() != Some(6)
        || source.selected_count_v3() != Some(0)
        || source.visible_hand_count_v3() != Some(7)
        || source.legal_action_count_v3() != Some(8)
        || measurement.classification() != MtgoOfflineVisibleCardIdentityClassificationV1::Match
        || measurement.visible_hand_count() != Some(7)
        || measurement.matched_identity_count() != 7
        || measurement.source_stage_commitment_sha256() != source.measurement_commitment_sha256_v3()
        || measurement.profile_commitment_sha256() != profile.profile_commitment_sha256()
    {
        return Err(
            "bottoming session requires one complete zero-selected seven-card measurement"
                .to_owned(),
        );
    }
    let identities = measurement.identities();
    if identities.len() != 7
        || identities
            .iter()
            .enumerate()
            .any(|(ordinal, identity)| usize::from(identity.ordinal()) != ordinal)
    {
        return Err("bottoming session identities must have canonical ordinals".to_owned());
    }
    let capture = source.source_capture_commitments_v3();
    let current_cards = identities
        .iter()
        .map(|identity| {
            let original_hand_ordinal = identity.ordinal();
            let adapter_object_id = stable_object_id_v5(
                &capture.capture_commitment_sha256,
                measurement.candidate_commitment_sha256(),
                original_hand_ordinal,
                identity.visible_card_name(),
            )?;
            Ok(StableBottomingCardV5 {
                adapter_object_id,
                original_hand_ordinal,
                visible_card_name: identity.visible_card_name().to_owned(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let confirmed_bottomed_cards = Vec::new();
    let history_commitment_sha256 = initial_history_commitment_v5(
        &capture.capture_commitment_sha256,
        source.measurement_commitment_sha256_v3(),
        measurement.candidate_commitment_sha256(),
        profile.profile_commitment_sha256(),
        &current_cards,
    )?;
    Ok(OpaqueMtgoCardAwareBottomingSessionV5 {
        current_state: source,
        profile,
        current_identity: Some(measurement),
        current_cards,
        confirmed_bottomed_cards,
        history_commitment_sha256,
    })
}

pub fn build_card_aware_bottoming_scoring_request_v5(
    session: &OpaqueMtgoCardAwareBottomingSessionV5,
    deployment: &MtgoExpectedModelDeploymentV1,
) -> Result<MtgoCardAwareBottomingScoringRequestV5, String> {
    validate_session_state_v5(session)?;
    let selected_count = session.selected_count_v5();
    let identity_source = if selected_count < 6 {
        MtgoBottomingCardIdentitySourceV5::CurrentVisibleTemplate
    } else {
        MtgoBottomingCardIdentitySourceV5::ConfirmedActionHistory
    };
    let ordered_visible_cards = session
        .current_cards
        .iter()
        .enumerate()
        .map(
            |(current_visible_ordinal, card)| MtgoBottomingVisibleCardV5 {
                adapter_object_id: card.adapter_object_id.clone(),
                original_hand_ordinal: card.original_hand_ordinal,
                current_visible_ordinal: u8::try_from(current_visible_ordinal).unwrap_or(u8::MAX),
                visible_card_name: card.visible_card_name.clone(),
                identity_source,
            },
        )
        .collect::<Vec<_>>();
    let ordered_confirmed_bottomed_cards = session
        .confirmed_bottomed_cards
        .iter()
        .map(|record| MtgoBottomingConfirmedCardV5 {
            adapter_object_id: record.card.adapter_object_id.clone(),
            original_hand_ordinal: record.card.original_hand_ordinal,
            selection_ordinal: record.selection_ordinal,
            visible_card_name: record.card.visible_card_name.clone(),
        })
        .collect::<Vec<_>>();
    let ordered_actions = canonical_actions_v5(&ordered_visible_cards, selected_count);
    let capture = session.current_state.source_capture_commitments_v3();
    let deployment_commitment_sha256 = model_deployment_commitment_v1(deployment)
        .map_err(|error| format!("bottoming model deployment: {error}"))?;
    let request = MtgoCardAwareBottomingScoringRequestV5 {
        schema_version: MTGO_BOTTOMING_CARD_AWARE_SCORING_SCHEMA_V5,
        source_capture_commitment_sha256: capture.capture_commitment_sha256,
        state_measurement_commitment_sha256: session
            .current_state
            .measurement_commitment_sha256_v3()
            .to_owned(),
        current_identity_measurement_commitment_sha256: session
            .current_identity
            .as_ref()
            .map(|identity| identity.candidate_commitment_sha256().to_owned()),
        state_profile_commitment_sha256: session
            .current_state
            .profile_commitment_sha256_v3()
            .to_owned(),
        visible_card_profile_commitment_sha256: session
            .profile
            .profile_commitment_sha256()
            .to_owned(),
        history_commitment_sha256: session.history_commitment_sha256.clone(),
        required_bottom_count: 6,
        selected_count,
        ordered_visible_cards,
        ordered_confirmed_bottomed_cards,
        ordered_actions,
        deployment_commitment_sha256,
    };
    validate_card_aware_bottoming_scoring_request_v5(&request)?;
    Ok(request)
}

pub fn card_aware_bottoming_scoring_request_commitment_v5(
    request: &MtgoCardAwareBottomingScoringRequestV5,
) -> Result<String, String> {
    validate_card_aware_bottoming_scoring_request_v5(request)?;
    canonical_json_commitment_v3(BOTTOMING_SCORING_REQUEST_DOMAIN_V5, request)
}

pub fn score_and_select_card_aware_bottoming_model_v5<S: MtgoExternalCardAwareBottomingScorerV5>(
    session: OpaqueMtgoCardAwareBottomingSessionV5,
    deployment: &MtgoExpectedModelDeploymentV1,
    scorer: &mut S,
) -> Result<OpaqueMtgoCardAwareBottomingModelSelectionV5, String> {
    let request = build_card_aware_bottoming_scoring_request_v5(&session, deployment)?;
    let response = scorer.score_card_aware_bottoming_v5(&request)?;
    validate_card_aware_bottoming_score_response_v5(session, deployment, response)
}

pub fn validate_card_aware_bottoming_score_response_v5(
    session: OpaqueMtgoCardAwareBottomingSessionV5,
    deployment: &MtgoExpectedModelDeploymentV1,
    response: MtgoCardAwareBottomingScoreResponseV5,
) -> Result<OpaqueMtgoCardAwareBottomingModelSelectionV5, String> {
    let request = build_card_aware_bottoming_scoring_request_v5(&session, deployment)?;
    let (selected_index, selected_semantic, selection_commitment_sha256) =
        validate_card_aware_bottoming_score_response_parts_v5(&request, &response)?;
    Ok(OpaqueMtgoCardAwareBottomingModelSelectionV5 {
        session,
        request,
        response,
        selected_index,
        selected_semantic,
        selection_commitment_sha256,
    })
}

pub fn confirm_card_aware_bottom_selection_v5(
    selection: OpaqueMtgoCardAwareBottomingModelSelectionV5,
    after: OpaqueMtgoDxgiBottomSixStateMeasurementV3,
) -> Result<OpaqueMtgoCardAwareBottomingSessionV5, String> {
    let OpaqueMtgoCardAwareBottomingModelSelectionV5 {
        mut session,
        selected_semantic,
        selection_commitment_sha256,
        ..
    } = selection;
    let (adapter_object_id, selection_ordinal) = match selected_semantic {
        MtgoOfflineBottomingActionSemanticV1::SelectForBottom {
            adapter_object_id,
            selection_ordinal,
        } => (adapter_object_id, selection_ordinal),
        _ => return Err("only a selected-card action can advance bottoming history".to_owned()),
    };
    let before_selected_count = session.selected_count_v5();
    if before_selected_count >= 6 || selection_ordinal != before_selected_count + 1 {
        return Err("bottoming selection ordinal does not advance the current state".to_owned());
    }
    let removed_index = session
        .current_cards
        .iter()
        .position(|card| card.adapter_object_id == adapter_object_id)
        .ok_or("selected bottoming object is not visible in the current state")?;

    if after.classification_v3() != MtgoOfflineBottomSixStateClassificationV3::Match
        || after.required_bottom_count_v3() != Some(6)
        || after.selected_count_v3() != Some(before_selected_count + 1)
        || after.visible_hand_count_v3()
            != Some(u8::try_from(session.current_cards.len() - 1).unwrap_or(u8::MAX))
        || after.done_visible_v3() != Some(before_selected_count + 1 == 6)
        || after.legal_action_count_v3()
            != Some(if before_selected_count + 1 == 6 {
                2
            } else {
                u8::try_from(session.current_cards.len()).unwrap_or(u8::MAX)
            })
        || after.profile_commitment_sha256_v3()
            != session.current_state.profile_commitment_sha256_v3()
    {
        return Err(
            "new bottoming state does not match the exact next count and controls".to_owned(),
        );
    }
    let before_capture = session.current_state.source_capture_commitments_v3();
    let after_capture = after.source_capture_commitments_v3();
    let before_transition_identity =
        pregame_transition_identity_commitment_v3(&session.current_state.source_frame.manifest)?;
    let after_transition_identity =
        pregame_transition_identity_commitment_v3(&after.source_frame.manifest)?;
    validate_pregame_capture_progression_v3(
        &before_capture,
        &before_transition_identity,
        &after_capture,
        &after_transition_identity,
    )?;

    let removed_card = session.current_cards.remove(removed_index);
    session
        .confirmed_bottomed_cards
        .push(ConfirmedBottomedCardV5 {
            card: removed_card,
            selection_ordinal,
        });

    let after_selected_count = before_selected_count + 1;
    let current_identity = if after_selected_count < 6 {
        let manifest = serialize_manifest_v2(&after.source_frame.manifest)?;
        let checked = check_untrusted_dxgi_capture_artifact_v1(
            &manifest,
            &after.source_frame.canonical_bgra8,
            &after.source_frame.preview_png,
        )
        .map_err(|error| format!("check next opaque bottoming capture: {error}"))?;
        let identity = classify_untrusted_offline_bottom_six_visible_card_identities_v3(
            &checked,
            &after.source_frame.canonical_bgra8,
            &session.profile,
        )
        .map_err(|error| format!("classify next opaque bottoming identities: {error}"))?;
        validate_current_identity_v5(&after, &identity, &session.profile, &session.current_cards)?;
        Some(identity)
    } else {
        None
    };
    let after_identity_commitment = current_identity
        .as_ref()
        .map(|identity| identity.candidate_commitment_sha256());
    let history_commitment_sha256 = advanced_history_commitment_v5(
        &session.history_commitment_sha256,
        &selection_commitment_sha256,
        &after_capture.capture_commitment_sha256,
        after.measurement_commitment_sha256_v3(),
        after_identity_commitment,
        &session.current_cards,
        &session.confirmed_bottomed_cards,
    )?;
    session.current_state = after;
    session.current_identity = current_identity;
    session.history_commitment_sha256 = history_commitment_sha256;
    validate_session_state_v5(&session)?;
    Ok(session)
}

fn validate_session_state_v5(
    session: &OpaqueMtgoCardAwareBottomingSessionV5,
) -> Result<(), String> {
    let selected_count = session.selected_count_v5();
    if selected_count > 6
        || session.current_cards.len() + session.confirmed_bottomed_cards.len() != 7
        || session.current_state.classification_v3()
            != MtgoOfflineBottomSixStateClassificationV3::Match
        || session.current_state.required_bottom_count_v3() != Some(6)
        || session.current_state.selected_count_v3() != Some(selected_count)
        || session.current_state.visible_hand_count_v3()
            != Some(u8::try_from(session.current_cards.len()).unwrap_or(u8::MAX))
        || session.current_state.done_visible_v3() != Some(selected_count == 6)
        || session.current_state.legal_action_count_v3()
            != Some(if selected_count == 6 {
                2
            } else {
                u8::try_from(session.current_cards.len() + 1).unwrap_or(u8::MAX)
            })
    {
        return Err("opaque bottoming session state is internally inconsistent".to_owned());
    }
    require_lower_sha256_v3(
        &session.history_commitment_sha256,
        "bottoming history commitment",
    )?;
    let mut object_ids = HashSet::new();
    let mut original_ordinals = HashSet::new();
    for card in session.current_cards.iter().chain(
        session
            .confirmed_bottomed_cards
            .iter()
            .map(|record| &record.card),
    ) {
        if !valid_adapter_object_id_v5(&card.adapter_object_id)
            || !valid_card_name_v5(&card.visible_card_name)
            || card.original_hand_ordinal > 6
            || !object_ids.insert(card.adapter_object_id.as_str())
            || !original_ordinals.insert(card.original_hand_ordinal)
        {
            return Err("bottoming session card identity is invalid or duplicated".to_owned());
        }
    }
    for (index, record) in session.confirmed_bottomed_cards.iter().enumerate() {
        if usize::from(record.selection_ordinal) != index + 1 {
            return Err("bottoming history selection ordinals are not canonical".to_owned());
        }
    }
    if selected_count < 6 {
        let identity = session
            .current_identity
            .as_ref()
            .ok_or("nonfinal bottoming state requires current visible identities")?;
        validate_current_identity_v5(
            &session.current_state,
            identity,
            &session.profile,
            &session.current_cards,
        )?;
    } else if session.current_identity.is_some() {
        return Err("final dimmed bottoming state must use confirmed action history".to_owned());
    }
    Ok(())
}

fn validate_current_identity_v5(
    state: &OpaqueMtgoDxgiBottomSixStateMeasurementV3,
    identity: &CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV3,
    profile: &CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
    expected_cards: &[StableBottomingCardV5],
) -> Result<(), String> {
    if identity.classification() != MtgoOfflineVisibleCardIdentityClassificationV1::Match
        || identity.visible_hand_count()
            != Some(u8::try_from(expected_cards.len()).unwrap_or(u8::MAX))
        || usize::from(identity.matched_identity_count()) != expected_cards.len()
        || identity.source_stage_commitment_sha256() != state.measurement_commitment_sha256_v3()
        || identity.profile_commitment_sha256() != profile.profile_commitment_sha256()
        || identity.identities().len() != expected_cards.len()
    {
        return Err("current bottoming identities do not cover the exact state".to_owned());
    }
    for (ordinal, (observed, expected)) in
        identity.identities().iter().zip(expected_cards).enumerate()
    {
        if usize::from(observed.ordinal()) != ordinal
            || observed.visible_card_name() != expected.visible_card_name
        {
            return Err(
                "current bottoming identities do not preserve action-bound visible order"
                    .to_owned(),
            );
        }
    }
    Ok(())
}

fn canonical_actions_v5(
    cards: &[MtgoBottomingVisibleCardV5],
    selected_count: u8,
) -> Vec<MtgoOfflineBottomingActionSemanticV1> {
    if selected_count == 6 {
        return vec![
            MtgoOfflineBottomingActionSemanticV1::SubmitBottoming,
            MtgoOfflineBottomingActionSemanticV1::CancelBottoming,
        ];
    }
    cards
        .iter()
        .map(
            |card| MtgoOfflineBottomingActionSemanticV1::SelectForBottom {
                adapter_object_id: card.adapter_object_id.clone(),
                selection_ordinal: selected_count + 1,
            },
        )
        .chain(std::iter::once(
            MtgoOfflineBottomingActionSemanticV1::CancelBottoming,
        ))
        .collect()
}

fn validate_card_aware_bottoming_scoring_request_v5(
    request: &MtgoCardAwareBottomingScoringRequestV5,
) -> Result<(), String> {
    if request.schema_version != MTGO_BOTTOMING_CARD_AWARE_SCORING_SCHEMA_V5
        || request.required_bottom_count != 6
        || request.selected_count > 6
        || request.ordered_visible_cards.len() != usize::from(7 - request.selected_count)
        || request.ordered_confirmed_bottomed_cards.len() != usize::from(request.selected_count)
    {
        return Err("card-aware bottoming request shape is invalid".to_owned());
    }
    for digest in [
        &request.source_capture_commitment_sha256,
        &request.state_measurement_commitment_sha256,
        &request.state_profile_commitment_sha256,
        &request.visible_card_profile_commitment_sha256,
        &request.history_commitment_sha256,
        &request.deployment_commitment_sha256,
    ] {
        require_lower_sha256_v3(digest, "card-aware bottoming request commitment")?;
    }
    match (
        request.selected_count,
        &request.current_identity_measurement_commitment_sha256,
    ) {
        (0..=5, Some(digest)) => {
            require_lower_sha256_v3(digest, "current bottoming identity commitment")?
        }
        (6, None) => {}
        _ => {
            return Err(
                "card-aware bottoming identity source does not match the current stage".to_owned(),
            )
        }
    }
    let expected_source = if request.selected_count < 6 {
        MtgoBottomingCardIdentitySourceV5::CurrentVisibleTemplate
    } else {
        MtgoBottomingCardIdentitySourceV5::ConfirmedActionHistory
    };
    let mut object_ids = HashSet::new();
    let mut original_ordinals = HashSet::new();
    for (ordinal, card) in request.ordered_visible_cards.iter().enumerate() {
        if usize::from(card.current_visible_ordinal) != ordinal
            || card.original_hand_ordinal > 6
            || card.identity_source != expected_source
            || !valid_adapter_object_id_v5(&card.adapter_object_id)
            || !valid_card_name_v5(&card.visible_card_name)
            || !object_ids.insert(card.adapter_object_id.as_str())
            || !original_ordinals.insert(card.original_hand_ordinal)
        {
            return Err("card-aware bottoming visible-card inventory is invalid".to_owned());
        }
    }
    for (index, card) in request.ordered_confirmed_bottomed_cards.iter().enumerate() {
        if usize::from(card.selection_ordinal) != index + 1
            || card.original_hand_ordinal > 6
            || !valid_adapter_object_id_v5(&card.adapter_object_id)
            || !valid_card_name_v5(&card.visible_card_name)
            || !object_ids.insert(card.adapter_object_id.as_str())
            || !original_ordinals.insert(card.original_hand_ordinal)
        {
            return Err("card-aware bottoming confirmed history is invalid".to_owned());
        }
    }
    if object_ids.len() != 7 || original_ordinals.len() != 7 {
        return Err("card-aware bottoming request must account for all seven objects".to_owned());
    }
    if request.ordered_actions
        != canonical_actions_v5(&request.ordered_visible_cards, request.selected_count)
    {
        return Err("card-aware bottoming actions are not canonical or complete".to_owned());
    }
    Ok(())
}

fn validate_card_aware_bottoming_score_response_parts_v5(
    request: &MtgoCardAwareBottomingScoringRequestV5,
    response: &MtgoCardAwareBottomingScoreResponseV5,
) -> Result<(usize, MtgoOfflineBottomingActionSemanticV1, String), String> {
    let request_commitment_sha256 = card_aware_bottoming_scoring_request_commitment_v5(request)?;
    if response.schema_version != MTGO_BOTTOMING_CARD_AWARE_SCORING_SCHEMA_V5
        || response.request_commitment_sha256 != request_commitment_sha256
    {
        return Err("bottoming score response does not bind the exact request".to_owned());
    }
    require_lower_sha256_v3(
        &response.request_commitment_sha256,
        "bottoming response request commitment",
    )?;
    if response.logits_f32_bits.len() != request.ordered_actions.len()
        || response.logits_f32_bits.is_empty()
    {
        return Err("bottoming score response logit count is invalid".to_owned());
    }
    let logits = response
        .logits_f32_bits
        .iter()
        .map(|bits| f32::from_bits(*bits))
        .collect::<Vec<_>>();
    if logits.iter().any(|value| !value.is_finite())
        || !f32::from_bits(response.value_f32_bits).is_finite()
    {
        return Err("bottoming score response must contain finite values".to_owned());
    }
    let mut selected_index = 0;
    for index in 1..logits.len() {
        if logits[index].total_cmp(&logits[selected_index]).is_gt() {
            selected_index = index;
        }
    }
    let selected_semantic = request.ordered_actions[selected_index].clone();
    #[derive(Serialize)]
    struct SelectionRecordV5<'a> {
        request: &'a MtgoCardAwareBottomingScoringRequestV5,
        response: &'a MtgoCardAwareBottomingScoreResponseV5,
        selected_index: usize,
        selected_semantic: &'a MtgoOfflineBottomingActionSemanticV1,
    }
    let selection_commitment_sha256 = canonical_json_commitment_v3(
        BOTTOMING_MODEL_SELECTION_DOMAIN_V5,
        &SelectionRecordV5 {
            request,
            response,
            selected_index,
            selected_semantic: &selected_semantic,
        },
    )?;
    Ok((
        selected_index,
        selected_semantic,
        selection_commitment_sha256,
    ))
}

fn stable_object_id_v5(
    source_capture_commitment_sha256: &str,
    identity_measurement_commitment_sha256: &str,
    original_hand_ordinal: u8,
    visible_card_name: &str,
) -> Result<String, String> {
    #[derive(Serialize)]
    struct RecordV5<'a> {
        source_capture_commitment_sha256: &'a str,
        identity_measurement_commitment_sha256: &'a str,
        original_hand_ordinal: u8,
        visible_card_name: &'a str,
    }
    let digest = canonical_json_commitment_v3(
        BOTTOMING_STABLE_OBJECT_DOMAIN_V5,
        &RecordV5 {
            source_capture_commitment_sha256,
            identity_measurement_commitment_sha256,
            original_hand_ordinal,
            visible_card_name,
        },
    )?;
    Ok(format!("mtgo-bottom-v5-{}", &digest[..32]))
}

fn initial_history_commitment_v5(
    source_capture_commitment_sha256: &str,
    state_measurement_commitment_sha256: &str,
    identity_measurement_commitment_sha256: &str,
    profile_commitment_sha256: &str,
    current_cards: &[StableBottomingCardV5],
) -> Result<String, String> {
    #[derive(Serialize)]
    struct RecordV5<'a> {
        history_kind: &'static str,
        source_capture_commitment_sha256: &'a str,
        state_measurement_commitment_sha256: &'a str,
        identity_measurement_commitment_sha256: &'a str,
        profile_commitment_sha256: &'a str,
        current_cards: &'a [StableBottomingCardV5],
    }
    canonical_json_commitment_v3(
        BOTTOMING_HISTORY_DOMAIN_V5,
        &RecordV5 {
            history_kind: "initial",
            source_capture_commitment_sha256,
            state_measurement_commitment_sha256,
            identity_measurement_commitment_sha256,
            profile_commitment_sha256,
            current_cards,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn advanced_history_commitment_v5(
    previous_history_commitment_sha256: &str,
    selection_commitment_sha256: &str,
    source_capture_commitment_sha256: &str,
    state_measurement_commitment_sha256: &str,
    identity_measurement_commitment_sha256: Option<&str>,
    current_cards: &[StableBottomingCardV5],
    confirmed_bottomed_cards: &[ConfirmedBottomedCardV5],
) -> Result<String, String> {
    #[derive(Serialize)]
    struct RecordV5<'a> {
        history_kind: &'static str,
        previous_history_commitment_sha256: &'a str,
        selection_commitment_sha256: &'a str,
        source_capture_commitment_sha256: &'a str,
        state_measurement_commitment_sha256: &'a str,
        identity_measurement_commitment_sha256: Option<&'a str>,
        current_cards: &'a [StableBottomingCardV5],
        confirmed_bottomed_cards: &'a [ConfirmedBottomedCardV5],
    }
    canonical_json_commitment_v3(
        BOTTOMING_HISTORY_DOMAIN_V5,
        &RecordV5 {
            history_kind: "advanced",
            previous_history_commitment_sha256,
            selection_commitment_sha256,
            source_capture_commitment_sha256,
            state_measurement_commitment_sha256,
            identity_measurement_commitment_sha256,
            current_cards,
            confirmed_bottomed_cards,
        },
    )
}

fn valid_adapter_object_id_v5(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn valid_card_name_v5(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deployment_v5() -> MtgoExpectedModelDeploymentV1 {
        MtgoExpectedModelDeploymentV1 {
            schema_version: mtgo_blackbox_v1::MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1,
            deployment_id: "bottoming-test-v5".to_owned(),
            checkpoint: mtgo_blackbox_v1::MtgoNativeCheckpointIdentityV1 {
                run_sha256: "1".repeat(64),
                checkpoint_manifest_sha256: "2".repeat(64),
                checkpoint_payload_sha256: "3".repeat(64),
                train_state_sha256: "4".repeat(64),
                model_parameter_sha256: "5".repeat(64),
                generation_index: 1,
            },
            scorer_contract_sha256: "6".repeat(64),
        }
    }

    fn request_v5(selected_count: u8) -> MtgoCardAwareBottomingScoringRequestV5 {
        let current_count = 7 - selected_count;
        let ordered_visible_cards = (0..current_count)
            .map(|ordinal| MtgoBottomingVisibleCardV5 {
                adapter_object_id: format!("object-{ordinal}"),
                original_hand_ordinal: ordinal,
                current_visible_ordinal: ordinal,
                visible_card_name: if ordinal % 2 == 0 {
                    "Plains".to_owned()
                } else {
                    "Island".to_owned()
                },
                identity_source: if selected_count < 6 {
                    MtgoBottomingCardIdentitySourceV5::CurrentVisibleTemplate
                } else {
                    MtgoBottomingCardIdentitySourceV5::ConfirmedActionHistory
                },
            })
            .collect::<Vec<_>>();
        let ordered_confirmed_bottomed_cards = (0..selected_count)
            .map(|index| MtgoBottomingConfirmedCardV5 {
                adapter_object_id: format!("object-{}", current_count + index),
                original_hand_ordinal: current_count + index,
                selection_ordinal: index + 1,
                visible_card_name: "Plains".to_owned(),
            })
            .collect::<Vec<_>>();
        let ordered_actions = canonical_actions_v5(&ordered_visible_cards, selected_count);
        MtgoCardAwareBottomingScoringRequestV5 {
            schema_version: MTGO_BOTTOMING_CARD_AWARE_SCORING_SCHEMA_V5,
            source_capture_commitment_sha256: "a".repeat(64),
            state_measurement_commitment_sha256: "b".repeat(64),
            current_identity_measurement_commitment_sha256: (selected_count < 6)
                .then(|| "c".repeat(64)),
            state_profile_commitment_sha256: "d".repeat(64),
            visible_card_profile_commitment_sha256: "e".repeat(64),
            history_commitment_sha256: "f".repeat(64),
            required_bottom_count: 6,
            selected_count,
            ordered_visible_cards,
            ordered_confirmed_bottomed_cards,
            ordered_actions,
            deployment_commitment_sha256: model_deployment_commitment_v1(&deployment_v5()).unwrap(),
        }
    }

    #[test]
    fn all_bottoming_request_stages_are_complete_and_canonical() {
        for selected_count in 0..=6 {
            let request = request_v5(selected_count);
            validate_card_aware_bottoming_scoring_request_v5(&request).unwrap();
            assert_eq!(
                request.ordered_actions.len(),
                if selected_count == 6 {
                    2
                } else {
                    usize::from(8 - selected_count)
                }
            );
            assert_eq!(
                card_aware_bottoming_scoring_request_commitment_v5(&request)
                    .unwrap()
                    .len(),
                64
            );
        }
    }

    #[test]
    fn bottoming_request_rejects_missing_object_wrong_order_and_forged_final_identity() {
        let mut missing = request_v5(2);
        missing.ordered_visible_cards.pop();
        assert!(validate_card_aware_bottoming_scoring_request_v5(&missing).is_err());

        let mut reordered = request_v5(2);
        reordered.ordered_visible_cards.swap(0, 1);
        assert!(validate_card_aware_bottoming_scoring_request_v5(&reordered).is_err());

        let mut forged_final = request_v5(6);
        forged_final.current_identity_measurement_commitment_sha256 = Some("c".repeat(64));
        assert!(validate_card_aware_bottoming_scoring_request_v5(&forged_final).is_err());
    }

    #[test]
    fn bottoming_response_binds_request_and_uses_stable_argmax() {
        let request = request_v5(5);
        let commitment = card_aware_bottoming_scoring_request_commitment_v5(&request).unwrap();
        let response = MtgoCardAwareBottomingScoreResponseV5 {
            schema_version: MTGO_BOTTOMING_CARD_AWARE_SCORING_SCHEMA_V5,
            request_commitment_sha256: commitment,
            logits_f32_bits: vec![0.25_f32.to_bits(), 0.75_f32.to_bits(), 0.0_f32.to_bits()],
            value_f32_bits: (-0.5_f32).to_bits(),
        };
        let (index, semantic, selection_commitment) =
            validate_card_aware_bottoming_score_response_parts_v5(&request, &response).unwrap();
        assert_eq!(index, 1);
        assert!(matches!(
            semantic,
            MtgoOfflineBottomingActionSemanticV1::SelectForBottom { .. }
        ));
        assert_eq!(selection_commitment.len(), 64);

        let stale = MtgoCardAwareBottomingScoreResponseV5 {
            request_commitment_sha256: "0".repeat(64),
            ..response.clone()
        };
        assert!(validate_card_aware_bottoming_score_response_parts_v5(&request, &stale).is_err());
        let nonfinite = MtgoCardAwareBottomingScoreResponseV5 {
            logits_f32_bits: vec![f32::NAN.to_bits(); request.ordered_actions.len()],
            ..response
        };
        assert!(
            validate_card_aware_bottoming_score_response_parts_v5(&request, &nonfinite).is_err()
        );
    }
}
