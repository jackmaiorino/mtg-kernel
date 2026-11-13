//! Card programs for the eternal catalog's Premodern sideboard wave P3a
//! (`docs/design/eternal_formats_support_v1.md`): the sideboard cards of
//! Stiflenought, Sligh, Goblins, Oath of Druids, RG Aggro, Stasis and White
//! Weenie.
//!
//! `eternal_cards_v1` forwards its effect leaves and queued triggers here,
//! wave P2b's recipe tables fall through to `build_eternal_p3a_v1.rs`, and
//! the engine calls the hooks in `statics` only in `eternal-fixtures`
//! builds. Every table is keyed by a printed name that occurs only in the
//! eternal registry. Behavior follows each card's current Oracle text,
//! checked against its XMage source.
//!
//! Primitives added here:
//! - "destroy all white permanents" and "destroy all non-Aura enchantments"
//!   (Anarchy, Tranquil Domain) as one simultaneous destruction;
//! - triggers on any player's spell by its mana value (Pyrostatic Pillar)
//!   and on an opponent's red spell (Warmth);
//! - an Aura whose trigger fires in the upkeep of the enchanted creature's
//!   controller and puts a -0/-1 counter on that creature (Essence Flare);
//! - protection from red until end of turn for any target creature
//!   (Crimson Acolyte), as one of wave E9a's protection records;
//! - in `statics`: a tax on red spells by the cast form's color (Chill), a
//!   prohibition on creatures' activated abilities (Cursed Totem), player
//!   shroud (True Believer), printed protection from red (Crimson Acolyte)
//!   and prevention of part of each red source's damage (Sphere of Law).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod statics;

use crate::card_def::{CardType, Subtype, TargetSpec};
use crate::effect::{EffectCond, EffectOp, ExecCtx, ObjectRef, PlayerRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{active_permanents, battlefield, live_link, TurnV1};
use crate::eternal_premodern_p1a_v1::{
    destroy_batch, halt, has_type, legal_object_target, queue_trigger,
};
use crate::event::{self, CommittedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardOpV1;
use crate::standard_creatures_v1::CounterTransferV1;
use crate::state::{AbilitySourceContractV4, Counters, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};

pub(crate) const CHILL: &str = "Chill";
pub(crate) const CRIMSON_ACOLYTE: &str = "Crimson Acolyte";
pub(crate) const CURSED_TOTEM: &str = "Cursed Totem";
pub(crate) const ESSENCE_FLARE: &str = "Essence Flare";
pub(crate) const PYROSTATIC_PILLAR: &str = "Pyrostatic Pillar";
pub(crate) const SPHERE_OF_LAW: &str = "Sphere of Law";
pub(crate) const TRUE_BELIEVER: &str = "True Believer";
pub(crate) const WARMTH: &str = "Warmth";

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::P3aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P3aOpV1 {
    /// Anarchy: destroy all white permanents.
    AnarchyV1,
    /// Tranquil Domain: destroy all non-Aura enchantments.
    TranquilDomainV1,
    /// Crimson Acolyte: target creature 0 gains protection from red until
    /// end of turn.
    AcolyteProtectionV1,
    /// Pyrostatic Pillar: 2 damage to `player`, who cast the spell.
    PillarDamageV1 { player: PlayerId },
    /// Essence Flare: a -0/-1 counter on `host`, the creature the Aura
    /// enchanted as its upkeep began.
    FlareCounterV1 { host: ObjectLinkV4 },
}

impl P3aOpV1 {
    /// Whether this leaf can yield a player decision by itself.
    pub(crate) fn contains_player_choice(&self) -> bool {
        false
    }
}

fn p3a(op: P3aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P3aV1(op)))
}

// ---- Programs -----------------------------------------------------------------------

/// Anarchy: "Destroy all white permanents."
pub fn anarchy() -> EffectOp {
    p3a(P3aOpV1::AnarchyV1)
}

/// Tranquil Domain: "Destroy all non-Aura enchantments."
pub fn tranquil_domain() -> EffectOp {
    p3a(P3aOpV1::TranquilDomainV1)
}

/// Overload: "Destroy target artifact if its mana value is 2 or less. If
/// this spell was kicked, destroy that artifact if its mana value is 5 or
/// less instead." Both checks read the artifact's mana value on resolution.
pub fn overload() -> EffectOp {
    let destroy_at_most = |maximum: u16| EffectOp::Conditional {
        cond: EffectCond::TargetManaValueAtMost(0, maximum),
        then: Box::new(EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    };
    EffectOp::Conditional {
        cond: EffectCond::WasKicked,
        then: Box::new(destroy_at_most(5)),
        else_: Box::new(destroy_at_most(2)),
    }
}

/// Crimson Acolyte: "{W}: Target creature gains protection from red until
/// end of turn."
pub fn crimson_acolyte() -> EffectOp {
    p3a(P3aOpV1::AcolyteProtectionV1)
}

/// Warmth's trigger: "you gain 2 life."
fn warmth_life() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 2,
    }
}

// ---- Triggered abilities ----------------------------------------------------------------

/// Target specs of this module's queued triggers: none of them targets.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if name == WARMTH {
        return *effect == warmth_life();
    }
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P3aV1(op))) = effect else {
        return false;
    };
    match name {
        PYROSTATIC_PILLAR => matches!(op, P3aOpV1::PillarDamageV1 { .. }),
        ESSENCE_FLARE => matches!(op, P3aOpV1::FlareCounterV1 { .. }),
        _ => false,
    }
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &P3aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        P3aOpV1::AnarchyV1 => {
            // Each permanent's current colors: Painter's Servant's and Wild
            // Mongrel's count.
            let white = crate::card_def::mana_color_mask(ManaColor::W);
            let victims = battlefield(state)
                .filter(|&object| crate::engine::object_color_mask(state, object) & white != 0)
                .collect::<Vec<_>>();
            destroy_batch(state, victims);
        }
        P3aOpV1::TranquilDomainV1 => {
            // Current types and subtypes: Opalescence's animated enchantments
            // are destroyed, and an Aura that stopped being one is too.
            let victims = battlefield(state)
                .filter(|&object| {
                    has_type(state, object, CardType::Enchantment)
                        && !crate::engine::has_effective_subtype(state, object, Subtype::Aura)
                })
                .collect::<Vec<_>>();
            destroy_batch(state, victims);
        }
        P3aOpV1::AcolyteProtectionV1 => {
            let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::Creature) else {
                return;
            };
            let record = (live_link(state, creature), ManaColor::R, TurnV1::now(state));
            let protections = &mut crate::eternal_legacy_e9a_v1::e9a_state(state).protections;
            if !protections.contains(&record) {
                protections.push(record);
            }
        }
        P3aOpV1::PillarDamageV1 { player } => {
            // The Pillar deals the damage even if it has left the battlefield.
            event::propose_and_commit(
                state,
                event::damage_from_exec_ctx_v1(ctx, Target::Player(*player), 2),
            );
        }
        P3aOpV1::FlareCounterV1 { host } => {
            // "That creature": the one enchanted as the trigger fired, if it
            // is still on the battlefield, wherever the Aura is now.
            let current = state.objects.try_get(host.object).is_some_and(|live| {
                live.zone == Zone::Battlefield && live.zone_change_count == host.zone_change_count
            });
            if !current {
                return;
            }
            let counter = CounterTransferV1 {
                counters: Counters {
                    minus0_minus1: 1,
                    ..Counters::default()
                },
                ..CounterTransferV1::default()
            };
            if crate::standard_creatures_v1::place_counters_checked_v1(
                state,
                host.object,
                ctx.controller,
                counter,
            )
            .is_none()
            {
                halt(state, ctx.source);
            }
        }
    }
}

// ---- Hooks ----------------------------------------------------------------------------

/// The triggers this module's cards set off in this collection, queued into
/// wave P1a's queue.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    for event in events {
        match *event {
            CommittedEvent::SpellCast { spell, controller } => {
                note_spell_cast(state, spell, controller);
            }
            CommittedEvent::UpkeepBegan { player } => note_upkeep(state, player),
            _ => {}
        }
    }
    Vec::new()
}

/// Pyrostatic Pillar: "Whenever a player casts a spell with mana value 3 or
/// less, Pyrostatic Pillar deals 2 damage to that player." Warmth:
/// "Whenever an opponent casts a red spell, you gain 2 life." Both read the
/// spell on the stack after its costs are paid: a split half's own mana
/// value and colors, its announced X included. Copies are not cast.
fn note_spell_cast(state: &mut GameState, spell: ObjectId, caster: PlayerId) {
    if crate::engine::object_mana_value(state, spell) <= 3 {
        let pillars = active_permanents(state, PYROSTATIC_PILLAR, None).collect::<Vec<_>>();
        for pillar in pillars {
            let source = AbilitySourceContractV4::capture(state, pillar);
            let controller = state.objects.get(pillar).controller;
            queue_trigger(
                state,
                source,
                controller,
                p3a(P3aOpV1::PillarDamageV1 { player: caster }),
                None,
            );
        }
    }
    let red = crate::card_def::mana_color_mask(ManaColor::R);
    if crate::engine::object_color_mask(state, spell) & red != 0 {
        let opponent = caster.opponent();
        let warmths = active_permanents(state, WARMTH, Some(opponent)).collect::<Vec<_>>();
        for warmth in warmths {
            let source = AbilitySourceContractV4::capture(state, warmth);
            queue_trigger(state, source, opponent, warmth_life(), None);
        }
    }
}

/// Essence Flare: "At the beginning of the upkeep of enchanted creature's
/// controller, put a -0/-1 counter on that creature." The Aura's controller
/// controls the trigger, which is bound to the creature it enchants now.
fn note_upkeep(state: &mut GameState, player: PlayerId) {
    let flares = active_permanents(state, ESSENCE_FLARE, None).collect::<Vec<_>>();
    for flare in flares {
        let Some(host) = state.objects.get(flare).v4.attached_to else {
            continue;
        };
        let enchanted = state.objects.try_get(host.object).is_some_and(|live| {
            live.zone == Zone::Battlefield
                && live.zone_change_count == host.zone_change_count
                && live.controller == player
        });
        if !enchanted {
            continue;
        }
        let source = AbilitySourceContractV4::capture(state, flare);
        let controller = state.objects.get(flare).controller;
        queue_trigger(
            state,
            source,
            controller,
            p3a(P3aOpV1::FlareCounterV1 { host }),
            None,
        );
    }
}
