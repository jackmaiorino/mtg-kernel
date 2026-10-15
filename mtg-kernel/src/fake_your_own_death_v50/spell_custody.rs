//! Finalized spell and removal ownership for the Partial printing candidate.
//! Installation records supply the future death owner's grant inventory.
//! Cleanup binds expiry to an original complete ending-turn roster. Death
//! custody and full consumer admission remain separate, unfinished work.

use super::grant_primitive::{
    primitive_install_grant_v50, PrimitiveDeathLookbackV50, PrimitiveGrantV50,
};
use crate::card_def::{CardCapability, CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{EffectObjectBinding, EffectOp};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::state::{GameState, StackItem, StackItemKind, StackTargetContractV4, Target, Zone};
use serde::{Deserialize, Serialize};

pub(crate) fn recipe() -> EffectOp {
    EffectOp::FakeInstallGrantV50
}

pub(crate) fn selected_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS.get(usize::from(id)).is_some_and(owns_definition)
        && crate::trigger::triggers_for(id).is_empty()
}

pub(crate) fn owns_definition(d: &crate::card_def::CardDef) -> bool {
    d.name == "Fake Your Own Death"
            && d.object_name == d.name
            // Partial registration exposes genuine paid correctness fixtures
            // while full-deck/model preflight still rejects the incomplete card.
            && matches!(d.capability, CardCapability::Partial | CardCapability::Full)
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
            && d.plot_cost.is_none()
            && d.madness_cost.is_none()
            && d.kicked_target_spec.is_none()
            && d.escape.is_none()
            && !d.enters_battlefield_tapped
            && d.mana_ability_def.is_none()
            && d.minimum_blockers == 1
            && d.omen.is_none()
            && !d.mana_ability_includes_chosen_color
            && d.as_enters_choose_color_other_than.is_none()
            && d.additional_mana_abilities.is_empty()
            && d.enters_battlefield_tapped_unless.is_none()
            && d.transform_face.is_none()
            && d.saga.is_none()
            && !d.changeling
            && d.bestow.is_none()
            && !d.delve
            && d.adventure.is_none()
            && !d.cant_be_blocked_by_monarchs_creatures
            && d.conditional_tap_yield.is_none()
            && d.enters_tapped_unless_controller.is_none()
            && d.additional_mana_ability_conditions.is_empty()
            && d.restricted_mana_abilities.is_empty()
            && d.animation.is_none()
            && d.activated_ability_generic_reductions.is_empty()
            && d.combat_static_rules == crate::card_def::CombatStaticRulesDef::NONE
            && d.as_enters_color_choice_v1.is_none()
            && d.chosen_color_creature_boost_v1.is_none()
            && d.damage_replacement_v1.is_none()
            && d.token_counter_replacement_v48.is_none()
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
        turn: u32,
        active_player: PlayerId,
    },
    Installed {
        input: u64,
        recipient: EffectObjectBinding,
        timestamp: u64,
        grant: PrimitiveGrantV50,
        turn: u32,
        active_player: PlayerId,
    },
    WithoutGrant {
        input: u64,
    },
    SpellDeparted {
        publication: u64,
        item: Box<StackItem>,
        movement: Option<u64>,
    },
    CleanupEntered {
        turn: u32,
        active_player: PlayerId,
        installations: Vec<u64>,
    },
    CleanupCompleted {
        occurrence: u64,
    },
    DeathCaptured {
        input: u64,
        ordinal: u32,
        installation: u64,
        lookback: PrimitiveDeathLookbackV50,
        departure: u64,
        graveyard_source: Option<crate::state::AbilitySourceContractV4>,
    },
    DeathInput {
        lookbacks: Vec<(u64, PrimitiveDeathLookbackV50)>,
        departure: u64,
    },
    CollectionInput {
        captures: Vec<u64>,
    },
    DeathCollected {
        input: u64,
        ordinal: u32,
        capture: u64,
    },
    CollectionCompleted {
        input: u64,
    },
    DeathPublished {
        capture: u64,
        collection: u64,
        item: Box<StackItem>,
    },
    DeathCounterInput {
        publication: u64,
        original_stack: Vec<StackItem>,
    },
    DeathCounterClosed {
        input: u64,
        remaining_stack: Vec<StackItem>,
    },
    DeathResolutionInput {
        publication: u64,
        original_stack: Vec<StackItem>,
        context: Box<crate::effect::ExecCtx>,
        original_source: Option<Box<crate::state::GameObject>>,
        in_original_graveyard: bool,
        program: Box<EffectOp>,
    },
    DeathTreasureInput {
        resolution: u64,
        replacement: crate::doubling_season_v48::ReplacementInputV48,
    },
    DeathResolutionClosed {
        input: u64,
        treasure: u64,
        returned: Option<u64>,
        created: Vec<(u64, crate::ids::ObjectId)>,
    },
    AuraReturnInput {
        input: Box<super::return_aura::InputV50>,
    },
    AuraAttachmentCommitted {
        input: u64,
        source: crate::state::ObjectLinkV4,
        host: crate::state::ObjectLinkV4,
    },
    AuraReturnCompleted {
        input: u64,
        selected: Option<EffectObjectBinding>,
        returned: Option<u64>,
        attachment: Option<u64>,
    },
}

pub(super) fn journal(event: &CommittedEvent) -> Option<&JournalV50> {
    match event {
        CommittedEvent::FakeJournalV50 { journal } => Some(journal),
        _ => None,
    }
}
pub(crate) fn emit(state: &mut GameState, value: JournalV50) -> u64 {
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
pub(super) fn validate_spell(item: &StackItem) -> Result<EffectObjectBinding, String> {
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
pub(super) fn publication_for(state: &GameState, item: &StackItem) -> Result<Option<u64>, String> {
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
    validate_copy_publication(state, &item)?;
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

pub(super) fn validate_copy_publication(state: &GameState, item: &StackItem) -> Result<(), String> {
    if item.is_copy {
        let (creation, original) = copy_creation(state, item.v4.stack_item_id)?;
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
        if *input <= creation || expected != *item {
            return Err(
                "Fake copy finalization changed original row outside its target input".into(),
            );
        }
    }
    Ok(())
}

pub(super) fn copy_creation(
    state: &GameState,
    id: StackItemId,
) -> Result<(u64, &StackItem), String> {
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
    if !item.is_copy || item.v4.source_contract.is_none_or(|s|s.spell_copy_origin.is_none())
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
    validate_grant_lifetime(state)?;
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
    let turn = state.turn_key_v1();
    let active_player = state.active_player;
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
            turn,
            active_player,
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
                turn,
                active_player,
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
            turn,
            active_player,
        }) = journal(event)
        {
            let input_index = usize::try_from(*input).map_err(|_| "Fake input index overflow")?;
            let Some(JournalV50::RemovalInput {
                publication,
                item,
                removal: RemovalV50::Resolving { target_legal: true },
                timestamp: input_timestamp,
                turn: input_turn,
                active_player: input_active_player,
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
                || turn != input_turn || active_player != input_active_player
                || state.engine.event_history.iter().filter(|e| matches!(journal(e), Some(JournalV50::Installed {input:i,..} | JournalV50::WithoutGrant {input:i}) if i == input)).count() != 1
                || state.engine.event_history.iter().filter(|e| matches!(journal(e), Some(JournalV50::RemovalInput {publication:p,..}) if p == publication)).count() != 1
            { return Err("Fake installation changed unique original grant provenance".into()); }
            result.push(*grant);
        }
    }
    Ok(result)
}

fn installations_before(
    state: &GameState,
    end: usize,
) -> Vec<(u64, PrimitiveGrantV50, u32, PlayerId)> {
    state
        .engine
        .event_history
        .iter()
        .take(end)
        .enumerate()
        .filter_map(|(index, event)| match journal(event) {
            Some(JournalV50::Installed {
                grant,
                turn,
                active_player,
                ..
            }) => Some((index as u64, *grant, *turn, *active_player)),
            _ => None,
        })
        .collect()
}
fn expired_before(state: &GameState, installation: u64, end: usize) -> bool {
    state.engine.event_history.iter().take(end).any(|event| match journal(event) {
        Some(JournalV50::CleanupCompleted {occurrence}) => usize::try_from(*occurrence).ok()
            .and_then(|i|state.engine.event_history.get(i)).and_then(journal)
            .is_some_and(|entry|matches!(entry,JournalV50::CleanupEntered {installations,..} if installations.contains(&installation))),
        _ => false,
    })
}

/// Engine ingress checks these prepared grant/death facts before automatic
/// work or an actor action. This is not a whole printing/consumer validator.
pub(crate) fn validate_retained_grant_history(state: &GameState) -> Result<(), String> {
    if state
        .engine
        .event_history
        .iter()
        .any(|event| journal(event).is_some())
    {
        validate_grant_lifetime(state)?;
        super::death_custody::validate_retained_owners(state)?;
    } else if super::death_custody::has_unowned_roots(state) {
        super::death_custody::validate_retained_owners(state)?;
    }
    Ok(())
}

/// Validates retained installation lifetime independently from whether its
/// creature currently has the granted death ability. No trigger admission.
pub fn validate_grant_lifetime(state: &GameState) -> Result<(), String> {
    validate_death_captures(state)?;
    super::death_custody::validate_collection(state)?;
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match journal(event) {
            Some(JournalV50::CleanupEntered {
                turn,
                active_player,
                installations,
            }) => {
                let expected = installations_before(state, index)
                    .into_iter()
                    .filter(|(i, ..)| !expired_before(state, *i, index))
                    .collect::<Vec<_>>();
                if expected.is_empty()
                    || expected.iter().map(|(i, ..)| *i).collect::<Vec<_>>() != *installations
                    || expected
                        .iter()
                        .any(|(_, _, t, p)| t != turn || p != active_player)
                    || !matches!(state.engine.event_history.get(index+1).and_then(journal),Some(JournalV50::CleanupCompleted {occurrence}) if *occurrence==index as u64)
                {
                    return Err(
                        "Fake Cleanup lost its complete original ending-turn roster or completion"
                            .into(),
                    );
                }
            }
            Some(JournalV50::CleanupCompleted { occurrence }) => {
                if occurrence.checked_add(1) != Some(index as u64)
                    || !matches!(
                        usize::try_from(*occurrence)
                            .ok()
                            .and_then(|i| state.engine.event_history.get(i))
                            .and_then(journal),
                        Some(JournalV50::CleanupEntered { .. })
                    )
                {
                    return Err("Fake expiry lacks its adjacent original Cleanup occurrence".into());
                }
            }
            _ => {}
        }
    }
    for (index, grant, turn, active_player) in
        installations_before(state, state.engine.event_history.len())
    {
        let expired = expired_before(state, index, state.engine.event_history.len());
        if grant.temporary_modifier_retained(state)? == expired
            || (!expired && (turn != state.turn_key_v1() || active_player != state.active_player))
        {
            return Err(
                "Fake retained grant lost original modifier or authenticated expiry".into(),
            );
        }
    }
    Ok(())
}

pub(crate) struct CleanupOwnerV50 {
    occurrence: u64,
    installations: Vec<u64>,
    turn: u32,
    active_player: PlayerId,
}

/// Called in the actual Cleanup branch before ordinary temporary effects clear.
pub(crate) fn before_cleanup(state: &mut GameState) -> Result<Option<CleanupOwnerV50>, String> {
    validate_grant_lifetime(state)?;
    let end = state.engine.event_history.len();
    let installations = installations_before(state, end)
        .into_iter()
        .filter(|(i, ..)| !expired_before(state, *i, end))
        .map(|(i, ..)| i)
        .collect::<Vec<_>>();
    if installations.is_empty() {
        return Ok(None);
    }
    if state.step != crate::state::Step::Cleanup {
        return Err("Fake expiry attempted outside actual Cleanup".into());
    }
    let turn = state.turn_key_v1();
    let active_player = state.active_player;
    let occurrence = emit(
        state,
        JournalV50::CleanupEntered {
            turn,
            active_player,
            installations: installations.clone(),
        },
    );
    Ok(Some(CleanupOwnerV50 {
        occurrence,
        installations,
        turn,
        active_player,
    }))
}

/// A private nonserializable loan binds completion to the original pre-clear
/// occurrence. Suppressed and departed grants expire along with the whole roster.
pub(crate) fn finish_cleanup(
    state: &mut GameState,
    owner: Option<CleanupOwnerV50>,
) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    if state.step != crate::state::Step::Cleanup
        || state.turn_key_v1() != owner.turn
        || state.active_player != owner.active_player
        || !state.engine.until_end_of_turn.is_empty()
        || owner.occurrence.checked_add(1) != Some(state.engine.event_history.len() as u64)
        || !matches!(state.engine.event_history.last().and_then(journal),Some(JournalV50::CleanupEntered {turn,active_player,installations}) if *turn==owner.turn && *active_player==owner.active_player && *installations==owner.installations)
    {
        return Err(
            "Fake Cleanup completion changed original loan, phase, roster or actual clear".into(),
        );
    }
    emit(
        state,
        JournalV50::CleanupCompleted {
            occurrence: owner.occurrence,
        },
    );
    validate_grant_lifetime(state)
}

/// Called against the immutable pre-batch board after replacement selection.
/// Grant order is installation order, regardless of the dying card's name.
pub(crate) fn capture_death(
    state: &GameState,
    event: &ProposedEvent,
) -> Result<Vec<CommittedEvent>, String> {
    if !state
        .engine
        .event_history
        .iter()
        .any(|e| matches!(journal(e), Some(JournalV50::Installed { .. })))
    {
        return Ok(Vec::new());
    }
    validate_grant_lifetime(state)?;
    let end = state.engine.event_history.len();
    let mut lookbacks = Vec::new();
    for (installation, grant, ..) in installations_before(state, end) {
        if expired_before(state, installation, end) {
            continue;
        }
        if let Some(lookback) =
            super::grant_primitive::primitive_death_lookback_v50(state, grant, event)?
        {
            lookbacks.push((installation, lookback));
        }
    }
    if lookbacks.is_empty() {
        return Ok(Vec::new());
    }
    let mut captures = vec![CommittedEvent::FakeJournalV50 {
        journal: Box::new(JournalV50::DeathInput {
            lookbacks: lookbacks.clone(),
            departure: u64::MAX,
        }),
    }];
    for (ordinal, (installation, lookback)) in lookbacks.into_iter().enumerate() {
        captures.push(CommittedEvent::FakeJournalV50 {
            journal: Box::new(JournalV50::DeathCaptured {
                input: u64::MAX,
                ordinal: u32::try_from(ordinal).map_err(|_| "Fake capture ordinal overflow")?,
                installation,
                lookback,
                departure: u64::MAX,
                graveyard_source: None,
            }),
        });
    }
    Ok(captures)
}

/// The original pre-death operand becomes committed authority only beside the
/// exact real move, before a later move or token SBA can erase the graveyard.
pub(crate) fn bind_death_departure(
    state: &GameState,
    event: &mut CommittedEvent,
    index: usize,
) -> Result<(), String> {
    let CommittedEvent::FakeJournalV50 { journal: entry } = event else {
        return Ok(());
    };
    if let JournalV50::DeathInput {
        lookbacks,
        departure,
    } = entry.as_mut()
    {
        if *departure != u64::MAX || lookbacks.is_empty() {
            return Err("Fake death input changed original nonempty roster or was rebound".into());
        }
        for (installation, lookback) in lookbacks.iter() {
            validate_death_installation(state, *installation, lookback, index)?;
            super::grant_primitive::primitive_bind_committed_death_v50(
                state,
                lookback,
                index as u64,
            )?;
        }
        *departure = index as u64;
        return Ok(());
    }
    let JournalV50::DeathCaptured {
        input,
        ordinal,
        installation,
        lookback,
        departure,
        graveyard_source,
    } = entry.as_mut()
    else {
        return Ok(());
    };
    if *input != u64::MAX
        || *departure != u64::MAX
        || graveyard_source.is_some()
        || !matches!(usize::try_from(*installation).ok().and_then(|i|state.engine.event_history.get(i)).and_then(journal),Some(JournalV50::Installed {grant,..}) if *grant==lookback.grant)
        || expired_before(state, *installation, index)
    {
        return Err("Fake death capture changed original installation or was rebound".into());
    }
    let (input_index, originals) = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, e)| match journal(e) {
            Some(JournalV50::DeathInput {
                lookbacks,
                departure,
            }) if *departure == index as u64 => Some((i, lookbacks)),
            _ => None,
        })
        .ok_or("Fake capture lost original complete death input")?;
    if originals.get(*ordinal as usize) != Some(&(*installation, *lookback))
        || input_index
            .checked_add(1)
            .and_then(|i| i.checked_add(*ordinal as usize))
            != Some(state.engine.event_history.len())
    {
        return Err("Fake capture changed original roster member or order".into());
    }
    let context =
        super::grant_primitive::primitive_bind_committed_death_v50(state, lookback, index as u64)?;
    *graveyard_source = Some(
        context
            .ability_source_contract
            .ok_or("Fake committed death lost original graveyard source")?,
    );
    *departure = index as u64;
    *input = input_index as u64;
    Ok(())
}

fn validate_death_installation(
    state: &GameState,
    installation: u64,
    lookback: &PrimitiveDeathLookbackV50,
    departure: usize,
) -> Result<(), String> {
    if installation >= departure as u64
        || !matches!(usize::try_from(installation).ok().and_then(|i|state.engine.event_history.get(i)).and_then(journal),Some(JournalV50::Installed {grant,..}) if *grant==lookback.grant)
        || expired_before(state, installation, departure)
        || lookback.battlefield_source.zone != Zone::Battlefield
        || lookback.battlefield_source.zone_change_count.checked_add(1)
            != Some(lookback.graveyard_zone_change_count)
        || !matches!(state.engine.event_history.get(departure),Some(CommittedEvent::ZoneChange {object,from:Zone::Battlefield,to:Zone::Graveyard,controller_before}) if *object==lookback.battlefield_source.source && *controller_before==lookback.battlefield_source.controller)
    {
        return Err(
            "Fake death input changed original installation or exact battlefield death".into(),
        );
    }
    Ok(())
}

/// Validates every original complete input against the independently committed
/// contiguous capture roster. Later source movement never supplies a new LKI.
pub fn validate_death_captures(state: &GameState) -> Result<(), String> {
    installed_grants(state)?;
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match journal(event) {
            Some(JournalV50::DeathInput {
                lookbacks,
                departure,
            }) => {
                let death = usize::try_from(*departure).map_err(|_| "Fake death index overflow")?;
                if death >= index || lookbacks.is_empty() {
                    return Err(
                        "Fake death input lacks earlier actual death or original roster".into(),
                    );
                }
                if state.engine.event_history.iter().filter(|e|matches!(journal(e),Some(JournalV50::DeathInput {departure:d,..}) if d==departure)).count()!=1 {
                    return Err("Fake actual death has duplicate original input rosters".into());
                }
                for (ordinal, (installation, lookback)) in lookbacks.iter().enumerate() {
                    validate_death_installation(state, *installation, lookback, death)?;
                    if lookbacks[..ordinal]
                        .iter()
                        .any(|(i, l)| i == installation || l.grant == lookback.grant)
                    {
                        return Err("Fake death input repeats an original grant".into());
                    }
                    let capture = state
                        .engine
                        .event_history
                        .get(index + 1 + ordinal)
                        .and_then(journal);
                    let Some(JournalV50::DeathCaptured {
                        input,
                        ordinal: actual,
                        installation: i,
                        lookback: l,
                        departure: d,
                        graveyard_source: Some(g),
                    }) = capture
                    else {
                        return Err(
                            "Fake original death input lost its complete committed capture roster"
                                .into(),
                        );
                    };
                    if *input != index as u64
                        || *actual as usize != ordinal
                        || i != installation
                        || l != lookback
                        || d != departure
                        || g.source != lookback.battlefield_source.source
                        || g.card_def != lookback.battlefield_source.card_def
                        || g.owner != lookback.battlefield_source.owner
                        || g.controller != lookback.battlefield_source.owner
                        || g.attached_to.is_some()
                        || g.zone != Zone::Graveyard
                        || g.zone_change_count != lookback.graveyard_zone_change_count
                    {
                        return Err("Fake committed capture changed original roster, death or graveyard source".into());
                    }
                }
            }
            Some(JournalV50::DeathCaptured {
                input,
                ordinal,
                installation,
                lookback,
                ..
            }) => {
                let input_index =
                    usize::try_from(*input).map_err(|_| "Fake capture input index overflow")?;
                if input_index
                    .checked_add(1)
                    .and_then(|i| i.checked_add(*ordinal as usize))
                    != Some(index)
                    || !matches!(state.engine.event_history.get(input_index).and_then(journal),Some(JournalV50::DeathInput {lookbacks,..}) if lookbacks.get(*ordinal as usize)==Some(&(*installation,*lookback)))
                {
                    return Err(
                        "Fake committed capture lacks its original unique input position".into(),
                    );
                }
            }
            _ => {}
        }
    }
    Ok(())
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
