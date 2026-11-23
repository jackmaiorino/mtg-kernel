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
//! schema (imported or fresh origin), any of its losses, any feature contract
//! (V3, V4, FlatV7, FlatV8, no-state-digest) and the net-8 or V8 inclusion
//! layout, into the registry of whatever build runs the transfer (default or
//! `eternal-fixtures`).
//!
//! The transfer is a pure function of its inputs: it reads and writes no
//! file. Its output is an ordinary checkpoint with the same fields as its
//! source except `card_db_hash`, `state_sha256` and the appended card rows
//! of the card embedding and its two moments. Nothing else, including the
//! warm-start provenance (`source_import`), is rewritten. The transfer
//! record ([`CardWaveTransferRecordV1`]) is a separate document, so the
//! strict checkpoint reader and every existing checkpoint's bytes are
//! unchanged.

use super::*;
use crate::card_def::CARD_DEFS;
use crate::native_policy_train_step_v1::native_train_state_parameter_layout_for_count_v1;
use crate::native_policy_value_net_v1::{CARD_EMBEDDING_DIM_V1, CARD_VOCAB_SIZE_V1};
use crate::phase1_registry_transfer_v1::{initial_card_row, registry_cards, INITIALIZER};

const RECORD_SCHEMA: &str = "mtg-kernel-expanded-card-wave-transfer/v1";
const CLI_REQUEST_SCHEMA: &str = "mtg-kernel-expanded-card-wave-transfer-cli-request/v1";
const CLI_RESULT_SCHEMA: &str = "mtg-kernel-expanded-card-wave-transfer-cli-result/v1";
const REGISTRY_IDENTITY_DOMAIN: &str = "mtg-kernel-card-wave-registry-segments/v1";
const MAPPING_RULE: &str = "exact-existing-card-record-prefix; card_token=card_db_id+1; appended-rows-initialized-moments-zeroed; all-other-bits-preserved";
const CARD_EMBEDDING_NAME: &str = "card_embedding.weight";
/// Registry fields that `build.rs` never reads (its `CardJson` does not
/// declare them), so they are outside `KERNEL_CARDDB_HASH`. Every other
/// field, including one this list does not know, must be unchanged.
const METADATA_FIELDS: [&str; 3] = ["complexity", "java_file", "sideboard_only"];
const MODEL_SHAPE_REFUSAL: &str = "checkpoint model shape differs from the net-8 train-state layout; card-wave transfer never reshapes a model";

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

    pub fn record_bytes_v1(&self) -> Result<Vec<u8>, String> {
        let mut bytes = serde_json::to_vec_pretty(&self.record).map_err(err)?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

/// The destination a transfer binds: a card-db hash and the card names in
/// id order. [`transfer_card_wave_checkpoint_v1`] binds the running build.
struct DestinationBindingV1<'a> {
    card_db_hash: String,
    names: Vec<&'a str>,
}

fn compiled_destination_v1() -> DestinationBindingV1<'static> {
    DestinationBindingV1 {
        card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
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

/// The concatenated `cards` of the segments, each read by the existing
/// strict registry reader (duplicate keys, empty or ambiguous names
/// refused), with names unique across the whole id space.
fn concatenated_cards_v1(segments: &[&[u8]]) -> Result<Vec<Value>, String> {
    ensure(
        !segments.is_empty(),
        "card-wave transfer requires at least one registry segment",
    )?;
    let mut cards = Vec::new();
    for segment in segments {
        cards.extend(registry_cards(segment)?);
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
    Ok(cards)
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
/// of `destination` in every hash-covered field. Refuses, in id order, the
/// first removed, renamed, reordered or changed row.
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
                "card id {id} ({old_name:?}) changed field {field:?}, which the card database hash covers; only appended cards are transferable"
            ));
        }
    }
    Ok(&destination[source.len()..])
}

/// One checkpoint section as the owned train-state tensors its layout names.
fn section_v1(
    tensors: &[ParameterBitsV1],
    layout: &[(&'static str, &'static [usize])],
) -> Result<Vec<NativeNamedParameterV1>, String> {
    ensure(tensors.len() == layout.len(), MODEL_SHAPE_REFUSAL)?;
    let mut section = Vec::with_capacity(layout.len());
    for (tensor, &(name, shape)) in tensors.iter().zip(layout) {
        ensure(
            tensor.name == name
                && tensor.shape == shape
                && tensor.values.len() == shape.iter().product::<usize>(),
            MODEL_SHAPE_REFUSAL,
        )?;
        section.push(NativeNamedParameterV1 {
            name,
            shape: shape.to_vec(),
            values: floats(&tensor.values),
        });
    }
    Ok(section)
}

/// The checkpoint's state digest by the native train-state hash, after its
/// validation (finite values, non-negative second moments, zero padding row
/// and gauge moments, gauge anchor, Adam step domain).
fn state_sha256_v1(saved: &ExpandedCheckpointV1) -> Result<String, String> {
    let layout = native_train_state_parameter_layout_for_count_v1(saved.parameters.len());
    let snapshot = NativePolicyValueTrainSnapshotV1 {
        adam_step: saved.adam_step,
        scorer_bias_anchor_bits: saved.scorer_bias_anchor_bits,
        parameters: section_v1(&saved.parameters, &layout)?,
        first_moments: section_v1(&saved.first_moments, &layout)?,
        second_moments: section_v1(&saved.second_moments, &layout)?,
    };
    Ok(hex(&snapshot.state_sha256_v1().map_err(err)?))
}

/// Transfer a trainer checkpoint across one appended card wave into the
/// running build. `source_registry` and `destination_registry` are each
/// build's registry files in build order (Pauper first). The destination
/// registry's names must be this build's compiled card names in id order,
/// and `request.destination_card_db_hash` this build's hash.
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
        request.destination_card_db_hash == destination.card_db_hash,
        "destination card database is not this build's KERNEL_CARDDB_HASH",
    )?;
    ensure(
        request.source_card_db_hash != request.destination_card_db_hash,
        "source and destination card databases are identical: there is no card wave to transfer, and a transferred checkpoint is never transferred again into its own build",
    )?;

    // The checkpoint, in either format, through the trainer's own strict
    // reader: an unknown field or tensor encoding refuses here.
    let mut saved = checkpoint_container_v1::parse_checkpoint_v1(checkpoint_bytes)?;
    ensure(
        saved.card_db_hash == request.source_card_db_hash,
        "checkpoint card database differs from the declared source card database",
    )?;
    ensure(
        saved.schema == CHECKPOINT_SCHEMA || saved.schema == FRESH_CHECKPOINT_SCHEMA,
        "card-wave transfer reads only an ordinary expanded-deck checkpoint",
    )?;
    ensure(
        saved.feature_contract_digest == request.feature_contract_digest
            && saved.feature_encoding_digest == request.feature_encoding_digest,
        "checkpoint feature contract differs from the request; card-wave transfer never migrates features",
    )?;
    ensure(
        saved.registry_transfer.is_none()
            && saved.archive_distillation.is_none()
            && saved.e_supported.is_none()
            && saved.soft_q.is_none()
            && saved.search_targets.is_none()
            && saved.ex1.is_none(),
        "checkpoint carries continuation provenance bound to its own card database (registry transfer, archive distillation, E, soft-Q, search targets or EX1); card-wave transfer refuses it",
    )?;

    let source_cards = concatenated_cards_v1(source_registry)?;
    let destination_cards = concatenated_cards_v1(destination_registry)?;
    ensure(
        destination_cards.len() == destination.names.len()
            && destination_cards
                .iter()
                .zip(&destination.names)
                .all(|(card, name)| has_name_v1(card, name)),
        "destination registry is not this build's compiled card registry (card names or order differ)",
    )?;
    let appended = appended_v1(&source_cards, &destination_cards)?;
    ensure(
        !appended.is_empty(),
        "no card is appended, yet the card database hash changed: a rules or field change of existing cards is not a card wave and is refused",
    )?;

    // Validates the whole source state against its recorded digest before
    // any bit changes.
    let source_state_sha256 = state_sha256_v1(&saved)?;
    ensure(
        source_state_sha256 == saved.state_sha256,
        "source checkpoint state digest differs",
    )?;
    ensure(
        saved.parameters[0].name == CARD_EMBEDDING_NAME
            && saved.parameters[0].shape == [CARD_VOCAB_SIZE_V1, CARD_EMBEDDING_DIM_V1],
        MODEL_SHAPE_REFUSAL,
    )?;

    let mut appended_cards = Vec::with_capacity(appended.len());
    for (offset, card) in appended.iter().enumerate() {
        let card_id = source_cards.len() + offset;
        let name = card_name_v1(card)?;
        let row = card_id + 1;
        let span = row * CARD_EMBEDDING_DIM_V1..(row + 1) * CARD_EMBEDDING_DIM_V1;
        let first = &saved.first_moments[0].values[span.clone()];
        let second = &saved.second_moments[0].values[span.clone()];
        ensure(
            first.iter().chain(second).all(|bits| *bits == 0),
            "an appended card's embedding row holds learned optimizer state; refusing to discard it (the declared source registry is not this checkpoint's)",
        )?;
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
    saved.card_db_hash = request.destination_card_db_hash.clone();
    let destination_state_sha256 = state_sha256_v1(&saved)?;
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
        source_card_count: source_cards.len(),
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
/// only if both the record and the destination checkpoint bytes are exactly
/// what the transfer produces in this build.
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
    let same_checkpoint = reproduced.checkpoint_bytes == destination_checkpoint_bytes;
    ensure(
        reproduced.record == record && same_checkpoint,
        "card-wave transfer record or checkpoint differs from the reproduced transfer",
    )?;
    Ok(reproduced)
}

/// File-based request of [`run_card_wave_transfer_cli_v1`]. Every path is
/// absolute; each build's registry files are listed in build order.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardWaveTransferCliRequestV1 {
    pub schema: String,
    pub source_checkpoint: PathBuf,
    pub source_card_db_hash: String,
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
    let metadata = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    ensure(
        metadata.len() <= MAX_FILE_BYTES,
        "card-wave transfer CLI input exceeds the size bound",
    )?;
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_cli_inputs_v1(paths: &[PathBuf]) -> Result<Vec<Vec<u8>>, String> {
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        files.push(read_cli_input_v1(path)?);
    }
    Ok(files)
}

/// Reads the inputs, transfers into this build, re-verifies the result from
/// the same bytes and writes the destination checkpoint and its record to a
/// fresh directory. Never modifies an input and starts no training.
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
    let checkpoint_bytes = read_cli_input_v1(&request.source_checkpoint)?;
    let source_files = read_cli_inputs_v1(&request.source_registry)?;
    let destination_files = read_cli_inputs_v1(&request.destination_registry)?;
    let source_registry: Vec<&[u8]> = source_files.iter().map(Vec::as_slice).collect();
    let destination_registry: Vec<&[u8]> = destination_files.iter().map(Vec::as_slice).collect();
    let saved = checkpoint_container_v1::parse_checkpoint_v1(&checkpoint_bytes)?;
    let transfer_request = CardWaveTransferRequestV1 {
        source_checkpoint_sha256: sha(&checkpoint_bytes),
        source_card_db_hash: request.source_card_db_hash.clone(),
        source_registry_sha256: registry_identity_v1(&source_registry),
        destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        destination_registry_sha256: registry_identity_v1(&destination_registry),
        feature_contract_digest: saved.feature_contract_digest,
        feature_encoding_digest: saved.feature_encoding_digest,
        initialization_seed: request.initialization_seed,
    };
    let transfer = transfer_card_wave_checkpoint_v1(
        &checkpoint_bytes,
        &source_registry,
        &destination_registry,
        &transfer_request,
    )?;
    let record_bytes = transfer.record_bytes_v1()?;
    verify_card_wave_transfer_v1(
        &record_bytes,
        transfer.checkpoint_bytes_v1(),
        &checkpoint_bytes,
        &source_registry,
        &destination_registry,
    )?;
    if let Some(parent) = request.output_directory.parent() {
        fs::create_dir_all(parent).map_err(err)?;
    }
    fs::create_dir(&request.output_directory).map_err(|error| {
        format!(
            "fresh output directory {}: {error}",
            request.output_directory.display()
        )
    })?;
    let checkpoint = PinnedFileV1 {
        path: request.output_directory.join("checkpoint.json"),
        sha256: sha(transfer.checkpoint_bytes_v1()),
    };
    let record = PinnedFileV1 {
        path: request.output_directory.join("transfer-record.json"),
        sha256: sha(&record_bytes),
    };
    fs::write(&checkpoint.path, transfer.checkpoint_bytes_v1()).map_err(err)?;
    fs::write(&record.path, record_bytes).map_err(err)?;
    Ok(serde_json::json!({
        "schema": CLI_RESULT_SCHEMA,
        "checkpoint": checkpoint,
        "transfer_record": record,
        "record": transfer.record_v1(),
        "verified": true,
    }))
}

#[cfg(test)]
mod tests;
