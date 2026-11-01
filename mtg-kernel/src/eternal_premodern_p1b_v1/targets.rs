//! Target filters of the Premodern wave P1b cards, carried as
//! `StandardTargetV1` variants. `standard_cards_v1::legal_targets` and
//! `target_matches` ask these first.

use super::LEGACYS_ALLURE;
use crate::card_def::{CardType, Subtype};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

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
/// Allure being activated, or, as its ability resolves or is checked later,
/// the most any of `controller`'s Allures had. The resolving ability itself
/// checks its own Allure's count (`P1bOpV1::AllureControlV1`).
fn treasure_bound(state: &GameState, controller: PlayerId) -> u16 {
    if let Some(pending) = state.engine.pending_activation.as_ref() {
        if super::definition_name(state, pending.source) == Some(LEGACYS_ALLURE)
            && state.objects.get(pending.source).zone == Zone::Battlefield
        {
            return super::statics::treasure_counters(state, pending.source);
        }
    }
    let Some(p1b) = super::read_state(state) else {
        return 0;
    };
    let live = p1b
        .treasure_counters
        .iter()
        .filter(|(host, _)| {
            super::link_live_in(state, *host, Zone::Battlefield)
                && state.objects.get(host.object).controller == controller
        })
        .map(|(_, count)| *count);
    let sacrificed = p1b.sacrificed_allures.iter().map(|(_, count)| *count);
    live.chain(sacrificed).max().unwrap_or(0)
}

/// Whether `object` satisfies this module's filter `target` for
/// `controller`, or `None` for every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    if !owns(target) {
        return None;
    }
    let Some(live) = state.objects.try_get(object) else {
        return Some(false);
    };
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, object, CardType::Creature)
    {
        return Some(false);
    }
    Some(match target {
        StandardTargetV1::CreaturePowerAtMostTreasureCountersV1 => {
            crate::engine::effective_power(state, object)
                <= i32::from(treasure_bound(state, controller))
        }
        StandardTargetV1::AssemblyWorkerCreatureV1 => {
            crate::engine::has_effective_subtype(state, object, Subtype::AssemblyWorker)
        }
        _ => false,
    })
}

/// The legal choices for this module's filter `target`, or `None` for every
/// other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    if !owns(target) {
        return None;
    }
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
