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
//! opponent, does not touch the science loop, Store records, population
//! selection, or a scorer bridge, and does not compute or verify a live
//! quantization-contract digest, a live forward-determinism build digest, or
//! a live checkpoint hash. All of that is deferred to design item 5 (the
//! search-loop change) and item 6 (stage-2-equivalent wiring), each gated on
//! its own diff review exactly as the design requires.
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
//!   fields" instruction), but, unlike v1, this schema does NOT freeze an
//!   authorized-seed allowlist analogous to
//!   `KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1`. v1's allowlist encodes
//!   seeds already pre-registered for v1's own countersigned calibration
//!   panel. This design's own governing document states plainly, twice,
//!   that it authorizes no seed at all ("No seed is consumed or proposed by
//!   this document"; "self-authorizing" seed use is explicitly deferred to
//!   "whatever calibration, panel, or pilot pre-registration step" assigns
//!   one). Freezing a seed allowlist here would fabricate an authorization
//!   this document does not grant. `validate` therefore applies only the
//!   minimal fail-closed structural guard available without inventing
//!   authorization: `action_seed` must be nonzero (zero reads as an
//!   uninitialized placeholder everywhere else in this codebase's seed
//!   conventions, never a genuine domain-separation seed). Binding a real
//!   allowlist is future work for whichever amendment or sheet pre-
//!   registers this design's first seeds.
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
//!   `value_quantization_contract_sha256`: PLACEHOLDER commitment fields.
//!   Design items 1 and 2 (the PUCT prior-quantization and value-
//!   quantization contract modules, Sections 1.2-1.3) are being built
//!   concurrently in a sibling worktree and are not available here. This
//!   schema validates these two fields ONLY structurally (lower-hex,
//!   64-character, i.e. SHA-256-shaped); it does not, and must not, freeze
//!   an expected digest value for either, because that value does not exist
//!   in this worktree. Asserting the record's digest equals the real
//!   contract digest items 1/2 produce is item 6's wiring responsibility.
//! - `forward_determinism_build_identity`: PLACEHOLDER commitment field for
//!   "a build/target-cpu flag digest or binary SHA-256 pinning the exact
//!   deterministic-forward build in use" (Section 1.4), extending v1's own
//!   registration-layer "scorer binary SHA-256" requirement to this
//!   design's forward-pass binary. Design item 3 (the deterministic-CPU-
//!   forward audit) is a separate implementation item, also not built here.
//!   Same placeholder discipline as the two quantization digests: validated
//!   structurally only, no frozen expected value.
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

use crate::card_def::KERNEL_CARDDB_HASH;
use crate::kernel_native_search_opponent_v1::{
    KernelNativeSearchTierV1, KERNEL_NATIVE_SEARCH_ALGORITHM_V1,
    KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1, KERNEL_NATIVE_SEARCH_DEPTH_CAP_V1,
    KERNEL_NATIVE_SEARCH_NODE_KEY_V1, KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1,
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
    /// The eleven-parameter shape is inherent to Section 1.4's record
    /// schema, not an API design choice; see the repo's existing
    /// `#[allow(clippy::too_many_arguments)]` precedent (`event.rs`,
    /// `effect.rs`, `async_flat_scored_rollout_v1.rs`/`_v2.rs`) for records
    /// and call sites with a similarly irreducible field count.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tier: KernelNativeSearchTierV1,
        action_seed: u64,
        private_diagnostic_identity: &str,
        checkpoint_store_path_or_lineage_id: &str,
        checkpoint_generation: u64,
        checkpoint_weight_bytes_sha256: &str,
        net_architecture_identity: &str,
        puct_prior_quantization_contract_sha256: &str,
        value_quantization_contract_sha256: &str,
        forward_determinism_build_identity: &str,
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
            puct_prior_quantization_contract_sha256: puct_prior_quantization_contract_sha256
                .to_string(),
            value_quantization_contract_sha256: value_quantization_contract_sha256.to_string(),
            forward_determinism_build_identity: forward_determinism_build_identity.to_string(),
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

        if self.action_seed == 0 {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidActionSeed,
            ));
        }

        if self.checkpoint_store_path_or_lineage_id.is_empty()
            || self.checkpoint_store_path_or_lineage_id.len()
                > MODEL_GUIDED_SEARCH_IDENTITY_STRING_MAX_LEN_V1
            || !is_lower_hex_v1(&self.checkpoint_weight_bytes_sha256, 64)
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

        if !is_lower_hex_v1(&self.puct_prior_quantization_contract_sha256, 64) {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidPuctPriorQuantizationContractDigest,
            ));
        }

        if !is_lower_hex_v1(&self.value_quantization_contract_sha256, 64) {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidValueQuantizationContractDigest,
            ));
        }

        if !is_lower_hex_v1(&self.forward_determinism_build_identity, 64) {
            return Err(ModelGuidedSearchAuthorityError::new(
                Kind::InvalidForwardDeterminismBuildIdentity,
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
    /// `engine_commit`, `card_db_hash`, `runtime_deck_catalog_sha256`):
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
            &self.puct_prior_quantization_contract_sha256,
            &self.value_quantization_contract_sha256,
            &self.forward_determinism_build_identity,
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
    /// `checkpoint_store_path_or_lineage_id` empty or too long, or
    /// `checkpoint_weight_bytes_sha256` not SHA-256-shaped.
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
    const VALID_PUCT_DIGEST: &str =
        "2222222222222222222222222222222222222222222222222222222222222222";
    const VALID_VALUE_DIGEST: &str =
        "3333333333333333333333333333333333333333333333333333333333333333";
    const VALID_FORWARD_BUILD: &str =
        "4444444444444444444444444444444444444444444444444444444444444444";

    fn authority_v1(
        tier: KernelNativeSearchTierV1,
    ) -> Result<ModelGuidedSearchAuthorityV1, ModelGuidedSearchAuthorityError> {
        ModelGuidedSearchAuthorityV1::new(
            tier,
            42,
            crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
            VALID_STORE_PATH,
            1_536,
            VALID_WEIGHT_SHA256,
            VALID_ARCH_IDENTITY,
            VALID_PUCT_DIGEST,
            VALID_VALUE_DIGEST,
            VALID_FORWARD_BUILD,
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

        let mut zero_seed = base;
        zero_seed.action_seed = 0;
        assert_eq!(
            zero_seed.validate().unwrap_err().kind(),
            ModelGuidedSearchAuthorityErrorKind::InvalidActionSeed
        );
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
