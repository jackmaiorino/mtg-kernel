//! Choice-bearing nonmana Ward payments. All choices bind exact card incarnations.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WardPaymentChoice {
    pub root: Box<EffectOp>,
    pub payer: PlayerId,
    pub candidates: Vec<EffectObjectBinding>,
    pub path: Vec<u16>,
    pub remaining: Vec<EffectFrame>,
}

fn stack_id(op: &EffectOp) -> Result<StackItemId, String> {
    match op {
        EffectOp::CounterUnlessPaysLife {
            targeting_stack_item,
            ..
        }
        | EffectOp::CounterUnlessDiscardsCard {
            targeting_stack_item,
            ..
        }
        | EffectOp::CounterUnlessCollectsEvidence {
            targeting_stack_item,
            ..
        } => Ok(*targeting_stack_item),
        _ => Err("Ward payment has no Ward root".into()),
    }
}
fn candidates(state: &GameState, root: &EffectOp, payer: PlayerId) -> Vec<EffectObjectBinding> {
    let cards = match root {
        EffectOp::CounterUnlessDiscardsCard { .. } => &state.players[payer.index()].hand,
        EffectOp::CounterUnlessCollectsEvidence { .. } => &state.players[payer.index()].graveyard,
        _ => return Vec::new(),
    };
    cards
        .iter()
        .map(|&object| EffectObjectBinding {
            object,
            expected_zone: state.objects.get(object).zone,
            expected_zone_change_count: state.objects.get(object).zone_change_count,
        })
        .collect()
}
fn total(state: &GameState, cards: &[EffectObjectBinding]) -> u32 {
    cards
        .iter()
        .map(|b| {
            u32::from(
                crate::card_def::CARD_DEFS[state.objects.get(b.object).card_def as usize]
                    .mana_value,
            )
        })
        .sum()
}
pub(super) fn selection_minimum(
    state: &GameState,
    choice: &WardPaymentChoice,
    selected: &[EffectObjectBinding],
) -> u16 {
    match *choice.root {
        EffectOp::CounterUnlessCollectsEvidence {
            minimum_mana_value, ..
        } if !selected.is_empty() && total(state, selected) < u32::from(minimum_mana_value) => {
            u16::try_from(selected.len() + 1).unwrap_or(u16::MAX)
        }
        _ => 0,
    }
}
pub(super) fn maximum(choice: &WardPaymentChoice) -> u16 {
    if matches!(*choice.root, EffectOp::CounterUnlessDiscardsCard { .. }) {
        1
    } else {
        u16::try_from(choice.candidates.len()).unwrap_or(u16::MAX)
    }
}
fn payable(state: &GameState, choice: &WardPaymentChoice) -> bool {
    match *choice.root {
        EffectOp::CounterUnlessPaysLife { life, .. } => {
            state.players[choice.payer.index()].life >= i32::from(life)
        }
        EffectOp::CounterUnlessDiscardsCard { .. } => !choice.candidates.is_empty(),
        EffectOp::CounterUnlessCollectsEvidence {
            minimum_mana_value, ..
        } => total(state, &choice.candidates) >= u32::from(minimum_mana_value),
        _ => false,
    }
}
pub(super) fn validate_choice(
    state: &GameState,
    continuation: &EffectContinuation,
    choice: &WardPaymentChoice,
) -> Result<(), String> {
    let root = continuation
        .resolving_item
        .inline_effect
        .as_ref()
        .ok_or("Ward payment lost root")?;
    if effect_op_at_path(root, &choice.path) != Some(&*choice.root)
        || continuation.frames != choice.remaining
    {
        return Err("Ward payment changed root or continuation".into());
    }
    let id = stack_id(&choice.root)?;
    let payer = state
        .stack
        .iter()
        .find(|item| item.v4.stack_item_id == id)
        .ok_or("Ward payment lost targeted stack item")?
        .controller;
    if payer != choice.payer
        || candidates(state, &choice.root, payer) != choice.candidates
        || !payable(state, choice)
    {
        return Err("Ward payment changed payer or payable cards".into());
    }
    Ok(())
}
pub(super) fn stage(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    root: EffectOp,
    path: Vec<u16>,
) -> Result<bool, String> {
    let id = stack_id(&root)?;
    let Some(payer) = state
        .stack
        .iter()
        .find(|item| item.v4.stack_item_id == id)
        .map(|item| item.controller)
    else {
        return Ok(false);
    };
    let choice = WardPaymentChoice {
        candidates: candidates(state, &root, payer),
        root: Box::new(root),
        payer,
        path: path.clone(),
        remaining: continuation.frames.clone(),
    };
    if !payable(state, &choice) {
        crate::engine::counter_stack_item_by_id(state, id)?;
        return Ok(false);
    }
    validate_choice(state, continuation, &choice)?;
    if matches!(*choice.root, EffectOp::CounterUnlessPaysLife { .. }) {
        continuation.choice = Some(PendingEffectChoice::ChooseBoolean {
            player: payer,
            path,
            default: false,
            purpose: EffectBooleanChoicePurpose::WardLife { choice },
        });
    } else {
        continuation.choice = Some(PendingEffectChoice::SelectTargets {
            player: payer,
            path,
            selected: Vec::new(),
            legal: choice
                .candidates
                .iter()
                .map(|b| EffectTargetCandidate {
                    target: Target::Object(b.object),
                    expected_object: Some(*b),
                })
                .collect(),
            min_targets: 0,
            max_targets: maximum(&choice),
            ordered: false,
            purpose: EffectTargetSelectionPurpose::WardCards { choice },
        });
    }
    Ok(true)
}
pub(super) fn resolve_payment(
    state: &mut GameState,
    continuation: &EffectContinuation,
    choice: &WardPaymentChoice,
    selected: &[EffectObjectBinding],
    pay: bool,
) -> Result<(), String> {
    validate_choice(state, continuation, choice)?;
    let id = stack_id(&choice.root)?;
    let canonical = canonicalize_binding_subset(&choice.candidates, selected)?;
    if !pay {
        if !selected.is_empty() {
            return Err("declined Ward payment contains cards".into());
        }
        return crate::engine::counter_stack_item_by_id(state, id);
    }
    match *choice.root {
        EffectOp::CounterUnlessPaysLife { life, .. } if selected.is_empty() => {
            event::propose_and_commit(
                state,
                event::ProposedEvent::life_payment(choice.payer, i32::from(life)),
            );
        }
        EffectOp::CounterUnlessDiscardsCard { .. } if canonical.len() == 1 => {
            event::propose_and_commit(
                state,
                event::ProposedEvent::zone_change(canonical[0].object, Zone::Graveyard),
            );
        }
        EffectOp::CounterUnlessCollectsEvidence {
            minimum_mana_value, ..
        } if total(state, &canonical) >= u32::from(minimum_mana_value) => {
            let events = canonical
                .iter()
                .map(|b| event::ProposedEvent::zone_change(b.object, Zone::Exile))
                .collect();
            event::propose_and_commit_batch(state, events);
        }
        _ => return Err("Ward payment answer does not pay its printed cost".into()),
    }
    Ok(())
}
