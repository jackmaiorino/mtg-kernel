//! A choice between two additional costs of the eternal catalog (#715):
//! "As an additional cost to cast this spell, sacrifice a creature or
//! discard a card."
//!
//! The caster announces which additional cost they pay with the spell's other
//! choices, before targets and before any payment (601.2b), and the choice
//! holds however the spell is cast: for its mana cost, for a granted
//! flashback, or without paying its mana cost. That is exactly where and how
//! a mode is chosen, so the choice is carried as the spell's two modes: mode
//! 0 pays the first printed cost and mode 1 the second. Both modes have the
//! card's one target shape and its one program, and a mode whose cost can't
//! be paid is not offered (`engine::viable_pending_spell_forms` quotes each).
//!
//! Each card's `additional_cost` stays its discard, which opens the cast's
//! discard stage; `components` then names what the chosen mode really pays.
//! Every table is keyed by a printed name that occurs only in the eternal
//! registry, so the Pauper, FDN and Standard catalogs never reach it.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardDef, CostComponent, PermanentFilter, CARD_DEFS};
use crate::effect::EffectOp;

const BONE_SHARDS: &str = "Bone Shards";
const DEMAND_ANSWERS: &str = "Demand Answers";
const BITTER_TRIUMPH: &str = "Bitter Triumph";

const DISCARD_A_CARD: &[CostComponent] = &[CostComponent::DiscardCards(1)];
const SACRIFICE_A_CREATURE: &[CostComponent] = &[CostComponent::SacrificeControlled {
    count: 1,
    filter: PermanentFilter::Creature,
}];
const SACRIFICE_AN_ARTIFACT: &[CostComponent] = &[CostComponent::SacrificeControlled {
    count: 1,
    filter: PermanentFilter::Artifact,
}];
const PAY_THREE_LIFE: &[CostComponent] = &[CostComponent::PayLife(3)];

/// The two additional costs of `name`, in printed order, or `None` for a
/// card without such a choice.
fn costs(name: &str) -> Option<[&'static [CostComponent]; 2]> {
    match name {
        // "Sacrifice a creature or discard a card."
        BONE_SHARDS => Some([SACRIFICE_A_CREATURE, DISCARD_A_CARD]),
        // "Sacrifice an artifact or discard a card."
        DEMAND_ANSWERS => Some([SACRIFICE_AN_ARTIFACT, DISCARD_A_CARD]),
        // "Discard a card or pay 3 life."
        BITTER_TRIUMPH => Some([DISCARD_A_CARD, PAY_THREE_LIFE]),
        _ => None,
    }
}

/// Whether `definition` has a choice between two additional costs.
pub(crate) fn applies(definition: &CardDef) -> bool {
    costs(definition.name).is_some()
}

/// The additional cost `mode` of `definition` pays, or `None` for a card
/// without such a choice (or a mode it does not have).
pub(crate) fn components(definition: &CardDef, mode: u8) -> Option<&'static [CostComponent]> {
    costs(definition.name)?.get(usize::from(mode)).copied()
}

/// The number of cards the chosen additional cost discards: `Some(None)`
/// when the chosen cost is not a discard, `Some(Some(1))` when it is, and
/// `None` for every other card. An unchosen mode reads as mode 0, as the
/// cost quotes do.
pub(crate) fn additional_discard_count(
    definition: &CardDef,
    mode: Option<u8>,
) -> Option<Option<u8>> {
    let chosen = components(definition, mode.unwrap_or(0))?;
    Some(chosen.iter().find_map(|component| match component {
        CostComponent::DiscardCards(count) => Some(*count),
        _ => None,
    }))
}

/// The permanent the chosen additional cost sacrifices, if it is a
/// sacrifice. `None` for every other card and for an unchosen mode.
pub(crate) fn sacrifice(definition: &CardDef, mode: Option<u8>) -> Option<(u8, PermanentFilter)> {
    components(definition, mode?)?
        .iter()
        .find_map(|component| match component {
            CostComponent::SacrificeControlled { count, filter } => Some((*count, *filter)),
            _ => None,
        })
}

/// The registered card's own program: both modes of a choice card resolve
/// the same way, so the second mode borrows the first's.
fn own_program(name: &str) -> EffectOp {
    CARD_DEFS
        .iter()
        .find(|definition| definition.name == name)
        .and_then(|definition| (definition.spell_effect)())
        .expect("a card with a choice of additional costs is a registered spell with a program")
}

/// Bone Shards' second mode (discard a card): "Destroy target creature or
/// planeswalker."
pub fn bone_shards_second_cost() -> EffectOp {
    own_program(BONE_SHARDS)
}

/// Demand Answers' second mode (discard a card): "Draw two cards."
pub fn demand_answers_second_cost() -> EffectOp {
    own_program(DEMAND_ANSWERS)
}

/// Bitter Triumph's second mode (pay 3 life): "Destroy target creature or
/// planeswalker."
pub fn bitter_triumph_second_cost() -> EffectOp {
    own_program(BITTER_TRIUMPH)
}
