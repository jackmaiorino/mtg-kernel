//! Sealed explicit Zombie label successor over the V9 public facts.
//! V10 through V12 retain their separate prepared identities.
use super::visible::{self, Handles};
use super::*;
use crate::policy_observation_v8::ObservationV8;
use crate::rl::ActionSemanticV1;
use crate::rl_session::human_v8::HumanDecisionBindingV8;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV13 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV13 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV13 {}
pub struct HumanZombieSessionV13 {
    inner: FastActorSessionV1,
    authority: AuthorityV13,
}
#[derive(Clone)]
pub struct HumanZombieSnapshotV13(FastActorSessionSnapshotV1);
impl HumanZombieSessionV13 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_human_v8()?,
            authority: AuthorityV13 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v13(&self) -> HumanZombieSnapshotV13 {
        HumanZombieSnapshotV13(self.inner.snapshot_v1())
    }
    pub fn restore_v13(&mut self, snapshot: &HumanZombieSnapshotV13) -> Result<(), Error> {
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner.restore_v1(&snapshot.0);
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<
        crate::soul_shackled_zombie_public_v1::ZombiePublicObservationV1,
        crate::rl::RlContractError,
    > {
        self.inner.human_zombie_observation_v13(expected)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionZombieV13 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanExtensionsV9>,
    pub single_graveyard_triggers: Vec<HumanSingleGraveyardContextV13>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanSingleGraveyardContextV13 {
    pub stage: crate::soul_shackled_zombie_public_v1::TriggerStageV1,
    pub source: HumanCardRefV1,
    pub original_controller: PlayerSeatV1,
    pub captured_definition_colors: Vec<crate::mana::ManaColor>,
    pub selected: Vec<HumanCardRefV1>,
    pub selected_graveyard_owner: Option<PlayerSeatV1>,
    pub selected_live_legal: Vec<bool>,
    pub minimum: u8,
    pub maximum: u8,
    pub legal_choices: Vec<HumanCardRefV1>,
    pub can_finish: bool,
}
fn project_contexts(
    contexts: &[crate::soul_shackled_zombie_public_v1::SingleGraveyardContextV1],
    handles: &Handles,
) -> Result<Vec<HumanSingleGraveyardContextV13>, Error> {
    contexts
        .iter()
        .map(|c| {
            Ok(HumanSingleGraveyardContextV13 {
                stage: c.stage.clone(),
                source: handles.get(&c.source)?.clone(),
                original_controller: c.original_controller,
                captured_definition_colors: c.captured_definition_colors.clone(),
                selected: c
                    .selected
                    .iter()
                    .map(|r| handles.get(r).cloned())
                    .collect::<Result<_, _>>()?,
                selected_graveyard_owner: c.selected_graveyard_owner,
                selected_live_legal: c.selected_live_legal.clone(),
                minimum: c.minimum,
                maximum: c.maximum,
                legal_choices: c
                    .legal_choices
                    .iter()
                    .map(|r| handles.get(r).cloned())
                    .collect::<Result<_, _>>()?,
                can_finish: c.can_finish,
            })
        })
        .collect()
}

fn label(
    action: &ActionSemanticV1,
    observation: &ObservationV8,
    contexts: &[crate::soul_shackled_zombie_public_v1::SingleGraveyardContextV1],
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    use ActionSemanticV1 as A;
    let source = match action {
        A::ChooseTarget { source, .. } | A::FinishTargetSelection { source, .. } => Some(source),
        _ => None,
    };
    if let Some(c) = source.and_then(|source| {
        contexts.iter().find(|c| {
            matches!(
                c.stage,
                crate::soul_shackled_zombie_public_v1::TriggerStageV1::Pending { ordinal: 0 }
            ) && c.source.arena_id == source.arena_id
                && c.can_finish
        })
    }) {
        if c.original_controller != human {
            return Err(Error::NotHumanTurn);
        }
        match action {
            A::ChooseTarget {
                target: crate::rl::TargetRefV1::Object { object },
                ..
            } => {
                if !c.legal_choices.contains(object) {
                    return Err(Error::InvalidVisibleReference);
                }
                return Ok(format!(
                    "Exile {} with {} (choose up to two from one graveyard; {} selected)",
                    handles.name(object)?,
                    handles.name(&c.source)?,
                    c.selected.len()
                ));
            }
            A::FinishTargetSelection { selected_count, .. } => {
                if usize::from(*selected_count) != c.selected.len() {
                    return Err(Error::InvalidAction);
                }
                return Ok(format!(
                    "Finish {} exile selection with {} selected",
                    handles.name(&c.source)?,
                    c.selected.len()
                ));
            }
            _ => return Err(Error::UnsupportedPrompt),
        }
    }
    super::labels::label_v8(action, observation, handles, human)
}

fn project_base(
    observation: &ObservationV8,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    contexts: &[crate::soul_shackled_zombie_public_v1::SingleGraveyardContextV1],
) -> Result<
    (
        HumanVisibleStateV8,
        Vec<HumanLegalActionV1>,
        Vec<u32>,
        Vec<HumanSingleGraveyardContextV13>,
    ),
    Error,
> {
    if observation.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    if actions.is_empty() {
        return Err(Error::UnsupportedPrompt);
    }
    let mut handles = Handles::from_parts(
        &observation.projection,
        &observation.own_hand,
        &observation.known_hand_cards,
        &observation.known_library_cards,
        observation.extensions.decision_local_library.as_ref(),
    )?;
    // Public stack and authenticated historical contexts establish producer
    // identities before relation or label lookup. No hidden inventory scan.
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &observation.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    for c in contexts {
        handles.insert(&c.source)?;
        for reference in c.selected.iter().chain(&c.legal_choices) {
            handles.insert(reference)?;
        }
    }
    let initial = super::life_ward_v8::project_state(observation, &mut handles)?;
    let mut unary = BTreeMap::<String, Vec<String>>::new();
    for row in &initial.extensions.ward_life_card_traits {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    // Exact ordered selected and legal relations participate before handles
    // are canonicalized. Only safe DTO facts enter these roles.
    for c in project_contexts(contexts, &handles)? {
        let relation = visible::without_handles(&c)?;
        unary
            .entry(c.source.handle.clone())
            .or_default()
            .push(format!("zombie-source:{relation}"));
        for (i, selected) in c.selected.iter().enumerate() {
            unary
                .entry(selected.handle.clone())
                .or_default()
                .push(format!("zombie-selected:{i}:{relation}"));
        }
        for legal in &c.legal_choices {
            unary
                .entry(legal.handle.clone())
                .or_default()
                .push(format!("zombie-legal:{relation}"));
        }
    }
    for rows in unary.values_mut() {
        rows.sort();
    }
    handles.canonicalize_parts(&initial, unary)?;
    let state = super::life_ward_v8::project_state(observation, &mut handles)?;
    let mut choices = actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            Ok((
                label(action, observation, contexts, &handles, human)?,
                u32::try_from(index).map_err(|_| Error::InvalidAction)?,
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    choices.sort_by(|a, b| a.0.cmp(&b.0));
    if choices.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::UnsupportedPrompt);
    }
    let engine_indexes = choices.iter().map(|(_, index)| *index).collect();
    let actions = choices
        .into_iter()
        .enumerate()
        .map(|(index, (label, _))| HumanLegalActionV1 {
            action_index: index as u32,
            label,
        })
        .collect();
    let contexts = project_contexts(contexts, &handles)?;
    Ok((state, actions, engine_indexes, contexts))
}

struct BoundZombieV13 {
    authority: AuthorityV13,
    expected: FastActorDecisionV1,
    binding: HumanDecisionBindingV8,
    public: crate::soul_shackled_zombie_public_v1::ZombiePublicObservationV1,
    visible: HumanDecisionZombieV13,
    engine_indexes: Vec<u32>,
}
pub struct HumanZombieProjectorV13 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundZombieV13>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV13)>,
}
impl HumanZombieProjectorV13 {
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
        session: &HumanZombieSessionV13,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionZombieV13, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .human_current_decision_input_v8(expected, self.human_seat)
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
        let (old, actions, engine_indexes, single_graveyard_triggers) =
            project_base(&observation, &actions, self.human_seat, &public.contexts)?;
        let flags = public.predecessor.extensions.attacked_this_turn;
        let seat = if self.human_seat == PlayerSeatV1::P0 {
            0
        } else {
            1
        };
        let visible = HumanDecisionZombieV13 {
            schema: "mtg-kernel-human-soul-shackled-zombie-decision/v13".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state: HumanVisibleStateV1 {
                public: old.public,
                own_hand: old.own_hand,
                known_library_cards: old.known_library_cards,
                known_hand_cards: old.known_hand_cards,
                extensions: HumanExtensionsV9 {
                    predecessor: old.extensions,
                    own_attacked_this_turn: flags[seat],
                    opponent_attacked_this_turn: flags[1 - seat],
                },
                policy_context: old.policy_context,
                known_face_down_cards: old.known_face_down_cards,
                poison_counters: old.poison_counters,
                poison_prevention: old.poison_prevention,
                creatures_attacked_this_turn: old.creatures_attacked_this_turn,
                ninja_emblems: old.ninja_emblems,
                restricted_mana: old.restricted_mana,
            },
            single_graveyard_triggers,
            actions,
        };
        self.pending = Some(BoundZombieV13 {
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
        session: &mut HumanZombieSessionV13,
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
            .human_validate_bound_decision_v8(pending.expected, self.human_seat, &pending.binding)
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
