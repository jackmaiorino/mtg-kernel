//! Sealed Arahbo successor. Complete typed arahbo stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::arahbo_public_v1::ArahboObservationV1;
use crate::rl::ActionSemanticV1;
use crate::rl_session::arahbo_v31::ArahboDecisionBindingV31;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only arahbo survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct OwnerV31 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for OwnerV31 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for OwnerV31 {}
pub struct HumanArahboSessionV31 {
    inner: FastActorSessionV1,
    arahbo: OwnerV31,
}
#[derive(Clone)]
pub struct HumanArahboSnapshotV31 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanArahboSessionV31 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_arahbo_v31()?,
            arahbo: OwnerV31 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v31(&self) -> HumanArahboSnapshotV31 {
        HumanArahboSnapshotV31 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.arahbo.owner.clone(),
        }
    }
    pub fn restore_v31(&mut self, snapshot: &HumanArahboSnapshotV31) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.arahbo.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .arahbo
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .arahbo_restore_v31(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.arahbo.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::arahbo_public_v1::ArahboObservationV1, crate::rl::RlContractError> {
        self.inner
            .arahbo_current_decision_input_v31(expected, expected.acting_player)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Arahbo public decision".into()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionArahboV31 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanArahboExtensionsV31>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanArahboOccurrenceV31 {
    pub entrant_at_entry: HumanCardRefV1,
    pub entrant_face_index: u8,
    pub root: crate::arahbo_v1::RootV1,
    pub entrant_was_token: bool,
    pub created_token: Option<HumanCardRefV1>,
    pub observer_at_entry: HumanCardRefV1,
    pub beneficiary: PlayerSeatV1,
    pub event_order: u64,
    pub lifecycle: crate::arahbo_public_v1::EntryLifecycleV1,
    pub stack_index: Option<u32>,
    pub pending_order_index: Option<u32>,
    pub original_source_present: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanArahboSourceV31 {
    pub source: HumanCardRefV1,
    pub controller: PlayerSeatV1,
    pub other_controlled_cat_creatures: Vec<HumanCardRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanArahboLinkedExileV31 {
    pub triggers: Vec<HumanBanishingTriggerV26>,
    pub links: Vec<HumanBanishingLinkV26>,
    pub attachment_choice: Option<HumanBanishingAttachmentV26>,
    pub confirmed_attachments: Vec<HumanBanishingConfirmedAttachmentV26>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanArahboExtensionsV31 {
    pub predecessor: HumanExtensionsV9,
    pub entry_occurrences: Vec<HumanArahboOccurrenceV31>,
    pub active_sources: Vec<HumanArahboSourceV31>,
    pub linked_exile: HumanArahboLinkedExileV31,
    pub garruk_occurrences: Vec<super::garruk_v30::HumanGarrukOccurrenceV30>,
    pub garruk_active_sources: Vec<super::garruk_v30::HumanGarrukSourceV30>,
}

fn parts(observation: &ArahboObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
    super::life_ward_v8::ProjectionPartsV8 {
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
        known_face_down_cards: &observation.known_face_down_cards,
        extensions: &observation.extensions,
    }
}
fn linked_state(
    observation: &crate::arahbo_public_v1::LinkedExileFactsV1,
    handles: &mut Handles,
) -> Result<HumanArahboLinkedExileV31, Error> {
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
    Ok(HumanArahboLinkedExileV31 {
        triggers,
        links,
        attachment_choice,
        confirmed_attachments,
    })
}
fn state(
    observation: &ArahboObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanArahboExtensionsV31>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let mut entry_occurrences = observation
        .entry_occurrences
        .iter()
        .map(|row| {
            Ok(HumanArahboOccurrenceV31 {
                entrant_at_entry: handles.get(&row.entrant_at_entry)?.clone(),
                entrant_face_index: row.entrant_face_index,
                root: row.root,
                entrant_was_token: row.entrant_was_token,
                created_token: row
                    .created_token
                    .as_ref()
                    .map(|token| handles.get(token).cloned())
                    .transpose()?,
                observer_at_entry: handles.get(&row.observer_at_entry)?.clone(),
                beneficiary: row.beneficiary,
                event_order: row.event_order,
                lifecycle: row.lifecycle,
                stack_index: row.stack_index,
                pending_order_index: row.pending_order_index,
                original_source_present: row.original_source_present,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    entry_occurrences.sort_by(|a, b| {
        (a.event_order, &a.observer_at_entry.handle)
            .cmp(&(b.event_order, &b.observer_at_entry.handle))
    });
    let mut active_sources = observation
        .active_sources
        .iter()
        .map(|row| {
            Ok(HumanArahboSourceV31 {
                source: handles.get(&row.source)?.clone(),
                controller: row.controller,
                other_controlled_cat_creatures: {
                    let mut recipients = row
                        .other_controlled_cat_creatures
                        .iter()
                        .map(|recipient| handles.get(recipient).cloned())
                        .collect::<Result<Vec<_>, Error>>()?;
                    recipients.sort_by(|a, b| a.handle.cmp(&b.handle));
                    recipients
                },
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    active_sources.sort_by(|a, b| a.source.handle.cmp(&b.source.handle));
    let mut garruk_occurrences = observation
        .garruk_occurrences
        .iter()
        .map(|row| {
            Ok(super::garruk_v30::HumanGarrukOccurrenceV30 {
                entrant_at_entry: handles.get(&row.entrant_at_entry)?.clone(),
                entrant_face_index: row.entrant_face_index,
                root: row.root,
                event_time_power: row.event_time_power,
                card_was_drawn: row.card_was_drawn,
                observer_at_entry: handles.get(&row.observer_at_entry)?.clone(),
                beneficiary: row.beneficiary,
                event_order: row.event_order,
                lifecycle: row.lifecycle,
                stack_index: row.stack_index,
                pending_order_index: row.pending_order_index,
                original_source_present: row.original_source_present,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    garruk_occurrences.sort_by(|a, b| {
        (a.event_order, &a.observer_at_entry.handle)
            .cmp(&(b.event_order, &b.observer_at_entry.handle))
    });
    let mut garruk_active_sources = observation
        .garruk_active_sources
        .iter()
        .map(|row| {
            let mut recipients = row
                .controlled_creatures_have_trample
                .iter()
                .map(|recipient| handles.get(recipient).cloned())
                .collect::<Result<Vec<_>, Error>>()?;
            recipients.sort_by(|a, b| a.handle.cmp(&b.handle));
            Ok(super::garruk_v30::HumanGarrukSourceV30 {
                source: handles.get(&row.source)?.clone(),
                controller: row.controller,
                controlled_creatures_have_trample: recipients,
                ferocious_now: row.ferocious_now,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    garruk_active_sources.sort_by(|a, b| a.source.handle.cmp(&b.source.handle));
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanArahboExtensionsV31 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            entry_occurrences,
            active_sources,
            linked_exile: linked_state(&observation.linked_exile, handles)?,
            garruk_occurrences,
            garruk_active_sources,
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
type ProjectedArahboV31 = (
    HumanVisibleStateV1<HumanArahboExtensionsV31>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

fn project_base(
    observation: &ArahboObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedArahboV31, Error> {
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
    for row in &observation.entry_occurrences {
        handles.insert(&row.entrant_at_entry)?;
        handles.insert(&row.observer_at_entry)?;
    }
    for row in &observation.active_sources {
        handles.insert(&row.source)?;
        for recipient in &row.other_controlled_cat_creatures {
            handles.insert(recipient)?;
        }
    }
    for row in &observation.linked_exile.triggers {
        handles.insert(&row.source_at_entry)?;
    }
    for row in &observation.linked_exile.links {
        handles.insert(&row.source_at_entry)?;
        handles.insert(&row.exiled_recipient)?;
    }
    if let Some(choice) = &observation.linked_exile.attachment_choice {
        handles.insert(&choice.aura)?;
        for host in &choice.legal_hosts {
            handles.insert(host)?;
        }
    }
    for row in &observation.linked_exile.confirmed_attachments {
        handles.insert(&row.aura)?;
        if let Some(host) = &row.current_chosen_host {
            handles.insert(host)?;
        }
    }
    for row in &observation.entry_occurrences {
        if let Some(token) = &row.created_token {
            handles.insert(token)?;
        }
    }
    for row in &observation.garruk_occurrences {
        handles.insert(&row.entrant_at_entry)?;
        handles.insert(&row.observer_at_entry)?;
    }
    for row in &observation.garruk_active_sources {
        handles.insert(&row.source)?;
        for recipient in &row.controlled_creatures_have_trample {
            handles.insert(recipient)?;
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
    for row in &initial.extensions.entry_occurrences {
        unary
            .entry(row.entrant_at_entry.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &initial.extensions.active_sources {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &initial.extensions.linked_exile.triggers {
        unary
            .entry(row.source_at_entry.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &initial.extensions.linked_exile.links {
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
    if let Some(row) = &initial.extensions.linked_exile.attachment_choice {
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
    for row in &initial.extensions.linked_exile.confirmed_attachments {
        let facts = visible::without_handles(row)?;
        unary
            .entry(row.aura.handle.clone())
            .or_default()
            .push(facts.clone());
        if let Some(host) = &row.current_chosen_host {
            unary.entry(host.handle.clone()).or_default().push(facts);
        }
    }
    for row in &initial.extensions.garruk_occurrences {
        unary
            .entry(row.entrant_at_entry.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &initial.extensions.garruk_active_sources {
        unary
            .entry(row.source.handle.clone())
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

fn label(
    action: &ActionSemanticV1,
    observation: &ArahboObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let Some(choice) = &observation.linked_exile.attachment_choice {
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
            .linked_exile
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
    if let ActionSemanticV1::CastSpell { actor, source, .. } = action {
        if observation
            .roots
            .iter()
            .any(|root| root.card_id == source.card_db_id)
        {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            return Ok(format!("Begin casting {} (printed cost {{2}}{{W}}): other Cats get +1/+1; its own entry or another nontoken Cat entering creates a 1/1 white Cat",handles.name(source)?));
        }
    }
    let ActionSemanticV1::OrderTriggers {
        actor,
        pending_sources,
        order,
    } = action
    else {
        return super::labels::label_parts_v8(action, &parts(observation), handles, human);
    };
    if !observation
        .entry_occurrences
        .iter()
        .any(|row| row.pending_order_index.is_some())
        && !observation
            .garruk_occurrences
            .iter()
            .any(|row| row.pending_order_index.is_some())
    {
        return super::labels::label_parts_v8(action, &parts(observation), handles, human);
    }
    let pending = &observation
        .projection
        .surface
        .engine_context
        .pending_triggers;
    let mut permutation = order.clone();
    permutation.sort_unstable();
    if *actor != human
        || pending_sources.len() < 2
        || pending_sources.len() != order.len()
        || permutation != (0..order.len()).collect::<Vec<_>>()
        || pending.len() < order.len()
    {
        return Err(Error::UnsupportedPrompt);
    }
    let mut descriptions = Vec::new();
    let mut ordinary = Vec::new();
    for &index in order {
        let source = &pending_sources[index];
        let trigger = &pending[index];
        if trigger.source.as_ref() != Some(source)
            || trigger.controller != human
            || trigger.trigger_kind != crate::rl::PendingTriggerKindV2::TriggeredAbility
        {
            return Err(Error::UnsupportedPrompt);
        }
        let rows: Vec<_> = observation
            .entry_occurrences
            .iter()
            .filter(|row| row.pending_order_index == Some(index as u32))
            .collect();
        if let [row] = rows.as_slice() {
            if row.observer_at_entry != *source
                || row.beneficiary != human
                || row.lifecycle != crate::arahbo_public_v1::EntryLifecycleV1::Pending
            {
                return Err(Error::InvalidVisibleReference);
            }
            descriptions.push(format!(
                "Create one 1/1 white Cat for {} from entry #{} of {} ({})",
                handles.name(source)?,
                row.event_order + 1,
                handles.name(&row.entrant_at_entry)?,
                match row.root {
                    crate::arahbo_v1::RootV1::OwnEntry => "its own entry",
                    crate::arahbo_v1::RootV1::OtherNontokenCatEntry => "another nontoken Cat entry",
                }
            ));
        } else if let Some(row) = observation
            .garruk_occurrences
            .iter()
            .find(|row| row.pending_order_index == Some(index as u32))
        {
            if row.observer_at_entry != *source
                || row.beneficiary != human
                || row.lifecycle != crate::garruk_public_v1::EntryLifecycleV1::Pending
            {
                return Err(Error::InvalidVisibleReference);
            }
            descriptions.push(match row.root {
             crate::garruks_uprising_v1::RootV1::OwnEtbFerocious => format!("Draw one if the original controller still controls a creature with power at least 4, from {} entry #{}",handles.name(source)?,row.event_order+1),
             crate::garruks_uprising_v1::RootV1::ControlledCreaturePowerEntry => format!("Draw one from {} for entry #{} of {} (entry power {})",handles.name(source)?,row.event_order+1,handles.name(&row.entrant_at_entry)?,row.event_time_power.ok_or(Error::InvalidVisibleReference)?),
           });
        } else if rows.is_empty() {
            if ordinary.contains(source) {
                return Err(Error::UnsupportedPrompt);
            }
            ordinary.push(source.clone());
            descriptions.push(format!("Trigger from {}", handles.name(source)?));
        } else {
            return Err(Error::InvalidVisibleReference);
        }
    }
    Ok(format!(
        "Put triggers on the stack from bottom to top (last resolves first): {}",
        descriptions.join("; then ")
    ))
}

struct BoundArahboV31 {
    arahbo: OwnerV31,
    expected: FastActorDecisionV1,
    binding: ArahboDecisionBindingV31,
    public: crate::arahbo_public_v1::ArahboObservationV1,
    visible: HumanDecisionArahboV31,
    engine_indexes: Vec<u32>,
}
pub struct HumanArahboProjectorV31 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundArahboV31>,
    accepted: Option<(HumanActionReceiptV1, OwnerV31)>,
}
impl HumanArahboProjectorV31 {
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
        session: &HumanArahboSessionV31,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionArahboV31, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .arahbo_current_decision_input_v31(expected, self.human_seat)
            .map_err(|_| Error::StaleDecision)?;
        let public = session
            .public_observation(expected)
            .map_err(|_| Error::StaleDecision)?;
        if let Some(pending) = &self.pending {
            if pending.arahbo == session.arahbo
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
        let visible = HumanDecisionArahboV31 {
            schema: "mtg-kernel-human-arahbo-decision/v31".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundArahboV31 {
            arahbo: session.arahbo.clone(),
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
        session: &mut HumanArahboSessionV31,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, Error> {
        if let Some((receipt, arahbo)) = &self.accepted {
            if receipt.prompt_seq == request.prompt_seq {
                if *arahbo != session.arahbo {
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
        if pending.arahbo != session.arahbo || request.prompt_seq != pending.visible.prompt_seq {
            return Err(Error::StaleDecision);
        }
        let index = *pending
            .engine_indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        session
            .inner
            .arahbo_validate_bound_decision_v31(pending.expected, self.human_seat, &pending.binding)
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
        self.accepted = Some((receipt.clone(), session.arahbo.clone()));
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests;
