//! Sealed Inspiration successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::inspiration_observation_v1::InspirationObservationV1;
use crate::rl::ActionSemanticV1;
use crate::rl_session::inspiration_v37::InspirationDecisionBindingV37;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV37 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV37 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV37 {}
pub struct HumanInspirationSessionV37 {
    inner: FastActorSessionV1,
    authority: AuthorityV37,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanInspirationSnapshotV37 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanInspirationSessionV37 {
    pub fn from_session(
        session: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_inspiration_v37()?,
            human_seat,
            authority: AuthorityV37 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v37(&self) -> HumanInspirationSnapshotV37 {
        HumanInspirationSnapshotV37 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v37(&mut self, snapshot: &HumanInspirationSnapshotV37) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .inspiration_restore_v37(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<
        crate::inspiration_observation_v1::InspirationObservationV1,
        crate::rl::RlContractError,
    > {
        if expected.acting_player != self.human_seat {
            return Err(crate::rl::RlContractError(
                "Inspiration Human view is fixed to its selected seat".into(),
            ));
        }
        self.inner
            .inspiration_current_decision_input_v37(expected, self.human_seat)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Inspiration public decision".into()))
    }
    /// Host-only policy callback. Opponent private rosters never enter a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, crate::rl::RlContractError>
    where
        F: FnMut(
            &InspirationObservationV1,
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
            .inspiration_current_decision_input_v37(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale opponent input".into()))?;
        let index = policy(&observation, &actions)?;
        if index as usize >= actions.len() {
            return Err(crate::rl::RlContractError(
                "opponent action out of range".into(),
            ));
        }
        self.inner
            .inspiration_validate_bound_decision_v37(expected, expected.acting_player, &binding)
            .map_err(|_| crate::rl::RlContractError("opponent input changed".into()))?;
        self.inner
            .step(expected.episode_id, expected.step, index)
            .map_err(|error| crate::rl::RlContractError(format!("opponent execution:{error:?}")))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionInspirationV37 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanInspirationExtensionsV37>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanInspirationExtensionsV37 {
    pub predecessor: HumanExtensionsV9,
    pub choice: Option<HumanInspirationChoiceV37>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanInspirationChoiceV37 {
    pub source: HumanCardRefV1,
    pub controller: PlayerSeatV1,
    pub cast_root: crate::rules_meaning_inspiration_v1::CastRootV1,
    pub phase: crate::inspiration_observation_v1::PhaseV1,
    pub selected: Vec<HumanCardRefV1>,
    pub legal: Vec<HumanCardRefV1>,
    pub minimum: u16,
    pub maximum: u16,
    pub ordered: bool,
    pub can_finish: bool,
    pub instant_or_sorcery_only: bool,
    pub choice_is_not_a_target: bool,
}
fn parts(observation: &InspirationObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
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
    observation: &InspirationObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanInspirationExtensionsV37>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let choice = observation
        .choice
        .as_ref()
        .map(|row| {
            Ok(HumanInspirationChoiceV37 {
                source: handles.get(&row.source)?.clone(),
                controller: row.controller,
                cast_root: row.cast_root,
                phase: row.phase,
                selected: row
                    .selected
                    .iter()
                    .map(|c| handles.get(c).cloned())
                    .collect::<Result<Vec<_>, Error>>()?,
                legal: row
                    .legal
                    .iter()
                    .map(|c| handles.get(c).cloned())
                    .collect::<Result<Vec<_>, Error>>()?,
                minimum: row.minimum,
                maximum: row.maximum,
                ordered: row.ordered,
                can_finish: row.can_finish,
                instant_or_sorcery_only: row.phase
                    == crate::inspiration_observation_v1::PhaseV1::ReturnCard,
                choice_is_not_a_target: true,
            })
        })
        .transpose()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanInspirationExtensionsV37 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            choice,
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
fn label(
    action: &ActionSemanticV1,
    observation: &InspirationObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let Some(choice) = &observation.choice {
        match action {
            ActionSemanticV1::ChooseEffectTarget {
                actor,
                target: crate::rl::TargetRefV1::Object { object },
                ..
            } => {
                if *actor != human || !choice.legal.contains(object) {
                    return Err(Error::UnsupportedPrompt);
                }
                return Ok(match choice.phase {
                    crate::inspiration_observation_v1::PhaseV1::MillOrder => {
                        format!("Order {} into your graveyard", handles.name(object)?)
                    }
                    crate::inspiration_observation_v1::PhaseV1::ReturnCard => format!(
                        "Return {} from your graveyard to your hand (not a target)",
                        handles.name(object)?
                    ),
                });
            }
            ActionSemanticV1::FinishEffectSelection { actor, .. } => {
                if *actor != human || !choice.can_finish {
                    return Err(Error::UnsupportedPrompt);
                }
                return Ok("Finish Inspiration selection (no eligible card remains)".into());
            }
            _ => {}
        }
    }
    super::labels::label_parts_v8(action, &parts(observation), handles, human)
}

type ProjectedInspirationV37 = (
    HumanVisibleStateV1<HumanInspirationExtensionsV37>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

// V40 uses the same typed safe fields; no predecessor artifact/input admission.
pub(super) fn register_for_akroma_parts(
    observation: &crate::inspiration_observation_v1::InspirationObservationV1,
    handles: &mut Handles,
) -> Result<(), Error> {
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &observation.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    if let Some(row) = &observation.choice {
        handles.insert(&row.source)?;
        for card in row.selected.iter().chain(&row.legal) {
            handles.insert(card)?;
        }
    }
    Ok(())
}
pub(super) fn state_for_akroma_parts(
    observation: &crate::inspiration_observation_v1::InspirationObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanInspirationExtensionsV37, Error> {
    Ok(state(observation, handles, human)?.extensions)
}
pub(super) fn label_for_akroma_parts(
    action: &ActionSemanticV1,
    observation: &crate::inspiration_observation_v1::InspirationObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    label(action, observation, handles, human)
}

pub(super) fn unary_for_akroma_parts(
    extension: &HumanInspirationExtensionsV37,
    unary: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for row in &extension.predecessor.predecessor.ward_life_card_traits {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    if let Some(row) = &extension.choice {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        for (index, card) in row.selected.iter().enumerate() {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push(format!("inspiration_selected_{index}"));
        }
        for card in &row.legal {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push("inspiration_legal".into());
        }
    }
    Ok(())
}
fn project_base(
    observation: &InspirationObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedInspirationV37, Error> {
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
    if let Some(row) = &observation.choice {
        handles.insert(&row.source)?;
        for card in row.selected.iter().chain(&row.legal) {
            handles.insert(card)?;
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
    if let Some(row) = &initial.extensions.choice {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        for (index, card) in row.selected.iter().enumerate() {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push(format!("inspiration_selected_{index}"));
        }
        for card in &row.legal {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push("inspiration_legal".into());
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
                label(action, observation, &handles, human)?,
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

struct BoundInspirationV37 {
    authority: AuthorityV37,
    expected: FastActorDecisionV1,
    binding: InspirationDecisionBindingV37,
    public: crate::inspiration_observation_v1::InspirationObservationV1,
    visible: HumanDecisionInspirationV37,
    engine_indexes: Vec<u32>,
}
pub struct HumanInspirationProjectorV37 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundInspirationV37>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV37)>,
}
impl HumanInspirationProjectorV37 {
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
        session: &HumanInspirationSessionV37,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionInspirationV37, Error> {
        if expected.acting_player != self.human_seat || self.human_seat != session.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .inspiration_current_decision_input_v37(expected, self.human_seat)
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
        let visible = HumanDecisionInspirationV37 {
            schema: "mtg-kernel-human-inspiration-decision/v37".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundInspirationV37 {
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
        session: &mut HumanInspirationSessionV37,
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
            .inspiration_validate_bound_decision_v37(
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
