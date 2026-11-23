//! Target filters of Vintage Cube wave C9c, carried as
//! `StandardTargetV1::C9cV1`. `standard_cards_v1::legal_targets`,
//! `target_matches`, `target_zone_allows`, and `StandardTargetV1::counts` ask
//! these before falling through to their built-in match.

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};
use serde::{Deserialize, Serialize};

/// Target filters owned by Vintage Cube wave C9c.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C9cTargetV1 {
    /// Elspeth, Storm Slayer (-3): "target creature an opponent controls with
    /// mana value 3 or greater".
    OpponentCreatureManaValueThreeOrGreaterV1,
    /// Nissa, Who Shakes the World (+1): "up to one target noncreature land
    /// you control".
    UpToOneControlledNoncreatureLandV1,
}

fn owned(target: StandardTargetV1) -> Option<C9cTargetV1> {
    match target {
        StandardTargetV1::C9cV1(filter) => Some(filter),
        _ => None,
    }
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    Some(match owned(target)? {
        C9cTargetV1::OpponentCreatureManaValueThreeOrGreaterV1 => (1, 1),
        C9cTargetV1::UpToOneControlledNoncreatureLandV1 => (1, 0),
    })
}

/// Whether an object target of this module's filter may be in `zone`.
pub(crate) fn zone_allows(target: StandardTargetV1, zone: Zone) -> bool {
    owned(target).is_some() && zone == Zone::Battlefield
}

/// Whether `object` satisfies this module's filter `target` for `controller`.
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
    if live.zone != Zone::Battlefield {
        return Some(false);
    }
    Some(match filter {
        C9cTargetV1::OpponentCreatureManaValueThreeOrGreaterV1 => {
            live.controller != controller
                && crate::engine::object_has_type(state, object, CardType::Creature)
                && crate::engine::object_mana_value(state, object) >= 3
        }
        C9cTargetV1::UpToOneControlledNoncreatureLandV1 => {
            live.controller == controller
                && crate::engine::object_has_type(state, object, CardType::Land)
                && !crate::engine::object_has_type(state, object, CardType::Creature)
        }
    })
}

/// Legal choices for slot `prefix.len()` of this module's filter, or `None`
/// for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    owned(target)?;
    if !prefix.is_empty() {
        return Some(Vec::new());
    }
    Some(
        [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
            .filter(|&object| target_matches(target, controller, object, state) == Some(true))
            .map(Target::Object)
            .collect(),
    )
}
