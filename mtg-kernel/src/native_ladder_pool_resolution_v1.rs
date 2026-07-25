//! Pool resolution loader (Self-Play Ladder Design Contract S2, Section 3):
//! resolves one `OpponentLadderCheckpointRefV1` plus a base directory to a
//! validated [`NativeCheckpointInferenceV1`] handle, and resolves a complete
//! `OpponentLadderPoolContractV1`'s three policy-driven refs to the three
//! handles `LadderOpponentEngineV1::new_v1` needs.
//!
//! ## Field-mapping decision (the ambiguity flagged for this layer)
//!
//! `load_native_checkpoint_inference_v1` needs `(&ValidatedTrainRunV2,
//! &CheckpointManifestV3, payload: &[u8])`. An `OpponentLadderCheckpointRefV1`
//! pins five fields: `source_run_sha256`, `generation`, `checkpoint_sha256`,
//! `sidecar_sha256`, `state_sha256`. The mapping decided here, after reading
//! both sides:
//!
//! - `source_run_sha256` is NOT a standalone file digest to check before
//!   parsing: there is no separate "run.json digest" field anywhere in the
//!   ref, and `run.json`'s own integrity is exactly its recomputed
//!   `run_sha256` (`decode_train_run_v2` already gives every caller this
//!   self-verifying property: tampered bytes recompute a different hash).
//!   This loader reads `run.json` from the base directory, decodes it, and
//!   cross-checks `run.run_sha256() == ref.source_run_sha256`, failing closed
//!   on mismatch.
//! - `generation` selects which Store boundary to load.
//! - `checkpoint_sha256` / `sidecar_sha256` / `state_sha256` map 1:1 onto the
//!   three physical files the Store layout module names for that generation
//!   (`checkpoints/update-{gen:08}.checkpoint.json`, `.sidecar.json`,
//!   `.state.f32le`; see `native_training_store_layout_v2`). This loader
//!   reads exactly those three named files and hashes them BEFORE parsing
//!   any content, failing closed with the exact (expected, actual) digest
//!   pair on any mismatch.
//!
//! A checkpoint at generation 0 decodes directly from its own manifest bytes
//! plus payload (`decode_genesis_checkpoint_manifest_v3`; no further
//! authority is needed). A checkpoint at any nonzero generation cannot be
//! validated from those three files alone: `CheckpointManifestV3`'s trained
//! variant requires an `UpdateEvidenceChainContextV1`, which is a hash chain
//! rooted at genesis (this is the Store's whole tamper-evidence design, not
//! an accident of this layer -- a checkpoint's declared progress/counters are
//! meaningless without proof they came from a genuine appended chain). So for
//! `generation > 0` this loader treats `base_dir` as a validated Store ROOT
//! for that ref's source run (containing `run.json` plus the
//! `checkpoints/heads/refs/segments` subdirectories and the persistent lock
//! leaf) and reuses the existing validated resume-walk entry point
//! (`load_native_training_boundary_v2`) to build the fully chain-proven
//! `CheckpointManifestV3`, rather than re-deriving that proof here. The
//! upfront three-file digest gate still runs first and independently; the
//! walk is additional proof, not a replacement for it.
//!
//! Consequence for the pilot runner (Deliverable 3): each pool member's base
//! directory must be a complete Store root, not three loose files, because a
//! non-genesis checkpoint is structurally unverifiable any other way. This is
//! documented explicitly because it reads as a stronger requirement than "a
//! directory holding the three checkpoint files" might suggest at a glance.

use crate::native_checkpoint_inference_v1::{
    load_native_checkpoint_inference_v1, NativeCheckpointInferenceErrorV1,
    NativeCheckpointInferenceV1,
};
use crate::native_training_store_checkpoint_v3::{
    decode_genesis_checkpoint_manifest_v3, CheckpointManifestV3, CheckpointManifestV3Error,
};
use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, sha256_v1};
use crate::native_training_store_layout_v2::NativeTrainingStoreFinalNameV2;
use crate::native_training_store_resume_v2::{
    load_native_training_boundary_v2, LoadedNativeTrainingBoundaryV2,
    NativeTrainingStoreResumeV2Error,
};
use crate::native_training_store_root_v2::{
    NativeTrainingStoreRootV2Error, ValidatedNativeTrainingStoreRootV2,
};
use crate::native_training_store_run_v2::{
    decode_train_run_v2, OpponentLadderCheckpointRefV1, OpponentLadderPoolContractV1,
    TrainRunV2Error, ValidatedTrainRunV2,
};
use core::fmt;
use std::path::Path;

/// Which of the three pinned artifacts a digest-gate failure names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LadderPoolResolutionFileKindV1 {
    Checkpoint,
    Sidecar,
    StatePayload,
}

impl fmt::Display for LadderPoolResolutionFileKindV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Checkpoint => "checkpoint.json",
            Self::Sidecar => "sidecar.json",
            Self::StatePayload => "state.f32le",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LadderPoolResolutionErrorV1 {
    /// Reading the named file failed (missing, unreadable, or too large to
    /// be a plausible checkpoint artifact).
    FileIo {
        file: LadderPoolResolutionFileKindV1,
    },
    /// The named file's SHA-256 does not match the ref's pinned digest.
    /// Carries the exact (expected, actual) pair per the contract's fixture
    /// requirement.
    DigestMismatch {
        file: LadderPoolResolutionFileKindV1,
        expected: String,
        actual: String,
    },
    /// `run.json` could not be read from the base directory.
    RunFileIo,
    /// `run.json` failed record validation.
    RunDecode,
    /// The decoded run's `run_sha256` does not match `ref.source_run_sha256`.
    RunIdentityMismatch { expected: String, actual: String },
    /// Opening the Store root for a nonzero-generation walk failed.
    StoreRootOpen,
    /// The validated Store walk to the requested generation failed.
    StoreWalk,
    /// Decoding the genesis (generation 0) checkpoint manifest failed.
    GenesisDecode,
    /// `load_native_checkpoint_inference_v1` rejected the validated
    /// authority.
    Inference,
}

impl fmt::Display for LadderPoolResolutionErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileIo { file } => write!(formatter, "could not read ladder pool file {file}"),
            Self::DigestMismatch {
                file,
                expected,
                actual,
            } => write!(
                formatter,
                "ladder pool file {file} digest mismatch: expected {expected}, actual {actual}"
            ),
            Self::RunFileIo => formatter.write_str("could not read run.json"),
            Self::RunDecode => formatter.write_str("run.json failed record validation"),
            Self::RunIdentityMismatch { expected, actual } => write!(
                formatter,
                "run_sha256 mismatch: pool ref expected {expected}, run.json is {actual}"
            ),
            Self::StoreRootOpen => formatter.write_str("could not open the Store root"),
            Self::StoreWalk => {
                formatter.write_str("validated Store walk to the requested generation failed")
            }
            Self::GenesisDecode => formatter.write_str("genesis checkpoint manifest decode failed"),
            Self::Inference => {
                formatter.write_str("load_native_checkpoint_inference_v1 rejected the authority")
            }
        }
    }
}

impl std::error::Error for LadderPoolResolutionErrorV1 {}

impl From<TrainRunV2Error> for LadderPoolResolutionErrorV1 {
    fn from(_: TrainRunV2Error) -> Self {
        Self::RunDecode
    }
}

impl From<CheckpointManifestV3Error> for LadderPoolResolutionErrorV1 {
    fn from(_: CheckpointManifestV3Error) -> Self {
        Self::GenesisDecode
    }
}

impl From<NativeTrainingStoreRootV2Error> for LadderPoolResolutionErrorV1 {
    fn from(_: NativeTrainingStoreRootV2Error) -> Self {
        Self::StoreRootOpen
    }
}

impl From<NativeTrainingStoreResumeV2Error> for LadderPoolResolutionErrorV1 {
    fn from(_: NativeTrainingStoreResumeV2Error) -> Self {
        Self::StoreWalk
    }
}

impl From<NativeCheckpointInferenceErrorV1> for LadderPoolResolutionErrorV1 {
    fn from(_: NativeCheckpointInferenceErrorV1) -> Self {
        Self::Inference
    }
}

/// The validated (run, checkpoint-authority) pair a resolved ref produces,
/// before the final `load_native_checkpoint_inference_v1` step. Kept
/// separate from [`resolve_ladder_checkpoint_ref_v1`] so a caller building
/// several handles from the SAME checkpoint (as fixture (a) does, reusing one
/// real checkpoint for all three engine slots) can pay the expensive
/// validated Store walk once and call `load_native_checkpoint_inference_v1`
/// against the shared authority as many times as needed.
#[derive(Debug)]
pub(crate) struct LadderCheckpointAuthorityV1 {
    run: ValidatedTrainRunV2,
    checkpoint: LadderCheckpointManifestSourceV1,
}

#[derive(Debug)]
enum LadderCheckpointManifestSourceV1 {
    Genesis {
        checkpoint: CheckpointManifestV3,
        payload: Vec<u8>,
    },
    Trained(LoadedNativeTrainingBoundaryV2),
}

impl LadderCheckpointAuthorityV1 {
    pub(crate) fn run(&self) -> &ValidatedTrainRunV2 {
        &self.run
    }

    pub(crate) fn checkpoint(&self) -> &CheckpointManifestV3 {
        match &self.checkpoint {
            LadderCheckpointManifestSourceV1::Genesis { checkpoint, .. } => checkpoint,
            LadderCheckpointManifestSourceV1::Trained(boundary) => boundary.checkpoint(),
        }
    }

    pub(crate) fn payload(&self) -> &[u8] {
        match &self.checkpoint {
            LadderCheckpointManifestSourceV1::Genesis { payload, .. } => payload,
            LadderCheckpointManifestSourceV1::Trained(boundary) => boundary.payload(),
        }
    }

    /// Loads a fresh, independent [`NativeCheckpointInferenceV1`] handle from
    /// this already-validated authority. Cheap relative to
    /// [`resolve_ladder_checkpoint_authority_v1`]: no filesystem walk, just
    /// the model construction `load_native_checkpoint_inference_v1` already
    /// does. Callable more than once to build several independent handles
    /// from the same on-disk checkpoint (`NativeCheckpointInferenceV1` is
    /// move-only and not `Clone` by design).
    pub(crate) fn load_handle_v1(
        &self,
    ) -> Result<NativeCheckpointInferenceV1, LadderPoolResolutionErrorV1> {
        Ok(load_native_checkpoint_inference_v1(
            &self.run,
            self.checkpoint(),
            self.payload(),
        )?)
    }
}

fn store_directory_basename_v1(final_name: NativeTrainingStoreFinalNameV2) -> &'static str {
    final_name
        .directory()
        .basename()
        .expect("checkpoint/sidecar/state-payload final names always live under a subdirectory")
}

/// Reads one named artifact under `base_dir`, with no digest check -- the
/// shared file-location logic behind both [`read_and_gate_v1`] (verify) and
/// [`stage_ladder_checkpoint_ref_v1`] (compute, the opposite direction).
fn read_named_artifact_bytes_v1(
    base_dir: &Path,
    final_name: NativeTrainingStoreFinalNameV2,
    kind: LadderPoolResolutionFileKindV1,
) -> Result<Vec<u8>, LadderPoolResolutionErrorV1> {
    let basename = final_name
        .final_basename()
        .map_err(|_| LadderPoolResolutionErrorV1::FileIo { file: kind })?;
    let path = base_dir
        .join(store_directory_basename_v1(final_name))
        .join(basename);
    std::fs::read(&path).map_err(|_| LadderPoolResolutionErrorV1::FileIo { file: kind })
}

/// Reads one named artifact under `base_dir`, hashes it, and fails closed if
/// the hash does not match `expected_hex` -- BEFORE any parsing of the bytes.
fn read_and_gate_v1(
    base_dir: &Path,
    final_name: NativeTrainingStoreFinalNameV2,
    kind: LadderPoolResolutionFileKindV1,
    expected_hex: &str,
) -> Result<Vec<u8>, LadderPoolResolutionErrorV1> {
    let bytes = read_named_artifact_bytes_v1(base_dir, final_name, kind)?;
    let actual = lower_hex_raw32_v1(sha256_v1(&bytes));
    if actual != expected_hex {
        return Err(LadderPoolResolutionErrorV1::DigestMismatch {
            file: kind,
            expected: expected_hex.to_owned(),
            actual,
        });
    }
    Ok(bytes)
}

/// Computes an [`OpponentLadderCheckpointRefV1`]-shaped digest set for a
/// checkpoint at `generation` under `base_dir`, entirely from the files on
/// disk (the source run identity from `run.json`'s own `run_sha256`, plus
/// the three pinned artifact digests) -- the same digest computation
/// [`resolve_ladder_checkpoint_authority_v1`] re-validates at load time, run
/// here in the OPPOSITE direction (compute, not check). This is a STAGING
/// helper for harnesses that need to mint a ref (a pool member, or Amendment
/// 1 / Section 8A point 2's continual-initialization section) from a
/// checkpoint that already exists on disk, without hand-maintaining a JSON
/// file of pre-computed digests. The returned struct's fields map 1:1 onto
/// [`crate::native_training_store_run_v2::OpponentLadderInitializationContractV1`]
/// (identical shape); callers pick whichever wrapper their record section
/// needs.
pub(crate) fn stage_ladder_checkpoint_ref_v1(
    base_dir: &Path,
    generation: u64,
) -> Result<OpponentLadderCheckpointRefV1, LadderPoolResolutionErrorV1> {
    let run_bytes = std::fs::read(base_dir.join("run.json"))
        .map_err(|_| LadderPoolResolutionErrorV1::RunFileIo)?;
    let run = decode_train_run_v2(&run_bytes)?;
    let checkpoint_bytes = read_named_artifact_bytes_v1(
        base_dir,
        NativeTrainingStoreFinalNameV2::CheckpointManifest {
            generation_index: generation,
        },
        LadderPoolResolutionFileKindV1::Checkpoint,
    )?;
    let sidecar_bytes = read_named_artifact_bytes_v1(
        base_dir,
        NativeTrainingStoreFinalNameV2::CheckpointSidecar {
            generation_index: generation,
        },
        LadderPoolResolutionFileKindV1::Sidecar,
    )?;
    let state_bytes = read_named_artifact_bytes_v1(
        base_dir,
        NativeTrainingStoreFinalNameV2::StatePayload {
            generation_index: generation,
        },
        LadderPoolResolutionFileKindV1::StatePayload,
    )?;
    Ok(OpponentLadderCheckpointRefV1 {
        source_run_sha256: run.run_sha256().to_owned(),
        generation,
        checkpoint_sha256: lower_hex_raw32_v1(sha256_v1(&checkpoint_bytes)),
        sidecar_sha256: lower_hex_raw32_v1(sha256_v1(&sidecar_bytes)),
        state_sha256: lower_hex_raw32_v1(sha256_v1(&state_bytes)),
    })
}

/// Resolves one checkpoint ref to a validated, chain-proven authority. See
/// the module docs for the complete field-mapping decision.
pub(crate) fn resolve_ladder_checkpoint_authority_v1(
    base_dir: &Path,
    checkpoint_ref: &OpponentLadderCheckpointRefV1,
) -> Result<LadderCheckpointAuthorityV1, LadderPoolResolutionErrorV1> {
    // 1. Fast, independent digest gate on exactly the three pinned files,
    //    strictly before any parsing.
    let checkpoint_bytes = read_and_gate_v1(
        base_dir,
        NativeTrainingStoreFinalNameV2::CheckpointManifest {
            generation_index: checkpoint_ref.generation,
        },
        LadderPoolResolutionFileKindV1::Checkpoint,
        &checkpoint_ref.checkpoint_sha256,
    )?;
    let _sidecar_bytes = read_and_gate_v1(
        base_dir,
        NativeTrainingStoreFinalNameV2::CheckpointSidecar {
            generation_index: checkpoint_ref.generation,
        },
        LadderPoolResolutionFileKindV1::Sidecar,
        &checkpoint_ref.sidecar_sha256,
    )?;
    let state_bytes = read_and_gate_v1(
        base_dir,
        NativeTrainingStoreFinalNameV2::StatePayload {
            generation_index: checkpoint_ref.generation,
        },
        LadderPoolResolutionFileKindV1::StatePayload,
        &checkpoint_ref.state_sha256,
    )?;

    // 2. run.json: integrity is its own recomputed run_sha256, cross-checked
    //    against the ref's pinned source_run_sha256.
    let run_bytes = std::fs::read(base_dir.join("run.json"))
        .map_err(|_| LadderPoolResolutionErrorV1::RunFileIo)?;
    let run = decode_train_run_v2(&run_bytes)?;
    if run.run_sha256() != checkpoint_ref.source_run_sha256 {
        return Err(LadderPoolResolutionErrorV1::RunIdentityMismatch {
            expected: checkpoint_ref.source_run_sha256.clone(),
            actual: run.run_sha256().to_owned(),
        });
    }

    // 3. The validated checkpoint authority: direct genesis decode, or the
    //    complete chain-proven walk for any nonzero generation.
    let checkpoint = if checkpoint_ref.generation == 0 {
        let checkpoint =
            decode_genesis_checkpoint_manifest_v3(&checkpoint_bytes, &state_bytes, &run)?;
        LadderCheckpointManifestSourceV1::Genesis {
            checkpoint,
            payload: state_bytes,
        }
    } else {
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(base_dir)?;
        let boundary = load_native_training_boundary_v2(&root, &run, checkpoint_ref.generation)?;
        LadderCheckpointManifestSourceV1::Trained(boundary)
    };

    Ok(LadderCheckpointAuthorityV1 { run, checkpoint })
}

/// Resolves one checkpoint ref straight to a validated
/// [`NativeCheckpointInferenceV1`] handle. See
/// [`resolve_ladder_checkpoint_authority_v1`] to reuse the validated
/// authority for building several handles from the same checkpoint cheaply.
pub(crate) fn resolve_ladder_checkpoint_ref_v1(
    base_dir: &Path,
    checkpoint_ref: &OpponentLadderCheckpointRefV1,
) -> Result<NativeCheckpointInferenceV1, LadderPoolResolutionErrorV1> {
    resolve_ladder_checkpoint_authority_v1(base_dir, checkpoint_ref)?.load_handle_v1()
}

/// Resolves a complete `OpponentLadderPoolContractV1`'s three policy-driven
/// refs to the three validated handles `LadderOpponentEngineV1::new_v1`
/// needs, one base directory per ref (each must be a complete Store root for
/// that ref's source run; see the module docs).
pub(crate) fn resolve_ladder_pool_v1(
    pool: &OpponentLadderPoolContractV1,
    primary_base_dir: &Path,
    predecessor_a_base_dir: &Path,
    predecessor_b_base_dir: &Path,
) -> Result<
    (
        NativeCheckpointInferenceV1,
        NativeCheckpointInferenceV1,
        NativeCheckpointInferenceV1,
    ),
    LadderPoolResolutionErrorV1,
> {
    let primary = resolve_ladder_checkpoint_ref_v1(primary_base_dir, &pool.primary)?;
    let predecessor_a =
        resolve_ladder_checkpoint_ref_v1(predecessor_a_base_dir, &pool.predecessor_a)?;
    let predecessor_b =
        resolve_ladder_checkpoint_ref_v1(predecessor_b_base_dir, &pool.predecessor_b)?;
    Ok((primary, predecessor_a, predecessor_b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flat_policy_v2::{
        FlatGlobalsV2, FlatRelativePlayerV2, FlatScorerActionCoreV2, FlatScoringDecisionViewV2,
    };
    use crate::native_ladder_opponent_v1::LadderOpponentEngineV1;
    use crate::native_opponent_policy_v2::{
        FROZEN_CHECKPOINT_OPPONENT_POLICY_SAMPLING_RULE_V2, OPPONENT_LADDER_POOL_IDENTITY_V2,
        OPPONENT_LADDER_POOL_SIZE_V2, OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2,
        OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2, OPPONENT_LADDER_POOL_WEIGHT_PRIMARY_V2,
        OPPONENT_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2,
    };
    use crate::native_opponent_sampler_v1::{
        NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_IDENTITY_V1,
        NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_MODEL_RULE_V1,
        UNIFORM_INDEX_MODULO_U64_ALGORITHM_V1, UNIFORM_INDEX_MODULO_U64_IDENTITY_V1,
    };
    use crate::native_trainer_schedule_v2::OpponentLadderPoolMemberV2;
    use crate::native_training_store_run_v2::{OpponentLadderUniformFloorV1, ValidatedTrainRunV2};
    use std::fs;

    // ------------------------------------------------------------------
    // Digest-gate unit tests: a small on-disk fixture with independently
    // known bytes/hashes, no real Store involved.
    // ------------------------------------------------------------------

    fn temp_dir_v1(label: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static ORDINAL: AtomicU64 = AtomicU64::new(0);
        let ordinal = ORDINAL.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mtg-kernel-ladder-pool-resolution-v1-{}-{label}-{ordinal}",
            std::process::id()
        ));
        fs::create_dir_all(path.join("checkpoints")).unwrap();
        path
    }

    fn write_gate_fixture_v1(
        dir: &std::path::Path,
        generation: u64,
    ) -> OpponentLadderCheckpointRefV1 {
        let checkpoint_bytes = b"checkpoint-fixture-bytes".to_vec();
        let sidecar_bytes = b"sidecar-fixture-bytes".to_vec();
        let state_bytes = b"state-fixture-bytes".to_vec();
        fs::write(
            dir.join("checkpoints").join(
                NativeTrainingStoreFinalNameV2::CheckpointManifest {
                    generation_index: generation,
                }
                .final_basename()
                .unwrap(),
            ),
            &checkpoint_bytes,
        )
        .unwrap();
        fs::write(
            dir.join("checkpoints").join(
                NativeTrainingStoreFinalNameV2::CheckpointSidecar {
                    generation_index: generation,
                }
                .final_basename()
                .unwrap(),
            ),
            &sidecar_bytes,
        )
        .unwrap();
        fs::write(
            dir.join("checkpoints").join(
                NativeTrainingStoreFinalNameV2::StatePayload {
                    generation_index: generation,
                }
                .final_basename()
                .unwrap(),
            ),
            &state_bytes,
        )
        .unwrap();
        OpponentLadderCheckpointRefV1 {
            source_run_sha256: "a".repeat(64),
            generation,
            checkpoint_sha256: lower_hex_raw32_v1(sha256_v1(&checkpoint_bytes)),
            sidecar_sha256: lower_hex_raw32_v1(sha256_v1(&sidecar_bytes)),
            state_sha256: lower_hex_raw32_v1(sha256_v1(&state_bytes)),
        }
    }

    #[test]
    fn digest_gate_accepts_matching_files_and_fails_before_run_json_is_even_read() {
        let dir = temp_dir_v1("gate-positive");
        let ok_ref = write_gate_fixture_v1(&dir, 4);
        // No run.json exists in this fixture directory. If the gate ran
        // AFTER attempting to read run.json, this would surface RunFileIo
        // instead of the file/digest checks below; asserting the specific
        // FileIo/DigestMismatch variants proves the gate runs first.
        let checkpoint_bytes = fs::read(
            dir.join("checkpoints").join(
                NativeTrainingStoreFinalNameV2::CheckpointManifest {
                    generation_index: 4,
                }
                .final_basename()
                .unwrap(),
            ),
        )
        .unwrap();
        assert_eq!(
            read_and_gate_v1(
                &dir,
                NativeTrainingStoreFinalNameV2::CheckpointManifest {
                    generation_index: 4
                },
                LadderPoolResolutionFileKindV1::Checkpoint,
                &ok_ref.checkpoint_sha256,
            )
            .unwrap(),
            checkpoint_bytes
        );

        // Corrupt digest -> fail closed with the exact expected/actual pair,
        // never touching run.json.
        let wrong_digest = "0".repeat(64);
        let error = read_and_gate_v1(
            &dir,
            NativeTrainingStoreFinalNameV2::CheckpointManifest {
                generation_index: 4,
            },
            LadderPoolResolutionFileKindV1::Checkpoint,
            &wrong_digest,
        )
        .unwrap_err();
        match error {
            LadderPoolResolutionErrorV1::DigestMismatch {
                file: LadderPoolResolutionFileKindV1::Checkpoint,
                expected,
                actual,
            } => {
                assert_eq!(expected, wrong_digest);
                assert_eq!(actual, ok_ref.checkpoint_sha256);
                assert_ne!(expected, actual);
            }
            other => panic!("expected DigestMismatch, got {other:?}"),
        }

        // The full resolve path also fails closed on a corrupted sidecar
        // digest, again before run.json ever needs to exist/parse (this
        // directory has none).
        let mut corrupted_ref = ok_ref.clone();
        corrupted_ref.sidecar_sha256 = "1".repeat(64);
        let error = resolve_ladder_checkpoint_authority_v1(&dir, &corrupted_ref).unwrap_err();
        assert!(matches!(
            error,
            LadderPoolResolutionErrorV1::DigestMismatch {
                file: LadderPoolResolutionFileKindV1::Sidecar,
                ..
            }
        ));

        let mut corrupted_state = ok_ref.clone();
        corrupted_state.state_sha256 = "2".repeat(64);
        let error = resolve_ladder_checkpoint_authority_v1(&dir, &corrupted_state).unwrap_err();
        assert!(matches!(
            error,
            LadderPoolResolutionErrorV1::DigestMismatch {
                file: LadderPoolResolutionFileKindV1::StatePayload,
                ..
            }
        ));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_fails_closed_with_file_io() {
        let dir = temp_dir_v1("gate-missing");
        let checkpoint_ref = OpponentLadderCheckpointRefV1 {
            source_run_sha256: "a".repeat(64),
            generation: 0,
            checkpoint_sha256: "b".repeat(64),
            sidecar_sha256: "c".repeat(64),
            state_sha256: "d".repeat(64),
        };
        let error = resolve_ladder_checkpoint_authority_v1(&dir, &checkpoint_ref).unwrap_err();
        assert!(matches!(
            error,
            LadderPoolResolutionErrorV1::FileIo {
                file: LadderPoolResolutionFileKindV1::Checkpoint
            }
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_identity_mismatch_fails_closed_after_the_file_gate_passes() {
        let dir = temp_dir_v1("gate-run-mismatch");
        let mut checkpoint_ref = write_gate_fixture_v1(&dir, 0);
        // Give it a real, decodable run.json (the K2/S4 fixture) whose
        // actual run_sha256 will NOT match the ref's fabricated
        // source_run_sha256 ("a" * 64), forcing a RunIdentityMismatch
        // rather than a RunFileIo/RunDecode failure.
        let run_bytes = crate::native_training_store_run_v2::test_fixture_bytes_v2();
        fs::write(dir.join("run.json"), &run_bytes).unwrap();
        let run = decode_train_run_v2(&run_bytes).unwrap();
        assert_ne!(run.run_sha256(), checkpoint_ref.source_run_sha256);
        checkpoint_ref.source_run_sha256 = "f".repeat(64);

        let error = resolve_ladder_checkpoint_authority_v1(&dir, &checkpoint_ref).unwrap_err();
        match error {
            LadderPoolResolutionErrorV1::RunIdentityMismatch { expected, actual } => {
                assert_eq!(expected, "f".repeat(64));
                assert_eq!(actual, run.run_sha256());
            }
            other => panic!("expected RunIdentityMismatch, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    // ------------------------------------------------------------------
    // Fixture (a): real-checkpoint end-to-end (Section 7 fixture family a).
    // ------------------------------------------------------------------

    /// Real, already-published S1 checkpoint (read-only). PRIMARY(1) per
    /// contract Section 3: seed 910005, generation 256.
    const REAL_STORE_ROOT: &str = r"D:\mtg-kernel-s1-mirror-20260724\dev1\run-1\store";
    const REAL_GENERATION: u64 = 256;

    /// Also exercises [`stage_ladder_checkpoint_ref_v1`] end-to-end against a
    /// real store (Amendment 1 / Section 8A point 2 pilot harness wiring):
    /// this used to hand-roll the same read-three-files-and-hash logic
    /// inline; now it calls the shared staging helper, so this fixture's own
    /// downstream tests (the digest gate matching positively, the loader
    /// resolving to a real engine decision) double as that helper's
    /// correctness proof against real Store bytes. Pinned digests are
    /// computed by hashing the real files at test start (contract Section 7
    /// fixture (a): "compute the pinned digests for the test by hashing the
    /// real files"), so this also exercises the digest gate POSITIVELY: the
    /// ref's pins are exactly this run's real bytes, so resolution only
    /// succeeds because the gate matches.
    fn real_checkpoint_ref_v1() -> OpponentLadderCheckpointRefV1 {
        stage_ladder_checkpoint_ref_v1(std::path::Path::new(REAL_STORE_ROOT), REAL_GENERATION)
            .expect("real S1 mirror checkpoint must resolve to a validated staged ref")
    }

    /// Independent cross-check for [`stage_ladder_checkpoint_ref_v1`]
    /// (Amendment 1 / Section 8A point 2 pilot harness wiring): the
    /// computed digests must match the ladder pilot's real, already-published
    /// `pool.json` PRIMARY entry exactly -- values independently produced by
    /// a different mechanism (the pool-staging process that authored that
    /// file) rather than by this function itself, so agreement is real
    /// evidence, not tautology. Read-only real evidence, not a fixture.
    #[test]
    fn stage_ladder_checkpoint_ref_v1_matches_the_real_pilot_pool_json_primary_entry() {
        const POOL_PRIMARY_STORE_ROOT: &str = r"D:\mtg-kernel-ladder-pilot-20260725\pool\primary";
        let staged =
            stage_ladder_checkpoint_ref_v1(std::path::Path::new(POOL_PRIMARY_STORE_ROOT), 256)
                .expect("real pilot pool primary checkpoint must resolve to a staged ref");
        assert_eq!(
            staged,
            OpponentLadderCheckpointRefV1 {
                source_run_sha256:
                    "520d3b849ac3ff37ea50a0498acf335885625feaed5437539bec5c42c5896b06".to_owned(),
                generation: 256,
                checkpoint_sha256:
                    "b051f364fa69185ae9bd2bd7bcfa3a23a974742bd002efbf04c7453885ab688f".to_owned(),
                sidecar_sha256: "702bcb64164453011572e95db1a6ae1fe2b392ee83531c0d68ed07180cdf1874"
                    .to_owned(),
                state_sha256: "ddbacacc6f9108fef9c1dd9e704e4d35fbbbc37f44a74526d27b0b3fb607f6db"
                    .to_owned(),
            }
        );
    }

    fn real_ladder_pool_fixture_v1(
        checkpoint_ref: &OpponentLadderCheckpointRefV1,
    ) -> OpponentLadderPoolContractV1 {
        // Frozen pool literals plus the real checkpoint ref reused in all
        // three policy slots (Section 7 fixture (a) explicitly permits this
        // for the end-to-end test: "you may use the same handle for all
        // three policy slots").
        OpponentLadderPoolContractV1 {
            identity: OPPONENT_LADDER_POOL_IDENTITY_V2.to_owned(),
            size: OPPONENT_LADDER_POOL_SIZE_V2,
            policy_member_sampling_rule: FROZEN_CHECKPOINT_OPPONENT_POLICY_SAMPLING_RULE_V2
                .to_owned(),
            weight_primary: OPPONENT_LADDER_POOL_WEIGHT_PRIMARY_V2,
            weight_predecessor_a: OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2,
            weight_predecessor_b: OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2,
            weight_uniform_floor: OPPONENT_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2,
            primary: checkpoint_ref.clone(),
            predecessor_a: checkpoint_ref.clone(),
            predecessor_b: checkpoint_ref.clone(),
            uniform_floor: OpponentLadderUniformFloorV1 {
                identity: NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_IDENTITY_V1.to_owned(),
                model_rule: NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_MODEL_RULE_V1.to_owned(),
                sampler_identity: UNIFORM_INDEX_MODULO_U64_IDENTITY_V1.to_owned(),
                sampler_algorithm: UNIFORM_INDEX_MODULO_U64_ALGORITHM_V1.to_owned(),
            },
        }
    }

    fn decision_parts_v1() -> (FlatGlobalsV2, [FlatScorerActionCoreV2; 1]) {
        // Minimal synthetic-but-structurally-valid decision view: this is
        // the SAME construction every other test module in this crate uses
        // for FlatScoringDecisionViewV2 (native_checkpoint_inference_v1,
        // async_flat_scored_rollout_v2, native_flat_tensorizer_diagnostic_v1,
        // flat_policy_v2 and private_physical_trajectory_v2's own test
        // modules all build this exact shape via the public `new`
        // constructor). No richer PUBLIC "real decision" fixture exists
        // anywhere in the crate; every richer decision comes from a live
        // rollout's private internal state. Reusing this established
        // pattern still drives the real tensorizer -> model forward path
        // end to end for the loaded checkpoint.
        (
            FlatGlobalsV2 {
                acting_player: FlatRelativePlayerV2::SelfPlayer,
                ..FlatGlobalsV2::default()
            },
            [FlatScorerActionCoreV2::default()],
        )
    }

    fn decision_view_v1<'a>(
        globals: &'a FlatGlobalsV2,
        actions: &'a [FlatScorerActionCoreV2],
    ) -> FlatScoringDecisionViewV2<'a> {
        FlatScoringDecisionViewV2::new(
            globals,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            actions,
            &[],
        )
    }

    #[test]
    #[ignore = "requires the real S1 mirror evidence directory on D:\\; run explicitly"]
    fn real_s1_checkpoint_resolves_through_the_loader_and_drives_a_real_engine_decision() {
        let checkpoint_ref = real_checkpoint_ref_v1();

        // Exercise the full Deliverable-1 loader path once (this pays the
        // validated Store walk exactly once): the digest gate runs
        // positively (the ref's pins are this run's real bytes), then the
        // walk builds the chain-proven authority.
        let authority = resolve_ladder_checkpoint_authority_v1(
            std::path::Path::new(REAL_STORE_ROOT),
            &checkpoint_ref,
        )
        .expect("real S1 checkpoint must resolve through the loader");
        assert_eq!(authority.checkpoint().generation_index(), REAL_GENERATION);
        let run: &ValidatedTrainRunV2 = authority.run();
        assert_eq!(run.run_sha256(), checkpoint_ref.source_run_sha256);

        // Also exercise the single-call convenience entry point positively,
        // proving the whole `resolve_ladder_checkpoint_ref_v1` ->
        // `load_native_checkpoint_inference_v1` path compiles and succeeds
        // end to end (Section 7 fixture (a): "through the Deliverable-1
        // loader path").
        let _ = resolve_ladder_checkpoint_ref_v1(
            std::path::Path::new(REAL_STORE_ROOT),
            &checkpoint_ref,
        )
        .expect("single-call loader entry point must also succeed");

        // Build three independent handles from the ALREADY-VALIDATED
        // authority (cheap: no further disk walk) for the three engine
        // slots, per the fixture's explicit permission to reuse one real
        // checkpoint everywhere.
        let primary = authority.load_handle_v1().unwrap();
        let predecessor_a = authority.load_handle_v1().unwrap();
        let predecessor_b = authority.load_handle_v1().unwrap();

        let pool = real_ladder_pool_fixture_v1(&checkpoint_ref);
        let engine = LadderOpponentEngineV1::new_v1(pool, primary, predecessor_a, predecessor_b)
            .expect("frozen pool literals must match");

        // Drive select_policy_action_v1 through a real FlatScoringDecisionViewV2.
        let (globals, actions) = decision_parts_v1();
        let view = decision_view_v1(&globals, &actions);
        let action = engine
            .select_policy_action_v1(OpponentLadderPoolMemberV2::Primary, view, 12_345)
            .expect("a one-action decision must select index 0");
        assert_eq!(action, 0);

        // The engine's episode-level pool-member selection is likewise live
        // over the real pool contract.
        let member = engine.pool_member_for_episode_v1(71_501, 0).unwrap();
        assert!(matches!(
            member,
            OpponentLadderPoolMemberV2::Primary
                | OpponentLadderPoolMemberV2::PredecessorA
                | OpponentLadderPoolMemberV2::PredecessorB
                | OpponentLadderPoolMemberV2::UniformFloor
        ));
    }
}
