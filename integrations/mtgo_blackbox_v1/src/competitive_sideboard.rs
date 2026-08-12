use crate::{
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveLifecyclePhaseV1, MtgoContractErrorV1, MtgoRectPxV1,
};
use mtg_kernel::card_def::{CardCapability, CARD_DEFS};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1: u32 = 1;
const MIN_SIDEBOARD_CONFIDENCE_BPS_V1: u16 = 9_500;
const MAX_DECK_CARD_KINDS_V1: usize = 256;
const MAX_DECK_CARDS_V1: u32 = 350;
const DECK_MANIFEST_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-competitive-deck-manifest-v1";
const SIDEBOARD_SNAPSHOT_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-visible-competitive-sideboard-snapshot-v1";
const SIDEBOARD_PLAN_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-competitive-sideboard-transfer-plan-v1";
const SIDEBOARD_READY_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-sideboard-ready-to-submit-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveDeckPartitionV1 {
    Mainboard,
    Sideboard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveDeckCardCountV1 {
    pub card_db_id: u16,
    pub card_name: String,
    pub count: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveDeckConfigurationV1 {
    pub mainboard: Vec<MtgoCompetitiveDeckCardCountV1>,
    pub sideboard: Vec<MtgoCompetitiveDeckCardCountV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveDeckManifestV1 {
    pub schema_version: u32,
    pub deck_list_sha256: String,
    pub format_sha256: String,
    pub starting_mainboard_count: u16,
    pub starting_sideboard_count: u16,
    pub configuration: MtgoCompetitiveDeckConfigurationV1,
}

/// A canonical local deck identity. It proves exact contents and kernel card
/// support, but does not itself prove format legality, visible MTGO selection,
/// event entry, spending authority, or input authority.
pub struct ValidatedMtgoCompetitiveDeckManifestV1 {
    manifest: MtgoCompetitiveDeckManifestV1,
    inventory: BTreeMap<u16, u16>,
    manifest_commitment_sha256: String,
}

impl ValidatedMtgoCompetitiveDeckManifestV1 {
    pub fn manifest_commitment_sha256(&self) -> &str {
        &self.manifest_commitment_sha256
    }

    pub fn format_sha256(&self) -> &str {
        &self.manifest.format_sha256
    }

    pub fn deck_list_sha256(&self) -> &str {
        &self.manifest.deck_list_sha256
    }

    pub fn starting_mainboard_count(&self) -> u16 {
        self.manifest.starting_mainboard_count
    }

    pub fn starting_sideboard_count(&self) -> u16 {
        self.manifest.starting_sideboard_count
    }

    pub fn configuration_v1(&self) -> &MtgoCompetitiveDeckConfigurationV1 {
        &self.manifest.configuration
    }

    pub fn claims_format_legality_v1(&self) -> bool {
        false
    }

    pub fn permits_live_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleCompetitiveSideboardCardV1 {
    pub partition: MtgoCompetitiveDeckPartitionV1,
    pub card_name: String,
    pub count: u16,
    pub rect_client_px: MtgoRectPxV1,
    pub content_sha256: String,
    pub confidence_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleCompetitiveSideboardZoneV1 {
    pub rect_client_px: MtgoRectPxV1,
    pub content_sha256: String,
    pub empty_drop_rect_client_px: MtgoRectPxV1,
    pub empty_drop_content_sha256: String,
    pub confidence_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleCompetitiveSideboardSnapshotV1 {
    pub schema_version: u32,
    pub snapshot_id: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub frame_sha256: String,
    pub lifecycle_snapshot_commitment_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub visible_configuration_complete: bool,
    pub mainboard_zone: MtgoVisibleCompetitiveSideboardZoneV1,
    pub sideboard_zone: MtgoVisibleCompetitiveSideboardZoneV1,
    pub cards: Vec<MtgoVisibleCompetitiveSideboardCardV1>,
}

/// Complete coordinate-private interpretation of one visible sideboard state.
///
/// The retained lifecycle snapshot and raw regions remain private. The value
/// has no conversion to input or lifecycle submission.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1) {
///     let _ = value.input_command();
///     let _ = value.card_rectangles();
///     let _ = value.submit_sideboard();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1 {
    _lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    configuration: MtgoCompetitiveDeckConfigurationV1,
    event_kind: MtgoCompetitiveEventKindV1,
    event_identity_sha256: String,
    match_identity_sha256: String,
    game_number: u8,
    frame_id: u64,
    frame_sequence: u64,
    frame_sha256: String,
    deck_manifest_commitment_sha256: String,
    policy_deployment_commitment_sha256: String,
    starting_mainboard_count: u16,
    starting_sideboard_count: u16,
    snapshot_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1 {
    pub fn configuration_v1(&self) -> &MtgoCompetitiveDeckConfigurationV1 {
        &self.configuration
    }

    pub fn event_kind(&self) -> MtgoCompetitiveEventKindV1 {
        self.event_kind
    }

    pub fn event_identity_sha256(&self) -> &str {
        &self.event_identity_sha256
    }

    pub fn match_identity_sha256(&self) -> &str {
        &self.match_identity_sha256
    }

    pub fn game_number(&self) -> u8 {
        self.game_number
    }

    pub fn frame_id(&self) -> u64 {
        self.frame_id
    }

    pub fn frame_sequence(&self) -> u64 {
        self.frame_sequence
    }

    pub fn frame_sha256(&self) -> &str {
        &self.frame_sha256
    }

    pub fn lifecycle_snapshot_commitment_sha256(&self) -> &str {
        self._lifecycle.snapshot_commitment_sha256()
    }

    pub fn deck_manifest_commitment_sha256(&self) -> &str {
        &self.deck_manifest_commitment_sha256
    }

    pub fn policy_deployment_commitment_sha256(&self) -> &str {
        &self.policy_deployment_commitment_sha256
    }

    pub fn snapshot_commitment_sha256(&self) -> &str {
        &self.snapshot_commitment_sha256
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveSideboardSelectionV1 {
    pub schema_version: u32,
    pub source_snapshot_commitment_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub target_configuration: MtgoCompetitiveDeckConfigurationV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveSideboardTransferDirectionV1 {
    MainboardToSideboard,
    SideboardToMainboard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveSideboardTransferV1 {
    pub card_db_id: u16,
    pub card_name: String,
    pub direction: MtgoCompetitiveSideboardTransferDirectionV1,
    pub count: u16,
}

/// Coordinate-free model-selected target configuration and deterministic
/// transfer list. It has no pixels, rectangles, input primitive, or submit
/// conversion.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveSideboardPlanV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoCompetitiveSideboardPlanV1) {
///     let _ = value.target_points_client_px();
///     let _ = value.execute();
///     let _ = value.submit_sideboard();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveSideboardPlanV1 {
    source: CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1,
    target_configuration: MtgoCompetitiveDeckConfigurationV1,
    transfers: Vec<MtgoCompetitiveSideboardTransferV1>,
    plan_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveSideboardPlanV1 {
    pub fn source_configuration_v1(&self) -> &MtgoCompetitiveDeckConfigurationV1 {
        self.source.configuration_v1()
    }

    pub fn target_configuration_v1(&self) -> &MtgoCompetitiveDeckConfigurationV1 {
        &self.target_configuration
    }

    pub fn transfers_v1(&self) -> &[MtgoCompetitiveSideboardTransferV1] {
        &self.transfers
    }

    pub fn no_changes_selected_v1(&self) -> bool {
        self.transfers.is_empty()
    }

    pub fn source_snapshot_commitment_sha256(&self) -> &str {
        self.source.snapshot_commitment_sha256()
    }

    pub fn plan_commitment_sha256(&self) -> &str {
        &self.plan_commitment_sha256
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }
}

/// A newer complete visible configuration exactly matching the selected
/// target. This remains checked-untrusted and cannot submit the UI.
pub struct CheckedUntrustedMtgoCompetitiveSideboardReadyV1 {
    _plan: CheckedUntrustedMtgoCompetitiveSideboardPlanV1,
    _confirmed_snapshot: CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1,
    event_kind: MtgoCompetitiveEventKindV1,
    event_identity_sha256: String,
    match_identity_sha256: String,
    game_number: u8,
    deck_manifest_commitment_sha256: String,
    policy_deployment_commitment_sha256: String,
    after_snapshot_commitment_sha256: String,
    after_lifecycle_snapshot_commitment_sha256: String,
    after_frame_id: u64,
    after_frame_sequence: u64,
    after_frame_sha256: String,
    ready_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveSideboardReadyV1 {
    pub fn event_kind(&self) -> MtgoCompetitiveEventKindV1 {
        self.event_kind
    }

    pub fn game_number(&self) -> u8 {
        self.game_number
    }

    pub fn event_identity_sha256(&self) -> &str {
        &self.event_identity_sha256
    }

    pub fn match_identity_sha256(&self) -> &str {
        &self.match_identity_sha256
    }

    pub fn deck_manifest_commitment_sha256(&self) -> &str {
        &self.deck_manifest_commitment_sha256
    }

    pub fn policy_deployment_commitment_sha256(&self) -> &str {
        &self.policy_deployment_commitment_sha256
    }

    pub fn after_snapshot_commitment_sha256(&self) -> &str {
        &self.after_snapshot_commitment_sha256
    }

    pub fn after_lifecycle_snapshot_commitment_sha256(&self) -> &str {
        &self.after_lifecycle_snapshot_commitment_sha256
    }

    pub fn after_frame_id(&self) -> u64 {
        self.after_frame_id
    }

    pub fn after_frame_sequence(&self) -> u64 {
        self.after_frame_sequence
    }

    pub fn after_frame_sha256(&self) -> &str {
        &self.after_frame_sha256
    }

    pub fn ready_commitment_sha256(&self) -> &str {
        &self.ready_commitment_sha256
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }
}

pub fn validate_competitive_deck_manifest_v1(
    manifest: MtgoCompetitiveDeckManifestV1,
) -> Result<ValidatedMtgoCompetitiveDeckManifestV1, MtgoContractErrorV1> {
    if manifest.schema_version != MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1 {
        return Err(error(
            "sideboard_manifest_schema",
            "expected schema version 1",
        ));
    }
    if !is_sha256_v1(&manifest.deck_list_sha256)
        || !is_sha256_v1(&manifest.format_sha256)
        || manifest.deck_list_sha256 == manifest.format_sha256
    {
        return Err(error(
            "sideboard_manifest_format",
            "deck-list and format commitments must be distinct lowercase SHA-256 digests",
        ));
    }
    let mainboard =
        validate_configuration_partition_v1(&manifest.configuration.mainboard, "mainboard")?;
    let sideboard =
        validate_configuration_partition_v1(&manifest.configuration.sideboard, "sideboard")?;
    let mainboard_count = total_count_v1(&mainboard)?;
    let sideboard_count = total_count_v1(&sideboard)?;
    if mainboard_count != u32::from(manifest.starting_mainboard_count)
        || sideboard_count != u32::from(manifest.starting_sideboard_count)
        || manifest.starting_mainboard_count == 0
        || u32::from(manifest.starting_mainboard_count)
            .checked_add(u32::from(manifest.starting_sideboard_count))
            .is_none_or(|total| total > MAX_DECK_CARDS_V1)
    {
        return Err(error(
            "sideboard_manifest_count",
            "declared starting counts must exactly match one bounded nonempty deck",
        ));
    }
    let inventory = combined_inventory_v1(&mainboard, &sideboard)?;
    let manifest_bytes = canonical_json_v1(&manifest, "sideboard manifest")?;
    let manifest_commitment_sha256 = commitment_v1(
        DECK_MANIFEST_COMMITMENT_DOMAIN_V1,
        &[manifest_bytes.as_slice()],
    );
    Ok(ValidatedMtgoCompetitiveDeckManifestV1 {
        manifest,
        inventory,
        manifest_commitment_sha256,
    })
}

pub fn validate_visible_competitive_sideboard_snapshot_v1(
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    manifest: &ValidatedMtgoCompetitiveDeckManifestV1,
    snapshot: MtgoVisibleCompetitiveSideboardSnapshotV1,
) -> Result<CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1, MtgoContractErrorV1> {
    if snapshot.schema_version != MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1
        || !snapshot.visible_configuration_complete
        || snapshot.snapshot_id.is_empty()
        || snapshot.snapshot_id.len() > 128
        || snapshot.snapshot_id.trim() != snapshot.snapshot_id
        || snapshot.snapshot_id.chars().any(char::is_control)
    {
        return Err(error(
            "sideboard_snapshot_header",
            "snapshot header is incomplete or noncanonical",
        ));
    }
    if lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::Sideboarding
        || snapshot.event_kind != lifecycle.event_kind()
        || snapshot.event_identity_sha256 != lifecycle.event_identity_sha256_v1().unwrap_or("")
        || snapshot.match_identity_sha256 != lifecycle.match_identity_sha256_v1().unwrap_or("")
        || lifecycle.game_number_v1() != Some(snapshot.game_number)
        || !(1..=2).contains(&snapshot.game_number)
        || snapshot.frame_id != lifecycle.frame_id_v1()
        || snapshot.frame_sequence != lifecycle.frame_sequence()
        || snapshot.frame_sha256 != lifecycle.frame_sha256_v1()
        || snapshot.lifecycle_snapshot_commitment_sha256 != lifecycle.snapshot_commitment_sha256()
    {
        return Err(error(
            "sideboard_snapshot_lifecycle",
            "sideboard observation must match one exact between-game lifecycle frame",
        ));
    }
    if snapshot.deck_manifest_commitment_sha256 != manifest.manifest_commitment_sha256
        || !is_sha256_v1(&snapshot.policy_deployment_commitment_sha256)
        || snapshot.policy_deployment_commitment_sha256 == snapshot.deck_manifest_commitment_sha256
        || snapshot.policy_deployment_commitment_sha256 == manifest.format_sha256()
        || !is_sha256_v1(&snapshot.event_identity_sha256)
        || !is_sha256_v1(&snapshot.match_identity_sha256)
        || !is_sha256_v1(&snapshot.frame_sha256)
    {
        return Err(error(
            "sideboard_snapshot_identity",
            "deck, policy, frame, event, or match identity is invalid or mismatched",
        ));
    }
    validate_visible_zones_v1(
        &snapshot.mainboard_zone,
        &snapshot.sideboard_zone,
        lifecycle.client_bounds_v1(),
    )?;
    validate_visible_cards_v1(
        &snapshot.cards,
        &snapshot.mainboard_zone,
        &snapshot.sideboard_zone,
        lifecycle.client_bounds_v1(),
    )?;
    let configuration = configuration_from_visible_cards_v1(&snapshot.cards);
    validate_configuration_against_manifest_v1(&configuration, manifest)?;
    let snapshot_bytes = canonical_json_v1(&snapshot, "sideboard snapshot")?;
    let snapshot_commitment_sha256 = commitment_v1(
        SIDEBOARD_SNAPSHOT_COMMITMENT_DOMAIN_V1,
        &[
            lifecycle.snapshot_commitment_sha256().as_bytes(),
            manifest.manifest_commitment_sha256.as_bytes(),
            snapshot.policy_deployment_commitment_sha256.as_bytes(),
            snapshot_bytes.as_slice(),
        ],
    );
    Ok(CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1 {
        _lifecycle: lifecycle,
        configuration,
        event_kind: snapshot.event_kind,
        event_identity_sha256: snapshot.event_identity_sha256,
        match_identity_sha256: snapshot.match_identity_sha256,
        game_number: snapshot.game_number,
        frame_id: snapshot.frame_id,
        frame_sequence: snapshot.frame_sequence,
        frame_sha256: snapshot.frame_sha256,
        deck_manifest_commitment_sha256: snapshot.deck_manifest_commitment_sha256,
        policy_deployment_commitment_sha256: snapshot.policy_deployment_commitment_sha256,
        starting_mainboard_count: manifest.starting_mainboard_count(),
        starting_sideboard_count: manifest.starting_sideboard_count(),
        snapshot_commitment_sha256,
    })
}

pub fn validate_competitive_sideboard_selection_v1(
    source: CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1,
    selection: MtgoCompetitiveSideboardSelectionV1,
) -> Result<CheckedUntrustedMtgoCompetitiveSideboardPlanV1, MtgoContractErrorV1> {
    if selection.schema_version != MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1
        || selection.source_snapshot_commitment_sha256 != source.snapshot_commitment_sha256
        || selection.deck_manifest_commitment_sha256 != source.deck_manifest_commitment_sha256
        || selection.policy_deployment_commitment_sha256
            != source.policy_deployment_commitment_sha256
    {
        return Err(error(
            "sideboard_selection_identity",
            "selection must bind the exact visible source, deck, and policy deployment",
        ));
    }
    let target_mainboard = validate_configuration_partition_v1(
        &selection.target_configuration.mainboard,
        "target mainboard",
    )?;
    let target_sideboard = validate_configuration_partition_v1(
        &selection.target_configuration.sideboard,
        "target sideboard",
    )?;
    let source_inventory = combined_inventory_v1(
        &source.configuration.mainboard,
        &source.configuration.sideboard,
    )?;
    let target_inventory = combined_inventory_v1(&target_mainboard, &target_sideboard)?;
    let target_mainboard_count = total_count_v1(&target_mainboard)?;
    let target_sideboard_count = total_count_v1(&target_sideboard)?;
    if source_inventory != target_inventory
        || target_mainboard_count < u32::from(source.starting_mainboard_count)
        || target_sideboard_count > u32::from(source.starting_sideboard_count)
    {
        return Err(error(
            "sideboard_selection_inventory",
            "target must conserve the exact deck inventory and retain legal-size bounds",
        ));
    }
    let transfers = derive_transfers_v1(
        &source.configuration,
        &selection.target_configuration,
        &source_inventory,
    )?;
    let selection_bytes = canonical_json_v1(&selection, "sideboard selection")?;
    let transfer_bytes = canonical_json_v1(&transfers, "sideboard transfers")?;
    let plan_commitment_sha256 = commitment_v1(
        SIDEBOARD_PLAN_COMMITMENT_DOMAIN_V1,
        &[
            source.snapshot_commitment_sha256.as_bytes(),
            source.deck_manifest_commitment_sha256.as_bytes(),
            source.policy_deployment_commitment_sha256.as_bytes(),
            selection_bytes.as_slice(),
            transfer_bytes.as_slice(),
            b"coordinate_free_exact_inventory_sideboard_plan_no_input_no_submit",
        ],
    );
    Ok(CheckedUntrustedMtgoCompetitiveSideboardPlanV1 {
        source,
        target_configuration: selection.target_configuration,
        transfers,
        plan_commitment_sha256,
    })
}

pub fn confirm_competitive_sideboard_target_visible_v1(
    plan: CheckedUntrustedMtgoCompetitiveSideboardPlanV1,
    confirmed: CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1,
) -> Result<CheckedUntrustedMtgoCompetitiveSideboardReadyV1, MtgoContractErrorV1> {
    let source = &plan.source;
    if confirmed.event_kind != source.event_kind
        || confirmed.event_identity_sha256 != source.event_identity_sha256
        || confirmed.match_identity_sha256 != source.match_identity_sha256
        || confirmed.game_number != source.game_number
        || confirmed.deck_manifest_commitment_sha256 != source.deck_manifest_commitment_sha256
        || confirmed.policy_deployment_commitment_sha256
            != source.policy_deployment_commitment_sha256
        || confirmed.frame_id == source.frame_id
        || confirmed.frame_sequence <= source.frame_sequence
        || confirmed.frame_sha256 == source.frame_sha256
        || confirmed.configuration != plan.target_configuration
    {
        return Err(error(
            "sideboard_target_confirmation",
            "confirmation must be a changed newer exact target on the same game, deck, and policy",
        ));
    }
    let ready_commitment_sha256 = commitment_v1(
        SIDEBOARD_READY_COMMITMENT_DOMAIN_V1,
        &[
            plan.plan_commitment_sha256.as_bytes(),
            source.snapshot_commitment_sha256.as_bytes(),
            confirmed.snapshot_commitment_sha256.as_bytes(),
            confirmed.frame_sequence.to_be_bytes().as_slice(),
            b"exact_selected_sideboard_target_visible_no_input_no_submit",
        ],
    );
    Ok(CheckedUntrustedMtgoCompetitiveSideboardReadyV1 {
        event_kind: source.event_kind,
        event_identity_sha256: source.event_identity_sha256.clone(),
        match_identity_sha256: source.match_identity_sha256.clone(),
        game_number: source.game_number,
        deck_manifest_commitment_sha256: source.deck_manifest_commitment_sha256.clone(),
        policy_deployment_commitment_sha256: source.policy_deployment_commitment_sha256.clone(),
        after_snapshot_commitment_sha256: confirmed.snapshot_commitment_sha256.clone(),
        after_lifecycle_snapshot_commitment_sha256: confirmed
            ._lifecycle
            .snapshot_commitment_sha256()
            .to_owned(),
        after_frame_id: confirmed.frame_id,
        after_frame_sequence: confirmed.frame_sequence,
        after_frame_sha256: confirmed.frame_sha256.clone(),
        _plan: plan,
        _confirmed_snapshot: confirmed,
        ready_commitment_sha256,
    })
}

fn validate_visible_cards_v1(
    cards: &[MtgoVisibleCompetitiveSideboardCardV1],
    mainboard_zone: &MtgoVisibleCompetitiveSideboardZoneV1,
    sideboard_zone: &MtgoVisibleCompetitiveSideboardZoneV1,
    client_bounds: &MtgoRectPxV1,
) -> Result<(), MtgoContractErrorV1> {
    if cards.is_empty() || cards.len() > MAX_DECK_CARD_KINDS_V1 * 2 {
        return Err(error(
            "sideboard_visible_cards_count",
            "visible sideboard card list is empty or unbounded",
        ));
    }
    let mut prior_key = None;
    for (index, card) in cards.iter().enumerate() {
        validate_visible_card_identity_v1(&card.card_name, card.count)?;
        let key = (card.partition, card.card_name.as_str());
        if prior_key.is_some_and(|prior| key <= prior) {
            return Err(error(
                "sideboard_visible_cards_order",
                "visible card rows must be unique and canonical by partition and visible name",
            ));
        }
        prior_key = Some(key);
        if !is_sha256_v1(&card.content_sha256)
            || card.confidence_bps < MIN_SIDEBOARD_CONFIDENCE_BPS_V1
            || card.confidence_bps > 10_000
        {
            return Err(error(
                "sideboard_visible_card_evidence",
                "each card row requires one high-confidence visible pixel commitment",
            ));
        }
        validate_rect_inside_v1(&card.rect_client_px, client_bounds)?;
        let zone = match card.partition {
            MtgoCompetitiveDeckPartitionV1::Mainboard => mainboard_zone,
            MtgoCompetitiveDeckPartitionV1::Sideboard => sideboard_zone,
        };
        validate_rect_inside_v1(&card.rect_client_px, &zone.rect_client_px)?;
        if rects_intersect_v1(&card.rect_client_px, &zone.empty_drop_rect_client_px)? {
            return Err(error(
                "sideboard_visible_drop_overlap",
                "visible card evidence must not overlap the reviewed empty drop target",
            ));
        }
        for other in &cards[..index] {
            if rects_intersect_v1(&card.rect_client_px, &other.rect_client_px)? {
                return Err(error(
                    "sideboard_visible_card_overlap",
                    "visible card evidence regions must not overlap",
                ));
            }
        }
    }
    Ok(())
}

fn validate_visible_zones_v1(
    mainboard: &MtgoVisibleCompetitiveSideboardZoneV1,
    sideboard: &MtgoVisibleCompetitiveSideboardZoneV1,
    client_bounds: &MtgoRectPxV1,
) -> Result<(), MtgoContractErrorV1> {
    for zone in [mainboard, sideboard] {
        validate_rect_inside_v1(&zone.rect_client_px, client_bounds)?;
        validate_rect_inside_v1(&zone.empty_drop_rect_client_px, &zone.rect_client_px)?;
        if !is_sha256_v1(&zone.content_sha256)
            || !is_sha256_v1(&zone.empty_drop_content_sha256)
            || zone.confidence_bps < MIN_SIDEBOARD_CONFIDENCE_BPS_V1
            || zone.confidence_bps > 10_000
        {
            return Err(error(
                "sideboard_visible_zone_evidence",
                "each board zone and empty drop target require high-confidence pixel commitments",
            ));
        }
    }
    if rects_intersect_v1(&mainboard.rect_client_px, &sideboard.rect_client_px)? {
        return Err(error(
            "sideboard_visible_zone_overlap",
            "mainboard and sideboard zones must not overlap",
        ));
    }
    Ok(())
}

fn validate_configuration_against_manifest_v1(
    configuration: &MtgoCompetitiveDeckConfigurationV1,
    manifest: &ValidatedMtgoCompetitiveDeckManifestV1,
) -> Result<(), MtgoContractErrorV1> {
    let mainboard = validate_configuration_partition_v1(&configuration.mainboard, "mainboard")?;
    let sideboard = validate_configuration_partition_v1(&configuration.sideboard, "sideboard")?;
    let inventory = combined_inventory_v1(&mainboard, &sideboard)?;
    let mainboard_count = total_count_v1(&mainboard)?;
    let sideboard_count = total_count_v1(&sideboard)?;
    if inventory != manifest.inventory
        || mainboard_count < u32::from(manifest.starting_mainboard_count())
        || sideboard_count > u32::from(manifest.starting_sideboard_count())
    {
        return Err(error(
            "sideboard_visible_inventory",
            "visible configuration must conserve the exact manifest inventory and size bounds",
        ));
    }
    Ok(())
}

fn validate_configuration_partition_v1(
    cards: &[MtgoCompetitiveDeckCardCountV1],
    label: &str,
) -> Result<Vec<MtgoCompetitiveDeckCardCountV1>, MtgoContractErrorV1> {
    if cards.len() > MAX_DECK_CARD_KINDS_V1 {
        return Err(error(
            "sideboard_partition_count",
            format!("{label} has too many distinct card entries"),
        ));
    }
    let mut prior_id = None;
    for card in cards {
        validate_card_identity_v1(card.card_db_id, &card.card_name, card.count)?;
        if prior_id.is_some_and(|prior| card.card_db_id <= prior) {
            return Err(error(
                "sideboard_partition_order",
                format!("{label} must be unique and sorted by card_db_id"),
            ));
        }
        prior_id = Some(card.card_db_id);
    }
    Ok(cards.to_vec())
}

fn validate_card_identity_v1(
    card_db_id: u16,
    card_name: &str,
    count: u16,
) -> Result<(), MtgoContractErrorV1> {
    let definition = CARD_DEFS.get(usize::from(card_db_id)).ok_or_else(|| {
        error(
            "sideboard_card_id",
            format!("unknown kernel card id {card_db_id}"),
        )
    })?;
    if count == 0
        || definition.name != card_name
        || definition.capability != CardCapability::Full
        || definition.is_token
    {
        return Err(error(
            "sideboard_card_identity",
            "every card count must name one exact fully supported nontoken kernel card",
        ));
    }
    Ok(())
}

fn validate_visible_card_identity_v1(
    card_name: &str,
    count: u16,
) -> Result<(), MtgoContractErrorV1> {
    if card_name.is_empty()
        || card_name.len() > 256
        || card_name.trim() != card_name
        || card_name.chars().any(char::is_control)
    {
        return Err(error(
            "sideboard_visible_card_name",
            "every visible card row must carry one bounded exact UI card name",
        ));
    }
    let card_db_id = mtg_kernel::card_def::card_id_by_name(card_name).ok_or_else(|| {
        error(
            "sideboard_visible_card_name",
            "visible card name is absent from the pinned kernel card database",
        )
    })?;
    validate_card_identity_v1(card_db_id, card_name, count)?;
    Ok(())
}

fn configuration_from_visible_cards_v1(
    cards: &[MtgoVisibleCompetitiveSideboardCardV1],
) -> MtgoCompetitiveDeckConfigurationV1 {
    let mut mainboard = Vec::new();
    let mut sideboard = Vec::new();
    for card in cards {
        let card_db_id = mtg_kernel::card_def::card_id_by_name(&card.card_name)
            .expect("validated visible sideboard names resolve exactly");
        let count = MtgoCompetitiveDeckCardCountV1 {
            card_db_id,
            card_name: card.card_name.clone(),
            count: card.count,
        };
        match card.partition {
            MtgoCompetitiveDeckPartitionV1::Mainboard => mainboard.push(count),
            MtgoCompetitiveDeckPartitionV1::Sideboard => sideboard.push(count),
        }
    }
    MtgoCompetitiveDeckConfigurationV1 {
        mainboard,
        sideboard,
    }
}

fn derive_transfers_v1(
    source: &MtgoCompetitiveDeckConfigurationV1,
    target: &MtgoCompetitiveDeckConfigurationV1,
    inventory: &BTreeMap<u16, u16>,
) -> Result<Vec<MtgoCompetitiveSideboardTransferV1>, MtgoContractErrorV1> {
    let source_main = count_map_v1(&source.mainboard);
    let target_main = count_map_v1(&target.mainboard);
    let mut transfers = Vec::new();
    for card_db_id in inventory.keys().copied() {
        let before = source_main.get(&card_db_id).copied().unwrap_or(0);
        let after = target_main.get(&card_db_id).copied().unwrap_or(0);
        if before == after {
            continue;
        }
        let card_name = CARD_DEFS
            .get(usize::from(card_db_id))
            .ok_or_else(|| error("sideboard_transfer_card", "transfer card disappeared"))?
            .name
            .to_owned();
        let (direction, count) = if before > after {
            (
                MtgoCompetitiveSideboardTransferDirectionV1::MainboardToSideboard,
                before - after,
            )
        } else {
            (
                MtgoCompetitiveSideboardTransferDirectionV1::SideboardToMainboard,
                after - before,
            )
        };
        transfers.push(MtgoCompetitiveSideboardTransferV1 {
            card_db_id,
            card_name,
            direction,
            count,
        });
    }
    Ok(transfers)
}

fn count_map_v1(cards: &[MtgoCompetitiveDeckCardCountV1]) -> BTreeMap<u16, u16> {
    cards
        .iter()
        .map(|card| (card.card_db_id, card.count))
        .collect()
}

fn combined_inventory_v1(
    mainboard: &[MtgoCompetitiveDeckCardCountV1],
    sideboard: &[MtgoCompetitiveDeckCardCountV1],
) -> Result<BTreeMap<u16, u16>, MtgoContractErrorV1> {
    let mut inventory = BTreeMap::new();
    for card in mainboard.iter().chain(sideboard) {
        let count = inventory.entry(card.card_db_id).or_insert(0_u16);
        *count = count.checked_add(card.count).ok_or_else(|| {
            error(
                "sideboard_inventory_overflow",
                "combined card inventory count overflowed",
            )
        })?;
    }
    Ok(inventory)
}

fn total_count_v1(cards: &[MtgoCompetitiveDeckCardCountV1]) -> Result<u32, MtgoContractErrorV1> {
    cards.iter().try_fold(0_u32, |total, card| {
        total.checked_add(u32::from(card.count)).ok_or_else(|| {
            error(
                "sideboard_total_overflow",
                "deck partition card count overflowed",
            )
        })
    })
}

fn validate_rect_inside_v1(
    rect: &MtgoRectPxV1,
    bounds: &MtgoRectPxV1,
) -> Result<(), MtgoContractErrorV1> {
    if rect.width == 0 || rect.height == 0 {
        return Err(error(
            "sideboard_visible_rect",
            "visible card rectangle must be nonempty",
        ));
    }
    let rect_right = rect
        .x
        .checked_add(rect.width)
        .ok_or_else(|| error("sideboard_visible_rect", "rectangle x endpoint overflowed"))?;
    let rect_bottom = rect
        .y
        .checked_add(rect.height)
        .ok_or_else(|| error("sideboard_visible_rect", "rectangle y endpoint overflowed"))?;
    let bounds_right = bounds
        .x
        .checked_add(bounds.width)
        .ok_or_else(|| error("sideboard_visible_rect", "bounds x endpoint overflowed"))?;
    let bounds_bottom = bounds
        .y
        .checked_add(bounds.height)
        .ok_or_else(|| error("sideboard_visible_rect", "bounds y endpoint overflowed"))?;
    if rect.x < bounds.x
        || rect.y < bounds.y
        || rect_right > bounds_right
        || rect_bottom > bounds_bottom
    {
        return Err(error(
            "sideboard_visible_rect",
            "visible card rectangle lies outside the client bounds",
        ));
    }
    Ok(())
}

fn rects_intersect_v1(
    left: &MtgoRectPxV1,
    right: &MtgoRectPxV1,
) -> Result<bool, MtgoContractErrorV1> {
    let left_right = left
        .x
        .checked_add(left.width)
        .ok_or_else(|| error("sideboard_visible_rect", "left x endpoint overflowed"))?;
    let left_bottom = left
        .y
        .checked_add(left.height)
        .ok_or_else(|| error("sideboard_visible_rect", "left y endpoint overflowed"))?;
    let right_right = right
        .x
        .checked_add(right.width)
        .ok_or_else(|| error("sideboard_visible_rect", "right x endpoint overflowed"))?;
    let right_bottom = right
        .y
        .checked_add(right.height)
        .ok_or_else(|| error("sideboard_visible_rect", "right y endpoint overflowed"))?;
    Ok(left.x < right_right
        && right.x < left_right
        && left.y < right_bottom
        && right.y < left_bottom)
}

fn canonical_json_v1<T: Serialize>(value: &T, label: &str) -> Result<Vec<u8>, MtgoContractErrorV1> {
    serde_json::to_vec(value).map_err(|error| {
        crate::MtgoContractErrorV1::new("sideboard_json", format!("{label}: {error}"))
    })
}

fn is_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

fn error(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}
