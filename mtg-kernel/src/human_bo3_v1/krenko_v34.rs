//! Sealed Krenko successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::krenko_observation_v1::KrenkoObservationV1;
use crate::rl::ActionSemanticV1;
use crate::rl_session::krenko_v34::KrenkoDecisionBindingV34;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV34 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV34 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV34 {}
pub struct HumanKrenkoSessionV34 {
    inner: FastActorSessionV1,
    authority: AuthorityV34,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanKrenkoSnapshotV34 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanKrenkoSessionV34 {
    pub fn from_session(
        session: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_krenko_v34()?,
            human_seat,
            authority: AuthorityV34 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v34(&self) -> HumanKrenkoSnapshotV34 {
        HumanKrenkoSnapshotV34 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v34(&mut self, snapshot: &HumanKrenkoSnapshotV34) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .krenko_restore_v34(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::krenko_observation_v1::KrenkoObservationV1, crate::rl::RlContractError> {
        if expected.acting_player != self.human_seat {
            return Err(crate::rl::RlContractError(
                "Krenko Human view is fixed to its selected seat".into(),
            ));
        }
        self.inner
            .krenko_current_decision_input_v34(expected, self.human_seat)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Krenko public decision".into()))
    }
    /// Host-only policy callback. Opponent private rosters never enter a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, crate::rl::RlContractError>
    where
        F: FnMut(
            &KrenkoObservationV1,
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
            .krenko_current_decision_input_v34(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale opponent input".into()))?;
        let index = policy(&observation, &actions)?;
        if index as usize >= actions.len() {
            return Err(crate::rl::RlContractError(
                "opponent action out of range".into(),
            ));
        }
        self.inner
            .krenko_validate_bound_decision_v34(expected, expected.acting_player, &binding)
            .map_err(|_| crate::rl::RlContractError("opponent input changed".into()))?;
        self.inner
            .step(expected.episode_id, expected.step, index)
            .map_err(|error| crate::rl::RlContractError(format!("opponent execution:{error:?}")))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionKrenkoV34 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanKrenkoExtensionsV34>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanKrenkoExtensionsV34 {
    pub predecessor: HumanExtensionsV9,
    pub sources: Vec<HumanKrenkoSourceV34>,
    pub activations: Vec<HumanKrenkoActivationV34>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanKrenkoSourceV34 {
    pub source: HumanCardRefV1,
    pub current_controlled_goblin_permanents: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanKrenkoRelationV34 {
    pub handle: String,
    pub current_visible_card: Option<HumanCardRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanKrenkoActivationV34 {
    pub source_at_payment: HumanCardRefV1,
    pub captured_controller: PlayerSeatV1,
    pub lifecycle: crate::krenko_public_v1::LifecycleV1,
    pub sampled_goblins: Option<u64>,
    pub created_tokens: Vec<HumanKrenkoRelationV34>,
}
fn relation(
    observation: &KrenkoObservationV1,
    handles: &Handles,
    target: crate::krenko_public_v1::TokenIncarnationV1,
) -> Result<HumanKrenkoRelationV34, Error> {
    let mut relations = Vec::new();
    for row in &observation.facts.activations {
        for token in &row.created_tokens {
            if !relations.contains(token) {
                relations.push(*token);
            }
        }
    }
    let index = relations
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
    Ok(HumanKrenkoRelationV34 {
        handle: format!("token{}", index + 1),
        current_visible_card: current
            .map(|c| handles.get(&c.stable).cloned())
            .transpose()?,
    })
}
fn parts(observation: &KrenkoObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
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
    observation: &KrenkoObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanKrenkoExtensionsV34>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let sources = observation
        .facts
        .sources
        .iter()
        .map(|row| {
            Ok(HumanKrenkoSourceV34 {
                source: handles.get(&row.source)?.clone(),
                current_controlled_goblin_permanents: row.current_controlled_goblin_permanents,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let activations = observation
        .facts
        .activations
        .iter()
        .map(|row| {
            Ok(HumanKrenkoActivationV34 {
                source_at_payment: handles.get(&row.source_at_payment)?.clone(),
                captured_controller: row.captured_controller,
                lifecycle: row.lifecycle,
                sampled_goblins: row.sampled_goblins,
                created_tokens: row
                    .created_tokens
                    .iter()
                    .map(|r| relation(observation, handles, *r))
                    .collect::<Result<Vec<_>, _>>()?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanKrenkoExtensionsV34 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            sources,
            activations,
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
type ProjectedKrenkoV34 = (
    HumanVisibleStateV1<HumanKrenkoExtensionsV34>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

// V40 uses the same typed safe fields; no predecessor artifact/input admission.
pub(super) fn register_for_akroma_parts(
    observation: &crate::krenko_observation_v1::KrenkoObservationV1,
    handles: &mut Handles,
) -> Result<(), Error> {
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &observation.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    for row in &observation.facts.sources {
        handles.insert(&row.source)?;
    }
    for row in &observation.facts.activations {
        handles.insert(&row.source_at_payment)?;
    }
    Ok(())
}
pub(super) fn state_for_akroma_parts(
    observation: &crate::krenko_observation_v1::KrenkoObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanKrenkoExtensionsV34, Error> {
    Ok(state(observation, handles, human)?.extensions)
}
pub(super) fn label_for_akroma_parts(
    action: &ActionSemanticV1,
    observation: &crate::krenko_observation_v1::KrenkoObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    super::labels::label_krenko_v34(action, observation, handles, human)
}

pub(super) fn unary_for_akroma_parts(
    extension: &HumanKrenkoExtensionsV34,
    unary: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for row in &extension.predecessor.predecessor.ward_life_card_traits {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &extension.sources {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &extension.activations {
        unary
            .entry(row.source_at_payment.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        for token in &row.created_tokens {
            if let Some(card) = &token.current_visible_card {
                unary
                    .entry(card.handle.clone())
                    .or_default()
                    .push(visible::without_handles(row)?);
            }
        }
    }
    Ok(())
}
fn project_base(
    observation: &KrenkoObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedKrenkoV34, Error> {
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
    for row in &observation.facts.sources {
        handles.insert(&row.source)?;
    }
    for row in &observation.facts.activations {
        handles.insert(&row.source_at_payment)?;
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
    for row in &initial.extensions.sources {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &initial.extensions.activations {
        unary
            .entry(row.source_at_payment.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        for token in &row.created_tokens {
            if let Some(card) = &token.current_visible_card {
                unary
                    .entry(card.handle.clone())
                    .or_default()
                    .push(visible::without_handles(row)?);
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
                super::labels::label_krenko_v34(action, observation, &handles, human)?,
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

struct BoundKrenkoV34 {
    authority: AuthorityV34,
    expected: FastActorDecisionV1,
    binding: KrenkoDecisionBindingV34,
    public: crate::krenko_observation_v1::KrenkoObservationV1,
    visible: HumanDecisionKrenkoV34,
    engine_indexes: Vec<u32>,
}
pub struct HumanKrenkoProjectorV34 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundKrenkoV34>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV34)>,
}
impl HumanKrenkoProjectorV34 {
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
        session: &HumanKrenkoSessionV34,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionKrenkoV34, Error> {
        if expected.acting_player != self.human_seat || self.human_seat != session.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .krenko_current_decision_input_v34(expected, self.human_seat)
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
        let visible = HumanDecisionKrenkoV34 {
            schema: "mtg-kernel-human-krenko-decision/v34".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundKrenkoV34 {
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
        session: &mut HumanKrenkoSessionV34,
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
            .krenko_validate_bound_decision_v34(pending.expected, self.human_seat, &pending.binding)
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
