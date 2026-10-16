//! Typed controlled-other-nontoken death recipe. Source preparation only:
//! the genuine Partial printing does not claim qualified gameplay/consumers.
use crate::{
    card_def::{CardDef, CardType, OtherControlledNontokenDeathDefV1, CARD_DEFS},
    effect::{
        EffectContinuation, EffectFrame, EffectOp, EffectOptionChoicePurpose, PendingEffectChoice,
        PlayerRef,
    },
    state::{GameState, StackItem, StackItemKind, Zone},
    trigger::{TriggerCondition, TriggeredAbilityDef},
};

/// Decline is option zero. Neither the library nor RNG is read by this choice.
pub fn death_program() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            EffectOp::Sequence(vec![]),
            EffectOp::RevealUntilCardTypeToHandRandomBottomV1 {
                player: PlayerRef::Controller,
                card_type: CardType::Creature,
            },
        ],
    }
}

static TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::OtherControlledNontokenCreatureDiesV1,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: death_program,
}];

pub(crate) fn triggers(card: &CardDef) -> &'static [TriggeredAbilityDef] {
    match card.other_controlled_nontoken_death_v1 {
        Some(OtherControlledNontokenDeathDefV1::MayRevealCreatureToHandRandomBottom) => &TRIGGERS,
        None => &[],
    }
}

/// Genuine native pre-removal authority, never a restored DTO or constructor
/// available outside this module. The public resume boundary keeps requiring
/// its complete visible row; only this owner may lend back the row just removed
/// by the engine for synchronous validation and execution.
pub(crate) struct ResumeOwnerV1 {
    pending: EffectContinuation,
    position: usize,
    remaining_stack: Vec<StackItem>,
}

pub(crate) fn acquire_resume_owner(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<ResumeOwnerV1>, String> {
    let Some(pending) = state.engine.pending_effect.as_ref() else {
        return Ok(None);
    };
    let selected = pending.resolving_item.kind == StackItemKind::TriggeredAbility
        && pending
            .resolving_item
            .v4
            .ability_source_contract
            .is_some_and(|source| {
                CARD_DEFS
                    .get(usize::from(source.card_def))
                    .is_some_and(|card| card.other_controlled_nontoken_death_v1.is_some())
            });
    if !selected {
        return Ok(None);
    }
    crate::effect::validate_pending_effect_choice(state)?;
    if pending.resolving_item != *item || pending.choice.is_some() {
        return Err("controlled-death native resumption lost its exact ready continuation".into());
    }
    let position = crate::engine::resolution_cast_v1::resolving_index(state, item.v4.stack_item_id)
        .ok_or("controlled-death native resumption lost its public row")?;
    let mut remaining_stack = state.stack.clone();
    if remaining_stack.remove(position) != *item
        || remaining_stack
            .iter()
            .any(|row| row.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("controlled-death native resumption changed/duplicated its public row".into());
    }
    Ok(Some(ResumeOwnerV1 {
        pending: pending.clone(),
        position,
        remaining_stack,
    }))
}

pub(crate) fn resume_owned(
    state: &mut GameState,
    owner: ResumeOwnerV1,
) -> Result<crate::effect::ResumableProgress, String> {
    if state.engine.pending_effect.as_ref() != Some(&owner.pending)
        || state.stack != owner.remaining_stack
    {
        return Err("controlled-death private removed-row ownership changed".into());
    }
    let item = &owner.pending.resolving_item;
    state.stack.insert(owner.position, item.clone());
    let loaned_stack = state.stack.clone();
    // Run the unchanged public guard with the authentic row loaned back at
    // its original position. No public choice, action, or restore intervenes.
    let result = crate::effect::resume_resumable_resolution(state);
    // The closed decline/reveal program never edits the stack. Return the
    // authentic removed-row state on both Ok and Err, then refuse any changed,
    // stale, or duplicated row instead of allowing a loan to escape.
    let returned_stack = std::mem::replace(&mut state.stack, owner.remaining_stack);
    if returned_stack != loaned_stack {
        return Err("controlled-death private stack changed before loan return".into());
    }
    result
}

/// A generic option wire shape still needs definition-owned payloads and
/// exact answered frames. This validates both suspended and answered restores,
/// including decline, before any consequence can inspect the hidden library.
pub(crate) fn validate_continuation(
    _state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    let Some(source) = pending.resolving_item.v4.ability_source_contract else {
        return Ok(());
    };
    if pending.resolving_item.kind != StackItemKind::TriggeredAbility
        || CARD_DEFS
            .get(usize::from(source.card_def))
            .is_none_or(|card| card.other_controlled_nontoken_death_v1.is_none())
    {
        return Ok(());
    }
    let root = death_program();
    if pending.resolving_item.inline_effect.as_ref() != Some(&root)
        || pending.answered_choice_guard.is_some()
        || pending.ctx.controller != source.controller
    {
        return Err("controlled-death reveal lost its definition-owned recipe".into());
    }
    let EffectOp::Choice { options, .. } = &root else {
        unreachable!()
    };
    match pending.choice.as_ref() {
        Some(PendingEffectChoice::ChooseOption {
            player,
            path,
            options: actual,
            purpose: EffectOptionChoicePurpose::Generic,
        }) if *player == source.controller
            && path.is_empty()
            && actual == options
            && pending.frames.is_empty() =>
        {
            Ok(())
        }
        None => match pending.frames.as_slice() {
            [EffectFrame::Program { op, path }] if op == &root && path.is_empty() => Ok(()),
            [EffectFrame::Program { op, path }]
                if path.len() == 1 && options.get(usize::from(path[0])) == Some(op) =>
            {
                Ok(())
            }
            _ => Err("controlled-death reveal answered frames changed".into()),
        },
        _ => Err("controlled-death reveal optional choice changed".into()),
    }
}
