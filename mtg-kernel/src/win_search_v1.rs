//! Bounded exact search for a win before the current turn ends.
//!
//! Oracle estimand: from a clone of the pending decision, the searched seat's
//! choices are OR nodes and the other seat's are AND nodes, on the true game
//! state with hidden zones. A natural win for the searched seat proves a
//! node; the game ending otherwise, or the turn number advancing, refutes it.
//! A node past the depth bound, any node after the node budget runs out, and
//! an action the engine refuses make the result unknown, never a guess.
//!
//! Robust estimand: one line of the searched seat's choices that wins in
//! every world of a declared sample consistent with what that seat knows
//! (the true world plus resampled hidden zones). The line is open loop: at
//! each of the seat's decisions it plays the same action in every world, so
//! a proven robust line is a certificate for the sample, while failing to find
//! one certifies nothing. Opponent decisions stay AND nodes in each world.
//!
//! One reduction is applied to both: when every legal action is a
//! `ChooseEffectTarget` of one source and the remaining required picks equal
//! the candidates left, the selection is forced complete and only its order
//! varies, so the first candidate is searched alone. Results count its uses.

use crate::rl::{ActionSemanticV1, PlayerSeatV1};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinSearchBoundsV1 {
    /// Sessions expanded (one native step each) before the search stops.
    pub max_nodes: u64,
    /// Decisions below the root before a line is cut as unknown.
    pub max_depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinVerdictV1 {
    Proven,
    Refuted,
    Unknown,
}

impl WinVerdictV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            WinVerdictV1::Proven => "proven",
            WinVerdictV1::Refuted => "refuted",
            WinVerdictV1::Unknown => "unknown",
        }
    }
}

/// Why a search is unknown: the first bound it met.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownReasonV1 {
    Budget,
    Depth,
    UnsupportedAction,
    /// No single action of the searched seat applies in every sampled world.
    InformationBoundary,
}

impl UnknownReasonV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            UnknownReasonV1::Budget => "budget",
            UnknownReasonV1::Depth => "depth",
            UnknownReasonV1::UnsupportedAction => "unsupported_action",
            UnknownReasonV1::InformationBoundary => "information_boundary",
        }
    }
}

/// What the search needs from a native session.
pub trait WinSearchSessionV1: Clone {
    /// `Err(winner)` when terminal, else the acting seat and the semantics of
    /// its legal actions in engine order (empty when they cannot be read).
    fn decision(&self) -> Result<(PlayerSeatV1, Vec<ActionSemanticV1>), Option<PlayerSeatV1>>;
    fn turn(&self) -> u32;
    /// Applies the action at `index` of `decision()`'s list.
    fn apply(&mut self, index: usize) -> Result<(), String>;
    /// A world consistent with the acting seat's information, from `seed`.
    fn resampled_world(&self, seed: u64) -> Result<Self, String>;
    /// A readable name for the action at `index` (only built for reported lines).
    fn label(&self, _index: usize, semantic: &ActionSemanticV1) -> String {
        format!("{semantic:?}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinSearchResultV1 {
    pub verdict: WinVerdictV1,
    pub unknown_reason: Option<UnknownReasonV1>,
    pub hero: PlayerSeatV1,
    pub turn: u32,
    pub nodes: u64,
    pub depth_cuts: u64,
    pub refused_actions: u64,
    pub forced_order_collapses: u64,
    /// Expanded nodes by acting side and action kind, e.g. "hero:CastSpell".
    pub expanded: std::collections::BTreeMap<String, u64>,
    /// The searched seat's choices along a proving line (oracle: following the
    /// first legal reply at each AND node of that line).
    pub line: Vec<String>,
}

struct Search {
    hero: PlayerSeatV1,
    turn: u32,
    bounds: WinSearchBoundsV1,
    nodes: u64,
    exhausted: bool,
    depth_cuts: u64,
    refused: u64,
    collapses: u64,
    expanded: std::collections::BTreeMap<String, u64>,
}

impl Search {
    fn new<S: WinSearchSessionV1>(root: &S, hero: PlayerSeatV1, bounds: WinSearchBoundsV1) -> Self {
        Search { hero, turn: root.turn(), bounds, nodes: 0, exhausted: false, depth_cuts: 0, refused: 0, collapses: 0, expanded: Default::default() }
    }

    fn result(self, verdict: WinVerdictV1, mut line: Vec<String>) -> WinSearchResultV1 {
        if verdict != WinVerdictV1::Proven {
            line.clear();
        }
        // An oracle unknown always meets one of the first three; a robust one
        // that met none failed only for lack of a line common to every world.
        let unknown_reason = (verdict == WinVerdictV1::Unknown).then(|| {
            if self.exhausted { UnknownReasonV1::Budget }
            else if self.refused > 0 { UnknownReasonV1::UnsupportedAction }
            else if self.depth_cuts > 0 { UnknownReasonV1::Depth }
            else { UnknownReasonV1::InformationBoundary }
        });
        WinSearchResultV1 {
            verdict, unknown_reason, hero: self.hero, turn: self.turn, nodes: self.nodes, depth_cuts: self.depth_cuts,
            refused_actions: self.refused, forced_order_collapses: self.collapses, expanded: self.expanded, line,
        }
    }

    /// Spends one node of budget, or reports that none is left.
    fn spend(&mut self) -> bool {
        if self.nodes >= self.bounds.max_nodes {
            self.exhausted = true;
            return false;
        }
        self.nodes += 1;
        true
    }

    /// Settled value of a session without expanding it, if any.
    fn settled<S: WinSearchSessionV1>(&mut self, session: &S, depth: u32) -> Option<WinVerdictV1> {
        match session.decision() {
            Err(winner) => Some(if winner == Some(self.hero) { WinVerdictV1::Proven } else { WinVerdictV1::Refuted }),
            Ok(_) if session.turn() != self.turn => Some(WinVerdictV1::Refuted),
            Ok(_) if depth >= self.bounds.max_depth => {
                self.depth_cuts += 1;
                Some(WinVerdictV1::Unknown)
            }
            Ok(_) => None,
        }
    }

    fn child<S: WinSearchSessionV1>(&mut self, session: &S, index: usize) -> Option<S> {
        let mut child = session.clone();
        match child.apply(index) {
            Ok(()) => Some(child),
            Err(_) => {
                self.refused += 1;
                None
            }
        }
    }

    fn oracle<S: WinSearchSessionV1>(&mut self, session: &S, depth: u32, line: &mut Vec<String>) -> WinVerdictV1 {
        if let Some(verdict) = self.settled(session, depth) {
            return verdict;
        }
        let (actor, actions) = session.decision().expect("settled handles terminals");
        if actions.is_empty() {
            self.refused += 1;
            return WinVerdictV1::Unknown;
        }
        let hero_moves = actor == self.hero;
        let count = self.searched_count(&actions);
        // OR at the searched seat (any proven child proves), AND at the other
        // seat (any refuted child refutes); otherwise unknown children decide.
        let mut unknown = false;
        for (index, action) in actions.iter().enumerate().take(count) {
            if !self.spend() {
                return WinVerdictV1::Unknown;
            }
            *self.expanded.entry(format!("{}:{}", if hero_moves { "hero" } else { "opponent" }, kind_name(action))).or_default() += 1;
            let Some(child) = self.child(session, index) else {
                unknown = true;
                continue;
            };
            let mark = line.len();
            if hero_moves {
                line.push(session.label(index, action));
            }
            // The reported line follows the first reply at AND nodes.
            let verdict = if hero_moves || index == 0 {
                self.oracle(&child, depth + 1, line)
            } else {
                self.oracle(&child, depth + 1, &mut Vec::new())
            };
            match verdict {
                WinVerdictV1::Proven if hero_moves => return WinVerdictV1::Proven,
                WinVerdictV1::Refuted if !hero_moves => return WinVerdictV1::Refuted,
                WinVerdictV1::Unknown => unknown = true,
                _ => {}
            }
            if hero_moves {
                line.truncate(mark);
            }
        }
        match (unknown, hero_moves) {
            (true, _) => WinVerdictV1::Unknown,
            (false, true) => WinVerdictV1::Refuted,
            (false, false) => WinVerdictV1::Proven,
        }
    }

    /// Open-loop line over a set of worlds. Proven means one line wins in all
    /// of them; anything else is reported as unknown by the caller.
    fn robust<S: WinSearchSessionV1>(&mut self, worlds: Vec<S>, depth: u32, line: &mut Vec<String>) -> WinVerdictV1 {
        let mut live = Vec::with_capacity(worlds.len());
        let mut unknown = false;
        for world in worlds {
            match self.settled(&world, depth) {
                Some(WinVerdictV1::Proven) => {}
                Some(WinVerdictV1::Refuted) => return WinVerdictV1::Refuted,
                Some(WinVerdictV1::Unknown) => unknown = true,
                None => live.push(world),
            }
        }
        if unknown {
            return WinVerdictV1::Unknown;
        }
        if live.is_empty() {
            return WinVerdictV1::Proven;
        }
        // Opponent decisions first, one world at a time, as AND nodes.
        if let Some(position) = live.iter().position(|w| w.decision().map(|(actor, _)| actor != self.hero).unwrap_or(false)) {
            let (_, actions) = live[position].decision().expect("live worlds are decisions");
            let count = self.searched_count(&actions);
            let mut unknown = false;
            for index in 0..count {
                if !self.spend() {
                    return WinVerdictV1::Unknown;
                }
                let Some(child) = self.child(&live[position], index) else {
                    unknown = true;
                    continue;
                };
                let mut next = live.clone();
                next[position] = child;
                let mark = line.len();
                match self.robust(next, depth + 1, line) {
                    WinVerdictV1::Refuted => return WinVerdictV1::Refuted,
                    WinVerdictV1::Unknown => unknown = true,
                    WinVerdictV1::Proven => {}
                }
                if index > 0 {
                    line.truncate(mark);
                }
            }
            return if unknown { WinVerdictV1::Unknown } else { WinVerdictV1::Proven };
        }
        // Every live world awaits the searched seat: one action for all of them.
        let (_, first) = live[0].decision().expect("live worlds are decisions");
        let count = self.searched_count(&first);
        let mut unknown = false;
        for (index, action) in first.iter().enumerate().take(count) {
            let mut next = Vec::with_capacity(live.len());
            for world in &live {
                let (_, actions) = world.decision().expect("live worlds are decisions");
                let Some(matching) = actions.iter().position(|a| a == action) else { break };
                if !self.spend() {
                    return WinVerdictV1::Unknown;
                }
                match self.child(world, matching) {
                    Some(child) => next.push(child),
                    None => break,
                }
            }
            if next.len() != live.len() {
                unknown = true;
                continue;
            }
            let mark = line.len();
            line.push(live[0].label(index, action));
            match self.robust(next, depth + 1, line) {
                WinVerdictV1::Proven => return WinVerdictV1::Proven,
                WinVerdictV1::Unknown => unknown = true,
                WinVerdictV1::Refuted => {}
            }
            line.truncate(mark);
        }
        if unknown { WinVerdictV1::Unknown } else { WinVerdictV1::Refuted }
    }

    fn searched_count(&mut self, actions: &[ActionSemanticV1]) -> usize {
        if forced_order_only(actions) {
            self.collapses += 1;
            1
        } else {
            actions.len()
        }
    }
}

fn kind_name(action: &ActionSemanticV1) -> String {
    let text = format!("{action:?}");
    text.split([' ', '{', '(']).next().unwrap_or("").to_string()
}

/// Oracle search from the session's pending decision, for its acting seat.
pub fn oracle_win_this_turn_v1<S: WinSearchSessionV1>(session: &S, bounds: WinSearchBoundsV1) -> Option<WinSearchResultV1> {
    let (hero, _) = session.decision().ok()?;
    let mut search = Search::new(session, hero, bounds);
    let mut line = Vec::new();
    let verdict = search.oracle(session, 0, &mut line);
    Some(search.result(verdict, line))
}

/// Robust search over the true world plus `seeds.len()` resampled worlds.
/// Only a proven verdict is a certificate; a failure is reported as unknown
/// (a world without a win is the oracle's to certify).
pub fn robust_win_this_turn_v1<S: WinSearchSessionV1>(session: &S, seeds: &[u64], bounds: WinSearchBoundsV1) -> Option<Result<WinSearchResultV1, String>> {
    let (hero, _) = session.decision().ok()?;
    let mut worlds = vec![session.clone()];
    for &seed in seeds {
        match session.resampled_world(seed) {
            Ok(world) => worlds.push(world),
            Err(error) => return Some(Err(error)),
        }
    }
    let mut search = Search::new(session, hero, bounds);
    let mut line = Vec::new();
    let verdict = match search.robust(worlds, 0, &mut line) {
        WinVerdictV1::Proven => WinVerdictV1::Proven,
        _ => WinVerdictV1::Unknown,
    };
    Some(Ok(search.result(verdict, line)))
}

/// Every candidate selects a target of one effect, and the picks still
/// required equal the candidates offered: only the order of a forced set varies.
fn forced_order_only(actions: &[ActionSemanticV1]) -> bool {
    let mut first = None;
    for action in actions {
        let ActionSemanticV1::ChooseEffectTarget { source, selected_count, min_targets, max_targets, .. } = action
        else {
            return false;
        };
        if min_targets != max_targets {
            return false;
        }
        match first {
            None => first = Some((source, *selected_count, *min_targets)),
            Some((s, c, m)) if s == source && c == *selected_count && m == *min_targets => {}
            Some(_) => return false,
        }
    }
    match first {
        Some((_, selected, required)) => usize::from(required.saturating_sub(selected)) == actions.len() && actions.len() > 1,
        None => false,
    }
}

impl WinSearchSessionV1 for crate::rl_session::FastActorSessionV1 {
    fn decision(&self) -> Result<(PlayerSeatV1, Vec<ActionSemanticV1>), Option<PlayerSeatV1>> {
        match self.current_response() {
            crate::rl_session::FastActorResponseV1::Terminal(terminal) => Err(terminal.winner),
            crate::rl_session::FastActorResponseV1::Decision(decision) => Ok((
                decision.acting_player,
                self.diagnostic_current_action_semantics()
                    .filter(|semantics| semantics.len() == decision.legal_action_count as usize)
                    .unwrap_or_default(),
            )),
        }
    }

    fn turn(&self) -> u32 {
        self.game_state().turn
    }

    fn apply(&mut self, index: usize) -> Result<(), String> {
        let crate::rl_session::FastActorResponseV1::Decision(decision) = self.current_response() else {
            return Err("the session is terminal".to_string());
        };
        self.step(decision.episode_id, decision.step, index as u32)
            .map(|_| ())
            .map_err(|error| format!("{error:?}"))
    }

    fn resampled_world(&self, seed: u64) -> Result<Self, String> {
        self.census_redeterminized_clone_v1(seed)
    }
}
