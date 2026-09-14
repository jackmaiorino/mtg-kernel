//! Engine checks with explicit untrained gameplay and synthetic head-fit data.
//! Private fixture construction is not a successful current-runtime certificate.
use super::*;
use crate::game_summary_v1::{GameConcessionStageV2, GameSummaryCompletionV2};
use crate::learned_sideboard_v1::{
    LearnedSideboardInputV1, SideboardActionV1, SideboardImitationExampleV1,
    SideboardPlayIdentityV1, SideboardTrainingConfigV1,
};
use crate::phase1_agent_v1::ActorVisibleDecisionV1;
use crate::phase1_bo3_collection_v1::{Bo3CollectionConfigV1, collect_loaded_inner};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn directory() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "human-v2-fixture-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    path
}

fn fixture(
    human: usize,
    fitted: bool,
) -> (
    HumanMatchServiceV2,
    Bo3CollectionConfigV1,
    [CompleteAgentPackageV1; 2],
) {
    let directory = directory();
    let mut cfg = crate::phase1_bo3_collection_v1::tests::config("human-v2-fixed-engine-fixture");
    // Two distinct basic identities allow real legal sideboard exchanges.
    cfg.registrations[0].sideboard = cfg.registrations[1].mainboard[..15].to_vec();
    cfg.registrations[1].sideboard = cfg.registrations[0].mainboard[..15].to_vec();
    let (policies, mut packages) = crate::phase1_bo3_collection_v1::tests::fixtures([
        PlayDrawChoiceV1::Draw,
        PlayDrawChoiceV1::Play,
    ]);
    let [p0, p1] = policies;
    let policy = if human == 0 { p1 } else { p0 };
    let model = 1 - human;
    let head = if fitted {
        let identity = SideboardPlayIdentityV1 {
            weights_sha256: policy.actual_model_identity_v1().weights_sha256,
            git_head: "a".repeat(40),
        };
        let embeddings =
            FrozenSideboardEmbeddingsV1::new_v1(policy.embedding_rows_v1(), identity.clone())
                .unwrap();
        let mut head = LearnedSideboardModelV1::new_v1(91, &embeddings);
        let deck = DeckConfigurationV1::new_exact_v1(
            cfg.registrations[model].mainboard.clone(),
            cfg.registrations[model].sideboard.clone(),
        )
        .unwrap();
        let input = LearnedSideboardInputV1 {
            registered_cards: deck.combined_card_counts_v1(),
            own_card_outcomes: vec![],
            opponent_evidence: vec![],
            resource_summaries: vec![],
            next_game_number: 2,
            acting_player_games_won: 0,
            opponent_games_won: 1,
        };
        // One actual fit step on synthetic Keep teaching data, no fabricated value label.
        head.train_imitation_v1(
            &[SideboardImitationExampleV1 {
                input,
                initial_mainboard: deck.mainboard().to_vec(),
                initial_sideboard: deck.sideboard().to_vec(),
                target_actions: vec![SideboardActionV1::Done],
                target_value: None,
            }],
            &embeddings,
            SideboardTrainingConfigV1 {
                epochs: 1,
                learning_rate: 0.01,
                value_loss_weight: 0.0,
            },
        )
        .unwrap();
        assert!(head.training_steps_v1() > 0);
        let text = head.to_json_v1().unwrap();
        let path = directory.join("synthetic-fitted-head.json");
        std::fs::write(&path, text.as_bytes()).unwrap();
        packages[model].sideboard = AgentSideboardPolicyV1::LearnedGreedyV1 {
            checkpoint: PinnedFileV1 {
                path,
                sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
            },
            play_identity: identity,
            embedding_table_sha256: embeddings.table_sha256_v1().to_owned(),
        };
        Some(head)
    } else {
        None
    };
    let config = HumanMatchConfigV2 {
        schema: HUMAN_MATCH_CONFIG_SCHEMA_V2.into(),
        package: PinnedFileV1 {
            path: directory.join("test-only-package-not-loaded.json"),
            sha256: "a".repeat(64),
        },
        registered: [0, 1].map(|i| ExpandedDeckListV1 {
            label: cfg.deck_ids[i].clone(),
            mainboard: cfg.registrations[i].mainboard.clone(),
            sideboard: cfg.registrations[i].sideboard.clone(),
        }),
        human_seat: human as u8,
        initial_chooser: 0,
        seed: cfg.seed,
        summary_tags: cfg.summary_tags.clone(),
        journal_path: directory.join("session.jsonl"),
        max_physical_games: cfg.max_physical_games,
        max_physical_decisions: cfg.max_physical_decisions,
        max_policy_steps: cfg.max_policy_steps,
    };
    let service = HumanMatchServiceV2::construct(
        config,
        packages[model].clone(),
        policy,
        head,
        json!({"test_only_actual_engine_fixture":true,"runtime_verified":false}),
    )
    .unwrap();
    (service, cfg, packages)
}

fn current(service: &mut HumanMatchServiceV2) -> Value {
    let response = service.handle(HumanMatchCommandV1::Current {
        request_id: "read".into(),
    });
    assert_eq!(response["ok"], true, "{response}");
    response["view"].clone()
}
fn send(service: &mut HumanMatchServiceV2, command: HumanMatchCommandV1) -> Value {
    let response = service.handle(command);
    assert_eq!(response["ok"], true, "{response}");
    response
}
fn begin(service: &mut HumanMatchServiceV2) {
    if current(service)["phase"] == "play_draw" {
        send(
            service,
            HumanMatchCommandV1::PlayDraw {
                request_id: "initial-choice".into(),
                game_index: 1,
                choice: PlayDrawChoiceV1::Draw,
            },
        );
    }
}
fn keep(service: &mut HumanMatchServiceV2, id: &str) {
    let game_index = service.game_index;
    let opening_revision = service.opening_revision;
    send(
        service,
        HumanMatchCommandV1::Keep {
            request_id: id.into(),
            game_index,
            opening_revision,
        },
    );
}
fn journal(service: &HumanMatchServiceV2) -> Vec<Value> {
    std::fs::read_to_string(&service.config.journal_path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn human_v2_both_seats_match_automated_package_gameplay_opening_and_fitted_sideboard() {
    for human in 0..2 {
        let (mut service, cfg, packages) = fixture(human, human == 1);
        let (mut policies, _) = crate::phase1_bo3_collection_v1::tests::fixtures([
            PlayDrawChoiceV1::Draw,
            PlayDrawChoiceV1::Play,
        ]);
        let owned_head = service.sideboard.clone();
        let mut heads = [None, None];
        heads[1 - human] = owned_head.as_ref();
        let automated =
            collect_loaded_inner(&cfg, packages.each_ref(), &mut policies, heads, None).unwrap();
        let expected: Vec<_> = automated
            .trajectory
            .games
            .iter()
            .flat_map(|g| {
                g.decisions.iter().filter_map(move |d| {
                    if d.actor == PlayerSeatV1::from(PlayerId(human as u8))
                        && matches!(d.visible, ActorVisibleDecisionV1::Gameplay { .. })
                    {
                        Some((g.game_index, d))
                    } else {
                        None
                    }
                })
            })
            .collect();
        let mut cursor = 0;
        for ordinal in 0..5000 {
            let view = current(&mut service);
            let id = format!("step-{ordinal}");
            match view["phase"].as_str().unwrap() {
                "complete" => break,
                "play_draw" => {
                    let AgentPlayDrawPolicyV1::Fixed { choice } = packages[human].play_draw else {
                        unreachable!()
                    };
                    let game_index = service.game_index;
                    send(
                        &mut service,
                        HumanMatchCommandV1::PlayDraw {
                            request_id: id,
                            game_index,
                            choice,
                        },
                    );
                }
                "mulligan" => keep(&mut service, &id),
                "sideboard" => {
                    let deck = service.configurations[human].clone();
                    let game_index = service.game_index;
                    send(
                        &mut service,
                        HumanMatchCommandV1::Sideboard {
                            request_id: id,
                            game_index,
                            mainboard: deck.mainboard().to_vec(),
                            sideboard: deck.sideboard().to_vec(),
                        },
                    );
                }
                "decision" => {
                    let (game_index, expected) = expected[cursor];
                    cursor += 1;
                    assert_eq!(service.game_index, game_index);
                    let session = service.session.as_ref().unwrap();
                    let FastActorResponseV1::Decision(decision) = session.current_response() else {
                        panic!()
                    };
                    let (observation, actions, _) = session
                        .human_current_decision_input_v1(decision, decision.acting_player)
                        .unwrap();
                    let ActorVisibleDecisionV1::Gameplay {
                        observation: original,
                        ordered_actions,
                    } = &expected.visible
                    else {
                        panic!()
                    };
                    assert_eq!(
                        serde_json::to_value(&observation).unwrap(),
                        serde_json::to_value(original).unwrap()
                    );
                    assert_eq!(&actions, ordered_actions);
                    let prompt_seq = view["decision"]["prompt_seq"].as_u64().unwrap();
                    let (projected, action_index) =
                        crate::human_bo3_v1::project_recorded_decision_v1(
                            original,
                            ordered_actions,
                            expected.actor,
                            prompt_seq,
                            expected.behavior.selected_index_v1(),
                        )
                        .unwrap();
                    assert_eq!(serde_json::to_value(projected).unwrap(), view["decision"]);
                    send(
                        &mut service,
                        HumanMatchCommandV1::Action {
                            request_id: id,
                            prompt_seq,
                            action_index,
                        },
                    );
                }
                other => panic!("unexpected human phase {other}: {view}"),
            }
        }
        assert_eq!(current(&mut service)["phase"], "complete");
        assert_eq!(cursor, expected.len());
        assert_eq!(service.history.len(), automated.trajectory.games.len());
        for (completed, game) in service.history.iter().zip(&automated.games) {
            assert_eq!(completed.completion, GameSummaryCompletionV2::Natural);
            assert_eq!(
                completed.summary.winner.map(PlayerSeatV1::from),
                game.observed_terminal.as_ref().unwrap().winner
            );
        }
        let expected_model: Vec<_> = automated
            .trajectory
            .games
            .iter()
            .flat_map(|g| {
                g.decisions.iter().filter_map(move |d| {
                    if d.actor == PlayerSeatV1::from(PlayerId((1 - human) as u8)) {
                        if let ActorVisibleDecisionV1::Gameplay { observation, .. } = &d.visible {
                            return Some((
                                g.game_index,
                                observation.step_index,
                                d.behavior.selected_index_v1() as u64,
                            ));
                        }
                    }
                    None
                })
            })
            .collect();
        let records = journal(&service);
        let actual_model: Vec<_> = records
            .iter()
            .filter(|r| r["event"] == "model_action")
            .map(|r| {
                let p = &r["payload"];
                (
                    p["game_index"].as_u64().unwrap() as u8,
                    p["step"].as_u64().unwrap(),
                    p["selected"].as_u64().unwrap(),
                )
            })
            .collect();
        assert_eq!(actual_model, expected_model);
        let mut seeds = SplitMix64::seed(cfg.seed);
        for r in records.iter().filter(|r| r["event"] == "game_start") {
            assert_eq!(r["payload"]["environment_seed"], seeds.next_u64());
        }
        if human == 1 {
            let expected_sideboards: Vec<_> = automated
                .trajectory
                .games
                .iter()
                .filter(|g| g.game_index > 1)
                .map(|g| {
                    g.decisions
                        .iter()
                        .filter_map(|d| {
                            if d.actor == PlayerSeatV1::P0 {
                                if let ActorVisibleDecisionV1::Sideboard {
                                    ordered_actions, ..
                                } = &d.visible
                                {
                                    Some(ordered_actions[d.behavior.selected_index_v1()])
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            let actual_sideboards: Vec<_> = records
                .iter()
                .filter(|r| r["event"] == "model_sideboard_committed")
                .map(|r| r["payload"]["decision"]["actions"].clone())
                .collect();
            assert_eq!(
                actual_sideboards,
                expected_sideboards
                    .iter()
                    .map(|a| serde_json::to_value(a).unwrap())
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn human_v2_concessions_keep_real_opening_and_gameplay_provenance() {
    for human in 0..2 {
        for bottom in [false, true] {
            let (mut service, _, _) = fixture(human, false);
            begin(&mut service);
            let revision = service.opening_revision;
            send(
                &mut service,
                HumanMatchCommandV1::Mulligan {
                    request_id: "mulligan".into(),
                    game_index: 1,
                    opening_revision: revision,
                },
            );
            if bottom {
                keep(&mut service, "keep-for-bottom");
            }
            send(
                &mut service,
                HumanMatchCommandV1::Concede {
                    request_id: "concede-opening".into(),
                    game_index: 1,
                },
            );
            let result = &service.history[0];
            assert_eq!(result.summary.winner, Some(PlayerId((1 - human) as u8)));
            let GameSummaryCompletionV2::Concession {
                conceding_player,
                stage:
                    GameConcessionStageV2::Opening {
                        phase,
                        mulligans_taken,
                        ..
                    },
            } = &result.completion
            else {
                panic!()
            };
            assert_eq!(*conceding_player, PlayerId(human as u8));
            assert_eq!(*mulligans_taken, 1);
            assert_eq!(
                *phase,
                if bottom {
                    HumanOpeningPhaseV1::Bottom
                } else {
                    HumanOpeningPhaseV1::Mulligan
                }
            );
            assert_eq!(result.history.observed_gameplay_decisions, 0);
            assert_eq!(result.history.committed_policy_steps, 0);
            assert!(result.summary.resource_curve.lands_by_turn.is_empty());
        }
        let (mut service, _, _) = fixture(human, false);
        begin(&mut service);
        keep(&mut service, "kept");
        let steps = service.session.as_ref().unwrap().policy_step_count();
        send(
            &mut service,
            HumanMatchCommandV1::Concede {
                request_id: "concede-gameplay".into(),
                game_index: 1,
            },
        );
        assert!(matches!(
            service.history[0].completion,
            GameSummaryCompletionV2::Concession {
                stage: GameConcessionStageV2::Gameplay,
                ..
            }
        ));
        assert_eq!(service.history[0].history.committed_policy_steps, steps);
        assert_eq!(
            service
                .match_state
                .match_state()
                .wins(PlayerId((1 - human) as u8)),
            Ok(1)
        );
    }
}

#[test]
fn human_v2_retries_invalid_requests_and_legal_exchange_preserve_model_choice() {
    let (mut service, _, _) = fixture(0, true);
    begin(&mut service);
    let old = HumanMatchCommandV1::Mulligan {
        request_id: "old-mulligan".into(),
        game_index: 1,
        opening_revision: service.opening_revision,
    };
    let first = send(&mut service, old.clone());
    let revision = service.opening_revision;
    keep(&mut service, "later-keep");
    let opening_before = service.opening.as_ref().unwrap().view();
    assert_eq!(service.handle(old), first);
    assert_eq!(
        serde_json::to_value(service.opening.as_ref().unwrap().view()).unwrap(),
        serde_json::to_value(opening_before).unwrap()
    );
    assert_eq!(
        service.handle(HumanMatchCommandV1::Keep {
            request_id: "old-mulligan".into(),
            game_index: 1,
            opening_revision: revision
        })["ok"],
        false
    );
    let stale = service.handle(HumanMatchCommandV1::Bottom {
        request_id: "stale".into(),
        game_index: 1,
        opening_revision: revision,
        hand_indices: vec![0],
    });
    assert_eq!(stale["ok"], false);
    let opening_revision = service.opening_revision;
    send(
        &mut service,
        HumanMatchCommandV1::Bottom {
            request_id: "bottom".into(),
            game_index: 1,
            opening_revision,
            hand_indices: vec![0],
        },
    );
    let state = serde_json::to_vec(service.session.as_ref().unwrap().game_state()).unwrap();
    let view = current(&mut service);
    for _ in 0..3 {
        assert_eq!(current(&mut service), view);
    }
    assert_eq!(
        service.handle(HumanMatchCommandV1::Action {
            request_id: "invalid".into(),
            prompt_seq: view["decision"]["prompt_seq"].as_u64().unwrap(),
            action_index: u32::MAX
        })["ok"],
        false
    );
    assert_eq!(
        serde_json::to_vec(service.session.as_ref().unwrap().game_state()).unwrap(),
        state
    );
    send(
        &mut service,
        HumanMatchCommandV1::Concede {
            request_id: "concede".into(),
            game_index: 1,
        },
    );
    let model_choice = service.configurations[1].clone();
    let mut main = service.configurations[0].mainboard().to_vec();
    let mut side = service.configurations[0].sideboard().to_vec();
    std::mem::swap(&mut main[0], &mut side[0]);
    let expected_configuration =
        DeckConfigurationV1::new_exact_v1(main.clone(), side.clone()).unwrap();
    send(
        &mut service,
        HumanMatchCommandV1::Sideboard {
            request_id: "exchange".into(),
            game_index: 2,
            mainboard: main,
            sideboard: side,
        },
    );
    assert_eq!(service.configurations[1], model_choice);
    assert_eq!(service.configurations[0], expected_configuration);
    let records = journal(&service);
    assert!(
        records
            .iter()
            .position(|r| r["event"] == "model_sideboard_committed")
            .unwrap()
            < records
                .iter()
                .position(|r| r["event"] == "human_sideboard_committed")
                .unwrap()
    );
    let public = serde_json::to_string(&current(&mut service)).unwrap();
    for secret in [
        "model_sideboard",
        "checkpoint",
        "environment_seed",
        "resource_curve",
        "source_import",
        "embedding_table",
    ] {
        assert!(!public.contains(secret));
    }
}

#[test]
fn human_v2_caps_failures_and_package_rejection_do_not_fabricate_results() {
    let (mut capped, _, _) = fixture(0, false);
    capped.config.max_physical_games = 1;
    begin(&mut capped);
    send(
        &mut capped,
        HumanMatchCommandV1::Concede {
            request_id: "cap-concession".into(),
            game_index: 1,
        },
    );
    assert_eq!(current(&mut capped)["phase"], "stopped");
    assert_eq!(capped.history.len(), 1);
    assert!(!matches!(
        capped.match_state.match_state().phase(),
        MatchPhaseV1::Complete { .. }
    ));
    let (mut failed, _, _) = fixture(0, false);
    begin(&mut failed);
    // A mismatched loaded auxiliary models a stopped continuation failure only.
    let other = FrozenPlayPolicyV1::training_fixture_v3();
    let identity = SideboardPlayIdentityV1 {
        weights_sha256: other.actual_model_identity_v1().weights_sha256,
        git_head: "b".repeat(40),
    };
    let mut altered_embeddings = other.embedding_rows_v1().to_vec();
    altered_embeddings[16] += 0.5;
    let embeddings = FrozenSideboardEmbeddingsV1::new_v1(&altered_embeddings, identity).unwrap();
    failed.sideboard = Some(LearnedSideboardModelV1::new_v1(0, &embeddings));
    let old_configuration = failed.configurations[1].clone();
    assert_eq!(
        failed.handle(HumanMatchCommandV1::Concede {
            request_id: "failed-sideboard".into(),
            game_index: 1
        })["ok"],
        false
    );
    assert_eq!(failed.history.len(), 1);
    assert_eq!(failed.match_state.match_state().wins(PlayerId::P1), Ok(1));
    assert_eq!(failed.configurations[1], old_configuration);
    assert_eq!(current(&mut failed)["phase"], "stopped");
    assert!(journal(&failed).iter().any(|r| r["event"] == "game_result"));
    let mut config = failed.config.clone();
    config.journal_path = directory().join("must-not-exist.jsonl");
    std::fs::write(&config.package.path, b"{}").unwrap();
    assert!(HumanMatchServiceV2::new(config.clone()).is_err());
    assert!(!config.journal_path.exists());
    assert!(
        strict::<HumanMatchCommandV1>(
            br#"{"command":"current","request_id":"a","request_id":"b"}"#
        )
        .is_err()
    );
}
