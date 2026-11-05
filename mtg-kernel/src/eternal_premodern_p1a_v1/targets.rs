//! Target filters of the wave P1a cards, carried as `StandardTargetV1`
//! variants. `standard_cards_v1::legal_targets` and `target_matches` ask
//! these first.
//!
//! Fire's divided damage is announced with its targets as wave E9a's Fury
//! announces its own (601.2d): each of its two picks assigns one point of
//! damage, so a target picked twice is one target dealt two.

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    use StandardTargetV1 as S;
    matches!(
        target,
        S::DividedTwoDamageAnyTargetsV1 | S::CreatureManaValueAtMostV1(_) | S::OpponentArtifactV1
    )
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    use StandardTargetV1 as S;
    Some(match target {
        S::DividedTwoDamageAnyTargetsV1 => (2, 2),
        S::CreatureManaValueAtMostV1(_) | S::OpponentArtifactV1 => (1, 1),
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
    if live.zone != Zone::Battlefield {
        return Some(false);
    }
    let has = |card_type| crate::engine::object_has_type(state, object, card_type);
    Some(match target {
        S::DividedTwoDamageAnyTargetsV1 => has(CardType::Creature) || has(CardType::Planeswalker),
        S::CreatureManaValueAtMostV1(maximum) => {
            has(CardType::Creature)
                && crate::engine::object_mana_value(state, object) <= u16::from(maximum)
        }
        S::OpponentArtifactV1 => live.controller != controller && has(CardType::Artifact),
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
    if !owns(target) {
        return None;
    }
    let (maximum, _) = counts(target)?;
    if prefix.len() >= usize::from(maximum) {
        return Some(Vec::new());
    }
    let mut candidates = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .filter(|&id| target_matches(target, controller, id, state) == Some(true))
        .map(Target::Object)
        .collect::<Vec<_>>();
    if target == StandardTargetV1::DividedTwoDamageAnyTargetsV1 {
        candidates.extend([Target::Player(PlayerId::P0), Target::Player(PlayerId::P1)]);
    }
    Some(candidates)
}
