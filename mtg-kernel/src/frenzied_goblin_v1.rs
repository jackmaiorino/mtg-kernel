//! Definition-owned attack provenance and optional-payment continuation shape.
use crate::effect::{
    EffectAnsweredChoiceGuard, EffectBooleanChoicePurpose, EffectContinuation, EffectFrame,
    EffectOp, PendingEffectChoice,
};
use crate::event::CommittedEvent;
use crate::mana::ManaColor;
use crate::state::{AbilitySourceContractV4, GameState, Zone};

pub(crate) fn is_owned(card_def: u16) -> bool {
    crate::card_def::CARD_DEFS
        .get(usize::from(card_def))
        .is_some_and(|d| d.name == "Frenzied Goblin" && d.is_executable())
}

pub(crate) fn validate_source(
    state: &GameState,
    card_def: u16,
    source: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    if !is_owned(card_def) {
        return Ok(());
    }
    let source = source.ok_or("Frenzied attack lost its captured source")?;
    if source.card_def != card_def
        || source.zone != Zone::Battlefield
        || state
            .engine
            .event_history
            .iter()
            .rev()
            .find_map(|event| match event {
                CommittedEvent::DeclaredAttacker {
                    source: attacker,
                    source_zone_change_count,
                    controller,
                } if *attacker == source.source
                    && *source_zone_change_count == source.zone_change_count =>
                {
                    Some(*controller)
                }
                _ => None,
            })
            != Some(source.controller)
    {
        return Err("Frenzied trigger lacks its committed exact attack declaration".into());
    }
    Ok(())
}

pub(crate) fn validate_continuation(
    _state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    let Some(source) = pending.ctx.ability_source_contract else {
        return Ok(());
    };
    if !is_owned(source.card_def) {
        return Ok(());
    }
    let root = crate::trigger::frenzied_goblin_effect();
    let consequence = EffectOp::PreventTargetBlockingUntilEndOfTurn { target_index: 0 };
    // These are the actual externally restorable phases. A declining answer
    // or an unaffordable root leaves no frames. The paid consequence executes
    // atomically inside the authenticated PayManaThen frame, never as a free
    // independently restorable Program frame.
    let valid = match (
        &pending.choice,
        pending.frames.as_slice(),
        &pending.answered_choice_guard,
    ) {
        (None, [EffectFrame::Program { op, path }], None) => *op == root && path.is_empty(),
        (
            Some(PendingEffectChoice::ChooseBoolean {
                player,
                path,
                default,
                purpose:
                    EffectBooleanChoicePurpose::PayManaThen {
                        player: payer,
                        colored,
                        generic,
                        then,
                    },
            }),
            [],
            None,
        ) => {
            *player == source.controller
                && *payer == source.controller
                && path.is_empty()
                && *default == Some(false)
                && colored == &[ManaColor::R]
                && *generic == 0
                && **then == consequence
        }
        (
            None,
            [frame @ EffectFrame::PayManaThen {
                player,
                colored,
                generic,
                then,
                path,
                canonical_path,
                expected_remaining_frames,
            }],
            Some(EffectAnsweredChoiceGuard::PayManaThen { frame: guard }),
        ) => {
            *player == source.controller
                && colored == &[ManaColor::R]
                && *generic == 0
                && **then == consequence
                && path.as_slice() == [1]
                && canonical_path.is_empty()
                && expected_remaining_frames.is_empty()
                && **guard == *frame
        }
        (None, [], None) => true,
        _ => false,
    };
    if !valid {
        return Err("Frenzied optional payment lost its definition-owned stage".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_optional_red_recipe_and_absent_definition_path_are_readonly() {
        assert_eq!(
            crate::trigger::frenzied_goblin_effect(),
            EffectOp::MayPayManaThen {
                player: crate::effect::PlayerRef::Controller,
                colored: vec![ManaColor::R],
                generic: 0,
                then: Box::new(EffectOp::PreventTargetBlockingUntilEndOfTurn { target_index: 0 }),
            }
        );
        let elf = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
        let state = GameState::new_from_libraries(&[elf; 4], &[elf; 4], |_| "elf".into(), 1141);
        let before = serde_json::to_vec(&state).unwrap();
        assert!(!is_owned(elf));
        validate_source(&state, elf, None).unwrap();
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}
