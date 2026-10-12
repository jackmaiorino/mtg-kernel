//! Exact public selection from a newly exiled library batch.
pub use super::strongbox_permission_v1::PermissionInstallationV1;
use super::*;
use sha2::{Digest, Sha256};

fn digest(binding: &ExileTopChoiceBindingV1) -> Result<[u8; 32], String> {
    let mut committed = binding.clone();
    committed.provenance_digest = [0; 32];
    let mut hash = Sha256::new();
    hash.update(b"kernel-newly-exiled-choice-v1\0");
    hash.update(serde_json::to_vec(&committed).map_err(|error| error.to_string())?);
    Ok(hash.finalize().into())
}

pub(super) fn recipe() -> EffectOp {
    crate::trigger::strongbox_raider_effect()
}
pub(super) fn is_recipe(root: &EffectOp) -> bool {
    *root == recipe()
}
fn validate_recipe(
    state: &GameState,
    pending: &EffectContinuation,
    count: u8,
    path: &[u16],
    remainder: &[EffectFrame],
) -> Result<(), String> {
    let root = validated_definition_owned_root_effect(state, pending)?;
    let expected = EffectOp::Conditional {
        cond: EffectCond::ControllerAttackedThisTurn,
        then: Box::new(EffectOp::ExileTopChooseOneUntilNextTurn { count: 2 }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    };
    // This first slice admits only Strongbox's terminal Raid branch. Broader
    // compositions need a separately authenticated continuation recipe.
    if *root != expected
        || count != 2
        || path != [0]
        || !remainder.is_empty()
        || !state.player_attacked_this_turn_v1(pending.ctx.controller)
    {
        return Err("exile-play selection lost its definition-owned terminal Raid recipe".into());
    }
    Ok(())
}

/// Capture actual events and post-move incarnations. Production calls this only
/// after authenticating the definition-owned root. Tests exercise the physical
/// capture/grant primitive directly with registered objects, without admitting a
/// replacement printed program or bypassing continuation validation.
pub(crate) fn capture_exiled_top(
    state: &mut GameState,
    ctx: &ExecCtx,
    count: u8,
    path: Vec<u16>,
    remainder: Vec<EffectFrame>,
) -> Result<ExileTopChoiceBindingV1, String> {
    if count != 2 {
        return Err("exile-play capture supports exactly two requested cards".into());
    }
    if let Some(stack_item) = ctx.stack_item_id {
        if state.engine.event_history.iter().any(|event| {
            matches!(
                event,
                event::CommittedEvent::ExilePlaySelectionCapturedV1 { resolving_stack_item, .. }
                | event::CommittedEvent::ExilePlaySelectionConsumedV1 { resolving_stack_item, .. }
                    if *resolving_stack_item == stack_item
            )
        }) {
            return Err("exile-play resolution already captured or consumed its batch".into());
        }
    }
    let player = ctx.controller;
    let original_library = bind_library_exact(state, player);
    let mut exiled = Vec::new();
    let mut exile_event_indices = Vec::new();
    let prefix = &original_library[..original_library.len().min(usize::from(count))];
    let history_start = state.engine.event_history.len();
    commit_zone_change_batch(state, prefix, Zone::Exile, false)?;
    for original in prefix {
        let live = state.objects.get(original.object);
        if live.zone != Zone::Exile {
            continue;
        }
        if live.zone_change_count
            != original
                .expected_zone_change_count
                .checked_add(1)
                .ok_or("exile generation overflow")?
        {
            return Err("exile capture encountered an unsupported multi-move replacement".into());
        }
        let index = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .skip(history_start)
            .find_map(|(index, event)| {
                matches!(event, event::CommittedEvent::ZoneChange {
                object, from: Zone::Library, to: Zone::Exile, controller_before,
            } if *object == original.object && *controller_before == player)
                .then_some(index)
            })
            .ok_or("new exile lacks its committed library departure")?;
        exile_event_indices
            .push(u32::try_from(index).map_err(|_| "exile history index exceeds u32")?);
        exiled.push(EffectObjectBinding {
            object: original.object,
            expected_zone: Zone::Exile,
            expected_zone_change_count: live.zone_change_count,
        });
    }
    let exiled_public = exiled
        .iter()
        .map(|binding| crate::rl::card_ref(state, binding.object).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    if exiled
        .iter()
        .zip(&exiled_public)
        .any(|(binding, public)| state.objects.get(binding.object).card_def != public.card_db_id)
    {
        return Err("Strongbox refuses unexpected hidden exile characteristics".into());
    }
    let mut binding = ExileTopChoiceBindingV1 {
        player,
        requested_count: count,
        stack_item_id: ctx.stack_item_id,
        source: ctx.source,
        granted_turn: state.turn_key_v1(),
        granted_active_player: state.active_player,
        original_library,
        resulting_library: bind_library_exact(state, player),
        exiled,
        exiled_public,
        exile_event_indices,
        canonical_path: path,
        expected_remaining_frames: remainder,
        provenance_digest: [0; 32],
    };
    binding.provenance_digest = digest(&binding)?;
    validate_live_batch(state, &binding)?;
    if let Some(resolving_stack_item) = binding.stack_item_id {
        state
            .engine
            .event_history
            .push(event::CommittedEvent::ExilePlaySelectionCapturedV1 {
                resolving_stack_item,
                binding: Box::new(binding.clone()),
            });
    }
    Ok(binding)
}

fn validate_capture_record(
    state: &GameState,
    binding: &ExileTopChoiceBindingV1,
) -> Result<(), String> {
    let Some(stack_item) = binding.stack_item_id else {
        return Ok(()); // physical primitive, never a production continuation
    };
    let mut captured = None;
    for event in &state.engine.event_history {
        match event {
            event::CommittedEvent::ExilePlaySelectionCapturedV1 {
                resolving_stack_item,
                binding: committed,
            } if *resolving_stack_item == stack_item => {
                if captured.replace(committed.as_ref()).is_some() {
                    return Err(
                        "exile-play resolution has duplicate committed capture authority".into(),
                    );
                }
            }
            event::CommittedEvent::ExilePlaySelectionConsumedV1 {
                resolving_stack_item,
                ..
            } if *resolving_stack_item == stack_item => {
                return Err("exile-play resolution already consumed its committed capture".into());
            }
            _ => {}
        }
    }
    if captured != Some(binding) {
        return Err(
            "exile-play continuation differs from its exact committed capture record".into(),
        );
    }
    Ok(())
}

fn consume_capture_record(
    state: &mut GameState,
    binding: &ExileTopChoiceBindingV1,
    selected: Option<EffectObjectBinding>,
) {
    if let Some(resolving_stack_item) = binding.stack_item_id {
        state
            .engine
            .event_history
            .push(event::CommittedEvent::ExilePlaySelectionConsumedV1 {
                resolving_stack_item,
                selected,
            });
    }
}

pub(super) fn validate_required_stage(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    let printed_strongbox = pending.resolving_item.kind
        == crate::state::StackItemKind::TriggeredAbility
        && state
            .objects
            .try_get(pending.ctx.source)
            .is_some_and(|source| {
                crate::card_def::CARD_DEFS
                    .get(source.card_def as usize)
                    .is_some_and(|definition| definition.name == "Strongbox Raider")
            });
    if !printed_strongbox
        && !pending
            .resolving_item
            .inline_effect
            .as_ref()
            .is_some_and(contains)
    {
        return Ok(());
    }
    if !is_recipe(validated_definition_owned_root_effect(state, pending)?.as_ref()) {
        return Err("Strongbox lost printed root".into());
    }
    let stack = pending
        .ctx
        .stack_item_id
        .ok_or("Strongbox lacks genuine stack identity")?;
    let has_capture = state.engine.event_history.iter().any(|event| matches!(event,
        event::CommittedEvent::ExilePlaySelectionCapturedV1 { resolving_stack_item, .. } if *resolving_stack_item == stack));
    if !has_capture
        && pending.choice.is_none()
        && pending.answered_choice_guard.is_none()
        && pending.frames
            == vec![EffectFrame::Program {
                op: recipe(),
                path: vec![],
            }]
    {
        return Ok(());
    }
    let awaiting = matches!(
        pending.choice,
        Some(PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::ChooseNewlyExiledCard { .. },
            ..
        })
    ) && pending.answered_choice_guard.is_none();
    let answered = pending.choice.is_none()
        && matches!(
            pending.answered_choice_guard,
            Some(EffectAnsweredChoiceGuard::ChosenExilePlayPermission { .. })
        );
    if !awaiting && !answered {
        return Err("Strongbox lost its mandatory selection or authenticated answer stage".into());
    }
    Ok(())
}

fn validate_live_batch(state: &GameState, binding: &ExileTopChoiceBindingV1) -> Result<(), String> {
    if binding.requested_count != 2
        || binding.provenance_digest != digest(binding)?
        || binding.granted_turn != state.turn_key_v1()
        || binding.granted_active_player != state.active_player
        || binding.exiled.len() > usize::from(binding.requested_count)
        || binding.exiled.len() != binding.exile_event_indices.len()
        || binding.resulting_library != bind_library_exact(state, binding.player)
    {
        return Err("new exile batch changed its capture, turn, library or commitment".into());
    }
    for (index, original) in binding.original_library.iter().enumerate() {
        if original.expected_zone != Zone::Library
            || binding.original_library[..index]
                .iter()
                .any(|other| other.object == original.object)
            || state
                .objects
                .try_get(original.object)
                .is_none_or(|live| live.owner != binding.player)
        {
            return Err("exile origin is not a unique controller-owned library".into());
        }
        if index >= usize::from(binding.requested_count) {
            validate_effect_object_binding(state, *original)?;
        }
    }
    let expected_remaining = binding
        .original_library
        .iter()
        .copied()
        .filter(|original| {
            let live = state.objects.get(original.object);
            live.zone == Zone::Library
                && live.zone_change_count == original.expected_zone_change_count
        })
        .collect::<Vec<_>>();
    if binding.resulting_library != expected_remaining {
        return Err("exile batch no longer preserves the exact untouched library remainder".into());
    }
    let prefix = &binding.original_library[..binding
        .original_library
        .len()
        .min(usize::from(binding.requested_count))];
    let expected_exiled = prefix
        .iter()
        .filter_map(|original| {
            let live = state.objects.get(original.object);
            (live.zone == Zone::Exile).then_some(EffectObjectBinding {
                object: original.object,
                expected_zone: Zone::Exile,
                expected_zone_change_count: live.zone_change_count,
            })
        })
        .collect::<Vec<_>>();
    let actual_public = binding
        .exiled
        .iter()
        .map(|card| crate::rl::card_ref(state, card.object).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    if binding.exiled_public != actual_public {
        return Err("Strongbox public exile snapshot changed".into());
    }
    if binding.exiled != expected_exiled {
        return Err("newly exiled candidates no longer match the original ordered prefix".into());
    }
    for (exiled, &event_index) in binding.exiled.iter().zip(&binding.exile_event_indices) {
        validate_effect_object_binding(state, *exiled)?;
        let original = prefix
            .iter()
            .find(|original| original.object == exiled.object)
            .ok_or("exile candidate was not in the captured top two")?;
        if exiled.expected_zone != Zone::Exile
            || Some(exiled.expected_zone_change_count)
                != original.expected_zone_change_count.checked_add(1)
            || state.engine.event_history.get(event_index as usize)
                != Some(&event::CommittedEvent::ZoneChange {
                    object: exiled.object,
                    from: Zone::Library,
                    to: Zone::Exile,
                    controller_before: binding.player,
                })
        {
            return Err("exile candidate lost its exact new incarnation or historical move".into());
        }
    }
    if binding
        .exile_event_indices
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
    {
        return Err("exile candidates changed their committed move order".into());
    }
    Ok(())
}

fn validate_metadata(
    state: &GameState,
    pending: &EffectContinuation,
    binding: &ExileTopChoiceBindingV1,
) -> Result<(), String> {
    validate_recipe(
        state,
        pending,
        binding.requested_count,
        &binding.canonical_path,
        &binding.expected_remaining_frames,
    )?;
    if binding.player != pending.ctx.controller
        || binding.source != pending.ctx.source
        || binding.stack_item_id != pending.ctx.stack_item_id
        || binding.stack_item_id != Some(pending.resolving_item.v4.stack_item_id)
    {
        return Err(
            "exile-play selection changed its captured source, controller or stack incarnation"
                .into(),
        );
    }
    validate_capture_record(state, binding)?;
    validate_live_batch(state, binding)
}

pub(super) fn begin(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    count: u8,
    path: Vec<u16>,
) -> Result<bool, String> {
    let remainder = pending.frames.clone();
    validate_recipe(state, pending, count, &path, &remainder)?;
    let binding = capture_exiled_top(state, &pending.ctx, count, path, remainder)?;
    match binding.exiled.as_slice() {
        [] => {
            consume_capture_record(state, &binding, None);
            Ok(false)
        }
        [chosen] => {
            grant_selected(state, &binding, *chosen)?;
            Ok(false)
        }
        _ => {
            pending.choice = Some(PendingEffectChoice::SelectTargets {
                player: binding.player,
                path: binding.canonical_path.clone(),
                selected: vec![],
                legal: binding
                    .exiled
                    .iter()
                    .copied()
                    .map(|expected_object| EffectTargetCandidate {
                        target: Target::Object(expected_object.object),
                        expected_object: Some(expected_object),
                    })
                    .collect(),
                min_targets: 1,
                max_targets: 1,
                ordered: true,
                purpose: EffectTargetSelectionPurpose::ChooseNewlyExiledCard { binding },
            });
            Ok(true)
        }
    }
}

pub(super) fn validate_choice(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    let Some(PendingEffectChoice::SelectTargets {
        player,
        path,
        selected,
        legal,
        min_targets,
        max_targets,
        ordered,
        purpose: EffectTargetSelectionPurpose::ChooseNewlyExiledCard { binding },
    }) = &pending.choice
    else {
        return Err("exile-play prompt lost its typed selection".into());
    };
    validate_metadata(state, pending, binding)?;
    let expected = binding
        .exiled
        .iter()
        .copied()
        .map(|expected_object| EffectTargetCandidate {
            target: Target::Object(expected_object.object),
            expected_object: Some(expected_object),
        })
        .collect::<Vec<_>>();
    if *player != binding.player
        || *path != binding.canonical_path
        || pending.frames != binding.expected_remaining_frames
        || pending.answered_choice_guard.is_some()
        || !selected.is_empty()
        || binding.exiled.len() != 2
        || *min_targets != 1
        || *max_targets != 1
        || !*ordered
        || *legal != expected
    {
        return Err(
            "exile-play prompt changed its ordered public options or exact-one shape".into(),
        );
    }
    Ok(())
}

pub(super) fn stage_answer(
    pending: &mut EffectContinuation,
    binding: ExileTopChoiceBindingV1,
    chosen: EffectObjectBinding,
) -> Result<(), String> {
    if pending.frames != binding.expected_remaining_frames || !binding.exiled.contains(&chosen) {
        return Err("exile-play answer is not in the exact captured batch".into());
    }
    let frame = EffectFrame::GrantChosenExilePlayPermission { binding, chosen };
    pending.answered_choice_guard = Some(EffectAnsweredChoiceGuard::ChosenExilePlayPermission {
        frame: Box::new(frame.clone()),
    });
    pending.frames.push(frame);
    Ok(())
}

pub(super) fn validate_answer(
    state: &GameState,
    pending: &EffectContinuation,
    frame: &EffectFrame,
) -> Result<(), String> {
    let EffectFrame::GrantChosenExilePlayPermission { binding, chosen } = frame else {
        return Err("exile-play answer guard changed frame kind".into());
    };
    validate_metadata(state, pending, binding)?;
    let mut expected = binding.expected_remaining_frames.clone();
    expected.push(frame.clone());
    if pending.choice.is_some() || pending.frames != expected || !binding.exiled.contains(chosen) {
        return Err(
            "exile-play answered continuation changed its choice or deferred frames".into(),
        );
    }
    Ok(())
}

pub(super) fn resume_answer(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    frame: EffectFrame,
) -> Result<(), String> {
    let EffectFrame::GrantChosenExilePlayPermission { binding, chosen } = &frame else {
        unreachable!()
    };
    if pending.frames != binding.expected_remaining_frames
        || pending.answered_choice_guard.as_ref()
            != Some(&EffectAnsweredChoiceGuard::ChosenExilePlayPermission {
                frame: Box::new(frame.clone()),
            })
    {
        return Err("exile-play resume changed its frame or answer guard".into());
    }
    validate_metadata(state, pending, binding)?;
    grant_selected(state, binding, *chosen)?;
    pending.answered_choice_guard = None;
    Ok(())
}

/// Grant to an exact captured physical incarnation. Root and continuation
/// authentication is performed by begin/resume before this internal primitive.
pub(crate) fn grant_selected(
    state: &mut GameState,
    binding: &ExileTopChoiceBindingV1,
    chosen: EffectObjectBinding,
) -> Result<(), String> {
    validate_capture_record(state, binding)?;
    validate_live_batch(state, binding)?;
    if !binding.exiled.contains(&chosen)
        || state
            .engine
            .exile_play_permissions
            .iter()
            .any(|permission| {
                permission.object == chosen.object
                    && permission.holder == binding.player
                    && permission.zone_change_generation == chosen.expected_zone_change_count
            })
    {
        return Err(
            "exile-play grant changed the selected card or repeated an existing permission".into(),
        );
    }
    let definition =
        &crate::card_def::CARD_DEFS[state.objects.get(chosen.object).card_def as usize];
    let play_or_cast = if definition.is_playable_land() {
        crate::engine::PlayOrCast::Play
    } else if definition.is_castable() {
        crate::engine::PlayOrCast::Cast
    } else {
        consume_capture_record(state, binding, Some(chosen));
        return Ok(());
    }; // unsupported card remains exiled without executable admission
    consume_capture_record(state, binding, Some(chosen));
    let permission = crate::engine::PlayPermission {
        without_mana_cost: crate::engine::FreeCastV1::default(),
        object: chosen.object,
        holder: binding.player,
        zone_change_generation: chosen.expected_zone_change_count,
        play_or_cast,
        expiry: crate::engine::PlayPermissionExpiry::UntilHoldersNextTurn {
            holder_turn_started: false,
        },
    };
    state.engine.exile_play_permissions.push(permission.clone());
    if let Some(stack) = binding.stack_item_id {
        let index = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Strongbox permission overflow")?;
        state
            .engine
            .event_history
            .push(event::CommittedEvent::StrongboxPermissionGrantedV1 {
                installation: Box::new(PermissionInstallationV1 {
                    index,
                    stack,
                    chosen,
                    permission,
                    granted_turn: state.turn_key_v1(),
                }),
            });
    }
    Ok(())
}

fn contains(op: &EffectOp) -> bool {
    match op {
        EffectOp::ExileTopChooseOneUntilNextTurn { .. } => true,
        EffectOp::Sequence(ops) => ops.iter().any(contains),
        EffectOp::Conditional { then, else_, .. } => contains(then) || contains(else_),
        EffectOp::Choice { options, .. } => options.iter().any(contains),
        _ => false,
    }
}

/// Frozen observation/action consumers cannot claim the new selection's
/// semantics merely by relabeling it as their generic CardSelection purpose.
pub(crate) fn require_legacy_consumer_supported(state: &GameState) -> Result<(), String> {
    let pending = state.engine.pending_effect.as_ref();
    let unsupported = state
        .stack
        .iter()
        .any(|item| item.inline_effect.as_ref().is_some_and(contains))
        || pending.is_some_and(|pending| {
            matches!(
                pending.choice,
                Some(PendingEffectChoice::SelectTargets {
                    purpose: EffectTargetSelectionPurpose::ChooseNewlyExiledCard { .. },
                    ..
                })
            ) || matches!(
                pending.answered_choice_guard,
                Some(EffectAnsweredChoiceGuard::ChosenExilePlayPermission { .. })
            ) || pending.frames.iter().any(|frame| {
                matches!(frame, EffectFrame::GrantChosenExilePlayPermission { .. })
                    || matches!(frame, EffectFrame::Program { op, .. } if contains(op))
            })
        });
    if unsupported {
        Err("newly exiled play selection requires an explicit successor observation/action consumer".into())
    } else {
        Ok(())
    }
}

/// Closed progress requires the unique actual exile/selection operation, not a None boundary alone.
pub(crate) fn validate_closed(
    state: &GameState,
    stack: StackItemId,
    source: crate::state::AbilitySourceContractV4,
    first: u32,
    closed: u32,
) -> Result<(), String> {
    let mut captures = Vec::new();
    let mut consumed = Vec::new();
    let mut skipped = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            event::CommittedEvent::ExilePlaySelectionCapturedV1 {
                resolving_stack_item,
                binding,
            } if *resolving_stack_item == stack => captures.push((index, binding.as_ref())),
            event::CommittedEvent::ExilePlaySelectionConsumedV1 {
                resolving_stack_item,
                selected,
            } if *resolving_stack_item == stack => consumed.push((index, *selected)),
            event::CommittedEvent::StrongboxRaidSkippedV1 {
                resolving_stack_item,
                initial_boundary_history_index,
            } if *resolving_stack_item == stack => {
                skipped.push((index, *initial_boundary_history_index))
            }
            _ => {}
        }
    }
    if let [(index, initial)] = skipped.as_slice() {
        if *initial == first
            && (first as usize) < *index
            && *index < (closed as usize)
            && captures.is_empty()
            && consumed.is_empty()
        {
            return Ok(());
        }
    }
    let ([(capture, binding)], [(consume, selected)]) = (captures.as_slice(), consumed.as_slice())
    else {
        return Err("Strongbox closure lacks unique capture/consumption".into());
    };
    if !skipped.is_empty()
        || binding.player != source.controller
        || binding.source != source.source
        || binding.stack_item_id != Some(stack)
        || binding.requested_count != 2
        || binding.provenance_digest != digest(binding)?
        || (first as usize) >= *capture
        || capture >= consume
        || *consume >= closed as usize
        || binding.exiled.len() != binding.exile_event_indices.len()
        || (binding.exiled.is_empty() && selected.is_some())
        || (!binding.exiled.is_empty()
            && selected.is_none_or(|chosen| !binding.exiled.contains(&chosen)))
    {
        return Err(
            "Strongbox closed operation changed source/selected incarnation/lifecycle".into(),
        );
    }
    if binding.exiled_public.len() != binding.exiled.len()
        || binding
            .exiled
            .iter()
            .zip(&binding.exiled_public)
            .any(|(card, public)| {
                public.arena_id != card.object.0
                    || public.zone != Zone::Exile
                    || public.zone_change_count != card.expected_zone_change_count
                    || public.owner != source.controller.into()
            })
    {
        return Err("Strongbox closed public exile characteristics changed".into());
    }
    for (card, index) in binding.exiled.iter().zip(&binding.exile_event_indices) {
        let original = binding
            .original_library
            .iter()
            .take(2)
            .find(|original| original.object == card.object)
            .ok_or("Strongbox selected foreign prefix")?;
        if card.expected_zone != Zone::Exile
            || Some(card.expected_zone_change_count)
                != original.expected_zone_change_count.checked_add(1)
            || *index as usize >= *capture
            || state.engine.event_history.get(*index as usize)
                != Some(&event::CommittedEvent::ZoneChange {
                    object: card.object,
                    from: Zone::Library,
                    to: Zone::Exile,
                    controller_before: source.controller,
                })
        {
            return Err("Strongbox closure lacks genuine ordered exile incarnation".into());
        }
    }
    let grants = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            event::CommittedEvent::StrongboxPermissionGrantedV1 { installation }
                if installation.stack == stack =>
            {
                Some((index, installation.as_ref()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if let Some(chosen) = selected {
        let public = binding
            .exiled_public
            .iter()
            .find(|public| {
                public.arena_id == chosen.object.0
                    && public.zone_change_count == chosen.expected_zone_change_count
            })
            .ok_or("Strongbox selected snapshot missing")?;
        let definition = &crate::card_def::CARD_DEFS[usize::from(public.card_db_id)];
        if definition.is_playable_land() || definition.is_castable() {
            if !matches!(grants.as_slice(),[(index,g)] if g.chosen==*chosen && *index>*consume && *index<closed as usize)
            {
                return Err("Strongbox selection lost actual permission installation".into());
            }
        } else if !grants.is_empty() {
            return Err("unsupported selection acquired permission".into());
        }
    } else if !grants.is_empty() {
        return Err("empty exile acquired permission".into());
    }
    Ok(())
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    super::strongbox_permission_v1::validate(state)?;
    if state.engine.event_log.iter().any(|e| {
        matches!(
            e,
            event::CommittedEvent::ExilePlaySelectionCapturedV1 { .. }
                | event::CommittedEvent::ExilePlaySelectionConsumedV1 { .. }
                | event::CommittedEvent::StrongboxPermissionGrantedV1 { .. }
                | event::CommittedEvent::StrongboxPermissionTurnStartedV1 { .. }
                | event::CommittedEvent::StrongboxPermissionCleanupV1 { .. }
                | event::CommittedEvent::StrongboxPermissionExpiredV1 { .. }
                | event::CommittedEvent::StrongboxRaidSkippedV1 { .. }
        )
    }) {
        return Err("Strongbox private journal entered public log".into());
    }
    for e in &state.engine.event_history {
        let stack = match e {
            event::CommittedEvent::ExilePlaySelectionCapturedV1 {
                resolving_stack_item,
                ..
            }
            | event::CommittedEvent::ExilePlaySelectionConsumedV1 {
                resolving_stack_item,
                ..
            }
            | event::CommittedEvent::StrongboxRaidSkippedV1 {
                resolving_stack_item,
                ..
            } => *resolving_stack_item,
            _ => continue,
        };
        let rows: Vec<_> = state
            .engine
            .event_history
            .iter()
            .filter_map(|e| match e {
                event::CommittedEvent::RaidEntryPublishedV1 { publication }
                    if publication.item.v4.stack_item_id == stack =>
                {
                    Some(publication)
                }
                _ => None,
            })
            .collect();
        if !matches!(rows.as_slice(),[p] if p.item.inline_effect.as_ref().is_some_and(is_recipe)) {
            return Err("Strongbox operation lost genuine printed publication".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::{put, ready_state};
    #[test]
    fn registered_physical_exile_grant_preserves_exact_new_generation_and_single_selection_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let first = put(&mut state, actor, "Forest", Zone::Library);
            let second = put(&mut state, actor, "Lightning Bolt", Zone::Library);
            let ctx = ExecCtx::no_targets(source, actor);
            let binding = capture_exiled_top(&mut state, &ctx, 2, vec![0], vec![]).unwrap();
            assert_eq!(
                binding.exiled.iter().map(|b| b.object).collect::<Vec<_>>(),
                vec![first, second]
            );
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            grant_selected(&mut state, &binding, binding.exiled[1]).unwrap();
            grant_selected(&mut restored, &binding, binding.exiled[1]).unwrap();
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            assert_eq!(state.engine.exile_play_permissions.len(), 1);
            let permission = &state.engine.exile_play_permissions[0];
            assert_eq!(
                (
                    permission.object,
                    permission.holder,
                    permission.zone_change_generation
                ),
                (second, actor, binding.exiled[1].expected_zone_change_count)
            );
            assert_eq!(permission.play_or_cast, crate::engine::PlayOrCast::Cast);
            assert_eq!(
                permission.expiry,
                crate::engine::PlayPermissionExpiry::UntilHoldersNextTurn {
                    holder_turn_started: false
                }
            );
            let before = serde_json::to_vec(&state).unwrap();
            assert!(grant_selected(&mut state, &binding, binding.exiled[1]).is_err());
            assert_eq!(before, serde_json::to_vec(&state).unwrap());
        }
    }
    #[test]
    fn registered_empty_and_short_physical_library_never_selects_foreign_exile() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for size in [0, 1] {
                let mut state = ready_state();
                state.active_player = actor;
                let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
                let foreign = put(&mut state, actor, "Forest", Zone::Hand);
                event::propose_and_commit(
                    &mut state,
                    event::ProposedEvent::zone_change(foreign, Zone::Exile),
                );
                if size == 1 {
                    put(&mut state, actor, "Lightning Bolt", Zone::Library);
                }
                let binding = capture_exiled_top(
                    &mut state,
                    &ExecCtx::no_targets(source, actor),
                    2,
                    vec![0],
                    vec![],
                )
                .unwrap();
                assert_eq!(binding.exiled.len(), size);
                let before = serde_json::to_vec(&state).unwrap();
                let other = EffectObjectBinding {
                    object: foreign,
                    expected_zone: Zone::Exile,
                    expected_zone_change_count: state.objects.get(foreign).zone_change_count,
                };
                assert!(grant_selected(&mut state, &binding, other).is_err());
                assert_eq!(before, serde_json::to_vec(&state).unwrap());
            }
        }
    }
}
