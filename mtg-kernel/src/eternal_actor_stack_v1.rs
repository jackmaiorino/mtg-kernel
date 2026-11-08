//! Bind an already public stack row to its particular printed program.
//! Neither private saved continuations nor arbitrary inline EffectOps are
//! serialized. Unrecognized dynamic/granted programs refuse for now.
use crate::{
    card_def::CARD_DEFS,
    effect::EffectOp,
    rl::{RlContractError, StackItemPublicV2},
    state::{GameState, StackItemKind},
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StackProgramV1 {
    pub stack_index: u32,
    pub origin: ProgramOriginV1,
    pub program: EffectOp,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ProgramOriginV1 {
    SpellMode { mode: u8 },
    PrintedActivation { ability_index: u8 },
    PrintedTrigger { trigger_index: u32 },
}

pub fn capture_v1(
    state: &GameState,
    public: &[StackItemPublicV2],
) -> Result<Vec<StackProgramV1>, RlContractError> {
    if state.stack.len() != public.len() {
        return Err(error());
    }
    state
        .stack
        .iter()
        .zip(public)
        .enumerate()
        .map(|(index, (item, view))| {
            if view.stack_index as usize != index
                || view.source.arena_id != item.source.0
                || view.controller != item.controller.into()
                || view.stack_item_kind != item.kind.into()
                || view.mode_chosen != item.mode_chosen
                || view.face_index != item.v4.face_index
            {
                return Err(error());
            }
            // The caller's authenticated public source can be historical. Never
            // read the physical source's current (possibly hidden) card definition.
            let printing = view.source.card_db_id;
            crate::eternal_actor_printed_v1::printing_v1(printing)?;
            let def = CARD_DEFS.get(usize::from(printing)).ok_or_else(error)?;
            let (origin, program) = match item.kind {
                StackItemKind::Spell if item.inline_effect.is_none() && item.v4.face_index == 0 => {
                    (
                        ProgramOriginV1::SpellMode {
                            mode: item.mode_chosen,
                        },
                        def.printed_mode_effect(item.mode_chosen)
                            .ok_or_else(error)?,
                    )
                }
                StackItemKind::ActivatedAbility => {
                    let ability_index = item.v4.activated_ability_index.ok_or_else(error)?;
                    let ability = def
                        .activated_abilities
                        .get(usize::from(ability_index))
                        .ok_or_else(error)?;
                    let program = (ability.effect)();
                    if item.inline_effect.as_ref() != Some(&program) || item.v4.granted_by.is_some()
                    {
                        return Err(error());
                    }
                    (
                        ProgramOriginV1::PrintedActivation { ability_index },
                        program,
                    )
                }
                StackItemKind::TriggeredAbility => {
                    let program = item.inline_effect.as_ref().ok_or_else(error)?;
                    let trigger_index = crate::trigger::triggers_for(printing)
                        .iter()
                        .position(|trigger| (trigger.effect)() == *program)
                        .ok_or_else(error)?;
                    (
                        ProgramOriginV1::PrintedTrigger {
                            trigger_index: trigger_index as u32,
                        },
                        program.clone(),
                    )
                }
                _ => return Err(error()),
            };
            Ok(StackProgramV1 {
                stack_index: view.stack_index,
                origin,
                program,
            })
        })
        .collect()
}

fn error() -> RlContractError {
    RlContractError("unrepresented or mismatched Eternal public stack program".into())
}

#[cfg(all(test, feature = "eternal-fixtures"))]
mod tests {
    use super::*;
    use crate::{
        ids::PlayerId,
        policy_observation_v6::tests::{put, ready_state_for},
        state::{StackItem, StackStateV4, Zone},
    };

    #[test]
    fn issue454_stack_program_distinguishes_two_wastescape_cast_triggers() {
        let mut state = ready_state_for(PlayerId::P0);
        let source = put(
            &mut state,
            PlayerId::P0,
            "Wastescape Battlemage",
            Zone::Stack,
        );
        let printing = state.objects.get(source).card_def;
        let triggers = crate::trigger::triggers_for(printing);
        assert_eq!(triggers.len(), 2);
        let mut outputs = Vec::new();
        for (index, trigger) in triggers.iter().enumerate() {
            state.stack = vec![StackItem {
                kind: StackItemKind::TriggeredAbility,
                source,
                controller: PlayerId::P0,
                targets: Vec::new(),
                is_copy: false,
                inline_effect: Some((trigger.effect)()),
                discarded: Vec::new(),
                is_flashback: false,
                mode_chosen: 0,
                madness_offer: false,
                kicked: true,
                v4: StackStateV4::default(),
            }];
            let view = StackItemPublicV2 {
                stack_index: 0,
                source: crate::rl::card_ref(&state, source).unwrap(),
                controller: PlayerId::P0.into(),
                targets: Vec::new(),
                stack_item_kind: StackItemKind::TriggeredAbility.into(),
                is_copy: false,
                is_flashback: false,
                mode_chosen: 0,
                madness_offer: false,
                kicked: true,
                cast_method: None,
                face_index: 0,
                x_value: 0,
                paid_cost_refs: Vec::new(),
                counter_distribution: None,
                counter_transfer: None,
                granted_ability: None,
            };
            let projected = capture_v1(&state, &[view.clone()]).unwrap();
            assert_eq!(
                projected[0].origin,
                ProgramOriginV1::PrintedTrigger {
                    trigger_index: index as u32
                }
            );
            // Only the frozen public source supplies the printing. A changed
            // hidden current definition is not consulted by this binder.
            let mut hidden = state.clone();
            hidden.objects.get_mut(source).zone = Zone::Hand;
            hidden.objects.get_mut(source).zone_change_count += 1;
            hidden.objects.get_mut(source).card_def =
                crate::card_def::card_id_by_name("Island").unwrap();
            assert_eq!(capture_v1(&hidden, &[view]).unwrap(), projected);
            outputs.push(projected);
        }
        assert_ne!(outputs[0], outputs[1]);
    }
}
