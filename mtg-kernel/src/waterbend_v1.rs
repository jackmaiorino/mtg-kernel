//! Shared rules for activated waterbend costs (CR 701.67a): printed generic
//! mana, tap count, tap cap, pricing, payability, and tap filtering.
//!
//! Every card whose printed activated abilities represent the `0..=N` tap
//! splits of a single `Waterbend {N}` cost (Aang's Iceberg, Invasion
//! Submersible, and Aang, Swift Savior) delegates here so the activation menu,
//! tap-target prompts, and restored pending-activation validation enforce one
//! rule.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardDef, CardType, CostComponent, CARD_DEFS};
use crate::effect::EffectObjectBinding;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::Cost;
use crate::state::{GameObject, GameState, Zone};

/// Printed generic mana in a card's activated waterbend cost, when every
/// printed activated ability on its front face is one tap-count payment of
/// that waterbend cost (with ability 0 as the all-mana payment).
fn printed_generic_for_card(name: &str) -> Option<u8> {
    match name {
        crate::eternal_cube_c5c_v1::AANGS_ICEBERG => Some(3),
        crate::eternal_sideboard_s1q_v1::INVASION_SUBMERSIBLE => Some(3),
        crate::eternal_cube_c9a_v1::AANG_SWIFT_SAVIOR => Some(8),
        _ => None,
    }
}

/// Resolves a face-up, front-face battlefield object with a printed waterbend
/// activated ability into its live object, card definition, and printed
/// waterbend generic mana.
fn waterbend_spec(
    state: &GameState,
    source: ObjectId,
) -> Option<(&GameObject, &'static CardDef, u8)> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let live = state.objects.try_get(source)?;
    if live.zone != Zone::Battlefield || live.v4.face_down_v1.is_some() || live.v4.face_index != 0 {
        return None;
    }
    let def = CARD_DEFS.get(usize::from(live.card_def))?;
    if !def.is_executable() {
        return None;
    }
    let generic = printed_generic_for_card(def.name)?;
    Some((live, def, generic))
}

/// Whether `components` is the cost slice of one of `def`'s own printed
/// waterbend abilities (pointer-identical to the definition's slice, so a
/// granted ability or copied cost slice is priced as usual).
fn is_printed_waterbend_cost(def: &CardDef, components: &[CostComponent]) -> bool {
    def.activated_abilities
        .iter()
        .any(|ability| std::ptr::eq(ability.cost, components))
}

/// Number of waterbend taps one payment carries: its `TapControlled` count,
/// or 0 for the all-mana payment.
fn tap_count(components: &[CostComponent]) -> u8 {
    components
        .iter()
        .find_map(|component| match component {
            CostComponent::TapControlled { count, .. } => Some(*count),
            _ => None,
        })
        .unwrap_or(0)
}

/// Maximum number of permanents a waterbend payment on `source` may tap:
/// at most the printed waterbend generic mana (even under taxes) and never
/// more than the reduced generic total shown by the all-mana payment (ability
/// 0, e.g. under Zirda or Forensic Gadgeteer).
fn tap_cap(
    state: &GameState,
    source: ObjectId,
    live: &GameObject,
    def: &CardDef,
    printed_generic: u8,
) -> u8 {
    let total = def.activated_abilities.first().and_then(|ability| {
        crate::engine::activation_mana_payment(ability.cost, live.controller, source, state)
    });
    total.map_or(printed_generic, |total| total.generic.min(printed_generic))
}

/// The printed cost every waterbend payment starts from (`{N}`), or `None`
/// for any non-waterbend or granted ability. The engine's activation total
/// (601.2f) adds taxes to it and applies reductions (including floors) before
/// `taps_paid` subtracts the chosen taps.
pub(crate) fn printed_cost(
    state: &GameState,
    source: ObjectId,
    components: &[CostComponent],
) -> Option<Cost> {
    let (_, def, generic) = waterbend_spec(state, source)?;
    is_printed_waterbend_cost(def, components).then_some(Cost {
        pips: &[],
        generic,
        x_count: 0,
    })
}

/// The mana a waterbend payment still needs once its taps paid their part of
/// `total` (601.2h: each tap pays `{1}` of the generic total). Other
/// abilities keep `total`.
pub(crate) fn taps_paid(
    state: &GameState,
    source: ObjectId,
    components: &[CostComponent],
    total: Cost,
) -> Cost {
    if printed_cost(state, source, components).is_none() {
        return total;
    }
    Cost {
        generic: total.generic.saturating_sub(tap_count(components)),
        ..total
    }
}

/// Whether a waterbend payment's tap count is within the current tap cap.
/// Shared by the activation menu and restored pending-activation validation.
/// Returns `true` for non-waterbend sources and granted abilities.
pub(crate) fn taps_within_cap(
    state: &GameState,
    source: ObjectId,
    components: &[CostComponent],
) -> bool {
    let Some((live, def, generic)) = waterbend_spec(state, source) else {
        return true;
    };
    !is_printed_waterbend_cost(def, components)
        || tap_count(components) <= tap_cap(state, source, live, def, generic)
}

/// Untapped artifacts and creatures `player` controls outside `chosen`:
/// what a waterbend tap may name (`PermanentFilter::ArtifactOrCreature`;
/// summoning sickness does not apply since the cost has no `{T}`).
fn tappable(state: &GameState, player: PlayerId, chosen: &[ObjectId]) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|object| !chosen.contains(object))
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield
                && live.controller == player
                && !live.tapped
                && (crate::engine::object_has_type(state, object, CardType::Artifact)
                    || crate::engine::object_has_type(state, object, CardType::Creature))
        })
        .collect()
}

/// Whether `more` further waterbend taps can be chosen alongside `chosen` so
/// that the remaining mana part (`cost`) stays payable with none of the
/// tapped permanents among the mana sources.
fn completion_exists(
    state: &GameState,
    player: PlayerId,
    source: ObjectId,
    cost: &Cost,
    chosen: &[ObjectId],
    more: usize,
) -> bool {
    let Some(plan) = crate::mana::plan_activation_mana_v1(cost, 0, player, state, source, chosen)
    else {
        return false;
    };
    let rest = tappable(state, player, chosen);
    if rest.len() < more {
        return false;
    }
    let outside_plan = rest
        .iter()
        .filter(|&&object| !plan.taps.iter().any(|&(tapped, _)| tapped == object))
        .count();
    outside_plan >= more || completion_among(state, player, source, cost, chosen, &rest, more)
}

/// Whether `more` picks among `among`, taken in order, leave `cost` payable
/// with them and `chosen` reserved.
fn completion_among(
    state: &GameState,
    player: PlayerId,
    source: ObjectId,
    cost: &Cost,
    chosen: &[ObjectId],
    among: &[ObjectId],
    more: usize,
) -> bool {
    if more == 0 {
        return crate::mana::plan_activation_mana_v1(cost, 0, player, state, source, chosen)
            .is_some();
    }
    if among.len() < more {
        return false;
    }
    among.iter().enumerate().any(|(index, &next)| {
        let mut with = chosen.to_vec();
        with.push(next);
        completion_among(
            state,
            player,
            source,
            cost,
            &with,
            &among[index + 1..],
            more - 1,
        )
    })
}

/// Whether waterbend payment `ability_index` on `source` may be offered in
/// the activation menu: within the tap cap, and some choice of its taps
/// leaves its remaining mana payable. Returns `true` for non-waterbend
/// sources or non-printed abilities.
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    ability_index: usize,
) -> bool {
    let Some((live, def, generic)) = waterbend_spec(state, source) else {
        return true;
    };
    let Some(ability) = def.activated_abilities.get(ability_index) else {
        return true;
    };
    let taps = tap_count(ability.cost);
    if taps > tap_cap(state, source, live, def, generic) {
        return false;
    }
    let cost = crate::engine::activation_mana_payment(ability.cost, live.controller, source, state)
        .unwrap_or_else(Cost::zero);
    completion_exists(
        state,
        live.controller,
        source,
        &cost,
        &[],
        usize::from(taps),
    )
}

/// Keeps only the waterbend taps after which the rest of the pending
/// waterbend payment can still be completed. Leaves non-waterbend and granted
/// pending activations untouched.
pub(crate) fn retain_payable_taps(
    state: &GameState,
    source: ObjectId,
    already_chosen: &[EffectObjectBinding],
    candidates: &mut Vec<EffectObjectBinding>,
) {
    let Some((live, def, _)) = waterbend_spec(state, source) else {
        return;
    };
    let Some(pending) = state.engine.pending_activation.as_ref().filter(|pending| {
        pending.source == source
            && pending.cauldron_grant.0.is_none()
            && pending.full_text_activation_v1.is_empty()
    }) else {
        return;
    };
    let Some(ability) = def
        .activated_abilities
        .get(usize::from(pending.ability_index))
    else {
        return;
    };
    let cost = crate::engine::activation_mana_payment(ability.cost, live.controller, source, state)
        .unwrap_or_else(Cost::zero);
    let taps = usize::from(tap_count(ability.cost));
    let chosen: Vec<ObjectId> = already_chosen
        .iter()
        .map(|binding| binding.object)
        .collect();
    let more = taps.saturating_sub(chosen.len() + 1);
    candidates.retain(|candidate| {
        let mut with = chosen.clone();
        with.push(candidate.object);
        completion_exists(state, pending.controller, source, &cost, &with, more)
    });
}
