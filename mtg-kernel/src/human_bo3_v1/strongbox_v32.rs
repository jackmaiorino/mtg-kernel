//! Sealed Strongbox successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::rl::ActionSemanticV1;
use crate::rl_session::strongbox_v32::StrongboxDecisionBindingV32;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use crate::strongbox_public_v1::StrongboxObservationV1;
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV32 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV32 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV32 {}
pub struct HumanStrongboxSessionV32 {
    inner: FastActorSessionV1,
    authority: AuthorityV32,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanStrongboxSnapshotV32 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanStrongboxSessionV32 {
    pub fn from_session(
        session: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_strongbox_v32()?,
            human_seat,
            authority: AuthorityV32 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v32(&self) -> HumanStrongboxSnapshotV32 {
        HumanStrongboxSnapshotV32 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v32(&mut self, snapshot: &HumanStrongboxSnapshotV32) -> Result<(), Error> {
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
    ) -> Result<crate::strongbox_public_v1::StrongboxObservationV1, crate::rl::RlContractError>
    {
        if expected.acting_player != self.human_seat {
            return Err(crate::rl::RlContractError(
                "Strongbox Human view is fixed to its selected seat".into(),
            ));
        }
        self.inner
            .strongbox_current_decision_input_v32(expected, self.human_seat)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Strongbox public decision".into()))
    }
    /// Host-only policy callback. Opponent private rosters never enter a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, crate::rl::RlContractError>
    where
        F: FnMut(
            &StrongboxObservationV1,
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
            .strongbox_current_decision_input_v32(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale opponent input".into()))?;
        let index = policy(&observation, &actions)?;
        if index as usize >= actions.len() {
            return Err(crate::rl::RlContractError(
                "opponent action out of range".into(),
            ));
        }
        self.inner
            .strongbox_validate_bound_decision_v32(expected, expected.acting_player, &binding)
            .map_err(|_| crate::rl::RlContractError("opponent input changed".into()))?;
        self.inner
            .step(expected.episode_id, expected.step, index)
            .map_err(|error| crate::rl::RlContractError(format!("opponent execution:{error:?}")))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionStrongboxV32 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanStrongboxExtensionsV32>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanStrongboxExtensionsV32 {
    pub predecessor: HumanExtensionsV9,
    pub entries: Vec<HumanStrongboxEntryV32>,
    pub selection_menu: Option<HumanStrongboxSelectionMenuV32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanStrongboxEntryV32 {
    pub entry_order: u32,
    pub source_at_entry: HumanCardRefV1,
    pub captured_controller: PlayerSeatV1,
    pub pending_trigger_ordinal: Option<u32>,
    pub live_stack_index: Option<u32>,
    pub phase: String,
    pub exiled: Vec<HumanCardRefV1>,
    pub selected: Option<HumanCardRefV1>,
    pub permission: Option<HumanStrongboxPermissionV32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanStrongboxPermissionV32 {
    pub card: HumanCardRefV1,
    pub holder: PlayerSeatV1,
    pub play_land: bool,
    pub cast_spell: bool,
    pub next_holder_turn_started: bool,
    pub expired: bool,
    pub exact_current_exile: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanStrongboxSelectionMenuV32 {
    pub source_at_entry: HumanCardRefV1,
    pub chooser: PlayerSeatV1,
    pub minimum: u16,
    pub maximum: u16,
    pub can_finish: bool,
    pub legal: Vec<HumanCardRefV1>,
    pub selected: Vec<HumanCardRefV1>,
}
fn menu(
    observation: &StrongboxObservationV1,
    handles: &Handles,
) -> Result<Option<HumanStrongboxSelectionMenuV32>, Error> {
    observation
        .selection_menu
        .as_ref()
        .map(|menu| {
            Ok(HumanStrongboxSelectionMenuV32 {
                source_at_entry: handles.get(&menu.source_at_entry)?.clone(),
                chooser: menu.chooser,
                minimum: menu.minimum,
                maximum: menu.maximum,
                can_finish: menu.can_finish,
                legal: menu
                    .legal
                    .iter()
                    .map(|r| handles.get(r).cloned())
                    .collect::<Result<Vec<_>, Error>>()?,
                selected: menu
                    .selected
                    .iter()
                    .map(|r| handles.get(r).cloned())
                    .collect::<Result<Vec<_>, Error>>()?,
            })
        })
        .transpose()
}
fn parts(observation: &StrongboxObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
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
    observation: &StrongboxObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanStrongboxExtensionsV32>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let entries = observation
        .entries
        .iter()
        .map(|row| {
            Ok(HumanStrongboxEntryV32 {
                entry_order: row.entry_order,
                source_at_entry: handles.get(&row.source_at_entry)?.clone(),
                captured_controller: row.captured_controller,
                pending_trigger_ordinal: row.pending_trigger_ordinal,
                live_stack_index: row.live_stack_index,
                phase: row.phase.clone(),
                exiled: row
                    .exiled
                    .iter()
                    .map(|r| handles.get(r).cloned())
                    .collect::<Result<Vec<_>, Error>>()?,
                selected: row
                    .selected
                    .as_ref()
                    .map(|r| handles.get(r).cloned())
                    .transpose()?,
                permission: row
                    .ordinary_permission
                    .as_ref()
                    .map(|p| {
                        Ok::<_, Error>(HumanStrongboxPermissionV32 {
                            card: handles.get(&p.card)?.clone(),
                            holder: p.holder,
                            play_land: p.play_land,
                            cast_spell: p.cast_spell,
                            next_holder_turn_started: p.next_holder_turn_started,
                            expired: p.expired,
                            exact_current_exile: p.exact_current_exile,
                        })
                    })
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanStrongboxExtensionsV32 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            entries,
            selection_menu: menu(observation, handles)?,
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
type ProjectedStrongboxV32 = (
    HumanVisibleStateV1<HumanStrongboxExtensionsV32>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

// V40 uses the same typed safe fields; no predecessor artifact/input admission.
pub(super) fn register_for_akroma_parts(
    observation: &crate::strongbox_public_v1::StrongboxObservationV1,
    handles: &mut Handles,
) -> Result<(), Error> {
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &observation.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    for row in &observation.entries {
        handles.insert(&row.source_at_entry)?;
        for card in &row.exiled {
            handles.insert(card)?;
        }
        if let Some(card) = &row.selected {
            handles.insert(card)?;
        }
        if let Some(permission) = &row.ordinary_permission {
            handles.insert(&permission.card)?;
        }
    }
    if let Some(menu) = &observation.selection_menu {
        handles.insert(&menu.source_at_entry)?;
        for card in menu.legal.iter().chain(&menu.selected) {
            handles.insert(card)?;
        }
    }
    Ok(())
}
pub(super) fn state_for_akroma_parts(
    observation: &crate::strongbox_public_v1::StrongboxObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanStrongboxExtensionsV32, Error> {
    Ok(state(observation, handles, human)?.extensions)
}
pub(super) fn label_for_akroma_parts(
    action: &ActionSemanticV1,
    observation: &crate::strongbox_public_v1::StrongboxObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    super::labels::label_strongbox_v32(action, observation, handles, human)
}

pub(super) fn unary_for_akroma_parts(
    extension: &HumanStrongboxExtensionsV32,
    unary: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for row in &extension.predecessor.predecessor.ward_life_card_traits {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &extension.entries {
        unary
            .entry(row.source_at_entry.handle.clone())
            .or_default()
            .push(format!(
                "strongbox_entry:{}",
                visible::without_handles(row)?
            ));
        for (index, card) in row.exiled.iter().enumerate() {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push(format!("strongbox_exile:{}:{index}", row.entry_order));
        }
        if let Some(card) = &row.selected {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push(format!("strongbox_selected:{}", row.entry_order));
        }
        if let Some(permission) = &row.permission {
            unary
                .entry(permission.card.handle.clone())
                .or_default()
                .push(format!(
                    "strongbox_permission:{}",
                    visible::without_handles(permission)?
                ));
        }
    }
    if let Some(menu) = &extension.selection_menu {
        unary
            .entry(menu.source_at_entry.handle.clone())
            .or_default()
            .push(format!(
                "strongbox_chooser:{}",
                visible::without_handles(menu)?
            ));
        for card in &menu.legal {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push("strongbox_legal_exile".into());
        }
    }
    Ok(())
}
fn project_base(
    observation: &StrongboxObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedStrongboxV32, Error> {
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
    for row in &observation.entries {
        handles.insert(&row.source_at_entry)?;
        for card in &row.exiled {
            handles.insert(card)?;
        }
        if let Some(card) = &row.selected {
            handles.insert(card)?;
        }
        if let Some(permission) = &row.ordinary_permission {
            handles.insert(&permission.card)?;
        }
    }
    if let Some(menu) = &observation.selection_menu {
        handles.insert(&menu.source_at_entry)?;
        for card in menu.legal.iter().chain(&menu.selected) {
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
    for row in &initial.extensions.entries {
        unary
            .entry(row.source_at_entry.handle.clone())
            .or_default()
            .push(format!(
                "strongbox_entry:{}",
                visible::without_handles(row)?
            ));
        for (index, card) in row.exiled.iter().enumerate() {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push(format!("strongbox_exile:{}:{index}", row.entry_order));
        }
        if let Some(card) = &row.selected {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push(format!("strongbox_selected:{}", row.entry_order));
        }
        if let Some(permission) = &row.permission {
            unary
                .entry(permission.card.handle.clone())
                .or_default()
                .push(format!(
                    "strongbox_permission:{}",
                    visible::without_handles(permission)?
                ));
        }
    }
    if let Some(menu) = &initial.extensions.selection_menu {
        unary
            .entry(menu.source_at_entry.handle.clone())
            .or_default()
            .push(format!(
                "strongbox_chooser:{}",
                visible::without_handles(menu)?
            ));
        for card in &menu.legal {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push("strongbox_legal_exile".into());
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
                super::labels::label_strongbox_v32(action, observation, &handles, human)?,
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

struct BoundStrongboxV32 {
    authority: AuthorityV32,
    expected: FastActorDecisionV1,
    binding: StrongboxDecisionBindingV32,
    public: crate::strongbox_public_v1::StrongboxObservationV1,
    visible: HumanDecisionStrongboxV32,
    engine_indexes: Vec<u32>,
}
pub struct HumanStrongboxProjectorV32 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundStrongboxV32>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV32)>,
}
impl HumanStrongboxProjectorV32 {
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
        session: &HumanStrongboxSessionV32,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionStrongboxV32, Error> {
        if expected.acting_player != self.human_seat || self.human_seat != session.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .strongbox_current_decision_input_v32(expected, self.human_seat)
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
        let visible = HumanDecisionStrongboxV32 {
            schema: "mtg-kernel-human-strongbox-decision/v32".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundStrongboxV32 {
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
        session: &mut HumanStrongboxSessionV32,
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
            .strongbox_validate_bound_decision_v32(
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
