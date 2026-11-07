//! Target filters of the Premodern wave P2c cards, carried as
//! `StandardTargetV1` variants. `standard_cards_v1::legal_targets`, `counts`
//! and `target_matches` ask these.

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// The most graveyard cards Krosan Reclamation targets.
const MOST_CARDS: usize = 2;

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    matches!(
        target,
        StandardTargetV1::PlayerThenUpToTwoOfTheirGraveyardCardsV1
            | StandardTargetV1::TwoLandsV1
    )
}

/// How many targets this module's filter takes, and how many at least:
/// a player, then up to two of their graveyard cards (Krosan Reclamation),
/// or exactly two lands (Petradon).
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    match target {
        StandardTargetV1::PlayerThenUpToTwoOfTheirGraveyardCardsV1 => {
            Some((1 + MOST_CARDS as u8, 1))
        }
        StandardTargetV1::TwoLandsV1 => Some((2, 2)),
        _ => None,
    }
}

/// The legal choices for this module's filter `target` after the chosen
/// `prefix`, or `None` for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    match target {
        StandardTargetV1::PlayerThenUpToTwoOfTheirGraveyardCardsV1 => {
            let Some(first) = prefix.first().copied() else {
                return Some([PlayerId::P0, PlayerId::P1].map(Target::Player).to_vec());
            };
            let Target::Player(player) = first else {
                return Some(Vec::new());
            };
            if prefix.len() > MOST_CARDS {
                return Some(Vec::new());
            }
            Some(
                state.players[player.index()]
                    .graveyard
                    .iter()
                    .copied()
                    .filter(|&card| {
                        let live = state.objects.get(card);
                        live.zone == Zone::Graveyard && live.owner == player && !live.v4.is_token
                    })
                    .map(Target::Object)
                    .filter(|candidate| !prefix.contains(candidate))
                    .collect(),
            )
        }
        StandardTargetV1::TwoLandsV1 => {
            if prefix.len() >= 2 {
                return Some(Vec::new());
            }
            Some(
                [PlayerId::P0, PlayerId::P1]
                    .iter()
                    .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
                    .filter(|&object| is_land_permanent(state, object))
                    .map(Target::Object)
                    .filter(|candidate| !prefix.contains(candidate))
                    .collect(),
            )
        }
        _ => None,
    }
}

fn is_land_permanent(state: &GameState, object: ObjectId) -> bool {
    state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Battlefield)
        && crate::engine::object_has_type(state, object, CardType::Land)
}

/// Whether `object` satisfies this module's filter `target` on its own:
/// Reclamation's slots depend on the player chosen first, so never; any
/// land permanent for Petradon.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    match target {
        StandardTargetV1::PlayerThenUpToTwoOfTheirGraveyardCardsV1 => Some(false),
        StandardTargetV1::TwoLandsV1 => Some(is_land_permanent(state, object)),
        _ => None,
    }
}
