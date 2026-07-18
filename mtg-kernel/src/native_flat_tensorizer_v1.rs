//! Python-compatible native tensors reconstructed from the flat scorer view.
//!
//! This interim V1 module implements only the legal-action half. The scorer
//! view's observation side is not yet injective over the Python observation
//! contract, so empty observation tables in [`NativeFlatDecisionTensorV1`] are
//! not a claim of a complete encoded decision. A later view version may fill
//! those tables without changing the action reconstruction isolated here.

use crate::flat_policy_v1::{
    FlatObjectCoreV1, FlatRelativePlayerV1, FlatScorerActionCoreV1, FlatScorerActionKindV1,
    FlatScorerActionRefV1, FlatScoringDecisionViewV1, FlatZoneV1,
};
use crate::rl_session::{
    FLAT_ACTION_FLAG_CAST_IT_V1, FLAT_ACTION_FLAG_CHANGE_TARGET_V1, FLAT_ACTION_FLAG_INCLUDE_V1,
    FLAT_ACTION_FLAG_PAY_V1, FLAT_ACTION_FLAG_USE_COST_V1, FLAT_ACTION_FLAG_VALUE_V1,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha512};
use std::collections::BTreeMap;
use std::fmt;

pub(crate) const NATIVE_FLAT_STATE_FEATURE_DIM_V1: usize = 219;
pub(crate) const NATIVE_FLAT_OBJECT_FEATURE_DIM_V1: usize = 98;
pub(crate) const NATIVE_FLAT_EDGE_FEATURE_DIM_V1: usize = 41;
pub(crate) const NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V1: usize = 99;
pub(crate) const NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V1: usize = 96;
pub(crate) const NATIVE_FLAT_ACTION_FEATURE_DIM_V1: usize = 195;
pub(crate) const NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1: usize = 25;
pub(crate) const NATIVE_FLAT_OBJECT_GROUP_COUNT_V1: usize = 20;

pub(crate) const NATIVE_FLAT_CURRENT_MAX_CARD_TOKEN_V1: u32 = u16::MAX as u32;
/// Domain-coverage blocker: Python admits `card_db_id == 65_535`, whose
/// one-based token is 65_536. `FlatScorerActionRefV1.card_token` is currently
/// `u16`, so that token cannot enter this V1 view without a schema widening.
pub(crate) const NATIVE_FLAT_PYTHON_REQUIRED_MAX_CARD_TOKEN_V1: u32 =
    NATIVE_FLAT_CURRENT_MAX_CARD_TOKEN_V1 + 1;

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

/// Owned row-major counterpart of Python `EncodedDecision`.
///
/// The integer vectors use `i64`, matching Torch `long`. This action-only
/// checkpoint fills the five `action*` fields transactionally and leaves the
/// observation fields for a later injective scorer-view contract.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeFlatDecisionTensorV1 {
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
pub(crate) enum NativeFlatTensorErrorV1 {
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
}

impl fmt::Display for NativeFlatTensorErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "native flat tensorization failed: {self:?}")
    }
}

impl std::error::Error for NativeFlatTensorErrorV1 {}

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
pub(crate) fn fill_native_flat_action_tensors_v1(
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

pub(crate) fn validate_native_flat_action_half_v1(
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
        let encoded = encode_action_v1(decision, action_index, action, &refs[start..end])?;
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
        decision.objects().len(),
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
            semantic_order.sort_by_key(|reference| reference.raw.model_object_index);
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
        ref_features.push(action_ref_features_v1(reference));
        ref_card_ids.push(i64::from(reference.resolved.raw.card_token));
        ref_node_indices.push(i64::from(reference.resolved.raw.model_object_index));
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
        if object.card_token != u32::from(reference.card_token)
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
        Value::from(u32::from(reference.raw.card_token) - 1),
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
) -> [f32; NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1] {
    let mut out = [0.0f32; NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1];
    out[usize::from(reference.role)] = 1.0;
    card_ref_features_v1(Some(reference.resolved), &mut out[10..23])
        .expect("resolved action references have canonical card identity");
    out[23] = scaled_number_v1(i64::from(reference.order_index), 32.0);
    out[24] = scaled_number_v1(i64::from(reference.associated_order), 32.0);
    out
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
    use crate::flat_policy_v1::{FlatGlobalsV1, FlatManaColorV1};
    use serde::Deserialize;
    use sha2::Sha256;

    const GOLDEN: &str = include_str!("../../data/flat_policy_v1/python_action_features_v1.json");
    const FEATURES_PY: &[u8] = include_bytes!("../../python/mtg_kernel_rl/features.py");

    #[derive(Deserialize)]
    struct GoldenDoc {
        authority_sha256: String,
        payload_sha256: String,
        dimensions: GoldenDimensions,
        current_rust_card_token_max: u32,
        domain_coverage_blockers: Vec<DomainBlocker>,
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
    struct DomainBlocker {
        name: String,
        python_card_token: u32,
        status: String,
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
        card_token: u16,
        model_object_index: u32,
    }

    fn golden() -> GoldenDoc {
        serde_json::from_str(GOLDEN).expect("Python action golden must parse")
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
            let encoded = encode_action_v1(decision, 0, &actions[0], &refs)
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
    fn card_token_65536_is_an_explicit_domain_blocker() {
        let document = golden();
        assert_eq!(
            document.current_rust_card_token_max,
            NATIVE_FLAT_CURRENT_MAX_CARD_TOKEN_V1
        );
        assert_eq!(NATIVE_FLAT_CURRENT_MAX_CARD_TOKEN_V1, 65_535);
        assert_eq!(NATIVE_FLAT_PYTHON_REQUIRED_MAX_CARD_TOKEN_V1, 65_536);
        assert!(u16::try_from(NATIVE_FLAT_PYTHON_REQUIRED_MAX_CARD_TOKEN_V1).is_err());
        assert_eq!(document.domain_coverage_blockers.len(), 1);
        let blocker = &document.domain_coverage_blockers[0];
        assert_eq!(blocker.name, "python-only-card-token-65536");
        assert_eq!(
            blocker.python_card_token,
            NATIVE_FLAT_PYTHON_REQUIRED_MAX_CARD_TOKEN_V1
        );
        assert_eq!(blocker.status, "domain-coverage-blocker");
        assert!(document
            .cases
            .iter()
            .any(|case| case.name == "card-token-65535"));
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
}
