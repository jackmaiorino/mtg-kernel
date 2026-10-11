//! Sealed explicit Heartfire label successor over the V9 public facts.
//! V10 and V11 remain reserved for the separate keep-one and Ooze consumers.
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
struct AuthorityV12 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV12 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV12 {}
pub struct HumanHeartfireSessionV12 {
    inner: FastActorSessionV1,
    authority: AuthorityV12,
}
#[derive(Clone)]
pub struct HumanHeartfireSnapshotV12(FastActorSessionSnapshotV1);
impl HumanHeartfireSessionV12 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_human_v8()?,
            authority: AuthorityV12 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v12(&self) -> HumanHeartfireSnapshotV12 {
        HumanHeartfireSnapshotV12(self.inner.snapshot_v1())
    }
    pub fn restore_v12(&mut self, snapshot: &HumanHeartfireSnapshotV12) -> Result<(), Error> {
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
    ) -> Result<crate::heartfire_public_v1::HeartfirePublicObservationV1, crate::rl::RlContractError>
    {
        self.inner
            .human_heartfire_public_v12(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale Heartfire public decision".into()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionHeartfireV12 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanHeartfireExtensionsV12>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanHeartfireSavedDamageV12 {
    pub source: HumanCardRefV1,
    pub stack_index: u32,
    pub signed_departing_power: i32,
    pub damage_if_target_legal_before_prevention: i32,
    pub captured_controller: PlayerSeatV1,
    pub captured_color_mask: u8,
    pub captured_lifelink: bool,
    pub captured_deathtouch: bool,
    pub captured_werewolf: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanHeartfireExtensionsV12 {
    pub predecessor: HumanExtensionsV9,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub saved_damage: Vec<HumanHeartfireSavedDamageV12>,
}
fn saved_damage(
    observation: &ObservationV8,
    public: &crate::heartfire_public_v1::HeartfirePublicObservationV1,
    handles: &Handles,
) -> Result<Vec<HumanHeartfireSavedDamageV12>, Error> {
    public
        .saved_damage
        .iter()
        .map(|saved| {
            let mut sources = observation
                .extensions
                .historical_public_sources
                .iter()
                .filter(|source| {
                    source.context
                        == crate::policy_observation_v7::HistoricalSourceContextV7::Stack {
                            stack_index: saved.stack_index,
                        }
                });
            let source = sources.next().ok_or(Error::InvalidVisibleReference)?;
            if sources.next().is_some()
                || source.stack_item_kind != crate::rl::StackItemKindV2::ActivatedAbility
                || source.source.card_db_id != saved.root.card_id
                || source.source.controller != saved.captured_controller
            {
                return Err(Error::InvalidVisibleReference);
            }
            Ok(HumanHeartfireSavedDamageV12 {
                source: handles.get(&source.source)?.clone(),
                stack_index: saved.stack_index,
                signed_departing_power: saved.signed_departing_power,
                damage_if_target_legal_before_prevention: saved
                    .damage_if_target_legal_before_prevention,
                captured_controller: saved.captured_controller,
                captured_color_mask: saved.captured_color_mask,
                captured_lifelink: saved.captured_lifelink,
                captured_deathtouch: saved.captured_deathtouch,
                captured_werewolf: saved.captured_werewolf,
            })
        })
        .collect()
}
fn wrap_state(
    old: HumanVisibleStateV8,
    flags: [bool; 2],
    human: PlayerSeatV1,
    saved_damage: Vec<HumanHeartfireSavedDamageV12>,
) -> HumanVisibleStateV1<HumanHeartfireExtensionsV12> {
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        policy_context: old.policy_context,
        known_face_down_cards: old.known_face_down_cards,
        poison_counters: old.poison_counters,
        poison_prevention: old.poison_prevention,
        creatures_attacked_this_turn: old.creatures_attacked_this_turn,
        ninja_emblems: old.ninja_emblems,
        restricted_mana: old.restricted_mana,
        extensions: HumanHeartfireExtensionsV12 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: flags[seat],
                opponent_attacked_this_turn: flags[1 - seat],
            },
            saved_damage,
        },
    }
}

struct ProjectedHeartfireV12 {
    state: HumanVisibleStateV1<HumanHeartfireExtensionsV12>,
    actions: Vec<HumanLegalActionV1>,
    engine_indexes: Vec<u32>,
}
fn project_base(
    observation: &ObservationV8,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    public: &crate::heartfire_public_v1::HeartfirePublicObservationV1,
) -> Result<ProjectedHeartfireV12, Error> {
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
    let initial = super::life_ward_v8::project_state(observation, &mut handles)?;
    let mut unary = BTreeMap::<String, Vec<String>>::new();
    for row in &initial.extensions.ward_life_card_traits {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in saved_damage(observation, public, &handles)? {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(&row)?);
    }
    for rows in unary.values_mut() {
        rows.sort();
    }
    handles.canonicalize_parts(&initial, unary)?;
    let old = super::life_ward_v8::project_state(observation, &mut handles)?;
    let state = wrap_state(
        old,
        public.predecessor.extensions.attacked_this_turn,
        human,
        saved_damage(observation, public, &handles)?,
    );
    let mut choices = actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            Ok((
                super::labels::label_heartfire_v12(action, observation, &handles, human)?,
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
    Ok(ProjectedHeartfireV12 {
        state,
        actions,
        engine_indexes,
    })
}

struct BoundHeartfireV12 {
    authority: AuthorityV12,
    expected: FastActorDecisionV1,
    binding: HumanDecisionBindingV8,
    public: crate::heartfire_public_v1::HeartfirePublicObservationV1,
    visible: HumanDecisionHeartfireV12,
    engine_indexes: Vec<u32>,
}
pub struct HumanHeartfireProjectorV12 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundHeartfireV12>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV12)>,
}
impl HumanHeartfireProjectorV12 {
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
        session: &HumanHeartfireSessionV12,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionHeartfireV12, Error> {
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
        let ProjectedHeartfireV12 {
            state,
            actions,
            engine_indexes,
        } = project_base(&observation, &actions, self.human_seat, &public)?;
        let visible = HumanDecisionHeartfireV12 {
            schema: "mtg-kernel-human-heartfire-decision/v12".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundHeartfireV12 {
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
        session: &mut HumanHeartfireSessionV12,
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
