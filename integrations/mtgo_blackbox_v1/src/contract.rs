use mtg_kernel::rl::{ActionSemanticV1, CardStableRefV1, ObservationV5};
use serde::{Deserialize, Serialize};

pub const MTGO_OBSERVED_DECISION_SCHEMA_V1: u32 = 1;
pub const MTGO_AUTHORIZATION_SCHEMA_V1: u32 = 1;
pub const MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1: u16 = 9_500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoRectPxV1 {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoMockFrameV1 {
    pub frame_id: u64,
    pub sequence: u64,
    pub sha256: String,
    pub client_bounds: MtgoRectPxV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoPublicDerivationV1 {
    CardNameNormalization,
    UiPromptReconciliation,
    ZoneIncarnation,
    LegalActionConstruction,
    PublicStateProjection,
    VisibleLogParsing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoEvidenceSourceV1 {
    FrameRegion {
        frame_id: u64,
        rect: MtgoRectPxV1,
        content_sha256: String,
    },
    VisibleGameLogText {
        frame_region_evidence_id: u64,
        text_sha256: String,
    },
    VisibleAccessibilityText {
        frame_region_evidence_id: u64,
        frame_id: u64,
        control_bounds: MtgoRectPxV1,
        is_offscreen: bool,
        automation_id_sha256: String,
        text_sha256: String,
    },
    ManualVisibleAnnotation {
        frame_region_evidence_id: u64,
        annotation_sha256: String,
    },
    DerivedPublicFact {
        parent_evidence_ids: Vec<u64>,
        derivation: MtgoPublicDerivationV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleEvidenceV1 {
    pub evidence_id: u64,
    pub sequence: u64,
    pub source: MtgoEvidenceSourceV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoObjectBindingV1 {
    pub adapter_object_id: String,
    pub kernel_ref: CardStableRefV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoSemanticDecisionPayloadV1 {
    pub observation: ObservationV5,
    pub legal_actions: Vec<ActionSemanticV1>,
    pub object_bindings: Vec<MtgoObjectBindingV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoLeafProvenanceV1 {
    pub json_pointer: String,
    pub value_sha256: String,
    pub evidence_ids: Vec<u64>,
    pub confidence_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDecisionReadinessV1 {
    pub observation_complete: bool,
    pub legal_action_set_complete: bool,
    pub client_prompt_reconciled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoObservedDecisionV1 {
    pub schema_version: u32,
    pub decision_id: String,
    pub frame_id: u64,
    pub payload: MtgoSemanticDecisionPayloadV1,
    pub frames: Vec<MtgoMockFrameV1>,
    pub evidence: Vec<MtgoVisibleEvidenceV1>,
    pub provenance: Vec<MtgoLeafProvenanceV1>,
    pub local_metadata_sha256: String,
    pub readiness: MtgoDecisionReadinessV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPayloadLeafV1 {
    pub json_pointer: String,
    pub value_sha256: String,
    pub requires_visible_evidence: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedMtgoObservedDecisionV1 {
    pub(crate) record: MtgoObservedDecisionV1,
    pub(crate) decision_commitment_sha256: String,
}

impl ValidatedMtgoObservedDecisionV1 {
    pub fn observation(&self) -> &ObservationV5 {
        &self.record.payload.observation
    }

    pub fn legal_actions(&self) -> &[ActionSemanticV1] {
        &self.record.payload.legal_actions
    }

    pub fn object_bindings(&self) -> &[MtgoObjectBindingV1] {
        &self.record.payload.object_bindings
    }

    pub fn frame_id(&self) -> u64 {
        self.record.frame_id
    }

    pub fn frame_sequence(&self) -> u64 {
        self.record
            .frames
            .iter()
            .find(|frame| frame.frame_id == self.record.frame_id)
            .map(|frame| frame.sequence)
            .expect("validated decisions always contain their decision frame")
    }

    pub fn decision_commitment_sha256(&self) -> &str {
        &self.decision_commitment_sha256
    }
}

/// Coordinate-free offline data. This is not authorization or an actuation proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoOfflineActionIntentV1 {
    pub schema_version: u32,
    pub decision_commitment_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub selected_index: u32,
    pub semantic: ActionSemanticV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoRuntimeModeV1 {
    OfflineReplay,
    ShadowObservation,
    PrivateMatchInput,
    OpenPlayInput,
    LeagueInput,
    ChallengeInput,
    OtherPrizeEventInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoAuthorizationScopeV1 {
    pub schema_version: u32,
    pub account_alias_sha256: String,
    pub written_permission_sha256: String,
    pub visible_channels_only: bool,
    pub shadow_observation: bool,
    pub private_match_input: bool,
    pub open_play_input: bool,
    pub league_input: bool,
    pub challenge_input: bool,
    pub other_prize_event_input: bool,
}

impl Default for MtgoAuthorizationScopeV1 {
    fn default() -> Self {
        Self {
            schema_version: MTGO_AUTHORIZATION_SCHEMA_V1,
            account_alias_sha256: String::new(),
            written_permission_sha256: String::new(),
            visible_channels_only: true,
            shadow_observation: false,
            private_match_input: false,
            open_play_input: false,
            league_input: false,
            challenge_input: false,
            other_prize_event_input: false,
        }
    }
}

impl MtgoAuthorizationScopeV1 {
    pub(crate) fn permits(&self, mode: MtgoRuntimeModeV1) -> bool {
        if self.schema_version != MTGO_AUTHORIZATION_SCHEMA_V1 || !self.visible_channels_only {
            return false;
        }
        if mode != MtgoRuntimeModeV1::OfflineReplay
            && (!looks_like_sha256_v1(&self.account_alias_sha256)
                || !looks_like_sha256_v1(&self.written_permission_sha256))
        {
            return false;
        }
        match mode {
            MtgoRuntimeModeV1::OfflineReplay => true,
            MtgoRuntimeModeV1::ShadowObservation => self.shadow_observation,
            MtgoRuntimeModeV1::PrivateMatchInput => self.private_match_input,
            MtgoRuntimeModeV1::OpenPlayInput => self.open_play_input,
            MtgoRuntimeModeV1::LeagueInput => self.league_input,
            MtgoRuntimeModeV1::ChallengeInput => self.challenge_input,
            MtgoRuntimeModeV1::OtherPrizeEventInput => self.other_prize_event_input,
        }
    }
}

fn looks_like_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
