//! Mandatory creature-sacrifice versus additional-mana cast choice.
//! No optional payment or alternative printed-cost semantics are introduced.
use super::*;
use serde::{Deserialize, Serialize};

pub(super) const SACRIFICE: &[CostComponent] = &[CostComponent::SacrificeControlled {
    count: 1,
    filter: PermanentFilter::Creature,
}];
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    pub pay_mana: Option<bool>,
    pub creature: Option<EffectObjectBinding>,
}
pub(super) fn mana_branch(def: &card_def::CardDef) -> Option<&'static [CostComponent]> {
    let [CostComponent::SacrificeCreatureOrPayMana(branch)] = def.additional_cost? else {
        return None;
    };
    let [CostComponent::Mana(cost)] = *branch else {
        return None;
    };
    (cost.x_count == 0).then_some(*branch)
}
pub(super) fn seed(def: &card_def::CardDef) -> Option<ChoiceV1> {
    mana_branch(def).map(|_| ChoiceV1 {
        pay_mana: None,
        creature: None,
    })
}
pub(super) fn validate_pending(state: &GameState, pending: &PendingCast) -> Result<(), String> {
    let source = state
        .objects
        .try_get(pending.spell)
        .ok_or("mandatory cost source absent")?;
    let def = card_def::CARD_DEFS
        .get(usize::from(source.card_def))
        .ok_or("mandatory cost definition absent")?;
    match (mana_branch(def), pending.mandatory_additional_cost) {
        (None, None) => return Ok(()),
        (Some(_), Some(choice)) => {
            if choice.pay_mana.is_none()
                && (choice.creature.is_some()
                    || !pending.sacrifice_chosen.is_empty()
                    || pending.additional_cost_discarded.is_some())
            {
                return Err("mandatory additional cost advanced without branch choice".into());
            }
            if choice.pay_mana == Some(true)
                && (choice.creature.is_some() || !pending.sacrifice_chosen.is_empty())
            {
                return Err("additional mana branch carries a sacrifice".into());
            }
            if let Some(binding) = choice.creature {
                let object = state
                    .objects
                    .try_get(binding.object)
                    .ok_or("mandatory sacrifice absent")?;
                if choice.pay_mana != Some(false)
                    || pending.sacrifice_chosen != [binding.object]
                    || binding.expected_zone != Zone::Battlefield
                    || object.zone != binding.expected_zone
                    || object.zone_change_count != binding.expected_zone_change_count
                    || object.controller != pending.controller
                    || !object_has_type(state, binding.object, CardType::Creature)
                {
                    return Err("mandatory sacrifice incarnation or control changed".into());
                }
            } else if !pending.sacrifice_chosen.is_empty() {
                return Err("mandatory sacrifice binding erased".into());
            }
            if choice
                .pay_mana
                .is_some_and(|branch| !branches(state, pending).contains(&branch))
            {
                return Err(
                    "selected mandatory additional-cost branch is no longer payable".into(),
                );
            }
            if choice.pay_mana.is_some() {
                let spec = selected_spell_target_spec(
                    def,
                    pending.mode_chosen.ok_or("mandatory cost before mode")?,
                    pending.kicked == Some(true),
                )
                .ok_or("mandatory cost spell form absent")?;
                if pending.cast_mode.is_none() || !pending_cast_targeting_is_complete(pending, spec)
                {
                    return Err("mandatory cost before targeting".into());
                }
            }
        }
        _ => return Err("mandatory additional cost stage erased or forged".into()),
    }
    Ok(())
}
pub(super) fn branches(state: &GameState, pending: &PendingCast) -> Vec<bool> {
    let def = &card_def::CARD_DEFS[usize::from(state.objects.get(pending.spell).card_def)];
    [false, true]
        .into_iter()
        .filter(|branch| {
            super::spell_costs_v1::selected_spell_quote_branch_v1(
                def,
                pending.spell,
                pending.origin_zone,
                finalized_cast_method(pending, pending.source_contract.cast_method, def),
                pending.kicked == Some(true),
                pending.mode_chosen.unwrap_or(0),
                pending.x_value.unwrap_or(0),
                &pending.targets_chosen,
                &pending.sacrifice_chosen,
                pending.controller,
                state,
                &[],
                Some(*branch),
            )
            .is_some()
        })
        .collect()
}
/// Private consumer authority: validate the original pending cast and derive the
/// payable ordered branches from the same combined payment planner as the engine.
pub(crate) fn consumer_branches(
    state: &GameState,
    pending: &PendingCast,
) -> Result<Vec<bool>, String> {
    super::validate_pending_cast(state, pending)?;
    Ok(branches(state, pending))
}
pub(super) fn answer(
    state: &mut GameState,
    pending: &PendingCast,
    index: u16,
) -> Result<(), String> {
    let choices = branches(state, pending);
    if choices.len() != 2 {
        return Err("mandatory cost branch choice is not currently offered".into());
    }
    let branch = *choices
        .get(usize::from(index))
        .ok_or("mandatory cost branch out of range")?;
    state
        .engine
        .pending_cast
        .as_mut()
        .ok_or("mandatory cast absent")?
        .mandatory_additional_cost
        .as_mut()
        .ok_or("mandatory stage absent")?
        .pay_mana = Some(branch);
    Ok(())
}
pub(super) fn bind_creature(
    state: &mut GameState,
    pending: &PendingCast,
    object: ObjectId,
) -> Result<(), String> {
    let Some(choice) = pending.mandatory_additional_cost else {
        return Ok(());
    };
    if choice.pay_mana != Some(false) || choice.creature.is_some() {
        return Err("mandatory sacrifice at wrong stage".into());
    }
    let live = state
        .objects
        .try_get(object)
        .ok_or("mandatory sacrifice missing")?;
    if live.zone != Zone::Battlefield
        || live.controller != pending.controller
        || !object_has_type(state, object, CardType::Creature)
    {
        return Err("mandatory sacrifice not controlled creature".into());
    }
    let binding = EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    };
    state
        .engine
        .pending_cast
        .as_mut()
        .ok_or("mandatory cast missing")?
        .mandatory_additional_cost
        .as_mut()
        .ok_or("mandatory stage missing")?
        .creature = Some(binding);
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidV1 {
    pub input_index: usize,
    pub stack_item_id: StackItemId,
    pub pay_mana: bool,
    pub original_creature: Option<EffectObjectBinding>,
    pub paid_creature: Option<PaidCostRefV4>,
}
pub(super) fn validate_paid_creature(
    state: &GameState,
    pending: &PendingCast,
) -> Result<(), String> {
    let Some(choice) = pending.mandatory_additional_cost else {
        return Ok(());
    };
    if choice.pay_mana == Some(true) && choice.creature.is_none() {
        return Ok(());
    }
    if choice.pay_mana != Some(false) {
        return Err("mandatory cost not selected before payment".into());
    }
    let original = choice
        .creature
        .ok_or("mandatory sacrifice missing before payment")?;
    let object = state
        .objects
        .try_get(original.object)
        .ok_or("paid sacrifice object absent")?;
    if object.zone == Zone::Battlefield
        && object.zone_change_count == original.expected_zone_change_count
    {
        return Err("mandatory sacrifice did not actually leave battlefield".into());
    }
    if !state.engine.event_history.iter().any(|event| matches!(event, CommittedEvent::ZoneChange { object: id, from: Zone::Battlefield, controller_before, .. } if *id == original.object && *controller_before == pending.controller)) {
        return Err("mandatory sacrifice lost actual departure".into());
    }
    Ok(())
}
pub(super) fn paid_choice(
    input: Option<(usize, StackItemId)>,
    pending: &PendingCast,
    references: &[PaidCostRefV4],
) -> Result<Option<PaidV1>, String> {
    let Some(choice) = pending.mandatory_additional_cost else {
        return Ok(None);
    };
    let pay_mana = choice
        .pay_mana
        .ok_or("mandatory cost choice missing at finalize")?;
    let paid_creature = if let Some(original) = choice.creature {
        Some(
            *references
                .iter()
                .find(|reference| reference.object == original.object)
                .ok_or("mandatory sacrifice paid provenance missing")?,
        )
    } else {
        None
    };
    let (input_index, stack_item_id) = input.ok_or("mandatory payment input missing")?;
    Ok(Some(PaidV1 {
        input_index,
        stack_item_id,
        pay_mana,
        original_creature: choice.creature,
        paid_creature,
    }))
}
pub(super) fn record_paid(state: &mut GameState, source: StackSourceContractV4) {
    if let Some(choice) = source
        .finalized_cast_binding
        .and_then(|binding| binding.mandatory_additional_cost)
    {
        state
            .engine
            .event_history
            .push(CommittedEvent::MandatoryCastPaymentV1 {
                source,
                choice,
                item: Box::new(
                    state
                        .stack
                        .iter()
                        .find(|item| item.v4.source_contract == Some(source))
                        .expect("validated finalized mandatory cast retains its actual row")
                        .clone(),
                ),
            });
    }
}
pub(super) fn validate_finalized(
    state: &GameState,
    item: &StackItem,
    def: &card_def::CardDef,
    pending: bool,
) -> Result<(), String> {
    let choice = item
        .v4
        .source_contract
        .and_then(|source| source.finalized_cast_binding)
        .and_then(|binding| binding.mandatory_additional_cost);
    if mana_branch(def).is_none() {
        return if choice.is_none() {
            Ok(())
        } else {
            Err("foreign mandatory payment provenance".into())
        };
    }
    if pending {
        return if choice.is_none() {
            Ok(())
        } else {
            Err("unpaid placeholder carries mandatory payment".into())
        };
    }
    let choice = choice.ok_or("finalized mandatory payment erased")?;
    let source = item
        .v4
        .source_contract
        .ok_or("mandatory paid source absent")?;
    let Some(CommittedEvent::MandatoryCastPaymentInputV1 {
        pending: original,
        item: original_item,
    }) = state.engine.event_history.get(choice.input_index)
    else {
        return Err("mandatory original payment input erased".into());
    };
    let expected_method =
        finalized_cast_method(original, original.source_contract.cast_method, def);
    let mut expected_origin = original.source_contract.spell_cast_origin;
    if let Some(origin) = expected_origin.as_mut() {
        origin.finalized_method = Some(expected_method);
    }
    if source.cast_method != expected_method
        || source.zone != original.source_contract.zone
        || source.spell_copy_origin != original.source_contract.spell_copy_origin
        || source.spell_cast_origin != expected_origin
    {
        return Err("mandatory finalized source differs from original cast route".into());
    }
    if original_item.v4.stack_item_id != choice.stack_item_id
        || item.v4.stack_item_id != choice.stack_item_id
        || original_item.v4.source_contract != Some(original.source_contract)
        || original.spell != source.source
        || original.controller != source.controller
        || original.source_contract.zone_change_count != source.zone_change_count
        || original.source_contract.card_def != source.card_def
        || original.source_contract.owner != source.owner
        || original.targets_chosen != item.targets
        || original.target_contracts != item.v4.target_contracts
        || original.mode_chosen != Some(item.mode_chosen)
        || original.kicked != Some(item.kicked)
        || original.x_value.map(u16::from) != Some(item.v4.x_value)
        || original.is_flashback != item.is_flashback
        || original.mandatory_additional_cost
            != Some(ChoiceV1 {
                pay_mana: Some(choice.pay_mana),
                creature: choice.original_creature,
            })
    {
        return Err("mandatory paid choice differs from original input".into());
    }

    if state.engine.event_history.iter().filter(|event| matches!(event, CommittedEvent::MandatoryCastPaymentV1 { source: original, choice: paid, .. } if *original == source && *paid == choice)).count() != 1 {
        return Err("mandatory payment original receipt erased or duplicated".into());
    }
    let publication_index = state.engine.event_history.iter().position(|event| matches!(event, CommittedEvent::MandatoryCastPaymentV1 { source: original, choice: paid, .. } if *original == source && *paid == choice)).ok_or("mandatory receipt missing")?;
    if choice.input_index >= publication_index {
        return Err("mandatory payment input order changed".into());
    }
    match (choice.pay_mana, choice.original_creature, choice.paid_creature) {
        (true, None, None) if item.v4.paid_cost_refs.is_empty() => Ok(()),
        (false, Some(original), Some(paid)) if original.expected_zone == Zone::Battlefield && paid.object == original.object && item.v4.paid_cost_refs == [paid] && paid.zone_change_count == original.expected_zone_change_count.checked_add(1).ok_or("mandatory sacrifice generation overflow")? => {
            if state.engine.event_history[choice.input_index+1..publication_index].iter().any(|event| matches!(event, CommittedEvent::ZoneChange { object, from: Zone::Battlefield, controller_before, .. } if *object == original.object && *controller_before == item.controller)) { Ok(()) } else { Err("mandatory sacrifice original departure erased".into()) }
        }
        _ => Err("mandatory paid branch and cost references disagree".into()),
    }
}
pub(crate) fn require_legacy_supported(
    state: &GameState,
    actor: Option<PlayerId>,
) -> Result<(), String> {
    let own_hand = actor.is_some_and(|actor| {
        state.players[actor.index()].hand.iter().any(|id| {
            state.objects.try_get(*id).is_some_and(|object| {
                object.owner == actor
                    && object.zone == Zone::Hand
                    && card_def::CARD_DEFS
                        .get(usize::from(object.card_def))
                        .is_some_and(|def| mana_branch(def).is_some())
            })
        })
    });
    if own_hand
        || state
            .engine
            .pending_cast
            .as_ref()
            .is_some_and(|pending| pending.mandatory_additional_cost.is_some())
        || state.stack.iter().any(|item| {
            item.v4
                .source_contract
                .and_then(|source| source.finalized_cast_binding)
                .is_some_and(|binding| binding.mandatory_additional_cost.is_some())
        })
    {
        Err("mandatory sacrifice-or-additional-mana casting requires a distinct consumer".into())
    } else {
        Ok(())
    }
}

pub(super) fn record_input(
    state: &mut GameState,
    pending: &PendingCast,
) -> Option<(usize, StackItemId)> {
    pending.mandatory_additional_cost?;
    let original = state
        .stack
        .iter()
        .find(|item| item.v4.source_contract == Some(pending.source_contract))
        .expect("validated mandatory cast retains its original placeholder")
        .clone();
    let stack_item_id = original.v4.stack_item_id;
    let index = state.engine.event_history.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::MandatoryCastPaymentInputV1 {
            pending: Box::new(pending.clone()),
            item: Box::new(original),
        });
    Some((index, stack_item_id))
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::MandatoryCastPaymentV1 { .. }
                | CommittedEvent::MandatoryCastPaymentInputV1 { .. }
        )
    }) {
        return Err("private mandatory payment journal reintroduced into event log".into());
    }

    // Public boundaries have a genuine live row until the actual physical
    // source departs. Private resolver/counter loans run between these
    // boundaries; linked-Aura suspension restores its pre-operation row.
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::MandatoryCastPaymentInputV1 {
                pending,
                item: placeholder,
            } => {
                let publications =
                    state
                        .engine
                        .event_history
                        .iter()
                        .enumerate()
                        .filter(|(_, event)| {
                            matches!(event,
                    CommittedEvent::MandatoryCastPaymentV1 { source, choice, .. }
                        if choice.input_index == index && choice.stack_item_id == placeholder.v4.stack_item_id && source.source == pending.spell
                        && source.zone_change_count == pending.source_contract.zone_change_count
                        && source.controller == pending.controller)
                        });
                if publications.count() != 1 {
                    return Err("mandatory payment input lost its unique publication".into());
                }
            }
            CommittedEvent::MandatoryCastPaymentV1 {
                source,
                choice,
                item: publication,
            } => {
                let original = state.engine.event_history.get(choice.input_index);
                if choice.input_index >= index
                    || !matches!(original, Some(CommittedEvent::MandatoryCastPaymentInputV1 { pending, item: placeholder })
                    if placeholder.v4.stack_item_id == choice.stack_item_id && pending.source_contract.source == source.source
                        && pending.source_contract.zone_change_count == source.zone_change_count
                        && pending.mandatory_additional_cost == Some(ChoiceV1 { pay_mana: Some(choice.pay_mana), creature: choice.original_creature }))
                {
                    return Err("mandatory publication lost original ordered payment input".into());
                }
                if publication.v4.source_contract != Some(*source)
                    || publication.v4.stack_item_id != choice.stack_item_id
                    || publication.kind != StackItemKind::Spell
                {
                    return Err("mandatory captured publication metadata changed".into());
                }
                let definition = card_def::CARD_DEFS
                    .get(usize::from(source.card_def))
                    .ok_or("mandatory historical publication definition missing")?;
                validate_finalized(state, publication, definition, false)?;
                let departure = state.engine.event_history[index + 1..].iter().any(|event| matches!(event,
                    CommittedEvent::ZoneChange { object, from: Zone::Stack, .. } if *object == source.source));
                let live = state
                    .stack
                    .iter()
                    .filter(|item| item.v4.source_contract == Some(*source))
                    .collect::<Vec<_>>();
                if departure {
                    if !live.is_empty() {
                        return Err("departed mandatory spell publication was reintroduced".into());
                    }
                    if state.objects.try_get(source.source).is_some_and(|object| {
                        object.zone == Zone::Stack
                            && object.zone_change_count == source.zone_change_count
                    }) {
                        return Err("mandatory departure lacks actual physical source move".into());
                    }
                } else {
                    let [item] = live.as_slice() else {
                        return Err("mandatory spell publication lost its unique live owner".into());
                    };
                    if item.v4.stack_item_id != choice.stack_item_id
                        || *item != publication.as_ref()
                    {
                        return Err(
                            "mandatory spell publication differs from original finalized row"
                                .into(),
                        );
                    }
                    if item.kind != StackItemKind::Spell {
                        return Err("mandatory spell publication changed kind".into());
                    }
                    if validated_stack_item_target_spec(item, state)?
                        != Some(card_def::CARD_DEFS[usize::from(source.card_def)].target_spec)
                    {
                        return Err("mandatory spell publication changed target root".into());
                    }
                }
            }
            _ => {}
        }
    }
    for item in &state.stack {
        let Some(id) = item
            .v4
            .source_contract
            .map(|source| source.card_def)
            .or_else(|| {
                state
                    .objects
                    .try_get(item.source)
                    .map(|source| source.card_def)
            })
        else {
            continue;
        };
        let def = card_def::CARD_DEFS
            .get(usize::from(id))
            .ok_or("mandatory cost source definition absent")?;
        if mana_branch(def).is_some() {
            validate_spell_stack_source(state, item)?;
            if let Some(pending) = state
                .engine
                .pending_cast
                .as_ref()
                .filter(|pending| pending.spell == item.source)
            {
                validate_pending(state, pending)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::{put, ready_state_for};
    #[test]
    fn unresolved_or_component_cannot_be_paid_as_optional_or_free_generic_cost_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
            let creature = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Battlefield),
            );
            state.players[actor.index()].mana_pool[2] = 2;
            state.players[actor.index()].mana_pool[5] = 3;
            let before = serde_json::to_vec(&state).unwrap();
            assert!(!component_payment_shape_supported(
                crate::eaten_alive_v1::ADDITIONAL
            ));
            assert!(pay_cost_components_spending_mana(
                &mut state,
                actor,
                source,
                crate::eaten_alive_v1::ADDITIONAL,
                &[creature],
                0
            )
            .is_none());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
    #[test]
    fn foreign_optional_stage_on_genuine_bolt_pending_refuses_before_target_mutation_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
            state.players[actor.index()].mana_pool[3] = 1;
            step(&mut state, Action::CastSpell(source)).unwrap();
            assert!(matches!(
                advance_until_decision(&mut state),
                Decision::ChooseTargets { .. }
            ));
            state
                .engine
                .pending_cast
                .as_mut()
                .unwrap()
                .mandatory_additional_cost = Some(ChoiceV1 {
                pay_mana: None,
                creature: None,
            });
            let before = serde_json::to_vec(&state).unwrap();
            assert!(step(
                &mut state,
                Action::ChooseTarget(Target::Player(actor.opponent()))
            )
            .is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}
