//! Target filters of the wave S1q cards, carried as
//! `StandardTargetV1::S1qV1`. `standard_cards_v1::legal_targets`,
//! `target_matches` and `StandardTargetV1::counts` ask them first.
//!
//! Cityscape Leveler's cast and attack trigger and Summon: Bahamut's first two
//! chapters: "destroy up to one target nonland permanent". Any player's
//! nonland permanent qualifies, the trigger's own source included (an
//! attacking Leveler or the Saga itself).

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};
use serde::{Deserialize, Serialize};

/// The filters of wave S1q.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1qTargetV1 {
    /// "Up to one target nonland permanent" (Cityscape Leveler, Summon:
    /// Bahamut).
    UpToOneNonlandPermanentV1,
}

fn owned(target: StandardTargetV1) -> Option<S1qTargetV1> {
    match target {
        StandardTargetV1::S1qV1(filter) => Some(filter),
        _ => None,
    }
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    Some(match owned(target)? {
        S1qTargetV1::UpToOneNonlandPermanentV1 => (1, 0),
    })
}

/// Whether `object` satisfies this module's filter `target`, or `None` for
/// every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    let filter = owned(target)?;
    let Some(live) = state.objects.try_get(object) else {
        return Some(false);
    };
    Some(match filter {
        S1qTargetV1::UpToOneNonlandPermanentV1 => {
            live.zone == Zone::Battlefield
                && !crate::engine::object_has_type(state, object, CardType::Land)
        }
    })
}

/// The legal choices for slot `prefix.len()` of this module's filter, or
/// `None` for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    let filter = owned(target)?;
    if !prefix.is_empty() {
        return Some(Vec::new());
    }
    Some(match filter {
        S1qTargetV1::UpToOneNonlandPermanentV1 => [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
            .filter(|&object| target_matches(target, object, state) == Some(true))
            .map(Target::Object)
            .collect(),
    })
}
