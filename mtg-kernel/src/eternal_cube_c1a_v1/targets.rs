//! Target filters of the wave C1a cards, carried as
//! `StandardTargetV1::C1aV1`. `standard_cards_v1::legal_targets`,
//! `target_matches`, `target_zone_allows` and `StandardTargetV1::counts` ask
//! them first.
//!
//! Mystic Confluence's "choose three" is one printed mode per multiset of
//! its three modes. A mode's filter takes one target per chosen "counter
//! target spell" (a spell on the stack) and then one per chosen "return
//! target creature" (a creature on the battlefield), in that printed order.
//! Each slot is the target of its own instance of a mode, so two slots may
//! name the same spell or creature (700.2d, 115.3).

use crate::card_def::CardType;
use crate::ids::ObjectId;
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, StackItemKind, Target, Zone};
use serde::{Deserialize, Serialize};

/// The filters of wave C1a.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C1aTargetV1 {
    /// Mystic Confluence: `spells` target spells, then `creatures` target
    /// creatures.
    MysticSlotsV1 { spells: u8, creatures: u8 },
}

fn slots(target: StandardTargetV1) -> Option<(u8, u8)> {
    match target {
        StandardTargetV1::C1aV1(C1aTargetV1::MysticSlotsV1 { spells, creatures }) => {
            Some((spells, creatures))
        }
        _ => None,
    }
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    slots(target).map(|(spells, creatures)| (spells + creatures, spells + creatures))
}

/// Whether an object target of this module's filter may be in `zone`.
pub(crate) fn zone_allows(target: StandardTargetV1, zone: Zone) -> bool {
    slots(target).is_some_and(|(spells, creatures)| match zone {
        Zone::Stack => spells > 0,
        Zone::Battlefield => creatures > 0,
        _ => false,
    })
}

/// Whether `object` is a spell on the stack.
fn is_spell(state: &GameState, object: ObjectId) -> bool {
    state
        .stack
        .iter()
        .any(|item| item.source == object && item.kind == StackItemKind::Spell)
}

/// Whether `object` is a creature on the battlefield.
fn is_creature(state: &GameState, object: ObjectId) -> bool {
    state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Battlefield)
        && crate::engine::object_has_type(state, object, CardType::Creature)
}

/// Whether `object` satisfies the filter of one of this module's slots (its
/// zone settles which), or `None` for every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    let (spells, creatures) = slots(target)?;
    Some((spells > 0 && is_spell(state, object)) || (creatures > 0 && is_creature(state, object)))
}

/// The legal choices for slot `prefix.len()` of this module's filter, or
/// `None` for every other filter. The spell being cast is not a legal target
/// of its own modes.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    _controller: crate::ids::PlayerId,
    source: Option<(ObjectId, u32)>,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    let (spells, creatures) = slots(target)?;
    let slot = prefix.len();
    if slot < usize::from(spells) {
        let announcing = state
            .engine
            .pending_cast
            .as_ref()
            .map(|pending| pending.spell);
        return Some(
            state
                .stack
                .iter()
                .filter(|item| item.kind == StackItemKind::Spell)
                .map(|item| item.source)
                .filter(|&spell| Some(spell) != announcing)
                .filter(|&spell| source.is_none_or(|(id, _)| id != spell))
                .map(Target::Object)
                .collect(),
        );
    }
    if slot < usize::from(spells) + usize::from(creatures) {
        return Some(
            [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1]
                .into_iter()
                .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
                .filter(|&object| is_creature(state, object))
                .map(Target::Object)
                .collect(),
        );
    }
    Some(Vec::new())
}
