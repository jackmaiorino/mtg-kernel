use crate::{
    MtgoAuthorizationScopeV1, MtgoEvidenceSourceV1, MtgoObjectBindingV1, MtgoObservedDecisionV1,
    MtgoOfflineActionIntentV1, MtgoPayloadLeafV1, MtgoRectPxV1, MtgoRuntimeModeV1,
    MtgoSemanticDecisionPayloadV1, MtgoVisibleEvidenceV1, ValidatedMtgoObservedDecisionV1,
    MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1, MTGO_AUTHORIZATION_SCHEMA_V1,
    MTGO_OBSERVED_DECISION_SCHEMA_V1,
};
use mtg_kernel::card_def::KERNEL_CARDDB_HASH;
use mtg_kernel::policy_surface_v5::{PolicySurfaceStageV5, POLICY_SURFACE_VERSION};
use mtg_kernel::rl::{
    ActionSemanticV1, CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, ObservationV5,
    PlayerSeatV1, PublicObservationProjectionV5, TargetRefV1, OBSERVATION_SCHEMA_VERSION_V5,
};
use mtg_kernel::surface_v2::H2_PREDICATE_VERSION;
use mtg_kernel::KERNEL_VERSION;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt;

const LEAF_HASH_DOMAIN_V1: &[u8] = b"mtgo-visible-leaf-v1";
const LOCAL_METADATA_HASH_DOMAIN_V1: &[u8] = b"mtgo-local-metadata-v1";
const DECISION_HASH_DOMAIN_V1: &[u8] = b"mtgo-observed-decision-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoContractErrorV1 {
    code: &'static str,
    detail: String,
}

impl MtgoContractErrorV1 {
    pub(crate) fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for MtgoContractErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for MtgoContractErrorV1 {}

pub fn payload_leaf_inventory_v1(
    payload: &MtgoSemanticDecisionPayloadV1,
) -> Result<Vec<MtgoPayloadLeafV1>, MtgoContractErrorV1> {
    let value = serde_json::to_value(payload)
        .map_err(|error| MtgoContractErrorV1::new("payload_serialization", error.to_string()))?;
    let mut leaves = Vec::new();
    collect_leaves_v1(&value, "", &mut leaves)?;
    leaves.sort_by(|left, right| left.json_pointer.cmp(&right.json_pointer));
    Ok(leaves)
}

pub fn local_metadata_commitment_v1(
    payload: &MtgoSemanticDecisionPayloadV1,
) -> Result<String, MtgoContractErrorV1> {
    let leaves = payload_leaf_inventory_v1(payload)?;
    let mut parts = Vec::new();
    for leaf in leaves.iter().filter(|leaf| !leaf.requires_visible_evidence) {
        parts.push(leaf.json_pointer.as_bytes().to_vec());
        parts.push(leaf.value_sha256.as_bytes().to_vec());
    }
    Ok(hash_length_prefixed_v1(
        LOCAL_METADATA_HASH_DOMAIN_V1,
        parts.iter().map(Vec::as_slice),
    ))
}

pub fn validate_observed_decision_v1(
    record: MtgoObservedDecisionV1,
) -> Result<ValidatedMtgoObservedDecisionV1, MtgoContractErrorV1> {
    if record.schema_version != MTGO_OBSERVED_DECISION_SCHEMA_V1 {
        return Err(MtgoContractErrorV1::new(
            "schema_version",
            "the observed decision schema must be exactly version 1",
        ));
    }
    validate_safe_identifier_v1("decision_id", &record.decision_id, 128)?;
    if !record.readiness.observation_complete
        || !record.readiness.legal_action_set_complete
        || !record.readiness.client_prompt_reconciled
    {
        return Err(MtgoContractErrorV1::new(
            "decision_incomplete",
            "observation, legal actions, and visible prompt must all be reconciled",
        ));
    }
    if record.payload.legal_actions.is_empty() || record.payload.legal_actions.len() > 64 {
        return Err(MtgoContractErrorV1::new(
            "legal_action_count",
            "a ready decision must expose between 1 and 64 actions",
        ));
    }
    if record.payload.object_bindings.len() > 1_024 {
        return Err(MtgoContractErrorV1::new(
            "object_binding_count",
            "the object binding table is unexpectedly large",
        ));
    }

    validate_frames_v1(&record)?;
    let evidence_by_id = validate_evidence_v1(&record)?;
    validate_observation_contract_v1(&record.payload)?;
    validate_actions_and_bindings_v1(&record.payload)?;
    validate_provenance_v1(&record, &evidence_by_id)?;

    validate_sha256_v1("local_metadata_sha256", &record.local_metadata_sha256)?;
    let expected_metadata = local_metadata_commitment_v1(&record.payload)?;
    if record.local_metadata_sha256 != expected_metadata {
        return Err(MtgoContractErrorV1::new(
            "local_metadata_commitment",
            "the local metadata commitment does not match the payload",
        ));
    }

    let commitment = decision_commitment_v1(&record)?;
    Ok(ValidatedMtgoObservedDecisionV1 {
        record,
        decision_commitment_sha256: commitment,
    })
}

pub fn make_offline_intent_v1(
    validated: &ValidatedMtgoObservedDecisionV1,
    selected_index: usize,
) -> Result<MtgoOfflineActionIntentV1, MtgoContractErrorV1> {
    let semantic = validated
        .legal_actions()
        .get(selected_index)
        .cloned()
        .ok_or_else(|| {
            MtgoContractErrorV1::new(
                "selected_index",
                "the selected index is outside the validated action vector",
            )
        })?;
    let selected_index = u32::try_from(selected_index).map_err(|_| {
        MtgoContractErrorV1::new("selected_index", "the selected index does not fit u32")
    })?;
    Ok(MtgoOfflineActionIntentV1 {
        schema_version: MTGO_OBSERVED_DECISION_SCHEMA_V1,
        decision_commitment_sha256: validated.decision_commitment_sha256().to_owned(),
        frame_id: validated.frame_id(),
        frame_sequence: validated.frame_sequence(),
        selected_index,
        semantic,
    })
}

pub fn validate_authorization_for_mode_v1(
    scope: &MtgoAuthorizationScopeV1,
    mode: MtgoRuntimeModeV1,
) -> Result<(), MtgoContractErrorV1> {
    if scope.schema_version != MTGO_AUTHORIZATION_SCHEMA_V1 {
        return Err(MtgoContractErrorV1::new(
            "authorization_schema",
            "the authorization scope must be exactly version 1",
        ));
    }
    if !scope.visible_channels_only {
        return Err(MtgoContractErrorV1::new(
            "hidden_channel_forbidden",
            "authorization never enables hidden or reverse-engineered channels",
        ));
    }
    if mode == MtgoRuntimeModeV1::OfflineReplay {
        return Ok(());
    }
    validate_sha256_v1("account_alias_sha256", &scope.account_alias_sha256)?;
    validate_sha256_v1(
        "written_permission_sha256",
        &scope.written_permission_sha256,
    )?;
    if !scope.permits(mode) {
        return Err(MtgoContractErrorV1::new(
            "mode_not_authorized",
            "the exact runtime mode is not present in the written scope",
        ));
    }
    Ok(())
}

fn validate_frames_v1(record: &MtgoObservedDecisionV1) -> Result<(), MtgoContractErrorV1> {
    if record.frames.is_empty() {
        return Err(MtgoContractErrorV1::new(
            "frames_empty",
            "at least one committed client frame is required",
        ));
    }
    let mut ids = HashSet::new();
    let mut sequences = HashSet::new();
    let mut decision_frame_found = false;
    let mut decision_frame_sequence = None;
    let mut maximum_frame_sequence = 0;
    for frame in &record.frames {
        if frame.frame_id == 0 || !ids.insert(frame.frame_id) {
            return Err(MtgoContractErrorV1::new(
                "frame_id",
                "frame identifiers must be nonzero and unique",
            ));
        }
        if frame.sequence == 0 || !sequences.insert(frame.sequence) {
            return Err(MtgoContractErrorV1::new(
                "frame_sequence",
                "frame sequences must be nonzero and unique",
            ));
        }
        validate_sha256_v1("frame.sha256", &frame.sha256)?;
        if frame.client_bounds.width == 0 || frame.client_bounds.height == 0 {
            return Err(MtgoContractErrorV1::new(
                "client_bounds",
                "client bounds must be nonempty",
            ));
        }
        maximum_frame_sequence = maximum_frame_sequence.max(frame.sequence);
        if frame.frame_id == record.frame_id {
            decision_frame_found = true;
            decision_frame_sequence = Some(frame.sequence);
        }
    }
    if !decision_frame_found {
        return Err(MtgoContractErrorV1::new(
            "decision_frame",
            "the decision frame is not present in the frame set",
        ));
    }
    if decision_frame_sequence != Some(maximum_frame_sequence) {
        return Err(MtgoContractErrorV1::new(
            "decision_frame_sequence",
            "the decision frame must be the newest committed frame",
        ));
    }
    Ok(())
}

fn validate_evidence_v1(
    record: &MtgoObservedDecisionV1,
) -> Result<HashMap<u64, &MtgoVisibleEvidenceV1>, MtgoContractErrorV1> {
    if record.evidence.is_empty() {
        return Err(MtgoContractErrorV1::new(
            "evidence_empty",
            "a ready decision must have player-visible evidence",
        ));
    }
    let frames: HashMap<_, _> = record
        .frames
        .iter()
        .map(|frame| (frame.frame_id, frame))
        .collect();
    let mut evidence_by_id = HashMap::new();
    let mut sequences = HashSet::new();
    for item in &record.evidence {
        if item.evidence_id == 0 || evidence_by_id.insert(item.evidence_id, item).is_some() {
            return Err(MtgoContractErrorV1::new(
                "evidence_id",
                "evidence identifiers must be nonzero and unique",
            ));
        }
        if item.sequence == 0 || !sequences.insert(item.sequence) {
            return Err(MtgoContractErrorV1::new(
                "evidence_sequence",
                "evidence sequences must be nonzero and unique",
            ));
        }
    }

    let mut ordered: Vec<_> = record.evidence.iter().collect();
    ordered.sort_by_key(|item| (item.sequence, item.evidence_id));
    for item in ordered {
        match &item.source {
            MtgoEvidenceSourceV1::FrameRegion {
                frame_id,
                rect,
                content_sha256,
            } => {
                let frame = frames.get(frame_id).ok_or_else(|| {
                    MtgoContractErrorV1::new(
                        "evidence_frame",
                        "frame-region evidence names an unknown frame",
                    )
                })?;
                if !rect_within_v1(rect, &frame.client_bounds) {
                    return Err(MtgoContractErrorV1::new(
                        "evidence_rect",
                        "frame-region evidence is empty or outside the client bounds",
                    ));
                }
                if item.sequence < frame.sequence {
                    return Err(MtgoContractErrorV1::new(
                        "evidence_frame_sequence",
                        "frame-region evidence cannot precede the frame it cites",
                    ));
                }
                validate_sha256_v1("region.content_sha256", content_sha256)?;
            }
            MtgoEvidenceSourceV1::VisibleGameLogText {
                frame_region_evidence_id,
                text_sha256,
            } => {
                require_direct_frame_region_parent_v1(
                    *frame_region_evidence_id,
                    item,
                    &evidence_by_id,
                )?;
                validate_sha256_v1("game_log.text_sha256", text_sha256)?;
            }
            MtgoEvidenceSourceV1::VisibleAccessibilityText {
                frame_region_evidence_id,
                frame_id,
                control_bounds,
                is_offscreen,
                automation_id_sha256,
                text_sha256,
            } => {
                let parent = require_direct_frame_region_parent_v1(
                    *frame_region_evidence_id,
                    item,
                    &evidence_by_id,
                )?;
                let MtgoEvidenceSourceV1::FrameRegion {
                    frame_id: parent_frame_id,
                    rect: parent_rect,
                    ..
                } = &parent.source
                else {
                    unreachable!("the helper returned a non-frame-region parent")
                };
                if *is_offscreen
                    || *frame_id != *parent_frame_id
                    || !rect_within_v1(control_bounds, parent_rect)
                {
                    return Err(MtgoContractErrorV1::new(
                        "accessibility_visibility",
                        "accessibility evidence must be explicitly on-screen inside its pixel corroboration",
                    ));
                }
                validate_sha256_v1("accessibility.automation_id_sha256", automation_id_sha256)?;
                validate_sha256_v1("accessibility.text_sha256", text_sha256)?;
            }
            MtgoEvidenceSourceV1::ManualVisibleAnnotation {
                frame_region_evidence_id,
                annotation_sha256,
            } => {
                require_direct_frame_region_parent_v1(
                    *frame_region_evidence_id,
                    item,
                    &evidence_by_id,
                )?;
                validate_sha256_v1("annotation.sha256", annotation_sha256)?;
            }
            MtgoEvidenceSourceV1::DerivedPublicFact {
                parent_evidence_ids,
                ..
            } => {
                if parent_evidence_ids.is_empty() {
                    return Err(MtgoContractErrorV1::new(
                        "derived_evidence_parents",
                        "derived evidence must name at least one prior visible record",
                    ));
                }
                let mut unique = HashSet::new();
                for parent_id in parent_evidence_ids {
                    if !unique.insert(*parent_id) {
                        return Err(MtgoContractErrorV1::new(
                            "derived_evidence_parents",
                            "derived evidence contains a duplicate parent",
                        ));
                    }
                    require_prior_parent_v1(*parent_id, item, &evidence_by_id)?;
                }
            }
        }
    }
    Ok(evidence_by_id)
}

fn require_direct_frame_region_parent_v1<'a>(
    parent_id: u64,
    child: &MtgoVisibleEvidenceV1,
    evidence_by_id: &'a HashMap<u64, &'a MtgoVisibleEvidenceV1>,
) -> Result<&'a MtgoVisibleEvidenceV1, MtgoContractErrorV1> {
    let parent = require_prior_parent_v1(parent_id, child, evidence_by_id)?;
    if !matches!(parent.source, MtgoEvidenceSourceV1::FrameRegion { .. }) {
        return Err(MtgoContractErrorV1::new(
            "visual_corroboration",
            "text or annotation evidence must directly cite a visible frame region",
        ));
    }
    Ok(parent)
}

fn require_prior_parent_v1<'a>(
    parent_id: u64,
    child: &MtgoVisibleEvidenceV1,
    evidence_by_id: &'a HashMap<u64, &MtgoVisibleEvidenceV1>,
) -> Result<&'a MtgoVisibleEvidenceV1, MtgoContractErrorV1> {
    let parent = evidence_by_id.get(&parent_id).copied().ok_or_else(|| {
        MtgoContractErrorV1::new("evidence_parent", "evidence names an unknown parent")
    })?;
    if parent.sequence >= child.sequence {
        return Err(MtgoContractErrorV1::new(
            "evidence_parent_order",
            "evidence parents must have a strictly earlier sequence",
        ));
    }
    Ok(parent)
}

fn validate_provenance_v1(
    record: &MtgoObservedDecisionV1,
    evidence_by_id: &HashMap<u64, &MtgoVisibleEvidenceV1>,
) -> Result<(), MtgoContractErrorV1> {
    let inventory = payload_leaf_inventory_v1(&record.payload)?;
    let expected: HashMap<_, _> = inventory
        .iter()
        .filter(|leaf| leaf.requires_visible_evidence)
        .map(|leaf| (leaf.json_pointer.as_str(), leaf.value_sha256.as_str()))
        .collect();
    if record.provenance.len() != expected.len() {
        return Err(MtgoContractErrorV1::new(
            "provenance_coverage",
            "visible payload leaves and provenance rows differ in count",
        ));
    }

    let mut pointers = HashSet::new();
    let mut directly_used_evidence = HashSet::new();
    for row in &record.provenance {
        if !pointers.insert(row.json_pointer.as_str()) {
            return Err(MtgoContractErrorV1::new(
                "provenance_pointer",
                "each payload leaf must have exactly one provenance row",
            ));
        }
        let expected_digest = expected.get(row.json_pointer.as_str()).ok_or_else(|| {
            MtgoContractErrorV1::new(
                "provenance_pointer",
                "provenance names an extra or local-metadata payload pointer",
            )
        })?;
        validate_sha256_v1("provenance.value_sha256", &row.value_sha256)?;
        if row.value_sha256 != *expected_digest {
            return Err(MtgoContractErrorV1::new(
                "provenance_value",
                "a provenance digest does not match its payload leaf",
            ));
        }
        if row.confidence_bps < MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1
            || row.confidence_bps > 10_000
        {
            return Err(MtgoContractErrorV1::new(
                "provenance_confidence",
                "every visible game-information leaf must meet the versioned confidence floor",
            ));
        }
        if row.evidence_ids.is_empty() {
            return Err(MtgoContractErrorV1::new(
                "provenance_evidence",
                "every visible game-information leaf must cite evidence",
            ));
        }
        let mut row_ids = HashSet::new();
        let mut root_frame_ids = HashSet::new();
        for evidence_id in &row.evidence_ids {
            if !row_ids.insert(*evidence_id) {
                return Err(MtgoContractErrorV1::new(
                    "provenance_evidence",
                    "a provenance row contains a duplicate evidence identifier",
                ));
            }
            if !evidence_by_id.contains_key(evidence_id) {
                return Err(MtgoContractErrorV1::new(
                    "provenance_evidence",
                    "a provenance row names unknown evidence",
                ));
            }
            directly_used_evidence.insert(*evidence_id);
            collect_evidence_root_frames_v1(*evidence_id, evidence_by_id, &mut root_frame_ids);
        }
        if requires_current_frame_v1(&row.json_pointer)
            && !root_frame_ids.contains(&record.frame_id)
        {
            return Err(MtgoContractErrorV1::new(
                "current_frame_evidence",
                "current observation, object binding, and legal-action leaves must cite the decision frame",
            ));
        }
    }

    let mut reachable = HashSet::new();
    for evidence_id in directly_used_evidence {
        collect_evidence_ancestors_v1(evidence_id, evidence_by_id, &mut reachable);
    }
    if reachable.len() != evidence_by_id.len() {
        return Err(MtgoContractErrorV1::new(
            "unused_evidence",
            "all committed evidence must support at least one payload leaf",
        ));
    }
    Ok(())
}

fn collect_evidence_root_frames_v1(
    evidence_id: u64,
    evidence_by_id: &HashMap<u64, &MtgoVisibleEvidenceV1>,
    frame_ids: &mut HashSet<u64>,
) {
    let Some(item) = evidence_by_id.get(&evidence_id) else {
        return;
    };
    match &item.source {
        MtgoEvidenceSourceV1::FrameRegion { frame_id, .. } => {
            frame_ids.insert(*frame_id);
        }
        MtgoEvidenceSourceV1::VisibleGameLogText {
            frame_region_evidence_id,
            ..
        }
        | MtgoEvidenceSourceV1::VisibleAccessibilityText {
            frame_region_evidence_id,
            ..
        }
        | MtgoEvidenceSourceV1::ManualVisibleAnnotation {
            frame_region_evidence_id,
            ..
        } => collect_evidence_root_frames_v1(*frame_region_evidence_id, evidence_by_id, frame_ids),
        MtgoEvidenceSourceV1::DerivedPublicFact {
            parent_evidence_ids,
            ..
        } => {
            for parent_id in parent_evidence_ids {
                collect_evidence_root_frames_v1(*parent_id, evidence_by_id, frame_ids);
            }
        }
    }
}

fn requires_current_frame_v1(json_pointer: &str) -> bool {
    if json_pointer.starts_with("/observation/known_library_cards")
        || json_pointer.starts_with("/observation/known_hand_cards")
    {
        return false;
    }
    json_pointer.starts_with("/observation/")
        || json_pointer.starts_with("/object_bindings/")
        || json_pointer.starts_with("/legal_actions/")
}

fn collect_evidence_ancestors_v1(
    evidence_id: u64,
    evidence_by_id: &HashMap<u64, &MtgoVisibleEvidenceV1>,
    reachable: &mut HashSet<u64>,
) {
    if !reachable.insert(evidence_id) {
        return;
    }
    let Some(item) = evidence_by_id.get(&evidence_id) else {
        return;
    };
    match &item.source {
        MtgoEvidenceSourceV1::FrameRegion { .. } => {}
        MtgoEvidenceSourceV1::VisibleGameLogText {
            frame_region_evidence_id,
            ..
        }
        | MtgoEvidenceSourceV1::VisibleAccessibilityText {
            frame_region_evidence_id,
            ..
        }
        | MtgoEvidenceSourceV1::ManualVisibleAnnotation {
            frame_region_evidence_id,
            ..
        } => collect_evidence_ancestors_v1(*frame_region_evidence_id, evidence_by_id, reachable),
        MtgoEvidenceSourceV1::DerivedPublicFact {
            parent_evidence_ids,
            ..
        } => {
            for parent_id in parent_evidence_ids {
                collect_evidence_ancestors_v1(*parent_id, evidence_by_id, reachable);
            }
        }
    }
}

fn validate_observation_contract_v1(
    payload: &MtgoSemanticDecisionPayloadV1,
) -> Result<(), MtgoContractErrorV1> {
    let observation = &payload.observation;
    if observation.schema_version != OBSERVATION_SCHEMA_VERSION_V5
        || observation.kernel_version != KERNEL_VERSION
        || observation.surface_version != H2_PREDICATE_VERSION
        || observation.policy_surface_version != POLICY_SURFACE_VERSION
        || observation.card_db_hash != KERNEL_CARDDB_HASH
        || observation.substep_count == 0
        || observation.substep_index >= observation.substep_count
    {
        return Err(MtgoContractErrorV1::new(
            "observation_metadata",
            "the reconstructed observation is incompatible with the current kernel contract",
        ));
    }
    if observation.visible_projection_hash != compute_visible_projection_hash_v5_v1(observation)? {
        return Err(MtgoContractErrorV1::new(
            "visible_projection_hash",
            "the reconstructed observation does not match its kernel projection hash",
        ));
    }

    let context = &observation.projection.policy_surface_context;
    let private = context.private_combat_selection.as_ref();
    match context.current_stage {
        PolicySurfaceStageV5::Surface => {
            if private.is_some()
                || observation.substep_index != 0
                || observation.substep_count != 1
                || payload.legal_actions.iter().any(|action| {
                    matches!(
                        action,
                        ActionSemanticV1::ChooseAttackerInclusion { .. }
                            | ActionSemanticV1::ChooseBlockerInclusion { .. }
                    )
                })
            {
                return Err(MtgoContractErrorV1::new(
                    "policy_surface_context",
                    "surface-stage metadata cannot contain an incremental combat scan",
                ));
            }
        }
        PolicySurfaceStageV5::AttackerInclusion | PolicySurfaceStageV5::BlockerInclusion => {
            let private = private.ok_or_else(|| {
                MtgoContractErrorV1::new(
                    "policy_surface_context",
                    "an incremental combat scan is missing its private selection context",
                )
            })?;
            if private.candidate_count == 0
                || private.candidate_index >= private.candidate_count
                || private.candidate_index != observation.substep_index
                || private.candidate_count != observation.substep_count
                || private.remaining_after_current.len()
                    != (private.candidate_count - private.candidate_index - 1) as usize
                || private.selected.len() > private.candidate_index as usize
            {
                return Err(MtgoContractErrorV1::new(
                    "policy_surface_context",
                    "combat-scan context does not partition the frozen candidate sequence",
                ));
            }
            let mut partition_refs = HashSet::new();
            for reference in private
                .selected
                .iter()
                .chain(std::iter::once(&private.current_candidate))
                .chain(private.remaining_after_current.iter())
            {
                if !partition_refs.insert(reference) {
                    return Err(MtgoContractErrorV1::new(
                        "policy_surface_context",
                        "combat-scan context contains a duplicate candidate reference",
                    ));
                }
            }
            if payload.legal_actions.len() != 2 {
                return Err(MtgoContractErrorV1::new(
                    "policy_surface_context",
                    "an incremental combat scan must expose exactly two Boolean actions",
                ));
            }
            let actor = observation.acting_player;
            let valid_pair = match context.current_stage {
                PolicySurfaceStageV5::AttackerInclusion => {
                    private.attacker.is_none()
                        && matches!(
                            &payload.legal_actions[0],
                            ActionSemanticV1::ChooseAttackerInclusion {
                                actor: action_actor,
                                attacker,
                                include: false,
                            } if *action_actor == actor
                                && attacker == &private.current_candidate
                        )
                        && matches!(
                            &payload.legal_actions[1],
                            ActionSemanticV1::ChooseAttackerInclusion {
                                actor: action_actor,
                                attacker,
                                include: true,
                            } if *action_actor == actor
                                && attacker == &private.current_candidate
                        )
                }
                PolicySurfaceStageV5::BlockerInclusion => {
                    let fixed_attacker = private.attacker.as_ref().ok_or_else(|| {
                        MtgoContractErrorV1::new(
                            "policy_surface_context",
                            "a blocker scan is missing its fixed attacker",
                        )
                    })?;
                    matches!(
                        &payload.legal_actions[0],
                        ActionSemanticV1::ChooseBlockerInclusion {
                            actor: action_actor,
                            attacker,
                            blocker,
                            include: false,
                        } if *action_actor == actor
                            && attacker == fixed_attacker
                            && blocker == &private.current_candidate
                    ) && matches!(
                        &payload.legal_actions[1],
                        ActionSemanticV1::ChooseBlockerInclusion {
                            actor: action_actor,
                            attacker,
                            blocker,
                            include: true,
                        } if *action_actor == actor
                            && attacker == fixed_attacker
                            && blocker == &private.current_candidate
                    )
                }
                PolicySurfaceStageV5::Surface => unreachable!(),
            };
            if !valid_pair {
                return Err(MtgoContractErrorV1::new(
                    "policy_surface_context",
                    "combat-scan actions must be the exact false-then-true pair for the current candidate",
                ));
            }
        }
    }
    Ok(())
}

pub fn compute_visible_projection_hash_v5_v1(
    observation: &ObservationV5,
) -> Result<u64, MtgoContractErrorV1> {
    #[derive(Serialize)]
    struct ObservationHashInputV1<'a> {
        schema_version: u32,
        kernel_version: &'a str,
        surface_version: u32,
        policy_surface_version: u32,
        card_db_hash: u64,
        acting_player: PlayerSeatV1,
        step_index: u64,
        physical_decision_id: u64,
        substep_index: u32,
        substep_count: u32,
        projection: &'a PublicObservationProjectionV5,
        own_hand: &'a [CardPrivateV1],
        known_library_cards: &'a [Vec<KnownLibraryCardV4>; 2],
        known_hand_cards: &'a [Vec<CardPrivateV1>; 2],
    }
    let input = ObservationHashInputV1 {
        schema_version: observation.schema_version,
        kernel_version: &observation.kernel_version,
        surface_version: observation.surface_version,
        policy_surface_version: observation.policy_surface_version,
        card_db_hash: observation.card_db_hash,
        acting_player: observation.acting_player,
        step_index: observation.step_index,
        physical_decision_id: observation.physical_decision_id,
        substep_index: observation.substep_index,
        substep_count: observation.substep_count,
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
    };
    let bytes = serde_json::to_vec(&input).map_err(|error| {
        MtgoContractErrorV1::new("observation_hash_serialization", error.to_string())
    })?;
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Ok(hash)
}

fn validate_actions_and_bindings_v1(
    payload: &MtgoSemanticDecisionPayloadV1,
) -> Result<(), MtgoContractErrorV1> {
    let mut adapter_ids = HashSet::new();
    let mut kernel_refs = HashSet::new();
    for binding in &payload.object_bindings {
        validate_binding_v1(binding)?;
        if !adapter_ids.insert(binding.adapter_object_id.as_str()) {
            return Err(MtgoContractErrorV1::new(
                "object_binding_id",
                "adapter object identifiers must be unique",
            ));
        }
        if !kernel_refs.insert(binding.kernel_ref.clone()) {
            return Err(MtgoContractErrorV1::new(
                "object_binding_ref",
                "kernel stable references must have a unique adapter binding",
            ));
        }
    }

    let mut semantics = HashSet::new();
    let mut required_refs = HashSet::new();
    for action in &payload.legal_actions {
        validate_action_shape_v1(action)?;
        let actor = action_actor_v1(action).ok_or_else(|| {
            MtgoContractErrorV1::new(
                "unsupported_action",
                "ambiguous and aggregate combat actions cannot enter the scored-policy slice",
            )
        })?;
        if actor != payload.observation.acting_player {
            return Err(MtgoContractErrorV1::new(
                "action_actor",
                "every action actor must match the observation acting player",
            ));
        }
        let serialized = serde_json::to_string(action)
            .map_err(|error| MtgoContractErrorV1::new("action_serialization", error.to_string()))?;
        if !semantics.insert(serialized) {
            return Err(MtgoContractErrorV1::new(
                "duplicate_action",
                "legal action semantics must be unique within a decision",
            ));
        }
        collect_action_refs_v1(action, &mut required_refs);
    }
    for required in required_refs {
        if !kernel_refs.contains(&required) {
            return Err(MtgoContractErrorV1::new(
                "unbound_action_object",
                "every card reference in an action must have an exact adapter binding",
            ));
        }
    }
    Ok(())
}

fn validate_action_shape_v1(action: &ActionSemanticV1) -> Result<(), MtgoContractErrorV1> {
    let invalid = match action {
        ActionSemanticV1::ChooseTarget { remaining, .. }
        | ActionSemanticV1::ChooseCostTarget { remaining, .. } => *remaining == 0,
        ActionSemanticV1::ChooseSpellMode {
            mode_index,
            mode_count,
            ..
        } => *mode_count == 0 || *mode_index >= *mode_count,
        ActionSemanticV1::ChooseEffectOption {
            option_index,
            option_count,
            ..
        } => *option_count == 0 || *option_index >= *option_count,
        ActionSemanticV1::ChooseEffectTarget {
            selected_count,
            min_targets,
            max_targets,
            ..
        } => *min_targets > *max_targets || *max_targets == 0 || *selected_count >= *max_targets,
        ActionSemanticV1::ChooseEffectNumber {
            number,
            minimum,
            maximum,
            ..
        } => *minimum > *maximum || *number < *minimum || *number > *maximum,
        ActionSemanticV1::Discard { cards, .. } => {
            cards.is_empty() || cards.iter().collect::<HashSet<_>>().len() != cards.len()
        }
        ActionSemanticV1::OrderTriggers {
            pending_sources,
            order,
            ..
        } => {
            if pending_sources.is_empty() || order.len() != pending_sources.len() {
                true
            } else {
                let indices: HashSet<_> = order.iter().copied().collect();
                indices.len() != order.len()
                    || indices.iter().any(|index| *index >= pending_sources.len())
            }
        }
        _ => false,
    };
    if invalid {
        return Err(MtgoContractErrorV1::new(
            "action_shape",
            "a semantic action contains an invalid count, index, range, or permutation",
        ));
    }
    Ok(())
}

fn validate_binding_v1(binding: &MtgoObjectBindingV1) -> Result<(), MtgoContractErrorV1> {
    validate_safe_identifier_v1("adapter_object_id", &binding.adapter_object_id, 128)
}

fn action_actor_v1(action: &ActionSemanticV1) -> Option<PlayerSeatV1> {
    match action {
        ActionSemanticV1::Pass { actor }
        | ActionSemanticV1::PlayLand { actor, .. }
        | ActionSemanticV1::CastSpell { actor, .. }
        | ActionSemanticV1::ActivateManaAbility { actor, .. }
        | ActionSemanticV1::ActivateAbility { actor, .. }
        | ActionSemanticV1::PlotSpell { actor, .. }
        | ActionSemanticV1::ChooseTarget { actor, .. }
        | ActionSemanticV1::ChooseCostTarget { actor, .. }
        | ActionSemanticV1::ChooseCastMode { actor, .. }
        | ActionSemanticV1::ChooseKicker { actor, .. }
        | ActionSemanticV1::ChooseSpellMode { actor, .. }
        | ActionSemanticV1::ChooseEffectOption { actor, .. }
        | ActionSemanticV1::ChooseEffectTarget { actor, .. }
        | ActionSemanticV1::FinishEffectSelection { actor, .. }
        | ActionSemanticV1::ChooseEffectColor { actor, .. }
        | ActionSemanticV1::ChooseEffectNumber { actor, .. }
        | ActionSemanticV1::ChooseEffectBoolean { actor, .. }
        | ActionSemanticV1::FinishTargetSelection { actor, .. }
        | ActionSemanticV1::ChooseOptionalCostUse { actor, .. }
        | ActionSemanticV1::ChooseOptionalCostWhich { actor, .. }
        | ActionSemanticV1::ChooseSpellCopyPayment { actor, .. }
        | ActionSemanticV1::ChooseSpellCopyRetarget { actor, .. }
        | ActionSemanticV1::ChooseMadnessCast { actor, .. }
        | ActionSemanticV1::Discard { actor, .. }
        | ActionSemanticV1::ChooseAttackerInclusion { actor, .. }
        | ActionSemanticV1::ChooseBlockerInclusion { actor, .. }
        | ActionSemanticV1::OrderTriggers { actor, .. } => Some(*actor),
        ActionSemanticV1::DeclareAttackers { .. }
        | ActionSemanticV1::DeclareBlockersForAttacker { .. }
        | ActionSemanticV1::Ambiguous { .. } => None,
    }
}

fn collect_action_refs_v1(action: &ActionSemanticV1, output: &mut HashSet<CardStableRefV1>) {
    match action {
        ActionSemanticV1::Pass { .. }
        | ActionSemanticV1::ChooseOptionalCostUse { .. }
        | ActionSemanticV1::ChooseOptionalCostWhich { .. }
        | ActionSemanticV1::Ambiguous { .. } => {}
        ActionSemanticV1::PlayLand { source, .. }
        | ActionSemanticV1::CastSpell { source, .. }
        | ActionSemanticV1::ActivateManaAbility { source, .. }
        | ActionSemanticV1::ActivateAbility { source, .. }
        | ActionSemanticV1::PlotSpell { source, .. }
        | ActionSemanticV1::ChooseCastMode { source, .. }
        | ActionSemanticV1::ChooseKicker { source, .. }
        | ActionSemanticV1::ChooseSpellMode { source, .. }
        | ActionSemanticV1::ChooseEffectOption { source, .. }
        | ActionSemanticV1::FinishEffectSelection { source, .. }
        | ActionSemanticV1::ChooseEffectColor { source, .. }
        | ActionSemanticV1::ChooseEffectNumber { source, .. }
        | ActionSemanticV1::ChooseEffectBoolean { source, .. }
        | ActionSemanticV1::FinishTargetSelection { source, .. }
        | ActionSemanticV1::ChooseSpellCopyPayment { source, .. }
        | ActionSemanticV1::ChooseSpellCopyRetarget { source, .. } => {
            output.insert(source.clone());
        }
        ActionSemanticV1::ChooseTarget { source, target, .. }
        | ActionSemanticV1::ChooseEffectTarget { source, target, .. } => {
            output.insert(source.clone());
            if let TargetRefV1::Object { object } = target {
                output.insert(object.clone());
            }
        }
        ActionSemanticV1::ChooseCostTarget {
            source, candidate, ..
        } => {
            output.insert(source.clone());
            output.insert(candidate.clone());
        }
        ActionSemanticV1::ChooseMadnessCast { card, .. } => {
            output.insert(card.clone());
        }
        ActionSemanticV1::Discard { cards, .. }
        | ActionSemanticV1::DeclareAttackers {
            attackers: cards, ..
        }
        | ActionSemanticV1::OrderTriggers {
            pending_sources: cards,
            ..
        } => output.extend(cards.iter().cloned()),
        ActionSemanticV1::DeclareBlockersForAttacker {
            attacker, blockers, ..
        } => {
            output.insert(attacker.clone());
            output.extend(blockers.iter().cloned());
        }
        ActionSemanticV1::ChooseAttackerInclusion { attacker, .. } => {
            output.insert(attacker.clone());
        }
        ActionSemanticV1::ChooseBlockerInclusion {
            attacker, blocker, ..
        } => {
            output.insert(attacker.clone());
            output.insert(blocker.clone());
        }
    }
}

fn collect_leaves_v1(
    value: &Value,
    pointer: &str,
    output: &mut Vec<MtgoPayloadLeafV1>,
) -> Result<(), MtgoContractErrorV1> {
    match value {
        Value::Array(values) if !values.is_empty() => {
            for (index, child) in values.iter().enumerate() {
                collect_leaves_v1(child, &format!("{pointer}/{index}"), output)?;
            }
        }
        Value::Object(values) if !values.is_empty() => {
            let mut keys: Vec<_> = values.keys().collect();
            keys.sort();
            for key in keys {
                let escaped = key.replace('~', "~0").replace('/', "~1");
                collect_leaves_v1(
                    values
                        .get(key)
                        .expect("a key collected from the same JSON object must exist"),
                    &format!("{pointer}/{escaped}"),
                    output,
                )?;
            }
        }
        _ => {
            let encoded = serde_json::to_vec(value).map_err(|error| {
                MtgoContractErrorV1::new("leaf_serialization", error.to_string())
            })?;
            output.push(MtgoPayloadLeafV1 {
                json_pointer: pointer.to_owned(),
                value_sha256: hash_length_prefixed_v1(
                    LEAF_HASH_DOMAIN_V1,
                    [pointer.as_bytes(), encoded.as_slice()],
                ),
                requires_visible_evidence: !is_local_metadata_pointer_v1(pointer),
            });
        }
    }
    Ok(())
}

fn is_local_metadata_pointer_v1(pointer: &str) -> bool {
    const EXACT: &[&str] = &[
        "/observation/schema_version",
        "/observation/kernel_version",
        "/observation/surface_version",
        "/observation/policy_surface_version",
        "/observation/card_db_hash",
        "/observation/step_index",
        "/observation/physical_decision_id",
        "/observation/substep_index",
        "/observation/substep_count",
        "/observation/visible_projection_hash",
    ];
    if EXACT.contains(&pointer)
        || pointer.ends_with("/arena_id")
        || pointer.ends_with("/zone_change_count")
    {
        return true;
    }
    let segments: Vec<_> = pointer.split('/').collect();
    segments.len() == 4
        && segments[1] == "object_bindings"
        && segments[2].parse::<usize>().is_ok()
        && segments[3] == "adapter_object_id"
}

fn decision_commitment_v1(record: &MtgoObservedDecisionV1) -> Result<String, MtgoContractErrorV1> {
    let mut canonical = record.clone();
    canonical
        .frames
        .sort_by_key(|frame| (frame.sequence, frame.frame_id));
    canonical
        .evidence
        .sort_by_key(|item| (item.sequence, item.evidence_id));
    canonical
        .provenance
        .sort_by(|left, right| left.json_pointer.cmp(&right.json_pointer));
    for row in &mut canonical.provenance {
        row.evidence_ids.sort_unstable();
    }
    let encoded = serde_json::to_vec(&canonical)
        .map_err(|error| MtgoContractErrorV1::new("decision_serialization", error.to_string()))?;
    Ok(hash_length_prefixed_v1(
        DECISION_HASH_DOMAIN_V1,
        [encoded.as_slice()],
    ))
}

fn hash_length_prefixed_v1<'a>(domain: &[u8], parts: impl IntoIterator<Item = &'a [u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_be_bytes());
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use fmt::Write;
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn validate_sha256_v1(label: &str, value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(MtgoContractErrorV1::new(
            "sha256",
            format!("{label} must be 64 lowercase hexadecimal characters"),
        ));
    }
    Ok(())
}

fn validate_safe_identifier_v1(
    label: &str,
    value: &str,
    max_len: usize,
) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > max_len
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
    {
        return Err(MtgoContractErrorV1::new(
            "identifier",
            format!("{label} is empty, too long, or contains unsafe characters"),
        ));
    }
    Ok(())
}

fn rect_within_v1(inner: &MtgoRectPxV1, outer: &MtgoRectPxV1) -> bool {
    if inner.width == 0 || inner.height == 0 {
        return false;
    }
    let inner_right = u64::from(inner.x) + u64::from(inner.width);
    let inner_bottom = u64::from(inner.y) + u64::from(inner.height);
    let outer_right = u64::from(outer.x) + u64::from(outer.width);
    let outer_bottom = u64::from(outer.y) + u64::from(outer.height);
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner_right <= outer_right
        && inner_bottom <= outer_bottom
}
