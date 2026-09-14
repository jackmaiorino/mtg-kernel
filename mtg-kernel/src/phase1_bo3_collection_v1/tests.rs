use super::*;
use crate::bo3_match::MatchOutcomeV1;
use crate::expanded_deck_training_v1::{
    ExpandedInferenceIdentityV1, ExpandedModelSourceV1, ExpandedSeatBehaviorV1, PinnedFileV1,
};
use crate::learned_bo3_v1::{
    run_learned_bo3_with_registrations_v1, LearnedBo3RunConfigV1, VisibleSideboardPolicyV1,
};
use crate::learned_sideboard_v1::{LearnedSideboardInputV1, SideboardPlayIdentityV1};
use crate::native_flat_tensorizer_v3::*;
use crate::sideboard_play_policy_v1::FrozenPlayObservationTransferV3;

fn pin(label: &str) -> PinnedFileV1 {
    PinnedFileV1 {
        path: label.into(),
        sha256: "a".repeat(64),
    }
}

/// Exact actual native fixture model/ancestry with explicitly test-only file
/// metadata. This is not a current-runtime certificate or a learned-strength run.
fn package(policy: &FrozenPlayPolicyV1, choice: PlayDrawChoiceV1) -> CompleteAgentPackageV1 {
    let model = policy.actual_model_identity_v1();
    let runtime = AgentRuntimeIdentityV1 {
        executable: pin("test-only-missing-executable"),
        toolchain: pin("test-only-toolchain"),
        engine_commit: "a".repeat(40),
        tracked_tree_sha256: "b".repeat(64),
        tracked_tree_contract: "test-only-tree".into(),
        build_git_clean: true,
        card_db_hash: model.card_db_hash.clone(),
        card_registry_sha256: policy
            .identity_v1()
            .destination_registry_sha256_v1()
            .to_owned(),
        feature_contract_digest: model.feature_contract_digest.clone(),
        feature_encoding_digest: model.feature_encoding_digest.clone(),
        features_source_sha256: FEATURES_SOURCE_SHA256_V3.into(),
        feature_descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V3.into(),
    };
    let result = CompleteAgentPackageV1 {
        schema: COMPLETE_AGENT_PACKAGE_SCHEMA_V1.into(),
        runtime,
        gameplay: ExpandedSeatBehaviorV1 {
            source: ExpandedModelSourceV1 {
                play_import: pin("test-only-import"),
                checkpoint: None,
                feature_transfer: FrozenPlayObservationTransferV3 {
                    expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
                    expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
                },
            },
            identity: ExpandedInferenceIdentityV1 {
                schema: "mtg-kernel-expanded-deck-inference/v1".into(),
                source_import: policy.identity_v1().clone(),
                checkpoint_sha256: None,
                model,
                state_sha256: "c".repeat(64),
                adam_step: 0,
                feature_schema_version: FEATURE_SCHEMA_VERSION_V3.into(),
                feature_registry_version: FEATURE_REGISTRY_VERSION_V3.into(),
                features_source_sha256: FEATURES_SOURCE_SHA256_V3.into(),
                feature_descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V3.into(),
            },
        },
        gameplay_sampler_identity: policy.runtime_sampler_identity_v1().into(),
        opening: AgentOpeningPolicyV1::Existing {
            protocol: Bo3OpeningProtocolV1::KeepSevenV2,
        },
        play_draw: AgentPlayDrawPolicyV1::Fixed { choice },
        sideboard: AgentSideboardPolicyV1::Keep,
        search: AgentSearchPolicyV1::Disabled,
    };
    result.validate_metadata_v1().unwrap();
    result
}

pub(crate) fn config(id: &str) -> Bo3CollectionConfigV1 {
    // A legal all-basic registration loses by actual engine decking. There is
    // no fabricated terminal, reward, forced winner or short-library fixture.
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let island = crate::card_def::card_id_by_name("Island").unwrap();
    Bo3CollectionConfigV1 {
        schema: BO3_COLLECTION_CONFIG_SCHEMA_V1.into(),
        match_id: id.into(),
        seed: 91831,
        initial_chooser: PlayerSeatV1::P0,
        deck_ids: ["fixture-forest".into(), "fixture-island".into()],
        registrations: [forest, island].map(|card| OwnDeckConfigurationV1 {
            mainboard: vec![card; 60],
            sideboard: vec![card; 15],
        }),
        summary_tags: Bo3SummaryTagsV1 {
            requires_target: BTreeSet::new(),
            is_counterspell: BTreeSet::new(),
        },
        max_physical_games: 3,
        max_physical_decisions: 20_000,
        max_policy_steps: 20_000,
        max_decision_records: MAX_RECORDS,
        max_decision_json_bytes: MAX_RECORD_BYTES,
    }
}

pub(crate) fn fixtures(
    choices: [PlayDrawChoiceV1; 2],
) -> ([FrozenPlayPolicyV1; 2], [CompleteAgentPackageV1; 2]) {
    let policies = [compact_board_policy(), compact_board_policy()];
    let packages = [0, 1].map(|i| package(&policies[i], choices[i]));
    (policies, packages)
}

/// A real, explicitly untrained Net8 fixture that strongly prefers the legal
/// Pass action. The generic fixed net otherwise spends thousands of decisions
/// activating a growing board of basic lands, exhausting the recorder bound.
/// No action, RNG draw, game state, library size or terminal is overridden.
/// The existing Hamilton sampler retains its actual clamped non-Pass mass.
fn compact_board_policy() -> FrozenPlayPolicyV1 {
    use crate::native_policy_value_net_v1::HIDDEN_DIM_V1;
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let mut parameters = policy.training_parameters_v3();
    for parameter in &mut parameters {
        parameter.values.fill(0.0);
    }
    // V3 action features retain FlatScorerActionKindV2's one-hot prefix;
    // Pass = 0. Linear weights are [output,input] row-major. The scorer
    // concatenates [state_hidden, action_hidden], each HIDDEN_DIM_V1 wide.
    for (name, input, value) in [
        ("action_encoder.0.weight", 0, 4.0),
        ("action_encoder.2.weight", 0, 4.0),
        ("scorer.0.weight", HIDDEN_DIM_V1, 4.0),
        ("scorer.2.weight", 0, 16.0),
    ] {
        let parameter = parameters.iter_mut().find(|p| p.name == name).unwrap();
        assert_eq!(parameter.shape.len(), 2);
        assert!(input < parameter.shape[1]);
        parameter.values[input] = value;
    }
    policy.replace_training_parameters_v3(&parameters).unwrap();
    assert_compact_policy_scores(&mut policy);
    policy
}

/// Inspect a real legal Pass/PlayLand menu before spending time on a full
/// fixture match. This catches a wrong feature/routing index immediately.
fn assert_compact_policy_scores(policy: &mut FrozenPlayPolicyV1) {
    use crate::rl::ActionSemanticV1;
    let cfg = config("compact-score-sanity");
    let mut opening = HumanOpeningV1::new(
        1,
        117,
        100,
        100,
        cfg.deck_ids,
        cfg.registrations.each_ref().map(|c| c.mainboard.clone()),
        PlayerId::P0,
        PlayerId::P0,
    )
    .unwrap();
    opening.keep().unwrap();
    let mut session = opening.into_session().unwrap();
    policy
        .reset_for_game_v1(paired_policy_seeds_v1(117))
        .unwrap();
    assert!(
        policy.embedding_rows_v1()[..crate::native_policy_value_net_v1::CARD_EMBEDDING_DIM_V1]
            .iter()
            .all(|v| v.to_bits() == 0)
    );
    for _ in 0..100 {
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("sanity menu did not arrive before terminal")
        };
        let input = PairedBo1PolicyInputV1::new(&session, decision);
        let (selected, scores) = policy.select_paired_with_scores_v1(&input).unwrap();
        let behavior =
            BehaviorDistributionV1::hamilton_from_logits_v1(&scores.logits, selected).unwrap();
        let record = input
            .capture_bo3_gameplay_v1(0, "a".repeat(64), behavior)
            .unwrap();
        let ActorVisibleDecisionV1::Gameplay {
            ordered_actions, ..
        } = &record.visible
        else {
            unreachable!()
        };
        if ordered_actions
            .iter()
            .any(|a| matches!(a, ActionSemanticV1::PlayLand { .. }))
        {
            let pass = ordered_actions
                .iter()
                .position(|a| matches!(a, ActionSemanticV1::Pass { .. }))
                .expect("main phase must offer Pass");
            assert_eq!(
                selected as usize, pass,
                "fixture sampler must choose the favored real Pass on this fixed sanity seed"
            );
            assert!(
                scores.logits[pass].is_finite() && (15.0..=16.0).contains(&scores.logits[pass])
            );
            assert!(scores
                .logits
                .iter()
                .enumerate()
                .all(|(i, x)| i == pass || *x == 0.0));
            let probability = record
                .behavior
                .selected_probability_v1(ordered_actions.len())
                .unwrap();
            assert!(
                probability > 0.9999 && probability < 1.0,
                "real Hamilton distribution must retain non-Pass mass"
            );
            return;
        }
        session
            .step(decision.episode_id, decision.step, selected)
            .unwrap();
    }
    panic!("no real Pass/PlayLand menu within the bounded sanity check");
}
fn collect_fixture(
    config: &Bo3CollectionConfigV1,
    choices: [PlayDrawChoiceV1; 2],
) -> (Bo3CollectedMatchV1, [CompleteAgentPackageV1; 2]) {
    let (mut policies, packages) = fixtures(choices);
    let result = collect_loaded(config, packages.each_ref(), &mut policies, [None, None]).unwrap();
    (result, packages)
}

fn gameplay_trace(record: &Bo3DecisionRecordV1) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(&record.visible, record.behavior.selected_index_v1())).unwrap()
        )
    )
}

struct LegacyTrace {
    policies: [FrozenPlayPolicyV1; 2],
    hashes: [String; 2],
    traces: Vec<Vec<String>>,
}
impl PairedBo1PolicyV1 for LegacyTrace {
    fn uses_observation_successor_v3(&self) -> bool {
        true
    }
    fn reset_for_game_v1(&mut self, seeds: [u64; 2]) -> Result<(), RlSessionError> {
        self.traces.push(Vec::new());
        for policy in &mut self.policies {
            policy.reset_for_game_v1(seeds)?;
        }
        Ok(())
    }
    fn select_action_v1(
        &mut self,
        input: PairedBo1PolicyInputV1<'_>,
    ) -> Result<u32, RlSessionError> {
        let i = seat(input.decision().acting_player);
        let mut captured = input
            .capture_bo3_gameplay_v1(
                0,
                self.hashes[i].clone(),
                BehaviorDistributionV1::Deterministic { selected_index: 0 },
            )
            .unwrap();
        let selected = self.policies[i].select_action_v1(input)?;
        captured.behavior = BehaviorDistributionV1::Deterministic {
            selected_index: selected,
        };
        self.traces
            .last_mut()
            .unwrap()
            .push(gameplay_trace(&captured));
        Ok(selected)
    }
}

#[test]
fn actual_natural_two_and_three_game_returns_replay_legacy_parity_and_equal_match_weights() {
    use PlayDrawChoiceV1::{Draw, Play};
    let cfg = config("actual-three");
    let (three, packages) = collect_fixture(&cfg, [Play, Play]);
    assert_eq!(three.trajectory.games.len(), 3, "{:?}", three.games);
    assert!(matches!(
        three.trajectory.ending,
        Bo3TrajectoryEndingV1::Complete {
            outcome: MatchOutcomeV1::Winner {
                winner: PlayerId::P0
            }
        }
    ));
    let (replay, _) = collect_fixture(&cfg, [Play, Play]);
    assert_eq!(
        serde_json::to_vec(&three).unwrap(),
        serde_json::to_vec(&replay).unwrap()
    );
    assert!(three
        .games
        .iter()
        .all(|g| g.error.is_none() && g.discarded_pending_selections == 0));
    assert!(three
        .trajectory
        .games
        .iter()
        .all(|g| g.terminal.as_ref().unwrap().classification == TerminalClassificationV1::Natural));
    let new_traces: Vec<Vec<_>> = three
        .trajectory
        .games
        .iter()
        .map(|g| {
            g.decisions
                .iter()
                .filter(|d| d.visible.component_v1() == LearningComponentV1::Gameplay)
                .map(gameplay_trace)
                .collect()
        })
        .collect();
    let (policies, _) = fixtures([Play, Play]);
    let mut legacy = LegacyTrace {
        policies,
        hashes: three.trajectory.behavior_packages_by_seat.clone(),
        traces: Vec::new(),
    };
    let keep = |_: &LearnedSideboardInputV1, current: &DeckConfigurationV1| {
        Ok((current.clone(), vec![SideboardActionV1::Done]))
    };
    let mut keep0 = keep;
    let mut keep1 = keep;
    let policies: [&mut dyn VisibleSideboardPolicyV1; 2] = [&mut keep0, &mut keep1];
    let old = run_learned_bo3_with_registrations_v1(
        LearnedBo3RunConfigV1 {
            deck_ids: cfg.deck_ids.clone(),
            seed: cfg.seed,
            game_one_chooser: player(cfg.initial_chooser),
            max_physical_games: cfg.max_physical_games,
            max_physical_decisions: cfg.max_physical_decisions,
            max_policy_steps: cfg.max_policy_steps,
            opening_protocol: Bo3OpeningProtocolV1::KeepSevenV2,
        },
        registrations(&cfg).unwrap(),
        &packages[0].gameplay.identity.model.weights_sha256,
        "keep-untrained",
        &RemovalCounterspellTagsV1 {
            requires_target: BTreeSet::new(),
            is_counterspell: BTreeSet::new(),
        },
        &mut legacy,
        policies,
    )
    .unwrap();
    assert_eq!(legacy.traces, new_traces);
    assert_eq!(old.games.len(), 3);
    for (i, original) in old.games.iter().enumerate() {
        assert_eq!(original.start, three.trajectory.games[i].start.unwrap());
        assert_eq!(
            Some(original.environment_seed),
            three.games[i].environment_seed
        );
        assert_eq!(
            original.winner.map(PlayerSeatV1::from),
            three.games[i].observed_terminal.as_ref().unwrap().winner
        );
    }
    // Fixed play for P0, fixed draw for P1 produces a genuine 2-0 through
    // the same legal registrations and engine; P0 remains the same package.
    let (two, two_packages) = collect_fixture(&config("actual-two"), [Play, Draw]);
    assert_eq!(two.trajectory.games.len(), 2, "{:?}", two.games);
    assert!(matches!(
        two.trajectory.ending,
        Bo3TrajectoryEndingV1::Complete { .. }
    ));
    let mut capped_config = config("actual-capped");
    capped_config.max_policy_steps = 1;
    let (capped, _) = collect_fixture(&capped_config, [Play, Play]);
    assert_eq!(
        capped.trajectory.ending,
        Bo3TrajectoryEndingV1::Incomplete {
            reason: IncompleteMatchReasonV1::DecisionCap
        }
    );
    assert!(capped.trajectory.games[0].terminal.is_none());
    assert_eq!(capped.games[0].discarded_pending_selections, 1);
    assert_eq!(
        capped.games[0]
            .observed_terminal
            .as_ref()
            .unwrap()
            .policy_step_count,
        1
    );
    assert_eq!(
        capped.games[0]
            .observed_terminal
            .as_ref()
            .unwrap()
            .terminal_classification,
        TerminalClassificationV1::Truncated
    );
    let validated = [
        three.trajectory.validate_v1(packages.each_ref()).unwrap(),
        two.trajectory.validate_v1(two_packages.each_ref()).unwrap(),
        capped.trajectory.validate_v1(packages.each_ref()).unwrap(),
    ];
    assert_eq!(validated[0].match_return_v1(PlayerSeatV1::P0), Some(1.0));
    assert_eq!(validated[1].match_return_v1(PlayerSeatV1::P1), Some(-1.0));
    assert_eq!(validated[2].match_return_v1(PlayerSeatV1::P0), None);
    let weights = equal_match_weights_v1(
        &validated,
        PlayerSeatV1::P0,
        LearningComponentV1::Gameplay,
        &three.trajectory.behavior_packages_by_seat[0],
    )
    .unwrap();
    let mut totals = std::collections::BTreeMap::<&str, f64>::new();
    for weight in &weights {
        *totals.entry(&weight.match_id).or_default() += weight.weight;
    }
    assert_eq!(totals.len(), 2);
    assert!(totals.values().all(|weight| (*weight - 0.5).abs() < 1e-10));
    // Each opener records only its own real land hand, with the actual keep
    // ordering of KeepSevenV2; every gameplay distribution is exact Q64.
    for game in &three.trajectory.games {
        let opening: Vec<_> = game
            .decisions
            .iter()
            .filter(|d| matches!(d.visible, ActorVisibleDecisionV1::Mulligan { .. }))
            .collect();
        assert_eq!(
            opening.iter().map(|d| d.actor).collect::<Vec<_>>(),
            [PlayerSeatV1::P1, PlayerSeatV1::P0]
        );
        for record in &game.decisions {
            assert_eq!(
                record.behavior_package_sha256,
                three.trajectory.behavior_packages_by_seat[seat(record.actor)]
            );
            match &record.visible {
                ActorVisibleDecisionV1::Mulligan { input, .. } => {
                    assert_eq!(
                        input.own_hand,
                        vec![cfg.registrations[seat(record.actor)].mainboard[0]; 7]
                    );
                    assert_eq!(input.opponent_has_kept, record.actor == PlayerSeatV1::P0);
                }
                ActorVisibleDecisionV1::Gameplay {
                    ordered_actions, ..
                } => {
                    assert!(matches!(
                        record.behavior,
                        BehaviorDistributionV1::HamiltonQ64 { .. }
                    ));
                    assert!(
                        record
                            .behavior
                            .selected_probability_v1(ordered_actions.len())
                            .unwrap()
                            > 0.0
                    );
                }
                _ => {}
            }
        }
    }
}

#[test]
fn actual_engine_error_does_not_commit_pending_selection_or_infer_commit_from_counter() {
    let (mut policies, packages) = fixtures([PlayDrawChoiceV1::Play; 2]);
    let cfg = config("actual-error");
    let hashes = packages.each_ref().map(|p| p.package_sha256_v1().unwrap());
    for advance_first in [false, true] {
        let mut opening = HumanOpeningV1::new(
            1,
            908,
            1000,
            1000,
            cfg.deck_ids.clone(),
            cfg.registrations.each_ref().map(|c| c.mainboard.clone()),
            PlayerId::P0,
            PlayerId::P0,
        )
        .unwrap();
        opening.keep().unwrap();
        let mut session = opening.into_session().unwrap();
        let mut game = Bo3TrainingGameV1 {
            game_index: 1,
            start: None,
            decisions: Vec::new(),
            terminal: None,
        };
        let mut budget = RecordBudget {
            count: 0,
            bytes: 0,
            max_count: 1000,
            max_bytes: MAX_RECORD_BYTES,
        };
        let mut diagnostic = Bo3CollectionGameDiagnosticsV1 {
            game_index: 1,
            environment_seed: Some(908),
            observed_terminal: None,
            discarded_pending_selections: 0,
            error: None,
        };
        let mut recorder = RecordingPolicy {
            capture: None,
            policies: &mut policies,
            hashes: &hashes,
            game: &mut game,
            budget: &mut budget,
            pending: None,
            recording_cap: false,
            rejected_selections: 0,
        };
        recorder
            .reset_for_game_v1(paired_policy_seeds_v1(908))
            .unwrap();
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("real initial decision required")
        };
        let selected = recorder
            .select_action_v1(PairedBo1PolicyInputV1::new(&session, decision))
            .unwrap();
        if advance_first {
            session
                .step(decision.episode_id, decision.step, selected)
                .unwrap();
        }
        // Real invalid/stale engine step, with no fake terminal or winner.
        let error = session
            .step(decision.episode_id, decision.step, u32::MAX)
            .unwrap_err();
        assert_eq!(session.policy_step_count(), u64::from(advance_first));
        assert!(recorder
            .finish_game(Err(error.to_string()), &mut diagnostic)
            .is_err());
        assert_eq!(diagnostic.discarded_pending_selections, 1);
        assert!(recorder.game.decisions.is_empty());
        assert!(recorder.game.terminal.is_none());
        assert_eq!(recorder.budget.count, 0);
    }
}

#[test]
fn bounded_recording_preserves_valid_prefix_and_public_entry_rejects_unverified_packages() {
    let mut cfg = config("bounded");
    cfg.max_decision_records = 4;
    let (result, packages) = collect_fixture(&cfg, [PlayDrawChoiceV1::Play; 2]);
    assert_eq!(result.committed_decision_records, 4);
    assert_eq!(
        result.trajectory.ending,
        Bo3TrajectoryEndingV1::Incomplete {
            reason: IncompleteMatchReasonV1::DecisionCap
        }
    );
    assert!(result
        .trajectory
        .validate_v1(packages.each_ref())
        .unwrap()
        .match_return_v1(PlayerSeatV1::P0)
        .is_none());
    assert_eq!(result.games[0].discarded_pending_selections, 1);
    assert!(collect_bo3_trajectory_v1(cfg.clone(), packages.clone()).is_err());
    let mut changed = packages.clone();
    changed[0].gameplay.identity.model.weights_sha256 = "d".repeat(64);
    let (mut policies, _) = fixtures([PlayDrawChoiceV1::Play; 2]);
    assert!(
        collect_loaded(&cfg, changed.each_ref(), &mut policies, [None, None])
            .unwrap_err()
            .contains("installed BO3 gameplay")
    );
    cfg.max_decision_records = 100;
    cfg.max_decision_json_bytes = 1;
    let (short, _) = collect_fixture(&cfg, [PlayDrawChoiceV1::Play; 2]);
    assert_eq!(short.committed_decision_records, 0);
    assert_eq!(short.committed_decision_json_bytes, 0);
    assert!(short.trajectory.games[0].start.is_none());
}

#[test]
fn actual_greedy_sideboard_runs_between_natural_games_with_matching_visible_trace() {
    let mut cfg = config("actual-head");
    cfg.registrations[0].sideboard = cfg.registrations[1].sideboard.clone();
    let (mut policies, mut packages) = fixtures([PlayDrawChoiceV1::Play; 2]);
    let identity = SideboardPlayIdentityV1 {
        weights_sha256: packages[0].gameplay.identity.model.weights_sha256.clone(),
        git_head: "a".repeat(40),
    };
    let embeddings =
        FrozenSideboardEmbeddingsV1::new_v1(policies[0].embedding_rows_v1(), identity.clone())
            .unwrap();
    // An actual head with zero policy scores deterministically takes the first
    // legal action on ties. This guarantees real 15-card exchanges in games 2
    // and 3, exercising intermediate state and carry-over. It is an untrained
    // engineering fixture, not a fit or a playing-strength result.
    let initialized = LearnedSideboardModelV1::new_v1(119, &embeddings);
    let mut head_json = serde_json::to_value(&initialized).unwrap();
    for weight in head_json["policy_weights"].as_array_mut().unwrap() {
        *weight = serde_json::json!(0.0);
    }
    let head =
        LearnedSideboardModelV1::from_json_v1(&serde_json::to_string(&head_json).unwrap()).unwrap();
    packages[0].sideboard = AgentSideboardPolicyV1::LearnedGreedyV1 {
        checkpoint: PinnedFileV1 {
            path: "test-only-sideboard".into(),
            sha256: head.checkpoint_sha256_v1().unwrap(),
        },
        play_identity: identity,
        embedding_table_sha256: embeddings.table_sha256_v1().into(),
    };
    let result = collect_loaded(
        &cfg,
        packages.each_ref(),
        &mut policies,
        [Some(&head), None],
    )
    .unwrap();
    assert!(
        result
            .trajectory
            .validate_v1(packages.each_ref())
            .unwrap()
            .is_complete_v1(),
        "{:?}",
        result.games
    );
    let embeddings = FrozenSideboardEmbeddingsV1::new_v1(
        policies[0].embedding_rows_v1(),
        head.play_identity_v1().clone(),
    )
    .unwrap();
    assert_eq!(result.trajectory.games.len(), 3);
    let initial = &result.trajectory.registrations_by_seat[0];
    let mut current =
        DeckConfigurationV1::new_exact_v1(initial.mainboard.clone(), initial.sideboard.clone())
            .unwrap();
    for game in result.trajectory.games.iter().skip(1) {
        let records: Vec<_> = game
            .decisions
            .iter()
            .filter(|r| {
                r.actor == PlayerSeatV1::P0
                    && r.visible.component_v1() == LearningComponentV1::Sideboard
            })
            .collect();
        assert!(!records.is_empty());
        let ActorVisibleDecisionV1::Sideboard { input, .. } = &records[0].visible else {
            unreachable!()
        };
        assert_eq!(input.resource_summaries.len(), game.game_index as usize - 1);
        let expected = head.deliberate_v1(input, &current, &embeddings).unwrap();
        assert_eq!(records.len(), 31);
        let mut reconstructed = SideboardDeliberationStateV1::new_v1(&current);
        for record in &records {
            let ActorVisibleDecisionV1::Sideboard {
                input,
                ordered_actions,
            } = &record.visible
            else {
                unreachable!()
            };
            assert_eq!(*ordered_actions, reconstructed.legal_actions_v1());
            let scored = head.score_v1(input, &reconstructed, &embeddings).unwrap();
            assert_eq!(scored.ordered_actions, *ordered_actions);
            let selected = ordered_actions[record.behavior.selected_index_v1()];
            assert_eq!(selected, scored.selected_action);
            assert_eq!(
                record
                    .behavior
                    .selected_probability_v1(ordered_actions.len())
                    .unwrap(),
                1.0
            );
            reconstructed.apply_v1(selected).unwrap();
        }
        assert!(reconstructed.is_done_v1());
        let actions: Vec<_> = records
            .iter()
            .map(|r| match &r.visible {
                ActorVisibleDecisionV1::Sideboard {
                    ordered_actions, ..
                } => ordered_actions[r.behavior.selected_index_v1()],
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(actions, expected.actions);
        assert_eq!(
            reconstructed.configuration_v1().unwrap(),
            expected.configuration
        );
        assert!(records
            .iter()
            .all(|r| matches!(r.behavior, BehaviorDistributionV1::Deterministic { .. })));
        current = expected.configuration;
    }
}

#[test]
fn bounded_public_request_parser_roundtrips_and_rejects_nested_duplicates() {
    let policy = FrozenPlayPolicyV1::training_fixture_v3();
    let agent = package(&policy, PlayDrawChoiceV1::Play);
    let request = Bo3CollectionRequestV1 {
        config: config("request-parser"),
        packages: [agent.clone(), agent],
    };
    let text = serde_json::to_string(&request).unwrap();
    assert_eq!(
        Bo3CollectionRequestV1::from_json_v1(&text).unwrap(),
        request
    );
    let duplicate = text.replacen(
        "\"build_git_clean\":true",
        "\"build_git_clean\":true,\"build_git_clean\":true",
        1,
    );
    assert_ne!(duplicate, text);
    assert!(Bo3CollectionRequestV1::from_json_v1(&duplicate)
        .unwrap_err()
        .contains("duplicate JSON object key"));
    let oversized = " ".repeat(MAX_BO3_COLLECTION_REQUEST_BYTES_V1 + 1);
    assert!(Bo3CollectionRequestV1::from_json_v1(&oversized)
        .unwrap_err()
        .contains("exceeds 4 MiB"));
    let mut unknown = serde_json::to_value(&request).unwrap();
    unknown["config"]["unrecognized"] = serde_json::json!(true);
    assert!(
        Bo3CollectionRequestV1::from_json_v1(&serde_json::to_string(&unknown).unwrap()).is_err()
    );
}
