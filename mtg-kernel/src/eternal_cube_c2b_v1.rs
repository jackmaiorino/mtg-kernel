//! Card programs for the eternal catalog's Vintage Cube wave C2b
//! (`docs/design/eternal_formats_support_v1.md`): utility lands.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c2b_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Most of this wave is data. Horizon Canopy's shape (Sunbaked Canyon,
//! Waterlogged Grove), Mana Confluence, the creature lands (Celestial
//! Colonnade, Creeping Tar Pit, whose animations live in
//! `build_support/standard_lands_v1.rs` like Ghitu Encampment's) and Library of
//! Alexandria's draw (gated by `activation_allowed`) reuse existing recipes.
//! Fabled Passage is `LibrarySearchDestinationV1::BattlefieldTappedUntapWith
//! FourLandsV1`, Abandoned Air Temple's entry rule is
//! `EntersTappedUnlessControllerDef::ControlsBasicLandV1`, and Shelldock
//! Isle's play half is `EffectOp::PlayHideawayIfLibraryTwentyOrFewerV1` (its
//! hideaway half is the engine's `EffectOp::Hideaway`, as Collector's Cage's).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardType, TargetSpec};
use crate::effect::{EffectContinuation, EffectOp, ExecCtx};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event;
use crate::ids::ObjectId;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

const LIBRARY_OF_ALEXANDRIA: &str = "Library of Alexandria";
const SHELLDOCK_ISLE: &str = "Shelldock Isle";

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C2bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C2bOpV1 {
    /// Abandoned Air Temple: a +1/+1 counter on each creature the controller
    /// controls.
    PlusOneCounterOnEachCreatureV1,
}

impl C2bOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    /// The counters are placed with no choice.
    pub(crate) fn contains_player_choice(&self) -> bool {
        match self {
            Self::PlusOneCounterOnEachCreatureV1 => false,
        }
    }
}

fn c2b(op: C2bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C2bV1(op)))
}

// ---- Programs -----------------------------------------------------------------------

/// Abandoned Air Temple: "{3}{W}, {T}: Put a +1/+1 counter on each creature
/// you control."
pub fn temple_counters() -> EffectOp {
    c2b(C2bOpV1::PlusOneCounterOnEachCreatureV1)
}

/// Shelldock Isle: "{U}, {T}: You may play the exiled card without paying its
/// mana cost if a library has twenty or fewer cards in it."
pub fn shelldock_play() -> EffectOp {
    EffectOp::PlayHideawayIfLibraryTwentyOrFewerV1
}

/// Shelldock Isle: "Hideaway 4 (When this land enters, look at the top four
/// cards of your library, exile one face down, then put the rest on the
/// bottom in a random order.)"
pub fn shelldock_hideaway() -> EffectOp {
    EffectOp::Hideaway { count: 4 }
}

// ---- Activation conditions ----------------------------------------------------------

/// Activated abilities this module gates: Library of Alexandria's "{T}: Draw
/// a card. Activate only if you have exactly seven cards in hand." The
/// condition is checked on activation only (the ability's resolution does not
/// look at the hand again).
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    name: &str,
    ability_index: usize,
) -> bool {
    if name == LIBRARY_OF_ALEXANDRIA && ability_index == 0 {
        let controller = state.objects.get(source).controller;
        return state.players[controller.index()].hand.len() == 7;
    }
    true
}

// ---- Triggered abilities ------------------------------------------------------------

const fn etb_trigger(effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

const SHELLDOCK_ISLE_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(shelldock_hideaway)];

/// Triggered abilities of this module's cards, falling through to wave
/// C2c.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        SHELLDOCK_ISLE => &SHELLDOCK_ISLE_TRIGGERS,
        _ => crate::eternal_cube_c2c_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        SHELLDOCK_ISLE if *effect == shelldock_hideaway() => Some(TargetSpec::None),
        _ => delayed_effect_matches(name, effect).then_some(TargetSpec::None),
    }
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C2bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C2bOpV1::PlusOneCounterOnEachCreatureV1 => {
            // "Each creature you control" is the set at resolution; the
            // counters go on in battlefield order.
            let creatures: Vec<ObjectId> = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&object| {
                    let live = state.objects.get(object);
                    live.zone == Zone::Battlefield
                        && live.controller == ctx.controller
                        && crate::engine::object_has_type(state, object, CardType::Creature)
                })
                .collect();
            for object in creatures {
                if event::add_plus_one_counters(state, object, ctx.controller, 1).is_err() {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        ctx.source,
                    ));
                    return;
                }
            }
        }
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C2bOpV1,
    _path: Vec<u16>,
) -> Result<bool, String> {
    execute(op, &continuation.ctx, state);
    Ok(false)
}
