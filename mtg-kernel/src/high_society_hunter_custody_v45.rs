//! Independent Hunter custody, not a renamed Gourmand receipt.
use crate::effect::{EffectContinuation, EffectObjectBinding, EffectOp};
use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::state::{
    AbilitySourceContractV4, GameObject, GameState, StackItem, StackItemKind, Zone,
};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalV45 {
    Published {
        capture: u64,
        item: Box<StackItem>,
    },
    RemovalInput {
        publication: u64,
        item: Box<StackItem>,
        countered: bool,
    },
    Menu {
        removal: u64,
        pending: Box<EffectContinuation>,
    },
    SelectionInput {
        menu: u64,
        before: Box<EffectContinuation>,
        after: Box<EffectContinuation>,
    },
    CostInput {
        selection: u64,
        pending: Box<EffectContinuation>,
        source_before: Box<GameObject>,
        selected_before: Option<Box<GameObject>>,
    },
    CostOperation {
        input: u64,
        sacrifice: Option<u64>,
        departure: Option<u64>,
        counter: Option<u64>,
    },
    DrawInput {
        removal: u64,
        original_library: Vec<EffectObjectBinding>,
    },
    DrawOperation {
        input: u64,
        draw: u64,
    },
    Closed {
        removal: u64,
        operation: Option<u64>,
    },
}

pub(crate) fn bound(capture: u64, attack: bool) -> EffectOp {
    EffectOp::HighSocietyHunterFromCaptureV45 {
        capture_history_index: capture,
        attack,
    }
}
pub(crate) fn binding(op: &EffectOp) -> Option<(u64, bool)> {
    if let EffectOp::HighSocietyHunterFromCaptureV45 {
        capture_history_index,
        attack,
    } = op
    {
        Some((*capture_history_index, *attack))
    } else {
        None
    }
}
pub(crate) fn recipe(attack: bool) -> EffectOp {
    if attack {
        crate::high_society_hunter_v1::attack_program()
    } else {
        crate::high_society_hunter_v1::death_program()
    }
}
pub(crate) fn source_at(
    state: &GameState,
    capture: u64,
) -> Result<(AbilitySourceContractV4, bool), String> {
    let at = usize::try_from(capture).map_err(|_| "Hunter capture exceeds usize")?;
    let (source, attack) = match state.engine.event_history.get(at) {
        Some(CommittedEvent::HighSocietyHunterAttackCaptureV1 { source, .. }) => {
            crate::high_society_hunter_v1::validate_source(state, *source)?;
            (*source, true)
        }
        Some(CommittedEvent::ControlledCreatureDeathCaptureV1 {
            source,
            ability_index,
            died,
            controller_before,
        }) => {
            if !crate::high_society_hunter_v1::owns_definition(source.card_def)
                || *ability_index != 1
                || source.source == died.object
                || state
                    .objects
                    .try_get(died.object)
                    .is_none_or(|o| o.v4.is_token)
                || !matches!(state.engine.event_history[..at].iter().rev().find(|e|!matches!(e,CommittedEvent::ControlledCreatureDeathCaptureV1{..})),
                    Some(CommittedEvent::ZoneChange{object,from:Zone::Battlefield,to:Zone::Graveyard,controller_before:actual})
                    if *object==died.object && actual==controller_before)
            {
                return Err("Hunter original other-nontoken death occurrence changed".into());
            }
            crate::engine::validate_historical_ability_source_contract(state, *source)?;
            (*source, false)
        }
        _ => return Err("Hunter original capture absent".into()),
    };
    if source.zone != Zone::Battlefield {
        return Err("Hunter original source was not a battlefield permanent".into());
    }
    Ok((source, attack))
}
pub(crate) fn normalize_root(
    state: &GameState,
    op: &EffectOp,
    source: Option<AbilitySourceContractV4>,
) -> Result<Option<EffectOp>, String> {
    let Some((capture, attack)) = binding(op) else {
        return Ok(None);
    };
    if source_at(state, capture)? != (source.ok_or("Hunter bound root lost source")?, attack) {
        return Err("Hunter root changed original capture/controller".into());
    }
    Ok(Some(recipe(attack)))
}
fn journal(state: &GameState, index: u64) -> Result<&JournalV45, String> {
    let at = usize::try_from(index).map_err(|_| "Hunter journal exceeds usize")?;
    match state.engine.event_history.get(at) {
        Some(CommittedEvent::HighSocietyHunterJournalV45 { journal }) => Ok(journal),
        _ => Err("Hunter committed operation absent".into()),
    }
}
fn append(state: &mut GameState, j: JournalV45) -> Result<u64, String> {
    let at = u64::try_from(state.engine.event_history.len())
        .map_err(|_| "Hunter history exceeds u64")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::HighSocietyHunterJournalV45 {
            journal: Box::new(j),
        });
    Ok(at)
}
pub(crate) fn captures(state: &GameState) -> Vec<u64> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            CommittedEvent::HighSocietyHunterAttackCaptureV1 { .. } => Some(at as u64),
            CommittedEvent::ControlledCreatureDeathCaptureV1 { source, .. }
                if crate::high_society_hunter_v1::owns_definition(source.card_def) =>
            {
                Some(at as u64)
            }
            _ => None,
        })
        .collect()
}
fn collected(state: &GameState, capture: u64) -> Vec<usize> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            CommittedEvent::HighSocietyHunterAttackCollectedV1 {
                capture_history_index,
            }
            | CommittedEvent::ControlledCreatureDeathCaptureCollectedV1 {
                capture_history_index,
            } if *capture_history_index == capture => Some(at),
            _ => None,
        })
        .collect()
}
pub(crate) fn selected(item: &StackItem) -> bool {
    item.kind == StackItemKind::TriggeredAbility
        && item.v4.granted_by.is_none()
        && item.inline_effect.as_ref().and_then(binding).is_some()
}
fn unstarted(item: &StackItem) -> StackItem {
    let mut original = item.clone();
    original.v4.high_society_hunter_started_v45.0 = None;
    original
}
fn publication<'a>(state: &'a GameState, item: &StackItem) -> Result<(u64, &'a StackItem), String> {
    let matches = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            CommittedEvent::HighSocietyHunterJournalV45 { journal } => match journal.as_ref() {
                JournalV45::Published { item: original, .. }
                    if original.v4.stack_item_id == item.v4.stack_item_id =>
                {
                    Some((at as u64, original.as_ref()))
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(at, original)] = matches.as_slice() else {
        return Err("Hunter lost its unique full original publication".into());
    };
    if unstarted(item) != **original {
        return Err("Hunter live row changed original full publication".into());
    }
    Ok((*at, *original))
}
fn canonical_item(
    item: &StackItem,
    source: AbilitySourceContractV4,
    capture: u64,
    attack: bool,
) -> StackItem {
    StackItem {
        kind: StackItemKind::TriggeredAbility,
        source: source.source,
        controller: source.controller,
        targets: Vec::new(),
        is_copy: false,
        inline_effect: Some(bound(capture, attack)),
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: crate::state::StackStateV4 {
            stack_item_id: item.v4.stack_item_id,
            ability_source_contract: Some(source),
            ..Default::default()
        },
    }
}
fn canonical_context(item: &StackItem) -> crate::effect::ExecCtx {
    let mut ctx = crate::effect::ExecCtx::no_targets(item.source, item.controller);
    ctx.stack_item_id = Some(item.v4.stack_item_id);
    ctx.ability_source_contract = item.v4.ability_source_contract;
    ctx
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    if !selected(item) {
        return Ok(());
    }
    let (capture, attack) = binding(item.inline_effect.as_ref().unwrap()).unwrap();
    let (source, kind) = source_at(state, capture)?;
    if *item!=canonical_item(item,source,capture,attack) || attack!=kind || item.source!=source.source || item.controller!=source.controller
        || item.v4.ability_source_contract!=Some(source) || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty() || item.v4.target_spec.is_some()
        || !item.v4.high_society_hunter_started_v45.is_empty() || collected(state,capture).len()!=1
        || state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::HighSocietyHunterJournalV45{journal} if matches!(journal.as_ref(),JournalV45::Published{capture:old,..} if *old==capture)))
    {return Err("Hunter publication lost original collected untargeted root".into())}
    append(
        state,
        JournalV45::Published {
            capture,
            item: Box::new(item.clone()),
        },
    )?;
    Ok(())
}

/// Constructed only while the actual unique finalized row remains live.
pub(crate) struct OwnerV45 {
    pub(crate) original: StackItem,
    expected: StackItem,
    publication: u64,
    initial: bool,
    countered: bool,
}
impl OwnerV45 {
    pub(crate) fn completion_matches(&self, item: &StackItem) -> bool {
        *item == self.expected
    }
}
pub(crate) fn acquire(
    state: &GameState,
    item: &StackItem,
    initial: bool,
    countered: bool,
) -> Result<Option<OwnerV45>, String> {
    if !selected(item) {
        return Ok(None);
    }
    validate(state)?;
    let (publication, _) = publication(state, item)?;
    if state
        .stack
        .iter()
        .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
        .count()
        != 1
        || !state.stack.iter().any(|i| i == item)
    {
        return Err("Hunter owner did not acquire actual full live row".into());
    }
    if (initial || countered) && !item.v4.high_society_hunter_started_v45.is_empty()
        || (!initial
            && !countered
            && state
                .engine
                .pending_effect
                .as_ref()
                .is_none_or(|p| p.resolving_item != *item))
    {
        return Err("Hunter owner cannot restart or counter a resolving root".into());
    }
    let mut expected = item.clone();
    if !countered {
        expected.v4.high_society_hunter_started_v45.0 = Some(publication);
    }
    Ok(Some(OwnerV45 {
        original: item.clone(),
        expected,
        publication,
        initial,
        countered,
    }))
}

pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    validate_inner(state, None)
}
/// Only the existing owning linked operation can publish this intermediate
/// cost stage. Its full deterministic replay proves the same original item,
/// accepted answer and actual departure, and discards speculative completion.
fn linked_cost_checkpoint(
    state: &GameState,
    at: u64,
    pending: &EffectContinuation,
) -> Result<bool, String> {
    if !crate::linked_aura_operation_v1::has_pending(state) {
        return Ok(false);
    }
    let owned = &state.engine.linked_aura_operation_v1;
    let original = if let Some(checkpoint) = owned.pending.as_ref() {
        checkpoint.original.as_ref()
    } else if let Some(checkpoint) = owned.pending_general_v36.as_ref() {
        checkpoint.original.as_ref()
    } else {
        return Ok(false);
    };
    if original.engine.event_history.len() as u64 > at
        || original.engine.pending_effect.as_ref() != Some(pending)
        || state.engine.pending_effect.as_ref() != Some(pending)
        || original
            .stack
            .iter()
            .filter(|item| **item == pending.resolving_item)
            .count()
            != 1
        || state
            .stack
            .iter()
            .filter(|item| **item == pending.resolving_item)
            .count()
            != 1
        || pending.choice.is_some()
        || pending
            .resolving_item
            .v4
            .high_society_hunter_started_v45
            .is_empty()
    {
        return Err(
            "Hunter intermediate cost lost original linked operation/answer/full row".into(),
        );
    }
    crate::linked_aura_operation_v1::validate(state)?;
    Ok(true)
}
/// A public linked-return checkpoint can precede the final cost operation.
/// Its owning replay authenticates this interval before any facts are exported.
pub(crate) fn linked_paid_incarnation(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<Option<crate::effect::EffectObjectBinding>, String> {
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::HighSocietyHunterJournalV45 { journal } = event else {
            continue;
        };
        let JournalV45::CostInput {
            pending: original, ..
        } = journal.as_ref()
        else {
            continue;
        };
        if original != pending || !linked_cost_checkpoint(state, at as u64, original)? {
            continue;
        }
        let [crate::effect::EffectFrame::ApplySelectedObjectsV1 { selected, .. }] =
            original.frames.as_slice()
        else {
            return Err("Hunter intermediate accepted consequence absent".into());
        };
        let Some(chosen) = selected.first() else {
            return Ok(None);
        };
        let interval = &state.engine.event_history[at + 1..];
        let sacrificed = interval.iter().position(|event| {
            matches!(event,
            CommittedEvent::Sacrificed { object, controller_before, .. }
            if *object == chosen.object && *controller_before == pending.ctx.controller)
        });
        let moved = interval.iter().position(|event| {
            matches!(event,
            CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to, .. }
            if *object == chosen.object && *to != Zone::Battlefield)
        });
        if matches!((sacrificed, moved), (Some(s), Some(m)) if s < m) {
            return Ok(Some(*chosen));
        }
    }
    Ok(None)
}
fn validate_inner(state: &GameState, owner: Option<&OwnerV45>) -> Result<(), String> {
    if state
        .engine
        .event_log
        .iter()
        .any(|e| matches!(e, CommittedEvent::HighSocietyHunterJournalV45 { .. }))
    {
        return Err("Private Hunter custody entered mutable collection log".into());
    }
    let waiting = state
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
        .collect::<Vec<&PendingTrigger>>();
    for capture in captures(state) {
        let (source, attack) = source_at(state, capture)?;
        let collections = collected(state, capture);
        let fresh = state
            .engine
            .event_log
            .iter()
            .filter(|e| **e == state.engine.event_history[capture as usize])
            .count();
        let held = waiting
            .iter()
            .filter(|p| p.effect == bound(capture, attack))
            .collect::<Vec<_>>();
        let pubs = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(at, e)| match e {
                CommittedEvent::HighSocietyHunterJournalV45 { journal } => match journal.as_ref() {
                    JournalV45::Published {
                        capture: original,
                        item,
                    } if *original == capture => Some((at as u64, item.as_ref())),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        if collections.is_empty() {
            if fresh != 1 || !held.is_empty() || !pubs.is_empty() {
                return Err("Hunter fresh mandatory capture lost its collection owner".into());
            }
            continue;
        }
        if collections.len() != 1 || collections[0] <= capture as usize || fresh != 0 {
            return Err("Hunter original collection moved, duplicated or replayed".into());
        }
        for p in &held {
            if p.source != source.source
                || p.controller != source.controller
                || p.source_contract != Some(source)
                || p.granted_by.is_some()
                || p.kicked
                || p.is_madness_offer
                || p.optional_additional_cost_paid.is_some()
                || !p.paid_cost_refs.is_empty()
                || !p.targets.is_empty()
                || !p.target_contracts.is_empty()
                || p.target_spec != crate::card_def::TargetSpec::None
            {
                return Err("Hunter waiting owner changed original full source".into());
            }
        }
        let [(pub_at, original)] = pubs.as_slice() else {
            if pubs.is_empty() && held.len() == 1 {
                continue;
            }
            return Err("Hunter captured root lost mandatory pending/publication owner".into());
        };
        if !held.is_empty()
            || *pub_at <= collections[0] as u64
            || original.v4.ability_source_contract != Some(source)
            || original.inline_effect.as_ref() != Some(&bound(capture, attack))
            || !selected(original)
            || **original != canonical_item(original, source, capture, attack)
        {
            return Err("Hunter publication changed original occurrence".into());
        }
        crate::engine::validated_stack_item_target_spec(original, state)?;
        let live = state
            .stack
            .iter()
            .filter(|i| i.v4.stack_item_id == original.v4.stack_item_id)
            .collect::<Vec<_>>();
        let removed = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(at, e)| match e {
                CommittedEvent::HighSocietyHunterJournalV45 { journal } => match journal.as_ref() {
                    JournalV45::RemovalInput {
                        publication,
                        item,
                        countered,
                    } if publication == pub_at => Some((at as u64, item.as_ref(), *countered)),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        let closed = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(at, e)| match e {
                CommittedEvent::HighSocietyHunterJournalV45 { journal } => match journal.as_ref() {
                    JournalV45::Closed { removal, operation }
                        if removed.iter().any(|(i, _, _)| i == removal) =>
                    {
                        Some((at as u64, *removal, *operation))
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        if removed.len() > 1 || closed.len() > 1 {
            return Err("Hunter genuine removal/completion was duplicated".into());
        }
        if let Some((input, item, countered)) = removed.first() {
            if *input <= *pub_at || unstarted(item) != **original {
                return Err("Hunter actual removal input changed original publication".into());
            }
            let phases = state.engine.event_history.iter().filter(|event| matches!(event,
                CommittedEvent::HighSocietyHunterJournalV45{journal} if matches!(journal.as_ref(),
                    JournalV45::Menu{removal,..}|JournalV45::DrawInput{removal,..} if removal==input))).count();
            if phases != usize::from(!*countered) {
                return Err(
                    "Hunter initial resolution lost or duplicated its genuine phase".into(),
                );
            }
            if let Some((closed_at, reference, operation)) = closed.first() {
                if reference != input
                    || *closed_at <= *input
                    || !live.is_empty()
                    || state
                        .engine
                        .pending_effect
                        .as_ref()
                        .is_some_and(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
                {
                    return Err(
                        "Hunter closed root was reintroduced or lost genuine removal".into(),
                    );
                }
                if *countered {
                    if operation.is_some() || *closed_at != input + 1 {
                        return Err(
                            "Hunter counter closure lacks immediate actual counter input".into(),
                        );
                    }
                } else {
                    let operation =
                        operation.ok_or("Hunter completion erased independent actual effect")?;
                    if operation + 1 != *closed_at || operation <= *input {
                        return Err("Hunter completion changed actual operation interval".into());
                    }
                    validate_operation(state, operation, attack, *input)?;
                }
                continue;
            }
            if *countered {
                return Err("Hunter actual counter removal lacks its closure".into());
            }
            let expected = state
                .engine
                .event_history
                .iter()
                .rev()
                .find_map(|e| match e {
                    CommittedEvent::HighSocietyHunterJournalV45 { journal } => match journal
                        .as_ref()
                    {
                        JournalV45::SelectionInput { after, .. }
                            if after.resolving_item.v4.stack_item_id == item.v4.stack_item_id =>
                        {
                            Some(after.as_ref())
                        }
                        JournalV45::Menu { removal, pending } if removal == input => {
                            Some(pending.as_ref())
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .ok_or("Hunter started row erased its mandatory menu/answer stage")?;
            if state.engine.pending_effect.as_ref() != Some(expected) {
                return Err("Hunter pending continuation changed or disappeared".into());
            }
        }
        let authentic = match (live.as_slice(), owner.filter(|o| o.publication == *pub_at)) {
            ([live], None) => unstarted(live) == **original,
            ([], Some(loan)) => unstarted(&loan.original) == **original,
            _ => false,
        };
        if !authentic {
            return Err("Hunter publication lost its actual full live owner".into());
        }
        if let [live] = live.as_slice() {
            if live.v4.high_society_hunter_started_v45.0 != removed.first().map(|_| *pub_at) {
                return Err(
                    "Hunter live started marker erased genuine removal or changed publication"
                        .into(),
                );
            }
        }
    }
    for item in &state.stack {
        if selected(item) || !item.v4.high_society_hunter_started_v45.is_empty() {
            publication(state, item)?;
        }
    }
    for pending in waiting {
        if binding(&pending.effect).is_some_and(|(capture, _)| !captures(state).contains(&capture))
        {
            return Err("Hunter waiting root has no original occurrence".into());
        }
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::HighSocietyHunterJournalV45 { journal: j } = event else {
            continue;
        };
        let at = at as u64;
        let previous = match j.as_ref() {
            JournalV45::Published { capture, .. } => {
                if !captures(state).contains(capture) {
                    return Err("Hunter publication names foreign capture".into());
                }
                None
            }
            JournalV45::RemovalInput { publication, .. } => Some((*publication, 0)),
            JournalV45::Menu { removal, pending } => {
                if !matches!(journal(state,*removal)?,JournalV45::RemovalInput{item,countered:false,..}if item.as_ref()==&pending.resolving_item)
                    || binding(
                        pending
                            .resolving_item
                            .inline_effect
                            .as_ref()
                            .ok_or("Hunter menu lost root")?,
                    )
                    .is_none_or(|(_, attack)| !attack)
                    || !pending.frames.is_empty()
                    || pending.choice.is_none()
                    || pending.answered_choice_guard.is_some()
                    || pending.ctx != canonical_context(&pending.resolving_item)
                {
                    return Err("Hunter original menu changed removal/root/stage".into());
                }
                Some((*removal, 1))
            }
            JournalV45::SelectionInput {
                menu,
                before,
                after,
            } => {
                if !matches!(journal(state,*menu)?,JournalV45::Menu{pending,..}if pending==before) {
                    return Err("Hunter answer changed independent original menu".into());
                }
                validate_selection(before, after)?;
                Some((*menu, 2))
            }
            JournalV45::CostInput {
                selection, pending, ..
            } => {
                if !matches!(journal(state,*selection)?,JournalV45::SelectionInput{after,..}if after==pending)
                {
                    return Err("Hunter original cost input changed accepted answer".into());
                }
                Some((*selection, 3))
            }
            JournalV45::CostOperation { input, .. } | JournalV45::DrawOperation { input, .. } => {
                let uses=state.engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::HighSocietyHunterJournalV45{journal}if matches!(journal.as_ref(),JournalV45::Closed{operation:Some(reference),..}if *reference==at))).count();
                if uses != 1 {
                    return Err("Hunter actual operation was erased, replayed or unused".into());
                }
                Some((*input, 4))
            }
            JournalV45::DrawInput {
                removal,
                original_library,
            } => {
                if !matches!(journal(state,*removal)?,JournalV45::RemovalInput{item,countered:false,..}if item.inline_effect.as_ref().and_then(binding).is_some_and(|(_,attack)|!attack))
                {
                    return Err("Hunter draw input changed actual death resolution".into());
                }
                if original_library.iter().enumerate().any(|(i, b)| {
                    b.expected_zone != Zone::Library
                        || original_library[..i]
                            .iter()
                            .any(|old| old.object == b.object)
                }) {
                    return Err("Hunter original draw input duplicated hidden incarnation".into());
                }
                Some((*removal, 1))
            }
            JournalV45::Closed { removal, .. } => Some((*removal, 1)),
        };
        if let Some((reference, kind)) = previous {
            if reference >= at {
                return Err("Hunter committed custody input moved forward or replayed".into());
            }
            let original = journal(state, reference)?;
            let valid = match kind {
                0 => matches!(original, JournalV45::Published { .. }),
                1 => matches!(original, JournalV45::RemovalInput { .. }),
                2 => matches!(original, JournalV45::Menu { .. }),
                3 => matches!(original, JournalV45::SelectionInput { .. }),
                4 => matches!(
                    original,
                    JournalV45::CostInput { .. } | JournalV45::DrawInput { .. }
                ),
                _ => false,
            };
            if !valid {
                return Err("Hunter committed custody refers to foreign operation kind".into());
            }
        }
        let uses = state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                let CommittedEvent::HighSocietyHunterJournalV45 { journal: next } = event else {
                    return false;
                };
                match (j.as_ref(), next.as_ref()) {
                    (JournalV45::Menu { .. }, JournalV45::SelectionInput { menu, .. }) => {
                        *menu == at
                    }
                    (
                        JournalV45::SelectionInput { .. },
                        JournalV45::CostInput { selection, .. },
                    ) => *selection == at,
                    (JournalV45::CostInput { .. }, JournalV45::CostOperation { input, .. })
                    | (JournalV45::DrawInput { .. }, JournalV45::DrawOperation { input, .. }) => {
                        *input == at
                    }
                    _ => false,
                }
            })
            .count();
        let owned_in_progress = if uses == 0 {
            if let JournalV45::CostInput { pending, .. } = j.as_ref() {
                linked_cost_checkpoint(state, at, pending)?
            } else {
                false
            }
        } else {
            false
        };
        if uses > 1
            || matches!(
                j.as_ref(),
                JournalV45::CostInput { .. } | JournalV45::DrawInput { .. }
            ) && uses != 1
                && !owned_in_progress
        {
            return Err("Hunter committed phase duplicated or lost its exact consumer".into());
        }
    }
    Ok(())
}

fn validate_operation(
    state: &GameState,
    operation: u64,
    attack: bool,
    removal: u64,
) -> Result<(), String> {
    let JournalV45::RemovalInput {
        item,
        countered: false,
        ..
    } = journal(state, removal)?
    else {
        return Err("Hunter effect lost actual resolving input".into());
    };
    let (capture, kind) = binding(
        item.inline_effect
            .as_ref()
            .ok_or("Hunter operation erased root")?,
    )
    .ok_or("Hunter operation changed root")?;
    let (source, original_kind) = source_at(state, capture)?;
    if kind != attack || original_kind != attack {
        return Err("Hunter operation switched attack and death meanings".into());
    }
    if attack {
        let JournalV45::CostOperation {
            input,
            sacrifice,
            departure,
            counter,
        } = journal(state, operation)?
        else {
            return Err("Hunter attack has no actual cost operation".into());
        };
        let JournalV45::CostInput {
            selection,
            pending,
            source_before,
            selected_before,
        } = journal(state, *input)?
        else {
            return Err("Hunter cost operation erased original input".into());
        };
        let JournalV45::SelectionInput { menu, after, .. } = journal(state, *selection)? else {
            return Err("Hunter cost input has no accepted selection".into());
        };
        let JournalV45::Menu {
            removal: actual, ..
        } = journal(state, *menu)?
        else {
            return Err("Hunter selection lost actual menu".into());
        };
        if actual != &removal
            || pending != after
            || *input >= operation
            || *selection >= *input
            || *menu >= *selection
            || *menu <= removal
            || capture >= removal
            || pending.resolving_item != **item
            || source_before.owner != source.owner
        {
            return Err("Hunter cost operation changed accepted input/item ordering".into());
        }
        let [crate::effect::EffectFrame::ApplySelectedObjectsV1 {
            rule,
            selected,
            path,
            ..
        }] = pending.frames.as_slice()
        else {
            return Err("Hunter cost input erased exact chosen consequence".into());
        };
        if !path.is_empty()
            || recipe(true) != (EffectOp::SelectObjectsV1 { rule: *rule })
            || selected.len() > 1
        {
            return Err("Hunter selected consequence changed printed root/cardinality".into());
        }
        let source_moves = state.engine.event_history[capture as usize + 1..*input as usize]
            .iter()
            .filter(|e| matches!(e,CommittedEvent::ZoneChange{object,..}|CommittedEvent::Draw{object:Some(object),..}if *object==source.source))
            .count();
        if source
            .zone_change_count
            .checked_add(u32::try_from(source_moves).map_err(|_| "Hunter source moves exceed u32")?)
            != Some(source_before.zone_change_count)
        {
            return Err("Hunter source-absence input erased actual departure/reentry".into());
        }
        if source_moves == 0 && source_before.zone != Zone::Battlefield {
            return Err(
                "Hunter live original source was rewritten as absent without a departure".into(),
            );
        }
        let last_source_zone = state.engine.event_history[capture as usize + 1..*input as usize]
            .iter()
            .rev()
            .find_map(|event| match event {
                CommittedEvent::ZoneChange { object, to, .. } if *object == source.source => {
                    Some(*to)
                }
                CommittedEvent::Draw {
                    object: Some(object),
                    ..
                } if *object == source.source => Some(Zone::Hand),
                _ => None,
            })
            .unwrap_or(Zone::Battlefield);
        if source_before.zone != last_source_zone {
            return Err("Hunter source input changed its actual last committed zone".into());
        }
        let mut actual_sacrifices = Vec::new();
        let mut actual_departures = Vec::new();
        let mut actual_counters = Vec::new();
        for (relative, event) in state.engine.event_history[*input as usize + 1..operation as usize]
            .iter()
            .enumerate()
        {
            let at = *input + 1 + relative as u64;
            match event {
                CommittedEvent::Sacrificed { object, .. }
                    if selected.first().is_some_and(|b| b.object == *object) =>
                {
                    actual_sacrifices.push(at)
                }
                CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Battlefield,
                    to,
                    ..
                } if *to != Zone::Battlefield
                    && selected.first().is_some_and(|b| b.object == *object) =>
                {
                    actual_departures.push(at)
                }
                CommittedEvent::PlusOneCountersAdded { object, .. } if *object == source.source => {
                    actual_counters.push(at)
                }
                _ => {}
            }
        }
        if actual_sacrifices.as_slice() != sacrifice.as_slice()
            || actual_departures.as_slice() != departure.as_slice()
            || actual_counters.as_slice() != counter.as_slice()
        {
            return Err(
                "Hunter operation omitted, duplicated or manufactured its actual effects".into(),
            );
        }
        match (selected.as_slice(), selected_before.as_deref()) {
            ([], None) if sacrifice.is_none() && departure.is_none() && counter.is_none() => {}
            ([chosen], Some(before)) => {
                if before.zone != Zone::Battlefield
                    || before.controller != source.controller
                    || before.zone_change_count != chosen.expected_zone_change_count
                    || chosen.expected_zone != Zone::Battlefield
                    || (chosen.object == source.source
                        && chosen.expected_zone_change_count == source.zone_change_count)
                {
                    return Err(
                        "Hunter cost input changed another creature incarnation/controller".into(),
                    );
                }
                let sacrificed = sacrifice.ok_or("Hunter actual sacrifice attempt absent")?;
                if sacrificed <= *input
                    || sacrificed >= operation
                    || !matches!(state.engine.event_history.get(sacrificed as usize),
                    Some(CommittedEvent::Sacrificed{object,controller_before,..})if *object==chosen.object && *controller_before==source.controller)
                {
                    return Err("Hunter cost operation lost actual original sacrifice".into());
                }
                if let Some(moved) = departure {
                    if *moved <= *input
                        || *moved >= operation
                        || !matches!(state.engine.event_history.get(*moved as usize),
                        Some(CommittedEvent::ZoneChange{object,from:Zone::Battlefield,to,..})if *object==chosen.object && *to!=Zone::Battlefield)
                    {
                        return Err("Hunter payment lost actual battlefield departure".into());
                    }
                }
                let receives = departure.is_some()
                    && source_before.zone == Zone::Battlefield
                    && source_before.zone_change_count == source.zone_change_count;
                if receives {
                    let counted = counter.ok_or("Hunter paid live source erased actual counter")?;
                    if counted <= *input
                        || counted >= operation
                        || !matches!(state.engine.event_history.get(counted as usize),
                        Some(CommittedEvent::PlusOneCountersAdded{object,zone_change_count,player,count})if *object==source.source && *zone_change_count==source.zone_change_count && *player==source.controller && *count==1)
                    {
                        return Err(
                            "Hunter actual counter changed source/controller/incarnation/amount"
                                .into(),
                        );
                    }
                } else if counter.is_some() {
                    return Err("Hunter unpaid or departed source manufactured counter".into());
                }
            }
            _ => return Err("Hunter cost input changed decline/selection".into()),
        }
    } else {
        let JournalV45::DrawOperation { input, draw } = journal(state, operation)? else {
            return Err("Hunter death erased actual draw operation".into());
        };
        let JournalV45::DrawInput {
            removal: actual,
            original_library,
        } = journal(state, *input)?
        else {
            return Err("Hunter draw erased original library input".into());
        };
        if actual != &removal
            || *input <= removal
            || capture >= removal
            || *draw <= *input
            || *draw >= operation
            || !matches!(state.engine.event_history.get(*draw as usize),Some(CommittedEvent::Draw{player,object})if *player==source.controller && *object==original_library.first().map(|b|b.object))
        {
            return Err("Hunter death draw changed original beneficiary/top/empty attempt".into());
        }
        let draws=state.engine.event_history[*input as usize+1..operation as usize].iter().filter(|event|matches!(event,CommittedEvent::Draw{player,..}if *player==source.controller)).count();
        if draws != 1 {
            return Err("Hunter draw input was consumed more than once".into());
        }
    }
    Ok(())
}

pub(crate) fn prepare_answer(state: &GameState) -> Result<Option<Box<EffectContinuation>>, String> {
    let Some(p) = state.engine.pending_effect.as_ref() else {
        return Ok(None);
    };
    if !selected(&p.resolving_item) {
        return Ok(None);
    }
    validate(state)?;
    Ok(Some(Box::new(p.clone())))
}
pub(crate) fn record_answer(
    state: &mut GameState,
    before: Option<Box<EffectContinuation>>,
) -> Result<(), String> {
    let Some(before) = before else { return Ok(()) };
    let after = state
        .engine
        .pending_effect
        .as_ref()
        .ok_or("Hunter accepted selection erased continuation")?
        .clone();
    let menu = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .rev()
        .find_map(|(at, e)| match e {
            CommittedEvent::HighSocietyHunterJournalV45 { journal } => match journal.as_ref() {
                JournalV45::Menu { pending, .. } if pending.as_ref() == before.as_ref() => {
                    Some(at as u64)
                }
                _ => None,
            },
            _ => None,
        })
        .ok_or("Hunter selection lost exact independent original menu")?;
    validate_selection(&before, &after)?;
    append(
        state,
        JournalV45::SelectionInput {
            menu,
            before,
            after: Box::new(after),
        },
    )?;
    Ok(())
}
fn validate_selection(
    before: &EffectContinuation,
    after: &EffectContinuation,
) -> Result<(), String> {
    use crate::effect::{EffectFrame, EffectTargetSelectionPurpose, PendingEffectChoice};
    let Some(PendingEffectChoice::SelectTargets {
        purpose:
            EffectTargetSelectionPurpose::SelectObjectsV1 {
                rule,
                original_candidates,
                canonical_path,
            },
        selected,
        legal,
        min_targets,
        max_targets,
        ordered,
        ..
    }) = &before.choice
    else {
        return Err("Hunter answer erased genuine optional sacrifice menu".into());
    };
    let [EffectFrame::ApplySelectedObjectsV1 {
        rule: after_rule,
        original_candidates: after_candidates,
        selected: chosen,
        path,
    }] = after.frames.as_slice()
    else {
        return Err("Hunter answer erased accepted consequence".into());
    };
    if !before.frames.is_empty()
        || !selected.is_empty()
        || *min_targets != 0
        || *max_targets != u16::from(!original_candidates.is_empty())
        || *ordered
        || recipe(true) != (EffectOp::SelectObjectsV1 { rule: *rule })
        || legal.len() != original_candidates.len()
        || legal
            .iter()
            .zip(original_candidates)
            .any(|(candidate, binding)| {
                candidate.expected_object != Some(*binding)
                    || candidate.target != crate::state::Target::Object(binding.object)
            })
        || !canonical_path.is_empty()
        || rule != after_rule
        || original_candidates != after_candidates
        || !path.is_empty()
        || chosen.len() > 1
        || chosen
            .iter()
            .any(|b| !legal.iter().any(|c| c.expected_object == Some(*b)))
        || after.choice.is_some()
        || after.answered_choice_guard.is_some()
    {
        return Err("Hunter accepted answer changed original option/cardinality".into());
    }
    let mut expected = before.clone();
    expected.choice = None;
    expected.frames = after.frames.clone();
    expected.answered_choice_guard = None;
    if expected != *after {
        return Err("Hunter selection changed full original item/context".into());
    }
    Ok(())
}
fn record_removed(state: &mut GameState, owner: &OwnerV45) -> Result<u64, String> {
    if state
        .stack
        .iter()
        .any(|i| i.v4.stack_item_id == owner.original.v4.stack_item_id)
    {
        return Err("Hunter removal input must follow actual removal".into());
    }
    append(
        state,
        JournalV45::RemovalInput {
            publication: owner.publication,
            item: Box::new(owner.expected.clone()),
            countered: owner.countered,
        },
    )
}
pub(crate) fn counter_removed(
    state: &mut GameState,
    owner: Option<&OwnerV45>,
    item: &StackItem,
) -> Result<(), String> {
    let Some(owner) = owner else { return Ok(()) };
    if !owner.countered || owner.original != *item {
        return Err("Hunter actual counter changed original removal loan".into());
    }
    let removal = record_removed(state, owner)?;
    append(
        state,
        JournalV45::Closed {
            removal,
            operation: None,
        },
    )?;
    Ok(())
}
pub(crate) fn begin_owned(
    state: &mut GameState,
    ctx: &crate::effect::ExecCtx,
    owner: &OwnerV45,
) -> Result<crate::effect::ResumableProgress, String> {
    if *ctx != canonical_context(&owner.expected)
        || !owner.initial
        || owner.countered
        || state.engine.pending_effect.is_some()
        || ctx.stack_item_id != Some(owner.expected.v4.stack_item_id)
        || ctx.source != owner.expected.source
        || ctx.controller != owner.expected.controller
        || ctx.ability_source_contract != owner.expected.v4.ability_source_contract
        || !ctx.targets.is_empty()
        || !ctx.target_contracts.is_empty()
    {
        return Err("Hunter initial private owner changed original context".into());
    }
    let (capture, attack) = binding(
        owner
            .expected
            .inline_effect
            .as_ref()
            .ok_or("Hunter owned root absent")?,
    )
    .ok_or("Hunter owned root changed")?;
    normalize_root(
        state,
        owner.expected.inline_effect.as_ref().unwrap(),
        ctx.ability_source_contract,
    )?;
    let removal = record_removed(state, owner)?;
    if !attack {
        let original_library = state.players[ctx.controller.index()]
            .library
            .iter()
            .map(|&object| EffectObjectBinding {
                object,
                expected_zone: Zone::Library,
                expected_zone_change_count: state.objects.get(object).zone_change_count,
            })
            .collect();
        let input = append(
            state,
            JournalV45::DrawInput {
                removal,
                original_library,
            },
        )?;
        let start = state.engine.event_history.len();
        // Borrow the acquired genuine item only during synchronous event callbacks.
        state.stack.push(owner.expected.clone());
        crate::effect::execute(&recipe(false), ctx, state);
        remove_borrowed(state, owner)?;
        let draws = state.engine.event_history[start..]
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                matches!(e,CommittedEvent::Draw{player,..}if *player==ctx.controller)
                    .then_some((start + i) as u64)
            })
            .collect::<Vec<_>>();
        let [draw] = draws.as_slice() else {
            return Err("Hunter death did not commit exactly one actual draw attempt".into());
        };
        let operation = append(state, JournalV45::DrawOperation { input, draw: *draw })?;
        validate_operation(state, operation, false, removal)?;
        append(
            state,
            JournalV45::Closed {
                removal,
                operation: Some(operation),
            },
        )?;
        return Ok(crate::effect::ResumableProgress::Complete(Box::new(
            owner.expected.clone(),
        )));
    }
    let mut pending = EffectContinuation {
        resolving_item: owner.expected.clone(),
        ctx: ctx.clone(),
        frames: Vec::new(),
        choice: None,
        answered_choice_guard: None,
    };
    let EffectOp::SelectObjectsV1 { rule } = recipe(true) else {
        unreachable!("exact printed recipe")
    };
    crate::effect::begin_hunter_selection_v45(state, &mut pending, rule)?;
    if binding(pending.resolving_item.inline_effect.as_ref().unwrap()) != Some((capture, true)) {
        return Err("Hunter menu changed bound capture".into());
    }
    append(
        state,
        JournalV45::Menu {
            removal,
            pending: Box::new(pending.clone()),
        },
    )?;
    state.engine.pending_effect = Some(pending);
    Ok(crate::effect::ResumableProgress::Suspended)
}
fn remove_borrowed(state: &mut GameState, owner: &OwnerV45) -> Result<(), String> {
    let rows = state
        .stack
        .iter()
        .enumerate()
        .filter(|(_, i)| i.v4.stack_item_id == owner.expected.v4.stack_item_id)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    let [index] = rows.as_slice() else {
        return Err("Hunter synchronous loan lost actual unique item".into());
    };
    if state.stack[*index] != owner.expected {
        return Err("Hunter synchronous callback changed borrowed row".into());
    }
    state.stack.remove(*index);
    Ok(())
}
pub(crate) fn authorize_cost(owner: &OwnerV45, pending: &EffectContinuation) -> Result<(), String> {
    if owner.initial || owner.countered || pending.resolving_item != owner.expected {
        return Err("Hunter cost has no genuine resumed owner".into());
    }
    Ok(())
}
pub(crate) fn resume_owned(
    state: &mut GameState,
    owner: &OwnerV45,
) -> Result<crate::effect::ResumableProgress, String> {
    if owner.initial || owner.countered {
        return Err("Hunter initial/counter loan cannot execute accepted cost".into());
    }
    validate_inner(state, Some(owner))?;
    let pending = state
        .engine
        .pending_effect
        .as_ref()
        .ok_or("Hunter resumed cost lost accepted selection")?
        .clone();
    authorize_cost(owner, &pending)?;
    let [crate::effect::EffectFrame::ApplySelectedObjectsV1 { selected, .. }] =
        pending.frames.as_slice()
    else {
        return Err("Hunter resume requires exact accepted sacrifice consequence".into());
    };
    let selection = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .rev()
        .find_map(|(at, e)| match e {
            CommittedEvent::HighSocietyHunterJournalV45 { journal } => match journal.as_ref() {
                JournalV45::SelectionInput { after, .. } if after.as_ref() == &pending => {
                    Some(at as u64)
                }
                _ => None,
            },
            _ => None,
        })
        .ok_or("Hunter resumed cost erased independent actual selection input")?;
    let removal = match journal(state, selection)? {
        JournalV45::SelectionInput { menu, .. } => match journal(state, *menu)? {
            JournalV45::Menu { removal, .. } => *removal,
            _ => return Err("Hunter answer erased actual initial menu".into()),
        },
        _ => return Err("Hunter answer input changed kind".into()),
    };
    crate::high_society_hunter_v1::preflight_payment(state, &pending, selected, owner)?;
    let source_before = Box::new(state.objects.get(pending.ctx.source).clone());
    let selected_before = selected
        .first()
        .map(|b| Box::new(state.objects.get(b.object).clone()));
    let input = append(
        state,
        JournalV45::CostInput {
            selection,
            pending: Box::new(pending.clone()),
            source_before,
            selected_before,
        },
    )?;
    let start = state.engine.event_history.len();
    state.stack.push(owner.expected.clone());
    let result = crate::high_society_hunter_v1::pay_selection(state, &pending, selected, owner);
    remove_borrowed(state, owner)?;
    result?;
    let selected_object = selected.first().map(|b| b.object);
    let mut sacrifice = None;
    let mut departure = None;
    let mut counter = None;
    for (relative, event) in state.engine.event_history[start..].iter().enumerate() {
        let at = (start + relative) as u64;
        match event {
            CommittedEvent::Sacrificed { object, .. } if Some(*object) == selected_object => {
                if sacrifice.replace(at).is_some() {
                    return Err("Hunter sacrifice occurred twice".into());
                }
            }
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                to,
                ..
            } if Some(*object) == selected_object && *to != Zone::Battlefield => {
                if departure.replace(at).is_some() {
                    return Err("Hunter cost departed twice".into());
                }
            }
            CommittedEvent::PlusOneCountersAdded { object, .. }
                if *object == pending.ctx.source =>
            {
                if counter.replace(at).is_some() {
                    return Err("Hunter source received two counter operations".into());
                }
            }
            _ => {}
        }
    }
    let operation = append(
        state,
        JournalV45::CostOperation {
            input,
            sacrifice,
            departure,
            counter,
        },
    )?;
    validate_operation(state, operation, true, removal)?;
    state.engine.pending_effect = None;
    append(
        state,
        JournalV45::Closed {
            removal,
            operation: Some(operation),
        },
    )?;
    Ok(crate::effect::ResumableProgress::Complete(Box::new(
        owner.expected.clone(),
    )))
}

#[cfg(test)]
mod tests;
