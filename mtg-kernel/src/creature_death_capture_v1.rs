//! Definition-owned death observers, frozen before an actual zone-change batch.
//! Legacy pools without selected death abilities emit no capture markers.

use crate::card_def::{CardType, CARD_DEFS};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::event_history_v1::EventKindV1;
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition};

pub(crate) fn enabled() -> bool {
    // Keep the cost of historical pools constant even for large zone batches.
    static HAS_OBSERVERS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *HAS_OBSERVERS.get_or_init(|| {
        CARD_DEFS.iter().enumerate().any(|(id, card)| {
            crate::fiendish_panda_v1::owns_definition(id as u16)
                || crate::fake_your_own_death_v50::spell_custody::selected_definition(id as u16)
                || card.is_executable()
                    && crate::trigger::triggers_for(id as u16)
                        .iter()
                        .any(|ability| {
                            matches!(
                                ability.condition,
                                TriggerCondition::ThisOrControlledCreatureDies
                                    | TriggerCondition::DiesWithRevivalCounterV1
                                    | TriggerCondition::DiesUnlessSubtype(_)
                                    | TriggerCondition::OtherNontokenCreatureDies
                                    | TriggerCondition::OtherControlledNontokenCreatureDiesV1
                                    | TriggerCondition::ControlledNontokenCreatureDiesWithoutSubtypeV1(_)
                            )
                        })
        })
    })
}

pub(crate) fn capture(
    state: &GameState,
    event: &ProposedEvent,
) -> Result<Vec<CommittedEvent>, String> {
    if !enabled() {
        return Ok(Vec::new());
    }
    let ProposedEvent::ZoneChange(change) = event else {
        return Ok(Vec::new());
    };
    let dead = state.objects.get(change.object);
    if dead.zone != Zone::Battlefield
        || change.to_zone != Zone::Graveyard
        || !crate::engine::object_has_type(state, change.object, CardType::Creature)
    {
        return Ok(Vec::new());
    }
    let mut captures = state
        .objects
        .iter()
        .filter(|(source, object)| {
            object.zone == Zone::Battlefield
                && CARD_DEFS[object.card_def as usize].is_executable()
                && crate::continuous_characteristics_v1::printed_abilities_active(state, *source)
        })
        .flat_map(|(source, object)| {
            crate::trigger::triggers_for(object.card_def)
                .iter()
                .enumerate()
                .filter(move |(_, ability)| {
                    ((matches!(ability.condition, TriggerCondition::DiesUnlessSubtype(subtype)
                        if source == change.object && !crate::engine::has_effective_subtype(state, source, subtype)))
                        || (ability.condition == TriggerCondition::ThisOrControlledCreatureDies
                        && object.controller == dead.controller)
                        || (ability.condition == TriggerCondition::DiesWithRevivalCounterV1
                            && source == change.object
                            && crate::card_def::CARD_DEFS[usize::from(object.card_def)].name
                                == "Nine-Lives Familiar"
                            && object.counters.revival > 0
                            && object.controller == dead.controller)
                        || (ability.condition == TriggerCondition::OtherNontokenCreatureDies
                            && crate::high_society_hunter_v1::owns_definition(object.card_def)
                            && !dead.v4.is_token
                            && source != change.object)
                        || (ability.condition
                            == TriggerCondition::OtherControlledNontokenCreatureDiesV1
                            && object.controller == dead.controller
                            && !dead.v4.is_token
                            && !CARD_DEFS[usize::from(dead.card_def)].is_token
                            && source != change.object)
                        || (matches!(ability.condition, TriggerCondition::ControlledNontokenCreatureDiesWithoutSubtypeV1(subtype)
                            if !crate::engine::has_effective_subtype(state, change.object, subtype))
                            && object.controller == dead.controller
                            && !dead.v4.is_token
                            && !CARD_DEFS[usize::from(dead.card_def)].is_token))
                        && ability.home_zone == Zone::Battlefield
                        && ability.face_index == object.v4.face_index
                })
                .map(
                    move |(ability_index, _)| CommittedEvent::ControlledCreatureDeathCaptureV1 {
                        source: AbilitySourceContractV4::capture(state, source),
                        ability_index: u16::try_from(ability_index)
                            .expect("bounded printed abilities"),
                        died: ObjectLinkV4 {
                            object: change.object,
                            zone_change_count: dead.zone_change_count,
                        },
                        controller_before: dead.controller,
                    },
                )
        })
        .collect::<Vec<_>>();
    // Legacy death markers must remain adjacent to the death in their
    // established order. Append Panda's indexed authority after them, and
    // retain the predecessor's single allocation when Panda is absent.
    if let Some(capture) = crate::fiendish_panda_v1::capture_death(state, event) {
        captures.push(capture);
    }
    captures.extend(crate::fake_your_own_death_v50::spell_custody::capture_death(state, event)?);
    Ok(captures)
}

/// A capture is authoritative only beside its committed death in both the
/// mutable collection log and independent committed history. One history index
/// is consumed once. Later source changes do not change the captured controller.
pub(crate) fn pending(
    state: &GameState,
    events: &[CommittedEvent],
    index: usize,
) -> Result<Option<(PendingTrigger, usize)>, crate::ids::ObjectId> {
    let CommittedEvent::ControlledCreatureDeathCaptureV1 {
        source,
        ability_index,
        died,
        controller_before,
    } = &events[index]
    else {
        return Ok(None);
    };
    let invalid = || source.source;
    let definition = CARD_DEFS
        .get(source.card_def as usize)
        .ok_or_else(invalid)?;
    let ability = crate::trigger::triggers_for(source.card_def)
        .get(usize::from(*ability_index))
        .ok_or_else(invalid)?;
    // A coherent mutable-log forgery cannot invent a precommit observer.
    // Match the full frozen source/controller/deceased/ability tuple against
    // committed authority and its exact death occurrence before using it.
    let mut authorities = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter(|(_, event)| *event == &events[index]);
    let (history_index, _) = authorities.next().ok_or_else(invalid)?;
    if authorities.next().is_some()
        || state.engine.event_history.scan_for_v1(&[EventKindV1::ControlledCreatureDeathCaptureCollectedV1]).iter().any(|event| {
            matches!(event, CommittedEvent::ControlledCreatureDeathCaptureCollectedV1 { capture_history_index }
                if u64::try_from(history_index).ok() == Some(*capture_history_index))
        })
    {
        return Err(invalid());
    }
    let death = preceding_death(
        state,
        events,
        index,
        history_index,
        *died,
        *controller_before,
    )
    .ok_or_else(invalid)?;
    if !definition.is_executable()
        || !matches!(
            ability.condition,
            TriggerCondition::ThisOrControlledCreatureDies
                | TriggerCondition::DiesWithRevivalCounterV1
                | TriggerCondition::DiesUnlessSubtype(_)
                | TriggerCondition::OtherNontokenCreatureDies
                | TriggerCondition::OtherControlledNontokenCreatureDiesV1
                | TriggerCondition::ControlledNontokenCreatureDiesWithoutSubtypeV1(_)
        )
        || (matches!(ability.condition, TriggerCondition::DiesUnlessSubtype(_))
            && (source.source != died.object || source.controller != *controller_before))
        || source.zone != Zone::Battlefield
        || (ability.condition == TriggerCondition::ThisOrControlledCreatureDies
            && source.controller != *controller_before)
        || (ability.condition == TriggerCondition::DiesWithRevivalCounterV1
            && (source.source != died.object
                || source.controller != *controller_before
                || crate::card_def::CARD_DEFS[usize::from(source.card_def)].name
                    != "Nine-Lives Familiar"
                || state
                    .counter_lki_for(source.source, source.zone_change_count)
                    .is_none_or(|counters| counters.revival <= 0)))
        || (ability.condition == TriggerCondition::OtherNontokenCreatureDies
            && (!crate::high_society_hunter_v1::owns_definition(source.card_def)
                || source.source == died.object
                || state
                    .objects
                    .try_get(died.object)
                    .is_none_or(|object| object.v4.is_token)))
        || (ability.condition == TriggerCondition::OtherControlledNontokenCreatureDiesV1
            && (definition.other_controlled_nontoken_death_v1.is_none()
                || source.controller != *controller_before
                || source.source == died.object
                || state.objects.try_get(died.object).is_none_or(|object| {
                    object.v4.is_token || CARD_DEFS[usize::from(object.card_def)].is_token
                })))
        || (matches!(
            ability.condition,
            TriggerCondition::ControlledNontokenCreatureDiesWithoutSubtypeV1(_)
        ) && (definition.name != "Valkyrie's Call"
            || source.controller != *controller_before
            || state.objects.try_get(died.object).is_none_or(|object| {
                object.v4.is_token || CARD_DEFS[usize::from(object.card_def)].is_token
            })))
        || !matches!(death, CommittedEvent::ZoneChange {
            object, from: Zone::Battlefield, to: Zone::Graveyard, controller_before: actual,
        } if *object == died.object && actual == controller_before)
        || state.objects.try_get(died.object).is_none_or(|object| {
            let later_moves = events[index + 1..].iter().filter(|event| {
                matches!(event, CommittedEvent::ZoneChange { object, .. } if *object == died.object)
            }).count();
            u32::try_from(later_moves)
                .ok()
                .and_then(|moves| died.zone_change_count.checked_add(1)?.checked_add(moves))
                != Some(object.zone_change_count)
        })
        || (matches!(
            ability.condition,
            TriggerCondition::OtherNontokenCreatureDies
                | TriggerCondition::OtherControlledNontokenCreatureDiesV1
                | TriggerCondition::ControlledNontokenCreatureDiesWithoutSubtypeV1(_)
                | TriggerCondition::DiesUnlessSubtype(_)
                | TriggerCondition::DiesWithRevivalCounterV1
        ) && crate::engine::validate_historical_ability_source_contract(state, *source)
            .is_err())
        || (ability.condition == TriggerCondition::ThisOrControlledCreatureDies
            && state.objects.try_get(source.source).is_none_or(|object| {
                object.card_def != source.card_def
                    || object.owner != source.owner
                    || object.zone_change_count < source.zone_change_count
                    || (object.zone_change_count == source.zone_change_count
                        && object.zone != source.zone)
                    || object.spell_copy_origin.is_some()
            }))
    {
        return Err(invalid());
    }
    let effect = if matches!(
        ability.condition,
        TriggerCondition::ControlledNontokenCreatureDiesWithoutSubtypeV1(_)
    ) {
        let object = state.objects.try_get(died.object).ok_or_else(invalid)?;
        crate::effect::EffectOp::ReturnValkyriesCallCreatureV1 {
            source: *source,
            capture_history_index: u64::try_from(history_index).map_err(|_| invalid())?,
            graveyard: crate::state::ObjectLinkV4 {
                object: died.object,
                zone_change_count: died.zone_change_count.checked_add(1).ok_or_else(invalid)?,
            },
            owner: object.owner,
        }
    } else if ability.condition == TriggerCondition::OtherNontokenCreatureDies {
        crate::high_society_hunter_custody_v45::bound(history_index as u64, false)
    } else if ability.condition == TriggerCondition::DiesWithRevivalCounterV1 {
        let counters = state
            .counter_lki_for(source.source, source.zone_change_count)
            .ok_or_else(invalid)?;
        let revival_counters = counters.revival.checked_sub(1).ok_or_else(invalid)?;
        let graveyard_generation = died.zone_change_count.checked_add(1).ok_or_else(invalid)?;
        crate::effect::EffectOp::ScheduleNineLivesFamiliarReturnV1 {
            graveyard: crate::state::ObjectLinkV4 {
                object: died.object,
                zone_change_count: graveyard_generation,
            },
            revival_counters,
        }
    } else {
        (ability.effect)()
    };
    let target_spec =
        crate::trigger::target_spec_for_trigger(source.card_def, &effect).ok_or_else(invalid)?;
    Ok(Some((
        PendingTrigger {
            controller: source.controller,
            source: source.source,
            effect,
            is_madness_offer: false,
            kicked: false,
            target_spec,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: Some(*source),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        },
        history_index,
    )))
}

/// A native departure may append its exact LKI receipts before death observers.
/// Niv's receipt is history-only; Alesha's is in both streams. Reconstruct only
/// this closed segment, retaining the original death and complete capture order.
/// Unrelated events, missing receipts, and altered receipt indices are refused.
fn preceding_death<'a>(
    state: &'a GameState,
    events: &[CommittedEvent],
    index: usize,
    history_index: usize,
    died: ObjectLinkV4,
    controller: crate::ids::PlayerId,
) -> Option<&'a CommittedEvent> {
    let (departure, niv) = departure_segment(state, history_index, died, controller)?;
    let history = &state.engine.event_history;
    // Match the exact native collection-log suffix against committed history.
    // Only the authenticated Niv receipt above is absent from that public log.
    let expected = history.get(departure..=history_index)?;
    let log_len = expected.len().checked_sub(usize::from(niv.is_some()))?;
    let actual = events.get(index.checked_add(1)?.checked_sub(log_len)?..=index)?;
    if !actual.iter().eq(expected
        .iter()
        .enumerate()
        .filter(|(offset, _)| Some(departure + offset) != niv)
        .map(|(_, event)| event))
    {
        return None;
    }
    history.get(departure)
}

/// Shared native/public history guard. This validates only the exact departure
/// segment, without collecting a capture or changing any consumer admission.
pub(crate) fn validated_capture_death(
    state: &GameState,
    history_index: usize,
) -> Result<&CommittedEvent, String> {
    let Some(CommittedEvent::ControlledCreatureDeathCaptureV1 {
        died,
        controller_before,
        ..
    }) = state.engine.event_history.get(history_index)
    else {
        return Err("controlled death capture is absent".into());
    };
    let (departure, _) = departure_segment(state, history_index, *died, *controller_before)
        .ok_or("controlled death lost its exact native departure/receipt segment")?;
    Ok(&state.engine.event_history[departure])
}

fn departure_segment(
    state: &GameState,
    history_index: usize,
    died: ObjectLinkV4,
    controller: crate::ids::PlayerId,
) -> Option<(usize, Option<usize>)> {
    let history = &state.engine.event_history;
    let mut cursor = history_index;
    while cursor > 0
        && matches!(
            history.get(cursor - 1),
            Some(CommittedEvent::ControlledCreatureDeathCaptureV1 { .. })
        )
    {
        cursor -= 1;
    }
    let alesha = match history.get(cursor.checked_sub(1)?)? {
        CommittedEvent::AleshaPowerLkiV1 { .. } => {
            cursor -= 1;
            Some(cursor)
        }
        _ => None,
    };
    let niv = match history.get(cursor.checked_sub(1)?)? {
        CommittedEvent::NivMizzetVisionaryDamageSourceDepartureV1 { .. } => {
            cursor -= 1;
            Some(cursor)
        }
        _ => None,
    };
    let departure = cursor.checked_sub(1)?;
    let death = history.get(departure)?;
    if !matches!(death, CommittedEvent::ZoneChange {
        object, from: Zone::Battlefield, to: Zone::Graveyard, controller_before,
    } if *object == died.object && *controller_before == controller)
        || niv.is_some() != crate::event::niv_mizzet_catalog_enabled_v1()
    {
        return None;
    }
    let departed_source = if let Some(at) = niv {
        let CommittedEvent::NivMizzetVisionaryDamageSourceDepartureV1 {
            source,
            departure_history_index,
        } = history.get(at)?
        else {
            return None;
        };
        let current = state.objects.try_get(source.source)?;
        if at != departure.checked_add(1)?
            || usize::try_from(*departure_history_index).ok()? != departure
            || source.source != died.object
            || source.zone != Zone::Battlefield
            || source.zone_change_count != died.zone_change_count
            || source.controller != controller
            || source.owner != current.owner
            || current.zone_change_count <= source.zone_change_count
            || !crate::engine::source_card_def_matches(state, *source, current.card_def)
        {
            return None;
        }
        // This is the deceased object, not an ability-source capability. A
        // permanent spell copy can genuinely die as a token under Bloodwitch;
        // its spell-copy origin does not invalidate the indexed departure.
        Some(*source)
    } else {
        None
    };
    if let Some(source) = departed_source {
        let is_alesha =
            CARD_DEFS.get(usize::from(source.card_def))?.name == "Alesha, Who Laughs at Fate";
        if alesha.is_some() != is_alesha {
            return None;
        }
    }
    if let Some(at) = alesha {
        let CommittedEvent::AleshaPowerLkiV1 {
            source,
            card_def,
            zone_change_count,
            departure_history_index,
            ..
        } = history.get(at)?
        else {
            return None;
        };
        if at != departure.checked_add(1 + usize::from(niv.is_some()))?
            || usize::try_from(*departure_history_index).ok()? != departure
            || *source != died.object
            || *zone_change_count != died.zone_change_count
            || CARD_DEFS.get(usize::from(*card_def))?.name != "Alesha, Who Laughs at Fate"
            || departed_source.is_some_and(|departed| departed.card_def != *card_def)
        {
            return None;
        }
    }
    Some((departure, niv))
}

/// A definition-owned new death program, not a predecessor or an arbitrary
/// historical source contract. Used only to retain its physical source during
/// hidden-state sampling after the event marker has been collected.
pub(crate) fn owns_program(
    source: &AbilitySourceContractV4,
    effect: &crate::effect::EffectOp,
) -> bool {
    source.zone == Zone::Battlefield
        && CARD_DEFS
            .get(source.card_def as usize)
            .is_some_and(|card| card.is_executable())
        && crate::trigger::triggers_for(source.card_def)
            .iter()
            .any(|ability| {
                matches!(
                    ability.condition,
                    TriggerCondition::ThisOrControlledCreatureDies
                        | TriggerCondition::DiesWithRevivalCounterV1
                        | TriggerCondition::DiesUnlessSubtype(_)
                        | TriggerCondition::OtherControlledNontokenCreatureDiesV1
                        | TriggerCondition::ControlledNontokenCreatureDiesWithoutSubtypeV1(_)
                ) && ability.home_zone == Zone::Battlefield
                    && if matches!(
                        ability.condition,
                        TriggerCondition::ControlledNontokenCreatureDiesWithoutSubtypeV1(_)
                    ) {
                        crate::trigger::source_bound_trigger_program_matches(
                            &(ability.effect)(),
                            effect,
                        )
                    } else {
                        (ability.effect)() == *effect
                    }
            })
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
