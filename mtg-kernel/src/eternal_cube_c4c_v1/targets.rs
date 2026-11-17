//! Target filters of the wave C4c cards, carried as
//! `StandardTargetV1::C4cV1`. `standard_cards_v1::legal_targets`,
//! `target_matches`, `target_zone_allows` and `StandardTargetV1::counts` ask
//! them first.
//!
//! Esika's Chariot: "target token you control".

use crate::card_def::CARD_DEFS;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};
use serde::{Deserialize, Serialize};

/// The filters of wave C4c.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C4cTargetV1 {
    /// Exactly one token on the battlefield that the announcing player
    /// controls (a copy of a token is a token too).
    ControlledTokenV1,
}

fn owned(target: StandardTargetV1) -> Option<C4cTargetV1> {
    match target {
        StandardTargetV1::C4cV1(filter) => Some(filter),
        _ => None,
    }
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    match owned(target)? {
        C4cTargetV1::ControlledTokenV1 => Some((1, 1)),
    }
}

/// Whether an object target of this module's filter may be in `zone`.
pub(crate) fn zone_allows(target: StandardTargetV1, zone: Zone) -> bool {
    owned(target).is_some() && zone == Zone::Battlefield
}

/// Whether `object` is a token `controller` controls on the battlefield, or
/// `None` for every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    owned(target)?;
    let Some(live) = state.objects.try_get(object) else {
        return Some(false);
    };
    Some(
        live.zone == Zone::Battlefield
            && live.controller == controller
            && (live.v4.is_token
                || CARD_DEFS
                    .get(usize::from(live.card_def))
                    .is_some_and(|def| def.is_token)),
    )
}

/// The legal choices for this module's filter after the already chosen
/// `prefix`, or `None` for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    let (maximum, _) = counts(target)?;
    if prefix.len() >= usize::from(maximum) {
        return Some(Vec::new());
    }
    Some(
        [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
            .filter(|&object| target_matches(target, controller, object, state) == Some(true))
            .map(Target::Object)
            .collect(),
    )
}
