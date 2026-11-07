//! Target filters of the wave S1b cards, carried as `StandardTargetV1`
//! variants. `standard_cards_v1::legal_targets` and `target_matches` ask
//! these first.
//!
//! Pyrokinesis's damage is divided as it is announced (601.2d), like Fury's:
//! four picks among creatures, one point each, a creature picked again
//! taking another point.

use crate::card_def::{CardType, Subtype, Supertype, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    use StandardTargetV1 as S;
    matches!(
        target,
        S::DividedFourDamageCreaturesV1
            | S::OpponentCreatureOrEnchantmentV1
            | S::NonbasicLandCardInAGraveyardV1
            | S::ControlledCreatureOrVehicleV1
    )
}

/// Whether this module's filter `target` takes graveyard cards.
pub(crate) fn in_graveyard(target: StandardTargetV1) -> bool {
    target == StandardTargetV1::NonbasicLandCardInAGraveyardV1
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    use StandardTargetV1 as S;
    Some(match target {
        S::DividedFourDamageCreaturesV1 => (4, 4),
        S::OpponentCreatureOrEnchantmentV1
        | S::NonbasicLandCardInAGraveyardV1
        | S::ControlledCreatureOrVehicleV1 => (1, 1),
        _ => return None,
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
    use StandardTargetV1 as S;
    if !owns(target) {
        return None;
    }
    let Some(live) = state.objects.try_get(object) else {
        return Some(false);
    };
    let has = |card_type| crate::engine::object_has_type(state, object, card_type);
    Some(match target {
        S::DividedFourDamageCreaturesV1 => {
            live.zone == Zone::Battlefield && has(CardType::Creature)
        }
        S::OpponentCreatureOrEnchantmentV1 => {
            live.zone == Zone::Battlefield
                && live.controller != controller
                && (has(CardType::Creature) || has(CardType::Enchantment))
        }
        S::ControlledCreatureOrVehicleV1 => {
            live.zone == Zone::Battlefield
                && live.controller == controller
                && (has(CardType::Creature)
                    || crate::engine::has_effective_subtype(state, object, Subtype::Vehicle))
        }
        S::NonbasicLandCardInAGraveyardV1 => {
            let def = &CARD_DEFS[usize::from(live.card_def)];
            live.zone == Zone::Graveyard
                && !live.v4.is_token
                && live.spell_copy_origin.is_none()
                && !(def.has_type(CardType::Land) && def.supertypes.contains(&Supertype::Basic))
        }
        _ => false,
    })
}

/// The legal choices for this module's filter `target` after the already
/// chosen `prefix`, or `None` for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    use StandardTargetV1 as S;
    if !owns(target) {
        return None;
    }
    let (maximum, _) = counts(target)?;
    if prefix.len() >= usize::from(maximum) {
        return Some(Vec::new());
    }
    let candidates: Vec<ObjectId> = if target == S::NonbasicLandCardInAGraveyardV1 {
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
            .collect(),
    )
}

/// The division of Pyrokinesis's damage: each target with the damage it is
/// dealt, in first-pick order; each pick is one point.
pub(crate) fn division(picks: &[Target]) -> Vec<(Target, i32)> {
    let mut out: Vec<(Target, i32)> = Vec::new();
    for &pick in picks {
        match out.iter_mut().find(|(target, _)| *target == pick) {
            Some((_, amount)) => *amount = amount.saturating_add(1),
            None => out.push((pick, 1)),
        }
    }
    out
}
