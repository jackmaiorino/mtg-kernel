//! Fixed-seat Elenda V52 Human projection. Backend decisions remain sealed.
use super::visible::Handles;
use super::*;
use crate::rl::{ActionSemanticV1, RlContractError};
use crate::rl_session::{
    ElendaDecisionV52, ElendaSessionV52, ElendaSnapshotV52, FastActorResponseV1,
};
use crate::{elenda_public_v52 as public, rules_meaning_elenda_v52 as meaning};
use std::{collections::BTreeMap, sync::Arc};
type Error = HumanDecisionErrorV1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanElendaMeaningV52 {
    pub root: meaning::RootV52,
    pub printed: meaning::PrintedCreatureV52,
    pub home: meaning::HomeV52,
    pub active_printed_ability_required: bool,
    pub evaluation: meaning::EvaluationV52,
    pub supported_starting_life: i32,
    pub bonus_combination: meaning::CombinationV52,
    pub ordered_rules: Vec<meaning::RuleV52>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanElendaTraitsV52 {
    pub token: bool,
    pub effective_name: String,
    pub effective_types: Vec<crate::card_def::CardType>,
    pub effective_supertypes: Vec<meaning::SupertypeV52>,
    pub effective_subtype_ids: Vec<u16>,
    pub effective_color_mask: u8,
    pub starting_life: i32,
    pub controller_life: i32,
    pub printed_abilities_active: bool,
    pub ordered_bonuses: Vec<public::BonusV52>,
    pub applied_power_bonus: i32,
    pub applied_toughness_bonus: i32,
    pub effective_power: Option<i32>,
    pub effective_toughness: Option<i32>,
    pub effective_lifelink: bool,
    pub effective_menace: bool,
    pub hexproof_from_opponents_instants: bool,
}
impl From<&public::CurrentV52> for HumanElendaTraitsV52 {
    fn from(row: &public::CurrentV52) -> Self {
        Self {
            token: row.token,
            effective_name: row.effective_name.clone(),
            effective_types: row.effective_types.clone(),
            effective_supertypes: row.effective_supertypes.clone(),
            effective_subtype_ids: row.effective_subtype_ids.clone(),
            effective_color_mask: row.effective_color_mask,
            starting_life: row.starting_life,
            controller_life: row.controller_life,
            printed_abilities_active: row.printed_abilities_active,
            ordered_bonuses: row.ordered_bonuses.clone(),
            applied_power_bonus: row.applied_power_bonus,
            applied_toughness_bonus: row.applied_toughness_bonus,
            effective_power: row.effective_power,
            effective_toughness: row.effective_toughness,
            effective_lifelink: row.effective_lifelink,
            effective_menace: row.effective_menace,
            hexproof_from_opponents_instants: row.hexproof_from_opponents_instants,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanElendaCurrentV52 {
    pub source: HumanCardRefV1,
    pub traits: HumanElendaTraitsV52,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanElendaFactsV52 {
    pub meaning: HumanElendaMeaningV52,
    pub current: Vec<HumanElendaCurrentV52>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanVisibleStateElendaV52 {
    pub predecessor: super::ravenous_v51::HumanVisibleStateRavenousV51,
    pub elenda: HumanElendaFactsV52,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionElendaV52 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateElendaV52,
    pub actions: Vec<HumanLegalActionV1>,
}
pub(super) fn facts(
    facts: &public::FactsV52,
    handles: &Handles,
) -> Result<HumanElendaFactsV52, Error> {
    let rule = &facts.meaning;
    let mut current = facts
        .current
        .iter()
        .map(|row| {
            Ok(HumanElendaCurrentV52 {
                source: handles.get(&row.source)?.clone(),
                traits: row.into(),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    // Battlefield rows form a public set. Physical arena insertion order is
    // not a presentation identity after canonical handles are established.
    current.sort_by(|left, right| left.source.handle.cmp(&right.source.handle));
    Ok(HumanElendaFactsV52 {
        meaning: HumanElendaMeaningV52 {
            root: rule.root,
            printed: rule.printed.clone(),
            home: rule.home,
            active_printed_ability_required: rule.active_printed_ability_required,
            evaluation: rule.evaluation,
            supported_starting_life: rule.supported_starting_life,
            bonus_combination: rule.bonus_combination,
            ordered_rules: rule.ordered_rules.clone(),
        },
        current,
    })
}
pub(super) fn unary(
    facts: &public::FactsV52,
    handles: &Handles,
    traits: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for row in &facts.current {
        // Explicit semantic fields only. Arena IDs, generations and selectors
        // cannot decide prompt-local presentation handles.
        let text = serde_json::to_string(&HumanElendaTraitsV52::from(row))
            .map_err(|_| Error::InvalidVisibleReference)?;
        traits
            .entry(handles.get(&row.source)?.handle.clone())
            .or_default()
            .push(format!("elenda_v52:{text}"));
    }
    Ok(())
}
fn project(
    decision: &ElendaDecisionV52,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionElendaV52, Vec<u32>), Error> {
    let observation = decision.observation();
    let (predecessor, actions, indexes, handles) =
        super::ravenous_v51::project_elenda_base_with_handles(
            &observation.common,
            decision.actions(),
            human,
            decision.family_projections_v52(),
            &observation.elenda,
        )?;
    let elenda = facts(&observation.elenda, &handles)?;
    Ok((
        HumanDecisionElendaV52 {
            schema: "mtg-kernel-human-elenda-decision/v52".into(),
            human_seat: human,
            prompt_seq,
            state: HumanVisibleStateElendaV52 {
                predecessor,
                elenda,
            },
            actions,
        },
        indexes,
    ))
}
#[derive(Clone)]
struct AuthorityV52 {
    owner: Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV52 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV52 {}
pub struct HumanElendaSessionV52 {
    inner: ElendaSessionV52,
    authority: AuthorityV52,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanElendaSnapshotV52 {
    inner: ElendaSnapshotV52,
    owner: Arc<()>,
}
impl HumanElendaSessionV52 {
    pub fn from_session(inner: ElendaSessionV52, human_seat: PlayerSeatV1) -> Self {
        Self {
            inner,
            human_seat,
            authority: AuthorityV52 {
                owner: Arc::new(()),
                generation: 0,
            },
        }
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v52(&self) -> HumanElendaSnapshotV52 {
        HumanElendaSnapshotV52 {
            inner: self.inner.snapshot_v52(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v52(&mut self, snapshot: &HumanElendaSnapshotV52) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let generation = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .restore_v52(&snapshot.inner)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = generation;
        Ok(())
    }
    fn decision(&self, expected: FastActorDecisionV1) -> Result<ElendaDecisionV52, Error> {
        let decision = self
            .inner
            .decision_v52()
            .map_err(|_| Error::StaleDecision)?;
        if decision.expected() != expected {
            return Err(Error::StaleDecision);
        }
        Ok(decision)
    }
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::elenda_observation_v52::ElendaObservationV52, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        Ok(self.decision(expected)?.observation().clone())
    }
    /// Host-only opponent callback. Its private hand is never a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, RlContractError>
    where
        F: FnMut(
            &crate::elenda_observation_v52::ElendaObservationV52,
            &[ActionSemanticV1],
        ) -> Result<u32, RlContractError>,
    {
        if expected.acting_player == self.human_seat {
            return Err(RlContractError(
                "opponent callback cannot act for Human seat".into(),
            ));
        }
        let decision = self
            .decision(expected)
            .map_err(|_| RlContractError("stale opponent decision".into()))?;
        let index = policy(decision.observation(), decision.actions())?;
        self.inner.step_v52(&decision, index)
    }
}
struct BoundElendaV52 {
    authority: AuthorityV52,
    decision: ElendaDecisionV52,
    visible: HumanDecisionElendaV52,
    indexes: Vec<u32>,
}
pub struct HumanElendaProjectorV52 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundElendaV52>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV52)>,
}
impl HumanElendaProjectorV52 {
    pub fn new(human_seat: PlayerSeatV1) -> Self {
        Self {
            human_seat,
            next_prompt_seq: 1,
            pending: None,
            accepted: None,
        }
    }
    pub fn project_current(
        &mut self,
        session: &HumanElendaSessionV52,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionElendaV52, Error> {
        if expected.acting_player != self.human_seat || session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = session.decision(expected)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.decision.expected() == expected
                && session
                    .inner
                    .validate_decision_v52(&pending.decision)
                    .is_ok()
            {
                let (visible, indexes) =
                    project(&decision, self.human_seat, pending.visible.prompt_seq)?;
                if visible == pending.visible && indexes == pending.indexes {
                    return Ok(visible);
                }
            }
        }
        let next = self
            .next_prompt_seq
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        let (visible, indexes) = project(&decision, self.human_seat, self.next_prompt_seq)?;
        self.pending = Some(BoundElendaV52 {
            authority: session.authority.clone(),
            decision,
            visible: visible.clone(),
            indexes,
        });
        self.next_prompt_seq = next;
        Ok(visible)
    }
    pub fn submit(
        &mut self,
        session: &mut HumanElendaSessionV52,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, Error> {
        if session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        if let Some((receipt, authority)) = &self.accepted {
            if receipt.prompt_seq == request.prompt_seq {
                if *authority != session.authority {
                    return Err(Error::StaleDecision);
                }
                return if receipt.action_index == request.action_index {
                    Ok(receipt.clone())
                } else {
                    Err(Error::ConflictingRetry)
                };
            }
        }
        let pending = self.pending.as_ref().ok_or(Error::StaleDecision)?;
        if pending.authority != session.authority
            || request.prompt_seq != pending.visible.prompt_seq
        {
            return Err(Error::StaleDecision);
        }
        session
            .inner
            .validate_decision_v52(&pending.decision)
            .map_err(|_| Error::StaleDecision)?;
        let (visible, indexes) = project(
            &pending.decision,
            self.human_seat,
            pending.visible.prompt_seq,
        )?;
        if visible != pending.visible || indexes != pending.indexes {
            return Err(Error::StaleDecision);
        }
        let index = *pending
            .indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        let result = session.inner.step_v52(&pending.decision, index);
        self.pending = None;
        result.map_err(|_| Error::ExecutionFailed)?;
        let receipt = HumanActionReceiptV1 {
            prompt_seq: request.prompt_seq,
            action_index: request.action_index,
        };
        self.accepted = Some((receipt.clone(), session.authority.clone()));
        Ok(receipt)
    }
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
