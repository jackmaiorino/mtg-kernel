//! Ordinary replacement effects competing for a Hallow-covered damage packet.
use crate::eternal_legacy_e9a_v1::{self as e9a, RedirectV1};
use crate::eternal_sideboard_s1d_v1::{statics as circles, CopShieldV1};
use crate::event::{self, ActiveReplacement, DamageProposed, ProposedEvent};
use crate::ids::ObjectId;
use crate::state::{GameState, Target};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrdinaryV1 {
    Sphere(ObjectId),
    Ojer {
        object: ObjectId,
        power: i32,
    },
    Redirect {
        position: usize,
        record: RedirectV1,
    },
    // Fog and protection are equivalent for this packet and neither is consumed.
    PreventAll,
    Nishoba(ObjectId),
    Registered(ActiveReplacement),
    Circle {
        position: usize,
        shield: CopShieldV1,
    },
}

pub(crate) fn prevention_without_cost(state: &GameState, damage: &DamageProposed) -> bool {
    crate::eternal_pauper_e6a_v1::prevents_damage(state, damage)
        || protection_applies(state, damage)
}

pub(crate) fn protection_applies(state: &GameState, damage: &DamageProposed) -> bool {
    damage
        .historical_facts(state)
        .expect("validated historical source")
        .map_or_else(
            || {
                crate::engine::damage_is_prevented_by_protection(
                    state,
                    damage.source,
                    damage.target,
                )
            },
            |facts| match damage.target {
                Target::Object(target) => {
                    crate::source_power_activation_v1::protected(state, facts, target)
                }
                _ => false,
            },
        )
}

/// Applied static abilities follow both portions of a split event (CR 614.5).
/// Registered effects use the proposal's existing touched-by list.
#[derive(Clone)]
pub(crate) struct Packet {
    pub damage: DamageProposed,
    used: Vec<OrdinaryV1>,
}

impl Packet {
    pub fn new(damage: &DamageProposed) -> Self {
        Self {
            damage: damage.clone(),
            used: Vec::new(),
        }
    }

    pub fn options(&self, state: &GameState) -> Vec<OrdinaryV1> {
        use OrdinaryV1::*;
        let damage = &self.damage;
        let mut options = Vec::new();
        options.extend(
            crate::eternal_premodern_p3a_v1::statics::applicable_spheres(state, damage).map(Sphere),
        );
        options.extend(
            crate::standard_cards_v1::applicable_ojers(state, damage)
                .map(|(object, power)| Ojer { object, power }),
        );
        options.retain(|effect| !self.used.contains(effect));
        options.extend(
            e9a::statics::applicable_redirects(state, damage)
                .map(|(position, record)| Redirect { position, record }),
        );
        if prevention_without_cost(state, damage) {
            options.push(PreventAll);
        }
        if let Target::Object(object) = damage.target {
            if crate::eternal_premodern_p2c_v1::statics::nishoba_applies(state, damage) {
                options.push(Nishoba(object));
            }
        }
        if !state.engine.active_replacements.is_empty() {
            let proposed = ProposedEvent::Damage(damage.clone());
            options.extend(
                state
                    .engine
                    .active_replacements
                    .iter()
                    .filter(|r| {
                        !damage.touched_by.contains(&r.id)
                            && event::replacement_applies(r, &proposed, state)
                    })
                    .cloned()
                    .map(Registered),
            );
        }
        options
    }

    /// Append surviving portions to a LIFO worklist, redirected point first.
    pub fn apply(mut self, state: &mut GameState, effect: OrdinaryV1, work: &mut Vec<Self>) {
        use OrdinaryV1::*;
        match effect {
            Circle { position, shield } => {
                if !circles::consume_selected_circle_shield(state, &self.damage, position, shield) {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        self.damage.source,
                    ));
                }
                return;
            }
            Sphere(_) => {
                self.used.push(effect);
                self.damage.amount = self.damage.amount.saturating_sub(2);
            }
            Ojer { power, .. } => {
                self.used.push(effect);
                self.damage.amount = power;
            }
            Redirect { position, record } => {
                e9a::e9a_state(state).redirects.remove(position);
                if record.from.object != record.to.object {
                    let mut redirected = self.clone();
                    redirected.damage.amount = 1;
                    redirected.damage.target = Target::Object(record.to.object);
                    self.damage.amount -= 1;
                    if self.damage.amount > 0 {
                        work.push(self);
                    }
                    work.push(redirected);
                    return;
                }
            }
            PreventAll => return,
            Nishoba(_) => {
                crate::eternal_premodern_p2c_v1::statics::prevent_damage(
                    state,
                    &self.damage,
                    false,
                );
                return;
            }
            Registered(replacement) => {
                self.damage.touched_by.push(replacement.id);
                let Some(ProposedEvent::Damage(damage)) = event::replacement_apply(
                    &replacement,
                    ProposedEvent::Damage(self.damage),
                    state,
                ) else {
                    return;
                };
                self.damage = damage;
            }
        }
        if self.damage.amount > 0 {
            work.push(self);
        }
    }
}
