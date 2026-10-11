//! Independent captured-source Human input and safe projection successor.
use super::visible::{self, Handles, Project};
use super::*;
use crate::heartfire_history_v1::HeartfireHistoryObservationV1;
use crate::policy_observation_v8::WardLifePaymentV8;
use crate::rl::ActionSemanticV1;
use crate::rl::{CardStableRefV1, StackItemKindV2};
use crate::rl_session::heartfire_history_v1::HeartfireHistoryDecisionBindingV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct HistoryAuthorityV1 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for HistoryAuthorityV1 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for HistoryAuthorityV1 {}
pub struct HumanHeartfireHistorySessionV1 {
    inner: FastActorSessionV1,
    authority: HistoryAuthorityV1,
}
#[derive(Clone)]
pub struct HumanHeartfireHistorySnapshotV1(FastActorSessionSnapshotV1);
impl HumanHeartfireHistorySessionV1 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_heartfire_history_v1()?,
            authority: HistoryAuthorityV1 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v1(&self) -> HumanHeartfireHistorySnapshotV1 {
        HumanHeartfireHistorySnapshotV1(self.inner.snapshot_v1())
    }
    pub fn restore_v1(&mut self, snapshot: &HumanHeartfireHistorySnapshotV1) -> Result<(), Error> {
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
    ) -> Result<HeartfireHistoryObservationV1, crate::rl::RlContractError> {
        self.inner
            .heartfire_history_current_input_v1(expected, expected.acting_player)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Heartfire public decision".into()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionHeartfireHistoryV1 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanHeartfireHistoryExtensionsV1>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanHeartfireHistorySavedDamageV1 {
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
pub struct HumanHeartfireHistoryExtensionsV1 {
    pub common: HumanExtensionsV9,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub saved_damage: Vec<HumanHeartfireHistorySavedDamageV1>,
}
fn saved_damage(
    observation: &HeartfireHistoryObservationV1,
    public: &HeartfireHistoryObservationV1,
    handles: &Handles,
) -> Result<Vec<HumanHeartfireHistorySavedDamageV1>, Error> {
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
            Ok(HumanHeartfireHistorySavedDamageV1 {
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
    saved_damage: Vec<HumanHeartfireHistorySavedDamageV1>,
) -> HumanVisibleStateV1<HumanHeartfireHistoryExtensionsV1> {
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
        extensions: HumanHeartfireHistoryExtensionsV1 {
            common: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: flags[seat],
                opponent_attacked_this_turn: flags[1 - seat],
            },
            saved_damage,
        },
    }
}

struct ProjectedHeartfireHistoryV1 {
    state: HumanVisibleStateV1<HumanHeartfireHistoryExtensionsV1>,
    actions: Vec<HumanLegalActionV1>,
    engine_indexes: Vec<u32>,
}
fn project_base(
    observation: &HeartfireHistoryObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    public: &HeartfireHistoryObservationV1,
) -> Result<ProjectedHeartfireHistoryV1, Error> {
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
    let initial = project_state(observation, &mut handles)?;
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
    let old = project_state(observation, &mut handles)?;
    let state = wrap_state(
        old,
        public.attacked_this_turn,
        human,
        saved_damage(observation, public, &handles)?,
    );
    let mut choices = actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            Ok((
                super::labels::label_heartfire_history_v1(action, observation, &handles, human)?,
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
    Ok(ProjectedHeartfireHistoryV1 {
        state,
        actions,
        engine_indexes,
    })
}

struct BoundHeartfireHistoryV1 {
    authority: HistoryAuthorityV1,
    expected: FastActorDecisionV1,
    binding: HeartfireHistoryDecisionBindingV1,
    public: HeartfireHistoryObservationV1,
    visible: HumanDecisionHeartfireHistoryV1,
    engine_indexes: Vec<u32>,
}
pub struct HumanHeartfireHistoryProjectorV1 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundHeartfireHistoryV1>,
    accepted: Option<(HumanActionReceiptV1, HistoryAuthorityV1)>,
}
impl HumanHeartfireHistoryProjectorV1 {
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
        session: &HumanHeartfireHistorySessionV1,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionHeartfireHistoryV1, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .heartfire_history_current_input_v1(expected, self.human_seat)
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
        let ProjectedHeartfireHistoryV1 {
            state,
            actions,
            engine_indexes,
        } = project_base(&observation, &actions, self.human_seat, &public)?;
        let visible = HumanDecisionHeartfireHistoryV1 {
            schema: "mtg-kernel-human-heartfire-history-decision/v1".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundHeartfireHistoryV1 {
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
        session: &mut HumanHeartfireHistorySessionV1,
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
            .heartfire_history_validate_bound_v1(
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

fn stack_ref(
    observation: &HeartfireHistoryObservationV1,
    index: u32,
    handles: &Handles,
) -> Result<HumanStackRefV8, Error> {
    let item = observation
        .projection
        .surface
        .stack
        .get(index as usize)
        .ok_or(Error::InvalidVisibleReference)?;
    if item.stack_index != index {
        return Err(Error::InvalidVisibleReference);
    }
    Ok(HumanStackRefV8 {
        stack_index: index,
        source: handles.get(&item.source)?.clone(),
    })
}

fn payment(
    observation: &HeartfireHistoryObservationV1,
    source: &CardStableRefV1,
    fee: &WardLifePaymentV8,
    handles: &Handles,
) -> Result<HumanWardLifePaymentV8, Error> {
    let targeting_item = stack_ref(observation, fee.targeting_stack_index, handles)?;
    let targeter = &observation.projection.surface.stack[fee.targeting_stack_index as usize];
    if targeter.controller != fee.payer {
        return Err(Error::InvalidVisibleReference);
    }
    Ok(HumanWardLifePaymentV8 {
        source: handles.get(source)?.clone(),
        targeting_item,
        payer: fee.payer,
        payer_life: observation.projection.surface.life_totals[match fee.payer {
            PlayerSeatV1::P0 => 0,
            PlayerSeatV1::P1 => 1,
        }],
        life: fee.life,
    })
}

fn project_state(
    observation: &HeartfireHistoryObservationV1,
    handles: &mut Handles,
) -> Result<HumanVisibleStateV8, Error> {
    // Select unchanged common facts into their safe DTO directly. This never
    // constructs, hashes or labels a purported predecessor observation.
    let common = visible::project_state_parts(
        &observation.projection,
        &observation.own_hand,
        &observation.known_library_cards,
        &observation.known_hand_cards,
        &observation.known_face_down_cards,
        |handles| {
            Ok(HumanExtensionsV1 {
                pending_cast_object_cost: observation
                    .extensions
                    .pending_cast_object_cost
                    .project(handles)?,
                decision_local_library: observation
                    .extensions
                    .decision_local_library
                    .project(handles)?,
                historical_public_sources: Vec::new(),
                pending_chosen_creature_cost: observation
                    .extensions
                    .pending_chosen_creature_cost
                    .project(handles)?,
                finalized_chosen_creature_costs: observation
                    .extensions
                    .finalized_chosen_creature_costs
                    .project(handles)?,
                pending_ward_payment: observation.extensions.pending_ward_payment.clone(),
                queued_ward_payments: observation.extensions.queued_ward_payments.clone(),
            })
        },
        handles,
    )?;
    let historical_public_sources = observation
        .extensions
        .historical_public_sources
        .iter()
        .map(|row| {
            Ok(HumanHistoricalSourceV8 {
                context: row.context,
                source: handles.get(&row.source)?.clone(),
                stack_item_kind: row.stack_item_kind,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let pending_ward_life_payment = observation
        .extensions
        .pending_ward_life_payment
        .as_ref()
        .map(|fee| {
            let source = observation
                .projection
                .surface
                .engine_context
                .pending_effect
                .as_ref()
                .and_then(|pending| pending.source.as_ref())
                .ok_or(Error::InvalidVisibleReference)?;
            payment(observation, source, fee, handles)
        })
        .transpose()?;
    let queued_ward_life_payments = observation
        .extensions
        .queued_ward_life_payments
        .iter()
        .map(|row| {
            let item = observation
                .projection
                .surface
                .stack
                .get(row.stack_index as usize)
                .ok_or(Error::InvalidVisibleReference)?;
            if item.stack_index != row.stack_index
                || item.stack_item_kind != StackItemKindV2::TriggeredAbility
            {
                return Err(Error::InvalidVisibleReference);
            }
            Ok(HumanQueuedWardLifePaymentV8 {
                stack_index: row.stack_index,
                payment: payment(observation, &item.source, &row.payment, handles)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let mut ward_life_card_traits = observation
        .extensions
        .ward_life_card_traits
        .iter()
        .map(|row| {
            Ok(HumanWardLifeCardTraitV8 {
                source: handles.get(&row.source)?.clone(),
                life: row.life,
                printed_face: row.printed_face,
                effective: row.effective,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    visible::sort_visible(&mut ward_life_card_traits);
    let old = common.extensions;
    Ok(HumanVisibleStateV1 {
        public: common.public,
        own_hand: common.own_hand,
        known_library_cards: common.known_library_cards,
        known_hand_cards: common.known_hand_cards,
        policy_context: common.policy_context,
        known_face_down_cards: common.known_face_down_cards,
        poison_counters: common.poison_counters,
        poison_prevention: common.poison_prevention,
        creatures_attacked_this_turn: common.creatures_attacked_this_turn,
        ninja_emblems: common.ninja_emblems,
        restricted_mana: common.restricted_mana,
        extensions: HumanExtensionsV8 {
            pending_cast_object_cost: old.pending_cast_object_cost,
            decision_local_library: old.decision_local_library,
            historical_public_sources,
            pending_chosen_creature_cost: old.pending_chosen_creature_cost,
            finalized_chosen_creature_costs: old.finalized_chosen_creature_costs,
            pending_ward_payment: old.pending_ward_payment,
            queued_ward_payments: old.queued_ward_payments,
            pending_ward_life_payment,
            queued_ward_life_payments,
            ward_life_card_traits,
        },
    })
}

#[cfg(test)]
mod tests;
