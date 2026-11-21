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

fn has_color_prevention(state: &GameState) -> bool {
    state.engine.active_replacements.iter().any(|replacement| {
        matches!(
            &replacement.kind,
            ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn { turn, active_player, .. }
                if *turn == state.turn_key_v1() && *active_player == state.active_player
        )
    })
}

/// Only states with potentially competing effects borrow the enclosing operation replay.
pub(crate) fn may_need_choice(state: &GameState) -> bool {
    cfg!(feature = "eternal-fixtures")
        && !cannot_prevent(state)
        && (permanent_v1::may_need_choice(state)
            || (circles::has_live_circle_shield(state)
                && (crate::eternal_pauper_e6a_v1::has_active_global_fog(state)
                    || has_color_prevention(state)))
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

fn cannot_prevent(state: &GameState) -> bool {
    state.engine.until_end_of_turn.iter().any(|effect| {
        matches!(
            effect,
            crate::engine::UntilEndOfTurnEffect::DamageCannotBePrevented { .. }
        )
    })
}

pub(crate) fn eligible(state: &GameState, damage: &DamageProposed) -> bool {
    damage.amount > 0
        && !cannot_prevent(state)
        && (permanent_v1::eligible(state, damage)
            || (circles::circle_shield_position(state, damage).is_some()
                && full_prevention(state, damage)))
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
        let effect = match effect {
            OrdinaryV1::Registered(ref replacement)
                if matches!(
                    replacement.kind,
                    ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn { .. }
                ) =>
            {
                OrdinaryV1::PreventAll
            }
            effect => effect,
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
    options
}

/// The affected player can obtain different surviving amounts by ordering the
/// active reduction and increase. Their authoritative matchers retain source
/// color, controller, printed-ability, face and historical-source restrictions.
pub(crate) fn residual_order_applies(state: &GameState, damage: &DamageProposed) -> bool {
    if damage.is_combat
        || damage.amount <= 0
        || cannot_prevent(state)
        || full_prevention(state, damage)
        || crate::eternal_pauper_pf1c_v1::ordinary_damage::prevention_without_cost(state, damage)
    {
        return false;
    }
    let Some(power) = crate::standard_cards_v1::matching_ojers(state, damage)
        .map(|(_, power)| power)
        .max()
    else {
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
