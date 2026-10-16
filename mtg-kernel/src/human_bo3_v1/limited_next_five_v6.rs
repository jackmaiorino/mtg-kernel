//! Separate schema-6 limited transport projection for the owning NextFive V59 decision.
use super::*;
use crate::rl_session::NextFiveDecisionV59;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitedNextFiveDecisionV6 {
    pub schema: String,
    pub actor: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateNextFiveV59,
    pub actions: Vec<HumanLegalActionV1>,
    pub action_meanings: Vec<Option<HumanNextFiveActivationV59>>,
}

pub(crate) fn project_limited_next_five_v6(
    decision: &NextFiveDecisionV59,
    actor: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(LimitedNextFiveDecisionV6, Vec<u32>), HumanDecisionErrorV1> {
    let (projected, indexes) = super::next_five_v59::project(decision, actor, prompt_seq)?;
    Ok((
        LimitedNextFiveDecisionV6 {
            schema: "mtg-kernel-limited-next-five-decision/v6".into(),
            actor,
            prompt_seq,
            state: projected.state,
            actions: projected.actions,
            action_meanings: projected.action_meanings,
        },
        indexes,
    ))
}
