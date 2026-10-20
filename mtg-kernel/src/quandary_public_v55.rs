//! Current Quandary roots. Exact hand/answer data is confined to its payer.
use crate::{
    effect::{EffectFrame, EffectTargetSelectionPurpose, PendingEffectChoice},
    ids::PlayerId,
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    rules_meaning_quandary_v55 as meaning,
    state::{GameState, StackItemKind},
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "mtg-kernel/quandary-public/v55";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateHandV55 {
    pub original_hand: Vec<public::ReferenceV1>,
    pub selected: Vec<public::ReferenceV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum StageV55 {
    OnStack,
    AwaitingPayment {
        payer: PlayerSeatV1,
        minimum: u16,
        maximum: u16,
        payer_hand: Option<PrivateHandV55>,
    },
    Answered {
        payer: PlayerSeatV1,
        payer_selected: Option<Vec<public::ReferenceV1>>,
    },
    PrintedUnpaidContinuation {
        payer: PlayerSeatV1,
        life_loss: i32,
    },
    InitialPrintedContinuation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StackV55 {
    pub stack_position: usize,
    pub root: public::RootV1,
    pub stage: StageV55,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV55 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meaning: meaning::MeaningV55,
    pub current: Vec<public::CurrentV1>,
    pub held: Vec<public::RootV1>,
    pub stack: Vec<StackV55>,
}
fn refs(
    state: &GameState,
    viewer: PlayerId,
    bindings: impl IntoIterator<Item = crate::effect::EffectObjectBinding>,
) -> Result<Vec<public::ReferenceV1>, String> {
    bindings
        .into_iter()
        .map(|b| {
            public::reference(state, b.object, Some(viewer))
                .ok_or_else(|| "payer hand reference not authorized".into())
        })
        .collect()
}
pub fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV55, String> {
    capture_scoped(state, viewer, public::CounterScopeV61::Legacy)
}
pub(crate) fn capture_complete_v61(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV55, String> {
    capture_scoped(state, viewer, public::CounterScopeV61::CompleteV61)
}
pub(crate) fn capture_complete_call_v62(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV55, String> {
    capture_scoped(state, viewer, public::CounterScopeV61::CallV62)
}
pub(crate) fn capture_tinybones_compat_v72(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV55, String> {
    capture_scoped(state, viewer, public::CounterScopeV61::TinybonesV72)
}
pub(crate) fn capture_kaito_compat_v75(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV55, String> {
    capture_for_consumer(
        state,
        viewer,
        public::CounterScopeV61::TinybonesV72,
        crate::optional_trigger_public_v1::ConsumerV1::KaitoV75,
    )
}
fn capture_scoped(
    state: &GameState,
    viewer: PlayerId,
    scope: public::CounterScopeV61,
) -> Result<FactsV55, String> {
    capture_for_consumer(
        state,
        viewer,
        scope,
        crate::optional_trigger_public_v1::ConsumerV1::Legacy,
    )
}
fn capture_for_consumer(
    state: &GameState,
    viewer: PlayerId,
    scope: public::CounterScopeV61,
    consumer: crate::optional_trigger_public_v1::ConsumerV1,
) -> Result<FactsV55, String> {
    let id =
        crate::card_def::card_id_by_name("Painful Quandary").ok_or("Quandary printing absent")?;
    let meaning = meaning::printed_meaning(id)?;
    public::validate_state_for_consumer(state, false, consumer)?;
    let mut held = Vec::new();
    for p in public::held(state) {
        if let Some(source) = p.source_contract.filter(|s| s.card_def == id) {
            held.push(public::root(state, source, Some(viewer))?);
        }
    }
    let mut stack = Vec::new();
    for (stack_position, item) in state.stack.iter().enumerate() {
        if item.kind != StackItemKind::TriggeredAbility {
            continue;
        }
        let Some(source) = item.v4.ability_source_contract.filter(|s| s.card_def == id) else {
            continue;
        };
        let payer = item.controller.opponent();
        let mut stage = StageV55::OnStack;
        if let Some(p) = state
            .engine
            .pending_effect
            .as_ref()
            .filter(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
        {
            stage = match p.choice.as_ref() {
                Some(PendingEffectChoice::SelectTargets {
                    selected,
                    purpose: EffectTargetSelectionPurpose::DiscardPaymentV1 { choice },
                    ..
                }) => {
                    let selected_bindings = selected
                        .iter()
                        .map(|candidate| {
                            candidate.expected_object.ok_or_else(|| {
                                "Quandary selected candidate lacks its exact binding".to_string()
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    StageV55::AwaitingPayment {
                        payer: payer.into(),
                        minimum: 0,
                        maximum: 1,
                        payer_hand: if viewer == payer {
                            Some(PrivateHandV55 {
                                original_hand: refs(state, viewer, choice.hand.iter().copied())?,
                                selected: refs(state, viewer, selected_bindings)?,
                            })
                        } else {
                            None
                        },
                    }
                }
                None => match p.frames.as_slice() {
                    [EffectFrame::DiscardPaymentV1 { selected, .. }] => StageV55::Answered {
                        payer: payer.into(),
                        payer_selected: if viewer == payer {
                            Some(refs(state, viewer, selected.iter().copied())?)
                        } else {
                            None
                        },
                    },
                    [EffectFrame::Program { path, .. }] if path.as_slice() == [0] => {
                        StageV55::PrintedUnpaidContinuation {
                            payer: payer.into(),
                            life_loss: 5,
                        }
                    }
                    [EffectFrame::Program { path, .. }] if path.is_empty() => {
                        StageV55::InitialPrintedContinuation
                    }
                    _ => return Err("Quandary continuation stage changed".into()),
                },
                _ => return Err("Quandary choice stage changed".into()),
            };
        }
        stack.push(StackV55 {
            stack_position,
            root: public::root(state, source, Some(viewer))?,
            stage,
        });
    }
    Ok(FactsV55 {
        schema: SCHEMA.into(),
        producer_sha256: meaning::producer_sha256(),
        viewer: viewer.into(),
        meaning,
        current: if scope == public::CounterScopeV61::TinybonesV72 {
            public::current_tinybones_compat_v72(state, id)?
        } else if scope == public::CounterScopeV61::CallV62 {
            public::current_complete_call_v62(state, id)?
        } else if scope == public::CounterScopeV61::CompleteV61 {
            public::current_complete_v61(state, id)?
        } else {
            public::current(state, id)?
        },
        held,
        stack,
    })
}
pub fn validate(state: &GameState, viewer: PlayerId, value: &FactsV55) -> Result<(), String> {
    if capture(state, viewer)? == *value {
        Ok(())
    } else {
        Err("Quandary public facts changed".into())
    }
}
