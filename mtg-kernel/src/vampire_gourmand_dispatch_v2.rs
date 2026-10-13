//! Mandatory actual-attack ownership before optional resolution payment.
use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::state::{AbilitySourceContractV4, GameState, StackItem, StackItemKind, Zone};
use crate::trigger::PendingTrigger;

pub(crate) fn owns_definition(id: u16) -> bool {
    crate::card_def::CARD_DEFS
        .get(id as usize)
        .is_some_and(|d| d.is_executable())
        && crate::trigger::triggers_for(id).iter().any(|t| {
            t.condition == crate::trigger::TriggerCondition::Attacks
                && t.home_zone == Zone::Battlefield
                && t.face_index == 0
                && (t.effect)() == crate::vampire_gourmand_v1::program()
        })
}
pub(crate) fn requires_successor(state: &GameState) -> bool {
    state
        .engine
        .event_history
        .iter()
        .any(|e| matches!(e, CommittedEvent::VampireGourmandAttackCaptureV2 { .. }))
        || state.stack.iter().any(selected)
        || state.engine.pending_triggers.iter().any(|p| {
            p.source_contract
                .is_some_and(|s| owns_definition(s.card_def))
        })
}

fn capture_at(state: &GameState, at: u32) -> Result<AbilitySourceContractV4, String> {
    let Some(CommittedEvent::VampireGourmandAttackCaptureV2 {
        declaration_history_index,
        source,
        ..
    }) = state.engine.event_history.get(at as usize)
    else {
        return Err("Gourmand original attack capture absent".into());
    };
    if *declaration_history_index >= at
        || !owns_definition(source.card_def)
        || source.zone != Zone::Battlefield
        || !matches!(state.engine.event_history.get(*declaration_history_index as usize),
            Some(CommittedEvent::DeclaredAttacker { source:id, source_zone_change_count, controller })
                if *id == source.source && *source_zone_change_count == source.zone_change_count && *controller == source.controller)
    {
        return Err("Gourmand capture lost genuine original declaration".into());
    }
    if state.engine.event_history[..at as usize].iter().any(|e| {
        matches!(e,
        CommittedEvent::VampireGourmandAttackCaptureV2 { declaration_history_index:previous, .. }
            if previous == declaration_history_index)
    }) {
        return Err("Gourmand attack repeated its original capture".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, *source)?;
    Ok(*source)
}
pub(crate) fn capture_declaration(state: &mut GameState, declaration: usize) {
    let Some(CommittedEvent::DeclaredAttacker { source, .. }) =
        state.engine.event_history.get(declaration)
    else {
        return;
    };
    let id = *source;
    let object = state.objects.get(id);
    if !owns_definition(object.card_def)
        || object.v4.face_down_v1.is_some()
        || object.v4.face_index != 0
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, id)
    {
        return;
    }
    let event = CommittedEvent::VampireGourmandAttackCaptureV2 {
        declaration_history_index: declaration as u32,
        turn: state.turn,
        active_player: state.active_player,
        source: AbilitySourceContractV4::capture(state, id),
    };
    state.engine.event_history.push(event.clone());
    state.engine.event_log.push(event);
}
fn captures(state: &GameState) -> Vec<u32> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| {
            matches!(e, CommittedEvent::VampireGourmandAttackCaptureV2 { .. }).then_some(at as u32)
        })
        .collect()
}
fn validate_open_turn(state: &GameState, capture: u32) -> Result<(), String> {
    let Some(CommittedEvent::VampireGourmandAttackCaptureV2 {
        turn,
        active_player,
        source,
        ..
    }) = state.engine.event_history.get(capture as usize)
    else {
        return Err("Gourmand original attack turn absent".into());
    };
    if *turn != state.turn
        || *active_player != state.active_player
        || source.controller != *active_player
    {
        return Err("Gourmand open attack crossed its original turn".into());
    }
    Ok(())
}
fn collections(state: &GameState, capture: u32) -> Vec<usize> {
    state.engine.event_history.iter().enumerate().filter_map(|(at,e)| matches!(e,
        CommittedEvent::VampireGourmandCollectedV2 { capture_history_index } if *capture_history_index == capture).then_some(at)).collect()
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, ObjectId> {
    let mut pending = Vec::new();
    let mut used = Vec::new();
    for at in captures(state) {
        let source = capture_at(state, at).map_err(|_| ObjectId(0))?;
        let event = &state.engine.event_history[at as usize];
        let count = events.iter().filter(|e| *e == event).count();
        let prior = collections(state, at);
        if prior.len() > 1 || (prior.is_empty() && count != 1) || (!prior.is_empty() && count != 0)
        {
            return Err(source.source);
        }
        if prior.is_empty() {
            used.push(at);
            pending.push(PendingTrigger {
                controller: source.controller,
                source: source.source,
                source_contract: Some(source),
                granted_by: None,
                effect: crate::vampire_gourmand_v1::program(),
                is_madness_offer: false,
                kicked: false,
                target_spec: crate::card_def::TargetSpec::None,
                targets: vec![],
                target_contracts: vec![],
                placement_ordered: false,
                optional_additional_cost_paid: None,
                paid_cost_refs: vec![],
            });
        }
    }
    for event in events {
        if matches!(event, CommittedEvent::VampireGourmandAttackCaptureV2 { .. })
            && !used
                .iter()
                .any(|at| state.engine.event_history[*at as usize] == *event)
        {
            return Err(ObjectId(0));
        }
    }
    for capture_history_index in used {
        state
            .engine
            .event_history
            .push(CommittedEvent::VampireGourmandCollectedV2 {
                capture_history_index,
            });
    }
    Ok(pending)
}
fn publications(state: &GameState, capture: u32) -> Vec<(usize, &StackItem)> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| {
            if let CommittedEvent::VampireGourmandPublishedV2 {
                capture_history_index,
                item,
            } = e
            {
                (*capture_history_index == capture).then_some((at, item.as_ref()))
            } else {
                None
            }
        })
        .collect()
}
fn selected(item: &StackItem) -> bool {
    item.kind == StackItemKind::TriggeredAbility
        && item.v4.granted_by.is_none()
        && item
            .v4
            .ability_source_contract
            .is_some_and(|s| owns_definition(s.card_def))
}
fn unstamped(item: &StackItem) -> StackItem {
    let mut value = item.clone();
    value.v4.vampire_gourmand_started_v1.0 = None;
    value
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    if !selected(item) {
        return Ok(());
    }
    crate::engine::validated_stack_item_target_spec(item, state)?;
    let source = item.v4.ability_source_contract.unwrap();
    let capture = captures(state)
        .into_iter()
        .rev()
        .find(|at| capture_at(state, *at).ok() == Some(source))
        .ok_or("Gourmand publication lost actual attack capture")?;
    if !publications(state, capture).is_empty() || collections(state, capture).len() != 1 {
        return Err("Gourmand repeated or uncollected publication".into());
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::VampireGourmandPublishedV2 {
            capture_history_index: capture,
            item: Box::new(item.clone()),
        });
    Ok(())
}
pub(crate) fn validate(state: &GameState, removed: Option<&StackItem>) -> Result<(), String> {
    let capture_ids = captures(state);
    for event in &state.engine.event_log {
        if matches!(
            event,
            CommittedEvent::VampireGourmandCollectedV2 { .. }
                | CommittedEvent::VampireGourmandPublishedV2 { .. }
                | CommittedEvent::VampireGourmandCounterRemovedV2 { .. }
                | CommittedEvent::VampireGourmandCounterClosedV2 { .. }
        ) {
            return Err("private Gourmand dispatch entered mutable trigger log".into());
        }
        if matches!(event, CommittedEvent::VampireGourmandAttackCaptureV2 { .. })
            && !capture_ids
                .iter()
                .any(|at| state.engine.event_history[*at as usize] == *event)
        {
            return Err("forged Gourmand attack dispatch log".into());
        }
    }
    let waiting: Vec<_> = state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|p| p.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|s| s.resolution_play.as_ref())
                .flat_map(|p| p.deferred_triggers.iter()),
        )
        .collect();
    for pending in &waiting {
        if pending.granted_by.is_none()
            && pending
                .source_contract
                .is_some_and(|s| owns_definition(s.card_def))
        {
            crate::engine::validate_pending_trigger_identity(state, pending)?;
            if pending.effect != crate::vampire_gourmand_v1::program()
                || !capture_ids
                    .iter()
                    .any(|at| capture_at(state, *at).ok() == pending.source_contract)
            {
                return Err("Gourmand pending owner erased original attack capture".into());
            }
        }
    }
    for capture in capture_ids.iter().copied() {
        let source = capture_at(state, capture)?;
        let collected = collections(state, capture);
        let logged = state
            .engine
            .event_log
            .iter()
            .filter(|e| **e == state.engine.event_history[capture as usize])
            .count();
        let latest_capture = capture_ids
            .iter()
            .rev()
            .find(|at| capture_at(state, **at).ok() == Some(source));
        let held: Vec<_> = waiting
            .iter()
            .filter(|p| {
                latest_capture == Some(&capture)
                    && p.source_contract == Some(source)
                    && p.granted_by.is_none()
                    && p.effect == crate::vampire_gourmand_v1::program()
            })
            .collect();
        for pending in &held {
            crate::engine::validate_pending_trigger_identity(state, pending)?;
        }
        let published = publications(state, capture);
        if collected.is_empty() {
            validate_open_turn(state, capture)?;
            if logged != 1 || !held.is_empty() || !published.is_empty() {
                return Err("Gourmand uncollected attack lost waiting ownership".into());
            }
            continue;
        }
        if collected.len() != 1 || collected[0] <= capture as usize || logged != 0 {
            return Err("Gourmand attack collection repeated or replayed".into());
        }
        match published.as_slice() {
            [] if held.len() == 1 => {
                validate_open_turn(state, capture)?;
            }
            [(published_at, original)] => {
                if *published_at <= collected[0]
                    || !held.is_empty()
                    || !selected(original)
                    || original.v4.ability_source_contract != Some(source)
                    || original.inline_effect.as_ref()
                        != Some(&crate::vampire_gourmand_v1::program())
                    || !original.targets.is_empty()
                    || !original.v4.target_contracts.is_empty()
                    || original.controller != source.controller
                {
                    return Err("Gourmand publication changed actual original trigger".into());
                }
                let live: Vec<_> = state
                    .stack
                    .iter()
                    .filter(|i| i.v4.stack_item_id == original.v4.stack_item_id)
                    .collect();
                let cost_closed: Vec<_> = state
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|e| {
                        if let CommittedEvent::VampireGourmandResolutionBoundaryV1 { receipt } = e {
                            (receipt.stack_item_id == original.v4.stack_item_id
                                && receipt.pending.is_none())
                            .then_some(receipt)
                        } else {
                            None
                        }
                    })
                    .collect();
                let counter_removed: Vec<_> = state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .filter_map(|(at, e)| {
                        if let CommittedEvent::VampireGourmandCounterRemovedV2 {
                            publication_history_index,
                            item,
                        } = e
                        {
                            (*publication_history_index as usize == *published_at)
                                .then_some((at, item))
                        } else {
                            None
                        }
                    })
                    .collect();
                let counter_closed: Vec<_> = state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .filter_map(|(at, e)| {
                        if let CommittedEvent::VampireGourmandCounterClosedV2 {
                            publication_history_index,
                            removal_history_index,
                        } = e
                        {
                            (*publication_history_index as usize == *published_at)
                                .then_some((at, *removal_history_index))
                        } else {
                            None
                        }
                    })
                    .collect();
                let closure_count = cost_closed.len() + counter_closed.len();
                if closure_count == 0 {
                    validate_open_turn(state, capture)?;
                    crate::engine::validated_stack_item_target_spec(original, state)?;
                    if !counter_removed.is_empty()
                        || match (
                            live.as_slice(),
                            removed.filter(|i| i.v4.stack_item_id == original.v4.stack_item_id),
                        ) {
                            ([live], None) => unstamped(live) != **original,
                            ([], Some(owner)) => unstamped(owner) != **original,
                            _ => true,
                        }
                    {
                        return Err(
                            "Gourmand mandatory publication lost or rewrote its live owner".into(),
                        );
                    }
                } else if closure_count == 1 && live.is_empty() {
                    if !cost_closed.is_empty() {
                        if !counter_removed.is_empty() {
                            return Err("Gourmand resolved and countered twice".into());
                        }
                        let begin = state
                            .engine
                            .event_history
                            .iter()
                            .find_map(|e| {
                                if let CommittedEvent::VampireGourmandResolutionBoundaryV1 {
                                    receipt,
                                } = e
                                {
                                    (receipt.stack_item_id == original.v4.stack_item_id
                                        && receipt.ordinal == 0)
                                        .then_some(receipt)
                                } else {
                                    None
                                }
                            })
                            .ok_or("Gourmand closed root lost actual begin")?;
                        if begin.history_index as usize <= *published_at
                            || begin
                                .pending
                                .as_ref()
                                .is_none_or(|p| unstamped(&p.resolving_item) != **original)
                        {
                            return Err("Gourmand closed root rewrote original publication".into());
                        }
                    } else if !matches!((counter_removed.as_slice(),counter_closed.as_slice()),
                        ([(removed_at,item)],[(closed_at,reference)]) if item.as_ref()==*original && removed_at>published_at
                            && *reference as usize==*removed_at && closed_at>removed_at)
                    {
                        return Err("Gourmand counter closure lost actual unique removal".into());
                    }
                } else {
                    return Err("Gourmand completed trigger was duplicated or reintroduced".into());
                }
            }
            _ => return Err("Gourmand attack lost pending owner or repeated publication".into()),
        }
    }
    for event in &state.engine.event_history {
        match event {
            CommittedEvent::VampireGourmandCollectedV2 {
                capture_history_index,
            }
            | CommittedEvent::VampireGourmandPublishedV2 {
                capture_history_index,
                ..
            } if !capture_ids.contains(capture_history_index) => {
                return Err("orphan Gourmand dispatch occurrence".into())
            }
            CommittedEvent::VampireGourmandCounterRemovedV2 {
                publication_history_index,
                ..
            }
            | CommittedEvent::VampireGourmandCounterClosedV2 {
                publication_history_index,
                ..
            } if !matches!(
                state
                    .engine
                    .event_history
                    .get(*publication_history_index as usize),
                Some(CommittedEvent::VampireGourmandPublishedV2 { .. })
            ) =>
            {
                return Err("orphan Gourmand counter occurrence".into())
            }
            _ => {}
        }
    }
    for item in &state.stack {
        if selected(item) && !state.engine.event_history.iter().any(|e| matches!(e,
            CommittedEvent::VampireGourmandPublishedV2 { item:original,.. } if original.v4.stack_item_id == item.v4.stack_item_id)) {
            return Err("Gourmand live root erased original publication".into());
        }
    }
    Ok(())
}
pub(crate) struct CounterOwnerV2 {
    original: StackItem,
    publication: u32,
}
pub(crate) fn acquire_counter(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<CounterOwnerV2>, String> {
    if !selected(item) {
        return Ok(None);
    }
    crate::effect::validate_pending_effect_choice(state)?;
    if !item.v4.vampire_gourmand_started_v1.is_empty() {
        return Err("suspended Gourmand cost cannot be countered as an unstarted row".into());
    }
    let publication=state.engine.event_history.iter().enumerate().find_map(|(at,e)|
        matches!(e,CommittedEvent::VampireGourmandPublishedV2 {item:original,..} if original.as_ref()==item).then_some(at as u32))
        .ok_or("Gourmand counter lost unique actual publication")?;
    Ok(Some(CounterOwnerV2 {
        original: item.clone(),
        publication,
    }))
}
pub(crate) fn counter_removed(
    state: &mut GameState,
    owner: Option<&CounterOwnerV2>,
    item: &StackItem,
) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    if *item != owner.original
        || state
            .stack
            .iter()
            .any(|i| i.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("Gourmand counter did not remove original row".into());
    }
    let removal_history_index = state
        .engine
        .event_history
        .len()
        .try_into()
        .map_err(|_| "Gourmand counter history exceeds u32")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::VampireGourmandCounterRemovedV2 {
            publication_history_index: owner.publication,
            item: Box::new(item.clone()),
        });
    state
        .engine
        .event_history
        .push(CommittedEvent::VampireGourmandCounterClosedV2 {
            publication_history_index: owner.publication,
            removal_history_index,
        });
    Ok(())
}

#[cfg(test)]
mod absence_tests {
    use super::*;
    #[test]
    fn absent_catalog_dispatch_preserves_bytes_and_primary_hash() {
        if crate::card_def::card_id_by_name("Vampire Gourmand").is_some() {
            return;
        }
        let mut state = crate::policy_observation_v6::tests::ready_state();
        let bytes = serde_json::to_vec(&state).unwrap();
        let hash = state.diagnostic_state_hash();
        validate(&state, None).unwrap();
        assert!(collect(&mut state, &[]).unwrap().is_empty());
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        assert_eq!(state.diagnostic_state_hash(), hash);
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod future_counter_primitive {
    use super::*;
    use crate::engine::{self, Action, Decision};
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};
    #[test]
    fn actual_declaration_publication_counter_removal_and_restore_reject_replay() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
            state.step = crate::state::Step::DeclareAttackers;
            // Physical true-definition setup; this primitive does not claim a cast fixture.
            let source = put(&mut state, actor, "Vampire Gourmand", Zone::Battlefield);
            engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            let saved = state
                .stack
                .iter()
                .find(|i| i.source == source)
                .unwrap()
                .clone();
            assert!(saved.v4.vampire_gourmand_started_v1.is_empty());
            engine::counter_stack_item_by_id(&mut state, saved.v4.stack_item_id).unwrap();
            crate::effect::validate_pending_effect_choice(&state).unwrap();
            let bytes = serde_json::to_vec(&state).unwrap();
            for case in 0..3 {
                let mut bad: GameState = serde_json::from_slice(&bytes).unwrap();
                match case {
                    0 => bad.stack.push(saved.clone()),
                    1 => bad.engine.event_history.retain(|e| {
                        !matches!(e, CommittedEvent::VampireGourmandCounterRemovedV2 { .. })
                    }),
                    _ => {
                        let closed = bad
                            .engine
                            .event_history
                            .iter()
                            .find(|e| {
                                matches!(e, CommittedEvent::VampireGourmandCounterClosedV2 { .. })
                            })
                            .unwrap()
                            .clone();
                        bad.engine.event_history.push(closed);
                    }
                }
                let before = serde_json::to_vec(&bad).unwrap();
                assert!(engine::step(&mut bad, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
            }
        }
    }
}
