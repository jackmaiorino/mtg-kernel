use crate::{
    resolve_checked_untrusted_kernel_card_correspondence_v1,
    CheckedUntrustedMtgoOfflinePlayLandHandReflowV1,
    CheckedUntrustedMtgoVisibleObjectActionCalibrationV1, MtgoContractErrorV1,
    MtgoOfflinePlayLandHandReflowClassificationV1, MtgoVisibleObjectCalibrationActionV1,
};
use mtg_kernel::rl::{CardStableRefV1, PlayerSeatV1};
use mtg_kernel::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_VISIBLE_OBJECT_INCARNATION_SCHEMA_V1: u32 = 1;

const OBJECT_LEDGER_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-visible-object-ledger-v1";
const SOURCE_BOUND_OBJECT_LEDGER_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-visible-object-ledger-source-bound-v1";
const MAX_VISIBLE_OBJECTS_V1: usize = 1_024;
const MAX_OBJECT_TRANSITIONS_V1: usize = 1_024;
const MAX_VISIBLE_OBJECT_ID_BYTES_V1: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleObjectSeedV1 {
    pub display_ordinal: u32,
    pub visible_object_id: String,
    pub visible_card_name: String,
    pub owner: PlayerSeatV1,
    pub controller: PlayerSeatV1,
    pub zone: Zone,
}

/// One visible hand slot after a checked PlayLand transition.
///
/// The declaration is bookkeeping input only. Its card label must match the
/// pre-action order after removing the played card, and it grants no semantic,
/// observation, scoring, or input authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleHandReflowSlotV1 {
    pub display_ordinal: u32,
    pub visible_object_id: String,
    pub visible_card_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum MtgoObjectLedgerTransitionKindV1 {
    PlayLandZoneChange,
    ActivateManaAbilityNoZoneChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MtgoObjectLedgerTransitionV1 {
    sequence: u64,
    kind: MtgoObjectLedgerTransitionKindV1,
    calibration_commitment_sha256: String,
    before_frame_sha256: String,
    after_frame_sha256: String,
    source_visible_object_id: String,
    result_visible_object_id: String,
    arena_id: u32,
    before_zone_change_count: u32,
    after_zone_change_count: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    remaining_hand_reflow: Vec<MtgoObjectLedgerHandReflowV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MtgoObjectLedgerHandReflowV1 {
    source_visible_object_id: String,
    result_visible_object_id: String,
    visible_card_name: String,
    arena_id: u32,
}

struct MtgoTrackedVisibleObjectV1 {
    visible_object_id: String,
    visible_card_name: String,
    stable: CardStableRefV1,
}

/// Checked-untrusted visible-object lineage and incarnation state.
///
/// Synthetic arena identifiers are allocated deterministically from the
/// caller's canonical display ordinals. A checked visible PlayLand transition
/// preserves that arena identifier, changes Hand to Battlefield, and increments
/// the zone-change count exactly once. A checked mana activation preserves the
/// complete battlefield reference.
///
/// The seed labels and calibration traces are not trusted semantic evidence.
/// This wrapper therefore exposes no stable references or object bindings
/// outside this crate and grants no observation, scoring, or input authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoVisibleObjectLedgerV1;
/// fn cannot_extract_bindings(
///     ledger: &CheckedUntrustedMtgoVisibleObjectLedgerV1,
/// ) {
///     let _ = ledger.current_object_bindings_v1();
/// }
/// ```
pub struct CheckedUntrustedMtgoVisibleObjectLedgerV1 {
    current_frame_sha256: String,
    seed_source_commitment_sha256: Option<String>,
    objects: Vec<MtgoTrackedVisibleObjectV1>,
    transitions: Vec<MtgoObjectLedgerTransitionV1>,
    ledger_commitment_sha256: String,
}

impl CheckedUntrustedMtgoVisibleObjectLedgerV1 {
    pub fn current_frame_sha256(&self) -> &str {
        &self.current_frame_sha256
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub fn seed_source_commitment_sha256(&self) -> Option<&str> {
        self.seed_source_commitment_sha256.as_deref()
    }

    pub fn transition_count(&self) -> usize {
        self.transitions.len()
    }

    pub fn ledger_commitment_sha256(&self) -> &str {
        &self.ledger_commitment_sha256
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

    #[cfg(test)]
    pub(crate) fn current_object_bindings_v1(&self) -> Vec<crate::MtgoObjectBindingV1> {
        self.objects
            .iter()
            .map(|object| crate::MtgoObjectBindingV1 {
                adapter_object_id: object.visible_object_id.clone(),
                kernel_ref: object.stable.clone(),
            })
            .collect()
    }
}

pub fn start_checked_untrusted_visible_object_ledger_v1(
    current_frame_sha256: &str,
    seeds: Vec<MtgoVisibleObjectSeedV1>,
) -> Result<CheckedUntrustedMtgoVisibleObjectLedgerV1, MtgoContractErrorV1> {
    start_checked_untrusted_visible_object_ledger_with_source_v1(current_frame_sha256, None, seeds)
}

pub(crate) fn start_checked_untrusted_visible_object_ledger_with_source_v1(
    current_frame_sha256: &str,
    seed_source_commitment_sha256: Option<&str>,
    seeds: Vec<MtgoVisibleObjectSeedV1>,
) -> Result<CheckedUntrustedMtgoVisibleObjectLedgerV1, MtgoContractErrorV1> {
    require_sha256_v1(current_frame_sha256, "object_ledger_frame_hash_invalid")?;
    if let Some(commitment) = seed_source_commitment_sha256 {
        require_sha256_v1(commitment, "object_ledger_seed_source_hash_invalid")?;
    }
    if seeds.is_empty() || seeds.len() > MAX_VISIBLE_OBJECTS_V1 {
        return Err(MtgoContractErrorV1::new(
            "object_ledger_seed_count_invalid",
            seeds.len().to_string(),
        ));
    }

    let mut visible_ids = HashSet::with_capacity(seeds.len());
    let mut objects = Vec::with_capacity(seeds.len());
    for (index, seed) in seeds.into_iter().enumerate() {
        let expected_ordinal = u32::try_from(index).map_err(|_| {
            MtgoContractErrorV1::new("object_ledger_display_ordinal_overflow", index.to_string())
        })?;
        if seed.display_ordinal != expected_ordinal {
            return Err(MtgoContractErrorV1::new(
                "object_ledger_display_order_invalid",
                format!(
                    "expected={expected_ordinal},actual={}",
                    seed.display_ordinal
                ),
            ));
        }
        validate_visible_object_id_v1(&seed.visible_object_id)?;
        if !visible_ids.insert(seed.visible_object_id.clone()) {
            return Err(MtgoContractErrorV1::new(
                "object_ledger_duplicate_visible_object_id",
                seed.visible_object_id,
            ));
        }
        let correspondence =
            resolve_checked_untrusted_kernel_card_correspondence_v1(&seed.visible_card_name)?;
        let arena_id = expected_ordinal.checked_add(1).ok_or_else(|| {
            MtgoContractErrorV1::new(
                "object_ledger_arena_id_overflow",
                expected_ordinal.to_string(),
            )
        })?;
        objects.push(MtgoTrackedVisibleObjectV1 {
            visible_object_id: seed.visible_object_id,
            visible_card_name: seed.visible_card_name,
            stable: CardStableRefV1 {
                arena_id,
                card_db_id: correspondence.card_db_id(),
                owner: seed.owner,
                controller: seed.controller,
                zone: seed.zone,
                zone_change_count: 0,
            },
        });
    }

    let mut ledger = CheckedUntrustedMtgoVisibleObjectLedgerV1 {
        current_frame_sha256: current_frame_sha256.to_owned(),
        seed_source_commitment_sha256: seed_source_commitment_sha256.map(str::to_owned),
        objects,
        transitions: Vec::new(),
        ledger_commitment_sha256: String::new(),
    };
    ledger.ledger_commitment_sha256 = commit_ledger_v1(&ledger)?;
    Ok(ledger)
}

pub fn advance_checked_untrusted_visible_object_calibration_v1(
    mut ledger: CheckedUntrustedMtgoVisibleObjectLedgerV1,
    calibration: &CheckedUntrustedMtgoVisibleObjectActionCalibrationV1,
    result_visible_object_id: Option<&str>,
) -> Result<CheckedUntrustedMtgoVisibleObjectLedgerV1, MtgoContractErrorV1> {
    if ledger.current_frame_sha256 != calibration.before_frame_sha256() {
        return Err(MtgoContractErrorV1::new(
            "object_ledger_transition_frame_mismatch",
            "the calibration does not begin at the ledger's current frame",
        ));
    }
    require_sha256_v1(
        calibration.after_frame_sha256(),
        "object_ledger_after_frame_hash_invalid",
    )?;
    if ledger.transitions.len() >= MAX_OBJECT_TRANSITIONS_V1 {
        return Err(MtgoContractErrorV1::new(
            "object_ledger_transition_count_invalid",
            ledger.transitions.len().to_string(),
        ));
    }

    let (kind, source_visible_object_id, visible_card_name, actor) = match calibration.action() {
        MtgoVisibleObjectCalibrationActionV1::PlayLand {
            actor,
            source_adapter_object_id,
            visible_card_name,
        } => (
            MtgoObjectLedgerTransitionKindV1::PlayLandZoneChange,
            source_adapter_object_id.as_str(),
            visible_card_name.as_str(),
            *actor,
        ),
        MtgoVisibleObjectCalibrationActionV1::ActivateManaAbility {
            actor,
            source_adapter_object_id,
            visible_card_name,
            ..
        } => (
            MtgoObjectLedgerTransitionKindV1::ActivateManaAbilityNoZoneChange,
            source_adapter_object_id.as_str(),
            visible_card_name.as_str(),
            *actor,
        ),
    };
    let object_index = ledger
        .objects
        .iter()
        .position(|object| object.visible_object_id == source_visible_object_id)
        .ok_or_else(|| {
            MtgoContractErrorV1::new(
                "object_ledger_transition_source_missing",
                source_visible_object_id,
            )
        })?;
    let source = &ledger.objects[object_index];
    if source.visible_card_name != visible_card_name
        || source.stable.owner != actor
        || source.stable.controller != actor
    {
        return Err(MtgoContractErrorV1::new(
            "object_ledger_transition_source_mismatch",
            source_visible_object_id,
        ));
    }

    let before_zone = source.stable.zone;
    let before_zone_change_count = source.stable.zone_change_count;
    let (result_visible_object_id, after_zone_change_count) = match kind {
        MtgoObjectLedgerTransitionKindV1::PlayLandZoneChange => {
            if before_zone != Zone::Hand {
                return Err(MtgoContractErrorV1::new(
                    "object_ledger_play_land_source_zone_invalid",
                    format!("{before_zone:?}"),
                ));
            }
            let result_visible_object_id = result_visible_object_id.ok_or_else(|| {
                MtgoContractErrorV1::new(
                    "object_ledger_play_land_result_missing",
                    source_visible_object_id,
                )
            })?;
            validate_visible_object_id_v1(result_visible_object_id)?;
            if result_visible_object_id == source_visible_object_id
                || ledger.objects.iter().enumerate().any(|(index, candidate)| {
                    index != object_index && candidate.visible_object_id == result_visible_object_id
                })
            {
                return Err(MtgoContractErrorV1::new(
                    "object_ledger_play_land_result_id_invalid",
                    result_visible_object_id,
                ));
            }
            let after_zone_change_count =
                before_zone_change_count.checked_add(1).ok_or_else(|| {
                    MtgoContractErrorV1::new(
                        "object_ledger_zone_change_count_overflow",
                        source_visible_object_id,
                    )
                })?;
            let object = &mut ledger.objects[object_index];
            object.visible_object_id = result_visible_object_id.to_owned();
            object.stable.zone = Zone::Battlefield;
            object.stable.zone_change_count = after_zone_change_count;
            (result_visible_object_id.to_owned(), after_zone_change_count)
        }
        MtgoObjectLedgerTransitionKindV1::ActivateManaAbilityNoZoneChange => {
            if before_zone != Zone::Battlefield || result_visible_object_id.is_some() {
                return Err(MtgoContractErrorV1::new(
                    "object_ledger_mana_activation_shape_invalid",
                    source_visible_object_id,
                ));
            }
            (
                source_visible_object_id.to_owned(),
                before_zone_change_count,
            )
        }
    };

    let sequence = u64::try_from(ledger.transitions.len() + 1).map_err(|_| {
        MtgoContractErrorV1::new(
            "object_ledger_transition_sequence_overflow",
            ledger.transitions.len().to_string(),
        )
    })?;
    let arena_id = ledger.objects[object_index].stable.arena_id;
    ledger.transitions.push(MtgoObjectLedgerTransitionV1 {
        sequence,
        kind,
        calibration_commitment_sha256: calibration.transition_commitment_sha256().to_owned(),
        before_frame_sha256: calibration.before_frame_sha256().to_owned(),
        after_frame_sha256: calibration.after_frame_sha256().to_owned(),
        source_visible_object_id: source_visible_object_id.to_owned(),
        result_visible_object_id,
        arena_id,
        before_zone_change_count,
        after_zone_change_count,
        remaining_hand_reflow: Vec::new(),
    });
    ledger.current_frame_sha256 = calibration.after_frame_sha256().to_owned();
    ledger.ledger_commitment_sha256 = commit_ledger_v1(&ledger)?;
    Ok(ledger)
}

/// Advances one checked PlayLand transition and relabels every remaining local
/// hand object in display order.
///
/// MTGO reflows the remaining cards after a land leaves the hand. This helper
/// requires the complete post-action slot list and preserves each object's
/// synthetic arena identifier while updating only its visible slot identifier.
/// The checked calibration still remains untrusted semantic evidence, and the
/// returned ledger cannot expose bindings or authorize observation, scoring,
/// or input outside this crate.
pub fn advance_checked_untrusted_visible_play_land_with_hand_reflow_v1(
    ledger: CheckedUntrustedMtgoVisibleObjectLedgerV1,
    calibration: &CheckedUntrustedMtgoVisibleObjectActionCalibrationV1,
    result_visible_object_id: &str,
    after_hand_slots: Vec<MtgoVisibleHandReflowSlotV1>,
) -> Result<CheckedUntrustedMtgoVisibleObjectLedgerV1, MtgoContractErrorV1> {
    let (actor, played_source_visible_object_id) = match calibration.action() {
        MtgoVisibleObjectCalibrationActionV1::PlayLand {
            actor,
            source_adapter_object_id,
            ..
        } => (*actor, source_adapter_object_id.as_str()),
        MtgoVisibleObjectCalibrationActionV1::ActivateManaAbility { .. } => {
            return Err(MtgoContractErrorV1::new(
                "object_ledger_hand_reflow_action_invalid",
                "hand reflow is defined only for PlayLand",
            ));
        }
    };
    if ledger.current_frame_sha256 != calibration.before_frame_sha256() {
        return Err(MtgoContractErrorV1::new(
            "object_ledger_transition_frame_mismatch",
            "the calibration does not begin at the ledger's current frame",
        ));
    }

    let played_index = ledger
        .objects
        .iter()
        .position(|object| object.visible_object_id == played_source_visible_object_id)
        .ok_or_else(|| {
            MtgoContractErrorV1::new(
                "object_ledger_transition_source_missing",
                played_source_visible_object_id,
            )
        })?;
    let remaining_indices = ledger
        .objects
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            (index != played_index
                && object.stable.owner == actor
                && object.stable.controller == actor
                && object.stable.zone == Zone::Hand)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    if after_hand_slots.len() != remaining_indices.len() {
        return Err(MtgoContractErrorV1::new(
            "object_ledger_hand_reflow_count_mismatch",
            format!(
                "expected={},actual={}",
                remaining_indices.len(),
                after_hand_slots.len()
            ),
        ));
    }

    validate_visible_object_id_v1(result_visible_object_id)?;
    let remaining_index_set = remaining_indices.iter().copied().collect::<HashSet<_>>();
    let mut result_ids = HashSet::with_capacity(after_hand_slots.len());
    let mut reflow = Vec::with_capacity(after_hand_slots.len());
    for (expected_ordinal, (&object_index, slot)) in remaining_indices
        .iter()
        .zip(after_hand_slots.iter())
        .enumerate()
    {
        let expected_ordinal = u32::try_from(expected_ordinal).map_err(|_| {
            MtgoContractErrorV1::new(
                "object_ledger_hand_reflow_ordinal_overflow",
                expected_ordinal.to_string(),
            )
        })?;
        if slot.display_ordinal != expected_ordinal {
            return Err(MtgoContractErrorV1::new(
                "object_ledger_hand_reflow_order_invalid",
                format!(
                    "expected={expected_ordinal},actual={}",
                    slot.display_ordinal
                ),
            ));
        }
        validate_visible_object_id_v1(&slot.visible_object_id)?;
        if !result_ids.insert(slot.visible_object_id.as_str())
            || slot.visible_object_id == result_visible_object_id
            || ledger.objects.iter().enumerate().any(|(index, object)| {
                !remaining_index_set.contains(&index)
                    && object.visible_object_id == slot.visible_object_id
            })
        {
            return Err(MtgoContractErrorV1::new(
                "object_ledger_hand_reflow_result_id_invalid",
                slot.visible_object_id.as_str(),
            ));
        }
        let object = &ledger.objects[object_index];
        if slot.visible_card_name != object.visible_card_name {
            return Err(MtgoContractErrorV1::new(
                "object_ledger_hand_reflow_identity_mismatch",
                format!(
                    "expected={},actual={}",
                    object.visible_card_name, slot.visible_card_name
                ),
            ));
        }
        reflow.push(MtgoObjectLedgerHandReflowV1 {
            source_visible_object_id: object.visible_object_id.clone(),
            result_visible_object_id: slot.visible_object_id.clone(),
            visible_card_name: object.visible_card_name.clone(),
            arena_id: object.stable.arena_id,
        });
    }

    let mut ledger = advance_checked_untrusted_visible_object_calibration_v1(
        ledger,
        calibration,
        Some(result_visible_object_id),
    )?;
    for (&object_index, slot) in remaining_indices.iter().zip(after_hand_slots) {
        ledger.objects[object_index].visible_object_id = slot.visible_object_id;
    }
    let transition = ledger.transitions.last_mut().ok_or_else(|| {
        MtgoContractErrorV1::new(
            "object_ledger_hand_reflow_transition_missing",
            "PlayLand advance did not append a transition",
        )
    })?;
    transition.remaining_hand_reflow = reflow;
    ledger.ledger_commitment_sha256 = commit_ledger_v1(&ledger)?;
    Ok(ledger)
}

/// Advances a PlayLand transition whose complete 8-to-7 hand reflow was
/// uniquely matched by the retained offline visual classifier.
///
/// The visual result binds the calibration's declared source ordinal to the
/// ledger's ordered local hand. Post-action visible IDs are derived from the
/// after-frame hash, so callers cannot relabel surviving objects. The returned
/// ledger remains checked-untrusted and grants no observation, scoring, or
/// input authority.
pub fn advance_checked_untrusted_visible_play_land_from_offline_reflow_v1(
    mut ledger: CheckedUntrustedMtgoVisibleObjectLedgerV1,
    calibration: &CheckedUntrustedMtgoVisibleObjectActionCalibrationV1,
    reflow: &CheckedUntrustedMtgoOfflinePlayLandHandReflowV1,
) -> Result<CheckedUntrustedMtgoVisibleObjectLedgerV1, MtgoContractErrorV1> {
    if reflow.classification() != MtgoOfflinePlayLandHandReflowClassificationV1::Match
        || reflow.calibration_commitment_sha256() != calibration.transition_commitment_sha256()
        || reflow.before_frame_sha256() != calibration.before_frame_sha256()
        || reflow.after_frame_sha256() != calibration.after_frame_sha256()
        || reflow.before_hand_count() != 8
        || reflow.after_hand_count() != 7
        || reflow.unique_visual_removed_ordinal() != Some(reflow.expected_source_ordinal())
    {
        return Err(MtgoContractErrorV1::new(
            "object_ledger_offline_reflow_not_matched",
            "ledger advance requires the exact uniquely matched visual reflow",
        ));
    }
    if ledger.current_frame_sha256 != calibration.before_frame_sha256() {
        return Err(MtgoContractErrorV1::new(
            "object_ledger_transition_frame_mismatch",
            "the calibration does not begin at the ledger's current frame",
        ));
    }
    if ledger.transitions.len() >= MAX_OBJECT_TRANSITIONS_V1 {
        return Err(MtgoContractErrorV1::new(
            "object_ledger_transition_count_invalid",
            ledger.transitions.len().to_string(),
        ));
    }

    let (actor, visible_card_name) = match calibration.action() {
        MtgoVisibleObjectCalibrationActionV1::PlayLand {
            actor,
            visible_card_name,
            ..
        } => (*actor, visible_card_name.as_str()),
        MtgoVisibleObjectCalibrationActionV1::ActivateManaAbility { .. } => {
            return Err(MtgoContractErrorV1::new(
                "object_ledger_offline_reflow_action_invalid",
                "offline hand reflow is defined only for PlayLand",
            ));
        }
    };
    let hand_indices = ledger
        .objects
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            (object.stable.owner == actor
                && object.stable.controller == actor
                && object.stable.zone == Zone::Hand)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    if hand_indices.len() != usize::from(reflow.before_hand_count()) {
        return Err(MtgoContractErrorV1::new(
            "object_ledger_offline_reflow_hand_count_mismatch",
            format!(
                "expected={},actual={}",
                reflow.before_hand_count(),
                hand_indices.len()
            ),
        ));
    }
    let played_hand_ordinal = usize::from(reflow.expected_source_ordinal());
    let played_index = *hand_indices.get(played_hand_ordinal).ok_or_else(|| {
        MtgoContractErrorV1::new(
            "object_ledger_offline_reflow_source_missing",
            reflow.expected_source_ordinal().to_string(),
        )
    })?;
    let played = &ledger.objects[played_index];
    if played.visible_card_name != visible_card_name || played.stable.zone != Zone::Hand {
        return Err(MtgoContractErrorV1::new(
            "object_ledger_offline_reflow_source_mismatch",
            format!(
                "expected={visible_card_name},actual={}",
                played.visible_card_name
            ),
        ));
    }

    let before_zone_change_count = played.stable.zone_change_count;
    let after_zone_change_count = before_zone_change_count.checked_add(1).ok_or_else(|| {
        MtgoContractErrorV1::new(
            "object_ledger_zone_change_count_overflow",
            played.visible_object_id.as_str(),
        )
    })?;
    let source_visible_object_id = played.visible_object_id.clone();
    let result_visible_object_id = format!(
        "frame:{}:battlefield-land:0",
        calibration.after_frame_sha256()
    );
    validate_visible_object_id_v1(&result_visible_object_id)?;

    let remaining_indices = hand_indices
        .iter()
        .copied()
        .filter(|index| *index != played_index)
        .collect::<Vec<_>>();
    if remaining_indices.len() != usize::from(reflow.after_hand_count()) {
        return Err(MtgoContractErrorV1::new(
            "object_ledger_offline_reflow_remaining_count_mismatch",
            remaining_indices.len().to_string(),
        ));
    }
    let mut remaining_hand_reflow = Vec::with_capacity(remaining_indices.len());
    let mut result_ids = HashSet::with_capacity(remaining_indices.len() + 1);
    result_ids.insert(result_visible_object_id.clone());
    for (after_ordinal, object_index) in remaining_indices.iter().copied().enumerate() {
        let result_id = format!(
            "frame:{}:hand-slot:{}",
            calibration.after_frame_sha256(),
            after_ordinal
        );
        validate_visible_object_id_v1(&result_id)?;
        if !result_ids.insert(result_id.clone())
            || ledger.objects.iter().enumerate().any(|(index, object)| {
                !hand_indices.contains(&index) && object.visible_object_id == result_id
            })
        {
            return Err(MtgoContractErrorV1::new(
                "object_ledger_offline_reflow_result_id_invalid",
                result_id,
            ));
        }
        let object = &ledger.objects[object_index];
        remaining_hand_reflow.push(MtgoObjectLedgerHandReflowV1 {
            source_visible_object_id: object.visible_object_id.clone(),
            result_visible_object_id: result_id,
            visible_card_name: object.visible_card_name.clone(),
            arena_id: object.stable.arena_id,
        });
    }

    let played_arena_id = ledger.objects[played_index].stable.arena_id;
    ledger.objects[played_index].visible_object_id = result_visible_object_id.clone();
    ledger.objects[played_index].stable.zone = Zone::Battlefield;
    ledger.objects[played_index].stable.zone_change_count = after_zone_change_count;
    for (&object_index, entry) in remaining_indices.iter().zip(remaining_hand_reflow.iter()) {
        ledger.objects[object_index].visible_object_id = entry.result_visible_object_id.clone();
    }

    let sequence = u64::try_from(ledger.transitions.len() + 1).map_err(|_| {
        MtgoContractErrorV1::new(
            "object_ledger_transition_sequence_overflow",
            ledger.transitions.len().to_string(),
        )
    })?;
    ledger.transitions.push(MtgoObjectLedgerTransitionV1 {
        sequence,
        kind: MtgoObjectLedgerTransitionKindV1::PlayLandZoneChange,
        calibration_commitment_sha256: calibration.transition_commitment_sha256().to_owned(),
        before_frame_sha256: calibration.before_frame_sha256().to_owned(),
        after_frame_sha256: calibration.after_frame_sha256().to_owned(),
        source_visible_object_id,
        result_visible_object_id,
        arena_id: played_arena_id,
        before_zone_change_count,
        after_zone_change_count,
        remaining_hand_reflow,
    });
    ledger.current_frame_sha256 = calibration.after_frame_sha256().to_owned();
    ledger.ledger_commitment_sha256 = commit_ledger_v1(&ledger)?;
    Ok(ledger)
}

#[derive(Serialize)]
struct LedgerCommitmentObjectV1<'a> {
    visible_object_id: &'a str,
    visible_card_name: &'a str,
    stable: &'a CardStableRefV1,
}

#[derive(Serialize)]
struct LedgerCommitmentPayloadV1<'a> {
    schema_version: u32,
    current_frame_sha256: &'a str,
    objects: Vec<LedgerCommitmentObjectV1<'a>>,
    transitions: &'a [MtgoObjectLedgerTransitionV1],
}

#[derive(Serialize)]
struct SourceBoundLedgerCommitmentPayloadV1<'a> {
    schema_version: u32,
    current_frame_sha256: &'a str,
    seed_source_commitment_sha256: &'a str,
    objects: Vec<LedgerCommitmentObjectV1<'a>>,
    transitions: &'a [MtgoObjectLedgerTransitionV1],
}

fn commit_ledger_v1(
    ledger: &CheckedUntrustedMtgoVisibleObjectLedgerV1,
) -> Result<String, MtgoContractErrorV1> {
    let objects = || {
        ledger
            .objects
            .iter()
            .map(|object| LedgerCommitmentObjectV1 {
                visible_object_id: &object.visible_object_id,
                visible_card_name: &object.visible_card_name,
                stable: &object.stable,
            })
            .collect()
    };
    let (domain, encoded) = if let Some(seed_source_commitment_sha256) =
        ledger.seed_source_commitment_sha256.as_deref()
    {
        let payload = SourceBoundLedgerCommitmentPayloadV1 {
            schema_version: MTGO_VISIBLE_OBJECT_INCARNATION_SCHEMA_V1,
            current_frame_sha256: &ledger.current_frame_sha256,
            seed_source_commitment_sha256,
            objects: objects(),
            transitions: &ledger.transitions,
        };
        (
            SOURCE_BOUND_OBJECT_LEDGER_COMMITMENT_DOMAIN_V1,
            serde_json::to_vec(&payload),
        )
    } else {
        let payload = LedgerCommitmentPayloadV1 {
            schema_version: MTGO_VISIBLE_OBJECT_INCARNATION_SCHEMA_V1,
            current_frame_sha256: &ledger.current_frame_sha256,
            objects: objects(),
            transitions: &ledger.transitions,
        };
        (
            OBJECT_LEDGER_COMMITMENT_DOMAIN_V1,
            serde_json::to_vec(&payload),
        )
    };
    let encoded = encoded.map_err(|error| {
        MtgoContractErrorV1::new("object_ledger_serialization_failed", error.to_string())
    })?;
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((encoded.len() as u64).to_le_bytes());
    hasher.update(encoded);
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_visible_object_id_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > MAX_VISIBLE_OBJECT_ID_BYTES_V1
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(MtgoContractErrorV1::new(
            "object_ledger_visible_object_id_invalid",
            value,
        ));
    }
    Ok(())
}

fn require_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(MtgoContractErrorV1::new(code, value));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        matched_offline_play_land_reflow_for_test_v1,
        validate_visible_object_action_calibration_trace_v1,
        MtgoVisibleObjectActionCalibrationTraceV1,
    };

    fn calibration(json: &str) -> CheckedUntrustedMtgoVisibleObjectActionCalibrationV1 {
        let record: MtgoVisibleObjectActionCalibrationTraceV1 = serde_json::from_str(json).unwrap();
        validate_visible_object_action_calibration_trace_v1(record).unwrap()
    }

    fn island_seed(
        frame: &str,
        zone: Zone,
        visible_object_id: &str,
    ) -> CheckedUntrustedMtgoVisibleObjectLedgerV1 {
        start_checked_untrusted_visible_object_ledger_v1(
            frame,
            vec![MtgoVisibleObjectSeedV1 {
                display_ordinal: 0,
                visible_object_id: visible_object_id.to_owned(),
                visible_card_name: "Island".to_owned(),
                owner: PlayerSeatV1::P0,
                controller: PlayerSeatV1::P0,
                zone,
            }],
        )
        .unwrap()
    }

    fn three_card_hand_seed(frame: &str) -> CheckedUntrustedMtgoVisibleObjectLedgerV1 {
        start_checked_untrusted_visible_object_ledger_v1(
            frame,
            ["Island", "Forest", "Mountain"]
                .into_iter()
                .enumerate()
                .map(|(ordinal, name)| MtgoVisibleObjectSeedV1 {
                    display_ordinal: u32::try_from(ordinal).unwrap(),
                    visible_object_id: format!("before-frame:hand-slot-{ordinal}"),
                    visible_card_name: name.to_owned(),
                    owner: PlayerSeatV1::P0,
                    controller: PlayerSeatV1::P0,
                    zone: Zone::Hand,
                })
                .collect(),
        )
        .unwrap()
    }

    fn two_slot_reflow() -> Vec<MtgoVisibleHandReflowSlotV1> {
        vec![
            MtgoVisibleHandReflowSlotV1 {
                display_ordinal: 0,
                visible_object_id: "after-frame:hand-slot-0".to_owned(),
                visible_card_name: "Forest".to_owned(),
            },
            MtgoVisibleHandReflowSlotV1 {
                display_ordinal: 1,
                visible_object_id: "after-frame:hand-slot-1".to_owned(),
                visible_card_name: "Mountain".to_owned(),
            },
        ]
    }

    fn eight_card_first_main_seed(frame: &str) -> CheckedUntrustedMtgoVisibleObjectLedgerV1 {
        start_checked_untrusted_visible_object_ledger_v1(
            frame,
            [
                "Island", "Forest", "Forest", "Forest", "Forest", "Forest", "Forest", "Forest",
            ]
            .into_iter()
            .enumerate()
            .map(|(ordinal, name)| MtgoVisibleObjectSeedV1 {
                display_ordinal: u32::try_from(ordinal).unwrap(),
                visible_object_id: format!("first-main:hand-slot-{ordinal}"),
                visible_card_name: name.to_owned(),
                owner: PlayerSeatV1::P0,
                controller: PlayerSeatV1::P0,
                zone: Zone::Hand,
            })
            .collect(),
        )
        .unwrap()
    }

    #[test]
    fn live_play_land_then_mana_preserves_lineage_and_increments_once() {
        let play_land = calibration(include_str!(
            "../fixtures/solitaire_play_land_transition_v1.json"
        ));
        let mana = calibration(include_str!(
            "../fixtures/solitaire_activate_island_mana_transition_v1.json"
        ));
        let ledger = island_seed(
            play_land.before_frame_sha256(),
            Zone::Hand,
            "before-frame:hand-slot-0",
        );
        let initial = ledger.current_object_bindings_v1();
        assert_eq!(initial[0].kernel_ref.zone_change_count, 0);

        let ledger = advance_checked_untrusted_visible_object_calibration_v1(
            ledger,
            &play_land,
            Some("before-frame:battlefield-object-0"),
        )
        .unwrap();
        let after_land = ledger.current_object_bindings_v1();
        assert_eq!(
            after_land[0].kernel_ref.arena_id,
            initial[0].kernel_ref.arena_id
        );
        assert_eq!(after_land[0].kernel_ref.zone, Zone::Battlefield);
        assert_eq!(after_land[0].kernel_ref.zone_change_count, 1);

        let ledger =
            advance_checked_untrusted_visible_object_calibration_v1(ledger, &mana, None).unwrap();
        let after_mana = ledger.current_object_bindings_v1();
        assert_eq!(after_mana[0].kernel_ref, after_land[0].kernel_ref);
        assert_eq!(ledger.transition_count(), 2);
        assert_eq!(ledger.current_frame_sha256(), mana.after_frame_sha256());
    }

    #[test]
    fn matched_offline_reflow_preserves_seven_survivors_and_moves_played_island() {
        let play_land = calibration(include_str!(
            "../fixtures/solitaire_play_land_transition_v1.json"
        ));
        let reflow = matched_offline_play_land_reflow_for_test_v1(&play_land);
        let ledger = eight_card_first_main_seed(play_land.before_frame_sha256());
        let ledger = advance_checked_untrusted_visible_play_land_from_offline_reflow_v1(
            ledger, &play_land, &reflow,
        )
        .unwrap();

        assert_eq!(
            ledger.current_frame_sha256(),
            play_land.after_frame_sha256()
        );
        assert_eq!(ledger.object_count(), 8);
        assert_eq!(ledger.transition_count(), 1);
        assert!(!ledger.safe_for_observation_v5());
        assert!(!ledger.safe_for_policy_scoring());
        assert!(!ledger.safe_for_input());

        let bindings = ledger.current_object_bindings_v1();
        assert_eq!(bindings.len(), 8);
        assert_eq!(bindings[0].kernel_ref.arena_id, 1);
        assert_eq!(bindings[0].kernel_ref.zone, Zone::Battlefield);
        assert_eq!(bindings[0].kernel_ref.zone_change_count, 1);
        assert_eq!(
            bindings[0].kernel_ref.card_db_id,
            resolve_checked_untrusted_kernel_card_correspondence_v1("Island")
                .unwrap()
                .card_db_id()
        );
        assert!(bindings[0]
            .adapter_object_id
            .ends_with(":battlefield-land:0"));
        for (ordinal, binding) in bindings.iter().enumerate().skip(1) {
            assert_eq!(
                binding.kernel_ref.arena_id,
                u32::try_from(ordinal + 1).unwrap()
            );
            assert_eq!(binding.kernel_ref.zone, Zone::Hand);
            assert_eq!(binding.kernel_ref.zone_change_count, 0);
            assert!(binding
                .adapter_object_id
                .ends_with(&format!(":hand-slot:{}", ordinal - 1)));
        }
    }

    #[test]
    fn matched_offline_reflow_rejects_a_ledger_with_the_wrong_source_card() {
        let play_land = calibration(include_str!(
            "../fixtures/solitaire_play_land_transition_v1.json"
        ));
        let reflow = matched_offline_play_land_reflow_for_test_v1(&play_land);
        let mut ledger = eight_card_first_main_seed(play_land.before_frame_sha256());
        ledger.objects.swap(0, 1);
        assert!(
            advance_checked_untrusted_visible_play_land_from_offline_reflow_v1(
                ledger, &play_land, &reflow,
            )
            .is_err()
        );
    }

    #[test]
    fn play_land_reflow_preserves_every_remaining_object_in_display_order() {
        let play_land = calibration(include_str!(
            "../fixtures/solitaire_play_land_transition_v1.json"
        ));
        let ledger = three_card_hand_seed(play_land.before_frame_sha256());
        let before = ledger.current_object_bindings_v1();
        let ordinary = advance_checked_untrusted_visible_object_calibration_v1(
            three_card_hand_seed(play_land.before_frame_sha256()),
            &play_land,
            Some("after-frame:battlefield-land-0"),
        )
        .unwrap();

        let ledger = advance_checked_untrusted_visible_play_land_with_hand_reflow_v1(
            ledger,
            &play_land,
            "after-frame:battlefield-land-0",
            two_slot_reflow(),
        )
        .unwrap();
        let after = ledger.current_object_bindings_v1();

        assert_eq!(
            ledger.current_frame_sha256(),
            play_land.after_frame_sha256()
        );
        assert_eq!(ledger.transition_count(), 1);
        assert_eq!(after.len(), 3);
        assert_eq!(after[0].adapter_object_id, "after-frame:battlefield-land-0");
        assert_eq!(after[0].kernel_ref.arena_id, before[0].kernel_ref.arena_id);
        assert_eq!(after[0].kernel_ref.zone, Zone::Battlefield);
        assert_eq!(after[0].kernel_ref.zone_change_count, 1);
        assert_eq!(after[1].adapter_object_id, "after-frame:hand-slot-0");
        assert_eq!(after[2].adapter_object_id, "after-frame:hand-slot-1");
        assert_eq!(after[1].kernel_ref, before[1].kernel_ref);
        assert_eq!(after[2].kernel_ref, before[2].kernel_ref);
        assert_ne!(
            ledger.ledger_commitment_sha256(),
            ordinary.ledger_commitment_sha256()
        );
        assert!(!ledger.safe_for_observation_v5());
        assert!(!ledger.safe_for_policy_scoring());
        assert!(!ledger.safe_for_input());
    }

    #[test]
    fn play_land_reflow_requires_complete_unique_ordered_matching_slots() {
        let play_land = calibration(include_str!(
            "../fixtures/solitaire_play_land_transition_v1.json"
        ));
        let result_id = "after-frame:battlefield-land-0";

        let missing = advance_checked_untrusted_visible_play_land_with_hand_reflow_v1(
            three_card_hand_seed(play_land.before_frame_sha256()),
            &play_land,
            result_id,
            two_slot_reflow()[..1].to_vec(),
        )
        .err()
        .unwrap();
        assert_eq!(missing.code(), "object_ledger_hand_reflow_count_mismatch");

        let mut wrong_order = two_slot_reflow();
        wrong_order[0].display_ordinal = 1;
        let wrong_order = advance_checked_untrusted_visible_play_land_with_hand_reflow_v1(
            three_card_hand_seed(play_land.before_frame_sha256()),
            &play_land,
            result_id,
            wrong_order,
        )
        .err()
        .unwrap();
        assert_eq!(
            wrong_order.code(),
            "object_ledger_hand_reflow_order_invalid"
        );

        let mut wrong_name = two_slot_reflow();
        wrong_name[0].visible_card_name = "Mountain".to_owned();
        let wrong_name = advance_checked_untrusted_visible_play_land_with_hand_reflow_v1(
            three_card_hand_seed(play_land.before_frame_sha256()),
            &play_land,
            result_id,
            wrong_name,
        )
        .err()
        .unwrap();
        assert_eq!(
            wrong_name.code(),
            "object_ledger_hand_reflow_identity_mismatch"
        );

        let mut duplicate = two_slot_reflow();
        duplicate[1].visible_object_id = duplicate[0].visible_object_id.clone();
        let duplicate = advance_checked_untrusted_visible_play_land_with_hand_reflow_v1(
            three_card_hand_seed(play_land.before_frame_sha256()),
            &play_land,
            result_id,
            duplicate,
        )
        .err()
        .unwrap();
        assert_eq!(
            duplicate.code(),
            "object_ledger_hand_reflow_result_id_invalid"
        );

        let mut played_result_collision = two_slot_reflow();
        played_result_collision[0].visible_object_id = result_id.to_owned();
        let collision = advance_checked_untrusted_visible_play_land_with_hand_reflow_v1(
            three_card_hand_seed(play_land.before_frame_sha256()),
            &play_land,
            result_id,
            played_result_collision,
        )
        .err()
        .unwrap();
        assert_eq!(
            collision.code(),
            "object_ledger_hand_reflow_result_id_invalid"
        );
    }

    #[test]
    fn hand_reflow_rejects_non_play_land_and_unknown_json_fields() {
        let mana = calibration(include_str!(
            "../fixtures/solitaire_activate_island_mana_transition_v1.json"
        ));
        let error = advance_checked_untrusted_visible_play_land_with_hand_reflow_v1(
            island_seed(
                mana.before_frame_sha256(),
                Zone::Battlefield,
                "before-frame:battlefield-object-0",
            ),
            &mana,
            "after-frame:battlefield-object-0",
            Vec::new(),
        )
        .err()
        .unwrap();
        assert_eq!(error.code(), "object_ledger_hand_reflow_action_invalid");

        let json = r#"{
            "display_ordinal": 0,
            "visible_object_id": "after-frame:hand-slot-0",
            "visible_card_name": "Forest",
            "input_authority": true
        }"#;
        assert!(serde_json::from_str::<MtgoVisibleHandReflowSlotV1>(json).is_err());
    }

    #[test]
    fn frame_source_name_zone_and_result_shape_fail_closed() {
        let play_land = calibration(include_str!(
            "../fixtures/solitaire_play_land_transition_v1.json"
        ));
        let wrong_frame = island_seed(&"0".repeat(64), Zone::Hand, "before-frame:hand-slot-0");
        assert_eq!(
            advance_checked_untrusted_visible_object_calibration_v1(
                wrong_frame,
                &play_land,
                Some("before-frame:battlefield-object-0"),
            )
            .err()
            .unwrap()
            .code(),
            "object_ledger_transition_frame_mismatch"
        );

        let missing_source = island_seed(
            play_land.before_frame_sha256(),
            Zone::Hand,
            "different:hand-slot-0",
        );
        assert_eq!(
            advance_checked_untrusted_visible_object_calibration_v1(
                missing_source,
                &play_land,
                Some("before-frame:battlefield-object-0"),
            )
            .err()
            .unwrap()
            .code(),
            "object_ledger_transition_source_missing"
        );

        let wrong_name = start_checked_untrusted_visible_object_ledger_v1(
            play_land.before_frame_sha256(),
            vec![MtgoVisibleObjectSeedV1 {
                display_ordinal: 0,
                visible_object_id: "before-frame:hand-slot-0".to_owned(),
                visible_card_name: "Mountain".to_owned(),
                owner: PlayerSeatV1::P0,
                controller: PlayerSeatV1::P0,
                zone: Zone::Hand,
            }],
        )
        .unwrap();
        assert_eq!(
            advance_checked_untrusted_visible_object_calibration_v1(
                wrong_name,
                &play_land,
                Some("before-frame:battlefield-object-0"),
            )
            .err()
            .unwrap()
            .code(),
            "object_ledger_transition_source_mismatch"
        );

        let wrong_zone = island_seed(
            play_land.before_frame_sha256(),
            Zone::Battlefield,
            "before-frame:hand-slot-0",
        );
        assert_eq!(
            advance_checked_untrusted_visible_object_calibration_v1(
                wrong_zone,
                &play_land,
                Some("before-frame:battlefield-object-0"),
            )
            .err()
            .unwrap()
            .code(),
            "object_ledger_play_land_source_zone_invalid"
        );

        let missing_result = island_seed(
            play_land.before_frame_sha256(),
            Zone::Hand,
            "before-frame:hand-slot-0",
        );
        assert_eq!(
            advance_checked_untrusted_visible_object_calibration_v1(
                missing_result,
                &play_land,
                None,
            )
            .err()
            .unwrap()
            .code(),
            "object_ledger_play_land_result_missing"
        );
    }

    #[test]
    fn mana_activation_rejects_a_replacement_visible_object() {
        let mana = calibration(include_str!(
            "../fixtures/solitaire_activate_island_mana_transition_v1.json"
        ));
        let ledger = island_seed(
            mana.before_frame_sha256(),
            Zone::Battlefield,
            "before-frame:battlefield-object-0",
        );
        assert_eq!(
            advance_checked_untrusted_visible_object_calibration_v1(
                ledger,
                &mana,
                Some("after-frame:battlefield-object-0"),
            )
            .err()
            .unwrap()
            .code(),
            "object_ledger_mana_activation_shape_invalid"
        );
    }

    #[test]
    fn duplicate_order_unsupported_card_and_invalid_identifier_reject() {
        let frame = "1".repeat(64);
        let seed = |ordinal, id: &str, name: &str| MtgoVisibleObjectSeedV1 {
            display_ordinal: ordinal,
            visible_object_id: id.to_owned(),
            visible_card_name: name.to_owned(),
            owner: PlayerSeatV1::P0,
            controller: PlayerSeatV1::P0,
            zone: Zone::Hand,
        };
        assert_eq!(
            start_checked_untrusted_visible_object_ledger_v1(
                &frame,
                vec![seed(1, "object-0", "Island")],
            )
            .err()
            .unwrap()
            .code(),
            "object_ledger_display_order_invalid"
        );
        assert_eq!(
            start_checked_untrusted_visible_object_ledger_v1(
                &frame,
                vec![seed(0, "same", "Island"), seed(1, "same", "Plains")],
            )
            .err()
            .unwrap()
            .code(),
            "object_ledger_duplicate_visible_object_id"
        );
        assert_eq!(
            start_checked_untrusted_visible_object_ledger_v1(
                &frame,
                vec![seed(0, "object-0", "Tolarian Terror")],
            )
            .err()
            .unwrap()
            .code(),
            "kernel_card_not_fully_supported"
        );
        assert_eq!(
            start_checked_untrusted_visible_object_ledger_v1(
                &frame,
                vec![seed(0, "bad id", "Island")],
            )
            .err()
            .unwrap()
            .code(),
            "object_ledger_visible_object_id_invalid"
        );
    }
}
