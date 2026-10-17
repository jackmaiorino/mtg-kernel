//! Separate schema-15 limited transport projection for the owning Chandra V74 decision.
use super::chandra_v74::HumanChandraCandidateV74;
use super::limited_common_v1::{LimitedCombatAssignmentV1, LimitedCombatV1};
use super::*;
use crate::rl_session::ChandraDecisionV74;
use visible::Project;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitedChandraDecisionV15 {
    pub schema: String,
    pub actor: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateChandraV74,
    pub london_mulligans: Option<crate::rl::PublicLondonMulligansV1>,
    pub foundations_combat: Option<LimitedCombatV1>,
    pub actions: Vec<HumanLegalActionV1>,
    pub action_meanings: Vec<HumanChandraCandidateV74>,
}

pub(crate) fn project_limited_chandra_v15(
    decision: &ChandraDecisionV74,
    actor: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(LimitedChandraDecisionV15, Vec<u32>), HumanDecisionErrorV1> {
    let observation = decision.observation();
    let (projected, indexes, mut handles) =
        super::chandra_v74::project_with_handles(decision, actor, prompt_seq)?;
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
        LimitedChandraDecisionV15 {
            schema: "mtg-kernel-limited-chandra-decision/v15".into(),
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
