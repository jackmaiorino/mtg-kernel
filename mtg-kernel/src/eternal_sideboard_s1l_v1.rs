//! Card programs for the eternal catalog's Legacy sideboard wave S1l
//! (`docs/design/eternal_formats_support_v1.md`): Triumph of Saint
//! Katherine, which with Lavinia, Azorius Renegade completes Beanstalk
//! Control without Yorion, both Azorius Tempo lists and Jeskai Tempo.
//!
//! `eternal_cards_v1` forwards its effect leaf and trigger here, wave P3g's
//! recipe tables fall through to `build_eternal_s1l_v1.rs`, and the engine
//! calls the hooks below only in `eternal-fixtures` builds. Its miracle
//! {1}{W} is wave E5b's draw trigger (`eternal_legacy_e5b_v1`). Every table
//! is keyed by a printed name that occurs only in the eternal registry.
//! Behavior follows the card's current Oracle text and rulings, checked
//! against its XMage source.
//!
//! Primitive added here: the top cards of a library put back in a random
//! order (`GameState::randomize_library_top_v1`).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::TargetSpec;
use crate::effect::{EffectOp, ExecCtx};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, ProposedEvent};
use crate::ids::ObjectId;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const TRIUMPH_OF_SAINT_KATHERINE: &str = "Triumph of Saint Katherine";

/// The library cards Praesidium Protectiva exiles with Triumph.
const PILE_LIBRARY_CARDS: usize = 6;

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::S1lV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1lOpV1 {
    /// Triumph of Saint Katherine's Praesidium Protectiva: exile it and the
    /// top six cards of its owner's library in a face-down pile, then
    /// shuffle the pile and put it back on top.
    TriumphPileV1,
}

fn s1l(op: S1lOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1lV1(op)))
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

// ---- Programs -----------------------------------------------------------------------

/// Triumph of Saint Katherine: "Praesidium Protectiva -- When this creature
/// is put into your graveyard from the battlefield, exile it and the top six
/// cards of your library in a face-down pile. If you do, shuffle that pile
/// and put it back on top of your library."
pub fn triumph_dies() -> EffectOp {
    s1l(S1lOpV1::TriumphPileV1)
}

// ---- Triggered abilities ------------------------------------------------------------

/// Praesidium Protectiva is a leaves-the-battlefield trigger (603.10a) that
/// works from the graveyard, as Academy Rector's does.
const TRIUMPH_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::LeftBattlefieldToGraveyard,
    home_zone: Zone::Graveyard,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: triumph_dies,
}];

/// The printed triggers of this module's cards.
fn own_triggers(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        TRIUMPH_OF_SAINT_KATHERINE => &TRIUMPH_TRIGGERS,
        _ => &[],
    }
}

/// Triggered abilities of this module's cards, then of wave S1m's, the
/// tail of the trigger tables.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match own_triggers(name) {
        [] => crate::eternal_sideboard_s1m_v1::triggers_for(name),
        own => own,
    }
}

/// Target specs of this module's triggered abilities: Praesidium Protectiva
/// targets nothing.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    own_triggers(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        .then_some(TargetSpec::None)
}

/// Whether a leave trigger of `name` with `effect` requires that the card's
/// owner controlled it as it left: "put into your graveyard from the
/// battlefield", and a card goes to its owner's graveyard, so a stolen
/// Triumph that dies doesn't trigger.
pub(crate) fn requires_owner_controller(name: &str, effect: &EffectOp) -> bool {
    cfg!(feature = "eternal-fixtures")
        && name == TRIUMPH_OF_SAINT_KATHERINE
        && *effect == triumph_dies()
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves.
pub(crate) fn execute(op: &S1lOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        S1lOpV1::TriumphPileV1 => triumph_pile(ctx, state),
    }
}

/// Praesidium Protectiva's resolution. If Triumph isn't in its owner's
/// graveyard as the incarnation that died, or the library has fewer than
/// six cards, nothing happens (its 2024-01-12 ruling). Otherwise Triumph is
/// exiled and put on top of the library, and the top seven cards are
/// randomized: the six library cards stay in place as objects, since they
/// would be face down in exile only during this resolution, seen by no one.
fn triumph_pile(ctx: &ExecCtx, state: &mut GameState) {
    let Some(card) = crate::eternal_premodern_p2a_v1::graveyard_ability_source(ctx, state) else {
        return;
    };
    let owner = state.objects.get(card).owner;
    if state.players[owner.index()].library.len() < PILE_LIBRARY_CARDS {
        return;
    }
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
    if state.objects.get(card).zone != Zone::Exile {
        return;
    }
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Library));
    if state.objects.get(card).zone != Zone::Library
        || state.players[owner.index()].library.first() != Some(&card)
    {
        halt(state, ctx.source);
        return;
    }
    if state
        .randomize_library_top_v1(owner, PILE_LIBRARY_CARDS + 1)
        .is_err()
    {
        halt(state, ctx.source);
    }
}
