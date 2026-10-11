//! Definition-owned death observers, frozen before an actual zone-change batch.
//! No card is registered by this module. Legacy pools emit no capture markers.

use crate::card_def::{CardType, CARD_DEFS};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition};

pub(crate) fn enabled() -> bool {
    // Keep the cost of historical pools constant even for large zone batches.
    static HAS_OBSERVERS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *HAS_OBSERVERS.get_or_init(|| {
        CARD_DEFS.iter().enumerate().any(|(id, card)| {
            card.is_executable()
                && crate::trigger::triggers_for(id as u16)
                    .iter()
                    .any(|ability| {
                        ability.condition == TriggerCondition::ThisOrControlledCreatureDies
                    })
        })
    })
}

pub(crate) fn capture(state: &GameState, event: &ProposedEvent) -> Vec<CommittedEvent> {
    if !enabled() {
        return Vec::new();
    }
    let ProposedEvent::ZoneChange(change) = event else {
        return Vec::new();
    };
    let dead = state.objects.get(change.object);
    if dead.zone != Zone::Battlefield
        || change.to_zone != Zone::Graveyard
        || !crate::engine::object_has_type(state, change.object, CardType::Creature)
    {
        return Vec::new();
    }
    state
        .objects
        .iter()
        .filter(|(source, object)| {
            object.zone == Zone::Battlefield
                && object.controller == dead.controller
                && CARD_DEFS[object.card_def as usize].is_executable()
                && crate::continuous_characteristics_v1::printed_abilities_active(state, *source)
        })
        .flat_map(|(source, object)| {
            crate::trigger::triggers_for(object.card_def)
                .iter()
                .enumerate()
                .filter(move |(_, ability)| {
                    ability.condition == TriggerCondition::ThisOrControlledCreatureDies
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
        .collect()
}

/// A capture is authoritative only beside its committed death event. Source
/// departure or control changes afterward do not change the captured controller.
pub(crate) fn pending(
    state: &GameState,
    events: &[CommittedEvent],
    index: usize,
) -> Result<Option<PendingTrigger>, crate::ids::ObjectId> {
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
    let death = events[..index].iter().rev().find(|event| {
        !matches!(
            event,
            CommittedEvent::ControlledCreatureDeathCaptureV1 { .. }
        )
    });
    if !definition.is_executable()
        || ability.condition != TriggerCondition::ThisOrControlledCreatureDies
        || source.zone != Zone::Battlefield
        || source.controller != *controller_before
        || !matches!(death, Some(CommittedEvent::ZoneChange {
            object, from: Zone::Battlefield, to: Zone::Graveyard, controller_before: actual,
        }) if *object == died.object && actual == controller_before)
        || state.objects.try_get(died.object).is_none_or(|object| {
            let later_moves = events[index + 1..].iter().filter(|event| {
                matches!(event, CommittedEvent::ZoneChange { object, .. } if *object == died.object)
            }).count();
            u32::try_from(later_moves)
                .ok()
                .and_then(|moves| died.zone_change_count.checked_add(1)?.checked_add(moves))
                != Some(object.zone_change_count)
        })
        || state.objects.try_get(source.source).is_none_or(|object| {
            object.card_def != source.card_def
                || object.owner != source.owner
                || object.zone_change_count < source.zone_change_count
                || (object.zone_change_count == source.zone_change_count
                    && object.zone != source.zone)
                || object.spell_copy_origin.is_some()
        })
    {
        return Err(invalid());
    }
    let effect = (ability.effect)();
    let target_spec =
        crate::trigger::target_spec_for_trigger(source.card_def, &effect).ok_or_else(invalid)?;
    Ok(Some(PendingTrigger {
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
    }))
}
