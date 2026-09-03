use mtg_kernel::canonical_json_v1::{
    to_canonical_json_bytes_v1, CanonicalJsonNullPathSegmentV1, CanonicalJsonNullPolicyV1,
};
use mtg_kernel::native_training_store_update_group_v1::{
    EPISODE_SCHEMA_V1, UPDATE_EVIDENCE_SCHEMA_V1, UPDATE_EVIDENCE_SHA256_IDENTITY_V1,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const PREVIOUS_UPDATE_NULL_PATH_V1: &[CanonicalJsonNullPathSegmentV1] = &[
    CanonicalJsonNullPathSegmentV1::ObjectKey("update_groups"),
    CanonicalJsonNullPathSegmentV1::AnyArrayElement,
    CanonicalJsonNullPathSegmentV1::ObjectKey("previous_update_evidence_sha256"),
];
const CONTINUATION_NULL_PATHS_V1: &[&[CanonicalJsonNullPathSegmentV1]] =
    &[PREVIOUS_UPDATE_NULL_PATH_V1];

struct TestDirectoryV1 {
    root: PathBuf,
}

impl TestDirectoryV1 {
    fn new_v1() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mtg-kernel-cycle4-m3-cli-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("store/segments")).expect("segments directory");
        fs::create_dir_all(root.join("store/checkpoints")).expect("checkpoints directory");
        Self { root }
    }
}

impl Drop for TestDirectoryV1 {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn lower_hex_v1(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn update_evidence_sha256_v1(
    run_sha256: &[u8; 32],
    update_index: u64,
    previous: Option<&[u8; 32]>,
    evidence_bytes: &[u8],
) -> [u8; 32] {
    fn atom_v1(hasher: &mut Sha256, tag: &str, payload: &[u8]) {
        hasher.update(u32::try_from(tag.len()).expect("tag length").to_be_bytes());
        hasher.update(tag.as_bytes());
        hasher.update(
            u64::try_from(payload.len())
                .expect("payload length")
                .to_be_bytes(),
        );
        hasher.update(payload);
    }

    let mut hasher = Sha256::new();
    atom_v1(
        &mut hasher,
        "domain",
        UPDATE_EVIDENCE_SHA256_IDENTITY_V1.as_bytes(),
    );
    atom_v1(&mut hasher, "run_sha256", run_sha256);
    atom_v1(
        &mut hasher,
        "update_index_u64be",
        &update_index.to_be_bytes(),
    );
    atom_v1(
        &mut hasher,
        "previous_update_evidence_sha256",
        previous.map_or(&[][..], |digest| digest.as_slice()),
    );
    atom_v1(&mut hasher, "evidence_canonical_json", evidence_bytes);
    hasher.finalize().into()
}

fn write_1536_tip_store_v1(directory: &TestDirectoryV1) {
    let run_raw = [0x31_u8; 32];
    let run_sha256 = lower_hex_v1(&run_raw);
    let opponent_sha256 = "41".repeat(32);
    let mut previous: Option<[u8; 32]> = None;
    let mut groups = Vec::with_capacity(1_536);

    for update_index in 1..=1_536_u64 {
        let in_window = update_index >= 1_025;
        let physical_terms = if in_window {
            vec![
                serde_json::json!({
                    "joint_log_probability_f32_bits": "bf000000",
                    "value_f32_bits": "00000000",
                    "terminal_return_i8": 1,
                    "substep_count": 1,
                }),
                serde_json::json!({
                    "joint_log_probability_f32_bits": "bf000000",
                    "value_f32_bits": "00000000",
                    "terminal_return_i8": 1,
                    "substep_count": 1,
                }),
            ]
        } else {
            Vec::new()
        };
        let episodes = if in_window {
            vec![serde_json::json!({
                "schema": EPISODE_SCHEMA_V1,
                "episode_index": 0,
                "environment_seed_u64_hex": "0000000000000001",
                "deck_ids": ["deck-a", "deck-b"],
                "deck_hashes_u64_hex": ["0000000000000002", "0000000000000003"],
                "learner_seat": "p0",
                "learner_return": 1,
                "terminal_outcome": "win",
                "winner": "p0",
                "terminal_classification": "Natural",
                "terminal_code": "NaturalGameOver",
                "policy_step_count": 2,
                "physical_decision_count": 2,
                "learner_policy_step_count": 2,
                "opponent_policy_step_count": 0,
                "learner_physical_decision_count": 2,
                "opponent_physical_decision_count": 0,
                "trajectory_sha256": "51".repeat(32),
                "opponent_population_slot": 0,
                "opponent_occupant_class": "policy",
                "opponent_run_sha256": "61".repeat(32),
                "opponent_checkpoint_manifest_sha256": opponent_sha256,
            })]
        } else {
            Vec::new()
        };
        let evidence = serde_json::json!({
            "schema": UPDATE_EVIDENCE_SCHEMA_V1,
            "run_sha256": run_sha256,
            "update_index": update_index,
            "learner_physical_decision_count": if in_window { 2 } else { 0 },
            "physical_terms": physical_terms,
            "episodes": episodes,
        });
        let evidence_bytes =
            to_canonical_json_bytes_v1(&evidence, CanonicalJsonNullPolicyV1::Forbid)
                .expect("canonical evidence");
        let digest =
            update_evidence_sha256_v1(&run_raw, update_index, previous.as_ref(), &evidence_bytes);
        groups.push(serde_json::json!({
            "update_index": update_index,
            "update_evidence_sha256": lower_hex_v1(&digest),
            "previous_update_evidence_sha256": previous
                .as_ref()
                .map(|value| lower_hex_v1(value)),
            "evidence": evidence,
        }));
        previous = Some(digest);
    }

    let continuation = serde_json::json!({ "update_groups": groups });
    let continuation_bytes = to_canonical_json_bytes_v1(
        &continuation,
        CanonicalJsonNullPolicyV1::AllowOnly(CONTINUATION_NULL_PATHS_V1),
    )
    .expect("canonical continuation");
    fs::write(
        directory
            .root
            .join("store/segments/segment-00000004.continuation-00000000.json"),
        continuation_bytes,
    )
    .expect("write continuation");
    fs::write(
        directory
            .root
            .join("store/checkpoints/update-00001536.checkpoint.json"),
        b"{\"synthetic_checkpoint_for_update\":1536}\n",
    )
    .expect("write checkpoint");
}

#[test]
fn reference_mode_refuses_a_1536_tip_without_writing_output_v1() {
    let directory = TestDirectoryV1::new_v1();
    write_1536_tip_store_v1(&directory);
    let audit_note = directory.root.join("audit-note.md");
    let output_path = directory.root.join("reference.json");
    fs::write(&audit_note, b"synthetic audit note\n").expect("write audit note");

    let output = Command::new(env!("CARGO_BIN_EXE_cycle4_m3_audit_v1"))
        .args([
            "--mode",
            "reference",
            "--store-root",
            directory.root.join("store").to_str().expect("store path"),
            "--audit-note",
            audit_note.to_str().expect("audit note path"),
            "--output",
            output_path.to_str().expect("output path"),
        ])
        .output()
        .expect("run reference producer");

    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(
        !output_path.exists(),
        "a rejected reference must not publish"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cycle4_m3_audit_v1_reference_window"),
        "{output:?}"
    );
}
