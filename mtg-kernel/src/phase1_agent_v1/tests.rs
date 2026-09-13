use super::*;
use crate::bo3_match::{
    BestOfThreeMatchStateV1, GameOutcomeV1, MatchOutcomeV1, MatchPhaseV1, PlayDrawChoiceV1,
};
use crate::card_def::{card_id_by_name, KERNEL_CARDDB_HASH};
use crate::expanded_deck_training_v1::{
    ExpandedInferenceIdentityV1, ExpandedModelSourceV1, ExpandedSeatBehaviorV1, PinnedFileV1,
};
use crate::fast_sampler::WIDE_CATEGORICAL_SAMPLER_VERSION_V1;
use crate::ids::PlayerId;
use crate::learned_bo3_v1::Bo3OpeningProtocolV1;
use crate::learned_sideboard_v1::{LearnedSideboardInputV1, SideboardActionV1};
use crate::rl::{PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::sideboard::CardCountV1;
use crate::sideboard_play_policy_v1::{
    FrozenPlayObservationTransferV3, FrozenPlayPolicyIdentityV1, PlayModelIdentityV1,
};
use std::path::PathBuf;

fn digest(byte: char) -> String {
    std::iter::repeat_n(byte, 64).collect()
}
fn pin(name: &str) -> PinnedFileV1 {
    PinnedFileV1 {
        path: PathBuf::from(name),
        sha256: digest('1'),
    }
}

fn package() -> CompleteAgentPackageV1 {
    let card_db = format!("{KERNEL_CARDDB_HASH:016x}");
    let model = PlayModelIdentityV1 {
        schema: "mtg-kernel-actual-play-model/v1".into(),
        weights_sha256: digest('2'),
        model_parameter_sha256: digest('3'),
        embedding_table_sha256: digest('4'),
        feature_contract_digest: digest('5'),
        feature_encoding_digest: digest('6'),
        card_db_hash: card_db.clone(),
    };
    // Deliberately different ancestry weights: package bindings must use model.weights.
    let ancestry = FrozenPlayPolicyIdentityV1 {
        schema: "fixture-ancestry".into(),
        source_export_schema: "fixture".into(),
        source_metadata_sha256: digest('1'),
        weights_sha256: digest('9'),
        model_parameter_sha256: digest('9'),
        source_run_sha256: digest('1'),
        source_generation: 1,
        source_git_commit: "a".repeat(40),
        source_card_db_hash: card_db.clone(),
        destination_card_db_hash: card_db.clone(),
        source_registry_sha256: digest('1'),
        destination_registry_sha256: digest('7'),
        source_card_count: 184,
        destination_card_count: 184,
        source_training_deck_ids: vec!["fixture".into()],
        namespace_rule: "fixture".into(),
        appended_rows: "fixture".into(),
        feature_contract_digest: digest('5'),
        feature_encoding_digest: digest('6'),
        sampler_identity: WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into(),
        reader_revalidated_store_chain: false,
        observation_successor: None,
    };
    CompleteAgentPackageV1 {
        schema: COMPLETE_AGENT_PACKAGE_SCHEMA_V1.into(),
        runtime: AgentRuntimeIdentityV1 {
            executable: pin("fixture-executable"),
            toolchain: pin("fixture-toolchain"),
            engine_commit: "a".repeat(40),
            tracked_tree_sha256: digest('c'),
            tracked_tree_contract: "fixture-tree-contract".into(),
            build_git_clean: true,
            card_db_hash: card_db,
            card_registry_sha256: digest('7'),
            feature_contract_digest: digest('5'),
            feature_encoding_digest: digest('6'),
            features_source_sha256: digest('8'),
            feature_descriptor_sha256: digest('a'),
        },
        gameplay: ExpandedSeatBehaviorV1 {
            source: ExpandedModelSourceV1 {
                play_import: pin("fixture-import"),
                checkpoint: Some(pin("fixture-checkpoint")),
                feature_transfer: FrozenPlayObservationTransferV3 {
                    expected_feature_contract_digest: digest('5'),
                    expected_feature_encoding_digest: digest('6'),
                },
            },
            identity: ExpandedInferenceIdentityV1 {
                schema: "mtg-kernel-expanded-deck-inference/v1".into(),
                source_import: ancestry,
                checkpoint_sha256: Some(digest('1')),
                model,
                state_sha256: digest('b'),
                adam_step: 483,
                feature_schema_version: "fixture-v6".into(),
                feature_registry_version: "fixture-v3".into(),
                features_source_sha256: digest('8'),
                feature_descriptor_sha256: digest('a'),
            },
        },
        gameplay_sampler_identity: WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into(),
        opening: AgentOpeningPolicyV1::Existing {
            protocol: Bo3OpeningProtocolV1::KeepSevenV2,
        },
        play_draw: AgentPlayDrawPolicyV1::Fixed {
            choice: PlayDrawChoiceV1::Play,
        },
        sideboard: AgentSideboardPolicyV1::Keep,
        search: AgentSearchPolicyV1::Disabled,
    }
}

fn deck() -> OwnDeckConfigurationV1 {
    let forest = card_id_by_name("Forest").unwrap();
    OwnDeckConfigurationV1 {
        mainboard: vec![forest; 60],
        sideboard: vec![forest; 15],
    }
}

fn record(
    index: &mut u64,
    actor: PlayerSeatV1,
    hash: &str,
    visible: ActorVisibleDecisionV1,
) -> Bo3DecisionRecordV1 {
    let result = Bo3DecisionRecordV1 {
        decision_index: *index,
        actor,
        behavior_package_sha256: hash.into(),
        behavior: BehaviorDistributionV1::Deterministic { selected_index: 0 },
        visible,
    };
    *index += 1;
    result
}

/// Synthetic outcome sequence for structural tests, not played match evidence.
fn trajectory(
    package: &CompleteAgentPackageV1,
    outcomes: &[TerminalOutcomeV1],
    id: &str,
) -> Bo3TrainingTrajectoryV1 {
    let hash = package.package_sha256_v1().unwrap();
    let own = deck();
    let mut state = BestOfThreeMatchStateV1::new_v1(PlayerId::P0).unwrap();
    let mut index = 0;
    let mut games = Vec::new();
    for (ordinal, &outcome) in outcomes.iter().enumerate() {
        let game_index = ordinal as u8 + 1;
        let wins = [
            state.wins(PlayerId::P0).unwrap(),
            state.wins(PlayerId::P1).unwrap(),
        ];
        let chooser = match state.phase() {
            MatchPhaseV1::AwaitingPlayDrawChoice { chooser, .. } => chooser,
            _ => panic!("fixture after completion"),
        };
        let mut decisions = Vec::new();
        if ordinal > 0 {
            for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
                let seat = if actor == PlayerSeatV1::P0 { 0 } else { 1 };
                decisions.push(record(
                    &mut index,
                    actor,
                    &hash,
                    ActorVisibleDecisionV1::Sideboard {
                        input: LearnedSideboardInputV1 {
                            registered_cards: vec![CardCountV1 {
                                card_id: own.mainboard[0],
                                count: 75,
                            }],
                            own_card_outcomes: vec![],
                            opponent_evidence: vec![],
                            resource_summaries: vec![],
                            next_game_number: game_index,
                            acting_player_games_won: wins[seat],
                            opponent_games_won: wins[1 - seat],
                        },
                        ordered_actions: vec![SideboardActionV1::Done],
                    },
                ));
            }
        }
        decisions.push(record(
            &mut index,
            chooser.into(),
            &hash,
            ActorVisibleDecisionV1::PlayDraw {
                own_configuration: own.clone(),
                own_games_won: wins[chooser.index()],
                opponent_games_won: wins[1 - chooser.index()],
                ordered_choices: vec![PlayDrawChoiceV1::Play, PlayDrawChoiceV1::Draw],
            },
        ));
        let start = state
            .choose_play_draw_v1(chooser, PlayDrawChoiceV1::Play)
            .unwrap();
        for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
            decisions.push(record(
                &mut index,
                actor,
                &hash,
                ActorVisibleDecisionV1::Mulligan {
                    input: ActorOpeningInputV1 {
                        own_configuration: own.clone(),
                        own_hand: vec![own.mainboard[0]; 7],
                        mulligans_taken: 0,
                        remaining_bottom: 0,
                        starting_player: start.starting_player.into(),
                        opponent_hand_count: 7,
                        opponent_has_kept: actor == PlayerSeatV1::P1,
                    },
                    ordered_choices: vec![MulliganChoiceV1::Keep, MulliganChoiceV1::Mulligan],
                },
            ));
        }
        let session = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
            u64::from(game_index), 42, 100, 100, ["fixture-p0".into(), "fixture-p1".into()],
            [own.mainboard.clone(), own.mainboard.clone()], start.starting_player).unwrap();
        let width = match session.current_response() {
            FastActorResponseV1::Decision(value) => value.legal_action_count,
            _ => panic!("fixture terminal"),
        };
        decisions.push(
            Bo3DecisionRecordV1::gameplay_from_session_v1(
                index,
                hash.clone(),
                BehaviorDistributionV1::hamilton_from_logits_v1(&vec![0.0; width as usize], 0)
                    .unwrap(),
                &session,
            )
            .unwrap(),
        );
        index += 1;
        let result = match outcome {
            TerminalOutcomeV1::P0Win => GameOutcomeV1::Win {
                winner: PlayerId::P0,
            },
            TerminalOutcomeV1::P1Win => GameOutcomeV1::Win {
                winner: PlayerId::P1,
            },
            TerminalOutcomeV1::Draw => GameOutcomeV1::Draw,
            _ => panic!("fixture is natural"),
        };
        state.record_game_result_v1(result).unwrap();
        games.push(Bo3TrainingGameV1 {
            game_index,
            start: Some(start),
            decisions,
            terminal: Some(Bo3GameTerminalV1 {
                classification: TerminalClassificationV1::Natural,
                outcome,
                gameplay_decision_count: 1,
            }),
        });
    }
    Bo3TrainingTrajectoryV1 {
        schema: BO3_TRAINING_TRAJECTORY_SCHEMA_V1.into(),
        match_id: id.into(),
        initial_chooser: PlayerSeatV1::P0,
        behavior_packages_by_seat: [hash.clone(), hash],
        registrations_by_seat: [own.clone(), own],
        games,
        ending: state.outcome().map_or(
            Bo3TrajectoryEndingV1::Incomplete {
                reason: IncompleteMatchReasonV1::PhysicalGameCap,
            },
            |outcome| Bo3TrajectoryEndingV1::Complete { outcome },
        ),
    }
}

#[test]
fn package_roundtrip_has_exact_identity() {
    let value = package();
    let decoded =
        CompleteAgentPackageV1::from_json_v1(&serde_json::to_string(&value).unwrap()).unwrap();
    assert_eq!(value, decoded);
    assert_eq!(
        value.package_sha256_v1().unwrap(),
        decoded.package_sha256_v1().unwrap()
    );
}

#[test]
fn package_changes_when_any_runtime_artifact_changes() {
    let value = package();
    let mut changed = value.clone();
    changed.runtime.executable.sha256 = digest('c');
    assert_ne!(
        value.package_sha256_v1().unwrap(),
        changed.package_sha256_v1().unwrap()
    );
    assert_eq!(
        value.gameplay.identity.model,
        changed.gameplay.identity.model
    );
    assert_eq!(value.sideboard, changed.sideboard);
}

#[test]
fn ancestry_weights_cannot_bind_sideboard_head() {
    let mut value = package();
    value.sideboard = AgentSideboardPolicyV1::LearnedGreedyV1 {
        checkpoint: pin("head"),
        play_identity: crate::learned_sideboard_v1::SideboardPlayIdentityV1 {
            weights_sha256: value.gameplay.identity.source_import.weights_sha256.clone(),
            git_head: "a".repeat(40),
        },
        embedding_table_sha256: value.gameplay.identity.model.embedding_table_sha256.clone(),
    };
    assert!(value.validate_metadata_v1().is_err());
}

#[test]
fn package_feature_mismatch_and_legacy_opening_are_rejected() {
    let mut value = package();
    value.runtime.feature_encoding_digest = digest('f');
    assert!(value.validate_metadata_v1().is_err());
    let mut value = package();
    value.opening = AgentOpeningPolicyV1::Existing {
        protocol: Bo3OpeningProtocolV1::LegacyKeepSevenV1,
    };
    assert!(value.validate_metadata_v1().is_err());
}

#[test]
fn duplicate_json_fields_and_hidden_input_fields_are_rejected() {
    let value = serde_json::to_string(&package()).unwrap();
    let duplicate = value.replacen('{', "{\"schema\":\"injected\",", 1);
    assert!(CompleteAgentPackageV1::from_json_v1(&duplicate).is_err());
    let mut input = serde_json::to_value(ActorOpeningInputV1 {
        own_configuration: deck(),
        own_hand: vec![],
        mulligans_taken: 0,
        remaining_bottom: 0,
        starting_player: PlayerSeatV1::P0,
        opponent_hand_count: 7,
        opponent_has_kept: false,
    })
    .unwrap();
    input["opponent_deck_id"] = serde_json::json!("hidden-label");
    assert!(serde_json::from_value::<ActorOpeningInputV1>(input).is_err());
}

#[test]
fn probability_errors_and_exact_q64_roundtrip() {
    for values in [
        vec![f64::NAN, 1.0],
        vec![-0.1, 1.1],
        vec![0.2, 0.2],
        vec![0.0, 1.0],
    ] {
        assert!(BehaviorDistributionV1::Categorical {
            selected_index: 0,
            probabilities: values
        }
        .selected_probability_v1(2)
        .is_err());
    }
    let exact = BehaviorDistributionV1::HamiltonQ64 {
        selected_index: 0,
        mass_numerators: vec!["18446744073709551616".into()],
    };
    assert_eq!(exact.selected_probability_v1(1).unwrap(), 1.0);
    let value = serde_json::to_string(&exact).unwrap();
    assert_eq!(
        exact,
        serde_json::from_str::<BehaviorDistributionV1>(&value).unwrap()
    );
    assert!(BehaviorDistributionV1::HamiltonQ64 {
        selected_index: 0,
        mass_numerators: vec!["018446744073709551616".into()]
    }
    .selected_probability_v1(1)
    .is_err());
}

#[test]
fn two_three_and_draw_extended_games_validate() {
    let value = package();
    for outcomes in [
        vec![TerminalOutcomeV1::P0Win, TerminalOutcomeV1::P0Win],
        vec![
            TerminalOutcomeV1::P0Win,
            TerminalOutcomeV1::P1Win,
            TerminalOutcomeV1::P0Win,
        ],
        vec![
            TerminalOutcomeV1::Draw,
            TerminalOutcomeV1::P0Win,
            TerminalOutcomeV1::P1Win,
            TerminalOutcomeV1::P0Win,
        ],
    ] {
        let data = trajectory(&value, &outcomes, "fixture");
        let validated = data.validate_v1([&value, &value]).unwrap();
        assert_eq!(validated.match_return_v1(PlayerSeatV1::P0), Some(1.0));
        assert_eq!(validated.match_return_v1(PlayerSeatV1::P1), Some(-1.0));
    }
}

#[test]
fn altered_game_order_actor_package_and_result_are_rejected() {
    let value = package();
    let data = trajectory(
        &value,
        &[TerminalOutcomeV1::P0Win, TerminalOutcomeV1::P0Win],
        "fixture",
    );
    let mut changed = data.clone();
    changed.games[1].game_index = 3;
    assert!(changed.validate_v1([&value, &value]).is_err());
    let mut changed = data.clone();
    changed.games[0].decisions[0].actor = PlayerSeatV1::P1;
    assert!(changed.validate_v1([&value, &value]).is_err());
    let mut changed = data.clone();
    changed.games[0].decisions[0].behavior_package_sha256 = digest('f');
    assert!(changed.validate_v1([&value, &value]).is_err());
    let mut changed = data;
    changed.ending = Bo3TrajectoryEndingV1::Complete {
        outcome: MatchOutcomeV1::Winner {
            winner: PlayerId::P1,
        },
    };
    assert!(changed.validate_v1([&value, &value]).is_err());
}

#[test]
fn equal_match_weights_exclude_incomplete_and_do_not_reward_length() {
    let value = package();
    let short = trajectory(
        &value,
        &[TerminalOutcomeV1::P0Win, TerminalOutcomeV1::P0Win],
        "short",
    );
    let long = trajectory(
        &value,
        &[
            TerminalOutcomeV1::P0Win,
            TerminalOutcomeV1::P1Win,
            TerminalOutcomeV1::P0Win,
        ],
        "long",
    );
    let partial = trajectory(&value, &[TerminalOutcomeV1::P0Win], "partial");
    let validated = [
        short.validate_v1([&value, &value]).unwrap(),
        long.validate_v1([&value, &value]).unwrap(),
        partial.validate_v1([&value, &value]).unwrap(),
    ];
    assert!(!validated[2].is_complete_v1());
    assert_eq!(validated[2].match_return_v1(PlayerSeatV1::P0), None);
    let weights = equal_match_weights_v1(
        &validated,
        PlayerSeatV1::P0,
        LearningComponentV1::Mulligan,
        &value.package_sha256_v1().unwrap(),
    )
    .unwrap();
    for id in ["short", "long"] {
        assert!(
            (weights
                .iter()
                .filter(|row| row.match_id == id)
                .map(|row| row.weight)
                .sum::<f64>()
                - 0.5)
                .abs()
                < 1e-12
        );
    }
    assert!(weights.iter().all(|row| row.actor == PlayerSeatV1::P0
        && row.component == LearningComponentV1::Mulligan
        && row.match_id != "partial"));
    assert!(equal_match_weights_v1(
        &validated,
        PlayerSeatV1::P0,
        LearningComponentV1::Mulligan,
        &digest('f')
    )
    .is_err());
}

#[test]
fn capped_game_cannot_emit_match_targets_or_continue() {
    let value = package();
    let mut data = trajectory(&value, &[TerminalOutcomeV1::P0Win], "capped");
    data.games[0].terminal = Some(Bo3GameTerminalV1 {
        classification: TerminalClassificationV1::Truncated,
        outcome: TerminalOutcomeV1::Truncated,
        gameplay_decision_count: 1,
    });
    data.ending = Bo3TrajectoryEndingV1::Incomplete {
        reason: IncompleteMatchReasonV1::DecisionCap,
    };
    assert!(!data.validate_v1([&value, &value]).unwrap().is_complete_v1());
    data.ending = Bo3TrajectoryEndingV1::Complete {
        outcome: MatchOutcomeV1::Winner {
            winner: PlayerId::P0,
        },
    };
    assert!(data.validate_v1([&value, &value]).is_err());
}

fn learned_opening_package() -> CompleteAgentPackageV1 {
    let mut value = package();
    value.opening = AgentOpeningPolicyV1::LearnedLondonV1 {
        policy: AuxiliaryPolicyBindingV1 {
            checkpoint: pin("unimplemented-opening"),
            play_weights_sha256: value.gameplay.identity.model.weights_sha256.clone(),
            embedding_table_sha256: value.gameplay.identity.model.embedding_table_sha256.clone(),
            feature_contract_digest: value.runtime.feature_contract_digest.clone(),
            feature_encoding_digest: value.runtime.feature_encoding_digest.clone(),
            card_db_hash: value.runtime.card_db_hash.clone(),
        },
    };
    value
}

#[test]
fn future_descriptor_never_falls_back_to_fixed_opening() {
    let value = learned_opening_package();
    value.validate_metadata_v1().unwrap();
    match value.load_supported_components_v1() {
        Ok(_) => panic!("unimplemented opening must not load"),
        Err(error) => assert!(error.contains("not implemented")),
    }
}

#[test]
fn partial_london_mulligan_and_bottom_sequence_validates_without_targets() {
    let value = learned_opening_package();
    let mut data = trajectory(&value, &[TerminalOutcomeV1::P0Win], "opening-prefix");
    let game = &mut data.games[0];
    // Replace the first keep with Mulligan, a fresh seven-card Keep and one
    // conditional bottom choice. End before gameplay. No outcome is invented.
    let mut redraw_keep = game.decisions[1].clone();
    game.decisions[1].behavior = BehaviorDistributionV1::Categorical {
        selected_index: 1,
        probabilities: vec![0.5, 0.5],
    };
    if let ActorVisibleDecisionV1::Mulligan { input, .. } = &mut redraw_keep.visible {
        input.mulligans_taken = 1;
    }
    let mut bottom = redraw_keep.clone();
    let input = match &redraw_keep.visible {
        ActorVisibleDecisionV1::Mulligan { input, .. } => input.clone(),
        _ => unreachable!(),
    };
    bottom.visible = ActorVisibleDecisionV1::Bottom {
        input: ActorOpeningInputV1 {
            remaining_bottom: 1,
            ..input
        },
        ordered_hand_indices: (0..7).collect(),
    };
    bottom.behavior = BehaviorDistributionV1::Categorical {
        selected_index: 2,
        probabilities: vec![1.0 / 7.0; 7],
    };
    game.decisions.insert(2, redraw_keep);
    game.decisions.insert(3, bottom);
    game.decisions.pop(); // Remove synthetic gameplay record.
    for (index, decision) in game.decisions.iter_mut().enumerate() {
        decision.decision_index = index as u64;
    }
    game.terminal = None;
    data.ending = Bo3TrajectoryEndingV1::Incomplete {
        reason: IncompleteMatchReasonV1::Interrupted,
    };
    assert!(!data.validate_v1([&value, &value]).unwrap().is_complete_v1());
    if let ActorVisibleDecisionV1::Bottom { input, .. } = &mut data.games[0].decisions[3].visible {
        input.remaining_bottom = 2;
    }
    assert!(data.validate_v1([&value, &value]).is_err());
}

#[test]
fn omitted_sideboard_done_and_future_evidence_are_rejected() {
    let value = package();
    let data = trajectory(
        &value,
        &[TerminalOutcomeV1::P0Win, TerminalOutcomeV1::P0Win],
        "fixture",
    );
    let mut omitted = data.clone();
    omitted.games[1].decisions.remove(0);
    let mut index = 0;
    for game in &mut omitted.games {
        for decision in &mut game.decisions {
            decision.decision_index = index;
            index += 1;
        }
    }
    assert!(omitted.validate_v1([&value, &value]).is_err());
    let mut future = data;
    if let ActorVisibleDecisionV1::Sideboard { input, .. } =
        &mut future.games[1].decisions[0].visible
    {
        input
            .opponent_evidence
            .push(crate::learned_sideboard_v1::SideboardOpponentEvidenceV1 {
                card_id: Some(deck().mainboard[0]),
                game_index: 2,
                first_seen_turn: 1,
                zone: crate::learned_sideboard_v1::VisibleEvidenceZoneV1::Battlefield,
            });
    }
    assert!(future.validate_v1([&value, &value]).is_err());
}

#[test]
fn duplicate_match_ids_do_not_double_weight_training() {
    let value = package();
    let data = trajectory(
        &value,
        &[TerminalOutcomeV1::P0Win, TerminalOutcomeV1::P0Win],
        "duplicate",
    );
    let validated = [
        data.validate_v1([&value, &value]).unwrap(),
        data.validate_v1([&value, &value]).unwrap(),
    ];
    assert!(equal_match_weights_v1(
        &validated,
        PlayerSeatV1::P0,
        LearningComponentV1::Gameplay,
        &value.package_sha256_v1().unwrap()
    )
    .is_err());
}

fn current_runtime_fixture() -> AgentRuntimeIdentityV1 {
    use sha2::{Digest, Sha256};
    let executable = std::env::current_exe().unwrap();
    let toolchain = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("rust-toolchain.toml");
    AgentRuntimeIdentityV1 {
        executable: PinnedFileV1 {
            sha256: format!("{:x}", Sha256::digest(std::fs::read(&executable).unwrap())),
            path: executable,
        },
        toolchain: PinnedFileV1 {
            path: toolchain,
            sha256: format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../../rust-toolchain.toml"))
            ),
        },
        engine_commit: env!("MTG_KERNEL_BUILD_GIT_HEAD").into(),
        tracked_tree_sha256: env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256").into(),
        tracked_tree_contract: env!("MTG_KERNEL_BUILD_TRACKED_TREE_CONTRACT").into(),
        build_git_clean: env!("MTG_KERNEL_BUILD_GIT_CLEAN") == "true",
        card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        card_registry_sha256: format!(
            "{:x}",
            Sha256::digest(include_bytes!("../../../data/cards_v1.json"))
        ),
        feature_contract_digest: crate::native_flat_tensorizer_v3::FEATURE_CONTRACT_DIGEST_V3
            .into(),
        feature_encoding_digest: crate::native_flat_tensorizer_v3::FEATURE_ENCODING_DIGEST_V3
            .into(),
        features_source_sha256: crate::native_flat_tensorizer_v3::FEATURES_SOURCE_SHA256_V3.into(),
        feature_descriptor_sha256: crate::native_flat_tensorizer_v3::FEATURE_DESCRIPTOR_SHA256_V3
            .into(),
    }
}

#[test]
fn matching_head_and_valid_foreign_file_pin_do_not_verify_current_executable() {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    struct TemporaryFile(PathBuf);
    impl Drop for TemporaryFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let file = TemporaryFile(std::env::temp_dir().join(format!(
        "phase1-foreign-executable-{}-{nonce}.bin",
        std::process::id()
    )));
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file.0)
        .unwrap();
    output
        .write_all(b"different executable with an honestly matching supplied-file digest")
        .unwrap();
    output.sync_all().unwrap();
    drop(output);
    let mut runtime = current_runtime_fixture();
    runtime.executable = PinnedFileV1 {
        path: file.0.clone(),
        sha256: format!("{:x}", Sha256::digest(std::fs::read(&file.0).unwrap())),
    };
    assert_eq!(runtime.engine_commit, env!("MTG_KERNEL_BUILD_GIT_HEAD"));
    assert!(runtime
        .verify_current_runtime_v1()
        .unwrap_err()
        .contains("currently running executable"));
}

#[test]
fn current_executable_does_not_override_wrong_tracked_tree() {
    let mut runtime = current_runtime_fixture();
    runtime.tracked_tree_sha256 = if runtime.tracked_tree_sha256 == digest('0') {
        digest('1')
    } else {
        digest('0')
    };
    assert!(runtime
        .verify_current_runtime_v1()
        .unwrap_err()
        .contains("tracked-tree identity"));
}

#[test]
fn exact_current_identity_requires_clean_compiled_source() {
    let runtime = current_runtime_fixture();
    if env!("MTG_KERNEL_BUILD_GIT_CLEAN") == "true" {
        let actual = runtime.verify_current_runtime_v1().unwrap();
        assert_eq!(actual.executable_sha256_v1(), runtime.executable.sha256);
    } else {
        assert!(runtime
            .verify_current_runtime_v1()
            .unwrap_err()
            .contains("clean compiled source tree"));
        let mut claimed_clean = runtime;
        claimed_clean.build_git_clean = true;
        assert!(claimed_clean
            .verify_current_runtime_v1()
            .unwrap_err()
            .contains("clean compiled source tree"));
    }
}
