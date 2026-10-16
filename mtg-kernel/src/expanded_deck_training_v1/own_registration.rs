//! Ordinary V4 collection with explicit selected-mainboard context per actor.
//! CPU collection and replay are available without the CUDA training backend.
use super::*;
use crate::own_registration_v1::{
    KnownOwnMainboardV1, RegistrationExpectationV1, RegistrationFeatureRowsV1, RegistrationModeV1,
};
use crate::paired_bo1_harness_v1::PairedBo1PolicyInputV1;
use crate::sideboard_play_policy_v1::own_registration::OwnRegistrationPlayPolicyV1;

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
pub mod evaluation;
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
pub mod training;

const SCHEMA: &str = "mtg-kernel-own-registration-trajectory/v1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Trajectory {
    schema: String,
    config_sha256: String,
    optimizer_state_sha256: String,
    mode: RegistrationModeV1,
    projection_enabled: bool,
    episode: ExpandedEpisodeV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    opponent: Option<ExpandedInferenceIdentityV1>,
    configuration_sha256: [String; 2],
    decisions: Vec<DecisionRecordV1>,
    auxiliary: Vec<Option<RegistrationFeatureRowsV1>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    learner_sampler: Option<String>,
    terminal: RlSessionTerminalV1,
}

fn actor_seat(actor: u8) -> Result<PlayerSeatV1, String> {
    match actor {
        0 => Ok(PlayerSeatV1::P0),
        1 => Ok(PlayerSeatV1::P1),
        _ => Err("registration actor outside physical seats".into()),
    }
}

fn expectation<'a>(
    episode: &'a ExpandedEpisodeV1,
    configs: &[DeckConfigurationV1; 2],
    actor: u8,
) -> Result<RegistrationExpectationV1<'a>, String> {
    Ok(RegistrationExpectationV1 {
        external_episode_id: &episode.id,
        actor: actor_seat(actor)?,
        selected_mainboard_sha256: configs[usize::from(actor)].mainboard_sha256_v1(),
    })
}

impl Trajectory {
    pub(super) fn validate_records(&self) -> Result<(), String> {
        ensure(
            self.schema == SCHEMA,
            "registration trajectory schema differs",
        )?;
        ensure(
            self.projection_enabled || self.mode == RegistrationModeV1::Own,
            "legacy continuation requires own-mode metadata",
        )?;
        ensure(
            self.opponent.is_some() == self.episode.opponent.is_some(),
            "registration opponent identity presence differs",
        )?;
        validate_episode_records_with_learner_sampler_v1(
            &self.episode,
            &self.configuration_sha256,
            &self.decisions,
            &self.terminal,
            self.learner_sampler.as_deref(),
        )?;
        let configs = self.episode.configurations()?;
        ensure(
            self.auxiliary.len() == self.decisions.len(),
            "registration sidecar length differs from decision records",
        )?;
        for (row, auxiliary) in self.decisions.iter().zip(&self.auxiliary) {
            let own = self.episode.opponent.is_none() || row.actor == self.episode.learner_seat;
            match (own, auxiliary) {
                (true, Some(rows)) => rows.validate_expected(
                    self.mode,
                    expectation(&self.episode, &configs, row.actor)?,
                )?,
                (false, None) => {}
                (true, None) => return Err("own-policy decision lacks registration sidecar".into()),
                (false, Some(_)) => {
                    return Err("fixed-opponent decision carries own-policy sidecar".into())
                }
            }
        }
        Ok(())
    }
}

fn collect(
    policy: &mut OwnRegistrationPlayPolicyV1,
    episode: &ExpandedEpisodeV1,
    config_sha256: &str,
    optimizer_sha256: &str,
    mode: RegistrationModeV1,
    projection_enabled: bool,
) -> Result<Trajectory, String> {
    let configs = episode.configurations()?;
    // A single immutable selected configuration supplies each actor's context
    // for the entire game, including reset suffixes and postboard games.
    let contexts = [
        KnownOwnMainboardV1::new(&episode.id, PlayerSeatV1::P0, &configs[0])?,
        KnownOwnMainboardV1::new(&episode.id, PlayerSeatV1::P1, &configs[1])?,
    ];
    let mut opponent = episode
        .opponent
        .as_ref()
        .map(|source| load_expanded_inference_bound_v1(source, policy.training_math_v1()))
        .transpose()?;
    if let Some((net, _)) = &opponent {
        ensure(
            net.feature_identity_v1() == policy.feature_identity_v1()
                && net.feature_identity_v1().generation == FreshLineageGenerationV1::V4,
            "registration collection requires the same exact V4 opponent feature identity",
        )?;
    }
    let mut session = episode_session_v1(episode, &configs)?;
    restore_reset_cut_v1(&mut session, episode)?;
    let seeds = paired_policy_seeds_v1(episode.seed);
    policy.reset_for_game(seeds).map_err(err)?;
    if let Some((net, _)) = &mut opponent {
        net.reset_sampling_v1(seeds);
    }
    let mut decisions = Vec::new();
    let mut auxiliary = Vec::new();
    loop {
        match session.current_response() {
            FastActorResponseV1::Terminal(terminal) => {
                ensure(
                    terminal.terminal_classification == TerminalClassificationV1::Natural
                        && terminal.terminal_code == TerminalSafeCodeV2::NaturalGameOver,
                    &format!(
                        "registration episode {} failed: classification={:?}, code={:?}, policy_steps={}, physical_decisions={}, reason={}",
                        episode.id, terminal.terminal_classification, terminal.terminal_code,
                        terminal.policy_step_count, terminal.physical_decision_count,
                        terminal.terminal_reason,
                    ),
                )?;
                let result = Trajectory {
                    schema: SCHEMA.into(),
                    config_sha256: config_sha256.into(),
                    optimizer_state_sha256: optimizer_sha256.into(),
                    mode,
                    projection_enabled,
                    episode: episode.clone(),
                    opponent: opponent.map(|(_, identity)| identity),
                    configuration_sha256: configs.each_ref().map(|c| hex(&c.mainboard_sha256_v1())),
                    decisions,
                    auxiliary,
                    learner_sampler: policy.collection_sampler_identity_v1().map(str::to_owned),
                    terminal,
                };
                result.validate_records()?;
                return Ok(result);
            }
            FastActorResponseV1::Decision(d) => {
                ensure(
                    decisions.len() < episode.max_policy_steps as usize,
                    &format!(
                        "registration episode {} exceeds suffix policy step limit",
                        episode.id
                    ),
                )?;
                let actor = seat(d.acting_player);
                let own = opponent.is_none() || actor == episode.learner_seat;
                let (selected, scores, tensor, sampler_identity) = if own {
                    let input = PairedBo1PolicyInputV1::new(&session, d);
                    let (selected, scores) = policy
                        .select_with_scores(
                            &input,
                            &contexts[usize::from(actor)],
                            expectation(episode, &configs, actor)?,
                        )
                        .map_err(err)?;
                    let (tensor, rows) = policy.captured()?;
                    auxiliary.push(Some(rows.clone()));
                    (
                        selected,
                        scores,
                        TensorBitsV1::from_tensor(&tensor.common),
                        policy.collection_sampler_identity_v1(),
                    )
                } else {
                    let net = &mut opponent.as_mut().ok_or("missing fixed opponent")?.0;
                    let (selected, scores, tensor) =
                        net.select_with_training_tensor_v4(&session)?;
                    auxiliary.push(None);
                    (
                        selected,
                        scores,
                        TensorBitsV1::from_tensor(&tensor.common),
                        None,
                    )
                };
                let record = DecisionRecordV1 {
                    step: d.step,
                    physical_decision_id: d.physical_decision_id,
                    substep_index: d.substep_index,
                    substep_count: d.substep_count,
                    actor,
                    selected,
                    logits: bits(&scores.logits),
                    value: scores.value.to_bits(),
                    tensor,
                    sampler_identity: sampler_identity
                        .or_else(|| decision_sampler_identity_v1(scores.logits.len()))
                        .map(str::to_owned),
                };
                session.step(d.episode_id, d.step, selected).map_err(err)?;
                decisions.push(record);
            }
        }
    }
}

type LearnerRegistrationGroupV1<'a> = Vec<(
    &'a DecisionRecordV1,
    NativeFlatDecisionTensorV4,
    &'a RegistrationFeatureRowsV1,
)>;

/// Only the declared learner's physical decisions enter a training update,
/// even when both physical seats used the same registration-aware policy.
pub(super) fn learner_groups(
    trajectory: &Trajectory,
) -> Result<Vec<LearnerRegistrationGroupV1<'_>>, String> {
    trajectory.validate_records()?;
    let mut groups = Vec::new();
    let mut index = 0;
    while index < trajectory.decisions.len() {
        let first = &trajectory.decisions[index];
        let end = index + first.substep_count as usize;
        if first.actor == trajectory.episode.learner_seat {
            let mut group = Vec::new();
            for i in index..end {
                let row = &trajectory.decisions[i];
                let auxiliary = trajectory.auxiliary[i]
                    .as_ref()
                    .ok_or("missing learner registration row")?;
                group.push((
                    row,
                    NativeFlatDecisionTensorV4 {
                        common: row.tensor.tensor(),
                    },
                    auxiliary,
                ));
            }
            groups.push(group);
        }
        index = end;
    }
    Ok(groups)
}

/// Indexed deterministic collection from private model/RNG forks. A failure
/// cancels new dispatch and every started worker is joined before returning.
#[allow(
    clippy::too_many_arguments,
    reason = "explicit collection identity and placement contract"
)]
pub(super) fn collect_parallel(
    policy: &OwnRegistrationPlayPolicyV1,
    episodes: &[ExpandedEpisodeV1],
    config_sha256: &str,
    optimizer_sha256: &str,
    mode: RegistrationModeV1,
    projection_enabled: bool,
    workers: usize,
) -> Result<Vec<Trajectory>, String> {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    validate_collection_workers_v1(workers)?;
    ensure(
        !episodes.is_empty(),
        "registration collection needs episodes",
    )?;
    ensure(
        policy.mode() == mode && policy.projection_enabled() == projection_enabled,
        "registration collection metadata differs from the actual policy",
    )?;
    let mut ids = BTreeSet::new();
    for episode in episodes {
        episode.configurations()?;
        ensure(
            ids.insert(&episode.id),
            "registration collection repeats an external episode id",
        )?;
    }
    let policies = (0..workers.min(episodes.len()))
        .map(|_| policy.fork_for_collection())
        .collect::<Result<Vec<_>, _>>()?;
    let next = AtomicUsize::new(0);
    let cancelled = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        let mut failure = None;
        for (worker, mut policy) in policies.into_iter().enumerate() {
            let next = &next;
            let cancelled = &cancelled;
            match std::thread::Builder::new()
                .name(format!("registration-collector-{worker}"))
                .stack_size(16 * 1024 * 1024)
                .spawn_scoped(scope, move || {
                    let mut results = Vec::new();
                    while !cancelled.load(Ordering::Acquire) {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(episode) = episodes.get(index) else { break; };
                        if cancelled.load(Ordering::Acquire) { break; }
                        match catch_unwind(AssertUnwindSafe(|| collect(
                            &mut policy, episode, config_sha256, optimizer_sha256, mode,
                            projection_enabled,
                        ))) {
                            Ok(Ok(trajectory)) => results.push((index, trajectory)),
                            outcome => {
                                cancelled.store(true, Ordering::Release);
                                let message = match outcome {
                                    Ok(Err(error)) => error,
                                    Err(_) => "worker panicked".into(),
                                    Ok(Ok(_)) => unreachable!(),
                                };
                                return Err(format!("registration collector {worker}, episode {index} {}: {message}", episode.id));
                            }
                        }
                    }
                    Ok(results)
                }) {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    cancelled.store(true, Ordering::Release);
                    failure = Some(format!("could not start registration collector {worker}: {error}"));
                    break;
                }
            }
        }
        let mut ordered: Vec<Option<Trajectory>> = (0..episodes.len()).map(|_| None).collect();
        for handle in handles {
            match handle.join() {
                Ok(Ok(results)) => {
                    for (index, trajectory) in results {
                        if ordered[index].replace(trajectory).is_some() {
                            failure.get_or_insert_with(|| {
                                "registration collection repeated an index".into()
                            });
                        }
                    }
                }
                Ok(Err(error)) => {
                    failure.get_or_insert(error);
                }
                Err(_) => {
                    cancelled.store(true, Ordering::Release);
                    failure.get_or_insert_with(|| "registration collector thread panicked".into());
                }
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        ordered
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                row.ok_or_else(|| format!("registration collection omitted episode {index}"))
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_policy_value_net_v1::own_registration::RegistrationInputWeightsV1;

    fn deck(name: &str) -> ExpandedDeckListV1 {
        let registered =
            crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(name).unwrap();
        ExpandedDeckListV1 {
            label: name.into(),
            mainboard: registered.registered_configuration().mainboard().to_vec(),
            sideboard: registered.registered_configuration().sideboard().to_vec(),
        }
    }

    fn episode(id: &str, learner_seat: u8) -> ExpandedEpisodeV1 {
        let decks = [deck("Wildfire"), deck("Burn")];
        ExpandedEpisodeV1 {
            id: id.into(),
            seed: 12345,
            starting_player: 0,
            learner_seat,
            opponent: None,
            opponent_search: None,
            opponent_kind: None,
            registered: decks.clone(),
            selected: decks,
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
            runtime_rules: None,
            reset: None,
        }
    }

    /// Synthetic metadata/grouping only. These inherited fixture rows are
    /// never forwarded as V4 tensors and do not represent completed games.
    fn grouping_fixture(mode: RegistrationModeV1, learner_seat: u8) -> Trajectory {
        let (fixture, _, _) = super::super::tests::replay_fixture(
            &mut FrozenPlayPolicyV1::training_fixture_v3(),
            None,
            learner_seat,
            &[0, 1, 1, 0],
        );
        let configs = fixture.episode.configurations().unwrap();
        let auxiliary = fixture
            .decisions
            .iter()
            .map(|row| {
                Some(
                    KnownOwnMainboardV1::new(
                        &fixture.episode.id,
                        actor_seat(row.actor).unwrap(),
                        &configs[row.actor as usize],
                    )
                    .unwrap()
                    .rows(mode)
                    .unwrap(),
                )
            })
            .collect();
        Trajectory {
            schema: SCHEMA.into(),
            config_sha256: "0".repeat(64),
            optimizer_state_sha256: "1".repeat(64),
            mode,
            projection_enabled: true,
            episode: fixture.episode,
            opponent: None,
            configuration_sha256: fixture.configuration_sha256,
            decisions: fixture.decisions,
            auxiliary,
            learner_sampler: None,
            terminal: fixture.terminal,
        }
    }

    #[test]
    fn sidecars_bind_each_shared_selfplay_actor_and_only_learner_groups_train() {
        for mode in [RegistrationModeV1::Own, RegistrationModeV1::Mean] {
            for learner_seat in [0, 1] {
                let trajectory = grouping_fixture(mode, learner_seat);
                trajectory.validate_records().unwrap();
                let groups = learner_groups(&trajectory).unwrap();
                assert_eq!(groups.len(), 2);
                assert!(groups.iter().flatten().all(|(row, _, rows)| {
                    row.actor == learner_seat && seat(rows.actor) == learner_seat
                }));
                assert!(trajectory.auxiliary.iter().all(Option::is_some));
                let roundtrip: Trajectory =
                    serde_json::from_slice(&serde_json::to_vec(&trajectory).unwrap()).unwrap();
                roundtrip.validate_records().unwrap();
            }
        }
    }

    #[test]
    fn sidecar_validation_rejects_missing_stale_other_actor_and_wrong_selection() {
        for change in 0..5 {
            let mut trajectory = grouping_fixture(RegistrationModeV1::Own, 0);
            match change {
                0 => {
                    trajectory.auxiliary.pop();
                }
                1 => {
                    trajectory.auxiliary[1] = None;
                }
                2 => {
                    trajectory.auxiliary[1]
                        .as_mut()
                        .unwrap()
                        .external_episode_id = "stale-game".into();
                }
                3 => {
                    trajectory.auxiliary[1] = trajectory.auxiliary[0].clone();
                }
                _ => {
                    let config = deck("Burn")
                        .validated()
                        .unwrap()
                        .registered_configuration()
                        .clone();
                    trajectory.auxiliary[1] = Some(
                        KnownOwnMainboardV1::new(&trajectory.episode.id, PlayerSeatV1::P1, &config)
                            .unwrap()
                            .rows(RegistrationModeV1::Own)
                            .unwrap(),
                    );
                }
            }
            assert!(trajectory.validate_records().is_err(), "mutation {change}");
            assert!(learner_groups(&trajectory).is_err(), "mutation {change}");
        }
    }

    #[test]
    fn fixed_opponent_rows_have_no_registration_sidecars() {
        for learner_seat in [0, 1] {
            let mut trajectory = grouping_fixture(RegistrationModeV1::Own, learner_seat);
            let (_, _, opponent) = super::super::tests::replay_fixture(
                &mut FrozenPlayPolicyV1::training_fixture_v3(),
                Some(&mut FrozenPlayPolicyV1::training_fixture_v3()),
                learner_seat,
                &[0, 1],
            );
            let opponent = opponent.unwrap();
            trajectory.episode.opponent = Some(opponent.source);
            trajectory.opponent = Some(opponent.identity);
            let mut opponent_index = None;
            for (index, row) in trajectory.decisions.iter().enumerate() {
                if row.actor != learner_seat {
                    trajectory.auxiliary[index] = None;
                    opponent_index = Some(index);
                }
            }
            trajectory.validate_records().unwrap();
            assert_eq!(learner_groups(&trajectory).unwrap().len(), 2);
            let index = opponent_index.unwrap();
            let configs = trajectory.episode.configurations().unwrap();
            let actor = trajectory.decisions[index].actor;
            trajectory.auxiliary[index] = Some(
                KnownOwnMainboardV1::new(
                    &trajectory.episode.id,
                    actor_seat(actor).unwrap(),
                    &configs[actor as usize],
                )
                .unwrap()
                .rows(RegistrationModeV1::Own)
                .unwrap(),
            );
            assert!(trajectory.validate_records().is_err());
        }
    }

    #[test]
    fn inherited_replay_rejects_truncation_sampler_and_group_tampering() {
        for change in 0..3 {
            let mut trajectory = grouping_fixture(RegistrationModeV1::Own, 0);
            match change {
                0 => {
                    trajectory.terminal.terminal_classification =
                        TerminalClassificationV1::Truncated;
                }
                1 => {
                    trajectory.decisions[0].sampler_identity = Some("wrong-sampler".into());
                }
                _ => {
                    trajectory.decisions[0].substep_count = 0;
                }
            }
            assert!(trajectory.validate_records().is_err());
            assert!(learner_groups(&trajectory).is_err());
        }
    }

    #[test]
    fn context_uses_selected_postboard_and_rebinds_external_episode_and_actor() {
        let mut first = episode("postboard-a", 0);
        first.postboard = true;
        let replacement = first.selected[0].sideboard[0];
        first.selected[0].sideboard[0] = first.selected[0].mainboard[0];
        first.selected[0].mainboard[0] = replacement;
        let selected = first.configurations().unwrap();
        let registered = first.registered[0].validated().unwrap();
        assert_ne!(
            selected[0].mainboard_sha256_v1(),
            registered.registered_configuration().mainboard_sha256_v1()
        );
        let context = KnownOwnMainboardV1::new(&first.id, PlayerSeatV1::P0, &selected[0]).unwrap();
        let rows = context.rows(RegistrationModeV1::Own).unwrap();
        rows.validate_expected(
            RegistrationModeV1::Own,
            expectation(&first, &selected, 0).unwrap(),
        )
        .unwrap();
        let mut next = first.clone();
        next.id = "postboard-b".into();
        assert!(context
            .validate_expected(expectation(&next, &selected, 0).unwrap())
            .is_err());
        assert!(context
            .validate_expected(expectation(&first, &selected, 1).unwrap())
            .is_err());
        let next_context =
            KnownOwnMainboardV1::new(&next.id, PlayerSeatV1::P0, &selected[0]).unwrap();
        assert_eq!(
            rows.features,
            next_context.rows(RegistrationModeV1::Own).unwrap().features
        );
        next_context
            .validate_expected(expectation(&next, &selected, 0).unwrap())
            .unwrap();
    }

    #[test]
    fn collection_rejects_unsupported_dispatch_before_loading_opponent() {
        let policy = OwnRegistrationPlayPolicyV1::new(
            FrozenPlayPolicyV1::training_fixture_v4(),
            RegistrationInputWeightsV1::zero(),
            RegistrationModeV1::Own,
        )
        .unwrap();
        let mut invalid = episode("unsupported", 0);
        invalid.opponent_search = Some(PinnedFileV1 {
            path: "never-read.json".into(),
            sha256: "0".repeat(64),
        });
        let error = match collect_parallel(
            &policy,
            &[invalid],
            "config",
            "optimizer",
            RegistrationModeV1::Own,
            true,
            1,
        ) {
            Ok(_) => panic!("unsupported search was admitted"),
            Err(error) => error,
        };
        assert!(error.contains("search opponent"));
    }

    #[test]
    #[ignore = "root-owned native qualification: bounded collection failure"]
    fn bounded_failure_reports_episode_terminal_and_counters() {
        let policy = OwnRegistrationPlayPolicyV1::new(
            FrozenPlayPolicyV1::training_fixture_v4(),
            RegistrationInputWeightsV1::zero(),
            RegistrationModeV1::Own,
        )
        .unwrap();
        let mut bounded = episode("bounded-failure", 0);
        bounded.max_physical_decisions = 1;
        bounded.max_policy_steps = 1;
        let error = match collect_parallel(
            &policy,
            &[bounded],
            "config",
            "optimizer",
            RegistrationModeV1::Own,
            true,
            2,
        ) {
            Ok(_) => panic!("truncated collection was admitted"),
            Err(error) => error,
        };
        for field in [
            "bounded-failure",
            "classification=",
            "code=",
            "policy_steps=",
            "physical_decisions=",
        ] {
            assert!(error.contains(field), "missing {field}: {error}");
        }
    }

    #[test]
    #[ignore = "root-owned native qualification: real complete selfplay games"]
    fn serial_parallel_collection_preserves_schedule_and_exact_bytes() {
        let mut policy = OwnRegistrationPlayPolicyV1::new(
            FrozenPlayPolicyV1::training_fixture_v4(),
            RegistrationInputWeightsV1::zero(),
            RegistrationModeV1::Own,
        )
        .unwrap();
        policy.enable_unclamped_collection_sampler_v1();
        let episodes = [
            episode("serial-parallel-a", 0),
            episode("serial-parallel-b", 1),
        ];
        let serial = collect_parallel(
            &policy,
            &episodes,
            "config",
            "optimizer",
            RegistrationModeV1::Own,
            true,
            1,
        )
        .unwrap();
        let parallel = collect_parallel(
            &policy,
            &episodes,
            "config",
            "optimizer",
            RegistrationModeV1::Own,
            true,
            2,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_vec(&serial).unwrap(),
            serde_json::to_vec(&parallel).unwrap()
        );
        for trajectory in &parallel {
            assert!(trajectory.opponent.is_none());
            assert!(trajectory.auxiliary.iter().all(Option::is_some));
            assert!(trajectory.decisions.iter().any(|row| row.actor == 0));
            assert!(trajectory.decisions.iter().any(|row| row.actor == 1));
            trajectory.validate_records().unwrap();
        }
    }
}
