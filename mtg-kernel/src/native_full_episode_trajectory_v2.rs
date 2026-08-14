//! Evidence-only versioned full-episode trajectory audit contract V2.
//!
//! V2 does not re-serialize a single V1 decision or terminal byte.  It owns a
//! `NativeFullEpisodeTrajectoryAccumulatorV1`, constructs that inner
//! accumulator from exactly the same start values it was given, delegates
//! preflight, accepted-row recording, and natural finish to it, and then
//! envelopes only the raw32 digest the inner accumulator returned.  There is
//! deliberately no API that accepts an externally supplied V1 receipt, an
//! externally supplied inner digest, or alternate inner episode / root / seat /
//! deck metadata: inner and outer provenance laundering is unrepresentable
//! rather than detected after the fact.  There is likewise no API that accepts
//! a declared or injected authority block.  The twenty-four owner-backed
//! authorities the envelope commits to are private frozen literals, and every
//! constructing entry point first runs one private live guard that compares the
//! current imported owner constants against those literals, so authority drift
//! is a fail-closed rejection rather than a silently re-frozen envelope.
//!
//! The byte contract is frozen as
//! `mtg-kernel-native-full-episode-trajectory-sha256-v2`: exactly thirty-four
//! atoms in the frozen order, using the same framing as V1
//! (`u32be(tag_len) || tag || u64be(payload_len) || payload`).  Every
//! `*_sha256_raw32` payload is a decoded 32-byte pin, never ASCII hex, and
//! every integer payload is fixed-width big-endian.  The envelope carries no
//! hash of the V2 artifact or of the V2 semantic stream, because either would
//! commit to a digest computed over itself.
//!
//! Production semantics enforced here: `episode_index` is checked u63; the
//! `pair_environment_seed` is the full unsigned 64-bit pair root and is never
//! masked; an even episode means a P0 learner and an odd episode means a P1
//! learner; both physical deck IDs must resolve exactly in the runtime catalog
//! and each supplied hash must equal that deck's frozen runtime hash; and
//! learner-seat parity never swaps the physical P0/P1 deck bindings.  Every V1
//! decision, group, width, selected-index, terminal, provenance, and count
//! rejection stays fail-closed through the owned inner contract and is mapped
//! into the closed V2 vocabulary.
//!
//! V1 bytes, APIs, goldens, and V1 consumers are unchanged by this module.
//! Since C2, this contract is live: a validated run classified as
//! environment randomization V2 executes through the V2 accumulator, the
//! run-bound receipt, and the consumed window-preflight authority defined
//! here, while every legacy V1 run keeps the frozen V1 inner behavior
//! exactly.

use crate::async_rollout::AsyncRolloutTerminalV1;
use crate::environment_randomization_v2::{
    ENVIRONMENT_RANDOMIZATION_GOLDENS_SCHEMA_V1, ENVIRONMENT_RANDOMIZATION_GOLDENS_SHA256_V1,
    ENVIRONMENT_RANDOMIZATION_IDENTITY_V2, ENVIRONMENT_RANDOMIZATION_NAMESPACE_V2,
    ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GENERATOR_IDENTITY_V1,
    ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_SCHEMA_V1,
    ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_SHA256_V1,
    ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PHYSICAL_PROJECTION_IDENTITY_V1,
    ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PORTABLE_VECTOR_STREAM_IDENTITY_V1,
    ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PORTABLE_VECTOR_STREAM_SHA256_V1,
};
use crate::native_full_episode_trajectory_v1::{
    NativeFullEpisodeTrajectoryAccumulatorV1, NativeFullEpisodeTrajectoryDecisionRowV1,
    NativeFullEpisodeTrajectoryErrorV1, NativeFullEpisodeTrajectoryReceiptV1,
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_FILE_SHA256_V1,
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V1,
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_SCHEMA_V1,
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V1,
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_SHA256_V1,
    NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V1,
};
use crate::native_trainer_schedule_v1::{
    native_trainer_episode_schedule_v1, NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1,
    NATIVE_TRAINER_SCHEDULE_VERSION_V1, PYTHON_REFERENCE_SEED_VERSION_V1,
};
use crate::rl::PlayerSeatV1;
use crate::rl_session::{SessionDeckHashesV1, SessionDeckIdsV1};
use crate::runtime_decks::{
    runtime_deck_by_id, RUNTIME_DECK_CATALOG_FILE_SHA256, RUNTIME_DECK_CATALOG_SCHEMA,
    RUNTIME_DECK_HASH_ALGORITHM, RUNTIME_DECK_MATERIALIZATION_PROTOCOL, RUNTIME_DECK_PROTOCOL,
};
use sha2::{Digest, Sha256};

// ------------------------------------------------------------- V2 identities

pub(crate) const NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V2: &str =
    "mtg-kernel-native-full-episode-trajectory-sha256-v2";
pub(crate) const NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_SCHEMA_V2: &str =
    "mtg_kernel_native_full_episode_trajectory_v2_goldens/v1";
pub(crate) const NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V2: &str =
    "mtg-kernel-native-full-episode-trajectory-v2-goldens-stdlib-python-v1";
pub(crate) const NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V2: &str =
    "mtg-kernel-native-full-episode-trajectory-v2-golden-vector-stream-sha256-v1";

/// Owner seals for the V2 goldens artifact and the V2 portable golden vector
/// stream, in the same shape V1 publishes its own.  These are downstream
/// contract seals for Phase B2, the native store, and the manifest; they are
/// deliberately absent from the thirty-four-atom envelope, from the twenty-four
/// owner-backed authority guard, and from every authority value this module
/// hashes, because a V2 envelope that committed to a hash of the V2 artifact or
/// of the V2 stream would commit to a digest computed over itself.
pub(crate) const NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_FILE_SHA256_V2: &str =
    "1a79f8153fe8adb7d984609d5e510f8e9f2f9e37358ed42744873ae4fd743672";
pub(crate) const NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_SHA256_V2: &str =
    "554003d75532a26d50ff6599cd909223176926773dce21efcd9895e41e51cb8e";

/// The exact number of atoms in the frozen V2 envelope.  `finish_v2` proves
/// this count on every constructed envelope.
pub(crate) const NATIVE_FULL_EPISODE_TRAJECTORY_ENVELOPE_ATOM_COUNT_V2: usize = 34;

// ------------------------------------------------------------------- domains

const U62_MAX_V2: u64 = (1_u64 << 62) - 1;
const U63_MAX_V2: u64 = (1_u64 << 63) - 1;

/// Mirrors the frozen but module-private V1 deck-ID predicate.  V1 is not
/// modified to expose it, so the identical bound is restated here and both
/// layers reject the same inputs with the same code.
const MAX_DECK_ID_BYTES_V2: usize = 64;

// ------------------------------------------------------- closed V2 vocabulary

/// The closed, portable V2 rejection vocabulary.  Twelve variants are the exact
/// image of the twelve frozen V1 failures; the remaining ten are V2's own start,
/// authority, wire-shape, and pair failures.  Nothing else may be added without
/// a new trajectory version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeFullEpisodeTrajectoryErrorV2 {
    AuthorityMismatch,
    EpisodeIndexOutsideU63,
    LearnerSeatRuleMismatch,
    InvalidDeckId,
    RuntimeDeckHashMismatch,
    EmptyDecisionStream,
    EpisodeMismatch,
    RowOrdinalMismatch,
    ActorRoleMismatch,
    MalformedPhysicalGroup,
    InvalidLegalActionCount,
    SelectedIndexOutOfRange,
    MalformedCommitment,
    CounterOverflow,
    NonNaturalTerminal,
    TerminalProvenanceMismatch,
    TerminalCountMismatch,
    ScheduleIntegerOutsideU63,
    PairIndexOutsideEpisodeDomain,
    PairEpisodeIndexMismatch,
    PairEnvironmentSeedMismatch,
    PairPhysicalDeckBindingMismatch,
}

impl NativeFullEpisodeTrajectoryErrorV2 {
    /// The exact portable code string carried by the V2 golden artifact.
    pub(crate) const fn portable_code_v2(self) -> &'static str {
        match self {
            Self::AuthorityMismatch => "authority-mismatch",
            Self::EpisodeIndexOutsideU63 => "episode-index-outside-u63",
            Self::LearnerSeatRuleMismatch => "learner-seat-rule-mismatch",
            Self::InvalidDeckId => "invalid-deck-id",
            Self::RuntimeDeckHashMismatch => "runtime-deck-hash-mismatch",
            Self::EmptyDecisionStream => "empty-decision-stream",
            Self::EpisodeMismatch => "episode-mismatch",
            Self::RowOrdinalMismatch => "row-ordinal-mismatch",
            Self::ActorRoleMismatch => "actor-role-mismatch",
            Self::MalformedPhysicalGroup => "malformed-physical-group",
            Self::InvalidLegalActionCount => "invalid-legal-action-count",
            Self::SelectedIndexOutOfRange => "selected-index-out-of-range",
            Self::MalformedCommitment => "malformed-commitment",
            Self::CounterOverflow => "counter-overflow",
            Self::NonNaturalTerminal => "non-natural-terminal",
            Self::TerminalProvenanceMismatch => "terminal-provenance-mismatch",
            Self::TerminalCountMismatch => "terminal-count-mismatch",
            Self::ScheduleIntegerOutsideU63 => "schedule-integer-outside-u63",
            Self::PairIndexOutsideEpisodeDomain => "pair-index-outside-episode-domain",
            Self::PairEpisodeIndexMismatch => "pair-episode-index-mismatch",
            Self::PairEnvironmentSeedMismatch => "pair-environment-seed-mismatch",
            Self::PairPhysicalDeckBindingMismatch => "pair-physical-deck-binding-mismatch",
        }
    }
}

/// Total, exhaustive image of the frozen V1 vocabulary in the V2 vocabulary.
/// Adding a V1 variant fails this match at compile time.
const fn map_inner_error_v2(
    error: NativeFullEpisodeTrajectoryErrorV1,
) -> NativeFullEpisodeTrajectoryErrorV2 {
    match error {
        NativeFullEpisodeTrajectoryErrorV1::InvalidDeckId => {
            NativeFullEpisodeTrajectoryErrorV2::InvalidDeckId
        }
        NativeFullEpisodeTrajectoryErrorV1::EpisodeMismatch => {
            NativeFullEpisodeTrajectoryErrorV2::EpisodeMismatch
        }
        NativeFullEpisodeTrajectoryErrorV1::EmptyDecisionStream => {
            NativeFullEpisodeTrajectoryErrorV2::EmptyDecisionStream
        }
        NativeFullEpisodeTrajectoryErrorV1::RowOrdinalMismatch => {
            NativeFullEpisodeTrajectoryErrorV2::RowOrdinalMismatch
        }
        NativeFullEpisodeTrajectoryErrorV1::ActorRoleMismatch => {
            NativeFullEpisodeTrajectoryErrorV2::ActorRoleMismatch
        }
        NativeFullEpisodeTrajectoryErrorV1::MalformedPhysicalGroup => {
            NativeFullEpisodeTrajectoryErrorV2::MalformedPhysicalGroup
        }
        NativeFullEpisodeTrajectoryErrorV1::InvalidLegalActionCount => {
            NativeFullEpisodeTrajectoryErrorV2::InvalidLegalActionCount
        }
        NativeFullEpisodeTrajectoryErrorV1::SelectedIndexOutOfRange => {
            NativeFullEpisodeTrajectoryErrorV2::SelectedIndexOutOfRange
        }
        NativeFullEpisodeTrajectoryErrorV1::CounterOverflow => {
            NativeFullEpisodeTrajectoryErrorV2::CounterOverflow
        }
        NativeFullEpisodeTrajectoryErrorV1::NonNaturalTerminal => {
            NativeFullEpisodeTrajectoryErrorV2::NonNaturalTerminal
        }
        NativeFullEpisodeTrajectoryErrorV1::TerminalProvenanceMismatch => {
            NativeFullEpisodeTrajectoryErrorV2::TerminalProvenanceMismatch
        }
        NativeFullEpisodeTrajectoryErrorV1::TerminalCountMismatch => {
            NativeFullEpisodeTrajectoryErrorV2::TerminalCountMismatch
        }
    }
}

// ---------------------------------------- independent frozen authority values
//
// Exactly twenty-four literals, one for each owner-backed envelope atom 2..=25:
// six inner V1 facts, four environment-randomization KDF facts, six reset
// physical-trajectory facts, three trainer-schedule facts, and five runtime deck
// catalog facts.  Atom 1 is V2's own domain string and is not an owner-backed
// authority, and the last nine atoms are per-episode start values.
//
// These literals are the V2 envelope's only authority input.  They are written
// out here independently of the owning modules rather than aliased to the
// imported constants, so a silent edit to any owner constant changes what the
// live guard sees while leaving what V2 hashes fixed: the guard then rejects,
// instead of quietly re-freezing itself around the drift.  Nothing outside this
// module can read or substitute them.

const EXPECTED_INNER_TRAJECTORY_IDENTITY_V2: &str =
    "mtg-kernel-native-full-episode-trajectory-sha256-v1";
const EXPECTED_INNER_TRAJECTORY_GOLDENS_SCHEMA_V2: &str =
    "mtg_kernel_native_full_episode_trajectory_goldens/v1";
const EXPECTED_INNER_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V2: &str =
    "mtg-kernel-native-full-episode-trajectory-goldens-stdlib-python-v1";
const EXPECTED_INNER_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V2: &str =
    "mtg-kernel-native-full-episode-trajectory-golden-vector-stream-sha256-v1";
const EXPECTED_INNER_TRAJECTORY_GOLDENS_FILE_SHA256_V2: &str =
    "502a1b4ba296fdc4b2f4e8fd61cc5b4d64f152c9b84b4e11a85967f76c3bde8b";
const EXPECTED_INNER_TRAJECTORY_GOLDEN_STREAM_SHA256_V2: &str =
    "f5230cbbc0b87735e7aa14c89ce31e41ce769de3f4292cafe63dad4733168d7a";

const EXPECTED_ENVIRONMENT_RANDOMIZATION_IDENTITY_V2: &str =
    "mtg-kernel-environment-randomization-sha256-v2";
const EXPECTED_ENVIRONMENT_RANDOMIZATION_NAMESPACE_V2: &str = "environment-randomization-substream";
const EXPECTED_ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_SCHEMA_V2: &str =
    "mtg-kernel-environment-randomization-v2-goldens/v1";
const EXPECTED_ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_FILE_SHA256_V2: &str =
    "bc2b0d66f8e3eb608b6035321f23a214bbf5141aaf7305f50f606f6c85b4a3bc";

const EXPECTED_RESET_TRAJECTORY_GOLDENS_SCHEMA_V2: &str =
    "mtg-kernel-environment-randomization-v2-reset-physical-trajectory-goldens/v1";
const EXPECTED_RESET_TRAJECTORY_GENERATOR_IDENTITY_V2: &str =
    "mtg-kernel-environment-randomization-v2-reset-physical-trajectory-goldens-stdlib-python-v1";
const EXPECTED_RESET_TRAJECTORY_PHYSICAL_PROJECTION_IDENTITY_V2: &str =
    "mtg-kernel-environment-randomization-v2-physical-card-definition-projection/v1";
const EXPECTED_RESET_TRAJECTORY_VECTOR_STREAM_IDENTITY_V2: &str = "mtg-kernel-environment-randomization-v2-reset-physical-trajectory-portable-vector-stream-sha256-v1";
const EXPECTED_RESET_TRAJECTORY_GOLDENS_FILE_SHA256_V2: &str =
    "18ec6cd138a76bce1bf06c6b794fe169fbe8d83c0a9265d0ff99119a4c4a16bc";
const EXPECTED_RESET_TRAJECTORY_VECTOR_STREAM_SHA256_V2: &str =
    "97f8eeff002ec15f3e30f58fd1f1e477a8abf1db3a38e25aaeb810f87da2a085";

const EXPECTED_TRAINER_SCHEDULE_IDENTITY_V2: &str = "mtg-kernel-native-trainer-schedule-sha256-v1";
const EXPECTED_TRAINER_SEED_VERSION_V2: &str = "kernel-python-rl-trainer-sha256-v2";
const EXPECTED_TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_V2: &str =
    "6b2e1edbbe49b4e02f98794f9057f5c2bb8e3079d2ba8cb3e2a4b9ea6c34867c";

const EXPECTED_RUNTIME_DECK_CATALOG_SCHEMA_V2: &str = "kernel_runtime_decks/v1";
const EXPECTED_RUNTIME_DECK_PROTOCOL_V2: &str = "canonical-mainboard-bo1/v1";
const EXPECTED_RUNTIME_DECK_MATERIALIZATION_PROTOCOL_V2: &str =
    "xmage_xml_row_then_copy_ordinal/v1";
const EXPECTED_RUNTIME_DECK_HASH_ALGORITHM_V2: &str = "fnv1a64-serde-json-u16-array/v1";
// Re-baselined once per the owner ruling on record (collab CLAUDE #236,
// 2026-08-14): the runtime-decks-nine catalog landing is one of the three
// accepted determinism-epoch causes (alongside the two 603.10-family
// observation fixes), and determinism literals re-baseline once at the
// merge epoch rather than carrying a dual profile here -- this module has
// no sealed historical evidence needing backward-compatible decode the way
// native_training_store_run_v2.rs's RunV2 records do; it is a live
// construction-time guard only. Value is the crate's own live
// RUNTIME_DECK_CATALOG_FILE_SHA256 (matches FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1
// in native_training_store_run_v2.rs, the same successor's own CURRENT pin).
const EXPECTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2: &str =
    "68e7602f3a4df6217119406973954630800c358a10fca9f28e6cf9f20fd3b851";

/// The twenty-four live-versus-expected pairs, in envelope atom order 2..=25.
/// The left element is the current constant imported from the owning module; the
/// right element is this module's independent frozen expectation.
const LIVE_AUTHORITY_CHECKS_V2: [(&str, &str); 24] = [
    (
        NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V1,
        EXPECTED_INNER_TRAJECTORY_IDENTITY_V2,
    ),
    (
        NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_SCHEMA_V1,
        EXPECTED_INNER_TRAJECTORY_GOLDENS_SCHEMA_V2,
    ),
    (
        NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V1,
        EXPECTED_INNER_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V2,
    ),
    (
        NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V1,
        EXPECTED_INNER_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V2,
    ),
    (
        NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_FILE_SHA256_V1,
        EXPECTED_INNER_TRAJECTORY_GOLDENS_FILE_SHA256_V2,
    ),
    (
        NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_SHA256_V1,
        EXPECTED_INNER_TRAJECTORY_GOLDEN_STREAM_SHA256_V2,
    ),
    (
        ENVIRONMENT_RANDOMIZATION_IDENTITY_V2,
        EXPECTED_ENVIRONMENT_RANDOMIZATION_IDENTITY_V2,
    ),
    (
        ENVIRONMENT_RANDOMIZATION_NAMESPACE_V2,
        EXPECTED_ENVIRONMENT_RANDOMIZATION_NAMESPACE_V2,
    ),
    (
        ENVIRONMENT_RANDOMIZATION_GOLDENS_SCHEMA_V1,
        EXPECTED_ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_SCHEMA_V2,
    ),
    (
        ENVIRONMENT_RANDOMIZATION_GOLDENS_SHA256_V1,
        EXPECTED_ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_FILE_SHA256_V2,
    ),
    (
        ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_SCHEMA_V1,
        EXPECTED_RESET_TRAJECTORY_GOLDENS_SCHEMA_V2,
    ),
    (
        ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GENERATOR_IDENTITY_V1,
        EXPECTED_RESET_TRAJECTORY_GENERATOR_IDENTITY_V2,
    ),
    (
        ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PHYSICAL_PROJECTION_IDENTITY_V1,
        EXPECTED_RESET_TRAJECTORY_PHYSICAL_PROJECTION_IDENTITY_V2,
    ),
    (
        ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PORTABLE_VECTOR_STREAM_IDENTITY_V1,
        EXPECTED_RESET_TRAJECTORY_VECTOR_STREAM_IDENTITY_V2,
    ),
    (
        ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_SHA256_V1,
        EXPECTED_RESET_TRAJECTORY_GOLDENS_FILE_SHA256_V2,
    ),
    (
        ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PORTABLE_VECTOR_STREAM_SHA256_V1,
        EXPECTED_RESET_TRAJECTORY_VECTOR_STREAM_SHA256_V2,
    ),
    (
        NATIVE_TRAINER_SCHEDULE_VERSION_V1,
        EXPECTED_TRAINER_SCHEDULE_IDENTITY_V2,
    ),
    (
        PYTHON_REFERENCE_SEED_VERSION_V1,
        EXPECTED_TRAINER_SEED_VERSION_V2,
    ),
    (
        NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1,
        EXPECTED_TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_V2,
    ),
    (
        RUNTIME_DECK_CATALOG_SCHEMA,
        EXPECTED_RUNTIME_DECK_CATALOG_SCHEMA_V2,
    ),
    (RUNTIME_DECK_PROTOCOL, EXPECTED_RUNTIME_DECK_PROTOCOL_V2),
    (
        RUNTIME_DECK_MATERIALIZATION_PROTOCOL,
        EXPECTED_RUNTIME_DECK_MATERIALIZATION_PROTOCOL_V2,
    ),
    (
        RUNTIME_DECK_HASH_ALGORITHM,
        EXPECTED_RUNTIME_DECK_HASH_ALGORITHM_V2,
    ),
    (
        RUNTIME_DECK_CATALOG_FILE_SHA256,
        EXPECTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2,
    ),
];

/// The single live authority guard.  Deliberately private and argument-free:
/// there is no declared, injected, or caller-supplied authority block anywhere
/// in the V2 API, so no caller can choose which authorities are checked or
/// which values are hashed.  Every constructing entry point calls this before
/// it looks at any start value.
fn guard_live_source_authorities_v2() -> Result<(), NativeFullEpisodeTrajectoryErrorV2> {
    for (_index, &(live, expected)) in LIVE_AUTHORITY_CHECKS_V2.iter().enumerate() {
        // Test-only synthetic mismatch: the armed slot's expectation is
        // swapped for a value no owner constant can equal, so the production
        // comparison below is what rejects, proving the guard live rather
        // than always-Ok.
        #[cfg(test)]
        let expected = if armed_live_authority_mismatch_slot_for_test_v2() == Some(_index) {
            "\u{0}armed-live-authority-mismatch"
        } else {
            expected
        };
        if live != expected {
            return Err(NativeFullEpisodeTrajectoryErrorV2::AuthorityMismatch);
        }
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    static ARMED_LIVE_AUTHORITY_MISMATCH_SLOT_V2: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn armed_live_authority_mismatch_slot_for_test_v2() -> Option<usize> {
    ARMED_LIVE_AUTHORITY_MISMATCH_SLOT_V2.with(std::cell::Cell::get)
}

/// RAII arming guard for the live-authority mismatch seam: drop restores the
/// exact prior thread-local value on every exit path, including panics.
#[cfg(test)]
pub(crate) struct NativeLiveAuthorityMismatchGuardV2 {
    saved: Option<usize>,
    thread_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(test)]
impl Drop for NativeLiveAuthorityMismatchGuardV2 {
    fn drop(&mut self) {
        ARMED_LIVE_AUTHORITY_MISMATCH_SLOT_V2.with(|cell| cell.set(self.saved));
    }
}

#[cfg(test)]
pub(crate) fn arm_live_authority_mismatch_for_test_v2(
    slot: usize,
) -> NativeLiveAuthorityMismatchGuardV2 {
    let saved = ARMED_LIVE_AUTHORITY_MISMATCH_SLOT_V2.with(|cell| cell.replace(Some(slot)));
    NativeLiveAuthorityMismatchGuardV2 {
        saved,
        thread_bound: std::marker::PhantomData,
    }
}

// ------------------------------------------------------------ start binding

/// The complete V2 start.  There is no inner-digest, inner-root, inner-seat,
/// inner-deck-ID, or inner-deck-hash field: these values are the only source
/// the owned V1 accumulator is ever constructed from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeFullEpisodeTrajectoryStartV2 {
    pub(crate) episode_index: u64,
    /// The full unsigned 64-bit pair root, passed verbatim as the V1
    /// environment seed and never masked.
    pub(crate) pair_environment_seed: u64,
    /// Ordered physical bindings: index 0 is the P0 seat, index 1 is the P1
    /// seat, regardless of which seat holds the learner.
    pub(crate) deck_ids: SessionDeckIdsV1,
    pub(crate) deck_hashes: SessionDeckHashesV1,
    pub(crate) learner_seat: PlayerSeatV1,
}

/// A start that has passed every V2 start rule.  The deck IDs are the catalog's
/// own `&'static str`s, which are byte-identical to the supplied IDs because
/// resolution is exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeFullEpisodeTrajectoryValidatedStartV2 {
    pub(crate) episode_index: u64,
    pub(crate) pair_index: u64,
    pub(crate) pair_environment_seed: u64,
    pub(crate) deck_ids: [&'static str; 2],
    pub(crate) deck_hashes: SessionDeckHashesV1,
    pub(crate) learner_seat: PlayerSeatV1,
}

/// Frozen V2 start precedence, exactly as implemented below:
///
/// 1. checked u63 `episode_index`;
/// 2. both physical deck-ID shape checks together, P0 and P1, before either ID
///    is looked up;
/// 3. P0 catalog resolution then the P0 runtime-hash equality;
/// 4. P1 catalog resolution then the P1 runtime-hash equality;
/// 5. learner-seat parity.
///
/// This function does not itself run the live authority guard.  The two
/// production paths that construct or bind, `new_v2` and the pair validator, run
/// the guard before they reach this function, so on those paths a start-shape
/// rejection can never precede authority drift.  Calling this function directly
/// checks start shape only and makes no authority claim whatsoever: an `Ok`
/// validated start is not evidence that any owner authority is undrifted.
pub(crate) fn validate_start_v2(
    start: &NativeFullEpisodeTrajectoryStartV2,
) -> Result<NativeFullEpisodeTrajectoryValidatedStartV2, NativeFullEpisodeTrajectoryErrorV2> {
    if start.episode_index > U63_MAX_V2 {
        return Err(NativeFullEpisodeTrajectoryErrorV2::EpisodeIndexOutsideU63);
    }
    if !valid_physical_deck_id_v2(&start.deck_ids[0])
        || !valid_physical_deck_id_v2(&start.deck_ids[1])
    {
        return Err(NativeFullEpisodeTrajectoryErrorV2::InvalidDeckId);
    }
    let deck_p0_id = resolve_physical_deck_v2(&start.deck_ids[0], start.deck_hashes[0])?;
    let deck_p1_id = resolve_physical_deck_v2(&start.deck_ids[1], start.deck_hashes[1])?;
    if start.learner_seat != learner_seat_for_episode_v2(start.episode_index) {
        return Err(NativeFullEpisodeTrajectoryErrorV2::LearnerSeatRuleMismatch);
    }
    Ok(NativeFullEpisodeTrajectoryValidatedStartV2 {
        episode_index: start.episode_index,
        pair_index: start.episode_index / 2,
        pair_environment_seed: start.pair_environment_seed,
        deck_ids: [deck_p0_id, deck_p1_id],
        deck_hashes: start.deck_hashes,
        learner_seat: start.learner_seat,
    })
}

/// Even episode means a P0 learner; odd means a P1 learner.
fn learner_seat_for_episode_v2(episode_index: u64) -> PlayerSeatV1 {
    if episode_index.is_multiple_of(2) {
        PlayerSeatV1::P0
    } else {
        PlayerSeatV1::P1
    }
}

/// The same predicate the frozen V1 accumulator applies to deck IDs.
fn valid_physical_deck_id_v2(deck_id: &str) -> bool {
    let bytes = deck_id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_DECK_ID_BYTES_V2
        && bytes.iter().all(|byte| (0x20..=0x7e).contains(byte))
}

/// Exact runtime-catalog resolution: the ID must resolve exactly, then the
/// supplied hash must equal that deck's frozen runtime hash.
fn resolve_physical_deck_v2(
    deck_id: &str,
    deck_hash: u64,
) -> Result<&'static str, NativeFullEpisodeTrajectoryErrorV2> {
    let definition =
        runtime_deck_by_id(deck_id).ok_or(NativeFullEpisodeTrajectoryErrorV2::InvalidDeckId)?;
    if definition.runtime_deck_hash != deck_hash {
        return Err(NativeFullEpisodeTrajectoryErrorV2::RuntimeDeckHashMismatch);
    }
    Ok(definition.id)
}

const fn physical_seat_code_v2(seat: PlayerSeatV1) -> u8 {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

// ---------------------------------------------------------------- V2 receipt

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeFullEpisodeTrajectoryReceiptV2 {
    pub(crate) episode_index: u64,
    pub(crate) pair_index: u64,
    pub(crate) pair_environment_seed: u64,
    pub(crate) deck_ids: [&'static str; 2],
    pub(crate) deck_hashes: SessionDeckHashesV1,
    pub(crate) learner_seat: PlayerSeatV1,
    /// The raw32 digest the owned inner V1 accumulator returned.
    pub(crate) inner_trajectory_sha256: [u8; 32],
    /// The frozen 34-atom V2 envelope digest over that inner digest.
    pub(crate) trajectory_sha256_v2: [u8; 32],
    pub(crate) policy_step_count: u64,
    pub(crate) physical_decision_count: u64,
    pub(crate) learner_policy_step_count: u64,
    pub(crate) opponent_policy_step_count: u64,
    pub(crate) learner_physical_decision_count: u64,
    pub(crate) opponent_physical_decision_count: u64,
}

// ------------------------------------------------------------ V2 accumulator

/// Owns exactly one inner V1 accumulator for the lifetime of one episode.  The
/// inner accumulator is never handed out, never replaced, and never fed values
/// other than the validated start this V2 accumulator was built from.
pub(crate) struct NativeFullEpisodeTrajectoryAccumulatorV2 {
    start: NativeFullEpisodeTrajectoryValidatedStartV2,
    inner: NativeFullEpisodeTrajectoryAccumulatorV1,
}

impl NativeFullEpisodeTrajectoryAccumulatorV2 {
    /// The only V2 constructor.  It runs the live authority guard before it
    /// inspects the episode index, the deck bindings, or the seat, so drifted
    /// authority can never reach the owned inner accumulator and no caller can
    /// supply, declare, or skip the authority set.  It then validates the V2
    /// start and constructs the owned inner V1 accumulator from exactly that
    /// start: episode, the full-u64 pair root as the V1 environment seed, the
    /// ordered physical deck IDs and hashes, and the learner seat.
    pub(crate) fn new_v2(
        start: &NativeFullEpisodeTrajectoryStartV2,
    ) -> Result<Self, NativeFullEpisodeTrajectoryErrorV2> {
        guard_live_source_authorities_v2()?;
        let validated = validate_start_v2(start)?;
        let inner = NativeFullEpisodeTrajectoryAccumulatorV1::new_v1(
            validated.episode_index,
            validated.pair_environment_seed,
            &start.deck_ids,
            validated.deck_hashes,
            validated.learner_seat,
        )
        .map_err(map_inner_error_v2)?;
        Ok(Self {
            start: validated,
            inner,
        })
    }

    pub(crate) fn validated_start_v2(&self) -> NativeFullEpisodeTrajectoryValidatedStartV2 {
        self.start
    }

    /// Delegates to the owned inner accumulator without changing its digest or
    /// counters.
    pub(crate) fn preflight_candidate_v2(
        &self,
        row: NativeFullEpisodeTrajectoryDecisionRowV1,
    ) -> Result<(), NativeFullEpisodeTrajectoryErrorV2> {
        self.inner
            .preflight_candidate_v1(row)
            .map_err(map_inner_error_v2)
    }

    /// Delegates the accepted row to the owned inner accumulator.  V2 folds no
    /// row bytes of its own.
    pub(crate) fn record_accepted_v2(
        &mut self,
        row: NativeFullEpisodeTrajectoryDecisionRowV1,
    ) -> Result<(), NativeFullEpisodeTrajectoryErrorV2> {
        self.inner
            .record_accepted_v1(row)
            .map_err(map_inner_error_v2)
    }

    /// Finishes the owned inner accumulator, then envelopes only the raw32
    /// digest it returned.  No terminal or row byte is re-serialized by V2.
    pub(crate) fn finish_natural_v2(
        self,
        terminal: AsyncRolloutTerminalV1,
        terminal_deck_hashes: SessionDeckHashesV1,
    ) -> Result<NativeFullEpisodeTrajectoryReceiptV2, NativeFullEpisodeTrajectoryErrorV2> {
        let Self { start, inner } = self;
        let inner_receipt = inner
            .finish_natural_v1(terminal, terminal_deck_hashes)
            .map_err(map_inner_error_v2)?;

        // Ownership invariant, guaranteed by construction: the inner receipt
        // can only describe the same start this V2 accumulator supplied.
        debug_assert_eq!(inner_receipt.episode_index, start.episode_index);
        debug_assert_eq!(inner_receipt.environment_seed, start.pair_environment_seed);
        debug_assert_eq!(inner_receipt.deck_hashes, start.deck_hashes);
        debug_assert_eq!(inner_receipt.learner_seat, start.learner_seat);

        let trajectory_sha256_v2 = envelope_sha256_v2(&start, inner_receipt.trajectory_sha256);
        Ok(NativeFullEpisodeTrajectoryReceiptV2 {
            episode_index: start.episode_index,
            pair_index: start.pair_index,
            pair_environment_seed: start.pair_environment_seed,
            deck_ids: start.deck_ids,
            deck_hashes: start.deck_hashes,
            learner_seat: start.learner_seat,
            inner_trajectory_sha256: inner_receipt.trajectory_sha256,
            trajectory_sha256_v2,
            policy_step_count: inner_receipt.policy_step_count,
            physical_decision_count: inner_receipt.physical_decision_count,
            learner_policy_step_count: inner_receipt.learner_policy_step_count,
            opponent_policy_step_count: inner_receipt.opponent_policy_step_count,
            learner_physical_decision_count: inner_receipt.learner_physical_decision_count,
            opponent_physical_decision_count: inner_receipt.opponent_physical_decision_count,
        })
    }
}

// ----------------------------------------------------------- frozen envelope

/// Deliberately private.  A public envelope builder would be an API that
/// accepts an externally supplied inner digest, which the V2 contract forbids;
/// the only caller is `finish_natural_v2`, which passes the digest its own
/// inner accumulator just returned.
fn envelope_sha256_v2(
    start: &NativeFullEpisodeTrajectoryValidatedStartV2,
    inner_trajectory_sha256: [u8; 32],
) -> [u8; 32] {
    let mut envelope = EnvelopeHasherV2::new_v2();
    envelope.atom_v2(
        "domain",
        NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V2.as_bytes(),
    );
    envelope.atom_v2(
        "inner_trajectory_identity_utf8",
        EXPECTED_INNER_TRAJECTORY_IDENTITY_V2.as_bytes(),
    );
    envelope.atom_v2(
        "inner_trajectory_goldens_schema_utf8",
        EXPECTED_INNER_TRAJECTORY_GOLDENS_SCHEMA_V2.as_bytes(),
    );
    envelope.atom_v2(
        "inner_trajectory_goldens_generator_identity_utf8",
        EXPECTED_INNER_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V2.as_bytes(),
    );
    envelope.atom_v2(
        "inner_trajectory_golden_stream_identity_utf8",
        EXPECTED_INNER_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V2.as_bytes(),
    );
    envelope.atom_v2(
        "inner_trajectory_goldens_file_sha256_raw32",
        &INNER_GOLDENS_FILE_SHA256_RAW32_V2,
    );
    envelope.atom_v2(
        "inner_trajectory_golden_stream_sha256_raw32",
        &INNER_GOLDEN_STREAM_SHA256_RAW32_V2,
    );
    envelope.atom_v2(
        "environment_randomization_identity_utf8",
        EXPECTED_ENVIRONMENT_RANDOMIZATION_IDENTITY_V2.as_bytes(),
    );
    envelope.atom_v2(
        "environment_randomization_namespace_utf8",
        EXPECTED_ENVIRONMENT_RANDOMIZATION_NAMESPACE_V2.as_bytes(),
    );
    envelope.atom_v2(
        "environment_randomization_kdf_goldens_schema_utf8",
        EXPECTED_ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_SCHEMA_V2.as_bytes(),
    );
    envelope.atom_v2(
        "environment_randomization_kdf_goldens_file_sha256_raw32",
        &ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_FILE_SHA256_RAW32_V2,
    );
    envelope.atom_v2(
        "reset_trajectory_goldens_schema_utf8",
        EXPECTED_RESET_TRAJECTORY_GOLDENS_SCHEMA_V2.as_bytes(),
    );
    envelope.atom_v2(
        "reset_trajectory_generator_identity_utf8",
        EXPECTED_RESET_TRAJECTORY_GENERATOR_IDENTITY_V2.as_bytes(),
    );
    envelope.atom_v2(
        "reset_trajectory_physical_projection_identity_utf8",
        EXPECTED_RESET_TRAJECTORY_PHYSICAL_PROJECTION_IDENTITY_V2.as_bytes(),
    );
    envelope.atom_v2(
        "reset_trajectory_vector_stream_identity_utf8",
        EXPECTED_RESET_TRAJECTORY_VECTOR_STREAM_IDENTITY_V2.as_bytes(),
    );
    envelope.atom_v2(
        "reset_trajectory_goldens_file_sha256_raw32",
        &RESET_TRAJECTORY_GOLDENS_FILE_SHA256_RAW32_V2,
    );
    envelope.atom_v2(
        "reset_trajectory_vector_stream_sha256_raw32",
        &RESET_TRAJECTORY_VECTOR_STREAM_SHA256_RAW32_V2,
    );
    envelope.atom_v2(
        "trainer_schedule_identity_utf8",
        EXPECTED_TRAINER_SCHEDULE_IDENTITY_V2.as_bytes(),
    );
    envelope.atom_v2(
        "trainer_seed_version_utf8",
        EXPECTED_TRAINER_SEED_VERSION_V2.as_bytes(),
    );
    envelope.atom_v2(
        "trainer_schedule_goldens_file_sha256_raw32",
        &TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_RAW32_V2,
    );
    envelope.atom_v2(
        "runtime_deck_catalog_schema_utf8",
        EXPECTED_RUNTIME_DECK_CATALOG_SCHEMA_V2.as_bytes(),
    );
    envelope.atom_v2(
        "runtime_deck_protocol_utf8",
        EXPECTED_RUNTIME_DECK_PROTOCOL_V2.as_bytes(),
    );
    envelope.atom_v2(
        "runtime_deck_materialization_protocol_utf8",
        EXPECTED_RUNTIME_DECK_MATERIALIZATION_PROTOCOL_V2.as_bytes(),
    );
    envelope.atom_v2(
        "runtime_deck_hash_algorithm_utf8",
        EXPECTED_RUNTIME_DECK_HASH_ALGORITHM_V2.as_bytes(),
    );
    envelope.atom_v2(
        "runtime_deck_catalog_file_sha256_raw32",
        &RUNTIME_DECK_CATALOG_FILE_SHA256_RAW32_V2,
    );
    envelope.atom_v2("episode_index_u64be", &start.episode_index.to_be_bytes());
    envelope.atom_v2("pair_index_u64be", &start.pair_index.to_be_bytes());
    envelope.atom_v2(
        "pair_environment_seed_u64be",
        &start.pair_environment_seed.to_be_bytes(),
    );
    envelope.atom_v2("deck_p0_id_utf8", start.deck_ids[0].as_bytes());
    envelope.atom_v2("deck_p0_hash_u64be", &start.deck_hashes[0].to_be_bytes());
    envelope.atom_v2("deck_p1_id_utf8", start.deck_ids[1].as_bytes());
    envelope.atom_v2("deck_p1_hash_u64be", &start.deck_hashes[1].to_be_bytes());
    envelope.atom_v2(
        "learner_seat_u8",
        &[physical_seat_code_v2(start.learner_seat)],
    );
    // The final atom is the raw32 digest the owned inner accumulator returned.
    // No V2 self-hash follows it.
    envelope.atom_v2("inner_trajectory_sha256_raw32", &inner_trajectory_sha256);
    envelope.finish_v2()
}

struct EnvelopeHasherV2 {
    hasher: Sha256,
    atom_count: usize,
}

impl EnvelopeHasherV2 {
    fn new_v2() -> Self {
        Self {
            hasher: Sha256::new(),
            atom_count: 0,
        }
    }

    /// The frozen V1 framing, reused unchanged:
    /// `u32be(tag_len) || tag || u64be(payload_len) || payload`.
    fn atom_v2(&mut self, tag: &str, payload: &[u8]) {
        let tag_len = u32::try_from(tag.len()).expect("V2 envelope atom tag length fits u32");
        let payload_len =
            u64::try_from(payload.len()).expect("V2 envelope atom payload length fits u64");
        self.hasher.update(tag_len.to_be_bytes());
        self.hasher.update(tag.as_bytes());
        self.hasher.update(payload_len.to_be_bytes());
        self.hasher.update(payload);
        self.atom_count += 1;
    }

    fn finish_v2(self) -> [u8; 32] {
        assert_eq!(
            self.atom_count, NATIVE_FULL_EPISODE_TRAJECTORY_ENVELOPE_ATOM_COUNT_V2,
            "the V2 envelope is frozen at exactly thirty-four atoms"
        );
        self.hasher.finalize().into()
    }
}

// --------------------------------------------------- decoded raw32 authorities
//
// Every `*_sha256_raw32` payload is the decoded 32-byte pin, never ASCII hex.
// Decoding is `const`, so a pin that is not exactly sixty-four lowercase hex
// digits fails the build rather than producing a wrong envelope.

const INNER_GOLDENS_FILE_SHA256_RAW32_V2: [u8; 32] =
    decode_sha256_pin_v2(EXPECTED_INNER_TRAJECTORY_GOLDENS_FILE_SHA256_V2);
const INNER_GOLDEN_STREAM_SHA256_RAW32_V2: [u8; 32] =
    decode_sha256_pin_v2(EXPECTED_INNER_TRAJECTORY_GOLDEN_STREAM_SHA256_V2);
const ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_FILE_SHA256_RAW32_V2: [u8; 32] =
    decode_sha256_pin_v2(EXPECTED_ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_FILE_SHA256_V2);
const RESET_TRAJECTORY_GOLDENS_FILE_SHA256_RAW32_V2: [u8; 32] =
    decode_sha256_pin_v2(EXPECTED_RESET_TRAJECTORY_GOLDENS_FILE_SHA256_V2);
const RESET_TRAJECTORY_VECTOR_STREAM_SHA256_RAW32_V2: [u8; 32] =
    decode_sha256_pin_v2(EXPECTED_RESET_TRAJECTORY_VECTOR_STREAM_SHA256_V2);
const TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_RAW32_V2: [u8; 32] =
    decode_sha256_pin_v2(EXPECTED_TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_V2);
const RUNTIME_DECK_CATALOG_FILE_SHA256_RAW32_V2: [u8; 32] =
    decode_sha256_pin_v2(EXPECTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2);

const fn decode_sha256_pin_v2(pin: &str) -> [u8; 32] {
    let bytes = pin.as_bytes();
    assert!(
        bytes.len() == 64,
        "a SHA-256 pin is exactly sixty-four lowercase hex digits"
    );
    let mut decoded = [0_u8; 32];
    let mut index = 0;
    while index < 32 {
        decoded[index] = (lower_hex_nibble_v2(bytes[index * 2]) << 4)
            | lower_hex_nibble_v2(bytes[index * 2 + 1]);
        index += 1;
    }
    decoded
}

const fn lower_hex_nibble_v2(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => panic!("a SHA-256 pin is exactly sixty-four lowercase hex digits"),
    }
}

// ------------------------------------------------------- wire-shape boundary

/// The only production path that can observe a malformed flat-action
/// commitment.  Rust callers that already hold a `[u8; 16]` cannot construct
/// one, so `MalformedCommitment` is reachable exactly at an artifact or wire
/// boundary; this helper is where that boundary is crossed.
pub(crate) fn checked_flat_action_v2_commitment_v2(
    lower_hex: &str,
) -> Result<[u8; 16], NativeFullEpisodeTrajectoryErrorV2> {
    let bytes = lower_hex.as_bytes();
    if bytes.len() != 32
        || !bytes
            .iter()
            .all(|&byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(NativeFullEpisodeTrajectoryErrorV2::MalformedCommitment);
    }
    let mut commitment = [0_u8; 16];
    for (index, slot) in commitment.iter_mut().enumerate() {
        *slot = (lower_hex_nibble_v2(bytes[index * 2]) << 4)
            | lower_hex_nibble_v2(bytes[index * 2 + 1]);
    }
    Ok(commitment)
}

// ----------------------------------------------------------- pair validator

/// The proven binding of one even/odd episode pair.  It deliberately carries no
/// trajectory digest: pair validity is decided from start metadata and the
/// schedule alone, so no externally supplied digest can influence it.
///
/// What this therefore proves is exactly the start, schedule, and physical
/// binding of the pair: that both starts are individually valid, that they are
/// the `2k` and `2k+1` episodes of the named pair with the P0/P1 learner seats,
/// that both carry the root the frozen trainer schedule derives for that pair,
/// and that the ordered physical deck bindings are identical across the seat
/// swap.  It proves nothing about either episode's decision stream or digest;
/// that evidence lives in each episode's own V2 receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeFullEpisodeTrajectoryPairBindingV2 {
    pub(crate) base_seed: u64,
    pub(crate) pair_index: u64,
    pub(crate) pair_environment_seed: u64,
    pub(crate) even_episode_index: u64,
    pub(crate) odd_episode_index: u64,
    pub(crate) deck_ids: [&'static str; 2],
    pub(crate) deck_hashes: SessionDeckHashesV1,
}

/// Closed pair arithmetic, in frozen precedence:
///
/// 1. `base_seed <= 2^63 - 1`;
/// 2. `pair_index <= 2^62 - 1`, so checked `2k` and `2k+1` both stay u63;
/// 3. the live authority guard, before either start is inspected;
/// 4. both starts validated independently, even before odd, before any pair
///    comparison;
/// 5. each start's derived `episode_index / 2` against the explicit pair index,
///    then exact `2k` and `2k+1`;
/// 6. P0 learner on the even start and P1 learner on the odd start;
/// 7. the schedule-derived shared root on both starts;
/// 8. identical ordered physical deck IDs and hashes.
pub(crate) fn validate_native_full_episode_trajectory_pair_v2(
    base_seed: u64,
    pair_index: u64,
    even_start: &NativeFullEpisodeTrajectoryStartV2,
    odd_start: &NativeFullEpisodeTrajectoryStartV2,
) -> Result<NativeFullEpisodeTrajectoryPairBindingV2, NativeFullEpisodeTrajectoryErrorV2> {
    if base_seed > U63_MAX_V2 {
        return Err(NativeFullEpisodeTrajectoryErrorV2::ScheduleIntegerOutsideU63);
    }
    if pair_index > U62_MAX_V2 {
        return Err(NativeFullEpisodeTrajectoryErrorV2::PairIndexOutsideEpisodeDomain);
    }
    guard_live_source_authorities_v2()?;

    let even = validate_start_v2(even_start)?;
    let odd = validate_start_v2(odd_start)?;

    if even.pair_index != pair_index || odd.pair_index != pair_index {
        return Err(NativeFullEpisodeTrajectoryErrorV2::PairEpisodeIndexMismatch);
    }
    let Some(expected_even_episode) = pair_index.checked_mul(2) else {
        return Err(NativeFullEpisodeTrajectoryErrorV2::PairIndexOutsideEpisodeDomain);
    };
    let Some(expected_odd_episode) = expected_even_episode.checked_add(1) else {
        return Err(NativeFullEpisodeTrajectoryErrorV2::PairIndexOutsideEpisodeDomain);
    };
    if even.episode_index != expected_even_episode || odd.episode_index != expected_odd_episode {
        return Err(NativeFullEpisodeTrajectoryErrorV2::PairEpisodeIndexMismatch);
    }
    if even.learner_seat != PlayerSeatV1::P0 || odd.learner_seat != PlayerSeatV1::P1 {
        return Err(NativeFullEpisodeTrajectoryErrorV2::LearnerSeatRuleMismatch);
    }

    // The shared root is derived through the frozen native trainer schedule
    // itself, never reimplemented here.
    let schedule = native_trainer_episode_schedule_v1(base_seed, even.episode_index)
        .map_err(|_| NativeFullEpisodeTrajectoryErrorV2::ScheduleIntegerOutsideU63)?;
    if schedule.pair_index != pair_index {
        return Err(NativeFullEpisodeTrajectoryErrorV2::PairEpisodeIndexMismatch);
    }
    if schedule.learner_seat != PlayerSeatV1::P0 {
        return Err(NativeFullEpisodeTrajectoryErrorV2::LearnerSeatRuleMismatch);
    }
    let derived_root = schedule.environment_seed;
    if even.pair_environment_seed != derived_root || odd.pair_environment_seed != derived_root {
        return Err(NativeFullEpisodeTrajectoryErrorV2::PairEnvironmentSeedMismatch);
    }

    // Learner-seat parity swapped between the two episodes; the ordered
    // physical bindings must not have moved with it.
    if even.deck_ids != odd.deck_ids || even.deck_hashes != odd.deck_hashes {
        return Err(NativeFullEpisodeTrajectoryErrorV2::PairPhysicalDeckBindingMismatch);
    }

    Ok(NativeFullEpisodeTrajectoryPairBindingV2 {
        base_seed,
        pair_index,
        pair_environment_seed: derived_root,
        even_episode_index: even.episode_index,
        odd_episode_index: odd.episode_index,
        deck_ids: even.deck_ids,
        deck_hashes: even.deck_hashes,
    })
}

// ------------------------------------------------ run-bound receipt wrapper

/// The closed crate-private run-bound receipt. Exactly two variants, one per
/// arm of the sealed run trajectory contract; there is no third state and no
/// wildcard consumer. External code can never name this enum: the module is
/// crate-private and the only public projection is the opaque
/// [`NativeTrainingTrajectoryReceiptV2`] wrapper below.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeRunBoundFullEpisodeReceiptV2 {
    LegacyV1(NativeFullEpisodeTrajectoryReceiptV1),
    EnvironmentRandomizationV2(NativeFullEpisodeTrajectoryReceiptV2),
}

/// One public opaque trajectory receipt for both run modes.
///
/// The inner run-bound variant is private: there is no public constructor, no
/// `Deref`, no `AsMut`, no serde, and no blanket `From`, so a caller can
/// neither forge a receipt from raw digests nor launder one variant into the
/// other. Common facts are exposed through immutable accessors only.
/// `trajectory_sha256` stays the compatibility accessor consumed by
/// `EpisodeWireV1`: the V1 trajectory digest for a legacy receipt and the
/// inner V1 digest for an environment randomization V2 receipt. The V2 outer
/// envelope digest is exposed only through the explicit optional accessor and
/// never enters any persisted byte stream.
///
/// External construction is impossible, both literally and by forging from a
/// raw V1 receipt's digest fields:
///
/// ```compile_fail
/// use mtg_kernel::native_training_executor_v1::NativeTrainingTrajectoryReceiptV2;
/// fn forge() -> NativeTrainingTrajectoryReceiptV2 {
///     NativeTrainingTrajectoryReceiptV2 {}
/// }
/// ```
///
/// The private inner field can be neither read nor mutated:
///
/// ```compile_fail
/// use mtg_kernel::native_training_executor_v1::NativeTrainingTrajectoryReceiptV2;
/// fn read_inner(receipt: &NativeTrainingTrajectoryReceiptV2) {
///     let _ = &receipt.inner;
/// }
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_executor_v1::NativeTrainingTrajectoryReceiptV2;
/// fn mutate_inner(receipt: &mut NativeTrainingTrajectoryReceiptV2) {
///     let _ = &mut receipt.inner;
/// }
/// ```
///
/// Digest facts are methods, not mutable fields:
///
/// ```compile_fail
/// use mtg_kernel::native_training_executor_v1::NativeTrainingTrajectoryReceiptV2;
/// fn mutate_digest(receipt: &mut NativeTrainingTrajectoryReceiptV2) {
///     receipt.trajectory_sha256 = [0_u8; 32];
/// }
/// ```
///
/// There is no serde in either direction:
///
/// ```compile_fail
/// use mtg_kernel::native_training_executor_v1::NativeTrainingTrajectoryReceiptV2;
/// fn require_serialize<T: serde::Serialize>() {}
/// fn probe() {
///     require_serialize::<NativeTrainingTrajectoryReceiptV2>();
/// }
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_executor_v1::NativeTrainingTrajectoryReceiptV2;
/// fn require_deserialize<'de, T: serde::Deserialize<'de>>() {}
/// fn probe<'de>() {
///     require_deserialize::<'de, NativeTrainingTrajectoryReceiptV2>();
/// }
/// ```
///
/// No `Deref` or `AsMut` widens the opaque surface:
///
/// ```compile_fail
/// use mtg_kernel::native_training_executor_v1::NativeTrainingTrajectoryReceiptV2;
/// fn require_deref<T: std::ops::Deref>() {}
/// fn probe() {
///     require_deref::<NativeTrainingTrajectoryReceiptV2>();
/// }
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_executor_v1::{
///     NativeTrainingTrajectoryReceiptV1, NativeTrainingTrajectoryReceiptV2,
/// };
/// fn require_as_mut<T: AsMut<NativeTrainingTrajectoryReceiptV1>>() {}
/// fn probe() {
///     require_as_mut::<NativeTrainingTrajectoryReceiptV2>();
/// }
/// ```
///
/// Neither raw receipt converts in via `From`:
///
/// ```compile_fail
/// use mtg_kernel::native_training_executor_v1::{
///     NativeTrainingTrajectoryReceiptV1, NativeTrainingTrajectoryReceiptV2,
/// };
/// fn require_from<T: From<NativeTrainingTrajectoryReceiptV1>>() {}
/// fn probe() {
///     require_from::<NativeTrainingTrajectoryReceiptV2>();
/// }
/// ```
///
/// The crate-private run-bound inner enum cannot even be named:
///
/// ```compile_fail
/// use mtg_kernel::native_full_episode_trajectory_v2::NativeRunBoundFullEpisodeReceiptV2;
/// ```
///
/// The private sealed mode and the run-bound constructors are likewise
/// unreachable from outside the crate:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_run_v2::NativeRunEnvironmentTrajectoryContractV1;
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_executor_v1::NativeTrainingExecutorV1;
/// use mtg_kernel::native_training_store_run_v2::ValidatedTrainRunV2;
/// fn probe(
///     config: mtg_kernel::native_training_executor_v1::NativeTrainingExecutionConfigV1,
///     run: &ValidatedTrainRunV2,
/// ) {
///     let _ = NativeTrainingExecutorV1::from_common_model_snapshot_run_bound_v2(
///         config,
///         std::path::Path::new("m"),
///         std::path::Path::new("p"),
///         run,
///     );
/// }
/// ```
///
/// The wrapper deliberately stays `Copy + Debug + Eq + Send + Sync`:
///
/// ```
/// use mtg_kernel::native_training_executor_v1::NativeTrainingTrajectoryReceiptV2;
/// fn positive<T: Copy + std::fmt::Debug + Eq + Send + Sync>() {}
/// positive::<NativeTrainingTrajectoryReceiptV2>();
/// ```
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NativeTrainingTrajectoryReceiptV2 {
    inner: NativeRunBoundFullEpisodeReceiptV2,
}

/// Custom, not derived: a derived `Debug` would recursively print the private
/// run-bound variant plus V2-only pair and deck-binding facts. Only the
/// authorized common projections and the optional outer digest are formatted,
/// and the private inner enum deliberately implements no `Debug` at all.
impl std::fmt::Debug for NativeTrainingTrajectoryReceiptV2 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeTrainingTrajectoryReceiptV2")
            .field("episode_index", &self.episode_index())
            .field("environment_seed", &self.environment_seed())
            .field("deck_hashes", &self.deck_hashes())
            .field("learner_seat", &self.learner_seat())
            .field("trajectory_sha256", &self.trajectory_sha256())
            .field(
                "outer_trajectory_sha256_v2",
                &self.outer_trajectory_sha256_v2(),
            )
            .field("policy_step_count", &self.policy_step_count())
            .field("physical_decision_count", &self.physical_decision_count())
            .field(
                "learner_policy_step_count",
                &self.learner_policy_step_count(),
            )
            .field(
                "opponent_policy_step_count",
                &self.opponent_policy_step_count(),
            )
            .field(
                "learner_physical_decision_count",
                &self.learner_physical_decision_count(),
            )
            .field(
                "opponent_physical_decision_count",
                &self.opponent_physical_decision_count(),
            )
            .finish_non_exhaustive()
    }
}

impl NativeTrainingTrajectoryReceiptV2 {
    pub(crate) const fn from_legacy_v1(receipt: NativeFullEpisodeTrajectoryReceiptV1) -> Self {
        Self {
            inner: NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt),
        }
    }

    pub(crate) const fn from_environment_randomization_v2(
        receipt: NativeFullEpisodeTrajectoryReceiptV2,
    ) -> Self {
        Self {
            inner: NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt),
        }
    }

    /// Crate-private variant inspection for exhaustive diagonal validation.
    pub(crate) const fn is_environment_randomization_v2(&self) -> bool {
        match self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(_) => false,
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(_) => true,
        }
    }

    pub const fn episode_index(&self) -> u64 {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt) => receipt.episode_index,
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                receipt.episode_index
            }
        }
    }

    /// The environment seed the episode was reset with: the legacy per-episode
    /// derived seed for a V1 receipt, the full-width shared pair root for an
    /// environment randomization V2 receipt.
    pub const fn environment_seed(&self) -> u64 {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt) => receipt.environment_seed,
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                receipt.pair_environment_seed
            }
        }
    }

    pub const fn deck_hashes(&self) -> SessionDeckHashesV1 {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt) => receipt.deck_hashes,
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                receipt.deck_hashes
            }
        }
    }

    pub const fn learner_seat(&self) -> PlayerSeatV1 {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt) => receipt.learner_seat,
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                receipt.learner_seat
            }
        }
    }

    /// The compatibility trajectory digest consumed by `EpisodeWireV1`: the V1
    /// digest for a legacy receipt, the inner V1 digest for an environment
    /// randomization V2 receipt.
    pub const fn trajectory_sha256(&self) -> [u8; 32] {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt) => receipt.trajectory_sha256,
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                receipt.inner_trajectory_sha256
            }
        }
    }

    /// The frozen 34-atom V2 outer envelope digest. `None` for a legacy
    /// receipt. Ephemeral evidence only: it never appears in EpisodeWire,
    /// canonical continuation bytes, checkpoint bytes, or any sidecar.
    pub const fn outer_trajectory_sha256_v2(&self) -> Option<[u8; 32]> {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(_) => None,
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                Some(receipt.trajectory_sha256_v2)
            }
        }
    }

    pub const fn policy_step_count(&self) -> u64 {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt) => receipt.policy_step_count,
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                receipt.policy_step_count
            }
        }
    }

    pub const fn physical_decision_count(&self) -> u64 {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt) => {
                receipt.physical_decision_count
            }
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                receipt.physical_decision_count
            }
        }
    }

    pub const fn learner_policy_step_count(&self) -> u64 {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt) => {
                receipt.learner_policy_step_count
            }
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                receipt.learner_policy_step_count
            }
        }
    }

    pub const fn opponent_policy_step_count(&self) -> u64 {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt) => {
                receipt.opponent_policy_step_count
            }
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                receipt.opponent_policy_step_count
            }
        }
    }

    pub const fn learner_physical_decision_count(&self) -> u64 {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt) => {
                receipt.learner_physical_decision_count
            }
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                receipt.learner_physical_decision_count
            }
        }
    }

    pub const fn opponent_physical_decision_count(&self) -> u64 {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt) => {
                receipt.opponent_physical_decision_count
            }
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                receipt.opponent_physical_decision_count
            }
        }
    }

    /// Optional read-only Legacy V1 view. `None` for an environment
    /// randomization V2 receipt; V2-only facts stay crate-private.
    pub const fn legacy_v1_view(&self) -> Option<&NativeFullEpisodeTrajectoryReceiptV1> {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt) => Some(receipt),
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(_) => None,
        }
    }

    /// Crate-private V2-only pair index. `None` for a legacy receipt.
    pub(crate) const fn pair_index_v2(&self) -> Option<u64> {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(_) => None,
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                Some(receipt.pair_index)
            }
        }
    }

    /// Crate-private V2-only catalog-resolved ordered physical deck bindings.
    /// `None` for a legacy receipt.
    pub(crate) const fn deck_ids_v2(&self) -> Option<[&'static str; 2]> {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(_) => None,
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                Some(receipt.deck_ids)
            }
        }
    }
}

/// Test-only coherent synthetic V2 receipt over a validated start: real
/// envelope, fixed inner digest, and internally consistent learner/opponent
/// count splits, so observer batteries can drive receipt-fact checks without
/// a full rollout.
#[cfg(test)]
pub(crate) fn envelope_probe_receipt_for_test_v2(
    episode_index: u64,
    pair_environment_seed: u64,
    deck_ids: &SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
) -> NativeTrainingTrajectoryReceiptV2 {
    let start = validate_start_v2(&NativeFullEpisodeTrajectoryStartV2 {
        episode_index,
        pair_environment_seed,
        deck_ids: deck_ids.clone(),
        deck_hashes,
        learner_seat: if episode_index.is_multiple_of(2) {
            PlayerSeatV1::P0
        } else {
            PlayerSeatV1::P1
        },
    })
    .expect("the probe start must validate");
    let inner = [0x3c_u8; 32];
    let outer = envelope_sha256_v2(&start, inner);
    NativeTrainingTrajectoryReceiptV2::from_environment_randomization_v2(
        NativeFullEpisodeTrajectoryReceiptV2 {
            episode_index: start.episode_index,
            pair_index: start.pair_index,
            pair_environment_seed: start.pair_environment_seed,
            deck_ids: start.deck_ids,
            deck_hashes: start.deck_hashes,
            learner_seat: start.learner_seat,
            inner_trajectory_sha256: inner,
            trajectory_sha256_v2: outer,
            policy_step_count: 3,
            physical_decision_count: 2,
            learner_policy_step_count: 2,
            opponent_policy_step_count: 1,
            learner_physical_decision_count: 1,
            opponent_physical_decision_count: 1,
        },
    )
}

/// Zero-learner sibling of [`envelope_probe_receipt_for_test_v2`]: the same
/// coherent probe with a zero learner split (policy 3 = 0 + 3, physical
/// 2 = 0 + 2), so a fresh private trainer observer that has seen zero
/// selected events admits its terminal through core grouping. Deleting a
/// thin receipt-validation callsite then deterministically changes each
/// receipt negative's outcome instead of hiding behind a later grouping
/// rejection.
#[cfg(test)]
pub(crate) fn zero_learner_envelope_probe_receipt_for_test_v2(
    episode_index: u64,
    pair_environment_seed: u64,
    deck_ids: &SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
) -> NativeTrainingTrajectoryReceiptV2 {
    let start = validate_start_v2(&NativeFullEpisodeTrajectoryStartV2 {
        episode_index,
        pair_environment_seed,
        deck_ids: deck_ids.clone(),
        deck_hashes,
        learner_seat: if episode_index.is_multiple_of(2) {
            PlayerSeatV1::P0
        } else {
            PlayerSeatV1::P1
        },
    })
    .expect("the probe start must validate");
    let inner = [0x3c_u8; 32];
    let outer = envelope_sha256_v2(&start, inner);
    NativeTrainingTrajectoryReceiptV2::from_environment_randomization_v2(
        NativeFullEpisodeTrajectoryReceiptV2 {
            episode_index: start.episode_index,
            pair_index: start.pair_index,
            pair_environment_seed: start.pair_environment_seed,
            deck_ids: start.deck_ids,
            deck_hashes: start.deck_hashes,
            learner_seat: start.learner_seat,
            inner_trajectory_sha256: inner,
            trajectory_sha256_v2: outer,
            policy_step_count: 3,
            physical_decision_count: 2,
            learner_policy_step_count: 0,
            opponent_policy_step_count: 3,
            learner_physical_decision_count: 0,
            opponent_physical_decision_count: 2,
        },
    )
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeV2ReceiptFactMutationForTestV2 {
    PairIndex,
    DeckId0,
    DeckId1,
    EpisodeIndex,
    PairRoot,
    DeckHash0,
    DeckHash1,
    LearnerSeat,
    PolicyStepCount,
    PhysicalDecisionCount,
    LearnerPolicyStepCount,
    LearnerPhysicalDecisionCount,
    OpponentPolicyStepCount,
    OpponentPhysicalDecisionCount,
}

#[cfg(test)]
impl NativeTrainingTrajectoryReceiptV2 {
    /// Test-only common-preserving variant flip: every common accessor of
    /// the returned receipt equals the original exactly (episode, seed,
    /// hashes, seat, compatibility digest, all six counts) while only the
    /// run-bound variant toggles. The V2 direction takes the catalog-static
    /// ordered deck IDs its variant requires; its outer digest is synthetic,
    /// which no common accessor exposes.
    pub(crate) fn variant_flipped_preserving_commons_for_test_v2(
        &self,
        deck_ids: [&'static str; 2],
    ) -> Self {
        match &self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(receipt) => {
                Self::from_environment_randomization_v2(NativeFullEpisodeTrajectoryReceiptV2 {
                    episode_index: receipt.episode_index,
                    pair_index: receipt.episode_index / 2,
                    pair_environment_seed: receipt.environment_seed,
                    deck_ids,
                    deck_hashes: receipt.deck_hashes,
                    learner_seat: receipt.learner_seat,
                    inner_trajectory_sha256: receipt.trajectory_sha256,
                    trajectory_sha256_v2: [0xEE; 32],
                    policy_step_count: receipt.policy_step_count,
                    physical_decision_count: receipt.physical_decision_count,
                    learner_policy_step_count: receipt.learner_policy_step_count,
                    opponent_policy_step_count: receipt.opponent_policy_step_count,
                    learner_physical_decision_count: receipt.learner_physical_decision_count,
                    opponent_physical_decision_count: receipt.opponent_physical_decision_count,
                })
            }
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                Self::from_legacy_v1(NativeFullEpisodeTrajectoryReceiptV1 {
                    episode_index: receipt.episode_index,
                    environment_seed: receipt.pair_environment_seed,
                    deck_hashes: receipt.deck_hashes,
                    learner_seat: receipt.learner_seat,
                    trajectory_sha256: receipt.inner_trajectory_sha256,
                    policy_step_count: receipt.policy_step_count,
                    physical_decision_count: receipt.physical_decision_count,
                    learner_policy_step_count: receipt.learner_policy_step_count,
                    opponent_policy_step_count: receipt.opponent_policy_step_count,
                    learner_physical_decision_count: receipt.learner_physical_decision_count,
                    opponent_physical_decision_count: receipt.opponent_physical_decision_count,
                })
            }
        }
    }

    /// The full common-accessor projection, for exact equality proofs around
    /// variant flips.
    #[allow(clippy::type_complexity)]
    pub(crate) fn common_accessor_tuple_for_test_v2(
        &self,
    ) -> (
        u64,
        u64,
        SessionDeckHashesV1,
        PlayerSeatV1,
        [u8; 32],
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
    ) {
        (
            self.episode_index(),
            self.environment_seed(),
            self.deck_hashes(),
            self.learner_seat(),
            self.trajectory_sha256(),
            self.policy_step_count(),
            self.physical_decision_count(),
            self.learner_policy_step_count(),
            self.opponent_policy_step_count(),
            self.learner_physical_decision_count(),
            self.opponent_physical_decision_count(),
        )
    }
}

#[cfg(test)]
impl NativeTrainingTrajectoryReceiptV2 {
    /// Test-only V2-fact corruption seam so Store and observer batteries can
    /// prove each bound V2 fact is validated, without widening the public
    /// wrapper surface. Panics on a legacy receipt.
    pub(crate) fn mutate_environment_fact_for_test_v2(
        &mut self,
        mutation: NativeV2ReceiptFactMutationForTestV2,
    ) {
        match &mut self.inner {
            NativeRunBoundFullEpisodeReceiptV2::LegacyV1(_) => {
                panic!("V2 fact mutation requires a V2 receipt")
            }
            NativeRunBoundFullEpisodeReceiptV2::EnvironmentRandomizationV2(receipt) => {
                match mutation {
                    NativeV2ReceiptFactMutationForTestV2::PairIndex => receipt.pair_index ^= 1,
                    NativeV2ReceiptFactMutationForTestV2::DeckId0 => {
                        receipt.deck_ids[0] = if receipt.deck_ids[0] == "Rally" {
                            "Burn"
                        } else {
                            "Rally"
                        };
                    }
                    NativeV2ReceiptFactMutationForTestV2::DeckId1 => {
                        receipt.deck_ids[1] = if receipt.deck_ids[1] == "Rally" {
                            "Burn"
                        } else {
                            "Rally"
                        };
                    }
                    NativeV2ReceiptFactMutationForTestV2::EpisodeIndex => {
                        receipt.episode_index ^= 1
                    }
                    NativeV2ReceiptFactMutationForTestV2::PairRoot => {
                        receipt.pair_environment_seed ^= 1
                    }
                    NativeV2ReceiptFactMutationForTestV2::DeckHash0 => receipt.deck_hashes[0] ^= 1,
                    NativeV2ReceiptFactMutationForTestV2::DeckHash1 => receipt.deck_hashes[1] ^= 1,
                    NativeV2ReceiptFactMutationForTestV2::LearnerSeat => {
                        receipt.learner_seat = match receipt.learner_seat {
                            PlayerSeatV1::P0 => PlayerSeatV1::P1,
                            PlayerSeatV1::P1 => PlayerSeatV1::P0,
                        }
                    }
                    NativeV2ReceiptFactMutationForTestV2::PolicyStepCount => {
                        receipt.policy_step_count += 1
                    }
                    NativeV2ReceiptFactMutationForTestV2::PhysicalDecisionCount => {
                        receipt.physical_decision_count += 1
                    }
                    NativeV2ReceiptFactMutationForTestV2::LearnerPolicyStepCount => {
                        receipt.learner_policy_step_count += 1
                    }
                    NativeV2ReceiptFactMutationForTestV2::LearnerPhysicalDecisionCount => {
                        receipt.learner_physical_decision_count += 1
                    }
                    NativeV2ReceiptFactMutationForTestV2::OpponentPolicyStepCount => {
                        receipt.opponent_policy_step_count += 1
                    }
                    NativeV2ReceiptFactMutationForTestV2::OpponentPhysicalDecisionCount => {
                        receipt.opponent_physical_decision_count += 1
                    }
                }
            }
        }
    }
}

// --------------------------------------------- run-bound accumulator dispatch

/// The closed crate-private run-bound accumulator. Exactly one variant per
/// run trajectory contract; preflight, accepted-row recording, and natural
/// finish dispatch exhaustively with no wildcard, and the finish of each
/// variant seals its receipt into the matching opaque wrapper variant. Legacy
/// native execution reaches the frozen V1 accumulator exactly as before.
pub(crate) enum NativeRunBoundFullEpisodeAccumulatorV2 {
    LegacyV1(NativeFullEpisodeTrajectoryAccumulatorV1),
    EnvironmentRandomizationV2(NativeFullEpisodeTrajectoryAccumulatorV2),
}

impl NativeRunBoundFullEpisodeAccumulatorV2 {
    pub(crate) fn new_legacy_v1(
        episode_index: u64,
        environment_seed: u64,
        deck_ids: &SessionDeckIdsV1,
        deck_hashes: SessionDeckHashesV1,
        learner_seat: PlayerSeatV1,
    ) -> Result<Self, NativeFullEpisodeTrajectoryErrorV2> {
        NativeFullEpisodeTrajectoryAccumulatorV1::new_v1(
            episode_index,
            environment_seed,
            deck_ids,
            deck_hashes,
            learner_seat,
        )
        .map(Self::LegacyV1)
        .map_err(map_inner_error_v2)
    }

    pub(crate) fn new_environment_randomization_v2(
        start: &NativeFullEpisodeTrajectoryStartV2,
    ) -> Result<Self, NativeFullEpisodeTrajectoryErrorV2> {
        NativeFullEpisodeTrajectoryAccumulatorV2::new_v2(start)
            .map(Self::EnvironmentRandomizationV2)
    }

    pub(crate) fn preflight_candidate(
        &self,
        row: NativeFullEpisodeTrajectoryDecisionRowV1,
    ) -> Result<(), NativeFullEpisodeTrajectoryErrorV2> {
        match self {
            Self::LegacyV1(inner) => inner
                .preflight_candidate_v1(row)
                .map_err(map_inner_error_v2),
            Self::EnvironmentRandomizationV2(inner) => inner.preflight_candidate_v2(row),
        }
    }

    pub(crate) fn record_accepted(
        &mut self,
        row: NativeFullEpisodeTrajectoryDecisionRowV1,
    ) -> Result<(), NativeFullEpisodeTrajectoryErrorV2> {
        match self {
            Self::LegacyV1(inner) => inner.record_accepted_v1(row).map_err(map_inner_error_v2),
            Self::EnvironmentRandomizationV2(inner) => inner.record_accepted_v2(row),
        }
    }

    pub(crate) fn finish_natural(
        self,
        terminal: AsyncRolloutTerminalV1,
        terminal_deck_hashes: SessionDeckHashesV1,
    ) -> Result<NativeTrainingTrajectoryReceiptV2, NativeFullEpisodeTrajectoryErrorV2> {
        match self {
            Self::LegacyV1(inner) => inner
                .finish_natural_v1(terminal, terminal_deck_hashes)
                .map(NativeTrainingTrajectoryReceiptV2::from_legacy_v1)
                .map_err(map_inner_error_v2),
            Self::EnvironmentRandomizationV2(inner) => inner
                .finish_natural_v2(terminal, terminal_deck_hashes)
                .map(NativeTrainingTrajectoryReceiptV2::from_environment_randomization_v2),
        }
    }
}

// ----------------------------------------- window preflight authority (V2)

/// Move-only consumed preflight authority for one complete even/odd episode
/// window under the environment randomization V2 contract.
///
/// Deliberately neither `Clone` nor `Copy`, with no public surface: the only
/// way to obtain one is [`preflight_native_environment_window_v2`], which
/// validates every pair of the window through the frozen pair validator, and
/// the only consumer moves it into the rollout entry, where a missing,
/// foreign, or window-mismatched authority rejects before result reservation,
/// channel creation, worker spawn, or reset. Its existence is itself the mode
/// binding: no legacy path constructs one and no V2 worker or reset is
/// reachable without consuming one.
pub(crate) struct NativeEnvironmentWindowPreflightAuthorityV2 {
    base_seed: u64,
    first_episode_index: u64,
    episode_count: u64,
    deck_ids: [&'static str; 2],
    deck_hashes: SessionDeckHashesV1,
}

impl NativeEnvironmentWindowPreflightAuthorityV2 {
    /// Exact window/config binding check, run by the consumer immediately
    /// before the authority is consumed. The supplied deck IDs must resolve in
    /// the runtime catalog to exactly the bindings this authority proved.
    pub(crate) fn matches_window_v2(
        &self,
        base_seed: u64,
        first_episode_index: u64,
        episode_count: u64,
        deck_ids: &SessionDeckIdsV1,
    ) -> bool {
        self.base_seed == base_seed
            && self.first_episode_index == first_episode_index
            && self.episode_count == episode_count
            && self.deck_ids[0] == deck_ids[0]
            && self.deck_ids[1] == deck_ids[1]
            && resolve_physical_deck_v2(&deck_ids[0], self.deck_hashes[0]).is_ok()
            && resolve_physical_deck_v2(&deck_ids[1], self.deck_hashes[1]).is_ok()
    }
}

/// Test-only pair corruption applied inside the window preflight, so ordering
/// tests can prove that one corrupted interior pair rejects the whole window
/// before any downstream construction. Keyed by the pair offset within the
/// window, not the absolute pair index.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeWindowPairCorruptionForTestV2 {
    EpisodeIndexDrift,
    PairRootDrift,
    LearnerSeatSwap,
    DeckHashDrift,
    DeckIdInvalid,
}

#[cfg(test)]
thread_local! {
    static WINDOW_PAIR_CORRUPTION_FOR_TEST_V2: std::cell::Cell<
        Option<(u64, NativeWindowPairCorruptionForTestV2)>,
    > = const { std::cell::Cell::new(None) };
}

/// RAII arming guard: drop restores the exact prior thread-local value on
/// every exit path, including panics and rejects that never reached the
/// target pair, so nested or caught-panic scopes cannot clobber each other
/// and an armed mutation can never poison a later test on the same thread.
#[cfg(test)]
pub(crate) struct NativeWindowPairCorruptionGuardV2 {
    saved: Option<(u64, NativeWindowPairCorruptionForTestV2)>,
    thread_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(test)]
impl Drop for NativeWindowPairCorruptionGuardV2 {
    fn drop(&mut self) {
        WINDOW_PAIR_CORRUPTION_FOR_TEST_V2.with(|cell| cell.set(self.saved));
    }
}

#[cfg(test)]
pub(crate) fn arm_window_pair_corruption_for_test_v2(
    window_pair_offset: u64,
    corruption: NativeWindowPairCorruptionForTestV2,
) -> NativeWindowPairCorruptionGuardV2 {
    let saved = WINDOW_PAIR_CORRUPTION_FOR_TEST_V2
        .with(|cell| cell.replace(Some((window_pair_offset, corruption))));
    NativeWindowPairCorruptionGuardV2 {
        saved,
        thread_bound: std::marker::PhantomData,
    }
}

#[cfg(test)]
fn apply_window_pair_corruption_for_test_v2(
    window_pair_offset: u64,
    even_start: &mut NativeFullEpisodeTrajectoryStartV2,
    odd_start: &mut NativeFullEpisodeTrajectoryStartV2,
) {
    WINDOW_PAIR_CORRUPTION_FOR_TEST_V2.with(|cell| {
        if let Some((target, corruption)) = cell.get() {
            if target == window_pair_offset {
                cell.set(None);
                match corruption {
                    NativeWindowPairCorruptionForTestV2::EpisodeIndexDrift => {
                        odd_start.episode_index ^= 0b10;
                    }
                    NativeWindowPairCorruptionForTestV2::PairRootDrift => {
                        even_start.pair_environment_seed ^= 1;
                        odd_start.pair_environment_seed ^= 1;
                    }
                    NativeWindowPairCorruptionForTestV2::LearnerSeatSwap => {
                        even_start.learner_seat = PlayerSeatV1::P1;
                        odd_start.learner_seat = PlayerSeatV1::P0;
                    }
                    NativeWindowPairCorruptionForTestV2::DeckHashDrift => {
                        odd_start.deck_hashes[0] ^= 1;
                        odd_start.deck_hashes[1] ^= 1;
                    }
                    NativeWindowPairCorruptionForTestV2::DeckIdInvalid => {
                        odd_start.deck_ids[0] = String::new();
                    }
                }
            }
        }
    });
}

/// Validates one complete even/odd episode window for the environment
/// randomization V2 contract and returns the consumed preflight authority.
///
/// Every pair of the window, interiors included, is validated through
/// [`validate_native_full_episode_trajectory_pair_v2`]: individual start
/// validity, exact `2k`/`2k+1` pairing, P0/P1 learner parity, the shared
/// full-width pair root derived through the frozen trainer schedule, and
/// seat-swap-stable ordered physical deck bindings. The window itself must be
/// pair-aligned: an odd first episode, an odd count, or an empty count is
/// outside the pair domain.
pub(crate) fn preflight_native_environment_window_v2(
    base_seed: u64,
    first_episode_index: u64,
    episode_count: u64,
    deck_ids: &SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
) -> Result<NativeEnvironmentWindowPreflightAuthorityV2, NativeFullEpisodeTrajectoryErrorV2> {
    if episode_count == 0
        || !first_episode_index.is_multiple_of(2)
        || !episode_count.is_multiple_of(2)
    {
        return Err(NativeFullEpisodeTrajectoryErrorV2::PairIndexOutsideEpisodeDomain);
    }
    let end_episode_exclusive = first_episode_index
        .checked_add(episode_count)
        .ok_or(NativeFullEpisodeTrajectoryErrorV2::ScheduleIntegerOutsideU63)?;
    if end_episode_exclusive - 1 > U63_MAX_V2 {
        return Err(NativeFullEpisodeTrajectoryErrorV2::ScheduleIntegerOutsideU63);
    }
    let first_pair_index = first_episode_index / 2;
    let pair_count = episode_count / 2;
    let mut proven_deck_ids: Option<[&'static str; 2]> = None;
    for pair_offset in 0..pair_count {
        let pair_index = first_pair_index
            .checked_add(pair_offset)
            .ok_or(NativeFullEpisodeTrajectoryErrorV2::PairIndexOutsideEpisodeDomain)?;
        let even_episode_index = pair_index
            .checked_mul(2)
            .ok_or(NativeFullEpisodeTrajectoryErrorV2::PairIndexOutsideEpisodeDomain)?;
        let odd_episode_index = even_episode_index
            .checked_add(1)
            .ok_or(NativeFullEpisodeTrajectoryErrorV2::PairIndexOutsideEpisodeDomain)?;
        // The shared root comes from the frozen trainer schedule; the pair
        // validator independently rederives and compares it.
        let schedule = native_trainer_episode_schedule_v1(base_seed, even_episode_index)
            .map_err(|_| NativeFullEpisodeTrajectoryErrorV2::ScheduleIntegerOutsideU63)?;
        let even_start = NativeFullEpisodeTrajectoryStartV2 {
            episode_index: even_episode_index,
            pair_environment_seed: schedule.environment_seed,
            deck_ids: deck_ids.clone(),
            deck_hashes,
            learner_seat: PlayerSeatV1::P0,
        };
        let odd_start = NativeFullEpisodeTrajectoryStartV2 {
            episode_index: odd_episode_index,
            pair_environment_seed: schedule.environment_seed,
            deck_ids: deck_ids.clone(),
            deck_hashes,
            learner_seat: PlayerSeatV1::P1,
        };
        #[cfg(test)]
        let (even_start, odd_start) = {
            let mut even_start = even_start;
            let mut odd_start = odd_start;
            apply_window_pair_corruption_for_test_v2(pair_offset, &mut even_start, &mut odd_start);
            (even_start, odd_start)
        };
        let binding = validate_native_full_episode_trajectory_pair_v2(
            base_seed,
            pair_index,
            &even_start,
            &odd_start,
        )?;
        proven_deck_ids = Some(binding.deck_ids);
    }
    let proven_deck_ids =
        proven_deck_ids.ok_or(NativeFullEpisodeTrajectoryErrorV2::PairIndexOutsideEpisodeDomain)?;
    Ok(NativeEnvironmentWindowPreflightAuthorityV2 {
        base_seed,
        first_episode_index,
        episode_count,
        deck_ids: proven_deck_ids,
        deck_hashes,
    })
}

/// Test-only INDEPENDENT 34-atom framing: a from-scratch reimplementation of
/// the frozen envelope (its own atom writer, tag order, and payload
/// encoding over the module's frozen authority expectations), deliberately
/// not delegating to `envelope_sha256_v2`. Higher-layer genuine-execution
/// oracles compare live receipt outer digests against this helper, and the
/// unit oracle proves this helper equals the production envelope, so the two
/// implementations check each other rather than one checking itself.
#[cfg(test)]
pub(crate) fn independent_envelope_sha256_for_test_v2(
    start: &NativeFullEpisodeTrajectoryValidatedStartV2,
    inner_trajectory_sha256: [u8; 32],
) -> [u8; 32] {
    fn atom(bytes: &mut Vec<u8>, tag: &str, payload: &[u8]) {
        bytes.extend_from_slice(&u32::try_from(tag.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(tag.as_bytes());
        bytes.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(payload);
    }
    let seat_byte = match start.learner_seat {
        PlayerSeatV1::P0 => 0_u8,
        PlayerSeatV1::P1 => 1,
    };
    let mut framed = Vec::new();
    atom(
        &mut framed,
        "domain",
        NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "inner_trajectory_identity_utf8",
        EXPECTED_INNER_TRAJECTORY_IDENTITY_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "inner_trajectory_goldens_schema_utf8",
        EXPECTED_INNER_TRAJECTORY_GOLDENS_SCHEMA_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "inner_trajectory_goldens_generator_identity_utf8",
        EXPECTED_INNER_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "inner_trajectory_golden_stream_identity_utf8",
        EXPECTED_INNER_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "inner_trajectory_goldens_file_sha256_raw32",
        &INNER_GOLDENS_FILE_SHA256_RAW32_V2,
    );
    atom(
        &mut framed,
        "inner_trajectory_golden_stream_sha256_raw32",
        &INNER_GOLDEN_STREAM_SHA256_RAW32_V2,
    );
    atom(
        &mut framed,
        "environment_randomization_identity_utf8",
        EXPECTED_ENVIRONMENT_RANDOMIZATION_IDENTITY_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "environment_randomization_namespace_utf8",
        EXPECTED_ENVIRONMENT_RANDOMIZATION_NAMESPACE_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "environment_randomization_kdf_goldens_schema_utf8",
        EXPECTED_ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_SCHEMA_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "environment_randomization_kdf_goldens_file_sha256_raw32",
        &ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_FILE_SHA256_RAW32_V2,
    );
    atom(
        &mut framed,
        "reset_trajectory_goldens_schema_utf8",
        EXPECTED_RESET_TRAJECTORY_GOLDENS_SCHEMA_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "reset_trajectory_generator_identity_utf8",
        EXPECTED_RESET_TRAJECTORY_GENERATOR_IDENTITY_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "reset_trajectory_physical_projection_identity_utf8",
        EXPECTED_RESET_TRAJECTORY_PHYSICAL_PROJECTION_IDENTITY_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "reset_trajectory_vector_stream_identity_utf8",
        EXPECTED_RESET_TRAJECTORY_VECTOR_STREAM_IDENTITY_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "reset_trajectory_goldens_file_sha256_raw32",
        &RESET_TRAJECTORY_GOLDENS_FILE_SHA256_RAW32_V2,
    );
    atom(
        &mut framed,
        "reset_trajectory_vector_stream_sha256_raw32",
        &RESET_TRAJECTORY_VECTOR_STREAM_SHA256_RAW32_V2,
    );
    atom(
        &mut framed,
        "trainer_schedule_identity_utf8",
        EXPECTED_TRAINER_SCHEDULE_IDENTITY_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "trainer_seed_version_utf8",
        EXPECTED_TRAINER_SEED_VERSION_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "trainer_schedule_goldens_file_sha256_raw32",
        &TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_RAW32_V2,
    );
    atom(
        &mut framed,
        "runtime_deck_catalog_schema_utf8",
        EXPECTED_RUNTIME_DECK_CATALOG_SCHEMA_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "runtime_deck_protocol_utf8",
        EXPECTED_RUNTIME_DECK_PROTOCOL_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "runtime_deck_materialization_protocol_utf8",
        EXPECTED_RUNTIME_DECK_MATERIALIZATION_PROTOCOL_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "runtime_deck_hash_algorithm_utf8",
        EXPECTED_RUNTIME_DECK_HASH_ALGORITHM_V2.as_bytes(),
    );
    atom(
        &mut framed,
        "runtime_deck_catalog_file_sha256_raw32",
        &RUNTIME_DECK_CATALOG_FILE_SHA256_RAW32_V2,
    );
    atom(
        &mut framed,
        "episode_index_u64be",
        &start.episode_index.to_be_bytes(),
    );
    atom(
        &mut framed,
        "pair_index_u64be",
        &start.pair_index.to_be_bytes(),
    );
    atom(
        &mut framed,
        "pair_environment_seed_u64be",
        &start.pair_environment_seed.to_be_bytes(),
    );
    atom(&mut framed, "deck_p0_id_utf8", start.deck_ids[0].as_bytes());
    atom(
        &mut framed,
        "deck_p0_hash_u64be",
        &start.deck_hashes[0].to_be_bytes(),
    );
    atom(&mut framed, "deck_p1_id_utf8", start.deck_ids[1].as_bytes());
    atom(
        &mut framed,
        "deck_p1_hash_u64be",
        &start.deck_hashes[1].to_be_bytes(),
    );
    atom(&mut framed, "learner_seat_u8", &[seat_byte]);
    atom(
        &mut framed,
        "inner_trajectory_sha256_raw32",
        &inner_trajectory_sha256,
    );
    Sha256::digest(&framed).into()
}

#[cfg(test)]
mod live_c2_tests {
    use super::*;

    fn rally_start_v2(
        episode_index: u64,
        pair_environment_seed: u64,
    ) -> NativeFullEpisodeTrajectoryStartV2 {
        let rally = runtime_deck_by_id("Rally").unwrap();
        NativeFullEpisodeTrajectoryStartV2 {
            episode_index,
            pair_environment_seed,
            deck_ids: ["Rally".to_owned(), "Rally".to_owned()],
            deck_hashes: [rally.runtime_deck_hash; 2],
            learner_seat: if episode_index.is_multiple_of(2) {
                PlayerSeatV1::P0
            } else {
                PlayerSeatV1::P1
            },
        }
    }

    /// Independent 34-atom framing oracle: the production envelope must
    /// equal the from-scratch test framing helper for genuinely distinct
    /// Rally/Burn bindings under both learner seats, so the deck order atoms
    /// and the learner-seat byte are non-vacuous, and neither implementation
    /// is checked against itself.
    #[test]
    fn envelope_matches_an_independent_thirty_four_atom_framing_oracle() {
        let rally = runtime_deck_by_id("Rally").unwrap();
        let burn = runtime_deck_by_id("Burn").unwrap();
        for episode_index in [6_u64, 7] {
            let start = validate_start_v2(&NativeFullEpisodeTrajectoryStartV2 {
                episode_index,
                pair_environment_seed: 0x0123_4567_89ab_cdef,
                deck_ids: ["Rally".to_owned(), "Burn".to_owned()],
                deck_hashes: [rally.runtime_deck_hash, burn.runtime_deck_hash],
                learner_seat: if episode_index.is_multiple_of(2) {
                    PlayerSeatV1::P0
                } else {
                    PlayerSeatV1::P1
                },
            })
            .unwrap();
            let inner = [0x5a_u8; 32];
            let oracle = independent_envelope_sha256_for_test_v2(&start, inner);
            assert_eq!(envelope_sha256_v2(&start, inner), oracle);
            assert_ne!(
                oracle, inner,
                "the outer envelope must not equal its inner digest"
            );
        }
    }

    /// Window preflight: a valid pair-aligned window over the frozen schedule
    /// admits and binds; every corruption kind rejects at its exact interior
    /// pair; unaligned windows never enter the pair domain.
    #[test]
    fn window_preflight_validates_every_pair_and_each_corruption_kind_rejects() {
        let rally = runtime_deck_by_id("Rally").unwrap();
        let deck_ids = ["Rally".to_owned(), "Rally".to_owned()];
        let deck_hashes = [rally.runtime_deck_hash; 2];
        let base_seed = 71_501_u64;

        let authority =
            preflight_native_environment_window_v2(base_seed, 0, 8, &deck_ids, deck_hashes)
                .expect("a valid K=2, S=4 window validates every pair");
        assert!(authority.matches_window_v2(base_seed, 0, 8, &deck_ids));
        assert!(!authority.matches_window_v2(base_seed ^ 1, 0, 8, &deck_ids));
        assert!(!authority.matches_window_v2(base_seed, 2, 8, &deck_ids));
        assert!(!authority.matches_window_v2(base_seed, 0, 6, &deck_ids));
        let foreign_decks = ["Burn".to_owned(), "Rally".to_owned()];
        assert!(!authority.matches_window_v2(base_seed, 0, 8, &foreign_decks));

        for (corruption, expected) in [
            (
                NativeWindowPairCorruptionForTestV2::EpisodeIndexDrift,
                NativeFullEpisodeTrajectoryErrorV2::PairEpisodeIndexMismatch,
            ),
            (
                NativeWindowPairCorruptionForTestV2::PairRootDrift,
                NativeFullEpisodeTrajectoryErrorV2::PairEnvironmentSeedMismatch,
            ),
            (
                NativeWindowPairCorruptionForTestV2::LearnerSeatSwap,
                NativeFullEpisodeTrajectoryErrorV2::LearnerSeatRuleMismatch,
            ),
            (
                NativeWindowPairCorruptionForTestV2::DeckHashDrift,
                NativeFullEpisodeTrajectoryErrorV2::RuntimeDeckHashMismatch,
            ),
            (
                NativeWindowPairCorruptionForTestV2::DeckIdInvalid,
                NativeFullEpisodeTrajectoryErrorV2::InvalidDeckId,
            ),
        ] {
            for pair_offset in 0..4_u64 {
                let _guard = arm_window_pair_corruption_for_test_v2(pair_offset, corruption);
                assert_eq!(
                    preflight_native_environment_window_v2(base_seed, 0, 8, &deck_ids, deck_hashes)
                        .map(|_| ())
                        .unwrap_err(),
                    expected,
                    "corruption {corruption:?} at pair offset {pair_offset} must reject"
                );
            }
        }
        // The RAII guards above disarmed on every exit; the same window
        // validates again.
        preflight_native_environment_window_v2(base_seed, 0, 8, &deck_ids, deck_hashes)
            .expect("disarmed corruption must leave no residue");

        for (first, count) in [(1_u64, 2_u64), (0, 3), (0, 0)] {
            assert_eq!(
                preflight_native_environment_window_v2(
                    base_seed,
                    first,
                    count,
                    &deck_ids,
                    deck_hashes
                )
                .map(|_| ())
                .unwrap_err(),
                NativeFullEpisodeTrajectoryErrorV2::PairIndexOutsideEpisodeDomain,
                "window ({first}, {count}) is outside the pair domain"
            );
        }
    }

    /// Live-authority guard sweep: every one of the twenty-four owner slots,
    /// armed as a synthetic live/expected mismatch inside the production
    /// comparison, rejects `AuthorityMismatch`, and the disarmed guard
    /// succeeds, so an always-Ok guard body or a deleted comparison cannot
    /// survive.
    #[test]
    fn live_authority_guard_rejects_every_armed_slot_and_passes_disarmed() {
        assert_eq!(LIVE_AUTHORITY_CHECKS_V2.len(), 24);
        for slot in 0..LIVE_AUTHORITY_CHECKS_V2.len() {
            let _guard = arm_live_authority_mismatch_for_test_v2(slot);
            assert_eq!(
                guard_live_source_authorities_v2().unwrap_err(),
                NativeFullEpisodeTrajectoryErrorV2::AuthorityMismatch,
                "armed slot {slot} must reject"
            );
        }
        guard_live_source_authorities_v2().expect("the disarmed guard must pass");

        // The production check sequence equals an independently enumerated
        // (live, expected) sequence in the frozen atom order, so a
        // duplicated, dropped, or miswired slot cannot hide behind the
        // sweep.
        let independent: [(&str, &str); 24] = [
            (
                NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V1,
                EXPECTED_INNER_TRAJECTORY_IDENTITY_V2,
            ),
            (
                NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_SCHEMA_V1,
                EXPECTED_INNER_TRAJECTORY_GOLDENS_SCHEMA_V2,
            ),
            (
                NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V1,
                EXPECTED_INNER_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V2,
            ),
            (
                NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V1,
                EXPECTED_INNER_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V2,
            ),
            (
                NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_FILE_SHA256_V1,
                EXPECTED_INNER_TRAJECTORY_GOLDENS_FILE_SHA256_V2,
            ),
            (
                NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_SHA256_V1,
                EXPECTED_INNER_TRAJECTORY_GOLDEN_STREAM_SHA256_V2,
            ),
            (
                ENVIRONMENT_RANDOMIZATION_IDENTITY_V2,
                EXPECTED_ENVIRONMENT_RANDOMIZATION_IDENTITY_V2,
            ),
            (
                ENVIRONMENT_RANDOMIZATION_NAMESPACE_V2,
                EXPECTED_ENVIRONMENT_RANDOMIZATION_NAMESPACE_V2,
            ),
            (
                ENVIRONMENT_RANDOMIZATION_GOLDENS_SCHEMA_V1,
                EXPECTED_ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_SCHEMA_V2,
            ),
            (
                ENVIRONMENT_RANDOMIZATION_GOLDENS_SHA256_V1,
                EXPECTED_ENVIRONMENT_RANDOMIZATION_KDF_GOLDENS_FILE_SHA256_V2,
            ),
            (
                ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_SCHEMA_V1,
                EXPECTED_RESET_TRAJECTORY_GOLDENS_SCHEMA_V2,
            ),
            (
                ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GENERATOR_IDENTITY_V1,
                EXPECTED_RESET_TRAJECTORY_GENERATOR_IDENTITY_V2,
            ),
            (
                ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PHYSICAL_PROJECTION_IDENTITY_V1,
                EXPECTED_RESET_TRAJECTORY_PHYSICAL_PROJECTION_IDENTITY_V2,
            ),
            (
                ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PORTABLE_VECTOR_STREAM_IDENTITY_V1,
                EXPECTED_RESET_TRAJECTORY_VECTOR_STREAM_IDENTITY_V2,
            ),
            (
                ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_SHA256_V1,
                EXPECTED_RESET_TRAJECTORY_GOLDENS_FILE_SHA256_V2,
            ),
            (
                ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PORTABLE_VECTOR_STREAM_SHA256_V1,
                EXPECTED_RESET_TRAJECTORY_VECTOR_STREAM_SHA256_V2,
            ),
            (
                NATIVE_TRAINER_SCHEDULE_VERSION_V1,
                EXPECTED_TRAINER_SCHEDULE_IDENTITY_V2,
            ),
            (
                PYTHON_REFERENCE_SEED_VERSION_V1,
                EXPECTED_TRAINER_SEED_VERSION_V2,
            ),
            (
                NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1,
                EXPECTED_TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_V2,
            ),
            (
                RUNTIME_DECK_CATALOG_SCHEMA,
                EXPECTED_RUNTIME_DECK_CATALOG_SCHEMA_V2,
            ),
            (RUNTIME_DECK_PROTOCOL, EXPECTED_RUNTIME_DECK_PROTOCOL_V2),
            (
                RUNTIME_DECK_MATERIALIZATION_PROTOCOL,
                EXPECTED_RUNTIME_DECK_MATERIALIZATION_PROTOCOL_V2,
            ),
            (
                RUNTIME_DECK_HASH_ALGORITHM,
                EXPECTED_RUNTIME_DECK_HASH_ALGORITHM_V2,
            ),
            (
                RUNTIME_DECK_CATALOG_FILE_SHA256,
                EXPECTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2,
            ),
        ];
        assert_eq!(
            LIVE_AUTHORITY_CHECKS_V2, independent,
            "the production check sequence must equal the independent enumeration exactly"
        );
    }

    /// Guard-precedence sentinels at both production callsites: an armed
    /// authority mismatch outranks an otherwise-first start rejection, and
    /// the disarmed control returns exactly that start rejection, so a
    /// deleted `?` at either callsite cannot survive.
    #[test]
    fn armed_authority_mismatch_propagates_through_both_production_callsites() {
        // new_v2: episode 1<<63 is outside u63, so the disarmed error is the
        // start rejection while the armed error is the guard's.
        let mut invalid_episode_start = rally_start_v2(0, 7);
        invalid_episode_start.episode_index = 1_u64 << 63;
        invalid_episode_start.learner_seat = PlayerSeatV1::P0;
        {
            let _guard = arm_live_authority_mismatch_for_test_v2(0);
            assert_eq!(
                NativeFullEpisodeTrajectoryAccumulatorV2::new_v2(&invalid_episode_start)
                    .map(|_| ())
                    .unwrap_err(),
                NativeFullEpisodeTrajectoryErrorV2::AuthorityMismatch
            );
        }
        assert_eq!(
            NativeFullEpisodeTrajectoryAccumulatorV2::new_v2(&invalid_episode_start)
                .map(|_| ())
                .unwrap_err(),
            NativeFullEpisodeTrajectoryErrorV2::EpisodeIndexOutsideU63
        );

        // Pair validation: base and pair bounds stay valid (bound failures
        // deliberately precede the guard), while the even component start is
        // invalid, so disarmed returns the component-start error.
        let mut invalid_even = rally_start_v2(0, 7);
        invalid_even.deck_ids[0] = String::new();
        let odd = rally_start_v2(1, 7);
        {
            let _guard = arm_live_authority_mismatch_for_test_v2(23);
            assert_eq!(
                validate_native_full_episode_trajectory_pair_v2(71_501, 0, &invalid_even, &odd)
                    .map(|_| ())
                    .unwrap_err(),
                NativeFullEpisodeTrajectoryErrorV2::AuthorityMismatch
            );
        }
        assert_eq!(
            validate_native_full_episode_trajectory_pair_v2(71_501, 0, &invalid_even, &odd)
                .map(|_| ())
                .unwrap_err(),
            NativeFullEpisodeTrajectoryErrorV2::InvalidDeckId
        );
    }

    /// Wrapper semantics over both variants, built from crate-private
    /// constructors: common accessors agree with the underlying receipts, the
    /// compatibility digest is the V1 digest or the inner V1 digest, V2-only
    /// projections exist exactly for the V2 variant, and the custom Debug
    /// never leaks pair or deck-binding facts.
    #[test]
    fn wrapper_projects_common_facts_and_seals_v2_only_facts() {
        let legacy_receipt = NativeFullEpisodeTrajectoryReceiptV1 {
            episode_index: 4,
            environment_seed: 9_001,
            deck_hashes: [11, 22],
            learner_seat: PlayerSeatV1::P0,
            trajectory_sha256: [0xaa; 32],
            policy_step_count: 10,
            physical_decision_count: 6,
            learner_policy_step_count: 7,
            opponent_policy_step_count: 3,
            learner_physical_decision_count: 4,
            opponent_physical_decision_count: 2,
        };
        let wrapped_legacy = NativeTrainingTrajectoryReceiptV2::from_legacy_v1(legacy_receipt);
        assert!(!wrapped_legacy.is_environment_randomization_v2());
        assert_eq!(wrapped_legacy.episode_index(), 4);
        assert_eq!(wrapped_legacy.environment_seed(), 9_001);
        assert_eq!(wrapped_legacy.deck_hashes(), [11, 22]);
        assert_eq!(wrapped_legacy.trajectory_sha256(), [0xaa; 32]);
        assert_eq!(wrapped_legacy.outer_trajectory_sha256_v2(), None);
        assert_eq!(wrapped_legacy.pair_index_v2(), None);
        assert_eq!(wrapped_legacy.deck_ids_v2(), None);
        assert_eq!(
            wrapped_legacy.legacy_v1_view(),
            Some(&legacy_receipt),
            "the optional legacy view is the exact V1 receipt"
        );

        let start = validate_start_v2(&rally_start_v2(6, 12_345)).unwrap();
        let inner = [0x17; 32];
        let outer = envelope_sha256_v2(&start, inner);
        let v2_receipt = NativeFullEpisodeTrajectoryReceiptV2 {
            episode_index: start.episode_index,
            pair_index: start.pair_index,
            pair_environment_seed: start.pair_environment_seed,
            deck_ids: start.deck_ids,
            deck_hashes: start.deck_hashes,
            learner_seat: start.learner_seat,
            inner_trajectory_sha256: inner,
            trajectory_sha256_v2: outer,
            policy_step_count: 9,
            physical_decision_count: 5,
            learner_policy_step_count: 6,
            opponent_policy_step_count: 3,
            learner_physical_decision_count: 3,
            opponent_physical_decision_count: 2,
        };
        let wrapped_v2 =
            NativeTrainingTrajectoryReceiptV2::from_environment_randomization_v2(v2_receipt);
        assert!(wrapped_v2.is_environment_randomization_v2());
        assert_eq!(wrapped_v2.episode_index(), 6);
        assert_eq!(wrapped_v2.environment_seed(), 12_345);
        assert_eq!(
            wrapped_v2.trajectory_sha256(),
            inner,
            "the compatibility digest is the inner V1 digest"
        );
        assert_eq!(wrapped_v2.outer_trajectory_sha256_v2(), Some(outer));
        assert_ne!(inner, outer);
        assert_eq!(wrapped_v2.pair_index_v2(), Some(3));
        assert_eq!(wrapped_v2.deck_ids_v2(), Some(start.deck_ids));
        assert_eq!(wrapped_v2.legacy_v1_view(), None);

        let debug = format!("{wrapped_v2:?}");
        assert!(
            !debug.contains("pair_index"),
            "Debug must not leak pair facts"
        );
        assert!(
            !debug.contains("deck_ids"),
            "Debug must not leak deck bindings"
        );
        assert!(
            !debug.contains("EnvironmentRandomizationV2") && !debug.contains("LegacyV1"),
            "Debug must not leak the private variant"
        );
    }
}

#[cfg(test)]
#[path = "native_full_episode_trajectory_v2_goldens.rs"]
mod portable_goldens;
