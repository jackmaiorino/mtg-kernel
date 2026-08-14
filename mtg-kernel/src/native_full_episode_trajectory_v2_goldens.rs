//! Portable golden-artifact consumer for the versioned full-episode trajectory
//! audit contract V2.
//!
//! This module is the independent Phase B2 consumer of
//! `data/native_full_episode_trajectory_v2_goldens.json`.  It is a child test
//! module of the production V2 contract, so it can see that module's private
//! items; it deliberately uses almost none of them.
//!
//! Independence rules enforced here, in the order they matter:
//!
//! 1. Every one of the twenty-six declared authority facts has its own frozen
//!    literal in this file.  The artifact is validated against *these* literals,
//!    never against production's, so a coordinated edit to production's private
//!    `EXPECTED_*` block cannot silently re-bless a drifted artifact.
//! 2. The twenty-four literals that production also backs are then cross-checked
//!    one-for-one against production's private `EXPECTED_*` values and against
//!    the live constants their owning modules currently export, and production's
//!    own private live guard is invoked.  Three independent witnesses must agree.
//! 3. The thirty-four-atom V2 envelope is rebuilt here from this module's own
//!    literals, the artifact's start values, and an inner digest this module
//!    computes itself from the stored inner preimage.  Production's private
//!    `envelope_sha256_v2` is never called, production's V2 self-seals are never
//!    used as the oracle, and the independent builder never calls the direct V1
//!    accumulator: that call is a separate companion check.
//! 4. The semantic stream is rebuilt from typed values in the generator's exact
//!    order, and its length, atom count, and SHA-256 are pinned.  The Python
//!    generator is never invoked.
//! 5. The two metadata-only facts (the environment reference Python source hash
//!    and the trainer-schedule goldens schema) are proven against the live
//!    embedded Python source and the live embedded schedule artifact, not
//!    against a declaration.
//!
//! Every one of the twenty-seven rejection fixtures is *reconstructed* rather
//! than replayed.  This module rebuilds the two positive baselines from its own
//! literals, applies the accepted generator's exact mutation for each case name,
//! states the expected code as a literal, and only then compares against the
//! stored fixture and runs production on the reconstructed input.  No test in
//! this file executes an artifact-supplied reject input or trusts an
//! artifact-supplied expected code.
//!
//! The consumer seam has exactly four layers, and each is separately reachable:
//!
//! * A, `load_sealed_artifact_v2`: the exact raw byte length and the exact raw
//!   SHA-256, then C, then D.
//! * B, `scan_raw_document_v2`: a hand-written recursive scanner over the raw
//!   bytes that rejects duplicate object keys and every float or nonfinite
//!   spelling at every depth.  It is deliberately not built on `serde_json`, so
//!   a serde failure can never make a scanner test vacuous.
//! * C, `load_unsealed_artifact_v2`: the reachable global size cap, the exact
//!   raw-byte gate, B, typed serde, exact canonical re-encoding equality, then D.
//! * D, `validate_artifact_semantics_v2`: the whole-artifact typed semantic
//!   validator, invoked from both the sealed and the unsealed success paths
//!   before any case is classified.

use super::*;
use crate::native_full_episode_trajectory_v1::NativeTrajectoryActorRoleV1;
use crate::rl::{TerminalClassificationV1, TerminalOutcomeV1, TerminalSafeCodeV2};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

// ---------------------------------------------------------------- embedded live bytes
//
// Paths resolve from `mtg-kernel/src`.  Everything this module proves is proven
// against bytes compiled into the test binary, so no test depends on the working
// directory, on a generator run, or on a declaration being honest.

const V2_ARTIFACT_BYTES: &[u8] =
    include_bytes!("../../data/native_full_episode_trajectory_v2_goldens.json");
const V1_TRAJECTORY_ARTIFACT_BYTES: &[u8] =
    include_bytes!("../../data/native_full_episode_trajectory_v1_goldens.json");
const ENVIRONMENT_KDF_ARTIFACT_BYTES: &[u8] =
    include_bytes!("../../data/environment_randomization_v2/goldens_v1.json");
const RESET_TRAJECTORY_ARTIFACT_BYTES: &[u8] = include_bytes!(
    "../../data/environment_randomization_v2/reset_physical_trajectory_goldens_v1.json"
);
const TRAINER_SCHEDULE_ARTIFACT_BYTES: &[u8] =
    include_bytes!("../../data/native_trainer_schedule_v1_goldens.json");
const RUNTIME_DECK_CATALOG_BYTES: &[u8] = include_bytes!("../../data/runtime_decks_v1.json");
const ENVIRONMENT_PYTHON_REFERENCE_BYTES: &[u8] =
    include_bytes!("../../python/tools/environment_randomization_v2_reference.py");

// ------------------------------------------------------------------- frozen pins

/// The exact raw byte length of the accepted V2 artifact.
const V2_ARTIFACT_RAW_LEN: usize = 251_658;
/// The exact SHA-256 of those raw bytes.
const V2_ARTIFACT_RAW_SHA256: &str =
    "771323c7d2748204666d3f17a36102768416cf471cfbd67f5a7c7decfe12defc";
/// The exact byte length of the rebuilt portable semantic stream.
const V2_SEMANTIC_STREAM_LEN: usize = 226_765;
/// The exact SHA-256 of that stream.
const V2_SEMANTIC_STREAM_SHA256: &str =
    "ece763620ec193fe993bdcb4848888d53cd137761fc2fa551766dcaa181c17a8";
/// The frozen atom count of that stream: six header atoms, six atoms per
/// positive case, one pair-positive count atom, four atoms per pair positive,
/// one trajectory-reject count atom, three atoms per trajectory reject, one
/// pair-reject count atom, and three atoms per pair reject.
const V2_SEMANTIC_STREAM_ATOM_COUNT: usize = 124;

const V2_POSITIVE_CASE_COUNT: usize = 5;
const V2_PAIR_POSITIVE_CASE_COUNT: usize = 1;
const V2_TRAJECTORY_REJECT_CASE_COUNT: usize = 21;
const V2_PAIR_REJECT_CASE_COUNT: usize = 6;

const MAX_GOLDEN_ARTIFACT_BYTES_V2: usize = 4 * 1_024 * 1_024;
const MAX_GOLDEN_CASES_V2: usize = 256;
const MAX_GOLDEN_DECISIONS_V2: usize = 4_096;

/// The exact V2 envelope atom count, restated independently of production's
/// public constant so a production edit cannot move the test-side oracle.
const ACCEPTED_ENVELOPE_ATOM_COUNT_V2: usize = 34;

const ACCEPTED_U62_MAX_V2: u64 = (1_u64 << 62) - 1;
const ACCEPTED_U63_MAX_V2: u64 = (1_u64 << 63) - 1;

// ------------------------------------------- B2's own twenty-six authority literals
//
// Twenty-four of these are also frozen inside production and exported live by
// their owning modules; two are metadata-only and have no live Rust constant at
// all.  They are transcribed here from the accepted Phase A contract rather than
// aliased, because a literal that is aliased to what it is checking proves
// nothing.

// inner_trajectory: six facts.
const ACCEPTED_INNER_IDENTITY_V2: &str = "mtg-kernel-native-full-episode-trajectory-sha256-v1";
const ACCEPTED_INNER_GOLDENS_SCHEMA_V2: &str =
    "mtg_kernel_native_full_episode_trajectory_goldens/v1";
const ACCEPTED_INNER_GOLDENS_GENERATOR_IDENTITY_V2: &str =
    "mtg-kernel-native-full-episode-trajectory-goldens-stdlib-python-v1";
const ACCEPTED_INNER_GOLDEN_STREAM_IDENTITY_V2: &str =
    "mtg-kernel-native-full-episode-trajectory-golden-vector-stream-sha256-v1";
const ACCEPTED_INNER_GOLDENS_FILE_SHA256_V2: &str =
    "502a1b4ba296fdc4b2f4e8fd61cc5b4d64f152c9b84b4e11a85967f76c3bde8b";
const ACCEPTED_INNER_GOLDEN_STREAM_SHA256_V2: &str =
    "f5230cbbc0b87735e7aa14c89ce31e41ce769de3f4292cafe63dad4733168d7a";

// environment_randomization: five facts, one of them metadata-only.
const ACCEPTED_ENVIRONMENT_IDENTITY_V2: &str = "mtg-kernel-environment-randomization-sha256-v2";
const ACCEPTED_ENVIRONMENT_NAMESPACE_V2: &str = "environment-randomization-substream";
const ACCEPTED_ENVIRONMENT_KDF_GOLDENS_SCHEMA_V2: &str =
    "mtg-kernel-environment-randomization-v2-goldens/v1";
const ACCEPTED_ENVIRONMENT_KDF_GOLDENS_FILE_SHA256_V2: &str =
    "bc2b0d66f8e3eb608b6035321f23a214bbf5141aaf7305f50f606f6c85b4a3bc";
/// Metadata-only: no live Rust constant carries it.  It is proven below by
/// hashing the embedded environment reference Python source.
const ACCEPTED_ENVIRONMENT_PYTHON_REFERENCE_FILE_SHA256_V2: &str =
    "9dd7e5357d98ff5a7ac302d285da91fb56cf0d422c5aef6bc9b53f2a5d822024";

// reset_trajectory: six facts.
const ACCEPTED_RESET_GOLDENS_SCHEMA_V2: &str =
    "mtg-kernel-environment-randomization-v2-reset-physical-trajectory-goldens/v1";
const ACCEPTED_RESET_GENERATOR_IDENTITY_V2: &str =
    "mtg-kernel-environment-randomization-v2-reset-physical-trajectory-goldens-stdlib-python-v1";
const ACCEPTED_RESET_PHYSICAL_PROJECTION_IDENTITY_V2: &str =
    "mtg-kernel-environment-randomization-v2-physical-card-definition-projection/v1";
const ACCEPTED_RESET_PORTABLE_STREAM_IDENTITY_V2: &str = "mtg-kernel-environment-randomization-v2-reset-physical-trajectory-portable-vector-stream-sha256-v1";
const ACCEPTED_RESET_GOLDENS_FILE_SHA256_V2: &str =
    "ab002901a598d40732d39f9b0f21abaa2b7445e63b1c14d45a44b7900f6b739b";
const ACCEPTED_RESET_PORTABLE_STREAM_SHA256_V2: &str =
    "15d312141f8d96f079684dd64b58b5bab803086a78ac9687e3c14aab91e0a3c9";

// trainer_schedule: four facts, one of them metadata-only.
const ACCEPTED_TRAINER_SCHEDULE_IDENTITY_V2: &str = "mtg-kernel-native-trainer-schedule-sha256-v1";
const ACCEPTED_TRAINER_SEED_VERSION_V2: &str = "kernel-python-rl-trainer-sha256-v2";
/// Metadata-only: no live Rust constant carries it.  It is proven below by
/// parsing the embedded trainer-schedule goldens artifact.
const ACCEPTED_TRAINER_SCHEDULE_GOLDENS_SCHEMA_V2: &str =
    "mtg_kernel_native_trainer_schedule_goldens/v1";
const ACCEPTED_TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_V2: &str =
    "6b2e1edbbe49b4e02f98794f9057f5c2bb8e3079d2ba8cb3e2a4b9ea6c34867c";

// runtime_deck_catalog: five facts.
const ACCEPTED_RUNTIME_DECK_CATALOG_SCHEMA_V2: &str = "kernel_runtime_decks/v1";
const ACCEPTED_RUNTIME_DECK_PROTOCOL_V2: &str = "canonical-mainboard-bo1/v1";
const ACCEPTED_RUNTIME_DECK_MATERIALIZATION_PROTOCOL_V2: &str =
    "xmage_xml_row_then_copy_ordinal/v1";
const ACCEPTED_RUNTIME_DECK_HASH_ALGORITHM_V2: &str = "fnv1a64-serde-json-u16-array/v1";
// Re-baselined once per the owner ruling on record (collab CLAUDE #236,
// 2026-08-14); see native_full_episode_trajectory_v2.rs's
// EXPECTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2 for the full rationale.
const ACCEPTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2: &str =
    "68e7602f3a4df6217119406973954630800c358a10fca9f28e6cf9f20fd3b851";

// B2's own copies of the four V2 identity strings the artifact declares.
const ACCEPTED_TRAJECTORY_IDENTITY_V2: &str = "mtg-kernel-native-full-episode-trajectory-sha256-v2";
const ACCEPTED_GOLDENS_SCHEMA_V2: &str = "mtg_kernel_native_full_episode_trajectory_v2_goldens/v1";
const ACCEPTED_GOLDENS_GENERATOR_IDENTITY_V2: &str =
    "mtg-kernel-native-full-episode-trajectory-v2-goldens-stdlib-python-v1";
const ACCEPTED_GOLDEN_STREAM_IDENTITY_V2: &str =
    "mtg-kernel-native-full-episode-trajectory-v2-golden-vector-stream-sha256-v1";

// -------------------------------------------------- B2's own fixture literals
//
// These are the accepted generator's fixture constants, transcribed.  They exist
// so that the two positive baselines, and therefore all twenty-seven rejection
// reconstructions built from them, are derived from this module's literals
// rather than from the artifact's bytes.

const ACCEPTED_BURN_DECK_ID_V2: &str = "Burn";
const ACCEPTED_RALLY_DECK_ID_V2: &str = "Rally";
const ACCEPTED_BURN_DECK_HASH_V2: u64 = 0x5fdb_7b92_986b_6fc1;
const ACCEPTED_RALLY_DECK_HASH_V2: u64 = 0x0c9f_01c2_5444_12bf;
const ACCEPTED_NATIVE_ROOT_V2: u64 = 5_293_664_275_683_392_565;
const ACCEPTED_NATIVE_BASE_SEED_V2: u64 = 71_501;
const ACCEPTED_NATIVE_PAIR_INDEX_V2: u64 = 0;
const ACCEPTED_ACTION_SEED_BASE_V2: u64 = 0x5eed_0000;
const ACCEPTED_COMMITMENT_BASE_V2: u64 = 0x00c0_0000;
const ACCEPTED_LEGAL_ACTION_COUNT_V2: u32 = 4;

/// `(actor seat, substep count)` for each physical group of the even and odd
/// baselines.  Both roles, a multi-substep group, and a trailing single-substep
/// group appear in each.
const ACCEPTED_EVEN_GROUPS_V2: [(&str, u32); 3] = [("p0", 3), ("p1", 1), ("p0", 1)];
const ACCEPTED_ODD_GROUPS_V2: [(&str, u32); 3] = [("p0", 1), ("p1", 2), ("p0", 1)];

/// The two positive baselines, named by literal.  T0 is the trajectory reject
/// base; P0 is the pair reject base.
const T0_POSITIVE_CASE_NAME_V2: &str = "episode-0-native-root-learner-p0-p0-win";
const P0_PAIR_POSITIVE_CASE_NAME_V2: &str = "pair-native-base-71501-index-0";

/// The exact ordered trajectory reject names, as the accepted generator emits
/// them after its name sort.  The reconstruction table is keyed by these names.
const TRAJECTORY_REJECT_NAMES_V2: [&str; V2_TRAJECTORY_REJECT_CASE_COUNT] = [
    "actor-role-mismatch",
    "authority-environment-namespace-drift",
    "compound-empty-stream-and-terminal-episode-mismatch",
    "compound-open-group-and-terminal-episode-mismatch",
    "deck-id-case-drift-burn",
    "deck-id-unknown-not-in-catalog",
    "empty-decision-stream",
    "episode-index-two-pow-63",
    "incomplete-physical-group",
    "learner-seat-parity-mismatch",
    "legal-action-count-sixty-five",
    "legal-action-count-zero",
    "malformed-commitment-short",
    "malformed-group-substep-index-at-count",
    "non-natural-terminal",
    "row-ordinal-mismatch",
    "runtime-deck-hash-mismatch-p1",
    "selected-index-equal-width",
    "terminal-count-mismatch",
    "terminal-deck-provenance-mismatch",
    "terminal-episode-mismatch",
];

/// The exact ordered pair reject names.
const PAIR_REJECT_NAMES_V2: [&str; V2_PAIR_REJECT_CASE_COUNT] = [
    "pair-base-seed-two-pow-63",
    "pair-index-two-pow-62",
    "pair-learner-seat-not-swapped",
    "pair-odd-episode-index-drift",
    "pair-odd-physical-deck-swap",
    "pair-odd-root-drift",
];

/// The eleven inner-provenance override names the V2 input shape forbids, in the
/// accepted Phase A order.  The V2 envelope owns its inner accumulator, so any
/// field that could supply an inner digest, inner root, inner seat, inner deck
/// binding, or inner episode index from outside is a laundering vector.  Each
/// must be an unknown field at the typed boundary.
const FORBIDDEN_INNER_OVERRIDE_NAMES_V2: [&str; 11] = [
    "inner_trajectory_sha256",
    "inner_trajectory_sha256_raw32",
    "inner_environment_seed_u64_hex",
    "inner_root_u64_hex",
    "inner_pair_environment_seed_u64_hex",
    "inner_learner_seat",
    "inner_deck_p0_id",
    "inner_deck_p0_hash_u64_hex",
    "inner_deck_p1_id",
    "inner_deck_p1_hash_u64_hex",
    "inner_episode_index_u64_hex",
];

// ----------------------------------------------------- strict typed artifact shapes
//
// Every object is exact: `deny_unknown_fields` plus a full field list makes both
// an unknown key and a missing key a decode failure.  Integer domains are the
// artifact's own: the four `*_u32` decision fields stay `u32` and are never
// widened to `u64` or relaxed to a string.

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GoldenArtifactV2 {
    generator_identity: String,
    pair_positive_cases: Vec<PairPositiveCaseV2>,
    pair_reject_cases: Vec<PairRejectCaseV2>,
    positive_cases: Vec<PositiveCaseV2>,
    schema: String,
    source_authorities: SourceAuthoritiesV2,
    trajectory_identity: String,
    trajectory_reject_cases: Vec<TrajectoryRejectCaseV2>,
    vector_stream_identity: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceAuthoritiesV2 {
    environment_randomization: EnvironmentRandomizationAuthorityV2,
    inner_trajectory: InnerTrajectoryAuthorityV2,
    reset_trajectory: ResetTrajectoryAuthorityV2,
    runtime_deck_catalog: RuntimeDeckCatalogAuthorityV2,
    trainer_schedule: TrainerScheduleAuthorityV2,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentRandomizationAuthorityV2 {
    identity: String,
    kdf_goldens_raw_file_sha256: String,
    kdf_goldens_schema: String,
    namespace: String,
    python_reference_raw_file_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InnerTrajectoryAuthorityV2 {
    golden_semantic_stream_identity: String,
    golden_semantic_stream_sha256: String,
    goldens_generator_identity: String,
    goldens_raw_file_sha256: String,
    goldens_schema: String,
    identity: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ResetTrajectoryAuthorityV2 {
    generator_identity: String,
    goldens_raw_file_sha256: String,
    goldens_schema: String,
    physical_projection_identity: String,
    portable_semantic_stream_identity: String,
    portable_semantic_stream_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RuntimeDeckCatalogAuthorityV2 {
    catalog_raw_file_sha256: String,
    deck_hash_algorithm: String,
    materialization_protocol: String,
    protocol: String,
    schema: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TrainerScheduleAuthorityV2 {
    goldens_raw_file_sha256: String,
    goldens_schema: String,
    identity: String,
    seed_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PositiveCaseV2 {
    inner_sha256: String,
    inner_stream_hex: String,
    input: TrajectoryInputV2,
    name: String,
    v2_sha256: String,
    v2_stream_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PairPositiveCaseV2 {
    even_trajectory_sha256: String,
    input: PairInputV2,
    name: String,
    odd_trajectory_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TrajectoryRejectCaseV2 {
    expected_code: String,
    input: TrajectoryInputV2,
    name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PairRejectCaseV2 {
    expected_code: String,
    input: PairInputV2,
    name: String,
}

/// The V2 trajectory input carries no inner digest and no inner-root,
/// inner-seat, or inner-deck override field.  Adding one would be an unknown
/// field here and a decode failure.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TrajectoryInputV2 {
    decisions: Vec<DecisionRowV2>,
    deck_p0_hash_u64_hex: String,
    deck_p0_id: String,
    deck_p1_hash_u64_hex: String,
    deck_p1_id: String,
    episode_index_u64_hex: String,
    learner_seat: String,
    pair_environment_seed_u64_hex: String,
    source_authorities: SourceAuthoritiesV2,
    terminal: TerminalRowV2,
}

/// Exactly eleven fields.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DecisionRowV2 {
    action_seed_u64_hex: String,
    actor_physical_decision_ordinal_u64_hex: String,
    actor_role: String,
    actor_seat: String,
    flat_action_v2_commitment_hex: String,
    legal_action_count_u32: u32,
    physical_decision_ordinal_u64_hex: String,
    row_ordinal_u64_hex: String,
    selected_index_u32: u32,
    substep_count_u32: u32,
    substep_index_u32: u32,
}

/// Exactly nine fields.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TerminalRowV2 {
    classification: String,
    deck_p0_hash_u64_hex: String,
    deck_p1_hash_u64_hex: String,
    episode_index_u64_hex: String,
    outcome: String,
    physical_decision_count_u64_hex: String,
    policy_step_count_u64_hex: String,
    terminal_code: String,
    winner: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PairInputV2 {
    base_seed_u64_hex: String,
    even_start: TrajectoryInputV2,
    odd_start: TrajectoryInputV2,
    pair_index_u64_hex: String,
}

/// The exact top-level shape of the trainer-schedule goldens artifact.  The two
/// opaque members are deliberately not re-specified here: this module owns the
/// schedule *schema binding*, not the schedule contract, and re-declaring the
/// vectors would duplicate an authority that already has its own owner.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrainerScheduleArtifactV2 {
    python_reference_seed_version: String,
    schedule_version: String,
    schema: String,
    str_atom_probe: serde_json::Value,
    vectors: serde_json::Value,
}

// ============================================================== layer B: raw scanner
//
// A hand-written recursive scanner over the raw document bytes.  It exists in
// this form on purpose: routing through `serde_json::Value` would collapse
// duplicate keys under last-key-wins before anything could observe them, and
// building the scanner *on* serde would make every scanner test vacuous the
// moment serde itself rejected the input for an unrelated reason.  Nothing here
// touches serde, so the scanner tests below prove the scanner.
//
// It rejects, at every nesting depth: a repeated key inside one object, any
// number token carrying a fraction or an exponent, and every nonfinite spelling.
// The project's canonical JSON helper is not used, because the accepted V2
// stream strings are far longer than its four-kibibyte decoded string cap.

/// Nesting deeper than this is rejected rather than recursed into.  The accepted
/// artifact nests six levels.
const MAX_RAW_SCAN_DEPTH_V2: usize = 64;

/// Every nonfinite spelling the scanner refuses at a value position.  JSON has
/// no nonfinite literal, so each of these is a rejection and not a parse.
const NONFINITE_SPELLINGS_V2: [&str; 12] = [
    "-Infinity",
    "-infinity",
    "-Inf",
    "-inf",
    "-NaN",
    "-nan",
    "Infinity",
    "infinity",
    "Inf",
    "inf",
    "NaN",
    "nan",
];

struct RawScannerV2<'a> {
    bytes: &'a [u8],
    index: usize,
    depth: usize,
}

impl<'a> RawScannerV2<'a> {
    fn new_v2(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            index: 0,
            depth: 0,
        }
    }

    fn peek_v2(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    fn bump_v2(&mut self) -> Option<u8> {
        let byte = self.peek_v2()?;
        self.index += 1;
        Some(byte)
    }

    fn skip_whitespace_v2(&mut self) {
        while matches!(self.peek_v2(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.index += 1;
        }
    }

    fn expect_v2(&mut self, byte: u8) -> Result<(), String> {
        if self.bump_v2() == Some(byte) {
            Ok(())
        } else {
            Err(format!(
                "expected {:?} at offset {}",
                char::from(byte),
                self.index
            ))
        }
    }

    fn nonfinite_spelling_v2(&self) -> Option<&'static str> {
        NONFINITE_SPELLINGS_V2
            .iter()
            .copied()
            .find(|spelling| self.bytes[self.index..].starts_with(spelling.as_bytes()))
    }

    fn scan_document_v2(&mut self) -> Result<(), String> {
        self.skip_whitespace_v2();
        self.scan_value_v2()?;
        self.skip_whitespace_v2();
        if self.index != self.bytes.len() {
            return Err(format!(
                "trailing bytes after the top-level value at offset {}",
                self.index
            ));
        }
        Ok(())
    }

    fn scan_value_v2(&mut self) -> Result<(), String> {
        if self.depth >= MAX_RAW_SCAN_DEPTH_V2 {
            return Err(format!(
                "document nests deeper than the scan depth cap of {MAX_RAW_SCAN_DEPTH_V2}"
            ));
        }
        if let Some(spelling) = self.nonfinite_spelling_v2() {
            return Err(format!(
                "nonfinite numeric literal {spelling:?} is not permitted at offset {}",
                self.index
            ));
        }
        match self
            .peek_v2()
            .ok_or_else(|| format!("unexpected end of document at offset {}", self.index))?
        {
            b'{' => self.scan_object_v2(),
            b'[' => self.scan_array_v2(),
            b'"' => self.scan_string_v2().map(|_| ()),
            b't' => self.scan_literal_v2("true"),
            b'f' => self.scan_literal_v2("false"),
            b'n' => self.scan_literal_v2("null"),
            _ => self.scan_number_v2(),
        }
    }

    fn scan_literal_v2(&mut self, literal: &str) -> Result<(), String> {
        if self.bytes[self.index..].starts_with(literal.as_bytes()) {
            self.index += literal.len();
            Ok(())
        } else {
            Err(format!(
                "expected the literal {literal} at offset {}",
                self.index
            ))
        }
    }

    /// Integers only.  A fraction or an exponent is a float literal, and a float
    /// literal is rejected wherever it appears.
    fn scan_number_v2(&mut self) -> Result<(), String> {
        let start = self.index;
        if self.peek_v2() == Some(b'-') {
            self.index += 1;
        }
        let digits_start = self.index;
        while matches!(self.peek_v2(), Some(b'0'..=b'9')) {
            self.index += 1;
        }
        if self.index == digits_start {
            return Err(format!("not a JSON value at offset {start}"));
        }
        if self.bytes[digits_start] == b'0' && self.index - digits_start > 1 {
            return Err(format!(
                "number has a leading zero at offset {digits_start}"
            ));
        }
        match self.peek_v2() {
            Some(b'.') => Err(format!(
                "float literal: a fraction is not permitted at offset {}",
                self.index
            )),
            Some(b'e' | b'E') => Err(format!(
                "float literal: an exponent is not permitted at offset {}",
                self.index
            )),
            _ => Ok(()),
        }
    }

    /// Decodes a string so that two spellings of one key, such as `a` and its
    /// escape form, collide as duplicates.  Layer C proves the document is ASCII
    /// before the scanner runs, so a raw byte is its own code point here.
    fn scan_string_v2(&mut self) -> Result<String, String> {
        self.expect_v2(b'"')?;
        let mut decoded = String::new();
        loop {
            let byte = self
                .bump_v2()
                .ok_or_else(|| "unterminated string".to_string())?;
            match byte {
                b'"' => return Ok(decoded),
                b'\\' => {
                    let escape = self
                        .bump_v2()
                        .ok_or_else(|| "unterminated string escape".to_string())?;
                    match escape {
                        b'"' => decoded.push('"'),
                        b'\\' => decoded.push('\\'),
                        b'/' => decoded.push('/'),
                        b'b' => decoded.push('\u{8}'),
                        b'f' => decoded.push('\u{c}'),
                        b'n' => decoded.push('\n'),
                        b'r' => decoded.push('\r'),
                        b't' => decoded.push('\t'),
                        b'u' => {
                            let mut code = 0_u32;
                            for _ in 0..4 {
                                let digit = self
                                    .bump_v2()
                                    .ok_or_else(|| "truncated \\u escape".to_string())?;
                                let nibble = any_case_hex_nibble_v2(digit)
                                    .ok_or_else(|| "invalid \\u escape digit".to_string())?;
                                code = (code << 4) | u32::from(nibble);
                            }
                            decoded.push(
                                char::from_u32(code)
                                    .ok_or_else(|| "invalid \\u code point".to_string())?,
                            );
                        }
                        other => {
                            return Err(format!("invalid string escape {:?}", char::from(other)))
                        }
                    }
                }
                other => decoded.push(char::from(other)),
            }
        }
    }

    fn scan_object_v2(&mut self) -> Result<(), String> {
        self.expect_v2(b'{')?;
        self.depth += 1;
        let mut seen: BTreeSet<String> = BTreeSet::new();
        self.skip_whitespace_v2();
        if self.peek_v2() == Some(b'}') {
            self.index += 1;
            self.depth -= 1;
            return Ok(());
        }
        loop {
            self.skip_whitespace_v2();
            let key = self.scan_string_v2()?;
            if !seen.insert(key.clone()) {
                return Err(format!("duplicate object key {key:?}"));
            }
            self.skip_whitespace_v2();
            self.expect_v2(b':')?;
            self.skip_whitespace_v2();
            self.scan_value_v2()?;
            self.skip_whitespace_v2();
            match self.bump_v2() {
                Some(b',') => continue,
                Some(b'}') => {
                    self.depth -= 1;
                    return Ok(());
                }
                _ => {
                    return Err(format!(
                        "malformed object: expected ',' or '}}' at offset {}",
                        self.index
                    ))
                }
            }
        }
    }

    fn scan_array_v2(&mut self) -> Result<(), String> {
        self.expect_v2(b'[')?;
        self.depth += 1;
        self.skip_whitespace_v2();
        if self.peek_v2() == Some(b']') {
            self.index += 1;
            self.depth -= 1;
            return Ok(());
        }
        loop {
            self.skip_whitespace_v2();
            self.scan_value_v2()?;
            self.skip_whitespace_v2();
            match self.bump_v2() {
                Some(b',') => continue,
                Some(b']') => {
                    self.depth -= 1;
                    return Ok(());
                }
                _ => {
                    return Err(format!(
                        "malformed array: expected ',' or ']' at offset {}",
                        self.index
                    ))
                }
            }
        }
    }
}

/// Layer B's entry point.
fn scan_raw_document_v2(raw: &[u8]) -> Result<(), String> {
    RawScannerV2::new_v2(raw).scan_document_v2()
}

// --------------------------------------------------------- canonical bytes and atoms

/// Compact, sorted-key JSON with no trailing LF.  `serde_json::Map` is a
/// `BTreeMap` here (the `preserve_order` feature is off), so serializing through
/// `Value` reproduces the generator's
/// `sort_keys=True, separators=(",", ":"), ensure_ascii=True` exactly: the
/// artifact is pure ASCII, so the ASCII-escaping difference is a no-op.
fn canonical_json_no_lf_v2<T: Serialize>(value: &T) -> Vec<u8> {
    let as_value = serde_json::to_value(value).expect("typed golden value converts to JSON");
    serde_json::to_vec(&as_value).expect("JSON value serializes canonically")
}

/// Canonical JSON with exactly one final LF: the artifact's own file form.
fn canonical_file_bytes_v2<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = canonical_json_no_lf_v2(value);
    bytes.push(b'\n');
    bytes
}

/// The frozen framing shared by the inner V1 contract, the V2 envelope, and the
/// portable golden stream: `u32be(tag_len) || tag || u64be(payload_len) || payload`.
fn atom_v2(tag: &str, payload: &[u8]) -> Vec<u8> {
    let tag_len = u32::try_from(tag.len()).expect("atom tag length fits u32");
    let payload_len = u64::try_from(payload.len()).expect("atom payload length fits u64");
    let mut out = Vec::with_capacity(12 + tag.len() + payload.len());
    out.extend_from_slice(&tag_len.to_be_bytes());
    out.extend_from_slice(tag.as_bytes());
    out.extend_from_slice(&payload_len.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Accumulates atoms and counts them, so a frozen atom count is proof and not a
/// comment.
struct AtomStreamV2 {
    bytes: Vec<u8>,
    atom_count: usize,
}

impl AtomStreamV2 {
    fn new_v2() -> Self {
        Self {
            bytes: Vec::new(),
            atom_count: 0,
        }
    }

    fn push_v2(&mut self, tag: &str, payload: &[u8]) {
        self.bytes.extend_from_slice(&atom_v2(tag, payload));
        self.atom_count += 1;
    }
}

fn u32be_v2(value: usize) -> [u8; 4] {
    u32::try_from(value)
        .expect("golden case count fits u32")
        .to_be_bytes()
}

fn sha256_hex_v2(payload: &[u8]) -> String {
    Sha256::digest(payload)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Substring search over raw bytes.  Payload *equality* would miss a pin that
/// was smuggled inside a larger committed payload, so every self-pin proof below
/// searches instead of compares.
fn contains_subslice_v2(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && needle.len() <= haystack.len()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

// ------------------------------------------------------------------ hex helpers

fn hex_nibble_v2(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

/// Only the `\u` escape decoder accepts uppercase; every artifact hex field is
/// lowercase-only.
fn any_case_hex_nibble_v2(value: u8) -> Option<u8> {
    match value {
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => hex_nibble_v2(value),
    }
}

fn is_lower_hex_v2(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn parse_fixed_hex_v2<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 || !is_lower_hex_v2(value) {
        return None;
    }
    let mut output = [0_u8; N];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex_nibble_v2(chunk[0])? << 4) | hex_nibble_v2(chunk[1])?;
    }
    Some(output)
}

fn parse_hex_vec_v2(value: &str) -> Option<Vec<u8>> {
    if value.is_empty() || (value.len() & 1) != 0 || !is_lower_hex_v2(value) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|chunk| Some((hex_nibble_v2(chunk[0])? << 4) | hex_nibble_v2(chunk[1])?))
        .collect()
}

fn raw32_v2(pin: &str) -> [u8; 32] {
    parse_fixed_hex_v2::<32>(pin).expect("a SHA-256 pin is exactly sixty-four lowercase hex digits")
}

fn u64_hex_v2(value: u64) -> String {
    format!("{value:016x}")
}

/// Flips the final hex nibble of a value, preserving its length and its lowercase
/// hex shape.  A same-shape drift survives every shape rule, so only a real
/// comparison against a recomputed value can catch it.
fn drift_last_nibble_v2(value: &str) -> String {
    let mut bytes = value.as_bytes().to_vec();
    let last = bytes.len() - 1;
    bytes[last] = if bytes[last] == b'0' { b'1' } else { b'0' };
    String::from_utf8(bytes).expect("a hex value stays ASCII under a nibble flip")
}

/// Splits a frozen atom stream back into `(tag, payload)` pairs, rejecting
/// truncated framing and trailing bytes.  This reads the framing only: it never
/// re-serialises a V1 decision or terminal payload, so it cannot become a second
/// copy of the inner row serialiser.
fn decompose_atom_stream_v2(bytes: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    let mut atoms = Vec::new();
    let mut index = 0_usize;
    while index < bytes.len() {
        let tag_len = u32::from_be_bytes(bytes.get(index..index + 4)?.try_into().ok()?) as usize;
        index += 4;
        let tag = std::str::from_utf8(bytes.get(index..index + tag_len)?)
            .ok()?
            .to_string();
        index += tag_len;
        let payload_len =
            u64::from_be_bytes(bytes.get(index..index + 8)?.try_into().ok()?) as usize;
        index += 8;
        let payload = bytes.get(index..index + payload_len)?.to_vec();
        index += payload_len;
        atoms.push((tag, payload));
    }
    Some(atoms)
}

fn parse_u64_hex_v2(value: &str) -> Result<u64, GoldenRunErrorV2> {
    parse_fixed_hex_v2::<8>(value)
        .map(u64::from_be_bytes)
        .ok_or(GoldenRunErrorV2::InvalidFixture(
            "value is not exactly sixteen lowercase hex digits",
        ))
}

fn printable_ascii_with_max_v2(value: &str, maximum: usize) -> bool {
    value.len() <= maximum && value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

fn is_case_name_v2(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

// ------------------------------------------------- the twenty-six declared facts
//
// The declaration validator compares an authority object, field by field,
// against this module's own literals.  The table is used twice: layer D applies
// it to the artifact's top-level authority block, and the trajectory runner
// applies it to a case's own block, where drift is the production-owned
// `AuthorityMismatch`.  It is also the negative-test surface: every one of the
// twenty-six paths is mutated in isolation on both surfaces.

const DECLARED_AUTHORITY_FIELD_COUNT_V2: usize = 26;

fn declared_authority_pairs_v2(
    declared: &SourceAuthoritiesV2,
) -> [(&'static str, &str, &'static str); DECLARED_AUTHORITY_FIELD_COUNT_V2] {
    let environment = &declared.environment_randomization;
    let inner = &declared.inner_trajectory;
    let reset = &declared.reset_trajectory;
    let runtime = &declared.runtime_deck_catalog;
    let trainer = &declared.trainer_schedule;
    [
        (
            "inner_trajectory.identity",
            inner.identity.as_str(),
            ACCEPTED_INNER_IDENTITY_V2,
        ),
        (
            "inner_trajectory.goldens_schema",
            inner.goldens_schema.as_str(),
            ACCEPTED_INNER_GOLDENS_SCHEMA_V2,
        ),
        (
            "inner_trajectory.goldens_generator_identity",
            inner.goldens_generator_identity.as_str(),
            ACCEPTED_INNER_GOLDENS_GENERATOR_IDENTITY_V2,
        ),
        (
            "inner_trajectory.goldens_raw_file_sha256",
            inner.goldens_raw_file_sha256.as_str(),
            ACCEPTED_INNER_GOLDENS_FILE_SHA256_V2,
        ),
        (
            "inner_trajectory.golden_semantic_stream_identity",
            inner.golden_semantic_stream_identity.as_str(),
            ACCEPTED_INNER_GOLDEN_STREAM_IDENTITY_V2,
        ),
        (
            "inner_trajectory.golden_semantic_stream_sha256",
            inner.golden_semantic_stream_sha256.as_str(),
            ACCEPTED_INNER_GOLDEN_STREAM_SHA256_V2,
        ),
        (
            "environment_randomization.identity",
            environment.identity.as_str(),
            ACCEPTED_ENVIRONMENT_IDENTITY_V2,
        ),
        (
            "environment_randomization.namespace",
            environment.namespace.as_str(),
            ACCEPTED_ENVIRONMENT_NAMESPACE_V2,
        ),
        (
            "environment_randomization.kdf_goldens_schema",
            environment.kdf_goldens_schema.as_str(),
            ACCEPTED_ENVIRONMENT_KDF_GOLDENS_SCHEMA_V2,
        ),
        (
            "environment_randomization.kdf_goldens_raw_file_sha256",
            environment.kdf_goldens_raw_file_sha256.as_str(),
            ACCEPTED_ENVIRONMENT_KDF_GOLDENS_FILE_SHA256_V2,
        ),
        (
            "environment_randomization.python_reference_raw_file_sha256",
            environment.python_reference_raw_file_sha256.as_str(),
            ACCEPTED_ENVIRONMENT_PYTHON_REFERENCE_FILE_SHA256_V2,
        ),
        (
            "reset_trajectory.goldens_schema",
            reset.goldens_schema.as_str(),
            ACCEPTED_RESET_GOLDENS_SCHEMA_V2,
        ),
        (
            "reset_trajectory.generator_identity",
            reset.generator_identity.as_str(),
            ACCEPTED_RESET_GENERATOR_IDENTITY_V2,
        ),
        (
            "reset_trajectory.physical_projection_identity",
            reset.physical_projection_identity.as_str(),
            ACCEPTED_RESET_PHYSICAL_PROJECTION_IDENTITY_V2,
        ),
        (
            "reset_trajectory.portable_semantic_stream_identity",
            reset.portable_semantic_stream_identity.as_str(),
            ACCEPTED_RESET_PORTABLE_STREAM_IDENTITY_V2,
        ),
        (
            "reset_trajectory.goldens_raw_file_sha256",
            reset.goldens_raw_file_sha256.as_str(),
            ACCEPTED_RESET_GOLDENS_FILE_SHA256_V2,
        ),
        (
            "reset_trajectory.portable_semantic_stream_sha256",
            reset.portable_semantic_stream_sha256.as_str(),
            ACCEPTED_RESET_PORTABLE_STREAM_SHA256_V2,
        ),
        (
            "trainer_schedule.identity",
            trainer.identity.as_str(),
            ACCEPTED_TRAINER_SCHEDULE_IDENTITY_V2,
        ),
        (
            "trainer_schedule.seed_version",
            trainer.seed_version.as_str(),
            ACCEPTED_TRAINER_SEED_VERSION_V2,
        ),
        (
            "trainer_schedule.goldens_schema",
            trainer.goldens_schema.as_str(),
            ACCEPTED_TRAINER_SCHEDULE_GOLDENS_SCHEMA_V2,
        ),
        (
            "trainer_schedule.goldens_raw_file_sha256",
            trainer.goldens_raw_file_sha256.as_str(),
            ACCEPTED_TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_V2,
        ),
        (
            "runtime_deck_catalog.schema",
            runtime.schema.as_str(),
            ACCEPTED_RUNTIME_DECK_CATALOG_SCHEMA_V2,
        ),
        (
            "runtime_deck_catalog.protocol",
            runtime.protocol.as_str(),
            ACCEPTED_RUNTIME_DECK_PROTOCOL_V2,
        ),
        (
            "runtime_deck_catalog.materialization_protocol",
            runtime.materialization_protocol.as_str(),
            ACCEPTED_RUNTIME_DECK_MATERIALIZATION_PROTOCOL_V2,
        ),
        (
            "runtime_deck_catalog.deck_hash_algorithm",
            runtime.deck_hash_algorithm.as_str(),
            ACCEPTED_RUNTIME_DECK_HASH_ALGORITHM_V2,
        ),
        (
            "runtime_deck_catalog.catalog_raw_file_sha256",
            runtime.catalog_raw_file_sha256.as_str(),
            ACCEPTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2,
        ),
    ]
}

/// The twenty-six accepted authority values as an owned object, built purely
/// from this module's literals.  The two positive baselines are rebuilt around
/// it, so no reconstructed fixture borrows an authority value from the artifact.
fn accepted_source_authorities_v2() -> SourceAuthoritiesV2 {
    SourceAuthoritiesV2 {
        environment_randomization: EnvironmentRandomizationAuthorityV2 {
            identity: ACCEPTED_ENVIRONMENT_IDENTITY_V2.to_string(),
            kdf_goldens_raw_file_sha256: ACCEPTED_ENVIRONMENT_KDF_GOLDENS_FILE_SHA256_V2
                .to_string(),
            kdf_goldens_schema: ACCEPTED_ENVIRONMENT_KDF_GOLDENS_SCHEMA_V2.to_string(),
            namespace: ACCEPTED_ENVIRONMENT_NAMESPACE_V2.to_string(),
            python_reference_raw_file_sha256: ACCEPTED_ENVIRONMENT_PYTHON_REFERENCE_FILE_SHA256_V2
                .to_string(),
        },
        inner_trajectory: InnerTrajectoryAuthorityV2 {
            golden_semantic_stream_identity: ACCEPTED_INNER_GOLDEN_STREAM_IDENTITY_V2.to_string(),
            golden_semantic_stream_sha256: ACCEPTED_INNER_GOLDEN_STREAM_SHA256_V2.to_string(),
            goldens_generator_identity: ACCEPTED_INNER_GOLDENS_GENERATOR_IDENTITY_V2.to_string(),
            goldens_raw_file_sha256: ACCEPTED_INNER_GOLDENS_FILE_SHA256_V2.to_string(),
            goldens_schema: ACCEPTED_INNER_GOLDENS_SCHEMA_V2.to_string(),
            identity: ACCEPTED_INNER_IDENTITY_V2.to_string(),
        },
        reset_trajectory: ResetTrajectoryAuthorityV2 {
            generator_identity: ACCEPTED_RESET_GENERATOR_IDENTITY_V2.to_string(),
            goldens_raw_file_sha256: ACCEPTED_RESET_GOLDENS_FILE_SHA256_V2.to_string(),
            goldens_schema: ACCEPTED_RESET_GOLDENS_SCHEMA_V2.to_string(),
            physical_projection_identity: ACCEPTED_RESET_PHYSICAL_PROJECTION_IDENTITY_V2
                .to_string(),
            portable_semantic_stream_identity: ACCEPTED_RESET_PORTABLE_STREAM_IDENTITY_V2
                .to_string(),
            portable_semantic_stream_sha256: ACCEPTED_RESET_PORTABLE_STREAM_SHA256_V2.to_string(),
        },
        runtime_deck_catalog: RuntimeDeckCatalogAuthorityV2 {
            catalog_raw_file_sha256: ACCEPTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2.to_string(),
            deck_hash_algorithm: ACCEPTED_RUNTIME_DECK_HASH_ALGORITHM_V2.to_string(),
            materialization_protocol: ACCEPTED_RUNTIME_DECK_MATERIALIZATION_PROTOCOL_V2.to_string(),
            protocol: ACCEPTED_RUNTIME_DECK_PROTOCOL_V2.to_string(),
            schema: ACCEPTED_RUNTIME_DECK_CATALOG_SCHEMA_V2.to_string(),
        },
        trainer_schedule: TrainerScheduleAuthorityV2 {
            goldens_raw_file_sha256: ACCEPTED_TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_V2.to_string(),
            goldens_schema: ACCEPTED_TRAINER_SCHEDULE_GOLDENS_SCHEMA_V2.to_string(),
            identity: ACCEPTED_TRAINER_SCHEDULE_IDENTITY_V2.to_string(),
            seed_version: ACCEPTED_TRAINER_SEED_VERSION_V2.to_string(),
        },
    }
}

/// Declaration drift returns the production-owned `AuthorityMismatch`.  This is
/// the only authority check a case input can influence; production's own live
/// guard, which no caller can parameterise, runs separately inside every
/// constructing entry point.
fn validate_declared_authorities_v2(
    declared: &SourceAuthoritiesV2,
) -> Result<(), NativeFullEpisodeTrajectoryErrorV2> {
    if declared_authority_pairs_v2(declared)
        .iter()
        .any(|(_, declared_value, accepted)| declared_value != accepted)
    {
        return Err(NativeFullEpisodeTrajectoryErrorV2::AuthorityMismatch);
    }
    Ok(())
}

/// The twenty-four B2 literals that a live owner module also exports, paired with
/// that live constant.  The two metadata-only facts are absent by construction:
/// they are proven against live bytes instead, below.
fn live_owner_pairs_v2() -> [(&'static str, &'static str, &'static str); 24] {
    [
        (
            "inner_trajectory.identity",
            NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V1,
            ACCEPTED_INNER_IDENTITY_V2,
        ),
        (
            "inner_trajectory.goldens_schema",
            NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_SCHEMA_V1,
            ACCEPTED_INNER_GOLDENS_SCHEMA_V2,
        ),
        (
            "inner_trajectory.goldens_generator_identity",
            NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V1,
            ACCEPTED_INNER_GOLDENS_GENERATOR_IDENTITY_V2,
        ),
        (
            "inner_trajectory.golden_semantic_stream_identity",
            NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V1,
            ACCEPTED_INNER_GOLDEN_STREAM_IDENTITY_V2,
        ),
        (
            "inner_trajectory.goldens_raw_file_sha256",
            NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_FILE_SHA256_V1,
            ACCEPTED_INNER_GOLDENS_FILE_SHA256_V2,
        ),
        (
            "inner_trajectory.golden_semantic_stream_sha256",
            NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_SHA256_V1,
            ACCEPTED_INNER_GOLDEN_STREAM_SHA256_V2,
        ),
        (
            "environment_randomization.identity",
            ENVIRONMENT_RANDOMIZATION_IDENTITY_V2,
            ACCEPTED_ENVIRONMENT_IDENTITY_V2,
        ),
        (
            "environment_randomization.namespace",
            ENVIRONMENT_RANDOMIZATION_NAMESPACE_V2,
            ACCEPTED_ENVIRONMENT_NAMESPACE_V2,
        ),
        (
            "environment_randomization.kdf_goldens_schema",
            ENVIRONMENT_RANDOMIZATION_GOLDENS_SCHEMA_V1,
            ACCEPTED_ENVIRONMENT_KDF_GOLDENS_SCHEMA_V2,
        ),
        (
            "environment_randomization.kdf_goldens_raw_file_sha256",
            ENVIRONMENT_RANDOMIZATION_GOLDENS_SHA256_V1,
            ACCEPTED_ENVIRONMENT_KDF_GOLDENS_FILE_SHA256_V2,
        ),
        (
            "reset_trajectory.goldens_schema",
            ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_SCHEMA_V1,
            ACCEPTED_RESET_GOLDENS_SCHEMA_V2,
        ),
        (
            "reset_trajectory.generator_identity",
            ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GENERATOR_IDENTITY_V1,
            ACCEPTED_RESET_GENERATOR_IDENTITY_V2,
        ),
        (
            "reset_trajectory.physical_projection_identity",
            ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PHYSICAL_PROJECTION_IDENTITY_V1,
            ACCEPTED_RESET_PHYSICAL_PROJECTION_IDENTITY_V2,
        ),
        (
            "reset_trajectory.portable_semantic_stream_identity",
            ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PORTABLE_VECTOR_STREAM_IDENTITY_V1,
            ACCEPTED_RESET_PORTABLE_STREAM_IDENTITY_V2,
        ),
        (
            "reset_trajectory.goldens_raw_file_sha256",
            ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_SHA256_V1,
            ACCEPTED_RESET_GOLDENS_FILE_SHA256_V2,
        ),
        (
            "reset_trajectory.portable_semantic_stream_sha256",
            ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PORTABLE_VECTOR_STREAM_SHA256_V1,
            ACCEPTED_RESET_PORTABLE_STREAM_SHA256_V2,
        ),
        (
            "trainer_schedule.identity",
            NATIVE_TRAINER_SCHEDULE_VERSION_V1,
            ACCEPTED_TRAINER_SCHEDULE_IDENTITY_V2,
        ),
        (
            "trainer_schedule.seed_version",
            PYTHON_REFERENCE_SEED_VERSION_V1,
            ACCEPTED_TRAINER_SEED_VERSION_V2,
        ),
        (
            "trainer_schedule.goldens_raw_file_sha256",
            NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1,
            ACCEPTED_TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_V2,
        ),
        (
            "runtime_deck_catalog.schema",
            RUNTIME_DECK_CATALOG_SCHEMA,
            ACCEPTED_RUNTIME_DECK_CATALOG_SCHEMA_V2,
        ),
        (
            "runtime_deck_catalog.protocol",
            RUNTIME_DECK_PROTOCOL,
            ACCEPTED_RUNTIME_DECK_PROTOCOL_V2,
        ),
        (
            "runtime_deck_catalog.materialization_protocol",
            RUNTIME_DECK_MATERIALIZATION_PROTOCOL,
            ACCEPTED_RUNTIME_DECK_MATERIALIZATION_PROTOCOL_V2,
        ),
        (
            "runtime_deck_catalog.deck_hash_algorithm",
            RUNTIME_DECK_HASH_ALGORITHM,
            ACCEPTED_RUNTIME_DECK_HASH_ALGORITHM_V2,
        ),
        (
            "runtime_deck_catalog.catalog_raw_file_sha256",
            RUNTIME_DECK_CATALOG_FILE_SHA256,
            ACCEPTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2,
        ),
    ]
}

/// The six live files whose SHA-256 the accepted contract pins.  Five back an
/// envelope authority; the sixth is the metadata-only environment Python source.
fn live_hashed_files_v2() -> [(&'static str, &'static [u8], &'static str); 6] {
    [
        (
            "inner_trajectory.goldens_raw_file_sha256",
            V1_TRAJECTORY_ARTIFACT_BYTES,
            ACCEPTED_INNER_GOLDENS_FILE_SHA256_V2,
        ),
        (
            "environment_randomization.kdf_goldens_raw_file_sha256",
            ENVIRONMENT_KDF_ARTIFACT_BYTES,
            ACCEPTED_ENVIRONMENT_KDF_GOLDENS_FILE_SHA256_V2,
        ),
        (
            "reset_trajectory.goldens_raw_file_sha256",
            RESET_TRAJECTORY_ARTIFACT_BYTES,
            ACCEPTED_RESET_GOLDENS_FILE_SHA256_V2,
        ),
        (
            "trainer_schedule.goldens_raw_file_sha256",
            TRAINER_SCHEDULE_ARTIFACT_BYTES,
            ACCEPTED_TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_V2,
        ),
        (
            "runtime_deck_catalog.catalog_raw_file_sha256",
            RUNTIME_DECK_CATALOG_BYTES,
            ACCEPTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2,
        ),
        (
            "environment_randomization.python_reference_raw_file_sha256",
            ENVIRONMENT_PYTHON_REFERENCE_BYTES,
            ACCEPTED_ENVIRONMENT_PYTHON_REFERENCE_FILE_SHA256_V2,
        ),
    ]
}

// ==================================================== layer D: semantic validation
//
// Names, counts, caps, hex widths, digest shapes, closed vocabularies, and the
// top-level twenty-six authorities.  It is invoked from both the sealed and the
// unsealed success paths, before any case is classified or executed.
//
// Order matters and is tested:
//
// 1. the *global cap stage*: all four case-list caps, then the decision cap on
//    every declared input across all six families and both pair sides;
// 2. the four V2 identity strings;
// 3. per list: emptiness, the name domain, and strict ordering;
// 4. the exact sealed counts;
// 5. the top-level twenty-six authorities against B2's literals;
// 6. per-case digest, stream, shape, and vocabulary rules.
//
// The cap stage is deliberately global and first.  Caps are the only rules whose
// violation means the artifact is too large to be reasoned about at all, so no
// per-list classification, no exact count pin, and no authority comparison may
// pre-empt them, and in particular a bad name or a bad order in an *early* family
// must not hide a cap violation in a *later* one.  That also keeps a cap-sized
// and a cap-plus-one artifact both reachable rather than pre-empted by the exact
// count pins.  Step 6 deliberately admits the semantically invalid rejection
// fixtures: a malformed commitment, an out-of-u63 episode index, an empty
// decision stream, and a non-natural terminal are production-owned rejections
// with their own portable codes, not fixture-shape violations.
fn validate_artifact_semantics_v2(artifact: &GoldenArtifactV2) -> Result<(), String> {
    // 1. The global cap stage.  Cap-only, with no classification mixed in.
    for (label, length) in [
        ("positive_cases", artifact.positive_cases.len()),
        ("pair_positive_cases", artifact.pair_positive_cases.len()),
        (
            "trajectory_reject_cases",
            artifact.trajectory_reject_cases.len(),
        ),
        ("pair_reject_cases", artifact.pair_reject_cases.len()),
    ] {
        validate_case_list_cap_v2(label, length)?;
    }
    for (label, input) in declared_trajectory_inputs_v2(artifact) {
        if input.decisions.len() > MAX_GOLDEN_DECISIONS_V2 {
            return Err(format!("{label}: decision cap exceeded"));
        }
    }

    if artifact.schema != ACCEPTED_GOLDENS_SCHEMA_V2 {
        return Err("artifact schema drift".to_string());
    }
    if artifact.generator_identity != ACCEPTED_GOLDENS_GENERATOR_IDENTITY_V2 {
        return Err("artifact generator identity drift".to_string());
    }
    if artifact.trajectory_identity != ACCEPTED_TRAJECTORY_IDENTITY_V2 {
        return Err("artifact trajectory identity drift".to_string());
    }
    if artifact.vector_stream_identity != ACCEPTED_GOLDEN_STREAM_IDENTITY_V2 {
        return Err("artifact vector stream identity drift".to_string());
    }

    validate_case_names_v2(
        artifact
            .positive_cases
            .iter()
            .map(|case| case.name.as_str()),
    )?;
    validate_case_names_v2(
        artifact
            .pair_positive_cases
            .iter()
            .map(|case| case.name.as_str()),
    )?;
    validate_case_names_v2(
        artifact
            .trajectory_reject_cases
            .iter()
            .map(|case| case.name.as_str()),
    )?;
    validate_case_names_v2(
        artifact
            .pair_reject_cases
            .iter()
            .map(|case| case.name.as_str()),
    )?;

    if artifact.positive_cases.len() != V2_POSITIVE_CASE_COUNT
        || artifact.pair_positive_cases.len() != V2_PAIR_POSITIVE_CASE_COUNT
        || artifact.trajectory_reject_cases.len() != V2_TRAJECTORY_REJECT_CASE_COUNT
        || artifact.pair_reject_cases.len() != V2_PAIR_REJECT_CASE_COUNT
    {
        return Err("artifact case count drift".to_string());
    }

    for (field, declared, accepted) in declared_authority_pairs_v2(&artifact.source_authorities) {
        if declared != accepted {
            return Err(format!("declared authority drift at {field}"));
        }
    }

    for case in &artifact.positive_cases {
        if parse_fixed_hex_v2::<32>(&case.inner_sha256).is_none()
            || parse_fixed_hex_v2::<32>(&case.v2_sha256).is_none()
        {
            return Err(format!("{}: digest is not lowercase raw32 hex", case.name));
        }
        if parse_hex_vec_v2(&case.inner_stream_hex).is_none()
            || parse_hex_vec_v2(&case.v2_stream_hex).is_none()
        {
            return Err(format!(
                "{}: stream is not nonempty even lowercase hex",
                case.name
            ));
        }
        validate_trajectory_input_shape_v2(&case.input, &case.name)?;
    }
    for case in &artifact.pair_positive_cases {
        if parse_fixed_hex_v2::<32>(&case.even_trajectory_sha256).is_none()
            || parse_fixed_hex_v2::<32>(&case.odd_trajectory_sha256).is_none()
        {
            return Err(format!("{}: digest is not lowercase raw32 hex", case.name));
        }
        validate_pair_input_shape_v2(&case.input, &case.name)?;
    }
    for case in &artifact.trajectory_reject_cases {
        if !closed_rejection_codes_v2().contains(&case.expected_code.as_str()) {
            return Err(format!("{}: code outside the closed vocabulary", case.name));
        }
        validate_trajectory_input_shape_v2(&case.input, &case.name)?;
    }
    for case in &artifact.pair_reject_cases {
        if !closed_rejection_codes_v2().contains(&case.expected_code.as_str()) {
            return Err(format!("{}: code outside the closed vocabulary", case.name));
        }
        validate_pair_input_shape_v2(&case.input, &case.name)?;
    }
    Ok(())
}

/// Every trajectory input the artifact declares, in every family, with both pair
/// sides enumerated separately.  The decision cap applies to all of them.
fn declared_trajectory_inputs_v2(artifact: &GoldenArtifactV2) -> Vec<(String, &TrajectoryInputV2)> {
    let mut inputs: Vec<(String, &TrajectoryInputV2)> = Vec::new();
    for case in &artifact.positive_cases {
        inputs.push((case.name.clone(), &case.input));
    }
    for case in &artifact.pair_positive_cases {
        inputs.push((format!("{}/even_start", case.name), &case.input.even_start));
        inputs.push((format!("{}/odd_start", case.name), &case.input.odd_start));
    }
    for case in &artifact.trajectory_reject_cases {
        inputs.push((case.name.clone(), &case.input));
    }
    for case in &artifact.pair_reject_cases {
        inputs.push((format!("{}/even_start", case.name), &case.input.even_start));
        inputs.push((format!("{}/odd_start", case.name), &case.input.odd_start));
    }
    inputs
}

/// The case cap alone, split out of name classification so that the whole cap
/// stage can run globally and first.  It takes a length, not the names, so that
/// nothing about a name can influence it.
fn validate_case_list_cap_v2(label: &str, length: usize) -> Result<(), String> {
    if length > MAX_GOLDEN_CASES_V2 {
        return Err(format!("{label}: case list exceeds the case cap"));
    }
    Ok(())
}

/// Emptiness, the name domain, and strict ordering.  The cap is *not* checked
/// here: it has already been applied globally, above, for every list.
fn validate_case_names_v2<'a>(names: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let names = names.collect::<Vec<_>>();
    if names.is_empty() {
        return Err("case list is empty".to_string());
    }
    if let Some(bad) = names.iter().find(|name| !is_case_name_v2(name)) {
        return Err(format!("case name {bad:?} is outside the name domain"));
    }
    if !names
        .windows(2)
        .all(|pair| pair[0].as_bytes() < pair[1].as_bytes())
    {
        return Err("case names are not strictly ascending".to_string());
    }
    Ok(())
}

/// Outer fixture shape only.  It deliberately does *not* check the u63 episode
/// domain, the deck catalog, the commitment length, or any counter rule: each of
/// those is a production-owned rejection with its own portable code, and a test
/// that pre-empted them here would report a fixture violation where the contract
/// requires a contract code.  The decision cap is applied earlier, for every
/// family at once.
fn validate_trajectory_input_shape_v2(
    input: &TrajectoryInputV2,
    label: &str,
) -> Result<(), String> {
    for value in [
        &input.episode_index_u64_hex,
        &input.pair_environment_seed_u64_hex,
        &input.deck_p0_hash_u64_hex,
        &input.deck_p1_hash_u64_hex,
        &input.terminal.episode_index_u64_hex,
        &input.terminal.deck_p0_hash_u64_hex,
        &input.terminal.deck_p1_hash_u64_hex,
        &input.terminal.policy_step_count_u64_hex,
        &input.terminal.physical_decision_count_u64_hex,
    ] {
        if parse_fixed_hex_v2::<8>(value).is_none() {
            return Err(format!("{label}: u64 field is not sixteen lowercase hex"));
        }
    }
    if !printable_ascii_with_max_v2(&input.deck_p0_id, 65)
        || !printable_ascii_with_max_v2(&input.deck_p1_id, 65)
    {
        return Err(format!("{label}: deck ID violates the outer ASCII bound"));
    }
    if input.learner_seat != "p0" && input.learner_seat != "p1" {
        return Err(format!("{label}: learner seat is outside its domain"));
    }
    if input.decisions.len() > MAX_GOLDEN_DECISIONS_V2 {
        return Err(format!("{label}: decision cap exceeded"));
    }
    for row in &input.decisions {
        for value in [
            &row.row_ordinal_u64_hex,
            &row.physical_decision_ordinal_u64_hex,
            &row.actor_physical_decision_ordinal_u64_hex,
            &row.action_seed_u64_hex,
        ] {
            if parse_fixed_hex_v2::<8>(value).is_none() {
                return Err(format!("{label}: u64 field is not sixteen lowercase hex"));
            }
        }
        if row.actor_seat != "p0" && row.actor_seat != "p1" {
            return Err(format!("{label}: actor seat is outside its domain"));
        }
        if row.actor_role != "learner" && row.actor_role != "opponent" {
            return Err(format!("{label}: actor role is outside its domain"));
        }
        if !printable_ascii_with_max_v2(&row.flat_action_v2_commitment_hex, 34) {
            return Err(format!(
                "{label}: commitment violates the outer ASCII bound"
            ));
        }
    }
    if !matches!(
        input.terminal.outcome.as_str(),
        "p0-win" | "p1-win" | "draw" | "truncated" | "halted"
    ) {
        return Err(format!("{label}: terminal outcome is outside its domain"));
    }
    if !matches!(input.terminal.winner.as_str(), "none" | "p0" | "p1") {
        return Err(format!("{label}: terminal winner is outside its domain"));
    }
    if !matches!(
        input.terminal.classification.as_str(),
        "natural" | "truncated" | "halted"
    ) {
        return Err(format!(
            "{label}: terminal classification is outside its domain"
        ));
    }
    if !matches!(
        input.terminal.terminal_code.as_str(),
        "natural-game-over" | "decision-cap" | "fail-closed"
    ) {
        return Err(format!("{label}: terminal code is outside its domain"));
    }
    Ok(())
}

fn validate_pair_input_shape_v2(input: &PairInputV2, label: &str) -> Result<(), String> {
    for value in [&input.base_seed_u64_hex, &input.pair_index_u64_hex] {
        if parse_fixed_hex_v2::<8>(value).is_none() {
            return Err(format!("{label}: u64 field is not sixteen lowercase hex"));
        }
    }
    validate_trajectory_input_shape_v2(&input.even_start, label)?;
    validate_trajectory_input_shape_v2(&input.odd_start, label)
}

// ================================================= layers A and C: the raw seam

/// The exact raw-byte gate.  Every rule is byte-level; nothing here interprets a
/// JSON token.  The BOM check precedes the ASCII check only so that a BOM reports
/// as a BOM rather than as generic non-ASCII, and the printable-body rule runs
/// last because it is meaningful only once exactly one final LF is proven: with
/// that proven, every remaining byte of the body must be printable ASCII, so a
/// control byte or a DEL is a rejection rather than something a parser might
/// silently tolerate.
fn validate_raw_bytes_v2(raw: &[u8]) -> Result<&str, String> {
    if raw.is_empty() {
        return Err("artifact is empty".to_string());
    }
    if raw.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err("artifact carries a UTF-8 BOM".to_string());
    }
    if !raw.is_ascii() {
        return Err("artifact is not ASCII".to_string());
    }
    if raw.contains(&b'\r') {
        return Err("artifact contains a carriage return".to_string());
    }
    if raw.last() != Some(&b'\n') {
        return Err("artifact does not end with exactly one LF".to_string());
    }
    if raw.iter().filter(|byte| **byte == b'\n').count() != 1 {
        return Err("artifact does not end with exactly one LF".to_string());
    }
    if let Some(offset) = raw[..raw.len() - 1]
        .iter()
        .position(|byte| !(0x20..=0x7e).contains(byte))
    {
        return Err(format!(
            "artifact body byte at offset {offset} is outside printable ASCII 0x20..=0x7e"
        ));
    }
    std::str::from_utf8(raw).map_err(|_| "artifact is not UTF-8".to_string())
}

/// Layer C: everything the sealed loader does apart from the two exact pins.  It
/// is kept separate so in-memory mutants, which cannot satisfy a length or digest
/// pin, still traverse the identical path.
///
/// Order: the reachable global size cap, the exact raw-byte gate, the layer B
/// scanner, typed serde, canonical sorted compact re-encoding plus one LF with
/// byte equality, then layer D.
fn load_unsealed_artifact_v2(raw: &[u8]) -> Result<GoldenArtifactV2, String> {
    if raw.len() > MAX_GOLDEN_ARTIFACT_BYTES_V2 {
        return Err("artifact exceeds the four-mebibyte cap".to_string());
    }
    let text = validate_raw_bytes_v2(raw)?;
    scan_raw_document_v2(text.as_bytes())?;
    let artifact: GoldenArtifactV2 =
        serde_json::from_str(text).map_err(|error| format!("typed decode failed: {error}"))?;
    if canonical_file_bytes_v2(&artifact) != raw {
        return Err("artifact is not canonical sorted compact JSON with one final LF".to_string());
    }
    validate_artifact_semantics_v2(&artifact)?;
    Ok(artifact)
}

/// Layer A: the sealed loader.  Length and digest are pinned before anything
/// interprets a byte, so a substituted artifact fails before it can influence a
/// parser; then layer C runs the whole unsealed path, and layer D is invoked
/// again explicitly on the sealed success path so that the semantic validator is
/// reachable from both entry points rather than declared and uncalled.
fn load_sealed_artifact_v2() -> GoldenArtifactV2 {
    assert_eq!(
        V2_ARTIFACT_BYTES.len(),
        V2_ARTIFACT_RAW_LEN,
        "the V2 golden artifact is pinned at an exact raw byte length"
    );
    assert_eq!(
        sha256_hex_v2(V2_ARTIFACT_BYTES),
        V2_ARTIFACT_RAW_SHA256,
        "the V2 golden artifact is pinned at an exact raw SHA-256"
    );
    let artifact = load_unsealed_artifact_v2(V2_ARTIFACT_BYTES)
        .expect("the accepted V2 artifact passes every unsealed gate");
    validate_artifact_semantics_v2(&artifact)
        .expect("the accepted V2 artifact is semantically exact on the sealed path");
    artifact
}

// ----------------------------------------------------------- closed code vocabulary

/// Every variant of the closed V2 vocabulary, restated here so that adding a
/// production variant without updating this list fails the exhaustive match in
/// `accepted_portable_code_v2`.
fn closed_rejection_variants_v2() -> [NativeFullEpisodeTrajectoryErrorV2; 22] {
    use NativeFullEpisodeTrajectoryErrorV2 as Code;
    [
        Code::AuthorityMismatch,
        Code::EpisodeIndexOutsideU63,
        Code::LearnerSeatRuleMismatch,
        Code::InvalidDeckId,
        Code::RuntimeDeckHashMismatch,
        Code::EmptyDecisionStream,
        Code::EpisodeMismatch,
        Code::RowOrdinalMismatch,
        Code::ActorRoleMismatch,
        Code::MalformedPhysicalGroup,
        Code::InvalidLegalActionCount,
        Code::SelectedIndexOutOfRange,
        Code::MalformedCommitment,
        Code::CounterOverflow,
        Code::NonNaturalTerminal,
        Code::TerminalProvenanceMismatch,
        Code::TerminalCountMismatch,
        Code::ScheduleIntegerOutsideU63,
        Code::PairIndexOutsideEpisodeDomain,
        Code::PairEpisodeIndexMismatch,
        Code::PairEnvironmentSeedMismatch,
        Code::PairPhysicalDeckBindingMismatch,
    ]
}

/// B2's own total mapping from the production variant to its portable code.
/// There is no wildcard arm: a new production variant fails to compile here until
/// it is explicitly handled, which is what makes the vocabulary closed rather
/// than merely enumerated.
const fn accepted_portable_code_v2(code: NativeFullEpisodeTrajectoryErrorV2) -> &'static str {
    use NativeFullEpisodeTrajectoryErrorV2 as Code;
    match code {
        Code::AuthorityMismatch => "authority-mismatch",
        Code::EpisodeIndexOutsideU63 => "episode-index-outside-u63",
        Code::LearnerSeatRuleMismatch => "learner-seat-rule-mismatch",
        Code::InvalidDeckId => "invalid-deck-id",
        Code::RuntimeDeckHashMismatch => "runtime-deck-hash-mismatch",
        Code::EmptyDecisionStream => "empty-decision-stream",
        Code::EpisodeMismatch => "episode-mismatch",
        Code::RowOrdinalMismatch => "row-ordinal-mismatch",
        Code::ActorRoleMismatch => "actor-role-mismatch",
        Code::MalformedPhysicalGroup => "malformed-physical-group",
        Code::InvalidLegalActionCount => "invalid-legal-action-count",
        Code::SelectedIndexOutOfRange => "selected-index-out-of-range",
        Code::MalformedCommitment => "malformed-commitment",
        Code::CounterOverflow => "counter-overflow",
        Code::NonNaturalTerminal => "non-natural-terminal",
        Code::TerminalProvenanceMismatch => "terminal-provenance-mismatch",
        Code::TerminalCountMismatch => "terminal-count-mismatch",
        Code::ScheduleIntegerOutsideU63 => "schedule-integer-outside-u63",
        Code::PairIndexOutsideEpisodeDomain => "pair-index-outside-episode-domain",
        Code::PairEpisodeIndexMismatch => "pair-episode-index-mismatch",
        Code::PairEnvironmentSeedMismatch => "pair-environment-seed-mismatch",
        Code::PairPhysicalDeckBindingMismatch => "pair-physical-deck-binding-mismatch",
    }
}

fn closed_rejection_codes_v2() -> [&'static str; 22] {
    let mut codes = [""; 22];
    for (slot, variant) in codes.iter_mut().zip(closed_rejection_variants_v2()) {
        *slot = accepted_portable_code_v2(variant);
    }
    codes
}

// --------------------------------------------------------------- fixture decoding

#[derive(Clone, Debug, PartialEq, Eq)]
enum GoldenRunErrorV2 {
    /// A production-owned V2 rejection.
    Contract(NativeFullEpisodeTrajectoryErrorV2),
    /// The artifact violated its own outer schema.  This is never a contract
    /// outcome and always a test failure.
    InvalidFixture(&'static str),
}

fn portable_run_code_v2(error: &GoldenRunErrorV2) -> &'static str {
    match error {
        GoldenRunErrorV2::Contract(code) => accepted_portable_code_v2(*code),
        GoldenRunErrorV2::InvalidFixture(reason) => {
            panic!("golden artifact violated its outer schema: {reason}")
        }
    }
}

fn player_seat_v2(value: &str) -> Result<PlayerSeatV1, GoldenRunErrorV2> {
    match value {
        "p0" => Ok(PlayerSeatV1::P0),
        "p1" => Ok(PlayerSeatV1::P1),
        _ => Err(GoldenRunErrorV2::InvalidFixture("invalid seat")),
    }
}

fn actor_role_v2(value: &str) -> Result<NativeTrajectoryActorRoleV1, GoldenRunErrorV2> {
    match value {
        "learner" => Ok(NativeTrajectoryActorRoleV1::Learner),
        "opponent" => Ok(NativeTrajectoryActorRoleV1::Opponent),
        _ => Err(GoldenRunErrorV2::InvalidFixture("invalid actor role")),
    }
}

fn terminal_outcome_v2(value: &str) -> Result<TerminalOutcomeV1, GoldenRunErrorV2> {
    match value {
        "p0-win" => Ok(TerminalOutcomeV1::P0Win),
        "p1-win" => Ok(TerminalOutcomeV1::P1Win),
        "draw" => Ok(TerminalOutcomeV1::Draw),
        "truncated" => Ok(TerminalOutcomeV1::Truncated),
        "halted" => Ok(TerminalOutcomeV1::Halted),
        _ => Err(GoldenRunErrorV2::InvalidFixture("invalid terminal outcome")),
    }
}

fn winner_v2(value: &str) -> Result<Option<PlayerSeatV1>, GoldenRunErrorV2> {
    match value {
        "none" => Ok(None),
        "p0" => Ok(Some(PlayerSeatV1::P0)),
        "p1" => Ok(Some(PlayerSeatV1::P1)),
        _ => Err(GoldenRunErrorV2::InvalidFixture("invalid winner")),
    }
}

fn terminal_classification_v2(value: &str) -> Result<TerminalClassificationV1, GoldenRunErrorV2> {
    match value {
        "natural" => Ok(TerminalClassificationV1::Natural),
        "truncated" => Ok(TerminalClassificationV1::Truncated),
        "halted" => Ok(TerminalClassificationV1::Halted),
        _ => Err(GoldenRunErrorV2::InvalidFixture(
            "invalid terminal classification",
        )),
    }
}

fn terminal_safe_code_v2(value: &str) -> Result<TerminalSafeCodeV2, GoldenRunErrorV2> {
    match value {
        "natural-game-over" => Ok(TerminalSafeCodeV2::NaturalGameOver),
        "decision-cap" => Ok(TerminalSafeCodeV2::DecisionCap),
        "fail-closed" => Ok(TerminalSafeCodeV2::FailClosed),
        _ => Err(GoldenRunErrorV2::InvalidFixture("invalid terminal code")),
    }
}

fn terminal_reward_v2(outcome: TerminalOutcomeV1) -> [i32; 2] {
    match outcome {
        TerminalOutcomeV1::P0Win => [1, -1],
        TerminalOutcomeV1::P1Win => [-1, 1],
        TerminalOutcomeV1::Draw | TerminalOutcomeV1::Truncated | TerminalOutcomeV1::Halted => {
            [0, 0]
        }
    }
}

fn accepted_seat_code_v2(seat: &str) -> Result<u8, GoldenRunErrorV2> {
    match seat {
        "p0" => Ok(0),
        "p1" => Ok(1),
        _ => Err(GoldenRunErrorV2::InvalidFixture("invalid seat")),
    }
}

/// The V2 start, built from exactly the artifact's start fields.  There is no
/// inner digest, inner root, inner seat, or inner deck override to carry.
fn start_from_input_v2(
    input: &TrajectoryInputV2,
) -> Result<NativeFullEpisodeTrajectoryStartV2, GoldenRunErrorV2> {
    Ok(NativeFullEpisodeTrajectoryStartV2 {
        episode_index: parse_u64_hex_v2(&input.episode_index_u64_hex)?,
        pair_environment_seed: parse_u64_hex_v2(&input.pair_environment_seed_u64_hex)?,
        deck_ids: [input.deck_p0_id.clone(), input.deck_p1_id.clone()],
        deck_hashes: [
            parse_u64_hex_v2(&input.deck_p0_hash_u64_hex)?,
            parse_u64_hex_v2(&input.deck_p1_hash_u64_hex)?,
        ],
        learner_seat: player_seat_v2(&input.learner_seat)?,
    })
}

/// A decision row with a caller-chosen commitment.  Splitting the commitment out
/// is what lets the runner preflight a row's ordering, role, width, and group
/// rules *before* it crosses the checked commitment boundary, which is the
/// generator's own order.
fn decision_row_v2(
    row: &DecisionRowV2,
    commitment: [u8; 16],
) -> Result<NativeFullEpisodeTrajectoryDecisionRowV1, GoldenRunErrorV2> {
    Ok(NativeFullEpisodeTrajectoryDecisionRowV1 {
        row_ordinal: parse_u64_hex_v2(&row.row_ordinal_u64_hex)?,
        actor_seat: player_seat_v2(&row.actor_seat)?,
        actor_role: actor_role_v2(&row.actor_role)?,
        physical_decision_ordinal: parse_u64_hex_v2(&row.physical_decision_ordinal_u64_hex)?,
        actor_physical_decision_ordinal: parse_u64_hex_v2(
            &row.actor_physical_decision_ordinal_u64_hex,
        )?,
        substep_index: row.substep_index_u32,
        substep_count: row.substep_count_u32,
        action_seed: parse_u64_hex_v2(&row.action_seed_u64_hex)?,
        legal_action_count: row.legal_action_count_u32,
        selected_index: row.selected_index_u32,
        flat_action_v2_commitment: commitment,
    })
}

fn terminal_from_input_v2(
    terminal: &TerminalRowV2,
) -> Result<(AsyncRolloutTerminalV1, SessionDeckHashesV1), GoldenRunErrorV2> {
    let outcome = terminal_outcome_v2(&terminal.outcome)?;
    let rollout_terminal = AsyncRolloutTerminalV1 {
        episode_id: parse_u64_hex_v2(&terminal.episode_index_u64_hex)?,
        terminal_outcome: outcome,
        terminal_classification: terminal_classification_v2(&terminal.classification)?,
        terminal_code: terminal_safe_code_v2(&terminal.terminal_code)?,
        winner: winner_v2(&terminal.winner)?,
        terminal_reward: terminal_reward_v2(outcome),
        policy_step_count: parse_u64_hex_v2(&terminal.policy_step_count_u64_hex)?,
        physical_decision_count: parse_u64_hex_v2(&terminal.physical_decision_count_u64_hex)?,
    };
    let deck_hashes = [
        parse_u64_hex_v2(&terminal.deck_p0_hash_u64_hex)?,
        parse_u64_hex_v2(&terminal.deck_p1_hash_u64_hex)?,
    ];
    Ok((rollout_terminal, deck_hashes))
}

// ------------------------------------------------------------ the trajectory runner

/// The one trajectory runner, entirely through the production V2 API.
///
/// Order, matching the accepted contract exactly:
///
/// 1. the declared authority block, which yields the production-owned
///    `AuthorityMismatch` before any start value is even parsed;
/// 2. the start values;
/// 3. the owned V2 accumulator, whose constructor runs production's own private
///    live guard and then production's start precedence;
/// 4. per row: preflight the row's contract rules with a placeholder commitment,
///    then cross the checked commitment boundary, then record.  The commitment
///    never participates in a transition decision, so preflighting first
///    reproduces the generator's order without changing any outcome;
/// 5. the natural finish.
fn run_trajectory_case_v2(
    input: &TrajectoryInputV2,
) -> Result<NativeFullEpisodeTrajectoryReceiptV2, GoldenRunErrorV2> {
    validate_declared_authorities_v2(&input.source_authorities)
        .map_err(GoldenRunErrorV2::Contract)?;
    let start = start_from_input_v2(input)?;
    let mut accumulator = NativeFullEpisodeTrajectoryAccumulatorV2::new_v2(&start)
        .map_err(GoldenRunErrorV2::Contract)?;

    for row in &input.decisions {
        accumulator
            .preflight_candidate_v2(decision_row_v2(row, [0_u8; 16])?)
            .map_err(GoldenRunErrorV2::Contract)?;
        let commitment = checked_flat_action_v2_commitment_v2(&row.flat_action_v2_commitment_hex)
            .map_err(GoldenRunErrorV2::Contract)?;
        accumulator
            .record_accepted_v2(decision_row_v2(row, commitment)?)
            .map_err(GoldenRunErrorV2::Contract)?;
    }

    let (terminal, terminal_deck_hashes) = terminal_from_input_v2(&input.terminal)?;
    accumulator
        .finish_natural_v2(terminal, terminal_deck_hashes)
        .map_err(GoldenRunErrorV2::Contract)
}

/// The positive companion oracle: the same start values driven straight through
/// the frozen V1 accumulator.  Direct V1 use is confined to this function, it is
/// never used to satisfy a V2 rejection, and, since the independent envelope
/// builder now takes an inner digest as an argument, it is never the envelope's
/// own source of that digest either.
fn run_direct_inner_v1_oracle(input: &TrajectoryInputV2) -> [u8; 32] {
    let episode_index = parse_u64_hex_v2(&input.episode_index_u64_hex).expect("positive episode");
    let environment_seed =
        parse_u64_hex_v2(&input.pair_environment_seed_u64_hex).expect("positive root");
    let deck_ids: SessionDeckIdsV1 = [input.deck_p0_id.clone(), input.deck_p1_id.clone()];
    let deck_hashes: SessionDeckHashesV1 = [
        parse_u64_hex_v2(&input.deck_p0_hash_u64_hex).expect("positive P0 hash"),
        parse_u64_hex_v2(&input.deck_p1_hash_u64_hex).expect("positive P1 hash"),
    ];
    let learner_seat = player_seat_v2(&input.learner_seat).expect("positive seat");
    let mut accumulator = NativeFullEpisodeTrajectoryAccumulatorV1::new_v1(
        episode_index,
        environment_seed,
        &deck_ids,
        deck_hashes,
        learner_seat,
    )
    .expect("a positive start builds a V1 accumulator");
    for row in &input.decisions {
        let commitment = parse_fixed_hex_v2::<16>(&row.flat_action_v2_commitment_hex)
            .expect("a positive commitment is raw16 lowercase hex");
        accumulator
            .record_accepted_v1(decision_row_v2(row, commitment).expect("positive row"))
            .expect("a positive row is accepted by V1");
    }
    let (terminal, terminal_deck_hashes) =
        terminal_from_input_v2(&input.terminal).expect("positive terminal");
    accumulator
        .finish_natural_v1(terminal, terminal_deck_hashes)
        .expect("a positive episode finishes naturally under V1")
        .trajectory_sha256
}

// ------------------------------------------------- independent 34-atom V2 envelope

/// The test-side V2 envelope, built from this module's own accepted literals, the
/// artifact's start values, and a caller-supplied inner digest.
///
/// The inner digest is a parameter on purpose.  If this builder computed it by
/// calling the direct V1 oracle, the "independent" envelope would share that
/// oracle with the very receipt it is meant to check.  The caller computes the
/// digest locally from the stored inner preimage, proves it against both the
/// stored digest and the two production receipts, and only then hands it in.
/// Production's private `envelope_sha256_v2` is never called and production's V2
/// self-seals never appear: either would make the oracle a restatement of the
/// thing under test.
fn independent_v2_envelope_v2(input: &TrajectoryInputV2, inner_sha256: [u8; 32]) -> Vec<u8> {
    let episode_index = parse_u64_hex_v2(&input.episode_index_u64_hex).expect("envelope episode");
    let pair_environment_seed =
        parse_u64_hex_v2(&input.pair_environment_seed_u64_hex).expect("envelope root");
    let deck_p0_hash = parse_u64_hex_v2(&input.deck_p0_hash_u64_hex).expect("envelope P0 hash");
    let deck_p1_hash = parse_u64_hex_v2(&input.deck_p1_hash_u64_hex).expect("envelope P1 hash");
    let learner_seat = accepted_seat_code_v2(&input.learner_seat).expect("envelope seat");

    let mut envelope = AtomStreamV2::new_v2();
    envelope.push_v2("domain", ACCEPTED_TRAJECTORY_IDENTITY_V2.as_bytes());
    envelope.push_v2(
        "inner_trajectory_identity_utf8",
        ACCEPTED_INNER_IDENTITY_V2.as_bytes(),
    );
    envelope.push_v2(
        "inner_trajectory_goldens_schema_utf8",
        ACCEPTED_INNER_GOLDENS_SCHEMA_V2.as_bytes(),
    );
    envelope.push_v2(
        "inner_trajectory_goldens_generator_identity_utf8",
        ACCEPTED_INNER_GOLDENS_GENERATOR_IDENTITY_V2.as_bytes(),
    );
    envelope.push_v2(
        "inner_trajectory_golden_stream_identity_utf8",
        ACCEPTED_INNER_GOLDEN_STREAM_IDENTITY_V2.as_bytes(),
    );
    envelope.push_v2(
        "inner_trajectory_goldens_file_sha256_raw32",
        &raw32_v2(ACCEPTED_INNER_GOLDENS_FILE_SHA256_V2),
    );
    envelope.push_v2(
        "inner_trajectory_golden_stream_sha256_raw32",
        &raw32_v2(ACCEPTED_INNER_GOLDEN_STREAM_SHA256_V2),
    );
    envelope.push_v2(
        "environment_randomization_identity_utf8",
        ACCEPTED_ENVIRONMENT_IDENTITY_V2.as_bytes(),
    );
    envelope.push_v2(
        "environment_randomization_namespace_utf8",
        ACCEPTED_ENVIRONMENT_NAMESPACE_V2.as_bytes(),
    );
    envelope.push_v2(
        "environment_randomization_kdf_goldens_schema_utf8",
        ACCEPTED_ENVIRONMENT_KDF_GOLDENS_SCHEMA_V2.as_bytes(),
    );
    envelope.push_v2(
        "environment_randomization_kdf_goldens_file_sha256_raw32",
        &raw32_v2(ACCEPTED_ENVIRONMENT_KDF_GOLDENS_FILE_SHA256_V2),
    );
    envelope.push_v2(
        "reset_trajectory_goldens_schema_utf8",
        ACCEPTED_RESET_GOLDENS_SCHEMA_V2.as_bytes(),
    );
    envelope.push_v2(
        "reset_trajectory_generator_identity_utf8",
        ACCEPTED_RESET_GENERATOR_IDENTITY_V2.as_bytes(),
    );
    envelope.push_v2(
        "reset_trajectory_physical_projection_identity_utf8",
        ACCEPTED_RESET_PHYSICAL_PROJECTION_IDENTITY_V2.as_bytes(),
    );
    envelope.push_v2(
        "reset_trajectory_vector_stream_identity_utf8",
        ACCEPTED_RESET_PORTABLE_STREAM_IDENTITY_V2.as_bytes(),
    );
    envelope.push_v2(
        "reset_trajectory_goldens_file_sha256_raw32",
        &raw32_v2(ACCEPTED_RESET_GOLDENS_FILE_SHA256_V2),
    );
    envelope.push_v2(
        "reset_trajectory_vector_stream_sha256_raw32",
        &raw32_v2(ACCEPTED_RESET_PORTABLE_STREAM_SHA256_V2),
    );
    envelope.push_v2(
        "trainer_schedule_identity_utf8",
        ACCEPTED_TRAINER_SCHEDULE_IDENTITY_V2.as_bytes(),
    );
    envelope.push_v2(
        "trainer_seed_version_utf8",
        ACCEPTED_TRAINER_SEED_VERSION_V2.as_bytes(),
    );
    envelope.push_v2(
        "trainer_schedule_goldens_file_sha256_raw32",
        &raw32_v2(ACCEPTED_TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_V2),
    );
    envelope.push_v2(
        "runtime_deck_catalog_schema_utf8",
        ACCEPTED_RUNTIME_DECK_CATALOG_SCHEMA_V2.as_bytes(),
    );
    envelope.push_v2(
        "runtime_deck_protocol_utf8",
        ACCEPTED_RUNTIME_DECK_PROTOCOL_V2.as_bytes(),
    );
    envelope.push_v2(
        "runtime_deck_materialization_protocol_utf8",
        ACCEPTED_RUNTIME_DECK_MATERIALIZATION_PROTOCOL_V2.as_bytes(),
    );
    envelope.push_v2(
        "runtime_deck_hash_algorithm_utf8",
        ACCEPTED_RUNTIME_DECK_HASH_ALGORITHM_V2.as_bytes(),
    );
    envelope.push_v2(
        "runtime_deck_catalog_file_sha256_raw32",
        &raw32_v2(ACCEPTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2),
    );
    envelope.push_v2("episode_index_u64be", &episode_index.to_be_bytes());
    envelope.push_v2("pair_index_u64be", &(episode_index / 2).to_be_bytes());
    envelope.push_v2(
        "pair_environment_seed_u64be",
        &pair_environment_seed.to_be_bytes(),
    );
    envelope.push_v2("deck_p0_id_utf8", input.deck_p0_id.as_bytes());
    envelope.push_v2("deck_p0_hash_u64be", &deck_p0_hash.to_be_bytes());
    envelope.push_v2("deck_p1_id_utf8", input.deck_p1_id.as_bytes());
    envelope.push_v2("deck_p1_hash_u64be", &deck_p1_hash.to_be_bytes());
    envelope.push_v2("learner_seat_u8", &[learner_seat]);
    envelope.push_v2("inner_trajectory_sha256_raw32", &inner_sha256);

    assert_eq!(
        envelope.atom_count, ACCEPTED_ENVELOPE_ATOM_COUNT_V2,
        "the independent V2 envelope is frozen at exactly thirty-four atoms"
    );
    envelope.bytes
}

// ------------------------------------------------------- portable semantic stream

/// Rebuilds the portable semantic stream in the generator's exact order from
/// typed values.  The generator is never invoked, and the artifact's own stream
/// hex appears only where the frozen stream definition requires those bytes as an
/// atom payload; the oracle is this order plus the length, atom-count, and digest
/// pins.
fn portable_semantic_stream_v2(artifact: &GoldenArtifactV2) -> (Vec<u8>, usize) {
    let mut stream = AtomStreamV2::new_v2();
    stream.push_v2("domain", ACCEPTED_GOLDEN_STREAM_IDENTITY_V2.as_bytes());
    stream.push_v2("schema_utf8", artifact.schema.as_bytes());
    stream.push_v2(
        "generator_identity_utf8",
        artifact.generator_identity.as_bytes(),
    );
    stream.push_v2(
        "trajectory_identity_utf8",
        artifact.trajectory_identity.as_bytes(),
    );
    stream.push_v2(
        "source_authorities_canonical_json_utf8",
        &canonical_json_no_lf_v2(&artifact.source_authorities),
    );
    stream.push_v2(
        "positive_case_count_u32be",
        &u32be_v2(artifact.positive_cases.len()),
    );
    for case in &artifact.positive_cases {
        stream.push_v2("positive_case_name_ascii", case.name.as_bytes());
        stream.push_v2(
            "positive_case_input_canonical_json_utf8",
            &canonical_json_no_lf_v2(&case.input),
        );
        stream.push_v2(
            "positive_case_inner_stream_raw",
            &parse_hex_vec_v2(&case.inner_stream_hex).expect("positive inner stream is even hex"),
        );
        stream.push_v2(
            "positive_case_inner_sha256_raw32",
            &raw32_v2(&case.inner_sha256),
        );
        stream.push_v2(
            "positive_case_v2_stream_raw",
            &parse_hex_vec_v2(&case.v2_stream_hex).expect("positive V2 stream is even hex"),
        );
        stream.push_v2("positive_case_v2_sha256_raw32", &raw32_v2(&case.v2_sha256));
    }
    stream.push_v2(
        "pair_positive_case_count_u32be",
        &u32be_v2(artifact.pair_positive_cases.len()),
    );
    for case in &artifact.pair_positive_cases {
        stream.push_v2("pair_positive_case_name_ascii", case.name.as_bytes());
        stream.push_v2(
            "pair_positive_case_input_canonical_json_utf8",
            &canonical_json_no_lf_v2(&case.input),
        );
        stream.push_v2(
            "pair_positive_even_trajectory_sha256_raw32",
            &raw32_v2(&case.even_trajectory_sha256),
        );
        stream.push_v2(
            "pair_positive_odd_trajectory_sha256_raw32",
            &raw32_v2(&case.odd_trajectory_sha256),
        );
    }
    stream.push_v2(
        "trajectory_reject_case_count_u32be",
        &u32be_v2(artifact.trajectory_reject_cases.len()),
    );
    for case in &artifact.trajectory_reject_cases {
        stream.push_v2("trajectory_reject_case_name_ascii", case.name.as_bytes());
        stream.push_v2(
            "trajectory_reject_case_input_canonical_json_utf8",
            &canonical_json_no_lf_v2(&case.input),
        );
        stream.push_v2(
            "trajectory_reject_expected_code_ascii",
            case.expected_code.as_bytes(),
        );
    }
    stream.push_v2(
        "pair_reject_case_count_u32be",
        &u32be_v2(artifact.pair_reject_cases.len()),
    );
    for case in &artifact.pair_reject_cases {
        stream.push_v2("pair_reject_case_name_ascii", case.name.as_bytes());
        stream.push_v2(
            "pair_reject_case_input_canonical_json_utf8",
            &canonical_json_no_lf_v2(&case.input),
        );
        stream.push_v2(
            "pair_reject_expected_code_ascii",
            case.expected_code.as_bytes(),
        );
    }
    (stream.bytes, stream.atom_count)
}

// ------------------------------------------------------------------ pair runner

/// The pair runner.
///
/// The two schedule-integer bounds are decided by the production pair validator
/// itself and precede every trajectory, so an out-of-domain base seed or pair
/// index goes straight through the validator.  Every other case runs the full
/// even trajectory and then the full odd trajectory before pair validation, which
/// is the accepted composition: a pair whose component episode is itself invalid
/// reports that component's code, from the even side first.
fn run_pair_case_v2(
    input: &PairInputV2,
) -> Result<NativeFullEpisodeTrajectoryPairBindingV2, GoldenRunErrorV2> {
    let base_seed = parse_u64_hex_v2(&input.base_seed_u64_hex)?;
    let pair_index = parse_u64_hex_v2(&input.pair_index_u64_hex)?;
    let even_start = start_from_input_v2(&input.even_start)?;
    let odd_start = start_from_input_v2(&input.odd_start)?;

    if base_seed > ACCEPTED_U63_MAX_V2 || pair_index > ACCEPTED_U62_MAX_V2 {
        return validate_native_full_episode_trajectory_pair_v2(
            base_seed,
            pair_index,
            &even_start,
            &odd_start,
        )
        .map_err(GoldenRunErrorV2::Contract);
    }

    run_trajectory_case_v2(&input.even_start)?;
    run_trajectory_case_v2(&input.odd_start)?;
    validate_native_full_episode_trajectory_pair_v2(base_seed, pair_index, &even_start, &odd_start)
        .map_err(GoldenRunErrorV2::Contract)
}

// ================================ independent baselines and rejection reconstruction
//
// The accepted generator builds every rejection fixture by deep-copying one of
// two positive baselines and applying one named mutation, or one named set of
// coordinated mutations.  This section reproduces that exactly:
//
// * `rebuilt_trajectory_input_v2` is an independent transcription of the
//   generator's `build_start`, `build_decisions`, and `build_terminal`, driven
//   only by B2's own fixture literals and B2's own authority literals;
// * `reconstruct_trajectory_reject_v2` and `reconstruct_pair_reject_v2` are keyed
//   by literal case name, clone a baseline, apply that case's exact mutation, and
//   return a literal expected code.
//
// Nothing in this section reads a stored reject fixture.  The stored fixture is
// only ever a comparison target.

fn accepted_learner_seat_for_episode_v2(episode_index: u64) -> &'static str {
    if episode_index.is_multiple_of(2) {
        "p0"
    } else {
        "p1"
    }
}

/// The generator's `build_decisions`: one row per substep of each physical group,
/// with the row ordinal, the physical ordinal, the per-actor physical ordinal, the
/// action seed, the selected index, and the commitment all derived positionally.
fn rebuilt_decision_rows_v2(learner_seat: &str, groups: &[(&str, u32)]) -> Vec<DecisionRowV2> {
    let mut rows: Vec<DecisionRowV2> = Vec::new();
    let mut row_ordinal: u64 = 0;
    let mut learner_physical: u64 = 0;
    let mut opponent_physical: u64 = 0;
    for (physical_ordinal, (seat, substep_count)) in (0_u64..).zip(groups.iter()) {
        let is_learner = *seat == learner_seat;
        let role = if is_learner { "learner" } else { "opponent" };
        let actor_ordinal = if is_learner {
            learner_physical
        } else {
            opponent_physical
        };
        for substep_index in 0..*substep_count {
            rows.push(DecisionRowV2 {
                action_seed_u64_hex: u64_hex_v2(ACCEPTED_ACTION_SEED_BASE_V2 + row_ordinal),
                actor_physical_decision_ordinal_u64_hex: u64_hex_v2(actor_ordinal),
                actor_role: role.to_string(),
                actor_seat: (*seat).to_string(),
                flat_action_v2_commitment_hex: format!(
                    "{:032x}",
                    ACCEPTED_COMMITMENT_BASE_V2 + row_ordinal
                ),
                legal_action_count_u32: ACCEPTED_LEGAL_ACTION_COUNT_V2,
                physical_decision_ordinal_u64_hex: u64_hex_v2(physical_ordinal),
                row_ordinal_u64_hex: u64_hex_v2(row_ordinal),
                selected_index_u32: (row_ordinal % 4) as u32,
                substep_count_u32: *substep_count,
                substep_index_u32: substep_index,
            });
            row_ordinal += 1;
        }
        if is_learner {
            learner_physical += 1;
        } else {
            opponent_physical += 1;
        }
    }
    rows
}

/// The generator's `build_terminal`.
fn rebuilt_terminal_row_v2(
    episode_index: u64,
    outcome: &str,
    policy_step_count: u64,
    physical_decision_count: u64,
) -> TerminalRowV2 {
    let winner = match outcome {
        "p0-win" => "p0",
        "p1-win" => "p1",
        "draw" => "none",
        other => panic!("the accepted baselines carry no outcome {other:?}"),
    };
    TerminalRowV2 {
        classification: "natural".to_string(),
        deck_p0_hash_u64_hex: u64_hex_v2(ACCEPTED_BURN_DECK_HASH_V2),
        deck_p1_hash_u64_hex: u64_hex_v2(ACCEPTED_RALLY_DECK_HASH_V2),
        episode_index_u64_hex: u64_hex_v2(episode_index),
        outcome: outcome.to_string(),
        physical_decision_count_u64_hex: u64_hex_v2(physical_decision_count),
        policy_step_count_u64_hex: u64_hex_v2(policy_step_count),
        terminal_code: "natural-game-over".to_string(),
        winner: winner.to_string(),
    }
}

/// The generator's `build_start`, from B2's literals only.
fn rebuilt_trajectory_input_v2(
    episode_index: u64,
    pair_environment_seed: u64,
    outcome: &str,
    groups: &[(&str, u32)],
) -> TrajectoryInputV2 {
    let learner_seat = accepted_learner_seat_for_episode_v2(episode_index);
    let decisions = rebuilt_decision_rows_v2(learner_seat, groups);
    let policy_step_count = decisions.len() as u64;
    TrajectoryInputV2 {
        decisions,
        deck_p0_hash_u64_hex: u64_hex_v2(ACCEPTED_BURN_DECK_HASH_V2),
        deck_p0_id: ACCEPTED_BURN_DECK_ID_V2.to_string(),
        deck_p1_hash_u64_hex: u64_hex_v2(ACCEPTED_RALLY_DECK_HASH_V2),
        deck_p1_id: ACCEPTED_RALLY_DECK_ID_V2.to_string(),
        episode_index_u64_hex: u64_hex_v2(episode_index),
        learner_seat: learner_seat.to_string(),
        pair_environment_seed_u64_hex: u64_hex_v2(pair_environment_seed),
        source_authorities: accepted_source_authorities_v2(),
        terminal: rebuilt_terminal_row_v2(
            episode_index,
            outcome,
            policy_step_count,
            groups.len() as u64,
        ),
    }
}

/// T0: the trajectory reject baseline, which is also the positive case named
/// `episode-0-native-root-learner-p0-p0-win`.
fn rebuilt_t0_input_v2() -> TrajectoryInputV2 {
    rebuilt_trajectory_input_v2(
        0,
        ACCEPTED_NATIVE_ROOT_V2,
        "p0-win",
        &ACCEPTED_EVEN_GROUPS_V2,
    )
}

/// P0: the pair reject baseline, which is also the pair positive case named
/// `pair-native-base-71501-index-0`.
fn rebuilt_p0_pair_input_v2() -> PairInputV2 {
    PairInputV2 {
        base_seed_u64_hex: u64_hex_v2(ACCEPTED_NATIVE_BASE_SEED_V2),
        even_start: rebuilt_trajectory_input_v2(
            0,
            ACCEPTED_NATIVE_ROOT_V2,
            "p0-win",
            &ACCEPTED_EVEN_GROUPS_V2,
        ),
        odd_start: rebuilt_trajectory_input_v2(
            1,
            ACCEPTED_NATIVE_ROOT_V2,
            "p1-win",
            &ACCEPTED_ODD_GROUPS_V2,
        ),
        pair_index_u64_hex: u64_hex_v2(ACCEPTED_NATIVE_PAIR_INDEX_V2),
    }
}

fn positive_case_by_name_v2<'a>(artifact: &'a GoldenArtifactV2, name: &str) -> &'a PositiveCaseV2 {
    artifact
        .positive_cases
        .iter()
        .find(|case| case.name == name)
        .unwrap_or_else(|| panic!("the artifact declares no positive case named {name:?}"))
}

fn pair_positive_case_by_name_v2<'a>(
    artifact: &'a GoldenArtifactV2,
    name: &str,
) -> &'a PairPositiveCaseV2 {
    artifact
        .pair_positive_cases
        .iter()
        .find(|case| case.name == name)
        .unwrap_or_else(|| panic!("the artifact declares no pair positive case named {name:?}"))
}

/// Every trajectory rejection, keyed by its literal name.  Each arm clones T0,
/// applies the accepted generator's exact mutation for that name, and returns a
/// literal expected code.  Neither the mutation nor the code is read from the
/// stored fixture.
fn reconstruct_trajectory_reject_v2(
    name: &str,
    baseline: &TrajectoryInputV2,
) -> (TrajectoryInputV2, &'static str) {
    let mut input = baseline.clone();
    match name {
        "actor-role-mismatch" => {
            input.decisions[0].actor_role = "opponent".to_string();
            (input, "actor-role-mismatch")
        }
        "authority-environment-namespace-drift" => {
            input.source_authorities.environment_randomization.namespace = "drifted".to_string();
            (input, "authority-mismatch")
        }
        // Coordinated: the stream is emptied, both terminal counts are zeroed to
        // match it, and the terminal episode is drifted.  The episode mismatch is
        // decided before the stream is examined, so the compound case reports the
        // episode code and not the empty-stream code.
        "compound-empty-stream-and-terminal-episode-mismatch" => {
            input.decisions.clear();
            input.terminal.policy_step_count_u64_hex = u64_hex_v2(0);
            input.terminal.physical_decision_count_u64_hex = u64_hex_v2(0);
            input.terminal.episode_index_u64_hex = u64_hex_v2(9);
            (input, "episode-mismatch")
        }
        // Coordinated: the final group's only row is widened so the group is left
        // open at stream end without tripping the in-loop continuation check
        // first, and the terminal episode is drifted.
        "compound-open-group-and-terminal-episode-mismatch" => {
            input
                .decisions
                .last_mut()
                .expect("the baseline declares a nonempty decision stream")
                .substep_count_u32 = 9;
            input.terminal.episode_index_u64_hex = u64_hex_v2(9);
            (input, "episode-mismatch")
        }
        "deck-id-case-drift-burn" => {
            input.deck_p0_id = "burn".to_string();
            (input, "invalid-deck-id")
        }
        "deck-id-unknown-not-in-catalog" => {
            input.deck_p0_id = "Nonexistent".to_string();
            (input, "invalid-deck-id")
        }
        // Coordinated: emptying the stream also zeroes both declared counts, so
        // the empty-stream code is reached rather than a count mismatch.
        "empty-decision-stream" => {
            input.decisions.clear();
            input.terminal.policy_step_count_u64_hex = u64_hex_v2(0);
            input.terminal.physical_decision_count_u64_hex = u64_hex_v2(0);
            (input, "empty-decision-stream")
        }
        "episode-index-two-pow-63" => {
            input.episode_index_u64_hex = u64_hex_v2(1_u64 << 63);
            (input, "episode-index-outside-u63")
        }
        "incomplete-physical-group" => {
            input.decisions[2].substep_count_u32 = 9;
            (input, "malformed-physical-group")
        }
        "learner-seat-parity-mismatch" => {
            input.learner_seat = "p1".to_string();
            (input, "learner-seat-rule-mismatch")
        }
        "legal-action-count-sixty-five" => {
            input.decisions[0].legal_action_count_u32 = 65;
            (input, "invalid-legal-action-count")
        }
        "legal-action-count-zero" => {
            input.decisions[0].legal_action_count_u32 = 0;
            (input, "invalid-legal-action-count")
        }
        "malformed-commitment-short" => {
            input.decisions[0].flat_action_v2_commitment_hex = "ab".repeat(15);
            (input, "malformed-commitment")
        }
        "malformed-group-substep-index-at-count" => {
            input.decisions[0].substep_index_u32 = 3;
            (input, "malformed-physical-group")
        }
        // Coordinated: a non-natural terminal is only representable if the
        // outcome, the winner, the classification, and the terminal code all move
        // together.
        "non-natural-terminal" => {
            input.terminal.outcome = "truncated".to_string();
            input.terminal.winner = "none".to_string();
            input.terminal.classification = "truncated".to_string();
            input.terminal.terminal_code = "decision-cap".to_string();
            (input, "non-natural-terminal")
        }
        "row-ordinal-mismatch" => {
            input.decisions[1].row_ordinal_u64_hex = u64_hex_v2(7);
            (input, "row-ordinal-mismatch")
        }
        "runtime-deck-hash-mismatch-p1" => {
            input.deck_p1_hash_u64_hex = u64_hex_v2(ACCEPTED_RALLY_DECK_HASH_V2 ^ 1);
            (input, "runtime-deck-hash-mismatch")
        }
        "selected-index-equal-width" => {
            input.decisions[0].selected_index_u32 = 4;
            (input, "selected-index-out-of-range")
        }
        "terminal-count-mismatch" => {
            input.terminal.policy_step_count_u64_hex = u64_hex_v2(99);
            (input, "terminal-count-mismatch")
        }
        "terminal-deck-provenance-mismatch" => {
            input.terminal.deck_p0_hash_u64_hex = u64_hex_v2(ACCEPTED_RALLY_DECK_HASH_V2);
            (input, "terminal-provenance-mismatch")
        }
        "terminal-episode-mismatch" => {
            input.terminal.episode_index_u64_hex = u64_hex_v2(9);
            (input, "episode-mismatch")
        }
        other => panic!("no trajectory rejection is reconstructed for the name {other:?}"),
    }
}

/// Every pair rejection, keyed by its literal name.  Each arm clones P0.
fn reconstruct_pair_reject_v2(name: &str, baseline: &PairInputV2) -> (PairInputV2, &'static str) {
    let mut input = baseline.clone();
    match name {
        "pair-base-seed-two-pow-63" => {
            input.base_seed_u64_hex = u64_hex_v2(1_u64 << 63);
            (input, "schedule-integer-outside-u63")
        }
        "pair-index-two-pow-62" => {
            input.pair_index_u64_hex = u64_hex_v2(1_u64 << 62);
            (input, "pair-index-outside-episode-domain")
        }
        "pair-learner-seat-not-swapped" => {
            input.odd_start.learner_seat = "p0".to_string();
            (input, "learner-seat-rule-mismatch")
        }
        // Coordinated: the odd side is rebuilt as episode three.  Episode three
        // keeps the odd parity, so the seat rule still holds and the odd start is
        // internally valid; its terminal episode moves with it, and its outcome
        // and winner move to the p0-win pair the generator declares.  Only the
        // pair-stage episode binding is left to fail.
        "pair-odd-episode-index-drift" => {
            input.odd_start.episode_index_u64_hex = u64_hex_v2(3);
            input.odd_start.terminal.episode_index_u64_hex = u64_hex_v2(3);
            input.odd_start.terminal.outcome = "p0-win".to_string();
            input.odd_start.terminal.winner = "p0".to_string();
            (input, "pair-episode-index-mismatch")
        }
        // Coordinated: both deck IDs, both start hashes, and both terminal
        // provenance hashes swap together, so the odd side stays internally
        // consistent and only the pair's physical deck binding fails.
        "pair-odd-physical-deck-swap" => {
            input.odd_start.deck_p0_id = ACCEPTED_RALLY_DECK_ID_V2.to_string();
            input.odd_start.deck_p0_hash_u64_hex = u64_hex_v2(ACCEPTED_RALLY_DECK_HASH_V2);
            input.odd_start.deck_p1_id = ACCEPTED_BURN_DECK_ID_V2.to_string();
            input.odd_start.deck_p1_hash_u64_hex = u64_hex_v2(ACCEPTED_BURN_DECK_HASH_V2);
            input.odd_start.terminal.deck_p0_hash_u64_hex = u64_hex_v2(ACCEPTED_RALLY_DECK_HASH_V2);
            input.odd_start.terminal.deck_p1_hash_u64_hex = u64_hex_v2(ACCEPTED_BURN_DECK_HASH_V2);
            (input, "pair-physical-deck-binding-mismatch")
        }
        "pair-odd-root-drift" => {
            input.odd_start.pair_environment_seed_u64_hex = u64_hex_v2(ACCEPTED_NATIVE_ROOT_V2 - 1);
            (input, "pair-environment-seed-mismatch")
        }
        other => panic!("no pair rejection is reconstructed for the name {other:?}"),
    }
}

/// One trajectory rejection, proven four ways: the stored name is the expected
/// name, the reconstructed input is the stored input, the literal code is the
/// stored code, and production, run on the *reconstructed* input, returns that
/// literal code.
fn verify_trajectory_reject_case_v2(
    expected_name: &str,
    case: &TrajectoryRejectCaseV2,
    baseline: &TrajectoryInputV2,
) -> Result<(), String> {
    if case.name != expected_name {
        return Err(format!(
            "case name drift: expected {expected_name:?}, stored {:?}",
            case.name
        ));
    }
    let (input, code) = reconstruct_trajectory_reject_v2(expected_name, baseline);
    if input != case.input {
        return Err(format!(
            "{expected_name}: reconstructed input is not the stored input"
        ));
    }
    if code != case.expected_code {
        return Err(format!(
            "{expected_name}: literal code {code:?} is not the stored code {:?}",
            case.expected_code
        ));
    }
    match run_trajectory_case_v2(&input) {
        Ok(_) => Err(format!(
            "{expected_name}: the reconstructed input was admitted"
        )),
        Err(error) => {
            let observed = portable_run_code_v2(&error);
            if observed == code {
                Ok(())
            } else {
                Err(format!(
                    "{expected_name}: production returned {observed:?}, not the literal {code:?}"
                ))
            }
        }
    }
}

/// One pair rejection, proven the same four ways.
fn verify_pair_reject_case_v2(
    expected_name: &str,
    case: &PairRejectCaseV2,
    baseline: &PairInputV2,
) -> Result<(), String> {
    if case.name != expected_name {
        return Err(format!(
            "case name drift: expected {expected_name:?}, stored {:?}",
            case.name
        ));
    }
    let (input, code) = reconstruct_pair_reject_v2(expected_name, baseline);
    if input != case.input {
        return Err(format!(
            "{expected_name}: reconstructed input is not the stored input"
        ));
    }
    if code != case.expected_code {
        return Err(format!(
            "{expected_name}: literal code {code:?} is not the stored code {:?}",
            case.expected_code
        ));
    }
    match run_pair_case_v2(&input) {
        Ok(_) => Err(format!(
            "{expected_name}: the reconstructed pair was admitted"
        )),
        Err(error) => {
            let observed = portable_run_code_v2(&error);
            if observed == code {
                Ok(())
            } else {
                Err(format!(
                    "{expected_name}: production returned {observed:?}, not the literal {code:?}"
                ))
            }
        }
    }
}

// ------------------------------------------------- positive verification helpers
//
// These return `Result` rather than asserting, so the same code path proves the
// accepted artifact and is reused as the detector in every tamper test below.

/// The positive-case oracle chain, in the order the independence rules require:
///
/// 1. decode the stored inner preimage and hash it here;
/// 2. that local digest must be the stored inner digest;
/// 3. it must also be what the owned V2 run returned;
/// 4. and what the direct V1 accumulator returned, as a separate companion;
/// 5. the local digest, never a production receipt, is what the independent
///    thirty-four-atom envelope is built around;
/// 6. that envelope must be the stored V2 preimage byte for byte, and must hash
///    to the stored V2 digest and to the production receipt alike.
fn verify_positive_case_v2(case: &PositiveCaseV2) -> Result<(), String> {
    let name = case.name.as_str();
    let inner_stream = parse_hex_vec_v2(&case.inner_stream_hex)
        .ok_or_else(|| format!("{name}: inner stream is not even lowercase hex"))?;
    let v2_stream = parse_hex_vec_v2(&case.v2_stream_hex)
        .ok_or_else(|| format!("{name}: V2 stream is not even lowercase hex"))?;

    let local_inner_hex = sha256_hex_v2(&inner_stream);
    if local_inner_hex != case.inner_sha256 {
        return Err(format!(
            "{name}: the locally computed inner digest is not the stored inner digest"
        ));
    }
    let local_inner = raw32_v2(&local_inner_hex);

    let receipt = run_trajectory_case_v2(&case.input)
        .map_err(|error| format!("{name}: the positive case was rejected: {error:?}"))?;
    if receipt.inner_trajectory_sha256 != local_inner {
        return Err(format!(
            "{name}: the owned V2 inner digest is not the locally computed digest"
        ));
    }
    let direct_inner = run_direct_inner_v1_oracle(&case.input);
    if direct_inner != local_inner {
        return Err(format!(
            "{name}: the direct V1 companion digest is not the locally computed digest"
        ));
    }

    let independent = independent_v2_envelope_v2(&case.input, local_inner);
    if independent != v2_stream {
        return Err(format!(
            "{name}: the independent envelope bytes are not the stored V2 stream"
        ));
    }
    if sha256_hex_v2(&independent) != case.v2_sha256 {
        return Err(format!(
            "{name}: the independent envelope digest is not the stored V2 digest"
        ));
    }
    if Sha256::digest(&independent).as_slice() != receipt.trajectory_sha256_v2.as_slice() {
        return Err(format!(
            "{name}: the independent envelope digest is not the production receipt"
        ));
    }
    verify_receipt_matches_input_v2(name, &case.input, &receipt)
}

/// The pair positive oracle chain: both components are finalised through the
/// owned V2 API and matched to their stored digests before any pair claim is
/// made, then the production pair validator's binding is checked field by field.
fn verify_pair_positive_case_v2(case: &PairPositiveCaseV2) -> Result<(), String> {
    let name = case.name.as_str();
    let even = run_trajectory_case_v2(&case.input.even_start)
        .map_err(|error| format!("{name}: the even episode was rejected: {error:?}"))?;
    let odd = run_trajectory_case_v2(&case.input.odd_start)
        .map_err(|error| format!("{name}: the odd episode was rejected: {error:?}"))?;
    if even.trajectory_sha256_v2 != raw32_v2(&case.even_trajectory_sha256) {
        return Err(format!("{name}: the even digest is not the stored digest"));
    }
    if odd.trajectory_sha256_v2 != raw32_v2(&case.odd_trajectory_sha256) {
        return Err(format!("{name}: the odd digest is not the stored digest"));
    }
    verify_receipt_matches_input_v2(name, &case.input.even_start, &even)?;
    verify_receipt_matches_input_v2(name, &case.input.odd_start, &odd)?;
    if even.learner_seat != PlayerSeatV1::P0 || odd.learner_seat != PlayerSeatV1::P1 {
        return Err(format!("{name}: the pair does not swap the learner seat"));
    }

    let binding = run_pair_case_v2(&case.input)
        .map_err(|error| format!("{name}: the pair was rejected: {error:?}"))?;
    let base_seed = parse_u64_hex_v2(&case.input.base_seed_u64_hex)
        .map_err(|_| format!("{name}: base seed is not sixteen lowercase hex"))?;
    let pair_index = parse_u64_hex_v2(&case.input.pair_index_u64_hex)
        .map_err(|_| format!("{name}: pair index is not sixteen lowercase hex"))?;
    if binding.base_seed != base_seed
        || binding.pair_index != pair_index
        || binding.even_episode_index != 2 * pair_index
        || binding.odd_episode_index != 2 * pair_index + 1
        || binding.even_episode_index != even.episode_index
        || binding.odd_episode_index != odd.episode_index
        || binding.pair_environment_seed != even.pair_environment_seed
        || binding.pair_environment_seed != odd.pair_environment_seed
        || binding.deck_ids != even.deck_ids
        || binding.deck_ids != odd.deck_ids
        || binding.deck_hashes != even.deck_hashes
        || binding.deck_hashes != odd.deck_hashes
    {
        return Err(format!(
            "{name}: the pair binding does not match its components"
        ));
    }

    // The schedule-derived root is the frozen trainer schedule's, not a
    // reimplementation and not the artifact's declaration.
    let schedule = native_trainer_episode_schedule_v1(base_seed, binding.even_episode_index)
        .map_err(|_| {
            format!("{name}: the base seed and episode are outside the schedule domain")
        })?;
    if schedule.pair_index != pair_index
        || schedule.learner_seat != PlayerSeatV1::P0
        || schedule.environment_seed != binding.pair_environment_seed
    {
        return Err(format!(
            "{name}: the frozen schedule does not derive this pair"
        ));
    }
    Ok(())
}

/// The receipt's own fields, checked against counts derived from the declared
/// rows rather than copied from the declared terminal.
fn verify_receipt_matches_input_v2(
    name: &str,
    input: &TrajectoryInputV2,
    receipt: &NativeFullEpisodeTrajectoryReceiptV2,
) -> Result<(), String> {
    let episode_index = parse_u64_hex_v2(&input.episode_index_u64_hex)
        .map_err(|_| format!("{name}: episode index is not sixteen lowercase hex"))?;
    let pair_environment_seed = parse_u64_hex_v2(&input.pair_environment_seed_u64_hex)
        .map_err(|_| format!("{name}: root is not sixteen lowercase hex"))?;
    let deck_p0_hash = parse_u64_hex_v2(&input.deck_p0_hash_u64_hex)
        .map_err(|_| format!("{name}: P0 hash is not sixteen lowercase hex"))?;
    let deck_p1_hash = parse_u64_hex_v2(&input.deck_p1_hash_u64_hex)
        .map_err(|_| format!("{name}: P1 hash is not sixteen lowercase hex"))?;
    let learner_seat = player_seat_v2(&input.learner_seat)
        .map_err(|_| format!("{name}: learner seat is outside its domain"))?;
    if receipt.episode_index != episode_index
        || receipt.pair_index != episode_index / 2
        || receipt.pair_environment_seed != pair_environment_seed
        || receipt.deck_ids != [input.deck_p0_id.as_str(), input.deck_p1_id.as_str()]
        || receipt.deck_hashes != [deck_p0_hash, deck_p1_hash]
        || receipt.learner_seat != learner_seat
    {
        return Err(format!(
            "{name}: the receipt does not restate its start values"
        ));
    }

    let learner_policy = input
        .decisions
        .iter()
        .filter(|row| row.actor_role == "learner")
        .count() as u64;
    let opponent_policy = input.decisions.len() as u64 - learner_policy;
    let learner_physical = input
        .decisions
        .iter()
        .filter(|row| {
            row.actor_role == "learner"
                && row.substep_index_u32.checked_add(1) == Some(row.substep_count_u32)
        })
        .count() as u64;
    let opponent_physical = input
        .decisions
        .iter()
        .filter(|row| {
            row.actor_role == "opponent"
                && row.substep_index_u32.checked_add(1) == Some(row.substep_count_u32)
        })
        .count() as u64;
    if receipt.learner_policy_step_count != learner_policy
        || receipt.opponent_policy_step_count != opponent_policy
        || receipt.learner_physical_decision_count != learner_physical
        || receipt.opponent_physical_decision_count != opponent_physical
        || receipt.policy_step_count != learner_policy + opponent_policy
        || receipt.physical_decision_count != learner_physical + opponent_physical
    {
        return Err(format!(
            "{name}: the receipt counters are not the row-derived counts"
        ));
    }

    let declared_policy = parse_u64_hex_v2(&input.terminal.policy_step_count_u64_hex)
        .map_err(|_| format!("{name}: declared policy count is not sixteen lowercase hex"))?;
    let declared_physical = parse_u64_hex_v2(&input.terminal.physical_decision_count_u64_hex)
        .map_err(|_| format!("{name}: declared physical count is not sixteen lowercase hex"))?;
    if receipt.policy_step_count != declared_policy
        || receipt.physical_decision_count != declared_physical
    {
        return Err(format!(
            "{name}: the declared terminal counts are not the derived counts"
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------- mutation helpers
//
// Mutants are built in memory from the accepted artifact, re-serialised
// canonically, and pushed back through layer C.  Nothing here writes a file and
// nothing depends on iteration order.

fn artifact_value_v2() -> serde_json::Value {
    serde_json::from_slice(V2_ARTIFACT_BYTES).expect("the accepted artifact parses as JSON")
}

fn canonical_bytes_of_value_v2(value: &serde_json::Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(value).expect("mutant serialises canonically");
    bytes.push(b'\n');
    bytes
}

/// Walks a `/`-separated path, where a numeric segment indexes an array and the
/// empty path is the document root.
fn value_at_mut_v2<'a>(
    root: &'a mut serde_json::Value,
    path: &str,
) -> Option<&'a mut serde_json::Value> {
    let mut cursor = root;
    if path.is_empty() {
        return Some(cursor);
    }
    for segment in path.split('/') {
        cursor = match cursor {
            serde_json::Value::Object(map) => map.get_mut(segment)?,
            serde_json::Value::Array(items) => items.get_mut(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cursor)
}

fn mutated_artifact_value_v2(path: &str, replacement: serde_json::Value) -> serde_json::Value {
    let mut root = artifact_value_v2();
    *value_at_mut_v2(&mut root, path).expect("mutation path exists in the accepted artifact") =
        replacement;
    root
}

fn removed_key_artifact_value_v2(parent_path: &str, key: &str) -> serde_json::Value {
    let mut root = artifact_value_v2();
    let parent = value_at_mut_v2(&mut root, parent_path).expect("mutation parent path exists");
    parent
        .as_object_mut()
        .expect("mutation parent is an object")
        .remove(key)
        .expect("removed key exists");
    root
}

fn inserted_key_artifact_value_v2(
    parent_path: &str,
    key: &str,
    value: serde_json::Value,
) -> serde_json::Value {
    let mut root = artifact_value_v2();
    let parent = value_at_mut_v2(&mut root, parent_path).expect("mutation parent path exists");
    parent
        .as_object_mut()
        .expect("mutation parent is an object")
        .insert(key.to_string(), value);
    root
}

/// Pushes a mutant through layer C and returns the failure, so the caller can
/// assert on the exact structural, typed, or semantic message.
fn loader_error_of_value_v2(value: &serde_json::Value) -> String {
    load_unsealed_artifact_v2(&canonical_bytes_of_value_v2(value))
        .expect_err("mutant must not pass the unsealed loader")
}

/// Decodes a mutant that is valid through layer D, for contract-level tests.
fn decode_mutant_v2(value: &serde_json::Value) -> GoldenArtifactV2 {
    load_unsealed_artifact_v2(&canonical_bytes_of_value_v2(value))
        .expect("this mutant is valid through layer D by construction")
}

/// A mutant that is valid through layer D but carries a same-shape drift in one
/// stored digest or preimage field.
fn nibble_drifted_artifact_v2(path: &str) -> GoldenArtifactV2 {
    let mut root = artifact_value_v2();
    let slot = value_at_mut_v2(&mut root, path).expect("drift path exists");
    let original = slot.as_str().expect("drift target is a string").to_string();
    let drifted = drift_last_nibble_v2(&original);
    assert_eq!(drifted.len(), original.len());
    assert_ne!(drifted, original);
    *slot = serde_json::Value::String(drifted);
    decode_mutant_v2(&root)
}

// ==========================================================================
// Tests
// ==========================================================================

#[test]
fn v2_artifact_raw_seal_is_exact() {
    let artifact = load_sealed_artifact_v2();
    // The seal already proved length and digest; prove the canonical round trip,
    // the cap, and every byte-level rule explicitly so a future edit cannot
    // quietly drop one.
    assert!(V2_ARTIFACT_BYTES.len() <= MAX_GOLDEN_ARTIFACT_BYTES_V2);
    assert_eq!(canonical_file_bytes_v2(&artifact), V2_ARTIFACT_BYTES);
    assert_eq!(V2_ARTIFACT_BYTES.last(), Some(&b'\n'));
    assert_eq!(
        V2_ARTIFACT_BYTES
            .iter()
            .filter(|byte| **byte == b'\n')
            .count(),
        1
    );
    assert!(V2_ARTIFACT_BYTES.is_ascii());
    assert!(V2_ARTIFACT_BYTES[..V2_ARTIFACT_BYTES.len() - 1]
        .iter()
        .all(|byte| (0x20..=0x7e).contains(byte)));
    assert!(validate_raw_bytes_v2(V2_ARTIFACT_BYTES).is_ok());
}

#[test]
fn v2_artifact_identities_counts_and_names_are_exact() {
    let artifact = load_sealed_artifact_v2();
    validate_artifact_semantics_v2(&artifact).expect("the accepted artifact is semantically exact");

    // The artifact's four identity strings agree with production's own V2
    // identity constants as well as with this module's independent literals.
    assert_eq!(
        artifact.schema,
        NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_SCHEMA_V2
    );
    assert_eq!(
        artifact.generator_identity,
        NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V2
    );
    assert_eq!(
        artifact.trajectory_identity,
        NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V2
    );
    assert_eq!(
        artifact.vector_stream_identity,
        NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V2
    );

    // Production's downstream V2 seals must equal the pins this module proved
    // against the live bytes.  These seals are consumer contracts, never the
    // envelope oracle.
    assert_eq!(
        NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_FILE_SHA256_V2,
        V2_ARTIFACT_RAW_SHA256
    );
    assert_eq!(
        NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_SHA256_V2,
        V2_SEMANTIC_STREAM_SHA256
    );

    assert_eq!(artifact.positive_cases.len(), V2_POSITIVE_CASE_COUNT);
    assert_eq!(
        artifact.pair_positive_cases.len(),
        V2_PAIR_POSITIVE_CASE_COUNT
    );
    assert_eq!(
        artifact.trajectory_reject_cases.len(),
        V2_TRAJECTORY_REJECT_CASE_COUNT
    );
    assert_eq!(artifact.pair_reject_cases.len(), V2_PAIR_REJECT_CASE_COUNT);

    // The two baselines exist under exactly their literal names.
    assert_eq!(
        positive_case_by_name_v2(&artifact, T0_POSITIVE_CASE_NAME_V2).name,
        T0_POSITIVE_CASE_NAME_V2
    );
    assert_eq!(
        pair_positive_case_by_name_v2(&artifact, P0_PAIR_POSITIVE_CASE_NAME_V2).name,
        P0_PAIR_POSITIVE_CASE_NAME_V2
    );
}

#[test]
fn v2_declared_authorities_match_b2_literals_production_literals_and_live_owners() {
    let artifact = load_sealed_artifact_v2();

    // 1. The artifact is validated against B2's own twenty-six literals.
    assert_eq!(
        declared_authority_pairs_v2(&artifact.source_authorities).len(),
        DECLARED_AUTHORITY_FIELD_COUNT_V2
    );
    for (field, declared, accepted) in declared_authority_pairs_v2(&artifact.source_authorities) {
        assert_eq!(declared, accepted, "declared authority drift at {field}");
    }
    assert_eq!(
        validate_declared_authorities_v2(&artifact.source_authorities),
        Ok(())
    );
    // The owned literal object is the artifact's own authority block.
    assert_eq!(
        accepted_source_authorities_v2(),
        artifact.source_authorities
    );

    // Every case input carries the identical authority object; none is exempt.
    for case in &artifact.positive_cases {
        assert_eq!(case.input.source_authorities, artifact.source_authorities);
    }
    for case in &artifact.pair_positive_cases {
        assert_eq!(
            case.input.even_start.source_authorities,
            artifact.source_authorities
        );
        assert_eq!(
            case.input.odd_start.source_authorities,
            artifact.source_authorities
        );
    }

    // 2. B2's twenty-four production-backed literals against production's own
    // private frozen expectations, one for one.
    assert_eq!(
        ACCEPTED_INNER_IDENTITY_V2,
        super::EXPECTED_INNER_TRAJECTORY_IDENTITY_V2
    );
    assert_eq!(
        ACCEPTED_INNER_GOLDENS_SCHEMA_V2,
        super::EXPECTED_INNER_TRAJECTORY_GOLDENS_SCHEMA_V2
    );
    assert_eq!(
        ACCEPTED_INNER_GOLDENS_GENERATOR_IDENTITY_V2,
        super::EXPECTED_INNER_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V2
    );
    assert_eq!(
        ACCEPTED_INNER_GOLDEN_STREAM_IDENTITY_V2,
        super::EXPECTED_INNER_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V2
    );
    assert_eq!(
        ACCEPTED_INNER_GOLDENS_FILE_SHA256_V2,
        super::EXPECTED_INNER_TRAJECTORY_GOLDENS_FILE_SHA256_V2
    );
    assert_eq!(
        ACCEPTED_INNER_GOLDEN_STREAM_SHA256_V2,
        super::EXPECTED_INNER_TRAJECTORY_GOLDEN_STREAM_SHA256_V2
    );
    assert_eq!(
        ACCEPTED_ENVIRONMENT_IDENTITY_V2,
        super::EXPECTED_ENVIRONMENT_RANDOMIZATION_IDENTITY_V2
    );
    assert_eq!(
        ACCEPTED_ENVIRONMENT_NAMESPACE_V2,
        super::EXPECTED_ENVIRONMENT_RANDOMIZATION_NAMESPACE_V2
    );
    assert_eq!(
        ACCEPTED_ENVIRONMENT_KDF_GOLDENS_SCHEMA_V2,
        super::EXPECTED_ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_SCHEMA_V2
    );
    assert_eq!(
        ACCEPTED_ENVIRONMENT_KDF_GOLDENS_FILE_SHA256_V2,
        super::EXPECTED_ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_FILE_SHA256_V2
    );
    assert_eq!(
        ACCEPTED_RESET_GOLDENS_SCHEMA_V2,
        super::EXPECTED_RESET_TRAJECTORY_GOLDENS_SCHEMA_V2
    );
    assert_eq!(
        ACCEPTED_RESET_GENERATOR_IDENTITY_V2,
        super::EXPECTED_RESET_TRAJECTORY_GENERATOR_IDENTITY_V2
    );
    assert_eq!(
        ACCEPTED_RESET_PHYSICAL_PROJECTION_IDENTITY_V2,
        super::EXPECTED_RESET_TRAJECTORY_PHYSICAL_PROJECTION_IDENTITY_V2
    );
    assert_eq!(
        ACCEPTED_RESET_PORTABLE_STREAM_IDENTITY_V2,
        super::EXPECTED_RESET_TRAJECTORY_VECTOR_STREAM_IDENTITY_V2
    );
    assert_eq!(
        ACCEPTED_RESET_GOLDENS_FILE_SHA256_V2,
        super::EXPECTED_RESET_TRAJECTORY_GOLDENS_FILE_SHA256_V2
    );
    assert_eq!(
        ACCEPTED_RESET_PORTABLE_STREAM_SHA256_V2,
        super::EXPECTED_RESET_TRAJECTORY_VECTOR_STREAM_SHA256_V2
    );
    assert_eq!(
        ACCEPTED_TRAINER_SCHEDULE_IDENTITY_V2,
        super::EXPECTED_TRAINER_SCHEDULE_IDENTITY_V2
    );
    assert_eq!(
        ACCEPTED_TRAINER_SEED_VERSION_V2,
        super::EXPECTED_TRAINER_SEED_VERSION_V2
    );
    assert_eq!(
        ACCEPTED_TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_V2,
        super::EXPECTED_TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_V2
    );
    assert_eq!(
        ACCEPTED_RUNTIME_DECK_CATALOG_SCHEMA_V2,
        super::EXPECTED_RUNTIME_DECK_CATALOG_SCHEMA_V2
    );
    assert_eq!(
        ACCEPTED_RUNTIME_DECK_PROTOCOL_V2,
        super::EXPECTED_RUNTIME_DECK_PROTOCOL_V2
    );
    assert_eq!(
        ACCEPTED_RUNTIME_DECK_MATERIALIZATION_PROTOCOL_V2,
        super::EXPECTED_RUNTIME_DECK_MATERIALIZATION_PROTOCOL_V2
    );
    assert_eq!(
        ACCEPTED_RUNTIME_DECK_HASH_ALGORITHM_V2,
        super::EXPECTED_RUNTIME_DECK_HASH_ALGORITHM_V2
    );
    assert_eq!(
        ACCEPTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2,
        super::EXPECTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2
    );

    // 3. B2's literals against the live constants their owning modules export,
    // then production's own argument-free live guard.
    for (field, live, accepted) in live_owner_pairs_v2() {
        assert_eq!(live, accepted, "live owner constant drift at {field}");
    }
    assert_eq!(super::guard_live_source_authorities_v2(), Ok(()));
}

#[test]
fn v2_live_embedded_files_hash_to_the_declared_pins() {
    let artifact = load_sealed_artifact_v2();
    for (field, bytes, accepted) in live_hashed_files_v2() {
        assert_eq!(
            sha256_hex_v2(bytes),
            accepted,
            "live file hash drift at {field}"
        );
    }
    // The declaration is then required to agree with what the live bytes hash
    // to, rather than the other way round.
    let declared = &artifact.source_authorities;
    assert_eq!(
        declared.inner_trajectory.goldens_raw_file_sha256,
        sha256_hex_v2(V1_TRAJECTORY_ARTIFACT_BYTES)
    );
    assert_eq!(
        declared
            .environment_randomization
            .kdf_goldens_raw_file_sha256,
        sha256_hex_v2(ENVIRONMENT_KDF_ARTIFACT_BYTES)
    );
    assert_eq!(
        declared.reset_trajectory.goldens_raw_file_sha256,
        sha256_hex_v2(RESET_TRAJECTORY_ARTIFACT_BYTES)
    );
    assert_eq!(
        declared.trainer_schedule.goldens_raw_file_sha256,
        sha256_hex_v2(TRAINER_SCHEDULE_ARTIFACT_BYTES)
    );
    assert_eq!(
        declared.runtime_deck_catalog.catalog_raw_file_sha256,
        sha256_hex_v2(RUNTIME_DECK_CATALOG_BYTES)
    );
    assert_eq!(
        declared
            .environment_randomization
            .python_reference_raw_file_sha256,
        sha256_hex_v2(ENVIRONMENT_PYTHON_REFERENCE_BYTES)
    );
}

#[test]
fn v2_trainer_schedule_artifact_schema_is_exact() {
    let artifact = load_sealed_artifact_v2();
    let schedule: TrainerScheduleArtifactV2 =
        serde_json::from_slice(TRAINER_SCHEDULE_ARTIFACT_BYTES)
            .expect("the live trainer schedule goldens artifact decodes exactly");

    // The metadata-only schema fact is proven against the live artifact, not
    // against the V2 declaration that carries it.
    assert_eq!(schedule.schema, ACCEPTED_TRAINER_SCHEDULE_GOLDENS_SCHEMA_V2);
    assert_eq!(
        artifact.source_authorities.trainer_schedule.goldens_schema,
        schedule.schema
    );
    // The schedule's other two identity fields must equal the live owner
    // constants and B2's literals alike.
    assert_eq!(
        schedule.schedule_version,
        ACCEPTED_TRAINER_SCHEDULE_IDENTITY_V2
    );
    assert_eq!(
        schedule.schedule_version,
        NATIVE_TRAINER_SCHEDULE_VERSION_V1
    );
    assert_eq!(
        schedule.python_reference_seed_version,
        ACCEPTED_TRAINER_SEED_VERSION_V2
    );
    assert_eq!(
        schedule.python_reference_seed_version,
        PYTHON_REFERENCE_SEED_VERSION_V1
    );
    assert!(schedule.str_atom_probe.is_object());
    assert!(schedule.vectors.is_array());
}

#[test]
fn v2_portable_semantic_stream_is_rebuilt_to_its_pins() {
    let artifact = load_sealed_artifact_v2();
    let (stream, atom_count) = portable_semantic_stream_v2(&artifact);
    assert_eq!(
        atom_count, V2_SEMANTIC_STREAM_ATOM_COUNT,
        "the V2 semantic stream is frozen at exactly one hundred twenty-four atoms"
    );
    assert_eq!(stream.len(), V2_SEMANTIC_STREAM_LEN);
    assert_eq!(sha256_hex_v2(&stream), V2_SEMANTIC_STREAM_SHA256);
}

#[test]
fn v2_positive_cases_match_production_and_the_independent_envelope() {
    let artifact = load_sealed_artifact_v2();
    assert_eq!(artifact.positive_cases.len(), V2_POSITIVE_CASE_COUNT);
    for case in &artifact.positive_cases {
        verify_positive_case_v2(case).unwrap_or_else(|error| panic!("{error}"));
    }
}

#[test]
fn v2_pair_positive_case_matches_production() {
    let artifact = load_sealed_artifact_v2();
    assert_eq!(
        artifact.pair_positive_cases.len(),
        V2_PAIR_POSITIVE_CASE_COUNT
    );
    for case in &artifact.pair_positive_cases {
        verify_pair_positive_case_v2(case).unwrap_or_else(|error| panic!("{error}"));
    }
}

/// The stored streams must have the frozen shapes as well as the frozen digests:
/// the inner stream is the V1 header, one `decision_row` per declared decision,
/// and exactly one `terminal_row`; the V2 stream is exactly thirty-four atoms
/// whose last is the inner digest.  Only the framing and the start-value payloads
/// are inspected, so no V1 row or terminal payload is re-serialised here.
#[test]
fn v2_stored_streams_decompose_into_the_frozen_shapes() {
    let artifact = load_sealed_artifact_v2();

    for case in &artifact.positive_cases {
        let name = case.name.as_str();
        let input = &case.input;
        let inner = parse_hex_vec_v2(&case.inner_stream_hex).expect("inner stream is even hex");
        let atoms = decompose_atom_stream_v2(&inner)
            .unwrap_or_else(|| panic!("{name}: inner stream framing is malformed"));

        let mut expected_tags = vec![
            "domain",
            "episode_index_u64be",
            "environment_seed_u64be",
            "deck_p0_id_utf8",
            "deck_p0_hash_u64be",
            "deck_p1_id_utf8",
            "deck_p1_hash_u64be",
            "learner_seat_u8",
        ];
        expected_tags.extend(std::iter::repeat_n("decision_row", input.decisions.len()));
        expected_tags.push("terminal_row");
        let observed_tags = atoms
            .iter()
            .map(|(tag, _)| tag.as_str())
            .collect::<Vec<_>>();
        assert_eq!(observed_tags, expected_tags, "{name}");

        // The inner header commits to the same start values the V2 envelope does.
        assert_eq!(atoms[0].1.as_slice(), ACCEPTED_INNER_IDENTITY_V2.as_bytes());
        assert_eq!(
            atoms[1].1.as_slice(),
            &parse_u64_hex_v2(&input.episode_index_u64_hex)
                .unwrap()
                .to_be_bytes()[..],
            "{name}"
        );
        assert_eq!(
            atoms[2].1.as_slice(),
            &parse_u64_hex_v2(&input.pair_environment_seed_u64_hex)
                .unwrap()
                .to_be_bytes()[..],
            "{name}: the full u64 pair root is the V1 environment seed, unmasked"
        );
        assert_eq!(atoms[3].1.as_slice(), input.deck_p0_id.as_bytes(), "{name}");
        assert_eq!(
            atoms[4].1.as_slice(),
            &parse_u64_hex_v2(&input.deck_p0_hash_u64_hex)
                .unwrap()
                .to_be_bytes()[..],
            "{name}"
        );
        assert_eq!(atoms[5].1.as_slice(), input.deck_p1_id.as_bytes(), "{name}");
        assert_eq!(
            atoms[6].1.as_slice(),
            &parse_u64_hex_v2(&input.deck_p1_hash_u64_hex)
                .unwrap()
                .to_be_bytes()[..],
            "{name}"
        );
        assert_eq!(
            atoms[7].1.as_slice(),
            &[accepted_seat_code_v2(&input.learner_seat).unwrap()][..],
            "{name}"
        );

        let v2_stream = parse_hex_vec_v2(&case.v2_stream_hex).expect("V2 stream is even hex");
        let v2_atoms = decompose_atom_stream_v2(&v2_stream)
            .unwrap_or_else(|| panic!("{name}: V2 stream framing is malformed"));
        assert_eq!(v2_atoms.len(), ACCEPTED_ENVELOPE_ATOM_COUNT_V2, "{name}");
        let (last_tag, last_payload) = &v2_atoms[ACCEPTED_ENVELOPE_ATOM_COUNT_V2 - 1];
        assert_eq!(last_tag, "inner_trajectory_sha256_raw32", "{name}");
        assert_eq!(
            last_payload.as_slice(),
            &raw32_v2(&case.inner_sha256)[..],
            "{name}"
        );
        assert!(
            v2_atoms
                .iter()
                .all(|(tag, _)| !tag.contains("v2_goldens") && !tag.contains("v2_stream")),
            "{name}"
        );
    }
}

/// Neither V2 self-pin may appear anywhere inside any payload this contract
/// commits to, in raw32 form or in lowercase ASCII-hex form.  A pin smuggled
/// inside a larger payload would not be caught by payload equality, so every
/// check here is a substring search.  The per-trajectory V2 digests are a
/// different thing entirely and are required to be present.
#[test]
fn v2_self_pins_never_appear_inside_any_committed_payload() {
    let artifact = load_sealed_artifact_v2();
    let mut payloads: Vec<(String, Vec<u8>)> = Vec::new();
    payloads.push((
        "source_authorities canonical JSON".to_string(),
        canonical_json_no_lf_v2(&artifact.source_authorities),
    ));
    for case in &artifact.positive_cases {
        let stored = parse_hex_vec_v2(&case.v2_stream_hex).expect("stored V2 stream is even hex");
        let inner_stream =
            parse_hex_vec_v2(&case.inner_stream_hex).expect("stored inner stream is even hex");
        let local_inner = raw32_v2(&sha256_hex_v2(&inner_stream));
        let rebuilt = independent_v2_envelope_v2(&case.input, local_inner);
        payloads.push((format!("{}: stored 34-atom envelope", case.name), stored));
        payloads.push((format!("{}: rebuilt 34-atom envelope", case.name), rebuilt));
    }
    let (semantic_stream, atom_count) = portable_semantic_stream_v2(&artifact);
    assert_eq!(atom_count, V2_SEMANTIC_STREAM_ATOM_COUNT);
    payloads.push((
        "rebuilt 124-atom semantic stream".to_string(),
        semantic_stream,
    ));
    assert_eq!(payloads.len(), 2 * V2_POSITIVE_CASE_COUNT + 2);

    for (pin_label, pin) in [
        ("the V2 artifact self-pin", V2_ARTIFACT_RAW_SHA256),
        ("the V2 semantic stream self-pin", V2_SEMANTIC_STREAM_SHA256),
    ] {
        let raw = raw32_v2(pin);
        for (label, payload) in &payloads {
            assert!(
                !contains_subslice_v2(payload, &raw),
                "{label} contains {pin_label} in raw32 form"
            );
            assert!(
                !contains_subslice_v2(payload, pin.as_bytes()),
                "{label} contains {pin_label} in lowercase ASCII-hex form"
            );
        }
        // The search itself is proven non-vacuous: it does find the pin when the
        // pin really is inside a larger payload.
        let mut planted = b"prefix-".to_vec();
        planted.extend_from_slice(&raw);
        planted.extend_from_slice(b"-suffix");
        assert!(contains_subslice_v2(&planted, &raw));
        let mut planted_hex = b"prefix-".to_vec();
        planted_hex.extend_from_slice(pin.as_bytes());
        planted_hex.extend_from_slice(b"-suffix");
        assert!(contains_subslice_v2(&planted_hex, pin.as_bytes()));
    }

    // The per-trajectory V2 digests are legitimate and are not confused with the
    // two top-level self-pins.
    for case in &artifact.positive_cases {
        assert_ne!(case.v2_sha256, V2_ARTIFACT_RAW_SHA256);
        assert_ne!(case.v2_sha256, V2_SEMANTIC_STREAM_SHA256);
        assert_ne!(case.inner_sha256, V2_ARTIFACT_RAW_SHA256);
        assert_ne!(case.inner_sha256, V2_SEMANTIC_STREAM_SHA256);
    }
}

/// Explicit output tamper: a single flipped byte in either committed output must
/// break the pin it is measured against.
#[test]
fn v2_output_tamper_is_detected() {
    let artifact = load_sealed_artifact_v2();

    let (stream, _) = portable_semantic_stream_v2(&artifact);
    for offset in [0_usize, 1, stream.len() / 2, stream.len() - 1] {
        let mut tampered = stream.clone();
        tampered[offset] ^= 0x01;
        assert_ne!(tampered, stream);
        assert_eq!(tampered.len(), V2_SEMANTIC_STREAM_LEN);
        assert_ne!(
            sha256_hex_v2(&tampered),
            V2_SEMANTIC_STREAM_SHA256,
            "a flipped byte at offset {offset} must break the semantic stream pin"
        );
    }
    // Truncation and extension are tamper too.
    assert_ne!(stream[..stream.len() - 1].len(), V2_SEMANTIC_STREAM_LEN);
    assert_ne!(
        sha256_hex_v2(&stream[..stream.len() - 1]),
        V2_SEMANTIC_STREAM_SHA256
    );

    let case = positive_case_by_name_v2(&artifact, T0_POSITIVE_CASE_NAME_V2);
    let inner_stream = parse_hex_vec_v2(&case.inner_stream_hex).expect("inner stream is even hex");
    let local_inner = raw32_v2(&sha256_hex_v2(&inner_stream));
    let envelope = independent_v2_envelope_v2(&case.input, local_inner);
    let receipt = run_trajectory_case_v2(&case.input).expect("the baseline is admitted");
    assert_eq!(
        Sha256::digest(&envelope).as_slice(),
        receipt.trajectory_sha256_v2.as_slice()
    );
    for offset in [0_usize, envelope.len() / 2, envelope.len() - 1] {
        let mut tampered = envelope.clone();
        tampered[offset] ^= 0x01;
        assert_ne!(
            Sha256::digest(&tampered).as_slice(),
            receipt.trajectory_sha256_v2.as_slice(),
            "a flipped byte at offset {offset} must break the envelope receipt"
        );
    }
    // A tampered inner digest is likewise a different envelope.
    let mut drifted_inner = local_inner;
    drifted_inner[31] ^= 0x01;
    assert_ne!(
        independent_v2_envelope_v2(&case.input, drifted_inner),
        envelope
    );
}

// ------------------------------------------- independent rejection reconstruction

/// The two baselines are rebuilt from B2's own fixture literals and must equal
/// the stored positives found by literal name.  Everything downstream clones the
/// rebuilt values, so no reconstructed rejection borrows a byte from the stored
/// reject fixtures.
#[test]
fn v2_reject_baselines_are_rebuilt_from_b2_literals() {
    let artifact = load_sealed_artifact_v2();

    let t0 = rebuilt_t0_input_v2();
    let stored_t0 = positive_case_by_name_v2(&artifact, T0_POSITIVE_CASE_NAME_V2);
    assert_eq!(t0, stored_t0.input, "T0 rebuild drift");

    let p0 = rebuilt_p0_pair_input_v2();
    let stored_p0 = pair_positive_case_by_name_v2(&artifact, P0_PAIR_POSITIVE_CASE_NAME_V2);
    assert_eq!(p0, stored_p0.input, "P0 rebuild drift");

    // The rebuilt baseline really is the accepted fixture shape, not an
    // accidental match of an empty or degenerate value.
    assert_eq!(t0.episode_index_u64_hex, u64_hex_v2(0));
    assert_eq!(
        t0.pair_environment_seed_u64_hex,
        u64_hex_v2(ACCEPTED_NATIVE_ROOT_V2)
    );
    assert_eq!(t0.deck_p0_id, ACCEPTED_BURN_DECK_ID_V2);
    assert_eq!(t0.deck_p1_id, ACCEPTED_RALLY_DECK_ID_V2);
    assert_eq!(t0.learner_seat, "p0");
    assert_eq!(t0.decisions.len(), 5);
    assert_eq!(t0.terminal.outcome, "p0-win");
    assert_eq!(t0.terminal.classification, "natural");
    assert_eq!(t0.terminal.policy_step_count_u64_hex, u64_hex_v2(5));
    assert_eq!(t0.terminal.physical_decision_count_u64_hex, u64_hex_v2(3));
    assert_eq!(
        t0.decisions[0].flat_action_v2_commitment_hex,
        "00000000000000000000000000c00000"
    );
    assert_eq!(
        p0.base_seed_u64_hex,
        u64_hex_v2(ACCEPTED_NATIVE_BASE_SEED_V2)
    );
    assert_eq!(
        p0.pair_index_u64_hex,
        u64_hex_v2(ACCEPTED_NATIVE_PAIR_INDEX_V2)
    );
    assert_eq!(p0.even_start, t0);
    assert_eq!(p0.odd_start.learner_seat, "p1");
    assert_eq!(p0.odd_start.decisions.len(), 4);

    // Both baselines are admitted, so every rejection below is caused by the
    // named mutation and not by the base.
    assert!(run_trajectory_case_v2(&t0).is_ok());
    assert!(run_pair_case_v2(&p0).is_ok());
}

#[test]
fn v2_trajectory_reject_cases_are_independently_reconstructed() {
    let artifact = load_sealed_artifact_v2();
    let baseline = rebuilt_t0_input_v2();
    assert_eq!(
        baseline,
        positive_case_by_name_v2(&artifact, T0_POSITIVE_CASE_NAME_V2).input
    );

    // The stored names must be exactly this ordered literal array.
    let stored_names = artifact
        .trajectory_reject_cases
        .iter()
        .map(|case| case.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(stored_names.as_slice(), &TRAJECTORY_REJECT_NAMES_V2[..]);
    assert_eq!(
        TRAJECTORY_REJECT_NAMES_V2.len(),
        V2_TRAJECTORY_REJECT_CASE_COUNT
    );

    for (index, case) in artifact.trajectory_reject_cases.iter().enumerate() {
        verify_trajectory_reject_case_v2(TRAJECTORY_REJECT_NAMES_V2[index], case, &baseline)
            .unwrap_or_else(|error| panic!("{error}"));
    }

    // The compound and coordinated cases carry exactly the shape the accepted
    // generator gives them, restated here so a silently simplified
    // reconstruction cannot pass.
    let (compound_empty, compound_empty_code) = reconstruct_trajectory_reject_v2(
        "compound-empty-stream-and-terminal-episode-mismatch",
        &baseline,
    );
    assert!(compound_empty.decisions.is_empty());
    assert_eq!(compound_empty.terminal.episode_index_u64_hex, u64_hex_v2(9));
    assert_eq!(
        compound_empty.terminal.policy_step_count_u64_hex,
        u64_hex_v2(0)
    );
    assert_eq!(
        compound_empty.terminal.physical_decision_count_u64_hex,
        u64_hex_v2(0)
    );
    assert_eq!(compound_empty_code, "episode-mismatch");

    let (compound_open, compound_open_code) = reconstruct_trajectory_reject_v2(
        "compound-open-group-and-terminal-episode-mismatch",
        &baseline,
    );
    assert_eq!(compound_open.decisions.last().unwrap().substep_count_u32, 9);
    assert_eq!(compound_open.terminal.episode_index_u64_hex, u64_hex_v2(9));
    assert_eq!(compound_open_code, "episode-mismatch");

    let (empty, empty_code) = reconstruct_trajectory_reject_v2("empty-decision-stream", &baseline);
    assert!(empty.decisions.is_empty());
    assert_eq!(empty.terminal.policy_step_count_u64_hex, u64_hex_v2(0));
    assert_eq!(
        empty.terminal.physical_decision_count_u64_hex,
        u64_hex_v2(0)
    );
    assert_eq!(
        empty.terminal.episode_index_u64_hex,
        baseline.terminal.episode_index_u64_hex
    );
    assert_eq!(empty_code, "empty-decision-stream");

    let (non_natural, non_natural_code) =
        reconstruct_trajectory_reject_v2("non-natural-terminal", &baseline);
    assert_eq!(non_natural.terminal.outcome, "truncated");
    assert_eq!(non_natural.terminal.winner, "none");
    assert_eq!(non_natural.terminal.classification, "truncated");
    assert_eq!(non_natural.terminal.terminal_code, "decision-cap");
    assert_eq!(non_natural_code, "non-natural-terminal");
}

#[test]
fn v2_pair_reject_cases_are_independently_reconstructed() {
    let artifact = load_sealed_artifact_v2();
    let baseline = rebuilt_p0_pair_input_v2();
    assert_eq!(
        baseline,
        pair_positive_case_by_name_v2(&artifact, P0_PAIR_POSITIVE_CASE_NAME_V2).input
    );

    let stored_names = artifact
        .pair_reject_cases
        .iter()
        .map(|case| case.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(stored_names.as_slice(), &PAIR_REJECT_NAMES_V2[..]);
    assert_eq!(PAIR_REJECT_NAMES_V2.len(), V2_PAIR_REJECT_CASE_COUNT);

    for (index, case) in artifact.pair_reject_cases.iter().enumerate() {
        verify_pair_reject_case_v2(PAIR_REJECT_NAMES_V2[index], case, &baseline)
            .unwrap_or_else(|error| panic!("{error}"));
    }

    // The two coordinated pair cases carry exactly their generator shape.
    let (drifted, drifted_code) =
        reconstruct_pair_reject_v2("pair-odd-episode-index-drift", &baseline);
    assert_eq!(drifted.odd_start.episode_index_u64_hex, u64_hex_v2(3));
    assert_eq!(
        drifted.odd_start.terminal.episode_index_u64_hex,
        u64_hex_v2(3)
    );
    assert_eq!(drifted.odd_start.terminal.outcome, "p0-win");
    assert_eq!(drifted.odd_start.terminal.winner, "p0");
    assert_eq!(drifted.odd_start.learner_seat, "p1");
    assert_eq!(drifted.even_start, baseline.even_start);
    assert_eq!(drifted_code, "pair-episode-index-mismatch");
    // The drifted odd start is internally valid on its own: only the pair-stage
    // binding fails.
    assert!(run_trajectory_case_v2(&drifted.odd_start).is_ok());

    let (swapped, swapped_code) =
        reconstruct_pair_reject_v2("pair-odd-physical-deck-swap", &baseline);
    assert_eq!(swapped.odd_start.deck_p0_id, ACCEPTED_RALLY_DECK_ID_V2);
    assert_eq!(swapped.odd_start.deck_p1_id, ACCEPTED_BURN_DECK_ID_V2);
    assert_eq!(
        swapped.odd_start.deck_p0_hash_u64_hex,
        u64_hex_v2(ACCEPTED_RALLY_DECK_HASH_V2)
    );
    assert_eq!(
        swapped.odd_start.deck_p1_hash_u64_hex,
        u64_hex_v2(ACCEPTED_BURN_DECK_HASH_V2)
    );
    assert_eq!(
        swapped.odd_start.terminal.deck_p0_hash_u64_hex,
        u64_hex_v2(ACCEPTED_RALLY_DECK_HASH_V2)
    );
    assert_eq!(
        swapped.odd_start.terminal.deck_p1_hash_u64_hex,
        u64_hex_v2(ACCEPTED_BURN_DECK_HASH_V2)
    );
    assert_eq!(swapped.even_start, baseline.even_start);
    assert_eq!(swapped_code, "pair-physical-deck-binding-mismatch");
    assert!(run_trajectory_case_v2(&swapped.odd_start).is_ok());
}

/// A stored expected code that drifts to a *different valid* code still passes
/// the closed-vocabulary rule, so only the literal reconstruction can catch it.
/// An unknown code is caught earlier, by layer D.
#[test]
fn v2_expected_code_drift_is_detected() {
    let baseline = rebuilt_t0_input_v2();

    let mutant = mutated_artifact_value_v2(
        "trajectory_reject_cases/0/expected_code",
        serde_json::json!("counter-overflow"),
    );
    let artifact = decode_mutant_v2(&mutant);
    assert_eq!(
        artifact.trajectory_reject_cases[0].expected_code,
        "counter-overflow"
    );
    let error = verify_trajectory_reject_case_v2(
        TRAJECTORY_REJECT_NAMES_V2[0],
        &artifact.trajectory_reject_cases[0],
        &baseline,
    )
    .expect_err("a drifted expected code must be caught");
    assert!(error.contains("is not the stored code"), "{error}");

    let pair_mutant = mutated_artifact_value_v2(
        "pair_reject_cases/0/expected_code",
        serde_json::json!("pair-environment-seed-mismatch"),
    );
    let pair_artifact = decode_mutant_v2(&pair_mutant);
    let pair_error = verify_pair_reject_case_v2(
        PAIR_REJECT_NAMES_V2[0],
        &pair_artifact.pair_reject_cases[0],
        &rebuilt_p0_pair_input_v2(),
    )
    .expect_err("a drifted pair expected code must be caught");
    assert!(
        pair_error.contains("is not the stored code"),
        "{pair_error}"
    );

    // An unknown code never reaches the reconstruction: layer D rejects it.
    let unknown = mutated_artifact_value_v2(
        "trajectory_reject_cases/0/expected_code",
        serde_json::json!("not-a-code"),
    );
    assert!(loader_error_of_value_v2(&unknown).contains("closed vocabulary"));

    // A drifted reject *input* is caught as well, even when its code is intact.
    let input_drift = mutated_artifact_value_v2(
        "trajectory_reject_cases/0/input/decisions/0/actor_seat",
        serde_json::json!("p1"),
    );
    let input_artifact = decode_mutant_v2(&input_drift);
    let input_error = verify_trajectory_reject_case_v2(
        TRAJECTORY_REJECT_NAMES_V2[0],
        &input_artifact.trajectory_reject_cases[0],
        &baseline,
    )
    .expect_err("a drifted reject input must be caught");
    assert!(
        input_error.contains("is not the stored input"),
        "{input_error}"
    );
}

/// A same-shape one-nibble drift survives every shape rule in layer D, so only a
/// recomputation can catch it.  All four stored positive fields and both stored
/// pair digests are covered.
#[test]
fn v2_stored_stream_and_digest_nibble_drift_is_detected() {
    for path in [
        "positive_cases/0/inner_sha256",
        "positive_cases/0/inner_stream_hex",
        "positive_cases/0/v2_sha256",
        "positive_cases/0/v2_stream_hex",
    ] {
        let artifact = nibble_drifted_artifact_v2(path);
        let error = verify_positive_case_v2(&artifact.positive_cases[0])
            .expect_err("a one-nibble drift must be caught");
        assert!(!error.is_empty(), "{path}");
    }
    for path in [
        "pair_positive_cases/0/even_trajectory_sha256",
        "pair_positive_cases/0/odd_trajectory_sha256",
    ] {
        let artifact = nibble_drifted_artifact_v2(path);
        let error = verify_pair_positive_case_v2(&artifact.pair_positive_cases[0])
            .expect_err("a one-nibble pair digest drift must be caught");
        assert!(
            error.contains("is not the stored digest"),
            "{path}: {error}"
        );
    }
}

// ------------------------------------------------------- closed code vocabulary

#[test]
fn v2_rejection_vocabulary_is_closed_and_fully_covered() {
    let artifact = load_sealed_artifact_v2();
    let codes = closed_rejection_codes_v2();
    assert_eq!(codes.len(), 22);

    // The table is a set: no two variants share a portable code.
    let unique: BTreeSet<&str> = codes.iter().copied().collect();
    assert_eq!(unique.len(), codes.len());

    // Exact strings, in the frozen order.
    assert_eq!(
        codes,
        [
            "authority-mismatch",
            "episode-index-outside-u63",
            "learner-seat-rule-mismatch",
            "invalid-deck-id",
            "runtime-deck-hash-mismatch",
            "empty-decision-stream",
            "episode-mismatch",
            "row-ordinal-mismatch",
            "actor-role-mismatch",
            "malformed-physical-group",
            "invalid-legal-action-count",
            "selected-index-out-of-range",
            "malformed-commitment",
            "counter-overflow",
            "non-natural-terminal",
            "terminal-provenance-mismatch",
            "terminal-count-mismatch",
            "schedule-integer-outside-u63",
            "pair-index-outside-episode-domain",
            "pair-episode-index-mismatch",
            "pair-environment-seed-mismatch",
            "pair-physical-deck-binding-mismatch",
        ]
    );

    // The artifact's rejection set is every code except counter-overflow, and
    // every declared code was actually produced by production in the
    // reconstruction tests above.
    let observed: BTreeSet<&str> = artifact
        .trajectory_reject_cases
        .iter()
        .map(|case| case.expected_code.as_str())
        .chain(
            artifact
                .pair_reject_cases
                .iter()
                .map(|case| case.expected_code.as_str()),
        )
        .collect();
    let expected: BTreeSet<&str> = unique
        .iter()
        .copied()
        .filter(|code| *code != "counter-overflow")
        .collect();
    assert_eq!(observed, expected);
    assert_eq!(observed.len(), 21);
}

/// B2's own variant-to-code mapping is a wildcard-free match over every variant
/// of the production enum, so a new production variant fails to compile until it
/// is handled here.  It must agree with production's own mapping everywhere.
#[test]
fn v2_rejection_vocabulary_is_compile_exhaustive() {
    let variants = closed_rejection_variants_v2();
    assert_eq!(variants.len(), 22);
    let unique: BTreeSet<&str> = variants
        .iter()
        .map(|variant| accepted_portable_code_v2(*variant))
        .collect();
    assert_eq!(unique.len(), variants.len());
    for variant in variants {
        assert_eq!(
            accepted_portable_code_v2(variant),
            variant.portable_code_v2(),
            "B2 and production disagree on {variant:?}"
        );
    }
}

/// Counter overflow is the one code no artifact case can carry: reaching it
/// requires driving a V1 accumulator's counters to `u64::MAX`, which needs 2^64
/// accepted rows.  It is covered here where it actually lives, in the total
/// mapping from the frozen V1 vocabulary into the closed V2 vocabulary, without
/// adding an API or fabricating an artifact case.
#[test]
fn v2_counter_overflow_is_covered_through_the_inner_mapping() {
    assert_eq!(
        super::map_inner_error_v2(NativeFullEpisodeTrajectoryErrorV1::CounterOverflow),
        NativeFullEpisodeTrajectoryErrorV2::CounterOverflow
    );
    assert_eq!(
        accepted_portable_code_v2(NativeFullEpisodeTrajectoryErrorV2::CounterOverflow),
        "counter-overflow"
    );
    assert_eq!(
        NativeFullEpisodeTrajectoryErrorV2::CounterOverflow.portable_code_v2(),
        "counter-overflow"
    );
    assert!(closed_rejection_codes_v2().contains(&"counter-overflow"));
}

#[test]
fn v2_envelope_atom_count_agrees_with_production() {
    assert_eq!(
        ACCEPTED_ENVELOPE_ATOM_COUNT_V2,
        NATIVE_FULL_EPISODE_TRAJECTORY_ENVELOPE_ATOM_COUNT_V2
    );
}

// ------------------------------------------------------- layer B: scanner negatives
//
// Every test here calls the scanner directly on a crafted document.  None of them
// can be satisfied by a serde failure, because serde is not on this path.

#[test]
fn v2_raw_scanner_accepts_the_accepted_artifact_and_ordinary_documents() {
    assert_eq!(scan_raw_document_v2(V2_ARTIFACT_BYTES), Ok(()));
    for accepted in [
        "{}",
        "[]",
        "0",
        "-17",
        "\"text\"",
        "true",
        "false",
        "null",
        "{\"a\":[1,2,{\"b\":null},true,false,\"s\"]}",
        "{\"a\":{\"b\":{\"c\":[0,1,2]}}}",
        " {\"a\":1} \n",
        "{\"a\":1,\"b\":1}",
    ] {
        assert_eq!(
            scan_raw_document_v2(accepted.as_bytes()),
            Ok(()),
            "{accepted:?}"
        );
    }
}

#[test]
fn v2_raw_scanner_rejects_duplicate_keys_at_every_depth() {
    for document in [
        "{\"a\":1,\"a\":2}",
        "{\"outer\":{\"a\":1,\"a\":2}}",
        "{\"outer\":[{\"a\":1,\"a\":2}]}",
        "{\"a\":{\"b\":[{\"c\":{\"d\":1,\"d\":2}}]}}",
        "[[[{\"deep\":1,\"deep\":2}]]]",
        "{\"schema\":\"x\",\"schema\":\"y\"}",
    ] {
        let error = scan_raw_document_v2(document.as_bytes())
            .expect_err("a duplicate key is rejected at every depth");
        assert!(
            error.contains("duplicate object key"),
            "{document:?}: {error}"
        );
    }

    // Two spellings of one key collide, so an escape cannot smuggle a duplicate
    // past the scanner.
    let escaped = "{\"a\":1,\"\\u0061\":2}";
    let error =
        scan_raw_document_v2(escaped.as_bytes()).expect_err("an escaped duplicate is rejected");
    assert!(error.contains("duplicate object key"), "{error}");

    // Distinct keys are not duplicates.
    assert_eq!(scan_raw_document_v2("{\"a\":1,\"b\":2}".as_bytes()), Ok(()));
}

#[test]
fn v2_raw_scanner_rejects_every_float_and_nonfinite_spelling() {
    for document in [
        "{\"a\":1.0}",
        "{\"a\":-0.5}",
        "{\"a\":1e999}",
        "{\"a\":1E5}",
        "{\"a\":1e5}",
        "{\"a\":1e-5}",
        "[1.5]",
        "{\"a\":{\"b\":[{\"c\":2.5}]}}",
        "4.5",
    ] {
        let error = scan_raw_document_v2(document.as_bytes())
            .expect_err("a float literal is rejected at every depth");
        assert!(error.contains("float literal"), "{document:?}: {error}");
    }
    // Every declared spelling, driven from the constant itself so the coverage is
    // exhaustive by construction, at four different depths each.
    assert_eq!(NONFINITE_SPELLINGS_V2.len(), 12);
    let unique: BTreeSet<&str> = NONFINITE_SPELLINGS_V2.iter().copied().collect();
    assert_eq!(unique.len(), NONFINITE_SPELLINGS_V2.len());
    for spelling in NONFINITE_SPELLINGS_V2 {
        for document in [
            spelling.to_string(),
            format!("[{spelling}]"),
            format!("{{\"a\":{spelling}}}"),
            format!("{{\"a\":{{\"b\":[{{\"c\":{spelling}}}]}}}}"),
        ] {
            let error = scan_raw_document_v2(document.as_bytes())
                .expect_err("a nonfinite spelling is rejected at every depth");
            assert!(
                error.contains("nonfinite numeric literal"),
                "{document:?}: {error}"
            );
        }
    }
    // Integers of both signs remain acceptable.
    assert_eq!(
        scan_raw_document_v2("{\"a\":-1,\"b\":0}".as_bytes()),
        Ok(())
    );
}

/// The same token vocabulary, driven end to end through layer C on real raw
/// documents.  Each mutant is built by byte surgery on the accepted artifact
/// rather than through `serde_json::Value`, because a `Value` round trip would
/// normalise or refuse these spellings before they ever reached the file, and the
/// point of this test is that the *raw* bytes are refused.  The assertion is on
/// the scanner's own message, which proves the scanner ran before typed serde
/// rather than after it.
#[test]
fn v2_loader_rejects_every_raw_float_and_nonfinite_token_before_typed_decoding() {
    let text = std::str::from_utf8(V2_ARTIFACT_BYTES).expect("artifact is UTF-8");
    // An integer token that occurs in the accepted artifact and can carry any
    // replacement spelling in a value position.
    const INTEGER_TOKEN_V2: &str = "\"legal_action_count_u32\":4";
    assert!(text.contains(INTEGER_TOKEN_V2));

    let mutate = |replacement: &str| {
        let document = text.replacen(
            INTEGER_TOKEN_V2,
            &format!("\"legal_action_count_u32\":{replacement}"),
            1,
        );
        assert_ne!(document.as_bytes(), V2_ARTIFACT_BYTES);
        // The raw byte gate must not be what refuses these: every replacement is
        // printable ASCII and the single final LF is preserved.
        assert!(document.is_ascii());
        assert!(document.ends_with('\n'));
        assert_eq!(document.matches('\n').count(), 1);
        document
    };

    for replacement in ["1.0", "4.0", "-0.5", "0.0001"] {
        let document = mutate(replacement);
        let error = load_unsealed_artifact_v2(document.as_bytes())
            .err()
            .unwrap_or_else(|| panic!("{replacement} must be refused"));
        assert!(
            error.contains("float literal") && error.contains("fraction"),
            "{replacement}: {error}"
        );
        assert!(!error.contains("typed decode failed"), "{replacement}");
    }
    for replacement in ["1e999", "1E5", "1e5", "1e-5", "4E+2"] {
        let document = mutate(replacement);
        let error = load_unsealed_artifact_v2(document.as_bytes())
            .err()
            .unwrap_or_else(|| panic!("{replacement} must be refused"));
        assert!(
            error.contains("float literal") && error.contains("exponent"),
            "{replacement}: {error}"
        );
        assert!(!error.contains("typed decode failed"), "{replacement}");
    }
    assert_eq!(NONFINITE_SPELLINGS_V2.len(), 12);
    for replacement in NONFINITE_SPELLINGS_V2 {
        let document = mutate(replacement);
        let error = load_unsealed_artifact_v2(document.as_bytes())
            .err()
            .unwrap_or_else(|| panic!("{replacement} must be refused"));
        assert!(
            error.contains("nonfinite numeric literal"),
            "{replacement}: {error}"
        );
        assert!(!error.contains("typed decode failed"), "{replacement}");
    }

    // The surgery itself is non-vacuous: at this exact site a well-formed integer
    // is scanned, decoded, re-encoded canonically, and admitted by layer D.  Every
    // rejection above is therefore caused by the token spelling alone and not by
    // the edit, its position, or a collateral rule.
    let integer_document = mutate("5");
    assert_eq!(scan_raw_document_v2(integer_document.as_bytes()), Ok(()));
    assert!(
        load_unsealed_artifact_v2(integer_document.as_bytes()).is_ok(),
        "the surgery site accepts a well-formed integer"
    );
}

#[test]
fn v2_raw_scanner_rejects_malformed_documents() {
    for document in [
        "{\"a\":1}}",
        "{\"a\" 1}",
        "{\"a\":tru}",
        "{\"a\":}",
        "{\"a\":01}",
        "[1,]",
        "\"unterminated",
        "{\"a\":\"unterminated}",
        "",
    ] {
        assert!(
            scan_raw_document_v2(document.as_bytes()).is_err(),
            "{document:?} must not scan"
        );
    }
    // Nesting past the depth cap is refused rather than recursed into.
    let deep = format!(
        "{}1{}",
        "[".repeat(MAX_RAW_SCAN_DEPTH_V2 + 4),
        "]".repeat(MAX_RAW_SCAN_DEPTH_V2 + 4)
    );
    let error = scan_raw_document_v2(deep.as_bytes()).expect_err("excess nesting is rejected");
    assert!(error.contains("scan depth cap"), "{error}");
}

// ---------------------------------------------------------- layer C: raw negatives

#[test]
fn v2_raw_gate_rejects_a_bom() {
    let mut mutant = vec![0xef_u8, 0xbb, 0xbf];
    mutant.extend_from_slice(V2_ARTIFACT_BYTES);
    let error = load_unsealed_artifact_v2(&mutant).expect_err("a BOM is rejected");
    assert!(error.contains("BOM"), "{error}");
}

#[test]
fn v2_raw_gate_rejects_a_carriage_return() {
    let mut mutant = V2_ARTIFACT_BYTES.to_vec();
    let last = mutant.len() - 1;
    mutant.insert(last, b'\r');
    let error = load_unsealed_artifact_v2(&mutant).expect_err("a carriage return is rejected");
    assert!(error.contains("carriage return"), "{error}");
}

#[test]
fn v2_raw_gate_rejects_lf_drift() {
    let missing_lf = &V2_ARTIFACT_BYTES[..V2_ARTIFACT_BYTES.len() - 1];
    assert!(load_unsealed_artifact_v2(missing_lf)
        .expect_err("a missing final LF is rejected")
        .contains("exactly one LF"));

    let mut doubled = V2_ARTIFACT_BYTES.to_vec();
    doubled.push(b'\n');
    assert!(load_unsealed_artifact_v2(&doubled)
        .expect_err("a second LF is rejected")
        .contains("exactly one LF"));

    let mut interior = V2_ARTIFACT_BYTES.to_vec();
    interior.insert(1, b'\n');
    assert!(load_unsealed_artifact_v2(&interior)
        .expect_err("an interior LF is rejected")
        .contains("exactly one LF"));
}

/// With exactly one final LF proven, every remaining byte must be printable
/// ASCII.  A control byte and a DEL are both rejected by the gate itself, not by
/// a downstream parser: neither is a JSON syntax error inside a string, so
/// serde parse and re-encode is not a substitute for this rule.
#[test]
fn v2_raw_gate_rejects_control_and_del_bytes() {
    for byte in [0x00_u8, 0x01, 0x09, 0x0b, 0x1f, 0x7f] {
        let mut mutant = V2_ARTIFACT_BYTES.to_vec();
        let last = mutant.len() - 1;
        mutant.insert(last, byte);
        assert!(mutant.is_ascii());
        assert_eq!(mutant.last(), Some(&b'\n'));
        assert_eq!(mutant.iter().filter(|value| **value == b'\n').count(), 1);
        let error = load_unsealed_artifact_v2(&mutant)
            .err()
            .unwrap_or_else(|| panic!("byte {byte:#04x} must be rejected"));
        assert!(
            error.contains("printable ASCII"),
            "byte {byte:#04x}: {error}"
        );
    }

    // The same rule inside a string body, where JSON alone would be permissive
    // about an unescaped DEL.
    let text = std::str::from_utf8(V2_ARTIFACT_BYTES).expect("artifact is UTF-8");
    let del_in_string = text.replacen("\"Burn\"", "\"Bur\u{7f}\"", 1);
    assert_eq!(del_in_string.len(), text.len());
    let error = load_unsealed_artifact_v2(del_in_string.as_bytes())
        .expect_err("a DEL inside a string is rejected");
    assert!(error.contains("printable ASCII"), "{error}");
}

#[test]
fn v2_raw_gate_rejects_non_ascii() {
    let mut mutant = V2_ARTIFACT_BYTES.to_vec();
    let last = mutant.len() - 1;
    mutant.insert(last, 0x80);
    let error = load_unsealed_artifact_v2(&mutant).expect_err("non-ASCII bytes are rejected");
    assert!(error.contains("ASCII"), "{error}");
}

#[test]
fn v2_raw_gate_rejects_an_empty_artifact() {
    let error = load_unsealed_artifact_v2(&[]).expect_err("an empty artifact is rejected");
    assert!(error.contains("empty"), "{error}");
}

/// The global size cap is the first rule layer C applies, so it is reachable:
/// a cap-sized document is measured by the later rules, and a cap-plus-one
/// document is refused before anything reads a byte of it.
#[test]
fn v2_raw_gate_size_cap_is_reachable() {
    let at_cap = vec![b'x'; MAX_GOLDEN_ARTIFACT_BYTES_V2];
    let at_cap_error = load_unsealed_artifact_v2(&at_cap)
        .expect_err("a cap-sized non-artifact is still rejected, but not by the cap");
    assert!(
        !at_cap_error.contains("four-mebibyte cap"),
        "{at_cap_error}"
    );

    let over_cap = vec![b'x'; MAX_GOLDEN_ARTIFACT_BYTES_V2 + 1];
    let over_cap_error =
        load_unsealed_artifact_v2(&over_cap).expect_err("an oversized artifact is rejected");
    assert!(
        over_cap_error.contains("four-mebibyte cap"),
        "{over_cap_error}"
    );
    assert!(V2_ARTIFACT_BYTES.len() < MAX_GOLDEN_ARTIFACT_BYTES_V2);
}

#[test]
fn v2_loader_rejects_trailing_bytes_after_the_document() {
    let mut mutant = V2_ARTIFACT_BYTES.to_vec();
    let last = mutant.len() - 1;
    mutant.insert(last, b' ');
    let error = load_unsealed_artifact_v2(&mutant).expect_err("trailing bytes are rejected");
    assert!(error.contains("canonical"), "{error}");
}

#[test]
fn v2_loader_rejects_key_order_and_canonical_drift() {
    // Two adjacent members of one decision object swapped out of sorted order.
    // The document still decodes to the identical typed value and carries no
    // duplicate key, so only the canonical byte comparison can catch it.
    let text = std::str::from_utf8(V2_ARTIFACT_BYTES).expect("artifact is UTF-8");
    let reordered = text.replacen(
        "\"actor_role\":\"learner\",\"actor_seat\":\"p0\"",
        "\"actor_seat\":\"p0\",\"actor_role\":\"learner\"",
        1,
    );
    assert_ne!(reordered.as_bytes(), V2_ARTIFACT_BYTES);
    assert_eq!(reordered.len(), V2_ARTIFACT_BYTES.len());
    let error =
        load_unsealed_artifact_v2(reordered.as_bytes()).expect_err("key order drift is rejected");
    assert!(error.contains("canonical"), "{error}");

    // An equivalent escape spelling decodes to the same string and is likewise
    // not the canonical encoding.
    let escaped = text.replacen(
        "\"learner_seat\":\"p0\"",
        "\"learner_seat\":\"\\u00700\"",
        1,
    );
    assert_ne!(escaped.as_bytes(), V2_ARTIFACT_BYTES);
    let error =
        load_unsealed_artifact_v2(escaped.as_bytes()).expect_err("escape drift is rejected");
    assert!(error.contains("canonical"), "{error}");

    // Pretty-printed spacing is likewise non-canonical.
    let artifact = load_sealed_artifact_v2();
    let mut pretty = serde_json::to_vec_pretty(
        &serde_json::to_value(&artifact).expect("artifact converts to JSON"),
    )
    .expect("pretty encoding succeeds");
    pretty.push(b'\n');
    let error = load_unsealed_artifact_v2(&pretty).expect_err("pretty spacing is rejected");
    assert!(
        error.contains("exactly one LF") || error.contains("canonical"),
        "{error}"
    );
}

#[test]
fn v2_loader_rejects_a_duplicate_key_before_typed_decoding() {
    // A duplicate cannot survive a `Value` round trip, so the mutant is built as
    // raw bytes: the accepted document with its final `}` replaced by a repeated
    // top-level member.
    let text = std::str::from_utf8(V2_ARTIFACT_BYTES).expect("artifact is UTF-8");
    let body = text.trim_end_matches('\n');
    let duplicated = format!("{},\"schema\":\"x\"}}\n", &body[..body.len() - 1]);
    let error =
        load_unsealed_artifact_v2(duplicated.as_bytes()).expect_err("a duplicate key is rejected");
    assert!(error.contains("duplicate object key"), "{error}");

    // A duplicate nested deep inside a case is caught by the same layer.
    let nested = text.replacen(
        "\"actor_role\":\"learner\"",
        "\"actor_role\":\"learner\",\"actor_role\":\"opponent\"",
        1,
    );
    let nested_error = load_unsealed_artifact_v2(nested.as_bytes())
        .expect_err("a nested duplicate key is rejected");
    assert!(
        nested_error.contains("duplicate object key"),
        "{nested_error}"
    );
}

#[test]
fn v2_loader_rejects_a_float_token_before_typed_decoding() {
    let mutant = mutated_artifact_value_v2(
        "positive_cases/0/input/decisions/0/legal_action_count_u32",
        serde_json::json!(4.5),
    );
    let error = loader_error_of_value_v2(&mutant);
    assert!(error.contains("float literal"), "{error}");
}

// -------------------------------------------------------- typed shape negatives
//
// These documents all pass the layer B scanner, so the rejection here is serde's
// and the scanner cannot make them vacuous.

/// Every typed shape in the artifact, with one key to add and one to remove.
const TYPED_SHAPE_PROBES_V2: [(&str, &str, &str); 15] = [
    ("", "unexpected_field", "vector_stream_identity"),
    ("source_authorities", "unexpected_field", "trainer_schedule"),
    (
        "source_authorities/environment_randomization",
        "unexpected_field",
        "namespace",
    ),
    (
        "source_authorities/inner_trajectory",
        "unexpected_field",
        "identity",
    ),
    (
        "source_authorities/reset_trajectory",
        "unexpected_field",
        "goldens_schema",
    ),
    (
        "source_authorities/runtime_deck_catalog",
        "unexpected_field",
        "protocol",
    ),
    (
        "source_authorities/trainer_schedule",
        "unexpected_field",
        "seed_version",
    ),
    ("positive_cases/0", "unexpected_field", "v2_sha256"),
    ("positive_cases/0/input", "unexpected_field", "learner_seat"),
    (
        "positive_cases/0/input/decisions/0",
        "unexpected_field",
        "actor_role",
    ),
    (
        "positive_cases/0/input/terminal",
        "unexpected_field",
        "winner",
    ),
    (
        "pair_positive_cases/0",
        "unexpected_field",
        "odd_trajectory_sha256",
    ),
    (
        "pair_positive_cases/0/input",
        "unexpected_field",
        "odd_start",
    ),
    (
        "trajectory_reject_cases/0",
        "unexpected_field",
        "expected_code",
    ),
    ("pair_reject_cases/0", "unexpected_field", "expected_code"),
];

#[test]
fn v2_typed_shapes_reject_unknown_and_missing_fields_everywhere() {
    assert_eq!(TYPED_SHAPE_PROBES_V2.len(), 15);
    for (path, unknown_key, missing_key) in TYPED_SHAPE_PROBES_V2 {
        let added = inserted_key_artifact_value_v2(path, unknown_key, serde_json::json!("x"));
        let added_error = loader_error_of_value_v2(&added);
        assert!(
            added_error.contains("unknown field"),
            "{path:?} unknown key: {added_error}"
        );

        let removed = removed_key_artifact_value_v2(path, missing_key);
        let removed_error = loader_error_of_value_v2(&removed);
        assert!(
            removed_error.contains("missing field"),
            "{path:?} missing key: {removed_error}"
        );
    }
}

/// The V2 input owns its inner accumulator.  Every field that could supply an
/// inner digest, root, seat, or deck binding from outside is an unknown field.
#[test]
fn v2_typed_shapes_reject_forbidden_inner_override_names() {
    assert_eq!(FORBIDDEN_INNER_OVERRIDE_NAMES_V2.len(), 11);
    let unique: BTreeSet<&str> = FORBIDDEN_INNER_OVERRIDE_NAMES_V2.iter().copied().collect();
    assert_eq!(unique.len(), FORBIDDEN_INNER_OVERRIDE_NAMES_V2.len());
    for name in FORBIDDEN_INNER_OVERRIDE_NAMES_V2 {
        for path in [
            "positive_cases/0/input",
            "trajectory_reject_cases/0/input",
            "pair_positive_cases/0/input/even_start",
            "pair_positive_cases/0/input/odd_start",
        ] {
            let mutant = inserted_key_artifact_value_v2(path, name, serde_json::json!("00"));
            let error = loader_error_of_value_v2(&mutant);
            assert!(error.contains("unknown field"), "{path}/{name}: {error}");
        }
    }
}

#[test]
fn v2_typed_shapes_reject_null_boolean_negative_and_overflow_numbers() {
    for (path, replacement, label) in [
        ("schema", serde_json::Value::Null, "top-level null"),
        (
            "positive_cases/0/input/terminal/winner",
            serde_json::Value::Null,
            "nested null",
        ),
        (
            "positive_cases/0/input/decisions/0/legal_action_count_u32",
            serde_json::json!(true),
            "boolean in an integer field",
        ),
        (
            "positive_cases/0/input/decisions/0/selected_index_u32",
            serde_json::json!(-1),
            "negative integer",
        ),
        (
            "positive_cases/0/input/decisions/0/substep_count_u32",
            serde_json::json!(4_294_967_296_u64),
            "u32 overflow",
        ),
        (
            "positive_cases/0/input/decisions/0/substep_index_u32",
            serde_json::json!("0"),
            "string in an integer field",
        ),
        (
            "positive_cases/0/input/episode_index_u64_hex",
            serde_json::json!(0),
            "integer in a hex string field",
        ),
        (
            "positive_cases/0/input/decisions",
            serde_json::json!({}),
            "object in an array field",
        ),
    ] {
        let mutant = mutated_artifact_value_v2(path, replacement);
        let error = loader_error_of_value_v2(&mutant);
        assert!(
            error.contains("typed decode failed"),
            "{label} at {path}: {error}"
        );
    }
}

// ------------------------------------------------------- layer D: semantic negatives

/// Every closed string domain the artifact declares, driven one at a time.  These
/// values are well-typed, so only layer D can refuse them.
#[test]
fn v2_semantics_reject_unknown_enum_values() {
    let accepted = load_sealed_artifact_v2();

    let mut seat = accepted.clone();
    seat.positive_cases[0].input.learner_seat = "p2".to_string();
    assert!(validate_artifact_semantics_v2(&seat)
        .unwrap_err()
        .contains("learner seat is outside its domain"));

    let mut actor_seat = accepted.clone();
    actor_seat.positive_cases[0].input.decisions[0].actor_seat = "p9".to_string();
    assert!(validate_artifact_semantics_v2(&actor_seat)
        .unwrap_err()
        .contains("actor seat is outside its domain"));

    let mut actor_role = accepted.clone();
    actor_role.positive_cases[0].input.decisions[0].actor_role = "wizard".to_string();
    assert!(validate_artifact_semantics_v2(&actor_role)
        .unwrap_err()
        .contains("actor role is outside its domain"));

    let mut outcome = accepted.clone();
    outcome.positive_cases[0].input.terminal.outcome = "p2-win".to_string();
    assert!(validate_artifact_semantics_v2(&outcome)
        .unwrap_err()
        .contains("terminal outcome is outside its domain"));

    let mut winner = accepted.clone();
    winner.positive_cases[0].input.terminal.winner = "p2".to_string();
    assert!(validate_artifact_semantics_v2(&winner)
        .unwrap_err()
        .contains("terminal winner is outside its domain"));

    let mut classification = accepted.clone();
    classification.positive_cases[0]
        .input
        .terminal
        .classification = "partial".to_string();
    assert!(validate_artifact_semantics_v2(&classification)
        .unwrap_err()
        .contains("terminal classification is outside its domain"));

    let mut terminal_code = accepted.clone();
    terminal_code.positive_cases[0].input.terminal.terminal_code = "fail-open".to_string();
    assert!(validate_artifact_semantics_v2(&terminal_code)
        .unwrap_err()
        .contains("terminal code is outside its domain"));

    let mut expected_code = accepted.clone();
    expected_code.trajectory_reject_cases[0].expected_code = "not-a-code".to_string();
    assert!(validate_artifact_semantics_v2(&expected_code)
        .unwrap_err()
        .contains("closed vocabulary"));

    let mut pair_code = accepted;
    pair_code.pair_reject_cases[0].expected_code = "also-not-a-code".to_string();
    assert!(validate_artifact_semantics_v2(&pair_code)
        .unwrap_err()
        .contains("closed vocabulary"));
}

#[test]
fn v2_semantics_reject_name_count_hex_and_digest_drift() {
    let accepted = load_sealed_artifact_v2();
    assert!(validate_artifact_semantics_v2(&accepted).is_ok());

    // Name domain.
    let mut bad_name = accepted.clone();
    bad_name.positive_cases[0].name = "Episode-0".to_string();
    assert!(validate_artifact_semantics_v2(&bad_name)
        .unwrap_err()
        .contains("name domain"));

    // Strict ascending order.
    let mut unsorted = accepted.clone();
    unsorted.positive_cases.swap(0, 1);
    assert!(validate_artifact_semantics_v2(&unsorted)
        .unwrap_err()
        .contains("strictly ascending"));

    // Empty list.
    let mut empty = accepted.clone();
    empty.pair_positive_cases.clear();
    assert!(validate_artifact_semantics_v2(&empty)
        .unwrap_err()
        .contains("case list is empty"));

    // Count pin.
    let mut short = accepted.clone();
    short.positive_cases.pop();
    assert!(validate_artifact_semantics_v2(&short)
        .unwrap_err()
        .contains("case count drift"));

    // Fixed-width hex.
    let mut bad_hex = accepted.clone();
    bad_hex.positive_cases[0].input.episode_index_u64_hex = "0".to_string();
    assert!(validate_artifact_semantics_v2(&bad_hex)
        .unwrap_err()
        .contains("sixteen lowercase hex"));

    // Uppercase hex is not lowercase hex.
    let mut upper_hex = accepted.clone();
    upper_hex.positive_cases[0].input.deck_p0_hash_u64_hex = "5FDB7B92986B6FC1".to_string();
    assert!(validate_artifact_semantics_v2(&upper_hex)
        .unwrap_err()
        .contains("sixteen lowercase hex"));

    // Digest shape.
    let mut bad_digest = accepted.clone();
    bad_digest.positive_cases[0].v2_sha256 = "abc".to_string();
    assert!(validate_artifact_semantics_v2(&bad_digest)
        .unwrap_err()
        .contains("lowercase raw32 hex"));

    let mut bad_pair_digest = accepted.clone();
    bad_pair_digest.pair_positive_cases[0].odd_trajectory_sha256 = "abc".to_string();
    assert!(validate_artifact_semantics_v2(&bad_pair_digest)
        .unwrap_err()
        .contains("lowercase raw32 hex"));

    // Stream shape.
    let mut bad_stream = accepted.clone();
    bad_stream.positive_cases[0].inner_stream_hex = String::new();
    assert!(validate_artifact_semantics_v2(&bad_stream)
        .unwrap_err()
        .contains("even lowercase hex"));

    // Identity drift, all four.
    let mut schema = accepted.clone();
    schema.schema = ACCEPTED_INNER_GOLDENS_SCHEMA_V2.to_string();
    assert!(validate_artifact_semantics_v2(&schema)
        .unwrap_err()
        .contains("schema drift"));

    let mut generator = accepted.clone();
    generator.generator_identity = ACCEPTED_INNER_GOLDENS_GENERATOR_IDENTITY_V2.to_string();
    assert!(validate_artifact_semantics_v2(&generator)
        .unwrap_err()
        .contains("generator identity drift"));

    let mut trajectory = accepted.clone();
    trajectory.trajectory_identity = ACCEPTED_INNER_IDENTITY_V2.to_string();
    assert!(validate_artifact_semantics_v2(&trajectory)
        .unwrap_err()
        .contains("trajectory identity drift"));

    let mut stream_identity = accepted;
    stream_identity.vector_stream_identity = ACCEPTED_INNER_GOLDEN_STREAM_IDENTITY_V2.to_string();
    assert!(validate_artifact_semantics_v2(&stream_identity)
        .unwrap_err()
        .contains("vector stream identity drift"));
}

/// The case cap is independent of the exact sealed counts and is checked first,
/// so both a cap-sized and a cap-plus-one artifact are reachable: at the cap the
/// failure is the count pin, one past it the failure is the cap.
#[test]
fn v2_semantics_case_cap_is_reachable_before_the_exact_count_pin() {
    let accepted = load_sealed_artifact_v2();
    let template = accepted.positive_cases[0].clone();
    let widened = |count: usize| {
        let mut artifact = accepted.clone();
        artifact.positive_cases = (0..count)
            .map(|index| {
                let mut case = template.clone();
                case.name = format!("case-{index:04}");
                case
            })
            .collect();
        artifact
    };

    let at_cap = widened(MAX_GOLDEN_CASES_V2);
    let at_cap_error = validate_artifact_semantics_v2(&at_cap)
        .expect_err("a cap-sized case list still fails the exact count pin");
    assert!(!at_cap_error.contains("case cap"), "{at_cap_error}");
    assert!(at_cap_error.contains("case count drift"), "{at_cap_error}");

    let over_cap = widened(MAX_GOLDEN_CASES_V2 + 1);
    let over_cap_error = validate_artifact_semantics_v2(&over_cap)
        .expect_err("a cap-plus-one case list is refused by the cap");
    assert!(over_cap_error.contains("case cap"), "{over_cap_error}");
}

/// The decision cap applies to every family and to both pair sides, and is
/// likewise reachable: exactly at the cap the artifact is still semantically
/// valid, one past it the cap fires.
#[test]
fn v2_semantics_decision_cap_is_reachable_in_every_family_and_both_pair_sides() {
    let accepted = load_sealed_artifact_v2();
    let row = accepted.positive_cases[0].input.decisions[0].clone();

    let resize = |artifact: &mut GoldenArtifactV2, family: usize, count: usize| match family {
        0 => artifact.positive_cases[0]
            .input
            .decisions
            .resize(count, row.clone()),
        1 => artifact.trajectory_reject_cases[0]
            .input
            .decisions
            .resize(count, row.clone()),
        2 => artifact.pair_positive_cases[0]
            .input
            .even_start
            .decisions
            .resize(count, row.clone()),
        3 => artifact.pair_positive_cases[0]
            .input
            .odd_start
            .decisions
            .resize(count, row.clone()),
        4 => artifact.pair_reject_cases[0]
            .input
            .even_start
            .decisions
            .resize(count, row.clone()),
        _ => artifact.pair_reject_cases[0]
            .input
            .odd_start
            .decisions
            .resize(count, row.clone()),
    };

    for family in 0..6 {
        let mut at_cap = accepted.clone();
        resize(&mut at_cap, family, MAX_GOLDEN_DECISIONS_V2);
        assert!(
            validate_artifact_semantics_v2(&at_cap).is_ok(),
            "family {family}: a cap-sized decision stream is still semantically valid"
        );

        let mut over_cap = accepted.clone();
        resize(&mut over_cap, family, MAX_GOLDEN_DECISIONS_V2 + 1);
        let error = validate_artifact_semantics_v2(&over_cap)
            .expect_err("a cap-plus-one decision stream is refused");
        assert!(
            error.contains("decision cap exceeded"),
            "family {family}: {error}"
        );
    }
}

/// The cap stage is global and first, so a name or ordering defect in an *early*
/// family can never hide a cap violation in a *later* one.  Each compound below
/// is paired with its counterfactual, so the test cannot pass merely because the
/// name or ordering defect was absent.
#[test]
fn v2_semantics_cap_stage_precedes_every_case_name_and_order_rule() {
    let accepted = load_sealed_artifact_v2();
    let row = accepted.positive_cases[0].input.decisions[0].clone();
    let pair_reject_template = accepted.pair_reject_cases[0].clone();
    let trajectory_reject_template = accepted.trajectory_reject_cases[0].clone();

    // The two early-family defects, each proven to fire on its own.
    let with_bad_name = |artifact: &mut GoldenArtifactV2| {
        artifact.positive_cases[0].name = "Episode-0".to_string();
    };
    let with_bad_order = |artifact: &mut GoldenArtifactV2| {
        artifact.positive_cases.swap(0, 1);
    };
    let mut name_only = accepted.clone();
    with_bad_name(&mut name_only);
    assert!(validate_artifact_semantics_v2(&name_only)
        .expect_err("a bad name is refused on its own")
        .contains("name domain"));
    let mut order_only = accepted.clone();
    with_bad_order(&mut order_only);
    assert!(validate_artifact_semantics_v2(&order_only)
        .expect_err("a bad order is refused on its own")
        .contains("strictly ascending"));

    // A later-family *list* cap outranks both.
    for (label, apply) in [
        ("bad name", &with_bad_name as &dyn Fn(&mut GoldenArtifactV2)),
        ("bad order", &with_bad_order),
    ] {
        let mut over_case_cap = accepted.clone();
        apply(&mut over_case_cap);
        over_case_cap.pair_reject_cases = (0..=MAX_GOLDEN_CASES_V2)
            .map(|index| {
                let mut case = pair_reject_template.clone();
                case.name = format!("case-{index:04}");
                case
            })
            .collect();
        let error = validate_artifact_semantics_v2(&over_case_cap)
            .expect_err("the compound artifact is refused");
        assert!(
            error.contains("pair_reject_cases: case list exceeds the case cap"),
            "{label} must not pre-empt a later-family list cap: {error}"
        );

        // The same, with the cap in the trajectory reject family instead.
        let mut over_trajectory_cap = accepted.clone();
        apply(&mut over_trajectory_cap);
        over_trajectory_cap.trajectory_reject_cases = (0..=MAX_GOLDEN_CASES_V2)
            .map(|index| {
                let mut case = trajectory_reject_template.clone();
                case.name = format!("case-{index:04}");
                case
            })
            .collect();
        let error = validate_artifact_semantics_v2(&over_trajectory_cap)
            .expect_err("the compound artifact is refused");
        assert!(
            error.contains("trajectory_reject_cases: case list exceeds the case cap"),
            "{label} must not pre-empt a later-family list cap: {error}"
        );

        // A later-family *decision* cap outranks both as well, on either pair
        // side and in the trajectory reject family.
        for family in ["pair_reject even", "pair_reject odd", "trajectory_reject"] {
            let mut over_decision_cap = accepted.clone();
            apply(&mut over_decision_cap);
            match family {
                "pair_reject even" => over_decision_cap.pair_reject_cases[0]
                    .input
                    .even_start
                    .decisions
                    .resize(MAX_GOLDEN_DECISIONS_V2 + 1, row.clone()),
                "pair_reject odd" => over_decision_cap.pair_reject_cases[0]
                    .input
                    .odd_start
                    .decisions
                    .resize(MAX_GOLDEN_DECISIONS_V2 + 1, row.clone()),
                _ => over_decision_cap.trajectory_reject_cases[0]
                    .input
                    .decisions
                    .resize(MAX_GOLDEN_DECISIONS_V2 + 1, row.clone()),
            }
            let error = validate_artifact_semantics_v2(&over_decision_cap)
                .expect_err("the compound artifact is refused");
            assert!(
                error.contains("decision cap exceeded"),
                "{label} must not pre-empt the {family} decision cap: {error}"
            );
        }
    }
}

/// Layer D validates the artifact's top-level authority block against B2's own
/// twenty-six literals.  Each field is drifted alone, through the unsealed
/// loader, so the whole four-layer path is exercised.
#[test]
fn v2_semantics_reject_top_level_authority_drift_one_field_at_a_time() {
    let accepted = load_sealed_artifact_v2();
    let paths: [&str; DECLARED_AUTHORITY_FIELD_COUNT_V2] = [
        "inner_trajectory/identity",
        "inner_trajectory/goldens_schema",
        "inner_trajectory/goldens_generator_identity",
        "inner_trajectory/goldens_raw_file_sha256",
        "inner_trajectory/golden_semantic_stream_identity",
        "inner_trajectory/golden_semantic_stream_sha256",
        "environment_randomization/identity",
        "environment_randomization/namespace",
        "environment_randomization/kdf_goldens_schema",
        "environment_randomization/kdf_goldens_raw_file_sha256",
        "environment_randomization/python_reference_raw_file_sha256",
        "reset_trajectory/goldens_schema",
        "reset_trajectory/generator_identity",
        "reset_trajectory/physical_projection_identity",
        "reset_trajectory/portable_semantic_stream_identity",
        "reset_trajectory/goldens_raw_file_sha256",
        "reset_trajectory/portable_semantic_stream_sha256",
        "trainer_schedule/identity",
        "trainer_schedule/seed_version",
        "trainer_schedule/goldens_schema",
        "trainer_schedule/goldens_raw_file_sha256",
        "runtime_deck_catalog/schema",
        "runtime_deck_catalog/protocol",
        "runtime_deck_catalog/materialization_protocol",
        "runtime_deck_catalog/deck_hash_algorithm",
        "runtime_deck_catalog/catalog_raw_file_sha256",
    ];
    assert_eq!(
        paths.len(),
        declared_authority_pairs_v2(&accepted.source_authorities).len()
    );

    for path in paths {
        let mutant = mutated_artifact_value_v2(
            &format!("source_authorities/{path}"),
            serde_json::json!("drifted-authority-value"),
        );
        let error = loader_error_of_value_v2(&mutant);
        assert!(
            error.contains("declared authority drift at"),
            "{path}: {error}"
        );
    }
}

// --------------------------------------------------------- authority negatives

/// The same twenty-six paths, drifted inside a *case's own* authority block,
/// must reach the production-owned `AuthorityMismatch` rather than a fixture
/// error: the top-level block is untouched, so layer D admits the artifact and
/// the runner is what refuses the case.
#[test]
fn v2_every_declared_authority_field_drift_is_authority_mismatch() {
    let accepted = load_sealed_artifact_v2();
    let paths: [&str; DECLARED_AUTHORITY_FIELD_COUNT_V2] = [
        "inner_trajectory/identity",
        "inner_trajectory/goldens_schema",
        "inner_trajectory/goldens_generator_identity",
        "inner_trajectory/goldens_raw_file_sha256",
        "inner_trajectory/golden_semantic_stream_identity",
        "inner_trajectory/golden_semantic_stream_sha256",
        "environment_randomization/identity",
        "environment_randomization/namespace",
        "environment_randomization/kdf_goldens_schema",
        "environment_randomization/kdf_goldens_raw_file_sha256",
        "environment_randomization/python_reference_raw_file_sha256",
        "reset_trajectory/goldens_schema",
        "reset_trajectory/generator_identity",
        "reset_trajectory/physical_projection_identity",
        "reset_trajectory/portable_semantic_stream_identity",
        "reset_trajectory/goldens_raw_file_sha256",
        "reset_trajectory/portable_semantic_stream_sha256",
        "trainer_schedule/identity",
        "trainer_schedule/seed_version",
        "trainer_schedule/goldens_schema",
        "trainer_schedule/goldens_raw_file_sha256",
        "runtime_deck_catalog/schema",
        "runtime_deck_catalog/protocol",
        "runtime_deck_catalog/materialization_protocol",
        "runtime_deck_catalog/deck_hash_algorithm",
        "runtime_deck_catalog/catalog_raw_file_sha256",
    ];
    assert_eq!(
        paths.len(),
        declared_authority_pairs_v2(&accepted.source_authorities).len()
    );

    for path in paths {
        let mutant = mutated_artifact_value_v2(
            &format!("positive_cases/0/input/source_authorities/{path}"),
            serde_json::json!("drifted-authority-value"),
        );
        let artifact = decode_mutant_v2(&mutant);
        let error = run_trajectory_case_v2(&artifact.positive_cases[0].input)
            .err()
            .unwrap_or_else(|| panic!("{path}: drifted authority was admitted"));
        assert_eq!(
            error,
            GoldenRunErrorV2::Contract(NativeFullEpisodeTrajectoryErrorV2::AuthorityMismatch),
            "{path}"
        );
        assert_eq!(portable_run_code_v2(&error), "authority-mismatch", "{path}");
    }
}

#[test]
fn v2_authority_drift_precedes_every_start_rule() {
    // A case that is simultaneously authority-drifted and start-invalid must
    // report authority drift: the declaration is checked before any start value
    // is parsed.
    let mut mutant = artifact_value_v2();
    *value_at_mut_v2(
        &mut mutant,
        "positive_cases/0/input/source_authorities/environment_randomization/namespace",
    )
    .expect("path exists") = serde_json::json!("drifted");
    *value_at_mut_v2(&mut mutant, "positive_cases/0/input/episode_index_u64_hex")
        .expect("path exists") = serde_json::json!("8000000000000000");
    let artifact = decode_mutant_v2(&mutant);
    let error = run_trajectory_case_v2(&artifact.positive_cases[0].input)
        .expect_err("a drifted authority is rejected");
    assert_eq!(portable_run_code_v2(&error), "authority-mismatch");
}

/// Precedence across the whole seam: a hidden structural or cap defect outranks
/// an authority mismatch and outranks the pair schedule-integer bounds, and the
/// pair bounds in turn outrank a component episode's own rejection.
#[test]
fn v2_hidden_structural_and_cap_defects_precede_authority_and_pair_overflow() {
    let accepted = load_sealed_artifact_v2();
    let row = accepted.positive_cases[0].input.decisions[0].clone();

    // A cap defect plus a top-level authority drift: the cap wins.
    let mut cap_and_authority = accepted.clone();
    cap_and_authority
        .source_authorities
        .environment_randomization
        .namespace = "drifted".to_string();
    cap_and_authority.positive_cases[0]
        .input
        .decisions
        .resize(MAX_GOLDEN_DECISIONS_V2 + 1, row.clone());
    let error = validate_artifact_semantics_v2(&cap_and_authority)
        .expect_err("the compound artifact is refused");
    assert!(error.contains("decision cap exceeded"), "{error}");

    // A case-cap defect plus the same authority drift: the cap wins again.
    let mut case_cap_and_authority = accepted.clone();
    case_cap_and_authority
        .source_authorities
        .trainer_schedule
        .seed_version = "drifted".to_string();
    let template = accepted.pair_reject_cases[0].clone();
    case_cap_and_authority.pair_reject_cases = (0..=MAX_GOLDEN_CASES_V2)
        .map(|index| {
            let mut case = template.clone();
            case.name = format!("case-{index:04}");
            case
        })
        .collect();
    let error = validate_artifact_semantics_v2(&case_cap_and_authority)
        .expect_err("the compound artifact is refused");
    assert!(error.contains("case cap"), "{error}");

    // A cap defect on a pair side whose base seed is also out of the u63 domain:
    // the cap is a load-time defect and is decided before any pair is run.
    let mut cap_and_base_seed_overflow = accepted.clone();
    cap_and_base_seed_overflow.pair_reject_cases[0]
        .input
        .base_seed_u64_hex = u64_hex_v2(1_u64 << 63);
    cap_and_base_seed_overflow.pair_reject_cases[0]
        .input
        .even_start
        .decisions
        .resize(MAX_GOLDEN_DECISIONS_V2 + 1, row.clone());
    let error = validate_artifact_semantics_v2(&cap_and_base_seed_overflow)
        .expect_err("the compound pair artifact is refused");
    assert!(error.contains("decision cap exceeded"), "{error}");
    // The overflow really is present, so the cap is what outranked it.
    assert_eq!(
        cap_and_base_seed_overflow.pair_reject_cases[0]
            .input
            .base_seed_u64_hex,
        u64_hex_v2(1_u64 << 63)
    );

    // The same compound against the *other* pair bound: a decision cap plus a
    // pair index at two-to-the-sixty-two.  The cap wins here too, so neither
    // schedule-integer bound can hide a cap defect at load time.
    let mut cap_and_pair_index_overflow = accepted.clone();
    cap_and_pair_index_overflow.pair_reject_cases[1]
        .input
        .pair_index_u64_hex = u64_hex_v2(1_u64 << 62);
    cap_and_pair_index_overflow.pair_reject_cases[1]
        .input
        .odd_start
        .decisions
        .resize(MAX_GOLDEN_DECISIONS_V2 + 1, row);
    let error = validate_artifact_semantics_v2(&cap_and_pair_index_overflow)
        .expect_err("the compound pair-index artifact is refused");
    assert!(error.contains("decision cap exceeded"), "{error}");
    assert_eq!(
        cap_and_pair_index_overflow.pair_reject_cases[1]
            .input
            .pair_index_u64_hex,
        u64_hex_v2(1_u64 << 62)
    );

    // Inside the pair runner, the schedule-integer bounds precede the component
    // episodes, so an overflowing base seed outranks a component's own drift.
    let mut overflow_and_component = rebuilt_p0_pair_input_v2();
    overflow_and_component.base_seed_u64_hex = u64_hex_v2(1_u64 << 63);
    overflow_and_component
        .odd_start
        .source_authorities
        .environment_randomization
        .namespace = "drifted".to_string();
    let error =
        run_pair_case_v2(&overflow_and_component).expect_err("the compound pair is rejected");
    assert_eq!(portable_run_code_v2(&error), "schedule-integer-outside-u63");

    // The pair-index bound behaves the same way inside the runner.
    let mut pair_index_and_component = rebuilt_p0_pair_input_v2();
    pair_index_and_component.pair_index_u64_hex = u64_hex_v2(1_u64 << 62);
    pair_index_and_component
        .odd_start
        .source_authorities
        .environment_randomization
        .namespace = "drifted".to_string();
    let error = run_pair_case_v2(&pair_index_and_component)
        .expect_err("the compound pair-index pair is rejected");
    assert_eq!(
        portable_run_code_v2(&error),
        "pair-index-outside-episode-domain"
    );

    // With the base seed restored, the component's own code is what surfaces.
    let mut component_only = rebuilt_p0_pair_input_v2();
    component_only
        .odd_start
        .source_authorities
        .environment_randomization
        .namespace = "drifted".to_string();
    let error = run_pair_case_v2(&component_only).expect_err("the drifted component is rejected");
    assert_eq!(portable_run_code_v2(&error), "authority-mismatch");
}

// ------------------------------------------------------- focused contract probes

#[test]
fn v2_checked_commitment_boundary_rejects_malformed_hex() {
    for malformed in [
        "",
        "00",
        "0000000000000000000000000000000",
        "000000000000000000000000000000000",
        "00000000000000000000000000C00000",
        "0000000000000000000000000000000g",
    ] {
        assert_eq!(
            checked_flat_action_v2_commitment_v2(malformed),
            Err(NativeFullEpisodeTrajectoryErrorV2::MalformedCommitment),
            "{malformed:?}"
        );
    }
    assert_eq!(
        checked_flat_action_v2_commitment_v2("00000000000000000000000000c00000"),
        Ok([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0,
            0x00, 0x00
        ])
    );
}

#[test]
fn v2_pair_validator_reports_the_even_side_before_the_odd_side() {
    let pair = rebuilt_p0_pair_input_v2();
    let base_seed = parse_u64_hex_v2(&pair.base_seed_u64_hex).unwrap();
    let pair_index = parse_u64_hex_v2(&pair.pair_index_u64_hex).unwrap();

    let mut even = start_from_input_v2(&pair.even_start).unwrap();
    let mut odd = start_from_input_v2(&pair.odd_start).unwrap();

    // Both sides invalid, in different ways: the even side's code must surface.
    even.episode_index = 1_u64 << 63;
    odd.deck_ids[0] = "not-in-catalog".to_string();
    assert_eq!(
        validate_native_full_episode_trajectory_pair_v2(base_seed, pair_index, &even, &odd),
        Err(NativeFullEpisodeTrajectoryErrorV2::EpisodeIndexOutsideU63)
    );

    // With the even side restored, the odd side's own code surfaces.
    let even = start_from_input_v2(&pair.even_start).unwrap();
    assert_eq!(
        validate_native_full_episode_trajectory_pair_v2(base_seed, pair_index, &even, &odd),
        Err(NativeFullEpisodeTrajectoryErrorV2::InvalidDeckId)
    );

    // The schedule-integer bounds precede both starts.
    assert_eq!(
        validate_native_full_episode_trajectory_pair_v2(
            ACCEPTED_U63_MAX_V2 + 1,
            pair_index,
            &even,
            &odd
        ),
        Err(NativeFullEpisodeTrajectoryErrorV2::ScheduleIntegerOutsideU63)
    );
    assert_eq!(
        validate_native_full_episode_trajectory_pair_v2(
            base_seed,
            ACCEPTED_U62_MAX_V2 + 1,
            &even,
            &odd
        ),
        Err(NativeFullEpisodeTrajectoryErrorV2::PairIndexOutsideEpisodeDomain)
    );
}

#[test]
fn v2_start_validation_precedence_is_exact() {
    let accepted = start_from_input_v2(&rebuilt_t0_input_v2()).unwrap();
    assert!(validate_start_v2(&accepted).is_ok());

    // The u63 episode domain precedes every deck rule.
    let mut episode = accepted.clone();
    episode.episode_index = 1_u64 << 63;
    episode.deck_ids[0] = String::new();
    assert_eq!(
        validate_start_v2(&episode),
        Err(NativeFullEpisodeTrajectoryErrorV2::EpisodeIndexOutsideU63)
    );

    // Deck-ID shape precedes catalog resolution, and both precede the seat rule.
    let mut shape = accepted.clone();
    shape.deck_ids[1] = String::new();
    shape.learner_seat = PlayerSeatV1::P1;
    assert_eq!(
        validate_start_v2(&shape),
        Err(NativeFullEpisodeTrajectoryErrorV2::InvalidDeckId)
    );

    // A resolvable deck with the wrong frozen hash is a hash mismatch, not an
    // unknown deck.
    let mut hash = accepted.clone();
    hash.deck_hashes[1] ^= 1;
    assert_eq!(
        validate_start_v2(&hash),
        Err(NativeFullEpisodeTrajectoryErrorV2::RuntimeDeckHashMismatch)
    );

    // Seat parity is last.
    let mut seat = accepted;
    seat.learner_seat = PlayerSeatV1::P1;
    assert_eq!(
        validate_start_v2(&seat),
        Err(NativeFullEpisodeTrajectoryErrorV2::LearnerSeatRuleMismatch)
    );
}
