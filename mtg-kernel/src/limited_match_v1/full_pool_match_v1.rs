//! Explicit terminal-search adapter for the sealed Foundations numeric owner.
use super::*;
use crate::{
    current_mixed_policy_v1::{KaitoDecision, KaitoSession},
    limited_full_pool_consumer_v1::{reset_numeric_reveal_facts_v3, ResetV1},
    rl_session::RlSessionTerminalV1,
};

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

pub(super) fn play_game(
    decks: [&[u16]; 2],
    policies: [&PolicySpecV1; 2],
    beliefs: [Option<&LimitedBeliefV1>; 2],
    env_seed: u64,
    starting_player: PlayerId,
    limits: GameLimitsV1,
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
        Ok(session) => play_owner(session, policies, beliefs, env_seed, limits, &mut result)
            .unwrap_or_else(GameOutcomeV1::Error),
        Err(error) => GameOutcomeV1::Error(error),
    };
    result
}

fn play_owner(
    mut session: KaitoSession,
    policies: [&PolicySpecV1; 2],
    beliefs: [Option<&LimitedBeliefV1>; 2],
    seed: u64,
    limits: GameLimitsV1,
    result: &mut GameResultV1,
) -> Result<GameOutcomeV1, String> {
    let mut rng = SplitMix64::seed(derive_seed(
        b"mtg-kernel/limited-full-pool-match-policy/v1\0",
        &[seed],
    ));
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
            PolicySpecV1::Pimc { sims, depth, .. } => {
                let belief =
                    beliefs[actor.index()].ok_or("full-pool match lacks a belief corpus")?;
                let (selected, stats) = pimc_choose_full_pool_v1(
                    &session,
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
