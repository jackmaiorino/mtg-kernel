//! Authority record schema for the model-guided searcher (IS-MCTS with a
//! PUCT-style expansion prior and a value-head leaf, per
//! `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md` Section 1). This module is
//! implementation item 4 of that design's Section 5.3: "New non-store
//! authority kind and extended record schema (Section 1.4), as a documented
//! contract. Buildable now as a schema; wiring is item 6."
//!
//! SCOPE, strictly: this module defines the record's TYPE, its SERDE shape,
//! its FAIL-CLOSED validation, and the frozen literals that pin its
//! compile-bound fields. It does not run a search, does not dispatch an
//! opponent, does not touch the science loop, Store records, or population
//! selection, and does not verify a live checkpoint hash.
//!
//! ## Discharged since the first revision (test-time-search S0)
//!
//! Two of this header's original scope disclaimers no longer hold, and are
//! corrected here rather than left standing:
//!
//! - "does not compute or verify a live quantization-contract digest [or] a
//!   live forward-determinism build digest". It does now.
//!   `LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5 (S0) requires
//!   "quantization and deterministic-build digests bound to content", and
//!   `crate::model_guided_search_contract_digests_v1` supplies exactly that
//!   from live contract behavior. `validate` compares all three digest
//!   fields against both the pinned literal and the live recomputation; the
//!   fields are no longer caller-supplied at all (see `new`).
//! - the `action_seed` bullet below, which stated that this schema
//!   deliberately freezes no allowlist because the governing document
//!   "authorizes no seed at all". The test-time-search sketch's S0/S1
//!   stages are explicitly CP7-free engineering, so an engineering-scoped
//!   allowlist authorizes nothing this design must not authorize; see
//!   `MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1`, whose own doc comment
//!   states its scope and, in the same discipline the original bullet
//!   demanded, declines to assign the formal S2/S3 blocks.
//!
//! ## Relationship to `kernel_native_search_opponent_v1`
//!
//! Design Section 1.4: "This design's record commits to all of the same
//! fields [as v1's `KernelNativeSearchAuthorityV1`] except the evaluator
//! digest, which is not applicable (there is no static evaluator on this
//! path), plus" a fixed list of new fields. Concretely:
//!
//! - `node_key_identity`, `policy_step_depth_cap`, `seed_domain`, and the
//!   tier ladder (`tier`/`transition_budget`, `KernelNativeSearchTierV1`)
//!   are reused directly from `kernel_native_search_opponent_v1`'s exported
//!   constants and type, not redefined with new literals, because Section
//!   1.1 lists per-simulation redetermination, the tree key, the tier
//!   ladder, and the seed-domain-separation formula as "unchanged, byte-for-
//!   byte, in behavior and in the exact constants that define them." Reusing
//!   the same symbols is a stronger guarantee than copying the same string:
//!   it makes divergence a compile-time impossibility rather than a
//!   copy-paste hazard.
//! - `evaluator_identity`/`evaluator_sha256` are dropped (Section 1.4: "not
//!   applicable").
//! - `authority_kind` and `algorithm_identity` take NEW, distinct values
//!   (below), so a model-guided record can never be aliased to or confused
//!   with `kernel-native-search-opponent-v1` "at any registration layer"
//!   (Section 1.4). `validate` explicitly rejects a record whose
//!   `authority_kind` or `algorithm_identity` equals v1's value, not only
//!   one that fails to equal this design's own value, so the two schemas
//!   stay mutually exclusive even under direct field tampering.
//! - `engine_commit`, `card_db_hash`, `runtime_deck_catalog_sha256`, and
//!   `private_diagnostic_identity` are "inherited verbatim from v1" (Section
//!   1.4's own closing bullet) and use the identical checks v1 uses.
//! - `action_seed` is kept as a field (Section 1.4's "all of the same
//!   fields" instruction) and is now checked against a real allowlist,
//!   `MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1`, exactly as v1 checks
//!   `KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1`. The first revision of this
//!   bullet declined to freeze one, correctly at the time: the
//!   model-guided-searcher design document authorizes no seed ("No seed is
//!   consumed or proposed by this document"), and freezing an allowlist
//!   would have fabricated an authorization it does not grant. What changed
//!   is that a later pre-registration does grant one, narrowly:
//!   `LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5's S0 ("no
//!   games") and S1 ("CP7-free") stages. The allowlist's own doc comment
//!   states that scope, and, keeping the original bullet's discipline,
//!   assigns no S2 or S3 block.
//!
//! ## New fields (Section 1.4's "plus" list)
//!
//! - Checkpoint identity: `checkpoint_store_path_or_lineage_id`,
//!   `checkpoint_generation`, `checkpoint_weight_bytes_sha256`, following
//!   the population-v2 program's own "parent identity (seed, generation,
//!   checkpoint hashes)" / "store path, generation, and checkpoint identity
//!   hashes" convention.
//! - `net_architecture_identity`: an architecture/version hash or tag (for
//!   example a Net8 family identity), required distinct from
//!   `checkpoint_weight_bytes_sha256` so a record can never silently pair
//!   mismatched architecture code with weight bytes trained under a
//!   different architecture.
//! - `puct_prior_quantization_contract_sha256`,
//!   `value_quantization_contract_sha256`,
//!   `forward_determinism_build_identity`: no longer placeholders. All
//!   three are filled by `new` from
//!   `crate::model_guided_search_contract_digests_v1`'s pinned literals and
//!   checked by `validate` against both that literal and the digest
//!   recomputed from live contract behavior. The third additionally
//!   satisfies Section 1.4's "a build/target-cpu flag digest ... pinning
//!   the exact deterministic-forward build in use" and the
//!   forward-determinism audit's own recommendation 5. See that module's
//!   header for what each digest commits to, and for why a source-file hash
//!   was rejected in favor of a behavioral one.
//! - `consumption_mode`: one of Section 2's three modes
//!   (search-at-inference, search-as-opponent, search-at-training-targets).
//!   This field is a closed Rust enum, so an unrecognized mode string is
//!   already rejected at deserialization by serde, before `validate` ever
//!   runs. `validate` does not, and must not, enforce any mode-specific gate
//!   (the mode-(b) decorrelation-integrity requirement, the mode-(c)
//!   reward-purism law, or either mode's pool-entry/pilot authorization):
//!   those are runtime/dispatch concerns belonging to design items 6-9, not
//!   to this schema.
//!
//! ## A resolved ambiguity, reported rather than silently guessed
//!
//! Section 1.4's first "plus" bullet reads: "A distinct algorithm identity
//! (for example `model-guided-searcher-v1`), never aliased to or confused
//! with `kernel-native-search-opponent-v1` at any registration layer." Read
//! literally against `KernelNativeSearchAuthorityV1`'s actual field names,
//! this is ambiguous: `model-guided-searcher-v1` (the example given) is
//! shaped exactly like v1's `authority_kind` value
//! (`kernel-native-search-opponent-v1`), not like v1's `algorithm_identity`
//! value (`deterministic-per-simulation-redeterminized-is-mcts-integer-ucb/
//! v1`, a longer, formula-descriptive string) -- yet the bullet's own prose
//! calls it "algorithm identity" and contrasts it against
//! `kernel-native-search-opponent-v1`, which is literally v1's
//! `authority_kind`, not v1's `algorithm_identity`. Item 4's own title
//! ("New non-store authority kind and extended record schema") independently
//! requires a new `authority_kind` value regardless of how this bullet is
//! read, since `authority_kind` is the field the registration-layer
//! allowlist (Section 1.4's own "Dispatch note" and the "seven registered-
//! lineage layers") actually keys on. RESOLUTION (flagged in the final
//! report for owner confirmation, not silently assumed): both fields take
//! new, distinct values here. `authority_kind` takes the design's own
//! example literally (`model-guided-searcher-v1`), matching the shape of
//! v1's `authority_kind` and item 4's title. `algorithm_identity` takes a
//! new, formula-descriptive value in v1's own shape,
//! `MODEL_GUIDED_SEARCH_ALGORITHM_V1` below. Both are plain string
//! literals, not structural commitments, so this resolution is cheaply
//! revisable at item 6 if the owner intends only one field to change.
//!
//! ## Countersign-panel follow-up: `checkpoint_generation` had no guard
//!
//! A confirmed panel finding on the first diff: `checkpoint_generation` was
//! never referenced in `validate`, was not exercised by the fresh-
//! reconstruction tamper-test suite, and had no guard beyond JSON `u64`
//! typing. Digest-coverage answer: the field IS part of the canonical
//! serialized bytes `digest`/`matches_fresh_reconstruction_v1` hash (no
//! serde skip was ever applied to it) -- but, being one of `Self::new`'s
//! reconstruction inputs, tampering it alone can never be caught by
//! `matches_fresh_reconstruction_v1` specifically, by the same designed
//! limitation that method's own doc comment already states for every other
//! input field (`tier`, `action_seed`, `checkpoint_store_path_or_lineage_id`,
//! `checkpoint_weight_bytes_sha256`, `net_architecture_identity`, both
//! quantization digests, `forward_determinism_build_identity`,
//! `consumption_mode`). The actual defect was narrower than "not in the
//! digest": unlike every one of those nine sibling input fields, this one
//! had zero compensating `validate` check of its own. Fixed by adding a
//! `checkpoint_generation > MODEL_GUIDED_SEARCH_CHECKPOINT_GENERATION_MAX_V1`
//! check to the `InvalidCheckpointIdentity` group, bounding it to 63 bits
//! (mirroring `native_training_store_run_v2.rs`'s `is_u63`/`U63_MAX` and
//! `native_training_store_checkpoint_v3.rs`'s `is_u63_v3` on
//! `generation_index`) without also requiring nonzero, since generation 0 is
//! a legitimate genesis checkpoint in this repo.

use crate::card_def::KERNEL_CARDDB_HASH;
use crate::kernel_native_search_opponent_v1::{
    KernelNativeSearchTierV1, KERNEL_NATIVE_SEARCH_ALGORITHM_V1,
    KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1, KERNEL_NATIVE_SEARCH_DEPTH_CAP_V1,
    KERNEL_NATIVE_SEARCH_NODE_KEY_V1, KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1,
};
use crate::model_guided_search_contract_digests_v1::{
    build_flag_violation_v1, forward_determinism_build_digest_v1, lower_hex_digest_v1,
    prior_quantization_contract_digest_v1, value_quantization_contract_digest_v1,
    MODEL_GUIDED_SEARCH_FORWARD_DETERMINISM_BUILD_SHA256_V1,
    MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_CONTRACT_SHA256_V1,
    MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CONTRACT_SHA256_V1,
};
use crate::runtime_decks::RUNTIME_DECK_CATALOG_FILE_SHA256;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// New non-store authority kind and its schema tag (design Section 1.4 /
/// item 4 title). Distinct from, and never equal to,
/// `KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1`; `validate` enforces this
/// directly rather than relying only on the two literals being spelled
/// differently.
pub const MODEL_GUIDED_SEARCH_AUTHORITY_SCHEMA_V1: &str = "model_guided_searcher_authority/v1";
pub const MODEL_GUIDED_SEARCH_AUTHORITY_KIND_V1: &str = "model-guided-searcher-v1";

/// Formula-descriptive algorithm identity, in v1's own shape
/// (`deterministic-per-simulation-redeterminized-is-mcts-integer-ucb/v1`),
/// extended to name this design's two new terms: the PUCT-style expansion
/// prior (Section 1.2) and the value-head leaf (Section 1.3). Distinct from,
/// and never equal to, `KERNEL_NATIVE_SEARCH_ALGORITHM_V1`; see the module
/// doc's "resolved ambiguity" note.
pub const MODEL_GUIDED_SEARCH_ALGORITHM_V1: &str =
    "deterministic-per-simulation-redeterminized-is-mcts-puct-prior-value-head-integer-ucb/v1";

/// Whole-record canonical-JSON byte-length cap: a fail-closed guard against
/// a degenerate/pathological string in one of the free-form identity
/// fields (`checkpoint_store_path_or_lineage_id`, `net_architecture_identity`).
/// Chosen generously (16 KiB) against a record whose fields are all short
/// identity strings and small integers; not a value the design pins, a
/// defensive bound in the repo's existing `RecordTooLarge` style (see
/// `native_training_store_run_v2.rs`, `native_training_store_checkpoint_v3.rs`).
pub const MODEL_GUIDED_SEARCH_AUTHORITY_MAX_BYTES_V1: usize = 16 * 1024;

/// Per-field length cap for the two free-form identity fields that are not
/// fixed-format hashes (`checkpoint_store_path_or_lineage_id`,
/// `net_architecture_identity`). Same defensive-bound rationale as
/// `MODEL_GUIDED_SEARCH_AUTHORITY_MAX_BYTES_V1`.
pub const MODEL_GUIDED_SEARCH_IDENTITY_STRING_MAX_LEN_V1: usize = 4096;

/// Structural sanity upper bound for `checkpoint_generation`, mirroring the
/// repo-wide convention for generation/index-shaped `u64` fields: bound to
/// 63 bits, not 64, so the value stays representable in a signed 64-bit
/// integer without ambiguity across every downstream consumer (JSON
/// numbers, Python interop, `i64` arithmetic). See
/// `native_training_store_run_v2.rs`'s `is_u63`/`U63_MAX` (used, among
/// other fields, on that record's own generation-shaped counters) and
/// `native_training_store_checkpoint_v3.rs`'s `is_u63_v3` applied directly
/// to `generation_index` (`CheckpointManifestV3`'s own generation field);
/// the same `(1 << 63) - 1` bound recurs in
/// `native_full_episode_trajectory_v2.rs`, `native_trainer_schedule_v2.rs`,
/// and `environment_randomization_v2`'s golden harness. Unlike
/// `is_positive_u63`, this bound does NOT also require nonzero: generation
/// 0 is a legitimate genesis checkpoint in this repo (see
/// `native_training_store_checkpoint_v3.rs`'s genesis-snapshot validation,
/// which requires `generation_index == 0`, and the self-play ladder's own
/// gen-zero tests), so `checkpoint_generation` accepts the full
/// `0..=MODEL_GUIDED_SEARCH_CHECKPOINT_GENERATION_MAX_V1` range inclusive,
/// not a strictly-positive subset of it.
pub const MODEL_GUIDED_SEARCH_CHECKPOINT_GENERATION_MAX_V1: u64 = (1_u64 << 63) - 1;

/// Launcher-owned authorized seed blocks for the model-guided
/// (test-time-search) authority, discharging the "Binding a real allowlist
/// is future work" note in this module's own header. The header's reasoning
/// stands and is why this array exists rather than a permissive nonzero
/// check: an allowlist encodes an authorization, so it must name the
/// authorization it encodes.
///
/// SCOPE: these four blocks are pre-registered for S0 ENGINEERING and S1
/// FEASIBILITY only (`LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5,
/// stages S0 and S1: "no games" and "CP7-free" respectively). They
/// authorize no CP7 panel, no S2 search-gain screen, and no formal
/// measurement of any kind. The S2 and S3 blocks are Jack's own
/// launch-parameter decision and are deliberately NOT assigned here, in the
/// same discipline `KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1`
/// documents for its own placeholder. The owner law that formal seed
/// literals live only in launcher-level code is why a formal block would
/// not belong in this array at all: this array is the registration surface
/// the launcher selects FROM by block id, exactly as
/// `KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1` already is for v1's own
/// calibration panels.
///
/// The band (3,1xx,xxx) is disjoint from v1's calibration band
/// (1,9xx,xxx) and its pool band (2,0xx,xxx), so a seed can never be
/// simultaneously authorized for two different search authorities;
/// `authorized_seed_blocks_are_disjoint_from_v1_bands_v1` asserts this
/// rather than leaving it to the eye.
///
/// DOMAIN SEPARATION. The block seed is not itself a per-decision seed. It
/// enters `ModelGuidedSearchAuthorityV1` as `action_seed`, which is part of
/// the record's canonical bytes and therefore of
/// [`ModelGuidedSearchAuthorityV1::digest`]; that digest is the first input
/// to `kernel_native_search_opponent_v1::derive_simulation_seed_v1`, which
/// then mixes in the episode id, the physical decision id, the SUBSTEP
/// index, the simulation ordinal, and the player to act under the frozen
/// `KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1` label. So exact policy-step and
/// substep domain separation is inherited verbatim from v1's formula, with
/// this design's own authority digest substituted for v1's, exactly as
/// Section 1.1 requires; nothing new is derived here.
pub const MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1: [u64; 4] =
    [3_101_001, 3_102_001, 3_103_001, 3_104_001];

/// Resolves a launcher-supplied seed BLOCK ID (an index into
/// [`MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1`]) to its seed. Returns
/// `None` for an out-of-range id, so a CLI can fail closed on an
/// unregistered block without the caller having to know the array's length.
/// Selecting by id, never by raw seed value, is what keeps an unregistered
/// literal from reaching an authority record through a command line.
pub fn authorized_seed_block_v1(block_id: usize) -> Option<u64> {
    MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1
        .get(block_id)
        .copied()
}

/// One of Section 2's three consumption modes. A closed enum: an
/// unrecognized mode string is rejected at deserialization, before
/// `validate` runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelGuidedSearchConsumptionModeV1 {
    /// Section 2.1, mode (a): search-at-inference.
    SearchAtInference,
    /// Section 2.2, mode (b): search-as-opponent.
    SearchAsOpponent,
    /// Section 2.3, mode (c): search-at-training-targets.
    SearchAtTrainingTargets,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelGuidedSearchAuthorityV1 {
    pub schema: String,
    pub authority_kind: String,
    pub algorithm_identity: String,
    pub node_key_identity: String,
    pub tier: KernelNativeSearchTierV1,
    pub transition_budget: u32,
    pub policy_step_depth_cap: u16,
    pub seed_domain: String,
    pub engine_commit: String,
    pub card_db_hash: u64,
    pub runtime_deck_catalog_sha256: String,
    pub private_diagnostic_identity: String,
    pub action_seed: u64,
    pub checkpoint_store_path_or_lineage_id: String,
    /// Bounded to `0..=MODEL_GUIDED_SEARCH_CHECKPOINT_GENERATION_MAX_V1`
    /// (`validate`'s `InvalidCheckpointIdentity` check). Zero is a
    /// legitimate value (a genesis checkpoint), so this field is
    /// deliberately NOT also required nonzero, unlike `action_seed`. See
    /// the constant's own doc comment for the convention this mirrors.
    /// This field IS part of the canonical serialized bytes `digest` and
    /// `matches_fresh_reconstruction_v1` hash (no serde skip), but, being
    /// one of `Self::new`'s reconstruction inputs, direct tampering of it
    /// alone is not independently caught by `matches_fresh_reconstruction_v1`
    /// (that method's own doc comment states this is true of every input
    /// field); the u63 bound above is this field's compensating `validate`
    /// check, exactly as every other input field has one.
    pub checkpoint_generation: u64,
    pub checkpoint_weight_bytes_sha256: String,
    pub net_architecture_identity: String,
    pub puct_prior_quantization_contract_sha256: String,
    pub value_quantization_contract_sha256: String,
    pub forward_determinism_build_identity: String,
    pub consumption_mode: ModelGuidedSearchConsumptionModeV1,
}

impl ModelGuidedSearchAuthorityV1 {
    /// Constructs a record from its caller-supplied, per-registration
    /// identity fields, filling every compile-bound/inherited field from
    /// this module's own frozen constants (or, where explicitly inherited
    /// verbatim from v1, that module's constants), then validates before
    /// returning.
    ///
    /// The eight-parameter shape is inherent to Section 1.4's record
    /// schema, not an API design choice; see the repo's existing
    /// `#[allow(clippy::too_many_arguments)]` precedent (`event.rs`,
    /// `effect.rs`, `async_flat_scored_rollout_v1.rs`/`_v2.rs`) for records
    /// and call sites with a similarly irreducible field count.
    ///
    /// The three contract-digest fields used to be caller-supplied
    /// parameters, because when this schema was written the contracts they
    /// name were "being built concurrently in a sibling worktree and are
    /// not available here" (module docs). They are available now, so they
    /// are filled from
    /// [`crate::model_guided_search_contract_digests_v1`]'s pinned,
    /// content-bound literals instead. That is strictly stronger than
    /// validating a caller-supplied value against the same literal: a
    /// record can no longer be CONSTRUCTED carrying a wrong digest, and
    /// because the three fields are no longer `new`'s inputs, they also
    /// come under [`Self::matches_fresh_reconstruction_v1`]'s tamper
    /// detection, which by that method's own documented limitation could
    /// never cover them while they were inputs.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tier: KernelNativeSearchTierV1,
        action_seed: u64,
        private_diagnostic_identity: &str,
        checkpoint_store_path_or_lineage_id: &str,
        checkpoint_generation: u64,
        checkpoint_weight_bytes_sha256: &str,
        net_architecture_identity: &str,
        consumption_mode: ModelGuidedSearchConsumptionModeV1,
    ) -> Result<Self, ModelGuidedSearchAuthorityError> {
        let record = Self {
            schema: MODEL_GUIDED_SEARCH_AUTHORITY_SCHEMA_V1.to_string(),
            authority_kind: MODEL_GUIDED_SEARCH_AUTHORITY_KIND_V1.to_string(),
            algorithm_identity: MODEL_GUIDED_SEARCH_ALGORITHM_V1.to_string(),
            node_key_identity: KERNEL_NATIVE_SEARCH_NODE_KEY_V1.to_string(),
            tier,
            transition_budget: tier.transition_budget(),
            policy_step_depth_cap: KERNEL_NATIVE_SEARCH_DEPTH_CAP_V1,
            seed_domain: KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1.to_string(),
            engine_commit: env!("MTG_KERNEL_BUILD_GIT_HEAD").to_string(),
            card_db_hash: KERNEL_CARDDB_HASH,
            runtime_deck_catalog_sha256: RUNTIME_DECK_CATALOG_FILE_SHA256.to_string(),
            private_diagnostic_identity: private_diagnostic_identity.to_string(),
            action_seed,
            checkpoint_store_path_or_lineage_id: checkpoint_store_path_or_lineage_id.to_string(),
            checkpoint_generation,
            checkpoint_weight_bytes_sha256: checkpoint_weight_bytes_sha256.to_string(),
            net_architecture_identity: net_architecture_identity.to_string(),
            puct_prior_quantization_contract_sha256:
                MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_CONTRACT_SHA256_V1.to_string(),
            value_quantization_contract_sha256:
                MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CONTRACT_SHA256_V1.to_string(),
            forward_determinism_build_identity:
                MODEL_GUIDED_SEARCH_FORWARD_DETERMINISM_BUILD_SHA256_V1.to_string(),
            consumption_mode,
        };
        record.validate()?;
        Ok(record)
    }

    /// Fail-closed structural and consistency validation. Every check is
    /// grouped under the most specific `ModelGuidedSearchAuthorityErrorKind`
    /// that names the field cluster it covers; the grouping mirrors Section
    /// 1.4's own bullet structure (one bullet, one kind) wherever the design
    /// text groups fields together (for example the "Engine commit, CardDB
    /// hash, runtime-deck catalog hash, and private diagnostic identity,
    /// inherited verbatim from v1" bullet maps to one `InvalidProvenance`
    /// kind covering all four fields).
    pub fn validate(&self) -> Result<(), ModelGuidedSearchAuthorityError> {
        use ModelGuidedSearchAuthorityErrorKind as Kind;

        let record_bytes = serde_json::to_vec(self)
            .map_err(|_| ModelGuidedSearchAuthorityError::new(Kind::RecordTooLarge))?;
        if record_bytes.len() > MODEL_GUIDED_SEARCH_AUTHORITY_MAX_BYTES_V1 {
            return Err(ModelGuidedSearchAuthorityError::new(Kind::RecordTooLarge));
        }

        if self.schema != MODEL_GUIDED_SEARCH_AUTHORITY_SCHEMA_V1
            || self.authority_kind != MODEL_GUIDED_SEARCH_AUTHORITY_KIND_V1
            || self.authority_kind == KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1
            || self.algorithm_identity != MODEL_GUIDED_SEARCH_ALGORITHM_V1
            || self.algorithm_identity == KERNEL_NATIVE_SEARCH_ALGORITHM_V1
        {
            return Err(ModelGuidedSearchAuthorityError::new(Kind::InvalidSchema));
        }

        if self.node_key_identity != KERNEL_NATIVE_SEARCH_NODE_KEY_V1 {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidNodeKeyIdentity,
            ));
        }

        if self.transition_budget != self.tier.transition_budget()
            || self.policy_step_depth_cap != KERNEL_NATIVE_SEARCH_DEPTH_CAP_V1
        {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidSearchLadder,
            ));
        }

        if self.seed_domain != KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1 {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidSeedDomain,
            ));
        }

        let valid_diagnostic = self.private_diagnostic_identity
            == crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM
            || self.private_diagnostic_identity
                == crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM_ENVIRONMENT_V2;
        if self.engine_commit != env!("MTG_KERNEL_BUILD_GIT_HEAD")
            || !is_lower_hex_v1(&self.engine_commit, 40)
            || self.card_db_hash != KERNEL_CARDDB_HASH
            || self.runtime_deck_catalog_sha256 != RUNTIME_DECK_CATALOG_FILE_SHA256
            || !valid_diagnostic
        {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidProvenance,
            ));
        }

        // Was: `action_seed == 0`, the "minimal fail-closed structural
        // guard available without inventing authorization" this module's
        // header describes. The authorization now exists
        // (`MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1`, S0/S1 scope
        // only), so the guard becomes the allowlist membership check v1
        // already applies to its own seeds. Re-run at every call site that
        // accepts an authority (construction, digest, and the start of
        // every action selection), the same temporal re-verification v1's
        // own allowlist doc describes.
        if !MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1.contains(&self.action_seed) {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidActionSeed,
            ));
        }

        if self.checkpoint_store_path_or_lineage_id.is_empty()
            || self.checkpoint_store_path_or_lineage_id.len()
                > MODEL_GUIDED_SEARCH_IDENTITY_STRING_MAX_LEN_V1
            || !is_lower_hex_v1(&self.checkpoint_weight_bytes_sha256, 64)
            || self.checkpoint_generation > MODEL_GUIDED_SEARCH_CHECKPOINT_GENERATION_MAX_V1
        {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidCheckpointIdentity,
            ));
        }

        if self.net_architecture_identity.is_empty()
            || self.net_architecture_identity.len() > MODEL_GUIDED_SEARCH_IDENTITY_STRING_MAX_LEN_V1
            || self.net_architecture_identity == self.checkpoint_weight_bytes_sha256
        {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidNetArchitectureIdentity,
            ));
        }

        // CONTENT-BOUND, no longer structural. Each field must equal both
        // the pinned literal AND the digest recomputed from live contract
        // content this process. The pinned literal alone would catch a
        // record minted against a different contract; the live
        // recomputation additionally catches a BUILD whose contract
        // behavior has drifted away from the literal it still ships,
        // which is the failure a structural lower-hex check could never
        // see. Recomputation is `OnceLock`-memoized in the digest module,
        // so this stays a pointer comparison plus a string compare on the
        // per-decision path.
        if self.puct_prior_quantization_contract_sha256
            != MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_CONTRACT_SHA256_V1
            || self.puct_prior_quantization_contract_sha256
                != lower_hex_digest_v1(prior_quantization_contract_digest_v1())
        {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidPuctPriorQuantizationContractDigest,
            ));
        }

        if self.value_quantization_contract_sha256
            != MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CONTRACT_SHA256_V1
            || self.value_quantization_contract_sha256
                != lower_hex_digest_v1(value_quantization_contract_digest_v1())
        {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidValueQuantizationContractDigest,
            ));
        }

        if self.forward_determinism_build_identity
            != MODEL_GUIDED_SEARCH_FORWARD_DETERMINISM_BUILD_SHA256_V1
            || self.forward_determinism_build_identity
                != lower_hex_digest_v1(forward_determinism_build_digest_v1())
        {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidForwardDeterminismBuildIdentity,
            ));
        }

        // The pinned build identity above commits to the CONTRACT; this
        // commits to the contract having actually been honoured. A build
        // under `RUSTFLAGS=-C llvm-args=-fp-contract=fast` still matches
        // every pinned literal (the literals describe source-level
        // behavior and target features, neither of which such a flag
        // changes) while the arithmetic underneath them may differ. The
        // audit's Section 6 item 2 names exactly this escape hatch, so
        // the authority refuses to exist rather than certify a build it
        // cannot vouch for.
        if build_flag_violation_v1().is_some() {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::ForbiddenBuildFlagOverride,
            ));
        }

        Ok(())
    }

    pub fn digest(&self) -> Result<[u8; 32], ModelGuidedSearchAuthorityError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| {
            ModelGuidedSearchAuthorityError::new(
                ModelGuidedSearchAuthorityErrorKind::RecordTooLarge,
            )
        })?;
        Ok(Sha256::digest(bytes).into())
    }

    /// Independently reconstructs a fresh authority from just this record's
    /// caller-supplied inputs via `Self::new`, then compares raw canonical-
    /// JSON SHA-256 digests. Deliberately hashes `self` directly rather than
    /// calling `self.digest()` (which would gate this check behind
    /// `validate` again); see
    /// `kernel_native_search_opponent_v1::matches_fresh_reconstruction_v1`'s
    /// doc comment for the full rationale this mirrors.
    ///
    /// Only catches tampering of fields NOT among `Self::new`'s inputs
    /// (`schema`, `authority_kind`, `algorithm_identity`, `node_key_identity`,
    /// `transition_budget`, `policy_step_depth_cap`, `seed_domain`,
    /// `engine_commit`, `card_db_hash`, `runtime_deck_catalog_sha256`, and,
    /// since the S0 content-binding change, all three contract-digest
    /// fields):
    /// tampering an input field and reconstructing from the tampered value
    /// would just reproduce a different, still-self-consistent record. Those
    /// input fields are exactly what `validate`'s explicit checks cover
    /// instead.
    ///
    /// `pub`, not `pub(crate)` (unlike v1's own analog): v1's equivalent has
    /// an in-crate, non-test production call site
    /// (`native_checkpoint_runner_v1.rs`, its stage-2 wiring); this schema
    /// has no wiring yet (item 6 is out of scope here), so this method's
    /// only current callers are this module's own tests. `pub` keeps it
    /// reachable as part of the schema's public contract for item 6 to call
    /// later, and, correspondingly, keeps it out of `dead_code` today: a
    /// `pub(crate)` item with no non-test in-crate caller would be flagged
    /// dead by `cargo clippy --all-targets -D warnings`, since the lib
    /// target (unlike the test target) never compiles `#[cfg(test)]` code.
    pub fn matches_fresh_reconstruction_v1(&self) -> bool {
        let Ok(reconstructed) = Self::new(
            self.tier,
            self.action_seed,
            &self.private_diagnostic_identity,
            &self.checkpoint_store_path_or_lineage_id,
            self.checkpoint_generation,
            &self.checkpoint_weight_bytes_sha256,
            &self.net_architecture_identity,
            self.consumption_mode,
        ) else {
            return false;
        };
        match (serde_json::to_vec(&reconstructed), serde_json::to_vec(self)) {
            (Ok(a), Ok(b)) => Sha256::digest(a) == Sha256::digest(b),
            _ => false,
        }
    }
}

/// Specific, field-cluster-scoped validation error kinds, matching the
/// repo's `CheckpointManifestV3ErrorKind` / `TrainRunV2ErrorKind` style
/// (`native_training_store_checkpoint_v3.rs`, `native_training_store_run_v2.rs`)
/// rather than `kernel_native_search_opponent_v1`'s single catch-all
/// `InvalidAuthority`. Each variant maps to exactly one `validate` check
/// group, and each group maps to either one of Section 1.4's own bullets or
/// its explicit "inherited verbatim from v1" closing bullet; see `validate`'s
/// doc comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelGuidedSearchAuthorityErrorKind {
    /// Whole-record canonical-JSON byte length exceeds
    /// `MODEL_GUIDED_SEARCH_AUTHORITY_MAX_BYTES_V1`.
    RecordTooLarge,
    /// `schema`, `authority_kind`, or `algorithm_identity` wrong, OR
    /// `authority_kind`/`algorithm_identity` aliased to v1's value.
    InvalidSchema,
    /// `node_key_identity` does not equal v1's inherited-verbatim tree-key
    /// identity.
    InvalidNodeKeyIdentity,
    /// `transition_budget` does not match `tier.transition_budget()`, or
    /// `policy_step_depth_cap` does not equal the inherited depth cap.
    InvalidSearchLadder,
    /// `seed_domain` does not equal v1's inherited-verbatim seed-domain
    /// label.
    InvalidSeedDomain,
    /// `engine_commit`, `card_db_hash`, `runtime_deck_catalog_sha256`, or
    /// `private_diagnostic_identity` invalid (Section 1.4's "inherited
    /// verbatim from v1" bullet).
    InvalidProvenance,
    /// `action_seed` is zero.
    InvalidActionSeed,
    /// `checkpoint_store_path_or_lineage_id` empty or too long,
    /// `checkpoint_weight_bytes_sha256` not SHA-256-shaped, or
    /// `checkpoint_generation` exceeds
    /// `MODEL_GUIDED_SEARCH_CHECKPOINT_GENERATION_MAX_V1` (zero is valid).
    InvalidCheckpointIdentity,
    /// `net_architecture_identity` empty, too long, or equal to
    /// `checkpoint_weight_bytes_sha256`.
    InvalidNetArchitectureIdentity,
    /// `puct_prior_quantization_contract_sha256` not SHA-256-shaped.
    InvalidPuctPriorQuantizationContractDigest,
    /// `value_quantization_contract_sha256` not SHA-256-shaped.
    InvalidValueQuantizationContractDigest,
    /// `forward_determinism_build_identity` not SHA-256-shaped.
    InvalidForwardDeterminismBuildIdentity,
    /// A build-override environment variable was set when this crate was
    /// compiled, so the pinned forward-determinism identity cannot be
    /// trusted to describe the arithmetic this binary actually performs.
    /// See `docs/audits/model_guided_forward_determinism_audit_v1.md`
    /// Section 6 item 2.
    ForbiddenBuildFlagOverride,
}

impl ModelGuidedSearchAuthorityErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::RecordTooLarge => "model_guided_search_authority_v1_record_too_large",
            Self::InvalidSchema => "model_guided_search_authority_v1_invalid_schema",
            Self::InvalidNodeKeyIdentity => {
                "model_guided_search_authority_v1_invalid_node_key_identity"
            }
            Self::InvalidSearchLadder => "model_guided_search_authority_v1_invalid_search_ladder",
            Self::InvalidSeedDomain => "model_guided_search_authority_v1_invalid_seed_domain",
            Self::InvalidProvenance => "model_guided_search_authority_v1_invalid_provenance",
            Self::InvalidActionSeed => "model_guided_search_authority_v1_invalid_action_seed",
            Self::InvalidCheckpointIdentity => {
                "model_guided_search_authority_v1_invalid_checkpoint_identity"
            }
            Self::InvalidNetArchitectureIdentity => {
                "model_guided_search_authority_v1_invalid_net_architecture_identity"
            }
            Self::InvalidPuctPriorQuantizationContractDigest => {
                "model_guided_search_authority_v1_invalid_puct_prior_quantization_contract_digest"
            }
            Self::InvalidValueQuantizationContractDigest => {
                "model_guided_search_authority_v1_invalid_value_quantization_contract_digest"
            }
            Self::InvalidForwardDeterminismBuildIdentity => {
                "model_guided_search_authority_v1_invalid_forward_determinism_build_identity"
            }
            Self::ForbiddenBuildFlagOverride => {
                "model_guided_search_authority_v1_forbidden_build_flag_override"
            }
        }
    }
}

/// No source bytes, field names, values, paths, or parser text are
/// retained; only the specific `ModelGuidedSearchAuthorityErrorKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelGuidedSearchAuthorityError {
    kind: ModelGuidedSearchAuthorityErrorKind,
}

impl ModelGuidedSearchAuthorityError {
    pub const fn new(kind: ModelGuidedSearchAuthorityErrorKind) -> Self {
        Self { kind }
    }

    pub const fn kind(&self) -> ModelGuidedSearchAuthorityErrorKind {
        self.kind
    }

    pub const fn code(&self) -> &'static str {
        self.kind.code()
    }
}

impl fmt::Display for ModelGuidedSearchAuthorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())
    }
}

impl std::error::Error for ModelGuidedSearchAuthorityError {}

fn is_lower_hex_v1(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel_native_search_opponent_v1::KernelNativeSearchAuthorityV1;
    use std::collections::HashSet;

    const VALID_STORE_PATH: &str = "D:/mtg-kernel-store/lineage-current-1";
    const VALID_WEIGHT_SHA256: &str =
        "1111111111111111111111111111111111111111111111111111111111111111";
    const VALID_ARCH_IDENTITY: &str = "net8-family/v1";
    /// Was `42`, back when `validate` only required a nonzero seed. The
    /// allowlist is real now, so the fixture must name a registered block.
    const VALID_ACTION_SEED: u64 = MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1[0];

    fn authority_v1(
        tier: KernelNativeSearchTierV1,
    ) -> Result<ModelGuidedSearchAuthorityV1, ModelGuidedSearchAuthorityError> {
        ModelGuidedSearchAuthorityV1::new(
            tier,
            VALID_ACTION_SEED,
            crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
            VALID_STORE_PATH,
            1_536,
            VALID_WEIGHT_SHA256,
            VALID_ARCH_IDENTITY,
            ModelGuidedSearchConsumptionModeV1::SearchAsOpponent,
        )
    }

    fn valid_json_value() -> serde_json::Value {
        let record = authority_v1(KernelNativeSearchTierV1::T512).unwrap();
        serde_json::to_value(&record).unwrap()
    }

    // ---- construction, digest, frozen literals ----

    #[test]
    fn authorized_record_constructs_validates_and_digests_reproducibly() {
        let record = authority_v1(KernelNativeSearchTierV1::T512).unwrap();
        assert!(record.validate().is_ok());
        assert_eq!(record.digest().unwrap(), record.digest().unwrap());
        assert_eq!(record.transition_budget, 512);
        assert_eq!(
            record.policy_step_depth_cap,
            KERNEL_NATIVE_SEARCH_DEPTH_CAP_V1
        );
        assert_eq!(record.node_key_identity, KERNEL_NATIVE_SEARCH_NODE_KEY_V1);
        assert_eq!(record.seed_domain, KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1);

        for tier in [
            KernelNativeSearchTierV1::T2048,
            KernelNativeSearchTierV1::T8192,
            KernelNativeSearchTierV1::T32768,
        ] {
            let other = authority_v1(tier).unwrap();
            assert_eq!(other.transition_budget, tier.transition_budget());
            assert_ne!(other.digest().unwrap(), record.digest().unwrap());
        }
    }

    #[test]
    fn frozen_literals_are_exact_and_distinct_from_v1() {
        assert_eq!(
            MODEL_GUIDED_SEARCH_AUTHORITY_SCHEMA_V1,
            "model_guided_searcher_authority/v1"
        );
        assert_eq!(
            MODEL_GUIDED_SEARCH_AUTHORITY_KIND_V1,
            "model-guided-searcher-v1"
        );
        assert_eq!(
            MODEL_GUIDED_SEARCH_ALGORITHM_V1,
            "deterministic-per-simulation-redeterminized-is-mcts-puct-prior-value-head-integer-ucb/v1"
        );
        assert_eq!(MODEL_GUIDED_SEARCH_AUTHORITY_MAX_BYTES_V1, 16 * 1024);
        assert_eq!(MODEL_GUIDED_SEARCH_IDENTITY_STRING_MAX_LEN_V1, 4096);
        assert_eq!(
            MODEL_GUIDED_SEARCH_CHECKPOINT_GENERATION_MAX_V1,
            (1_u64 << 63) - 1
        );
        assert_ne!(
            MODEL_GUIDED_SEARCH_AUTHORITY_KIND_V1,
            KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1
        );
        assert_ne!(
            MODEL_GUIDED_SEARCH_ALGORITHM_V1,
            KERNEL_NATIVE_SEARCH_ALGORITHM_V1
        );
    }

    #[test]
    fn error_codes_are_unique_and_stable() {
        let kinds = [
            ModelGuidedSearchAuthorityErrorKind::RecordTooLarge,
            ModelGuidedSearchAuthorityErrorKind::InvalidSchema,
            ModelGuidedSearchAuthorityErrorKind::InvalidNodeKeyIdentity,
            ModelGuidedSearchAuthorityErrorKind::InvalidSearchLadder,
            ModelGuidedSearchAuthorityErrorKind::InvalidSeedDomain,
            ModelGuidedSearchAuthorityErrorKind::InvalidProvenance,
            ModelGuidedSearchAuthorityErrorKind::InvalidActionSeed,
            ModelGuidedSearchAuthorityErrorKind::InvalidCheckpointIdentity,
            ModelGuidedSearchAuthorityErrorKind::InvalidNetArchitectureIdentity,
            ModelGuidedSearchAuthorityErrorKind::InvalidPuctPriorQuantizationContractDigest,
            ModelGuidedSearchAuthorityErrorKind::InvalidValueQuantizationContractDigest,
            ModelGuidedSearchAuthorityErrorKind::InvalidForwardDeterminismBuildIdentity,
        ];
        let codes: HashSet<&str> = kinds.iter().map(|kind| kind.code()).collect();
        assert_eq!(codes.len(), kinds.len(), "every error code must be unique");
        for kind in kinds {
            assert!(kind.code().starts_with("model_guided_search_authority_v1_"));
        }
    }

    // ---- fresh-reconstruction tripwire (mutation-catching, independent of validate) ----

    #[test]
    fn fresh_reconstruction_check_accepts_genuine_and_rejects_direct_field_tamper() {
        let authority = authority_v1(KernelNativeSearchTierV1::T512).unwrap();
        assert!(authority.matches_fresh_reconstruction_v1());

        macro_rules! assert_tamper_rejected {
            ($field:ident, $value:expr) => {{
                let mut tampered = authority.clone();
                tampered.$field = $value;
                assert!(
                    !tampered.matches_fresh_reconstruction_v1(),
                    "tampering {} must be caught by reconstruction",
                    stringify!($field)
                );
            }};
        }
        assert_tamper_rejected!(schema, "wrong-schema/v1".to_owned());
        assert_tamper_rejected!(authority_kind, "wrong-authority-kind/v1".to_owned());
        assert_tamper_rejected!(algorithm_identity, "wrong-algorithm/v1".to_owned());
        assert_tamper_rejected!(node_key_identity, "wrong-node-key/v1".to_owned());
        assert_tamper_rejected!(seed_domain, "wrong-seed-domain/v1".to_owned());
        assert_tamper_rejected!(engine_commit, "0".repeat(40));
        assert_tamper_rejected!(card_db_hash, authority.card_db_hash.wrapping_add(1));
        assert_tamper_rejected!(runtime_deck_catalog_sha256, "0".repeat(64));
        assert_tamper_rejected!(policy_step_depth_cap, authority.policy_step_depth_cap + 1);
    }

    // ---- validate() semantic rejection tests, one per error kind ----

    #[test]
    fn validate_rejects_schema_authority_kind_and_algorithm_identity_corruption() {
        let base = authority_v1(KernelNativeSearchTierV1::T512).unwrap();

        let mut wrong_schema = base.clone();
        wrong_schema.schema = "wrong/v1".to_owned();
        assert_eq!(
            wrong_schema.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidSchema
        );

        let mut wrong_kind = base.clone();
        wrong_kind.authority_kind = "wrong-kind/v1".to_owned();
        assert_eq!(
            wrong_kind.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidSchema
        );

        let mut aliased_kind = base.clone();
        aliased_kind.authority_kind = KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1.to_owned();
        assert_eq!(
            aliased_kind.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidSchema,
            "must reject aliasing v1's authority_kind, not just missing its own"
        );

        let mut wrong_algorithm = base.clone();
        wrong_algorithm.algorithm_identity = "wrong-algorithm/v1".to_owned();
        assert_eq!(
            wrong_algorithm.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidSchema
        );

        let mut aliased_algorithm = base;
        aliased_algorithm.algorithm_identity = KERNEL_NATIVE_SEARCH_ALGORITHM_V1.to_owned();
        assert_eq!(
            aliased_algorithm.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidSchema,
            "must reject aliasing v1's algorithm_identity, not just missing its own"
        );
    }

    #[test]
    fn validate_rejects_node_key_search_ladder_and_seed_domain_corruption() {
        let base = authority_v1(KernelNativeSearchTierV1::T512).unwrap();

        let mut wrong_node_key = base.clone();
        wrong_node_key.node_key_identity = "wrong/v1".to_owned();
        assert_eq!(
            wrong_node_key.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidNodeKeyIdentity
        );

        let mut wrong_budget = base.clone();
        wrong_budget.transition_budget += 1;
        assert_eq!(
            wrong_budget.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidSearchLadder
        );

        let mut wrong_depth = base.clone();
        wrong_depth.policy_step_depth_cap += 1;
        assert_eq!(
            wrong_depth.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidSearchLadder
        );

        let mut wrong_seed_domain = base;
        wrong_seed_domain.seed_domain = "wrong/v1".to_owned();
        assert_eq!(
            wrong_seed_domain.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidSeedDomain
        );
    }

    #[test]
    fn validate_rejects_provenance_and_action_seed_corruption() {
        let base = authority_v1(KernelNativeSearchTierV1::T512).unwrap();

        let mut wrong_engine = base.clone();
        wrong_engine.engine_commit = "z".repeat(40);
        assert_eq!(
            wrong_engine.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidProvenance
        );

        let mut wrong_carddb = base.clone();
        wrong_carddb.card_db_hash = wrong_carddb.card_db_hash.wrapping_add(1);
        assert_eq!(
            wrong_carddb.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidProvenance
        );

        let mut wrong_deck_catalog = base.clone();
        wrong_deck_catalog.runtime_deck_catalog_sha256 = "0".repeat(64);
        assert_eq!(
            wrong_deck_catalog.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidProvenance
        );

        let mut wrong_diagnostic = base.clone();
        wrong_diagnostic.private_diagnostic_identity = "wrong/v1".to_owned();
        assert_eq!(
            wrong_diagnostic.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidProvenance
        );

        // Zero was the only rejected seed before the allowlist existed. It
        // still is rejected, but now so is every other unregistered value,
        // including ones that look plausible: v1's own calibration seed and
        // v1's own pool seed must not be reusable as a model-guided seed,
        // or the two authorities' seed spaces would silently overlap.
        for unregistered in [
            0,
            1,
            MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1[0] + 1,
            crate::kernel_native_search_opponent_v1::KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1[0],
            crate::kernel_native_search_opponent_v1::KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1
                [0],
            u64::MAX,
        ] {
            let mut wrong_seed = base.clone();
            wrong_seed.action_seed = unregistered;
            assert_eq!(
                wrong_seed.validate().unwrap_err().kind(),
                ModelGuidedSearchAuthorityErrorKind::InvalidActionSeed,
                "seed {unregistered} must not be authorized"
            );
        }

        // Every registered block, conversely, constructs and validates.
        for (block_id, &seed) in MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1
            .iter()
            .enumerate()
        {
            assert_eq!(authorized_seed_block_v1(block_id), Some(seed));
            let mut registered = base.clone();
            registered.action_seed = seed;
            assert!(
                registered.validate().is_ok(),
                "block {block_id} must validate"
            );
        }
        assert_eq!(
            authorized_seed_block_v1(MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1.len()),
            None
        );
    }

    /// The model-guided seed band must not intersect either of v1's own
    /// bands, so one seed can never be simultaneously authorized for two
    /// different search authorities.
    #[test]
    fn authorized_seed_blocks_are_disjoint_from_v1_bands_v1() {
        use crate::kernel_native_search_opponent_v1::{
            KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1, KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1,
        };
        let ours: HashSet<u64> = MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1
            .into_iter()
            .collect();
        assert_eq!(
            ours.len(),
            MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1.len(),
            "the allowlist must not contain a duplicate block"
        );
        for seed in KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1
            .into_iter()
            .chain(KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1)
        {
            assert!(!ours.contains(&seed), "seed {seed} is claimed twice");
        }
        assert!(!ours.contains(&0));
    }

    /// The three contract-digest fields are no longer caller-supplied, so
    /// `new` always mints them from the pinned literals, and tampering one
    /// is caught by BOTH `validate` and the fresh-reconstruction check
    /// (which could not see them while they were inputs).
    #[test]
    fn contract_digests_are_content_bound_and_tamper_evident_v1() {
        use crate::model_guided_search_contract_digests_v1::{
            forward_determinism_build_digest_v1, lower_hex_digest_v1,
            prior_quantization_contract_digest_v1, value_quantization_contract_digest_v1,
        };
        let base = authority_v1(KernelNativeSearchTierV1::T512).unwrap();
        assert_eq!(
            base.puct_prior_quantization_contract_sha256,
            lower_hex_digest_v1(prior_quantization_contract_digest_v1())
        );
        assert_eq!(
            base.value_quantization_contract_sha256,
            lower_hex_digest_v1(value_quantization_contract_digest_v1())
        );
        assert_eq!(
            base.forward_determinism_build_identity,
            lower_hex_digest_v1(forward_determinism_build_digest_v1())
        );
        assert!(base.matches_fresh_reconstruction_v1());

        // A well-formed but wrong SHA-256-shaped value used to pass the old
        // structural check; each must now be rejected with its own kind.
        let wrong = "5".repeat(64);
        let mut wrong_prior = base.clone();
        wrong_prior
            .puct_prior_quantization_contract_sha256
            .clone_from(&wrong);
        assert_eq!(
            wrong_prior.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidPuctPriorQuantizationContractDigest
        );
        assert!(!wrong_prior.matches_fresh_reconstruction_v1());

        let mut wrong_value = base.clone();
        wrong_value
            .value_quantization_contract_sha256
            .clone_from(&wrong);
        assert_eq!(
            wrong_value.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidValueQuantizationContractDigest
        );
        assert!(!wrong_value.matches_fresh_reconstruction_v1());

        let mut wrong_build = base;
        wrong_build
            .forward_determinism_build_identity
            .clone_from(&wrong);
        assert_eq!(
            wrong_build.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidForwardDeterminismBuildIdentity
        );
        assert!(!wrong_build.matches_fresh_reconstruction_v1());
    }

    #[test]
    fn validate_rejects_checkpoint_and_net_architecture_identity_corruption() {
        let base = authority_v1(KernelNativeSearchTierV1::T512).unwrap();

        let mut empty_store_path = base.clone();
        empty_store_path.checkpoint_store_path_or_lineage_id = String::new();
        assert_eq!(
            empty_store_path.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidCheckpointIdentity
        );

        let mut too_long_store_path = base.clone();
        too_long_store_path.checkpoint_store_path_or_lineage_id =
            "x".repeat(MODEL_GUIDED_SEARCH_IDENTITY_STRING_MAX_LEN_V1 + 1);
        assert_eq!(
            too_long_store_path.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidCheckpointIdentity
        );

        let mut not_hex_weight = base.clone();
        not_hex_weight.checkpoint_weight_bytes_sha256 = "not-a-hash".to_owned();
        assert_eq!(
            not_hex_weight.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidCheckpointIdentity
        );

        let mut short_hex_weight = base.clone();
        short_hex_weight.checkpoint_weight_bytes_sha256 = "ab".repeat(31);
        assert_eq!(
            short_hex_weight.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidCheckpointIdentity
        );

        // checkpoint_generation: tamper past the u63 sanity bound is
        // rejected as InvalidCheckpointIdentity (countersign-panel finding:
        // this field previously had no validate() guard at all).
        let mut over_max_generation = base.clone();
        over_max_generation.checkpoint_generation =
            MODEL_GUIDED_SEARCH_CHECKPOINT_GENERATION_MAX_V1 + 1;
        assert_eq!(
            over_max_generation.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidCheckpointIdentity
        );

        // Zero and the exact max boundary both remain legitimate: zero is a
        // genesis checkpoint (this schema deliberately does NOT layer a
        // nonzero requirement onto checkpoint_generation the way it does for
        // action_seed), and the bound itself is inclusive.
        let mut zero_generation = base.clone();
        zero_generation.checkpoint_generation = 0;
        assert!(zero_generation.validate().is_ok());

        let mut max_generation = base.clone();
        max_generation.checkpoint_generation = MODEL_GUIDED_SEARCH_CHECKPOINT_GENERATION_MAX_V1;
        assert!(max_generation.validate().is_ok());

        let mut empty_arch = base.clone();
        empty_arch.net_architecture_identity = String::new();
        assert_eq!(
            empty_arch.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidNetArchitectureIdentity
        );

        let mut too_long_arch = base.clone();
        too_long_arch.net_architecture_identity =
            "x".repeat(MODEL_GUIDED_SEARCH_IDENTITY_STRING_MAX_LEN_V1 + 1);
        assert_eq!(
            too_long_arch.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidNetArchitectureIdentity
        );

        let mut aliased_arch = base;
        aliased_arch.net_architecture_identity =
            aliased_arch.checkpoint_weight_bytes_sha256.clone();
        assert_eq!(
            aliased_arch.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidNetArchitectureIdentity,
            "net architecture identity must never silently equal the weight-bytes hash"
        );
    }

    #[test]
    fn validate_rejects_malformed_quantization_and_forward_determinism_digests() {
        let base = authority_v1(KernelNativeSearchTierV1::T512).unwrap();

        let mut bad_puct = base.clone();
        bad_puct.puct_prior_quantization_contract_sha256 = "short".to_owned();
        assert_eq!(
            bad_puct.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidPuctPriorQuantizationContractDigest
        );

        let mut bad_value = base.clone();
        bad_value.value_quantization_contract_sha256 = "Z".repeat(64);
        assert_eq!(
            bad_value.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidValueQuantizationContractDigest
        );

        let mut bad_forward = base;
        bad_forward.forward_determinism_build_identity = "0".repeat(63);
        assert_eq!(
            bad_forward.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidForwardDeterminismBuildIdentity
        );
    }

    /// The whole-record byte cap is checked before any per-field check
    /// (`validate`'s first block), so a field long enough to blow the
    /// record-level cap is caught as `RecordTooLarge` specifically, never
    /// misreported as the per-field `InvalidCheckpointIdentity` check
    /// further down (exercised separately, with a much smaller overrun, by
    /// `validate_rejects_checkpoint_and_net_architecture_identity_corruption`).
    #[test]
    fn validate_rejects_whole_record_over_the_byte_cap() {
        let mut oversized = authority_v1(KernelNativeSearchTierV1::T512).unwrap();
        oversized.checkpoint_store_path_or_lineage_id =
            "x".repeat(MODEL_GUIDED_SEARCH_AUTHORITY_MAX_BYTES_V1);
        assert_eq!(
            oversized.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::RecordTooLarge
        );
    }

    // ---- malformed-shape (deserialization) rejection tests ----

    #[test]
    fn deserialization_rejects_every_missing_field() {
        let value = valid_json_value();
        let object = value.as_object().unwrap();
        assert!(!object.is_empty());
        for key in object.keys() {
            let mut mutated = object.clone();
            mutated.remove(key);
            let mutated_value = serde_json::Value::Object(mutated);
            let result: Result<ModelGuidedSearchAuthorityV1, _> =
                serde_json::from_value(mutated_value);
            assert!(
                result.is_err(),
                "removing field `{key}` must be rejected at deserialization"
            );
        }
    }

    #[test]
    fn deserialization_rejects_unknown_field() {
        let mut object = valid_json_value().as_object().unwrap().clone();
        object.insert("unexpected_extra_field".to_owned(), serde_json::json!(true));
        let mutated_value = serde_json::Value::Object(object);
        let result: Result<ModelGuidedSearchAuthorityV1, _> = serde_json::from_value(mutated_value);
        assert!(result.is_err());
    }

    #[test]
    fn deserialization_rejects_wrong_kind_for_representative_fields() {
        let base_object = valid_json_value().as_object().unwrap().clone();

        let mut wrong_tier_kind = base_object.clone();
        wrong_tier_kind.insert("tier".to_owned(), serde_json::json!("not-a-real-tier"));
        assert!(
            serde_json::from_value::<ModelGuidedSearchAuthorityV1>(serde_json::Value::Object(
                wrong_tier_kind
            ))
            .is_err()
        );

        let mut tier_as_number = base_object.clone();
        tier_as_number.insert("tier".to_owned(), serde_json::json!(512));
        assert!(
            serde_json::from_value::<ModelGuidedSearchAuthorityV1>(serde_json::Value::Object(
                tier_as_number
            ))
            .is_err()
        );

        let mut card_db_hash_as_string = base_object.clone();
        card_db_hash_as_string.insert("card_db_hash".to_owned(), serde_json::json!("not-a-number"));
        assert!(
            serde_json::from_value::<ModelGuidedSearchAuthorityV1>(serde_json::Value::Object(
                card_db_hash_as_string
            ))
            .is_err()
        );

        let mut generation_as_string = base_object.clone();
        generation_as_string.insert(
            "checkpoint_generation".to_owned(),
            serde_json::json!("1536"),
        );
        assert!(
            serde_json::from_value::<ModelGuidedSearchAuthorityV1>(serde_json::Value::Object(
                generation_as_string
            ))
            .is_err()
        );

        let mut unknown_consumption_mode = base_object;
        unknown_consumption_mode.insert(
            "consumption_mode".to_owned(),
            serde_json::json!("search_at_the_disco"),
        );
        assert!(
            serde_json::from_value::<ModelGuidedSearchAuthorityV1>(serde_json::Value::Object(
                unknown_consumption_mode
            ))
            .is_err()
        );
    }

    #[test]
    fn deserialization_rejects_hybrid_mix_of_v1_and_model_guided_fields() {
        // A record carrying every model-guided field PLUS v1's evaluator
        // fields is not a superset either schema accepts: v1's evaluator
        // fields are unknown to this schema (deny_unknown_fields).
        let mut hybrid = valid_json_value().as_object().unwrap().clone();
        hybrid.insert(
            "evaluator_identity".to_owned(),
            serde_json::json!("kernel-native-opponent-only-integer-evaluator/v1"),
        );
        hybrid.insert(
            "evaluator_sha256".to_owned(),
            serde_json::json!("5".repeat(64)),
        );
        let result: Result<ModelGuidedSearchAuthorityV1, _> =
            serde_json::from_value(serde_json::Value::Object(hybrid));
        assert!(result.is_err());
    }

    #[test]
    fn the_two_authority_schemas_never_structurally_accept_each_others_records() {
        // A genuine v1 record fed into the new type must fail: v1 lacks the
        // new schema's required checkpoint/quantization/consumption-mode
        // fields, and carries evaluator_identity/evaluator_sha256, which the
        // new schema's deny_unknown_fields rejects.
        let v1_record = KernelNativeSearchAuthorityV1::current(
            KernelNativeSearchTierV1::T512,
            1_987_001,
            crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
        )
        .unwrap();
        let v1_value = serde_json::to_value(&v1_record).unwrap();
        assert!(
            serde_json::from_value::<ModelGuidedSearchAuthorityV1>(v1_value.clone()).is_err(),
            "a genuine v1 record must not deserialize as a model-guided record"
        );

        // And the reverse: a genuine model-guided record fed into v1's own
        // type must fail: it lacks evaluator_identity/evaluator_sha256, and
        // carries this schema's new fields, which v1's own
        // deny_unknown_fields rejects.
        let model_guided_value = valid_json_value();
        assert!(
            serde_json::from_value::<KernelNativeSearchAuthorityV1>(model_guided_value).is_err(),
            "a genuine model-guided record must not deserialize as a v1 record"
        );
        // Sanity: the v1 value really does round-trip as itself.
        assert!(serde_json::from_value::<KernelNativeSearchAuthorityV1>(v1_value).is_ok());
    }

    #[test]
    fn consumption_mode_round_trips_all_three_variants() {
        for (mode, expected_json) in [
            (
                ModelGuidedSearchConsumptionModeV1::SearchAtInference,
                "\"search_at_inference\"",
            ),
            (
                ModelGuidedSearchConsumptionModeV1::SearchAsOpponent,
                "\"search_as_opponent\"",
            ),
            (
                ModelGuidedSearchConsumptionModeV1::SearchAtTrainingTargets,
                "\"search_at_training_targets\"",
            ),
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, expected_json);
            let round_tripped: ModelGuidedSearchConsumptionModeV1 =
                serde_json::from_str(&json).unwrap();
            assert_eq!(round_tripped, mode);
        }
    }
}
