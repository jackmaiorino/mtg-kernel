//! Additive, test-only Net8 action-ingress admission diagnostic.
//!
//! This module is deliberately nested under the existing reliance probe so it
//! can reuse that probe's frozen corpus, score, metric, and digest primitives.
//! Nothing in this file is reachable from a production library build.

use super::*;
use crate::common_model_snapshot_v1::{
    common_model_snapshot_paths_v1, load_common_model_snapshot_v1, MODEL_INIT_SEED_V1,
    SNAPSHOT_IDENTITY_V1,
};
use crate::flat_policy_v2::{
    flat_action_ref_projection_role_id_v2, FlatObjectCoreV2, FlatObjectGroupV2,
    FlatObjectSourceKindV2, FlatRelativePlayerV2, FlatScorerActionCoreV2, FlatScorerActionKindV2,
    FlatScorerActionRefV2, FlatZoneV2,
};
use crate::native_flat_tensorizer_v2::{
    diagnostic_native_flat_action_semantic_bindings_v2, fill_native_flat_action_tensors_v2,
};
use crate::native_policy_train_step_v1::NativePolicyValueTrainStateV1;
use crate::native_policy_value_net_v1::{NativeActionIngressCaptureV1, ACTION_REF_FEATURE_DIM_V1};
use crate::rl::{ActionSemanticV1, CardStableRefV1, PlayerSeatV1};
use crate::rl_session::{
    FastActorSemanticBindingAuditV2, FLAT_ACTION_FLAG_INCLUDE_V1, FLAT_ACTION_FLAG_VALUE_V1,
};
use serde::Serialize;
use std::collections::HashSet;

const ADMISSION_SCHEMA_V2: &str = "mtg-kernel-action-ingress-admission-envelope/v2";
const ADMISSION_PAYLOAD_SCHEMA_V2: &str = "mtg-kernel-action-ingress-admission-payload/v2";
const ADMISSION_TEST_IDENTITY_V2: &str = "native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::action_ingress_admission_v2::official_action_ingress_admission_probe_v2";
const STRUCTURED_REPAIR_IDENTITY_V1: &str =
    "net8-action-kind-conditioned-boolean-slot69-counterfactual-v1";
const DIGEST_GATE_IDENTITY_V1: &str = "net8-fixed-action-digest-gate-f32-v1";
const MODEL_ROLE_ENV_V2: &str = "ACTION_INGRESS_V2_MODEL_ROLE";
const STORE_ROOT_ENV_V2: &str = "ACTION_INGRESS_V2_STORE_ROOT";
const REPORT_MARKER_V2: &str = "ACTION_INGRESS_ADMISSION_V2_JSON=";
const PRE_TRANSFORM_BINDING_IDENTITY_V1: &str =
    "sha256-length-framed-retained-action-semantic-operational-core-ref-object-scorer-projection-canonical-json-tail-v1";
const PRE_TRANSFORM_BINDING_ENCODING_V1: &str =
    "ordered typed rows; atom=u32be(label_len)||label||u64be(value_len)||value; integer and f32-bit arrays are little-endian; JSON and digest blocks are raw bytes";
const SLOT69_V1: usize = 69;
const ACTION_NON_DIGEST_INGRESS_DIM_V1: usize = ACTION_EXPLICIT_END_V1 + HIDDEN_DIM_V1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DigestGateV1 {
    Full,
    Zero,
    Scaled(u32),
}

impl DigestGateV1 {
    const fn name(self) -> &'static str {
        match self {
            Self::Full => "FULL",
            Self::Zero => "ZERO",
            Self::Scaled(_) => "SCALED",
        }
    }

    fn scale_v1(self) -> std::result::Result<Option<f32>, AdmissionTransformErrorV1> {
        match self {
            Self::Full | Self::Zero => Ok(None),
            Self::Scaled(bits) => {
                let scale = f32::from_bits(bits);
                if bits & 0x8000_0000 != 0 || !scale.is_finite() {
                    return Err(AdmissionTransformErrorV1::InvalidScaleBits);
                }
                Ok(Some(scale))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdmissionTransformErrorV1 {
    ActionShape,
    ActionKind,
    ActionFlag,
    FrozenSlot69,
    InvalidScaleBits,
    MissingScaleBits,
    MalformedScaleBits,
}

fn parse_gate_v1(
    mode: &str,
    scale_bits: Option<&str>,
) -> std::result::Result<DigestGateV1, AdmissionTransformErrorV1> {
    match (mode, scale_bits) {
        ("FULL", None) => Ok(DigestGateV1::Full),
        ("ZERO", None) => Ok(DigestGateV1::Zero),
        ("SCALED", Some(bits))
            if bits.len() == 8
                && bits
                    .as_bytes()
                    .iter()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)) =>
        {
            let bits = u32::from_str_radix(bits, 16)
                .map_err(|_| AdmissionTransformErrorV1::MalformedScaleBits)?;
            let gate = DigestGateV1::Scaled(bits);
            gate.scale_v1()?;
            Ok(gate)
        }
        ("SCALED", None) => Err(AdmissionTransformErrorV1::MissingScaleBits),
        ("SCALED", Some(_)) => Err(AdmissionTransformErrorV1::MalformedScaleBits),
        _ => Err(AdmissionTransformErrorV1::MalformedScaleBits),
    }
}

fn exact_positive_zero_v1(value: f32) -> bool {
    value.to_bits() == 0
}

fn repair_and_gate_v1(
    baseline: &NativeFlatDecisionTensorV2,
    actions: &[FlatScorerActionCoreV2],
    gate: DigestGateV1,
) -> std::result::Result<NativeFlatDecisionTensorV2, AdmissionTransformErrorV1> {
    if baseline.action_features.len() != actions.len() * ACTION_FEATURE_DIM_V1 || actions.is_empty()
    {
        return Err(AdmissionTransformErrorV1::ActionShape);
    }
    let scale = gate.scale_v1()?;
    let mut output = baseline.clone();
    for (row_index, (row, action)) in output
        .action_features
        .chunks_exact_mut(ACTION_FEATURE_DIM_V1)
        .zip(actions)
        .enumerate()
    {
        let kind = action.kind as usize;
        if kind >= 27
            || row[..27].iter().enumerate().any(|(index, value)| {
                value.to_bits() != if index == kind { 1.0f32.to_bits() } else { 0 }
            })
        {
            return Err(AdmissionTransformErrorV1::ActionKind);
        }
        match action.kind {
            FlatScorerActionKindV2::ChooseEffectBoolean => {
                if action.flags & !FLAT_ACTION_FLAG_VALUE_V1 != 0 {
                    return Err(AdmissionTransformErrorV1::ActionFlag);
                }
                let expected = if action.flags & FLAT_ACTION_FLAG_VALUE_V1 != 0 {
                    1.0f32.to_bits()
                } else {
                    0
                };
                if row[SLOT69_V1].to_bits() != expected {
                    return Err(AdmissionTransformErrorV1::FrozenSlot69);
                }
            }
            FlatScorerActionKindV2::ChooseAttackerInclusion
            | FlatScorerActionKindV2::ChooseBlockerInclusion => {
                if action.flags & !FLAT_ACTION_FLAG_INCLUDE_V1 != 0
                    || !exact_positive_zero_v1(row[SLOT69_V1])
                {
                    return Err(if action.flags & !FLAT_ACTION_FLAG_INCLUDE_V1 != 0 {
                        AdmissionTransformErrorV1::ActionFlag
                    } else {
                        AdmissionTransformErrorV1::FrozenSlot69
                    });
                }
                row[SLOT69_V1] = if action.flags & FLAT_ACTION_FLAG_INCLUDE_V1 != 0 {
                    1.0
                } else {
                    0.0
                };
            }
            _ => {}
        }
        match gate {
            DigestGateV1::Full => {}
            DigestGateV1::Zero => row[ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1].fill(0.0),
            DigestGateV1::Scaled(_) => {
                let scale = scale.expect("validated SCALED gate has a bound scale");
                for value in &mut row[ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1] {
                    *value *= scale;
                }
            }
        }
        debug_assert_eq!(row_index < actions.len(), true);
    }
    Ok(output)
}

fn f32_bits_v1(values: &[f32]) -> Vec<u32> {
    values.iter().map(|value| value.to_bits()).collect()
}

fn f32_slices_bit_exact_v1(left: &[f32], right: &[f32]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.to_bits() == right.to_bits())
}

fn native_tensor_bit_exact_v1(
    left: &NativeFlatDecisionTensorV2,
    right: &NativeFlatDecisionTensorV2,
) -> bool {
    f32_slices_bit_exact_v1(&left.state, &right.state)
        && f32_slices_bit_exact_v1(&left.object_features, &right.object_features)
        && left.object_card_ids == right.object_card_ids
        && left.object_groups == right.object_groups
        && left.object_node_ids == right.object_node_ids
        && f32_slices_bit_exact_v1(&left.edge_features, &right.edge_features)
        && left.edge_source_indices == right.edge_source_indices
        && left.edge_target_indices == right.edge_target_indices
        && f32_slices_bit_exact_v1(&left.action_features, &right.action_features)
        && f32_slices_bit_exact_v1(&left.action_ref_features, &right.action_ref_features)
        && left.action_ref_card_ids == right.action_ref_card_ids
        && left.action_ref_action_indices == right.action_ref_action_indices
        && left.action_ref_node_indices == right.action_ref_node_indices
}

fn native_tensor_corpora_bit_exact_v1(
    left: &[NativeFlatDecisionTensorV2],
    right: &[NativeFlatDecisionTensorV2],
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| native_tensor_bit_exact_v1(left, right))
}

fn non_action_tensor_fields_bit_exact_v1(
    left: &NativeFlatDecisionTensorV2,
    right: &NativeFlatDecisionTensorV2,
) -> bool {
    f32_slices_bit_exact_v1(&left.state, &right.state)
        && f32_slices_bit_exact_v1(&left.object_features, &right.object_features)
        && left.object_card_ids == right.object_card_ids
        && left.object_groups == right.object_groups
        && left.object_node_ids == right.object_node_ids
        && f32_slices_bit_exact_v1(&left.edge_features, &right.edge_features)
        && left.edge_source_indices == right.edge_source_indices
        && left.edge_target_indices == right.edge_target_indices
        && f32_slices_bit_exact_v1(&left.action_ref_features, &right.action_ref_features)
        && left.action_ref_card_ids == right.action_ref_card_ids
        && left.action_ref_action_indices == right.action_ref_action_indices
        && left.action_ref_node_indices == right.action_ref_node_indices
}

fn non_action_tensor_corpora_bit_exact_v1(
    baseline: &[NativeFlatDecisionTensorV2],
    actual: &[NativeFlatDecisionTensorV2],
) -> bool {
    baseline.len() == actual.len()
        && baseline
            .iter()
            .zip(actual)
            .all(|(baseline, actual)| non_action_tensor_fields_bit_exact_v1(baseline, actual))
}

fn decision_scores_bit_exact_v1(left: &[DecisionScoreV1], right: &[DecisionScoreV1]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            f32_slices_bit_exact_v1(&left.logits, &right.logits)
                && left.value.to_bits() == right.value.to_bits()
        })
}

fn parameter_snapshots_bit_exact_v1(
    left: &[NativeNamedParameterV1],
    right: &[NativeNamedParameterV1],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.name == right.name
                && left.shape == right.shape
                && f32_slices_bit_exact_v1(&left.values, &right.values)
        })
}

fn assert_only_slot69_and_digest_may_change_v1(
    before: &NativeFlatDecisionTensorV2,
    after: &NativeFlatDecisionTensorV2,
    gate: DigestGateV1,
) {
    assert!(f32_slices_bit_exact_v1(&before.state, &after.state));
    assert!(f32_slices_bit_exact_v1(
        &before.object_features,
        &after.object_features
    ));
    assert_eq!(before.object_card_ids, after.object_card_ids);
    assert_eq!(before.object_groups, after.object_groups);
    assert_eq!(before.object_node_ids, after.object_node_ids);
    assert!(f32_slices_bit_exact_v1(
        &before.edge_features,
        &after.edge_features
    ));
    assert_eq!(before.edge_source_indices, after.edge_source_indices);
    assert_eq!(before.edge_target_indices, after.edge_target_indices);
    assert!(f32_slices_bit_exact_v1(
        &before.action_ref_features,
        &after.action_ref_features
    ));
    assert_eq!(before.action_ref_card_ids, after.action_ref_card_ids);
    assert_eq!(
        before.action_ref_action_indices,
        after.action_ref_action_indices
    );
    assert_eq!(
        before.action_ref_node_indices,
        after.action_ref_node_indices
    );
    for (left, right) in before
        .action_features
        .chunks_exact(ACTION_FEATURE_DIM_V1)
        .zip(after.action_features.chunks_exact(ACTION_FEATURE_DIM_V1))
    {
        for column in 0..ACTION_FEATURE_DIM_V1 {
            if column == SLOT69_V1 || (ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1).contains(&column) {
                continue;
            }
            assert_eq!(left[column].to_bits(), right[column].to_bits());
        }
        if gate == DigestGateV1::Full {
            assert_eq!(
                f32_bits_v1(&left[ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1]),
                f32_bits_v1(&right[ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1])
            );
        }
    }
}

fn synthetic_action_tensor_v1(
    kinds_and_flags: &[(FlatScorerActionKindV2, u16)],
) -> (NativeFlatDecisionTensorV2, Vec<FlatScorerActionCoreV2>) {
    let mut tensor = synthetic_tensor_v1(1, kinds_and_flags.len());
    tensor.action_features.fill(0.0);
    let actions = kinds_and_flags
        .iter()
        .map(|(kind, flags)| FlatScorerActionCoreV2 {
            kind: *kind,
            flags: *flags,
            ..FlatScorerActionCoreV2::default()
        })
        .collect::<Vec<_>>();
    for (row, action) in tensor
        .action_features
        .chunks_exact_mut(ACTION_FEATURE_DIM_V1)
        .zip(&actions)
    {
        row[action.kind as usize] = 1.0;
        if action.kind == FlatScorerActionKindV2::ChooseEffectBoolean
            && action.flags & FLAT_ACTION_FLAG_VALUE_V1 != 0
        {
            row[SLOT69_V1] = 1.0;
        }
        for (column, value) in row[ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1]
            .iter_mut()
            .enumerate()
        {
            *value = (column as f32 - 47.5) / 48.0;
        }
    }
    (tensor, actions)
}

#[test]
fn slot69_repair_and_fixed_digest_gates_are_exact_and_fail_closed_v2() {
    let (baseline, actions) = synthetic_action_tensor_v1(&[
        (FlatScorerActionKindV2::ChooseEffectBoolean, 0),
        (
            FlatScorerActionKindV2::ChooseEffectBoolean,
            FLAT_ACTION_FLAG_VALUE_V1,
        ),
        (FlatScorerActionKindV2::ChooseAttackerInclusion, 0),
        (
            FlatScorerActionKindV2::ChooseAttackerInclusion,
            FLAT_ACTION_FLAG_INCLUDE_V1,
        ),
        (FlatScorerActionKindV2::ChooseBlockerInclusion, 0),
        (
            FlatScorerActionKindV2::ChooseBlockerInclusion,
            FLAT_ACTION_FLAG_INCLUDE_V1,
        ),
        (FlatScorerActionKindV2::Pass, 0),
    ]);

    let full = repair_and_gate_v1(&baseline, &actions, DigestGateV1::Full).unwrap();
    assert_only_slot69_and_digest_may_change_v1(&baseline, &full, DigestGateV1::Full);
    assert_eq!(
        full.action_features
            .chunks_exact(ACTION_FEATURE_DIM_V1)
            .map(|row| row[SLOT69_V1].to_bits())
            .collect::<Vec<_>>(),
        vec![
            0,
            1.0f32.to_bits(),
            0,
            1.0f32.to_bits(),
            0,
            1.0f32.to_bits(),
            0
        ]
    );

    let zero = repair_and_gate_v1(&baseline, &actions, DigestGateV1::Zero).unwrap();
    assert_only_slot69_and_digest_may_change_v1(&baseline, &zero, DigestGateV1::Zero);
    assert!(zero
        .action_features
        .chunks_exact(ACTION_FEATURE_DIM_V1)
        .all(|row| row[ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1]
            .iter()
            .all(|value| exact_positive_zero_v1(*value))));

    let half_bits = 0.5f32.to_bits();
    let scaled = repair_and_gate_v1(&baseline, &actions, DigestGateV1::Scaled(half_bits)).unwrap();
    for (before, after) in baseline
        .action_features
        .chunks_exact(ACTION_FEATURE_DIM_V1)
        .zip(scaled.action_features.chunks_exact(ACTION_FEATURE_DIM_V1))
    {
        for column in ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1 {
            assert_eq!(after[column].to_bits(), (before[column] * 0.5).to_bits());
        }
    }

    assert_eq!(parse_gate_v1("FULL", None).unwrap().name(), "FULL");
    assert_eq!(
        parse_gate_v1("SCALED", Some("3f000000")).unwrap(),
        DigestGateV1::Scaled(half_bits)
    );
    assert_eq!(
        parse_gate_v1("SCALED", None),
        Err(AdmissionTransformErrorV1::MissingScaleBits)
    );
    for bits in [
        (-0.0f32).to_bits(),
        (-1.0f32).to_bits(),
        f32::NAN.to_bits(),
        f32::INFINITY.to_bits(),
    ] {
        assert_eq!(
            repair_and_gate_v1(&baseline, &actions, DigestGateV1::Scaled(bits)),
            Err(AdmissionTransformErrorV1::InvalidScaleBits)
        );
    }

    let mut bad_kind = baseline.clone();
    bad_kind.action_features[0] = 1.0;
    assert_eq!(
        repair_and_gate_v1(&bad_kind, &actions, DigestGateV1::Full),
        Err(AdmissionTransformErrorV1::ActionKind)
    );
    let mut bad_slot = baseline.clone();
    bad_slot.action_features[2 * ACTION_FEATURE_DIM_V1 + SLOT69_V1] = -0.0;
    assert_eq!(
        repair_and_gate_v1(&bad_slot, &actions, DigestGateV1::Full),
        Err(AdmissionTransformErrorV1::FrozenSlot69)
    );
    assert_semantic_binding_mismatch_seams_v1();

    let session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
        991_001,
        0x7365_6d61_6e74_6963,
        1_024,
        65_536,
        ["Rally".to_owned(), "Rally".to_owned()],
    )
    .unwrap();
    let semantics = session.diagnostic_current_action_semantics().unwrap();
    assert!(semantics.len() > 1);
    let (live_binding, scoring) = live_semantic_decision_binding_v1(&session, &semantics);
    assert!(operational_scorer_binding_exact_v1(
        &live_binding.operational,
        &scoring
    ));
    let mut reordered = semantics.clone();
    reordered.swap(0, 1);
    assert!(session
        .diagnostic_bind_retained_action_semantics_v2(&reordered)
        .is_err());
    assert!(session
        .diagnostic_bind_retained_action_semantics_v2(&semantics[..semantics.len() - 1])
        .is_err());
    let first_reference = live_binding
        .operational
        .operational_refs
        .first()
        .expect("opening Rally decision has referenced card actions");
    let mut bad_projection = live_binding.action_object_to_model_object.clone();
    bad_projection[usize::from(first_reference.object_index)] ^= 0x8000_0000;
    assert_ne!(
        operational_scorer_projection_v1(&live_binding.operational, &scoring),
        Some(bad_projection)
    );
    let mut bad_scorer_ref = scoring;
    bad_scorer_ref.action_refs[0].model_object_index ^= 0x8000_0000;
    assert!(!operational_scorer_binding_exact_v1(
        &live_binding.operational,
        &bad_scorer_ref
    ));
}

#[derive(Clone, Debug, PartialEq)]
struct RetainedSemanticBindingRowV1 {
    decision_index: usize,
    action_index: usize,
    semantic_json: Vec<u8>,
    core: FlatScorerActionCoreV2,
    raw_refs: Vec<FlatScorerActionRefV2>,
    canonical_model_json: Vec<u8>,
    sha512_blocks: Vec<u8>,
    frozen_digest_tail_bits: Vec<u32>,
    encoded_ref_feature_bits: Vec<u32>,
    encoded_ref_card_ids: Vec<i64>,
    encoded_ref_node_indices: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq)]
struct LiveSemanticDecisionBindingV1 {
    semantics: Vec<ActionSemanticV1>,
    operational: FastActorSemanticBindingAuditV2,
    scorer_actions: Vec<FlatScorerActionCoreV2>,
    scorer_refs: Vec<FlatScorerActionRefV2>,
    action_object_to_model_object: Vec<u32>,
}

#[derive(Clone)]
struct AdmissionCorpusV1 {
    tensors: Vec<NativeFlatDecisionTensorV2>,
    scoring_decisions: Vec<OwnedScoringDecisionV1>,
    live_sessions: Vec<FastActorSessionV1>,
    live_semantic_bindings: Vec<LiveSemanticDecisionBindingV1>,
    canonical_semantics: Vec<Vec<Vec<u8>>>,
    semantic_binding_rows: Vec<RetainedSemanticBindingRowV1>,
    semantic_binding_capture_sha256: String,
    episode_count: usize,
    multi_action_decision_count: usize,
    total_action_count: usize,
    sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SemanticBindingMismatchV1 {
    Encoder,
    Count,
    Order,
    Semantic,
    ActionKind,
    Core,
    References,
    CanonicalModelJson,
    CanonicalModelDigest,
    FrozenDigestTail,
}

fn operational_scorer_binding_exact_v1(
    operational: &FastActorSemanticBindingAuditV2,
    scoring: &OwnedScoringDecisionV1,
) -> bool {
    operational_scorer_projection_v1(operational, scoring).is_some()
}

fn operational_object_matches_model_object_v1(
    operational: crate::rl_session::FlatActionObjectV2,
    model: FlatObjectCoreV2,
) -> bool {
    let (group_matches, ordinal_matches) = match operational.group {
        crate::rl_session::FlatActionObjectGroupV1::SelfHand => (
            model.group == FlatObjectGroupV2::SelfHand,
            model.visible_ordinal == u32::from(operational.actor_visible_ordinal),
        ),
        crate::rl_session::FlatActionObjectGroupV1::KnownOpponentHand => (
            model.group == FlatObjectGroupV2::KnownOpponentHand,
            model.visible_ordinal == u32::from(operational.actor_visible_ordinal),
        ),
        crate::rl_session::FlatActionObjectGroupV1::SelfBattlefield => (
            model.group == FlatObjectGroupV2::SelfBattlefield,
            model.visible_ordinal == u32::from(operational.actor_visible_ordinal),
        ),
        crate::rl_session::FlatActionObjectGroupV1::OpponentBattlefield => (
            model.group == FlatObjectGroupV2::OpponentBattlefield,
            model.visible_ordinal == u32::from(operational.actor_visible_ordinal),
        ),
        crate::rl_session::FlatActionObjectGroupV1::SelfGraveyard => (
            model.group == FlatObjectGroupV2::SelfGraveyard,
            model.visible_ordinal == u32::from(operational.actor_visible_ordinal),
        ),
        crate::rl_session::FlatActionObjectGroupV1::OpponentGraveyard => (
            model.group == FlatObjectGroupV2::OpponentGraveyard,
            model.visible_ordinal == u32::from(operational.actor_visible_ordinal),
        ),
        crate::rl_session::FlatActionObjectGroupV1::Exile => (
            model.group == FlatObjectGroupV2::Exile,
            model.visible_ordinal == u32::from(operational.actor_visible_ordinal),
        ),
        crate::rl_session::FlatActionObjectGroupV1::Stack => {
            let pending = model.group == FlatObjectGroupV2::PendingContext
                && model.source_kind == FlatObjectSourceKindV2::Pending;
            (
                model.group == FlatObjectGroupV2::Stack || pending,
                pending || model.visible_ordinal == u32::from(operational.actor_visible_ordinal),
            )
        }
        crate::rl_session::FlatActionObjectGroupV1::Command => (false, false),
        crate::rl_session::FlatActionObjectGroupV1::KnownSelfLibrary => (
            model.group == FlatObjectGroupV2::KnownSelfLibrary,
            model.visible_ordinal == u32::from(operational.actor_visible_ordinal),
        ),
        crate::rl_session::FlatActionObjectGroupV1::KnownOpponentLibrary => (
            model.group == FlatObjectGroupV2::KnownOpponentLibrary,
            model.visible_ordinal == u32::from(operational.actor_visible_ordinal),
        ),
    };
    group_matches
        && ordinal_matches
        && operational.card_token == model.card_token
        && operational.owner_relative == model.owner as u8
        && operational.controller_relative == model.controller as u8
        && Some(operational.zone) == model.zone.map(|zone| zone as u8)
}

fn operational_scorer_projection_v1(
    operational: &FastActorSemanticBindingAuditV2,
    scoring: &OwnedScoringDecisionV1,
) -> Option<Vec<u32>> {
    if operational.operational_actions.len() != scoring.actions.len()
        || operational.operational_refs.len() != scoring.action_refs.len()
    {
        return None;
    }
    if !operational
        .operational_actions
        .iter()
        .copied()
        .map(FlatScorerActionCoreV2::from)
        .eq(scoring.actions.iter().copied())
    {
        return None;
    }
    let mut projection = vec![None; operational.operational_objects.len()];
    for (operational_ref, scorer_ref) in operational
        .operational_refs
        .iter()
        .zip(&scoring.action_refs)
    {
        if operational_ref.action_index != scorer_ref.action_index
            || flat_action_ref_projection_role_id_v2(operational_ref.role)
                != scorer_ref.projection_role_id
            || operational_ref.order_index != scorer_ref.order_index
            || operational_ref.associated_order != scorer_ref.associated_order
            || operational_ref.card_token != scorer_ref.card_token
        {
            return None;
        }
        let slot = projection.get_mut(usize::from(operational_ref.object_index))?;
        match slot {
            Some(expected) if *expected != scorer_ref.model_object_index => return None,
            Some(_) => {}
            None => *slot = Some(scorer_ref.model_object_index),
        }
    }
    let projection = projection.into_iter().collect::<Option<Vec<_>>>()?;
    let distinct = projection.iter().copied().collect::<HashSet<_>>();
    if distinct.len() != projection.len()
        || projection.iter().zip(&operational.operational_objects).any(
            |(model_object_index, operational_object)| {
                scoring
                    .objects
                    .get(*model_object_index as usize)
                    .map_or(true, |model_object| {
                        !operational_object_matches_model_object_v1(
                            *operational_object,
                            *model_object,
                        )
                    })
            },
        )
    {
        return None;
    }
    Some(projection)
}

fn live_semantic_decision_binding_v1(
    session: &FastActorSessionV1,
    semantics: &[ActionSemanticV1],
) -> (LiveSemanticDecisionBindingV1, OwnedScoringDecisionV1) {
    let operational = session
        .diagnostic_bind_retained_action_semantics_v2(semantics)
        .expect("retained typed semantics must bind the live production V2 cache");
    let scoring = OwnedScoringDecisionV1::from_session_v1(session);
    let action_object_to_model_object = operational_scorer_projection_v1(&operational, &scoring)
        .expect("regenerated operational core/refs must bind exact scorer core/refs and objects");
    (
        LiveSemanticDecisionBindingV1 {
            semantics: semantics.to_vec(),
            operational,
            scorer_actions: scoring.actions.clone(),
            scorer_refs: scoring.action_refs.clone(),
            action_object_to_model_object,
        },
        scoring,
    )
}

fn semantic_binding_rows_for_decision_v1(
    decision_index: usize,
    tensor: &NativeFlatDecisionTensorV2,
    decision: &OwnedScoringDecisionV1,
    semantic_json: &[Vec<u8>],
) -> std::result::Result<Vec<RetainedSemanticBindingRowV1>, SemanticBindingMismatchV1> {
    let encoded = diagnostic_native_flat_action_semantic_bindings_v2(decision.view_v1())
        .map_err(|_| SemanticBindingMismatchV1::Encoder)?;
    let action_rows = tensor
        .action_features
        .chunks_exact(ACTION_FEATURE_DIM_V1)
        .collect::<Vec<_>>();
    if decision.actions.len() != semantic_json.len()
        || decision.actions.len() != encoded.len()
        || decision.actions.len() != action_rows.len()
    {
        return Err(SemanticBindingMismatchV1::Count);
    }
    if tensor.action_ref_features.len()
        != tensor.action_ref_card_ids.len() * ACTION_REF_FEATURE_DIM_V1
        || tensor.action_ref_card_ids.len() != tensor.action_ref_action_indices.len()
        || tensor.action_ref_card_ids.len() != tensor.action_ref_node_indices.len()
    {
        return Err(SemanticBindingMismatchV1::References);
    }

    let mut rows = Vec::with_capacity(decision.actions.len());
    let mut raw_ref_cursor = 0usize;
    let mut encoded_ref_cursor = 0usize;
    for action_index in 0..decision.actions.len() {
        let core = decision.actions[action_index];
        let binding = &encoded[action_index];
        let frozen_row = action_rows[action_index];
        let semantic_value: serde_json::Value =
            serde_json::from_slice(&semantic_json[action_index])
                .map_err(|_| SemanticBindingMismatchV1::Semantic)?;
        let model_value: serde_json::Value = serde_json::from_slice(&binding.canonical_json)
            .map_err(|_| SemanticBindingMismatchV1::CanonicalModelJson)?;
        if semantic_value.get("action_kind")
            != model_value
                .get("semantic")
                .and_then(|semantic| semantic.get("action_kind"))
        {
            return Err(SemanticBindingMismatchV1::ActionKind);
        }
        if model_value
            .get("semantic")
            .and_then(|semantic| semantic.get("actor"))
            .and_then(serde_json::Value::as_str)
            != Some("self")
        {
            return Err(SemanticBindingMismatchV1::CanonicalModelJson);
        }
        match core.kind {
            FlatScorerActionKindV2::ChooseAttackerInclusion
            | FlatScorerActionKindV2::ChooseBlockerInclusion => {
                let semantic_include = semantic_value
                    .get("include")
                    .and_then(serde_json::Value::as_bool);
                let model_include = model_value
                    .get("semantic")
                    .and_then(|semantic| semantic.get("include"))
                    .and_then(serde_json::Value::as_bool);
                let core_include = core.flags & FLAT_ACTION_FLAG_INCLUDE_V1 != 0;
                if semantic_include != Some(core_include) {
                    return Err(SemanticBindingMismatchV1::Core);
                }
                if model_include != semantic_include {
                    return Err(SemanticBindingMismatchV1::CanonicalModelJson);
                }
            }
            FlatScorerActionKindV2::ChooseEffectBoolean => {
                let semantic_value_bit = semantic_value
                    .get("value")
                    .and_then(serde_json::Value::as_bool);
                let model_value_bit = model_value
                    .get("semantic")
                    .and_then(|semantic| semantic.get("value"))
                    .and_then(serde_json::Value::as_bool);
                let core_value = core.flags & FLAT_ACTION_FLAG_VALUE_V1 != 0;
                if semantic_value_bit != Some(core_value) {
                    return Err(SemanticBindingMismatchV1::Core);
                }
                if model_value_bit != semantic_value_bit {
                    return Err(SemanticBindingMismatchV1::CanonicalModelJson);
                }
            }
            _ => {}
        }

        let start =
            usize::try_from(core.ref_start).map_err(|_| SemanticBindingMismatchV1::References)?;
        let end = start
            .checked_add(usize::from(core.ref_len))
            .ok_or(SemanticBindingMismatchV1::References)?;
        if start != raw_ref_cursor || end > decision.action_refs.len() {
            return Err(SemanticBindingMismatchV1::References);
        }
        let raw_refs = decision.action_refs[start..end].to_vec();
        if raw_refs
            .iter()
            .any(|reference| usize::try_from(reference.action_index).ok() != Some(action_index))
        {
            return Err(SemanticBindingMismatchV1::Order);
        }
        raw_ref_cursor = end;

        if f32_bits_v1(&binding.action_features[..ACTION_EXPLICIT_END_V1])
            != f32_bits_v1(&frozen_row[..ACTION_EXPLICIT_END_V1])
        {
            return Err(SemanticBindingMismatchV1::Core);
        }
        let encoded_tail =
            f32_bits_v1(&binding.action_features[ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1]);
        let frozen_tail = f32_bits_v1(&frozen_row[ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1]);
        if encoded_tail != frozen_tail {
            return Err(SemanticBindingMismatchV1::FrozenDigestTail);
        }

        let encoded_ref_count = binding.action_ref_features.len();
        if encoded_ref_count != usize::from(core.ref_len)
            || binding.action_ref_card_ids.len() != encoded_ref_count
            || binding.action_ref_node_indices.len() != encoded_ref_count
        {
            return Err(SemanticBindingMismatchV1::References);
        }
        let encoded_ref_end = encoded_ref_cursor
            .checked_add(encoded_ref_count)
            .ok_or(SemanticBindingMismatchV1::References)?;
        if encoded_ref_end > tensor.action_ref_card_ids.len() {
            return Err(SemanticBindingMismatchV1::References);
        }
        let frozen_ref_feature_bits = f32_bits_v1(
            &tensor.action_ref_features[encoded_ref_cursor * ACTION_REF_FEATURE_DIM_V1
                ..encoded_ref_end * ACTION_REF_FEATURE_DIM_V1],
        );
        let encoded_ref_feature_bits = binding
            .action_ref_features
            .iter()
            .flat_map(|features| features.iter().map(|value| value.to_bits()))
            .collect::<Vec<_>>();
        if frozen_ref_feature_bits != encoded_ref_feature_bits
            || tensor.action_ref_card_ids[encoded_ref_cursor..encoded_ref_end]
                != binding.action_ref_card_ids
            || tensor.action_ref_node_indices[encoded_ref_cursor..encoded_ref_end]
                != binding.action_ref_node_indices
            || tensor.action_ref_action_indices[encoded_ref_cursor..encoded_ref_end]
                .iter()
                .any(|actual| usize::try_from(*actual).ok() != Some(action_index))
        {
            return Err(SemanticBindingMismatchV1::References);
        }

        rows.push(RetainedSemanticBindingRowV1 {
            decision_index,
            action_index,
            semantic_json: semantic_json[action_index].clone(),
            core,
            raw_refs,
            canonical_model_json: binding.canonical_json.clone(),
            sha512_blocks: binding
                .sha512_blocks
                .iter()
                .flat_map(|block| block.iter().copied())
                .collect(),
            frozen_digest_tail_bits: frozen_tail,
            encoded_ref_feature_bits,
            encoded_ref_card_ids: binding.action_ref_card_ids.clone(),
            encoded_ref_node_indices: binding.action_ref_node_indices.clone(),
        });
        encoded_ref_cursor = encoded_ref_end;
    }
    if raw_ref_cursor != decision.action_refs.len()
        || encoded_ref_cursor != tensor.action_ref_card_ids.len()
    {
        return Err(SemanticBindingMismatchV1::References);
    }
    Ok(rows)
}

fn action_core_without_kind_v1(mut core: FlatScorerActionCoreV2) -> FlatScorerActionCoreV2 {
    core.kind = FlatScorerActionKindV2::Pass;
    core
}

fn compare_semantic_binding_rows_v1(
    expected: &[RetainedSemanticBindingRowV1],
    actual: &[RetainedSemanticBindingRowV1],
) -> std::result::Result<(), SemanticBindingMismatchV1> {
    if expected.len() != actual.len() {
        return Err(SemanticBindingMismatchV1::Count);
    }
    for (expected, actual) in expected.iter().zip(actual) {
        if (expected.decision_index, expected.action_index)
            != (actual.decision_index, actual.action_index)
        {
            return Err(SemanticBindingMismatchV1::Order);
        }
        if expected.semantic_json != actual.semantic_json {
            return Err(SemanticBindingMismatchV1::Semantic);
        }
        if expected.core.kind != actual.core.kind {
            return Err(SemanticBindingMismatchV1::ActionKind);
        }
        if action_core_without_kind_v1(expected.core) != action_core_without_kind_v1(actual.core) {
            return Err(SemanticBindingMismatchV1::Core);
        }
        if expected.raw_refs != actual.raw_refs
            || expected.encoded_ref_feature_bits != actual.encoded_ref_feature_bits
            || expected.encoded_ref_card_ids != actual.encoded_ref_card_ids
            || expected.encoded_ref_node_indices != actual.encoded_ref_node_indices
        {
            return Err(SemanticBindingMismatchV1::References);
        }
        if expected.canonical_model_json != actual.canonical_model_json {
            return Err(SemanticBindingMismatchV1::CanonicalModelJson);
        }
        if expected.sha512_blocks != actual.sha512_blocks {
            return Err(SemanticBindingMismatchV1::CanonicalModelDigest);
        }
        if expected.frozen_digest_tail_bits != actual.frozen_digest_tail_bits {
            return Err(SemanticBindingMismatchV1::FrozenDigestTail);
        }
    }
    Ok(())
}

fn assert_semantic_binding_mismatch_seams_v1() {
    let mut first = RetainedSemanticBindingRowV1 {
        decision_index: 0,
        action_index: 0,
        semantic_json: br#"{"action_kind":"pass","actor":"p0"}"#.to_vec(),
        core: FlatScorerActionCoreV2::default(),
        raw_refs: vec![FlatScorerActionRefV2::default()],
        canonical_model_json: br#"{"semantic":{"action_kind":"pass","actor":"self"}}"#.to_vec(),
        sha512_blocks: vec![0; 6 * 64],
        frozen_digest_tail_bits: vec![0; ACTION_HASH_END_V1 - ACTION_HASH_BEGIN_V1],
        encoded_ref_feature_bits: vec![0; ACTION_REF_FEATURE_DIM_V1],
        encoded_ref_card_ids: vec![0],
        encoded_ref_node_indices: vec![0],
    };
    let mut second = first.clone();
    second.action_index = 1;
    let expected = vec![first.clone(), second];
    assert_eq!(
        compare_semantic_binding_rows_v1(&expected, &expected),
        Ok(())
    );

    let mut actual = expected.clone();
    actual.pop();
    assert_eq!(
        compare_semantic_binding_rows_v1(&expected, &actual),
        Err(SemanticBindingMismatchV1::Count)
    );
    let mut actual = expected.clone();
    actual.swap(0, 1);
    assert_eq!(
        compare_semantic_binding_rows_v1(&expected, &actual),
        Err(SemanticBindingMismatchV1::Order)
    );
    let mut actual = expected.clone();
    actual[0].semantic_json.push(b' ');
    assert_eq!(
        compare_semantic_binding_rows_v1(&expected, &actual),
        Err(SemanticBindingMismatchV1::Semantic)
    );
    let mut actual = expected.clone();
    actual[0].core.kind = FlatScorerActionKindV2::PlayLand;
    assert_eq!(
        compare_semantic_binding_rows_v1(&expected, &actual),
        Err(SemanticBindingMismatchV1::ActionKind)
    );
    let mut actual = expected.clone();
    actual[0].core.flags = 1;
    assert_eq!(
        compare_semantic_binding_rows_v1(&expected, &actual),
        Err(SemanticBindingMismatchV1::Core)
    );
    let mut actual = expected.clone();
    actual[0].raw_refs[0].card_token = 1;
    assert_eq!(
        compare_semantic_binding_rows_v1(&expected, &actual),
        Err(SemanticBindingMismatchV1::References)
    );
    let mut actual = expected.clone();
    actual[0].canonical_model_json.push(b' ');
    assert_eq!(
        compare_semantic_binding_rows_v1(&expected, &actual),
        Err(SemanticBindingMismatchV1::CanonicalModelJson)
    );
    let mut actual = expected.clone();
    actual[0].sha512_blocks[0] = 1;
    assert_eq!(
        compare_semantic_binding_rows_v1(&expected, &actual),
        Err(SemanticBindingMismatchV1::CanonicalModelDigest)
    );
    let mut actual = expected.clone();
    actual[0].frozen_digest_tail_bits[0] = 1;
    assert_eq!(
        compare_semantic_binding_rows_v1(&expected, &actual),
        Err(SemanticBindingMismatchV1::FrozenDigestTail)
    );
    first.encoded_ref_feature_bits[0] = 1;
    assert_eq!(
        compare_semantic_binding_rows_v1(&expected, &[first, expected[1].clone()]),
        Err(SemanticBindingMismatchV1::References)
    );
}

fn hash_u32_bits_v1(hasher: &mut Sha256, label: &[u8], values: &[u32]) {
    let bytes = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    hash_atom_v1(hasher, label, &bytes);
}

fn hash_i64_values_v1(hasher: &mut Sha256, label: &[u8], values: &[i64]) {
    let bytes = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    hash_atom_v1(hasher, label, &bytes);
}

fn hash_action_core_v1(hasher: &mut Sha256, core: FlatScorerActionCoreV2) {
    hash_atom_v1(hasher, b"core.kind", &[core.kind as u8]);
    hash_atom_v1(hasher, b"core.flags", &core.flags.to_le_bytes());
    hash_atom_v1(hasher, b"core.ability_index", &[core.ability_index]);
    hash_atom_v1(hasher, b"core.remaining", &[core.remaining]);
    hash_atom_v1(hasher, b"core.mode_index", &[core.mode_index]);
    hash_atom_v1(hasher, b"core.mode_count", &[core.mode_count]);
    hash_atom_v1(
        hasher,
        b"core.option_index",
        &core.option_index.to_le_bytes(),
    );
    hash_atom_v1(
        hasher,
        b"core.option_count",
        &core.option_count.to_le_bytes(),
    );
    hash_atom_v1(
        hasher,
        b"core.selected_count",
        &core.selected_count.to_le_bytes(),
    );
    hash_atom_v1(hasher, b"core.min_targets", &core.min_targets.to_le_bytes());
    hash_atom_v1(hasher, b"core.max_targets", &core.max_targets.to_le_bytes());
    hash_atom_v1(hasher, b"core.number", &core.number.to_le_bytes());
    hash_atom_v1(hasher, b"core.minimum", &core.minimum.to_le_bytes());
    hash_atom_v1(hasher, b"core.maximum", &core.maximum.to_le_bytes());
    hash_atom_v1(hasher, b"core.mana_choice", &[core.mana_choice]);
    hash_atom_v1(hasher, b"core.color", &[core.color]);
    hash_atom_v1(hasher, b"core.cast_mode", &[core.cast_mode]);
    hash_atom_v1(hasher, b"core.cost_kind", &[core.cost_kind]);
    hash_atom_v1(
        hasher,
        b"core.optional_cost_choice",
        &[core.optional_cost_choice],
    );
    hash_atom_v1(hasher, b"core.target_kind", &[core.target_kind]);
    hash_atom_v1(hasher, b"core.target_player", &[core.target_player]);
    hash_atom_v1(hasher, b"core.ref_start", &core.ref_start.to_le_bytes());
    hash_atom_v1(hasher, b"core.ref_len", &core.ref_len.to_le_bytes());
}

fn hash_action_ref_v1(hasher: &mut Sha256, reference: FlatScorerActionRefV2) {
    hash_atom_v1(
        hasher,
        b"ref.action_index",
        &reference.action_index.to_le_bytes(),
    );
    hash_atom_v1(
        hasher,
        b"ref.projection_role_id",
        &[reference.projection_role_id],
    );
    hash_atom_v1(
        hasher,
        b"ref.order_index",
        &reference.order_index.to_le_bytes(),
    );
    hash_atom_v1(
        hasher,
        b"ref.associated_order",
        &reference.associated_order.to_le_bytes(),
    );
    hash_atom_v1(
        hasher,
        b"ref.card_token",
        &reference.card_token.to_le_bytes(),
    );
    hash_atom_v1(
        hasher,
        b"ref.model_object_index",
        &reference.model_object_index.to_le_bytes(),
    );
}

fn hash_live_semantic_binding_v1(
    hasher: &mut Sha256,
    decision_index: usize,
    binding: &LiveSemanticDecisionBindingV1,
) {
    hash_atom_v1(
        hasher,
        b"live.decision_index",
        &(decision_index as u64).to_le_bytes(),
    );
    hash_atom_v1(
        hasher,
        b"live.semantic_count",
        &(binding.semantics.len() as u64).to_le_bytes(),
    );
    for (action_index, semantic) in binding.semantics.iter().enumerate() {
        hash_atom_v1(
            hasher,
            b"live.semantic_action_index",
            &(action_index as u64).to_le_bytes(),
        );
        hash_atom_v1(
            hasher,
            b"live.typed_semantic_json",
            &serde_json::to_vec(semantic).expect("typed action semantic serializes"),
        );
    }
    let authority = binding.operational.binding;
    for (label, value) in [
        (b"binding.slice_version".as_slice(), authority.slice_version),
        (
            b"binding.ref_role_mapping_version".as_slice(),
            authority.ref_role_mapping_version,
        ),
        (
            b"binding.card_token_mapping_version".as_slice(),
            authority.card_token_mapping_version,
        ),
        (
            b"binding.candidate_commitment_version".as_slice(),
            authority.candidate_commitment_version,
        ),
        (b"binding.substep_index".as_slice(), authority.substep_index),
        (b"binding.substep_count".as_slice(), authority.substep_count),
        (
            b"binding.legal_action_count".as_slice(),
            authority.legal_action_count,
        ),
    ] {
        hash_atom_v1(hasher, label, &value.to_le_bytes());
    }
    for (label, value) in [
        (b"binding.card_db_hash".as_slice(), authority.card_db_hash),
        (b"binding.episode_id".as_slice(), authority.episode_id),
        (
            b"binding.environment_revision".as_slice(),
            authority.environment_revision,
        ),
        (
            b"binding.bound_policy_step_count".as_slice(),
            authority.bound_policy_step_count,
        ),
        (
            b"binding.physical_decision_id".as_slice(),
            authority.physical_decision_id,
        ),
        (
            b"binding.bound_physical_decision_count".as_slice(),
            authority.bound_physical_decision_count,
        ),
    ] {
        hash_atom_v1(hasher, label, &value.to_le_bytes());
    }
    hash_atom_v1(hasher, b"binding.acting_player", &[authority.acting_player]);
    hash_atom_v1(hasher, b"binding.decision_kind", &[authority.decision_kind]);
    hash_atom_v1(
        hasher,
        b"binding.candidate_order_commitment",
        &authority.candidate_order_commitment,
    );

    hash_atom_v1(
        hasher,
        b"operational_action_count",
        &(binding.operational.operational_actions.len() as u64).to_le_bytes(),
    );
    for action in binding.operational.operational_actions.iter().copied() {
        hash_action_core_v1(hasher, FlatScorerActionCoreV2::from(action));
    }
    hash_atom_v1(
        hasher,
        b"operational_ref_count",
        &(binding.operational.operational_refs.len() as u64).to_le_bytes(),
    );
    for reference in binding.operational.operational_refs.iter().copied() {
        hash_atom_v1(
            hasher,
            b"operational_ref.action_index",
            &reference.action_index.to_le_bytes(),
        );
        hash_atom_v1(hasher, b"operational_ref.role", &[reference.role as u8]);
        hash_atom_v1(
            hasher,
            b"operational_ref.order_index",
            &reference.order_index.to_le_bytes(),
        );
        hash_atom_v1(
            hasher,
            b"operational_ref.associated_order",
            &reference.associated_order.to_le_bytes(),
        );
        hash_atom_v1(
            hasher,
            b"operational_ref.card_token",
            &reference.card_token.to_le_bytes(),
        );
        hash_atom_v1(
            hasher,
            b"operational_ref.object_index",
            &reference.object_index.to_le_bytes(),
        );
    }
    hash_atom_v1(
        hasher,
        b"operational_object_count",
        &(binding.operational.operational_objects.len() as u64).to_le_bytes(),
    );
    for object in binding.operational.operational_objects.iter().copied() {
        hash_atom_v1(
            hasher,
            b"operational_object.card_token",
            &object.card_token.to_le_bytes(),
        );
        hash_atom_v1(hasher, b"operational_object.group", &[object.group as u8]);
        hash_atom_v1(
            hasher,
            b"operational_object.actor_visible_ordinal",
            &object.actor_visible_ordinal.to_le_bytes(),
        );
        hash_atom_v1(
            hasher,
            b"operational_object.owner_relative",
            &[object.owner_relative],
        );
        hash_atom_v1(
            hasher,
            b"operational_object.controller_relative",
            &[object.controller_relative],
        );
        hash_atom_v1(hasher, b"operational_object.zone", &[object.zone]);
        hash_atom_v1(
            hasher,
            b"operational_object.zone_change_count",
            &object.zone_change_count.to_le_bytes(),
        );
    }
    hash_atom_v1(
        hasher,
        b"action_object_to_model_object_count",
        &(binding.action_object_to_model_object.len() as u64).to_le_bytes(),
    );
    for model_object_index in binding.action_object_to_model_object.iter().copied() {
        hash_atom_v1(
            hasher,
            b"action_object_to_model_object",
            &model_object_index.to_le_bytes(),
        );
    }
    hash_atom_v1(
        hasher,
        b"scorer_action_count",
        &(binding.scorer_actions.len() as u64).to_le_bytes(),
    );
    for action in binding.scorer_actions.iter().copied() {
        hash_action_core_v1(hasher, action);
    }
    hash_atom_v1(
        hasher,
        b"scorer_ref_count",
        &(binding.scorer_refs.len() as u64).to_le_bytes(),
    );
    for reference in binding.scorer_refs.iter().copied() {
        hash_action_ref_v1(hasher, reference);
    }
}

fn retained_semantic_binding_sha256_v1(
    rows: &[RetainedSemanticBindingRowV1],
    live_bindings: &[LiveSemanticDecisionBindingV1],
) -> String {
    let mut hasher = Sha256::new();
    hash_atom_v1(
        &mut hasher,
        b"identity",
        PRE_TRANSFORM_BINDING_IDENTITY_V1.as_bytes(),
    );
    hash_atom_v1(
        &mut hasher,
        b"live_decision_count",
        &(live_bindings.len() as u64).to_le_bytes(),
    );
    for (decision_index, binding) in live_bindings.iter().enumerate() {
        hash_live_semantic_binding_v1(&mut hasher, decision_index, binding);
    }
    hash_atom_v1(
        &mut hasher,
        b"row_count",
        &(rows.len() as u64).to_le_bytes(),
    );
    for row in rows {
        hash_atom_v1(
            &mut hasher,
            b"decision_index",
            &(row.decision_index as u64).to_le_bytes(),
        );
        hash_atom_v1(
            &mut hasher,
            b"action_index",
            &(row.action_index as u64).to_le_bytes(),
        );
        hash_atom_v1(&mut hasher, b"semantic_json", &row.semantic_json);
        hash_action_core_v1(&mut hasher, row.core);
        hash_atom_v1(
            &mut hasher,
            b"raw_ref_count",
            &(row.raw_refs.len() as u64).to_le_bytes(),
        );
        for (ref_index, reference) in row.raw_refs.iter().copied().enumerate() {
            hash_atom_v1(&mut hasher, b"ref_index", &(ref_index as u64).to_le_bytes());
            hash_action_ref_v1(&mut hasher, reference);
        }
        hash_atom_v1(
            &mut hasher,
            b"canonical_model_json",
            &row.canonical_model_json,
        );
        hash_atom_v1(&mut hasher, b"sha512_blocks", &row.sha512_blocks);
        hash_u32_bits_v1(
            &mut hasher,
            b"frozen_digest_tail_f32_bits",
            &row.frozen_digest_tail_bits,
        );
        hash_u32_bits_v1(
            &mut hasher,
            b"encoded_ref_feature_f32_bits",
            &row.encoded_ref_feature_bits,
        );
        hash_i64_values_v1(
            &mut hasher,
            b"encoded_ref_card_ids",
            &row.encoded_ref_card_ids,
        );
        hash_i64_values_v1(
            &mut hasher,
            b"encoded_ref_node_indices",
            &row.encoded_ref_node_indices,
        );
    }
    lower_hex_raw32_v1(hasher.finalize().into())
}

fn build_admission_corpus_v1(decision_count: usize) -> AdmissionCorpusV1 {
    assert!(decision_count > 1);
    let mut tensors = Vec::with_capacity(decision_count);
    let mut scoring_decisions = Vec::with_capacity(decision_count);
    let mut live_sessions = Vec::with_capacity(decision_count);
    let mut live_semantic_bindings = Vec::with_capacity(decision_count);
    let mut canonical_semantics = Vec::with_capacity(decision_count);
    let mut semantic_binding_rows = Vec::new();
    let mut tensorizer = NativeFlatTensorizerV2::new();
    let mut episode_ordinal = 0usize;

    while tensors.len() < decision_count {
        assert!(episode_ordinal < CORPUS_MAX_EPISODES_V1);
        let episode_id = CORPUS_BASE_EPISODE_ID_V1 + episode_ordinal as u64;
        let environment_seed = fixed_environment_seed_v1(episode_ordinal);
        let mut selection_state =
            environment_seed ^ episode_id.rotate_left(17) ^ 0x756e_6966_6f72_6d31;
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            environment_seed,
            1_024,
            65_536,
            ["Rally".to_owned(), "Rally".to_owned()],
        )
        .expect("fixed admission corpus session reset");

        for _ in 0..CORPUS_DECISIONS_PER_EPISODE_V1 {
            if tensors.len() == decision_count {
                break;
            }
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                break;
            };
            let semantics = session
                .diagnostic_current_action_semantics()
                .expect("fixed admission corpus has a live decision");
            let (live_semantic_binding, owned) =
                live_semantic_decision_binding_v1(&session, &semantics);
            assert_eq!(semantics.len(), owned.actions.len());
            let mut tensor = NativeFlatDecisionTensorV2::default();
            tensorizer
                .fill(owned.view_v1(), &mut tensor)
                .expect("fixed admission corpus tensorization");
            assert_eq!(action_count_v1(&tensor), semantics.len());
            let semantic_json = semantics
                .iter()
                .map(|semantic| {
                    serde_json::to_vec(semantic)
                        .expect("action semantics have deterministic JSON serialization")
                })
                .collect::<Vec<_>>();
            semantic_binding_rows.extend(
                semantic_binding_rows_for_decision_v1(
                    tensors.len(),
                    &tensor,
                    &owned,
                    &semantic_json,
                )
                .expect("fixed admission corpus semantic/core/ref/model-json/tail binding"),
            );
            canonical_semantics.push(semantic_json);
            live_sessions.push(session.clone());
            live_semantic_bindings.push(live_semantic_binding);
            scoring_decisions.push(owned);
            tensors.push(tensor);

            let selected = (splitmix64_next_v1(&mut selection_state)
                % u64::from(expected.legal_action_count)) as u32;
            session
                .step(expected.episode_id, expected.step, selected)
                .expect("fixed admission corpus modulo-index step");
        }
        episode_ordinal += 1;
    }

    let multi_action_decision_count = tensors
        .iter()
        .filter(|tensor| action_count_v1(tensor) > 1)
        .count();
    let total_action_count = tensors.iter().map(action_count_v1).sum();
    let sha256 = corpus_sha256_v1(&tensors);
    let semantic_binding_capture_sha256 =
        retained_semantic_binding_sha256_v1(&semantic_binding_rows, &live_semantic_bindings);
    AdmissionCorpusV1 {
        tensors,
        scoring_decisions,
        live_sessions,
        live_semantic_bindings,
        canonical_semantics,
        semantic_binding_rows,
        semantic_binding_capture_sha256,
        episode_count: episode_ordinal,
        multi_action_decision_count,
        total_action_count,
        sha256,
    }
}

#[derive(Clone, Debug, Serialize)]
struct PreTransformBindingReportV1 {
    identity: &'static str,
    transcript_encoding: &'static str,
    all_rows_passed: bool,
    decision_count: usize,
    row_count: usize,
    action_reference_count: usize,
    operational_object_count: usize,
    action_object_projection_count: usize,
    live_session_semantics_to_core_refs_revalidated_at_capture: bool,
    live_session_semantics_to_core_refs_revalidated_pre_transform: bool,
    typed_semantics_exact: bool,
    production_v2_binding_exact: bool,
    operational_core_refs_exact: bool,
    scorer_core_refs_exact: bool,
    operational_object_to_scorer_model_object_exact: bool,
    zone_change_count_retained_in_operational_identity: bool,
    count_and_order_exact: bool,
    action_kind_exact: bool,
    action_core_exact: bool,
    action_references_exact: bool,
    canonical_model_json_exact: bool,
    canonical_model_digest_exact: bool,
    frozen_digest_tail_exact: bool,
    capture_sha256: String,
    revalidated_sha256: String,
    capture_matches_revalidation: bool,
}

fn validate_pre_transform_semantic_bindings_v1(
    corpus: &AdmissionCorpusV1,
) -> PreTransformBindingReportV1 {
    assert_eq!(corpus.tensors.len(), corpus.scoring_decisions.len());
    assert_eq!(corpus.tensors.len(), corpus.live_sessions.len());
    assert_eq!(corpus.tensors.len(), corpus.live_semantic_bindings.len());
    assert_eq!(corpus.tensors.len(), corpus.canonical_semantics.len());
    let mut revalidated = Vec::with_capacity(corpus.total_action_count);
    let mut revalidated_live = Vec::with_capacity(corpus.tensors.len());
    for decision_index in 0..corpus.tensors.len() {
        let retained_typed = &corpus.live_semantic_bindings[decision_index].semantics;
        let (live_binding, scoring) = live_semantic_decision_binding_v1(
            &corpus.live_sessions[decision_index],
            retained_typed,
        );
        assert_eq!(
            live_binding, corpus.live_semantic_bindings[decision_index],
            "live typed semantic/operational/scorer binding replay must be exact"
        );
        let semantic_json = retained_typed
            .iter()
            .map(|semantic| {
                serde_json::to_vec(semantic)
                    .expect("retained typed action semantics deterministically serialize")
            })
            .collect::<Vec<_>>();
        assert_eq!(semantic_json, corpus.canonical_semantics[decision_index]);
        revalidated.extend(
            semantic_binding_rows_for_decision_v1(
                decision_index,
                &corpus.tensors[decision_index],
                &scoring,
                &semantic_json,
            )
            .expect("retained semantics must revalidate against core/refs/model JSON/frozen tail"),
        );
        revalidated_live.push(live_binding);
    }
    compare_semantic_binding_rows_v1(&corpus.semantic_binding_rows, &revalidated)
        .expect("pre-transform retained semantic binding replay must be exact");
    let revalidated_sha256 = retained_semantic_binding_sha256_v1(&revalidated, &revalidated_live);
    let capture_matches_revalidation = corpus.semantic_binding_capture_sha256 == revalidated_sha256;
    assert!(capture_matches_revalidation);
    assert_eq!(revalidated.len(), corpus.total_action_count);

    PreTransformBindingReportV1 {
        identity: PRE_TRANSFORM_BINDING_IDENTITY_V1,
        transcript_encoding: PRE_TRANSFORM_BINDING_ENCODING_V1,
        all_rows_passed: true,
        decision_count: corpus.tensors.len(),
        row_count: revalidated.len(),
        action_reference_count: revalidated.iter().map(|row| row.raw_refs.len()).sum(),
        operational_object_count: revalidated_live
            .iter()
            .map(|binding| binding.operational.operational_objects.len())
            .sum(),
        action_object_projection_count: revalidated_live
            .iter()
            .map(|binding| binding.action_object_to_model_object.len())
            .sum(),
        live_session_semantics_to_core_refs_revalidated_at_capture: true,
        live_session_semantics_to_core_refs_revalidated_pre_transform: true,
        typed_semantics_exact: true,
        production_v2_binding_exact: true,
        operational_core_refs_exact: true,
        scorer_core_refs_exact: true,
        operational_object_to_scorer_model_object_exact: true,
        zone_change_count_retained_in_operational_identity: true,
        count_and_order_exact: true,
        action_kind_exact: true,
        action_core_exact: true,
        action_references_exact: true,
        canonical_model_json_exact: true,
        canonical_model_digest_exact: true,
        frozen_digest_tail_exact: true,
        capture_sha256: corpus.semantic_binding_capture_sha256.clone(),
        revalidated_sha256,
        capture_matches_revalidation,
    }
}

fn transformed_corpus_v1(
    corpus: &AdmissionCorpusV1,
    gate: DigestGateV1,
) -> Vec<NativeFlatDecisionTensorV2> {
    corpus
        .tensors
        .iter()
        .zip(&corpus.scoring_decisions)
        .map(|(tensor, decision)| {
            let transformed = repair_and_gate_v1(tensor, &decision.actions, gate)
                .expect("frozen corpus must satisfy the counterfactual adapter");
            assert_only_slot69_and_digest_may_change_v1(tensor, &transformed, gate);
            transformed
        })
        .collect()
}

fn rotate_digest_upstream_then_zero_v1(
    corpus: &AdmissionCorpusV1,
) -> Vec<NativeFlatDecisionTensorV2> {
    corpus
        .tensors
        .iter()
        .zip(&corpus.scoring_decisions)
        .map(|(tensor, decision)| {
            let mut stressed = tensor.clone();
            rotate_action_block_in_place_v1(
                &mut stressed,
                ACTION_HASH_BEGIN_V1,
                ACTION_HASH_END_V1,
                1,
            );
            repair_and_gate_v1(&stressed, &decision.actions, DigestGateV1::Zero)
                .expect("digest rotation remains a valid adapter input")
        })
        .collect()
}

fn canonical_semantics_are_pairwise_distinct_v1(corpus: &AdmissionCorpusV1) -> bool {
    corpus.canonical_semantics.iter().all(|decision| {
        let distinct = decision.iter().cloned().collect::<HashSet<_>>();
        distinct.len() == decision.len()
    })
}

fn parameter_v1<'a>(
    parameters: &'a [NativeNamedParameterV1],
    name: &'static str,
    shape: &[usize],
) -> &'a [f32] {
    let parameter = parameters
        .iter()
        .find(|parameter| parameter.name == name)
        .unwrap_or_else(|| panic!("missing diagnostic parameter {name}"));
    assert_eq!(parameter.shape, shape);
    assert!(parameter.values.iter().all(|value| value.is_finite()));
    &parameter.values
}

fn non_digest_action_ingress_v1(
    tensor: &NativeFlatDecisionTensorV2,
    action_ref_pooled: &[f32],
) -> Vec<Vec<f32>> {
    assert_eq!(
        action_ref_pooled.len(),
        action_count_v1(tensor) * HIDDEN_DIM_V1
    );
    tensor
        .action_features
        .chunks_exact(ACTION_FEATURE_DIM_V1)
        .enumerate()
        .map(|(action, row)| {
            let mut ingress = Vec::with_capacity(ACTION_NON_DIGEST_INGRESS_DIM_V1);
            ingress.extend_from_slice(&row[..ACTION_EXPLICIT_END_V1]);
            ingress.extend_from_slice(
                &action_ref_pooled[action * HIDDEN_DIM_V1..(action + 1) * HIDDEN_DIM_V1],
            );
            assert_eq!(ingress.len(), ACTION_NON_DIGEST_INGRESS_DIM_V1);
            assert!(ingress.iter().all(|value| value.is_finite()));
            ingress
        })
        .collect()
}

fn exact_forward_capture_matches_v1(
    capture: &NativeActionIngressCaptureV1,
    tensor: &NativeFlatDecisionTensorV2,
) -> bool {
    capture.identity == "native-policy-value-net8-exact-pre-action-encoder-ingress-v1"
        && capture.schema == NativeEncodedDecisionSchemaV1::contract_v1()
        && capture.action_count == action_count_v1(tensor)
        && capture.hidden_dim == HIDDEN_DIM_V1
        && capture.action_ref_pooled.len() == capture.action_count * capture.hidden_dim
        && capture
            .action_ref_pooled
            .iter()
            .all(|value| value.is_finite())
}

fn assert_exact_forward_capture_v1(
    capture: &NativeActionIngressCaptureV1,
    tensor: &NativeFlatDecisionTensorV2,
) {
    assert!(exact_forward_capture_matches_v1(capture, tensor));
}

fn numerically_distinct_v1(left: &[f32], right: &[f32]) -> bool {
    assert_eq!(left.len(), right.len());
    assert!(left.iter().chain(right).all(|value| value.is_finite()));
    left.iter().zip(right).any(|(left, right)| left != right)
}

fn ingress_stream_sha256_v1(ingress: &[Vec<Vec<f32>>]) -> String {
    let mut hasher = Sha256::new();
    hash_atom_v1(
        &mut hasher,
        b"identity",
        b"net8-repaired-zero-action-nondigest-ingress-163-f32le-v1",
    );
    for (decision, rows) in ingress.iter().enumerate() {
        hash_atom_v1(
            &mut hasher,
            b"decision_index",
            &(decision as u64).to_be_bytes(),
        );
        for (row, values) in rows.iter().enumerate() {
            hash_atom_v1(&mut hasher, b"row_index", &(row as u64).to_be_bytes());
            hash_f32_slice_v1(&mut hasher, b"ingress", values);
        }
    }
    lower_hex_raw32_v1(hasher.finalize().into())
}

#[derive(Clone, Debug, Serialize)]
struct IngressRowDigestV1 {
    decision_index: usize,
    action_index: usize,
    sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum InclusionPairKeyV1 {
    Attacker {
        actor: PlayerSeatV1,
        attacker: CardStableRefV1,
    },
    Blocker {
        actor: PlayerSeatV1,
        attacker: CardStableRefV1,
        blocker: CardStableRefV1,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InclusionPairKindV1 {
    Attacker,
    Blocker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SemanticInclusionPairV1 {
    kind: InclusionPairKindV1,
    false_index: usize,
    true_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SemanticInclusionPairErrorV1 {
    DuplicatePolarity,
    MissingPolarity,
}

fn semantic_inclusion_pairs_v1(
    semantics: &[ActionSemanticV1],
) -> std::result::Result<Vec<SemanticInclusionPairV1>, SemanticInclusionPairErrorV1> {
    struct PendingPairV1 {
        key: InclusionPairKeyV1,
        kind: InclusionPairKindV1,
        false_index: Option<usize>,
        true_index: Option<usize>,
    }

    let mut pending = Vec::<PendingPairV1>::new();
    for (action_index, semantic) in semantics.iter().enumerate() {
        let (key, kind, include) = match semantic {
            ActionSemanticV1::ChooseAttackerInclusion {
                actor,
                attacker,
                include,
            } => (
                InclusionPairKeyV1::Attacker {
                    actor: *actor,
                    attacker: attacker.clone(),
                },
                InclusionPairKindV1::Attacker,
                *include,
            ),
            ActionSemanticV1::ChooseBlockerInclusion {
                actor,
                attacker,
                blocker,
                include,
            } => (
                InclusionPairKeyV1::Blocker {
                    actor: *actor,
                    attacker: attacker.clone(),
                    blocker: blocker.clone(),
                },
                InclusionPairKindV1::Blocker,
                *include,
            ),
            _ => continue,
        };
        let pair_index = pending
            .iter()
            .position(|candidate| candidate.key == key)
            .unwrap_or_else(|| {
                pending.push(PendingPairV1 {
                    key,
                    kind,
                    false_index: None,
                    true_index: None,
                });
                pending.len() - 1
            });
        let slot = if include {
            &mut pending[pair_index].true_index
        } else {
            &mut pending[pair_index].false_index
        };
        if slot.replace(action_index).is_some() {
            return Err(SemanticInclusionPairErrorV1::DuplicatePolarity);
        }
    }

    pending
        .into_iter()
        .map(|pair| {
            Ok(SemanticInclusionPairV1 {
                kind: pair.kind,
                false_index: pair
                    .false_index
                    .ok_or(SemanticInclusionPairErrorV1::MissingPolarity)?,
                true_index: pair
                    .true_index
                    .ok_or(SemanticInclusionPairErrorV1::MissingPolarity)?,
            })
        })
        .collect()
}

#[derive(Clone, Debug, Serialize)]
struct IngressAdmissionV1 {
    exact_forward_capture_identity: &'static str,
    exact_forward_schema_version: &'static str,
    exact_forward_registry_version: &'static str,
    exact_forward_contract_digest: &'static str,
    exact_forward_encoding_digest: &'static str,
    exact_forward_hidden_dim: usize,
    exact_forward_schema_matches_frozen_contract: bool,
    exact_forward_capture_decision_count: usize,
    exact_forward_pooled_value_count: usize,
    canonical_semantics_pairwise_distinct: bool,
    repaired_zero_ingress_pairwise_distinct: bool,
    semantic_inclusion_pairs_complete_one_to_one: bool,
    semantic_inclusion_pair_direct_slot69_only: bool,
    semantic_inclusion_pair_pooled_refs_bit_exact: bool,
    repaired_zero_ingress_dim: usize,
    repaired_zero_ingress_row_count: usize,
    repaired_zero_ingress_sha256: String,
    repaired_zero_ingress_row_digest_identity: &'static str,
    repaired_zero_ingress_row_digests: Vec<IngressRowDigestV1>,
    attacker_false_true_pair_count: usize,
    blocker_false_true_pair_count: usize,
}

fn ingress_admission_v1(
    model: &NativePolicyValueNetV1,
    corpus: &AdmissionCorpusV1,
    repaired_zero: &[NativeFlatDecisionTensorV2],
) -> (IngressAdmissionV1, Vec<Vec<Vec<f32>>>, Vec<DecisionScoreV1>) {
    assert_eq!(corpus.tensors.len(), repaired_zero.len());
    let mut all_ingress = Vec::with_capacity(repaired_zero.len());
    let mut scores = Vec::with_capacity(repaired_zero.len());
    let mut all_distinct = true;
    let mut attacker_pairs = 0usize;
    let mut blocker_pairs = 0usize;
    let mut capture_schema = None;
    let mut pooled_value_count = 0usize;
    for (decision_index, tensor) in repaired_zero.iter().enumerate() {
        let (output, capture) = model
            .diagnostic_forward_action_ingress_v1(encoded_decision_view_v1(tensor))
            .expect("repaired ZERO tensor must reach the exact Net8 action ingress");
        assert_exact_forward_capture_v1(&capture, tensor);
        if let Some(expected) = capture_schema {
            assert_eq!(capture.schema, expected);
        } else {
            capture_schema = Some(capture.schema);
        }
        pooled_value_count += capture.action_ref_pooled.len();
        scores.push(DecisionScoreV1 {
            logits: output.logits,
            value: output.value,
        });
        let ingress = non_digest_action_ingress_v1(tensor, &capture.action_ref_pooled);
        for left in 0..ingress.len() {
            for right in left + 1..ingress.len() {
                if !numerically_distinct_v1(&ingress[left], &ingress[right]) {
                    all_distinct = false;
                }
            }
        }
        let semantic_pairs =
            semantic_inclusion_pairs_v1(&corpus.live_semantic_bindings[decision_index].semantics)
                .expect("every retained inclusion semantic must have one exact opposite polarity");
        for pair in semantic_pairs {
            let false_row = &tensor.action_features[pair.false_index * ACTION_FEATURE_DIM_V1
                ..(pair.false_index + 1) * ACTION_FEATURE_DIM_V1];
            let true_row = &tensor.action_features[pair.true_index * ACTION_FEATURE_DIM_V1
                ..(pair.true_index + 1) * ACTION_FEATURE_DIM_V1];
            assert_eq!(false_row[SLOT69_V1].to_bits(), 0);
            assert_eq!(true_row[SLOT69_V1].to_bits(), 1.0f32.to_bits());
            assert!((0..ACTION_EXPLICIT_END_V1).all(|column| {
                column == SLOT69_V1 || false_row[column].to_bits() == true_row[column].to_bits()
            }));
            let false_pooled = &capture.action_ref_pooled
                [pair.false_index * HIDDEN_DIM_V1..(pair.false_index + 1) * HIDDEN_DIM_V1];
            let true_pooled = &capture.action_ref_pooled
                [pair.true_index * HIDDEN_DIM_V1..(pair.true_index + 1) * HIDDEN_DIM_V1];
            assert!(f32_slices_bit_exact_v1(false_pooled, true_pooled));
            match pair.kind {
                InclusionPairKindV1::Attacker => attacker_pairs += 1,
                InclusionPairKindV1::Blocker => blocker_pairs += 1,
            }
        }
        all_ingress.push(ingress);
    }
    let schema = capture_schema.expect("frozen admission corpus is nonempty");
    let row_count = all_ingress.iter().map(Vec::len).sum();
    let repaired_zero_ingress_row_digests = all_ingress
        .iter()
        .enumerate()
        .flat_map(|(decision_index, rows)| {
            rows.iter()
                .enumerate()
                .map(move |(action_index, values)| IngressRowDigestV1 {
                    decision_index,
                    action_index,
                    sha256: f32le_sha256_v1(values),
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(repaired_zero_ingress_row_digests.len(), row_count);
    let report = IngressAdmissionV1 {
        exact_forward_capture_identity:
            "native-policy-value-net8-exact-pre-action-encoder-ingress-v1",
        exact_forward_schema_version: schema.version,
        exact_forward_registry_version: schema.registry_version,
        exact_forward_contract_digest: schema.contract_digest,
        exact_forward_encoding_digest: schema.encoding_digest,
        exact_forward_hidden_dim: HIDDEN_DIM_V1,
        exact_forward_schema_matches_frozen_contract: schema
            == NativeEncodedDecisionSchemaV1::contract_v1(),
        exact_forward_capture_decision_count: repaired_zero.len(),
        exact_forward_pooled_value_count: pooled_value_count,
        canonical_semantics_pairwise_distinct: canonical_semantics_are_pairwise_distinct_v1(corpus),
        repaired_zero_ingress_pairwise_distinct: all_distinct,
        semantic_inclusion_pairs_complete_one_to_one: true,
        semantic_inclusion_pair_direct_slot69_only: true,
        semantic_inclusion_pair_pooled_refs_bit_exact: true,
        repaired_zero_ingress_dim: ACTION_NON_DIGEST_INGRESS_DIM_V1,
        repaired_zero_ingress_row_count: row_count,
        repaired_zero_ingress_sha256: ingress_stream_sha256_v1(&all_ingress),
        repaired_zero_ingress_row_digest_identity: "sha256-f32le-163-v1",
        repaired_zero_ingress_row_digests,
        attacker_false_true_pair_count: attacker_pairs,
        blocker_false_true_pair_count: blocker_pairs,
    };
    (report, all_ingress, scores)
}

#[test]
fn digest_zero_stress_and_non_digest_ingress_controls_are_exact_v2() {
    let (mut baseline, actions) = synthetic_action_tensor_v1(&[
        (FlatScorerActionKindV2::Pass, 0),
        (FlatScorerActionKindV2::ChooseAttackerInclusion, 0),
        (
            FlatScorerActionKindV2::ChooseAttackerInclusion,
            FLAT_ACTION_FLAG_INCLUDE_V1,
        ),
    ]);
    baseline.action_ref_features = vec![0.0; 3 * ACTION_REF_FEATURE_DIM_V1];
    baseline.action_ref_features[0] = 1.0;
    baseline.action_ref_features[ACTION_REF_FEATURE_DIM_V1 + 1] = 1.0;
    baseline.action_ref_features[2 * ACTION_REF_FEATURE_DIM_V1 + 1] = 1.0;
    baseline.action_ref_card_ids = vec![0, 0, 0];
    baseline.action_ref_action_indices = vec![0, 1, 2];
    baseline.action_ref_node_indices = vec![0, 0, 0];

    let zero = repair_and_gate_v1(&baseline, &actions, DigestGateV1::Zero).unwrap();
    let mut rotated = baseline.clone();
    rotate_action_block_in_place_v1(&mut rotated, ACTION_HASH_BEGIN_V1, ACTION_HASH_END_V1, 1);
    let stressed_zero = repair_and_gate_v1(&rotated, &actions, DigestGateV1::Zero).unwrap();
    assert!(native_tensor_bit_exact_v1(&zero, &stressed_zero));

    let mut finite_replacement = baseline.clone();
    for (row_index, row) in finite_replacement
        .action_features
        .chunks_exact_mut(ACTION_FEATURE_DIM_V1)
        .enumerate()
    {
        for (column, value) in row[ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1]
            .iter_mut()
            .enumerate()
        {
            *value = (row_index * 101 + column) as f32 / 127.0;
        }
    }
    assert!(native_tensor_bit_exact_v1(
        &zero,
        &repair_and_gate_v1(&finite_replacement, &actions, DigestGateV1::Zero).unwrap()
    ));

    let model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let (_output, capture) = model
        .diagnostic_forward_action_ingress_v1(encoded_decision_view_v1(&zero))
        .unwrap();
    assert_exact_forward_capture_v1(&capture, &zero);
    let mut bad_capture_schema = capture.clone();
    bad_capture_schema.schema.action_ref_feature_dim += 1;
    assert!(!exact_forward_capture_matches_v1(
        &bad_capture_schema,
        &zero
    ));
    let mut bad_capture_rows = capture.clone();
    bad_capture_rows.action_ref_pooled.pop();
    assert!(!exact_forward_capture_matches_v1(&bad_capture_rows, &zero));
    let ingress = non_digest_action_ingress_v1(&zero, &capture.action_ref_pooled);
    assert_eq!(ingress.len(), 3);
    assert!(ingress
        .iter()
        .all(|row| row.len() == ACTION_NON_DIGEST_INGRESS_DIM_V1));
    assert!(numerically_distinct_v1(&ingress[0], &ingress[1]));
    assert!(numerically_distinct_v1(&ingress[1], &ingress[2]));
    assert!(!numerically_distinct_v1(&[0.0], &[-0.0]));
    assert!(!f32_slices_bit_exact_v1(&[0.0], &[-0.0]));
    let mut signed_zero_tensor = zero.clone();
    signed_zero_tensor.state[0] = -0.0;
    let mut positive_zero_tensor = signed_zero_tensor.clone();
    positive_zero_tensor.state[0] = 0.0;
    assert!(!native_tensor_bit_exact_v1(
        &positive_zero_tensor,
        &signed_zero_tensor
    ));
    assert!(!decision_scores_bit_exact_v1(
        &[DecisionScoreV1 {
            logits: vec![0.0],
            value: 0.0,
        }],
        &[DecisionScoreV1 {
            logits: vec![-0.0],
            value: -0.0,
        }]
    ));
    assert!(!parameter_snapshots_bit_exact_v1(
        &[NativeNamedParameterV1 {
            name: "signed_zero_seam",
            shape: vec![1],
            values: vec![0.0],
        }],
        &[NativeNamedParameterV1 {
            name: "signed_zero_seam",
            shape: vec![1],
            values: vec![-0.0],
        }]
    ));
    assert!(ingress[1].iter().enumerate().all(|(column, value)| {
        column == SLOT69_V1 || value.to_bits() == ingress[2][column].to_bits()
    }));
    assert_eq!(ingress[1][SLOT69_V1].to_bits(), 0);
    assert_eq!(ingress[2][SLOT69_V1].to_bits(), 1.0f32.to_bits());

    let attacker = CardStableRefV1 {
        arena_id: 7,
        card_db_id: 40,
        owner: PlayerSeatV1::P0,
        controller: PlayerSeatV1::P0,
        zone: crate::state::Zone::Battlefield,
        zone_change_count: 3,
    };
    let pair = [
        ActionSemanticV1::ChooseAttackerInclusion {
            actor: PlayerSeatV1::P0,
            attacker: attacker.clone(),
            include: false,
        },
        ActionSemanticV1::ChooseAttackerInclusion {
            actor: PlayerSeatV1::P0,
            attacker: attacker.clone(),
            include: true,
        },
    ];
    assert_eq!(
        semantic_inclusion_pairs_v1(&pair).unwrap(),
        vec![SemanticInclusionPairV1 {
            kind: InclusionPairKindV1::Attacker,
            false_index: 0,
            true_index: 1,
        }]
    );
    assert_eq!(
        semantic_inclusion_pairs_v1(&pair[..1]),
        Err(SemanticInclusionPairErrorV1::MissingPolarity)
    );
    assert_eq!(
        semantic_inclusion_pairs_v1(&[pair[0].clone(), pair[0].clone(), pair[1].clone()]),
        Err(SemanticInclusionPairErrorV1::DuplicatePolarity)
    );
    let mut different_attacker = attacker;
    different_attacker.zone_change_count += 1;
    assert_eq!(
        semantic_inclusion_pairs_v1(&[
            pair[0].clone(),
            ActionSemanticV1::ChooseAttackerInclusion {
                actor: PlayerSeatV1::P0,
                attacker: different_attacker,
                include: true,
            },
        ]),
        Err(SemanticInclusionPairErrorV1::MissingPolarity)
    );

    let corpus = build_admission_corpus_v1(CORPUS_DECISION_COUNT_V1);
    let pre_transform_binding = validate_pre_transform_semantic_bindings_v1(&corpus);
    assert!(pre_transform_binding.capture_matches_revalidation);
    let repaired_zero = transformed_corpus_v1(&corpus, DigestGateV1::Zero);
    let zero_stress = rotate_digest_upstream_then_zero_v1(&corpus);
    assert!(native_tensor_corpora_bit_exact_v1(
        &repaired_zero,
        &zero_stress
    ));
    let (live_ingress, _, exact_scores) = ingress_admission_v1(&model, &corpus, &repaired_zero);
    assert!(live_ingress.exact_forward_schema_matches_frozen_contract);
    assert!(live_ingress.semantic_inclusion_pairs_complete_one_to_one);
    assert!(live_ingress.semantic_inclusion_pair_direct_slot69_only);
    assert!(live_ingress.semantic_inclusion_pair_pooled_refs_bit_exact);
    assert!(live_ingress.attacker_false_true_pair_count > 0);
    assert!(live_ingress.blocker_false_true_pair_count > 0);
    assert!(decision_scores_bit_exact_v1(
        &exact_scores,
        &score_corpus_v1(&model, &repaired_zero)
    ));
    assert!(decision_scores_bit_exact_v1(
        &exact_scores,
        &score_corpus_v1(&model, &zero_stress)
    ));
}

fn decode_lower_hex_v1(value: &str) -> Vec<u8> {
    fn nibble_v1(value: u8) -> u8 {
        match value {
            b'0'..=b'9' => value - b'0',
            b'a'..=b'f' => value - b'a' + 10,
            _ => panic!("checked static authority hex must be lowercase ASCII"),
        }
    }
    assert!(value.len().is_multiple_of(2));
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| (nibble_v1(pair[0]) << 4) | nibble_v1(pair[1]))
        .collect()
}

fn f32_bits_from_le_hex_v1(value: &str) -> Vec<u32> {
    decode_lower_hex_v1(value)
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect()
}

#[test]
fn supplemental_player_target_cross_runtime_tensorization_is_bit_exact_v2() {
    const STATIC_REPORT: &str =
        include_str!("../../../../data/action_ingress_admission_v1/static-admission.json");
    let report: serde_json::Value =
        serde_json::from_str(STATIC_REPORT).expect("checked static admission report");
    let supplemental = &report["supplemental_case"];
    assert_eq!(
        supplemental["name"],
        "supplemental-primary-choose-effect-target-player-self-v1"
    );
    let canonical = supplemental["canonical_json"].as_str().unwrap();
    assert_eq!(
        canonical,
        "{\"semantic\":{\"action_kind\":\"choose_effect_target\",\"actor\":\"self\",\"max_targets\":3,\"min_targets\":1,\"selected_count\":1,\"source\":{\"card_db_id\":40,\"controller\":\"self\",\"owner\":\"self\",\"zone\":\"Hand\"},\"target\":{\"player\":\"self\",\"target_kind\":\"player\"}}}"
    );

    let globals = FlatGlobalsV2 {
        acting_player: FlatRelativePlayerV2::SelfPlayer,
        ..FlatGlobalsV2::default()
    };
    let objects = [FlatObjectCoreV2 {
        card_token: 41,
        owner: FlatRelativePlayerV2::SelfPlayer,
        controller: FlatRelativePlayerV2::SelfPlayer,
        zone: Some(FlatZoneV2::Hand),
        ..FlatObjectCoreV2::default()
    }];
    let actions = [FlatScorerActionCoreV2 {
        kind: FlatScorerActionKindV2::ChooseEffectTarget,
        selected_count: 1,
        min_targets: 1,
        max_targets: 3,
        target_kind: 1,
        target_player: 1,
        ref_start: 0,
        ref_len: 1,
        ..FlatScorerActionCoreV2::default()
    }];
    let refs = [FlatScorerActionRefV2 {
        action_index: 0,
        projection_role_id: 0,
        order_index: 0,
        associated_order: 0,
        card_token: 41,
        model_object_index: 0,
    }];
    let view = FlatScoringDecisionViewV2::new(
        &globals,
        &objects,
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &actions,
        &refs,
    );
    let bindings = diagnostic_native_flat_action_semantic_bindings_v2(view)
        .expect("Rust helper must expose the exact production action encoding");
    let [binding] = bindings.as_slice() else {
        panic!("supplemental case must encode exactly one action");
    };
    assert_eq!(binding.canonical_json.as_slice(), canonical.as_bytes());
    let expected_blocks = supplemental["digests"]["sha512_blocks_hex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| decode_lower_hex_v1(value.as_str().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(expected_blocks.len(), binding.sha512_blocks.len());
    for (actual, expected) in binding.sha512_blocks.iter().zip(&expected_blocks) {
        assert_eq!(actual.as_slice(), expected.as_slice());
    }
    let mut bad_canonical = canonical.as_bytes().to_vec();
    bad_canonical[0] ^= 1;
    assert_ne!(binding.canonical_json, bad_canonical);
    let mut bad_blocks = binding.sha512_blocks;
    bad_blocks[5][63] ^= 1;
    assert_ne!(binding.sha512_blocks, bad_blocks);

    let mut output = NativeFlatDecisionTensorV2::default();
    fill_native_flat_action_tensors_v2(view, &mut output)
        .expect("Rust tensorizer must admit the predeclared supplemental case");

    let expected_action_bits = f32_bits_from_le_hex_v1(
        supplemental["tensors"]["action_features"]["f32_le_hex"]
            .as_str()
            .unwrap(),
    );
    let expected_ref_bits = f32_bits_from_le_hex_v1(
        supplemental["tensors"]["action_ref_features"]["f32_le_hex"]
            .as_str()
            .unwrap(),
    );
    assert_eq!(f32_bits_v1(&output.action_features), expected_action_bits);
    assert_eq!(f32_bits_v1(&output.action_ref_features), expected_ref_bits);
    assert_eq!(output.action_ref_card_ids, vec![41]);
    assert_eq!(output.action_ref_action_indices, vec![0]);
    assert_eq!(output.action_ref_node_indices, vec![0]);

    let blocks = binding
        .sha512_blocks
        .iter()
        .flat_map(|block| block.iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(
        lower_hex_raw32_v1(Sha256::digest(&blocks).into()),
        supplemental["digests"]["raw_digest_sha256"]
            .as_str()
            .unwrap()
    );
    let digest_tail_bytes = output.action_features[ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1]
        .iter()
        .flat_map(|value| value.to_bits().to_le_bytes())
        .collect::<Vec<_>>();
    assert_eq!(
        lower_hex_raw32_v1(Sha256::digest(&digest_tail_bytes).into()),
        supplemental["digests"]["quantized_tail_sha256"]
            .as_str()
            .unwrap()
    );
}

#[derive(Clone, Debug, Serialize)]
struct RowInputStatisticsV1 {
    decision_index: usize,
    action_index: usize,
    direct_squared_norm: f64,
    digest_squared_norm: f64,
}

#[derive(Clone, Debug, Serialize)]
struct InputStatisticsV1 {
    source_condition: &'static str,
    direct_value_count: usize,
    digest_value_count: usize,
    direct_value_rms: f64,
    digest_value_rms: f64,
    mean_direct_squared_norm: f64,
    mean_digest_squared_norm: f64,
    per_action_row: Vec<RowInputStatisticsV1>,
}

fn input_statistics_v1(tensors: &[NativeFlatDecisionTensorV2]) -> InputStatisticsV1 {
    let mut direct_squared_sum = 0.0f64;
    let mut digest_squared_sum = 0.0f64;
    let mut direct_value_count = 0usize;
    let mut digest_value_count = 0usize;
    let mut per_action_row = Vec::new();
    for (decision_index, tensor) in tensors.iter().enumerate() {
        for (action_index, row) in tensor
            .action_features
            .chunks_exact(ACTION_FEATURE_DIM_V1)
            .enumerate()
        {
            assert!(row.iter().all(|value| value.is_finite()));
            let direct_squared_norm = row[..ACTION_EXPLICIT_END_V1]
                .iter()
                .copied()
                .map(|value| {
                    let value = f64::from(value);
                    value * value
                })
                .sum::<f64>();
            let digest_squared_norm = row[ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1]
                .iter()
                .copied()
                .map(|value| {
                    let value = f64::from(value);
                    value * value
                })
                .sum::<f64>();
            direct_squared_sum += direct_squared_norm;
            digest_squared_sum += digest_squared_norm;
            direct_value_count += ACTION_EXPLICIT_END_V1;
            digest_value_count += ACTION_HASH_END_V1 - ACTION_HASH_BEGIN_V1;
            per_action_row.push(RowInputStatisticsV1 {
                decision_index,
                action_index,
                direct_squared_norm,
                digest_squared_norm,
            });
        }
    }
    let action_row_count = per_action_row.len();
    assert!(action_row_count > 0);
    InputStatisticsV1 {
        source_condition: "repaired/FULL",
        direct_value_count,
        digest_value_count,
        direct_value_rms: (direct_squared_sum / direct_value_count as f64).sqrt(),
        digest_value_rms: (digest_squared_sum / digest_value_count as f64).sqrt(),
        mean_direct_squared_norm: direct_squared_sum / action_row_count as f64,
        mean_digest_squared_norm: digest_squared_sum / action_row_count as f64,
        per_action_row,
    }
}

#[derive(Clone, Debug, Serialize)]
struct RowContributionRmsV1 {
    decision_index: usize,
    action_index: usize,
    direct_contribution_squared_norm: f64,
    digest_contribution_squared_norm: f64,
    direct_contribution_rms: f64,
    digest_contribution_rms: f64,
}

#[derive(Clone, Debug, Serialize)]
struct FirstLayerContributionRmsV1 {
    source_condition: &'static str,
    tensor_name: &'static str,
    accumulator: &'static str,
    hidden_dim: usize,
    direct_contribution_rms: f64,
    digest_contribution_rms: f64,
    per_action_row: Vec<RowContributionRmsV1>,
}

fn first_layer_contribution_rms_v1(
    parameters: &[NativeNamedParameterV1],
    tensors: &[NativeFlatDecisionTensorV2],
) -> FirstLayerContributionRmsV1 {
    let weight = parameter_v1(
        parameters,
        "action_encoder.0.weight",
        &[HIDDEN_DIM_V1, ACTION_ENCODER_INPUT_V1],
    );
    let mut direct_squared_sum = 0.0f64;
    let mut digest_squared_sum = 0.0f64;
    let mut contribution_count = 0usize;
    let mut per_action_row = Vec::new();
    for (decision_index, tensor) in tensors.iter().enumerate() {
        for (action_index, input) in tensor
            .action_features
            .chunks_exact(ACTION_FEATURE_DIM_V1)
            .enumerate()
        {
            let mut row_direct_squared = 0.0f64;
            let mut row_digest_squared = 0.0f64;
            for hidden in 0..HIDDEN_DIM_V1 {
                let weight_row = &weight
                    [hidden * ACTION_ENCODER_INPUT_V1..(hidden + 1) * ACTION_ENCODER_INPUT_V1];
                let mut direct = 0.0f32;
                for column in ACTION_EXPLICIT_BEGIN_V1..ACTION_EXPLICIT_END_V1 {
                    let product = input[column] * weight_row[column];
                    direct += product;
                }
                let mut digest = 0.0f32;
                for column in ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1 {
                    let product = input[column] * weight_row[column];
                    digest += product;
                }
                let direct = f64::from(direct);
                let digest = f64::from(digest);
                row_direct_squared += direct * direct;
                row_digest_squared += digest * digest;
            }
            direct_squared_sum += row_direct_squared;
            digest_squared_sum += row_digest_squared;
            contribution_count += HIDDEN_DIM_V1;
            per_action_row.push(RowContributionRmsV1 {
                decision_index,
                action_index,
                direct_contribution_squared_norm: row_direct_squared,
                digest_contribution_squared_norm: row_digest_squared,
                direct_contribution_rms: (row_direct_squared / HIDDEN_DIM_V1 as f64).sqrt(),
                digest_contribution_rms: (row_digest_squared / HIDDEN_DIM_V1 as f64).sqrt(),
            });
        }
    }
    FirstLayerContributionRmsV1 {
        source_condition: "repaired/FULL",
        tensor_name: "action_encoder.0.weight",
        accumulator: "exact-positive-zero-f32-forward-column-order-bias-excluded",
        hidden_dim: HIDDEN_DIM_V1,
        direct_contribution_rms: (direct_squared_sum / contribution_count as f64).sqrt(),
        digest_contribution_rms: (digest_squared_sum / contribution_count as f64).sqrt(),
        per_action_row,
    }
}

#[derive(Clone, Debug, Serialize)]
struct FunctionalEffectV1 {
    name: &'static str,
    output_sha256: String,
    multi_action_decision_count: usize,
    mean_jensen_shannon_nats: f64,
    mean_centered_logit_rms_delta: f64,
    top_action_flip_count: usize,
    top_action_flip_fraction: f64,
    value_bits_invariant: bool,
}

fn functional_effect_v1(
    role: &str,
    name: &'static str,
    baseline: &[DecisionScoreV1],
    actual: &[DecisionScoreV1],
) -> FunctionalEffectV1 {
    assert_eq!(baseline.len(), actual.len());
    let mut js_sum = 0.0f64;
    let mut centered_sum = 0.0f64;
    let mut multi_action_decision_count = 0usize;
    let mut top_action_flip_count = 0usize;
    let mut value_bits_invariant = true;
    for (before, after) in baseline.iter().zip(actual) {
        assert_eq!(before.logits.len(), after.logits.len());
        if before.logits.len() > 1 {
            js_sum += jensen_shannon_v1(&before.logits, &after.logits);
            centered_sum += centered_logit_rms_delta_v1(&before.logits, &after.logits);
            top_action_flip_count +=
                usize::from(top_index_v1(&before.logits) != top_index_v1(&after.logits));
            multi_action_decision_count += 1;
        }
        value_bits_invariant &= before.value.to_bits() == after.value.to_bits();
    }
    assert!(multi_action_decision_count > 0);
    FunctionalEffectV1 {
        name,
        output_sha256: score_stream_sha256_v1(role, name, actual),
        multi_action_decision_count,
        mean_jensen_shannon_nats: js_sum / multi_action_decision_count as f64,
        mean_centered_logit_rms_delta: centered_sum / multi_action_decision_count as f64,
        top_action_flip_count,
        top_action_flip_fraction: top_action_flip_count as f64 / multi_action_decision_count as f64,
        value_bits_invariant,
    }
}

#[derive(Clone, Debug, Serialize)]
struct DigestMinusDirectContrastV1 {
    mean_jensen_shannon_nats: f64,
    mean_centered_logit_rms_delta: f64,
    top_action_flip_fraction: f64,
}

fn digest_minus_direct_v1(
    direct: &FunctionalEffectV1,
    digest: &FunctionalEffectV1,
) -> DigestMinusDirectContrastV1 {
    DigestMinusDirectContrastV1 {
        mean_jensen_shannon_nats: digest.mean_jensen_shannon_nats - direct.mean_jensen_shannon_nats,
        mean_centered_logit_rms_delta: digest.mean_centered_logit_rms_delta
            - direct.mean_centered_logit_rms_delta,
        top_action_flip_fraction: digest.top_action_flip_fraction - direct.top_action_flip_fraction,
    }
}

fn descriptive_label_v1(role: ModelRoleV1, contrast: &DigestMinusDirectContrastV1) -> &'static str {
    let all_positive = contrast.mean_jensen_shannon_nats > 0.0
        && contrast.mean_centered_logit_rms_delta > 0.0
        && contrast.top_action_flip_fraction > 0.0;
    let all_negative = contrast.mean_jensen_shannon_nats < 0.0
        && contrast.mean_centered_logit_rms_delta < 0.0
        && contrast.top_action_flip_fraction < 0.0;
    match (role, all_positive, all_negative) {
        (ModelRoleV1::RawCommonSnapshot, true, _) => "RAW-INIT-DIGEST-DOMINANT",
        (ModelRoleV1::RawCommonSnapshot, _, true) => "RAW-INIT-DIRECT-DOMINANT",
        (ModelRoleV1::RawCommonSnapshot, _, _) => "RAW-INIT-MIXED",
        (_, true, _) => "IMPORTED-DIGEST-DOMINANT",
        (_, _, true) => "IMPORTED-DIRECT-DOMINANT",
        _ => "IMPORTED-MIXED",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ModelRoleV1 {
    RawCommonSnapshot,
    ImportedMirrorG0,
    ImportedDivergedG0,
}

impl ModelRoleV1 {
    const fn name(self) -> &'static str {
        match self {
            Self::RawCommonSnapshot => "raw-common-snapshot",
            Self::ImportedMirrorG0 => "imported-mirror-g0",
            Self::ImportedDivergedG0 => "imported-diverged-g0",
        }
    }

    const fn kind(self) -> &'static str {
        match self {
            Self::RawCommonSnapshot => "frozen-common-model-snapshot",
            Self::ImportedMirrorG0 | Self::ImportedDivergedG0 => {
                "validated-native-training-store-generation-zero"
            }
        }
    }

    fn parse_v1(value: &str) -> Option<Self> {
        match value {
            "raw-common-snapshot" => Some(Self::RawCommonSnapshot),
            "imported-mirror-g0" => Some(Self::ImportedMirrorG0),
            "imported-diverged-g0" => Some(Self::ImportedDivergedG0),
            _ => None,
        }
    }

    const fn expected_model_parameter_sha256(self) -> &'static str {
        match self {
            Self::RawCommonSnapshot => {
                "36157c71b9fd736d4913e6c5722dcb9c1e4f119b7b28b108bde9d74f18862d54"
            }
            Self::ImportedMirrorG0 => {
                "db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d"
            }
            Self::ImportedDivergedG0 => {
                "9c692503df20669686d4b5706cd5ed53989a60ca9dec3778c10312b3bddc722e"
            }
        }
    }

    const fn prior_baseline_output_sha256(self) -> Option<&'static str> {
        match self {
            Self::RawCommonSnapshot => None,
            Self::ImportedMirrorG0 => {
                Some("92d40cc1bd5ad4d54cb65cabb66b2788e4de16306727e6efc8c92f1b37e631da")
            }
            Self::ImportedDivergedG0 => {
                Some("39d5466625461fe9eb364436255a0dec0ba75d10c1d0fcdc27b6cc582a436dfc")
            }
        }
    }

    #[cfg(windows)]
    const fn expected_store_root(self) -> Option<&'static str> {
        match self {
            Self::RawCommonSnapshot => None,
            Self::ImportedMirrorG0 => {
                Some(r"D:\mtg-kernel-exploiter-v3b-20260726\runs-arm1\dev0\run-0\store")
            }
            Self::ImportedDivergedG0 => {
                Some(r"D:\mtg-kernel-exploiter-v3b-20260726\runs-arm2\dev0\run-0\store")
            }
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct StoreProvenanceV1 {
    run_sha256: String,
    identity_bundle_sha256: String,
    segment_ordinal: u64,
    segment_manifest_sha256: String,
    parent_boundary_head_sha256: Option<String>,
    boundary_head_sha256: String,
    boundary_head_record_sha256: String,
    checkpoint_manifest_sha256: String,
    checkpoint_payload_sha256: String,
    checkpoint_sidecar_sha256: String,
    logical_state_sha256: String,
    train_state_sha256: String,
    last_update_evidence_sha256: Option<String>,
    adam_step: u64,
}

fn store_provenance_v1(identity: CheckpointIdentityV1) -> StoreProvenanceV1 {
    StoreProvenanceV1 {
        run_sha256: identity.run_sha256,
        identity_bundle_sha256: identity.identity_bundle_sha256,
        segment_ordinal: identity.segment_ordinal,
        segment_manifest_sha256: identity.segment_manifest_sha256,
        parent_boundary_head_sha256: identity.parent_boundary_head_sha256,
        boundary_head_sha256: identity.boundary_head_sha256,
        boundary_head_record_sha256: identity.boundary_head_record_sha256,
        checkpoint_manifest_sha256: identity.checkpoint_manifest_sha256,
        checkpoint_payload_sha256: identity.checkpoint_payload_sha256,
        checkpoint_sidecar_sha256: identity.checkpoint_sidecar_sha256,
        logical_state_sha256: identity.logical_state_sha256,
        train_state_sha256: identity.train_state_sha256,
        last_update_evidence_sha256: identity.last_update_evidence_sha256,
        adam_step: identity.adam_step,
    }
}

struct ExpectedStoreProvenanceV1 {
    run_sha256: &'static str,
    identity_bundle_sha256: &'static str,
    segment_manifest_sha256: &'static str,
    boundary_head_sha256: &'static str,
    boundary_head_record_sha256: &'static str,
    checkpoint_manifest_sha256: &'static str,
    checkpoint_payload_sha256: &'static str,
    checkpoint_sidecar_sha256: &'static str,
    logical_state_sha256: &'static str,
    train_state_sha256: &'static str,
}

fn expected_store_provenance_v1(role: ModelRoleV1) -> ExpectedStoreProvenanceV1 {
    match role {
        ModelRoleV1::ImportedMirrorG0 => ExpectedStoreProvenanceV1 {
            run_sha256: "0b46f9507caede181e745da51dabbb6c9f73d72d3eb2315f089ef248c60e2f80",
            identity_bundle_sha256:
                "3b3e4e2270d307e7984314b91be69f1ccad0ec171d3210e3048a7ba2eb747024",
            segment_manifest_sha256:
                "54c1d3cc527bc339f55734a47c660b2f5078b291a9d2d4b0cdfd36eeeaa8ec5e",
            boundary_head_sha256:
                "659a9e4cd250cf1f38a678d3632d1ce6ae1fd6aa7d7bc02918bb6e0d4762cfd2",
            boundary_head_record_sha256:
                "0b45da9663aed2f56460c85693122dae267b9a7f782152023dcbe02f2fa3d64e",
            checkpoint_manifest_sha256:
                "fb780bfb8c5de8f88a9a1254108c7f45f7a90dba75f8ef614c8103681c7127a1",
            checkpoint_payload_sha256:
                "2a0840425ccfd09df56747d016d8fcd6b5bc19bba09b6f8cbcdc4507b7315095",
            checkpoint_sidecar_sha256:
                "a6a6c1934f388ff0e212bb15a5f43f7fd6a03dc9ec1dff91acfe762a4a72b62f",
            logical_state_sha256:
                "f46efcc86d9cc6ad2aec8bcc13e02560d1cd3bc3da166bb9a9e7054430dba18a",
            train_state_sha256: "0b35c448201efe92375f48a22201c432d3272a3286fae1440f6e7aa2277b9de5",
        },
        ModelRoleV1::ImportedDivergedG0 => ExpectedStoreProvenanceV1 {
            run_sha256: "fee86543272b4f709be46bb7f9eec820d979d264a93b606408e07c9a6871e51f",
            identity_bundle_sha256:
                "27c1c4798f8eb4a396e1952d055cb04122ce44d24fc8ff98118787ae0cb0985c",
            segment_manifest_sha256:
                "9957484508c494032526b91a3226c8b30e3e82d5a50e4070479b74e5fda4a5b5",
            boundary_head_sha256:
                "142fe85ace4c0b8e4d006b2d424c5f65604375eb0f64856395e87b783d648a13",
            boundary_head_record_sha256:
                "42360bb84f74a995be98f473181601baedd22ff238012fed4222d1790d11c456",
            checkpoint_manifest_sha256:
                "2503dc79396fd9cf22e2771324e13b246f686de89503e497680d50091a4fbd99",
            checkpoint_payload_sha256:
                "0d818f5803a96c7ae15c0a550cc9cec99bc50bf72a996697e2d0a1f09fd41145",
            checkpoint_sidecar_sha256:
                "3c6ef5aa5fb4358014a95870060cc1cb0d80b2f38ee5fde8660167968f666ad0",
            logical_state_sha256:
                "c05d303a31e300398ea40d3eca4b37b75a7cc832648fa0dda22920586a93e09b",
            train_state_sha256: "207f2b99499ec67fcca99b332b28614771be84088696bbd4983c2053b482bd2c",
        },
        ModelRoleV1::RawCommonSnapshot => panic!("raw snapshot has no Store provenance"),
    }
}

fn assert_store_provenance_v1(role: ModelRoleV1, actual: &StoreProvenanceV1) {
    let expected = expected_store_provenance_v1(role);
    assert_eq!(actual.run_sha256, expected.run_sha256);
    assert_eq!(
        actual.identity_bundle_sha256,
        expected.identity_bundle_sha256
    );
    assert_eq!(actual.segment_ordinal, 0);
    assert_eq!(
        actual.segment_manifest_sha256,
        expected.segment_manifest_sha256
    );
    assert_eq!(actual.parent_boundary_head_sha256, None);
    assert_eq!(actual.boundary_head_sha256, expected.boundary_head_sha256);
    assert_eq!(
        actual.boundary_head_record_sha256,
        expected.boundary_head_record_sha256
    );
    assert_eq!(
        actual.checkpoint_manifest_sha256,
        expected.checkpoint_manifest_sha256
    );
    assert_eq!(
        actual.checkpoint_payload_sha256,
        expected.checkpoint_payload_sha256
    );
    assert_eq!(
        actual.checkpoint_sidecar_sha256,
        expected.checkpoint_sidecar_sha256
    );
    assert_eq!(actual.logical_state_sha256, expected.logical_state_sha256);
    assert_eq!(actual.train_state_sha256, expected.train_state_sha256);
    assert_eq!(actual.last_update_evidence_sha256, None);
    assert_eq!(actual.adam_step, 0);
}

#[derive(Clone, Debug, Serialize)]
struct ModelIdentityReportV1 {
    role: &'static str,
    kind: &'static str,
    generation_index: u64,
    model_parameter_sha256: String,
    parameter_manifest_sha256: String,
    initialization_seed: Option<u64>,
    snapshot_identity: Option<&'static str>,
    snapshot_manifest_file_sha256: Option<String>,
    snapshot_payload_sha256: Option<String>,
    named_parameter_stream_sha256: Option<String>,
    provenance: Option<StoreProvenanceV1>,
    prior_baseline_output_digest_identity: Option<&'static str>,
    prior_baseline_output_sha256: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
struct CorpusAdmissionReportV1 {
    identity: &'static str,
    digest_identity: &'static str,
    sha256: String,
    expected_sha256: &'static str,
    decision_count: usize,
    episode_count: usize,
    multi_action_decision_count: usize,
    total_action_count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct TransformReportV1 {
    structured_repair_identity: &'static str,
    slot: usize,
    effect_boolean_rule: &'static str,
    attacker_inclusion_rule: &'static str,
    blocker_inclusion_rule: &'static str,
    digest_gate_identity: &'static str,
    scientific_gate_modes: [&'static str; 2],
    scaled_gate_scientific_read: bool,
}

#[derive(Clone, Debug, Serialize)]
struct GateReportV1 {
    full_copies_digest_without_multiplication: bool,
    zero_uses_exact_positive_zero: bool,
    zero_stress_mapping: &'static str,
    zero_stress_equals_ordinary_zero: bool,
    invalid_scale_bits_fail_closed: bool,
}

#[derive(Clone, Debug, Serialize)]
struct RuntimeAdmissionReportV1 {
    admitted: bool,
    corpus_digest_matches: bool,
    pre_transform_binding: PreTransformBindingReportV1,
    bitwise_comparison_identity: &'static str,
    exact_forward_capture_identity: &'static str,
    exact_forward_schema_version: &'static str,
    exact_forward_registry_version: &'static str,
    exact_forward_contract_digest: &'static str,
    exact_forward_encoding_digest: &'static str,
    exact_forward_hidden_dim: usize,
    exact_forward_schema_matches_frozen_contract: bool,
    exact_forward_capture_decision_count: usize,
    exact_forward_pooled_value_count: usize,
    canonical_semantics_pairwise_distinct: bool,
    repaired_zero_ingress_pairwise_distinct: bool,
    semantic_inclusion_pairs_complete_one_to_one: bool,
    semantic_inclusion_pair_direct_slot69_only: bool,
    semantic_inclusion_pair_pooled_refs_bit_exact: bool,
    repaired_zero_ingress_dim: usize,
    repaired_zero_ingress_row_count: usize,
    repaired_zero_ingress_sha256: String,
    repaired_zero_ingress_row_digest_identity: &'static str,
    repaired_zero_ingress_row_digests: Vec<IngressRowDigestV1>,
    attacker_false_true_pair_count: usize,
    blocker_false_true_pair_count: usize,
    attacker_pairs_witnessed: bool,
    blocker_pairs_witnessed: bool,
    non_action_tensors_bit_exact: bool,
    zero_stress_bit_exact: bool,
    zero_stress_tensors_bit_exact: bool,
    zero_stress_outputs_bit_exact: bool,
    every_action_only_intervention_value_bits_invariant: bool,
    model_parameters_bit_exact_before_after: bool,
}

#[derive(Clone, Debug, Serialize)]
struct EffectsReportV1 {
    direct_sibling: FunctionalEffectV1,
    digest_sibling: FunctionalEffectV1,
    repaired_full_vs_repaired_zero: FunctionalEffectV1,
    digest_minus_direct: DigestMinusDirectContrastV1,
    descriptive_label: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct OutputDigestsV1 {
    digest_identity: &'static str,
    baseline_frozen_full: String,
    repaired_full: String,
    repaired_zero: String,
    prior_baseline_reproduced_sha256: Option<String>,
    prior_baseline_exact_match: Option<bool>,
    repeated_baseline_frozen_full_bit_exact: bool,
    repeated_repaired_full_bit_exact: bool,
    repeated_repaired_zero_bit_exact: bool,
    zero_stress_equals_repaired_zero: bool,
    repair_only_value_bits_invariant: bool,
}

#[derive(Clone, Debug, Serialize)]
struct AdmissionPayloadV1 {
    schema: &'static str,
    label: &'static str,
    test_identity: &'static str,
    model: ModelIdentityReportV1,
    corpus: CorpusAdmissionReportV1,
    transform: TransformReportV1,
    gate: GateReportV1,
    admission: RuntimeAdmissionReportV1,
    input_statistics: InputStatisticsV1,
    first_layer_contribution_rms: FirstLayerContributionRmsV1,
    effects: EffectsReportV1,
    output_digests: OutputDigestsV1,
    nonclaims: Vec<&'static str>,
}

#[derive(Serialize)]
struct AdmissionEnvelopeV1 {
    schema: &'static str,
    payload_sha256: String,
    payload: AdmissionPayloadV1,
}

fn rotate_action_corpus_block_v1(
    source: &[NativeFlatDecisionTensorV2],
    begin: usize,
    end: usize,
) -> Vec<NativeFlatDecisionTensorV2> {
    let mut output = source.to_vec();
    for tensor in &mut output {
        rotate_action_block_in_place_v1(tensor, begin, end, 1);
    }
    output
}

fn score_values_bit_exact_v1(left: &[DecisionScoreV1], right: &[DecisionScoreV1]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.value.to_bits() == right.value.to_bits())
}

fn execute_model_admission_v1(
    role: ModelRoleV1,
    model: &NativePolicyValueNetV1,
    model_identity: ModelIdentityReportV1,
) -> AdmissionPayloadV1 {
    assert_eq!(model_identity.role, role.name());
    assert_eq!(model_identity.generation_index, 0);
    assert_eq!(
        model.parameter_manifest_sha256_v1(),
        role.expected_model_parameter_sha256()
    );
    assert_eq!(
        model_identity.model_parameter_sha256,
        role.expected_model_parameter_sha256()
    );
    assert_eq!(
        model_identity.parameter_manifest_sha256,
        role.expected_model_parameter_sha256()
    );
    let parameter_before = model.parameter_snapshot_v1();

    let corpus = build_admission_corpus_v1(CORPUS_DECISION_COUNT_V1);
    assert_eq!(
        corpus.sha256,
        "72103ea367a662f76675a044ad4efcf4c52bf86d32630df88e5247cf79f5e5e0"
    );
    assert_eq!(corpus.episode_count, 4);
    assert_eq!(corpus.multi_action_decision_count, 256);
    assert_eq!(corpus.total_action_count, 1_115);
    assert!(canonical_semantics_are_pairwise_distinct_v1(&corpus));

    let pre_transform_binding = validate_pre_transform_semantic_bindings_v1(&corpus);
    assert!(pre_transform_binding.all_rows_passed);
    assert_eq!(pre_transform_binding.row_count, 1_115);
    let repaired_full = transformed_corpus_v1(&corpus, DigestGateV1::Full);
    let repaired_zero = transformed_corpus_v1(&corpus, DigestGateV1::Zero);
    let zero_stress = rotate_digest_upstream_then_zero_v1(&corpus);
    let zero_stress_tensors_bit_exact =
        native_tensor_corpora_bit_exact_v1(&repaired_zero, &zero_stress);
    assert!(zero_stress_tensors_bit_exact);
    let direct_sibling = rotate_action_corpus_block_v1(
        &repaired_full,
        ACTION_EXPLICIT_BEGIN_V1,
        ACTION_EXPLICIT_END_V1,
    );
    let digest_sibling =
        rotate_action_corpus_block_v1(&repaired_full, ACTION_HASH_BEGIN_V1, ACTION_HASH_END_V1);
    let non_action_tensors_bit_exact = [
        repaired_full.as_slice(),
        repaired_zero.as_slice(),
        zero_stress.as_slice(),
        direct_sibling.as_slice(),
        digest_sibling.as_slice(),
    ]
    .into_iter()
    .all(|actual| non_action_tensor_corpora_bit_exact_v1(&corpus.tensors, actual));
    assert!(non_action_tensors_bit_exact);
    let (ingress_admission, _ingress, repaired_zero_scores) =
        ingress_admission_v1(model, &corpus, &repaired_zero);
    assert!(ingress_admission.canonical_semantics_pairwise_distinct);
    assert!(ingress_admission.repaired_zero_ingress_pairwise_distinct);
    assert!(ingress_admission.semantic_inclusion_pairs_complete_one_to_one);
    assert!(ingress_admission.semantic_inclusion_pair_direct_slot69_only);
    assert!(ingress_admission.semantic_inclusion_pair_pooled_refs_bit_exact);
    assert!(ingress_admission.exact_forward_schema_matches_frozen_contract);
    assert!(ingress_admission.attacker_false_true_pair_count > 0);
    assert!(ingress_admission.blocker_false_true_pair_count > 0);

    let baseline_scores = score_corpus_v1(model, &corpus.tensors);
    let repeated_baseline_scores = score_corpus_v1(model, &corpus.tensors);
    let repeated_baseline_bit_exact =
        decision_scores_bit_exact_v1(&baseline_scores, &repeated_baseline_scores);
    assert!(repeated_baseline_bit_exact);
    let prior_baseline_reproduced_sha256 = role
        .prior_baseline_output_sha256()
        .map(|_| score_stream_sha256_v1("g0", "baseline", &baseline_scores));
    let prior_baseline_exact_match = role
        .prior_baseline_output_sha256()
        .map(|expected| prior_baseline_reproduced_sha256.as_deref() == Some(expected));
    assert_ne!(
        prior_baseline_exact_match,
        Some(false),
        "Store g0 must reproduce its prior v2 frozen/full baseline stream"
    );
    let repaired_full_scores = score_corpus_v1(model, &repaired_full);
    let repeated_repaired_full_scores = score_corpus_v1(model, &repaired_full);
    let repeated_repaired_full_bit_exact =
        decision_scores_bit_exact_v1(&repaired_full_scores, &repeated_repaired_full_scores);
    assert!(repeated_repaired_full_bit_exact);
    let repeated_repaired_zero_scores = score_corpus_v1(model, &repaired_zero);
    let repeated_repaired_zero_bit_exact =
        decision_scores_bit_exact_v1(&repaired_zero_scores, &repeated_repaired_zero_scores);
    assert!(repeated_repaired_zero_bit_exact);
    let zero_stress_scores = score_corpus_v1(model, &zero_stress);
    let zero_stress_outputs_bit_exact =
        decision_scores_bit_exact_v1(&repaired_zero_scores, &zero_stress_scores);
    assert!(zero_stress_outputs_bit_exact);
    let direct_sibling_scores = score_corpus_v1(model, &direct_sibling);
    let digest_sibling_scores = score_corpus_v1(model, &digest_sibling);
    let direct_effect = functional_effect_v1(
        role.name(),
        "repaired_direct_sibling_rotation",
        &repaired_full_scores,
        &direct_sibling_scores,
    );
    let digest_effect = functional_effect_v1(
        role.name(),
        "repaired_digest_sibling_rotation",
        &repaired_full_scores,
        &digest_sibling_scores,
    );
    let full_vs_zero_effect = functional_effect_v1(
        role.name(),
        "repaired_full_vs_repaired_zero",
        &repaired_full_scores,
        &repaired_zero_scores,
    );
    let digest_minus_direct = digest_minus_direct_v1(&direct_effect, &digest_effect);
    let descriptive_label = descriptive_label_v1(role, &digest_minus_direct);
    let value_invariant = [
        &repaired_full_scores,
        &repaired_zero_scores,
        &zero_stress_scores,
        &direct_sibling_scores,
        &digest_sibling_scores,
    ]
    .into_iter()
    .all(|actual| score_values_bit_exact_v1(&baseline_scores, actual));
    assert!(
        value_invariant,
        "every action-only intervention must preserve exact value bits"
    );
    assert!(direct_effect.value_bits_invariant);
    assert!(digest_effect.value_bits_invariant);
    assert!(full_vs_zero_effect.value_bits_invariant);

    let parameter_after = model.parameter_snapshot_v1();
    let parameter_bits_exact =
        parameter_snapshots_bit_exact_v1(&parameter_before, &parameter_after);
    assert!(
        parameter_bits_exact,
        "diagnostic may not mutate model parameters"
    );

    let admitted = pre_transform_binding.all_rows_passed
        && ingress_admission.canonical_semantics_pairwise_distinct
        && ingress_admission.repaired_zero_ingress_pairwise_distinct
        && ingress_admission.semantic_inclusion_pairs_complete_one_to_one
        && ingress_admission.semantic_inclusion_pair_direct_slot69_only
        && ingress_admission.semantic_inclusion_pair_pooled_refs_bit_exact
        && ingress_admission.exact_forward_schema_matches_frozen_contract
        && ingress_admission.attacker_false_true_pair_count > 0
        && ingress_admission.blocker_false_true_pair_count > 0
        && non_action_tensors_bit_exact
        && zero_stress_tensors_bit_exact
        && zero_stress_outputs_bit_exact
        && repeated_baseline_bit_exact
        && repeated_repaired_full_bit_exact
        && repeated_repaired_zero_bit_exact
        && value_invariant
        && parameter_bits_exact;
    assert!(admitted);

    AdmissionPayloadV1 {
        schema: ADMISSION_PAYLOAD_SCHEMA_V2,
        label: "ACTION-INGRESS-ADMISSION-V2-DIAGNOSTIC-NON-EVIDENCE",
        test_identity: ADMISSION_TEST_IDENTITY_V2,
        model: model_identity,
        corpus: CorpusAdmissionReportV1 {
            identity: PROBE_CORPUS_IDENTITY_V1,
            digest_identity: PROBE_CORPUS_DIGEST_IDENTITY_V1,
            sha256: corpus.sha256,
            expected_sha256:
                "72103ea367a662f76675a044ad4efcf4c52bf86d32630df88e5247cf79f5e5e0",
            decision_count: corpus.tensors.len(),
            episode_count: corpus.episode_count,
            multi_action_decision_count: corpus.multi_action_decision_count,
            total_action_count: corpus.total_action_count,
        },
        transform: TransformReportV1 {
            structured_repair_identity: STRUCTURED_REPAIR_IDENTITY_V1,
            slot: SLOT69_V1,
            effect_boolean_rule: "retain-frozen-value-bit",
            attacker_inclusion_rule: "include-true-one-else-positive-zero",
            blocker_inclusion_rule: "include-true-one-else-positive-zero",
            digest_gate_identity: DIGEST_GATE_IDENTITY_V1,
            scientific_gate_modes: ["FULL", "ZERO"],
            scaled_gate_scientific_read: false,
        },
        gate: GateReportV1 {
            full_copies_digest_without_multiplication: true,
            zero_uses_exact_positive_zero: true,
            zero_stress_mapping:
                "within-decision-dst-j-receives-src-(j+1)-mod-n-upstream-then-ZERO",
            zero_stress_equals_ordinary_zero: zero_stress_tensors_bit_exact,
            invalid_scale_bits_fail_closed: true,
        },
        admission: RuntimeAdmissionReportV1 {
            admitted,
            corpus_digest_matches: true,
            pre_transform_binding,
            bitwise_comparison_identity: "ieee754-f32-to_bits-exact-v1",
            exact_forward_capture_identity: ingress_admission.exact_forward_capture_identity,
            exact_forward_schema_version: ingress_admission.exact_forward_schema_version,
            exact_forward_registry_version: ingress_admission.exact_forward_registry_version,
            exact_forward_contract_digest: ingress_admission.exact_forward_contract_digest,
            exact_forward_encoding_digest: ingress_admission.exact_forward_encoding_digest,
            exact_forward_hidden_dim: ingress_admission.exact_forward_hidden_dim,
            exact_forward_schema_matches_frozen_contract: ingress_admission
                .exact_forward_schema_matches_frozen_contract,
            exact_forward_capture_decision_count: ingress_admission
                .exact_forward_capture_decision_count,
            exact_forward_pooled_value_count: ingress_admission.exact_forward_pooled_value_count,
            canonical_semantics_pairwise_distinct: ingress_admission
                .canonical_semantics_pairwise_distinct,
            repaired_zero_ingress_pairwise_distinct: ingress_admission
                .repaired_zero_ingress_pairwise_distinct,
            semantic_inclusion_pairs_complete_one_to_one: ingress_admission
                .semantic_inclusion_pairs_complete_one_to_one,
            semantic_inclusion_pair_direct_slot69_only: ingress_admission
                .semantic_inclusion_pair_direct_slot69_only,
            semantic_inclusion_pair_pooled_refs_bit_exact: ingress_admission
                .semantic_inclusion_pair_pooled_refs_bit_exact,
            repaired_zero_ingress_dim: ingress_admission.repaired_zero_ingress_dim,
            repaired_zero_ingress_row_count: ingress_admission.repaired_zero_ingress_row_count,
            repaired_zero_ingress_sha256: ingress_admission.repaired_zero_ingress_sha256,
            repaired_zero_ingress_row_digest_identity: ingress_admission
                .repaired_zero_ingress_row_digest_identity,
            repaired_zero_ingress_row_digests: ingress_admission
                .repaired_zero_ingress_row_digests,
            attacker_false_true_pair_count: ingress_admission
                .attacker_false_true_pair_count,
            blocker_false_true_pair_count: ingress_admission.blocker_false_true_pair_count,
            attacker_pairs_witnessed: ingress_admission.attacker_false_true_pair_count > 0,
            blocker_pairs_witnessed: ingress_admission.blocker_false_true_pair_count > 0,
            non_action_tensors_bit_exact,
            zero_stress_bit_exact: zero_stress_tensors_bit_exact
                && zero_stress_outputs_bit_exact,
            zero_stress_tensors_bit_exact,
            zero_stress_outputs_bit_exact,
            every_action_only_intervention_value_bits_invariant: value_invariant,
            model_parameters_bit_exact_before_after: parameter_bits_exact,
        },
        input_statistics: input_statistics_v1(&repaired_full),
        first_layer_contribution_rms: first_layer_contribution_rms_v1(
            &parameter_before,
            &repaired_full,
        ),
        effects: EffectsReportV1 {
            direct_sibling: direct_effect,
            digest_sibling: digest_effect,
            repaired_full_vs_repaired_zero: full_vs_zero_effect,
            digest_minus_direct,
            descriptive_label,
        },
        output_digests: OutputDigestsV1 {
            digest_identity: PROBE_OUTPUT_DIGEST_IDENTITY_V1,
            baseline_frozen_full: score_stream_sha256_v1(
                role.name(),
                "baseline_frozen_full",
                &baseline_scores,
            ),
            repaired_full: score_stream_sha256_v1(
                role.name(),
                "repaired_full",
                &repaired_full_scores,
            ),
            repaired_zero: score_stream_sha256_v1(
                role.name(),
                "repaired_zero",
                &repaired_zero_scores,
            ),
            prior_baseline_reproduced_sha256,
            prior_baseline_exact_match,
            repeated_baseline_frozen_full_bit_exact: repeated_baseline_bit_exact,
            repeated_repaired_full_bit_exact,
            repeated_repaired_zero_bit_exact,
            zero_stress_equals_repaired_zero: zero_stress_outputs_bit_exact,
            repair_only_value_bits_invariant: score_values_bit_exact_v1(
                &baseline_scores,
                &repaired_full_scores,
            ),
        },
        nonclaims: vec![
            "This no-training screen cannot establish digest usefulness or harm.",
            "Initialization-time sensitivity is not learned memorization or causal attribution.",
            "Checked-corpus distinguishability is not a proof of sufficiency outside the checked corpora.",
            "This diagnostic cannot promote a model or support a pro-level-play claim.",
        ],
    }
}

fn emit_payload_v1(payload: AdmissionPayloadV1) {
    let payload_bytes =
        serde_json::to_vec(&payload).expect("serialize deterministic admission payload");
    let envelope = AdmissionEnvelopeV1 {
        schema: ADMISSION_SCHEMA_V2,
        payload_sha256: lower_hex_raw32_v1(sha256_v1(&payload_bytes)),
        payload,
    };
    let report =
        serde_json::to_string(&envelope).expect("serialize deterministic admission envelope");
    println!("{REPORT_MARKER_V2}{report}");
}

fn run_raw_common_snapshot_v1() {
    assert!(
        std::env::var_os(STORE_ROOT_ENV_V2).is_none(),
        "{STORE_ROOT_ENV_V2} must be absent for raw-common-snapshot"
    );
    let initial_model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .expect("construct private common-snapshot placeholder model");
    let mut state = NativePolicyValueTrainStateV1::new_v1(initial_model)
        .expect("construct private common-snapshot placeholder state");
    let (manifest_path, payload_path) = common_model_snapshot_paths_v1();
    let record = load_common_model_snapshot_v1(&manifest_path, &payload_path, &mut state)
        .expect("strict frozen common-snapshot load");
    assert_eq!(record.identity, SNAPSHOT_IDENTITY_V1);
    assert_eq!(record.model_init_seed, MODEL_INIT_SEED_V1);
    assert_eq!(
        record.manifest_file_sha256,
        "d5d296f5d4ee1f7e40a6005f1e1dd328b2885f6b95f0c6968c6bf1b87351c7cc"
    );
    assert_eq!(
        record.payload_sha256,
        "79f715b11ccce80ac66cc832bfdc0c963a8a20f27f7b492fdfbb433c008a90a5"
    );
    assert_eq!(
        record.named_parameter_stream_sha256,
        "36157c71b9fd736d4913e6c5722dcb9c1e4f119b7b28b108bde9d74f18862d54"
    );
    assert_eq!(
        record.loaded_named_parameter_stream_sha256,
        record.named_parameter_stream_sha256
    );
    assert_eq!(record.parameter_element_count, PARAMETER_COUNT_V1 as u64);
    assert_eq!(state.adam_step_v1(), 0);
    let model = state.model_v1();
    let parameter_manifest_sha256 = model.parameter_manifest_sha256_v1();
    assert_eq!(
        parameter_manifest_sha256,
        ModelRoleV1::RawCommonSnapshot.expected_model_parameter_sha256()
    );
    let identity = ModelIdentityReportV1 {
        role: ModelRoleV1::RawCommonSnapshot.name(),
        kind: ModelRoleV1::RawCommonSnapshot.kind(),
        generation_index: 0,
        model_parameter_sha256: parameter_manifest_sha256.clone(),
        parameter_manifest_sha256,
        initialization_seed: Some(record.model_init_seed),
        snapshot_identity: Some(SNAPSHOT_IDENTITY_V1),
        snapshot_manifest_file_sha256: Some(record.manifest_file_sha256),
        snapshot_payload_sha256: Some(record.payload_sha256),
        named_parameter_stream_sha256: Some(record.named_parameter_stream_sha256),
        provenance: None,
        prior_baseline_output_digest_identity: None,
        prior_baseline_output_sha256: None,
    };
    emit_payload_v1(execute_model_admission_v1(
        ModelRoleV1::RawCommonSnapshot,
        model,
        identity,
    ));
}

#[cfg(windows)]
fn run_imported_store_g0_v1(role: ModelRoleV1) {
    use crate::native_training_store_resume_v2::load_native_training_boundary_v2;
    use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
    use crate::native_training_store_run_v2::decode_train_run_v2;
    use std::fs;
    use std::path::PathBuf;

    assert!(matches!(
        role,
        ModelRoleV1::ImportedMirrorG0 | ModelRoleV1::ImportedDivergedG0
    ));
    let store_root = std::env::var_os(STORE_ROOT_ENV_V2)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("{STORE_ROOT_ENV_V2} must name the exact canonical Store"));
    assert_eq!(
        store_root,
        PathBuf::from(role.expected_store_root().unwrap()),
        "{STORE_ROOT_ENV_V2} must equal the predeclared canonical Store path"
    );
    let root =
        ValidatedNativeTrainingStoreRootV2::open_v2(store_root).expect("open exact Store root");
    let run_bytes = fs::read(root.root_path().join("run.json")).expect("read exact Store run.json");
    let run = decode_train_run_v2(&run_bytes).expect("decode exact Store run.json");
    assert!(run.record().contracts.wide_model_experiment_v1.is_none());
    assert_eq!(run.record().environment().deck_ids()[0], "Rally");
    assert_eq!(run.record().environment().deck_ids()[1], "Rally");

    let boundary =
        load_native_training_boundary_v2(&root, &run, 0).expect("load exact generation zero");
    assert_eq!(boundary.generation_index(), 0);
    let inference =
        load_native_checkpoint_inference_v1(&run, boundary.checkpoint(), boundary.payload())
            .expect("strict generation-zero inference load");
    assert_eq!(inference.generation_index(), 0);
    assert_eq!(
        lower_hex_raw32_v1(inference.model_parameter_sha256()),
        role.expected_model_parameter_sha256()
    );
    let snapshot = decoded_snapshot_v1(boundary.checkpoint(), boundary.payload());
    let provenance = store_provenance_v1(checkpoint_identity_v1(
        role.name(),
        boundary.boundary(),
        &inference,
        &snapshot,
    ));
    assert_store_provenance_v1(role, &provenance);
    let parameter_manifest_sha256 = inference.model.parameter_manifest_sha256_v1();
    assert_eq!(
        parameter_manifest_sha256,
        role.expected_model_parameter_sha256()
    );
    let identity = ModelIdentityReportV1 {
        role: role.name(),
        kind: role.kind(),
        generation_index: inference.generation_index(),
        model_parameter_sha256: lower_hex_raw32_v1(inference.model_parameter_sha256()),
        parameter_manifest_sha256,
        initialization_seed: None,
        snapshot_identity: None,
        snapshot_manifest_file_sha256: None,
        snapshot_payload_sha256: None,
        named_parameter_stream_sha256: None,
        provenance: Some(provenance),
        prior_baseline_output_digest_identity: Some(PROBE_OUTPUT_DIGEST_IDENTITY_V1),
        prior_baseline_output_sha256: role.prior_baseline_output_sha256(),
    };
    emit_payload_v1(execute_model_admission_v1(role, &inference.model, identity));
}

#[cfg(not(windows))]
fn run_imported_store_g0_v1(_role: ModelRoleV1) {
    panic!("imported Store authority reads are Windows-only");
}

/// CPU-only, no-training action-ingress admission screen.
///
/// Required environment:
///
/// - `ACTION_INGRESS_V2_MODEL_ROLE`: exactly `raw-common-snapshot`,
///   `imported-mirror-g0`, or `imported-diverged-g0`;
/// - `ACTION_INGRESS_V2_STORE_ROOT`: absent for the raw role and exactly the
///   predeclared canonical Store for either imported role.
///
/// The emitted JSON intentionally excludes timing.
#[test]
#[ignore = "official no-training diagnostic; run explicitly with --ignored --exact --nocapture"]
fn official_action_ingress_admission_probe_v2() {
    let role_value = std::env::var(MODEL_ROLE_ENV_V2)
        .unwrap_or_else(|_| panic!("{MODEL_ROLE_ENV_V2} must bind one exact model role"));
    let role = ModelRoleV1::parse_v1(&role_value)
        .unwrap_or_else(|| panic!("{MODEL_ROLE_ENV_V2} contains an unrecognized model role"));
    match role {
        ModelRoleV1::RawCommonSnapshot => run_raw_common_snapshot_v1(),
        ModelRoleV1::ImportedMirrorG0 | ModelRoleV1::ImportedDivergedG0 => {
            run_imported_store_g0_v1(role);
        }
    }
}
