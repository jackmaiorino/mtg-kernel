//! Target filters of the wave C2c cards, carried as
//! `StandardTargetV1::C2cV1`. `standard_cards_v1::legal_targets`,
//! `target_matches`, `target_zone_allows` and `StandardTargetV1::counts` ask
//! them first.
//!
//! Candelabra of Tawnos: "{X}, {T}: Untap X target lands." The engine has no
//! free X for an activation, so the card has one ability per X and ability `n`
//! takes exactly `n` distinct target lands, of either player, tapped or not.
//! The lands are named in increasing object order, so a set of lands has one
//! announcement and an ability whose X exceeds the lands it could target has
//! no complete announcement (it is not activatable). A land that is not
//! targetable (shroud, hexproof for an opponent's land) is removed from the
//! choices by the engine after this filter answers.

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};
use serde::{Deserialize, Serialize};

/// The filters of wave C2c.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C2cTargetV1 {
    /// Exactly this many distinct lands on either battlefield.
    LandsV1(u8),
}

fn owned(target: StandardTargetV1) -> Option<C2cTargetV1> {
    match target {
        StandardTargetV1::C2cV1(filter) => Some(filter),
        _ => None,
    }
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    Some(match owned(target)? {
        C2cTargetV1::LandsV1(count) => (count, count),
    })
}

/// Whether an object target of this module's filter may be in `zone`.
pub(crate) fn zone_allows(target: StandardTargetV1, zone: Zone) -> bool {
    owned(target).is_some() && zone == Zone::Battlefield
}

/// Whether `object` is a land on a battlefield, or `None` for every other
/// filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    owned(target)?;
    Some(
        state
            .objects
            .try_get(object)
            .is_some_and(|live| live.zone == Zone::Battlefield)
            && crate::engine::object_has_type(state, object, CardType::Land),
    )
}

/// The legal choices for slot `prefix.len()` of this module's filter, or
/// `None` for every other filter: the lands after the last one named.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    let C2cTargetV1::LandsV1(count) = owned(target)?;
    if prefix.len() >= usize::from(count) {
        return Some(Vec::new());
    }
    let after = prefix.iter().filter_map(|target| match target {
        Target::Object(object) => Some(*object),
        _ => None,
    });
    let after = after.max();
    let mut lands = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .filter(|&object| after.is_none_or(|last| object > last))
        .filter(|&object| target_matches(target, object, state) == Some(true))
        .collect::<Vec<_>>();
    lands.sort_unstable();
    Some(lands.into_iter().map(Target::Object).collect())
}
