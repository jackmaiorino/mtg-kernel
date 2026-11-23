//! Card-wave registry transfer of an expanded-deck trainer checkpoint.
//!
//! Card ids form one append-only space: the Pauper registry, then the
//! Foundations or Standard registry, then (under `eternal-fixtures`) the
//! eternal registry, concatenated by `build.rs`. A card wave appends rows to
//! that space and so changes `KERNEL_CARDDB_HASH`, which every checkpoint
//! records and every loader requires to equal the running build's. This
//! module carries one checkpoint across one appended wave, following the
//! method of `phase1_registry_transfer_v1` (prove the old registry is a
//! prefix of the new one, keep every existing bit, initialise appended rows
//! from SHA-256 of their name and a seed, zero their optimizer moments) but
//! for any ordinary checkpoint the trainer writes on main: either checkpoint
//! schema (imported or fresh origin), the terminal, GAE and recovery losses,
//! every feature contract the trainer's own resolver reads (V3, V4, FlatV7,
//! FlatV8, no-state-digest) with that contract's model layout (net-8, or the
//! V8 inclusion net for FlatV8), into the registry of whatever build runs the
//! transfer (default, `limited-fdn-fixtures`, `standard-magezero-fixtures` or
//! `eternal-fixtures`).
//!
//! Three bindings make the transfer refuse anything that is not that wave:
//!
//! - The destination is the running build: its `KERNEL_CARDDB_HASH` and its
//!   registry files byte for byte, which this module embeds the way
//!   `build.rs` reads them ([`COMPILED_REGISTRY`]).
//! - The checkpoint is one the trainer would restore: its feature contract,
//!   input contract, loss, scalars, tensor layout and state digest pass the
//!   trainer's own checkpoint checks, not copies of them.
//! - The source registry is the checkpoint's own: its origin records the
//!   source card database, the first registry file's bytes and a card count,
//!   and no row past the source registry holds optimizer state. The origin
//!   never went through a registry transfer, and its feature identity is of
//!   the checkpoint's own feature generation.
//!
//! The transfer is a pure function of its inputs: it reads and writes no
//! file. Its output is an ordinary checkpoint with the same fields as its
//! source except `card_db_hash`, `state_sha256` and the appended card rows
//! of the card embedding and its two moments. Nothing else, including the
//! warm-start provenance (`source_import`), is rewritten, so a model source
//! resumes the result only once its origin is re-bound to the destination
//! build, which this module does not do (`docs/CARD-WAVE-TRANSFER.md`,
//! Limits). The transfer record ([`CardWaveTransferRecordV1`]) is a separate
//! document, so the strict checkpoint reader and every existing checkpoint's
//! bytes are unchanged.

use super::*;
use crate::card_def::CARD_DEFS;
use crate::native_policy_value_net_v1::{CARD_EMBEDDING_DIM_V1, CARD_VOCAB_SIZE_V1};
use crate::phase1_registry_transfer_v1::{
    initial_card_row, registry_document_cards, strict_registry_document, INITIALIZER,
};
use crate::sideboard_play_policy_v1::{fresh_lineage_generation_v1, FreshLineageGenerationV1};
use std::io::Read as _;

const RECORD_SCHEMA: &str = "mtg-kernel-expanded-card-wave-transfer/v1";
const CLI_REQUEST_SCHEMA: &str = "mtg-kernel-expanded-card-wave-transfer-cli-request/v1";
const CLI_RESULT_SCHEMA: &str = "mtg-kernel-expanded-card-wave-transfer-cli-result/v1";
const REGISTRY_IDENTITY_DOMAIN: &str = "mtg-kernel-card-wave-registry-segments/v1";
const MAPPING_RULE: &str = "exact-existing-card-record-prefix; card_token=card_db_id+1; appended-rows-initialized-moments-zeroed; all-other-bits-preserved";
const CARD_EMBEDDING_NAME: &str = "card_embedding.weight";
/// The registry schema version `build.rs` compiles (its
/// `EXPECTED_SCHEMA_VERSION`). A registry file of another version is not one
/// a build of this source reads.
const REGISTRY_SCHEMA_VERSION: u64 = 2;
/// Registry fields that `build.rs` never reads: its `CardJson` does not
/// declare them and admits unknown fields, so they are outside
/// `KERNEL_CARDDB_HASH`. Every other field must be unchanged. That includes
/// fields this list does not name, even ones `CardJson` does not declare
/// either (`object_name`, `scryfall_id` and `oracle_id` of the Foundations
/// registry): comparing them can only refuse more.
const METADATA_FIELDS: [&str; 3] = ["complexity", "java_file", "sideboard_only"];
const MODEL_SHAPE_REFUSAL: &str = "checkpoint model shape differs from the train-state layout of its feature contract; card-wave transfer never reshapes a model";
const MAX_CLI_REQUEST_BYTES: usize = 1024 * 1024;
/// The schema an imported origin takes once the phase-1 registry transfer
/// has re-bound it (`FrozenPlayPolicyV1::from_registry_transfer_v1`): it
/// stays `Imported`, so only this schema tells it from an untransferred one.
const REGISTRY_TRANSFERRED_IMPORT_SCHEMA: &str = "mtg-kernel-registry-transferred-play/v1";
const TERMINAL_LOSS_IDENTITY: &str = "terminal_reinforce_value/v3";

const PAUPER_REGISTRY: &[u8] = include_bytes!("../../../data/cards_v1.json");
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
const FDN_REGISTRY: &[u8] = include_bytes!("../../../data/limited/fdn_v1/cards_v1.json");
#[cfg(feature = "standard-magezero-fixtures")]
const STANDARD_REGISTRY: &[u8] = include_bytes!("../../../data/standard/magezero_v1/cards_v1.json");
#[cfg(feature = "eternal-fixtures")]
const ETERNAL_REGISTRY: &[u8] = include_bytes!("../../../data/eternal_v1/cards_v1.json");

/// This build's registry files, byte for byte, in the order `build.rs`
/// concatenates them into the card id space: the Pauper registry, then under
/// `limited-fdn-fixtures` the Foundations registry or (under
/// `standard-magezero-fixtures`) the Standard one in its place, then under
/// `eternal-fixtures` the eternal registry.
#[cfg(not(feature = "limited-fdn-fixtures"))]
const COMPILED_REGISTRY: &[&[u8]] = &[PAUPER_REGISTRY];
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
const COMPILED_REGISTRY: &[&[u8]] = &[PAUPER_REGISTRY, FDN_REGISTRY];
#[cfg(all(
    feature = "standard-magezero-fixtures",
    not(feature = "eternal-fixtures")
))]
const COMPILED_REGISTRY: &[&[u8]] = &[PAUPER_REGISTRY, STANDARD_REGISTRY];
#[cfg(feature = "eternal-fixtures")]
const COMPILED_REGISTRY: &[&[u8]] = &[PAUPER_REGISTRY, STANDARD_REGISTRY, ETERNAL_REGISTRY];

/// Pins of one transfer. Registry identities are [`registry_identity_v1`]
/// of each build's registry files in build order, never of one file alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardWaveTransferRequestV1 {
    pub source_checkpoint_sha256: String,
    /// The `card_db_hash` the source checkpoint records.
    pub source_card_db_hash: String,
    pub source_registry_sha256: String,
    /// Must equal this build's `KERNEL_CARDDB_HASH`.
    pub destination_card_db_hash: String,
    /// Must be the identity of this build's own registry files.
    pub destination_registry_sha256: String,
    /// The source checkpoint's own feature identity. A transfer never
    /// migrates features, so these are only compared.
    pub feature_contract_digest: String,
    pub feature_encoding_digest: String,
    pub initialization_seed: u64,
}

/// One appended registry row and the embedding row it initialised.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardWaveAppendedCardV1 {
    pub card_id: u16,
    pub embedding_row: u32,
    pub name: String,
}

/// The transfer record kept beside the destination checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardWaveTransferRecordV1 {
    pub schema: String,
    pub request: CardWaveTransferRequestV1,
    pub mapping_rule: String,
    pub initializer: String,
    pub source_registry_segment_sha256: Vec<String>,
    pub destination_registry_segment_sha256: Vec<String>,
    pub source_card_count: usize,
    pub destination_card_count: usize,
    pub appended_cards: Vec<CardWaveAppendedCardV1>,
    pub checkpoint_schema: String,
    pub loss_identity: String,
    pub adam_step: u64,
    pub source_state_sha256: String,
    pub destination_state_sha256: String,
    pub destination_checkpoint_sha256: String,
    /// Scalars of each state section (parameters, first moments, second
    /// moments) copied bit for bit.
    pub preserved_scalar_count_per_state_section: usize,
    /// Scalars of each state section in appended card rows.
    pub initialized_scalar_count_per_state_section: usize,
}

/// A validated transfer: the destination checkpoint (a v1 JSON checkpoint
/// document) and its record.
#[derive(Debug)]
pub struct CardWaveTransferV1 {
    checkpoint_bytes: Vec<u8>,
    record: CardWaveTransferRecordV1,
}

impl CardWaveTransferV1 {
    pub fn checkpoint_bytes_v1(&self) -> &[u8] {
        &self.checkpoint_bytes
    }

    pub fn record_v1(&self) -> &CardWaveTransferRecordV1 {
        &self.record
    }

    /// The record's one serialization: what the transfer writes and the only
    /// bytes [`verify_card_wave_transfer_v1`] accepts for it.
    pub fn record_bytes_v1(&self) -> Result<Vec<u8>, String> {
        let mut bytes = serde_json::to_vec_pretty(&self.record).map_err(err)?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

/// The registry a transfer binds as its destination: the build's registry
/// files and its card names in id order. [`transfer_card_wave_checkpoint_v1`]
/// binds the running build; the card database is always the running build's.
struct DestinationBindingV1<'a> {
    segments: Vec<&'a [u8]>,
    names: Vec<&'a str>,
}

fn compiled_destination_v1() -> DestinationBindingV1<'static> {
    DestinationBindingV1 {
        segments: COMPILED_REGISTRY.to_vec(),
        names: CARD_DEFS.iter().map(|card| card.name).collect(),
    }
}

/// SHA-256 over a domain tag, the segment count and every segment's SHA-256,
/// in build order (Pauper, then Foundations or Standard, then eternal).
pub fn registry_identity_v1(segments: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(REGISTRY_IDENTITY_DOMAIN.as_bytes());
    hasher.update([0]);
    hasher.update((segments.len() as u64).to_le_bytes());
    for segment in segments {
        hasher.update(Sha256::digest(segment));
    }
    hex(&hasher.finalize())
}

fn card_name_v1(card: &Value) -> Result<&str, String> {
    let name = card.get("name").and_then(Value::as_str);
    name.ok_or_else(|| "card name absent".to_string())
}

fn has_name_v1(card: &Value, name: &str) -> bool {
    card.get("name").and_then(Value::as_str) == Some(name)
}

/// The cards of one build's registry files in id order, and how many of them
/// its first file holds.
struct RegistryCardsV1 {
    cards: Vec<Value>,
    first_segment_cards: usize,
}

/// The concatenated `cards` of the segments. Each file is read by the
/// existing strict registry reader (duplicate keys, empty or ambiguous names
/// refused) and must be of the schema version `build.rs` compiles; names are
/// unique across the whole id space.
fn registry_v1(segments: &[&[u8]]) -> Result<RegistryCardsV1, String> {
    ensure(
        !segments.is_empty(),
        "card-wave transfer requires at least one registry segment",
    )?;
    let mut cards = Vec::new();
    let mut first_segment_cards = 0;
    for (ordinal, segment) in segments.iter().enumerate() {
        let document = strict_registry_document(segment)?;
        ensure(
            document.get("version").and_then(Value::as_u64) == Some(REGISTRY_SCHEMA_VERSION),
            "registry file is not of the schema version build.rs compiles",
        )?;
        cards.extend(registry_document_cards(&document)?);
        if ordinal == 0 {
            first_segment_cards = cards.len();
        }
    }
    ensure(
        cards.len() < CARD_VOCAB_SIZE_V1,
        "registry exceeds the card id space of the embedding table",
    )?;
    let mut names = BTreeSet::new();
    for card in &cards {
        ensure(
            names.insert(card_name_v1(card)?),
            "duplicate card name across registry segments",
        )?;
    }
    Ok(RegistryCardsV1 {
        cards,
        first_segment_cards,
    })
}

/// The first field (in sorted order) of two records of one card whose value
/// differs, ignoring only [`METADATA_FIELDS`]. An absent field differs from
/// a present one, even one equal to its build-time default.
fn changed_covered_field_v1(old: &Value, new: &Value) -> Result<Option<String>, String> {
    let old = old.as_object().ok_or("registry card is not an object")?;
    let new = new.as_object().ok_or("registry card is not an object")?;
    let keys: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    for key in keys {
        if !METADATA_FIELDS.contains(&key.as_str()) && old.get(key) != new.get(key) {
            return Ok(Some(key.clone()));
        }
    }
    Ok(None)
}

/// The appended destination rows, after proving `source` is an exact prefix
/// of `destination` in every field but [`METADATA_FIELDS`]. Refuses, in id
/// order, the first removed, renamed, reordered or changed row.
fn appended_v1<'a>(source: &[Value], destination: &'a [Value]) -> Result<&'a [Value], String> {
    for (id, old) in source.iter().enumerate() {
        let old_name = card_name_v1(old)?;
        let Some(new) = destination.get(id) else {
            return Err(format!(
                "card id {id} ({old_name:?}) is removed; only appended cards are transferable"
            ));
        };
        let new_name = card_name_v1(new)?;
        if old_name != new_name {
            if destination.iter().any(|card| has_name_v1(card, old_name)) {
                return Err(format!(
                    "card id {id} ({old_name:?}) moved: the registry is reordered; only appended cards are transferable"
                ));
            }
            return Err(format!(
                "card id {id} ({old_name:?}) is removed or renamed to {new_name:?}; only appended cards are transferable"
            ));
        }
        if let Some(field) = changed_covered_field_v1(old, new)? {
            return Err(format!(
                "card id {id} ({old_name:?}) changed field {field:?}, which is not a field the card database hash is known to ignore; only appended cards are transferable"
            ));
        }
    }
    Ok(&destination[source.len()..])
}

/// The source checkpoint, in either format and sealed or not, through the
/// trainer's own strict reader, with nothing the transfer would drop:
///
/// - an E, soft-Q, search-target or EX1 continuation member, read by the
///   trainer's own probe, for which an explicit `null` is a member too (the
///   typed reader takes it for absence);
/// - a v1 JSON document that is not the canonical serialization of the
///   checkpoint it parses to (a `null` member, a member of the origin the
///   reader skips, other spacing), since the destination is that
///   serialization. A container is canonical by its own structure check.
fn source_checkpoint_v1(checkpoint_bytes: &[u8]) -> Result<ExpandedCheckpointV1, String> {
    let record = record_envelope_v1::open_v1(checkpoint_bytes, MAX_FILE_BYTES)?;
    let probe: CheckpointESupportedProbeV1 =
        serde_json::from_slice(&checkpoint_container_v1::metadata_view_v1(&record)?)
            .map_err(err)?;
    ensure(
        probe.e_supported.is_none()
            && probe.soft_q.is_none()
            && probe.search_targets.is_none()
            && probe.ex1.is_none(),
        "checkpoint names an E, soft-Q, search-target or EX1 continuation member (an explicit null counts); card-wave transfer refuses it",
    )?;
    let saved = checkpoint_container_v1::parse_checkpoint_v1(&record)?;
    if !checkpoint_container_v1::is_container_v1(&record) {
        let canonical = serde_json::to_vec(&saved).map_err(err)?;
        ensure(
            canonical.as_slice() == &*record,
            "v1 JSON source checkpoint is not the canonical serialization of the checkpoint it parses to; card-wave transfer would not keep its bytes",
        )?;
    }
    Ok(saved)
}

/// The checkpoint is an ordinary one of an untransferred origin, without
/// continuation provenance bound to its own card database. An import the
/// phase-1 registry transfer re-bound is still `Imported`, under its own
/// schema ([`REGISTRY_TRANSFERRED_IMPORT_SCHEMA`]), and is refused as the
/// transferred fresh origin is.
fn check_ordinary_checkpoint_v1(saved: &ExpandedCheckpointV1) -> Result<(), String> {
    let untransferred = match &saved.source_import {
        PlayPolicyOriginV1::Imported(identity) => {
            identity.schema != REGISTRY_TRANSFERRED_IMPORT_SCHEMA
        }
        PlayPolicyOriginV1::FreshInitialization(_) => true,
        PlayPolicyOriginV1::TransferredFreshInitialization(_) => false,
    };
    ensure(
        untransferred,
        "card-wave transfer reads only a checkpoint of an imported or fresh origin that never went through a registry transfer",
    )?;
    ensure(
        saved.schema == ordinary_checkpoint_schema_v1(&saved.source_import),
        "card-wave transfer reads only an ordinary expanded-deck checkpoint of an imported or fresh origin, under that origin's checkpoint schema",
    )?;
    ensure(
        saved.registry_transfer.is_none() && saved.archive_distillation.is_none(),
        "checkpoint carries registry-transfer or archive-distillation provenance bound to its own card database; card-wave transfer refuses it",
    )
}

/// The trainer's own checks of a checkpoint record that read no policy, run
/// against the destination card database the record is about to name: the
/// feature and input contract (`no_state_digest_v1::resolve_contract_v1`, as
/// `check_checkpoint_fields_v1` calls it), the stored no-state-digest
/// columns, and the loss with the scalars it requires
/// (`check_checkpoint_loss_fields_v1`). The learning rate and value
/// coefficient are positive and finite, as every update that wrote an
/// ordinary checkpoint required. Returns whether the contract is FlatV8,
/// whose model is the V8 inclusion net.
fn check_trainer_record_v1(
    saved: &ExpandedCheckpointV1,
    card_db_hash: &str,
) -> Result<bool, String> {
    let resolved = no_state_digest_v1::resolve_contract_v1(
        &saved.feature_contract_digest,
        &saved.feature_encoding_digest,
        card_db_hash,
        saved.input_contract.as_ref(),
    )
    .map_err(|error| {
        format!("checkpoint feature contract is not one the trainer reads: {error}")
    })?;
    if saved.input_contract.is_some() {
        no_state_digest_v1::check_saved_columns_v1(saved)?;
    }
    check_origin_features_v1(&saved.source_import, resolved.generation)?;
    check_checkpoint_loss_fields_v1(saved)?;
    check_loss_scalars_v1(saved)?;
    for bits in [saved.learning_rate_bits, saved.value_coefficient_bits] {
        let value = f32::from_bits(bits);
        ensure(
            value.is_finite() && value > 0.0,
            "checkpoint learning rate or value coefficient is not positive and finite",
        )?;
    }
    Ok(resolved.base_contract_digest
        == crate::native_flat_tensorizer_v8::FEATURE_CONTRACT_DIGEST_V8)
}

/// The origin's feature identity is of the checkpoint's own feature
/// generation, as every trainer path that writes the checkpoint requires of
/// the policy it trains: V3 checkpoints come from a V3 policy, and V4, FlatV7
/// and FlatV8 ones (and the no-state-digest conversion of either generation)
/// from a V4 one, since FlatV7 and FlatV8 keep the V4 origin they convert.
/// An import records its contract and encoding digests; a fresh origin all
/// four feature fields, classified by the trainer's own
/// `fresh_lineage_generation_v1`.
fn check_origin_features_v1(
    origin: &PlayPolicyOriginV1,
    generation: FreshLineageGenerationV1,
) -> Result<(), String> {
    const REFUSAL: &str =
        "checkpoint origin's feature identity is not of the checkpoint's own feature generation";
    match origin {
        PlayPolicyOriginV1::Imported(identity) => {
            let expected = match generation {
                FreshLineageGenerationV1::V3 => {
                    (FEATURE_CONTRACT_DIGEST_V3, FEATURE_ENCODING_DIGEST_V3)
                }
                FreshLineageGenerationV1::V4 => {
                    (FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4)
                }
            };
            ensure(
                (
                    identity.feature_contract_digest.as_str(),
                    identity.feature_encoding_digest.as_str(),
                ) == expected,
                REFUSAL,
            )
        }
        PlayPolicyOriginV1::FreshInitialization(identity) => {
            let origin_generation = fresh_lineage_generation_v1(
                &identity.feature_contract_digest,
                &identity.feature_encoding_digest,
                &identity.features_source_sha256,
                &identity.feature_descriptor_sha256,
            )
            .map_err(|error| format!("{REFUSAL}: {error}"))?;
            ensure(origin_generation == generation, REFUSAL)
        }
        PlayPolicyOriginV1::TransferredFreshInitialization(_) => {
            Err("checkpoint origin already went through a registry transfer".into())
        }
    }
}

/// The loss scalars a trainer update writes, beyond the presence rule of
/// `check_checkpoint_loss_fields_v1` (whose conjunction admits one or two
/// stray scalars on the terminal loss): the terminal loss carries none of
/// them, and the GAE loss carries the selection its update validated
/// (`ExpandedLossSelectionV1::validate_v1`: finite gamma and lambda in
/// `0.0..=1.0`, entropy coefficient 0.0). The recovery loss's exact scalars
/// are already checked there (`validate_exploration_checkpoint_v1`).
fn check_loss_scalars_v1(saved: &ExpandedCheckpointV1) -> Result<(), String> {
    let scalars = (
        saved.gamma_bits,
        saved.gae_lambda_bits,
        saved.entropy_coefficient_bits,
    );
    if saved.loss_identity == TERMINAL_LOSS_IDENTITY {
        return ensure(
            scalars == (None, None, None),
            "checkpoint of the terminal loss carries a GAE scalar",
        );
    }
    if saved.loss_identity == GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1 {
        let (Some(gamma), Some(lambda), Some(entropy_coefficient)) = scalars else {
            return Err("checkpoint gae scalar presence does not match its loss_identity".into());
        };
        ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma: f32::from_bits(gamma),
            lambda: f32::from_bits(lambda),
            entropy_coefficient: f32::from_bits(entropy_coefficient),
        }
        .validate_v1()
        .map_err(|error| {
            format!("checkpoint GAE scalars are not ones an update writes: {error}")
        })?;
    }
    Ok(())
}

/// The checkpoint's state digest by the trainer's own restore
/// (`restore_checkpoint_fields_v1`): its sections against the model of its
/// feature contract (`checkpoint_tensors_v1`: tensor count, names, shapes
/// and lengths), the native validation and digest of those tensors (finite
/// values, non-negative second moments, zero padding row and gauge moments,
/// gauge anchor, Adam step domain), and the train state built from them.
fn restored_state_sha256_v1(saved: &ExpandedCheckpointV1, flat_v8: bool) -> Result<String, String> {
    let template = if flat_v8 {
        NativePolicyValueNetV1::v8_inclusion_template_v1().map_err(err)?
    } else {
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(err)?
    };
    let tensors = checkpoint_tensors_v1(saved, &template)
        .map_err(|error| format!("{MODEL_SHAPE_REFUSAL}: {error}"))?;
    let state_sha256 = tensors.state_sha256_v1().map_err(err)?;
    // The checkpoint's own parameters carry its gauge anchor, which the
    // restore requires of the model it builds the state on.
    let model = template
        .with_parameters_v1(&tensors.parameters)
        .map_err(err)?;
    NativePolicyValueTrainStateV1::from_tensors_v1(model, &tensors).map_err(err)?;
    Ok(hex(&state_sha256))
}

/// Binds the source checkpoint to the declared source registry through the
/// origin it records, which every origin the trainer loads stamps from its
/// own build: that build's card database hash, the SHA-256 of the registry
/// file it embeds (`data/cards_v1.json`, the first segment) and a card
/// count. A fresh origin counts the build's whole card id space; an import
/// counts the cards of that first file, which it was validated against.
fn bind_source_registry_v1(
    saved: &ExpandedCheckpointV1,
    first_segment: &[u8],
    source: &RegistryCardsV1,
) -> Result<(), String> {
    let origin = &saved.source_import;
    let recorded_cards = match origin {
        PlayPolicyOriginV1::Imported(_) => source.first_segment_cards,
        PlayPolicyOriginV1::FreshInitialization(_) => source.cards.len(),
        PlayPolicyOriginV1::TransferredFreshInitialization(_) => {
            return Err("checkpoint origin already went through a registry transfer".into())
        }
    };
    ensure(
        origin.destination_card_db_hash_v1() == saved.card_db_hash,
        "checkpoint origin records another card database than the checkpoint itself",
    )?;
    ensure(
        origin.destination_registry_sha256_v1() == sha(first_segment),
        "checkpoint is not bound to the supplied source registry: its origin records another first registry file",
    )?;
    ensure(
        origin.destination_card_count_v1() == recorded_cards,
        "checkpoint is not bound to the supplied source registry: its origin records another card count",
    )
}

/// Transfer a trainer checkpoint across one appended card wave into the
/// running build. `source_registry` and `destination_registry` are each
/// build's registry files in build order (Pauper first). The destination
/// registry must be this build's own registry files, byte for byte, and
/// `request.destination_card_db_hash` this build's hash.
pub fn transfer_card_wave_checkpoint_v1(
    checkpoint_bytes: &[u8],
    source_registry: &[&[u8]],
    destination_registry: &[&[u8]],
    request: &CardWaveTransferRequestV1,
) -> Result<CardWaveTransferV1, String> {
    transfer_to_destination_v1(
        checkpoint_bytes,
        source_registry,
        destination_registry,
        request,
        &compiled_destination_v1(),
    )
}

fn transfer_to_destination_v1(
    checkpoint_bytes: &[u8],
    source_registry: &[&[u8]],
    destination_registry: &[&[u8]],
    request: &CardWaveTransferRequestV1,
    destination: &DestinationBindingV1<'_>,
) -> Result<CardWaveTransferV1, String> {
    let build_card_db_hash = format!("{KERNEL_CARDDB_HASH:016x}");
    ensure(
        checkpoint_bytes.len() as u64 <= MAX_FILE_BYTES,
        "source checkpoint exceeds the checkpoint size bound",
    )?;
    ensure(
        sha(checkpoint_bytes) == request.source_checkpoint_sha256,
        "source checkpoint SHA256 differs",
    )?;
    ensure(
        registry_identity_v1(source_registry) == request.source_registry_sha256,
        "source registry identity differs",
    )?;
    ensure(
        registry_identity_v1(destination_registry) == request.destination_registry_sha256,
        "destination registry identity differs",
    )?;
    ensure(
        request.destination_card_db_hash == build_card_db_hash,
        "destination card database is not this build's KERNEL_CARDDB_HASH",
    )?;
    ensure(
        request.source_card_db_hash != request.destination_card_db_hash,
        "source and destination card databases are identical: there is no card wave to transfer, and a transferred checkpoint is never transferred again into its own build",
    )?;
    // The destination is the build itself: the very bytes it compiled, in
    // its own files, so no row or field of it is the caller's to supply.
    ensure(
        destination_registry.len() == destination.segments.len()
            && destination_registry
                .iter()
                .zip(&destination.segments)
                .all(|(supplied, compiled)| supplied == compiled),
        "destination registry is not this build's compiled registry: its files must be the build's own registry files, byte for byte and in build order",
    )?;

    let mut saved = source_checkpoint_v1(checkpoint_bytes)?;
    ensure(
        saved.card_db_hash == request.source_card_db_hash,
        "checkpoint card database differs from the declared source card database",
    )?;
    check_ordinary_checkpoint_v1(&saved)?;
    ensure(
        saved.feature_contract_digest == request.feature_contract_digest
            && saved.feature_encoding_digest == request.feature_encoding_digest,
        "checkpoint feature contract differs from the request; card-wave transfer never migrates features",
    )?;
    let flat_v8 = check_trainer_record_v1(&saved, &build_card_db_hash)?;

    let source = registry_v1(source_registry)?;
    let destination_cards = registry_v1(destination_registry)?.cards;
    ensure(
        destination_cards.len() == destination.names.len()
            && destination_cards
                .iter()
                .zip(&destination.names)
                .all(|(card, name)| has_name_v1(card, name)),
        "this build's compiled card names differ from its embedded registry",
    )?;
    let appended = appended_v1(&source.cards, &destination_cards)?;
    ensure(
        !appended.is_empty(),
        "no card is appended, yet the card database hash changed: a rules or field change of existing cards is not a card wave and is refused",
    )?;
    bind_source_registry_v1(&saved, source_registry[0], &source)?;

    // Validates the whole source state against its recorded digest before
    // any bit changes.
    let source_state_sha256 = restored_state_sha256_v1(&saved, flat_v8)?;
    ensure(
        source_state_sha256 == saved.state_sha256,
        "source checkpoint state digest differs",
    )?;
    ensure(
        saved.parameters[0].name == CARD_EMBEDDING_NAME
            && saved.parameters[0].shape == [CARD_VOCAB_SIZE_V1, CARD_EMBEDDING_DIM_V1],
        MODEL_SHAPE_REFUSAL,
    )?;
    // No card past the source registry was ever trained: every row from the
    // first appended one to the end of the table, still unassigned ones
    // included, holds +0.0 in both moments. A registry declared shorter than
    // the checkpoint's own would otherwise have its trained rows replaced.
    let first_unassigned = (source.cards.len() + 1) * CARD_EMBEDDING_DIM_V1;
    ensure(
        saved.first_moments[0].values[first_unassigned..]
            .iter()
            .chain(&saved.second_moments[0].values[first_unassigned..])
            .all(|bits| *bits == 0),
        "an embedding row past the declared source registry holds learned optimizer state; refusing to discard it (the declared source registry is not this checkpoint's)",
    )?;

    let mut appended_cards = Vec::with_capacity(appended.len());
    for (offset, card) in appended.iter().enumerate() {
        let card_id = source.cards.len() + offset;
        let name = card_name_v1(card)?;
        let row = card_id + 1;
        let span = row * CARD_EMBEDDING_DIM_V1..(row + 1) * CARD_EMBEDDING_DIM_V1;
        let initial = initial_card_row(name, request.initialization_seed);
        let parameters = &mut saved.parameters[0].values[span.clone()];
        for (slot, value) in parameters.iter_mut().zip(initial) {
            *slot = value.to_bits();
        }
        saved.first_moments[0].values[span.clone()].fill(0);
        saved.second_moments[0].values[span].fill(0);
        appended_cards.push(CardWaveAppendedCardV1 {
            card_id: u16::try_from(card_id).map_err(err)?,
            embedding_row: u32::try_from(row).map_err(err)?,
            name: name.into(),
        });
    }
    saved.card_db_hash = build_card_db_hash;
    let destination_state_sha256 = restored_state_sha256_v1(&saved, flat_v8)?;
    saved.state_sha256 = destination_state_sha256.clone();
    let checkpoint_bytes = serde_json::to_vec(&saved).map_err(err)?;

    let section_scalars: usize = saved.parameters.iter().map(|t| t.values.len()).sum();
    let initialized = appended.len() * CARD_EMBEDDING_DIM_V1;
    let source_segments = source_registry.iter().copied().map(sha).collect();
    let destination_segments = destination_registry.iter().copied().map(sha).collect();
    let record = CardWaveTransferRecordV1 {
        schema: RECORD_SCHEMA.into(),
        request: request.clone(),
        mapping_rule: MAPPING_RULE.into(),
        initializer: INITIALIZER.into(),
        source_registry_segment_sha256: source_segments,
        destination_registry_segment_sha256: destination_segments,
        source_card_count: source.cards.len(),
        destination_card_count: destination_cards.len(),
        appended_cards,
        checkpoint_schema: saved.schema,
        loss_identity: saved.loss_identity,
        adam_step: saved.adam_step,
        source_state_sha256,
        destination_state_sha256,
        destination_checkpoint_sha256: sha(&checkpoint_bytes),
        preserved_scalar_count_per_state_section: section_scalars - initialized,
        initialized_scalar_count_per_state_section: initialized,
    };
    Ok(CardWaveTransferV1 {
        checkpoint_bytes,
        record,
    })
}

/// Reproduce a persisted transfer from its preserved inputs and accept it
/// only if both the record and the destination checkpoint are, byte for
/// byte, what the transfer produces in this build.
pub fn verify_card_wave_transfer_v1(
    record_bytes: &[u8],
    destination_checkpoint_bytes: &[u8],
    source_checkpoint_bytes: &[u8],
    source_registry: &[&[u8]],
    destination_registry: &[&[u8]],
) -> Result<CardWaveTransferV1, String> {
    let reproduce = |request: &CardWaveTransferRequestV1| {
        transfer_card_wave_checkpoint_v1(
            source_checkpoint_bytes,
            source_registry,
            destination_registry,
            request,
        )
    };
    verify_reproduced_v1(record_bytes, destination_checkpoint_bytes, reproduce)
}

/// [`verify_card_wave_transfer_v1`] with the reproducing transfer supplied.
/// The record is read only for its request; what is accepted is the
/// reproduced record's own bytes, so a record that parses to the same
/// document from other bytes is refused too.
fn verify_reproduced_v1(
    record_bytes: &[u8],
    destination_checkpoint_bytes: &[u8],
    reproduce: impl FnOnce(&CardWaveTransferRequestV1) -> Result<CardWaveTransferV1, String>,
) -> Result<CardWaveTransferV1, String> {
    ensure(
        record_bytes.len() as u64 <= MAX_FILE_BYTES,
        "card-wave transfer record exceeds the size bound",
    )?;
    let record: CardWaveTransferRecordV1 = serde_json::from_slice(record_bytes).map_err(err)?;
    ensure(
        record.schema == RECORD_SCHEMA,
        "card-wave transfer record schema differs",
    )?;
    let reproduced = reproduce(&record.request)?;
    ensure(
        reproduced.record_bytes_v1()? == record_bytes
            && reproduced.checkpoint_bytes == destination_checkpoint_bytes,
        "card-wave transfer record or checkpoint differs from the reproduced transfer",
    )?;
    Ok(reproduced)
}

/// File-based request of [`run_card_wave_transfer_cli_v1`]. Every path is
/// absolute; each build's registry files are listed in build order. The
/// feature digests are the caller's pin of the checkpoint's feature
/// identity: the command never copies them from the checkpoint it is given.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardWaveTransferCliRequestV1 {
    pub schema: String,
    pub source_checkpoint: PathBuf,
    pub source_card_db_hash: String,
    pub feature_contract_digest: String,
    pub feature_encoding_digest: String,
    pub source_registry: Vec<PathBuf>,
    pub destination_registry: Vec<PathBuf>,
    pub initialization_seed: u64,
    /// Must not exist; receives `checkpoint.json` and `transfer-record.json`.
    pub output_directory: PathBuf,
}

fn read_cli_input_v1(path: &Path) -> Result<Vec<u8>, String> {
    ensure(
        path.is_absolute(),
        "card-wave transfer CLI requires absolute paths",
    )?;
    let located = |e: std::io::Error| format!("{}: {e}", path.display());
    let metadata = fs::metadata(path).map_err(located)?;
    ensure(
        metadata.is_file(),
        "card-wave transfer CLI input is not a regular file",
    )?;
    ensure(
        metadata.len() <= MAX_FILE_BYTES,
        "card-wave transfer CLI input exceeds the size bound",
    )?;
    // Bounded by the read itself too: a file that grows after its metadata
    // was read is never read past the bound.
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).map_err(err)?);
    fs::File::open(path)
        .and_then(|file| file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(located)?;
    ensure(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "card-wave transfer CLI input exceeds the size bound",
    )?;
    Ok(bytes)
}

fn read_cli_inputs_v1(paths: &[PathBuf]) -> Result<Vec<Vec<u8>>, String> {
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        files.push(read_cli_input_v1(path)?);
    }
    Ok(files)
}

/// [`run_card_wave_transfer_cli_v1`] of a request document: at most 1 MiB of
/// JSON without a repeated object key, decoded strictly.
pub fn run_card_wave_transfer_cli_json_v1(request: &[u8]) -> Result<Value, String> {
    ensure(
        request.len() <= MAX_CLI_REQUEST_BYTES,
        "card-wave transfer CLI request exceeds 1 MiB",
    )?;
    let text = std::str::from_utf8(request).map_err(err)?;
    crate::rl::parse_strict_json_value(text).map_err(err)?;
    let request: CardWaveTransferCliRequestV1 = serde_json::from_str(text).map_err(err)?;
    run_card_wave_transfer_cli_v1(&request)
}

/// Reads the inputs, transfers into this build and publishes the destination
/// checkpoint and its record to a fresh directory, then reads both files
/// back and accepts them only as the reproduced transfer's own bytes
/// ([`verify_card_wave_transfer_v1`]); files that fail are removed. Never
/// modifies an input and starts no training.
pub fn run_card_wave_transfer_cli_v1(
    request: &CardWaveTransferCliRequestV1,
) -> Result<Value, String> {
    ensure(
        request.schema == CLI_REQUEST_SCHEMA,
        "unknown card-wave transfer CLI request schema",
    )?;
    ensure(
        request.output_directory.is_absolute(),
        "card-wave transfer CLI requires an absolute output_directory",
    )?;
    // Refused before any work; `create_dir` below still decides.
    ensure(
        !request.output_directory.exists(),
        "fresh output directory required: output_directory already exists",
    )?;
    let checkpoint_bytes = read_cli_input_v1(&request.source_checkpoint)?;
    let source_files = read_cli_inputs_v1(&request.source_registry)?;
    let destination_files = read_cli_inputs_v1(&request.destination_registry)?;
    let source_registry: Vec<&[u8]> = source_files.iter().map(Vec::as_slice).collect();
    let destination_registry: Vec<&[u8]> = destination_files.iter().map(Vec::as_slice).collect();
    let transfer_request = CardWaveTransferRequestV1 {
        source_checkpoint_sha256: sha(&checkpoint_bytes),
        source_card_db_hash: request.source_card_db_hash.clone(),
        source_registry_sha256: registry_identity_v1(&source_registry),
        destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        destination_registry_sha256: registry_identity_v1(&destination_registry),
        feature_contract_digest: request.feature_contract_digest.clone(),
        feature_encoding_digest: request.feature_encoding_digest.clone(),
        initialization_seed: request.initialization_seed,
    };
    let transfer = transfer_card_wave_checkpoint_v1(
        &checkpoint_bytes,
        &source_registry,
        &destination_registry,
        &transfer_request,
    )?;
    let record_bytes = transfer.record_bytes_v1()?;
    if let Some(parent) = request.output_directory.parent() {
        fs::create_dir_all(parent).map_err(err)?;
    }
    fs::create_dir(&request.output_directory).map_err(|error| {
        format!(
            "fresh output directory {}: {error}",
            request.output_directory.display()
        )
    })?;
    let outputs = [
        ("checkpoint.json", transfer.checkpoint_bytes_v1()),
        ("transfer-record.json", record_bytes.as_slice()),
    ];
    // What a later reader finds on disk, reproduced from the inputs.
    let published = publish_verified_outputs_v1(&request.output_directory, &outputs, |pins| {
        let written_checkpoint = fs::read(&pins[0].path).map_err(err)?;
        let written_record = fs::read(&pins[1].path).map_err(err)?;
        verify_card_wave_transfer_v1(
            &written_record,
            &written_checkpoint,
            &checkpoint_bytes,
            &source_registry,
            &destination_registry,
        )
        .map(drop)
        .map_err(|error| format!("written card-wave transfer failed its replay: {error}"))
    })?;
    let [checkpoint, record]: [PinnedFileV1; 2] = published
        .try_into()
        .map_err(|_| "card-wave transfer published another number of files".to_string())?;
    Ok(serde_json::json!({
        "schema": CLI_RESULT_SCHEMA,
        "checkpoint": checkpoint,
        "transfer_record": record,
        "record": transfer.record_v1(),
        "verified": true,
    }))
}

/// Publishes `outputs` in order into `directory`, which the caller has just
/// created, and accepts them only if `verify` accepts the published files.
/// On any failure, a publication's or the verification's, it removes every
/// file and staging file of these names and then the directory: only this
/// call's own files, in the directory its caller created.
fn publish_verified_outputs_v1(
    directory: &Path,
    outputs: &[(&str, &[u8])],
    verify: impl FnOnce(&[PinnedFileV1]) -> Result<(), String>,
) -> Result<Vec<PinnedFileV1>, String> {
    let published = outputs
        .iter()
        .map(|(name, bytes)| publish_json_bytes_v1(directory, name, bytes))
        .collect::<Result<Vec<_>, _>>()
        .and_then(|pins| verify(&pins).map(|()| pins));
    if published.is_err() {
        for (name, _) in outputs {
            let _ = fs::remove_file(directory.join(name));
            let _ = fs::remove_file(directory.join(format!(".{name}.stage")));
        }
        let _ = fs::remove_dir(directory);
    }
    published
}

#[cfg(test)]
mod tests;
