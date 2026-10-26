//! DraftZero-shaped Limited matches with fair search (issue #110, milestones 6
//! and 7).
//!
//! Games run on the Prepared V57 session, which accepts any 40-card or larger
//! mainboard made only of Full cards. A policy picks one legal action index
//! at a time:
//!
//! - `random`: uniform over the legal actions.
//! - `pimc:sims=N:depth=D:<world>`: root-level determinized search. Every
//!   simulation draws one world of the acting player's information set,
//!   applies a root action chosen by UCB1, plays uniform random decisions for
//!   at most `D` steps and scores the result. `<world>` says how the world is
//!   drawn:
//!   - `closedlist` (fair): the opponent's unseen hand and library are drawn
//!     from a public belief corpus of decks, conditioned on the opponent's
//!     cards the actor can see. The true hidden composition is never read.
//!   - `truelist`: the opponent's true unseen cards are shuffled among their
//!     slots. This leaks the opponent's decklist, like the older Pauper
//!     searches; it exists for comparison.
//!   - `peek`: no redetermination at all (sees every hidden card).
//!
//! In every mode the actor's own library order is reshuffled, since the
//! actor knows its own decklist but not the order.

use crate::card_def::{card_id_by_name, CARD_DEFS, KERNEL_CARDDB_HASH};
use crate::ids::{ObjectId, PlayerId};
use crate::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use crate::rl::{PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::state::{GameState, ObjectStateV4, SplitMix64, Zone};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const LIMITED_MATCH_SCHEMA_V1: &str = "mtg-kernel-limited-match/v1";
/// UCB1 exploration constant on values in [-1, 1].
const PIMC_EXPLORATION_V1: f64 = 1.4;
/// A truncated rollout's heuristic score is scaled below a natural result.
const PIMC_HEURISTIC_SCALE_V1: f64 = 0.5;

// ---------------------------------------------------------------------------
// decks

/// A DraftZero/gorge deck file: `{name, cards: [{name, count}]}`. Other fields
/// (`format`, `notes`) are ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct DeckFileV1 {
    #[serde(default)]
    pub name: Option<String>,
    pub cards: Vec<CustomCardCountV1>,
}

impl DeckFileV1 {
    /// Full-only mainboard in row and copy order (the Limited reset rule).
    pub fn resolve(&self) -> Result<Vec<u16>, String> {
        CustomDeckV1 {
            cards: self.cards.clone(),
        }
        .resolve()
    }

    /// Names this engine cannot play: `(name, reason)`, in row order.
    pub fn unsupported(&self) -> Vec<(String, &'static str)> {
        self.cards
            .iter()
            .filter_map(|row| match card_id_by_name(&row.name) {
                None => Some((row.name.clone(), "unknown")),
                Some(id) => {
                    let card = &CARD_DEFS[id as usize];
                    (card.is_token || !card.has_full_support())
                        .then(|| (row.name.clone(), "not full"))
                }
            })
            .collect()
    }
}

/// The public deck corpus a `closedlist` seat samples the opponent from.
#[derive(Debug, Clone, Default)]
pub struct LimitedBeliefV1 {
    /// Sorted card-definition multiset per deck.
    decks: Vec<Vec<u16>>,
}

impl LimitedBeliefV1 {
    pub fn new(decks: impl IntoIterator<Item = Vec<u16>>) -> Self {
        Self {
            decks: decks
                .into_iter()
                .map(|mut cards| {
                    cards.sort_unstable();
                    cards
                })
                .collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.decks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.decks.is_empty()
    }
}

// ---------------------------------------------------------------------------
// worlds

/// Hand and library objects of `owner` whose identity `actor` does not know,
/// in the sampler's canonical order (hand, then library), minus `pinned`.
/// The actor knows its own hand.
pub(crate) fn unknown_slots_v1(
    state: &GameState,
    actor: PlayerId,
    owner: PlayerId,
    pinned: &[ObjectId],
) -> Vec<ObjectId> {
    let player = &state.players[owner.index()];
    let mut slots = Vec::new();
    if owner != actor {
        for &id in &player.hand {
            let generation = state.objects.get(id).zone_change_count;
            let known = state
                .known_hand_cards(actor, owner)
                .iter()
                .any(|k| k.object == id && k.zone_change_count == generation);
            if !known && !pinned.contains(&id) {
                slots.push(id);
            }
        }
    }
    for (position, &id) in player.library.iter().enumerate() {
        let generation = state.objects.get(id).zone_change_count;
        let known = state.known_library_cards(actor, owner).iter().any(|k| {
            usize::try_from(k.position).ok() == Some(position)
                && k.object == id
                && k.zone_change_count == generation
        });
        if !known && !pinned.contains(&id) {
            slots.push(id);
        }
    }
    slots
}

/// The opponent's physical cards outside `unknown` (public zones and cards the
/// actor knows in hand or library), sorted. Tokens and spell copies are not
/// deck cards.
fn observed_cards_v1(state: &GameState, owner: PlayerId, unknown: &HashSet<ObjectId>) -> Vec<u16> {
    let mut observed: Vec<u16> = state
        .objects
        .iter()
        .filter(|(id, o)| {
            o.owner == owner
                && !o.v4.is_token
                && o.spell_copy_origin.is_none()
                && o.v4.face_down_v1.is_none()
                && !unknown.contains(id)
                && matches!(
                    o.zone,
                    Zone::Hand
                        | Zone::Library
                        | Zone::Battlefield
                        | Zone::Graveyard
                        | Zone::Exile
                        | Zone::Stack
                        | Zone::Command
                )
        })
        .map(|(_, o)| o.card_def)
        .collect();
    observed.sort_unstable();
    observed
}

/// Both sorted.
fn is_submultiset(small: &[u16], big: &[u16]) -> bool {
    let mut j = 0;
    for &x in small {
        while j < big.len() && big[j] < x {
            j += 1;
        }
        if j == big.len() || big[j] != x {
            return false;
        }
        j += 1;
    }
    true
}

/// `big` minus the multiset intersection with `small` (both sorted), and the
/// intersection's size.
fn remainder(big: &[u16], small: &[u16]) -> (Vec<u16>, usize) {
    let (mut out, mut shared, mut j) = (Vec::new(), 0, 0);
    for &x in big {
        while j < small.len() && small[j] < x {
            j += 1;
        }
        if j < small.len() && small[j] == x {
            shared += 1;
            j += 1;
        } else {
            out.push(x);
        }
    }
    (out, shared)
}

/// Unbiased draw in [0, bound).
fn draw(rng: &mut SplitMix64, bound: usize) -> usize {
    let bound = bound as u64;
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let value = rng.next_u64();
        if value >= threshold {
            return (value % bound) as usize;
        }
    }
}

fn derive_seed(domain: &[u8], parts: &[u64]) -> u64 {
    let mut hash = Sha256::new();
    hash.update(domain);
    for part in parts {
        hash.update(part.to_le_bytes());
    }
    u64::from_le_bytes(hash.finalize()[..8].try_into().expect("eight digest bytes"))
}

/// How a belief draw went (diagnostics only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeliefDrawV1 {
    /// A corpus deck of the right size containing every observed card.
    Consistent,
    /// No such deck: a deck sharing the most observed cards supplied the rest.
    NearestDeck,
}

/// Rewrite the opponent's unknown slots with a draw from `belief`. Reads only
/// public facts: the slot count, the observed cards and the corpus.
pub(crate) fn apply_belief_v1(
    state: &mut GameState,
    actor: PlayerId,
    pinned: &[ObjectId],
    belief: &LimitedBeliefV1,
    seed: u64,
) -> Result<BeliefDrawV1, String> {
    if belief.is_empty() {
        return Err("empty belief corpus".into());
    }
    let opponent = actor.opponent();
    let slots = unknown_slots_v1(state, actor, opponent, pinned);
    if slots.is_empty() {
        return Ok(BeliefDrawV1::Consistent);
    }
    let unknown: HashSet<ObjectId> = slots.iter().copied().collect();
    let observed = observed_cards_v1(state, opponent, &unknown);
    // Face-down cards outside hand and library are deck cards the actor cannot
    // identify: they count toward the deck size but constrain nothing.
    let face_down = state
        .objects
        .iter()
        .filter(|(id, o)| {
            o.owner == opponent
                && !o.v4.is_token
                && o.spell_copy_origin.is_none()
                && o.v4.face_down_v1.is_some()
                && !unknown.contains(id)
        })
        .count();
    let total = observed.len() + slots.len() + face_down;
    let mut rng = SplitMix64::seed(derive_seed(b"mtg-kernel/limited-belief/v1\0", &[seed]));
    let consistent: Vec<usize> = (0..belief.decks.len())
        .filter(|&d| belief.decks[d].len() == total && is_submultiset(&observed, &belief.decks[d]))
        .collect();
    let (deck, kind) = if consistent.is_empty() {
        let shared: Vec<usize> = belief
            .decks
            .iter()
            .map(|deck| remainder(deck, &observed).1)
            .collect();
        let best = *shared.iter().max().expect("nonempty corpus");
        let nearest: Vec<usize> = (0..shared.len()).filter(|&d| shared[d] == best).collect();
        (
            nearest[draw(&mut rng, nearest.len())],
            BeliefDrawV1::NearestDeck,
        )
    } else {
        (
            consistent[draw(&mut rng, consistent.len())],
            BeliefDrawV1::Consistent,
        )
    };
    let (mut rest, _) = remainder(&belief.decks[deck], &observed);
    if rest.is_empty() {
        return Err("belief deck has no cards left for the unknown slots".into());
    }
    for i in (1..rest.len()).rev() {
        let j = draw(&mut rng, i + 1);
        rest.swap(i, j);
    }
    // A nearest deck can be smaller than the remaining slots: cycle it.
    for (k, &id) in slots.iter().enumerate() {
        let def = rest[k % rest.len()];
        let object = state.objects.get_mut(id);
        object.card_def = def;
        object.name = CARD_DEFS[def as usize].name.to_string();
        object.v4 = ObjectStateV4::from_card_def(def);
    }
    Ok(kind)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldModeV1 {
    ClosedList,
    TrueList,
    Peek,
}

/// One world of the current actor's information set (see the module docs).
/// `Err` is a declared rejection: the caller skips that simulation.
pub fn sample_world_v1(
    session: &FastActorSessionV1,
    mode: WorldModeV1,
    belief: Option<&LimitedBeliefV1>,
    seed: u64,
) -> Result<FastActorSessionV1, String> {
    match mode {
        WorldModeV1::Peek => Ok(session.clone()),
        WorldModeV1::TrueList => session.limited_belief_clone_v1(seed, |_, _, _| Ok(())),
        WorldModeV1::ClosedList => {
            let belief = belief.ok_or("closedlist needs a belief corpus")?;
            session.limited_belief_clone_v1(seed, |state, actor, pinned| {
                apply_belief_v1(state, actor, pinned, belief, seed).map(|_| ())
            })
        }
    }
}

// ---------------------------------------------------------------------------
// policies

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicySpecV1 {
    Random,
    Pimc {
        sims: u32,
        depth: u32,
        world: WorldModeV1,
    },
}

impl PolicySpecV1 {
    pub fn parse(spec: &str) -> Result<Self, String> {
        let mut parts = spec.split(':');
        match parts.next() {
            Some("random") if parts.next().is_none() => Ok(Self::Random),
            Some("pimc") => {
                let (mut sims, mut depth, mut world) = (64u32, 64u32, None);
                for part in parts {
                    let set_world = |w: WorldModeV1, world: &mut Option<WorldModeV1>| {
                        if world.replace(w).is_some() {
                            Err(format!("{spec:?}: more than one world mode"))
                        } else {
                            Ok(())
                        }
                    };
                    match part.split_once('=') {
                        Some(("sims", v)) => {
                            sims = v.parse().map_err(|_| format!("bad sims {v:?}"))?
                        }
                        Some(("depth", v)) => {
                            depth = v.parse().map_err(|_| format!("bad depth {v:?}"))?
                        }
                        None if part == "closedlist" => {
                            set_world(WorldModeV1::ClosedList, &mut world)?
                        }
                        None if part == "truelist" => set_world(WorldModeV1::TrueList, &mut world)?,
                        None if part == "peek" => set_world(WorldModeV1::Peek, &mut world)?,
                        _ => return Err(format!("{spec:?}: unknown field {part:?}")),
                    }
                }
                if sims == 0 {
                    return Err(format!("{spec:?}: sims must be positive"));
                }
                Ok(Self::Pimc {
                    sims,
                    depth,
                    world: world.ok_or_else(|| {
                        format!("{spec:?}: name a world mode (closedlist, truelist or peek)")
                    })?,
                })
            }
            _ => Err(format!("unknown policy {spec:?}")),
        }
    }

    pub fn needs_belief(&self) -> bool {
        matches!(
            self,
            Self::Pimc {
                world: WorldModeV1::ClosedList,
                ..
            }
        )
    }
}

/// Per-decision search statistics (also the milestone 6 fairness witness).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PimcStatsV1 {
    pub visits: Vec<u32>,
    pub value_sum: Vec<f64>,
    pub rejected: u32,
    pub rollout_errors: u32,
    pub first_rollout_error: Option<String>,
}

fn seat(player: PlayerSeatV1) -> PlayerId {
    match player {
        PlayerSeatV1::P0 => PlayerId::P0,
        PlayerSeatV1::P1 => PlayerId::P1,
    }
}

/// `trusted` skips the pre-step contract re-check (see
/// `limited_trusted_step_v1`); a world edited since its last advance takes the
/// full step.
fn step_index(session: &mut FastActorSessionV1, index: u32, trusted: bool) -> Result<(), String> {
    match session.current_response() {
        FastActorResponseV1::Decision(d) => if trusted {
            session.limited_trusted_step_v1(d.episode_id, d.step, index)
        } else {
            session.step(d.episode_id, d.step, index)
        }
        .map(|_| ())
        .map_err(|e| e.message),
        FastActorResponseV1::Terminal(_) => Err("step after terminal".into()),
    }
}

/// Value in [-1, 1] for `root` of a finished or cut-off rollout. A halted
/// rollout (an unsupported runtime mechanic) is an error, not a score.
fn rollout_value(session: &FastActorSessionV1, root: PlayerId) -> Result<f64, String> {
    use crate::kernel_native_search_opponent_v1::evaluate_state_v1;
    Ok(match session.current_response() {
        FastActorResponseV1::Terminal(t) => match t.terminal_classification {
            TerminalClassificationV1::Natural => match (t.terminal_outcome, root) {
                (TerminalOutcomeV1::P0Win, PlayerId::P0)
                | (TerminalOutcomeV1::P1Win, PlayerId::P1) => 1.0,
                (TerminalOutcomeV1::P0Win | TerminalOutcomeV1::P1Win, _) => -1.0,
                _ => 0.0,
            },
            TerminalClassificationV1::Truncated => {
                PIMC_HEURISTIC_SCALE_V1 * f64::from(evaluate_state_v1(session.game_state(), root))
                    / 10_000.0
            }
            TerminalClassificationV1::Halted => {
                return Err(format!("rollout halted: {}", t.terminal_reason))
            }
        },
        FastActorResponseV1::Decision(_) => {
            PIMC_HEURISTIC_SCALE_V1 * f64::from(evaluate_state_v1(session.game_state(), root))
                / 10_000.0
        }
    })
}

/// A decision always offers at least one action; index 0 for a malformed empty menu.
fn random_index(rng: &mut SplitMix64, legal: u32) -> u32 {
    if legal <= 1 {
        return 0;
    }
    draw(rng, legal as usize) as u32
}

/// Root-level determinized search: the action index and its statistics.
pub fn pimc_choose_v1(
    session: &FastActorSessionV1,
    sims: u32,
    depth: u32,
    world: WorldModeV1,
    belief: Option<&LimitedBeliefV1>,
    seed: u64,
) -> Result<(u32, PimcStatsV1), String> {
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        return Err("no current decision".into());
    };
    let legal = decision.legal_action_count;
    let n = legal as usize;
    let mut stats = PimcStatsV1 {
        visits: vec![0; n],
        value_sum: vec![0.0; n],
        ..Default::default()
    };
    if legal <= 1 {
        return Ok((0, stats));
    }
    let root = seat(decision.acting_player);
    for sim in 0..sims {
        let total: u32 = stats.visits.iter().sum();
        let action = match stats.visits.iter().position(|&v| v == 0) {
            Some(unvisited) => unvisited,
            None => {
                let log_total = f64::from(total).ln();
                let score = |a: usize| {
                    let v = f64::from(stats.visits[a]);
                    stats.value_sum[a] / v + PIMC_EXPLORATION_V1 * (log_total / v).sqrt()
                };
                // First maximum: ties go to the lowest index.
                (1..n).fold(0, |best, a| if score(a) > score(best) { a } else { best })
            }
        };
        let world_seed = derive_seed(
            b"mtg-kernel/limited-pimc-world/v1\0",
            &[seed, u64::from(sim)],
        );
        let mut world_session = match sample_world_v1(session, world, belief, world_seed) {
            Ok(w) => w,
            Err(_) => {
                stats.rejected += 1;
                continue;
            }
        };
        let mut rng = SplitMix64::seed(derive_seed(
            b"mtg-kernel/limited-pimc-rollout/v1\0",
            &[world_seed],
        ));
        let value = (|| {
            // The world was redeterminized: its root step revalidates.
            step_index(&mut world_session, action as u32, false)?;
            for _ in 0..depth {
                let FastActorResponseV1::Decision(d) = world_session.current_response() else {
                    break;
                };
                let pick = random_index(&mut rng, d.legal_action_count);
                step_index(&mut world_session, pick, true)?;
            }
            rollout_value(&world_session, root)
        })();
        let value = value.unwrap_or_else(|error| {
            stats.rollout_errors += 1;
            stats.first_rollout_error.get_or_insert(error);
            0.0
        });
        stats.visits[action] += 1;
        stats.value_sum[action] += value;
    }
    if stats.visits.iter().all(|&v| v == 0) {
        // Every world was rejected: fall back to a public-seeded uniform pick.
        let mut rng = SplitMix64::seed(derive_seed(
            b"mtg-kernel/limited-pimc-fallback/v1\0",
            &[seed],
        ));
        return Ok((random_index(&mut rng, legal), stats));
    }
    let best = (1..n).fold(0, |best, a| {
        if stats.visits[a] > stats.visits[best] {
            a
        } else {
            best
        }
    });
    Ok((best as u32, stats))
}

// ---------------------------------------------------------------------------
// games

#[derive(Debug, Clone, Copy)]
pub struct GameLimitsV1 {
    pub max_turns: u32,
    pub max_decisions: u64,
    /// Re-check the session contract before every game step (slower; the
    /// trusted step plays identically, see the equivalence test).
    pub full_validation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameOutcomeV1 {
    Winner(PlayerId),
    Draw,
    /// Turn or decision cap, or the session's own truncation.
    Stall,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct GameResultV1 {
    pub outcome: GameOutcomeV1,
    pub turns: u32,
    pub decisions: u64,
    pub rejected_worlds: u64,
    /// Search simulations whose root step or rollout the engine refused or
    /// halted. They score 0, so a nonzero count weakens the search.
    pub rollout_errors: u64,
    pub first_rollout_error: Option<String>,
}

/// One game: `decks[seat]`, `policies[seat]`, `beliefs[seat]` (the corpus a
/// `closedlist` seat samples its opponent from).
pub fn play_game_v1(
    decks: [&[u16]; 2],
    policies: [&PolicySpecV1; 2],
    beliefs: [Option<&LimitedBeliefV1>; 2],
    env_seed: u64,
    starting_player: PlayerId,
    limits: GameLimitsV1,
) -> GameResultV1 {
    let mut result = GameResultV1 {
        outcome: GameOutcomeV1::Stall,
        turns: 0,
        decisions: 0,
        rejected_worlds: 0,
        rollout_errors: 0,
        first_rollout_error: None,
    };
    let identities = decks.map(|deck| {
        let mut hash = Sha256::new();
        hash.update(b"kernel_limited_common_deck/v1");
        hash.update(KERNEL_CARDDB_HASH.to_le_bytes());
        for card in deck {
            hash.update(card.to_le_bytes());
        }
        format!("limited-common-v1:{:x}", hash.finalize())
    });
    // Physical-decision and policy-step limits well above the decision cap,
    // so the cap below (a stall) is what ends long games.
    let session_limits = (
        limits.max_decisions.saturating_mul(4).max(1),
        limits.max_decisions.saturating_mul(512).max(1),
    );
    let mut session = match FastActorSessionV1::reset_prepared_v57(
        env_seed,
        env_seed,
        session_limits,
        identities,
        [decks[0].to_vec(), decks[1].to_vec()],
        starting_player,
    ) {
        Ok(session) => session,
        Err(error) => {
            result.outcome = GameOutcomeV1::Error(error.message);
            return result;
        }
    };
    let mut rng = SplitMix64::seed(derive_seed(
        b"mtg-kernel/limited-match-policy/v1\0",
        &[env_seed],
    ));
    loop {
        result.turns = session.game_state().turn;
        let decision = match session.current_response() {
            FastActorResponseV1::Terminal(t) => {
                result.outcome = match (t.terminal_classification, t.terminal_outcome) {
                    (TerminalClassificationV1::Natural, TerminalOutcomeV1::P0Win) => {
                        GameOutcomeV1::Winner(PlayerId::P0)
                    }
                    (TerminalClassificationV1::Natural, TerminalOutcomeV1::P1Win) => {
                        GameOutcomeV1::Winner(PlayerId::P1)
                    }
                    (TerminalClassificationV1::Natural, _) => GameOutcomeV1::Draw,
                    (TerminalClassificationV1::Truncated, _) => GameOutcomeV1::Stall,
                    (TerminalClassificationV1::Halted, _) => {
                        GameOutcomeV1::Error(t.terminal_reason)
                    }
                };
                return result;
            }
            FastActorResponseV1::Decision(d) => d,
        };
        if result.turns > limits.max_turns || result.decisions >= limits.max_decisions {
            result.outcome = GameOutcomeV1::Stall;
            return result;
        }
        let actor = seat(decision.acting_player);
        let index = match policies[actor.index()] {
            PolicySpecV1::Random => random_index(&mut rng, decision.legal_action_count),
            PolicySpecV1::Pimc { sims, depth, world } => {
                let seed = rng.next_u64();
                match pimc_choose_v1(
                    &session,
                    *sims,
                    *depth,
                    *world,
                    beliefs[actor.index()],
                    seed,
                ) {
                    Ok((index, stats)) => {
                        result.rejected_worlds += u64::from(stats.rejected);
                        result.rollout_errors += u64::from(stats.rollout_errors);
                        if result.first_rollout_error.is_none() {
                            result.first_rollout_error = stats.first_rollout_error;
                        }
                        index
                    }
                    Err(error) => {
                        result.outcome = GameOutcomeV1::Error(error);
                        return result;
                    }
                }
            }
        };
        let stepped = if limits.full_validation {
            session.step(decision.episode_id, decision.step, index)
        } else {
            session.limited_trusted_step_v1(decision.episode_id, decision.step, index)
        };
        if let Err(error) = stepped {
            let mut message = error.message;
            let strays = roster_strays_v1(session.game_state());
            if !strays.is_empty() {
                message = format!("{message} [zone without roster: {}]", strays.join("; "));
            }
            // A refused step leaves the session unchanged, so the menu is the one acted on.
            if let Some(action) = session
                .diagnostic_current_action_semantics()
                .and_then(|menu| menu.get(index as usize).cloned())
            {
                let action = format!("{action:?}");
                message = format!(
                    "{message} [turn {} decision {} action {index}: {}]",
                    result.turns,
                    result.decisions,
                    action.chars().take(400).collect::<String>()
                );
            }
            result.outcome = GameOutcomeV1::Error(message);
            return result;
        }
        result.decisions += 1;
    }
}

/// Diagnostics for an engine refusal: battlefield and graveyard objects that
/// no player's zone list holds.
pub fn roster_strays_v1(state: &GameState) -> Vec<String> {
    state
        .objects
        .iter()
        .filter(|(id, o)| {
            let players = &state.players;
            match o.zone {
                Zone::Battlefield => !players.iter().any(|p| p.battlefield.contains(id)),
                Zone::Graveyard => !players.iter().any(|p| p.graveyard.contains(id)),
                _ => false,
            }
        })
        .map(|(id, o)| {
            format!(
                "{} {:?} {:?} owner {} controller {} token {} turn {}",
                o.name,
                id,
                o.zone,
                o.owner.index(),
                o.controller.index(),
                o.v4.is_token,
                state.turn
            )
        })
        .collect()
}

/// The pair's deal: two distinct deck indices into `count` decks, the shared
/// environment seed of both legs and the starting seat.
pub fn deal_pair_v1(
    seed: u64,
    pair: u64,
    count: usize,
) -> Result<([usize; 2], u64, PlayerId), String> {
    if count < 2 {
        return Err(format!("{count} playable decks: a pair needs two"));
    }
    let mut rng = SplitMix64::seed(derive_seed(
        b"mtg-kernel/limited-match-pair/v1\0",
        &[seed, pair],
    ));
    let first = draw(&mut rng, count);
    let mut second = draw(&mut rng, count - 1);
    if second >= first {
        second += 1;
    }
    let env_seed = rng.next_u64();
    let starting = if rng.next_u64() & 1 == 0 {
        PlayerId::P0
    } else {
        PlayerId::P1
    };
    Ok(([first, second], env_seed, starting))
}

#[cfg(test)]
#[path = "limited_match_v1/tests.rs"]
mod tests;
