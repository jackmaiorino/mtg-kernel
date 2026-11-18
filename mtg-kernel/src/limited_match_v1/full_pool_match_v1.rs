//! Explicit terminal-search adapter for the sealed Foundations numeric owner.
use super::*;
use crate::{
    current_mixed_policy_v1::{KaitoDecision, KaitoSession},
    limited_full_pool_consumer_v1::{reset_numeric_reveal_facts_v3, ResetV1},
    rl_session::RlSessionTerminalV1,
};

const HEURISTIC_WORLD_TAG_V1: &[u8] = b"mtg-kernel/limited-sampled-public-heuristic-world/v1\0";
const HEURISTIC_TIE_TAG_V1: &[u8] = b"mtg-kernel/limited-sampled-public-heuristic-tie/v1\0";
const HEURISTIC_FALLBACK_TAG_V1: &[u8] =
    b"mtg-kernel/limited-sampled-public-heuristic-fallback/v1\0";
const HEURISTIC_SEARCH_TIE_TAG_V1: &[u8] = b"mtg-kernel/limited-pimc-heuristic-tie/v1\0";

fn encode_current(session: &KaitoSession) -> Result<KaitoDecision, String> {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        return Err("full-pool search has no current decision".into());
    };
    session.encode(expected).map_err(|error| error.to_string())
}

fn legal_count(decision: &KaitoDecision) -> Result<u32, String> {
    let count = u32::try_from(decision.packet().actions().len())
        .map_err(|_| "full-pool action count exceeds u32")?;
    if count == 0 {
        return Err("full-pool decision has no legal actions".into());
    }
    Ok(count)
}

fn terminal_outcome(terminal: RlSessionTerminalV1) -> GameOutcomeV1 {
    match (terminal.terminal_classification, terminal.terminal_outcome) {
        (TerminalClassificationV1::Natural, TerminalOutcomeV1::P0Win) => {
            GameOutcomeV1::Winner(PlayerId::P0)
        }
        (TerminalClassificationV1::Natural, TerminalOutcomeV1::P1Win) => {
            GameOutcomeV1::Winner(PlayerId::P1)
        }
        (TerminalClassificationV1::Natural, TerminalOutcomeV1::Draw) => GameOutcomeV1::Draw,
        // The trainer refuses the same inconsistent terminal.
        (TerminalClassificationV1::Natural, outcome) => GameOutcomeV1::Error(format!(
            "natural terminal has no W/D/L outcome: {outcome:?}"
        )),
        (TerminalClassificationV1::Truncated, _) => GameOutcomeV1::Stall,
        (TerminalClassificationV1::Halted, _) => GameOutcomeV1::Error(terminal.terminal_reason),
    }
}

fn rollout(
    mut world: KaitoSession,
    root: PlayerId,
    action: u32,
    depth: u32,
    max_turns: u32,
    rng: &mut SplitMix64,
) -> Result<f64, String> {
    let mut decision = encode_current(&world)?;
    let mut action = action;
    for step in 0..=depth {
        match world
            .apply(&decision, action)
            .map_err(|error| error.to_string())?
        {
            FastActorResponseV1::Terminal(terminal) => {
                return match terminal_outcome(terminal) {
                    GameOutcomeV1::Winner(winner) if winner == root => Ok(1.0),
                    GameOutcomeV1::Winner(_) => Ok(-1.0),
                    GameOutcomeV1::Draw | GameOutcomeV1::Stall => Ok(0.0),
                    GameOutcomeV1::Error(reason) => Err(format!("rollout halted: {reason}")),
                };
            }
            FastActorResponseV1::Decision(expected) if step < depth => {
                decision = world.encode(expected).map_err(|error| error.to_string())?;
                if decision.public_turn() > max_turns {
                    break;
                }
                action = random_index(rng, legal_count(&decision)?);
            }
            FastActorResponseV1::Decision(_) => break,
        }
    }
    // This profile values observed terminal outcomes only.
    Ok(0.0)
}

fn pimc_choose_full_pool_v1(
    session: &KaitoSession,
    decision: &KaitoDecision,
    sims: u32,
    depth: u32,
    max_turns: u32,
    belief: &LimitedBeliefV1,
    seed: u64,
) -> Result<(u32, PimcStatsV1), String> {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        return Err("full-pool search has no current decision".into());
    };
    if sims == 0 {
        return Err("full-pool search needs a positive simulation count".into());
    }
    let legal = legal_count(decision)?;
    Ok(pimc_choose_with_v1(
        legal,
        sims,
        seed,
        true,
        |world_seed| {
            session
                .sampled_full_pool_closed_list_v1(decision, world_seed, belief)
                .map_err(|error| error.to_string())
        },
        |sampled, action, rng| {
            rollout(
                sampled,
                seat(expected.acting_player),
                action,
                depth,
                max_turns,
                rng,
            )
        },
    ))
}

/// The value in `[-1, 1]` for `root` of a terminal reached inside a sampled
/// world: `None` for a session truncation, which the heuristic profiles
/// score like a cutoff.
fn natural_value_v1(terminal: RlSessionTerminalV1, root: PlayerId) -> Result<Option<f64>, String> {
    match terminal_outcome(terminal) {
        GameOutcomeV1::Winner(winner) if winner == root => Ok(Some(1.0)),
        GameOutcomeV1::Winner(_) => Ok(Some(-1.0)),
        GameOutcomeV1::Draw => Ok(Some(0.0)),
        GameOutcomeV1::Stall => Ok(None),
        GameOutcomeV1::Error(reason) => Err(format!("rollout halted: {reason}")),
    }
}

/// A cutoff inside a sampled world, scored as `limited_match_v1::rollout_value`
/// scores one on the raw session: `PIMC_HEURISTIC_SCALE_V1 * evaluate_state_v1
/// / 10_000`, inside `[-0.45, 0.45]` since the evaluator clamps at 9,000.
fn heuristic_leaf_value_v1(world: &KaitoSession, root: PlayerId) -> Result<f64, String> {
    let score = world
        .sampled_world_state_eval_v1(root)
        .map_err(|error| error.to_string())?;
    Ok(PIMC_HEURISTIC_SCALE_V1 * f64::from(score) / 10_000.0)
}

/// `rollout` with a heuristic leaf: the same steps and random draws, but a
/// depth or turn cutoff (or a session truncation) is scored by
/// [`heuristic_leaf_value_v1`] instead of `0`.
fn rollout_heuristic_v1(
    mut world: KaitoSession,
    root: PlayerId,
    action: u32,
    depth: u32,
    max_turns: u32,
    rng: &mut SplitMix64,
) -> Result<f64, String> {
    let mut decision = encode_current(&world)?;
    let mut action = action;
    for step in 0..=depth {
        match world
            .apply(&decision, action)
            .map_err(|error| error.to_string())?
        {
            FastActorResponseV1::Terminal(terminal) => {
                return match natural_value_v1(terminal, root)? {
                    Some(value) => Ok(value),
                    None => heuristic_leaf_value_v1(&world, root),
                };
            }
            FastActorResponseV1::Decision(expected) if step < depth => {
                decision = world.encode(expected).map_err(|error| error.to_string())?;
                if decision.public_turn() > max_turns {
                    break;
                }
                action = random_index(rng, legal_count(&decision)?);
            }
            FastActorResponseV1::Decision(_) => break,
        }
    }
    heuristic_leaf_value_v1(&world, root)
}

/// Among the most visited root actions, the higher value sum (equal visits,
/// so the higher mean), then a draw from `seed`. An all-rejected search keeps
/// its public-seeded fallback pick.
fn heuristic_search_pick_v1(fallback: u32, stats: &PimcStatsV1, seed: u64) -> u32 {
    if stats.visits.iter().all(|&visits| visits == 0) {
        return fallback;
    }
    let most = stats.visits.iter().copied().max().unwrap_or(0);
    let candidates: Vec<usize> = (0..stats.visits.len())
        .filter(|&action| stats.visits[action] == most)
        .collect();
    let best_value = candidates
        .iter()
        .map(|&action| stats.value_sum[action])
        .fold(f64::NEG_INFINITY, f64::max);
    let ties: Vec<usize> = candidates
        .into_iter()
        .filter(|&action| stats.value_sum[action] == best_value)
        .collect();
    seeded_tie_v1(&ties, HEURISTIC_SEARCH_TIE_TAG_V1, seed)
}

/// `ties[0]` for a single candidate, else a uniform draw from `domain` and
/// `seed`.
fn seeded_tie_v1(ties: &[usize], domain: &[u8], seed: u64) -> u32 {
    let index = if ties.len() <= 1 {
        0
    } else {
        let mut rng = SplitMix64::seed(derive_seed(domain, &[seed]));
        draw(&mut rng, ties.len())
    };
    ties.get(index).copied().unwrap_or(0) as u32
}

/// `pimc:sims=N:depth=D:closedlist-full-pool-heuristic-v1`: the fair
/// full-pool search with heuristic leaves and a seeded final tie-break.
fn pimc_choose_full_pool_heuristic_v1(
    session: &KaitoSession,
    decision: &KaitoDecision,
    sims: u32,
    depth: u32,
    max_turns: u32,
    belief: &LimitedBeliefV1,
    seed: u64,
) -> Result<(u32, PimcStatsV1), String> {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        return Err("full-pool search has no current decision".into());
    };
    if sims == 0 {
        return Err("full-pool search needs a positive simulation count".into());
    }
    let legal = legal_count(decision)?;
    let (fallback, stats) = pimc_choose_with_v1(
        legal,
        sims,
        seed,
        true,
        |world_seed| {
            session
                .sampled_full_pool_closed_list_v1(decision, world_seed, belief)
                .map_err(|error| error.to_string())
        },
        |sampled, action, rng| {
            rollout_heuristic_v1(
                sampled,
                seat(expected.acting_player),
                action,
                depth,
                max_turns,
                rng,
            )
        },
    );
    if legal <= 1 {
        return Ok((fallback, stats));
    }
    let pick = heuristic_search_pick_v1(fallback, &stats, seed);
    Ok((pick, stats))
}

/// `sampled-public-heuristic/v1`: one world drawn from the public belief
/// corpus for the whole decision; every legal root action is applied once to
/// that world (restored between actions) and the resulting state is scored
/// for the actor, a natural terminal as `1`, `0` or `-1` and anything else by
/// [`heuristic_leaf_value_v1`]. The highest score wins; ties are drawn from
/// `seed`. A rejected world, or a world in which every action failed, falls
/// back to a uniform pick seeded from `seed` alone.
fn sampled_public_heuristic_v1(
    session: &KaitoSession,
    decision: &KaitoDecision,
    belief: &LimitedBeliefV1,
    seed: u64,
) -> Result<(u32, PimcStatsV1), String> {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        return Err("full-pool heuristic has no current decision".into());
    };
    let legal = legal_count(decision)?;
    let n = legal as usize;
    let mut stats = PimcStatsV1 {
        visits: vec![0; n],
        value_sum: vec![0.0; n],
        ..Default::default()
    };
    if legal <= 1 {
        return Ok((0, stats));
    }
    let fallback = || {
        let mut rng = SplitMix64::seed(derive_seed(HEURISTIC_FALLBACK_TAG_V1, &[seed]));
        random_index(&mut rng, legal)
    };
    let world_seed = derive_seed(HEURISTIC_WORLD_TAG_V1, &[seed]);
    let mut world = match session.sampled_full_pool_closed_list_v1(decision, world_seed, belief) {
        Ok(world) => world,
        Err(error) => {
            stats.rejected = 1;
            stats.rejection_reasons.insert(error.to_string(), 1);
            return Ok((fallback(), stats));
        }
    };
    let root = seat(expected.acting_player);
    let snapshot = world.snapshot();
    let mut scores = Vec::with_capacity(n);
    for action in 0..legal {
        if action > 0 {
            world
                .restore(&snapshot)
                .map_err(|error| format!("heuristic world restore: {error}"))?;
        }
        let score = encode_current(&world).and_then(|root_decision| {
            match world
                .apply(&root_decision, action)
                .map_err(|error| error.to_string())?
            {
                FastActorResponseV1::Terminal(terminal) => {
                    match natural_value_v1(terminal, root)? {
                        Some(value) => Ok(value),
                        None => heuristic_leaf_value_v1(&world, root),
                    }
                }
                FastActorResponseV1::Decision(_) => heuristic_leaf_value_v1(&world, root),
            }
        });
        match score {
            Ok(value) => {
                stats.visits[action as usize] = 1;
                stats.value_sum[action as usize] = value;
                scores.push(Some(value));
            }
            Err(error) => {
                stats.rollout_errors += 1;
                stats.first_rollout_error.get_or_insert(error);
                scores.push(None);
            }
        }
    }
    let Some(best) = scores.iter().flatten().copied().reduce(f64::max) else {
        return Ok((fallback(), stats));
    };
    let ties: Vec<usize> = (0..n)
        .filter(|&action| scores[action] == Some(best))
        .collect();
    Ok((seeded_tie_v1(&ties, HEURISTIC_TIE_TAG_V1, seed), stats))
}

/// Adds one searched decision's counts to the game result.
fn record_search_v1(result: &mut GameResultV1, legal: u32, attempts: u32, stats: PimcStatsV1) {
    result.rejected_worlds += u64::from(stats.rejected);
    result.rollout_errors += u64::from(stats.rollout_errors);
    if result.first_rollout_error.is_none() {
        result.first_rollout_error = stats.first_rollout_error;
    }
    if legal > 1 {
        result.searched_decisions += 1;
        result.search_fallbacks += u64::from(stats.rejected == attempts);
        for (reason, count) in stats.rejection_reasons {
            *result.rejection_reasons.entry(reason).or_default() += u64::from(count);
        }
    }
}

pub(super) fn play_game(
    decks: [&[u16]; 2],
    policies: [&PolicySpecV1; 2],
    beliefs: [Option<&LimitedBeliefV1>; 2],
    env_seed: u64,
    starting_player: PlayerId,
    limits: GameLimitsV1,
    stream: Option<MatchStreamV1>,
) -> GameResultV1 {
    let mut result = GameResultV1::empty();
    let run = || {
        for (policy, belief) in policies.into_iter().zip(beliefs) {
            policy.validate_full_pool_session_v1(SessionKindV1::FullPoolV1)?;
            if policy.needs_belief() {
                full_pool_belief_v1(
                    belief.ok_or("closedlist-full-pool-v1 needs a belief corpus")?,
                )?;
            }
        }
        let deck_ids = decks.map(|deck| {
            let mut hash = Sha256::new();
            hash.update(b"kernel_limited_full_pool_deck/v1");
            hash.update(KERNEL_CARDDB_HASH.to_le_bytes());
            for card in deck {
                hash.update(card.to_le_bytes());
            }
            format!("limited-full-pool-v1:{:x}", hash.finalize())
        });
        let reset = ResetV1 {
            expected_card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: env_seed,
            env_seed,
            starting_player: starting_player.into(),
            deck_ids,
            mainboards: decks.map(<[u16]>::to_vec),
            max_physical_decisions: limits.max_decisions.saturating_mul(4).max(1),
            max_policy_steps: limits.max_decisions.saturating_mul(512).max(1),
        };
        reset_numeric_reveal_facts_v3(&reset)
    };
    result.outcome = match run() {
        Ok(mut session) => play_owner_in_pair(
            &mut session,
            policies,
            beliefs,
            env_seed,
            limits,
            stream,
            &mut result,
        )
        .unwrap_or_else(GameOutcomeV1::Error),
        Err(error) => GameOutcomeV1::Error(error),
    };
    result
}

#[cfg(all(test, not(feature = "standard-magezero-fixtures")))]
fn play_owner(
    mut session: KaitoSession,
    policies: [&PolicySpecV1; 2],
    beliefs: [Option<&LimitedBeliefV1>; 2],
    seed: u64,
    limits: GameLimitsV1,
    result: &mut GameResultV1,
) -> Result<GameOutcomeV1, String> {
    play_owner_in_pair(&mut session, policies, beliefs, seed, limits, None, result)
}

/// Both seats share one policy generator seeded from the environment seed,
/// as before the learner policy existed; a `learner-checkpoint` seat instead
/// draws from its own stream (the research lead's ruling 5), so its samples
/// do not depend on the opponent's draws and the shared generator advances
/// exactly as it would without it.
fn play_owner_in_pair(
    session: &mut KaitoSession,
    policies: [&PolicySpecV1; 2],
    beliefs: [Option<&LimitedBeliefV1>; 2],
    seed: u64,
    limits: GameLimitsV1,
    stream: Option<MatchStreamV1>,
    result: &mut GameResultV1,
) -> Result<GameOutcomeV1, String> {
    let mut rng = SplitMix64::seed(derive_seed(
        b"mtg-kernel/limited-full-pool-match-policy/v1\0",
        &[seed],
    ));
    let mut learner_rngs: [Option<SplitMix64>; 2] = [None, None];
    for (index, policy) in policies.iter().enumerate() {
        if matches!(policy, PolicySpecV1::LearnerCheckpoint(_)) {
            let stream = stream.ok_or("learner-checkpoint needs the game's pair stream")?;
            let stream_seed = learner_policy_v1::learner_stream_seed_v1(stream, index as u64);
            learner_rngs[index] = Some(SplitMix64::seed(stream_seed));
        }
    }
    loop {
        let expected = match session.current_response() {
            FastActorResponseV1::Terminal(terminal) => return Ok(terminal_outcome(terminal)),
            FastActorResponseV1::Decision(expected) => expected,
        };
        let decision = session
            .encode(expected)
            .map_err(|error| error.to_string())?;
        result.turns = decision.public_turn();
        if result.turns > limits.max_turns || result.decisions >= limits.max_decisions {
            return Ok(GameOutcomeV1::Stall);
        }
        let actor = seat(expected.acting_player);
        let legal = legal_count(&decision)?;
        let selected = match policies[actor.index()] {
            PolicySpecV1::Random => random_index(&mut rng, legal),
            PolicySpecV1::Pimc {
                sims,
                depth,
                world: WorldModeV1::FullPoolClosedListHeuristicV1,
            } => {
                let belief =
                    beliefs[actor.index()].ok_or("full-pool match lacks a belief corpus")?;
                let (selected, stats) = pimc_choose_full_pool_heuristic_v1(
                    session,
                    &decision,
                    *sims,
                    *depth,
                    limits.max_turns,
                    belief,
                    rng.next_u64(),
                )?;
                record_search_v1(result, legal, *sims, stats);
                selected
            }
            PolicySpecV1::SampledPublicHeuristicV1 => {
                let belief =
                    beliefs[actor.index()].ok_or("full-pool match lacks a belief corpus")?;
                let (selected, stats) =
                    sampled_public_heuristic_v1(session, &decision, belief, rng.next_u64())?;
                record_search_v1(result, legal, 1, stats);
                selected
            }
            PolicySpecV1::LearnerCheckpoint(checkpoint) => {
                let stream = learner_rngs[actor.index()]
                    .as_mut()
                    .ok_or("learner-checkpoint seat has no stream")?;
                if legal <= 1 {
                    0
                } else {
                    let input = session
                        .training_input_v1(&decision)
                        .map_err(|error| error.to_string())?;
                    let selected = checkpoint.sample_v1(&input, stream.next_u64())?;
                    u32::try_from(selected).map_err(|_| "learner selection exceeds u32")?
                }
            }
            PolicySpecV1::Pimc { sims, depth, .. } => {
                let belief =
                    beliefs[actor.index()].ok_or("full-pool match lacks a belief corpus")?;
                let (selected, stats) = pimc_choose_full_pool_v1(
                    session,
                    &decision,
                    *sims,
                    *depth,
                    limits.max_turns,
                    belief,
                    rng.next_u64(),
                )?;
                result.rejected_worlds += u64::from(stats.rejected);
                result.rollout_errors += u64::from(stats.rollout_errors);
                if result.first_rollout_error.is_none() {
                    result.first_rollout_error = stats.first_rollout_error;
                }
                if legal > 1 {
                    result.searched_decisions += 1;
                    result.search_fallbacks += u64::from(stats.rejected == *sims);
                    for (reason, count) in stats.rejection_reasons {
                        *result.rejection_reasons.entry(reason).or_default() += u64::from(count);
                    }
                }
                selected
            }
        };
        if trace_enabled_v1() {
            eprintln!(
                "full-pool trace turn {} decision {} action {selected}/{legal}",
                result.turns, result.decisions,
            );
        }
        session
            .apply(&decision, selected)
            .map_err(|error| error.to_string())?;
        result.decisions += 1;
    }
}

#[cfg(all(test, not(feature = "standard-magezero-fixtures")))]
#[path = "full_pool_match_v1_tests.rs"]
mod tests;
