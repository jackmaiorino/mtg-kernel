//! Fixed-Rally, in-process matched-uniform runtime trial candidate.
//!
//! The timed lane is [`FastActorSessionV1`]. Full policy-v5 sessions are
//! used only as untimed parity oracles over frozen Burn and Rally episodes.
//! This is neither an XMage comparison nor end-to-end training throughput.

use crate::DECISION_SAFETY_CAP;
use mtg_kernel::card_def::KERNEL_CARDDB_HASH;
use mtg_kernel::rl::{ActionSemanticV1, PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1};
use mtg_kernel::rl_session::{
    FastActorDecisionKindV1, FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1,
    RlEpisodeSessionV1, RlSessionDecisionV1, RlSessionResponseV1, RlSessionTerminalV1,
    SessionDeckIdsV1,
};
use mtg_kernel::runtime_decks::{runtime_deck_by_id, RuntimeDeckDefinition};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::io::Write as _;
use std::process::{Command, Stdio};
use std::sync::{Arc, Barrier, OnceLock};
use std::time::{Duration, Instant};

const SCHEMA_V2: &str = "kernel_rl_matched_uniform_runtime_trial/v2";
const CLAIM_SCOPE: &str = "local_rust_runtime_trial_candidate_not_formal_comparison_not_training";
const POLICY_ID: &str = "seeded_uniform_mirror/v2";
const SEED_DERIVATION_VERSION: &str = "kernel-python-rl-seed-v2";
const GOLDEN_RATIO_64: u64 = 0x9E37_79B9_7F4A_7C15;
const PHYSICAL_DECISION_MULTIPLIER: u64 = 0xD1B5_4A32_D192_ED03;
const POLICY_SUBSTEP_MULTIPLIER: u64 = 0x94D0_49BB_1331_11EB;
const ENV_SEED_DOMAIN: u64 = 0x4556_5F52_4C5F_7632;
const UNIFORM_POLICY_DOMAIN: u64 = 0x5059_5F55_4E49_7632;
const POLICY_SUBSTEP_DOMAIN: u64 = 0x5355_4253_5445_5032;
const P0_SEAT_TAG: u64 = 0x5030;
const P1_SEAT_TAG: u64 = 0x5031;
const UINT63_MAX: u64 = i64::MAX as u64;
const VALIDATION_BASE_SEED: u64 = 71_501;
const VALIDATION_GAMES_PER_DECK: u64 = 2;
const MEASUREMENT_EPISODE_BASE: u64 = 1u64 << 62;
const EPISODES_PER_TIMED_PHASE: u64 = 1u64 << 62;
const VALIDATION_BURN_EPISODE_IDS: [u64; 2] = [2u64 << 60, (2u64 << 60) + 1];
const VALIDATION_RALLY_EPISODE_IDS: [u64; 2] = [3u64 << 60, (3u64 << 60) + 1];
const MAX_ACTORS: usize = 256;
const MATCHED_ACTOR_COUNTS: [usize; 4] = [1, 4, 8, 16];
const NANOS_PER_SECOND: u64 = 1_000_000_000;

#[derive(Debug)]
pub(crate) struct MatchedUniformConfigV2 {
    expected_commit: String,
    trial_id: String,
    binding_mode: BindingModeV2,
    affinity_contract_id: String,
    cpu_contract_id: String,
    topology_contract_id: String,
    host_contract_id: String,
    power_contract_id: String,
    expected_available_processors: usize,
    actors: usize,
    warmup_ns: u64,
    measure_ns: u64,
    base_seed: u64,
    transcript_games_per_deck: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingModeV2 {
    Strict,
    DirtySmoke,
}

impl BindingModeV2 {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::DirtySmoke => "dirty-smoke",
        }
    }
}

impl MatchedUniformConfigV2 {
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        let mut expected_commit = None;
        let mut trial_id = None;
        let mut binding_mode = None;
        let mut affinity_contract_id = None;
        let mut cpu_contract_id = None;
        let mut topology_contract_id = None;
        let mut host_contract_id = None;
        let mut power_contract_id = None;
        let mut expected_available_processors = None;
        let mut actors = None;
        let mut warmup_ns = None;
        let mut measure_ns = None;
        let mut base_seed = None;
        let mut transcript_games_per_deck = 0;
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
                "--expected-commit" => {
                    if value.len() != 40
                        || !value
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    {
                        return Err(
                            "--expected-commit must be 40 lowercase hexadecimal characters".into(),
                        );
                    }
                    expected_commit = Some(value.clone());
                }
                "--trial-id" => {
                    validate_private_token(value, flag, 128)?;
                    trial_id = Some(value.clone());
                }
                "--binding-mode" => {
                    binding_mode = Some(match value.as_str() {
                        "strict" => BindingModeV2::Strict,
                        "dirty-smoke" => BindingModeV2::DirtySmoke,
                        _ => return Err("--binding-mode must be strict or dirty-smoke".into()),
                    });
                }
                "--affinity-contract-id" => {
                    validate_private_token(value, flag, 128)?;
                    affinity_contract_id = Some(value.clone());
                }
                "--cpu-contract-id" => {
                    validate_private_token(value, flag, 128)?;
                    cpu_contract_id = Some(value.clone());
                }
                "--topology-contract-id" => {
                    validate_private_token(value, flag, 128)?;
                    topology_contract_id = Some(value.clone());
                }
                "--host-contract-id" => {
                    validate_private_token(value, flag, 128)?;
                    host_contract_id = Some(value.clone());
                }
                "--power-contract-id" => {
                    validate_private_token(value, flag, 128)?;
                    power_contract_id = Some(value.clone());
                }
                "--expected-available-processors" => {
                    expected_available_processors = Some(parse_usize(value, flag, 1, 4_096)?);
                }
                "--actors" => {
                    let parsed = parse_usize(value, flag, 1, MAX_ACTORS)?;
                    if !MATCHED_ACTOR_COUNTS.contains(&parsed) {
                        return Err("--actors must be exactly one of 1,4,8,16".into());
                    }
                    actors = Some(parsed);
                }
                "--warmup-seconds" => {
                    warmup_ns = Some(parse_exact_positive_seconds_ns(value, flag)?);
                }
                "--measure-seconds" => {
                    measure_ns = Some(parse_exact_positive_seconds_ns(value, flag)?);
                }
                "--base-seed" => {
                    base_seed = Some(parse_u64(value, flag, 0, UINT63_MAX)?);
                }
                "--transcript-games-per-deck" => {
                    transcript_games_per_deck =
                        parse_u64(value, flag, 0, VALIDATION_GAMES_PER_DECK)?;
                }
                _ => return Err(format!("unknown option: {flag}")),
            }
            index += 2;
        }
        Ok(Self {
            expected_commit: expected_commit
                .ok_or_else(|| "missing --expected-commit".to_string())?,
            trial_id: trial_id.ok_or_else(|| "missing --trial-id".to_string())?,
            binding_mode: binding_mode.ok_or_else(|| "missing --binding-mode".to_string())?,
            affinity_contract_id: affinity_contract_id
                .ok_or_else(|| "missing --affinity-contract-id".to_string())?,
            cpu_contract_id: cpu_contract_id
                .ok_or_else(|| "missing --cpu-contract-id".to_string())?,
            topology_contract_id: topology_contract_id
                .ok_or_else(|| "missing --topology-contract-id".to_string())?,
            host_contract_id: host_contract_id
                .ok_or_else(|| "missing --host-contract-id".to_string())?,
            power_contract_id: power_contract_id
                .ok_or_else(|| "missing --power-contract-id".to_string())?,
            expected_available_processors: expected_available_processors
                .ok_or_else(|| "missing --expected-available-processors".to_string())?,
            actors: actors.ok_or_else(|| "missing --actors".to_string())?,
            warmup_ns: warmup_ns.ok_or_else(|| "missing --warmup-seconds".to_string())?,
            measure_ns: measure_ns.ok_or_else(|| "missing --measure-seconds".to_string())?,
            base_seed: base_seed.ok_or_else(|| "missing --base-seed".to_string())?,
            transcript_games_per_deck,
        })
    }
}

fn validate_private_token(value: &str, flag: &str, maximum_length: usize) -> Result<(), String> {
    if value.is_empty()
        || value.len() > maximum_length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!(
            "{flag} must be a 1..={maximum_length} character privacy-safe token"
        ));
    }
    Ok(())
}

fn parse_exact_positive_seconds_ns(value: &str, flag: &str) -> Result<u64, String> {
    let (whole, fractional) = match value.split_once('.') {
        Some((whole, fractional)) => (whole, Some(fractional)),
        None => (value, None),
    };
    let canonical_whole = whole == "0"
        || (!whole.is_empty()
            && !whole.starts_with('0')
            && whole.bytes().all(|byte| byte.is_ascii_digit()));
    let canonical_fraction = fractional.is_none_or(|digits| {
        !digits.is_empty() && digits.len() <= 9 && digits.bytes().all(|byte| byte.is_ascii_digit())
    });
    if !canonical_whole || !canonical_fraction {
        return Err(format!(
            "{flag} must be positive canonical decimal seconds with at most 9 fractional digits"
        ));
    }
    let whole_seconds = whole
        .parse::<u64>()
        .map_err(|_| format!("{flag} is outside exact nanosecond range"))?;
    let whole_ns = whole_seconds
        .checked_mul(NANOS_PER_SECOND)
        .ok_or_else(|| format!("{flag} is outside exact nanosecond range"))?;
    let fractional_ns = fractional
        .map(|digits| {
            let value = digits
                .parse::<u64>()
                .map_err(|_| format!("{flag} is outside exact nanosecond range"))?;
            let scale = 10u64.pow(9 - digits.len() as u32);
            value
                .checked_mul(scale)
                .ok_or_else(|| format!("{flag} is outside exact nanosecond range"))
        })
        .transpose()?
        .unwrap_or(0);
    let nanos = whole_ns
        .checked_add(fractional_ns)
        .ok_or_else(|| format!("{flag} is outside exact nanosecond range"))?;
    if nanos == 0 || nanos > i64::MAX as u64 {
        return Err(format!(
            "{flag} must be positive and fit exact signed nanoseconds"
        ));
    }
    Ok(nanos)
}

fn parse_u64(value: &str, flag: &str, minimum: u64, maximum: u64) -> Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("{flag} must be an unsigned integer"))?;
    if !(minimum..=maximum).contains(&parsed) {
        return Err(format!("{flag} must be in {minimum}..={maximum}"));
    }
    Ok(parsed)
}

fn parse_usize(value: &str, flag: &str, minimum: usize, maximum: usize) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("{flag} must be an unsigned integer"))?;
    if !(minimum..=maximum).contains(&parsed) {
        return Err(format!("{flag} must be in {minimum}..={maximum}"));
    }
    Ok(parsed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimedEpisodePhaseV1 {
    Warmup,
    Measurement,
}

impl TimedEpisodePhaseV1 {
    const fn base(self) -> u64 {
        match self {
            Self::Warmup => 0,
            Self::Measurement => MEASUREMENT_EPISODE_BASE,
        }
    }
}

fn striped_timed_episode_id(
    phase: TimedEpisodePhaseV1,
    local_index: u64,
    actor: usize,
    actor_count: usize,
) -> Result<u64, String> {
    if actor_count == 0 || actor_count > MAX_ACTORS || actor >= actor_count {
        return Err("invalid actor coordinates for striped episode schedule".into());
    }
    let actor = u64::try_from(actor).map_err(|_| "actor index exceeds u64".to_string())?;
    let actor_count =
        u64::try_from(actor_count).map_err(|_| "actor count exceeds u64".to_string())?;
    let remaining = EPISODES_PER_TIMED_PHASE - 1 - actor;
    if local_index > remaining / actor_count {
        return Err("episode schedule exhausted reserved uint63 phase range".into());
    }
    Ok(phase.base() + local_index * actor_count + actor)
}

const fn splitmix64_once(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(GOLDEN_RATIO_64);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn derive_seed(
    base_seed: u64,
    episode: u64,
    physical_decision: u64,
    seat_tag: u64,
    domain: u64,
) -> Result<u64, String> {
    if base_seed > UINT63_MAX || episode > UINT63_MAX || physical_decision > UINT63_MAX {
        return Err("seed derivation input escaped uint63".into());
    }
    Ok(splitmix64_once(
        base_seed
            ^ domain
            ^ episode.wrapping_mul(GOLDEN_RATIO_64)
            ^ physical_decision.wrapping_mul(PHYSICAL_DECISION_MULTIPLIER)
            ^ seat_tag,
    ))
}

fn derive_env_seed(base_seed: u64, episode: u64) -> Result<u64, String> {
    derive_seed(base_seed, episode, 0, P0_SEAT_TAG, ENV_SEED_DOMAIN)
}

fn derive_group_seed(
    base_seed: u64,
    episode: u64,
    physical_decision: u64,
    seat: PlayerSeatV1,
) -> Result<u64, String> {
    derive_seed(
        base_seed,
        episode,
        physical_decision,
        match seat {
            PlayerSeatV1::P0 => P0_SEAT_TAG,
            PlayerSeatV1::P1 => P1_SEAT_TAG,
        },
        UNIFORM_POLICY_DOMAIN,
    )
}

fn derive_leaf_seed(group_seed: u64, substep_index: u32) -> u64 {
    splitmix64_once(
        group_seed
            ^ POLICY_SUBSTEP_DOMAIN
            ^ u64::from(substep_index).wrapping_mul(POLICY_SUBSTEP_MULTIPLIER),
    )
}

fn unsigned_modulo(value: u64, bound: usize) -> Result<usize, String> {
    if bound == 0 {
        return Err("uniform modulo bound must be positive".into());
    }
    let bound = u64::try_from(bound).map_err(|_| "uniform modulo bound exceeds u64".to_string())?;
    usize::try_from(value % bound).map_err(|_| "uniform rank exceeds usize".into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingChoiceV1 {
    Attackers,
    Blocker { selected_rank: Option<usize> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingGroupV1 {
    global_physical_id: u64,
    actor: PlayerSeatV1,
    local_physical_index: u64,
    group_seed: u64,
    substep_count: u32,
    choice: PendingChoiceV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PolicySelectionV1 {
    selected_index: u32,
    included: Option<bool>,
    actor_local_physical_index: u64,
    group_seed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PolicyCountersV1 {
    physical_by_seat: [u64; 2],
    surface_steps: u64,
    leaf_evaluations: u64,
    xmage_action_selections: u64,
}

#[derive(Debug)]
struct MatchedUniformPolicyV2 {
    base_seed: u64,
    episode_id: u64,
    counters: PolicyCountersV1,
    pending: Option<PendingGroupV1>,
}

impl MatchedUniformPolicyV2 {
    fn new(base_seed: u64, episode_id: u64) -> Result<Self, String> {
        if base_seed > UINT63_MAX || episode_id > UINT63_MAX {
            return Err("matched-uniform policy requires uint63 base seed and episode id".into());
        }
        Ok(Self {
            base_seed,
            episode_id,
            counters: PolicyCountersV1 {
                physical_by_seat: [0, 0],
                surface_steps: 0,
                leaf_evaluations: 0,
                xmage_action_selections: 0,
            },
            pending: None,
        })
    }

    fn select(&mut self, decision: FastActorDecisionV1) -> Result<PolicySelectionV1, String> {
        if decision.episode_id != self.episode_id {
            return Err("policy episode id does not match decision".into());
        }
        let seat_index = match decision.acting_player {
            PlayerSeatV1::P0 => 0,
            PlayerSeatV1::P1 => 1,
        };
        let global_completed = self.counters.physical_by_seat[0]
            .checked_add(self.counters.physical_by_seat[1])
            .ok_or_else(|| "policy physical counter total overflow".to_string())?;
        if decision.physical_decision_id != global_completed {
            return Err("global physical id disagrees with per-seat counters".into());
        }
        if decision.substep_count == 0 || decision.substep_index >= decision.substep_count {
            return Err("decision has invalid physical substep metadata".into());
        }
        let local_physical_index = self.counters.physical_by_seat[seat_index];
        let (group_seed, pending_choice) = if decision.substep_index == 0 {
            if self.pending.is_some() || local_physical_index == UINT63_MAX {
                return Err("new physical group overlaps pending state or exhausts uint63".into());
            }
            let group_seed = derive_group_seed(
                self.base_seed,
                self.episode_id,
                local_physical_index,
                decision.acting_player,
            )?;
            let pending_choice = match decision.decision_kind {
                FastActorDecisionKindV1::Surface => {
                    if decision.substep_count != 1 || decision.legal_action_count == 0 {
                        return Err("surface decision has invalid group or legal count".into());
                    }
                    None
                }
                FastActorDecisionKindV1::AttackerInclusion => {
                    if decision.legal_action_count != 2 {
                        return Err("attacker inclusion is not a binary menu".into());
                    }
                    Some(PendingChoiceV1::Attackers)
                }
                FastActorDecisionKindV1::BlockerInclusion => {
                    if decision.legal_action_count != 2 {
                        return Err("blocker inclusion is not a binary menu".into());
                    }
                    let gate = derive_leaf_seed(group_seed, 0);
                    self.counters.leaf_evaluations = self
                        .counters
                        .leaf_evaluations
                        .checked_add(1)
                        .ok_or_else(|| "leaf counter overflow".to_string())?;
                    let selected_rank = if gate % 100 < 35 {
                        self.counters.leaf_evaluations = self
                            .counters
                            .leaf_evaluations
                            .checked_add(1)
                            .ok_or_else(|| "leaf counter overflow".to_string())?;
                        Some(unsigned_modulo(
                            derive_leaf_seed(group_seed, 1),
                            decision.substep_count as usize,
                        )?)
                    } else {
                        None
                    };
                    self.counters.xmage_action_selections = self
                        .counters
                        .xmage_action_selections
                        .checked_add(1)
                        .ok_or_else(|| "policy selection counter overflow".to_string())?;
                    Some(PendingChoiceV1::Blocker { selected_rank })
                }
            };
            if let Some(choice) = pending_choice {
                self.pending = Some(PendingGroupV1 {
                    global_physical_id: decision.physical_decision_id,
                    actor: decision.acting_player,
                    local_physical_index,
                    group_seed,
                    substep_count: decision.substep_count,
                    choice,
                });
            }
            (group_seed, pending_choice)
        } else {
            let pending = self
                .pending
                .ok_or_else(|| "continuing substep has no pending physical group".to_string())?;
            if pending.global_physical_id != decision.physical_decision_id
                || pending.actor != decision.acting_player
                || pending.local_physical_index != local_physical_index
                || pending.substep_count != decision.substep_count
            {
                return Err("continuing substep does not match pending physical group".into());
            }
            let expected_kind = match pending.choice {
                PendingChoiceV1::Attackers => FastActorDecisionKindV1::AttackerInclusion,
                PendingChoiceV1::Blocker { .. } => FastActorDecisionKindV1::BlockerInclusion,
            };
            if decision.decision_kind != expected_kind || decision.legal_action_count != 2 {
                return Err("continuing substep changed decision kind or action shape".into());
            }
            (pending.group_seed, Some(pending.choice))
        };

        let (selected_index, included) = match decision.decision_kind {
            FastActorDecisionKindV1::Surface => {
                self.counters.leaf_evaluations = self
                    .counters
                    .leaf_evaluations
                    .checked_add(1)
                    .ok_or_else(|| "leaf counter overflow".to_string())?;
                self.counters.xmage_action_selections = self
                    .counters
                    .xmage_action_selections
                    .checked_add(1)
                    .ok_or_else(|| "policy selection counter overflow".to_string())?;
                (
                    u32::try_from(unsigned_modulo(
                        derive_leaf_seed(group_seed, 0),
                        decision.legal_action_count as usize,
                    )?)
                    .map_err(|_| "selected action index exceeds u32".to_string())?,
                    None,
                )
            }
            FastActorDecisionKindV1::AttackerInclusion => {
                self.counters.leaf_evaluations = self
                    .counters
                    .leaf_evaluations
                    .checked_add(1)
                    .ok_or_else(|| "leaf counter overflow".to_string())?;
                self.counters.xmage_action_selections = self
                    .counters
                    .xmage_action_selections
                    .checked_add(1)
                    .ok_or_else(|| "policy selection counter overflow".to_string())?;
                let include = derive_leaf_seed(group_seed, decision.substep_index) % 2 == 1;
                (u32::from(include), Some(include))
            }
            FastActorDecisionKindV1::BlockerInclusion => {
                let Some(PendingChoiceV1::Blocker { selected_rank }) = pending_choice else {
                    return Err("blocker substep lost its aggregate sample".into());
                };
                let include = selected_rank == Some(decision.substep_index as usize);
                (u32::from(include), Some(include))
            }
        };
        self.counters.surface_steps = self
            .counters
            .surface_steps
            .checked_add(1)
            .ok_or_else(|| "surface step counter overflow".to_string())?;

        if decision.substep_index + 1 == decision.substep_count {
            self.counters.physical_by_seat[seat_index] = self.counters.physical_by_seat[seat_index]
                .checked_add(1)
                .ok_or_else(|| "physical counter overflow".to_string())?;
            self.pending = None;
        }
        Ok(PolicySelectionV1 {
            selected_index,
            included,
            actor_local_physical_index: local_physical_index,
            group_seed,
        })
    }

    fn finish(
        self,
        policy_steps: u64,
        physical_decisions: u64,
    ) -> Result<PolicyCountersV1, String> {
        let total = self.counters.physical_by_seat[0]
            .checked_add(self.counters.physical_by_seat[1])
            .ok_or_else(|| "policy physical total overflow".to_string())?;
        if self.pending.is_some()
            || self.counters.surface_steps != policy_steps
            || total != physical_decisions
        {
            return Err("terminal policy counters disagree with the session".into());
        }
        Ok(self.counters)
    }
}

fn decision_kind(decision: &RlSessionDecisionV1) -> Result<FastActorDecisionKindV1, String> {
    match decision
        .legal_actions
        .first()
        .map(|action| &action.semantic)
    {
        Some(ActionSemanticV1::ChooseAttackerInclusion { .. }) => {
            Ok(FastActorDecisionKindV1::AttackerInclusion)
        }
        Some(ActionSemanticV1::ChooseBlockerInclusion { .. }) => {
            Ok(FastActorDecisionKindV1::BlockerInclusion)
        }
        Some(_) => Ok(FastActorDecisionKindV1::Surface),
        None => Err("full-v5 decision has zero legal actions".into()),
    }
}

fn fast_shape(decision: &RlSessionDecisionV1) -> Result<FastActorDecisionV1, String> {
    Ok(FastActorDecisionV1 {
        episode_id: decision.episode_id,
        step: decision.step,
        environment_revision: decision.step,
        physical_decision_id: decision.physical_decision_id,
        substep_index: decision.substep_index,
        substep_count: decision.substep_count,
        acting_player: decision.acting_player,
        decision_kind: decision_kind(decision)?,
        legal_action_count: u32::try_from(decision.legal_actions.len())
            .map_err(|_| "full-v5 legal action count exceeds u32".to_string())?,
    })
}

fn validate_selected_semantic(
    decision: &RlSessionDecisionV1,
    selection: PolicySelectionV1,
) -> Result<(), String> {
    let action = decision
        .legal_actions
        .get(selection.selected_index as usize)
        .ok_or_else(|| "policy selected an out-of-range full-v5 action".to_string())?;
    if action.selected_index != selection.selected_index {
        return Err("full-v5 selected indices are not dense canonical ranks".into());
    }
    if let Some(expected) = selection.included {
        let observed = match action.semantic {
            ActionSemanticV1::ChooseAttackerInclusion { include, .. }
            | ActionSemanticV1::ChooseBlockerInclusion { include, .. } => include,
            _ => return Err("binary combat selection resolved to a noncombat semantic".into()),
        };
        if observed != expected {
            return Err("combat Boolean action order differs from exclude-then-include".into());
        }
    }
    Ok(())
}

fn deck_ids(deck: &'static RuntimeDeckDefinition) -> SessionDeckIdsV1 {
    [deck.id.to_string(), deck.id.to_string()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum EpisodeStatusV1 {
    NaturalTerminal,
    PhysicalDecisionCap,
    PolicyStepCap,
    Halted,
    FailClosed,
    DriverError,
}

fn terminal_status(terminal: &RlSessionTerminalV1) -> EpisodeStatusV1 {
    match terminal.terminal_classification {
        TerminalClassificationV1::Natural => EpisodeStatusV1::NaturalTerminal,
        TerminalClassificationV1::Truncated
            if terminal
                .terminal_reason
                .starts_with("physical_decision_cap_reached:") =>
        {
            EpisodeStatusV1::PhysicalDecisionCap
        }
        TerminalClassificationV1::Truncated
            if terminal
                .terminal_reason
                .starts_with("policy_step_cap_reached:") =>
        {
            EpisodeStatusV1::PolicyStepCap
        }
        TerminalClassificationV1::Halted
            if terminal.terminal_reason.starts_with("fail_closed:") =>
        {
            EpisodeStatusV1::FailClosed
        }
        TerminalClassificationV1::Halted => EpisodeStatusV1::Halted,
        TerminalClassificationV1::Truncated => EpisodeStatusV1::DriverError,
    }
}

#[derive(Debug)]
struct EpisodeResultV1 {
    status: EpisodeStatusV1,
    terminal_outcome: Option<TerminalOutcomeV1>,
    policy_steps: u64,
    physical_decisions: u64,
    physical_by_seat: [u64; 2],
    policy_action_selections: u64,
    policy_leaf_evaluations: u64,
}

fn driver_error(policy_steps: u64, physical_decisions: u64) -> EpisodeResultV1 {
    EpisodeResultV1 {
        status: EpisodeStatusV1::DriverError,
        terminal_outcome: None,
        policy_steps,
        physical_decisions,
        physical_by_seat: [0, 0],
        policy_action_selections: 0,
        policy_leaf_evaluations: 0,
    }
}

fn play_fast_episode(
    deck: &'static RuntimeDeckDefinition,
    base_seed: u64,
    episode: u64,
) -> EpisodeResultV1 {
    let env_seed = match derive_env_seed(base_seed, episode) {
        Ok(seed) => seed,
        Err(_) => return driver_error(0, 0),
    };
    let mut session = match FastActorSessionV1::reset_with_decks_and_limits(
        episode,
        env_seed,
        DECISION_SAFETY_CAP,
        DECISION_SAFETY_CAP.saturating_mul(128),
        deck_ids(deck),
    ) {
        Ok(session) => session,
        Err(_) => return driver_error(0, 0),
    };
    let mut policy = match MatchedUniformPolicyV2::new(base_seed, episode) {
        Ok(policy) => policy,
        Err(_) => return driver_error(0, 0),
    };
    loop {
        match session.current_response() {
            FastActorResponseV1::Decision(decision) => {
                let selection = match policy.select(decision) {
                    Ok(selection) => selection,
                    Err(_) => {
                        return driver_error(
                            session.policy_step_count(),
                            session.physical_decision_count(),
                        )
                    }
                };
                if session
                    .step(episode, decision.step, selection.selected_index)
                    .is_err()
                {
                    return driver_error(
                        session.policy_step_count(),
                        session.physical_decision_count(),
                    );
                }
            }
            FastActorResponseV1::Terminal(terminal) => {
                let counters = match policy
                    .finish(terminal.policy_step_count, terminal.physical_decision_count)
                {
                    Ok(counters) => counters,
                    Err(_) => {
                        return driver_error(
                            terminal.policy_step_count,
                            terminal.physical_decision_count,
                        )
                    }
                };
                return EpisodeResultV1 {
                    status: terminal_status(&terminal),
                    terminal_outcome: Some(terminal.terminal_outcome),
                    policy_steps: terminal.policy_step_count,
                    physical_decisions: terminal.physical_decision_count,
                    physical_by_seat: counters.physical_by_seat,
                    policy_action_selections: counters.xmage_action_selections,
                    policy_leaf_evaluations: counters.leaf_evaluations,
                };
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
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
    fn record(&mut self, status: EpisodeStatusV1) {
        self.attempted_games += 1;
        match status {
            EpisodeStatusV1::NaturalTerminal => self.natural_terminal_games += 1,
            EpisodeStatusV1::PhysicalDecisionCap => self.physical_decision_cap_games += 1,
            EpisodeStatusV1::PolicyStepCap => self.policy_step_cap_games += 1,
            EpisodeStatusV1::Halted => self.halted_games += 1,
            EpisodeStatusV1::FailClosed => self.fail_closed_games += 1,
            EpisodeStatusV1::DriverError => self.driver_error_games += 1,
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

    fn is_exact_natural(self, expected: u64) -> bool {
        self.attempted_games == expected
            && self.natural_terminal_games == expected
            && self.physical_decision_cap_games == 0
            && self.policy_step_cap_games == 0
            && self.halted_games == 0
            && self.fail_closed_games == 0
            && self.driver_error_games == 0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
struct NaturalOutcomesV1 {
    p0_wins: u64,
    p1_wins: u64,
    draws: u64,
}

impl NaturalOutcomesV1 {
    fn record(&mut self, outcome: Option<TerminalOutcomeV1>) {
        match outcome {
            Some(TerminalOutcomeV1::P0Win) => self.p0_wins += 1,
            Some(TerminalOutcomeV1::P1Win) => self.p1_wins += 1,
            Some(TerminalOutcomeV1::Draw) => self.draws += 1,
            Some(TerminalOutcomeV1::Truncated | TerminalOutcomeV1::Halted) | None => {}
        }
    }

    fn merge(&mut self, other: Self) {
        self.p0_wins += other.p0_wins;
        self.p1_wins += other.p1_wins;
        self.draws += other.draws;
    }

    const fn total(self) -> u64 {
        self.p0_wins + self.p1_wins + self.draws
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PhaseTotalsV1 {
    outcomes: OutcomeCountsV1,
    natural_outcomes: NaturalOutcomesV1,
    policy_steps: u64,
    physical_decisions: u64,
    physical_by_seat: [u64; 2],
    policy_action_selections: u64,
    policy_leaf_evaluations: u64,
}

impl PhaseTotalsV1 {
    fn record(&mut self, result: EpisodeResultV1) {
        self.outcomes.record(result.status);
        if result.status == EpisodeStatusV1::NaturalTerminal {
            self.natural_outcomes.record(result.terminal_outcome);
        }
        self.policy_steps += result.policy_steps;
        self.physical_decisions += result.physical_decisions;
        self.physical_by_seat[0] += result.physical_by_seat[0];
        self.physical_by_seat[1] += result.physical_by_seat[1];
        self.policy_action_selections += result.policy_action_selections;
        self.policy_leaf_evaluations += result.policy_leaf_evaluations;
    }

    fn merge(&mut self, other: Self) {
        self.outcomes.merge(other.outcomes);
        self.natural_outcomes.merge(other.natural_outcomes);
        self.policy_steps += other.policy_steps;
        self.physical_decisions += other.physical_decisions;
        self.physical_by_seat[0] += other.physical_by_seat[0];
        self.physical_by_seat[1] += other.physical_by_seat[1];
        self.policy_action_selections += other.policy_action_selections;
        self.policy_leaf_evaluations += other.policy_leaf_evaluations;
    }
}

#[derive(Debug, Serialize)]
struct ActorPhaseV2 {
    actor_index: usize,
    attempted: u64,
    natural_completions: u64,
    first_episode_id: Option<u64>,
    last_episode_id: Option<u64>,
    finish_offset_ns_from_common_start: u64,
    in_flight_at_deadline_finished_naturally: u64,
}

#[derive(Debug, Serialize)]
struct PhaseV2 {
    requested_wall_ns: u64,
    elapsed_slowest_actor_ns: u64,
    attempted_games: u64,
    natural_completions: u64,
    games_with_any_invalidity: u64,
    outcomes: OutcomeCountsV1,
    natural_outcomes: NaturalOutcomesV1,
    policy_steps: u64,
    physical_decisions: u64,
    physical_decisions_by_seat: [u64; 2],
    policy_action_selections: u64,
    policy_leaf_evaluations: u64,
    in_flight_at_deadline_finished_naturally: u64,
    unfinished_after_join: u64,
    actors: Vec<ActorPhaseV2>,
    all_attempted_games_finished_naturally: bool,
}

#[derive(Debug, Serialize)]
struct RatesV2 {
    denominator_seconds: f64,
    natural_games_per_second: f64,
    policy_steps_per_second: f64,
    physical_decisions_per_second: f64,
    policy_action_selections_per_second: f64,
    policy_leaf_evaluations_per_second: f64,
}

#[derive(Debug)]
struct TimedTrialV2 {
    warmup: PhaseV2,
    measurement: PhaseV2,
    rates: RatesV2,
}

#[derive(Debug, Clone, Copy)]
struct PhaseClockV2 {
    start: Instant,
    deadline: Instant,
}

struct WorkerPhaseV2 {
    actor: usize,
    totals: PhaseTotalsV1,
    first_episode_id: Option<u64>,
    last_episode_id: Option<u64>,
    finish_offset_ns: u64,
    tail_completions: u64,
}

fn elapsed_ns(start: Instant) -> Result<u64, String> {
    u64::try_from(start.elapsed().as_nanos())
        .map_err(|_| "phase elapsed time exceeded u64 nanoseconds".to_string())
}

fn run_duration_phase(
    deck: &'static RuntimeDeckDefinition,
    actors: usize,
    requested_ns: u64,
    phase: TimedEpisodePhaseV1,
    base_seed: u64,
    require_nonzero_games: bool,
) -> Result<PhaseV2, String> {
    if !MATCHED_ACTOR_COUNTS.contains(&actors) {
        return Err("duration phase actor count must be exactly one of 1,4,8,16".into());
    }
    if requested_ns == 0 || requested_ns > i64::MAX as u64 {
        return Err("duration phase requires positive exact signed nanoseconds".into());
    }
    let requested = Duration::from_nanos(requested_ns);
    let ready = Arc::new(Barrier::new(actors + 1));
    let start_gate = Arc::new(Barrier::new(actors + 1));
    let common_clock = Arc::new(OnceLock::<PhaseClockV2>::new());
    let handles: Vec<_> = (0..actors)
        .map(|actor| {
            let ready = Arc::clone(&ready);
            let start_gate = Arc::clone(&start_gate);
            let common_clock = Arc::clone(&common_clock);
            std::thread::spawn(move || -> Result<WorkerPhaseV2, String> {
                ready.wait();
                start_gate.wait();
                let clock = *common_clock
                    .get()
                    .ok_or_else(|| "main did not establish the common phase clock".to_string())?;
                let mut totals = PhaseTotalsV1::default();
                let mut local_index = 0u64;
                let mut first_episode_id = None;
                let mut last_episode_id = None;
                let mut tail_completions = 0u64;
                loop {
                    if Instant::now() >= clock.deadline {
                        break;
                    }
                    let episode = striped_timed_episode_id(phase, local_index, actor, actors)?;
                    if first_episode_id.is_none() {
                        first_episode_id = Some(episode);
                    }
                    last_episode_id = Some(episode);
                    let result = play_fast_episode(deck, base_seed, episode);
                    let finished_after_deadline = Instant::now() > clock.deadline;
                    let natural = result.status == EpisodeStatusV1::NaturalTerminal;
                    totals.record(result);
                    if finished_after_deadline && natural {
                        tail_completions = tail_completions
                            .checked_add(1)
                            .ok_or_else(|| "tail completion counter overflow".to_string())?;
                    }
                    local_index = local_index
                        .checked_add(1)
                        .ok_or_else(|| "actor local episode counter overflow".to_string())?;
                }
                Ok(WorkerPhaseV2 {
                    actor,
                    totals,
                    first_episode_id,
                    last_episode_id,
                    finish_offset_ns: elapsed_ns(clock.start)?,
                    tail_completions,
                })
            })
        })
        .collect();

    ready.wait();
    let start = Instant::now();
    let deadline = start
        .checked_add(requested)
        .ok_or_else(|| "phase deadline exceeded Instant range".to_string())?;
    common_clock
        .set(PhaseClockV2 { start, deadline })
        .map_err(|_| "common phase clock was installed more than once".to_string())?;
    start_gate.wait();

    let mut totals = PhaseTotalsV1::default();
    let mut actor_records = Vec::with_capacity(actors);
    let mut aggregate_attempted = 0u64;
    let mut aggregate_tails = 0u64;
    let mut elapsed_slowest_actor_ns = 0u64;
    for handle in handles {
        let worker = handle
            .join()
            .map_err(|_| "matched-uniform duration worker thread panicked".to_string())??;
        let attempted = worker.totals.outcomes.attempted_games;
        let natural = worker.totals.outcomes.natural_terminal_games;
        let expected_first = (attempted > 0).then(|| {
            striped_timed_episode_id(phase, 0, worker.actor, actors)
                .expect("validated duration actor coordinates")
        });
        let expected_last = attempted.checked_sub(1).map(|last_index| {
            striped_timed_episode_id(phase, last_index, worker.actor, actors)
                .expect("launched duration episode remained in its uint63 partition")
        });
        if worker.first_episode_id != expected_first
            || worker.last_episode_id != expected_last
            || !worker.totals.outcomes.is_exact_natural(attempted)
            || worker.totals.natural_outcomes.total() != attempted
            || worker.totals.physical_by_seat[0] + worker.totals.physical_by_seat[1]
                != worker.totals.physical_decisions
            || worker.tail_completions > attempted
        {
            return Err("duration phase refused inconsistent per-actor outcomes".into());
        }
        aggregate_attempted = aggregate_attempted
            .checked_add(attempted)
            .ok_or_else(|| "duration phase attempted-game total overflow".to_string())?;
        aggregate_tails = aggregate_tails
            .checked_add(worker.tail_completions)
            .ok_or_else(|| "duration phase tail total overflow".to_string())?;
        elapsed_slowest_actor_ns = elapsed_slowest_actor_ns.max(worker.finish_offset_ns);
        actor_records.push(ActorPhaseV2 {
            actor_index: worker.actor,
            attempted,
            natural_completions: natural,
            first_episode_id: worker.first_episode_id,
            last_episode_id: worker.last_episode_id,
            finish_offset_ns_from_common_start: worker.finish_offset_ns,
            in_flight_at_deadline_finished_naturally: worker.tail_completions,
        });
        totals.merge(worker.totals);
    }
    actor_records.sort_by_key(|actor| actor.actor_index);
    if (require_nonzero_games && aggregate_attempted == 0)
        || !totals.outcomes.is_exact_natural(aggregate_attempted)
        || totals.natural_outcomes.total() != aggregate_attempted
        || totals.physical_by_seat[0] + totals.physical_by_seat[1] != totals.physical_decisions
        || elapsed_slowest_actor_ns == 0
    {
        return Err("duration phase refused zero, non-natural, or inconsistent outcomes".into());
    }
    Ok(PhaseV2 {
        requested_wall_ns: requested_ns,
        elapsed_slowest_actor_ns,
        attempted_games: aggregate_attempted,
        natural_completions: totals.outcomes.natural_terminal_games,
        games_with_any_invalidity: 0,
        outcomes: totals.outcomes,
        natural_outcomes: totals.natural_outcomes,
        policy_steps: totals.policy_steps,
        physical_decisions: totals.physical_decisions,
        physical_decisions_by_seat: totals.physical_by_seat,
        policy_action_selections: totals.policy_action_selections,
        policy_leaf_evaluations: totals.policy_leaf_evaluations,
        in_flight_at_deadline_finished_naturally: aggregate_tails,
        unfinished_after_join: 0,
        actors: actor_records,
        all_attempted_games_finished_naturally: true,
    })
}

fn measure_duration_trial(
    deck: &'static RuntimeDeckDefinition,
    actors: usize,
    warmup_ns: u64,
    measure_ns: u64,
    base_seed: u64,
) -> Result<TimedTrialV2, String> {
    let warmup = run_duration_phase(
        deck,
        actors,
        warmup_ns,
        TimedEpisodePhaseV1::Warmup,
        base_seed,
        false,
    )?;
    let measurement = run_duration_phase(
        deck,
        actors,
        measure_ns,
        TimedEpisodePhaseV1::Measurement,
        base_seed,
        true,
    )?;
    let seconds = measurement.elapsed_slowest_actor_ns as f64 / NANOS_PER_SECOND as f64;
    let rates = RatesV2 {
        denominator_seconds: seconds,
        natural_games_per_second: measurement.outcomes.natural_terminal_games as f64 / seconds,
        policy_steps_per_second: measurement.policy_steps as f64 / seconds,
        physical_decisions_per_second: measurement.physical_decisions as f64 / seconds,
        policy_action_selections_per_second: measurement.policy_action_selections as f64 / seconds,
        policy_leaf_evaluations_per_second: measurement.policy_leaf_evaluations as f64 / seconds,
    };
    if !rates.denominator_seconds.is_finite()
        || !rates.natural_games_per_second.is_finite()
        || !rates.policy_steps_per_second.is_finite()
        || !rates.physical_decisions_per_second.is_finite()
        || !rates.policy_action_selections_per_second.is_finite()
        || !rates.policy_leaf_evaluations_per_second.is_finite()
    {
        return Err("duration trial refused non-finite absolute rates".into());
    }
    Ok(TimedTrialV2 {
        warmup,
        measurement,
        rates,
    })
}

fn fnv1a64_continue(mut hash: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn fnv_hex(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a64_continue(0xcbf29ce484222325, bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn normalize_source_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    std::str::from_utf8(bytes)
        .map_err(|_| "embedded source component was not UTF-8".to_string())?;
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            if bytes.get(index + 1) != Some(&b'\n') {
                return Err("embedded source component contained a bare carriage return".into());
            }
            normalized.push(b'\n');
            index += 2;
        } else {
            normalized.push(bytes[index]);
            index += 1;
        }
    }
    Ok(normalized)
}

fn semantic_category(semantic: &ActionSemanticV1) -> &'static str {
    match semantic {
        ActionSemanticV1::Pass { .. } => "pass",
        ActionSemanticV1::PlayLand { .. } => "play_land",
        ActionSemanticV1::CastSpell { .. } => "cast_spell",
        ActionSemanticV1::ActivateManaAbility { .. } => "activate_mana_ability",
        ActionSemanticV1::ActivateAbility { .. } => "activate_ability",
        ActionSemanticV1::PlotSpell { .. } => "plot_spell",
        ActionSemanticV1::ChooseTarget { .. } => "choose_target",
        ActionSemanticV1::ChooseCostTarget { .. } => "choose_cost_target",
        ActionSemanticV1::ChooseCastMode { .. } => "choose_cast_mode",
        ActionSemanticV1::ChooseKicker { .. } => "choose_kicker",
        ActionSemanticV1::ChooseSpellMode { .. } => "choose_spell_mode",
        ActionSemanticV1::ChooseEffectOption { .. } => "choose_effect_option",
        ActionSemanticV1::ChooseEffectTarget { .. } => "choose_effect_target",
        ActionSemanticV1::FinishEffectSelection { .. } => "finish_effect_selection",
        ActionSemanticV1::ChooseEffectColor { .. } => "choose_effect_color",
        ActionSemanticV1::ChooseEffectNumber { .. } => "choose_effect_number",
        ActionSemanticV1::ChooseEffectBoolean { .. } => "choose_effect_boolean",
        ActionSemanticV1::FinishTargetSelection { .. } => "finish_target_selection",
        ActionSemanticV1::ChooseOptionalCostUse { .. } => "choose_optional_cost_use",
        ActionSemanticV1::ChooseOptionalCostWhich { .. } => "choose_optional_cost_which",
        ActionSemanticV1::ChooseSpellCopyPayment { .. } => "choose_spell_copy_payment",
        ActionSemanticV1::ChooseSpellCopyRetarget { .. } => "choose_spell_copy_retarget",
        ActionSemanticV1::ChooseMadnessCast { .. } => "choose_madness_cast",
        ActionSemanticV1::Discard { .. } => "discard",
        ActionSemanticV1::DeclareAttackers { .. } => "declare_attackers_legacy",
        ActionSemanticV1::DeclareBlockersForAttacker { .. } => {
            "declare_blockers_for_attacker_legacy"
        }
        ActionSemanticV1::ChooseAttackerInclusion { .. } => "declare_attackers",
        ActionSemanticV1::ChooseBlockerInclusion { .. } => "declare_blocker_for_attacker",
        ActionSemanticV1::OrderTriggers { .. } => "order_triggers",
        ActionSemanticV1::Ambiguous { .. } => "ambiguous",
    }
}

#[derive(Debug, Serialize)]
struct TranscriptDecisionV1 {
    episode_id: u64,
    actor: PlayerSeatV1,
    actor_local_physical_decision_index: u64,
    global_policy_step: u64,
    global_physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    decision_category: &'static str,
    ordered_legal_semantics_fnv1a64: String,
    ordered_stable_action_ids_fnv1a64: String,
    legal_count: u32,
    selected_index: u32,
    selected_semantic_fnv1a64: String,
    group_seed_hex: String,
    core_state_hash_before_hex: String,
}

#[derive(Debug, Serialize)]
struct TranscriptEpisodeV1 {
    episode_id: u64,
    env_seed_hex: String,
    decisions: Vec<TranscriptDecisionV1>,
    terminal_outcome: TerminalOutcomeV1,
    winner: Option<PlayerSeatV1>,
    policy_steps: u64,
    physical_decisions: u64,
    physical_decisions_by_seat: [u64; 2],
    terminal_core_state_hash_hex: String,
}

#[derive(Debug, Serialize)]
struct DeckValidationV1 {
    deck_id: &'static str,
    runtime_deck_hash: u64,
    episode_ids: Vec<u64>,
    paired_games: u64,
    all_outcomes_natural: bool,
    zero_invalids: bool,
    exact_metadata_semantic_order_selection_counter_state_and_terminal_parity: bool,
    policy_steps_compared: u64,
    physical_decisions_compared: u64,
    physical_decisions_by_seat: [u64; 2],
    semantic_categories_seen: BTreeMap<String, u64>,
    trajectory_digest_fnv1a64_hex: String,
    transcripts: Vec<TranscriptEpisodeV1>,
}

fn validate_deck(
    deck: &'static RuntimeDeckDefinition,
    fixed_episode_ids: &[u64],
    transcript_games: u64,
) -> Result<DeckValidationV1, String> {
    let mut episode_ids = Vec::new();
    let mut policy_steps_compared = 0u64;
    let mut physical_decisions_compared = 0u64;
    let mut physical_by_seat = [0u64; 2];
    let mut categories = BTreeMap::<String, u64>::new();
    let mut trajectory_digest = 0xcbf29ce484222325u64;
    let mut transcripts = Vec::new();
    if fixed_episode_ids.len()
        != usize::try_from(VALIDATION_GAMES_PER_DECK)
            .map_err(|_| "validation game count exceeds usize".to_string())?
    {
        return Err("fixed validation episode count does not match schema contract".into());
    }
    for (game_index, &episode) in (0u64..).zip(fixed_episode_ids.iter()) {
        if episode > UINT63_MAX {
            return Err("fixed validation episode escaped uint63".into());
        }
        episode_ids.push(episode);
        let env_seed = derive_env_seed(VALIDATION_BASE_SEED, episode)?;
        let mut full = RlEpisodeSessionV1::reset_with_decks_and_limits(
            episode,
            env_seed,
            DECISION_SAFETY_CAP,
            DECISION_SAFETY_CAP.saturating_mul(128),
            deck_ids(deck),
        )
        .map_err(|error| error.to_string())?;
        let mut fast = FastActorSessionV1::reset_with_decks_and_limits(
            episode,
            env_seed,
            DECISION_SAFETY_CAP,
            DECISION_SAFETY_CAP.saturating_mul(128),
            deck_ids(deck),
        )
        .map_err(|error| error.to_string())?;
        let mut full_policy = MatchedUniformPolicyV2::new(VALIDATION_BASE_SEED, episode)?;
        let mut fast_policy = MatchedUniformPolicyV2::new(VALIDATION_BASE_SEED, episode)?;
        let mut transcript_decisions = Vec::new();
        loop {
            let full_core_hash = full.privileged_core_environment_hash();
            let fast_core_hash = fast.privileged_core_environment_hash();
            if full_core_hash != fast_core_hash
                || full.diagnostic_state_hash() != fast.diagnostic_state_hash()
            {
                return Err(format!(
                    "{0} validation core/state hash mismatch in episode {episode}",
                    deck.id
                ));
            }
            trajectory_digest = fnv1a64_continue(trajectory_digest, &full_core_hash.to_le_bytes());
            match (full.current_response(), fast.current_response()) {
                (
                    RlSessionResponseV1::Decision(full_decision),
                    FastActorResponseV1::Decision(fast_decision),
                ) => {
                    let full_shape = fast_shape(&full_decision)?;
                    if full_shape != fast_decision {
                        return Err(format!(
                            "{} validation decision metadata mismatch in episode {episode}",
                            deck.id
                        ));
                    }
                    let fast_semantics = fast
                        .diagnostic_current_action_semantics()
                        .ok_or_else(|| "fast actor lost current semantic audit view".to_string())?;
                    let full_semantics: Vec<_> = full_decision
                        .legal_actions
                        .iter()
                        .map(|action| action.semantic.clone())
                        .collect();
                    if full_semantics != fast_semantics {
                        return Err(format!(
                            "{} validation semantic action order mismatch in episode {episode}",
                            deck.id
                        ));
                    }
                    for (rank, action) in full_decision.legal_actions.iter().enumerate() {
                        if action.selected_index as usize != rank {
                            return Err("full-v5 canonical ranks are not dense".into());
                        }
                    }
                    let full_selection = full_policy.select(full_shape)?;
                    let fast_selection = fast_policy.select(fast_decision)?;
                    if full_selection != fast_selection {
                        return Err(format!(
                            "{} validation selected index/seed mismatch in episode {episode}",
                            deck.id
                        ));
                    }
                    validate_selected_semantic(&full_decision, full_selection)?;
                    let selected =
                        &full_decision.legal_actions[full_selection.selected_index as usize];
                    let category = semantic_category(&full_decision.legal_actions[0].semantic);
                    *categories.entry(category.to_string()).or_default() += 1;
                    let semantic_bytes =
                        serde_json::to_vec(&full_semantics).map_err(|error| error.to_string())?;
                    let stable_ids: Vec<_> = full_decision
                        .legal_actions
                        .iter()
                        .map(|action| action.stable_id.as_str())
                        .collect();
                    let stable_bytes =
                        serde_json::to_vec(&stable_ids).map_err(|error| error.to_string())?;
                    trajectory_digest = fnv1a64_continue(trajectory_digest, &semantic_bytes);
                    trajectory_digest = fnv1a64_continue(
                        trajectory_digest,
                        &full_selection.selected_index.to_le_bytes(),
                    );
                    if game_index < transcript_games {
                        transcript_decisions.push(TranscriptDecisionV1 {
                            episode_id: episode,
                            actor: full_decision.acting_player,
                            actor_local_physical_decision_index: full_selection
                                .actor_local_physical_index,
                            global_policy_step: full_decision.step,
                            global_physical_decision_id: full_decision.physical_decision_id,
                            substep_index: full_decision.substep_index,
                            substep_count: full_decision.substep_count,
                            decision_category: category,
                            ordered_legal_semantics_fnv1a64: fnv_hex(&semantic_bytes),
                            ordered_stable_action_ids_fnv1a64: fnv_hex(&stable_bytes),
                            legal_count: u32::try_from(full_decision.legal_actions.len())
                                .map_err(|_| "transcript legal count exceeds u32".to_string())?,
                            selected_index: full_selection.selected_index,
                            selected_semantic_fnv1a64: fnv_hex(
                                &serde_json::to_vec(&selected.semantic)
                                    .map_err(|error| error.to_string())?,
                            ),
                            group_seed_hex: format!("{:016x}", full_selection.group_seed),
                            core_state_hash_before_hex: format!("{full_core_hash:016x}"),
                        });
                    }
                    full.step(
                        episode,
                        full_decision.step,
                        full_selection.selected_index,
                        &selected.stable_id,
                    )
                    .map_err(|error| error.to_string())?;
                    fast.step(episode, fast_decision.step, fast_selection.selected_index)
                        .map_err(|error| error.to_string())?;
                    policy_steps_compared += 1;
                }
                (
                    RlSessionResponseV1::Terminal(full_terminal),
                    FastActorResponseV1::Terminal(fast_terminal),
                ) => {
                    if full_terminal != fast_terminal
                        || terminal_status(&full_terminal) != EpisodeStatusV1::NaturalTerminal
                    {
                        return Err(format!(
                            "{} validation terminal mismatch or non-natural outcome in episode {episode}",
                            deck.id
                        ));
                    }
                    let full_counters = full_policy.finish(
                        full_terminal.policy_step_count,
                        full_terminal.physical_decision_count,
                    )?;
                    let fast_counters = fast_policy.finish(
                        fast_terminal.policy_step_count,
                        fast_terminal.physical_decision_count,
                    )?;
                    if full_counters != fast_counters {
                        return Err(format!(
                            "{} validation per-seat policy counters mismatch in episode {episode}",
                            deck.id
                        ));
                    }
                    physical_decisions_compared += full_terminal.physical_decision_count;
                    physical_by_seat[0] += full_counters.physical_by_seat[0];
                    physical_by_seat[1] += full_counters.physical_by_seat[1];
                    if game_index < transcript_games {
                        transcripts.push(TranscriptEpisodeV1 {
                            episode_id: episode,
                            env_seed_hex: format!("{env_seed:016x}"),
                            decisions: transcript_decisions,
                            terminal_outcome: full_terminal.terminal_outcome,
                            winner: full_terminal.winner,
                            policy_steps: full_terminal.policy_step_count,
                            physical_decisions: full_terminal.physical_decision_count,
                            physical_decisions_by_seat: full_counters.physical_by_seat,
                            terminal_core_state_hash_hex: format!(
                                "{:016x}",
                                full.privileged_core_environment_hash()
                            ),
                        });
                    }
                    break;
                }
                _ => {
                    return Err(format!(
                        "{} validation full/fast terminal-state mismatch in episode {episode}",
                        deck.id
                    ))
                }
            }
        }
    }
    if physical_by_seat[0] + physical_by_seat[1] != physical_decisions_compared {
        return Err("validation per-seat physical counters do not sum to global total".into());
    }
    Ok(DeckValidationV1 {
        deck_id: deck.id,
        runtime_deck_hash: deck.runtime_deck_hash,
        episode_ids,
        paired_games: VALIDATION_GAMES_PER_DECK,
        all_outcomes_natural: true,
        zero_invalids: true,
        exact_metadata_semantic_order_selection_counter_state_and_terminal_parity: true,
        policy_steps_compared,
        physical_decisions_compared,
        physical_decisions_by_seat: physical_by_seat,
        semantic_categories_seen: categories,
        trajectory_digest_fnv1a64_hex: format!("{trajectory_digest:016x}"),
        transcripts,
    })
}

#[derive(Debug, Serialize)]
struct ValidationSuiteV1 {
    base_seed: u64,
    games_per_deck: u64,
    transcript_games_per_deck: u64,
    full_v5_is_untimed_oracle: bool,
    burn: DeckValidationV1,
    rally: DeckValidationV1,
}

fn validation_suite(transcript_games: u64) -> Result<ValidationSuiteV1, String> {
    let burn = validate_deck(
        runtime_deck_by_id("Burn").expect("validated Burn runtime deck"),
        &VALIDATION_BURN_EPISODE_IDS,
        transcript_games,
    )?;
    let rally = validate_deck(
        runtime_deck_by_id("Rally").expect("validated Rally runtime deck"),
        &VALIDATION_RALLY_EPISODE_IDS,
        transcript_games,
    )?;
    for required in ["order_triggers"] {
        if !burn.semantic_categories_seen.contains_key(required) {
            return Err(format!(
                "fixed Burn validation did not cover required category {required}"
            ));
        }
    }
    for required in [
        "activate_mana_ability",
        "cast_spell",
        "choose_kicker",
        "choose_spell_copy_payment",
        "choose_target",
        "declare_attackers",
        "declare_blocker_for_attacker",
        "discard",
        "play_land",
    ] {
        if !rally.semantic_categories_seen.contains_key(required) {
            return Err(format!(
                "fixed Rally validation did not cover required category {required}"
            ));
        }
    }
    Ok(ValidationSuiteV1 {
        base_seed: VALIDATION_BASE_SEED,
        games_per_deck: VALIDATION_GAMES_PER_DECK,
        transcript_games_per_deck: transcript_games,
        full_v5_is_untimed_oracle: true,
        burn,
        rally,
    })
}

const SEVEN_COMPONENT_EXACT_MATCH_CONTRACT: &str =
    "utf8_crlf_to_lf_reject_bare_cr_exact_bytes_with_sha256_diagnostics/v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SourceComponentV2 {
    component: &'static str,
    normalized_sha256: String,
}

#[derive(Clone, Copy)]
struct EmbeddedComponentSpecV2<'a> {
    component: &'static str,
    repo_relative_path: &'static str,
    embedded_bytes: &'a [u8],
}

fn seven_embedded_component_specs() -> Vec<EmbeddedComponentSpecV2<'static>> {
    vec![
        EmbeddedComponentSpecV2 {
            component: "matched_uniform_runtime",
            repo_relative_path: "mtg-kernel/examples/bench_kernel/matched_uniform_runtime.rs",
            embedded_bytes: include_bytes!("matched_uniform_runtime.rs").as_slice(),
        },
        EmbeddedComponentSpecV2 {
            component: "bench_kernel_entrypoint",
            repo_relative_path: "mtg-kernel/examples/bench_kernel.rs",
            embedded_bytes: include_bytes!("../bench_kernel.rs").as_slice(),
        },
        EmbeddedComponentSpecV2 {
            component: "rl_session",
            repo_relative_path: "mtg-kernel/src/rl_session.rs",
            embedded_bytes: include_bytes!("../../src/rl_session.rs").as_slice(),
        },
        EmbeddedComponentSpecV2 {
            component: "rl",
            repo_relative_path: "mtg-kernel/src/rl.rs",
            embedded_bytes: include_bytes!("../../src/rl.rs").as_slice(),
        },
        EmbeddedComponentSpecV2 {
            component: "policy_surface_v5",
            repo_relative_path: "mtg-kernel/src/policy_surface_v5.rs",
            embedded_bytes: include_bytes!("../../src/policy_surface_v5.rs").as_slice(),
        },
        EmbeddedComponentSpecV2 {
            component: "surface_v2",
            repo_relative_path: "mtg-kernel/src/surface_v2.rs",
            embedded_bytes: include_bytes!("../../src/surface_v2.rs").as_slice(),
        },
        EmbeddedComponentSpecV2 {
            component: "engine",
            repo_relative_path: "mtg-kernel/src/engine.rs",
            embedded_bytes: include_bytes!("../../src/engine.rs").as_slice(),
        },
    ]
}

fn verify_embedded_component_specs_against_commit(
    expected_commit: &str,
    specs: &[EmbeddedComponentSpecV2<'_>],
) -> Result<Vec<SourceComponentV2>, String> {
    specs
        .iter()
        .map(|spec| {
            let object = format!("{expected_commit}:{}", spec.repo_relative_path);
            let committed = Command::new("git")
                .args(["show", object.as_str()])
                .output()
                .map_err(|_| {
                    format!(
                        "failed to execute git for embedded component {}",
                        spec.component
                    )
                })?;
            if !committed.status.success() {
                return Err(format!(
                    "git could not bind embedded component {} to the expected commit",
                    spec.component
                ));
            }
            let embedded_bytes = normalize_source_bytes(spec.embedded_bytes)
                .map_err(|reason| format!("embedded component {}: {reason}", spec.component))?;
            let committed_bytes = normalize_source_bytes(&committed.stdout)
                .map_err(|reason| format!("committed component {}: {reason}", spec.component))?;
            if embedded_bytes != committed_bytes {
                return Err(format!(
                    "running binary component {} does not match the expected commit",
                    spec.component
                ));
            }
            Ok(SourceComponentV2 {
                component: spec.component,
                normalized_sha256: sha256_hex(&embedded_bytes),
            })
        })
        .collect()
}

fn verify_seven_embedded_components(
    expected_commit: &str,
) -> Result<Vec<SourceComponentV2>, String> {
    verify_embedded_component_specs_against_commit(
        expected_commit,
        &seven_embedded_component_specs(),
    )
}

const TRACKED_TREE_HASH_CONTRACT: &str =
    "git-ls-tree-r-z-path-mode-type-framed-blob-content-or-gitlink-oid-sha256/v1";
const BUILD_GIT_HEAD: &str = env!("MTG_KERNEL_BUILD_GIT_HEAD");
const BUILD_GIT_CLEAN: &str = env!("MTG_KERNEL_BUILD_GIT_CLEAN");
const BUILD_TRACKED_TREE_SHA256: &str = env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256");
const BUILD_TRACKED_TREE_CONTRACT: &str = env!("MTG_KERNEL_BUILD_TRACKED_TREE_CONTRACT");

#[derive(Debug)]
struct GitTreeEntryV3 {
    mode: Vec<u8>,
    kind: Vec<u8>,
    object_id: String,
    path: Vec<u8>,
}

fn git_output_privacy_safe(args: &[&str], operation: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|_| format!("failed to execute git for {operation}"))?;
    if !output.status.success() {
        return Err(format!("git failed during {operation}"));
    }
    Ok(output.stdout)
}

fn parse_runtime_tree_entries(bytes: &[u8]) -> Result<Vec<GitTreeEntryV3>, String> {
    let mut entries = Vec::new();
    for record in bytes
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| "tracked tree record had no path separator".to_string())?;
        let mut metadata = record[..tab].split(|byte| *byte == b' ');
        let mode = metadata
            .next()
            .ok_or_else(|| "tracked tree record had no mode".to_string())?;
        let kind = metadata
            .next()
            .ok_or_else(|| "tracked tree record had no type".to_string())?;
        let object_id = metadata
            .next()
            .ok_or_else(|| "tracked tree record had no object id".to_string())?;
        if metadata.next().is_some()
            || mode.is_empty()
            || kind.is_empty()
            || object_id.is_empty()
            || record[tab + 1..].is_empty()
        {
            return Err("tracked tree record had malformed metadata".into());
        }
        let object_id = std::str::from_utf8(object_id)
            .map_err(|_| "tracked tree object id was not ASCII".to_string())?
            .to_string();
        if !object_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("tracked tree object id was malformed".into());
        }
        entries.push(GitTreeEntryV3 {
            mode: mode.to_vec(),
            kind: kind.to_vec(),
            object_id,
            path: record[tab + 1..].to_vec(),
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    if entries.windows(2).any(|pair| pair[0].path == pair[1].path) {
        return Err("tracked tree contained duplicate paths".into());
    }
    Ok(entries)
}

fn runtime_git_blob_contents(entries: &[GitTreeEntryV3]) -> Result<Vec<Option<Vec<u8>>>, String> {
    let mut child = Command::new("git")
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "failed to execute git cat-file for tracked tree".to_string())?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "git cat-file stdin was not available".to_string())?;
        for entry in entries.iter().filter(|entry| entry.kind == b"blob") {
            writeln!(stdin, "{}", entry.object_id)
                .map_err(|_| "failed to request a tracked blob".to_string())?;
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|_| "git cat-file did not complete for tracked tree".to_string())?;
    if !output.status.success() {
        return Err("git cat-file failed for tracked tree".into());
    }

    let mut cursor = 0usize;
    let mut contents = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.kind == b"commit" {
            if entry.mode != b"160000" {
                return Err("tracked commit entry was not a gitlink".into());
            }
            contents.push(None);
            continue;
        }
        if entry.kind != b"blob" || entry.mode == b"160000" {
            return Err("tracked tree contained an unsupported entry type".into());
        }
        let relative_newline = output.stdout[cursor..]
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or_else(|| "git cat-file response had no header terminator".to_string())?;
        let header_end = cursor + relative_newline;
        let header = std::str::from_utf8(&output.stdout[cursor..header_end])
            .map_err(|_| "git cat-file header was not ASCII".to_string())?;
        let mut fields = header.split(' ');
        let returned_id = fields
            .next()
            .ok_or_else(|| "git cat-file header had no object id".to_string())?;
        let returned_kind = fields
            .next()
            .ok_or_else(|| "git cat-file header had no type".to_string())?;
        let size = fields
            .next()
            .ok_or_else(|| "git cat-file header had no size".to_string())?
            .parse::<usize>()
            .map_err(|_| "git cat-file blob size was invalid".to_string())?;
        if fields.next().is_some() || returned_id != entry.object_id || returned_kind != "blob" {
            return Err("git cat-file returned unexpected blob metadata".into());
        }
        let content_start = header_end + 1;
        let content_end = content_start
            .checked_add(size)
            .ok_or_else(|| "tracked blob bounds overflowed".to_string())?;
        if content_end >= output.stdout.len() || output.stdout[content_end] != b'\n' {
            return Err("git cat-file returned a truncated tracked blob".into());
        }
        contents.push(Some(output.stdout[content_start..content_end].to_vec()));
        cursor = content_end + 1;
    }
    if cursor != output.stdout.len() {
        return Err("git cat-file returned unconsumed blob bytes".into());
    }
    Ok(contents)
}

fn tracked_tree_hash_frame(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn tracked_tree_sha256_at_commit(commit: &str) -> Result<String, String> {
    let listing = git_output_privacy_safe(
        &["ls-tree", "-r", "-z", "--full-tree", commit],
        "tracked tree listing",
    )?;
    let entries = parse_runtime_tree_entries(&listing)?;
    let contents = runtime_git_blob_contents(&entries)?;
    let mut hasher = Sha256::new();
    hasher.update(TRACKED_TREE_HASH_CONTRACT.as_bytes());
    hasher.update([0]);
    hasher.update((entries.len() as u64).to_be_bytes());
    for (entry, content) in entries.iter().zip(contents.iter()) {
        tracked_tree_hash_frame(&mut hasher, &entry.path);
        tracked_tree_hash_frame(&mut hasher, &entry.mode);
        tracked_tree_hash_frame(&mut hasher, &entry.kind);
        match content {
            Some(bytes) => tracked_tree_hash_frame(&mut hasher, bytes),
            None => tracked_tree_hash_frame(&mut hasher, entry.object_id.as_bytes()),
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct CommitTreeBuildProofV3 {
    tracked_tree_hash_contract: &'static str,
    build_git_head: String,
    build_git_clean: bool,
    build_tracked_tree_sha256: String,
    expected_commit_tree_sha256_before: String,
    expected_commit_tree_sha256_after: String,
    build_head_matches_expected_commit: bool,
    build_tree_matches_expected_commit: bool,
    expected_commit_tree_equal_before_after: bool,
    local_strict_commit_tree_requirements_satisfied: bool,
}

fn parse_build_clean() -> Result<bool, String> {
    match BUILD_GIT_CLEAN {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("build clean binding was not canonical Boolean text".into()),
    }
}

fn commit_tree_build_proof(
    expected_commit: &str,
    expected_tree_before: String,
    expected_tree_after: String,
) -> Result<CommitTreeBuildProofV3, String> {
    commit_tree_build_proof_from_inputs(
        expected_commit,
        expected_tree_before,
        expected_tree_after,
        BUILD_GIT_HEAD,
        parse_build_clean()?,
        BUILD_TRACKED_TREE_SHA256,
        BUILD_TRACKED_TREE_CONTRACT,
    )
}

fn commit_tree_build_proof_from_inputs(
    expected_commit: &str,
    expected_tree_before: String,
    expected_tree_after: String,
    build_git_head: &str,
    build_git_clean: bool,
    build_tracked_tree_sha256: &str,
    build_tree_contract: &str,
) -> Result<CommitTreeBuildProofV3, String> {
    if build_tree_contract != TRACKED_TREE_HASH_CONTRACT {
        return Err("build and runtime tracked-tree contracts differed".into());
    }
    let build_head_matches_expected_commit = build_git_head == expected_commit;
    let build_tree_matches_expected_commit =
        build_tracked_tree_sha256 == expected_tree_before.as_str();
    let expected_commit_tree_equal_before_after = expected_tree_before == expected_tree_after;
    let local_strict_commit_tree_requirements_satisfied = build_git_clean
        && build_head_matches_expected_commit
        && build_tree_matches_expected_commit
        && expected_commit_tree_equal_before_after;
    Ok(CommitTreeBuildProofV3 {
        tracked_tree_hash_contract: TRACKED_TREE_HASH_CONTRACT,
        build_git_head: build_git_head.to_string(),
        build_git_clean,
        build_tracked_tree_sha256: build_tracked_tree_sha256.to_string(),
        expected_commit_tree_sha256_before: expected_tree_before,
        expected_commit_tree_sha256_after: expected_tree_after,
        build_head_matches_expected_commit,
        build_tree_matches_expected_commit,
        expected_commit_tree_equal_before_after,
        local_strict_commit_tree_requirements_satisfied,
    })
}

fn validate_commit_tree_build_proof(
    binding_mode: BindingModeV2,
    proof: &CommitTreeBuildProofV3,
) -> Result<(), String> {
    if !proof.expected_commit_tree_equal_before_after {
        return Err("expected commit tracked tree changed during the trial".into());
    }
    if binding_mode == BindingModeV2::Strict
        && !proof.local_strict_commit_tree_requirements_satisfied
    {
        return Err("strict commit-tree proof did not match the expected clean commit tree".into());
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct DeckBindingV1 {
    deck_id: &'static str,
    runtime_deck_hash: u64,
    source_sha256: &'static str,
    mainboard_count: usize,
}

fn deck_binding(deck: &'static RuntimeDeckDefinition) -> DeckBindingV1 {
    DeckBindingV1 {
        deck_id: deck.id,
        runtime_deck_hash: deck.runtime_deck_hash,
        source_sha256: deck.source_sha256,
        mainboard_count: deck.mainboard_count,
    }
}

#[derive(Debug, Serialize)]
struct PolicyBindingV1 {
    policy_id: &'static str,
    seed_derivation_version: &'static str,
    algorithm: &'static str,
    noncombat_selection: &'static str,
    attacker_selection: &'static str,
    blocker_selection: &'static str,
    trigger_order_selection: &'static str,
    physical_counter_scope: &'static str,
    forced_decision_contract: &'static str,
}

#[derive(Debug, Serialize)]
struct RuntimeRecordV2<'a> {
    schema_version: &'static str,
    record_type: &'static str,
    trial_id: &'a str,
    valid: bool,
    claim_scope: &'static str,
    validity_scope: &'static str,
    formal_comparison_claim: bool,
    compiled_input_closure_attested: bool,
    formal_build_attestation_present: bool,
    formal_build_attestation_required: bool,
    formal_build_attestation_required_kind: &'static str,
    formal_paired_multiplier_authorized: bool,
    external_paired_validator_required: bool,
    external_comparison_gate: &'static [&'static str],
    invalid_reasons: Vec<&'static str>,
    source_binding: SourceBindingV2<'a>,
    commit_tree_build_proof: CommitTreeBuildProofV3,
    runtime_contract: RuntimeContractV2<'a>,
    card_database_hash: u64,
    measured_deck: DeckBindingV1,
    validation_decks: [DeckBindingV1; 2],
    policy: PolicyBindingV1,
    workload: WorkloadBindingV2,
    validation: ValidationSuiteV1,
    warmup: PhaseV2,
    measurement: PhaseV2,
    rates: RatesV2,
}

#[derive(Debug, Serialize)]
struct SourceBindingV2<'a> {
    package: &'static str,
    package_version: &'static str,
    expected_commit: &'a str,
    actual_commit: &'a str,
    binding_mode: &'static str,
    working_tree_clean: bool,
    git_status_entry_count: usize,
    git_status_sha256: String,
    verified_unchanged_through_trial_end: bool,
    commit_tree_and_seven_component_binding: &'static str,
    seven_component_exact_match_contract: &'static str,
    seven_embedded_components_match_expected_commit: bool,
    seven_components_verified_before_timed_work: bool,
    seven_components_verified_after_timed_work: bool,
    verified_embedded_components: Vec<SourceComponentV2>,
    build_profile: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct EffectiveRuntimeBindingV2 {
    available_processors: usize,
    target_os: &'static str,
    target_arch: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EffectiveRuntimeSpanV2 {
    before: EffectiveRuntimeBindingV2,
    after: EffectiveRuntimeBindingV2,
}

#[derive(Debug, Serialize)]
struct RuntimeContractV2<'a> {
    actors: usize,
    actor_threads: usize,
    thread_model: &'static str,
    affinity_contract_id: &'a str,
    cpu_contract_id: &'a str,
    topology_contract_id: &'a str,
    host_contract_id: &'a str,
    power_contract_id: &'a str,
    expected_available_processors: usize,
    observed_available_processors: usize,
    observed_available_processors_after: usize,
    available_processors_match: bool,
    effective_runtime_binding_before: EffectiveRuntimeBindingV2,
    effective_runtime_binding_after: EffectiveRuntimeBindingV2,
    effective_runtime_binding_equal_before_after: bool,
    hardware_contract_ids_are_external_attestations: bool,
    hardware_or_affinity_actually_verified_by_process: bool,
    target_os: &'static str,
    target_arch: &'static str,
}

fn capture_effective_runtime_binding() -> Result<EffectiveRuntimeBindingV2, String> {
    let available_processors = std::thread::available_parallelism()
        .map(usize::from)
        .map_err(|_| "failed to capture effective available processors".to_string())?;
    Ok(EffectiveRuntimeBindingV2 {
        available_processors,
        target_os: std::env::consts::OS,
        target_arch: std::env::consts::ARCH,
    })
}

fn validate_effective_runtime_binding(
    expected_available_processors: usize,
    before: EffectiveRuntimeBindingV2,
    after: EffectiveRuntimeBindingV2,
) -> Result<(), String> {
    if before != after {
        return Err("effective runtime binding changed during the trial".into());
    }
    if before.available_processors != expected_available_processors
        || after.available_processors != expected_available_processors
    {
        return Err("effective available processors did not match the declared contract".into());
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct WorkloadBindingV2 {
    workload_id: &'static str,
    policy_id: &'static str,
    seed_derivation_version: &'static str,
    measured_matchup: &'static str,
    actors: usize,
    warmup_requested_ns: u64,
    measurement_requested_ns: u64,
    base_seed: u64,
    warmup_episode_range: &'static str,
    measurement_episode_range: &'static str,
    actor_episode_schedule: &'static str,
    deadline_policy: &'static str,
    throughput_denominator: &'static str,
    decision_count_source: &'static str,
    decision_safety_cap: u64,
    timed_lane: &'static str,
    untimed_oracle: &'static str,
    exclusions: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceSnapshotV2 {
    head: String,
    status: Vec<u8>,
}

impl SourceSnapshotV2 {
    fn is_clean(&self) -> bool {
        self.status.is_empty()
    }

    fn status_entry_count(&self) -> usize {
        self.status
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .count()
    }
}

fn build_record<'a>(
    config: &'a MatchedUniformConfigV2,
    runtime: EffectiveRuntimeSpanV2,
    source: &'a SourceSnapshotV2,
    verified_embedded_components: Vec<SourceComponentV2>,
    commit_tree_build_proof: CommitTreeBuildProofV3,
    validation: ValidationSuiteV1,
    trial: TimedTrialV2,
) -> RuntimeRecordV2<'a> {
    let rally = runtime_deck_by_id("Rally").expect("validated Rally runtime deck");
    let burn = runtime_deck_by_id("Burn").expect("validated Burn runtime deck");
    let strict = config.binding_mode == BindingModeV2::Strict;
    let valid = strict
        && source.is_clean()
        && commit_tree_build_proof.local_strict_commit_tree_requirements_satisfied;
    let mut invalid_reasons = Vec::new();
    if !strict {
        invalid_reasons.push("dirty_smoke_binding_mode_is_nonclaiming");
    }
    if !source.is_clean() {
        invalid_reasons.push("source_tree_not_clean");
    }
    if !commit_tree_build_proof.build_git_clean {
        invalid_reasons.push("build_source_tree_not_clean");
    }
    if !commit_tree_build_proof.build_head_matches_expected_commit {
        invalid_reasons.push("build_head_did_not_match_expected_commit");
    }
    if !commit_tree_build_proof.build_tree_matches_expected_commit {
        invalid_reasons.push("build_tree_did_not_match_expected_commit");
    }
    RuntimeRecordV2 {
        schema_version: SCHEMA_V2,
        record_type: "rust_matched_uniform_runtime_trial",
        trial_id: &config.trial_id,
        valid,
        claim_scope: if valid { CLAIM_SCOPE } else { "diagnostic_only" },
        validity_scope: "local_rust_runtime_candidate_only_not_formal_pair_authorization",
        formal_comparison_claim: false,
        compiled_input_closure_attested: false,
        formal_build_attestation_present: false,
        formal_build_attestation_required: true,
        formal_build_attestation_required_kind:
            "external_sealed_builder_full_compiled_input_closure/v1",
        formal_paired_multiplier_authorized: false,
        external_paired_validator_required: true,
        external_comparison_gate: &[
            "same_host_cpu_topology_power_and_affinity_contract_ids",
            "same_effective_available_processors",
            "paired_XMage_and_Rust_trials",
            "AB_BA_execution_order_matrix",
            "external_sealed_builder_full_compiled_input_closure_attestation",
            "external_validator_attestation",
        ],
        invalid_reasons,
        source_binding: SourceBindingV2 {
            package: env!("CARGO_PKG_NAME"),
            package_version: env!("CARGO_PKG_VERSION"),
            expected_commit: &config.expected_commit,
            actual_commit: &source.head,
            binding_mode: config.binding_mode.as_str(),
            working_tree_clean: source.is_clean(),
            git_status_entry_count: source.status_entry_count(),
            git_status_sha256: sha256_hex(&source.status),
            verified_unchanged_through_trial_end: true,
            commit_tree_and_seven_component_binding:
                "runtime_git_head_and_commit_tree_plus_seven_embedded_component_exact_match/v3",
            seven_component_exact_match_contract: SEVEN_COMPONENT_EXACT_MATCH_CONTRACT,
            seven_embedded_components_match_expected_commit: true,
            seven_components_verified_before_timed_work: true,
            seven_components_verified_after_timed_work: true,
            verified_embedded_components,
            build_profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        },
        commit_tree_build_proof,
        runtime_contract: RuntimeContractV2 {
            actors: config.actors,
            actor_threads: config.actors,
            thread_model: "one synchronous Rust thread per actor",
            affinity_contract_id: &config.affinity_contract_id,
            cpu_contract_id: &config.cpu_contract_id,
            topology_contract_id: &config.topology_contract_id,
            host_contract_id: &config.host_contract_id,
            power_contract_id: &config.power_contract_id,
            expected_available_processors: config.expected_available_processors,
            observed_available_processors: runtime.before.available_processors,
            observed_available_processors_after: runtime.after.available_processors,
            available_processors_match: runtime.before.available_processors
                == config.expected_available_processors
                && runtime.after.available_processors == config.expected_available_processors,
            effective_runtime_binding_before: runtime.before,
            effective_runtime_binding_after: runtime.after,
            effective_runtime_binding_equal_before_after: runtime.before == runtime.after,
            hardware_contract_ids_are_external_attestations: true,
            hardware_or_affinity_actually_verified_by_process: false,
            target_os: std::env::consts::OS,
            target_arch: std::env::consts::ARCH,
        },
        card_database_hash: KERNEL_CARDDB_HASH,
        measured_deck: deck_binding(rally),
        validation_decks: [deck_binding(burn), deck_binding(rally)],
        policy: PolicyBindingV1 {
            policy_id: POLICY_ID,
            seed_derivation_version: SEED_DERIVATION_VERSION,
            algorithm: "hierarchical_splitmix64_v2_group_then_leaf_unsigned_modulo",
            noncombat_selection: "leaf_0_modulo_canonical_ordered_legal_count",
            attacker_selection: "leaf_i_modulo_2_equals_1_independently_per_attacker",
            blocker_selection:
                "leaf_0_modulo_100_below_35_then_leaf_1_modulo_canonical_blocker_count",
            trigger_order_selection: "leaf_0_modulo_full_permutation_menu_as_one_physical_decision",
            physical_counter_scope:
                "one_local_uint63_counter_per_physical_actor_seat_incremented_once_per_group",
            forced_decision_contract:
                "engine_suppresses_forced_pass_singleton_discard_and_unaffordable_kicker_before_policy",
        },
        workload: WorkloadBindingV2 {
            workload_id: "rally_mirror_bo1_keep7_p0_starts/v2",
            policy_id: POLICY_ID,
            seed_derivation_version: SEED_DERIVATION_VERSION,
            measured_matchup: "Rally_vs_Rally_bo1_fixed_p0_start_keep_seven",
            actors: config.actors,
            warmup_requested_ns: config.warmup_ns,
            measurement_requested_ns: config.measure_ns,
            base_seed: config.base_seed,
            warmup_episode_range: "[0,2^62)",
            measurement_episode_range: "[2^62,2^63)",
            actor_episode_schedule: "episode_base + actor_index + local_index*actor_count",
            deadline_policy: "stop launching at common deadline, finish every launched game",
            throughput_denominator: "slowest actor finish offset from common phase start",
            decision_count_source:
                "actual MatchedUniformPolicyV2 counters; never assigned episode IDs",
            decision_safety_cap: DECISION_SAFETY_CAP,
            timed_lane: "fast_actor_session_v1_in_process_no_transcript_materialization",
            untimed_oracle: "full_policy_surface_v5_burn_and_rally_fixed_validation_only",
            exclusions: &[
                "xmage",
                "cross_engine_multiplier",
                "jsonl_protocol",
                "process_ipc",
                "python",
                "feature_encoding",
                "neural_inference",
                "training",
                "optimizer",
                "artifact_persistence",
            ],
        },
        validation,
        warmup: trial.warmup,
        measurement: trial.measurement,
        rates: trial.rates,
    }
}

fn capture_git_source_binding(expected_commit: &str) -> Result<SourceSnapshotV2, String> {
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|_| "failed to execute git for source binding".to_string())?;
    if !head.status.success() {
        return Err("git rev-parse failed during source binding".into());
    }
    let observed = std::str::from_utf8(&head.stdout)
        .map_err(|_| "git HEAD was not UTF-8".to_string())?
        .trim();
    if observed != expected_commit {
        return Err("--expected-commit does not match the runtime worktree HEAD".into());
    }
    let status = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .output()
        .map_err(|_| "failed to execute git status for source binding".to_string())?;
    if !status.status.success() {
        return Err("git status failed during source binding".into());
    }
    Ok(SourceSnapshotV2 {
        head: observed.to_string(),
        status: status.stdout,
    })
}

pub(crate) fn run_matched_uniform_runtime_json_v2(config: MatchedUniformConfigV2) {
    assert!(
        !std::hint::black_box(cfg!(debug_assertions)),
        "matched-uniform runtime trial requires a release build"
    );
    let runtime_before = capture_effective_runtime_binding()
        .unwrap_or_else(|message| panic!("matched-uniform runtime binding failed: {message}"));
    assert_eq!(
        runtime_before.available_processors, config.expected_available_processors,
        "matched-uniform runtime trial refused an effective parallelism mismatch"
    );
    let source_before = capture_git_source_binding(&config.expected_commit)
        .unwrap_or_else(|message| panic!("matched-uniform source binding failed: {message}"));
    assert!(
        config.binding_mode == BindingModeV2::DirtySmoke || source_before.is_clean(),
        "strict matched-uniform runtime trial requires a clean source worktree"
    );
    let embedded_components_before = verify_seven_embedded_components(&config.expected_commit)
        .unwrap_or_else(|message| {
            panic!("matched-uniform embedded-component binding failed: {message}")
        });
    let expected_tree_before = tracked_tree_sha256_at_commit(&config.expected_commit)
        .unwrap_or_else(|message| panic!("matched-uniform tracked-tree binding failed: {message}"));
    let provisional_commit_tree_proof = commit_tree_build_proof(
        &config.expected_commit,
        expected_tree_before.clone(),
        expected_tree_before.clone(),
    )
    .unwrap_or_else(|message| panic!("matched-uniform commit-tree proof failed: {message}"));
    validate_commit_tree_build_proof(config.binding_mode, &provisional_commit_tree_proof)
        .unwrap_or_else(|message| panic!("matched-uniform commit-tree proof failed: {message}"));
    let validation = validation_suite(config.transcript_games_per_deck)
        .unwrap_or_else(|message| panic!("matched-uniform validation failed: {message}"));
    let deck = runtime_deck_by_id("Rally").expect("validated Rally runtime deck");
    let trial = measure_duration_trial(
        deck,
        config.actors,
        config.warmup_ns,
        config.measure_ns,
        config.base_seed,
    )
    .unwrap_or_else(|message| panic!("matched-uniform runtime trial refused rates: {message}"));
    let runtime_after = capture_effective_runtime_binding().unwrap_or_else(|message| {
        panic!("matched-uniform final runtime binding failed: {message}")
    });
    let source_after = capture_git_source_binding(&config.expected_commit)
        .unwrap_or_else(|message| panic!("matched-uniform final source binding failed: {message}"));
    let embedded_components_after = verify_seven_embedded_components(&config.expected_commit)
        .unwrap_or_else(|message| {
            panic!("matched-uniform final embedded-component binding failed: {message}")
        });
    let expected_tree_after = tracked_tree_sha256_at_commit(&config.expected_commit)
        .unwrap_or_else(|message| {
            panic!("matched-uniform final tracked-tree binding failed: {message}")
        });
    assert_eq!(
        source_after, source_before,
        "matched-uniform source bytes or status changed during the trial"
    );
    assert_eq!(
        embedded_components_after, embedded_components_before,
        "matched-uniform embedded-component binding changed during the trial"
    );
    validate_effective_runtime_binding(
        config.expected_available_processors,
        runtime_before,
        runtime_after,
    )
    .unwrap_or_else(|message| panic!("matched-uniform runtime binding failed: {message}"));
    let commit_tree_proof = commit_tree_build_proof(
        &config.expected_commit,
        expected_tree_before,
        expected_tree_after,
    )
    .unwrap_or_else(|message| panic!("matched-uniform commit-tree proof failed: {message}"));
    validate_commit_tree_build_proof(config.binding_mode, &commit_tree_proof)
        .unwrap_or_else(|message| panic!("matched-uniform commit-tree proof failed: {message}"));
    let record = build_record(
        &config,
        EffectiveRuntimeSpanV2 {
            before: runtime_before,
            after: runtime_after,
        },
        &source_before,
        embedded_components_before,
        commit_tree_proof,
        validation,
        trial,
    );
    println!(
        "{}",
        serde_json::to_string(&record).expect("matched-uniform runtime record serializes")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn config() -> MatchedUniformConfigV2 {
        MatchedUniformConfigV2 {
            expected_commit: COMMIT.into(),
            trial_id: "pair-0001-rust".into(),
            binding_mode: BindingModeV2::Strict,
            affinity_contract_id: "test-affinity.v1".into(),
            cpu_contract_id: "test-cpu.v1".into(),
            topology_contract_id: "test-topology.v1".into(),
            host_contract_id: "test-host.v1".into(),
            power_contract_id: "test-power.v1".into(),
            expected_available_processors: 1,
            actors: 1,
            warmup_ns: NANOS_PER_SECOND,
            measure_ns: NANOS_PER_SECOND * 2,
            base_seed: VALIDATION_BASE_SEED,
            transcript_games_per_deck: 0,
        }
    }

    fn required_cli_args() -> Vec<String> {
        [
            ("--expected-commit", COMMIT),
            ("--actors", "4"),
            ("--base-seed", "71501"),
            ("--warmup-seconds", "1.000000001"),
            ("--measure-seconds", "2.5"),
            ("--trial-id", "Pair_A-0001.rust"),
            ("--binding-mode", "strict"),
            ("--affinity-contract-id", "affinity_contract"),
            ("--expected-available-processors", "16"),
            ("--cpu-contract-id", "cpu_contract"),
            ("--topology-contract-id", "topology_contract"),
            ("--host-contract-id", "host_contract"),
            ("--power-contract-id", "power_contract"),
        ]
        .into_iter()
        .flat_map(|(flag, value)| [flag.to_string(), value.to_string()])
        .collect()
    }

    #[test]
    fn cli_requires_one_matched_actor_count_and_all_external_contract_ids() {
        let parsed = MatchedUniformConfigV2::parse(&required_cli_args()).unwrap();
        assert_eq!(parsed.actors, 4);
        assert_eq!(parsed.warmup_ns, NANOS_PER_SECOND + 1);
        assert_eq!(parsed.measure_ns, NANOS_PER_SECOND * 2 + 500_000_000);
        assert_eq!(parsed.binding_mode, BindingModeV2::Strict);
        assert!(MatchedUniformConfigV2::parse(&[]).is_err());

        for invalid_actor in ["1,4", "2", "0", "17"] {
            let mut args = required_cli_args();
            let value = args.iter().position(|arg| arg == "--actors").unwrap() + 1;
            args[value] = invalid_actor.into();
            assert!(MatchedUniformConfigV2::parse(&args).is_err());
        }
        let mut private = required_cli_args();
        let value = private
            .iter()
            .position(|arg| arg == "--host-contract-id")
            .unwrap()
            + 1;
        private[value] = "C:\\private".into();
        assert!(MatchedUniformConfigV2::parse(&private).is_err());
        let mut seed = required_cli_args();
        let value = seed.iter().position(|arg| arg == "--base-seed").unwrap() + 1;
        seed[value] = (UINT63_MAX + 1).to_string();
        assert!(MatchedUniformConfigV2::parse(&seed).is_err());
    }

    #[test]
    fn decimal_seconds_parse_to_exact_positive_nanoseconds() {
        assert_eq!(
            parse_exact_positive_seconds_ns("0.000000001", "--seconds").unwrap(),
            1
        );
        assert_eq!(
            parse_exact_positive_seconds_ns("1.25", "--seconds").unwrap(),
            1_250_000_000
        );
        assert_eq!(
            parse_exact_positive_seconds_ns("9223372036.854775807", "--seconds").unwrap(),
            i64::MAX as u64
        );
        for invalid in [
            "0",
            "0.000000000",
            "00.1",
            ".1",
            "1.",
            "+1",
            "1e0",
            "1.0000000000",
            "9223372036.854775808",
        ] {
            assert!(parse_exact_positive_seconds_ns(invalid, "--seconds").is_err());
        }
    }

    #[test]
    fn cross_language_seed_goldens_are_exact() {
        assert_eq!(derive_env_seed(71_501, 0).unwrap(), 0x6bbf_b0c0_fc58_c50c);
        assert_eq!(derive_env_seed(71_501, 1).unwrap(), 0x1f71_5016_d3bd_86dd);
        let p0 = derive_group_seed(71_501, 0, 0, PlayerSeatV1::P0).unwrap();
        let p1 = derive_group_seed(71_501, 0, 0, PlayerSeatV1::P1).unwrap();
        assert_eq!(p0, 0xa703_05ab_1de3_83fc);
        assert_eq!(p1, 0xa466_8aed_f2a7_7373);
        assert_eq!(derive_leaf_seed(p0, 0), 0xfeaa_a9f4_fbe0_2bd4);
        assert_eq!(derive_leaf_seed(p0, 1), 0x65d3_83ba_c2c5_e8f0);
        assert_eq!(derive_leaf_seed(p1, 0), 0x019e_c8ee_5277_b4ee);
        assert_eq!(derive_leaf_seed(p1, 1), 0xf11b_1a29_7a72_89fa);
        assert_eq!(unsigned_modulo(derive_leaf_seed(p0, 0), 7).unwrap(), 6);
    }

    fn shape(
        episode_id: u64,
        physical_decision_id: u64,
        substep_index: u32,
        substep_count: u32,
        acting_player: PlayerSeatV1,
        decision_kind: FastActorDecisionKindV1,
        legal_action_count: u32,
    ) -> FastActorDecisionV1 {
        FastActorDecisionV1 {
            episode_id,
            step: u64::from(substep_index),
            environment_revision: u64::from(substep_index),
            physical_decision_id,
            substep_index,
            substep_count,
            acting_player,
            decision_kind,
            legal_action_count,
        }
    }

    #[test]
    fn policy_preserves_one_group_per_seat_for_noncombat_combat_and_trigger_menu() {
        let mut policy = MatchedUniformPolicyV2::new(71_501, 0).unwrap();
        let first = policy
            .select(shape(
                0,
                0,
                0,
                1,
                PlayerSeatV1::P0,
                FastActorDecisionKindV1::Surface,
                7,
            ))
            .unwrap();
        assert_eq!(first.selected_index, 6);
        assert_eq!(first.actor_local_physical_index, 0);

        let trigger_menu = policy
            .select(shape(
                0,
                1,
                0,
                1,
                PlayerSeatV1::P1,
                FastActorDecisionKindV1::Surface,
                6,
            ))
            .unwrap();
        assert!(trigger_menu.selected_index < 6);
        assert_eq!(trigger_menu.actor_local_physical_index, 0);

        let attacker_count = 3;
        for substep in 0..attacker_count {
            let selected = policy
                .select(shape(
                    0,
                    2,
                    substep,
                    attacker_count,
                    PlayerSeatV1::P0,
                    FastActorDecisionKindV1::AttackerInclusion,
                    2,
                ))
                .unwrap();
            let group = derive_group_seed(71_501, 0, 1, PlayerSeatV1::P0).unwrap();
            assert_eq!(
                selected.included,
                Some(derive_leaf_seed(group, substep) % 2 == 1)
            );
        }
        assert_eq!(policy.counters.physical_by_seat, [2, 1]);
        assert!(policy.pending.is_none());
    }

    #[test]
    fn blocker_gate_uses_leaf_zero_and_rank_uses_leaf_one_once() {
        for episode in 0..100 {
            let group = derive_group_seed(71_501, episode, 0, PlayerSeatV1::P0).unwrap();
            let expected = if derive_leaf_seed(group, 0) % 100 < 35 {
                Some(unsigned_modulo(derive_leaf_seed(group, 1), 4).unwrap())
            } else {
                None
            };
            let mut policy = MatchedUniformPolicyV2::new(71_501, episode).unwrap();
            let observed: Vec<_> = (0..4)
                .map(|substep| {
                    policy
                        .select(shape(
                            episode,
                            0,
                            substep,
                            4,
                            PlayerSeatV1::P0,
                            FastActorDecisionKindV1::BlockerInclusion,
                            2,
                        ))
                        .unwrap()
                        .included
                        .unwrap()
                })
                .collect();
            assert_eq!(
                observed.iter().filter(|&&include| include).count(),
                usize::from(expected.is_some())
            );
            if let Some(rank) = expected {
                assert!(observed[rank]);
            }
        }
    }

    #[test]
    fn timed_episode_schedule_matches_xmage_v2_striping_and_uint63_halves() {
        assert_eq!(
            striped_timed_episode_id(TimedEpisodePhaseV1::Warmup, 0, 0, 4).unwrap(),
            0
        );
        assert_eq!(
            striped_timed_episode_id(TimedEpisodePhaseV1::Warmup, 3, 2, 4).unwrap(),
            14
        );
        assert_eq!(
            striped_timed_episode_id(TimedEpisodePhaseV1::Measurement, 2, 3, 4).unwrap(),
            MEASUREMENT_EPISODE_BASE + 11
        );

        let mut episodes = HashSet::new();
        for phase in [
            TimedEpisodePhaseV1::Warmup,
            TimedEpisodePhaseV1::Measurement,
        ] {
            for actor in 0..16 {
                for game in 0..100 {
                    let episode = striped_timed_episode_id(phase, game, actor, 16).unwrap();
                    assert!(episode <= UINT63_MAX);
                    assert!(episodes.insert(episode));
                }
            }
        }
        assert_eq!(episodes.len(), 2 * 16 * 100);
        assert!(striped_timed_episode_id(
            TimedEpisodePhaseV1::Measurement,
            EPISODES_PER_TIMED_PHASE,
            0,
            1
        )
        .is_err());
        assert!(striped_timed_episode_id(TimedEpisodePhaseV1::Warmup, 0, 4, 4).is_err());
    }

    #[test]
    fn fixed_burn_and_rally_oracle_validation_is_exact_and_can_emit_transcript() {
        let suite = validation_suite(1).unwrap();
        for validation in [&suite.burn, &suite.rally] {
            assert!(validation.all_outcomes_natural);
            assert!(validation.zero_invalids);
            assert!(
                validation
                    .exact_metadata_semantic_order_selection_counter_state_and_terminal_parity
            );
            assert!(validation.policy_steps_compared > 0);
            assert_eq!(validation.transcripts.len(), 1);
            assert!(!validation.transcripts[0].decisions.is_empty());
        }
        assert!(suite
            .rally
            .semantic_categories_seen
            .contains_key("declare_attackers"));
    }

    fn phase_fixture(phase: TimedEpisodePhaseV1) -> PhaseV2 {
        let episode = phase.base();
        PhaseV2 {
            requested_wall_ns: NANOS_PER_SECOND,
            elapsed_slowest_actor_ns: NANOS_PER_SECOND + 1,
            attempted_games: 1,
            natural_completions: 1,
            games_with_any_invalidity: 0,
            outcomes: OutcomeCountsV1 {
                attempted_games: 1,
                natural_terminal_games: 1,
                ..OutcomeCountsV1::default()
            },
            natural_outcomes: NaturalOutcomesV1 {
                p0_wins: 1,
                ..NaturalOutcomesV1::default()
            },
            policy_steps: 20,
            physical_decisions: 10,
            physical_decisions_by_seat: [6, 4],
            policy_action_selections: 12,
            policy_leaf_evaluations: 15,
            in_flight_at_deadline_finished_naturally: 1,
            unfinished_after_join: 0,
            actors: vec![ActorPhaseV2 {
                actor_index: 0,
                attempted: 1,
                natural_completions: 1,
                first_episode_id: Some(episode),
                last_episode_id: Some(episode),
                finish_offset_ns_from_common_start: NANOS_PER_SECOND + 1,
                in_flight_at_deadline_finished_naturally: 1,
            }],
            all_attempted_games_finished_naturally: true,
        }
    }

    #[test]
    fn schema_is_one_line_privacy_safe_and_has_no_ratio_fields() {
        let config = config();
        let source = SourceSnapshotV2 {
            head: COMMIT.into(),
            status: vec![],
        };
        let runtime = EffectiveRuntimeBindingV2 {
            available_processors: 1,
            target_os: "test-os",
            target_arch: "test-arch",
        };
        let tree_hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let commit_tree_proof = commit_tree_build_proof_from_inputs(
            COMMIT,
            tree_hash.into(),
            tree_hash.into(),
            COMMIT,
            true,
            tree_hash,
            TRACKED_TREE_HASH_CONTRACT,
        )
        .unwrap();
        let record = build_record(
            &config,
            EffectiveRuntimeSpanV2 {
                before: runtime,
                after: runtime,
            },
            &source,
            vec![SourceComponentV2 {
                component: "test_component",
                normalized_sha256: tree_hash.into(),
            }],
            commit_tree_proof,
            validation_suite(0).unwrap(),
            TimedTrialV2 {
                warmup: phase_fixture(TimedEpisodePhaseV1::Warmup),
                measurement: phase_fixture(TimedEpisodePhaseV1::Measurement),
                rates: RatesV2 {
                    denominator_seconds: 1.0,
                    natural_games_per_second: 1.0,
                    policy_steps_per_second: 20.0,
                    physical_decisions_per_second: 10.0,
                    policy_action_selections_per_second: 12.0,
                    policy_leaf_evaluations_per_second: 15.0,
                },
            },
        );
        let encoded = serde_json::to_string(&record).unwrap();
        assert!(!encoded.contains('\n'));
        assert!(!encoded.contains("C:\\"));
        assert!(!encoded.contains("repo_root"));
        assert!(!encoded.contains("java_home"));
        assert!(!encoded.contains("classpath"));
        assert!(!encoded.contains("speedup"));
        assert!(!encoded.contains("ratio"));
        let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["schema_version"], SCHEMA_V2);
        assert_eq!(value["claim_scope"], CLAIM_SCOPE);
        assert_eq!(
            value["validity_scope"],
            "local_rust_runtime_candidate_only_not_formal_pair_authorization"
        );
        assert_eq!(value["formal_comparison_claim"], false);
        assert_eq!(value["compiled_input_closure_attested"], false);
        assert_eq!(value["formal_build_attestation_present"], false);
        assert_eq!(value["formal_build_attestation_required"], true);
        assert_eq!(
            value["formal_build_attestation_required_kind"],
            "external_sealed_builder_full_compiled_input_closure/v1"
        );
        assert_eq!(value["formal_paired_multiplier_authorized"], false);
        assert_eq!(value["valid"], true);
        assert_eq!(value["measured_deck"]["deck_id"], "Rally");
        assert_eq!(value["policy"]["policy_id"], POLICY_ID);
        assert_eq!(value["runtime_contract"]["actors"], 1);
        assert_eq!(
            value["runtime_contract"]["observed_available_processors"],
            1
        );
        assert_eq!(
            value["runtime_contract"]["observed_available_processors_after"],
            1
        );
        assert_eq!(
            value["runtime_contract"]["effective_runtime_binding_equal_before_after"],
            true
        );
        assert_eq!(
            value["source_binding"]["seven_component_exact_match_contract"],
            SEVEN_COMPONENT_EXACT_MATCH_CONTRACT
        );
        assert_eq!(
            value["source_binding"]["seven_embedded_components_match_expected_commit"],
            true
        );
        assert_eq!(
            value["commit_tree_build_proof"]["tracked_tree_hash_contract"],
            TRACKED_TREE_HASH_CONTRACT
        );
        assert_eq!(
            value["commit_tree_build_proof"]["local_strict_commit_tree_requirements_satisfied"],
            true
        );
        assert!(value["external_comparison_gate"]
            .as_array()
            .unwrap()
            .iter()
            .any(|gate| {
                gate == "external_sealed_builder_full_compiled_input_closure_attestation"
            }));
        assert_eq!(value["measurement"]["policy_action_selections"], 12);
    }

    #[test]
    fn runtime_git_binding_accepts_exact_head_and_rejects_a_false_claim() {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let head = std::str::from_utf8(&output.stdout).unwrap().trim();
        assert!(capture_git_source_binding(head).is_ok());
        let false_claim = if head == "0000000000000000000000000000000000000000" {
            "1111111111111111111111111111111111111111"
        } else {
            "0000000000000000000000000000000000000000"
        };
        assert!(capture_git_source_binding(false_claim).is_err());

        let current_head_spec = EmbeddedComponentSpecV2 {
            component: "rl",
            repo_relative_path: "mtg-kernel/src/rl.rs",
            embedded_bytes: include_bytes!("../../src/rl.rs"),
        };
        let verified =
            verify_embedded_component_specs_against_commit(head, &[current_head_spec]).unwrap();
        assert_eq!(verified.len(), 1);
        assert_eq!(verified[0].component, "rl");
        assert_eq!(verified[0].normalized_sha256.len(), 64);
        assert!(
            verify_embedded_component_specs_against_commit(false_claim, &[current_head_spec])
                .is_err()
        );
        let stale_spec = EmbeddedComponentSpecV2 {
            embedded_bytes: b"stale embedded source\n",
            ..current_head_spec
        };
        assert!(verify_embedded_component_specs_against_commit(head, &[stale_spec]).is_err());
        assert_eq!(
            normalize_source_bytes(b"one\r\ntwo\r\n").unwrap(),
            normalize_source_bytes(b"one\ntwo\n").unwrap()
        );
        assert!(normalize_source_bytes(b"one\rtwo\n").is_err());
        assert!(normalize_source_bytes(&[0xff]).is_err());

        let tree_hash = tracked_tree_sha256_at_commit(head).unwrap();
        assert_eq!(BUILD_GIT_HEAD, head);
        assert_eq!(BUILD_TRACKED_TREE_CONTRACT, TRACKED_TREE_HASH_CONTRACT);
        assert_eq!(BUILD_TRACKED_TREE_SHA256, tree_hash);
        let exact_build = commit_tree_build_proof_from_inputs(
            head,
            tree_hash.clone(),
            tree_hash.clone(),
            head,
            true,
            &tree_hash,
            TRACKED_TREE_HASH_CONTRACT,
        )
        .unwrap();
        assert!(validate_commit_tree_build_proof(BindingModeV2::Strict, &exact_build).is_ok());
        let stale_build = commit_tree_build_proof_from_inputs(
            head,
            tree_hash.clone(),
            tree_hash.clone(),
            false_claim,
            true,
            &tree_hash,
            TRACKED_TREE_HASH_CONTRACT,
        )
        .unwrap();
        assert!(validate_commit_tree_build_proof(BindingModeV2::Strict, &stale_build).is_err());
        let dirty_build = commit_tree_build_proof_from_inputs(
            head,
            tree_hash.clone(),
            tree_hash.clone(),
            head,
            false,
            &tree_hash,
            TRACKED_TREE_HASH_CONTRACT,
        )
        .unwrap();
        assert!(validate_commit_tree_build_proof(BindingModeV2::Strict, &dirty_build).is_err());
        assert!(validate_commit_tree_build_proof(BindingModeV2::DirtySmoke, &dirty_build).is_ok());
        let false_tree = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
        let wrong_tree_build = commit_tree_build_proof_from_inputs(
            head,
            tree_hash.clone(),
            tree_hash.clone(),
            head,
            true,
            false_tree,
            TRACKED_TREE_HASH_CONTRACT,
        )
        .unwrap();
        assert!(
            validate_commit_tree_build_proof(BindingModeV2::Strict, &wrong_tree_build).is_err()
        );
        let changed_tree = commit_tree_build_proof_from_inputs(
            head,
            tree_hash.clone(),
            false_tree.into(),
            head,
            true,
            &tree_hash,
            TRACKED_TREE_HASH_CONTRACT,
        )
        .unwrap();
        assert!(
            validate_commit_tree_build_proof(BindingModeV2::DirtySmoke, &changed_tree).is_err()
        );

        let runtime = capture_effective_runtime_binding().unwrap();
        assert!(
            validate_effective_runtime_binding(runtime.available_processors, runtime, runtime)
                .is_ok()
        );
        assert!(validate_effective_runtime_binding(
            runtime.available_processors + 1,
            runtime,
            runtime
        )
        .is_err());
        let changed = EffectiveRuntimeBindingV2 {
            available_processors: runtime.available_processors + 1,
            ..runtime
        };
        assert!(
            validate_effective_runtime_binding(runtime.available_processors, runtime, changed)
                .is_err()
        );
    }

    #[test]
    fn short_duration_trial_uses_common_deadlines_and_exact_natural_counters() {
        let trial = measure_duration_trial(
            runtime_deck_by_id("Rally").unwrap(),
            1,
            100_000_000,
            100_000_000,
            71_501,
        )
        .unwrap();
        assert!(trial.measurement.outcomes.natural_terminal_games > 0);
        assert!(trial.rates.natural_games_per_second.is_finite());
        assert!(trial.rates.policy_steps_per_second.is_finite());
        assert!(trial.rates.physical_decisions_per_second.is_finite());
        assert!(trial.rates.policy_action_selections_per_second.is_finite());
        assert!(trial.rates.policy_leaf_evaluations_per_second.is_finite());
        assert_eq!(
            trial.measurement.physical_decisions_by_seat[0]
                + trial.measurement.physical_decisions_by_seat[1],
            trial.measurement.physical_decisions
        );
        assert_eq!(trial.measurement.actors.len(), 1);
        assert_eq!(
            trial.measurement.actors[0].attempted,
            trial.measurement.outcomes.attempted_games
        );
        assert_eq!(
            trial.measurement.actors[0].first_episode_id,
            Some(MEASUREMENT_EPISODE_BASE)
        );
        assert_eq!(
            trial.measurement.actors[0].finish_offset_ns_from_common_start,
            trial.measurement.elapsed_slowest_actor_ns
        );
        assert!(trial.measurement.policy_action_selections > 0);
        assert!(trial.measurement.policy_leaf_evaluations > 0);
    }
}
