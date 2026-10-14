//! Finalized spell and removal ownership for the still-unregistered printing.
//! Installation records supply the future death owner's grant inventory.
//! Cleanup/death custody and consumer admission are separate, unfinished work.

use super::grant_primitive::{primitive_install_grant_v50, PrimitiveGrantV50};
use crate::card_def::{CardCapability, CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{EffectObjectBinding, EffectOp};
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, StackItemId};
use crate::state::{GameState, StackItem, StackItemKind, StackTargetContractV4, Target, Zone};
use serde::{Deserialize, Serialize};

pub(crate) fn recipe() -> EffectOp {
    EffectOp::FakeInstallGrantV50
}

pub(crate) fn selected_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
        d.name == "Fake Your Own Death"
            && d.object_name == d.name
            && d.capability == CardCapability::Full
            && d.is_executable()
            && d.types == [CardType::Instant]
            && d.cost.generic == 1
            && d.cost.x_count == 0
            && d.cost.pips == [crate::mana::Pip::Colored(crate::mana::ManaColor::B)]
            && d.colors == [crate::mana::ManaColor::B]
            && d.mana_value == 2
            && !d.is_token
            && !d.is_land
            && d.keywords == Keywords::NONE
            && d.target_spec == TargetSpec::Creature
            && (d.spell_effect)() == Some(recipe())
            && d.activated_abilities.is_empty()
            && crate::trigger::triggers_for(id).is_empty()
            && d.subtypes.is_empty()
            && d.supertypes.is_empty()
            && d.power.is_none()
            && d.toughness.is_none()
            && d.ward_cost.is_none()
            && d.alt_cost.is_none()
            && d.mode2.is_none()
            && d.mode3.is_none()
            && d.additional_modes.is_empty()
            && d.flashback.is_none()
            && d.kicker_cost.is_none()
            && d.additional_cost.is_none()
            && d.optional_additional_cost.is_none()
            && d.equipment.is_none()
            && d.attachment.is_none()
            && d.produces_mana.is_empty()
            && d.mana_ability_choices.is_empty()
            && (d.mana_ability)().is_none()
            && d.starting_loyalty.is_none()
            && d.enters_with_plus_one_counters.is_none()
            && d.controlled_creature_grant.is_none()
            && d.controlled_counter_keyword.is_none()
            && d.game_outcome_protection_v1.is_none()
            && d.generic_cost_reduction.is_none()
            && !d.spell_cannot_be_countered
            && !d.records_mana_spent
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RemovalV50 {
    Resolving { target_legal: bool },
    Countered,
    OtherDeparture { zone: Zone },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalV50 {
    CopyCreated {
        item: Box<StackItem>,
    },
    CopyTargetInput {
        creation: u64,
        target: Option<(Target, StackTargetContractV4)>,
    },
    Published {
        item: Box<StackItem>,
    },
    RemovalInput {
        publication: u64,
        item: Box<StackItem>,
        removal: RemovalV50,
        timestamp: u64,
    },
    Installed {
        input: u64,
        recipient: EffectObjectBinding,
        timestamp: u64,
        grant: PrimitiveGrantV50,
    },
    WithoutGrant {
        input: u64,
    },
    SpellDeparted {
        publication: u64,
        item: Box<StackItem>,
        movement: Option<u64>,
    },
}

fn journal(event: &CommittedEvent) -> Option<&JournalV50> {
    match event {
        CommittedEvent::FakeJournalV50 { journal } => Some(journal),
        _ => None,
    }
}
fn emit(state: &mut GameState, value: JournalV50) -> u64 {
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::FakeJournalV50 {
            journal: Box::new(value),
        });
    index
}
fn selected_item(state: &GameState, item: &StackItem) -> bool {
    item.inline_effect == Some(recipe())
        || item
            .v4
            .source_contract
            .is_some_and(|s| selected_definition(s.card_def))
        || state
            .objects
            .try_get(item.source)
            .is_some_and(|o| selected_definition(o.card_def))
}
fn validate_spell(item: &StackItem) -> Result<EffectObjectBinding, String> {
    let source = item
        .v4
        .source_contract
        .ok_or("Fake spell lost source contract")?;
    let [Target::Object(target)] = item.targets.as_slice() else {
        return Err("Fake spell lost its single creature target".into());
    };
    let [StackTargetContractV4::Object {
        object,
        zone: Zone::Battlefield,
        zone_change_count,
        ..
    }] = item.v4.target_contracts.as_slice()
    else {
        return Err("Fake spell lost target incarnation".into());
    };
    if !selected_definition(source.card_def)
        || source.source != item.source
        || source.controller != item.controller
        || source.zone != Zone::Stack
        || item.kind != StackItemKind::Spell
        || item.inline_effect.is_some()
        || item.v4.target_spec != Some(TargetSpec::Creature)
        || target != object
        || item.madness_offer
        || item.kicked
        || item.is_flashback
        || item.v4.ability_source_contract.is_some()
        || item.v4.granted_by.is_some()
        || item.v4.x_value != 0
        || item.v4.optional_additional_cost_paid.is_some()
    {
        return Err("Fake finalized spell changed printing or target shape".into());
    }
    Ok(EffectObjectBinding {
        object: *object,
        expected_zone: Zone::Battlefield,
        expected_zone_change_count: *zone_change_count,
    })
}
fn publication_for(state: &GameState, item: &StackItem) -> Result<Option<u64>, String> {
    let matches = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match journal(e) {
            Some(JournalV50::Published { item: original })
                if original.v4.stack_item_id == item.v4.stack_item_id =>
            {
                Some((i as u64, original.as_ref()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [(index, original)] if *original == item => Ok(Some(*index)),
        _ => Err("Fake lost unique original finalized publication".into()),
    }
}
pub(crate) fn publish_spell(state: &mut GameState, id: StackItemId) -> Result<(), String> {
    let Some(item) = state
        .stack
        .iter()
        .find(|i| i.v4.stack_item_id == id)
        .cloned()
    else {
        return Ok(());
    };
    if !selected_item(state, &item) {
        return Ok(());
    }
    validate_spell(&item)?;
    if item.is_copy {
        let (creation, original) = copy_creation(state, id)?;
        let inputs = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match journal(e) {
                Some(JournalV50::CopyTargetInput {
                    creation: c,
                    target,
                }) if *c == creation => Some((i as u64, *target)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(input, target)] = inputs.as_slice() else {
            return Err("Fake copy lacks unique original target-finalization input".into());
        };
        let mut expected = original.clone();
        if let Some((target, contract)) = target {
            expected.targets = vec![*target];
            expected.v4.target_contracts = vec![*contract];
        }
        if *input <= creation || expected != item {
            return Err(
                "Fake copy finalization changed original row outside its target input".into(),
            );
        }
    }
    if publication_for(state, &item)?.is_some() {
        return Err("Fake spell published twice".into());
    }
    emit(
        state,
        JournalV50::Published {
            item: Box::new(item),
        },
    );
    Ok(())
}

fn copy_creation(state: &GameState, id: StackItemId) -> Result<(u64, &StackItem), String> {
    let originals = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match journal(e) {
            Some(JournalV50::CopyCreated { item }) if item.v4.stack_item_id == id => {
                Some((i as u64, item.as_ref()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    match originals.as_slice() {
        [(index, item)]
            if item.is_copy
                && item
                    .v4
                    .source_contract
                    .is_some_and(|s| s.spell_copy_origin.is_some()) =>
        {
            Ok((*index, *item))
        }
        _ => Err("Fake copy lost unique actual creation row".into()),
    }
}
pub(crate) fn publish_copy_created(state: &mut GameState, id: StackItemId) -> Result<(), String> {
    let Some(item) = state
        .stack
        .iter()
        .find(|i| i.v4.stack_item_id == id)
        .cloned()
    else {
        return Ok(());
    };
    if !selected_item(state, &item) {
        return Ok(());
    }
    validate_spell(&item)?;
    if !item.is_copy || !item.v4.source_contract.is_some_and(|s|s.spell_copy_origin.is_some())
        || state.engine.event_history.iter().any(|e|matches!(journal(e),Some(JournalV50::CopyCreated {item:old}) if old.v4.stack_item_id == id))
    { return Err("Fake copy creation changed original ancestry or duplicated its row".into()); }
    emit(
        state,
        JournalV50::CopyCreated {
            item: Box::new(item),
        },
    );
    Ok(())
}
pub(crate) fn record_copy_target_input(
    state: &mut GameState,
    id: StackItemId,
    target: Option<Target>,
) -> Result<(), String> {
    let Some(item) = state
        .stack
        .iter()
        .find(|i| i.v4.stack_item_id == id)
        .cloned()
    else {
        return Ok(());
    };
    if !selected_item(state, &item) {
        return Ok(());
    }
    let (creation, original) = copy_creation(state, id)?;
    if original != &item || publication_for(state,&item)?.is_some()
        || state.engine.event_history.iter().any(|e|matches!(journal(e),Some(JournalV50::CopyTargetInput {creation:c,..}) if *c == creation))
    { return Err("Fake copy targeting did not borrow its unresolved original creation".into()); }
    // The engine has independently checked a replacement's actual legality.
    let target = target.map(|t| (t, StackTargetContractV4::capture(state, t)));
    emit(state, JournalV50::CopyTargetInput { creation, target });
    Ok(())
}

/// A loan issued while the unique original spell row is still present.
/// It is deliberately neither serializable nor externally constructible.
pub(crate) struct RemovalOwnerV50 {
    publication: u64,
    item: StackItem,
}
pub(crate) fn acquire(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<RemovalOwnerV50>, String> {
    if !selected_item(state, item) {
        return Ok(None);
    }
    validate_spell(item)?;
    let publication = publication_for(state, item)?.ok_or("Fake removal lacks publication")?;
    if state.stack.iter().filter(|i| i.v4.stack_item_id == item.v4.stack_item_id).count() != 1
        || !state.stack.iter().any(|i| i == item)
        || state.engine.event_history.iter().any(|e| matches!(journal(e),Some(JournalV50::RemovalInput {publication:p,..}) if *p == publication))
    { return Err("Fake removal did not borrow an unresolved original live row".into()); }
    Ok(Some(RemovalOwnerV50 {
        publication,
        item: item.clone(),
    }))
}
pub(crate) fn resolve(
    state: &mut GameState,
    owner: RemovalOwnerV50,
    removal: RemovalV50,
) -> Result<(), String> {
    if state.stack.iter().any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
        || publication_for(state, &owner.item)? != Some(owner.publication)
        || state.engine.event_history.iter().any(|e| matches!(journal(e),Some(JournalV50::RemovalInput {publication,..}) if *publication == owner.publication))
    { return Err("Fake removal changed its original loan or left a live row".into()); }
    let recipient = validate_spell(&owner.item)?;
    let timestamp = state.engine.next_effect_timestamp;
    if removal == (RemovalV50::Resolving { target_legal: true }) {
        // The engine independently rechecks target legality before removal.
        // The primitive rechecks the exact live incarnation before installation.
        timestamp
            .checked_add(1)
            .ok_or("Fake installation timestamp overflow")?;
    }
    let input = emit(
        state,
        JournalV50::RemovalInput {
            publication: owner.publication,
            item: Box::new(owner.item.clone()),
            removal,
            timestamp,
        },
    );
    if removal == (RemovalV50::Resolving { target_legal: true }) {
        let grant = primitive_install_grant_v50(state, recipient)?;
        if !grant.matches_installation(recipient, timestamp) {
            return Err("Fake installation changed its issued recipient or timestamp".into());
        }
        emit(
            state,
            JournalV50::Installed {
                input,
                recipient,
                timestamp,
                grant,
            },
        );
    } else {
        emit(state, JournalV50::WithoutGrant { input });
    }
    // Ordinary central departure owns copy cessation and records it below.
    if owner.item.is_copy {
        state.stack.push(owner.item);
    }
    Ok(())
}

/// Original installation inventory, before future cleanup/death filtering.
/// This is not consumer admission or permission to publish a death trigger.
pub fn installed_grants(state: &GameState) -> Result<Vec<PrimitiveGrantV50>, String> {
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let Some(JournalV50::RemovalInput {
            publication,
            item,
            removal,
            ..
        }) = journal(event)
        else {
            continue;
        };
        validate_spell(item)?;
        if *publication >= index as u64 || publication_for(state, item)? != Some(*publication) {
            return Err("Fake removal lost preceding original publication".into());
        }
        let completions = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match journal(e) {
                Some(JournalV50::Installed { input, .. }) if *input == index as u64 => {
                    Some((i, true))
                }
                Some(JournalV50::WithoutGrant { input }) if *input == index as u64 => {
                    Some((i, false))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(completion, installed)] = completions.as_slice() else {
            return Err("Fake removal lost its unique installation or no-grant completion".into());
        };
        if *completion <= index
            || *installed != (*removal == (RemovalV50::Resolving { target_legal: true }))
        {
            return Err("Fake removal completion changed original legality or order".into());
        }
    }
    let mut result = Vec::new();
    let mut timestamps = std::collections::BTreeSet::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        if let Some(JournalV50::Installed {
            input,
            recipient,
            timestamp,
            grant,
        }) = journal(event)
        {
            let input_index = usize::try_from(*input).map_err(|_| "Fake input index overflow")?;
            let Some(JournalV50::RemovalInput {
                publication,
                item,
                removal: RemovalV50::Resolving { target_legal: true },
                timestamp: input_timestamp,
            }) = state
                .engine
                .event_history
                .get(input_index)
                .and_then(journal)
            else {
                return Err("Fake installation lost legal original removal input".into());
            };
            if input_index >= index || validate_spell(item)? != *recipient
                || publication_for(state,item)? != Some(*publication)
                || *publication >= *input || !grant.matches_installation(*recipient,*timestamp)
                || result.contains(grant)
                || timestamp != input_timestamp || !timestamps.insert(*timestamp)
                || state.engine.event_history.iter().filter(|e| matches!(journal(e), Some(JournalV50::Installed {input:i,..} | JournalV50::WithoutGrant {input:i}) if i == input)).count() != 1
                || state.engine.event_history.iter().filter(|e| matches!(journal(e), Some(JournalV50::RemovalInput {publication:p,..}) if p == publication)).count() != 1
            { return Err("Fake installation changed unique original grant provenance".into()); }
            result.push(*grant);
        }
    }
    Ok(result)
}
pub(crate) fn departure_candidate(state: &GameState, source: ObjectId) -> Option<(u64, StackItem)> {
    state.engine.event_history.iter().enumerate().rev().find_map(|(i,e)| match journal(e) {
        Some(JournalV50::Published {item}) if item.source == source
            && !state.engine.event_history.iter().any(|e| matches!(journal(e), Some(JournalV50::SpellDeparted {publication,..}) if *publication == i as u64)) => Some((i as u64,item.as_ref().clone())),
        _ => None,
    })
}
pub(crate) fn record_departure(
    state: &mut GameState,
    original: Option<(u64, StackItem)>,
    start: usize,
    ceased: bool,
) -> Result<(), String> {
    let Some((publication, item)) = original else {
        return Ok(());
    };
    if state
        .stack
        .iter()
        .any(|i| i.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("Fake departure left the original spell row live".into());
    }
    let movement = if item.is_copy {
        if !ceased {
            return Err("Fake virtual spell did not actually cease".into());
        }
        None
    } else {
        Some(state.engine.event_history.iter().enumerate().skip(start).find_map(|(i,e)| matches!(e,CommittedEvent::ZoneChange {object,from:Zone::Stack,..} if *object == item.source).then_some(i as u64)).ok_or("Fake departure lost actual spell movement")?)
    };
    emit(
        state,
        JournalV50::SpellDeparted {
            publication,
            item: Box::new(item),
            movement,
        },
    );
    Ok(())
}
