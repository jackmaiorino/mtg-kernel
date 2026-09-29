//! Complete games through the public collector with the unchanged D3 wrapper
//! on the opponent seat (fixture nets, reviewed budget). Same seed gives the
//! same trajectory bytes; learner rows still replay the behavior sampler;
//! tampered search or learner rows are refused; ordinary validation refuses a
//! search trajectory; the loader enforces both frozen pins before any read.
use super::super::replay_audit::admits_public_trajectory;
use super::super::{collect_with_opponent, weights, Trajectory};
use super::*;

fn fixture_descriptor(net: &FrozenPlayPolicyV1) -> V4InformationSetSearchDescriptorV1 {
    let mut d: V4InformationSetSearchDescriptorV1 =
        serde_json::from_str(REVIEWED_DESCRIPTOR_JSON).unwrap();
    let m = net.actual_model_identity_v1();
    d.weights_sha256 = m.weights_sha256;
    d.model_parameter_sha256 = m.model_parameter_sha256;
    d.embedding_table_sha256 = m.embedding_table_sha256;
    d.feature_contract_digest = m.feature_contract_digest;
    d.feature_encoding_digest = m.feature_encoding_digest;
    d
}

fn pin(path: &str, sha256: &str) -> PinnedFileV1 {
    PinnedFileV1 {
        path: path.into(),
        sha256: sha256.into(),
    }
}

fn source(checkpoint: &str) -> ExpandedModelSourceV1 {
    ExpandedModelSourceV1 {
        play_import: pin("play-import.json", &"6c".repeat(32)),
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: "c4".repeat(32),
            expected_feature_encoding_digest: "27".repeat(32),
        },
        checkpoint: Some(pin("checkpoint.json", checkpoint)),
    }
}

fn identity(net: &FrozenPlayPolicyV1, checkpoint: Option<&str>) -> ExpandedInferenceIdentityV1 {
    ExpandedInferenceIdentityV1 {
        schema: "fixture".into(),
        source_import: net.identity_v1().clone(),
        checkpoint_sha256: checkpoint.map(str::to_owned),
        model: net.actual_model_identity_v1(),
        state_sha256: "0".repeat(64),
        adam_step: 0,
        feature_schema_version: "fixture".into(),
        feature_registry_version: "fixture".into(),
        features_source_sha256: "0".repeat(64),
        feature_descriptor_sha256: "0".repeat(64),
    }
}

fn episode(learner_seat: u8) -> ExpandedEpisodeV1 {
    let registration =
        crate::sideboard::checked_in_pauper_registered_deck_by_id_v1("Burn").unwrap();
    let cards = registration.registered_configuration();
    let deck = ExpandedDeckListV1 {
        label: "Burn".into(),
        mainboard: cards.mainboard().to_vec(),
        sideboard: cards.sideboard().to_vec(),
    };
    ExpandedEpisodeV1 {
        id: format!("search-opponent-fixture-{learner_seat}"),
        seed: 2_026_092_701 + u64::from(learner_seat),
        starting_player: 1 - learner_seat,
        learner_seat,
        opponent: Some(source(G115_CHECKPOINT_SHA256)),
        opponent_kind: None,
        opponent_search: Some(pin("d3.json", REVIEWED_DESCRIPTOR_SHA256)),
        registered: [deck.clone(), deck.clone()],
        selected: [deck.clone(), deck],
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
    }
}

/// One game with fixture nets; the D3 wrapper plays the opponent seat when
/// the episode pins it, the ordinary sampled net otherwise.
fn play(episode: &ExpandedEpisodeV1) -> Result<Trajectory, String> {
    let base = FrozenPlayPolicyV1::training_fixture_v4();
    let mut learner = PublicInputPlayPolicyV1::new(base, weights(&ProjectionSnapshot::zero())?)?
        .with_inputs_enabled(false);
    let net = FrozenPlayPolicyV1::training_fixture_v4();
    let search = match episode.opponent_search {
        Some(_) => Some(SearchOpponentV1::new(
            &net,
            fixture_descriptor(&net),
            REVIEWED_DESCRIPTOR_SHA256.into(),
            1 - episode.learner_seat,
        )?),
        None => None,
    };
    let id = identity(&net, Some(G115_CHECKPOINT_SHA256));
    collect_with_opponent(
        &mut learner,
        episode,
        "config",
        "state",
        false,
        None,
        super::super::opponent_kind::OpponentSeatV1::Net {
            net,
            identity: id,
            search,
        },
    )
}

#[test]
fn replay_audit_admits_ordinary_and_refuses_search_opponent_trajectories() {
    assert_ne!(
        SEARCH_OPPONENT_TRAJECTORY_SCHEMA,
        "mtg-kernel-public-input-trajectory/v1"
    );
    let mut plain = episode(0);
    plain.opponent_search = None;
    let t = play(&plain).unwrap();
    assert_eq!(t.schema, "mtg-kernel-public-input-trajectory/v1");
    assert!(t.search.is_none() && admits_public_trajectory(&t));
    let mut relabeled =
        serde_json::from_slice::<Trajectory>(&serde_json::to_vec(&t).unwrap()).unwrap();
    relabeled.schema = SEARCH_OPPONENT_TRAJECTORY_SCHEMA.into();
    assert!(!admits_public_trajectory(&relabeled));
    let mut carried =
        serde_json::from_slice::<Trajectory>(&serde_json::to_vec(&t).unwrap()).unwrap();
    let net = FrozenPlayPolicyV1::training_fixture_v4();
    carried.search = Some(SearchTrajectoryV1 {
        schema: SEARCH_RECORD_SCHEMA.into(),
        seat: 1,
        descriptor_sha256: REVIEWED_DESCRIPTOR_SHA256.into(),
        descriptor: fixture_descriptor(&net),
        build: SearchBuildV1::current(),
        decisions: vec![],
    });
    assert!(!admits_public_trajectory(&carried));
}

#[test]
#[ignore = "two complete D3-opponent games per seat, several minutes; run explicitly"]
fn search_opponent_games_replay_validate_and_refuse_tampering() {
    for learner_seat in [0, 1] {
        let episode = episode(learner_seat);
        let a = play(&episode).unwrap();
        let b = play(&episode).unwrap();
        assert_eq!(
            serde_json::to_vec(&a).unwrap(),
            serde_json::to_vec(&b).unwrap()
        );
        assert_eq!(a.schema, SEARCH_OPPONENT_TRAJECTORY_SCHEMA);
        assert!(!admits_public_trajectory(&a));
        let search = a.search.as_ref().expect("search record");
        let (learner, opponent): (Vec<_>, Vec<_>) =
            a.decisions.iter().partition(|r| r.actor == learner_seat);
        assert!(!learner.is_empty() && !opponent.is_empty());
        assert_eq!(search.decisions.len(), opponent.len());
        assert!(opponent
            .iter()
            .all(|r| r.sampler_identity.as_deref() == Some(SEARCH_SAMPLER_IDENTITY)));
        assert!(
            learner
                .iter()
                .all(|r| r.sampler_identity.as_deref()
                    == decision_sampler_identity_v1(r.logits.len()))
        );
        let hashes = a.configuration_sha256.clone();
        search
            .validate(
                &a.episode,
                &hashes,
                &a.decisions,
                &a.terminal,
                a.learner_sampler.as_deref(),
            )
            .unwrap();
        assert!(validate_episode_records_with_learner_sampler_v1(
            &a.episode,
            &hashes,
            &a.decisions,
            &a.terminal,
            a.learner_sampler.as_deref()
        )
        .is_err());
        for seat in [1 - learner_seat, learner_seat] {
            let mut tampered = a.decisions.clone();
            let row = tampered
                .iter_mut()
                .find(|r| r.actor == seat && r.logits.len() > 1)
                .expect("a decision with a choice");
            row.selected = (row.selected + 1) % row.logits.len() as u32;
            assert!(search
                .validate(
                    &a.episode,
                    &hashes,
                    &tampered,
                    &a.terminal,
                    a.learner_sampler.as_deref()
                )
                .is_err());
        }
    }
}

#[test]
fn loader_enforces_reviewed_descriptor_and_g115_before_reading() {
    let net = FrozenPlayPolicyV1::training_fixture_v4();
    let episode = episode(0);
    let reviewed = pin("missing-d3.json", REVIEWED_DESCRIPTOR_SHA256);
    let g115 = identity(&net, Some(G115_CHECKPOINT_SHA256));
    let error = |r: Result<SearchOpponentV1, String>| r.err().expect("refused");
    assert_eq!(
        error(SearchOpponentV1::load(
            &pin("d3.json", &"00".repeat(32)),
            &episode,
            &net,
            &g115
        )),
        "search opponent requires the reviewed D3 descriptor"
    );
    let mut other = episode.clone();
    other.opponent = Some(source(&"11".repeat(32)));
    assert_eq!(
        error(SearchOpponentV1::load(&reviewed, &other, &net, &g115)),
        "search opponent requires the frozen g115 checkpoint"
    );
    assert_eq!(
        error(SearchOpponentV1::load(
            &reviewed,
            &episode,
            &net,
            &identity(&net, None)
        )),
        "search opponent requires the frozen g115 checkpoint"
    );
    // Both pins pass; the descriptor bytes are then read and fail closed.
    assert!(SearchOpponentV1::load(&reviewed, &episode, &net, &g115).is_err());
}

/// FABLE-REVIEW-20260927 change 4: a typed search error stops collection
/// and leaves a failure receipt with public bindings only.
#[test]
fn typed_search_failure_publishes_a_public_failure_record() {
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::Zone;
    let mut state = ready_state();
    put(
        &mut state,
        crate::ids::PlayerId::P0,
        "Lightning Bolt",
        Zone::Hand,
    );
    for name in ["Forest", "Mountain", "Island"] {
        put(&mut state, crate::ids::PlayerId::P0, name, Zone::Library);
        put(&mut state, crate::ids::PlayerId::P1, name, Zone::Library);
    }
    let session = FastActorSessionV1::from_v3_fixture_state(state);
    let FastActorResponseV1::Decision(mut stale) = session.current_response() else {
        panic!("fixture has no live decision");
    };
    stale.step += 1;
    // The wrapper plays whichever seat holds this fixture's first decision.
    let searcher = seat(stale.acting_player);
    let mut net = FrozenPlayPolicyV1::training_fixture_v4();
    let mut search = SearchOpponentV1::new(
        &net,
        fixture_descriptor(&net),
        REVIEWED_DESCRIPTOR_SHA256.into(),
        searcher,
    )
    .unwrap();
    search.reset_for_game([1, 2], "failure-fixture").unwrap();
    net.reset_sampling_v1([1, 2]);
    let error = search
        .select(&mut net, &session, stale)
        .err()
        .expect("typed failure");
    assert!(error.contains(SEARCH_FAILURE_MARKER), "{error}");
    let directory = std::env::temp_dir().join(format!(
        "mtg-search-failure-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    assert_eq!(publish_failure(&directory, error.clone()), error);
    let record: Value =
        serde_json::from_slice(&std::fs::read(directory.join("search-failure.json")).unwrap())
            .unwrap();
    std::fs::remove_dir_all(&directory).unwrap();
    assert_eq!(
        record["schema"],
        "mtg-kernel-public-search-opponent-failure/v1"
    );
    assert_eq!(record["episode_id"], "failure-fixture");
    assert_eq!(record["seat"], searcher);
    assert_eq!(record["step"], stale.step);
    assert_eq!(record["error"], "InvalidAdapterBinding");
    assert!(record.get("state").is_none());
    assert_eq!(
        publish_failure(&directory, "ordinary error".into()),
        "ordinary error"
    );
}

/// FABLE-REVIEW-20260927 change 2, the audit core on a live fixture root:
/// every perturbation is built here and none moves the D3 decision; the
/// fresh rerun selects the action the wrapper played. Power: a wrong recorded
/// action is refused, and a built variant whose visible key differs stops
/// the audit instead of being skipped (review 20:00, change 2).
#[test]
fn boundary_audit_checks_every_available_perturbation_of_a_live_root() {
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::Zone;
    let (searcher, learner) = (crate::ids::PlayerId::P0, crate::ids::PlayerId::P1);
    let fixture = |searcher_life: Option<i32>| {
        let mut state = ready_state();
        put(&mut state, searcher, "Lightning Bolt", Zone::Hand);
        state.players[searcher.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 3;
        for name in ["Gut Shot", "Lotus Petal"] {
            let id = put(&mut state, learner, name, Zone::Hand);
            state.objects.get_mut(id).zone_change_count = 1;
        }
        for owner in [searcher, learner] {
            for name in ["Forest", "Mountain", "Island", "Swamp", "Counterspell"] {
                put(&mut state, owner, name, Zone::Library);
            }
        }
        if let Some(life) = searcher_life {
            state.players[searcher.index()].life = life;
        }
        FastActorSessionV1::from_v3_fixture_state(state)
    };
    let session = fixture(None);
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("fixture has no live decision");
    };
    let mut net = FrozenPlayPolicyV1::training_fixture_v4();
    let mut search = SearchOpponentV1::new(
        &net,
        fixture_descriptor(&net),
        REVIEWED_DESCRIPTOR_SHA256.into(),
        0,
    )
    .unwrap();
    search.reset_for_game([1, 2], "audit-fixture").unwrap();
    let (played, _, _) = search.select(&mut net, &session, decision).unwrap();
    let mut counts = BoundaryAuditCountsV1::default();
    search
        .audit_root(&net, &session, decision, played, &mut counts)
        .unwrap();
    assert_eq!(
        counts,
        BoundaryAuditCountsV1 {
            roots: 1,
            checked: [1; 4],
            unavailable: [0; 4],
            inadmissible: [0; 4],
        }
    );
    let wrong = (played + 1) % decision.legal_action_count;
    let error = search
        .audit_root(
            &net,
            &session,
            decision,
            wrong,
            &mut BoundaryAuditCountsV1::default(),
        )
        .unwrap_err();
    assert!(error.contains("but the wrapper played"), "{error}");
    let record = sealed_audit_stop(error);
    assert_eq!(record["cause"], "rerun-differs-from-played");
    assert_eq!(record["variant"], Value::Null);
    assert_eq!(
        (
            record["detail"]["played"].clone(),
            record["detail"]["rerun_selected"].clone()
        ),
        (json!(wrong), json!(played))
    );
    // A built variant with a visible change is a leak, not a skip.
    let visible = fixture(Some(7));
    let mut counts = BoundaryAuditCountsV1::default();
    let error = search
        .audit_variants(
            &net,
            &session,
            decision,
            played,
            [None, None, None, Some(visible)],
            &mut counts,
        )
        .unwrap_err();
    assert!(
        error.contains("changed the searcher's decision binding or visible key"),
        "{error}"
    );
    assert_eq!(
        (counts.unavailable, counts.inadmissible),
        ([1, 1, 1, 0], [0, 0, 0, 1])
    );
    // Change 12: the stop is sealed as a typed record with the run's counts
    // before this root, published by the collector's failure path.
    let before = BoundaryAuditCountsV1 {
        roots: 5,
        checked: [4, 5, 5, 5],
        unavailable: [1, 0, 0, 0],
        inadmissible: [0; 4],
    };
    let record = sealed_audit_stop(with_run_counts(error, &before));
    assert_eq!(
        record["schema"],
        "mtg-kernel-public-search-opponent-audit-failure/v1"
    );
    assert_eq!(record["cause"], "inadmissible-variant");
    assert_eq!(
        (record["variant"].clone(), record["variant_index"].clone()),
        (json!("future randomness"), json!(3))
    );
    assert_eq!(
        (
            record["episode_id"].clone(),
            record["step"].clone(),
            record["seat"].clone()
        ),
        (json!("audit-fixture"), json!(decision.step), json!(0))
    );
    assert_eq!(record["root_counts"], json!(counts));
    assert_eq!(record["run_counts_before_this_root"], json!(before));
    assert_eq!(
        record["detail"],
        json!({ "decision_binding_kept": true, "visible_key_kept": false })
    );
    assert!(record.get("state").is_none());
}

/// Publishes an audit stop error the way the collector does and returns the
/// sealed record; the error itself passes through unchanged.
fn sealed_audit_stop(error: String) -> Value {
    assert!(error.contains(AUDIT_FAILURE_MARKER), "{error}");
    let directory = std::env::temp_dir().join(format!(
        "mtg-audit-failure-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    assert_eq!(publish_failure(&directory, error.clone()), error);
    let record: Value = serde_json::from_slice(
        &std::fs::read(directory.join("search-opponent-audit-failure.json")).unwrap(),
    )
    .unwrap();
    assert!(!directory.join("search-failure.json").exists());
    std::fs::remove_dir_all(&directory).unwrap();
    record
}
