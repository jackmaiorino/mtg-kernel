//! Explicit life-aware human successor. Only the selected safe DTO and the
//! existing prompt/action request and receipt cross the transport boundary.
//! The complete V8 observation and executable binding stay private.

use super::visible::{self, Handles, Project};
use super::*;
use crate::policy_observation_v7::HistoricalSourceContextV7;
use crate::policy_observation_v8::{ObservationV8, WardLifePaymentV8, WardLifePrintedFaceV8};
use crate::rl::{ActionSemanticV1, CardStableRefV1, StackItemKindV2};
use crate::rl_session::human_v8::HumanDecisionBindingV8;
use std::collections::BTreeMap;

type Error = HumanDecisionErrorV1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionV8 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV8,
    pub actions: Vec<HumanLegalActionV1>,
}

pub type HumanVisibleStateV8 = HumanVisibleStateV1<HumanExtensionsV8>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanHistoricalSourceV8 {
    pub context: HistoricalSourceContextV7,
    pub source: HumanCardRefV1,
    pub stack_item_kind: StackItemKindV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanStackRefV8 {
    pub stack_index: u32,
    pub source: HumanCardRefV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanWardLifePaymentV8 {
    pub source: HumanCardRefV1,
    pub targeting_item: HumanStackRefV8,
    pub payer: PlayerSeatV1,
    pub payer_life: i32,
    pub life: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanQueuedWardLifePaymentV8 {
    pub stack_index: u32,
    pub payment: HumanWardLifePaymentV8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanWardLifeCardTraitV8 {
    pub source: HumanCardRefV1,
    pub life: u8,
    pub printed_face: WardLifePrintedFaceV8,
    pub effective: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanExtensionsV8 {
    pub pending_cast_object_cost: Option<HumanCastObjectCostV1>,
    pub decision_local_library: Option<HumanLibraryChoiceV1>,
    pub historical_public_sources: Vec<HumanHistoricalSourceV8>,
    pub pending_chosen_creature_cost: Option<HumanChosenCreatureCostV1>,
    pub finalized_chosen_creature_costs: Vec<HumanFinalizedCreatureCostV1>,
    pub pending_ward_payment: Option<crate::policy_observation_v6::WardPaymentV6>,
    pub queued_ward_payments: Vec<crate::policy_observation_v6::QueuedWardPaymentV6>,
    pub pending_ward_life_payment: Option<HumanWardLifePaymentV8>,
    pub queued_ward_life_payments: Vec<HumanQueuedWardLifePaymentV8>,
    pub ward_life_card_traits: Vec<HumanWardLifeCardTraitV8>,
}

fn stack_ref(
    observation: &ObservationV8,
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
    observation: &ObservationV8,
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

pub(super) fn project_state(
    observation: &ObservationV8,
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

fn project_decision(
    observation: &ObservationV8,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionV8, Vec<u32>), Error> {
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
    for rows in unary.values_mut() {
        rows.sort();
    }
    handles.canonicalize_parts(&initial, unary)?;
    let state = project_state(observation, &mut handles)?;
    let mut choices = actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            Ok((
                super::labels::label_v8(action, observation, &handles, human)?,
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
    Ok((
        HumanDecisionV8 {
            schema: "mtg-kernel-human-decision/v8".into(),
            human_seat: human,
            prompt_seq,
            state,
            actions,
        },
        engine_indexes,
    ))
}

// No Debug or Serialize for the private origin, engine mapping or binding.
struct BoundHumanDecisionV8 {
    expected: FastActorDecisionV1,
    binding: HumanDecisionBindingV8,
    visible: HumanDecisionV8,
    engine_action_indexes: Vec<u32>,
}

pub struct HumanDecisionProjectorV8 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundHumanDecisionV8>,
    accepted: Option<HumanActionReceiptV1>,
}

impl HumanDecisionProjectorV8 {
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
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionV8, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .human_current_decision_input_v8(expected, self.human_seat)
            .map_err(|_| Error::StaleDecision)?;
        if let Some(pending) = &self.pending {
            if pending.expected == expected && pending.binding == binding {
                return Ok(pending.visible.clone());
            }
        }
        let next = self
            .next_prompt_seq
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        let (visible, engine_action_indexes) = project_decision(
            &observation,
            &actions,
            self.human_seat,
            self.next_prompt_seq,
        )?;
        self.pending = Some(BoundHumanDecisionV8 {
            expected,
            binding,
            visible: visible.clone(),
            engine_action_indexes,
        });
        self.next_prompt_seq = next;
        Ok(visible)
    }

    /// Immediate identical retry returns its receipt without applying twice.
    /// These in-memory receipts do not provide durable transport recovery.
    pub fn submit(
        &mut self,
        session: &mut FastActorSessionV1,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, Error> {
        if let Some(receipt) = &self.accepted {
            if receipt.prompt_seq == request.prompt_seq {
                return if receipt.action_index == request.action_index {
                    Ok(receipt.clone())
                } else {
                    Err(Error::ConflictingRetry)
                };
            }
        }
        let pending = self.pending.as_ref().ok_or(Error::StaleDecision)?;
        if pending.visible.prompt_seq != request.prompt_seq {
            return Err(Error::StaleDecision);
        }
        let engine_index = *pending
            .engine_action_indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        session
            .human_validate_bound_decision_v8(pending.expected, self.human_seat, &pending.binding)
            .map_err(|_| Error::StaleDecision)?;
        let result = session.step(
            pending.expected.episode_id,
            pending.expected.step,
            engine_index,
        );
        self.pending = None;
        result.map_err(|_| Error::ExecutionFailed)?;
        let receipt = HumanActionReceiptV1 {
            prompt_seq: request.prompt_seq,
            action_index: request.action_index,
        };
        self.accepted = Some(receipt.clone());
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests;
