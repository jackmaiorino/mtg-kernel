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
//! V1 bytes, APIs, goldens, and consumers are unchanged by this module, and
//! nothing here is activated at runtime yet.

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
    NativeFullEpisodeTrajectoryErrorV1, NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_FILE_SHA256_V1,
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
    "e6cfffe080c349ceca82ddfc6504fb61801ac07cc3e1ae57a80345296f7ec45b";
pub(crate) const NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_SHA256_V2: &str =
    "19171ada77ecd142ac458365563f6e65ad9f5ba352625c77a121ce0d00bb537f";

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
    "ab002901a598d40732d39f9b0f21abaa2b7445e63b1c14d45a44b7900f6b739b";
const EXPECTED_RESET_TRAJECTORY_VECTOR_STREAM_SHA256_V2: &str =
    "15d312141f8d96f079684dd64b58b5bab803086a78ac9687e3c14aab91e0a3c9";

const EXPECTED_TRAINER_SCHEDULE_IDENTITY_V2: &str = "mtg-kernel-native-trainer-schedule-sha256-v1";
const EXPECTED_TRAINER_SEED_VERSION_V2: &str = "kernel-python-rl-trainer-sha256-v2";
const EXPECTED_TRAINER_SCHEDULE_GOLDENS_FILE_SHA256_V2: &str =
    "6b2e1edbbe49b4e02f98794f9057f5c2bb8e3079d2ba8cb3e2a4b9ea6c34867c";

const EXPECTED_RUNTIME_DECK_CATALOG_SCHEMA_V2: &str = "kernel_runtime_decks/v1";
const EXPECTED_RUNTIME_DECK_PROTOCOL_V2: &str = "canonical-mainboard-bo1/v1";
const EXPECTED_RUNTIME_DECK_MATERIALIZATION_PROTOCOL_V2: &str =
    "xmage_xml_row_then_copy_ordinal/v1";
const EXPECTED_RUNTIME_DECK_HASH_ALGORITHM_V2: &str = "fnv1a64-serde-json-u16-array/v1";
const EXPECTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2: &str =
    "5ea19e8a08f0e9c9657e9a6a90382329785f27eeabbbe066e80e7025e8ee62c0";

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
    if LIVE_AUTHORITY_CHECKS_V2
        .iter()
        .any(|&(live, expected)| live != expected)
    {
        return Err(NativeFullEpisodeTrajectoryErrorV2::AuthorityMismatch);
    }
    Ok(())
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

#[cfg(test)]
#[path = "native_full_episode_trajectory_v2_goldens.rs"]
mod portable_goldens;
