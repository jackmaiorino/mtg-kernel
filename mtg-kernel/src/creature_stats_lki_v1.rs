//! Shared incarnation-exact creature stats; absent cache preserves predecessor bytes.
use crate::card_def::CardType;
use crate::ids::ObjectId;
use crate::state::{GameState, Zone};

pub(crate) fn record_creature_stats_lki(state: &mut GameState, id: ObjectId) {
    if !crate::engine::object_has_type(state, id, CardType::Creature) {
        return;
    }
    let source = crate::state::ObjectLinkV4 {
        object: id,
        zone_change_count: state.objects.get(id).zone_change_count,
    };
    let entry = crate::state::CreatureStatsLkiV1 {
        source,
        power: crate::engine::effective_power(state, id),
        toughness: crate::engine::effective_toughness(state, id),
    };
    let entries = state.creature_stats_lki_v1.get_or_insert_with(Vec::new);
    if !entries.iter().any(|old| old.source == source) {
        entries.push(entry);
    }
}

pub(crate) fn current_or_last_creature_stats(
    state: &GameState,
    binding: crate::effect::EffectObjectBinding,
) -> Option<(i32, i32)> {
    if state.objects.try_get(binding.object).is_some_and(|object| {
        object.zone == Zone::Battlefield
            && object.zone_change_count == binding.expected_zone_change_count
    }) {
        return Some((
            crate::engine::effective_power(state, binding.object),
            crate::engine::effective_toughness(state, binding.object),
        ));
    }
    state
        .creature_stats_lki_v1
        .as_ref()?
        .iter()
        .find_map(|entry| {
            (entry.source.object == binding.object
                && entry.source.zone_change_count == binding.expected_zone_change_count)
                .then_some((entry.power, entry.toughness))
        })
}
