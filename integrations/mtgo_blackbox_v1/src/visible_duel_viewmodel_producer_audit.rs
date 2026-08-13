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
const PRIVATE_VISIBLE_ACTION_JOIN_GETTER_COUNT_V1: u32 = 31;
const REFERENCE_ASSEMBLY_SHA256_V1: &str =
    "f3fef1adfd5b1b6d25a5db577f9a1b184c8b91bb98f19a13428c669266c20dc8";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleDuelViewModelProducerPropertyV1 {
    pub assembly_file_name: String,
    pub declaring_type: String,
    pub property_name: String,
    pub context: MtgoVisibleViewModelPropertyContextV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoPrivateVisibleActionJoinBindingV1 {
    VisibleEnabledPromptControl,
    SeatedPlayerVisibleCard,
    BoundVisibleActionObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPrivateVisibleActionJoinGetterV1 {
    pub assembly_file_name: String,
    pub declaring_type: String,
    pub property_name: String,
    pub binding: MtgoPrivateVisibleActionJoinBindingV1,
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
    pub private_visible_action_join_getters: Vec<MtgoPrivateVisibleActionJoinGetterV1>,
    pub private_visible_action_join_reference_assembly_sha256: String,
    pub first_exported_success_schema: String,
    pub emits_fixed_abstention_only_until_projection_complete: bool,
    pub sanitized_player_visible_decision_slice_present: bool,
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
    pub visible_chrome_getter_layer_present: bool,
    pub visible_chrome_getter_count: u32,
    pub visible_chrome_values_exported: bool,
    pub visible_zone_and_card_getter_layer_present: bool,
    pub visible_zone_and_card_getter_count: u32,
    pub visible_zone_and_card_values_exported: bool,
    pub private_visible_action_join_layer_present: bool,
    pub private_visible_action_join_getter_count: u32,
    pub private_action_objects_exported: bool,
    pub private_action_identifiers_exported: bool,
    pub opponent_action_collections_inspected: bool,
    pub private_action_joins_bound_to_visible_sources: bool,
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
    private_visible_action_join_getter_count: usize,
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

    pub fn visible_chrome_getter_layer_present_v1(&self) -> bool {
        true
    }

    pub fn visible_chrome_getter_count_v1(&self) -> u32 {
        19
    }

    pub fn visible_zone_and_card_getter_layer_present_v1(&self) -> bool {
        true
    }

    pub fn visible_zone_and_card_getter_count_v1(&self) -> u32 {
        25
    }

    pub fn private_visible_action_join_layer_present_v1(&self) -> bool {
        true
    }

    pub fn private_visible_action_join_getter_count_v1(&self) -> usize {
        self.private_visible_action_join_getter_count
    }

    pub fn offline_sealed_action_dispatch_present_v1(&self) -> bool {
        true
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
    let mut private_visible_action_join_getters = private_visible_action_join_getters_v1();
    private_visible_action_join_getters.sort_by_key(private_action_join_key_v1);
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
        private_visible_action_join_getters,
        private_visible_action_join_reference_assembly_sha256: REFERENCE_ASSEMBLY_SHA256_V1
            .to_owned(),
        first_exported_success_schema: FIRST_EXPORTED_SUCCESS_SCHEMA_V1.to_owned(),
        emits_fixed_abstention_only_until_projection_complete: false,
        sanitized_player_visible_decision_slice_present: true,
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
        visible_chrome_getter_layer_present: true,
        visible_chrome_getter_count: 19,
        visible_chrome_values_exported: true,
        visible_zone_and_card_getter_layer_present: true,
        visible_zone_and_card_getter_count: 25,
        visible_zone_and_card_values_exported: true,
        private_visible_action_join_layer_present: true,
        private_visible_action_join_getter_count: PRIVATE_VISIBLE_ACTION_JOIN_GETTER_COUNT_V1,
        private_action_objects_exported: false,
        private_action_identifiers_exported: false,
        opponent_action_collections_inspected: false,
        private_action_joins_bound_to_visible_sources: true,
        action_execution_present: true,
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
        || audit.private_visible_action_join_reference_assembly_sha256
            != REFERENCE_ASSEMBLY_SHA256_V1
        || audit.first_exported_success_schema != FIRST_EXPORTED_SUCCESS_SCHEMA_V1
    {
        return Err(error_v1(
            "visible_duel_viewmodel_producer_audit_header",
            "producer root, surface, information boundary, and output schema must be exact",
        ));
    }
    if audit.emits_fixed_abstention_only_until_projection_complete
        || !audit.sanitized_player_visible_decision_slice_present
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
        || !audit.visible_chrome_getter_layer_present
        || audit.visible_chrome_getter_count != 19
        || !audit.visible_chrome_values_exported
        || !audit.visible_zone_and_card_getter_layer_present
        || audit.visible_zone_and_card_getter_count != 25
        || !audit.visible_zone_and_card_values_exported
        || !audit.private_visible_action_join_layer_present
        || audit.private_visible_action_join_getter_count
            != PRIVATE_VISIBLE_ACTION_JOIN_GETTER_COUNT_V1
        || audit.private_action_objects_exported
        || audit.private_action_identifiers_exported
        || audit.opponent_action_collections_inspected
        || !audit.private_action_joins_bound_to_visible_sources
        || !audit.action_execution_present
        || audit.producer_execution_attested
        || audit.full_projection_implemented
        || audit.safe_for_live_semantic_evidence
        || audit.safe_for_model_scoring
        || audit.safe_for_input
    {
        return Err(error_v1(
            "visible_duel_viewmodel_producer_audit_boundary",
            "untrusted producer audit must require the exact bounded dispatch seam, reject unknown input, and grant no unrestricted traversal, raw output, execution, model, or live-input authority",
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

    let expected_private = private_visible_action_join_getters_v1()
        .into_iter()
        .collect::<HashSet<_>>();
    let actual_private = audit
        .private_visible_action_join_getters
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    if actual_private.len() != audit.private_visible_action_join_getters.len()
        || actual_private != expected_private
    {
        return Err(error_v1(
            "visible_duel_viewmodel_producer_audit_private_action_join_allowlist",
            "private action join getters must exactly match the reviewed visible-source-bound surface",
        ));
    }

    for (declaring_type, property_name) in [
        (
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "Actions",
        ),
        ("Shiny.Play.Duel.ViewModel.OptionButton", "Action"),
    ] {
        if !surface.forbidden_properties.iter().any(|property| {
            property.assembly_file_name == "DuelScene.dll"
                && property.declaring_type == declaring_type
                && property.property_name == property_name
        }) {
            return Err(error_v1(
                "visible_duel_viewmodel_producer_audit_private_action_export_boundary",
                "raw action roots must remain forbidden from the exported visible-property surface",
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
        private_visible_action_join_getter_count: audit.private_visible_action_join_getters.len(),
    })
}

fn private_visible_action_join_getters_v1() -> Vec<MtgoPrivateVisibleActionJoinGetterV1> {
    use MtgoPrivateVisibleActionJoinBindingV1::{
        BoundVisibleActionObject, SeatedPlayerVisibleCard, VisibleEnabledPromptControl,
    };

    [
        (
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "Actions",
            SeatedPlayerVisibleCard,
        ),
        (
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "Game",
            BoundVisibleActionObject,
        ),
        (
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.OptionButton",
            "Action",
            VisibleEnabledPromptControl,
        ),
        (
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.ManaPoolItemViewModel",
            "Color",
            BoundVisibleActionObject,
        ),
        (
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PromptBoxViewModel",
            "DoneButton",
            VisibleEnabledPromptControl,
        ),
        (
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PromptBoxViewModel",
            "OkPromptButton",
            VisibleEnabledPromptControl,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.IGameAction",
            "ActionType",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.IGameAction",
            "IsDefault",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.IGameAction",
            "Name",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "ActionChoices",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "AltMenuAction",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "AttackVictimId",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "CanBePerformedLocally",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "ConfirmBeforeTargetingOwnCard",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "ConfirmModeString",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "GroupName",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "HasXTarget",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "InSideboard",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "IsActivatedAbility",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "IsCastAction",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "IsFakeAction",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "IsManaAbility",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "IsSubmenuItem",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "ModeMaxChoices",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "ModeMinChoices",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "ModeChoiceMapping",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "ModeOptions",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "Targets",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "XDeterminedByTargetWithGreatestCMC",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "XIsAMinimum",
            BoundVisibleActionObject,
        ),
        (
            "WotC.MtGO.Client.Model.Reference.dll",
            "WotC.MtGO.Client.Model.Play.ICardAction",
            "XTargetDivisor",
            BoundVisibleActionObject,
        ),
    ]
    .into_iter()
    .map(
        |(assembly_file_name, declaring_type, property_name, binding)| {
            MtgoPrivateVisibleActionJoinGetterV1 {
                assembly_file_name: assembly_file_name.to_owned(),
                declaring_type: declaring_type.to_owned(),
                property_name: property_name.to_owned(),
                binding,
            }
        },
    )
    .collect()
}

fn property_key_v1(property: &MtgoVisibleDuelViewModelProducerPropertyV1) -> String {
    format!(
        "{}\0{}\0{}",
        property.assembly_file_name, property.declaring_type, property.property_name
    )
}

fn private_action_join_key_v1(property: &MtgoPrivateVisibleActionJoinGetterV1) -> String {
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
        assert_eq!(checked.allowed_property_count_v1(), 74);
        assert!(checked.visible_chrome_getter_layer_present_v1());
        assert_eq!(checked.visible_chrome_getter_count_v1(), 19);
        assert!(checked.visible_zone_and_card_getter_layer_present_v1());
        assert_eq!(checked.visible_zone_and_card_getter_count_v1(), 25);
        assert!(checked.private_visible_action_join_layer_present_v1());
        assert_eq!(checked.private_visible_action_join_getter_count_v1(), 31);
        assert!(checked.offline_sealed_action_dispatch_present_v1());
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

        let mut missing_dispatch = mtgo_visible_duel_viewmodel_producer_audit_v1();
        missing_dispatch.action_execution_present = false;
        assert!(
            check_untrusted_visible_duel_viewmodel_producer_audit_v1(missing_dispatch).is_err()
        );
    }

    #[test]
    fn private_action_join_allowlist_and_export_boundary_are_exact() {
        let exact = mtgo_visible_duel_viewmodel_producer_audit_v1();

        let mut unknown = exact.clone();
        unknown.private_visible_action_join_getters[0].property_name = "GameCard".to_owned();
        assert_eq!(
            check_untrusted_visible_duel_viewmodel_producer_audit_v1(unknown)
                .err()
                .unwrap()
                .code(),
            "visible_duel_viewmodel_producer_audit_private_action_join_allowlist"
        );

        let mut exported = exact;
        exported.private_action_objects_exported = true;
        assert_eq!(
            check_untrusted_visible_duel_viewmodel_producer_audit_v1(exported)
                .err()
                .unwrap()
                .code(),
            "visible_duel_viewmodel_producer_audit_boundary"
        );
    }
}
