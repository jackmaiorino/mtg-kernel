//! Target filters of the Premodern wave P2a cards, carried as
//! `StandardTargetV1` variants. `standard_cards_v1::legal_targets` and
//! `target_matches` ask these first.

use crate::ids::PlayerId;
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// The most graveyard cards Gaea's Blessing targets.
const MOST_CARDS: usize = 3;

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    matches!(
        target,
        StandardTargetV1::PlayerThenUpToThreeOfTheirGraveyardCardsV1
    )
}

/// How many targets this module's filter takes, and how many at least:
/// the player, then up to three of their graveyard cards.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    owns(target).then_some((1 + MOST_CARDS as u8, 1))
}

/// Whether `target` takes a player in slot `slot`.
pub(crate) fn player_slot(target: StandardTargetV1, slot: usize) -> bool {
    owns(target) && slot == 0
}

/// The legal choices for this module's filter `target` after the chosen
/// `prefix`, or `None` for every other filter: any player, then the cards in
/// that player's graveyard not chosen yet.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    if !owns(target) {
        return None;
    }
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

/// Whether `object` satisfies this module's filter `target` on its own:
/// never, since each slot depends on the player chosen first.
pub(crate) fn target_matches(target: StandardTargetV1) -> Option<bool> {
    owns(target).then_some(false)
}
