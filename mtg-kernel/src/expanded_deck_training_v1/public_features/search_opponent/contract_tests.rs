//! Engine contract of the search opponent (CODEX #521 C1, C2, C5): an absent
//! pin changes no serialized byte, every consumer but the public collector
//! refuses a search episode, and the run receipt binds the frozen identities.
use super::*;

fn deck(id: &str) -> ExpandedDeckListV1 {
    let registration = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    let cards = registration.registered_configuration();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: cards.mainboard().to_vec(),
        sideboard: cards.sideboard().to_vec(),
    }
}

fn pin(path: &str, sha256: &str) -> PinnedFileV1 {
    PinnedFileV1 {
        path: path.into(),
        sha256: sha256.into(),
    }
}

fn g115() -> ExpandedModelSourceV1 {
    ExpandedModelSourceV1 {
        play_import: pin("b-descriptor-windows.json", &"6c".repeat(32)),
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: "c4".repeat(32),
            expected_feature_encoding_digest: "27".repeat(32),
        },
        checkpoint: Some(pin("checkpoint.json", G115_CHECKPOINT_SHA256)),
    }
}

fn episode(search: bool) -> ExpandedEpisodeV1 {
    let decks = [deck("Affinity"), deck("Terror")];
    ExpandedEpisodeV1 {
        id: "search-contract".into(),
        seed: 18_295_962_125_787_485_905,
        starting_player: 1,
        learner_seat: 0,
        opponent: Some(g115()),
        opponent_kind: None,
        opponent_search: search.then(|| {
            pin(
                "d3-search-descriptor-reviewed.json",
                REVIEWED_DESCRIPTOR_SHA256,
            )
        }),
        registered: decks.clone(),
        selected: decks,
        postboard: false,
        max_physical_decisions: 40_000,
        max_policy_steps: 80_000,
    }
}

#[test]
fn absent_search_pin_keeps_every_serialized_byte() {
    let plain = episode(false);
    let bytes = serde_json::to_vec(&plain).unwrap();
    assert!(!String::from_utf8(bytes.clone())
        .unwrap()
        .contains("opponent_search"));
    let back: ExpandedEpisodeV1 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&back).unwrap(), bytes);
    let pinned = serde_json::to_vec(&episode(true)).unwrap();
    let back: ExpandedEpisodeV1 = serde_json::from_slice(&pinned).unwrap();
    assert_eq!(serde_json::to_vec(&back).unwrap(), pinned);
    assert!(back.opponent_search.is_some());
}

#[test]
fn only_the_public_collector_admits_a_search_episode() {
    let plain = episode(false);
    assert!(plain.configurations().is_ok());
    assert!(plain.configurations_for_public_collector_v1().is_ok());
    let search = episode(true);
    assert_eq!(
        search.configurations().unwrap_err(),
        "search opponent is admitted only by the public-feature collector"
    );
    assert!(search.configurations_for_public_collector_v1().is_ok());
    let mut orphan = episode(true);
    orphan.opponent = None;
    assert_eq!(
        orphan.configurations_for_public_collector_v1().unwrap_err(),
        "search opponent requires an explicit opponent model"
    );
}

#[test]
fn run_receipt_binds_frozen_identities_and_is_absent_for_ordinary_runs() {
    let config = |episodes: Vec<ExpandedEpisodeV1>| Config {
        source: g115(),
        updates: vec![episodes],
        inputs_enabled: false,
        learning_rate: 1e-4,
        value_coefficient: 0.5,
        gamma: 1.0,
        lambda: 0.9,
        gpu_ordinal: 1,
        max_chunk_substeps: 128,
        projection_mode: ProjectionMode::StateOnly,
        entropy_coefficient: 0.0,
    };
    assert!(run_receipt(&config(vec![episode(false)]), "c")
        .unwrap()
        .is_none());
    let receipt = run_receipt(&config(vec![episode(false), episode(true)]), "c")
        .unwrap()
        .unwrap();
    assert_eq!(receipt["descriptor_sha256"], REVIEWED_DESCRIPTOR_SHA256);
    assert_eq!(
        receipt["opponent_checkpoint_sha256"],
        G115_CHECKPOINT_SHA256
    );
    assert_eq!(receipt["search_episode_ids"], json!(["search-contract"]));
    assert_eq!(
        receipt["build"]["git_head"],
        env!("MTG_KERNEL_BUILD_GIT_HEAD")
    );
    assert_eq!(receipt["executable_sha256"].as_str().unwrap().len(), 64);
    let mut other = episode(true);
    other.opponent_search = Some(pin("other.json", &"00".repeat(32)));
    assert!(run_receipt(&config(vec![other]), "c").is_err());
    let mut other = episode(true);
    other.opponent = Some(ExpandedModelSourceV1 {
        checkpoint: None,
        ..g115()
    });
    assert!(run_receipt(&config(vec![other]), "c").is_err());
}
