//! Portable environment-randomization-v2 reset physical-trajectory golden
//! proof. Evidence only: this module activates no runtime consumer.
//!
//! The portable reset boundary is the state immediately after the fourteen
//! alternating opening draws and before session policy advance. The embedded
//! artifact carries only the physical card-definition projection of that
//! boundary: runtime card-definition ids, the mandatory zero-based
//! source-copy-index permutation, the projected card-id permutation, hands,
//! remaining libraries, the fourteen draw records, and the paired-role
//! learner-swap binding. It deliberately contains no Rust `ObjectId`, no
//! serialized `GameState`, no diagnostic or policy/core hash, and no JSONL
//! response. Those Rust-only pins stay separate and unchanged.
//!
//! This module does not claim byte-complete private session equality. The
//! unchanged same-file `rl_session` V2 reset tests already prove that field by
//! field, and the unchanged `rl_contract` state/diagnostic pins bound the
//! serialized form. Treat the portable artifact, those private tests, and
//! those contract pins as one layered proof.
//!
//! The direct builder boundary is pre-policy-advance. Both public reset
//! constructors eagerly advance toward the first decision and may mutate the
//! `GameState`, so the direct builder is compared only against the complete
//! portable physical projection and the exact V2 root/ordinals, never against
//! a post-advance diagnostic or core hash.

use std::collections::BTreeSet;
use std::fmt;

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use super::{
    derive_environment_randomization_seed_v2, permutation_v2, PhysicalOwnerV2, ShufflePurposeV2,
    ENVIRONMENT_RANDOMIZATION_GOLDENS_SCHEMA_V1, ENVIRONMENT_RANDOMIZATION_GOLDENS_SHA256_V1,
    ENVIRONMENT_RANDOMIZATION_IDENTITY_V2,
    ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GENERATOR_IDENTITY_V1,
    ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_SCHEMA_V1,
    ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_SHA256_V1,
    ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_V1,
    ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PHYSICAL_PROJECTION_IDENTITY_V1,
    ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PORTABLE_VECTOR_STREAM_IDENTITY_V1,
    ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PORTABLE_VECTOR_STREAM_SHA256_V1,
};
use crate::event::CommittedEvent;
use crate::ids::PlayerId;
use crate::native_trainer_schedule_v1::{
    NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1, NATIVE_TRAINER_SCHEDULE_VERSION_V1,
    PYTHON_REFERENCE_SEED_VERSION_V1,
};
use crate::rl::PlayerSeatV1;
use crate::rl_session::{
    FastActorResponseV1, FastActorSessionV1, RlEpisodeSessionV1, RlSessionResponseV1,
    CANONICAL_BURN_DECK_ID, CANONICAL_RALLY_DECK_ID,
};
use crate::runtime_decks::{
    runtime_deck_by_id, RUNTIME_DECK_CATALOG_FILE_SHA256, RUNTIME_DECK_CATALOG_SCHEMA,
    RUNTIME_DECK_HASH_ALGORITHM, RUNTIME_DECK_MATERIALIZATION_PROTOCOL, RUNTIME_DECK_PROTOCOL,
};
use crate::state::GameState;

// Parser ceilings, frozen by the ruling.
const MAX_ARTIFACT_BYTES: usize = 1024 * 1024;
const MAX_RESET_CASES: usize = 8;
const MAX_PAIRED_CASES: usize = 8;
const MAX_REJECT_CASES: usize = 32;
const MAX_CARDS_PER_DECK: usize = 256;
const MAX_DRAW_EVENTS: usize = 32;

// Exact positive cardinalities.
const EXACT_RESET_CASES: usize = 2;
const EXACT_PAIRED_CASES: usize = 1;
const EXACT_REJECT_CASES: usize = 6;

const OPENING_HAND_COUNT: usize = 7;
const OPENING_DRAW_ROUNDS: u32 = 7;

const NATIVE_SCHEDULE_IDENTITY: &str = NATIVE_TRAINER_SCHEDULE_VERSION_V1;
const TRAINER_VERSION_ATOM: &str = PYTHON_REFERENCE_SEED_VERSION_V1;
const U63_MAX: u64 = (1_u64 << 63) - 1;
const NATIVE_BASE_SEED: u64 = 71_501;
const NATIVE_PAIR_ROOT: u64 = 5_293_664_275_683_392_565;
const ROOT_940001: u64 = 940_001;

const ENVIRONMENT_RANDOMIZATION_PYTHON_REFERENCE_V2: &[u8] =
    include_bytes!("../../../python/tools/environment_randomization_v2_reference.py");
const ENVIRONMENT_RANDOMIZATION_PYTHON_REFERENCE_SHA256_V2: &str =
    "9dd7e5357d98ff5a7ac302d285da91fb56cf0d422c5aef6bc9b53f2a5d822024";

const NATIVE_CASE_NAME: &str = "burn-rally-native-base-71501-pair-0";
const ROOT_940001_CASE_NAME: &str = "burn-rally-root-940001";
const PAIRED_CASE_NAME: &str = "native-base-71501-pair-0-learner-role-swap";

// Stored, stable reject vocabulary. These are golden-validator codes only and
// mint no runtime error variant.
const CODE_SEAT: &str = "learner-seat-rule-mismatch";
const CODE_ROOT: &str = "pair-environment-seed-mismatch";
const CODE_DECKS: &str = "physical-deck-binding-mismatch";
const CODE_BIJECTION: &str = "source-permutation-not-bijection";
const CODE_RANGE: &str = "source-permutation-index-out-of-range";
const CODE_PROJECTION: &str = "source-permutation-card-projection-mismatch";

// In-memory structural codes: proof of rejection only, never stored.
const CODE_LENGTH: &str = "source-permutation-length-mismatch";
const CODE_TRAJECTORY: &str = "hand-library-draw-inconsistent";
const CODE_SCHEDULE_IDENTITY: &str = "trainer-schedule-identity-mismatch";
const CODE_EPISODE: &str = "episode-index-rule-mismatch";
const CODE_SHARED: &str = "shared-reset-case-reference-mismatch";

// --------------------------------------------------------------------------
// Strict typed artifact shape
// --------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
enum Owner {
    P0,
    P1,
}

impl Owner {
    const fn as_str(self) -> &'static str {
        match self {
            Owner::P0 => "p0",
            Owner::P1 => "p1",
        }
    }

    const fn physical(self) -> PhysicalOwnerV2 {
        match self {
            Owner::P0 => PhysicalOwnerV2::P0,
            Owner::P1 => PhysicalOwnerV2::P1,
        }
    }

    const fn player(self) -> PlayerId {
        match self {
            Owner::P0 => PlayerId::P0,
            Owner::P1 => PlayerId::P1,
        }
    }

    const fn seat(self) -> PlayerSeatV1 {
        match self {
            Owner::P0 => PlayerSeatV1::P0,
            Owner::P1 => PlayerSeatV1::P1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    schema: String,
    generator_identity: String,
    environment_randomization_identity: String,
    physical_projection_identity: String,
    portable_vector_stream_identity: String,
    source_authorities: SourceAuthorities,
    projection_contract: ProjectionContract,
    reset_cases: Vec<ResetCase>,
    paired_role_cases: Vec<PairedRoleCase>,
    reject_cases: Vec<RejectCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceAuthorities {
    runtime_deck_catalog: RuntimeDeckCatalogAuthority,
    environment_randomization_python_reference: RawFileAuthority,
    environment_randomization_kdf_goldens: GoldenAuthority,
    native_trainer_schedule: NativeScheduleAuthority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeDeckCatalogAuthority {
    schema: String,
    protocol: String,
    materialization_order: String,
    deck_hash_algorithm: String,
    raw_file_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFileAuthority {
    raw_file_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenAuthority {
    schema: String,
    raw_file_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeScheduleAuthority {
    identity: String,
    python_reference_seed_version: String,
    goldens_schema: String,
    goldens_raw_file_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectionContract {
    card_definition_domain: String,
    source_copy_index_domain: String,
    library_order: String,
    initial_shuffle_purpose: String,
    initial_shuffle_ordinal: u64,
    opening_hand_count: u32,
    opening_draw_rounds: u32,
    opening_draw_order_per_round: [Owner; 2],
    live_ordinals_after_reset: [u64; 2],
    authority_scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResetCase {
    name: String,
    input: ResetInput,
    expected_projection: ResetProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResetInput {
    pair_environment_seed: u64,
    p0: PhysicalDeckInput,
    p1: PhysicalDeckInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PhysicalDeckInput {
    physical_owner: Owner,
    deck_id: String,
    runtime_deck_hash_u64_hex: String,
    source_card_definition_ids: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResetProjection {
    p0: OwnerProjection,
    p1: OwnerProjection,
    draw_events: Vec<DrawRecord>,
    next_live_shuffle_ordinals: [u64; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerProjection {
    physical_owner: Owner,
    derived_initial_seed: u64,
    source_index_permutation: Vec<u16>,
    card_definition_id_permutation: Vec<u16>,
    opening_hand_card_definition_ids: Vec<u16>,
    remaining_library_card_definition_ids: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DrawRecord {
    global_event_ordinal: u32,
    owner_draw_ordinal: u32,
    physical_owner: Owner,
    card_definition_id: u16,
    owner_hand_count_after: u32,
    owner_library_count_after: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PairedRoleCase {
    name: String,
    input: PairedRoleInput,
    expected_shared_reset_case_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PairedRoleInput {
    trainer_schedule_identity: String,
    base_seed: u64,
    pair_index: u64,
    even_episode: EpisodeBinding,
    odd_episode: EpisodeBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EpisodeBinding {
    episode_index: u64,
    learner_seat: Owner,
    pair_environment_seed: u64,
    p0_deck_id: String,
    p1_deck_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResetCaseBody {
    input: ResetInput,
    expected_projection: ResetProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PairedRoleCaseBody {
    input: PairedRoleInput,
    expected_shared_reset_case_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RejectCase {
    name: String,
    input: RejectInput,
    expected_rejection: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// Keep both exact typed arms visible in this evidence-only contract. Boxing
// them would change the in-memory proof shape solely to satisfy a size lint.
#[allow(clippy::large_enum_variant)]
#[serde(
    tag = "kind",
    content = "case",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
enum RejectInput {
    ResetProjection(ResetCaseBody),
    PairedRole(PairedRoleCaseBody),
}

// --------------------------------------------------------------------------
// Duplicate-key rejection before typed deserialization
// --------------------------------------------------------------------------

/// Walks the whole document rejecting duplicate object keys at every nesting
/// level and rejecting every float literal. It intentionally builds no
/// `serde_json::Value`: routing through `Value` first would silently collapse
/// duplicates under last-key-wins before anything could observe them.
struct DuplicateKeyScan;

impl<'de> Deserialize<'de> for DuplicateKeyScan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ScanVisitor)
    }
}

struct ScanVisitor;

impl<'de> Visitor<'de> for ScanVisitor {
    type Value = DuplicateKeyScan;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a float-free JSON document without duplicate object keys")
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Err(E::custom(format!("float literal {value} is not permitted")))
    }

    fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(DuplicateKeyScan)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ScanVisitor)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element::<DuplicateKeyScan>()?.is_some() {}
        Ok(DuplicateKeyScan)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(de::Error::custom(format!("duplicate object key {key:?}")));
            }
            map.next_value::<DuplicateKeyScan>()?;
        }
        Ok(DuplicateKeyScan)
    }
}

fn reject_duplicate_keys(raw: &str) -> Result<(), String> {
    serde_json::from_str::<DuplicateKeyScan>(raw)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

// --------------------------------------------------------------------------
// Canonical bytes and portable semantic stream
// --------------------------------------------------------------------------

fn sha256_hex(payload: &[u8]) -> String {
    Sha256::digest(payload)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Compact, sorted-key JSON. `serde_json::Map` is a `BTreeMap` here (the
/// `preserve_order` feature is off), so serializing through `Value` reproduces
/// Python's `sort_keys=True, separators=(",", ":")` exactly. The artifact is
/// pure ASCII, so `ensure_ascii=True` is a no-op difference.
fn canonical_json<T: Serialize>(value: &T) -> String {
    let as_value = serde_json::to_value(value).expect("typed value converts to JSON");
    serde_json::to_string(&as_value).expect("JSON value serializes canonically")
}

/// Canonical JSON with exactly one final LF, matching the generator's `CJ`.
fn cj<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = canonical_json(value).into_bytes();
    bytes.push(b'\n');
    bytes
}

fn atom(tag: &str, payload: &[u8]) -> Vec<u8> {
    let tag_len = u32::try_from(tag.len()).expect("atom tag length fits u32");
    let payload_len = u64::try_from(payload.len()).expect("atom payload length fits u64");
    let mut out = Vec::with_capacity(12 + tag.len() + payload.len());
    out.extend_from_slice(&tag_len.to_be_bytes());
    out.extend_from_slice(tag.as_bytes());
    out.extend_from_slice(&payload_len.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

fn u64be(value: usize) -> [u8; 8] {
    u64::try_from(value).expect("count fits u64").to_be_bytes()
}

/// Independently reconstructs the portable semantic stream from typed values.
/// It never reads a Python-precomputed semantic stream.
fn portable_semantic_stream(artifact: &Artifact) -> Vec<u8> {
    let mut stream = Vec::new();
    stream.extend_from_slice(&atom(
        "domain",
        artifact.portable_vector_stream_identity.as_bytes(),
    ));
    stream.extend_from_slice(&atom("artifact_schema_utf8", artifact.schema.as_bytes()));
    stream.extend_from_slice(&atom(
        "environment_randomization_identity_utf8",
        artifact.environment_randomization_identity.as_bytes(),
    ));
    stream.extend_from_slice(&atom(
        "physical_projection_identity_utf8",
        artifact.physical_projection_identity.as_bytes(),
    ));
    stream.extend_from_slice(&atom(
        "source_authorities_canonical_json",
        &cj(&artifact.source_authorities),
    ));
    stream.extend_from_slice(&atom(
        "projection_contract_canonical_json",
        &cj(&artifact.projection_contract),
    ));

    stream.extend_from_slice(&atom(
        "reset_case_count_u64be",
        &u64be(artifact.reset_cases.len()),
    ));
    for case in &artifact.reset_cases {
        let mut nested = Vec::new();
        nested.extend_from_slice(&atom("name_utf8", case.name.as_bytes()));
        nested.extend_from_slice(&atom("input_canonical_json", &cj(&case.input)));
        nested.extend_from_slice(&atom(
            "expected_projection_canonical_json",
            &cj(&case.expected_projection),
        ));
        stream.extend_from_slice(&atom("reset_case", &nested));
    }

    stream.extend_from_slice(&atom(
        "paired_role_case_count_u64be",
        &u64be(artifact.paired_role_cases.len()),
    ));
    for case in &artifact.paired_role_cases {
        let mut nested = Vec::new();
        nested.extend_from_slice(&atom("name_utf8", case.name.as_bytes()));
        nested.extend_from_slice(&atom("input_canonical_json", &cj(&case.input)));
        nested.extend_from_slice(&atom(
            "expected_shared_reset_case_name_utf8",
            case.expected_shared_reset_case_name.as_bytes(),
        ));
        stream.extend_from_slice(&atom("paired_role_case", &nested));
    }

    stream.extend_from_slice(&atom(
        "reject_case_count_u64be",
        &u64be(artifact.reject_cases.len()),
    ));
    for case in &artifact.reject_cases {
        let mut nested = Vec::new();
        nested.extend_from_slice(&atom("name_utf8", case.name.as_bytes()));
        nested.extend_from_slice(&atom("input_canonical_json", &cj(&case.input)));
        nested.extend_from_slice(&atom(
            "expected_rejection_ascii",
            case.expected_rejection.as_bytes(),
        ));
        stream.extend_from_slice(&atom("reject_case", &nested));
    }
    stream
}

// --------------------------------------------------------------------------
// Strict string domains
// --------------------------------------------------------------------------

fn is_case_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 128 {
        return false;
    }
    let first = bytes[0];
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn is_lowercase_hex(value: &str, width: usize) -> bool {
    value.len() == width
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_printable_ascii_deck_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| (0x20..=0x7E).contains(&byte))
}

// --------------------------------------------------------------------------
// Unsealed validators
// --------------------------------------------------------------------------

fn expected_source_authorities() -> SourceAuthorities {
    SourceAuthorities {
        runtime_deck_catalog: RuntimeDeckCatalogAuthority {
            schema: RUNTIME_DECK_CATALOG_SCHEMA.to_owned(),
            protocol: RUNTIME_DECK_PROTOCOL.to_owned(),
            materialization_order: RUNTIME_DECK_MATERIALIZATION_PROTOCOL.to_owned(),
            deck_hash_algorithm: RUNTIME_DECK_HASH_ALGORITHM.to_owned(),
            raw_file_sha256: RUNTIME_DECK_CATALOG_FILE_SHA256.to_owned(),
        },
        environment_randomization_python_reference: RawFileAuthority {
            raw_file_sha256: ENVIRONMENT_RANDOMIZATION_PYTHON_REFERENCE_SHA256_V2.to_owned(),
        },
        environment_randomization_kdf_goldens: GoldenAuthority {
            schema: ENVIRONMENT_RANDOMIZATION_GOLDENS_SCHEMA_V1.to_owned(),
            raw_file_sha256: ENVIRONMENT_RANDOMIZATION_GOLDENS_SHA256_V1.to_owned(),
        },
        native_trainer_schedule: NativeScheduleAuthority {
            identity: NATIVE_TRAINER_SCHEDULE_VERSION_V1.to_owned(),
            python_reference_seed_version: PYTHON_REFERENCE_SEED_VERSION_V1.to_owned(),
            goldens_schema: "mtg_kernel_native_trainer_schedule_goldens/v1".to_owned(),
            goldens_raw_file_sha256: NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1.to_owned(),
        },
    }
}

fn expected_projection_contract() -> ProjectionContract {
    ProjectionContract {
        card_definition_domain: "u16-runtime-card-definition-id".to_owned(),
        source_copy_index_domain: "zero-based-materialized-mainboard-index".to_owned(),
        library_order: "index-zero-is-next-draw".to_owned(),
        initial_shuffle_purpose: "initial-library-shuffle".to_owned(),
        initial_shuffle_ordinal: 0,
        opening_hand_count: 7,
        opening_draw_rounds: 7,
        opening_draw_order_per_round: [Owner::P0, Owner::P1],
        live_ordinals_after_reset: [0, 0],
        authority_scope:
            "stdlib-python-kdf-permutation-runtime-card-definition-and-draw-projection-only"
                .to_owned(),
    }
}

fn validate_reset_body_ceilings(body: &ResetCaseBody, label: &str) -> Result<(), String> {
    for (owner, expected_owner, deck, projection) in [
        (
            "p0",
            Owner::P0,
            &body.input.p0,
            &body.expected_projection.p0,
        ),
        (
            "p1",
            Owner::P1,
            &body.input.p1,
            &body.expected_projection.p1,
        ),
    ] {
        if deck.physical_owner != expected_owner {
            return Err(format!(
                "{label} {owner}: input physical owner is {:?}, expected {expected_owner:?}",
                deck.physical_owner
            ));
        }
        if !is_printable_ascii_deck_id(&deck.deck_id) {
            return Err(format!(
                "{label} {owner}: deck id {:?} is not printable ASCII",
                deck.deck_id
            ));
        }
        if !is_lowercase_hex(&deck.runtime_deck_hash_u64_hex, 16) {
            return Err(format!(
                "{label} {owner}: deck hash {:?} is not 16 lowercase hex",
                deck.runtime_deck_hash_u64_hex
            ));
        }
        if deck.source_card_definition_ids.len() > MAX_CARDS_PER_DECK {
            return Err(format!("{label} {owner}: source deck ceiling exceeded"));
        }

        for (field, length) in [
            (
                "source-index permutation",
                projection.source_index_permutation.len(),
            ),
            (
                "card-definition permutation",
                projection.card_definition_id_permutation.len(),
            ),
            (
                "opening hand",
                projection.opening_hand_card_definition_ids.len(),
            ),
            (
                "remaining library",
                projection.remaining_library_card_definition_ids.len(),
            ),
        ] {
            if length > MAX_CARDS_PER_DECK {
                return Err(format!("{label} {owner}: {field} ceiling exceeded"));
            }
        }

        let partition_length = projection
            .opening_hand_card_definition_ids
            .len()
            .checked_add(projection.remaining_library_card_definition_ids.len())
            .ok_or_else(|| format!("{label} {owner}: card partition length overflow"))?;
        if partition_length > MAX_CARDS_PER_DECK {
            return Err(format!(
                "{label} {owner}: combined hand/library ceiling exceeded"
            ));
        }
    }

    if body.expected_projection.draw_events.len() > MAX_DRAW_EVENTS {
        return Err(format!("{label}: draw-event ceiling exceeded"));
    }
    Ok(())
}

fn validate_episode_deck_ids(input: &PairedRoleInput, label: &str) -> Result<(), String> {
    for (episode, binding) in [("even", &input.even_episode), ("odd", &input.odd_episode)] {
        for (owner, deck_id) in [("p0", &binding.p0_deck_id), ("p1", &binding.p1_deck_id)] {
            if !is_printable_ascii_deck_id(deck_id) {
                return Err(format!(
                    "{label} {episode} {owner}: deck id {deck_id:?} is not printable ASCII"
                ));
            }
        }
    }
    Ok(())
}

/// Unsealed structural validator. Negative tests call this directly so the
/// raw-file hash gate cannot make them vacuous.
fn validate_artifact_structure(artifact: &Artifact) -> Result<(), String> {
    if artifact.schema != ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_SCHEMA_V1 {
        return Err("schema identity mismatch".to_string());
    }
    if artifact.generator_identity
        != ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GENERATOR_IDENTITY_V1
    {
        return Err("generator identity mismatch".to_string());
    }
    if artifact.environment_randomization_identity != ENVIRONMENT_RANDOMIZATION_IDENTITY_V2 {
        return Err("environment randomization identity mismatch".to_string());
    }
    if artifact.physical_projection_identity
        != ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PHYSICAL_PROJECTION_IDENTITY_V1
    {
        return Err("physical projection identity mismatch".to_string());
    }
    if artifact.portable_vector_stream_identity
        != ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PORTABLE_VECTOR_STREAM_IDENTITY_V1
    {
        return Err("portable stream identity mismatch".to_string());
    }
    if artifact.projection_contract != expected_projection_contract() {
        return Err("physical projection contract mismatch".to_string());
    }

    if artifact.reset_cases.len() > MAX_RESET_CASES {
        return Err("reset case ceiling exceeded".to_string());
    }
    if artifact.paired_role_cases.len() > MAX_PAIRED_CASES {
        return Err("paired case ceiling exceeded".to_string());
    }
    if artifact.reject_cases.len() > MAX_REJECT_CASES {
        return Err("reject case ceiling exceeded".to_string());
    }
    if artifact.reset_cases.len() != EXACT_RESET_CASES {
        return Err("exact reset case count violated".to_string());
    }
    if artifact.paired_role_cases.len() != EXACT_PAIRED_CASES {
        return Err("exact paired case count violated".to_string());
    }
    if artifact.reject_cases.len() != EXACT_REJECT_CASES {
        return Err("exact reject case count violated".to_string());
    }

    validate_names(
        artifact.reset_cases.iter().map(|case| case.name.as_str()),
        "reset_cases",
    )?;
    validate_names(
        artifact
            .paired_role_cases
            .iter()
            .map(|case| case.name.as_str()),
        "paired_role_cases",
    )?;
    validate_names(
        artifact.reject_cases.iter().map(|case| case.name.as_str()),
        "reject_cases",
    )?;

    for authority_sha in [
        &artifact
            .source_authorities
            .runtime_deck_catalog
            .raw_file_sha256,
        &artifact
            .source_authorities
            .environment_randomization_python_reference
            .raw_file_sha256,
        &artifact
            .source_authorities
            .environment_randomization_kdf_goldens
            .raw_file_sha256,
        &artifact
            .source_authorities
            .native_trainer_schedule
            .goldens_raw_file_sha256,
    ] {
        if !is_lowercase_hex(authority_sha, 64) {
            return Err(format!(
                "authority SHA-256 {authority_sha:?} is not 64 lowercase hex"
            ));
        }
    }

    let python_reference_sha = sha256_hex(ENVIRONMENT_RANDOMIZATION_PYTHON_REFERENCE_V2);
    if python_reference_sha != ENVIRONMENT_RANDOMIZATION_PYTHON_REFERENCE_SHA256_V2 {
        return Err(format!(
            "environment-randomization Python reference SHA-256 drifted: \
             expected {ENVIRONMENT_RANDOMIZATION_PYTHON_REFERENCE_SHA256_V2}, \
             observed {python_reference_sha}"
        ));
    }
    let expected_authorities = expected_source_authorities();
    if artifact.source_authorities != expected_authorities {
        return Err(format!(
            "source authorities mismatch: expected {expected_authorities:?}, observed {:?}",
            artifact.source_authorities
        ));
    }

    for case in &artifact.reset_cases {
        let body = ResetCaseBody {
            input: case.input.clone(),
            expected_projection: case.expected_projection.clone(),
        };
        validate_reset_body_ceilings(&body, &case.name)?;
        if let Some(code) = validate_reset_body(&body) {
            return Err(format!(
                "positive reset case {} rejected: {code}",
                case.name
            ));
        }
    }

    for case in &artifact.paired_role_cases {
        let body = PairedRoleCaseBody {
            input: case.input.clone(),
            expected_shared_reset_case_name: case.expected_shared_reset_case_name.clone(),
        };
        validate_episode_deck_ids(&body.input, &case.name)?;
        if let Some(code) = validate_paired_body(&body, artifact) {
            return Err(format!(
                "positive paired case {} rejected: {code}",
                case.name
            ));
        }
    }
    for case in &artifact.reject_cases {
        validate_stored_reject(case, artifact)?;
    }
    Ok(())
}

fn validate_names<'a, I>(names: I, label: &str) -> Result<(), String>
where
    I: Iterator<Item = &'a str>,
{
    let mut previous: Option<&str> = None;
    for name in names {
        if !is_case_name(name) {
            return Err(format!("{label}: case name {name:?} violates the grammar"));
        }
        if let Some(previous_name) = previous {
            if name <= previous_name {
                return Err(format!(
                    "{label}: case names are not strictly increasing at {name:?}"
                ));
            }
        }
        previous = Some(name);
    }
    Ok(())
}

fn expected_draw_events(p0: &OwnerProjection, p1: &OwnerProjection) -> Vec<DrawRecord> {
    let mut events = Vec::new();
    for draw_round in 0..OPENING_DRAW_ROUNDS {
        for (offset, (owner, projection)) in
            [(Owner::P0, p0), (Owner::P1, p1)].into_iter().enumerate()
        {
            let library_len = projection.card_definition_id_permutation.len() as u32;
            events.push(DrawRecord {
                global_event_ordinal: 2 * draw_round + offset as u32,
                owner_draw_ordinal: draw_round,
                physical_owner: owner,
                card_definition_id: projection.card_definition_id_permutation[draw_round as usize],
                owner_hand_count_after: draw_round + 1,
                owner_library_count_after: library_len - (draw_round + 1),
            });
        }
    }
    events
}

/// Frozen precedence: source-index length, then index range, then bijection,
/// then source-index-to-card projection, then hand/library/draw consistency.
fn validate_reset_body(body: &ResetCaseBody) -> Option<&'static str> {
    let decks = [
        &body.input.p0.source_card_definition_ids,
        &body.input.p1.source_card_definition_ids,
    ];
    let inputs = [&body.input.p0, &body.input.p1];
    let projections = [&body.expected_projection.p0, &body.expected_projection.p1];

    for (deck, projection) in decks.iter().zip(projections.iter()) {
        if projection.source_index_permutation.len() != deck.len() {
            return Some(CODE_LENGTH);
        }
    }
    for (deck, projection) in decks.iter().zip(projections.iter()) {
        let limit = deck.len();
        if projection
            .source_index_permutation
            .iter()
            .any(|index| usize::from(*index) >= limit)
        {
            return Some(CODE_RANGE);
        }
    }
    for (deck, projection) in decks.iter().zip(projections.iter()) {
        let mut sorted = projection.source_index_permutation.clone();
        sorted.sort_unstable();
        let expected: Vec<u16> = (0..deck.len() as u16).collect();
        if sorted != expected {
            return Some(CODE_BIJECTION);
        }
    }
    for (deck, projection) in decks.iter().zip(projections.iter()) {
        let projected: Vec<u16> = projection
            .source_index_permutation
            .iter()
            .map(|index| deck[usize::from(*index)])
            .collect();
        if projected != projection.card_definition_id_permutation {
            return Some(CODE_PROJECTION);
        }
    }
    for ((owner, input), projection) in [Owner::P0, Owner::P1]
        .into_iter()
        .zip(inputs)
        .zip(projections)
    {
        if input.physical_owner != owner || projection.physical_owner != owner {
            return Some(CODE_TRAJECTORY);
        }
        let permutation = &projection.card_definition_id_permutation;
        if permutation.len() < OPENING_HAND_COUNT {
            return Some(CODE_TRAJECTORY);
        }
        if projection.opening_hand_card_definition_ids != permutation[..OPENING_HAND_COUNT] {
            return Some(CODE_TRAJECTORY);
        }
        if projection.remaining_library_card_definition_ids != permutation[OPENING_HAND_COUNT..] {
            return Some(CODE_TRAJECTORY);
        }
    }
    if body.expected_projection.draw_events
        != expected_draw_events(&body.expected_projection.p0, &body.expected_projection.p1)
    {
        return Some(CODE_TRAJECTORY);
    }
    if body.expected_projection.next_live_shuffle_ordinals != [0, 0] {
        return Some(CODE_TRAJECTORY);
    }
    None
}

/// Frozen precedence: schedule identity, then episode/index relationships,
/// then learner seats, then derived/shared roots, then the shared-reset
/// reference, then fixed physical deck bindings.
fn validate_paired_body(body: &PairedRoleCaseBody, artifact: &Artifact) -> Option<&'static str> {
    if body.input.trainer_schedule_identity != NATIVE_SCHEDULE_IDENTITY {
        return Some(CODE_SCHEDULE_IDENTITY);
    }
    let even = &body.input.even_episode;
    let odd = &body.input.odd_episode;
    if !even.episode_index.is_multiple_of(2) {
        return Some(CODE_EPISODE);
    }
    let Some(expected_odd_index) = even.episode_index.checked_add(1) else {
        return Some(CODE_EPISODE);
    };
    if odd.episode_index != expected_odd_index {
        return Some(CODE_EPISODE);
    }
    if even.episode_index > U63_MAX
        || odd.episode_index > U63_MAX
        || body.input.pair_index > U63_MAX
    {
        return Some(CODE_EPISODE);
    }
    if even.episode_index / 2 != body.input.pair_index
        || odd.episode_index / 2 != body.input.pair_index
    {
        return Some(CODE_EPISODE);
    }
    if even.learner_seat != Owner::P0 || odd.learner_seat != Owner::P1 {
        return Some(CODE_SEAT);
    }

    if body.input.base_seed > U63_MAX {
        return Some(CODE_ROOT);
    }
    let schedule = match crate::native_trainer_schedule_v1::native_trainer_episode_schedule_v1(
        body.input.base_seed,
        even.episode_index,
    ) {
        Ok(schedule) => schedule,
        Err(_) => return Some(CODE_ROOT),
    };
    let derived = schedule.environment_seed;
    if even.pair_environment_seed != derived || odd.pair_environment_seed != derived {
        return Some(CODE_ROOT);
    }

    let shared = artifact
        .reset_cases
        .iter()
        .find(|case| case.name == body.expected_shared_reset_case_name);
    match shared {
        Some(case) if case.input.pair_environment_seed == derived => {}
        _ => return Some(CODE_SHARED),
    }

    for episode in [even, odd] {
        if episode.p0_deck_id != CANONICAL_BURN_DECK_ID {
            return Some(CODE_DECKS);
        }
        if episode.p1_deck_id != CANONICAL_RALLY_DECK_ID {
            return Some(CODE_DECKS);
        }
    }
    None
}

fn cloned_reset_body(artifact: &Artifact, name: &str) -> Result<ResetCaseBody, String> {
    artifact
        .reset_cases
        .iter()
        .find(|case| case.name == name)
        .map(|case| ResetCaseBody {
            input: case.input.clone(),
            expected_projection: case.expected_projection.clone(),
        })
        .ok_or_else(|| format!("missing positive reset case {name:?}"))
}

fn cloned_paired_body(artifact: &Artifact, name: &str) -> Result<PairedRoleCaseBody, String> {
    artifact
        .paired_role_cases
        .iter()
        .find(|case| case.name == name)
        .map(|case| PairedRoleCaseBody {
            input: case.input.clone(),
            expected_shared_reset_case_name: case.expected_shared_reset_case_name.clone(),
        })
        .ok_or_else(|| format!("missing positive paired case {name:?}"))
}

fn replace_exact<T>(
    slot: &mut T,
    expected_old: T,
    replacement: T,
    label: &str,
) -> Result<(), String>
where
    T: fmt::Debug + PartialEq,
{
    if *slot != expected_old {
        return Err(format!(
            "{label}: expected old leaf {expected_old:?}, observed {slot:?}"
        ));
    }
    *slot = replacement;
    Ok(())
}

fn reconstructed_reject(
    artifact: &Artifact,
    name: &str,
) -> Result<(RejectInput, &'static str), String> {
    match name {
        "paired-role-learner-seat-not-swapped" => {
            let mut body = cloned_paired_body(artifact, PAIRED_CASE_NAME)?;
            replace_exact(
                &mut body.input.odd_episode.learner_seat,
                Owner::P1,
                Owner::P0,
                name,
            )?;
            Ok((RejectInput::PairedRole(body), CODE_SEAT))
        }
        "paired-role-odd-environment-seed-drift" => {
            let mut body = cloned_paired_body(artifact, PAIRED_CASE_NAME)?;
            replace_exact(
                &mut body.input.odd_episode.pair_environment_seed,
                NATIVE_PAIR_ROOT,
                NATIVE_PAIR_ROOT - 1,
                name,
            )?;
            Ok((RejectInput::PairedRole(body), CODE_ROOT))
        }
        "paired-role-odd-physical-decks-swapped" => {
            let mut body = cloned_paired_body(artifact, PAIRED_CASE_NAME)?;
            replace_exact(
                &mut body.input.odd_episode.p0_deck_id,
                CANONICAL_BURN_DECK_ID.to_owned(),
                CANONICAL_RALLY_DECK_ID.to_owned(),
                name,
            )?;
            replace_exact(
                &mut body.input.odd_episode.p1_deck_id,
                CANONICAL_RALLY_DECK_ID.to_owned(),
                CANONICAL_BURN_DECK_ID.to_owned(),
                name,
            )?;
            Ok((RejectInput::PairedRole(body), CODE_DECKS))
        }
        "reset-source-permutation-duplicate-index" => {
            let mut body = cloned_reset_body(artifact, ROOT_940001_CASE_NAME)?;
            let slot = body
                .expected_projection
                .p0
                .source_index_permutation
                .get_mut(17)
                .ok_or_else(|| format!("{name}: missing permutation index 17"))?;
            replace_exact(slot, 37_u16, 36_u16, name)?;
            Ok((RejectInput::ResetProjection(body), CODE_BIJECTION))
        }
        "reset-source-permutation-index-out-of-range" => {
            let mut body = cloned_reset_body(artifact, ROOT_940001_CASE_NAME)?;
            let slot = body
                .expected_projection
                .p0
                .source_index_permutation
                .get_mut(0)
                .ok_or_else(|| format!("{name}: missing permutation index 0"))?;
            replace_exact(slot, 36_u16, 60_u16, name)?;
            Ok((RejectInput::ResetProjection(body), CODE_RANGE))
        }
        "reset-source-permutation-projection-mismatch" => {
            let mut body = cloned_reset_body(artifact, ROOT_940001_CASE_NAME)?;
            let slot = body
                .expected_projection
                .p0
                .card_definition_id_permutation
                .get_mut(0)
                .ok_or_else(|| format!("{name}: missing card permutation index 0"))?;
            replace_exact(slot, 47_u16, 37_u16, name)?;
            Ok((RejectInput::ResetProjection(body), CODE_PROJECTION))
        }
        _ => Err(format!("reject {name:?} has no frozen reconstruction")),
    }
}

fn validate_stored_reject(case: &RejectCase, artifact: &Artifact) -> Result<(), String> {
    match &case.input {
        RejectInput::ResetProjection(body) => {
            validate_reset_body_ceilings(body, &case.name)?;
        }
        RejectInput::PairedRole(body) => {
            validate_episode_deck_ids(&body.input, &case.name)?;
        }
    }

    // Equality precedes classification and proves that no extra leaf differs
    // from the named accepted positive.
    let (expected_input, expected_code) = reconstructed_reject(artifact, &case.name)?;
    if case.input != expected_input {
        return Err(format!(
            "reject {:?} is not its exact frozen reconstruction",
            case.name
        ));
    }
    if case.expected_rejection != expected_code {
        return Err(format!(
            "reject {:?}: expected code {expected_code:?}, stored {:?}",
            case.name, case.expected_rejection
        ));
    }

    let observed = classify_reject(&case.input, artifact);
    if observed != Some(expected_code) {
        return Err(format!(
            "reject {:?}: expected classification {expected_code:?}, observed {observed:?}",
            case.name
        ));
    }
    Ok(())
}

fn classify_reject(input: &RejectInput, artifact: &Artifact) -> Option<&'static str> {
    match input {
        RejectInput::ResetProjection(body) => validate_reset_body(body),
        RejectInput::PairedRole(body) => validate_paired_body(body, artifact),
    }
}

// --------------------------------------------------------------------------
// Sealed load path
// --------------------------------------------------------------------------

/// Hashes the raw embedded bytes before any parse, enforces the byte-shape and
/// size bounds, rejects duplicate keys before typed decoding, strictly decodes,
/// and proves exact canonical re-encoding.
fn sealed_artifact() -> Artifact {
    let raw = ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_V1;
    let bytes = raw.as_bytes();

    assert_eq!(
        sha256_hex(bytes),
        ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_SHA256_V1,
        "embedded portable golden bytes do not match the sealed SHA-256"
    );
    assert!(bytes.len() <= MAX_ARTIFACT_BYTES, "artifact exceeds 1 MiB");
    assert!(bytes.is_ascii(), "artifact must be pure ASCII");
    assert!(!bytes.contains(&b'\r'), "artifact must contain no CR");
    assert!(
        !raw.starts_with('\u{feff}'),
        "artifact must not carry a BOM"
    );
    assert!(raw.ends_with('\n'), "artifact must end with LF");
    assert_eq!(
        bytes.iter().filter(|byte| **byte == b'\n').count(),
        1,
        "artifact must contain exactly one LF"
    );

    reject_duplicate_keys(raw).expect("artifact has no duplicate object keys and no floats");

    let artifact: Artifact =
        serde_json::from_str(raw).expect("artifact strictly decodes into the typed shape");

    let body = raw.strip_suffix('\n').expect("artifact ends with LF");
    assert_eq!(
        canonical_json(&artifact),
        body,
        "typed artifact must re-encode to the exact canonical bytes"
    );
    artifact
}

#[cfg(test)]
mod tests {
    use super::*;

    fn burn_and_rally() -> (Vec<u16>, Vec<u16>) {
        (crate::rl::burn_deck_ids(), crate::rl::rally_deck_ids())
    }

    fn deck_ids() -> [String; 2] {
        [
            CANONICAL_BURN_DECK_ID.to_string(),
            CANONICAL_RALLY_DECK_ID.to_string(),
        ]
    }

    fn case_by_name<'a>(artifact: &'a Artifact, name: &str) -> &'a ResetCase {
        artifact
            .reset_cases
            .iter()
            .find(|case| case.name == name)
            .expect("named reset case exists")
    }

    fn hand_definition_order(state: &GameState, player: PlayerId) -> Vec<u16> {
        state.players[player.index()]
            .hand
            .iter()
            .map(|object| state.objects.get(*object).card_def)
            .collect()
    }

    fn library_definition_order(state: &GameState, player: PlayerId) -> Vec<u16> {
        state.players[player.index()]
            .library
            .iter()
            .map(|object| state.objects.get(*object).card_def)
            .collect()
    }

    // ---- 1. sealed load, identities, counts, order, both SHA pins --------

    #[test]
    fn sealed_artifact_identities_counts_and_canonical_bytes() {
        let artifact = sealed_artifact();
        validate_artifact_structure(&artifact).expect("accepted artifact validates");

        assert_eq!(artifact.reset_cases.len(), EXACT_RESET_CASES);
        assert_eq!(artifact.paired_role_cases.len(), EXACT_PAIRED_CASES);
        assert_eq!(artifact.reject_cases.len(), EXACT_REJECT_CASES);

        let names: Vec<&str> = artifact
            .reset_cases
            .iter()
            .map(|case| case.name.as_str())
            .collect();
        assert_eq!(names, [NATIVE_CASE_NAME, ROOT_940001_CASE_NAME]);
        assert_eq!(artifact.paired_role_cases[0].name, PAIRED_CASE_NAME);
        assert_eq!(
            case_by_name(&artifact, ROOT_940001_CASE_NAME)
                .input
                .pair_environment_seed,
            ROOT_940001
        );

        assert_eq!(
            sha256_hex(ENVIRONMENT_RANDOMIZATION_PYTHON_REFERENCE_V2),
            ENVIRONMENT_RANDOMIZATION_PYTHON_REFERENCE_SHA256_V2
        );
        assert_eq!(artifact.source_authorities, expected_source_authorities());
        assert_eq!(artifact.projection_contract, expected_projection_contract());

        let schedule = &artifact.source_authorities.native_trainer_schedule;
        assert_eq!(schedule.identity, NATIVE_SCHEDULE_IDENTITY);
        assert_eq!(schedule.python_reference_seed_version, TRAINER_VERSION_ATOM);
        assert_eq!(
            schedule.goldens_schema,
            "mtg_kernel_native_trainer_schedule_goldens/v1"
        );

        let contract = &artifact.projection_contract;
        assert_eq!(
            contract.card_definition_domain,
            "u16-runtime-card-definition-id"
        );
        assert_eq!(
            contract.source_copy_index_domain,
            "zero-based-materialized-mainboard-index"
        );
        assert_eq!(contract.library_order, "index-zero-is-next-draw");
        assert_eq!(contract.initial_shuffle_purpose, "initial-library-shuffle");
        assert_eq!(contract.initial_shuffle_ordinal, 0);
        assert_eq!(contract.opening_hand_count, 7);
        assert_eq!(contract.opening_draw_rounds, 7);
        assert_eq!(
            contract.opening_draw_order_per_round,
            [Owner::P0, Owner::P1]
        );
        assert_eq!(contract.live_ordinals_after_reset, [0, 0]);
        assert_eq!(
            contract.authority_scope,
            "stdlib-python-kdf-permutation-runtime-card-definition-and-draw-projection-only"
        );
    }

    #[test]
    fn independently_reconstructed_semantic_stream_matches_the_pin() {
        let artifact = sealed_artifact();
        let stream = portable_semantic_stream(&artifact);
        assert_eq!(
            sha256_hex(&stream),
            ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_PORTABLE_VECTOR_STREAM_SHA256_V1,
            "reconstructed portable stream does not match the sealed SHA-256"
        );
    }

    // ---- 2. decks and hashes match the generated runtime catalog ---------

    #[test]
    fn ordered_decks_and_hashes_match_the_runtime_catalog() {
        let artifact = sealed_artifact();
        for case in &artifact.reset_cases {
            for (deck_input, expected_owner, expected_id) in [
                (&case.input.p0, Owner::P0, CANONICAL_BURN_DECK_ID),
                (&case.input.p1, Owner::P1, CANONICAL_RALLY_DECK_ID),
            ] {
                assert_eq!(deck_input.physical_owner, expected_owner);
                assert_eq!(deck_input.deck_id, expected_id);
                let catalog = runtime_deck_by_id(&deck_input.deck_id)
                    .expect("deck resolves in the generated catalog");
                assert_eq!(
                    deck_input.source_card_definition_ids, catalog.card_ids,
                    "exact ordered materialized mainboard"
                );
                assert_eq!(
                    deck_input.runtime_deck_hash_u64_hex,
                    format!("{:016x}", catalog.runtime_deck_hash),
                    "runtime deck hash provenance"
                );
            }
        }
    }

    // ---- 3. production KDF and permutation over indices and card ids -----

    #[test]
    fn production_kdf_and_permutation_reproduce_both_vectors() {
        let artifact = sealed_artifact();
        for case in &artifact.reset_cases {
            let root = case.input.pair_environment_seed;
            for (deck_input, projection) in [
                (&case.input.p0, &case.expected_projection.p0),
                (&case.input.p1, &case.expected_projection.p1),
            ] {
                let owner = deck_input.physical_owner;
                let seed = derive_environment_randomization_seed_v2(
                    root,
                    owner.physical(),
                    ShufflePurposeV2::InitialLibraryShuffle,
                    0,
                )
                .expect("initial substream derives");
                assert_eq!(
                    seed,
                    projection.derived_initial_seed,
                    "{} {} derived seed",
                    case.name,
                    owner.as_str()
                );

                // Unique source indices: this is what makes an incorrect swap
                // of two equal card copies detectable.
                let source_indices: Vec<u16> =
                    (0..deck_input.source_card_definition_ids.len() as u16).collect();
                assert_eq!(
                    permutation_v2(seed, &source_indices),
                    projection.source_index_permutation,
                    "{} {} source index permutation",
                    case.name,
                    owner.as_str()
                );

                // The repeated card-id array shuffled directly.
                assert_eq!(
                    permutation_v2(seed, &deck_input.source_card_definition_ids),
                    projection.card_definition_id_permutation,
                    "{} {} card id permutation",
                    case.name,
                    owner.as_str()
                );

                // Projection through the index vector agrees with both.
                let projected: Vec<u16> = projection
                    .source_index_permutation
                    .iter()
                    .map(|index| deck_input.source_card_definition_ids[usize::from(*index)])
                    .collect();
                assert_eq!(projected, projection.card_definition_id_permutation);
            }
        }
    }

    // ---- 4. direct pre-advance builder against the portable projection ---

    #[test]
    fn direct_builder_matches_the_complete_portable_projection() {
        let artifact = sealed_artifact();
        let (burn, rally) = burn_and_rally();
        for case in &artifact.reset_cases {
            let root = case.input.pair_environment_seed;
            let state = crate::rl::build_deck_pair_state_environment_v2(root, &burn, &rally)
                .expect("v2 Burn/Rally pair builds");

            for (owner, projection) in [
                (Owner::P0, &case.expected_projection.p0),
                (Owner::P1, &case.expected_projection.p1),
            ] {
                assert_eq!(
                    hand_definition_order(&state, owner.player()),
                    projection.opening_hand_card_definition_ids,
                    "{} {} hand",
                    case.name,
                    owner.as_str()
                );
                assert_eq!(
                    library_definition_order(&state, owner.player()),
                    projection.remaining_library_card_definition_ids,
                    "{} {} remaining library",
                    case.name,
                    owner.as_str()
                );
            }

            // All fourteen committed Draw variants, in exact event-history
            // order. Historical hand/library counts are not stored by
            // CommittedEvent; those are checked below from the closed
            // alternating-draw formula and final zones.
            assert_eq!(state.engine.event_history.len(), 14);
            assert_eq!(case.expected_projection.draw_events.len(), 14);
            for (event_index, (event, record)) in state
                .engine
                .event_history
                .iter()
                .zip(&case.expected_projection.draw_events)
                .enumerate()
            {
                assert_eq!(
                    record.global_event_ordinal,
                    u32::try_from(event_index).expect("fourteen event indices fit u32")
                );
                match event {
                    CommittedEvent::Draw { player, object } => {
                        assert_eq!(*player, record.physical_owner.player());
                        let object = object.expect("opening draws move a real object");
                        assert_eq!(
                            state.objects.get(object).card_def,
                            record.card_definition_id
                        );
                    }
                    other => panic!("expected a committed draw, observed {other:?}"),
                }
            }
            for record in &case.expected_projection.draw_events {
                assert_eq!(
                    record.global_event_ordinal,
                    2 * record.owner_draw_ordinal
                        + if record.physical_owner == Owner::P0 {
                            0
                        } else {
                            1
                        }
                );
                assert_eq!(record.owner_hand_count_after, record.owner_draw_ordinal + 1);
                assert_eq!(
                    record.owner_library_count_after,
                    60 - (record.owner_draw_ordinal + 1)
                );
            }

            assert!(state.legacy_rng().is_none(), "no legacy RNG on the v2 path");
            let randomization = state
                .environment_randomization_v2()
                .expect("v2-only randomness");
            assert_eq!(randomization.pair_environment_seed(), root);
            assert_eq!(
                randomization.next_live_shuffle_ordinal(PhysicalOwnerV2::P0),
                0
            );
            assert_eq!(
                randomization.next_live_shuffle_ordinal(PhysicalOwnerV2::P1),
                0
            );
            assert_eq!(
                case.expected_projection.next_live_shuffle_ordinals,
                [0, 0],
                "portable artifact agrees on the live ordinals"
            );
            assert_eq!(
                state.diagnostic_state_hash_algorithm(),
                "fnv1a64-serde-json-game-state-envelope-v9",
                "v9 diagnostic identity"
            );
        }
    }

    // ---- 5. both public post-advance resets against each other -----------

    #[test]
    fn public_full_and_combined_fast_resets_agree_post_advance() {
        let artifact = sealed_artifact();
        for case in &artifact.reset_cases {
            let root = case.input.pair_environment_seed;
            let full = RlEpisodeSessionV1::reset_with_decks_and_limits_environment_v2(
                0,
                root,
                8,
                1024,
                deck_ids(),
            )
            .expect("full environment-v2 reset succeeds");
            let fast =
                FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2_environment_v2(
                    0,
                    root,
                    8,
                    1024,
                    deck_ids(),
                )
                .expect("combined fast environment-v2 reset succeeds");

            // Counters.
            assert_eq!(full.policy_step_count(), fast.policy_step_count());
            assert_eq!(
                full.physical_decision_count(),
                fast.physical_decision_count()
            );

            // V6 diagnostic hash and privileged core-environment hash.
            assert_eq!(full.diagnostic_state_hash(), fast.diagnostic_state_hash());
            assert_eq!(
                full.privileged_core_environment_hash(),
                fast.privileged_core_environment_hash()
            );

            let full_decision = match full.current_response() {
                RlSessionResponseV1::Decision(decision) => decision,
                RlSessionResponseV1::Terminal(_) => panic!("full session must be live"),
            };
            let fast_decision = match fast.current_response() {
                FastActorResponseV1::Decision(decision) => decision,
                FastActorResponseV1::Terminal(_) => panic!("fast session must be live"),
            };

            // Normalized current metadata common to both surfaces.
            assert_eq!(full_decision.episode_id, fast_decision.episode_id);
            assert_eq!(full_decision.step, fast_decision.step);
            assert_eq!(
                full_decision.physical_decision_id,
                fast_decision.physical_decision_id
            );
            assert_eq!(full_decision.substep_index, fast_decision.substep_index);
            assert_eq!(full_decision.substep_count, fast_decision.substep_count);
            assert_eq!(full_decision.acting_player, fast_decision.acting_player);
            assert_eq!(
                full_decision.legal_actions.len() as u32,
                fast_decision.legal_action_count
            );

            // Full legal-action semantics equal the fast diagnostic semantics.
            let full_semantics: Vec<_> = full_decision
                .legal_actions
                .iter()
                .map(|action| action.semantic.clone())
                .collect();
            let fast_semantics = fast
                .diagnostic_current_action_semantics()
                .expect("fast session exposes current semantics");
            assert_eq!(full_semantics, fast_semantics);

            // Deck-hash provenance through the full wire response, the fast
            // session's immutable crate-visible provenance accessor, and the
            // runtime catalog. The allocation-light fast decision response
            // intentionally omits these hashes.
            assert_eq!(full_decision.deck_ids, deck_ids());
            let catalog_hashes = [
                runtime_deck_by_id(CANONICAL_BURN_DECK_ID)
                    .expect("Burn resolves")
                    .runtime_deck_hash,
                runtime_deck_by_id(CANONICAL_RALLY_DECK_ID)
                    .expect("Rally resolves")
                    .runtime_deck_hash,
            ];
            assert_eq!(
                full_decision.deck_hashes,
                fast.native_full_trajectory_deck_hashes_v1()
            );
            assert_eq!(full_decision.deck_hashes, catalog_hashes);
            assert_eq!(
                case.input.p0.runtime_deck_hash_u64_hex,
                format!("{:016x}", catalog_hashes[0])
            );
            assert_eq!(
                case.input.p1.runtime_deck_hash_u64_hex,
                format!("{:016x}", catalog_hashes[1])
            );
        }
    }

    // ---- 6. paired learner-role swap over the frozen schedule ------------

    #[test]
    fn paired_role_swap_shares_one_root_and_fixed_physical_decks() {
        let artifact = sealed_artifact();
        let paired = &artifact.paired_role_cases[0];
        assert_eq!(paired.input.base_seed, NATIVE_BASE_SEED);
        assert_eq!(paired.input.pair_index, 0);
        assert_eq!(
            paired.input.trainer_schedule_identity,
            NATIVE_SCHEDULE_IDENTITY
        );

        let even = crate::native_trainer_schedule_v1::native_trainer_episode_schedule_v1(
            NATIVE_BASE_SEED,
            0,
        )
        .expect("episode 0 schedules");
        let odd = crate::native_trainer_schedule_v1::native_trainer_episode_schedule_v1(
            NATIVE_BASE_SEED,
            1,
        )
        .expect("episode 1 schedules");

        // Exact roles.
        assert_eq!(even.learner_seat, PlayerSeatV1::P0);
        assert_eq!(odd.learner_seat, PlayerSeatV1::P1);
        assert_eq!(
            even.learner_seat,
            paired.input.even_episode.learner_seat.seat()
        );
        assert_eq!(
            odd.learner_seat,
            paired.input.odd_episode.learner_seat.seat()
        );

        // Identical root, matching the frozen pin and the shared reset case.
        assert_eq!(even.pair_index, 0);
        assert_eq!(odd.pair_index, 0);
        assert_eq!(even.environment_seed, odd.environment_seed);
        assert_eq!(even.environment_seed, NATIVE_PAIR_ROOT);
        assert_eq!(
            paired.input.even_episode.pair_environment_seed,
            NATIVE_PAIR_ROOT
        );
        assert_eq!(
            paired.input.odd_episode.pair_environment_seed,
            NATIVE_PAIR_ROOT
        );
        assert_eq!(paired.expected_shared_reset_case_name, NATIVE_CASE_NAME);
        assert_eq!(
            case_by_name(&artifact, NATIVE_CASE_NAME)
                .input
                .pair_environment_seed,
            NATIVE_PAIR_ROOT
        );

        // Only the learner role swaps: physical deck inputs never do.
        for episode in [&paired.input.even_episode, &paired.input.odd_episode] {
            assert_eq!(episode.p0_deck_id, CANONICAL_BURN_DECK_ID);
            assert_eq!(episode.p1_deck_id, CANONICAL_RALLY_DECK_ID);
        }

        // Identical direct GameState and identical physical projections.
        let (burn, rally) = burn_and_rally();
        let even_state =
            crate::rl::build_deck_pair_state_environment_v2(even.environment_seed, &burn, &rally)
                .expect("even episode state builds");
        let odd_state =
            crate::rl::build_deck_pair_state_environment_v2(odd.environment_seed, &burn, &rally)
                .expect("odd episode state builds");
        assert_eq!(even_state, odd_state, "the paired episodes share one state");
        for player in [PlayerId::P0, PlayerId::P1] {
            assert_eq!(
                hand_definition_order(&even_state, player),
                hand_definition_order(&odd_state, player)
            );
            assert_eq!(
                library_definition_order(&even_state, player),
                library_definition_order(&odd_state, player)
            );
        }

        let shared = case_by_name(&artifact, NATIVE_CASE_NAME);
        assert_eq!(
            hand_definition_order(&even_state, PlayerId::P0),
            shared
                .expected_projection
                .p0
                .opening_hand_card_definition_ids
        );
        assert_eq!(
            hand_definition_order(&even_state, PlayerId::P1),
            shared
                .expected_projection
                .p1
                .opening_hand_card_definition_ids
        );
    }

    #[test]
    fn schedule_identity_is_an_artifact_binding_not_the_version_atom() {
        // Substituting the schedule identity for the version atom yields the
        // wrong root; the frozen derivation must keep them distinct.
        let mut hasher = Sha256::new();
        for (tag, payload) in [
            ("version", NATIVE_SCHEDULE_IDENTITY.as_bytes()),
            ("namespace", b"train-env".as_slice()),
            ("field-name", b"base_seed".as_slice()),
        ] {
            hasher.update((tag.len() as u32).to_be_bytes());
            hasher.update(tag.as_bytes());
            hasher.update((payload.len() as u64).to_be_bytes());
            hasher.update(payload);
        }
        hasher.update(3_u32.to_be_bytes());
        hasher.update(b"u63");
        hasher.update(8_u64.to_be_bytes());
        hasher.update(NATIVE_BASE_SEED.to_be_bytes());
        let tag = "field-name";
        let payload: &[u8] = b"pair_index";
        hasher.update((tag.len() as u32).to_be_bytes());
        hasher.update(tag.as_bytes());
        hasher.update((payload.len() as u64).to_be_bytes());
        hasher.update(payload);
        hasher.update(3_u32.to_be_bytes());
        hasher.update(b"u63");
        hasher.update(8_u64.to_be_bytes());
        hasher.update(0_u64.to_be_bytes());
        let digest = hasher.finalize();
        let mut prefix = [0_u8; 8];
        prefix.copy_from_slice(&digest[..8]);
        let wrong = u64::from_be_bytes(prefix) & ((1_u64 << 63) - 1);
        assert_eq!(wrong, 3_926_161_255_480_587_309);
        assert_ne!(wrong, NATIVE_PAIR_ROOT);
    }

    // ---- 7. stored rejects plus in-memory strictness rejects -------------

    #[test]
    fn every_stored_reject_executes_with_its_frozen_code() {
        let artifact = sealed_artifact();
        let expected = [
            ("paired-role-learner-seat-not-swapped", CODE_SEAT),
            ("paired-role-odd-environment-seed-drift", CODE_ROOT),
            ("paired-role-odd-physical-decks-swapped", CODE_DECKS),
            ("reset-source-permutation-duplicate-index", CODE_BIJECTION),
            ("reset-source-permutation-index-out-of-range", CODE_RANGE),
            (
                "reset-source-permutation-projection-mismatch",
                CODE_PROJECTION,
            ),
        ];
        let observed_names: Vec<&str> = artifact
            .reject_cases
            .iter()
            .map(|case| case.name.as_str())
            .collect();
        let expected_names: Vec<&str> = expected.iter().map(|(name, _)| *name).collect();
        assert_eq!(observed_names, expected_names, "name-sorted reject order");

        for (case, (_, code)) in artifact.reject_cases.iter().zip(&expected) {
            assert_eq!(case.expected_rejection, *code, "{}", case.name);
            validate_stored_reject(case, &artifact)
                .expect("stored reject is its complete frozen reconstruction");
            let observed = classify_reject(&case.input, &artifact)
                .expect("stored reject input must be rejected");
            assert_eq!(observed, *code, "{}", case.name);
        }
    }

    #[test]
    fn duplicate_index_reject_breaks_bijection_without_touching_projection() {
        let artifact = sealed_artifact();
        let positive = case_by_name(&artifact, ROOT_940001_CASE_NAME);
        let reject = artifact
            .reject_cases
            .iter()
            .find(|case| case.name == "reset-source-permutation-duplicate-index")
            .expect("duplicate reject exists");
        let body = match &reject.input {
            RejectInput::ResetProjection(body) => body,
            RejectInput::PairedRole(_) => panic!("duplicate reject is a reset projection"),
        };
        let positive_permutation = &positive.expected_projection.p0.source_index_permutation;
        let reject_permutation = &body.expected_projection.p0.source_index_permutation;
        let source = &positive.input.p0.source_card_definition_ids;

        assert_eq!(
            (positive_permutation[0], positive_permutation[17]),
            (36, 37)
        );
        assert_eq!((reject_permutation[0], reject_permutation[17]), (36, 36));
        assert_eq!((source[36], source[37]), (47, 47));

        // Every projected card byte is preserved, so only the bijection breaks.
        assert_eq!(
            body.expected_projection.p0.card_definition_id_permutation,
            positive
                .expected_projection
                .p0
                .card_definition_id_permutation
        );
        let mut sorted = reject_permutation.clone();
        sorted.sort_unstable();
        let identity: Vec<u16> = (0..60).collect();
        assert_ne!(
            sorted, identity,
            "the rejected permutation is not a bijection"
        );
    }

    #[test]
    fn in_memory_duplicate_case_names_are_rejected() {
        let mut artifact = sealed_artifact();
        artifact.reset_cases[1].name = artifact.reset_cases[0].name.clone();
        let error = validate_artifact_structure(&artifact)
            .expect_err("duplicate case names must be rejected");
        assert!(error.contains("strictly increasing"), "observed {error}");
    }

    #[test]
    fn in_memory_out_of_order_case_names_are_rejected() {
        let mut artifact = sealed_artifact();
        artifact.reset_cases.swap(0, 1);
        let error = validate_artifact_structure(&artifact)
            .expect_err("out-of-order case names must be rejected");
        assert!(error.contains("strictly increasing"), "observed {error}");
    }

    #[test]
    fn in_memory_malformed_lowercase_hex_is_rejected() {
        let mut artifact = sealed_artifact();
        artifact.reset_cases[0]
            .input
            .p0
            .runtime_deck_hash_u64_hex
            .make_ascii_uppercase();
        let error = validate_artifact_structure(&artifact)
            .expect_err("uppercase deck hash must be rejected");
        assert!(error.contains("16 lowercase hex"), "observed {error}");

        let mut artifact = sealed_artifact();
        artifact
            .source_authorities
            .runtime_deck_catalog
            .raw_file_sha256
            .push('a');
        let error = validate_artifact_structure(&artifact)
            .expect_err("wrong-width SHA-256 must be rejected");
        assert!(error.contains("64 lowercase hex"), "observed {error}");
    }

    #[test]
    fn in_memory_exact_authorities_and_projection_contract_are_enforced() {
        let mut artifact = sealed_artifact();
        artifact.source_authorities.runtime_deck_catalog.protocol =
            "canonical-mainboard-bo1/v2".to_owned();
        let error = validate_artifact_structure(&artifact)
            .expect_err("well-formed authority drift must be rejected");
        assert!(
            error.contains("source authorities mismatch"),
            "observed {error}"
        );

        let mut artifact = sealed_artifact();
        artifact.projection_contract.library_order = "index-zero-is-last-draw".to_owned();
        let error = validate_artifact_structure(&artifact)
            .expect_err("well-formed projection-contract drift must be rejected");
        assert!(
            error.contains("physical projection contract mismatch"),
            "observed {error}"
        );
    }

    #[test]
    fn in_memory_wrong_exact_counts_are_rejected() {
        let mut artifact = sealed_artifact();
        artifact.reset_cases.pop();
        assert!(validate_artifact_structure(&artifact).is_err());

        let mut artifact = sealed_artifact();
        artifact.paired_role_cases.clear();
        assert!(validate_artifact_structure(&artifact).is_err());

        let mut artifact = sealed_artifact();
        artifact.reject_cases.pop();
        assert!(validate_artifact_structure(&artifact).is_err());
    }

    #[test]
    fn reset_and_paired_body_ceilings_cover_positive_and_reject_shapes() {
        let artifact = sealed_artifact();
        let base = cloned_reset_body(&artifact, ROOT_940001_CASE_NAME)
            .expect("positive reset body exists");

        let mut body = base.clone();
        body.expected_projection
            .p0
            .source_index_permutation
            .resize(MAX_CARDS_PER_DECK + 1, 0);
        assert!(
            validate_reset_body_ceilings(&body, "oversized").is_err(),
            "source-index ceiling"
        );

        let mut body = base.clone();
        body.expected_projection
            .p0
            .card_definition_id_permutation
            .resize(MAX_CARDS_PER_DECK + 1, 0);
        assert!(
            validate_reset_body_ceilings(&body, "oversized").is_err(),
            "card-definition ceiling"
        );

        let mut body = base.clone();
        body.expected_projection
            .p0
            .opening_hand_card_definition_ids
            .resize(MAX_CARDS_PER_DECK + 1, 0);
        assert!(
            validate_reset_body_ceilings(&body, "oversized").is_err(),
            "opening-hand ceiling"
        );

        let mut body = base.clone();
        body.expected_projection
            .p0
            .remaining_library_card_definition_ids
            .resize(MAX_CARDS_PER_DECK + 1, 0);
        assert!(
            validate_reset_body_ceilings(&body, "oversized").is_err(),
            "remaining-library ceiling"
        );

        let mut body = base;
        let first_draw = body.expected_projection.draw_events[0];
        body.expected_projection
            .draw_events
            .resize(MAX_DRAW_EVENTS + 1, first_draw);
        assert!(
            validate_reset_body_ceilings(&body, "oversized").is_err(),
            "draw-event ceiling"
        );

        let mut paired =
            cloned_paired_body(&artifact, PAIRED_CASE_NAME).expect("positive paired body exists");
        paired.input.odd_episode.p0_deck_id = "x".repeat(65);
        assert!(
            validate_episode_deck_ids(&paired.input, "oversized").is_err(),
            "paired reject deck-id ceiling"
        );
    }

    #[test]
    fn physical_input_owner_binding_and_extra_reject_leaves_are_rejected() {
        let artifact = sealed_artifact();
        let mut body = cloned_reset_body(&artifact, ROOT_940001_CASE_NAME)
            .expect("positive reset body exists");
        body.input.p0.physical_owner = Owner::P1;
        assert_eq!(validate_reset_body(&body), Some(CODE_TRAJECTORY));
        assert!(
            validate_reset_body_ceilings(&body, "owner-mismatch").is_err(),
            "the structural path binds p0 input to physical owner p0"
        );

        let mut reject = artifact.reject_cases[0].clone();
        match &mut reject.input {
            RejectInput::PairedRole(body) => {
                body.expected_shared_reset_case_name = ROOT_940001_CASE_NAME.to_owned();
            }
            RejectInput::ResetProjection(_) => panic!("first frozen reject is paired-role"),
        }
        let error = validate_stored_reject(&reject, &artifact)
            .expect_err("an extra reject leaf must fail complete typed equality");
        assert!(
            error.contains("exact frozen reconstruction"),
            "observed {error}"
        );
    }

    #[test]
    fn raw_unknown_field_is_rejected_by_the_typed_decoder() {
        // Mutates raw JSON bytes rather than attempting an impossible
        // in-memory typed-struct mutation.
        let raw = ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_V1;
        let mutated = raw.replacen('{', "{\"unexpected_field\":1,", 1);
        assert!(
            serde_json::from_str::<Artifact>(&mutated).is_err(),
            "deny_unknown_fields must reject an unknown top-level field"
        );

        let nested = raw.replacen(
            "\"projection_contract\":{",
            "\"projection_contract\":{\"unexpected_nested\":1,",
            1,
        );
        assert_ne!(nested, raw, "the nested mutation must apply");
        assert!(
            serde_json::from_str::<Artifact>(&nested).is_err(),
            "deny_unknown_fields must reject an unknown nested field"
        );

        let reject_wrapper = raw.replacen(
            "\"input\":{\"case\":",
            "\"input\":{\"unexpected_wrapper_field\":0,\"case\":",
            1,
        );
        assert_ne!(
            reject_wrapper, raw,
            "the reject-wrapper mutation must apply"
        );
        assert!(
            serde_json::from_str::<Artifact>(&reject_wrapper).is_err(),
            "deny_unknown_fields must reject an unknown adjacent-tag field"
        );
    }

    #[test]
    fn raw_duplicate_key_is_rejected_before_typed_decoding() {
        let raw = ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_V1;
        let mutated = raw.replacen('{', "{\"schema\":\"x\",", 1);
        assert!(
            reject_duplicate_keys(&mutated).is_err(),
            "duplicate top-level key must be rejected before typed decoding"
        );
        // A Value-first parser would collapse this duplicate under
        // last-key-wins. The explicit scan guarantees duplicate rejection
        // independently of the typed struct decoder.
        assert!(reject_duplicate_keys(raw).is_ok());
    }

    #[test]
    fn raw_float_literal_is_rejected_before_typed_decoding() {
        let raw = ENVIRONMENT_RANDOMIZATION_RESET_TRAJECTORY_GOLDENS_V1;
        let mutated = raw.replacen(
            "\"initial_shuffle_ordinal\":0",
            "\"initial_shuffle_ordinal\":0.0",
            1,
        );
        assert_ne!(mutated, raw, "the float mutation must apply");
        assert!(
            reject_duplicate_keys(&mutated).is_err(),
            "float literals must be rejected"
        );
    }

    #[test]
    fn in_memory_reset_precedence_is_range_then_bijection_then_projection() {
        let artifact = sealed_artifact();
        let positive = case_by_name(&artifact, ROOT_940001_CASE_NAME);
        let base = ResetCaseBody {
            input: positive.input.clone(),
            expected_projection: positive.expected_projection.clone(),
        };
        assert_eq!(validate_reset_body(&base), None);

        // 60 is out of range and also breaks the bijection: range wins.
        let mut body = base.clone();
        body.expected_projection.p0.source_index_permutation[0] = 60;
        assert_eq!(validate_reset_body(&body), Some(CODE_RANGE));

        // In-range duplicate: bijection wins over projection.
        let mut body = base.clone();
        body.expected_projection.p0.source_index_permutation[17] = 36;
        assert_eq!(validate_reset_body(&body), Some(CODE_BIJECTION));

        // Bijection intact, projection disturbed.
        let mut body = base.clone();
        body.expected_projection.p0.card_definition_id_permutation[0] = 37;
        assert_eq!(validate_reset_body(&body), Some(CODE_PROJECTION));

        // Truncated permutation: length wins.
        let mut body = base.clone();
        body.expected_projection.p0.source_index_permutation.pop();
        assert_eq!(validate_reset_body(&body), Some(CODE_LENGTH));

        // Trajectory inconsistency.
        let mut body = base;
        body.expected_projection.next_live_shuffle_ordinals = [1, 0];
        assert_eq!(validate_reset_body(&body), Some(CODE_TRAJECTORY));
    }

    #[test]
    fn in_memory_paired_precedence_is_frozen() {
        let artifact = sealed_artifact();
        let paired = &artifact.paired_role_cases[0];
        let base = PairedRoleCaseBody {
            input: paired.input.clone(),
            expected_shared_reset_case_name: paired.expected_shared_reset_case_name.clone(),
        };
        assert_eq!(validate_paired_body(&base, &artifact), None);

        let mut body = base.clone();
        body.input.trainer_schedule_identity = "wrong-identity".to_string();
        assert_eq!(
            validate_paired_body(&body, &artifact),
            Some(CODE_SCHEDULE_IDENTITY)
        );

        let mut body = base.clone();
        body.input.odd_episode.episode_index = 5;
        assert_eq!(validate_paired_body(&body, &artifact), Some(CODE_EPISODE));

        let mut body = base.clone();
        body.input.odd_episode.learner_seat = Owner::P0;
        assert_eq!(validate_paired_body(&body, &artifact), Some(CODE_SEAT));

        let mut body = base.clone();
        body.input.odd_episode.pair_environment_seed = NATIVE_PAIR_ROOT - 1;
        assert_eq!(validate_paired_body(&body, &artifact), Some(CODE_ROOT));

        let mut body = base.clone();
        body.expected_shared_reset_case_name = ROOT_940001_CASE_NAME.to_string();
        assert_eq!(validate_paired_body(&body, &artifact), Some(CODE_SHARED));

        let mut body = base.clone();
        body.input.odd_episode.p0_deck_id = CANONICAL_RALLY_DECK_ID.to_string();
        body.input.odd_episode.p1_deck_id = CANONICAL_BURN_DECK_ID.to_string();
        assert_eq!(validate_paired_body(&body, &artifact), Some(CODE_DECKS));

        let mut body = base.clone();
        body.input.even_episode.episode_index = U63_MAX + 1;
        body.input.odd_episode.episode_index = U63_MAX + 2;
        assert_eq!(validate_paired_body(&body, &artifact), Some(CODE_EPISODE));

        let mut body = base;
        body.input.base_seed = U63_MAX + 1;
        assert_eq!(validate_paired_body(&body, &artifact), Some(CODE_ROOT));
    }
}
