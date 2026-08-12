use crate::{
    player_visible_game_log_action_decision_commitment_v1,
    validate_player_visible_duel_gesture_plan_v1,
    CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1, MtgoCompetitiveEventKindV1,
    MtgoContractErrorV1, MtgoDuelActionFamilyV1, MtgoPlayerRelativeRoleV1,
    MtgoPlayerVisibleConfirmedDuelDecisionV1, MtgoPlayerVisibleDuelActionV1,
    MtgoPlayerVisibleDuelGesturePlanV1, MtgoPlayerVisibleDuelGesturePrimitiveV1, MtgoRectPxV1,
    MtgoSizePxV1, MtgoVisibleGameLogActionCorroborationKindV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_SCHEMA_V1: u32 = 1;
pub const MTGO_PLAYER_VISIBLE_GAMEPLAY_BEFORE_INPUT_SCHEMA_V1: u32 = 1;

const PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-gameplay-postcondition-v1";
const PLAYER_VISIBLE_GAMEPLAY_BEFORE_INPUT_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-gameplay-before-input-v1";
const MAX_PLAYER_VISIBLE_POSTCONDITION_TRANSITIONS_V1: usize = 16;

/// Action-specific visible region category used to confirm one emitted
/// player-visible gesture primitive. These labels describe only UI-visible
/// changes. They contain no kernel or MTGO object identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoPlayerVisibleGameplayPostconditionKindV1 {
    SelectedControlChanged,
    PromptChanged,
    PhaseBarChanged,
    PlayerCountsChanged,
    BattlefieldChanged,
    HandChanged,
    GraveyardChanged,
    LibraryChanged,
    ExileChanged,
    ManaPoolChanged,
    StackChanged,
    CombatChanged,
    ChoiceSurfaceChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleGameplayRegionTransitionV1 {
    pub kind: MtgoPlayerVisibleGameplayPostconditionKindV1,
    pub rect_client_px: MtgoRectPxV1,
    pub before_bgra8_sha256: String,
    pub after_bgra8_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleGameplayBeforeRegionV1 {
    pub kind: MtgoPlayerVisibleGameplayPostconditionKindV1,
    pub rect_client_px: MtgoRectPxV1,
    pub before_bgra8_sha256: String,
}

/// Complete visible-only change plan declared before one gesture primitive is
/// emitted. A trusted producer must compute each hash from its retained source
/// pixels. This record contains no coordinates suitable for input and grants no
/// authority to emit the gesture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleGameplayBeforeInputRecordV1 {
    pub schema_version: u32,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub deployment_commitment_sha256: String,
    pub decision_commitment_sha256: String,
    pub selection_commitment_sha256: String,
    pub source_frame_id: u64,
    pub source_frame_sequence: u64,
    pub source_frame_sha256: String,
    pub client_size_px: MtgoSizePxV1,
    pub player_visible_decision: MtgoPlayerVisibleConfirmedDuelDecisionV1,
    pub gesture_plan: MtgoPlayerVisibleDuelGesturePlanV1,
    pub primitive_index: u16,
    pub primitive_is_final: bool,
    pub region_set_complete: bool,
    pub regions: Vec<MtgoPlayerVisibleGameplayBeforeRegionV1>,
    pub expected_game_log_baseline_commitment_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleGameplayAfterRegionV1 {
    pub kind: MtgoPlayerVisibleGameplayPostconditionKindV1,
    pub rect_client_px: MtgoRectPxV1,
    pub after_bgra8_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleGameplayAfterFrameV1 {
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub after_frame_sha256: String,
    pub regions: Vec<MtgoPlayerVisibleGameplayAfterRegionV1>,
}

/// Structurally checked before-input declaration. It is deliberately opaque:
/// callers can retain its commitment but cannot recover rectangles or convert
/// it to an input operation.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoPlayerVisibleGameplayBeforeInputV1;
/// fn cannot_act(value: CheckedUntrustedMtgoPlayerVisibleGameplayBeforeInputV1) {
///     let _ = value.regions();
///     let _ = value.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoPlayerVisibleGameplayBeforeInputV1 {
    record: MtgoPlayerVisibleGameplayBeforeInputRecordV1,
    action_family: MtgoDuelActionFamilyV1,
    before_input_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPlayerVisibleGameplayBeforeInputV1 {
    pub fn action_family_v1(&self) -> MtgoDuelActionFamilyV1 {
        self.action_family
    }

    pub fn before_input_commitment_sha256_v1(&self) -> &str {
        &self.before_input_commitment_sha256
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Producer-facing visible-only postcondition record. A future opaque capture
/// runtime must recompute every region hash from its retained before and after
/// pixels. Structural checking alone does not authorize another input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleGameplayPostconditionRecordV1 {
    pub schema_version: u32,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub deployment_commitment_sha256: String,
    pub decision_commitment_sha256: String,
    pub selection_commitment_sha256: String,
    pub source_frame_id: u64,
    pub source_frame_sequence: u64,
    pub source_frame_sha256: String,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub after_frame_sha256: String,
    pub client_size_px: MtgoSizePxV1,
    pub player_visible_decision: MtgoPlayerVisibleConfirmedDuelDecisionV1,
    pub gesture_plan: MtgoPlayerVisibleDuelGesturePlanV1,
    pub primitive_index: u16,
    pub primitive_is_final: bool,
    pub transition_set_complete: bool,
    pub transitions: Vec<MtgoPlayerVisibleGameplayRegionTransitionV1>,
    pub expected_game_log_baseline_commitment_sha256: Option<String>,
}

/// Structurally checked action-specific visible postcondition. The exact
/// player-visible decision is retained for confirmed game history, while
/// pixels, rectangles, Game Log internals, and input methods remain sealed.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoPlayerVisibleGameplayPostconditionV1;
/// fn cannot_act(value: CheckedUntrustedMtgoPlayerVisibleGameplayPostconditionV1) {
///     let _ = value.input_command();
///     let _ = value.transitions();
///     let _ = value.game_log_text();
/// }
/// ```
pub struct CheckedUntrustedMtgoPlayerVisibleGameplayPostconditionV1 {
    record: MtgoPlayerVisibleGameplayPostconditionRecordV1,
    _game_log_corroboration: Option<CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1>,
    action_family: MtgoDuelActionFamilyV1,
    confirmation_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPlayerVisibleGameplayPostconditionV1 {
    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.record.event_kind
    }

    pub fn game_number_v1(&self) -> u8 {
        self.record.game_number
    }

    pub fn action_family_v1(&self) -> MtgoDuelActionFamilyV1 {
        self.action_family
    }

    pub fn player_visible_decision_v1(&self) -> &MtgoPlayerVisibleConfirmedDuelDecisionV1 {
        &self.record.player_visible_decision
    }

    pub fn after_frame_id_v1(&self) -> u64 {
        self.record.after_frame_id
    }

    pub fn after_frame_sequence_v1(&self) -> u64 {
        self.record.after_frame_sequence
    }

    pub fn confirmation_commitment_sha256_v1(&self) -> &str {
        &self.confirmation_commitment_sha256
    }

    pub fn has_exact_visible_game_log_corroboration_v1(&self) -> bool {
        self._game_log_corroboration.is_some()
    }

    pub fn safe_for_additional_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub(crate) fn event_identity_sha256_v1(&self) -> &str {
        &self.record.event_identity_sha256
    }

    pub(crate) fn match_identity_sha256_v1(&self) -> &str {
        &self.record.match_identity_sha256
    }

    pub(crate) fn deployment_commitment_sha256_v1(&self) -> &str {
        &self.record.deployment_commitment_sha256
    }

    pub(crate) fn decision_commitment_sha256_v1(&self) -> &str {
        &self.record.decision_commitment_sha256
    }

    pub(crate) fn selection_commitment_sha256_v1(&self) -> &str {
        &self.record.selection_commitment_sha256
    }

    pub(crate) fn source_frame_id_v1(&self) -> u64 {
        self.record.source_frame_id
    }

    pub(crate) fn source_frame_sequence_v1(&self) -> u64 {
        self.record.source_frame_sequence
    }

    pub(crate) fn source_frame_sha256_v1(&self) -> &str {
        &self.record.source_frame_sha256
    }
}

/// Checks that the entire player-visible region plan was fixed before input.
/// This is a structural, untrusted boundary. A Windows producer must still
/// rehash every region from an opaque retained DXGI frame.
pub fn check_untrusted_player_visible_gameplay_before_input_v1(
    record: MtgoPlayerVisibleGameplayBeforeInputRecordV1,
) -> Result<CheckedUntrustedMtgoPlayerVisibleGameplayBeforeInputV1, MtgoContractErrorV1> {
    validate_before_input_record_shape_v1(&record)?;
    let checked_gesture =
        validate_player_visible_duel_gesture_plan_v1(record.gesture_plan.clone())?;
    if checked_gesture.selected_action_v1() != &record.player_visible_decision.selected_action {
        return Err(error_v1(
            "player_visible_before_input_action_mismatch",
            "gesture and confirmed decision must retain the exact selected visible action",
        ));
    }
    let primitive = checked_gesture
        .primitives_v1()
        .get(usize::from(record.primitive_index))
        .ok_or_else(|| {
            error_v1(
                "player_visible_before_input_primitive_index",
                record.primitive_index.to_string(),
            )
        })?;
    if record.primitive_is_final
        != (usize::from(record.primitive_index) + 1 == checked_gesture.primitives_v1().len())
    {
        return Err(error_v1(
            "player_visible_before_input_primitive_finality",
            "primitive finality differs from the complete visible gesture plan",
        ));
    }
    let action_family = checked_gesture.action_family_v1();
    let kinds = validate_before_regions_v1(&record)?;
    require_action_specific_transition_v1(
        action_family,
        primitive,
        record.primitive_is_final,
        &kinds,
    )?;
    if let Some(expected) = record
        .expected_game_log_baseline_commitment_sha256
        .as_deref()
    {
        require_sha256_v1(
            expected,
            "player_visible_before_input_game_log_baseline_hash",
        )?;
        if !record.primitive_is_final
            || expected_game_log_kind_v1(&record.player_visible_decision.selected_action).is_none()
        {
            return Err(error_v1(
                "player_visible_before_input_game_log_unsupported",
                "a Game Log baseline is valid only for a supported final visible primitive",
            ));
        }
    }

    let record_json = serde_json::to_vec(&record).map_err(|error| {
        error_v1(
            "player_visible_before_input_serialization",
            error.to_string(),
        )
    })?;
    let family_json = serde_json::to_vec(&action_family).map_err(|error| {
        error_v1(
            "player_visible_before_input_serialization",
            error.to_string(),
        )
    })?;
    let before_input_commitment_sha256 = commitment_v1(
        PLAYER_VISIBLE_GAMEPLAY_BEFORE_INPUT_DOMAIN_V1,
        &[
            &record_json,
            &family_json,
            b"checked_untrusted_visible_only_no_input_or_event_entry",
        ],
    );
    Ok(CheckedUntrustedMtgoPlayerVisibleGameplayBeforeInputV1 {
        record,
        action_family,
        before_input_commitment_sha256,
    })
}

/// Consumes the exact before-input declaration and pairs it with hashes for
/// the same visible rectangles in a newer frame. No region may be added,
/// removed, relabelled, or moved after input.
pub fn complete_untrusted_player_visible_gameplay_postcondition_v1(
    before: CheckedUntrustedMtgoPlayerVisibleGameplayBeforeInputV1,
    after: MtgoPlayerVisibleGameplayAfterFrameV1,
    game_log_corroboration: Option<CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1>,
) -> Result<CheckedUntrustedMtgoPlayerVisibleGameplayPostconditionV1, MtgoContractErrorV1> {
    if after.regions.len() != before.record.regions.len() {
        return Err(error_v1(
            "player_visible_postcondition_after_region_set",
            "after-frame regions must exactly match the complete before-input plan",
        ));
    }
    let mut transitions = Vec::with_capacity(before.record.regions.len());
    for (before_region, after_region) in before.record.regions.iter().zip(&after.regions) {
        if before_region.kind != after_region.kind
            || before_region.rect_client_px != after_region.rect_client_px
        {
            return Err(error_v1(
                "player_visible_postcondition_after_region_identity",
                "after-frame region kind or rectangle differs from the before-input plan",
            ));
        }
        transitions.push(MtgoPlayerVisibleGameplayRegionTransitionV1 {
            kind: before_region.kind,
            rect_client_px: before_region.rect_client_px.clone(),
            before_bgra8_sha256: before_region.before_bgra8_sha256.clone(),
            after_bgra8_sha256: after_region.after_bgra8_sha256.clone(),
        });
    }
    let record = MtgoPlayerVisibleGameplayPostconditionRecordV1 {
        schema_version: MTGO_PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_SCHEMA_V1,
        event_kind: before.record.event_kind,
        event_identity_sha256: before.record.event_identity_sha256,
        match_identity_sha256: before.record.match_identity_sha256,
        game_number: before.record.game_number,
        deployment_commitment_sha256: before.record.deployment_commitment_sha256,
        decision_commitment_sha256: before.record.decision_commitment_sha256,
        selection_commitment_sha256: before.record.selection_commitment_sha256,
        source_frame_id: before.record.source_frame_id,
        source_frame_sequence: before.record.source_frame_sequence,
        source_frame_sha256: before.record.source_frame_sha256,
        after_frame_id: after.after_frame_id,
        after_frame_sequence: after.after_frame_sequence,
        after_frame_sha256: after.after_frame_sha256,
        client_size_px: before.record.client_size_px,
        player_visible_decision: before.record.player_visible_decision,
        gesture_plan: before.record.gesture_plan,
        primitive_index: before.record.primitive_index,
        primitive_is_final: before.record.primitive_is_final,
        transition_set_complete: true,
        transitions,
        expected_game_log_baseline_commitment_sha256: before
            .record
            .expected_game_log_baseline_commitment_sha256,
    };
    check_untrusted_player_visible_gameplay_postcondition_v1(record, game_log_corroboration)
}

/// Checks one exact player-visible gesture stage against action-specific UI
/// changes. When Game Log evidence is present, it must bind the exact baseline
/// declared before input and the corroboration kind must match the selected
/// visible action. It strengthens confirmation but never substitutes for the
/// required pixel-visible transition.
pub fn check_untrusted_player_visible_gameplay_postcondition_v1(
    record: MtgoPlayerVisibleGameplayPostconditionRecordV1,
    game_log_corroboration: Option<CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1>,
) -> Result<CheckedUntrustedMtgoPlayerVisibleGameplayPostconditionV1, MtgoContractErrorV1> {
    validate_record_shape_v1(&record)?;
    let checked_gesture =
        validate_player_visible_duel_gesture_plan_v1(record.gesture_plan.clone())?;
    if checked_gesture.selected_action_v1() != &record.player_visible_decision.selected_action {
        return Err(error_v1(
            "player_visible_postcondition_action_mismatch",
            "gesture and confirmed decision must retain the exact selected visible action",
        ));
    }
    let primitive = checked_gesture
        .primitives_v1()
        .get(usize::from(record.primitive_index))
        .ok_or_else(|| {
            error_v1(
                "player_visible_postcondition_primitive_index",
                record.primitive_index.to_string(),
            )
        })?;
    if record.primitive_is_final
        != (usize::from(record.primitive_index) + 1 == checked_gesture.primitives_v1().len())
    {
        return Err(error_v1(
            "player_visible_postcondition_primitive_finality",
            "primitive finality differs from the complete visible gesture plan",
        ));
    }

    let action_family = checked_gesture.action_family_v1();
    let kinds = validate_transitions_v1(&record)?;
    require_action_specific_transition_v1(
        action_family,
        primitive,
        record.primitive_is_final,
        &kinds,
    )?;

    let game_log_commitment = match (
        record
            .expected_game_log_baseline_commitment_sha256
            .as_deref(),
        game_log_corroboration.as_ref(),
    ) {
        (None, None) => None,
        (Some(expected), Some(corroboration)) => {
            if !record.primitive_is_final {
                return Err(error_v1(
                    "player_visible_postcondition_game_log_finality",
                    "visible Game Log corroboration is valid only after the final gesture primitive",
                ));
            }
            require_sha256_v1(
                expected,
                "player_visible_postcondition_game_log_baseline_hash",
            )?;
            if expected != corroboration.baseline_commitment_sha256_v1() {
                return Err(error_v1(
                    "player_visible_postcondition_game_log_baseline_mismatch",
                    "visible Game Log corroboration belongs to another pre-input baseline",
                ));
            }
            if corroboration.player_visible_decision_commitment_sha256_v1()
                != player_visible_game_log_action_decision_commitment_v1(
                    &record.player_visible_decision,
                )?
            {
                return Err(error_v1(
                    "player_visible_postcondition_game_log_decision_mismatch",
                    "visible Game Log corroboration belongs to another player-visible decision",
                ));
            }
            let expected_kind =
                expected_game_log_kind_v1(&record.player_visible_decision.selected_action)
                    .ok_or_else(|| {
                        error_v1(
                        "player_visible_postcondition_game_log_unsupported",
                        "this visible action family has no exact Game Log corroboration contract",
                    )
                    })?;
            if expected_kind != corroboration.kind_v1() {
                return Err(error_v1(
                    "player_visible_postcondition_game_log_kind",
                    "visible Game Log corroboration changed the selected action family",
                ));
            }
            Some(
                corroboration
                    .corroboration_commitment_sha256_v1()
                    .to_owned(),
            )
        }
        _ => {
            return Err(error_v1(
                "player_visible_postcondition_game_log_pair",
                "Game Log baseline and corroboration must be present or absent together",
            ))
        }
    };

    let record_json = serde_json::to_vec(&record).map_err(|error| {
        error_v1(
            "player_visible_postcondition_serialization",
            error.to_string(),
        )
    })?;
    let family_json = serde_json::to_vec(&action_family).map_err(|error| {
        error_v1(
            "player_visible_postcondition_serialization",
            error.to_string(),
        )
    })?;
    let game_log_commitment = game_log_commitment.as_deref().unwrap_or("none");
    let confirmation_commitment_sha256 = commitment_v1(
        PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_DOMAIN_V1,
        &[
            &record_json,
            &family_json,
            game_log_commitment.as_bytes(),
            b"checked_untrusted_visible_only_no_additional_input_or_event_entry",
        ],
    );
    Ok(CheckedUntrustedMtgoPlayerVisibleGameplayPostconditionV1 {
        record,
        _game_log_corroboration: game_log_corroboration,
        action_family,
        confirmation_commitment_sha256,
    })
}

fn validate_record_shape_v1(
    record: &MtgoPlayerVisibleGameplayPostconditionRecordV1,
) -> Result<(), MtgoContractErrorV1> {
    if record.schema_version != MTGO_PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_SCHEMA_V1 {
        return Err(error_v1(
            "player_visible_postcondition_schema",
            record.schema_version.to_string(),
        ));
    }
    if !(1..=3).contains(&record.game_number)
        || record.source_frame_id == 0
        || record.source_frame_sequence == 0
        || record.after_frame_id == 0
        || record.after_frame_id == record.source_frame_id
        || record.after_frame_sequence <= record.source_frame_sequence
        || record.after_frame_sha256 == record.source_frame_sha256
        || record.client_size_px.width == 0
        || record.client_size_px.height == 0
        || record.player_visible_decision.current_state.acting_player
            != MtgoPlayerRelativeRoleV1::SeatedPlayer
        || action_actor_v1(&record.player_visible_decision.selected_action)
            != MtgoPlayerRelativeRoleV1::SeatedPlayer
    {
        return Err(error_v1(
            "player_visible_postcondition_source",
            "event, source, after-frame, geometry, or seated-player identity is invalid",
        ));
    }
    for (value, code) in [
        (
            &record.event_identity_sha256,
            "player_visible_postcondition_event_hash",
        ),
        (
            &record.match_identity_sha256,
            "player_visible_postcondition_match_hash",
        ),
        (
            &record.deployment_commitment_sha256,
            "player_visible_postcondition_deployment_hash",
        ),
        (
            &record.decision_commitment_sha256,
            "player_visible_postcondition_decision_hash",
        ),
        (
            &record.selection_commitment_sha256,
            "player_visible_postcondition_selection_hash",
        ),
        (
            &record.source_frame_sha256,
            "player_visible_postcondition_source_frame_hash",
        ),
        (
            &record.after_frame_sha256,
            "player_visible_postcondition_after_frame_hash",
        ),
    ] {
        require_sha256_v1(value, code)?;
    }
    Ok(())
}

fn validate_before_input_record_shape_v1(
    record: &MtgoPlayerVisibleGameplayBeforeInputRecordV1,
) -> Result<(), MtgoContractErrorV1> {
    if record.schema_version != MTGO_PLAYER_VISIBLE_GAMEPLAY_BEFORE_INPUT_SCHEMA_V1 {
        return Err(error_v1(
            "player_visible_before_input_schema",
            record.schema_version.to_string(),
        ));
    }
    if !(1..=3).contains(&record.game_number)
        || record.source_frame_id == 0
        || record.source_frame_sequence == 0
        || record.client_size_px.width == 0
        || record.client_size_px.height == 0
        || record.player_visible_decision.current_state.acting_player
            != MtgoPlayerRelativeRoleV1::SeatedPlayer
        || action_actor_v1(&record.player_visible_decision.selected_action)
            != MtgoPlayerRelativeRoleV1::SeatedPlayer
    {
        return Err(error_v1(
            "player_visible_before_input_source",
            "event, source, geometry, or seated-player identity is invalid",
        ));
    }
    for (value, code) in [
        (
            &record.event_identity_sha256,
            "player_visible_before_input_event_hash",
        ),
        (
            &record.match_identity_sha256,
            "player_visible_before_input_match_hash",
        ),
        (
            &record.deployment_commitment_sha256,
            "player_visible_before_input_deployment_hash",
        ),
        (
            &record.decision_commitment_sha256,
            "player_visible_before_input_decision_hash",
        ),
        (
            &record.selection_commitment_sha256,
            "player_visible_before_input_selection_hash",
        ),
        (
            &record.source_frame_sha256,
            "player_visible_before_input_source_frame_hash",
        ),
    ] {
        require_sha256_v1(value, code)?;
    }
    Ok(())
}

fn validate_before_regions_v1(
    record: &MtgoPlayerVisibleGameplayBeforeInputRecordV1,
) -> Result<HashSet<MtgoPlayerVisibleGameplayPostconditionKindV1>, MtgoContractErrorV1> {
    if !record.region_set_complete
        || record.regions.is_empty()
        || record.regions.len() > MAX_PLAYER_VISIBLE_POSTCONDITION_TRANSITIONS_V1
    {
        return Err(error_v1(
            "player_visible_before_input_region_set",
            record.regions.len().to_string(),
        ));
    }
    let mut kinds = HashSet::new();
    let mut rects = Vec::with_capacity(record.regions.len());
    for region in &record.regions {
        require_sha256_v1(
            &region.before_bgra8_sha256,
            "player_visible_before_input_region_hash",
        )?;
        let rect = &region.rect_client_px;
        let right = rect
            .x
            .checked_add(rect.width)
            .ok_or_else(|| error_v1("player_visible_before_input_region", "right edge overflow"))?;
        let bottom = rect.y.checked_add(rect.height).ok_or_else(|| {
            error_v1("player_visible_before_input_region", "bottom edge overflow")
        })?;
        let rect_key = (rect.x, rect.y, rect.width, rect.height);
        if rect.width == 0
            || rect.height == 0
            || right > record.client_size_px.width
            || bottom > record.client_size_px.height
            || !kinds.insert(region.kind)
            || rects
                .iter()
                .any(|existing| transition_rects_intersect_v1(*existing, rect_key))
        {
            return Err(error_v1(
                "player_visible_before_input_region",
                "regions must be unique, nonoverlapping, nonempty, and inside the client",
            ));
        }
        rects.push(rect_key);
    }
    Ok(kinds)
}

fn validate_transitions_v1(
    record: &MtgoPlayerVisibleGameplayPostconditionRecordV1,
) -> Result<HashSet<MtgoPlayerVisibleGameplayPostconditionKindV1>, MtgoContractErrorV1> {
    if !record.transition_set_complete
        || record.transitions.is_empty()
        || record.transitions.len() > MAX_PLAYER_VISIBLE_POSTCONDITION_TRANSITIONS_V1
    {
        return Err(error_v1(
            "player_visible_postcondition_transition_set",
            record.transitions.len().to_string(),
        ));
    }
    let mut kinds = HashSet::new();
    let mut rects = Vec::with_capacity(record.transitions.len());
    for transition in &record.transitions {
        require_sha256_v1(
            &transition.before_bgra8_sha256,
            "player_visible_postcondition_before_region_hash",
        )?;
        require_sha256_v1(
            &transition.after_bgra8_sha256,
            "player_visible_postcondition_after_region_hash",
        )?;
        let rect = &transition.rect_client_px;
        let right = rect.x.checked_add(rect.width).ok_or_else(|| {
            error_v1("player_visible_postcondition_region", "right edge overflow")
        })?;
        let bottom = rect.y.checked_add(rect.height).ok_or_else(|| {
            error_v1(
                "player_visible_postcondition_region",
                "bottom edge overflow",
            )
        })?;
        let rect_key = (rect.x, rect.y, rect.width, rect.height);
        if rect.width == 0
            || rect.height == 0
            || right > record.client_size_px.width
            || bottom > record.client_size_px.height
            || transition.before_bgra8_sha256 == transition.after_bgra8_sha256
            || !kinds.insert(transition.kind)
            || rects
                .iter()
                .any(|existing| transition_rects_intersect_v1(*existing, rect_key))
        {
            return Err(error_v1(
                "player_visible_postcondition_region",
                "transition regions must be changed, nonoverlapping, nonempty, and inside the client",
            ));
        }
        rects.push(rect_key);
    }
    Ok(kinds)
}

fn transition_rects_intersect_v1(left: (u32, u32, u32, u32), right: (u32, u32, u32, u32)) -> bool {
    let (left_x, left_y, left_width, left_height) = left;
    let (right_x, right_y, right_width, right_height) = right;
    let left_right = u64::from(left_x) + u64::from(left_width);
    let left_bottom = u64::from(left_y) + u64::from(left_height);
    let right_right = u64::from(right_x) + u64::from(right_width);
    let right_bottom = u64::from(right_y) + u64::from(right_height);
    u64::from(left_x) < right_right
        && u64::from(right_x) < left_right
        && u64::from(left_y) < right_bottom
        && u64::from(right_y) < left_bottom
}

fn require_action_specific_transition_v1(
    family: MtgoDuelActionFamilyV1,
    primitive: &MtgoPlayerVisibleDuelGesturePrimitiveV1,
    is_final: bool,
    kinds: &HashSet<MtgoPlayerVisibleGameplayPostconditionKindV1>,
) -> Result<(), MtgoContractErrorV1> {
    use MtgoPlayerVisibleGameplayPostconditionKindV1 as K;
    if !is_final {
        if kinds.contains(&K::ChoiceSurfaceChanged) || kinds.contains(&K::SelectedControlChanged) {
            return Ok(());
        }
        return Err(error_v1(
            "player_visible_postcondition_intermediate_transition",
            "an intermediate primitive must visibly change its selected control or choice surface",
        ));
    }
    let any = |allowed: &[K]| allowed.iter().any(|kind| kinds.contains(kind));
    let satisfied = match family {
        MtgoDuelActionFamilyV1::PriorityPass => any(&[K::PromptChanged, K::PhaseBarChanged]),
        MtgoDuelActionFamilyV1::PlayLand => {
            kinds.contains(&K::BattlefieldChanged)
                && any(&[
                    K::HandChanged,
                    K::GraveyardChanged,
                    K::LibraryChanged,
                    K::ExileChanged,
                ])
        }
        MtgoDuelActionFamilyV1::CastOrPlotSpell => {
            any(&[
                K::HandChanged,
                K::GraveyardChanged,
                K::LibraryChanged,
                K::ExileChanged,
            ]) && any(&[K::StackChanged, K::ExileChanged, K::PromptChanged])
        }
        MtgoDuelActionFamilyV1::ManaAbility => {
            kinds.contains(&K::ManaPoolChanged)
                && any(&[
                    K::SelectedControlChanged,
                    K::BattlefieldChanged,
                    K::PlayerCountsChanged,
                ])
        }
        MtgoDuelActionFamilyV1::NonManaAbility => any(&[
            K::StackChanged,
            K::ChoiceSurfaceChanged,
            K::ManaPoolChanged,
            K::BattlefieldChanged,
        ]),
        MtgoDuelActionFamilyV1::TargetChoice
        | MtgoDuelActionFamilyV1::CostOrModeChoice
        | MtgoDuelActionFamilyV1::EffectChoice => {
            any(&[K::ChoiceSurfaceChanged, K::PromptChanged, K::StackChanged])
        }
        MtgoDuelActionFamilyV1::Discard => kinds.contains(&K::HandChanged),
        MtgoDuelActionFamilyV1::CombatChoice => kinds.contains(&K::CombatChanged),
        MtgoDuelActionFamilyV1::TriggerOrdering => any(&[K::ChoiceSurfaceChanged, K::StackChanged]),
    };
    if !satisfied {
        return Err(error_v1(
            "player_visible_postcondition_action_transition",
            format!(
                "the final {family:?} primitive lacks its required player-visible transition ({primitive:?})"
            ),
        ));
    }
    Ok(())
}

fn expected_game_log_kind_v1(
    action: &MtgoPlayerVisibleDuelActionV1,
) -> Option<MtgoVisibleGameLogActionCorroborationKindV1> {
    match action {
        MtgoPlayerVisibleDuelActionV1::PlayLand { .. } => {
            Some(MtgoVisibleGameLogActionCorroborationKindV1::PlayedCard)
        }
        MtgoPlayerVisibleDuelActionV1::CastSpell { .. } => {
            Some(MtgoVisibleGameLogActionCorroborationKindV1::CastSpell)
        }
        MtgoPlayerVisibleDuelActionV1::Discard { .. } => {
            Some(MtgoVisibleGameLogActionCorroborationKindV1::DiscardedCards)
        }
        MtgoPlayerVisibleDuelActionV1::DeclareAttackers { .. } => {
            Some(MtgoVisibleGameLogActionCorroborationKindV1::DeclaredAttackers)
        }
        _ => None,
    }
}

fn action_actor_v1(action: &MtgoPlayerVisibleDuelActionV1) -> MtgoPlayerRelativeRoleV1 {
    use MtgoPlayerVisibleDuelActionV1::*;
    match action {
        Pass { actor }
        | PlayLand { actor, .. }
        | CastSpell { actor, .. }
        | ActivateManaAbility { actor, .. }
        | ActivateAbility { actor, .. }
        | PlotSpell { actor, .. }
        | ChooseTarget { actor, .. }
        | ChooseCostTarget { actor, .. }
        | ChooseCastMode { actor, .. }
        | ChooseKicker { actor, .. }
        | ChooseSpellMode { actor, .. }
        | ChooseEffectOption { actor, .. }
        | ChooseEffectTarget { actor, .. }
        | FinishEffectSelection { actor, .. }
        | ChooseEffectColor { actor, .. }
        | ChooseEffectNumber { actor, .. }
        | ChooseEffectBoolean { actor, .. }
        | FinishTargetSelection { actor, .. }
        | ChooseOptionalCostUse { actor, .. }
        | ChooseOptionalCostWhich { actor, .. }
        | ChooseSpellCopyPayment { actor, .. }
        | ChooseSpellCopyRetarget { actor, .. }
        | ChooseMadnessCast { actor, .. }
        | Discard { actor, .. }
        | DeclareAttackers { actor, .. }
        | DeclareBlockersForAttacker { actor, .. }
        | ChooseAttackerInclusion { actor, .. }
        | ChooseBlockerInclusion { actor, .. }
        | OrderTriggers { actor, .. } => *actor,
    }
}

fn require_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(error_v1(code, value));
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
        begin_checked_untrusted_player_visible_game_log_action_baseline_v1,
        classify_checked_untrusted_mtgo_visible_game_log_semantics_v1,
        corroborate_checked_untrusted_player_visible_game_log_action_v1,
        parse_checked_untrusted_mtgo_visible_game_log_v1, MtgoPlayerVisibleBlockerAssignmentV1,
        MtgoPlayerVisibleCombatStateV1, MtgoPlayerVisibleDuelStateV1, MtgoPlayerVisibleNamedCardV1,
        MtgoPlayerVisibleObjectRefV1,
    };
    use mtg_kernel::rl::ZoneIndependentStepV1;

    fn digest(character: char) -> String {
        std::iter::repeat_n(character, 64).collect()
    }

    fn state_v1() -> MtgoPlayerVisibleDuelStateV1 {
        MtgoPlayerVisibleDuelStateV1 {
            acting_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            turn: 1,
            phase: ZoneIndependentStepV1::Main1,
            active_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            initiative: None,
            life_totals: [20, 20],
            mana_pools: [[0; 6]; 2],
            hand_counts: [7, 7],
            library_counts: [53, 53],
            battlefield: [Vec::new(), Vec::new()],
            graveyards: [Vec::new(), Vec::new()],
            exile: Vec::new(),
            stack: Vec::new(),
            combat: MtgoPlayerVisibleCombatStateV1 {
                attackers_declared: false,
                blockers_declared: false,
                ordered_attackers: Vec::new(),
                blocker_assignments: Vec::<MtgoPlayerVisibleBlockerAssignmentV1>::new(),
            },
            visible_object_relations: Vec::new(),
            own_hand: Vec::new(),
            known_library_cards: [Vec::new(), Vec::new()],
            known_hand_cards: [Vec::new(), Vec::new()],
        }
    }

    fn pass_record_v1() -> MtgoPlayerVisibleGameplayPostconditionRecordV1 {
        let action = MtgoPlayerVisibleDuelActionV1::Pass {
            actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
        };
        MtgoPlayerVisibleGameplayPostconditionRecordV1 {
            schema_version: MTGO_PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_SCHEMA_V1,
            event_kind: MtgoCompetitiveEventKindV1::League,
            event_identity_sha256: digest('a'),
            match_identity_sha256: digest('b'),
            game_number: 1,
            deployment_commitment_sha256: digest('c'),
            decision_commitment_sha256: digest('d'),
            selection_commitment_sha256: digest('e'),
            source_frame_id: 10,
            source_frame_sequence: 20,
            source_frame_sha256: digest('f'),
            after_frame_id: 11,
            after_frame_sequence: 21,
            after_frame_sha256: digest('0'),
            client_size_px: MtgoSizePxV1 {
                width: 1280,
                height: 720,
            },
            player_visible_decision: MtgoPlayerVisibleConfirmedDuelDecisionV1 {
                current_state: state_v1(),
                selected_action: action.clone(),
            },
            gesture_plan: MtgoPlayerVisibleDuelGesturePlanV1 {
                schema_version: 1,
                selected_action: action,
                primitive_set_complete: true,
                primitives: vec![MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivatePrimary {
                    activation: crate::MtgoDuelPrimaryActivationV1::SingleLeftClick,
                }],
            },
            primitive_index: 0,
            primitive_is_final: true,
            transition_set_complete: true,
            transitions: vec![MtgoPlayerVisibleGameplayRegionTransitionV1 {
                kind: MtgoPlayerVisibleGameplayPostconditionKindV1::PromptChanged,
                rect_client_px: MtgoRectPxV1 {
                    x: 100,
                    y: 40,
                    width: 300,
                    height: 80,
                },
                before_bgra8_sha256: digest('1'),
                after_bgra8_sha256: digest('2'),
            }],
            expected_game_log_baseline_commitment_sha256: None,
        }
    }

    fn write_dotnet_string_v1(target: &mut Vec<u8>, value: &str) {
        assert!(value.len() < 0x80);
        target.push(value.len() as u8);
        target.extend_from_slice(value.as_bytes());
    }

    fn visible_log_semantics_v1(
        lines: &[&str],
    ) -> crate::CheckedUntrustedMtgoVisibleGameLogSemanticProjectionV1 {
        const SOURCE_ID: &str = "82ba951e-fe1f-423a-9cdb-bfe879ea2c6b";
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        write_dotnet_string_v1(&mut bytes, SOURCE_ID);
        bytes.extend_from_slice(&4_u16.to_le_bytes());
        write_dotnet_string_v1(&mut bytes, SOURCE_ID);
        for (index, line) in lines.iter().enumerate() {
            bytes.extend_from_slice(&(638_905_999_999_999_999 + index as u64).to_le_bytes());
            bytes.push(0);
            write_dotnet_string_v1(&mut bytes, line);
        }
        let source = parse_checked_untrusted_mtgo_visible_game_log_v1(&bytes).unwrap();
        classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&source, "UnbuckledPie")
            .unwrap()
    }

    fn play_land_record_v1() -> MtgoPlayerVisibleGameplayPostconditionRecordV1 {
        let source = MtgoPlayerVisibleObjectRefV1 { visible_ordinal: 1 };
        let action = MtgoPlayerVisibleDuelActionV1::PlayLand {
            actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            source,
        };
        let mut record = pass_record_v1();
        record
            .player_visible_decision
            .current_state
            .own_hand
            .push(MtgoPlayerVisibleNamedCardV1 {
                object_ref: source,
                card_name: "Mountain".to_owned(),
            });
        record.player_visible_decision.selected_action = action.clone();
        record.gesture_plan.selected_action = action;
        record.transitions = vec![
            MtgoPlayerVisibleGameplayRegionTransitionV1 {
                kind: MtgoPlayerVisibleGameplayPostconditionKindV1::BattlefieldChanged,
                rect_client_px: MtgoRectPxV1 {
                    x: 100,
                    y: 200,
                    width: 500,
                    height: 250,
                },
                before_bgra8_sha256: digest('3'),
                after_bgra8_sha256: digest('4'),
            },
            MtgoPlayerVisibleGameplayRegionTransitionV1 {
                kind: MtgoPlayerVisibleGameplayPostconditionKindV1::HandChanged,
                rect_client_px: MtgoRectPxV1 {
                    x: 100,
                    y: 500,
                    width: 500,
                    height: 180,
                },
                before_bgra8_sha256: digest('5'),
                after_bgra8_sha256: digest('6'),
            },
        ];
        record
    }

    fn before_input_from_record_v1(
        record: &MtgoPlayerVisibleGameplayPostconditionRecordV1,
    ) -> MtgoPlayerVisibleGameplayBeforeInputRecordV1 {
        MtgoPlayerVisibleGameplayBeforeInputRecordV1 {
            schema_version: MTGO_PLAYER_VISIBLE_GAMEPLAY_BEFORE_INPUT_SCHEMA_V1,
            event_kind: record.event_kind,
            event_identity_sha256: record.event_identity_sha256.clone(),
            match_identity_sha256: record.match_identity_sha256.clone(),
            game_number: record.game_number,
            deployment_commitment_sha256: record.deployment_commitment_sha256.clone(),
            decision_commitment_sha256: record.decision_commitment_sha256.clone(),
            selection_commitment_sha256: record.selection_commitment_sha256.clone(),
            source_frame_id: record.source_frame_id,
            source_frame_sequence: record.source_frame_sequence,
            source_frame_sha256: record.source_frame_sha256.clone(),
            client_size_px: record.client_size_px.clone(),
            player_visible_decision: record.player_visible_decision.clone(),
            gesture_plan: record.gesture_plan.clone(),
            primitive_index: record.primitive_index,
            primitive_is_final: record.primitive_is_final,
            region_set_complete: record.transition_set_complete,
            regions: record
                .transitions
                .iter()
                .map(|transition| MtgoPlayerVisibleGameplayBeforeRegionV1 {
                    kind: transition.kind,
                    rect_client_px: transition.rect_client_px.clone(),
                    before_bgra8_sha256: transition.before_bgra8_sha256.clone(),
                })
                .collect(),
            expected_game_log_baseline_commitment_sha256: record
                .expected_game_log_baseline_commitment_sha256
                .clone(),
        }
    }

    fn after_frame_from_record_v1(
        record: &MtgoPlayerVisibleGameplayPostconditionRecordV1,
    ) -> MtgoPlayerVisibleGameplayAfterFrameV1 {
        MtgoPlayerVisibleGameplayAfterFrameV1 {
            after_frame_id: record.after_frame_id,
            after_frame_sequence: record.after_frame_sequence,
            after_frame_sha256: record.after_frame_sha256.clone(),
            regions: record
                .transitions
                .iter()
                .map(|transition| MtgoPlayerVisibleGameplayAfterRegionV1 {
                    kind: transition.kind,
                    rect_client_px: transition.rect_client_px.clone(),
                    after_bgra8_sha256: transition.after_bgra8_sha256.clone(),
                })
                .collect(),
        }
    }

    fn play_land_corroboration_v1() -> (
        String,
        CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1,
    ) {
        let before = visible_log_semantics_v1(&["@PUnbuckledPie joined the game."]);
        let after = visible_log_semantics_v1(&[
            "@PUnbuckledPie joined the game.",
            "@PUnbuckledPie plays @[Mountain@:296710,467:@].",
        ]);
        let record = play_land_record_v1();
        let baseline = begin_checked_untrusted_player_visible_game_log_action_baseline_v1(
            &before,
            &record.player_visible_decision,
        )
        .unwrap();
        let baseline_commitment = baseline.baseline_commitment_sha256_v1().to_owned();
        let corroboration =
            corroborate_checked_untrusted_player_visible_game_log_action_v1(baseline, &after)
                .unwrap();
        (baseline_commitment, corroboration)
    }

    #[test]
    fn pass_requires_specific_newer_visible_transition_and_grants_no_authority() {
        let checked =
            check_untrusted_player_visible_gameplay_postcondition_v1(pass_record_v1(), None)
                .unwrap();
        assert_eq!(
            checked.action_family_v1(),
            MtgoDuelActionFamilyV1::PriorityPass
        );
        assert_eq!(checked.after_frame_id_v1(), 11);
        assert_eq!(checked.after_frame_sequence_v1(), 21);
        assert_eq!(checked.confirmation_commitment_sha256_v1().len(), 64);
        assert!(!checked.has_exact_visible_game_log_corroboration_v1());
        assert!(!checked.safe_for_additional_input_v1());
        assert!(!checked.permits_event_entry_v1());
        assert!(!checked.permits_spending_v1());
    }

    #[test]
    fn every_action_family_requires_a_declared_visible_effect() {
        use MtgoDuelActionFamilyV1 as F;
        use MtgoPlayerVisibleGameplayPostconditionKindV1 as K;
        let primitive = MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivatePrimary {
            activation: crate::MtgoDuelPrimaryActivationV1::SingleLeftClick,
        };
        let cases = [
            (F::PriorityPass, vec![K::PromptChanged]),
            (F::PlayLand, vec![K::BattlefieldChanged, K::HandChanged]),
            (F::CastOrPlotSpell, vec![K::HandChanged, K::StackChanged]),
            (
                F::ManaAbility,
                vec![K::ManaPoolChanged, K::SelectedControlChanged],
            ),
            (F::NonManaAbility, vec![K::StackChanged]),
            (F::TargetChoice, vec![K::ChoiceSurfaceChanged]),
            (F::CostOrModeChoice, vec![K::PromptChanged]),
            (F::EffectChoice, vec![K::ChoiceSurfaceChanged]),
            (F::Discard, vec![K::HandChanged]),
            (F::CombatChoice, vec![K::CombatChanged]),
            (F::TriggerOrdering, vec![K::StackChanged]),
        ];
        for (family, kinds) in cases {
            let kinds = kinds.into_iter().collect::<HashSet<_>>();
            require_action_specific_transition_v1(family, &primitive, true, &kinds).unwrap();
        }

        let incomplete_land = [K::BattlefieldChanged].into_iter().collect::<HashSet<_>>();
        assert_eq!(
            require_action_specific_transition_v1(F::PlayLand, &primitive, true, &incomplete_land,)
                .unwrap_err()
                .code(),
            "player_visible_postcondition_action_transition"
        );

        for visible_source_zone in [
            K::HandChanged,
            K::GraveyardChanged,
            K::LibraryChanged,
            K::ExileChanged,
        ] {
            let land = [K::BattlefieldChanged, visible_source_zone]
                .into_iter()
                .collect::<HashSet<_>>();
            require_action_specific_transition_v1(F::PlayLand, &primitive, true, &land).unwrap();

            let spell = [visible_source_zone, K::StackChanged]
                .into_iter()
                .collect::<HashSet<_>>();
            require_action_specific_transition_v1(F::CastOrPlotSpell, &primitive, true, &spell)
                .unwrap();
        }
    }

    #[test]
    fn visible_only_confirmations_begin_and_extend_exact_game_history() {
        let first =
            check_untrusted_player_visible_gameplay_postcondition_v1(pass_record_v1(), None)
                .unwrap();
        let history = crate::begin_checked_untrusted_competitive_player_visible_game_history_from_player_visible_postcondition_v1(
            "league-visible-only-history-v1",
            first,
        )
        .unwrap();
        assert_eq!(history.decision_count_v1(), 1);

        let mut second_record = pass_record_v1();
        second_record.decision_commitment_sha256 = digest('6');
        second_record.selection_commitment_sha256 = digest('7');
        second_record.source_frame_id = 12;
        second_record.source_frame_sequence = 22;
        second_record.source_frame_sha256 = digest('8');
        second_record.after_frame_id = 13;
        second_record.after_frame_sequence = 23;
        second_record.after_frame_sha256 = digest('9');
        let second =
            check_untrusted_player_visible_gameplay_postcondition_v1(second_record, None).unwrap();
        let history = crate::append_checked_untrusted_competitive_player_visible_game_history_from_player_visible_postcondition_v1(
            history,
            second,
        )
        .unwrap();
        assert_eq!(history.decision_count_v1(), 2);
        assert_eq!(history.decision_v1(1).unwrap().sequence_v1(), 2);
    }

    fn error_code_v1<T>(result: Result<T, MtgoContractErrorV1>) -> &'static str {
        match result {
            Ok(_) => panic!("expected player-visible postcondition rejection"),
            Err(error) => error.code(),
        }
    }

    #[test]
    fn generic_or_unchanged_screen_deltas_do_not_confirm_pass() {
        let mut wrong = pass_record_v1();
        wrong.transitions[0].kind =
            MtgoPlayerVisibleGameplayPostconditionKindV1::BattlefieldChanged;
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_postcondition_v1(
                wrong, None,
            )),
            "player_visible_postcondition_action_transition"
        );

        let mut unchanged = pass_record_v1();
        unchanged.transitions[0].after_bgra8_sha256 =
            unchanged.transitions[0].before_bgra8_sha256.clone();
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_postcondition_v1(
                unchanged, None,
            )),
            "player_visible_postcondition_region"
        );
    }

    #[test]
    fn frame_order_action_identity_and_gesture_finality_are_exact() {
        let mut stale = pass_record_v1();
        stale.after_frame_sequence = stale.source_frame_sequence;
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_postcondition_v1(
                stale, None,
            )),
            "player_visible_postcondition_source"
        );

        let mut changed_action = pass_record_v1();
        changed_action.gesture_plan.selected_action = MtgoPlayerVisibleDuelActionV1::PlayLand {
            actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            source: MtgoPlayerVisibleObjectRefV1 { visible_ordinal: 1 },
        };
        assert!(
            check_untrusted_player_visible_gameplay_postcondition_v1(changed_action, None).is_err()
        );

        let mut wrong_finality = pass_record_v1();
        wrong_finality.primitive_is_final = false;
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_postcondition_v1(
                wrong_finality,
                None,
            )),
            "player_visible_postcondition_primitive_finality"
        );
    }

    #[test]
    fn incomplete_duplicate_overlapping_and_out_of_bounds_transition_sets_fail_closed() {
        let mut incomplete = pass_record_v1();
        incomplete.transition_set_complete = false;
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_postcondition_v1(
                incomplete, None,
            )),
            "player_visible_postcondition_transition_set"
        );

        let mut duplicate = pass_record_v1();
        duplicate.transitions.push(duplicate.transitions[0].clone());
        duplicate.transitions[1].after_bgra8_sha256 = digest('3');
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_postcondition_v1(
                duplicate, None,
            )),
            "player_visible_postcondition_region"
        );

        let mut overlapping = play_land_record_v1();
        overlapping.transitions[1].rect_client_px.y = 400;
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_postcondition_v1(
                overlapping,
                None,
            )),
            "player_visible_postcondition_region"
        );

        let mut outside = pass_record_v1();
        outside.transitions[0].rect_client_px.x = 1_200;
        outside.transitions[0].rect_client_px.width = 100;
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_postcondition_v1(
                outside, None,
            )),
            "player_visible_postcondition_region"
        );
    }

    #[test]
    fn log_baseline_and_corroboration_must_be_paired() {
        let mut record = pass_record_v1();
        record.expected_game_log_baseline_commitment_sha256 = Some(digest('9'));
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_postcondition_v1(
                record, None,
            )),
            "player_visible_postcondition_game_log_pair"
        );
    }

    #[test]
    fn game_log_corroboration_binds_the_exact_visible_decision_not_only_its_family() {
        let (baseline, corroboration) = play_land_corroboration_v1();
        let mut exact = play_land_record_v1();
        exact.expected_game_log_baseline_commitment_sha256 = Some(baseline);
        let checked =
            check_untrusted_player_visible_gameplay_postcondition_v1(exact, Some(corroboration))
                .unwrap();
        assert!(checked.has_exact_visible_game_log_corroboration_v1());

        let (baseline, corroboration) = play_land_corroboration_v1();
        let mut substituted = play_land_record_v1();
        let forest = MtgoPlayerVisibleObjectRefV1 { visible_ordinal: 2 };
        substituted
            .player_visible_decision
            .current_state
            .own_hand
            .push(MtgoPlayerVisibleNamedCardV1 {
                object_ref: forest,
                card_name: "Forest".to_owned(),
            });
        let changed_action = MtgoPlayerVisibleDuelActionV1::PlayLand {
            actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            source: forest,
        };
        substituted.player_visible_decision.selected_action = changed_action.clone();
        substituted.gesture_plan.selected_action = changed_action;
        substituted.expected_game_log_baseline_commitment_sha256 = Some(baseline);
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_postcondition_v1(
                substituted,
                Some(corroboration),
            )),
            "player_visible_postcondition_game_log_decision_mismatch"
        );
    }

    #[test]
    fn game_log_corroboration_cannot_confirm_an_intermediate_primitive() {
        let cards = [
            MtgoPlayerVisibleObjectRefV1 { visible_ordinal: 1 },
            MtgoPlayerVisibleObjectRefV1 { visible_ordinal: 2 },
        ];
        let action = MtgoPlayerVisibleDuelActionV1::Discard {
            actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            cards: cards.to_vec(),
        };
        let mut record = pass_record_v1();
        record
            .player_visible_decision
            .current_state
            .own_hand
            .extend([
                MtgoPlayerVisibleNamedCardV1 {
                    object_ref: cards[0],
                    card_name: "Lava Spike".to_owned(),
                },
                MtgoPlayerVisibleNamedCardV1 {
                    object_ref: cards[1],
                    card_name: "Lightning Bolt".to_owned(),
                },
            ]);
        record.player_visible_decision.selected_action = action.clone();
        record.gesture_plan.selected_action = action;
        record.gesture_plan.primitives = vec![
            MtgoPlayerVisibleDuelGesturePrimitiveV1::SelectVisibleObject { object: cards[0] },
            MtgoPlayerVisibleDuelGesturePrimitiveV1::SelectVisibleObject { object: cards[1] },
            MtgoPlayerVisibleDuelGesturePrimitiveV1::Submit,
        ];
        record.primitive_is_final = false;
        record.transitions = vec![MtgoPlayerVisibleGameplayRegionTransitionV1 {
            kind: MtgoPlayerVisibleGameplayPostconditionKindV1::SelectedControlChanged,
            rect_client_px: MtgoRectPxV1 {
                x: 100,
                y: 200,
                width: 500,
                height: 250,
            },
            before_bgra8_sha256: digest('3'),
            after_bgra8_sha256: digest('4'),
        }];
        let before = visible_log_semantics_v1(&["@PUnbuckledPie joined the game."]);
        let after = visible_log_semantics_v1(&[
            "@PUnbuckledPie joined the game.",
            "@PUnbuckledPie discards @[Lava Spike@:5,6:@].",
            "@PUnbuckledPie discards @[Lightning Bolt@:7,8:@].",
        ]);
        let baseline = begin_checked_untrusted_player_visible_game_log_action_baseline_v1(
            &before,
            &record.player_visible_decision,
        )
        .unwrap();
        let baseline_commitment = baseline.baseline_commitment_sha256_v1().to_owned();
        let corroboration =
            corroborate_checked_untrusted_player_visible_game_log_action_v1(baseline, &after)
                .unwrap();
        record.expected_game_log_baseline_commitment_sha256 = Some(baseline_commitment);
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_postcondition_v1(
                record,
                Some(corroboration),
            )),
            "player_visible_postcondition_game_log_finality"
        );
    }

    #[test]
    fn complete_before_input_region_plan_pairs_only_the_same_newer_visible_regions() {
        let final_record = play_land_record_v1();
        let before_record = before_input_from_record_v1(&final_record);
        let before = check_untrusted_player_visible_gameplay_before_input_v1(before_record)
            .expect("complete before-input plan");
        assert_eq!(before.action_family_v1(), MtgoDuelActionFamilyV1::PlayLand);
        assert!(!before.safe_for_input_v1());
        assert!(!before.permits_event_entry_v1());
        assert!(!before.permits_spending_v1());
        assert_eq!(before.before_input_commitment_sha256_v1().len(), 64);

        let checked = complete_untrusted_player_visible_gameplay_postcondition_v1(
            before,
            after_frame_from_record_v1(&final_record),
            None,
        )
        .expect("same visible regions changed in a newer frame");
        assert_eq!(checked.action_family_v1(), MtgoDuelActionFamilyV1::PlayLand);
        assert_eq!(checked.after_frame_id_v1(), final_record.after_frame_id);
    }

    #[test]
    fn before_input_plan_rejects_incomplete_missing_and_overlapping_regions() {
        let record = play_land_record_v1();

        let mut incomplete = before_input_from_record_v1(&record);
        incomplete.region_set_complete = false;
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_before_input_v1(
                incomplete,
            )),
            "player_visible_before_input_region_set"
        );

        let mut missing = before_input_from_record_v1(&record);
        missing.regions.pop();
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_before_input_v1(
                missing,
            )),
            "player_visible_postcondition_action_transition"
        );

        let mut overlapping = before_input_from_record_v1(&record);
        overlapping.regions[1].rect_client_px.y = overlapping.regions[0].rect_client_px.y;
        assert_eq!(
            error_code_v1(check_untrusted_player_visible_gameplay_before_input_v1(
                overlapping,
            )),
            "player_visible_before_input_region"
        );
    }

    #[test]
    fn after_frame_cannot_change_the_predeclared_region_set_or_reuse_source_pixels() {
        let record = play_land_record_v1();

        let before = check_untrusted_player_visible_gameplay_before_input_v1(
            before_input_from_record_v1(&record),
        )
        .unwrap();
        let mut missing = after_frame_from_record_v1(&record);
        missing.regions.pop();
        assert_eq!(
            error_code_v1(complete_untrusted_player_visible_gameplay_postcondition_v1(
                before, missing, None,
            )),
            "player_visible_postcondition_after_region_set"
        );

        let before = check_untrusted_player_visible_gameplay_before_input_v1(
            before_input_from_record_v1(&record),
        )
        .unwrap();
        let mut moved = after_frame_from_record_v1(&record);
        moved.regions[0].rect_client_px.x += 1;
        assert_eq!(
            error_code_v1(complete_untrusted_player_visible_gameplay_postcondition_v1(
                before, moved, None,
            )),
            "player_visible_postcondition_after_region_identity"
        );

        let before = check_untrusted_player_visible_gameplay_before_input_v1(
            before_input_from_record_v1(&record),
        )
        .unwrap();
        let mut unchanged = after_frame_from_record_v1(&record);
        unchanged.regions[0].after_bgra8_sha256 = record.transitions[0].before_bgra8_sha256.clone();
        assert_eq!(
            error_code_v1(complete_untrusted_player_visible_gameplay_postcondition_v1(
                before, unchanged, None,
            )),
            "player_visible_postcondition_region"
        );
    }
}
