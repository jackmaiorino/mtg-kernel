use super::*;
use crate::expanded_deck_training_v1::tests::replay_fixture;
use std::sync::Barrier;

const LR: f32 = 0.00003;
const VC: f32 = 0.75;

fn train_groups(state: &mut NativePolicyValueTrainStateV1, groups: &[LearnerTensorGroupV1<'_>]) {
    let substeps: Vec<Vec<NativePolicySubstepV1<'_>>> = groups
        .iter()
        .map(|(_, rows)| {
            rows.iter()
                .map(|(row, tensor)| NativePolicySubstepV1 {
                    forward: NativePolicyForwardInputV1::Encoded(Box::new(
                        encoded_decision_view_generic_v1(tensor, FreshLineageGenerationV1::V3),
                    )),
                    selected_action_index: row.selected as usize,
                    expected_raw_action_logit_bits: &row.logits,
                    expected_value_bits: row.value,
                })
                .collect()
        })
        .collect();
    let native: Vec<_> = substeps
        .iter()
        .zip(groups)
        .map(|(substeps, (reward, _))| NativePolicyPhysicalDecisionV1 {
            substeps,
            terminal_return: *reward,
            baseline_bits: 0,
        })
        .collect();
    state
        .train_step_feature_transfer_v3(&native, VC, LR)
        .unwrap();
}

fn state_for(policy: &FrozenPlayPolicyV1) -> NativePolicyValueTrainStateV1 {
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    model
        .replace_parameter_snapshot_v1(&policy.training_parameters_v3())
        .unwrap();
    NativePolicyValueTrainStateV1::new_v1(model).unwrap()
}

fn bind_state(
    t: &mut ExpandedTrajectoryV1,
    learner: &mut ExpandedSeatBehaviorV1,
    state: &NativePolicyValueTrainStateV1,
) {
    learner.identity.state_sha256 = hex(&state.state_sha256_v1().unwrap());
    learner.identity.adam_step = state.adam_step_v1();
    t.behavior_state_sha256 = learner.identity.state_sha256.clone();
    if let Some(seats) = &mut t.seat_behaviors {
        seats[t.episode.learner_seat as usize] = learner.clone();
    }
}

fn paired_substeps(t: &mut ExpandedTrajectoryV1) {
    assert_eq!(t.decisions.len() % 2, 0);
    for (index, group) in t.decisions.chunks_mut(2).enumerate() {
        assert_eq!(group[0].actor, group[1].actor);
        for (substep, row) in group.iter_mut().enumerate() {
            row.physical_decision_id = index as u64;
            row.substep_index = substep as u32;
            row.substep_count = 2;
        }
    }
    t.terminal.physical_decision_count = (t.decisions.len() / 2) as u64;
    validate_trajectory(t).unwrap();
}

fn group_bytes(groups: &[LearnerTensorGroupV1<'_>]) -> Vec<u8> {
    let rows: Vec<_> = groups
        .iter()
        .map(|(reward, rows)| {
            (
                *reward,
                rows.iter()
                    .map(|(record, tensor)| {
                        (
                            record.step,
                            record.physical_decision_id,
                            record.substep_index,
                            record.substep_count,
                            record.actor,
                            TensorBitsV1::from_tensor(tensor),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    serde_json::to_vec(&rows).unwrap()
}

fn snapshot_bits(state: &NativePolicyValueTrainStateV1) -> (u64, u32, Vec<Vec<Vec<u32>>>) {
    let snapshot = state.snapshot_v1().unwrap();
    (
        snapshot.adam_step,
        snapshot.scorer_bias_anchor_bits,
        [
            &snapshot.parameters,
            &snapshot.first_moments,
            &snapshot.second_moments,
        ]
        .iter()
        .map(|tensors| tensors.iter().map(|tensor| bits(&tensor.values)).collect())
        .collect(),
    )
}

#[test]
fn phase1_preparation_real_updates_preserve_all_state_bits_and_group_order() {
    // These are real actor-visible decision tensors and real native updates.
    // The terminal/grouping fixture is synthetic, not a completed-game result.
    for workers in [1, 4, 10] {
        let mut serial_policy = FrozenPlayPolicyV1::training_fixture_v3();
        let mut parallel_policy = serial_policy.fork_for_collection_v3().unwrap();
        let mut serial = state_for(&serial_policy);
        let mut parallel = state_for(&parallel_policy);
        let mut opponent = FrozenPlayPolicyV1::training_fixture_v3();
        let original_opponent = opponent.actual_model_identity_v1();
        let mut parameters = opponent.training_parameters_v3();
        parameters
            .iter_mut()
            .find(|p| p.name == "card_embedding.weight")
            .unwrap()
            .values[crate::native_policy_value_net_v1::CARD_EMBEDDING_DIM_V1] += 0.03125;
        opponent
            .replace_training_parameters_v3(&parameters)
            .unwrap();
        let frozen_opponent = opponent.actual_model_identity_v1();
        assert_ne!(frozen_opponent, original_opponent);
        for update in 0..3 {
            let actors: Vec<_> = (0..16).flat_map(|_| [0, 0, 1, 1]).collect();
            let (mut first, mut learner, other) =
                replay_fixture(&mut serial_policy, Some(&mut opponent), 0, &actors);
            bind_state(&mut first, &mut learner, &serial);
            first.episode.id = format!("ordered-preparation-{update}-0");
            paired_substeps(&mut first);
            let mut second = first.clone();
            second.episode.id = format!("ordered-preparation-{update}-1");
            let episodes = [first, second];
            let mut reference = Vec::new();
            for episode in &episodes {
                validate_actual_behaviors_v1(episode, &learner, other.as_ref()).unwrap();
                reference.extend(
                    replay_learner_groups_v1(episode, &serial_policy, Some(&opponent)).unwrap(),
                );
            }
            let mut loads = 0;
            let prepared =
                prepare_with_loader_v1(&episodes, &parallel_policy, &learner, workers, |source| {
                    loads += 1;
                    assert_eq!(source, &other.as_ref().unwrap().source);
                    Ok(LoadedOpponentV1 {
                        policy: opponent.fork_for_collection_v3().unwrap(),
                        behavior: other.clone().unwrap(),
                    })
                })
                .unwrap();
            assert_eq!(loads, 1, "repeated opponent must load only once");
            assert_eq!(prepared.telemetry.opponent_load_calls, 1);
            assert_eq!(prepared.telemetry.physical_group_jobs, 64);
            assert_eq!(prepared.telemetry.started_workers, workers);
            assert_eq!(group_bytes(&reference), group_bytes(&prepared.groups));
            train_groups(&mut serial, &reference);
            train_groups(&mut parallel, &prepared.groups);
            assert_eq!(snapshot_bits(&serial), snapshot_bits(&parallel));
            assert_eq!(
                serial.state_sha256_v1().unwrap(),
                parallel.state_sha256_v1().unwrap()
            );
            assert_eq!(opponent.actual_model_identity_v1(), frozen_opponent);
            serial_policy
                .replace_training_parameters_v3(&serial.snapshot_v1().unwrap().parameters)
                .unwrap();
            parallel_policy
                .replace_training_parameters_v3(&parallel.snapshot_v1().unwrap().parameters)
                .unwrap();
        }
        assert_eq!(serial.adam_step_v1(), 3);
        assert!(serial
            .snapshot_v1()
            .unwrap()
            .first_moments
            .iter()
            .flat_map(|t| &t.values)
            .any(|v| *v != 0.0));
    }
}

#[test]
fn phase1_preparation_current_model_reuse_avoids_disk_load_and_keeps_rng_outputs() {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let (mut t, learner, _) = replay_fixture(&mut policy, None, 0, &[0, 1, 0, 1]);
    t.schema = POPULATION_TRAJECTORY_SCHEMA.into();
    t.episode.opponent = Some(learner.source.clone());
    t.seat_behaviors = Some([learner.clone(), learner.clone()]);
    validate_trajectory(&t).unwrap();
    let expected = replay_learner_groups_v1(&t, &policy, Some(&policy)).unwrap();
    let prepared = prepare_with_loader_v1(std::slice::from_ref(&t), &policy, &learner, 4, |_| {
        panic!("current source should reuse the already validated learner")
    })
    .unwrap();
    assert_eq!(prepared.telemetry.opponent_load_calls, 0);
    assert_eq!(prepared.telemetry.current_model_reuses, 1);
    assert_eq!(group_bytes(&expected), group_bytes(&prepared.groups));
}

#[test]
fn phase1_preparation_rejects_stale_and_corrupted_opponent_before_learning() {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let mut opponent = FrozenPlayPolicyV1::training_fixture_v3();
    let (original, learner, other) =
        replay_fixture(&mut policy, Some(&mut opponent), 0, &[0, 1, 0, 1]);
    let before = policy.actual_model_identity_v1();
    for workers in [1, 4] {
        for kind in [
            "stale_learner",
            "opponent_identity",
            "opponent_value",
            "missing_opponent",
        ] {
            let mut changed = original.clone();
            if kind == "stale_learner" {
                changed.behavior_state_sha256 = "e".repeat(64);
                changed.seat_behaviors.as_mut().unwrap()[0]
                    .identity
                    .state_sha256 = "e".repeat(64);
            } else if kind == "opponent_identity" {
                changed.seat_behaviors.as_mut().unwrap()[1]
                    .identity
                    .state_sha256 = "d".repeat(64);
            } else if kind == "opponent_value" {
                changed.decisions[1].value =
                    (f32::from_bits(changed.decisions[1].value) + 0.125).to_bits();
            }
            let result = prepare_with_loader_v1(
                std::slice::from_ref(&changed),
                &policy,
                &learner,
                workers,
                |_| {
                    if kind == "missing_opponent" {
                        return Err("deliberate missing frozen source".into());
                    }
                    Ok(LoadedOpponentV1 {
                        policy: opponent.fork_for_collection_v3().unwrap(),
                        behavior: other.clone().unwrap(),
                    })
                },
            );
            let error = result.err().unwrap();
            let expected = match kind {
                "stale_learner" => "stale trajectory",
                "opponent_identity" => "actual opponent behavior identity",
                "opponent_value" => "stored tensor does not reproduce",
                _ => "deliberate missing frozen source",
            };
            assert!(error.contains(expected), "{kind}: {error}");
            assert_eq!(policy.actual_model_identity_v1(), before);
        }
    }
}

#[test]
fn phase1_preparation_reports_earliest_group_error_and_joins_every_worker() {
    let barrier = Barrier::new(4);
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let result = ordered_jobs_v1(4, 100, |index| {
        let count = active.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(count, Ordering::SeqCst);
        if index < 4 {
            barrier.wait();
        }
        // Group 3 fails immediately, while earlier group 1 still has work.
        if index == 1 {
            for _ in 0..1000 {
                thread::yield_now();
            }
        }
        active.fetch_sub(1, Ordering::SeqCst);
        if index == 1 || index == 3 {
            Err(format!("bad-{index}"))
        } else {
            Ok(index)
        }
    });
    assert_eq!(result.unwrap_err(), "preparation group 1: bad-1");
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(peak.load(Ordering::SeqCst), 4);
    let panic_result = ordered_jobs_v1(4, 20, |index| {
        assert_ne!(index, 2, "injected preparation panic");
        Ok(index)
    });
    assert_eq!(
        panic_result.unwrap_err(),
        "preparation group 2: preparation worker panicked"
    );
}

#[test]
fn phase1_preparation_limits_reject_before_dispatch_or_opponent_load() {
    for workers in [0, 33] {
        assert!(ordered_jobs_v1(workers, 1, |_| -> Result<(), String> {
            panic!("must not dispatch")
        })
        .is_err());
    }
    assert!(
        ordered_jobs_v1(1, MAX_GROUPS + 1, |_| -> Result<(), String> {
            panic!("must not dispatch")
        })
        .is_err()
    );
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let mut opponent = FrozenPlayPolicyV1::training_fixture_v3();
    let (template, learner, _) = replay_fixture(&mut policy, Some(&mut opponent), 0, &[0]);
    let episodes: Vec<_> = (0..33)
        .map(|index| {
            let mut t = template.clone();
            t.episode.id = format!("many-opponents-{index}");
            let pin = format!("{index:064x}");
            t.episode.opponent.as_mut().unwrap().play_import.sha256 = pin.clone();
            t.seat_behaviors.as_mut().unwrap()[1]
                .source
                .play_import
                .sha256 = pin;
            t
        })
        .collect();
    let result = prepare_with_loader_v1(&episodes, &policy, &learner, 4, |_| {
        panic!("opponent bound must be checked before loading")
    });
    assert!(result.err().unwrap().contains("32 distinct opponents"));
    let mut oversized = template;
    // Actual vector payload, not a mocked limit: no forward or source load may
    // begin when one decoded tensor already exceeds the preparation budget.
    oversized.decisions[0].tensor.state = vec![0; MAX_DECODED_TENSOR_BYTES / 4 + 1];
    let result = prepare_with_loader_v1(
        std::slice::from_ref(&oversized),
        &policy,
        &learner,
        4,
        |_| panic!("tensor bound must be checked before loading"),
    );
    assert!(result.err().unwrap().contains("256 MiB decoded tensor"));
}

#[test]
fn phase1_preparation_preserves_legacy_command_wire_and_models_are_sync() {
    fn require_sync<T: Sync>() {}
    require_sync::<FrozenPlayPolicyV1>();
    require_sync::<ExpandedTrajectoryV1>();
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let (_, behavior, _) = replay_fixture(&mut policy, None, 0, &[0]);
    let command = ExpandedTrainingCommandV1::Update {
        source: behavior.source.clone(),
        trajectories: vec![],
        learning_rate: LR,
        value_coefficient: VC,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        output_directory: PathBuf::from("legacy-output"),
    };
    let value = serde_json::to_value(&command).unwrap();
    assert_eq!(value["mode"], "update");
    assert!(value.get("preparation_workers").is_none());
    assert!(value.get("update_backend").is_none());
    assert!(value.get("update_backward_execution").is_none());
    let bytes = serde_json::to_vec(&command).unwrap();
    assert_eq!(
        bytes,
        serde_json::to_vec(&serde_json::from_slice::<ExpandedTrainingCommandV1>(&bytes).unwrap())
            .unwrap()
    );
    let explicit = ExpandedTrainingCommandV1::UpdatePrepared {
        source: behavior.source,
        trajectories: vec![],
        learning_rate: LR,
        value_coefficient: VC,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        preparation_workers: 4,
        output_directory: PathBuf::from("prepared-output"),
    };
    let value = serde_json::to_value(explicit).unwrap();
    assert_eq!(value["mode"], "update_prepared");
    assert_eq!(value["preparation_workers"], 4);
    assert!(value.get("update_backend").is_none());
    assert!(value.get("update_backward_execution").is_none());
}
