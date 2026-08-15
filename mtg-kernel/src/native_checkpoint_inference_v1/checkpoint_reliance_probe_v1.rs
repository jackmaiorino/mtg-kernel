//! Test-only trained-checkpoint reliance diagnostic.
//!
//! This module deliberately sits below `#[cfg(test)]` in the checkpoint
//! inference module. It never exposes the private model, changes a checkpoint,
//! or adds a production inference path. The ignored probe loads two exact
//! Store authorities, tensorizes a checkpoint-independent Rally corpus, and
//! scores deterministic post-tensorization counterfactuals.

#![cfg_attr(not(windows), allow(dead_code))]

use super::*;
use crate::flat_policy_v2::{
    FlatCompletedDungeonV2, FlatContextPathElementV2, FlatDecisionEncoderV2,
    FlatEffectSubtypeChangeV2, FlatGlobalsV2, FlatObjectAbilityUseV2, FlatObjectCoreV2,
    FlatObjectGoadV2, FlatObjectSubtypeV2, FlatRelationV2, FlatScorerActionCoreV2,
    FlatScorerActionRefV2, FlatScoringDecisionViewV2, FlatScoringOwnedBuffersV2,
};
use crate::native_flat_tensorizer_v2::{
    NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2, NATIVE_FLAT_ACTION_FEATURE_DIM_V2,
    NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V2, NATIVE_FLAT_STATE_FEATURE_DIM_V2,
};
use crate::native_policy_train_step_v1::NativePolicyValueTrainSnapshotV1;
use crate::native_policy_value_net_v1::{
    NativeNamedParameterV1, ACTION_FEATURE_DIM_V1, HIDDEN_DIM_V1, STATE_DIM_V1,
};
use crate::native_training_store_boundary_v2::ValidatedNativeTrainingBoundaryV2;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use serde::Serialize;
use sha2::{Digest, Sha256};

const PROBE_SCHEMA_V1: &str = "mtg-kernel-checkpoint-reliance-probe/v1";
const PROBE_PAYLOAD_SCHEMA_V1: &str = "mtg-kernel-checkpoint-reliance-probe-payload/v1";
const PROBE_LABEL_V1: &str = "OBSERVATION-HASH-RELIANCE-DIAGNOSTIC-NON-EVIDENCE";
const PROBE_TEST_IDENTITY_V1: &str =
    "native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::trained_checkpoint_hash_vs_direct_reliance_probe_v1";
const PROBE_CORPUS_IDENTITY_V1: &str =
    "rally-mirror-splitmix64-modulo-fixed-256-post-tensorization-v1";
const PROBE_OUTPUT_DIGEST_IDENTITY_V1: &str =
    "sha256-framed-role-condition-decision-logit-value-f32le-v1";
const PROBE_CORPUS_DIGEST_IDENTITY_V1: &str = "sha256-framed-thirteen-native-flat-tensors-v1";

const STORE_ROOT_ENV_V1: &str = "OBS_RELIANCE_STORE_ROOT";
const GENERATION_ENV_V1: &str = "OBS_RELIANCE_CANDIDATE_GEN";
const EXPECTED_BASE_SEED_ENV_V1: &str = "OBS_RELIANCE_EXPECTED_BASE_SEED";

const CORPUS_DECISION_COUNT_V1: usize = 256;
const CORPUS_DECISIONS_PER_EPISODE_V1: usize = 64;
const CORPUS_STATE_DONOR_SHIFT_V1: usize = 129;
const CORPUS_BASE_EPISODE_ID_V1: u64 = 880_000;
const CORPUS_BASE_ENVIRONMENT_SEED_V1: u64 = 0x6d74_672d_6861_7368;
const CORPUS_MAX_EPISODES_V1: usize = 64;

const STATE_EXPLICIT_BEGIN_V1: usize = 0;
const STATE_EXPLICIT_END_V1: usize = 123;
const STATE_HASH_BEGIN_V1: usize = STATE_EXPLICIT_END_V1;
const STATE_HASH_END_V1: usize = 219;
const ACTION_EXPLICIT_BEGIN_V1: usize = 0;
const ACTION_EXPLICIT_END_V1: usize = 99;
const ACTION_HASH_BEGIN_V1: usize = ACTION_EXPLICIT_END_V1;
const ACTION_HASH_END_V1: usize = 195;
const STATE_ENCODER_INPUT_V1: usize = 1_499;
const ACTION_ENCODER_INPUT_V1: usize = 259;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InterventionV1 {
    StateHashPermutation,
    ActionHashPermutation,
    BothHashPermutation,
    StateDirectPermutation,
    ActionDirectPermutation,
    BothDirectPermutation,
    HashZeroAblation,
    DirectZeroAblation,
}

const INTERVENTIONS_V1: [InterventionV1; 8] = [
    InterventionV1::StateHashPermutation,
    InterventionV1::ActionHashPermutation,
    InterventionV1::BothHashPermutation,
    InterventionV1::StateDirectPermutation,
    InterventionV1::ActionDirectPermutation,
    InterventionV1::BothDirectPermutation,
    InterventionV1::HashZeroAblation,
    InterventionV1::DirectZeroAblation,
];

impl InterventionV1 {
    const fn name(self) -> &'static str {
        match self {
            Self::StateHashPermutation => "state_hash_permutation",
            Self::ActionHashPermutation => "action_hash_permutation",
            Self::BothHashPermutation => "both_hash_permutation",
            Self::StateDirectPermutation => "state_direct_permutation",
            Self::ActionDirectPermutation => "action_direct_permutation",
            Self::BothDirectPermutation => "both_direct_permutation",
            Self::HashZeroAblation => "hash_zero_ablation",
            Self::DirectZeroAblation => "direct_zero_ablation",
        }
    }

    const fn state_range(self) -> Option<(usize, usize)> {
        match self {
            Self::StateHashPermutation | Self::BothHashPermutation | Self::HashZeroAblation => {
                Some((STATE_HASH_BEGIN_V1, STATE_HASH_END_V1))
            }
            Self::StateDirectPermutation
            | Self::BothDirectPermutation
            | Self::DirectZeroAblation => Some((STATE_EXPLICIT_BEGIN_V1, STATE_EXPLICIT_END_V1)),
            Self::ActionHashPermutation | Self::ActionDirectPermutation => None,
        }
    }

    const fn action_range(self) -> Option<(usize, usize)> {
        match self {
            Self::ActionHashPermutation | Self::BothHashPermutation | Self::HashZeroAblation => {
                Some((ACTION_HASH_BEGIN_V1, ACTION_HASH_END_V1))
            }
            Self::ActionDirectPermutation
            | Self::BothDirectPermutation
            | Self::DirectZeroAblation => Some((ACTION_EXPLICIT_BEGIN_V1, ACTION_EXPLICIT_END_V1)),
            Self::StateHashPermutation | Self::StateDirectPermutation => None,
        }
    }

    const fn permutes_state(self) -> bool {
        matches!(
            self,
            Self::StateHashPermutation
                | Self::BothHashPermutation
                | Self::StateDirectPermutation
                | Self::BothDirectPermutation
        )
    }

    const fn permutes_actions(self) -> bool {
        matches!(
            self,
            Self::ActionHashPermutation
                | Self::BothHashPermutation
                | Self::ActionDirectPermutation
                | Self::BothDirectPermutation
        )
    }

    const fn is_zero_ablation(self) -> bool {
        matches!(self, Self::HashZeroAblation | Self::DirectZeroAblation)
    }
}

#[derive(Clone)]
struct OwnedScoringDecisionV1 {
    globals: FlatGlobalsV2,
    objects: Vec<FlatObjectCoreV2>,
    relations: Vec<FlatRelationV2>,
    object_subtypes: Vec<FlatObjectSubtypeV2>,
    ability_uses: Vec<FlatObjectAbilityUseV2>,
    goads: Vec<FlatObjectGoadV2>,
    completed_dungeons: Vec<FlatCompletedDungeonV2>,
    effect_subtype_changes: Vec<FlatEffectSubtypeChangeV2>,
    context_path_elements: Vec<FlatContextPathElementV2>,
    actions: Vec<FlatScorerActionCoreV2>,
    action_refs: Vec<FlatScorerActionRefV2>,
}

impl OwnedScoringDecisionV1 {
    fn from_session_v1(session: &FastActorSessionV1) -> Self {
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("fixed Rally corpus expected a live decision");
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
            .unwrap_or_else(|error| panic!("fixed Rally corpus encode failed: {error:?}"));
        owned.globals = encoded.globals;
        owned
    }

    fn view_v1(&self) -> FlatScoringDecisionViewV2<'_> {
        FlatScoringDecisionViewV2::new(
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
}

#[derive(Clone)]
struct FixedCorpusV1 {
    tensors: Vec<NativeFlatDecisionTensorV2>,
    episode_count: usize,
    multi_action_decision_count: usize,
    total_action_count: usize,
    sha256: String,
}

fn splitmix64_next_v1(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn fixed_environment_seed_v1(episode_ordinal: usize) -> u64 {
    CORPUS_BASE_ENVIRONMENT_SEED_V1
        ^ (episode_ordinal as u64)
            .wrapping_mul(0xd6e8_feb8_6659_fd93)
            .rotate_left(23)
}

fn build_fixed_rally_corpus_v1(decision_count: usize) -> FixedCorpusV1 {
    assert!(decision_count > 1);
    let mut tensors = Vec::with_capacity(decision_count);
    let mut tensorizer = NativeFlatTensorizerV2::new();
    let mut episode_ordinal = 0usize;

    while tensors.len() < decision_count {
        assert!(
            episode_ordinal < CORPUS_MAX_EPISODES_V1,
            "fixed Rally corpus did not reach its requested decision count"
        );
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
        .expect("fixed Rally corpus session reset");

        for _ in 0..CORPUS_DECISIONS_PER_EPISODE_V1 {
            if tensors.len() == decision_count {
                break;
            }
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                break;
            };
            let owned = OwnedScoringDecisionV1::from_session_v1(&session);
            let mut tensor = NativeFlatDecisionTensorV2::default();
            tensorizer
                .fill(owned.view_v1(), &mut tensor)
                .expect("fixed Rally corpus tensorization");
            assert_eq!(
                action_count_v1(&tensor),
                expected.legal_action_count as usize
            );
            tensors.push(tensor);

            let selected = (splitmix64_next_v1(&mut selection_state)
                % u64::from(expected.legal_action_count)) as u32;
            session
                .step(expected.episode_id, expected.step, selected)
                .expect("fixed Rally corpus modulo-index step");
        }
        episode_ordinal += 1;
    }

    let multi_action_decision_count = tensors
        .iter()
        .filter(|tensor| action_count_v1(tensor) > 1)
        .count();
    let total_action_count = tensors.iter().map(action_count_v1).sum();
    let sha256 = corpus_sha256_v1(&tensors);
    FixedCorpusV1 {
        tensors,
        episode_count: episode_ordinal,
        multi_action_decision_count,
        total_action_count,
        sha256,
    }
}

fn action_count_v1(tensor: &NativeFlatDecisionTensorV2) -> usize {
    assert!(!tensor.action_features.is_empty());
    assert!(tensor
        .action_features
        .len()
        .is_multiple_of(ACTION_FEATURE_DIM_V1));
    tensor.action_features.len() / ACTION_FEATURE_DIM_V1
}

fn hash_atom_v1(hasher: &mut Sha256, label: &[u8], bytes: &[u8]) {
    hasher.update((label.len() as u32).to_be_bytes());
    hasher.update(label);
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn hash_f32_slice_v1(hasher: &mut Sha256, label: &[u8], values: &[f32]) {
    hasher.update((label.len() as u32).to_be_bytes());
    hasher.update(label);
    hasher.update((values.len() as u64).to_be_bytes());
    for value in values {
        hasher.update(value.to_bits().to_le_bytes());
    }
}

fn hash_i64_slice_v1(hasher: &mut Sha256, label: &[u8], values: &[i64]) {
    hasher.update((label.len() as u32).to_be_bytes());
    hasher.update(label);
    hasher.update((values.len() as u64).to_be_bytes());
    for value in values {
        hasher.update(value.to_le_bytes());
    }
}

fn corpus_sha256_v1(tensors: &[NativeFlatDecisionTensorV2]) -> String {
    let mut hasher = Sha256::new();
    hash_atom_v1(
        &mut hasher,
        b"identity",
        PROBE_CORPUS_DIGEST_IDENTITY_V1.as_bytes(),
    );
    for (index, tensor) in tensors.iter().enumerate() {
        hash_atom_v1(
            &mut hasher,
            b"decision_index",
            &(index as u64).to_be_bytes(),
        );
        hash_f32_slice_v1(&mut hasher, b"state", &tensor.state);
        hash_f32_slice_v1(&mut hasher, b"object_features", &tensor.object_features);
        hash_i64_slice_v1(&mut hasher, b"object_card_ids", &tensor.object_card_ids);
        hash_i64_slice_v1(&mut hasher, b"object_groups", &tensor.object_groups);
        hash_i64_slice_v1(&mut hasher, b"object_node_ids", &tensor.object_node_ids);
        hash_f32_slice_v1(&mut hasher, b"edge_features", &tensor.edge_features);
        hash_i64_slice_v1(
            &mut hasher,
            b"edge_source_indices",
            &tensor.edge_source_indices,
        );
        hash_i64_slice_v1(
            &mut hasher,
            b"edge_target_indices",
            &tensor.edge_target_indices,
        );
        hash_f32_slice_v1(&mut hasher, b"action_features", &tensor.action_features);
        hash_f32_slice_v1(
            &mut hasher,
            b"action_ref_features",
            &tensor.action_ref_features,
        );
        hash_i64_slice_v1(
            &mut hasher,
            b"action_ref_card_ids",
            &tensor.action_ref_card_ids,
        );
        hash_i64_slice_v1(
            &mut hasher,
            b"action_ref_action_indices",
            &tensor.action_ref_action_indices,
        );
        hash_i64_slice_v1(
            &mut hasher,
            b"action_ref_node_indices",
            &tensor.action_ref_node_indices,
        );
    }
    lower_hex_raw32_v1(hasher.finalize().into())
}

fn permute_state_block_v1(
    source: &[NativeFlatDecisionTensorV2],
    begin: usize,
    end: usize,
    donor_shift: usize,
) -> Vec<NativeFlatDecisionTensorV2> {
    assert!(!source.is_empty());
    assert!(begin < end && end <= STATE_DIM_V1);
    let shift = donor_shift % source.len();
    let mut output = source.to_vec();
    for (index, target) in output.iter_mut().enumerate() {
        let donor = &source[(index + shift) % source.len()];
        target.state[begin..end].copy_from_slice(&donor.state[begin..end]);
    }
    output
}

fn rotate_action_block_in_place_v1(
    tensor: &mut NativeFlatDecisionTensorV2,
    begin: usize,
    end: usize,
    source_shift: usize,
) {
    assert!(begin < end && end <= ACTION_FEATURE_DIM_V1);
    let count = action_count_v1(tensor);
    if count <= 1 {
        return;
    }
    let shift = source_shift % count;
    let original = tensor.action_features.clone();
    for target_row in 0..count {
        let source_row = (target_row + shift) % count;
        let target_begin = target_row * ACTION_FEATURE_DIM_V1 + begin;
        let source_begin = source_row * ACTION_FEATURE_DIM_V1 + begin;
        tensor.action_features[target_begin..target_begin + (end - begin)]
            .copy_from_slice(&original[source_begin..source_begin + (end - begin)]);
    }
}

fn intervention_corpus_v1(
    baseline: &[NativeFlatDecisionTensorV2],
    intervention: InterventionV1,
) -> Vec<NativeFlatDecisionTensorV2> {
    assert_eq!(baseline.len(), CORPUS_DECISION_COUNT_V1);
    let mut output = if intervention.permutes_state() {
        let (begin, end) = intervention
            .state_range()
            .expect("state permutation has a state range");
        permute_state_block_v1(baseline, begin, end, CORPUS_STATE_DONOR_SHIFT_V1)
    } else {
        baseline.to_vec()
    };

    if intervention.permutes_actions() {
        let (begin, end) = intervention
            .action_range()
            .expect("action permutation has an action range");
        for tensor in &mut output {
            rotate_action_block_in_place_v1(tensor, begin, end, 1);
        }
    }
    if intervention.is_zero_ablation() {
        let (state_begin, state_end) = intervention
            .state_range()
            .expect("zero ablation has a state range");
        let (action_begin, action_end) = intervention
            .action_range()
            .expect("zero ablation has an action range");
        for tensor in &mut output {
            tensor.state[state_begin..state_end].fill(0.0);
            for row in tensor
                .action_features
                .chunks_exact_mut(ACTION_FEATURE_DIM_V1)
            {
                row[action_begin..action_end].fill(0.0);
            }
        }
    }
    output
}

fn assert_unchanged_except_v1(
    baseline: &[f32],
    actual: &[f32],
    changed_range: Option<(usize, usize)>,
) {
    assert_eq!(baseline.len(), actual.len());
    match changed_range {
        Some((begin, end)) => {
            assert_eq!(
                baseline[..begin]
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                actual[..begin]
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                baseline[end..]
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                actual[end..]
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>()
            );
        }
        None => assert_eq!(
            baseline
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            actual
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        ),
    }
}

fn assert_action_rows_unchanged_except_v1(
    baseline: &[f32],
    actual: &[f32],
    changed_range: Option<(usize, usize)>,
) {
    assert_eq!(baseline.len(), actual.len());
    for (baseline_row, actual_row) in baseline
        .chunks_exact(ACTION_FEATURE_DIM_V1)
        .zip(actual.chunks_exact(ACTION_FEATURE_DIM_V1))
    {
        assert_unchanged_except_v1(baseline_row, actual_row, changed_range);
    }
}

fn state_block_multiset_v1(
    tensors: &[NativeFlatDecisionTensorV2],
    begin: usize,
    end: usize,
) -> Vec<Vec<u32>> {
    let mut blocks = tensors
        .iter()
        .map(|tensor| {
            tensor.state[begin..end]
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    blocks.sort_unstable();
    blocks
}

fn action_block_multiset_v1(
    tensors: &[NativeFlatDecisionTensorV2],
    begin: usize,
    end: usize,
) -> Vec<Vec<u32>> {
    let mut blocks = tensors
        .iter()
        .flat_map(|tensor| tensor.action_features.chunks_exact(ACTION_FEATURE_DIM_V1))
        .map(|row| {
            row[begin..end]
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    blocks.sort_unstable();
    blocks
}

fn assert_intervention_integrity_v1(
    baseline: &[NativeFlatDecisionTensorV2],
    actual: &[NativeFlatDecisionTensorV2],
    intervention: InterventionV1,
) {
    assert_eq!(baseline.len(), actual.len());
    for (before, after) in baseline.iter().zip(actual) {
        assert_eq!(before.object_features, after.object_features);
        assert_eq!(before.object_card_ids, after.object_card_ids);
        assert_eq!(before.object_groups, after.object_groups);
        assert_eq!(before.object_node_ids, after.object_node_ids);
        assert_eq!(before.edge_features, after.edge_features);
        assert_eq!(before.edge_source_indices, after.edge_source_indices);
        assert_eq!(before.edge_target_indices, after.edge_target_indices);
        assert_eq!(before.action_ref_features, after.action_ref_features);
        assert_eq!(before.action_ref_card_ids, after.action_ref_card_ids);
        assert_eq!(
            before.action_ref_action_indices,
            after.action_ref_action_indices
        );
        assert_eq!(
            before.action_ref_node_indices,
            after.action_ref_node_indices
        );
        assert_unchanged_except_v1(&before.state, &after.state, intervention.state_range());
        assert_action_rows_unchanged_except_v1(
            &before.action_features,
            &after.action_features,
            intervention.action_range(),
        );
    }

    if intervention.permutes_state() {
        let (begin, end) = intervention.state_range().unwrap();
        assert_eq!(
            state_block_multiset_v1(baseline, begin, end),
            state_block_multiset_v1(actual, begin, end)
        );
    }
    if intervention.permutes_actions() {
        let (begin, end) = intervention.action_range().unwrap();
        assert_eq!(
            action_block_multiset_v1(baseline, begin, end),
            action_block_multiset_v1(actual, begin, end)
        );
    }
    if matches!(
        intervention,
        InterventionV1::StateHashPermutation | InterventionV1::BothHashPermutation
    ) {
        for (before, after) in baseline.iter().zip(actual) {
            assert_ne!(
                before.state[STATE_HASH_BEGIN_V1..STATE_HASH_END_V1]
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                after.state[STATE_HASH_BEGIN_V1..STATE_HASH_END_V1]
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                "state-hash donor must be a genuine derangement"
            );
        }
    }
    if intervention.is_zero_ablation() {
        let (state_begin, state_end) = intervention.state_range().unwrap();
        let (action_begin, action_end) = intervention.action_range().unwrap();
        for tensor in actual {
            assert!(tensor.state[state_begin..state_end]
                .iter()
                .all(|value| value.to_bits() == 0));
            assert!(tensor
                .action_features
                .chunks_exact(ACTION_FEATURE_DIM_V1)
                .all(|row| row[action_begin..action_end]
                    .iter()
                    .all(|value| value.to_bits() == 0)));
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct DecisionScoreV1 {
    logits: Vec<f32>,
    value: f32,
}

fn score_corpus_v1(
    model: &NativePolicyValueNetV1,
    tensors: &[NativeFlatDecisionTensorV2],
) -> Vec<DecisionScoreV1> {
    tensors
        .iter()
        .map(|tensor| {
            let output = model
                .forward_v1(encoded_decision_view_v1(tensor))
                .expect("counterfactual tensor must remain a valid Net8 input");
            DecisionScoreV1 {
                logits: output.logits,
                value: output.value,
            }
        })
        .collect()
}

fn score_stream_sha256_v1(role: &str, condition: &str, scores: &[DecisionScoreV1]) -> String {
    let mut hasher = Sha256::new();
    hash_atom_v1(
        &mut hasher,
        b"identity",
        PROBE_OUTPUT_DIGEST_IDENTITY_V1.as_bytes(),
    );
    hash_atom_v1(&mut hasher, b"role", role.as_bytes());
    hash_atom_v1(&mut hasher, b"condition", condition.as_bytes());
    for (index, score) in scores.iter().enumerate() {
        hash_atom_v1(
            &mut hasher,
            b"decision_index",
            &(index as u64).to_be_bytes(),
        );
        hash_f32_slice_v1(&mut hasher, b"logits", &score.logits);
        hash_f32_slice_v1(&mut hasher, b"value", &[score.value]);
    }
    lower_hex_raw32_v1(hasher.finalize().into())
}

fn append_score_stream_v1(
    hasher: &mut Sha256,
    role: &str,
    condition: &str,
    scores: &[DecisionScoreV1],
) {
    hash_atom_v1(hasher, b"role", role.as_bytes());
    hash_atom_v1(hasher, b"condition", condition.as_bytes());
    for (index, score) in scores.iter().enumerate() {
        hash_atom_v1(hasher, b"decision_index", &(index as u64).to_be_bytes());
        hash_f32_slice_v1(hasher, b"logits", &score.logits);
        hash_f32_slice_v1(hasher, b"value", &[score.value]);
    }
}

fn stable_softmax_v1(logits: &[f32]) -> Vec<f64> {
    assert!(!logits.is_empty());
    assert!(logits.iter().all(|value| value.is_finite()));
    let maximum = logits
        .iter()
        .copied()
        .map(f64::from)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut probabilities = logits
        .iter()
        .map(|value| (f64::from(*value) - maximum).exp())
        .collect::<Vec<_>>();
    let sum: f64 = probabilities.iter().sum();
    assert!(sum.is_finite() && sum > 0.0);
    for probability in &mut probabilities {
        *probability /= sum;
    }
    probabilities
}

fn jensen_shannon_v1(baseline: &[f32], actual: &[f32]) -> f64 {
    assert_eq!(baseline.len(), actual.len());
    let p = stable_softmax_v1(baseline);
    let q = stable_softmax_v1(actual);
    let mut divergence = 0.0;
    for (left, right) in p.into_iter().zip(q) {
        let middle = (left + right) * 0.5;
        if left > 0.0 {
            divergence += 0.5 * left * (left / middle).ln();
        }
        if right > 0.0 {
            divergence += 0.5 * right * (right / middle).ln();
        }
    }
    assert!(divergence.is_finite());
    divergence.max(0.0)
}

fn centered_logit_rms_delta_v1(baseline: &[f32], actual: &[f32]) -> f64 {
    assert_eq!(baseline.len(), actual.len());
    assert!(!baseline.is_empty());
    assert!(baseline.iter().chain(actual).all(|value| value.is_finite()));
    let baseline_mean =
        baseline.iter().copied().map(f64::from).sum::<f64>() / baseline.len() as f64;
    let actual_mean = actual.iter().copied().map(f64::from).sum::<f64>() / actual.len() as f64;
    let squared_sum = baseline
        .iter()
        .zip(actual)
        .map(|(before, after)| {
            let delta = (f64::from(*after) - actual_mean) - (f64::from(*before) - baseline_mean);
            delta * delta
        })
        .sum::<f64>();
    (squared_sum / baseline.len() as f64).sqrt()
}

fn top_index_v1(logits: &[f32]) -> usize {
    assert!(!logits.is_empty());
    assert!(logits.iter().all(|value| value.is_finite()));
    let mut top = 0usize;
    for index in 1..logits.len() {
        if logits[index] > logits[top] {
            top = index;
        }
    }
    top
}

fn strict_sign_flip_v1(before: f32, after: f32) -> bool {
    (before < 0.0 && after > 0.0) || (before > 0.0 && after < 0.0)
}

#[derive(Clone, Debug, Serialize)]
struct DistributionSummaryV1 {
    mean: f64,
    p50_nearest_rank: f64,
    p95_nearest_rank: f64,
    max: f64,
}

fn distribution_summary_v1(values: &[f64]) -> DistributionSummaryV1 {
    assert!(!values.is_empty());
    assert!(values.iter().all(|value| value.is_finite()));
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let mut ordered = values.to_vec();
    ordered.sort_by(f64::total_cmp);
    let nearest_rank = |fraction: f64| {
        let rank = (fraction * ordered.len() as f64).ceil() as usize;
        ordered[rank.max(1).min(ordered.len()) - 1]
    };
    DistributionSummaryV1 {
        mean,
        p50_nearest_rank: nearest_rank(0.50),
        p95_nearest_rank: nearest_rank(0.95),
        max: *ordered.last().unwrap(),
    }
}

#[derive(Clone, Debug, Serialize)]
struct EffectReportV1 {
    intervention: &'static str,
    intervention_output_sha256: String,
    policy_decision_count: usize,
    jensen_shannon_nats: DistributionSummaryV1,
    centered_logit_rms_delta: DistributionSummaryV1,
    top_action_flip_count: usize,
    top_action_flip_fraction: f64,
    baseline_top_probability_delta_baseline_minus_intervened: DistributionSummaryV1,
    value_decision_count: usize,
    baseline_value_exact_zero_count: usize,
    intervened_value_exact_zero_count: usize,
    value_zero_transition_count: usize,
    value_absolute_delta: DistributionSummaryV1,
    value_rmse: f64,
    value_sign_flip_count: usize,
    value_sign_flip_fraction: f64,
}

fn effect_report_v1(
    role: &str,
    intervention: InterventionV1,
    baseline: &[DecisionScoreV1],
    actual: &[DecisionScoreV1],
) -> EffectReportV1 {
    assert_eq!(baseline.len(), actual.len());
    let mut js = Vec::new();
    let mut centered = Vec::new();
    let mut top_probability_drop = Vec::new();
    let mut top_action_flip_count = 0usize;
    let mut value_absolute_delta = Vec::with_capacity(baseline.len());
    let mut value_squared_delta = 0.0;
    let mut value_sign_flip_count = 0usize;
    let mut baseline_value_exact_zero_count = 0usize;
    let mut intervened_value_exact_zero_count = 0usize;
    let mut value_zero_transition_count = 0usize;

    for (before, after) in baseline.iter().zip(actual) {
        assert_eq!(before.logits.len(), after.logits.len());
        if before.logits.len() > 1 {
            js.push(jensen_shannon_v1(&before.logits, &after.logits));
            centered.push(centered_logit_rms_delta_v1(&before.logits, &after.logits));
            let before_probabilities = stable_softmax_v1(&before.logits);
            let after_probabilities = stable_softmax_v1(&after.logits);
            let baseline_top = top_index_v1(&before.logits);
            top_probability_drop
                .push(before_probabilities[baseline_top] - after_probabilities[baseline_top]);
            if baseline_top != top_index_v1(&after.logits) {
                top_action_flip_count += 1;
            }
        }
        let delta = f64::from(after.value) - f64::from(before.value);
        value_absolute_delta.push(delta.abs());
        value_squared_delta += delta * delta;
        let before_is_zero = before.value == 0.0;
        let after_is_zero = after.value == 0.0;
        if before_is_zero {
            baseline_value_exact_zero_count += 1;
        }
        if after_is_zero {
            intervened_value_exact_zero_count += 1;
        }
        if before_is_zero != after_is_zero {
            value_zero_transition_count += 1;
        }
        if strict_sign_flip_v1(before.value, after.value) {
            value_sign_flip_count += 1;
        }
    }
    assert!(!js.is_empty(), "probe corpus needs multi-action decisions");
    let policy_decision_count = js.len();
    let value_decision_count = value_absolute_delta.len();
    EffectReportV1 {
        intervention: intervention.name(),
        intervention_output_sha256: score_stream_sha256_v1(role, intervention.name(), actual),
        policy_decision_count,
        jensen_shannon_nats: distribution_summary_v1(&js),
        centered_logit_rms_delta: distribution_summary_v1(&centered),
        top_action_flip_count,
        top_action_flip_fraction: top_action_flip_count as f64 / policy_decision_count as f64,
        baseline_top_probability_delta_baseline_minus_intervened: distribution_summary_v1(
            &top_probability_drop,
        ),
        value_decision_count,
        baseline_value_exact_zero_count,
        intervened_value_exact_zero_count,
        value_zero_transition_count,
        value_absolute_delta: distribution_summary_v1(&value_absolute_delta),
        value_rmse: (value_squared_delta / value_decision_count as f64).sqrt(),
        value_sign_flip_count,
        value_sign_flip_fraction: value_sign_flip_count as f64 / value_decision_count as f64,
    }
}

#[derive(Clone, Debug, Serialize)]
struct WithinModelContrastV1 {
    name: &'static str,
    hash_intervention: &'static str,
    direct_intervention: &'static str,
    mean_jensen_shannon_hash_minus_direct: f64,
    mean_centered_logit_rms_hash_minus_direct: f64,
    top_action_flip_fraction_hash_minus_direct: f64,
    mean_value_absolute_delta_hash_minus_direct: f64,
    value_rmse_hash_minus_direct: f64,
}

fn effect_by_name_v1<'a>(effects: &'a [EffectReportV1], name: &'static str) -> &'a EffectReportV1 {
    effects
        .iter()
        .find(|effect| effect.intervention == name)
        .unwrap_or_else(|| panic!("missing effect {name}"))
}

fn within_model_contrasts_v1(effects: &[EffectReportV1]) -> Vec<WithinModelContrastV1> {
    [
        (
            "state_hash_minus_direct",
            InterventionV1::StateHashPermutation,
            InterventionV1::StateDirectPermutation,
        ),
        (
            "action_hash_minus_direct",
            InterventionV1::ActionHashPermutation,
            InterventionV1::ActionDirectPermutation,
        ),
        (
            "both_hash_minus_direct",
            InterventionV1::BothHashPermutation,
            InterventionV1::BothDirectPermutation,
        ),
        (
            "zero_hash_minus_direct",
            InterventionV1::HashZeroAblation,
            InterventionV1::DirectZeroAblation,
        ),
    ]
    .into_iter()
    .map(|(name, hash, direct)| {
        let hash_effect = effect_by_name_v1(effects, hash.name());
        let direct_effect = effect_by_name_v1(effects, direct.name());
        WithinModelContrastV1 {
            name,
            hash_intervention: hash.name(),
            direct_intervention: direct.name(),
            mean_jensen_shannon_hash_minus_direct: hash_effect.jensen_shannon_nats.mean
                - direct_effect.jensen_shannon_nats.mean,
            mean_centered_logit_rms_hash_minus_direct: hash_effect.centered_logit_rms_delta.mean
                - direct_effect.centered_logit_rms_delta.mean,
            top_action_flip_fraction_hash_minus_direct: hash_effect.top_action_flip_fraction
                - direct_effect.top_action_flip_fraction,
            mean_value_absolute_delta_hash_minus_direct: hash_effect.value_absolute_delta.mean
                - direct_effect.value_absolute_delta.mean,
            value_rmse_hash_minus_direct: hash_effect.value_rmse - direct_effect.value_rmse,
        }
    })
    .collect()
}

#[derive(Clone, Debug, Serialize)]
struct ModelFunctionalReportV1 {
    role: &'static str,
    generation_index: u64,
    baseline_output_sha256: String,
    repeat_baseline_bit_exact: bool,
    effects: Vec<EffectReportV1>,
    hash_minus_direct_contrasts: Vec<WithinModelContrastV1>,
}

fn functional_report_v1(
    role: &'static str,
    inference: &NativeCheckpointInferenceV1,
    baseline_tensors: &[NativeFlatDecisionTensorV2],
    intervention_tensors: &[(InterventionV1, Vec<NativeFlatDecisionTensorV2>)],
    aggregate_output_hasher: &mut Sha256,
) -> ModelFunctionalReportV1 {
    let baseline = score_corpus_v1(&inference.model, baseline_tensors);
    let repeated_baseline = score_corpus_v1(&inference.model, baseline_tensors);
    assert_eq!(
        baseline, repeated_baseline,
        "same-runtime repeated baseline must be bit exact"
    );
    append_score_stream_v1(aggregate_output_hasher, role, "baseline", &baseline);
    let mut effects = Vec::with_capacity(intervention_tensors.len());
    for (intervention, tensors) in intervention_tensors {
        let actual = score_corpus_v1(&inference.model, tensors);
        append_score_stream_v1(aggregate_output_hasher, role, intervention.name(), &actual);
        effects.push(effect_report_v1(role, *intervention, &baseline, &actual));
    }

    for intervention in [
        InterventionV1::ActionHashPermutation,
        InterventionV1::ActionDirectPermutation,
    ] {
        let effect = effect_by_name_v1(&effects, intervention.name());
        assert_eq!(
            effect.value_absolute_delta.max, 0.0,
            "action-only intervention cannot reach Net8 value head"
        );
    }

    ModelFunctionalReportV1 {
        role,
        generation_index: inference.generation_index(),
        baseline_output_sha256: score_stream_sha256_v1(role, "baseline", &baseline),
        repeat_baseline_bit_exact: true,
        hash_minus_direct_contrasts: within_model_contrasts_v1(&effects),
        effects,
    }
}

fn replay_aggregate_output_stream_sha256_v1(
    g0: &NativeCheckpointInferenceV1,
    candidate: &NativeCheckpointInferenceV1,
    baseline_tensors: &[NativeFlatDecisionTensorV2],
    intervention_tensors: &[(InterventionV1, Vec<NativeFlatDecisionTensorV2>)],
) -> String {
    let mut hasher = Sha256::new();
    hash_atom_v1(
        &mut hasher,
        b"identity",
        PROBE_OUTPUT_DIGEST_IDENTITY_V1.as_bytes(),
    );
    for (role, inference) in [("g0", g0), ("candidate", candidate)] {
        let baseline = score_corpus_v1(&inference.model, baseline_tensors);
        append_score_stream_v1(&mut hasher, role, "baseline", &baseline);
        for (intervention, tensors) in intervention_tensors {
            let actual = score_corpus_v1(&inference.model, tensors);
            append_score_stream_v1(&mut hasher, role, intervention.name(), &actual);
        }
    }
    lower_hex_raw32_v1(hasher.finalize().into())
}

#[derive(Clone, Debug, Serialize)]
struct TrainingEffectContrastV1 {
    intervention: &'static str,
    candidate_minus_g0_mean_jensen_shannon: f64,
    candidate_minus_g0_mean_centered_logit_rms: f64,
    candidate_minus_g0_top_action_flip_fraction: f64,
    candidate_minus_g0_mean_value_absolute_delta: f64,
    candidate_minus_g0_value_rmse: f64,
}

fn training_effect_contrasts_v1(
    g0: &ModelFunctionalReportV1,
    candidate: &ModelFunctionalReportV1,
) -> Vec<TrainingEffectContrastV1> {
    INTERVENTIONS_V1
        .into_iter()
        .map(|intervention| {
            let before = effect_by_name_v1(&g0.effects, intervention.name());
            let after = effect_by_name_v1(&candidate.effects, intervention.name());
            TrainingEffectContrastV1 {
                intervention: intervention.name(),
                candidate_minus_g0_mean_jensen_shannon: after.jensen_shannon_nats.mean
                    - before.jensen_shannon_nats.mean,
                candidate_minus_g0_mean_centered_logit_rms: after.centered_logit_rms_delta.mean
                    - before.centered_logit_rms_delta.mean,
                candidate_minus_g0_top_action_flip_fraction: after.top_action_flip_fraction
                    - before.top_action_flip_fraction,
                candidate_minus_g0_mean_value_absolute_delta: after.value_absolute_delta.mean
                    - before.value_absolute_delta.mean,
                candidate_minus_g0_value_rmse: after.value_rmse - before.value_rmse,
            }
        })
        .collect()
}

#[derive(Clone, Copy)]
struct IngressGroupSpecV1 {
    name: &'static str,
    tensor_name: &'static str,
    input_dim: usize,
    column_begin: usize,
    column_end: usize,
}

const INGRESS_GROUPS_V1: [IngressGroupSpecV1; 4] = [
    IngressGroupSpecV1 {
        name: "state_direct",
        tensor_name: "state_encoder.0.weight",
        input_dim: STATE_ENCODER_INPUT_V1,
        column_begin: STATE_EXPLICIT_BEGIN_V1,
        column_end: STATE_EXPLICIT_END_V1,
    },
    IngressGroupSpecV1 {
        name: "state_hash",
        tensor_name: "state_encoder.0.weight",
        input_dim: STATE_ENCODER_INPUT_V1,
        column_begin: STATE_HASH_BEGIN_V1,
        column_end: STATE_HASH_END_V1,
    },
    IngressGroupSpecV1 {
        name: "action_direct",
        tensor_name: "action_encoder.0.weight",
        input_dim: ACTION_ENCODER_INPUT_V1,
        column_begin: ACTION_EXPLICIT_BEGIN_V1,
        column_end: ACTION_EXPLICIT_END_V1,
    },
    IngressGroupSpecV1 {
        name: "action_hash",
        tensor_name: "action_encoder.0.weight",
        input_dim: ACTION_ENCODER_INPUT_V1,
        column_begin: ACTION_HASH_BEGIN_V1,
        column_end: ACTION_HASH_END_V1,
    },
];

fn named_tensor_v1<'a>(
    tensors: &'a [NativeNamedParameterV1],
    name: &'static str,
) -> &'a NativeNamedParameterV1 {
    tensors
        .iter()
        .find(|tensor| tensor.name == name)
        .unwrap_or_else(|| panic!("missing named tensor {name}"))
}

fn ingress_values_v1(tensors: &[NativeNamedParameterV1], group: IngressGroupSpecV1) -> Vec<f32> {
    let tensor = named_tensor_v1(tensors, group.tensor_name);
    assert_eq!(tensor.shape, [HIDDEN_DIM_V1, group.input_dim]);
    let width = group.column_end - group.column_begin;
    let mut values = Vec::with_capacity(HIDDEN_DIM_V1 * width);
    for row in 0..HIDDEN_DIM_V1 {
        let begin = row * group.input_dim + group.column_begin;
        values.extend_from_slice(&tensor.values[begin..begin + width]);
    }
    values
}

fn f32le_sha256_v1(values: &[f32]) -> String {
    let mut hasher = Sha256::new();
    for value in values {
        hasher.update(value.to_bits().to_le_bytes());
    }
    lower_hex_raw32_v1(hasher.finalize().into())
}

#[derive(Clone, Debug, Serialize)]
struct TensorStatisticsV1 {
    element_count: usize,
    nonzero_count: usize,
    f32le_sha256: String,
    mean: f64,
    mean_absolute: f64,
    rms: f64,
    max_absolute: f64,
}

fn tensor_statistics_v1(values: &[f32]) -> TensorStatisticsV1 {
    assert!(!values.is_empty());
    assert!(values.iter().all(|value| value.is_finite()));
    let element_count = values.len();
    let nonzero_count = values
        .iter()
        .filter(|value| value.to_bits() & 0x7fff_ffff != 0)
        .count();
    let mean = values.iter().copied().map(f64::from).sum::<f64>() / element_count as f64;
    let mean_absolute = values
        .iter()
        .copied()
        .map(|value| f64::from(value).abs())
        .sum::<f64>()
        / element_count as f64;
    let rms = (values
        .iter()
        .copied()
        .map(|value| {
            let value = f64::from(value);
            value * value
        })
        .sum::<f64>()
        / element_count as f64)
        .sqrt();
    let max_absolute = values
        .iter()
        .copied()
        .map(|value| f64::from(value).abs())
        .fold(0.0, f64::max);
    TensorStatisticsV1 {
        element_count,
        nonzero_count,
        f32le_sha256: f32le_sha256_v1(values),
        mean,
        mean_absolute,
        rms,
        max_absolute,
    }
}

#[derive(Clone, Debug, Serialize)]
struct DeltaStatisticsV1 {
    element_count: usize,
    changed_bit_pattern_count: usize,
    mean: f64,
    mean_absolute: f64,
    rms: f64,
    max_absolute: f64,
}

fn delta_statistics_v1(g0: &[f32], candidate: &[f32]) -> DeltaStatisticsV1 {
    assert_eq!(g0.len(), candidate.len());
    assert!(!g0.is_empty());
    let deltas = g0
        .iter()
        .zip(candidate)
        .map(|(before, after)| f64::from(*after) - f64::from(*before))
        .collect::<Vec<_>>();
    let element_count = deltas.len();
    let changed_bit_pattern_count = g0
        .iter()
        .zip(candidate)
        .filter(|(before, after)| before.to_bits() != after.to_bits())
        .count();
    let mean = deltas.iter().sum::<f64>() / element_count as f64;
    let mean_absolute = deltas.iter().map(|value| value.abs()).sum::<f64>() / element_count as f64;
    let rms = (deltas.iter().map(|value| value * value).sum::<f64>() / element_count as f64).sqrt();
    let max_absolute = deltas.iter().map(|value| value.abs()).fold(0.0, f64::max);
    DeltaStatisticsV1 {
        element_count,
        changed_bit_pattern_count,
        mean,
        mean_absolute,
        rms,
        max_absolute,
    }
}

#[derive(Clone, Debug, Serialize)]
struct TensorSectionComparisonV1 {
    g0: TensorStatisticsV1,
    candidate: TensorStatisticsV1,
    candidate_minus_g0: DeltaStatisticsV1,
}

fn section_comparison_v1(g0: &[f32], candidate: &[f32]) -> TensorSectionComparisonV1 {
    TensorSectionComparisonV1 {
        g0: tensor_statistics_v1(g0),
        candidate: tensor_statistics_v1(candidate),
        candidate_minus_g0: delta_statistics_v1(g0, candidate),
    }
}

#[derive(Clone, Debug, Serialize)]
struct IngressGroupReportV1 {
    name: &'static str,
    tensor_name: &'static str,
    row_count: usize,
    input_dim: usize,
    column_begin_inclusive: usize,
    column_end_exclusive: usize,
    element_count: usize,
    weights: TensorSectionComparisonV1,
    adam_first_moments: TensorSectionComparisonV1,
    adam_second_moments: TensorSectionComparisonV1,
}

fn ingress_group_report_v1(
    group: IngressGroupSpecV1,
    g0: &NativePolicyValueTrainSnapshotV1,
    candidate: &NativePolicyValueTrainSnapshotV1,
) -> IngressGroupReportV1 {
    let g0_weights = ingress_values_v1(&g0.parameters, group);
    let candidate_weights = ingress_values_v1(&candidate.parameters, group);
    let g0_first = ingress_values_v1(&g0.first_moments, group);
    let candidate_first = ingress_values_v1(&candidate.first_moments, group);
    let g0_second = ingress_values_v1(&g0.second_moments, group);
    let candidate_second = ingress_values_v1(&candidate.second_moments, group);
    assert!(g0_second
        .iter()
        .chain(&candidate_second)
        .all(|value| *value >= 0.0));
    let element_count = HIDDEN_DIM_V1 * (group.column_end - group.column_begin);
    assert_eq!(g0_weights.len(), element_count);
    IngressGroupReportV1 {
        name: group.name,
        tensor_name: group.tensor_name,
        row_count: HIDDEN_DIM_V1,
        input_dim: group.input_dim,
        column_begin_inclusive: group.column_begin,
        column_end_exclusive: group.column_end,
        element_count,
        weights: section_comparison_v1(&g0_weights, &candidate_weights),
        adam_first_moments: section_comparison_v1(&g0_first, &candidate_first),
        adam_second_moments: section_comparison_v1(&g0_second, &candidate_second),
    }
}

fn finite_ratio_v1(numerator: f64, denominator: f64) -> Option<f64> {
    if denominator == 0.0 {
        None
    } else {
        let ratio = numerator / denominator;
        assert!(ratio.is_finite());
        Some(ratio)
    }
}

#[derive(Clone, Debug, Serialize)]
struct HashDirectIngressRatioV1 {
    pathway: &'static str,
    hash_group: &'static str,
    direct_group: &'static str,
    candidate_weight_rms_ratio: Option<f64>,
    candidate_minus_g0_weight_rms_ratio: Option<f64>,
    candidate_adam_first_moment_rms_ratio: Option<f64>,
    candidate_adam_second_moment_mean_ratio: Option<f64>,
    candidate_adam_second_moment_rms_ratio: Option<f64>,
}

fn ingress_group_by_name_v1<'a>(
    reports: &'a [IngressGroupReportV1],
    name: &'static str,
) -> &'a IngressGroupReportV1 {
    reports
        .iter()
        .find(|report| report.name == name)
        .unwrap_or_else(|| panic!("missing ingress report {name}"))
}

fn hash_direct_ingress_ratios_v1(
    reports: &[IngressGroupReportV1],
) -> Vec<HashDirectIngressRatioV1> {
    [
        ("state", "state_hash", "state_direct"),
        ("action", "action_hash", "action_direct"),
    ]
    .into_iter()
    .map(|(pathway, hash_name, direct_name)| {
        let hash = ingress_group_by_name_v1(reports, hash_name);
        let direct = ingress_group_by_name_v1(reports, direct_name);
        HashDirectIngressRatioV1 {
            pathway,
            hash_group: hash_name,
            direct_group: direct_name,
            candidate_weight_rms_ratio: finite_ratio_v1(
                hash.weights.candidate.rms,
                direct.weights.candidate.rms,
            ),
            candidate_minus_g0_weight_rms_ratio: finite_ratio_v1(
                hash.weights.candidate_minus_g0.rms,
                direct.weights.candidate_minus_g0.rms,
            ),
            candidate_adam_first_moment_rms_ratio: finite_ratio_v1(
                hash.adam_first_moments.candidate.rms,
                direct.adam_first_moments.candidate.rms,
            ),
            candidate_adam_second_moment_mean_ratio: finite_ratio_v1(
                hash.adam_second_moments.candidate.mean,
                direct.adam_second_moments.candidate.mean,
            ),
            candidate_adam_second_moment_rms_ratio: finite_ratio_v1(
                hash.adam_second_moments.candidate.rms,
                direct.adam_second_moments.candidate.rms,
            ),
        }
    })
    .collect()
}

fn decoded_snapshot_v1(
    checkpoint: &CheckpointManifestV3,
    payload: &[u8],
) -> NativePolicyValueTrainSnapshotV1 {
    let expected = expected_payload_digests_v1(checkpoint).unwrap();
    let decoded = decode_native_train_state_payload_verified_v1(
        payload,
        checkpoint.train_state().adam_step(),
        u32::try_from(checkpoint.train_state().scorer_bias_anchor_f32_bits()).unwrap(),
        &expected,
    )
    .expect("strict diagnostic train-state decode");
    assert_eq!(decoded.snapshot.adam_step, checkpoint.generation_index());
    assert_eq!(
        decoded.digests.model_parameter_sha256,
        checkpoint.model_parameter_sha256()
    );
    assert_eq!(
        decoded.digests.native_state_sha256,
        checkpoint.train_state_sha256()
    );
    decoded.snapshot
}

#[derive(Clone, Debug, Serialize)]
struct CheckpointIdentityV1 {
    role: &'static str,
    generation_index: u64,
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
    model_parameter_sha256: String,
    last_update_evidence_sha256: Option<String>,
    adam_step: u64,
}

fn checkpoint_identity_v1(
    role: &'static str,
    boundary: &ValidatedNativeTrainingBoundaryV2,
    inference: &NativeCheckpointInferenceV1,
    snapshot: &NativePolicyValueTrainSnapshotV1,
) -> CheckpointIdentityV1 {
    assert_eq!(snapshot.adam_step, inference.generation_index());
    let facts = boundary.boundary_facts_v2();
    assert_eq!(facts.generation_index, inference.generation_index());
    assert_eq!(facts.run_sha256, lower_hex_raw32_v1(inference.run_sha256()));
    assert_eq!(
        facts.checkpoint_manifest_sha256,
        inference.checkpoint_manifest_sha256()
    );
    assert_eq!(
        facts.checkpoint_payload_sha256,
        inference.checkpoint_payload_sha256()
    );
    assert_eq!(
        facts.model_parameter_sha256,
        inference.model_parameter_sha256()
    );
    assert_eq!(facts.train_state_sha256, inference.train_state_sha256());
    CheckpointIdentityV1 {
        role,
        generation_index: inference.generation_index(),
        run_sha256: lower_hex_raw32_v1(inference.run_sha256()),
        identity_bundle_sha256: facts.identity_bundle_sha256.to_owned(),
        segment_ordinal: facts.segment_ordinal,
        segment_manifest_sha256: lower_hex_raw32_v1(facts.segment_manifest_sha256),
        parent_boundary_head_sha256: facts.parent_head_sha256.map(lower_hex_raw32_v1),
        boundary_head_sha256: lower_hex_raw32_v1(facts.head_sha256),
        boundary_head_record_sha256: lower_hex_raw32_v1(facts.head_record_sha256),
        checkpoint_manifest_sha256: lower_hex_raw32_v1(inference.checkpoint_manifest_sha256()),
        checkpoint_payload_sha256: lower_hex_raw32_v1(inference.checkpoint_payload_sha256()),
        checkpoint_sidecar_sha256: lower_hex_raw32_v1(facts.checkpoint_sidecar_sha256),
        logical_state_sha256: lower_hex_raw32_v1(facts.logical_state_sha256),
        train_state_sha256: lower_hex_raw32_v1(inference.train_state_sha256()),
        model_parameter_sha256: lower_hex_raw32_v1(inference.model_parameter_sha256()),
        last_update_evidence_sha256: facts.last_update_evidence_sha256.map(lower_hex_raw32_v1),
        adam_step: snapshot.adam_step,
    }
}

#[derive(Clone, Debug, Serialize)]
struct FeaturePartitionV1 {
    state_feature_dim: usize,
    state_direct_range: [usize; 2],
    state_observation_hash_range: [usize; 2],
    action_feature_dim: usize,
    action_direct_range: [usize; 2],
    action_legal_hash_range: [usize; 2],
    hash_feature_dim_each: usize,
    state_encoder_first_weight_shape: [usize; 2],
    action_encoder_first_weight_shape: [usize; 2],
    structured_explicit_inputs_are_a_separate_bucket: bool,
}

#[derive(Clone, Debug, Serialize)]
struct CorpusReportV1 {
    identity: &'static str,
    digest_identity: &'static str,
    sha256: String,
    deck_ids: [&'static str; 2],
    decision_count: usize,
    episode_count: usize,
    decisions_per_episode_cap: usize,
    multi_action_decision_count: usize,
    total_action_count: usize,
    base_episode_id: u64,
    base_environment_seed: u64,
    action_selection: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct PermutationReportV1 {
    state_block_mapping: &'static str,
    state_donor_shift: usize,
    action_block_mapping: &'static str,
    forced_action_policy_metric_rule: &'static str,
    zero_ablation_value: &'static str,
    integrity_controls: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
struct ProbePayloadV1 {
    schema: &'static str,
    label: &'static str,
    test_identity: &'static str,
    run_base_seed: u64,
    model_architecture_version: &'static str,
    model_config_fingerprint: &'static str,
    feature_contract_digest: &'static str,
    feature_encoding_digest: &'static str,
    checkpoints: Vec<CheckpointIdentityV1>,
    feature_partition: FeaturePartitionV1,
    corpus: CorpusReportV1,
    permutation: PermutationReportV1,
    ingress_groups: Vec<IngressGroupReportV1>,
    hash_to_direct_ingress_ratios: Vec<HashDirectIngressRatioV1>,
    functional_models: Vec<ModelFunctionalReportV1>,
    candidate_minus_g0_functional_effects: Vec<TrainingEffectContrastV1>,
    aggregate_output_stream_sha256: String,
    repeat_aggregate_output_stream_bit_exact: bool,
    output_digest_identity: &'static str,
    nonclaims: Vec<&'static str>,
}

#[derive(Serialize)]
struct ProbeEnvelopeV1 {
    schema: &'static str,
    payload_sha256: String,
    payload: ProbePayloadV1,
}

fn synthetic_tensor_v1(marker: usize, action_count: usize) -> NativeFlatDecisionTensorV2 {
    assert!(action_count > 0);
    let mut tensor = NativeFlatDecisionTensorV2 {
        state: vec![0.0; STATE_DIM_V1],
        object_features: vec![0.0; 98],
        object_card_ids: vec![0],
        object_groups: vec![0],
        object_node_ids: vec![0],
        edge_features: Vec::new(),
        edge_source_indices: Vec::new(),
        edge_target_indices: Vec::new(),
        action_features: vec![0.0; action_count * ACTION_FEATURE_DIM_V1],
        action_ref_features: Vec::new(),
        action_ref_card_ids: Vec::new(),
        action_ref_action_indices: Vec::new(),
        action_ref_node_indices: Vec::new(),
    };
    for (index, value) in tensor.state.iter_mut().enumerate() {
        *value = (marker * STATE_DIM_V1 + index) as f32 / 10_000.0;
    }
    for row in 0..action_count {
        for column in 0..ACTION_FEATURE_DIM_V1 {
            tensor.action_features[row * ACTION_FEATURE_DIM_V1 + column] =
                (marker * 10_000 + row * ACTION_FEATURE_DIM_V1 + column) as f32 / 100_000.0;
        }
    }
    tensor
}

fn rotate_complete_actions_and_refs_v1(
    tensor: &mut NativeFlatDecisionTensorV2,
    source_shift: usize,
) {
    let count = action_count_v1(tensor);
    if count <= 1 {
        return;
    }
    let shift = source_shift % count;
    rotate_action_block_in_place_v1(tensor, 0, ACTION_FEATURE_DIM_V1, shift);
    for action_index in &mut tensor.action_ref_action_indices {
        let old = usize::try_from(*action_index).unwrap();
        *action_index = i64::try_from((old + count - shift) % count).unwrap();
    }
}

fn synthetic_tensor_with_action_refs_v1() -> NativeFlatDecisionTensorV2 {
    let mut tensor = synthetic_tensor_v1(7, 3);
    tensor.action_ref_features = vec![0.0; 3 * 25];
    for row in 0..3 {
        tensor.action_ref_features[row * 25 + row] = 1.0;
    }
    tensor.action_ref_card_ids = vec![0, 0, 0];
    tensor.action_ref_action_indices = vec![0, 1, 2];
    tensor.action_ref_node_indices = vec![0, 0, 0];
    tensor
}

#[test]
fn feature_partitions_and_ingress_groups_are_exact_v1() {
    assert_eq!(STATE_DIM_V1, 219);
    assert_eq!(STATE_DIM_V1, NATIVE_FLAT_STATE_FEATURE_DIM_V2);
    assert_eq!(STATE_HASH_END_V1 - STATE_HASH_BEGIN_V1, 96);
    assert_eq!(STATE_EXPLICIT_END_V1, STATE_HASH_BEGIN_V1);
    assert_eq!(ACTION_FEATURE_DIM_V1, 195);
    assert_eq!(ACTION_FEATURE_DIM_V1, NATIVE_FLAT_ACTION_FEATURE_DIM_V2);
    assert_eq!(
        ACTION_EXPLICIT_END_V1,
        NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2
    );
    assert_eq!(
        ACTION_HASH_END_V1 - ACTION_HASH_BEGIN_V1,
        NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V2
    );
    assert_eq!(ACTION_EXPLICIT_END_V1, ACTION_HASH_BEGIN_V1);
    assert_eq!(INGRESS_GROUPS_V1[0].column_end, 123);
    assert_eq!(INGRESS_GROUPS_V1[1].column_begin, 123);
    assert_eq!(INGRESS_GROUPS_V1[1].column_end, 219);
    assert_eq!(INGRESS_GROUPS_V1[2].column_end, 99);
    assert_eq!(INGRESS_GROUPS_V1[3].column_begin, 99);
    assert_eq!(INGRESS_GROUPS_V1[3].column_end, 195);
    assert_eq!(
        HIDDEN_DIM_V1 * (INGRESS_GROUPS_V1[1].column_end - INGRESS_GROUPS_V1[1].column_begin),
        6_144
    );
    assert_eq!(
        HIDDEN_DIM_V1 * (INGRESS_GROUPS_V1[3].column_end - INGRESS_GROUPS_V1[3].column_begin),
        6_144
    );
}

#[test]
fn permutation_and_ablation_transforms_are_scoped_and_reversible_v1() {
    let baseline = (0..CORPUS_DECISION_COUNT_V1)
        .map(|index| synthetic_tensor_v1(index, index % 4 + 1))
        .collect::<Vec<_>>();
    for intervention in INTERVENTIONS_V1 {
        let actual = intervention_corpus_v1(&baseline, intervention);
        assert_intervention_integrity_v1(&baseline, &actual, intervention);
    }

    for intervention in [
        InterventionV1::StateHashPermutation,
        InterventionV1::ActionHashPermutation,
        InterventionV1::BothHashPermutation,
        InterventionV1::StateDirectPermutation,
        InterventionV1::ActionDirectPermutation,
        InterventionV1::BothDirectPermutation,
    ] {
        let mut restored = intervention_corpus_v1(&baseline, intervention);
        if let Some((begin, end)) = intervention.state_range() {
            restored = permute_state_block_v1(
                &restored,
                begin,
                end,
                CORPUS_DECISION_COUNT_V1 - CORPUS_STATE_DONOR_SHIFT_V1,
            );
        }
        if let Some((begin, end)) = intervention.action_range() {
            for tensor in &mut restored {
                let count = action_count_v1(tensor);
                if count > 1 {
                    rotate_action_block_in_place_v1(tensor, begin, end, count - 1);
                }
            }
        }
        assert_eq!(
            corpus_sha256_v1(&restored),
            corpus_sha256_v1(&baseline),
            "permutation plus inverse must restore all tensor bits for {}",
            intervention.name()
        );
    }
}

#[test]
fn metric_primitives_are_gauge_invariant_and_fail_on_nonfinite_v1() {
    let baseline = [-1_000.0_f32, 0.0, 1_000.0];
    let shifted = [-993.0_f32, 7.0, 1_007.0];
    assert!(jensen_shannon_v1(&baseline, &shifted) <= f64::EPSILON);
    assert!(centered_logit_rms_delta_v1(&baseline, &shifted) <= f64::EPSILON);
    assert_eq!(top_index_v1(&[2.0, 2.0, 1.0]), 0);
    assert_eq!(stable_softmax_v1(&[0.0]), vec![1.0]);

    let before = [DecisionScoreV1 {
        logits: vec![0.0],
        value: -0.25,
    }];
    let after = [DecisionScoreV1 {
        logits: vec![9.0],
        value: 0.25,
    }];
    let report = effect_report_v1(
        "test",
        InterventionV1::StateHashPermutation,
        &[
            before[0].clone(),
            DecisionScoreV1 {
                logits: vec![0.0, 1.0],
                value: 0.0,
            },
        ],
        &[
            after[0].clone(),
            DecisionScoreV1 {
                logits: vec![1.0, 0.0],
                value: 0.5,
            },
        ],
    );
    assert_eq!(report.policy_decision_count, 1);
    assert_eq!(report.value_decision_count, 2);
    assert_eq!(report.value_sign_flip_count, 1);
    assert_eq!(report.baseline_value_exact_zero_count, 1);
    assert_eq!(report.intervened_value_exact_zero_count, 0);
    assert_eq!(report.value_zero_transition_count, 1);

    assert!(std::panic::catch_unwind(|| stable_softmax_v1(&[f32::NAN])).is_err());
}

#[test]
fn complete_action_rotation_is_logit_equivariant_and_value_invariant_v1() {
    let model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let baseline_tensor = synthetic_tensor_with_action_refs_v1();
    let baseline = model
        .forward_v1(encoded_decision_view_v1(&baseline_tensor))
        .unwrap();
    let mut rotated_tensor = baseline_tensor.clone();
    rotate_complete_actions_and_refs_v1(&mut rotated_tensor, 1);
    let rotated = model
        .forward_v1(encoded_decision_view_v1(&rotated_tensor))
        .unwrap();

    assert_eq!(baseline.value.to_bits(), rotated.value.to_bits());
    for target in 0..baseline.logits.len() {
        assert_eq!(
            rotated.logits[target].to_bits(),
            baseline.logits[(target + 1) % baseline.logits.len()].to_bits()
        );
    }
}

#[test]
fn fixed_rally_corpus_is_repeatable_without_external_artifacts_v1() {
    let first = build_fixed_rally_corpus_v1(CORPUS_DECISION_COUNT_V1);
    let second = build_fixed_rally_corpus_v1(CORPUS_DECISION_COUNT_V1);
    assert_eq!(first.tensors.len(), 256);
    // Re-baselined once per the owner ruling on record (collab CLAUDE #236,
    // 2026-08-14): build_fixed_rally_corpus_v1 is a live reproduction (built
    // twice in this test, both compared equal below), not a decode of
    // sealed evidence, so it moves with the accepted observation/catalog
    // changes. This value coincidentally matches the historical corpus
    // identity pinned in scripts/action_ingress_admission_v1/v2's sealed,
    // dated diagnostics (CORPUS_SHA256), which stay frozen forever on their
    // own terms and are untouched by this update; the two are independent.
    assert_eq!(
        first.sha256,
        "6685d907752db0e82b62b123ffb88142d2fb59adf40f6a354c8a61ab3bd81c41"
    );
    assert_eq!(first.episode_count, 4);
    assert_eq!(first.multi_action_decision_count, 256);
    assert_eq!(first.total_action_count, 1_115);
    assert_eq!(first.sha256, second.sha256);
    assert_eq!(first.episode_count, second.episode_count);
    assert_eq!(
        first
            .tensors
            .iter()
            .map(action_count_v1)
            .collect::<Vec<_>>(),
        second
            .tensors
            .iter()
            .map(action_count_v1)
            .collect::<Vec<_>>()
    );
}

/// CPU-only, read-only diagnostic over one exact native Store.
///
/// Required environment:
/// - `OBS_RELIANCE_STORE_ROOT`: exact Store root;
/// - `OBS_RELIANCE_CANDIDATE_GEN`: nonzero checkpoint generation.
/// - `OBS_RELIANCE_EXPECTED_BASE_SEED`: exact run schedule base seed.
///
/// The Store is independently walked for generation zero and the candidate.
/// The emitted JSON excludes timing, so identical source, artifacts, runtime,
/// and environment produce identical report bytes.
#[cfg(windows)]
#[test]
#[ignore = "external Store diagnostic; run explicitly with --ignored --exact --nocapture"]
fn trained_checkpoint_hash_vs_direct_reliance_probe_v1() {
    use crate::native_training_store_resume_v2::load_native_training_boundary_v2;
    use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
    use crate::native_training_store_run_v2::decode_train_run_v2;
    use std::fs;
    use std::path::PathBuf;
    use std::time::Instant;

    let total_started = Instant::now();
    let store_root = std::env::var_os(STORE_ROOT_ENV_V1)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("{STORE_ROOT_ENV_V1} must name an exact native Store root"));
    let generation: u64 = std::env::var(GENERATION_ENV_V1)
        .unwrap_or_else(|_| panic!("{GENERATION_ENV_V1} must name a nonzero generation"))
        .parse()
        .expect("diagnostic generation u64");
    assert!(generation > 0, "diagnostic candidate must be trained");
    let expected_base_seed: u64 = std::env::var(EXPECTED_BASE_SEED_ENV_V1)
        .unwrap_or_else(|_| {
            panic!("{EXPECTED_BASE_SEED_ENV_V1} must name the exact run schedule base seed")
        })
        .parse()
        .expect("diagnostic expected base seed u64");

    let root = ValidatedNativeTrainingStoreRootV2::open_v2(store_root)
        .expect("open exact native Store root");
    let run_bytes = fs::read(root.root_path().join("run.json")).expect("read exact Store run.json");
    let run = decode_train_run_v2(&run_bytes).expect("decode exact Store run.json");
    assert_eq!(
        run.record().schedule().base_seed(),
        expected_base_seed,
        "Store run base seed must equal the frozen pair identity"
    );
    assert!(
        run.record().contracts.wide_model_experiment_v1.is_none(),
        "reliance probe v1 accepts only the narrow architecture"
    );
    assert_eq!(run.record().environment().deck_ids()[0], "Rally");
    assert_eq!(run.record().environment().deck_ids()[1], "Rally");

    let authority_started = Instant::now();
    let g0_boundary =
        load_native_training_boundary_v2(&root, &run, 0).expect("load exact generation zero");
    let candidate_boundary = load_native_training_boundary_v2(&root, &run, generation)
        .expect("load exact candidate generation");
    assert_eq!(g0_boundary.generation_index(), 0);
    assert_eq!(candidate_boundary.generation_index(), generation);
    let g0_inference =
        load_native_checkpoint_inference_v1(&run, g0_boundary.checkpoint(), g0_boundary.payload())
            .expect("strict generation-zero inference load");
    let candidate_inference = load_native_checkpoint_inference_v1(
        &run,
        candidate_boundary.checkpoint(),
        candidate_boundary.payload(),
    )
    .expect("strict candidate inference load");
    assert_eq!(
        g0_inference.run_sha256(),
        candidate_inference.run_sha256(),
        "candidate and generation zero must bind the same run authority"
    );
    assert_ne!(
        g0_inference.model_parameter_sha256(),
        candidate_inference.model_parameter_sha256(),
        "trained candidate must differ from generation-zero parameters"
    );
    let g0_snapshot = decoded_snapshot_v1(g0_boundary.checkpoint(), g0_boundary.payload());
    let candidate_snapshot = decoded_snapshot_v1(
        candidate_boundary.checkpoint(),
        candidate_boundary.payload(),
    );
    let authority_elapsed = authority_started.elapsed();

    let corpus_started = Instant::now();
    let corpus = build_fixed_rally_corpus_v1(CORPUS_DECISION_COUNT_V1);
    assert_eq!(corpus.tensors.len(), CORPUS_DECISION_COUNT_V1);
    assert!(
        corpus.multi_action_decision_count >= 128,
        "fixed corpus must contain at least 128 multi-action decisions"
    );
    let intervention_tensors = INTERVENTIONS_V1
        .into_iter()
        .map(|intervention| {
            let tensors = intervention_corpus_v1(&corpus.tensors, intervention);
            assert_intervention_integrity_v1(&corpus.tensors, &tensors, intervention);
            (intervention, tensors)
        })
        .collect::<Vec<_>>();
    let corpus_elapsed = corpus_started.elapsed();

    let ingress_groups = INGRESS_GROUPS_V1
        .into_iter()
        .map(|group| ingress_group_report_v1(group, &g0_snapshot, &candidate_snapshot))
        .collect::<Vec<_>>();
    let hash_to_direct_ingress_ratios = hash_direct_ingress_ratios_v1(&ingress_groups);

    let scoring_started = Instant::now();
    let mut aggregate_output_hasher = Sha256::new();
    hash_atom_v1(
        &mut aggregate_output_hasher,
        b"identity",
        PROBE_OUTPUT_DIGEST_IDENTITY_V1.as_bytes(),
    );
    let g0_functional = functional_report_v1(
        "g0",
        &g0_inference,
        &corpus.tensors,
        &intervention_tensors,
        &mut aggregate_output_hasher,
    );
    let candidate_functional = functional_report_v1(
        "candidate",
        &candidate_inference,
        &corpus.tensors,
        &intervention_tensors,
        &mut aggregate_output_hasher,
    );
    let candidate_minus_g0_functional_effects =
        training_effect_contrasts_v1(&g0_functional, &candidate_functional);
    let aggregate_output_stream_sha256 =
        lower_hex_raw32_v1(aggregate_output_hasher.finalize().into());
    let repeated_aggregate_output_stream_sha256 = replay_aggregate_output_stream_sha256_v1(
        &g0_inference,
        &candidate_inference,
        &corpus.tensors,
        &intervention_tensors,
    );
    assert_eq!(
        repeated_aggregate_output_stream_sha256, aggregate_output_stream_sha256,
        "repeated full-condition scoring must preserve the raw output-stream digest"
    );
    let scoring_elapsed = scoring_started.elapsed();

    let payload = ProbePayloadV1 {
        schema: PROBE_PAYLOAD_SCHEMA_V1,
        label: PROBE_LABEL_V1,
        test_identity: PROBE_TEST_IDENTITY_V1,
        run_base_seed: expected_base_seed,
        model_architecture_version: MODEL_ARCHITECTURE_VERSION_V1,
        model_config_fingerprint: MODEL_CONFIG_FINGERPRINT_V1,
        feature_contract_digest: FEATURE_CONTRACT_DIGEST_V1,
        feature_encoding_digest: FEATURE_ENCODING_DIGEST_V1,
        checkpoints: vec![
            checkpoint_identity_v1(
                "g0",
                g0_boundary.boundary(),
                &g0_inference,
                &g0_snapshot,
            ),
            checkpoint_identity_v1(
                "candidate",
                candidate_boundary.boundary(),
                &candidate_inference,
                &candidate_snapshot,
            ),
        ],
        feature_partition: FeaturePartitionV1 {
            state_feature_dim: STATE_DIM_V1,
            state_direct_range: [STATE_EXPLICIT_BEGIN_V1, STATE_EXPLICIT_END_V1],
            state_observation_hash_range: [STATE_HASH_BEGIN_V1, STATE_HASH_END_V1],
            action_feature_dim: ACTION_FEATURE_DIM_V1,
            action_direct_range: [ACTION_EXPLICIT_BEGIN_V1, ACTION_EXPLICIT_END_V1],
            action_legal_hash_range: [ACTION_HASH_BEGIN_V1, ACTION_HASH_END_V1],
            hash_feature_dim_each: NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V2,
            state_encoder_first_weight_shape: [HIDDEN_DIM_V1, STATE_ENCODER_INPUT_V1],
            action_encoder_first_weight_shape: [HIDDEN_DIM_V1, ACTION_ENCODER_INPUT_V1],
            structured_explicit_inputs_are_a_separate_bucket: true,
        },
        corpus: CorpusReportV1 {
            identity: PROBE_CORPUS_IDENTITY_V1,
            digest_identity: PROBE_CORPUS_DIGEST_IDENTITY_V1,
            sha256: corpus.sha256,
            deck_ids: ["Rally", "Rally"],
            decision_count: corpus.tensors.len(),
            episode_count: corpus.episode_count,
            decisions_per_episode_cap: CORPUS_DECISIONS_PER_EPISODE_V1,
            multi_action_decision_count: corpus.multi_action_decision_count,
            total_action_count: corpus.total_action_count,
            base_episode_id: CORPUS_BASE_EPISODE_ID_V1,
            base_environment_seed: CORPUS_BASE_ENVIRONMENT_SEED_V1,
            action_selection: "splitmix64-next-modulo-legal-action-count-v1",
        },
        permutation: PermutationReportV1 {
            state_block_mapping: "target-i-receives-source-(i+129)-mod-256",
            state_donor_shift: CORPUS_STATE_DONOR_SHIFT_V1,
            action_block_mapping: "within-decision-target-row-i-receives-source-(i+1)-mod-A",
            forced_action_policy_metric_rule: "A=1 excluded from policy metrics; included in value metrics",
            zero_ablation_value: "positive-zero-f32",
            integrity_controls: vec![
                "all non-target tensor fields bit-exact",
                "permuted block-bit multisets preserved exactly",
                "state hash donor differs for every decision",
                "permutation plus inverse restores exact corpus digest",
                "whole-action permutation rotates logits bit-exact and preserves value",
                "same-runtime repeated full-condition output stream is bit-exact",
            ],
        },
        ingress_groups,
        hash_to_direct_ingress_ratios,
        functional_models: vec![g0_functional, candidate_functional],
        candidate_minus_g0_functional_effects,
        aggregate_output_stream_sha256,
        repeat_aggregate_output_stream_bit_exact: true,
        output_digest_identity: PROBE_OUTPUT_DIGEST_IDENTITY_V1,
        nonclaims: vec![
            "Weight and Adam-moment summaries are structural diagnostics, not functional attribution.",
            "Permutation effects measure checkpoint sensitivity to feature association, not causal semantic importance.",
            "Direct controls cover sibling raw state/action blocks; object, card, edge, group, and action-ref paths are a separate structured-explicit bucket.",
            "A large opaque-hash effect is a memorization/reliance warning, not proof of leakage, collision, or generalization failure.",
            "This diagnostic cannot promote a checkpoint or support a pro-level-play claim.",
            "CPU tanh results are repeatable only under the pinned runtime; no cross-libm bit-parity claim is made.",
        ],
    };
    let payload_bytes =
        serde_json::to_vec(&payload).expect("serialize deterministic diagnostic payload");
    let payload_sha256 = lower_hex_raw32_v1(sha256_v1(&payload_bytes));
    let envelope = ProbeEnvelopeV1 {
        schema: PROBE_SCHEMA_V1,
        payload_sha256,
        payload,
    };
    let report =
        serde_json::to_string(&envelope).expect("serialize deterministic diagnostic envelope");
    println!("OBS_RELIANCE_JSON={report}");
    println!(
        "OBS_RELIANCE_TIMING authority_ms={} corpus_ms={} scoring_ms={} total_ms={}",
        authority_elapsed.as_millis(),
        corpus_elapsed.as_millis(),
        scoring_elapsed.as_millis(),
        total_started.elapsed().as_millis()
    );
}

// Carries the TreatmentAwareScorerV1/ReceiptRetainingObserverV1 diagnostic
// scaffolding for the formal unit-tape gradient screen, kept in place for
// not-yet-wired follow-on work rather than deleted; allowed at module
// scope instead of item-by-item.
#[allow(dead_code)]
mod action_block_gradient_diagnostic_v1;
mod action_ingress_admission_v1;
mod action_ingress_admission_v2;
