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
    let id =
        crate::card_def::card_id_by_name("Spinner of Souls").ok_or("Spinner printing absent")?;
    let meaning = meaning::printed_meaning(id)?;
    public::validate_state(state)?;
    let mut held = Vec::new();
    for pending in public::held(state) {
        if let Some(source) = pending.source_contract.filter(|s| s.card_def == id) {
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
        current: public::current(state, id)?,
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
