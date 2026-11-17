//! A choice between two additional costs of the eternal catalog (#715):
//! "As an additional cost to cast this spell, sacrifice a creature or
//! discard a card."
//!
//! The caster announces which additional cost they pay with the spell's other
//! choices, before targets and before any payment (601.2b), and the choice
//! holds for a cast for the mana cost, for a granted flashback and for a cast
//! without paying the mana cost. That is exactly where and how a mode is
//! chosen, so the choice is carried as the spell's two modes: mode 0 pays the
//! first printed cost and mode 1 the second. Both modes have the card's one
//! target shape and its one program, and a mode whose cost can't be paid is
//! not offered (`engine::viable_pending_spell_forms` quotes each).
//!
//! Each card's `additional_cost` stays its discard, which opens the cast's
//! discard stage; `components` then names what the chosen mode really pays.
//! Every table is keyed by a printed name that occurs only in the eternal
//! registry, so the Pauper, FDN and Standard catalogs never reach it.
//!
//! Bitter Triumph ("discard a card or pay 3 life") has the same printed shape
//! but keeps its earlier registration (the discard is its additional cost and
//! paying life an alternative cost of the same mana), because the Eternal
//! actor's printed contract for the reported mainboards describes that shape
//! (`eternal_actor_printed_v1`).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardDef, CostComponent, PermanentFilter, CARD_DEFS};
use crate::effect::EffectOp;

const BONE_SHARDS: &str = "Bone Shards";
const DEMAND_ANSWERS: &str = "Demand Answers";

const DISCARD_A_CARD: &[CostComponent] = &[CostComponent::DiscardCards(1)];
const SACRIFICE_A_CREATURE: &[CostComponent] = &[CostComponent::SacrificeControlled {
    count: 1,
    filter: PermanentFilter::Creature,
}];
const SACRIFICE_AN_ARTIFACT: &[CostComponent] = &[CostComponent::SacrificeControlled {
    count: 1,
    filter: PermanentFilter::Artifact,
}];

/// The two additional costs of `name`, in printed order, or `None` for a
/// card without such a choice.
fn costs(name: &str) -> Option<[&'static [CostComponent]; 2]> {
    match name {
        // "Sacrifice a creature or discard a card."
        BONE_SHARDS => Some([SACRIFICE_A_CREATURE, DISCARD_A_CARD]),
        // "Sacrifice an artifact or discard a card."
        DEMAND_ANSWERS => Some([SACRIFICE_AN_ARTIFACT, DISCARD_A_CARD]),
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

/// Whether `group` is one of `definition`'s two additional costs.
pub(crate) fn is_its_cost(definition: &CardDef, group: &[CostComponent]) -> bool {
    costs(definition.name).is_some_and(|both| both.iter().any(|cost| *cost == group))
}

/// The additional cost `mode` of `definition` pays, in the words a human
/// prompt shows ("sacrifice a creature", "discard a card"), or `None` for a
/// card without such a choice.
pub(crate) fn cost_words(definition: &CardDef, mode: u8) -> Option<&'static str> {
    let cost = components(definition, mode)?;
    if cost == SACRIFICE_A_CREATURE {
        Some("sacrifice a creature")
    } else if cost == SACRIFICE_AN_ARTIFACT {
        Some("sacrifice an artifact")
    } else if cost == DISCARD_A_CARD {
        Some("discard a card")
    } else {
        None
    }
}

/// The number of cards the chosen additional cost discards: `Some(None)`
/// when the chosen cost is not a discard, `Some(Some(1))` when it is, and
/// `None` for every other card.
///
/// An unchosen mode discards nothing yet, as `sacrifice` sacrifices nothing
/// for it: the cast chooses its mode before targets, and the discard stage
/// opens only after the targets and the sacrifice, so the stage always reads
/// a chosen mode. Before that, `engine::validate_pending_cast` accepts the
/// seeded state (`additional_cost_discarded` still `None`) whether this reads
/// a discard or not.
pub(crate) fn additional_discard_count(
    definition: &CardDef,
    mode: Option<u8>,
) -> Option<Option<u8>> {
    if !applies(definition) {
        return None;
    }
    let Some(chosen) = mode.and_then(|mode| components(definition, mode)) else {
        return Some(None);
    };
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

#[cfg(all(test, feature = "eternal-fixtures"))]
mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;

    fn def(name: &str) -> &'static CardDef {
        &CARD_DEFS[usize::from(card_id_by_name(name).unwrap())]
    }

    #[test]
    fn cost_words_name_each_cost_in_printed_order() {
        let shards = def(BONE_SHARDS);
        assert_eq!(cost_words(shards, 0), Some("sacrifice a creature"));
        assert_eq!(cost_words(shards, 1), Some("discard a card"));
        let demand = def(DEMAND_ANSWERS);
        assert_eq!(cost_words(demand, 0), Some("sacrifice an artifact"));
        assert_eq!(cost_words(demand, 1), Some("discard a card"));
        assert_eq!(cost_words(shards, 2), None);
        assert_eq!(cost_words(def("Bitter Triumph"), 0), None);
    }

    #[test]
    fn an_unchosen_mode_pays_nothing_yet() {
        for name in [BONE_SHARDS, DEMAND_ANSWERS] {
            let card = def(name);
            assert_eq!(additional_discard_count(card, None), Some(None), "{name}");
            assert_eq!(sacrifice(card, None), None, "{name}");
            assert_eq!(
                additional_discard_count(card, Some(0)),
                Some(None),
                "{name}"
            );
            assert!(sacrifice(card, Some(0)).is_some(), "{name}");
            assert_eq!(
                additional_discard_count(card, Some(1)),
                Some(Some(1)),
                "{name}"
            );
            assert_eq!(sacrifice(card, Some(1)), None, "{name}");
        }
        let triumph = def("Bitter Triumph");
        assert_eq!(additional_discard_count(triumph, None), None);
        assert!(!applies(triumph));
    }

    #[test]
    fn only_the_two_printed_costs_are_its_costs() {
        let shards = def(BONE_SHARDS);
        assert!(is_its_cost(shards, SACRIFICE_A_CREATURE));
        assert!(is_its_cost(shards, DISCARD_A_CARD));
        assert!(!is_its_cost(shards, SACRIFICE_AN_ARTIFACT));
        assert!(!is_its_cost(def("Bitter Triumph"), DISCARD_A_CARD));
    }
}
