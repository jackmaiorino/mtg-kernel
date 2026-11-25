//! Scoped replacement choices without Hallow: Circle/global prevention and
//! Sphere of Law/Ojer Axonil, and protection/Phantom Nishoba. Fully replaced residuals return to ordinary commit
//! through local receipts; public proposals and state schemas remain unchanged.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

mod permanent_v1;

use crate::eternal_pauper_pf1c_v1::ordinary_damage::{OrdinaryV1, Packet};
use crate::eternal_sideboard_s1d_v1::statics as circles;
use crate::event::{self, DamageProposed, ProposedEvent, ReplacementEffectKind};
use crate::ids::PlayerId;
use crate::linked_aura_operation_v1::{
    legacy_damage_order_v1, legacy_hallow_only_v1, legacy_sphere_order_v1,
    soul_scar_damage_order_v1,
};
use crate::state::GameState;
pub(crate) mod nomads_v1;

/// Only a current paid Circle, protected Nishoba or Sphere/Ojer pair borrows
/// operation replay. The actual packet predicate retains source/color/turn
/// matching; a packet with just one useful prevention keeps its existing menu.
pub(crate) fn may_need_choice(state: &GameState) -> bool {
    if legacy_damage_order_v1() {
        return !legacy_hallow_only_v1()
            && cfg!(feature = "eternal-fixtures")
            && !cannot_prevent(state)
            && ((circles::has_live_circle_shield(state)
                && (crate::eternal_pauper_e6a_v1::has_active_global_fog(state)
                    || has_current_color_fog(state)))
                || (legacy_sphere_order_v1() && has_sphere_ojer(state)));
    }
    if nomads_v1::has_active_redirects(state) {
        return true;
    }
    cfg!(feature = "eternal-fixtures")
        && (soul_scar_may_need_choice(state)
            || has_unicorn_ojer(state)
            || (!cannot_prevent(state)
                && (circles::has_live_circle_shield(state)
                    || crate::eternal_pauper_pf2b_v1::has_live_aurora_shield(state)
                    || permanent_v1::may_need_choice(state)
                    || has_sphere_ojer(state))))
}

pub(crate) fn soul_scar_may_need_choice(state: &GameState) -> bool {
    soul_scar_damage_order_v1()
        && crate::eternal_e5a_v1::active_permanents(
            state,
            crate::eternal_cube_c7b_v1::SOUL_SCAR_MAGE,
            None,
        )
        .next()
        .is_some()
}

fn has_current_color_fog(state: &GameState) -> bool {
    state.engine.active_replacements.iter().any(|replacement| {
        matches!(
            replacement.kind,
            ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn { turn, active_player, .. }
                if turn == state.turn_key_v1() && active_player == state.active_player
        )
    })
}

fn has_sphere_ojer(state: &GameState) -> bool {
    [PlayerId::P0, PlayerId::P1].into_iter().any(|player| {
        crate::eternal_premodern_p3a_v1::statics::has_sphere_of_law(state, player)
            && crate::standard_cards_v1::has_active_ojer(state, player.opponent())
    })
}

fn has_unicorn_ojer(state: &GameState) -> bool {
    crate::eternal_pauper_pf2c_v1::has_benevolent_unicorn(state)
        && [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .any(|player| crate::standard_cards_v1::has_active_ojer(state, player))
}

fn full_prevention(state: &GameState, damage: &DamageProposed) -> bool {
    if crate::eternal_pauper_e6a_v1::global_prevents_damage(state, damage) {
        return true;
    }
    let event = ProposedEvent::Damage(damage.clone());
    state.engine.active_replacements.iter().any(|replacement| {
        matches!(
            replacement.kind,
            ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn { .. }
        ) && !damage.touched_by.contains(&replacement.id)
            && event::replacement_applies(replacement, &event, state)
    })
}

pub(crate) fn cannot_prevent(state: &GameState) -> bool {
    state.engine.until_end_of_turn.iter().any(|effect| {
        matches!(
            effect,
            crate::engine::UntilEndOfTurnEffect::DamageCannotBePrevented { .. }
        )
    })
}

pub(crate) fn cannot_prevent_damage(state: &GameState, damage: &DamageProposed) -> bool {
    cannot_prevent(state)
        || (cfg!(feature = "eternal-fixtures")
            && crate::eternal_cube_c7b_v1::combat_damage_cannot_be_prevented(state, damage))
}

pub(crate) fn soul_scar_competes(state: &GameState, damage: &DamageProposed) -> bool {
    if !soul_scar_damage_order_v1()
        || crate::eternal_cube_c7b_v1::noncombat_damage_replacement_source(state, damage).is_none()
    {
        return false;
    }
    let cannot_prevent = cannot_prevent_damage(state, damage);
    Packet::new(damage)
        .options(state)
        .iter()
        .any(|effect| match effect {
            OrdinaryV1::SoulScar(_) => false,
            OrdinaryV1::Nishoba(target) if cannot_prevent => {
                permanent_v1::counter_can_be_removed(state, *target)
            }
            _ => true,
        })
        || (!cannot_prevent && circles::circle_shield_position(state, damage).is_some())
}

pub(crate) fn unicorn_competes(state: &GameState, damage: &DamageProposed) -> bool {
    crate::eternal_pauper_pf2c_v1::applicable_unicorns(state, damage)
        .next()
        .is_some()
        && residual_order_applies(state, damage)
}

fn aurora_competes(state: &GameState, damage: &DamageProposed) -> bool {
    let shields = crate::eternal_pauper_pf2b_v1::applicable_aurora_shields(state, damage);
    if shields.is_empty() {
        return false;
    }
    if shields
        .iter()
        .any(|shield| shield.remaining != shields[0].remaining)
        || circles::circle_shield_position(state, damage).is_some()
    {
        return true;
    }
    Packet::new(damage)
        .options(state)
        .iter()
        .any(|effect| !matches!(effect, OrdinaryV1::AuroraShield(_)))
}

pub(crate) fn eligible(state: &GameState, damage: &DamageProposed) -> bool {
    if soul_scar_competes(state, damage) || unicorn_competes(state, damage) {
        return true;
    }
    if damage.amount <= 0 || cannot_prevent_damage(state, damage) {
        return false;
    }
    if legacy_damage_order_v1() {
        return !legacy_hallow_only_v1()
            && circles::circle_shield_position(state, damage).is_some()
            && full_prevention(state, damage);
    }

    if permanent_v1::eligible(state, damage) || aurora_competes(state, damage) {
        return true;
    }
    let mut matching = circles::matching_circle_shields(state, damage);
    let Some((_, first)) = matching.next() else {
        return false;
    };
    if matching.any(|(_, shield)| shield != first)
        || full_prevention(state, damage)
        || crate::eternal_pauper_pf1c_v1::ordinary_damage::prevention_without_cost(state, damage)
    {
        return true;
    }
    // A registered finite shield may be spent instead of, or before, the
    // Circle. Its application retains the existing charge/touched-by data.
    let event = ProposedEvent::Damage(damage.clone());
    if state.engine.active_replacements.iter().any(|replacement| {
        !damage.touched_by.contains(&replacement.id)
            && event::replacement_applies(replacement, &event, state)
            && matches!(replacement.kind,
                ReplacementEffectKind::PreventNextDamage { remaining, .. } if remaining > 0)
    }) {
        return true;
    }
    let spheres =
        crate::eternal_premodern_p3a_v1::statics::applicable_spheres(state, damage).count();
    let unicorns = crate::eternal_pauper_pf2c_v1::applicable_unicorns(state, damage).count();
    spheres.saturating_mul(2).saturating_add(unicorns) >= damage.amount as usize
}

/// Re-evaluate all state-bearing replacements after each chosen effect.
/// A portion with no remaining applicable effect returns its final damage;
/// finite shields retain their distinct consumption choice.
pub(crate) fn packet_options(
    state: &GameState,
    packet: &Packet,
    competing_source: bool,
) -> Vec<OrdinaryV1> {
    let mut options = Vec::new();
    // Preserve the already published Circle/global menu. On the new residual
    // path, identical reductions are one initial choice; each real Sphere is
    // still applied at most once and may surround an intervening increase.
    let coalesce_spheres = !full_prevention(state, &packet.damage);
    if !cannot_prevent_damage(state, &packet.damage) {
        for (position, shield) in circles::matching_circle_shields(state, &packet.damage) {
            if !options.iter().any(|effect| {
                matches!(effect,
                OrdinaryV1::Circle { shield: existing, .. } if *existing == shield)
            }) {
                options.push(OrdinaryV1::Circle { position, shield });
            }
        }
    }
    let legacy = legacy_damage_order_v1();
    for effect in packet.options(state) {
        let free_prevention = if legacy {
            matches!(&effect, OrdinaryV1::Registered(replacement) if matches!(
                replacement.kind, ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn { .. }))
        } else {
            permanent_v1::free_prevention(&effect)
        };
        let effect = if free_prevention {
            OrdinaryV1::PreventAll
        } else {
            effect
        };
        if coalesce_spheres
            && matches!(effect, OrdinaryV1::Sphere(_))
            && options
                .iter()
                .any(|option| matches!(option, OrdinaryV1::Sphere(_)))
        {
            continue;
        }
        if matches!(effect, OrdinaryV1::Unicorn(_))
            && options
                .iter()
                .any(|option| matches!(option, OrdinaryV1::Unicorn(_)))
        {
            continue;
        }
        if let OrdinaryV1::AuroraShield(id) = effect {
            let remaining = crate::eternal_pauper_pf2b_v1::aurora_shield_remaining(state, id);
            if options.iter().any(|option| {
                matches!(
                    option,
                    OrdinaryV1::AuroraShield(other)
                        if crate::eternal_pauper_pf2b_v1::aurora_shield_remaining(state, *other)
                            == remaining
                )
            }) {
                continue;
            }
        }
        if !options.contains(&effect) {
            options.push(effect);
        }
    }
    if !legacy {
        permanent_v1::coalesce_spent_nishoba(
            state,
            &mut options,
            cannot_prevent_damage(state, &packet.damage),
        );
        nomads_v1::normalize(state, packet, &mut options, competing_source);
    }
    options
}

/// The affected player can obtain different surviving amounts by ordering the
/// active reduction and increase. Their authoritative matchers retain source
/// color, controller, printed-ability, face and historical-source restrictions.
pub(crate) fn residual_order_applies(state: &GameState, damage: &DamageProposed) -> bool {
    let legacy = legacy_damage_order_v1();
    let unpreventable = cannot_prevent_damage(state, damage);
    let unicorns = crate::eternal_pauper_pf2c_v1::applicable_unicorns(state, damage).count();
    if (legacy && !legacy_sphere_order_v1()) || damage.is_combat || damage.amount <= 0 {
        return false;
    }
    if unpreventable {
        if unicorns == 0 {
            return false;
        }
    } else if (!legacy && circles::circle_shield_position(state, damage).is_some())
        // With any matching Circle, only its observable consumption predicate
        // admits the packet. Amount changes before an inevitable identical
        // shield consumption do not introduce a redundant numeric menu.
        || full_prevention(state, damage)
        || crate::eternal_pauper_pf1c_v1::ordinary_damage::prevention_without_cost(state, damage)
    {
        return false;
    }
    let powers: Vec<_> = crate::standard_cards_v1::matching_ojers(state, damage)
        .map(|(_, power)| power)
        .collect();
    let spheres = if unpreventable {
        0
    } else {
        crate::eternal_premodern_p3a_v1::statics::applicable_spheres(state, damage).count()
    };
    if !legacy && powers.len() == 1 {
        return single_ojer_has_distinct_outcomes(damage.amount, spheres, unicorns, powers[0]);
    }
    // Keep every native Ojer's multiplicity. For multiple increases this gate
    // is conservative about equivalent numeric menus, while the actual
    // frontier still applies each exact effect once and preserves all outcomes.
    let Some(power) = powers.into_iter().max() else {
        return false;
    };
    let mut amount = damage.amount;
    for _ in 0..spheres {
        if amount <= 0 {
            break;
        }
        if amount < power {
            return true;
        }
        amount = amount.saturating_sub(2);
    }
    for _ in 0..unicorns {
        if amount <= 0 {
            break;
        }
        if amount < power {
            return true;
        }
        amount = amount.saturating_sub(1);
    }
    false
}

/// Exact scalar bounds with one Ojer: its earliest and latest legal use
/// bracket every possible final amount. Zero ends the event before an unused
/// increase can revive it. All arithmetic is widened before subtraction.
fn single_ojer_has_distinct_outcomes(
    amount: i32,
    spheres: usize,
    unicorns: usize,
    power: i32,
) -> bool {
    let amount = i64::from(amount);
    let power = i64::from(power);
    let spheres = i64::try_from(spheres)
        .unwrap_or(i64::MAX / 8)
        .min(i64::MAX / 8);
    let unicorns = i64::try_from(unicorns)
        .unwrap_or(i64::MAX / 8)
        .min(i64::MAX / 8);
    let total_red = 2 * spheres + unicorns;
    if amount <= 0 || total_red == 0 {
        return false;
    }
    let need = (amount - power + 1).max(0);
    let r_min = if need == 0 {
        0
    } else if unicorns > 0 {
        need
    } else {
        need + (need & 1)
    };
    let r_max = if amount <= 1 {
        0
    } else if unicorns > 0 {
        (amount - 1).min(total_red)
    } else {
        (2 * spheres).min((amount - 1) & !1)
    };
    if r_min > r_max || r_min > total_red {
        return false;
    }
    let minimum = if total_red >= amount {
        0
    } else {
        (power - (total_red - r_min)).max(0)
    };
    let maximum = (power - (total_red - r_max)).max(0);
    minimum != maximum
}

/// Local, nonserialized evidence that the frontier finished this exact damage
/// event. All other events still use the ordinary replacement pass. Keeping
/// this separate from DamageProposed preserves public event bytes and avoids
/// applying a static reduction or multiplier twice during batch commitment.
#[derive(Default)]
pub(crate) struct PreparedDamageBatchV1(Vec<Option<DamageProposed>>);

impl PreparedDamageBatchV1 {
    pub(crate) fn new(events: Vec<Option<DamageProposed>>) -> Self {
        Self(events)
    }
    pub(crate) fn apply(
        &self,
        state: &mut GameState,
        at: usize,
        event: ProposedEvent,
    ) -> Option<ProposedEvent> {
        let Some(Some(damage)) = self.0.get(at) else {
            return event::apply_replacements(state, event);
        };
        if event != ProposedEvent::Damage(damage.clone()) {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                damage.source,
            ));
            return None;
        }
        #[cfg(feature = "eternal-fixtures")]
        if !soul_scar_damage_order_v1()
            && crate::eternal_cube_c7b_v1::replace_noncombat_damage(state, damage)
        {
            return None;
        }
        Some(event)
    }
}

pub(crate) enum BatchDamageResultV1 {
    Suspended,
    Complete(PreparedDamageBatchV1),
}

pub(crate) enum DamageResultV1 {
    Uncovered,
    Prevented,
    Residual(DamageProposed),
}
