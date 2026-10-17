//! Complete Elenda V52 safe facts for the separate actor-facing schema-4 transport.
use super::*;
use crate::rl_session::ElendaDecisionV52;
use visible::Project;

use super::elenda_v52::HumanVisibleStateElendaV52;
use super::limited_common_v1::{LimitedCombatAssignmentV1, LimitedCombatV1};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitedElendaDecisionV4 {
    pub schema: String,
    pub actor: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateElendaV52,
    pub london_mulligans: Option<crate::rl::PublicLondonMulligansV1>,
    pub foundations_combat: Option<LimitedCombatV1>,
    pub actions: Vec<HumanLegalActionV1>,
}

pub(crate) fn project_limited_elenda_v4(
    decision: &ElendaDecisionV52,
    actor: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(LimitedElendaDecisionV4, Vec<u32>), HumanDecisionErrorV1> {
    let observation = decision.observation();
    let (predecessor, actions, indexes, mut handles) =
        ravenous_v51::project_elenda_base_with_handles(
            &observation.common,
            decision.actions(),
            actor,
            decision.family_projections_v52(),
            &observation.elenda,
        )?;
    let elenda = elenda_v52::facts(&observation.elenda, &handles)?;
    let state = HumanVisibleStateElendaV52 {
        predecessor,
        elenda,
    };
    let foundations_combat = observation
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
        LimitedElendaDecisionV4 {
            schema: "mtg-kernel-limited-elenda-decision/v4".into(),
            actor,
            prompt_seq,
            state,
            actions,
            london_mulligans: observation.common.projection.london_mulligans.clone(),
            foundations_combat,
        },
        indexes,
    ))
}
