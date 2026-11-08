//! Bind an already public stack row to its particular printed program.
//! Neither private saved continuations nor arbitrary inline EffectOps are
//! serialized. Unrecognized dynamic/granted programs refuse for now.
use crate::{
    card_def::CARD_DEFS,
    effect::EffectOp,
    rl::{RlContractError, StackItemPublicV2},
    state::{GameState, StackItem, StackItemKind},
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StackProgramV1 {
    pub stack_index: u32,
    pub origin: ProgramOriginV1,
    pub program: ProgramV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ProgramV1 {
    Printed(Box<EffectOp>),
    CounterCapturedCastSpell {
        /// Public stack ordinal, or absent after that exact spell has left.
        target_stack_index: Option<u32>,
        respects_cannot_be_countered: bool,
    },
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
            if view.stack_index as usize != index {
                return Err(error());
            }
            bind_item(state, item, view)
        })
        .collect()
}

fn bind_item(
    state: &GameState,
    item: &StackItem,
    view: &StackItemPublicV2,
) -> Result<StackProgramV1, RlContractError> {
    if view.source.arena_id != item.source.0
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
        StackItemKind::Spell if item.inline_effect.is_none() && item.v4.face_index == 0 => (
            ProgramOriginV1::SpellMode {
                mode: item.mode_chosen,
            },
            ProgramV1::Printed(Box::new(
                def.printed_mode_effect(item.mode_chosen)
                    .ok_or_else(error)?,
            )),
        ),
        StackItemKind::ActivatedAbility => {
            let ability_index = item.v4.activated_ability_index.ok_or_else(error)?;
            let ability = def
                .activated_abilities
                .get(usize::from(ability_index))
                .ok_or_else(error)?;
            let program = (ability.effect)();
            if item.inline_effect.as_ref() != Some(&program) || item.v4.granted_by.is_some() {
                return Err(error());
            }
            (
                ProgramOriginV1::PrintedActivation { ability_index },
                ProgramV1::Printed(Box::new(program)),
            )
        }
        StackItemKind::TriggeredAbility => {
            let program = item.inline_effect.as_ref().ok_or_else(error)?;
            let (trigger_index, program) = bind_trigger(state, printing, program)?;
            (
                ProgramOriginV1::PrintedTrigger {
                    trigger_index: trigger_index as u32,
                },
                program,
            )
        }
        _ => return Err(error()),
    };
    Ok(StackProgramV1 {
        stack_index: view.stack_index,
        origin,
        program,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingProgramV1 {
    /// Complete public frozen source/target/cast metadata. The index denotes
    /// this pending sequence, not a newly allocated native stack incarnation.
    pub item: StackItemPublicV2,
    pub meaning: StackProgramV1,
}

/// Complete printed-program identity while a public trigger waits for ordering
/// or targeting. The supplied ordinary observation authenticates the public
/// source; the private native trigger is never serialized wholesale.
pub fn capture_pending_v1(
    state: &GameState,
    viewer: crate::ids::PlayerId,
    public: &[crate::rl::PendingTriggerSemanticV2],
) -> Result<Vec<PendingProgramV1>, RlContractError> {
    let pending = state
        .engine
        .pending_triggers
        .iter()
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|pending| pending.waiting_triggers()),
        )
        .collect::<Vec<_>>();
    if pending.len() != public.len() {
        return Err(error());
    }
    pending
        .into_iter()
        .zip(public)
        .enumerate()
        .map(|(index, (pending, observed))| {
            crate::engine::validate_pending_trigger_identity(state, pending)
                .map_err(RlContractError)?;
            if pending.is_madness_offer || pending.granted_by.is_some() {
                return Err(error());
            }
            let item = crate::engine::pending_trigger_stack_candidate(state, pending)
                .map_err(RlContractError)?;
            let view = crate::rl::stack_item_public_v2_from_frozen(
                state,
                viewer,
                u32::try_from(index).map_err(|_| error())?,
                &item,
            )?;
            if observed.source.as_ref() != Some(&view.source)
                || observed.controller != view.controller
                || observed.kicked != view.kicked
            {
                return Err(error());
            }
            let meaning = bind_item(state, &item, &view)?;
            Ok(PendingProgramV1 {
                item: view,
                meaning,
            })
        })
        .collect()
}

/// A resolving source has already been announced. Publish only its frozen
/// stack metadata and exact admitted printed program, never saved frames or
/// ExecCtx (which may contain private selections and native identities).
pub fn capture_resolving_v1(
    state: &GameState,
    viewer: crate::ids::PlayerId,
    public: Option<&crate::rl::PendingEffectSemanticV4>,
) -> Result<Option<PendingProgramV1>, RlContractError> {
    let Some(pending) = state.engine.pending_effect.as_ref() else {
        return Ok(None);
    };
    crate::effect::validate_pending_effect_choice(state).map_err(RlContractError)?;
    let observed = public.ok_or_else(error)?;
    let item = &pending.resolving_item;
    let view = crate::rl::stack_item_public_v2_from_frozen(state, viewer, 0, item)?;
    if observed.source.as_ref() != Some(&view.source) || observed.controller != view.controller {
        return Err(error());
    }
    let meaning = bind_item(state, item, &view)?;
    Ok(Some(PendingProgramV1 {
        item: view,
        meaning,
    }))
}

fn bind_trigger(
    state: &GameState,
    printing: u16,
    program: &EffectOp,
) -> Result<(usize, ProgramV1), RlContractError> {
    use crate::{
        eternal_cards_v1::EternalOpV1 as E, eternal_vintage_v1::VintageOpV1 as V,
        standard_cards_v1::StandardOpV1 as S,
    };
    let triggers = crate::trigger::triggers_for(printing);
    if let Some(index) = triggers
        .iter()
        .position(|trigger| (trigger.effect)() == *program)
    {
        return Ok((index, ProgramV1::Printed(Box::new(program.clone()))));
    }
    // Vexing Bauble and Chalice materialize the publicly observed cast into
    // an exact stack incarnation. Publish its stack position, never its raw ID.
    if let EffectOp::StandardV1(S::EternalV1(E::VintageV1(V::CounterCastSpellV1 { spell }))) =
        program
    {
        let template = EffectOp::StandardV1(S::EternalV1(E::VintageV1(V::BindCounterCastSpellV1)));
        let index = triggers
            .iter()
            .position(|trigger| (trigger.effect)() == template)
            .ok_or_else(error)?;
        let target_stack_index = state
            .stack
            .iter()
            .position(|item| item.v4.stack_item_id == *spell)
            .map(|index| u32::try_from(index).map_err(|_| error()))
            .transpose()?;
        return Ok((
            index,
            ProgramV1::CounterCapturedCastSpell {
                target_stack_index,
                respects_cannot_be_countered: true,
            },
        ));
    }
    Err(error())
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

    fn stack_source(state: &mut GameState, name: &str) -> crate::ids::ObjectId {
        let source = put(state, PlayerId::P0, name, Zone::Hand);
        state.players[PlayerId::P0.index()]
            .hand
            .retain(|id| *id != source);
        state.objects.get_mut(source).zone = Zone::Stack;
        source
    }

    #[test]
    fn issue454_pending_program_binds_printed_trigger_without_hidden_hand() {
        for viewer in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(viewer);
            let source = put(
                &mut state,
                viewer,
                "Atraxa, Grand Unifier",
                Zone::Battlefield,
            );
            let hidden = put(&mut state, viewer.opponent(), "Island", Zone::Hand);
            let printing = state.objects.get(source).card_def;
            let triggers = crate::trigger::triggers_for(printing);
            assert_eq!(triggers.len(), 1);
            state
                .engine
                .pending_triggers
                .push(crate::eternal_cards_v1::delayed_trigger(
                    crate::state::AbilitySourceContractV4::capture(&state, source),
                    viewer,
                    (triggers[0].effect)(),
                ));
            let observed = crate::rl::PendingTriggerSemanticV2 {
                source: Some(crate::rl::card_ref(&state, source).unwrap()),
                controller: viewer.into(),
                trigger_kind: crate::rl::PendingTriggerKindV2::TriggeredAbility,
                kicked: false,
                counter_distribution: None,
                counter_transfer: None,
            };
            let baseline = capture_pending_v1(&state, viewer, &[observed.clone()]).unwrap();
            assert_eq!(
                baseline[0].meaning.origin,
                ProgramOriginV1::PrintedTrigger { trigger_index: 0 }
            );
            for name in ["Swamp", "Force of Will"] {
                state.objects.get_mut(hidden).card_def =
                    crate::card_def::card_id_by_name(name).unwrap();
                assert_eq!(
                    capture_pending_v1(&state, viewer, &[observed.clone()]).unwrap(),
                    baseline
                );
            }
            state.engine.pending_triggers[0].effect = EffectOp::Sequence(Vec::new());
            assert!(capture_pending_v1(&state, viewer, &[observed]).is_err());
        }
    }

    #[test]
    fn issue454_bound_counter_uses_public_stack_position_and_not_native_id() {
        use crate::{
            eternal_cards_v1::EternalOpV1 as E, eternal_vintage_v1::VintageOpV1 as V,
            ids::StackItemId, standard_cards_v1::StandardOpV1 as S,
        };
        let mut state = ready_state_for(PlayerId::P0);
        let source = stack_source(&mut state, "Ponder");
        let counter = |id| {
            EffectOp::StandardV1(S::EternalV1(E::VintageV1(V::CounterCastSpellV1 {
                spell: StackItemId(id),
            })))
        };
        for name in ["Vexing Bauble", "Chalice of the Void"] {
            let printing = crate::card_def::card_id_by_name(name).unwrap();
            let absent = bind_trigger(&state, printing, &counter(51)).unwrap();
            assert_eq!(
                absent,
                bind_trigger(&state, printing, &counter(999)).unwrap()
            );
            let mut observed = Vec::new();
            for id in [51, 999] {
                state.stack = vec![StackItem {
                    kind: StackItemKind::Spell,
                    source,
                    controller: PlayerId::P0,
                    targets: Vec::new(),
                    is_copy: false,
                    inline_effect: None,
                    discarded: Vec::new(),
                    is_flashback: false,
                    mode_chosen: 0,
                    madness_offer: false,
                    kicked: false,
                    v4: StackStateV4 {
                        stack_item_id: StackItemId(id),
                        ..Default::default()
                    },
                }];
                observed.push(bind_trigger(&state, printing, &counter(id)).unwrap());
            }
            assert_eq!(observed[0], observed[1]);
            assert_ne!(observed[0], absent);
            state.stack.clear();
        }
    }

    #[test]
    fn issue454_stack_program_distinguishes_two_wastescape_cast_triggers() {
        let mut state = ready_state_for(PlayerId::P0);
        let source = stack_source(&mut state, "Wastescape Battlemage");
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
