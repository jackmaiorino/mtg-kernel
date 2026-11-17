//! Target filters of the wave C4a cards, carried as
//! `StandardTargetV1::C4aV1`. `standard_cards_v1::legal_targets`,
//! `target_matches`, `target_zone_allows` and `StandardTargetV1::counts` ask
//! them first.
//!
//! Displacer Kitten: "up to one target nonland permanent you control". The
//! Kitten itself is a legal choice. Lutri, the Spellchaser: "target instant
//! or sorcery spell you control", a spell on the stack.

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, StackItemKind, Target, Zone};
use serde::{Deserialize, Serialize};

/// The filters of wave C4a.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C4aTargetV1 {
    /// "Up to one target nonland permanent you control" (Displacer Kitten).
    UpToOneControlledNonlandPermanentV1,
    /// "Target instant or sorcery spell you control" (Lutri, the
    /// Spellchaser).
    ControlledInstantOrSorcerySpellV1,
}

fn owned(target: StandardTargetV1) -> Option<C4aTargetV1> {
    match target {
        StandardTargetV1::C4aV1(filter) => Some(filter),
        _ => None,
    }
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    Some(match owned(target)? {
        C4aTargetV1::UpToOneControlledNonlandPermanentV1 => (1, 0),
        C4aTargetV1::ControlledInstantOrSorcerySpellV1 => (1, 1),
    })
}

/// Whether an object target of this module's filter may be in `zone`.
pub(crate) fn zone_allows(target: StandardTargetV1, zone: Zone) -> bool {
    match owned(target) {
        Some(C4aTargetV1::UpToOneControlledNonlandPermanentV1) => zone == Zone::Battlefield,
        Some(C4aTargetV1::ControlledInstantOrSorcerySpellV1) => zone == Zone::Stack,
        None => false,
    }
}

/// Whether `object` is an instant or sorcery spell on the stack that
/// `controller` controls.
fn is_controlled_instant_or_sorcery_spell(
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> bool {
    state.stack.iter().any(|item| {
        item.kind == StackItemKind::Spell
            && item.source == object
            && item.controller == controller
            && (crate::engine::stack_spell_has_type(state, item, CardType::Instant)
                || crate::engine::stack_spell_has_type(state, item, CardType::Sorcery))
    })
}

/// Whether `object` satisfies this module's filter `target` for
/// `controller`, or `None` for every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    let filter = owned(target)?;
    let Some(live) = state.objects.try_get(object) else {
        return Some(false);
    };
    Some(match filter {
        C4aTargetV1::UpToOneControlledNonlandPermanentV1 => {
            live.zone == Zone::Battlefield
                && live.controller == controller
                && !crate::engine::object_has_type(state, object, CardType::Land)
        }
        C4aTargetV1::ControlledInstantOrSorcerySpellV1 => {
            live.zone == Zone::Stack
                && is_controlled_instant_or_sorcery_spell(controller, object, state)
        }
    })
}

/// The legal choices for slot `prefix.len()` of this module's filter, or
/// `None` for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    let filter = owned(target)?;
    if !prefix.is_empty() {
        return Some(Vec::new());
    }
    Some(match filter {
        C4aTargetV1::UpToOneControlledNonlandPermanentV1 => [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
            .filter(|&object| target_matches(target, controller, object, state) == Some(true))
            .map(Target::Object)
            .collect(),
        C4aTargetV1::ControlledInstantOrSorcerySpellV1 => state
            .stack
            .iter()
            .filter(|item| item.kind == StackItemKind::Spell)
            .map(|item| item.source)
            .filter(|&object| target_matches(target, controller, object, state) == Some(true))
            .map(Target::Object)
            .collect(),
    })
}
