//! Private top-N selection and ordering, with no zone changes between choices.
use super::*;

fn prefix(library: &[EffectObjectBinding], count: u8) -> &[EffectObjectBinding] {
    &library[..library.len().min(usize::from(count))]
}

fn metadata(
    state: &GameState,
    pending: &EffectContinuation,
    player: PlayerId,
    count: u8,
    library: &[EffectObjectBinding],
    path: &[u16],
    remainder: &[EffectFrame],
) -> Result<Option<u16>, String> {
    let root = validated_definition_owned_root_effect(state, pending)?;
    let max_kept = if private_keep_one_v1::is_recipe(&root) {
        private_keep_one_v1::validate_recipe(state, pending, player, count, path, remainder)?;
        private_keep_one_v1::validate_snapshot(state, pending, player, library)?;
        Some(1)
    } else {
        let EffectOp::Surveil {
            player: printed_player,
            count: printed_count,
        } = *root
        else {
            return Err("library partition lost its definition-owned root program".into());
        };
        if count <= 1
            || printed_count != count
            || pending.ctx.resolve_player(printed_player, state) != player
            || !path.is_empty()
            || !remainder.is_empty()
        {
            return Err("surveil batch changed its source, count or continuation".into());
        }
        None
    };
    if library != bind_library_exact(state, player) || prefix(library, count).is_empty() {
        return Err("surveil batch changed its source, count, library or continuation".into());
    }
    for &binding in library {
        validate_effect_object_binding(state, binding)?;
        if binding.expected_zone != Zone::Library
            || state.objects.get(binding.object).owner != player
        {
            return Err("surveil batch lost its private library binding".into());
        }
    }
    Ok(max_kept)
}

fn kept(
    library: &[EffectObjectBinding],
    count: u8,
    graveyard: &[EffectObjectBinding],
) -> Result<Vec<EffectObjectBinding>, String> {
    let top = prefix(library, count);
    if graveyard
        .iter()
        .enumerate()
        .any(|(i, card)| !top.contains(card) || graveyard[..i].contains(card))
    {
        return Err("surveil graveyard subset changed its bound prefix".into());
    }
    Ok(top
        .iter()
        .copied()
        .filter(|card| !graveyard.contains(card))
        .collect())
}

fn stage(
    continuation: &mut EffectContinuation,
    player: PlayerId,
    count: u8,
    library: Vec<EffectObjectBinding>,
    graveyard: Option<Vec<EffectObjectBinding>>,
    path: Vec<u16>,
    remainder: Vec<EffectFrame>,
    max_kept: Option<u16>,
) -> Result<(), String> {
    let candidates = match &graveyard {
        None => prefix(&library, count).to_vec(),
        Some(cards) => kept(&library, count, cards)?,
    };
    let maximum =
        u16::try_from(candidates.len()).map_err(|_| "surveil prefix exceeds target range")?;
    continuation.choice = Some(PendingEffectChoice::SelectTargets {
        player,
        path: path.clone(),
        selected: Vec::new(),
        legal: candidates
            .into_iter()
            .map(|binding| EffectTargetCandidate {
                target: Target::Object(binding.object),
                expected_object: Some(binding),
            })
            .collect(),
        min_targets: if graveyard.is_some() {
            maximum
        } else {
            max_kept.map_or(0, |limit| maximum.saturating_sub(limit))
        },
        max_targets: maximum,
        ordered: true,
        purpose: EffectTargetSelectionPurpose::SurveilLibraryMany {
            player,
            requested_count: count,
            original_library: library,
            graveyard_order: graveyard,
            canonical_path: path,
            expected_remaining_frames: remainder,
        },
    });
    Ok(())
}

pub(super) fn begin(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    player: PlayerId,
    count: u8,
    path: Vec<u16>,
) -> Result<bool, String> {
    let library = bind_library_exact(state, player);
    let remainder = pending.frames.clone();
    if private_keep_one_v1::is_recipe(
        validated_definition_owned_root_effect(state, pending)?.as_ref(),
    ) {
        private_keep_one_v1::validate_recipe(state, pending, player, count, &path, &remainder)?;
        let binding = private_keep_one_v1::capture(state, &pending.ctx)?;
        if library.is_empty() {
            private_keep_one_v1::commit(state, &binding, &[])?;
            return Ok(false);
        }
    } else if library.is_empty() {
        return Ok(false);
    }
    let max_kept = metadata(state, pending, player, count, &library, &path, &remainder)?;
    state.reveal_library_top(player, player, prefix(&library, count).len());
    stage(
        pending, player, count, library, None, path, remainder, max_kept,
    )?;
    Ok(true)
}

pub(super) fn validate_choice(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    let Some(PendingEffectChoice::SelectTargets {
        player: chooser,
        path,
        selected,
        legal,
        min_targets,
        max_targets,
        ordered,
        purpose:
            EffectTargetSelectionPurpose::SurveilLibraryMany {
                player,
                requested_count,
                original_library,
                graveyard_order,
                canonical_path,
                expected_remaining_frames,
            },
    }) = &pending.choice
    else {
        return Err("surveil batch lost its typed choice".into());
    };
    let max_kept = metadata(
        state,
        pending,
        *player,
        *requested_count,
        original_library,
        canonical_path,
        expected_remaining_frames,
    )?;
    if max_kept.is_some() && graveyard_order.is_some() {
        return Err("keep-one look cannot acquire a retained-card ordering stage".into());
    }
    let candidates = match graveyard_order {
        None => prefix(original_library, *requested_count).to_vec(),
        Some(cards) => kept(original_library, *requested_count, cards)?,
    };
    let maximum =
        u16::try_from(candidates.len()).map_err(|_| "surveil prefix exceeds target range")?;
    if chooser != player
        || path != canonical_path
        || pending.frames != *expected_remaining_frames
        || !ordered
        || *max_targets != maximum
        || *min_targets
            != if graveyard_order.is_some() {
                maximum
            } else {
                max_kept.map_or(0, |limit| maximum.saturating_sub(limit))
            }
    {
        return Err("surveil batch prompt changed its player, path or selection shape".into());
    }
    let mut actual = selected
        .iter()
        .chain(legal)
        .map(|candidate| {
            let binding = candidate
                .expected_object
                .ok_or("surveil candidate lost its incarnation")?;
            if candidate.target != Target::Object(binding.object) {
                return Err("surveil candidate changed its target");
            }
            Ok(binding)
        })
        .collect::<Result<Vec<_>, &str>>()?;
    actual.sort_by_key(|card| card.object);
    let mut expected = candidates;
    expected.sort_by_key(|card| card.object);
    if actual != expected {
        return Err("surveil candidates no longer partition the bound prefix".into());
    }
    Ok(())
}

pub(super) fn validate_frame(
    state: &GameState,
    pending: &EffectContinuation,
    frame: &EffectFrame,
) -> Result<(), String> {
    let EffectFrame::SurveilLibraryMany {
        player,
        requested_count,
        original_library,
        graveyard_order,
        kept_order,
        path,
        expected_remaining_frames,
    } = frame
    else {
        return Err("surveil batch guard changed frame kind".into());
    };
    let max_kept = metadata(
        state,
        pending,
        *player,
        *requested_count,
        original_library,
        path,
        expected_remaining_frames,
    )?;
    let expected = kept(original_library, *requested_count, graveyard_order)?;
    if max_kept.is_some_and(|limit| expected.len() > usize::from(limit))
        || (max_kept.is_some() && kept_order.is_some())
    {
        return Err("keep-one answer attempted multiple retained cards or an extra stage".into());
    }
    if let Some(ordered) = kept_order {
        validate_exact_binding_permutation(&expected, ordered, "surveil kept order")?;
    }
    Ok(())
}

pub(super) fn resume(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    frame: EffectFrame,
) -> Result<bool, String> {
    let EffectFrame::SurveilLibraryMany {
        player,
        requested_count,
        original_library,
        graveyard_order,
        kept_order,
        path,
        expected_remaining_frames,
    } = &frame
    else {
        unreachable!()
    };
    if pending.frames != *expected_remaining_frames
        || pending.answered_choice_guard.as_ref()
            != Some(&EffectAnsweredChoiceGuard::SurveilLibraryMany {
                frame: Box::new(frame.clone()),
            })
    {
        return Err("surveil batch answered frame/guard mismatch".into());
    }
    validate_frame(state, pending, &frame)?;
    pending.answered_choice_guard = None;
    if private_keep_one_v1::is_recipe(
        validated_definition_owned_root_effect(state, pending)?.as_ref(),
    ) {
        private_keep_one_v1::commit_pending(state, pending, graveyard_order)?;
        return Ok(false);
    }
    let kept = kept(original_library, *requested_count, graveyard_order)?;
    if kept_order.is_none() && kept.len() >= 2 {
        stage(
            pending,
            *player,
            *requested_count,
            original_library.clone(),
            Some(graveyard_order.clone()),
            path.clone(),
            expected_remaining_frames.clone(),
            None,
        )?;
        return Ok(true);
    }
    // Both accepted orders are now fixed. Commit the graveyard moves as one
    // batch, then reorder only the still-live kept prefix without a new prompt.
    commit_zone_change_batch(state, graveyard_order, Zone::Graveyard, true)?;
    let ordered = kept_order.as_ref().unwrap_or(&kept);
    if ordered.len() >= 2 {
        validate_bound_library_prefix_exact(state, *player, &kept)?;
        state.reorder_library_top(
            *player,
            &ordered
                .iter()
                .map(|binding| binding.object)
                .collect::<Vec<_>>(),
            &[*player],
        )?;
    }
    Ok(false)
}
