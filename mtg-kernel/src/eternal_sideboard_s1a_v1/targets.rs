//! Target filters of the sideboard wave S1a cards, carried as
//! `StandardTargetV1` variants. `standard_cards_v1::legal_targets` and
//! `target_matches` ask these first. Every filter takes battlefield
//! permanents except Swan Song's, which takes spells on the stack.

use crate::card_def::{CardType, Supertype, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, StackItem, Target, Zone};

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    use StandardTargetV1 as S;
    matches!(
        target,
        S::NoncreatureArtifactOrEnchantmentV1
            | S::BlackOrRedPermanentV1
            | S::CreaturePowerOrToughnessAtMostOneV1
            | S::UpToTwoArtifactsOrEnchantmentsV1
            | S::EnchantmentInstantOrSorcerySpellV1
            | S::NonbasicLandCardInAGraveyardV1
    )
}

/// Whether this module's filter `target` takes graveyard cards.
pub(crate) fn in_graveyard(target: StandardTargetV1) -> bool {
    target == StandardTargetV1::NonbasicLandCardInAGraveyardV1
}

/// Whether this module's filter `target` takes spells on the stack.
pub(crate) fn on_stack(target: StandardTargetV1) -> bool {
    target == StandardTargetV1::EnchantmentInstantOrSorcerySpellV1
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    use StandardTargetV1 as S;
    Some(match target {
        S::UpToTwoArtifactsOrEnchantmentsV1 => (2, 0),
        S::NoncreatureArtifactOrEnchantmentV1
        | S::BlackOrRedPermanentV1
        | S::CreaturePowerOrToughnessAtMostOneV1
        | S::EnchantmentInstantOrSorcerySpellV1
        | S::NonbasicLandCardInAGraveyardV1 => (1, 1),
        _ => return None,
    })
}

/// Whether the spell `item` on the stack is an enchantment, instant or
/// sorcery spell (Swan Song).
pub(crate) fn spell_matches(state: &GameState, item: &StackItem) -> bool {
    [CardType::Enchantment, CardType::Instant, CardType::Sorcery]
        .into_iter()
        .any(|card_type| crate::engine::stack_spell_has_type(state, item, card_type))
}

/// Whether the battlefield permanent `object` satisfies this module's
/// permanent filter `target`, or `None` for every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    _controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    use StandardTargetV1 as S;
    if !owns(target) || on_stack(target) {
        return None;
    }
    let Some(live) = state.objects.try_get(object) else {
        return Some(false);
    };
    if target == S::NonbasicLandCardInAGraveyardV1 {
        let def = CARD_DEFS.get(usize::from(live.card_def));
        return Some(
            live.zone == Zone::Graveyard
                && !live.v4.is_token
                && def.is_some_and(|def| {
                    !(def.has_type(CardType::Land) && def.supertypes.contains(&Supertype::Basic))
                }),
        );
    }
    if live.zone != Zone::Battlefield {
        return Some(false);
    }
    let has = |card_type| crate::engine::object_has_type(state, object, card_type);
    Some(match target {
        S::NoncreatureArtifactOrEnchantmentV1 => {
            (has(CardType::Artifact) || has(CardType::Enchantment)) && !has(CardType::Creature)
        }
        S::BlackOrRedPermanentV1 => {
            crate::engine::object_color_mask(state, object)
                & (crate::card_def::mana_color_mask(ManaColor::B)
                    | crate::card_def::mana_color_mask(ManaColor::R))
                != 0
        }
        S::CreaturePowerOrToughnessAtMostOneV1 => {
            has(CardType::Creature)
                && (crate::engine::effective_power(state, object) <= 1
                    || crate::engine::effective_toughness(state, object) <= 1)
        }
        S::UpToTwoArtifactsOrEnchantmentsV1 => {
            has(CardType::Artifact) || has(CardType::Enchantment)
        }
        _ => return None,
    })
}

/// The legal choices for this module's permanent filter `target` after the
/// already chosen `prefix`, or `None` for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    if !owns(target) || on_stack(target) {
        return None;
    }
    let limit = counts(target).map_or(1, |(most, _)| usize::from(most));
    if prefix.len() >= limit {
        return Some(Vec::new());
    }
    if in_graveyard(target) {
        return Some(
            [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .flat_map(|player| state.players[player.index()].graveyard.iter().copied())
                .filter(|&card| target_matches(target, controller, card, state) == Some(true))
                .map(Target::Object)
                .collect(),
        );
    }
    Some(
        super::battlefield(state)
            .filter(|&object| target_matches(target, controller, object, state) == Some(true))
            .map(Target::Object)
            .filter(|candidate| !prefix.contains(candidate))
            .collect(),
    )
}
