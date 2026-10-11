//! Authenticated terminal Raid look: keep zero or one, graveyard the rest.
use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PrivateKeepOneBindingV1 {
    pub player: PlayerId,
    pub source: ObjectId,
    pub stack_item_id: Option<StackItemId>,
    pub turn: u32,
    pub active_player: PlayerId,
    pub original_library: Vec<EffectObjectBinding>,
}
pub(crate) fn recipe() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::ControllerAttackedThisTurn,
        then: Box::new(EffectOp::LookTopMayKeepOneRestGraveyard { count: 3 }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}
pub(super) fn is_recipe(root: &EffectOp) -> bool {
    *root == recipe()
}
pub(super) fn validate_recipe(
    state: &GameState,
    pending: &EffectContinuation,
    player: PlayerId,
    count: u8,
    path: &[u16],
    remainder: &[EffectFrame],
) -> Result<(), String> {
    if !is_recipe(&validated_definition_owned_root_effect(state, pending)?)
        || pending.resolving_item.kind != crate::state::StackItemKind::TriggeredAbility
        || player != pending.ctx.controller
        || count != 3
        || path != [0]
        || !remainder.is_empty()
        || !state.player_attacked_this_turn_v1(player)
    {
        return Err("keep-one look lost its definition-owned terminal Raid recipe".into());
    }
    Ok(())
}
fn captured(state: &GameState, stack: StackItemId) -> Result<&PrivateKeepOneBindingV1, String> {
    let mut found = None;
    for event in &state.engine.event_history {
        match event {
            event::CommittedEvent::PrivateKeepOneLibraryCapturedV1 { binding }
                if binding.stack_item_id == Some(stack) =>
            {
                if found.replace(binding.as_ref()).is_some() {
                    return Err("keep-one look has duplicate committed capture authority".into());
                }
            }
            event::CommittedEvent::PrivateKeepOneLibraryConsumedV1 {
                resolving_stack_item,
                ..
            } if *resolving_stack_item == stack => {
                return Err("keep-one look already consumed its capture authority".into());
            }
            _ => {}
        }
    }
    found.ok_or_else(|| "keep-one look lacks its committed capture authority".into())
}
fn validate_live(state: &GameState, binding: &PrivateKeepOneBindingV1) -> Result<(), String> {
    if binding.turn != state.turn
        || binding.active_player != state.active_player
        || binding.original_library != bind_library_exact(state, binding.player)
    {
        return Err("keep-one look changed its turn or exact library".into());
    }
    for &card in &binding.original_library {
        validate_effect_object_binding(state, card)?;
        if card.expected_zone != Zone::Library
            || state.objects.get(card.object).owner != binding.player
        {
            return Err("keep-one look changed its private library owner or incarnation".into());
        }
    }
    if let Some(stack) = binding.stack_item_id {
        if captured(state, stack)? != binding {
            return Err("keep-one look differs from its immutable committed capture".into());
        }
    }
    Ok(())
}
/// Physical capture primitive. Production authenticates the printed root first.
/// A context without a stack ID never authenticates a production continuation.
pub(super) fn capture(
    state: &mut GameState,
    ctx: &ExecCtx,
) -> Result<PrivateKeepOneBindingV1, String> {
    if let Some(stack) = ctx.stack_item_id {
        if state.engine.event_history.iter().any(|event| {
            matches!(event,
            event::CommittedEvent::PrivateKeepOneLibraryCapturedV1 { binding }
                if binding.stack_item_id == Some(stack))
        }) {
            return Err("keep-one look already captured this resolution".into());
        }
    }
    let binding = PrivateKeepOneBindingV1 {
        player: ctx.controller,
        source: ctx.source,
        stack_item_id: ctx.stack_item_id,
        turn: state.turn,
        active_player: state.active_player,
        original_library: bind_library_exact(state, ctx.controller),
    };
    state.reveal_library_top(
        binding.player,
        binding.player,
        binding.original_library.len().min(3),
    );
    if binding.stack_item_id.is_some() {
        state
            .engine
            .event_history
            .push(event::CommittedEvent::PrivateKeepOneLibraryCapturedV1 {
                binding: Box::new(binding.clone()),
            });
    }
    validate_live(state, &binding)?;
    Ok(binding)
}
pub(super) fn validate_snapshot(
    state: &GameState,
    pending: &EffectContinuation,
    player: PlayerId,
    library: &[EffectObjectBinding],
) -> Result<(), String> {
    let stack = pending
        .ctx
        .stack_item_id
        .ok_or("keep-one continuation lacks a resolving stack incarnation")?;
    let binding = captured(state, stack)?;
    if binding.player != player
        || binding.source != pending.ctx.source
        || binding.original_library != library
    {
        return Err(
            "keep-one continuation substituted its captured controller, source or batch".into(),
        );
    }
    validate_live(state, binding)
}
/// An ordered graveyard subset leaves exactly zero or one looked-at card on top.
pub(super) fn commit(
    state: &mut GameState,
    binding: &PrivateKeepOneBindingV1,
    graveyard: &[EffectObjectBinding],
) -> Result<(), String> {
    validate_live(state, binding)?;
    let top = &binding.original_library[..binding.original_library.len().min(3)];
    if graveyard.len() < top.len().saturating_sub(1)
        || graveyard.len() > top.len()
        || graveyard
            .iter()
            .enumerate()
            .any(|(index, card)| !top.contains(card) || graveyard[..index].contains(card))
    {
        return Err(
            "keep-one answer must leave exactly zero or one distinct looked-at card on top".into(),
        );
    }
    let start = state.engine.event_history.len();
    commit_zone_change_batch(state, graveyard, Zone::Graveyard, true)?;
    let move_history_indices: Vec<_> = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .skip(start)
        .filter_map(|(index, event)| {
            matches!(event, event::CommittedEvent::ZoneChange {
            object, from: Zone::Library, to: Zone::Graveyard, ..
        } if graveyard.iter().any(|binding| binding.object == *object))
            .then_some(index)
        })
        .collect();
    if move_history_indices.len() != graveyard.len() {
        return Err("keep-one graveyard operation did not commit every selected move".into());
    }
    if let Some(resolving_stack_item) = binding.stack_item_id {
        state
            .engine
            .event_history
            .push(event::CommittedEvent::PrivateKeepOneLibraryConsumedV1 {
                resolving_stack_item,
                graveyard: graveyard.to_vec(),
                move_history_indices,
            });
    }
    Ok(())
}
pub(super) fn commit_pending(
    state: &mut GameState,
    pending: &EffectContinuation,
    graveyard: &[EffectObjectBinding],
) -> Result<(), String> {
    let stack = pending
        .ctx
        .stack_item_id
        .ok_or("keep-one answer lacks its resolving stack incarnation")?;
    let binding = captured(state, stack)?.clone();
    commit(state, &binding, graveyard)
}
pub(super) fn validate_required_stage(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    let definition = pending
        .ctx
        .ability_source_contract
        .map(|source| source.card_def);
    if !definition
        .and_then(|id| crate::card_def::CARD_DEFS.get(usize::from(id)))
        .is_some_and(|def| def.name == "Gutless Plunderer")
    {
        return Ok(());
    }
    if !is_recipe(&validated_definition_owned_root_effect(state, pending)?) {
        return Err("Gutless Plunderer lost its definition-owned Raid root".into());
    }
    let stack = pending
        .ctx
        .stack_item_id
        .ok_or("Gutless Plunderer lost its resolving stack incarnation")?;
    let has_capture = state.engine.event_history.iter().any(|event| matches!(event,
        event::CommittedEvent::PrivateKeepOneLibraryCapturedV1 { binding } if binding.stack_item_id == Some(stack)));
    let initial = !has_capture
        && pending.choice.is_none()
        && pending.answered_choice_guard.is_none()
        && pending.frames
            == vec![EffectFrame::Program {
                op: recipe(),
                path: vec![],
            }];
    let choosing = matches!(
        &pending.choice,
        Some(PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::SurveilLibraryMany {
                graveyard_order: None,
                ..
            },
            ..
        })
    ) && pending.answered_choice_guard.is_none();
    let answered = pending.choice.is_none()
        && matches!(
            &pending.answered_choice_guard,
            Some(EffectAnsweredChoiceGuard::SurveilLibraryMany { .. })
        );
    if !initial && !(has_capture && (choosing || answered)) {
        return Err(
            "Gutless Plunderer lost its mandatory private choice or authenticated answer".into(),
        );
    }
    Ok(())
}
/// A terminal progress snapshot cannot substitute for the actual private operation.
pub(super) fn validate_closed(
    state: &GameState,
    stack: StackItemId,
    source: crate::state::AbilitySourceContractV4,
    first_index: u32,
    closed_index: u32,
) -> Result<(), String> {
    let mut captures = Vec::new();
    let mut consumed = Vec::new();
    let mut skipped = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            event::CommittedEvent::PrivateKeepOneLibraryCapturedV1 { binding }
                if binding.stack_item_id == Some(stack) =>
            {
                captures.push((index, binding.as_ref()))
            }
            event::CommittedEvent::PrivateKeepOneLibraryConsumedV1 {
                resolving_stack_item,
                graveyard,
                move_history_indices,
            } if *resolving_stack_item == stack => {
                consumed.push((index, graveyard, move_history_indices))
            }
            event::CommittedEvent::PrivateKeepOneRaidSkippedV1 {
                resolving_stack_item,
                initial_boundary_history_index,
            } if *resolving_stack_item == stack => {
                skipped.push((index, *initial_boundary_history_index))
            }
            _ => {}
        }
    }
    let first = first_index as usize;
    let closed = closed_index as usize;
    if let [(index, initial)] = skipped.as_slice() {
        if *initial == first_index
            && first < *index
            && *index < closed
            && captures.is_empty()
            && consumed.is_empty()
        {
            return Ok(());
        }
    }
    let ([(capture_index, binding)], [(consume_index, graveyard, moves)]) =
        (captures.as_slice(), consumed.as_slice())
    else {
        return Err("keep-one closure lacks a unique actual capture and consumption".into());
    };
    let top = &binding.original_library[..binding.original_library.len().min(3)];
    if !skipped.is_empty()
        || binding.source != source.source
        || binding.player != source.controller
        || first >= *capture_index
        || capture_index >= consume_index
        || *consume_index >= closed
        || graveyard.len() < top.len().saturating_sub(1)
        || graveyard.len() > top.len()
        || graveyard.len() != moves.len()
    {
        return Err("keep-one closed operation changed its source, batch or lifecycle".into());
    }
    for (position, (card, index)) in graveyard.iter().zip(moves.iter()).enumerate() {
        if !top.contains(card) || graveyard[..position].contains(card)
            || card.expected_zone != Zone::Library || *index <= *capture_index || *index >= *consume_index
            || (position > 0 && moves[position - 1] >= *index)
            || !matches!(state.engine.event_history.get(*index), Some(event::CommittedEvent::ZoneChange {
                object, from: Zone::Library, to: Zone::Graveyard, ..
            }) if *object == card.object)
            || state.engine.event_history[*capture_index + 1..*index].iter().any(|event|
                matches!(event, event::CommittedEvent::ZoneChange { object, .. } if *object == card.object))
        { return Err("keep-one closure lacks each exact committed selected graveyard move".into()); }
    }
    Ok(())
}

pub(crate) fn selection_shape(state: &GameState) -> Result<Option<(u16, u16)>, String> {
    validate_pending_effect_choice(state)?;
    let Some(pending) = state.engine.pending_effect.as_ref() else {
        return Ok(None);
    };
    // Ward and other generated continuations have their own authenticated roots.
    // Only a printed Gutless source belongs to this optional instruction.
    let Some(source) = pending.ctx.ability_source_contract else {
        return Ok(None);
    };
    if !crate::card_def::CARD_DEFS
        .get(source.card_def as usize)
        .is_some_and(|definition| definition.name == "Gutless Plunderer")
    {
        return Ok(None);
    }
    if !is_recipe(&validated_definition_owned_root_effect(state, pending)?) {
        return Err("Gutless private instruction lost its definition-owned root".into());
    }
    match &pending.choice {
        Some(PendingEffectChoice::SelectTargets {
            min_targets,
            max_targets,
            purpose:
                EffectTargetSelectionPurpose::SurveilLibraryMany {
                    graveyard_order: None,
                    ..
                },
            ..
        }) => Ok(Some((*min_targets, *max_targets))),
        _ => Ok(None),
    }
}
#[cfg(test)]
mod tests;
