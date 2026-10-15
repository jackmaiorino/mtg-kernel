//! Typed controlled-other-nontoken death recipe. Source preparation only:
//! the genuine Partial printing does not claim qualified gameplay/consumers.
use crate::{
    card_def::{CardDef, CardType, OtherControlledNontokenDeathDefV1, CARD_DEFS},
    effect::{
        EffectContinuation, EffectFrame, EffectOp, EffectOptionChoicePurpose, PendingEffectChoice,
        PlayerRef,
    },
    state::{GameState, StackItemKind, Zone},
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
    let EffectOp::Choice { options, .. } = root else {
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
            && actual == &options
            && pending.frames.is_empty() =>
        {
            Ok(())
        }
        None => match pending.frames.as_slice() {
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
