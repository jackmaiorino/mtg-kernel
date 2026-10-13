//! Prepared event-time Panda roots. No printing or consumer is admitted here.
use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ExecCtx};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::{AbilitySourceContractV4, GameState, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};

pub const PRINTED_COST: Cost = Cost {
    generic: 2,
    pips: &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::B)],
    x_count: 0,
};
pub fn life_recipe() -> EffectOp {
    EffectOp::FiendishPandaCounterFromCaptureV1 {
        capture_history_index: u64::MAX,
    }
}
pub fn death_recipe() -> EffectOp {
    EffectOp::FiendishPandaReturnFromCaptureV1 {
        capture_history_index: u64::MAX,
        maximum: 0,
    }
}
pub const TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::ControllerGainsLife,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: life_recipe,
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefieldToGraveyard,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: death_recipe,
    },
];

pub(crate) fn owns_definition(id: u16) -> bool {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return false;
    }
    CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
        d.has_full_support()
            && d.name == "Fiendish Panda"
            && d.cost == PRINTED_COST
            && d.mana_value == 4
            && d.types == [CardType::Creature]
            && d.colors == [ManaColor::W, ManaColor::B]
            && d.subtypes == [Subtype::Bear, Subtype::Demon]
            && d.supertypes.is_empty()
            && d.power == Some(3)
            && d.toughness == Some(2)
            && d.keywords == Keywords::NONE
            && !d.is_token
            && !d.is_land
            && (d.spell_effect)().is_none()
            && d.target_spec == TargetSpec::None
            && d.activated_abilities.is_empty()
            && d.additional_cost.is_none()
            && d.alt_cost.is_none()
            && d.optional_additional_cost.is_none()
            && d.kicker_cost.is_none()
            && d.flashback.is_none()
            && d.escape.is_none()
            && d.mode2.is_none()
            && d.mode3.is_none()
            && d.additional_modes.is_empty()
            && d.generic_cost_reduction.is_none()
            && d.ward_cost.is_none()
            && !d.spell_cannot_be_countered
            && !d.records_mana_spent
            && d.equipment.is_none()
            && d.starting_loyalty.is_none()
            && d.controlled_creature_grant.is_none()
            && d.game_outcome_protection_v1.is_none()
            && d.controlled_counter_keyword.is_none()
            && d.plot_cost.is_none()
            && d.madness_cost.is_none()
            && d.kicked_target_spec.is_none()
            && d.omen.is_none()
            && d.adventure.is_none()
            && d.bestow.is_none()
            && !d.delve
            && d.attachment.is_none()
            && d.transform_face.is_none()
            && d.saga.is_none()
            && !d.changeling
            && d.animation.is_none()
            && d.damage_replacement_v1.is_none()
    })
}
fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        CARD_DEFS
            .iter()
            .enumerate()
            .any(|(id, _)| owns_definition(id as u16))
    })
}
fn live_source(state: &GameState, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|o| {
        owns_definition(o.card_def)
            && o.zone == Zone::Battlefield
            && o.v4.face_index == 0
            && o.v4.face_down_v1.is_none()
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

/// Called after replacement, before any member of the actual death batch moves.
/// The event index is filled only beside the actual committed graveyard move.
pub(crate) fn capture_death(state: &GameState, event: &ProposedEvent) -> Option<CommittedEvent> {
    let ProposedEvent::ZoneChange(change) = event else {
        return None;
    };
    if change.to_zone != Zone::Graveyard
        || !live_source(state, change.object)
        || !crate::engine::object_has_type(state, change.object, CardType::Creature)
    {
        return None;
    }
    Some(CommittedEvent::FiendishPandaDeathCaptureV1 {
        source: AbilitySourceContractV4::capture(state, change.object),
        departure_history_index: u64::MAX,
        power: crate::engine::effective_power(state, change.object),
    })
}
pub(crate) fn bind_departure(event: &mut CommittedEvent, index: usize) {
    if let CommittedEvent::FiendishPandaDeathCaptureV1 {
        departure_history_index,
        ..
    } = event
    {
        *departure_history_index = index as u64;
    }
}
pub(crate) fn record_gain(state: &mut GameState, index: usize) {
    if !enabled() {
        return;
    }
    let Some(CommittedEvent::LifeGain { player, amount }) = state.engine.event_history.get(index)
    else {
        return;
    };
    let (player, amount) = (*player, *amount);
    if amount <= 0 {
        return;
    }
    let observers = state
        .objects
        .iter()
        .filter_map(|(id, o)| {
            (o.controller == player && live_source(state, id))
                .then(|| AbilitySourceContractV4::capture(state, id))
        })
        .collect::<Vec<_>>();
    for source in observers {
        let capture = CommittedEvent::FiendishPandaLifeGainCaptureV1 {
            source,
            gain_history_index: index as u64,
            amount,
        };
        state.engine.event_history.push(capture.clone());
        state.engine.event_log.push(capture);
    }
}
fn root_at(
    state: &GameState,
    index: usize,
) -> Result<(AbilitySourceContractV4, EffectOp, TargetSpec), String> {
    let event = state
        .engine
        .event_history
        .get(index)
        .ok_or("Panda original occurrence absent")?;
    let (source, original, effect, target_spec) = match event {
        CommittedEvent::FiendishPandaLifeGainCaptureV1 {
            source,
            gain_history_index,
            amount,
        } => {
            let original = usize::try_from(*gain_history_index)
                .map_err(|_| "Panda gain index exceeds usize")?;
            if *amount <= 0
                || !matches!(state.engine.event_history.get(original), Some(CommittedEvent::LifeGain { player, amount: actual }) if *player == source.controller && actual == amount)
            {
                return Err("Panda original positive controller gain changed".into());
            }
            (
                *source,
                original,
                EffectOp::FiendishPandaCounterFromCaptureV1 {
                    capture_history_index: index as u64,
                },
                TargetSpec::None,
            )
        }
        CommittedEvent::FiendishPandaDeathCaptureV1 {
            source,
            departure_history_index,
            power,
        } => {
            let original = usize::try_from(*departure_history_index)
                .map_err(|_| "Panda departure index exceeds usize")?;
            if !matches!(state.engine.event_history.get(original), Some(CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to: Zone::Graveyard, controller_before }) if *object == source.source && *controller_before == source.controller)
            {
                return Err("Panda original battlefield death changed".into());
            }
            (
                *source,
                original,
                EffectOp::FiendishPandaReturnFromCaptureV1 {
                    capture_history_index: index as u64,
                    maximum: *power,
                },
                TargetSpec::AnotherNonBearCreatureCardInOwnGraveyardManaValueAtMost(*power),
            )
        }
        _ => return Err("Panda capture points to a foreign occurrence".into()),
    };
    if original >= index || !owns_definition(source.card_def) || source.zone != Zone::Battlefield {
        return Err("Panda source or occurrence ordering changed".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    if state.engine.event_history[..index]
        .iter()
        .any(|prior| match (prior, event) {
            (
                CommittedEvent::FiendishPandaLifeGainCaptureV1 {
                    source: a,
                    gain_history_index: x,
                    ..
                },
                CommittedEvent::FiendishPandaLifeGainCaptureV1 {
                    source: b,
                    gain_history_index: y,
                    ..
                },
            ) => a == b && x == y,
            (
                CommittedEvent::FiendishPandaDeathCaptureV1 {
                    source: a,
                    departure_history_index: x,
                    ..
                },
                CommittedEvent::FiendishPandaDeathCaptureV1 {
                    source: b,
                    departure_history_index: y,
                    ..
                },
            ) => a == b && x == y,
            _ => false,
        })
    {
        return Err("Panda occurrence captured twice".into());
    }
    Ok((source, effect, target_spec))
}
pub(crate) fn target_spec(id: u16, effect: &EffectOp) -> Option<TargetSpec> {
    if !owns_definition(id) {
        return None;
    }
    match effect {
        EffectOp::FiendishPandaCounterFromCaptureV1 { .. } => Some(TargetSpec::None),
        EffectOp::FiendishPandaReturnFromCaptureV1 { maximum, .. } => {
            Some(TargetSpec::AnotherNonBearCreatureCardInOwnGraveyardManaValueAtMost(*maximum))
        }
        _ => None,
    }
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, ObjectId> {
    if !enabled() {
        return Ok(Vec::new());
    }
    let indices = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            matches!(
                e,
                CommittedEvent::FiendishPandaLifeGainCaptureV1 { .. }
                    | CommittedEvent::FiendishPandaDeathCaptureV1 { .. }
            )
            .then_some(i)
        })
        .collect::<Vec<_>>();
    let mut waiting = Vec::new();
    let mut consumed = Vec::new();
    for index in indices {
        let (source, effect, spec) = root_at(state, index).map_err(|_| ObjectId(0))?;
        let prior = state.engine.event_history.iter().filter(|e| matches!(e, CommittedEvent::FiendishPandaCaptureCollectedV1 { capture_history_index } if *capture_history_index == index as u64)).count();
        let current = events
            .iter()
            .filter(|e| *e == &state.engine.event_history[index])
            .count();
        if prior > 1 || prior == 0 && current != 1 || prior == 1 && current != 0 {
            return Err(source.source);
        }
        if prior == 0 {
            waiting.push(PendingTrigger {
                controller: source.controller,
                source: source.source,
                effect,
                is_madness_offer: false,
                kicked: false,
                target_spec: spec,
                targets: Vec::new(),
                target_contracts: Vec::new(),
                placement_ordered: false,
                source_contract: Some(source),
                granted_by: None,
                optional_additional_cost_paid: None,
                paid_cost_refs: Vec::new(),
            });
            consumed.push(index as u64);
        }
    }
    for capture_history_index in consumed {
        state
            .engine
            .event_history
            .push(CommittedEvent::FiendishPandaCaptureCollectedV1 {
                capture_history_index,
            });
    }
    Ok(waiting)
}
pub(crate) fn execute(
    state: &mut GameState,
    ctx: &ExecCtx,
    effect: &EffectOp,
) -> Result<(), String> {
    let index = match effect {
        EffectOp::FiendishPandaCounterFromCaptureV1 {
            capture_history_index,
        }
        | EffectOp::FiendishPandaReturnFromCaptureV1 {
            capture_history_index,
            ..
        } => usize::try_from(*capture_history_index)
            .map_err(|_| "Panda capture index exceeds usize")?,
        _ => return Err("Panda executor received foreign program".into()),
    };
    let (source, expected, spec) = root_at(state, index)?;
    if expected != *effect || ctx.source != source.source || ctx.controller != source.controller
        || ctx.ability_source_contract != Some(source)
        || state.engine.event_history.iter().filter(|e| matches!(e, CommittedEvent::FiendishPandaCaptureCollectedV1 { capture_history_index } if *capture_history_index == index as u64)).count() != 1
    { return Err("Panda root lost its collected original source".into()); }
    match effect {
        EffectOp::FiendishPandaCounterFromCaptureV1 { .. } => {
            if !ctx.targets.is_empty() {
                return Err("Panda life counter is not targeted".into());
            }
            if state.objects.try_get(source.source).is_some_and(|o| {
                o.zone == Zone::Battlefield
                    && o.owner == source.owner
                    && o.zone_change_count == source.zone_change_count
            }) {
                crate::event::add_plus_one_counters(state, source.source, source.controller, 1)?;
            }
        }
        EffectOp::FiendishPandaReturnFromCaptureV1 { .. } => {
            if ctx.targets.len() != 1 {
                return Err("Panda death root requires exactly one target".into());
            }
            if ctx.target_incarnation_matches(0, state)
                && crate::engine::effect_target_is_legal_from_ability_source(
                    state,
                    source,
                    source.controller,
                    spec,
                    &ctx.targets,
                    0,
                )
            {
                let Target::Object(object) = ctx.targets[0] else {
                    return Err("Panda graveyard target is not an object".into());
                };
                crate::event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(object, Zone::Battlefield),
                );
            }
        }
        _ => unreachable!("authenticated Panda root"),
    }
    Ok(())
}
pub(crate) fn require_legacy_supported(
    state: &GameState,
    actor: Option<PlayerId>,
) -> Result<(), String> {
    let retained = state.engine.event_history.iter().any(|e| {
        matches!(
            e,
            CommittedEvent::FiendishPandaLifeGainCaptureV1 { .. }
                | CommittedEvent::FiendishPandaDeathCaptureV1 { .. }
                | CommittedEvent::FiendishPandaCaptureCollectedV1 { .. }
        )
    });
    if retained {
        return Err("Fiendish Panda historical roots require their consumer successor".into());
    }
    if !enabled() {
        return Ok(());
    }
    let visible = state.objects.iter().any(|(_, o)| {
        (matches!(
            o.zone,
            Zone::Battlefield | Zone::Stack | Zone::Graveyard | Zone::Exile
        ) || o.zone == Zone::Hand && actor == Some(o.owner))
            && o.v4.face_down_v1.is_none()
            && owns_definition(o.card_def)
    });
    if visible {
        return Err(
            "Fiendish Panda requires its distinct captured-power/counter consumer successor".into(),
        );
    }
    Ok(())
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::FiendishPandaLifeGainCaptureV1 { .. }
            | CommittedEvent::FiendishPandaDeathCaptureV1 { .. } => {
                root_at(state, index)?;
            }
            CommittedEvent::FiendishPandaCaptureCollectedV1 {
                capture_history_index,
            } => {
                let original = usize::try_from(*capture_history_index)
                    .map_err(|_| "Panda collected index exceeds usize")?;
                if original >= index {
                    return Err("Panda collection precedes original capture".into());
                }
                root_at(state, original)?;
                if state.engine.event_history[..index].iter().any(|e| matches!(e, CommittedEvent::FiendishPandaCaptureCollectedV1 { capture_history_index: old } if old == capture_history_index)) {
                    return Err("Panda occurrence collected twice".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
