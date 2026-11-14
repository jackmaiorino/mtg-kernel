//! Card programs for the eternal catalog's Legacy sideboard wave S1m
//! (`docs/design/eternal_formats_support_v1.md`): Extinguisher Battleship,
//! the only gap of Mystic Forge Combo, and Fiery Confluence, the only gap of
//! Red Stompy.
//!
//! `eternal_cards_v1` forwards its effect leaves and the Battleship's entry
//! trigger here, wave S1l's recipe tables fall through to
//! `build_eternal_s1m_v1.rs`, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry. Behavior follows the cards' current
//! Oracle text, the comprehensive rules for station (702.184, 721) and the
//! rulings, checked against the XMage sources.
//!
//! Primitives added here: station (702.184a), whose cost is
//! `CostComponent::TapOtherUntappedControlledCreatureV1`, and a station
//! symbol's characteristics (721.2b): creature status, base power and
//! toughness and keywords from charge counters alone.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::{Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{CreatureFilter, EffectOp, ExecCtx, ObjectRef, TargetRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e5b_v1::CONFLUENCE_MODES;
use crate::ids::ObjectId;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{GameState, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const EXTINGUISHER_BATTLESHIP: &str = "Extinguisher Battleship";

/// "STATION 5+": the charge counters at which the Battleship is a creature.
const STATION_THRESHOLD: i32 = 5;
/// The station symbol's base power and toughness.
const STATION_POWER_TOUGHNESS: (i32, i32) = (10, 10);
/// The damage the Battleship's entry trigger deals to each creature.
const BATTLESHIP_DAMAGE: i32 = 4;

/// The Battleship entry trigger's target: "target noncreature permanent".
pub(crate) const BATTLESHIP_TARGET: TargetSpec =
    TargetSpec::StandardV1(StandardTargetV1::NoncreaturePermanentV1);

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::S1mV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1mOpV1 {
    /// Extinguisher Battleship's entry trigger: destroy target noncreature
    /// permanent, then 4 damage to each creature.
    BattleshipEntersV1,
    /// Station: charge counters equal to the tapped creature's power.
    StationV1,
    /// Fiery Confluence with this multiset of modes (0: 1 damage to each
    /// creature, 1: 2 damage to each opponent, 2: destroy target artifact),
    /// in printed order.
    FieryConfluenceV1 { modes: [u8; 3] },
}

fn s1m(op: S1mOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1mV1(op)))
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

// ---- Programs -----------------------------------------------------------------------

/// Extinguisher Battleship: "When this Spacecraft enters, destroy target
/// noncreature permanent. Then this Spacecraft deals 4 damage to each
/// creature."
pub fn battleship_enters() -> EffectOp {
    s1m(S1mOpV1::BattleshipEntersV1)
}

/// Extinguisher Battleship's station: "Tap another untapped creature you
/// control: Put a number of charge counters on this permanent equal to the
/// tapped creature's power. Activate only as a sorcery." (702.184a)
pub fn station() -> EffectOp {
    s1m(S1mOpV1::StationV1)
}

fn fiery_confluence(index: usize) -> EffectOp {
    s1m(S1mOpV1::FieryConfluenceV1 {
        modes: CONFLUENCE_MODES[index],
    })
}

/// Fiery Confluence: "Choose three. You may choose the same mode more than
/// once. - Fiery Confluence deals 1 damage to each creature. - Fiery
/// Confluence deals 2 damage to each opponent. - Destroy target artifact."
/// Mode choice `n` is the multiset `CONFLUENCE_MODES[n]`.
pub fn fiery_confluence_0() -> EffectOp {
    fiery_confluence(0)
}
pub fn fiery_confluence_1() -> EffectOp {
    fiery_confluence(1)
}
pub fn fiery_confluence_2() -> EffectOp {
    fiery_confluence(2)
}
pub fn fiery_confluence_3() -> EffectOp {
    fiery_confluence(3)
}
pub fn fiery_confluence_4() -> EffectOp {
    fiery_confluence(4)
}
pub fn fiery_confluence_5() -> EffectOp {
    fiery_confluence(5)
}
pub fn fiery_confluence_6() -> EffectOp {
    fiery_confluence(6)
}
pub fn fiery_confluence_7() -> EffectOp {
    fiery_confluence(7)
}
pub fn fiery_confluence_8() -> EffectOp {
    fiery_confluence(8)
}
pub fn fiery_confluence_9() -> EffectOp {
    fiery_confluence(9)
}

/// The target filter of Fiery Confluence's mode multiset `modes`: one
/// "target artifact" slot per destroy mode.
pub(crate) fn fiery_confluence_target(modes: [u8; 3]) -> TargetSpec {
    match modes.iter().filter(|&&mode| mode == 2).count() as u8 {
        0 => TargetSpec::None,
        slots => TargetSpec::StandardV1(StandardTargetV1::RepeatableArtifactsV1(slots)),
    }
}

// ---- Triggered abilities ------------------------------------------------------------

const BATTLESHIP_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: battleship_enters,
}];

/// Triggered abilities of this module's cards, falling through to wave P3h.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        EXTINGUISHER_BATTLESHIP => &BATTLESHIP_TRIGGERS,
        _ => crate::eternal_premodern_p3h_v1::triggers_for(name),
    }
}

/// Target specs of this module's triggered abilities: the Battleship's
/// entry trigger targets a noncreature permanent.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    (name == EXTINGUISHER_BATTLESHIP && *effect == battleship_enters()).then_some(BATTLESHIP_TARGET)
}

// ---- Station characteristics (721.2b) -----------------------------------------------

/// Whether `object` is an Extinguisher Battleship on the battlefield with
/// five or more charge counters. It doesn't read the object's abilities, so
/// that types and abilities don't depend on each other: the station symbol
/// started to apply in layer 4, so an effect that removes its abilities in
/// layer 6 leaves it a creature with its base power and toughness (613.6).
pub(crate) fn stationed(state: &GameState, object: ObjectId) -> bool {
    cfg!(feature = "eternal-fixtures")
        && state.objects.try_get(object).is_some_and(|live| {
            live.zone == Zone::Battlefield
                && live.counters.charge >= STATION_THRESHOLD
                && live.v4.face_down_v1.is_none()
                && live.v4.face_index == 0
                && CARD_DEFS
                    .get(usize::from(live.card_def))
                    .is_some_and(|def| def.name == EXTINGUISHER_BATTLESHIP && def.is_executable())
        })
}

/// The station symbol's base power and toughness, 10/10. The engine asks
/// it after every other base-setting effect: each of those in the registry
/// (Karn, the Great Creator's +1, the Tezzeret emblem's Robot, Oko's Elk,
/// Azure Beastbinder, Unable to Scream) starts after the Battleship entered,
/// so it wins by timestamp (613.7), as the rulings require for Karn and the
/// Tezzeret emblem.
pub(crate) fn station_base_power_toughness(
    state: &GameState,
    object: ObjectId,
) -> Option<(i32, i32)> {
    stationed(state, object).then_some(STATION_POWER_TOUGHNESS)
}

/// The station symbol's flying and trample. They are granted with the
/// Battleship's timestamp, so an ability-removing effect with a later
/// timestamp (Dress Down, Azure Beastbinder, Unable to Scream, Oko's Elk)
/// removes them and an earlier one doesn't.
pub(crate) fn granted_keywords(state: &GameState, object: ObjectId) -> Keywords {
    if !stationed(state, object) {
        return Keywords::NONE;
    }
    let timestamp = state.objects.get(object).v4.layer_timestamp.unwrap_or(0);
    if crate::continuous_characteristics_v1::grant_survives(state, object, timestamp) {
        Keywords::FLYING | Keywords::TRAMPLE
    } else {
        Keywords::NONE
    }
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves.
pub(crate) fn execute(op: &S1mOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        S1mOpV1::BattleshipEntersV1 => battleship_enters_resolve(ctx, state),
        S1mOpV1::StationV1 => station_resolve(ctx, state),
        S1mOpV1::FieryConfluenceV1 { modes } => fiery_confluence_resolve(ctx, state, *modes),
    }
}

/// Destroys `object` through the engine's destroy leaf (indestructible and
/// regeneration apply).
fn destroy(ctx: &ExecCtx, state: &mut GameState, object: ObjectId) {
    let mut ctx = ctx.clone();
    ctx.targets = vec![Target::Object(object)];
    ctx.target_contracts = Vec::new();
    crate::effect::execute(
        &EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        },
        &ctx,
        state,
    );
}

fn damage_each_creature(ctx: &ExecCtx, state: &mut GameState, amount: i32) {
    crate::effect::execute(
        &EffectOp::DamageAllCreatures {
            filter: CreatureFilter::All,
            amount,
        },
        ctx,
        state,
    );
}

/// The entry trigger's resolution. Its only target is judged from the
/// trigger's frozen source; if it is illegal, the ability doesn't resolve
/// and deals no damage (608.2b, the card's ruling). The Battleship deals the
/// damage even if it has left, as it last existed.
fn battleship_enters_resolve(ctx: &ExecCtx, state: &mut GameState) {
    let Some(target) = crate::eternal_premodern_p1b_v1::legal_object_target_from_ability_source(
        ctx,
        state,
        0,
        BATTLESHIP_TARGET,
    ) else {
        return;
    };
    destroy(ctx, state, target);
    damage_each_creature(ctx, state, BATTLESHIP_DAMAGE);
}

/// Station's resolution. If the Battleship is still the incarnation that
/// activated, it gets charge counters equal to the tapped creature's power
/// now, or as that creature last existed on the battlefield, and none if
/// that power is 0 or less (rulings). The cost recorded that power, which
/// the engine refreshes as the creature leaves.
fn station_resolve(ctx: &ExecCtx, state: &mut GameState) {
    let [paid] = ctx.paid_cost_refs.as_slice() else {
        halt(state, ctx.source);
        return;
    };
    let Some(last_known) = paid.power_lki else {
        halt(state, ctx.source);
        return;
    };
    let Some(battleship) = crate::eternal_premodern_p1a_v1::live_ability_source(ctx, state) else {
        return;
    };
    let power = match state.objects.try_get(paid.object) {
        Some(live)
            if live.zone == Zone::Battlefield
                && live.zone_change_count == paid.zone_change_count =>
        {
            crate::engine::effective_power(state, paid.object)
        }
        _ => last_known,
    };
    if power <= 0 {
        return;
    }
    let counters = &mut state.objects.get_mut(battleship).counters.charge;
    *counters = counters.saturating_add(power);
}

/// Fiery Confluence's resolution: the chosen modes in printed order, each
/// damage mode chosen more than once a separate damage event, each destroy
/// mode its own target slot. An illegal artifact is skipped; if every
/// target is illegal, the engine doesn't resolve the spell (608.2b).
fn fiery_confluence_resolve(ctx: &ExecCtx, state: &mut GameState, modes: [u8; 3]) {
    let spec = fiery_confluence_target(modes);
    let mut ordered = modes;
    ordered.sort_unstable();
    let mut artifact_slot = 0usize;
    for mode in ordered {
        match mode {
            0 => damage_each_creature(ctx, state, 1),
            1 => crate::effect::execute(
                &EffectOp::DealDamage {
                    target: TargetRef::Opponent,
                    amount: 2,
                },
                ctx,
                state,
            ),
            _ => {
                let slot = artifact_slot;
                artifact_slot += 1;
                if let Some(artifact) =
                    crate::eternal_premodern_p1a_v1::legal_object_target(ctx, state, slot, spec)
                {
                    destroy(ctx, state, artifact);
                }
            }
        }
    }
}
