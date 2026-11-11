use super::*;
use crate::expanded_deck_training_v1::tests::replay_fixture;
use crate::sideboard_play_policy_v1::FrozenPlayDecisionScoresV1;
use std::sync::Barrier;

const LR: f32 = 0.00003;
const VC: f32 = 0.75;

fn train_groups(state: &mut NativePolicyValueTrainStateV1, groups: &[LearnerTensorGroupV1<'_>]) {
    let substeps: Vec<Vec<NativePolicySubstepV1<'_>>> = groups
        .iter()
        .map(|(_, _, rows)| {
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
        .map(
            |(substeps, (reward, _, _))| NativePolicyPhysicalDecisionV1 {
                substeps,
                terminal_return: *reward,
                baseline_bits: 0,
            },
        )
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
        .map(|(reward, episode_ordinal, rows)| {
            (
                *reward,
                *episode_ordinal,
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
    for (workers, row_cache) in [
        (1, ReplayRowCacheV1::Off),
        (4, ReplayRowCacheV1::Off),
        (1, ReplayRowCacheV1::LearnerRows),
        (4, ReplayRowCacheV1::LearnerRows),
        (10, ReplayRowCacheV1::LearnerRows),
    ] {
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
            for (episode_ordinal, episode) in episodes.iter().enumerate() {
                validate_actual_behaviors_v1(episode, &learner, other.as_ref()).unwrap();
                reference.extend(
                    replay_learner_groups_v1(
                        episode_ordinal,
                        episode,
                        &serial_policy,
                        Some(&opponent),
                    )
                    .unwrap(),
                );
            }
            let mut loads = 0;
            let prepared = prepare_with_loader_v1(
                &episodes,
                &parallel_policy,
                &learner,
                workers,
                DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
                row_cache,
                |source| {
                    loads += 1;
                    assert_eq!(source, &other.as_ref().unwrap().source);
                    Ok(LoadedOpponentV1 {
                        policy: opponent.fork_for_collection_v3().unwrap(),
                        behavior: other.clone().unwrap(),
                    })
                },
            )
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

/// Task 2 (`TRAINING-SIGNAL-DESIGN-001.md` section 5): both preparation
/// paths compute an episode index while grouping physical decisions and,
/// before this change, discarded it once the per-episode lists were
/// flattened. Three synthetic episodes of different learner-group counts
/// (1, 2, 1) prove the recovered `episode_ordinal` on every returned group
/// exactly reconstructs the input partition, for both the parallel path
/// (`prepare_with_loader_v1`) and the sequential path
/// (`replay_learner_groups_v1`): the parallel-vs-sequential parity style of
/// `phase1_preparation_real_updates_preserve_all_state_bits_and_group_order`
/// (`group_bytes` equality, which now also covers `episode_ordinal` since
/// `group_bytes` was extended to serialize it) extended to episode
/// boundaries specifically, checked directly against the input rather than
/// only against each other.
#[test]
fn phase1_preparation_recovers_episode_boundaries_matching_input_for_both_paths() {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let actor_lists: [&[u8]; 3] = [&[0], &[0, 1, 0, 1], &[0, 1]];
    let expected_group_counts: Vec<usize> = actor_lists
        .iter()
        .map(|actors| actors.iter().filter(|&&actor| actor == 0).count())
        .collect();
    let mut episodes = Vec::new();
    let mut learner = None;
    for (index, actors) in actor_lists.iter().enumerate() {
        let (mut t, behavior, _) = replay_fixture(&mut policy, None, 0, actors);
        t.episode.id = format!("episode-boundary-{index}");
        episodes.push(t);
        learner = Some(behavior);
    }
    let learner = learner.unwrap();
    let sequential: Vec<_> = episodes
        .iter()
        .enumerate()
        .flat_map(|(episode_ordinal, episode)| {
            replay_learner_groups_v1(episode_ordinal, episode, &policy, None).unwrap()
        })
        .collect();
    let prepared = prepare_with_loader_v1(
        &episodes,
        &policy,
        &learner,
        4,
        DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        ReplayRowCacheV1::LearnerRows,
        |_| panic!("no opponent expected in this fixture"),
    )
    .unwrap();
    let expected_ordinals: Vec<usize> = expected_group_counts
        .iter()
        .enumerate()
        .flat_map(|(episode_ordinal, count)| std::iter::repeat_n(episode_ordinal, *count))
        .collect();
    for (label, groups) in [("sequential", &sequential), ("parallel", &prepared.groups)] {
        let actual_ordinals: Vec<usize> = groups.iter().map(|(_, ordinal, _)| *ordinal).collect();
        assert_eq!(
            actual_ordinals, expected_ordinals,
            "{label} episode boundaries"
        );
    }
    assert_eq!(group_bytes(&sequential), group_bytes(&prepared.groups));
}

#[test]
fn phase1_preparation_current_model_reuse_avoids_disk_load_and_keeps_rng_outputs() {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let (mut t, learner, _) = replay_fixture(&mut policy, None, 0, &[0, 1, 0, 1]);
    t.schema = POPULATION_TRAJECTORY_SCHEMA.into();
    t.episode.opponent = Some(learner.source.clone());
    t.seat_behaviors = Some([learner.clone(), learner.clone()]);
    validate_trajectory(&t).unwrap();
    let expected = replay_learner_groups_v1(0, &t, &policy, Some(&policy)).unwrap();
    let prepared = prepare_with_loader_v1(
        std::slice::from_ref(&t),
        &policy,
        &learner,
        4,
        DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        ReplayRowCacheV1::LearnerRows,
        |_| panic!("current source should reuse the already validated learner"),
    )
    .unwrap();
    assert_eq!(prepared.telemetry.opponent_load_calls, 0);
    assert_eq!(prepared.telemetry.current_model_reuses, 1);
    assert_eq!(group_bytes(&expected), group_bytes(&prepared.groups));
}

/// A deferring preparation (the bounded verified update's) decodes the
/// learner's rows without replaying them: the reference's groups, whatever a
/// learner row's stored outputs, while every other row is replayed, counted
/// and checked as before, with the reference's error.
#[test]
fn deferring_preparation_replays_and_counts_only_the_other_rows() {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let (mut t, learner, _) = replay_fixture(&mut policy, None, 0, &[0, 1, 0, 1, 1, 0]);
    t.schema = POPULATION_TRAJECTORY_SCHEMA.into();
    t.episode.opponent = Some(learner.source.clone());
    t.seat_behaviors = Some([learner.clone(), learner.clone()]);
    validate_trajectory(&t).unwrap();
    assert_eq!(t.episode.learner_seat, 0);
    let reference = |t: &ExpandedTrajectoryV1| {
        prepare_with_loader_v1(
            std::slice::from_ref(t),
            &policy,
            &learner,
            2,
            DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
            ReplayRowCacheV1::Off,
            |_| panic!("the current source reuses the learner"),
        )
        .map(|prepared| (group_bytes(&prepared.groups), prepared.telemetry))
    };
    let deferred = |t: &ExpandedTrajectoryV1, workers| {
        let other_forwards = AtomicU64::new(0);
        let prepared = prepare_counting_v1(
            std::slice::from_ref(t),
            &policy,
            &learner,
            workers,
            DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
            LearnerRowsV1::Defer {
                other_forwards: &other_forwards,
            },
            |_| panic!("the current source reuses the learner"),
        )
        .map(|prepared| (group_bytes(&prepared.groups), prepared.telemetry));
        (prepared, other_forwards.load(Ordering::Relaxed))
    };
    let (groups, telemetry) = reference(&t).unwrap();
    assert_eq!(telemetry.deferred_learner_rows, None);
    for workers in [1, 3] {
        let (prepared, other_forwards) = deferred(&t, workers);
        let (deferred_groups, telemetry) = prepared.unwrap();
        assert_eq!(deferred_groups, groups);
        assert_eq!(other_forwards, 3);
        assert_eq!(telemetry.deferred_learner_rows, Some(3));
        assert_eq!(telemetry.physical_group_jobs, 6);
    }

    // A learner row's stored value is the verified learner's to check.
    let mut corrupt = t.clone();
    corrupt.decisions[2].value ^= 1;
    assert!(reference(&corrupt)
        .unwrap_err()
        .contains("stored tensor does not reproduce rollout outputs"));
    let (prepared, other_forwards) = deferred(&corrupt, 2);
    assert!(prepared.is_ok());
    assert_eq!(other_forwards, 3);
    // Another row's is still the preparation's.
    let mut corrupt = t.clone();
    corrupt.decisions[3].value ^= 1;
    let error = reference(&corrupt).unwrap_err();
    assert_eq!(
        error,
        "preparation group 3: stored tensor does not reproduce rollout outputs"
    );
    let (prepared, other_forwards) = deferred(&corrupt, 1);
    assert_eq!(prepared.unwrap_err(), error);
    // One worker replays in order and stops at the corrupt row.
    assert_eq!(other_forwards, 2);
}

#[test]
fn phase1_preparation_rejects_stale_and_corrupted_opponent_before_learning() {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let mut opponent = FrozenPlayPolicyV1::training_fixture_v3();
    let (original, learner, other) =
        replay_fixture(&mut policy, Some(&mut opponent), 0, &[0, 1, 0, 1]);
    let before = policy.actual_model_identity_v1();
    for (workers, row_cache) in [
        (1, ReplayRowCacheV1::Off),
        (4, ReplayRowCacheV1::Off),
        (1, ReplayRowCacheV1::LearnerRows),
        (4, ReplayRowCacheV1::LearnerRows),
    ] {
        for kind in [
            "stale_learner",
            "opponent_identity",
            "opponent_value",
            "learner_logit",
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
            } else if kind == "learner_logit" {
                // The lowest bit of one stored learner logit.
                changed.decisions[0].logits[0] ^= 1;
            }
            let result = prepare_with_loader_v1(
                std::slice::from_ref(&changed),
                &policy,
                &learner,
                workers,
                DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
                row_cache,
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
                "opponent_value" | "learner_logit" => "stored tensor does not reproduce",
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
fn phase1_preparation_worker_state_is_private_and_kept_across_a_workers_jobs() {
    let made = AtomicUsize::new(0);
    let (results, timings) = ordered_jobs_with_state_v1(
        4,
        200,
        || {
            made.fetch_add(1, Ordering::SeqCst);
            (thread::current().id(), 0usize)
        },
        |state, ordinal| {
            // Made on this worker's own thread and seen by no other.
            assert_eq!(state.0, thread::current().id());
            state.1 += 1;
            Ok((ordinal, state.0, state.1))
        },
    )
    .unwrap();
    assert_eq!(made.load(Ordering::SeqCst), 4);
    // A worker takes its jobs in ordinal order, so in ordinal order each
    // worker's state has counted exactly its own earlier jobs.
    let mut counted = std::collections::HashMap::new();
    for (index, (ordinal, worker, count)) in results.into_iter().enumerate() {
        assert_eq!(ordinal, index);
        let earlier = counted.entry(worker).or_insert(0usize);
        *earlier += 1;
        assert_eq!(count, *earlier);
    }
    assert!(counted.len() <= 4);
    assert_eq!(counted.values().sum::<usize>(), 200);
    assert_eq!(
        timings.iter().map(|t| t.completed_groups).sum::<usize>(),
        200
    );
}

/// The row-cached replay forward (`score_replay_tensor_ref_v1` with a
/// worker's storage) against the reference replay
/// (`score_replay_tensor_v1`): the same bits for every stored row, the same
/// errors, and no row of another model or of replaced parameters.
#[test]
fn phase1_preparation_row_cached_replay_matches_the_reference_replay() {
    const V3: FreshLineageGenerationV1 = FreshLineageGenerationV1::V3;
    let mut learner = FrozenPlayPolicyV1::training_fixture_v3();
    let mut opponent = FrozenPlayPolicyV1::training_fixture_v3();
    let mut parameters = opponent.training_parameters_v3();
    for name in ["object_encoder.0.bias", "action_encoder.2.bias"] {
        parameters
            .iter_mut()
            .find(|p| p.name == name)
            .unwrap()
            .values[0] += 0.25;
    }
    opponent
        .replace_training_parameters_v3(&parameters)
        .unwrap();
    let actors: Vec<u8> = [0, 1, 1, 0, 1, 0, 0, 1].repeat(4);
    let (trajectory, _, _) = replay_fixture(&mut learner, Some(&mut opponent), 0, &actors);
    let same = |a: &FrozenPlayDecisionScoresV1, b: &FrozenPlayDecisionScoresV1| {
        bits(&a.logits) == bits(&b.logits) && a.value.to_bits() == b.value.to_bits()
    };
    let served = |rows: &ReplayForwardRowsV1| -> u64 {
        rows.counts_v1().iter().map(|(served, _)| served).sum()
    };
    let computed = |rows: &ReplayForwardRowsV1| -> u64 {
        rows.counts_v1().iter().map(|(_, computed)| computed).sum()
    };

    // Each seat's rows through that seat's own storage, twice over.
    let mut storage = [
        ReplayForwardRowsV1::default(),
        ReplayForwardRowsV1::default(),
    ];
    for _ in 0..2 {
        for row in &trajectory.decisions {
            let acting = if row.actor == 0 { &learner } else { &opponent };
            let common = row.tensor.tensor();
            let reference = acting
                .score_replay_tensor_v1(V3, common.clone(), &[])
                .unwrap();
            let cached = acting
                .score_replay_tensor_ref_v1(
                    V3,
                    &common,
                    &[],
                    Some(&mut storage[row.actor as usize]),
                )
                .unwrap();
            assert!(same(&cached, &reference));
            assert_eq!(bits(&cached.logits), row.logits);
            assert_eq!(cached.value.to_bits(), row.value);
            // Without storage it is the reference forward.
            let borrowed = acting
                .score_replay_tensor_ref_v1(V3, &common, &[], None)
                .unwrap();
            assert!(same(&borrowed, &reference));
        }
    }
    for rows in &storage {
        assert!(served(rows) > 0 && computed(rows) > 0);
    }

    // One storage scored by two models in turn serves neither the other's
    // rows: the models differ, and so do their outputs for a tensor.
    let mut rows = ReplayForwardRowsV1::default();
    for row in &trajectory.decisions {
        let common = row.tensor.tensor();
        let mut outputs = Vec::new();
        for acting in [&learner, &opponent, &learner] {
            let reference = acting
                .score_replay_tensor_v1(V3, common.clone(), &[])
                .unwrap();
            let cached = acting
                .score_replay_tensor_ref_v1(V3, &common, &[], Some(&mut rows))
                .unwrap();
            assert!(same(&cached, &reference));
            outputs.push(cached);
        }
        assert!(!same(&outputs[0], &outputs[1]));
        assert!(same(&outputs[0], &outputs[2]));
    }

    // Refused inputs are refused with the reference's error, and leave the
    // storage exact.
    let row = &trajectory.decisions[0];
    let common = row.tensor.tensor();
    let mut truncated = common.clone();
    truncated.object_features.pop();
    let mut nonfinite = common.clone();
    nonfinite.state[0] = f32::NAN;
    for bad in [&truncated, &nonfinite] {
        assert_eq!(
            learner
                .score_replay_tensor_ref_v1(V3, bad, &[], Some(&mut rows))
                .unwrap_err(),
            learner
                .score_replay_tensor_v1(V3, bad.clone(), &[])
                .unwrap_err()
        );
    }
    assert_eq!(
        learner
            .score_replay_tensor_ref_v1(V3, &common, &[0, 1], Some(&mut rows))
            .unwrap_err(),
        learner
            .score_replay_tensor_v1(V3, common.clone(), &[0, 1])
            .unwrap_err()
    );
    let before = learner
        .score_replay_tensor_ref_v1(V3, &common, &[], Some(&mut rows))
        .unwrap();
    assert_eq!(bits(&before.logits), row.logits);
    assert_eq!(before.value.to_bits(), row.value);
    let again = learner
        .score_replay_tensor_ref_v1(V3, &common, &[], Some(&mut rows))
        .unwrap();
    assert!(same(&again, &before));
    assert!(served(&rows) > 0);

    // Replaced parameters never meet rows of the old ones, although this
    // storage still holds the old model.
    let mut parameters = learner.training_parameters_v3();
    parameters
        .iter_mut()
        .find(|p| p.name == "object_encoder.0.bias")
        .unwrap()
        .values[0] += 0.5;
    learner.replace_training_parameters_v3(&parameters).unwrap();
    let reference = learner
        .score_replay_tensor_v1(V3, common.clone(), &[])
        .unwrap();
    let cached = learner
        .score_replay_tensor_ref_v1(V3, &common, &[], Some(&mut rows))
        .unwrap();
    assert!(same(&cached, &reference));
    assert!(!same(&cached, &before));
    // The storage started over for the new parameters.
    assert_eq!(served(&rows), 0);
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
    let result = prepare_with_loader_v1(
        &episodes,
        &policy,
        &learner,
        4,
        DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        ReplayRowCacheV1::LearnerRows,
        |_| panic!("opponent bound must be checked before loading"),
    );
    assert!(result.err().unwrap().contains("32 distinct opponents"));
    let mut oversized = template;
    // Actual vector payload, not a mocked limit: no forward or source load may
    // begin when one decoded tensor already exceeds the preparation budget.
    oversized.decisions[0].tensor.state =
        vec![0; DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES * MEBIBYTE / 4 + 1];
    let result = prepare_with_loader_v1(
        std::slice::from_ref(&oversized),
        &policy,
        &learner,
        4,
        DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        ReplayRowCacheV1::LearnerRows,
        |_| panic!("tensor bound must be checked before loading"),
    );
    assert!(result.err().unwrap().contains("256 MiB decoded tensor"));
    // The bound is the caller's, not a constant: a smaller configured bound
    // rejects a smaller payload and the message names the actual bound.
    let mut small = oversized.clone();
    small.decisions[0].tensor.state = vec![0; MIN_PREPARED_TENSOR_MEBIBYTES * MEBIBYTE / 4 + 1];
    let result = prepare_with_loader_v1(
        std::slice::from_ref(&small),
        &policy,
        &learner,
        4,
        MIN_PREPARED_TENSOR_MEBIBYTES,
        ReplayRowCacheV1::LearnerRows,
        |_| panic!("tensor bound must be checked before loading"),
    );
    assert!(result.err().unwrap().contains("64 MiB decoded tensor"));
    assert!(validate_max_prepared_tensor_mebibytes_v1(63).is_err());
    assert!(validate_max_prepared_tensor_mebibytes_v1(4097).is_err());
    assert!(validate_max_prepared_tensor_mebibytes_v1(64).is_ok());
    assert!(validate_max_prepared_tensor_mebibytes_v1(4096).is_ok());
}

#[test]
fn phase1_preparation_preserves_legacy_command_wire_and_models_are_sync() {
    fn require_sync<T: Sync>() {}
    require_sync::<FrozenPlayPolicyV1>();
    require_sync::<ExpandedTrajectoryV1>();
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let (_, behavior, _) = replay_fixture(&mut policy, None, 0, &[0]);
    let command = ExpandedTrainingCommandV1::Update {
        line_b: None,
        source: behavior.source.clone(),
        trajectories: vec![],
        learning_rate: LR,
        value_coefficient: VC,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: ExpandedLossSelectionV1::default(),
        training_math: TrainingMathV1::PlatformLibm,
        record_format: crate::expanded_deck_training_v1::CheckpointRecordFormatV1::V1,
        output_directory: PathBuf::from("legacy-output"),
    };
    let value = serde_json::to_value(&command).unwrap();
    assert_eq!(value["mode"], "update");
    assert!(value.get("preparation_workers").is_none());
    assert!(value.get("update_backend").is_none());
    assert!(value.get("update_backward_execution").is_none());
    assert!(value.get("loss_selection").is_none());
    let bytes = serde_json::to_vec(&command).unwrap();
    assert_eq!(
        bytes,
        serde_json::to_vec(&serde_json::from_slice::<ExpandedTrainingCommandV1>(&bytes).unwrap())
            .unwrap()
    );
    let explicit = ExpandedTrainingCommandV1::UpdatePrepared {
        line_b: None,
        source: behavior.source,
        trajectories: vec![],
        learning_rate: LR,
        value_coefficient: VC,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: ExpandedLossSelectionV1::default(),
        preparation_workers: 4,
        max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        training_math: TrainingMathV1::PlatformLibm,
        record_format: crate::expanded_deck_training_v1::CheckpointRecordFormatV1::V1,
        record_envelope: Default::default(),
        output_directory: PathBuf::from("prepared-output"),
    };
    let value = serde_json::to_value(explicit).unwrap();
    assert_eq!(value["mode"], "update_prepared");
    assert_eq!(value["preparation_workers"], 4);
    assert!(value.get("loss_selection").is_none());
    assert!(value.get("update_backend").is_none());
    assert!(value.get("update_backward_execution").is_none());
    assert!(value.get("max_prepared_tensor_mebibytes").is_none());
}

/// The exact JSON shape a compiled block config must carry to select
/// `gae_advantage_value/v1` on `Update`/`UpdatePrepared`: a `loss_selection`
/// object, sibling to `update_backend`, with `"kind":
/// "gae_advantage_value_v1"` plus the three scalar fields. Pins the tagged
/// enum's serde rendering precisely (verified here, not just predicted from
/// the crate's other `snake_case` variants), since the lead patches configs
/// from this shape directly.
#[test]
fn phase1_gae_loss_selection_wire_shape_is_exact() {
    let selection = ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.9,
        entropy_coefficient: 0.0,
    };
    let value = serde_json::to_value(selection).unwrap();
    assert_eq!(
        value,
        // `0.9_f32 as f64` (not the `0.9` f64 literal): `float_roundtrip`
        // preserves the f32 bit pattern exactly, and 0.9 is not exactly
        // representable in either width, so the two widths' nearest values
        // differ past the 7th digit.
        serde_json::json!({
            "kind": "gae_advantage_value_v1",
            "gamma": 1.0,
            "lambda": 0.9_f32 as f64,
            "entropy_coefficient": 0.0
        })
    );
    let round_tripped: ExpandedLossSelectionV1 = serde_json::from_value(value).unwrap();
    assert_eq!(round_tripped, selection);
}

/// A batch whose sealed files declare more than the bound together is
/// refused with the batch-bound error before any file is opened, whatever the
/// worker count: the files below verify against their pins but never open.
#[test]
fn sealed_batches_are_admitted_before_any_file_is_opened() {
    let directory = std::env::temp_dir().join(format!(
        "ordered-admit-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&directory).unwrap();
    // A frame header declaring 300 MiB and one empty last raw block.
    let mut frame = vec![0x28, 0xb5, 0x2f, 0xfd, 0xe0];
    frame.extend_from_slice(&(300u64 << 20).to_le_bytes());
    frame.extend_from_slice(&[0x01, 0x00, 0x00]);
    let pins: Vec<PinnedFileV1> = (0..2)
        .map(|index| {
            let path = directory.join(format!("episode-{index:04}.json"));
            fs::write(&path, &frame).unwrap();
            PinnedFileV1 {
                path,
                sha256: sha(&frame),
            }
        })
        .collect();
    for workers in [1, 2] {
        assert_eq!(
            read_trajectories_v1(&pins, workers).unwrap_err(),
            "trajectory batch exceeds 512 MiB CPU ingestion bound"
        );
    }
    // One such file alone is admitted, verified, and fails only when opened.
    let error = read_trajectories_v1(&pins[..1], 1).unwrap_err();
    assert_ne!(
        error,
        "trajectory batch exceeds 512 MiB CPU ingestion bound"
    );
    fs::remove_dir_all(&directory).unwrap();
}
