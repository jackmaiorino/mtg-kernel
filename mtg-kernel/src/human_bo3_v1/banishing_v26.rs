//! Sealed Banishing successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::banishing_public_v1::BanishingObservationV1;
use crate::rl::ActionSemanticV1;
use crate::rl_session::banishing_v26::BanishingDecisionBindingV26;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV26 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV26 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV26 {}
pub struct HumanBanishingSessionV26 {
    inner: FastActorSessionV1,
    authority: AuthorityV26,
}
#[derive(Clone)]
pub struct HumanBanishingSnapshotV26 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanBanishingSessionV26 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_banishing_v26()?,
            authority: AuthorityV26 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v26(&self) -> HumanBanishingSnapshotV26 {
        HumanBanishingSnapshotV26 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v26(&mut self, snapshot: &HumanBanishingSnapshotV26) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .banishing_restore_v26(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::banishing_public_v1::BanishingObservationV1, crate::rl::RlContractError>
    {
        self.inner
            .banishing_current_decision_input_v26(expected, expected.acting_player)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Banishing public decision".into()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionBanishingV26 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanBanishingExtensionsV26>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanBanishingTriggerV26 {
    pub source_at_entry: HumanCardRefV1,
    pub captured_controller: PlayerSeatV1,
    pub original_source_present: bool,
    pub phase: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanBanishingLinkV26 {
    pub source_at_entry: HumanCardRefV1,
    pub exiled_recipient: HumanCardRefV1,
    pub recipient_owner: PlayerSeatV1,
    pub recipient_was_token: bool,
    pub expired: bool,
    pub duration_ended: bool,
    pub return_pending: bool,
    pub recipient_ceased: bool,
    pub outcome: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanBanishingAttachmentV26 {
    pub aura: HumanCardRefV1,
    pub owner: PlayerSeatV1,
    pub legal_hosts: Vec<HumanCardRefV1>,
    pub untargeted: bool,
    pub mandatory: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanBanishingConfirmedAttachmentV26 {
    pub aura: HumanCardRefV1,
    pub owner: PlayerSeatV1,
    pub current_chosen_host: Option<HumanCardRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanBanishingExtensionsV26 {
    pub predecessor: HumanExtensionsV9,
    pub triggers: Vec<HumanBanishingTriggerV26>,
    pub links: Vec<HumanBanishingLinkV26>,
    pub attachment_choice: Option<HumanBanishingAttachmentV26>,
    pub confirmed_attachments: Vec<HumanBanishingConfirmedAttachmentV26>,
}
fn parts(observation: &BanishingObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
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
    observation: &BanishingObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanBanishingExtensionsV26>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let triggers = observation
        .triggers
        .iter()
        .map(|row| {
            Ok(HumanBanishingTriggerV26 {
                source_at_entry: handles.get(&row.source_at_entry)?.clone(),
                captured_controller: row.captured_controller,
                original_source_present: row.original_source_present,
                phase: row.phase.clone(),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let links = observation
        .links
        .iter()
        .map(|row| {
            Ok(HumanBanishingLinkV26 {
                source_at_entry: handles.get(&row.source_at_entry)?.clone(),
                exiled_recipient: handles.get(&row.exiled_recipient)?.clone(),
                recipient_owner: row.recipient_owner,
                recipient_was_token: row.recipient_was_token,
                expired: row.expired,
                duration_ended: row.duration_ended,
                return_pending: row.return_pending,
                recipient_ceased: row.recipient_ceased,
                outcome: row.outcome.as_ref().map(|reason| {
                    match reason {
                        crate::banishing_light_v1::NoReturnV1::RecipientDeparted => {
                            "recipient_left_exile"
                        }
                        crate::banishing_light_v1::NoReturnV1::TokenCannotReturn => {
                            "token_cannot_return"
                        }
                        crate::banishing_light_v1::NoReturnV1::AuraNoLegalHost { .. } => {
                            "aura_no_legal_host"
                        }
                    }
                    .into()
                }),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let attachment_choice = observation
        .attachment_choice
        .as_ref()
        .map(|row| -> Result<HumanBanishingAttachmentV26, Error> {
            Ok(HumanBanishingAttachmentV26 {
                aura: handles.get(&row.aura)?.clone(),
                owner: row.owner,
                untargeted: row.untargeted,
                mandatory: row.mandatory,
                legal_hosts: row
                    .legal_hosts
                    .iter()
                    .map(|host| Ok(handles.get(host)?.clone()))
                    .collect::<Result<Vec<_>, Error>>()?,
            })
        })
        .transpose()?;
    let confirmed_attachments = observation
        .confirmed_attachments
        .iter()
        .map(|row| {
            Ok(HumanBanishingConfirmedAttachmentV26 {
                aura: handles.get(&row.aura)?.clone(),
                owner: row.owner,
                current_chosen_host: row
                    .current_chosen_host
                    .as_ref()
                    .map(|host| -> Result<HumanCardRefV1, Error> { Ok(handles.get(host)?.clone()) })
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanBanishingExtensionsV26 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            triggers,
            links,
            attachment_choice,
            confirmed_attachments,
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
type ProjectedBanishingV26 = (
    HumanVisibleStateV1<HumanBanishingExtensionsV26>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

fn project_base(
    observation: &BanishingObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedBanishingV26, Error> {
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
    for row in &observation.triggers {
        handles.insert(&row.source_at_entry)?;
    }
    for row in &observation.links {
        handles.insert(&row.source_at_entry)?;
        handles.insert(&row.exiled_recipient)?;
    }
    if let Some(choice) = &observation.attachment_choice {
        handles.insert(&choice.aura)?;
        for host in &choice.legal_hosts {
            handles.insert(host)?;
        }
    }
    for row in &observation.confirmed_attachments {
        handles.insert(&row.aura)?;
        if let Some(host) = &row.current_chosen_host {
            handles.insert(host)?;
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
    for row in &initial.extensions.triggers {
        unary
            .entry(row.source_at_entry.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &initial.extensions.links {
        let facts = visible::without_handles(row)?;
        unary
            .entry(row.source_at_entry.handle.clone())
            .or_default()
            .push(facts.clone());
        unary
            .entry(row.exiled_recipient.handle.clone())
            .or_default()
            .push(facts);
    }
    if let Some(row) = &initial.extensions.attachment_choice {
        unary
            .entry(row.aura.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        for host in &row.legal_hosts {
            unary
                .entry(host.handle.clone())
                .or_default()
                .push("legal_untargeted_attachment_host".into());
        }
    }
    for row in &initial.extensions.confirmed_attachments {
        let facts = visible::without_handles(row)?;
        unary
            .entry(row.aura.handle.clone())
            .or_default()
            .push(facts.clone());
        if let Some(host) = &row.current_chosen_host {
            unary.entry(host.handle.clone()).or_default().push(facts);
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

fn label(
    action: &ActionSemanticV1,
    observation: &BanishingObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let Some(choice) = &observation.attachment_choice {
        let ActionSemanticV1::ChooseEffectTarget {
            actor,
            source,
            target: crate::rl::TargetRefV1::Object { object },
            selected_count: 0,
            min_targets: 1,
            max_targets: 1,
        } = action
        else {
            return Err(Error::UnsupportedPrompt);
        };
        if *actor != human
            || choice.owner != human
            || *source != choice.aura
            || !choice.legal_hosts.contains(object)
        {
            return Err(Error::InvalidAction);
        }
        return Ok(format!(
            "Attach returning {} to {} (owner choice, untargeted)",
            handles.name(source)?,
            handles.name(object)?
        ));
    }
    if let ActionSemanticV1::CastSpell { actor, source, .. } = action {
        if observation
            .roots
            .iter()
            .any(|root| root.card_id == source.card_db_id)
        {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            return Ok(format!("Begin casting {} (printed cost {{2}}{{W}}): its entry triggers mandatory exile of an opponent nonland permanent until this exact source leaves",handles.name(source)?));
        }
    }
    super::labels::label_parts_v8(action, &parts(observation), handles, human)
}

struct BoundBanishingV26 {
    authority: AuthorityV26,
    expected: FastActorDecisionV1,
    binding: BanishingDecisionBindingV26,
    public: crate::banishing_public_v1::BanishingObservationV1,
    visible: HumanDecisionBanishingV26,
    engine_indexes: Vec<u32>,
}
pub struct HumanBanishingProjectorV26 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundBanishingV26>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV26)>,
}
impl HumanBanishingProjectorV26 {
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
        session: &HumanBanishingSessionV26,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionBanishingV26, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .banishing_current_decision_input_v26(expected, self.human_seat)
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
        let visible = HumanDecisionBanishingV26 {
            schema: "mtg-kernel-human-banishing-decision/v26".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundBanishingV26 {
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
        session: &mut HumanBanishingSessionV26,
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
            .banishing_validate_bound_decision_v26(
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
