//! Targetless Faerie creation and its independently owned reflexive trigger.
//! Original engine input/publication journals are trusted committed occurrences.
use crate::card_def::{CardType, TargetSpec, CARD_DEFS};
use crate::effect::EffectOp;
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, StackItemId};
use crate::state::{GameState, StackItem, StackItemKind, StackSourceContractV4, Target, Zone};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};

pub(crate) fn recipe() -> EffectOp {
    EffectOp::FaebloomCreateFaeriesV43
}
pub(crate) fn selected_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
        d.name == "Faebloom Trick"
            && d.is_executable()
            && d.types == [CardType::Instant]
            && !d.is_token
            && !d.is_land
            && d.cost.generic == 2
            && d.cost.x_count == 0
            && d.cost.pips == [crate::mana::Pip::Colored(crate::mana::ManaColor::U)]
            && d.colors == [crate::mana::ManaColor::U]
            && d.subtypes.is_empty()
            && d.supertypes.is_empty()
            && d.power.is_none()
            && d.toughness.is_none()
            && d.ward_cost.is_none()
            && d.alt_cost.is_none()
            && d.mode2.is_none()
            && (d.spell_effect)() == Some(recipe())
            && d.target_spec == TargetSpec::None
    })
}
fn token_definition() -> Result<u16, String> {
    let id = crate::card_def::card_id_by_name("Faerie Token")
        .ok_or("Faebloom needs its actual Faerie token")?;
    let d = &CARD_DEFS[usize::from(id)];
    if !d.is_executable()
        || !d.is_token
        || !d.has_type(CardType::Creature)
        || d.power != Some(1)
        || d.toughness != Some(1)
        || crate::card_def::mana_colors_mask(d.colors) != 2
        || !d.keywords.contains(crate::card_def::Keywords::FLYING)
    {
        return Err("Faebloom Faerie blueprint changed".into());
    }
    Ok(id)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RemovalV43 {
    Resolving { target_legal: bool },
    Countered,
    OtherDeparture { zone: Zone },
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalV43 {
    Published {
        item: Box<StackItem>,
    },
    RemovalInput {
        publication: u64,
        item: Box<StackItem>,
        removal: RemovalV43,
    },
    Created {
        input: u64,
        token_events: Vec<u64>,
    },
    ReflexiveCaptured {
        creation: u64,
        history_index: u64,
        source: StackSourceContractV4,
    },
    Collected {
        capture: u64,
    },
    TriggerPublished {
        capture: u64,
        item: Box<StackItem>,
        target_at_placement: crate::rl::CardStableRefV1,
    },
    Dropped {
        capture: u64,
        pending: Box<PendingTrigger>,
    },
    Tapped {
        input: u64,
        event: u64,
    },
    NoLegalTarget {
        input: u64,
    },
    Countered {
        input: u64,
    },
    OtherDeparted {
        input: u64,
    },
    SpellDeparted {
        publication: u64,
        item: Box<StackItem>,
        movement: Option<u64>,
    },
    /// Original target visibility, issued immediately beside real Targeted.
    TargetInput {
        targeted_event: u64,
        item: Box<StackItem>,
        snapshot: Box<crate::state::GameObject>,
    },
}
fn journal(event: &CommittedEvent) -> Option<&JournalV43> {
    match event {
        CommittedEvent::FaebloomJournalV43 { journal } => Some(journal),
        _ => None,
    }
}
fn at(state: &GameState, index: u64) -> Result<&JournalV43, String> {
    state
        .engine
        .event_history
        .get(usize::try_from(index).map_err(|_| "Faebloom index overflow")?)
        .and_then(journal)
        .ok_or_else(|| "Faebloom lost original occurrence".into())
}
fn emit(state: &mut GameState, j: JournalV43) -> u64 {
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::FaebloomJournalV43 {
            journal: Box::new(j),
        });
    index
}
pub(crate) fn capture_of(op: &EffectOp) -> Option<u64> {
    match op {
        EffectOp::FaebloomTapFromCaptureV43 { capture, .. } => Some(*capture),
        _ => None,
    }
}
pub(crate) fn source_at(state: &GameState, index: u64) -> Result<StackSourceContractV4, String> {
    let JournalV43::ReflexiveCaptured {
        creation,
        history_index,
        source,
    } = at(state, index)?
    else {
        return Err("Faebloom trigger does not name a genuine creation capture".into());
    };
    if *history_index != index
        || *creation >= index
        || source.zone != Zone::Stack
        || !selected_definition(source.card_def)
    {
        return Err("Faebloom capture identity changed".into());
    }
    let JournalV43::Created { input, .. } = at(state, *creation)? else {
        return Err("Faebloom lost creation".into());
    };
    let JournalV43::RemovalInput {
        item,
        removal: RemovalV43::Resolving { target_legal: true },
        ..
    } = at(state, *input)?
    else {
        return Err("Faebloom capture lost actual resolving spell input".into());
    };
    if item.kind != StackItemKind::Spell || item.v4.source_contract != Some(*source) {
        return Err("Faebloom capture changed its original spell".into());
    }
    Ok(*source)
}
fn pending_for(source: StackSourceContractV4, capture: u64) -> PendingTrigger {
    PendingTrigger {
        controller: source.controller,
        source: source.source,
        effect: EffectOp::FaebloomTapFromCaptureV43 { capture, source },
        is_madness_offer: false,
        kicked: false,
        target_spec: TargetSpec::OpponentControlledCreature,
        targets: Vec::new(),
        target_contracts: Vec::new(),
        placement_ordered: false,
        source_contract: None,
        granted_by: None,
        optional_additional_cost_paid: None,
        paid_cost_refs: Vec::new(),
    }
}
pub(crate) fn validate_trigger_metadata(state: &GameState, item: &StackItem) -> Result<(), String> {
    let op = item
        .inline_effect
        .as_ref()
        .ok_or("Faebloom reflexive lost effect")?;
    let index = capture_of(op).ok_or("Faebloom reflexive changed its root")?;
    let source = source_at(state, index)?;
    if *op != pending_for(source, index).effect
        || item.source != source.source
        || item.controller != source.controller
        || item.v4.source_contract != Some(source)
        || item.kind != StackItemKind::TriggeredAbility
        || item.is_copy
        || item.madness_offer
        || item.is_flashback
        || item.kicked
        || item.v4.ability_source_contract.is_some()
        || item.v4.cast_method.is_some()
        || item.v4.granted_by.is_some()
        || item.v4.activated_ability_index.is_some()
        || item.v4.x_value != 0
        || !item.v4.paid_cost_refs.is_empty()
        || item.v4.optional_additional_cost_paid.is_some()
        || item.v4.face_index != 0
    {
        return Err("Faebloom reflexive changed original producer metadata".into());
    }
    Ok(())
}
fn selected_item(state: &GameState, item: &StackItem) -> bool {
    item.inline_effect.as_ref().and_then(capture_of).is_some()
        || item
            .v4
            .source_contract
            .is_some_and(|s| selected_definition(s.card_def))
        || state
            .objects
            .try_get(item.source)
            .is_some_and(|o| selected_definition(o.card_def))
}
fn validate_spell(item: &StackItem) -> Result<(), String> {
    let s = item
        .v4
        .source_contract
        .ok_or("Faebloom spell lost original contract")?;
    if !selected_definition(s.card_def)
        || s.source != item.source
        || s.controller != item.controller
        || s.zone != Zone::Stack
        || item.kind != StackItemKind::Spell
        || item.inline_effect.is_some()
        || item.v4.target_spec != Some(TargetSpec::None)
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || item.madness_offer
        || item.kicked
        || item.is_flashback
        || item.v4.ability_source_contract.is_some()
        || item.v4.granted_by.is_some()
        || item.v4.x_value != 0
        || item.v4.optional_additional_cost_paid.is_some()
    {
        return Err("Faebloom spell changed its genuine targetless printing".into());
    }
    Ok(())
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
    if item.kind != StackItemKind::Spell || !selected_item(state, &item) {
        return Ok(());
    }
    validate_spell(&item)?;
    if publication_for(state, &item).is_some() {
        return Err("Faebloom spell published twice".into());
    }
    emit(
        state,
        JournalV43::Published {
            item: Box::new(item),
        },
    );
    Ok(())
}
fn publication_for<'a>(state: &'a GameState, item: &StackItem) -> Option<(u64, &'a StackItem)> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .find_map(|(i, e)| match journal(e) {
            Some(
                JournalV43::Published { item: original }
                | JournalV43::TriggerPublished { item: original, .. },
            ) if original.v4.stack_item_id == item.v4.stack_item_id => {
                Some((i as u64, original.as_ref()))
            }
            _ => None,
        })
}
pub(crate) fn publish_trigger(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(capture) = item.inline_effect.as_ref().and_then(capture_of) else {
        return Ok(());
    };
    validate_trigger_metadata(state, item)?;
    if item.v4.target_spec != Some(TargetSpec::OpponentControlledCreature)
        || item.targets.len() != 1
        || item.v4.target_contracts.len() != 1
        || publication_for(state, item).is_some()
    {
        return Err("Faebloom trigger changed mandatory finalized target".into());
    }
    let inputs = matching(
        state,
        |j| matches!(j,JournalV43::TargetInput {item:original,..} if original.v4.stack_item_id==item.v4.stack_item_id),
    );
    let [(_, JournalV43::TargetInput { snapshot, .. })] = inputs.as_slice() else {
        return Err("Faebloom publication lost unique target visibility input".into());
    };
    let Some(Target::Object(target)) = item.targets.first() else {
        return Err("Faebloom placement target changed".into());
    };
    let target_at_placement =
        crate::sower_restriction_capture_v1::masked_ref(*target, snapshot).map_err(|e| e.0)?;
    emit(
        state,
        JournalV43::TriggerPublished {
            capture,
            item: Box::new(item.clone()),
            target_at_placement,
        },
    );
    Ok(())
}
pub(crate) fn record_target_input(
    state: &mut GameState,
    id: StackItemId,
    targeted_event: usize,
) -> Result<(), String> {
    let Some(item) = state
        .stack
        .iter()
        .find(|row| row.v4.stack_item_id == id)
        .cloned()
    else {
        return Ok(());
    };
    if item.inline_effect.as_ref().and_then(capture_of).is_none() {
        return Ok(());
    }
    validate_trigger_metadata(state, &item)?;
    let Some(Target::Object(target)) = item.targets.first() else {
        return Err("Faebloom target visibility has foreign target".into());
    };
    let snapshot = state.objects.get(*target).clone();
    if matching(
        state,
        |j| matches!(j,JournalV43::TargetInput {item:original,..} if original.v4.stack_item_id==id),
    )
    .len()
        != 0
    {
        return Err("Faebloom target visibility input duplicated".into());
    }
    emit(
        state,
        JournalV43::TargetInput {
            targeted_event: targeted_event as u64,
            item: Box::new(item),
            snapshot: Box::new(snapshot),
        },
    );
    Ok(())
}

/// This loan is acquired only while the genuine complete original row is live.
/// It is deliberately neither serializable nor caller-constructible.
pub(crate) struct RemovalOwnerV43 {
    publication: u64,
    item: StackItem,
}
pub(crate) fn acquire(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<RemovalOwnerV43>, String> {
    if !selected_item(state, item) {
        return Ok(None);
    }
    validate_state(state)?;
    if item.kind == StackItemKind::Spell {
        token_definition()?;
    }
    let (publication, original) =
        publication_for(state, item).ok_or("Faebloom lost mandatory publication")?;
    if original != item
        || state
            .stack
            .iter()
            .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
            .count()
            != 1
    {
        return Err("Faebloom removal did not borrow original live row".into());
    }
    Ok(Some(RemovalOwnerV43 {
        publication,
        item: item.clone(),
    }))
}
pub(crate) fn resolve(
    state: &mut GameState,
    owner: RemovalOwnerV43,
    removal: RemovalV43,
) -> Result<(), String> {
    if state
        .stack
        .iter()
        .any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
    {
        return Err("Faebloom owned row was not actually removed".into());
    }
    let input = emit(
        state,
        JournalV43::RemovalInput {
            publication: owner.publication,
            item: Box::new(owner.item.clone()),
            removal,
        },
    );
    match removal {
        RemovalV43::Countered => {
            emit(state, JournalV43::Countered { input });
        }
        RemovalV43::OtherDeparture { .. } => {
            emit(state, JournalV43::OtherDeparted { input });
        }
        RemovalV43::Resolving {
            target_legal: false,
        } => {
            if owner.item.kind != StackItemKind::TriggeredAbility {
                return Err("Faebloom targetless spell cannot fizzle".into());
            }
            emit(state, JournalV43::NoLegalTarget { input });
        }
        RemovalV43::Resolving { target_legal: true } if owner.item.kind == StackItemKind::Spell => {
            let token = token_definition()?;
            let start = state.engine.event_history.len();
            crate::event::propose_and_commit_batch(
                state,
                vec![
                    ProposedEvent::create_token(token, owner.item.controller),
                    ProposedEvent::create_token(token, owner.item.controller),
                ],
            );
            if state.engine.halted.is_some() {
                return Err("Faebloom genuine token batch halted".into());
            }
            let token_events = state
                .engine
                .event_history
                .iter()
                .enumerate()
                .skip(start)
                .filter_map(|(i, e)| {
                    matches!(e,CommittedEvent::CreateToken {token_def,controller,..}
                    if *token_def==token && *controller==owner.item.controller)
                    .then_some(i as u64)
                })
                .collect::<Vec<_>>();
            if token_events.len() != 2 {
                return Err("Faebloom actual token creation count changed".into());
            }
            let creation = emit(
                state,
                JournalV43::Created {
                    input,
                    token_events,
                },
            );
            let source = owner
                .item
                .v4
                .source_contract
                .ok_or("Faebloom lost source after removal")?;
            let history_index = state.engine.event_history.len() as u64;
            let capture = JournalV43::ReflexiveCaptured {
                creation,
                history_index,
                source,
            };
            emit(state, capture.clone());
            state
                .engine
                .event_log
                .push(CommittedEvent::FaebloomJournalV43 {
                    journal: Box::new(capture),
                });
        }
        RemovalV43::Resolving { target_legal: true } => {
            let Some(Target::Object(target)) = owner.item.targets.first() else {
                return Err("Faebloom tap changed target kind".into());
            };
            let before = state.engine.event_history.len();
            crate::event::propose_and_commit(state, ProposedEvent::tap(*target));
            let event = state
                .engine
                .event_history
                .iter()
                .enumerate()
                .skip(before)
                .find_map(|(i, e)| {
                    matches!(e,CommittedEvent::Tap {object} if object==target).then_some(i as u64)
                })
                .ok_or("Faebloom legal tap did not commit")?;
            emit(state, JournalV43::Tapped { input, event });
        }
    }
    if owner.item.kind == StackItemKind::Spell && owner.item.is_copy {
        state.stack.push(owner.item);
    }
    Ok(())
}
pub(crate) fn departure_candidate(state: &GameState, source: ObjectId) -> Option<(u64, StackItem)> {
    state.engine.event_history.iter().enumerate().rev().find_map(|(index,e)|match journal(e) {
        Some(JournalV43::Published {item}) if item.source==source
            && !state.engine.event_history.iter().any(|e|matches!(journal(e),Some(JournalV43::SpellDeparted {publication,..}) if *publication==index as u64))
            =>Some((index as u64,item.as_ref().clone())), _=>None })
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
        return Err("Faebloom departure left live spell".into());
    }
    let movement = if item.is_copy {
        if !ceased {
            return Err("Faebloom copy never actually ceased".into());
        }
        None
    } else {
        Some(state.engine.event_history.iter().enumerate().skip(start).find_map(|(i,e)|
            matches!(e,CommittedEvent::ZoneChange {object,from:Zone::Stack,..} if *object==item.source).then_some(i as u64))
            .ok_or("Faebloom departure lost real stack movement")?)
    };
    emit(
        state,
        JournalV43::SpellDeparted {
            publication,
            item: Box::new(item),
            movement,
        },
    );
    Ok(())
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, ObjectId> {
    let mut result = Vec::new();
    for event in events {
        let Some(JournalV43::ReflexiveCaptured {
            history_index,
            source,
            ..
        }) = journal(event)
        else {
            continue;
        };
        if at(state,*history_index).ok()!=journal(event) || source_at(state,*history_index).ok()!=Some(*source)
            || state.engine.event_history.iter().any(|e|matches!(journal(e),Some(JournalV43::Collected {capture}) if capture==history_index)) {
            return Err(source.source)
        }
        emit(
            state,
            JournalV43::Collected {
                capture: *history_index,
            },
        );
        result.push(pending_for(*source, *history_index));
    }
    Ok(result)
}
pub(crate) fn drop_unavailable(
    state: &mut GameState,
    pending: &PendingTrigger,
) -> Result<(), String> {
    let Some(capture) = capture_of(&pending.effect) else {
        return Ok(());
    };
    let source = source_at(state, capture)?;
    if pending.source != source.source
        || pending.controller != source.controller
        || crate::engine::pending_trigger_targets_can_complete(pending, state)
    {
        return Err("Faebloom unavailable closure had a legal target".into());
    }
    emit(
        state,
        JournalV43::Dropped {
            capture,
            pending: Box::new(pending.clone()),
        },
    );
    Ok(())
}
fn waiting(state: &GameState) -> impl Iterator<Item = &PendingTrigger> {
    state
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
}
pub(crate) fn owns_pending(state: &GameState, pending: &PendingTrigger) -> bool {
    waiting(state).filter(|row| *row == pending).count() == 1
}
fn matching(state: &GameState, predicate: impl Fn(&JournalV43) -> bool) -> Vec<(u64, &JournalV43)> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| journal(e).filter(|j| predicate(j)).map(|j| (i as u64, j)))
        .collect()
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    // Captures alone are trigger events; every other receipt is private history.
    for e in &state.engine.event_log {
        if let Some(j) = journal(e) {
            let JournalV43::ReflexiveCaptured { history_index, .. } = j else {
                return Err("Faebloom private journal entered public collection log".into());
            };
            if at(state, *history_index)? != j {
                return Err("Faebloom log capture lost immutable original".into());
            }
        }
    }
    for row in &state.stack {
        if selected_item(state, row) {
            let (_, original) =
                publication_for(state, row).ok_or("Faebloom live row lost original publication")?;
            if original != row {
                return Err("Faebloom published row changed".into());
            }
        }
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let Some(j) = journal(event) else { continue };
        let index = index as u64;
        match j {
            JournalV43::Published { item } | JournalV43::TriggerPublished { item, .. } => {
                if let JournalV43::TriggerPublished { capture, item, .. } = j {
                    if item.inline_effect.as_ref().and_then(capture_of) != Some(*capture) {
                        return Err("Faebloom publication redirected capture".into());
                    }
                }
                if let JournalV43::TriggerPublished {
                    item,
                    target_at_placement,
                    ..
                } = j
                {
                    let inputs = matching(
                        state,
                        |j| matches!(j,JournalV43::TargetInput {item:original,..} if original.v4.stack_item_id==item.v4.stack_item_id),
                    );
                    let [(
                        input_at,
                        JournalV43::TargetInput {
                            item: original,
                            snapshot,
                            ..
                        },
                    )] = inputs.as_slice()
                    else {
                        return Err(
                            "Faebloom published target lost original visibility input".into()
                        );
                    };
                    let Some(Target::Object(target)) = item.targets.first() else {
                        return Err("Faebloom published target kind changed".into());
                    };
                    if original.as_ref() != item.as_ref()
                        || *input_at >= index
                        || crate::sower_restriction_capture_v1::masked_ref(*target, snapshot)
                            .map_err(|e| e.0)?
                            != *target_at_placement
                    {
                        return Err(
                            "Faebloom published mask changed original target visibility".into()
                        );
                    }
                }
                if let JournalV43::TriggerPublished { item, .. } = j {
                    let [crate::state::StackTargetContractV4::Object {
                        object,
                        zone: Zone::Battlefield,
                        zone_change_count,
                        ..
                    }] = item.v4.target_contracts.as_slice()
                    else {
                        return Err("Faebloom finalized trigger lost creature incarnation".into());
                    };
                    if item.targets!=vec![Target::Object(*object)] || state.engine.event_history.iter().take(index as usize).filter(|e|
                        matches!(e,CommittedEvent::Targeted {target,target_zone_change_count,targeting_stack_item,targeting_controller}
                            if target==object && target_zone_change_count==zone_change_count && *targeting_stack_item==item.v4.stack_item_id && *targeting_controller==item.controller)).count()!=1 {
                        return Err("Faebloom finalized recipient changed actual Targeted occurrence".into())
                    }
                }
                if matches!(j, JournalV43::Published { .. }) {
                    validate_spell(item)?;
                } else {
                    validate_trigger_metadata(state, item)?;
                }
                if publication_for(state,item)!=Some((index,item.as_ref())) || item.v4.stack_item_id==StackItemId::default()
                    || matching(state,|j|matches!(j,JournalV43::Published {item:other}|JournalV43::TriggerPublished {item:other,..} if other.v4.stack_item_id==item.v4.stack_item_id)).len()!=1 {
                    return Err("Faebloom duplicate or relocated publication".into())
                }
                let live = state
                    .stack
                    .iter()
                    .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                    .count();
                let inputs = matching(
                    state,
                    |j| matches!(j,JournalV43::RemovalInput {publication,..} if *publication==index),
                );
                if inputs.is_empty() {
                    if live != 1 {
                        return Err("Faebloom mandatory live owner erased".into());
                    }
                } else if inputs.len() != 1 || live != 0 || inputs[0].0 <= index {
                    return Err("Faebloom removed publication was replayed".into());
                }
                if item.kind == StackItemKind::Spell {
                    let departures = matching(
                        state,
                        |j| matches!(j,JournalV43::SpellDeparted {publication,..} if *publication==index),
                    );
                    if inputs.is_empty() && !departures.is_empty()
                        || !inputs.is_empty()
                            && (departures.len() != 1 || departures[0].0 <= inputs[0].0)
                    {
                        return Err("Faebloom closed spell lost actual final departure".into());
                    }
                }
            }
            JournalV43::RemovalInput {
                publication,
                item,
                removal,
            } => {
                let original = match at(state, *publication)? {
                    JournalV43::Published { item } | JournalV43::TriggerPublished { item, .. } => {
                        item
                    }
                    _ => return Err("Faebloom input names foreign publication".into()),
                };
                if *publication >= index || item != original {
                    return Err("Faebloom removal changed original full item".into());
                }
                let outputs = matching(
                    state,
                    |j| matches!(j,JournalV43::Created {input,..}|JournalV43::Tapped {input,..}|JournalV43::NoLegalTarget {input}|JournalV43::Countered {input}|JournalV43::OtherDeparted {input} if *input==index),
                );
                if outputs.len() != 1 || outputs[0].0 <= index {
                    return Err("Faebloom actual removal lost unique completion".into());
                }
                let valid = matches!(
                    (removal, outputs[0].1),
                    (
                        RemovalV43::OtherDeparture { .. },
                        JournalV43::OtherDeparted { .. }
                    ) | (RemovalV43::Countered, JournalV43::Countered { .. })
                        | (
                            RemovalV43::Resolving {
                                target_legal: false
                            },
                            JournalV43::NoLegalTarget { .. }
                        )
                ) || matches!(
                    (removal, outputs[0].1, item.kind),
                    (
                        RemovalV43::Resolving { target_legal: true },
                        JournalV43::Created { .. },
                        StackItemKind::Spell
                    ) | (
                        RemovalV43::Resolving { target_legal: true },
                        JournalV43::Tapped { .. },
                        StackItemKind::TriggeredAbility
                    )
                );
                if matches!(removal, RemovalV43::OtherDeparture { .. })
                    && item.kind != StackItemKind::Spell
                    || matches!(
                        removal,
                        RemovalV43::Resolving {
                            target_legal: false
                        }
                    ) && item.kind != StackItemKind::TriggeredAbility
                {
                    return Err("Faebloom removal outcome changed original item kind".into());
                }
                if !valid {
                    return Err("Faebloom outcome changed actual removal input".into());
                }
            }
            JournalV43::Created {
                input,
                token_events,
            } => {
                let JournalV43::RemovalInput {
                    item,
                    removal: RemovalV43::Resolving { target_legal: true },
                    ..
                } = at(state, *input)?
                else {
                    return Err("Faebloom creation lost resolving input".into());
                };
                let token = token_definition()?;
                if item.kind!=StackItemKind::Spell || *input>=index || token_events.len()!=2 || token_events[0]>=token_events[1]
                    || token_events.iter().any(|at| *at<=*input || *at>=index || !matches!(state.engine.event_history.get(*at as usize),Some(CommittedEvent::CreateToken {token_def,controller,..}) if *token_def==token && *controller==item.controller))
                    || matching(state,|j|matches!(j,JournalV43::ReflexiveCaptured {creation,..} if *creation==index)).len()!=1 {
                    return Err("Faebloom completion lost actual two-token operation or reflexive capture".into())
                }
            }
            JournalV43::ReflexiveCaptured {
                history_index,
                source,
                ..
            } => {
                source_at(state, index)?;
                if *history_index != index {
                    return Err("Faebloom capture moved".into());
                }
                let collected = matching(
                    state,
                    |j| matches!(j,JournalV43::Collected {capture} if *capture==index),
                );
                let logged = state
                    .engine
                    .event_log
                    .iter()
                    .filter(|e| journal(e) == Some(j))
                    .count();
                let pending = waiting(state)
                    .filter(|p| capture_of(&p.effect) == Some(index))
                    .collect::<Vec<_>>();
                for p in &pending {
                    if p.effect != pending_for(*source, index).effect
                        || p.controller != source.controller
                        || p.source != source.source
                        || p.source_contract.is_some()
                        || p.granted_by.is_some()
                        || p.target_spec != TargetSpec::OpponentControlledCreature
                        || p.kicked
                        || p.is_madness_offer
                        || p.optional_additional_cost_paid.is_some()
                        || !p.paid_cost_refs.is_empty()
                    {
                        return Err("Faebloom waiting reflexive changed original owner/root".into());
                    }
                }
                let finished = matching(
                    state,
                    |j| matches!(j,JournalV43::TriggerPublished {capture,..}|JournalV43::Dropped {capture,..} if *capture==index),
                );
                if collected.is_empty() {
                    if logged != 1 || !pending.is_empty() || !finished.is_empty() {
                        return Err("Faebloom fresh reflexive lost sole log custody".into());
                    }
                } else if collected.len() != 1
                    || collected[0].0 <= index
                    || logged != 0
                    || pending.len() + finished.len() != 1
                    || finished.iter().any(|(at, _)| *at <= collected[0].0)
                {
                    return Err(
                        "Faebloom mandatory reflexive was erased, duplicated or replayed".into(),
                    );
                }
            }
            JournalV43::TargetInput {
                targeted_event,
                item,
                snapshot,
            } => {
                validate_trigger_metadata(state, item)?;
                let [crate::state::StackTargetContractV4::Object {
                    object,
                    card_def,
                    owner,
                    controller,
                    zone: Zone::Battlefield,
                    zone_change_count,
                    spell_copy_origin,
                }] = item.v4.target_contracts.as_slice()
                else {
                    return Err("Faebloom target input lost original creature contract".into());
                };
                if *targeted_event+1!=index || item.targets!=vec![Target::Object(*object)]
                    || snapshot.card_def!=*card_def || snapshot.owner!=*owner || snapshot.controller!=*controller
                    || snapshot.zone!=Zone::Battlefield || snapshot.zone_change_count!=*zone_change_count || snapshot.spell_copy_origin!=*spell_copy_origin
                    || state.engine.event_history.get(*targeted_event as usize)!=Some(&CommittedEvent::Targeted {target:*object,target_zone_change_count:*zone_change_count,targeting_stack_item:item.v4.stack_item_id,targeting_controller:item.controller})
                    || matching(state,|j|matches!(j,JournalV43::TriggerPublished {item:published,..} if published.v4.stack_item_id==item.v4.stack_item_id)).len()!=1 {
                    return Err("Faebloom target input changed real Targeted or original visibility".into())
                }
            }
            JournalV43::Collected { capture } | JournalV43::Dropped { capture, .. } => {
                if let JournalV43::Dropped { capture, pending } = j {
                    let expected = pending_for(source_at(state, *capture)?, *capture);
                    if pending.effect != expected.effect
                        || pending.source != expected.source
                        || pending.controller != expected.controller
                        || pending.source_contract.is_some()
                        || pending.granted_by.is_some()
                        || pending.target_spec != expected.target_spec
                        || pending.is_madness_offer
                        || pending.kicked
                        || pending.optional_additional_cost_paid.is_some()
                        || !pending.paid_cost_refs.is_empty()
                    {
                        return Err("Faebloom drop changed original mandatory trigger".into());
                    }
                }
                source_at(state, *capture)?;
                if *capture >= index {
                    return Err("Faebloom collection/closure preceded capture".into());
                }
            }
            JournalV43::Tapped { input, event } => {
                let JournalV43::RemovalInput {
                    item,
                    removal: RemovalV43::Resolving { target_legal: true },
                    ..
                } = at(state, *input)?
                else {
                    return Err("Faebloom tap lost actual removal input".into());
                };
                let Some(Target::Object(target)) = item.targets.first() else {
                    return Err("Faebloom tap changed target".into());
                };
                if *input >= *event
                    || *event >= index
                    || state.engine.event_history.get(*event as usize)
                        != Some(&CommittedEvent::Tap { object: *target })
                {
                    return Err("Faebloom completed tap lost actual tap occurrence".into());
                }
            }
            JournalV43::SpellDeparted {
                publication,
                item,
                movement,
            } => {
                let JournalV43::Published { item: original } = at(state, *publication)? else {
                    return Err("Faebloom departure named foreign publication".into());
                };
                if *publication>=index || item!=original || item.is_copy!=movement.is_none()
                    || movement.is_some_and(|at|at>=index || !matches!(state.engine.event_history.get(at as usize),Some(CommittedEvent::ZoneChange {object,from:Zone::Stack,..}) if *object==item.source)) {
                    return Err("Faebloom final departure changed original spell or movement".into())
                }
            }
            JournalV43::NoLegalTarget { input }
            | JournalV43::Countered { input }
            | JournalV43::OtherDeparted { input } => {
                if *input >= index || !matches!(at(state, *input)?, JournalV43::RemovalInput { .. })
                {
                    return Err("Faebloom closure lost actual removal".into());
                }
            }
        }
    }
    for p in waiting(state).filter(|p| capture_of(&p.effect).is_some()) {
        source_at(state, capture_of(&p.effect).unwrap())?;
    }
    Ok(())
}

pub(crate) fn requires_successor(state: &GameState, actor: Option<crate::state::PlayerId>) -> bool {
    state
        .engine
        .event_history
        .iter()
        .any(|e| journal(e).is_some())
        || state.objects.iter().any(|(_, o)| {
            selected_definition(o.card_def)
                && (matches!(
                    o.zone,
                    Zone::Battlefield | Zone::Stack | Zone::Graveyard | Zone::Exile
                ) || o.zone == Zone::Hand && actor == Some(o.owner))
        })
}
pub(crate) fn require_legacy_supported(
    state: &GameState,
    actor: Option<crate::state::PlayerId>,
) -> Result<(), String> {
    if requires_successor(state, actor) {
        return Err("Faebloom reflexive creation requires V43".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
