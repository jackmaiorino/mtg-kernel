//! Python-compatible native tensors reconstructed from the V2 flat scorer view.
//!
//! The V2 implementation reconstructs every tensor consumed by Python
//! `EncodedDecision` from the typed scorer view, including the full one-based
//! card-token domain and the canonical-observation SHA-512 state tail.

use crate::flat_policy_v2::{
    FlatContextElementKindV2, FlatContextKindV2, FlatContextRelationDataV2, FlatContextSubroleV2,
    FlatEffectRelationDataV2, FlatEffectSubtypeChangeKindV2, FlatManaColorV2,
    FlatObjectAbilityUseV2, FlatObjectCoreV2 as FlatObjectCoreV1, FlatObjectGoadV2,
    FlatObjectGroupV2, FlatObjectSubtypeV2, FlatPendingEffectChoiceV2, FlatRelationPayloadV2,
    FlatRelationRoleV2, FlatRelationV2, FlatRelativePlayerV2 as FlatRelativePlayerV1,
    FlatScorerActionCoreV2 as FlatScorerActionCoreV1,
    FlatScorerActionKindV2 as FlatScorerActionKindV1,
    FlatScorerActionRefV2 as FlatScorerActionRefV1,
    FlatScoringDecisionViewV2 as FlatScoringDecisionViewV1, FlatTargetKindV2, FlatTurnRelationV2,
    FlatZoneV2 as FlatZoneV1,
};
use crate::rl_session::{
    FLAT_ACTION_FLAG_CAST_IT_V1, FLAT_ACTION_FLAG_CHANGE_TARGET_V1, FLAT_ACTION_FLAG_INCLUDE_V1,
    FLAT_ACTION_FLAG_PAY_V1, FLAT_ACTION_FLAG_USE_COST_V1, FLAT_ACTION_FLAG_VALUE_V1,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha512};
use std::collections::BTreeMap;
use std::fmt;

pub(crate) const NATIVE_FLAT_STATE_FEATURE_DIM_V2: usize = 219;
pub(crate) const NATIVE_FLAT_OBJECT_FEATURE_DIM_V2: usize = 98;
pub(crate) const NATIVE_FLAT_EDGE_FEATURE_DIM_V2: usize = 41;
pub(crate) const NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2: usize = 99;
pub(crate) const NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V2: usize = 96;
pub(crate) const NATIVE_FLAT_ACTION_FEATURE_DIM_V2: usize = 195;
pub(crate) const NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2: usize = 25;
pub(crate) const NATIVE_FLAT_OBJECT_GROUP_COUNT_V2: usize = 20;
pub(crate) const NATIVE_FLAT_MAX_CARD_TOKEN_V2: u32 = (u16::MAX as u32) + 1;

// The action encoder below was mechanically migrated from the reviewed V1
// implementation. These private aliases keep that byte-sensitive logic small
// while the module's externally consumed contract is explicitly V2.
const NATIVE_FLAT_STATE_FEATURE_DIM_V1: usize = NATIVE_FLAT_STATE_FEATURE_DIM_V2;
const NATIVE_FLAT_OBJECT_FEATURE_DIM_V1: usize = NATIVE_FLAT_OBJECT_FEATURE_DIM_V2;
const NATIVE_FLAT_EDGE_FEATURE_DIM_V1: usize = NATIVE_FLAT_EDGE_FEATURE_DIM_V2;
const NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V1: usize =
    NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2;
const NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V1: usize = NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V2;
const NATIVE_FLAT_ACTION_FEATURE_DIM_V1: usize = NATIVE_FLAT_ACTION_FEATURE_DIM_V2;
const NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1: usize = NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2;
const NATIVE_FLAT_OBJECT_GROUP_COUNT_V1: usize = NATIVE_FLAT_OBJECT_GROUP_COUNT_V2;
const NATIVE_FLAT_CURRENT_MAX_CARD_TOKEN_V1: u32 = NATIVE_FLAT_MAX_CARD_TOKEN_V2;

const ACTION_HASH_BLOCK_COUNT_V1: usize = 6;
const ACTION_HASH_BLOCK_BYTES_V1: usize = 64;
const ACTION_HASH_NAMESPACE_V1: &[u8] = b"legal-action";
const MAX_TRIGGER_REFS_V1: usize = 7;

const ROLE_SOURCE_V1: u8 = 0;
const ROLE_CANDIDATE_V1: u8 = 1;
const ROLE_CARD_V1: u8 = 2;
const ROLE_ATTACKER_V1: u8 = 3;
const ROLE_BLOCKER_V1: u8 = 4;
const ROLE_TARGET_OBJECT_V1: u8 = 5;
const ROLE_CARDS_V1: u8 = 6;
const ROLE_PENDING_SOURCES_V1: u8 = 9;

const MANA_COLORS_V1: [&str; 6] = ["W", "U", "B", "R", "G", "C"];
const CAST_MODES_V1: [&str; 2] = ["Normal", "Alternative"];
const COST_KINDS_V1: [&str; 11] = [
    "SacrificeLands",
    "SacrificePermanents",
    "SacrificeCreatures",
    "SacrificeArtifacts",
    "DiscardCards",
    "ExileFromGraveyard",
    "TapPermanents",
    "ReturnPermanentsToHand",
    "PayLife",
    "RemoveCounters",
    "PutCounters",
];
const OPTIONAL_COST_CHOICES_V1: [&str; 3] = ["Decline", "Discard", "SacrificeLand"];
const PHASE_NAMES_V2: [&str; 12] = [
    "untap",
    "upkeep",
    "draw",
    "main1",
    "begin_combat",
    "declare_attackers",
    "declare_blockers",
    "combat_damage",
    "end_combat",
    "main2",
    "end",
    "cleanup",
];
const ENGINE_STAGE_NAMES_V2: [&str; 10] = [
    "priority",
    "pending_cast",
    "pending_activation",
    "pending_discard",
    "pending_optional_cost",
    "pending_optional_cost_sacrifice",
    "pending_spell_copy",
    "pending_effect",
    "pending_triggers",
    "halted",
];
const SURFACE_STAGE_NAMES_V2: [&str; 5] = [
    "priority",
    "declare_blockers_for_attacker",
    "discard_pick",
    "optional_cost_use",
    "optional_cost_which",
];
const POLICY_STAGE_NAMES_V2: [&str; 3] = ["surface", "attacker_inclusion", "blocker_inclusion"];
const STACK_KIND_NAMES_V2: [&str; 4] = [
    "spell",
    "activated_ability",
    "triggered_ability",
    "madness_offer",
];
const CAST_METHOD_NAMES_V2: [&str; 8] = [
    "normal",
    "alternative",
    "flashback",
    "madness",
    "plotted",
    "escape",
    "bestow",
    "omen",
];
const EFFECT_DURATION_NAMES_V2: [&str; 4] = [
    "end_of_turn",
    "until_controllers_next_turn",
    "while_attached",
    "while_source_present",
];
const DISCARD_RESUME_NAMES_V2: [&str; 5] = [
    "none",
    "finish_cast",
    "finish_activation",
    "finish_spell_resolution",
    "finish_optional_cost",
];
const SPELL_COPY_STAGE_NAMES_V2: [&str; 3] = ["payment", "retarget", "target"];
const TARGET_PURPOSE_NAMES_V2: [&str; 8] = [
    "effect_targets",
    "card_selection",
    "permanent_selection",
    "player_selection",
    "damage_division",
    "cost_payment",
    "library_order",
    "search_result",
];
const BOOLEAN_PURPOSE_NAMES_V2: [&str; 3] = ["optional_effect", "shuffle", "pay_cost"];
const OBJECT_OUTPUT_GROUP_ORDER_V2: [FlatObjectGroupV2; 15] = [
    FlatObjectGroupV2::SelfHand,
    FlatObjectGroupV2::KnownSelfLibrary,
    FlatObjectGroupV2::KnownOpponentLibrary,
    FlatObjectGroupV2::KnownSelfHand,
    FlatObjectGroupV2::KnownOpponentHand,
    FlatObjectGroupV2::SelfBattlefield,
    FlatObjectGroupV2::OpponentBattlefield,
    FlatObjectGroupV2::SelfGraveyard,
    FlatObjectGroupV2::OpponentGraveyard,
    FlatObjectGroupV2::Exile,
    FlatObjectGroupV2::Stack,
    FlatObjectGroupV2::HistoricalStackTarget,
    FlatObjectGroupV2::HistoricalPaidCost,
    FlatObjectGroupV2::PendingContext,
    FlatObjectGroupV2::PrivateContext,
];

/// Owned row-major counterpart of Python `EncodedDecision`.
///
/// The integer vectors use `i64`, matching Torch `long`. All thirteen fields
/// are filled transactionally from one validated decision view.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeFlatDecisionTensorV2 {
    pub(crate) state: Vec<f32>,
    pub(crate) object_features: Vec<f32>,
    pub(crate) object_card_ids: Vec<i64>,
    pub(crate) object_groups: Vec<i64>,
    pub(crate) object_node_ids: Vec<i64>,
    pub(crate) edge_features: Vec<f32>,
    pub(crate) edge_source_indices: Vec<i64>,
    pub(crate) edge_target_indices: Vec<i64>,
    pub(crate) action_features: Vec<f32>,
    pub(crate) action_ref_features: Vec<f32>,
    pub(crate) action_ref_card_ids: Vec<i64>,
    pub(crate) action_ref_action_indices: Vec<i64>,
    pub(crate) action_ref_node_indices: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeFlatTensorErrorV2 {
    EmptyActionTable,
    ActingPlayerNotRelativeSelf,
    ActionReferenceRange,
    ActionReferenceShape,
    ActionReferenceObject,
    NonCanonicalActionCore,
    InvalidActionRange,
    InvalidTriggerOrder,
    CheckedIntegerRange,
    AllocationFailed,
    CanonicalJson,
    OutputInvariant,
    CardTokenRange,
    ObjectShape,
    ObjectOrder,
    ChildTableRange,
    ChildTableShape,
    RelationShape,
    RelationOrder,
    ContextShape,
    EnumRange,
    Poisoned,
}

impl fmt::Display for NativeFlatTensorErrorV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "native flat tensorization failed: {self:?}")
    }
}

impl std::error::Error for NativeFlatTensorErrorV2 {}

type NativeFlatTensorErrorV1 = NativeFlatTensorErrorV2;
type NativeFlatDecisionTensorV1 = NativeFlatDecisionTensorV2;

#[derive(Clone, Debug)]
struct ObjectProjectionV2 {
    raw_to_node: Vec<Option<usize>>,
    node_to_raw: Vec<usize>,
}

#[derive(Default)]
pub(crate) struct NativeFlatTensorizerV2 {
    poisoned: bool,
}

impl NativeFlatTensorizerV2 {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn fill(
        &mut self,
        decision: FlatScoringDecisionViewV1<'_>,
        output: &mut NativeFlatDecisionTensorV2,
    ) -> Result<(), NativeFlatTensorErrorV2> {
        if self.poisoned {
            return Err(NativeFlatTensorErrorV2::Poisoned);
        }
        match encode_full_decision_v2(decision) {
            Ok(encoded) => {
                *output = encoded;
                Ok(())
            }
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }

    pub(crate) fn is_poisoned(&self) -> bool {
        self.poisoned
    }
}

#[derive(Clone, Copy)]
struct ResolvedActionRefV1<'a> {
    raw: &'a FlatScorerActionRefV1,
    object: &'a FlatObjectCoreV1,
}

#[derive(Clone, Copy)]
struct ProjectedActionRefV1<'a> {
    resolved: ResolvedActionRefV1<'a>,
    role: u8,
    order_index: u16,
    associated_order: u16,
}

struct EncodedActionV1 {
    canonical_json: Vec<u8>,
    sha512_blocks: [[u8; ACTION_HASH_BLOCK_BYTES_V1]; ACTION_HASH_BLOCK_COUNT_V1],
    features: [f32; NATIVE_FLAT_ACTION_FEATURE_DIM_V1],
    ref_features: Vec<[f32; NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1]>,
    ref_card_ids: Vec<i64>,
    ref_node_indices: Vec<i64>,
}

struct ActionHalfV1 {
    action_features: Vec<f32>,
    action_ref_features: Vec<f32>,
    action_ref_card_ids: Vec<i64>,
    action_ref_action_indices: Vec<i64>,
    action_ref_node_indices: Vec<i64>,
}

/// Fills only the legal-action tensors. All validation and allocation happen
/// before any field in `output` is replaced.
fn fill_native_flat_action_tensors_v1(
    decision: FlatScoringDecisionViewV1<'_>,
    output: &mut NativeFlatDecisionTensorV1,
) -> Result<(), NativeFlatTensorErrorV1> {
    let encoded = encode_action_half_v1(decision)?;
    output.action_features = encoded.action_features;
    output.action_ref_features = encoded.action_ref_features;
    output.action_ref_card_ids = encoded.action_ref_card_ids;
    output.action_ref_action_indices = encoded.action_ref_action_indices;
    output.action_ref_node_indices = encoded.action_ref_node_indices;
    Ok(())
}

/// Fills the Python-parity action tensors from the explicit V2 scorer view.
/// Validation and allocation complete before any output field is replaced.
pub(crate) fn fill_native_flat_action_tensors_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    output: &mut NativeFlatDecisionTensorV2,
) -> Result<(), NativeFlatTensorErrorV2> {
    fill_native_flat_action_tensors_v1(decision, output)
}

fn validate_native_flat_action_half_v1(
    output: &NativeFlatDecisionTensorV1,
    action_count: usize,
    action_ref_count: usize,
    object_count: usize,
) -> Result<(), NativeFlatTensorErrorV1> {
    validate_native_flat_action_slices_v1(
        &output.action_features,
        &output.action_ref_features,
        &output.action_ref_card_ids,
        &output.action_ref_action_indices,
        &output.action_ref_node_indices,
        action_count,
        action_ref_count,
        object_count,
    )
}

pub(crate) fn validate_native_flat_action_half_v2(
    output: &NativeFlatDecisionTensorV2,
    action_count: usize,
    action_ref_count: usize,
    object_count: usize,
) -> Result<(), NativeFlatTensorErrorV2> {
    validate_native_flat_action_half_v1(output, action_count, action_ref_count, object_count)
}

struct ObjectHalfV2 {
    projection: ObjectProjectionV2,
    features: Vec<f32>,
    card_ids: Vec<i64>,
    groups: Vec<i64>,
    node_ids: Vec<i64>,
}

struct EdgeHalfV2 {
    features: Vec<f32>,
    sources: Vec<i64>,
    targets: Vec<i64>,
}

pub(crate) fn fill_native_flat_decision_tensors_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    output: &mut NativeFlatDecisionTensorV2,
) -> Result<(), NativeFlatTensorErrorV2> {
    let encoded = encode_full_decision_v2(decision)?;
    *output = encoded;
    Ok(())
}

fn encode_full_decision_v2(
    decision: FlatScoringDecisionViewV1<'_>,
) -> Result<NativeFlatDecisionTensorV2, NativeFlatTensorErrorV2> {
    if decision.globals().acting_player != FlatRelativePlayerV1::SelfPlayer {
        return Err(NativeFlatTensorErrorV2::ActingPlayerNotRelativeSelf);
    }
    validate_auxiliary_tables_v2(decision)?;
    let objects = encode_objects_v2(decision)?;
    let edges = encode_edges_v2(decision, &objects.projection)?;
    let canonical = canonical_observation_v2(decision, &objects.projection)?;
    let state = encode_state_v2(decision, &canonical)?;
    let actions = encode_action_half_with_projection_v2(decision, Some(&objects.projection))?;
    let output = NativeFlatDecisionTensorV2 {
        state,
        object_features: objects.features,
        object_card_ids: objects.card_ids,
        object_groups: objects.groups,
        object_node_ids: objects.node_ids,
        edge_features: edges.features,
        edge_source_indices: edges.sources,
        edge_target_indices: edges.targets,
        action_features: actions.action_features,
        action_ref_features: actions.action_ref_features,
        action_ref_card_ids: actions.action_ref_card_ids,
        action_ref_action_indices: actions.action_ref_action_indices,
        action_ref_node_indices: actions.action_ref_node_indices,
    };
    validate_full_output_v2(
        &output,
        objects.projection.node_to_raw.len(),
        decision.actions().len(),
        decision.action_refs().len(),
    )?;
    Ok(output)
}

fn object_output_group_rank_v2(group: FlatObjectGroupV2) -> Option<usize> {
    OBJECT_OUTPUT_GROUP_ORDER_V2
        .iter()
        .position(|candidate| *candidate == group)
}

fn is_synthetic_object_v2(object: &FlatObjectCoreV1) -> bool {
    object.card_token == 0
}

fn build_object_projection_v2(
    objects: &[FlatObjectCoreV1],
) -> Result<ObjectProjectionV2, NativeFlatTensorErrorV2> {
    let mut node_to_raw = objects
        .iter()
        .enumerate()
        .filter_map(|(raw, object)| (!is_synthetic_object_v2(object)).then_some(raw))
        .collect::<Vec<_>>();
    for &raw in &node_to_raw {
        let object = &objects[raw];
        if !(1..=NATIVE_FLAT_MAX_CARD_TOKEN_V2).contains(&object.card_token)
            || object.owner == FlatRelativePlayerV1::None
            || object.controller == FlatRelativePlayerV1::None
            || object.zone.is_none()
            || object_output_group_rank_v2(object.group).is_none()
        {
            return Err(NativeFlatTensorErrorV2::ObjectShape);
        }
    }
    node_to_raw.sort_by_key(|&raw| {
        let object = &objects[raw];
        (
            object_output_group_rank_v2(object.group).unwrap_or(usize::MAX),
            object.visible_ordinal,
            raw,
        )
    });
    for pair in node_to_raw.windows(2) {
        let left = &objects[pair[0]];
        let right = &objects[pair[1]];
        if left.group == right.group && left.visible_ordinal == right.visible_ordinal {
            return Err(NativeFlatTensorErrorV2::ObjectOrder);
        }
    }
    let mut raw_to_node = vec![None; objects.len()];
    for (node, &raw) in node_to_raw.iter().enumerate() {
        raw_to_node[raw] = Some(node);
    }
    Ok(ObjectProjectionV2 {
        raw_to_node,
        node_to_raw,
    })
}

fn encode_objects_v2(
    decision: FlatScoringDecisionViewV1<'_>,
) -> Result<ObjectHalfV2, NativeFlatTensorErrorV2> {
    let projection = build_object_projection_v2(decision.objects())?;
    let output_count = projection.node_to_raw.len().max(1);
    let mut features = try_vec_capacity(
        output_count
            .checked_mul(NATIVE_FLAT_OBJECT_FEATURE_DIM_V2)
            .ok_or(NativeFlatTensorErrorV2::CheckedIntegerRange)?,
    )?;
    let mut card_ids = try_vec_capacity(output_count)?;
    let mut groups = try_vec_capacity(output_count)?;
    let mut node_ids = try_vec_capacity(output_count)?;
    for (node, &raw) in projection.node_to_raw.iter().enumerate() {
        let object = &decision.objects()[raw];
        let row = object_features_v2(decision, raw, object)?;
        features.extend_from_slice(&row);
        card_ids.push(i64::from(object.card_token));
        groups.push(i64::from(object.group as u8));
        node_ids
            .push(i64::try_from(node).map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?);
    }
    if projection.node_to_raw.is_empty() {
        features.resize(NATIVE_FLAT_OBJECT_FEATURE_DIM_V2, 0.0);
        card_ids.push(0);
        groups.push(0);
        node_ids.push(0);
    }
    Ok(ObjectHalfV2 {
        projection,
        features,
        card_ids,
        groups,
        node_ids,
    })
}

fn relative_features_v2(player: FlatRelativePlayerV1) -> Result<[f32; 3], NativeFlatTensorErrorV2> {
    match player {
        FlatRelativePlayerV1::SelfPlayer => Ok([1.0, 0.0, 0.0]),
        FlatRelativePlayerV1::Opponent => Ok([0.0, 1.0, 0.0]),
        FlatRelativePlayerV1::None => Ok([0.0, 0.0, 1.0]),
    }
}

fn required_relative_features_v2(
    player: FlatRelativePlayerV1,
) -> Result<[f32; 3], NativeFlatTensorErrorV2> {
    if player == FlatRelativePlayerV1::None {
        Err(NativeFlatTensorErrorV2::ObjectShape)
    } else {
        relative_features_v2(player)
    }
}

fn append_one_hot_v2(
    output: &mut Vec<f32>,
    index: usize,
    width: usize,
) -> Result<(), NativeFlatTensorErrorV2> {
    if index >= width {
        return Err(NativeFlatTensorErrorV2::EnumRange);
    }
    output.extend((0..width).map(|candidate| f32::from(candidate == index)));
    Ok(())
}

fn scaled_i64_v2(value: i64, scale: f64) -> f32 {
    (value as f64 / scale) as f32
}

fn scaled_u64_v2(value: u64, scale: f64) -> f32 {
    (value as f64 / scale) as f32
}

fn append_mask_v2(
    output: &mut Vec<f32>,
    value: u32,
    width: usize,
) -> Result<(), NativeFlatTensorErrorV2> {
    if width >= 32 || value >= (1_u32 << width) {
        return Err(NativeFlatTensorErrorV2::ObjectShape);
    }
    output.extend((0..width).map(|bit| f32::from(value & (1_u32 << bit) != 0)));
    Ok(())
}

fn turn_relation_features_v2(relation: FlatTurnRelationV2, output: &mut Vec<f32>) {
    output.extend((0..3).map(|index| f32::from(index == relation as usize)));
}

fn checked_table_slice_v2<T>(
    table: &[T],
    start: u32,
    count: u32,
) -> Result<&[T], NativeFlatTensorErrorV2> {
    let start = usize::try_from(start).map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
    let count = usize::try_from(count).map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
    let end = start
        .checked_add(count)
        .ok_or(NativeFlatTensorErrorV2::CheckedIntegerRange)?;
    table
        .get(start..end)
        .ok_or(NativeFlatTensorErrorV2::ChildTableRange)
}

fn object_subtypes_v2<'a>(
    decision: FlatScoringDecisionViewV1<'a>,
    raw: usize,
    object: &FlatObjectCoreV1,
) -> Result<&'a [FlatObjectSubtypeV2], NativeFlatTensorErrorV2> {
    let rows = checked_table_slice_v2(
        decision.object_subtypes(),
        object.subtype_start,
        object.subtype_count,
    )?;
    validate_owned_ordered_rows_v2(rows, raw, |row| row.object_index, |row| row.order)?;
    Ok(rows)
}

fn object_ability_uses_v2<'a>(
    decision: FlatScoringDecisionViewV1<'a>,
    raw: usize,
    object: &FlatObjectCoreV1,
) -> Result<&'a [FlatObjectAbilityUseV2], NativeFlatTensorErrorV2> {
    let rows = checked_table_slice_v2(
        decision.ability_uses(),
        object.ability_use_start,
        object.ability_use_count,
    )?;
    validate_owned_ordered_rows_v2(rows, raw, |row| row.object_index, |row| row.order)?;
    Ok(rows)
}

fn object_goads_v2<'a>(
    decision: FlatScoringDecisionViewV1<'a>,
    raw: usize,
    object: &FlatObjectCoreV1,
) -> Result<&'a [FlatObjectGoadV2], NativeFlatTensorErrorV2> {
    let rows = checked_table_slice_v2(decision.goads(), object.goad_start, object.goad_count)?;
    validate_owned_ordered_rows_v2(rows, raw, |row| row.object_index, |row| row.order)?;
    Ok(rows)
}

fn validate_owned_ordered_rows_v2<T>(
    rows: &[T],
    raw: usize,
    owner: impl Fn(&T) -> u32,
    order: impl Fn(&T) -> u32,
) -> Result<(), NativeFlatTensorErrorV2> {
    for (index, row) in rows.iter().enumerate() {
        if usize::try_from(owner(row)).ok() != Some(raw)
            || usize::try_from(order(row)).ok() != Some(index)
        {
            return Err(NativeFlatTensorErrorV2::ChildTableShape);
        }
    }
    Ok(())
}

fn attachment_count_for_object_v2(
    relations: &[FlatRelationV2],
    raw: usize,
) -> Result<usize, NativeFlatTensorErrorV2> {
    let raw = u32::try_from(raw).map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
    let mut count = 0usize;
    for pair in relation_pairs_v2(relations, FlatRelationRoleV2::Attachment)? {
        let host = pair
            .iter()
            .find(|relation| relation.associated_order == 0)
            .ok_or(NativeFlatTensorErrorV2::RelationShape)?;
        if host.target_object == Some(raw) {
            count = count
                .checked_add(1)
                .ok_or(NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        }
    }
    Ok(count)
}

fn relation_pairs_v2(
    relations: &[FlatRelationV2],
    role: FlatRelationRoleV2,
) -> Result<Vec<[&FlatRelationV2; 2]>, NativeFlatTensorErrorV2> {
    let selected = relations
        .iter()
        .filter(|relation| relation.role == role)
        .collect::<Vec<_>>();
    if selected.len() % 2 != 0 {
        return Err(NativeFlatTensorErrorV2::RelationShape);
    }
    let mut pairs = Vec::with_capacity(selected.len() / 2);
    for chunk in selected.chunks_exact(2) {
        if chunk[0].source_object != chunk[1].source_object
            || chunk[0].primary_order != chunk[1].primary_order
            || chunk[0].secondary_order != chunk[1].secondary_order
            || chunk[0].associated_order == chunk[1].associated_order
            || !matches!(chunk[0].payload, FlatRelationPayloadV2::None)
            || !matches!(chunk[1].payload, FlatRelationPayloadV2::None)
        {
            return Err(NativeFlatTensorErrorV2::RelationShape);
        }
        pairs.push([chunk[0], chunk[1]]);
    }
    Ok(pairs)
}

fn object_features_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    raw: usize,
    object: &FlatObjectCoreV1,
) -> Result<[f32; NATIVE_FLAT_OBJECT_FEATURE_DIM_V2], NativeFlatTensorErrorV2> {
    let mut row = Vec::with_capacity(NATIVE_FLAT_OBJECT_FEATURE_DIM_V2);
    row.extend(required_relative_features_v2(object.owner)?);
    row.extend(required_relative_features_v2(object.controller)?);
    let zone = object.zone.ok_or(NativeFlatTensorErrorV2::ObjectShape)? as usize;
    append_one_hot_v2(&mut row, zone, 7)?;
    let subtypes = object_subtypes_v2(decision, raw, object)?;
    let uses = object_ability_uses_v2(decision, raw, object)?;
    let goads = object_goads_v2(decision, raw, object)?;
    if !object.card_details_present
        && (object.tapped
            || object.summoning_sick
            || object.damage != 0
            || object.counters != [0; 5]
            || object.plotted_turn != FlatTurnRelationV2::Absent
            || object.is_token
            || object.face_index != 0
            || object.chosen_color.is_some()
            || object.entered_battlefield_turn != FlatTurnRelationV2::Absent
            || object.skip_next_untap
            || object.type_flags != [false; 6]
            || object.base_power.is_some()
            || object.base_toughness.is_some()
            || object.effective_power.is_some()
            || object.effective_toughness.is_some()
            || object.effective_color_mask != 0
            || object.keyword_flags != [false; 14]
            || object.ward_generic != 0
            || object.minimum_blockers != 0
            || object.landwalk_mask != 0
            || !subtypes.is_empty()
            || !uses.is_empty()
            || !goads.is_empty())
    {
        return Err(NativeFlatTensorErrorV2::ObjectShape);
    }
    row.push(f32::from(object.card_details_present && object.tapped));
    row.push(f32::from(
        object.card_details_present && object.summoning_sick,
    ));
    row.push(scaled_i64_v2(
        if object.card_details_present {
            i64::from(object.damage)
        } else {
            0
        },
        20.0,
    ));
    for counter in object.counters {
        row.push(scaled_i64_v2(
            if object.card_details_present {
                i64::from(counter)
            } else {
                0
            },
            10.0,
        ));
    }
    let attachment_count = if object.card_details_present {
        attachment_count_for_object_v2(decision.relations(), raw)?
    } else {
        0
    };
    row.push(scaled_u64_v2(
        u64::try_from(attachment_count)
            .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?,
        8.0,
    ));
    turn_relation_features_v2(object.plotted_turn, &mut row);
    row.push(f32::from(object.card_details_present && object.is_token));
    row.push(scaled_i64_v2(
        if object.card_details_present {
            i64::from(object.face_index)
        } else {
            0
        },
        8.0,
    ));
    row.push(f32::from(
        object.card_details_present && object.chosen_color.is_some(),
    ));
    if let Some(color) = object.chosen_color.filter(|_| object.card_details_present) {
        append_one_hot_v2(&mut row, color as usize, 6)?;
    } else {
        row.extend([0.0; 6]);
    }
    turn_relation_features_v2(object.entered_battlefield_turn, &mut row);
    row.push(scaled_u64_v2(uses.len() as u64, 8.0));
    row.push(scaled_u64_v2(
        uses.iter().map(|entry| u64::from(entry.uses)).sum(),
        16.0,
    ));
    row.push(scaled_u64_v2(
        u64::from(
            uses.iter()
                .map(|entry| entry.ability_index)
                .max()
                .unwrap_or(0),
        ),
        16.0,
    ));
    for kind in 0..2 {
        if uses.iter().any(|entry| entry.ability_kind > 1) {
            return Err(NativeFlatTensorErrorV2::EnumRange);
        }
        row.push(scaled_u64_v2(
            uses.iter()
                .filter(|entry| usize::from(entry.ability_kind) == kind)
                .map(|entry| u64::from(entry.uses))
                .sum(),
            16.0,
        ));
    }
    row.push(f32::from(
        object.card_details_present && object.skip_next_untap,
    ));
    row.push(scaled_u64_v2(goads.len() as u64, 2.0));
    row.push(f32::from(
        goads
            .iter()
            .any(|entry| entry.player == FlatRelativePlayerV1::SelfPlayer),
    ));
    row.push(f32::from(
        goads
            .iter()
            .any(|entry| entry.player == FlatRelativePlayerV1::Opponent),
    ));
    if goads
        .iter()
        .any(|entry| entry.player == FlatRelativePlayerV1::None)
    {
        return Err(NativeFlatTensorErrorV2::ObjectShape);
    }
    row.extend(object.type_flags.into_iter().map(f32::from));
    for value in [
        object.base_power,
        object.base_toughness,
        object.effective_power,
        object.effective_toughness,
    ] {
        row.push(scaled_i64_v2(i64::from(value.unwrap_or(0)), 20.0));
    }
    append_mask_v2(&mut row, u32::from(object.effective_color_mask), 6)?;
    row.push(scaled_u64_v2(subtypes.len() as u64, 16.0));
    row.extend(object.keyword_flags.into_iter().map(f32::from));
    row.push(scaled_i64_v2(i64::from(object.ward_generic), 16.0));
    row.push(scaled_i64_v2(i64::from(object.minimum_blockers), 8.0));
    append_mask_v2(&mut row, u32::from(object.landwalk_mask), 6)?;
    append_one_hot_v2(&mut row, object.source_kind as usize, 12)?;
    row.push(scaled_i64_v2(i64::from(object.visible_ordinal), 64.0));
    let row: [f32; NATIVE_FLAT_OBJECT_FEATURE_DIM_V2] = row
        .try_into()
        .map_err(|_| NativeFlatTensorErrorV2::OutputInvariant)?;
    Ok(row)
}

fn validate_auxiliary_tables_v2(
    decision: FlatScoringDecisionViewV1<'_>,
) -> Result<(), NativeFlatTensorErrorV2> {
    let mut subtype_used = vec![false; decision.object_subtypes().len()];
    let mut ability_used = vec![false; decision.ability_uses().len()];
    let mut goad_used = vec![false; decision.goads().len()];
    for (raw, object) in decision.objects().iter().enumerate() {
        mark_child_slice_v2(
            &mut subtype_used,
            object.subtype_start,
            object.subtype_count,
        )?;
        mark_child_slice_v2(
            &mut ability_used,
            object.ability_use_start,
            object.ability_use_count,
        )?;
        mark_child_slice_v2(&mut goad_used, object.goad_start, object.goad_count)?;
        let _ = object_subtypes_v2(decision, raw, object)?;
        let _ = object_ability_uses_v2(decision, raw, object)?;
        let _ = object_goads_v2(decision, raw, object)?;
    }
    if subtype_used.iter().any(|used| !used)
        || ability_used.iter().any(|used| !used)
        || goad_used.iter().any(|used| !used)
    {
        return Err(NativeFlatTensorErrorV2::ChildTableShape);
    }
    validate_completed_dungeons_v2(decision)?;
    validate_effect_subtypes_v2(decision)?;
    validate_context_elements_v2(decision)?;
    Ok(())
}

fn mark_child_slice_v2(
    used: &mut [bool],
    start: u32,
    count: u32,
) -> Result<(), NativeFlatTensorErrorV2> {
    let start = usize::try_from(start).map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
    let count = usize::try_from(count).map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
    let end = start
        .checked_add(count)
        .ok_or(NativeFlatTensorErrorV2::CheckedIntegerRange)?;
    let slice = used
        .get_mut(start..end)
        .ok_or(NativeFlatTensorErrorV2::ChildTableRange)?;
    if slice.iter().any(|value| *value) {
        return Err(NativeFlatTensorErrorV2::ChildTableShape);
    }
    slice.fill(true);
    Ok(())
}

fn validate_completed_dungeons_v2(
    decision: FlatScoringDecisionViewV1<'_>,
) -> Result<(), NativeFlatTensorErrorV2> {
    let mut used = vec![false; decision.completed_dungeons().len()];
    for (player_index, player) in decision.globals().players.iter().enumerate() {
        mark_child_slice_v2(
            &mut used,
            player.completed_dungeon_start,
            player.completed_dungeon_count,
        )?;
        let rows = checked_table_slice_v2(
            decision.completed_dungeons(),
            player.completed_dungeon_start,
            player.completed_dungeon_count,
        )?;
        for (order, row) in rows.iter().enumerate() {
            if row.player as usize != player_index || usize::try_from(row.order).ok() != Some(order)
            {
                return Err(NativeFlatTensorErrorV2::ChildTableShape);
            }
        }
    }
    if used.iter().any(|value| !value) {
        return Err(NativeFlatTensorErrorV2::ChildTableShape);
    }
    Ok(())
}

fn validate_effect_subtypes_v2(
    decision: FlatScoringDecisionViewV1<'_>,
) -> Result<(), NativeFlatTensorErrorV2> {
    let effect_count = decision
        .relations()
        .iter()
        .filter(|relation| relation.role == FlatRelationRoleV2::EffectSource)
        .count();
    let mut last = None;
    for row in decision.effect_subtype_changes() {
        let effect = usize::try_from(row.effect_order)
            .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        if effect >= effect_count {
            return Err(NativeFlatTensorErrorV2::ChildTableShape);
        }
        let key = (row.effect_order, row.kind as u8, row.order);
        if let Some(previous) = last {
            if key <= previous {
                return Err(NativeFlatTensorErrorV2::ChildTableShape);
            }
            if key.0 == previous.0 && key.1 == previous.1 && key.2 != previous.2 + 1 {
                return Err(NativeFlatTensorErrorV2::ChildTableShape);
            }
        }
        last = Some(key);
    }
    Ok(())
}

fn validate_context_elements_v2(
    decision: FlatScoringDecisionViewV1<'_>,
) -> Result<(), NativeFlatTensorErrorV2> {
    let mut last = None;
    for row in decision.context_path_elements() {
        let key = (
            row.context as u8,
            row.context_order,
            row.kind as u8,
            row.order,
        );
        if let Some(previous) = last {
            if key <= previous {
                return Err(NativeFlatTensorErrorV2::ChildTableShape);
            }
            if key.0 == previous.0
                && key.1 == previous.1
                && key.2 == previous.2
                && key.3 != previous.3 + 1
            {
                return Err(NativeFlatTensorErrorV2::ChildTableShape);
            }
        }
        last = Some(key);
    }
    Ok(())
}

fn projected_required_node_v2(
    raw: Option<u32>,
    projection: &ObjectProjectionV2,
) -> Result<usize, NativeFlatTensorErrorV2> {
    projected_node_index_v2(
        raw.ok_or(NativeFlatTensorErrorV2::RelationShape)?,
        Some(projection),
    )
    .map_err(|_| NativeFlatTensorErrorV2::RelationShape)
}

fn edge_row_v2(
    role: FlatRelationRoleV2,
    primary: u32,
    secondary: u32,
    associated: u32,
    extra: &[f32],
) -> Result<[f32; NATIVE_FLAT_EDGE_FEATURE_DIM_V2], NativeFlatTensorErrorV2> {
    if extra.len() > 24 {
        return Err(NativeFlatTensorErrorV2::OutputInvariant);
    }
    let mut row = Vec::with_capacity(NATIVE_FLAT_EDGE_FEATURE_DIM_V2);
    append_one_hot_v2(&mut row, role as usize, 14)?;
    row.push(scaled_i64_v2(i64::from(primary), 64.0));
    row.push(scaled_i64_v2(i64::from(secondary), 64.0));
    row.push(scaled_i64_v2(i64::from(associated), 64.0));
    row.extend_from_slice(extra);
    row.resize(NATIVE_FLAT_EDGE_FEATURE_DIM_V2, 0.0);
    row.try_into()
        .map_err(|_| NativeFlatTensorErrorV2::OutputInvariant)
}

#[allow(clippy::too_many_arguments)]
fn push_edge_v2(
    output: &mut EdgeHalfV2,
    source: usize,
    target: usize,
    role: FlatRelationRoleV2,
    primary: u32,
    secondary: u32,
    associated: u32,
    extra: &[f32],
) -> Result<(), NativeFlatTensorErrorV2> {
    output
        .features
        .extend_from_slice(&edge_row_v2(role, primary, secondary, associated, extra)?);
    output
        .sources
        .push(i64::try_from(source).map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?);
    output
        .targets
        .push(i64::try_from(target).map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?);
    Ok(())
}

fn encode_edges_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<EdgeHalfV2, NativeFlatTensorErrorV2> {
    let mut output = EdgeHalfV2 {
        features: Vec::new(),
        sources: Vec::new(),
        targets: Vec::new(),
    };
    for role in [
        FlatRelationRoleV2::KnownLibrary,
        FlatRelationRoleV2::KnownHand,
    ] {
        for relation in decision
            .relations()
            .iter()
            .filter(|relation| relation.role == role)
        {
            let node = projected_required_node_v2(relation.source_object, projection)?;
            if projected_required_node_v2(relation.target_object, projection)? != node
                || relation.associated_order != 0
            {
                return Err(NativeFlatTensorErrorV2::RelationShape);
            }
            let owner = match relation.payload {
                FlatRelationPayloadV2::Known { owner } => owner,
                _ => return Err(NativeFlatTensorErrorV2::RelationShape),
            };
            if usize::from(owner as u8)
                != usize::try_from(relation.primary_order).unwrap_or(usize::MAX)
                || owner == FlatRelativePlayerV1::None
            {
                return Err(NativeFlatTensorErrorV2::RelationShape);
            }
            push_edge_v2(
                &mut output,
                node,
                node,
                role,
                relation.primary_order,
                relation.secondary_order,
                0,
                &required_relative_features_v2(owner)?,
            )?;
        }
    }
    for pair in relation_pairs_v2(decision.relations(), FlatRelationRoleV2::Attachment)? {
        let host = pair
            .iter()
            .find(|relation| relation.associated_order == 0)
            .ok_or(NativeFlatTensorErrorV2::RelationShape)?;
        let attachment = pair
            .iter()
            .find(|relation| relation.associated_order == 1)
            .ok_or(NativeFlatTensorErrorV2::RelationShape)?;
        let source_raw = host
            .source_object
            .ok_or(NativeFlatTensorErrorV2::RelationShape)?;
        let source_index = usize::try_from(source_raw)
            .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        if projection
            .raw_to_node
            .get(source_index)
            .copied()
            .flatten()
            .is_some()
        {
            return Err(NativeFlatTensorErrorV2::RelationShape);
        }
        push_edge_v2(
            &mut output,
            projected_required_node_v2(host.target_object, projection)?,
            projected_required_node_v2(attachment.target_object, projection)?,
            FlatRelationRoleV2::Attachment,
            host.primary_order,
            0,
            0,
            &[],
        )?;
    }
    for relation in decision.relations().iter().filter(|relation| {
        matches!(
            relation.role,
            FlatRelationRoleV2::AttachedTo | FlatRelationRoleV2::ExiledBy
        )
    }) {
        if !matches!(relation.payload, FlatRelationPayloadV2::None) {
            return Err(NativeFlatTensorErrorV2::RelationShape);
        }
        push_edge_v2(
            &mut output,
            projected_required_node_v2(relation.source_object, projection)?,
            projected_required_node_v2(relation.target_object, projection)?,
            relation.role,
            relation.primary_order,
            relation.secondary_order,
            relation.associated_order,
            &[],
        )?;
    }
    for relation in decision.relations().iter().filter(|relation| {
        matches!(
            relation.role,
            FlatRelationRoleV2::StackTarget | FlatRelationRoleV2::PaidCost
        )
    }) {
        match relation.role {
            FlatRelationRoleV2::PaidCost => {
                if !matches!(relation.payload, FlatRelationPayloadV2::None) {
                    return Err(NativeFlatTensorErrorV2::RelationShape);
                }
                push_edge_v2(
                    &mut output,
                    projected_required_node_v2(relation.source_object, projection)?,
                    projected_required_node_v2(relation.target_object, projection)?,
                    relation.role,
                    relation.primary_order,
                    relation.secondary_order,
                    relation.associated_order,
                    &[],
                )?;
            }
            FlatRelationRoleV2::StackTarget => {
                let payload = match relation.payload {
                    FlatRelationPayloadV2::Stack(payload) => payload,
                    _ => return Err(NativeFlatTensorErrorV2::RelationShape),
                };
                if relation.secondary_order == 0 {
                    if relation.source_object != relation.target_object
                        || payload.target_kind != FlatTargetKindV2::None
                    {
                        return Err(NativeFlatTensorErrorV2::RelationShape);
                    }
                } else if payload.target_kind == FlatTargetKindV2::Object {
                    push_edge_v2(
                        &mut output,
                        projected_required_node_v2(relation.source_object, projection)?,
                        projected_required_node_v2(relation.target_object, projection)?,
                        relation.role,
                        relation.primary_order,
                        relation.secondary_order - 1,
                        relation.associated_order,
                        &[],
                    )?;
                } else if payload.target_kind != FlatTargetKindV2::Player
                    || relation.target_object.is_some()
                {
                    return Err(NativeFlatTensorErrorV2::RelationShape);
                }
            }
            _ => unreachable!(),
        }
    }
    for relation in decision
        .relations()
        .iter()
        .filter(|relation| relation.role == FlatRelationRoleV2::CombatAttacker)
    {
        let blocked = match relation.payload {
            FlatRelationPayloadV2::CombatAttacker { blocked_order } => blocked_order,
            _ => return Err(NativeFlatTensorErrorV2::RelationShape),
        };
        let node = projected_required_node_v2(relation.target_object, projection)?;
        push_edge_v2(
            &mut output,
            node,
            node,
            relation.role,
            relation.primary_order,
            0,
            0,
            &[f32::from(blocked.is_some())],
        )?;
    }
    for pair in relation_pairs_v2(decision.relations(), FlatRelationRoleV2::CombatBlocker)? {
        let attacker = pair
            .iter()
            .find(|relation| relation.associated_order == 0)
            .ok_or(NativeFlatTensorErrorV2::RelationShape)?;
        let blocker = pair
            .iter()
            .find(|relation| relation.associated_order == 1)
            .ok_or(NativeFlatTensorErrorV2::RelationShape)?;
        push_edge_v2(
            &mut output,
            projected_required_node_v2(attacker.target_object, projection)?,
            projected_required_node_v2(blocker.target_object, projection)?,
            FlatRelationRoleV2::CombatBlocker,
            attacker.primary_order,
            attacker.secondary_order,
            0,
            &[],
        )?;
    }
    encode_effect_edges_v2(decision, projection, &mut output)?;
    for relation in decision
        .relations()
        .iter()
        .filter(|relation| relation.role == FlatRelationRoleV2::Permission)
    {
        let payload = match relation.payload {
            FlatRelationPayloadV2::Permission(payload) => payload,
            _ => return Err(NativeFlatTensorErrorV2::RelationShape),
        };
        let node = projected_required_node_v2(relation.target_object, projection)?;
        let extra = permission_edge_extra_v2(payload)?;
        push_edge_v2(
            &mut output,
            node,
            node,
            relation.role,
            relation.primary_order,
            0,
            0,
            &extra,
        )?;
    }
    encode_context_edges_v2(
        decision,
        projection,
        FlatRelationRoleV2::PendingContext,
        &mut output,
    )?;
    encode_context_edges_v2(
        decision,
        projection,
        FlatRelationRoleV2::PrivateContext,
        &mut output,
    )?;
    Ok(output)
}

fn effect_payload_v2(
    relation: &FlatRelationV2,
) -> Result<FlatEffectRelationDataV2, NativeFlatTensorErrorV2> {
    match relation.payload {
        FlatRelationPayloadV2::Effect(payload) => Ok(payload),
        _ => Err(NativeFlatTensorErrorV2::RelationShape),
    }
}

fn effect_baseline_v2(
    relations: &[FlatRelationV2],
    order: u32,
) -> Result<&FlatRelationV2, NativeFlatTensorErrorV2> {
    let mut matches = relations.iter().filter(|relation| {
        relation.role == FlatRelationRoleV2::EffectSource && relation.primary_order == order
    });
    let value = matches
        .next()
        .ok_or(NativeFlatTensorErrorV2::RelationShape)?;
    if matches.next().is_some() {
        return Err(NativeFlatTensorErrorV2::RelationShape);
    }
    Ok(value)
}

fn effect_edge_extra_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    order: u32,
) -> Result<Vec<f32>, NativeFlatTensorErrorV2> {
    let baseline = effect_baseline_v2(decision.relations(), order)?;
    let payload = effect_payload_v2(baseline)?;
    let mut affected_self = false;
    let mut affected_opponent = false;
    for relation in decision.relations().iter().filter(|relation| {
        relation.role == FlatRelationRoleV2::EffectAffected
            && relation.primary_order == order
            && relation.associated_order == 1
    }) {
        let affected = effect_payload_v2(relation)?.affected_player;
        match affected {
            FlatRelativePlayerV1::SelfPlayer => affected_self = true,
            FlatRelativePlayerV1::Opponent => affected_opponent = true,
            FlatRelativePlayerV1::None => return Err(NativeFlatTensorErrorV2::RelationShape),
        }
    }
    let mut extra = Vec::with_capacity(24);
    extra.push(scaled_i64_v2(i64::from(payload.layers), 16.0));
    extra.push(scaled_i64_v2(i64::from(payload.power_delta), 20.0));
    extra.push(scaled_i64_v2(i64::from(payload.toughness_delta), 20.0));
    extra.push(f32::from(payload.grants_haste));
    append_one_hot_v2(&mut extra, payload.duration as usize, 4)?;
    extra.push(f32::from(payload.global));
    extra.extend(relative_features_v2(payload.controller)?);
    extra.push(f32::from(affected_self));
    extra.push(f32::from(affected_opponent));
    extra.push(f32::from(payload.set_power.is_some()));
    extra.push(scaled_i64_v2(
        i64::from(payload.set_power.unwrap_or(0)),
        20.0,
    ));
    extra.push(f32::from(payload.set_toughness.is_some()));
    extra.push(scaled_i64_v2(
        i64::from(payload.set_toughness.unwrap_or(0)),
        20.0,
    ));
    extra.push(scaled_i64_v2(i64::from(payload.add_color_mask), 63.0));
    extra.push(scaled_i64_v2(i64::from(payload.remove_color_mask), 63.0));
    extra.push(scaled_i64_v2(i64::from(payload.ward_generic_delta), 16.0));
    extra.push(scaled_i64_v2(
        i64::from(payload.minimum_blockers.unwrap_or(0)),
        8.0,
    ));
    extra.push(scaled_i64_v2(
        i64::from(payload.prevent_damage_from_color_mask),
        63.0,
    ));
    extra.push(f32::from(payload.damage_cannot_be_prevented));
    if extra.len() != 24 {
        return Err(NativeFlatTensorErrorV2::OutputInvariant);
    }
    Ok(extra)
}

fn encode_effect_edges_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    output: &mut EdgeHalfV2,
) -> Result<(), NativeFlatTensorErrorV2> {
    for baseline in decision
        .relations()
        .iter()
        .filter(|relation| relation.role == FlatRelationRoleV2::EffectSource)
    {
        let order = baseline.primary_order;
        let extra = effect_edge_extra_v2(decision, order)?;
        let source = baseline
            .target_object
            .map(|raw| projected_node_index_v2(raw, Some(projection)))
            .transpose()?;
        if let Some(source) = source {
            push_edge_v2(
                output,
                source,
                source,
                FlatRelationRoleV2::EffectSource,
                order,
                0,
                0,
                &extra,
            )?;
        }
        for relation in decision.relations().iter().filter(|relation| {
            relation.role == FlatRelationRoleV2::EffectAffected
                && relation.primary_order == order
                && relation.associated_order == 0
        }) {
            let target = projected_required_node_v2(relation.target_object, projection)?;
            push_edge_v2(
                output,
                source.unwrap_or(target),
                target,
                FlatRelationRoleV2::EffectAffected,
                order,
                relation.secondary_order,
                0,
                &extra,
            )?;
        }
    }
    Ok(())
}

fn permission_edge_extra_v2(
    payload: crate::flat_policy_v2::FlatPermissionRelationDataV2,
) -> Result<Vec<f32>, NativeFlatTensorErrorV2> {
    let mut extra = Vec::with_capacity(8);
    extra.extend(required_relative_features_v2(payload.holder)?);
    append_one_hot_v2(&mut extra, usize::from(payload.play_or_cast), 2)?;
    append_one_hot_v2(&mut extra, usize::from(payload.expiry), 2)?;
    extra.push(f32::from(payload.holder_turn_started));
    Ok(extra)
}

fn encode_context_edges_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    role: FlatRelationRoleV2,
    output: &mut EdgeHalfV2,
) -> Result<(), NativeFlatTensorErrorV2> {
    let mut edge_order = 0_u32;
    for relation in decision
        .relations()
        .iter()
        .filter(|relation| relation.role == role)
    {
        let payload = match relation.payload {
            FlatRelationPayloadV2::Context(payload) => payload,
            _ => return Err(NativeFlatTensorErrorV2::RelationShape),
        };
        let Some(raw_target) = relation.target_object else {
            if payload.target_kind != FlatTargetKindV2::Player
                && !(payload.context == FlatContextKindV2::PendingTrigger
                    && payload.subrole == FlatContextSubroleV2::PendingTriggerSource)
            {
                return Err(NativeFlatTensorErrorV2::RelationShape);
            }
            continue;
        };
        let node = projected_node_index_v2(raw_target, Some(projection))?;
        push_edge_v2(
            output,
            node,
            node,
            role,
            edge_order,
            u32::from(payload.subrole as u8),
            0,
            &[],
        )?;
        edge_order = edge_order
            .checked_add(1)
            .ok_or(NativeFlatTensorErrorV2::CheckedIntegerRange)?;
    }
    Ok(())
}

fn encode_state_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    canonical: &Value,
) -> Result<Vec<f32>, NativeFlatTensorErrorV2> {
    let globals = decision.globals();
    let mut state = Vec::with_capacity(NATIVE_FLAT_STATE_FEATURE_DIM_V2);
    append_one_hot_v2(&mut state, globals.phase as usize, 12)?;
    state.extend(relative_features_v2(globals.active_player)?);
    state.extend(relative_features_v2(globals.priority_player)?);
    state.extend(relative_features_v2(globals.initiative)?);
    for player in globals.players {
        state.push(scaled_i64_v2(i64::from(player.life), 20.0));
        state.extend(
            player
                .mana
                .into_iter()
                .map(|value| scaled_i64_v2(i64::from(value), 10.0)),
        );
        state.push(scaled_u64_v2(player.hand_count, 16.0));
        state.push(scaled_u64_v2(player.library_count, 64.0));
        state.push(f32::from(player.has_lost));
        state.push(scaled_i64_v2(i64::from(player.lands_played_this_turn), 4.0));
        state.push(f32::from(player.drew_from_empty));
        state.push(scaled_i64_v2(i64::from(player.draws_this_turn), 8.0));
        state.push(scaled_i64_v2(i64::from(player.spells_cast_this_turn), 8.0));
        state.push(f32::from(player.dungeon_id.is_some()));
        state.push(scaled_i64_v2(
            i64::from(player.dungeon_id.unwrap_or(0)),
            32.0,
        ));
        state.push(f32::from(player.room_id.is_some()));
        state.push(scaled_i64_v2(i64::from(player.room_id.unwrap_or(0)), 32.0));
        state.push(scaled_i64_v2(
            i64::from(player.completed_dungeon_count),
            8.0,
        ));
    }
    let attacker_count = count_role_v2(decision.relations(), FlatRelationRoleV2::CombatAttacker);
    let blocker_relation_count =
        count_role_v2(decision.relations(), FlatRelationRoleV2::CombatBlocker);
    if !blocker_relation_count.is_multiple_of(2) {
        return Err(NativeFlatTensorErrorV2::RelationShape);
    }
    state.push(f32::from(globals.attackers_declared));
    state.push(f32::from(globals.blockers_declared));
    state.push(scaled_u64_v2(attacker_count as u64, 16.0));
    state.push(scaled_u64_v2((blocker_relation_count / 2) as u64, 32.0));
    state.push(scaled_u64_v2(
        decision
            .relations()
            .iter()
            .filter(|relation| {
                relation.role == FlatRelationRoleV2::StackTarget && relation.secondary_order == 0
            })
            .count() as u64,
        32.0,
    ));
    state.push(scaled_u64_v2(
        count_role_v2(decision.relations(), FlatRelationRoleV2::EffectSource) as u64,
        32.0,
    ));
    state.push(scaled_u64_v2(
        count_role_v2(decision.relations(), FlatRelationRoleV2::Permission) as u64,
        32.0,
    ));
    state.push(scaled_u64_v2(
        count_role_v2(decision.relations(), FlatRelationRoleV2::AttachedTo) as u64,
        32.0,
    ));
    state.push(scaled_u64_v2(
        count_role_v2(decision.relations(), FlatRelationRoleV2::ExiledBy) as u64,
        32.0,
    ));
    for role in [
        FlatRelationRoleV2::KnownLibrary,
        FlatRelationRoleV2::KnownHand,
    ] {
        for owner in [
            FlatRelativePlayerV1::SelfPlayer,
            FlatRelativePlayerV1::Opponent,
        ] {
            state.push(scaled_u64_v2(
                decision
                    .relations()
                    .iter()
                    .filter(|relation| {
                        relation.role == role
                            && matches!(
                                relation.payload,
                                FlatRelationPayloadV2::Known { owner: candidate }
                                    if candidate == owner
                            )
                    })
                    .count() as u64,
                16.0,
            ));
        }
    }
    let engine = globals.engine;
    state.extend(engine.priority_passes.into_iter().map(f32::from));
    state.push(f32::from(engine.stack_nonempty));
    state.push(f32::from(engine.stack_activity_since_priority_boundary));
    state.push(f32::from(engine.mana_activity_since_priority_boundary));
    state.extend(relative_features_v2(engine.last_mana_ability_activator)?);
    append_one_hot_v2(&mut state, usize::from(engine.current_stage), 10)?;
    state.extend(
        [
            engine.pending_cast.is_some(),
            engine.pending_activation.is_some(),
            engine.pending_discard.is_some(),
            engine.pending_optional_cost.is_some(),
            engine.pending_optional_sacrifice.is_some(),
            engine.pending_spell_copy.is_some(),
            engine.pending_effect.is_some(),
        ]
        .into_iter()
        .map(f32::from),
    );
    state.push(scaled_i64_v2(i64::from(engine.pending_trigger_count), 16.0));
    let surface = globals.surface;
    append_one_hot_v2(&mut state, usize::from(surface.current_stage), 5)?;
    state.extend(surface.combat_priority_spent.into_iter().map(f32::from));
    state.push(f32::from(surface.combat_priority_rearmed_by_stack_activity));
    state.push(f32::from(surface.combat_priority_rearmed_by_mana_activity));
    state.push(f32::from(surface.stack_grew_since_round_open));
    state.push(f32::from(surface.mana_activity_since_round_open));
    state.push(f32::from(
        surface.stack_length_changed_since_observed.unwrap_or(false),
    ));
    state.push(f32::from(surface.mana_activity_since_last_stack_change));
    state.push(f32::from(surface.madness_cast_reprompt_source_present));
    state.push(f32::from(surface.private_blockers_present));
    state.push(f32::from(
        surface.private_discard_remaining_needed.is_some(),
    ));
    state.push(f32::from(surface.private_optional_stage.is_some()));
    let policy = globals.policy_surface;
    append_one_hot_v2(&mut state, usize::from(policy.current_stage), 3)?;
    state.push(f32::from(policy.private_combat_present));
    state.push(scaled_i64_v2(i64::from(policy.candidate_index), 32.0));
    state.push(scaled_i64_v2(i64::from(policy.candidate_count), 32.0));
    state.push(scaled_i64_v2(i64::from(policy.selected_count), 32.0));
    state.push(scaled_i64_v2(i64::from(policy.remaining_count), 32.0));
    if state.len() != NATIVE_FLAT_STATE_FEATURE_DIM_V2 - NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V2 {
        return Err(NativeFlatTensorErrorV2::OutputInvariant);
    }
    let canonical_json =
        serde_json::to_vec(canonical).map_err(|_| NativeFlatTensorErrorV2::CanonicalJson)?;
    state.extend(digest_features_v2(
        b"observation-state",
        &canonical_json,
        NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V2,
    ));
    Ok(state)
}

fn count_role_v2(relations: &[FlatRelationV2], role: FlatRelationRoleV2) -> usize {
    relations
        .iter()
        .filter(|relation| relation.role == role)
        .count()
}

fn digest_features_v2(namespace: &[u8], canonical_json: &[u8], dims: usize) -> Vec<f32> {
    let mut output = Vec::with_capacity(dims);
    let mut counter = 0_u32;
    while output.len() < dims {
        let mut digest = Sha512::new();
        digest.update(namespace);
        digest.update(counter.to_le_bytes());
        digest.update(canonical_json);
        let block = digest.finalize();
        for chunk in block.chunks_exact(4) {
            let integer = u32::from_le_bytes(chunk.try_into().expect("four-byte hash chunk"));
            output.push(((f64::from(integer) / f64::from(u32::MAX)) * 2.0 - 1.0) as f32);
            if output.len() == dims {
                break;
            }
        }
        counter += 1;
    }
    output
}

fn canonical_observation_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let globals = decision.globals();
    let mut observation = Map::new();
    observation.insert("acting_player".to_owned(), Value::String("self".to_owned()));
    observation.insert(
        "known_hand_cards".to_owned(),
        canonical_known_cards_v2(decision, projection, FlatRelationRoleV2::KnownHand, false)?,
    );
    observation.insert(
        "known_library_cards".to_owned(),
        canonical_known_cards_v2(decision, projection, FlatRelationRoleV2::KnownLibrary, true)?,
    );
    observation.insert(
        "own_hand".to_owned(),
        Value::Array(
            group_objects_v2(decision, projection, FlatObjectGroupV2::SelfHand)?
                .into_iter()
                .map(|raw| {
                    Ok(object_value_v2([(
                        "stable",
                        canonical_stable_ref_v2(decision, raw, None)?,
                    )]))
                })
                .collect::<Result<Vec<_>, NativeFlatTensorErrorV2>>()?,
        ),
    );
    let mut projection_value = Map::new();
    projection_value.insert(
        "active_player".to_owned(),
        relative_player_value_v2(globals.active_player, false)?,
    );
    projection_value.insert(
        "battlefield".to_owned(),
        Value::Array(vec![
            canonical_public_group_v2(decision, projection, FlatObjectGroupV2::SelfBattlefield)?,
            canonical_public_group_v2(
                decision,
                projection,
                FlatObjectGroupV2::OpponentBattlefield,
            )?,
        ]),
    );
    projection_value.insert(
        "combat".to_owned(),
        canonical_combat_v2(decision, projection)?,
    );
    projection_value.insert(
        "continuous_effects".to_owned(),
        canonical_effects_v2(decision, projection)?,
    );
    projection_value.insert(
        "engine_context".to_owned(),
        canonical_engine_context_v2(decision, projection)?,
    );
    projection_value.insert(
        "exile".to_owned(),
        canonical_public_group_v2(decision, projection, FlatObjectGroupV2::Exile)?,
    );
    projection_value.insert(
        "exile_play_permissions".to_owned(),
        canonical_permissions_v2(decision, projection)?,
    );
    projection_value.insert(
        "graveyards".to_owned(),
        Value::Array(vec![
            canonical_public_group_v2(decision, projection, FlatObjectGroupV2::SelfGraveyard)?,
            canonical_public_group_v2(decision, projection, FlatObjectGroupV2::OpponentGraveyard)?,
        ]),
    );
    projection_value.insert(
        "hand_counts".to_owned(),
        Value::Array(
            globals
                .players
                .iter()
                .map(|player| Value::from(player.hand_count))
                .collect(),
        ),
    );
    projection_value.insert(
        "initiative".to_owned(),
        relative_player_value_v2(globals.initiative, true)?,
    );
    projection_value.insert(
        "library_counts".to_owned(),
        Value::Array(
            globals
                .players
                .iter()
                .map(|player| Value::from(player.library_count))
                .collect(),
        ),
    );
    projection_value.insert(
        "life_totals".to_owned(),
        Value::Array(
            globals
                .players
                .iter()
                .map(|player| Value::from(player.life))
                .collect(),
        ),
    );
    projection_value.insert(
        "mana_pools".to_owned(),
        Value::Array(
            globals
                .players
                .iter()
                .map(|player| {
                    Value::Array(player.mana.into_iter().map(Value::from).collect::<Vec<_>>())
                })
                .collect(),
        ),
    );
    projection_value.insert(
        "object_relations".to_owned(),
        canonical_object_relations_v2(decision, projection)?,
    );
    projection_value.insert(
        "phase".to_owned(),
        enum_value_v2(globals.phase as usize, &PHASE_NAMES_V2)?,
    );
    projection_value.insert(
        "player_status".to_owned(),
        canonical_player_status_v2(decision)?,
    );
    projection_value.insert(
        "policy_surface_context".to_owned(),
        canonical_policy_surface_v2(decision, projection)?,
    );
    projection_value.insert(
        "priority_player".to_owned(),
        relative_player_value_v2(globals.priority_player, false)?,
    );
    projection_value.insert(
        "stack".to_owned(),
        canonical_stack_v2(decision, projection)?,
    );
    projection_value.insert(
        "surface_context".to_owned(),
        canonical_surface_context_v2(decision, projection)?,
    );
    observation.insert("projection".to_owned(), Value::Object(projection_value));
    Ok(Value::Object(observation))
}

fn object_value_v2<const N: usize>(entries: [(&str, Value); N]) -> Value {
    let mut output = Map::new();
    for (key, value) in entries {
        output.insert(key.to_owned(), value);
    }
    Value::Object(output)
}

fn enum_value_v2<const N: usize>(
    index: usize,
    names: &[&str; N],
) -> Result<Value, NativeFlatTensorErrorV2> {
    names
        .get(index)
        .map(|name| Value::String((*name).to_owned()))
        .ok_or(NativeFlatTensorErrorV2::EnumRange)
}

fn relative_player_value_v2(
    player: FlatRelativePlayerV1,
    optional: bool,
) -> Result<Value, NativeFlatTensorErrorV2> {
    match player {
        FlatRelativePlayerV1::SelfPlayer => Ok(Value::String("self".to_owned())),
        FlatRelativePlayerV1::Opponent => Ok(Value::String("opponent".to_owned())),
        FlatRelativePlayerV1::None if optional => Ok(Value::Null),
        FlatRelativePlayerV1::None => Err(NativeFlatTensorErrorV2::ObjectShape),
    }
}

fn zone_value_v2(zone: FlatZoneV1) -> Value {
    Value::String(zone_name_v1(zone).to_owned())
}

fn color_value_v2(color: FlatManaColorV2) -> Result<Value, NativeFlatTensorErrorV2> {
    enum_value_v2(color as usize, &MANA_COLORS_V1)
}

fn canonical_stable_ref_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    raw: usize,
    controller_override: Option<FlatRelativePlayerV1>,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let object = decision
        .objects()
        .get(raw)
        .ok_or(NativeFlatTensorErrorV2::ObjectShape)?;
    if object.card_token == 0 || object.card_token > NATIVE_FLAT_MAX_CARD_TOKEN_V2 {
        return Err(NativeFlatTensorErrorV2::CardTokenRange);
    }
    Ok(object_value_v2([
        ("card_db_id", Value::from(object.card_token - 1)),
        (
            "controller",
            relative_player_value_v2(controller_override.unwrap_or(object.controller), false)?,
        ),
        ("owner", relative_player_value_v2(object.owner, false)?),
        (
            "zone",
            zone_value_v2(object.zone.ok_or(NativeFlatTensorErrorV2::ObjectShape)?),
        ),
    ]))
}

fn group_objects_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    group: FlatObjectGroupV2,
) -> Result<Vec<usize>, NativeFlatTensorErrorV2> {
    let mut rows = projection
        .node_to_raw
        .iter()
        .copied()
        .filter(|&raw| decision.objects()[raw].group == group)
        .collect::<Vec<_>>();
    rows.sort_by_key(|&raw| decision.objects()[raw].visible_ordinal);
    for (order, &raw) in rows.iter().enumerate() {
        if group != FlatObjectGroupV2::KnownSelfLibrary
            && group != FlatObjectGroupV2::KnownOpponentLibrary
            && usize::try_from(decision.objects()[raw].visible_ordinal).ok() != Some(order)
        {
            return Err(NativeFlatTensorErrorV2::ObjectOrder);
        }
    }
    Ok(rows)
}

fn canonical_public_group_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    group: FlatObjectGroupV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    Ok(Value::Array(
        group_objects_v2(decision, projection, group)?
            .into_iter()
            .map(|raw| canonical_public_card_v2(decision, raw))
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

fn canonical_public_card_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    raw: usize,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let object = &decision.objects()[raw];
    if !object.card_details_present {
        return Err(NativeFlatTensorErrorV2::ObjectShape);
    }
    let subtypes = object_subtypes_v2(decision, raw, object)?;
    let uses = object_ability_uses_v2(decision, raw, object)?;
    let goads = object_goads_v2(decision, raw, object)?;
    let mut goad_values = goads
        .iter()
        .map(|goad| {
            Ok(object_value_v2([
                ("expires_at_turn", Value::from(goad.expires_after_turns)),
                ("player", relative_player_value_v2(goad.player, false)?),
            ]))
        })
        .collect::<Result<Vec<_>, NativeFlatTensorErrorV2>>()?;
    sort_canonical_values_v2(&mut goad_values);
    let mut counters = Map::new();
    for (name, value) in [
        "plus1_plus1",
        "minus1_minus1",
        "minus0_minus1",
        "stun",
        "lore",
    ]
    .into_iter()
    .zip(object.counters)
    {
        counters.insert(name.to_owned(), Value::from(value));
    }
    let mut type_flags = Map::new();
    for (name, value) in [
        "land",
        "creature",
        "instant",
        "sorcery",
        "artifact",
        "enchantment",
    ]
    .into_iter()
    .zip(object.type_flags)
    {
        type_flags.insert(name.to_owned(), Value::Bool(value));
    }
    let mut keywords = Map::new();
    for (name, value) in [
        "flying",
        "reach",
        "haste",
        "vigilance",
        "trample",
        "first_strike",
        "double_strike",
        "deathtouch",
        "menace",
        "defender",
        "lifelink",
        "hexproof",
        "indestructible",
        "protection_from_monocolored",
    ]
    .into_iter()
    .zip(object.keyword_flags)
    {
        keywords.insert(name.to_owned(), Value::Bool(value));
    }
    keywords.insert(
        "landwalk_mask".to_owned(),
        Value::from(object.landwalk_mask),
    );
    keywords.insert(
        "minimum_blockers".to_owned(),
        Value::from(object.minimum_blockers),
    );
    keywords.insert("ward_generic".to_owned(), Value::from(object.ward_generic));
    let characteristics = object_value_v2([
        (
            "base_power",
            object.base_power.map_or(Value::Null, Value::from),
        ),
        (
            "base_toughness",
            object.base_toughness.map_or(Value::Null, Value::from),
        ),
        (
            "effective_color_mask",
            Value::from(object.effective_color_mask),
        ),
        ("effective_keywords", Value::Object(keywords)),
        (
            "effective_power",
            object.effective_power.map_or(Value::Null, Value::from),
        ),
        (
            "effective_subtype_ids",
            Value::Array(
                subtypes
                    .iter()
                    .map(|entry| Value::from(entry.subtype_id))
                    .collect(),
            ),
        ),
        (
            "effective_toughness",
            object.effective_toughness.map_or(Value::Null, Value::from),
        ),
        ("type_flags", Value::Object(type_flags)),
    ]);
    Ok(object_value_v2([
        (
            "ability_uses_this_turn",
            Value::Array(
                uses.iter()
                    .map(|entry| {
                        object_value_v2([
                            ("ability_index", Value::from(entry.ability_index)),
                            (
                                "ability_kind",
                                Value::String(
                                    if entry.ability_kind == 0 {
                                        "mana"
                                    } else {
                                        "activated"
                                    }
                                    .to_owned(),
                                ),
                            ),
                            ("uses", Value::from(entry.uses)),
                        ])
                    })
                    .collect(),
            ),
        ),
        // Python preserves this list container while omitting its
        // operational-only arena-id elements.
        ("attachments", Value::Array(Vec::new())),
        ("characteristics", characteristics),
        (
            "chosen_color",
            object
                .chosen_color
                .map(color_value_v2)
                .transpose()?
                .unwrap_or(Value::Null),
        ),
        ("counters", Value::Object(counters)),
        ("damage", Value::from(object.damage)),
        (
            "entered_battlefield_turn",
            turn_relation_value_v2(object.entered_battlefield_turn),
        ),
        ("face_index", Value::from(object.face_index)),
        ("goaded_by", Value::Array(goad_values)),
        ("is_token", Value::Bool(object.is_token)),
        ("plotted_turn", turn_relation_value_v2(object.plotted_turn)),
        ("skip_next_untap", Value::Bool(object.skip_next_untap)),
        ("stable", canonical_stable_ref_v2(decision, raw, None)?),
        ("summoning_sick", Value::Bool(object.summoning_sick)),
        ("tapped", Value::Bool(object.tapped)),
    ]))
}

fn turn_relation_value_v2(relation: FlatTurnRelationV2) -> Value {
    match relation {
        FlatTurnRelationV2::Absent => Value::Null,
        FlatTurnRelationV2::ThisTurn => Value::String("this_turn".to_owned()),
        FlatTurnRelationV2::EarlierTurn => Value::String("earlier_turn".to_owned()),
    }
}

fn sort_canonical_values_v2(values: &mut [Value]) {
    values.sort_by_key(canonical_value_bytes);
}

fn canonical_known_cards_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    role: FlatRelationRoleV2,
    include_position: bool,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let mut owners = vec![Vec::<(u32, Value)>::new(), Vec::<(u32, Value)>::new()];
    for relation in decision
        .relations()
        .iter()
        .filter(|relation| relation.role == role)
    {
        let owner = match relation.payload {
            FlatRelationPayloadV2::Known { owner } => owner,
            _ => return Err(NativeFlatTensorErrorV2::RelationShape),
        };
        let owner_index = match owner {
            FlatRelativePlayerV1::SelfPlayer => 0,
            FlatRelativePlayerV1::Opponent => 1,
            FlatRelativePlayerV1::None => return Err(NativeFlatTensorErrorV2::RelationShape),
        };
        if relation.primary_order != owner_index as u32 {
            return Err(NativeFlatTensorErrorV2::RelationOrder);
        }
        let raw = usize::try_from(
            relation
                .target_object
                .ok_or(NativeFlatTensorErrorV2::RelationShape)?,
        )
        .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        let _ = projected_required_node_v2(relation.target_object, projection)?;
        let private = object_value_v2([("stable", canonical_stable_ref_v2(decision, raw, None)?)]);
        let value = if include_position {
            object_value_v2([
                ("card", private),
                ("position", Value::from(relation.secondary_order)),
            ])
        } else {
            private
        };
        owners[owner_index].push((relation.secondary_order, value));
    }
    for rows in &mut owners {
        rows.sort_by_key(|(order, _)| *order);
        if !include_position {
            for (index, (order, _)) in rows.iter().enumerate() {
                if usize::try_from(*order).ok() != Some(index) {
                    return Err(NativeFlatTensorErrorV2::RelationOrder);
                }
            }
        }
    }
    let mut arrays = owners
        .into_iter()
        .map(|rows| rows.into_iter().map(|(_, value)| value).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    if !include_position {
        for values in &mut arrays {
            sort_canonical_values_v2(values);
        }
    }
    Ok(Value::Array(arrays.into_iter().map(Value::Array).collect()))
}

fn canonical_player_status_v2(
    decision: FlatScoringDecisionViewV1<'_>,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let mut players = Vec::with_capacity(2);
    for (player_index, player) in decision.globals().players.iter().enumerate() {
        let rows = checked_table_slice_v2(
            decision.completed_dungeons(),
            player.completed_dungeon_start,
            player.completed_dungeon_count,
        )?;
        let mut completed = rows
            .iter()
            .map(|row| Value::from(row.dungeon_id))
            .collect::<Vec<_>>();
        sort_canonical_values_v2(&mut completed);
        let dungeon = object_value_v2([
            ("completed_dungeons", Value::Array(completed)),
            (
                "dungeon_id",
                player.dungeon_id.map_or(Value::Null, Value::from),
            ),
            ("room_id", player.room_id.map_or(Value::Null, Value::from)),
        ]);
        if rows.iter().any(|row| row.player as usize != player_index) {
            return Err(NativeFlatTensorErrorV2::ChildTableShape);
        }
        players.push(object_value_v2([
            ("draws_this_turn", Value::from(player.draws_this_turn)),
            ("drew_from_empty", Value::Bool(player.drew_from_empty)),
            ("dungeon", dungeon),
            ("has_lost", Value::Bool(player.has_lost)),
            (
                "lands_played_this_turn",
                Value::from(player.lands_played_this_turn),
            ),
            (
                "spells_cast_this_turn",
                Value::from(player.spells_cast_this_turn),
            ),
        ]));
    }
    Ok(Value::Array(players))
}

fn canonical_object_relations_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let mut values = Vec::new();
    for relation in decision.relations().iter().filter(|relation| {
        matches!(
            relation.role,
            FlatRelationRoleV2::AttachedTo | FlatRelationRoleV2::ExiledBy
        )
    }) {
        let source = usize::try_from(
            relation
                .source_object
                .ok_or(NativeFlatTensorErrorV2::RelationShape)?,
        )
        .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        let target = usize::try_from(
            relation
                .target_object
                .ok_or(NativeFlatTensorErrorV2::RelationShape)?,
        )
        .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        let _ = projected_required_node_v2(relation.source_object, projection)?;
        let _ = projected_required_node_v2(relation.target_object, projection)?;
        let (kind, target_key) = match relation.role {
            FlatRelationRoleV2::AttachedTo => ("attached_to", "attached_to"),
            FlatRelationRoleV2::ExiledBy => ("exiled_by", "exiled_by"),
            _ => unreachable!(),
        };
        values.push(object_value_v2([
            (target_key, canonical_stable_ref_v2(decision, target, None)?),
            ("object", canonical_stable_ref_v2(decision, source, None)?),
            ("relation_kind", Value::String(kind.to_owned())),
        ]));
    }
    sort_canonical_values_v2(&mut values);
    Ok(Value::Array(values))
}

fn sorted_primary_relations_v2(
    relations: &[FlatRelationV2],
    role: FlatRelationRoleV2,
) -> Result<Vec<&FlatRelationV2>, NativeFlatTensorErrorV2> {
    let mut output = relations
        .iter()
        .filter(|relation| relation.role == role)
        .collect::<Vec<_>>();
    output.sort_by_key(|relation| {
        (
            relation.primary_order,
            relation.secondary_order,
            relation.associated_order,
        )
    });
    Ok(output)
}

fn canonical_stack_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let baselines = decision
        .relations()
        .iter()
        .filter(|relation| {
            relation.role == FlatRelationRoleV2::StackTarget && relation.secondary_order == 0
        })
        .collect::<Vec<_>>();
    let mut stack = Vec::with_capacity(baselines.len());
    for (stack_order, baseline) in baselines.iter().enumerate() {
        if usize::try_from(baseline.primary_order).ok() != Some(stack_order) {
            return Err(NativeFlatTensorErrorV2::RelationOrder);
        }
        let payload = match baseline.payload {
            FlatRelationPayloadV2::Stack(payload) => payload,
            _ => return Err(NativeFlatTensorErrorV2::RelationShape),
        };
        let source_raw = usize::try_from(
            baseline
                .source_object
                .ok_or(NativeFlatTensorErrorV2::RelationShape)?,
        )
        .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        let _ = projected_required_node_v2(baseline.source_object, projection)?;
        let mut target_rows = decision
            .relations()
            .iter()
            .filter(|relation| {
                relation.role == FlatRelationRoleV2::StackTarget
                    && relation.primary_order == baseline.primary_order
                    && relation.secondary_order > 0
            })
            .collect::<Vec<_>>();
        target_rows.sort_by_key(|relation| relation.secondary_order);
        let mut targets = Vec::with_capacity(target_rows.len());
        for (target_order, relation) in target_rows.iter().enumerate() {
            if usize::try_from(relation.secondary_order).ok() != Some(target_order + 1) {
                return Err(NativeFlatTensorErrorV2::RelationOrder);
            }
            let target_payload = match relation.payload {
                FlatRelationPayloadV2::Stack(value) => value,
                _ => return Err(NativeFlatTensorErrorV2::RelationShape),
            };
            if target_payload.controller != payload.controller
                || target_payload.stack_item_kind != payload.stack_item_kind
                || target_payload.is_copy != payload.is_copy
                || target_payload.is_flashback != payload.is_flashback
                || target_payload.mode_chosen != payload.mode_chosen
                || target_payload.madness_offer != payload.madness_offer
                || target_payload.kicked != payload.kicked
                || target_payload.cast_method != payload.cast_method
                || target_payload.face_index != payload.face_index
                || target_payload.x_value != payload.x_value
            {
                return Err(NativeFlatTensorErrorV2::RelationShape);
            }
            targets.push(match target_payload.target_kind {
                FlatTargetKindV2::Player => object_value_v2([
                    (
                        "player",
                        relative_player_value_v2(target_payload.target_player, false)?,
                    ),
                    ("target_kind", Value::String("player".to_owned())),
                ]),
                FlatTargetKindV2::Object => {
                    let raw = usize::try_from(
                        relation
                            .target_object
                            .ok_or(NativeFlatTensorErrorV2::RelationShape)?,
                    )
                    .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
                    object_value_v2([
                        (
                            "object",
                            canonical_stable_ref_v2(
                                decision,
                                raw,
                                Some(target_payload.target_object_controller),
                            )?,
                        ),
                        ("target_kind", Value::String("object".to_owned())),
                    ])
                }
                FlatTargetKindV2::None => return Err(NativeFlatTensorErrorV2::RelationShape),
            });
        }
        let mut paid_rows = decision
            .relations()
            .iter()
            .filter(|relation| {
                relation.role == FlatRelationRoleV2::PaidCost
                    && relation.primary_order == baseline.primary_order
            })
            .collect::<Vec<_>>();
        paid_rows.sort_by_key(|relation| relation.secondary_order);
        let mut paid = Vec::with_capacity(paid_rows.len());
        for (paid_order, relation) in paid_rows.iter().enumerate() {
            if usize::try_from(relation.secondary_order).ok() != Some(paid_order) {
                return Err(NativeFlatTensorErrorV2::RelationOrder);
            }
            let raw = usize::try_from(
                relation
                    .target_object
                    .ok_or(NativeFlatTensorErrorV2::RelationShape)?,
            )
            .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
            paid.push(canonical_stable_ref_v2(decision, raw, None)?);
        }
        stack.push(object_value_v2([
            (
                "cast_method",
                if payload.cast_method == 0 {
                    Value::Null
                } else {
                    enum_value_v2(usize::from(payload.cast_method - 1), &CAST_METHOD_NAMES_V2)?
                },
            ),
            (
                "controller",
                relative_player_value_v2(payload.controller, false)?,
            ),
            ("face_index", Value::from(payload.face_index)),
            ("is_copy", Value::Bool(payload.is_copy)),
            ("is_flashback", Value::Bool(payload.is_flashback)),
            ("kicked", Value::Bool(payload.kicked)),
            ("madness_offer", Value::Bool(payload.madness_offer)),
            ("mode_chosen", Value::from(payload.mode_chosen)),
            ("paid_cost_refs", Value::Array(paid)),
            (
                "source",
                canonical_stable_ref_v2(decision, source_raw, None)?,
            ),
            (
                "stack_item_kind",
                enum_value_v2(usize::from(payload.stack_item_kind), &STACK_KIND_NAMES_V2)?,
            ),
            ("targets", Value::Array(targets)),
            ("x_value", Value::from(payload.x_value)),
        ]));
    }
    Ok(Value::Array(stack))
}

fn canonical_combat_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let mut attackers = decision
        .relations()
        .iter()
        .filter(|relation| relation.role == FlatRelationRoleV2::CombatAttacker)
        .collect::<Vec<_>>();
    attackers.sort_by_key(|relation| relation.primary_order);
    let mut ordered_attackers = Vec::with_capacity(attackers.len());
    let mut blocked = Vec::<(u32, usize)>::new();
    for (order, relation) in attackers.iter().enumerate() {
        if usize::try_from(relation.primary_order).ok() != Some(order) {
            return Err(NativeFlatTensorErrorV2::RelationOrder);
        }
        let raw = usize::try_from(
            relation
                .target_object
                .ok_or(NativeFlatTensorErrorV2::RelationShape)?,
        )
        .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        let _ = projected_required_node_v2(relation.target_object, projection)?;
        ordered_attackers.push(canonical_stable_ref_v2(decision, raw, None)?);
        match relation.payload {
            FlatRelationPayloadV2::CombatAttacker {
                blocked_order: Some(mapping_order),
            } => blocked.push((mapping_order, raw)),
            FlatRelationPayloadV2::CombatAttacker {
                blocked_order: None,
            } => {}
            _ => return Err(NativeFlatTensorErrorV2::RelationShape),
        }
    }
    blocked.sort_by_key(|(order, _)| *order);
    let blocker_pairs = relation_pairs_v2(decision.relations(), FlatRelationRoleV2::CombatBlocker)?;
    let mut mapping = Vec::with_capacity(blocked.len());
    for (mapping_order, (stored_order, attacker_raw)) in blocked.iter().enumerate() {
        if usize::try_from(*stored_order).ok() != Some(mapping_order) {
            return Err(NativeFlatTensorErrorV2::RelationOrder);
        }
        let mut blockers = blocker_pairs
            .iter()
            .filter_map(|pair| {
                let attacker = pair
                    .iter()
                    .find(|relation| relation.associated_order == 0)?;
                (usize::try_from(attacker.primary_order).ok() == Some(mapping_order))
                    .then_some(*pair)
            })
            .collect::<Vec<_>>();
        blockers.sort_by_key(|pair| pair[0].secondary_order);
        let mut blocker_values = Vec::with_capacity(blockers.len());
        for (blocker_order, pair) in blockers.iter().enumerate() {
            let attacker = pair
                .iter()
                .find(|relation| relation.associated_order == 0)
                .ok_or(NativeFlatTensorErrorV2::RelationShape)?;
            let blocker = pair
                .iter()
                .find(|relation| relation.associated_order == 1)
                .ok_or(NativeFlatTensorErrorV2::RelationShape)?;
            if usize::try_from(attacker.secondary_order).ok() != Some(blocker_order)
                || usize::try_from(
                    attacker
                        .target_object
                        .ok_or(NativeFlatTensorErrorV2::RelationShape)?,
                )
                .ok()
                    != Some(*attacker_raw)
            {
                return Err(NativeFlatTensorErrorV2::RelationOrder);
            }
            let blocker_raw = usize::try_from(
                blocker
                    .target_object
                    .ok_or(NativeFlatTensorErrorV2::RelationShape)?,
            )
            .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
            blocker_values.push(canonical_stable_ref_v2(decision, blocker_raw, None)?);
        }
        mapping.push(Value::Array(vec![
            canonical_stable_ref_v2(decision, *attacker_raw, None)?,
            Value::Array(blocker_values),
        ]));
    }
    Ok(object_value_v2([
        ("attacker_to_ordered_blockers", Value::Array(mapping)),
        (
            "attackers_declared",
            Value::Bool(decision.globals().attackers_declared),
        ),
        (
            "blockers_declared",
            Value::Bool(decision.globals().blockers_declared),
        ),
        ("ordered_attackers", Value::Array(ordered_attackers)),
    ]))
}

fn canonical_effects_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let mut baselines = decision
        .relations()
        .iter()
        .filter(|relation| relation.role == FlatRelationRoleV2::EffectSource)
        .collect::<Vec<_>>();
    baselines.sort_by_key(|relation| relation.primary_order);
    let mut effects = Vec::with_capacity(baselines.len());
    for (effect_order, baseline) in baselines.iter().enumerate() {
        if usize::try_from(baseline.primary_order).ok() != Some(effect_order) {
            return Err(NativeFlatTensorErrorV2::RelationOrder);
        }
        let payload = effect_payload_v2(baseline)?;
        let source = baseline
            .target_object
            .map(|raw| {
                let raw_index = usize::try_from(raw)
                    .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
                let _ = projected_required_node_v2(Some(raw), projection)?;
                canonical_stable_ref_v2(decision, raw_index, None)
            })
            .transpose()?
            .unwrap_or(Value::Null);
        let mut affected_objects = Vec::new();
        let mut affected_players = Vec::new();
        for relation in decision.relations().iter().filter(|relation| {
            relation.role == FlatRelationRoleV2::EffectAffected
                && relation.primary_order == baseline.primary_order
        }) {
            let row_payload = effect_payload_v2(relation)?;
            if relation.associated_order == 0 {
                let raw = usize::try_from(
                    relation
                        .target_object
                        .ok_or(NativeFlatTensorErrorV2::RelationShape)?,
                )
                .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
                affected_objects.push(canonical_stable_ref_v2(decision, raw, None)?);
            } else if relation.associated_order == 1 {
                affected_players.push(relative_player_value_v2(
                    row_payload.affected_player,
                    false,
                )?);
            } else {
                return Err(NativeFlatTensorErrorV2::RelationShape);
            }
        }
        sort_canonical_values_v2(&mut affected_objects);
        sort_canonical_values_v2(&mut affected_players);
        let mut add_subtypes = decision
            .effect_subtype_changes()
            .iter()
            .filter(|row| {
                usize::try_from(row.effect_order).ok() == Some(effect_order)
                    && row.kind == FlatEffectSubtypeChangeKindV2::Add
            })
            .map(|row| Value::from(row.subtype_id))
            .collect::<Vec<_>>();
        let mut remove_subtypes = decision
            .effect_subtype_changes()
            .iter()
            .filter(|row| {
                usize::try_from(row.effect_order).ok() == Some(effect_order)
                    && row.kind == FlatEffectSubtypeChangeKindV2::Remove
            })
            .map(|row| Value::from(row.subtype_id))
            .collect::<Vec<_>>();
        sort_canonical_values_v2(&mut add_subtypes);
        sort_canonical_values_v2(&mut remove_subtypes);
        effects.push(object_value_v2([
            ("add_color_mask", Value::from(payload.add_color_mask)),
            ("add_keyword_mask", Value::from(payload.add_keyword_mask)),
            ("add_landwalk_mask", Value::from(payload.add_landwalk_mask)),
            ("add_subtype_ids", Value::Array(add_subtypes)),
            ("affected_objects", Value::Array(affected_objects)),
            ("affected_players", Value::Array(affected_players)),
            (
                "controller",
                relative_player_value_v2(payload.controller, true)?,
            ),
            (
                "damage_cannot_be_prevented",
                Value::Bool(payload.damage_cannot_be_prevented),
            ),
            (
                "duration",
                enum_value_v2(usize::from(payload.duration), &EFFECT_DURATION_NAMES_V2)?,
            ),
            ("global", Value::Bool(payload.global)),
            ("grants_haste", Value::Bool(payload.grants_haste)),
            ("layers", Value::from(payload.layers)),
            (
                "minimum_blockers",
                payload.minimum_blockers.map_or(Value::Null, Value::from),
            ),
            ("power_delta", Value::from(payload.power_delta)),
            (
                "prevent_damage_from_color_mask",
                Value::from(payload.prevent_damage_from_color_mask),
            ),
            ("remove_color_mask", Value::from(payload.remove_color_mask)),
            (
                "remove_keyword_mask",
                Value::from(payload.remove_keyword_mask),
            ),
            (
                "remove_landwalk_mask",
                Value::from(payload.remove_landwalk_mask),
            ),
            ("remove_subtype_ids", Value::Array(remove_subtypes)),
            (
                "set_power",
                payload.set_power.map_or(Value::Null, Value::from),
            ),
            (
                "set_toughness",
                payload.set_toughness.map_or(Value::Null, Value::from),
            ),
            ("source", source),
            ("toughness_delta", Value::from(payload.toughness_delta)),
            (
                "ward_generic_delta",
                Value::from(payload.ward_generic_delta),
            ),
        ]));
    }
    Ok(Value::Array(effects))
}

fn canonical_permissions_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let mut values = Vec::new();
    for relation in decision
        .relations()
        .iter()
        .filter(|relation| relation.role == FlatRelationRoleV2::Permission)
    {
        let payload = match relation.payload {
            FlatRelationPayloadV2::Permission(payload) => payload,
            _ => return Err(NativeFlatTensorErrorV2::RelationShape),
        };
        let raw = usize::try_from(
            relation
                .target_object
                .ok_or(NativeFlatTensorErrorV2::RelationShape)?,
        )
        .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        let _ = projected_required_node_v2(relation.target_object, projection)?;
        let expiry = match payload.expiry {
            0 => object_value_v2([("expiry_kind", Value::String("end_of_turn".to_owned()))]),
            1 => object_value_v2([
                (
                    "expiry_kind",
                    Value::String("until_holders_next_turn".to_owned()),
                ),
                (
                    "holder_turn_started",
                    Value::Bool(payload.holder_turn_started),
                ),
            ]),
            _ => return Err(NativeFlatTensorErrorV2::EnumRange),
        };
        values.push(object_value_v2([
            ("expiry", expiry),
            ("holder", relative_player_value_v2(payload.holder, false)?),
            ("object", canonical_stable_ref_v2(decision, raw, None)?),
            (
                "play_or_cast",
                enum_value_v2(usize::from(payload.play_or_cast), &["play", "cast"])?,
            ),
        ]));
    }
    sort_canonical_values_v2(&mut values);
    Ok(Value::Array(values))
}

fn context_rows_v2<'a>(
    decision: FlatScoringDecisionViewV1<'a>,
    role: FlatRelationRoleV2,
    context: FlatContextKindV2,
    subrole: FlatContextSubroleV2,
) -> Result<Vec<(&'a FlatRelationV2, FlatContextRelationDataV2)>, NativeFlatTensorErrorV2> {
    let mut rows = Vec::new();
    for relation in decision
        .relations()
        .iter()
        .filter(|relation| relation.role == role)
    {
        let payload = match relation.payload {
            FlatRelationPayloadV2::Context(payload) => payload,
            _ => return Err(NativeFlatTensorErrorV2::ContextShape),
        };
        if payload.context == context && payload.subrole == subrole {
            rows.push((relation, payload));
        }
    }
    rows.sort_by_key(|(relation, _)| {
        (
            relation.primary_order,
            relation.secondary_order,
            relation.associated_order,
        )
    });
    Ok(rows)
}

fn context_ref_values_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    role: FlatRelationRoleV2,
    context: FlatContextKindV2,
    subrole: FlatContextSubroleV2,
    expected_count: u32,
) -> Result<Vec<Value>, NativeFlatTensorErrorV2> {
    let rows = context_rows_v2(decision, role, context, subrole)?;
    if usize::try_from(expected_count).ok() != Some(rows.len()) {
        return Err(NativeFlatTensorErrorV2::ContextShape);
    }
    let mut values = Vec::with_capacity(rows.len());
    for (order, (relation, payload)) in rows.iter().enumerate() {
        if usize::try_from(relation.primary_order).ok() != Some(order)
            || payload.target_kind != FlatTargetKindV2::None
            || payload.target_player != FlatRelativePlayerV1::None
        {
            return Err(NativeFlatTensorErrorV2::ContextShape);
        }
        let raw = usize::try_from(
            relation
                .target_object
                .ok_or(NativeFlatTensorErrorV2::ContextShape)?,
        )
        .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        let _ = projected_required_node_v2(relation.target_object, projection)?;
        values.push(canonical_stable_ref_v2(decision, raw, None)?);
    }
    Ok(values)
}

fn optional_context_ref_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    role: FlatRelationRoleV2,
    context: FlatContextKindV2,
    subrole: FlatContextSubroleV2,
    present: bool,
) -> Result<Value, NativeFlatTensorErrorV2> {
    if present {
        let mut values = context_ref_values_v2(decision, projection, role, context, subrole, 1)?;
        Ok(values.remove(0))
    } else {
        let rows = context_rows_v2(decision, role, context, subrole)?;
        if rows.is_empty() {
            Ok(Value::Null)
        } else {
            Err(NativeFlatTensorErrorV2::ContextShape)
        }
    }
}

fn context_target_values_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    context: FlatContextKindV2,
    subrole: FlatContextSubroleV2,
    expected_count: u32,
) -> Result<Vec<Value>, NativeFlatTensorErrorV2> {
    let rows = context_rows_v2(
        decision,
        FlatRelationRoleV2::PendingContext,
        context,
        subrole,
    )?;
    if usize::try_from(expected_count).ok() != Some(rows.len()) {
        return Err(NativeFlatTensorErrorV2::ContextShape);
    }
    let mut values = Vec::with_capacity(rows.len());
    for (order, (relation, payload)) in rows.iter().enumerate() {
        if usize::try_from(relation.primary_order).ok() != Some(order) {
            return Err(NativeFlatTensorErrorV2::ContextShape);
        }
        values.push(canonical_context_target_v2(
            decision, projection, relation, *payload,
        )?);
    }
    Ok(values)
}

fn canonical_context_target_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    relation: &FlatRelationV2,
    payload: FlatContextRelationDataV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    match payload.target_kind {
        FlatTargetKindV2::Player => {
            if relation.target_object.is_some() {
                return Err(NativeFlatTensorErrorV2::ContextShape);
            }
            Ok(object_value_v2([
                (
                    "player",
                    relative_player_value_v2(payload.target_player, false)?,
                ),
                ("target_kind", Value::String("player".to_owned())),
            ]))
        }
        FlatTargetKindV2::Object => {
            let raw = usize::try_from(
                relation
                    .target_object
                    .ok_or(NativeFlatTensorErrorV2::ContextShape)?,
            )
            .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
            let _ = projected_required_node_v2(relation.target_object, projection)?;
            Ok(object_value_v2([
                ("object", canonical_stable_ref_v2(decision, raw, None)?),
                ("target_kind", Value::String("object".to_owned())),
            ]))
        }
        FlatTargetKindV2::None => Err(NativeFlatTensorErrorV2::ContextShape),
    }
}

fn context_element_slice_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    start: u32,
    count: u32,
    kind: FlatContextElementKindV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let rows = checked_table_slice_v2(decision.context_path_elements(), start, count)?;
    let mut values = Vec::with_capacity(rows.len());
    for (order, row) in rows.iter().enumerate() {
        if row.context != FlatContextKindV2::PendingEffect
            || row.context_order != 0
            || row.kind != kind
            || usize::try_from(row.order).ok() != Some(order)
        {
            return Err(NativeFlatTensorErrorV2::ChildTableShape);
        }
        values.push(match kind {
            FlatContextElementKindV2::StructuralPath => Value::from(row.value),
            FlatContextElementKindV2::LegalColor => {
                let color = usize::from(row.value);
                enum_value_v2(color, &MANA_COLORS_V1)?
            }
        });
    }
    Ok(Value::Array(values))
}

fn canonical_engine_context_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let engine = decision.globals().engine;
    let pending_cast = match engine.pending_cast {
        None => Value::Null,
        Some(pending) => object_value_v2([
            (
                "additional_cost_discarded",
                if pending.discarded_present {
                    Value::Array(context_ref_values_v2(
                        decision,
                        projection,
                        FlatRelationRoleV2::PendingContext,
                        FlatContextKindV2::PendingCast,
                        FlatContextSubroleV2::PendingCastDiscarded,
                        pending.discarded_count,
                    )?)
                } else {
                    if pending.discarded_count != 0 {
                        return Err(NativeFlatTensorErrorV2::ContextShape);
                    }
                    Value::Null
                },
            ),
            (
                "cast_mode",
                if pending.cast_mode == 0 {
                    Value::Null
                } else {
                    enum_value_v2(usize::from(pending.cast_mode - 1), &CAST_MODES_V1)?
                },
            ),
            (
                "chosen_targets",
                Value::Array(context_target_values_v2(
                    decision,
                    projection,
                    FlatContextKindV2::PendingCast,
                    FlatContextSubroleV2::PendingCastChosenTarget,
                    pending.chosen_target_count,
                )?),
            ),
            (
                "controller",
                relative_player_value_v2(pending.controller, false)?,
            ),
            ("is_flashback", Value::Bool(pending.is_flashback)),
            ("kicked", pending.kicked.map_or(Value::Null, Value::Bool)),
            (
                "mode_chosen",
                pending.mode_chosen.map_or(Value::Null, Value::from),
            ),
            ("origin_zone", zone_value_v2(pending.origin_zone)),
            (
                "sacrifice_chosen",
                Value::Array(context_ref_values_v2(
                    decision,
                    projection,
                    FlatRelationRoleV2::PendingContext,
                    FlatContextKindV2::PendingCast,
                    FlatContextSubroleV2::PendingCastSacrificed,
                    pending.sacrificed_count,
                )?),
            ),
            (
                "source",
                optional_context_ref_v2(
                    decision,
                    projection,
                    FlatRelationRoleV2::PendingContext,
                    FlatContextKindV2::PendingCast,
                    FlatContextSubroleV2::PendingCastSource,
                    pending.source_present,
                )?,
            ),
        ]),
    };
    let pending_activation = match engine.pending_activation {
        None => Value::Null,
        Some(pending) => object_value_v2([
            ("ability_index", Value::from(pending.ability_index)),
            (
                "chosen_targets",
                Value::Array(context_target_values_v2(
                    decision,
                    projection,
                    FlatContextKindV2::PendingActivation,
                    FlatContextSubroleV2::PendingActivationChosenTarget,
                    pending.chosen_target_count,
                )?),
            ),
            (
                "controller",
                relative_player_value_v2(pending.controller, false)?,
            ),
            (
                "cost_discard_paid",
                if pending.discard_paid_present {
                    Value::Array(context_ref_values_v2(
                        decision,
                        projection,
                        FlatRelationRoleV2::PendingContext,
                        FlatContextKindV2::PendingActivation,
                        FlatContextSubroleV2::PendingActivationDiscarded,
                        pending.discard_paid_count,
                    )?)
                } else {
                    if pending.discard_paid_count != 0 {
                        return Err(NativeFlatTensorErrorV2::ContextShape);
                    }
                    Value::Null
                },
            ),
            (
                "source",
                optional_context_ref_v2(
                    decision,
                    projection,
                    FlatRelationRoleV2::PendingContext,
                    FlatContextKindV2::PendingActivation,
                    FlatContextSubroleV2::PendingActivationSource,
                    pending.source_present,
                )?,
            ),
        ]),
    };
    let pending_discard = match engine.pending_discard {
        None => Value::Null,
        Some(pending) => object_value_v2([
            ("count", Value::from(pending.count)),
            ("player", relative_player_value_v2(pending.player, false)?),
            (
                "resume_source",
                optional_context_ref_v2(
                    decision,
                    projection,
                    FlatRelationRoleV2::PendingContext,
                    FlatContextKindV2::PendingDiscard,
                    FlatContextSubroleV2::PendingDiscardResumeSource,
                    pending.resume_source_present,
                )?,
            ),
            (
                "resume_stage",
                enum_value_v2(usize::from(pending.resume_stage), &DISCARD_RESUME_NAMES_V2)?,
            ),
        ]),
    };
    let pending_optional_cost = canonical_pending_optional_cost_v2(decision, projection)?;
    let pending_optional_sacrifice = canonical_pending_optional_sacrifice_v2(decision, projection)?;
    let pending_spell_copy = canonical_pending_spell_copy_v2(decision, projection)?;
    let pending_effect = canonical_pending_effect_v2(decision, projection)?;
    let pending_triggers = canonical_pending_triggers_v2(decision, projection)?;
    Ok(object_value_v2([
        (
            "current_stage",
            enum_value_v2(usize::from(engine.current_stage), &ENGINE_STAGE_NAMES_V2)?,
        ),
        (
            "last_mana_ability_activator_since_priority_boundary",
            relative_player_value_v2(engine.last_mana_ability_activator, true)?,
        ),
        (
            "mana_activity_since_priority_boundary",
            Value::Bool(engine.mana_activity_since_priority_boundary),
        ),
        ("pending_activation", pending_activation),
        ("pending_cast", pending_cast),
        ("pending_discard", pending_discard),
        ("pending_effect", pending_effect),
        ("pending_optional_cost", pending_optional_cost),
        (
            "pending_optional_cost_sacrifice",
            pending_optional_sacrifice,
        ),
        ("pending_spell_copy", pending_spell_copy),
        ("pending_triggers", pending_triggers),
        (
            "priority_passes",
            Value::Array(
                engine
                    .priority_passes
                    .into_iter()
                    .map(Value::Bool)
                    .collect(),
            ),
        ),
        (
            "stack_activity_since_priority_boundary",
            Value::Bool(engine.stack_activity_since_priority_boundary),
        ),
        ("stack_nonempty", Value::Bool(engine.stack_nonempty)),
    ]))
}

fn canonical_pending_optional_cost_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let Some(pending) = decision.globals().engine.pending_optional_cost else {
        return Ok(Value::Null);
    };
    Ok(object_value_v2([
        ("discard_cards", Value::from(pending.discard_cards)),
        ("discard_payable", Value::Bool(pending.discard_payable)),
        ("player", relative_player_value_v2(pending.player, false)?),
        ("sacrifice_lands", Value::from(pending.sacrifice_lands)),
        ("sacrifice_payable", Value::Bool(pending.sacrifice_payable)),
        (
            "source",
            optional_context_ref_v2(
                decision,
                projection,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingOptionalCost,
                FlatContextSubroleV2::PendingOptionalCostSource,
                pending.source_present,
            )?,
        ),
        (
            "spell_resume_source",
            optional_context_ref_v2(
                decision,
                projection,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingOptionalCost,
                FlatContextSubroleV2::PendingOptionalCostSpellResumeSource,
                pending.spell_resume_source_present,
            )?,
        ),
        (
            "spell_resume_zone",
            pending
                .spell_resume_zone
                .map(zone_value_v2)
                .unwrap_or(Value::Null),
        ),
    ]))
}

fn canonical_pending_optional_sacrifice_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let Some(pending) = decision.globals().engine.pending_optional_sacrifice else {
        return Ok(Value::Null);
    };
    Ok(object_value_v2([
        (
            "chosen",
            Value::Array(context_ref_values_v2(
                decision,
                projection,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingOptionalCostSacrifice,
                FlatContextSubroleV2::PendingOptionalSacrificeChosen,
                pending.chosen_count,
            )?),
        ),
        ("player", relative_player_value_v2(pending.player, false)?),
        ("remaining", Value::from(pending.remaining)),
        (
            "source",
            optional_context_ref_v2(
                decision,
                projection,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingOptionalCostSacrifice,
                FlatContextSubroleV2::PendingOptionalSacrificeSource,
                pending.source_present,
            )?,
        ),
        (
            "spell_resume_source",
            optional_context_ref_v2(
                decision,
                projection,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingOptionalCostSacrifice,
                FlatContextSubroleV2::PendingOptionalSacrificeSpellResumeSource,
                pending.spell_resume_source_present,
            )?,
        ),
        (
            "spell_resume_zone",
            pending
                .spell_resume_zone
                .map(zone_value_v2)
                .unwrap_or(Value::Null),
        ),
    ]))
}

fn canonical_pending_spell_copy_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let Some(pending) = decision.globals().engine.pending_spell_copy else {
        return Ok(Value::Null);
    };
    let mut inherited = context_target_values_v2(
        decision,
        projection,
        FlatContextKindV2::PendingSpellCopy,
        FlatContextSubroleV2::PendingSpellCopyInheritedTarget,
        1,
    )?;
    let inherited_target = inherited.remove(0);
    let expected_kind = match inherited_target
        .as_object()
        .and_then(|value| value.get("target_kind"))
        .and_then(Value::as_str)
    {
        Some("player") => FlatTargetKindV2::Player,
        Some("object") => FlatTargetKindV2::Object,
        _ => return Err(NativeFlatTensorErrorV2::ContextShape),
    };
    if expected_kind != pending.inherited_target_kind {
        return Err(NativeFlatTensorErrorV2::ContextShape);
    }
    Ok(object_value_v2([
        (
            "copy",
            optional_context_ref_v2(
                decision,
                projection,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingSpellCopy,
                FlatContextSubroleV2::PendingSpellCopyCopy,
                pending.copy_present,
            )?,
        ),
        ("inherited_target", inherited_target),
        (
            "parent",
            optional_context_ref_v2(
                decision,
                projection,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingSpellCopy,
                FlatContextSubroleV2::PendingSpellCopyParent,
                pending.parent_present,
            )?,
        ),
        ("player", relative_player_value_v2(pending.player, false)?),
        (
            "stage",
            enum_value_v2(usize::from(pending.stage), &SPELL_COPY_STAGE_NAMES_V2)?,
        ),
    ]))
}

fn canonical_pending_effect_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let Some(pending) = decision.globals().engine.pending_effect else {
        return Ok(Value::Null);
    };
    let choice = match pending.choice {
        None => Value::Null,
        Some(FlatPendingEffectChoiceV2::Options {
            player,
            path_start,
            path_count,
            option_count,
        }) => object_value_v2([
            ("choice_kind", Value::String("options".to_owned())),
            ("option_count", Value::from(option_count)),
            ("player", relative_player_value_v2(player, false)?),
            (
                "structural_path",
                context_element_slice_v2(
                    decision,
                    path_start,
                    path_count,
                    FlatContextElementKindV2::StructuralPath,
                )?,
            ),
        ]),
        Some(FlatPendingEffectChoiceV2::Targets {
            player,
            path_start,
            path_count,
            selected_count,
            legal_count,
            min_targets,
            max_targets,
            can_finish,
            ordered,
            purpose,
        }) => object_value_v2([
            ("can_finish", Value::Bool(can_finish)),
            ("choice_kind", Value::String("targets".to_owned())),
            (
                "legal_targets",
                Value::Array(context_target_values_v2(
                    decision,
                    projection,
                    FlatContextKindV2::PendingEffect,
                    FlatContextSubroleV2::PendingEffectLegalTarget,
                    legal_count,
                )?),
            ),
            ("max_targets", Value::from(max_targets)),
            ("min_targets", Value::from(min_targets)),
            ("ordered", Value::Bool(ordered)),
            ("player", relative_player_value_v2(player, false)?),
            (
                "purpose",
                enum_value_v2(usize::from(purpose), &TARGET_PURPOSE_NAMES_V2)?,
            ),
            (
                "selected_targets",
                Value::Array(context_target_values_v2(
                    decision,
                    projection,
                    FlatContextKindV2::PendingEffect,
                    FlatContextSubroleV2::PendingEffectSelectedTarget,
                    selected_count,
                )?),
            ),
            (
                "structural_path",
                context_element_slice_v2(
                    decision,
                    path_start,
                    path_count,
                    FlatContextElementKindV2::StructuralPath,
                )?,
            ),
        ]),
        Some(FlatPendingEffectChoiceV2::Color {
            player,
            path_start,
            path_count,
            legal_color_start,
            legal_color_count,
        }) => object_value_v2([
            ("choice_kind", Value::String("color".to_owned())),
            (
                "legal_colors",
                context_element_slice_v2(
                    decision,
                    legal_color_start,
                    legal_color_count,
                    FlatContextElementKindV2::LegalColor,
                )?,
            ),
            ("player", relative_player_value_v2(player, false)?),
            (
                "structural_path",
                context_element_slice_v2(
                    decision,
                    path_start,
                    path_count,
                    FlatContextElementKindV2::StructuralPath,
                )?,
            ),
        ]),
        Some(FlatPendingEffectChoiceV2::Number {
            player,
            path_start,
            path_count,
            minimum,
            maximum,
        }) => object_value_v2([
            ("choice_kind", Value::String("number".to_owned())),
            ("maximum", Value::from(maximum)),
            ("minimum", Value::from(minimum)),
            ("player", relative_player_value_v2(player, false)?),
            (
                "structural_path",
                context_element_slice_v2(
                    decision,
                    path_start,
                    path_count,
                    FlatContextElementKindV2::StructuralPath,
                )?,
            ),
        ]),
        Some(FlatPendingEffectChoiceV2::Boolean {
            player,
            path_start,
            path_count,
            default,
            purpose,
        }) => object_value_v2([
            ("choice_kind", Value::String("boolean".to_owned())),
            ("default", default.map_or(Value::Null, Value::Bool)),
            ("player", relative_player_value_v2(player, false)?),
            (
                "purpose",
                enum_value_v2(usize::from(purpose), &BOOLEAN_PURPOSE_NAMES_V2)?,
            ),
            (
                "structural_path",
                context_element_slice_v2(
                    decision,
                    path_start,
                    path_count,
                    FlatContextElementKindV2::StructuralPath,
                )?,
            ),
        ]),
    };
    Ok(object_value_v2([
        ("choice", choice),
        (
            "controller",
            relative_player_value_v2(pending.controller, false)?,
        ),
        (
            "source",
            optional_context_ref_v2(
                decision,
                projection,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingEffect,
                FlatContextSubroleV2::PendingEffectSource,
                pending.source_present,
            )?,
        ),
    ]))
}

fn canonical_pending_triggers_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let engine = decision.globals().engine;
    let mut rows = context_rows_v2(
        decision,
        FlatRelationRoleV2::PendingContext,
        FlatContextKindV2::PendingTrigger,
        FlatContextSubroleV2::PendingTriggerSource,
    )?;
    rows.sort_by_key(|(relation, _)| relation.primary_order);
    if usize::try_from(engine.pending_trigger_count).ok() != Some(rows.len()) {
        return Err(NativeFlatTensorErrorV2::ContextShape);
    }
    let mut values = Vec::with_capacity(rows.len());
    for (order, (relation, payload)) in rows.iter().enumerate() {
        if usize::try_from(relation.primary_order).ok() != Some(order) {
            return Err(NativeFlatTensorErrorV2::RelationOrder);
        }
        let source = if let Some(raw) = relation.target_object {
            let raw =
                usize::try_from(raw).map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
            let _ = projected_required_node_v2(relation.target_object, projection)?;
            canonical_stable_ref_v2(decision, raw, None)?
        } else {
            Value::Null
        };
        let trigger_kind = match payload.trigger_kind {
            1 => "triggered_ability",
            2 => "madness_offer",
            _ => return Err(NativeFlatTensorErrorV2::EnumRange),
        };
        values.push(object_value_v2([
            (
                "controller",
                relative_player_value_v2(payload.controller, false)?,
            ),
            ("kicked", Value::Bool(payload.kicked)),
            ("source", source),
            ("trigger_kind", Value::String(trigger_kind.to_owned())),
        ]));
    }
    Ok(Value::Array(values))
}

fn canonical_surface_context_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let surface = decision.globals().surface;
    let madness = optional_context_ref_v2(
        decision,
        projection,
        FlatRelationRoleV2::PrivateContext,
        FlatContextKindV2::MadnessCastReprompt,
        FlatContextSubroleV2::MadnessCastRepromptSource,
        surface.madness_cast_reprompt_source_present,
    )?;
    let private_blockers = canonical_private_blockers_v2(decision, projection)?;
    let private_discard = canonical_private_discard_v2(decision, projection)?;
    let private_optional = match (
        surface.private_optional_discard_payable,
        surface.private_optional_sacrifice_payable,
        surface.private_optional_stage,
    ) {
        (None, None, None) => Value::Null,
        (Some(discard), Some(sacrifice), Some(stage)) => object_value_v2([
            ("discard_payable", Value::Bool(discard)),
            ("sacrifice_payable", Value::Bool(sacrifice)),
            (
                "stage",
                enum_value_v2(usize::from(stage), &SURFACE_STAGE_NAMES_V2)?,
            ),
        ]),
        _ => return Err(NativeFlatTensorErrorV2::ContextShape),
    };
    Ok(object_value_v2([
        (
            "combat_priority_rearmed_by_mana_activity",
            Value::Bool(surface.combat_priority_rearmed_by_mana_activity),
        ),
        (
            "combat_priority_rearmed_by_stack_activity",
            Value::Bool(surface.combat_priority_rearmed_by_stack_activity),
        ),
        (
            "combat_priority_spent",
            Value::Array(
                surface
                    .combat_priority_spent
                    .into_iter()
                    .map(Value::Bool)
                    .collect(),
            ),
        ),
        (
            "current_stage",
            enum_value_v2(usize::from(surface.current_stage), &SURFACE_STAGE_NAMES_V2)?,
        ),
        ("madness_cast_reprompt_source", madness),
        (
            "mana_activity_since_last_stack_change",
            Value::Bool(surface.mana_activity_since_last_stack_change),
        ),
        (
            "mana_activity_since_round_open",
            Value::Bool(surface.mana_activity_since_round_open),
        ),
        ("private_blockers", private_blockers),
        ("private_discard", private_discard),
        ("private_optional_cost", private_optional),
        (
            "stack_grew_since_round_open",
            Value::Bool(surface.stack_grew_since_round_open),
        ),
        (
            "stack_length_changed_since_observed",
            surface
                .stack_length_changed_since_observed
                .map_or(Value::Null, Value::Bool),
        ),
    ]))
}

fn canonical_private_blockers_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let present = decision.globals().surface.private_blockers_present;
    if !present {
        if decision.relations().iter().any(|relation| {
            matches!(
                relation.payload,
                FlatRelationPayloadV2::Context(FlatContextRelationDataV2 {
                    context: FlatContextKindV2::PrivateBlockers,
                    ..
                })
            )
        }) {
            return Err(NativeFlatTensorErrorV2::ContextShape);
        }
        return Ok(Value::Null);
    }
    let current_rows = context_rows_v2(
        decision,
        FlatRelationRoleV2::PrivateContext,
        FlatContextKindV2::PrivateBlockers,
        FlatContextSubroleV2::PrivateBlockersCurrentAttacker,
    )?;
    let current = match current_rows.as_slice() {
        [] => Value::Null,
        [(relation, _)] => {
            let raw = usize::try_from(
                relation
                    .target_object
                    .ok_or(NativeFlatTensorErrorV2::ContextShape)?,
            )
            .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
            let _ = projected_required_node_v2(relation.target_object, projection)?;
            canonical_stable_ref_v2(decision, raw, None)?
        }
        _ => return Err(NativeFlatTensorErrorV2::ContextShape),
    };
    let attackers = context_rows_v2(
        decision,
        FlatRelationRoleV2::PrivateContext,
        FlatContextKindV2::PrivateBlockers,
        FlatContextSubroleV2::PrivateBlockersAccumulatedAttacker,
    )?;
    let blockers = context_rows_v2(
        decision,
        FlatRelationRoleV2::PrivateContext,
        FlatContextKindV2::PrivateBlockers,
        FlatContextSubroleV2::PrivateBlockersAccumulatedBlocker,
    )?;
    if attackers.len() != blockers.len() {
        return Err(NativeFlatTensorErrorV2::ContextShape);
    }
    let mut accumulated = Vec::with_capacity(attackers.len());
    for (order, ((attacker, _), (blocker, _))) in attackers.iter().zip(&blockers).enumerate() {
        if usize::try_from(attacker.primary_order).ok() != Some(order)
            || usize::try_from(blocker.primary_order).ok() != Some(order)
            || attacker.associated_order != 0
            || blocker.associated_order != 1
        {
            return Err(NativeFlatTensorErrorV2::RelationOrder);
        }
        let attacker_raw = usize::try_from(
            attacker
                .target_object
                .ok_or(NativeFlatTensorErrorV2::ContextShape)?,
        )
        .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        let blocker_raw = usize::try_from(
            blocker
                .target_object
                .ok_or(NativeFlatTensorErrorV2::ContextShape)?,
        )
        .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        accumulated.push(Value::Array(vec![
            canonical_stable_ref_v2(decision, attacker_raw, None)?,
            canonical_stable_ref_v2(decision, blocker_raw, None)?,
        ]));
    }
    let remaining_attackers = context_rows_v2(
        decision,
        FlatRelationRoleV2::PrivateContext,
        FlatContextKindV2::PrivateBlockers,
        FlatContextSubroleV2::PrivateBlockersRemainingAttacker,
    )?;
    let remaining_blockers = context_rows_v2(
        decision,
        FlatRelationRoleV2::PrivateContext,
        FlatContextKindV2::PrivateBlockers,
        FlatContextSubroleV2::PrivateBlockersRemainingBlocker,
    )?;
    let mut remaining = Vec::with_capacity(remaining_attackers.len());
    for (attacker_order, (attacker, _)) in remaining_attackers.iter().enumerate() {
        if usize::try_from(attacker.primary_order).ok() != Some(attacker_order) {
            return Err(NativeFlatTensorErrorV2::RelationOrder);
        }
        let attacker_raw = usize::try_from(
            attacker
                .target_object
                .ok_or(NativeFlatTensorErrorV2::ContextShape)?,
        )
        .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        let mut blocker_rows = remaining_blockers
            .iter()
            .filter(|(relation, _)| {
                usize::try_from(relation.primary_order).ok() == Some(attacker_order)
            })
            .collect::<Vec<_>>();
        blocker_rows.sort_by_key(|(relation, _)| relation.secondary_order);
        let mut blocker_values = Vec::with_capacity(blocker_rows.len());
        for (blocker_order, (relation, _)) in blocker_rows.iter().enumerate() {
            if usize::try_from(relation.secondary_order).ok() != Some(blocker_order) {
                return Err(NativeFlatTensorErrorV2::RelationOrder);
            }
            let raw = usize::try_from(
                relation
                    .target_object
                    .ok_or(NativeFlatTensorErrorV2::ContextShape)?,
            )
            .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
            blocker_values.push(canonical_stable_ref_v2(decision, raw, None)?);
        }
        remaining.push(Value::Array(vec![
            canonical_stable_ref_v2(decision, attacker_raw, None)?,
            Value::Array(blocker_values),
        ]));
    }
    Ok(object_value_v2([
        ("accumulated", Value::Array(accumulated)),
        ("current_attacker", current),
        ("remaining", Value::Array(remaining)),
    ]))
}

fn canonical_private_discard_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let surface = decision.globals().surface;
    let Some(remaining_needed) = surface.private_discard_remaining_needed else {
        if surface.private_discard_chosen_count != 0 || surface.private_discard_remaining_count != 0
        {
            return Err(NativeFlatTensorErrorV2::ContextShape);
        }
        return Ok(Value::Null);
    };
    Ok(object_value_v2([
        (
            "chosen",
            Value::Array(context_ref_values_v2(
                decision,
                projection,
                FlatRelationRoleV2::PrivateContext,
                FlatContextKindV2::PrivateDiscard,
                FlatContextSubroleV2::PrivateDiscardChosen,
                surface.private_discard_chosen_count,
            )?),
        ),
        (
            "remaining_choices",
            Value::Array(context_ref_values_v2(
                decision,
                projection,
                FlatRelationRoleV2::PrivateContext,
                FlatContextKindV2::PrivateDiscard,
                FlatContextSubroleV2::PrivateDiscardRemainingChoice,
                surface.private_discard_remaining_count,
            )?),
        ),
        ("remaining_needed", Value::from(remaining_needed)),
    ]))
}

fn canonical_policy_surface_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
) -> Result<Value, NativeFlatTensorErrorV2> {
    let policy = decision.globals().policy_surface;
    let private = if !policy.private_combat_present {
        if policy.private_combat_attacker_present
            || policy.candidate_index != 0
            || policy.candidate_count != 0
            || policy.selected_count != 0
            || policy.remaining_count != 0
        {
            return Err(NativeFlatTensorErrorV2::ContextShape);
        }
        Value::Null
    } else {
        let current_rows = context_rows_v2(
            decision,
            FlatRelationRoleV2::PrivateContext,
            FlatContextKindV2::PrivateCombatSelection,
            FlatContextSubroleV2::PrivateCombatCurrentCandidate,
        )?;
        if current_rows.len() != 1 || current_rows[0].0.primary_order != policy.candidate_index {
            return Err(NativeFlatTensorErrorV2::ContextShape);
        }
        let current_raw = usize::try_from(
            current_rows[0]
                .0
                .target_object
                .ok_or(NativeFlatTensorErrorV2::ContextShape)?,
        )
        .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
        object_value_v2([
            (
                "attacker",
                optional_context_ref_v2(
                    decision,
                    projection,
                    FlatRelationRoleV2::PrivateContext,
                    FlatContextKindV2::PrivateCombatSelection,
                    FlatContextSubroleV2::PrivateCombatAttacker,
                    policy.private_combat_attacker_present,
                )?,
            ),
            ("candidate_count", Value::from(policy.candidate_count)),
            ("candidate_index", Value::from(policy.candidate_index)),
            (
                "current_candidate",
                canonical_stable_ref_v2(decision, current_raw, None)?,
            ),
            (
                "remaining_after_current",
                Value::Array(context_ref_values_v2(
                    decision,
                    projection,
                    FlatRelationRoleV2::PrivateContext,
                    FlatContextKindV2::PrivateCombatSelection,
                    FlatContextSubroleV2::PrivateCombatRemainingCandidate,
                    policy.remaining_count,
                )?),
            ),
            (
                "selected",
                Value::Array(context_ref_values_v2(
                    decision,
                    projection,
                    FlatRelationRoleV2::PrivateContext,
                    FlatContextKindV2::PrivateCombatSelection,
                    FlatContextSubroleV2::PrivateCombatSelected,
                    policy.selected_count,
                )?),
            ),
        ])
    };
    Ok(object_value_v2([
        (
            "current_stage",
            enum_value_v2(usize::from(policy.current_stage), &POLICY_STAGE_NAMES_V2)?,
        ),
        ("private_combat_selection", private),
    ]))
}

fn validate_full_output_v2(
    output: &NativeFlatDecisionTensorV2,
    real_object_count: usize,
    action_count: usize,
    action_ref_count: usize,
) -> Result<(), NativeFlatTensorErrorV2> {
    let object_count = real_object_count.max(1);
    if output.state.len() != NATIVE_FLAT_STATE_FEATURE_DIM_V2
        || output.object_features.len()
            != object_count
                .checked_mul(NATIVE_FLAT_OBJECT_FEATURE_DIM_V2)
                .ok_or(NativeFlatTensorErrorV2::CheckedIntegerRange)?
        || output.object_card_ids.len() != object_count
        || output.object_groups.len() != object_count
        || output.object_node_ids.len() != object_count
        || output.edge_features.len()
            != output
                .edge_source_indices
                .len()
                .checked_mul(NATIVE_FLAT_EDGE_FEATURE_DIM_V2)
                .ok_or(NativeFlatTensorErrorV2::CheckedIntegerRange)?
        || output.edge_target_indices.len() != output.edge_source_indices.len()
        || !output
            .state
            .iter()
            .chain(&output.object_features)
            .chain(&output.edge_features)
            .all(|value| value.is_finite())
    {
        return Err(NativeFlatTensorErrorV2::OutputInvariant);
    }
    for (index, node) in output.object_node_ids.iter().enumerate() {
        if usize::try_from(*node).ok() != Some(index) {
            return Err(NativeFlatTensorErrorV2::OutputInvariant);
        }
    }
    for (&source, &target) in output
        .edge_source_indices
        .iter()
        .zip(&output.edge_target_indices)
    {
        if usize::try_from(source)
            .ok()
            .is_none_or(|value| value >= object_count)
            || usize::try_from(target)
                .ok()
                .is_none_or(|value| value >= object_count)
        {
            return Err(NativeFlatTensorErrorV2::OutputInvariant);
        }
    }
    validate_native_flat_action_slices_v1(
        &output.action_features,
        &output.action_ref_features,
        &output.action_ref_card_ids,
        &output.action_ref_action_indices,
        &output.action_ref_node_indices,
        action_count,
        action_ref_count,
        object_count,
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_native_flat_action_slices_v1(
    action_features: &[f32],
    action_ref_features: &[f32],
    action_ref_card_ids: &[i64],
    action_ref_action_indices: &[i64],
    action_ref_node_indices: &[i64],
    action_count: usize,
    action_ref_count: usize,
    object_count: usize,
) -> Result<(), NativeFlatTensorErrorV1> {
    if action_count == 0 {
        return Err(NativeFlatTensorErrorV1::OutputInvariant);
    }
    let action_elements = action_count
        .checked_mul(NATIVE_FLAT_ACTION_FEATURE_DIM_V1)
        .ok_or(NativeFlatTensorErrorV1::CheckedIntegerRange)?;
    let ref_elements = action_ref_count
        .checked_mul(NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1)
        .ok_or(NativeFlatTensorErrorV1::CheckedIntegerRange)?;
    if action_features.len() != action_elements
        || action_ref_features.len() != ref_elements
        || action_ref_card_ids.len() != action_ref_count
        || action_ref_action_indices.len() != action_ref_count
        || action_ref_node_indices.len() != action_ref_count
        || !action_features
            .iter()
            .chain(action_ref_features)
            .all(|value| value.is_finite())
    {
        return Err(NativeFlatTensorErrorV1::OutputInvariant);
    }
    for index in 0..action_ref_count {
        let action = usize::try_from(action_ref_action_indices[index])
            .map_err(|_| NativeFlatTensorErrorV1::OutputInvariant)?;
        let node = usize::try_from(action_ref_node_indices[index])
            .map_err(|_| NativeFlatTensorErrorV1::OutputInvariant)?;
        if action >= action_count
            || node >= object_count
            || action_ref_card_ids[index] <= 0
            || action_ref_card_ids[index] > i64::from(NATIVE_FLAT_CURRENT_MAX_CARD_TOKEN_V1)
        {
            return Err(NativeFlatTensorErrorV1::OutputInvariant);
        }
    }
    Ok(())
}

fn encode_action_half_v1(
    decision: FlatScoringDecisionViewV1<'_>,
) -> Result<ActionHalfV1, NativeFlatTensorErrorV1> {
    encode_action_half_with_projection_v2(decision, None)
}

fn encode_action_half_with_projection_v2(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: Option<&ObjectProjectionV2>,
) -> Result<ActionHalfV1, NativeFlatTensorErrorV1> {
    if decision.globals().acting_player != FlatRelativePlayerV1::SelfPlayer {
        return Err(NativeFlatTensorErrorV1::ActingPlayerNotRelativeSelf);
    }
    let actions = decision.actions();
    let refs = decision.action_refs();
    if actions.is_empty() {
        return Err(NativeFlatTensorErrorV1::EmptyActionTable);
    }
    let action_elements = actions
        .len()
        .checked_mul(NATIVE_FLAT_ACTION_FEATURE_DIM_V1)
        .ok_or(NativeFlatTensorErrorV1::CheckedIntegerRange)?;
    let ref_elements = refs
        .len()
        .checked_mul(NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1)
        .ok_or(NativeFlatTensorErrorV1::CheckedIntegerRange)?;
    let mut out = ActionHalfV1 {
        action_features: try_vec_capacity(action_elements)?,
        action_ref_features: try_vec_capacity(ref_elements)?,
        action_ref_card_ids: try_vec_capacity(refs.len())?,
        action_ref_action_indices: try_vec_capacity(refs.len())?,
        action_ref_node_indices: try_vec_capacity(refs.len())?,
    };
    let mut ref_cursor = 0usize;
    for (action_index, action) in actions.iter().enumerate() {
        let start = usize::try_from(action.ref_start)
            .map_err(|_| NativeFlatTensorErrorV1::CheckedIntegerRange)?;
        let end = start
            .checked_add(usize::from(action.ref_len))
            .ok_or(NativeFlatTensorErrorV1::CheckedIntegerRange)?;
        if start != ref_cursor || end > refs.len() {
            return Err(NativeFlatTensorErrorV1::ActionReferenceRange);
        }
        let encoded = encode_action_v1(
            decision,
            action_index,
            action,
            &refs[start..end],
            projection,
        )?;
        out.action_features.extend_from_slice(&encoded.features);
        let action_index = i64::try_from(action_index)
            .map_err(|_| NativeFlatTensorErrorV1::CheckedIntegerRange)?;
        for ((features, token), node) in encoded
            .ref_features
            .iter()
            .zip(&encoded.ref_card_ids)
            .zip(&encoded.ref_node_indices)
        {
            out.action_ref_features.extend_from_slice(features);
            out.action_ref_card_ids.push(*token);
            out.action_ref_action_indices.push(action_index);
            out.action_ref_node_indices.push(*node);
        }
        ref_cursor = end;
    }
    if ref_cursor != refs.len() {
        return Err(NativeFlatTensorErrorV1::ActionReferenceRange);
    }
    validate_native_flat_action_slices_v1(
        &out.action_features,
        &out.action_ref_features,
        &out.action_ref_card_ids,
        &out.action_ref_action_indices,
        &out.action_ref_node_indices,
        actions.len(),
        refs.len(),
        projection.map_or(decision.objects().len(), |value| value.node_to_raw.len()),
    )?;
    Ok(out)
}

fn try_vec_capacity<T>(capacity: usize) -> Result<Vec<T>, NativeFlatTensorErrorV1> {
    let mut out = Vec::new();
    out.try_reserve_exact(capacity)
        .map_err(|_| NativeFlatTensorErrorV1::AllocationFailed)?;
    Ok(out)
}

fn encode_action_v1<'a>(
    decision: FlatScoringDecisionViewV1<'a>,
    action_index: usize,
    action: &FlatScorerActionCoreV1,
    raw_refs: &'a [FlatScorerActionRefV1],
    projection: Option<&ObjectProjectionV2>,
) -> Result<EncodedActionV1, NativeFlatTensorErrorV1> {
    let resolved = resolve_action_refs_v1(decision, action_index, raw_refs)?;
    let mut expected = FlatScorerActionCoreV1 {
        kind: action.kind,
        ref_start: action.ref_start,
        ref_len: action.ref_len,
        ..FlatScorerActionCoreV1::default()
    };
    let mut semantic = Map::<String, Value>::new();
    semantic.insert(
        "action_kind".to_owned(),
        Value::String(action_kind_name_v1(action.kind).to_owned()),
    );
    semantic.insert("actor".to_owned(), Value::String("self".to_owned()));
    let mut projected_refs = Vec::<ProjectedActionRefV1<'a>>::new();

    match action.kind {
        FlatScorerActionKindV1::Pass => require_no_refs(&resolved)?,
        FlatScorerActionKindV1::PlayLand
        | FlatScorerActionKindV1::CastSpell
        | FlatScorerActionKindV1::PlotSpell => {
            let source = require_only_ref(&resolved, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
        }
        FlatScorerActionKindV1::ActivateManaAbility => {
            if action.mana_choice > 6 {
                return Err(NativeFlatTensorErrorV1::InvalidActionRange);
            }
            expected.mana_choice = action.mana_choice;
            let source = require_only_ref(&resolved, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert(
                "mana_choice".to_owned(),
                optional_one_based_name(action.mana_choice, &MANA_COLORS_V1),
            );
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
        }
        FlatScorerActionKindV1::ActivateAbility => {
            expected.ability_index = action.ability_index;
            let source = require_only_ref(&resolved, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert(
                "ability_index".to_owned(),
                Value::from(action.ability_index),
            );
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
        }
        FlatScorerActionKindV1::ChooseTarget => {
            if action.remaining == 0 {
                return Err(NativeFlatTensorErrorV1::InvalidActionRange);
            }
            expected.remaining = action.remaining;
            expected.target_kind = action.target_kind;
            expected.target_player = action.target_player;
            let source = require_singular_ref(&resolved, 0, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert("remaining".to_owned(), Value::from(action.remaining));
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
            let (target, target_ref) = canonical_target_v1(action, &resolved, 1)?;
            semantic.insert("target".to_owned(), target);
            if let Some(target_ref) = target_ref {
                projected_refs.push(projected_singular(target_ref, ROLE_TARGET_OBJECT_V1));
            }
        }
        FlatScorerActionKindV1::ChooseCostTarget => {
            if action.remaining == 0 || !(1..=11).contains(&action.cost_kind) {
                return Err(NativeFlatTensorErrorV1::InvalidActionRange);
            }
            expected.remaining = action.remaining;
            expected.cost_kind = action.cost_kind;
            let source = require_singular_ref(&resolved, 0, ROLE_SOURCE_V1)?;
            let candidate = require_singular_ref(&resolved, 1, ROLE_CANDIDATE_V1)?;
            require_ref_count(&resolved, 2)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert("candidate".to_owned(), canonical_card_ref_v1(candidate)?);
            semantic.insert("remaining".to_owned(), Value::from(action.remaining));
            semantic.insert(
                "cost_kind".to_owned(),
                Value::String(one_based_name(action.cost_kind, &COST_KINDS_V1)?.to_owned()),
            );
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
            projected_refs.push(projected_singular(candidate, ROLE_CANDIDATE_V1));
        }
        FlatScorerActionKindV1::ChooseCastMode => {
            if !(1..=2).contains(&action.cast_mode) {
                return Err(NativeFlatTensorErrorV1::InvalidActionRange);
            }
            expected.cast_mode = action.cast_mode;
            let source = require_only_ref(&resolved, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert(
                "mode".to_owned(),
                Value::String(one_based_name(action.cast_mode, &CAST_MODES_V1)?.to_owned()),
            );
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
        }
        FlatScorerActionKindV1::ChooseKicker | FlatScorerActionKindV1::ChooseSpellCopyPayment => {
            validate_only_flag(action.flags, FLAT_ACTION_FLAG_PAY_V1)?;
            expected.flags = action.flags;
            let source = require_only_ref(&resolved, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert(
                "pay".to_owned(),
                Value::Bool(action.flags & FLAT_ACTION_FLAG_PAY_V1 != 0),
            );
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
        }
        FlatScorerActionKindV1::ChooseSpellMode => {
            if action.mode_count == 0 || action.mode_index >= action.mode_count {
                return Err(NativeFlatTensorErrorV1::InvalidActionRange);
            }
            expected.mode_index = action.mode_index;
            expected.mode_count = action.mode_count;
            let source = require_only_ref(&resolved, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert("mode_index".to_owned(), Value::from(action.mode_index));
            semantic.insert("mode_count".to_owned(), Value::from(action.mode_count));
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
        }
        FlatScorerActionKindV1::ChooseEffectOption => {
            if action.option_count < 2 || action.option_index >= action.option_count {
                return Err(NativeFlatTensorErrorV1::InvalidActionRange);
            }
            expected.option_index = action.option_index;
            expected.option_count = action.option_count;
            let source = require_only_ref(&resolved, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert("option_index".to_owned(), Value::from(action.option_index));
            semantic.insert("option_count".to_owned(), Value::from(action.option_count));
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
        }
        FlatScorerActionKindV1::ChooseEffectTarget => {
            if action.min_targets > action.max_targets
                || action.selected_count >= action.max_targets
            {
                return Err(NativeFlatTensorErrorV1::InvalidActionRange);
            }
            expected.selected_count = action.selected_count;
            expected.min_targets = action.min_targets;
            expected.max_targets = action.max_targets;
            expected.target_kind = action.target_kind;
            expected.target_player = action.target_player;
            let source = require_singular_ref(&resolved, 0, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert(
                "selected_count".to_owned(),
                Value::from(action.selected_count),
            );
            semantic.insert("min_targets".to_owned(), Value::from(action.min_targets));
            semantic.insert("max_targets".to_owned(), Value::from(action.max_targets));
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
            let (target, target_ref) = canonical_target_v1(action, &resolved, 1)?;
            semantic.insert("target".to_owned(), target);
            if let Some(target_ref) = target_ref {
                projected_refs.push(projected_singular(target_ref, ROLE_TARGET_OBJECT_V1));
            }
        }
        FlatScorerActionKindV1::FinishEffectSelection
        | FlatScorerActionKindV1::FinishTargetSelection => {
            expected.selected_count = action.selected_count;
            let source = require_only_ref(&resolved, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert(
                "selected_count".to_owned(),
                Value::from(action.selected_count),
            );
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
        }
        FlatScorerActionKindV1::ChooseEffectColor => {
            if !(1..=6).contains(&action.color) {
                return Err(NativeFlatTensorErrorV1::InvalidActionRange);
            }
            expected.color = action.color;
            let source = require_only_ref(&resolved, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert(
                "color".to_owned(),
                Value::String(one_based_name(action.color, &MANA_COLORS_V1)?.to_owned()),
            );
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
        }
        FlatScorerActionKindV1::ChooseEffectNumber => {
            if action.minimum > action.maximum
                || action.number < action.minimum
                || action.number > action.maximum
            {
                return Err(NativeFlatTensorErrorV1::InvalidActionRange);
            }
            expected.number = action.number;
            expected.minimum = action.minimum;
            expected.maximum = action.maximum;
            let source = require_only_ref(&resolved, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert("number".to_owned(), Value::from(action.number));
            semantic.insert("minimum".to_owned(), Value::from(action.minimum));
            semantic.insert("maximum".to_owned(), Value::from(action.maximum));
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
        }
        FlatScorerActionKindV1::ChooseEffectBoolean => {
            validate_only_flag(action.flags, FLAT_ACTION_FLAG_VALUE_V1)?;
            expected.flags = action.flags;
            let source = require_only_ref(&resolved, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert(
                "value".to_owned(),
                Value::Bool(action.flags & FLAT_ACTION_FLAG_VALUE_V1 != 0),
            );
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
        }
        FlatScorerActionKindV1::ChooseOptionalCostUse => {
            validate_only_flag(action.flags, FLAT_ACTION_FLAG_USE_COST_V1)?;
            expected.flags = action.flags;
            require_no_refs(&resolved)?;
            semantic.insert(
                "use_cost".to_owned(),
                Value::Bool(action.flags & FLAT_ACTION_FLAG_USE_COST_V1 != 0),
            );
        }
        FlatScorerActionKindV1::ChooseOptionalCostWhich => {
            if !(1..=3).contains(&action.optional_cost_choice) {
                return Err(NativeFlatTensorErrorV1::InvalidActionRange);
            }
            expected.optional_cost_choice = action.optional_cost_choice;
            require_no_refs(&resolved)?;
            semantic.insert(
                "choice".to_owned(),
                Value::String(
                    one_based_name(action.optional_cost_choice, &OPTIONAL_COST_CHOICES_V1)?
                        .to_owned(),
                ),
            );
        }
        FlatScorerActionKindV1::ChooseSpellCopyRetarget => {
            validate_only_flag(action.flags, FLAT_ACTION_FLAG_CHANGE_TARGET_V1)?;
            expected.flags = action.flags;
            let source = require_only_ref(&resolved, ROLE_SOURCE_V1)?;
            semantic.insert("source".to_owned(), canonical_card_ref_v1(source)?);
            semantic.insert(
                "change_target".to_owned(),
                Value::Bool(action.flags & FLAT_ACTION_FLAG_CHANGE_TARGET_V1 != 0),
            );
            projected_refs.push(projected_singular(source, ROLE_SOURCE_V1));
        }
        FlatScorerActionKindV1::ChooseMadnessCast => {
            validate_only_flag(action.flags, FLAT_ACTION_FLAG_CAST_IT_V1)?;
            expected.flags = action.flags;
            let card = require_only_ref(&resolved, ROLE_CARD_V1)?;
            semantic.insert("card".to_owned(), canonical_card_ref_v1(card)?);
            semantic.insert(
                "cast_it".to_owned(),
                Value::Bool(action.flags & FLAT_ACTION_FLAG_CAST_IT_V1 != 0),
            );
            projected_refs.push(projected_singular(card, ROLE_CARD_V1));
        }
        FlatScorerActionKindV1::Discard => {
            if resolved.is_empty() {
                return Err(NativeFlatTensorErrorV1::ActionReferenceShape);
            }
            let mut semantic_order = order_indexed_refs(&resolved, ROLE_CARDS_V1)?;
            let mut cards = semantic_order
                .iter()
                .map(|reference| canonical_card_ref_v1(*reference))
                .collect::<Result<Vec<_>, _>>()?;
            cards.sort_by(|left, right| {
                canonical_value_bytes(left).cmp(&canonical_value_bytes(right))
            });
            semantic.insert("cards".to_owned(), Value::Array(cards));
            semantic_order.sort_by_key(|reference| {
                projected_node_index_v2(reference.raw.model_object_index, projection)
                    .unwrap_or(usize::MAX)
            });
            if semantic_order.iter().any(|reference| {
                projected_node_index_v2(reference.raw.model_object_index, projection).is_err()
            }) {
                return Err(NativeFlatTensorErrorV1::ActionReferenceObject);
            }
            for (order, reference) in semantic_order.into_iter().enumerate() {
                projected_refs.push(ProjectedActionRefV1 {
                    resolved: reference,
                    role: ROLE_CARDS_V1,
                    order_index: u16::try_from(order)
                        .map_err(|_| NativeFlatTensorErrorV1::CheckedIntegerRange)?,
                    associated_order: 0,
                });
            }
        }
        FlatScorerActionKindV1::ChooseAttackerInclusion => {
            validate_only_flag(action.flags, FLAT_ACTION_FLAG_INCLUDE_V1)?;
            expected.flags = action.flags;
            let attacker = require_only_ref(&resolved, ROLE_ATTACKER_V1)?;
            semantic.insert("attacker".to_owned(), canonical_card_ref_v1(attacker)?);
            semantic.insert(
                "include".to_owned(),
                Value::Bool(action.flags & FLAT_ACTION_FLAG_INCLUDE_V1 != 0),
            );
            projected_refs.push(projected_singular(attacker, ROLE_ATTACKER_V1));
        }
        FlatScorerActionKindV1::ChooseBlockerInclusion => {
            validate_only_flag(action.flags, FLAT_ACTION_FLAG_INCLUDE_V1)?;
            expected.flags = action.flags;
            let attacker = require_singular_ref(&resolved, 0, ROLE_ATTACKER_V1)?;
            let blocker = require_singular_ref(&resolved, 1, ROLE_BLOCKER_V1)?;
            require_ref_count(&resolved, 2)?;
            semantic.insert("attacker".to_owned(), canonical_card_ref_v1(attacker)?);
            semantic.insert("blocker".to_owned(), canonical_card_ref_v1(blocker)?);
            semantic.insert(
                "include".to_owned(),
                Value::Bool(action.flags & FLAT_ACTION_FLAG_INCLUDE_V1 != 0),
            );
            projected_refs.push(projected_singular(attacker, ROLE_ATTACKER_V1));
            projected_refs.push(projected_singular(blocker, ROLE_BLOCKER_V1));
        }
        FlatScorerActionKindV1::OrderTriggers => {
            if resolved.is_empty() || resolved.len() > MAX_TRIGGER_REFS_V1 {
                return Err(NativeFlatTensorErrorV1::InvalidTriggerOrder);
            }
            let ordered = order_indexed_refs(&resolved, ROLE_PENDING_SOURCES_V1)?;
            let mut seen = 0u8;
            let mut pending = Vec::with_capacity(ordered.len());
            let mut order = Vec::with_capacity(ordered.len());
            for reference in ordered {
                let associated = usize::from(reference.raw.associated_order);
                if associated >= resolved.len() {
                    return Err(NativeFlatTensorErrorV1::InvalidTriggerOrder);
                }
                let bit = 1u8 << associated;
                if seen & bit != 0 {
                    return Err(NativeFlatTensorErrorV1::InvalidTriggerOrder);
                }
                seen |= bit;
                pending.push(canonical_card_ref_v1(reference)?);
                order.push(Value::from(reference.raw.associated_order));
                projected_refs.push(ProjectedActionRefV1 {
                    resolved: reference,
                    role: ROLE_PENDING_SOURCES_V1,
                    order_index: reference.raw.order_index,
                    associated_order: reference.raw.associated_order,
                });
            }
            let expected_seen = ((1u16 << resolved.len()) - 1) as u8;
            if seen != expected_seen {
                return Err(NativeFlatTensorErrorV1::InvalidTriggerOrder);
            }
            semantic.insert("pending_sources".to_owned(), Value::Array(pending));
            semantic.insert("order".to_owned(), Value::Array(order));
        }
    }

    if *action != expected {
        return Err(NativeFlatTensorErrorV1::NonCanonicalActionCore);
    }
    if projected_refs.len() != resolved.len() {
        return Err(NativeFlatTensorErrorV1::ActionReferenceShape);
    }
    let canonical_json = canonical_action_json_v1(semantic)?;
    let (sha512_blocks, hash_features) = action_hash_features_v1(&canonical_json);
    let mut features = explicit_action_features_v1(action, &resolved)?;
    features[NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V1..].copy_from_slice(&hash_features);

    let mut ref_features = try_vec_capacity(projected_refs.len())?;
    let mut ref_card_ids = try_vec_capacity(projected_refs.len())?;
    let mut ref_node_indices = try_vec_capacity(projected_refs.len())?;
    for reference in projected_refs {
        ref_features.push(action_ref_features_v1(reference)?);
        ref_card_ids.push(i64::from(reference.resolved.raw.card_token));
        ref_node_indices.push(
            i64::try_from(projected_node_index_v2(
                reference.resolved.raw.model_object_index,
                projection,
            )?)
            .map_err(|_| NativeFlatTensorErrorV1::CheckedIntegerRange)?,
        );
    }
    Ok(EncodedActionV1 {
        canonical_json,
        sha512_blocks,
        features,
        ref_features,
        ref_card_ids,
        ref_node_indices,
    })
}

fn projected_node_index_v2(
    raw_index: u32,
    projection: Option<&ObjectProjectionV2>,
) -> Result<usize, NativeFlatTensorErrorV2> {
    let raw_index =
        usize::try_from(raw_index).map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
    match projection {
        None => Ok(raw_index),
        Some(projection) => projection
            .raw_to_node
            .get(raw_index)
            .copied()
            .flatten()
            .ok_or(NativeFlatTensorErrorV2::ActionReferenceObject),
    }
}

fn resolve_action_refs_v1<'a>(
    decision: FlatScoringDecisionViewV1<'a>,
    action_index: usize,
    refs: &'a [FlatScorerActionRefV1],
) -> Result<Vec<ResolvedActionRefV1<'a>>, NativeFlatTensorErrorV1> {
    let mut out = try_vec_capacity(refs.len())?;
    for reference in refs {
        if usize::try_from(reference.action_index).ok() != Some(action_index)
            || reference.card_token == 0
        {
            return Err(NativeFlatTensorErrorV1::ActionReferenceObject);
        }
        let object_index = usize::try_from(reference.model_object_index)
            .map_err(|_| NativeFlatTensorErrorV1::CheckedIntegerRange)?;
        let object = decision
            .objects()
            .get(object_index)
            .ok_or(NativeFlatTensorErrorV1::ActionReferenceObject)?;
        if object.card_token != reference.card_token
            || object.owner == FlatRelativePlayerV1::None
            || object.controller == FlatRelativePlayerV1::None
            || object.zone.is_none()
        {
            return Err(NativeFlatTensorErrorV1::ActionReferenceObject);
        }
        out.push(ResolvedActionRefV1 {
            raw: reference,
            object,
        });
    }
    Ok(out)
}

fn require_no_refs(refs: &[ResolvedActionRefV1<'_>]) -> Result<(), NativeFlatTensorErrorV1> {
    require_ref_count(refs, 0)
}

fn require_ref_count(
    refs: &[ResolvedActionRefV1<'_>],
    expected: usize,
) -> Result<(), NativeFlatTensorErrorV1> {
    if refs.len() == expected {
        Ok(())
    } else {
        Err(NativeFlatTensorErrorV1::ActionReferenceShape)
    }
}

fn require_only_ref<'a>(
    refs: &[ResolvedActionRefV1<'a>],
    role: u8,
) -> Result<ResolvedActionRefV1<'a>, NativeFlatTensorErrorV1> {
    require_ref_count(refs, 1)?;
    require_singular_ref(refs, 0, role)
}

fn require_singular_ref<'a>(
    refs: &[ResolvedActionRefV1<'a>],
    index: usize,
    role: u8,
) -> Result<ResolvedActionRefV1<'a>, NativeFlatTensorErrorV1> {
    let reference = refs
        .get(index)
        .copied()
        .ok_or(NativeFlatTensorErrorV1::ActionReferenceShape)?;
    if reference.raw.projection_role_id != role
        || reference.raw.order_index != 0
        || reference.raw.associated_order != 0
    {
        return Err(NativeFlatTensorErrorV1::ActionReferenceShape);
    }
    Ok(reference)
}

fn projected_singular(resolved: ResolvedActionRefV1<'_>, role: u8) -> ProjectedActionRefV1<'_> {
    ProjectedActionRefV1 {
        resolved,
        role,
        order_index: 0,
        associated_order: 0,
    }
}

fn order_indexed_refs<'a>(
    refs: &[ResolvedActionRefV1<'a>],
    role: u8,
) -> Result<Vec<ResolvedActionRefV1<'a>>, NativeFlatTensorErrorV1> {
    if refs.iter().any(|reference| {
        reference.raw.projection_role_id != role || reference.raw.associated_order != 0
    }) && role != ROLE_PENDING_SOURCES_V1
    {
        return Err(NativeFlatTensorErrorV1::ActionReferenceShape);
    }
    if refs
        .iter()
        .any(|reference| reference.raw.projection_role_id != role)
    {
        return Err(NativeFlatTensorErrorV1::ActionReferenceShape);
    }
    let mut ordered = refs.to_vec();
    ordered.sort_by_key(|reference| reference.raw.order_index);
    if ordered
        .iter()
        .enumerate()
        .any(|(index, reference)| usize::from(reference.raw.order_index) != index)
    {
        return Err(if role == ROLE_PENDING_SOURCES_V1 {
            NativeFlatTensorErrorV1::InvalidTriggerOrder
        } else {
            NativeFlatTensorErrorV1::ActionReferenceShape
        });
    }
    Ok(ordered)
}

fn validate_only_flag(flags: u16, allowed: u16) -> Result<(), NativeFlatTensorErrorV1> {
    if flags & !allowed == 0 {
        Ok(())
    } else {
        Err(NativeFlatTensorErrorV1::NonCanonicalActionCore)
    }
}

fn canonical_target_v1<'a>(
    action: &FlatScorerActionCoreV1,
    refs: &[ResolvedActionRefV1<'a>],
    object_ref_index: usize,
) -> Result<(Value, Option<ResolvedActionRefV1<'a>>), NativeFlatTensorErrorV1> {
    let mut target = Map::new();
    match action.target_kind {
        1 => {
            if !(1..=2).contains(&action.target_player) {
                return Err(NativeFlatTensorErrorV1::InvalidActionRange);
            }
            require_ref_count(refs, object_ref_index)?;
            target.insert(
                "player".to_owned(),
                Value::String(relative_player_name_v1(action.target_player - 1)?.to_owned()),
            );
            target.insert("target_kind".to_owned(), Value::String("player".to_owned()));
            Ok((Value::Object(target), None))
        }
        2 => {
            if action.target_player != 0 {
                return Err(NativeFlatTensorErrorV1::InvalidActionRange);
            }
            let reference = require_singular_ref(refs, object_ref_index, ROLE_TARGET_OBJECT_V1)?;
            require_ref_count(refs, object_ref_index + 1)?;
            target.insert("object".to_owned(), canonical_card_ref_v1(reference)?);
            target.insert("target_kind".to_owned(), Value::String("object".to_owned()));
            Ok((Value::Object(target), Some(reference)))
        }
        _ => Err(NativeFlatTensorErrorV1::InvalidActionRange),
    }
}

fn canonical_card_ref_v1(
    reference: ResolvedActionRefV1<'_>,
) -> Result<Value, NativeFlatTensorErrorV1> {
    let mut out = Map::new();
    out.insert(
        "card_db_id".to_owned(),
        Value::from(reference.raw.card_token - 1),
    );
    out.insert(
        "controller".to_owned(),
        Value::String(relative_player_v1(reference.object.controller)?.to_owned()),
    );
    out.insert(
        "owner".to_owned(),
        Value::String(relative_player_v1(reference.object.owner)?.to_owned()),
    );
    out.insert(
        "zone".to_owned(),
        Value::String(
            zone_name_v1(
                reference
                    .object
                    .zone
                    .ok_or(NativeFlatTensorErrorV1::ActionReferenceObject)?,
            )
            .to_owned(),
        ),
    );
    Ok(Value::Object(out))
}

fn canonical_action_json_v1(
    semantic: Map<String, Value>,
) -> Result<Vec<u8>, NativeFlatTensorErrorV1> {
    let mut outer = BTreeMap::new();
    outer.insert("semantic", Value::Object(semantic));
    serde_json::to_vec(&outer).map_err(|_| NativeFlatTensorErrorV1::CanonicalJson)
}

fn canonical_value_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("canonical action values contain only serializable scalars")
}

fn action_hash_features_v1(
    canonical_json: &[u8],
) -> (
    [[u8; ACTION_HASH_BLOCK_BYTES_V1]; ACTION_HASH_BLOCK_COUNT_V1],
    [f32; NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V1],
) {
    let mut blocks = [[0u8; ACTION_HASH_BLOCK_BYTES_V1]; ACTION_HASH_BLOCK_COUNT_V1];
    let mut features = [0.0f32; NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V1];
    let mut feature_index = 0usize;
    for (counter, block) in blocks.iter_mut().enumerate() {
        let mut digest = Sha512::new();
        digest.update(ACTION_HASH_NAMESPACE_V1);
        digest.update((counter as u32).to_le_bytes());
        digest.update(canonical_json);
        block.copy_from_slice(&digest.finalize());
        for chunk in block.chunks_exact(4) {
            let integer = u32::from_le_bytes(chunk.try_into().expect("four-byte chunk"));
            let unit = f64::from(integer) / f64::from(u32::MAX);
            let signed = unit * 2.0 - 1.0;
            features[feature_index] = signed as f32;
            feature_index += 1;
        }
    }
    debug_assert_eq!(feature_index, features.len());
    (blocks, features)
}

fn explicit_action_features_v1(
    action: &FlatScorerActionCoreV1,
    refs: &[ResolvedActionRefV1<'_>],
) -> Result<[f32; NATIVE_FLAT_ACTION_FEATURE_DIM_V1], NativeFlatTensorErrorV1> {
    let mut out = [0.0f32; NATIVE_FLAT_ACTION_FEATURE_DIM_V1];
    let kind = action.kind as usize;
    if kind >= 27 {
        return Err(NativeFlatTensorErrorV1::InvalidActionRange);
    }
    out[kind] = 1.0;
    out[27] = 1.0;
    let source_like = [
        ROLE_SOURCE_V1,
        ROLE_CANDIDATE_V1,
        ROLE_CARD_V1,
        ROLE_ATTACKER_V1,
    ]
    .into_iter()
    .find_map(|role| {
        refs.iter()
            .find(|reference| reference.raw.projection_role_id == role)
            .copied()
    });
    card_ref_features_v1(source_like, &mut out[30..43])?;
    match action.target_kind {
        0 => {
            out[47] = 1.0;
        }
        1 => {
            out[43] = 1.0;
            match action.target_player {
                1 => out[45] = 1.0,
                2 => out[46] = 1.0,
                _ => return Err(NativeFlatTensorErrorV1::InvalidActionRange),
            }
        }
        2 => {
            out[44] = 1.0;
            out[47] = 1.0;
        }
        _ => return Err(NativeFlatTensorErrorV1::InvalidActionRange),
    }
    out[48] = scaled_number_v1(i64::from(action.ability_index), 8.0);
    out[49] = scaled_number_v1(i64::from(action.remaining), 8.0);
    out[50] = scaled_number_v1(i64::from(action.mode_index), 8.0);
    out[51] = scaled_number_v1(i64::from(action.mode_count), 8.0);
    out[52] = scaled_number_v1(i64::from(action.option_index), 16.0);
    out[53] = scaled_number_v1(i64::from(action.option_count), 16.0);
    out[54] = flag_feature(action.flags, FLAT_ACTION_FLAG_PAY_V1);
    out[55] = flag_feature(action.flags, FLAT_ACTION_FLAG_CHANGE_TARGET_V1);
    out[56] = flag_feature(action.flags, FLAT_ACTION_FLAG_USE_COST_V1);
    out[57] = flag_feature(action.flags, FLAT_ACTION_FLAG_CAST_IT_V1);
    let cards = refs
        .iter()
        .filter(|reference| reference.raw.projection_role_id == ROLE_CARDS_V1)
        .count();
    let pending = refs
        .iter()
        .filter(|reference| reference.raw.projection_role_id == ROLE_PENDING_SOURCES_V1)
        .count();
    out[58] = scaled_number_v1(
        i64::try_from(cards).map_err(|_| NativeFlatTensorErrorV1::CheckedIntegerRange)?,
        8.0,
    );
    out[59] = 0.0;
    out[60] = 0.0;
    out[61] = scaled_number_v1(
        i64::try_from(pending).map_err(|_| NativeFlatTensorErrorV1::CheckedIntegerRange)?,
        16.0,
    );
    out[62] = out[61];
    out[63] = scaled_number_v1(i64::from(action.selected_count), 16.0);
    out[64] = scaled_number_v1(i64::from(action.min_targets), 16.0);
    out[65] = scaled_number_v1(i64::from(action.max_targets), 16.0);
    out[66] = scaled_number_v1(i64::from(action.number), 16.0);
    out[67] = scaled_number_v1(i64::from(action.minimum), 16.0);
    out[68] = scaled_number_v1(i64::from(action.maximum), 16.0);
    out[69] = flag_feature(action.flags, FLAT_ACTION_FLAG_VALUE_V1);
    if action.mana_choice != 0 {
        let index = usize::from(action.mana_choice - 1);
        if index >= MANA_COLORS_V1.len() {
            return Err(NativeFlatTensorErrorV1::InvalidActionRange);
        }
        out[70] = 1.0;
        out[71 + index] = 1.0;
    }
    if action.color != 0 {
        let index = usize::from(action.color - 1);
        if index >= MANA_COLORS_V1.len() {
            return Err(NativeFlatTensorErrorV1::InvalidActionRange);
        }
        out[77 + index] = 1.0;
    }
    let cast_mode = if action.cast_mode == 0 {
        0
    } else {
        usize::from(action.cast_mode - 1)
    };
    if cast_mode >= CAST_MODES_V1.len() {
        return Err(NativeFlatTensorErrorV1::InvalidActionRange);
    }
    out[83 + cast_mode] = 1.0;
    let cost_kind = if action.cost_kind == 0 {
        0
    } else {
        usize::from(action.cost_kind - 1)
    };
    if cost_kind >= COST_KINDS_V1.len() {
        return Err(NativeFlatTensorErrorV1::InvalidActionRange);
    }
    out[85 + cost_kind] = 1.0;
    let choice = if action.optional_cost_choice == 0 {
        0
    } else {
        usize::from(action.optional_cost_choice - 1)
    };
    if choice >= OPTIONAL_COST_CHOICES_V1.len() {
        return Err(NativeFlatTensorErrorV1::InvalidActionRange);
    }
    out[96 + choice] = 1.0;
    Ok(out)
}

fn action_ref_features_v1(
    reference: ProjectedActionRefV1<'_>,
) -> Result<[f32; NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1], NativeFlatTensorErrorV1> {
    let mut out = [0.0f32; NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1];
    out[usize::from(reference.role)] = 1.0;
    card_ref_features_v1(Some(reference.resolved), &mut out[10..23])?;
    out[23] = scaled_number_v1(i64::from(reference.order_index), 32.0);
    out[24] = scaled_number_v1(i64::from(reference.associated_order), 32.0);
    Ok(out)
}

fn card_ref_features_v1(
    reference: Option<ResolvedActionRefV1<'_>>,
    out: &mut [f32],
) -> Result<(), NativeFlatTensorErrorV1> {
    debug_assert_eq!(out.len(), 13);
    out.fill(0.0);
    let Some(reference) = reference else {
        out[2] = 1.0;
        out[5] = 1.0;
        return Ok(());
    };
    match reference.object.owner {
        FlatRelativePlayerV1::SelfPlayer => out[0] = 1.0,
        FlatRelativePlayerV1::Opponent => out[1] = 1.0,
        FlatRelativePlayerV1::None => return Err(NativeFlatTensorErrorV1::ActionReferenceObject),
    }
    match reference.object.controller {
        FlatRelativePlayerV1::SelfPlayer => out[3] = 1.0,
        FlatRelativePlayerV1::Opponent => out[4] = 1.0,
        FlatRelativePlayerV1::None => return Err(NativeFlatTensorErrorV1::ActionReferenceObject),
    }
    let zone = reference
        .object
        .zone
        .ok_or(NativeFlatTensorErrorV1::ActionReferenceObject)? as usize;
    if zone >= 7 {
        return Err(NativeFlatTensorErrorV1::ActionReferenceObject);
    }
    out[6 + zone] = 1.0;
    Ok(())
}

fn scaled_number_v1(value: i64, scale: f64) -> f32 {
    (value as f64 / scale) as f32
}

fn flag_feature(flags: u16, flag: u16) -> f32 {
    if flags & flag == 0 {
        0.0
    } else {
        1.0
    }
}

fn optional_one_based_name<const N: usize>(value: u8, values: &[&str; N]) -> Value {
    if value == 0 {
        Value::Null
    } else {
        Value::String(values[usize::from(value - 1)].to_owned())
    }
}

fn one_based_name<'a, const N: usize>(
    value: u8,
    values: &'a [&str; N],
) -> Result<&'a str, NativeFlatTensorErrorV1> {
    value
        .checked_sub(1)
        .and_then(|index| values.get(usize::from(index)).copied())
        .ok_or(NativeFlatTensorErrorV1::InvalidActionRange)
}

fn relative_player_v1(
    player: FlatRelativePlayerV1,
) -> Result<&'static str, NativeFlatTensorErrorV1> {
    match player {
        FlatRelativePlayerV1::SelfPlayer => Ok("self"),
        FlatRelativePlayerV1::Opponent => Ok("opponent"),
        FlatRelativePlayerV1::None => Err(NativeFlatTensorErrorV1::ActionReferenceObject),
    }
}

fn relative_player_name_v1(player: u8) -> Result<&'static str, NativeFlatTensorErrorV1> {
    match player {
        0 => Ok("self"),
        1 => Ok("opponent"),
        _ => Err(NativeFlatTensorErrorV1::InvalidActionRange),
    }
}

fn zone_name_v1(zone: FlatZoneV1) -> &'static str {
    match zone {
        FlatZoneV1::Library => "Library",
        FlatZoneV1::Hand => "Hand",
        FlatZoneV1::Battlefield => "Battlefield",
        FlatZoneV1::Graveyard => "Graveyard",
        FlatZoneV1::Stack => "Stack",
        FlatZoneV1::Exile => "Exile",
        FlatZoneV1::Command => "Command",
    }
}

fn action_kind_name_v1(kind: FlatScorerActionKindV1) -> &'static str {
    match kind {
        FlatScorerActionKindV1::Pass => "pass",
        FlatScorerActionKindV1::PlayLand => "play_land",
        FlatScorerActionKindV1::CastSpell => "cast_spell",
        FlatScorerActionKindV1::ActivateManaAbility => "activate_mana_ability",
        FlatScorerActionKindV1::ActivateAbility => "activate_ability",
        FlatScorerActionKindV1::PlotSpell => "plot_spell",
        FlatScorerActionKindV1::ChooseTarget => "choose_target",
        FlatScorerActionKindV1::ChooseCostTarget => "choose_cost_target",
        FlatScorerActionKindV1::ChooseCastMode => "choose_cast_mode",
        FlatScorerActionKindV1::ChooseKicker => "choose_kicker",
        FlatScorerActionKindV1::ChooseSpellMode => "choose_spell_mode",
        FlatScorerActionKindV1::ChooseEffectOption => "choose_effect_option",
        FlatScorerActionKindV1::ChooseEffectTarget => "choose_effect_target",
        FlatScorerActionKindV1::FinishEffectSelection => "finish_effect_selection",
        FlatScorerActionKindV1::ChooseEffectColor => "choose_effect_color",
        FlatScorerActionKindV1::ChooseEffectNumber => "choose_effect_number",
        FlatScorerActionKindV1::ChooseEffectBoolean => "choose_effect_boolean",
        FlatScorerActionKindV1::FinishTargetSelection => "finish_target_selection",
        FlatScorerActionKindV1::ChooseOptionalCostUse => "choose_optional_cost_use",
        FlatScorerActionKindV1::ChooseOptionalCostWhich => "choose_optional_cost_which",
        FlatScorerActionKindV1::ChooseSpellCopyPayment => "choose_spell_copy_payment",
        FlatScorerActionKindV1::ChooseSpellCopyRetarget => "choose_spell_copy_retarget",
        FlatScorerActionKindV1::ChooseMadnessCast => "choose_madness_cast",
        FlatScorerActionKindV1::Discard => "discard",
        FlatScorerActionKindV1::ChooseAttackerInclusion => "choose_attacker_inclusion",
        FlatScorerActionKindV1::ChooseBlockerInclusion => "choose_blocker_inclusion",
        FlatScorerActionKindV1::OrderTriggers => "order_triggers",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flat_policy_v2::{
        encode_observation_owned_tables_for_fixture_v2, FlatCompletedDungeonV2,
        FlatContextPathElementV2, FlatDecisionEncoderV2, FlatEffectSubtypeChangeV2, FlatGlobalsV2,
        FlatManaColorV2 as FlatManaColorV1, FlatScoringOwnedBuffersV2,
    };
    use crate::rl::{
        make_legal_action_v5, ActionSemanticV1, KnownLibraryCardV4, ObjectRelationPublicV4,
        ObservationV5, PlayerSeatV1,
    };
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
    use crate::state::Zone;
    use serde::{Deserialize, Serialize};
    use sha2::Sha256;

    type FlatGlobalsV1 = FlatGlobalsV2;

    const GOLDEN: &str = include_str!("../../data/flat_policy_v2/python_action_features_v2.json");
    const FULL_GOLDEN: &str =
        include_str!("../../data/flat_policy_v2/python_full_features_v2.json");
    const FEATURES_PY: &[u8] = include_bytes!("../../python/mtg_kernel_rl/features.py");

    #[derive(Deserialize)]
    struct GoldenDoc {
        authority_sha256: String,
        payload_sha256: String,
        dimensions: GoldenDimensions,
        current_rust_card_token_max: u32,
        domain_coverage_blockers: Vec<Value>,
        cases: Vec<GoldenCase>,
    }

    #[derive(Deserialize)]
    struct GoldenDimensions {
        action_explicit: usize,
        action_hash: usize,
        action: usize,
        action_ref: usize,
    }

    #[derive(Deserialize)]
    struct GoldenCase {
        name: String,
        coverage: Vec<String>,
        flat_input: FlatInput,
        canonical_json: String,
        sha512_blocks_hex: Vec<String>,
        full_feature_f32_le_hex: String,
        action_ref_feature_f32_le_hex: String,
        action_ref_card_ids: Vec<i64>,
        action_ref_node_indices: Vec<i64>,
    }

    #[derive(Deserialize)]
    struct FlatInput {
        core: CoreFixture,
        objects: Vec<ObjectFixture>,
        refs: Vec<RefFixture>,
    }

    #[derive(Deserialize)]
    struct CoreFixture {
        kind: String,
        flags: u16,
        ability_index: u8,
        remaining: u8,
        mode_index: u8,
        mode_count: u8,
        option_index: u16,
        option_count: u16,
        selected_count: u16,
        min_targets: u16,
        max_targets: u16,
        number: i32,
        minimum: i32,
        maximum: i32,
        mana_choice: u8,
        color: u8,
        cast_mode: u8,
        cost_kind: u8,
        optional_cost_choice: u8,
        target_kind: u8,
        target_player: u8,
        ref_start: u32,
        ref_len: u16,
    }

    #[derive(Deserialize)]
    struct ObjectFixture {
        card_token: u32,
        owner: u8,
        controller: u8,
        zone: u8,
    }

    #[derive(Deserialize)]
    struct RefFixture {
        action_index: u32,
        projection_role_id: u8,
        order_index: u16,
        associated_order: u16,
        card_token: u32,
        model_object_index: u32,
    }

    #[derive(Deserialize)]
    struct FullGoldenDoc {
        authority_sha256: String,
        payload_sha256: String,
        dimensions: FullGoldenDimensions,
        cases: Vec<FullGoldenCase>,
    }

    #[derive(Deserialize)]
    struct FullGoldenDimensions {
        state: usize,
        object: usize,
        edge: usize,
        action: usize,
        action_ref: usize,
        object_groups: usize,
    }

    #[derive(Deserialize)]
    struct FullGoldenCase {
        name: String,
        canonical_observation_json: String,
        state_sha512_blocks_hex: Vec<String>,
        rust_fixture: FullRustFixture,
        tensors: FullGoldenTensors,
    }

    #[derive(Deserialize)]
    struct FullRustFixture {
        episode_id: u64,
        environment_seed: u64,
        deck_ids: [String; 2],
        replay_selected_indices: Vec<u32>,
        fixture_transform: String,
        observation: Value,
        legal_actions: Value,
    }

    #[derive(Deserialize)]
    struct FullTensorPayload {
        shape: Vec<usize>,
        f32_le_hex: Option<String>,
        i64_values: Option<Vec<i64>>,
    }

    #[derive(Deserialize)]
    struct FullGoldenTensors {
        state: FullTensorPayload,
        object_features: FullTensorPayload,
        object_card_ids: FullTensorPayload,
        object_groups: FullTensorPayload,
        object_node_ids: FullTensorPayload,
        edge_features: FullTensorPayload,
        edge_source_indices: FullTensorPayload,
        edge_target_indices: FullTensorPayload,
        action_features: FullTensorPayload,
        action_ref_features: FullTensorPayload,
        action_ref_card_ids: FullTensorPayload,
        action_ref_action_indices: FullTensorPayload,
        action_ref_node_indices: FullTensorPayload,
    }

    fn golden() -> GoldenDoc {
        serde_json::from_str(GOLDEN).expect("Python action golden must parse")
    }

    fn full_golden() -> FullGoldenDoc {
        serde_json::from_str(FULL_GOLDEN).expect("Python full-feature golden must parse")
    }

    fn player(value: u8) -> FlatRelativePlayerV1 {
        match value {
            0 => FlatRelativePlayerV1::SelfPlayer,
            1 => FlatRelativePlayerV1::Opponent,
            _ => panic!("invalid relative player fixture"),
        }
    }

    fn zone(value: u8) -> FlatZoneV1 {
        match value {
            0 => FlatZoneV1::Library,
            1 => FlatZoneV1::Hand,
            2 => FlatZoneV1::Battlefield,
            3 => FlatZoneV1::Graveyard,
            4 => FlatZoneV1::Stack,
            5 => FlatZoneV1::Exile,
            6 => FlatZoneV1::Command,
            _ => panic!("invalid zone fixture"),
        }
    }

    fn kind(value: &str) -> FlatScorerActionKindV1 {
        match value {
            "pass" => FlatScorerActionKindV1::Pass,
            "play_land" => FlatScorerActionKindV1::PlayLand,
            "cast_spell" => FlatScorerActionKindV1::CastSpell,
            "activate_mana_ability" => FlatScorerActionKindV1::ActivateManaAbility,
            "activate_ability" => FlatScorerActionKindV1::ActivateAbility,
            "plot_spell" => FlatScorerActionKindV1::PlotSpell,
            "choose_target" => FlatScorerActionKindV1::ChooseTarget,
            "choose_cost_target" => FlatScorerActionKindV1::ChooseCostTarget,
            "choose_cast_mode" => FlatScorerActionKindV1::ChooseCastMode,
            "choose_kicker" => FlatScorerActionKindV1::ChooseKicker,
            "choose_spell_mode" => FlatScorerActionKindV1::ChooseSpellMode,
            "choose_effect_option" => FlatScorerActionKindV1::ChooseEffectOption,
            "choose_effect_target" => FlatScorerActionKindV1::ChooseEffectTarget,
            "finish_effect_selection" => FlatScorerActionKindV1::FinishEffectSelection,
            "choose_effect_color" => FlatScorerActionKindV1::ChooseEffectColor,
            "choose_effect_number" => FlatScorerActionKindV1::ChooseEffectNumber,
            "choose_effect_boolean" => FlatScorerActionKindV1::ChooseEffectBoolean,
            "finish_target_selection" => FlatScorerActionKindV1::FinishTargetSelection,
            "choose_optional_cost_use" => FlatScorerActionKindV1::ChooseOptionalCostUse,
            "choose_optional_cost_which" => FlatScorerActionKindV1::ChooseOptionalCostWhich,
            "choose_spell_copy_payment" => FlatScorerActionKindV1::ChooseSpellCopyPayment,
            "choose_spell_copy_retarget" => FlatScorerActionKindV1::ChooseSpellCopyRetarget,
            "choose_madness_cast" => FlatScorerActionKindV1::ChooseMadnessCast,
            "discard" => FlatScorerActionKindV1::Discard,
            "choose_attacker_inclusion" => FlatScorerActionKindV1::ChooseAttackerInclusion,
            "choose_blocker_inclusion" => FlatScorerActionKindV1::ChooseBlockerInclusion,
            "order_triggers" => FlatScorerActionKindV1::OrderTriggers,
            _ => panic!("unknown action-kind fixture {value}"),
        }
    }

    fn core(value: &CoreFixture) -> FlatScorerActionCoreV1 {
        FlatScorerActionCoreV1 {
            kind: kind(&value.kind),
            flags: value.flags,
            ability_index: value.ability_index,
            remaining: value.remaining,
            mode_index: value.mode_index,
            mode_count: value.mode_count,
            option_index: value.option_index,
            option_count: value.option_count,
            selected_count: value.selected_count,
            min_targets: value.min_targets,
            max_targets: value.max_targets,
            number: value.number,
            minimum: value.minimum,
            maximum: value.maximum,
            mana_choice: value.mana_choice,
            color: value.color,
            cast_mode: value.cast_mode,
            cost_kind: value.cost_kind,
            optional_cost_choice: value.optional_cost_choice,
            target_kind: value.target_kind,
            target_player: value.target_player,
            ref_start: value.ref_start,
            ref_len: value.ref_len,
        }
    }

    fn parts(
        value: &FlatInput,
    ) -> (
        FlatGlobalsV1,
        Vec<FlatObjectCoreV1>,
        Vec<FlatScorerActionCoreV1>,
        Vec<FlatScorerActionRefV1>,
    ) {
        let globals = FlatGlobalsV1 {
            acting_player: FlatRelativePlayerV1::SelfPlayer,
            ..FlatGlobalsV1::default()
        };
        let objects = value
            .objects
            .iter()
            .map(|object| FlatObjectCoreV1 {
                card_token: object.card_token,
                owner: player(object.owner),
                controller: player(object.controller),
                zone: Some(zone(object.zone)),
                ..FlatObjectCoreV1::default()
            })
            .collect();
        let refs = value
            .refs
            .iter()
            .map(|reference| FlatScorerActionRefV1 {
                action_index: reference.action_index,
                projection_role_id: reference.projection_role_id,
                order_index: reference.order_index,
                associated_order: reference.associated_order,
                card_token: reference.card_token,
                model_object_index: reference.model_object_index,
            })
            .collect();
        (globals, objects, vec![core(&value.core)], refs)
    }

    fn view<'a>(
        globals: &'a FlatGlobalsV1,
        objects: &'a [FlatObjectCoreV1],
        actions: &'a [FlatScorerActionCoreV1],
        refs: &'a [FlatScorerActionRefV1],
    ) -> FlatScoringDecisionViewV1<'a> {
        FlatScoringDecisionViewV1::new(
            globals,
            objects,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            actions,
            refs,
        )
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn u32_le_words_from_hex(value: &str) -> Vec<u32> {
        fn nibble(value: u8) -> u8 {
            match value {
                b'0'..=b'9' => value - b'0',
                b'a'..=b'f' => value - b'a' + 10,
                _ => panic!("golden hex must be lowercase ASCII"),
            }
        }

        assert_eq!(value.len() % 8, 0, "golden f32 hex must contain u32 words");
        value
            .as_bytes()
            .chunks_exact(8)
            .map(|word| {
                let mut bytes = [0u8; 4];
                for (index, pair) in word.chunks_exact(2).enumerate() {
                    bytes[index] = (nibble(pair[0]) << 4) | nibble(pair[1]);
                }
                u32::from_le_bytes(bytes)
            })
            .collect()
    }

    #[test]
    fn dimensions_and_python_authority_are_pinned() {
        let document = golden();
        assert_eq!(
            document.dimensions.action_explicit,
            NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V1
        );
        assert_eq!(
            document.dimensions.action_hash,
            NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V1
        );
        assert_eq!(
            document.dimensions.action,
            NATIVE_FLAT_ACTION_FEATURE_DIM_V1
        );
        assert_eq!(
            document.dimensions.action_ref,
            NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1
        );
        assert_eq!(hex(&Sha256::digest(FEATURES_PY)), document.authority_sha256);

        let mut payload: Value = serde_json::from_str(GOLDEN).unwrap();
        payload.as_object_mut().unwrap().remove("payload_sha256");
        let canonical = serde_json::to_vec(&payload).unwrap();
        assert_eq!(hex(&Sha256::digest(canonical)), document.payload_sha256);
    }

    #[test]
    fn every_python_action_golden_matches_bit_exactly() {
        let document = golden();
        assert_eq!(
            document
                .cases
                .iter()
                .filter(|case| case.coverage.iter().any(|value| value == "all-27-variants"))
                .count(),
            27
        );
        for case in &document.cases {
            let (globals, objects, actions, refs) = parts(&case.flat_input);
            let decision = view(&globals, &objects, &actions, &refs);
            let encoded = encode_action_v1(decision, 0, &actions[0], &refs, None)
                .unwrap_or_else(|error| panic!("{} failed to encode: {error:?}", case.name));
            assert_eq!(
                String::from_utf8(encoded.canonical_json.clone()).unwrap(),
                case.canonical_json,
                "{} canonical JSON",
                case.name
            );
            assert_eq!(encoded.canonical_json, case.canonical_json.as_bytes());
            assert_eq!(
                encoded
                    .sha512_blocks
                    .iter()
                    .map(|block| hex(block))
                    .collect::<Vec<_>>(),
                case.sha512_blocks_hex,
                "{} SHA-512 blocks",
                case.name
            );
            let full_bits = encoded
                .features
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>();
            let expected_full_bits = u32_le_words_from_hex(&case.full_feature_f32_le_hex);
            assert_eq!(full_bits, expected_full_bits, "{} full features", case.name);
            let expected_ref_bits = u32_le_words_from_hex(&case.action_ref_feature_f32_le_hex)
                .chunks_exact(NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1)
                .map(|row| row.to_vec())
                .collect::<Vec<_>>();
            assert_eq!(
                encoded
                    .ref_features
                    .iter()
                    .map(|row| row.iter().map(|value| value.to_bits()).collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
                expected_ref_bits,
                "{} ref features",
                case.name
            );
            assert_eq!(
                encoded.ref_card_ids, case.action_ref_card_ids,
                "{} ref tokens",
                case.name
            );
            assert_eq!(
                encoded.ref_node_indices, case.action_ref_node_indices,
                "{} ref nodes",
                case.name
            );

            let mut output = NativeFlatDecisionTensorV1::default();
            fill_native_flat_action_tensors_v1(decision, &mut output).unwrap();
            assert_eq!(
                output
                    .action_features
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                expected_full_bits,
                "{} owned features",
                case.name
            );
            assert_eq!(
                output
                    .action_ref_features
                    .chunks_exact(NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1)
                    .map(|row| row.iter().map(|value| value.to_bits()).collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
                expected_ref_bits,
                "{} owned ref features",
                case.name
            );
            assert_eq!(output.action_ref_card_ids, case.action_ref_card_ids);
            assert_eq!(
                output.action_ref_action_indices,
                vec![0; case.action_ref_card_ids.len()]
            );
            assert_eq!(output.action_ref_node_indices, case.action_ref_node_indices);
            validate_native_flat_action_half_v1(&output, 1, refs.len(), objects.len()).unwrap();
        }
    }

    #[test]
    fn card_token_65536_is_admitted_and_python_authoritative() {
        let document = golden();
        assert_eq!(
            document.current_rust_card_token_max,
            NATIVE_FLAT_CURRENT_MAX_CARD_TOKEN_V1
        );
        assert_eq!(NATIVE_FLAT_CURRENT_MAX_CARD_TOKEN_V1, 65_536);
        assert!(document.domain_coverage_blockers.is_empty());
        assert!(document
            .cases
            .iter()
            .any(|case| case.name == "card-token-65535"));
        let case = document
            .cases
            .iter()
            .find(|case| case.name == "card-token-65536")
            .expect("V2 golden must pin the widened token");
        assert_eq!(case.action_ref_card_ids, [65_536]);
        let (globals, objects, actions, refs) = parts(&case.flat_input);
        let mut output = NativeFlatDecisionTensorV2::default();
        fill_native_flat_action_tensors_v2(view(&globals, &objects, &actions, &refs), &mut output)
            .unwrap();
        assert_eq!(output.action_ref_card_ids, [65_536]);
    }

    #[test]
    fn inactive_core_fields_and_unknown_flags_fail_closed_transactionally() {
        let document = golden();
        let case = document
            .cases
            .iter()
            .find(|case| case.name == "primary-pass")
            .unwrap();
        let (globals, objects, actions, refs) = parts(&case.flat_input);
        let base = actions[0];
        let mut mutations = Vec::new();
        macro_rules! mutate {
            ($field:ident, $value:expr) => {{
                let mut row = base;
                row.$field = $value;
                mutations.push(row);
            }};
        }
        mutate!(flags, 1);
        mutate!(ability_index, 1);
        mutate!(remaining, 1);
        mutate!(mode_index, 1);
        mutate!(mode_count, 1);
        mutate!(option_index, 1);
        mutate!(option_count, 1);
        mutate!(selected_count, 1);
        mutate!(min_targets, 1);
        mutate!(max_targets, 1);
        mutate!(number, 1);
        mutate!(minimum, 1);
        mutate!(maximum, 1);
        mutate!(mana_choice, 1);
        mutate!(color, 1);
        mutate!(cast_mode, 1);
        mutate!(cost_kind, 1);
        mutate!(optional_cost_choice, 1);
        mutate!(target_kind, 1);
        mutate!(target_player, 1);
        for mutated in mutations {
            let actions = [mutated];
            let decision = view(&globals, &objects, &actions, &refs);
            let mut output = NativeFlatDecisionTensorV1 {
                action_features: vec![13.0],
                action_ref_features: vec![17.0],
                action_ref_card_ids: vec![19],
                action_ref_action_indices: vec![23],
                action_ref_node_indices: vec![29],
                ..NativeFlatDecisionTensorV1::default()
            };
            let before = output.clone();
            assert!(fill_native_flat_action_tensors_v1(decision, &mut output).is_err());
            assert_eq!(output, before);
        }
    }

    #[test]
    fn successful_fill_replaces_every_poisoned_action_output_field() {
        let document = golden();
        let case = document
            .cases
            .iter()
            .find(|case| case.name == "primary-choose_cost_target")
            .unwrap();
        let (globals, objects, actions, refs) = parts(&case.flat_input);
        let decision = view(&globals, &objects, &actions, &refs);

        let mut expected = NativeFlatDecisionTensorV1::default();
        fill_native_flat_action_tensors_v1(decision, &mut expected).unwrap();

        let mut reused = NativeFlatDecisionTensorV1 {
            action_features: vec![f32::NAN, 13.0],
            action_ref_features: vec![f32::INFINITY, 17.0],
            action_ref_card_ids: vec![-19],
            action_ref_action_indices: vec![-23],
            action_ref_node_indices: vec![-29],
            ..NativeFlatDecisionTensorV1::default()
        };
        fill_native_flat_action_tensors_v1(decision, &mut reused).unwrap();
        assert_eq!(reused, expected);
    }

    #[test]
    fn standalone_validator_rejects_a_forged_empty_action_half() {
        assert_eq!(
            validate_native_flat_action_half_v1(&NativeFlatDecisionTensorV1::default(), 0, 0, 0,),
            Err(NativeFlatTensorErrorV1::OutputInvariant)
        );
    }

    #[test]
    fn reference_role_token_object_and_trigger_permutation_mutations_fail_closed() {
        let document = golden();
        let singular = document
            .cases
            .iter()
            .find(|case| case.name == "primary-cast_spell")
            .unwrap();
        let (globals, objects, mut actions, mut refs) = parts(&singular.flat_input);
        actions[0].ref_len += 1;
        refs.push(refs[0]);
        assert_eq!(
            fill_native_flat_action_tensors_v1(
                view(&globals, &objects, &actions, &refs),
                &mut NativeFlatDecisionTensorV1::default()
            ),
            Err(NativeFlatTensorErrorV1::ActionReferenceShape)
        );

        let cost = document
            .cases
            .iter()
            .find(|case| case.name == "primary-choose_cost_target")
            .unwrap();
        let (globals, objects, actions, mut refs) = parts(&cost.flat_input);
        refs[1].projection_role_id = ROLE_SOURCE_V1;
        assert!(fill_native_flat_action_tensors_v1(
            view(&globals, &objects, &actions, &refs),
            &mut NativeFlatDecisionTensorV1::default()
        )
        .is_err());

        let (_, objects, actions, mut refs) = parts(&cost.flat_input);
        refs[0].card_token = refs[0].card_token.saturating_add(1);
        assert_eq!(
            fill_native_flat_action_tensors_v1(
                view(&globals, &objects, &actions, &refs),
                &mut NativeFlatDecisionTensorV1::default()
            ),
            Err(NativeFlatTensorErrorV1::ActionReferenceObject)
        );

        let trigger = document
            .cases
            .iter()
            .find(|case| case.name == "triggers-seven-permuted")
            .unwrap();
        let (globals, objects, actions, mut refs) = parts(&trigger.flat_input);
        refs[1].associated_order = refs[0].associated_order;
        assert_eq!(
            fill_native_flat_action_tensors_v1(
                view(&globals, &objects, &actions, &refs),
                &mut NativeFlatDecisionTensorV1::default()
            ),
            Err(NativeFlatTensorErrorV1::InvalidTriggerOrder)
        );
        let (_, objects, actions, mut refs) = parts(&trigger.flat_input);
        refs[0].order_index = 2;
        assert_eq!(
            fill_native_flat_action_tensors_v1(
                view(&globals, &objects, &actions, &refs),
                &mut NativeFlatDecisionTensorV1::default()
            ),
            Err(NativeFlatTensorErrorV1::InvalidTriggerOrder)
        );
    }

    #[test]
    fn multiple_actions_preserve_python_rows_and_bind_reference_action_indices() {
        let document = golden();
        let play = document
            .cases
            .iter()
            .find(|case| case.name == "primary-play_land")
            .unwrap();
        let cast = document
            .cases
            .iter()
            .find(|case| case.name == "primary-cast_spell")
            .unwrap();
        let (globals, objects, play_actions, play_refs) = parts(&play.flat_input);
        let (_, _, cast_actions, cast_refs) = parts(&cast.flat_input);
        let mut cast_action = cast_actions[0];
        cast_action.ref_start = 1;
        let actions = [play_actions[0], cast_action];
        let mut cast_ref = cast_refs[0];
        cast_ref.action_index = 1;
        let refs = [play_refs[0], cast_ref];

        let mut output = NativeFlatDecisionTensorV1::default();
        fill_native_flat_action_tensors_v1(view(&globals, &objects, &actions, &refs), &mut output)
            .unwrap();

        let expected_action_bits = u32_le_words_from_hex(&play.full_feature_f32_le_hex)
            .into_iter()
            .chain(u32_le_words_from_hex(&cast.full_feature_f32_le_hex))
            .collect::<Vec<_>>();
        assert_eq!(
            output
                .action_features
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            expected_action_bits
        );
        let expected_ref_bits = u32_le_words_from_hex(&play.action_ref_feature_f32_le_hex)
            .into_iter()
            .chain(u32_le_words_from_hex(&cast.action_ref_feature_f32_le_hex))
            .collect::<Vec<_>>();
        assert_eq!(
            output
                .action_ref_features
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            expected_ref_bits
        );
        assert_eq!(output.action_ref_card_ids, vec![41, 41]);
        assert_eq!(output.action_ref_action_indices, vec![0, 1]);
        assert_eq!(output.action_ref_node_indices, vec![0, 0]);
    }

    #[test]
    fn discard_hash_sort_and_ref_node_sort_are_independent() {
        let document = golden();
        let case = document
            .cases
            .iter()
            .find(|case| case.name == "primary-discard")
            .unwrap();
        assert_eq!(case.flat_input.refs.len(), 2);
        assert_ne!(
            case.flat_input.refs[0].model_object_index,
            case.action_ref_node_indices[0] as u32
        );
        let cards = serde_json::from_str::<Value>(&case.canonical_json).unwrap()["semantic"]
            ["cards"]
            .as_array()
            .unwrap()
            .clone();
        let keys = cards.iter().map(canonical_value_bytes).collect::<Vec<_>>();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        assert!(case
            .action_ref_node_indices
            .windows(2)
            .all(|window| window[0] <= window[1]));
    }

    #[test]
    fn enum_layout_compile_time_probes_remain_exhaustive() {
        let _ = FlatManaColorV1::White;
        assert_eq!(NATIVE_FLAT_STATE_FEATURE_DIM_V1, 219);
        assert_eq!(NATIVE_FLAT_OBJECT_FEATURE_DIM_V1, 98);
        assert_eq!(NATIVE_FLAT_EDGE_FEATURE_DIM_V1, 41);
        assert_eq!(NATIVE_FLAT_OBJECT_GROUP_COUNT_V1, 20);
    }

    #[derive(Clone)]
    struct OwnedScoringDecisionV2 {
        globals: FlatGlobalsV2,
        objects: Vec<FlatObjectCoreV1>,
        relations: Vec<FlatRelationV2>,
        object_subtypes: Vec<FlatObjectSubtypeV2>,
        ability_uses: Vec<FlatObjectAbilityUseV2>,
        goads: Vec<FlatObjectGoadV2>,
        completed_dungeons: Vec<FlatCompletedDungeonV2>,
        effect_subtype_changes: Vec<FlatEffectSubtypeChangeV2>,
        context_path_elements: Vec<FlatContextPathElementV2>,
        actions: Vec<FlatScorerActionCoreV1>,
        action_refs: Vec<FlatScorerActionRefV1>,
    }

    impl OwnedScoringDecisionV2 {
        fn from_session(session: &FastActorSessionV1) -> Self {
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("expected a live decision");
            };
            let mut encoder = FlatDecisionEncoderV2::default();
            let mut owned = Self {
                globals: FlatGlobalsV2::default(),
                objects: Vec::new(),
                relations: Vec::new(),
                object_subtypes: Vec::new(),
                ability_uses: Vec::new(),
                goads: Vec::new(),
                completed_dungeons: Vec::new(),
                effect_subtype_changes: Vec::new(),
                context_path_elements: Vec::new(),
                actions: Vec::new(),
                action_refs: Vec::new(),
            };
            let encoded = session
                .encode_current_flat_scoring_decision_owned_v2(
                    expected,
                    &mut encoder,
                    &mut FlatScoringOwnedBuffersV2 {
                        objects: &mut owned.objects,
                        relations: &mut owned.relations,
                        object_subtypes: &mut owned.object_subtypes,
                        ability_uses: &mut owned.ability_uses,
                        goads: &mut owned.goads,
                        completed_dungeons: &mut owned.completed_dungeons,
                        effect_subtype_changes: &mut owned.effect_subtype_changes,
                        context_path_elements: &mut owned.context_path_elements,
                        actions: &mut owned.actions,
                        action_refs: &mut owned.action_refs,
                    },
                )
                .unwrap();
            owned.globals = encoded.globals;
            owned
        }

        fn view(&self) -> FlatScoringDecisionViewV1<'_> {
            FlatScoringDecisionViewV1::new(
                &self.globals,
                &self.objects,
                &self.relations,
                &self.object_subtypes,
                &self.ability_uses,
                &self.goads,
                &self.completed_dungeons,
                &self.effect_subtype_changes,
                &self.context_path_elements,
                &self.actions,
                &self.action_refs,
            )
        }

        fn replace_observation_tables(&mut self, observation: &ObservationV5) {
            let referenced_rows = self
                .action_refs
                .iter()
                .map(|reference| {
                    let index = usize::try_from(reference.model_object_index).unwrap();
                    (index, self.objects[index])
                })
                .collect::<Vec<_>>();
            let tables = encode_observation_owned_tables_for_fixture_v2(observation).unwrap();
            self.globals = tables.globals;
            self.objects = tables.objects;
            self.relations = tables.relations;
            self.object_subtypes = tables.object_subtypes;
            self.ability_uses = tables.ability_uses;
            self.goads = tables.goads;
            self.completed_dungeons = tables.completed_dungeons;
            self.effect_subtype_changes = tables.effect_subtype_changes;
            self.context_path_elements = tables.context_path_elements;
            for (index, expected) in referenced_rows {
                assert_eq!(
                    self.objects.get(index),
                    Some(&expected),
                    "synthetic observation transform changed a scorer action-ref row"
                );
            }
        }
    }

    #[test]
    fn production_v2_opening_decision_tensorizes_end_to_end() {
        let session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            71_001,
            0x71_001,
            256,
            32_768,
            ["Burn".to_string(), "Burn".to_string()],
        )
        .unwrap();
        let owned = OwnedScoringDecisionV2::from_session(&session);
        let mut output = NativeFlatDecisionTensorV2::default();
        fill_native_flat_decision_tensors_v2(owned.view(), &mut output).unwrap();
        validate_full_output_v2(
            &output,
            output.object_card_ids.len(),
            owned.actions.len(),
            owned.action_refs.len(),
        )
        .unwrap();
    }

    fn full_tensor_sentinel_v2() -> NativeFlatDecisionTensorV2 {
        NativeFlatDecisionTensorV2 {
            state: vec![1.25, -2.5],
            object_features: vec![3.75],
            object_card_ids: vec![-4],
            object_groups: vec![-5],
            object_node_ids: vec![-6],
            edge_features: vec![7.25],
            edge_source_indices: vec![-8],
            edge_target_indices: vec![-9],
            action_features: vec![10.5],
            action_ref_features: vec![-11.75],
            action_ref_card_ids: vec![-12],
            action_ref_action_indices: vec![-13],
            action_ref_node_indices: vec![-14],
        }
    }

    #[test]
    fn full_tensorizer_is_transactional_and_stateful_errors_poison_reuse() {
        let session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            71_002,
            0x71_002,
            256,
            32_768,
            ["Burn".to_string(), "Burn".to_string()],
        )
        .unwrap();
        let valid = OwnedScoringDecisionV2::from_session(&session);
        let mut expected = NativeFlatDecisionTensorV2::default();
        fill_native_flat_decision_tensors_v2(valid.view(), &mut expected).unwrap();

        let mut malformed = valid.clone();
        malformed.objects[0].card_token = NATIVE_FLAT_MAX_CARD_TOKEN_V2 + 1;
        let mut output = full_tensor_sentinel_v2();
        let sentinel = output.clone();
        assert!(fill_native_flat_decision_tensors_v2(malformed.view(), &mut output).is_err());
        assert_eq!(
            output, sentinel,
            "stateless failure published partial tensors"
        );

        let mut stateful = NativeFlatTensorizerV2::new();
        assert!(stateful.fill(malformed.view(), &mut output).is_err());
        assert!(stateful.is_poisoned());
        assert_eq!(output, sentinel, "poisoning failure changed caller output");
        assert_eq!(
            stateful.fill(valid.view(), &mut output),
            Err(NativeFlatTensorErrorV2::Poisoned)
        );
        assert_eq!(output, sentinel, "poisoned reuse changed caller output");

        let mut successful = NativeFlatTensorizerV2::new();
        successful.fill(valid.view(), &mut output).unwrap();
        assert!(!successful.is_poisoned());
        assert_eq!(
            output, expected,
            "successful reuse left a stale tensor field"
        );
    }

    #[test]
    fn malformed_child_relation_and_blocked_order_tables_fail_closed() {
        let document = full_golden();
        let opening = document
            .cases
            .iter()
            .find(|case| case.name == "burn-mirror-opening")
            .unwrap();
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            opening.rust_fixture.episode_id,
            opening.rust_fixture.environment_seed,
            256,
            32_768,
            opening.rust_fixture.deck_ids.clone(),
        )
        .unwrap();
        let valid = OwnedScoringDecisionV2::from_session(&session);

        let mut malformed_child = valid.clone();
        malformed_child.objects[0].subtype_start = 1;
        malformed_child.objects[0].subtype_count = 1;
        assert!(fill_native_flat_decision_tensors_v2(
            malformed_child.view(),
            &mut NativeFlatDecisionTensorV2::default()
        )
        .is_err());

        let mut malformed_relation = valid.clone();
        malformed_relation.relations.push(FlatRelationV2 {
            role: FlatRelationRoleV2::KnownLibrary,
            source_object: Some(0),
            target_object: Some(0),
            payload: FlatRelationPayloadV2::None,
            ..FlatRelationV2::default()
        });
        assert!(fill_native_flat_decision_tensors_v2(
            malformed_relation.view(),
            &mut NativeFlatDecisionTensorV2::default()
        )
        .is_err());

        let combat = document
            .cases
            .iter()
            .find(|case| case.name == "burn-mirror-combat")
            .unwrap();
        for &selected_index in &combat.rust_fixture.replay_selected_indices {
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("combat replay terminated early");
            };
            session
                .step(expected.episode_id, expected.step, selected_index)
                .unwrap();
        }
        let mut malformed_blocked_order = OwnedScoringDecisionV2::from_session(&session);
        let attacker = malformed_blocked_order
            .relations
            .iter_mut()
            .find(|relation| relation.role == FlatRelationRoleV2::CombatAttacker)
            .expect("combat fixture must have an attacker relation");
        attacker.payload = FlatRelationPayloadV2::CombatAttacker {
            blocked_order: Some(1),
        };
        assert!(fill_native_flat_decision_tensors_v2(
            malformed_blocked_order.view(),
            &mut NativeFlatDecisionTensorV2::default()
        )
        .is_err());
    }

    #[test]
    fn production_v2_replays_tensorize_every_decision_across_deck_orders() {
        let mut seen_relation_roles = std::collections::BTreeSet::new();
        let mut seen_object_groups = std::collections::BTreeSet::new();
        let mut seen_engine_stages = std::collections::BTreeSet::new();
        let mut seen_policy_stages = std::collections::BTreeSet::new();
        for (scenario, episode_id, environment_seed, deck_ids) in [
            (
                "burn-mirror",
                71_010,
                0x71_010,
                ["Burn".to_string(), "Burn".to_string()],
            ),
            (
                "rally-mirror",
                71_011,
                0x71_011,
                ["Rally".to_string(), "Rally".to_string()],
            ),
            (
                "burn-rally",
                71_012,
                0x71_012,
                ["Burn".to_string(), "Rally".to_string()],
            ),
        ] {
            let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
                episode_id,
                environment_seed,
                512,
                65_536,
                deck_ids,
            )
            .unwrap();
            let mut decisions = 0_usize;
            let mut output = NativeFlatDecisionTensorV2::default();
            for step in 0..128 {
                let FastActorResponseV1::Decision(expected) = session.current_response() else {
                    break;
                };
                let owned = OwnedScoringDecisionV2::from_session(&session);
                seen_relation_roles.extend(owned.relations.iter().map(|row| row.role as u8));
                seen_object_groups.extend(owned.objects.iter().map(|row| row.group as u8));
                seen_engine_stages.insert(owned.globals.engine.current_stage);
                seen_policy_stages.insert(owned.globals.policy_surface.current_stage);
                fill_native_flat_decision_tensors_v2(owned.view(), &mut output)
                    .unwrap_or_else(|error| panic!("{scenario} step {step}: {error:?}"));
                validate_full_output_v2(
                    &output,
                    output.object_card_ids.len(),
                    owned.actions.len(),
                    owned.action_refs.len(),
                )
                .unwrap();
                let semantics = session.diagnostic_current_action_semantics().unwrap();
                let selected = replay_selection_v2(step, &semantics);
                session
                    .step(expected.episode_id, expected.step, selected)
                    .unwrap();
                decisions += 1;
            }
            assert!(decisions >= 32, "{scenario} replay was unexpectedly short");
        }
        for role in [
            FlatRelationRoleV2::StackTarget,
            FlatRelationRoleV2::CombatAttacker,
            FlatRelationRoleV2::CombatBlocker,
            FlatRelationRoleV2::EffectAffected,
            FlatRelationRoleV2::EffectSource,
            FlatRelationRoleV2::Permission,
            FlatRelationRoleV2::PendingContext,
            FlatRelationRoleV2::PrivateContext,
            FlatRelationRoleV2::PaidCost,
        ] {
            assert!(
                seen_relation_roles.contains(&(role as u8)),
                "production replays did not exercise relation role {role:?}"
            );
        }
        for group in [
            FlatObjectGroupV2::SelfHand,
            FlatObjectGroupV2::SelfBattlefield,
            FlatObjectGroupV2::OpponentBattlefield,
            FlatObjectGroupV2::SelfGraveyard,
            FlatObjectGroupV2::OpponentGraveyard,
            FlatObjectGroupV2::Exile,
            FlatObjectGroupV2::Stack,
            FlatObjectGroupV2::Combat,
            FlatObjectGroupV2::ContinuousEffect,
            FlatObjectGroupV2::Permission,
            FlatObjectGroupV2::CombatBlock,
            FlatObjectGroupV2::PendingContext,
            FlatObjectGroupV2::PrivateContext,
        ] {
            assert!(
                seen_object_groups.contains(&(group as u8)),
                "production replays did not exercise object group {group:?}"
            );
        }
        assert_eq!(seen_engine_stages, [0, 1, 3, 6].into_iter().collect());
        assert_eq!(seen_policy_stages, [0, 1, 2].into_iter().collect());
    }

    fn assert_full_float_tensor(
        case: &str,
        name: &str,
        actual: &[f32],
        expected: &FullTensorPayload,
        shape: &[usize],
    ) {
        assert_eq!(expected.shape, shape, "{case} {name} shape authority");
        assert_eq!(
            actual.len(),
            shape.iter().product::<usize>(),
            "{case} {name} length"
        );
        let expected_bits = u32_le_words_from_hex(
            expected
                .f32_le_hex
                .as_deref()
                .unwrap_or_else(|| panic!("{case} {name} lacks f32 payload")),
        );
        assert_eq!(
            actual
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            expected_bits,
            "{case} {name} bits"
        );
        assert!(expected.i64_values.is_none(), "{case} {name} mixed payload");
    }

    fn assert_full_integer_tensor(
        case: &str,
        name: &str,
        actual: &[i64],
        expected: &FullTensorPayload,
        shape: &[usize],
    ) {
        assert_eq!(expected.shape, shape, "{case} {name} shape authority");
        assert_eq!(
            actual.len(),
            shape.iter().product::<usize>(),
            "{case} {name} length"
        );
        assert_eq!(
            actual,
            expected
                .i64_values
                .as_deref()
                .unwrap_or_else(|| panic!("{case} {name} lacks i64 payload")),
            "{case} {name} values"
        );
        assert!(expected.f32_le_hex.is_none(), "{case} {name} mixed payload");
    }

    #[test]
    fn production_v2_full_decision_matches_python_all_thirteen_tensors() {
        let document = full_golden();
        assert_eq!(document.dimensions.state, NATIVE_FLAT_STATE_FEATURE_DIM_V2);
        assert_eq!(
            document.dimensions.object,
            NATIVE_FLAT_OBJECT_FEATURE_DIM_V2
        );
        assert_eq!(document.dimensions.edge, NATIVE_FLAT_EDGE_FEATURE_DIM_V2);
        assert_eq!(
            document.dimensions.action,
            NATIVE_FLAT_ACTION_FEATURE_DIM_V2
        );
        assert_eq!(
            document.dimensions.action_ref,
            NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2
        );
        assert_eq!(
            document.dimensions.object_groups,
            NATIVE_FLAT_OBJECT_GROUP_COUNT_V2
        );
        assert_eq!(hex(&Sha256::digest(FEATURES_PY)), document.authority_sha256);
        let mut payload: Value = serde_json::from_str(FULL_GOLDEN).unwrap();
        payload.as_object_mut().unwrap().remove("payload_sha256");
        assert_eq!(
            hex(&Sha256::digest(serde_json::to_vec(&payload).unwrap())),
            document.payload_sha256
        );

        for case in document.cases {
            let fixture = &case.rust_fixture;
            let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
                fixture.episode_id,
                fixture.environment_seed,
                256,
                32_768,
                fixture.deck_ids.clone(),
            )
            .unwrap();
            for &selected_index in &fixture.replay_selected_indices {
                let FastActorResponseV1::Decision(expected) = session.current_response() else {
                    panic!("{} replay terminated early", case.name);
                };
                session
                    .step(expected.episode_id, expected.step, selected_index)
                    .unwrap();
            }
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("{} fixture decision is terminal", case.name);
            };
            if fixture.fixture_transform == "actor-seat-swap" {
                assert_ne!(
                    fixture.observation["acting_player"],
                    serde_json::to_value(expected.acting_player).unwrap(),
                    "{} actor-swap fixture",
                    case.name
                );
            } else {
                assert_eq!(
                    fixture.observation["acting_player"],
                    serde_json::to_value(expected.acting_player).unwrap(),
                    "{} actor fixture",
                    case.name
                );
            }
            let transformed_observation: ObservationV5 =
                serde_json::from_value(fixture.observation.clone()).unwrap();
            let mut owned = OwnedScoringDecisionV2::from_session(&session);
            apply_owned_fixture_transform_v2(
                &mut owned,
                &fixture.fixture_transform,
                &transformed_observation,
            );
            assert_eq!(
                fixture.legal_actions.as_array().unwrap().len(),
                owned.actions.len(),
                "{} action-count fixture",
                case.name
            );
            let projection = build_object_projection_v2(&owned.objects).unwrap();
            let canonical = serde_json::to_string(
                &canonical_observation_v2(owned.view(), &projection).unwrap(),
            )
            .unwrap();
            assert_eq!(
                canonical, case.canonical_observation_json,
                "{} canonical",
                case.name
            );
            assert_eq!(case.state_sha512_blocks_hex.len(), 6);
            let actual_blocks = (0_u32..6)
                .map(|counter| {
                    let mut digest = Sha512::new();
                    digest.update(b"observation-state");
                    digest.update(counter.to_le_bytes());
                    digest.update(canonical.as_bytes());
                    hex(&digest.finalize())
                })
                .collect::<Vec<_>>();
            assert_eq!(
                actual_blocks, case.state_sha512_blocks_hex,
                "{} state digest",
                case.name
            );

            let mut output = NativeFlatDecisionTensorV2::default();
            fill_native_flat_decision_tensors_v2(owned.view(), &mut output).unwrap();
            let object_count = output.object_card_ids.len();
            let edge_count = output.edge_source_indices.len();
            let action_count = owned.actions.len();
            let action_ref_count = owned.action_refs.len();
            let tensors = &case.tensors;
            assert_full_float_tensor(
                &case.name,
                "state",
                &output.state,
                &tensors.state,
                &[NATIVE_FLAT_STATE_FEATURE_DIM_V2],
            );
            assert_full_float_tensor(
                &case.name,
                "object_features",
                &output.object_features,
                &tensors.object_features,
                &[object_count, NATIVE_FLAT_OBJECT_FEATURE_DIM_V2],
            );
            assert_full_integer_tensor(
                &case.name,
                "object_card_ids",
                &output.object_card_ids,
                &tensors.object_card_ids,
                &[object_count],
            );
            assert_full_integer_tensor(
                &case.name,
                "object_groups",
                &output.object_groups,
                &tensors.object_groups,
                &[object_count],
            );
            assert_full_integer_tensor(
                &case.name,
                "object_node_ids",
                &output.object_node_ids,
                &tensors.object_node_ids,
                &[object_count],
            );
            assert_full_float_tensor(
                &case.name,
                "edge_features",
                &output.edge_features,
                &tensors.edge_features,
                &[edge_count, NATIVE_FLAT_EDGE_FEATURE_DIM_V2],
            );
            assert_full_integer_tensor(
                &case.name,
                "edge_source_indices",
                &output.edge_source_indices,
                &tensors.edge_source_indices,
                &[edge_count],
            );
            assert_full_integer_tensor(
                &case.name,
                "edge_target_indices",
                &output.edge_target_indices,
                &tensors.edge_target_indices,
                &[edge_count],
            );
            assert_full_float_tensor(
                &case.name,
                "action_features",
                &output.action_features,
                &tensors.action_features,
                &[action_count, NATIVE_FLAT_ACTION_FEATURE_DIM_V2],
            );
            assert_full_float_tensor(
                &case.name,
                "action_ref_features",
                &output.action_ref_features,
                &tensors.action_ref_features,
                &[action_ref_count, NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2],
            );
            assert_full_integer_tensor(
                &case.name,
                "action_ref_card_ids",
                &output.action_ref_card_ids,
                &tensors.action_ref_card_ids,
                &[action_ref_count],
            );
            assert_full_integer_tensor(
                &case.name,
                "action_ref_action_indices",
                &output.action_ref_action_indices,
                &tensors.action_ref_action_indices,
                &[action_ref_count],
            );
            assert_full_integer_tensor(
                &case.name,
                "action_ref_node_indices",
                &output.action_ref_node_indices,
                &tensors.action_ref_node_indices,
                &[action_ref_count],
            );
        }
    }

    #[derive(Serialize)]
    struct EmittedFullDecisionFixtureV2 {
        schema: &'static str,
        name: String,
        episode_id: u64,
        environment_seed: u64,
        deck_ids: [String; 2],
        replay_selected_indices: Vec<u32>,
        fixture_transform: &'static str,
        observation: crate::rl::ObservationV5,
        legal_actions: Vec<crate::rl::LegalActionV5>,
        canonical_observation_json: String,
    }

    fn emitted_full_fixture_v2(
        session: &FastActorSessionV1,
        name: String,
        episode_id: u64,
        environment_seed: u64,
        deck_ids: [String; 2],
        replay_selected_indices: Vec<u32>,
        fixture_transform: &'static str,
    ) -> EmittedFullDecisionFixtureV2 {
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("expected a live decision");
        };
        let mut observation = session.flat_policy_observation_v2(expected).unwrap();
        let mut legal_actions = session
            .diagnostic_current_action_semantics()
            .unwrap()
            .into_iter()
            .enumerate()
            .map(|(index, semantic)| {
                make_legal_action_v5(u32::try_from(index).unwrap(), semantic, None).unwrap()
            })
            .collect::<Vec<_>>();
        let mut owned = OwnedScoringDecisionV2::from_session(session);
        match fixture_transform {
            "identity" => {}
            "zero-objects" => {
                observation.own_hand.clear();
                observation.projection.surface.hand_counts = [0, 0];
                legal_actions = vec![make_legal_action_v5(
                    0,
                    ActionSemanticV1::Pass {
                        actor: expected.acting_player,
                    },
                    None,
                )
                .unwrap()];
                apply_owned_fixture_transform_v2(&mut owned, fixture_transform, &observation);
            }
            "card-token-65536" => {
                observation.own_hand[0].stable.card_db_id = u16::MAX;
                apply_owned_fixture_transform_v2(&mut owned, fixture_transform, &observation);
            }
            "actor-seat-swap" => {
                let mut observation_value = serde_json::to_value(&observation).unwrap();
                swap_seat_strings_v2(&mut observation_value);
                observation = serde_json::from_value(observation_value).unwrap();
                let mut action_value = serde_json::to_value(&legal_actions).unwrap();
                swap_seat_strings_v2(&mut action_value);
                legal_actions = serde_json::from_value(action_value).unwrap();
            }
            "combat-blocked-order-absent" => {
                observation
                    .projection
                    .surface
                    .combat
                    .attacker_to_ordered_blockers
                    .clear();
                apply_owned_fixture_transform_v2(&mut owned, fixture_transform, &observation);
            }
            "combat-blocked-order-present-empty" => {
                let attacker = observation.projection.surface.combat.ordered_attackers[0].clone();
                observation
                    .projection
                    .surface
                    .combat
                    .attacker_to_ordered_blockers = vec![(attacker, Vec::new())];
                apply_owned_fixture_transform_v2(&mut owned, fixture_transform, &observation);
            }
            "synthetic-known-cards-object-relations-v1" => {
                // Declared synthetic authority: the current Burn/Rally replay
                // pool does not naturally expose these model-visible paths.
                let actor = observation.acting_player;
                let opponent = match actor {
                    PlayerSeatV1::P0 => PlayerSeatV1::P1,
                    PlayerSeatV1::P1 => PlayerSeatV1::P0,
                };
                let opponent_index = match opponent {
                    PlayerSeatV1::P0 => 0,
                    PlayerSeatV1::P1 => 1,
                };
                let mut known_hand = observation.own_hand[0].clone();
                known_hand.card_name = "synthetic-known-hand".to_string();
                known_hand.stable.arena_id = 0xff00_0001;
                known_hand.stable.card_db_id = 61_001;
                known_hand.stable.owner = opponent;
                known_hand.stable.controller = opponent;
                known_hand.stable.zone = Zone::Hand;
                known_hand.stable.zone_change_count = 17;
                observation.known_hand_cards[opponent_index] = vec![known_hand];

                let mut known_library = observation.own_hand[1].clone();
                known_library.card_name = "synthetic-known-library".to_string();
                known_library.stable.arena_id = 0xff00_0002;
                known_library.stable.card_db_id = 61_002;
                known_library.stable.owner = opponent;
                known_library.stable.controller = opponent;
                known_library.stable.zone = Zone::Library;
                known_library.stable.zone_change_count = 23;
                observation.known_library_cards[opponent_index] = vec![KnownLibraryCardV4 {
                    position: 0,
                    card: known_library,
                }];

                let first = observation.own_hand[0].stable.clone();
                let second = observation.own_hand[1].stable.clone();
                observation.projection.surface.object_relations = vec![
                    ObjectRelationPublicV4::AttachedTo {
                        object: first.clone(),
                        attached_to: second.clone(),
                    },
                    ObjectRelationPublicV4::ExiledBy {
                        object: second,
                        exiled_by: first,
                    },
                ];
                apply_owned_fixture_transform_v2(&mut owned, fixture_transform, &observation);
            }
            _ => panic!("unknown full fixture transform"),
        }
        let projection = build_object_projection_v2(&owned.objects).unwrap();
        let canonical_observation_json =
            serde_json::to_string(&canonical_observation_v2(owned.view(), &projection).unwrap())
                .unwrap();
        EmittedFullDecisionFixtureV2 {
            schema: "native-flat-full-v2-rust-fixture-v1",
            name,
            episode_id,
            environment_seed,
            deck_ids,
            replay_selected_indices,
            fixture_transform,
            observation,
            legal_actions,
            canonical_observation_json,
        }
    }

    fn apply_owned_fixture_transform_v2(
        owned: &mut OwnedScoringDecisionV2,
        fixture_transform: &str,
        transformed_observation: &ObservationV5,
    ) {
        match fixture_transform {
            "identity" | "actor-seat-swap" => {}
            "zero-objects" => {
                owned.globals.players[0].hand_count = 0;
                owned.globals.players[1].hand_count = 0;
                owned.objects.clear();
                owned.relations.clear();
                owned.object_subtypes.clear();
                owned.ability_uses.clear();
                owned.goads.clear();
                owned.completed_dungeons.clear();
                owned.effect_subtype_changes.clear();
                owned.context_path_elements.clear();
                owned.actions = vec![FlatScorerActionCoreV1::default()];
                owned.action_refs.clear();
            }
            "card-token-65536" => {
                owned.objects[0].card_token = NATIVE_FLAT_MAX_CARD_TOKEN_V2;
            }
            "synthetic-known-cards-object-relations-v1" => {
                owned.replace_observation_tables(transformed_observation);
            }
            "combat-blocked-order-absent" => {
                let mut count = 0;
                for relation in &mut owned.relations {
                    if relation.role == FlatRelationRoleV2::CombatAttacker {
                        relation.payload = FlatRelationPayloadV2::CombatAttacker {
                            blocked_order: None,
                        };
                        count += 1;
                    }
                }
                assert_eq!(count, 1, "red-pair fixture requires one attacker");
                assert!(owned
                    .relations
                    .iter()
                    .all(|relation| relation.role != FlatRelationRoleV2::CombatBlocker));
            }
            "combat-blocked-order-present-empty" => {
                let mut count = 0;
                for relation in &mut owned.relations {
                    if relation.role == FlatRelationRoleV2::CombatAttacker {
                        relation.payload = FlatRelationPayloadV2::CombatAttacker {
                            blocked_order: Some(0),
                        };
                        count += 1;
                    }
                }
                assert_eq!(count, 1, "red-pair fixture requires one attacker");
                assert!(owned
                    .relations
                    .iter()
                    .all(|relation| relation.role != FlatRelationRoleV2::CombatBlocker));
            }
            _ => panic!("unknown full fixture transform"),
        }
    }

    fn swap_seat_strings_v2(value: &mut Value) {
        match value {
            Value::String(text) if text == "p0" => *text = "p1".to_string(),
            Value::String(text) if text == "p1" => *text = "p0".to_string(),
            Value::Array(items) => {
                for item in items {
                    swap_seat_strings_v2(item);
                }
            }
            Value::Object(fields) => {
                for child in fields.values_mut() {
                    swap_seat_strings_v2(child);
                }
            }
            _ => {}
        }
    }

    fn replay_selection_v2(step: usize, semantics: &[ActionSemanticV1]) -> u32 {
        let pass = semantics
            .iter()
            .position(|semantic| matches!(semantic, ActionSemanticV1::Pass { .. }));
        let preferred = if step % 4 == 3 {
            pass
        } else {
            [
                |semantic: &ActionSemanticV1| matches!(semantic, ActionSemanticV1::PlayLand { .. }),
                |semantic: &ActionSemanticV1| {
                    matches!(semantic, ActionSemanticV1::ActivateManaAbility { .. })
                },
                |semantic: &ActionSemanticV1| {
                    matches!(semantic, ActionSemanticV1::CastSpell { .. })
                },
                |semantic: &ActionSemanticV1| {
                    matches!(
                        semantic,
                        ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                            | ActionSemanticV1::ChooseBlockerInclusion { include: true, .. }
                    )
                },
            ]
            .into_iter()
            .find_map(|predicate| semantics.iter().position(predicate))
        };
        u32::try_from(
            preferred
                .or(pass)
                .unwrap_or_else(|| semantics.len().saturating_sub(1)),
        )
        .unwrap()
    }

    #[test]
    #[ignore = "fixture generator invoked explicitly by the Python golden tool"]
    fn emit_native_full_v2_fixtures() {
        let episode_id = 71_001;
        let environment_seed = 0x71_001;
        let deck_ids = ["Burn".to_string(), "Burn".to_string()];
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            environment_seed,
            256,
            32_768,
            deck_ids.clone(),
        )
        .unwrap();
        let checkpoints = [0_usize, 1, 8, 32, 48];
        let mut replay = Vec::new();
        let mut emitted_stack = false;
        let mut emitted_combat = false;
        let mut emitted_effect = false;
        let mut emitted_private_combat = false;
        let mut emitted_pending_choice = false;
        for step in 0..=120 {
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                break;
            };
            if checkpoints.contains(&step) {
                let fixture = emitted_full_fixture_v2(
                    &session,
                    if step == 0 {
                        "burn-mirror-opening".to_string()
                    } else {
                        format!("burn-mirror-replay-{step:02}")
                    },
                    episode_id,
                    environment_seed,
                    deck_ids.clone(),
                    replay.clone(),
                    "identity",
                );
                println!(
                    "NATIVE_FLAT_FULL_V2_FIXTURE={}",
                    serde_json::to_string(&fixture).unwrap()
                );
                if step == 0 {
                    for (name, transform) in [
                        ("synthetic-zero-objects", "zero-objects"),
                        ("synthetic-card-token-65536", "card-token-65536"),
                        ("synthetic-actor-seat-swap", "actor-seat-swap"),
                        (
                            "synthetic-known-cards-object-relations-v1",
                            "synthetic-known-cards-object-relations-v1",
                        ),
                    ] {
                        let fixture = emitted_full_fixture_v2(
                            &session,
                            name.to_string(),
                            episode_id,
                            environment_seed,
                            deck_ids.clone(),
                            replay.clone(),
                            transform,
                        );
                        println!(
                            "NATIVE_FLAT_FULL_V2_FIXTURE={}",
                            serde_json::to_string(&fixture).unwrap()
                        );
                    }
                }
            }
            let observation = session.flat_policy_observation_v2(expected).unwrap();
            let public = &observation.projection.surface;
            let dynamic_name = if !emitted_stack && !public.stack.is_empty() {
                emitted_stack = true;
                Some("burn-mirror-stack")
            } else if !emitted_combat && !public.combat.ordered_attackers.is_empty() {
                emitted_combat = true;
                Some("burn-mirror-combat")
            } else if !emitted_effect && !public.continuous_effects.is_empty() {
                emitted_effect = true;
                Some("burn-mirror-effect")
            } else if !emitted_private_combat
                && observation
                    .projection
                    .policy_surface_context
                    .private_combat_selection
                    .is_some()
            {
                emitted_private_combat = true;
                Some("burn-mirror-private-combat")
            } else if !emitted_pending_choice
                && (public.engine_context.pending_cast.is_some()
                    || public.engine_context.pending_activation.is_some()
                    || public.engine_context.pending_optional_cost.is_some()
                    || public
                        .engine_context
                        .pending_optional_cost_sacrifice
                        .is_some()
                    || public.engine_context.pending_spell_copy.is_some()
                    || public.engine_context.pending_effect.is_some()
                    || !public.engine_context.pending_triggers.is_empty())
            {
                emitted_pending_choice = true;
                Some("burn-mirror-pending-choice")
            } else {
                None
            };
            if let Some(name) = dynamic_name {
                let fixture = emitted_full_fixture_v2(
                    &session,
                    name.to_string(),
                    episode_id,
                    environment_seed,
                    deck_ids.clone(),
                    replay.clone(),
                    "identity",
                );
                println!(
                    "NATIVE_FLAT_FULL_V2_FIXTURE={}",
                    serde_json::to_string(&fixture).unwrap()
                );
                if name == "burn-mirror-combat" {
                    let present_empty = emitted_full_fixture_v2(
                        &session,
                        "burn-mirror-combat-present-empty".to_string(),
                        episode_id,
                        environment_seed,
                        deck_ids.clone(),
                        replay.clone(),
                        "combat-blocked-order-present-empty",
                    );
                    println!(
                        "NATIVE_FLAT_FULL_V2_FIXTURE={}",
                        serde_json::to_string(&present_empty).unwrap()
                    );
                }
            }
            let semantics = session.diagnostic_current_action_semantics().unwrap();
            let selected = replay_selection_v2(step, &semantics);
            replay.push(selected);
            session
                .step(expected.episode_id, expected.step, selected)
                .unwrap();
        }

        let rally_episode_id = 71_011;
        let rally_environment_seed = 0x71_011;
        let rally_deck_ids = ["Rally".to_string(), "Rally".to_string()];
        let mut rally_session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            rally_episode_id,
            rally_environment_seed,
            256,
            32_768,
            rally_deck_ids.clone(),
        )
        .unwrap();
        let mut rally_replay = Vec::new();
        let mut emitted_rally_effect = false;
        let mut emitted_rally_permission = false;
        let mut emitted_rally_object_relation = false;
        let mut emitted_rally_paid_cost = false;
        let mut emitted_rally_blocker = false;
        let mut emitted_rally_known_card = false;
        for step in 0..128 {
            let FastActorResponseV1::Decision(expected) = rally_session.current_response() else {
                break;
            };
            let observation = rally_session.flat_policy_observation_v2(expected).unwrap();
            let public = &observation.projection.surface;
            let mut names = Vec::new();
            if !emitted_rally_effect && !public.continuous_effects.is_empty() {
                emitted_rally_effect = true;
                names.push("rally-mirror-effect");
            }
            if !emitted_rally_permission && !public.exile_play_permissions.is_empty() {
                emitted_rally_permission = true;
                names.push("rally-mirror-permission");
            }
            if !emitted_rally_object_relation && !public.object_relations.is_empty() {
                emitted_rally_object_relation = true;
                names.push("rally-mirror-object-relation");
            }
            if !emitted_rally_paid_cost
                && public
                    .stack
                    .iter()
                    .any(|item| !item.paid_cost_refs.is_empty())
            {
                emitted_rally_paid_cost = true;
                names.push("rally-mirror-paid-cost");
            }
            if !emitted_rally_blocker
                && public
                    .combat
                    .attacker_to_ordered_blockers
                    .iter()
                    .any(|(_, blockers)| !blockers.is_empty())
            {
                emitted_rally_blocker = true;
                names.push("rally-mirror-blocker");
            }
            if !emitted_rally_known_card
                && (observation
                    .known_library_cards
                    .iter()
                    .any(|cards| !cards.is_empty())
                    || observation
                        .known_hand_cards
                        .iter()
                        .any(|cards| !cards.is_empty()))
            {
                emitted_rally_known_card = true;
                names.push("rally-mirror-known-card");
            }
            for name in names {
                let fixture = emitted_full_fixture_v2(
                    &rally_session,
                    name.to_string(),
                    rally_episode_id,
                    rally_environment_seed,
                    rally_deck_ids.clone(),
                    rally_replay.clone(),
                    "identity",
                );
                println!(
                    "NATIVE_FLAT_FULL_V2_FIXTURE={}",
                    serde_json::to_string(&fixture).unwrap()
                );
            }
            let semantics = rally_session.diagnostic_current_action_semantics().unwrap();
            let selected = replay_selection_v2(step, &semantics);
            rally_replay.push(selected);
            rally_session
                .step(expected.episode_id, expected.step, selected)
                .unwrap();
        }

        let mixed_episode_id = 71_012;
        let mixed_environment_seed = 0x71_012;
        let mixed_deck_ids = ["Burn".to_string(), "Rally".to_string()];
        let mut mixed_session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            mixed_episode_id,
            mixed_environment_seed,
            256,
            32_768,
            mixed_deck_ids.clone(),
        )
        .unwrap();
        let mut mixed_replay = Vec::new();
        for step in 0..128 {
            let FastActorResponseV1::Decision(expected) = mixed_session.current_response() else {
                break;
            };
            let observation = mixed_session.flat_policy_observation_v2(expected).unwrap();
            if observation
                .projection
                .surface
                .stack
                .iter()
                .any(|item| !item.paid_cost_refs.is_empty())
            {
                let fixture = emitted_full_fixture_v2(
                    &mixed_session,
                    "burn-rally-paid-cost".to_string(),
                    mixed_episode_id,
                    mixed_environment_seed,
                    mixed_deck_ids.clone(),
                    mixed_replay.clone(),
                    "identity",
                );
                println!(
                    "NATIVE_FLAT_FULL_V2_FIXTURE={}",
                    serde_json::to_string(&fixture).unwrap()
                );
                break;
            }
            let semantics = mixed_session.diagnostic_current_action_semantics().unwrap();
            let selected = replay_selection_v2(step, &semantics);
            mixed_replay.push(selected);
            mixed_session
                .step(expected.episode_id, expected.step, selected)
                .unwrap();
        }
    }
}
