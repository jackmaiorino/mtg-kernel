//! A payer-private optional discard with a definition-owned unpaid branch.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DiscardPaymentChoiceV1 {
    pub player: PlayerId,
    pub player_ref: PlayerRef,
    pub unpaid: Box<EffectOp>,
    pub hand: Vec<EffectObjectBinding>,
    pub path: Vec<u16>,
}

pub(super) fn validate_required_stage(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    fn contains(op: &EffectOp) -> bool {
        match op {
            EffectOp::MayDiscardCardOtherwiseV1 { .. } => true,
            EffectOp::Sequence(ops) | EffectOp::Choice { options: ops, .. } => {
                ops.iter().any(contains)
            }
            EffectOp::Conditional { then, else_, .. } => contains(then) || contains(else_),
            EffectOp::MaySacrificeNonlandOtherwiseV1 { unpaid, .. } => contains(unpaid),
            _ => false,
        }
    }
    let candidate = pending.resolving_item.inline_effect.clone().or_else(|| {
        let source = state.objects.try_get(pending.resolving_item.source)?;
        crate::card_def::CARD_DEFS
            .get(usize::from(source.card_def))?
            .printed_mode_effect(pending.resolving_item.mode_chosen)
    });
    if !candidate.as_ref().is_some_and(contains) {
        return Ok(());
    }
    let root = validated_definition_owned_root_effect(state, pending)?;
    for frame in &pending.frames {
        if let EffectFrame::Program { op, path } = frame {
            if effect_op_at_path(&root, path) != Some(op) {
                return Err("discard payment continuation injected a nonprinted program".into());
            }
        }
    }
    // A root payment with a synchronous unpaid child has only these phases.
    // Choice-bearing unpaid children use their own definition-owned validator.
    if let EffectOp::MayDiscardCardOtherwiseV1 { unpaid, .. } = root.as_ref() {
        if contains_player_choice(unpaid) {
            return Ok(());
        }
        if let Some(choice) = &pending.choice {
            if !matches!(
                choice,
                PendingEffectChoice::SelectTargets {
                    purpose: EffectTargetSelectionPurpose::DiscardPaymentV1 { .. },
                    ..
                }
            ) {
                return Err("discard payment changed its choice purpose".into());
            }
        } else if pending.answered_choice_guard.is_none() {
            let initial = vec![EffectFrame::Program {
                op: (*root).clone(),
                path: vec![],
            }];
            let declined = vec![EffectFrame::Program {
                op: (**unpaid).clone(),
                path: vec![0],
            }];
            if pending.frames != initial && pending.frames != declined {
                return Err("discard payment lost its required root or unpaid continuation".into());
            }
        }
    }
    Ok(())
}

/// Derive the actual pending siblings from the static program. A duplicated
/// snapshot in a forged guard is not authority for trailing instructions.
fn remaining(root: &EffectOp, path: &[u16]) -> Result<Vec<EffectFrame>, String> {
    let mut op = root;
    let mut prefix = Vec::new();
    let mut frames = Vec::new();
    for &component in path {
        op = match op {
            EffectOp::Sequence(ops) => {
                let index = usize::from(component);
                let child = ops.get(index).ok_or("discard payment path changed")?;
                for (ordinal, sibling) in ops.iter().enumerate().skip(index + 1).rev() {
                    let mut path = prefix.clone();
                    path.push(u16::try_from(ordinal).map_err(|_| "effect path overflow")?);
                    frames.push(EffectFrame::Program {
                        op: sibling.clone(),
                        path,
                    });
                }
                child
            }
            EffectOp::Conditional { then, else_, .. } => match component {
                0 => then,
                1 => else_,
                _ => return Err("discard payment branch changed".into()),
            },
            EffectOp::Choice { options, .. } => options
                .get(usize::from(component))
                .ok_or("discard payment option changed")?,
            EffectOp::MayDiscardCardOtherwiseV1 { unpaid, .. }
            | EffectOp::MaySacrificeNonlandOtherwiseV1 { unpaid, .. }
                if component == 0 =>
            {
                unpaid
            }
            _ => return Err("discard payment path is not definition-owned".into()),
        };
        prefix.push(component);
    }
    Ok(frames)
}

pub(super) fn validate_choice(
    state: &GameState,
    pending: &EffectContinuation,
    choice: &DiscardPaymentChoiceV1,
) -> Result<(), String> {
    let root = validated_definition_owned_root_effect(state, pending)?;
    if effect_op_at_path(&root, &choice.path)
        != Some(&EffectOp::MayDiscardCardOtherwiseV1 {
            player: choice.player_ref,
            unpaid: choice.unpaid.clone(),
        })
        || pending.ctx.resolve_player(choice.player_ref, state) != choice.player
        || pending.frames != remaining(&root, &choice.path)?
        || choice.hand.is_empty()
    {
        return Err("discard payment lost its definition-owned payer, branch or frames".into());
    }
    validate_bound_hand_exact(state, choice.player, &choice.hand)
}

pub(super) fn validate_selected(
    choice: &DiscardPaymentChoiceV1,
    selected: &[EffectObjectBinding],
) -> Result<(), String> {
    if selected.len() > 1 || selected.iter().any(|card| !choice.hand.contains(card)) {
        return Err("discard payment selected a different hand incarnation".into());
    }
    Ok(())
}

fn unpaid(pending: &mut EffectContinuation, program: EffectOp, mut path: Vec<u16>) {
    path.push(0);
    pending
        .frames
        .push(EffectFrame::Program { op: program, path });
}

pub(super) fn stage(
    state: &GameState,
    pending: &mut EffectContinuation,
    player_ref: PlayerRef,
    program: Box<EffectOp>,
    path: Vec<u16>,
) -> Result<bool, String> {
    let player = pending.ctx.resolve_player(player_ref, state);
    let hand = bind_hand(state, player);
    // Even the forced unpaid route must originate in the printed program.
    let root = validated_definition_owned_root_effect(state, pending)?;
    if effect_op_at_path(&root, &path)
        != Some(&EffectOp::MayDiscardCardOtherwiseV1 {
            player: player_ref,
            unpaid: program.clone(),
        })
        || pending.frames != remaining(&root, &path)?
    {
        return Err("discard payment root changed".into());
    }
    if hand.is_empty() {
        unpaid(pending, *program, path);
        return Ok(false);
    }
    let choice = DiscardPaymentChoiceV1 {
        player,
        player_ref,
        unpaid: program,
        hand,
        path: path.clone(),
    };
    validate_choice(state, pending, &choice)?;
    pending.choice = Some(PendingEffectChoice::SelectTargets {
        player,
        path,
        selected: vec![],
        legal: choice
            .hand
            .iter()
            .map(|card| EffectTargetCandidate {
                target: Target::Object(card.object),
                expected_object: Some(*card),
            })
            .collect(),
        min_targets: 0,
        max_targets: 1,
        ordered: false,
        purpose: EffectTargetSelectionPurpose::DiscardPaymentV1 { choice },
    });
    Ok(true)
}

pub(super) fn finish(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    choice: &DiscardPaymentChoiceV1,
    selected: &[EffectObjectBinding],
) -> Result<(), String> {
    validate_choice(state, pending, choice)?;
    validate_selected(choice, selected)?;
    if let Some(card) = selected.first() {
        // Use the genuine discard event/replacement path, including Madness.
        crate::engine::commit_discarded_card(state, card.object);
    } else {
        unpaid(pending, (*choice.unpaid).clone(), choice.path.clone());
    }
    Ok(())
}
