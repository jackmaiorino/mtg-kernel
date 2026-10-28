//! Target filters of the wave E7b cards, carried as
//! `StandardTargetV1` variants. `standard_cards_v1::legal_targets` and
//! `target_matches` ask these first.

use crate::card_def::{CardType, Keywords, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, StackItemKind, Target, Zone};

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    use StandardTargetV1 as S;
    matches!(
        target,
        S::NonlandPermanentManaValueAtMostV1(_)
            | S::OpponentPermanentV1
            | S::AnyNumberOfSpellsV1
            | S::UpToOneCreatureOrPlaneswalkerSpellV1
            | S::LandCardInAGraveyardV1
            | S::InstantOrSorceryCardInAGraveyardV1
            | S::UpToOneLandCardInOwnGraveyardV1
            | S::LandCardInOwnGraveyardV1
            | S::UpToOneCreatureWithTrampleOrHasteV1
    )
}

/// Whether this module's filter `target` takes graveyard cards.
pub(crate) fn in_graveyard(target: StandardTargetV1) -> bool {
    use StandardTargetV1 as S;
    matches!(
        target,
        S::LandCardInAGraveyardV1
            | S::InstantOrSorceryCardInAGraveyardV1
            | S::UpToOneLandCardInOwnGraveyardV1
            | S::LandCardInOwnGraveyardV1
    )
}

/// Whether this module's filter `target` takes spells.
pub(crate) fn on_stack(target: StandardTargetV1) -> bool {
    matches!(
        target,
        StandardTargetV1::AnyNumberOfSpellsV1
            | StandardTargetV1::UpToOneCreatureOrPlaneswalkerSpellV1
    )
}

/// Whether `object` satisfies this module's filter `target` for
/// `controller`, or `None` for every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    use StandardTargetV1 as S;
    if !owns(target) {
        return None;
    }
    let Some(live) = state.objects.try_get(object) else {
        return Some(false);
    };
    let def = CARD_DEFS.get(live.card_def as usize)?;
    let card =
        live.zone == Zone::Graveyard && !live.v4.is_token && live.spell_copy_origin.is_none();
    let has = |card_type| crate::engine::object_has_type(state, object, card_type);
    let permanent = live.zone == Zone::Battlefield;
    Some(match target {
        S::NonlandPermanentManaValueAtMostV1(maximum) => {
            permanent
                && !has(CardType::Land)
                && crate::engine::object_mana_value(state, object) <= u16::from(maximum)
        }
        S::OpponentPermanentV1 => permanent && live.controller != controller,
        S::AnyNumberOfSpellsV1 => spell_matches(target, object, state),
        S::UpToOneCreatureOrPlaneswalkerSpellV1 => spell_matches(target, object, state),
        S::LandCardInAGraveyardV1 => card && def.has_type(CardType::Land),
        S::InstantOrSorceryCardInAGraveyardV1 => {
            card && (def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery))
        }
        S::UpToOneLandCardInOwnGraveyardV1 | S::LandCardInOwnGraveyardV1 => {
            card && live.owner == controller && def.has_type(CardType::Land)
        }
        S::UpToOneCreatureWithTrampleOrHasteV1 => {
            permanent
                && has(CardType::Creature)
                && (crate::engine::has_effective_keyword(state, object, Keywords::TRAMPLE)
                    || crate::engine::has_effective_keyword(state, object, Keywords::HASTE))
        }
        _ => return None,
    })
}

/// Whether the spell `source` on the stack (not the one being announced)
/// matches a spell filter.
fn spell_matches(target: StandardTargetV1, source: ObjectId, state: &GameState) -> bool {
    let announcing = state
        .engine
        .pending_cast
        .as_ref()
        .map(|pending| pending.spell);
    if Some(source) == announcing {
        return false;
    }
    let Some(item) = state
        .stack
        .iter()
        .find(|item| item.kind == StackItemKind::Spell && item.source == source)
    else {
        return false;
    };
    match target {
        StandardTargetV1::AnyNumberOfSpellsV1 => true,
        StandardTargetV1::UpToOneCreatureOrPlaneswalkerSpellV1 => {
            crate::engine::stack_spell_has_type(state, item, CardType::Creature)
                || crate::engine::stack_spell_has_type(state, item, CardType::Planeswalker)
        }
        _ => false,
    }
}

/// The legal choices for this module's filter `target` after the already
/// chosen `prefix`, or `None` for every other filter. Multi-target filters
/// take distinct targets (115.3).
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    source: Option<(ObjectId, u32)>,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    if !owns(target) {
        return None;
    }
    let candidates: Vec<ObjectId> = if on_stack(target) {
        state
            .stack
            .iter()
            .filter(|item| item.kind == StackItemKind::Spell)
            .map(|item| item.source)
            .filter(|&spell| source.is_none_or(|(id, _)| id != spell))
            .collect()
    } else if in_graveyard(target) {
        [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].graveyard.iter().copied())
            .collect()
    } else {
        [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
            .collect()
    };
    Some(
        candidates
            .into_iter()
            .filter(|&id| target_matches(target, controller, id, state) == Some(true))
            .map(Target::Object)
            .filter(|candidate| !prefix.contains(candidate))
            .collect(),
    )
}
