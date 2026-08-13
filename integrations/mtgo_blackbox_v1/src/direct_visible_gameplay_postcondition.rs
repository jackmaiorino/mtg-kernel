use crate::{
    player_visible_duel_action_family_v1, player_visible_game_log_action_decision_commitment_v1,
    CheckedUntrustedMtgoDirectVisibleCompetitiveActionPlanV1,
    CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1, MtgoContractErrorV1,
    MtgoDuelActionFamilyV1, MtgoPlayerVisibleConfirmedDuelDecisionV1,
    MtgoPlayerVisibleGameplayPostconditionKindV1, MtgoRectPxV1, MtgoSizePxV1,
    MtgoVisibleGameLogActionCorroborationKindV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_DIRECT_VISIBLE_GAMEPLAY_BEFORE_DISPATCH_SCHEMA_V1: u32 = 1;
pub const MTGO_DIRECT_VISIBLE_GAMEPLAY_AFTER_DISPATCH_SCHEMA_V1: u32 = 1;

const DIRECT_VISIBLE_BEFORE_DISPATCH_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-gameplay-before-dispatch-v1";
const DIRECT_VISIBLE_POSTCONDITION_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-gameplay-postcondition-v1";
const MAX_TRANSITIONS_V1: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDirectVisibleGameplayBeforeRegionV1 {
    pub kind: MtgoPlayerVisibleGameplayPostconditionKindV1,
    pub rect_client_px: MtgoRectPxV1,
    pub before_bgra8_sha256: String,
}

/// Action-specific visible regions fixed before the private client action is
/// dispatched. This contains no dispatch command, client object, input method,
/// process handle, or hidden client fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDirectVisibleGameplayBeforeDispatchRecordV1 {
    pub schema_version: u32,
    pub direct_competitive_scope_commitment_sha256: String,
    pub source_frame_id: u64,
    pub source_frame_sequence: u64,
    pub source_captured_at_unix_millis: u128,
    pub source_frame_sha256: String,
    pub client_size_px: MtgoSizePxV1,
    pub region_set_complete: bool,
    pub regions: Vec<MtgoDirectVisibleGameplayBeforeRegionV1>,
    pub expected_game_log_baseline_commitment_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDirectVisibleGameplayAfterRegionV1 {
    pub kind: MtgoPlayerVisibleGameplayPostconditionKindV1,
    pub rect_client_px: MtgoRectPxV1,
    pub after_bgra8_sha256: String,
}

/// A newer composed-frame observation captured after the fixed submitted
/// receipt from the sealed producer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDirectVisibleGameplayAfterDispatchFrameV1 {
    pub schema_version: u32,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub captured_at_unix_millis: u128,
    pub after_frame_sha256: String,
    pub client_size_px: MtgoSizePxV1,
    pub regions: Vec<MtgoDirectVisibleGameplayAfterRegionV1>,
}

/// Move-only pre-dispatch visible postcondition plan. Its only consumer is the
/// Windows dispatch owner. Public callers can inspect commitments and fixed
/// false authority flags, never rectangles or client details.
pub struct CheckedUntrustedMtgoDirectVisibleGameplayBeforeDispatchV1 {
    plan: CheckedUntrustedMtgoDirectVisibleCompetitiveActionPlanV1,
    record: MtgoDirectVisibleGameplayBeforeDispatchRecordV1,
    action_family: MtgoDuelActionFamilyV1,
    before_dispatch_commitment_sha256: String,
}

/// Coordinate-free dispatch join consumed by the Windows operator. These are
/// commitments to the exact approved match and refreshed player-visible model
/// selection. They are insufficient to perform input without the opaque
/// exact-game session, input gate, sealed producer invocation, and a newer
/// visible postcondition.
pub struct MtgoDirectVisibleGameplayDispatchCommitmentsV1<'a> {
    event_kind: crate::MtgoCompetitiveEventKindV1,
    game_number: u8,
    event_identity_sha256: &'a str,
    match_identity_sha256: &'a str,
    deployment_commitment_sha256: &'a str,
    mode_authorization_commitment_sha256: &'a str,
    gameplay_authorization_commitment_sha256: &'a str,
    decision_commitment_sha256: &'a str,
    selection_commitment_sha256: &'a str,
    refresh_commitment_sha256: &'a str,
    exact_producer_result_sha256: &'a str,
    selected_index: usize,
    source_frame_id: u64,
    source_frame_sequence: u64,
    source_captured_at_unix_millis: u128,
    source_frame_sha256: &'a str,
    broker_binary_sha256: &'a str,
    producer_binary_sha256: &'a str,
    direct_competitive_scope_commitment_sha256: &'a str,
    before_dispatch_commitment_sha256: &'a str,
}

impl MtgoDirectVisibleGameplayDispatchCommitmentsV1<'_> {
    pub fn event_kind_v1(&self) -> crate::MtgoCompetitiveEventKindV1 {
        self.event_kind
    }

    pub fn game_number_v1(&self) -> u8 {
        self.game_number
    }

    pub fn event_identity_sha256_v1(&self) -> &str {
        self.event_identity_sha256
    }

    pub fn match_identity_sha256_v1(&self) -> &str {
        self.match_identity_sha256
    }

    pub fn deployment_commitment_sha256_v1(&self) -> &str {
        self.deployment_commitment_sha256
    }

    pub fn mode_authorization_commitment_sha256_v1(&self) -> &str {
        self.mode_authorization_commitment_sha256
    }

    pub fn gameplay_authorization_commitment_sha256_v1(&self) -> &str {
        self.gameplay_authorization_commitment_sha256
    }

    pub fn decision_commitment_sha256_v1(&self) -> &str {
        self.decision_commitment_sha256
    }

    pub fn selection_commitment_sha256_v1(&self) -> &str {
        self.selection_commitment_sha256
    }

    pub fn refresh_commitment_sha256_v1(&self) -> &str {
        self.refresh_commitment_sha256
    }

    pub fn exact_producer_result_sha256_v1(&self) -> &str {
        self.exact_producer_result_sha256
    }

    pub fn selected_index_v1(&self) -> usize {
        self.selected_index
    }

    pub fn source_frame_id_v1(&self) -> u64 {
        self.source_frame_id
    }

    pub fn source_frame_sequence_v1(&self) -> u64 {
        self.source_frame_sequence
    }

    pub fn source_captured_at_unix_millis_v1(&self) -> u128 {
        self.source_captured_at_unix_millis
    }

    pub fn source_frame_sha256_v1(&self) -> &str {
        self.source_frame_sha256
    }

    pub fn broker_binary_sha256_v1(&self) -> &str {
        self.broker_binary_sha256
    }

    pub fn producer_binary_sha256_v1(&self) -> &str {
        self.producer_binary_sha256
    }

    pub fn direct_competitive_scope_commitment_sha256_v1(&self) -> &str {
        self.direct_competitive_scope_commitment_sha256
    }

    pub fn before_dispatch_commitment_sha256_v1(&self) -> &str {
        self.before_dispatch_commitment_sha256
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

impl CheckedUntrustedMtgoDirectVisibleGameplayBeforeDispatchV1 {
    pub fn selected_action_v1(&self) -> &crate::MtgoPlayerVisibleDuelActionV1 {
        self.plan.selected_action_v1()
    }

    pub fn dispatch_commitments_v1(&self) -> MtgoDirectVisibleGameplayDispatchCommitmentsV1<'_> {
        MtgoDirectVisibleGameplayDispatchCommitmentsV1 {
            event_kind: self.plan.event_kind_v1(),
            game_number: self.plan.game_number_v1(),
            event_identity_sha256: self.plan.event_identity_sha256_v1(),
            match_identity_sha256: self.plan.match_identity_sha256_v1(),
            deployment_commitment_sha256: self.plan.deployment_commitment_sha256_v1(),
            mode_authorization_commitment_sha256: self
                .plan
                .mode_authorization_commitment_sha256_v1(),
            gameplay_authorization_commitment_sha256: self
                .plan
                .gameplay_authorization_commitment_sha256_v1(),
            decision_commitment_sha256: self.plan.decision_commitment_sha256_v1(),
            selection_commitment_sha256: self.plan.selection_commitment_sha256_v1(),
            refresh_commitment_sha256: self.plan.refresh_commitment_sha256_v1(),
            exact_producer_result_sha256: self.plan.exact_producer_result_sha256_v1(),
            selected_index: self.plan.selected_index_v1(),
            source_frame_id: self.plan.source_frame_id_v1(),
            source_frame_sequence: self.plan.source_frame_sequence_v1(),
            source_captured_at_unix_millis: self.plan.source_captured_at_unix_millis_v1(),
            source_frame_sha256: self.plan.source_frame_sha256_v1(),
            broker_binary_sha256: self.plan.broker_binary_sha256_v1(),
            producer_binary_sha256: self.plan.producer_binary_sha256_v1(),
            direct_competitive_scope_commitment_sha256: self
                .plan
                .direct_competitive_scope_commitment_sha256_v1(),
            before_dispatch_commitment_sha256: &self.before_dispatch_commitment_sha256,
        }
    }

    pub fn action_family_v1(&self) -> MtgoDuelActionFamilyV1 {
        self.action_family
    }

    pub fn before_dispatch_commitment_sha256_v1(&self) -> &str {
        &self.before_dispatch_commitment_sha256
    }

    #[doc(hidden)]
    pub fn after_frame_record_v1(
        &self,
        after_frame_id: u64,
        after_frame_sequence: u64,
        captured_at_unix_millis: u128,
        after_frame_sha256: String,
        after_region_sha256: Vec<String>,
    ) -> Result<MtgoDirectVisibleGameplayAfterDispatchFrameV1, MtgoContractErrorV1> {
        if after_region_sha256.len() != self.record.regions.len() {
            return Err(error_v1(
                "direct_visible_after_dispatch_region_count",
                "the trusted after-frame owner must rehash every fixed before region",
            ));
        }
        let regions = self
            .record
            .regions
            .iter()
            .zip(after_region_sha256)
            .map(
                |(before, after_bgra8_sha256)| MtgoDirectVisibleGameplayAfterRegionV1 {
                    kind: before.kind,
                    rect_client_px: before.rect_client_px.clone(),
                    after_bgra8_sha256,
                },
            )
            .collect();
        Ok(MtgoDirectVisibleGameplayAfterDispatchFrameV1 {
            schema_version: MTGO_DIRECT_VISIBLE_GAMEPLAY_AFTER_DISPATCH_SCHEMA_V1,
            after_frame_id,
            after_frame_sequence,
            captured_at_unix_millis,
            after_frame_sha256,
            client_size_px: self.record.client_size_px.clone(),
            regions,
        })
    }

    #[doc(hidden)]
    pub fn fixed_region_rects_v1(&self) -> Vec<MtgoRectPxV1> {
        self.record
            .regions
            .iter()
            .map(|region| region.rect_client_px.clone())
            .collect()
    }

    #[doc(hidden)]
    pub fn candidate_changes_every_fixed_region_v1(
        &self,
        after_region_sha256: &[String],
    ) -> Result<bool, MtgoContractErrorV1> {
        if after_region_sha256.len() != self.record.regions.len() {
            return Err(error_v1(
                "direct_visible_after_dispatch_region_count",
                "the trusted after-frame owner must rehash every fixed before region",
            ));
        }
        for digest in after_region_sha256 {
            require_sha256_v1(digest, "direct_visible_postcondition_after_region_hash")?;
        }
        Ok(self
            .record
            .regions
            .iter()
            .zip(after_region_sha256)
            .all(|(before, after)| before.before_bgra8_sha256 != *after))
    }

    #[doc(hidden)]
    pub fn source_captured_at_unix_millis_v1(&self) -> u128 {
        self.record.source_captured_at_unix_millis
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// One exact direct-source action whose required player-visible regions changed
/// on a newer composed frame after the sealed producer returned submitted.
/// This is transport-neutral history evidence. It records no mouse gesture and
/// exposes no client action, raw pixels, rectangle, Game Log text, or input
/// authority.
pub struct CheckedUntrustedMtgoDirectVisibleGameplayPostconditionV1 {
    _game_log_corroboration: Option<CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1>,
    event_kind: crate::MtgoCompetitiveEventKindV1,
    event_identity_sha256: String,
    match_identity_sha256: String,
    game_number: u8,
    deployment_commitment_sha256: String,
    decision_commitment_sha256: String,
    selection_commitment_sha256: String,
    source_frame_id: u64,
    source_frame_sequence: u64,
    source_frame_sha256: String,
    after_frame_id: u64,
    after_frame_sequence: u64,
    player_visible_decision: MtgoPlayerVisibleConfirmedDuelDecisionV1,
    action_family: MtgoDuelActionFamilyV1,
    dispatch_receipt_commitment_sha256: String,
    confirmation_commitment_sha256: String,
}

impl CheckedUntrustedMtgoDirectVisibleGameplayPostconditionV1 {
    pub fn event_kind_v1(&self) -> crate::MtgoCompetitiveEventKindV1 {
        self.event_kind
    }

    pub fn game_number_v1(&self) -> u8 {
        self.game_number
    }

    pub fn player_visible_decision_v1(&self) -> &MtgoPlayerVisibleConfirmedDuelDecisionV1 {
        &self.player_visible_decision
    }

    pub fn action_family_v1(&self) -> MtgoDuelActionFamilyV1 {
        self.action_family
    }

    pub fn dispatch_receipt_commitment_sha256_v1(&self) -> &str {
        &self.dispatch_receipt_commitment_sha256
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
        &self.event_identity_sha256
    }

    pub(crate) fn match_identity_sha256_v1(&self) -> &str {
        &self.match_identity_sha256
    }

    pub(crate) fn deployment_commitment_sha256_v1(&self) -> &str {
        &self.deployment_commitment_sha256
    }

    pub(crate) fn decision_commitment_sha256_v1(&self) -> &str {
        &self.decision_commitment_sha256
    }

    pub(crate) fn selection_commitment_sha256_v1(&self) -> &str {
        &self.selection_commitment_sha256
    }

    pub(crate) fn source_frame_id_v1(&self) -> u64 {
        self.source_frame_id
    }

    pub(crate) fn source_frame_sequence_v1(&self) -> u64 {
        self.source_frame_sequence
    }

    pub(crate) fn source_frame_sha256_v1(&self) -> &str {
        &self.source_frame_sha256
    }

    #[doc(hidden)]
    pub fn after_frame_id_v1(&self) -> u64 {
        self.after_frame_id
    }

    #[doc(hidden)]
    pub fn after_frame_sequence_v1(&self) -> u64 {
        self.after_frame_sequence
    }
}

pub fn prepare_direct_visible_gameplay_before_dispatch_v1(
    plan: CheckedUntrustedMtgoDirectVisibleCompetitiveActionPlanV1,
    record: MtgoDirectVisibleGameplayBeforeDispatchRecordV1,
) -> Result<CheckedUntrustedMtgoDirectVisibleGameplayBeforeDispatchV1, MtgoContractErrorV1> {
    if record.schema_version != MTGO_DIRECT_VISIBLE_GAMEPLAY_BEFORE_DISPATCH_SCHEMA_V1
        || record.direct_competitive_scope_commitment_sha256
            != plan.direct_competitive_scope_commitment_sha256_v1()
        || record.source_frame_id != plan.source_frame_id_v1()
        || record.source_frame_sequence != plan.source_frame_sequence_v1()
        || record.source_captured_at_unix_millis != plan.source_captured_at_unix_millis_v1()
        || record.source_frame_sha256 != plan.source_frame_sha256_v1()
        || &record.client_size_px != plan.client_size_px_v1()
    {
        return Err(error_v1(
            "direct_visible_before_dispatch_source",
            "the before-dispatch plan must retain the exact match-bound direct source frame",
        ));
    }
    require_sha256_v1(
        &record.direct_competitive_scope_commitment_sha256,
        "direct_visible_before_dispatch_scope_hash",
    )?;
    let action_family = player_visible_duel_action_family_v1(plan.selected_action_v1());
    let kinds = validate_before_regions_v1(&record)?;
    require_action_specific_transition_v1(action_family, &kinds)?;
    validate_game_log_baseline_v1(
        &plan,
        record
            .expected_game_log_baseline_commitment_sha256
            .as_deref(),
    )?;
    let record_json = serde_json::to_vec(&record).map_err(|error| {
        error_v1(
            "direct_visible_before_dispatch_serialization",
            error.to_string(),
        )
    })?;
    let before_dispatch_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_BEFORE_DISPATCH_DOMAIN_V1,
        &[
            &record_json,
            plan.direct_competitive_scope_commitment_sha256_v1()
                .as_bytes(),
            b"visible_regions_fixed_before_private_client_action_no_authority",
        ],
    );
    Ok(CheckedUntrustedMtgoDirectVisibleGameplayBeforeDispatchV1 {
        plan,
        record,
        action_family,
        before_dispatch_commitment_sha256,
    })
}

/// Completes the structural half of the direct visible postcondition. The
/// Windows producer must supply the opaque live capture and sealed dispatch
/// proof before calling this function. The result remains checked-untrusted
/// and grants no authority for another input.
#[doc(hidden)]
pub fn complete_direct_visible_gameplay_postcondition_v1(
    before: CheckedUntrustedMtgoDirectVisibleGameplayBeforeDispatchV1,
    dispatch_receipt_commitment_sha256: &str,
    dispatch_submitted_at_unix_millis: u128,
    after: MtgoDirectVisibleGameplayAfterDispatchFrameV1,
    game_log_corroboration: Option<CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1>,
) -> Result<CheckedUntrustedMtgoDirectVisibleGameplayPostconditionV1, MtgoContractErrorV1> {
    require_sha256_v1(
        dispatch_receipt_commitment_sha256,
        "direct_visible_dispatch_receipt_hash",
    )?;
    if dispatch_submitted_at_unix_millis < before.record.source_captured_at_unix_millis
        || after.schema_version != MTGO_DIRECT_VISIBLE_GAMEPLAY_AFTER_DISPATCH_SCHEMA_V1
        || after.after_frame_id == 0
        || after.after_frame_id == before.record.source_frame_id
        || after.after_frame_sequence <= before.record.source_frame_sequence
        || after.captured_at_unix_millis <= dispatch_submitted_at_unix_millis
        || after.after_frame_sha256 == before.record.source_frame_sha256
        || after.client_size_px != before.record.client_size_px
        || after.regions.len() != before.record.regions.len()
    {
        return Err(error_v1(
            "direct_visible_postcondition_source",
            "the after-dispatch frame must be distinct, strictly newer, and retain client geometry",
        ));
    }
    require_sha256_v1(
        &after.after_frame_sha256,
        "direct_visible_postcondition_after_frame_hash",
    )?;
    let mut kinds = HashSet::new();
    let mut transition_parts = Vec::with_capacity(after.regions.len());
    for (before_region, after_region) in before.record.regions.iter().zip(&after.regions) {
        require_sha256_v1(
            &after_region.after_bgra8_sha256,
            "direct_visible_postcondition_after_region_hash",
        )?;
        if before_region.kind != after_region.kind
            || before_region.rect_client_px != after_region.rect_client_px
            || before_region.before_bgra8_sha256 == after_region.after_bgra8_sha256
            || !kinds.insert(before_region.kind)
        {
            return Err(error_v1(
                "direct_visible_postcondition_region",
                "every exact pre-dispatch region must visibly change on the newer frame",
            ));
        }
        let before_json = serde_json::to_vec(before_region).map_err(|error| {
            error_v1(
                "direct_visible_postcondition_serialization",
                error.to_string(),
            )
        })?;
        let after_json = serde_json::to_vec(after_region).map_err(|error| {
            error_v1(
                "direct_visible_postcondition_serialization",
                error.to_string(),
            )
        })?;
        transition_parts.push(commitment_v1(
            b"mtgo-direct-visible-gameplay-region-transition-v1",
            &[&before_json, &after_json],
        ));
    }
    require_action_specific_transition_v1(before.action_family, &kinds)?;
    let game_log_commitment = validate_game_log_corroboration_v1(
        &before.plan,
        before
            .record
            .expected_game_log_baseline_commitment_sha256
            .as_deref(),
        game_log_corroboration.as_ref(),
    )?;
    let after_json = serde_json::to_vec(&after).map_err(|error| {
        error_v1(
            "direct_visible_postcondition_serialization",
            error.to_string(),
        )
    })?;
    let transition_bytes = serde_json::to_vec(&transition_parts).map_err(|error| {
        error_v1(
            "direct_visible_postcondition_serialization",
            error.to_string(),
        )
    })?;
    let confirmation_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_POSTCONDITION_DOMAIN_V1,
        &[
            before.before_dispatch_commitment_sha256.as_bytes(),
            dispatch_receipt_commitment_sha256.as_bytes(),
            &dispatch_submitted_at_unix_millis.to_be_bytes(),
            &after_json,
            &transition_bytes,
            game_log_commitment.as_bytes(),
            b"sealed_client_action_confirmed_only_by_newer_player_visible_result",
        ],
    );
    Ok(CheckedUntrustedMtgoDirectVisibleGameplayPostconditionV1 {
        event_kind: before.plan.event_kind_v1(),
        event_identity_sha256: before.plan.event_identity_sha256_v1().to_owned(),
        match_identity_sha256: before.plan.match_identity_sha256_v1().to_owned(),
        game_number: before.plan.game_number_v1(),
        deployment_commitment_sha256: before.plan.deployment_commitment_sha256_v1().to_owned(),
        decision_commitment_sha256: before.plan.decision_commitment_sha256_v1().to_owned(),
        selection_commitment_sha256: before.plan.selection_commitment_sha256_v1().to_owned(),
        source_frame_id: before.plan.source_frame_id_v1(),
        source_frame_sequence: before.plan.source_frame_sequence_v1(),
        source_frame_sha256: before.plan.source_frame_sha256_v1().to_owned(),
        after_frame_id: after.after_frame_id,
        after_frame_sequence: after.after_frame_sequence,
        player_visible_decision: before.plan.player_visible_confirmed_decision_v1().clone(),
        action_family: before.action_family,
        dispatch_receipt_commitment_sha256: dispatch_receipt_commitment_sha256.to_owned(),
        confirmation_commitment_sha256,
        _game_log_corroboration: game_log_corroboration,
    })
}

fn validate_before_regions_v1(
    record: &MtgoDirectVisibleGameplayBeforeDispatchRecordV1,
) -> Result<HashSet<MtgoPlayerVisibleGameplayPostconditionKindV1>, MtgoContractErrorV1> {
    if !record.region_set_complete
        || record.regions.is_empty()
        || record.regions.len() > MAX_TRANSITIONS_V1
    {
        return Err(error_v1(
            "direct_visible_before_dispatch_region_set",
            record.regions.len().to_string(),
        ));
    }
    let mut kinds = HashSet::new();
    let mut rects = Vec::with_capacity(record.regions.len());
    for region in &record.regions {
        require_sha256_v1(
            &region.before_bgra8_sha256,
            "direct_visible_before_dispatch_region_hash",
        )?;
        let rect = &region.rect_client_px;
        let right = rect
            .x
            .checked_add(rect.width)
            .ok_or_else(|| error_v1("direct_visible_before_dispatch_region", "right overflow"))?;
        let bottom = rect
            .y
            .checked_add(rect.height)
            .ok_or_else(|| error_v1("direct_visible_before_dispatch_region", "bottom overflow"))?;
        let key = (rect.x, rect.y, rect.width, rect.height);
        if rect.width == 0
            || rect.height == 0
            || right > record.client_size_px.width
            || bottom > record.client_size_px.height
            || !kinds.insert(region.kind)
            || rects
                .iter()
                .any(|existing| rects_intersect_v1(*existing, key))
        {
            return Err(error_v1(
                "direct_visible_before_dispatch_region",
                "regions must be unique, nonoverlapping, nonempty, and inside the client",
            ));
        }
        rects.push(key);
    }
    Ok(kinds)
}

fn require_action_specific_transition_v1(
    family: MtgoDuelActionFamilyV1,
    kinds: &HashSet<MtgoPlayerVisibleGameplayPostconditionKindV1>,
) -> Result<(), MtgoContractErrorV1> {
    use MtgoPlayerVisibleGameplayPostconditionKindV1 as K;
    let any = |allowed: &[K]| allowed.iter().any(|kind| kinds.contains(kind));
    let valid = match family {
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
    if valid {
        Ok(())
    } else {
        Err(error_v1(
            "direct_visible_postcondition_action_transition",
            format!("the final {family:?} action lacks its required player-visible transition"),
        ))
    }
}

fn validate_game_log_baseline_v1(
    plan: &CheckedUntrustedMtgoDirectVisibleCompetitiveActionPlanV1,
    baseline: Option<&str>,
) -> Result<(), MtgoContractErrorV1> {
    if let Some(baseline) = baseline {
        require_sha256_v1(baseline, "direct_visible_game_log_baseline_hash")?;
        if expected_game_log_kind_v1(plan.selected_action_v1()).is_none() {
            return Err(error_v1(
                "direct_visible_game_log_baseline_unsupported",
                "a Game Log baseline is valid only for a supported visible action",
            ));
        }
    }
    Ok(())
}

fn validate_game_log_corroboration_v1(
    plan: &CheckedUntrustedMtgoDirectVisibleCompetitiveActionPlanV1,
    baseline: Option<&str>,
    corroboration: Option<&CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1>,
) -> Result<String, MtgoContractErrorV1> {
    match (baseline, corroboration) {
        (None, None) => Ok("none".to_owned()),
        (Some(expected), Some(corroboration)) => {
            if corroboration.baseline_commitment_sha256_v1() != expected
                || corroboration.kind_v1()
                    != expected_game_log_kind_v1(plan.selected_action_v1())
                        .expect("baseline already validated for a supported action")
                || corroboration.player_visible_decision_commitment_sha256_v1()
                    != player_visible_game_log_action_decision_commitment_v1(
                        plan.player_visible_confirmed_decision_v1(),
                    )?
            {
                return Err(error_v1(
                    "direct_visible_game_log_corroboration",
                    "Game Log corroboration must bind the exact baseline and selected visible action",
                ));
            }
            Ok(corroboration
                .corroboration_commitment_sha256_v1()
                .to_owned())
        }
        _ => Err(error_v1(
            "direct_visible_game_log_pairing",
            "a declared Game Log baseline and corroboration must appear together",
        )),
    }
}

fn expected_game_log_kind_v1(
    action: &crate::MtgoPlayerVisibleDuelActionV1,
) -> Option<MtgoVisibleGameLogActionCorroborationKindV1> {
    match action {
        crate::MtgoPlayerVisibleDuelActionV1::PlayLand { .. } => {
            Some(MtgoVisibleGameLogActionCorroborationKindV1::PlayedCard)
        }
        crate::MtgoPlayerVisibleDuelActionV1::CastSpell { .. } => {
            Some(MtgoVisibleGameLogActionCorroborationKindV1::CastSpell)
        }
        crate::MtgoPlayerVisibleDuelActionV1::Discard { .. } => {
            Some(MtgoVisibleGameLogActionCorroborationKindV1::DiscardedCards)
        }
        crate::MtgoPlayerVisibleDuelActionV1::DeclareAttackers { .. } => {
            Some(MtgoVisibleGameLogActionCorroborationKindV1::DeclaredAttackers)
        }
        _ => None,
    }
}

fn rects_intersect_v1(left: (u32, u32, u32, u32), right: (u32, u32, u32, u32)) -> bool {
    let left_right = u64::from(left.0) + u64::from(left.2);
    let left_bottom = u64::from(left.1) + u64::from(left.3);
    let right_right = u64::from(right.0) + u64::from(right.2);
    let right_bottom = u64::from(right.1) + u64::from(right.3);
    u64::from(left.0) < right_right
        && u64::from(right.0) < left_right
        && u64::from(left.1) < right_bottom
        && u64::from(right.1) < left_bottom
}

fn require_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(code, "expected lowercase SHA-256"));
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
        begin_checked_untrusted_competitive_player_visible_game_history_from_direct_visible_postcondition_v1,
        direct_visible_competitive_gameplay::tests::competitive_plan_v1,
        MtgoCompetitiveEventKindV1,
    };

    fn digest_v1(character: char) -> String {
        character.to_string().repeat(64)
    }

    fn before_regions_v1() -> Vec<MtgoDirectVisibleGameplayBeforeRegionV1> {
        vec![
            MtgoDirectVisibleGameplayBeforeRegionV1 {
                kind: MtgoPlayerVisibleGameplayPostconditionKindV1::BattlefieldChanged,
                rect_client_px: MtgoRectPxV1 {
                    x: 10,
                    y: 10,
                    width: 100,
                    height: 100,
                },
                before_bgra8_sha256: digest_v1('c'),
            },
            MtgoDirectVisibleGameplayBeforeRegionV1 {
                kind: MtgoPlayerVisibleGameplayPostconditionKindV1::HandChanged,
                rect_client_px: MtgoRectPxV1 {
                    x: 10,
                    y: 120,
                    width: 100,
                    height: 100,
                },
                before_bgra8_sha256: digest_v1('d'),
            },
        ]
    }

    fn before_record_v1(
        plan: &CheckedUntrustedMtgoDirectVisibleCompetitiveActionPlanV1,
    ) -> MtgoDirectVisibleGameplayBeforeDispatchRecordV1 {
        MtgoDirectVisibleGameplayBeforeDispatchRecordV1 {
            schema_version: MTGO_DIRECT_VISIBLE_GAMEPLAY_BEFORE_DISPATCH_SCHEMA_V1,
            direct_competitive_scope_commitment_sha256: plan
                .direct_competitive_scope_commitment_sha256_v1()
                .to_owned(),
            source_frame_id: plan.source_frame_id_v1(),
            source_frame_sequence: plan.source_frame_sequence_v1(),
            source_captured_at_unix_millis: plan.source_captured_at_unix_millis_v1(),
            source_frame_sha256: plan.source_frame_sha256_v1().to_owned(),
            client_size_px: plan.client_size_px_v1().clone(),
            region_set_complete: true,
            regions: before_regions_v1(),
            expected_game_log_baseline_commitment_sha256: None,
        }
    }

    fn prepared_v1() -> CheckedUntrustedMtgoDirectVisibleGameplayBeforeDispatchV1 {
        let plan = competitive_plan_v1(MtgoCompetitiveEventKindV1::League);
        let record = before_record_v1(&plan);
        prepare_direct_visible_gameplay_before_dispatch_v1(plan, record).unwrap()
    }

    fn after_frame_v1() -> MtgoDirectVisibleGameplayAfterDispatchFrameV1 {
        MtgoDirectVisibleGameplayAfterDispatchFrameV1 {
            schema_version: MTGO_DIRECT_VISIBLE_GAMEPLAY_AFTER_DISPATCH_SCHEMA_V1,
            after_frame_id: 103,
            after_frame_sequence: 203,
            captured_at_unix_millis: 1_003,
            after_frame_sha256: digest_v1('e'),
            client_size_px: MtgoSizePxV1 {
                width: 1_240,
                height: 740,
            },
            regions: vec![
                MtgoDirectVisibleGameplayAfterRegionV1 {
                    kind: MtgoPlayerVisibleGameplayPostconditionKindV1::BattlefieldChanged,
                    rect_client_px: MtgoRectPxV1 {
                        x: 10,
                        y: 10,
                        width: 100,
                        height: 100,
                    },
                    after_bgra8_sha256: digest_v1('a'),
                },
                MtgoDirectVisibleGameplayAfterRegionV1 {
                    kind: MtgoPlayerVisibleGameplayPostconditionKindV1::HandChanged,
                    rect_client_px: MtgoRectPxV1 {
                        x: 10,
                        y: 120,
                        width: 100,
                        height: 100,
                    },
                    after_bgra8_sha256: digest_v1('b'),
                },
            ],
        }
    }

    #[test]
    fn play_land_fixes_complete_action_specific_visible_regions_before_dispatch() {
        let prepared = prepared_v1();
        assert_eq!(
            prepared.action_family_v1(),
            MtgoDuelActionFamilyV1::PlayLand
        );
        assert_eq!(prepared.before_dispatch_commitment_sha256_v1().len(), 64);
        let dispatch = prepared.dispatch_commitments_v1();
        assert_eq!(dispatch.event_kind_v1(), MtgoCompetitiveEventKindV1::League);
        assert_eq!(dispatch.game_number_v1(), 1);
        assert_eq!(dispatch.selected_index_v1(), 1);
        assert_eq!(dispatch.source_frame_id_v1(), 102);
        assert_eq!(dispatch.source_frame_sequence_v1(), 202);
        assert_eq!(dispatch.source_captured_at_unix_millis_v1(), 1_001);
        for digest in [
            dispatch.event_identity_sha256_v1(),
            dispatch.match_identity_sha256_v1(),
            dispatch.deployment_commitment_sha256_v1(),
            dispatch.mode_authorization_commitment_sha256_v1(),
            dispatch.gameplay_authorization_commitment_sha256_v1(),
            dispatch.decision_commitment_sha256_v1(),
            dispatch.selection_commitment_sha256_v1(),
            dispatch.refresh_commitment_sha256_v1(),
            dispatch.exact_producer_result_sha256_v1(),
            dispatch.source_frame_sha256_v1(),
            dispatch.broker_binary_sha256_v1(),
            dispatch.producer_binary_sha256_v1(),
            dispatch.direct_competitive_scope_commitment_sha256_v1(),
            dispatch.before_dispatch_commitment_sha256_v1(),
        ] {
            assert_eq!(digest.len(), 64);
        }
        assert!(!dispatch.safe_for_live_input_v1());
        assert!(!prepared.safe_for_live_input_v1());
        assert!(!prepared.permits_event_entry_v1());
        assert!(!prepared.permits_spending_v1());
    }

    #[test]
    fn missing_required_region_and_crossed_source_reject_before_dispatch() {
        let plan = competitive_plan_v1(MtgoCompetitiveEventKindV1::League);
        let mut missing = before_record_v1(&plan);
        missing.regions.pop();
        assert_eq!(
            prepare_direct_visible_gameplay_before_dispatch_v1(plan, missing)
                .err()
                .expect("missing hand transition rejects")
                .code(),
            "direct_visible_postcondition_action_transition"
        );

        let plan = competitive_plan_v1(MtgoCompetitiveEventKindV1::League);
        let mut crossed = before_record_v1(&plan);
        crossed.source_frame_sequence += 1;
        assert_eq!(
            prepare_direct_visible_gameplay_before_dispatch_v1(plan, crossed)
                .err()
                .expect("crossed source rejects")
                .code(),
            "direct_visible_before_dispatch_source"
        );
    }

    #[test]
    fn trusted_after_frame_polling_requires_every_fixed_region_to_change() {
        let prepared = prepared_v1();
        assert!(prepared
            .candidate_changes_every_fixed_region_v1(&[digest_v1('a'), digest_v1('b')])
            .unwrap());
        assert!(!prepared
            .candidate_changes_every_fixed_region_v1(&[digest_v1('c'), digest_v1('b')])
            .unwrap());
        assert!(prepared
            .candidate_changes_every_fixed_region_v1(&[digest_v1('a')])
            .is_err());
        assert!(prepared
            .candidate_changes_every_fixed_region_v1(&[
                "not-a-digest".to_owned(),
                digest_v1('b'),
            ])
            .is_err());
    }

    #[test]
    fn exact_newer_visible_result_extends_transport_neutral_game_history() {
        let confirmed = complete_direct_visible_gameplay_postcondition_v1(
            prepared_v1(),
            &digest_v1('f'),
            1_002,
            after_frame_v1(),
            None,
        )
        .unwrap();
        assert_eq!(
            confirmed.action_family_v1(),
            MtgoDuelActionFamilyV1::PlayLand
        );
        assert!(!confirmed.has_exact_visible_game_log_corroboration_v1());
        assert!(!confirmed.safe_for_additional_input_v1());
        let expected = confirmed.player_visible_decision_v1().clone();
        let history =
            begin_checked_untrusted_competitive_player_visible_game_history_from_direct_visible_postcondition_v1(
                "direct-visible-history-v1",
                confirmed,
            )
            .unwrap();
        assert_eq!(history.decision_count_v1(), 1);
        assert_eq!(
            history.decision_v1(0).unwrap().player_visible_decision_v1(),
            &expected
        );
        assert!(!history.safe_for_model_scoring_v1());
        assert!(!history.safe_for_input_v1());
    }

    #[test]
    fn unchanged_moved_or_not_newer_visible_results_reject() {
        let mut unchanged = after_frame_v1();
        unchanged.regions[0].after_bgra8_sha256 = digest_v1('c');
        assert_eq!(
            complete_direct_visible_gameplay_postcondition_v1(
                prepared_v1(),
                &digest_v1('f'),
                1_002,
                unchanged,
                None,
            )
            .err()
            .expect("unchanged region rejects")
            .code(),
            "direct_visible_postcondition_region"
        );

        let mut moved = after_frame_v1();
        moved.regions[1].rect_client_px.x += 1;
        assert_eq!(
            complete_direct_visible_gameplay_postcondition_v1(
                prepared_v1(),
                &digest_v1('f'),
                1_002,
                moved,
                None,
            )
            .err()
            .expect("moved region rejects")
            .code(),
            "direct_visible_postcondition_region"
        );

        let mut early = after_frame_v1();
        early.captured_at_unix_millis = 1_002;
        assert_eq!(
            complete_direct_visible_gameplay_postcondition_v1(
                prepared_v1(),
                &digest_v1('f'),
                1_002,
                early,
                None,
            )
            .err()
            .expect("non-newer frame rejects")
            .code(),
            "direct_visible_postcondition_source"
        );
    }
}
