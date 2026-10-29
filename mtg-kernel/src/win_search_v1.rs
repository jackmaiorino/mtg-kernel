//! Bounded exact search for a win before the current turn ends.
//!
//! Oracle estimand: from a clone of the pending decision, the searched seat's
//! choices are OR nodes and the other seat's are AND nodes, on the true game
//! state with hidden zones. A natural win for the searched seat proves a
//! node; the game ending otherwise, or the turn passing to the other seat,
//! refutes it.
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
    /// The current turn: the engine's round number with the active seat (a
    /// round holds both players' turns, so the round alone is not a turn).
    fn turn(&self) -> (u32, PlayerSeatV1);
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
    pub turn: (u32, PlayerSeatV1),
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
    turn: (u32, PlayerSeatV1),
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
        Search {
            hero,
            turn: root.turn(),
            bounds,
            nodes: 0,
            exhausted: false,
            depth_cuts: 0,
            refused: 0,
            collapses: 0,
            expanded: Default::default(),
        }
    }

    fn result(self, verdict: WinVerdictV1, mut line: Vec<String>) -> WinSearchResultV1 {
        if verdict != WinVerdictV1::Proven {
            line.clear();
        }
        // An oracle unknown always meets one of the first three; a robust one
        // that met none failed only for lack of a line common to every world.
        let reason = if self.exhausted {
            UnknownReasonV1::Budget
        } else if self.refused > 0 {
            UnknownReasonV1::UnsupportedAction
        } else if self.depth_cuts > 0 {
            UnknownReasonV1::Depth
        } else {
            UnknownReasonV1::InformationBoundary
        };
        let unknown_reason = (verdict == WinVerdictV1::Unknown).then_some(reason);
        WinSearchResultV1 {
            verdict,
            unknown_reason,
            hero: self.hero,
            turn: self.turn,
            nodes: self.nodes,
            depth_cuts: self.depth_cuts,
            refused_actions: self.refused,
            forced_order_collapses: self.collapses,
            expanded: self.expanded,
            line,
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
            Err(winner) => Some(if winner == Some(self.hero) {
                WinVerdictV1::Proven
            } else {
                WinVerdictV1::Refuted
            }),
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

    fn oracle<S: WinSearchSessionV1>(
        &mut self,
        session: &S,
        depth: u32,
        line: &mut Vec<String>,
    ) -> WinVerdictV1 {
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
        // At the other seat's decisions try a pass first: it is usually the
        // refutation, and an AND node stops at its first refuted child.
        let mut order: Vec<usize> = (0..count).collect();
        if !hero_moves {
            order.sort_by_key(|&i| !matches!(actions[i], ActionSemanticV1::Pass { .. }));
        }
        for (position, index) in order.into_iter().enumerate() {
            let action = &actions[index];
            if !self.spend() {
                return WinVerdictV1::Unknown;
            }
            *self
                .expanded
                .entry(format!(
                    "{}:{}",
                    if hero_moves { "hero" } else { "opponent" },
                    kind_name(action)
                ))
                .or_default() += 1;
            let Some(child) = self.child(session, index) else {
                unknown = true;
                continue;
            };
            let mark = line.len();
            if hero_moves {
                line.push(session.label(index, action));
            }
            // The reported line follows the first reply tried at AND nodes.
            let verdict = if hero_moves || position == 0 {
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
    fn robust<S: WinSearchSessionV1>(
        &mut self,
        worlds: Vec<S>,
        depth: u32,
        line: &mut Vec<String>,
    ) -> WinVerdictV1 {
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
        if let Some(position) = live.iter().position(|w| {
            w.decision()
                .map(|(actor, _)| actor != self.hero)
                .unwrap_or(false)
        }) {
            let (_, actions) = live[position]
                .decision()
                .expect("live worlds are decisions");
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
            return if unknown {
                WinVerdictV1::Unknown
            } else {
                WinVerdictV1::Proven
            };
        }
        // Every live world awaits the searched seat: one action for all of them.
        let (_, first) = live[0].decision().expect("live worlds are decisions");
        let count = self.searched_count(&first);
        let mut unknown = false;
        for (index, action) in first.iter().enumerate().take(count) {
            let mut next = Vec::with_capacity(live.len());
            for world in &live {
                let (_, actions) = world.decision().expect("live worlds are decisions");
                // Match what the seat sees: resampling relabels hidden objects,
                // so object ids and zone-change counts are not part of the key.
                let key = public_key(action);
                let Some(matching) = actions.iter().position(|a| public_key(a) == key) else {
                    break;
                };
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
        if unknown {
            WinVerdictV1::Unknown
        } else {
            WinVerdictV1::Refuted
        }
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
pub fn oracle_win_this_turn_v1<S: WinSearchSessionV1>(
    session: &S,
    bounds: WinSearchBoundsV1,
) -> Option<WinSearchResultV1> {
    let (hero, _) = session.decision().ok()?;
    let mut search = Search::new(session, hero, bounds);
    let mut line = Vec::new();
    let verdict = search.oracle(session, 0, &mut line);
    Some(search.result(verdict, line))
}

/// Robust search over the true world plus `seeds.len()` resampled worlds.
/// Only a proven verdict is a certificate; a failure is reported as unknown
/// (a world without a win is the oracle's to certify).
pub fn robust_win_this_turn_v1<S: WinSearchSessionV1>(
    session: &S,
    seeds: &[u64],
    bounds: WinSearchBoundsV1,
) -> Option<Result<WinSearchResultV1, String>> {
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

/// An action as the acting seat identifies it across sampled worlds: its
/// debug form without object ids or zone-change counts.
fn public_key(action: &ActionSemanticV1) -> String {
    let text = format!("{action:?}");
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = ["arena_id: ", "zone_change_count: "]
        .iter()
        .filter_map(|p| rest.find(p).map(|i| (i, p.len())))
        .min()
    {
        out.push_str(&rest[..at.0]);
        let tail = &rest[at.0 + at.1..];
        rest = tail.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    out.push_str(rest);
    out
}

/// Every candidate selects a target of one effect, and the picks still
/// required equal the candidates offered: only the order of a forced set varies.
fn forced_order_only(actions: &[ActionSemanticV1]) -> bool {
    let mut first = None;
    for action in actions {
        let ActionSemanticV1::ChooseEffectTarget {
            source,
            selected_count,
            min_targets,
            max_targets,
            ..
        } = action
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
        Some((_, selected, required)) => {
            usize::from(required.saturating_sub(selected)) == actions.len() && actions.len() > 1
        }
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

    fn turn(&self) -> (u32, PlayerSeatV1) {
        let state = self.game_state();
        (state.turn, state.active_player.into())
    }

    fn apply(&mut self, index: usize) -> Result<(), String> {
        let crate::rl_session::FastActorResponseV1::Decision(decision) = self.current_response()
        else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::CardStableRefV1;
    use crate::state::Zone;

    const HERO: PlayerSeatV1 = PlayerSeatV1::P0;
    const OPP: PlayerSeatV1 = PlayerSeatV1::P1;

    fn card(arena_id: u32, card_db_id: u16, zone: Zone) -> CardStableRefV1 {
        CardStableRefV1 {
            arena_id,
            card_db_id,
            owner: HERO,
            controller: HERO,
            zone,
            zone_change_count: arena_id,
        }
    }

    /// A toy game: each node names its actor and children; leaves are a win
    /// for one seat, the end of the turn, or more of the same turn.
    #[derive(Clone, Debug)]
    enum Node {
        Decide(PlayerSeatV1, Vec<(ActionSemanticV1, usize)>),
        Won(Option<PlayerSeatV1>),
        NextTurn,
    }

    #[derive(Clone)]
    struct Toy {
        nodes: std::rc::Rc<Vec<Node>>,
        at: usize,
        worlds: std::rc::Rc<Vec<Vec<Node>>>,
    }

    impl WinSearchSessionV1 for Toy {
        fn decision(&self) -> Result<(PlayerSeatV1, Vec<ActionSemanticV1>), Option<PlayerSeatV1>> {
            match &self.nodes[self.at] {
                Node::Decide(actor, edges) => {
                    Ok((*actor, edges.iter().map(|(a, _)| a.clone()).collect()))
                }
                Node::Won(winner) => Err(*winner),
                Node::NextTurn => Ok((OPP, vec![ActionSemanticV1::Pass { actor: OPP }])),
            }
        }
        fn turn(&self) -> (u32, PlayerSeatV1) {
            (
                1,
                if matches!(self.nodes[self.at], Node::NextTurn) {
                    OPP
                } else {
                    HERO
                },
            )
        }
        fn apply(&mut self, index: usize) -> Result<(), String> {
            let Node::Decide(_, edges) = &self.nodes[self.at] else {
                return Err("not a decision".into());
            };
            self.at = edges[index].1;
            Ok(())
        }
        fn resampled_world(&self, seed: u64) -> Result<Self, String> {
            let nodes = self
                .worlds
                .get(seed as usize)
                .ok_or("no such world")?
                .clone();
            Ok(Toy {
                nodes: std::rc::Rc::new(nodes),
                at: self.at,
                worlds: self.worlds.clone(),
            })
        }
    }

    fn toy(nodes: Vec<Node>) -> Toy {
        Toy {
            nodes: std::rc::Rc::new(nodes),
            at: 0,
            worlds: std::rc::Rc::new(Vec::new()),
        }
    }

    fn play(id: u32) -> ActionSemanticV1 {
        ActionSemanticV1::PlayLand {
            actor: HERO,
            source: card(id, 7, Zone::Hand),
        }
    }

    const BOUNDS: WinSearchBoundsV1 = WinSearchBoundsV1 {
        max_nodes: 1_000,
        max_depth: 50,
    };

    #[test]
    fn oracle_needs_one_winning_choice_and_every_reply() {
        // Hero: line 1 loses to the turn ending, line 2 reaches an opponent
        // choice where both replies still lose for the opponent.
        let nodes = vec![
            Node::Decide(HERO, vec![(play(1), 1), (play(2), 2)]),
            Node::NextTurn,
            Node::Decide(
                OPP,
                vec![(ActionSemanticV1::Pass { actor: OPP }, 3), (play(9), 3)],
            ),
            Node::Won(Some(HERO)),
        ];
        let result = oracle_win_this_turn_v1(&toy(nodes.clone()), BOUNDS).unwrap();
        assert_eq!(result.verdict, WinVerdictV1::Proven);
        assert_eq!(result.line.len(), 1);
        // One reply that escapes refutes the line.
        let mut refuted = nodes;
        refuted[2] = Node::Decide(
            OPP,
            vec![(ActionSemanticV1::Pass { actor: OPP }, 3), (play(9), 1)],
        );
        let result = oracle_win_this_turn_v1(&toy(refuted), BOUNDS).unwrap();
        assert_eq!(
            (result.verdict, result.unknown_reason),
            (WinVerdictV1::Refuted, None)
        );
    }

    #[test]
    fn budget_and_depth_give_unknown_never_a_guess() {
        let nodes = vec![
            Node::Decide(HERO, vec![(play(1), 1), (play(2), 2)]),
            Node::NextTurn,
            Node::Won(Some(HERO)),
        ];
        let tight = WinSearchBoundsV1 {
            max_nodes: 1,
            max_depth: 50,
        };
        let result = oracle_win_this_turn_v1(&toy(nodes.clone()), tight).unwrap();
        assert_eq!(
            (result.verdict, result.unknown_reason),
            (WinVerdictV1::Unknown, Some(UnknownReasonV1::Budget))
        );
        let shallow = WinSearchBoundsV1 {
            max_nodes: 1_000,
            max_depth: 0,
        };
        let result = oracle_win_this_turn_v1(&toy(nodes), shallow).unwrap();
        assert_eq!(
            (result.verdict, result.unknown_reason),
            (WinVerdictV1::Unknown, Some(UnknownReasonV1::Depth))
        );
    }

    #[test]
    fn robust_line_must_win_in_every_world_with_one_choice() {
        // The true world wins by playing land 1; world 0 wins by land 2 only.
        let truth = vec![
            Node::Decide(HERO, vec![(play(1), 1), (play(2), 2)]),
            Node::Won(Some(HERO)),
            Node::NextTurn,
        ];
        let other = vec![
            Node::Decide(HERO, vec![(play(1), 2), (play(2), 1)]),
            Node::Won(Some(HERO)),
            Node::NextTurn,
        ];
        let session = Toy {
            worlds: std::rc::Rc::new(vec![other.clone(), truth.clone()]),
            ..toy(truth.clone())
        };
        assert_eq!(
            oracle_win_this_turn_v1(&session, BOUNDS).unwrap().verdict,
            WinVerdictV1::Proven
        );
        let robust = robust_win_this_turn_v1(&session, &[0], BOUNDS)
            .unwrap()
            .unwrap();
        assert_eq!(robust.verdict, WinVerdictV1::Unknown);
        let robust = robust_win_this_turn_v1(&session, &[1], BOUNDS)
            .unwrap()
            .unwrap();
        assert_eq!(robust.verdict, WinVerdictV1::Proven);
    }

    #[test]
    fn public_key_ignores_hidden_object_identity() {
        let a = ActionSemanticV1::CastSpell {
            actor: HERO,
            source: card(10, 151, Zone::Graveyard),
        };
        let b = ActionSemanticV1::CastSpell {
            actor: HERO,
            source: card(77, 151, Zone::Graveyard),
        };
        let c = ActionSemanticV1::CastSpell {
            actor: HERO,
            source: card(10, 152, Zone::Graveyard),
        };
        assert_eq!(public_key(&a), public_key(&b));
        assert_ne!(public_key(&a), public_key(&c));
    }
}
