use crate::{
    mtgo_visible_duel_viewmodel_candidate_surface_v1, MtgoContractErrorV1,
    MtgoVisibleViewModelPropertyContextV1,
    MTGO_VISIBLE_DUEL_VIEWMODEL_CANDIDATE_SURFACE_COMMITMENT_V1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_VISIBLE_DUEL_VIEWMODEL_PRODUCER_AUDIT_SCHEMA_V1: u32 = 1;

const AUDIT_KIND_V1: &str = "mtgo_visible_duel_viewmodel_producer_audit_v1";
const INFORMATION_BOUNDARY_V1: &str = "seated_player_visible_ui_equivalent_only_v1";
const FIRST_EXPORTED_SUCCESS_SCHEMA_V1: &str = "mtgo_player_visible_duel_decision_input_v1";
const ROOT_TYPE_V1: &str = "Shiny.Play.Duel.DuelScene";
const ROOT_ACCESSOR_V1: &str = "FrameworkElement.DataContext";
const ROOT_VIEWMODEL_TYPE_V1: &str = "Shiny.Play.Duel.ViewModel.DuelSceneViewModel";
const AUDIT_DOMAIN_V1: &[u8] = b"mtgo-visible-duel-viewmodel-producer-audit-v1";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleDuelViewModelProducerPropertyV1 {
    pub assembly_file_name: String,
    pub declaring_type: String,
    pub property_name: String,
    pub context: MtgoVisibleViewModelPropertyContextV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleDuelViewModelProducerAuditV1 {
    pub schema_version: u32,
    pub audit_kind: String,
    pub information_boundary: String,
    pub candidate_surface_commitment_sha256: String,
    pub root_element_type: String,
    pub root_viewmodel_accessor: String,
    pub root_viewmodel_type: String,
    pub allowed_property_getters: Vec<MtgoVisibleDuelViewModelProducerPropertyV1>,
    pub first_exported_success_schema: String,
    pub emits_fixed_abstention_only_until_projection_complete: bool,
    pub unknown_types_reject: bool,
    pub unknown_properties_reject: bool,
    pub recursive_reflection_forbidden: bool,
    pub unrestricted_backing_object_traversal_present: bool,
    pub backing_objects_exported: bool,
    pub raw_values_exported: bool,
    pub raw_values_logged: bool,
    pub free_form_errors_exported: bool,
    pub network_access_present: bool,
    pub child_process_access_present: bool,
    pub file_write_access_present: bool,
    pub broker_created_bounded_local_memory_output_present: bool,
    pub maximum_output_bytes: u32,
    pub output_transport_status_contains_game_information: bool,
    pub action_execution_present: bool,
    pub producer_execution_attested: bool,
    pub full_projection_implemented: bool,
    pub safe_for_live_semantic_evidence: bool,
    pub safe_for_model_scoring: bool,
    pub safe_for_input: bool,
}

/// Compile-pinned audit of the managed producer boundary. This is source
/// structure, not a live-value or execution attestation. It cannot expose the
/// candidate list or create model or input authority.
pub struct CheckedUntrustedMtgoVisibleDuelViewModelProducerAuditV1 {
    commitment_sha256: String,
    allowed_property_count: usize,
}

impl CheckedUntrustedMtgoVisibleDuelViewModelProducerAuditV1 {
    pub fn commitment_sha256_v1(&self) -> &str {
        &self.commitment_sha256
    }

    pub fn allowed_property_count_v1(&self) -> usize {
        self.allowed_property_count
    }

    pub fn producer_execution_attested_v1(&self) -> bool {
        false
    }

    pub fn full_projection_implemented_v1(&self) -> bool {
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

pub fn mtgo_visible_duel_viewmodel_producer_audit_v1() -> MtgoVisibleDuelViewModelProducerAuditV1 {
    let surface = mtgo_visible_duel_viewmodel_candidate_surface_v1();
    let mut allowed_property_getters = surface
        .candidates
        .iter()
        .map(|candidate| MtgoVisibleDuelViewModelProducerPropertyV1 {
            assembly_file_name: candidate.assembly_file_name.clone(),
            declaring_type: candidate.declaring_type.clone(),
            property_name: candidate.property_name.clone(),
            context: candidate.context,
        })
        .collect::<Vec<_>>();
    allowed_property_getters.sort_by_key(property_key_v1);
    MtgoVisibleDuelViewModelProducerAuditV1 {
        schema_version: MTGO_VISIBLE_DUEL_VIEWMODEL_PRODUCER_AUDIT_SCHEMA_V1,
        audit_kind: AUDIT_KIND_V1.to_owned(),
        information_boundary: INFORMATION_BOUNDARY_V1.to_owned(),
        candidate_surface_commitment_sha256:
            MTGO_VISIBLE_DUEL_VIEWMODEL_CANDIDATE_SURFACE_COMMITMENT_V1.to_owned(),
        root_element_type: ROOT_TYPE_V1.to_owned(),
        root_viewmodel_accessor: ROOT_ACCESSOR_V1.to_owned(),
        root_viewmodel_type: ROOT_VIEWMODEL_TYPE_V1.to_owned(),
        allowed_property_getters,
        first_exported_success_schema: FIRST_EXPORTED_SUCCESS_SCHEMA_V1.to_owned(),
        emits_fixed_abstention_only_until_projection_complete: true,
        unknown_types_reject: true,
        unknown_properties_reject: true,
        recursive_reflection_forbidden: true,
        unrestricted_backing_object_traversal_present: false,
        backing_objects_exported: false,
        raw_values_exported: false,
        raw_values_logged: false,
        free_form_errors_exported: false,
        network_access_present: false,
        child_process_access_present: false,
        file_write_access_present: false,
        broker_created_bounded_local_memory_output_present: true,
        maximum_output_bytes: 1_048_576,
        output_transport_status_contains_game_information: false,
        action_execution_present: false,
        producer_execution_attested: false,
        full_projection_implemented: false,
        safe_for_live_semantic_evidence: false,
        safe_for_model_scoring: false,
        safe_for_input: false,
    }
}

pub fn check_untrusted_visible_duel_viewmodel_producer_audit_v1(
    audit: MtgoVisibleDuelViewModelProducerAuditV1,
) -> Result<CheckedUntrustedMtgoVisibleDuelViewModelProducerAuditV1, MtgoContractErrorV1> {
    if audit.schema_version != MTGO_VISIBLE_DUEL_VIEWMODEL_PRODUCER_AUDIT_SCHEMA_V1
        || audit.audit_kind != AUDIT_KIND_V1
        || audit.information_boundary != INFORMATION_BOUNDARY_V1
        || audit.candidate_surface_commitment_sha256
            != MTGO_VISIBLE_DUEL_VIEWMODEL_CANDIDATE_SURFACE_COMMITMENT_V1
        || audit.root_element_type != ROOT_TYPE_V1
        || audit.root_viewmodel_accessor != ROOT_ACCESSOR_V1
        || audit.root_viewmodel_type != ROOT_VIEWMODEL_TYPE_V1
        || audit.first_exported_success_schema != FIRST_EXPORTED_SUCCESS_SCHEMA_V1
    {
        return Err(error_v1(
            "visible_duel_viewmodel_producer_audit_header",
            "producer root, surface, information boundary, and output schema must be exact",
        ));
    }
    if !audit.emits_fixed_abstention_only_until_projection_complete
        || !audit.unknown_types_reject
        || !audit.unknown_properties_reject
        || !audit.recursive_reflection_forbidden
        || audit.unrestricted_backing_object_traversal_present
        || audit.backing_objects_exported
        || audit.raw_values_exported
        || audit.raw_values_logged
        || audit.free_form_errors_exported
        || audit.network_access_present
        || audit.child_process_access_present
        || audit.file_write_access_present
        || !audit.broker_created_bounded_local_memory_output_present
        || audit.maximum_output_bytes != 1_048_576
        || audit.output_transport_status_contains_game_information
        || audit.action_execution_present
        || audit.producer_execution_attested
        || audit.full_projection_implemented
        || audit.safe_for_live_semantic_evidence
        || audit.safe_for_model_scoring
        || audit.safe_for_input
    {
        return Err(error_v1(
            "visible_duel_viewmodel_producer_audit_boundary",
            "untrusted producer audit must reject unknown input and grant no unrestricted traversal, raw output, side effect, execution, model, or input authority",
        ));
    }

    let surface = mtgo_visible_duel_viewmodel_candidate_surface_v1();
    let expected = surface
        .candidates
        .iter()
        .map(|candidate| MtgoVisibleDuelViewModelProducerPropertyV1 {
            assembly_file_name: candidate.assembly_file_name.clone(),
            declaring_type: candidate.declaring_type.clone(),
            property_name: candidate.property_name.clone(),
            context: candidate.context,
        })
        .collect::<HashSet<_>>();
    let actual = audit
        .allowed_property_getters
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    if actual.len() != audit.allowed_property_getters.len() || actual != expected {
        return Err(error_v1(
            "visible_duel_viewmodel_producer_audit_allowlist",
            "producer getter allowlist must exactly match the reviewed candidate surface",
        ));
    }
    for forbidden in &surface.forbidden_properties {
        if audit.allowed_property_getters.iter().any(|property| {
            property.assembly_file_name == forbidden.assembly_file_name
                && property.declaring_type == forbidden.declaring_type
                && property.property_name == forbidden.property_name
        }) {
            return Err(error_v1(
                "visible_duel_viewmodel_producer_audit_forbidden_property",
                "backing-only property entered the exported visible-property allowlist",
            ));
        }
    }

    let canonical = serde_json::to_vec(&audit).map_err(|error| {
        error_v1(
            "visible_duel_viewmodel_producer_audit_serialization",
            error.to_string(),
        )
    })?;
    Ok(CheckedUntrustedMtgoVisibleDuelViewModelProducerAuditV1 {
        commitment_sha256: commitment_v1(AUDIT_DOMAIN_V1, &[&canonical]),
        allowed_property_count: audit.allowed_property_getters.len(),
    })
}

fn property_key_v1(property: &MtgoVisibleDuelViewModelProducerPropertyV1) -> String {
    format!(
        "{}\0{}\0{}",
        property.assembly_file_name, property.declaring_type, property.property_name
    )
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

    #[test]
    fn exact_compiled_producer_audit_is_non_authorizing() {
        let checked = check_untrusted_visible_duel_viewmodel_producer_audit_v1(
            mtgo_visible_duel_viewmodel_producer_audit_v1(),
        )
        .unwrap();
        assert_eq!(checked.allowed_property_count_v1(), 46);
        assert!(!checked.producer_execution_attested_v1());
        assert!(!checked.full_projection_implemented_v1());
        assert!(!checked.safe_for_live_semantic_evidence_v1());
        assert!(!checked.safe_for_model_scoring_v1());
        assert!(!checked.safe_for_input_v1());
    }

    #[test]
    fn forbidden_property_unknown_property_and_side_effect_reject() {
        let exact = mtgo_visible_duel_viewmodel_producer_audit_v1();

        let mut forbidden = exact.clone();
        forbidden.allowed_property_getters[0] = MtgoVisibleDuelViewModelProducerPropertyV1 {
            assembly_file_name: "DuelScene.dll".to_owned(),
            declaring_type: "Shiny.Play.Duel.ViewModel.DuelSceneViewModel".to_owned(),
            property_name: "Game".to_owned(),
            context: MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
        };
        assert!(check_untrusted_visible_duel_viewmodel_producer_audit_v1(forbidden).is_err());

        let mut unknown = exact.clone();
        unknown
            .allowed_property_getters
            .push(MtgoVisibleDuelViewModelProducerPropertyV1 {
                assembly_file_name: "Unexpected.dll".to_owned(),
                declaring_type: "Unexpected.Type".to_owned(),
                property_name: "Unexpected".to_owned(),
                context: MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            });
        assert!(check_untrusted_visible_duel_viewmodel_producer_audit_v1(unknown).is_err());

        let mut side_effect = exact;
        side_effect.file_write_access_present = true;
        assert_eq!(
            check_untrusted_visible_duel_viewmodel_producer_audit_v1(side_effect)
                .err()
                .unwrap()
                .code(),
            "visible_duel_viewmodel_producer_audit_boundary"
        );
    }
}
