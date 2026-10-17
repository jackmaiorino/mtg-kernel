//! Complete Prepared V57 safe facts for the separate actor-facing schema-5 transport.
use super::*;
use crate::rl_session::PreparedDecisionV57;
use visible::Project;

use super::limited_common_v1::{LimitedCombatAssignmentV1, LimitedCombatV1};
use super::prepared_v57::HumanVisibleStatePreparedV57;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitedPreparedDecisionV5 {
    pub schema: String,
    pub actor: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStatePreparedV57,
    pub london_mulligans: Option<crate::rl::PublicLondonMulligansV1>,
    pub foundations_combat: Option<LimitedCombatV1>,
    pub actions: Vec<HumanLegalActionV1>,
    pub action_meanings: Vec<Option<super::prepared_v57::HumanPreparedActivationV57>>,
}

pub(crate) fn project_limited_prepared_v5(
    decision: &PreparedDecisionV57,
    actor: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(LimitedPreparedDecisionV5, Vec<u32>), HumanDecisionErrorV1> {
    let observation = decision.observation();
    let (state, actions, indexes, mut handles, action_meanings) =
        super::prepared_v57::project_with_handles(decision, actor)?;
    let foundations_combat = observation
        .common
        .common
        .projection
        .foundations_combat
        .as_ref()
        .map(|combat| {
            let assignments = combat
                .assignments
                .iter()
                .map(|assignment| {
                    Ok(LimitedCombatAssignmentV1 {
                        source: handles.get(&assignment.source)?.clone(),
                        recipient: assignment.recipient.project(&mut handles)?,
                        amount: assignment.amount,
                    })
                })
                .collect::<Result<Vec<_>, HumanDecisionErrorV1>>()?;
            Ok::<LimitedCombatV1, HumanDecisionErrorV1>(LimitedCombatV1 {
                phase: combat.phase.clone(),
                assignments,
            })
        })
        .transpose()?;
    Ok((
        LimitedPreparedDecisionV5 {
            schema: "mtg-kernel-limited-prepared-decision/v5".into(),
            actor,
            prompt_seq,
            state,
            actions,
            action_meanings,
            london_mulligans: observation
                .common
                .common
                .projection
                .london_mulligans
                .clone(),
            foundations_combat,
        },
        indexes,
    ))
}
