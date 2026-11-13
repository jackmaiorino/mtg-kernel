//! Card programs for the eternal catalog's Premodern sideboard wave P3c
//! (`docs/design/eternal_formats_support_v1.md`): the sideboard cards of
//! Psychatog, Elves and Reanimator.
//!
//! `eternal_cards_v1` forwards its effect leaves here, wave P3b's triggered
//! ability table falls through to `triggers_for` below, wave P3b's recipe
//! tables fall through to `build_eternal_p3c_v1.rs`, and the engine calls
//! the hooks in `statics` only in `eternal-fixtures` builds. Every table is
//! keyed by a printed name that occurs only in the eternal registry.
//! Behavior follows each card's current Oracle text and rulings.
//!
//! Primitives added here:
//! - gain control of target creature with mana value X or less (Dominate),
//!   with Blue Sun's Twilight's X-bounded cast target;
//! - destroy target artifact, which can't be regenerated, and its
//!   controller gains life equal to its mana value (Crumble);
//! - in `statics`: printed protection from white (Ihsan's Shade) and from
//!   all colors (Iridescent Angel), Multani, Maro-Sorcerer's
//!   characteristic-defining power and toughness, and Dominate's cast
//!   target hooks.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod statics;

use crate::card_def::TargetSpec;
use crate::effect::{EffectOp, ExecCtx, ObjectRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{live_link, TurnV1};
use crate::eternal_premodern_p1a_v1::{destroy, forbid_regeneration, legal_object_target};
use crate::event::{self, ProposedEvent};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const DOMINATE: &str = "Dominate";
pub(crate) const IHSANS_SHADE: &str = "Ihsan's Shade";
pub(crate) const IRIDESCENT_ANGEL: &str = "Iridescent Angel";
pub(crate) const MULTANI: &str = "Multani, Maro-Sorcerer";
pub(crate) const SPIKE_FEEDER: &str = "Spike Feeder";
pub(crate) const UKTABI_ORANGUTAN: &str = "Uktabi Orangutan";

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::P3cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P3cOpV1 {
    /// Dominate: gain control of target creature 0 if its mana value is at
    /// most the announced X.
    DominateV1,
    /// Crumble: destroy target artifact 0, which can't be regenerated. Its
    /// controller gains life equal to its mana value.
    CrumbleV1,
}

impl P3cOpV1 {
    /// Whether this leaf can yield a player decision or pushes frames. None
    /// of this module's leaves does.
    pub(crate) fn contains_player_choice(&self) -> bool {
        false
    }
}

fn p3c(op: P3cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P3cV1(op)))
}

// ---- Programs -----------------------------------------------------------------------

/// Dominate: "Gain control of target creature with mana value X or less."
/// The cast target hooks in `statics` bound the target by a payable X.
pub fn dominate() -> EffectOp {
    p3c(P3cOpV1::DominateV1)
}

/// Crumble: "Destroy target artifact. It can't be regenerated. That
/// artifact's controller gains life equal to its mana value."
pub fn crumble() -> EffectOp {
    p3c(P3cOpV1::CrumbleV1)
}

// ---- Triggered abilities ----------------------------------------------------------------

const fn trigger(condition: TriggerCondition, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

/// Uktabi Orangutan: "When Uktabi Orangutan enters, destroy target
/// artifact."
fn orangutan_destroy() -> EffectOp {
    EffectOp::DestroyObject {
        object: ObjectRef::Target(0),
    }
}

const UKTABI_ORANGUTAN_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, orangutan_destroy)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        UKTABI_ORANGUTAN => &UKTABI_ORANGUTAN_TRIGGERS,
        // Vintage sideboard wave S1e's printed triggers.
        _ => crate::eternal_sideboard_s1e_v1::triggers_for(name),
    }
}

/// Target specs of this module's triggered abilities. `None` for every
/// other card.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if !triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
    {
        return None;
    }
    Some(match name {
        UKTABI_ORANGUTAN => TargetSpec::ArtifactPermanent,
        _ => TargetSpec::None,
    })
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &P3cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        P3cOpV1::DominateV1 => {
            let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::Creature) else {
                return;
            };
            // "With mana value X or less" belongs to the target, so it is
            // checked again on resolution (608.2b).
            if crate::engine::object_mana_value(state, creature) > ctx.x_value {
                return;
            }
            crate::standard_cards_v1::gain_control(state, creature, ctx.controller);
        }
        P3cOpV1::CrumbleV1 => {
            let Some(artifact) = legal_object_target(ctx, state, 0, TargetSpec::ArtifactPermanent)
            else {
                return;
            };
            // "That artifact's controller" and "its mana value" as it is
            // destroyed, read first. An artifact that survives (indestructible)
            // still gives the life.
            let controller = state.objects.get(artifact).controller;
            let mana_value = crate::engine::object_mana_value(state, artifact);
            // "It can't be regenerated" applies to this destruction only, as
            // for Pillage (wave P2d).
            let record = (live_link(state, artifact), TurnV1::now(state));
            let added =
                !crate::eternal_premodern_p1a_v1::statics::cant_be_regenerated(state, artifact);
            if added {
                forbid_regeneration(state, artifact);
            }
            destroy(ctx, state, artifact);
            if added {
                crate::eternal_premodern_p1a_v1::p1a_state(state)
                    .no_regeneration
                    .retain(|entry| *entry != record);
            }
            if state.engine.halted.is_none() && mana_value > 0 {
                event::propose_and_commit(
                    state,
                    ProposedEvent::life_gain(controller, i32::from(mana_value)),
                );
            }
        }
    }
}
