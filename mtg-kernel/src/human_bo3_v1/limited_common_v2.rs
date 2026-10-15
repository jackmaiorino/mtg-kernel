//! Complete V47 safe facts for the separate actor-facing Limited V2 successor.
use super::*;
use crate::panda_observation_v47::PandaObservationV47;
use crate::rl::ActionSemanticV1;
use visible::Project;

use super::limited_common_v1::{LimitedCombatAssignmentV1, LimitedCombatV1};
use super::panda_v47::HumanPandaExtensionsV47;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitedCommonDecisionV2 {
    pub schema: String,
    pub actor: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanPandaExtensionsV47>,
    pub london_mulligans: Option<crate::rl::PublicLondonMulligansV1>,
    pub foundations_combat: Option<LimitedCombatV1>,
    pub actions: Vec<HumanLegalActionV1>,
}

pub(crate) fn project_limited_common_v2(
    observation: &PandaObservationV47,
    actions: &[ActionSemanticV1],
    actor: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
    prompt_seq: u64,
) -> Result<(LimitedCommonDecisionV2, Vec<u32>), HumanDecisionErrorV1> {
    let (state, actions, indexes, mut handles) =
        panda_v47::project_base_with_handles(observation, actions, actor, families)?;
    let foundations_combat = observation
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
        LimitedCommonDecisionV2 {
            schema: "mtg-kernel-limited-common-decision/v2".into(),
            actor,
            prompt_seq,
            state,
            actions,
            london_mulligans: observation.projection.london_mulligans.clone(),
            foundations_combat,
        },
        indexes,
    ))
}
