//! The pool fixture's members and draws (#599 milestone 1): public, generated
//! opponent sources for the native fixture's pool cases, so the opponent
//! caches can be measured on a W5-shaped roster without any private model.
//!
//! Every member plays from the fixture's one fresh source (the shared import:
//! its descriptor, initialization manifest and parameters) with a checkpoint
//! container of its own, as a W5 pool opponent does: about 15 MB of
//! checkpoint over about 5 MB of shared import inputs. Member `n` moves one
//! weight in each of its tensors, and sets every optimizer moment, by amounts
//! fixed by `n`, so members are distinct models with distinct pins and the
//! same bytes at every generation into the same root. The generator loads
//! each member once, so every member is a valid, pinned source.
//!
//! Every scheduled slot plays the member `draw_v1` names: the SHA-256 of the
//! compact JSON array `[tag, seed root, update, slot]`, its first eight bytes
//! as a big-endian integer, modulo the member count.

use super::*;
use crate::native_expanded_training_run_v1::{
    ExpandedOpponentAssignmentV1, ExpandedRunIterationV1, NamedExpandedOpponentV1,
};

/// The draw's tag, the first element of every hashed key.
pub(crate) const DRAW_TAG_V1: &str = "mtg-kernel-pool-fixture-draw/v1";

/// The weight step of a member's moved weights: 2^-8, exact in f32.
const STEP_V1: f32 = 1.0 / 256.0;

/// Tensors whose values the train state fixes: the card embeddings'
/// padding row (its parameters and moments) and the scorer's gauge anchor
/// (`validate_owned_train_snapshot_v1`).
const CARD_EMBEDDING_V1: &str = "card_embedding.weight";
const SCORER_ANCHOR_V1: &str = "scorer.2.bias";

/// The roster id of member `member`.
pub(crate) fn member_id_v1(member: usize) -> String {
    format!("member-{member:02}")
}

/// The member that slot `slot` of update `update` plays, of `members`.
pub(crate) fn draw_v1(seed_root: &str, update: usize, slot: usize, members: usize) -> usize {
    assert!(members > 0, "a pool has at least one member");
    let key = serde_json::to_vec(&json!([DRAW_TAG_V1, seed_root, update, slot]))
        .expect("a draw key serializes");
    let digest = Sha256::digest(&key);
    let mut prefix = [0u8; 8];
    prefix.copy_from_slice(&digest[..8]);
    (u64::from_be_bytes(prefix) % members as u64) as usize
}

/// Gives every scheduled episode of `iterations` the member drawn for its
/// slot, and returns the drawn ids, update by update.
pub(crate) fn assign_draws_v1(
    iterations: &mut [ExpandedRunIterationV1],
    members: &[NamedExpandedOpponentV1],
    seed_root: &str,
) -> Vec<Vec<String>> {
    iterations
        .iter_mut()
        .enumerate()
        .map(|(update, iteration)| {
            iteration
                .episodes
                .iter_mut()
                .enumerate()
                .map(|(slot, scheduled)| {
                    let member = draw_v1(seed_root, update, slot, members.len());
                    let id = members[member].id.clone();
                    scheduled.opponent = ExpandedOpponentAssignmentV1::Fixed { id: id.clone() };
                    id
                })
                .collect()
        })
        .collect()
}

/// Member `member`'s train state: `base` with one weight of every tensor
/// but the fixed ones moved, every free moment set, and its own Adam step.
fn member_snapshot_v1(
    base: &NativePolicyValueTrainSnapshotV1,
    member: usize,
) -> NativePolicyValueTrainSnapshotV1 {
    let mut snapshot = base.clone();
    snapshot.adam_step = 1 + member as u64;
    let step = STEP_V1 * (1 + member % 4) as f32;
    for (index, parameter) in snapshot.parameters.iter_mut().enumerate() {
        if parameter.name == CARD_EMBEDDING_V1 || parameter.name == SCORER_ANCHOR_V1 {
            continue;
        }
        let at = (member * 7_919 + index * 104_729) % parameter.values.len();
        parameter.values[at] += step;
    }
    for (moments, scale) in [
        (&mut snapshot.first_moments, 1.0e-4_f32),
        (&mut snapshot.second_moments, 1.0e-8_f32),
    ] {
        for moment in moments.iter_mut() {
            let fixed = if moment.name == CARD_EMBEDDING_V1 {
                moment.shape[1]
            } else if moment.name == SCORER_ANCHOR_V1 {
                1
            } else {
                0
            };
            for (index, value) in moment.values.iter_mut().enumerate().skip(fixed) {
                *value = scale * (1 + (index + member) % 13) as f32;
            }
        }
    }
    snapshot
}

/// An ordinary fresh GAE checkpoint of `policy` holding `snapshot`, which
/// the container writer admits.
fn member_checkpoint_v1(
    policy: &FrozenPlayPolicyV1,
    snapshot: &NativePolicyValueTrainSnapshotV1,
) -> Result<ExpandedCheckpointV1, String> {
    let identity = policy.feature_identity_v1();
    let as_bits = |rows: &[NativeNamedParameterV1]| {
        rows.iter()
            .map(ParameterBitsV1::from_native)
            .collect::<Vec<_>>()
    };
    Ok(ExpandedCheckpointV1 {
        schema: ordinary_checkpoint_schema_v1(policy.identity_v1()).into(),
        feature_contract_digest: identity.feature_contract_digest.into(),
        feature_encoding_digest: identity.feature_encoding_digest.into(),
        card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        source_import: policy.identity_v1().clone(),
        state_sha256: hex(&snapshot.state_sha256_v1().map_err(err)?),
        e_supported: None,
        soft_q: None,
        search_targets: None,
        ex1: None,
        adam_step: snapshot.adam_step,
        scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
        parameters: as_bits(&snapshot.parameters),
        first_moments: as_bits(&snapshot.first_moments),
        second_moments: as_bits(&snapshot.second_moments),
        trajectories: Vec::new(),
        loss_identity: GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1.into(),
        gamma_bits: Some(1.0_f32.to_bits()),
        gae_lambda_bits: Some(0.9_f32.to_bits()),
        entropy_coefficient_bits: Some(0.0_f32.to_bits()),
        exploration: None,
        learning_rate_bits: 0.0003_f32.to_bits(),
        value_coefficient_bits: 0.5_f32.to_bits(),
        registry_transfer: None,
        archive_distillation: None,
        training_math: TrainingMathV1::PlatformLibm,
        input_contract: None,
    })
}

/// Writes `count` members, member `n` at `directory/member-NN/checkpoint.json`,
/// over `import` (the fixture's fresh source, without a checkpoint), and
/// returns them as roster entries. Every member is loaded once and must be a
/// distinct model.
pub(crate) fn write_members_v1(
    directory: &Path,
    import: &ExpandedModelSourceV1,
    count: usize,
) -> Result<Vec<NamedExpandedOpponentV1>, String> {
    ensure(
        import.checkpoint.is_none() && import.s_learner.is_none(),
        "pool members share one fresh import without a checkpoint",
    )?;
    let (policy, state) = initialize_any_math_v1(import)?;
    let base = state.snapshot_v1().map_err(err)?;
    let mut identities = vec![policy.actual_model_identity_v1()];
    let mut members = Vec::with_capacity(count);
    for member in 0..count {
        let saved = member_checkpoint_v1(&policy, &member_snapshot_v1(&base, member))?;
        checkpoint_container_v1::admit_writer_v1(&saved)?;
        let bytes = checkpoint_container_v1::encode_v1(&saved)?;
        let member_directory = directory.join(member_id_v1(member));
        fs::create_dir_all(&member_directory).map_err(err)?;
        let source = ExpandedModelSourceV1 {
            checkpoint: Some(publish_json_bytes_v1(
                &member_directory,
                "checkpoint.json",
                &bytes,
            )?),
            ..import.clone()
        };
        let (loaded, _) = load_expanded_inference_any_math_v1(&source)?;
        let identity = loaded.actual_model_identity_v1();
        ensure(
            !identities.contains(&identity),
            "a pool member repeats another member's model",
        )?;
        identities.push(identity);
        members.push(NamedExpandedOpponentV1 {
            id: member_id_v1(member),
            source,
        });
    }
    Ok(members)
}

#[cfg(test)]
mod tests;
