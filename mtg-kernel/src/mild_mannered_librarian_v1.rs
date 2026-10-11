//! Printed Librarian preparation reuses persistent upgrades and source counters.
use crate::card_def::{Subtype, CARD_DEFS};
use crate::effect::{EffectOp, ExecCtx, PlayerRef};
use crate::ids::ObjectId;
use crate::standard_creatures_v1::CreatureEffectV1;
use crate::state::{GameState, Zone};

pub fn program_v1() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::CreatureUpgrade(CreatureEffectV1::LibrarianWerewolfAndCounters),
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// A printed definition-owned restriction, separate from the per-turn counter.
pub(crate) fn activation_allowed(state: &GameState, source: ObjectId, index: usize) -> bool {
    if !is_librarian(state, source, index) {
        return true;
    }
    !state
        .objects
        .get(source)
        .v4
        .creature_upgrade
        .as_ref()
        .is_some_and(|upgrade| upgrade.once_activated.contains(&0))
}
pub(crate) fn activation_paid(state: &mut GameState, source: ObjectId, index: usize) {
    if is_librarian(state, source, index) {
        let uses = &mut state
            .objects
            .get_mut(source)
            .v4
            .creature_upgrade
            .get_or_insert_with(Default::default)
            .once_activated;
        if !uses.contains(&0) {
            uses.push(0);
        }
    }
}
pub(crate) fn is_librarian(state: &GameState, source: ObjectId, index: usize) -> bool {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return false;
    }
    let Some(object) = state.objects.try_get(source) else {
        return false;
    };
    let Some(def) = CARD_DEFS.get(usize::from(object.card_def)) else {
        return false;
    };
    def.is_executable()
        && def.name == "Mild-Mannered Librarian"
        && index == 0
        && def.activated_abilities.len() == 1
        && (def.activated_abilities[0].effect)() == program_v1()
}

/// Resolution's root has already authenticated the captured historical source.
/// These two instructions affect the same surviving permanent; draw is separate.
pub(crate) fn resolve_source_upgrade(state: &mut GameState, ctx: &ExecCtx) {
    let Some(source) = ctx.ability_source_contract else {
        return;
    };
    let Some(live) = state.objects.try_get(source.source) else {
        return;
    };
    if source.source != ctx.source
        || source.controller != ctx.controller
        || source.zone != Zone::Battlefield
        || live.zone != Zone::Battlefield
        || live.owner != source.owner
        || live.zone_change_count != source.zone_change_count
    {
        return;
    }
    let timestamp = crate::engine::next_timestamp(state);
    state
        .objects
        .get_mut(source.source)
        .v4
        .creature_upgrade
        .get_or_insert_with(Default::default)
        .creature_types = Some((vec![Subtype::Werewolf.stable_id()], timestamp));
    if crate::event::add_plus_one_counters(state, source.source, ctx.controller, 2).is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "mild_mannered_librarian_v1_tests.rs"]
mod tests;
