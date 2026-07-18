//! Paired, fixed-work comparison of the full in-process v5 session and the
//! explicitly non-wire fast-actor session.
//!
//! This is a Rust environment ceiling diagnostic. It excludes inference,
//! learning, persistence, JSONL, process IPC, Python, and XMage, so it must not
//! be presented as end-to-end training throughput or an XMage speedup claim.

use crate::{rng_below, rng_chance, DECISION_SAFETY_CAP};
use mtg_kernel::rl::{ActionSemanticV1, TerminalClassificationV1};
use mtg_kernel::rl_session::{
    FastActorDecisionKindV1, FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1,
    RlEpisodeSessionV1, RlSessionDecisionV1, RlSessionResponseV1, RlSessionTerminalV1,
    SessionDeckIdsV1,
};
use mtg_kernel::runtime_decks::{runtime_deck_by_id, RuntimeDeckDefinition};
use mtg_kernel::state::SplitMix64;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Arc, Barrier};
use std::time::Instant;

const SCHEMA_V2: &str = "kernel_rl_fast_actor_ceiling/v2";
const POLICY_V1: &str = "seeded_uniform_aggregate_combat/v1";
const PARTITION_STRIDE: u64 = 1u64 << 52;
const PARTITIONS_PER_DOMAIN: u64 = 1_024;
const WARMUP_PARTITION_OFFSET: u64 = 256;
const VALIDATION_PARTITION_OFFSET: u64 = 512;
const EPISODE_DOMAIN_OFFSET: u64 = 0;
const ENV_DOMAIN_OFFSET: u64 = PARTITIONS_PER_DOMAIN;
const POLICY_DOMAIN_OFFSET: u64 = PARTITIONS_PER_DOMAIN * 2;

#[derive(Debug)]
pub(crate) struct FastActorConfigV2 {
    git_commit: String,
    deck_id: String,
    actors: Vec<usize>,
    warmup_games: u64,
    games_per_actor: u64,
    validation_games: u64,
    seed: u64,
}

impl FastActorConfigV2 {
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        let mut git_commit = None;
        let mut deck_id = "Rally".to_string();
        let mut actors = vec![1, 4, 8, 16];
        let mut warmup_games = 1u64;
        let mut games_per_actor = 4u64;
        let mut validation_games = 2u64;
        let mut seed = 0xFA57_AC70u64;
        let mut seen = HashSet::new();
        let mut index = 0;
        while index < args.len() {
            let flag = args[index].as_str();
            if !seen.insert(flag.to_string()) {
                return Err(format!("duplicate option: {flag}"));
            }
            let value = args
                .get(index + 1)
                .ok_or_else(|| format!("missing value for {flag}"))?;
            match flag {
                "--git-commit" => {
                    if value.len() != 40
                        || !value
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    {
                        return Err(
                            "--git-commit must be 40 lowercase hexadecimal characters".into()
                        );
                    }
                    git_commit = Some(value.clone());
                }
                "--deck" => {
                    if !matches!(value.as_str(), "Burn" | "Rally") {
                        return Err("--deck must be exact Burn or Rally".into());
                    }
                    deck_id = value.clone();
                }
                "--actors" => {
                    let parsed = value
                        .split(',')
                        .map(|part| {
                            part.parse::<usize>()
                                .map_err(|_| "--actors must be a comma-separated integer list")
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    if parsed.is_empty()
                        || parsed.iter().any(|count| !(1..=256).contains(count))
                        || parsed.iter().copied().collect::<HashSet<_>>().len() != parsed.len()
                    {
                        return Err(
                            "--actors must contain unique integers in the range 1..=256".into()
                        );
                    }
                    actors = parsed;
                }
                "--warmup-games" => {
                    warmup_games = parse_bounded_u64(value, flag, 0, 10_000)?;
                }
                "--games-per-actor" => {
                    games_per_actor = parse_bounded_u64(value, flag, 1, 100_000)?;
                }
                "--validation-games" => {
                    validation_games = parse_bounded_u64(value, flag, 1, 64)?;
                }
                "--seed" => {
                    seed = value
                        .parse::<u64>()
                        .map_err(|_| "--seed must be an unsigned 64-bit integer".to_string())?;
                }
                _ => return Err(format!("unknown option: {flag}")),
            }
            index += 2;
        }
        Ok(Self {
            git_commit: git_commit.ok_or_else(|| "missing --git-commit".to_string())?,
            deck_id,
            actors,
            warmup_games,
            games_per_actor,
            validation_games,
            seed,
        })
    }
}

fn parse_bounded_u64(value: &str, flag: &str, minimum: u64, maximum: u64) -> Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("{flag} must be an unsigned integer"))?;
    if !(minimum..=maximum).contains(&parsed) {
        return Err(format!("{flag} must be in {minimum}..={maximum}"));
    }
    Ok(parsed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct EpisodeSeedBundleV1 {
    episode_id: u64,
    env_seed: u64,
    policy_seed: u64,
}

#[derive(Debug, Clone, Copy)]
enum SeedPhaseV1 {
    Measurement,
    Warmup,
    Validation,
}

impl SeedPhaseV1 {
    fn partition_offset(self) -> u64 {
        match self {
            SeedPhaseV1::Measurement => 0,
            SeedPhaseV1::Warmup => WARMUP_PARTITION_OFFSET,
            SeedPhaseV1::Validation => VALIDATION_PARTITION_OFFSET,
        }
    }
}

fn partition_value(
    base_seed: u64,
    domain_offset: u64,
    phase: SeedPhaseV1,
    actor: usize,
    game_index: u64,
) -> Option<u64> {
    if actor >= 256 || game_index >= PARTITION_STRIDE {
        return None;
    }
    let partition = domain_offset + phase.partition_offset() + actor as u64;
    Some(
        base_seed
            .wrapping_add(partition.wrapping_mul(PARTITION_STRIDE))
            .wrapping_add(game_index),
    )
}

fn seed_bundle(
    base_seed: u64,
    phase: SeedPhaseV1,
    actor: usize,
    game_index: u64,
) -> Option<EpisodeSeedBundleV1> {
    Some(EpisodeSeedBundleV1 {
        episode_id: partition_value(base_seed, EPISODE_DOMAIN_OFFSET, phase, actor, game_index)?,
        env_seed: partition_value(base_seed, ENV_DOMAIN_OFFSET, phase, actor, game_index)?,
        policy_seed: partition_value(base_seed, POLICY_DOMAIN_OFFSET, phase, actor, game_index)?,
    })
}

#[derive(Debug)]
enum PendingCombatChoiceV1 {
    Attackers {
        physical_decision_id: u64,
        mask: Vec<bool>,
    },
    Blockers {
        physical_decision_id: u64,
        chosen_index: Option<usize>,
        candidate_count: usize,
    },
}

#[derive(Debug)]
struct SeededAggregatePolicyV1 {
    rng: SplitMix64,
    pending_combat: Option<PendingCombatChoiceV1>,
}

#[derive(Debug, Clone, Copy)]
enum SelectionV1 {
    Index(usize),
    Include(bool),
}

impl SeededAggregatePolicyV1 {
    fn new(seed: u64) -> Self {
        Self {
            rng: SplitMix64::seed(seed),
            pending_combat: None,
        }
    }

    fn select_shape(
        &mut self,
        kind: FastActorDecisionKindV1,
        physical_decision_id: u64,
        substep_index: u32,
        substep_count: u32,
        action_count: usize,
    ) -> Result<SelectionV1, String> {
        match kind {
            FastActorDecisionKindV1::Surface => {
                if substep_index != 0 || substep_count != 1 || self.pending_combat.is_some() {
                    return Err(
                        "surface decision encountered with invalid combat-group state".into(),
                    );
                }
                if action_count == 0 {
                    return Err("surface decision produced zero legal actions".into());
                }
                Ok(SelectionV1::Index(rng_below(&mut self.rng, action_count)))
            }
            FastActorDecisionKindV1::AttackerInclusion
            | FastActorDecisionKindV1::BlockerInclusion => {
                let index = substep_index as usize;
                let count = substep_count as usize;
                if count == 0 || index >= count || action_count != 2 {
                    return Err("combat decision has invalid binary group shape".into());
                }
                if index == 0 {
                    if self.pending_combat.is_some() {
                        return Err("combat group overlapped a pending group".into());
                    }
                    self.pending_combat = Some(match kind {
                        FastActorDecisionKindV1::AttackerInclusion => {
                            PendingCombatChoiceV1::Attackers {
                                physical_decision_id,
                                mask: (0..count)
                                    .map(|_| rng_chance(&mut self.rng, 1, 2))
                                    .collect(),
                            }
                        }
                        FastActorDecisionKindV1::BlockerInclusion => {
                            PendingCombatChoiceV1::Blockers {
                                physical_decision_id,
                                chosen_index: if rng_chance(&mut self.rng, 35, 100) {
                                    Some(rng_below(&mut self.rng, count))
                                } else {
                                    None
                                },
                                candidate_count: count,
                            }
                        }
                        FastActorDecisionKindV1::Surface => unreachable!(),
                    });
                }
                let include = match (&self.pending_combat, kind) {
                    (
                        Some(PendingCombatChoiceV1::Attackers {
                            physical_decision_id: pending_id,
                            mask,
                        }),
                        FastActorDecisionKindV1::AttackerInclusion,
                    ) if *pending_id == physical_decision_id && mask.len() == count => mask[index],
                    (
                        Some(PendingCombatChoiceV1::Blockers {
                            physical_decision_id: pending_id,
                            chosen_index,
                            candidate_count,
                        }),
                        FastActorDecisionKindV1::BlockerInclusion,
                    ) if *pending_id == physical_decision_id && *candidate_count == count => {
                        *chosen_index == Some(index)
                    }
                    _ => return Err("combat group does not match its pending sample".into()),
                };
                if index + 1 == count {
                    self.pending_combat = None;
                }
                Ok(SelectionV1::Include(include))
            }
        }
    }

    fn full_action(&mut self, decision: &RlSessionDecisionV1) -> Result<(u32, String), String> {
        let first = decision
            .legal_actions
            .first()
            .ok_or_else(|| "full v5 decision produced zero legal actions".to_string())?;
        let kind = match first.semantic {
            ActionSemanticV1::ChooseAttackerInclusion { .. } => {
                FastActorDecisionKindV1::AttackerInclusion
            }
            ActionSemanticV1::ChooseBlockerInclusion { .. } => {
                FastActorDecisionKindV1::BlockerInclusion
            }
            _ => FastActorDecisionKindV1::Surface,
        };
        let selection = self.select_shape(
            kind,
            decision.physical_decision_id,
            decision.substep_index,
            decision.substep_count,
            decision.legal_actions.len(),
        )?;
        let selected = match selection {
            SelectionV1::Index(index) => index,
            SelectionV1::Include(include) => decision
                .legal_actions
                .iter()
                .position(|candidate| match (&candidate.semantic, kind) {
                    (
                        ActionSemanticV1::ChooseAttackerInclusion {
                            include: candidate_include,
                            ..
                        },
                        FastActorDecisionKindV1::AttackerInclusion,
                    ) => *candidate_include == include,
                    (
                        ActionSemanticV1::ChooseBlockerInclusion {
                            include: candidate_include,
                            ..
                        },
                        FastActorDecisionKindV1::BlockerInclusion,
                    ) => *candidate_include == include,
                    _ => false,
                })
                .ok_or_else(|| "full v5 combat decision omitted a Boolean action".to_string())?,
        };
        let action = &decision.legal_actions[selected];
        if action.selected_index as usize != selected {
            return Err("full v5 selected indices are not dense and ordered".into());
        }
        Ok((action.selected_index, action.stable_id.clone()))
    }

    fn fast_action(&mut self, decision: FastActorDecisionV1) -> Result<u32, String> {
        let selection = self.select_shape(
            decision.decision_kind,
            decision.physical_decision_id,
            decision.substep_index,
            decision.substep_count,
            decision.legal_action_count as usize,
        )?;
        let selected = match selection {
            SelectionV1::Index(index) => index,
            // The shared core generator defines the combat order as
            // [exclude, include]. Paired validation below checks it against
            // full v5 semantics before any timed trial is emitted.
            SelectionV1::Include(include) => usize::from(include),
        };
        u32::try_from(selected).map_err(|_| "fast selected index exceeds u32".into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum GameOutcomeV1 {
    NaturalTerminal,
    PhysicalDecisionCap,
    PolicyStepCap,
    Halted,
    FailClosed,
    DriverError,
}

fn terminal_outcome(terminal: &RlSessionTerminalV1) -> GameOutcomeV1 {
    match terminal.terminal_classification {
        TerminalClassificationV1::Natural => GameOutcomeV1::NaturalTerminal,
        TerminalClassificationV1::Truncated
            if terminal
                .terminal_reason
                .starts_with("physical_decision_cap_reached:") =>
        {
            GameOutcomeV1::PhysicalDecisionCap
        }
        TerminalClassificationV1::Truncated
            if terminal
                .terminal_reason
                .starts_with("policy_step_cap_reached:") =>
        {
            GameOutcomeV1::PolicyStepCap
        }
        TerminalClassificationV1::Halted
            if terminal.terminal_reason.starts_with("fail_closed:") =>
        {
            GameOutcomeV1::FailClosed
        }
        TerminalClassificationV1::Halted => GameOutcomeV1::Halted,
        _ => GameOutcomeV1::DriverError,
    }
}

#[derive(Debug, Clone, Copy)]
struct GameResultV1 {
    outcome: GameOutcomeV1,
    policy_steps: u64,
    physical_decisions: u64,
}

fn deck_ids(deck: &'static RuntimeDeckDefinition) -> SessionDeckIdsV1 {
    [deck.id.to_string(), deck.id.to_string()]
}

fn play_full(deck: &'static RuntimeDeckDefinition, seeds: EpisodeSeedBundleV1) -> GameResultV1 {
    let mut session = match RlEpisodeSessionV1::reset_with_decks_and_limits(
        seeds.episode_id,
        seeds.env_seed,
        DECISION_SAFETY_CAP,
        DECISION_SAFETY_CAP.saturating_mul(128),
        deck_ids(deck),
    ) {
        Ok(session) => session,
        Err(_) => {
            return GameResultV1 {
                outcome: GameOutcomeV1::DriverError,
                policy_steps: 0,
                physical_decisions: 0,
            };
        }
    };
    let mut policy = SeededAggregatePolicyV1::new(seeds.policy_seed);
    let mut response = session.current_response();
    loop {
        match response {
            RlSessionResponseV1::Terminal(terminal) => {
                return GameResultV1 {
                    outcome: terminal_outcome(&terminal),
                    policy_steps: terminal.policy_step_count,
                    physical_decisions: terminal.physical_decision_count,
                };
            }
            RlSessionResponseV1::Decision(decision) => {
                let (index, id) = match policy.full_action(&decision) {
                    Ok(selected) => selected,
                    Err(_) => {
                        return GameResultV1 {
                            outcome: GameOutcomeV1::DriverError,
                            policy_steps: session.policy_step_count(),
                            physical_decisions: session.physical_decision_count(),
                        };
                    }
                };
                response = match session.step(seeds.episode_id, decision.step, index, &id) {
                    Ok(next) => next,
                    Err(_) => {
                        return GameResultV1 {
                            outcome: GameOutcomeV1::DriverError,
                            policy_steps: session.policy_step_count(),
                            physical_decisions: session.physical_decision_count(),
                        };
                    }
                };
            }
        }
    }
}

fn play_fast(deck: &'static RuntimeDeckDefinition, seeds: EpisodeSeedBundleV1) -> GameResultV1 {
    let mut session = match FastActorSessionV1::reset_with_decks_and_limits(
        seeds.episode_id,
        seeds.env_seed,
        DECISION_SAFETY_CAP,
        DECISION_SAFETY_CAP.saturating_mul(128),
        deck_ids(deck),
    ) {
        Ok(session) => session,
        Err(_) => {
            return GameResultV1 {
                outcome: GameOutcomeV1::DriverError,
                policy_steps: 0,
                physical_decisions: 0,
            };
        }
    };
    let mut policy = SeededAggregatePolicyV1::new(seeds.policy_seed);
    let mut response = session.current_response();
    loop {
        match response {
            FastActorResponseV1::Terminal(terminal) => {
                return GameResultV1 {
                    outcome: terminal_outcome(&terminal),
                    policy_steps: terminal.policy_step_count,
                    physical_decisions: terminal.physical_decision_count,
                };
            }
            FastActorResponseV1::Decision(decision) => {
                let index = match policy.fast_action(decision) {
                    Ok(selected) => selected,
                    Err(_) => {
                        return GameResultV1 {
                            outcome: GameOutcomeV1::DriverError,
                            policy_steps: session.policy_step_count(),
                            physical_decisions: session.physical_decision_count(),
                        };
                    }
                };
                response = match session.step(seeds.episode_id, decision.step, index) {
                    Ok(next) => next,
                    Err(_) => {
                        return GameResultV1 {
                            outcome: GameOutcomeV1::DriverError,
                            policy_steps: session.policy_step_count(),
                            physical_decisions: session.physical_decision_count(),
                        };
                    }
                };
            }
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
struct OutcomeCountsV1 {
    attempted_games: u64,
    natural_terminal_games: u64,
    physical_decision_cap_games: u64,
    policy_step_cap_games: u64,
    halted_games: u64,
    fail_closed_games: u64,
    driver_error_games: u64,
}

impl OutcomeCountsV1 {
    fn record(&mut self, outcome: GameOutcomeV1) {
        self.attempted_games += 1;
        match outcome {
            GameOutcomeV1::NaturalTerminal => self.natural_terminal_games += 1,
            GameOutcomeV1::PhysicalDecisionCap => self.physical_decision_cap_games += 1,
            GameOutcomeV1::PolicyStepCap => self.policy_step_cap_games += 1,
            GameOutcomeV1::Halted => self.halted_games += 1,
            GameOutcomeV1::FailClosed => self.fail_closed_games += 1,
            GameOutcomeV1::DriverError => self.driver_error_games += 1,
        }
    }

    fn merge(&mut self, other: Self) {
        self.attempted_games += other.attempted_games;
        self.natural_terminal_games += other.natural_terminal_games;
        self.physical_decision_cap_games += other.physical_decision_cap_games;
        self.policy_step_cap_games += other.policy_step_cap_games;
        self.halted_games += other.halted_games;
        self.fail_closed_games += other.fail_closed_games;
        self.driver_error_games += other.driver_error_games;
    }

    fn is_exact_natural(self, expected_games: u64) -> bool {
        self.attempted_games == expected_games
            && self.natural_terminal_games == expected_games
            && self.physical_decision_cap_games == 0
            && self.policy_step_cap_games == 0
            && self.halted_games == 0
            && self.fail_closed_games == 0
            && self.driver_error_games == 0
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum LaneKindV1 {
    FullV5,
    FastActor,
}

fn play_lane(
    lane: LaneKindV1,
    deck: &'static RuntimeDeckDefinition,
    seeds: EpisodeSeedBundleV1,
) -> GameResultV1 {
    match lane {
        LaneKindV1::FullV5 => play_full(deck, seeds),
        LaneKindV1::FastActor => play_fast(deck, seeds),
    }
}

#[derive(Serialize)]
struct LaneTrialV1 {
    lane: LaneKindV1,
    actors: usize,
    warmup_games_per_actor: u64,
    measurement_games_per_actor: u64,
    warmup_outcomes: OutcomeCountsV1,
    measurement_outcomes: OutcomeCountsV1,
    all_outcomes_natural: bool,
    policy_steps: u64,
    physical_decisions: u64,
    actor_finish_ns: Vec<u64>,
    common_wall_ns: u64,
    attempted_games_per_second: f64,
    natural_games_per_second: f64,
    policy_steps_per_second: f64,
    physical_decisions_per_second: f64,
}

fn measure_lane(
    lane: LaneKindV1,
    deck: &'static RuntimeDeckDefinition,
    actors: usize,
    warmup_games: u64,
    games_per_actor: u64,
    base_seed: u64,
) -> LaneTrialV1 {
    #[derive(Debug)]
    struct WorkerResultV1 {
        actor: usize,
        warmup_counts: OutcomeCountsV1,
        measurement_counts: OutcomeCountsV1,
        policy_steps: u64,
        physical_decisions: u64,
        elapsed_ns: u64,
    }

    let barrier = Arc::new(Barrier::new(actors));
    let handles: Vec<_> = (0..actors)
        .map(|actor| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let mut warmup_counts = OutcomeCountsV1::default();
                for game_index in 0..warmup_games {
                    let seeds = seed_bundle(base_seed, SeedPhaseV1::Warmup, actor, game_index)
                        .expect("bounded warmup seed partition");
                    warmup_counts.record(play_lane(lane, deck, seeds).outcome);
                }
                barrier.wait();
                let started = Instant::now();
                let mut counts = OutcomeCountsV1::default();
                let mut policy_steps = 0u64;
                let mut physical_decisions = 0u64;
                for game_index in 0..games_per_actor {
                    let seeds = seed_bundle(base_seed, SeedPhaseV1::Measurement, actor, game_index)
                        .expect("bounded measurement seed partition");
                    let result = play_lane(lane, deck, seeds);
                    counts.record(result.outcome);
                    policy_steps = policy_steps.saturating_add(result.policy_steps);
                    physical_decisions =
                        physical_decisions.saturating_add(result.physical_decisions);
                }
                WorkerResultV1 {
                    actor,
                    warmup_counts,
                    measurement_counts: counts,
                    policy_steps,
                    physical_decisions,
                    elapsed_ns: u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                }
            })
        })
        .collect();
    let mut warmup_outcomes = OutcomeCountsV1::default();
    let mut measurement_outcomes = OutcomeCountsV1::default();
    let mut policy_steps = 0u64;
    let mut physical_decisions = 0u64;
    let mut actor_finish_ns = vec![0; actors];
    for handle in handles {
        let result = handle.join().expect("fast-actor benchmark worker panicked");
        warmup_outcomes.merge(result.warmup_counts);
        measurement_outcomes.merge(result.measurement_counts);
        policy_steps = policy_steps.saturating_add(result.policy_steps);
        physical_decisions = physical_decisions.saturating_add(result.physical_decisions);
        actor_finish_ns[result.actor] = result.elapsed_ns;
    }
    let common_wall_ns = actor_finish_ns.iter().copied().max().unwrap_or(1).max(1);
    let seconds = common_wall_ns as f64 / 1_000_000_000.0;
    let expected_warmup_games = (actors as u64).saturating_mul(warmup_games);
    let expected_measurement_games = (actors as u64).saturating_mul(games_per_actor);
    LaneTrialV1 {
        lane,
        actors,
        warmup_games_per_actor: warmup_games,
        measurement_games_per_actor: games_per_actor,
        warmup_outcomes,
        measurement_outcomes,
        all_outcomes_natural: warmup_outcomes.is_exact_natural(expected_warmup_games)
            && measurement_outcomes.is_exact_natural(expected_measurement_games),
        policy_steps,
        physical_decisions,
        actor_finish_ns,
        common_wall_ns,
        attempted_games_per_second: measurement_outcomes.attempted_games as f64 / seconds,
        natural_games_per_second: measurement_outcomes.natural_terminal_games as f64 / seconds,
        policy_steps_per_second: policy_steps as f64 / seconds,
        physical_decisions_per_second: physical_decisions as f64 / seconds,
    }
}

fn validate_paired_trials(full: &LaneTrialV1, fast: &LaneTrialV1) -> Result<(), String> {
    if !matches!(full.lane, LaneKindV1::FullV5)
        || !matches!(fast.lane, LaneKindV1::FastActor)
        || full.actors != fast.actors
        || full.warmup_games_per_actor != fast.warmup_games_per_actor
        || full.measurement_games_per_actor != fast.measurement_games_per_actor
    {
        return Err("full and fast trials do not describe the same actor workload".into());
    }
    let actors = u64::try_from(full.actors)
        .map_err(|_| "actor count does not fit the benchmark counter".to_string())?;
    let expected_warmup_games = actors
        .checked_mul(full.warmup_games_per_actor)
        .ok_or_else(|| "warmup game total overflow".to_string())?;
    let expected_measurement_games = actors
        .checked_mul(full.measurement_games_per_actor)
        .ok_or_else(|| "measurement game total overflow".to_string())?;
    if !full.warmup_outcomes.is_exact_natural(expected_warmup_games)
        || !fast.warmup_outcomes.is_exact_natural(expected_warmup_games)
        || !full
            .measurement_outcomes
            .is_exact_natural(expected_measurement_games)
        || !fast
            .measurement_outcomes
            .is_exact_natural(expected_measurement_games)
    {
        return Err("a benchmark lane produced a non-natural or incomplete outcome set".into());
    }
    if full.warmup_outcomes != fast.warmup_outcomes
        || full.measurement_outcomes != fast.measurement_outcomes
        || full.policy_steps != fast.policy_steps
        || full.physical_decisions != fast.physical_decisions
    {
        return Err(
            "full and fast measurement outcomes or transition totals are not identical".into(),
        );
    }
    if !full.all_outcomes_natural || !fast.all_outcomes_natural {
        return Err("lane validity flag disagrees with exact natural outcome accounting".into());
    }
    Ok(())
}

#[derive(Serialize)]
struct ValidationRecordV1 {
    paired_games: u64,
    exact_state_surface_binding_action_order_terminal_parity: bool,
    aggregate_combat_groups_seen: u64,
    policy_steps_compared: u64,
    trajectory_digest_fnv1a64_hex: String,
}

fn fnv1a64_continue(mut hash: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn validate_pairs(
    deck: &'static RuntimeDeckDefinition,
    base_seed: u64,
    games: u64,
) -> Result<ValidationRecordV1, String> {
    let mut digest = 0xcbf29ce484222325u64;
    let mut aggregate_combat_groups_seen = 0u64;
    let mut policy_steps_compared = 0u64;
    for game_index in 0..games {
        let seeds = seed_bundle(base_seed, SeedPhaseV1::Validation, 0, game_index)
            .ok_or_else(|| "validation seed partition exhausted".to_string())?;
        let mut full = RlEpisodeSessionV1::reset_with_decks_and_limits(
            seeds.episode_id,
            seeds.env_seed,
            DECISION_SAFETY_CAP,
            DECISION_SAFETY_CAP.saturating_mul(128),
            deck_ids(deck),
        )
        .map_err(|err| err.to_string())?;
        let mut fast = FastActorSessionV1::reset_with_decks_and_limits(
            seeds.episode_id,
            seeds.env_seed,
            DECISION_SAFETY_CAP,
            DECISION_SAFETY_CAP.saturating_mul(128),
            deck_ids(deck),
        )
        .map_err(|err| err.to_string())?;
        let mut full_policy = SeededAggregatePolicyV1::new(seeds.policy_seed);
        let mut fast_policy = SeededAggregatePolicyV1::new(seeds.policy_seed);
        loop {
            let full_hash = full.privileged_core_environment_hash();
            let fast_hash = fast.privileged_core_environment_hash();
            if full_hash != fast_hash
                || full.diagnostic_state_hash() != fast.diagnostic_state_hash()
            {
                return Err(format!(
                    "paired validation state/core hash mismatch at game {game_index} step {}",
                    full.policy_step_count()
                ));
            }
            digest = fnv1a64_continue(digest, full_hash);
            match (full.current_response(), fast.current_response()) {
                (
                    RlSessionResponseV1::Terminal(full_terminal),
                    FastActorResponseV1::Terminal(fast_terminal),
                ) => {
                    if full_terminal != fast_terminal {
                        return Err(format!(
                            "paired validation terminal mismatch at game {game_index}"
                        ));
                    }
                    break;
                }
                (
                    RlSessionResponseV1::Decision(full_decision),
                    FastActorResponseV1::Decision(fast_decision),
                ) => {
                    let full_kind = match full_decision.legal_actions.first().map(|a| &a.semantic) {
                        Some(ActionSemanticV1::ChooseAttackerInclusion { .. }) => {
                            FastActorDecisionKindV1::AttackerInclusion
                        }
                        Some(ActionSemanticV1::ChooseBlockerInclusion { .. }) => {
                            FastActorDecisionKindV1::BlockerInclusion
                        }
                        Some(_) => FastActorDecisionKindV1::Surface,
                        None => return Err("full validation decision has zero actions".into()),
                    };
                    if full_decision.episode_id != fast_decision.episode_id
                        || full_decision.step != fast_decision.step
                        || full_decision.physical_decision_id != fast_decision.physical_decision_id
                        || full_decision.substep_index != fast_decision.substep_index
                        || full_decision.substep_count != fast_decision.substep_count
                        || full_decision.acting_player != fast_decision.acting_player
                        || full_kind != fast_decision.decision_kind
                        || full_decision.legal_actions.len()
                            != fast_decision.legal_action_count as usize
                    {
                        return Err(format!(
                            "paired validation decision metadata mismatch at game {game_index} step {}",
                            full_decision.step
                        ));
                    }
                    if fast_decision.substep_index == 0
                        && fast_decision.decision_kind != FastActorDecisionKindV1::Surface
                    {
                        aggregate_combat_groups_seen += 1;
                    }
                    let (full_index, full_id) = full_policy.full_action(&full_decision)?;
                    let fast_index = fast_policy.fast_action(fast_decision)?;
                    if full_index != fast_index {
                        return Err(format!(
                            "paired validation action-order mismatch at game {game_index} step {}",
                            full_decision.step
                        ));
                    }
                    full.step(seeds.episode_id, full_decision.step, full_index, &full_id)
                        .map_err(|err| err.to_string())?;
                    fast.step(seeds.episode_id, fast_decision.step, fast_index)
                        .map_err(|err| err.to_string())?;
                    policy_steps_compared += 1;
                }
                _ => {
                    return Err(format!(
                        "paired validation terminal-state mismatch at game {game_index}"
                    ));
                }
            }
        }
    }
    Ok(ValidationRecordV1 {
        paired_games: games,
        exact_state_surface_binding_action_order_terminal_parity: true,
        aggregate_combat_groups_seen,
        policy_steps_compared,
        trajectory_digest_fnv1a64_hex: format!("{digest:016x}"),
    })
}

#[derive(Serialize)]
struct DeckRecordV1<'a> {
    ordered_ids: [&'a str; 2],
    ordered_runtime_deck_hashes: [u64; 2],
    source_path: &'a str,
    source_sha256: &'a str,
    mainboard_count: usize,
}

#[derive(Serialize)]
struct BinaryRecordV1<'a> {
    package: &'static str,
    package_version: &'static str,
    git_commit_claim: &'a str,
    source_verification: &'static str,
    build_profile: &'static str,
}

#[derive(Serialize)]
struct WorkloadRecordV1<'a> {
    policy: &'static str,
    policy_description: &'static str,
    actor_counts: &'a [usize],
    warmup_games_per_actor: u64,
    measurement_games_per_actor: u64,
    validation_games: u64,
    base_seed: u64,
    seed_schedule: &'static str,
    exact_measured_episode_sets_paired_between_lanes: bool,
    decision_safety_cap: u64,
    full_v5_includes: &'static str,
    fast_actor_includes: &'static str,
    exclusions: &'static [&'static str],
}

#[derive(Serialize)]
struct ActorTrialV1 {
    actors: usize,
    full_v5: LaneTrialV1,
    fast_actor: LaneTrialV1,
    fast_over_full_natural_games_per_second: f64,
    fast_over_full_policy_steps_per_second: f64,
}

#[derive(Serialize)]
struct FastActorRecordV2<'a> {
    schema: &'static str,
    claim_scope: &'static str,
    deck: DeckRecordV1<'a>,
    binary: BinaryRecordV1<'a>,
    available_parallelism: usize,
    workload: WorkloadRecordV1<'a>,
    validation: ValidationRecordV1,
    trials: Vec<ActorTrialV1>,
}

fn build_record<'a>(
    config: &'a FastActorConfigV2,
    deck: &'static RuntimeDeckDefinition,
    validation: ValidationRecordV1,
    trials: Vec<ActorTrialV1>,
) -> FastActorRecordV2<'a> {
    FastActorRecordV2 {
        schema: SCHEMA_V2,
        claim_scope: "rust_in_process_actor_environment_ceiling_not_end_to_end_training",
        deck: DeckRecordV1 {
            ordered_ids: [deck.id, deck.id],
            ordered_runtime_deck_hashes: [deck.runtime_deck_hash; 2],
            source_path: deck.source_path,
            source_sha256: deck.source_sha256,
            mainboard_count: deck.mainboard_count,
        },
        binary: BinaryRecordV1 {
            package: env!("CARGO_PKG_NAME"),
            package_version: env!("CARGO_PKG_VERSION"),
            git_commit_claim: &config.git_commit,
            source_verification: "user_supplied_unverified/v1",
            build_profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        },
        available_parallelism: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
        workload: WorkloadRecordV1 {
            policy: POLICY_V1,
            policy_description: "uniform legal noncombat choice; one-half attacker inclusion sampled once per aggregate group; no-block-or-one-block sampled once per aggregate group with 35-percent block probability",
            actor_counts: &config.actors,
            warmup_games_per_actor: config.warmup_games,
            measurement_games_per_actor: config.games_per_actor,
            validation_games: config.validation_games,
            base_seed: config.seed,
            seed_schedule: "wrapping_u64_domain_phase_actor_partition_plus_game_index/v1",
            exact_measured_episode_sets_paired_between_lanes: true,
            decision_safety_cap: DECISION_SAFETY_CAP,
            full_v5_includes: "policy_surface_v5_plus_observation_v5_visible_hash_stable_action_ids_and_response_clones",
            fast_actor_includes: "same_policy_surface_v5_and_shared_ordered_action_core_without_observation_hash_stable_display_json_or_python",
            exclusions: &[
                "jsonl_serialization_and_protocol",
                "process_ipc",
                "python",
                "neural_model_inference",
                "learner",
                "loss_backward_optimizer",
                "artifact_persistence",
                "xmage",
            ],
        },
        validation,
        trials,
    }
}

pub(crate) fn run_fast_actor_ceiling_json_v2(config: FastActorConfigV2) {
    let deck = runtime_deck_by_id(&config.deck_id).expect("validated runtime deck");
    let validation = validate_pairs(deck, config.seed, config.validation_games)
        .unwrap_or_else(|message| panic!("fast actor paired validation failed: {message}"));
    let trials = config
        .actors
        .iter()
        .copied()
        .map(|actors| {
            let full_v5 = measure_lane(
                LaneKindV1::FullV5,
                deck,
                actors,
                config.warmup_games,
                config.games_per_actor,
                config.seed,
            );
            let fast_actor = measure_lane(
                LaneKindV1::FastActor,
                deck,
                actors,
                config.warmup_games,
                config.games_per_actor,
                config.seed,
            );
            validate_paired_trials(&full_v5, &fast_actor).unwrap_or_else(|message| {
                panic!("fast actor benchmark refused comparison: {message}")
            });
            let fast_over_full_natural_games_per_second =
                fast_actor.natural_games_per_second / full_v5.natural_games_per_second;
            let fast_over_full_policy_steps_per_second =
                fast_actor.policy_steps_per_second / full_v5.policy_steps_per_second;
            assert!(
                fast_over_full_natural_games_per_second.is_finite()
                    && fast_over_full_policy_steps_per_second.is_finite(),
                "fast-actor speedup ratios must be finite"
            );
            ActorTrialV1 {
                actors,
                full_v5,
                fast_actor,
                fast_over_full_natural_games_per_second,
                fast_over_full_policy_steps_per_second,
            }
        })
        .collect();
    let record = build_record(&config, deck, validation, trials);
    println!(
        "{}",
        serde_json::to_string(&record).expect("fast actor ceiling record serializes")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "d2caf438dc3acbbcee926ab5d3effe422a77e5b3";

    fn config() -> FastActorConfigV2 {
        FastActorConfigV2 {
            git_commit: COMMIT.into(),
            deck_id: "Rally".into(),
            actors: vec![1, 4, 8, 16],
            warmup_games: 0,
            games_per_actor: 1,
            validation_games: 1,
            seed: u64::MAX - 81_700,
        }
    }

    #[test]
    fn cli_is_strict_and_n1_n4_n8_n16_defaults_are_explicit() {
        let parsed = FastActorConfigV2::parse(&[
            "--git-commit".into(),
            COMMIT.into(),
            "--warmup-games".into(),
            "0".into(),
            "--games-per-actor".into(),
            "1".into(),
            "--validation-games".into(),
            "1".into(),
        ])
        .unwrap();
        assert_eq!(parsed.deck_id, "Rally");
        assert_eq!(parsed.actors, [1, 4, 8, 16]);
        assert_eq!(parsed.warmup_games, 0);
        assert_eq!(parsed.games_per_actor, 1);
        assert!(FastActorConfigV2::parse(&[]).is_err());
        assert!(FastActorConfigV2::parse(&[
            "--git-commit".into(),
            COMMIT.into(),
            "--actors".into(),
            "1,1".into(),
        ])
        .is_err());
        assert!(FastActorConfigV2::parse(&[
            "--git-commit".into(),
            COMMIT.into(),
            "--deck".into(),
            "rally".into(),
        ])
        .is_err());
    }

    #[test]
    fn seed_domains_phases_and_actor_partitions_are_disjoint() {
        let mut values = HashSet::new();
        for domain in [
            EPISODE_DOMAIN_OFFSET,
            ENV_DOMAIN_OFFSET,
            POLICY_DOMAIN_OFFSET,
        ] {
            for phase in [
                SeedPhaseV1::Measurement,
                SeedPhaseV1::Warmup,
                SeedPhaseV1::Validation,
            ] {
                for actor in 0..16 {
                    for game in 0..100 {
                        assert!(values.insert(
                            partition_value(u64::MAX - 81_700, domain, phase, actor, game).unwrap()
                        ));
                    }
                }
            }
        }
        assert_eq!(values.len(), 3 * 3 * 16 * 100);
    }

    #[test]
    fn paired_validation_covers_burn_and_rally_aggregate_combat() {
        for deck_id in ["Burn", "Rally"] {
            let validation =
                validate_pairs(runtime_deck_by_id(deck_id).unwrap(), 81_700, 1).unwrap();
            assert!(validation.exact_state_surface_binding_action_order_terminal_parity);
            assert!(validation.policy_steps_compared > 0);
            if deck_id == "Rally" {
                assert!(validation.aggregate_combat_groups_seen > 0);
            }
        }
    }

    #[test]
    fn schema_is_new_one_line_finite_and_does_not_change_existing_v1_shapes() {
        let config = config();
        let validation =
            validate_pairs(runtime_deck_by_id("Rally").unwrap(), config.seed, 1).unwrap();
        let record = build_record(
            &config,
            runtime_deck_by_id("Rally").unwrap(),
            validation,
            vec![],
        );
        let encoded = serde_json::to_string(&record).unwrap();
        assert!(!encoded.contains('\n'));
        assert!(!encoded.contains("NaN"));
        assert!(!encoded.contains("Infinity"));
        let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["schema"], SCHEMA_V2);
        assert_eq!(
            value["workload"]["exact_measured_episode_sets_paired_between_lanes"],
            true
        );
        assert_eq!(
            value["workload"]["actor_counts"],
            serde_json::json!([1, 4, 8, 16])
        );
    }

    #[test]
    fn one_game_lane_smoke_has_exact_accounting_and_comparison_gate() {
        let deck = runtime_deck_by_id("Burn").unwrap();
        let full = measure_lane(LaneKindV1::FullV5, deck, 1, 0, 1, 81_701);
        let mut fast = measure_lane(LaneKindV1::FastActor, deck, 1, 0, 1, 81_701);
        for trial in [&full, &fast] {
            assert_eq!(trial.warmup_outcomes.attempted_games, 0);
            assert_eq!(trial.measurement_outcomes.attempted_games, 1);
            assert_eq!(trial.measurement_outcomes.natural_terminal_games, 1);
            assert!(trial.all_outcomes_natural);
            assert!(trial.policy_steps > 0);
            assert!(trial.attempted_games_per_second.is_finite());
        }
        validate_paired_trials(&full, &fast).unwrap();

        fast.policy_steps += 1;
        assert!(validate_paired_trials(&full, &fast).is_err());
        fast.policy_steps -= 1;
        fast.physical_decisions += 1;
        assert!(validate_paired_trials(&full, &fast).is_err());
        fast.physical_decisions -= 1;
        fast.measurement_outcomes.natural_terminal_games = 0;
        fast.measurement_outcomes.driver_error_games = 1;
        fast.all_outcomes_natural = false;
        assert!(validate_paired_trials(&full, &fast).is_err());

        fast.measurement_outcomes.natural_terminal_games = 1;
        fast.measurement_outcomes.driver_error_games = 0;
        fast.all_outcomes_natural = true;
        validate_paired_trials(&full, &fast).unwrap();
        let natural_ratio = fast.natural_games_per_second / full.natural_games_per_second;
        let policy_ratio = fast.policy_steps_per_second / full.policy_steps_per_second;
        let encoded = serde_json::to_value(ActorTrialV1 {
            actors: 1,
            full_v5: full,
            fast_actor: fast,
            fast_over_full_natural_games_per_second: natural_ratio,
            fast_over_full_policy_steps_per_second: policy_ratio,
        })
        .unwrap();
        assert!(encoded
            .get("fast_over_full_natural_games_per_second")
            .is_some());
        assert!(encoded
            .get("fast_over_full_attempted_games_per_second")
            .is_none());
    }
}
