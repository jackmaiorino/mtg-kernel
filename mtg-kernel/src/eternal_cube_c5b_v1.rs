//! Card programs for the eternal catalog's Vintage Cube wave C5b
//! (`docs/design/eternal_formats_support_v1.md`): token makers and mana creatures.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c5b_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::TargetSpec;
use crate::effect::{EffectContinuation, EffectOp, ExecCtx};
use crate::state::GameState;
use crate::trigger::TriggeredAbilityDef;
use serde::{Deserialize, Serialize};

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C5bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C5bOpV1 {}

impl C5bOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        match *self {}
    }
}

// ---- Triggered abilities ------------------------------------------------------------

/// Triggered abilities of this module's cards, falling through to wave
/// C5c.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    crate::eternal_cube_c5c_v1::triggers_for(name)
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C5bOpV1, _ctx: &ExecCtx, _state: &mut GameState) {
    match *op {}
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    _state: &mut GameState,
    _continuation: &mut EffectContinuation,
    op: &C5bOpV1,
    _path: Vec<u16>,
) -> Result<bool, String> {
    match *op {}
}
