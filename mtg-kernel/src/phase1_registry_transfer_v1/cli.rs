//! Minimal file-based CLI entry point for the explicit registry transfer
//! (`transfer_expanded_checkpoint_to_current_registry_v1`). The pure library
//! function above performs no I/O; this module is the only place in the
//! transfer family that reads or writes files, and it never mutates its
//! inputs. It produces a self-contained bundle: a copy of the pinned source
//! checkpoint and source registry, the transfer envelope, a minimal
//! inference-only continuation schedule, and an `ExpandedRegistryTransferSourceV1`
//! document usable directly as an `ExpandedModelSourceV1.play_import`. It
//! then re-derives the artifact through `verify_registry_transfer_artifact_v1`
//! and records that receipt alongside the bundle.
//!
//! This module never starts training or collection; it only ever produces an
//! inference-ready play-import descriptor. The bounded trainer's own
//! `checkpoint: Some(...)` successor path and its schema checks are unchanged.

use super::*;
use crate::expanded_deck_training_v1::{
    ExpandedDeckListV1, ExpandedEpisodeV1, ExpandedRegistryTransferScheduleV1,
    ExpandedRegistryTransferSourceV1,
};
use crate::sideboard::checked_in_pauper_registered_deck_by_id_v1;
use serde_json::json;
use std::path::PathBuf;

const REQUEST_SCHEMA: &str = "phase1-registry-transfer-cli-request/v1";
// Must match `expanded_deck_training_v1::registry_transfer_source::SOURCE_SCHEMA`
// and `..._source::ExpandedRegistryTransferScheduleV1`'s schema tag (both
// private to that sibling module); `read_schedule`/`initialize` validate
// these strings at load time, so a mismatch here fails loudly and immediately.
const SOURCE_SCHEMA: &str = "mtg-kernel-expanded-registry-transfer-source/v1";
const SCHEDULE_SCHEMA: &str = "mtg-kernel-expanded-registry-resolved-schedule/v1";
const MAX_CLI_INPUT_BYTES: u64 = 512 * 1024 * 1024;

/// One pinned checkpoint plus its declared source registry, transferred to
/// this build's compiled-in registry. `initialization_seed` seeds only the
/// deterministic per-card initializer for cards appended since the source
/// registry (see `phase1_registry_transfer_v1::mod` docs); it never affects
/// any shared card's parameters or optimizer state.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryTransferCliRequestV1 {
    pub schema: String,
    pub source_checkpoint: PathBuf,
    pub source_registry: PathBuf,
    pub initialization_seed: u64,
    /// Must not already exist; the CLI writes a fresh, self-contained bundle.
    pub output_directory: PathBuf,
}

fn read_bounded(path: &std::path::Path) -> Result<Vec<u8>, String> {
    require(path.is_absolute(), "registry transfer CLI requires absolute paths")?;
    let metadata = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    require(
        metadata.len() <= MAX_CLI_INPUT_BYTES,
        "registry transfer CLI input exceeds size bound",
    )?;
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_new(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_json_pretty(path: &std::path::Path, value: &impl Serialize) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    write_new(path, &bytes)?;
    Ok(bytes)
}

fn pin(path: PathBuf, bytes: &[u8]) -> PinnedFileV1 {
    PinnedFileV1 {
        path,
        sha256: sha(bytes),
    }
}

/// Probes only the small scalar fields this CLI needs from a real
/// `mtg-kernel-expanded-deck-checkpoint/v1` file. Deliberately does not
/// derive `deny_unknown_fields`: the full checkpoint carries large
/// parameter/moment/trajectory arrays this probe ignores entirely, leaving
/// the actual transfer function (which decodes the strict, full, private
/// `ExpandedCheckpointInputV1` shape) as the sole schema authority.
#[derive(Deserialize)]
struct CheckpointScalarProbeV1 {
    state_sha256: String,
    adam_step: u64,
    card_db_hash: String,
}

/// A single self-play episode that satisfies `ExpandedEpisodeV1::configurations()`
/// and every `read_schedule` bound, but is never collected or replayed: this
/// CLI only ever produces an inference-ready `play_import` (`checkpoint: None`),
/// so `load_expanded_inference_v1` calls `registry_transfer_source::initialize`
/// far enough to validate and restore the transferred policy without ever
/// touching the schedule's cursor or batch content. A non-empty schedule is
/// mandatory (`read_schedule` rejects an empty `batches` list); this is the
/// smallest schedule that still passes every bound.
fn inference_only_episode() -> Result<ExpandedEpisodeV1, String> {
    let deck = checked_in_pauper_registered_deck_by_id_v1("Rally").map_err(|e| e.to_string())?;
    let list = ExpandedDeckListV1 {
        label: deck.deck_id().to_owned(),
        mainboard: deck.registered_configuration().mainboard().to_vec(),
        sideboard: deck.registered_configuration().sideboard().to_vec(),
    };
    Ok(ExpandedEpisodeV1 {
        id: "registry-transfer-inference-only-placeholder-0".into(),
        seed: 1,
        starting_player: 0,
        learner_seat: 0,
        opponent: None,
        registered: [list.clone(), list.clone()],
        selected: [list.clone(), list],
        postboard: false,
        max_physical_decisions: 100,
        max_policy_steps: 100,
    })
}

/// Runs the explicit registry transfer for one pinned checkpoint and writes
/// a self-contained, inference-ready bundle to `request.output_directory`:
/// copies of the source checkpoint and source registry, the transfer
/// envelope, a minimal inference-only continuation schedule, the
/// `ExpandedRegistryTransferSourceV1` document (usable directly as an
/// `ExpandedModelSourceV1.play_import`), and a receipt from re-deriving the
/// artifact through `verify_registry_transfer_artifact_v1`. No training,
/// collection, or registry/model mutation occurs; every write below is this
/// CLI's own, never the pure transfer/verify functions'.
pub fn run_registry_transfer_cli_v1(request: &RegistryTransferCliRequestV1) -> Result<Value, String> {
    require(
        request.schema == REQUEST_SCHEMA,
        "unknown registry transfer CLI request schema",
    )?;
    require(
        request.output_directory.is_absolute(),
        "registry transfer CLI requires an absolute output_directory",
    )?;
    let checkpoint_bytes = read_bounded(&request.source_checkpoint)?;
    let source_registry_bytes = read_bounded(&request.source_registry)?;
    let probe: CheckpointScalarProbeV1 =
        serde_json::from_slice(&checkpoint_bytes).map_err(|e| format!("source checkpoint: {e}"))?;

    let transfer_request = RegistryTransferRequestV1 {
        source_checkpoint_sha256: sha(&checkpoint_bytes),
        source_registry_sha256: sha(&source_registry_bytes),
        source_state_sha256: probe.state_sha256,
        source_adam_step: probe.adam_step,
        source_card_db_hash: probe.card_db_hash,
        destination_registry_sha256: sha(CURRENT_REGISTRY),
        destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        features: RegistryTransferFeaturesV1::current_v1(),
        initialization_seed: request.initialization_seed,
    };
    let transfer = transfer_expanded_checkpoint_to_current_registry_v1(
        &checkpoint_bytes,
        &source_registry_bytes,
        &transfer_request,
    )?;
    let envelope_bytes = transfer.artifact_bytes_v1()?;
    let envelope_sha256 = sha(&envelope_bytes);
    let receipt = transfer.receipt_v1().clone();

    if let Some(parent) = request.output_directory.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir(&request.output_directory).map_err(|error| {
        format!(
            "fresh output directory {}: {error}",
            request.output_directory.display()
        )
    })?;
    let out = |name: &str| request.output_directory.join(name);

    write_new(&out("source-checkpoint.json"), &checkpoint_bytes)?;
    write_new(&out("source-registry.json"), &source_registry_bytes)?;
    write_new(&out("transfer-envelope.json"), &envelope_bytes)?;

    let schedule = ExpandedRegistryTransferScheduleV1 {
        schema: SCHEDULE_SCHEMA.into(),
        initial_adam_step: receipt.scalars.adam_step,
        learning_rate_bits: receipt.scalars.learning_rate_bits,
        value_coefficient_bits: receipt.scalars.value_coefficient_bits,
        batches: vec![vec![inference_only_episode()?]],
    };
    let schedule_bytes = write_json_pretty(&out("continuation-schedule.json"), &schedule)?;

    let descriptor = ExpandedRegistryTransferSourceV1 {
        schema: SOURCE_SCHEMA.into(),
        source_checkpoint: pin(out("source-checkpoint.json"), &checkpoint_bytes),
        source_registry: pin(out("source-registry.json"), &source_registry_bytes),
        transfer_envelope: pin(out("transfer-envelope.json"), &envelope_bytes),
        continuation_schedule: pin(out("continuation-schedule.json"), &schedule_bytes),
    };
    let play_import_bytes = write_json_pretty(&out("play-import-source.json"), &descriptor)?;
    let play_import = pin(out("play-import-source.json"), &play_import_bytes);

    // Reproduce the transfer from the preserved bytes exactly as a later
    // loader would, and record that receipt: this is the deliverable's own
    // `verify_registry_transfer_artifact_v1` evidence, not a repeat of the
    // transfer above (a tampered envelope would fail here even though the
    // transfer above already succeeded against the original inputs).
    let reverified = verify_registry_transfer_artifact_v1(
        &envelope_bytes,
        &envelope_sha256,
        &checkpoint_bytes,
        &source_registry_bytes,
    )?;
    require(
        reverified.receipt_v1() == &receipt,
        "verify_registry_transfer_artifact_v1 receipt differs from the original transfer",
    )?;
    let verify_receipt = json!({
        "schema": "phase1-registry-transfer-cli-verify-receipt/v1",
        "verified": true,
        "transfer_envelope_sha256": envelope_sha256,
        "source_checkpoint_sha256": transfer_request.source_checkpoint_sha256,
        "source_registry_sha256": transfer_request.source_registry_sha256,
        "receipt": reverified.receipt_v1(),
    });
    write_json_pretty(&out("verify-receipt.json"), &verify_receipt)?;

    Ok(json!({
        "schema": "phase1-registry-transfer-cli-result/v1",
        "output_directory": request.output_directory,
        "play_import": play_import,
        "transfer_envelope_sha256": envelope_sha256,
        "receipt": receipt,
        "verified": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expanded_deck_training_v1::tests::checkpoint_fixture_v1;
    use crate::expanded_deck_training_v1::{load_expanded_inference_v1, ExpandedModelSourceV1};
    use crate::native_flat_tensorizer_v3::{FEATURE_DESCRIPTOR_SHA256_V3, FEATURES_SOURCE_SHA256_V3};
    use crate::sideboard_play_policy_v1::FrozenPlayObservationTransferV3;

    fn temp_dir(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "registry-transfer-cli-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// Degenerate but legitimate case: source registry equals the current
    /// compiled registry, so the transfer appends zero cards. This isolates
    /// the CLI's own file plumbing and the `load_expanded_inference_v1`
    /// dispatch from the append/mapping machinery, which the sibling
    /// `registry_transfer_source::tests` module already exercises in depth.
    /// `ExpandedCheckpointV1.source_import` is a private field outside
    /// `expanded_deck_training_v1` (by design: callers restore state only
    /// through the crate's own validated constructors). This test does not
    /// need to write it through the typed struct: it round-trips the fixture
    /// through `serde_json::Value` and edits the same wire keys a real
    /// checkpoint file already carries (verified directly against the real
    /// Adam3/Adam483 checkpoints' `source_import` shape while researching
    /// this change), which is exactly what the CLI itself will read back.
    fn self_transfer_fixture(root: &std::path::Path) -> RegistryTransferCliRequestV1 {
        let (_, _, saved) = checkpoint_fixture_v1();
        let mut value = serde_json::to_value(&saved).unwrap();
        let source_import = value
            .get_mut("source_import")
            .expect("checkpoint fixture must serialize a source_import object");
        source_import["destination_registry_sha256"] = json!(sha(CURRENT_REGISTRY));
        source_import["destination_card_db_hash"] = json!(format!("{KERNEL_CARDDB_HASH:016x}"));
        source_import["destination_card_count"] = json!(CARD_DEFS.len());
        source_import["feature_contract_digest"] = json!(FEATURE_CONTRACT_DIGEST_V3);
        source_import["feature_encoding_digest"] = json!(FEATURE_ENCODING_DIGEST_V3);
        source_import["observation_successor"] = json!({
            "schema": "mtg-kernel-frozen-play-observation-transfer/v3",
            "source_feature_contract_digest": "1".repeat(64),
            "source_feature_encoding_digest": "2".repeat(64),
            "destination": {
                "expected_feature_contract_digest": FEATURE_CONTRACT_DIGEST_V3,
                "expected_feature_encoding_digest": FEATURE_ENCODING_DIGEST_V3,
            },
            "features_source_sha256": FEATURES_SOURCE_SHA256_V3,
            "feature_descriptor_sha256": FEATURE_DESCRIPTOR_SHA256_V3,
            "semantics": "synthetic cli unit-test ancestry; no evidence claim",
        });
        let source_checkpoint = root.join("checkpoint.json");
        write_new(&source_checkpoint, &serde_json::to_vec(&value).unwrap()).unwrap();
        let source_registry = root.join("registry.json");
        write_new(&source_registry, CURRENT_REGISTRY).unwrap();
        RegistryTransferCliRequestV1 {
            schema: REQUEST_SCHEMA.into(),
            source_checkpoint,
            source_registry,
            initialization_seed: 7,
            output_directory: root.join("out"),
        }
    }

    #[test]
    fn registry_transfer_cli_produces_a_play_import_that_load_expanded_inference_v1_accepts() {
        let root = temp_dir("ok");
        let request = self_transfer_fixture(&root);
        let output_directory = request.output_directory.clone();
        let result = run_registry_transfer_cli_v1(&request).unwrap();
        assert_eq!(result["schema"], "phase1-registry-transfer-cli-result/v1");
        assert_eq!(result["verified"], true);

        for name in [
            "source-checkpoint.json",
            "source-registry.json",
            "transfer-envelope.json",
            "continuation-schedule.json",
            "play-import-source.json",
            "verify-receipt.json",
        ] {
            assert!(output_directory.join(name).exists(), "missing {name}");
        }

        let play_import_path = output_directory.join("play-import-source.json");
        let play_import_bytes = std::fs::read(&play_import_path).unwrap();
        let source = ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path: play_import_path,
                sha256: sha(&play_import_bytes),
            },
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
                expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
            },
            checkpoint: None,
        };
        // This is deliverable 1's own confirmation: the transfer source this
        // CLI just wrote loads for evaluation through the exact function
        // `run_population_batch` uses for each seat.
        let (policy, identity) = load_expanded_inference_v1(&source).unwrap();
        assert!(identity.has_supported_origin_schema_v1());
        assert_eq!(
            policy.identity_v1().as_imported_v1().unwrap().schema,
            "mtg-kernel-registry-transferred-play/v1"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn registry_transfer_cli_refuses_a_non_fresh_output_directory() {
        let root = temp_dir("exists");
        let request = self_transfer_fixture(&root);
        std::fs::create_dir(&request.output_directory).unwrap();
        let error = run_registry_transfer_cli_v1(&request).unwrap_err();
        assert!(error.contains("fresh output directory"), "{error}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn registry_transfer_cli_rejects_relative_output_directory() {
        let root = temp_dir("relative");
        let mut request = self_transfer_fixture(&root);
        request.output_directory = PathBuf::from("relative-out");
        let error = run_registry_transfer_cli_v1(&request).unwrap_err();
        assert!(error.contains("absolute"), "{error}");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
