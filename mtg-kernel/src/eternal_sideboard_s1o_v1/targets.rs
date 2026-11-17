//! Target filters of the wave S1o cards, carried as
//! `StandardTargetV1::S1oV1`. `standard_cards_v1::legal_targets`,
//! `target_matches` and `StandardTargetV1::counts` ask them first.

use crate::card_def::CardType;
use crate::effect::EffectObjectBinding;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};
use serde::{Deserialize, Serialize};

/// The filters of wave S1o.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1oTargetV1 {
    /// Eowyn, Fearless Knight: "target creature an opponent controls with
    /// greater power" than the source's power, read from the source's
    /// battlefield incarnation while it is there and from its last-known
    /// power after it left (608.2b, 113.7a).
    OpponentCreatureGreaterPowerV1,
    /// Unlicensed Hearse: "up to two target cards from a single graveyard".
    /// The first card fixes the graveyard: the second must have the same
    /// owner.
    UpToTwoCardsFromOneGraveyardV1,
}

fn filter(target: StandardTargetV1) -> Option<S1oTargetV1> {
    match target {
        StandardTargetV1::S1oV1(filter) => Some(filter),
        _ => None,
    }
}

/// Whether this module's filter `target` takes graveyard cards.
pub(crate) fn in_graveyard(target: StandardTargetV1) -> bool {
    filter(target) == Some(S1oTargetV1::UpToTwoCardsFromOneGraveyardV1)
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    Some(match filter(target)? {
        S1oTargetV1::OpponentCreatureGreaterPowerV1 => (1, 1),
        S1oTargetV1::UpToTwoCardsFromOneGraveyardV1 => (2, 0),
    })
}

/// A card a graveyard target may name: a physical card in a graveyard.
fn graveyard_card(state: &GameState, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Graveyard && !live.v4.is_token && live.spell_copy_origin.is_none()
    })
}

/// A creature on the battlefield controlled by an opponent of `controller`.
fn opponent_creature(state: &GameState, controller: PlayerId, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.controller != controller
            && crate::engine::object_has_type(state, object, CardType::Creature)
    })
}

/// The power of the source incarnation `(object, zone_change_count)`: its
/// current power while that battlefield incarnation remains, otherwise its
/// last-known power as it left the battlefield.
fn source_power(state: &GameState, source: (ObjectId, u32)) -> Option<i32> {
    crate::creature_stats_lki_v1::current_or_last_creature_stats(
        state,
        EffectObjectBinding {
            object: source.0,
            expected_zone: Zone::Battlefield,
            expected_zone_change_count: source.1,
        },
    )
    .map(|(power, _)| power)
}

/// Whether `object` satisfies this module's filter `target` for `controller`
/// apart from any relation to the source (Eowyn's power comparison is made
/// in `legal_targets`, which knows the source), or `None` for every other
/// filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    Some(match filter(target)? {
        S1oTargetV1::OpponentCreatureGreaterPowerV1 => opponent_creature(state, controller, object),
        S1oTargetV1::UpToTwoCardsFromOneGraveyardV1 => graveyard_card(state, object),
    })
}

/// The legal choices for slot `prefix.len()` of this module's filter, or
/// `None` for every other filter. Without a source, Eowyn's filter offers
/// nothing (it fails closed, as the source-power graveyard filter does).
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    source: Option<(ObjectId, u32)>,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    let filter = filter(target)?;
    let (maximum, _) = counts(target)?;
    if prefix.len() >= usize::from(maximum) {
        return Some(Vec::new());
    }
    Some(match filter {
        S1oTargetV1::OpponentCreatureGreaterPowerV1 => {
            let Some(power) = source.and_then(|source| source_power(state, source)) else {
                return Some(Vec::new());
            };
            state.players[controller.opponent().index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&object| {
                    opponent_creature(state, controller, object)
                        && crate::engine::effective_power(state, object) > power
                })
                .map(Target::Object)
                .collect()
        }
        S1oTargetV1::UpToTwoCardsFromOneGraveyardV1 => {
            let owner = match prefix.first() {
                None => None,
                Some(Target::Object(first)) => {
                    let Some(live) = state.objects.try_get(*first) else {
                        return Some(Vec::new());
                    };
                    Some(live.owner)
                }
                Some(_) => return Some(Vec::new()),
            };
            [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .filter(|player| owner.is_none_or(|owner| owner == *player))
                .flat_map(|player| {
                    state.players[player.index()]
                        .graveyard
                        .iter()
                        .copied()
                        .filter(move |&object| {
                            graveyard_card(state, object)
                                && state.objects.get(object).owner == player
                        })
                })
                .map(Target::Object)
                .filter(|candidate| !prefix.contains(candidate))
                .collect()
        }
    })
}
