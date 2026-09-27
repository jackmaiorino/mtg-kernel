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
