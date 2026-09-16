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
    FreshPlayPolicyIdentityV1, FrozenPlayObservationTransferV3, FrozenPlayPolicyIdentityV1,
    PlayModelIdentityV1, PlayPolicyOriginV1, FRESH_PLAY_INITIALIZATION_SCHEMA_V1,
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
                source_import: ancestry.into(),
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
    // Imported origins retain their exact pre-enum object wire shape.
    assert_eq!(
        serde_json::to_value(&value.gameplay.identity.source_import).unwrap(),
        serde_json::to_value(
            value
                .gameplay
                .identity
                .source_import
                .as_imported_v1()
                .unwrap()
        )
        .unwrap()
    );
}

#[test]
fn imported_package_cannot_claim_fresh_inference_schema() {
    let mut value = package();
    value.gameplay.identity.schema = "mtg-kernel-expanded-deck-inference/v2".into();
    assert!(value.validate_metadata_v1().is_err());
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
            weights_sha256: value
                .gameplay
                .identity
                .source_import
                .initial_weights_sha256_v1()
                .to_owned(),
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

// A LearnedLondonV1 opening is admissible only for a fresh gameplay identity
// on the V4 runtime contract (item 19). `learned_opening_package()`'s
// gameplay identity is Imported (frozen ancestry, from `package()`), so this
// is still rejected, now by the new structural gate rather than a blanket
// "not implemented" refusal.
#[test]
fn imported_gameplay_identity_rejects_learned_opening_package() {
    let value = learned_opening_package();
    value.validate_metadata_v1().unwrap();
    match value.load_supported_components_v1() {
        Ok(_) => panic!("an imported (frozen) gameplay identity must not admit a learned opening"),
        Err(error) => assert!(error.contains("imported (frozen) gameplay identity")),
    }
}

// A plain Existing{KeepSevenV2}+Fixed+Disabled package must never touch the
// new learned-policy gate at all; it still fails only on runtime/file
// verification, exactly as it did before item 19.
#[test]
fn existing_keep_seven_package_bypasses_the_learned_policy_gate() {
    let value = package();
    match value.load_supported_components_v1() {
        Ok(_) => panic!("fixture runtime/files are not real; success is not expected here"),
        Err(error) => assert!(
            !error.contains("learned opening/play-draw"),
            "plain KeepSevenV2/Fixed packages must not reach the learned-policy gate: {error}"
        ),
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

/// The same real, currently-running-binary evidence as `current_runtime_fixture`,
/// with the feature tuple swapped for the compiled V4 (fresh-lineage) contract.
fn v4_runtime_fixture() -> AgentRuntimeIdentityV1 {
    AgentRuntimeIdentityV1 {
        feature_contract_digest: crate::native_flat_tensorizer_v4::FEATURE_CONTRACT_DIGEST_V4
            .into(),
        feature_encoding_digest: crate::native_flat_tensorizer_v4::FEATURE_ENCODING_DIGEST_V4
            .into(),
        features_source_sha256: crate::native_flat_tensorizer_v4::FEATURES_SOURCE_SHA256_V4.into(),
        feature_descriptor_sha256: crate::native_flat_tensorizer_v4::FEATURE_DESCRIPTOR_SHA256_V4
            .into(),
        ..current_runtime_fixture()
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

// Item 18: verify_current_runtime_v1 matches an explicit two-entry compiled
// table (V3, V4), never an open-ended allow-list or partial match.

#[test]
fn v3_feature_identity_verifies_as_generation_v3() {
    let runtime = current_runtime_fixture();
    if env!("MTG_KERNEL_BUILD_GIT_CLEAN") != "true" {
        return; // Covered separately by exact_current_identity_requires_clean_compiled_source.
    }
    let actual = runtime.verify_current_runtime_v1().unwrap();
    assert_eq!(actual.generation_v1(), RuntimeContractGenerationV1::V3);
}

#[test]
fn v4_feature_identity_verifies_as_generation_v4() {
    let runtime = v4_runtime_fixture();
    if env!("MTG_KERNEL_BUILD_GIT_CLEAN") != "true" {
        return;
    }
    let actual = runtime.verify_current_runtime_v1().unwrap();
    assert_eq!(actual.generation_v1(), RuntimeContractGenerationV1::V4);
}

#[test]
fn mixed_generation_feature_tuple_is_rejected() {
    if env!("MTG_KERNEL_BUILD_GIT_CLEAN") != "true" {
        return; // Reaching the generation match requires a clean-tree pass first.
    }
    // V3 contract digest paired with a V4 encoding digest.
    let mut mixed = current_runtime_fixture();
    mixed.feature_encoding_digest =
        crate::native_flat_tensorizer_v4::FEATURE_ENCODING_DIGEST_V4.into();
    assert!(mixed
        .verify_current_runtime_v1()
        .unwrap_err()
        .contains("neither compiled V3 nor V4"));
    // And vice versa: V4 contract digest paired with a V3 encoding digest.
    let mut mixed = v4_runtime_fixture();
    mixed.feature_encoding_digest =
        crate::native_flat_tensorizer_v3::FEATURE_ENCODING_DIGEST_V3.into();
    assert!(mixed
        .verify_current_runtime_v1()
        .unwrap_err()
        .contains("neither compiled V3 nor V4"));
}

#[test]
fn any_single_altered_feature_field_is_rejected_for_either_generation() {
    if env!("MTG_KERNEL_BUILD_GIT_CLEAN") != "true" {
        return;
    }
    for baseline in [current_runtime_fixture(), v4_runtime_fixture()] {
        let bad = digest('9');
        let mut altered = baseline.clone();
        altered.feature_contract_digest = bad.clone();
        assert!(altered
            .verify_current_runtime_v1()
            .unwrap_err()
            .contains("neither compiled V3 nor V4"));

        let mut altered = baseline.clone();
        altered.feature_encoding_digest = bad.clone();
        assert!(altered
            .verify_current_runtime_v1()
            .unwrap_err()
            .contains("neither compiled V3 nor V4"));

        let mut altered = baseline.clone();
        altered.features_source_sha256 = bad.clone();
        assert!(altered
            .verify_current_runtime_v1()
            .unwrap_err()
            .contains("neither compiled V3 nor V4"));

        let mut altered = baseline;
        altered.feature_descriptor_sha256 = bad;
        assert!(altered
            .verify_current_runtime_v1()
            .unwrap_err()
            .contains("neither compiled V3 nor V4"));
    }
}

// Item 19: LearnedLondonV1/LearnedV1 admission requires a fresh gameplay
// identity AND a verified V4 runtime; a package whose digests coincidentally
// line up but whose origin is Imported must still be rejected.

/// A `LearnedLondonV1` package whose declared gameplay identity is Fresh and
/// whose runtime metadata is fully self-consistent with `runtime`. The
/// `play_import` pin points at a real, existing (non-JSON) file so that, if
/// admission succeeds, the failure surfaces from the loader parsing it, not
/// from a missing fixture file standing in for our own gate.
fn learned_opening_fresh_package(runtime: AgentRuntimeIdentityV1) -> CompleteAgentPackageV1 {
    let mut value = package();
    value.runtime = runtime;
    value.gameplay.identity.model.card_db_hash = value.runtime.card_db_hash.clone();
    value.gameplay.identity.model.feature_contract_digest =
        value.runtime.feature_contract_digest.clone();
    value.gameplay.identity.model.feature_encoding_digest =
        value.runtime.feature_encoding_digest.clone();
    value.gameplay.identity.features_source_sha256 = value.runtime.features_source_sha256.clone();
    value.gameplay.identity.feature_descriptor_sha256 =
        value.runtime.feature_descriptor_sha256.clone();
    value.gameplay.source.feature_transfer.expected_feature_contract_digest =
        value.runtime.feature_contract_digest.clone();
    value.gameplay.source.feature_transfer.expected_feature_encoding_digest =
        value.runtime.feature_encoding_digest.clone();
    value.gameplay.identity.schema = "mtg-kernel-expanded-deck-inference/v2".into();
    value.gameplay.identity.source_import =
        PlayPolicyOriginV1::FreshInitialization(FreshPlayPolicyIdentityV1 {
            schema: FRESH_PLAY_INITIALIZATION_SCHEMA_V1.into(),
            initialization_manifest_sha256: digest('1'),
            lineage_id: "fixture-lineage".into(),
            initializer: "trainer-seeded-v1".into(),
            base_seed: 1,
            model_init_seed: 1,
            seed_derivation: "kernel-python-rl-trainer-sha256-v2".into(),
            producer_git_commit: "a".repeat(40),
            initial_weights_sha256: digest('1'),
            initial_model_parameter_sha256: digest('1'),
            parameter_layout_sha256: digest('1'),
            destination_registry_sha256: value.runtime.card_registry_sha256.clone(),
            destination_card_db_hash: value.runtime.card_db_hash.clone(),
            destination_card_count: crate::card_def::CARD_DEFS.len(),
            feature_contract_digest: value.runtime.feature_contract_digest.clone(),
            feature_encoding_digest: value.runtime.feature_encoding_digest.clone(),
            features_source_sha256: value.runtime.features_source_sha256.clone(),
            feature_descriptor_sha256: value.runtime.feature_descriptor_sha256.clone(),
            sampler_identity: WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into(),
        });
    value.gameplay.source.play_import = PinnedFileV1 {
        path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("rust-toolchain.toml"),
        sha256: {
            use sha2::{Digest, Sha256};
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../../rust-toolchain.toml"))
            )
        },
    };
    value.gameplay.source.checkpoint = None;
    value.gameplay.identity.checkpoint_sha256 = None;
    value.opening = AgentOpeningPolicyV1::LearnedLondonV1 {
        policy: AuxiliaryPolicyBindingV1 {
            checkpoint: pin("fixture-learned-opening"),
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
fn fresh_identity_with_v4_runtime_admits_learned_opening_past_the_gate() {
    if env!("MTG_KERNEL_BUILD_GIT_CLEAN") != "true" {
        return;
    }
    let value = learned_opening_fresh_package(v4_runtime_fixture());
    value.validate_metadata_v1().unwrap();
    assert!(value.gameplay.identity.source_import.is_fresh_v1());
    assert_eq!(
        value.runtime.verify_current_runtime_v1().unwrap().generation_v1(),
        RuntimeContractGenerationV1::V4
    );
    match value.load_supported_components_v1() {
        Ok(_) => panic!(
            "the play_import fixture is deliberately not a loadable checkpoint; \
             full success is not expected here"
        ),
        Err(error) => {
            // Neither of our new admission-gate messages: the gate let this
            // package through, and it failed only in the loader beyond it.
            assert!(!error.contains("learned opening/play-draw"));
            assert!(!error.contains("not implemented by this interface"));
        }
    }
}

#[test]
fn fresh_identity_with_v3_runtime_rejects_learned_opening() {
    if env!("MTG_KERNEL_BUILD_GIT_CLEAN") != "true" {
        return;
    }
    let value = learned_opening_fresh_package(current_runtime_fixture());
    value.validate_metadata_v1().unwrap();
    assert!(value.gameplay.identity.source_import.is_fresh_v1());
    match value.load_supported_components_v1() {
        Ok(_) => panic!("a V3 runtime must not admit a learned opening"),
        Err(error) => assert!(error.contains("fresh-lineage (V4) runtime contract")),
    }
}

// Follow-up coverage from the refutation pass on item 19: the earlier
// `imported_gameplay_identity_rejects_learned_opening_package` used
// `package()`'s placeholder runtime digests, so the origin gate fired before
// `verify_current_runtime_v1` ever ran. These tests force a genuinely
// verifying V4 runtime alongside an Imported origin, so the origin gate is
// proven to hold on its own merits, not merely because the runtime also
// failed to verify. They also add direct `AgentPlayDrawPolicyV1::LearnedV1`
// coverage, mirroring the existing `LearnedLondonV1` opening tests.

/// Starts from `learned_opening_fresh_package` (fresh origin, runtime-aligned
/// digests, a real non-checkpoint `play_import` file) and forces the
/// gameplay identity back to an Imported (frozen) origin, keeping every
/// digest field aligned with `runtime` so `validate_metadata_v1` still
/// passes and the rejection can only come from the origin's own kind.
fn imported_learned_opening_package(runtime: AgentRuntimeIdentityV1) -> CompleteAgentPackageV1 {
    let mut value = learned_opening_fresh_package(runtime);
    value.gameplay.identity.schema = "mtg-kernel-expanded-deck-inference/v1".into();
    value.gameplay.identity.source_import = FrozenPlayPolicyIdentityV1 {
        schema: "fixture-ancestry".into(),
        source_export_schema: "fixture".into(),
        source_metadata_sha256: digest('1'),
        weights_sha256: digest('9'),
        model_parameter_sha256: digest('9'),
        source_run_sha256: digest('1'),
        source_generation: 1,
        source_git_commit: "a".repeat(40),
        source_card_db_hash: value.runtime.card_db_hash.clone(),
        destination_card_db_hash: value.runtime.card_db_hash.clone(),
        source_registry_sha256: digest('1'),
        destination_registry_sha256: value.runtime.card_registry_sha256.clone(),
        source_card_count: 184,
        destination_card_count: 184,
        source_training_deck_ids: vec!["fixture".into()],
        namespace_rule: "fixture".into(),
        appended_rows: "fixture".into(),
        feature_contract_digest: value.runtime.feature_contract_digest.clone(),
        feature_encoding_digest: value.runtime.feature_encoding_digest.clone(),
        sampler_identity: WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into(),
        reader_revalidated_store_chain: false,
        observation_successor: None,
    }
    .into();
    value
}

#[test]
fn imported_gameplay_identity_rejects_learned_opening_even_on_a_genuine_v4_runtime() {
    if env!("MTG_KERNEL_BUILD_GIT_CLEAN") != "true" {
        return;
    }
    let value = imported_learned_opening_package(v4_runtime_fixture());
    value.validate_metadata_v1().unwrap();
    assert!(!value.gameplay.identity.source_import.is_fresh_v1());
    // Proves the origin gate holds on its own: the runtime genuinely
    // verifies as V4, not just as some unverifiable placeholder.
    assert_eq!(
        value.runtime.verify_current_runtime_v1().unwrap().generation_v1(),
        RuntimeContractGenerationV1::V4
    );
    match value.load_supported_components_v1() {
        Ok(_) => panic!(
            "an imported (frozen) gameplay identity must not admit a learned opening, \
             even on a genuine V4 runtime"
        ),
        Err(error) => assert!(error.contains("imported (frozen) gameplay identity")),
    }
}

/// `learned_opening_fresh_package` with the opening reverted to the ordinary
/// `Existing{KeepSevenV2}` path and `play_draw` switched to `LearnedV1`,
/// exercising the play-draw half of the gate directly.
fn learned_play_draw_fresh_package(runtime: AgentRuntimeIdentityV1) -> CompleteAgentPackageV1 {
    let mut value = learned_opening_fresh_package(runtime);
    value.opening = AgentOpeningPolicyV1::Existing {
        protocol: Bo3OpeningProtocolV1::KeepSevenV2,
    };
    value.play_draw = AgentPlayDrawPolicyV1::LearnedV1 {
        policy: AuxiliaryPolicyBindingV1 {
            checkpoint: pin("fixture-learned-play-draw"),
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
fn fresh_identity_with_v4_runtime_admits_learned_play_draw_past_the_gate() {
    if env!("MTG_KERNEL_BUILD_GIT_CLEAN") != "true" {
        return;
    }
    let value = learned_play_draw_fresh_package(v4_runtime_fixture());
    value.validate_metadata_v1().unwrap();
    assert!(value.gameplay.identity.source_import.is_fresh_v1());
    assert_eq!(
        value.runtime.verify_current_runtime_v1().unwrap().generation_v1(),
        RuntimeContractGenerationV1::V4
    );
    match value.load_supported_components_v1() {
        Ok(_) => panic!(
            "the play_import fixture is deliberately not a loadable checkpoint; \
             full success is not expected here"
        ),
        Err(error) => {
            assert!(!error.contains("learned opening/play-draw"));
            assert!(!error.contains("not implemented by this interface"));
        }
    }
}

/// `imported_learned_opening_package` with the opening reverted to
/// `Existing{KeepSevenV2}` and `play_draw` switched to `LearnedV1`, proving
/// the same structural gate rejects an imported origin for play-draw too,
/// even on a genuine V4 runtime.
fn imported_learned_play_draw_package(runtime: AgentRuntimeIdentityV1) -> CompleteAgentPackageV1 {
    let mut value = imported_learned_opening_package(runtime);
    value.opening = AgentOpeningPolicyV1::Existing {
        protocol: Bo3OpeningProtocolV1::KeepSevenV2,
    };
    value.play_draw = AgentPlayDrawPolicyV1::LearnedV1 {
        policy: AuxiliaryPolicyBindingV1 {
            checkpoint: pin("fixture-learned-play-draw"),
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
fn imported_gameplay_identity_rejects_learned_play_draw_even_on_a_genuine_v4_runtime() {
    if env!("MTG_KERNEL_BUILD_GIT_CLEAN") != "true" {
        return;
    }
    let value = imported_learned_play_draw_package(v4_runtime_fixture());
    value.validate_metadata_v1().unwrap();
    assert!(!value.gameplay.identity.source_import.is_fresh_v1());
    assert_eq!(
        value.runtime.verify_current_runtime_v1().unwrap().generation_v1(),
        RuntimeContractGenerationV1::V4
    );
    match value.load_supported_components_v1() {
        Ok(_) => panic!(
            "an imported (frozen) gameplay identity must not admit a learned play-draw \
             policy, even on a genuine V4 runtime"
        ),
        Err(error) => assert!(error.contains("imported (frozen) gameplay identity")),
    }
}
