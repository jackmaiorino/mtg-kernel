//! The V4 inference export of a FlatV7 checkpoint, and its equality check.
//!
//! FlatV7 (`native_flat_tensorizer_v7.rs`) is the V4 tensor generation under
//! its own feature identity: the same shapes, parameters and forward
//! arithmetic (`feature_transfer_config_v7` is the V4 config with the V7
//! digests), and an in-game decision gives the V4 tensors byte for byte.
//! Only two decision classes differ. A London pregame decision is V7's only:
//! the V4 tensorizer refuses it. An in-game decision with a numeric choice
//! (`ChooseEffectNumber`, action-kind column 15) is V4's only: V7 gives that
//! column the meaning `london_opening` and refuses the whole decision.
//!
//! A FlatV7 lineage saves checkpoints under the V7 identity, and a scorer
//! that serves the V4 identity (the Spellbench board's) cannot load one: the
//! V4 source refuses a record whose identity is FlatV7. The export writes
//! the same checkpoint record with the two identity members
//! (`feature_contract_digest`, `feature_encoding_digest`) set to V4's and
//! nothing else changed: every parameter and Adam moment bit, the Adam step,
//! the gauge anchor, the provenance (`source_import`), the loss and its
//! scalars and the trajectory pins. The state digest (`state_sha256`) is a
//! function of the tensors alone, so it is unchanged, and so is the
//! `model_state_sha256` a scorer reports. It is the reverse of the
//! V4-to-V7 transfer (`into_flat_v7_transfer_v1`), which also keeps every
//! parameter.
//!
//! [`export_flat_v7_to_v4_inference_v1`] admits only a FlatV7 source (both
//! V7 digests, net-8, a checkpoint) whose checkpoint is a fresh-lineage
//! record with both V7 digests. It loads it in full under the V7 identity
//! before writing anything, writes the export, loads the export in full
//! under the V4 identity, and refuses (removing the file) unless the two
//! loads have the same state digest and model weights. Its receipt carries
//! both pins, both state digests and the parameter-bytes digest of each.
//!
//! [`flat_v7_v4_equality_v1`] is the in-game equality check: it plays games
//! between fixed decks without London mulligans and, at every decision,
//! sends the same decision to two Spellbench scorers at the same sample
//! seed: the V7 policy with its own V7 tensors and V7 digests, and the
//! exported V4 policy with its V4 tensors and V4 digests. A row is equal
//! when the tensors, every logit bit, the value bit and the sampled row are
//! equal. A numeric-choice decision is counted apart: the V4 scorer answers
//! it and the V7 tensorizer refuses it.

use super::*;
use crate::native_flat_tensorizer_v7::{FEATURE_CONTRACT_DIGEST_V7, FEATURE_ENCODING_DIGEST_V7};
use crate::sideboard_play_policy_v1::{FRESH_FEATURE_IDENTITY_V4, FRESH_FEATURE_IDENTITY_V7};
use crate::spellbench_scorer_v1::{
    SpellbenchScorerV1, SCORER_REQUEST_SCHEMA_V1, SELECTION_SAMPLED_WIDE_V1,
};
use crate::xmage_observed_inference_v1::ObservedTensorBitsV1;

pub const V4_INFERENCE_EXPORT_RECEIPT_SCHEMA_V1: &str =
    "mtg-kernel-flat-v7-to-v4-inference-export-receipt/v1";
pub const V7_V4_EQUALITY_REQUEST_SCHEMA_V1: &str = "mtg-kernel-flat-v7-v4-equality-request/v1";
pub const V7_V4_EQUALITY_RECEIPT_SCHEMA_V1: &str = "mtg-kernel-flat-v7-v4-equality-receipt/v1";

/// Width of a V4 (and FlatV7) action row.
const ACTION_ROW_WIDTH_V1: usize = 195;
/// The action-kind column of `ChooseEffectNumber` in a V4 action row, which
/// FlatV7 gives to `london_opening`.
const NUMERIC_CHOICE_KIND_COLUMN_V1: usize = 15;
/// The action-kind one-hot columns of an action row.
const ACTION_KIND_COLUMNS_V1: usize = 27;

/// The V4 identity of `source`'s play import with `checkpoint`.
fn v4_source_v1(source: &ExpandedModelSourceV1, checkpoint: PinnedFileV1) -> ExpandedModelSourceV1 {
    ExpandedModelSourceV1 {
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
            expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
        },
        checkpoint: Some(checkpoint),
        ..source.clone()
    }
}

/// The source's admission: net-8, both FlatV7 digests, and a checkpoint.
fn admit_source_v1(source: &ExpandedModelSourceV1) -> Result<&PinnedFileV1, String> {
    ensure(
        source.s_learner.is_none(),
        "the V4 inference export takes a net-8 source; a native S source refuses it",
    )?;
    ensure(
        source.feature_transfer.expected_feature_contract_digest == FEATURE_CONTRACT_DIGEST_V7
            && source.feature_transfer.expected_feature_encoding_digest
                == FEATURE_ENCODING_DIGEST_V7,
        "the V4 inference export takes a FlatV7 source (both FlatV7 feature digests)",
    )?;
    source
        .checkpoint
        .as_ref()
        .ok_or_else(|| "the V4 inference export requires the FlatV7 checkpoint".into())
}

/// The checkpoint record's admission: a fresh-lineage record with both
/// FlatV7 digests. A V4 record (which the FlatV7 source would load through
/// the V4-to-V7 transfer), a FlatV8 one and a half-V7 one are refused here;
/// every other check is the full FlatV7 load's.
fn admit_checkpoint_v1(saved: &ExpandedCheckpointV1) -> Result<(), String> {
    ensure(
        saved.schema == FRESH_CHECKPOINT_SCHEMA && saved.source_import.is_fresh_v1(),
        "the V4 inference export takes a fresh-lineage checkpoint",
    )?;
    ensure(
        saved.feature_contract_digest == FEATURE_CONTRACT_DIGEST_V7,
        "the V4 inference export takes a checkpoint with the FlatV7 contract digest",
    )?;
    ensure(
        saved.feature_encoding_digest == FEATURE_ENCODING_DIGEST_V7,
        "the V4 inference export takes a checkpoint with the FlatV7 encoding digest",
    )
}

/// SHA-256 of every tensor's little-endian `f32` bits, in record order.
fn tensor_bytes_sha256_v1(rows: &[ParameterBitsV1]) -> String {
    let mut digest = Sha256::new();
    for row in rows {
        for value in &row.values {
            digest.update(value.to_le_bytes());
        }
    }
    hex(&digest.finalize())
}

/// Byte positions of the value of `"member":"..."` in a v1 JSON record.
fn member_value_span_v1(bytes: &[u8], member: &str) -> Result<std::ops::Range<usize>, String> {
    let key = format!("\"{member}\":\"");
    let start = bytes
        .windows(key.len())
        .position(|window| window == key.as_bytes())
        .ok_or("an identity member is missing from the record")?
        + key.len();
    let length = bytes[start..]
        .iter()
        .position(|&byte| byte == b'"')
        .ok_or("an identity member is not a string")?;
    Ok(start..start + length)
}

/// Exports the FlatV7 checkpoint of `source` under the V4 identity to
/// `output` (a fresh absolute path) and returns the receipt.
pub fn export_flat_v7_to_v4_inference_v1(
    source: &ExpandedModelSourceV1,
    output: &Path,
) -> Result<Value, String> {
    let input = admit_source_v1(source)?;
    let input_bytes = read_pinned_bytes(input)?;
    let record = record_envelope_v1::open_v1(&input_bytes, MAX_FILE_BYTES)?;
    let input_form = if checkpoint_container_v1::is_container_v1(&record) {
        "checkpoint-container-v2"
    } else {
        "checkpoint-v1-json"
    };
    let saved = checkpoint_container_v1::parse_checkpoint_v1(&input_bytes)?;
    admit_checkpoint_v1(&saved)?;
    // The full FlatV7 load: parameter layout and shapes, moments, state
    // digest, provenance, loss, card database and math.
    let (v7_policy, v7_identity) = load_expanded_inference_v1(source)?;
    ensure(
        v7_policy.feature_identity_v1() == FRESH_FEATURE_IDENTITY_V7,
        "the input did not load under the FlatV7 identity",
    )?;
    let exported = ExpandedCheckpointV1 {
        feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
        feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
        ..saved.clone()
    };
    let bytes = serde_json::to_vec(&exported).map_err(err)?;
    // Only the two identity members differ from the input's record.
    let identity_members = ["feature_contract_digest", "feature_encoding_digest"];
    let without_identity = |record: &ExpandedCheckpointV1| -> Result<Value, String> {
        let mut value = serde_json::to_value(record).map_err(err)?;
        let object = value.as_object_mut().ok_or("a record is not an object")?;
        for member in identity_members {
            object.remove(member);
        }
        Ok(value)
    };
    let reparsed: ExpandedCheckpointV1 = serde_json::from_slice(&bytes).map_err(err)?;
    ensure(
        without_identity(&reparsed)? == without_identity(&saved)?,
        "the export differs from its input outside the identity members",
    )?;
    let input_parameters = tensor_bytes_sha256_v1(&saved.parameters);
    let output_parameters = tensor_bytes_sha256_v1(&reparsed.parameters);
    ensure(
        input_parameters == output_parameters,
        "the export's parameter bytes differ from its input's",
    )?;
    // A canonical v1 JSON input differs from the export only inside the two
    // identity values (both digests are 64 hex characters).
    let canonical_input = input_form == "checkpoint-v1-json"
        && serde_json::to_vec(&saved).map_err(err)?.as_slice() == &record[..];
    let differing_bytes = if canonical_input {
        let spans = identity_members
            .iter()
            .map(|member| member_value_span_v1(&bytes, member))
            .collect::<Result<Vec<_>, _>>()?;
        ensure(
            bytes.len() == record.len(),
            "the export's length differs from its input's",
        )?;
        let differing: Vec<usize> = (0..bytes.len())
            .filter(|&index| bytes[index] != record[index])
            .collect();
        ensure(
            differing
                .iter()
                .all(|index| spans.iter().any(|span| span.contains(index))),
            "the export differs from its input outside the identity values",
        )?;
        Some(differing.len())
    } else {
        None
    };
    let written = publish_checkpoint_file_v1(output, &bytes)?;
    let verified = verify_export_v1(source, &written, &v7_policy, &v7_identity);
    let (v4_source, v4_identity) = match verified {
        Ok(verified) => verified,
        Err(error) => {
            let _ = fs::remove_file(&written.path);
            return Err(error);
        }
    };
    Ok(json!({
        "schema": V4_INFERENCE_EXPORT_RECEIPT_SCHEMA_V1,
        "purpose": "inference under the V4 feature identity; the parameters are the FlatV7 checkpoint's",
        "input": {
            "checkpoint": input,
            "form": input_form,
            "canonical": canonical_input,
            "feature_contract_digest": saved.feature_contract_digest,
            "feature_encoding_digest": saved.feature_encoding_digest,
            "state_sha256": v7_identity.state_sha256,
            "parameter_bytes_sha256": input_parameters,
            "first_moment_bytes_sha256": tensor_bytes_sha256_v1(&saved.first_moments),
            "second_moment_bytes_sha256": tensor_bytes_sha256_v1(&saved.second_moments),
            "model": v7_identity.model,
        },
        "output": {
            "checkpoint": written,
            "form": "checkpoint-v1-json",
            "feature_contract_digest": FEATURE_CONTRACT_DIGEST_V4,
            "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V4,
            "state_sha256": v4_identity.state_sha256,
            "parameter_bytes_sha256": output_parameters,
            "first_moment_bytes_sha256": tensor_bytes_sha256_v1(&reparsed.first_moments),
            "second_moment_bytes_sha256": tensor_bytes_sha256_v1(&reparsed.second_moments),
            "model": v4_identity.model,
        },
        "changed_members": identity_members,
        "differing_bytes": differing_bytes,
        "adam_step": saved.adam_step,
        "v4_source": v4_source,
        "git_commit": env!("MTG_KERNEL_BUILD_GIT_HEAD"),
    }))
}

/// The written export, loaded in full under the V4 identity: the same state
/// digest and model weights as the FlatV7 load.
fn verify_export_v1(
    source: &ExpandedModelSourceV1,
    written: &PinnedFileV1,
    v7_policy: &FrozenPlayPolicyV1,
    v7_identity: &ExpandedInferenceIdentityV1,
) -> Result<(ExpandedModelSourceV1, ExpandedInferenceIdentityV1), String> {
    let v4_source = v4_source_v1(source, written.clone());
    let (v4_policy, v4_identity) = load_expanded_inference_v1(&v4_source)?;
    ensure(
        v4_policy.feature_identity_v1() == FRESH_FEATURE_IDENTITY_V4,
        "the export did not load under the V4 identity",
    )?;
    let (v4_model, v7_model) = (
        v4_policy.actual_model_identity_v1(),
        v7_policy.actual_model_identity_v1(),
    );
    ensure(
        v4_identity.state_sha256 == v7_identity.state_sha256
            && v4_identity.adam_step == v7_identity.adam_step
            && v4_identity.source_import == v7_identity.source_import
            && v4_model.weights_sha256 == v7_model.weights_sha256
            && v4_model.model_parameter_sha256 == v7_model.model_parameter_sha256
            && v4_model.embedding_table_sha256 == v7_model.embedding_table_sha256,
        "the export's V4 load differs from the FlatV7 load",
    )?;
    Ok((v4_source, v4_identity))
}

/// One equality game: fixed decks and starting player, no London mulligans
/// (each seat keeps its seven).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct V7V4EqualityGameV1 {
    pub seed: u64,
    pub starting_player: u8,
    pub decks: [String; 2],
}

/// The equality check's request: the FlatV7 source, the V4 export of its
/// checkpoint, the games and a bound on each game's decisions.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct V7V4EqualityRequestV1 {
    pub schema: String,
    pub v7_source: ExpandedModelSourceV1,
    pub v4_checkpoint: PinnedFileV1,
    pub max_decisions_per_game: u64,
    /// The Spellbench neutral choices (`spellbench_neutral_v1.choices`), as
    /// the board's engine plays: single-option choices are posed, and a
    /// forced X = 0 is a singleton numeric choice. Off is the trainer's
    /// engine, which resolves them itself.
    pub neutral_choices: bool,
    pub games: Vec<V7V4EqualityGameV1>,
}

/// The forced X = 0 decision both ways, on copies of `state`: the board's
/// path (the posed singleton numeric choice, option 0) and the trainer's
/// (no decision: the engine sets X to its forced value itself). Both copies
/// then run on to the next decision under the trainer's rules, and their
/// announced X, next decision and every byte of the game state must agree.
pub(crate) fn forced_x_trainer_path_v1(state: &crate::state::GameState) -> Result<Value, String> {
    use crate::engine::{advance_until_decision, step, Action};
    let announced = |state: &crate::state::GameState| {
        state
            .engine
            .pending_cast
            .as_ref()
            .and_then(|pending| pending.selected_x_v59())
    };
    let mut board = state.clone();
    step(&mut board, Action::ChooseEffectOption(0))?;
    let board_x = announced(&board);
    board.engine.spellbench_neutral_v1.choices = false;
    let board_next = advance_until_decision(&mut board);
    let mut trainer = state.clone();
    trainer.engine.spellbench_neutral_v1.choices = false;
    let trainer_next = advance_until_decision(&mut trainer);
    let board_bytes = serde_json::to_vec(&board).map_err(err)?;
    let trainer_bytes = serde_json::to_vec(&trainer).map_err(err)?;
    let next_equal = format!("{board_next:?}") == format!("{trainer_next:?}");
    let bytes_equal = board_bytes == trainer_bytes;
    Ok(json!({
        "board_announced_x": board_x,
        "next_decision_equal": next_equal,
        "state_bytes_equal": bytes_equal,
        "state_sha256": [sha(&board_bytes), sha(&trainer_bytes)],
        "equal": board_x == Some(0) && next_equal && bytes_equal,
    }))
}

fn request_bits_v1(tensor: &NativeFlatDecisionTensorV2) -> ObservedTensorBitsV1 {
    let b = |values: &[f32]| {
        values
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>()
    };
    ObservedTensorBitsV1 {
        state: b(&tensor.state),
        object_features: b(&tensor.object_features),
        object_card_ids: tensor.object_card_ids.clone(),
        object_groups: tensor.object_groups.clone(),
        object_node_ids: tensor.object_node_ids.clone(),
        edge_features: b(&tensor.edge_features),
        edge_source_indices: tensor.edge_source_indices.clone(),
        edge_target_indices: tensor.edge_target_indices.clone(),
        action_features: b(&tensor.action_features),
        action_ref_features: b(&tensor.action_ref_features),
        action_ref_card_ids: tensor.action_ref_card_ids.clone(),
        action_ref_action_indices: tensor.action_ref_action_indices.clone(),
        action_ref_node_indices: tensor.action_ref_node_indices.clone(),
    }
}

/// A scorer request line for `tensor` under the identity `(contract,
/// encoding)`, rows mapped to themselves.
fn scorer_request_v1(
    tensor: &NativeFlatDecisionTensorV2,
    contract: &str,
    encoding: &str,
    game: usize,
    step: u64,
    seat: crate::rl::PlayerSeatV1,
    sample_seed: u64,
) -> Result<Vec<u8>, String> {
    let rows = (tensor.action_features.len() / ACTION_ROW_WIDTH_V1) as u32;
    let game_id = format!("v7-v4-equality-{game}");
    serde_json::to_vec(&json!({
        "schema": SCORER_REQUEST_SCHEMA_V1,
        "request_id": format!("{game_id}:{step}"),
        "game_id": game_id,
        "seat": if seat == crate::rl::PlayerSeatV1::P0 { "p0" } else { "p1" },
        "step": step,
        "feature_contract_digest": contract,
        "feature_encoding_digest": encoding,
        "row_candidate_ids": (0..rows).collect::<Vec<_>>(),
        "sample_seed": sample_seed,
        "tensor": request_bits_v1(tensor),
    }))
    .map_err(err)
}

/// The action kinds (one-hot columns) present among `tensor`'s rows.
fn action_kinds_v1(tensor: &NativeFlatDecisionTensorV2) -> BTreeSet<usize> {
    tensor
        .action_features
        .chunks_exact(ACTION_ROW_WIDTH_V1)
        .filter_map(|row| row[..ACTION_KIND_COLUMNS_V1].iter().position(|&x| x == 1.0))
        .collect()
}

/// Runs the in-game equality check of `request` and returns its receipt.
pub fn flat_v7_v4_equality_v1(request: &V7V4EqualityRequestV1) -> Result<Value, String> {
    ensure(
        request.schema == V7_V4_EQUALITY_REQUEST_SCHEMA_V1,
        "unknown V7-V4 equality request schema",
    )?;
    ensure(
        !request.games.is_empty() && request.max_decisions_per_game > 0,
        "the equality check needs games and a positive decision bound",
    )?;
    admit_source_v1(&request.v7_source)?;
    let v4_source = v4_source_v1(&request.v7_source, request.v4_checkpoint.clone());
    let (v7_policy, v7_identity) = load_expanded_inference_v1(&request.v7_source)?;
    let (v4_policy, v4_identity) = load_expanded_inference_v1(&v4_source)?;
    ensure(
        v7_policy.feature_identity_v1() == FRESH_FEATURE_IDENTITY_V7
            && v4_policy.feature_identity_v1() == FRESH_FEATURE_IDENTITY_V4,
        "the equality check takes a FlatV7 policy and a V4 policy",
    )?;
    ensure(
        v4_identity.state_sha256 == v7_identity.state_sha256,
        "the V4 checkpoint is not the FlatV7 checkpoint's state",
    )?;
    let scorer = |policy: &FrozenPlayPolicyV1,
                  identity: &ExpandedInferenceIdentityV1|
     -> Result<SpellbenchScorerV1, String> {
        SpellbenchScorerV1::new(
            policy.duplicate_loaded_v1()?,
            serde_json::to_value(identity).map_err(err)?,
            identity.state_sha256.clone(),
            SELECTION_SAMPLED_WIDE_V1,
        )
        .map_err(String::from)
    };
    let mut v7_scorer = scorer(&v7_policy, &v7_identity)?;
    let mut v4_scorer = scorer(&v4_policy, &v4_identity)?;
    let (mut rows, mut mismatches, mut numeric_choice, mut v7_only_refusals) =
        (0u64, 0u64, 0u64, 0u64);
    let mut numeric_equal = 0u64;
    let mut numeric_cases = Vec::new();
    let mut kinds = std::collections::BTreeMap::<usize, u64>::new();
    let mut first_mismatch = Value::Null;
    let mut games = Vec::new();
    for (index, game) in request.games.iter().enumerate() {
        let mainboard = |id: &str| -> Result<Vec<u16>, String> {
            crate::runtime_decks::runtime_deck_by_id(id)
                .map(|deck| deck.card_ids.to_vec())
                .ok_or_else(|| format!("unknown runtime deck {id:?}"))
        };
        let starting = match game.starting_player {
            0 => PlayerId::P0,
            1 => PlayerId::P1,
            _ => return Err("starting_player is 0 or 1".into()),
        };
        let mut session = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
            index as u64,
            game.seed,
            100_000,
            1_000_000,
            game.decks.clone(),
            [mainboard(&game.decks[0])?, mainboard(&game.decks[1])?],
            starting,
        )
        .map_err(|error| format!("{error:?}"))?;
        // The trainer's collection reset (`episode_session_v1`, no London).
        if request.neutral_choices {
            session.enable_spellbench_neutral_choices();
        }
        let mut v4_driver = v4_policy.duplicate_loaded_v1()?;
        let mut v7_driver = v7_policy.duplicate_loaded_v1()?;
        let mut seeds = SplitMix64::seed(game.seed ^ 0x7e57_e9a1_17f4_0001);
        let (mut game_rows, mut game_numeric, mut decisions) = (0u64, 0u64, 0u64);
        let mut ending = "decision bound";
        let mut v4_refusal = Value::Null;
        while decisions < request.max_decisions_per_game {
            let FastActorResponseV1::Decision(decision) = session.current_response() else {
                ending = "terminal";
                break;
            };
            decisions += 1;
            if let Err(error) = v4_driver.score_fast_session_v1(&session) {
                // The V4 policy cannot encode this decision either: the game
                // ends here and the receipt says why.
                ending = "V4 refusal";
                v4_refusal = json!(error);
                break;
            }
            let v4_tensor = v4_driver.last_scored_training_tensor_v4()?.common.clone();
            let numeric = v4_tensor
                .action_features
                .chunks_exact(ACTION_ROW_WIDTH_V1)
                .any(|row| row[NUMERIC_CHOICE_KIND_COLUMN_V1] == 1.0);
            let seed = seeds.next_u64();
            let v4_line = scorer_request_v1(
                &v4_tensor,
                FEATURE_CONTRACT_DIGEST_V4,
                FEATURE_ENCODING_DIGEST_V4,
                index,
                decision.step,
                decision.acting_player,
                seed,
            )?;
            let v4_choice = v4_scorer
                .score_line(&v4_line)
                .map_err(|code| format!("the V4 scorer refused a request: {code}"))?;
            match v7_driver.score_fast_session_v1(&session) {
                Ok(_) => {
                    let v7_tensor = v7_driver.last_scored_training_tensor_v4()?.common.clone();
                    let v7_line = scorer_request_v1(
                        &v7_tensor,
                        FEATURE_CONTRACT_DIGEST_V7,
                        FEATURE_ENCODING_DIGEST_V7,
                        index,
                        decision.step,
                        decision.acting_player,
                        seed,
                    )?;
                    let v7_choice = v7_scorer
                        .score_line(&v7_line)
                        .map_err(|code| format!("the V7 scorer refused a request: {code}"))?;
                    let same = v4_tensor == v7_tensor
                        && !numeric
                        && [
                            "logits_bits",
                            "value_bits",
                            "selected_row",
                            "model_state_sha256",
                        ]
                        .iter()
                        .all(|member| v4_choice[member] == v7_choice[member]);
                    rows += 1;
                    game_rows += 1;
                    for kind in action_kinds_v1(&v4_tensor) {
                        *kinds.entry(kind).or_default() += 1;
                    }
                    if !same {
                        mismatches += 1;
                        if first_mismatch.is_null() {
                            first_mismatch = json!({"game": index, "step": decision.step,
                                "tensors_equal": v4_tensor == v7_tensor, "numeric_choice": numeric,
                                "v4": v4_choice, "v7": v7_choice});
                        }
                    }
                }
                Err(error) => {
                    if numeric {
                        // The board's numeric choice against the trainer's
                        // path: one forced X = 0 row, row 0 chosen, and the
                        // same game state as the trainer's automatic X.
                        numeric_choice += 1;
                        game_numeric += 1;
                        let semantics = session
                            .diagnostic_current_action_semantics()
                            .unwrap_or_default();
                        let forced_zero = matches!(
                            semantics.as_slice(),
                            [crate::rl::ActionSemanticV1::ChooseEffectNumber {
                                number: 0,
                                minimum: 0,
                                maximum: 0,
                                ..
                            }]
                        );
                        let trainer = forced_x_trainer_path_v1(session.game_state())?;
                        let ok = forced_zero
                            && v4_choice["selected_row"] == json!(0)
                            && trainer["equal"] == json!(true);
                        numeric_equal += u64::from(ok);
                        if numeric_cases.len() < 64 || !ok {
                            numeric_cases.push(json!({"game": index, "step": decision.step,
                                "decks": game.decks, "actor": format!("{:?}", decision.acting_player),
                                "semantics": format!("{semantics:?}"),
                                "v4_selected_row": v4_choice["selected_row"],
                                "v4_logits_bits": v4_choice["logits_bits"],
                                "v7_refusal": error, "trainer_path": trainer, "equal": ok}));
                        }
                    } else {
                        v7_only_refusals += 1;
                        if first_mismatch.is_null() {
                            first_mismatch = json!({"game": index, "step": decision.step,
                                "v7_refusal": error});
                        }
                    }
                    // A refusal poisons the V7 tensorizer: start a fresh one.
                    v7_driver = v7_policy.duplicate_loaded_v1()?;
                }
            }
            let row = v4_choice["selected_row"]
                .as_u64()
                .ok_or("the V4 scorer chose no row")?;
            session
                .step(decision.episode_id, decision.step, row as u32)
                .map_err(|error| format!("{error:?}"))?;
        }
        games.push(
            json!({"game": game, "decisions": decisions, "equal_rows": game_rows,
            "numeric_choice_decisions": game_numeric, "ending": ending,
            "v4_refusal": v4_refusal}),
        );
    }
    ensure(rows > 0, "the equality check compared no rows")?;
    Ok(json!({
        "schema": V7_V4_EQUALITY_RECEIPT_SCHEMA_V1,
        "definition": "each decision of each game goes to two Spellbench scorers at the same sample seed: the FlatV7 policy with its own FlatV7 tensors and digests, and the V4 export with its V4 tensors and digests; a row is equal when the tensors, every logit's f32 bits, the value's bits, the sampled row and the model state digest are equal; the game follows the V4 scorer's choice; a numeric-choice decision (action-kind column 15, which the FlatV7 tensorizer refuses) is counted apart and is equal when it is the forced X = 0 singleton, the V4 scorer chooses its one row, and the board's explicit X = 0 and the trainer's automatic X = 0 lead to the same next decision and the same game-state bytes",
        "v7_source": request.v7_source,
        "v4_checkpoint": request.v4_checkpoint,
        "model_state_sha256": v4_identity.state_sha256,
        "games": games,
        "rows": rows,
        "mismatches": mismatches,
        "neutral_choices": request.neutral_choices,
        "numeric_choice_decisions": numeric_choice,
        "numeric_choice_equal_to_trainer_path": numeric_equal,
        "numeric_choice_cases": numeric_cases,
        "v7_only_refusals": v7_only_refusals,
        "action_kind_rows": kinds.iter().map(|(kind, count)| (kind.to_string(), json!(count))).collect::<serde_json::Map<_, _>>(),
        "first_mismatch": first_mismatch,
        "equal": mismatches == 0 && v7_only_refusals == 0 && numeric_equal == numeric_choice,
        "git_commit": env!("MTG_KERNEL_BUILD_GIT_HEAD"),
    }))
}

/// [`flat_v7_v4_equality_v1`] of one JSON request.
pub fn flat_v7_v4_equality_json_v1(text: &str) -> Result<Value, String> {
    crate::rl::parse_strict_json_value(text).map_err(err)?;
    let request: V7V4EqualityRequestV1 = serde_json::from_str(text).map_err(err)?;
    flat_v7_v4_equality_v1(&request)
}

#[cfg(test)]
mod tests;
