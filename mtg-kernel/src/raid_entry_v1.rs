//! Original-controller authority for the two definition-owned Raid entry roots.
use crate::card_def::CARD_DEFS;
use crate::effect::EffectOp;
use crate::event::CommittedEvent;
use crate::state::{AbilitySourceContractV4, GameState, Zone};

pub(crate) fn owns_definition(card: u16) -> bool {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return false;
    }
    CARD_DEFS.get(usize::from(card)).is_some_and(|def| {
        let abilities = crate::trigger::triggers_for(card);
        let expected = match def.name {
            "Gorehorn Raider" => crate::trigger::gorehorn_raider_effect(),
            "Gutless Plunderer" => crate::trigger::gutless_plunderer_effect(),
            _ => return false,
        };
        def.is_executable()
            && abilities.len() == 1
            && abilities[0].condition
                == crate::trigger::TriggerCondition::EtbAfterControllerAttackedThisTurn
            && abilities[0].home_zone == Zone::Battlefield
            && abilities[0].face_index == 0
            && !abilities[0].intervening_if_kicked
            && !abilities[0].intervening_if_controls_another_source_card
            && (abilities[0].effect)() == expected
    })
}

pub(crate) fn record_entry(state: &mut GameState, entry_history_index: usize) {
    let id = match state.engine.event_history.get(entry_history_index) {
        Some(CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            ..
        })
        | Some(CommittedEvent::CreateToken { object, .. }) => *object,
        _ => return,
    };
    let object = state.objects.get(id);
    if !owns_definition(object.card_def) {
        return;
    }
    let source = AbilitySourceContractV4::capture(state, id);
    state
        .engine
        .event_history
        .push(CommittedEvent::RaidEntrySourceV1 {
            source,
            entry_history_index,
        });
}

pub(crate) fn validate_source_for_program(
    state: &GameState,
    program: &EffectOp,
    source: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    let intrinsic = *program == crate::trigger::gorehorn_raider_effect()
        || *program == crate::trigger::gutless_plunderer_effect();
    let Some(source) = source.filter(|source| owns_definition(source.card_def)) else {
        return if intrinsic {
            Err("printed Raid root lost its independent source definition".into())
        } else {
            Ok(())
        };
    };
    if program != &(crate::trigger::triggers_for(source.card_def)[0].effect)() {
        return Err("Raid entry root differs from its original printed definition".into());
    }
    if source.zone != Zone::Battlefield
        || state
            .engine
            .event_log
            .iter()
            .any(|event| matches!(event, CommittedEvent::RaidEntrySourceV1 { .. }))
    {
        return Err("Raid entry lost its private battlefield capture".into());
    }
    let entries = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            CommittedEvent::RaidEntrySourceV1 {
                source: captured,
                entry_history_index,
            } if captured.source == source.source
                && captured.zone_change_count == source.zone_change_count =>
            {
                Some((index, captured, *entry_history_index))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(receipt_index, original, entry_index)] = entries.as_slice() else {
        return Err("Raid entry lost its unique original controller capture".into());
    };
    let entry = match state.engine.event_history.get(*entry_index) {
        Some(CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            ..
        }) => *object == source.source,
        Some(CommittedEvent::CreateToken {
            object,
            token_def,
            controller,
        }) => {
            *object == source.source
                && *token_def == source.card_def
                && *controller == source.controller
                && *controller == source.owner
        }
        _ => false,
    };
    if **original != source || *entry_index >= *receipt_index || !entry {
        return Err("Raid entry source/controller changed from its genuine entry".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, source)
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::RaidEntrySourceV1 { .. }
                | CommittedEvent::PrivateKeepOneLibraryCapturedV1 { .. }
                | CommittedEvent::PrivateKeepOneLibraryConsumedV1 { .. }
                | CommittedEvent::PrivateKeepOneRaidSkippedV1 { .. }
        )
    }) {
        return Err("private Raid entry authority cannot enter the public event log".into());
    }
    for pending in state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|pending| pending.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|standard| standard.resolution_play.as_ref())
                .flat_map(|resolution| resolution.deferred_triggers.iter()),
        )
    {
        if pending.granted_by.is_none()
            && (pending.effect == crate::trigger::gorehorn_raider_effect()
                || pending.effect == crate::trigger::gutless_plunderer_effect()
                || pending
                    .source_contract
                    .is_some_and(|source| owns_definition(source.card_def))
                || state
                    .objects
                    .try_get(pending.source)
                    .is_some_and(|source| owns_definition(source.card_def)))
        {
            crate::engine::validate_pending_trigger_identity(state, pending)?;
            validate_source_for_program(state, &pending.effect, pending.source_contract)?;
        }
    }
    for item in &state.stack {
        let printed = item.v4.granted_by.is_none()
            && (item.inline_effect.as_ref().is_some_and(|op| {
                *op == crate::trigger::gorehorn_raider_effect()
                    || *op == crate::trigger::gutless_plunderer_effect()
            }) || item
                .v4
                .ability_source_contract
                .is_some_and(|source| owns_definition(source.card_def))
                || (item.kind == crate::state::StackItemKind::TriggeredAbility
                    && state
                        .objects
                        .try_get(item.source)
                        .is_some_and(|source| owns_definition(source.card_def))));
        if printed {
            crate::engine::validated_stack_item_target_spec(item, state)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_absence_path_does_not_record_or_modify_any_state() {
        for actor in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let before = serde_json::to_vec(&state).unwrap();
            let hash = state.state_hash();
            record_entry(&mut state, usize::MAX);
            validate_state(&state).unwrap();
            assert_eq!(before, serde_json::to_vec(&state).unwrap());
            assert_eq!(hash, state.state_hash());
        }
    }
}
