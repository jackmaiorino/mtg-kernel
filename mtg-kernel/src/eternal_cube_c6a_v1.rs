//! Vintage Cube wave C6a: discard, exile and impulse-draw trigger creatures.
//! Inti remains unsupported until all multi-card discard callers preserve
//! instruction boundaries: ordinary cleanup, Time Stop cleanup, Breakthrough,
//! and Cabal Therapy's legacy fallback still loop over single-card commits.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectOp, ExecCtx, ImpulseDuration, ObjectRef, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::{EternalOpV1, EternalTriggerV1};
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTriggerV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Gau's intervening condition and resolution-time power.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C6aOpV1 {
    GauDamageV1,
}

impl C6aOpV1 {
    pub(crate) fn contains_player_choice(&self) -> bool {
        false
    }
}

const IVORA: &str = "Ivora, Insatiable Heir";
const GAU: &str = "Gau, Feral Youth";
const LAELIA: &str = "Laelia, the Blade Reforged";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C6aTriggerV1 {
    EachEndStepIfCardLeftYourGraveyardV1,
    CardsExiledFromYourLibraryOrGraveyardV1,
}

/// Graveyard departures must be remembered even before Gau enters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C6aStateV1 {
    pub graveyard_departures: [Option<TurnV1>; 2],
    /// Ouroboroid's incarnation-scoped last-known-power convention.
    pub departed_power: Vec<(ObjectLinkV4, i32)>,
}

impl C6aStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Called after replacement selection, before the committed zone change.
/// `frozen_power` preserves the pre-batch value for simultaneous departures.
pub(crate) fn note_zone_change(
    state: &mut GameState,
    object: ObjectId,
    from: Zone,
    to: Zone,
    frozen_power: Option<i32>,
) {
    if from == to || !matches!(from, Zone::Graveyard | Zone::Battlefield) {
        return;
    }
    let live = state.objects.get(object);
    if from == Zone::Graveyard && !live.v4.is_token && live.spell_copy_origin.is_none() {
        let owner = live.owner;
        // A Gau anywhere in the game can enter after this departure.
        if state.objects.iter().any(|(_, card)| {
            let def = &CARD_DEFS[usize::from(card.card_def)];
            def.name == GAU && def.is_executable()
        }) {
            let now = TurnV1::now(state);
            state
                .eternal_v1
                .get_or_insert_with(Default::default)
                .c6a
                .graveyard_departures[owner.index()] = Some(now);
        }
    } else if from == Zone::Battlefield && CARD_DEFS[usize::from(live.card_def)].name == GAU {
        let link = ObjectLinkV4 {
            object,
            zone_change_count: live.zone_change_count,
        };
        let power = frozen_power.unwrap_or_else(|| crate::engine::effective_power(state, object));
        let entries = &mut state
            .eternal_v1
            .get_or_insert_with(Default::default)
            .c6a
            .departed_power;
        if !entries.iter().any(|(old, _)| *old == link) {
            entries.push((link, power));
        }
    }
}

fn card_left_graveyard(state: &GameState, player: PlayerId) -> bool {
    state.eternal_v1.as_ref().is_some_and(|eternal| {
        eternal.c6a.graveyard_departures[player.index()] == Some(TurnV1::now(state))
    })
}

/// Freeze Gau before any other simultaneous departure removes its boosts.
pub(crate) fn before_batch_departures(state: &mut GameState, events: &[ProposedEvent]) {
    let departing = events
        .iter()
        .filter_map(|event| {
            let ProposedEvent::ZoneChange(change) = event else {
                return None;
            };
            let live = state.objects.get(change.object);
            if live.zone != Zone::Battlefield
                || change.to_zone == Zone::Battlefield
                || CARD_DEFS[usize::from(live.card_def)].name != GAU
            {
                return None;
            }
            Some((
                change.object,
                change.to_zone,
                crate::engine::effective_power(state, change.object),
            ))
        })
        .collect::<Vec<_>>();
    for (object, to, power) in departing {
        note_zone_change(state, object, Zone::Battlefield, to, Some(power));
    }
}

fn gau_damage() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C6aV1(
        C6aOpV1::GauDamageV1,
    )))
}

/// Experimental Synthesizer's identical impulse permission.
fn laelia_impulse() -> EffectOp {
    EffectOp::ImpulseDraw {
        count: 1,
        duration: ImpulseDuration::EndOfTurn,
    }
}

const fn condition(kind: C6aTriggerV1) -> TriggerCondition {
    TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(EternalTriggerV1::C6aV1(kind)))
}

fn qualifying_exile(state: &GameState, event: &CommittedEvent, player: PlayerId) -> bool {
    let CommittedEvent::ZoneChange {
        object,
        from: Zone::Library | Zone::Graveyard,
        to: Zone::Exile,
        ..
    } = event
    else {
        return false;
    };
    state.objects.try_get(*object).is_some_and(|card| {
        card.owner == player && !card.v4.is_token && card.spell_copy_origin.is_none()
    })
}

pub(crate) fn trigger_matches(
    kind: C6aTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    // The Eternal trigger dispatcher already checks executable battlefield
    // presence and that the source's printed abilities remain active.
    let controller = state.objects.get(source).controller;
    match kind {
        C6aTriggerV1::EachEndStepIfCardLeftYourGraveyardV1 => {
            matches!(events[index], CommittedEvent::BeginningEndStep { .. })
                && card_left_graveyard(state, controller)
        }
        C6aTriggerV1::CardsExiledFromYourLibraryOrGraveyardV1 => {
            // Moonshadow's Full-card convention: one trigger per collected
            // event batch, including both source zones in the same batch.
            // Separate instructions collected together are also coalesced.
            qualifying_exile(state, &events[index], controller)
                && !events[..index]
                    .iter()
                    .any(|event| qualifying_exile(state, event, controller))
        }
    }
}

/// Put the resolving creature spell onto the battlefield.
pub fn permanent() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::ThisSource,
        to_zone: Zone::Battlefield,
    }
}

/// Bloodtithe Harvester's entry effect, also used for combat player damage.
fn blood() -> EffectOp {
    EffectOp::CreateToken {
        token_def: card_id_by_name("Blood Token").expect("registered Blood Token"),
        controller: PlayerRef::Controller,
    }
}

fn ivora_counter() -> EffectOp {
    EffectOp::AddPlusOneCounterToAbilitySource
}

/// Alesha's attack-counter binding also works if the source stops being a
/// creature before resolution, and never follows it through a zone change.
fn source_counter() -> EffectOp {
    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
}

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

const IVORA_TRIGGERS: [TriggeredAbilityDef; 3] = [
    trigger(TriggerCondition::Etb, blood),
    trigger(TriggerCondition::DealsCombatDamageToPlayer, blood),
    trigger(TriggerCondition::ControllerDiscardsCardV1, ivora_counter),
];

const GAU_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Attacks, source_counter),
    trigger(
        condition(C6aTriggerV1::EachEndStepIfCardLeftYourGraveyardV1),
        gau_damage,
    ),
];

const LAELIA_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Attacks, laelia_impulse),
    trigger(
        condition(C6aTriggerV1::CardsExiledFromYourLibraryOrGraveyardV1),
        source_counter,
    ),
];

/// Capture observers before the discard moves any card, as Currency Converter
/// does. This counts cards regardless of their replacement destination and
/// retains each observer's original controller and battlefield incarnation.
/// These triggers disclose no identity from the discarded hand.
pub(crate) fn note_discards(state: &mut GameState, cards: &[ObjectId]) {
    for player in [PlayerId::P0, PlayerId::P1] {
        let sources = crate::eternal_e5a_v1::active_permanents(state, IVORA, Some(player))
            .map(|object| AbilitySourceContractV4::capture(state, object))
            .collect::<Vec<_>>();
        if sources.is_empty() {
            continue;
        }
        let count = cards
            .iter()
            .filter(|&&card| {
                state.players[player.index()].hand.contains(&card)
                    && state.objects.get(card).zone == Zone::Hand
            })
            .count();
        for source in sources {
            for _ in 0..count {
                state
                    .engine
                    .pending_triggers
                    .push(crate::eternal_cards_v1::delayed_trigger(
                        source,
                        player,
                        ivora_counter(),
                    ));
            }
        }
    }
}

pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        IVORA => &IVORA_TRIGGERS,
        GAU => &GAU_TRIGGERS,
        LAELIA => &LAELIA_TRIGGERS,
        _ => crate::eternal_cube_c6b_v1::triggers_for(name),
    }
}

pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    let known = match name {
        IVORA => *effect == blood() || *effect == ivora_counter(),
        GAU | LAELIA => {
            (name == GAU && *effect == gau_damage())
                || (name == LAELIA && *effect == laelia_impulse())
                || matches!(
                    effect,
                    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
                        | EffectOp::PutPlusOnePlusOneCounterOnBoundObject { .. }
                )
        }
        _ => false,
    };
    known.then_some(TargetSpec::None)
}

pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    name == IVORA && *effect == ivora_counter()
}

pub(crate) fn execute(op: &C6aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C6aOpV1::GauDamageV1 => {
            if !card_left_graveyard(state, ctx.controller) {
                return;
            }
            let Some(source) = ctx.ability_source_contract else {
                return;
            };
            let link = ObjectLinkV4 {
                object: source.source,
                zone_change_count: source.zone_change_count,
            };
            let current = state.objects.try_get(link.object).is_some_and(|live| {
                live.zone == Zone::Battlefield && live.zone_change_count == link.zone_change_count
            });
            let power = if current {
                crate::engine::effective_power(state, link.object)
            } else {
                state
                    .eternal_v1
                    .as_ref()
                    .and_then(|eternal| {
                        eternal
                            .c6a
                            .departed_power
                            .iter()
                            .find(|(old, _)| *old == link)
                    })
                    .map_or(0, |(_, power)| *power)
            };
            if power > 0 {
                crate::effect::execute(
                    &EffectOp::DealDamage {
                        target: TargetRef::Opponent,
                        amount: power,
                    },
                    ctx,
                    state,
                );
            }
        }
    }
}

pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C6aOpV1,
    _path: Vec<u16>,
) -> Result<bool, String> {
    execute(op, &continuation.ctx, state);
    Ok(false)
}
