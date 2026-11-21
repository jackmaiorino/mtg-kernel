//! Color protection and Phantom Nishoba both prevent the incoming damage.
//! Only Nishoba's effect can remove a counter from the affected permanent.
use crate::eternal_legacy_e9a_v1::{active_permanents, live_link, statics::protection_mask};
use crate::eternal_pauper_pf1c_v1::ordinary_damage::{protection_applies, OrdinaryV1};
use crate::eternal_premodern_p2c_v1::{self as p2c, statics::nishoba_applies, PHANTOM_NISHOBA};
use crate::event::DamageProposed;
use crate::ids::ObjectId;
use crate::state::{GameState, Target};

fn counter_can_be_removed(state: &GameState, target: ObjectId) -> bool {
    state.objects.get(target).counters.plus1_plus1 > 0
        && !p2c::read_state(state)
            .and_then(|p2c| p2c.nishoba_batch.as_ref())
            .is_some_and(|paid| paid.contains(&live_link(state, target)))
}

pub(super) fn may_need_choice(state: &GameState) -> bool {
    active_permanents(state, PHANTOM_NISHOBA, None)
        .any(|id| counter_can_be_removed(state, id) && protection_mask(state, id) != 0)
}

pub(super) fn eligible(state: &GameState, damage: &DamageProposed) -> bool {
    let Target::Object(target) = damage.target else {
        return false;
    };
    nishoba_applies(state, damage)
        && counter_can_be_removed(state, target)
        && protection_mask(state, target) != 0
        && protection_applies(state, damage)
}

/// Another simultaneous packet can already have removed the cohort's one
/// counter. Either prevention now blanks this portion without a further cost.
pub(super) fn coalesce_spent_nishoba(state: &GameState, options: &mut Vec<OrdinaryV1>) {
    if options.contains(&OrdinaryV1::PreventAll) {
        options.retain(|effect| {
            !matches!(effect, OrdinaryV1::Nishoba(target) if !counter_can_be_removed(state, *target))
        });
    }
}
