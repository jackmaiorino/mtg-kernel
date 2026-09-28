//! R14 dispatch: inference alone admits registry-evolution descriptors, never
//! with a successor checkpoint; learner paths refuse them; unknown schemas
//! fail without falling back to another route.

use super::*;
use crate::native_flat_tensorizer_v3::{FEATURE_CONTRACT_DIGEST_V3, FEATURE_ENCODING_DIGEST_V3};

struct Fixture {
    root: PathBuf,
    files: Vec<PathBuf>,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "r14-dispatch-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        Self {
            root,
            files: Vec::new(),
        }
    }

    fn pin(&mut self, name: &str, value: &Value) -> PinnedFileV1 {
        let bytes = serde_json::to_vec(value).unwrap();
        let path = self.root.join(name);
        fs::write(&path, &bytes).unwrap();
        self.files.push(path.clone());
        PinnedFileV1 {
            path,
            sha256: sha(&bytes),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Only the unique files this fixture created are removed.
        for path in &self.files {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir(&self.root);
    }
}

fn v3_transfer() -> FrozenPlayObservationTransferV3 {
    FrozenPlayObservationTransferV3 {
        expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
        expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
    }
}

fn descriptor(schema: &str) -> Value {
    serde_json::json!({
        "schema": schema,
        "export_directory": "unused-export",
        "expected_metadata_sha256": "0".repeat(64),
        "expected_model_parameter_sha256": "0".repeat(64),
        "source_run_path": "unused-run.json",
        "source_registry_path": "unused-registry.json",
        "expected_source_registry_sha256": "0".repeat(64),
        "source_registry_git_commit": "0".repeat(40),
        "expected_destination_card_db_hash": "0".repeat(16),
        "allowlist_path": "unused-allowlist.json",
        "expected_allowlist_sha256": "0".repeat(64)
    })
}

fn source(play_import: PinnedFileV1, checkpoint: Option<PinnedFileV1>) -> ExpandedModelSourceV1 {
    ExpandedModelSourceV1 {
        play_import,
        feature_transfer: v3_transfer(),
        checkpoint,
    }
}

#[test]
fn learner_initialization_refuses_registry_evolution_descriptors() {
    let mut fixture = Fixture::new("learner");
    let play_import = fixture.pin(
        "import.json",
        &descriptor(REGISTRY_EVOLUTION_IMPORT_SCHEMA_V1),
    );
    let error = initialize_with_transfer_context(&source(play_import, None))
        .err()
        .expect("learner initialization must refuse R14");
    assert_eq!(error, REGISTRY_EVOLUTION_LEARNER_REFUSAL_V1);
}

#[test]
fn inference_refuses_registry_evolution_with_a_successor_checkpoint() {
    let mut fixture = Fixture::new("checkpoint");
    let play_import = fixture.pin(
        "import.json",
        &descriptor(REGISTRY_EVOLUTION_IMPORT_SCHEMA_V1),
    );
    let checkpoint = fixture.pin("checkpoint.json", &serde_json::json!({"unused": true}));
    let error = load_expanded_inference_v1(&source(play_import, Some(checkpoint)))
        .err()
        .expect("R14 with a successor checkpoint must be refused");
    assert_eq!(
        error,
        "registry evolution imports admit no successor checkpoint"
    );
}

#[test]
fn unknown_schemas_are_refused_without_fallback() {
    let mut fixture = Fixture::new("unknown");
    let play_import = fixture.pin("import.json", &descriptor("mtg-kernel-unknown-import/v1"));
    let inference = load_expanded_inference_v1(&source(play_import.clone(), None))
        .err()
        .expect("unknown schema must be refused by inference");
    assert!(inference.contains("unknown field"), "{inference}");
    let learner = initialize_with_transfer_context(&source(play_import, None))
        .err()
        .expect("unknown schema must be refused by learner initialization");
    assert!(learner.contains("unknown field"), "{learner}");
}

#[test]
fn bo3_transition_admission_refuses_registry_evolution_descriptors() {
    let mut fixture = Fixture::new("bo3");
    let play_import = fixture.pin(
        "import.json",
        &descriptor(REGISTRY_EVOLUTION_IMPORT_SCHEMA_V1),
    );
    let checkpoint = fixture.pin("checkpoint.json", &serde_json::json!({"unused": true}));
    assert!(
        validate_ordinary_source_descriptor_v1(&source(play_import, Some(checkpoint))).is_err()
    );
}
