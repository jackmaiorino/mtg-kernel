//! Target filters of the wave E8a cards, carried as `StandardTargetV1`
//! variants. `standard_cards_v1::legal_targets` and `target_matches` ask
//! these first. Every filter takes battlefield permanents, except Cloak and
//! Dagger's first slot, which takes an opponent.

use crate::card_def::{CardType, Supertype};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    use StandardTargetV1 as S;
    matches!(
        target,
        S::AnotherPermanentV1
            | S::UpToOneArtifactOrEnchantmentV1
            | S::UpToOneArtifactCreatureOrEnchantmentV1
            | S::ArtifactOrCreatureV1
            | S::UpToOneNonbasicLandV1
            | S::UpToOneOpponentNonlandNontokenManaValueAtMostV1(_)
            | S::OpponentThenUpToOneTheirCreatureV1
            | S::UpToOneOpponentCreatureOrPlaneswalkerV1
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
    if live.zone != Zone::Battlefield {
        return Some(false);
    }
    let has = |card_type| crate::engine::object_has_type(state, object, card_type);
    let opponent = live.controller != controller;
    Some(match target {
        S::AnotherPermanentV1 => true,
        S::UpToOneArtifactOrEnchantmentV1 => has(CardType::Artifact) || has(CardType::Enchantment),
        S::UpToOneArtifactCreatureOrEnchantmentV1 => {
            has(CardType::Artifact) || has(CardType::Creature) || has(CardType::Enchantment)
        }
        S::ArtifactOrCreatureV1 => has(CardType::Artifact) || has(CardType::Creature),
        S::UpToOneNonbasicLandV1 => {
            has(CardType::Land)
                && !crate::engine::effective_supertypes(state, object).contains(&Supertype::Basic)
        }
        S::UpToOneOpponentNonlandNontokenManaValueAtMostV1(maximum) => {
            opponent
                && !has(CardType::Land)
                && !live.v4.is_token
                && crate::engine::object_mana_value(state, object) <= u16::from(maximum)
        }
        S::OpponentThenUpToOneTheirCreatureV1 => opponent && has(CardType::Creature),
        S::UpToOneOpponentCreatureOrPlaneswalkerV1 => {
            opponent && (has(CardType::Creature) || has(CardType::Planeswalker))
        }
        _ => return None,
    })
}

/// The legal choices for this module's filter `target` after the already
/// chosen `prefix`, or `None` for every other filter. `source` is the
/// spell or ability's own object, which "another" excludes.
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
    let battlefield = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .filter(|&id| state.objects.get(id).zone == Zone::Battlefield);
    if target == StandardTargetV1::OpponentThenUpToOneTheirCreatureV1 {
        // "Target opponent and up to one target creature they control."
        return Some(match prefix {
            [] => vec![Target::Player(controller.opponent())],
            [Target::Player(opponent)] => battlefield
                .filter(|&id| {
                    state.objects.get(id).controller == *opponent
                        && crate::engine::object_has_type(state, id, CardType::Creature)
                })
                .map(Target::Object)
                .collect(),
            _ => Vec::new(),
        });
    }
    if !prefix.is_empty() {
        return Some(Vec::new());
    }
    Some(
        battlefield
            .filter(|&id| {
                target != StandardTargetV1::AnotherPermanentV1
                    || source.is_none_or(|(own, _)| own != id)
            })
            .filter(|&id| target_matches(target, controller, id, state) == Some(true))
            .map(Target::Object)
            .collect(),
    )
}
