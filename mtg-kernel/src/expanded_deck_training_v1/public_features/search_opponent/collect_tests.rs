//! Complete games through the public collector with the unchanged D3 wrapper
//! on the opponent seat (fixture nets, reviewed budget). Same seed gives the
//! same trajectory bytes; learner rows still replay the behavior sampler;
//! tampered search or learner rows are refused; ordinary validation refuses a
//! search trajectory; the loader enforces both frozen pins before any read.
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
        opponent_search: Some(pin("d3.json", REVIEWED_DESCRIPTOR_SHA256)),
        registered: [deck.clone(), deck.clone()],
        selected: [deck.clone(), deck],
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
    }
}

fn play(episode: &ExpandedEpisodeV1) -> Result<Trajectory, String> {
    let base = FrozenPlayPolicyV1::training_fixture_v4();
    let mut learner = PublicInputPlayPolicyV1::new(base, weights(&ProjectionSnapshot::zero())?)?
        .with_inputs_enabled(false);
    let net = FrozenPlayPolicyV1::training_fixture_v4();
    let search = SearchOpponentV1::new(
        &net,
        fixture_descriptor(&net),
        REVIEWED_DESCRIPTOR_SHA256.into(),
        1 - episode.learner_seat,
    )?;
    let id = identity(&net, Some(G115_CHECKPOINT_SHA256));
    collect_with_opponent(
        &mut learner,
        episode,
        "config",
        "state",
        false,
        (net, id),
        Some(search),
    )
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
            .validate(&a.episode, &hashes, &a.decisions, &a.terminal)
            .unwrap();
        assert!(
            validate_episode_records_v1(&a.episode, &hashes, &a.decisions, &a.terminal).is_err()
        );
        for seat in [1 - learner_seat, learner_seat] {
            let mut tampered = a.decisions.clone();
            let row = tampered
                .iter_mut()
                .find(|r| r.actor == seat && r.logits.len() > 1)
                .expect("a decision with a choice");
            row.selected = (row.selected + 1) % row.logits.len() as u32;
            assert!(search
                .validate(&a.episode, &hashes, &tampered, &a.terminal)
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
