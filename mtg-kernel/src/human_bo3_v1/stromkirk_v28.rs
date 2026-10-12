//! Sealed Stromkirk successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::rl::ActionSemanticV1;
use crate::rl_session::stromkirk_v28::StromkirkDecisionBindingV28;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use crate::stromkirk_public_v1::StromkirkObservationV1;
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV28 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV28 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV28 {}
pub struct HumanStromkirkSessionV28 {
    inner: FastActorSessionV1,
    authority: AuthorityV28,
}
#[derive(Clone)]
pub struct HumanStromkirkSnapshotV28 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanStromkirkSessionV28 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_stromkirk_v28()?,
            authority: AuthorityV28 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v28(&self) -> HumanStromkirkSnapshotV28 {
        HumanStromkirkSnapshotV28 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v28(&mut self, snapshot: &HumanStromkirkSnapshotV28) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner.restore_v1(&snapshot.snapshot);
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::stromkirk_public_v1::StromkirkObservationV1, crate::rl::RlContractError>
    {
        self.inner
            .stromkirk_current_decision_input_v28(expected, expected.acting_player)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Stromkirk public decision".into()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionStromkirkV28 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanStromkirkExtensionsV28>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanLossContextV28 {
    pub source_at_end_step: HumanCardRefV1,
    pub captured_controller: PlayerSeatV1,
    pub pending_trigger_ordinal: Option<u32>,
    pub live_stack_index: Option<u32>,
    pub opponent_lost_life_if_resolved_now: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanStromkirkExtensionsV28 {
    pub predecessor: HumanExtensionsV9,
    pub loss_contexts: Vec<HumanLossContextV28>,
    pub own_lost_life_this_turn: bool,
    pub opponent_lost_life_this_turn: bool,
}
fn parts(observation: &StromkirkObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
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
    observation: &StromkirkObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanStromkirkExtensionsV28>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let mut loss_contexts = observation
        .loss_contexts
        .iter()
        .map(|row| {
            Ok(HumanLossContextV28 {
                source_at_end_step: handles.get(&row.source_at_end_step)?.clone(),
                captured_controller: row.captured_controller,
                pending_trigger_ordinal: row.pending_trigger_ordinal,
                live_stack_index: row.live_stack_index,
                opponent_lost_life_if_resolved_now: row.opponent_lost_life_if_resolved_now,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    loss_contexts.sort_by(|a, b| {
        a.source_at_end_step
            .handle
            .cmp(&b.source_at_end_step.handle)
            .then(a.pending_trigger_ordinal.cmp(&b.pending_trigger_ordinal))
            .then(a.live_stack_index.cmp(&b.live_stack_index))
    });
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanStromkirkExtensionsV28 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            loss_contexts,
            own_lost_life_this_turn: observation.lost_life_this_turn[seat],
            opponent_lost_life_this_turn: observation.lost_life_this_turn[1 - seat],
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
    observation: &StromkirkObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    let old = super::labels::label_parts_v8(action, &parts(observation), handles, human)?;
    let source = match action {
        ActionSemanticV1::CastSpell { source, .. } => Some(source),
        ActionSemanticV1::ChooseTarget { source, .. } => Some(source),
        _ => None,
    };
    let Some(source) = source else {
        return Ok(old);
    };
    let Some(root) = observation
        .roots
        .iter()
        .find(|root| root.card_id == source.card_db_id)
    else {
        return Ok(old);
    };
    let selector =
        crate::rules_meaning_stromkirk_v1::live_selector().map_err(|_| Error::UnsupportedPrompt)?;
    let meaning =
        crate::rules_meaning_stromkirk_v1::meaning(&selector, root.card_id, root.ability_index)
            .map_err(|_| Error::UnsupportedPrompt)?;
    if root.root_sha256 != meaning.root_sha256 {
        return Err(Error::UnsupportedPrompt);
    }
    if matches!(action, ActionSemanticV1::CastSpell { .. }) {
        return Ok(format!("{old}; printed {{2}}{{B}} 2/2 Vampire Rogue; at your end step, if an opponent lost life this turn, put one +1/+1 counter on target Vampire permanent you control"));
    }
    let contexts: Vec<_> = observation
        .loss_contexts
        .iter()
        .filter(|row| {
            &row.source_at_end_step == source
                && row.captured_controller == human
                && row.pending_trigger_ordinal.is_some()
        })
        .collect();
    let [context] = contexts.as_slice() else {
        return Err(Error::UnsupportedPrompt);
    };
    Ok(format!("{old}; target Vampire permanent you control receives one +1/+1 counter if your opponent lost life this turn (currently {})", context.opponent_lost_life_if_resolved_now))
}

type ProjectedStromkirkV28 = (
    HumanVisibleStateV1<HumanStromkirkExtensionsV28>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

fn project_base(
    observation: &StromkirkObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedStromkirkV28, Error> {
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
    for row in &observation.loss_contexts {
        handles.insert(&row.source_at_end_step)?;
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
    for row in &initial.extensions.loss_contexts {
        unary
            .entry(row.source_at_end_step.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
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

struct BoundStromkirkV28 {
    authority: AuthorityV28,
    expected: FastActorDecisionV1,
    binding: StromkirkDecisionBindingV28,
    public: crate::stromkirk_public_v1::StromkirkObservationV1,
    visible: HumanDecisionStromkirkV28,
    engine_indexes: Vec<u32>,
}
pub struct HumanStromkirkProjectorV28 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundStromkirkV28>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV28)>,
}
impl HumanStromkirkProjectorV28 {
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
        session: &HumanStromkirkSessionV28,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionStromkirkV28, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .stromkirk_current_decision_input_v28(expected, self.human_seat)
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
        let visible = HumanDecisionStromkirkV28 {
            schema: "mtg-kernel-human-stromkirk-decision/v28".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundStromkirkV28 {
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
        session: &mut HumanStromkirkSessionV28,
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
            .stromkirk_validate_bound_decision_v28(
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
