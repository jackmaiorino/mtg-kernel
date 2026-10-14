//! Untargeted Aura return under the genuine gained-death operation.
use super::spell_custody::{emit, journal, JournalV50};
use crate::effect::EffectObjectBinding;
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::PlayerId;
use crate::linked_aura_operation_v1::{GeneralAnswerV36, GeneralChoiceV36, OperationV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV50 {
    pub resolution: u64,
    pub source: AbilitySourceContractV4,
    pub aura: EffectObjectBinding,
    pub owner: PlayerId,
    pub candidates: Vec<EffectObjectBinding>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputV50 {
    pub original: Box<GameState>,
    pub operation: OperationV1,
    pub prior_answers: Vec<GeneralAnswerV36>,
    pub choice: ChoiceV50,
}
impl Hash for InputV50 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        serde_json::to_vec(self)
            .expect("owned Aura input serializes")
            .hash(state);
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointV50 {
    pub original: Box<GameState>,
    pub operation: OperationV1,
    pub answers: Vec<GeneralAnswerV36>,
    pub choice: GeneralChoiceV36,
    pub input: u64,
    pub used_banner: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionV50 {
    Suspended,
    NoHost,
    Host(EffectObjectBinding),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntryLoanV50 {
    input: u64,
    source: ObjectLinkV4,
    host: ObjectLinkV4,
}

pub(crate) fn pending(state: &GameState) -> Option<&CheckpointV50> {
    state
        .engine
        .linked_aura_operation_v1
        .pending_fake_aura_v50
        .as_deref()
}

fn hosts(state: &GameState, aura: EffectObjectBinding) -> Result<Vec<EffectObjectBinding>, String> {
    let candidates = crate::effect::returning_aura_hosts(state, aura)?;
    let colors = crate::engine::object_color_mask(state, aura.object);
    Ok(candidates
        .into_iter()
        .filter(|host| {
            !crate::controlled_creature_grants_v1::protects_from_colors(state, host.object, colors)
        })
        .collect())
}

pub(crate) fn prepare(state: &GameState, resolution: u64) -> Result<ChoiceV50, String> {
    let Some(JournalV50::DeathResolutionInput {
        context,
        original_source: Some(original),
        in_original_graveyard: true,
        program,
        ..
    }) = state
        .engine
        .event_history
        .get(resolution as usize)
        .and_then(journal)
    else {
        return Err("Fake Aura return lost its original resolution input".into());
    };
    if !matches!(program.as_ref(), crate::effect::EffectOp::Sequence(steps) if steps.len() == 2)
        || crate::card_def::CARD_DEFS[usize::from(original.card_def)]
            .attachment
            .is_none()
    {
        return Err("Fake Aura choice borrowed a non-Aura or no-return program".into());
    }
    let source = context
        .ability_source_contract
        .ok_or("Fake Aura lost graveyard context")?;
    let aura = EffectObjectBinding {
        object: source.source,
        expected_zone: Zone::Graveyard,
        expected_zone_change_count: source.zone_change_count,
    };
    if state.objects.try_get(aura.object) != Some(original.as_ref())
        || source.zone != Zone::Graveyard
        || original.zone != Zone::Graveyard
        || original.zone_change_count != source.zone_change_count
        || source.card_def != original.card_def
        || source.owner != original.owner
        || original.v4.is_token
        || !state.players[source.owner.index()]
            .graveyard
            .contains(&source.source)
    {
        return Err("Fake Aura choice lost its actual original graveyard card".into());
    }
    Ok(ChoiceV50 {
        resolution,
        source,
        aura,
        owner: source.owner,
        candidates: hosts(state, aura)?,
    })
}

pub(crate) fn validate_choice(state: &GameState, choice: &ChoiceV50) -> Result<(), String> {
    if prepare(state, choice.resolution)? != *choice {
        return Err("Fake Aura owner or complete untargeted host menu changed".into());
    }
    Ok(())
}

pub(crate) fn resolve_return(state: &mut GameState, resolution: u64) -> Result<bool, String> {
    let choice = prepare(state, resolution)?;
    let (input, selected) =
        crate::linked_aura_operation_v1::choose_fake_aura_v50(state, choice.clone())?;
    let selected = match selected {
        SelectionV50::Suspended => return Ok(false),
        SelectionV50::NoHost => None,
        SelectionV50::Host(host) => Some(host),
    };
    if let Some(host) = selected {
        let loan = EntryLoanV50 {
            input,
            source: ObjectLinkV4 {
                object: choice.aura.object,
                zone_change_count: choice
                    .aura
                    .expected_zone_change_count
                    .checked_add(1)
                    .ok_or("Fake Aura generation overflow")?,
            },
            host: ObjectLinkV4 {
                object: host.object,
                zone_change_count: host.expected_zone_change_count,
            },
        };
        crate::linked_aura_operation_v1::install_fake_aura_entry_v50(state, loan)?;
        crate::event::batch_with_linked_aura_attachments_v1(
            state,
            vec![ProposedEvent::zone_change_to_battlefield_tapped(
                choice.aura.object,
            )],
            vec![(choice.aura, host)],
        );
        crate::linked_aura_operation_v1::clear_fake_aura_entry_v50(state);
        if state.engine.halted.is_some() {
            return Err("Fake attached Aura return halted".into());
        }
    }
    let start = input as usize + 1;
    let returned = state.engine.event_history.iter().enumerate().skip(start).filter_map(|(at, event)|
        matches!(event, CommittedEvent::ZoneChange { object, from: Zone::Graveyard, to: Zone::Battlefield, .. }
            if *object == choice.aura.object).then_some(at as u64)).collect::<Vec<_>>();
    let attached = state.engine.event_history.iter().enumerate().skip(start).filter_map(|(at, event)|
        matches!(journal(event), Some(JournalV50::AuraAttachmentCommitted { input: original, .. })
            if *original == input).then_some(at as u64)).collect::<Vec<_>>();
    if returned.len() != usize::from(selected.is_some())
        || attached.len() != usize::from(selected.is_some())
    {
        return Err(
            "Fake Aura return lost its actual move or independent attachment commit".into(),
        );
    }
    emit(
        state,
        JournalV50::AuraReturnCompleted {
            input,
            selected,
            returned: returned.first().copied(),
            attachment: attached.first().copied(),
        },
    );
    Ok(true)
}

/// Called only by the actual two-way attachment commit with a private local loan.
pub(crate) fn record_attachment(
    state: &mut GameState,
    source: ObjectLinkV4,
    host: ObjectLinkV4,
) -> Result<(), String> {
    let Some(loan) = crate::linked_aura_operation_v1::fake_aura_entry_v50(state).cloned() else {
        return Ok(());
    };
    if source.object != loan.source.object {
        return Ok(());
    }
    if source != loan.source || host != loan.host
        || state.objects.get(source.object).v4.attached_to != Some(host)
        || !state.objects.get(host.object).attachments.contains(&source.object)
        || !matches!(state.engine.event_history.get(loan.input as usize).and_then(journal),
            Some(JournalV50::AuraReturnInput { input }) if input.choice.aura.object == source.object
                && input.choice.candidates.iter().any(|candidate| candidate.object == host.object
                    && candidate.expected_zone_change_count == host.zone_change_count))
        || state.engine.event_history.iter().any(|event|
            matches!(journal(event), Some(JournalV50::AuraAttachmentCommitted { input, .. }) if *input == loan.input))
    { return Err("Fake Aura attachment changed its private actual entry loan".into()); }
    emit(
        state,
        JournalV50::AuraAttachmentCommitted {
            input: loan.input,
            source,
            host,
        },
    );
    Ok(())
}

pub(crate) fn no_host_outcome(state: &GameState, resolution: u64) -> bool {
    state.engine.event_history.iter().enumerate().any(|(at, event)| {
        matches!(journal(event), Some(JournalV50::AuraReturnInput { input })
            if input.choice.resolution == resolution && input.choice.candidates.is_empty())
            && state.engine.event_history.iter().any(|event|
                matches!(journal(event), Some(JournalV50::AuraReturnCompleted { input, selected: None,
                    returned: None, attachment: None }) if *input == at as u64))
    })
}

pub(crate) fn validate_history(state: &GameState) -> Result<(), String> {
    for (at, event) in state.engine.event_history.iter().enumerate() {
        match journal(event) {
            Some(JournalV50::AuraReturnInput { input }) => {
                let Some(JournalV50::DeathResolutionInput {
                    publication,
                    original_stack,
                    context,
                    original_source: Some(source),
                    in_original_graveyard: true,
                    program,
                }) = state
                    .engine
                    .event_history
                    .get(input.choice.resolution as usize)
                    .and_then(journal)
                else {
                    return Err("Fake Aura input orphaned from actual death resolution".into());
                };
                let n = input.original.engine.event_history.len();
                if state
                    .engine
                    .event_history
                    .iter()
                    .filter(|event| {
                        matches!(journal(event), Some(JournalV50::AuraReturnInput { input: other })
                        if other.choice.resolution == input.choice.resolution)
                    })
                    .count()
                    != 1
                    || input.choice.resolution >= at as u64
                    || input.choice.resolution < n as u64
                    || n > state.engine.event_history.len()
                    || state.engine.event_history[..n] != input.original.engine.event_history[..]
                    || !matches!(input.operation, OperationV1::Step(_) | OperationV1::Advance)
                    || input.original.stack != *original_stack
                    || context.ability_source_contract != Some(input.choice.source)
                    || input.choice.source.zone != Zone::Graveyard
                    || input.choice.owner != input.choice.source.owner
                    || input.choice.aura
                        != (EffectObjectBinding {
                            object: input.choice.source.source,
                            expected_zone: Zone::Graveyard,
                            expected_zone_change_count: input.choice.source.zone_change_count,
                        })
                    || !matches!(program.as_ref(), crate::effect::EffectOp::Sequence(steps) if steps.len() == 2)
                    || crate::card_def::CARD_DEFS[usize::from(source.card_def)]
                        .attachment
                        .is_none()
                    || input.original.objects.try_get(input.choice.aura.object)
                        != Some(source.as_ref())
                    || hosts(&input.original, input.choice.aura)? != input.choice.candidates
                    || !matches!(state.engine.event_history.get(*publication as usize).and_then(journal),
                        Some(JournalV50::DeathPublished { item, .. }) if original_stack.last() == Some(item.as_ref()))
                {
                    return Err("Fake Aura input changed original operation, source or complete host roster".into());
                }
                let closed = state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .filter_map(|(i, event)| match journal(event) {
                        Some(JournalV50::AuraReturnCompleted {
                            input: original,
                            selected,
                            returned,
                            attachment,
                        }) if *original == at as u64 => {
                            Some((i, *selected, *returned, *attachment))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if closed.is_empty() {
                    if crate::linked_aura_operation_v1::locally_owned(state) {
                        continue;
                    }
                    let checkpoint = pending(state).ok_or("Fake Aura mandatory owner erased")?;
                    if checkpoint.input != at as u64
                        || *checkpoint.original != *input.original
                        || checkpoint.operation != input.operation
                        || checkpoint.answers != input.prior_answers
                        || checkpoint.choice
                            != (GeneralChoiceV36::FakeAuraV50 {
                                choice: Box::new(input.choice.clone()),
                                input: at as u64,
                            })
                        || input.choice.candidates.len() < 2
                    {
                        return Err("Fake Aura unresolved input lost its exact checkpoint".into());
                    }
                } else {
                    let [(closed_at, selected, returned, attachment)] = closed.as_slice() else {
                        return Err("Fake Aura returned more than once".into());
                    };
                    let relation_count = state.engine.event_history.iter().filter(|event|
                        matches!(journal(event), Some(JournalV50::AuraAttachmentCommitted { input: original, .. })
                            if *original == at as u64)).count();
                    if relation_count != usize::from(selected.is_some()) {
                        return Err("Fake Aura lost its unique actual attachment occurrence".into());
                    }
                    match (selected, returned, attachment) {
                        (None, None, None) if input.choice.candidates.is_empty() => {}
                        (Some(host), Some(movement), Some(relation))
                            if input.choice.candidates.contains(host) =>
                        {
                            if *movement <= at as u64
                                || *relation <= *movement
                                || *relation >= *closed_at as u64
                                || !matches!(state.engine.event_history.get(*movement as usize),
                                    Some(CommittedEvent::ZoneChange { object, from: Zone::Graveyard, to: Zone::Battlefield, .. })
                                        if *object == input.choice.aura.object)
                                || !matches!(state.engine.event_history.get(*relation as usize).and_then(journal),
                                    Some(JournalV50::AuraAttachmentCommitted { input: original, source, host: actual })
                                        if *original == at as u64 && source.object == input.choice.aura.object
                                            && Some(source.zone_change_count) == input.choice.aura.expected_zone_change_count.checked_add(1)
                                            && actual.object == host.object && actual.zone_change_count == host.expected_zone_change_count)
                            {
                                return Err(
                                    "Fake Aura closure lost ordered move and actual attachment"
                                        .into(),
                                );
                            }
                        }
                        _ => {
                            return Err(
                                "Fake Aura closure changed no-host or selected-host outcome".into(),
                            )
                        }
                    }
                }
            }
            Some(
                JournalV50::AuraReturnCompleted { input, .. }
                | JournalV50::AuraAttachmentCommitted { input, .. },
            ) => {
                if *input >= at as u64
                    || !matches!(
                        state
                            .engine
                            .event_history
                            .get(*input as usize)
                            .and_then(journal),
                        Some(JournalV50::AuraReturnInput { .. })
                    )
                {
                    return Err("Fake Aura completion or relation orphaned".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Every closed Aura-capable death resolution owns this mandatory chain, even
/// when an adversary erases all of its new records rather than leaving orphans.
pub(crate) fn validate_resolution_closed(
    state: &GameState,
    resolution: u64,
    treasure: u64,
    returned: Option<u64>,
) -> Result<(), String> {
    let inputs = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, event)| match journal(event) {
            Some(JournalV50::AuraReturnInput { input })
                if input.choice.resolution == resolution =>
            {
                Some(at as u64)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [input] = inputs.as_slice() else {
        return Err("Fake closed Aura resolution lost its unique mandatory return input".into());
    };
    let completions = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, event)| match journal(event) {
            Some(JournalV50::AuraReturnCompleted {
                input: original,
                returned,
                ..
            }) if original == input => Some((at as u64, *returned)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(completion, aura_returned)] = completions.as_slice() else {
        return Err("Fake closed Aura resolution lost its unique mandatory completion".into());
    };
    if *input <= resolution
        || *completion <= *input
        || *completion >= treasure
        || *aura_returned != returned
    {
        return Err(
            "Fake Aura input/completion must precede Treasure and bind the actual return result"
                .into(),
        );
    }
    Ok(())
}
