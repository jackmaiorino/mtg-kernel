//! `x_kernel_flat_v4`: the acting seat's V4 model input for the current
//! decision of a flat-action V3 `FastActorSessionV1`, built with the same
//! encoder and tensorizer calls as `FrozenPlayPolicyV1::score_owned`, plus
//! the row to candidate-id map and the feature identities.

use crate::card_def::KERNEL_CARDDB_HASH;
use crate::flat_policy_v2::{
    FlatCompletedDungeonV2, FlatContextPathElementV2, FlatEffectSubtypeChangeV2,
    FlatObjectAbilityUseV2, FlatObjectCoreV2, FlatObjectGoadV2, FlatObjectSubtypeV2,
    FlatRelationV2, FlatScorerActionCoreV2, FlatScorerActionRefV2, FlatScoringDecisionViewV2,
    FlatScoringOwnedBuffersV2,
};
use crate::flat_policy_v4::{FlatDecisionEncoderV4, FlatScoringDecisionViewV4};
use crate::native_flat_tensorizer_v4::{
    NativeFlatDecisionTensorV4, NativeFlatTensorizerV4, FEATURE_CONTRACT_DIGEST_V4,
    FEATURE_ENCODING_DIGEST_V4,
};
use crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::xmage_observed_inference_v1::ObservedTensorBitsV1;
use serde_json::{json, Value};

pub const SPELLBENCH_EXTENSION_KERNEL_FLAT_V4: &str = "x_kernel_flat_v4";
pub const FLAT_V4_EXTENSION_SCHEMA_V1: &str = "mtg-kernel-spellbench-flat-v4/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FlatV4EncodeErrorV1 {
    NotADecision,
    Encode(String),
    Tensorize(String),
    RowCount { rows: usize, tensor_actions: usize },
}

impl FlatV4EncodeErrorV1 {
    pub(crate) fn halt_reason(&self) -> String {
        let detail = match self {
            Self::NotADecision => "not_a_decision".to_string(),
            Self::Encode(message) => format!("encode:{message}"),
            Self::Tensorize(message) => format!("tensorize:{message}"),
            Self::RowCount {
                rows,
                tensor_actions,
            } => format!("row_count:{rows}:{tensor_actions}"),
        };
        format!("engine_contract_failure:flat_v4_encode:{detail}")
    }
}

/// Reused encoder cache and owned tables (the layout of the
/// `flat_policy_v4.rs` test helper `OwnedScoringV4`).
#[derive(Default)]
pub(crate) struct FlatV4EncodeScratchV1 {
    encoder: FlatDecisionEncoderV4,
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

pub(crate) fn encode_flat_v4_tensor_v1(
    fast: &FastActorSessionV1,
    scratch: &mut FlatV4EncodeScratchV1,
) -> Result<NativeFlatDecisionTensorV4, FlatV4EncodeErrorV1> {
    let FastActorResponseV1::Decision(expected) = fast.current_response() else {
        return Err(FlatV4EncodeErrorV1::NotADecision);
    };
    let FlatV4EncodeScratchV1 {
        encoder,
        objects,
        relations,
        object_subtypes,
        ability_uses,
        goads,
        completed_dungeons,
        effect_subtype_changes,
        context_path_elements,
        actions,
        action_refs,
    } = scratch;
    let decision = fast
        .encode_current_flat_scoring_decision_owned_v4(
            expected,
            encoder,
            &mut FlatScoringOwnedBuffersV2 {
                objects,
                relations,
                object_subtypes,
                ability_uses,
                goads,
                completed_dungeons,
                effect_subtype_changes,
                context_path_elements,
                actions,
                action_refs,
            },
        )
        .map_err(|error| FlatV4EncodeErrorV1::Encode(format!("{error:?}")))?;
    let view = FlatScoringDecisionViewV4::new(
        FlatScoringDecisionViewV2::new(
            &decision.globals,
            objects,
            relations,
            object_subtypes,
            ability_uses,
            goads,
            completed_dungeons,
            effect_subtype_changes,
            context_path_elements,
            actions,
            action_refs,
        ),
        &decision.extensions,
    );
    let mut tensor = NativeFlatDecisionTensorV4::default();
    NativeFlatTensorizerV4::default()
        .fill(view, &mut tensor)
        .map_err(|error| FlatV4EncodeErrorV1::Tensorize(format!("{error:?}")))?;
    Ok(tensor)
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|value| value.to_bits()).collect()
}

pub(crate) fn tensor_bits_v1(tensor: &NativeFlatDecisionTensorV4) -> ObservedTensorBitsV1 {
    let t = &tensor.common;
    ObservedTensorBitsV1 {
        state: bits(&t.state),
        object_features: bits(&t.object_features),
        object_card_ids: t.object_card_ids.clone(),
        object_groups: t.object_groups.clone(),
        object_node_ids: t.object_node_ids.clone(),
        edge_features: bits(&t.edge_features),
        edge_source_indices: t.edge_source_indices.clone(),
        edge_target_indices: t.edge_target_indices.clone(),
        action_features: bits(&t.action_features),
        action_ref_features: bits(&t.action_ref_features),
        action_ref_card_ids: t.action_ref_card_ids.clone(),
        action_ref_action_indices: t.action_ref_action_indices.clone(),
        action_ref_node_indices: t.action_ref_node_indices.clone(),
    }
}

pub(crate) fn flat_v4_extension_value_v1(
    tensor: &NativeFlatDecisionTensorV4,
    row_candidate_ids: &[u32],
    acting_seat: &str,
    step: u64,
) -> Result<Value, FlatV4EncodeErrorV1> {
    let action_dim = NativePolicyValueModelConfigV1::contract_v1().action_feature_dim;
    let features = tensor.common.action_features.len();
    let tensor_actions = features / action_dim;
    if tensor_actions * action_dim != features || tensor_actions != row_candidate_ids.len() {
        return Err(FlatV4EncodeErrorV1::RowCount {
            rows: row_candidate_ids.len(),
            tensor_actions,
        });
    }
    Ok(json!({
        "schema": FLAT_V4_EXTENSION_SCHEMA_V1,
        "feature_contract_digest": FEATURE_CONTRACT_DIGEST_V4,
        "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V4,
        "card_db_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
        "acting_seat": acting_seat,
        "step": step,
        "row_candidate_ids": row_candidate_ids,
        "tensor": serde_json::to_value(tensor_bits_v1(tensor)).expect("tensor bits serialize"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
    use serde_json::json;

    const DECKS: [&str; 8] = [
        "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "CawGates", "Faeries",
    ];

    fn bits_value(tensor: &NativeFlatDecisionTensorV4) -> Value {
        serde_json::to_value(tensor_bits_v1(tensor)).unwrap()
    }

    #[test]
    fn extension_tensor_equals_the_harness_tensor_on_live_decisions() {
        let mut harness = FrozenPlayPolicyV1::training_fixture_v4();
        let mut scratch = FlatV4EncodeScratchV1::default();
        let mut checked = 0;
        for (index, deck) in DECKS.iter().enumerate() {
            let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
                900 + index as u64,
                31 + index as u64,
                400,
                40_000,
                [deck.to_string(), deck.to_string()],
            )
            .unwrap();
            for turn in 0..120_u64 {
                let FastActorResponseV1::Decision(decision) = session.current_response() else {
                    break;
                };
                let ours = encode_flat_v4_tensor_v1(&session, &mut scratch).unwrap();
                harness.score_fast_session_v1(&session).unwrap();
                let theirs = harness.last_scored_training_tensor_v4().unwrap();
                assert_eq!(
                    bits_value(&ours),
                    bits_value(theirs),
                    "{deck} decision {turn}"
                );
                checked += 1;
                let row = (turn % u64::from(decision.legal_action_count)) as u32;
                session
                    .step(decision.episode_id, decision.step, row)
                    .unwrap();
            }
        }
        assert!(checked >= 400, "only {checked} decisions checked");
    }

    fn max_integer(value: &Value) -> u64 {
        match value {
            Value::Number(number) => number.as_u64().unwrap_or(u64::MAX),
            Value::Array(items) => items.iter().map(max_integer).max().unwrap_or(0),
            Value::Object(map) => map.values().map(max_integer).max().unwrap_or(0),
            _ => 0,
        }
    }

    #[test]
    fn extension_value_carries_identity_row_map_and_safe_integers() {
        let session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
            77,
            5,
            400,
            40_000,
            ["Burn".to_string(), "Burn".to_string()],
        )
        .unwrap();
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("decision")
        };
        let tensor =
            encode_flat_v4_tensor_v1(&session, &mut FlatV4EncodeScratchV1::default()).unwrap();
        let rows: Vec<u32> = (0..decision.legal_action_count).rev().collect();
        let value = flat_v4_extension_value_v1(&tensor, &rows, "p0", decision.step).unwrap();
        assert_eq!(value["schema"], "mtg-kernel-spellbench-flat-v4/v1");
        assert_eq!(
            value["feature_contract_digest"],
            "c4af415a3b0cf1e9c9960dbe2bc2d134c63e9f08206a9a364e113121fea5538b"
        );
        assert_eq!(
            value["feature_encoding_digest"],
            "271c0e5a0fdce75663c897e89a9d7280ab1a3bbb6679bd10ecb5f524991952de"
        );
        assert_eq!(value["card_db_hash"], "064a7c989255ab3c");
        assert_eq!(value["acting_seat"], "p0");
        assert_eq!(value["row_candidate_ids"], json!(rows));
        assert_eq!(value["tensor"].as_object().unwrap().len(), 13);
        assert!(max_integer(&value) <= 1_u64 << 53);
    }

    #[test]
    fn a_row_map_of_the_wrong_length_is_refused() {
        let session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
            78,
            6,
            400,
            40_000,
            ["Burn".to_string(), "Burn".to_string()],
        )
        .unwrap();
        let tensor =
            encode_flat_v4_tensor_v1(&session, &mut FlatV4EncodeScratchV1::default()).unwrap();
        let actions = tensor.common.action_features.len() / 195;
        let rows: Vec<u32> = (0..actions as u32 + 1).collect();
        assert_eq!(
            flat_v4_extension_value_v1(&tensor, &rows, "p0", 0),
            Err(FlatV4EncodeErrorV1::RowCount {
                rows: actions + 1,
                tensor_actions: actions
            })
        );
    }
}
