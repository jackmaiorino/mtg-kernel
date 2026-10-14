//! Original gained-ability collection and publication for the Partial candidate.
//! These records are not permission to execute the private root.

use super::spell_custody::{emit, journal, JournalV50};
use crate::effect::{EffectOp, ExecCtx};
use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::state::{
    AbilitySourceContractV4, GameObject, GameState, StackItem, StackItemKind, StackStateV4, Zone,
};
use crate::trigger::PendingTrigger;

fn binding(item: &StackItem) -> Option<u64> {
    item.inline_effect.as_ref().and_then(effect_capture)
}

fn effect_capture(effect: &EffectOp) -> Option<u64> {
    match effect {
        EffectOp::FakeDeathFromCaptureV50 { capture } => Some(*capture),
        _ => None,
    }
}

pub(crate) fn has_unowned_roots(state: &GameState) -> bool {
    state.stack.iter().any(|item| binding(item).is_some())
        || held_triggers(state).any(|pending| effect_capture(&pending.effect).is_some())
        || state
            .engine
            .event_log
            .iter()
            .any(|event| journal(event).is_some())
}

fn held_triggers(state: &GameState) -> impl Iterator<Item = &PendingTrigger> {
    state
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
                .flat_map(|pending| pending.deferred_triggers.iter()),
        )
}

fn source_at(state: &GameState, capture: u64) -> Result<AbilitySourceContractV4, String> {
    let index = usize::try_from(capture).map_err(|_| "Fake trigger capture index overflow")?;
    match state.engine.event_history.get(index).and_then(journal) {
        Some(JournalV50::DeathCaptured { lookback, .. }) => Ok(lookback.battlefield_source),
        _ => Err("Fake trigger lost its original committed death capture".into()),
    }
}

fn canonical_item(item: &StackItem, source: AbilitySourceContractV4, capture: u64) -> StackItem {
    StackItem {
        kind: StackItemKind::TriggeredAbility,
        source: source.source,
        controller: source.controller,
        targets: Vec::new(),
        is_copy: false,
        inline_effect: Some(EffectOp::FakeDeathFromCaptureV50 { capture }),
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4 {
            stack_item_id: item.v4.stack_item_id,
            ability_source_contract: Some(source),
            ..Default::default()
        },
    }
}

fn collection_at(state: &GameState, capture: u64) -> Result<u64, String> {
    let mut found = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            matches!(journal(event), Some(JournalV50::DeathCollected { capture: original, .. })
            if *original == capture)
            .then_some(index as u64)
        });
    let index = found
        .next()
        .ok_or("Fake trigger lost its original collection")?;
    if found.next().is_some() || index <= capture {
        return Err("Fake trigger collection was repeated or moved before capture".into());
    }
    Ok(index)
}

/// The pending candidate and full published row have the same exact untargeted
/// gained root. Its creature definition need not print this gained ability.
pub(crate) fn validate_trigger_identity(
    state: &GameState,
    item: &StackItem,
) -> Result<bool, String> {
    let Some(capture) = binding(item) else {
        return Ok(false);
    };
    super::spell_custody::validate_grant_lifetime(state)?;
    let source = source_at(state, capture)?;
    collection_at(state, capture)?;
    if *item != canonical_item(item, source, capture) {
        return Err(
            "Fake gained trigger changed its original full source or untargeted root".into(),
        );
    }
    Ok(true)
}

/// Called by the ordinary APNAP publisher after actual unique row insertion.
/// The original full row is retained before any later counter or removal.
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(capture) = binding(item) else {
        return Ok(());
    };
    validate_trigger_identity(state, item)?;
    let collection = collection_at(state, capture)?;
    if item.v4.stack_item_id == Default::default()
        || state.stack.last() != Some(item)
        || state.stack.iter().filter(|row| row.v4.stack_item_id == item.v4.stack_item_id).count() != 1
        || state.engine.event_history.iter().any(|event|
            matches!(journal(event), Some(JournalV50::DeathPublished { capture: original, item: row, .. })
                if *original == capture || row.v4.stack_item_id == item.v4.stack_item_id))
    {
        return Err("Fake gained trigger lost its unique actual publication row".into());
    }
    emit(
        state,
        JournalV50::DeathPublished {
            capture,
            collection,
            item: Box::new(item.clone()),
        },
    );
    Ok(())
}

/// A live full row is borrowed before the actual counter removes it. Neither
/// this loan nor its in-flight completion can be deserialized from a game.
pub(crate) struct CounterOwnerV50 {
    publication: u64,
    original_stack: Vec<StackItem>,
    position: usize,
}
pub(crate) struct CounterRemovalV50 {
    owner: CounterOwnerV50,
    input: u64,
}

pub(crate) fn acquire_counter(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<CounterOwnerV50>, String> {
    let Some(capture) = binding(item) else {
        return Ok(None);
    };
    super::spell_custody::validate_retained_grant_history(state)?;
    validate_trigger_identity(state, item)?;
    let publications = state.engine.event_history.iter().enumerate().filter_map(|(index, event)| {
        matches!(journal(event), Some(JournalV50::DeathPublished { capture: original, item: row, .. })
            if *original == capture && row.as_ref() == item).then_some(index as u64)
    }).collect::<Vec<_>>();
    let [publication] = publications.as_slice() else {
        return Err("Fake counter lost its unique original full publication".into());
    };
    let positions = state
        .stack
        .iter()
        .enumerate()
        .filter_map(|(index, row)| (row.v4.stack_item_id == item.v4.stack_item_id).then_some(index))
        .collect::<Vec<_>>();
    let [position] = positions.as_slice() else {
        return Err("Fake counter lost its unique actual full live row".into());
    };
    if state.stack[*position] != *item {
        return Err("Fake counter changed its original full row".into());
    }
    Ok(Some(CounterOwnerV50 {
        publication: *publication,
        original_stack: state.stack.clone(),
        position: *position,
    }))
}

pub(crate) fn begin_counter(
    state: &mut GameState,
    owner: Option<CounterOwnerV50>,
) -> Result<Option<CounterRemovalV50>, String> {
    let Some(owner) = owner else { return Ok(None) };
    super::spell_custody::validate_retained_grant_history(state)?;
    let publication = usize::try_from(owner.publication)
        .map_err(|_| "Fake counter loan publication index overflow")?;
    if !matches!(state.engine.event_history.get(publication).and_then(journal),
        Some(JournalV50::DeathPublished { item, .. })
        if item.as_ref() == &owner.original_stack[owner.position])
        || !counter_inputs(state, owner.publication).is_empty()
    {
        return Err("Fake counter loan changed or closed its original full publication".into());
    }
    if state.stack != owner.original_stack {
        return Err("Fake counter input changed the complete borrowed original stack".into());
    }
    let input = emit(
        state,
        JournalV50::DeathCounterInput {
            publication: owner.publication,
            original_stack: owner.original_stack.clone(),
        },
    );
    Ok(Some(CounterRemovalV50 { owner, input }))
}

pub(crate) fn finish_counter(
    state: &mut GameState,
    removal: Option<CounterRemovalV50>,
) -> Result<(), String> {
    let Some(removal) = removal else {
        return Ok(());
    };
    let mut expected = removal.owner.original_stack.clone();
    expected.remove(removal.owner.position);
    if state.stack != expected
        || state.engine.event_history.len() as u64 != removal.input + 1
        || !matches!(state.engine.event_history.last().and_then(journal),
            Some(JournalV50::DeathCounterInput { publication, original_stack })
            if *publication == removal.owner.publication && *original_stack == removal.owner.original_stack)
    {
        return Err(
            "Fake actual counter changed its original input or exact remaining stack".into(),
        );
    }
    emit(
        state,
        JournalV50::DeathCounterClosed {
            input: removal.input,
            remaining_stack: expected,
        },
    );
    Ok(())
}

fn counter_inputs(state: &GameState, publication: u64) -> Vec<u64> {
    state.engine.event_history.iter().enumerate().filter_map(|(index, event)|
        matches!(journal(event), Some(JournalV50::DeathCounterInput { publication: original, .. })
            if *original == publication).then_some(index as u64)).collect()
}

/// The public gained row keeps its battlefield source. This private operand
/// carries the independently captured graveyard incarnation used for return.
pub(crate) struct DeathResolutionOwnerV50 {
    loan: CounterOwnerV50,
    context: ExecCtx,
    original_source: Option<Box<GameObject>>,
    in_original_graveyard: bool,
    program: EffectOp,
}

fn death_context(state: &GameState, item: &StackItem) -> Result<(ExecCtx, bool), String> {
    let capture = binding(item).ok_or("Fake resolution lost its captured root")?;
    let Some(JournalV50::DeathCaptured {
        lookback,
        graveyard_source: Some(source),
        ..
    }) = usize::try_from(capture)
        .ok()
        .and_then(|index| state.engine.event_history.get(index))
        .and_then(journal)
    else {
        return Err("Fake resolution lost its original graveyard input".into());
    };
    let mut context = ExecCtx::no_targets(item.source, item.controller);
    context.stack_item_id = Some(item.v4.stack_item_id);
    context.ability_source_contract = Some(*source);
    Ok((context, lookback.was_token_at_death))
}

pub(crate) fn acquire_resolution(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<DeathResolutionOwnerV50>, String> {
    let Some(loan) = acquire_counter(state, item)? else {
        return Ok(None);
    };
    if loan.position.checked_add(1) != Some(state.stack.len()) {
        return Err("Fake resolution did not borrow the actual stack top".into());
    }
    let (context, token) = death_context(state, item)?;
    let source = context.ability_source_contract.unwrap();
    let original_source = state.objects.try_get(source.source).cloned().map(Box::new);
    let in_original_graveyard = state.players[source.owner.index()]
        .graveyard
        .contains(&source.source);
    let program = super::primitive_death_program_from_input_v50(
        &context,
        token,
        original_source.as_deref(),
        in_original_graveyard,
    )?;
    Ok(Some(DeathResolutionOwnerV50 {
        loan,
        context,
        original_source,
        in_original_graveyard,
        program,
    }))
}

pub(crate) fn resolve_owned(
    state: &mut GameState,
    owner: DeathResolutionOwnerV50,
    item: &StackItem,
    battlefield_context: &ExecCtx,
) -> Result<(), String> {
    // Reauthenticate retained inputs after the actual pop. The stable ingress
    // validator requires the live row, so use metadata and the private loan's
    // exact remaining-stack check at this synchronous callback.
    validate_trigger_identity(state, item)?;
    let (retained_context, retained_token) = death_context(state, item)?;
    if retained_context != owner.context
        || super::primitive_death_program_from_input_v50(
            &retained_context,
            retained_token,
            owner.original_source.as_deref(),
            owner.in_original_graveyard,
        )? != owner.program
    {
        return Err(
            "Fake resolution loan lost its original captured context or ordered program".into(),
        );
    }
    let mut original_context = owner.context.clone();
    original_context.ability_source_contract = item.v4.ability_source_contract;
    let mut remaining = owner.loan.original_stack.clone();
    let original = remaining.remove(owner.loan.position);
    let source = owner.context.ability_source_contract.unwrap();
    if original != *item
        || original_context != *battlefield_context
        || state.stack != remaining
        || state.objects.try_get(source.source) != owner.original_source.as_deref()
        || state.players[source.owner.index()]
            .graveyard
            .contains(&source.source)
            != owner.in_original_graveyard
        || !matches!(usize::try_from(owner.loan.publication).ok().and_then(|index| state.engine.event_history.get(index)).and_then(journal),
            Some(JournalV50::DeathPublished { item: row, .. }) if row.as_ref() == item)
        || !counter_inputs(state, owner.loan.publication).is_empty()
        || !resolution_inputs(state, owner.loan.publication).is_empty()
    {
        return Err(
            "Fake resolution changed its borrowed full stack, context or original source".into(),
        );
    }
    let input = emit(
        state,
        JournalV50::DeathResolutionInput {
            publication: owner.loan.publication,
            original_stack: owner.loan.original_stack,
            context: Box::new(owner.context.clone()),
            original_source: owner.original_source,
            in_original_graveyard: owner.in_original_graveyard,
            program: Box::new(owner.program.clone()),
        },
    );
    let EffectOp::Sequence(steps) = owner.program else {
        return Err("Fake resolution lost its ordered return/creation program".into());
    };
    if steps.len() == 2 {
        if crate::card_def::CARD_DEFS[usize::from(state.objects.get(item.source).card_def)]
            .attachment
            .is_some()
        {
            if !super::return_aura::resolve_return(state, input)? {
                return Ok(());
            }
        } else {
            crate::effect::execute(&steps[0], &owner.context, state);
        }
    }
    if state.engine.halted.is_some() {
        return Err("Fake owned return halted".into());
    }
    let returned = state.engine.event_history.iter().enumerate().skip(input as usize + 1)
        .find_map(|(index, event)| matches!(event, CommittedEvent::ZoneChange { object, from: Zone::Graveyard, to: Zone::Battlefield, .. }
            if *object == item.source).then_some(index as u64));
    let replacement =
        crate::doubling_season_v48::ReplacementInputV48::capture(state, item.controller);
    let expected_count = replacement.replaced_token_count(1)?;
    let treasure = emit(
        state,
        JournalV50::DeathTreasureInput {
            resolution: input,
            replacement,
        },
    );
    crate::effect::execute(steps.last().unwrap(), &owner.context, state);
    if state.engine.halted.is_some() {
        return Err("Fake owned Treasure creation halted".into());
    }
    let created = token_roster(
        state,
        treasure as usize + 1,
        state.engine.event_history.len(),
        item.controller,
    )?;
    if created.len() != expected_count {
        return Err("Fake creation lost its complete replaced Treasure roster".into());
    }
    emit(
        state,
        JournalV50::DeathResolutionClosed {
            input,
            treasure,
            returned,
            created,
        },
    );
    Ok(())
}

fn resolution_inputs(state: &GameState, publication: u64) -> Vec<u64> {
    state.engine.event_history.iter().enumerate().filter_map(|(index, event)|
        matches!(journal(event), Some(JournalV50::DeathResolutionInput { publication: original, .. })
            if *original == publication).then_some(index as u64)).collect()
}

fn token_roster(
    state: &GameState,
    start: usize,
    end: usize,
    controller: crate::ids::PlayerId,
) -> Result<Vec<(u64, ObjectId)>, String> {
    let definition = super::treasure_definition()?;
    let mut roster = Vec::new();
    for (index, event) in state
        .engine
        .event_history
        .iter()
        .enumerate()
        .take(end)
        .skip(start)
    {
        if let CommittedEvent::CreateToken {
            object,
            token_def,
            controller: actual,
            ..
        } = event
        {
            if *token_def != definition
                || *actual != controller
                || roster.iter().any(|(_, old)| old == object)
            {
                return Err(
                    "Fake Treasure operation changed its complete issued token roster".into(),
                );
            }
            roster.push((index as u64, *object));
        }
    }
    Ok(roster)
}

fn validate_pending_shape(
    state: &GameState,
    pending: &PendingTrigger,
    capture: u64,
) -> Result<(), String> {
    let source = source_at(state, capture)?;
    let expected = PendingTrigger {
        controller: source.controller,
        source: source.source,
        effect: EffectOp::FakeDeathFromCaptureV50 { capture },
        is_madness_offer: false,
        kicked: false,
        target_spec: crate::card_def::TargetSpec::None,
        targets: Vec::new(),
        target_contracts: Vec::new(),
        placement_ordered: pending.placement_ordered,
        source_contract: Some(source),
        granted_by: None,
        optional_additional_cost_paid: None,
        paid_cost_refs: Vec::new(),
    };
    if *pending != expected {
        return Err(
            "Fake held trigger changed its original source/controller or full pending row".into(),
        );
    }
    Ok(())
}

/// Check stable actor/advance boundaries, separately from synchronous collection
/// and publication callbacks. Removal owners will later supply a closed state;
/// until then every published gained trigger must retain its exact live row.
pub(crate) fn validate_retained_owners(state: &GameState) -> Result<(), String> {
    super::return_aura::validate_history(state)?;
    validate_mutable_capture_roster(state, &state.engine.event_log)?;
    for pending in held_triggers(state) {
        let Some(capture) = effect_capture(&pending.effect) else {
            continue;
        };
        collection_at(state, capture)?;
        validate_pending_shape(state, pending, capture)?;
    }
    for item in &state.stack {
        let Some(capture) = binding(item) else {
            continue;
        };
        if state.engine.event_history.iter().filter(|event|
            matches!(journal(event), Some(JournalV50::DeathPublished { capture: original, item: row, .. })
                if *original == capture && row.as_ref() == item)).count() != 1
        {
            return Err("Fake live gained trigger lost its complete original publication".into());
        }
    }
    for event in &state.engine.event_history {
        if let Some(JournalV50::DeathCollected { capture, .. }) = journal(event) {
            let pending = held_triggers(state)
                .filter(|pending| effect_capture(&pending.effect) == Some(*capture))
                .collect::<Vec<_>>();
            let published = state
                .engine
                .event_history
                .iter()
                .enumerate()
                .filter_map(|(index, event)| match journal(event) {
                    Some(JournalV50::DeathPublished {
                        capture: original,
                        item,
                        ..
                    }) if original == capture => Some((index as u64, item.as_ref())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            match (pending.as_slice(), published.as_slice()) {
                ([held], []) => validate_pending_shape(state, held, *capture)?,
                ([], [(publication, original)]) => {
                    let rows = state
                        .stack
                        .iter()
                        .filter(|row| row.v4.stack_item_id == original.v4.stack_item_id)
                        .collect::<Vec<_>>();
                    let counter = counter_inputs(state, *publication);
                    let resolution = resolution_inputs(state, *publication);
                    let departures = counter.len() + resolution.len();
                    let mut entry_borrowed_row = false;
                    if let [input] = resolution.as_slice() {
                        let completions = resolution_completions(state, *input);
                        if completions.is_empty() {
                            let Some((before, operation)) =
                                crate::linked_aura_operation_v1::pending_original_operation(state)
                            else {
                                return Err("Fake removed resolution lost its completion or enclosing entry checkpoint".into());
                            };
                            if *input < before.engine.event_history.len() as u64
                                || !matches!(
                                    operation,
                                    crate::linked_aura_operation_v1::OperationV1::Advance
                                        | crate::linked_aura_operation_v1::OperationV1::Step(_)
                                )
                            {
                                return Err("Fake entry checkpoint did not produce the original death resolution".into());
                            }
                            if !matches!(
                                usize::try_from(*input).ok()
                                    .and_then(|index| state.engine.event_history.get(index))
                                    .and_then(journal),
                                Some(JournalV50::DeathResolutionInput {
                                    publication: retained, original_stack, ..
                                }) if retained == publication
                                    && original_stack.last() == Some(*original)
                                    && before.stack == *original_stack
                            ) {
                                return Err("Fake entry checkpoint changed the original full resolution input".into());
                            }
                            // The existing entry owner restores exactly the original
                            // resolving row while its choice borrows this operation.
                            // Reconstruct the complete checkpoint before admitting it.
                            crate::linked_aura_operation_v1::validate(state)?;
                            entry_borrowed_row = true;
                        } else if completions.len() != 1 {
                            return Err("Fake death resolution completed more than once".into());
                        }
                    }
                    if ((departures == 0 || entry_borrowed_row) && rows.as_slice() != [*original])
                        || (departures == 1 && !entry_borrowed_row && !rows.is_empty())
                        || departures > 1
                    {
                        return Err(
                            "Fake published gained trigger changed or lost its unique live owner"
                                .into(),
                        );
                    }
                }
                _ => {
                    return Err(
                        "Fake collected gained trigger lost or duplicated its mandatory owner"
                            .into(),
                    )
                }
            }
        }
    }
    Ok(())
}

fn captures_before(state: &GameState, end: usize) -> Vec<u64> {
    state.engine.event_history.iter().take(end).enumerate().filter_map(|(index, event)| {
        (matches!(journal(event), Some(JournalV50::DeathCaptured { .. }))
            && !state.engine.event_history.iter().take(end).any(|event|
                matches!(journal(event), Some(JournalV50::DeathCollected {capture,..}) if *capture==index as u64)))
        .then_some(index as u64)
    }).collect()
}

/// Every original collection input consumes its complete earlier uncollected
/// capture roster, contiguously and once. No erased mutable receipt can make an
/// already collected gained ability available again.
pub(crate) fn validate_collection(state: &GameState) -> Result<(), String> {
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match journal(event) {
            Some(JournalV50::CollectionInput { captures }) => {
                if captures.is_empty() || *captures != captures_before(state, index) {
                    return Err(
                        "Fake collection changed its complete original uncollected roster".into(),
                    );
                }
                for (ordinal, capture) in captures.iter().enumerate() {
                    if !matches!(state.engine.event_history.get(index+1+ordinal).and_then(journal),
                        Some(JournalV50::DeathCollected {input,ordinal:actual,capture:c})
                        if *input==index as u64 && *actual as usize==ordinal && c==capture)
                    {
                        return Err(
                            "Fake collection lost an original ordered capture consumption".into(),
                        );
                    }
                }
                if !matches!(state.engine.event_history.get(index+1+captures.len()).and_then(journal),
                    Some(JournalV50::CollectionCompleted {input}) if *input==index as u64)
                {
                    return Err(
                        "Fake collection lost its adjacent complete-roster completion".into(),
                    );
                }
            }
            Some(JournalV50::DeathCollected {
                input,
                ordinal,
                capture,
            }) => {
                let input_index =
                    usize::try_from(*input).map_err(|_| "Fake collection input index overflow")?;
                if input_index
                    .checked_add(1)
                    .and_then(|i| i.checked_add(*ordinal as usize))
                    != Some(index)
                    || !matches!(state.engine.event_history.get(input_index).and_then(journal),
                        Some(JournalV50::CollectionInput {captures}) if captures.get(*ordinal as usize)==Some(capture))
                {
                    return Err(
                        "Fake consumed capture lost its exact original collection position".into(),
                    );
                }
            }
            Some(JournalV50::CollectionCompleted { input }) => {
                let input_index = usize::try_from(*input)
                    .map_err(|_| "Fake collection completion index overflow")?;
                let Some(JournalV50::CollectionInput { captures }) = state
                    .engine
                    .event_history
                    .get(input_index)
                    .and_then(journal)
                else {
                    return Err("Fake collection completion lost its original input".into());
                };
                if input_index
                    .checked_add(1)
                    .and_then(|i| i.checked_add(captures.len()))
                    != Some(index)
                {
                    return Err("Fake collection completion changed its original adjacency".into());
                }
            }
            Some(JournalV50::DeathPublished {
                capture,
                collection,
                item,
            }) => {
                let source = source_at(state, *capture)?;
                if *collection >= index as u64
                    || collection_at(state, *capture)? != *collection
                    || item.v4.stack_item_id == Default::default()
                    || **item != canonical_item(item, source, *capture)
                    || state.engine.event_history.iter().filter(|event|
                        matches!(journal(event), Some(JournalV50::DeathPublished { capture: original, .. })
                            if original == capture)).count() != 1
                    || state.engine.event_history.iter().filter(|event|
                        matches!(journal(event), Some(JournalV50::DeathPublished { item: row, .. })
                            if row.v4.stack_item_id == item.v4.stack_item_id)).count() != 1
                {
                    return Err("Fake publication changed its original capture, collection or full row".into());
                }
            }
            Some(JournalV50::DeathCounterInput {
                publication,
                original_stack,
            }) => {
                let publication_index = usize::try_from(*publication)
                    .map_err(|_| "Fake counter publication index overflow")?;
                let Some(JournalV50::DeathPublished { item, .. }) = state
                    .engine
                    .event_history
                    .get(publication_index)
                    .and_then(journal)
                else {
                    return Err("Fake counter input lost its original gained publication".into());
                };
                let positions = original_stack
                    .iter()
                    .enumerate()
                    .filter_map(|(position, row)| {
                        (row.v4.stack_item_id == item.v4.stack_item_id).then_some(position)
                    })
                    .collect::<Vec<_>>();
                let [position] = positions.as_slice() else {
                    return Err("Fake counter input lost its original unique live row".into());
                };
                if publication_index >= index
                    || original_stack[*position] != **item
                    || counter_inputs(state, *publication).as_slice() != [index as u64]
                {
                    return Err(
                        "Fake counter input changed or repeated its complete original publication"
                            .into(),
                    );
                }
                let mut expected = original_stack.clone();
                expected.remove(*position);
                if !matches!(state.engine.event_history.get(index+1).and_then(journal),
                    Some(JournalV50::DeathCounterClosed { input, remaining_stack })
                    if *input == index as u64 && *remaining_stack == expected)
                {
                    return Err(
                        "Fake counter lost its adjacent exact original-stack completion".into(),
                    );
                }
            }
            Some(JournalV50::DeathCounterClosed { input, .. }) => {
                if usize::try_from(*input)
                    .ok()
                    .and_then(|input| input.checked_add(1))
                    != Some(index)
                    || !matches!(
                        state.engine.event_history.get(index - 1).and_then(journal),
                        Some(JournalV50::DeathCounterInput { .. })
                    )
                {
                    return Err("Fake counter closure lost its independent original input".into());
                }
            }
            _ => {}
        }
    }
    validate_resolution_history(state)
}

fn resolution_completions(state: &GameState, input: u64) -> Vec<(usize, &JournalV50)> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match journal(event) {
            Some(
                entry @ JournalV50::DeathResolutionClosed {
                    input: original, ..
                },
            ) if *original == input => Some((index, entry)),
            _ => None,
        })
        .collect()
}

/// Metadata validation permits a synchronous in-flight input. Only the stable
/// ingress validator above can admit a completed or replay-owned state.
fn validate_resolution_history(state: &GameState) -> Result<(), String> {
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match journal(event) {
            Some(JournalV50::DeathResolutionInput {
                publication,
                original_stack,
                context,
                original_source,
                in_original_graveyard,
                program,
            }) => {
                let Some(JournalV50::DeathPublished { item, .. }) = usize::try_from(*publication)
                    .ok()
                    .and_then(|at| state.engine.event_history.get(at))
                    .and_then(journal)
                else {
                    return Err("Fake resolution input lost its original publication".into());
                };
                let (expected_context, token) = death_context(state, item)?;
                if *publication >= index as u64
                    || original_stack.last() != Some(item.as_ref())
                    || original_stack
                        .iter()
                        .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                        .count()
                        != 1
                    || context.as_ref() != &expected_context
                    || resolution_inputs(state, *publication).as_slice() != [index as u64]
                    || !counter_inputs(state, *publication).is_empty()
                    || program.as_ref()
                        != &super::primitive_death_program_from_input_v50(
                            context,
                            token,
                            original_source.as_deref(),
                            *in_original_graveyard,
                        )?
                {
                    return Err("Fake resolution input changed its complete original row, death context or program".into());
                }
                let completions = resolution_completions(state, index as u64);
                if completions.len() > 1 {
                    return Err("Fake resolution duplicated its original completion".into());
                }
                if let [(
                    closed_at,
                    JournalV50::DeathResolutionClosed {
                        treasure,
                        returned,
                        created,
                        ..
                    },
                )] = completions.as_slice()
                {
                    let treasure_at = usize::try_from(*treasure)
                        .map_err(|_| "Fake Treasure input index overflow")?;
                    let Some(JournalV50::DeathTreasureInput {
                        resolution,
                        replacement,
                    }) = state
                        .engine
                        .event_history
                        .get(treasure_at)
                        .and_then(journal)
                    else {
                        return Err("Fake completion lost its independent Treasure input".into());
                    };
                    let treasury = state.engine.event_history.iter().filter(|event|
                        matches!(journal(event), Some(JournalV50::DeathTreasureInput { resolution, .. }) if *resolution == index as u64)).count();
                    let returns = state.engine.event_history.iter().enumerate().take(treasure_at).skip(index+1).filter_map(|(at, event)|
                        matches!(event, CommittedEvent::ZoneChange { object, from: Zone::Graveyard, to: Zone::Battlefield, .. }
                            if *object == item.source).then_some(at as u64)).collect::<Vec<_>>();
                    if matches!(program.as_ref(), EffectOp::Sequence(steps) if steps.len() == 2)
                        && original_source.as_ref().is_some_and(|source| {
                            crate::card_def::CARD_DEFS
                                .get(usize::from(source.card_def))
                                .is_some_and(|definition| definition.attachment.is_some())
                        })
                    {
                        super::return_aura::validate_resolution_closed(
                            state,
                            index as u64,
                            *treasure,
                            *returned,
                        )?;
                    }
                    let expects_return = matches!(program.as_ref(), EffectOp::Sequence(steps) if steps.len() == 2)
                        && !super::return_aura::no_host_outcome(state, index as u64);
                    if *resolution != index as u64
                        || treasury != 1
                        || treasure_at <= index
                        || treasure_at >= *closed_at
                        || replacement.controller != item.controller
                        || returns.len() != usize::from(expects_return)
                        || returns.first().copied() != *returned
                        || *created
                            != token_roster(state, treasure_at + 1, *closed_at, item.controller)?
                        || created.len() != replacement.replaced_token_count(1)?
                    {
                        return Err("Fake completion changed ordered return or the complete replaced Treasure roster".into());
                    }
                    for source in &replacement.seasons {
                        if source.controller != item.controller
                            || source.zone != Zone::Battlefield
                            || !crate::doubling_season_v48::owns_definition(source.card_def)
                        {
                            return Err(
                                "Fake Treasure replacement borrowed a foreign source".into()
                            );
                        }
                        crate::engine::validate_historical_ability_source_contract(state, *source)?;
                    }
                }
            }
            Some(JournalV50::DeathTreasureInput { resolution, .. }) => {
                if *resolution >= index as u64
                    || !matches!(
                        usize::try_from(*resolution)
                            .ok()
                            .and_then(|at| state.engine.event_history.get(at))
                            .and_then(journal),
                        Some(JournalV50::DeathResolutionInput { .. })
                    )
                {
                    return Err("Fake Treasure input lost its preceding original resolution".into());
                }
            }
            Some(JournalV50::DeathResolutionClosed { input, .. }) => {
                if *input >= index as u64
                    || !matches!(
                        usize::try_from(*input)
                            .ok()
                            .and_then(|at| state.engine.event_history.get(at))
                            .and_then(journal),
                        Some(JournalV50::DeathResolutionInput { .. })
                    )
                {
                    return Err(
                        "Fake resolution closure lost its independent original input".into(),
                    );
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Derive mandatory gained triggers from independent committed history. Every
/// uncollected input and capture must appear exactly once in the current log;
/// consumed ones must be absent. Validate the complete batch before journaling
/// any collection or returning pending triggers for ordinary APNAP placement.
pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, ObjectId> {
    if !state
        .engine
        .event_history
        .iter()
        .any(|e| journal(e).is_some())
        && !events.iter().any(|e| journal(e).is_some())
    {
        return Ok(Vec::new());
    }
    super::spell_custody::validate_grant_lifetime(state).map_err(|_| ObjectId(0))?;
    let captures = validate_mutable_capture_roster(state, events).map_err(|_| ObjectId(0))?;
    let mut pending = Vec::with_capacity(captures.len());
    for capture in &captures {
        let Some(JournalV50::DeathCaptured { lookback, .. }) = state
            .engine
            .event_history
            .get(*capture as usize)
            .and_then(journal)
        else {
            return Err(ObjectId(0));
        };
        let source = lookback.battlefield_source;
        pending.push(PendingTrigger {
            controller: source.controller,
            source: source.source,
            effect: EffectOp::FakeDeathFromCaptureV50 { capture: *capture },
            is_madness_offer: false,
            kicked: false,
            target_spec: crate::card_def::TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: Some(source),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
    }
    if !captures.is_empty() {
        let input = emit(
            state,
            JournalV50::CollectionInput {
                captures: captures.clone(),
            },
        );
        for (ordinal, capture) in captures.into_iter().enumerate() {
            emit(
                state,
                JournalV50::DeathCollected {
                    input,
                    ordinal: ordinal as u32,
                    capture,
                },
            );
        }
        emit(state, JournalV50::CollectionCompleted { input });
    }
    Ok(pending)
}

fn validate_mutable_capture_roster(
    state: &GameState,
    events: &[CommittedEvent],
) -> Result<Vec<u64>, String> {
    let captures = captures_before(state, state.engine.event_history.len());
    u32::try_from(captures.len()).map_err(|_| "Fake capture roster exceeds u32")?;
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let needed = match journal(event) {
            Some(JournalV50::DeathCaptured { .. }) => captures.contains(&(index as u64)),
            Some(JournalV50::DeathInput { .. }) => captures.iter().any(|capture| {
                matches!(state.engine.event_history.get(*capture as usize).and_then(journal),
                    Some(JournalV50::DeathCaptured {input,..}) if *input==index as u64)
            }),
            _ => continue,
        };
        if events.iter().filter(|e| *e == event).count() != usize::from(needed) {
            return Err("Fake mutable log changed its complete original uncollected roster".into());
        }
    }
    for event in events.iter().filter(|e| journal(e).is_some()) {
        if !matches!(
            journal(event),
            Some(JournalV50::DeathInput { .. } | JournalV50::DeathCaptured { .. })
        ) || state
            .engine
            .event_history
            .iter()
            .filter(|e| *e == event)
            .count()
            != 1
        {
            return Err("Fake mutable log contains a forged or non-capture journal".into());
        }
    }
    Ok(captures)
}
