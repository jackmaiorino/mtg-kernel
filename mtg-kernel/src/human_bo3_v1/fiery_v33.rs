//! Sealed Fiery successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::fiery_observation_v1::FieryObservationV1;
use crate::rl::ActionSemanticV1;
use crate::rl_session::fiery_v33::FieryDecisionBindingV33;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV33 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV33 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV33 {}
pub struct HumanFierySessionV33 {
    inner: FastActorSessionV1,
    authority: AuthorityV33,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanFierySnapshotV33 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanFierySessionV33 {
    pub fn from_session(
        session: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_fiery_v33()?,
            human_seat,
            authority: AuthorityV33 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v33(&self) -> HumanFierySnapshotV33 {
        HumanFierySnapshotV33 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v33(&mut self, snapshot: &HumanFierySnapshotV33) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .fiery_restore_v33(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::fiery_observation_v1::FieryObservationV1, crate::rl::RlContractError> {
        if expected.acting_player != self.human_seat {
            return Err(crate::rl::RlContractError(
                "Fiery Human view is fixed to its selected seat".into(),
            ));
        }
        self.inner
            .fiery_current_decision_input_v33(expected, self.human_seat)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Fiery public decision".into()))
    }
    /// Host-only policy callback. Opponent private rosters never enter a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, crate::rl::RlContractError>
    where
        F: FnMut(
            &FieryObservationV1,
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
            .fiery_current_decision_input_v33(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale opponent input".into()))?;
        let index = policy(&observation, &actions)?;
        if index as usize >= actions.len() {
            return Err(crate::rl::RlContractError(
                "opponent action out of range".into(),
            ));
        }
        self.inner
            .fiery_validate_bound_decision_v33(expected, expected.acting_player, &binding)
            .map_err(|_| crate::rl::RlContractError("opponent input changed".into()))?;
        self.inner
            .step(expected.episode_id, expected.step, index)
            .map_err(|error| crate::rl::RlContractError(format!("opponent execution:{error:?}")))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionFieryV33 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanFieryExtensionsV33>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFieryExtensionsV33 {
    pub predecessor: HumanExtensionsV9,
    pub pending_targets: Option<HumanFieryPendingV33>,
    pub death_replacements: Vec<HumanFieryReplacementV33>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFieryRelationV33 {
    pub handle: String,
    pub current_visible_card: Option<HumanCardRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFieryPendingV33 {
    pub source: HumanCardRefV1,
    pub caster: PlayerSeatV1,
    pub phase: crate::fiery_public_v1::CastTargetPhaseV1,
    pub selected: Vec<HumanFieryRelationV33>,
    pub legal: Vec<HumanCardRefV1>,
    pub can_finish_without_equipment: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFieryReplacementV33 {
    pub source_at_resolution: HumanCardRefV1,
    pub original_target: HumanFieryRelationV33,
    pub original_target_present: bool,
    pub ending_turn_key: u32,
    pub ending_active_player: PlayerSeatV1,
    pub lifecycle: crate::fiery_public_v1::LifecycleV1,
}
fn visible_target(
    observation: &FieryObservationV1,
    target: crate::fiery_public_v1::TargetIncarnationV1,
) -> Option<&crate::rl::CardStableRefV1> {
    observation
        .projection
        .surface
        .battlefield
        .iter()
        .flatten()
        .find(|card| {
            card.stable.arena_id == target.arena_id
                && card.stable.zone_change_count == target.zone_change_count
        })
        .map(|card| &card.stable)
}
fn relation(
    observation: &FieryObservationV1,
    handles: &Handles,
    target: crate::fiery_public_v1::TargetIncarnationV1,
) -> Result<HumanFieryRelationV33, Error> {
    let mut relations = Vec::new();
    if let Some(pending) = &observation.facts.pending_cast_targets {
        for target in &pending.selected_targets {
            if !relations.contains(target) {
                relations.push(*target);
            }
        }
    }
    for row in &observation.facts.death_replacements {
        if !relations.contains(&row.original_target) {
            relations.push(row.original_target);
        }
    }
    let index = relations
        .iter()
        .position(|row| *row == target)
        .ok_or(Error::InvalidVisibleReference)?;
    Ok(HumanFieryRelationV33 {
        handle: format!("target{}", index + 1),
        current_visible_card: visible_target(observation, target)
            .map(|r| handles.get(r).cloned())
            .transpose()?,
    })
}
fn parts(observation: &FieryObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
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
    observation: &FieryObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanFieryExtensionsV33>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let pending_targets = observation
        .facts
        .pending_cast_targets
        .as_ref()
        .map(|row| -> Result<_, Error> {
            Ok(HumanFieryPendingV33 {
                source: handles.get(&row.announced_source)?.clone(),
                caster: row.caster,
                phase: row.phase,
                selected: row
                    .selected_targets
                    .iter()
                    .map(|r| relation(observation, handles, *r))
                    .collect::<Result<Vec<_>, _>>()?,
                legal: observation
                    .cast_menu
                    .as_ref()
                    .map(|m| {
                        m.legal
                            .iter()
                            .map(|r| handles.get(r).cloned())
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default(),
                can_finish_without_equipment: row.can_finish_without_equipment,
            })
        })
        .transpose()?;
    let death_replacements = observation
        .facts
        .death_replacements
        .iter()
        .map(|row| -> Result<_, Error> {
            Ok(HumanFieryReplacementV33 {
                source_at_resolution: handles.get(&row.source_at_resolution)?.clone(),
                original_target: relation(observation, handles, row.original_target)?,
                original_target_present: row.original_target_present,
                ending_turn_key: row.ending_turn_key,
                ending_active_player: row.ending_active_player,
                lifecycle: row.lifecycle,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanFieryExtensionsV33 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            pending_targets,
            death_replacements,
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
type ProjectedFieryV33 = (
    HumanVisibleStateV1<HumanFieryExtensionsV33>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

// V40 uses the same typed safe fields; no predecessor artifact/input admission.
pub(super) fn register_for_akroma_parts(
    observation: &crate::fiery_observation_v1::FieryObservationV1,
    handles: &mut Handles,
) -> Result<(), Error> {
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &observation.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    if let Some(pending) = &observation.facts.pending_cast_targets {
        handles.insert(&pending.announced_source)?;
    }
    for row in &observation.facts.death_replacements {
        handles.insert(&row.source_at_resolution)?;
    }
    if let Some(menu) = &observation.cast_menu {
        for r in &menu.legal {
            handles.insert(r)?;
        }
    }
    Ok(())
}
pub(super) fn state_for_akroma_parts(
    observation: &crate::fiery_observation_v1::FieryObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanFieryExtensionsV33, Error> {
    Ok(state(observation, handles, human)?.extensions)
}
pub(super) fn label_for_akroma_parts(
    action: &ActionSemanticV1,
    observation: &crate::fiery_observation_v1::FieryObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    super::labels::label_fiery_v33(action, observation, handles, human)
}

pub(super) fn unary_for_akroma_parts(
    extension: &HumanFieryExtensionsV33,
    unary: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for row in &extension.predecessor.predecessor.ward_life_card_traits {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &extension.death_replacements {
        unary
            .entry(row.source_at_resolution.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        if let Some(card) = &row.original_target.current_visible_card {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push(visible::without_handles(row)?);
        }
    }
    if let Some(pending) = &extension.pending_targets {
        unary
            .entry(pending.source.handle.clone())
            .or_default()
            .push(visible::without_handles(pending)?);
        for card in &pending.legal {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push("fiery_legal_target".into());
        }
    }
    Ok(())
}
fn project_base(
    observation: &FieryObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedFieryV33, Error> {
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
    if let Some(pending) = &observation.facts.pending_cast_targets {
        handles.insert(&pending.announced_source)?;
    }
    for row in &observation.facts.death_replacements {
        handles.insert(&row.source_at_resolution)?;
    }
    if let Some(menu) = &observation.cast_menu {
        for r in &menu.legal {
            handles.insert(r)?;
        }
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
    for row in &initial.extensions.death_replacements {
        unary
            .entry(row.source_at_resolution.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        if let Some(card) = &row.original_target.current_visible_card {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push(visible::without_handles(row)?);
        }
    }
    if let Some(pending) = &initial.extensions.pending_targets {
        unary
            .entry(pending.source.handle.clone())
            .or_default()
            .push(visible::without_handles(pending)?);
        for card in &pending.legal {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push("fiery_legal_target".into());
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
                super::labels::label_fiery_v33(action, observation, &handles, human)?,
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

struct BoundFieryV33 {
    authority: AuthorityV33,
    expected: FastActorDecisionV1,
    binding: FieryDecisionBindingV33,
    public: crate::fiery_observation_v1::FieryObservationV1,
    visible: HumanDecisionFieryV33,
    engine_indexes: Vec<u32>,
}
pub struct HumanFieryProjectorV33 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundFieryV33>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV33)>,
}
impl HumanFieryProjectorV33 {
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
        session: &HumanFierySessionV33,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionFieryV33, Error> {
        if expected.acting_player != self.human_seat || self.human_seat != session.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .fiery_current_decision_input_v33(expected, self.human_seat)
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
        let visible = HumanDecisionFieryV33 {
            schema: "mtg-kernel-human-fiery-decision/v33".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundFieryV33 {
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
        session: &mut HumanFierySessionV33,
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
            .fiery_validate_bound_decision_v33(pending.expected, self.human_seat, &pending.binding)
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
