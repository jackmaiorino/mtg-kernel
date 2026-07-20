#![cfg_attr(test, recursion_limit = "512")]

//! Experimental, deterministic, resumable game core for a fixed Pauper pool.
//!
//! Scope: exactly the pinned nine-deck Pauper pool (150 unique roster names;
//! 132 currently registered deck cards plus required tokens). The
//! Java XMage engine remains the reference implementation and claim surface.
//! This kernel is intended to reduce rules-engine cost in training and search
//! workloads; an end-to-end training speedup over XMage has not yet been
//! established. Behavior is validated by golden-trace replay equivalence and
//! branch differential testing against the reference. Unsupported mechanics
//! FAIL CLOSED (a deck that needs them cannot be simulated here and must run on
//! the reference engine).
//!
//! Architectural invariants (Sol #84):
//! - Deterministic transitions; no unordered-map iteration anywhere.
//! - Stable arena ids for all game objects; canonical identity across snapshots.
//! - Versioned snapshot/restore boundary; v1 snapshots are full-state clones,
//!   not O(1).
//! - Strict single-session JSONL reset/step boundary; parallelism and batching
//!   are orchestrated outside the Rust process.

#[cfg(all(
    feature = "native-training-store-v2-production",
    target_os = "windows",
    not(all(
        target_env = "msvc",
        any(target_arch = "x86_64", target_arch = "aarch64"),
        not(debug_assertions)
    ))
))]
compile_error!(
    "native-training-store-v2-production requires Windows MSVC x86_64/aarch64 release without debug assertions"
);

pub mod async_flat_scored_rollout_v1;
pub mod async_flat_scored_rollout_v2;
pub mod async_rollout;
pub mod async_rollout_v2;
pub mod card_def;
// Fail-closed canonical JSON codec shared by the native training store's
// records. Schema validation remains a separate layer.
pub mod canonical_json_v1;
// Schema-neutral, no-overwrite filesystem publication building blocks for the
// future native trainer store. This module does not define record identities,
// CLI behavior, or latest-pointer semantics.
pub mod durable_publication_v1;
// Schema-neutral move-only publication building blocks. Immutable files use a
// no-replace move; replaceable pointer files use a distinct receipt type.
pub mod durable_move_publication_v2;
// Strict Python-authoritative initial-model snapshot loader for matched trials.
#[allow(dead_code)]
pub(crate) mod common_model_snapshot_v1;
pub mod effect;
pub mod engine;
pub mod event;
pub mod fast_sampler;
pub(crate) mod flat_action_contract_v2;
pub mod flat_policy_v1;
pub mod flat_policy_v2;
pub mod ids;
pub mod mana;
// Fixed-shape synthetic CPU oracle only; not a production trainer API.
#[allow(dead_code)]
pub(crate) mod native_flat_cpu_reference_v1;
// Auditable CPU inference reference for Python kernel-policy-value-net-8;
// deliberately not a production or performance backend.
#[allow(dead_code)]
pub(crate) mod native_policy_value_net_v1;
// Pure in-memory bridge from already-validated Store authorities and exact
// train-state bytes to a private, immutable native inference model.
pub mod native_checkpoint_inference_v1;
// Exact CPU loss/backward/Adam reference for terminal_reinforce_value/v3;
// deliberately not a scheduler, checkpoint format, or performance backend.
#[cfg(feature = "native-flat-tensorizer-diagnostic")]
pub mod native_flat_tensorizer_diagnostic_v1;
#[allow(dead_code)]
pub(crate) mod native_flat_tensorizer_v2;
#[allow(dead_code)]
pub(crate) mod native_full_episode_trajectory_v1;
#[allow(dead_code)]
pub mod native_opponent_sampler_v1;
#[allow(dead_code)]
pub(crate) mod native_policy_train_step_v1;
// Headerless, deterministic full model/Adam state payload codec. Store and CLI
// publication remain separate, later layers.
#[allow(dead_code)]
pub(crate) mod native_train_state_payload_v1;
#[allow(dead_code)]
pub(crate) mod native_trainer_schedule_v1;
// Opt-in in-memory wall-clock diagnostics. These types are intentionally not a
// Store, benchmark, checkpoint, or evidence schema and implement no codec.
pub mod native_training_phase_diagnostic_v1;
// In-memory native rollout -> tensor -> inference -> grouped Adam integration.
// Persistence and the external trainer/runner record boundary remain separate.
#[allow(dead_code)]
pub(crate) mod native_trainer_v1;
// Public in-process execution facade for the native trainer. This deliberately
// owns no CLI grammar, serialized record, or filesystem publication contract.
pub mod native_training_executor_v1;
// Non-persisting provenance guard for the exact admitted production build.
// The non-Windows required-feature binary contains only its parser/refusal stub
// and therefore never compiles this module or its generated constants.
#[cfg(all(
    feature = "native-training-store-v2-production",
    target_os = "windows",
    target_env = "msvc",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(debug_assertions)
))]
pub mod native_store_production_capture_v2;
// Pure update-zero sidecar/head boundary authority. Trained parent-bound
// construction remains unavailable in this first slice.
pub mod native_training_store_boundary_v2;
// Pure checkpoint-v3 authority for genesis and evidence-bound trained state.
pub mod native_training_store_checkpoint_v3;
// Pure complete-chain continuation-v2 authority. Largest-prefix partitioning
// is never claimed from one file in isolation.
pub mod native_training_store_segment_continuation_v2;
// Pure SegmentManifestV2 authority. This first slice authorizes genesis only;
// trained manifests require a later explicit parent-bound entry point.
pub mod native_training_store_segment_manifest_v2;
// Pure Episode/UpdateEvidence/UpdateGroup authority. This validates one
// complete update and advances a move-only evidence-chain context; it owns no
// continuation partitioning, filesystem, publication, receipt, or mutation.
pub mod native_training_store_update_group_v1;
// Schema-neutral checked ATOM/raw32/SHA framing shared by Store records.
#[allow(dead_code)]
pub(crate) mod native_training_store_digest_v1;
// Pure typed run/v2 record validation and deterministic digest authority.
// Capture, filesystem publication, and learning-quality claims live elsewhere.
pub mod native_training_store_run_v2;
// Private-construction persistence receipt and the strict native generation
// store boundary. The high-level publisher/read/recovery path lands here.
pub mod native_training_store_v2;
pub mod phase_profile;
pub mod policy_surface_v5;
pub(crate) mod private_physical_trajectory_core;
pub(crate) mod private_physical_trajectory_v1;
pub(crate) mod private_physical_trajectory_v2;
pub mod rl;
pub mod rl_session;
pub mod runtime_decks;
pub mod snapshot;
pub mod state;
/// Frozen committed-source-tree capture for science workload preflight and
/// postflight binding.
pub mod strict_source_tree_attestation_v1;
pub mod surface;
pub mod surface_v2;
pub mod trace;
pub mod trigger;

pub const KERNEL_VERSION: &str = "0.0.4-spike";
