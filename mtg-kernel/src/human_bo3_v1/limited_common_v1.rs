//! Safe labels only for the separate actor-facing Limited successor.
use super::*;
use crate::akroma_public_v1::AkromaObservationV1;
use crate::rl::ActionSemanticV1;
use visible::Handles;
use visible::Project;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitedCombatAssignmentV1 {
    pub source: HumanCardRefV1,
    pub recipient: HumanTargetV1,
    pub amount: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitedCombatV1 {
    pub phase: String,
    pub assignments: Vec<LimitedCombatAssignmentV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitedCommonDecisionV1 {
    pub schema: String,
    pub actor: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanAkromaExtensionsV40>,
    pub london_mulligans: Option<crate::rl::PublicLondonMulligansV1>,
    pub foundations_combat: Option<LimitedCombatV1>,
    pub actions: Vec<HumanLegalActionV1>,
}

pub(crate) fn project_limited_common_v1(
    observation: &AkromaObservationV1,
    actions: &[ActionSemanticV1],
    actor: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
    prompt_seq: u64,
) -> Result<(LimitedCommonDecisionV1, Vec<u32>), HumanDecisionErrorV1> {
    let (state, actions, indexes, mut handles) =
        akroma_v40::project_limited_with_handles(observation, actions, actor, families)?;
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
        LimitedCommonDecisionV1 {
            schema: "mtg-kernel-limited-common-decision/v1".into(),
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

pub(super) fn label(
    action: &ActionSemanticV1,
    handles: &Handles,
    actor: PlayerSeatV1,
) -> Result<Option<String>, HumanDecisionErrorV1> {
    use ActionSemanticV1 as A;
    let (owner, label) = match action {
        A::ChooseLondonMulligan {
            actor,
            mulligan_count,
            mulligan,
        } => (
            *actor,
            if *mulligan {
                format!(
                    "Mulligan: shuffle this hand and draw seven ({} cards to bottom if kept)",
                    u16::from(*mulligan_count) + 1
                )
            } else {
                format!("Keep this hand ({} cards to bottom)", mulligan_count)
            },
        ),
        A::ChooseLondonBottom {
            actor,
            remaining,
            card,
        } => (
            *actor,
            format!(
                "Put {} on the bottom of your library ({} remaining)",
                handles.name(card)?,
                remaining
            ),
        ),
        A::ChooseLegendPermanent { actor, keep, .. } => (
            *actor,
            format!("Keep {} for the legend rule", handles.name(keep)?),
        ),
        A::ChooseCombatDamageRange {
            actor,
            source,
            recipient,
            minimum,
            maximum,
            split_at,
            upper_half,
        } => {
            let (low, high) = if *upper_half {
                (
                    split_at
                        .checked_add(1)
                        .ok_or(HumanDecisionErrorV1::InvalidAction)?,
                    *maximum,
                )
            } else {
                (*minimum, *split_at)
            };
            if low > high || *minimum > *maximum {
                return Err(HumanDecisionErrorV1::InvalidAction);
            }
            (
                *actor,
                format!(
                    "Assign {} damage to {}: select range {} to {}",
                    handles.name(source)?,
                    labels::target(recipient, handles, *actor)?,
                    low,
                    high
                ),
            )
        }
        A::ChooseAttackTarget {
            actor,
            attacker,
            target,
        } => (
            *actor,
            format!(
                "Attack {} with {}",
                labels::target(target, handles, *actor)?,
                handles.name(attacker)?
            ),
        ),
        _ => return Ok(None),
    };
    if owner != actor {
        return Err(HumanDecisionErrorV1::NotHumanTurn);
    }
    Ok(Some(label))
}
