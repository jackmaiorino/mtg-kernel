//! Opponent kinds interface v1: declaration bytes and conflicts, the
//! public-checkpoint seat through complete fixture games, and learner
//! isolation across opponent kinds.
use super::super::replay_audit::admits_public_trajectory;
use super::super::{collect_with_opponent, weights, Trajectory};
use super::*;

fn pin(path: &str, sha256: &str) -> PinnedFileV1 {
    PinnedFileV1 {
        path: path.into(),
        sha256: sha256.into(),
    }
}

fn burn() -> ExpandedDeckListV1 {
    let registration =
        crate::sideboard::checked_in_pauper_registered_deck_by_id_v1("Burn").unwrap();
    let cards = registration.registered_configuration();
    ExpandedDeckListV1 {
        label: "Burn".into(),
        mainboard: cards.mainboard().to_vec(),
        sideboard: cards.sideboard().to_vec(),
    }
}

fn recent_kind() -> ExpandedOpponentKindV1 {
    ExpandedOpponentKindV1::PublicCheckpoint {
        config: pin("replica-4/configs/a.json", &"17".repeat(32)),
        checkpoint: pin("replica-4/endpoints/a/checkpoint.json", &"57".repeat(32)),
    }
}

fn episode(learner_seat: u8, kind: Option<ExpandedOpponentKindV1>) -> ExpandedEpisodeV1 {
    let fixture_source = ExpandedModelSourceV1 {
        play_import: pin("play-import.json", &"6c".repeat(32)),
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: "c4".repeat(32),
            expected_feature_encoding_digest: "27".repeat(32),
        },
        checkpoint: None,
    };
    ExpandedEpisodeV1 {
        id: format!("opponent-kind-fixture-{learner_seat}"),
        seed: 2_026_092_702 + u64::from(learner_seat),
        starting_player: learner_seat,
        learner_seat,
        opponent: kind.is_none().then_some(fixture_source),
        opponent_search: None,
        opponent_kind: kind,
        registered: [burn(), burn()],
        selected: [burn(), burn()],
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
    }
}

fn learner() -> PublicInputPlayPolicyV1 {
    PublicInputPlayPolicyV1::new(
        FrozenPlayPolicyV1::training_fixture_v4(),
        weights(&ProjectionSnapshot::zero()).unwrap(),
    )
    .unwrap()
    .with_inputs_enabled(false)
}

fn public_seat() -> OpponentSeatV1 {
    OpponentSeatV1::PublicCheckpoint {
        policy: learner(),
        identity: json!({"schema": "public-input-evaluation-model/v1", "fixture": true}),
        rows: Vec::new(),
    }
}

fn net_seat() -> OpponentSeatV1 {
    let net = FrozenPlayPolicyV1::training_fixture_v4();
    let identity = ExpandedInferenceIdentityV1 {
        schema: "fixture".into(),
        source_import: net.identity_v1().clone(),
        checkpoint_sha256: None,
        model: net.actual_model_identity_v1(),
        state_sha256: "0".repeat(64),
        adam_step: 0,
        feature_schema_version: "fixture".into(),
        feature_registry_version: "fixture".into(),
        features_source_sha256: "0".repeat(64),
        feature_descriptor_sha256: "0".repeat(64),
    };
    OpponentSeatV1::Net {
        net,
        identity,
        search: None,
    }
}

fn play(episode: &ExpandedEpisodeV1, seat: OpponentSeatV1) -> Trajectory {
    collect_with_opponent(
        &mut learner(),
        episode,
        "config",
        "state",
        false,
        None,
        seat,
    )
    .unwrap()
}

#[test]
fn opponent_kind_keeps_bytes_matches_evaluator_shape_and_refuses_conflicts() {
    let plain = episode(0, None);
    let bytes = serde_json::to_vec(&plain).unwrap();
    assert!(!String::from_utf8(bytes.clone())
        .unwrap()
        .contains("opponent_kind"));
    let recent = episode(0, Some(recent_kind()));
    let back: ExpandedEpisodeV1 =
        serde_json::from_slice(&serde_json::to_vec(&recent).unwrap()).unwrap();
    assert_eq!(back.opponent_kind, Some(recent_kind()));
    // The same JSON as the evaluator's ModelSource::PublicCheckpoint.
    let evaluation_source = json!({"kind": "public_checkpoint",
        "config": {"path": "replica-4/configs/a.json", "sha256": "17".repeat(32)},
        "checkpoint": {"path": "replica-4/endpoints/a/checkpoint.json", "sha256": "57".repeat(32)}});
    assert_eq!(
        serde_json::to_value(recent_kind()).unwrap(),
        evaluation_source
    );
    let unknown = json!({"kind": "public_checkpoint", "config": evaluation_source["config"],
        "checkpoint": evaluation_source["checkpoint"], "fallback": true});
    assert!(serde_json::from_value::<ExpandedOpponentKindV1>(unknown).is_err());
    assert_eq!(
        recent.configurations().unwrap_err(),
        "opt-in opponent kinds are admitted only by the public-feature collector"
    );
    assert!(recent.configurations_for_public_collector_v1().is_ok());
    let mut both = recent.clone();
    both.opponent = plain.opponent.clone();
    assert_eq!(
        both.configurations_for_public_collector_v1().unwrap_err(),
        "an opponent kind excludes opponent and opponent_search"
    );
    let legacy = ExpandedOpponentKindV1::Legacy {
        source: plain.opponent.clone().unwrap(),
        v3_forced_actions: true,
        v3_spell_target_reference_adapter: true,
        admission: pin("v3-admission.json", &"9e".repeat(32)),
    };
    assert!(load_seat(&legacy, &episode(0, Some(legacy.clone()))).is_err());
}

#[test]
fn public_checkpoint_games_validate_replay_and_refuse_tampering() {
    for learner_seat in [0, 1] {
        let episode = episode(learner_seat, Some(recent_kind()));
        let a = play(&episode, public_seat());
        let b = play(&episode, public_seat());
        assert_eq!(
            serde_json::to_vec(&a).unwrap(),
            serde_json::to_vec(&b).unwrap()
        );
        assert_eq!(a.schema, PUBLIC_CHECKPOINT_TRAJECTORY_SCHEMA);
        assert!(a.opponent.is_none() && a.search.is_none());
        assert!(!admits_public_trajectory(&a));
        let record = a.opponent_record.as_ref().expect("opponent record");
        let opponent_rows = a
            .decisions
            .iter()
            .filter(|r| r.actor != learner_seat)
            .count();
        assert!(opponent_rows > 0 && record.rows.len() == opponent_rows);
        // Opponent public rows are stored; the update reads learner rows only.
        assert!(a.auxiliary.iter().all(Option::is_some));
        let hashes = a.configuration_sha256.clone();
        record
            .validate(&a.episode, &hashes, &a.decisions, &a.terminal)
            .unwrap();
        for seat in [1 - learner_seat, learner_seat] {
            let mut tampered = a.decisions.clone();
            let row = tampered
                .iter_mut()
                .find(|r| r.actor == seat && r.logits.len() > 1)
                .expect("a decision with a choice");
            row.selected = (row.selected + 1) % row.logits.len() as u32;
            assert!(record
                .validate(&a.episode, &hashes, &tampered, &a.terminal)
                .is_err());
        }
    }
}

/// Learner isolation: before the opponent first acts, the learner sees the
/// same state with the same weights and sampler position, so its rows must
/// be byte-identical whatever the opponent kind.
#[test]
fn learner_rows_do_not_depend_on_the_opponent_kind() {
    let recent = play(&episode(0, Some(recent_kind())), public_seat());
    let ordinary = play(&episode(0, None), net_seat());
    let prefix = |t: &Trajectory| {
        let rows: Vec<_> = t.decisions.iter().take_while(|r| r.actor == 0).collect();
        assert!(!rows.is_empty(), "learner moves first in this fixture");
        (
            serde_json::to_vec(&rows).unwrap(),
            serde_json::to_vec(&t.auxiliary[..rows.len()]).unwrap(),
        )
    };
    assert_eq!(prefix(&recent), prefix(&ordinary));
}

fn legacy_kind(forced: bool, spell_adapter: bool) -> ExpandedOpponentKindV1 {
    ExpandedOpponentKindV1::Legacy {
        source: episode(0, None).opponent.unwrap(),
        v3_forced_actions: forced,
        v3_spell_target_reference_adapter: spell_adapter,
        admission: pin("v3-admission.json", &"9e".repeat(32)),
    }
}

fn legacy_seat(forced: bool, spell_adapter: bool) -> OpponentSeatV1 {
    OpponentSeatV1::Legacy {
        policy: FrozenPlayPolicyV1::training_fixture_v3(),
        identity: json!({"schema": "legacy-collection-model/v1", "fixture": true}),
        forced,
        spell_adapter,
        rows: Vec::new(),
    }
}

/// Legacy V3 against a V4 learner, both adapter settings: same seed gives
/// the same bytes; forced singletons are recorded unscored and replay one
/// draw; scored rows carry V3 digests; tampering is refused.
#[test]
fn legacy_v3_games_record_singletons_unscored_validate_and_refuse_tampering() {
    for (forced, spell_adapter) in [(true, true), (false, false)] {
        for learner_seat in [0, 1] {
            let episode = episode(learner_seat, Some(legacy_kind(forced, spell_adapter)));
            let a = play(&episode, legacy_seat(forced, spell_adapter));
            let b = play(&episode, legacy_seat(forced, spell_adapter));
            assert_eq!(
                serde_json::to_vec(&a).unwrap(),
                serde_json::to_vec(&b).unwrap()
            );
            assert_eq!(a.schema, LEGACY_TRAJECTORY_SCHEMA);
            assert!(!admits_public_trajectory(&a));
            let record = a.opponent_record.as_ref().expect("opponent record");
            let opponent: Vec<_> = a
                .decisions
                .iter()
                .filter(|r| r.actor != learner_seat)
                .collect();
            assert_eq!(record.rows.len(), opponent.len());
            let singletons = opponent
                .iter()
                .filter(|r| r.sampler_identity.as_deref() == Some(V3_FORCED_SINGLETON_SAMPLER))
                .count();
            if forced {
                assert!(singletons > 0, "fixture games contain forced singletons");
                assert!(opponent
                    .iter()
                    .filter(|r| r.logits.is_empty())
                    .all(|r| r.selected == 0 && r.tensor.is_empty()));
            } else {
                assert_eq!(singletons, 0);
                assert!(opponent.iter().all(|r| !r.logits.is_empty()));
            }
            let hashes = a.configuration_sha256.clone();
            record
                .validate(&a.episode, &hashes, &a.decisions, &a.terminal)
                .unwrap();
            let mut tampered = a.decisions.clone();
            let row = tampered
                .iter_mut()
                .find(|r| r.actor != learner_seat && r.logits.len() > 1)
                .expect("a scored opponent choice");
            row.selected = (row.selected + 1) % row.logits.len() as u32;
            assert!(record
                .validate(&a.episode, &hashes, &tampered, &a.terminal)
                .is_err());
            if forced {
                // A fabricated scored singleton is refused.
                let mut fabricated = a.decisions.clone();
                let row = fabricated
                    .iter_mut()
                    .find(|r| r.sampler_identity.as_deref() == Some(V3_FORCED_SINGLETON_SAMPLER))
                    .unwrap();
                row.logits = vec![0f32.to_bits()];
                assert!(record
                    .validate(&a.episode, &hashes, &fabricated, &a.terminal)
                    .is_err());
            }
        }
    }
}

/// Admission refuses a receipt that disagrees with the declaration, before
/// any model load: wrong descriptor bytes, flags, route (R14 included until
/// its acceptance is bound) or import descriptor.
#[test]
fn legacy_admission_refuses_mismatched_receipts() {
    let directory = std::env::temp_dir().join(format!(
        "mtg-legacy-admission-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let write = |name: &str, bytes: &[u8]| {
        let path = directory.join(name);
        std::fs::write(&path, bytes).unwrap();
        PinnedFileV1 {
            path,
            sha256: sha(bytes),
        }
    };
    let source = episode(0, None).opponent.unwrap();
    let descriptor = write("model-source.json", &serde_json::to_vec(&source).unwrap());
    let mut other_source = source.clone();
    other_source.play_import.sha256 = "ab".repeat(32);
    let other_descriptor = write(
        "other-source.json",
        &serde_json::to_vec(&other_source).unwrap(),
    );
    let receipt = LegacyAdmissionV1 {
        schema: LEGACY_ADMISSION_SCHEMA.into(),
        member: "fixture".into(),
        route: "strict".into(),
        model_source: descriptor,
        import_descriptor: AdmissionImportV1 {
            path: source.play_import.path.clone(),
            sha256: source.play_import.sha256.clone(),
            schema: None,
        },
        expected_identity: AdmissionIdentityV1 {
            identity_schema: "mtg-kernel-frozen-sideboard-play-transfer/v1".into(),
            model_parameter_sha256: "00".repeat(32),
            weights_sha256: "00".repeat(32),
            feature_generation: "V3".into(),
            observation_successor: true,
            feature_contract_digest: "93".repeat(32),
            feature_encoding_digest: "c4".repeat(32),
        },
        adapter_flags: AdmissionFlagsV1 {
            v3_forced_actions: true,
            v3_spell_target_reference_adapter: true,
        },
        registry_pins: json!({}),
        evidence: json!({}),
        nonclaims: vec![],
    };
    let mut r14 = receipt.clone();
    r14.route = "r14".into();
    let mut flags = receipt.clone();
    flags.adapter_flags.v3_spell_target_reference_adapter = false;
    let mut route = receipt.clone();
    route.route = "unreviewed".into();
    let mut bytes = receipt.clone();
    bytes.model_source = other_descriptor;
    let mut import = receipt.clone();
    import.import_descriptor.sha256 = "cd".repeat(32);
    let mut generation = receipt.clone();
    generation.expected_identity.feature_generation = "V4".into();
    for (name, bad) in [
        ("r14.json", &r14),
        ("flags.json", &flags),
        ("route.json", &route),
        ("bytes.json", &bytes),
        ("import.json", &import),
        ("generation.json", &generation),
    ] {
        let admission = write(name, &serde_json::to_vec(bad).unwrap());
        let error = admit_legacy(&source, true, true, &admission).err().unwrap();
        assert_eq!(
            error, "legacy opponent differs from its admission receipt",
            "{name}"
        );
    }
    std::fs::remove_dir_all(&directory).unwrap();
}

/// Admission helper, not a check: prints the loaded identity fields of the
/// ExpandedModelSourceV1 JSON named by MTG_LEGACY_PROBE_SOURCE, so an
/// admission receipt's expected_identity is taken from a real load.
#[test]
#[ignore = "admission helper; needs MTG_LEGACY_PROBE_SOURCE"]
fn legacy_admission_identity_probe() {
    let path = std::env::var("MTG_LEGACY_PROBE_SOURCE").expect("MTG_LEGACY_PROBE_SOURCE");
    let source: ExpandedModelSourceV1 =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let (policy, identity) = load_expanded_inference_v1(&source).unwrap();
    println!(
        "LEGACY_PROBE {}",
        json!({
            "identity_schema": serde_json::to_value(&identity.source_import).unwrap()["schema"],
            "model_parameter_sha256": identity.model.model_parameter_sha256,
            "weights_sha256": identity.model.weights_sha256,
            "feature_generation": if policy.feature_generation_v1() == PlayPolicyGenerationV1::V3 { "V3" } else { "V4" },
            "observation_successor": policy.uses_observation_successor_v3(),
            "feature_contract_digest": identity.model.feature_contract_digest,
            "feature_encoding_digest": identity.model.feature_encoding_digest,
        })
    );
}

/// The collector's Legacy seat makes the evaluator's choices: replaying a
/// collected game through the evaluator's own Legacy dispatch (same seeds,
/// same physical-seat stream) reproduces every opponent action.
#[test]
fn legacy_seat_matches_the_evaluator_dispatch() {
    for (forced, spell_adapter) in [(true, true), (false, false)] {
        let episode = episode(1, Some(legacy_kind(forced, spell_adapter)));
        let trajectory = play(&episode, legacy_seat(forced, spell_adapter));
        let configs = episode.configurations_for_public_collector_v1().unwrap();
        let mut session = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
            1,
            episode.seed,
            episode.max_physical_decisions,
            episode.max_policy_steps,
            episode.selected.each_ref().map(|d| d.label.clone()),
            configs.each_ref().map(|c| c.mainboard().to_vec()),
            PlayerId(episode.starting_player),
        )
        .unwrap();
        let mut evaluator = crate::learned_bo3_v1::public_evaluation::legacy_play_for_test(
            FrozenPlayPolicyV1::training_fixture_v3(),
            forced,
            spell_adapter,
        );
        evaluator
            .reset_for_game_v1(paired_policy_seeds_v1(episode.seed))
            .unwrap();
        let mut compared = 0;
        for row in &trajectory.decisions {
            let FastActorResponseV1::Decision(d) = session.current_response() else {
                panic!("replay ended early");
            };
            assert_eq!((d.step, seat(d.acting_player)), (row.step, row.actor));
            if row.actor != episode.learner_seat {
                let chosen = evaluator
                    .select_action_v1(PairedBo1PolicyInputV1::new(&session, d))
                    .unwrap();
                assert_eq!(chosen, row.selected, "step {}", row.step);
                compared += 1;
            }
            session.step(d.episode_id, d.step, row.selected).unwrap();
        }
        assert!(compared > 0);
        assert!(matches!(
            session.current_response(),
            FastActorResponseV1::Terminal(_)
        ));
    }
}
