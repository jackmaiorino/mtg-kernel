//! Sealed FishingPole successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::fishing_pole_observation_v39::FishingPoleObservationV1;
use crate::rl::ActionSemanticV1;
use crate::rl_session::fishing_pole_v39::FishingPoleDecisionBindingV39;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV39 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV39 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV39 {}
pub struct HumanFishingPoleSessionV39 {
    inner: FastActorSessionV1,
    authority: AuthorityV39,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanFishingPoleSnapshotV39 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanFishingPoleSessionV39 {
    pub fn from_session(
        session: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_fishing_pole_v39()?,
            human_seat,
            authority: AuthorityV39 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v39(&self) -> HumanFishingPoleSnapshotV39 {
        HumanFishingPoleSnapshotV39 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v39(&mut self, snapshot: &HumanFishingPoleSnapshotV39) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .fishing_pole_restore_v39(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<
        crate::fishing_pole_observation_v39::FishingPoleObservationV1,
        crate::rl::RlContractError,
    > {
        if expected.acting_player != self.human_seat {
            return Err(crate::rl::RlContractError(
                "FishingPole Human view is fixed to its selected seat".into(),
            ));
        }
        self.inner
            .fishing_pole_current_decision_input_v39(expected, self.human_seat)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale FishingPole public decision".into()))
    }
    /// Host-only policy callback. Opponent private rosters never enter a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, crate::rl::RlContractError>
    where
        F: FnMut(
            &FishingPoleObservationV1,
            &[ActionSemanticV1],
        ) -> Result<u32, crate::rl::RlContractError>,
    {
        if expected.acting_player == self.human_seat {
            return Err(crate::rl::RlContractError(
                "opponent callback cannot act for the Human seat".into(),
            ));
        }
        let (observation, actions, binding) = self
            .inner
            .fishing_pole_current_decision_input_v39(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale opponent input".into()))?;
        let semantics = actions
            .iter()
            .map(|action| action.semantic.clone())
            .collect::<Vec<_>>();
        let index = policy(&observation, &semantics)?;
        if index as usize >= actions.len() {
            return Err(crate::rl::RlContractError(
                "opponent action out of range".into(),
            ));
        }
        self.inner
            .fishing_pole_validate_bound_decision_v39(expected, expected.acting_player, &binding)
            .map_err(|_| crate::rl::RlContractError("opponent input changed".into()))?;
        self.inner
            .step(expected.episode_id, expected.step, index)
            .map_err(|error| crate::rl::RlContractError(format!("opponent execution:{error:?}")))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionFishingPoleV39 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanFishingPoleExtensionsV39>,
    pub actions: Vec<HumanLegalActionV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFishingPoleExtensionsV39 {
    pub predecessor: HumanExtensionsV9,
    pub poles: Vec<HumanFishingPoleSourceV39>,
    pub operations: Vec<HumanFishingPoleOperationV39>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFishingPoleSourceV39 {
    pub source: HumanCardRefV1,
    pub bait: i32,
    pub attached_host: Option<HumanFishingPoleRelationV39>,
    pub current_named_pole_cost_candidate: bool,
    pub printed_abilities_active: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFishingPoleRelationV39 {
    pub handle: String,
    pub current_visible_card: Option<HumanCardRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFishingPoleOperationV39 {
    pub original_pole: HumanCardRefV1,
    pub original_host: HumanFishingPoleRelationV39,
    pub original_controller: PlayerSeatV1,
    pub trigger: bool,
    pub stage: crate::fishing_pole_public_v39::StageV39,
    pub bait_before: Option<i32>,
    pub bait_after: Option<i32>,
    pub created_fish: Option<HumanFishingPoleRelationV39>,
}
fn relation(
    observation: &FishingPoleObservationV1,
    handles: &Handles,
    target: crate::fishing_pole_public_v39::IncarnationV39,
) -> Result<HumanFishingPoleRelationV39, Error> {
    let mut relations = Vec::new();
    for pole in &observation.facts.poles {
        if let Some(host) = pole.attached_host {
            if !relations.contains(&host) {
                relations.push(host);
            }
        }
    }
    for row in &observation.facts.operations {
        for reference in [Some(row.original_host), row.created_fish]
            .into_iter()
            .flatten()
        {
            if !relations.contains(&reference) {
                relations.push(reference);
            }
        }
    }
    let ordinal = relations
        .iter()
        .position(|r| *r == target)
        .ok_or(Error::InvalidVisibleReference)?;
    let current = observation
        .projection
        .surface
        .battlefield
        .iter()
        .flatten()
        .find(|c| {
            c.stable.arena_id == target.arena_id
                && c.stable.zone_change_count == target.zone_change_count
        });
    Ok(HumanFishingPoleRelationV39 {
        handle: format!("incarnation{}", ordinal + 1),
        current_visible_card: current
            .map(|c| handles.get(&c.stable).cloned())
            .transpose()?,
    })
}
fn parts(observation: &FishingPoleObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
    super::life_ward_v8::ProjectionPartsV8 {
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
        known_face_down_cards: &observation.known_face_down_cards,
        extensions: &observation.extensions,
    }
}
fn state(
    observation: &FishingPoleObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanFishingPoleExtensionsV39>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let poles = observation
        .facts
        .poles
        .iter()
        .map(|row| {
            Ok(HumanFishingPoleSourceV39 {
                source: handles.get(&row.source)?.clone(),
                bait: row.bait,
                attached_host: row
                    .attached_host
                    .map(|host| relation(observation, handles, host))
                    .transpose()?,
                current_named_pole_cost_candidate: row.current_named_pole_cost_candidate,
                printed_abilities_active: row.printed_abilities_active,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let operations = observation
        .facts
        .operations
        .iter()
        .map(|row| {
            Ok(HumanFishingPoleOperationV39 {
                original_pole: handles.get(&row.original_pole)?.clone(),
                original_host: relation(observation, handles, row.original_host)?,
                original_controller: row.original_controller,
                trigger: row.trigger,
                stage: row.stage,
                bait_before: row.bait_before,
                bait_after: row.bait_after,
                created_fish: row
                    .created_fish
                    .map(|fish| relation(observation, handles, fish))
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanFishingPoleExtensionsV39 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            poles,
            operations,
        },
        policy_context: old.policy_context,
        known_face_down_cards: old.known_face_down_cards,
        poison_counters: old.poison_counters,
        poison_prevention: old.poison_prevention,
        creatures_attacked_this_turn: old.creatures_attacked_this_turn,
        ninja_emblems: old.ninja_emblems,
        restricted_mana: old.restricted_mana,
    })
}
type ProjectedFishingPoleV39 = (
    HumanVisibleStateV1<HumanFishingPoleExtensionsV39>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

fn project_base(
    observation: &FishingPoleObservationV1,
    actions: &[crate::rl_session::fishing_pole_v39::ClassifiedActionV39],
    human: PlayerSeatV1,
) -> Result<ProjectedFishingPoleV39, Error> {
    if observation.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    if actions.is_empty() {
        return Err(Error::UnsupportedPrompt);
    }
    observation
        .validate_artifact()
        .map_err(|_| Error::InvalidVisibleReference)?;
    let mut handles = Handles::from_parts(
        &observation.projection,
        &observation.own_hand,
        &observation.known_hand_cards,
        &observation.known_library_cards,
        observation.extensions.decision_local_library.as_ref(),
    )?;
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &observation.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    for row in &observation.facts.poles {
        handles.insert(&row.source)?;
    }
    for row in &observation.facts.operations {
        handles.insert(&row.original_pole)?;
    }
    let initial = state(observation, &mut handles, human)?;
    let mut unary = BTreeMap::<String, Vec<String>>::new();
    for row in &initial
        .extensions
        .predecessor
        .predecessor
        .ward_life_card_traits
    {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for (ordinal, row) in initial.extensions.poles.iter().enumerate() {
        let facts = visible::without_handles(row)?;
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(format!("Pole{ordinal}:source:{facts}"));
        if let Some(host) = row
            .attached_host
            .as_ref()
            .and_then(|host| host.current_visible_card.as_ref())
        {
            unary
                .entry(host.handle.clone())
                .or_default()
                .push(format!("Pole{ordinal}:host:{facts}"));
        }
    }
    for (ordinal, row) in initial.extensions.operations.iter().enumerate() {
        let facts = visible::without_handles(row)?;
        unary
            .entry(row.original_pole.handle.clone())
            .or_default()
            .push(format!("Operation{ordinal}:source:{facts}"));
        for (role, relation) in [
            ("host", Some(&row.original_host)),
            ("fish", row.created_fish.as_ref()),
        ] {
            if let Some(card) = relation.and_then(|r| r.current_visible_card.as_ref()) {
                unary
                    .entry(card.handle.clone())
                    .or_default()
                    .push(format!("Operation{ordinal}:{role}:{facts}"));
            }
        }
    }
    for rows in unary.values_mut() {
        rows.sort();
    }
    handles.canonicalize_parts(&initial, unary)?;
    let state = state(observation, &mut handles, human)?;
    let mut choices = actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            Ok((
                super::labels::label_fishing_pole_v39(
                    &action.semantic,
                    action.granted_label.as_ref(),
                    observation,
                    &handles,
                    human,
                )?,
                u32::try_from(index).map_err(|_| Error::InvalidAction)?,
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    choices.sort_by(|a, b| a.0.cmp(&b.0));
    if choices.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::UnsupportedPrompt);
    }
    let indexes = choices.iter().map(|(_, index)| *index).collect();
    let actions = choices
        .into_iter()
        .enumerate()
        .map(|(index, (label, _))| HumanLegalActionV1 {
            action_index: index as u32,
            label,
        })
        .collect();
    Ok((state, actions, indexes))
}

struct BoundFishingPoleV39 {
    authority: AuthorityV39,
    expected: FastActorDecisionV1,
    binding: FishingPoleDecisionBindingV39,
    public: crate::fishing_pole_observation_v39::FishingPoleObservationV1,
    visible: HumanDecisionFishingPoleV39,
    engine_indexes: Vec<u32>,
}
pub struct HumanFishingPoleProjectorV39 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundFishingPoleV39>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV39)>,
}
impl HumanFishingPoleProjectorV39 {
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
        session: &HumanFishingPoleSessionV39,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionFishingPoleV39, Error> {
        if expected.acting_player != self.human_seat || self.human_seat != session.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .fishing_pole_current_decision_input_v39(expected, self.human_seat)
            .map_err(|_| Error::StaleDecision)?;
        let public = session
            .public_observation(expected)
            .map_err(|_| Error::StaleDecision)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.expected == expected
                && pending.binding == binding
                && pending.public == public
            {
                return Ok(pending.visible.clone());
            }
        }
        let next = self
            .next_prompt_seq
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        let (state, actions, engine_indexes) =
            project_base(&observation, &actions, self.human_seat)?;
        let visible = HumanDecisionFishingPoleV39 {
            schema: "mtg-kernel-human-fishing_pole-decision/v39".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundFishingPoleV39 {
            authority: session.authority.clone(),
            expected,
            binding,
            public,
            visible: visible.clone(),
            engine_indexes,
        });
        self.next_prompt_seq = next;
        Ok(visible)
    }
    pub fn submit(
        &mut self,
        session: &mut HumanFishingPoleSessionV39,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, Error> {
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
        let index = *pending
            .engine_indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        session
            .inner
            .fishing_pole_validate_bound_decision_v39(
                pending.expected,
                self.human_seat,
                &pending.binding,
            )
            .map_err(|_| Error::StaleDecision)?;
        if session
            .public_observation(pending.expected)
            .map_err(|_| Error::StaleDecision)?
            != pending.public
        {
            return Err(Error::StaleDecision);
        }
        let result = session
            .inner
            .step(pending.expected.episode_id, pending.expected.step, index);
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

#[cfg(test)]
mod tests;
