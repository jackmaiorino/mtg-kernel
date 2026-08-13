use crate::{
    validate_player_visible_duel_decision_input_strict_v1, validate_player_visible_expected_v2,
    MtgoContractErrorV1, MtgoPlayerVisibleDuelDecisionInputV1,
    MTGO_VISIBLE_DUEL_VIEWMODEL_CANDIDATE_SURFACE_COMMITMENT_V1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MTGO_VISIBLE_DUEL_VIEWMODEL_BROKER_PROTOCOL_SCHEMA_V1: u32 = 1;

const REQUEST_KIND_V1: &str = "mtgo_visible_duel_viewmodel_broker_request_v1";
const RESPONSE_KIND_V1: &str = "mtgo_visible_duel_viewmodel_broker_response_v1";
const INFORMATION_BOUNDARY_V1: &str = "seated_player_visible_ui_equivalent_only_v1";
const REQUEST_DOMAIN_V1: &[u8] = b"mtgo-visible-duel-viewmodel-broker-request-v1";
const RESPONSE_DOMAIN_V1: &[u8] = b"mtgo-visible-duel-viewmodel-broker-response-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoVisibleDuelViewModelBrokerAbstentionReasonV1 {
    ClientIdentityMismatch,
    DuelSurfaceUnavailable,
    SurfaceShapeMismatch,
    VisibilityContextUnconfirmed,
    UiCorpusQualificationMissing,
    ProjectionIncomplete,
    VisibleActionSetIncomplete,
    CaptureBracketChanged,
    OutputValidationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleDuelViewModelBrokerRequestV1 {
    pub schema_version: u32,
    pub request_kind: String,
    pub information_boundary: String,
    pub transaction_nonce_sha256: String,
    pub candidate_surface_commitment_sha256: String,
    pub broker_binary_sha256: String,
    pub producer_binary_sha256: String,
    pub before_frame_commitment_sha256: String,
    pub before_frame_pixels_sha256: String,
    pub before_visible_projection_regions_sha256: String,
    pub before_frame_sequence: u64,
    pub client_identity_commitment_sha256: String,
    pub capture_role: String,
    pub expected_first_exported_schema: String,
    pub request_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoVisibleDuelViewModelBrokerResultV1 {
    VisibleDecision {
        decision: Box<MtgoPlayerVisibleDuelDecisionInputV1>,
    },
    Abstained {
        reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleDuelViewModelBrokerResponseV1 {
    pub schema_version: u32,
    pub response_kind: String,
    pub information_boundary: String,
    pub request_commitment_sha256: String,
    pub transaction_nonce_sha256: String,
    pub candidate_surface_commitment_sha256: String,
    pub broker_binary_sha256: String,
    pub producer_binary_sha256: String,
    pub before_frame_commitment_sha256: String,
    pub after_frame_commitment_sha256: String,
    pub after_frame_pixels_sha256: String,
    pub after_visible_projection_regions_sha256: String,
    pub before_frame_sequence: u64,
    pub after_frame_sequence: u64,
    pub client_identity_commitment_sha256: String,
    pub capture_role: String,
    pub result: MtgoVisibleDuelViewModelBrokerResultV1,
    pub raw_source_values_emitted: bool,
    pub internal_identifiers_emitted: bool,
    pub free_form_diagnostics_emitted: bool,
    pub response_commitment_sha256: String,
    pub safe_for_live_semantic_evidence: bool,
    pub safe_for_model_scoring: bool,
    pub safe_for_input: bool,
}

/// A strictly parsed broker request. It contains only immutable identities and
/// visible-frame commitments. It carries no path, process handle, client value,
/// account alias, event identifier, or input authority.
pub struct CheckedUntrustedMtgoVisibleDuelViewModelBrokerRequestV1 {
    request: MtgoVisibleDuelViewModelBrokerRequestV1,
}

impl CheckedUntrustedMtgoVisibleDuelViewModelBrokerRequestV1 {
    pub fn request_commitment_sha256_v1(&self) -> &str {
        &self.request.request_commitment_sha256
    }

    pub fn before_frame_sequence_v1(&self) -> u64 {
        self.request.before_frame_sequence
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

/// A move-only, structurally checked response. The only data-bearing result is
/// the already sanitized player-visible decision schema. An abstention is a
/// fixed enum with no free-form text. This wrapper is still untrusted because
/// it does not attest how a future broker acquired or filtered live values.
pub struct CheckedUntrustedMtgoVisibleDuelViewModelBrokerResponseV1 {
    response: MtgoVisibleDuelViewModelBrokerResponseV1,
}

impl CheckedUntrustedMtgoVisibleDuelViewModelBrokerResponseV1 {
    pub fn response_commitment_sha256_v1(&self) -> &str {
        &self.response.response_commitment_sha256
    }

    pub fn abstention_reason_v1(&self) -> Option<MtgoVisibleDuelViewModelBrokerAbstentionReasonV1> {
        match self.response.result {
            MtgoVisibleDuelViewModelBrokerResultV1::Abstained { reason } => Some(reason),
            MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { .. } => None,
        }
    }

    pub fn visible_decision_commitment_sha256_v1(
        &self,
    ) -> Result<Option<String>, MtgoContractErrorV1> {
        match &self.response.result {
            MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { decision } => {
                decision.commitment_sha256_v1().map(Some)
            }
            MtgoVisibleDuelViewModelBrokerResultV1::Abstained { .. } => Ok(None),
        }
    }

    pub fn producer_execution_attested_v1(&self) -> bool {
        false
    }

    pub fn safe_for_live_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub fn mtgo_visible_duel_viewmodel_broker_request_commitment_v1(
    request: &MtgoVisibleDuelViewModelBrokerRequestV1,
) -> Result<String, MtgoContractErrorV1> {
    let mut copy = request.clone();
    copy.request_commitment_sha256.clear();
    let canonical = serde_json::to_vec(&copy).map_err(|error| {
        error_v1(
            "visible_duel_viewmodel_broker_request_serialization",
            error.to_string(),
        )
    })?;
    Ok(commitment_v1(REQUEST_DOMAIN_V1, &[&canonical]))
}

pub fn mtgo_visible_duel_viewmodel_broker_response_commitment_v1(
    response: &MtgoVisibleDuelViewModelBrokerResponseV1,
) -> Result<String, MtgoContractErrorV1> {
    let mut copy = response.clone();
    copy.response_commitment_sha256.clear();
    let canonical = serde_json::to_vec(&copy).map_err(|error| {
        error_v1(
            "visible_duel_viewmodel_broker_response_serialization",
            error.to_string(),
        )
    })?;
    Ok(commitment_v1(RESPONSE_DOMAIN_V1, &[&canonical]))
}

pub fn check_untrusted_visible_duel_viewmodel_broker_request_v1(
    request: MtgoVisibleDuelViewModelBrokerRequestV1,
) -> Result<CheckedUntrustedMtgoVisibleDuelViewModelBrokerRequestV1, MtgoContractErrorV1> {
    if request.schema_version != MTGO_VISIBLE_DUEL_VIEWMODEL_BROKER_PROTOCOL_SCHEMA_V1
        || request.request_kind != REQUEST_KIND_V1
        || request.information_boundary != INFORMATION_BOUNDARY_V1
        || request.candidate_surface_commitment_sha256
            != MTGO_VISIBLE_DUEL_VIEWMODEL_CANDIDATE_SURFACE_COMMITMENT_V1
        || request.capture_role != "acting_player_duel"
        || request.expected_first_exported_schema != "mtgo_player_visible_duel_decision_input_v1"
        || request.before_frame_sequence == 0
    {
        return Err(error_v1(
            "visible_duel_viewmodel_broker_request_header",
            "request schema, visible boundary, surface, capture role, output schema, and sequence must be exact",
        ));
    }
    for digest in [
        &request.transaction_nonce_sha256,
        &request.candidate_surface_commitment_sha256,
        &request.broker_binary_sha256,
        &request.producer_binary_sha256,
        &request.before_frame_commitment_sha256,
        &request.before_frame_pixels_sha256,
        &request.before_visible_projection_regions_sha256,
        &request.client_identity_commitment_sha256,
        &request.request_commitment_sha256,
    ] {
        validate_sha256_v1(digest)?;
    }
    if request.request_commitment_sha256
        != mtgo_visible_duel_viewmodel_broker_request_commitment_v1(&request)?
    {
        return Err(error_v1(
            "visible_duel_viewmodel_broker_request_commitment",
            "request commitment does not match the exact bounded request",
        ));
    }
    Ok(CheckedUntrustedMtgoVisibleDuelViewModelBrokerRequestV1 { request })
}

pub fn check_untrusted_visible_duel_viewmodel_broker_response_v1(
    request: &CheckedUntrustedMtgoVisibleDuelViewModelBrokerRequestV1,
    response: MtgoVisibleDuelViewModelBrokerResponseV1,
) -> Result<CheckedUntrustedMtgoVisibleDuelViewModelBrokerResponseV1, MtgoContractErrorV1> {
    if response.schema_version != MTGO_VISIBLE_DUEL_VIEWMODEL_BROKER_PROTOCOL_SCHEMA_V1
        || response.response_kind != RESPONSE_KIND_V1
        || response.information_boundary != INFORMATION_BOUNDARY_V1
        || response.request_commitment_sha256 != request.request.request_commitment_sha256
        || response.transaction_nonce_sha256 != request.request.transaction_nonce_sha256
        || response.candidate_surface_commitment_sha256
            != request.request.candidate_surface_commitment_sha256
        || response.broker_binary_sha256 != request.request.broker_binary_sha256
        || response.producer_binary_sha256 != request.request.producer_binary_sha256
        || response.before_frame_commitment_sha256 != request.request.before_frame_commitment_sha256
        || response.after_visible_projection_regions_sha256
            != request.request.before_visible_projection_regions_sha256
        || response.before_frame_sequence != request.request.before_frame_sequence
        || response.client_identity_commitment_sha256
            != request.request.client_identity_commitment_sha256
        || response.capture_role != request.request.capture_role
        || response.after_frame_sequence <= response.before_frame_sequence
    {
        return Err(error_v1(
            "visible_duel_viewmodel_broker_response_binding",
            "response must bind the exact request identities and a strictly newer frame",
        ));
    }
    for digest in [
        &response.request_commitment_sha256,
        &response.transaction_nonce_sha256,
        &response.candidate_surface_commitment_sha256,
        &response.broker_binary_sha256,
        &response.producer_binary_sha256,
        &response.before_frame_commitment_sha256,
        &response.after_frame_commitment_sha256,
        &response.after_frame_pixels_sha256,
        &response.after_visible_projection_regions_sha256,
        &response.client_identity_commitment_sha256,
        &response.response_commitment_sha256,
    ] {
        validate_sha256_v1(digest)?;
    }
    if response.raw_source_values_emitted
        || response.internal_identifiers_emitted
        || response.free_form_diagnostics_emitted
        || response.safe_for_live_semantic_evidence
        || response.safe_for_model_scoring
        || response.safe_for_input
    {
        return Err(error_v1(
            "visible_duel_viewmodel_broker_response_boundary",
            "raw values, identifiers, free-form diagnostics, and live authority must all be absent",
        ));
    }
    if let MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { decision } = &response.result {
        validate_player_visible_expected_v2(decision)?;
        validate_player_visible_duel_decision_input_strict_v1(decision)?;
    }
    if response.response_commitment_sha256
        != mtgo_visible_duel_viewmodel_broker_response_commitment_v1(&response)?
    {
        return Err(error_v1(
            "visible_duel_viewmodel_broker_response_commitment",
            "response commitment does not match the exact bounded response",
        ));
    }
    Ok(CheckedUntrustedMtgoVisibleDuelViewModelBrokerResponseV1 { response })
}

fn validate_sha256_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(error_v1(
            "visible_duel_viewmodel_broker_digest",
            "broker protocol digests must be lowercase SHA-256",
        ));
    }
    Ok(())
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
    use crate::{
        MtgoPlayerRelativeRoleV1, MtgoPlayerVisibleCombatStateV1, MtgoPlayerVisibleDuelActionV1,
        MtgoPlayerVisibleDuelStateV1, ZoneIndependentStepV1,
    };

    fn request_v1() -> MtgoVisibleDuelViewModelBrokerRequestV1 {
        let mut request = MtgoVisibleDuelViewModelBrokerRequestV1 {
            schema_version: MTGO_VISIBLE_DUEL_VIEWMODEL_BROKER_PROTOCOL_SCHEMA_V1,
            request_kind: REQUEST_KIND_V1.to_owned(),
            information_boundary: INFORMATION_BOUNDARY_V1.to_owned(),
            transaction_nonce_sha256: "1".repeat(64),
            candidate_surface_commitment_sha256:
                MTGO_VISIBLE_DUEL_VIEWMODEL_CANDIDATE_SURFACE_COMMITMENT_V1.to_owned(),
            broker_binary_sha256: "2".repeat(64),
            producer_binary_sha256: "3".repeat(64),
            before_frame_commitment_sha256: "4".repeat(64),
            before_frame_pixels_sha256: "5".repeat(64),
            before_visible_projection_regions_sha256: "a".repeat(64),
            before_frame_sequence: 41,
            client_identity_commitment_sha256: "6".repeat(64),
            capture_role: "acting_player_duel".to_owned(),
            expected_first_exported_schema: "mtgo_player_visible_duel_decision_input_v1".to_owned(),
            request_commitment_sha256: String::new(),
        };
        request.request_commitment_sha256 =
            mtgo_visible_duel_viewmodel_broker_request_commitment_v1(&request).unwrap();
        request
    }

    fn decision_v1() -> MtgoPlayerVisibleDuelDecisionInputV1 {
        MtgoPlayerVisibleDuelDecisionInputV1 {
            current_state: MtgoPlayerVisibleDuelStateV1 {
                acting_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                turn: 1,
                phase: ZoneIndependentStepV1::Main1,
                active_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                initiative: None,
                life_totals: [20, 20],
                mana_pools: [[0; 6], [0; 6]],
                hand_counts: [0, 0],
                library_counts: [53, 53],
                battlefield: [Vec::new(), Vec::new()],
                graveyards: [Vec::new(), Vec::new()],
                exile: Vec::new(),
                stack: Vec::new(),
                combat: MtgoPlayerVisibleCombatStateV1 {
                    attackers_declared: false,
                    blockers_declared: false,
                    ordered_attackers: Vec::new(),
                    blocker_assignments: Vec::new(),
                },
                visible_object_relations: Vec::new(),
                own_hand: Vec::new(),
                known_library_cards: [Vec::new(), Vec::new()],
                known_hand_cards: [Vec::new(), Vec::new()],
            },
            ordered_legal_actions: vec![MtgoPlayerVisibleDuelActionV1::Pass {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            }],
        }
    }

    fn response_v1(
        request: &MtgoVisibleDuelViewModelBrokerRequestV1,
        result: MtgoVisibleDuelViewModelBrokerResultV1,
    ) -> MtgoVisibleDuelViewModelBrokerResponseV1 {
        let mut response = MtgoVisibleDuelViewModelBrokerResponseV1 {
            schema_version: MTGO_VISIBLE_DUEL_VIEWMODEL_BROKER_PROTOCOL_SCHEMA_V1,
            response_kind: RESPONSE_KIND_V1.to_owned(),
            information_boundary: INFORMATION_BOUNDARY_V1.to_owned(),
            request_commitment_sha256: request.request_commitment_sha256.clone(),
            transaction_nonce_sha256: request.transaction_nonce_sha256.clone(),
            candidate_surface_commitment_sha256: request
                .candidate_surface_commitment_sha256
                .clone(),
            broker_binary_sha256: request.broker_binary_sha256.clone(),
            producer_binary_sha256: request.producer_binary_sha256.clone(),
            before_frame_commitment_sha256: request.before_frame_commitment_sha256.clone(),
            after_frame_commitment_sha256: "7".repeat(64),
            after_frame_pixels_sha256: "8".repeat(64),
            after_visible_projection_regions_sha256: request
                .before_visible_projection_regions_sha256
                .clone(),
            before_frame_sequence: request.before_frame_sequence,
            after_frame_sequence: request.before_frame_sequence + 1,
            client_identity_commitment_sha256: request.client_identity_commitment_sha256.clone(),
            capture_role: request.capture_role.clone(),
            result,
            raw_source_values_emitted: false,
            internal_identifiers_emitted: false,
            free_form_diagnostics_emitted: false,
            response_commitment_sha256: String::new(),
            safe_for_live_semantic_evidence: false,
            safe_for_model_scoring: false,
            safe_for_input: false,
        };
        response.response_commitment_sha256 =
            mtgo_visible_duel_viewmodel_broker_response_commitment_v1(&response).unwrap();
        response
    }

    #[test]
    fn exact_decision_and_fixed_abstention_are_checked_but_unattested() {
        let raw_request = request_v1();
        let request =
            check_untrusted_visible_duel_viewmodel_broker_request_v1(raw_request.clone()).unwrap();
        assert_eq!(request.before_frame_sequence_v1(), 41);
        assert!(!request.safe_for_input_v1());

        let decision = check_untrusted_visible_duel_viewmodel_broker_response_v1(
            &request,
            response_v1(
                &raw_request,
                MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision {
                    decision: Box::new(decision_v1()),
                },
            ),
        )
        .unwrap();
        assert!(decision
            .visible_decision_commitment_sha256_v1()
            .unwrap()
            .is_some());
        assert_eq!(decision.abstention_reason_v1(), None);
        assert!(!decision.producer_execution_attested_v1());
        assert!(!decision.safe_for_model_scoring_v1());
        assert!(!decision.safe_for_input_v1());

        let abstention = check_untrusted_visible_duel_viewmodel_broker_response_v1(
            &request,
            response_v1(
                &raw_request,
                MtgoVisibleDuelViewModelBrokerResultV1::Abstained {
                    reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::ProjectionIncomplete,
                },
            ),
        )
        .unwrap();
        assert_eq!(
            abstention.abstention_reason_v1(),
            Some(MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::ProjectionIncomplete)
        );
        assert_eq!(
            abstention.visible_decision_commitment_sha256_v1().unwrap(),
            None
        );
    }

    #[test]
    fn unknown_hidden_or_diagnostic_fields_cannot_deserialize() {
        let request = request_v1();
        let mut response = serde_json::to_value(response_v1(
            &request,
            MtgoVisibleDuelViewModelBrokerResultV1::Abstained {
                reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::ProjectionIncomplete,
            },
        ))
        .unwrap();
        for (name, value) in [
            ("opponent_hidden_hand", serde_json::json!(["Island"])),
            ("game_card_id", serde_json::json!(1234)),
            ("raw_view_model", serde_json::json!({"Game": {}})),
            ("debug_text", serde_json::json!("raw client error")),
            ("source_path", serde_json::json!("C:\\private")),
        ] {
            let mut mutated = response.clone();
            mutated
                .as_object_mut()
                .expect("response object")
                .insert(name.to_owned(), value);
            assert!(
                serde_json::from_value::<MtgoVisibleDuelViewModelBrokerResponseV1>(mutated)
                    .is_err()
            );
        }
        response["result"] = serde_json::json!({
            "result_kind": "abstained",
            "reason": "projection_incomplete",
            "detail": "GameCard 1234"
        });
        assert!(
            serde_json::from_value::<MtgoVisibleDuelViewModelBrokerResponseV1>(response).is_err()
        );
    }

    #[test]
    fn crossed_identity_stale_frame_and_authority_broadening_reject() {
        let raw_request = request_v1();
        let request =
            check_untrusted_visible_duel_viewmodel_broker_request_v1(raw_request.clone()).unwrap();

        let mut crossed = response_v1(
            &raw_request,
            MtgoVisibleDuelViewModelBrokerResultV1::Abstained {
                reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::ProjectionIncomplete,
            },
        );
        crossed.client_identity_commitment_sha256 = "9".repeat(64);
        crossed.response_commitment_sha256 =
            mtgo_visible_duel_viewmodel_broker_response_commitment_v1(&crossed).unwrap();
        assert_eq!(
            check_untrusted_visible_duel_viewmodel_broker_response_v1(&request, crossed)
                .err()
                .unwrap()
                .code(),
            "visible_duel_viewmodel_broker_response_binding"
        );

        let mut stale = response_v1(
            &raw_request,
            MtgoVisibleDuelViewModelBrokerResultV1::Abstained {
                reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::ProjectionIncomplete,
            },
        );
        stale.after_frame_sequence = stale.before_frame_sequence;
        stale.response_commitment_sha256 =
            mtgo_visible_duel_viewmodel_broker_response_commitment_v1(&stale).unwrap();
        assert_eq!(
            check_untrusted_visible_duel_viewmodel_broker_response_v1(&request, stale)
                .err()
                .unwrap()
                .code(),
            "visible_duel_viewmodel_broker_response_binding"
        );

        let mut visible_drift = response_v1(
            &raw_request,
            MtgoVisibleDuelViewModelBrokerResultV1::Abstained {
                reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::ProjectionIncomplete,
            },
        );
        visible_drift.after_visible_projection_regions_sha256 = "b".repeat(64);
        visible_drift.response_commitment_sha256 =
            mtgo_visible_duel_viewmodel_broker_response_commitment_v1(&visible_drift).unwrap();
        assert_eq!(
            check_untrusted_visible_duel_viewmodel_broker_response_v1(&request, visible_drift)
                .err()
                .unwrap()
                .code(),
            "visible_duel_viewmodel_broker_response_binding"
        );

        let mut authority = response_v1(
            &raw_request,
            MtgoVisibleDuelViewModelBrokerResultV1::Abstained {
                reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::ProjectionIncomplete,
            },
        );
        authority.safe_for_model_scoring = true;
        authority.response_commitment_sha256 =
            mtgo_visible_duel_viewmodel_broker_response_commitment_v1(&authority).unwrap();
        assert_eq!(
            check_untrusted_visible_duel_viewmodel_broker_response_v1(&request, authority)
                .err()
                .unwrap()
                .code(),
            "visible_duel_viewmodel_broker_response_boundary"
        );
    }
}
