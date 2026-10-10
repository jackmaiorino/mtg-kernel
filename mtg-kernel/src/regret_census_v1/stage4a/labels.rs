//! Diagnostic Spy suffix labels (RUNNER.md section 6). Labelling only:
//! nothing that chooses, samples, scores or backs up reads these labels.
//!
//! The frozen resolved suffix, on one trajectory and in this order:
//! 1. (cast roots only) the focal player's cast Balustrade Spy resolves: the
//!    Spy spell object leaves the stack onto the battlefield;
//! 2. Spy's self-targeted triggered ability resolves: an ability item from a
//!    focal Balustrade Spy targeting the focal player leaves the stack and the
//!    focal library shrinks in that transition;
//! 3. Dread Return targeting Lotleth Giant resolves and the Giant enters the
//!    battlefield: a focal Dread Return spell targeting a Lotleth Giant leaves
//!    the stack in the same transition in which that Giant moves onto the
//!    battlefield.
//!
//! Detection compares the state before and after every engine transition of
//! the trajectory (both players' actions), so it records resolutions, not
//! choices. Offered and chosen labels are kept separately, from focal menus.

use crate::card_def::CARD_DEFS;
use crate::ids::{ObjectId, PlayerId};
use crate::rl::{ActionSemanticV1, PlayerSeatV1, TargetRefV1};
use crate::rl_session::FastActorSessionV1;
use crate::state::{GameState, StackItem, Target, Zone};
use serde_json::{json, Value};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug)]
pub(crate) struct SpyDefs {
    pub(crate) spy: u16,
    pub(crate) dread_return: u16,
    pub(crate) giant: u16,
}

fn def_id(name: &str) -> u16 {
    CARD_DEFS
        .iter()
        .position(|c| c.name == name)
        .unwrap_or_else(|| panic!("card {name} is not defined")) as u16
}

pub(crate) fn spy_defs() -> SpyDefs {
    static DEFS: OnceLock<SpyDefs> = OnceLock::new();
    *DEFS.get_or_init(|| SpyDefs {
        spy: def_id("Balustrade Spy"),
        dread_return: def_id("Dread Return"),
        giant: def_id("Lotleth Giant"),
    })
}

/// Pre-transition snapshot of the items the tracker watches.
#[derive(Clone, Debug, Default)]
pub(crate) struct Before {
    /// Focal Spy spell objects on the stack.
    spy_spells: Vec<ObjectId>,
    /// Number of focal Spy self-target ability items on the stack.
    self_target_items: usize,
    /// (Dread Return source, Giant target) pairs of focal spells on the
    /// stack or pending with that target chosen.
    dr_giant: Vec<(ObjectId, ObjectId)>,
    library: usize,
    /// The focal player chose the Spy self-target in this very transition
    /// (the trigger can go on the stack and resolve within it).
    pub(crate) self_chosen_now: bool,
}

impl Before {
    /// The focal player chose to cast Spy in this transition: the spell can
    /// be cast, finalized and resolved within it, so watch its hand objects.
    pub(crate) fn watch_spy_cast(&mut self, state: &GameState, focal: PlayerId, d: &SpyDefs) {
        for &o in &state.players[focal.index()].hand {
            if state.objects.get(o).card_def == d.spy && !self.spy_spells.contains(&o) {
                self.spy_spells.push(o);
            }
        }
    }
}

fn is_spell(item: &StackItem, state: &GameState) -> bool {
    item.inline_effect.is_none() && state.objects.get(item.source).zone == Zone::Stack
}

fn self_target(item: &StackItem, state: &GameState, focal: PlayerId, d: &SpyDefs) -> bool {
    item.inline_effect.is_some()
        && item.controller == focal
        && state.objects.get(item.source).card_def == d.spy
        && item.targets.contains(&Target::Player(focal))
}

impl Before {
    pub(crate) fn take(state: &GameState, focal: PlayerId, d: &SpyDefs) -> Self {
        let mut b = Self {
            library: state.players[focal.index()].library.len(),
            ..Self::default()
        };
        for item in &state.stack {
            if item.controller != focal || item.is_copy {
                continue;
            }
            let def = state.objects.get(item.source).card_def;
            if is_spell(item, state) && def == d.spy {
                b.spy_spells.push(item.source);
            }
            if self_target(item, state, focal, d) {
                b.self_target_items += 1;
            }
            if is_spell(item, state) && def == d.dread_return {
                for t in &item.targets {
                    if let Target::Object(g) = t {
                        if state.objects.get(*g).card_def == d.giant {
                            b.dr_giant.push((item.source, *g));
                        }
                    }
                }
            }
        }
        // A cast whose last choice finalizes it can resolve in the same
        // transition (the engine skips priority passes with no other
        // action), so a pending DR cast that has chosen a Giant counts too.
        if let Some(p) = &state.engine.pending_cast {
            if p.controller == focal && state.objects.get(p.spell).card_def == d.dread_return {
                for t in &p.targets_chosen {
                    if let Target::Object(g) = t {
                        if state.objects.get(*g).card_def == d.giant
                            && !b.dr_giant.contains(&(p.spell, *g))
                        {
                            b.dr_giant.push((p.spell, *g));
                        }
                    }
                }
            }
        }
        b
    }

    fn quiet(&self) -> bool {
        self.spy_spells.is_empty()
            && self.self_target_items == 0
            && !self.self_chosen_now
            && self.dr_giant.is_empty()
    }
}

/// Resolution events in one transition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Events {
    pub(crate) spy_resolved: bool,
    pub(crate) self_target_resolved: bool,
    pub(crate) dr_giant_resolved: bool,
    /// A focal Dread Return targeting a Lotleth Giant was on the stack.
    pub(crate) dr_giant_stacked: bool,
}

pub(crate) fn events(before: &Before, after: &GameState, focal: PlayerId, d: &SpyDefs) -> Events {
    let mut e = Events::default();
    if before.quiet() {
        return e;
    }
    e.dr_giant_stacked = !before.dr_giant.is_empty();
    e.spy_resolved = before.spy_spells.iter().any(|&s| {
        let o = after.objects.get(s);
        o.zone == Zone::Battlefield && o.controller == focal
    });
    let watched = before.self_target_items + usize::from(before.self_chosen_now);
    if watched > 0 {
        let now = after
            .stack
            .iter()
            .filter(|i| i.controller == focal && !i.is_copy && self_target(i, after, focal, d))
            .count();
        e.self_target_resolved =
            now < watched && after.players[focal.index()].library.len() < before.library;
    }
    e.dr_giant_resolved = before.dr_giant.iter().any(|&(dr, g)| {
        let pending = after.engine.pending_cast.as_ref().is_some_and(|p| p.spell == dr);
        let dr_left = !pending
            && (after.objects.get(dr).zone != Zone::Stack
                || !after.stack.iter().any(|i| i.source == dr && !i.is_copy));
        let giant = after.objects.get(g);
        let resolved = dr_left && giant.zone == Zone::Battlefield && giant.controller == focal;
        if dr_left && !resolved && std::env::var_os("S4A_TRACE_LABELS").is_some() {
            // Diagnostic only (never set in formal runs): why a DR -> Giant
            // spell left the stack without the Giant entering.
            eprintln!(
                "label-trace: DR left the stack; dr zone {:?}, giant zone {:?} controller {:?}, turn {}, stack {}, life {:?}",
                after.objects.get(dr).zone,
                giant.zone,
                giant.controller,
                after.turn,
                after.stack.len(),
                [after.players[0].life, after.players[1].life]
            );
        }
        resolved
    });
    e
}

/// Per-trajectory suffix state and labels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Suffix {
    /// Whether the trajectory must start with the Spy cast (cast roots).
    pub(crate) needs_cast: bool,
    /// Steps completed in order: 0 none, then 1..=3 as above (target roots
    /// start at 1).
    stage: u8,
    pub(crate) cast_offered: bool,
    pub(crate) cast_chosen: bool,
    pub(crate) self_offered: bool,
    pub(crate) self_chosen: bool,
    pub(crate) dr_giant_offered: bool,
    pub(crate) dr_giant_chosen: bool,
    pub(crate) spy_resolved: bool,
    pub(crate) self_target_resolved: bool,
    pub(crate) dr_giant_resolved: bool,
    pub(crate) dr_giant_stacked: bool,
    /// Set by `observe_menu` when the focal player chooses the Spy
    /// self-target; read and cleared by the next transition.
    pub(crate) self_pending: bool,
    /// Set by `observe_menu` when the focal player chooses to cast Spy; read
    /// and cleared by the next transition.
    pub(crate) cast_pending: bool,
    /// Evaluation only: SHA-256 chain over every transition's acting seat
    /// and chosen action semantic (both players), for retained-failure
    /// diagnosis without rerunning. `None` in selection playouts.
    pub(crate) history: Option<[u8; 32]>,
    pub(crate) history_len: u64,
}

impl Suffix {
    pub(crate) fn new(needs_cast: bool) -> Self {
        Self {
            needs_cast,
            stage: if needs_cast { 0 } else { 1 },
            ..Self::default()
        }
    }

    pub(crate) fn complete(&self) -> bool {
        self.stage == 3
    }

    pub(crate) fn apply(&mut self, e: Events) {
        self.spy_resolved |= e.spy_resolved;
        self.self_target_resolved |= e.self_target_resolved;
        self.dr_giant_resolved |= e.dr_giant_resolved;
        self.dr_giant_stacked |= e.dr_giant_stacked;
        // Advance in order; a later step only counts after the earlier one.
        if self.stage == 0 && e.spy_resolved {
            self.stage = 1;
        } else if self.stage == 1 && e.self_target_resolved {
            self.stage = 2;
        } else if self.stage == 2 && e.dr_giant_resolved {
            self.stage = 3;
        }
    }

    /// Offered/chosen labels at a focal decision with menu `sem`, choice `a`.
    pub(crate) fn observe_menu(
        &mut self,
        session: &FastActorSessionV1,
        sem: &[ActionSemanticV1],
        a: usize,
        focal: PlayerId,
        d: &SpyDefs,
    ) {
        let state = session.game_state();
        for (i, x) in sem.iter().enumerate() {
            let chosen = i == a;
            let (cast, own, dr) = classify(x, state, focal, d);
            self.cast_offered |= cast;
            self.cast_chosen |= cast && chosen;
            self.self_offered |= own;
            self.self_chosen |= own && chosen;
            self.self_pending |= own && chosen;
            self.cast_pending |= cast && chosen;
            self.dr_giant_offered |= dr;
            self.dr_giant_chosen |= dr && chosen;
        }
    }

    pub(crate) fn json(&self) -> Value {
        json!({"complete":self.complete(),"stage":self.stage,
            "cast_offered":self.cast_offered,"cast_chosen":self.cast_chosen,
            "self_offered":self.self_offered,"self_chosen":self.self_chosen,
            "dr_giant_offered":self.dr_giant_offered,"dr_giant_chosen":self.dr_giant_chosen,
            "spy_resolved":self.spy_resolved,"self_target_resolved":self.self_target_resolved,
            "dr_giant_resolved":self.dr_giant_resolved,"dr_giant_stacked":self.dr_giant_stacked,
            "action_history_sha256":self.history.map(|h| h.iter().map(|b| format!("{b:02x}")).collect::<String>()),
            "action_history_len":self.history_len})
    }
}

/// (cast Spy, Spy targets focal, Dread Return targets Lotleth Giant).
pub(crate) fn classify(
    x: &ActionSemanticV1,
    state: &GameState,
    focal: PlayerId,
    d: &SpyDefs,
) -> (bool, bool, bool) {
    match x {
        ActionSemanticV1::CastSpell { source, .. } => (source.card_db_id == d.spy, false, false),
        ActionSemanticV1::ChooseTarget { source, target, .. } => {
            let own = source.card_db_id == d.spy
                && matches!(target, TargetRefV1::Player { player } if *player == PlayerSeatV1::from(focal));
            let dr = source.card_db_id == d.dread_return
                && matches!(target, TargetRefV1::Object { object } if object.card_db_id == d.giant);
            let _ = state;
            (false, own, dr)
        }
        _ => (false, false, false),
    }
}

/// Root strata (RUNNER.md section 4) from the current menu alone:
/// cast = a legal cast-Spy action; target = the Spy target menu offers the
/// focal player and another legal target.
pub(crate) fn root_strata(
    sem: &[ActionSemanticV1],
    state: &GameState,
    focal: PlayerId,
    d: &SpyDefs,
) -> (bool, bool) {
    let cast = sem.iter().any(|x| classify(x, state, focal, d).0);
    let spy_targets: Vec<&ActionSemanticV1> = sem
        .iter()
        .filter(|x| matches!(x, ActionSemanticV1::ChooseTarget { source, .. } if source.card_db_id == d.spy))
        .collect();
    let own = spy_targets.iter().any(|x| classify(x, state, focal, d).1);
    let other = spy_targets.iter().any(|x| !classify(x, state, focal, d).1);
    (cast, own && other)
}
