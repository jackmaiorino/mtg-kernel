//! Bounded, machine-readable comparison of three in-process Rust boundaries.
//!
//! This diagnostic deliberately stops before JSONL, Python, inference, a
//! learner, or persistence.  It is a capacity-attribution tool, not an
//! end-to-end training benchmark and not an XMage speedup claim.

use crate::{
    build_mirror_state_from_ids, random_action_for_decision, rng_below, rng_chance,
    DECISION_SAFETY_CAP,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::rl::{ActionSemanticV1, TerminalClassificationV1};
use mtg_kernel::rl_session::{
    RlEpisodeSessionV1, RlSessionDecisionV1, RlSessionResponseV1, SessionDeckIdsV1,
};
use mtg_kernel::runtime_decks::{runtime_deck_by_id, RuntimeDeckDefinition};
use mtg_kernel::state::{GameState, SplitMix64};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Arc, Barrier, OnceLock};
use std::time::{Duration, Instant};

const SCHEMA_V1: &str = "kernel_rl_three_lane_ceiling/v1";
const POLICY_V1: &str = "seeded_uniform_h2_semantics/v1";

// A u64 contains 2,048 disjoint 2^53-value partitions.  Three seed domains
// each consume 512 partitions (measurement/warmup x at most 256 actors),
// leaving a fourth domain unused.  Translating every value by an arbitrary
// base seed preserves collision-freedom under wrapping arithmetic.
const PARTITION_STRIDE: u64 = 1u64 << 53;
const PHASE_PARTITIONS: u64 = 512;
const WARMUP_PARTITION_OFFSET: u64 = 256;
const EPISODE_DOMAIN_OFFSET: u64 = 0;
const ENV_DOMAIN_OFFSET: u64 = PHASE_PARTITIONS;
const POLICY_DOMAIN_OFFSET: u64 = PHASE_PARTITIONS * 2;

#[derive(Debug)]
pub(crate) struct ThreeLaneConfigV1 {
    git_commit: String,
    deck_id: String,
    actors: Vec<usize>,
    warmup: Duration,
    measure: Duration,
    seed: u64,
}

impl ThreeLaneConfigV1 {
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        let mut git_commit = None;
        let mut deck_id = "Rally".to_string();
        let mut actors = vec![1, 4, 8, 16];
        let mut warmup_ms = 150u64;
        let mut measure_ms = 4_000u64;
        let mut seed = 0x9E37_0000u64;
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
                            "--git-commit must be 40 lowercase hexadecimal characters".to_string()
                        );
                    }
                    git_commit = Some(value.clone());
                }
                "--deck" => {
                    if !matches!(value.as_str(), "Burn" | "Rally") {
                        return Err("--deck must be exact Burn or Rally".to_string());
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
                        return Err("--actors must contain unique integers in the range 1..=256"
                            .to_string());
                    }
                    actors = parsed;
                }
                "--warmup-ms" => {
                    warmup_ms = parse_bounded_ms(value, "--warmup-ms", 0)?;
                }
                "--measure-ms" => {
                    measure_ms = parse_bounded_ms(value, "--measure-ms", 1)?;
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
            warmup: Duration::from_millis(warmup_ms),
            measure: Duration::from_millis(measure_ms),
            seed,
        })
    }
}

fn parse_bounded_ms(value: &str, flag: &str, minimum: u64) -> Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("{flag} must be an unsigned integer"))?;
    if !(minimum..=600_000).contains(&parsed) {
        return Err(format!("{flag} must be in {minimum}..=600000"));
    }
    Ok(parsed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct EpisodeSeedBundleV1 {
    episode_id: u64,
    env_seed: u64,
    policy_seed: u64,
}

fn partition_value(
    base_seed: u64,
    domain_offset: u64,
    warmup: bool,
    actor: usize,
    game_index: u64,
) -> Option<u64> {
    if actor >= 256 || game_index >= PARTITION_STRIDE {
        return None;
    }
    let phase_offset = if warmup { WARMUP_PARTITION_OFFSET } else { 0 };
    let partition = domain_offset + phase_offset + actor as u64;
    Some(
        base_seed
            .wrapping_add(partition.wrapping_mul(PARTITION_STRIDE))
            .wrapping_add(game_index),
    )
}

fn seed_bundle(
    base_seed: u64,
    warmup: bool,
    actor: usize,
    game_index: u64,
) -> Option<EpisodeSeedBundleV1> {
    Some(EpisodeSeedBundleV1 {
        episode_id: partition_value(base_seed, EPISODE_DOMAIN_OFFSET, warmup, actor, game_index)?,
        env_seed: partition_value(base_seed, ENV_DOMAIN_OFFSET, warmup, actor, game_index)?,
        policy_seed: partition_value(base_seed, POLICY_DOMAIN_OFFSET, warmup, actor, game_index)?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameOutcomeV1 {
    NaturalTerminal,
    PhysicalDecisionCap,
    PolicyStepCap,
    Halted,
    ApplyError,
    FailClosed,
    DriverError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LaneGameResultV1 {
    outcome: GameOutcomeV1,
    policy_steps: u64,
    physical_decisions: u64,
    diagnostic_state_hash: u64,
}

#[cfg(test)]
fn equivalence_hash_for_state(state: &GameState) -> u64 {
    state.diagnostic_state_hash()
}

#[cfg(not(test))]
fn equivalence_hash_for_state(_state: &GameState) -> u64 {
    // State hashing is test-only evidence.  Keeping it out of the release
    // measurement prevents the diagnostic itself from taxing every game.
    0
}

#[cfg(test)]
fn equivalence_hash_for_session(session: &RlEpisodeSessionV1) -> u64 {
    session.diagnostic_state_hash()
}

#[cfg(not(test))]
fn equivalence_hash_for_session(_session: &RlEpisodeSessionV1) -> u64 {
    0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum LaneKindV1 {
    EngineRaw,
    HarnessSurfaceV2,
    RlSessionV5Inproc,
}

fn play_engine_raw(
    deck: &'static RuntimeDeckDefinition,
    seeds: EpisodeSeedBundleV1,
    decision_cap: u64,
) -> LaneGameResultV1 {
    let mut state = build_mirror_state_from_ids(deck.card_ids, seeds.env_seed);
    let mut rng = SplitMix64::seed(seeds.policy_seed);
    let mut steps = 0u64;
    let outcome = loop {
        let decision = engine::advance_until_decision(&mut state);
        match decision {
            Decision::GameOver { .. } => break GameOutcomeV1::NaturalTerminal,
            Decision::Halted { .. } => break GameOutcomeV1::Halted,
            decision => {
                // As in the H2/session lanes, recognize a terminal produced
                // by the cap-th action before refusing action cap+1.
                if steps >= decision_cap {
                    break GameOutcomeV1::PhysicalDecisionCap;
                }
                let action = random_action_for_decision(&decision, &state, &mut rng);
                if engine::step(&mut state, action).is_err() {
                    break GameOutcomeV1::ApplyError;
                }
                steps = steps.saturating_add(1);
            }
        }
    };
    LaneGameResultV1 {
        outcome,
        policy_steps: steps,
        physical_decisions: steps,
        diagnostic_state_hash: equivalence_hash_for_state(&state),
    }
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
struct SeededUniformH2PolicyV1 {
    rng: SplitMix64,
    pending_combat: Option<PendingCombatChoiceV1>,
}

impl SeededUniformH2PolicyV1 {
    fn new(seed: u64) -> Self {
        Self {
            rng: SplitMix64::seed(seed),
            pending_combat: None,
        }
    }

    fn surface_action(&mut self, decision: &SurfaceDecision) -> Result<SurfaceAction, String> {
        if self.pending_combat.is_some() {
            return Err("H2 aggregate decision encountered with a pending V5 combat group".into());
        }
        match decision {
            SurfaceDecision::Decision(Decision::DeclareAttackers { eligible, .. }) => {
                let mask = sample_attacker_mask(&mut self.rng, eligible.len());
                Ok(SurfaceAction::Action(Action::DeclareAttackers(
                    eligible
                        .iter()
                        .zip(mask)
                        .filter_map(|(&id, include)| include.then_some(id))
                        .collect(),
                )))
            }
            SurfaceDecision::DeclareBlockersForAttacker { legal_blockers, .. } => {
                let chosen = sample_blocker_choice(&mut self.rng, legal_blockers.len());
                Ok(SurfaceAction::DeclareBlockersForAttacker(
                    chosen
                        .map(|index| vec![legal_blockers[index]])
                        .unwrap_or_default(),
                ))
            }
            SurfaceDecision::Decision(decision) => {
                let count = noncombat_action_count(decision)?;
                if count == 0 {
                    return Err("nonterminal H2 decision produced zero policy actions".into());
                }
                let selected = rng_below(&mut self.rng, count);
                noncombat_action_by_index(decision, selected).map(SurfaceAction::Action)
            }
        }
    }

    fn session_action(&mut self, decision: &RlSessionDecisionV1) -> Result<(u32, String), String> {
        if decision.legal_actions.is_empty() {
            return Err("nonterminal V5 response produced zero legal actions".into());
        }
        let first = &decision.legal_actions[0].semantic;
        let selected = match first {
            ActionSemanticV1::ChooseAttackerInclusion { .. } => {
                self.session_combat_action(decision, true)?
            }
            ActionSemanticV1::ChooseBlockerInclusion { .. } => {
                self.session_combat_action(decision, false)?
            }
            _ => {
                if decision.substep_index != 0 || decision.substep_count != 1 {
                    return Err("noncombat V5 decision advertised combat substeps".into());
                }
                if self.pending_combat.is_some() {
                    return Err("V5 left a sampled combat group unfinished".into());
                }
                rng_below(&mut self.rng, decision.legal_actions.len())
            }
        };
        let action = decision
            .legal_actions
            .get(selected)
            .ok_or_else(|| "policy selected an out-of-range V5 action".to_string())?;
        if action.selected_index as usize != selected {
            return Err("V5 legal action selected_index is not dense and ordered".into());
        }
        Ok((action.selected_index, action.stable_id.clone()))
    }

    fn session_combat_action(
        &mut self,
        decision: &RlSessionDecisionV1,
        attackers: bool,
    ) -> Result<usize, String> {
        let index = decision.substep_index as usize;
        let count = decision.substep_count as usize;
        if count == 0 || index >= count {
            return Err("invalid V5 combat substep shape".into());
        }
        if index == 0 {
            if self.pending_combat.is_some() {
                return Err("new V5 combat group started before the prior group completed".into());
            }
            self.pending_combat = Some(if attackers {
                PendingCombatChoiceV1::Attackers {
                    physical_decision_id: decision.physical_decision_id,
                    mask: sample_attacker_mask(&mut self.rng, count),
                }
            } else {
                PendingCombatChoiceV1::Blockers {
                    physical_decision_id: decision.physical_decision_id,
                    chosen_index: sample_blocker_choice(&mut self.rng, count),
                    candidate_count: count,
                }
            });
        }
        let include = match (&self.pending_combat, attackers) {
            (
                Some(PendingCombatChoiceV1::Attackers {
                    physical_decision_id,
                    mask,
                }),
                true,
            ) if *physical_decision_id == decision.physical_decision_id && mask.len() == count => {
                mask[index]
            }
            (
                Some(PendingCombatChoiceV1::Blockers {
                    physical_decision_id,
                    chosen_index,
                    candidate_count,
                }),
                false,
            ) if *physical_decision_id == decision.physical_decision_id
                && *candidate_count == count =>
            {
                *chosen_index == Some(index)
            }
            _ => return Err("V5 combat substeps do not match the sampled group".into()),
        };
        let selected = decision
            .legal_actions
            .iter()
            .position(|candidate| match &candidate.semantic {
                ActionSemanticV1::ChooseAttackerInclusion {
                    include: candidate_include,
                    ..
                } if attackers => *candidate_include == include,
                ActionSemanticV1::ChooseBlockerInclusion {
                    include: candidate_include,
                    ..
                } if !attackers => *candidate_include == include,
                _ => false,
            })
            .ok_or_else(|| "V5 combat decision omitted the sampled Boolean action".to_string())?;
        if index + 1 == count {
            self.pending_combat = None;
        }
        Ok(selected)
    }
}

fn sample_attacker_mask(rng: &mut SplitMix64, count: usize) -> Vec<bool> {
    (0..count).map(|_| rng_chance(rng, 1, 2)).collect()
}

fn sample_blocker_choice(rng: &mut SplitMix64, count: usize) -> Option<usize> {
    if count != 0 && rng_chance(rng, 35, 100) {
        Some(rng_below(rng, count))
    } else {
        None
    }
}

fn noncombat_action_count(decision: &Decision) -> Result<usize, String> {
    match decision {
        Decision::CastSpellOrPass {
            castable_spells,
            mana_abilities,
            land_drops,
            activatable_abilities,
            plot_actions,
            ..
        } => Ok(castable_spells.len()
            + mana_abilities.len()
            + land_drops.len()
            + activatable_abilities.len()
            + plot_actions.len()
            + 1),
        Decision::ChooseTargets { legal_targets, .. } => Ok(legal_targets.len()),
        Decision::ChooseCostTargets { candidates, .. } => Ok(candidates.len()),
        Decision::ChooseCastMode { options, .. } => Ok(options.len()),
        Decision::ChooseKicker { .. }
        | Decision::ChooseEffectBoolean { .. }
        | Decision::ChooseSpellCopyPayment { .. }
        | Decision::ChooseSpellCopyRetarget { .. }
        | Decision::ChooseMadnessCast { .. } => Ok(2),
        Decision::ChooseSpellMode { mode_count, .. } => Ok(*mode_count as usize),
        Decision::ChooseEffectOption { option_count, .. } => Ok(*option_count as usize),
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish,
            ..
        } => Ok(legal_targets.len() + usize::from(*can_finish)),
        Decision::ChooseOptionalCost {
            discard_payable,
            sacrifice_payable,
            ..
        } => match (*discard_payable, *sacrifice_payable) {
            (false, false) | (true, true) => Ok(2),
            flags => Err(format!(
                "unsupported H2 optional-cost presentation flags {flags:?}"
            )),
        },
        Decision::Discard { count, choices, .. } => {
            if *count != 1 {
                Err(format!("H2 discard reshape expected count=1, got {count}"))
            } else {
                Ok(choices.len())
            }
        }
        Decision::OrderTriggers { pending, .. } => factorial(pending.len()),
        Decision::DeclareAttackers { .. } => {
            Err("aggregate attackers must use grouped combat sampling".into())
        }
        Decision::DeclareBlockers { .. } => {
            Err("raw DeclareBlockers escaped the H2 per-attacker reshape".into())
        }
        Decision::GameOver { .. } | Decision::Halted { .. } => Ok(0),
    }
}

fn factorial(n: usize) -> Result<usize, String> {
    (1..=n).try_fold(1usize, |value, item| {
        value
            .checked_mul(item)
            .ok_or_else(|| "trigger-order action count overflow".to_string())
    })
}

fn noncombat_action_by_index(decision: &Decision, index: usize) -> Result<Action, String> {
    let action = match decision {
        Decision::CastSpellOrPass {
            castable_spells,
            mana_abilities,
            land_drops,
            activatable_abilities,
            plot_actions,
            ..
        } => {
            let mut cursor = index;
            if let Some(&id) = castable_spells.get(cursor) {
                return Ok(Action::CastSpell(id));
            }
            cursor = cursor.saturating_sub(castable_spells.len());
            if let Some(&id) = mana_abilities.get(cursor) {
                return Ok(Action::ActivateManaAbility(id));
            }
            cursor = cursor.saturating_sub(mana_abilities.len());
            if let Some(&id) = land_drops.get(cursor) {
                return Ok(Action::PlayLand(id));
            }
            cursor = cursor.saturating_sub(land_drops.len());
            if let Some(&(id, ability_index)) = activatable_abilities.get(cursor) {
                return Ok(Action::ActivateAbility(id, ability_index));
            }
            cursor = cursor.saturating_sub(activatable_abilities.len());
            if let Some(&id) = plot_actions.get(cursor) {
                return Ok(Action::PlotSpell(id));
            }
            cursor = cursor.saturating_sub(plot_actions.len());
            if cursor == 0 {
                Action::Pass
            } else {
                return Err("cast-or-pass policy index out of range".into());
            }
        }
        Decision::ChooseTargets { legal_targets, .. } => Action::ChooseTarget(
            *legal_targets
                .get(index)
                .ok_or_else(|| "target policy index out of range".to_string())?,
        ),
        Decision::ChooseCostTargets { candidates, .. } => Action::ChooseCostTarget(
            *candidates
                .get(index)
                .ok_or_else(|| "cost-target policy index out of range".to_string())?,
        ),
        Decision::ChooseCastMode { options, .. } => Action::ChooseCastMode(
            *options
                .get(index)
                .ok_or_else(|| "cast-mode policy index out of range".to_string())?,
        ),
        Decision::ChooseKicker { .. } => Action::ChooseKicker(index == 1),
        Decision::ChooseSpellMode { mode_count, .. } if index < *mode_count as usize => {
            Action::ChooseSpellMode(index as u8)
        }
        Decision::ChooseEffectOption { option_count, .. } if index < *option_count as usize => {
            Action::ChooseEffectOption(index as u16)
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            if let Some(&target) = legal_targets.get(index) {
                Action::ChooseEffectTarget(target)
            } else if *can_finish && index == legal_targets.len() {
                Action::FinishEffectSelection
            } else {
                return Err("effect-target policy index out of range".into());
            }
        }
        Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(index == 1),
        Decision::ChooseOptionalCost {
            discard_payable,
            sacrifice_payable,
            ..
        } => match (*discard_payable, *sacrifice_payable, index) {
            (false, false, 0) => Action::ChooseOptionalCostStage(false),
            (false, false, 1) => Action::ChooseOptionalCostStage(true),
            (true, true, 0) => Action::ChooseOptionalCostStage(true),
            (true, true, 1) => Action::ChooseOptionalCostStage(false),
            _ => return Err("optional-cost policy index out of range".into()),
        },
        Decision::ChooseSpellCopyPayment { .. } => Action::ChooseSpellCopyPayment(index == 0),
        Decision::ChooseSpellCopyRetarget { .. } => Action::ChooseSpellCopyRetarget(index == 0),
        Decision::ChooseMadnessCast { .. } => Action::ChooseMadnessCast(index == 1),
        Decision::Discard { count, choices, .. } if *count == 1 => Action::Discard(vec![*choices
            .get(index)
            .ok_or_else(|| "discard policy index out of range".to_string())?]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers(nth_permutation(pending.len(), index)?)
        }
        Decision::DeclareAttackers { .. }
        | Decision::DeclareBlockers { .. }
        | Decision::GameOver { .. }
        | Decision::Halted { .. }
        | Decision::ChooseSpellMode { .. }
        | Decision::ChooseEffectOption { .. }
        | Decision::Discard { .. } => return Err("noncombat policy index out of range".into()),
    };
    Ok(action)
}

fn nth_permutation(n: usize, selected: usize) -> Result<Vec<usize>, String> {
    let total = factorial(n)?;
    if selected >= total {
        return Err("trigger-order policy index out of range".into());
    }
    let mut current: Vec<usize> = (0..n).collect();
    let mut remaining = selected;
    for start in 0..n {
        let block = factorial(n - start - 1)?;
        let choice = if block == 0 { 0 } else { remaining / block };
        remaining %= block.max(1);
        current.swap(start, start + choice);
    }
    Ok(current)
}

fn play_harness_surface_v2(
    deck: &'static RuntimeDeckDefinition,
    seeds: EpisodeSeedBundleV1,
    decision_cap: u64,
) -> LaneGameResultV1 {
    let mut state = build_mirror_state_from_ids(deck.card_ids, seeds.env_seed);
    let mut surface = HarnessSurfaceV2::new_with_suppression_audit_mode(SuppressionAuditMode::Off);
    let mut policy = SeededUniformH2PolicyV1::new(seeds.policy_seed);
    let mut physical_decisions = 0u64;
    let outcome = loop {
        let decision = surface.next_decision(&mut state);
        match &decision {
            SurfaceDecision::Decision(Decision::GameOver { .. }) => {
                break GameOutcomeV1::NaturalTerminal;
            }
            SurfaceDecision::Decision(Decision::Halted { .. }) => {
                break GameOutcomeV1::Halted;
            }
            _ => {}
        }
        // Match `RlEpisodeSessionV1`: advance/suppress to the next H2
        // boundary first, allow a natural/halted terminal at that boundary,
        // and only then truncate before applying another physical decision.
        if physical_decisions >= decision_cap {
            break GameOutcomeV1::PhysicalDecisionCap;
        }
        let action = match policy.surface_action(&decision) {
            Ok(action) => action,
            Err(_) => break GameOutcomeV1::DriverError,
        };
        if surface.apply(&mut state, action).is_err() {
            break GameOutcomeV1::ApplyError;
        }
        physical_decisions = physical_decisions.saturating_add(1);
    };
    LaneGameResultV1 {
        outcome,
        policy_steps: physical_decisions,
        physical_decisions,
        diagnostic_state_hash: equivalence_hash_for_state(&state),
    }
}

fn play_rl_session_v5(
    deck: &'static RuntimeDeckDefinition,
    seeds: EpisodeSeedBundleV1,
    decision_cap: u64,
) -> LaneGameResultV1 {
    let deck_ids: SessionDeckIdsV1 = [deck.id.to_string(), deck.id.to_string()];
    let policy_cap = decision_cap.saturating_mul(128).max(1);
    let mut session = match RlEpisodeSessionV1::reset_with_decks_and_limits(
        seeds.episode_id,
        seeds.env_seed,
        decision_cap,
        policy_cap,
        deck_ids,
    ) {
        Ok(session) => session,
        Err(_) => {
            return LaneGameResultV1 {
                outcome: GameOutcomeV1::DriverError,
                policy_steps: 0,
                physical_decisions: 0,
                diagnostic_state_hash: 0,
            };
        }
    };
    let mut policy = SeededUniformH2PolicyV1::new(seeds.policy_seed);

    // This is the only explicit response materialization after reset.  Every
    // subsequent response is the value returned by `step`; calling
    // `current_response` again would add a second production clone and make
    // this boundary measurement dishonest.
    let mut response = session.current_response();
    let outcome = loop {
        match response {
            RlSessionResponseV1::Terminal(terminal) => {
                break session_terminal_outcome(
                    terminal.terminal_classification,
                    &terminal.terminal_reason,
                );
            }
            RlSessionResponseV1::Decision(decision) => {
                let (selected_index, selected_action_id) = match policy.session_action(&decision) {
                    Ok(selected) => selected,
                    Err(_) => break GameOutcomeV1::DriverError,
                };
                response = match session.step(
                    seeds.episode_id,
                    decision.step,
                    selected_index,
                    &selected_action_id,
                ) {
                    Ok(response) => response,
                    Err(_) => break GameOutcomeV1::DriverError,
                };
            }
        }
    };
    LaneGameResultV1 {
        outcome,
        policy_steps: session.policy_step_count(),
        physical_decisions: session.physical_decision_count(),
        diagnostic_state_hash: equivalence_hash_for_session(&session),
    }
}

fn session_terminal_outcome(
    classification: TerminalClassificationV1,
    reason: &str,
) -> GameOutcomeV1 {
    match classification {
        TerminalClassificationV1::Natural => GameOutcomeV1::NaturalTerminal,
        TerminalClassificationV1::Truncated
            if reason.starts_with("physical_decision_cap_reached:") =>
        {
            GameOutcomeV1::PhysicalDecisionCap
        }
        TerminalClassificationV1::Truncated if reason.starts_with("policy_step_cap_reached:") => {
            GameOutcomeV1::PolicyStepCap
        }
        TerminalClassificationV1::Truncated => GameOutcomeV1::DriverError,
        TerminalClassificationV1::Halted if reason.starts_with("fail_closed:") => {
            GameOutcomeV1::FailClosed
        }
        TerminalClassificationV1::Halted if reason.starts_with("engine_halted:") => {
            GameOutcomeV1::Halted
        }
        TerminalClassificationV1::Halted => GameOutcomeV1::DriverError,
    }
}

fn play_lane(
    lane: LaneKindV1,
    deck: &'static RuntimeDeckDefinition,
    seeds: EpisodeSeedBundleV1,
) -> LaneGameResultV1 {
    match lane {
        LaneKindV1::EngineRaw => play_engine_raw(deck, seeds, DECISION_SAFETY_CAP),
        LaneKindV1::HarnessSurfaceV2 => play_harness_surface_v2(deck, seeds, DECISION_SAFETY_CAP),
        LaneKindV1::RlSessionV5Inproc => play_rl_session_v5(deck, seeds, DECISION_SAFETY_CAP),
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
struct OutcomeCountsV1 {
    attempted_games: u64,
    natural_terminal_games: u64,
    physical_decision_cap_truncations: u64,
    policy_step_cap_truncations: u64,
    halted_games: u64,
    apply_errors: u64,
    fail_closed_games: u64,
    driver_errors: u64,
}

impl OutcomeCountsV1 {
    fn record(&mut self, outcome: GameOutcomeV1) {
        self.attempted_games = self.attempted_games.saturating_add(1);
        match outcome {
            GameOutcomeV1::NaturalTerminal => {
                self.natural_terminal_games = self.natural_terminal_games.saturating_add(1)
            }
            GameOutcomeV1::PhysicalDecisionCap => {
                self.physical_decision_cap_truncations =
                    self.physical_decision_cap_truncations.saturating_add(1)
            }
            GameOutcomeV1::PolicyStepCap => {
                self.policy_step_cap_truncations =
                    self.policy_step_cap_truncations.saturating_add(1)
            }
            GameOutcomeV1::Halted => self.halted_games = self.halted_games.saturating_add(1),
            GameOutcomeV1::ApplyError => self.apply_errors = self.apply_errors.saturating_add(1),
            GameOutcomeV1::FailClosed => {
                self.fail_closed_games = self.fail_closed_games.saturating_add(1)
            }
            GameOutcomeV1::DriverError => self.driver_errors = self.driver_errors.saturating_add(1),
        }
    }

    fn merge(&mut self, other: Self) {
        self.attempted_games = self.attempted_games.saturating_add(other.attempted_games);
        self.natural_terminal_games = self
            .natural_terminal_games
            .saturating_add(other.natural_terminal_games);
        self.physical_decision_cap_truncations = self
            .physical_decision_cap_truncations
            .saturating_add(other.physical_decision_cap_truncations);
        self.policy_step_cap_truncations = self
            .policy_step_cap_truncations
            .saturating_add(other.policy_step_cap_truncations);
        self.halted_games = self.halted_games.saturating_add(other.halted_games);
        self.apply_errors = self.apply_errors.saturating_add(other.apply_errors);
        self.fail_closed_games = self
            .fail_closed_games
            .saturating_add(other.fail_closed_games);
        self.driver_errors = self.driver_errors.saturating_add(other.driver_errors);
    }

    fn is_exact(&self) -> bool {
        self.attempted_games
            == self
                .natural_terminal_games
                .saturating_add(self.physical_decision_cap_truncations)
                .saturating_add(self.policy_step_cap_truncations)
                .saturating_add(self.halted_games)
                .saturating_add(self.apply_errors)
                .saturating_add(self.fail_closed_games)
                .saturating_add(self.driver_errors)
    }

    fn all_natural(&self) -> bool {
        self.is_exact()
            && self.physical_decision_cap_truncations == 0
            && self.policy_step_cap_truncations == 0
            && self.halted_games == 0
            && self.apply_errors == 0
            && self.fail_closed_games == 0
            && self.driver_errors == 0
    }
}

#[derive(Debug, Serialize)]
struct LaneTrialV1 {
    lane: LaneKindV1,
    actors: usize,
    warmup: OutcomeCountsV1,
    measurement: OutcomeCountsV1,
    warmup_seed_partition_exhausted: bool,
    measurement_seed_partition_exhausted: bool,
    outcomes_valid: bool,
    policy_steps: u64,
    physical_decisions: u64,
    actor_seed_starts: Vec<EpisodeSeedBundleV1>,
    actor_warmup_seed_starts: Vec<EpisodeSeedBundleV1>,
    actor_attempt_counts: Vec<u64>,
    actor_finish_ns: Vec<u64>,
    measure_target_ns: u64,
    common_wall_ns: u64,
    overshoot_ns: u64,
    natural_games_per_second: f64,
    policy_steps_per_second: f64,
    physical_decisions_per_second: f64,
    natural_games_per_second_per_actor: f64,
}

#[derive(Debug)]
struct ActorResultV1 {
    actor: usize,
    seed_start: EpisodeSeedBundleV1,
    warmup_seed_start: EpisodeSeedBundleV1,
    warmup: OutcomeCountsV1,
    measurement: OutcomeCountsV1,
    warmup_seed_partition_exhausted: bool,
    measurement_seed_partition_exhausted: bool,
    policy_steps: u64,
    physical_decisions: u64,
    finish_ns: u64,
}

fn duration_ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn measure_lane(
    lane: LaneKindV1,
    deck: &'static RuntimeDeckDefinition,
    actors: usize,
    warmup: Duration,
    measure: Duration,
    base_seed: u64,
) -> LaneTrialV1 {
    let barrier = Arc::new(Barrier::new(actors));
    let shared_start = Arc::new(OnceLock::new());
    let handles: Vec<_> = (0..actors)
        .map(|actor| {
            let barrier = Arc::clone(&barrier);
            let shared_start = Arc::clone(&shared_start);
            std::thread::spawn(move || {
                let seed_start = seed_bundle(base_seed, false, actor, 0)
                    .expect("validated actor has a measurement partition");
                let warmup_seed_start = seed_bundle(base_seed, true, actor, 0)
                    .expect("validated actor has a warmup partition");
                let warmup_start = Instant::now();
                let mut warmup_counts = OutcomeCountsV1::default();
                let mut warmup_seed_partition_exhausted = false;
                while warmup_start.elapsed() < warmup {
                    let Some(seeds) =
                        seed_bundle(base_seed, true, actor, warmup_counts.attempted_games)
                    else {
                        warmup_seed_partition_exhausted = true;
                        break;
                    };
                    warmup_counts.record(play_lane(lane, deck, seeds).outcome);
                }

                barrier.wait();
                if actor == 0 {
                    shared_start
                        .set(Instant::now())
                        .expect("actor zero sets the shared measurement start once");
                }
                barrier.wait();
                let start = *shared_start.get().expect("actor zero set shared start");
                let deadline = start + measure;
                let mut measurement_counts = OutcomeCountsV1::default();
                let mut measurement_seed_partition_exhausted = false;
                let mut policy_steps = 0u64;
                let mut physical_decisions = 0u64;
                while Instant::now() < deadline {
                    let Some(seeds) =
                        seed_bundle(base_seed, false, actor, measurement_counts.attempted_games)
                    else {
                        measurement_seed_partition_exhausted = true;
                        break;
                    };
                    let result = play_lane(lane, deck, seeds);
                    measurement_counts.record(result.outcome);
                    policy_steps = policy_steps.saturating_add(result.policy_steps);
                    physical_decisions =
                        physical_decisions.saturating_add(result.physical_decisions);
                }
                ActorResultV1 {
                    actor,
                    seed_start,
                    warmup_seed_start,
                    warmup: warmup_counts,
                    measurement: measurement_counts,
                    warmup_seed_partition_exhausted,
                    measurement_seed_partition_exhausted,
                    policy_steps,
                    physical_decisions,
                    finish_ns: duration_ns(start.elapsed()),
                }
            })
        })
        .collect();

    let mut warmup_counts = OutcomeCountsV1::default();
    let mut measurement_counts = OutcomeCountsV1::default();
    let mut warmup_seed_partition_exhausted = false;
    let mut measurement_seed_partition_exhausted = false;
    let mut policy_steps = 0u64;
    let mut physical_decisions = 0u64;
    let mut actor_seed_starts = vec![seed_bundle(base_seed, false, 0, 0).unwrap(); actors];
    let mut actor_warmup_seed_starts = vec![seed_bundle(base_seed, true, 0, 0).unwrap(); actors];
    let mut actor_attempt_counts = vec![0u64; actors];
    let mut actor_finish_ns = vec![0u64; actors];
    for handle in handles {
        let actor = handle.join().expect("three-lane ceiling worker panicked");
        actor_seed_starts[actor.actor] = actor.seed_start;
        actor_warmup_seed_starts[actor.actor] = actor.warmup_seed_start;
        actor_attempt_counts[actor.actor] = actor.measurement.attempted_games;
        actor_finish_ns[actor.actor] = actor.finish_ns;
        warmup_counts.merge(actor.warmup);
        measurement_counts.merge(actor.measurement);
        warmup_seed_partition_exhausted |= actor.warmup_seed_partition_exhausted;
        measurement_seed_partition_exhausted |= actor.measurement_seed_partition_exhausted;
        policy_steps = policy_steps.saturating_add(actor.policy_steps);
        physical_decisions = physical_decisions.saturating_add(actor.physical_decisions);
    }
    assert!(warmup_counts.is_exact(), "warmup outcomes are exhaustive");
    assert!(
        measurement_counts.is_exact(),
        "measurement outcomes are exhaustive"
    );
    let measure_target_ns = duration_ns(measure);
    let common_wall_ns = actor_finish_ns.iter().copied().max().unwrap_or(0);
    let overshoot_ns = common_wall_ns.saturating_sub(measure_target_ns);
    let seconds = common_wall_ns as f64 / 1_000_000_000.0;
    let natural_games_per_second = measurement_counts.natural_terminal_games as f64 / seconds;
    let policy_steps_per_second = policy_steps as f64 / seconds;
    let physical_decisions_per_second = physical_decisions as f64 / seconds;
    let natural_games_per_second_per_actor = natural_games_per_second / actors as f64;
    assert!(
        natural_games_per_second.is_finite()
            && policy_steps_per_second.is_finite()
            && physical_decisions_per_second.is_finite()
            && natural_games_per_second_per_actor.is_finite(),
        "three-lane rates must be finite"
    );
    LaneTrialV1 {
        lane,
        actors,
        warmup: warmup_counts,
        measurement: measurement_counts,
        warmup_seed_partition_exhausted,
        measurement_seed_partition_exhausted,
        outcomes_valid: warmup_counts.all_natural()
            && measurement_counts.all_natural()
            && actor_attempt_counts.iter().all(|attempts| *attempts != 0)
            && !warmup_seed_partition_exhausted
            && !measurement_seed_partition_exhausted,
        policy_steps,
        physical_decisions,
        actor_seed_starts,
        actor_warmup_seed_starts,
        actor_attempt_counts,
        actor_finish_ns,
        measure_target_ns,
        common_wall_ns,
        overshoot_ns,
        natural_games_per_second,
        policy_steps_per_second,
        physical_decisions_per_second,
        natural_games_per_second_per_actor,
    }
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
struct HardwareRecordV1 {
    available_parallelism: usize,
}

#[derive(Serialize)]
struct LaneDefinitionV1 {
    lane: LaneKindV1,
    includes: &'static str,
    policy_adapter: &'static str,
    suppression_audit_mode: &'static str,
}

#[derive(Serialize)]
struct WorkloadRecordV1<'a> {
    claim_scope: &'static str,
    h2_v5_paired_policy: &'static str,
    h2_v5_paired_policy_description: &'static str,
    lanes: [LaneDefinitionV1; 3],
    exclusions: &'static [&'static str],
    actor_counts: &'a [usize],
    warmup_ns: u64,
    measure_target_ns: u64,
    base_seed: u64,
    seed_schedule: &'static str,
    seed_partition_stride: u64,
    partitions_per_domain: u64,
    warmup_partition_offset: u64,
    episode_domain_offset: u64,
    env_domain_offset: u64,
    policy_domain_offset: u64,
    per_actor_game_seed_increment: u64,
    decision_safety_cap: u64,
}

#[derive(Serialize)]
struct ComparabilityRecordV1 {
    h2_v5_seed_prefix_paired: bool,
    h2_v5_policy_semantics_paired: bool,
    h2_v5_pairing_scope: &'static str,
    h2_v5_measured_episode_sets_identical: bool,
    engine_raw_h2_trajectory_equivalent: bool,
    engine_raw_h2_non_equivalence_reason: &'static str,
    raw_non_natural_outcome_policy: &'static str,
    timing_denominator: &'static str,
    lane_execution: &'static str,
}

#[derive(Serialize)]
struct ActorTrialV1 {
    actors: usize,
    lanes: Vec<LaneTrialV1>,
}

#[derive(Serialize)]
struct ThreeLaneRecordV1<'a> {
    schema: &'static str,
    claim_scope: &'static str,
    deck: DeckRecordV1<'a>,
    binary: BinaryRecordV1<'a>,
    hardware: HardwareRecordV1,
    workload: WorkloadRecordV1<'a>,
    comparability: ComparabilityRecordV1,
    trials: Vec<ActorTrialV1>,
}

fn build_record<'a>(
    config: &'a ThreeLaneConfigV1,
    deck: &'static RuntimeDeckDefinition,
    trials: Vec<ActorTrialV1>,
) -> ThreeLaneRecordV1<'a> {
    ThreeLaneRecordV1 {
        schema: SCHEMA_V1,
        claim_scope: "rust_in_process_environment_ceiling_not_end_to_end_training",
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
        hardware: HardwareRecordV1 {
            available_parallelism: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
        },
        workload: WorkloadRecordV1 {
            claim_scope: "rust_in_process_environment_ceiling_not_end_to_end_training",
            h2_v5_paired_policy: POLICY_V1,
            h2_v5_paired_policy_description: "uniform legal noncombat choice; independent one-half attacker inclusion; per-attacker no-block-or-one-block choice with 35-percent block probability; V5 pre-samples each aggregate H2 combat group",
            lanes: [
                LaneDefinitionV1 {
                    lane: LaneKindV1::EngineRaw,
                    includes: "engine_advance_step_only",
                    policy_adapter: "seeded_random_legal_raw_decision_adapter/v1",
                    suppression_audit_mode: "not_applicable",
                },
                LaneDefinitionV1 {
                    lane: LaneKindV1::HarnessSurfaceV2,
                    includes: "engine_plus_harness_surface_v2_without_stable_ids_or_observations",
                    policy_adapter: POLICY_V1,
                    suppression_audit_mode: SuppressionAuditMode::Off.as_str(),
                },
                LaneDefinitionV1 {
                    lane: LaneKindV1::RlSessionV5Inproc,
                    includes: "rl_episode_session_v1_policy_surface_v5_observations_legal_actions_privileged_integrity_transactional_clone_single_response_materialization",
                    policy_adapter: POLICY_V1,
                    suppression_audit_mode: SuppressionAuditMode::Off.as_str(),
                },
            ],
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
            actor_counts: &config.actors,
            warmup_ns: duration_ns(config.warmup),
            measure_target_ns: duration_ns(config.measure),
            base_seed: config.seed,
            seed_schedule: "wrapping_u64_domain_phase_actor_partition_plus_game_index/v1",
            seed_partition_stride: PARTITION_STRIDE,
            partitions_per_domain: PHASE_PARTITIONS,
            warmup_partition_offset: WARMUP_PARTITION_OFFSET,
            episode_domain_offset: EPISODE_DOMAIN_OFFSET,
            env_domain_offset: ENV_DOMAIN_OFFSET,
            policy_domain_offset: POLICY_DOMAIN_OFFSET,
            per_actor_game_seed_increment: 1,
            decision_safety_cap: DECISION_SAFETY_CAP,
        },
        comparability: ComparabilityRecordV1 {
            h2_v5_seed_prefix_paired: true,
            h2_v5_policy_semantics_paired: true,
            h2_v5_pairing_scope: "same actor/game-index prefixes use identical episode/env/policy bundles; V5 grouped combat Boolean scans commit the same aggregate H2 action",
            h2_v5_measured_episode_sets_identical: false,
            engine_raw_h2_trajectory_equivalent: false,
            engine_raw_h2_non_equivalence_reason: "raw engine exposes priority and aggregate combat windows that HarnessSurfaceV2 suppresses or reshapes, so equal seeds do not imply equal trajectories",
            raw_non_natural_outcome_policy: "any raw halt, cap, apply error, fail-closed result, or driver error invalidates that lane trial and routes to a separate raw-lane correctness investigation; it is never hidden or promoted as H2/V5 evidence",
            timing_denominator: "maximum actor finish offset from one shared start after every counted game completed",
            lane_execution: "sequential_lanes_per_actor_count_no_cross_lane_overlap/v1",
        },
        trials,
    }
}

pub(crate) fn run_three_lane_ceiling_json_v1(config: ThreeLaneConfigV1) {
    let deck = runtime_deck_by_id(&config.deck_id).expect("validated runtime deck");
    let trials = config
        .actors
        .iter()
        .copied()
        .map(|actors| ActorTrialV1 {
            actors,
            lanes: [
                LaneKindV1::EngineRaw,
                LaneKindV1::HarnessSurfaceV2,
                LaneKindV1::RlSessionV5Inproc,
            ]
            .into_iter()
            .map(|lane| {
                measure_lane(
                    lane,
                    deck,
                    actors,
                    config.warmup,
                    config.measure,
                    config.seed,
                )
            })
            .collect(),
        })
        .collect();
    let record = build_record(&config, deck, trials);
    println!(
        "{}",
        serde_json::to_string(&record).expect("three-lane ceiling record serializes")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "7735d368117c20211c66e72cb5efc71e1bd4d74f";

    fn config() -> ThreeLaneConfigV1 {
        ThreeLaneConfigV1 {
            git_commit: COMMIT.to_string(),
            deck_id: "Rally".to_string(),
            actors: vec![1],
            warmup: Duration::ZERO,
            measure: Duration::from_millis(1),
            seed: u64::MAX - 71_500,
        }
    }

    #[test]
    fn cli_is_strict_and_defaults_are_explicit() {
        let parsed = ThreeLaneConfigV1::parse(&[
            "--git-commit".into(),
            COMMIT.into(),
            "--deck".into(),
            "Burn".into(),
            "--actors".into(),
            "1,4,8,16".into(),
            "--warmup-ms".into(),
            "0".into(),
            "--measure-ms".into(),
            "1".into(),
            "--seed".into(),
            "71501".into(),
        ])
        .unwrap();
        assert_eq!(parsed.deck_id, "Burn");
        assert_eq!(parsed.actors, [1, 4, 8, 16]);
        assert_eq!(parsed.warmup, Duration::ZERO);
        assert_eq!(parsed.measure, Duration::from_millis(1));
        assert_eq!(parsed.seed, 71501);

        assert!(ThreeLaneConfigV1::parse(&[]).is_err());
        assert!(ThreeLaneConfigV1::parse(&[
            "--git-commit".into(),
            COMMIT.into(),
            "--actors".into(),
            "1,1".into(),
        ])
        .is_err());
        assert!(ThreeLaneConfigV1::parse(&[
            "--git-commit".into(),
            COMMIT.into(),
            "--deck".into(),
            "rally".into(),
        ])
        .is_err());
    }

    #[test]
    fn all_seed_domains_phases_and_actor_partitions_are_disjoint() {
        let base = u64::MAX - 71_500;
        let mut values = HashSet::new();
        for domain in [
            EPISODE_DOMAIN_OFFSET,
            ENV_DOMAIN_OFFSET,
            POLICY_DOMAIN_OFFSET,
        ] {
            for warmup in [false, true] {
                for actor in 0..16 {
                    for game_index in 0..10_000 {
                        assert!(values.insert(
                            partition_value(base, domain, warmup, actor, game_index).unwrap()
                        ));
                    }
                }
            }
        }
        assert_eq!(values.len(), 3 * 2 * 16 * 10_000);
        assert!(seed_bundle(base, false, 256, 0).is_none());
        assert!(seed_bundle(base, false, 0, PARTITION_STRIDE).is_none());
    }

    #[test]
    fn combat_group_sampling_consumes_rng_once_for_the_whole_mask() {
        let seed = 0xfeed_beef;
        let mut aggregate = SplitMix64::seed(seed);
        let expected_attackers = sample_attacker_mask(&mut aggregate, 7);
        let expected_blocker = sample_blocker_choice(&mut aggregate, 5);
        let expected_next = aggregate.next_u64();

        let mut grouped = SplitMix64::seed(seed);
        let mask = sample_attacker_mask(&mut grouped, 7);
        for (index, expected) in expected_attackers.into_iter().enumerate() {
            assert_eq!(mask[index], expected);
        }
        let blocker = sample_blocker_choice(&mut grouped, 5);
        assert_eq!(blocker, expected_blocker);
        assert_eq!(grouped.next_u64(), expected_next);
    }

    #[test]
    fn h2_and_v5_fixed_seed_prefix_reach_identical_state() {
        let deck = runtime_deck_by_id("Rally").unwrap();
        for game_index in 0..3 {
            let seeds = seed_bundle(71_501, false, 0, game_index).unwrap();
            for cap in 1..=64 {
                let h2 = play_harness_surface_v2(deck, seeds, cap);
                let v5 = play_rl_session_v5(deck, seeds, cap);
                assert_eq!(h2.outcome, v5.outcome, "game_index={game_index} cap={cap}");
                assert_eq!(
                    h2.physical_decisions, v5.physical_decisions,
                    "game_index={game_index} cap={cap}"
                );
                assert_eq!(
                    h2.diagnostic_state_hash, v5.diagnostic_state_hash,
                    "game_index={game_index} cap={cap}"
                );
                assert!(v5.policy_steps >= v5.physical_decisions);
            }
        }

        for deck_id in ["Burn", "Rally"] {
            let deck = runtime_deck_by_id(deck_id).unwrap();
            let seeds = seed_bundle(71_501, false, 0, 0).unwrap();
            let h2 = play_harness_surface_v2(deck, seeds, DECISION_SAFETY_CAP);
            let v5 = play_rl_session_v5(deck, seeds, DECISION_SAFETY_CAP);
            assert_eq!(h2.outcome, GameOutcomeV1::NaturalTerminal, "{deck_id}");
            assert_eq!(h2.outcome, v5.outcome, "{deck_id}");
            assert_eq!(h2.physical_decisions, v5.physical_decisions, "{deck_id}");
            assert_eq!(
                h2.diagnostic_state_hash, v5.diagnostic_state_hash,
                "{deck_id}"
            );
            assert!(v5.policy_steps >= v5.physical_decisions, "{deck_id}");
            if deck_id == "Rally" {
                assert!(v5.policy_steps > v5.physical_decisions);
            }
        }
    }

    #[test]
    fn outcome_mapping_and_counter_invariants_are_exhaustive() {
        assert_eq!(
            session_terminal_outcome(TerminalClassificationV1::Natural, "game_over"),
            GameOutcomeV1::NaturalTerminal
        );
        assert_eq!(
            session_terminal_outcome(
                TerminalClassificationV1::Truncated,
                "physical_decision_cap_reached:64"
            ),
            GameOutcomeV1::PhysicalDecisionCap
        );
        assert_eq!(
            session_terminal_outcome(
                TerminalClassificationV1::Truncated,
                "policy_step_cap_reached:8192"
            ),
            GameOutcomeV1::PolicyStepCap
        );
        assert_eq!(
            session_terminal_outcome(TerminalClassificationV1::Truncated, "unknown"),
            GameOutcomeV1::DriverError
        );
        assert_eq!(
            session_terminal_outcome(
                TerminalClassificationV1::Halted,
                "engine_halted:Unsupported:source:1"
            ),
            GameOutcomeV1::Halted
        );
        assert_eq!(
            session_terminal_outcome(
                TerminalClassificationV1::Halted,
                "fail_closed:session_integrity"
            ),
            GameOutcomeV1::FailClosed
        );
        assert_eq!(
            session_terminal_outcome(TerminalClassificationV1::Halted, "unknown"),
            GameOutcomeV1::DriverError
        );

        let mut counts = OutcomeCountsV1::default();
        for outcome in [
            GameOutcomeV1::NaturalTerminal,
            GameOutcomeV1::PhysicalDecisionCap,
            GameOutcomeV1::PolicyStepCap,
            GameOutcomeV1::Halted,
            GameOutcomeV1::ApplyError,
            GameOutcomeV1::FailClosed,
            GameOutcomeV1::DriverError,
        ] {
            counts.record(outcome);
        }
        assert!(counts.is_exact());
        assert_eq!(counts.attempted_games, 7);
        assert!(!counts.all_natural());
        let mut natural = OutcomeCountsV1::default();
        natural.record(GameOutcomeV1::NaturalTerminal);
        assert!(natural.all_natural());
    }

    #[test]
    fn raw_exact_cap_still_recognizes_the_natural_terminal() {
        let deck = runtime_deck_by_id("Burn").unwrap();
        let seeds = seed_bundle(71_501, false, 0, 17).unwrap();
        let uncapped = play_engine_raw(deck, seeds, DECISION_SAFETY_CAP);
        assert_eq!(uncapped.outcome, GameOutcomeV1::NaturalTerminal);
        assert!(uncapped.physical_decisions > 1);

        let exact = play_engine_raw(deck, seeds, uncapped.physical_decisions);
        assert_eq!(exact.outcome, GameOutcomeV1::NaturalTerminal);
        assert_eq!(exact.physical_decisions, uncapped.physical_decisions);
        assert_eq!(exact.diagnostic_state_hash, uncapped.diagnostic_state_hash);

        let one_short = play_engine_raw(deck, seeds, uncapped.physical_decisions - 1);
        assert_eq!(one_short.outcome, GameOutcomeV1::PhysicalDecisionCap);
        assert_eq!(
            one_short.physical_decisions,
            uncapped.physical_decisions - 1
        );
    }

    #[test]
    fn common_wall_timing_uses_slowest_actor_and_exact_overshoot() {
        let actor_finish_ns = [1_100_000u64, 1_250_000, 1_175_000];
        let target = 1_000_000u64;
        let common = actor_finish_ns.into_iter().max().unwrap();
        assert_eq!(common, 1_250_000);
        assert_eq!(common.saturating_sub(target), 250_000);
        assert!(actor_finish_ns.into_iter().all(|finish| finish <= common));
    }

    #[test]
    fn record_serialization_is_one_line_finite_and_explicit() {
        let config = config();
        let record = build_record(&config, runtime_deck_by_id("Rally").unwrap(), Vec::new());
        let encoded = serde_json::to_string(&record).unwrap();
        assert!(!encoded.contains('\n'));
        assert!(!encoded.contains("NaN"));
        assert!(!encoded.contains("Infinity"));
        let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["schema"], SCHEMA_V1);
        assert_eq!(
            value["workload"]["claim_scope"],
            "rust_in_process_environment_ceiling_not_end_to_end_training"
        );
        assert_eq!(
            value["comparability"]["engine_raw_h2_trajectory_equivalent"],
            false
        );
        assert_eq!(
            value["workload"]["exclusions"][0],
            "jsonl_serialization_and_protocol"
        );
        assert_eq!(
            value["workload"]["lanes"][0]["suppression_audit_mode"],
            "not_applicable"
        );
        assert_eq!(
            value["workload"]["lanes"][1]["suppression_audit_mode"],
            "off"
        );
        assert_eq!(
            value["workload"]["lanes"][2]["suppression_audit_mode"],
            "off"
        );
    }

    #[test]
    fn nth_permutation_matches_rl_recursive_swap_order() {
        assert_eq!(nth_permutation(3, 0).unwrap(), [0, 1, 2]);
        assert_eq!(nth_permutation(3, 1).unwrap(), [0, 2, 1]);
        assert_eq!(nth_permutation(3, 2).unwrap(), [1, 0, 2]);
        assert_eq!(nth_permutation(3, 3).unwrap(), [1, 2, 0]);
        assert_eq!(nth_permutation(3, 4).unwrap(), [2, 1, 0]);
        assert_eq!(nth_permutation(3, 5).unwrap(), [2, 0, 1]);
    }
}
