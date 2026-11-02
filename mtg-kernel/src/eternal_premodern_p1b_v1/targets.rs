//! Target filters of the Premodern wave P1b cards, carried as
//! `StandardTargetV1` variants. `standard_cards_v1::legal_targets` and
//! `target_matches` ask these first.

use super::LEGACYS_ALLURE;
use crate::card_def::{CardType, Subtype};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, ObjectLinkV4, Target, Zone};

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    matches!(
        target,
        StandardTargetV1::CreaturePowerAtMostTreasureCountersV1
            | StandardTargetV1::AssemblyWorkerCreatureV1
    )
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    owns(target).then_some((1, 1))
}

/// The treasure counters that bound Legacy's Allure's target: those of the
/// targeting Allure's own incarnation `source` (an object and its zone
/// change count). While the Allure is on the battlefield (as its ability is
/// offered and its target chosen) they are its live counters; once it was
/// sacrificed to activate the ability, the counters recorded for that
/// incarnation and its ability on the stack. No source, or another
/// incarnation, bounds nothing above zero.
fn treasure_bound(state: &GameState, source: Option<(ObjectId, u32)>) -> u16 {
    let Some((object, zone_change_count)) = source else {
        return 0;
    };
    if super::definition_name(state, object) != Some(LEGACYS_ALLURE) {
        return 0;
    }
    let live = ObjectLinkV4 {
        object,
        zone_change_count,
    };
    if super::link_live_in(state, live, Zone::Battlefield) {
        return super::statics::treasure_counters(state, object);
    }
    // The sacrificed incarnation, named by the ability's frozen source or by
    // the graveyard card it became.
    [Some(zone_change_count), zone_change_count.checked_sub(1)]
        .into_iter()
        .flatten()
        .find_map(|count| {
            super::statics::sacrificed_allure_counters(
                state,
                ObjectLinkV4 {
                    object,
                    zone_change_count: count,
                },
            )
        })
        .unwrap_or(0)
}

/// Whether `object` matches `target` given Legacy's Allure's treasure
/// `bound`.
fn matches_with_bound(
    target: StandardTargetV1,
    object: ObjectId,
    bound: u16,
    state: &GameState,
) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, object, CardType::Creature)
    {
        return false;
    }
    match target {
        StandardTargetV1::CreaturePowerAtMostTreasureCountersV1 => {
            crate::engine::effective_power(state, object) <= i32::from(bound)
        }
        StandardTargetV1::AssemblyWorkerCreatureV1 => {
            crate::engine::has_effective_subtype(state, object, Subtype::AssemblyWorker)
        }
        _ => false,
    }
}

/// Whether `object` satisfies this module's filter `target`, or `None` for
/// every other filter. With no targeting source, Legacy's Allure's filter
/// takes only creatures with power 0 or less.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    _controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    owns(target).then(|| matches_with_bound(target, object, 0, state))
}

/// The legal choices for this module's filter `target` from the targeting
/// `source` incarnation, or `None` for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    source: Option<(ObjectId, u32)>,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    if !owns(target) {
        return None;
    }
    if !prefix.is_empty() {
        return Some(Vec::new());
    }
    let bound = if target == StandardTargetV1::CreaturePowerAtMostTreasureCountersV1 {
        treasure_bound(state, source)
    } else {
        0
    };
    Some(
        [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
            .filter(|&object| matches_with_bound(target, object, bound, state))
            .map(Target::Object)
            .collect(),
    )
}
