//! The V8 inclusion conversion of an RT1 checkpoint (collab
//! LANES/w4-combat-20261030/DESIGN.md v5.1, section 5): a V4 (or FlatV7)
//! fresh-lineage checkpoint, such as an OH2 b512 dose, becomes a FlatV8
//! checkpoint of the V8 inclusion net by the function-preserving conversion
//! (`native_policy_train_step_v1::v8_inclusion_v1`), and a preservation check
//! plays real games with the converted policy and compares every logit and
//! value, bit for bit, with the parent's on the same decisions.
//!
//! The converted checkpoint is the parent's with the FlatV8 identity, the
//! converted parameters and moments (every new moment +0, `adam_step` and the
//! gauge anchor kept) and its own state digest; its source import, loss and
//! optimizer scalars are the parent's, so it resumes as the parent did,
//! under a FlatV8 source (`flat_v8_requested_v1`).
//!
//! The preservation check loads both checkpoints through the ordinary source
//! loader (the parent under its own identity, the child under FlatV8), plays
//! the requested games with the child as both seats and, at every decision,
//! scores the parent on the same decision's FlatV7 tensors (the FlatV8 row
//! without its include column; for an in-game decision these are the V4
//! tensors) through the parent's V4-generation forward. A row is retained
//! when it was scored; the receipt counts rows, rows with an include bit set,
//! rows with a pairing relation and mismatches (f32 bits, `to_bits`, of every logit and
//! the value).

use super::*;
use crate::native_policy_train_step_v1::v8_inclusion_v1::{
    convert_net8_snapshot_to_v8_v1, V8_CONVERSION_CONSTRUCTION_VERSION_V1,
};
use crate::native_policy_train_step_v1::widening_v1::TORCH_DEFAULT_UNIFORM_V1;
use crate::native_policy_value_net_v1::v8_inclusion_v1::V8_ACTION_FEATURE_DIM_V1;

/// FlatV7's London pregame stage column (`features_v10.py`), 1.0 exactly at
/// a pregame decision.
const LONDON_PREGAME_STAGE_COLUMN_V1: usize = 89;

pub const V8_CONVERSION_REQUEST_SCHEMA_V1: &str = "mtg-kernel-v8-inclusion-conversion-request/v1";
pub const V8_CONVERSION_RECEIPT_SCHEMA_V1: &str = "mtg-kernel-v8-inclusion-conversion-receipt/v1";

/// One preservation game: the child plays both seats.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct V8PreservationGameV1 {
    pub seed: u64,
    pub starting_player: u8,
    pub decks: [String; 2],
    /// London mulligans (`london-v1`), both seats learned.
    #[serde(default)]
    pub london: bool,
}

/// One conversion: the parent checkpoint and its play import, the output
/// path, the seed of the pairing path's draws and the preservation games.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct V8ConversionRequestV1 {
    pub schema: String,
    pub play_import: PinnedFileV1,
    pub parent_checkpoint: PinnedFileV1,
    pub output_checkpoint: PathBuf,
    pub seed: u64,
    pub preservation_games: Vec<V8PreservationGameV1>,
}

/// The parent's train snapshot (net-8 layout), checked against its digest.
fn parent_snapshot_v1(
    saved: &ExpandedCheckpointV1,
) -> Result<NativePolicyValueTrainSnapshotV1, String> {
    let template =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(err)?
            .parameter_snapshot_v1();
    let snapshot = NativePolicyValueTrainSnapshotV1 {
        adam_step: saved.adam_step,
        scorer_bias_anchor_bits: saved.scorer_bias_anchor_bits,
        parameters: restore_parameters(&saved.parameters, &template)?,
        first_moments: restore_parameters(&saved.first_moments, &template)?,
        second_moments: restore_parameters(&saved.second_moments, &template)?,
    };
    ensure(
        hex(&snapshot.state_sha256_v1().map_err(err)?) == saved.state_sha256,
        "parent checkpoint state hash differs",
    )?;
    Ok(snapshot)
}

/// The parent's admission: an ordinary fresh-lineage checkpoint of the V4
/// generation (V4 or FlatV7 identity) under the platform libm, with no input
/// contract, transfer, distillation or auxiliary continuation.
fn admit_parent_v1(saved: &ExpandedCheckpointV1) -> Result<(), String> {
    ensure(
        saved.schema == FRESH_CHECKPOINT_SCHEMA && saved.source_import.is_fresh_v1(),
        "the V8 conversion takes a fresh-lineage checkpoint",
    )?;
    ensure(
        [
            (FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4),
            (
                crate::native_flat_tensorizer_v7::FEATURE_CONTRACT_DIGEST_V7,
                crate::native_flat_tensorizer_v7::FEATURE_ENCODING_DIGEST_V7,
            ),
        ]
        .contains(&(
            saved.feature_contract_digest.as_str(),
            saved.feature_encoding_digest.as_str(),
        )),
        "the V8 conversion takes a V4 or FlatV7 checkpoint",
    )?;
    ensure(
        saved.input_contract.is_none()
            && saved.registry_transfer.is_none()
            && saved.archive_distillation.is_none()
            && saved.e_supported.is_none()
            && saved.soft_q.is_none()
            && saved.search_targets.is_none()
            && saved.ex1.is_none()
            && saved.training_math.is_platform_libm(),
        "the V8 conversion takes an ordinary platform-libm checkpoint",
    )
}

/// The converted checkpoint: its pin, the parent's record and the
/// conversion's provenance.
pub(crate) struct WrittenV8CheckpointV1 {
    pub(crate) pin: PinnedFileV1,
    pub(crate) parent: ExpandedCheckpointV1,
    pub(crate) provenance:
        crate::native_policy_train_step_v1::v8_inclusion_v1::V8ConversionProvenanceV1,
}

/// Converts the pinned `parent` checkpoint and writes the FlatV8 checkpoint
/// to `output`, which must not exist.
pub(crate) fn write_v8_checkpoint_v1(
    parent: &PinnedFileV1,
    output: &Path,
    seed: u64,
) -> Result<WrittenV8CheckpointV1, String> {
    ensure(!output.exists(), "the V8 conversion output already exists")?;
    let parent_bytes = read_pinned_bytes(parent)?;
    let saved: ExpandedCheckpointV1 = serde_json::from_slice(&parent_bytes).map_err(err)?;
    admit_parent_v1(&saved)?;
    let snapshot = parent_snapshot_v1(&saved)?;
    let converted = convert_net8_snapshot_to_v8_v1(&snapshot, TORCH_DEFAULT_UNIFORM_V1, seed)
        .map_err(|error| error.to_string())?;
    let child = converted.state.snapshot_v1().map_err(err)?;
    let tensors = |values: &[NativeNamedParameterV1]| -> Vec<ParameterBitsV1> {
        values.iter().map(ParameterBitsV1::from_native).collect()
    };
    let child_checkpoint = ExpandedCheckpointV1 {
        feature_contract_digest: crate::native_flat_tensorizer_v8::FEATURE_CONTRACT_DIGEST_V8
            .into(),
        feature_encoding_digest: crate::native_flat_tensorizer_v8::FEATURE_ENCODING_DIGEST_V8
            .into(),
        state_sha256: converted.provenance.child_state_sha256.clone(),
        adam_step: child.adam_step,
        scorer_bias_anchor_bits: child.scorer_bias_anchor_bits,
        parameters: tensors(&child.parameters),
        first_moments: tensors(&child.first_moments),
        second_moments: tensors(&child.second_moments),
        ..saved.clone()
    };
    let child_bytes = serde_json::to_vec(&child_checkpoint).map_err(err)?;
    if let Some(directory) = output.parent() {
        fs::create_dir_all(directory).map_err(err)?;
    }
    fs::write(output, &child_bytes).map_err(err)?;
    Ok(WrittenV8CheckpointV1 {
        pin: PinnedFileV1 {
            path: output.to_path_buf(),
            sha256: sha(&child_bytes),
        },
        parent: saved,
        provenance: converted.provenance,
    })
}

/// Converts `request.parent_checkpoint` and checks the preservation; returns
/// the receipt entry (the caller's to publish).
pub fn convert_checkpoint_to_v8_inclusion_v1(
    request: &V8ConversionRequestV1,
) -> Result<Value, String> {
    ensure(
        request.schema == V8_CONVERSION_REQUEST_SCHEMA_V1,
        "unknown V8 conversion request schema",
    )?;
    let written = write_v8_checkpoint_v1(
        &request.parent_checkpoint,
        &request.output_checkpoint,
        request.seed,
    )?;
    let preservation = preservation_check_v1(request, &written.pin)?;
    Ok(json!({
        "schema": V8_CONVERSION_RECEIPT_SCHEMA_V1,
        "construction_version": V8_CONVERSION_CONSTRUCTION_VERSION_V1,
        "parent": {"path": request.parent_checkpoint.path,
                   "sha256": request.parent_checkpoint.sha256,
                   "state_sha256": written.parent.state_sha256,
                   "feature_contract_digest": written.parent.feature_contract_digest},
        "output": {"path": written.pin.path, "sha256": written.pin.sha256,
                   "state_sha256": written.provenance.child_state_sha256,
                   "feature_contract_digest":
                       crate::native_flat_tensorizer_v8::FEATURE_CONTRACT_DIGEST_V8},
        "play_import": request.play_import,
        "init_law": written.provenance.init_law,
        "seed": request.seed,
        "adam_step": written.provenance.adam_step,
        "provenance": written.provenance,
        "preservation": preservation,
    }))
}

/// [`convert_checkpoint_to_v8_inclusion_v1`] of one JSON request.
pub fn convert_checkpoint_to_v8_inclusion_json_v1(text: &str) -> Result<Value, String> {
    crate::rl::parse_strict_json_value(text).map_err(err)?;
    let request: V8ConversionRequestV1 = serde_json::from_str(text).map_err(err)?;
    convert_checkpoint_to_v8_inclusion_v1(&request)
}

fn preservation_check_v1(
    request: &V8ConversionRequestV1,
    child_pin: &PinnedFileV1,
) -> Result<Value, String> {
    let source =
        |contract: &str, encoding: &str, checkpoint: &PinnedFileV1| ExpandedModelSourceV1 {
            play_import: request.play_import.clone(),
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: contract.into(),
                expected_feature_encoding_digest: encoding.into(),
            },
            checkpoint: Some(checkpoint.clone()),
        };
    let parent_saved: ExpandedCheckpointV1 = read_pinned(&request.parent_checkpoint)?;
    let parent_source = source(
        &parent_saved.feature_contract_digest,
        &parent_saved.feature_encoding_digest,
        &request.parent_checkpoint,
    );
    let child_source = source(
        crate::native_flat_tensorizer_v8::FEATURE_CONTRACT_DIGEST_V8,
        crate::native_flat_tensorizer_v8::FEATURE_ENCODING_DIGEST_V8,
        child_pin,
    );
    let (parent_policy, _) = initialize(&parent_source)?;
    let (mut child_policy, child_state) = initialize(&child_source)?;
    ensure(
        child_policy.feature_identity_v1()
            == crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V8,
        "the converted checkpoint did not load as FlatV8",
    )?;
    let learner = ExpandedSeatBehaviorV1 {
        source: child_source.clone(),
        identity: inference_identity_v1(&child_source, &child_policy, &child_state)?,
    };
    let deck = |id: &str| -> Result<ExpandedDeckListV1, String> {
        let registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id)
            .map_err(|error| format!("{error:?}"))?;
        Ok(ExpandedDeckListV1 {
            label: id.into(),
            mainboard: registered.registered_configuration().mainboard().to_vec(),
            sideboard: registered.registered_configuration().sideboard().to_vec(),
        })
    };
    let mut rows = 0u64;
    let mut include_active_rows = 0u64;
    let mut pairing_active_rows = 0u64;
    let mut pairing_rows = 0u64;
    let mut pregame_rows = 0u64;
    let mut exposed_pregame = 0u64;
    let mut mismatches = 0u64;
    let mut games = Vec::new();
    for (index, game) in request.preservation_games.iter().enumerate() {
        let decks = [deck(&game.decks[0])?, deck(&game.decks[1])?];
        let episode = ExpandedEpisodeV1 {
            id: format!("v8-preservation-{index}-{}", game.seed),
            seed: game.seed,
            starting_player: game.starting_player,
            learner_seat: 0,
            opponent: None,
            opponent_kind: None,
            opponent_search: None,
            registered: decks.clone(),
            selected: decks,
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
            runtime_rules: None,
            reset: None,
            opening: game.london.then_some(PauperOpeningV1::LondonV1),
            mulligan_modes: game
                .london
                .then_some([PauperMulliganModeV1::Learned, PauperMulliganModeV1::Learned]),
        };
        let trajectory = collect_episode(&mut child_policy, &learner, None, &episode)?;
        ensure(
            trajectory.feature_contract_digest
                == crate::native_flat_tensorizer_v8::FEATURE_CONTRACT_DIGEST_V8,
            "a preservation game did not record the FlatV8 identity",
        )?;
        let mut game_rows = 0u64;
        let mut game_pregame_rows = 0u64;
        for row in &trajectory.decisions {
            let tensor = row.tensor.tensor();
            let pairs = &row.tensor.pairing_node_indices;
            // The child's reference forward reproduces the recorded outputs.
            let child_output = child_policy.score_replay_tensor_v1(
                FreshLineageGenerationV1::V4,
                tensor.clone(),
                pairs,
            )?;
            ensure(
                bits(&child_output.logits) == row.logits
                    && child_output.value.to_bits() == row.value,
                "the converted policy's replay differs from its collection",
            )?;
            // The parent on the same decision's FlatV7 tensors.
            let mut stripped = tensor.clone();
            stripped.action_features = tensor
                .action_features
                .chunks_exact(V8_ACTION_FEATURE_DIM_V1)
                .flat_map(|row| row[..V8_ACTION_FEATURE_DIM_V1 - 1].iter().copied())
                .collect();
            let parent_output = parent_policy.score_replay_tensor_v1(
                FreshLineageGenerationV1::V4,
                stripped,
                &[],
            )?;
            let include = tensor
                .action_features
                .chunks_exact(V8_ACTION_FEATURE_DIM_V1)
                .any(|row| row[V8_ACTION_FEATURE_DIM_V1 - 1] == 1.0);
            // Bitwise: every logit and the value carry the same f32 bits
            // (so a signed-zero change is a mismatch), all of them finite.
            let same = parent_output
                .logits
                .iter()
                .chain([&parent_output.value])
                .all(|value| value.is_finite())
                && bits(&parent_output.logits) == bits(&child_output.logits)
                && parent_output.value.to_bits() == child_output.value.to_bits();
            rows += 1;
            game_rows += 1;
            include_active_rows += u64::from(include);
            pairing_active_rows += u64::from(!pairs.is_empty());
            pairing_rows += (pairs.len() / 2) as u64;
            mismatches += u64::from(!same);
            // A London pregame row sets the FlatV7 pregame stage column.
            game_pregame_rows += u64::from(
                game.london
                    && tensor.state.get(LONDON_PREGAME_STAGE_COLUMN_V1).copied() == Some(1.0),
            );
        }
        let exposed: u64 = trajectory.opening_record.as_ref().map_or(0, |record| {
            record
                .seats
                .iter()
                .map(|seat| u64::from(seat.exposed_pregame_decisions))
                .sum()
        });
        pregame_rows += game_pregame_rows;
        exposed_pregame += exposed;
        games.push(json!({"game": game, "rows": game_rows,
            "london_pregame_rows": game_pregame_rows,
            "opening_record_exposed_pregame_decisions": exposed,
            "terminal_classification": trajectory.terminal.terminal_classification}));
    }
    ensure(rows > 0, "the preservation check scored no rows")?;
    Ok(json!({
        "definition": "the converted policy plays every game as both seats; at each decision the parent scores the same decision's FlatV7 tensors (the FlatV8 row without its include column) through its own V4-generation forward; a mismatch is any logit or the value whose f32 bits differ (to_bits, so a signed-zero change counts), or any nonfinite parent output",
        "games": games,
        "rows": rows,
        "include_active_rows": include_active_rows,
        "pairing_active_rows": pairing_active_rows,
        "pairing_relation_rows": pairing_rows,
        "london_pregame_rows": pregame_rows,
        "opening_record_exposed_pregame_decisions": exposed_pregame,
        "mismatches": mismatches,
        "preserved": mismatches == 0,
    }))
}
