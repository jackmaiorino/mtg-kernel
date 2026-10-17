//! Separate schema-16 limited transport projection for the owning Kaito V75 decision.
use super::kaito_v75::HumanKaitoCandidateV75;
use super::limited_common_v1::{LimitedCombatAssignmentV1, LimitedCombatV1};
use super::*;
use crate::rl_session::KaitoDecisionV75;
use visible::Project;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitedKaitoDecisionV16 {
    pub schema: String,
    pub actor: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateKaitoV75,
    pub london_mulligans: Option<crate::rl::PublicLondonMulligansV1>,
    pub foundations_combat: Option<LimitedCombatV1>,
    pub actions: Vec<HumanLegalActionV1>,
    pub action_meanings: Vec<HumanKaitoCandidateV75>,
}

pub(crate) fn project_limited_kaito_v16(
    decision: &KaitoDecisionV75,
    actor: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(LimitedKaitoDecisionV16, Vec<u32>), HumanDecisionErrorV1> {
    let observation = decision.observation();
    let (projected, indexes, mut handles) =
        super::kaito_v75::project_with_handles(decision, actor, prompt_seq)?;
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
        LimitedKaitoDecisionV16 {
            schema: "mtg-kernel-limited-kaito-decision/v16".into(),
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
