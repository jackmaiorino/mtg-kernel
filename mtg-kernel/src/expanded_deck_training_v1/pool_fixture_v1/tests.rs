use super::*;
use crate::native_expanded_training_run_v1::ScheduledExpandedEpisodeV1;
use crate::sideboard_play_policy_v1::{FrozenPlayObservationTransferV3, FRESH_FEATURE_IDENTITY_V4};

#[test]
fn draws_are_the_published_sha256_draws() {
    // Computed outside Rust from the definition: sha256 of the compact JSON
    // array, first eight bytes big-endian, modulo the member count.
    let draws = |update, members| {
        (0..8)
            .map(|slot| draw_v1("pool-fixture-v1", update, slot, members))
            .collect::<Vec<_>>()
    };
    assert_eq!(draws(0, 18), [17, 1, 8, 10, 9, 17, 16, 14]);
    assert_eq!(draws(1, 18), [0, 1, 15, 9, 4, 9, 15, 12]);
    assert_eq!(draws(0, 2), [1, 1, 0, 0, 1, 1, 0, 0]);
    assert_eq!(draw_v1("pool-fixture-v1", 7, 3, 1), 0);
    assert_eq!(member_id_v1(7), "member-07");
}

fn list_v1(id: &str) -> ExpandedDeckListV1 {
    let registration = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    let cards = registration.registered_configuration();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: cards.mainboard().to_vec(),
        sideboard: cards.sideboard().to_vec(),
    }
}

#[test]
fn assigned_draws_name_each_slot_its_member() {
    let registered = [list_v1("Affinity"), list_v1("Elves")];
    let scheduled = |slot: u64| ScheduledExpandedEpisodeV1 {
        episode: ExpandedEpisodeV1 {
            id: format!("slot-{slot}"),
            seed: slot,
            starting_player: 0,
            learner_seat: 0,
            registered: registered.clone(),
            selected: registered.clone(),
            postboard: false,
            max_physical_decisions: 4000,
            max_policy_steps: 40000,
            opponent: None,
            opponent_kind: None,
            opponent_search: None,
            runtime_rules: None,
            reset: None,
            opening: None,
            mulligan_modes: None,
        },
        opponent: ExpandedOpponentAssignmentV1::Current,
    };
    let mut iterations: Vec<ExpandedRunIterationV1> = (0..2)
        .map(|_| ExpandedRunIterationV1 {
            episodes: (0..3).map(scheduled).collect(),
        })
        .collect();
    let source: ExpandedModelSourceV1 = serde_json::from_value(json!({
        "play_import": {"path": "/pool/descriptor.json", "sha256": "a".repeat(64)},
        "feature_transfer": {
            "expected_feature_contract_digest": FEATURE_CONTRACT_DIGEST_V4,
            "expected_feature_encoding_digest": FEATURE_ENCODING_DIGEST_V4,
        },
        "checkpoint": null,
    }))
    .unwrap();
    let members: Vec<NamedExpandedOpponentV1> = (0..18)
        .map(|member| NamedExpandedOpponentV1 {
            id: member_id_v1(member),
            source: source.clone(),
        })
        .collect();
    let table = assign_draws_v1(&mut iterations, &members, "pool-fixture-v1");
    assert_eq!(
        table,
        [
            ["member-17", "member-01", "member-08"],
            ["member-00", "member-01", "member-15"]
        ]
    );
    for (iteration, ids) in iterations.iter().zip(&table) {
        for (scheduled, id) in iteration.episodes.iter().zip(ids) {
            assert!(matches!(
                &scheduled.opponent,
                ExpandedOpponentAssignmentV1::Fixed { id: drawn } if drawn == id
            ));
        }
    }
}

#[test]
fn member_states_are_fixed_by_the_member_and_keep_the_fixed_values() {
    let policy = FrozenPlayPolicyV1::training_fixture_v3();
    let state = NativePolicyValueTrainStateV1::new_v1(policy.training_model_v3()).unwrap();
    let base = state.snapshot_v1().unwrap();
    let first = member_snapshot_v1(&base, 0);
    assert_eq!(first, member_snapshot_v1(&base, 0));
    let second = member_snapshot_v1(&base, 1);
    assert_ne!(first.parameters, second.parameters);
    assert_ne!(first.first_moments, second.first_moments);
    for snapshot in [&first, &second] {
        // The train state accepts both, so their digests exist.
        snapshot.state_sha256_v1().unwrap();
        let anchor = snapshot
            .parameters
            .iter()
            .find(|parameter| parameter.name == SCORER_ANCHOR_V1)
            .unwrap();
        assert_eq!(anchor.values[0].to_bits(), snapshot.scorer_bias_anchor_bits);
    }
}

fn temp_root_v1() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "pool-fixture-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}

/// Runs `body` on a 64 MiB thread: unoptimized loaders keep several
/// policies in a frame.
fn on_large_stack_v1(body: fn()) {
    let run = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(body)
        .unwrap();
    if let Err(payload) = run.join() {
        std::panic::resume_unwind(payload);
    }
}

#[test]
fn members_are_distinct_valid_containers_over_one_import() {
    on_large_stack_v1(members_are_distinct_valid_containers_over_one_import_v1);
}

fn members_are_distinct_valid_containers_over_one_import_v1() {
    let root = temp_root_v1();
    let identity = FRESH_FEATURE_IDENTITY_V4;
    let descriptor =
        write_synthetic_fresh_source_with_parameters_v1(&root.join("source"), identity, |_| {});
    let bytes = serde_json::to_vec(&descriptor).unwrap();
    let path = root.join("descriptor.json");
    fs::write(&path, &bytes).unwrap();
    let import = ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path,
            sha256: sha(&bytes),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: identity.feature_encoding_digest.into(),
        },
        checkpoint: None,
        s_learner: None,
    };
    let members = write_members_v1(&root.join("pool"), &import, 2).unwrap();
    assert_eq!(
        members
            .iter()
            .map(|member| member.id.as_str())
            .collect::<Vec<_>>(),
        ["member-00", "member-01"]
    );
    let checkpoints: Vec<&PinnedFileV1> = members
        .iter()
        .map(|member| member.source.checkpoint.as_ref().unwrap())
        .collect();
    assert_ne!(checkpoints[0].sha256, checkpoints[1].sha256);
    for (member, checkpoint) in members.iter().zip(&checkpoints) {
        assert_eq!(member.source.play_import, import.play_import);
        assert_eq!(
            checkpoint.path,
            root.join("pool").join(&member.id).join("checkpoint.json")
        );
        let written = fs::read(&checkpoint.path).unwrap();
        assert!(checkpoint_container_v1::is_container_v1(&written));
        // Three tensor sections (parameters and both moments) of the
        // 4,923,976-byte payload, plus the document.
        assert!(written.len() > 3 * 4_923_976);
        assert_eq!(sha(&written), checkpoint.sha256);
    }
    // The collection boundary verifies both for the process opponent cache.
    let episodes: Vec<ExpandedEpisodeV1> = members
        .iter()
        .map(|member| {
            let registered = [list_v1("Affinity"), list_v1("Elves")];
            ExpandedEpisodeV1 {
                id: member.id.clone(),
                seed: 1,
                starting_player: 0,
                learner_seat: 0,
                registered: registered.clone(),
                selected: registered,
                postboard: false,
                max_physical_decisions: 4000,
                max_policy_steps: 40000,
                opponent: Some(member.source.clone()),
                opponent_kind: None,
                opponent_search: None,
                runtime_rules: None,
                reset: None,
                opening: None,
                mulligan_modes: None,
            }
        })
        .collect();
    assert_eq!(
        verify_collection_opponent_sources_v1(&episodes).unwrap(),
        members
            .iter()
            .map(|member| member.source.clone())
            .collect::<Vec<_>>()
    );
    // A second generation into an occupied root refuses to overwrite.
    assert!(write_members_v1(&root.join("pool"), &import, 1).is_err());
    fs::remove_dir_all(&root).unwrap();
}
