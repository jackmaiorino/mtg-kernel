//! Current validated Spinner roots. No reconstructed or completed occurrences.
use crate::{
    effect::{EffectFrame, PendingEffectChoice},
    optional_trigger_public_v1 as public, rules_meaning_spinner_v54 as meaning,
    state::{GameState, StackItemKind},
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "mtg-kernel/spinner-public/v54";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum StageV54 {
    OnStack,
    AwaitingReveal,
    Answered { reveal: bool },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StackV54 {
    pub stack_position: usize,
    pub root: public::RootV1,
    pub stage: StageV54,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV54 {
    pub schema: String,
    pub producer_sha256: String,
    pub meaning: meaning::MeaningV54,
    pub current: Vec<public::CurrentV1>,
    pub held: Vec<public::RootV1>,
    pub stack: Vec<StackV54>,
}
pub fn capture(state: &GameState) -> Result<FactsV54, String> {
    capture_scoped(state, public::CounterScopeV61::Legacy)
}
pub(crate) fn capture_complete_v61(state: &GameState) -> Result<FactsV54, String> {
    capture_scoped(state, public::CounterScopeV61::CompleteV61)
}
pub(crate) fn capture_complete_call_v62(state: &GameState) -> Result<FactsV54, String> {
    capture_scoped(state, public::CounterScopeV61::CallV62)
}
pub(crate) fn capture_tinybones_compat_v72(state: &GameState) -> Result<FactsV54, String> {
    capture_scoped(state, public::CounterScopeV61::TinybonesV72)
}
fn capture_scoped(state: &GameState, scope: public::CounterScopeV61) -> Result<FactsV54, String> {
    let id =
        crate::card_def::card_id_by_name("Spinner of Souls").ok_or("Spinner printing absent")?;
    let meaning = meaning::printed_meaning(id)?;
    public::validate_state(state)?;
    let printed_root = (crate::trigger::triggers_for(id)[0].effect)();
    let mut held = Vec::new();
    for pending in public::held(state) {
        if let Some(source) = pending.source_contract.filter(|s| s.card_def == id) {
            // A host's printing does not turn an independently granted root
            // (for example Rod damage) into its printed death trigger.
            if pending.granted_by.is_some() || pending.effect != printed_root {
                continue;
            }
            held.push(public::root(state, source, None)?);
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
        if item.v4.granted_by.is_some() || item.inline_effect.as_ref() != Some(&printed_root) {
            continue;
        }
        let mut stage = StageV54::OnStack;
        if let Some(p) = state
            .engine
            .pending_effect
            .as_ref()
            .filter(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
        {
            stage = match p.choice.as_ref() {
                Some(PendingEffectChoice::ChooseOption { .. }) => StageV54::AwaitingReveal,
                None => {
                    let [EffectFrame::Program { path, .. }] = p.frames.as_slice() else {
                        return Err("Spinner answered stage changed".into());
                    };
                    let [option] = path.as_slice() else {
                        return Err("Spinner answer path changed".into());
                    };
                    StageV54::Answered {
                        reveal: *option == 1,
                    }
                }
                _ => return Err("Spinner choice stage changed".into()),
            };
        }
        stack.push(StackV54 {
            stack_position,
            root: public::root(state, source, None)?,
            stage,
        });
    }
    Ok(FactsV54 {
        schema: SCHEMA.into(),
        producer_sha256: meaning::producer_sha256(),
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
pub fn validate(state: &GameState, value: &FactsV54) -> Result<(), String> {
    if capture(state)? == *value {
        Ok(())
    } else {
        Err("Spinner public facts changed".into())
    }
}
