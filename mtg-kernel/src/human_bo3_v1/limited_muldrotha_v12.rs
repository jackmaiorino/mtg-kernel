//! Separate schema-12 limited transport projection for the owning Genesis Wave V71 decision.
use super::limited_common_v1::{LimitedCombatAssignmentV1, LimitedCombatV1};
use super::muldrotha_v71::HumanMuldrothaCandidateV71;
use super::*;
use crate::rl_session::MuldrothaDecisionV71;
use visible::Project;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitedMuldrothaDecisionV12 {
    pub schema: String,
    pub actor: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateMuldrothaV71,
    pub london_mulligans: Option<crate::rl::PublicLondonMulligansV1>,
    pub foundations_combat: Option<LimitedCombatV1>,
    pub actions: Vec<HumanLegalActionV1>,
    pub action_meanings: Vec<HumanMuldrothaCandidateV71>,
}

pub(crate) fn project_limited_muldrotha_v12(
    decision: &MuldrothaDecisionV71,
    actor: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(LimitedMuldrothaDecisionV12, Vec<u32>), HumanDecisionErrorV1> {
    let observation = decision.observation();
    let (projected, indexes, mut handles) =
        super::muldrotha_v71::project_with_handles(decision, actor, prompt_seq)?;
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
        LimitedMuldrothaDecisionV12 {
            schema: "mtg-kernel-limited-muldrotha-decision/v12".into(),
            actor,
            prompt_seq,
            state: projected.state,
            london_mulligans: observation
                .common
                .common
                .projection
                .london_mulligans
                .clone(),
            foundations_combat,
            actions: projected.actions,
            action_meanings: projected.action_meanings,
        },
        indexes,
    ))
}
