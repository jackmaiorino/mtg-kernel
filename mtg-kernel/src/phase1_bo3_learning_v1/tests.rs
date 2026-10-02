use super::*;
use crate::bo3_match::PlayDrawChoiceV1;
use crate::phase1_bo3_collection_v1::{
    collect_loaded_inner,
    tests::{config, fixtures, fixtures_v4},
};

/// `pub(crate)`, not private: `continuation::tests` (a sibling module) reuses
/// this, `captured`/`captured_v4`'s fixed limit, to build a real, in-memory
/// `TrainableResultDto` for `apply_prepared`'s own regression test.
pub(crate) fn limits() -> Bo3NativeCaptureLimitsV1 {
    Bo3NativeCaptureLimitsV1 {
        max_payload_bytes: 256 * 1024 * 1024,
        max_json_bytes: 256 * 1024 * 1024,
    }
}

/// Actual native models and original engine captures. File/runtime metadata
/// comes from the clearly test-only fixture and is never producer-verified.
///
/// `pub(crate)`, not private: `continuation::tests` reuses this, alongside
/// `captured_v4`, `replay_attempt`, `finish_prepared` and
/// `with_native_groups_v1`, to drive a real V3 `PreparedBo3GameplayBatchV1`
/// through `apply_prepared` and cross-check the resulting Adam step and
/// parameters against a direct `train_step_weighted_feature_transfer_v3`
/// call on the same groups. Nothing about this function's behavior changes.
pub(crate) fn captured(
    config: Bo3CollectionConfigV1,
    choices: [PlayDrawChoiceV1; 2],
    limits: Bo3NativeCaptureLimitsV1,
    compare_disabled: bool,
) -> (TrainableResultDto, [FrozenPlayPolicyV1; 2]) {
    let (mut policies, packages) = fixtures(choices);
    let disabled = compare_disabled.then(|| {
        collect_loaded_inner(
            &config,
            packages.each_ref(),
            &mut policies,
            [None, None],
            None,
        )
        .unwrap()
    });
    let mut sink = CaptureBuffer::new(limits.clone()).unwrap();
    let collected = collect_loaded_inner(
        &config,
        packages.each_ref(),
        &mut policies,
        [None, None],
        Some(&mut sink),
    )
    .unwrap();
    if let Some(disabled) = disabled {
        assert_eq!(
            serde_json::to_vec(&disabled).unwrap(),
            serde_json::to_vec(&collected).unwrap(),
            "capture must not change any V1 core result byte"
        );
    }
    let request = TrainableBo3RequestV1 {
        schema: TRAINABLE_BO3_REQUEST_SCHEMA_V1.into(),
        config: config.clone(),
        packages: packages.clone(),
        capture_limits: limits,
    };
    let current_runtimes = packages.each_ref().map(|p| ProducerRuntimeClaimV1 {
        executable_path: "test-only-unverified-producer".into(),
        executable_sha256: p.runtime.executable.sha256.clone(),
        engine_commit: p.runtime.engine_commit.clone(),
        tracked_tree_sha256: p.runtime.tracked_tree_sha256.clone(),
        tracked_tree_contract: p.runtime.tracked_tree_contract.clone(),
        toolchain_sha256: p.runtime.toolchain.sha256.clone(),
    });
    let result = TrainableResultDto {
        schema: TRAINABLE_BO3_RESULT_SCHEMA_V1.into(),
        request,
        result: CollectionResultDto {
            schema: BO3_COLLECTION_RESULT_SCHEMA_V1.into(),
            config,
            packages,
            current_runtimes,
            collected: serde_json::from_value(serde_json::to_value(collected).unwrap()).unwrap(),
        },
        native_capture: sink.finish(),
    };
    (result, policies)
}

/// V4 sibling of `captured`: same real native-model/original-engine capture
/// shape, sourced from the compact-board V4 fresh-lineage fixture pairing
/// (`fixtures_v4`) instead of V3's, so `replay_attempt`'s generation
/// dispatch is exercised against a real, complete V4 game.
///
/// `pub(crate)`, not private: see `captured`'s doc comment.
pub(crate) fn captured_v4(
    config: Bo3CollectionConfigV1,
    choices: [PlayDrawChoiceV1; 2],
    limits: Bo3NativeCaptureLimitsV1,
) -> (TrainableResultDto, [FrozenPlayPolicyV1; 2]) {
    let (mut policies, packages) = fixtures_v4(choices);
    let mut sink = CaptureBuffer::new(limits.clone()).unwrap();
    let collected = collect_loaded_inner(
        &config,
        packages.each_ref(),
        &mut policies,
        [None, None],
        Some(&mut sink),
    )
    .unwrap();
    let request = TrainableBo3RequestV1 {
        schema: TRAINABLE_BO3_REQUEST_SCHEMA_V1.into(),
        config: config.clone(),
        packages: packages.clone(),
        capture_limits: limits,
    };
    let current_runtimes = packages.each_ref().map(|p| ProducerRuntimeClaimV1 {
        executable_path: "test-only-unverified-producer".into(),
        executable_sha256: p.runtime.executable.sha256.clone(),
        engine_commit: p.runtime.engine_commit.clone(),
        tracked_tree_sha256: p.runtime.tracked_tree_sha256.clone(),
        tracked_tree_contract: p.runtime.tracked_tree_contract.clone(),
        toolchain_sha256: p.runtime.toolchain.sha256.clone(),
    });
    let result = TrainableResultDto {
        schema: TRAINABLE_BO3_RESULT_SCHEMA_V1.into(),
        request,
        result: CollectionResultDto {
            schema: BO3_COLLECTION_RESULT_SCHEMA_V1.into(),
            config,
            packages,
            current_runtimes,
            collected: serde_json::from_value(serde_json::to_value(collected).unwrap()).unwrap(),
        },
        native_capture: sink.finish(),
    };
    (result, policies)
}

fn rehash(result: &mut TrainableResultDto) {
    result.result.collected.trajectory_sha256 =
        hash_json(&result.result.collected.trajectory).unwrap();
    result.result.collected.committed_decision_records = result
        .result
        .collected
        .trajectory
        .games
        .iter()
        .map(|g| g.decisions.len() as u64)
        .sum();
    result.result.collected.committed_decision_json_bytes = result
        .result
        .collected
        .trajectory
        .games
        .iter()
        .flat_map(|g| &g.decisions)
        .map(|d| canonical_size(d, u64::MAX).unwrap())
        .sum();
    result.native_capture.committed_payload_bytes = result
        .native_capture
        .records
        .iter()
        .map(|r| r.payload_bytes().unwrap())
        .sum();
    result.native_capture.committed_json_bytes = result
        .native_capture
        .records
        .iter()
        .map(|r| canonical_size(r, u64::MAX).unwrap())
        .sum();
}

/// Structural grouping test only: regroup two original adjacent same-actor
/// scorer rows without changing tensors, selections, RNG or the observed real
/// terminal. This does not claim the engine emitted this V6 decomposition.
fn assert_structural_complete_multisubstep_group(
    original: &TrainableResultDto,
    policies: [&FrozenPlayPolicyV1; 2],
) {
    let mut regrouped = original.clone();
    let game = &mut regrouped.result.collected.trajectory.games[0];
    let pair = game
        .decisions
        .windows(2)
        .position(|rows| {
            rows.iter().all(|row| row.actor == PlayerSeatV1::P0)
                && rows.iter().all(|row| {
                    matches!(
                        &row.visible,
                        ActorVisibleDecisionV1::Gameplay { observation, .. }
                            if observation.substep_count == 1
                    )
                })
        })
        .expect("actual fixture must contain adjacent learner gameplay rows");
    let mut next_physical = 0;
    for (i, decision) in game.decisions.iter_mut().enumerate() {
        if let ActorVisibleDecisionV1::Gameplay { observation, .. } = &mut decision.visible {
            observation.physical_decision_id = next_physical;
            observation.substep_index = if i == pair + 1 { 1 } else { 0 };
            observation.substep_count = if i == pair || i == pair + 1 { 2 } else { 1 };
            if i != pair {
                next_physical += 1;
            }
        }
    }
    regrouped.result.collected.games[0]
        .observed_terminal
        .as_mut()
        .unwrap()
        .physical_decision_count = next_physical;
    rehash(&mut regrouped);
    let prepared =
        replay_attempt(regrouped, PlayerSeatV1::P0, policies, MAX_PREPARED_BYTES).unwrap();
    assert_eq!(
        prepared
            .groups
            .iter()
            .filter(|g| g.substeps.len() == 2)
            .count(),
        1
    );
    assert_eq!(
        prepared.substeps,
        prepared
            .groups
            .iter()
            .map(|g| g.substeps.len())
            .sum::<usize>()
    );
    let learner = original.result.packages[0].gameplay.clone();
    let batch = finish_prepared(
        learner,
        FreshLineageGenerationV1::V3,
        vec![prepared.report],
        prepared.groups,
        prepared.payload,
        prepared.substeps,
    )
    .unwrap();
    batch.with_native_groups_v1(|groups, weights| {
        let index = groups.iter().position(|g| g.substeps.len() == 2).unwrap();
        assert_eq!(
            weights[index].to_bits(),
            weights[0].to_bits(),
            "a two-substep physical group must receive one group weight"
        );
        assert_eq!(groups[index].substeps.len(), 2);
    });
}

#[test]
fn phase1_bo3_training_actual_two_three_game_capture_preserves_core_and_equal_match_groups() {
    use PlayDrawChoiceV1::{Draw, Play};
    let (three, policies) = captured(config("native-capture-three"), [Play, Play], limits(), true);
    assert_eq!(three.result.collected.trajectory.games.len(), 3);
    let learner = three.result.packages[0].gameplay.clone();
    assert!(three.native_capture.records.len() > 100);
    assert_structural_complete_multisubstep_group(&three, policies.each_ref());
    assert!(
        three
            .result
            .collected
            .trajectory
            .games
            .iter()
            .flat_map(|g| &g.decisions)
            .filter(|d| matches!(d.visible, ActorVisibleDecisionV1::Gameplay { .. }))
            .count()
            == three.native_capture.records.len()
    );
    let first = replay_attempt(
        three,
        PlayerSeatV1::P0,
        policies.each_ref(),
        MAX_PREPARED_BYTES,
    )
    .unwrap();
    assert!(first.report.complete && first.report.eligible);
    let (two, policies) = captured(config("native-capture-two"), [Play, Draw], limits(), false);
    assert_eq!(two.result.collected.trajectory.games.len(), 2);
    let second = replay_attempt(
        two,
        PlayerSeatV1::P0,
        policies.each_ref(),
        MAX_PREPARED_BYTES - first.payload,
    )
    .unwrap();
    assert!(second.report.complete && second.report.eligible);
    let counts = [first.groups.len(), second.groups.len()];
    assert_ne!(
        counts[0], counts[1],
        "natural matches must witness unequal decision counts"
    );
    let payload = first.payload + second.payload;
    let substeps = first.substeps + second.substeps;
    let reports = vec![first.report, second.report];
    let all = first.groups.into_iter().chain(second.groups).collect();
    let prepared = finish_prepared(
        learner,
        FreshLineageGenerationV1::V3,
        reports,
        all,
        payload,
        substeps,
    )
    .unwrap();
    assert_eq!(
        prepared.report.disposition,
        Bo3PreparationDispositionV1::Ready
    );
    assert_eq!(prepared.report.eligible_matches, 2);
    let mut offset = 0;
    for count in counts {
        let expected = (1.0_f64 / 2.0 / count as f64) as f32;
        assert!(prepared.report.weight_bits[offset..offset + count]
            .iter()
            .all(|b| *b == expected.to_bits()));
        let sum: f64 = prepared.report.weight_bits[offset..offset + count]
            .iter()
            .map(|b| f64::from(f32::from_bits(*b)))
            .sum();
        assert!((sum - 0.5).abs() < 1e-7);
        offset += count;
    }
    prepared.with_native_groups_v1(|groups, weights| {
        assert_eq!(groups.len(), counts.iter().sum::<usize>());
        assert_eq!(weights.len(), groups.len());
        assert!(groups
            .iter()
            .all(|g| g.baseline_bits == 0 && matches!(g.terminal_return, -1 | 1)));
        assert!(groups[..counts[0]]
            .iter()
            .all(|g| g.terminal_return == groups[0].terminal_return));
        assert!(groups[counts[0]..]
            .iter()
            .all(|g| g.terminal_return == groups[counts[0]].terminal_return));
        // Opponent rows were replayed, but the borrowed update groups contain
        // only the requested learner's committed actual scorer tensors.
        assert!(groups
            .iter()
            .flat_map(|g| g.substeps)
            .all(|s| matches!(s.forward, NativePolicyForwardInputV1::Encoded(_))));
    });
}

/// V4 sibling of the test above: drives a real, complete V4 fresh-lineage
/// game through `replay_attempt` (whole-tuple generation dispatch, never a
/// flag: a V4 policy's `successor` is `None`, so falling through to the V3
/// scorer would fail loudly with "training requires explicit successor
/// features" rather than silently mis-scoring) and confirms the prepared
/// batch is admitted by `finish_prepared`/`with_native_groups_v1` all the
/// way to a native update-ready group, exactly like the V3 path.
#[test]
fn phase1_bo3_training_actual_v4_game_capture_replays_through_native_groups() {
    let (result, policies) = captured_v4(
        config("native-capture-v4"),
        [PlayDrawChoiceV1::Play, PlayDrawChoiceV1::Draw],
        limits(),
    );
    let learner = result.result.packages[0].gameplay.clone();
    assert!(!result.native_capture.records.is_empty());
    let prepared = replay_attempt(
        result,
        PlayerSeatV1::P0,
        policies.each_ref(),
        MAX_PREPARED_BYTES,
    )
    .unwrap();
    assert!(prepared.report.complete && prepared.report.eligible);
    assert!(!prepared.groups.is_empty());
    let batch = finish_prepared(
        learner,
        FreshLineageGenerationV1::V4,
        vec![prepared.report],
        prepared.groups,
        prepared.payload,
        prepared.substeps,
    )
    .unwrap();
    assert_eq!(batch.learner_generation_v1(), FreshLineageGenerationV1::V4);
    assert_eq!(
        batch.report_v1().disposition,
        Bo3PreparationDispositionV1::Ready
    );
    batch.with_native_groups_v1(|groups, weights| {
        assert!(!groups.is_empty());
        assert_eq!(weights.len(), groups.len());
        assert!(groups
            .iter()
            .all(|g| g.baseline_bits == 0 && matches!(g.terminal_return, -1 | 1)));
        // Opponent rows were replayed via score_training_tensor_v4, but the
        // borrowed update groups contain only the requested learner's
        // committed actual scorer tensors, encoded through the V4 schema.
        assert!(groups
            .iter()
            .flat_map(|g| g.substeps)
            .all(|s| matches!(s.forward, NativePolicyForwardInputV1::Encoded(_))));
    });
}

#[test]
fn phase1_bo3_training_capped_prefix_keeps_witnesses_and_no_update() {
    let mut cfg = config("native-capture-capped");
    cfg.max_physical_decisions = 12;
    let (mut result, policies) = captured(cfg, [PlayDrawChoiceV1::Play; 2], limits(), true);
    assert!(matches!(
        result.result.collected.trajectory.ending,
        Bo3TrajectoryEndingV1::Incomplete { .. }
    ));
    assert!(!result.native_capture.records.is_empty());
    let learner = result.result.packages[0].gameplay.clone();
    let prepared = replay_attempt(
        result.clone(),
        PlayerSeatV1::P0,
        policies.each_ref(),
        MAX_PREPARED_BYTES,
    )
    .unwrap();
    assert!(!prepared.report.eligible && prepared.groups.is_empty() && prepared.payload == 0);
    let batch = finish_prepared(
        learner,
        FreshLineageGenerationV1::V3,
        vec![prepared.report],
        Vec::new(),
        0,
        0,
    )
    .unwrap();
    assert_eq!(
        batch.report.disposition,
        Bo3PreparationDispositionV1::NoUpdate
    );
    assert_eq!(batch.report.incomplete_matches, 1);
    assert!(batch.report.weight_bits.is_empty());
    // A structural missing-suffix fixture does not invent a winner. Valid
    // committed rows stay available even if their last physical group is partial.
    let last = result
        .result
        .collected
        .trajectory
        .games
        .last_mut()
        .unwrap()
        .decisions
        .iter_mut()
        .rev()
        .find(|d| matches!(d.visible, ActorVisibleDecisionV1::Gameplay { .. }))
        .unwrap();
    if let ActorVisibleDecisionV1::Gameplay { observation, .. } = &mut last.visible {
        assert_eq!(observation.substep_count, 1);
        observation.substep_count = 2;
    }
    rehash(&mut result);
    let partial = replay_attempt(
        result,
        PlayerSeatV1::P0,
        policies.each_ref(),
        MAX_PREPARED_BYTES,
    )
    .unwrap();
    assert!(!partial.report.eligible && partial.groups.is_empty());
}

#[test]
fn phase1_bo3_training_native_capture_cap_drops_pending_without_touching_v1_default() {
    let mut cfg = config("native-capture-byte-cap");
    cfg.max_physical_decisions = 12;
    let (result, _) = captured(
        cfg.clone(),
        [PlayDrawChoiceV1::Play; 2],
        Bo3NativeCaptureLimitsV1 {
            max_payload_bytes: 1,
            max_json_bytes: 1,
        },
        false,
    );
    assert!(result.native_capture.records.is_empty());
    assert!(result.result.collected.games[0].discarded_pending_selections > 0);
    assert!(matches!(
        result.result.collected.trajectory.ending,
        Bo3TrajectoryEndingV1::Incomplete {
            reason: IncompleteMatchReasonV1::DecisionCap
        }
    ));
    assert!(result.result.collected.trajectory.games[0]
        .decisions
        .iter()
        .all(|d| !matches!(d.visible, ActorVisibleDecisionV1::Gameplay { .. })));
    // The old core path still runs independently of the native capture bound.
    let (mut policies, packages) = fixtures([PlayDrawChoiceV1::Play; 2]);
    let old =
        collect_loaded_inner(&cfg, packages.each_ref(), &mut policies, [None, None], None).unwrap();
    assert!(old.trajectory.games[0]
        .decisions
        .iter()
        .any(|d| matches!(d.visible, ActorVisibleDecisionV1::Gameplay { .. })));
}

#[test]
fn phase1_bo3_training_rejects_tampered_original_witnesses_and_all_actor_sampler_replay() {
    let mut cfg = config("native-capture-tampering");
    cfg.max_physical_decisions = 12;
    let (base, policies) = captured(cfg, [PlayDrawChoiceV1::Play; 2], limits(), false);
    assert!(base.native_capture.records.len() >= 3);
    for change in 0..9 {
        let mut changed = base.clone();
        match change {
            0 => changed.native_capture.records[0].raw_logit_bits[0] ^= 1,
            1 => changed.native_capture.records[0].raw_value_bits ^= 1,
            2 => changed.native_capture.records.swap(0, 1),
            3 => {
                changed.native_capture.records.pop();
            }
            4 => changed
                .native_capture
                .records
                .push(changed.native_capture.records[0].clone()),
            5 => changed.native_capture.records[0]
                .tensor_bits
                .state
                .pop()
                .map(|_| ())
                .unwrap(),
            6 => changed.result.collected.games[0].environment_seed = Some(1),
            7 => {
                let opponent = changed.result.collected.trajectory.games[0]
                    .decisions
                    .iter()
                    .find(|d| {
                        d.actor == PlayerSeatV1::P1
                            && matches!(d.visible, ActorVisibleDecisionV1::Gameplay { .. })
                    })
                    .unwrap()
                    .decision_index;
                changed
                    .native_capture
                    .records
                    .iter_mut()
                    .find(|r| r.decision_index == opponent)
                    .unwrap()
                    .raw_value_bits ^= 1;
            }
            _ => {
                let gameplay = changed.result.collected.trajectory.games[0]
                    .decisions
                    .iter_mut()
                    .find(|d| matches!(d.visible, ActorVisibleDecisionV1::Gameplay { .. }))
                    .unwrap();
                if let ActorVisibleDecisionV1::Gameplay { observation, .. } = &mut gameplay.visible
                {
                    observation.physical_decision_id += 1;
                }
            }
        }
        rehash(&mut changed);
        assert!(
            replay_attempt(
                changed,
                PlayerSeatV1::P0,
                policies.each_ref(),
                MAX_PREPARED_BYTES
            )
            .is_err(),
            "mutation {change}"
        );
    }
    let before = policies.each_ref().map(|p| p.actual_model_identity_v1());
    replay_attempt(
        base,
        PlayerSeatV1::P0,
        policies.each_ref(),
        MAX_PREPARED_BYTES,
    )
    .unwrap();
    assert_eq!(
        before,
        policies.each_ref().map(|p| p.actual_model_identity_v1())
    );
}

#[test]
fn phase1_bo3_training_strict_reader_rejects_nested_unknowns_and_does_not_mint_runtime_claims() {
    let mut cfg = config("native-capture-reader");
    cfg.max_physical_decisions = 2;
    let (result, _) = captured(cfg, [PlayDrawChoiceV1::Play; 2], limits(), false);
    let bytes = serde_json::to_vec(&result).unwrap();
    let _: TrainableResultDto = strict_json(&bytes).unwrap();
    assert!(
        verify_producer(&result.result).is_err(),
        "test-only producer metadata cannot become a verified current runtime"
    );
    let mut json = serde_json::to_value(&result).unwrap();
    json["result"]["packages"][0]["gameplay"]["identity"]["source_import"]["unknown"] =
        serde_json::json!(123);
    assert!(strict_json::<TrainableResultDto>(&serde_json::to_vec(&json).unwrap()).is_err());
    assert!(strict_json::<serde_json::Value>(br#"{"nested":{"x":1,"x":2}}"#).is_err());
    assert!(ordinary_source(&result.request.packages[0].gameplay)
        .unwrap_err()
        .contains("ordinary existing checkpoint"));
    assert!(
        collect_trainable_bo3_v1(result.request).is_err(),
        "public capture cannot accept fixture metadata as an ordinary real checkpoint"
    );
}

#[test]
fn phase1_bo3_training_rejects_rehashed_core_config_and_sampler_mismatches() {
    let mut cfg = config("native-binding-negatives");
    cfg.max_physical_decisions = 2;
    let (base, policies) = captured(cfg, [PlayDrawChoiceV1::Play; 2], limits(), false);
    for change in 0..4 {
        let mut changed = base.clone();
        match change {
            0 => changed.result.collected.trajectory.match_id = "other-valid-match".into(),
            1 => changed.result.collected.trajectory.initial_chooser = PlayerSeatV1::P1,
            2 => changed
                .result
                .collected
                .trajectory
                .registrations_by_seat
                .swap(0, 1),
            _ => {
                changed.result.packages[1].gameplay_sampler_identity =
                    crate::fast_sampler::FAST_CATEGORICAL_SAMPLER_VERSION.into();
                changed.request.packages = changed.result.packages.clone();
                let pins = changed
                    .result
                    .packages
                    .each_ref()
                    .map(|p| p.package_sha256_v1().unwrap());
                changed
                    .result
                    .collected
                    .trajectory
                    .behavior_packages_by_seat = pins.clone();
                for game in &mut changed.result.collected.trajectory.games {
                    for decision in &mut game.decisions {
                        decision.behavior_package_sha256 = pins[seat(decision.actor)].clone();
                    }
                }
            }
        }
        rehash(&mut changed);
        // Recompute and read the exact result bytes, so rejection is semantic,
        // not an accidental stale JSON digest. File pin integration is separate.
        let bytes = serde_json::to_vec(&changed).unwrap();
        let decoded = strict_json::<TrainableResultDto>(&bytes).unwrap();
        let error = replay_attempt(
            decoded,
            PlayerSeatV1::P0,
            policies.each_ref(),
            MAX_PREPARED_BYTES,
        )
        .err()
        .expect("inconsistent producer must be rejected");
        assert!(
            error.contains(if change == 3 {
                "package sampler differs"
            } else {
                "trajectory match id, chooser or registrations"
            }),
            "{error}"
        );
    }
}

#[test]
fn phase1_bo3_training_physical_identity_ignores_caps_transport_and_optimizer_history() {
    let (_, packages) = fixtures([PlayDrawChoiceV1::Play; 2]);
    let cfg = config("physical-identity-original");
    let original = physical_match_identity(&cfg, packages.each_ref()).unwrap();
    let mut relocated = packages.clone();
    for package in &mut relocated {
        package.runtime.executable.path = "relocated/executable".into();
        package.runtime.toolchain.path = "relocated/toolchain".into();
        package.runtime.engine_commit = "d".repeat(40);
        package.runtime.tracked_tree_sha256 = "e".repeat(64);
        package.gameplay.source.play_import.path = "relocated/import".into();
    }
    // A frozen opponent with identical installed weights but different Adam
    // history remains the same inference opponent. Fresh learner equality is a
    // separate full source/model/Adam check in the public preparation path.
    relocated[1].gameplay.source.checkpoint = Some(PinnedFileV1 {
        path: "relocated/other-checkpoint".into(),
        sha256: "f".repeat(64),
    });
    relocated[1].gameplay.identity.checkpoint_sha256 = Some("f".repeat(64));
    relocated[1].gameplay.identity.state_sha256 = "d".repeat(64);
    relocated[1].gameplay.identity.adam_step += 100;
    let crate::sideboard_play_policy_v1::PlayPolicyOriginV1::Imported(origin) =
        &mut relocated[1].gameplay.identity.source_import
    else {
        panic!("fixture must retain imported origin")
    };
    origin.source_generation += 1;
    let mut shortened = cfg.clone();
    shortened.match_id = "different-label".into();
    shortened.deck_ids = ["other-label-a".into(), "other-label-b".into()];
    shortened.max_physical_games = 1;
    shortened.max_physical_decisions = 2;
    shortened.max_policy_steps = 3;
    shortened.max_decision_records = 4;
    shortened.max_decision_json_bytes = 5;
    shortened
        .summary_tags
        .requires_target
        .insert(shortened.registrations[0].mainboard[0]);
    let duplicate = physical_match_identity(&shortened, relocated.each_ref()).unwrap();
    assert_eq!(original, duplicate);
    let mut seen = BTreeSet::new();
    assert!(seen.insert(original.clone()));
    assert!(
        !seen.insert(duplicate),
        "caps, labels and relocation cannot bypass duplicate rejection"
    );
    for change in 0..5 {
        let mut changed_config = cfg.clone();
        let mut changed_packages = packages.clone();
        match change {
            0 => changed_config.seed += 1,
            1 => changed_config.initial_chooser = PlayerSeatV1::P1,
            2 => changed_config.registrations.swap(0, 1),
            3 => {
                changed_packages[1].play_draw = AgentPlayDrawPolicyV1::Fixed {
                    choice: PlayDrawChoiceV1::Draw,
                }
            }
            _ => {
                changed_packages[1]
                    .gameplay
                    .identity
                    .model
                    .model_parameter_sha256 = "d".repeat(64)
            }
        }
        assert_ne!(
            original,
            physical_match_identity(&changed_config, changed_packages.each_ref()).unwrap()
        );
    }
}
