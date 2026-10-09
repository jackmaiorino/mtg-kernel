//! Budgeted play primitives shared by every arm.
//!
//! Every engine transition goes through `apply`, which charges the arm's
//! meter first and refuses the transition that would exceed the cap. Forced
//! decisions (one legal action) take that action without a policy call or a
//! sampling draw; this is the plain policy for the whole instrument, corpus
//! and replay included.

use super::labels::{events, Before, SpyDefs, Suffix};
use crate::ids::PlayerId;
use crate::rl::TerminalClassificationV1;
use crate::rl_session::{FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;

/// A transition ceiling and the transitions charged against it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Meter {
    pub(crate) cap: u64,
    pub(crate) spent: u64,
}

impl Meter {
    pub(crate) fn new(cap: u64) -> Self {
        Self { cap, spent: 0 }
    }

    /// Charges one transition, or refuses it when it would exceed the cap.
    pub(crate) fn charge(&mut self) -> Result<(), PlayErr> {
        if self.spent + 1 > self.cap {
            return Err(PlayErr::Truncated);
        }
        self.spent += 1;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlayErr {
    /// The meter refused the next transition (planned resource truncation).
    Truncated,
    /// Invalid execution: an instrument fault, never a losing game.
    Fault(String),
}

/// How a trajectory ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum End {
    /// Natural terminal outcome for the focal player (a natural draw is a
    /// natural non-win).
    Natural { win: bool },
    /// The engine ended the game without a natural result.
    NonNatural,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Counters {
    pub(crate) inference: u64,
}

pub(crate) fn decision(s: &FastActorSessionV1) -> Option<FastActorDecisionV1> {
    match s.current_response() {
        FastActorResponseV1::Decision(d) => Some(d),
        FastActorResponseV1::Terminal(_) => None,
    }
}

pub(crate) fn terminal(s: &FastActorSessionV1, focal: PlayerId) -> Option<End> {
    let FastActorResponseV1::Terminal(t) = s.current_response() else {
        return None;
    };
    if t.terminal_classification != TerminalClassificationV1::Natural {
        return Some(End::NonNatural);
    }
    Some(End::Natural {
        win: t
            .winner
            .is_some_and(|w| crate::rl::PlayerSeatV1::from(focal) == w),
    })
}

pub(crate) fn acting(d: &FastActorDecisionV1) -> PlayerId {
    match d.acting_player {
        crate::rl::PlayerSeatV1::P0 => PlayerId(0),
        crate::rl::PlayerSeatV1::P1 => PlayerId(1),
    }
}

/// The plain policy: forced decisions take their only action without a draw.
pub(crate) fn act(
    p: &mut FrozenPlayPolicyV1,
    s: &FastActorSessionV1,
    d: &FastActorDecisionV1,
    c: &mut Counters,
) -> Result<u32, PlayErr> {
    if d.legal_action_count < 2 {
        return Ok(0);
    }
    c.inference += 1;
    p.select_fast_session_v1(s).map_err(PlayErr::Fault)
}

/// Applies one action, charging the meter first; updates the suffix labels
/// from the state before and after the transition.
pub(crate) fn apply(
    s: &mut FastActorSessionV1,
    d: &FastActorDecisionV1,
    a: u32,
    meter: &mut Meter,
    focal: PlayerId,
    labels: Option<&mut Suffix>,
    defs: &SpyDefs,
) -> Result<(), PlayErr> {
    meter.charge()?;
    let before = labels.as_ref().map(|l| {
        let mut b = Before::take(s.game_state(), focal, defs);
        b.self_chosen_now = l.self_pending;
        b
    });
    s.step(d.episode_id, d.step, a)
        .map_err(|e| PlayErr::Fault(format!("step {} action {a}: {e:?}", d.step)))?;
    if let (Some(l), Some(b)) = (labels, before) {
        l.apply(events(&b, s.game_state(), focal, defs));
        l.self_pending = false;
    }
    Ok(())
}

/// Offered/chosen labels at a focal multi-action decision.
pub(crate) fn observe(
    labels: Option<&mut Suffix>,
    s: &FastActorSessionV1,
    a: u32,
    focal: PlayerId,
    defs: &SpyDefs,
) {
    if let Some(l) = labels {
        if let Some(sem) = s.diagnostic_current_action_semantics() {
            l.observe_menu(s, &sem, a as usize, focal, defs);
        }
    }
}

/// Plain play for both seats to the end.
#[allow(clippy::too_many_arguments, reason = "explicit policy roles")]
pub(crate) fn plain_to_end(
    s: &mut FastActorSessionV1,
    focal: PlayerId,
    f: &mut FrozenPlayPolicyV1,
    o: &mut FrozenPlayPolicyV1,
    meter: &mut Meter,
    c: &mut Counters,
    mut labels: Option<&mut Suffix>,
    defs: &SpyDefs,
) -> Result<End, PlayErr> {
    loop {
        if let Some(end) = terminal(s, focal) {
            return Ok(end);
        }
        let d = decision(s).expect("non-terminal response is a decision");
        let a = if acting(&d) == focal {
            let a = act(f, s, &d, c)?;
            if d.legal_action_count >= 2 {
                observe(labels.as_deref_mut(), s, a, focal, defs);
            }
            a
        } else {
            act(o, s, &d, c)?
        };
        apply(s, &d, a, meter, focal, labels.as_deref_mut(), defs)?;
    }
}
