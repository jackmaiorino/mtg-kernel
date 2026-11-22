//! Scoped replacement choices without Hallow: Circle/global prevention and
//! Sphere of Law/Ojer Axonil, and protection/Phantom Nishoba. Fully replaced residuals return to ordinary commit
//! through local receipts; public proposals and state schemas remain unchanged.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

mod permanent_v1;

use crate::eternal_pauper_pf1c_v1::ordinary_damage::{OrdinaryV1, Packet};
use crate::eternal_sideboard_s1d_v1::statics as circles;
use crate::event::{self, DamageProposed, ProposedEvent, ReplacementEffectKind};
use crate::ids::PlayerId;
use crate::state::GameState;
pub(crate) mod nomads_v1;

/// Only a current paid Circle, protected Nishoba or Sphere/Ojer pair borrows
/// operation replay. The actual packet predicate retains source/color/turn
/// matching; a packet with just one useful prevention keeps its existing menu.
pub(crate) fn may_need_choice(state: &GameState) -> bool {
    if nomads_v1::has_active_redirects(state) {
        return true;
    }
    cfg!(feature = "eternal-fixtures")
        && !cannot_prevent(state)
        && (circles::has_live_circle_shield(state)
            || permanent_v1::may_need_choice(state)
            || [PlayerId::P0, PlayerId::P1].into_iter().any(|player| {
                crate::eternal_premodern_p3a_v1::statics::has_sphere_of_law(state, player)
                    && crate::standard_cards_v1::has_active_ojer(state, player.opponent())
            }))
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

pub(crate) fn eligible(state: &GameState, damage: &DamageProposed) -> bool {
    if damage.amount <= 0 || cannot_prevent(state) {
        return false;
    }
    if permanent_v1::eligible(state, damage) {
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
    // A registered complete shield can be used instead of the Circle. Its
    // existing application path retains any finite charge and touched-by data.
    let event = ProposedEvent::Damage(damage.clone());
    if state.engine.active_replacements.iter().any(|replacement|
        !damage.touched_by.contains(&replacement.id)
            && event::replacement_applies(replacement, &event, state)
            && matches!(replacement.kind,
                ReplacementEffectKind::PreventNextDamage { remaining, .. } if remaining >= damage.amount)) {
        return true;
    }
    let spheres =
        crate::eternal_premodern_p3a_v1::statics::applicable_spheres(state, damage).count();
    spheres >= (damage.amount as u32).div_ceil(2) as usize
}

/// Re-evaluate all state-bearing replacements after each chosen effect.
/// A portion with no remaining applicable effect returns its final damage;
/// finite shields retain their distinct consumption choice.
pub(crate) fn packet_options(state: &GameState, packet: &Packet) -> Vec<OrdinaryV1> {
    let mut options = Vec::new();
    // Preserve the already published Circle/global menu. On the new residual
    // path, identical reductions are one initial choice; each real Sphere is
    // still applied at most once and may surround an intervening increase.
    let coalesce_spheres = !full_prevention(state, &packet.damage);
    for (position, shield) in circles::matching_circle_shields(state, &packet.damage) {
        if !options.iter().any(|effect| {
            matches!(effect,
            OrdinaryV1::Circle { shield: existing, .. } if *existing == shield)
        }) {
            options.push(OrdinaryV1::Circle { position, shield });
        }
    }
    for effect in packet.options(state) {
        let effect = if permanent_v1::free_prevention(&effect) {
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
        if !options.contains(&effect) {
            options.push(effect);
        }
    }
    permanent_v1::coalesce_spent_nishoba(state, &mut options);
    nomads_v1::normalize(state, packet, &mut options);
    options
}

/// The affected player can obtain different surviving amounts by ordering the
/// active reduction and increase. Their authoritative matchers retain source
/// color, controller, printed-ability, face and historical-source restrictions.
pub(crate) fn residual_order_applies(state: &GameState, damage: &DamageProposed) -> bool {
    if damage.is_combat
        || damage.amount <= 0
        || cannot_prevent(state)
        // With any matching Circle, only its observable consumption predicate
        // admits the packet. Amount changes before an inevitable identical
        // shield consumption do not introduce a redundant numeric menu.
        || circles::circle_shield_position(state, damage).is_some()
        || full_prevention(state, damage)
        || crate::eternal_pauper_pf1c_v1::ordinary_damage::prevention_without_cost(state, damage)
    {
        return false;
    }
    let powers: Vec<_> = crate::standard_cards_v1::matching_ojers(state, damage)
        .map(|(_, power)| power)
        .collect();
    let spheres =
        crate::eternal_premodern_p3a_v1::statics::applicable_spheres(state, damage).count();
    if powers.len() == 1 {
        return single_ojer_has_distinct_outcomes(damage.amount, spheres, powers[0]);
    }
    // Keep every native Ojer's multiplicity. For multiple increases this gate
    // is conservative about equivalent numeric menus, while the actual
    // frontier still applies each exact effect once and preserves all outcomes.
    let Some(power) = powers.into_iter().max() else {
        return false;
    };
    let mut amount = damage.amount;
    for _ in crate::eternal_premodern_p3a_v1::statics::applicable_spheres(state, damage) {
        if amount <= 0 {
            break;
        }
        if amount < power {
            return true;
        }
        amount = amount.saturating_sub(2);
    }
    false
}

/// Exact scalar bounds with one Ojer: its earliest and latest legal use
/// bracket every possible final amount. Zero ends the event before an unused
/// increase can revive it. All arithmetic is widened before subtraction.
fn single_ojer_has_distinct_outcomes(amount: i32, spheres: usize, power: i32) -> bool {
    let amount = i64::from(amount);
    let power = i64::from(power);
    let spheres = i64::try_from(spheres)
        .unwrap_or(i64::MAX / 4)
        .min(i64::MAX / 4);
    if amount <= 0 || spheres == 0 {
        return false;
    }
    let earliest = if amount < power {
        0
    } else {
        (amount - power) / 2 + 1
    };
    let latest = spheres.min((amount - 1) / 2);
    if earliest > latest {
        return false;
    }
    let minimum = if 2 * spheres >= amount {
        0
    } else {
        (power - 2 * (spheres - earliest)).max(0)
    };
    let maximum = (power - 2 * (spheres - latest)).max(0);
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
